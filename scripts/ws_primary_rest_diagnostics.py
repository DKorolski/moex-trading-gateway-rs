#!/usr/bin/env python3
"""Offline REST diagnostics, not a runtime admission or parity verdict.

Read-only journal inspection. V3 assessment.kind is distinct from transport
status and from provisional/committed basis. A failed diagnostic attestation
never changes a recorded model input or the runtime's nonblocking decision.
Canonical admission/hash verification remains the accepted journal verifier's
responsibility; this reader additionally binds committed comparisons to the
same label's preceding source -> step -> commit chain.
"""
import argparse
import datetime as dt
from decimal import Decimal
import hashlib
import json
from pathlib import Path

KINDS = ('Match', 'VolumeRevision', 'PriceDifference', 'InventoryDifference')
BASES = ('PROVISIONAL_WS', 'COMMITTED_MODEL_INPUT')
UNAVAILABLE = ('EvidenceUnavailable', 'Unavailable', 'CancelledForStop')


def unique_json(body):
    def pairs(items):
        out = {}
        for key, value in items:
            if key in out:
                raise ValueError('duplicate JSON key')
            out[key] = value
        return out
    return json.loads(body, object_pairs_hook=pairs,
                      parse_constant=lambda _: (_ for _ in ()).throw(ValueError('nonfinite JSON')))


def timestamp(value):
    parsed = dt.datetime.fromisoformat(value.replace('Z', '+00:00'))
    if parsed.tzinfo is None:
        raise ValueError('timestamp without timezone')
    return int(parsed.timestamp() * 1000)


def number(value):
    if not isinstance(value, (str, int)) or isinstance(value, bool):
        raise ValueError('not a decimal')
    n = Decimal(value)
    if not n.is_finite():
        raise ValueError('nonfinite decimal')
    return n


def bar_signature(bar):
    numeric = ('open', 'high', 'low', 'close', 'volume')
    return (json.dumps({k: v for k, v in bar.items() if k not in numeric}, sort_keys=True),
            *(number(bar[k]) for k in numeric))


def assess(expected, observed):
    """Same inventory/price/volume precedence as pure Rust rest_observation."""
    for bars in (expected, observed):
        if not isinstance(bars, list) or len(bars) > 10:
            raise ValueError('invalid M1 inventory')
        if len({b['open_ts'] for b in bars}) != len(bars):
            raise ValueError('duplicate minute')
        for b in bars:
            bar_signature(b)
    old = {b['open_ts']: b for b in expected}
    new = {b['open_ts']: b for b in observed}
    missing = [timestamp(b['open_ts']) for b in expected if b['open_ts'] not in new]
    extra = [timestamp(b['open_ts']) for b in observed if b['open_ts'] not in old]
    prices, volumes = [], []
    for b in expected:
        if b['open_ts'] not in new:
            continue
        n = new[b['open_ts']]
        if (any(b[k] != n[k] for k in ('instrument', 'timeframe_sec', 'open_ts', 'close_ts'))
                or any(number(b[k]) != number(n[k]) for k in ('open', 'high', 'low', 'close'))):
            prices.append(timestamp(b['open_ts']))
        if number(b['volume']) != number(n['volume']):
            volumes.append(timestamp(b['open_ts']))
    kind = ('InventoryDifference' if missing or extra else 'PriceDifference' if prices else
            'VolumeRevision' if volumes else 'Match')
    return dict(kind=kind, missing_known_m1_ms=missing, additional_rest_m1_ms=extra,
                price_changed_m1_ms=prices, volume_changed_m1_ms=volumes)


