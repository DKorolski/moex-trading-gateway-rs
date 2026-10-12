#!/usr/bin/env python3
"""One offline readmission release ZIP. Never builds, signs, installs or starts O3.

Checks retained observations; not a remote attestation or operational permit.
Historical fb1ceb3 packaging/verifiers remain unchanged.
"""
import argparse
import copy
import datetime as dt
import json
import os
from pathlib import Path
import stat
import subprocess
import zipfile

import stage8b_p1f_o3_release_handoff as previous
import stage8b_p1f_o3_readmission_install as install
import stage8b_p1f_o3_readmission_qualify as qual
import stage8b_p1f_o3_release_native as builder

h, r = previous.h, previous.r
sha, encode, require = h.sha, h.encode, h.require
ROOT = Path(__file__).resolve().parents[1]
P = 'o3-readmission/'
HELPER_SOURCE = 'crates/finam-gateway/examples/o3_local_signer_witness.rs'
QUALIFIED_REF = '45a3f1b900aa19f9b78e2d98069d2bf030e2298e'
QUALIFIED_SPEC = 'f19ff3a430c4e49b697a50005333cb16f7ecaeaed16557b370af340ce2dc175b'
QUALIFIED_INSTALLER = '01a340eec5b406dba0ab390da4b38612fad28c759c9a89f823996f6581250785'
QUALIFIED_RESULT = '59ac5446806d22a56648ad3b5057e1588f937c05e01c927b37f4833af1ac5533'


def verify_installation_continuity(read, files, baseline, spec):
    """Retain the actual qualified spec; allow only installer-derived metadata drift."""
    prefix = P+'qualified-installation/'
    raw = read(prefix+'spec.json')
    require(sha(raw) == QUALIFIED_SPEC, 'original qualified spec')
    old = json.loads(raw)
    require(sha(read(prefix+'installer.py')) == old['installer_sha256'] == QUALIFIED_INSTALLER,
            'original qualified installer')
    require(sha(read(P+'native/result.json')) == QUALIFIED_RESULT, 'original native result bytes')
    require(read(prefix+'baseline.json') == install.canonical(baseline)
            and sha(install.canonical(baseline)) == install.BASELINE, 'unchanged qualified baseline')
    own = sha(files['scripts/stage8b_p1f_o3_readmission_install.py'])
    manifests = {}
    for n in (install.O3,install.c.MANIFEST):
        raw = read(prefix+'payload/'+Path(n).name)
        require(install.prior.metadata(n,raw) == {k:v for k,v in old['updates'][n].items() if k != 'old_sha256'},
                'original qualified manifest')
        value = json.loads(raw); value['installer_sha256'] = own
        if n == install.c.MANIFEST:
            value['payload'][install.O3] = install.prior.metadata(install.O3,manifests[install.O3])
        manifests[n] = install.canonical(value)
        require(read(P+'installation/payload/'+Path(n).name) == manifests[n], 'installer-only manifest correction')
    expected = copy.deepcopy(old)
    expected['installer_sha256'] = own
    expected['new_manifest_sha256'] = sha(manifests[install.c.MANIFEST])
    for n,raw in manifests.items(): expected['updates'][n].update(install.prior.metadata(n,raw))
    require(spec == expected, 'installer-only spec correction / unchanged five ELF slots')
    return old, dict(qualified_package_ref=QUALIFIED_REF,qualified_spec_sha256=QUALIFIED_SPEC,
        corrected_spec_sha256=sha(install.canonical(spec)),native_result_sha256=QUALIFIED_RESULT,
        installer_only_metadata_change=True,production_rebuilt=False,native_rerun=False)


def verify_installer_tests(read, files, spec):
    for phase,code,installer_hash,spec_hash in (
        ('before-tests',1,QUALIFIED_INSTALLER,QUALIFIED_SPEC),
        ('after-tests',0,spec['installer_sha256'],sha(install.canonical(spec)))):
        prefix = P+'installer-correction/'+phase+'/'
        row = json.loads(read(prefix+'command.json')); log = read(prefix+'tests.log')
        require(type(row['exit_code']) is int and row['exit_code'] == code
                and row['log_sha256'] == sha(log) and b'Ran 23 tests in ' in log, 'installer regression evidence')
        require(row['candidate_spec_sha256'] == spec_hash and row['source_sha256'] == {
            'scripts/stage8b_p1f_o3_readmission_install.py':installer_hash,
            'scripts/test_stage8b_p1f_o3_readmission_install.py':sha(files['scripts/test_stage8b_p1f_o3_readmission_install.py'])},
            'installer regression source binding')
        require(row['source_unchanged'] is True and row['container_removed'] is True
                and row['network'] == 'none' and row['remote_writes'] is False and row['private_inputs'] is False,
                'local installer regression scope')
        if code == 0:
            require(log.endswith(b'\nOK\n') and b'fresh-public-read atime_changed=True' in log,
                    'fresh-read / corrected regression PASS')
        else:
            require(b'FAILED (failures=3, errors=1)' in log, 'retained reproduction')


