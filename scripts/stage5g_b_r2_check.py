#!/usr/bin/env python3
"""Fail-closed checker for Stage 5G-b R2."""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

MODULE = "crates/strategy-runtime-core/src/stage5g_mock_ack.rs"
CONTRACT = "docs/stage-5/stage5g-b-r2-contract.json"
DESIGN = "docs/stage-5/5g-b-r2-transition-history-coherence.md"
STATUS = "docs/current-status.md"
R1_GATE = "scripts/stage5g_b_r1_snapshot_gate.sh"
PREDECESSOR = "00d158978904c177828ff2a330b1f3c1bfb4bb10"
STAGE5G_A = "011fd4b7baaa41fffdad7d3c28e463b7977f5989"


def fail(message: str) -> None:
    raise ValueError(message)


def read(root: Path, relative: str) -> str:
    path = root / relative
    if not path.is_file():
        fail(f"required R2 file missing: {relative}")
    return path.read_text(encoding="utf-8")


def require(text: str, tokens: list[str], label: str) -> None:
    for token in tokens:
        if token not in text:
            fail(f"{label} token missing: {token}")


def check(root: Path) -> None:
    contract = json.loads(read(root, CONTRACT))
    if contract.get("schema_version") != 3:
        fail("R2 contract schema drift")
    if contract.get("immutable_predecessor") != PREDECESSOR:
        fail("R2 predecessor drift")
    if contract.get("accepted_stage5g_a_ref") != STAGE5G_A:
        fail("Stage 5G-a authority drift")
    if contract.get("status") != "implementation_review_candidate":
        fail("R2 status drift")
    if any(contract.get("closed_surfaces", {}).values()):
        fail("R2 closed surface opened")
    transition = contract.get("next_transition", {})
    if transition != {
        "independent_review_required": True,
        "stage5g_c_open": False,
        "main_merge_authorized": False,
        "deployment_authorized": False,
    }:
        fail("R2 transition authority drift")

    source = read(root, MODULE)
    production, marker, tests = source.partition("#[cfg(test)]")
    if not marker:
        fail("R2 test module missing")
    require(production, [
        "pub const STAGE5G_MOCK_ACK_SCHEMA_VERSION: u16 = 3;",
        "last_ack_received_ts_utc: Option<chrono::DateTime<chrono::Utc>>",
        "pub last_ack_received_ts_utc: Option<String>",
        "moex.stage5g.mock-ack-lifecycle.v3\\0",
        "NoSendProofContradictsPriorLifecycleEvidence",
        "MissingBrokerOrderIdAfterObservedIdentity",
        "NonMonotonicAckTime",
    ], "R2 source")

    no_send_start = production.find("fn stage5g_no_send_proof_contradiction(")
    no_send_end = production.find("fn stage5g_terminal_ack_loses_observed_broker_identity(")
    if no_send_start < 0 or no_send_end <= no_send_start:
        fail("no-send provenance predicate missing")
    require(production[no_send_start:no_send_end], [
        "slot.state == Stage5gMockAckSlotState::Waiting && slot.latest_ack.is_none()",
        "slot.state == Stage5gMockAckSlotState::NoSendProofRequired",
        "prior.status == CommandAckStatus::Expired",
        "prior.reason.is_none()",
        "prior.broker_order_id.is_none()",
        "NoSendProofContradictsPriorLifecycleEvidence",
    ], "no-send provenance")

    continuity_start = no_send_end
    continuity_end = production.find("fn stage5g_ack_reason_is_coherent(")
    if continuity_end <= continuity_start:
        fail("broker identity continuity predicate missing")
    require(production[continuity_start:continuity_end], [
        "slot.observed_broker_order_id.is_some()",
        "ack.broker_order_id.is_none()",
        "CommandAckStatus::Accepted | CommandAckStatus::Recovered | CommandAckStatus::Rejected",
    ], "broker identity continuity")
    require(production, [
        "if stage5g_terminal_ack_loses_observed_broker_identity(&state.slots[slot_index], &event.ack) {",
        "if observed != incoming",
        "Stage5gMockAckError::BrokerOrderIdConflict",
        "event.ack.received_ts < *last",
        "Stage5gMockAckError::NonMonotonicAckTime",
        "state.last_ack_received_ts_utc = Some(event.ack.received_ts)",
        ".map(stage5g_ack_timestamp)",
    ], "R2 lifecycle enforcement")
    for bypass in [
        "if false && stage5g_terminal_ack_loses_observed_broker_identity",
        "if false && observed != incoming",
        "false && event.ack.received_ts < *last",
    ]:
        if bypass in production:
            fail(f"R2 lifecycle enforcement bypass: {bypass}")
    if production.count(".map(stage5g_ack_timestamp)") != 2:
        fail("ACK time watermark is not bound in both public/internal summaries")

    resolver_calls = re.findall(
        r"(?<![A-Za-z0-9_])resolve_stage5c_paper_intent_lifecycle\(", production
    )
    if len(resolver_calls) != 1:
        fail("Stage 5C resolver must have exactly one production callsite")
    require(tests, [
        "fn no_send_proof_requires_clean_waiting_or_unproved_expiry_provenance()",
        "fn observed_broker_identity_cannot_be_lost_by_terminal_ack()",
        "fn ack_time_watermark_is_non_decreasing_and_fingerprinted()",
        "fn resolved_duplicate_rejects_reversed_ack_time()",
        "fn production_public_attach_apply_accepted_resolves_stage5c_once()",
        "fn production_public_submitted_then_recovered_resolves_stage5c_once()",
        "fn production_public_pre_callback_block_retains_linear_session()",
        "fn production_public_contradiction_blocks_and_duplicate_is_idempotent()",
        "attach_stage5g_mock_ack_session(",
        "apply_stage5g_mock_ack(",
        "apply_stage5g_duplicate_after_resolution(",
    ], "R2 executable witnesses")
    deterministic_prefix = tests.split("fn production_integration_strategy", 1)[0]
    if "Utc::now()" in deterministic_prefix:
        fail("wall clock entered deterministic Stage 5G oracle")
    if tests.count("Utc::now()") != 1:
        fail("controlled production clock witness drift")

    gate = read(root, R1_GATE)
    require(gate, [PREDECESSOR, "checkout --quiet --detach", "base_negative=15/15", "r1_negative=18/18"], "R1 snapshot gate")
    require(read(root, DESIGN), [PREDECESSOR, "Stage 5G-c", "non-decreasing", "No constructor or bypass"], "R2 design")
    require(read(root, STATUS), ["Stage 5G-b R2 is an implementation review candidate", PREDECESSOR, "Stage 5G-c remains blocked"], "current status")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    args = parser.parse_args()
    try:
        check(args.root.resolve())
    except (ValueError, json.JSONDecodeError) as error:
        print(f"stage5g-b-r2-check: FAIL: {error}", file=sys.stderr)
        return 1
    print("stage5g-b-r2-check: PASS")
    print("no_send_provenance: fail_closed")
    print("broker_identity_continuity: exact")
    print("ack_time_watermark: non_decreasing_fingerprint_v3")
    print("production_wrapper_integration: public_api_single_stage5c_callsite")
    print("closed_surfaces: preserved")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
