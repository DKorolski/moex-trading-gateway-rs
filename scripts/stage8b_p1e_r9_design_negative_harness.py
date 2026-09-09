#!/usr/bin/env python3
"""Separated integrity and redigested semantic mutations for P1-e R9."""

from __future__ import annotations

import csv
import io
import json
from typing import Any, Callable

import stage8b_p1e_r9_design_check as checker


BASE = checker.read_all()


def json_bytes(value: Any) -> bytes:
    return (json.dumps(value, indent=2, sort_keys=True) + "\n").encode()


def mutate_json(blobs: dict[str, bytes], name: str, change: Callable[[dict[str, Any]], None]) -> None:
    value = json.loads(blobs[name])
    change(value)
    blobs[name] = json_bytes(value)


def mutate_csv(blobs: dict[str, bytes], name: str, change: Callable[[list[dict[str, str]]], None]) -> None:
    rows = checker.csv_rows(blobs[name])
    fields = list(rows[0])
    change(rows)
    stream = io.StringIO(newline="")
    writer = csv.DictWriter(stream, fieldnames=fields, lineterminator="\n")
    writer.writeheader()
    writer.writerows(rows)
    blobs[name] = stream.getvalue().encode()


def fixture(blobs: dict[str, bytes], fixture_id: str) -> dict[str, Any]:
    return next(item for item in json.loads(blobs["outcomes"])["fixtures"] if item["fixture_id"] == fixture_id)


def mutate_fixture(blobs: dict[str, bytes], target_id: str, **changes: Any) -> None:
    def change(value: dict[str, Any]) -> None:
        next(item for item in value["fixtures"] if item["fixture_id"] == target_id).update(changes)
    mutate_json(blobs, "outcomes", change)


def amendment(blobs: dict[str, bytes], cell_id: str) -> dict[str, Any]:
    return next(item for item in json.loads(blobs["routes"])["exact_row_amendments"] if item["cell_id"] == cell_id)


def mutate_amendment(blobs: dict[str, bytes], cell_id: str, **changes: Any) -> None:
    def change(value: dict[str, Any]) -> None:
        next(item for item in value["exact_row_amendments"] if item["cell_id"] == cell_id).update(changes)
    mutate_json(blobs, "routes", change)


def redigest(blobs: dict[str, bytes]) -> None:
    active = json.loads(blobs["active"])
    active["r9_source"]["sha256"] = checker.sha256(blobs["acceptance"])
    blobs["active"] = json_bytes(active)
    semantic = json.loads(blobs["semantic"])
    bindings = semantic["contract_bindings"]
    for key, name in (
        ("latch_route_transition_matrix_v2_sha256", "routes"),
        ("route_outcome_fixture_matrix_v1_sha256", "outcomes"),
        ("source_timer_precedence_v4_sha256", "timer"),
        ("supervisor_event_matrix_v4_sha256", "events"),
        ("i0_regression_gate_v2_sha256", "regression"),
    ):
        bindings[key] = checker.sha256(blobs[name])
    blobs["semantic"] = json_bytes(semantic)
    evidence = json.loads(blobs["evidence"])
    for name in evidence["contract_sha256"]:
        evidence["contract_sha256"][name] = checker.sha256(blobs[name])
    blobs["evidence"] = json_bytes(evidence)


integrity_cases: list[tuple[str, dict[str, bytes]]] = []
for name in ("design", "acceptance", "active", "routes", "outcomes", "timer", "events", "regression"):
    blobs = dict(BASE)
    blobs[name] += b"\n"
    integrity_cases.append((f"integrity-{name}-byte-drift", blobs))


semantic_cases: list[tuple[str, dict[str, bytes]]] = []


def add(name: str, change: Callable[[dict[str, bytes]], None]) -> None:
    blobs = dict(BASE)
    change(blobs)
    redigest(blobs)
    semantic_cases.append((name, blobs))