def records(read, prefix, entries):
    ids = [v['id'] for v in entries]
    require(len(ids) == len(set(ids)), 'duplicate command record')
    for v in entries:
        label = v['id']; h.safe_name(label)
        require(json.loads(read(prefix+label+'.json')) == v, 'command metadata: '+label)
        for kind in ('stdout', 'stderr'):
            require(sha(read(prefix+label+'.'+kind)) == v[kind+'_sha256'], 'command log: '+label)
    return set(ids)


def verify_native(read, files, build, baseline, spec):
    prefix = P+'native/'
    result = json.loads(read(prefix+'result.json'))
    require(result['status'] == 'NATIVE_SYSTEMD_START_STOP_PASS_NOT_OPERATIONAL_O3', 'new native PASS required')
    for field in ('native','schedule_key_transferred','selected_key_removed','positive_phase_run',
                  'live_child_stop_proven','containers_removed','remote_public_staging_removed',
                  'host_binfmt_and_container_inventory_unchanged','protected_host_unchanged','qualification_fixture_source_changed'):
        require(result[field] is True, 'native flag: '+field)
    for field in ('operational_o3','installation_performed','host_state_used','production_rust_cargo_changed','automatic_launch_scheduled'):
        require(result[field] is False, 'native scope: '+field)
    require(result['cleanup_notes'] == [], 'native cleanup incomplete')
    require(result['source_ref'] == r.READMISSION_REF and result['production_build_sha256'] == install.BUILD
            and result['support_build_sha256'] == qual.SUPPORT_RECORD
            and result['installation_spec_sha256'] == sha(install.canonical(spec)), 'native input binding')
    helper_raw = read(P+'helper/helper-build.json')
    require(sha(helper_raw) == result['corrected_helper_build_sha256'] == qual.HELPER_RECORD
            and result['corrected_helper_source_ref'] == qual.HELPER_REF, 'corrected helper binding')
    helper = json.loads(helper_raw)
    for name, field, source in (
        ('driver.py','driver_sha256','scripts/stage8b_p1f_o3_readmission_qualify.py'),
        ('fixture-setup.py','fixture_setup_sha256','scripts/stage8b_p1f_o3_native_fixture.py')):
        require(read(prefix+name) == files[source] and sha(read(prefix+name)) == result[field], 'executed support source')
    require(result['observer_sha256'] == sha(read(prefix+'host-collector.py')) == qual.OBSERVER, 'exact observer')
    hashes = {b['name']:b['sha256'] for b in build['binaries']}
    hashes.update(qual.SUPPORT)
    hashes['input-helper'] = helper['binaries'][0]['sha256']
    hashes.update({u+'.in':sha(files['deploy/stage8b-p1f-o3/'+u+'.in']) for u in qual.UNITS})
    require(result['payload_sha256'] == hashes and set(result['support_not_release']) == set(qual.SUPPORT), 'qualified payload binding')
    require(result['systemd_image'] == qual.base.IMAGE and result['redis_image'] == qual.REDIS
            and result['selected_schedule_public_key'] == qual.PUBLIC, 'native image/trust binding')
    entries = result['records']; ids = records(read, prefix+'logs/', entries)
    required = set('''protected-before host-global-before real-clock machine swap-disabled images staging
        public-payload-directory create copy-public start-container isolation six-minute-backstop group user
        base-setup extra-setup bootstrap-support-executable bootstrap-support-not-writable exact-release-hashes
        redis-create redis-empty fixture-real-clock fixture-inputs fixture-public-custody fixture-provenance
        production-bootstrap pre-start-exact-release-hashes selected-key-tmpfs input-schedule-unreadable
        lifecycle-readable unit-syntax container-reload loaded-units start-signer real-signer-production-checkpoint
        install-public-envelope provision-fresh-redis redis-provisioned start-supervisor active-stable
        redis-observation-before-stop bounded-supervisor-stop stopped-processes bounded-signer-stop
        all-units-stopped supervisor-journal signer-journal all-recorded-identities-gone delete-selected-input
        final-journal-signer final-journal-supervisor remove-public-staging host-global-after protected-after'''.split())
    required |= {'public-transfer-'+n for n in hashes}
    require(required <= ids, 'missing native command')
    for family in ('ready-', 'redis-ready-', 'signer-ready-', 'active-processes-'):
        require(any(n.startswith(family) for n in ids), 'missing native readiness')
    require(len([n for n in ids if n.startswith('confirm-removed-')]) == 2, 'container removal evidence')
    for v in entries:
        n = v['id']; code = v['exit_code']
        expected = 1 if n in ('input-schedule-unreadable','bootstrap-support-not-writable') else 0
        require(type(code) is int and code == expected, 'native command exit: '+n)
        require(v['private_stdin_omitted'] is (n == 'selected-key-tmpfs'), 'private input boundary')
        if n == 'selected-key-tmpfs':
            require(v['stdin_sha256'] is None and v['private_output_valid'] is True, 'no private digest/output')
    log = lambda n: read(prefix+'logs/'+n+'.stdout')
    lj = lambda n: json.loads(log(n))
    for name in ('real-clock','fixture-real-clock'):
        qual.check_window(dt.datetime.fromtimestamp(int(log(name).strip()),dt.timezone.utc))
    require(log('selected-key-tmpfs') == b'PASS selected credential staged in container tmpfs; content omitted\n', 'private transfer diagnostic')
    before, after = lj('protected-before'), lj('protected-after')
    qual.check_host(before, baseline); qual.check_host(after, baseline)
    require(all(before[k] == after[k] for k in (*install.BASELINE_FIELDS,'public_seal')), 'protected host drift')
    require(lj('host-global-before') == lj('host-global-after'), 'host container/binfmt drift')
    require(log('machine').strip() == b'x86_64' and len(log('swap-disabled').splitlines()) == 1, 'native machine/no swap')
    v = lj('isolation'); require(len(v) == 1, 'one systemd container')
    host = v[0]['HostConfig']
    require(host['Privileged'] is False and not host['Binds'] and host['NetworkMode'] == 'none'
            and host['CgroupnsMode'] == 'private' and host['RestartPolicy']['Name'] == 'no'
            and all(m['Type'] == 'tmpfs' for m in v[0]['Mounts']), 'container isolation')
    require(host['NanoCpus'] == 1000000000 and host['Memory'] == host['MemorySwap'] == 805306368, 'native limits')
    require(lj('redis-empty') == 0 and lj('redis-provisioned') == 12, 'isolated Redis sequence')
    for name in ('exact-release-hashes','pre-start-exact-release-hashes'):
        observed = {Path(row.split()[1]).name:row.split()[0] for row in log(name).decode().splitlines()}
        require(observed == {b['name']:b['sha256'] for b in build['binaries']}, 'executed ELF identity')
    active = lj('active-stable'); phase = result['synthetic_phase_sha256']
    initial_id = [v['id'] for v in entries if v['id'].startswith('active-processes-')][-1]
    initial = lj(initial_id)
    require(set(active) == {'signer','supervisor'}, 'two active units')
    for role, unit in active.items():
        require({pid:p['start_ticks'] for pid,p in initial[role]['processes'].items()} ==
                {pid:p['start_ticks'] for pid,p in unit['processes'].items()}, 'process replaced during stability interval')
        require(unit['ActiveState'] == 'active' and unit['NRestarts'] == '0'
                and unit['MainPID'] in unit['processes'], 'running process')
        require(len(unit['processes']) == (1 if role == 'signer' else 2), 'child inventory')
        for pid, proc in unit['processes'].items():
            child = role == 'supervisor' and pid != unit['MainPID']
            exe = 'stage8b-p1f-o3-composition' if child else ('stage8b-p1f-o3-signer' if role == 'signer' else 'stage8b-p1f-local-supervisor')
            require(proc['command'].split() == [qual.FIXED+exe,'run',phase], 'running argv')
            creds = dict(line.split(':',1) for line in proc['credentials'])
            require(creds['Uid'].split() == [('987' if child else '0')]*4
                    and creds['Gid'].split() == ['987']*4 and creds['NoNewPrivs'].strip() == '1', 'identity/NNP')
            caps = 0 if child else (1 if role == 'signer' else 0xe0)
            require(all(int(creds[n],16) == caps for n in ('CapEff','CapPrm'))
                    and all(int(creds[n],16) == 0 for n in ('CapInh','CapAmb')), 'effective authority')
            if child: require(not creds['Groups'].split() and proc['ppid'] == unit['MainPID'], 'child custody')
    stopped = lj('all-units-stopped')
    require(all(v['MainPID'] == '0' and not v['processes'] and v['NRestarts'] == '0' for v in stopped.values()), 'surviving unit')
    require(stopped['supervisor']['ExecMainStatus'] == '0' and stopped['signer']['ExecMainStatus'] == '70', 'actual terminal statuses')
    require(lj('all-recorded-identities-gone') == {}, 'recorded process survived')
    report = result['supervision_result']
    rows = []
    for line in log('supervisor-journal').splitlines():
        try: v = json.loads(line)
        except ValueError: continue
        if isinstance(v,dict) and v.get('domain') == 'stage8b-p1f-local-supervision-result-v1': rows.append(v)
    require(rows == [report] and report['disposition'] == 'GRACEFUL_STOP' and report['child_starts'] == 1
            and report['last_exit_code'] == 0 and report['last_signal'] is None and report['force_killed'] is False
            and report['manifest_sha256'] == phase and 0 < result['supervisor_stop_seconds'] < 42, 'bounded graceful stop')
    require(log('delete-selected-input').strip() == b'PASS no staged schedule credential or systemd credential directory', 'credential cleanup')
    require(all(not log(n).strip() for n in ids if n.startswith('confirm-removed-')), 'remaining test container')
    require(all(b'proceeding without' not in log(n).lower() for n in ('supervisor-journal','signer-journal')), 'sandbox degraded')
    return dict(scope='ISOLATED_NATIVE_START_STOP_NOT_OPERATIONAL_O3',command_records=len(entries),
                stop_seconds=result['supervisor_stop_seconds'],supervisor_exit=0,signer_exit=70,protected_host_unchanged=True)


