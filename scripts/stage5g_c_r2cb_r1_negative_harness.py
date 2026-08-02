#!/usr/bin/env python3
"""Semantic mutations for Stage 5G-c R2-c-b R1 chronology."""

from __future__ import annotations

import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
ORDER = Path("crates/strategy-runtime-core/src/stage5g_order_position.rs")
DESCRIPTOR = Path("docs/stage-5/stage5g-c-r2cb-r1-historical-trade-replay-chronology.json")
FINAM_FIXTURE = Path("fixtures/finam/stage5g_r2cb_full_snapshot_sequence.json")
GOLDEN = Path("fixtures/expected/stage5g_r2cb_three_poll_broker_truth.json")
CHECKER = Path("scripts/stage5g_c_r2cb_r1_authority_check.py")

CASES = (
    (
        "known-trade-chronology-branch-removed",
        ORDER,
        "STAGE5G-C-R2CB-R1-KNOWN-TRADE-CHRONOLOGY-BEGIN",
        "STAGE5G-C-R2CB-R1-KNOWN-TRADE-CHRONOLOGY-REMOVED",
    ),
    (
        "full-struct-equality-restored",
        ORDER,
        "if !immutable_trade_payload_matches(known, trade)",
        "if known != *trade",
    ),
    (
        "known-trade-subjected-to-global-source-watermark",
        ORDER,
        "if trade.received_ts < known.received_ts {",
        "if trade.received_ts < known.received_ts\n                || slot.last_trade_source_ts.is_some_and(|last| trade.source_ts < last)\n            {",
    ),
    (
        "known-received-regression-allowed",
        ORDER,
        "if trade.received_ts < known.received_ts {",
        "if trade.received_ts > known.received_ts {",
    ),
    (
        "known-trade-double-counted",
        ORDER,
        "            continue;\n        }\n        // STAGE5G-C-R2CB-R1-KNOWN-TRADE-CHRONOLOGY-END",
        "        }\n        // STAGE5G-C-R2CB-R1-KNOWN-TRADE-CHRONOLOGY-END",
    ),
    (
        "third-poll-native-fixture-removed",
        FINAM_FIXTURE,
        '"poll3": {',
        '"poll_removed": {',
    ),
    (
        "connector-neutral-third-poll-removed",
        GOLDEN,
        '"order_status": "filled"',
        '"order_status": "partially_filled"',
    ),
    (
        "runtime-three-poll-witness-removed",
        ORDER,
        "r2cb_public_runtime_three_poll_golden_converges_through_stage5c",
        "r2cb_public_runtime_witness_removed",
    ),
    (
        "immutable-trade-price-conflict-weakened",
        ORDER,
        "        && left.price == right.price\n",
        "",
    ),
    (
        "unseen-late-trade-policy-removed",
        ORDER,
        "A previously unseen late trade remains fail closed",
        "A previously unseen late trade is accepted",
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
        with tempfile.TemporaryDirectory(prefix="stage5g-r2cb-r1-negative-") as raw:
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
    print(f"stage5g-c-r2cb-r1-negative-harness: PASS {passed}/{len(CASES)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
