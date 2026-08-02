#!/usr/bin/env python3
"""Semantic mutations for R2 trade-ledger/watermark coherence."""

from __future__ import annotations

import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
ORDER = Path("crates/strategy-runtime-core/src/stage5g_order_position.rs")
DESCRIPTOR = Path("docs/stage-5/stage5g-c-r2cb-r2-trade-ledger-watermark-coherence.json")
CHECKER = Path("scripts/stage5g_c_r2cb_r2_authority_check.py")

CASES = (
    (
        "historical-max-replaced-with-or-previous",
        ORDER,
        "[slot.last_trade_source_ts, committed_source_max]\n        .into_iter()\n        .flatten()\n        .max()",
        "committed_source_max.or(slot.last_trade_source_ts)",
    ),
    (
        "previous-global-receipt-watermark-discarded",
        ORDER,
        "[slot.last_trade_received_ts, committed_received_max]",
        "[None, committed_received_max]",
    ),
    (
        "subset-refresh-witness-removed",
        ORDER,
        "r2cb_r2_subset_refresh_preserves_committed_max_and_blocks_unseen_late_trade",
        "r2cb_r2_subset_refresh_witness_removed",
    ),
    (
        "unseen-late-after-subset-witness-removed",
        ORDER,
        "assert_eq!(\n            blocked.reason(),\n            Stage5gOrderPositionError::TradeTimeRegression\n        );",
        "assert_ne!(\n            blocked.reason(),\n            Stage5gOrderPositionError::TradeTimeRegression\n        );",
    ),
    (
        "position-only-target-trade-block-removed",
        ORDER,
        "STAGE5G-C-R2CB-R2-POSITION-ONLY-TRADE-BLOCK-BEGIN",
        "STAGE5G-C-R2CB-R2-POSITION-ONLY-TRADE-BLOCK-REMOVED",
    ),
    (
        "trade-watermark-advanced-before-ledger-commit",
        ORDER,
        "    let result = apply_to_slot(\n",
        "    refresh_trade_watermarks_from_committed_ledger(&mut next_slot);\n    let result = apply_to_slot(\n",
    ),
    (
        "contradictory-client-id-position-only-witness-removed",
        ORDER,
        "r2cb_r2_position_only_contradictory_target_trades_are_never_ignored",
        "r2cb_r2_position_only_contradictory_target_trades_are_ignored",
    ),
    (
        "terminal-target-trade-guard-removed",
        ORDER,
        "if has_target_order || has_target_trade || contradictory_target_position",
        "if has_target_order || contradictory_target_position",
    ),
    (
        "accepted-three-poll-public-witness-removed",
        ORDER,
        "r2cb_public_runtime_three_poll_golden_converges_through_stage5c",
        "r2cb_public_runtime_three_poll_witness_removed",
    ),
    (
        "typed-position-only-reason-weakened",
        ORDER,
        "Stage5gOrderPositionError::TargetTradeWithoutOrder,\n            session,",
        "Stage5gOrderPositionError::PositionIncomplete,\n            session,",
    ),
    (
        "r3-entry-point-bypassed",
        ORDER,
        "validate_stage5c_market_terminal_outcome_r3",
        "validate_stage5c_market_terminal_outcome_r2",
    ),
    (
        "stage5g-d-live-opened",
        DESCRIPTOR,
        '"stage5g_d": false',
        '"stage5g_d": true',
    ),
)


def copy_root(destination: Path) -> None:
    shutil.copytree(
        ROOT,
        destination,
        ignore=shutil.ignore_patterns(".git", "target", "reports", "tmp", "*.log", "*.zip"),
    )


def main() -> int:
    passed = 0
    for name, relative, old, new in CASES:
        with tempfile.TemporaryDirectory(prefix="stage5g-r2cb-r2-negative-") as raw:
            mutant = Path(raw) / "repo"
            copy_root(mutant)
            path = mutant / relative
            source = path.read_text()
            if old not in source:
                raise RuntimeError(f"mutation anchor missing: {name}")
            path.write_text(source.replace(old, new, 1))
            result = subprocess.run(
                [sys.executable, str(mutant / CHECKER), "--root", str(mutant)],
                cwd=mutant,
                text=True,
                capture_output=True,
                check=False,
            )
            if result.returncode == 0:
                print(f"FAIL mutation survived: {name}")
                return 1
            print(f"PASS {name}")
            passed += 1
    print(f"stage5g-c-r2cb-r2-negative-harness: PASS {passed}/{len(CASES)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
