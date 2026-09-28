#!/usr/bin/env python3
"""Fail-closed cross-contract checker for Stage 8B-P1-e R9 design."""

from __future__ import annotations

import csv
import hashlib
import io
import json
import pathlib
import subprocess
from typing import Any

import stage8b_p1e_r8_design_check as r8


ROOT = pathlib.Path(__file__).resolve().parents[1]
BASE = "fcac93e47e6dbb2f5c96c0fa28ce1c99cd603b3e"
ACCEPTED = "c2a9e1246dfdd59f3a6297268de907dedcb19903"
R8_REVIEW_SHA256 = "fedf29f37ef583905a9f09a2eccdeaa3303b11d14f8df237f5b8224384bd7ef4"
SOURCE_SHA = "a870fbbb6ec9fc60b7df9c35a2aca7a5daf81f695b019352d7f20c9b51439d16"
DOCS = "docs/stage-8/"
FILES = {
    "design": DOCS + "stage8b-p1e-deployable-supervisor-design-r9.md",
    "acceptance": DOCS + "stage8b-p1e-deployable-supervisor-r9-acceptance-matrix.csv",
    "active": DOCS + "stage8b-p1e-active-acceptance-contract-v9.json",
    "semantic": DOCS + "stage8b-p1e-semantic-authority-registry-v9.json",
    "routes": DOCS + "stage8b-p1e-latch-route-transition-matrix-v2.json",
    "outcomes": DOCS + "stage8b-p1e-route-outcome-fixture-matrix-v1.json",
    "timer": DOCS + "stage8b-p1e-source-timer-precedence-v4.json",
    "events": DOCS + "stage8b-p1e-supervisor-event-matrix-v4.csv",
    "regression": DOCS + "stage8b-p1e-i0-regression-gate-v2.json",
    "evidence": DOCS + "stage8b-p1e-deployable-supervisor-r9-design-evidence.json",
    "source": "crates/runtime-durable-service/src/stage8b_p1_semantic/redis.rs",
    "status": "docs/current-status.md",
    "roadmap": "docs/roadmap.md",
}
EXPECTED_CHANGED = {FILES[name] for name in (
    "design", "acceptance", "active", "semantic", "routes", "outcomes",
    "timer", "events", "regression", "evidence", "status", "roadmap",
)} | {
    "scripts/stage8b_p1e_i0_scope_check.py",
    "scripts/stage8b_p1e_i0_p1d4_regression_check.py",
    "scripts/stage8b_p1e_i0_regression_gate.sh",
    "scripts/stage8b_p1e_r9_design_check.py",
    "scripts/stage8b_p1e_r9_design_negative_harness.py",
    "scripts/stage8b_p1e_r9_design_gate.sh",
    "scripts/stage8b_p1e_r9_design_handoff_safety_check.py",
    "scripts/make_stage8b_p1e_r9_design_handoff.py",
}
INTEGRITY_SHA = {
    "design": "6d2b722a3bbec029bb5b571a9fec9b148627a172e8436d48eb1b1611c4a067e3",
    "acceptance": "05f32352e9f7a005668da941da31325f3c0ae8609dde7ef080c2752dae286bc4",
    "active": "2879a44d13b4c5bcab17175969bab2e3c929ac56fe91d25990a6d6f0de5a2092",
    "semantic": "9be4e3ce11ab9c163b59acc0db9bbe6a0be68d5c45db36fe7530f06af3169482",
    "routes": "cf181a83a233469536c68f8b35141a27556aadc686d12570c851409673a7892d",
    "outcomes": "541b002b34e96a4042a20b534be9b6c504a516a9458dfcabd3dab7f8388dce04",
    "timer": "097e1ae6a5164280e8694c7aee27513851d509955bdb0aa1dd9e8c5a12cc6b16",
    "events": "c881ad39ad941b0aad20beb19540d10065b4165c8a9e78c82c94891bc0baab16",
    "regression": "4d01fe160dac5fcd58a1239193e98913265f92aa56f1f6538f7b47a31dd88189",
}
EVIDENCE_SHA = "d30bb5cd64962ca67007093713e20a759f768c7e2beaa1422a113a561e24ddd4"
NORMALIZED_MODEL_SHA256 = "e56bd442481b81124f01eafd91aef36bf21d2d6f861ceb5a1e09932d0d160c2f"
SUPERSEDED = {"P1ER8-018", "P1ER8-019", "P1ER8-020", "P1ER8-025", "P1ER8-026"}
REPLACEMENTS = {
    "shutdown.ContinuationLinearization": "in-flight-permit-completes-only-its-route-boundary;four-reattachments-stop-without-new-seal;normal-loop-E05-never-overrides-E25",
    "test.RouteVariantBoundaryCoverage": "thirty-route-cells-bind-forty-six-authenticated-outcome-fixtures-with-exact-effect-counters-owner-source-and-stop-point",
}
NEW_SEMANTICS = {
    "timer.SourceTimerLatchPrecedence": "pending-or-already-source-resolution-then-latch-check-then-timer-reclassification-then-latch-check-with-timer-execution-later",
    "regression.I0CurrentSourceEntryPoint": "accepted-R9-relative-three-file-scope-plus-reusable-P1d4-content-105x2-negatives-P1d2-P1d3-doctests-clippy-on-I0-head",
    "test.RouteOutcomeNormalizedOracle": "integrity-hash-tests-are-separated-from-redigested-semantic-mutations-rejected-by-independent-normalized-model",
}
PRODUCTION_ALLOWLIST = [
    "crates/runtime-durable-service/src/stage8b_p1_semantic/redis.rs",
    "crates/runtime-durable-service/src/stage8b_p1_semantic.rs",
    "crates/runtime-durable-service/src/lib.rs",
]
FIXTURE_FIELDS = {
    "fixture_id", "cell_id", "authenticated_fixture", "branch_outcome",
    "expected_boundary", "expected_owner_or_disposition", "effect_profile",
    "source_pel_disposition", "stop_point", "post_boundary_latch_recheck",
}
COUNTER_FIELDS = [
    "durable_commit_total", "callback_total", "publication_total",
    "publication_revalidation_total", "xack_total",
    "timer_reclassification_total", "timer_execution_total",
]
PROFILE_ORACLE = {
    "ATTACH": [0, 0, 0, 0, 0, 0, 0],
    "COMMIT": [1, 0, 0, 0, 0, 0, 0],
    "P1D4_COMMIT": [1, 0, 0, 1, 0, 0, 0],
    "PUBLICATION": [0, 0, 1, 0, 0, 0, 0],
    "SEMANTIC_READY": [1, 1, 0, 0, 1, 0, 0],
    "SEMANTIC_PENDING": [1, 1, 0, 0, 0, 0, 0],
    "TERMINAL_PENDING": [0, 0, 0, 0, 1, 0, 0],
    "TERMINAL_ALREADY": [0, 0, 0, 0, 0, 0, 0],
    "P1D4_TERMINAL_PENDING": [0, 0, 0, 1, 1, 0, 0],
    "P1D4_TERMINAL_ALREADY": [0, 0, 0, 1, 0, 0, 0],
    "DUE_PENDING_RECLASSIFY": [0, 0, 0, 0, 1, 1, 0],
    "DUE_ALREADY_RECLASSIFY": [0, 0, 0, 0, 0, 1, 0],
    "DUE_PENDING_STALE": [0, 0, 0, 0, 1, 1, 0],
    "DUE_ALREADY_STALE": [0, 0, 0, 0, 0, 1, 0],
}


