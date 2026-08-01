#!/usr/bin/env python3
"""Executable semantic mutations for R2-c-a R1 state coherence."""

from __future__ import annotations

import os
import shutil
import subprocess
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SOURCE = Path("crates/strategy-runtime-core/src/stage5c_paper_host.rs")
TEST_PREFIX = "stage5c_paper_host::bootstrap_notification_tests::"

CASES = (
    (
        "confirmed-ack-authority-removed",
        "broker_core::HybridRuntimeAckStatus::Accepted\n                | broker_core::HybridRuntimeAckStatus::Confirmed",
        "broker_core::HybridRuntimeAckStatus::Accepted",
        "stage5g_r2ca_zero_fill_entry_resolves_pending_for_accepted_and_confirmed_ack",
    ),
    (
        "partial-fill-treated-as-existing-position",
        "facts.lifecycle_event_ts_utc,\n            false,",
        "facts.lifecycle_event_ts_utc,\n            true,",
        "stage5g_r2ca_partial_entry_and_exit_update_position_and_retain_recovery_intent",
    ),
    (
        "generated-recovery-intent-discarded",
        "generated_intent_batch = Some(callback_batch);",
        "drop(callback_batch);",
        "stage5g_r2ca_partial_entry_and_exit_update_position_and_retain_recovery_intent",
    ),
    (
        "order-source-receipt-chronology-disabled",
        ".is_some_and(|source_ts| source_ts > order.received_ts)",
        ".is_some_and(|_| false)",
        "stage5g_r2ca_rejects_non_monotonic_order_trade_and_position_chronology",
    ),
    (
        "trade-source-receipt-chronology-disabled",
        "trade.source_ts > trade.received_ts",
        "false",
        "stage5g_r2ca_rejects_non_monotonic_order_trade_and_position_chronology",
    ),
    (
        "position-source-receipt-chronology-disabled",
        "position_source_ts > position.received_ts",
        "false",
        "stage5g_r2ca_rejects_non_monotonic_order_trade_and_position_chronology",
    ),
)


def main() -> int:
    target = ROOT / "target" / "stage5g-c-r2ca-r1-semantic-negative"
    passed = 0
    for name, old, new, test in CASES:
        with tempfile.TemporaryDirectory(prefix="stage5g-r2ca-r1-semantic-") as raw:
            repo = Path(raw) / "repo"
            shutil.copytree(
                ROOT,
                repo,
                ignore=shutil.ignore_patterns(
                    ".git", "target", "reports", "tmp", "*.log", "*.zip"
                ),
            )
            path = repo / SOURCE
            source = path.read_text()
            if source.count(old) != 1:
                raise RuntimeError(f"mutation anchor cardinality drift: {name}")
            path.write_text(source.replace(old, new, 1))
            environment = os.environ.copy()
            environment["CARGO_TARGET_DIR"] = str(target)
            result = subprocess.run(
                [
                    "cargo",
                    "test",
                    "-p",
                    "strategy-runtime-core",
                    f"{TEST_PREFIX}{test}",
                    "--",
                    "--exact",
                ],
                cwd=repo,
                env=environment,
                check=False,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
            )
            if result.returncode == 0:
                print(f"FAIL mutation survived: {name}")
                return 1
            print(f"PASS {name}")
            passed += 1
    print(f"stage5g-c-r2ca-r1-semantic-negative-harness: PASS {passed}/{len(CASES)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
