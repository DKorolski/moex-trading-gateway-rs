#!/usr/bin/env python3
"""Offline fixed installation preparation. No host/network/authority operations."""
import argparse
import copy
from datetime import datetime, timezone
import json
from pathlib import Path
import zipfile

import stage8b_p1f_o2_no_riskgate_artifact as artifact
import stage8b_p1f_o2_no_riskgate_install as update

ROOT = Path(__file__).resolve().parents[1]
ARTIFACT = 'moex-trading-project-64f1fd5-no-riskgate-o2-artifact.zip'
ARTIFACT_SHA = '66de9f7b362c90ca457c3f0dfb921c7eaf31091ce4d3aa5bbf95268a3544cee3'
PRIOR = 'moex-trading-project-3923c5c-o2-terminal-successor-review.zip'
PRIOR_SHA = '748522220cfa049b1b4cd2fa6f5e8a0d6d4aa85da50416fa12625b13e1389879'
TERMINAL = 'finam-o2-3923c5c-bounded-successor-evidence-20260930.zip'
REVIEW = 'FINAM_64f1fd5_NO_RISKGATE_O2_ARTIFACT_REVIEW_2026-10-01.md'
REVIEW_SHA = '3366ed7127ca0404d6f96614868c149d132552c999a976859fe2df7d640d8477'
PLAN_PATH = 'docs/stage-8/stage8b-p1e-first-boot-source-plan-v3.json'
PLAN_SHA = 'd722d70a897578ce93217f34c82dff2a7ed6c6c402a12914b7b862c2d95b693c'
FINGERPRINT = '665f18112142f6c9aee1b613e341870f0d559d783fe41df725e26415507e6290'
CALENDAR = dict(session_date='2026-10-02', prior_dates=['2026-09-28', '2026-09-29', '2026-09-30', '2026-10-01'],
                execution_window_start_utc='2026-10-02T04:10:00Z', execution_window_end_utc='2026-10-02T20:50:00Z',
                candidate_cutoff_rule='latest complete canonical M10 close at or before trusted_now',
                candidate_max_age_seconds=900, broker_truth_max_age_seconds=300,
                outside_window='REJECT; explicitly rebind calendar and installation identity, never auto-roll',
                calendar_sources=['https://www.moex.com/n101220', 'https://www.moex.com/n98363', 'https://www.moex.com/n103379'],
                declared_intraday_breaks=[], execution_authorized=False)
sha, canonical, require = update.custody.sha, update.custody.canonical, update.require


def pinned_zip(path, digest):
    require(sha(path.read_bytes()) == digest, 'input archive pin: ' + path.name)
    return zipfile.ZipFile(path)


def load_predecessor(terminal, prior):
    with pinned_zip(prior, PRIOR_SHA) as z:
        raw = z.read('update-payload/installation-o2-v1.json')
        require(sha(raw) == update.OLD_SHA, 'prior installation pin')
        manifest = update.strict_json(raw)
        wanted = {v['sha256'] for v in manifest['payload'].values()}
        by_hash = {}
        for name in z.namelist():
            data = z.read(name)
            digest = sha(data)
            if digest in wanted:
                by_hash[digest] = data
        require(wanted == set(by_hash), 'missing exact predecessor bytes')
        blobs = {p: by_hash[e['sha256']] for p, e in manifest['payload'].items()}
        blobs[update.custody.MANIFEST] = raw
    with pinned_zip(terminal, update.TERMINAL_SHA) as z:
        expected = update.strict_json(z.read('evidence/postflight.stdout'))
        expected['observation'] = update.strict_json(z.read('evidence/before-claim-preflight.stdout'))['observation']
    head = expected['history_head']
    require(expected['status'] == 'PASS' and expected['installation_sha256'] == update.OLD_SHA
            and head['state'] == 'FAILED' and head['latest_sequence'] == 4 and head['authority_generation'] == 1
            and head['latest_event_sha256'] == '5d3dd63b7b26a538431268c4ef5f1b0a565fde4e31135ae1aa735d79d1b3f662'
            and expected['old_terminal_receipt_sha256'] == '334b4163ab4229d38b4aaf7ad0c5fdbed27c1587b9aaacb37aa000de3c9a5f43', 'retained terminal lineage')
    return blobs, expected


def session(date):
    def ts(time):
        return int(datetime.fromisoformat(date + 'T' + time).replace(tzinfo=timezone.utc).timestamp())
    return dict(session_date=date, windows=[dict(first_close_time_utc=ts('04:10:00'), last_close_time_utc=ts('20:50:00'))])


