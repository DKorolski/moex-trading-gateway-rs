#!/usr/bin/env python3
"""Seal full O2 artifact built from the accepted merge, not a moving checkout."""
import argparse
import io
import json
from pathlib import Path, PurePosixPath
import subprocess
import zipfile

import make_stage8b_design_handoff as common
import stage8b_p1e_i1a_handoff_safety_check as git_objects
import stage8b_p1f_o2_build_linux as builder
import stage8b_p1f_o2_successor_build as source
import stage8b_p1f_o2_install as old

ROOT = Path(__file__).resolve().parents[1]
NAME = 'moex-trading-project-589b801-o2-terminal-successor-artifact.zip'
REVIEW_SHA = '86d30c59b13f5681703abe3b03096f823f008fb3b1ea7cf814a58878a951950d'
sha = builder.sha
require = old.require

def check(path):
    with zipfile.ZipFile(path) as z:
        infos = z.infolist()
        require(z.testzip() is None and len({i.filename for i in infos}) == len(infos), 'ZIP CRC/duplicates')
        for i in infos:
            p = PurePosixPath(i.filename)
            require(str(p) == i.filename and not p.is_absolute() and '..' not in p.parts and '\\' not in i.filename, 'unsafe path')
            require(i.external_attr >> 16 in (0o100644, 0o100755), 'nonregular member')
        files = {i.filename: z.read(i) for i in infos}
    meta = json.loads(files['artifact-evidence/descriptor.json'])
    require(meta['execution_authorized'] is False and meta['target_mutation_performed'] is False, 'execution opened')
    require(meta['implementation_ref'] == source.SOURCE_REF and meta['source_tree'] == source.SOURCE_TREE, 'compiled baseline drift')
    require(set(meta['extras_sha256']) == {n for n in files if n.startswith('artifact-evidence/')} - {'artifact-evidence/descriptor.json'}, 'extra inventory')
    for name, digest in meta['extras_sha256'].items():
        require(sha(files[name]) == digest, 'evidence digest mismatch')
    manifest = json.loads(files['artifact-evidence/source-tree-manifest.json'])
    entries = manifest['entries']
    blobs = {}
    modes = {i.filename: f'{i.external_attr >> 16:06o}' for i in infos}
    for e in entries:
        name = e['path']
        require(name not in blobs and sha(files[name]) == e['sha256'] and len(files[name]) == e['size']
                and modes[name] == e['mode'], 'source inventory drift')
        blobs[name] = files[name]
    require(set(files) == set(blobs) | set(meta['extras_sha256']) | {'artifact-evidence/descriptor.json'}, 'member inventory drift')
    raw_commit = files['artifact-evidence/source-commit.raw']
    require(git_objects.git_object_id('commit', raw_commit) == source.SOURCE_REF and
            raw_commit.splitlines()[0] == f'tree {source.SOURCE_TREE}'.encode() and
            git_objects.build_tree_oid(entries, blobs) == source.SOURCE_TREE, 'raw Git binding')
    build = json.loads(files['artifact-evidence/build.json'])
    require(build['implementation_ref'] == source.SOURCE_REF and build['source_tree'] == source.SOURCE_TREE
            and build['rust_image'] == builder.IMAGE and build['cargo_args'] == builder.CARGO_ARGS
            and build['network'] == 'none' and build['cargo_offline'] is True
            and build['execution_authorized'] is False and build['target_mutation_performed'] is False, 'build recipe drift')
    require(build['build_log_sha256'] == sha(files['artifact-evidence/build.log'])
            and build['source_commit_raw_sha256'] == sha(raw_commit), 'build evidence drift')
    require(len(build['binaries']) == 3 and {b['name'] for b in build['binaries']} == set(builder.BINS), 'binary inventory')
    for b in build['binaries']:
        raw = files['artifact-evidence/payload/' + b['name']]
        require(sha(raw) == b['sha256'] and len(raw) == b['size'] and raw[:6] == b'\x7fELF\x02\x01'
                and int.from_bytes(raw[18:20], 'little') == 62, 'binary binding')
    proof = json.loads(files['artifact-evidence/lock-proof/result.json'])
    unit = files['deploy/stage8b-p1e/moex-finam-p1f-o2-materializer.service']
    require(proof['result'] == 'PASS' and proof['unit_sha256'] == sha(unit), 'lock proof binding')
    require(proof['production_materialization_executed'] is False and proof['finam_contact'] is False, 'fixture scope')
    require(len(proof['cases']) == 3, 'lock cases')
    for case, omit in zip(proof['cases'], (None, '.execution.lock', '.guardian.lock')):
        p = case['probe']
        require(p['authority_config_readonly'] and p['effective_capabilities_zero'] and p['no_new_privileges'], 'hardening proof')
        for name, result in p['lock_inodes'].items():
            require(result['opened'] == (name != omit), 'lock negative coverage')
    container = json.loads(files['artifact-evidence/lock-proof/container.json'])
    require(container['Mounts'] == [] and container['HostConfig']['NetworkMode'] == 'none'
            and container['HostConfig']['CgroupnsMode'] == 'private', 'fixture isolation')
    require(sha(files['artifact-evidence/terminal-acceptance.md']) == REVIEW_SHA, 'acceptance review drift')
    require(b'PASS stage8b-p1f-o2-elf-smoke' in files['artifact-evidence/elf-smoke.log'], 'ELF witness absent')
    return dict(safe=True, archive_sha256=sha(path.read_bytes()), members=len(files), source_ref=source.SOURCE_REF,
                source_tree=source.SOURCE_TREE, duplicates=0, unsafe_paths=0, symlinks=0, special_files=0,
                build_binding=True, lock_mount_witness=True, execution_authorized=False)

