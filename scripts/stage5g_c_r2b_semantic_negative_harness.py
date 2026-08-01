#!/usr/bin/env python3
"""Mutation tests for Stage 5G-c R2-b semantic lifecycle guards."""

from __future__ import annotations

import os
import shutil
import subprocess
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SOURCE = Path("crates/strategy-runtime-core/src/stage5g_order_position.rs")

CASES = (
    (
        "entry-position-regression-guard-removed",
        "if qty.abs() + f64::EPSILON < previous.abs() {",
        "if false {",
        "r2b_market_entry_position_progress_is_monotonic",
    ),
    (
        "exit-position-regression-guard-removed",
        "if qty.abs() > previous.abs() + f64::EPSILON {",
        "if false {",
        "r2b_market_exit_position_progress_is_monotonic_until_flat",
    ),
    (
        "working-market-order-made-terminal",
        "OrderStatus::New | OrderStatus::Working | OrderStatus::PartiallyFilled => {\n                        slot.terminal = false;",
        "OrderStatus::New | OrderStatus::Working | OrderStatus::PartiallyFilled => {\n                        slot.terminal = true;",
        "r2b_target_market_order_status_is_authoritative_when_present",
    ),
    (
        "uncorrelated-trade-poisons-watermark",
        "&& (trade.broker_order_id.as_ref() == target_order_id\n                    || trade.client_order_id.as_ref() == Some(target_client_order_id))",
        "&& true",
        "r2b_only_exact_correlated_trade_advances_slot_watermark",
    ),
)


def main() -> int:
    passed = 0
    target_dir = ROOT / "target" / "stage5g-c-r2b-negative"
    for name, old, new, test_name in CASES:
        with tempfile.TemporaryDirectory(prefix="stage5g-c-r2b-semantic-") as raw:
            repo = Path(raw) / "repo"
            shutil.copytree(
                ROOT,
                repo,
                ignore=shutil.ignore_patterns(".git", "target", "reports", "tmp", "*.log"),
            )
            path = repo / SOURCE
            source = path.read_text()
            if source.count(old) != 1:
                raise RuntimeError(f"mutation anchor cardinality drift: {name}")
            path.write_text(source.replace(old, new, 1))
            environment = os.environ.copy()
            environment["CARGO_TARGET_DIR"] = str(target_dir)
            result = subprocess.run(
                [
                    "cargo",
                    "test",
                    "-p",
                    "strategy-runtime-core",
                    f"stage5g_order_position::tests::{test_name}",
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
    print(f"stage5g-c-r2b-semantic-negative-harness: PASS {passed}/{len(CASES)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
