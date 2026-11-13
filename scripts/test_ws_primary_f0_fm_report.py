"""Synthetic reader tests only; no runtime, broker or live qualification."""
import copy
import csv
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

import ws_primary_f0_fm_report as r

T=1794542400000
S='a'*64
C='b'*64


def state(position=0,pending=None):
    return dict(last_position_qty=position,pending_entry_request_id=pending,pending_exit_request_id=None)


def seed():
    return dict(kind='seed',observation=dict(mode='WARMUP',run_id='SYNTHETIC_F0_FM',segment_id='new',
        seed_sha256='d'*64,model_sha256='e'*64,config_sha256='f'*64,virtual_position_lots=0))


def day(labels=None):
    return r.encode(dict(schema_version=1,session_date='2026-11-13',required_m10_labels_ms=labels or [T,T+r.M10]))


def chain(label=T,action=None,seq=1,before=0,origin='REST_EXCEPTION'):
    close=label+r.M10
    receipt=dict(label_ms=label,close_ms=close,canonical_digest=C,origin=origin,reason='StartupPartial' if origin=='REST_EXCEPTION' else None,
        actual_m1=[dict(close='1000.5') for _ in range(10)],minute_bitmap=1023,generation=1,canonical_ready_at_ms=close+1000)
    virtual=origin=='RECOVERY_BACKFILL';mode='BACKFILL_REPLAY' if virtual else 'LIVE_OBSERVED'
    clock=close if virtual else close+1100
    wrapped=dict(canonical=receipt,replay_deadline_ms=close+15000) if virtual else receipt
    rows=[dict(kind='canonical_source',event=dict(source_sha256=S,receipt=wrapped)),
        dict(kind='model_step',event=dict(mode=mode,instrument='IMOEXF',model_label_utc_ms=label,
            nominal_close_utc_ms=close,canonical_input_sha256=C,raw_provenance_sha256=S,
            received_at_ms=clock,canonical_ready_at_ms=clock,model_start_at_ms=clock,result_at_ms=clock,
            before=state(before),after=state(before,'SYNTHETIC_INTENT' if action else None),
            decision=dict(action=action,quantity_lots=1 if action else 0,reason='bo_entry' if action else 'before_wait')))]
    if action:
        position={'EnterLong':1,'EnterShort':-1,'Exit':0}[action]
        fill=dict(policy=r.CANDLE_POLICY,request=dict(sequence=seq,instrument='IMOEXF',action=action,quantity_lots=1,decision_at_ms=clock),
            model_label_utc_ms=label,nominal_close_utc_ms=close,received_at_ms=clock,applied_at_ms=clock,
            canonical_input_sha256=C,raw_provenance_sha256=S,price_ticks=2001,position_lots=position)
        rows.append(dict(kind='hypothetical_candle_feedback',event=dict(mode=mode,fill=fill,state_after=state(position))))
    rows.append(dict(kind='canonical_commit',event=dict(model_label_utc_ms=label,source_sha256=S,
        model_result_at_ms=close+1101,publication_checked_at_ms=close+1102,feedback_complete=True)))
    return rows


def body(rows):
    return b''.join(r.encode(dict(row,sequence=i))+b'\n' for i,row in enumerate(rows))


def report(rows,labels=None,**kw):
    return r.analyze(body(rows),day(labels),evidence_kind='synthetic_fixture',**kw)