class CheckFailure(RuntimeError):
    pass


def require(condition: bool, message: str) -> None:
    if not condition:
        raise CheckFailure(message)


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def csv_rows(data: bytes) -> list[dict[str, str]]:
    return list(csv.DictReader(io.StringIO(data.decode("utf-8"))))


def read_all(overrides: dict[str, bytes] | None = None) -> dict[str, bytes]:
    overrides = overrides or {}
    return {name: overrides.get(name, (ROOT / path).read_bytes()) for name, path in FILES.items()}


def load_json(blobs: dict[str, bytes], name: str) -> dict[str, Any]:
    value = json.loads(blobs[name])
    require(isinstance(value, dict), f"{name} must be an object")
    return value


def changed_files() -> set[str]:
    tracked = subprocess.check_output(["git", "diff", "--name-only", BASE], cwd=ROOT, text=True).splitlines()
    untracked = subprocess.check_output(["git", "ls-files", "--others", "--exclude-standard"], cwd=ROOT, text=True).splitlines()
    return set(tracked) | set(untracked)


def r8_active_ids() -> set[str]:
    return (r8.v7_active_ids() - r8.SUPERSEDED) | {f"P1ER8-{number:03d}" for number in range(1, 28)}


def r8_semantics() -> dict[str, str]:
    result = r8.v7_semantics()
    result.update(r8.REPLACED_SEMANTICS)
    result.update(r8.NEW_SEMANTICS)
    return result