def verify_build(read, kind):
    support = kind == 'support'; prefix = P+kind+'/'
    raw = read(prefix+('support-build.json' if support else 'build.json'))
    require(sha(raw) == (qual.SUPPORT_RECORD if support else install.BUILD), 'exact build record')
    value = json.loads(raw)
    require(value['source_ref'] == r.READMISSION_REF and value['source_tree'] == r.READMISSION_TREE
            and value['image'] == r.b.IMAGE and value['source_tree_reverified'] is True, 'build source/image')
    require(value['container_removed'] is True and value['remote_staging_removed'] is True
            and value['cleanup_errors'] == [] and value['private_inputs'] is False, 'build cleanup/private scope')
    require(value['installation_performed'] is False and value['operational_o3_invoked'] is False, 'build not activation')
    records(read,prefix+'logs/',value['records'])
    for v in value['records']:
        require(v['exit_code'] == 0 or (v['id'].startswith('state-') and v['exit_code'] in (124,255)), 'build command failure')
    step = 'production-support-build' if support else 'production-build'
    log = read(prefix+'evidence/'+step+'.log'); rec = json.loads(read(prefix+'evidence/'+step+'.json'))
    require(rec['exit_code'] == 0 and rec['log_sha256'] == sha(log), 'Cargo log binding')
    artifacts = [json.loads(line) for line in log.splitlines() if line.startswith(b'{')]
    artifacts = [v for v in artifacts if v.get('reason') == 'compiler-artifact']
    require(len(artifacts) == value['production_artifact_count'] and artifacts, 'production artifact inventory')
    require(not any(v['profile']['test'] or any('fixture' in f for f in v['features']) for v in artifacts), 'production fixture contamination')
    targets = {'stage8b-p1-paper-supervisor'} if support else set(r.BINS)
    require([v for v in artifacts if v.get('executable') and v['target']['name'] in targets] == value['production_artifacts'], 'selected production artifacts')
    for v in value['binaries']:
        raw = read((prefix+'payload/' if support else P+'installation/payload/')+v['name'])
        require(sha(raw) == v['sha256'] and len(raw) == v['size'] and raw[:6] == b'\x7fELF\x02\x01'
                and int.from_bytes(raw[18:20],'little') == 62, 'native ELF binding')
    if support:
        log = read(prefix+'evidence/fixture-input-build.log')
        rec = json.loads(read(prefix+'evidence/fixture-input-build.json'))
        require(rec['exit_code'] == 0 and rec['log_sha256'] == sha(log), 'fixture helper log')
        items = [json.loads(line) for line in log.splitlines() if line.startswith(b'{')]
        items = [v for v in items if v.get('reason') == 'compiler-artifact']
        require([v for v in items if v.get('executable') == '/target/release/examples/o3_local_signer_witness'] ==
                [value['fixture_input_artifact']], 'separate fixture helper target')
        require({v['target']['name']:v['features'] for v in items if 'lib' in v['target']['kind']} ==
                value['fixture_library_features'], 'separate fixture graph')
    return value


