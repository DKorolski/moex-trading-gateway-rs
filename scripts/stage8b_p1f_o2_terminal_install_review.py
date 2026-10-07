#!/usr/bin/env python3
"""Local bounded installation gate and immutable handoff, without VPS access."""
import argparse
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import uuid
import zipfile

import stage8b_p1f_nrg01_test_gate as bounded
import stage8b_p1f_o2_v4_timestamp_artifact as qualified
artifact = qualified.base
import stage8b_p1f_o2_terminal_install_package as prep

ROOT = prep.ROOT
P = 'installation-review/'
ARM_IMAGE = 'sha256:9bec3cfe280a732eea281cb1add79f75523d1fe2ac86c12b129e4bc2509ec163'
require, sha = prep.require, prep.sha


def clean_ref():
    require(not artifact.source.builder.git('status', '--porcelain').strip(), 'clean committed tree required')
    return artifact.source.builder.git('rev-parse', 'HEAD').decode().strip()


def gate(package, build, output):
    ref = clean_ref()
    prep.validate_prepared(package)
    output.mkdir(parents=True, exist_ok=False)
    before = bounded.inventory()
    inputs = {p.relative_to(package).as_posix(): sha(p.read_bytes()) for p in package.rglob('*') if p.is_file()}
    records = []
    def run(command, seconds=120, expected_exit=0):
        name = None
        if command[:2] == ['docker', 'run']:
            name = 'stage8b-nrg-install-' + uuid.uuid4().hex[:12]
            command = command[:2] + ['--name', name] + command[2:]
        print('RUN ' + str(len(records)) + ' ' + ' '.join(command), flush=True)
        try:
            record = bounded.run(command, output/f'{len(records):02d}', seconds)
        finally:
            if name:
                subprocess.run(['docker', 'rm', '-f', name], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=30)
        record['expected_exit'] = expected_exit
        records.append(record)
        (output/'commands.json').write_bytes(artifact.encoded(records))
        require(record['exit_code'] == expected_exit and not record['deadline_exceeded'], 'gate failed; logs retained')
        print('PASS ' + record['directory'], flush=True)
    scripts = sorted(str(p.relative_to(ROOT)) for p in (ROOT/'scripts').glob('*terminal_install*.py'))
    run([sys.executable, '-m', 'py_compile', *scripts])
    run(['bash', '-n', 'scripts/stage8b_p1f_o2_sparse_install_smoke.sh'])
    run(['rustfmt', '--edition', '2021', '--check', 'scripts/fixtures/stage8b-o2-sparse-install-probe.rs'])
    run([sys.executable, 'scripts/current_tree_authority_check.py'])
    run([sys.executable, 'scripts/current_tree_authority_negative_harness.py'], 300)
    run([sys.executable, 'scripts/test_stage8b_p1f_o2_terminal_install_package.py', str(package)], 180)
    docker = ['docker', 'run', '--rm', '--network', 'none', '--ulimit', 'core=0']
    def mount(path, target, readonly=True):
        return ['--mount', f'type=bind,src={path},dst={target}' + (',readonly' if readonly else '')]
    run(docker + ['--platform', 'linux/arm64'] + mount(ROOT/'scripts', '/work/scripts') + mount(package, '/package') +
        [ARM_IMAGE, 'python3', '/work/scripts/test_stage8b_p1f_o2_terminal_install.py'], 180)
    # The released payload bytes must be those in the accepted build output.
    info = json.loads((build/'build.json').read_bytes())
    require(info['implementation_ref'] == prep.COMPILED_REF and info['rust_image'] == artifact.source.IMAGE, 'build recipe')
    for binary in info['binaries']:
        require(sha((package/'payload'/binary['name']).read_bytes()) == binary['sha256'] == sha((build/'target/release'/binary['name']).read_bytes()), 'ELF mismatch')
    deps = build/'target/release/deps'
    with prep.pinned_zip(package/'binary-artifact.zip', prep.ARTIFACT_SHA) as z:
        accepted_rlibs = json.loads(z.read(prep.qualified.HANDOFF_PREFIX+'inherited/result.json'))['rlib_sha256']
    externs = []
    linked_rlibs = {}
    for name in ('runtime_durable_service', 'finam_gateway', 'serde_json', 'chrono'):
        # Timestamp release tests left additional dependency variants in target.
        # Select only the exact filename AND hash retained by the accepted probe.
        # Never choose the newest file or rebuild a differently configured rlib.
        choices = [deps / filename for filename in accepted_rlibs
                   if filename.startswith(f'lib{name}-') and filename.endswith('.rlib')]
        require(len(choices) == 1, 'ambiguous accepted dependency')
        digest = sha(choices[0].read_bytes())
        require(accepted_rlibs.get(choices[0].name) == digest, 'accepted release rlib mismatch')
        linked_rlibs[choices[0].name] = digest
        externs += ['--extern', name + '=/deps/' + choices[0].name]
    amd = docker + ['--platform', 'linux/amd64']
    run(amd + mount(deps, '/deps') + mount(ROOT/'scripts/fixtures/stage8b-o2-sparse-install-probe.rs', '/probe.rs') +
        mount(output, '/proof', False) + [artifact.source.IMAGE, 'rustc', '--edition=2021', '-D', 'warnings', '/probe.rs', '-L', 'dependency=/deps',
                                       *externs, '-o', '/proof/input-probe'])
    run(amd + mount(output, '/proof') + mount(package, '/package') + [artifact.source.IMAGE, '/proof/input-probe'])
    run(amd + mount(package, '/package') + mount(package/'payload', '/payload') + mount(build/'source/deploy/stage8b-p1e', '/units') +
        mount(build/'source/scripts/stage8b_p1f_o2_elf_smoke.sh', '/legacy-smoke.sh') +
        mount(build/'source/docs/stage-8/stage8b-p1f-o2-supervisor-template.json', '/template.json') +
        mount(ROOT/'scripts/stage8b_p1f_o2_sparse_install_smoke.sh', '/smoke.sh') + [artifact.source.IMAGE, 'bash', '/smoke.sh'])
    run(['git', 'diff', '--exit-code', '21cc7eb9530ccc4480bae2a4f8264705a7383c01', 'HEAD', '--', 'crates', 'Cargo.toml', 'Cargo.lock', 'deploy', '.github'])
    run(['git', 'diff', '--check'])
    lf = output/'old-lf-template.json'
    lf.write_bytes((package/'payload/supervisor.template.json').read_bytes() + b'\n')
    run(amd + mount(output, '/proof') + mount(package, '/package') +
        mount(lf, '/package/payload/supervisor.template.json') + [artifact.source.IMAGE, '/proof/input-probe'],
        expected_exit=101)
    require(b'packaged supervisor template must be canonical serde_json without LF'
            in (output/'12/output.txt').read_bytes(), 'LF negative failed elsewhere')
    require(clean_ref() == ref and before == bounded.inventory(), 'source changed')
    require(inputs == {p.relative_to(package).as_posix(): sha(p.read_bytes()) for p in package.rglob('*') if p.is_file()}, 'inputs changed')
    summary = dict(source_ref=ref, gate_passed=True, commands=records, planned_commands=13, input_sha256=inputs,
                   linked_rlib_sha256=linked_rlibs,
                   vps_contact=False, execution_authorized=False, installation_performed=False,
                   filesystem_tests=17, injected_frontiers=14, prepared_negatives=24, authority_negatives=45,
                   systemd_observations='mocked in native ARM Linux filesystem fixtures; no target execution',
                   elf_platform='Docker linux/amd64 emulation, network none')
    (output/'result.json').write_bytes(artifact.encoded(summary))
    print('PASS terminal-successor-install gate 13/13; installation/execution=false')