def validate_active(blobs: dict[str, bytes], active: dict[str, Any]) -> None:
    require(active.get("schema_version") == 9 and active.get("domain") == "moex.stage8b.p1e.active-acceptance-contract.v9", "active identity")
    require(active.get("direct_parent") == BASE and active.get("accepted_predecessor") == ACCEPTED, "active lineage")
    require(active.get("base_active_contract") == {"path": r8.FILES["active"], "sha256": r8.sha256((ROOT / r8.FILES["active"]).read_bytes()), "active_rows": 279}, "active V8 binding")
    rows = csv_rows(blobs["acceptance"])
    ids = {row["id"] for row in rows}
    require(len(rows) == len(ids) == 24 and ids == {f"P1ER9-{number:03d}" for number in range(1, 25)}, "R9 acceptance inventory")
    require(all(row["status"] == "REQUIRED" for row in rows), "optional R9 row")
    require(active.get("r9_source") == {"path": FILES["acceptance"], "sha256": sha256(blobs["acceptance"]), "row_count": 24, "superseded_rows": []}, "R9 source binding")
    base_ids = r8_active_ids()
    require(len(base_ids) == 279 and set(active.get("superseded_base_rows", [])) == SUPERSEDED <= base_ids, "R9 superseded rows")
    final_ids = (base_ids - SUPERSEDED) | ids
    mapped: list[str] = []
    for item in active.get("supersession_map", []):
        require(set(item["replacement"]) <= final_ids, "inactive R9 replacement")
        mapped.extend(item["superseded"])
    require(len(mapped) == len(set(mapped)) and set(mapped) == SUPERSEDED, "R9 supersession exactness")
    require(len(final_ids) == 298 and active.get("active_contract_expectation") == {"base_v8_active_rows": 279, "base_rows_superseded_by_r9": 5, "r9_active_rows": 24, "total_active_rows": 298, "all_status_required": True}, "R9 active expectation")


def validate_semantic(blobs: dict[str, bytes], semantic: dict[str, Any]) -> None:
    require(semantic.get("schema_version") == 9 and semantic.get("domain") == "moex.stage8b.p1e.semantic-authority-registry.v9", "semantic identity")
    require(semantic.get("base_registry") == {"path": r8.FILES["semantic"], "sha256": r8.sha256((ROOT / r8.FILES["semantic"]).read_bytes()), "active_key_count": 31}, "semantic V8 binding")
    old = r8_semantics()
    require(semantic.get("superseded_active_authorities") == {key: old[key] for key in REPLACEMENTS}, "semantic superseded values")
    require(semantic.get("replacement_active_authorities") == REPLACEMENTS, "semantic replacements")
    require(semantic.get("required_new_keys") == list(NEW_SEMANTICS), "semantic new-key order")
    require(semantic.get("new_active_authorities") == NEW_SEMANTICS, "semantic new values")
    composed = dict(old)
    composed.update(REPLACEMENTS)
    require(not set(NEW_SEMANTICS) & set(composed), "semantic new-key collision")
    composed.update(NEW_SEMANTICS)
    require(len(composed) == 34 and semantic.get("composed_expectation") == {"retained_base_keys": 29, "replacement_keys": 2, "new_keys": 3, "total_active_keys": 34, "active_conflicts": 0}, "semantic composition")
    require(semantic.get("contract_bindings") == {
        "latch_route_transition_matrix_v2_sha256": sha256(blobs["routes"]),
        "route_outcome_fixture_matrix_v1_sha256": sha256(blobs["outcomes"]),
        "source_timer_precedence_v4_sha256": sha256(blobs["timer"]),
        "supervisor_event_matrix_v4_sha256": sha256(blobs["events"]),
        "i0_regression_gate_v2_sha256": sha256(blobs["regression"]),
    }, "semantic contract bindings")