def verify_helper(read):
    """Distinct fixture-only build; it never replaces production build evidence."""
    prefix = P+'helper/'
    raw = read(prefix+'helper-build.json')
    require(sha(raw) == qual.HELPER_RECORD, 'corrected helper record')
    value = json.loads(raw)
    require(value['source_ref'] == qual.HELPER_REF and value['image'] == r.b.IMAGE
            and value['status'] == 'FIXTURE_HELPER_BUILT_NOT_RELEASE'
            and value['build_kind'] == 'fixture_helper_correction'
            and value['production_artifacts'] == [] and value['production_artifact_count'] == 0
            and value['build_steps'] == builder.build_plan(fixture_only=True)[0], 'helper-only build profile')
    require(value['container_removed'] is True and value['remote_staging_removed'] is True
            and value['cleanup_errors'] == [] and value['private_inputs'] is False
            and value['installation_performed'] is False and value['operational_o3_invoked'] is False
            and value['source_tree_reverified'] is True and value['cargo_offline'] is True, 'helper cleanup/scope')
    records(read,prefix+'logs/',value['records'])
    for v in value['records']:
        require(v['exit_code'] == 0 or (v['id'].startswith('state-') and v['exit_code'] in (124,255)), 'helper command failure')
    for step in value['build_steps']:
        log = read(prefix+'evidence/'+step['name']+'.log')
        rec = json.loads(read(prefix+'evidence/'+step['name']+'.json'))
        require(rec['exit_code'] == 0 and rec['log_sha256'] == sha(log), 'helper Cargo log binding')
        if step['name'] == 'fixture-regression':
            require(b'1 passed; 0 failed' in log and b'fixture_matches_production_paper_source_and_old_marker_is_rejected' in log,
                    'real helper constructor regression')
        else:
            items = [json.loads(line) for line in log.splitlines() if line.startswith(b'{')]
            items = [v for v in items if v.get('reason') == 'compiler-artifact']
            require([v for v in items if v.get('executable') == '/target/release/examples/o3_local_signer_witness'] ==
                    [value['fixture_input_artifact']], 'corrected helper target')
            require(value['fixture_input_artifact']['target']['kind'] == ['example']
                    and value['fixture_input_artifact']['profile']['test'] is False, 'fixture executable profile')
            require({v['target']['name']:v['features'] for v in items if 'lib' in v['target']['kind']} ==
                    value['fixture_library_features'], 'corrected helper feature graph')
    require(len(value['binaries']) == 1 and value['binaries'][0]['name'] == 'input-helper', 'single helper output')
    binary = value['binaries'][0]; raw = read(prefix+'payload/input-helper')
    require(sha(raw) == binary['sha256'] and len(raw) == binary['size'] and raw[:6] == b'\x7fELF\x02\x01'
            and int.from_bytes(raw[18:20],'little') == 62, 'corrected helper ELF')
    return value


