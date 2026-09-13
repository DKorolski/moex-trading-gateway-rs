#!/usr/bin/env python3
"""Targeted mutation harness for the Stage 8B-P1-e I1A source contract."""

from __future__ import annotations

import copy
import json

import stage8b_p1e_i1a_source_check as check


def replace(content: dict[str, str], key: str, old: str, new: str) -> dict[str, str]:
    mutated = copy.deepcopy(content)
    if old not in mutated[key]:
        raise RuntimeError(f"mutation anchor missing: {key}: {old}")
    mutated[key] = mutated[key].replace(old, new, 1)
    return mutated


def main() -> None:
    base = check.load_content()
    check.validate_content(base)
    cases = [
        ("semantic-domain", "core", "schedule-semantic-identity.sha256.v1", "schedule-semantic-identity.sha256.v2"),
        ("semantic-identity", "core", "pub struct Stage8bP1eScheduleSemanticIdentityV1", "struct RemovedSemanticIdentity"),
        ("strict-envelope", "core", "pub struct Stage8bP1eScheduleEnvelopeV3", "struct RemovedEnvelopeV3"),
        ("strict-fields", "core", "#[serde(deny_unknown_fields)]\n    pub struct Stage8bP1eScheduleInstrumentV1", "#[serde(default)]\n    pub struct Stage8bP1eScheduleInstrumentV1"),
        ("semantic-hash", "core", "pub fn stage8b_p1e_schedule_semantic_sha256(", "fn removed_semantic_hash("),
        ("signature-prehash", "core", "pub fn stage8b_p1e_schedule_unsigned_signature_sha256(", "fn removed_signature_prehash("),
        ("signature-verifier", "core", "fn verify_signature(", "fn removed_signature_verifier("),
        ("payload-projection", "core", "envelope.semantic_identity != expected_identity", "false"),
        ("strict-canonical", "core", "stage8b_p1e_canonical_json(&envelope)? != exact_envelope_bytes", "false"),
        ("open-closed", "core", "heartbeat_and_open_to_closed_follow_one_semantic_revision_domain", "removed_open_closed_test"),
        ("closed-boundary", "core", "closed_boundary_proof_is_exactly_the_last_tradable_m10_grid_boundary", "removed_boundary_test"),
        ("negative-matrix", "core", "authentication_freshness_identity_and_stage4_negative_matrix_fails_closed", "removed_negative_matrix"),
        ("day-transition", "core", "signed_trading_day_transition_is_monotonic_and_payload_projection_is_exact", "removed_day_test"),
        ("fixture-hash", "core", "accepted_r2_semantic_fixture_hashes_match_the_production_hasher", "removed_fixture_test"),
        ("consumer-snapshot-progression", "core", "consumer_accepts_late_join_and_same_hash_revision_jump", "removed_consumer_snapshot_test"),
        ("consumer-late-join", "core", "return Ok(Stage8bP1eScheduleProgressionV1::Bootstrap);", "return Err(Stage8bP1eScheduleSourceError::ProgressionConflict);"),
        ("v4-version", "journal", "STAGE6_DURABLE_RECORD_SCHEMA_VERSION_V4: u16 = 4", "STAGE6_DURABLE_RECORD_SCHEMA_VERSION_V4: u16 = 5"),
        ("v4-kind", "journal", 'record_kind: "schedule_evidence_bound".to_string()', 'record_kind: "schedule_bound".to_string()'),
        ("v4-dispatch", "journal", "4 => Stage6JournalRecordV4::decode_canonical(bytes).map(Self::V4)", "4 => Err(Stage6ReconciliationV2Error::UnsupportedSchema(4))"),
        ("v1-golden", "journal", "v1_golden_bytes_and_record_identity_remain_unchanged", "removed_v1_golden"),
        ("v2-golden", "journal", "canonical_golden_matrix_is_stable", "removed_v2_golden"),
        ("v4-negative", "journal", "v4_schedule_binding_roundtrips_and_fails_closed_on_wire_drift", "removed_v4_negative"),
        ("market-predecessor", "journal", "market_v4_is_the_exact_request_predecessor_of_dispatch", "removed_market_predecessor"),
        ("ahead-classifier", "live", "pub fn classify_stage8b_p1e_schedule_journal_ahead_candidate(", "fn removed_ahead_classifier("),
        ("one-successor", "live", "p1e_v4_journal_ahead_classifier_accepts_only_one_exact_successor", "removed_successor_test"),
        ("append", "live", ".append_versioned(&Stage6JournalRecordVersioned::V4(record.clone()))", ".append_versioned(&Stage6JournalRecordVersioned::V3(record.clone()))"),
        ("reread", "live", "self.refresh_after_append()?", "removed_refresh_after_append()?"),
        ("exact-v4-replacement", "live", "fn stage8b_p1e_current_v4_for_replacement", "fn removed_current_v4_for_replacement"),
        ("v4-evaluation-continuation", "live", "apply_stage8b_p1d3_evaluation_stage_after_schedule_binding", "removed_evaluation_after_schedule_binding"),
        ("v4-truth-continuation", "live", "apply_stage8b_p1d3_autonomous_truth_stage_after_schedule_binding", "removed_truth_after_schedule_binding"),
        ("v4-prefix-recovery", "live", "fn stage8b_p1e_schedule_checkpoint_before_journal_ahead_outcome(", "fn removed_schedule_checkpoint_before_journal_ahead_outcome("),
        ("market-v4-recovery-binding", "live", "fn stage8b_p1e_market_binding_before_recovered_dispatch", "fn removed_market_binding_before_recovered_dispatch"),
        ("market-v4-proof", "p1d1", "v4_proof:\n        Option<crate::stage5e_no_io_lifecycle::p1e_schedule_source::Stage8bP1eV4BindingProofV1>", "removed_v4_proof: Option<()>"),
        ("market-bound-clock", "p1d1", "fn bound_at_utc(&self) -> Option<DateTime<Utc>>", "fn removed_bound_at_utc(&self) -> Option<DateTime<Utc>>"),
        ("market-exact-redis-id", "p1d1", "self.source_redis_id == expected.redis_id", "true"),
        ("market-exact-semantic-id", "p1d1", "self.semantic_id_sha256 == expected.semantic_id_sha256", "true"),
        ("market-exact-payload", "p1d1", "self.payload_sha256 == expected.payload_sha256", "true"),
        ("market-exact-open-ts", "p1d1", "self.open_ts_utc_ms == expected.open_ts_utc_ms", "true"),
        ("market-exact-close-ts", "p1d1", "self.close_ts_utc_ms == expected.close_ts_utc_ms", "true"),
        ("route-tag", "p1d3", "enum Stage8bP1d3ScheduleStepRoute", "enum RemovedScheduleStepRoute"),
        ("cancel-route-rejection", "p1d3", "if step.route == Stage8bP1d3ScheduleStepRoute::Cancel", "if false"),
        ("v4-record-cross-binding", "p1d3", "pub(crate) fn matches_stage8b_p1e_schedule_v4_record", "pub(crate) fn removed_matches_schedule_v4_record"),
        ("v4-expiry-receipt-order", "p1d3", "fn ordered_autonomous_receipt_timestamp", "fn removed_autonomous_receipt_timestamp"),
        ("working-exact-redis-id", "p1d3", "self.redis_id == expected.redis_id", "true"),
        ("working-exact-semantic-id", "p1d3", "self.semantic_id_sha256 == expected.semantic_id_sha256", "true"),
        ("working-exact-payload", "p1d3", "self.payload_sha256 == expected.payload_sha256", "true"),
        ("working-exact-open-ts", "p1d3", "self.open_ts_utc_ms == expected.open_ts_utc_ms", "true"),
        ("working-exact-close-ts", "p1d3", "self.close_ts_utc_ms == expected.close_ts_utc_ms", "true"),
        ("day-expiry-exact-m10", "p1d3", "exact_last_eligible_m10", "removed_day_m10"),
        ("seal", "service", "self.advance_recovery_seal(commitment_key)?;\n        if self.committed_seal.seal_generation() != pending.expected_covering_seal_generation()", "removed_advance_recovery_seal(commitment_key)?;\n        if self.committed_seal.seal_generation() != pending.expected_covering_seal_generation()"),
        ("outcome-before-seal-frontier", "service", "let fail_before_replacement_seal = self.stage8a4_test_fail_before_covering_seal", "let fail_before_replacement_seal = false"),
        ("restart-owner", "service", "pub struct Stage8bP1eScheduleBindingCommittedOwner", "struct RemovedScheduleBindingCommittedOwner"),
        ("newest-command", "reader", 'redis::cmd("XREVRANGE")', 'redis::cmd("XRANGE")'),
        ("newest-count", "reader", ".arg(64)", ".arg(1)"),
        ("latch-c", "reader", "BeforeScheduleRead", "RemovedBeforeScheduleRead"),
        ("latch-d", "reader", "AfterScheduleReadBeforeBinding", "RemovedAfterRead"),
        ("latch-e", "reader", "AfterBinding", "RemovedAfterBinding"),
        ("latch-f", "reader", "BeforeEffect", "RemovedBeforeEffect"),
        ("market-route", "reader", "commit_stage8b_p1e_market_schedule", "removed_market_route"),
        ("working-route", "reader", "commit_stage8b_p1e_working_limit_schedule", "removed_working_route"),
        ("cancel-route", "reader", "commit_stage8b_p1e_cancel_schedule", "removed_cancel_route"),
        ("expiry-route", "reader", "commit_stage8b_p1e_day_expiry_schedule", "removed_expiry_route"),
        ("no-second-seal", "reader", "latches_e_and_f_retain_the_exact_committed_binding_without_a_second_seal", "removed_no_second_seal"),
        ("non-authorizing-binding-owner", "reader", "pub enum Stage8bP1eScheduleBindingCommitV1", "pub enum RemovedScheduleBindingCommit"),
        ("guarded-restart-resume", "reader", "pub fn resume_stage8b_p1e_committed_schedule_binding", "pub fn removed_schedule_binding_resume"),
        ("signed-reader-positive", "reader", "signed_reader_accepts_late_join_retention_and_a_b_a_snapshot_progression", "removed_signed_reader_positive"),
        ("signed-reader-negative", "reader", "signed_reader_rejects_revision_rollback_and_sequence_or_hash_conflicts", "removed_signed_reader_negative"),
        ("observable-latch-e", "reader", "signal_after_latch_d_and_binding_is_observed_by_mandatory_latch_e", "removed_observable_latch_e"),
        ("market-v4-effect", "reader", "signed_v4_market_reaches_existing_effect_and_restart_once", "removed_market_v4_effect"),
        ("working-v4-untouched", "reader", "signed_v4_working_untouched_reaches_existing_effect_once", "removed_working_v4_untouched"),
        ("working-v4-fill", "reader", "signed_v4_working_fill_survives_binding_and_effect_restarts_without_duplication", "removed_working_v4_fill"),
        ("expiry-v4-effect", "reader", "signed_v4_day_expiry_reaches_existing_effect_and_restart_once", "removed_expiry_v4_effect"),
        ("cancel-route-negative", "reader", "signed_closed_cancel_authority_cannot_enter_working_evaluation", "removed_cancel_route_negative"),
        ("cancel-route-positive", "reader", "signed_closed_cancel_authority_reaches_cancel_effect_once", "removed_cancel_route_positive"),
        ("journal-ahead-effect", "reader", "v4_journal_ahead_restart_commits_one_seal_and_reaches_market_effect_once", "removed_journal_ahead_effect"),
        ("market-v4-outcome-recovery", "reader", "signed_v4_market_recovers_dispatch_outcome_ahead_of_ack_replacement_once", "removed_market_v4_outcome_recovery"),
        ("working-v4-outcome-recovery", "reader", "signed_v4_working_fill_recovers_outcome_ahead_of_replacement_once", "removed_working_v4_outcome_recovery"),
        ("expiry-v4-outcome-recovery", "reader", "signed_v4_day_expiry_recovers_outcome_ahead_of_replacement_once", "removed_expiry_v4_outcome_recovery"),
        ("cancel-v4-outcome-recovery", "reader", "signed_v4_cancel_recovers_dispatch_and_outcome_ahead_of_replacement_once", "removed_cancel_v4_outcome_recovery"),
        ("finam-adapter", "publisher", "pub fn adapt_stage8b_p1e_readonly_schedule(", "fn removed_finam_adapter("),
        ("raw-finam-hash", "publisher", "raw_response_sha256: sha256_hex(&input.exact_finam_response_bytes)", "raw_response_sha256: \"0\".repeat(64)"),
        ("moscow-day", "publisher", "Europe/Moscow", "UTC"),
        ("endpoint-overlap", "publisher", "inclusive_session_endpoints_reject_adjacency_and_overlap", "removed_endpoint_test"),
        ("prepared", "publisher", "pub enum Stage8bP1eSchedulePublisherPhaseV1 {\n    Prepared,\n    Published,\n}", "pub enum Stage8bP1eSchedulePublisherPhaseV1 {\n    Published,\n}"),
        ("file-sync", "publisher", "file.sync_all()?", "file.flush()?"),
        ("dir-sync", "publisher", "File::open(parent)?.sync_all()?", "drop(parent)"),
        ("xadd", "publisher", 'redis::cmd("XADD")', 'redis::cmd("SET")'),
        ("nomkstream", "publisher", '.arg("NOMKSTREAM")', '.arg("MKSTREAM")'),
        ("maxlen-exact", "publisher", '.arg("=")', '.arg("~")'),
        ("response-loss", "publisher", "prepared_state_survives_response_loss_and_replays_exact_signed_bytes", "removed_response_loss_test"),
        ("publisher-revision", "publisher", "publisher_open_to_closed_changes_revision_with_unchanged_sessions", "removed_revision_test"),
        ("supervisor-kind", "supervisor", "    P1eScheduleBindingCommitted,\n    Blocked,", "    RemovedP1eScheduleBindingCommitted,\n    Blocked,"),
        ("redis-exhaustive", "redis", 'Stage7bRestartOutcome::P1eScheduleBindingCommitted(_) => "P1eScheduleBindingCommitted"', 'Stage7bRestartOutcome::P1eScheduleBindingCommitted(_) => "Removed"'),
        ("doc-reader", "document", "XREVRANGE + - COUNT 64", "XRANGE - + COUNT 1"),
        ("doc-publisher", "document", "XADD NOMKSTREAM MAXLEN = 4096", "XADD MAXLEN ~ 4096"),
        ("doc-crash", "document", "105 x 2", "105 x 1"),
        ("closed-db", "evidence", '"operational_redis_db0": false', '"operational_redis_db0": true'),
        ("closed-finam", "evidence", '"finam_post_delete": false', '"finam_post_delete": true'),
        ("closed-live", "evidence", '"runtime_live": false', '"runtime_live": true'),
        ("next-stage", "evidence", '"next_stage_authorized": false', '"next_stage_authorized": true'),
    ]
    passed = 0
    for name, key, old, new in cases:
        mutated = replace(base, key, old, new)
        try:
            check.validate_content(mutated)
        except (check.CheckFailure, json.JSONDecodeError):
            passed += 1
            print(f"PASS {name}")
            continue
        raise SystemExit(f"FAIL stage8b-p1e-i1a-source-negative-harness accepted mutation: {name}")
    print(f"PASS stage8b-p1e-i1a-source-negative-harness {passed}/{len(cases)}")


if __name__ == "__main__":
    main()
