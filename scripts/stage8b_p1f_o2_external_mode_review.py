#!/usr/bin/env python3
"""Bounded source-review gate/ZIP. No deployment, authority rebind or live IO."""
import argparse
import io
import json
from pathlib import Path, PurePosixPath
import subprocess
import sys
import tarfile
import zipfile

import current_tree_authority_check as authority
import stage8b_p1f_nrg01_test_gate as bounded
import stage8b_p1f_o2_recovery_review as handoff

ROOT = Path(__file__).resolve().parents[1]
BASE = 'e05b4bfae3971459053ab22149aa88b2b5e9382c'
SOURCE = 'crates/runtime-durable-service/src/stage8b_p1f_guardian.rs'
ALLOWED = {SOURCE, 'docs/current-status.md', 'docs/roadmap.md',
           'docs/stage-8/stage8b-p1f-o2-external-mode-correction.md',
           'scripts/stage8b_p1f_o2_external_mode_review.py'}
handoff.BASE = BASE
EXACT = 'stage8b_p1f_guardian::tests::o2_external_writer_service_umask_matrix'
TEST = ['cargo', 'test', '--locked', '--offline', '-p', 'runtime-durable-service', '--all-features']
COMMANDS = [
    ('fmt', ['cargo', 'fmt', '--all', '--check'], 120),
    ('guardian-debug', TEST + ['--lib', 'stage8b_p1f_guardian::tests::', '--', '--test-threads=1', '--nocapture'], 900),
    ('systemd-debug', TEST + ['--lib', 'stage8b_p1f_o2_systemd::tests::', '--', '--test-threads=1'], 300),
    ('o2-release', TEST + ['--release', '--lib', 'stage8b_p1f_guardian::tests::o2_', '--', '--test-threads=1', '--nocapture'], 1200),
    ('doctests', TEST + ['--doc'], 300),
    ('clippy', ['cargo', 'clippy', '--locked', '--offline', '-p', 'runtime-durable-service', '--all-features', '--all-targets', '--', '-D', 'warnings'], 1200),
    ('diff', ['git', 'diff', '--check', BASE], 60),
]


def run(label, command, output, seconds):
    print('RUN ' + label + ': ' + ' '.join(command), flush=True)
    rec = bounded.run(command, output / label, seconds)
    print('EXIT ' + label + ': ' + str(rec['exit_code']), flush=True)
    return rec


