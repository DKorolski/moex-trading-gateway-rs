#!/usr/bin/env python3
"""Fail-closed checker for the Stage 5G-c R1 remediation boundary."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
from pathlib import Path

MODULE = "crates/strategy-runtime-core/src/stage5g_order_position.rs"
ACK_MODULE = "crates/strategy-runtime-core/src/stage5g_mock_ack.rs"
STAGE5C = "crates/strategy-runtime-core/src/stage5c_paper_host.rs"
CONTRACT = "docs/stage-5/stage5g-c-contract.json"
DESIGN = "docs/stage-5/5g-c-order-trade-position-convergence.md"
STATUS = "docs/current-status.md"
BASE = "dba5362444ec279391eed92ff28ebb4ceb729c09"

PINNED = {
    STAGE5C: "2291d3bd77cfb99754f8374d0339fb8419103a2823b3486675ce1781f0f17000",
    ACK_MODULE: "75565bfd3fb86dad723c18f413e5c1253a60839a21ed4e723f598b3e994b4ccf",
    MODULE: "7c7758d75a788cdcf465e8043d9300f3c95a606b35ffbb5d28c47052ab442108",
    "crates/broker-core/src/operational_snapshot.rs":
        "53e78a922b1c1a7948485f3016acdbcd64c3766618274a3b039233fc67d541ca",
    "docs/stage-5/stage-5c-api-freeze-manifest.json":
        "f8c555d11de1271f5041b4d3abf880ac7a406d6fb23f5e4d38ca25468a974323",
}

WITNESSES = [
    "stage5gc_r1_public_market_entry_exact_position_converges",
    "stage5gc_r1_public_market_entry_partial_then_exact_converges",
    "stage5gc_r1_public_stage5f_f04_market_exit_flat_converges",
    "stage5gc_r1_public_rejected_exit_preserves_existing_position",
    "stage5gc_r1_public_stage5c_preflight_block_restores_retryable_session",
]


def fail(message: str) -> None:
    raise ValueError(message)


def read(root: Path, relative: str) -> str:
    path = root / relative
    if not path.is_file():
        fail(f"required Stage 5G-c R1 file missing: {relative}")
    return path.read_text(encoding="utf-8")


def require(text: str, tokens: list[str], label: str) -> None:
    for token in tokens:
        if token not in text:
            fail(f"{label} token missing: {token}")


def check(root: Path) -> None:
    contract = json.loads(read(root, CONTRACT))
    if contract.get("schema_version") != 2:
        fail("R1 contract schema drift")
    if contract.get("accepted_predecessor") != BASE:
        fail("R1 predecessor drift")
    if contract.get("status") != "remediation_review_candidate":
        fail("R1 status drift")
    if contract.get("production_witnesses") != WITNESSES:
        fail("R1 production witness vector drift")
    if any(contract.get("closed_surfaces", {}).values()):
        fail("closed surface opened")
    semantics = contract.get("semantics", {})
    required = {
        "market_entry_requires_exact_source_target",
        "market_exit_requires_flat",
        "partial_market_position_remains_awaiting",
        "terminal_candidate_commits_after_stage5c_preflight",
        "pre_callback_block_restores_pre_candidate_state",
        "remaining_stage5c_expectations_fail_closed",
        "terminal_partial_fill_requires_exact_trade_and_position",
        "every_populated_trade_identity_must_match",
        "trade_quantity_strictly_positive",
        "broker_event_watermarks_fail_closed",
        "lifecycle_fingerprint_v2_binds_complete_continuation",
    }
    if any(semantics.get(key) is not True for key in required):
        fail("R1 semantics weakened")
    if semantics.get("stage5c_j_callsite_count") != 1:
        fail("Stage 5C-j callsite contract drift")

    for relative, expected in PINNED.items():
        actual = hashlib.sha256((root / relative).read_bytes()).hexdigest()
        if actual != expected:
            fail(f"pinned R1 source drift: {relative}")

    source = read(root, MODULE)
    production, marker, tests = source.rpartition("#[cfg(test)]\nmod tests")
    if not marker:
        fail("focused R1 tests missing")
    require(production, [
        "Stage5gSourceIntentProjection",
        "source_projection_matches_ack",
        "Stage5gOrderPositionError::PositionIncomplete",
        "Stage5gOrderPositionError::RejectedOrderHasFill",
        "Stage5gOrderPositionError::TradeIdentityMismatch",
        "Stage5gOrderPositionError::NonPositiveTradeQuantity",
        "Stage5gOrderPositionError::BrokerTruthTimeRegression",
        "Stage5gOrderPositionError::ComponentTimeAfterSnapshot",
        "let pre_candidate_state = session.state.clone();",
        "state: pre_candidate_state",
        "remaining_lifecycle_expectations().is_empty()",
        "moex.stage5g.order-position-lifecycle.v2\\0",
        '"orders": orders',
        '"trades": trades',
        '"last_order_source_ts"',
        '"last_position_source_ts"',
        "last_broker_truth_received_ts",
    ], "R1 production")
    if production.count("resolve_stage5c_paper_broker_lifecycle(") != 1:
        fail("Stage 5C-j must have exactly one R1 callsite")
    for opaque in ("Stage5gOrderPositionSession", "Stage5gConvergedPaperStrategy"):
        if re.search(rf"#\[derive[^\]]*\]\s*pub struct {opaque}\b", production):
            fail(f"linear capability became derivable: {opaque}")
    for forbidden in (
        "redis::", "reqwest::", ".post(", ".delete(", "tokio::spawn",
        "std::thread", "thread::sleep", "Utc::now(",
    ):
        if forbidden in production:
            fail(f"forbidden I/O/autonomous token in R1: {forbidden}")
    for number in range(1, 17):
        if f"fn gop{number:02d}_" not in tests:
            fail(f"inherited GOP{number:02d} test missing")
    require(tests, [
        "r1_fingerprint_v2_separates_partial_state_and_continuation",
        "r1_partial_cancel_requires_exact_position_and_rejected_fill_blocks",
        "r1_trade_every_present_identity_must_match_and_qty_is_positive",
        "r1_broker_truth_and_component_time_regression_block",
    ], "R1 tests")

    ack_source = read(root, ACK_MODULE)
    require(
        ack_source,
        [f"fn {name}()" for name in WITNESSES],
        "R1 public production integration",
    )

    stage5c = read(root, STAGE5C)
    require(stage5c, [
        "pub(crate) struct Stage5gSourceIntentProjection",
        "pre_position_qty",
        "expected_attribution",
        "stage5g_source_intent_projections",
    ], "Stage 5C source-owned projection")
    require(read(root, DESIGN), [BASE, "Fingerprint schema v2", "F02", "F04"], "R1 design")
    require(read(root, STATUS), [BASE, "Stage 5G-c R1", "Stage 5G-d"], "status")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    args = parser.parse_args()
    try:
        check(args.root.resolve())
    except (ValueError, json.JSONDecodeError) as error:
        print(f"stage5g-c-r1-check: FAIL: {error}", file=sys.stderr)
        return 1
    print("stage5g-c-r1-check: PASS")
    print("production_witnesses: 5/5")
    print("stage5c_j_callsite: 1")
    print("closed_surfaces: preserved")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
