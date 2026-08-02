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
    inventory = "docs/stage-5/stage5g-d-timer-continuation-inventory.json"
    cases = [
        ("drop-exact-nanos", lambda r: mutate_all(r, timer, "timestamp_subsec_nanos", "timestamp_subsec_millis")),
        ("omit-replay-ledger", lambda r: mutate_all(r, timer, "evidence_replay_ledger", "removed_replay_ledger")),
        ("omit-exact-watermark", lambda r: mutate_all(r, timer, "last_broker_truth_received_at", "removed_exact_watermark")),
        ("omit-ms-watermark", lambda r: mutate_all(r, timer, "last_broker_truth_received_ms", "removed_ms_watermark")),
        ("omit-local-sequence", lambda r: mutate_all(r, timer, "last_total_sequence", "removed_total_sequence")),
        ("allow-equal-timer", lambda r: mutate_text(r, timer, "input.now_ts_utc_ms <= last", "input.now_ts_utc_ms < last")),
        ("open-scheduler", lambda r: mutate_text(r, timer, "use broker_core::StrategyRequestId;", "use broker_core::StrategyRequestId;\nuse std::thread;")),
        ("open-redis", lambda r: mutate_text(r, timer, "use broker_core::StrategyRequestId;", "use broker_core::StrategyRequestId;\nuse redis::Client;")),
        ("open-finam-http", lambda r: mutate_text(r, timer, "use broker_core::StrategyRequestId;", "use broker_core::StrategyRequestId;\nuse reqwest::Method;")),
        ("remove-generated-escrow", lambda r: mutate_text(r, timer, "pub struct Stage5gTimerGeneratedIntentEscrow", "struct RemovedGeneratedIntentEscrow")),
        ("sequence-in-package-identity", lambda r: mutate_text(r, order, "evidence.request_id,", "evidence.total_sequence,\n        evidence.request_id,")),
        ("open-stage5g-e", lambda r: mutate_text(r, inventory, '"stage5g_e": false', '"stage5g_e": true')),
    ]
    for label, mutation in cases:
        must_fail(label, mutation)
    print(f"stage5g-d-negative-harness: PASS {len(cases)}/{len(cases)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