def check(path):
    files, modes = {}, {}
    with zipfile.ZipFile(path) as z:
        require(z.testzip() is None and len(z.namelist()) == len(set(z.namelist())), 'CRC/duplicates')
        for info in z.infolist():
            artifact.safety.validate_member_name(info.filename)
            require(str(artifact.PurePosixPath(info.filename)) == info.filename, 'noncanonical member')
            require(info.external_attr >> 16 in (0o100644, 0o100755), 'symlink/special member')
            files[info.filename] = z.read(info)
            modes[info.filename] = f'{info.external_attr >> 16:06o}'
    meta = json.loads(files[P+'descriptor.json'])
    manifest = json.loads(files[P+'source-manifest.json'])
    entries = manifest['entries']
    require(len(entries) == manifest['entry_count'] == len({e['path'] for e in entries}), 'source cardinality')
    blobs = {e['path']: files[e['path']] for e in entries}
    require(all(sha(blobs[e['path']]) == e['sha256'] and len(blobs[e['path']]) == e['size'] and modes[e['path']] == e['mode'] for e in entries), 'source manifest')
    tree = artifact.objects.build_tree_oid(entries, blobs)
    commit = files[P+'source-commit.raw']
    ref = artifact.objects.git_object_id('commit', commit)
    require(ref == manifest['source_ref'] == meta['source_ref'] and commit.splitlines()[0] == f'tree {tree}'.encode(), 'Git binding')
    require(set(files) == set(blobs) | set(meta['generated_sha256']) | {P+'descriptor.json'}, 'member inventory')
    require(all(sha(files[p]) == digest for p, digest in meta['generated_sha256'].items()), 'generated bytes')
    require(meta['installation_authorized'] is False and meta['execution_authorized'] is False, 'scope opened')
    require(files['handoff-commit.txt'] == f'source_ref={ref}\ncompiled_ref={prep.COMPILED_REF}\narchive_name={path.name}\n'.encode(), 'marker')
    with tempfile.TemporaryDirectory(prefix='nrg-install-check-') as temp:
        root = Path(temp)
        for name, raw in files.items():
            if name.startswith('installation-terminal/'):
                output = root/name
                output.parent.mkdir(parents=True, exist_ok=True)
                output.write_bytes(raw)
        spec = prep.validate_prepared(root/'installation-terminal')
    gate = json.loads(files[P+'gate/result.json'])
    require(gate['source_ref'] == ref and gate['gate_passed'] is True and gate['planned_commands'] == len(gate['commands']) == 13, 'gate binding')
    require(all(gate[k] is False for k in ('vps_contact', 'execution_authorized', 'installation_performed')), 'gate scope')
    prepared = {n.removeprefix('installation-terminal/'): sha(raw) for n, raw in files.items() if n.startswith('installation-terminal/')}
    require(gate['input_sha256'] == prepared, 'tested input binding')
    for index, record in enumerate(gate['commands']):
        require(type(record['exit_code']) is int and record['exit_code'] == record['expected_exit'] == (101 if index == 12 else 0) and record['deadline_exceeded'] is False and record['cleanup_error'] is None, 'gate exit')
        for name, digest in record['artifacts'].items():
            require(sha(files[P+'gate/'+record['directory']+'/'+name]) == digest, 'gate log')
    for i, witness in [('04', 'cases=45/45'), ('05', 'negatives=24/24'), ('06', 'Ran 17 tests'),
                       ('06', 'frontiers 14/14'), ('08', 'PASS accepted-rlib source-v4'), ('09', 'PASS sparse-install ELF inputs'), ('12', 'packaged supervisor template must be canonical')]:
        require(witness.encode() in files[P+f'gate/{i}/output.txt'], 'gate witness')
    return dict(archive_sha256=sha(path.read_bytes()), source_ref=ref, compiled_ref=prep.COMPILED_REF,
                members=len(files), tracked=len(blobs), duplicates=0, unsafe_paths=0, symlinks=0, special_files=0,
                new_installation_sha256=spec['new_manifest_sha256'], expected_terminal='EXPIRED/1/12',
                write_slots=8, installation_authorized=False, execution_authorized=False, gate_passed=True)


