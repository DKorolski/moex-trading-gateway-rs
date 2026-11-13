#!/usr/bin/env python3
"""Exact R0 repro by default; optional explicit R1 integrity regression mode.

Only owned fixture copies and SQLite files are written. Refreshing the manifest
implementation fingerprint requires --refresh-implementation; no model, binding,
candle, clock or numerical evidence value is silently changed.
"""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import sqlite3
import subprocess

p = argparse.ArgumentParser()
p.add_argument('--binary', type=Path, required=True)
p.add_argument('--package', type=Path, required=True)
p.add_argument('--output', type=Path, required=True)
p.add_argument('--expect-fixed', action='store_true',
               help='Require mutant reopen/export rejection and a valid control next commit.')
p.add_argument('--refresh-implementation', action='store_true',
               help='Explicitly replace only the owned manifest implementation fingerprint with the current CLI fingerprint.')
a = p.parse_args()
a.output.mkdir(parents=True, exist_ok=False)

def run(*args):
    r = subprocess.run([str(a.binary), *map(str, args)], capture_output=True, text=True, timeout=120)
    return {'returncode': r.returncode, 'stdout': r.stdout, 'stderr': r.stderr}

evidence = a.package / 'evidence' / 'local-integration'
supplied_manifest = evidence / 'manifest.json'
original_manifest_bytes = supplied_manifest.read_bytes()
original_manifest = json.loads(original_manifest_bytes)
original_implementation = original_manifest['implementation_sha256']
reported = run('implementation')
assert reported['returncode'] == 0, reported
current_implementation = reported['stdout'].strip()
assert len(current_implementation) == 64 and all(c in '0123456789abcdef' for c in current_implementation), reported
selected_manifest = dict(original_manifest)
if a.refresh_implementation:
    selected_manifest['implementation_sha256'] = current_implementation
    selected_manifest_bytes = json.dumps(selected_manifest, separators=(',', ':')).encode()
else:
    selected_manifest_bytes = original_manifest_bytes
assert {k: v for k, v in selected_manifest.items() if k != 'implementation_sha256'} == {k: v for k, v in original_manifest.items() if k != 'implementation_sha256'}
manifest = a.output / 'manifest.json'
(a.output / 'manifest-original.json').write_bytes(original_manifest_bytes)
manifest.write_bytes(selected_manifest_bytes)
manifest_record = {
    'supplied_manifest': str(supplied_manifest),
    'original_manifest_file_sha256': hashlib.sha256(original_manifest_bytes).hexdigest(),
    'selected_manifest_file_sha256': hashlib.sha256(selected_manifest_bytes).hexdigest(),
    'original_implementation_sha256': original_implementation,
    'selected_implementation_sha256': selected_manifest['implementation_sha256'],
    'binary_implementation_sha256': current_implementation,
    'refresh_explicitly_requested': a.refresh_implementation,
    'implementation_fingerprint_changed': selected_manifest['implementation_sha256'] != original_implementation,
    'other_manifest_values_unchanged': True,
    'supplied_manifest_preserved': True,
}
(a.output / 'manifest-selection.json').write_text(json.dumps(manifest_record, indent=2) + '\n')
events = json.loads((evidence / 'candles.json').read_text())
first = events[0]
second = next(c for c in events if c['instrument'] == first['instrument'] and c['source_sequence'] == first['source_sequence'] + 1)
one = a.output / 'first.json'
two = a.output / 'second.json'
one.write_text(json.dumps(first, separators=(',', ':')))
two.write_text(json.dumps(second, separators=(',', ':')))

base = a.output / 'baseline.sqlite'
control_create = run('create', manifest, base)
assert control_create['returncode'] == 0, ('positive-control create failed; do not treat this as mutant rejection', control_create, manifest_record)
created = run('apply', manifest, base, one)
assert created['returncode'] == 0, created
baseline_export = run('export', manifest, base)
assert baseline_export['returncode'] == 0, baseline_export
mutant = a.output / 'outcome-sequence-mutant.sqlite'
shutil.copyfile(base, mutant)
with sqlite3.connect(mutant) as db:
    untouched_before = {t: db.execute(f'SELECT * FROM {t} ORDER BY 1').fetchall() for t in ('meta', 'inputs', 'arms')}
    before = db.execute('SELECT arm,source_sequence,revision,result,result_sha FROM outcomes ORDER BY arm').fetchall()
    arm = before[0][0]
    assert before[0][1] == first['source_sequence']
    db.execute('UPDATE outcomes SET source_sequence=? WHERE arm=? AND source_sequence=?',
               (second['source_sequence'], arm, first['source_sequence']))
    after = db.execute('SELECT arm,source_sequence,revision,result,result_sha FROM outcomes ORDER BY arm').fetchall()
    assert after[0][0] == before[0][0]
    assert after[0][2:] == before[0][2:]
    assert after[1:] == before[1:]
    untouched_after = {t: db.execute(f'SELECT * FROM {t} ORDER BY 1').fetchall() for t in ('meta', 'inputs', 'arms')}
    assert untouched_after == untouched_before
    checks = {'quick_check': db.execute('PRAGMA quick_check').fetchall(),
              'integrity_check': db.execute('PRAGMA integrity_check').fetchall(),
              'foreign_key_check': db.execute('PRAGMA foreign_key_check').fetchall()}
