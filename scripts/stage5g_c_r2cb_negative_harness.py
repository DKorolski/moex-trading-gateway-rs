#!/usr/bin/env python3
"""Governance mutations for Stage 5G-c R2-c-b parity."""

from __future__ import annotations

import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
ORDER = Path("crates/strategy-runtime-core/src/stage5g_order_position.rs")
DESCRIPTOR = Path("docs/stage-5/stage5g-c-r2cb-broker-truth-finam-parity.json")
FIXTURE = Path("fixtures/finam/stage5g_r2cb_full_snapshot_sequence.json")
CHECKER = Path("scripts/stage5g_c_r2cb_authority_check.py")

CASES = (
    ("r3-entry-points-bypassed", ORDER, "validate_stage5c_market_terminal_outcome_r3", "validate_stage5c_market_terminal_outcome_r2", True),
    ("receipt-milliseconds-removed", ORDER, ".timestamp_millis()", ".timestamp()", True),
    ("trade-receipt-made-immutable", ORDER, "&& left.source_ts == right.source_ts\n}", "&& left.source_ts == right.source_ts\n        && left.received_ts == right.received_ts\n}", False),
    ("same-snapshot-trade-map-removed", ORDER, "BTreeMap<String, BrokerTradeSnapshot>", "Vec<BrokerTradeSnapshot>", True),
    ("absent-position-flat-removed", ORDER, "CanonicalPositionDerivation::AbsentFlat", "CanonicalPositionDerivation::ExplicitSingle", True),
    ("market-order-status-classification-removed", ORDER, "match order.status {", "match OrderStatus::Filled {", True),
    ("fill-position-coherence-removed", ORDER, "validate_order_position_coherence", "accept_order_position_without_coherence", True),
    ("canonical-order-sort-removed", ORDER, "canonical_json_sort(&mut truth.orders);", "truth.orders.reverse();", True),
    ("exact-receipt-continuation-removed", ORDER, "last_broker_truth_received_ms", "last_broker_truth_received_seconds", True),
    ("live-surface-opened", DESCRIPTOR, '"runtime_live": false', '"runtime_live": true', False),
    ("repeated-trade-payload-changed", FIXTURE, '"trade_id": "FINAM-R2CB-TRADE-A"', '"trade_id": "FINAM-R2CB-TRADE-X"', False),
)


def copy_root(destination: Path) -> None:
    shutil.copytree(
        ROOT,
        destination,
        ignore=shutil.ignore_patterns(".git", "target", "reports", "tmp", "*.log", "*.zip"),
    )


def main() -> int:
    passed = 0
    for name, relative, old, new, replace_all in CASES:
        with tempfile.TemporaryDirectory(prefix="stage5g-r2cb-negative-") as raw:
            mutant = Path(raw) / "repo"
            copy_root(mutant)
            path = mutant / relative
            source = path.read_text()
            if old not in source:
                raise RuntimeError(f"mutation anchor missing: {name}")
            path.write_text(source.replace(old, new) if replace_all else source.replace(old, new, 1))
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
    print(f"stage5g-c-r2cb-negative-harness: PASS {passed}/{len(CASES)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
