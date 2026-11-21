"""Offline targeted reader tests. Optional immutable fixture replay via env."""
import copy
import hashlib
import json
import os
from pathlib import Path
import unittest
import zipfile

import ws_primary_rest_diagnostics as reader

T = 1794283200000


def bar(volume='100'):
    return dict(instrument=dict(exchange='Moex', market='Futures', symbol='IMOEXF', venue_symbol='IMOEXF@RTSX'),
                timeframe_sec=60, open_ts='2026-11-10T04:00:00Z', close_ts='2026-11-10T04:01:00Z',
                open='2469', high='2470.5', low='2461.5', close='2464.5', volume=volume,
                is_final=False, source_kind='LiveStream')


def assessment(kind='Match', basis='COMMITTED_MODEL_INPUT'):
    old = [bar()]
    new = [bar('99' if kind == 'VolumeRevision' else '100')]
    return dict(label_ms=T, known_basis=basis, expected_m1=old, observed_m1=new,
                model_input_rewritten=False, model_impact='NOT_EVALUATED_REQUIRES_OFFLINE_REPLAY',
                **reader.assess(old, new))


def chain():
    return [
        dict(kind='canonical_source', event=dict(source_sha256='a' * 64,
            receipt=dict(label_ms=T, canonical_digest='b' * 64, actual_m1=[bar()]))),
        dict(kind='model_step', event=dict(model_label_utc_ms=T,
            raw_provenance_sha256='a' * 64, canonical_input_sha256='b' * 64)),
        dict(kind='canonical_commit', event=dict(model_label_utc_ms=T,
            source_sha256='a' * 64, feedback_complete=True)),
    ]


def audit(a=None, status='EvidenceOnly'):
    return dict(kind='transport_event', event=dict(event='primary_audit_result', label_ms=T,
                status=status, nonblocking=True, assessment=a))


def run(rows):
    rows = copy.deepcopy(rows)
    for i, row in enumerate(rows):
        row['sequence'] = i
    before = json.dumps(rows, sort_keys=True)
    result = reader.summarize(rows)
    assert json.dumps(rows, sort_keys=True) == before
    assert result['runtime_verdict_changed'] is False
    assert result['model_inputs_rewritten'] is False
    assert result['runtime_rest_veto_added'] is False
    return result