mutant_export = run('export', manifest, mutant)
baseline_apply = run('apply', manifest, base, two)
baseline_after = run('export', manifest, base)
assert baseline_apply['returncode'] == 0, ('positive-control next apply failed', baseline_apply)
assert baseline_after['returncode'] == 0, ('positive-control final export failed', baseline_after)
assert json.loads(baseline_after['stdout'])['revision'] == 2
mutant_apply = None
mutant_after = None
if a.expect_fixed:
    assert mutant_export['returncode'] == 1, ('expected a normal validation rejection during mutant reopen/export', mutant_export)
    diagnostic = mutant_export['stderr'].lower()
    assert 'local_session_error:' in diagnostic, ('missing CLI validation diagnostic', mutant_export)
    assert 'manifest' not in diagnostic and 'implementation' not in diagnostic, ('unrelated manifest/implementation rejection is not an integrity regression PASS', mutant_export, manifest_record)
    expectation = 'FIX_VERIFIED_MUTANT_REOPEN_REJECTED_CONTROL_PASSED'
else:
    assert mutant_export['returncode'] == 0, ('R0 defect not reproduced: mutant reopen/export rejected', mutant_export)
    assert mutant_export['stdout'] == baseline_export['stdout'], 'R0 mutant export differs from positive control'
    mutant_apply = run('apply', manifest, mutant, two)
    mutant_after = run('export', manifest, mutant)
    assert mutant_apply['returncode'] == 1 and 'UNIQUE constraint failed: outcomes.arm, outcomes.source_sequence' in mutant_apply['stderr'], mutant_apply
    assert mutant_after['returncode'] == 0 and mutant_after['stdout'] == mutant_export['stdout'], mutant_after
    assert json.loads(mutant_after['stdout'])['revision'] == 1
    expectation = 'R0_DEFECT_REPRODUCED_CONTROL_PASSED'
summary = {
    'expectation': expectation,
    'expect_fixed': a.expect_fixed,
    'manifest_selection': manifest_record,
    'binary_sha256': hashlib.sha256(a.binary.read_bytes()).hexdigest(),
    'instrument': first['instrument'], 'mutated_arm': arm,
    'sql_sequence_before': first['source_sequence'],
    'sql_sequence_after': second['source_sequence'],
    'changed_only_one_sql_index_value': True,
    'result_json_result_sha_input_checkpoint_and_chain_unchanged': True,
    'outcome_payload_and_hash_digest_before': hashlib.sha256(json.dumps([r[2:] for r in before], separators=(',', ':')).encode()).hexdigest(),
    'outcome_payload_and_hash_digest_after': hashlib.sha256(json.dumps([r[2:] for r in after], separators=(',', ':')).encode()).hexdigest(),
    'meta_inputs_arms_byte_values_unchanged': untouched_after == untouched_before,
    'sqlite_checks': checks,
    'mutant_open_export_returncode': mutant_export['returncode'],
    'mutant_open_export_stderr': mutant_export['stderr'],
    'mutant_export_equals_baseline': mutant_export['stdout'] == baseline_export['stdout'],
    'next_valid_input_mutant': {'returncode': mutant_apply['returncode'], 'stderr': mutant_apply['stderr']} if mutant_apply else {'not_run': 'mutant already rejected at reopen/export in expect-fixed mode'},
    'next_valid_input_baseline': {'returncode': baseline_apply['returncode'], 'stderr': baseline_apply['stderr']},
    'positive_control_create_apply_export_and_next_apply_passed': True,
    'mutant_revision_after_failed_apply': json.loads(mutant_after['stdout'])['revision'] if mutant_after and mutant_after['returncode'] == 0 else None,
    'baseline_revision_after_next_apply': json.loads(baseline_after['stdout'])['revision'] if baseline_after['returncode'] == 0 else None,
    'mutant_failed_apply_did_not_change_export': mutant_after['stdout'] == mutant_export['stdout'] if mutant_after else None,
}
(a.output / 'summary.json').write_text(json.dumps(summary, indent=2) + '\n')
print(json.dumps(summary, indent=2))
