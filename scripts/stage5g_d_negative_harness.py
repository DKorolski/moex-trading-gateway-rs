#!/usr/bin/env python3
"""Mutation harness for the Stage 5G-d fail-closed checker."""

from __future__ import annotations

import json
import shutil
import tempfile
from pathlib import Path

import stage5g_d_check as checker

ROOT = Path(__file__).resolve().parents[1]
PATHS = (
    "crates/strategy-runtime-core/src/stage5g_timer.rs",
    "crates/strategy-runtime-core/src/stage5g_order_position.rs",
    "crates/strategy-runtime-core/src/stage5c_paper_host.rs",
    "crates/strategy-runtime-core/src/lib.rs",
    "docs/stage-5/stage5g-d-timer-continuation-inventory.json",
    "docs/stage-5/stage5g-d-timer-continuation-contract.md",
    "docs/stage-5/stage5g-d-r1b-composition-restore.json",
)


def mutate_text(root: Path, relative: str, old: str, new: str) -> None:
    path = root / relative
    text = path.read_text()
    if old not in text:
        raise RuntimeError(f"mutation anchor missing: {relative}: {old}")
    path.write_text(text.replace(old, new, 1))


def mutate_all(root: Path, relative: str, old: str, new: str) -> None:
    path = root / relative
    text = path.read_text()
    if old not in text:
        raise RuntimeError(f"mutation anchor missing: {relative}: {old}")
    path.write_text(text.replace(old, new))


def move_before(root: Path, relative: str, block: str, anchor: str) -> None:
    path = root / relative
    text = path.read_text()
    if block not in text or anchor not in text:
        raise RuntimeError(f"move anchor missing: {relative}")
    text = text.replace(block, "", 1)
    path.write_text(text.replace(anchor, block + anchor, 1))


def move_after(root: Path, relative: str, block: str, anchor: str) -> None:
    path = root / relative
    text = path.read_text()
    if block not in text or anchor not in text:
        raise RuntimeError(f"move anchor missing: {relative}")
    text = text.replace(block, "", 1)
    path.write_text(text.replace(anchor, anchor + block, 1))


def must_fail(label: str, mutation) -> None:
    with tempfile.TemporaryDirectory(prefix="stage5g-d-negative-") as raw:
        root = Path(raw)
        for relative in PATHS:
            destination = root / relative
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(ROOT / relative, destination)
        mutation(root)
        try:
            checker.validate(root, check_git=False)
        except (checker.CheckFailure, ValueError, KeyError, json.JSONDecodeError):
            print(f"PASS {label}")
            return
        raise SystemExit(f"FAIL mutation escaped checker: {label}")


