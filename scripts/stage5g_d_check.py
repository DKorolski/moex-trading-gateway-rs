#!/usr/bin/env python3
"""Fail-closed source/contract checker for Stage 5G-d."""

from __future__ import annotations

import argparse
import json
import re
import subprocess
from pathlib import Path

BASE = "e6d2d94d709ff2f6b589a565e255dbb0049d2705"
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
        "evidence.broker_truth.received_ts.timestamp_millis() < continuation_checkpoint",
        ".is_some_and(|last| evidence.broker_truth.received_ts < last)",
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
    )
    for token in required_tokens:
        require(token in timer, f"required timer contract token missing: {token}")

    for token in (
        "BrokerTruthBeforeContinuationCheckpoint",
        "evidence.broker_truth.received_ts.timestamp_millis() < checkpoint",
        "current_evidence_identity: Option<String>",
        "stage5gd_zero_intent_bar_rearms_timer_and_later_bar_without_callback_loss",
    ):
        require(token in order, f"required chronology/liveness witness missing: {token}")
    for token in (
        "new_post_restore_package_requires_continuation_chronology_but_exact_replay_does_not",
        "multi_package_restore_requires_ordered_ledger_and_latest_current_projection",
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

    classifier_start = timer.index("pub fn classify_stage5g_post_checkpoint_evidence(")
    classifier_end = timer.index("fn checkpoint_envelope(", classifier_start)
    classifier = timer[classifier_start:classifier_end]
    known_identity = classifier.index("if let Some(previous)")
    continuation_guard = classifier.index(
        "evidence.broker_truth.received_ts.timestamp_millis() < continuation_checkpoint"
    )
    receipt_regression = classifier.index("last_broker_truth_received_at")
    append_new = classifier.index("replay.evidence_identities.push")
    require(
        known_identity < continuation_guard < receipt_regression < append_new,
        "restart classifier chronology/order drift",
    )
    require(
        classifier.count(
            "evidence.broker_truth.received_ts.timestamp_millis() < continuation_checkpoint"
        )
        == 1,
        "restart continuation guard must occur exactly once after exact replay classification",
    )

    identity_start = order.index("fn evidence_identity(")
    identity_end = order.index("// STAGE5G-C-REPLAY-PACKAGE-IDENTITY-END")
    identity_body = order[identity_start:identity_end]
    require("total_sequence" not in identity_body, "local sequence entered package identity")
    require("fingerprint" not in identity_body, "payload fingerprint entered package identity")

    require(inventory["stage"] == "5G-d", "inventory stage drift")
    require(inventory["status"] == "r1b_r2_review_candidate", "inventory status drift")
    require(inventory["accepted_predecessor"] == BASE, "inventory predecessor drift")
    require(len(inventory["scenario_family"]) == 8, "timer scenario inventory must remain 8/8")
    require(len(inventory["checkpoint_fields"]) == 8, "checkpoint field inventory drift")
    for surface, opened in inventory["closed_surfaces"].items():
        require(opened is False, f"closed surface opened: {surface}")

    require(descriptor["stage"] == "5G-d R1-b R2", "descriptor stage drift")
    require(
        descriptor["status"] == "implementation_review_candidate",
        "descriptor status drift",
    )
    require(descriptor["accepted_predecessor"] == BASE, "descriptor predecessor drift")
    require(descriptor["negative_case_count"] == 42, "descriptor negative count drift")
    for flag in (
        "restart_new_package_causal_guard",
        "historical_exact_replay_allowed",
        "ledger_latest_state_coherence",
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
