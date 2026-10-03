#!/usr/bin/env python3
"""Sparse O2 release qualification and immutable no-Git handoff. Never installs."""
import argparse
import json
from pathlib import Path, PurePosixPath
import subprocess
import uuid
import zipfile

import make_stage8b_design_handoff as common
import stage8b_p1e_i1a_handoff_safety_check as objects
import stage8b_p1f_alor_finam_source_correction_handoff_safety_check as safety
from stage8b_p1f_o2_no_riskgate_artifact import snapshot, encoded
import stage8b_p1f_o2_sparse_build as source

ROOT = Path(__file__).resolve().parents[1]
P = 'artifact-sparse/'
PROBE = 'scripts/fixtures/stage8b-o2-sparse-release-probe.rs'
SMOKE = 'scripts/stage8b_p1f_o2_sparse_smoke.sh'
REVIEW = 'docs/stage-8/reviews/REVIEW_d63c378_SPARSE_M10_RU.txt'
REVIEW_SHA = '57ad5768fd1f4d49b5ca8e74869bd2e6371a1da4402eb25872dc9290d7e10b8d'
PROFILE = 'imoexf-baseline07-bo-only-no-riskgate-paper-v2'
PROFILE_SHA = '6d7dff3543993b7727a161f85af3037f74762ecd2ce8332e0594b9c92d987c38'
# Fixed, reproducible offline qualification outputs; never operational inputs.
FIXTURE_SHA = {
    "calendar-template-fixture.json": "9ac20febff1e195985178fa2e23d494a46bbd48e8f6d8220535544df3c33daaf",
    "materialization-policy-fixture.json": "df81eb4edd2995aee5c6010de92d425702aa7ead83087f959a48cb4a14296e68",
    "mixed-foreign-policy.json": "c8750a22d5e7df6cd5be9645b0a86e3cfa273e165ec9d8112a558b6fe4ecf478",
    "mixed-legacy-fingerprint.json": "45c46e6e3e9bcb264610a4c96193f9e837ca404869442cc53e5d1ddb490b9987",
    "mixed-materialization-policy.json": "5f455eeb090b064a0c6a7f2852c67bc60103d3f9b1ec62fc3abc346663abe035",
    "mixed-missing-policy.json": "a3fa95dee3eda4c62c1d987783ec1efffbdbd8ace49d98e6b9058145acaea85c",
    "mixed-schema1.json": "190791b441164d6ed3eb4fb0d09311d61c6392901e2c0a1728e8ec8be6f7b6eb",
    "probe-result.json": "25bb9900de71352899b808e36858f779f4a851640943cbb99215d534061027a5",
    "source-v4-fixture.json": "8bf546ee74ceb3e8273aed7ec8dd7d3e26cd212a8aa42e1f3304d9de96a682f3",
    "supervisor-fixture.json": "d4a91dfa4e9e68bcb7af49fd41f22e8ac254e327e9db36259ef506cf89e0ba2a"
}
QUALIFIED = ('commands.json', 'result.json', 'smoke.log', 'probe.rs', 'smoke.sh',
             'probe-result.json', 'supervisor-fixture.json', 'source-v4-fixture.json',
             'materialization-policy-fixture.json', 'mixed-materialization-policy.json',
             'calendar-template-fixture.json', 'mixed-schema1.json', 'mixed-missing-policy.json',
             'mixed-foreign-policy.json', 'mixed-legacy-fingerprint.json')
SMOKE_MARKERS = (
    'PASS sparse exact-elf bootstrap-schema2 config-valid',
    *(f'PASS sparse exact-elf mixed-{n} exit=64' for n in
      ('schema1', 'missing-policy', 'foreign-policy', 'legacy-fingerprint')),
    'PASS sparse exact-elf policy-v3 reaches-account-boundary',
    'PASS sparse exact-elf policy-mismatch rejected',
    'PASS sparse-elf-smoke network=none state=empty activation=false',
)
sha = source.builder.sha


def require(value, message):
    if not value:
        raise ValueError(message)


