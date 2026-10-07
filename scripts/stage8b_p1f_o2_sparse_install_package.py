#!/usr/bin/env python3
"""Fixed sparse successor. Offline preparation only; no signing/install/activation.

Compact predecessor extracts are authenticated by the accepted manifest hash
and exact retained observation hashes, not a self-declared extract inventory.
"""
import argparse
import copy
from datetime import datetime, timezone
import json
from pathlib import Path
import zipfile

import stage8b_p1f_o2_sparse_artifact as artifact
import stage8b_p1f_o2_sparse_install as update

ROOT = Path(__file__).resolve().parents[1]
ARTIFACT = 'moex-trading-project-823fd35-sparse-o2-artifact.zip'
ARTIFACT_SHA = 'accb724cbe2b99d11847546e37b482a51a36483744b23fc58ebc0fecb94b0a38'
PRIOR = 'moex-trading-project-eb1a974-no-riskgate-installation.zip'
PRIOR_SHA = '7c5515dec039b6b2ada62baf819c739bf14eb7ed7b487df2749ac5383f3c1710'
TERMINAL = 'finam-o2-eb1a974-no-riskgate-bounded-evidence-20261002.zip'
REVIEW_SHA = 'b0e4edb07bcf30a09ba527df5484a351d81a557bf17166dd6377fd5e29832e1e'
PLAN_PATH = 'crates/runtime-durable-service/src/stage8b_p1e_first_boot_source/observed.rs'
PLAN_SHA = '8f3cac0b35529301ef2ea056a2e1ab00db0aa5e955786b8141cf1ef701d1e61d'
IDENTITY = 'fbd9ceb7ee964944e79db559c4e4d5cc7af54578b653ce218e183d01fa1444c3'
FINGERPRINT = '665f18112142f6c9aee1b613e341870f0d559d783fe41df725e26415507e6290'
TERMINAL_MEMBERS = {
    'evidence/postflight.stdout': 'dfb0c24341d8ed3f735fa5fc984f428e02f503b8cbce4a36a11dcbc45430702f',
    'evidence/rebound-preflight.stdout': '9c7ff645ed7c3f409befb9ccc59aca828d1426844c5f0e51caebd860ccce4537',
    'evidence/final-authority.stdout': 'd93c3ff40a0b74cd50886c037c661beddb0263a7a31877136609e3feedf51a14',
}
CALENDAR = dict(session_date='2026-10-05', prior_dates=['2026-09-29', '2026-09-30', '2026-10-01', '2026-10-02'],
                execution_window_start_utc='2026-10-05T04:10:00Z', execution_window_end_utc='2026-10-05T20:50:00Z',
                candidate_cutoff_rule='latest complete canonical M10 close at or before trusted_now',
                candidate_max_age_seconds=900, broker_truth_max_age_seconds=300,
                outside_window='REJECT; explicitly rebind calendar, never auto-roll or reuse authorization',
                calendar_sources=['https://www.moex.com/n101220', 'https://www.moex.com/n98363', 'https://www.moex.com/n103379'],
                prior_session_selection='four explicit weekday sessions; no synthetic weekend intervals',
                declared_intraday_breaks=[], execution_authorized=False)
sha, canonical, require = update.custody.sha, update.custody.canonical, update.require


def pinned_zip(path, digest):
    require(sha(path.read_bytes()) == digest, 'input archive pin: ' + path.name)
    return zipfile.ZipFile(path)


def read_compact(path):
    with zipfile.ZipFile(path) as z:
        require(len(z.namelist()) == len(set(z.namelist())) and z.testzip() is None, 'extract duplicates/CRC')
        for info in z.infolist():
            artifact.safety.validate_member_name(info.filename)
            require(str(artifact.PurePosixPath(info.filename)) == info.filename, 'noncanonical extract path')
            require(info.external_attr >> 16 == 0o100644, 'extract special file')
        return {n: z.read(n) for n in z.namelist()}


def write_compact(path, files):
    with zipfile.ZipFile(path, 'x', zipfile.ZIP_DEFLATED) as z:
        for name, raw in sorted(files.items()):
            z.writestr(artifact.common.zip_info(name, '100644'), raw)


