#!/usr/bin/env python3
"""Source-review snapshot + allowlisted offline logs. Never an installation ZIP.

Reuse existing Git-tree/member-safety helpers. Private staged source, binaries,
ceremony material and the generated historical installation fixture are excluded.
"""
import argparse
import json
from pathlib import Path
import subprocess
import zipfile

import make_stage8b_design_handoff as packing
import stage8b_p1e_i1a_handoff_safety_check as tree
from stage8b_p1f_alor_finam_source_correction_handoff_safety_check import validate_member_name
from stage8b_p1f_o2_no_riskgate_artifact import snapshot, encoded
from stage8b_p1f_o2_v4_timestamp_install_package import sha

ROOT = Path(__file__).resolve().parents[1]
BASE = '836a2973495b505fac56422308da4a65e921d930'
COMPILED = 'f3b349949802abd5eff80cad6b9e9fc37bc327e1'
P = 'handoff-evidence/o2-template-encoding/'
GATE_FILES = ('result.json', 'commands.json', *(f'{i:02d}.{suffix}' for i in range(8) for suffix in ('stdout', 'stderr')))
CHECK_FILES = ('result.json', 'authority.stdout', 'authority.stderr', 'authority-negative.stdout', 'authority-negative.stderr')


def require(value, message):
    if not value:
        raise ValueError(message)


def git(*args):
    return subprocess.check_output(['git', *args], cwd=ROOT)


def check(path):
    with zipfile.ZipFile(path) as z:
        infos = z.infolist()
        require(len(infos) == len(set(z.namelist())) and z.testzip() is None, 'duplicates/CRC')
        for i in infos:
            validate_member_name(i.filename)
            require(i.filename == str(Path(i.filename).as_posix()), 'noncanonical path')
            require(i.external_attr >> 16 in (0o100644, 0o100755), 'nonregular mode')
        files = {i.filename: z.read(i) for i in infos}
        modes = {i.filename: f'{i.external_attr >> 16:06o}' for i in infos}
    descriptor = json.loads(files[P + 'descriptor.json'])
    manifest = json.loads(files[P + 'source-manifest.json'])
    entries = manifest['entries']
    require(len(entries) == manifest['entry_count'] == len({e['path'] for e in entries}), 'source count')
    tracked = {e['path']: files[e['path']] for e in entries}
    for e in entries:
        raw = tracked[e['path']]
        require(sha(raw) == e['sha256'] and len(raw) == e['size'] and modes[e['path']] == e['mode'], 'source bytes')
    raw_commit = files[P + 'source-commit.raw']
    ref = tree.git_object_id('commit', raw_commit)
    require(ref == manifest['source_ref'] == descriptor['source_ref'], 'source ref')
    headers = raw_commit.split(b'\n\n', 1)[0].splitlines()
    require(headers[0] == ('tree ' + tree.build_tree_oid(entries, tracked)).encode(), 'source tree')
    require([h for h in headers if h.startswith(b'parent ')] == [('parent ' + BASE).encode()], 'parent')
    expected = {P + n for n in ('source-manifest.json', 'source-commit.raw', 'source-diff.patch', 'workspace-status.txt')}
    expected |= {P + 'gate/' + n for n in GATE_FILES} | {P + 'checks/' + n for n in CHECK_FILES} | {'handoff-commit.txt'}
    require(set(descriptor['generated_sha256']) == expected, 'generated allowlist')
    require(set(files) == set(tracked) | expected | {P + 'descriptor.json'}, 'archive allowlist')
    require(all(sha(files[n]) == h for n, h in descriptor['generated_sha256'].items()), 'generated hash')
    require(files['handoff-commit.txt'] == f'source_ref={ref}\nsource_short_ref={ref[:7]}\ncompiled_ref={COMPILED}\narchive_name={path.name}\n'.encode(), 'marker')
    require(descriptor['baseline_ref'] == BASE and descriptor['compiled_ref'] == COMPILED, 'lineage')
    require(descriptor['scope'] == 'PACKAGING_CORRECTION_REVIEW_NOT_INSTALLABLE', 'scope')
    for flag in ('installation_authorized', 'execution_authorized', 'independent_acceptance'):
        require(descriptor[flag] is False, 'authority expansion')
    require(descriptor['o2_status'] == 'HOLD', 'O2 status')
    gate = json.loads(files[P + 'gate/result.json'])
    require(gate['result'] == 'PASS' and gate['commands'] == len(gate['records']) == 8, 'gate result')
    require(gate['records'] == json.loads(files[P + 'gate/commands.json']), 'command records')
    require(gate['fixture_authority_only'] is True and gate['o2_verdict'] == 'HOLD', 'fixture boundary')
    for flag in ('vps_mutations', 'production_keys_used', 'redis_contact', 'finam_contact',
                 'operational_successor_prepared', 'installation_authorized', 'execution_authorized'):
        require(gate[flag] is False, 'gate boundary')
    for n, h in gate['tested_source_sha256'].items():
        require(sha(tracked[n]) == h, 'tested source binding')
    for i, record in enumerate(gate['records']):
        require(record['exit_code'] == record['expected_exit'] == (101 if i == 7 else 0), 'gate exit')
        for suffix in ('stdout', 'stderr'):
            require(sha(files[P + f'gate/{i:02d}.{suffix}']) == record[suffix + '_sha256'], 'gate log')
    require(b'ReadyForBootstrap, exact source/config reread' in files[P + 'gate/04.stdout'], 'guardian witness')
    require(b'packaged supervisor template must be canonical' in files[P + 'gate/07.stderr'], 'negative witness')
    checks = json.loads(files[P + 'checks/result.json'])
    require(checks['source_ref'] == ref and checks['clean_checkout'] is True, 'clean checks binding')
    require(set(checks['records']) == {'authority', 'authority-negative'}, 'checks inventory')
    for name, record in checks['records'].items():
        require(record['exit_code'] == 0, 'authority failure')
        for suffix in ('stdout', 'stderr'):
            require(sha(files[P + f'checks/{name}.{suffix}']) == record[suffix + '_sha256'], 'authority log')
    require(b'cases=45/45' in files[P + 'checks/authority-negative.stdout'], 'negative count')
    return dict(source_ref=ref, compiled_ref=COMPILED, archive_sha256=sha(path.read_bytes()),
                archive_bytes=path.stat().st_size, members=len(files), tracked=len(tracked),
                duplicates=0, unsafe_paths=0, symlinks=0, special_files=0, git_tree_binding='PASS',
                private_staged_source_included=False, installation_payload_included=False,
                offline_gate='PASS', o2_status='HOLD', installation_authorized=False, execution_authorized=False)