def qualify(build, output):
    meta = json.loads((build / 'build.json').read_bytes())
    require(meta['implementation_ref'] == source.SOURCE_REF and meta['source_tree'] == source.SOURCE_TREE,
            'build binding')
    for binary in meta['binaries']:
        require(sha((build / 'target/release' / binary['name']).read_bytes()) == binary['sha256'], 'ELF drift')
    output.mkdir(parents=True, exist_ok=False)
    (output / 'probe.rs').write_bytes((ROOT / PROBE).read_bytes())
    (output / 'smoke.sh').write_bytes((ROOT / SMOKE).read_bytes())
    deps = build / 'target/release/deps'
    externs, rlibs = [], {}
    for name in ('runtime_durable_service', 'finam_gateway', 'broker_finam', 'chrono', 'serde_json', 'sha2'):
        matches = list(deps.glob(f'lib{name}-*.rlib'))
        require(len(matches) == 1, f'ambiguous release rlib {name}')
        rlibs[matches[0].name] = sha(matches[0].read_bytes())
        externs += ['--extern', name + '=/deps/' + matches[0].name]
    commands = []
    prefix = ['docker', 'run', '--rm', '--network', 'none', '--platform', 'linux/amd64', '--ulimit', 'core=0']

    def run(args):
        name = 'stage8b-sparse-probe-' + uuid.uuid4().hex[:12]
        command = prefix[:3] + ['--name', name] + prefix[3:] + args
        try:
            done = subprocess.run(command, capture_output=True, timeout=180)
            item = dict(command=command, exit_code=done.returncode, timeout=False,
                        stdout=done.stdout.decode(errors='replace'), stderr=done.stderr.decode(errors='replace'))
        except subprocess.TimeoutExpired as error:
            cleanup = subprocess.run(['docker', 'rm', '-f', name], capture_output=True, timeout=30)
            item = dict(command=command, exit_code=124, timeout=True, cleanup_exit_code=cleanup.returncode,
                        stdout=(error.stdout or b'').decode(errors='replace'),
                        stderr=(error.stderr or b'').decode(errors='replace'))
        commands.append(item)
        (output / 'commands.json').write_bytes(encoded(commands))
        require(item['exit_code'] == 0, 'qualification failed; see commands.json')
        return item['stdout'].encode()

    def mount(src, dst, writable=False):
        return ['--mount', f'type=bind,src={src},dst={dst}' + ('' if writable else ',readonly')]

    run(mount(deps, '/deps') + mount(output, '/proof', True) + [source.IMAGE, 'rustc',
        '--edition=2021', '-D', 'warnings', '/proof/probe.rs', '-L', 'dependency=/deps', *externs,
        '-o', '/proof/probe'])
    run(mount(build / 'source', '/src') + mount(output, '/proof', True) + [source.IMAGE, 'bash', '-ceu',
        'install -d -m 0700 /var/lib/moex-finam-p1-paper/state; /proof/probe'])
    log = run(mount(build / 'target/release', '/payload') +
        mount(build / 'source/deploy/stage8b-p1e', '/units') +
        mount(build / 'source/scripts/stage8b_p1f_o2_elf_smoke.sh', '/legacy-smoke.sh') +
        mount(build / 'source/docs/stage-8/stage8b-p1f-o2-supervisor-template.json', '/template.json') +
        mount(output, '/input') + [source.IMAGE, 'bash', '/input/smoke.sh'])
    (output / 'smoke.log').write_bytes(log)
    require(all(m.encode() in log for m in SMOKE_MARKERS), 'smoke incomplete')
    (output / 'result.json').write_bytes(encoded(dict(result='PASS', compiled_ref=source.SOURCE_REF,
        rlib_sha256=rlibs, probe_executable_sha256=sha((output / 'probe').read_bytes()),
        fixture_only=True, operational_config_issued=False, finam_contact=False, redis_contact=False)))
    print('PASS sparse release-library materialization/V4 and exact-ELF config/policy selection')


