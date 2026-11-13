#!/usr/bin/env python3
"""File-only F0/FM evidence projection; never admission or session acceptance.

Reads an original journal plus the independently supplied model-day calendar.
Does not run a model, contact a broker, rewrite inputs, or replace the accepted
Rust verifier. Prefix snapshots and completed runtime exports remain distinct.
"""
import argparse
from collections import Counter, defaultdict
import csv
from decimal import Decimal, InvalidOperation
import hashlib
import io
import json
import math
import os
from pathlib import Path
import re
import stat
import sys

MAX_BYTES = 32 * 1024 * 1024
MAX_ROWS = 16385
MAX_ROW_BYTES = 1048576
M10 = 600000
CANDLE_POLICY = 'moex.shadow.closed-m10-close.full-fill.one-lot.zero-cost.v1'
HASH = re.compile(r'[0-9a-f]{64}')
KINDS = ('observed_runtime', 'replayed_model', 'synthetic_fixture')


class Refused(ValueError):
    """Fixed public reason only; never include raw rows or credentials."""


def need(ok, code):
    if not ok:
        raise Refused(code)


def sha(body):
    return hashlib.sha256(body).hexdigest()


def encode(value):
    return json.dumps(value, sort_keys=True, ensure_ascii=False,
                      separators=(',', ':'), allow_nan=False).encode()


def integer(value):
    return type(value) is int and 0 <= value <= 2**63 - 1


def unique_json(body):
    def pairs(items):
        out = {}
        for key, value in items:
            need(key not in out, 'DUPLICATE_JSON_KEY')
            out[key] = value
        return out

    def finite(value):
        result = float(value)
        need(math.isfinite(result), 'NONFINITE_NUMBER')
        return result

    try:
        return json.loads(body, object_pairs_hook=pairs, parse_float=finite,
                          parse_constant=lambda _: need(False, 'NONFINITE_NUMBER'))
    except (UnicodeError, json.JSONDecodeError, RecursionError) as error:
        raise Refused('MALFORMED_JSON') from error


def read_file(path, limit=MAX_BYTES):
    path = Path(path)
    fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW)
    with os.fdopen(fd, 'rb') as f:
        meta = os.fstat(f.fileno())
        need(stat.S_ISREG(meta.st_mode) and meta.st_size <= limit, 'INPUT_FILE_BOUND_OR_TYPE')
        body = f.read(limit + 1)
    need(len(body) <= limit, 'INPUT_BYTES_BOUND')
    return body


def journal_rows(body, allow_prefix=False):
    need(len(body) <= MAX_BYTES, 'JOURNAL_BYTES_BOUND')
    lines = body.splitlines(keepends=True)
    ignored = b''
    if lines and not lines[-1].endswith(b'\n'):
        need(allow_prefix, 'UNTERMINATED_JOURNAL_ROW')
        ignored = lines.pop()
    need(len(lines) <= MAX_ROWS, 'JOURNAL_ROWS_BOUND')
    rows, offset = [], 0
    for index, line in enumerate(lines):
        need(0 < len(line) <= MAX_ROW_BYTES, 'JOURNAL_ROW_BOUND')
        row = unique_json(line)
        need(isinstance(row, dict) and integer(row.get('sequence'))
             and row['sequence'] == index, 'JOURNAL_SEQUENCE')
        need(isinstance(row.get('kind'), str), 'JOURNAL_ROW_SHAPE')
        if row['kind'] != 'seed':
            need(isinstance(row.get('event'), dict), 'JOURNAL_EVENT_SHAPE')
            event = row['event']
            if row['kind'] in ('canonical_source', 'restart_source'):
                receipt = event.get('receipt')
                need(isinstance(receipt, dict) and isinstance(receipt.get('canonical', receipt), dict), 'SOURCE_RECEIPT_SHAPE')
                bars = receipt.get('canonical',receipt).get('actual_m1')
                need(bars is None or isinstance(bars,list) and len(bars)<=10
                     and all(isinstance(b,dict) for b in bars), 'M1_INVENTORY_SHAPE')
            if row['kind'] == 'model_step':
                decision = event.get('decision')
                need(isinstance(decision,dict) and (decision.get('action') is None
                     or isinstance(decision['action'],str)), 'MODEL_DECISION_SHAPE')
            if row['kind'] == 'hypothetical_candle_feedback':
                need(isinstance(event.get('fill'), dict)
                     and isinstance(event['fill'].get('request'), dict), 'FEEDBACK_SHAPE')
            if event.get('event') == 'primary_source_readiness':
                need(isinstance(event.get('snapshot'), dict), 'READINESS_SHAPE')
        else:
            need(isinstance(row.get('observation'), dict), 'SEED_OBSERVATION_SHAPE')
        row['_ref'] = dict(sequence=index, byte_offset=offset,
                           bytes=len(line), line_sha256=sha(line))
        rows.append(row)
        offset += len(line)
    return rows, dict(original_sha256=sha(body), original_bytes=len(body),
                      analyzed_prefix_sha256=sha(body[:offset]), analyzed_bytes=offset,
                      ignored_unterminated_tail_bytes=len(ignored), rows=len(rows))


