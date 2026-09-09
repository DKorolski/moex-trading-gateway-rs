#!/usr/bin/env python3
"""Fail-closed cross-contract checker for Stage 8B-P1-e R7 design."""

from __future__ import annotations

import csv
import hashlib
import io
import json
import pathlib
import subprocess
from typing import Any

import stage8b_p1e_r6_design_check as r6


ROOT = pathlib.Path(__file__).resolve().parents[1]
BASE = "c0d7ee4c10e4d060fd15ea31f4593adcb793642b"
ACCEPTED = "c2a9e1246dfdd59f3a6297268de907dedcb19903"
R6_REVIEW_SHA256 = "8eb5d682927105bf6505a85ffdc249a281716adfaac5fc9e179c2d58ed4ed74b"
SOURCE_SHA = "a870fbbb6ec9fc60b7df9c35a2aca7a5daf81f695b019352d7f20c9b51439d16"
DOCS = "docs/stage-8/"
FILES = {
    "design": DOCS + "stage8b-p1e-deployable-supervisor-design-r7.md",
    "acceptance": DOCS + "stage8b-p1e-deployable-supervisor-r7-acceptance-matrix.csv",
    "active": DOCS + "stage8b-p1e-active-acceptance-contract-v7.json",
    "semantic": DOCS + "stage8b-p1e-semantic-authority-registry-v7.json",
    "seam": DOCS + "stage8b-p1e-latch-aware-source-seam-v1.json",
    "events": DOCS + "stage8b-p1e-supervisor-event-matrix-v2.csv",
    "tests": DOCS + "stage8b-p1e-latch-race-test-matrix-v1.json",
    "evidence": DOCS + "stage8b-p1e-deployable-supervisor-r7-design-evidence.json",
    "source": "crates/runtime-durable-service/src/stage8b_p1_semantic/redis.rs",
    "status": "docs/current-status.md",
    "roadmap": "docs/roadmap.md",
}
EXPECTED_CHANGED = {FILES[name] for name in (
    "design", "acceptance", "active", "semantic", "seam", "events",
    "tests", "evidence", "status", "roadmap",
)} | {
    "scripts/stage8b_p1e_r7_design_check.py",
    "scripts/stage8b_p1e_r7_design_negative_harness.py",
    "scripts/stage8b_p1e_r7_design_gate.sh",
    "scripts/stage8b_p1e_r7_design_handoff_safety_check.py",
    "scripts/make_stage8b_p1e_r7_design_handoff.py",
}
R7_ACCEPTANCE_SHA = "46dff914d5203f908d8b022b92a56688c171293e708f86928cea53fcae04eb34"
SUPERSEDED = {
    "P1ER6-002", "P1ER6-003", "P1ER6-004", "P1ER6-005", "P1ER6-006",
    "P1ER6-010", "P1ER6-013", "P1ER6-014", "P1ER6-016", "P1ER6-018",
    "P1ER6-030",
}
REPLACED_SEMANTICS = {
    "redis.ReclaimRequiredContinuation": "fifteen-route-exact-acquire-functions-return-linear-owner-before-permit-only-continuation-and-no-second-acquisition",
    "redis.TerminalSourceResolution": "five-logical-exact-lookup-routes-return-linear-pre-resolution-owner-before-permit-only-XACK-or-AlreadyAcknowledged",
    "shutdown.PostDeliveryLatch": "consuming-linear-owner-decision-Continue-or-RetainForRestart-with-post-clear-bounded-drain-linearization",
}
NEW_SEMANTICS = {
    "redis.PostAcquisitionLinearOwner": "opaque-private-nonclone-noncopy-nonserde-owner-of-durable-phase-transport-delivery-route-and-acquisition-kind",
    "shutdown.ContinuationLinearization": "no-select-cancellation-before-or-after-permit-and-next-covering-boundary-recheck-after-post-decision-signal",
}
ALLOWED_I0 = [
    "crates/runtime-durable-service/src/stage8b_p1_semantic/redis.rs",
    "crates/runtime-durable-service/src/stage8b_p1_semantic.rs",
    "crates/runtime-durable-service/src/lib.rs",
]


class CheckFailure(RuntimeError):
    pass