def check(path):
    with zipfile.ZipFile(path) as archive:
        infos = archive.infolist()
        require(archive.testzip() is None, 'CRC')
        require(len(infos) == len(set(archive.namelist())), 'duplicates')
        for item in infos:
            safety.validate_member_name(item.filename)
            require(str(PurePosixPath(item.filename)) == item.filename, 'noncanonical path')
            require(item.external_attr >> 16 in (0o100644, 0o100755), 'special/symlink')
        files = {i.filename: archive.read(i) for i in infos}
        modes = {i.filename: f'{i.external_attr >> 16:06o}' for i in infos}
    meta = json.loads(files[P + 'descriptor.json'])
    require(meta['status'] == 'BINARY_ARTIFACT_REVIEW_CANDIDATE_NOT_INSTALLABLE', 'status')
    require(meta['compiled_ref'] == source.SOURCE_REF and meta['compiled_tree'] == source.SOURCE_TREE and
            meta['accepted_source_ref'] == source.ACCEPTED_SOURCE, 'source pins')
    for name in ('installation_authorized', 'execution_authorized', 'target_mutation_performed'):
        require(meta[name] is False, 'operational boundary')
    for name, digest in meta['generated_sha256'].items():
        require(sha(files[name]) == digest, 'generated digest: ' + name)
    manifests = {}
    for label, ref in [('review', meta['review_ref']), ('build', source.SOURCE_REF)]:
        manifest = json.loads(files[P + label + '-manifest.json'])
        entries = manifest['entries']
        require(manifest['source_ref'] == ref and len({e['path'] for e in entries}) == len(entries)
                == manifest['entry_count'], 'source inventory')
        blobs = {}
        for entry in entries:
            name = entry['path']
            preimage = P + 'build-preimages/' + name
            member = preimage if label == 'build' and preimage in files else name
            raw = files[member]
            require(sha(raw) == entry['sha256'] and len(raw) == entry['size'] and
                    modes[member] == entry['mode'], 'source blob: ' + name)
            blobs[name] = raw
        commit = files[P + label + '-commit.raw']
        tree = objects.build_tree_oid(entries, blobs)
        require(objects.git_object_id('commit', commit) == ref and
                commit.splitlines()[0] == f'tree {tree}'.encode(), 'Git binding')
        if label == 'build':
            require(tree == source.SOURCE_TREE and
                    f'parent {source.ACCEPTED_SOURCE}'.encode() in commit.splitlines(), 'build lineage')
        manifests[label] = blobs
    expected_generated = {P + n for n in ('review-manifest.json', 'build-manifest.json',
        'review-commit.raw', 'build-commit.raw', 'build.json', 'build.log')}
    expected_generated |= {P + 'qualification/' + n for n in QUALIFIED}
    expected_generated |= {P + 'payload/' + n for n in source.builder.BINS}
    expected_generated.add('handoff-commit.txt')
    for name, raw in manifests['build'].items():
        if manifests['review'].get(name) != raw:
            expected_generated.add(P + 'build-preimages/' + name)
    require(set(meta['generated_sha256']) == expected_generated, 'evidence inventory')
    require(set(files) == set(manifests['review']) | expected_generated | {P+'descriptor.json'}, 'member inventory')
    for name in set(manifests['build']) | set(manifests['review']):
        if name.startswith(('crates/', '.github/')) or name in ('Cargo.toml', 'Cargo.lock'):
            require(manifests['build'].get(name) == manifests['review'].get(name), 'production/workflow delta')
    require(sha(files[REVIEW]) == REVIEW_SHA, 'source review')
    build = json.loads(files[P+'build.json'])
    require(build['implementation_ref'] == source.SOURCE_REF and build['source_tree'] == source.SOURCE_TREE
            and build['rust_image'] == source.IMAGE and build['cargo_args'] == source.builder.CARGO_ARGS
            and build['network'] == 'none' and build['cargo_offline'] is True
            and build['execution_authorized'] is False and build['target_mutation_performed'] is False, 'build recipe')
    require(build['build_log_sha256'] == sha(files[P+'build.log']) and
            build['source_commit_raw_sha256'] == sha(files[P+'build-commit.raw']) and
            b'Finished `release` profile' in files[P+'build.log'], 'build logs')
    require(len(build['binaries']) == 3 and {b['name'] for b in build['binaries']} == set(source.builder.BINS), 'ELFs')
    for binary in build['binaries']:
        raw = files[P+'payload/'+binary['name']]
        require(len(raw) == binary['size'] and sha(raw) == binary['sha256'] and raw[:6] == b'\x7fELF\x02\x01'
                and int.from_bytes(raw[18:20], 'little') == 62, 'ELF binding')
    q = lambda name: json.loads(files[P+'qualification/'+name])
    for name, digest in FIXTURE_SHA.items():
        require(sha(files[P+'qualification/'+name]) == digest, 'fixed probe output: '+name)
    proof = q('result.json')
    require(proof['result'] == 'PASS' and proof['compiled_ref'] == source.SOURCE_REF and
            proof['fixture_only'] is True and all(proof[k] is False for k in
                ('operational_config_issued', 'finam_contact', 'redis_contact')), 'qualification boundary')
    require(files[P+'qualification/probe.rs'] == files[PROBE] and
            files[P+'qualification/smoke.sh'] == files[SMOKE], 'qualification source')
    commands = q('commands.json')
    require(len(commands) == 3 and all(c['exit_code'] == 0 and c['timeout'] is False for c in commands), 'commands')
    for record in commands:
        c = record['command']
        require(c[:4] == ['docker', 'run', '--rm', '--name'] and c[4].startswith('stage8b-sparse-probe-') and
                c[5:11] == ['--network', 'none', '--platform', 'linux/amd64', '--ulimit', 'core=0'] and
                c.count(source.IMAGE) == 1 and '--privileged' not in c and
                not any('docker.sock' in s for s in c), 'probe isolation')
    require(commands[0]['command'][commands[0]['command'].index(source.IMAGE)+1:][:5] ==
            ['rustc', '--edition=2021', '-D', 'warnings', '/proof/probe.rs'], 'probe compiler')
    require(commands[1]['command'][-4:] == [source.IMAGE, 'bash', '-ceu',
            'install -d -m 0700 /var/lib/moex-finam-p1-paper/state; /proof/probe'], 'probe execution')
    require(commands[2]['command'][-3:] == [source.IMAGE, 'bash', '/input/smoke.sh'], 'ELF execution')
    log = files[P+'qualification/smoke.log']
    require(commands[2]['stdout'].encode() == log and all(m.encode() in log for m in SMOKE_MARKERS), 'smoke markers')
    for binary in build['binaries']:
        require((binary['sha256']+'  /usr/local/libexec/moex/'+binary['name']+'\n').encode() in log, 'smoke ELF hash')
    observed, config, policy, bundle = (q(n) for n in ('probe-result.json', 'supervisor-fixture.json',
        'materialization-policy-fixture.json', 'source-v4-fixture.json'))
    require(json.loads(commands[1]['stdout']) == observed, 'probe output')
    require(observed['materialization_policy_schema'] == policy['schema_version'] == 3 and
            observed['bootstrap_schema'] == config['bootstrap']['schema_version'] == 2 and
            observed['source_schema'] == bundle['schema_version'] == 4, 'sparse schemas')
    require(policy['domain'] == 'stage8b-p1f-o2-materialization-policy-v3' and
            bundle['domain'] == 'moex.stage8b.p1e.first-boot-source-bundle.v4', 'sparse domains')
    require(config['runtime_profile_id'] == policy['runtime_profile_id'] == PROFILE and
            config['runtime_profile_sha256'] == policy['runtime_profile_sha256'] ==
            bundle['runtime_profile_sha256'] == observed['runtime_profile_sha256'] == PROFILE_SHA, 'profile binding')
    require(config['bootstrap']['market_data_policy_sha256'] == policy['market_data_policy_sha256'] ==
            observed['market_data_policy_sha256'] and config['bootstrap']['runtime_config_fingerprint_sha256'] ==
            observed['runtime_fingerprint'], 'config policy/fingerprint')
    require(policy['operational_identity_sha256'] == bundle['operational_identity_sha256'] ==
            observed['operational_identity_sha256'] != observed['strict_operational_identity_sha256'], 'identity')
    require(config['first_boot_source_bundle_sha256'] == observed['source_bundle_sha256'] ==
            sha(files[P+'qualification/source-v4-fixture.json']), 'V4 binding')
    for name, expected in [('history_m10_count',404),('sparse_history_m10_count',18),
                           ('synthetic_candidate_m1_count',7),('riskgate_observation_count',0)]:
        require(type(observed[name]) is int and observed[name] == expected, 'witness count '+name)
    require(observed['strict_identity_rejected'] is True and observed['fixture_only'] is True and
            observed['finam_contact'] is False and observed['redis_contact'] is False, 'witness boundary')
    marker = dict(line.split('=',1) for line in files['handoff-commit.txt'].decode().splitlines())
    require(marker == dict(source_short_ref=meta['review_ref'][:7], source_ref=meta['review_ref'],
            compiled_ref=source.SOURCE_REF, archive_name=path.name), 'handoff marker')
    return dict(archive_sha256=sha(path.read_bytes()), members=len(files), reviewed_ref=meta['review_ref'],
        compiled_ref=source.SOURCE_REF, compiled_tree=source.SOURCE_TREE, git_trees_verified=2,
        binary_count=3, qualification_passed=True, duplicates=0, unsafe_paths=0, symlinks=0,
        special_files=0, crc_passed=True, installation_authorized=False, execution_authorized=False)


