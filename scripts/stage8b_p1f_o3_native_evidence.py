"""Offline checks for the single authorized native start/stop witness.

Retained observations, not an operational permit or remote attestation service.
No command in this module runs the witness, transfers keys or contacts the VPS.
"""
import json
from pathlib import Path

from stage8b_p1f_o3_source_handoff import require, sha

PREFIX = 'o3-release/native/'
TEMPLATE = 'deploy/stage8b-p1f-o3/moex-finam-p1f-o3-supervisor.service.in'
OLD_ARCHIVE = 'd6057f5316437780a781fc123c4484b2c4c19dc2b9b0a6bfd0b239dcdfa6deb2'
DRIVER = '1aef49a06af9bb7c5a790604fe6f14ca294f2d7789c5d99b635b3bc0056f4cbc'
IDS = '''host-global-before machine swap-disabled images staging create copy-public
start-container isolation ready-0 six-minute-backstop group user base-setup extra-setup
exact-release-hashes redis-create redis-ready-0 redis-empty fixture-inputs
fixture-public-custody fixture-provenance production-bootstrap pre-start-exact-release-hashes
selected-key-tmpfs input-schedule-unreadable lifecycle-readable unit-syntax container-reload
loaded-units start-signer signer-ready-0 real-signer-production-checkpoint install-public-envelope
provision-fresh-redis redis-provisioned start-supervisor active-processes-0 active-stable
redis-observation-before-stop bounded-supervisor-stop stopped-processes bounded-signer-stop
all-units-stopped supervisor-journal signer-journal all-recorded-identities-gone
delete-selected-input final-journal-signer final-journal-supervisor
remove-moex-o3-positive-3a590fb62c6e-redis remove-moex-o3-positive-3a590fb62c6e
remove-public-files remove-empty-staging host-global-after'''.split()


def directives(raw):
    return [s.strip() for s in raw.decode().splitlines()
            if s.strip() and not s.lstrip().startswith('#')]


