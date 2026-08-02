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
    cases = [
        ("drop-exact-nanos", lambda r: mutate_all(r, timer, "timestamp_subsec_nanos", "timestamp_subsec_millis")),
        ("omit-replay-ledger", lambda r: mutate_all(r, timer, "evidence_replay_ledger", "removed_replay_ledger")),
        ("omit-exact-watermark", lambda r: mutate_all(r, timer, "last_broker_truth_received_at", "removed_exact_watermark")),
        ("omit-ms-watermark", lambda r: mutate_all(r, timer, "last_broker_truth_received_ms", "removed_ms_watermark")),
        ("omit-local-sequence", lambda r: mutate_all(r, timer, "last_total_sequence", "removed_total_sequence")),
        ("use-older-bar-entrypoint", lambda r: mutate_all(r, timer, "advance_stage5c_timer_settlement_next_bar_transactional_at_checkpoint", "advance_stage5c_timer_settlement_next_bar")),
        ("understate-inner-checkpoint", lambda r: mutate_text(r, timer, "checkpoint_ts_utc_ms < inner", "false && checkpoint_ts_utc_ms < inner")),
        ("allow-equal-timer", lambda r: mutate_text(r, timer, "input.now_ts_utc_ms <= last", "input.now_ts_utc_ms < last")),
        ("reintroduce-wall-clock", lambda r: mutate_text(r, timer, "use chrono::{DateTime, Utc};", "use chrono::{DateTime, Utc};\nconst WALL_CLOCK: fn() -> chrono::DateTime<Utc> = Utc::now;")),
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
        ("sequence-in-package-identity", lambda r: mutate_text(r, order, "evidence.request_id,", "evidence.total_sequence,\n        evidence.request_id,")),
        ("open-stage5g-e", lambda r: mutate_text(r, inventory, '"stage5g_e": false', '"stage5g_e": true')),
        ("open-stage5g-f", lambda r: mutate_text(r, inventory, '"stage5g_f": false', '"stage5g_f": true')),
    ]
    for label, mutation in cases:
        must_fail(label, mutation)
    print(f"stage5g-d-negative-harness: PASS {len(cases)}/{len(cases)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