def model_day(body):
    value = unique_json(body)
    need(isinstance(value, dict) and type(value.get('schema_version')) is int
         and value['schema_version'] == 1, 'CALENDAR_SCHEMA')
    labels = value.get('required_m10_labels_ms')
    need(isinstance(labels, list) and 0 < len(labels) <= 144
         and all(integer(t) and t % M10 == 0 for t in labels)
         and labels == sorted(set(labels)), 'CALENDAR_LABELS')
    return value, labels


def refs(rows):
    return [r['_ref'] for r in rows]


def position(state):
    if not isinstance(state, dict):
        return None
    value = state.get('last_position_qty')
    if type(value) not in (int, float):
        return None
    try:
        if not math.isfinite(value):
            return None
    except OverflowError:
        return None
    return value


def pending(state):
    if not isinstance(state, dict) or not any(k in state for k in (
            'pending_entry_request_id', 'pending_exit_request_id')):
        return None
    return any(state.get(k) is not None for k in (
        'pending_entry_request_id', 'pending_exit_request_id'))


def event_label(row):
    event = row.get('event', {})
    if row['kind'] in ('canonical_source', 'restart_source'):
        receipt = event.get('receipt', {})
        return receipt.get('canonical', receipt).get('label_ms')
    if row['kind'] == 'hypothetical_candle_feedback':
        return event.get('fill', {}).get('model_label_utc_ms')
    if event.get('event') == 'primary_source_readiness':
        return event.get('snapshot', {}).get('label_ms')
    return event.get('model_label_utc_ms', event.get('label_ms'))


def transport_view(row):
    event = row['event']
    keys = ('event', 'role', 'label_ms', 'class', 'reason', 'status', 'issued_at_ms',
            'recorded_at_ms', 'selected_at_ms', 'deadline_ms', 'nonblocking',
            'model_input_rewritten', 'origin', 'generation', 'auth_generation')
    value = {k: event[k] for k in keys if k in event}
    assessment = event.get('assessment')
    if isinstance(assessment, dict):
        value['assessment'] = {k: assessment[k] for k in (
            'kind', 'known_basis', 'model_input_rewritten') if k in assessment}
    return dict(source_ref=row['_ref'], event=value)


