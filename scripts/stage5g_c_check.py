#!/usr/bin/env python3
"""Fail-closed source/contract checker for Stage 5G-c."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
from pathlib import Path

MODULE = "crates/strategy-runtime-core/src/stage5g_order_position.rs"
ACK_MODULE = "crates/strategy-runtime-core/src/stage5g_mock_ack.rs"
CONTRACT = "docs/stage-5/stage5g-c-contract.json"
DESIGN = "docs/stage-5/5g-c-order-trade-position-convergence.md"
DESCRIPTOR = "docs/stage-5/stage5g-b-r3-acceptance-descriptor.json"
STATUS = "docs/current-status.md"
PREDECESSOR = "92f57c7831d8a15fb2e37668d3b07f1ccea03af7"
SCENARIO_CASE_IDS = [
    "GOP01_WORKING_ORDER_REMAINS_ACTIVE",
    "GOP02_PARTIAL_FILL_ADVANCES_MONOTONICALLY",
    "GOP03_PARTIAL_FILL_REGRESSION_BLOCKS",
    "GOP04_FILLED_REQUIRES_TARGET_POSITION_CONFIRMATION",
    "GOP05_CANCELED_TERMINATES_WITHOUT_POSITION_CHANGE",
    "GOP06_REJECTED_TERMINATES_WITHOUT_POSITION_CHANGE",
    "GOP07_EXPIRED_TERMINATES_WITHOUT_POSITION_CHANGE",
    "GOP08_UNKNOWN_ORDER_STATUS_BLOCKS",
    "GOP09_IDENTICAL_EVENT_REPLAY_IS_IDEMPOTENT",
    "GOP10_CONFLICTING_DUPLICATE_EVENT_BLOCKS",
    "GOP11_NON_TARGET_EVENT_CANNOT_SETTLE_TARGET",
    "GOP12_ACCOUNT_WIDE_ACTIVE_ORDER_IS_SAFETY_GUARD",
    "GOP13_TARGET_POSITION_SIDE_MISMATCH_BLOCKS",
    "GOP14_TARGET_POSITION_OVERFILL_BLOCKS",
    "GOP15_CORRELATED_TRADE_SUPPORTS_FILL_TRUTH",
    "GOP16_TRADE_IDENTITY_OR_QUANTITY_MISMATCH_BLOCKS",
]

AUTHORITIES = {
    "crates/strategy-runtime-core/src/stage5c_paper_host.rs":
        "93c0b48e1b564ef1763354579885bea3cd5b448133afccbc611584184bb13f2d",
    "crates/broker-core/src/operational_snapshot.rs":
        "53e78a922b1c1a7948485f3016acdbcd64c3766618274a3b039233fc67d541ca",
    "docs/stage-5/stage-5c-api-freeze-manifest.json":
        "f8c555d11de1271f5041b4d3abf880ac7a406d6fb23f5e4d38ca25468a974323",
}


def fail(message: str) -> None:
    raise ValueError(message)


def read(root: Path, relative: str) -> str:
    path = root / relative
    if not path.is_file():
        fail(f"required Stage 5G-c file missing: {relative}")
    return path.read_text(encoding="utf-8")


def require(text: str, tokens: list[str], label: str) -> None:
    for token in tokens:
        if token not in text:
            fail(f"{label} token missing: {token}")


def check(root: Path) -> None:
    contract = json.loads(read(root, CONTRACT))
    if contract.get("schema_version") != 1:
        fail("Stage 5G-c contract schema drift")
    if contract.get("accepted_predecessor") != PREDECESSOR:
        fail("accepted predecessor drift")
    if contract.get("status") != "implementation_review_candidate":
        fail("implementation status drift")
    if contract.get("scenario_case_ids") != SCENARIO_CASE_IDS:
        fail("Stage 5G-c must retain the exact ordered 16-case GOP matrix")
    semantics = contract.get("semantics", {})
    required_true = {
        "canonical_snapshots_only",
        "active_and_partial_accumulate_without_callback",
        "terminal_complete_vector_calls_stage5c_j_once",
        "target_instrument_truth_is_lifecycle_truth",
        "account_wide_orders_are_safety_guard",
        "trade_identity_and_quantity_are_exact",
        "duplicate_exact_is_idempotent",
        "conflicting_duplicate_fails_closed",
        "broker_order_id_is_exact_string",
    }
    if any(semantics.get(key) is not True for key in required_true):
        fail("Stage 5G-c semantic contract weakened")
    if any(contract.get("closed_surfaces", {}).values()):
        fail("Stage 5G-c closed surface opened")
    if contract.get("next_transition") != {
        "independent_review_required": True,
        "stage5g_d_open": False,
        "main_merge_authorized": False,
        "deployment_authorized": False,
    }:
        fail("Stage 5G-c transition authority drift")

    descriptor = json.loads(read(root, DESCRIPTOR))
    if descriptor.get("status") != "accepted":
        fail("Stage 5G-b acceptance descriptor is not accepted")
    if descriptor.get("source", {}).get("full_commit_sha") != PREDECESSOR:
        fail("Stage 5G-b acceptance source drift")
    if descriptor.get("independent_review", {}).get("authorized_successor") != (
        "5G-c-order-trade-position-convergence"
    ):
        fail("Stage 5G-c is not authorized by accepted review")

    for relative, expected in AUTHORITIES.items():
        actual = hashlib.sha256((root / relative).read_bytes()).hexdigest()
        if actual != expected:
            fail(f"frozen authority drift: {relative}")

    source = read(root, MODULE)
    production, marker, tests = source.rpartition("#[cfg(test)]\nmod tests")
    if not marker:
        fail("Stage 5G-c focused tests missing")
    require(production, [
        "BrokerOrderSnapshot",
        "BrokerTradeSnapshot",
        "BrokerPositionSnapshot",
        "BrokerTruthSnapshot",
        "pub struct Stage5gOrderPositionSession",
        "pub struct Stage5gConvergedPaperStrategy",
        "resolve_stage5c_paper_broker_lifecycle(",
        "Stage5gOrderPositionError::AccountWideActiveOrderSafetyGuard",
        "Stage5gOrderPositionError::FilledQuantityRegression",
        "Stage5gOrderPositionError::TradeQuantityMismatch",
        "Stage5gOrderPositionError::PositionOverfill",
        "moex.stage5g.order-position-lifecycle.v1\\0",
    ], "Stage 5G-c production")
    if production.count("resolve_stage5c_paper_broker_lifecycle(") != 1:
        fail("Stage 5C-j must have exactly one Stage 5G-c callsite")
    for opaque in ("Stage5gOrderPositionSession", "Stage5gConvergedPaperStrategy"):
        if re.search(rf"#\[derive[^\]]*\]\s*pub struct {opaque}\b", production):
            fail(f"linear capability became derivable: {opaque}")
    for forbidden in (
        "redis::", "reqwest::", ".post(", ".delete(", "tokio::spawn",
        "std::thread", "thread::sleep", "Utc::now(",
    ):
        if forbidden in production:
            fail(f"forbidden I/O/autonomous token in Stage 5G-c: {forbidden}")
    for number in range(1, 17):
        if f"fn gop{number:02d}_" not in tests:
            fail(f"focused GOP{number:02d} test missing")

    ack_source = read(root, ACK_MODULE)
    require(ack_source, [
        "pub(crate) struct Stage5gResolvedMockAckContext",
        "pub(crate) fn into_stage5g_c_parts",
        "pub(crate) fn from_stage5g_c_parts",
        "fn stage5gc_public_terminal_ack_converges_without_broker_callback()",
        "Stage5gMockAckAdmissionError::BindingRequestIdentityMismatch",
    ], "Stage 5G-c ACK bridge")
    require(read(root, DESIGN), [
        PREDECESSOR, "canonical `BrokerTruthSnapshot`", "GOP01", "GOP16",
        "Stage 5C-j", "Stage 5G-f",
    ], "Stage 5G-c design")
    require(read(root, STATUS), [
        "Stage 5G-b R3 was independently accepted", PREDECESSOR,
        "Stage 5G-c is an implementation review candidate", "Stage 5G-d remains",
    ], "current status")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    args = parser.parse_args()
    try:
        check(args.root.resolve())
    except (ValueError, json.JSONDecodeError) as error:
        print(f"stage5g-c-check: FAIL: {error}", file=sys.stderr)
        return 1
    print("stage5g-c-check: PASS")
    print("scenario_matrix: GOP01-GOP16")
    print("canonical_truth: broker-core")
    print("callback_authority: Stage5C-j exactly once")
    print("closed_surfaces: preserved")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