def require(value: bool, message: str) -> None:
    if not value:
        raise CheckFailure(message)


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def csv_rows(data: bytes) -> list[dict[str, str]]:
    return list(csv.DictReader(io.StringIO(data.decode("utf-8"))))


def read_all(overrides: dict[str, bytes] | None = None) -> dict[str, bytes]:
    overrides = overrides or {}
    return {name: overrides.get(path, (ROOT / path).read_bytes()) for name, path in FILES.items()}


def load_json(blobs: dict[str, bytes], name: str) -> dict[str, Any]:
    value = json.loads(blobs[name])
    require(isinstance(value, dict), f"{name} must be object")
    return value


def changed_files() -> set[str]:
    tracked = subprocess.run(
        ["git", "diff", "--name-only", BASE], cwd=ROOT, check=True,
        text=True, capture_output=True,
    ).stdout.splitlines()
    untracked = subprocess.run(
        ["git", "ls-files", "--others", "--exclude-standard"], cwd=ROOT,
        check=True, text=True, capture_output=True,
    ).stdout.splitlines()
    return set(tracked) | set(untracked)


def v6_active_ids() -> set[str]:
    base = r6.base_v5_active_ids()
    rows = csv_rows((ROOT / r6.FILES["acceptance"]).read_bytes())
    result = (base - r6.SUPERSEDED) | {row["id"] for row in rows}
    require(len(result) == 242, "V6 active source inventory")
    return result


def v6_semantics() -> dict[str, str]:
    registry = json.loads((ROOT / r6.FILES["semantic"]).read_bytes())
    base_path = registry["base_registry"]["path"]
    base = json.loads((ROOT / base_path).read_bytes())["expected_active_values"]
    result = dict(base)
    result.update(registry["replacement_active_authorities"])
    result.update(registry["new_active_authorities"])
    require(len(result) == 26, "V6 semantic source inventory")
    return result


def validate_active(blobs: dict[str, bytes], active: dict[str, Any]) -> None:
    require(active.get("schema_version") == 7 and active.get("domain") == "moex.stage8b.p1e.active-acceptance-contract.v7", "active identity")
    require(active.get("direct_parent") == BASE and active.get("accepted_predecessor") == ACCEPTED, "active lineage")
    base = active["base_active_contract"]
    base_data = (ROOT / base["path"]).read_bytes()
    require(base == {"path": r6.FILES["active"], "sha256": sha256(base_data), "active_rows": 242}, "active V6 binding")
    rows = csv_rows(blobs["acceptance"])
    ids = {row["id"] for row in rows}
    require(len(rows) == len(ids) == 34 and ids == {f"P1ER7-{n:03d}" for n in range(1, 35)}, "R7 matrix inventory")
    require(all(row["status"] == "REQUIRED" for row in rows), "R7 optional row")
    source = active["r7_source"]
    require(source == {"path": FILES["acceptance"], "sha256": sha256(blobs["acceptance"]), "row_count": 34, "superseded_rows": []}, "R7 source binding")
    require(source["sha256"] == R7_ACCEPTANCE_SHA, "R7 acceptance digest")
    base_ids = v6_active_ids()
    require(set(active["superseded_base_rows"]) == SUPERSEDED <= base_ids, "R7 superseded rows")
    mapped: list[str] = []
    final_ids = (base_ids - SUPERSEDED) | ids
    for item in active["supersession_map"]:
        require(item["superseded"] and item["replacement"], "empty R7 supersession")
        require(set(item["replacement"]) <= final_ids, "inactive R7 replacement")
        mapped.extend(item["superseded"])
    require(len(mapped) == len(set(mapped)) and set(mapped) == SUPERSEDED, "R7 supersession exactness")
    require(len(final_ids) == 265 and active["active_contract_expectation"] == {
        "base_v6_active_rows": 242, "base_rows_superseded_by_r7": 11,
        "r7_active_rows": 34, "total_active_rows": 265,
        "all_status_required": True,
    }, "R7 active expectation")