def report_label(label, group, journal_hash, observed_until_ms):
    found = lambda kind: [r for r in group if r['kind'] == kind]
    sources, steps = found('canonical_source'), found('model_step')
    feedbacks, commits = found('hypothetical_candle_feedback'), found('canonical_commit')
    findings = []
    def check(ok, code):
        if not ok:
            findings.append(code)
    for name, rows in [('SOURCE', sources), ('STEP', steps), ('FEEDBACK', feedbacks), ('COMMIT', commits)]:
        check(len(rows) <= 1, 'DUPLICATE_' + name)
    source = sources[0]['event'] if sources else {}
    wrapped = source.get('receipt', {})
    receipt = wrapped.get('canonical', wrapped)
    step = steps[0]['event'] if steps else {}
    commit = commits[0]['event'] if commits else {}
    feedback = feedbacks[0]['event'] if feedbacks else {}
    fill = feedback.get('fill', {})
    decision = step.get('decision')
    action = decision.get('action') if isinstance(decision, dict) else None
    complete = len(sources) == len(steps) == len(commits) == 1
    if complete:
        check(sources[0]['sequence'] < steps[0]['sequence'] < commits[0]['sequence'], 'CHAIN_ORDER')
        source_hash, canonical = source.get('source_sha256'), receipt.get('canonical_digest')
        check(isinstance(source_hash, str) and HASH.fullmatch(source_hash) is not None
              and source_hash == step.get('raw_provenance_sha256') == commit.get('source_sha256'), 'SOURCE_HASH_BINDING')
        check(isinstance(canonical, str) and HASH.fullmatch(canonical) is not None
              and canonical == step.get('canonical_input_sha256'), 'CANONICAL_HASH_BINDING')
        check(step.get('nominal_close_utc_ms') == receipt.get('close_ms') == label + M10, 'NOMINAL_CLOSE_BINDING')
        check(commit.get('feedback_complete') is True, 'COMMIT_FEEDBACK_FLAG')
        check(commit.get('model_result_at_ms') is not None, 'ACTUAL_MODEL_RESULT_MISSING')
    if steps:
        check(isinstance(decision, dict) and isinstance(decision.get('reason'), str)
              and bool(decision['reason']), 'DECISION_REASON_MISSING')
        if action is None:
            check(isinstance(decision, dict) and type(decision.get('quantity_lots')) is int
                  and decision['quantity_lots'] == 0, 'NO_ACTION_QUANTITY')
            check(not feedbacks, 'FEEDBACK_WITHOUT_INTENT')
        else:
            check(action in ('EnterLong', 'EnterShort', 'Exit'), 'UNKNOWN_ACTION')
            check(type(decision.get('quantity_lots')) is int and decision['quantity_lots'] == 1, 'INTENT_QUANTITY')
            check(len(feedbacks) == 1, 'INTENT_FEEDBACK_MISSING_OR_DUPLICATE')
        before_position, decision_position = position(step.get('before')), position(step.get('after'))
        if before_position is not None and decision_position is not None:
            check(before_position == decision_position, 'POSITION_CHANGED_BEFORE_FEEDBACK')
    if feedbacks:
        request = fill.get('request', {})
        check(bool(steps), 'FEEDBACK_WITHOUT_STEP')
        check(fill.get('policy') == CANDLE_POLICY, 'FEEDBACK_POLICY')
        check(fill.get('model_label_utc_ms') == label and fill.get('nominal_close_utc_ms') == label + M10,
              'FEEDBACK_LABEL')
        check(request.get('action') == action and type(request.get('quantity_lots')) is int
              and request['quantity_lots'] == 1, 'FEEDBACK_INTENT_BINDING')
        check(request.get('instrument') == step.get('instrument'), 'FEEDBACK_INSTRUMENT_BINDING')
        check(feedback.get('mode') == step.get('mode'), 'FEEDBACK_MODE_BINDING')
        check(request.get('decision_at_ms') == step.get('result_at_ms'), 'FEEDBACK_DECISION_TIME_BINDING')
        check(integer(request.get('sequence')) and request['sequence'] > 0, 'INTENT_SEQUENCE_MISSING')
        check(fill.get('canonical_input_sha256') == step.get('canonical_input_sha256')
              and fill.get('raw_provenance_sha256') == step.get('raw_provenance_sha256'), 'FEEDBACK_HASH_BINDING')
        if steps and commits:
            check(steps[0]['sequence'] < feedbacks[0]['sequence'] < commits[0]['sequence'], 'FEEDBACK_ORDER')
        check(type(fill.get('position_lots')) is int
              and fill['position_lots'] == position(feedback.get('state_after')), 'FEEDBACK_POSITION_BINDING')
        expected_position = {'EnterLong': 1, 'EnterShort': -1, 'Exit': 0}.get(action)
        check(expected_position is not None and fill.get('position_lots') == expected_position, 'FEEDBACK_POSITION_TRANSITION')
        before_position = position(step.get('before'))
        if before_position is not None:
            check(before_position == 0 if action in ('EnterLong','EnterShort') else before_position in (-1,1),
                  'FEEDBACK_PREVIOUS_POSITION')
        check(pending(feedback.get('state_after')) is False, 'POST_FEEDBACK_PENDING_UNKNOWN_OR_TRUE')
        times = [fill.get('received_at_ms'), request.get('decision_at_ms'), fill.get('applied_at_ms')]
        check(all(integer(t) for t in times) and times == sorted(times), 'FEEDBACK_CAUSAL_TIMES')
        check(integer(fill.get('price_ticks')) and fill['price_ticks'] > 0, 'FEEDBACK_PRICE_TICKS')
        bars = receipt.get('actual_m1')
        if isinstance(bars, list) and bars and all(isinstance(b,dict) for b in bars):
            try:
                # Canonical receipt retains ordered actual M1. Do not use a
                # later REST/audit revision as the fill's close-price basis.
                close = Decimal(str(bars[-1]['close']))
                check(close.is_finite() and type(fill.get('price_ticks')) is int
                      and Decimal(fill['price_ticks']) * Decimal('0.5') == close, 'CLOSE_FILL_PRICE')
            except (KeyError,InvalidOperation,TypeError):
                findings.append('CLOSE_FILL_PRICE_UNAVAILABLE')
    actual_m1 = receipt.get('actual_m1')
    bitmap = receipt.get('minute_bitmap')
    if isinstance(actual_m1, list) and integer(bitmap):
        check(len(actual_m1) == bin(bitmap).count('1') and bitmap < 1024, 'M1_COUNT_BITMAP')
    readiness = [r for r in group if r['kind'] == 'transport_event'
                 and r['event'].get('event') == 'primary_source_readiness']
    selections = [r for r in group if r['kind'] == 'transport_event'
                  and r['event'].get('event') == 'primary_precommit_rest_selected']
    cutoff_sequence = selections[0]['sequence'] if selections else sources[0]['sequence'] if sources else MAX_ROWS
    prior_readiness = [r for r in readiness if r['sequence'] < cutoff_sequence]
    diagnostic = prior_readiness[-1]['event']['snapshot'] if prior_readiness else None
    ready_witnesses = [r for r in prior_readiness
                       if r['event']['snapshot'].get('outcome') == 'WS_READY_CANDIDATE']
    phases = dict(state_before=step.get('before'), state_after_decision=step.get('after'),
                  state_before_missing_reason=None if isinstance(step.get('before'),dict) else 'NOT_RECORDED',
                  state_after_decision_missing_reason=None if isinstance(step.get('after'),dict) else 'NOT_RECORDED',
                  state_after_feedback=feedback.get('state_after') if len(feedbacks) == 1 else None,
                  state_after_feedback_missing_reason=None if len(feedbacks) == 1 else
                  'NO_MODEL_STEP_OBSERVED' if not steps else
                  'NO_FEEDBACK_EVENT_OBSERVED' if action is not None else 'NO_INTENT_NO_FEEDBACK_EXPECTED')
    effective = phases['state_after_feedback'] if len(feedbacks) == 1 else phases['state_after_decision'] if action is None else None
    publication = commit.get('publication_checked_at_ms')
    origin = receipt.get('origin')
    replay_publication = [r for r in group if r['kind']=='transport_event'
                          and r['event'].get('event')=='recovery_step_durable_publication']
    actual_start = (replay_publication[0]['event'].get('actual_model_start_at_ms') if replay_publication
                    else None) if step.get('mode')=='BACKFILL_REPLAY' else step.get('model_start_at_ms')
    if receipt:
        check(origin in ('WS_LIVE', 'REST_EXCEPTION', 'RECOVERY_BACKFILL'), 'UNKNOWN_SOURCE_ORIGIN')
    if origin == 'RECOVERY_BACKFILL' and steps:
        check(step.get('mode') == 'BACKFILL_REPLAY', 'RECOVERY_IS_NOT_LIVE_SIGNAL')
    if any(found(k) for k in ('restart_source',)):
        findings.append('RESTART_SOURCE_REQUIRES_SEPARATE_MAPPING')
    status = ('COMMITTED_REPORTED' if complete else 'INCOMPLETE_CHAIN') if sources or steps or commits or feedbacks else (
        'MISSING_DUE_AS_OF_SNAPSHOT' if observed_until_ms is not None and label + M10 <= observed_until_ms else
        'NOT_YET_DUE' if observed_until_ms is not None else 'NO_RECORD')
    if findings:
        status = 'FINDINGS'
    return dict(label_ms=label, nominal_close_ms=label + M10, status=status, origin=origin,
                reason=receipt.get('reason'), canonical_digest=receipt.get('canonical_digest'),
                source_sha256=source.get('source_sha256'), accepted_m1_count=len(actual_m1) if isinstance(actual_m1, list) else None,
                minute_bitmap=bitmap, generation=receipt.get('generation'),
                current_generation_bitmap_at_last_diagnostic=diagnostic.get('current_generation_bitmap') if diagnostic else None,
                last_readiness_before_selection=diagnostic,
                boundary_received_at_ms=receipt.get('boundary_received_at_ms'),
                grace_due_at_ms_at_last_diagnostic=diagnostic.get('grace_due_at_ms') if diagnostic else None,
                first_ws_candidate_diagnostic_at_ms=ready_witnesses[0]['event']['snapshot'].get('writer_processed_at_ms') if ready_witnesses else None,
                unselected_ws_ready_time_missing_reason=None if ready_witnesses or origin == 'WS_LIVE' else 'NO_RECORDED_WS_READY_WITNESS',
                canonical_ready_at_ms=receipt.get('canonical_ready_at_ms'),
                source_selection=[transport_view(r) for r in selections],
                rest_outcomes=[transport_view(r) for r in group if r['kind'] == 'transport_event'
                               and r['event'].get('event') in ('primary_rest_observation','primary_rest_nonblocking_failed','primary_audit_result')],
                decision=decision, mode=step.get('mode'), model_received_at_ms=step.get('received_at_ms'),
                model_ready_at_ms=step.get('canonical_ready_at_ms'), model_start_at_ms=step.get('model_start_at_ms'),
                actual_model_start_at_ms=actual_start,
                model_result_at_ms=commit.get('model_result_at_ms'), step_result_clock_ms=step.get('result_at_ms'),
                step_clock_semantics='VIRTUAL_REPLAY_CLOCK' if step.get('mode') == 'BACKFILL_REPLAY' else 'MEASURED_RUNTIME_CLOCK',
                publication_checked_at_ms=publication,
                commit_after_close_ms=publication-label-M10 if integer(publication) else None,
                feedback=feedback if feedbacks else None, reported_position_after=position(effective),
                reported_pending_after=pending(effective), phases=phases, findings=sorted(set(findings)),
                refs=dict(journal_sha256=journal_hash, source=refs(sources), step=refs(steps),
                          feedback=refs(feedbacks), commit=refs(commits), readiness=refs(readiness)))


