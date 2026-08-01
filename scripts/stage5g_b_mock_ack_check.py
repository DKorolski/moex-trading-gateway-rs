#!/usr/bin/env python3
"""Fail-closed Stage 5G-b mock ACK contract checker."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
from pathlib import Path


REQUIRED_CASES = [
    "GACK01_PLACE_ACCEPTED_EXACT_IDS",
    "GACK02_SUBMITTED_MISSING_BROKER_ID_KEEPS_PENDING",
    "GACK03_RECOVERED_EXACT_BROKER_ID",
    "GACK04_REJECTED_EXACT_REQUEST_CLEARS_PENDING",
    "GACK05_TIMEOUT_KEEPS_PENDING",
    "GACK06_UNKNOWN_PENDING_KEEPS_PENDING",
    "GACK07_DUPLICATE_REQUIRES_PRIOR_OUTCOME",
    "GACK08_EXPIRED_REQUIRES_EXACT_NO_SEND_PROOF",
    "GACK09_REQUEST_OR_CLIENT_ID_MISMATCH_BLOCKS",
    "GACK10_BROKER_ORDER_ID_CONFLICT_BLOCKS",
]

REVIEW_NEGATIVES = [
    "ACK_BEFORE_INTENT_OWNERSHIP_COMPILE_FAIL",
    "DUPLICATE_ACK_BLOCKS",
    "TERMINAL_ACK_TWICE_BLOCKS",
    "WRONG_ACCOUNT_OR_INSTRUMENT_BLOCKS",
    "WRONG_SIDE_OR_ACTION_BLOCKS",
    "ACK_AFTER_LIFECYCLE_EXPIRY_BLOCKS",
    "REQUEST_OR_CLIENT_ID_MISMATCH_BLOCKS",
    "ACK_CANNOT_CHANGE_BROKER_TRUTH_WITHOUT_ORDER_EVENT",
]

EXPECTED_PREDECESSORS = {
    "stage5f_source_ref": "fb8245e2f91cfc1678548a1228e8558d9adc2181",
    "stage5f_closure_ref": "cac83da38725aeadd6d029a3078157c2ab7fa004",
    "stage5g_a_design_ref": "011fd4b7baaa41fffdad7d3c28e463b7977f5989",
    "stage5g_a_verdict": "ACCEPTED",
}

EXPECTED_FALSE_SURFACES = {
    "real_finam_post",
    "real_finam_delete",
    "finam_transport",
    "redis_live_consumer",
    "redis_consumer_groups",
    "broker_dispatch",
    "broker_execution",
    "runtime_live",
    "live_ready",
    "unattended_execution",
    "real_orders",
    "order_trade_position_events",
    "native_stop_sltp_bracket",
    "stage6_durable_command_chain",
}

PRODUCTION_FORBIDDEN = [
    r"\breqwest\b",
    r"\bredis\s*::",
    r"\btokio\s*::",
    r"\bstd\s*::\s*thread\b",
    r"\bstd\s*::\s*net\b",
    r"\.\s*post\s*\(",
    r"\.\s*delete\s*\(",
    r"\bBrokerOrderSnapshot\b",
    r"\bBrokerTradeSnapshot\b",
    r"\bBrokerPositionSnapshot\b",
    r"\bon_broker_order\b",
    r"\bon_broker_position\b",
    r"\bon_broker_trade\b",
    r"\bBrokerOrderId\s*::\s*new\s*\(",
    r"\benum\s+CommandAckStatus\b",
]


def fail(message: str) -> None:
    raise ValueError(message)


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def load_json(path: Path) -> object:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        fail(f"cannot read valid JSON {path}: {error}")


def require_file(root: Path, relative: str) -> Path:
    path = root / relative
    if not path.is_file():
        fail(f"required file missing: {relative}")
    return path


def strip_rust_comments_and_literals(source: str) -> str:
    """Keep Rust token shape while removing comments and string/char bodies."""

    output: list[str] = []
    index = 0
    block_depth = 0
    state = "code"
    raw_hashes = 0
    while index < len(source):
        if state == "code":
            if source.startswith("//", index):
                state = "line_comment"
                output.extend("  ")
                index += 2
            elif source.startswith("/*", index):
                state = "block_comment"
                block_depth = 1
                output.extend("  ")
                index += 2
            elif source[index] == '"':
                state = "string"
                output.append('"')
                index += 1
            elif source[index] == "'" and re.match(
                r"'(?:\\.|[^\\'\n])'", source[index:]
            ):
                state = "char"
                output.append("'")
                index += 1
            elif source[index] == "r":
                raw = re.match(r'r(#{0,16})"', source[index:])
                if raw:
                    raw_hashes = len(raw.group(1))
                    state = "raw_string"
                    output.extend(" " * raw.end())
                    index += raw.end()
                else:
                    output.append(source[index])
                    index += 1
            else:
                output.append(source[index])
                index += 1
        elif state == "line_comment":
            if source[index] == "\n":
                state = "code"
                output.append("\n")
            else:
                output.append(" ")
            index += 1
        elif state == "block_comment":
            if source.startswith("/*", index):
                block_depth += 1
                output.extend("  ")
                index += 2
            elif source.startswith("*/", index):
                block_depth -= 1
                output.extend("  ")
                index += 2
                if block_depth == 0:
                    state = "code"
            else:
                output.append("\n" if source[index] == "\n" else " ")
                index += 1
        elif state in {"string", "char"}:
            terminator = '"' if state == "string" else "'"
            if source[index] == "\\":
                output.append(" ")
                index += 1
                if index < len(source):
                    output.append(" ")
                    index += 1
            elif source[index] == terminator:
                output.append(terminator)
                index += 1
                state = "code"
            else:
                output.append("\n" if source[index] == "\n" else " ")
                index += 1
        else:
            terminator = '"' + ("#" * raw_hashes)
            if source.startswith(terminator, index):
                output.extend(" " * len(terminator))
                index += len(terminator)
                state = "code"
            else:
                output.append("\n" if source[index] == "\n" else " ")
                index += 1
    return "".join(output)


def check(root: Path) -> None:
    contract_path = require_file(
        root, "docs/stage-5/stage5g-b-mock-ack-contract.json"
    )
    entry_path = require_file(
        root, "docs/stage-5/stage5g-lifecycle-entry-inventory.json"
    )
    design_path = require_file(root, "docs/stage-5/5g-b-mock-ack-attachment.md")
    module_path = require_file(
        root, "crates/strategy-runtime-core/src/stage5g_mock_ack.rs"
    )
    lib_path = require_file(root, "crates/strategy-runtime-core/src/lib.rs")
    status_path = require_file(root, "docs/current-status.md")

    contract = load_json(contract_path)
    entry = load_json(entry_path)
    if not isinstance(contract, dict) or not isinstance(entry, dict):
        fail("contract and entry inventory must be JSON objects")
    if contract.get("schema_version") != 1:
        fail("Stage 5G-b schema_version drift")
    if contract.get("stage") != "5G-b-mock-ack-attachment":
        fail("Stage 5G-b stage identity drift")
    if contract.get("status") != "implementation_review_candidate":
        fail("Stage 5G-b status must remain implementation_review_candidate")
    if contract.get("accepted_predecessors") != EXPECTED_PREDECESSORS:
        fail("accepted predecessor binding drift")
    if contract.get("required_case_ids") != REQUIRED_CASES:
        fail("Stage 5G-b required ACK matrix drift")
    if contract.get("review_negative_cases") != REVIEW_NEGATIVES:
        fail("Stage 5G-b review negative matrix drift")

    entry_cases = None
    for family in entry.get("scenario_families", []):
        if family.get("id") == "ACK" and family.get("owner_stage") == "5G-b":
            entry_cases = family.get("case_ids")
            break
    if entry_cases != REQUIRED_CASES:
        fail("Stage 5G-a ACK inventory no longer matches Stage 5G-b")

    surfaces = contract.get("closed_surfaces")
    if not isinstance(surfaces, dict) or set(surfaces) != EXPECTED_FALSE_SURFACES:
        fail("Stage 5G-b closed-surface inventory drift")
    if any(surfaces.values()):
        fail("a Stage 5G-b closed surface was enabled")
    transition = contract.get("next_transition")
    if not isinstance(transition, dict):
        fail("Stage 5G-b next_transition missing")
    expected_transition = {
        "stage5g_b_review_required": True,
        "after_acceptance": "5G-c-order-trade-position-convergence",
        "stage5g_c_open": False,
        "stage6_open": False,
        "main_merge_authorized": False,
        "deployment_authorized": False,
    }
    if transition != expected_transition:
        fail("Stage 5G-b transition authority drift")

    for authority in entry.get("reuse_authorities", []):
        if authority.get("mutability") not in {"frozen", "frozen_for_5g_entry"}:
            continue
        relative = authority.get("path")
        expected_hash = authority.get("sha256")
        if not isinstance(relative, str) or not isinstance(expected_hash, str):
            fail("invalid frozen authority record")
        if sha256(require_file(root, relative)) != expected_hash:
            fail(f"frozen Stage 5G authority drift: {relative}")

    module_source = module_path.read_text(encoding="utf-8")
    production_source = module_source.split("#[cfg(test)]", maxsplit=1)[0]
    stripped = strip_rust_comments_and_literals(production_source)
    for pattern in PRODUCTION_FORBIDDEN:
        if re.search(pattern, stripped):
            fail(f"forbidden Stage 5G-b production surface matched: {pattern}")
    if len(re.findall(r"resolve_stage5c_paper_intent_lifecycle\s*\(", stripped)) != 1:
        fail("Stage 5G-b must have exactly one Stage 5C-i callsite")
    if len(re.findall(r"\.\s*evaluate_ack\s*\(", stripped)) != 2:
        fail("Stage 5G-b Broker Core ACK policy delegation drift")
    for required in [
        "pub struct Stage5gMockAckSession",
        "pub struct Stage5gResolvedMockAckPaperStrategy",
        "pub fn attach_stage5g_mock_ack_session",
        "pub fn apply_stage5g_mock_ack",
        "pub fn apply_stage5g_duplicate_after_resolution",
        "broker_truth_changed(&self) -> bool",
    ]:
        if required not in production_source:
            fail(f"Stage 5G-b implementation marker missing: {required}")

    for marker in [
        "gack01_place_accepted_exact_ids_resolves_without_broker_truth",
        "gack02_and_gack03_missing_broker_id_waits_then_recovered_resolves",
        "gack04_rejected_exact_request_clears_pending",
        "gack05_and_gack06_ambiguous_statuses_keep_pending",
        "gack07_duplicate_requires_prior_outcome_and_exact_duplicate_is_noop",
        "gack08_expired_requires_exact_no_send_proof",
        "gack09_wrong_request_and_client_ids_block_atomically",
        "gack10_conflicting_broker_order_id_blocks",
        "wrong_account_instrument_side_and_action_block_before_callback",
        "duplicate_ack_terminal_twice_and_expired_lifecycle_block",
        "cancel_binding_is_exact_and_carries_no_side",
        "lifecycle_fingerprint_is_deterministic_for_same_input",
    ]:
        if marker not in module_source:
            fail(f"Stage 5G-b test witness missing: {marker}")

    lib_source = lib_path.read_text(encoding="utf-8")
    if lib_source.count("mod stage5g_mock_ack;") != 1:
        fail("Stage 5G-b module activation drift")
    for marker in [
        "attach_stage5g_mock_ack_session",
        "apply_stage5g_mock_ack",
        "Stage5gMockAckSession",
        "Stage 5G-b ACK feedback cannot be attached before ownership",
        "The linear Stage 5G-b session itself cannot be forged",
    ]:
        if marker not in lib_source:
            fail(f"Stage 5G-b export/type-state witness missing: {marker}")

    design = design_path.read_text(encoding="utf-8")
    for case_id in REQUIRED_CASES:
        if case_id not in design:
            fail(f"Stage 5G-b design omits {case_id}")
    for marker in [
        "Stage 5G-c remains blocked",
        "no generated or synthetic `BrokerOrderId`",
        "ACK cannot change broker truth without an order event",
    ]:
        if marker not in design:
            fail(f"Stage 5G-b design boundary missing: {marker}")

    status = status_path.read_text(encoding="utf-8")
    for marker in [
        "Stage 5G-a is independently accepted",
        "Stage 5G-b is an implementation review candidate",
        "Stage 5G-c remains blocked",
    ]:
        if marker not in status:
            fail(f"current-status Stage 5G-b marker missing: {marker}")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    args = parser.parse_args()
    try:
        check(args.root.resolve())
    except ValueError as error:
        print(f"stage5g-b-mock-ack-check: FAIL: {error}", file=sys.stderr)
        return 1
    print("stage5g-b-mock-ack-check: PASS")
    print(f"required_ack_cases: {len(REQUIRED_CASES)}/{len(REQUIRED_CASES)}")
    print(f"review_negative_cases: {len(REVIEW_NEGATIVES)}/{len(REVIEW_NEGATIVES)}")
    print("closed_surfaces: preserved")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