def validate_semantic(blobs: dict[str, bytes], semantic: dict[str, Any]) -> None:
    require(semantic.get("schema_version") == 7 and semantic.get("domain") == "moex.stage8b.p1e.semantic-authority-registry.v7", "semantic identity")
    base = semantic["base_registry"]
    base_data = (ROOT / base["path"]).read_bytes()
    require(base == {"path": r6.FILES["semantic"], "sha256": sha256(base_data), "active_key_count": 26}, "semantic V6 binding")
    old = v6_semantics()
    require(semantic["superseded_active_authorities"] == {key: old[key] for key in REPLACED_SEMANTICS}, "semantic superseded values")
    require(semantic["replacement_active_authorities"] == REPLACED_SEMANTICS, "semantic replacements")
    require(semantic["required_new_keys"] == list(NEW_SEMANTICS), "semantic new key order")
    require(semantic["new_active_authorities"] == NEW_SEMANTICS, "semantic new values")
    composed = dict(old)
    composed.update(REPLACED_SEMANTICS)
    require(not set(NEW_SEMANTICS) & set(composed), "semantic new-key collision")
    composed.update(NEW_SEMANTICS)
    require(len(composed) == 28 and semantic["composed_expectation"] == {
        "retained_base_keys": 23, "replacement_keys": 3, "new_keys": 2,
        "total_active_keys": 28, "active_conflicts": 0,
    }, "semantic composition")
    expected = {
        "latch_aware_source_seam_v1_sha256": sha256(blobs["seam"]),
        "supervisor_event_matrix_v2_sha256": sha256(blobs["events"]),
        "latch_race_test_matrix_v1_sha256": sha256(blobs["tests"]),
        "acquisition_model_v3_sha256": sha256((ROOT / r6.FILES["acquisition"]).read_bytes()),
        "operational_overlay_v6_sha256": sha256((ROOT / r6.FILES["operational"]).read_bytes()),
        "first_boot_transaction_v5_sha256": sha256((ROOT / r6.FILES["transaction"]).read_bytes()),
    }
    require(semantic["contract_bindings"] == expected, "semantic contract bindings")


def validate_current_source(blobs: dict[str, bytes], seam: dict[str, Any]) -> None:
    require(sha256(blobs["source"]) == SOURCE_SHA, "current source baseline digest")
    baseline = seam["current_source_baseline"]
    require(baseline == {
        "path": FILES["source"], "sha256": SOURCE_SHA,
        "state": "monolithic-pre-seam", "modified_in_r7_design": False,
    }, "current source baseline contract")
    source = blobs["source"].decode("utf-8")
    zero = r6.function_body(source, "resolve_stage8b_p1_zero_intent_ack_with_redis")
    require(zero.index(".exact_delivery_for_evidence(") < zero.index(".acknowledge_exact("), "current zero-intent mismatch oracle")
    journal = r6.function_body(source, "resume_stage8b_p1_journal_ahead_with_redis")
    require(journal.index(".reclaim_single_pending(") < journal.index(".parse_exact("), "current reclaim/parse mismatch oracle")
    dispatch = r6.function_body(source, "resume_stage8b_p1d3_dispatch_limit_with_redis")
    require(dispatch.index(".reclaim_exact_evidence(") < dispatch.index("Stage8bP1d3InitialObservation::Candidate"), "current reclaim/provider mismatch oracle")
    semantic = r6.function_body(source, "resume_stage8b_p1d3_semantic_with_redis")
    require(semantic.index(".reclaim_exact_binding(") < semantic.index("complete_stage8b_p1d3_semantic("), "current reclaim/callback mismatch oracle")
    generated_truth = r6.function_body(source, "resume_stage8b_p1d4_truth_with_redis")
    require(generated_truth.index(".exact_delivery_for_evidence(") < generated_truth.index(".revalidate_p1d4_publication("), "current lookup/revalidation mismatch oracle")