def package(build_dir, proof_dir, review):
    require(sha(review.read_bytes()) == REVIEW_SHA, 'terminal acceptance pin')
    manifest, entries = common.source_manifest(source.SOURCE_REF)
    files = {e['path']: builder.git('show', f"{source.SOURCE_REF}:{e['path']}") for e in entries}
    modes = {e['path']: e['mode'] for e in entries}
    extra = {'build.json': (build_dir / 'build.json').read_bytes(), 'build.log': (build_dir / 'build.log').read_bytes(),
             'source-commit.raw': (build_dir / 'source-commit.raw').read_bytes(), 'source-tree-manifest.json': manifest,
             'terminal-acceptance.md': review.read_bytes()}
    for name in builder.BINS:
        extra['payload/' + name] = (build_dir / 'target/release' / name).read_bytes()
    for name in ('result.json', 'container.json', 'commands.json', 'journal.txt'):
        extra['lock-proof/' + name] = (proof_dir / name).read_bytes()
    for name in ('stage8b_p1f_o2_lock_mount_test.py', 'stage8b_p1f_o2_lock_mount_probe.py'):
        extra['lock-proof/' + name] = (ROOT / 'scripts' / name).read_bytes()
    # Exact ELF smoke uses isolated source export, never the workstation checkout.
    cmd = ['docker', 'run', '--rm', '--network', 'none', '--platform', 'linux/amd64',
           '--mount', f'type=bind,src={build_dir / "target/release"},dst=/payload,readonly',
           '--mount', f'type=bind,src={build_dir / "source/deploy/stage8b-p1e"},dst=/units,readonly',
           '--mount', f'type=bind,src={build_dir / "source/scripts/stage8b_p1f_o2_elf_smoke.sh"},dst=/probe.sh,readonly',
           '--mount', f'type=bind,src={build_dir / "source/docs/stage-8/stage8b-p1f-o2-supervisor-template.json"},dst=/template.json,readonly',
           builder.IMAGE, 'bash', '/probe.sh']
    result = subprocess.run(cmd, capture_output=True)
    log = result.stdout + result.stderr
    (build_dir / 'elf-smoke.log').write_bytes(log)
    require(result.returncode == 0, 'ELF smoke failed; see build/elf-smoke.log')
    extra['elf-smoke.log'] = log
    extra['elf-smoke-command.json'] = (json.dumps(cmd) + '\n').encode()
    generated = {'artifact-evidence/' + name: raw for name, raw in extra.items()}
    descriptor = dict(schema_version=1, domain='moex.o2.terminal-successor-artifact.v1',
                      implementation_ref=source.SOURCE_REF, source_tree=source.SOURCE_TREE,
                      execution_authorized=False, target_mutation_performed=False,
                      extras_sha256={name: sha(raw) for name, raw in generated.items()},
                      qualification_limits=['systemd lock proof: native ARM Linux; amd64 ELF separately tested',
                                            'no production materialization or VPS activation'])
    generated['artifact-evidence/descriptor.json'] = (json.dumps(descriptor, indent=2) + '\n').encode()
    require(not (set(files) & set(generated)), 'source/evidence collision')
    out = ROOT / 'reports/handoff' / NAME
    require(not out.exists(), 'immutable artifact exists')
    with zipfile.ZipFile(out, 'x', zipfile.ZIP_DEFLATED) as z:
        for name, raw in {**files, **generated}.items():
            z.writestr(common.zip_info(name, modes.get(name, '100644')), raw)
    report = check(out)
    Path(str(out) + '.sha256').write_text(report['archive_sha256'] + '  ' + out.name + '\n')
    Path(str(out) + '.safety.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report, indent=2))

if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('action', choices=('package', 'check'))
    parser.add_argument('path', type=Path)
    parser.add_argument('--proof', type=Path)
    parser.add_argument('--review', type=Path)
    args = parser.parse_args()
    if args.action == 'check':
        print(json.dumps(check(args.path), indent=2))
    else:
        package(args.path.resolve(), args.proof.resolve(), args.review.resolve())
