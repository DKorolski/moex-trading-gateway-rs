#!/usr/bin/env python3
"""Offline check of the stopped 304cd56 attempt; never contacts the target."""
import json
from pathlib import Path

import stage8b_p1f_o0_collect as o0
import stage8b_p1f_o1_collect as o1
import stage8b_p1f_o2_install as install

ROOT = Path(__file__).resolve().parents[1]
EVIDENCE = ROOT / 'docs/stage-8/evidence/o2-installation-r1'


def read_json(name):
    return json.loads((EVIDENCE / name).read_bytes(), object_pairs_hook=install.old.reject_duplicate_keys)


def check():
    attempt = read_json('attempt.json')
    install.require(attempt['outcome'] == 'STOPPED_AT_NATIVE_PREFLIGHT_BEFORE_INSTALL'
                    and attempt['command_exit_codes']['accepted_installer_preflight'] == 1
                    and attempt['install_invocations'] == attempt['resume_invocations'] == attempt['rollback_invocations'] == 0
                    and attempt['root_only_staging_created'] is True and attempt['staging_retained'] is True,
                    'execution record scope drift')
    for key in ('managed_payload_replacement_performed', 'corrected_installer_deployed', 'daemon_reload_performed',
                'service_enable_or_start_performed', 'signing_genesis_or_bootstrap_performed',
                'finam_contact_performed', 'redis_mutation_performed'):
        install.require(attempt[key] is False, 'forbidden operation in execution record')
    pre_raw = (EVIDENCE / 'fresh-o0.stdout').read_bytes()
    post_raw = (EVIDENCE / 'post-stop-o0.stdout').read_bytes()
    old_raw = (EVIDENCE / 'fresh-o1.stdout').read_bytes()
    pre = o0.build(o0.parse(pre_raw), pre_raw)
    post = o0.build(o0.parse(post_raw), post_raw)
    old = o1.build(o1.parse(old_raw), old_raw, pre, pre_raw)
    # Historical O0's absent-P1 and O1's first-ever-install clauses do not apply
    # to replacement of an already accepted O1. All remaining checks are retained.
    for value in (pre, post):
        install.require(all(v for k, v in value['checks'].items() if k != 'p1_identity_absent'), 'fresh platform/P0/DB15 drift')
    install.require(all(v for k, v in old['checks'].items() if k != 'fresh_o0_complete'), 'old O1 bytes/custody/state drift')
    install.require(pre['p0_services'] == post['p0_services'], 'P0 changed during stopped attempt')
    install.require(pre['p1_paths'] == post['p1_paths'], 'P1 path state changed')
    install.require((EVIDENCE / 'native-preflight.stdout').read_bytes() == b'', 'unexpected preflight success output')
    install.require((EVIDENCE / 'native-preflight.stderr').read_bytes() == b'stage8b-p1f-o2-install: FAIL incomplete systemd response\n', 'failure evidence drift')
    captured = read_json('not-found-fixture.json')
    install.require((EVIDENCE / 'not-found-fixture.json').read_bytes() == (ROOT / 'scripts/fixtures/stage8b-o2-systemd255-not-found.json').read_bytes(), 'native fixture differs from captured bytes')
    install.require(captured['accepted_installer_ref'] == '304cd56bd33e2145f327c5f1ea02f56837cc3e62'
                    and captured['managed_state'] == 'EXACT_O1_UNCHANGED'
                    and captured['transaction_present'] is False
                    and captured['new_installation_manifest_present'] is False, 'post-stop managed state drift')
    expected = {'MainPID': '0', 'ControlPID': '0', 'ControlGroup': '', 'LoadState': 'not-found',
                'ActiveState': 'inactive', 'SubState': 'dead', 'FragmentPath': '', 'DropInPaths': '', 'Job': ''}
    install.require({item['unit'] for item in captured['observations']} == set(install.NEW_UNITS), 'observed unit inventory drift')
    for item in captured['observations']:
        install.require(item['returncode'] == 0 and item['stderr'] == '', 'query failure is not absence')
        install.require(dict(line.split('=', 1) for line in item['stdout'].splitlines()) == expected, 'native nine-field shape drift')
    diagnostics = read_json('systemd-diagnostic.json')
    for item in diagnostics['observations']:
        install.require(item['returncode'] == 0, 'read-only diagnostic failed')
        if item['unit'] in install.NEW_UNITS:
            install.require(item['missing'] == ['ExecStart'] and item['line_count'] == 9, 'not-found --all observation drift')
        else:
            install.require(item['missing'] == [] and item['line_count'] == 10, 'loaded property observation drift')
    staging = read_json('staging-extract.stdout')
    install.require(staging['result'] == 'EXACT_PACKAGE_STAGED' and staging['installed_payload_changed'] is False
                    and staging['activation_performed'] is False
                    and staging['archive_sha256'] == '22c034decddbd3062a40b1d6a95b71d5beec222c2298f83f112659890ef68bbc', 'staging scope drift')
    verified = read_json('remote-package-verify.stdout')
    install.require(verified['result'] == 'PASS' and verified['source_ref'] == captured['accepted_installer_ref'], 'accepted remote package drift')
    review = EVIDENCE / 'FINAM_304cd56_O2_INSTALLATION_PACKAGE_REVIEW_2026-09-29.md'
    install.require(install.sha(review.read_bytes()) == '20b48c7ad8a7a0d16f279eb11f663b078093d1bfce4e5a9a658e079ddcb2dec9', 'accepted installation review drift')
    print('PASS o2-installation-r1-evidence stopped_before_install=true staging_retained=true p0_unchanged=true db15_empty=true')


if __name__ == '__main__':
    check()