add("active-total-drift", lambda b: mutate_json(b, "active", lambda v: v["active_contract_expectation"].update(total_active_rows=299)))
add("active-supersession-omitted", lambda b: mutate_json(b, "active", lambda v: v["superseded_base_rows"].remove("P1ER8-020")))
add("semantic-reattachment-authority-drift", lambda b: mutate_json(b, "semantic", lambda v: v["replacement_active_authorities"].update({"shutdown.ContinuationLinearization": "drain-all-S_ack-to-S_truth"})))
add("semantic-normalized-oracle-key-omitted", lambda b: mutate_json(b, "semantic", lambda v: v["required_new_keys"].remove("test.RouteOutcomeNormalizedOracle")))
add("route-proof-count-drift", lambda b: mutate_json(b, "routes", lambda v: v.update(proof_case_count=45)))
add("route-lr04-called-durable-seal", lambda b: mutate_amendment(b, "LR04-default", boundary_kind="durable_covering_seal"))
add("route-lr10-falls-through-to-truth", lambda b: mutate_amendment(b, "LR10-default", post_permit_signal_stop_policy="continue-to-S_truth"))
add("route-due-already-fixture-omitted", lambda b: mutate_json(b, "routes", lambda v: v["proof_fixture_ids_by_cell"]["LT05-due-day-timer"].remove("FX42")))
add("outcome-duplicate-fixture-id", lambda b: mutate_fixture(b, "FX46", fixture_id="FX45"))
add("outcome-fixture-removed", lambda b: mutate_json(b, "outcomes", lambda v: (v["fixtures"].pop(), v.update(case_count=45))))
add("outcome-lr04-implicit-truth", lambda b: mutate_fixture(b, "FX04", expected_boundary="p1d2_s_truth"))
add("outcome-lr04-source-acknowledged", lambda b: mutate_fixture(b, "FX04", source_pel_disposition="acknowledged"))
add("outcome-attach-commit-counter", lambda b: mutate_json(b, "outcomes", lambda v: v["effect_profiles"]["ATTACH"].update(durable_commit_total=1)))
add("outcome-due-already-xack", lambda b: mutate_json(b, "outcomes", lambda v: v["effect_profiles"]["DUE_ALREADY_RECLASSIFY"].update(xack_total=1)))
add("outcome-due-executes-timer", lambda b: mutate_json(b, "outcomes", lambda v: v["effect_profiles"]["DUE_PENDING_RECLASSIFY"].update(timer_execution_total=1)))
add("outcome-stale-union-restored", lambda b: mutate_fixture(b, "FX45", expected_owner_or_disposition="SourceResolvedThenTimerReclassified|StaleTimerDiscarded"))
add("event-e05-owner-broadened", lambda b: mutate_csv(b, "events", lambda rows: next(row for row in rows if row["id"] == "E05").update(owner_phase="s_ack_committed")))
add("event-e05-route-exclusion-removed", lambda b: mutate_csv(b, "events", lambda rows: next(row for row in rows if row["id"] == "E05").update(allowed_next_effect="persist_reread_cross_validate_s_truth_only")))
add("event-e25-implicit-fallthrough-allowed", lambda b: mutate_csv(b, "events", lambda rows: next(row for row in rows if row["id"] == "E25").update(allowed_next_effect="complete_route_then_continue_to_s_truth_and_xack")))
add("timer-already-source-omitted", lambda b: mutate_json(b, "timer", lambda v: v["simultaneous_due_timer"]["source_variants"].remove("source_already_acknowledged_with_continuous_frontier")))
add("timer-first-latch-mutates-timer", lambda b: mutate_json(b, "timer", lambda v: v["simultaneous_due_timer"].update(first_latch_recheck="if-set-discard-timer")))
add("timer-execution-bundled-with-reclassification", lambda b: mutate_json(b, "timer", lambda v: v["simultaneous_due_timer"].update(timer_execution="execute-during-reclassification")))
add("regression-historical-entrypoint-restored", lambda b: mutate_json(b, "regression", lambda v: v.update(entrypoint="bash scripts/stage8b_p1d4_source_gate.sh")))
add("regression-production-allowlist-expanded", lambda b: mutate_json(b, "regression", lambda v: v["current_i0_scope"]["production_allowlist"].append("crates/finam-gateway/src/lib.rs")))
add("regression-direct-historical-gate", lambda b: mutate_json(b, "regression", lambda v: v["historical_p1d4"].update(historical_gate_invoked_directly_by_i0=True)))
add("regression-one-clean-run", lambda b: mutate_json(b, "regression", lambda v: v["historical_p1d4"].update(clean_runs=1)))
add("regression-p1d3-filter-omitted", lambda b: mutate_json(b, "regression", lambda v: v["cross_slice_test_filters"].pop()))
add("evidence-claims-source-modified", lambda b: mutate_json(b, "evidence", lambda v: v.update(source_modified=True)))

if len(integrity_cases) != 8 or len(semantic_cases) != 28:
    raise SystemExit(f"R9 mutation inventory drifted: integrity={len(integrity_cases)} semantic={len(semantic_cases)}")


errors = (
    checker.CheckFailure, checker.r8.CheckFailure, checker.r8.r7.CheckFailure,
    checker.r8.r7.r6.CheckFailure, OSError, ValueError, KeyError, TypeError,
    json.JSONDecodeError,
)
escaped: list[str] = []
for name, blobs in integrity_cases:
    try:
        checker.validate(blobs, check_integrity=True)
    except errors:
        print(f"PASS integrity {name}")
    else:
        escaped.append(name)
        print(f"FAIL integrity {name}")

for name, blobs in semantic_cases:
    try:
        checker.validate(blobs, check_integrity=False)
    except errors:
        print(f"PASS semantic-redigested {name}")
    else:
        escaped.append(name)
        print(f"FAIL semantic-redigested {name}")

if escaped:
    raise SystemExit("R9 mutations escaped: " + ", ".join(escaped))
print("PASS stage8b-p1e-r9-design-negative-harness integrity=8/8 semantic-redigested=28/28 hash-guard-disabled-for-semantic=true")