def package(build, proof, output):
    require(not source.builder.git('status','--porcelain').strip(), 'commit packaging tree first')
    ref = source.builder.git('rev-parse','HEAD').decode().strip()
    files, modes, manifest = snapshot(ref)
    built, build_modes, bm = snapshot(source.SOURCE_REF)
    extra = {P+'review-manifest.json':encoded(manifest), P+'build-manifest.json':encoded(bm),
        P+'review-commit.raw':source.builder.git('cat-file','commit',ref),
        P+'build-commit.raw':(build/'source-commit.raw').read_bytes(),
        P+'build.json':(build/'build.json').read_bytes(), P+'build.log':(build/'build.log').read_bytes()}
    for name, raw in built.items():
        if files.get(name) != raw or modes.get(name) != build_modes[name]:
            extra[P+'build-preimages/'+name] = raw
            modes[P+'build-preimages/'+name] = build_modes[name]
    for name in source.builder.BINS:
        extra[P+'payload/'+name] = (build/'target/release'/name).read_bytes()
    for name in QUALIFIED:
        extra[P+'qualification/'+name] = (proof/name).read_bytes()
    extra['handoff-commit.txt'] = (f'source_short_ref={ref[:7]}\nsource_ref={ref}\n'
        f'compiled_ref={source.SOURCE_REF}\narchive_name={output.name}\n').encode()
    require(not set(files) & set(extra), 'source/evidence collision')
    extra[P+'descriptor.json'] = encoded(dict(status='BINARY_ARTIFACT_REVIEW_CANDIDATE_NOT_INSTALLABLE',
        review_ref=ref, compiled_ref=source.SOURCE_REF, compiled_tree=source.SOURCE_TREE,
        accepted_source_ref=source.ACCEPTED_SOURCE, installation_authorized=False,
        execution_authorized=False,target_mutation_performed=False,
        generated_sha256={n:sha(b) for n,b in extra.items()}))
    with zipfile.ZipFile(output,'x',zipfile.ZIP_DEFLATED) as z:
        for name,raw in {**files,**extra}.items():
            z.writestr(common.zip_info(name,modes.get(name,'100644')),raw)
    result=check(output)
    Path(str(output)+'.sha256').write_text(result['archive_sha256']+'  '+output.name+'\n')
    Path(str(output)+'.safety.json').write_bytes(encoded(result))
    print(encoded(result).decode())


if __name__ == '__main__':
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('action',choices=('qualify','package','check'))
    p.add_argument('path',type=Path)
    p.add_argument('--build',type=Path)
    p.add_argument('--proof',type=Path)
    a=p.parse_args()
    if a.action=='check':
        print(encoded(check(a.path.resolve())).decode())
    elif a.action=='qualify':
        qualify(a.build.resolve(),a.path.resolve())
    else:
        package(a.build.resolve(),a.proof.resolve(),a.path.resolve())