def build(output, gate, checks):
    ref = git('rev-parse', 'HEAD').decode().strip()
    require(git('rev-parse', 'HEAD^').decode().strip() == BASE, 'unexpected parent')
    require(not git('diff', '--name-only', BASE, ref, '--', 'crates', 'Cargo.toml', 'Cargo.lock', 'deploy', '.github').strip(), 'production/deploy/CI delta')
    source, modes, manifest = snapshot(ref)
    generated = {P + 'source-manifest.json': encoded(manifest),
                 P + 'source-commit.raw': git('cat-file', 'commit', ref),
                 P + 'source-diff.patch': git('diff', '--binary', BASE, ref),
                 P + 'workspace-status.txt': git('status', '--short'),
                 'handoff-commit.txt': f'source_ref={ref}\nsource_short_ref={ref[:7]}\ncompiled_ref={COMPILED}\narchive_name={output.name}\n'.encode()}
    for label, directory, names in (('gate', gate, GATE_FILES), ('checks', checks, CHECK_FILES)):
        for name in names:
            path = directory / name
            require(path.is_file() and not path.is_symlink(), 'nonregular evidence')
            generated[P + label + '/' + name] = path.read_bytes()
    require(not set(source) & set(generated), 'source/evidence collision')
    generated[P + 'descriptor.json'] = encoded(dict(source_ref=ref, baseline_ref=BASE, compiled_ref=COMPILED,
        scope='PACKAGING_CORRECTION_REVIEW_NOT_INSTALLABLE', o2_status='HOLD', installation_authorized=False,
        execution_authorized=False, independent_acceptance=False, generated_sha256={n: sha(v) for n, v in generated.items()}))
    with zipfile.ZipFile(output, 'x', zipfile.ZIP_DEFLATED) as z:
        for name, raw in (source | generated).items():
            z.writestr(packing.zip_info(name, modes.get(name, '100644')), raw)
    result = check(output)
    Path(str(output) + '.sha256').write_text(result['archive_sha256'] + '  ' + output.name + '\n')
    Path(str(output) + '.safety.json').write_bytes(encoded(result))
    return result


if __name__ == '__main__':
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('action', choices=('build', 'check'))
    p.add_argument('archive', type=Path)
    p.add_argument('--gate', type=Path)
    p.add_argument('--checks', type=Path)
    a = p.parse_args()
    result = build(a.archive.resolve(), a.gate.resolve(), a.checks.resolve()) if a.action == 'build' else check(a.archive.resolve())
    print(json.dumps(result, indent=2))