def analyze(journal, calendar, *, evidence_kind, allow_prefix=False,
            observed_until_ms=None, completion=None):
    need(evidence_kind in KINDS, 'EVIDENCE_KIND')
    need(observed_until_ms is None or integer(observed_until_ms), 'OBSERVATION_TIME')
    rows, journal_identity = journal_rows(journal, allow_prefix)
    cal, labels = model_day(calendar)
    groups = defaultdict(list)
    global_findings = []
    for row in rows:
        label = event_label(row)
        if row['kind'] in ('canonical_source','restart_source','model_step','hypothetical_candle_feedback','canonical_commit'):
            need(integer(label), 'REQUIRED_EVENT_LABEL')
        if label is not None:
            need(integer(label), 'EVENT_LABEL_TYPE')
            if label not in labels and row['kind'] in ('canonical_source','restart_source','model_step','hypothetical_candle_feedback','canonical_commit'):
                global_findings.append(dict(code='LABEL_OUTSIDE_EXPECTED_CALENDAR', source_ref=row['_ref'], label_ms=label))
            groups[label].append(row)
    seed_rows = [r for r in rows if r['kind'] == 'seed']
    if rows and (len(seed_rows) != 1 or rows[0]['kind'] != 'seed'):
        global_findings.append(dict(code='SEED_OBSERVATION_SHAPE'))
    seed = seed_rows[0].get('observation', {}) if seed_rows else {}
    records = [report_label(t, groups[t], journal_identity['original_sha256'], observed_until_ms) for t in labels]
    fill_sequences = {}
    last_fill_sequence = None
    for record in records:
        if record['feedback']:
            sequence = record['feedback'].get('fill', {}).get('request', {}).get('sequence')
            if integer(sequence):
                if sequence in fill_sequences:
                    record['findings'].append('REPEATED_FILL_SEQUENCE');record['status']='FINDINGS'
                    global_findings.append(dict(code='REPEATED_FILL_SEQUENCE', intent_sequence=sequence,
                                                labels=[fill_sequences[sequence], record['label_ms']]))
                if last_fill_sequence is not None and sequence <= last_fill_sequence:
                    record['findings'].append('NONINCREASING_INTENT_SEQUENCE');record['status']='FINDINGS'
                fill_sequences[sequence] = record['label_ms']
                last_fill_sequence = sequence
    previous = None
    for record in records:
        if record['refs']['step']:
            if previous is not None and previous['label_ms'] + M10 == record['label_ms']:
                before = position(record['phases']['state_before'])
                if before is not None and previous['reported_position_after'] is not None and before != previous['reported_position_after']:
                    record['findings'].append('POSITION_CONTINUITY');record['status']='FINDINGS'
            previous = record
    committed = [r for r in records if r['status'] == 'COMMITTED_REPORTED']
    issues = global_findings + [dict(code=c, label_ms=r['label_ms']) for r in records for c in r['findings']]
    step_records = [r for r in records if r['decision'] is not None]
    raw_decisions = [r['event']['decision'] for r in rows if r['kind']=='model_step']
    waits = [r['label_ms'] for r in step_records if r['decision'].get('reason') == 'before_wait']
    after_wait = [r['label_ms'] for r in step_records if r['decision'].get('reason') != 'before_wait'
                  and waits and r['label_ms'] > max(waits)]
    reported_completion = unique_json(completion) if completion is not None else None
    need(reported_completion is None or isinstance(reported_completion,dict), 'COMPLETION_SHAPE')
    seal = reported_completion.get('journal') if isinstance(reported_completion, dict) else None
    completion_binding = bool(isinstance(seal, dict) and seal.get('sha256') == sha(journal)
                              and seal.get('bytes') == len(journal) and seal.get('last_sequence') == len(rows)-1
                              and not journal_identity['ignored_unterminated_tail_bytes'])
    last = next((r for r in reversed(records) if r['refs']['step']), None)
    transports = [r for r in rows if r['kind'] == 'transport_event']
    # HTTP receipt has no label/ticket ID in this version: never guess a label
    # from its neighbour, role or timestamp. Keep the observable receipt global.
    http = [dict(source_ref=r['_ref'], role=r['event'].get('role'), receipt=r['event'].get('receipt'),
                 label_association='UNAVAILABLE_NO_TICKET_ID_IN_RECORDED_EVENT') for r in transports
            if r['event'].get('event') == 'primary_http_completed']
    summary = dict(schema_version='finam-f0-fm-report/1', scope='FILE_PROJECTION_NOT_ADMISSION_OR_ACCEPTANCE',
        evidence_kind=evidence_kind, status='FINDINGS' if issues else 'PARTIAL_OBSERVATION' if len(committed)<len(labels) else 'CALENDAR_COVERAGE_COMPLETE_REPORTED',
        journal=journal_identity, model_day_sha256=sha(calendar), session_date=cal.get('session_date'),
        run_id=seed.get('run_id'), segment_id=seed.get('segment_id'), config_sha256=seed.get('config_sha256'),
        seed_sha256=seed.get('seed_sha256'), model_sha256=seed.get('model_sha256'),
        feedback_policy_sha256=seed.get('feedback_policy_sha256'), initial_position_lots=seed.get('virtual_position_lots'),
        expected_labels=len(labels), committed_bound_chains=len(committed),
        coverage_counts=dict(Counter(r['status'] for r in records)),
        origins=dict(Counter(r['origin'] for r in committed)),
        reported_model_steps=len(raw_decisions), decision_reasons=dict(Counter(str(d.get('reason')) for d in raw_decisions)),
        last_before_wait_label_ms=max(waits) if waits else None,
        first_post_wait_decision_label_ms=min(after_wait) if after_wait else None,
        observed_actions=dict(Counter(str(d.get('action')) for d in raw_decisions if d.get('action') is not None)),
        observed_candle_feedback=sum(r['kind']=='hypothetical_candle_feedback' for r in rows),
        fill_policy=CANDLE_POLICY, last_reported_position=last['reported_position_after'] if last else None,
        last_reported_pending=last['reported_pending_after'] if last else None,
        branches_unobserved=[a for a in ('EnterLong','EnterShort','Exit') if not any(r['decision'].get('action')==a for r in step_records)],
        transport_counts=dict(Counter(str(r['event'].get('event')) for r in transports)), http_receipts_unassigned=http,
        findings=issues, observed_until_ms=observed_until_ms,
        reported_completion=reported_completion, completion_journal_binding_matches=completion_binding,
        eod_verification='NOT_PERFORMED_USE_ACCEPTED_RUST_AND_OPERATIONAL_READERS',
        full_session_accepted=False, clean_full_sessions_added=0, current_alor_parity=False,
        native_hashes_recomputed=False, runtime_verdict_unchanged=True,
        diagnostic_limitations=['NO_COUNTERFACTUAL_WS_READY_TIME_WITHOUT_RECORDED_WITNESS',
            'HTTP_LABEL_ASSOCIATION_UNAVAILABLE_WITHOUT_TICKET_ID',
            'STEP_AFTER_IS_BEFORE_FEEDBACK', 'REPLAY_STEP_CLOCKS_ARE_VIRTUAL',
            'SOURCE_ADMISSION_AND_NATIVE_HASH_VERIFICATION_DELEGATED_TO_ACCEPTED_READER'])
    return summary, records


