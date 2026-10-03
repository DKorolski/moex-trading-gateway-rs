#!/usr/bin/env python3
"""Offline O2 binary artifact and no-Git verification, NOT an installation package."""
import argparse
import copy
import io
import json
from pathlib import Path, PurePosixPath
import re
import subprocess
import tarfile
import uuid
import zipfile

import make_stage8b_design_handoff as common
import stage8b_p1e_i1a_handoff_safety_check as objects
import stage8b_p1f_o2_artifact_handoff_safety_check as paths
import stage8b_p1f_o2_no_riskgate_build as source

ROOT = Path(__file__).resolve().parents[1]
P = 'artifact-no-riskgate/'
REVIEW_NAME = 'FINAM_8e7a647_CALENDAR_RANGE_REVIEW_2026-10-01.md'
REVIEW_SHA = 'a4f025e8b084f494e2b85c243d79ca765fcaa98d044332d647972dd97f651d25'
PROFILE = 'imoexf-baseline07-bo-only-no-riskgate-paper-v2'
PROFILE_SHA = '6d7dff3543993b7727a161f85af3037f74762ecd2ce8332e0594b9c92d987c38'
sha = source.builder.sha
require = paths.require


def encoded(value):
    return (json.dumps(value, sort_keys=True, indent=2) + '\n').encode()


def snapshot(ref):
    """Export blobs from Git, never incidental checkout files or credentials."""
    files, modes = {}, {}
    raw = source.builder.git('archive', '--format=tar', ref)
    with tarfile.open(fileobj=io.BytesIO(raw)) as archive:
        for member in archive:
            if member.isdir():
                continue
            require(member.isfile(), 'nonregular Git archive entry')
            files[member.name] = archive.extractfile(member).read()
            modes[member.name] = '100755' if member.mode & 0o111 else '100644'
    entries = [dict(path=p, mode=modes[p], size=len(raw), sha256=sha(raw))
               for p, raw in sorted(files.items())]
    # The raw Git tree reconstruction below also rejects hidden gitlinks.
    return files, modes, dict(source_ref=ref, entries=entries, entry_count=len(entries))


