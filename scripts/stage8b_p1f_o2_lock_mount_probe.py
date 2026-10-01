#!/usr/bin/env python3
"""Disposable-container-only materializer mount proof; no broker/authority inputs."""
import errno
import fcntl
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys

CONTROL = Path('/var/lib/moex-finam-p1-paper-control')
CONFIG = Path('/etc/moex-finam-p1-paper')
STAGING = Path('/var/lib/moex-finam-p1-paper-o2-staging')
UNIT = 'moex-finam-p1f-o2-materializer.service'

def command(*args):
    return subprocess.check_output(args, text=True, timeout=30)

def probe(case):
    observations = {}
    for name in ('.execution.lock', '.guardian.lock'):
        try:
            fd = os.open(CONTROL / name, os.O_RDWR | os.O_NOFOLLOW | os.O_CLOEXEC)
        except OSError as e:
            assert e.errno == errno.EROFS, (name, e.errno)
            observations[name] = {'opened': False, 'errno': e.errno}
        else:
            try:
                fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
                s = os.fstat(fd)
                observations[name] = {'opened': True, 'dev': s.st_dev, 'ino': s.st_ino, 'nlink': s.st_nlink}
            finally:
                os.close(fd)
    for path in (CONTROL / 'authority/sentinel', CONFIG / 'o2/source-template.json'):
        try:
            fd = os.open(path, os.O_WRONLY)
        except OSError as e:
            assert e.errno == errno.EROFS, (str(path), e.errno)
        else:
            os.close(fd)
            raise AssertionError('unexpected authority/config write access')
    status = Path('/proc/self/status').read_text()
    assert 'CapEff:\t0000000000000000' in status and 'NoNewPrivs:\t1' in status
    result = {'case': case, 'lock_inodes': observations, 'authority_config_readonly': True,
              'effective_capabilities_zero': True, 'no_new_privileges': True}
    (STAGING / (case + '.json')).write_text(json.dumps(result, sort_keys=True))

def run():
    assert Path('/run/o2-disposable-fixture').read_text() == 'no-host-mounts-no-credentials\n'
    original = Path('/input/materializer.service').read_bytes()
    destination = Path('/etc/systemd/system') / UNIT
    destination.write_bytes(original)
    for path in (CONTROL / 'authority', CONFIG / 'o2', STAGING, Path('/run/moex-finam-p1f-o2-input')):
        path.mkdir(parents=True, exist_ok=True)
    for name in ('.execution.lock', '.guardian.lock'):
        p = CONTROL / name
        p.write_bytes(b'')
        p.chmod(0o600)
    for p in (CONTROL / 'authority/sentinel', CONFIG / 'o2/source-template.json',
              CONFIG / 'o2/materialization-policy.json', CONFIG / 'o2/active-manifest.sha256'):
        p.write_bytes(b'fixture-only-not-authority\n')
        p.chmod(0o440)
    # Owner-write allowed outside the sandbox, so EROFS proves the mount
    # restriction rather than a DAC denial caused by CapabilityBoundingSet=.
    (CONTROL / 'authority/sentinel').chmod(0o600)
    (CONFIG / 'o2/source-template.json').chmod(0o600)
    for name in ('finam-readonly.token', 'finam-account.id'):
        p = Path('/run/moex-finam-p1f-o2-input') / name
        p.write_bytes(b'not-a-real-credential\n')
        p.chmod(0o600)
    dropin = Path('/etc/systemd/system') / (UNIT + '.d/probe.conf')
    dropin.parent.mkdir()
    results = []
    for case, omit in [('positive', None), ('without-execution-lock', '.execution.lock'), ('without-guardian-lock', '.guardian.lock')]:
        text = '[Service]\nExecStart=\nExecStart=/usr/bin/python3 /opt/lock-probe.py probe ' + case + '\n'
        text += f'StandardOutput=append:{STAGING}/{case}.log\nStandardError=append:{STAGING}/{case}.log\n'
        if omit:
            keep = '.guardian.lock' if omit == '.execution.lock' else '.execution.lock'
            text += 'ReadWritePaths=\nReadWritePaths=' + str(STAGING) + ' ' + str(CONTROL / keep) + '\n'
        dropin.write_text(text)
        command('systemctl', 'daemon-reload')
        try:
            command('systemctl', 'start', UNIT)
        except subprocess.CalledProcessError:
            print(command('systemctl', 'show', UNIT, '--property=Result,ExecMainCode,ExecMainStatus'), flush=True)
            log = STAGING / (case + '.log')
            print(log.read_text() if log.exists() else 'no service log', flush=True)
            raise
        result = json.loads((STAGING / (case + '.json')).read_text())
        for name, observed in result['lock_inodes'].items():
            assert observed['opened'] == (name != omit)
            if observed['opened']:
                s = (CONTROL / name).stat()
                assert (s.st_dev, s.st_ino, s.st_nlink) == (observed['dev'], observed['ino'], observed['nlink'])
        properties = command('systemctl', 'show', UNIT, '--property=Result,ExecMainStatus,User,Group,ProtectSystem,NoNewPrivileges,CapabilityBoundingSet,ReadWritePaths,ReadOnlyPaths,PrivateTmp,PrivateDevices,ProtectControlGroups,ProtectHome,RestrictAddressFamilies')
        assert 'Result=success\n' in properties and 'ExecMainStatus=0\n' in properties
        assert 'ProtectSystem=strict\n' in properties and 'CapabilityBoundingSet=\n' in properties
        assert destination.read_bytes() == original
        results.append({'probe': result, 'dropin': text, 'systemd_properties': properties})
    print(json.dumps({'result': 'PASS', 'unit_sha256': hashlib.sha256(original).hexdigest(),
                      'systemd_version': command('systemctl', '--version'), 'cases': results,
                      'scope': 'exact materializer mount/hardening, ExecStart replaced by fixture probe',
                      'production_materialization_executed': False, 'finam_contact': False}, indent=2))

if __name__ == '__main__':
    if sys.argv[1:] == ['run']:
        run()
    elif len(sys.argv) == 3 and sys.argv[1] == 'probe':
        probe(sys.argv[2])
    else:
        raise SystemExit('fixture only: run | probe CASE')
