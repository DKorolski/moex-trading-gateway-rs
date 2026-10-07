#!/usr/bin/env python3
"""Full accepted artifact successor after EXPIRED/1/12. Offline preparation only; no signing/install/activation.

Compact predecessor extracts are authenticated by the accepted manifest hash
and exact retained observation hashes, not a self-declared extract inventory.
"""
import argparse
import copy
import io
from datetime import datetime, timezone
import json
from pathlib import Path
import zipfile

import stage8b_p1f_o2_abort_artifact as qualified
import stage8b_p1f_o2_v4_timestamp_artifact as helpers
artifact = helpers.base
COMPILED_REF = '21cc7eb9530ccc4480bae2a4f8264705a7383c01'
import stage8b_p1f_o2_terminal_install as update

ROOT = Path(__file__).resolve().parents[1]
ARTIFACT = 'moex-trading-project-21cc7eb-o2-abort-artifact-review.zip'
ARTIFACT_SHA = 'ef8efa957222960b6b9cb9c0d75668fd8bc13d9b6d4ab8977270bb8f7a3ca73d'
PRIOR = 'moex-trading-project-e05b4bf-o2-template-installation.zip'
PRIOR_SHA = 'b163f18df3e29937fe516e0870a45bdd966337a1f108cb4be71ba172b48288cc'

REVIEW_SHA = 'ab233eeacee8b69f187806b0af1fbeca2fd94210582e24f4e887dac9ec91dc77'
PLAN_PATH = 'crates/runtime-durable-service/src/stage8b_p1e_first_boot_source/observed.rs'
PLAN_SHA = '8f3cac0b35529301ef2ea056a2e1ab00db0aa5e955786b8141cf1ef701d1e61d'
IDENTITY = 'fbd9ceb7ee964944e79db559c4e4d5cc7af54578b653ce218e183d01fa1444c3'
FINGERPRINT = '665f18112142f6c9aee1b613e341870f0d559d783fe41df725e26415507e6290'
TERMINAL_MEMBERS = {
    'snapshot.stdout': '433905a1275c2aae8ba3576425916ca28aa982026924318e80617d14ab7c07d1',
    'snapshot.record.json': '5ee60664cd15d574d5cb618415045a5404205db25ae7a531dc50d9083017216f',
    'snapshot.stderr': 'e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855',
    'bounded-result.zip': update.TERMINAL_SHA,
    'snapshot_readonly.py': '21d96b66d8268d3e36d60f0873c85af6f96bcceaccf03a406ba55a65ad7d8b46',
    'capture_readonly.py': '1893791c2d651f66c31c0bdd1c20b3a4306310b1cb2bfd5bd47e1fe3a903fab3',
}
CALENDAR = dict(session_date='2026-10-08', prior_dates=['2026-10-02', '2026-10-05', '2026-10-06', '2026-10-07'],
                execution_window_start_utc='2026-10-08T04:10:00Z', execution_window_end_utc='2026-10-08T20:50:00Z',
                candidate_cutoff_rule='latest complete canonical M10 close at or before trusted_now',
                candidate_max_age_seconds=900, broker_truth_max_age_seconds=300,
                outside_window='REJECT; explicitly rebind calendar, never auto-roll or reuse authorization',
                calendar_sources=['https://www.moex.com/n101220', 'https://www.moex.com/n103379'],
                prior_session_selection='four explicit weekday sessions; no synthetic weekend intervals',
                declared_intraday_breaks=[], execution_authorized=False,
                operational_revalidation_required=True)
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
    snapshot = update.strict_json(evidence['snapshot.stdout'])
    record = update.strict_json(evidence['snapshot.record.json'])
    require(record['exit_code'] == 0 and record['stdout_sha256'] == sha(evidence['snapshot.stdout'])
            and record['stderr_sha256'] == sha(evidence['snapshot.stderr']), 'snapshot capture binding')
    require(snapshot['status'] == 'PASS' and snapshot['installation_sha256'] == update.OLD_SHA
            and snapshot['installation_manifest_text'].encode() == raw
            and all(snapshot[k] is False for k in ('redis_contact', 'finam_contact', 'secret_values_exported', 'mutation_performed')), 'native snapshot scope')
    expected = snapshot['expected']
    head = expected['history_head']
    require(head['state'] == 'EXPIRED' and type(head['latest_sequence']) is int and head['latest_sequence'] == 12
            and type(head['authority_generation']) is int and head['authority_generation'] == 1
            and head['latest_event_sha256'] == 'f245a28a65b73dbeeb6e91bc911befe48660db4be985ebe65136bc383d1defac'
            and head['active_manifest_sha256'] is None and head['deadline_utc'] is None, 'retained terminal lineage')
    with zipfile.ZipFile(io.BytesIO(evidence['bounded-result.zip'])) as z:
        result = update.strict_json(z.read('post/stdout.json'))
    proof = result['evidence']
    require(result['status'] == 'PASS_TERMINAL_RECOVERY' and not result['failures'] and proof['head'] == head, 'accepted terminal head')
    # Exact accepted post-recovery control inventory, including source archive's
    # original 0400 custody, not just event/receipt hashes or PASS flags.
    require(expected['authority_inventory'] == proof['control_tree'], 'accepted complete control preservation')
    for directory, key in [('/var/lib/moex-finam-p1-paper/state', 'durable_inventory'), *update.RETAINED_DIRS]:
        if directory in proof['trees']:
            require(expected[key] == proof['trees'][directory], 'accepted retained tree preservation')
    phase = '992be63a6406153eb5bab43d746927df598d59e24df2d113ae12c605752b80b5'
    require(expected['staging_inventory'][phase + '.json'] == proof['staged'], 'retained materializer diagnostic')
    # Fresh config and scheduler inventories are additionally pinned by the raw
    # snapshot digest above; they cannot be supplied or rebased by the installer.
    return blobs, expected


