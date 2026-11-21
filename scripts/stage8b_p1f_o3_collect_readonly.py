#!/usr/bin/env python3
"""Send read-only collector via stdin; no remote helper staging or service action."""
import argparse
import datetime as dt
import json
from pathlib import Path
import subprocess

from stage8b_p1f_o3_release import put, encoded
from stage8b_p1f_o2_build_linux import sha

MODULES = ('stage8b_p1e_i1_fixed_install', 'stage8b_p1f_o2_install',
           'stage8b_p1f_o2_terminal_install', 'stage8b_p1f_o2_replay_install',
           'stage8b_p1f_o3_install_inventory')
SSH = ['ssh', '-i', '/Users/denisq/.ssh/id_rsa_lightnode', '-o', 'BatchMode=yes',
    '-o', 'IdentitiesOnly=yes', '-o', 'StrictHostKeyChecking=yes', '-o', 'HostKeyAlgorithms=ssh-ed25519',
    '-o', 'ConnectTimeout=10', '-o', 'ServerAliveInterval=15', '-o', 'ServerAliveCountMax=3', 'root@45.150.11.252']


def collect(out):
    out.mkdir(parents=True, exist_ok=False)
    code = 'import sys,types,json\n'
    for n in MODULES:
        raw = (Path(__file__).parent/(n+'.py')).read_text()
        code += f'm=types.ModuleType({n!r});m.__file__="/readonly-input/scripts/{n}.py";sys.modules[{n!r}]=m\nexec({raw!r},m.__dict__)\n'
    code += 'print(json.dumps(m.collect(),sort_keys=True))\n'
    compile(code, '<readonly>', 'exec')
    put(out/'collector.py', code.encode())
    started = dt.datetime.now(dt.timezone.utc).isoformat()
    r = subprocess.run([*SSH, 'python3', '-B', '-'], input=code.encode(), capture_output=True, timeout=180)
    put(out/'stdout.json', r.stdout); put(out/'stderr.txt', r.stderr)
    put(out/'record.json', encoded(dict(command=[*SSH, 'python3', '-B', '-'], started_at_utc=started,
        finished_at_utc=dt.datetime.now(dt.timezone.utc).isoformat(), exit_code=r.returncode,
        stdin_sha256=sha(code.encode()), stdout_sha256=sha(r.stdout), stderr_sha256=sha(r.stderr))))
    if r.returncode:
        print(r.stderr.decode(errors='replace')[-3000:]); raise SystemExit(r.returncode)
    v = json.loads(r.stdout)
    print(json.dumps(dict(status=v['status'], head=v['expected']['history_head'],
        installed_slots=len(v['installed']), new_slots_absent=len(v['new_slots_absent']),
        mutation_performed=False), indent=2))


if __name__ == '__main__':
    p = argparse.ArgumentParser(description=__doc__); p.add_argument('output', type=Path)
    collect(p.parse_args().output.resolve())
