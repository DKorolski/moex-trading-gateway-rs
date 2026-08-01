#!/usr/bin/env python3
"""Adversarial semantic mutation matrix for Stage 5G-c R1."""

from __future__ import annotations

import shutil
import subprocess
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CHECKER = "scripts/stage5g_c_check.py"
ORDER = "crates/strategy-runtime-core/src/stage5g_order_position.rs"
ACK = "crates/strategy-runtime-core/src/stage5g_mock_ack.rs"
STAGE5C = "crates/strategy-runtime-core/src/stage5c_paper_host.rs"
CONTRACT = "docs/stage-5/stage5g-c-contract.json"
PATHS = [
    ORDER,
    ACK,
    STAGE5C,
    "crates/broker-core/src/operational_snapshot.rs",
    "docs/stage-5/stage-5c-api-freeze-manifest.json",
    CONTRACT,
    "docs/stage-5/5g-c-order-trade-position-convergence.md",
    "docs/current-status.md",
    CHECKER,
]


def replace(path: Path, before: str, after: str) -> None:
    text = path.read_text(encoding="utf-8")
    if before not in text:
        raise RuntimeError(f"mutation anchor missing in {path}: {before}")
    path.write_text(text.replace(before, after, 1), encoding="utf-8")


CASES = [
    ("market-exit-flat-rule-removed", ORDER, "qty.abs() <= f64::EPSILON", "qty < -1.0"),
    ("intent-class-rebound-to-entry", ORDER, "source.intent_class", "crate::BrokerNeutralHybridIntentClass::Entry"),
    ("partial-market-marked-terminal", ORDER, "Ok(false)", "Ok(true)"),
    ("remaining-expectations-ignored", ORDER, "if !resolved.remaining_lifecycle_expectations().is_empty()", "if false"),
    ("pre-candidate-state-not-restored", ORDER, "state: pre_candidate_state", "state: Stage5gOrderPositionState::default()"),
    ("fingerprint-domain-downgraded", ORDER, "moex.stage5g.order-position-lifecycle.v2", "moex.stage5g.order-position-lifecycle.v1"),
    ("order-projection-removed", ORDER, '"orders": orders', '"orders": []'),
    ("trade-projection-removed", ORDER, '"trades": trades', '"trades": []'),
    ("partial-cancel-position-check-removed", ORDER, "Stage5gOrderPositionError::PositionIncomplete", "Stage5gOrderPositionError::UnknownOrderStatus"),
    ("contradictory-trade-id-accepted", ORDER, "Stage5gOrderPositionError::TradeIdentityMismatch", "Stage5gOrderPositionError::TradeQuantityMismatch"),
    ("non-positive-trade-accepted", ORDER, "Stage5gOrderPositionError::NonPositiveTradeQuantity", "Stage5gOrderPositionError::TradeQuantityMismatch"),
    ("broker-truth-watermark-removed", ORDER, "Stage5gOrderPositionError::BrokerTruthTimeRegression", "Stage5gOrderPositionError::NonMonotonicSequence"),
    ("reversed-component-time-accepted", ORDER, "Stage5gOrderPositionError::ComponentTimeAfterSnapshot", "Stage5gOrderPositionError::NonMonotonicSequence"),
    ("public-integration-witness-removed", ACK, "fn stage5gc_r1_public_market_entry_exact_position_converges()", "fn removed_stage5gc_r1_public_market_entry_exact_position_converges()"),
    ("second-stage5c-callsite-added", ORDER, "fn converge_through_stage5c(", "const EXTRA_CALLSITE: &str = \"resolve_stage5c_paper_broker_lifecycle(\";\nfn converge_through_stage5c("),
    ("stage5g-d-opened", CONTRACT, '"stage5g_d": false', '"stage5g_d": true'),
]


def main() -> int:
    passed = 0
    for name, relative, before, after in CASES:
        with tempfile.TemporaryDirectory(prefix="stage5g-c-r1-negative-") as raw:
            root = Path(raw)
            for required in PATHS:
                target = root / required
                target.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(ROOT / required, target)
            replace(root / relative, before, after)
            result = subprocess.run(
                ["python3", str(root / CHECKER), "--root", str(root)],
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                check=False,
            )
            if result.returncode == 0:
                print(f"FAIL {name}: checker accepted mutation")
                return 1
            passed += 1
            print(f"PASS {name}")
    print(f"stage5g-c-r1-negative-harness: PASS {passed}/{len(CASES)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