def committed_input(rows, before_index, label):
    """Never borrow a future commit or the next label's canonical bars."""
    source = step = None
    for row in rows[:before_index]:
        e = row.get('event', {})
        if row.get('kind') == 'canonical_source':
            receipt = e['receipt'].get('canonical', e['receipt'])
            if receipt.get('label_ms') == label:
                source = e
                step = None
        elif row.get('kind') == 'model_step' and e.get('model_label_utc_ms') == label:
            step = e
        elif row.get('kind') == 'canonical_commit' and e.get('model_label_utc_ms') == label:
            if (source is None or step is None or e.get('feedback_complete') is not True
                    or not isinstance(source.get('source_sha256'), str)
                    or e.get('source_sha256') != source['source_sha256']
                    or step.get('raw_provenance_sha256') != source['source_sha256']):
                return None
            receipt = source['receipt'].get('canonical', source['receipt'])
            if receipt.get('canonical_digest') != step.get('canonical_input_sha256'):
                return None
            return dict(source_sha256=source['source_sha256'], commit_sequence=row['sequence'],
                        bars=receipt.get('actual_m1', receipt.get('bars')))
    return None


def inspect_assessment(rows, index, event, a):
    detail = dict(label_ms=event.get('label_ms'), kind=a.get('kind'),
                  known_basis=a.get('known_basis'), integrity_valid=False,
                  committed_basis_attested=False, reason=None)
    try:
        label = event['label_ms']
        if type(label) is not int or type(a['label_ms']) is not int or a['label_ms'] != label:
            raise ValueError('ASSESSMENT_LABEL_MISMATCH')
        if a['known_basis'] not in BASES:
            raise ValueError('UNKNOWN_BASIS')
        if event.get('event') == 'primary_audit_result' and a['known_basis'] != 'COMMITTED_MODEL_INPUT':
            raise ValueError('AUDIT_REQUIRES_COMMITTED_BASIS')
        if event.get('model_input_rewritten', False) is not False or a['model_input_rewritten'] is not False:
            raise ValueError('REWRITE_CLAIM')
        computed = assess(a['expected_m1'], a['observed_m1'])
        if any(a.get(k) != v for k, v in computed.items()):
            raise ValueError('COMPARISON_NOT_REPRODUCED')
        committed = committed_input(rows, index, label)
        if a['known_basis'] == 'COMMITTED_MODEL_INPUT':
            if committed is None:
                raise ValueError('NO_PRECEDING_COMMIT_FOR_LABEL')
            if ([bar_signature(b) for b in a['expected_m1']]
                    != [bar_signature(b) for b in committed['bars']]):
                raise ValueError('COMMITTED_EXPECTED_MISMATCH')
            detail.update(committed_basis_attested=True,
                          committed_source_sha256=committed['source_sha256'],
                          commit_sequence=committed['commit_sequence'])
        elif committed is not None:
            raise ValueError('PROVISIONAL_AFTER_COMMIT')
        else:
            detail['reason'] = 'PROVISIONAL_VALUES_NOT_COMMITTED_ATTESTATION'
        detail['integrity_valid'] = True
    except (ValueError, KeyError, TypeError, ArithmeticError) as error:
        detail['reason'] = str(error)
    return detail