def qualify(build, output):
    evidence = json.loads((build / 'build.json').read_bytes())
    require(evidence['implementation_ref'] == source.SOURCE_REF and
            evidence['source_tree'] == source.SOURCE_TREE and
            evidence['rust_image'] == source.IMAGE, 'unexpected build')
    for binary in evidence['binaries']:
        require(sha((build / 'target/release' / binary['name']).read_bytes()) == binary['sha256'], 'ELF drift')
    output.mkdir(parents=True, exist_ok=False)
    commands = []

    def run(command, timeout=120):
        name = 'stage8b-nrg-probe-' + uuid.uuid4().hex[:12]
        command = command[:3] + ['--name', name] + command[3:]
        try:
            result = subprocess.run(command, capture_output=True, timeout=timeout)
            item = dict(command=command, exit_code=result.returncode, timeout=False,
                        stdout=result.stdout.decode(errors='replace'), stderr=result.stderr.decode(errors='replace'))
        except subprocess.TimeoutExpired as error:
            cleanup = subprocess.run(['docker', 'rm', '-f', name], capture_output=True, timeout=30)
            item = dict(command=command, exit_code=124, timeout=True,
                        stdout=(error.stdout or b'').decode(errors='replace'),
                        stderr=(error.stderr or b'').decode(errors='replace'),
                        cleanup_exit_code=cleanup.returncode)
        commands.append(item)
        (output / 'commands.json').write_bytes(encoded(commands))
        require(item['exit_code'] == 0, 'qualification failed; retained commands.json')
        return item['stdout'].encode()

    docker = ['docker', 'run', '--rm', '--network', 'none', '--platform', 'linux/amd64', '--ulimit', 'core=0']
    rlibs = list((build / 'target/release/deps').glob('libruntime_durable_service-*.rlib'))
    require(len(rlibs) == 1, 'ambiguous release rlib')
    probe = ROOT / 'scripts/fixtures/stage8b-o2-no-riskgate-profile-probe.rs'
    (output / 'profile-probe.rs').write_bytes(probe.read_bytes())
    run(docker + ['--mount', f'type=bind,src={build / "target/release/deps"},dst=/deps,readonly',
                 '--mount', f'type=bind,src={output},dst=/proof', source.IMAGE,
                 'rustc', '--edition=2021', '/proof/profile-probe.rs', '-L', 'dependency=/deps',
                 '--extern', 'runtime_durable_service=/deps/' + rlibs[0].name,
                 '-o', '/proof/profile-probe'])
    fingerprints = run(docker + ['--mount', f'type=bind,src={output},dst=/proof,readonly',
                                source.IMAGE, '/proof/profile-probe'])
    (output / 'fingerprints.txt').write_bytes(fingerprints)
    fp = dict(line.split('=', 1) for line in fingerprints.decode().splitlines())
    require(set(fp) == {'legacy', 'no_riskgate'} and fp['legacy'] != fp['no_riskgate'] and
            all(re.fullmatch('[0-9a-f]{64}', v) for v in fp.values()), 'profile probe')
    legacy = json.loads((build / 'source/docs/stage-8/stage8b-p1f-o2-supervisor-template.json').read_bytes())
    require(legacy['bootstrap']['runtime_config_fingerprint_sha256'] == fp['legacy'], 'legacy fingerprint')
    current = copy.deepcopy(legacy)
    current.update(runtime_profile_id=PROFILE, runtime_profile_sha256=PROFILE_SHA)
    current['bootstrap']['runtime_config_fingerprint_sha256'] = fp['no_riskgate']
    (output / 'supervisor-fixture.json').write_bytes(encoded(current))
    for field in ('runtime_profile_sha256', 'runtime_config_fingerprint_sha256'):
        mixed = copy.deepcopy(current)
        if field == 'runtime_profile_sha256':
            mixed[field] = legacy[field]
        else:
            mixed['bootstrap'][field] = fp['legacy']
        (output / ('mixed-' + field + '.json')).write_bytes(encoded(mixed))
    smoke = ROOT / 'scripts/stage8b_p1f_o2_no_riskgate_smoke.sh'
    (output / 'smoke.sh').write_bytes(smoke.read_bytes())
    log = run(docker + [
        '--mount', f'type=bind,src={build / "target/release"},dst=/payload,readonly',
        '--mount', f'type=bind,src={build / "source/deploy/stage8b-p1e"},dst=/units,readonly',
        '--mount', f'type=bind,src={build / "source/scripts/stage8b_p1f_o2_elf_smoke.sh"},dst=/legacy-smoke.sh,readonly',
        '--mount', f'type=bind,src={build / "source/docs/stage-8/stage8b-p1f-o2-supervisor-template.json"},dst=/template.json,readonly',
        '--mount', f'type=bind,src={output},dst=/input,readonly', source.IMAGE, 'bash', '/input/smoke.sh'])
    (output / 'smoke.log').write_bytes(log)
    require(b'PASS no-riskgate-elf-smoke network=none state=empty activation=false' in log, 'smoke marker')
    (output / 'result.json').write_bytes(encoded(dict(result='PASS', compiled_ref=source.SOURCE_REF,
        fixture_only=True, operational_config_issued=False, finam_contact=False, redis_contact=False,
        runtime_fingerprint=fp['no_riskgate'], runtime_profile_sha256=PROFILE_SHA)))
    print('PASS exact-ELF legacy and no-riskgate config admission; no activation')


