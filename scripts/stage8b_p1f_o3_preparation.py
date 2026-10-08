#!/usr/bin/env python3
"""Local O3 readiness evidence only. No SSH, operational signing or deployment.

Uses existing exact-test harnesses and a disposable local Redis. The open
composition gaps are expected findings, not a runnable O3 approval.
"""
import argparse
import datetime as dt
import hashlib
import io
import json
import os
from pathlib import Path, PurePosixPath
import re
import subprocess
import sys
import zipfile

ROOT = Path(__file__).resolve().parents[1]
SPEC = 'docs/stage-8/stage8b-p1f-o3-preparation.json'
TESTS = [
    ('finam-gateway', 'stage8b_p1f_fixed_producers::tests::ie_linked_o2_fixed_producers_supervised_runtime_truth_and_readmission'),
    ('finam-gateway', 'stage8b_p1f_fixed_producers::tests::observed_linked::observed_real_history_fixed_producer_paper_truth_xack_restart'),
    ('runtime-durable-service', 'stage8b_p1_supervisor::tests::observed_window::observed_supervisor_published_window_is_atomic_and_exact'),
    ('runtime-durable-service', 'stage8b_p1e_process::tests::production_v5_bootstrap_runs_continuous_market_lifecycle_and_readmits_exactly'),
    ('finam-gateway', 'stage8b_p1f_fixed_producers::tests::fixed_o3_o4_schedule_uses_one_retained_sequence_and_revision'),
    ('finam-gateway', 'stage8b_p1f_fixed_producers::tests::id_linked_real_redis_response_loss_restarts_prepared_without_duplicate'),
    ('runtime-durable-service', 'stage8b_p1f_guardian::tests::local_supervision_starts_after_admission_and_stops_on_sigterm'),
    ('runtime-durable-service', 'stage8b_p1e_process::tests::os_process_sigkill_then_restart_preserves_single_ready_owner'),
    ('runtime-durable-service', 'stage8b_p1_semantic::redis::tests::p1c_journal_ahead_reclaims_real_pel_before_reconstructing_s1'),
    ('finam-gateway', 'stage8b_p1f_fixed_producers::tests::o3_to_o4_m10_continuity_rejects_reset_stale_and_conflict'),
    ('runtime-durable-service', 'stage8b_p1_semantic::redis::tests::p1d4_initial_dispatch_only_frontiers_reuse_one_dispatch_and_xack_last'),
    ('runtime-durable-service', 'stage8b_p1f_fixed_redis::tests::resource_probe_uses_real_bounded_redis_reads_and_hash_only_audit'),
]


def require(ok, message):
    if not ok:
        raise RuntimeError(message)


def sha(data):
    return hashlib.sha256(data).hexdigest()


def encode(value):
    return (json.dumps(value, indent=2, sort_keys=True) + '\n').encode()


def put(path, data):
    with path.open('xb') as stream:
        stream.write(data)


def run(directory, label, command):
    start = dt.datetime.now(dt.timezone.utc).isoformat()
    environment = dict(os.environ, RUST_MIN_STACK='33554432', CARGO_NET_OFFLINE='true')
    result = subprocess.run(command, cwd=ROOT, env=environment, stdout=subprocess.PIPE,
                            stderr=subprocess.STDOUT, timeout=1200)
    put(directory/(label+'.log'), result.stdout)
    put(directory/(label+'.json'), encode(dict(command=command, started_at_utc=start,
        finished_at_utc=dt.datetime.now(dt.timezone.utc).isoformat(), exit_code=result.returncode,
        output_sha256=sha(result.stdout), timeout=False)))
    print(f'{label}: exit={result.returncode}', flush=True)
    require(result.returncode == 0, f'{label} failed; exact log retained')
    return result.stdout


def inputs(handoff):
    spec = json.loads((ROOT/SPEC).read_bytes())
    for field in ('operational_authorization_issued','operationally_ready','phase_manifest_signed',
                  'target_mutation_performed','finam_order_writes_allowed'):
        require(spec[field] is False, field)
    require(not spec['retained_root']['rebootstrap_allowed'], 'second bootstrap forbidden')
    review = ROOT/'docs/stage-8/reviews/REVIEW_2d67937_O2_BOUNDED_20261008_2004_RU.txt'
    require(sha(review.read_bytes()) == spec['accepted_o2_review_sha256'], 'review bytes')
    for kind in ('accepted_o2', 'accepted_installation'):
        path = handoff/spec[kind+'_archive']
        require(sha(path.read_bytes()) == spec[kind+'_archive_sha256'], kind+' archive')
    with zipfile.ZipFile(handoff/spec['accepted_o2_archive']) as z:
        require(z.testzip() is None, 'O2 CRC')
        final = json.loads(z.read('evidence/final-readonly.stdout'))
        require(final['head']['state'] == 'COMPLETED' and final['head']['latest_sequence'] == 26, 'O2 head')
        require(final['installed_manifest_sha256'] == spec['installed_manifest_sha256'], 'installation binding')
    return spec