def verify(path):
    with zipfile.ZipFile(path) as z:
        infos = z.infolist(); names = [v.filename for v in infos]
        require(len(names) == len(set(names)) and sum(v.file_size for v in infos) < 300*1024*1024, 'archive inventory/size')
        modes = {}
        for v in infos:
            h.safe_name(v.filename); mode = v.external_attr>>16
            require(stat.S_ISREG(mode) and stat.S_IMODE(mode) in (0o644,0o755), 'nonregular member')
            modes[v.filename] = '100755' if mode & 0o111 else '100644'
        require(z.testzip() is None, 'CRC failure')
        read = z.read; js = lambda n: json.loads(read(P+n))
        inventory = js('files-sha256.json')
        require(set(inventory) == set(names)-{P+'files-sha256.json'}, 'complete archive manifest')
        for name, digest in inventory.items():
            raw = read(name); require(sha(raw) == digest, 'member hash: '+name); h.scan(name,raw)
            # Source preimages are independently bound to pinned Git trees below.
            # They can contain this checker's literal forbidden-token pattern;
            # the custody-token ban applies to observations, not scanner source.
            if name.startswith(P) and not name.startswith((P+'source-preimages/',P+'helper/source-preimages/')):
                require(b'r2b-trust-rebind-generation-2-20260830' not in raw, 'private custody path in evidence')
        d, index = js('descriptor.json'), js('source-index.json')
        # handoff-commit.txt may be generated, rather than a tracked Git blob.
        require(set(index)|{'handoff-commit.txt'} == {n for n in names if not n.startswith(P)}, 'tracked source inventory')
        files = {n:read(P+'original-handoff-commit.txt') if n == 'handoff-commit.txt' else read(n) for n in index}
        require(all(sha(files[n]) == e['sha256'] and modes[n] == e['mode'] for n,e in index.items()), 'source hash/mode')
        tree = h.tree_hash(files,modes); raw = read(P+'package-commit.raw')
        require(tree == d['package_tree'] and raw.startswith(('tree '+tree+'\n').encode())
                and previous.commit_hash(raw) == d['package_ref'], 'package Git binding')
        require((d['source_ref'],d['source_tree']) == (r.READMISSION_REF,r.READMISSION_TREE), 'accepted compiled source')
        idx = js('accepted-source-index.json')
        base = {n:files[n] if n in files and sha(files[n]) == e['sha256'] else read(P+'source-preimages/'+n) for n,e in idx.items()}
        require(all(sha(base[n]) == e['sha256'] for n,e in idx.items())
                and h.tree_hash(base,{n:e['mode'] for n,e in idx.items()}) == r.READMISSION_TREE, 'accepted source Git tree')
        raw = read(P+'accepted-source-commit.raw')
        require(previous.commit_hash(raw) == r.READMISSION_REF and raw.startswith(('tree '+r.READMISSION_TREE+'\n').encode()), 'accepted commit object')
        changed = {n for n in set(files)|set(base) if files.get(n) != base.get(n) or index.get(n,{}).get('mode') != idx.get(n,{}).get('mode')}
        require(changed == set(d['packaging_delta']), 'packaging delta')
        for n in changed:
            require(n in {'docs/current-status.md','docs/stage-8/gov-ci-1-authority.json',
                    'docs/stage-8/reviews/REVIEW_5df1c88_O3_SOURCE_ACCEPT_RU.txt',
                    'docs/stage-8/reviews/REVIEW_45a3f1b_O3_RELEASE_RESULT_RU.txt',
                    'docs/stage-8/stage8b-p1f-o3-readmission-source-correction.md',
                    'docs/stage-8/stage8b-p1f-o3-readmission-release.md'}
                    or n == HELPER_SOURCE
                    or n.startswith(('scripts/stage8b_p1f_o3_','scripts/test_stage8b_p1f_o3_')), 'unexpected runtime/deploy/CI delta: '+n)
        h.validate_templates(files,native=True)
        handoff = read('handoff-commit.txt').decode()
        require('full_sha='+d['package_ref'] in handoff and 'source_ref='+r.READMISSION_REF in handoff
                and 'archive='+d['archive'] in handoff, 'handoff provenance')
        build = verify_build(read,'build'); verify_build(read,'support')
        helper = verify_helper(read)
        hi = js('helper/source-index.json')
        hf = {n:files[n] if n in files and sha(files[n]) == e['sha256'] else read(P+'helper/source-preimages/'+n) for n,e in hi.items()}
        require(all(sha(hf[n]) == e['sha256'] and len(hf[n]) == e['size'] for n,e in hi.items())
                and h.tree_hash(hf,{n:e['mode'] for n,e in hi.items()}) == helper['source_tree'], 'helper Git tree')
        hr = read(P+'helper/source-commit.raw')
        require(previous.commit_hash(hr) == qual.HELPER_REF and sha(hr) == helper['source_commit_raw_sha256']
                and hr.startswith(('tree '+helper['source_tree']+'\n').encode()), 'helper commit binding')
        rust = lambda n: n.startswith('crates/') or n in ('Cargo.toml','Cargo.lock')
        hd = {n for n in set(hf)|set(base) if rust(n) and (hf.get(n) != base.get(n) or hi.get(n,{}).get('mode') != idx.get(n,{}).get('mode'))}
        require(hd == {HELPER_SOURCE} and files[HELPER_SOURCE] == hf[HELPER_SOURCE], 'only disposable example changed')
        source_index = {n:dict(sha256=sha(raw),size=len(raw),mode=idx[n]['mode']) for n,raw in base.items()}
        require(js('build/source-index.json') == js('support/source-index.json') == source_index, 'compiled input inventories')
        baseline, spec = js('installation/baseline.json'), js('installation/spec.json')
        derived, _, payload = install.derive(baseline, read(P+'installation/build.json'),
            read(P+'installation/before-outer.json'),read(P+'installation/before-o3.json'),
            {n:read(P+'installation/payload/'+Path(n).name) for n in install.ELFS})
        require(spec == derived and spec['installer_sha256'] == sha(files['scripts/stage8b_p1f_o3_readmission_install.py']), 'derived seven-slot plan')
        require(read(P+'installation/build.json') == read(P+'build/build.json'), 'one production build')
        for n,raw in payload.items(): require(raw == read(P+'installation/payload/'+Path(n).name), 'derived replacement: '+n)
        qualified_spec, continuity = verify_installation_continuity(read,files,baseline,spec)
        require(d['installation_continuity'] == continuity, 'declared installation continuity')
        native = verify_native(read,files,build,baseline,qualified_spec)
        verify_installer_tests(read,files,spec)
        old, new = (json.loads(v['docs/stage-8/gov-ci-1-authority.json']) for v in (base,files))
        inventories = {'production_code_manifest','governance_control_plane_manifest'}
        require({k:v for k,v in old.items() if k not in inventories} == {k:v for k,v in new.items() if k not in inventories}, 'authority policy changed')
        gates = js('gates/result.json')
        require(set(gates) == {'authority','authority-negative','templates','runner','builder','native-evidence','diff'}, 'gate inventory')
        for name, value in gates.items():
            require(value['exit_code'] == 0 and value['sha256'] == sha(read(P+'gates/'+name+'.log')), 'gate result')
        require(b'Ran 19 tests in ' in read(P+'installer-filesystem/tests.log')
                and read(P+'installer-filesystem/tests.log').endswith(b'\nOK\n'), 'retained Linux filesystem tests')
        return dict(status='PASS',verdict='INTEGRITY_AND_ISOLATED_QUALIFICATION_NOT_INSTALLATION_ACCEPTANCE',
            archive_sha256=sha(path.read_bytes()),package_ref=d['package_ref'],package_tree=tree,
            compiled_source_ref=r.READMISSION_REF,members=len(names),tracked_files=len(index),
            duplicates=0,symlinks=0,unsafe_paths=0,release_elfs=5,write_slots=7,
            native_qualification=native,installation_continuity=continuity,installer_filesystem_tests=23,
            installation_performed=False,operational_o3=False)