def validate_seam(blobs: dict[str, bytes], seam: dict[str, Any]) -> None:
    require(seam.get("schema_version") == 1 and seam.get("domain") == "moex.stage8b.p1e.latch-aware-source-seam.v1", "seam identity")
    require(seam["selected_option"] == "A-typed-linear-post-acquisition-owner", "seam selected option")
    validate_current_source(blobs, seam)
    auth = seam["i0_authorization_after_independent_r7_acceptance"]
    require(auth == {
        "authorized": True, "allowed_production_files": ALLOWED_I0,
        "cargo_change_allowed": False, "workflow_change_allowed": False,
        "supervisor_binary_allowed": False, "deployment_or_activation_allowed": False,
    }, "I0 source allowlist")
    types = seam["opaque_protocol_types"]
    require(types["acquired_owner"] == "Stage8bP1ePostAcquisitionOwnerV1" and types["continue_permit"] == "Stage8bP1eContinuationPermitV1", "linear type names")
    require(types["decision"] == "Stage8bP1ePostAcquisitionDecisionV1" and types["retained_receipt"] == "Stage8bP1eRetainedSourceReceiptV1", "decision type names")
    require(types["private_fields"] is True and types["private_inner_route_enum"] is True, "owner privacy")
    require(all(types[key] is False for key in ("clone", "copy", "serialize", "deserialize", "reconstruct", "split", "payload_or_transport_getter")), "owner linear traits")
    require(set(types["acquired_owner_owns"]) == {
        "exact-durable-phase-owner", "Stage8bP1RedisSemanticCompositionTransport",
        "Stage8bP1PendingM10Delivery", "authenticated-logical-route", "acquisition-kind",
    }, "owner field authority")
    latch = seam["latch_protocol"]
    require(latch["decision_function"] == "decide_stage8b_p1e_post_acquisition_latch" and latch["input_consumed"] == types["acquired_owner"], "latch API")
    require(latch["set_result"].startswith("RetainForRestart(") and latch["clear_result"].startswith("Continue("), "latch decisions")
    require(latch["retain_receipt_contains_payload_or_authority"] is False and latch["continuation_permit_route_bound"] is True and latch["continuation_permit_single_use"] is True, "latch ownership")
    require(latch["acquisition_future_select_cancellation_allowed"] is False and latch["continuation_future_select_cancellation_allowed"] is False, "select cancellation")
    require(latch["post_clear_linearization"] == "bounded-drain-to-exact-next-authenticated-covering-boundary-then-recheck-latch", "post-clear linearization")
    require({"acknowledge_exact", "parse_exact", "provider", "schedule", "revalidate_p1d4_publication", "XACK"} <= set(seam["acquisition_body_forbidden_after_delivery"]), "acquire forbidden effects")
    require({"reclaim_single_pending", "reclaim_exact_evidence", "reclaim_exact_binding", "exact_delivery_for_evidence", "exact_delivery_for_binding", "XREADGROUP", "XAUTOCLAIM", "XPENDING"} == set(seam["continuation_body_forbidden_acquisition"]), "continuation acquisition ban")
    require(seam["legacy_wrapper_rule"] == "all-current-owner-plus-transport-signatures-replaced-by-route-bound-continuation-permit-signatures", "legacy wrapper closure")
    require(seam["zero_intent_order"] == [
        "exact_delivery_for_evidence", "Stage8bP1ePostAcquisitionOwnerV1",
        "decide_stage8b_p1e_post_acquisition_latch", "Stage8bP1eContinuationPermitV1",
        "acknowledge_exact",
    ], "zero-intent split")
    acquisition = json.loads((ROOT / r6.FILES["acquisition"]).read_bytes())
    reclaim = seam["reclaim_required_routes"]
    terminal = seam["terminal_routes"]
    require(len(reclaim) == 15 and len(terminal) == 5, "seam route inventory")
    require([(x["owner"], x["phase"]) for x in reclaim] == [(x["owner"], x["phase"]) for x in acquisition["reclaim_required_semantic_continuations"]], "reclaim route binding")
    require([(x["owner"], x["phase"]) for x in terminal] == [(x["owner"], x["phase"]) for x in acquisition["terminal_source_resolution_continuations"]], "terminal route binding")
    for row in reclaim + terminal:
        require(row["acquire"] and row["continue"] and len(row["acquire"]) == len(row["continue"]), "route physical entrypoints")
        for acquire, continuation in zip(row["acquire"], row["continue"]):
            expected = continuation.replace("resume_", "acquire_", 1).replace("resolve_", "acquire_", 1)
            require(acquire == expected, f"route acquire naming {acquire}")
    acquire_names = {name for row in reclaim + terminal for name in row["acquire"]}
    continue_names = {name for row in reclaim + terminal for name in row["continue"]}
    require(len(acquire_names) == len(continue_names) == 21, "physical route counts")
    source = blobs["source"].decode("utf-8")
    require(all(source.count(name) >= 1 for name in continue_names), "current continuation inventory")
    require(all(name not in source for name in acquire_names), "current seam unexpectedly present")
    require(seam["route_counts"] == {"reclaim_logical": 15, "terminal_logical": 5, "physical_acquire_entrypoints": 21, "physical_continuation_entrypoints": 21}, "route count contract")
    obligations = set(seam["future_i0_source_checker_must_prove"])
    require(len(obligations) == 8 and "all-20-logical-route-latch-race-tests" in obligations and "no-select-race-around-linear-futures" in obligations, "future source checker obligations")