def check(path):
    with zipfile.ZipFile(path) as z:
        infos = z.infolist()
        require(z.testzip() is None and len(infos) == len(set(z.namelist())), 'CRC/duplicate')
        for i in infos:
            paths.validate_name(i.filename)
            require(str(PurePosixPath(i.filename)) == i.filename, 'noncanonical path')
            require(i.external_attr >> 16 in (0o100644, 0o100755), 'nonregular member')
        files = {i.filename: z.read(i) for i in infos}
        modes = {i.filename: f'{i.external_attr >> 16:06o}' for i in infos}
    meta = json.loads(files[P + 'descriptor.json'])
    require(meta['status'] == 'BINARY_ARTIFACT_REVIEW_CANDIDATE_NOT_INSTALLABLE', 'status')
    for field in ('installation_authorized', 'execution_authorized', 'target_mutation_performed'):
        require(meta[field] is False, 'operational boundary')
    require(meta['compiled_ref'] == source.SOURCE_REF and meta['compiled_tree'] == source.SOURCE_TREE, 'build ref')
    require(meta['review_ref'] != source.SOURCE_REF, 'packaging/build ref conflation')
    for name, digest in meta['generated_sha256'].items():
        require(sha(files[name]) == digest, 'generated digest ' + name)
    manifests = {}
    for label, ref in [('review', meta['review_ref']), ('build', source.SOURCE_REF)]:
        manifest = json.loads(files[P + label + '-manifest.json'])
        require(manifest['source_ref'] == ref, 'manifest ref')
        entries = manifest['entries']
        blobs = {}
        require(len({e['path'] for e in entries}) == len(entries) == manifest['entry_count'], 'manifest duplicates')
        for e in entries:
            name = e['path']
            key = P + 'build-preimages/' + name
            member = key if label == 'build' and key in files else name
            raw = files[member]
            require(sha(raw) == e['sha256'] and len(raw) == e['size'] and modes[member] == e['mode'], 'source digest')
            blobs[name] = raw
        commit = files[P + label + '-commit.raw']
        tree = objects.build_tree_oid(entries, blobs)
        require(objects.git_object_id('commit', commit) == ref and commit.splitlines()[0] == f'tree {tree}'.encode(), 'Git binding')
        if label == 'build':
            require(tree == source.SOURCE_TREE, 'build tree')
        manifests[label] = blobs
    require(set(files) == set(manifests['review']) | set(meta['generated_sha256']) | {P + 'descriptor.json'}, 'member inventory')
    # Packaging may add helpers/docs; never silently rebuild from modified Rust/Cargo.
    for name in set(manifests['build']) | set(manifests['review']):
        if name.startswith('crates/') or name in ('Cargo.toml', 'Cargo.lock') or name.startswith('.github/'):
            require(manifests['build'].get(name) == manifests['review'].get(name), 'production/workflow delta')
    build = json.loads(files[P + 'build.json'])
    require(build['implementation_ref'] == source.SOURCE_REF and build['source_tree'] == source.SOURCE_TREE and
            build['rust_image'] == source.IMAGE and build['cargo_args'] == source.builder.CARGO_ARGS and
            build['network'] == 'none' and build['cargo_offline'] is True and
            build['execution_authorized'] is False and build['target_mutation_performed'] is False, 'build recipe')
    require(build['build_log_sha256'] == sha(files[P + 'build.log']) and
            build['source_commit_raw_sha256'] == sha(files[P + 'build-commit.raw']), 'build log binding')
    require(b'Finished `release` profile' in files[P + 'build.log'], 'release build incomplete')
    require(len(build['binaries']) == 3 and {b['name'] for b in build['binaries']} == set(source.builder.BINS), 'three ELFs')
    for b in build['binaries']:
        raw = files[P + 'payload/' + b['name']]
        require(sha(raw) == b['sha256'] and len(raw) == b['size'] and raw[:6] == b'\x7fELF\x02\x01' and
                int.from_bytes(raw[18:20], 'little') == 62, 'ELF identity')
    require(sha(files[P + 'review/' + REVIEW_NAME]) == REVIEW_SHA, 'accepted review pin')
    proof = json.loads(files[P + 'qualification/result.json'])
    require(proof['result'] == 'PASS' and proof['compiled_ref'] == source.SOURCE_REF and
            proof['fixture_only'] is True and proof['operational_config_issued'] is False and
            proof['finam_contact'] is False and proof['redis_contact'] is False and
            proof['runtime_profile_sha256'] == PROFILE_SHA, 'qualification boundary')
    commands = json.loads(files[P + 'qualification/commands.json'])
    require(len(commands) == 3 and all(c['exit_code'] == 0 for c in commands), 'qualification commands')
    isolated_prefix = ['docker', 'run', '--rm', '--network', 'none', '--platform', 'linux/amd64', '--ulimit', 'core=0']
    for item in commands:
        original = item['command']
        require(original[3] == '--name' and original[4].startswith('stage8b-nrg-probe-') and
                item['timeout'] is False, 'probe lifecycle')
        command = original[:3] + original[5:]
        require(command[:len(isolated_prefix)] == isolated_prefix and command.count(source.IMAGE) == 1 and
                '--privileged' not in command and not any('docker.sock' in part for part in command), 'probe isolation')
    log = files[P + 'qualification/smoke.log']
    for binary in build['binaries']:
        require((binary['sha256'] + '  /usr/local/libexec/moex/' + binary['name'] + '\n').encode() in log,
                'smoke payload digest')
    require(commands[-1]['stdout'].encode() == log and
            b'PASS no-riskgate-elf-smoke network=none state=empty activation=false' in log and
            b'PASS no-riskgate exact-elf mixed-runtime_profile_sha256 exit=64' in log and
            b'PASS no-riskgate exact-elf mixed-runtime_config_fingerprint_sha256 exit=64' in log, 'ELF qualification')
    fixture = json.loads(files[P + 'qualification/supervisor-fixture.json'])
    require(fixture['runtime_profile_id'] == PROFILE and fixture['runtime_profile_sha256'] == PROFILE_SHA and
            fixture['bootstrap']['runtime_config_fingerprint_sha256'] == proof['runtime_fingerprint'], 'fixture profile')
    fingerprints = dict(line.split('=', 1) for line in files[P + 'qualification/fingerprints.txt'].decode().splitlines())
    require(fingerprints['no_riskgate'] == proof['runtime_fingerprint'] and
            fingerprints['legacy'] != fingerprints['no_riskgate'] and
            commands[1]['stdout'].encode() == files[P + 'qualification/fingerprints.txt'], 'probe fingerprint binding')
    require(files[P + 'qualification/profile-probe.rs'] == manifests['review']['scripts/fixtures/stage8b-o2-no-riskgate-profile-probe.rs'] and
            files[P + 'qualification/smoke.sh'] == manifests['review']['scripts/stage8b_p1f_o2_no_riskgate_smoke.sh'], 'probe source')
    marker = dict(line.split('=', 1) for line in files['handoff-commit.txt'].decode().splitlines())
    require(marker == dict(source_short_ref=meta['review_ref'][:7], source_ref=meta['review_ref'],
                           compiled_ref=source.SOURCE_REF, archive_name=path.name), 'handoff descriptor')
    return dict(archive_sha256=sha(path.read_bytes()), members=len(files), reviewed_ref=meta['review_ref'],
        compiled_ref=source.SOURCE_REF, compiled_tree=source.SOURCE_TREE, git_trees_verified=2,
        duplicates=0, unsafe_paths=0, symlinks=0, special_files=0, crc_passed=True,
        binary_count=3, qualification_passed=True, installation_authorized=False, execution_authorized=False)


