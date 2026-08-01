#!/usr/bin/env python3
"""Fail-closed checker for Stage 5G-b R3 duplicate transition identity."""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

MODULE = "crates/strategy-runtime-core/src/stage5g_mock_ack.rs"
CONTRACT = "docs/stage-5/stage5g-b-r3-contract.json"
DESIGN = "docs/stage-5/5g-b-r3-duplicate-transition-identity.md"
STATUS = "docs/current-status.md"
R2_GATE = "scripts/stage5g_b_r2_snapshot_gate.sh"
ORIGIN_GATE = "scripts/stage5g_b_r3_origin_sync_gate.sh"
PREDECESSOR = "d03f6e5e88fb853290457d6d6dac08f21c2cf28b"
GOLDEN = "9e009c1c4e00809b94c3af7291f6aa4411dd67c65bd6a2bd1b5108d85256bf38"


def fail(message: str) -> None:
    raise ValueError(message)


def read(root: Path, relative: str) -> str:
    path = root / relative
    if not path.is_file():
        fail(f"required R3 file missing: {relative}")
    return path.read_text(encoding="utf-8")


def require(text: str, tokens: list[str], label: str) -> None:
    for token in tokens:
        if token not in text:
            fail(f"{label} token missing: {token}")


def function_slice(source: str, name: str, next_name: str) -> str:
    start = source.find(f"fn {name}(")
    end = source.find(f"fn {next_name}(", start + 1)
    if start < 0 or end <= start:
        fail(f"cannot isolate fn {name}")
    return source[start:end]


def check(root: Path) -> None:
    contract = json.loads(read(root, CONTRACT))
    if contract.get("schema_version") != 4:
        fail("R3 contract schema drift")
    if contract.get("immutable_predecessor") != PREDECESSOR:
        fail("R3 predecessor drift")
    if contract.get("status") != "implementation_review_candidate":
        fail("R3 status drift")
    fingerprint = contract.get("fingerprint", {})
    if fingerprint.get("schema_version") != 4:
        fail("R3 fingerprint schema drift")
    if fingerprint.get("domain") != "moex.stage5g.mock-ack-lifecycle.v4\\0":
        fail("R3 fingerprint domain drift")
    if fingerprint.get("current_lifecycle_fingerprint_sha256_bound") is not True:
        fail("current lifecycle fingerprint contract unbound")
    if fingerprint.get("golden_stage5f_market_transition_sha256") != GOLDEN:
        fail("R3 golden drift")
    if any(contract.get("closed_surfaces", {}).values()):
        fail("R3 closed surface opened")
    if contract.get("next_transition") != {
        "independent_review_required": True,
        "stage5g_c_open": False,
        "main_merge_authorized": False,
        "deployment_authorized": False,
    }:
        fail("R3 transition authority drift")

    source = read(root, MODULE)
    production, marker, tests = source.partition("#[cfg(test)]")
    if not marker:
        fail("R3 tests missing")
    require(production, [
        "pub const STAGE5G_MOCK_ACK_SCHEMA_VERSION: u16 = 4;",
        "moex.stage5g.mock-ack-lifecycle.v4\\0",
        "current_lifecycle_fingerprint_sha256: String",
        "current_lifecycle_fingerprint_sha256: stage5g_state_fingerprint(state)",
    ], "R3 transition schema")
    transition = function_slice(
        production, "stage5g_transition_fingerprint", "stage5g_canonical_ack_fingerprint_projection"
    )
    if transition.count("current_lifecycle_fingerprint_sha256") != 2:
        fail("current lifecycle fingerprint projection binding drift")
    if "pre_callback_lifecycle_fingerprint_sha256.to_string()" in transition:
        fail("current lifecycle fingerprint rebound to pre-callback identity")

    duplicate = function_slice(
        production,
        "stage5g_apply_duplicate_to_resolved_state",
        "stage5g_event_disposition",
    )
    require(duplicate, [
        "state.last_ack_received_ts_utc = Some(event.ack.received_ts)",
        "state.last_total_sequence = Some(event.total_sequence)",
        "state.duplicate_status_count += 1",
    ], "successful duplicate mutation")
    if "false && event.ack.received_ts" in duplicate:
        fail("duplicate ACK-time check bypassed")

    require(tests, [
        "fn duplicate_timestamp_changes_transition_fingerprint()",
        "fn duplicate_timestamp_changes_continuation_semantics()",
        "T+25 is valid after the T+20 watermark",
        "Stage5gMockAckError::NonMonotonicAckTime",
        "fn production_public_duplicate_time_changes_transition_fingerprint_without_callback_replay()",
        "duplicate replay must not invoke Stage 5C again",
        "earlier.transition_fingerprint_sha256()",
        "later.transition_fingerprint_sha256()",
        GOLDEN,
    ], "R3 executable witnesses")
    if len(re.findall(r"(?<![A-Za-z0-9_])resolve_stage5c_paper_intent_lifecycle\(", production)) != 1:
        fail("Stage 5C resolver production callsite drift")

    r2_gate = read(root, R2_GATE)
    require(r2_gate, [PREDECESSOR, "checkout --quiet --detach", "r2_negative=12/12"], "R2 snapshot")
    origin_gate = read(root, ORIGIN_GATE)
    require(origin_gate, ["origin/stage5g-lifecycle", 'test "$head_ref" = "$origin_ref"', "origin-sync-gate: PASS"], "origin sync")
    require(read(root, DESIGN), [PREDECESSOR, "current_lifecycle_fingerprint_sha256", "T+20", "T+30", "Stage 5G-c"], "R3 design")
    require(read(root, STATUS), ["Stage 5G-b R3 is an implementation review candidate", PREDECESSOR, "Stage 5G-c remains blocked"], "current status")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    args = parser.parse_args()
    try:
        check(args.root.resolve())
    except (ValueError, json.JSONDecodeError) as error:
        print(f"stage5g-b-r3-check: FAIL: {error}", file=sys.stderr)
        return 1
    print("stage5g-b-r3-check: PASS")
    print("transition_fingerprint: current_lifecycle_state_bound_v4")
    print("duplicate_time_collision: closed")
    print("continuation_semantics: witnessed")
    print("production_wrapper: no_callback_replay")
    print("closed_surfaces: preserved")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