def fixture_groups(outcomes: dict[str, Any]) -> dict[str, list[str]]:
    groups: dict[str, list[str]] = {}
    for fixture in outcomes["fixtures"]:
        groups.setdefault(fixture["cell_id"], []).append(fixture["fixture_id"])
    return groups


def validate_routes(routes: dict[str, Any], outcomes: dict[str, Any]) -> None:
    require(routes.get("schema_version") == 2 and routes.get("domain") == "moex.stage8b.p1e.latch-route-transition-matrix.v2", "route identity")
    require(routes.get("base_matrix") == {"path": r8.FILES["routes"], "sha256": r8.ROUTES_SHA, "row_count": 30}, "route V1 binding")
    base_cells = {row["cell_id"] for row in json.loads((ROOT / r8.FILES["routes"]).read_bytes())["rows"]}
    mapping = routes.get("proof_fixture_ids_by_cell", {})
    require(routes.get("row_count") == 30 and routes.get("logical_route_count") == 20 and routes.get("proof_case_count") == 46, "route counts")
    require(set(mapping) == base_cells and len(base_cells) == 30, "route cell inventory")
    flattened = [fixture for cell in mapping.values() for fixture in cell]
    require(len(flattened) == len(set(flattened)) == 46 and set(flattened) == {f"FX{number:02d}" for number in range(1, 47)}, "route fixture inventory")
    require(mapping == fixture_groups(outcomes), "route fixture grouping")
    amendments = {item["cell_id"]: item for item in routes.get("exact_row_amendments", [])}
    require(set(amendments) == {"LR02-default", "LR04-default", "LR10-default", "LR13-default", "LT05-due-day-timer"}, "route amendment inventory")
    for cell in ("LR02-default", "LR04-default", "LR10-default", "LR13-default"):
        item = amendments[cell]
        require(item["boundary_kind"] == "redis_reattachment_checkpoint" and "zero-" in item["post_permit_signal_stop_policy"], f"{cell} reattachment stop")
    due = amendments["LT05-due-day-timer"]
    require("pending-branch:one-XACK" in due["xack_legality"] and "already-acknowledged-branch:zero-XACK" in due["xack_legality"], "due source branches")
    require("timer-execution-forbidden" in due["timer_disposition"] and len(due["proof_fixture_ids"]) == 4, "due timer boundary")
    require(routes["event_matrix_precedence"] == {"E25": "route-matrix-boundary-wins-for-an-in-flight-permit", "E05": "normal-loop-owned-S_ack-only-and-never-applies-inside-LR04-LR10-LR13-permit-continuation"}, "E05/E25 precedence")


def validate_outcomes(outcomes: dict[str, Any]) -> None:
    require(outcomes.get("schema_version") == 1 and outcomes.get("domain") == "moex.stage8b.p1e.route-outcome-fixture-matrix.v1", "outcome identity")
    fixtures = outcomes.get("fixtures", [])
    ids = [fixture.get("fixture_id") for fixture in fixtures]
    require(outcomes.get("case_count") == len(fixtures) == len(set(ids)) == 46 and set(ids) == {f"FX{number:02d}" for number in range(1, 47)}, "outcome fixture inventory")
    require(outcomes.get("counter_fields") == COUNTER_FIELDS, "counter field inventory")
    profiles = outcomes.get("effect_profiles", {})
    require(set(profiles) == set(PROFILE_ORACLE), "effect profile inventory")
    for name, expected in PROFILE_ORACLE.items():
        require([profiles[name].get(field) for field in COUNTER_FIELDS] == expected, f"effect profile {name}")
    require({fixture["effect_profile"] for fixture in fixtures} == set(PROFILE_ORACLE), "unused effect profile")
    require(len({fixture["authenticated_fixture"] for fixture in fixtures}) == 46, "authenticated fixtures not unique")
    for fixture in fixtures:
        require(set(fixture) == FIXTURE_FIELDS, f"fixture fields {fixture.get('fixture_id')}")
        require(fixture["expected_boundary"] and fixture["expected_owner_or_disposition"] and fixture["stop_point"], f"fixture exact result {fixture['fixture_id']}")
        require("|" not in fixture["expected_boundary"] and "|" not in fixture["expected_owner_or_disposition"], f"fixture union escaped {fixture['fixture_id']}")
    by_id = {fixture["fixture_id"]: fixture for fixture in fixtures}
    for fixture_id in ("FX02", "FX04", "FX10", "FX23"):
        fixture = by_id[fixture_id]
        require(fixture["effect_profile"] == "ATTACH" and fixture["source_pel_disposition"] == "pending" and "reattachment-checkpoint" in fixture["stop_point"], f"reattachment fixture {fixture_id}")
    expected_due = {
        "FX41": ("DUE_PENDING_RECLASSIFY", "SourceResolvedThenTimerReclassified"),
        "FX42": ("DUE_ALREADY_RECLASSIFY", "SourceResolvedThenTimerReclassified"),
        "FX45": ("DUE_PENDING_STALE", "StaleTimerDiscarded"),
        "FX46": ("DUE_ALREADY_STALE", "StaleTimerDiscarded"),
    }
    for fixture_id, expected in expected_due.items():
        fixture = by_id[fixture_id]
        require((fixture["effect_profile"], fixture["expected_owner_or_disposition"]) == expected, f"due branch {fixture_id}")
        require(profiles[fixture["effect_profile"]]["timer_execution_total"] == 0, f"due execution {fixture_id}")


