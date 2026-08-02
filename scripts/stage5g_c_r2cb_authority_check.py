#!/usr/bin/env python3
"""Fail-closed authority check for Stage 5G-c R2-c-b parity."""

from __future__ import annotations

import argparse
import hashlib
import json
import subprocess
import sys
from pathlib import Path

BASE = "bede0868424086fbc2655fbfe5a0f5f1f5fefd54"
ORDER_POSITION = Path("crates/strategy-runtime-core/src/stage5g_order_position.rs")
STAGE5C = Path("crates/strategy-runtime-core/src/stage5c_paper_host.rs")
STAGE5G_B = Path("crates/strategy-runtime-core/src/stage5g_mock_ack.rs")
FINAM_MAPPER = Path("crates/broker-finam/src/mapper.rs")
FIXTURE = Path("fixtures/finam/stage5g_r2cb_full_snapshot_sequence.json")
DESCRIPTOR = Path("docs/stage-5/stage5g-c-r2cb-broker-truth-finam-parity.json")
R3_SNAPSHOT = Path("scripts/stage5g_c_r2ca_r3_snapshot_gate.py")

IMMUTABLE = {
    STAGE5C: "ca357ea9e2dd39910d119e1033e00eef7698cf459255a95825591cd1c86984e7",
    STAGE5G_B: "a3aa1a64ebc763750b52530925c03b4573a30627c05211491a0ae51f64da7b67",
    R3_SNAPSHOT: "2f73a9882e3efa9a091079a59741bb068a8a1d5820e81d31ae30246702975315",
}


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def require(source: str, tokens: tuple[str, ...], label: str) -> None:
    for token in tokens:
        if token not in source:
            raise ValueError(f"{label} token missing: {token}")


def check(root: Path) -> None:
    for relative, expected in IMMUTABLE.items():
        path = root / relative
        if not path.is_file() or sha256(path) != expected:
            raise ValueError(f"accepted authority drift: {relative}")

    if (root / ".git").exists():
        resolved = subprocess.check_output(
            ["git", "rev-parse", f"{BASE}^{{commit}}"], cwd=root, text=True
        ).strip()
        if resolved != BASE:
            raise ValueError("accepted predecessor does not resolve exactly")
    r3 = subprocess.run(
        [sys.executable, str(root / R3_SNAPSHOT), "--root", str(root)],
        cwd=root,
        text=True,
        capture_output=True,
        check=False,
    )
    if r3.returncode != 0:
        raise ValueError(f"accepted R3 snapshot rejected tree: {r3.stderr.strip()}")

    descriptor = json.loads((root / DESCRIPTOR).read_text())
    if descriptor.get("accepted_predecessor") != BASE:
        raise ValueError("descriptor predecessor drift")
    if descriptor.get("accepted_r3_entry_points") != [
        "validate_stage5c_market_terminal_outcome_r3",
        "settle_stage5c_validated_market_terminal_outcome_r3",
    ]:
        raise ValueError("descriptor R3 entry points drift")
    contract = descriptor.get("broker_truth_contract", {})
    required_true = (
        "immutable_trade_excludes_received_ts",
        "same_snapshot_trade_id_dedup_or_conflict",
        "repeated_full_snapshot_trade_idempotent",
        "market_order_classified_before_position",
        "filled_qty_position_delta_exact",
        "fingerprinted_collections_canonical",
    )
    if any(contract.get(key) is not True for key in required_true):
        raise ValueError("descriptor parity invariant drift")
    if contract.get("package_receipt_clock") != "BrokerTruthSnapshot.received_ts.timestamp_millis()":
        raise ValueError("descriptor receipt-clock drift")
    closed = descriptor.get("closed_surfaces")
    if not isinstance(closed, dict) or not closed or any(value is not False for value in closed.values()):
        raise ValueError("closed surface opened")

    source = (root / ORDER_POSITION).read_text()
    require(
        source,
        (
            "validate_stage5c_market_terminal_outcome_r3",
            "settle_stage5c_validated_market_terminal_outcome_r3",
            "canonicalize_broker_truth_snapshot",
            "BTreeMap<String, BrokerTradeSnapshot>",
            "canonical_json_sort(&mut truth.orders);",
            "canonical_json_sort(&mut truth.positions);",
            "canonical_json_sort(&mut truth.instruments);",
            "immutable_trade_payload_matches",
            "left.source_ts == right.source_ts",
            "last_broker_truth_received_ms",
            "received_ts.timestamp_millis()",
            "CanonicalPositionDerivation::AbsentFlat",
            "CanonicalPositionDerivation::Aggregate",
            "match order.status {",
            "fn validate_order_position_coherence",
            "instrument_identity_matches",
            "moex.stage5g.order-position-evidence.v2",
            "moex.stage5g.order-position-lifecycle.v3",
            "STAGE5G-C-R2CB-PARITY-TESTS-BEGIN",
        ),
        "order-position",
    )
    if "validate_stage5c_market_terminal_outcome_r1" in source or "validate_stage5c_market_terminal_outcome_r2" in source:
        raise ValueError("R1/R2 terminal authority bypass reachable")
    immutable_body = source.split("fn immutable_trade_payload_matches", 1)[1].split("pub fn apply_stage5g_order_position_evidence", 1)[0]
    if "received_ts" in immutable_body:
        raise ValueError("observation receipt leaked into immutable trade identity")
    classify = source.split("// A concrete broker order is authoritative", 1)[1].split(
        "Stage5gMockPlaceKind::Limit", 1
    )[0]
    if classify.find("match order.status") > classify.find("canonical_target_position"):
        raise ValueError("MARKET position evaluated before status classification")

    mapper = (root / FINAM_MAPPER).read_text()
    require(
        mapper,
        (
            "STAGE5G-C-R2CB-FINAM-FIXTURE-BEGIN",
            "stage5g_r2cb_finam_full_snapshot_fixture_preserves_repeated_trade_identity",
            "stage5g_r2cb_full_snapshot_sequence.json",
        ),
        "FINAM mapper",
    )
    fixture = json.loads((root / FIXTURE).read_text())
    if fixture.get("fixture_kind") != "synthetic_finam_stage5g_r2cb_full_snapshot_sequence_v1":
        raise ValueError("FINAM fixture kind drift")
    old = fixture["partial"]["trades"]["trades"][0]
    repeated = fixture["filled"]["trades"]["trades"][0]
    if old != repeated or fixture["partial_received_ts"] == fixture["filled_received_ts"]:
        raise ValueError("FINAM repeated-trade receipt witness drift")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    args = parser.parse_args()
    try:
        check(args.root.resolve())
    except (ValueError, OSError, KeyError, json.JSONDecodeError, subprocess.CalledProcessError) as error:
        print(f"stage5g-c-r2cb-authority-check: FAIL: {error}", file=sys.stderr)
        return 1
    print("stage5g-c-r2cb-authority-check: PASS")
    print(f"accepted_predecessor: {BASE}")
    print("exact_r3/finam_fixture/parity/receipt_ms/closed_surfaces: PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