def package(work, native, out, helper=None, correction=None):
    # Refuse missing/failed witness before running gates or creating an archive.
    require(json.loads((native/'result.json').read_bytes())['status'] ==
            'NATIVE_SYSTEMD_START_STOP_PASS_NOT_OPERATIONAL_O3', 'new native PASS required')
    require(helper is not None, 'corrected fixture helper build required')
    require(correction is not None, 'installer correction evidence required')
    require(previous.git('status','--porcelain') == b'', 'commit completed packaging sources first')
    ref = previous.git('rev-parse','HEAD').decode().strip()
    files, modes = previous.snapshot(ref); base, bm = previous.snapshot(r.READMISSION_REF)
    require(not any(n.startswith(P) for n in files), 'reserved evidence prefix')
    payload = dict(files); name = 'moex-trading-project-'+ref[:7]+'-o3-readmission-release.zip'
    payload[P+'original-handoff-commit.txt'] = files.get('handoff-commit.txt',b'')
    payload['handoff-commit.txt'] = f'short_sha={ref[:7]}\nfull_sha={ref}\nsource_ref={r.READMISSION_REF}\narchive={name}\n'.encode()
    for label, tree_files, tree_modes, commit in (
        ('source',files,modes,ref),('accepted-source',base,bm,r.READMISSION_REF)):
        payload[P+label+'-index.json'] = encode({n:dict(sha256=sha(raw),mode=tree_modes[n]) for n,raw in tree_files.items()})
        payload[P+('package' if label == 'source' else label)+'-commit.raw'] = previous.git('cat-file','commit',commit)
    changed = {n for n in set(files)|set(base) if files.get(n) != base.get(n) or modes.get(n) != bm.get(n)}
    for n in changed:
        if n in base: payload[P+'source-preimages/'+n] = base[n]
    def add_file(src, dest):
        require(not src.is_symlink() and src.is_file(), 'missing/linked public evidence')
        payload[P+dest] = src.read_bytes()
    def add_tree(src, dest, exclude=()):
        require(src.is_dir() and not src.is_symlink(), 'missing public evidence directory')
        for p in sorted(src.rglob('*')):
            require(not p.is_symlink(), 'evidence symlink')
            if p.is_file() and not set(p.relative_to(src).parts)&set(exclude):
                add_file(p,dest+'/'+p.relative_to(src).as_posix())
    add_tree(correction/'installation','installation')
    for n in ('spec.json','baseline.json'):
        add_file(work/'installation-prepared-r1'/n,'qualified-installation/'+n)
    for n in (install.O3,install.c.MANIFEST):
        add_file(work/'installation-prepared-r1/payload'/Path(n).name,
                 'qualified-installation/payload/'+Path(n).name)
    payload[P+'qualified-installation/installer.py'] = previous.git(
        'show',QUALIFIED_REF+':scripts/stage8b_p1f_o3_readmission_install.py')
    for phase in ('before-tests','after-tests'):
        for n in ('tests.log','command.json'):
            add_file(correction/phase/n,'installer-correction/'+phase+'/'+n)
    for kind, directory, record in (
        ('build','native-build-r1','build.json'),('support','native-support','support-build.json')):
        for sub in ('logs','evidence'): add_tree(work/directory/sub,kind+'/'+sub)
        add_file(work/directory/record,kind+'/'+record)
        add_file(work/directory/'source-index.json',kind+'/source-index.json')
        if kind == 'support': add_tree(work/directory/'payload','support/payload')
    add_tree(native,'native',exclude=('payload',))
    failed = native.parent/'native-run-r0'
    failed_record = json.loads((failed/'result.json').read_bytes())
    require(failed_record['status'] == 'FAILED_NOT_QUALIFIED'
            and failed_record['containers_removed'] is True and failed_record['cleanup_notes'] == [], 'prior attempt/cleanup record')
    add_tree(failed,'historical-failed-native',exclude=('payload',))
    for sub in ('logs','evidence','payload'): add_tree(helper/sub,'helper/'+sub)
    for n in ('helper-build.json','source-index.json','source-commit.raw'): add_file(helper/n,'helper/'+n)
    hf, hm = previous.snapshot(qual.HELPER_REF)
    for n, raw in hf.items():
        if files.get(n) != raw: payload[P+'helper/source-preimages/'+n] = raw
    add_file(work/'readmission-installer-tests-r1.log','installer-filesystem/tests.log')
    add_file(work/'readmission-installer-tests-command.json','installer-filesystem/command.json')
    commands = {
        'authority':['python3','-B','scripts/current_tree_authority_check.py'],
        'authority-negative':['python3','-B','scripts/current_tree_authority_negative_harness.py'],
        'templates':['python3','-B','scripts/test_stage8b_p1f_o3_source_handoff.py'],
        'runner':['python3','-B','scripts/test_stage8b_p1f_o3_readmission_qualify.py'],
        'builder':['python3','-B','scripts/test_stage8b_p1f_o3_release_native.py'],
        'native-evidence':['python3','-B','scripts/test_stage8b_p1f_o3_readmission_handoff.py'],
        'diff':['git','diff','--check',r.READMISSION_REF,ref]}
    gates = {}
    for label, command in commands.items():
        q = subprocess.run(command,cwd=ROOT,capture_output=True,timeout=600,
                           env=dict(os.environ,O3_READMISSION_EVIDENCE=str(work),O3_READMISSION_NATIVE=str(native),
                                    O3_READMISSION_HELPER=str(helper),O3_READMISSION_PACKAGE=str(correction/'installation')))
        raw = q.stdout+q.stderr; require(q.returncode == 0, 'gate failed: '+label)
        gates[label] = dict(command=command,exit_code=q.returncode,sha256=sha(raw)); payload[P+'gates/'+label+'.log'] = raw
    payload[P+'gates/result.json'] = encode(gates)
    _, continuity = verify_installation_continuity(payload.__getitem__,files,
        json.loads(payload[P+'installation/baseline.json']),json.loads(payload[P+'installation/spec.json']))
    payload[P+'descriptor.json'] = encode(dict(archive=name,package_ref=ref,
        package_tree=previous.git('rev-parse',ref+'^{tree}').decode().strip(),source_ref=r.READMISSION_REF,
        source_tree=r.READMISSION_TREE,packaging_delta=sorted(changed),
        installation_continuity=continuity,
        corrected_fixture_source_ref=qual.HELPER_REF,corrected_fixture_build_sha256=qual.HELPER_RECORD,
        repository='https://github.com/DKorolski/moex-trading-gateway-rs',
        branch=previous.git('branch','--show-current').decode().strip(),
        installation_performed=False,operational_o3=False,acceptance='REVIEW_PENDING'))
    payload[P+'files-sha256.json'] = encode({n:sha(raw) for n,raw in payload.items()})
    out.mkdir(parents=True,exist_ok=True); path = out/name; partial = Path(str(path)+'.partial')
    require(not any(Path(str(path)+s).exists() for s in ('','.partial','.sha256','.safety.json','.review.txt')), 'output already exists')
    with zipfile.ZipFile(partial,'x',zipfile.ZIP_DEFLATED,compresslevel=6) as z:
        for n,raw in sorted(payload.items()):
            h.scan(n,raw); i = zipfile.ZipInfo(n,(2026,10,12,0,0,0)); i.create_system = 3
            i.external_attr = (stat.S_IFREG|(0o755 if modes.get(n) == '100755' else 0o644))<<16
            i.compress_type = zipfile.ZIP_DEFLATED; z.writestr(i,raw)
    safety = verify(partial); partial.rename(path)
    r.put(Path(str(path)+'.sha256'),f'{safety["archive_sha256"]}  {name}\n'.encode())
    r.put(Path(str(path)+'.safety.json'),encode(safety))
    r.put(Path(str(path)+'.review.txt'),f'''Repository: https://github.com/DKorolski/moex-trading-gateway-rs
Branch: {previous.git('branch','--show-current').decode().strip()}
Package commit: {ref}
Accepted compiled source: {r.READMISSION_REF}
Archive: {name}
SHA-256: {safety['archive_sha256']}

O3 installer-only correction of RELEASE HOLD {QUALIFIED_REF[:7]}.
P1: reassert replacement-parent and TX.parent fsync before terminal journal on retry.
P2: exclude read-induced atime drift, retain fd/path identity and mutation checks.
23 local Linux filesystem tests PASS; original installer fails four new tests.
Five production ELFs, two units, two helpers and protected baseline unchanged.
No production/helper rebuild, native rerun, credential use or VPS contact in this correction.
Original isolated native PASS and executed support sources retained byte-for-byte.
Qualified spec: {QUALIFIED_SPEC}
Corrected installation spec: {continuity['corrected_spec_sha256']}
Original result installation_spec_sha256 is NOT rewritten to the corrected spec.
Installer: seven exact replacements, no config/template/reload/history/Redis writes.
Installer tests use real Linux files/rename/fsync with synthetic host/DBSIZE; not power-crash proof.
No installation performed; no operational O3; no operational authorization.
Historical schedule-key witness remains test-only; no private bytes in ZIP.
Verify: python3 -B scripts/stage8b_p1f_o3_readmission_handoff.py verify ARCHIVE
Start at docs/stage-8/stage8b-p1f-o3-readmission-release.md and o3-readmission/descriptor.json.
Next: review -> separately permitted stopped install -> installation evidence -> separately permitted bounded O3.
'''.encode())
    print(json.dumps(safety,indent=2))


if __name__ == '__main__':
    p = argparse.ArgumentParser(description=__doc__); p.add_argument('action',choices=('package','verify'))
    p.add_argument('path',type=Path); p.add_argument('--work',type=Path); p.add_argument('--native',type=Path)
    p.add_argument('--helper-build',type=Path)
    p.add_argument('--correction',type=Path)
    a = p.parse_args()
    if a.action == 'verify': print(json.dumps(verify(a.path.resolve()),indent=2))
    else: package(a.work.resolve(),a.native.resolve(),a.path.resolve(),helper=a.helper_build.resolve(),
                  correction=a.correction.resolve() if a.correction else None)
