#!/usr/bin/env python3
"""Adversarial mutation matrix for the Stage 5G-c checker."""

from __future__ import annotations

import shutil
import subprocess
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
MODULE = "crates/strategy-runtime-core/src/stage5g_order_position.rs"
CHECKER = "scripts/stage5g_c_check.py"
PATHS = [
    MODULE,
    "crates/strategy-runtime-core/src/stage5g_mock_ack.rs",
    "crates/strategy-runtime-core/src/stage5c_paper_host.rs",
    "crates/broker-core/src/operational_snapshot.rs",
    "docs/stage-5/stage-5c-api-freeze-manifest.json",
    "docs/stage-5/stage5g-c-contract.json",
    "docs/stage-5/stage5g-b-r3-acceptance-descriptor.json",
    "docs/stage-5/5g-c-order-trade-position-convergence.md",
    "docs/current-status.md",
    CHECKER,
]


def replace(path: Path, before: str, after: str) -> None:
    text = path.read_text(encoding="utf-8")
    if before not in text:
        raise RuntimeError(f"mutation anchor missing: {before}")
    path.write_text(text.replace(before, after, 1), encoding="utf-8")


CASES = [
    ("open-redis-surface", '"redis_live_consumer": false', '"redis_live_consumer": true'),
    ("drop-gop16", '"GOP16_TRADE_IDENTITY_OR_QUANTITY_MISMATCH_BLOCKS"', '"REMOVED_GOP16"'),
    ("weaken-terminal-callback", '"terminal_complete_vector_calls_stage5c_j_once": true', '"terminal_complete_vector_calls_stage5c_j_once": false'),
    ("remove-stage5c-call", "resolve_stage5c_paper_broker_lifecycle(", "removed_stage5c_paper_broker_lifecycle("),
    ("remove-partial-regression-test", "fn gop03_partial_fill_regression_blocks()", "fn removed_gop03_partial_fill_regression_blocks()"),
    ("forge-linear-clone", "pub struct Stage5gOrderPositionSession {", "#[derive(Clone)]\npub struct Stage5gOrderPositionSession {"),
    ("introduce-redis-client", "use broker_core::{", "use redis::Client;\nuse broker_core::{"),
    (
        "drift-stage5c-authority",
        "RuntimeHostBootstrapSnapshot",
        "MutatedRuntimeHostBootstrapSnapshot",
    ),
    ("revoke-accepted-predecessor", '"status": "accepted"', '"status": "rejected"'),
    ("open-stage5g-d", '"stage5g_d_open": false', '"stage5g_d_open": true'),
]


def main() -> int:
    passed = 0
    for name, before, after in CASES:
        with tempfile.TemporaryDirectory(prefix="stage5g-c-negative-") as raw:
            root = Path(raw)
            for relative in PATHS:
                target = root / relative
                target.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(ROOT / relative, target)
            target = root / MODULE
            if name == "drift-stage5c-authority":
                target = root / "crates/strategy-runtime-core/src/stage5c_paper_host.rs"
            elif name == "revoke-accepted-predecessor":
                target = root / "docs/stage-5/stage5g-b-r3-acceptance-descriptor.json"
            elif name in {"open-redis-surface", "drop-gop16", "weaken-terminal-callback", "open-stage5g-d"}:
                target = root / "docs/stage-5/stage5g-c-contract.json"
            replace(target, before, after)
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
    print(f"stage5g-c-negative-harness: PASS {passed}/{len(CASES)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
