#!/usr/bin/env python3
"""Local gate and immutable review package; no target contact or activation."""
import argparse
import io
import json
from pathlib import Path, PurePosixPath
import subprocess
import tempfile
import zipfile

import make_stage8b_design_handoff as common
import stage8b_p1e_i1a_handoff_safety_check as objects
import stage8b_p1f_o2_successor_artifact as artifact
import stage8b_p1f_o2_terminal_update as update

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / 'reports/handoff'
ARTIFACT_SHA = 'a671a9b6e238ae9ec4540b29f413ea4a15fedf00c1d4744c5aa20eb9e3d48923'
TERMINAL = 'finam-o2-43d5f4e-terminal-recovery-evidence-20260930.zip'
IMAGE = 'sha256:9bec3cfe280a732eea281cb1add79f75523d1fe2ac86c12b129e4bc2509ec163'
PREFIX = 'successor-review-evidence/'
sha, require = artifact.sha, artifact.require

def git(*args):
    return subprocess.check_output(['git', *args], cwd=ROOT)

def clean_ref():
    require(not git('status', '--porcelain', '--untracked-files=all').strip(), 'dirty review tree')
    return git('rev-parse', 'HEAD').decode().strip()

def marker(ref, tree, name):
    return f'source_short_ref={ref[:7]}\nsource_ref={ref}\nsource_tree={tree}\ncompiled_source_ref={artifact.source.SOURCE_REF}\narchive_name={name}\n'.encode()

def gate(directory):
    ref = clean_ref()
    directory.mkdir(parents=True, exist_ok=False)
    # This is an unprivileged disposable filesystem fixture, not the systemd
    # proof container, and it never mounts operational or credential paths.
    docker = ['docker', 'run', '--rm', '--network', 'none', '--platform', 'linux/arm64']
    for local, remote in (
        (ROOT / 'scripts', '/work/scripts'),
        (OUT / 'moex-trading-project-7196aaa-stage8b-p1f-o2-execution-artifact.zip', '/inputs/old.zip'),
        (update.SPEC, '/inputs/spec.json'),
        (ROOT / 'tmp/o2-successor-update-payload', '/payload'),
    ):
        docker += ['--mount', f'type=bind,src={local},dst={remote},readonly']
    docker += [IMAGE, 'python3', '/work/scripts/test_stage8b_p1f_o2_terminal_update.py']
    scripts = sorted(str(p.relative_to(ROOT)) for p in (ROOT / 'scripts').glob('*o2*.py')
                     if any(word in p.name for word in ('successor', 'terminal_update', 'lock_mount')))
    commands = [
        ('python-syntax', ['python3', '-m', 'py_compile', *scripts]),
        ('rust-fmt', ['cargo', 'fmt', '--all', '--', '--check']),
        ('diff', ['git', 'diff', '--check', artifact.source.SOURCE_REF]),
        ('authority', ['python3', 'scripts/current_tree_authority_check.py']),
        ('authority-negative', ['python3', 'scripts/current_tree_authority_negative_harness.py']),
        ('artifact-negative', ['python3', 'scripts/test_stage8b_p1f_o2_successor_artifact.py', str(OUT / artifact.NAME)]),
        ('update-linux', docker),
    ]
    records = []
    for name, command in commands:
        print('RUN ' + name, flush=True)
        with (directory / (name + '.log')).open('wb') as stream:
            result = subprocess.run(command, cwd=ROOT, stdout=stream, stderr=subprocess.STDOUT)
        records.append(dict(name=name, command=command, exit_code=result.returncode))
        (directory / 'commands.json').write_text(json.dumps(records, indent=2) + '\n')
        require(result.returncode == 0, 'gate failed: ' + name)
    require(not git('diff', '--name-only', artifact.source.SOURCE_REF, '--', 'crates', 'Cargo.toml', 'Cargo.lock', 'deploy', 'config', '.github').strip(), 'production/config/workflow scope drift')
    require(clean_ref() == ref, 'source changed during gate')
    summary = dict(source_ref=ref, source_tree=git('rev-parse', 'HEAD^{tree}').decode().strip(),
                   compiled_source_ref=artifact.source.SOURCE_REF, status='REVIEW_PENDING', gate_passed=True,
                   vps_contacted=False, execution_authorized=False, operational_installation_performed=False,
                   rust_tests='inherited accepted source; fresh release build/ELF smoke in nested artifact',
                   systemd_proof='native ARM fixture, not native amd64 or production materialization',
                   update_tests='11 Linux filesystem tests, 12 exception/reopen frontiers; not SIGKILL',
                   artifact_negative_cases=8, authority_negative_cases=45,
                   logs_sha256={p.name: sha(p.read_bytes()) for p in directory.iterdir() if p.is_file()})
    (directory / 'summary.json').write_text(json.dumps(summary, indent=2) + '\n')
    print('PASS successor review gate; deployment/activation remain closed', flush=True)