def gate(out, handoff):
    spec = inputs(handoff)
    out.mkdir(parents=True, exist_ok=False)
    # These immutable checks reproduce the *absence* of required composition.
    with zipfile.ZipFile(handoff/spec['accepted_installation_archive']) as z:
        raw = z.read('installation-replay/payload/installation-o2-v1.json')
        require(sha(raw) == spec['installed_manifest_sha256'], 'payload manifest')
        manifest = json.loads(raw)
        put(out/'accepted-installation-manifest.json', raw)
    payload = sorted(manifest['payload'])
    require(not any('local-supervisor' in p for p in payload), 'inventory changed; reassess O3-G1')
    process = (ROOT/'crates/runtime-durable-service/src/stage8b_p1e_process.rs').read_text()
    needle = 'execute_stage8b_p1e_observed_run_v1('
    hits = []
    for path in sorted((ROOT/'crates').rglob('*.rs')):
        for number, line in enumerate(path.read_text().splitlines(), 1):
            if needle in line:
                hits.append(dict(path=str(path.relative_to(ROOT)), line=number, text=line.strip()))
    require(len(hits) == 1 and hits[0]['text'].startswith('pub async fn'), 'observed call graph changed')
    require('execute_run_with_observed_window(supervisor, trusted_now, None).await' in process, 'ordinary run changed')
    metadata = json.loads(run(out, '01-cargo-targets', ['cargo','metadata','--offline','--no-deps','--format-version=1']))
    binaries = sorted((p['name'], t['name']) for p in metadata['packages'] for t in p['targets'] if 'bin' in t['kind'])
    require(('runtime-durable-service', 'stage8b-p1f-local-supervisor') in binaries, 'supervisor source missing')
    evidence = dict(kind='SOURCE_AND_ACCEPTED_INVENTORY_READINESS_NOT_TARGET_OBSERVATION',
        installed_payload_paths=payload, observed_entrypoint_occurrences=hits, cargo_binaries=binaries,
        open_gaps=[g['id'] for g in spec['gaps']], operational_ready=False)
    put(out/'readiness.json', encode(evidence))
    protected = run(out, '02-protected-diff', ['git','diff',spec['source_baseline'],'--','crates','Cargo.toml','Cargo.lock','deploy','.github'])
    require(protected == b'', 'preparation may not alter production or deployment')
    run(out, '03-current-authority', [sys.executable,'-B','scripts/current_tree_authority_check.py'])
    run(out, '04-diff-check', ['git','diff','--check'])
    for index, (package, selector) in enumerate(TESTS, 10):
        run(out, str(index)+'-witness', ['bash','scripts/stage8b_p1f_ie_exact_test.sh',package,selector])
    names = subprocess.check_output(['git','ls-files','--cached','--others','--exclude-standard'],cwd=ROOT).decode().splitlines()
    put(out/'checked-source.json', encode({name:sha((ROOT/name).read_bytes()) for name in sorted(set(names))}))
    put(out/'result.json', encode(dict(status='PREPARATION_CHECKS_PASS', operational_o3='NOT_RUN',
        local_exact_witnesses=len(TESTS), protected_changes=0, open_composition_gaps=3,
        source_baseline=spec['source_baseline'], scope='local tests and readiness only',
        generated_at_utc=dt.datetime.now(dt.timezone.utc).isoformat())))


def verify(path):
    with zipfile.ZipFile(path) as z:
        require(z.testzip() is None, 'CRC')
        names = z.namelist()
        require(len(names) == len(set(names)), 'duplicate members')
        for info in z.infolist():
            p = PurePosixPath(info.filename)
            require(not p.is_absolute() and '..' not in p.parts and '\\' not in info.filename, 'unsafe path')
            require(info.external_attr >> 16 in (0o100644, 0o100755), 'unsafe mode')
        files = {n:z.read(n) for n in names}
    index = json.loads(files['handoff-evidence/index.json'])
    require(set(index)|{'handoff-evidence/index.json'} == set(files), 'index inventory')
    require(all(sha(files[n])==h for n,h in index.items()), 'index digest')
    result = json.loads(files['handoff-evidence/local/result.json'])
    require(result['status']=='PREPARATION_CHECKS_PASS' and result['operational_o3']=='NOT_RUN', 'verdict')
    require(result['local_exact_witnesses']==len(TESTS), 'witness count')
    for index, (package, selector) in enumerate(TESTS,10):
        stem=f'handoff-evidence/local/{index}-witness'
        command=json.loads(files[stem+'.json'])['command']
        require(command==['bash','scripts/stage8b_p1f_ie_exact_test.sh',package,selector], 'exact selector')
        marker=f'PASS stage8b-p1f-ie-exact-test package={package} selector={selector} selected=1 passed=1 failed=0 ignored=0'
        require(marker.encode() in files[stem+'.log'], 'exact-test result marker')
    for name, raw in files.items():
        if re.fullmatch(r'handoff-evidence/local/\d+[-\w]*\.json',name):
            record=json.loads(raw)
            require(record['exit_code']==0 and record['timeout'] is False, 'command failure')
            require(sha(files[name[:-5]+'.log'])==record['output_sha256'], 'command log')
    spec=json.loads(files[SPEC])
    require(sha(files['handoff-evidence/'+spec['accepted_o2_archive']])==spec['accepted_o2_archive_sha256'], 'O2 lineage')
    source_index=json.loads(files['handoff-evidence/source-tree.json'])
    require(all(sha(files[n])==h for n,h in source_index['files'].items()), 'source index')
    checked=json.loads(files['handoff-evidence/local/checked-source.json'])
    require(checked==source_index['files'], 'gate/source tree drift')
    require(spec['operational_authorization_issued'] is False and spec['operationally_ready'] is False, 'activation')
    return dict(archive=path.name,sha256=sha(path.read_bytes()),bytes=path.stat().st_size,
        source_ref=source_index['source_ref'],members=len(files),duplicates=0,unsafe_paths=0,symlinks=0,
        integrity='PASS',status='PREPARATION_REVIEW_CANDIDATE',operational_o3='NOT_RUN',local_exact_witnesses=len(TESTS))


