#!/usr/bin/env python3
"""Executable semantic mutations for the R2 terminal-fill boundary."""

from __future__ import annotations

import os
import shutil
import subprocess
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
STAGE5C = Path("crates/strategy-runtime-core/src/stage5c_paper_host.rs")
RUNTIME = Path("crates/strategy-runtime-core/src/hybrid_intraday_runtime.rs")
TEST_PREFIX = "stage5c_paper_host::stage5g_r2ca_r2_tests::"

CASES = (
    (
        "full-fill-contradiction-guard-removed",
        STAGE5C,
        "&& facts.filled_qty == facts.order_qty\n    {",
        "&& false\n    {",
        "r2_canceled_or_expired_full_fill_blocks_entry_and_exit_for_both_ack_paths",
    ),
    (
        "canceled-full-fill-admitted",
        STAGE5C,
        "facts.order_status,\n        broker_core::OrderStatus::Canceled | broker_core::OrderStatus::Expired\n    ) && facts.filled_qty",
        "facts.order_status,\n        broker_core::OrderStatus::Expired\n    ) && facts.filled_qty",
        "r2_canceled_or_expired_full_fill_blocks_entry_and_exit_for_both_ack_paths",
    ),
    (
        "expired-full-fill-admitted",
        STAGE5C,
        "facts.order_status,\n        broker_core::OrderStatus::Canceled | broker_core::OrderStatus::Expired\n    ) && facts.filled_qty",
        "facts.order_status,\n        broker_core::OrderStatus::Canceled\n    ) && facts.filled_qty",
        "r2_canceled_or_expired_full_fill_blocks_entry_and_exit_for_both_ack_paths",
    ),
    (
        "bracket-grace-ignored",
        STAGE5C,
        ".stage5g_r2ca_r2_bracket_reconcile_active_at(evidence_now_ms);",
        ".stage5g_r2ca_r2_bracket_reconcile_active_at(evidence_now_ms) && false;",
        "r2_partial_exit_inside_grace_is_timer_ready_then_timer_escrows_residual",
    ),
    (
        "wall-clock-reintroduced",
        STAGE5C,
        "facts.lifecycle_event_ts_utc.checked_mul(1_000)",
        "Some(chrono::Utc::now().timestamp_millis())",
        "r2_same_state_and_evidence_are_independent_of_process_wall_clock",
    ),
    (
        "empty-partial-intent-terminalized",
        STAGE5C,
        "if generated_intents.is_empty() && !bracket_grace_active {",
        "if generated_intents.is_empty() {",
        "r2_partial_exit_inside_grace_is_timer_ready_then_timer_escrows_residual",
    ),
    (
        "candidate-rollback-removed",
        STAGE5C,
        "Err(reason) => return Err(stage5c_r2_block(reason, resolved)),",
        "Err(reason) => panic!(\"candidate failure consumed capability: {reason:?}\"),",
        "r2_candidate_failure_rolls_back_exact_state_and_allows_corrected_retry",
    ),
    (
        "owner-cycle-preflight-removed",
        RUNTIME,
        "&& self.active_cycle_id == Some(entry.cycle_id)",
        "&& self.active_cycle_id.is_some()",
        "r2_source_owner_cycle_preflight_blocks_request_only_authority",
    ),
    (
        "source-recovered-path-witness-removed",
        STAGE5C,
        "AckPath::SubmittedRecovered => {",
        "AckPath::Accepted => {",
        "r2_source_path_zero_fill_entry_rejected_and_recovered_canceled_are_timer_ready",
    ),
    (
        "timer-private-state-sync-removed",
        RUNTIME,
        "// Timer helpers mutate private pending lifecycle fields. Publish that\n        // exact mutation before Stage 5C validates/escrows generated intents.\n        self.sync_state();",
        "// Mutant deliberately leaves public state stale.",
        "r2_partial_exit_inside_grace_is_timer_ready_then_timer_escrows_residual",
    ),
)


def main() -> int:
    target = ROOT / "target" / "stage5g-c-r2ca-r2-semantic-negative"
    passed = 0
    for name, relative, old, new, test in CASES:
        with tempfile.TemporaryDirectory(prefix="stage5g-r2ca-r2-semantic-") as raw:
            repo = Path(raw) / "repo"
            shutil.copytree(
                ROOT,
                repo,
                ignore=shutil.ignore_patterns(".git", "target", "reports", "tmp", "*.log", "*.zip"),
            )
            path = repo / relative
            source = path.read_text()
            if source.count(old) != 1:
                raise RuntimeError(f"mutation anchor cardinality drift: {name} ({source.count(old)})")
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
    print(f"stage5g-c-r2ca-r2-semantic-negative-harness: PASS {passed}/{len(CASES)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
