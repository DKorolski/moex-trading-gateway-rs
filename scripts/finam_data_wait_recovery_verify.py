#!/usr/bin/env python3
"""Offline V7 source qualification; no FINAM, VPS or credentials."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import time

ROOT = Path(__file__).resolve().parents[1]


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--output', required=True, type=Path)
    p.add_argument('--target-dir', required=True, type=Path)
    p.add_argument('--test-threads', type=int, default=2)
    a = p.parse_args()
    assert a.test_threads > 0
    out = a.output.resolve()
    out.mkdir(parents=True, exist_ok=False)
    env = dict(os.environ, RUST_TEST_THREADS=str(a.test_threads))
    common = ['--offline', '--locked', '-p', 'finam-signal-shadow', '--target-dir', str(a.target_dir.resolve())]
    commands = [
        ('native-default', ['cargo', 'test'] + common + ['--lib']),
        ('native-bridge', ['cargo', 'test'] + common + ['--features', 'models-bridge', '--lib']),
        ('doctests', ['cargo', 'test'] + common + ['--features', 'models-bridge', '--doc']),
        ('strict-clippy', ['cargo', 'clippy'] + common + ['--features', 'models-bridge', '--lib', '--tests', '--', '-D', 'warnings']),
        ('v7-repeat-1', ['cargo', 'test'] + common + ['--lib', 'data_wait']),
        ('v7-repeat-2', ['cargo', 'test'] + common + ['--lib', 'data_wait']),
    ]
    gates = []
    for name, cmd in commands:
        start = time.monotonic()
        r = subprocess.run(cmd, cwd=ROOT, env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, timeout=2400)
        (out/(name+'.txt')).write_text(r.stdout)
        result = dict(name=name, command=cmd, exit_code=r.returncode, elapsed_seconds=round(time.monotonic()-start, 3), test_summaries=re.findall(r'test result:.*', r.stdout))
        gates.append(result)
        (out/(name+'.json')).write_text(json.dumps(result, indent=2)+'\n')
        print(name, r.returncode, flush=True)
    summary = dict(schema='finam.ws-primary.data-wait-v7.source-gates.v1', result='PASS' if all(g['exit_code'] == 0 for g in gates) else 'FAIL', rustc=subprocess.check_output(['rustc', '--version'], text=True).strip(), test_threads=a.test_threads, gates=gates, vps_changed=False, linux_release=False, operational_activation=False, observed_sessions=0)
    (out/'summary.json').write_text(json.dumps(summary, indent=2)+'\n')
    (out/'SHA256SUMS').write_text(''.join(hashlib.sha256(f.read_bytes()).hexdigest()+'  '+f.name+'\n' for f in sorted(out.iterdir()) if f.is_file() and f.name != 'SHA256SUMS'))
    return 0 if summary['result'] == 'PASS' else 1


if __name__ == '__main__':
    raise SystemExit(main())
