#!/usr/bin/env python3
"""Exact mutation checks for the Stage 8A-4 I4 design contract."""

from __future__ import annotations

import os
import shutil
import subprocess
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CHECKER = "scripts/stage8a4_durable_composition_i4_design_check.py"

MUTATIONS = [
    ("predecessor", "593ff255ef7826a22e66c9aff6f7ea47acf47644", "0" * 40),
    ("review-sha", "1da167c3e7f1266473133d2d8a1412906a26d7f83b5dc026ce84dc7969090257", "0" * 64),
    ("receipt-authority", '"receipt_alone_is_authority": false', '"receipt_alone_is_authority": true'),
    ("complete-v2", '"complete_v2_exact_suffix_required": true', '"complete_v2_exact_suffix_required": false'),
    ("covering-s1", '"covering_s1_required": true', '"covering_s1_required": false'),
    ("request-finalized", '"request_finalized_required": true', '"request_finalized_required": false'),
    ("hold-authorized", '"pending_or_hold_authorized": false', '"pending_or_hold_authorized": true'),
    ("cancel-working", "ExactWorking | none | none | unresolved", "ExactWorking | none | Recovered | accepted"),
    ("place-rejected", "ExactTerminalRejected | Rejected | Rejected | BrokerRejected", "ExactTerminalRejected | Rejected | Recovered | RecoveredByBrokerTruth"),
    ("cancel-filled", "ExactTerminalFilled | ExecutionObserved", "ExactTerminalFilled | Canceled"),
    ("cancel-expired", "ExactTerminalExpired | AlreadyTerminalNonExecution", "ExactTerminalExpired | Canceled"),
    ("cancel-cancelled", "ExactTerminalCancelled | Canceled", "ExactTerminalCancelled | ExecutionObserved"),
    ("readiness-independent", '"independent_from_terminal_ack": true', '"independent_from_terminal_ack": false'),
    ("stop-ready", '"fresh_run_allowed_required": true', '"fresh_run_allowed_required": false'),
    ("stale-ready", '"stop_stale_unreadable_unknown_or_orphan_block": true', '"stop_stale_unreadable_unknown_or_orphan_block": false'),
    ("broker-freshness", '"fresh_composite_and_broker_truth_required": true', '"fresh_composite_and_broker_truth_required": false'),
    ("unknown-orphan", "unknown or orphan account safety blocks readiness", "unknown or orphan account safety permits readiness"),
    ("post-effect-reuse", '"i3_post_effect_snapshot_reusable": false', '"i3_post_effect_snapshot_reusable": true'),
    ("duplicate-append", "duplicate derivation appends no journal record", "duplicate derivation may append journal record"),
    ("stable-identity", "stable terminal ACK identity excludes unrelated later seal generations", "stable terminal ACK identity includes unrelated later seal generations"),
    ("opaque-types", "facade and authorities are nonserializable opaque types", "facade and authorities are serializable public types"),
    ("caller-seal", "no caller-built seal checkpoint or digest-only constructor", "caller-built seal checkpoint is allowed"),
    ("redis-open", '"redis_ack_xack": true', '"redis_ack_xack": false'),
    ("live-open", '"runtime_live": true', '"runtime_live": false'),
]


def mutate(tree: Path, old: str, new: str) -> None:
    matches = []
    for path in list((tree / "docs/stage-8").glob("*I4*")) + [
        tree / "docs/stage-8/stage8a4-durable-composition-i4-design-authority.json"
    ]:
        text = path.read_text(encoding="utf-8")
        if old in text:
            path.write_text(text.replace(old, new, 1), encoding="utf-8")
            matches.append(path)
            break
    if not matches:
        raise RuntimeError(f"mutation source missing: {old}")


def main() -> None:
    with tempfile.TemporaryDirectory(prefix="stage8a4-i4-design-negative-") as raw:
        base = Path(raw) / "tree"
        shutil.copytree(ROOT / "docs", base / "docs")
        shutil.copytree(ROOT / "scripts", base / "scripts")
        for name, old, new in MUTATIONS:
            case = Path(raw) / name
            shutil.copytree(base, case)
            mutate(case, old, new)
            environment = os.environ.copy()
            environment["STAGE8A4_I4_ROOT"] = str(case)
            result = subprocess.run(
                ["python3", str(ROOT / CHECKER)],
                cwd=ROOT,
                env=environment,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                check=False,
            )
            if result.returncode == 0:
                raise SystemExit(f"FAIL {name}")
            print(f"PASS {name}")
    print(f"stage8a4-durable-composition-i4-design-negative: PASS {len(MUTATIONS)}/{len(MUTATIONS)}")


if __name__ == "__main__":
    main()
