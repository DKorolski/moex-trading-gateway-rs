#!/usr/bin/env python3
"""Bounded, exact-archive postseal gate. Does not start Docker, Redis or FINAM."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import sys

import stage8b_p1f_nrg01_test_gate as bounded


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('archive', type=Path)
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    archive, output = args.archive.resolve(), args.output.resolve()
    if subprocess.check_output(['git', 'status', '--porcelain'], cwd=bounded.ROOT).strip():
        raise SystemExit('Gate requires a clean committed packaging tree')
    output.mkdir(parents=True, exist_ok=False)
    ref = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=bounded.ROOT).decode().strip()
    before = bounded.inventory()
    digest = hashlib.sha256(archive.read_bytes()).hexdigest()
    checks = [
        ([sys.executable, '-m', 'py_compile', 'scripts/stage8b_p1f_o2_no_riskgate_build.py',
          'scripts/stage8b_p1f_o2_no_riskgate_artifact.py', 'scripts/stage8b_p1f_o2_no_riskgate_gate.py',
          'scripts/test_stage8b_p1f_o2_no_riskgate_artifact.py'], 30),
        (['bash', '-n', 'scripts/stage8b_p1f_o2_no_riskgate_smoke.sh'], 30),
        (['rustfmt', '--edition', '2021', '--check', 'scripts/fixtures/stage8b-o2-no-riskgate-profile-probe.rs'], 30),
        ([sys.executable, 'scripts/current_tree_authority_check.py'], 60),
        ([sys.executable, 'scripts/current_tree_authority_negative_harness.py'], 300),
        ([sys.executable, 'scripts/stage8b_p1f_o2_no_riskgate_artifact.py', 'check', str(archive)], 120),
        ([sys.executable, 'scripts/test_stage8b_p1f_o2_no_riskgate_artifact.py', str(archive)], 300),
        (['git', 'diff', '--exit-code', 'ca1e5da7ea41eec219bce1cfe2bdf4b8d63d9029', 'HEAD', '--',
          'crates', 'Cargo.toml', 'Cargo.lock', '.github'], 60),
        (['git', 'diff', '--check'], 30),
    ]
    records = []
    for index, (command, seconds) in enumerate(checks):
        print('RUN ' + ' '.join(command), flush=True)
        record = bounded.run(command, output / f'{index:02d}', seconds)
        records.append(record)
        print(('PASS ' if record['exit_code'] == 0 else 'FAIL ') + record['directory'], flush=True)
        if record['exit_code']:
            break
    unchanged = before == bounded.inventory() and digest == hashlib.sha256(archive.read_bytes()).hexdigest()
    passed = unchanged and len(records) == len(checks) and all(r['exit_code'] == 0 for r in records)
    result = dict(source_ref=ref, archive_name=archive.name, archive_sha256=digest,
                  source_inventory=before, source_and_archive_unchanged=unchanged,
                  commands=records, planned_commands=len(checks), gate_passed=passed,
                  installation_authorized=False, execution_authorized=False)
    (output / 'result.json').write_text(json.dumps(result, indent=2, sort_keys=True) + '\n')
    print('NO_RISKGATE_ARTIFACT_GATE=' + ('PASS' if passed else 'FAIL'), flush=True)
    return 0 if passed else 1


if __name__ == '__main__':
    sys.exit(main())