def verify(read, names, files, base, build, snapshot):
    js = lambda n: json.loads(read(PREFIX + n))
    log = lambda n: read(PREFIX + 'run/logs/' + n + '.stdout')
    lj = lambda n: json.loads(log(n))
    result = js('run/result.json')
    require(result['status'] == 'NATIVE_SYSTEMD_START_STOP_PASS_NOT_OPERATIONAL_O3', 'native verdict')
    require(result['input_archive_sha256'] == OLD_ARCHIVE, 'native predecessor')
    require(result['driver_sha256'] == sha(read(PREFIX + 'run/driver.py')) == DRIVER, 'executed driver')
    require(sha(read(PREFIX + 'run/keyless-base.py')) ==
            'ddaf4a803205e24a9687a471eed909639af695b2410ece7772b340861de40f9b', 'executed driver support')
    for key in ('native', 'source_changed', 'schedule_key_transferred', 'positive_phase_run',
                'live_child_stop_proven', 'containers_removed', 'selected_key_removed',
                'remote_public_staging_removed', 'host_binfmt_and_container_inventory_unchanged'):
        require(result[key] is True, 'native flag: ' + key)
    for key in ('operational_o3', 'installation_performed', 'host_state_used', 'rust_cargo_changed'):
        require(result[key] is False, 'native scope: ' + key)
    require(result['cleanup_notes'] == [], 'native cleanup incomplete')
    old, new = base[TEMPLATE], files[TEMPLATE]
    require(directives(new) == ['User=' if s == 'User=root' else s for s in directives(old)],
            'only supervisor User directive may change')
    require(result['template_correction']['old_sha256'] == sha(old)
            and result['template_correction']['new_sha256'] == sha(new), 'qualified template binding')
    hashes = {b['name']: b['sha256'] for b in build['binaries']}
    for role in ('signer', 'supervisor'):
        name = 'moex-finam-p1f-o3-' + role + '.service.in'
        hashes[name] = sha(files['deploy/stage8b-p1f-o3/' + name])
    require(all(result['payload_sha256'][n] == v for n, v in hashes.items()), 'native release/template identity')
    require(set(result['payload_sha256']) == set(hashes) | {'input-helper', 'bootstrap-source-witness'}
            and result['support_not_release'] == ['input-helper', 'bootstrap-source-witness'], 'support separation')
    records = result['records']
    require([v['id'] for v in records] == IDS, 'native exact command inventory/order')
    expected_files = {PREFIX + 'run/' + n for n in ('result.json', 'driver.py', 'keyless-base.py')}
    for v in records:
        n = v['id']; expected = 1 if n == 'input-schedule-unreadable' else 0
        allowed = [0, 1] if n in ('ready-0', 'final-journal-signer', 'final-journal-supervisor') else [expected]
        require(type(v['exit_code']) is int and v['exit_code'] == expected
                and v['expected'] == allowed, 'native command exit: ' + n)
        require(v['private_stdin_omitted'] is (n == 'selected-key-tmpfs'), 'private stdin boundary')
        if n == 'selected-key-tmpfs':
            require(v['stdin_sha256'] is None, 'no private stdin digest')
        for ext in ('stdout', 'stderr', 'json'):
            path = PREFIX + 'run/logs/' + n + '.' + ext; expected_files.add(path)
            raw = read(path)
            require(json.loads(raw) == v if ext == 'json' else sha(raw) == v[ext + '_sha256'],
                    'native retained command record: ' + n)
    for when in ('before', 'after'):
        for n in ('collector.py', 'stdout.json', 'stderr.txt', 'record.json'):
            expected_files.add(PREFIX + when + '/' + n)
        record = js(when + '/record.json')
        require(record['exit_code'] == 0 and all(record[k] == sha(read(PREFIX + when + '/' + n))
                for k, n in (('stdin_sha256', 'collector.py'), ('stdout_sha256', 'stdout.json'),
                             ('stderr_sha256', 'stderr.txt'))), 'host observation records')
    require({n for n in names if n.startswith(PREFIX)} == expected_files, 'native evidence inventory')
    require(js('before/stdout.json') == js('after/stdout.json') == snapshot, 'host full snapshot changed')
    require(snapshot['status'] == 'PASS_READ_ONLY' and len(snapshot['installed']) == 15
            and len(snapshot['new_slots_absent']) == 7, 'accepted installation untouched')
    head = snapshot['expected']['history_head']
    require((head['state'], head['authority_generation'], head['latest_sequence']) == ('COMPLETED', 1, 26),
            'accepted O2 history changed')
    require(lj('host-global-before') == lj('host-global-after'), 'host global inventory changed')
    require(log('machine').strip() == b'x86_64' and len(log('swap-disabled').splitlines()) == 1, 'native/no swap')
    isolation = lj('isolation'); require(len(isolation) == 1, 'one systemd container')
    hc = isolation[0]['HostConfig']
    require(hc['NetworkMode'] == 'none' and hc['CgroupnsMode'] == 'private'
            and hc['Privileged'] is False and not hc['Binds'] and not isolation[0]['Mounts']
            and hc['RestartPolicy']['Name'] == 'no', 'isolated container boundary')
    require(log('redis-empty').strip() == b'0' and log('redis-provisioned').strip() == b'12', 'isolated Redis witness')
    for n in ('exact-release-hashes', 'pre-start-exact-release-hashes'):
        observed = {Path(line.split()[1]).name: line.split()[0] for line in log(n).decode().splitlines()}
        require(observed == {b['name']: b['sha256'] for b in build['binaries']}, 'executed ELF identity')
    phase = result['synthetic_phase_sha256']
    identities = {}
    for n in ('active-processes-0', 'active-stable'):
        state = lj(n)
        require(set(state) == {'signer', 'supervisor'}, 'active units inventory')
        for role, u in state.items():
            require(u['ActiveState'] == 'active' and u['SubState'] == 'running'
                    and u['NRestarts'] == '0' and u['MainPID'] in u['processes'], 'unit not running')
            require(len(u['processes']) == (1 if role == 'signer' else 2), 'child count')
            for pid, proc in u['processes'].items():
                creds = {k: v.split() for k, v in (s.split(':', 1) for s in proc['credentials'])}
                child = role == 'supervisor' and pid != u['MainPID']
                exe = 'stage8b-p1f-o3-composition' if child else (
                    'stage8b-p1f-o3-signer' if role == 'signer' else 'stage8b-p1f-local-supervisor')
                require(proc['command'].split() == ['/usr/local/libexec/moex/' + exe, 'run', phase], 'process argv')
                require(creds['Uid'] == [('987' if child else '0')] * 4
                        and creds['Gid'] == ['987'] * 4 and creds['NoNewPrivs'] == ['1'], 'identity/NNP')
                for cap in ('CapInh', 'CapAmb'):
                    require(int(creds[cap][0], 16) == 0, 'ambient/inheritable authority')
                wanted = 0 if child else (1 if role == 'signer' else 0xe0)
                require(all(int(creds[c][0], 16) == wanted for c in ('CapEff', 'CapPrm')), 'effective authority')
                if child:
                    require(creds['Groups'] == [] and proc['ppid'] == u['MainPID'], 'child custody')
                if n == 'active-processes-0': identities[pid] = proc['start_ticks']
                else: require(identities.get(pid) == proc['start_ticks'], 'unexpected child replacement')
    for u in lj('all-units-stopped').values():
        require(u['MainPID'] == '0' and u['NRestarts'] == '0' and not u['processes'] and not u['ControlGroup'],
                'remaining unit processes')
    stopped = lj('all-units-stopped')
    require(stopped['supervisor']['ExecMainStatus'] == '0' and stopped['signer']['ExecMainStatus'] == '70',
            'retain actual stop statuses')
    require(lj('all-recorded-identities-gone') == {}, 'surviving recorded identity')
    outcomes = [json.loads(s) for s in log('supervisor-journal').splitlines() if s.startswith(b'{')]
    outcome = result['supervision_result']
    require(outcomes == [outcome] and outcome['disposition'] == 'GRACEFUL_STOP'
            and outcome['child_starts'] == 1 and outcome['last_exit_code'] == 0
            and outcome['last_signal'] is None and outcome['force_killed'] is False
            and outcome['manifest_sha256'] == phase, 'graceful stop result')
    require(0 < result['supervisor_stop_seconds'] < 40, 'bounded stop')
    require(log('delete-selected-input').strip() == b'PASS no staged schedule credential or systemd credential directory',
            'credential cleanup')
    require(all(b'proceeding without' not in log(n).lower() for n in ('final-journal-signer', 'final-journal-supervisor')),
            'sandbox degradation warning')
    return {'scope': 'isolated_native_start_stop_only', 'command_records': len(records),
            'host_snapshots_exact_equal': True, 'supervisor_stop_seconds': result['supervisor_stop_seconds'],
            'supervisor_exit': 0, 'signer_terminal_exit': 70, 'operational_o3': False}