def package(out, evidence, handoff):
    spec=inputs(handoff)
    require(subprocess.check_output(['git','status','--porcelain'],cwd=ROOT)==b'', 'commit preparation first')
    commit=subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT).decode().strip()
    source=subprocess.check_output(['git','archive','--format=zip',commit],cwd=ROOT)
    with zipfile.ZipFile(io.BytesIO(source)) as z:
        files={i.filename:z.read(i) for i in z.infolist() if not i.is_dir()}
        modes={i.filename:i.external_attr >> 16 for i in z.infolist() if not i.is_dir()}
    # The tracked tree is immutable; extra evidence is separately indexed.
    tree_index={n:sha(b) for n,b in files.items()}
    for path in sorted(evidence.iterdir()):
        require(path.is_file() and not path.is_symlink(), 'unsafe evidence')
        files['handoff-evidence/local/'+path.name]=path.read_bytes()
    files['handoff-evidence/source-tree.json']=encode(dict(source_ref=commit,files=tree_index))
    files['handoff-evidence/'+spec['accepted_o2_archive']]=(handoff/spec['accepted_o2_archive']).read_bytes()
    # Avoid replacing any tracked handoff-commit.txt; this descriptor is distinct.
    files['handoff-evidence/o3-handoff.json']=encode(dict(source_ref=commit,baseline=spec['source_baseline'],
        branch=subprocess.check_output(['git','branch','--show-current'],cwd=ROOT).decode().strip(),
        authorization='NOT_ISSUED', operational_o3='NOT_RUN', production_diff='NONE'))
    require('handoff-commit.txt' not in files, 'do not replace tracked provenance')
    files['handoff-commit.txt']=(f'source_ref={commit}\nshort_sha={commit[:7]}\narchive_name=moex-trading-project-{commit[:7]}-o3-preparation.zip\nkind=O3_PREPARATION_NOT_EXECUTION\n').encode()
    for name, raw in files.items():
        p=PurePosixPath(name)
        require(p.name not in ('.env','auth.private.json') and '.git' not in p.parts and 'target' not in p.parts, 'unsafe member')
        require(not re.search(rb'-----BEGIN (?:OPENSSH |RSA |EC )?PRIVATE KEY-----',raw), 'private key')
    files['handoff-evidence/index.json']=encode({n:sha(b) for n,b in files.items()})
    out.mkdir(parents=True,exist_ok=True)
    path=out/f'moex-trading-project-{commit[:7]}-o3-preparation.zip'
    with zipfile.ZipFile(path,'x',zipfile.ZIP_DEFLATED,compresslevel=9) as z:
        for name,raw in sorted(files.items()):
            info=zipfile.ZipInfo(name);info.create_system=3
            mode=modes.get(name,0o100644)
            require(mode in (0o100644,0o100755), 'source symlink/special file')
            info.external_attr=mode<<16;info.compress_type=zipfile.ZIP_DEFLATED
            z.writestr(info,raw)
    report=verify(path)
    put(Path(str(path)+'.sha256'),(report['sha256']+'  '+path.name+'\n').encode())
    put(Path(str(path)+'.safety.json'),encode(report))
    print(json.dumps(report,indent=2))


if __name__=='__main__':
    parser=argparse.ArgumentParser()
    parser.add_argument('mode',choices=('gate','package','verify'))
    parser.add_argument('path',type=Path)
    parser.add_argument('--handoff',type=Path)
    parser.add_argument('--evidence',type=Path)
    args=parser.parse_args()
    if args.mode=='verify':
        print(json.dumps(verify(args.path),indent=2))
    else:
        require(args.handoff is not None, '--handoff required')
        if args.mode=='gate': gate(args.path,args.handoff)
        else:
            require(args.evidence is not None, '--evidence required')
            package(args.path,args.evidence,args.handoff)