def load_predecessor(terminal, prior):
    files = read_compact(prior)
    raw = files[update.custody.MANIFEST.lstrip('/')]
    require(sha(raw) == update.OLD_SHA, 'prior installation pin')
    manifest = update.strict_json(raw)
    require(set(files) == {p.lstrip('/') for p in manifest['payload']} | {update.custody.MANIFEST.lstrip('/')}, 'prior extract inventory')
    blobs = {p: files[p.lstrip('/')] for p in manifest['payload']}
    for p, value in manifest['payload'].items():
        require(sha(blobs[p]) == value['sha256'] and len(blobs[p]) == value['size'], 'prior extract bytes: ' + p)
    blobs[update.custody.MANIFEST] = raw
    evidence = read_compact(terminal)
    require(set(evidence) == set(TERMINAL_MEMBERS), 'terminal extract inventory')
    require(all(sha(evidence[n]) == digest for n, digest in TERMINAL_MEMBERS.items()), 'terminal evidence pin')
    expected = update.strict_json(evidence['evidence/postflight.stdout'])
    expected['observation'] = update.strict_json(evidence['evidence/rebound-preflight.stdout'])['observation']
    head = expected['history_head']
    require(expected['status'] == 'PASS' and expected['installation_sha256'] == update.OLD_SHA
            and head['state'] == 'FAILED' and head['latest_sequence'] == 6 and head['authority_generation'] == 1
            and head['latest_event_sha256'] == 'e68b805c360a4ee463767cd50815e1b59f084e5d30ca6cffa0a378c4b7a0f7de', 'retained terminal lineage')
    final = update.strict_json(evidence['evidence/final-authority.stdout'])['authority']
    require(all(final[k] == v for k, v in head.items() if k in final), 'terminal readback mismatch')
    return blobs, expected


def session(date):
    def ts(time):
        return int(datetime.fromisoformat(date + 'T' + time).replace(tzinfo=timezone.utc).timestamp())
    return dict(session_date=date, windows=[dict(first_close_time_utc=ts('04:10:00'), last_close_time_utc=ts('20:50:00'))])


def material(prior, z):
    payload = {name: z.read(artifact.P + 'payload/' + Path(name).name) for name in update.CHANGES[:3]}
    policy = update.strict_json(prior[update.CHANGES[3]])
    policy.update(schema_version=3, domain='stage8b-p1f-o2-materialization-policy-v3',
                  market_data_policy_sha256=PLAN_SHA, operational_identity_sha256=IDENTITY,
                  runtime_profile_id=artifact.PROFILE, runtime_profile_sha256=artifact.PROFILE_SHA,
                  bars_start_utc=CALENDAR['prior_dates'][0] + 'T04:00:00Z',
                  bars_end_utc=CALENDAR['execution_window_end_utc'])
    source = update.strict_json(prior[update.CHANGES[4]])
    # V3 remains a calendar-only scaffold; production materializer emits V4.
    source['operational_identity_sha256'] = IDENTITY
    source['history_coverage'] = dict(source_mode='config-bound-explicit-session-windows-v1', sessions_sha256='0'*64,
                                    sessions=[session(d) for d in CALENDAR['prior_dates']], candidate_session=session(CALENDAR['session_date']))
    supervisor = update.strict_json(prior[update.CHANGES[5]])
    supervisor['bootstrap'].update(schema_version=2, market_data_policy_sha256=PLAN_SHA)
    for path, value in zip(update.CHANGES[3:6], [policy, source, supervisor]):
        payload[path] = canonical(value)
    compat = update.strict_json(prior[update.custody.old.MANIFEST])
    compat['binary_sha256'] = sha(payload[update.custody.old.BINARY_PATH])
    for name in compat['managed_payload_sha256']:
        if name in payload:
            compat['managed_payload_sha256'][name] = sha(payload[name])
    payload[update.custody.old.MANIFEST] = canonical(compat)
    old = update.strict_json(prior[update.custody.MANIFEST])
    new = copy.deepcopy(old)
    for name, raw in payload.items():
        new['payload'][name].update(sha256=sha(raw), size=len(raw))
    new.update(artifact_ref=artifact.source.SOURCE_REF, artifact_sha256=ARTIFACT_SHA,
               installer_sha256=sha(Path(update.__file__).read_bytes()), predecessor_manifest_sha256=update.OLD_SHA,
               update_revision='sparse-4668b42-after-failed-6-v1')
    payload[update.custody.MANIFEST] = canonical(new)
    spec = dict(schema_version=1, domain='moex.o2.sparse-installation.v1', execution_authorized=False,
                installation_authorized=False, target_mutation_performed=False,
                artifact_sha256=ARTIFACT_SHA, compiled_source_ref=artifact.source.SOURCE_REF,
                old_manifest_sha256=update.OLD_SHA, new_manifest_sha256=sha(payload[update.custody.MANIFEST]),
                old_inventory=old, preserved_installation_id=old['installation_id'],
                prior_full_archive_sha256=PRIOR_SHA, terminal_evidence_sha256=update.TERMINAL_SHA,
                terminal_member_sha256=TERMINAL_MEMBERS, source_plan_sha256=PLAN_SHA,
                profile_sha256=artifact.PROFILE_SHA, runtime_fingerprint=FINGERPRINT, calendar=CALENDAR,
                operational_identity_sha256=IDENTITY, root_migration_authorized=False,
                expected_terminal='FAILED/1/6',
                updates={name: dict(old_sha256=sha(prior[name]), new_sha256=sha(raw), size=len(raw)) for name, raw in payload.items()})
    return payload, spec