def material(prior, z):
    """Reconstruct exactly the proposed eight replacement slots, no credentials."""
    payload = {name: z.read(artifact.P + 'payload/' + Path(name).name) for name in update.CHANGES[:3]}
    policy = update.strict_json(prior[update.CHANGES[3]])
    policy.update(schema_version=2, domain='stage8b-p1f-o2-materialization-policy-v2',
                  runtime_profile_id=artifact.PROFILE, runtime_profile_sha256=artifact.PROFILE_SHA,
                  bars_start_utc=CALENDAR['prior_dates'][0] + 'T04:00:00Z',
                  bars_end_utc=CALENDAR['execution_window_end_utc'])
    source = update.strict_json(prior[update.CHANGES[4]])
    source.update(schema_version=3, domain='moex.stage8b.p1e.first-boot-source-bundle.v3', runtime_profile_sha256=artifact.PROFILE_SHA)
    source['history_coverage'] = dict(source_mode='config-bound-explicit-session-windows-v1', sessions_sha256='0'*64,
                                    sessions=[session(d) for d in CALENDAR['prior_dates']], candidate_session=session(CALENDAR['session_date']))
    supervisor = update.strict_json(prior[update.CHANGES[5]])
    supervisor.update(runtime_profile_id=artifact.PROFILE, runtime_profile_sha256=artifact.PROFILE_SHA)
    supervisor['bootstrap']['runtime_config_fingerprint_sha256'] = FINGERPRINT
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
               update_revision='no-riskgate-ca1e5da-after-failed-4-v1')
    payload[update.custody.MANIFEST] = canonical(new)
    spec = dict(schema_version=1, domain='moex.o2.no-riskgate-installation.v1', execution_authorized=False,
                installation_authorized=False, target_mutation_performed=False,
                artifact_sha256=ARTIFACT_SHA, compiled_source_ref=artifact.source.SOURCE_REF,
                old_manifest_sha256=update.OLD_SHA, new_manifest_sha256=sha(payload[update.custody.MANIFEST]),
                old_inventory=old, preserved_installation_id=old['installation_id'],
                terminal_evidence_sha256=update.TERMINAL_SHA, source_plan_sha256=PLAN_SHA,
                profile_sha256=artifact.PROFILE_SHA, runtime_fingerprint=FINGERPRINT, calendar=CALENDAR,
                updates={name: dict(old_sha256=sha(prior[name]), new_sha256=sha(raw), size=len(raw)) for name, raw in payload.items()})
    return payload, spec


def validate_prepared(directory):
    expected_files = {'spec.json', 'source-plan-v3.json', 'binary-artifact.zip', 'prior-installation.zip', 'retained-terminal.zip', 'binary-artifact-acceptance.md'}
    expected_files |= {'payload/' + Path(p).name for p in update.CHANGES}
    require({p.relative_to(directory).as_posix() for p in directory.rglob('*') if p.is_file()} == expected_files, 'prepared inventory')
    require(not any(p.is_symlink() for p in directory.rglob('*')), 'prepared symlink')
    prior, expected = load_predecessor(directory / 'retained-terminal.zip', directory / 'prior-installation.zip')
    with pinned_zip(directory / 'binary-artifact.zip', ARTIFACT_SHA) as z:
        payload, spec = material(prior, z)
    # Pin equality, including exact JSON number types, not Python's True == 1.
    require((directory/'spec.json').read_bytes() == canonical(spec), 'spec exact binding')
    for name, raw in payload.items():
        require((directory/'payload'/Path(name).name).read_bytes() == raw, 'prepared slot binding: ' + name)
    require(sha((directory/'source-plan-v3.json').read_bytes()) == PLAN_SHA, 'source-plan V3 pin')
    require(sha((directory/'binary-artifact-acceptance.md').read_bytes()) == REVIEW_SHA, 'binary artifact acceptance pin')
    return spec


def prepare(handoff, review, output):
    require(sha(review.read_bytes()) == REVIEW_SHA, 'review pin')
    artifact.check(handoff / ARTIFACT)
    prior, _ = load_predecessor(handoff/TERMINAL, handoff/PRIOR)
    with pinned_zip(handoff/ARTIFACT, ARTIFACT_SHA) as z:
        payload, spec = material(prior, z)
    output.mkdir(parents=True, exist_ok=False)
    (output/'payload').mkdir()
    for name, raw in payload.items():
        (output/'payload'/Path(name).name).write_bytes(raw)
    for name, path in [('binary-artifact.zip', handoff/ARTIFACT), ('prior-installation.zip', handoff/PRIOR),
                       ('retained-terminal.zip', handoff/TERMINAL), ('source-plan-v3.json', ROOT/PLAN_PATH),
                       ('binary-artifact-acceptance.md', review)]:
        (output/name).write_bytes(path.read_bytes())
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