def validate_events(blobs: dict[str, bytes]) -> None:
    base_rows = csv_rows((ROOT / "docs/stage-8/stage8b-p1e-supervisor-event-matrix-v1.csv").read_bytes())
    rows = csv_rows(blobs["events"])
    require(len(rows) == 25 and {row["id"] for row in rows} == {f"E{n:02d}" for n in range(1, 26)}, "event inventory")
    old = {row["id"]: row for row in base_rows}
    new = {row["id"]: row for row in rows}
    for key in old:
        if key not in {"E03", "E18"}:
            require(new[key] == old[key], f"event drift {key}")
    require(new["E03"]["allowed_next_effect"].startswith("set_monotonic_latch_and_await_non_cancellable_acquisition"), "E03 noncancellable acquisition")
    require(new["E18"]["owner_availability"] == "linear_owner" and "RetainForRestart" in new["E18"]["allowed_next_effect"] and "zero_parse" in new["E18"]["allowed_next_effect"], "E18 linear retain")
    require(new["E25"]["owner_availability"] == "continuation_permit" and "do_not_cancel_permit" in new["E25"]["allowed_next_effect"] and "recheck_latch" in new["E25"]["allowed_next_effect"], "E25 post-decision race")


def validate_tests(tests: dict[str, Any], seam: dict[str, Any]) -> None:
    require(tests.get("schema_version") == 1 and tests.get("domain") == "moex.stage8b.p1e.latch-race-test-matrix.v1", "test matrix identity")
    routes = tests["routes"]
    require(len(routes) == 20 and {row["id"] for row in routes} == ({f"LR{n:02d}" for n in range(1, 16)} | {f"LT{n:02d}" for n in range(1, 6)}), "test route IDs")
    expected = {(row["owner"], row["phase"]) for row in seam["reclaim_required_routes"] + seam["terminal_routes"]}
    require({(row["owner"], row["phase"]) for row in routes} == expected, "test route coverage")
    require(sum(row["class"] == "reclaim" for row in routes) == 15 and sum(row["class"] == "terminal" for row in routes) == 5, "test class counts")
    dispatch = next(row for row in routes if row["id"] == "LR11")
    require(dispatch["operation_variants"] == ["limit", "expiry", "cancel"], "dispatch operation coverage")
    pre = tests["common_pre_set_latch_assertions"]
    require(pre["decision"] == "RetainForRestart" and all(pre[key] == 0 for key in pre if key.endswith("_total")), "pre-set latch assertions")
    clear = tests["common_clear_latch_assertions"]
    require(clear["decision"] == "Continue" and clear["permit_total"] == clear["permit_consumption_total"] == 1, "clear latch assertions")
    compile_fail = set(tests["required_api_and_compile_fail_tests"])
    negatives = set(tests["required_source_body_negatives"])
    require(len(compile_fail) == 8 and "legacy-owner-plus-transport-wrapper-call-does-not-compile" in compile_fail, "compile-fail inventory")
    require(negatives == {
        "direct-terminal-lookup-to-XACK", "reclaim-to-parse-before-latch",
        "reclaim-to-provider-before-latch", "reclaim-to-schedule-before-latch",
        "reclaim-to-callback-before-latch", "p1d4-revalidation-before-latch",
        "external-select-cancels-acquisition-owner",
        "external-select-cancels-continuation-permit",
        "second-source-acquisition-after-owner",
        "old-wrapper-owner-plus-transport-bypass",
    }, "source negative inventory")
    require(tests["route_counts"] == {"reclaim": 15, "terminal": 5, "total": 20}, "test count contract")


