#!/usr/bin/env python3
"""Fail-closed source/contract checker for Stage 5G-d."""

from __future__ import annotations

import argparse
import json
import re
import subprocess
from pathlib import Path

BASE = "6cafcd7d7caae8b29364c41cb3eece0511e4d42c"
STAGE5C_AUTHORITY = "d0494537d7c1739a16350b2d28f71b304165c812"


class CheckFailure(RuntimeError):
    pass


def require(condition: bool, message: str) -> None:
    if not condition:
        raise CheckFailure(message)


def validate(root: Path, *, check_git: bool = True) -> None:
    timer_path = root / "crates/strategy-runtime-core/src/stage5g_timer.rs"
    order_path = root / "crates/strategy-runtime-core/src/stage5g_order_position.rs"
    stage5c_path = root / "crates/strategy-runtime-core/src/stage5c_paper_host.rs"
    lib_path = root / "crates/strategy-runtime-core/src/lib.rs"
    inventory_path = root / "docs/stage-5/stage5g-d-timer-continuation-inventory.json"
    contract_path = root / "docs/stage-5/stage5g-d-timer-continuation-contract.md"
    descriptor_path = root / "docs/stage-5/stage5g-d-r1b-composition-restore.json"
    for path in (
        timer_path,
        order_path,
        stage5c_path,
        lib_path,
        inventory_path,
        contract_path,
        descriptor_path,
    ):
        require(path.is_file(), f"missing required Stage 5G-d file: {path}")

    timer = timer_path.read_text()
    order = order_path.read_text()
    stage5c = stage5c_path.read_text()
    lib = lib_path.read_text()
    inventory = json.loads(inventory_path.read_text())
    descriptor = json.loads(descriptor_path.read_text())

    required_tokens = (
        "pub struct Stage5gTimerSession",
        "pub struct Stage5gTimerGeneratedIntentEscrow",
        "pub struct Stage5gTimerCheckpointEnvelope",
        "pub fn apply_stage5g_timer_checkpoint",
        "pub fn continue_stage5g_timer_with_timer",
        "pub fn continue_stage5g_timer_with_bar",
        "pub fn attach_stage5g_timer_generated_mock_ack",
        "pub fn apply_stage5g_timer_mock_ack",
        "pub fn attach_stage5g_timer_order_position_session",
        "pub fn classify_stage5g_post_checkpoint_evidence",
        "advance_stage5c_paper_loop_once",
        "advance_stage5c_timer_settlement_next_bar_transactional_at_checkpoint",
        "stage5gd_accepted_bar_checkpoint_ts_utc_ms",
        "advance_stage5c_timer_settlement_timer",
        "pub fn settle_stage5g_bar_continuation",
        "Stage5gBarContinuationTransition",
        "Stage5gBarContinuationTransition::Ready",
        "stage5gd_rearm_zero_intent_bar_continuation",
        "Stage5gTimerMockAckError::AckBeforeContinuationCheckpoint",
        "event.ack.received_ts.timestamp_millis() < session.checkpoint_ts_utc_ms",
        "Stage5gTimerOrderPositionAdmissionBlocked",
        "pub fn retry(",
        "attach_stage5g_timer_order_position_session(self.resolved)",
        "pub current_evidence_identity",
        "parse_replay_evidence_identity",
        "exact_current_identity_count",
        "entry.identity == current_identity",
        "Stage5gCheckpointReplayError::BrokerTruthBeforeContinuationCheckpoint",
        "received_at.timestamp_millis() < continuation_checkpoint",
        ".is_some_and(|last| received_at < last)",
        "canonicalize_stage5g_order_position_evidence(evidence)",
        "canonical_evidence.identity().to_string()",
        "canonical_evidence.fingerprint().to_string()",
        "canonical_new_package_candidate: Some(canonical_evidence)",
        "canonical_new_package_candidate: None",
        "owns_canonical_new_package_candidate",
        "Stage5gCheckpointReplayError::TradeIdentityConflict",
        "Stage5gCheckpointReplayError::EvidenceIdentityGrammarViolation",
        "Stage5gTimerCheckpointError::ReplayLedgerReceiptRegression",
        "previous_ledger_receipt.is_some_and(|previous| parsed.received_at < previous)",
        "Stage5gTimerCheckpointError::CurrentEvidenceIdentityNotLatest",
        "Stage5gTimerCheckpointError::CurrentPackageReceiptMismatch",
        "final_ledger_identity != Some(current_identity)",
        "current.received_at != received_at || final_ledger_receipt != Some(received_at)",
        "last_continuation_checkpoint_ts_utc_ms",
        "ContinuationBeforeInnerSettlement",
        ".is_some_and(|inner| checkpoint_ts_utc_ms < inner)",
        "MissingExactBrokerTruthReceipt",
        "MissingTotalSequence",
        "MissingContinuationCheckpoint",
        "ContinuationBeforeBrokerTruth",
        "CurrentPackageMissingFromReplayLedger",
        ".ok_or(Stage5gTimerCheckpointError::MissingExactBrokerTruthReceipt)?",
        ".ok_or(Stage5gTimerCheckpointError::MissingTotalSequence)?",
        ".ok_or(Stage5gTimerCheckpointError::MissingContinuationCheckpoint)?",
        "if continuation_checkpoint < received_ms",
        "replay.last_continuation_checkpoint_ts_utc_ms = max_optional_checkpoint(\n"
        "        replay.last_continuation_checkpoint_ts_utc_ms,\n"
        "        Some(checkpoint_ts_utc_ms),\n"
        "    );",
        "NonMonotonicCheckpoint",
        "input.now_ts_utc_ms <= last",
        "ConflictingDuplicateEvidence",
        "pub last_broker_truth_received_at",
        "pub last_broker_truth_received_ms",
        "pub evidence_replay_ledger",
        "pub last_total_sequence",
        "timestamp_subsec_nanos()",
        "parsed_request_id.to_string() != request_id",
    )
    for token in required_tokens:
        require(token in timer, f"required timer contract token missing: {token}")

    for token in (
        "BrokerTruthBeforeContinuationCheckpoint",
        "evidence.broker_truth.received_ts.timestamp_millis() < checkpoint",
        "current_evidence_identity: Option<String>",
        "stage5gd_zero_intent_bar_rearms_timer_and_later_bar_without_callback_loss",
        "stage5gd_active_path_stores_single_authority_canonical_fingerprint",
        "stage5gd_active_path_rejects_conflicting_trade_identity_before_replay_append",
        "stage5gd_r4_exact_duplicate_merge_is_order_independent_and_keeps_max_receipt",
        "stage5gd_r4_optional_venue_permutations_fail_closed_without_first_row_authority",
        "stage5gd_r4_same_venue_conflicting_instrument_fields_fail_closed",
        "stage5gd_r4_committed_trade_ledger_uses_exact_instrument_projection",
        "stage5gd_r5_qty_scale_permutations_fail_closed_under_exact_decimal_policy",
        "stage5gd_r5_price_and_optional_amount_scale_drift_fail_closed",
        "stage5gd_r5_signed_zero_representation_is_explicit_and_fail_closed",
        "stage5gd_r5_exact_decimal_rows_merge_deterministically_at_equal_and_later_receipts",
        "stage5gd_r5_committed_trade_ledger_uses_exact_decimal_authority",
        "pub(crate) struct Stage5gCanonicalOrderPositionEvidence",
        "pub(crate) enum Stage5gEvidenceCanonicalizationError",
        "Stage5gCanonicalImmutableTradePayloadV1",
        "STAGE5G_IMMUTABLE_TRADE_PAYLOAD_SCHEMA_VERSION",
        "STAGE5G_IMMUTABLE_TRADE_PAYLOAD_DOMAIN",
        "canonical_immutable_trade_payload_v1",
        "merge_canonical_trade_observation_v1",
        "Stage5gCanonicalDecimalV1",
        "STAGE5G_CANONICAL_DECIMAL_SCHEMA_VERSION",
        "STAGE5G_CANONICAL_DECIMAL_DOMAIN",
        "canonical_decimal_v1",
    ):
        require(token in order, f"required chronology/liveness witness missing: {token}")
    for token in (
        "new_post_restore_package_requires_continuation_chronology_but_exact_replay_does_not",
        "multi_package_restore_requires_ordered_ledger_and_latest_current_projection",
        "post_checkpoint_duplicate_trade_redelivery_matches_active_canonical_fingerprint",
        "post_checkpoint_known_payload_change_and_trade_identity_conflict_fail_closed",
        "new_post_checkpoint_package_owns_one_deduplicated_canonical_candidate",
        "replay_identity_grammar_requires_canonical_uuid_and_colon_free_account",
        "stage5gd_r4_active_restart_exact_duplicate_reversal_is_exact_replay",
        "stage5gd_r4_new_package_instrument_conflicts_preserve_checkpoint",
        "stage5gd_r5_restart_scaled_permutations_fail_closed_without_checkpoint_mutation",
    ):
        require(token in timer, f"required restart/ledger witness missing: {token}")
    require(
        "STAGE5G-D-R1B-R1-ZERO-REARM-BEGIN" in stage5c
        and "STAGE5G-D-R1B-R1-ZERO-REARM-END" in stage5c,
        "narrow zero-intent re-arm bridge missing",
    )
    require(
        "pub(crate) fn stage5gd_rearm_zero_intent_bar_continuation(" in stage5c,
        "zero-intent re-arm authority function missing",
    )

    forbidden_tokens = (
        "std::thread",
        "thread::spawn",
        "tokio::spawn",
        "tokio::time::sleep",
        "std::thread::sleep",
        "Utc::now",
        "redis::",
        "reqwest",
        "finam_client",
        "Method::POST",
        "Method::DELETE",
        ".post(",
        ".delete(",
        "let checkpoint_ts_utc_ms = replay.last_broker_truth_received_ms",
        "last_continuation_checkpoint_ts_utc_ms: replay.last_broker_truth_received_ms",
        ".ends_with(package_discriminator)",
    )
    for token in forbidden_tokens:
        require(token not in timer, f"forbidden Stage 5G-d surface: {token}")

    for obsolete_call in (
        "advance_stage5c_timer_settlement_next_bar(",
        "advance_stage5c_timer_settlement_next_bar_at_checkpoint(",
    ):
        require(obsolete_call not in timer, f"obsolete Stage 5C bar entry point used: {obsolete_call}")
    for raw_bypass in (
        "pub fn into_stage5g_b_settled",
        "pub fn into_settled(self) -> Stage5cSettledPaperStrategy",
    ):
        require(raw_bypass not in timer, f"raw Stage 5G-d ownership bypass restored: {raw_bypass}")

    require("mod stage5g_timer;" in lib, "Stage 5G-d module not sealed in lib.rs")
    require(
        "attach_stage5g_market_terminal_timer_session" in lib,
        "R3 market-terminal timer attachment is not exported",
    )
    require(
        "Stage5gReplayCheckpoint" in order
        and "replay_checkpoint: Stage5gReplayCheckpoint" in order
        and "last_continuation_checkpoint_ts_utc_ms" in order,
        "Stage 5G-c exact replay checkpoint is not carried into convergence",
    )
    require(
        "fn stage5gd_timer_generated_cleanup_roundtrips_through_ack_truth_and_next_session()"
        in order,
        "complete timer-generated ACK/BrokerTruth route witness missing",
    )

    authority_start = order.index(
        "pub(crate) fn canonicalize_stage5g_order_position_evidence("
    )
    authority_end = order.index("fn canonicalize_broker_truth_snapshot(", authority_start)
    authority = order[authority_start:authority_end]
    for token in (
        "account_id.is_empty() || account_id.contains(':')",
        "canonicalize_broker_truth_snapshot(&mut evidence.broker_truth)?",
        "let identity = evidence_identity(&evidence)",
        "let fingerprint = canonical_evidence_fingerprint(&evidence)",
    ):
        require(token in authority, f"canonical evidence authority drift: {token}")
    require(
        order.count("canonical_evidence_fingerprint(") == 2,
        "canonical fingerprint must have one definition and one authority call",
    )
    require(
        order.count("canonicalize_broker_truth_snapshot(") == 2,
        "BrokerTruth canonicalizer escaped the single evidence authority",
    )
    for token in (
        "Some(existing) => merge_canonical_trade_observation_v1(existing, trade)",
        "Stage5gEvidenceCanonicalizationError::TradeIdentityConflict",
        "truth.trades = trades_by_id.into_values().collect()",
        "canonical_json_sort(&mut truth.orders)",
        "canonical_json_sort(&mut truth.positions)",
        "canonical_json_sort(&mut truth.instruments)",
        "canonical_json_sort(&mut cash.cash)",
    ):
        require(token in order, f"canonical BrokerTruth policy drift: {token}")

    decimal_start = order.index("fn canonical_decimal_v1(")
    projection_start = order.index("fn canonical_immutable_trade_payload_v1(", decimal_start)
    decimal = order[decimal_start:projection_start]
    for token in (
        "schema_version: STAGE5G_CANONICAL_DECIMAL_SCHEMA_VERSION",
        "domain: STAGE5G_CANONICAL_DECIMAL_DOMAIN",
        "exact_bytes: value.serialize()",
    ):
        require(token in decimal, f"exact Decimal projection drift: {token}")
    for forbidden in ("normalize()", ".abs()", "rescale(", "round("):
        require(forbidden not in decimal, f"Decimal exact policy normalized: {forbidden}")
    projection_end = order.index("fn immutable_trade_payload_matches(", projection_start)
    projection = order[projection_start:projection_end]
    for token in (
        "schema_version: STAGE5G_IMMUTABLE_TRADE_PAYLOAD_SCHEMA_VERSION",
        "domain: STAGE5G_IMMUTABLE_TRADE_PAYLOAD_DOMAIN",
        "account_id: trade.account_id.clone()",
        "broker_trade_id: trade.broker_trade_id.clone()",
        "broker_order_id: trade.broker_order_id.clone()",
        "client_order_id: trade.client_order_id.clone()",
        "instrument: trade.instrument.clone()",
        "side: trade.side",
        "qty: canonical_decimal_v1(trade.qty)",
        "price: canonical_decimal_v1(trade.price)",
        "gross_amount: trade.gross_amount.map(canonical_decimal_v1)",
        "commission: trade.commission.map(canonical_decimal_v1)",
        "broker_asset_id: trade.broker_asset_id.clone()",
        "board: trade.board.clone()",
        "expiration_date: trade.expiration_date",
        "source_ts: trade.source_ts",
    ):
        require(token in projection, f"immutable trade projection field drift: {token}")
    require(
        "instrument_identity_matches" not in projection,
        "broad instrument correlation entered immutable trade projection",
    )
    for forbidden in ("qty: trade.qty", "price: trade.price", "Option<Decimal>", ".normalize()"):
        require(forbidden not in projection, f"raw/numeric Decimal escaped exact projection: {forbidden}")
    require(
        (decimal + projection).count("canonical_decimal_v1") == 5,
        "exact Decimal authority must have one definition and four fixed-point uses",
    )
    matches_start = order.index("fn immutable_trade_payload_matches(")
    matches_end = order.index("fn merge_canonical_trade_observation_v1(", matches_start)
    matches = order[matches_start:matches_end]
    require(
        "canonical_immutable_trade_payload_v1(left)"
        " == canonical_immutable_trade_payload_v1(right)" in matches,
        "immutable trade equality escaped the versioned exact projection",
    )
    require(
        "instrument_identity_matches" not in matches,
        "broad instrument identity helper controls immutable trade equality",
    )
    for numeric_bypass in ("left.qty == right.qty", "left.price == right.price", ".normalize()"):
        require(numeric_bypass not in matches, f"numeric Decimal equality bypass: {numeric_bypass}")
    merge_start = matches_end
    merge_end = order.index("pub fn apply_stage5g_order_position_evidence(", merge_start)
    merge = order[merge_start:merge_end]
    merge_conflict = merge.index("if !immutable_trade_payload_matches(existing, &incoming)")
    merge_max = merge.index("if incoming.received_ts > existing.received_ts")
    merge_replace = merge.index("*existing = incoming")
    require(
        merge_conflict < merge_max < merge_replace,
        "deterministic immutable trade merge order drift",
    )
    require(
        order.count("canonical_immutable_trade_payload_v1(") == 3,
        "immutable trade projection must have one definition and one exact pair comparison",
    )
    require(
        order.count("merge_canonical_trade_observation_v1(") == 4,
        "snapshot and committed ledgers must share one deterministic trade merge authority",
    )

    active_start = order.index("pub fn apply_stage5g_order_position_evidence(")
    active_end = order.index("fn classify_evidence_replay(", active_start)
    active = order[active_start:active_end]
    require(
        active.count("canonicalize_stage5g_order_position_evidence(evidence)") == 1,
        "active path must use the single canonical evidence authority exactly once",
    )
    active_canonical = active.index("canonicalize_stage5g_order_position_evidence(evidence)")
    active_identity = active.index("canonical_evidence.identity()")
    active_fingerprint = active.index("canonical_evidence.fingerprint()")
    active_replay = active.index("classify_evidence_replay(")
    require(
        active_canonical < active_identity < active_fingerprint < active_replay,
        "active canonicalization/fingerprint/replay order drift",
    )
    for bypass in (
        "canonicalize_broker_truth_snapshot(",
        "evidence_identity(&evidence)",
        "canonical_evidence_fingerprint(&evidence)",
    ):
        require(bypass not in active, f"active raw canonicalization bypass: {bypass}")

    classifier_start = timer.index("pub fn classify_stage5g_post_checkpoint_evidence(")
    classifier_end = timer.index("fn checkpoint_envelope(", classifier_start)
    classifier = timer[classifier_start:classifier_end]
    canonicalize = classifier.index("canonicalize_stage5g_order_position_evidence(evidence)")
    canonical_identity = classifier.index("canonical_evidence.identity()")
    canonical_fingerprint = classifier.index("canonical_evidence.fingerprint()")
    known_identity = classifier.index("if let Some(previous)")
    continuation_guard = classifier.index("received_at.timestamp_millis() < continuation_checkpoint")
    receipt_regression = classifier.index("last_broker_truth_received_at")
    append_new = classifier.index("replay.evidence_identities.push")
    require(
        canonicalize
        < canonical_identity
        < canonical_fingerprint
        < known_identity
        < continuation_guard
        < receipt_regression
        < append_new,
        "restart classifier chronology/order drift",
    )
    require(
        classifier.count(
            "received_at.timestamp_millis() < continuation_checkpoint"
        )
        == 1,
        "restart continuation guard must occur exactly once after exact replay classification",
    )
    for bypass in (
        "evidence_identity(&evidence)",
        "canonical_evidence_fingerprint(&evidence)",
        "canonicalize_broker_truth_snapshot(",
    ):
        require(bypass not in classifier, f"restart raw canonicalization bypass: {bypass}")

    identity_start = order.index("fn evidence_identity(")
    identity_end = order.index("// STAGE5G-C-REPLAY-PACKAGE-IDENTITY-END")
    identity_body = order[identity_start:identity_end]
    require("total_sequence" not in identity_body, "local sequence entered package identity")
    require("fingerprint" not in identity_body, "payload fingerprint entered package identity")

    fingerprint_start = order.index("fn canonical_evidence_fingerprint(")
    fingerprint_end = order.index("fn replay_checkpoint(", fingerprint_start)
    fingerprint = order[fingerprint_start:fingerprint_end]
    require(
        ".normalize()" not in fingerprint and "rescale(" not in fingerprint,
        "fingerprint-only Decimal normalization violates exact stored-row policy",
    )

    require(inventory["stage"] == "5G-d", "inventory stage drift")
    require(inventory["status"] == "r1b_r5_review_candidate", "inventory status drift")
    require(inventory["accepted_predecessor"] == BASE, "inventory predecessor drift")
    require(len(inventory["scenario_family"]) == 8, "timer scenario inventory must remain 8/8")
    require(len(inventory["checkpoint_fields"]) == 8, "checkpoint field inventory drift")
    for surface, opened in inventory["closed_surfaces"].items():
        require(opened is False, f"closed surface opened: {surface}")

    require(descriptor["stage"] == "5G-d R1-b R5", "descriptor stage drift")
    require(
        descriptor["status"] == "implementation_review_candidate",
        "descriptor status drift",
    )
    require(descriptor["accepted_predecessor"] == BASE, "descriptor predecessor drift")
    require(descriptor["negative_case_count"] == 71, "descriptor negative count drift")
    for flag in (
        "restart_new_package_causal_guard",
        "historical_exact_replay_allowed",
        "ledger_latest_state_coherence",
        "single_canonical_evidence_authority",
        "active_restart_fingerprint_parity",
        "canonical_new_package_candidate_owned",
        "canonical_identity_grammar_enforced",
        "exact_immutable_trade_projection",
        "deterministic_trade_representative",
        "new_package_checkpoint_apply_required",
        "exact_decimal_representation",
        "decimal_scale_and_sign_bound",
        "canonical_trade_row_not_normalized",
    ):
        require(descriptor[flag] is True, f"descriptor R2 property missing: {flag}")
    for surface, opened in descriptor["closed_surfaces"].items():
        require(opened is False, f"descriptor closed surface opened: {surface}")

    require(
        "validate_stage5c_market_terminal_outcome_r2" not in timer
        and "validate_stage5c_market_terminal_outcome_r1" not in timer,
        "obsolete Market-terminal authority entered Stage 5G-d",
    )

    if check_git:
        accepted = subprocess.check_output(
            ["git", "show", f"{STAGE5C_AUTHORITY}:crates/strategy-runtime-core/src/stage5c_paper_host.rs"],
            cwd=root,
            text=True,
        )
        normalized_stage5c = re.sub(
            r"\n// STAGE5G-D-R1B-R1-ZERO-REARM-BEGIN.*?"
            r"// STAGE5G-D-R1B-R1-ZERO-REARM-END\n",
            "",
            stage5c,
            flags=re.S,
        )
        require(
            normalized_stage5c == accepted,
            "Stage 5C changed outside the single no-callback zero-intent re-arm bridge",
        )


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument("--skip-git", action="store_true")
    args = parser.parse_args()
    try:
        validate(args.root.resolve(), check_git=not args.skip_git)
    except (CheckFailure, ValueError, KeyError, json.JSONDecodeError) as error:
        print(f"stage5g-d-check: FAIL: {error}")
        return 1
    print("stage5g-d-check: PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
