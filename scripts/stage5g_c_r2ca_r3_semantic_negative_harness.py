#!/usr/bin/env python3
"""Executable receipt-clock mutations for the R3 terminal boundary."""

from __future__ import annotations

import os
import shutil
import subprocess
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SOURCE = Path("crates/strategy-runtime-core/src/stage5c_paper_host.rs")
TEST_PREFIX = "stage5c_paper_host::stage5g_r2ca_r3_tests::"

CASES = (
    (
        "receipt-milliseconds-truncated",
        "let evidence_received_ms = evidence.truth.received_ts.timestamp_millis();",
        "let evidence_received_ms = evidence.truth.received_ts.timestamp() * 1_000;",
        "r3_same_second_post_start_receipt_uses_inside_grace_policy",
    ),
    (
        "receipt-clock-rebound-to-component-source",
        "let evidence_received_ms = evidence.truth.received_ts.timestamp_millis();",
        "let evidence_received_ms = evidence.truth.orders[0].source_ts.expect(\"source\").timestamp_millis();",
        "r3_fresh_snapshot_same_source_later_receipt_unblocks_retry",
    ),
    (
        "before-timer-check-removed",
        "if is_partial_exit && bracket_started_ms.is_some_and(|started| evidence_received_ms < started) {",
        "if false && bracket_started_ms.is_some_and(|started| evidence_received_ms < started) {",
        "r3_pre_timer_receipt_blocks_and_preserves_capability",
    ),
    (
        "after-grace-receipt-treated-inside-grace",
        ".stage5g_r2ca_r2_bracket_reconcile_active_at(evidence_received_ms);",
        ".stage5g_r2ca_r2_bracket_reconcile_active_at(bracket_started_ms.unwrap_or(evidence_received_ms));",
        "r3_delayed_receipt_after_grace_escrows_recovery_immediately",
    ),
    (
        "full-fill-contradiction-removed",
        "&& facts.filled_qty == facts.order_qty\n    {",
        "&& false\n    {",
        "r3_inherits_full_fill_contradiction_and_transaction_rollback",
    ),
    (
        "candidate-rollback-removed",
        "Err(reason) => return Err(stage5c_r2_block(reason, resolved)),",
        "Err(reason) => panic!(\"R2 candidate consumed: {reason:?}\"),",
        "r3_inherits_full_fill_contradiction_and_transaction_rollback",
    ),
)


def main() -> int:
    target = ROOT / "target" / "stage5g-c-r2ca-r3-semantic-negative"
    passed = 0
    for name, old, new, test in CASES:
        with tempfile.TemporaryDirectory(prefix="stage5g-r2ca-r3-semantic-") as raw:
            repo = Path(raw) / "repo"
            shutil.copytree(
                ROOT,
                repo,
                ignore=shutil.ignore_patterns(".git", "target", "reports", "tmp", "*.log", "*.zip"),
            )
            path = repo / SOURCE
            source = path.read_text()
            occurrence_count = source.count(old)
            if occurrence_count < 1:
                raise RuntimeError(f"mutation anchor missing: {name}")
            if name == "full-fill-contradiction-removed":
                if occurrence_count != 2:
                    raise RuntimeError("R2/R3 full-fill guard cardinality drift")
                path.write_text(source.replace(old, new))
            else:
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
    print(f"stage5g-c-r2ca-r3-semantic-negative-harness: PASS {passed}/{len(CASES)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