class Diagnostics(unittest.TestCase):
    def test_v3_evidence_only_volume_revision_is_counted_by_kind(self):
        r = run(chain() + [audit(assessment('VolumeRevision'))])
        self.assertEqual(r['volume_revision_audits'], 1)
        self.assertTrue(r['volume_revision_observed'])
        self.assertEqual(r['roles']['Audit']['committed_basis_attestations'], 1)

    def test_assessment_object_and_list_have_same_counts(self):
        a = assessment()
        for value in (a, [a]):
            r = run(chain() + [audit(value)])
            self.assertEqual(r['roles']['Audit']['comparison_kinds']['Match'], 1)
            self.assertEqual(r['roles']['Audit']['committed_basis_attestations'], 1)

    def test_multiple_assessments_are_counted_separately(self):
        r = run(chain() + [audit([assessment(), assessment('VolumeRevision')])])
        self.assertEqual(r['roles']['Audit']['assessment_count'], 2)
        self.assertEqual(r['volume_revision_audits'], 1)

    def test_unavailable_without_assessment_is_not_match(self):
        e = audit(status='EvidenceUnavailable')
        del e['event']['assessment']
        r = run([e])
        self.assertEqual(r['roles']['Audit']['unavailable_events'], 1)
        self.assertEqual(sum(r['roles']['Audit']['comparison_kinds'].values()), 0)

    def test_null_empty_and_absent_assessments_are_unassessed(self):
        for value in (None, [], 'absent'):
            e = audit(value)
            if value == 'absent':
                del e['event']['assessment']
            r = run([e])
            self.assertEqual(r['roles']['Audit']['unassessed_events'], 1)
            self.assertEqual(r['roles']['Audit']['comparison_kinds']['Match'], 0)

    def test_unknown_kind_is_not_match(self):
        a = assessment()
        a['kind'] = 'FutureKind'
        r = run(chain() + [audit(a)])
        self.assertEqual(r['roles']['Audit']['unknown_kind_assessments'], 1)
        self.assertEqual(r['roles']['Audit']['assessment_integrity_failures'], 1)
        self.assertEqual(r['roles']['Audit']['comparison_kinds']['Match'], 0)

    def test_wrong_committed_expected_removes_attestation_not_runtime_verdict(self):
        a = assessment()
        a['expected_m1'][0]['volume'] = '101'
        a.update(reader.assess(a['expected_m1'], a['observed_m1']))
        r = run(chain() + [audit(a)])
        self.assertEqual(r['roles']['Audit']['committed_basis_attestations'], 0)
        self.assertEqual(r['assessments'][0]['reason'], 'COMMITTED_EXPECTED_MISMATCH')

    def test_future_commit_cannot_attest_earlier_comparison(self):
        r = run([audit(assessment())] + chain())
        self.assertEqual(r['assessments'][0]['reason'], 'NO_PRECEDING_COMMIT_FOR_LABEL')
        self.assertEqual(r['roles']['Audit']['committed_basis_attestations'], 0)

    def test_wrong_label_and_unknown_basis_remove_attestation(self):
        for key, value in (('label_ms', T + 600000), ('known_basis', 'invented')):
            a = assessment()
            a[key] = value
            r = run(chain() + [audit(a)])
            self.assertEqual(r['roles']['Audit']['assessment_integrity_failures'], 1)
            self.assertEqual(r['roles']['Audit']['committed_basis_attestations'], 0)

    def test_provisional_proof_is_separate_from_committed_attestation(self):
        e = dict(kind='transport_event', event=dict(event='primary_rest_observation', role='Proof',
                 label_ms=T, nonblocking=True, assessment=assessment(basis='PROVISIONAL_WS')))
        r = run([e] + chain())
        self.assertEqual(r['roles']['Proof']['comparison_kinds']['Match'], 1)
        self.assertEqual(r['roles']['Proof']['committed_basis_attestations'], 0)
        self.assertTrue(r['assessments'][0]['integrity_valid'])
        r = run(chain() + [e])
        self.assertEqual(r['assessments'][0]['reason'], 'PROVISIONAL_AFTER_COMMIT')

    def test_proof_can_attest_correct_preceding_committed_input(self):
        e = dict(kind='transport_event', event=dict(event='primary_rest_observation', role='Proof',
                 label_ms=T, nonblocking=True, assessment=assessment()))
        r = run(chain() + [e])
        self.assertEqual(r['roles']['Proof']['committed_basis_attestations'], 1)

    def test_audit_cannot_attest_provisional_basis(self):
        r = run([audit(assessment(basis='PROVISIONAL_WS'))])
        self.assertEqual(r['assessments'][0]['reason'], 'AUDIT_REQUIRES_COMMITTED_BASIS')

    def test_wrong_source_step_commit_hashes_remove_attestation(self):
        for index, key in ((1, 'raw_provenance_sha256'), (1, 'canonical_input_sha256'), (2, 'source_sha256')):
            rows = chain()
            rows[index]['event'][key] = 'c' * 64
            r = run(rows + [audit(assessment())])
            self.assertEqual(r['roles']['Audit']['committed_basis_attestations'], 0)

    def test_legacy_status_only_for_actual_legacy_comparison(self):
        e = dict(kind='transport_event', event=dict(event='primary_audit_result', label_ms=T, status='VolumeRevision'))
        r = run([e])
        self.assertEqual(r['volume_revision_audits'], 1)
        self.assertEqual(r['roles']['Audit']['legacy_comparison_kinds']['VolumeRevision'], 1)
        e['event']['nonblocking'] = True
        r = run([e])
        self.assertEqual(r['volume_revision_audits'], 0)
        self.assertEqual(r['roles']['Audit']['unassessed_events'], 1)

    def test_missing_outcome_is_not_success(self):
        e = dict(kind='transport_event', event=dict(event='primary_audit_scheduled', label_ms=T))
        r = run([e])
        self.assertEqual(r['roles']['Audit']['missing_outcomes'], 1)
        self.assertEqual(r['roles']['Audit']['comparison_kinds']['Match'], 0)

    def test_audit_failure_trace_and_outcome_count_once(self):
        trace = dict(kind='transport_event', event=dict(event='primary_rest_nonblocking_failed',
                     role='Audit', label_ms=T, nonblocking=True, **{'class': 'Integrity'}))
        r = run([trace, audit(status='EvidenceUnavailable')])
        self.assertEqual(r['roles']['Audit']['outcome_events'], 1)
        self.assertEqual(r['roles']['Audit']['unavailable_events'], 1)

    def test_unavailable_and_cancelled_take_precedence_over_retained_assessment(self):
        for status in ('EvidenceUnavailable', 'Unavailable', 'CancelledForStop'):
            for as_list in (False, True):
                for with_trace in (False, True):
                    for kind in ('Match', 'VolumeRevision'):
                        with self.subTest(status=status, as_list=as_list, trace=with_trace, kind=kind):
                            a = assessment(kind)
                            e = audit([a] if as_list else a, status=status)
                            e['event'].update(source_sha256=None, **{'class': 'Integrity'})
                            trace = dict(kind='transport_event', event=dict(event='primary_rest_nonblocking_failed',
                                role='Audit', label_ms=T, nonblocking=True, **{'class': 'Integrity'}))
                            r = run(chain() + ([trace] if with_trace else []) + [e])
                            summary = r['roles']['Audit']
                            self.assertEqual(summary['outcome_events'], 1)
                            self.assertEqual(summary['unavailable_events'], 1)
                            self.assertEqual(sum(summary['comparison_kinds'].values()), 0)
                            self.assertEqual(summary['committed_basis_attestations'], 0)
                            self.assertFalse(r['volume_revision_observed'])
                            d = r['assessments'][0]
                            self.assertFalse(d['integrity_valid'])
                            self.assertFalse(d['committed_basis_attested'])
                            self.assertEqual(d['status'], status)
                            self.assertEqual(d['failure_class'], 'Integrity')
                            self.assertEqual(d['reported_kind'], kind)
                            self.assertEqual(d['classification'], 'UNAVAILABLE_UNATTESTED')

    def test_failure_events_cannot_attest_retained_assessments(self):
        for name in ('primary_rest_nonblocking_failed', 'primary_rest_proof_failed'):
            for role in ('Proof', 'Audit'):
                if name == 'primary_rest_proof_failed' and role != 'Proof':
                    continue
                for as_list in (False, True):
                    with self.subTest(name=name, role=role, as_list=as_list):
                        a = assessment()
                        e = dict(kind='transport_event', event=dict(event=name, role=role, label_ms=T,
                            assessment=[a] if as_list else a, **{'class': 'Cancelled'}))
                        r = run(chain() + [e])
                        self.assertEqual(r['roles'][role]['unavailable_events'], 1)
                        self.assertEqual(sum(r['roles'][role]['comparison_kinds'].values()), 0)
                        self.assertFalse(r['assessments'][0]['integrity_valid'])
                        self.assertEqual(r['assessments'][0]['failure_class'], 'Cancelled')

    def test_failure_trace_does_not_disappear_behind_contradictory_available_audit(self):
        trace = dict(kind='transport_event', event=dict(event='primary_rest_nonblocking_failed',
                     role='Audit', label_ms=T, **{'class': 'Integrity'}))
        for status in ('EvidenceOnly', 'Match'):
            r = run(chain() + [trace, audit(assessment(), status=status)])
            self.assertEqual(r['roles']['Audit']['unavailable_events'], 1)
            self.assertEqual(r['roles']['Audit']['outcome_events'], 1)
            self.assertEqual(r['roles']['Audit']['comparison_kinds']['Match'], 0)
            self.assertEqual(r['roles']['Audit']['committed_basis_attestations'], 0)
            self.assertEqual(r['assessments'][0]['failure_trace_classes'], ['Integrity'])

    @unittest.skipUnless(os.environ.get('FINAM_BOUNDED_RESULT_ZIP'), 'set immutable public fixture path')
    def test_real_bounded_fixture_and_original_verdict_unchanged(self):
        body = Path(os.environ['FINAM_BOUNDED_RESULT_ZIP']).read_bytes()
        self.assertEqual(hashlib.sha256(body).hexdigest(),
                         '79e1e7b3961e628077b59e1c29205f8706401d50a0dd174b94e5c0c6bfc7e601')
        with zipfile.ZipFile(os.environ['FINAM_BOUNDED_RESULT_ZIP']) as z:
            raw = z.read('state/observations/journal.jsonl')
            original_result = z.read('state/result.json')
            rows = [reader.unique_json(l) for l in raw.splitlines()]
            r = reader.summarize(rows)
            self.assertEqual(r['roles']['Proof']['comparison_kinds']['Match'], 1)
            self.assertEqual(r['roles']['Proof']['comparison_kinds']['VolumeRevision'], 3)
            self.assertEqual(r['roles']['Audit']['comparison_kinds']['Match'], 1)
            self.assertEqual(r['roles']['Audit']['committed_basis_attestations'], 1)
            self.assertEqual(r['roles']['Proof']['committed_basis_attestations'], 0)
            self.assertTrue(r['volume_revision_observed'])
            self.assertEqual(r['volume_revision_proofs'], 3)
            self.assertEqual(r['volume_revision_audits'], 0)
            self.assertEqual(json.loads(original_result)['status'], 'BOUNDED_EVIDENCE_READY_FOR_REVIEW')
            self.assertEqual(z.read('state/result.json'), original_result)
        self.assertEqual(hashlib.sha256(Path(os.environ['FINAM_BOUNDED_RESULT_ZIP']).read_bytes()).hexdigest(),
                         hashlib.sha256(body).hexdigest())


if __name__ == '__main__':
    unittest.main(verbosity=2)