def package(prepared, gate_dir, output):
    ref = clean_ref()
    prep.validate_prepared(prepared)
    source, modes, manifest = artifact.snapshot(ref)
    generated = {P+'source-manifest.json': artifact.encoded(manifest), P+'source-commit.raw': artifact.source.builder.git('cat-file', 'commit', ref)}
    for name, directory in [('installation-terminal/', prepared), (P+'gate/', gate_dir)]:
        for p in sorted(directory.rglob('*')):
            if p.is_file() and p.name != 'input-probe':
                require(not p.is_symlink(), 'generated symlink')
                generated[name+p.relative_to(directory).as_posix()] = p.read_bytes()
    generated['handoff-commit.txt'] = f'source_ref={ref}\ncompiled_ref={prep.COMPILED_REF}\narchive_name={output.name}\n'.encode()
    require(not set(source) & set(generated), 'source/evidence collision')
    generated[P+'descriptor.json'] = artifact.encoded(dict(source_ref=ref, installation_authorized=False, execution_authorized=False,
                                                          generated_sha256={n: sha(raw) for n, raw in generated.items()}))
    with zipfile.ZipFile(output, 'x', zipfile.ZIP_DEFLATED) as z:
        for name, raw in (source | generated).items():
            z.writestr(artifact.common.zip_info(name, modes.get(name, '100644')), raw)
    result = check(output)
    Path(str(output)+'.sha256').write_text(result['archive_sha256']+'  '+output.name+'\n')
    Path(str(output)+'.safety.json').write_bytes(artifact.encoded(result))
    print(json.dumps(result, indent=2))


if __name__ == '__main__':
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('action', choices=('gate', 'package', 'check'))
    p.add_argument('output', type=Path)
    p.add_argument('--prepared', type=Path)
    p.add_argument('--build', type=Path)
    p.add_argument('--gate', type=Path)
    a = p.parse_args()
    if a.action == 'gate':
        gate(a.prepared.resolve(), a.build.resolve(), a.output.resolve())
    elif a.action == 'package':
        package(a.prepared.resolve(), a.gate.resolve(), a.output.resolve())
    else:
        print(json.dumps(check(a.output.resolve()), indent=2))