def validate_timer(timer: dict[str, Any]) -> None:
    require(timer.get("schema_version") == 4 and timer.get("domain") == "moex.stage8b.p1e.source-timer-precedence.v4", "timer identity")
    require(timer.get("base_contract") == {"path": DOCS + "stage8b-p1e-source-timer-precedence-v3.json", "sha256": sha256((ROOT / (DOCS + "stage8b-p1e-source-timer-precedence-v3.json")).read_bytes())}, "timer V3 binding")
    due = timer.get("simultaneous_due_timer", {})
    require(due.get("source_variants") == ["one_exact_pending_source_any_idle_age", "source_already_acknowledged_with_continuous_frontier"], "timer source variants")
    require(due.get("first_transition") == "resolve-terminal-source-without-reclaim" and due.get("first_latch_recheck", "").startswith("if-set-stop"), "timer first checkpoint")
    require(due.get("second_transition_if_clear", "").startswith("reclassify-original-timer") and due.get("second_latch_recheck") == "required-before-any-timer-execution", "timer second checkpoint")
    require(due.get("timer_execution") == "separate-next-owner-loop-step-only-if-latch-clear" and due.get("timer_before_source") == "forbidden", "timer execution separation")
    require(len(timer.get("exact_proof_cases", [])) == 4, "timer proof cases")


def validate_events(blobs: dict[str, bytes]) -> None:
    rows = csv_rows(blobs["events"])
    require(len(rows) == 25 and {row["id"] for row in rows} == {f"E{number:02d}" for number in range(1, 26)}, "event inventory")
    by_id = {row["id"]: row for row in rows}
    require(by_id["E05"]["owner_phase"] == "normal_loop_s_ack_committed_before_shutdown_request", "E05 owner scope")
    require("never_applies_inside_LR04_LR10_LR13" in by_id["E05"]["allowed_next_effect"], "E05 route exclusion")
    require(by_id["E25"]["owner_phase"] == "route_bound_post_decision_continuation", "E25 owner scope")
    require("complete_only_exact_route_matrix_v2_boundary" in by_id["E25"]["allowed_next_effect"] and "never_falls_through_to_s_truth_or_xack" in by_id["E25"]["allowed_next_effect"], "E25 boundary precedence")
    require(by_id["E25"]["source_pel_disposition"].endswith("route_transition_matrix_v2"), "E25 V2 binding")


