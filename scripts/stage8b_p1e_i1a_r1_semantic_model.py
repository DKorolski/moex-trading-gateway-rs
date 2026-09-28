#!/usr/bin/env python3
"""Execute the Stage 8B-P1-e I1A R1 design model fixtures."""

from __future__ import annotations

import json
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
FIXTURES = ROOT / "docs/stage-8/stage8b-p1e-i1a-r1-model-fixtures-v1.json"


def fail(message: str) -> None:
    raise SystemExit(f"stage8b-p1e-i1a-r1-semantic-model: FAIL: {message}")


def reject_duplicate_keys(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    result: dict[str, Any] = {}
    for key, value in pairs:
        if key in result:
            raise ValueError(f"duplicate key: {key}")
        result[key] = value
    return result


def load() -> dict[str, Any]:
    try:
        value = json.loads(FIXTURES.read_text(), object_pairs_hook=reject_duplicate_keys)
    except (OSError, UnicodeDecodeError, ValueError, json.JSONDecodeError) as error:
        fail(f"invalid fixture JSON: {error}")
    if not isinstance(value, dict):
        fail("fixture root is not an object")
    return value


def progression(case: dict[str, Any]) -> str:
    candidate = case["candidate"]
    if candidate["valid"] is not True:
        return "blocked-invalid-newest-no-fallback"
    high_water = case["high_water"]
    if high_water is None:
        return "accept-bootstrap"
    if candidate["generation"] != high_water["generation"]:
        return "blocked-unreviewed-generation"
    if candidate["sequence"] < high_water["sequence"]:
        return "blocked-rollback"
    if candidate["sequence"] == high_water["sequence"]:
        return (
            "idempotent"
            if candidate["envelope_hash"] == high_water["envelope_hash"]
            else "terminal-conflict"
        )
    if candidate["published_at_ms"] <= high_water["published_at_ms"]:
        return "blocked-time-rollback"
    if candidate["semantic_revision"] < high_water["semantic_revision"]:
        return "blocked-semantic-rollback"
    if (
        candidate["semantic_revision"] == high_water["semantic_revision"]
        and candidate["semantic_hash"] != high_water["semantic_hash"]
    ):
        return "terminal-semantic-conflict"
    return "accept-monotonic-jump"


def route(case: dict[str, Any]) -> str:
    if (
        case.get("restart") == "after_binding"
        and case.get("durable_binding") is True
        and case.get("identical_transition") is True
    ):
        return "reissue-historical-day-expiry"
    if case["fresh"] is not True:
        return "blocked-fresh-source-required"
    route_name = case["route"]
    kind_state = (case["kind"], case["state"])
    if route_name == "market_execution":
        return "issue-market" if kind_state == ("tradability", "open") else "blocked-route-evidence"
    if route_name == "working_limit_evaluation":
        return "issue-working" if kind_state == ("tradability", "open") else "blocked-route-evidence"
    if route_name == "cancel_step":
        if kind_state not in {("tradability", "open"), ("day_boundary", "closed")}:
            return "blocked-route-evidence"
        return "issue-cancel-no-trading-grant" if case["exact_book"] is True else "blocked-route-evidence"
    if route_name != "day_expiry":
        fail(f"unknown route in {case['id']}: {route_name}")
    if kind_state != ("day_boundary", "closed"):
        return "blocked-route-evidence"
    if not all(
        case[name] is True
        for name in ("exact_book", "exact_last_m10", "no_later_eligible_m10", "boundary_reached")
    ):
        return "blocked-day-proof"
    return "issue-day-expiry"


PHASE_RESULTS = {
    "B00_SOURCE_VERIFIED_VOLATILE": "reacquire-fresh-source",
    "B01_JOURNAL_FSYNCED_SEAL_MISSING": "complete-one-exact-covering-seal",
    "B02_SEAL_DURABLE_REREAD_PENDING": "reread-cross-validate-no-second-seal",
    "B03_BINDING_COMMITTED": "issue-exact-authority-no-business-effect",
    "B04_AUTHORITY_ISSUED_VOLATILE": "reissue-exact-authority-no-duplicate-effect",
    "B05_BUSINESS_TRANSITION_DURABLE": "existing-lifecycle-continuation-no-rebind",
    "B06_SOURCE_XACKED_LAST": "terminal-no-effect",
}

TIMER_RESULTS = {
    "after_source_before_reclassification": "stop-original-timer-no-seal-no-effect",
    "after_reclassification_before_schedule_work": "stop-reclassified-owner-no-seal-no-effect",
    "after_schedule_read_before_binding": "stop-reclassified-owner-no-seal-no-effect",
    "during_noncancellable_binding": "finish-exact-binding-then-stop-no-effect",
    "after_binding_before_execution": "stop-bound-owner-no-second-seal-no-effect",
    "all_latches_clear": "three-steps-one-binding-one-effect",
    "schedule_missing": "degraded-reclassified-owner-no-seal-no-effect",
}


def phase(case: dict[str, Any]) -> str:
    try:
        return PHASE_RESULTS[case["phase"]]
    except KeyError as error:
        fail(f"unknown binding phase in {case['id']}: {error}")


def timer(case: dict[str, Any]) -> str:
    try:
        return TIMER_RESULTS[case["frontier"]]
    except KeyError as error:
        fail(f"unknown timer frontier in {case['id']}: {error}")


def publisher(case: dict[str, Any]) -> str:
    if case["command_has_nomkstream"] is not True:
        return "contract-rejected-before-command"
    return "append-exact-maxlen-4096" if case["stream_exists"] is True else "fail-no-create"


def main() -> None:
    fixtures = load()
    if fixtures.get("schema_version") != 1:
        fail("schema version drift")
    if fixtures.get("domain") != "moex.stage8b.p1e.i1a.r1.model-fixtures.v1":
        fail("domain drift")
    groups = [
        ("progression_cases", 12, progression),
        ("route_cases", 12, route),
        ("binding_phase_cases", 7, phase),
        ("timer_cases", 7, timer),
        ("publisher_cases", 3, publisher),
    ]
    seen: set[str] = set()
    passed = 0
    for name, expected_count, evaluator in groups:
        cases = fixtures.get(name)
        if not isinstance(cases, list) or len(cases) != expected_count:
            fail(f"{name} count is not {expected_count}")
        for case in cases:
            if not isinstance(case, dict) or not isinstance(case.get("id"), str):
                fail(f"invalid case in {name}")
            if case["id"] in seen:
                fail(f"duplicate case id: {case['id']}")
            seen.add(case["id"])
            actual = evaluator(case)
            if actual != case.get("expected"):
                fail(f"{case['id']}: expected {case.get('expected')!r}, got {actual!r}")
            print(f"PASS {case['id']} {actual}")
            passed += 1
    print(f"stage8b-p1e-i1a-r1-semantic-model: PASS {passed}/{passed}")


if __name__ == "__main__":
    main()
