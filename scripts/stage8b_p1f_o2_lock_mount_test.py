#!/usr/bin/env python3
"""Native systemd in an isolated local container; never a VPS/host-root mount."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import time
import uuid

ROOT = Path(__file__).resolve().parents[1]
IMAGE = 'sha256:9bec3cfe280a732eea281cb1add79f75523d1fe2ac86c12b129e4bc2509ec163'

def run(output):
    output.mkdir(parents=True, exist_ok=False)
    name = 'moex-o2-lock-test-' + uuid.uuid4().hex[:12]
    commands = []
    def cmd(*args):
        p = subprocess.run(args, capture_output=True, timeout=90)
        commands.append({'argv': list(args), 'exit_code': p.returncode,
                         'stdout': p.stdout.decode(errors='replace'), 'stderr': p.stderr.decode(errors='replace')})
        assert p.returncode == 0, commands[-1]
        return p.stdout
    created = False
    try:
        # CAP_SYS_ADMIN is required by the container's systemd to build mount
        # namespaces. No host mounts, cgroupns=private, no socket, no network.
        cmd('docker', 'run', '-d', '--name', name, '--privileged', '--cgroupns=private',
            '--network=none', '--tmpfs', '/run', '--tmpfs', '/run/lock', '--tmpfs', '/tmp',
            '--env', 'container=docker', '--platform', 'linux/arm64', IMAGE, '/bin/sh', '-c',
            'ln -s /dev/null /etc/systemd/system/systemd-binfmt.service && exec /lib/systemd/systemd')
        created = True
        inspection = json.loads(cmd('docker', 'inspect', name))[0]
        assert inspection['Mounts'] == [] and inspection['HostConfig']['NetworkMode'] == 'none'
        assert inspection['HostConfig']['CgroupnsMode'] == 'private' and inspection['Image'] == IMAGE
        assert cmd('docker', 'exec', name, 'readlink', '/etc/systemd/system/systemd-binfmt.service').strip() == b'/dev/null'
        # Bounded startup wait; only this container is queried or removed.
        for _ in range(30):
            p = subprocess.run(['docker', 'exec', name, 'systemctl', 'is-system-running'], capture_output=True, timeout=10)
            if p.stdout.strip() in (b'running', b'degraded'):
                break
            time.sleep(1)
        else:
            raise AssertionError('systemd did not start')
        cmd('docker', 'exec', name, 'mkdir', '-p', '/input', '/opt')
        cmd('docker', 'cp', str(ROOT / 'deploy/stage8b-p1e/moex-finam-p1f-o2-materializer.service'), name + ':/input/materializer.service')
        cmd('docker', 'cp', str(ROOT / 'scripts/stage8b_p1f_o2_lock_mount_probe.py'), name + ':/opt/lock-probe.py')
        cmd('docker', 'exec', name, 'python3', '-c',
            "from pathlib import Path; Path('/run/o2-disposable-fixture').write_text('no-host-mounts-no-credentials\\n')")
        raw = cmd('docker', 'exec', name, 'python3', '/opt/lock-probe.py', 'run')
        (output / 'result.json').write_bytes(raw)
        result = json.loads(raw)
        assert result['result'] == 'PASS' and len(result['cases']) == 3
        assert result['unit_sha256'] == hashlib.sha256((ROOT / 'deploy/stage8b-p1e/moex-finam-p1f-o2-materializer.service').read_bytes()).hexdigest()
        (output / 'container.json').write_text(json.dumps(inspection, indent=2) + '\n')
        print('PASS isolated-systemd-lock-mounts positive=1 negatives=2 production_execution=false')
    finally:
        if created:
            diagnostic = subprocess.run(['docker', 'exec', name, 'journalctl', '--no-pager',
                                         '-u', 'moex-finam-p1f-o2-materializer.service', '-n', '100'],
                                        capture_output=True, timeout=20)
            (output / 'journal.txt').write_bytes(diagnostic.stdout + diagnostic.stderr)
            cmd('docker', 'rm', '-f', name)
            # A privileged fixture must not break the Docker VM's emulator.
            cmd('docker', 'run', '--rm', '--network=none', '--platform=linux/amd64',
                'rust@sha256:82150a52ec202c1b14d7817e14516c392bb7f5cfebd88f1ed531cb37ebd39922', '/bin/true')
        (output / 'commands.json').write_text(json.dumps(commands, indent=2) + '\n')

if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('output', type=Path)
    run(parser.parse_args().output.resolve())