def validate_regression(regression: dict[str, Any]) -> None:
    require(regression.get("schema_version") == 2 and regression.get("domain") == "moex.stage8b.p1e.i0-regression-gate.v2", "regression identity")
    require(regression.get("entrypoint") == "bash scripts/stage8b_p1e_i0_regression_gate.sh ACCEPTED_R9_COMMIT", "I0 entrypoint")
    ref = regression.get("accepted_r9_ref_contract", {})
    require(ref and all(ref.values()), "accepted R9 ref contract")
    scope = regression.get("current_i0_scope", {})
    require(scope.get("production_allowlist") == PRODUCTION_ALLOWLIST, "I0 production allowlist")
    require(scope.get("helper_prefix_allowlist") == ["docs/stage-8/stage8b-p1e-i0-", "scripts/stage8b_p1e_i0_", "reports/stage8b-p1e-i0-"], "I0 helper allowlist")
    require(scope.get("explicit_shared_files") == ["docs/current-status.md", "docs/roadmap.md"], "I0 shared files")
    require(all(scope.get(key) is False for key in ("cargo_allowed", "workflow_allowed", "config_or_unit_allowed", "deployment_allowed")), "I0 closed scope")
    historical = regression.get("historical_p1d4", {})
    require(historical.get("historical_gate") == "scripts/stage8b_p1d4_source_gate.sh" and historical.get("historical_gate_unchanged") is True and historical.get("historical_gate_invoked_directly_by_i0") is False, "historical gate separation")
    require(historical.get("positive_sigkill_cells") == 105 and historical.get("clean_runs") == 2 and "validate_content" in historical.get("reusable_content_validator", ""), "P1-d4 current-source suite")
    require(sha256((ROOT / "scripts/stage8b_p1d4_source_gate.sh").read_bytes()) == "f2aacabab791870cb5c334a2238dba80748f5b1cd2e377c74f6daf8f566d9e61", "historical P1-d4 gate drift")
    filters = regression.get("cross_slice_test_filters", [])
    require(len(filters) == len(set(filters)) == 6 and all(name.startswith(("p1d2_", "p1d3_")) for name in filters), "cross-slice test filters")
    commands = regression.get("mandatory_commands", [])
    require(len(commands) == 11 and not any(command == "bash scripts/stage8b_p1d4_source_gate.sh" for command in commands), "I0 mandatory commands")
    require("six exact P1-d2/P1-d3 test filters" in commands and "P1-d4 105-cell SIGKILL evidence exactly twice on current I0 source" in commands, "I0 suite obligations")
    require(set(regression.get("closed_surfaces", [])) == {"operational-DB0", "VPS-activation", "FINAM-POST-DELETE", "broker-dispatch", "runtime-live", "real-orders"}, "regression closed surfaces")


def normalized_model(blobs: dict[str, bytes]) -> dict[str, Any]:
    routes = load_json(blobs, "routes")
    outcomes = load_json(blobs, "outcomes")
    timer = load_json(blobs, "timer")
    regression = load_json(blobs, "regression")
    events = {row["id"]: row for row in csv_rows(blobs["events"])}
    return {
        "route_counts": [routes["row_count"], routes["logical_route_count"], routes["proof_case_count"]],
        "route_boundaries": routes["boundary_kinds"],
        "route_signal_precedence": routes["signal_precedence"],
        "route_amendments": sorted(routes["exact_row_amendments"], key=lambda item: item["cell_id"]),
        "route_fixture_bindings": routes["proof_fixture_ids_by_cell"],
        "route_event_precedence": routes["event_matrix_precedence"],
        "counter_fields": outcomes["counter_fields"],
        "effect_profiles": outcomes["effect_profiles"],
        "fixtures": sorted(outcomes["fixtures"], key=lambda item: item["fixture_id"]),
        "timer": timer,
        "events": {key: events[key] for key in ("E05", "E18", "E25")},
        "regression": regression,
    }


def normalized_sha(blobs: dict[str, bytes]) -> str:
    encoded = json.dumps(normalized_model(blobs), sort_keys=True, separators=(",", ":")).encode()
    return sha256(encoded)


def validate_evidence(blobs: dict[str, bytes], evidence: dict[str, Any]) -> None:
    require(evidence.get("status") == "R9_DESIGN_REVIEW_CANDIDATE" and evidence.get("parent") == BASE and evidence.get("accepted_predecessor") == ACCEPTED, "evidence lineage")
    require(evidence.get("r8_review_sha256") == R8_REVIEW_SHA256, "R8 review binding")
    require(evidence.get("design_only") is True and evidence.get("source_modified") is False and evidence.get("i0_source_seam_authorized_now") is False, "evidence design boundary")
    require(evidence.get("i0_source_seam_authorized_after_independent_acceptance") is True and evidence.get("supervisor_source_implementation_authorized") is False, "evidence authorization")
    require(tuple(evidence.get(key) for key in (
        "base_v8_active_rows", "superseded_v8_rows", "r9_rows", "active_rows",
        "semantic_authority_keys", "route_transition_rows", "route_outcome_fixtures",
        "supervisor_event_rows", "i0_production_allowlist_files",
        "p1d4_positive_sigkill_cells", "p1d4_clean_runs", "p1d2_p1d3_exact_filters",
        "negative_cases", "integrity_negative_cases", "semantic_negative_cases",
    )) == (279, 5, 24, 298, 34, 30, 46, 25, 3, 105, 2, 6, 36, 8, 28), "evidence counts")
    require(evidence.get("accepted_source_baseline_sha256") == SOURCE_SHA and sha256(blobs["source"]) == SOURCE_SHA, "source baseline")
    require(all(value is False for value in evidence.get("closed_surfaces", {}).values()), "closed surface opened")
    require(set(evidence.get("contract_sha256", {})) == set(INTEGRITY_SHA), "evidence hash inventory")
    for name in INTEGRITY_SHA:
        require(evidence["contract_sha256"][name] == sha256(blobs[name]), f"evidence hash {name}")