def package(build, proof, review, failure_log, output):
    require(not source.builder.git('status', '--porcelain').strip(), 'commit packaging tree first')
    ref = source.builder.git('rev-parse', 'HEAD').decode().strip()
    files, modes, manifest = snapshot(ref)
    built, build_modes, build_manifest = snapshot(source.SOURCE_REF)
    extra = {P + 'review-manifest.json': encoded(manifest), P + 'build-manifest.json': encoded(build_manifest),
        P + 'review-commit.raw': source.builder.git('cat-file', 'commit', ref),
        P + 'build-commit.raw': (build / 'source-commit.raw').read_bytes(),
        P + 'build.json': (build / 'build.json').read_bytes(), P + 'build.log': (build / 'build.log').read_bytes(),
        P + 'prior-toolchain-failure.log': failure_log.read_bytes(),
        P + 'review/' + REVIEW_NAME: review.read_bytes()}
    for name, raw in built.items():
        if files.get(name) != raw or modes.get(name) != build_modes[name]:
            member = P + 'build-preimages/' + name
            extra[member] = raw
            modes[member] = build_modes[name]
    for name in source.builder.BINS:
        extra[P + 'payload/' + name] = (build / 'target/release' / name).read_bytes()
    for name in ('commands.json', 'result.json', 'smoke.log', 'fingerprints.txt', 'profile-probe.rs', 'smoke.sh',
                 'supervisor-fixture.json', 'mixed-runtime_profile_sha256.json', 'mixed-runtime_config_fingerprint_sha256.json'):
        extra[P + 'qualification/' + name] = (proof / name).read_bytes()
    extra['handoff-commit.txt'] = f'source_short_ref={ref[:7]}\nsource_ref={ref}\ncompiled_ref={source.SOURCE_REF}\narchive_name={output.name}\n'.encode()
    require(not set(files) & set(extra), 'source/evidence collision')
    descriptor = dict(status='BINARY_ARTIFACT_REVIEW_CANDIDATE_NOT_INSTALLABLE',
        review_ref=ref, compiled_ref=source.SOURCE_REF, compiled_tree=source.SOURCE_TREE,
        installation_authorized=False, execution_authorized=False, target_mutation_performed=False,
        generated_sha256={name: sha(raw) for name, raw in extra.items()})
    extra[P + 'descriptor.json'] = encoded(descriptor)
    with zipfile.ZipFile(output, 'x', zipfile.ZIP_DEFLATED) as z:
        for name, raw in {**files, **extra}.items():
            z.writestr(common.zip_info(name, modes.get(name, '100644')), raw)
    report = check(output)
    Path(str(output) + '.sha256').write_text(report['archive_sha256'] + '  ' + output.name + '\n')
    Path(str(output) + '.safety.json').write_bytes(encoded(report))
    print(encoded(report).decode())


if __name__ == '__main__':
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('action', choices=('qualify', 'package', 'check'))
    p.add_argument('path', type=Path)
    p.add_argument('--build', type=Path)
    p.add_argument('--proof', type=Path)
    p.add_argument('--review', type=Path)
    p.add_argument('--failure-log', type=Path)
    a = p.parse_args()
    if a.action == 'check':
        print(encoded(check(a.path.resolve())).decode())
    elif a.action == 'qualify':
        qualify(a.build.resolve(), a.path.resolve())
    else:
        package(a.build.resolve(), a.proof.resolve(), a.review.resolve(), a.failure_log.resolve(), a.path.resolve())