def write_report(output, summary, records):
    output = Path(output)
    output.mkdir(mode=0o700)
    fields = ['label_ms','nominal_close_ms','status','origin','reason','accepted_m1_count','minute_bitmap','generation',
              'current_generation_bitmap_at_last_diagnostic','boundary_received_at_ms','grace_due_at_ms_at_last_diagnostic',
              'first_ws_candidate_diagnostic_at_ms','canonical_ready_at_ms','model_start_at_ms','actual_model_start_at_ms','model_result_at_ms',
              'step_clock_semantics','publication_checked_at_ms','commit_after_close_ms','reported_position_after','reported_pending_after',
              'decision_action','decision_reason','source_sequence','step_sequence','feedback_sequence','commit_sequence',
              'source_sha256','canonical_digest','last_readiness_reason','rest_outcome_classes','source_selected_at_ms','findings']
    csv_body = io.StringIO(newline='');writer = csv.DictWriter(csv_body, fieldnames=fields, lineterminator='\n')
    writer.writeheader()
    for r in records:
        row={k:r.get(k) for k in fields}
        decision=r['decision'] or {}
        row.update(decision_action=decision.get('action'),decision_reason=decision.get('reason'),
                   last_readiness_reason=(r['last_readiness_before_selection'] or {}).get('predeadline_reason'),
                   rest_outcome_classes='|'.join(str(v['event'].get('class',v['event'].get('assessment',{}).get('kind','UNKNOWN'))) for v in r['rest_outcomes']),
                   source_selected_at_ms=r['source_selection'][0]['event'].get('selected_at_ms') if r['source_selection'] else None,
                   findings='|'.join(r['findings']))
        for name in ('source','step','feedback','commit'):
            row[name+'_sequence']='|'.join(str(ref['sequence']) for ref in r['refs'][name])
        writer.writerow(row)
    note = ('F0/FM: файловый разбор, НЕ допуск и НЕ приёмка сессии.\n'
            'Оригинальный журнал не изменён. JSONL содержит refs: sequence, byte offset, hash точной строки.\n'
            'Полнота считается по отдельному model-day, не по найденным строкам.\n'
            'Отсутствующее наблюдение остаётся null/unknown. state_after_decision != state_after_feedback.\n'
            'BACKFILL_REPLAY сохраняет виртуальные времена; actual commit clocks находятся отдельно.\n'
            'Штатные Rust verifier, EOD/exit/cgroup readers и result review выполняются отдельно.\n'
            'F0/FM не меняет runtime verdict, не добавляет clean sessions или ALOR parity.\n')
    files = {'summary.json':encode(summary)+b'\n', 'm10.jsonl':b''.join(encode(r)+b'\n' for r in records),
             'm10.csv':csv_body.getvalue().encode(), 'README_RU.txt':note.encode()}
    for n,b in files.items():
        with (output/n).open('xb') as f:f.write(b)
    manifest = dict(schema_version=1, scope=summary['scope'], journal=summary['journal'],
                    model_day_sha256=summary['model_day_sha256'], files={n:dict(sha256=sha(b),bytes=len(b)) for n,b in files.items()})
    with (output/'manifest.json').open('xb') as f:f.write(encode(manifest)+b'\n')


def main(argv=None):
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--journal',required=True,type=Path);p.add_argument('--model-day',required=True,type=Path)
    p.add_argument('--output',required=True,type=Path);p.add_argument('--kind',required=True,choices=KINDS)
    p.add_argument('--allow-prefix',action='store_true');p.add_argument('--observed-until-ms',type=int)
    p.add_argument('--completion',type=Path)
    a=p.parse_args(argv)
    try:
        summary,records=analyze(read_file(a.journal),read_file(a.model_day,1048576),evidence_kind=a.kind,
            allow_prefix=a.allow_prefix,observed_until_ms=a.observed_until_ms,
            completion=read_file(a.completion,1048576) if a.completion else None)
        write_report(a.output,summary,records)
    except (Refused,OSError) as error:
        code=str(error) if isinstance(error,Refused) else 'FILE_IO_REFUSED'
        print('F0_FM_REFUSED:'+code,file=sys.stderr);return 1
    print(json.dumps({k:summary[k] for k in ('status','expected_labels','committed_bound_chains','evidence_kind','full_session_accepted')}))
    return 2 if summary['findings'] else 0


if __name__=='__main__':
    raise SystemExit(main())