def validate_evidence(blobs: dict[str, bytes], evidence: dict[str, Any]) -> None:
    require(evidence.get("status") == "R7_DESIGN_REVIEW_CANDIDATE" and evidence.get("parent") == BASE and evidence.get("accepted_predecessor") == ACCEPTED, "evidence lineage")
    require(evidence.get("r6_review_sha256") == R6_REVIEW_SHA256, "R6 review binding")
    require(evidence.get("design_only") is True and evidence.get("source_modified") is False and evidence.get("i0_source_seam_authorized_now") is False, "evidence design boundary")
    require(evidence.get("i0_source_seam_authorized_after_independent_acceptance") is True and evidence.get("supervisor_source_implementation_authorized") is False, "evidence authorization boundary")
    require(tuple(evidence.get(key) for key in (
        "base_v6_active_rows", "superseded_v6_rows", "r7_rows", "active_rows",
        "semantic_authority_keys", "reclaim_required_routes", "terminal_logical_routes",
        "latch_race_routes", "supervisor_event_rows", "operational_composed_rows_inherited",
        "negative_cases",
    )) == (242, 11, 34, 265, 28, 15, 5, 20, 25, 51, 32), "evidence counts")
    require(evidence.get("accepted_source_baseline_sha256") == SOURCE_SHA, "evidence source baseline")
    require(all(value is False for value in evidence["closed_surfaces"].values()), "closed surface opened")
    expected = {"design", "acceptance", "active", "semantic", "seam", "events", "tests"}
    require(set(evidence["contract_sha256"]) == expected, "evidence hash inventory")
    for name in expected:
        require(evidence["contract_sha256"][name] == sha256(blobs[name]), f"evidence hash {name}")


def validate(overrides: dict[str, bytes] | None = None, check_scope: bool = False) -> None:
    r6.validate()
    blobs = read_all(overrides)
    active = load_json(blobs, "active")
    semantic = load_json(blobs, "semantic")
    seam = load_json(blobs, "seam")
    tests = load_json(blobs, "tests")
    evidence = load_json(blobs, "evidence")
    validate_active(blobs, active)
    validate_semantic(blobs, semantic)
    validate_seam(blobs, seam)
    validate_events(blobs)
    validate_tests(tests, seam)
    validate_evidence(blobs, evidence)
    design = blobs["design"].decode("utf-8")
    require("Status: design-only review candidate" in design and "selects review option A" in design, "design status/model")
    require("old owner-plus-transport signatures are removed" in design and "R7 itself is design-only" in design, "design bypass/scope")
    require("P1-e R7 latch-aware source-seam design candidate" in blobs["status"].decode(), "status R7")
    require("P1-e R7 is the\nactive design candidate" in blobs["roadmap"].decode(), "roadmap R7")
    if check_scope:
        actual = changed_files()
        require(actual == EXPECTED_CHANGED, f"scope mismatch missing={sorted(EXPECTED_CHANGED-actual)} extra={sorted(actual-EXPECTED_CHANGED)}")


def main() -> None:
    try:
        validate(check_scope=True)
    except (CheckFailure, r6.CheckFailure, r6.r5.CheckFailure, r6.r5.r4.CheckFailure, OSError, ValueError, KeyError, TypeError, json.JSONDecodeError) as error:
        print(f"stage8b-p1e-r7-design-check: FAIL {error}")
        raise SystemExit(1)
    print("PASS stage8b-p1e-r7-design-scope files=15 production_rust=0 cargo=0 workflow=0 active_unit=0")
    print("PASS stage8b-p1e-r7-active-contract rows=265 base=242 superseded=11 r7=34")
    print("PASS stage8b-p1e-r7-semantic-authority keys=28 conflicts=0")
    print("PASS stage8b-p1e-r7-source-baseline monolithic=true sha256=" + SOURCE_SHA)
    print("PASS stage8b-p1e-r7-latch-seam option=A reclaim=15 terminal=5 physical_entrypoints=21")
    print("PASS stage8b-p1e-r7-latch-race routes=20 events=25 E18=retain E25=bounded-drain")


if __name__ == "__main__":
    main()