def main() -> int:
    timer = "crates/strategy-runtime-core/src/stage5g_timer.rs"
    order = "crates/strategy-runtime-core/src/stage5g_order_position.rs"
    stage5c = "crates/strategy-runtime-core/src/stage5c_paper_host.rs"
    inventory = "docs/stage-5/stage5g-d-timer-continuation-inventory.json"
    restart_guard = (
        "    let received_at = canonical_evidence.evidence().broker_truth.received_ts;\n"
        "    let continuation_checkpoint = envelope\n"
        "        .payload\n"
        "        .last_continuation_checkpoint_ts_utc_ms\n"
        "        .expect(\"validated Stage 5G-d checkpoint has a continuation watermark\");\n"
        "    if received_at.timestamp_millis() < continuation_checkpoint {\n"
        "        return Err(Stage5gCheckpointReplayError::BrokerTruthBeforeContinuationCheckpoint);\n"
        "    }\n"
    )
    restart_canonicalization = (
        "    let canonical_evidence =\n"
        "        canonicalize_stage5g_order_position_evidence(evidence).map_err(|reason| match reason {\n"
        "            Stage5gEvidenceCanonicalizationError::TradeIdentityConflict => {\n"
        "                Stage5gCheckpointReplayError::TradeIdentityConflict\n"
        "            }\n"
        "            Stage5gEvidenceCanonicalizationError::EvidenceIdentityGrammarViolation => {\n"
        "                Stage5gCheckpointReplayError::EvidenceIdentityGrammarViolation\n"
        "            }\n"
        "        })?;\n"
        "    let identity = canonical_evidence.identity().to_string();\n"
        "    let fingerprint = canonical_evidence.fingerprint().to_string();\n"
    )
    cases = [
        ("drop-exact-nanos", lambda r: mutate_all(r, timer, "timestamp_subsec_nanos", "timestamp_subsec_millis")),
        ("omit-replay-ledger", lambda r: mutate_all(r, timer, "evidence_replay_ledger", "removed_replay_ledger")),
        ("omit-exact-watermark", lambda r: mutate_all(r, timer, "last_broker_truth_received_at", "removed_exact_watermark")),
        ("omit-ms-watermark", lambda r: mutate_all(r, timer, "last_broker_truth_received_ms", "removed_ms_watermark")),
        ("omit-local-sequence", lambda r: mutate_all(r, timer, "last_total_sequence", "removed_total_sequence")),
        ("use-older-bar-entrypoint", lambda r: mutate_all(r, timer, "advance_stage5c_timer_settlement_next_bar_transactional_at_checkpoint", "advance_stage5c_timer_settlement_next_bar")),
        ("understate-inner-checkpoint", lambda r: mutate_text(r, timer, "checkpoint_ts_utc_ms < inner", "false && checkpoint_ts_utc_ms < inner")),
        ("allow-equal-timer", lambda r: mutate_text(r, timer, "input.now_ts_utc_ms <= last", "input.now_ts_utc_ms < last")),
        ("reintroduce-wall-clock", lambda r: mutate_text(r, timer, "use chrono::{DateTime, TimeZone, Utc};", "use chrono::{DateTime, TimeZone, Utc};\nconst WALL_CLOCK: fn() -> chrono::DateTime<Utc> = Utc::now;")),
        ("open-scheduler", lambda r: mutate_text(r, timer, "use broker_core::StrategyRequestId;", "use broker_core::StrategyRequestId;\nuse std::thread;")),
        ("open-redis", lambda r: mutate_text(r, timer, "use broker_core::StrategyRequestId;", "use broker_core::StrategyRequestId;\nuse redis::Client;")),
        ("open-finam-http", lambda r: mutate_text(r, timer, "use broker_core::StrategyRequestId;", "use broker_core::StrategyRequestId;\nuse reqwest::Method;")),
        ("remove-generated-escrow", lambda r: mutate_text(r, timer, "pub struct Stage5gTimerGeneratedIntentEscrow", "struct RemovedGeneratedIntentEscrow")),
        ("restore-raw-escrow-bypass", lambda r: mutate_text(r, timer, "impl Stage5gTimerGeneratedIntentEscrow {", "impl Stage5gTimerGeneratedIntentEscrow {\n    pub fn into_stage5g_b_settled(self) -> Stage5cSettledPaperStrategy { self.settled }")),
        ("restore-raw-bar-bypass", lambda r: mutate_text(r, timer, "impl Stage5gBarContinuationPaperStrategy {", "impl Stage5gBarContinuationPaperStrategy {\n    pub fn into_settled(self) -> Stage5cSettledPaperStrategy { self.settled }")),
        ("reset-retry-to-broker-ms", lambda r: mutate_text(r, timer, "summary,\n                            replay,\n                            last_continuation_checkpoint_ts_utc_ms,", "summary,\n                            replay,\n                            last_continuation_checkpoint_ts_utc_ms: replay.last_broker_truth_received_ms,")),
        ("drop-order-position-checkpoint", lambda r: mutate_text(r, timer, "replay.last_continuation_checkpoint_ts_utc_ms = max_optional_checkpoint", "replay.last_continuation_checkpoint_ts_utc_ms = None; let _ = max_optional_checkpoint")),
        ("remove-timer-generated-route-witness", lambda r: mutate_text(r, order, "fn stage5gd_timer_generated_cleanup_roundtrips_through_ack_truth_and_next_session()", "fn removed_timer_generated_route_witness()")),
        ("remove-zero-intent-ready-conversion", lambda r: mutate_all(r, timer, "Stage5gBarContinuationTransition::Ready", "Stage5gBarContinuationTransition::RemovedReady")),
        ("zero-intent-output-has-no-consumer", lambda r: mutate_text(r, order, "fn stage5gd_zero_intent_bar_rearms_timer_and_later_bar_without_callback_loss()", "fn removed_zero_intent_liveness_witness()")),
        ("remove-zero-intent-rearm-authority", lambda r: mutate_all(r, stage5c, "stage5gd_rearm_zero_intent_bar_continuation", "removed_zero_intent_rearm")),
        ("remove-exact-ack-checkpoint-guard", lambda r: mutate_text(r, timer, "event.ack.received_ts.timestamp_millis() < session.checkpoint_ts_utc_ms", "false")),
        ("reduce-ack-guard-to-seconds", lambda r: mutate_text(r, timer, "event.ack.received_ts.timestamp_millis() < session.checkpoint_ts_utc_ms", "event.ack.received_ts.timestamp() < session.checkpoint_ts_utc_ms.div_euclid(1_000)")),
        ("remove-broker-truth-checkpoint-guard", lambda r: mutate_text(r, order, "evidence.broker_truth.received_ts.timestamp_millis() < checkpoint", "false")),
        ("remove-timer-admission-wrapper", lambda r: mutate_all(r, timer, "Stage5gTimerOrderPositionAdmissionBlocked", "RemovedTimerOrderPositionAdmissionBlocked")),
        ("retry-returns-raw-ack-capability", lambda r: mutate_text(r, timer, "attach_stage5g_timer_order_position_session(self.resolved)", "compile_error!(\"raw ACK retry escape\")")),
        ("restore-suffix-only-current-identity", lambda r: mutate_text(r, timer, "entry.identity == current_identity", "entry.identity.ends_with(package_discriminator)")),
        ("omit-current-evidence-identity", lambda r: mutate_all(r, timer, "current_evidence_identity", "removed_current_evidence_identity")),
        ("remove-restart-continuation-guard", lambda r: mutate_text(r, timer, "received_at.timestamp_millis() < continuation_checkpoint", "false")),
        ("move-restart-guard-after-append", lambda r: move_after(r, timer, restart_guard, "    replay.evidence_identities.push(EvidenceIdentity {\n        identity: identity.clone(),\n        fingerprint,\n    });\n")),
        ("reduce-restart-guard-to-last-truth", lambda r: mutate_text(r, timer, "received_at.timestamp_millis() < continuation_checkpoint", "replay.last_broker_truth_received_at.is_some_and(|last| received_at < last)")),
        ("remove-restart-broker-truth-regression", lambda r: mutate_text(r, timer, ".is_some_and(|last| received_at < last)", ".is_some_and(|_| false)")),
        ("block-exact-replay-by-continuation", lambda r: move_before(r, timer, restart_guard, "    let identity = canonical_evidence.identity().to_string();\n")),
        ("remove-corrected-later-retry-witness", lambda r: mutate_text(r, timer, "fn new_post_restore_package_requires_continuation_chronology_but_exact_replay_does_not()", "fn removed_corrected_later_retry_witness()")),
        ("allow-older-current-ledger-entry", lambda r: mutate_text(r, timer, "final_ledger_identity != Some(current_identity)", "false")),
        ("remove-ledger-receipt-order", lambda r: mutate_text(r, timer, "previous_ledger_receipt.is_some_and(|previous| parsed.received_at < previous)", "false")),
        ("allow-current-receipt-below-ledger-maximum", lambda r: mutate_text(r, timer, "current.received_at != received_at || final_ledger_receipt != Some(received_at)", "current.received_at != received_at && final_ledger_receipt != Some(received_at)")),
        ("remove-multi-package-restore-negative", lambda r: mutate_text(r, timer, "fn multi_package_restore_requires_ordered_ledger_and_latest_current_projection()", "fn removed_multi_package_restore_negative()")),
        ("remove-restart-canonical-authority", lambda r: mutate_text(r, timer, "canonicalize_stage5g_order_position_evidence(evidence)", "removed_stage5g_order_position_evidence_authority(evidence)")),
        ("restart-hashes-raw-evidence", lambda r: mutate_text(r, timer, "let fingerprint = canonical_evidence.fingerprint().to_string();", "let fingerprint = canonical_evidence_fingerprint(&evidence);")),
        ("split-active-restart-canonicalizers", lambda r: mutate_text(r, order, "canonicalize_stage5g_order_position_evidence(evidence)", "canonicalize_stage5g_order_position_evidence_active(evidence)")),
        ("remove-exact-duplicate-restart-witness", lambda r: mutate_text(r, timer, "fn post_checkpoint_duplicate_trade_redelivery_matches_active_canonical_fingerprint()", "fn removed_duplicate_trade_redelivery_witness()")),
        ("remove-conflicting-trade-restart-witness", lambda r: mutate_text(r, timer, "fn post_checkpoint_known_payload_change_and_trade_identity_conflict_fail_closed()", "fn removed_trade_identity_conflict_witness()")),
        ("count-canonical-trade-twice", lambda r: mutate_text(r, order, "truth.trades = trades_by_id.into_values().collect();", "truth.trades = trades_by_id.into_values().flat_map(|trade| [trade.clone(), trade]).collect();")),
        ("allow-conflicting-trade-identity", lambda r: mutate_text(r, order, "if !immutable_trade_payload_matches(existing, &incoming)", "if false && !immutable_trade_payload_matches(existing, &incoming)")),
        ("remove-canonical-candidate-witness", lambda r: mutate_text(r, timer, "fn new_post_checkpoint_package_owns_one_deduplicated_canonical_candidate()", "fn removed_canonical_candidate_witness()")),
        ("remove-active-canonical-fingerprint-witness", lambda r: mutate_text(r, order, "fn stage5gd_active_path_stores_single_authority_canonical_fingerprint()", "fn removed_active_canonical_fingerprint_witness()")),
        ("remove-canonical-identity-grammar-witness", lambda r: mutate_text(r, timer, "fn replay_identity_grammar_requires_canonical_uuid_and_colon_free_account()", "fn removed_identity_grammar_witness()")),
        ("replace-exact-trade-projection-with-broad-instrument-identity", lambda r: mutate_text(r, order, "canonical_immutable_trade_payload_v1(left) == canonical_immutable_trade_payload_v1(right)", "instrument_identity_matches(&left.instrument, &right.instrument)")),
        ("remove-instrument-from-immutable-trade-projection", lambda r: mutate_text(r, order, "instrument: trade.instrument.clone(),", "instrument: InstrumentId { venue_symbol: None, ..trade.instrument.clone() },")),
        ("restore-first-row-trade-representative", lambda r: mutate_text(r, order, "*existing = incoming;", "existing.received_ts = incoming.received_ts;")),
        ("remove-max-observation-receipt-policy", lambda r: mutate_text(r, order, "incoming.received_ts > existing.received_ts", "incoming.received_ts < existing.received_ts")),
        ("remove-optional-venue-permutation-witness", lambda r: mutate_text(r, order, "fn stage5gd_r4_optional_venue_permutations_fail_closed_without_first_row_authority()", "fn removed_optional_venue_permutation_witness()")),
        ("remove-same-venue-conflicting-fields-witness", lambda r: mutate_text(r, order, "fn stage5gd_r4_same_venue_conflicting_instrument_fields_fail_closed()", "fn removed_same_venue_conflicting_fields_witness()")),
        ("remove-r4-active-restart-parity-witness", lambda r: mutate_text(r, timer, "fn stage5gd_r4_active_restart_exact_duplicate_reversal_is_exact_replay()", "fn removed_r4_active_restart_parity_witness()")),
        ("move-r4-conflict-after-replay-ledger-append", lambda r: move_after(r, timer, restart_canonicalization, "    replay.evidence_identities.push(EvidenceIdentity {\n        identity: identity.clone(),\n        fingerprint,\n    });\n")),
        ("sequence-in-package-identity", lambda r: mutate_text(r, order, "evidence.request_id,", "evidence.total_sequence,\n        evidence.request_id,")),
        ("open-stage5g-e", lambda r: mutate_text(r, inventory, '"stage5g_e": false', '"stage5g_e": true')),
        ("open-stage5g-f", lambda r: mutate_text(r, inventory, '"stage5g_f": false', '"stage5g_f": true')),
        ("open-runtime-live", lambda r: mutate_text(r, inventory, '"runtime_live": false', '"runtime_live": true')),
    ]
    for label, mutation in cases:
        must_fail(label, mutation)
    print(f"stage5g-d-negative-harness: PASS {len(cases)}/{len(cases)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