def validate_prepared(directory):
    expected_files = {'spec.json', 'sparse-policy-contract.rs', 'binary-artifact.zip', 'prior-installation.zip', 'retained-terminal.zip', 'binary-artifact-acceptance.md'}
    expected_files |= {'payload/' + Path(p).name for p in update.CHANGES}
    require({p.relative_to(directory).as_posix() for p in directory.rglob('*') if p.is_file()} == expected_files, 'prepared inventory')
    require(not any(p.is_symlink() for p in directory.rglob('*')), 'prepared symlink')
    prior, _ = load_predecessor(directory / 'retained-terminal.zip', directory / 'prior-installation.zip')
    with pinned_zip(directory / 'binary-artifact.zip', ARTIFACT_SHA) as z:
        payload, spec = material(prior, z)
        require((directory/'sparse-policy-contract.rs').read_bytes() == z.read(PLAN_PATH), 'policy contract bytes')
    require((directory/'spec.json').read_bytes() == canonical(spec), 'spec exact binding')
    for name, raw in payload.items():
        require((directory/'payload'/Path(name).name).read_bytes() == raw, 'prepared slot binding: ' + name)
    require(sha((directory/'binary-artifact-acceptance.md').read_bytes()) == REVIEW_SHA, 'binary artifact acceptance pin')
    return spec


def prepare(handoff, review, output):
    require(sha(review.read_bytes()) == REVIEW_SHA, 'review pin')
    artifact.check(handoff / ARTIFACT)
    # Validate full original archives once; retain only authenticated public bytes.
    with pinned_zip(handoff/PRIOR, PRIOR_SHA) as z:
        raw = z.read('installation-no-riskgate/payload/installation-o2-v1.json')
        require(sha(raw) == update.OLD_SHA, 'old manifest')
        manifest = update.strict_json(raw)
        wanted = {v['sha256'] for v in manifest['payload'].values()}
        by_hash = {}
        for name in z.namelist():
            if name.endswith('.zip'):
                continue
            data = z.read(name)
            if sha(data) in wanted:
                by_hash[sha(data)] = data
        require(set(by_hash) == wanted, 'missing old payload bytes')
        prior_files = {p.lstrip('/'): by_hash[e['sha256']] for p, e in manifest['payload'].items()}
        prior_files[update.custody.MANIFEST.lstrip('/')] = raw
    with pinned_zip(handoff/TERMINAL, update.TERMINAL_SHA) as z:
        retained = {n: z.read(n) for n in TERMINAL_MEMBERS}
    output.mkdir(parents=True, exist_ok=False)
    (output/'payload').mkdir()
    write_compact(output/'prior-installation.zip', prior_files)
    write_compact(output/'retained-terminal.zip', retained)
    prior, _ = load_predecessor(output/'retained-terminal.zip', output/'prior-installation.zip')
    with pinned_zip(handoff/ARTIFACT, ARTIFACT_SHA) as z:
        payload, spec = material(prior, z)
        (output/'sparse-policy-contract.rs').write_bytes(z.read(PLAN_PATH))
    for name, raw in payload.items():
        (output/'payload'/Path(name).name).write_bytes(raw)
    (output/'binary-artifact.zip').write_bytes((handoff/ARTIFACT).read_bytes())
    (output/'binary-artifact-acceptance.md').write_bytes(review.read_bytes())
    (output/'spec.json').write_bytes(canonical(spec))
    validate_prepared(output)
    print(json.dumps(dict(result='PREPARED_NOT_INSTALLED', write_slots=len(payload), new_manifest_sha256=spec['new_manifest_sha256'])))


if __name__ == '__main__':
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('action', choices=('prepare', 'check'))
    p.add_argument('directory', type=Path)
    p.add_argument('--handoff', type=Path, default=ROOT/'reports/handoff')
    p.add_argument('--review', type=Path)
    a = p.parse_args()
    if a.action == 'prepare':
        prepare(a.handoff, a.review, a.directory)
    else:
        print(json.dumps(validate_prepared(a.directory), sort_keys=True))