def safe_files(path):
    with zipfile.ZipFile(path) as z:
        infos = z.infolist()
        require(z.testzip() is None and len({i.filename for i in infos}) == len(infos), 'ZIP CRC/duplicates')
        for i in infos:
            p = PurePosixPath(i.filename)
            require(str(p) == i.filename and not p.is_absolute() and '..' not in p.parts and '\\' not in i.filename, 'unsafe path')
            require(i.external_attr >> 16 in (0o100644, 0o100755), 'symlink/special member')
            require(not {'.git', '.env', 'target', 'tmp', '__pycache__'} & set(p.parts), 'excluded path')
        return {i.filename: z.read(i) for i in infos}, {i.filename: f'{i.external_attr >> 16:06o}' for i in infos}

def check(path):
    files, modes = safe_files(path)
    meta = json.loads(files[PREFIX + 'descriptor.json'])
    manifest = json.loads(files[PREFIX + 'source-tree-manifest.json'])
    entries = manifest['entries']
    source = {}
    for e in entries:
        name = e['path']
        require(name not in source and sha(files[name]) == e['sha256'] and len(files[name]) == e['size']
                and modes[name] == e['mode'], 'source inventory')
        source[name] = files[name]
    require(manifest['entry_count'] == len(entries), 'source cardinality')
    raw_commit = files[PREFIX + 'source-commit.raw']
    ref = objects.git_object_id('commit', raw_commit)
    tree = objects.build_tree_oid(entries, source)
    require(ref == meta['source_ref'] == manifest['source_ref'] and tree == meta['source_tree']
            and raw_commit.splitlines()[0] == f'tree {tree}'.encode(), 'Git binding')
    generated = meta['generated_sha256']
    require(not set(source) & set(generated) and set(files) == set(source) | set(generated) | {PREFIX + 'descriptor.json'}, 'package inventory')
    require(all(sha(files[n]) == h for n, h in generated.items()), 'generated hash drift')
    require(files['handoff-commit.txt'] == marker(ref, tree, path.name), 'marker mismatch')
    summary = json.loads(files[PREFIX + 'gate/summary.json'])
    require(summary['source_ref'] == ref and summary['source_tree'] == tree and summary['gate_passed'] is True
            and summary['status'] == 'REVIEW_PENDING', 'gate binding')
    require(all(summary[k] is False for k in ('vps_contacted', 'execution_authorized', 'operational_installation_performed')), 'scope opened')
    for name, digest in summary['logs_sha256'].items():
        require(sha(files[PREFIX + 'gate/' + name]) == digest, 'gate log drift')
    records = json.loads(files[PREFIX + 'gate/commands.json'])
    require([r['name'] for r in records] == ['python-syntax', 'rust-fmt', 'diff', 'authority', 'authority-negative', 'artifact-negative', 'update-linux']
            and all(type(r['exit_code']) is int and r['exit_code'] == 0 for r in records), 'gate command inventory')
    for log, witness in [('update-linux', b'Ran 11 tests'), ('update-linux', b'PASS terminal-update durable-payload-frontiers 12/12'),
                         ('artifact-negative', b'PASS successor-artifact negatives=8/8'),
                         ('authority-negative', b'current-tree-authority-negative: PASS cases=45/45')]:
        require(witness in files[PREFIX + 'gate/' + log + '.log'], 'missing test witness')
    new_raw = files['artifacts/' + artifact.NAME]
    require(sha(new_raw) == ARTIFACT_SHA, 'full artifact pin')
    require(sha(files['accepted-evidence/' + TERMINAL]) == update.TERMINAL_SHA, 'terminal evidence pin')
    with tempfile.TemporaryDirectory(prefix='o2-review-check-') as temp:
        nested = Path(temp) / artifact.NAME
        nested.write_bytes(new_raw)
        artifact.check(nested)
    spec = json.loads(files['docs/stage-8/stage8b-p1f-o2-terminal-update.json'])
    require(spec['artifact_sha256'] == ARTIFACT_SHA and spec['terminal_evidence_sha256'] == update.TERMINAL_SHA
            and spec['execution_authorized'] is False and spec['target_mutation_performed'] is False
            and set(spec['updates']) == set(update.CHANGES), 'update descriptor scope')
    for name in update.CHANGES:
        require(sha(files['update-payload/' + Path(name).name]) == spec['updates'][name]['new_sha256'], 'update payload hash')
    install = json.loads(files['update-payload/' + Path(update.custody.MANIFEST).name])
    require(sha(files['update-payload/' + Path(update.custody.MANIFEST).name]) == spec['new_manifest_sha256']
            and install['installer_sha256'] == sha(source['scripts/stage8b_p1f_o2_terminal_update.py'])
            and install['installation_id'] == spec['preserved_installation_id'] == update.custody.INSTALLATION_ID, 'installation lineage')
    return dict(safe=True, archive_sha256=sha(path.read_bytes()), source_ref=ref, source_tree=tree,
                compiled_source_ref=artifact.source.SOURCE_REF, members=len(files), tracked=len(source),
                duplicates=0, unsafe_paths=0, symlinks=0, special_files=0, commit_tree_binding=True,
                full_artifact_binding=True, terminal_history_binding=True, execution_authorized=False)

