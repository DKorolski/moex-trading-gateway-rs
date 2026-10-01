#!/usr/bin/env python3
"""P1-NRG01 test-only qualification with process-group deadlines and raw logs.

No VPS/FINAM/operational Redis access. Tests provision their own loopback Redis.
No retries: three sequential exact witnesses are required, every result retained.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[1]
CASE = 'stage8b_p1e_process::tests::production_heartbeat_turns_draining_while_ready_poll_response_is_withheld'


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def inventory():
    names = subprocess.check_output(
        ['git', 'ls-files', '--cached', '--others', '--exclude-standard', '-z'], cwd=ROOT
    ).decode().split('\0')
    return {name: sha((ROOT / name).read_bytes()) for name in sorted(set(names)) if name}


def signal_group(pid, value):
    try:
        os.killpg(pid, value)
        return True
    except ProcessLookupError:
        return False


def run(command, directory, seconds):
    directory.mkdir()
    children = directory / 'children'
    children.mkdir(mode=0o700)
    env = dict(os.environ, RUST_MIN_STACK='33554432', CARGO_NET_OFFLINE='true',
               CARGO_TERM_COLOR='never', STAGE8B_PROCESS_TEST_EVIDENCE_DIR=str(children))
    for key in list(env):
        if 'REDIS' in key:
            env.pop(key)
    started = time.monotonic()
    expired = False
    child = None
    cleanup_error = None
    with (directory / 'output.txt').open('wb') as log:
        try:
            child = subprocess.Popen(command, cwd=ROOT, env=env, stdout=log,
                                     stderr=subprocess.STDOUT, start_new_session=True)
            try:
                code = child.wait(timeout=seconds)
            except subprocess.TimeoutExpired:
                expired = True
                code = 124
                log.write(b'\nNRG01 EXTERNAL DEADLINE EXCEEDED; NOT PASS\n')
                log.flush()
        except (OSError, KeyboardInterrupt) as error:
            code = 130 if isinstance(error, KeyboardInterrupt) else 125
            log.write(('\nNRG01 runner interrupted: ' + repr(error) + '\n').encode())
            log.flush()
        finally:
            # This is a fresh private process group, not a name-based kill and
            # never the developer's/persistent Redis process group.
            if child is not None:
                try:
                    if signal_group(child.pid, signal.SIGTERM):
                        time.sleep(0.5)
                    # Reap the leader before killpg (macOS zombie-only groups).
                    child.poll()
                    signal_group(child.pid, signal.SIGKILL)
                except OSError as error:
                    cleanup_error = repr(error)
                    log.write(('\nNRG01 group cleanup failed: ' + cleanup_error + '\n').encode())
                    code = code or 125
                    child.kill()
                child.wait()
    artifacts = {p.relative_to(directory).as_posix(): sha(p.read_bytes())
                 for p in sorted(directory.rglob('*')) if p.is_file()}
    return {'command': command, 'exit_code': code, 'deadline_exceeded': expired,
            'cleanup_error': cleanup_error,
            'deadline_seconds': seconds, 'elapsed_seconds': round(time.monotonic()-started, 3),
            'directory': directory.name, 'artifacts': artifacts}


def commands(phase):
    base = ['cargo', 'test', '-p', 'runtime-durable-service', '--lib']
    exact = base + ['--all-features', CASE, '--', '--exact', '--test-threads=1', '--nocapture']
    result = [
        (['cargo', 'fmt', '--all', '--', '--check'], 120),
        (['cargo', 'clippy', '--workspace', '--all-targets', '--all-features', '--', '-D', 'warnings'], 1200),
        (base + ['--all-features', 'nrg01_', '--', '--test-threads=1', '--nocapture'], 300),
    ]
    result += [(exact, 180)] * 3
    result += [(base + ['--all-features', 'production_run_cancels_server_processed_redis_attach_and_s06_requests',
                       '--', '--test-threads=1', '--nocapture'], 180)]
    if phase == 'qualification':
        result += [
            (base + ['stage8b_p1e_process::tests', '--', '--test-threads=1', '--nocapture'], 1200),
            # Existing full unfiltered suite; nocapture changes diagnostics only.
            # Prior complete/default run was ~23 min, interrupted full run ~35 min.
            (base + ['--all-features', '--', '--test-threads=1', '--nocapture'], 5400),
            (['python3', 'scripts/current_tree_authority_check.py'], 60),
            (['python3', 'scripts/current_tree_authority_negative_harness.py'], 300),
            (['git', 'diff', '--check'], 60),
        ]
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', required=True, type=Path)
    parser.add_argument('--phase', choices=['targeted', 'qualification'], default='targeted')
    args = parser.parse_args()
    output = args.output.resolve()
    if output.exists():
        raise SystemExit('Refusing to overwrite retained evidence')
    output.mkdir(parents=True)
    before = inventory()
    ref = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT).decode().strip()
    if args.phase == 'qualification' and subprocess.check_output(['git', 'status', '--porcelain'], cwd=ROOT).strip():
        raise SystemExit('Qualification requires a clean committed tree')
    # Bounded deadline negative control: supervisor reaps its own child on TERM.
    control = """import os,signal,subprocess,sys,time
child=subprocess.Popen([sys.executable,'-c','import time; time.sleep(60)'])
def stop(*_):
 child.kill(); child.wait(); print('watchdog-control child-reaped',flush=True); sys.exit(0)
signal.signal(signal.SIGTERM,stop)
print('watchdog-control ready',flush=True)
time.sleep(60)
"""
    watchdog = run([sys.executable, '-c', control], output/'watchdog-negative', 2)
    control_log = (output/'watchdog-negative/output.txt').read_text()
    watchdog_ok = (watchdog['deadline_exceeded'] is True and watchdog['exit_code'] == 124
                   and watchdog['cleanup_error'] is None
                   and 'watchdog-control child-reaped' in control_log)
    records = []
    planned = commands(args.phase)
    for index, (command, limit) in enumerate(planned):
        if not watchdog_ok:
            break
        print('RUN ' + ' '.join(command), flush=True)
        record = run(command, output / '{:02d}'.format(index), limit)
        records.append(record)
        print(('PASS ' if record['exit_code'] == 0 else 'FAIL ') + record['directory'], flush=True)
        if record['exit_code']:
            print((output/record['directory']/'output.txt').read_text(errors='replace')[-7000:], flush=True)
            break
    unchanged = before == inventory()
    passed = (unchanged and watchdog_ok and len(records) == len(planned)
              and all(r['exit_code'] == 0 and not r['deadline_exceeded'] for r in records))
    result = {'source_ref': ref, 'source_inventory': before, 'source_unchanged_during_gate': unchanged,
              'phase': args.phase, 'commands': records, 'planned_commands': len(planned),
              'watchdog_negative': watchdog, 'watchdog_negative_passed': watchdog_ok,
              'gate_passed': passed, 'fresh_github_ci_claimed': False, 'operational_activation': False}
    (output/'result.json').write_text(json.dumps(result, indent=2, sort_keys=True)+'\n')
    print('NRG01_' + args.phase.upper() + '=' + ('PASS' if passed else 'FAIL'), flush=True)
    return 0 if passed else 1


if __name__ == '__main__':
    sys.exit(main())