def summarize(rows):
    if len(rows) > 16385 or any(type(r.get('sequence')) is not int or r['sequence'] != i for i, r in enumerate(rows)):
        raise ValueError('invalid journal sequence or bounds')
    roles = {r: dict(scheduled_events=0, outcome_events=0, assessment_count=0,
        comparison_kinds={k: 0 for k in KINDS}, basis_counts={b: 0 for b in BASES},
        legacy_comparison_kinds={k: 0 for k in KINDS}, unavailable_events=0,
        unassessed_events=0, unknown_kind_assessments=0, assessment_integrity_failures=0,
        committed_basis_attestations=0, missing_outcomes=0) for r in ('Proof', 'Audit')}
    details, outcomes, scheduled = [], {r: set() for r in roles}, {r: set() for r in roles}
    audit_results = {r.get('event', {}).get('label_ms') for r in rows
                     if r.get('event', {}).get('event') == 'primary_audit_result'}
    for index, row in enumerate(rows):
        e = row.get('event', {})
        name = e.get('event')
        if name in ('primary_rest_proof_scheduled', 'primary_audit_scheduled'):
            role = 'Proof' if name == 'primary_rest_proof_scheduled' else 'Audit'
            roles[role]['scheduled_events'] += 1
            scheduled[role].add(e.get('label_ms'))
            continue
        role = ('Audit' if name == 'primary_audit_result' else
                'Proof' if name == 'primary_rest_proof_failed' else e.get('role'))
        if role not in roles or name not in ('primary_rest_observation', 'primary_audit_result',
                                             'primary_rest_nonblocking_failed', 'primary_rest_proof_failed'):
            continue
        # V3 Audit writes both a failure trace and an outcome; count once.
        if name == 'primary_rest_nonblocking_failed' and role == 'Audit' and e.get('label_ms') in audit_results:
            continue
        summary = roles[role]
        summary['outcome_events'] += 1
        outcomes[role].add(e.get('label_ms'))
        a = e.get('assessment')
        assessments = a if isinstance(a, list) else [a] if isinstance(a, dict) else []
        if not assessments:
            legacy = (name == 'primary_audit_result' and e.get('nonblocking') is not True
                      and 'assessment' not in e and e.get('status') in ('Match', 'VolumeRevision'))
            if legacy:
                summary['comparison_kinds'][e['status']] += 1
                summary['legacy_comparison_kinds'][e['status']] += 1
            elif e.get('status') in UNAVAILABLE or name in ('primary_rest_nonblocking_failed', 'primary_rest_proof_failed'):
                summary['unavailable_events'] += 1
            else:
                summary['unassessed_events'] += 1
            continue
        for a in assessments:
            summary['assessment_count'] += 1
            if not isinstance(a, dict):
                detail = dict(integrity_valid=False, committed_basis_attested=False, reason='ASSESSMENT_NOT_OBJECT')
            else:
                kind, basis = a.get('kind'), a.get('known_basis')
                if kind in KINDS:
                    summary['comparison_kinds'][kind] += 1
                else:
                    summary['unknown_kind_assessments'] += 1
                if basis in BASES:
                    summary['basis_counts'][basis] += 1
                detail = inspect_assessment(rows, index, e, a)
            summary['assessment_integrity_failures'] += not detail['integrity_valid']
            summary['committed_basis_attestations'] += detail['committed_basis_attested']
            details.append(dict(event=name, role=role, event_sequence=row['sequence'], **detail))
    for role, value in roles.items():
        value['missing_outcomes'] = len(scheduled[role] - outcomes[role])
    volumes = {r: value['comparison_kinds']['VolumeRevision'] for r, value in roles.items()}
    return dict(schema_version=1, status='OFFLINE_DIAGNOSTICS_NOT_RUNTIME_VERDICT', roles=roles,
        assessments=details, comparison_counts_are_reported_kinds_not_parity_attestations=True,
        volume_revision_proofs=volumes['Proof'], volume_revision_audits=volumes['Audit'],
        volume_revision_observed=any(volumes.values()),
        canonical_admission_validation='REQUIRES_ACCEPTED_JOURNAL_VERIFIER',
        model_impact='NOT_EVALUATED_REQUIRES_OFFLINE_REPLAY',
        runtime_verdict_changed=False, model_inputs_rewritten=False, runtime_rest_veto_added=False)


def read_journal(path):
    if path.stat().st_size > 64 * 1024 * 1024:
        raise ValueError('journal bounds')
    body = path.read_bytes()
    lines = body.splitlines()
    if any(len(line) > 1048576 for line in lines):
        raise ValueError('row bounds')
    return body, [unique_json(line) for line in lines]


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('journal', type=Path)
    args = parser.parse_args()
    body, rows = read_journal(args.journal)
    print(json.dumps(dict(journal_sha256=hashlib.sha256(body).hexdigest(), **summarize(rows)), indent=2))