def package(gate_dir):
    ref = clean_ref()
    summary = json.loads((gate_dir / 'summary.json').read_text())
    require(summary['source_ref'] == ref and summary['gate_passed'], 'gate not bound to HEAD')
    manifest, entries = common.source_manifest(ref)
    path = OUT / f'moex-trading-project-{ref[:7]}-o2-terminal-successor-review.zip'
    require(not path.exists(), 'immutable review archive exists')
    generated = {'handoff-commit.txt': marker(ref, summary['source_tree'], path.name),
                 PREFIX + 'source-tree-manifest.json': manifest,
                 PREFIX + 'source-commit.raw': git('cat-file', 'commit', ref),
                 'artifacts/' + artifact.NAME: (OUT / artifact.NAME).read_bytes(),
                 'accepted-evidence/' + TERMINAL: (OUT / TERMINAL).read_bytes()}
    for name in [*summary['logs_sha256'], 'summary.json']:
        generated[PREFIX + 'gate/' + name] = (gate_dir / name).read_bytes()
    for name in update.CHANGES:
        generated['update-payload/' + Path(name).name] = (ROOT / 'tmp/o2-successor-update-payload' / Path(name).name).read_bytes()
    # Retain setup failures and initial runs. They are NOT passing gate evidence.
    for directory in ('o2-successor-lock-proof', 'o2-successor-lock-proof-r1', 'o2-successor-lock-proof-r2',
                      'o2-successor-lock-proof-native', 'o2-successor-lock-proof-native-r1'):
        for name in ('commands.json', 'journal.txt', 'result.json', 'container.json'):
            p = ROOT / 'tmp' / directory / name
            if p.is_file():
                generated[PREFIX + 'retained-local-attempts/' + directory + '/' + name] = p.read_bytes()
    for name in ('elf-smoke-initial-exec-format.log',):
        generated[PREFIX + 'retained-local-attempts/' + name] = (ROOT / 'tmp/o2-successor-linux-589b801' / name).read_bytes()
    for name in ('linux-update-tests.log', 'linux-update-tests-final.log'):
        generated[PREFIX + 'retained-local-attempts/' + name] = (ROOT / 'tmp/o2-successor-gate' / name).read_bytes()
    descriptor = dict(source_ref=ref, source_tree=summary['source_tree'],
                      compiled_source_ref=artifact.source.SOURCE_REF,
                      generated_sha256={n: sha(raw) for n, raw in generated.items()})
    generated[PREFIX + 'descriptor.json'] = (json.dumps(descriptor, indent=2) + '\n').encode()
    require(not {e['path'] for e in entries} & set(generated), 'source/evidence collision')
    with zipfile.ZipFile(path, 'x', zipfile.ZIP_DEFLATED) as z:
        for e in entries:
            z.writestr(common.zip_info(e['path'], e['mode']), git('show', f"{ref}:{e['path']}"))
        for name, raw in generated.items():
            z.writestr(common.zip_info(name), raw)
    report = check(path)
    require(clean_ref() == ref, 'source changed during packaging')
    Path(str(path) + '.sha256').write_text(report['archive_sha256'] + '  ' + path.name + '\n')
    Path(str(path) + '.safety.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report | {'archive': str(path)}, indent=2), flush=True)

if __name__ == '__main__':
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('action', choices=('gate', 'package', 'check'))
    p.add_argument('path', type=Path)
    a = p.parse_args()
    if a.action == 'check':
        print(json.dumps(check(a.path.resolve()), indent=2))
    else:
        {'gate': gate, 'package': package}[a.action](a.path.resolve())