def validate(overrides: dict[str, bytes] | None = None, check_scope: bool = False, check_integrity: bool = True) -> None:
    r8.validate({
        r8.FILES["status"]: subprocess.check_output(["git", "show", f"{BASE}:{r8.FILES['status']}"], cwd=ROOT),
        r8.FILES["roadmap"]: subprocess.check_output(["git", "show", f"{BASE}:{r8.FILES['roadmap']}"], cwd=ROOT),
    })
    blobs = read_all(overrides)
    if check_integrity:
        for name, expected in INTEGRITY_SHA.items():
            require(sha256(blobs[name]) == expected, f"integrity digest {name}")
        require(sha256(blobs["evidence"]) == EVIDENCE_SHA, "integrity digest evidence")
    validate_active(blobs, load_json(blobs, "active"))
    validate_semantic(blobs, load_json(blobs, "semantic"))
    outcomes = load_json(blobs, "outcomes")
    validate_routes(load_json(blobs, "routes"), outcomes)
    validate_outcomes(outcomes)
    validate_timer(load_json(blobs, "timer"))
    validate_events(blobs)
    validate_regression(load_json(blobs, "regression"))
    validate_evidence(blobs, load_json(blobs, "evidence"))
    require(normalized_sha(blobs) == NORMALIZED_MODEL_SHA256, "normalized semantic model")
    design = blobs["design"].decode("utf-8")
    require("Reattachment checkpoint is not a durable seal" in design and "exact set of 46 authenticated fixtures" in design, "design boundary text")
    require("historical diff scope is intentionally non-reusable" in design and "105 SIGKILL cells in two clean runs" in design, "design regression text")
    require("P1-e R9 is the active narrow design/checker candidate" in blobs["status"].decode(), "status R9")
    require("R9 is the active design-only correction before I0" in blobs["roadmap"].decode(), "roadmap R9")
    if check_scope:
        actual = changed_files()
        require(actual == EXPECTED_CHANGED, f"scope mismatch missing={sorted(EXPECTED_CHANGED-actual)} extra={sorted(actual-EXPECTED_CHANGED)}")


def main() -> None:
    try:
        validate(check_scope=True)
    except (CheckFailure, r8.CheckFailure, r8.r7.CheckFailure, r8.r7.r6.CheckFailure, OSError, subprocess.CalledProcessError, ValueError, KeyError, TypeError, json.JSONDecodeError) as error:
        print(f"stage8b-p1e-r9-design-check: FAIL {error}")
        raise SystemExit(1)
    print("PASS stage8b-p1e-r9-design-scope files=20 production_rust=0 cargo=0 workflow=0 config=0")
    print("PASS stage8b-p1e-r9-active-contract rows=298 base=279 superseded=5 r9=24")
    print("PASS stage8b-p1e-r9-semantic-authority keys=34 conflicts=0")
    print("PASS stage8b-p1e-r9-route-outcomes cells=30 fixtures=46 reattachments=4 exact_counters=7")
    print("PASS stage8b-p1e-r9-source-timer pending=true already_acknowledged=true latch_checks=2 timer_execution=later")
    print("PASS stage8b-p1e-r9-i0-regression historical_scope_gate=false p1d4_sigkill=105x2 p1d2_p1d3=6")
    print(f"PASS stage8b-p1e-r9-normalized-semantic-oracle sha256={NORMALIZED_MODEL_SHA256}")


if __name__ == "__main__":
    main()