class F0FM(unittest.TestCase):
    def test_empty_is_not_no_action_or_full_coverage(self):
        s,rows=report([])
        self.assertEqual(s['committed_bound_chains'],0)
        self.assertEqual(s['reported_model_steps'],0)
        self.assertEqual(s['decision_reasons'],{})
        self.assertEqual(s['status'],'PARTIAL_OBSERVATION')
        self.assertIsNone(rows[0]['decision'])
        self.assertEqual(rows[0]['phases']['state_after_feedback_missing_reason'],'NO_MODEL_STEP_OBSERVED')

    def test_no_action_has_reason_and_separate_missing_feedback_phase(self):
        s,rows=report([seed()]+chain())
        self.assertEqual(s['committed_bound_chains'],1)
        self.assertEqual(rows[0]['decision']['reason'],'before_wait')
        self.assertIsNone(rows[0]['phases']['state_after_feedback'])
        self.assertEqual(rows[0]['phases']['state_after_feedback_missing_reason'],'NO_INTENT_NO_FEEDBACK_EXPECTED')

    def test_exact_row_offsets_hashes_and_no_input_mutation(self):
        original=body([seed()]+chain());saved=original[:]
        s,rows=r.analyze(original,day(),evidence_kind='observed_runtime')
        ref=rows[0]['refs']['step'][0]
        line=original[ref['byte_offset']:ref['byte_offset']+ref['bytes']]
        self.assertEqual(r.sha(line),ref['line_sha256'])
        self.assertEqual(json.loads(line)['sequence'],ref['sequence'])
        self.assertEqual(original,saved)
        self.assertEqual(s['journal']['original_sha256'],r.sha(original))

    def test_calendar_not_found_rows_determines_coverage(self):
        s,_=report([seed()]+chain(),labels=[T,T+r.M10,T+2*r.M10])
        self.assertEqual(s['expected_labels'],3)
        self.assertEqual(s['committed_bound_chains'],1)

    def test_missing_due_and_future_are_distinct(self):
        _,rows=report([seed()],observed_until_ms=T+r.M10+1)
        self.assertEqual([x['status'] for x in rows],['MISSING_DUE_AS_OF_SNAPSHOT','NOT_YET_DUE'])

    def test_incomplete_chain_does_not_gain_commit(self):
        s,rows=report([seed()]+chain()[:-1])
        self.assertEqual(s['committed_bound_chains'],0)
        self.assertEqual(rows[0]['status'],'INCOMPLETE_CHAIN')

    def test_duplicated_source_step_commit_are_findings(self):
        for index in (0,1,2):
            rows=chain();rows.insert(index,copy.deepcopy(rows[index]))
            with self.subTest(index=index):
                s,out=report([seed()]+rows)
                self.assertEqual(s['status'],'FINDINGS')
                self.assertEqual(s['committed_bound_chains'],0)
                self.assertTrue(out[0]['findings'][0].startswith('DUPLICATE_'))

    def test_hash_mismatch_and_wrong_order(self):
        for kind in ('hash','order'):
            rows=chain()
            if kind=='hash':rows[-1]['event']['source_sha256']='c'*64
            else:rows[0],rows[1]=rows[1],rows[0]
            with self.subTest(kind=kind):
                s,_=report([seed()]+rows)
                self.assertEqual(s['status'],'FINDINGS')

    def test_event_sequence_gap_and_duplicate_rejected(self):
        rows=[dict(seed(),sequence=0),dict(kind='transport_event',sequence=2,event={})]
        for n in (0,2):
            rows[-1]['sequence']=n
            with self.assertRaisesRegex(r.Refused,'JOURNAL_SEQUENCE'):
                r.journal_rows(b''.join(r.encode(x)+b'\n' for x in rows))

    def test_bad_complete_line_never_becomes_partial_success(self):
        with self.assertRaisesRegex(r.Refused,'MALFORMED_JSON'):
            r.journal_rows(body([seed()])+b'{broken}\n',allow_prefix=True)

    def test_tail_is_explicit_prefix_only_and_original_hash_is_preserved(self):
        prefix=body([seed()]+chain());original=prefix+b'{"sequence":4'
        with self.assertRaisesRegex(r.Refused,'UNTERMINATED'):
            r.journal_rows(original)
        s,_=r.analyze(original,day(),evidence_kind='observed_runtime',allow_prefix=True)
        self.assertEqual(s['journal']['original_sha256'],r.sha(original))
        self.assertEqual(s['journal']['analyzed_prefix_sha256'],r.sha(prefix))
        self.assertEqual(s['journal']['ignored_unterminated_tail_bytes'],len(original)-len(prefix))

    def test_valid_but_unterminated_last_row_is_also_uncommitted_prefix(self):
        complete=body([seed()]+chain())
        s,_=r.analyze(complete[:-1],day(),evidence_kind='observed_runtime',allow_prefix=True)
        self.assertEqual(s['committed_bound_chains'],0)

    def test_duplicate_json_nonfinite_and_overflow_rejected(self):
        for value in (b'{"x":1,"x":2}',b'{"x":NaN}',b'{"x":Infinity}',b'{"x":1e999}'):
            with self.subTest(value=value),self.assertRaises(r.Refused):r.unique_json(value)

    def test_invalid_calendar_and_event_shapes_rejected(self):
        for value in (dict(schema_version=True,required_m10_labels_ms=[T]),
                      dict(schema_version=1,required_m10_labels_ms=[T,T])):
            with self.assertRaises(r.Refused):r.model_day(r.encode(value))
        for row in (dict(kind='canonical_source',event=dict(receipt=[])),
                    dict(kind='model_step',event=dict(model_label_utc_ms=T,decision=[])),
                    dict(kind='hypothetical_candle_feedback',event=dict(fill={})),
                    dict(kind='canonical_commit',event={})):
            with self.assertRaises(r.Refused):report([seed(),row])

    def test_long_short_exit_have_correct_phase_and_policy(self):
        for action,before,after in (('EnterLong',0,1),('EnterShort',0,-1),('Exit',1,0)):
            with self.subTest(action=action):
                s,rows=report([seed()]+chain(action=action,before=before))
                self.assertEqual(s['findings'],[])
                self.assertEqual(rows[0]['phases']['state_after_decision']['last_position_qty'],before)
                self.assertEqual(rows[0]['phases']['state_after_feedback']['last_position_qty'],after)
                self.assertEqual(rows[0]['feedback']['fill']['policy'],r.CANDLE_POLICY)

    def test_feedback_missing_duplicate_or_without_intent(self):
        rows=chain(action='EnterLong');missing=rows[:2]+rows[-1:]
        duplicate=rows[:3]+[copy.deepcopy(rows[2])]+rows[3:]
        without=copy.deepcopy(rows);without[1]['event']['decision'].update(action=None,quantity_lots=0)
        for value in (missing,duplicate,without):
            with self.subTest(value=value):
                s,_=report([seed()]+value)
                self.assertEqual(s['status'],'FINDINGS')

    def test_phase_swap_or_broken_fill_hash_is_visible(self):
        for case in ('phase','hash','price','pending','policy','causality','instrument','mode','decision_time'):
            rows=chain(action='EnterLong');feedback=rows[2]['event']
            if case=='phase':rows[1]['event']['after']=state(1)
            elif case=='hash':feedback['fill']['canonical_input_sha256']='0'*64
            elif case=='price':feedback['fill']['price_ticks']=2002
            elif case=='pending':feedback['state_after']['pending_entry_request_id']='still-pending'
            elif case=='policy':feedback['fill']['policy']='other'
            elif case=='instrument':feedback['fill']['request']['instrument']='OTHER'
            elif case=='mode':feedback['mode']='BACKFILL_REPLAY'
            elif case=='decision_time':feedback['fill']['request']['decision_at_ms']+=1
            else:feedback['fill']['applied_at_ms']=T
            with self.subTest(case=case):
                s,_=report([seed()]+rows)
                self.assertEqual(s['status'],'FINDINGS')

    def test_repeated_fill_sequence_does_not_count_as_once(self):
        s,_=report([seed()]+chain(action='EnterLong')+chain(T+r.M10,action='Exit',before=1,seq=1))
        self.assertIn('REPEATED_FILL_SEQUENCE',[v['code'] for v in s['findings']])

    def test_position_continuity_is_checked_across_feedback(self):
        s,_=report([seed()]+chain(action='EnterLong')+chain(T+r.M10,before=0))
        self.assertIn('POSITION_CONTINUITY',[v['code'] for v in s['findings']])

    def test_replay_clocks_are_virtual_and_not_relabelled_live(self):
        s,rows=report([seed()]+chain(origin='RECOVERY_BACKFILL',action='EnterLong'))
        self.assertEqual(s['findings'],[])
        self.assertEqual(rows[0]['step_clock_semantics'],'VIRTUAL_REPLAY_CLOCK')
        self.assertNotEqual(rows[0]['step_result_clock_ms'],rows[0]['model_result_at_ms'])
        self.assertEqual(rows[0]['origin'],'RECOVERY_BACKFILL')

    def test_no_counterfactual_ws_ready_time_is_invented(self):
        event=dict(kind='transport_event',event=dict(event='primary_source_readiness',snapshot=dict(label_ms=T,
            cached_bitmap=1023,current_generation_bitmap=1023,boundary_received_at_ms=None,
            outcome='WAITING',writer_processed_at_ms=T+r.M10+1000)))
        _,rows=report([seed(),event]+chain())
        self.assertIsNone(rows[0]['first_ws_candidate_diagnostic_at_ms'])
        self.assertEqual(rows[0]['unselected_ws_ready_time_missing_reason'],'NO_RECORDED_WS_READY_WITNESS')

    def test_future_diagnostic_cannot_explain_past_selection(self):
        ready=dict(kind='transport_event',event=dict(event='primary_source_readiness',snapshot=dict(label_ms=T,
            outcome='WS_READY_CANDIDATE',writer_processed_at_ms=T+r.M10+1200)))
        selected=dict(kind='transport_event',event=dict(event='primary_precommit_rest_selected',label_ms=T))
        _,rows=report([seed(),selected]+chain()+[ready])
        self.assertIsNone(rows[0]['first_ws_candidate_diagnostic_at_ms'])

    def test_late_rest_revision_does_not_replace_committed_input(self):
        original=[seed()]+chain(origin='WS_LIVE')
        revision=dict(kind='transport_event',event=dict(event='primary_audit_result',label_ms=T,
            assessment=dict(kind='PriceDifference',observed_m1=[dict(close='9000')]),nonblocking=True))
        _,a=report(original);_,b=report(original+[revision])
        self.assertEqual(a[0]['canonical_digest'],b[0]['canonical_digest'])
        self.assertEqual(a[0]['decision'],b[0]['decision'])
        self.assertEqual(a[0]['phases'],b[0]['phases'])

    def test_http_receipt_is_not_guessed_to_nearest_label(self):
        http=dict(kind='transport_event',event=dict(event='primary_http_completed',role='Proof',receipt=dict(get_attempts=1)))
        s,_=report([seed(),http]+chain())
        self.assertEqual(s['http_receipts_unassigned'][0]['label_association'],'UNAVAILABLE_NO_TICKET_ID_IN_RECORDED_EVENT')

    def test_complete_calendar_and_self_reported_eod_are_not_acceptance(self):
        journal=body([seed()]+chain());completion=r.encode(dict(semantic_eod_passed=True,journal=dict(
            sha256=r.sha(journal),bytes=len(journal),last_sequence=3)))
        s,_=r.analyze(journal,day([T]),evidence_kind='synthetic_fixture',completion=completion)
        self.assertEqual(s['status'],'CALENDAR_COVERAGE_COMPLETE_REPORTED')
        self.assertTrue(s['completion_journal_binding_matches'])
        self.assertFalse(s['full_session_accepted']);self.assertEqual(s['clean_full_sessions_added'],0)

    def test_wait_transition_comes_from_actual_decisions(self):
        a=chain(T);b=chain(T+3*3600000);b[1]['event']['decision']['reason']='breakout_veto'
        s,_=report([seed()]+a+b,labels=[T,T+3*3600000])
        self.assertEqual(s['last_before_wait_label_ms'],T)
        self.assertEqual(s['first_post_wait_decision_label_ms'],T+3*3600000)

    def test_report_is_deterministic_new_directory_and_hashed(self):
        s,rows=report([seed()]+chain())
        self.assertEqual((s,rows),report([seed()]+chain()))
        with tempfile.TemporaryDirectory() as t:
            out=Path(t)/'new';r.write_report(out,s,rows)
            manifest=json.loads((out/'manifest.json').read_bytes())
            for n,v in manifest['files'].items():self.assertEqual(r.sha((out/n).read_bytes()),v['sha256'])
            with self.assertRaises(FileExistsError):r.write_report(out,s,rows)
            csv_rows=list(csv.DictReader(io.StringIO((out/'m10.csv').read_text())))
            self.assertEqual(csv_rows[1]['origin'],'')

    def test_symlink_input_and_nonfinite_jsonl_are_refused(self):
        with tempfile.TemporaryDirectory() as t:
            p=Path(t);(p/'real').write_bytes(b'{}');(p/'link').symlink_to(p/'real')
            with self.assertRaises(OSError):r.read_file(p/'link')
        with self.assertRaises(r.Refused):r.journal_rows(b'{"sequence":0,"kind":"seed","observation":{"x":NaN}}\n')

    def csv_row(self,summary,records):
        with tempfile.TemporaryDirectory() as t:
            out=Path(t)/'report';r.write_report(out,summary,records)
            return next(csv.DictReader(io.StringIO((out/'m10.csv').read_text())))

    def rest(self,status='EvidenceOnly',assessment=None,name='primary_audit_result',role='Audit'):
        event=dict(event=name,label_ms=T,status=status,role=role,nonblocking=True)
        if assessment is not None:event['assessment']=assessment
        return dict(kind='transport_event',event=event)

    def test_r1_unavailable_precedes_retained_object_and_list_assessments(self):
        for status in ('EvidenceUnavailable','Unavailable','CancelledForStop'):
            for assessment in (dict(kind='Match'),[dict(kind='Match'),dict(kind='VolumeRevision')]):
                with self.subTest(status=status,assessment=assessment):
                    s,rows=report([seed()]+chain()+[self.rest(status,assessment)])
                    v=rows[0]['rest_outcomes'][0]
                    self.assertEqual(v['actual_outcome'],'UNAVAILABLE_UNATTESTED')
                    self.assertEqual(v['reported_assessments'][0]['kind'],'Match')
                    self.assertEqual(v['event']['status'],status)
                    self.assertEqual(self.csv_row(s,rows)['rest_outcome_classes'],'UNAVAILABLE_UNATTESTED')
                    self.assertEqual(s['findings'],[]);self.assertEqual(s['committed_bound_chains'],1)

    def test_r1_evidence_only_match_is_reported_not_integrity_attested(self):
        s,rows=report([seed()]+chain()+[self.rest(assessment=dict(kind='Match',known_basis='COMMITTED_MODEL_INPUT'))])
        v=rows[0]['rest_outcomes'][0];csv_row=self.csv_row(s,rows)
        self.assertEqual(v['actual_outcome'],'Match')
        self.assertEqual(v['outcome_scope'],'AVAILABILITY_AND_REPORTED_KIND_NOT_INTEGRITY_ATTESTATION')
        self.assertEqual(csv_row['rest_reported_assessment_kinds'],'Match')
        self.assertEqual(csv_row['rest_outcome_roles'],'Audit')
        self.assertFalse(s['full_session_accepted'])

    def test_r1_failure_trace_overrides_retained_match_and_audit_is_counted_once(self):
        proof=self.rest(assessment=dict(kind='Match'),name='primary_rest_observation',role='Proof')
        audit=self.rest(assessment=dict(kind='Match'))
        failure=dict(kind='transport_event',event=dict(event='primary_rest_nonblocking_failed',label_ms=T,role='Audit',**{'class':'Timeout'}))
        for events in ([proof,failure,audit],[proof,audit,failure]):
            with self.subTest(events=events):
                s,rows=report([seed()]+chain()+events)
                self.assertEqual(self.csv_row(s,rows)['rest_outcome_classes'],'Match|UNAVAILABLE_UNATTESTED')
                self.assertEqual(self.csv_row(s,rows)['rest_reported_assessment_kinds'],'Match|Match')
                self.assertEqual(len(rows[0]['rest_outcomes']),3)
                target=next(v for v in rows[0]['rest_outcomes'] if v['event']['event']=='primary_audit_result')
                self.assertEqual(target['failure_trace_classes'],['Timeout'])
                self.assertEqual(s['committed_bound_chains'],1);self.assertEqual(s['findings'],[])

    def test_r1_no_assessment_or_proof_failure_never_becomes_match(self):
        for event,want in [(self.rest(), 'UNASSESSED_OR_UNKNOWN'),
            (self.rest('EvidenceUnavailable'), 'UNAVAILABLE_UNATTESTED'),
            (self.rest(assessment=dict(kind='Match'),name='primary_rest_proof_failed',role='Proof'),'UNAVAILABLE_UNATTESTED')]:
            with self.subTest(event=event):
                s,rows=report([seed()]+chain()+[event])
                self.assertEqual(self.csv_row(s,rows)['rest_outcome_classes'],want)

    def test_r1_nonposition_state_mutants_are_explicitly_outside_scope(self):
        for index in (1,4):
            rows=chain()+chain(T+r.M10)
            rows[index]['event']['before']['prev_day_range']=999
            s,records=report([seed()]+rows)
            self.assertEqual(s['committed_bound_chains'],2);self.assertEqual(s['findings'],[])
            self.assertEqual(s['state_continuity_scope'],'POSITION_ONLY')
            self.assertFalse(s['seed_state_continuity_checked']);self.assertFalse(s['full_state_continuity_checked'])
            self.assertIn('NOT_FULL_STATE',s['committed_bound_chains_scope'])
            self.assertEqual(self.csv_row(s,records)['state_continuity_scope'],'POSITION_ONLY')

    def test_r1_fifo_without_writer_is_refused_without_blocking(self):
        with tempfile.TemporaryDirectory() as t:
            fifo=Path(t)/'input';os.mkfifo(fifo)
            code='import sys;sys.path.insert(0,sys.argv[1]);from ws_primary_f0_fm_report import read_file,Refused\ntry:read_file(sys.argv[2])\nexcept Refused:print("REFUSED_NON_REGULAR")\nelse:raise SystemExit(1)'
            result=subprocess.run([sys.executable,'-B','-c',code,str(Path(r.__file__).parent),str(fifo)],capture_output=True,timeout=3)
            self.assertEqual(result.returncode,0)
            self.assertEqual(result.stdout.strip(),b'REFUSED_NON_REGULAR')


if __name__=='__main__':unittest.main(verbosity=2)