def gate(output):
    ref = handoff.clean_ref()
    handoff.require(set(handoff.git('diff', '--name-only', BASE, ref).decode().splitlines()) == ALLOWED,
                    'unexpected source scope')
    pinned = json.loads((ROOT / authority.AUTHORITY).read_text())['production_code_manifest']['entries']
    actual = authority.file_inventory(ROOT, authority.production_files(ROOT))
    drift = {n for n in set(pinned) | set(actual) if pinned.get(n) != actual.get(n)}
    handoff.require(drift == {SOURCE}, 'unexpected authority drift')
    output.mkdir(parents=True, exist_ok=False)
    logs, records = {}, []
    for label, command, seconds in COMMANDS:
        record = run(label, command, output, seconds)
        records.append(record)
        handoff.require(record['exit_code'] == 0 and not record['deadline_exceeded'], 'gate failed: ' + label)
        raw = (output / label / 'output.txt').read_bytes()
        if label == 'guardian-debug':
            for name in ('o2_external_writer_service_umask_matrix',
                         'o2_external_writer_rejects_existing_bad_custody_and_conflicts',
                         'o2_external_writer_reopen_exact_temp_and_lost_response',
                         'o2_expired_pending_materialization_remains_closed_and_preserved'):
                handoff.require(name.encode() in raw, 'new test missing')
        if label in ('guardian-debug', 'o2-release'):
            handoff.require(b'PASS exact materialization/temp/receipt replay under umask 0077' in raw,
                            'service umask witness missing')
        if label in ('guardian-debug', 'systemd-debug', 'o2-release'):
            handoff.require(b'0 passed; 0 failed' not in raw and b'test result: ok.' in raw, 'empty test selection')
        logs[label + '/output.txt'] = handoff.digest(raw)

    # Disposable source fixture: same committed tests, remove only the new
    # descriptor-mode call. No file in the reviewed worktree is rewritten.
    negative = output / 'negative-tree'
    negative.mkdir()
    with tarfile.open(fileobj=io.BytesIO(handoff.git('archive', '--format=tar', ref))) as archive:
        for member in archive:
            name = PurePosixPath(member.name)
            handoff.require(not name.is_absolute() and '..' not in name.parts
                            and '\\' not in member.name and (member.isdir() or member.isfile()), 'unsafe source fixture')
            target = negative / str(name)
            if member.isdir():
                target.mkdir(parents=True, exist_ok=True)
            else:
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_bytes(archive.extractfile(member).read())
                target.chmod(member.mode)
    path = negative / SOURCE
    original = path.read_text()
    begin = original.index('    fn write_external_or_require_exact(')
    end = original.index('    fn read_authority_file<', begin)
    method = original[begin:end]
    correction = '''        if unsafe { libc::fchmod(file.as_raw_fd(), 0o440) } != 0 {
            return Err(std::io::Error::last_os_error().into());
        }
'''
    handoff.require(method.count(correction) == 1, 'mutation target drift')
    path.write_text(original[:begin] + method.replace(correction, '', 1) + original[end:])
    previous = bounded.ROOT
    bounded.ROOT = negative
    try:
        record = run('negative-missing-fchmod', TEST + ['--lib', EXACT, '--', '--exact', '--test-threads=1', '--nocapture'], output, 900)
    finally:
        bounded.ROOT = previous
    raw = (output / 'negative-missing-fchmod/output.txt').read_bytes()
    handoff.require(record['exit_code'] == 101 and not record['deadline_exceeded']
                    and b'InvalidCustody' in raw and b'0 passed; 1 failed' in raw,
                    'missing-fchmod mutation was not rejected by the actual test')
    records.append(record)
    logs['negative-missing-fchmod/output.txt'] = handoff.digest(raw)

    record = run('authority-pending', [sys.executable, 'scripts/current_tree_authority_check.py'], output, 60)
    raw = (output / 'authority-pending/output.txt').read_bytes()
    handoff.require(record['exit_code'] != 0 and not record['deadline_exceeded']
                    and b'production entry drift' in raw, 'unexpected authority result')
    records.append(record)
    logs['authority-pending/output.txt'] = handoff.digest(raw)
    handoff.require(handoff.clean_ref() == ref, 'reviewed tree changed during gate')
    summary = dict(stage='O2 external-file mode source correction', status='SOURCE_REVIEW_PENDING',
        source_ref=ref, source_tree=handoff.git('rev-parse', 'HEAD^{tree}').decode().strip(),
        baseline_ref=BASE, source_gate_passed=True, commands=COMMANDS, records=records,
        logs_sha256=logs, authority_status='ACCEPTED_BASELINE_REBIND_PENDING',
        authority_drift_paths=sorted(drift), merge_ready=False, o2_status='HOLD',
        execution_authorized=False, vps_contacted=False, finam_contacted=False,
        operational_redis_activated=False, terminal_recovery_implemented=False,
        limitations=['local macOS source tests; not native Linux artifact acceptance',
                     'durable frontier fault injection; no new SIGKILL matrix',
                     'pending terminal recovery is a review proposal, not deployed code',
                     'targeted suites only; no complete workspace test or CI claim'])
    (output / 'summary.json').write_text(json.dumps(summary, indent=2) + '\n')
    print('PASS source gate; O2 HOLD; authority/recovery review pending', flush=True)


def package(output):
    ref = handoff.clean_ref()
    summary = json.loads((output / 'summary.json').read_text())
    handoff.require(summary['source_ref'] == ref, 'gate source mismatch')
    manifest, entries = handoff.packaging.source_manifest(ref)
    archive = ROOT / 'reports/handoff' / f'moex-trading-project-{ref[:7]}-o2-external-mode-review.zip'
    handoff.require(not archive.exists(), 'immutable archive exists')
    archive.parent.mkdir(parents=True, exist_ok=True)
    extra = {'handoff-commit.txt': handoff.marker(ref, summary['source_tree'], archive.name),
             'handoff-evidence/source-commit.raw': handoff.git('cat-file', 'commit', ref),
             'handoff-evidence/source-tree-manifest.json': manifest,
             'handoff-evidence/summary.json': (output / 'summary.json').read_bytes()}
    for name in summary['logs_sha256']:
        extra['handoff-evidence/' + name] = (output / name).read_bytes()
    with zipfile.ZipFile(archive, 'x', zipfile.ZIP_DEFLATED, compresslevel=9) as z:
        for entry in entries:
            name = entry['path']
            handoff.require(name not in extra, 'source/evidence collision')
            z.writestr(handoff.packaging.zip_info(name, entry['mode']), handoff.git('show', ref + ':' + name))
        for name, raw in extra.items():
            z.writestr(handoff.packaging.zip_info(name), raw)
    result = handoff.check_archive(archive)
    Path(str(archive) + '.sha256').write_text(result['archive_sha256'] + '  ' + archive.name + '\n')
    Path(str(archive) + '.safety.json').write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps(dict(result, archive=str(archive)), indent=2))


if __name__ == '__main__':
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('action', choices=['gate', 'package', 'check'])
    p.add_argument('path', type=Path)
    args = p.parse_args()
    if args.action == 'gate':
        gate(args.path.resolve())
    elif args.action == 'package':
        package(args.path.resolve())
    else:
        print(json.dumps(handoff.check_archive(args.path.resolve()), indent=2))