def session(date):
    def ts(time):
        return int(datetime.fromisoformat(date + 'T' + time).replace(tzinfo=timezone.utc).timestamp())
    return dict(session_date=date, windows=[dict(first_close_time_utc=ts('04:10:00'), last_close_time_utc=ts('20:50:00'))])


def supervisor_template_bytes(value):
    """Guardian parses this slot as canonical serde_json::Value, without LF.

    Do not change the newline-terminated inventory/policy/history encoding.
    Exact emitted bytes, including this distinction, belong in hash bindings.
    """
    return json.dumps(value, sort_keys=True, separators=(',', ':'),
                      ensure_ascii=False, allow_nan=False).encode('utf-8')


def material(prior, z):
    payload = {name: z.read(qualified.HANDOFF_PREFIX + 'payload/' + Path(name).name) for name in update.CHANGES[:3]}
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
        payload[path] = supervisor_template_bytes(value) if path == update.CHANGES[5] else canonical(value)
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
    new.update(artifact_ref=COMPILED_REF, artifact_sha256=ARTIFACT_SHA,
               installer_sha256=sha(Path(update.__file__).read_bytes()), predecessor_manifest_sha256=update.OLD_SHA,
               update_revision='terminal-successor-21cc7eb-after-expired-12-v1')
    payload[update.custody.MANIFEST] = canonical(new)
    spec = dict(schema_version=1, domain='moex.o2.terminal-successor-installation.v1', execution_authorized=False,
                installation_authorized=False, target_mutation_performed=False,
                artifact_sha256=ARTIFACT_SHA, compiled_source_ref=COMPILED_REF,
                old_manifest_sha256=update.OLD_SHA, new_manifest_sha256=sha(payload[update.custody.MANIFEST]),
                old_inventory=old, preserved_installation_id=old['installation_id'],
                prior_full_archive_sha256=PRIOR_SHA, terminal_evidence_sha256=update.TERMINAL_SHA,
                terminal_member_sha256=TERMINAL_MEMBERS, source_plan_sha256=PLAN_SHA,
                profile_sha256=artifact.PROFILE_SHA, runtime_fingerprint=FINGERPRINT, calendar=CALENDAR,
                operational_identity_sha256=IDENTITY, root_migration_authorized=False,
                expected_terminal='EXPIRED/1/12',
                updates={name: dict(old_sha256=sha(prior[name]), new_sha256=sha(raw), size=len(raw)) for name, raw in payload.items()})
    return payload, spec


def validate_prepared(directory):
    expected_files = {'spec.json', 'sparse-policy-contract.rs', 'binary-artifact.zip', 'prior-installation.zip', 'retained-terminal.zip', 'terminal-recovery-acceptance.txt'}
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
    require(sha((directory/'terminal-recovery-acceptance.txt').read_bytes()) == REVIEW_SHA, 'binary artifact acceptance pin')
    return spec


def prepare(handoff, review, observation, bounded_evidence, output):
    require(sha(review.read_bytes()) == REVIEW_SHA, 'review pin')
    qualified.check(handoff / ARTIFACT)
    # The accepted old installation ZIP binds the complete old public payload.
    # Retain a compact extract, not recursively nested multi-stage archives.
    import io
    with pinned_zip(handoff/PRIOR, PRIOR_SHA) as z:
        with zipfile.ZipFile(io.BytesIO(z.read('installation-template-encoding/prior-installation.zip'))) as compact:
            prior_files = {n: compact.read(n) for n in compact.namelist()}
        for path in update.CHANGES:
            prior_files[path.lstrip('/')] = z.read('installation-template-encoding/payload/' + Path(path).name)
        raw = prior_files[update.custody.MANIFEST.lstrip('/')]
        require(sha(raw) == update.OLD_SHA, 'old manifest')
    retained = {n: (bounded_evidence if n == 'bounded-result.zip' else observation/n).read_bytes()
                for n in TERMINAL_MEMBERS}
    require(all(sha(retained[n]) == v for n, v in TERMINAL_MEMBERS.items()), 'observation pins')
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
    (output/'terminal-recovery-acceptance.txt').write_bytes(review.read_bytes())
    (output/'spec.json').write_bytes(canonical(spec))
    validate_prepared(output)
    print(json.dumps(dict(result='PREPARED_NOT_INSTALLED', write_slots=len(payload),
        changed_slots=sum(v['old_sha256'] != v['new_sha256'] for v in spec['updates'].values()),
        new_manifest_sha256=spec['new_manifest_sha256'])))


if __name__ == '__main__':
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('action', choices=('prepare', 'check'))
    p.add_argument('directory', type=Path)
    p.add_argument('--handoff', type=Path, default=ROOT/'reports/handoff')
    p.add_argument('--review', type=Path)
    p.add_argument('--observation', type=Path)
    p.add_argument('--bounded-evidence', type=Path)
    a = p.parse_args()
    if a.action == 'prepare':
        prepare(a.handoff, a.review, a.observation, a.bounded_evidence, a.directory)
    else:
        print(json.dumps(validate_prepared(a.directory), sort_keys=True))
