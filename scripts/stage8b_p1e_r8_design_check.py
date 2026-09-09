#!/usr/bin/env python3
"""Fail-closed cross-contract checker for Stage 8B-P1-e R8 design."""

from __future__ import annotations

import csv
import hashlib
import io
import json
import pathlib
import subprocess
from typing import Any

import stage8b_p1e_r7_design_check as r7


ROOT = pathlib.Path(__file__).resolve().parents[1]
BASE = "7ebdcef45c1c55f4783bd6b2b1502ea78d4d97d7"
ACCEPTED = "c2a9e1246dfdd59f3a6297268de907dedcb19903"
R7_REVIEW_SHA256 = "ec2fcaa9cb43813421109f6d1ab46200228bdf4cecc81cfe84fd733f941e4f35"
SOURCE_SHA = r7.SOURCE_SHA
DOCS = "docs/stage-8/"
FILES = {
    "design": DOCS + "stage8b-p1e-deployable-supervisor-design-r8.md",
    "acceptance": DOCS + "stage8b-p1e-deployable-supervisor-r8-acceptance-matrix.csv",
    "active": DOCS + "stage8b-p1e-active-acceptance-contract-v8.json",
    "semantic": DOCS + "stage8b-p1e-semantic-authority-registry-v8.json",
    "seam": DOCS + "stage8b-p1e-latch-aware-source-seam-v2.json",
    "events": DOCS + "stage8b-p1e-supervisor-event-matrix-v3.csv",
    "routes": DOCS + "stage8b-p1e-latch-route-transition-matrix-v1.json",
    "shutdown": DOCS + "stage8b-p1e-shutdown-intent-v1.json",
    "regression": DOCS + "stage8b-p1e-i0-regression-gate-v1.json",
    "evidence": DOCS + "stage8b-p1e-deployable-supervisor-r8-design-evidence.json",
    "source": "crates/runtime-durable-service/src/stage8b_p1_semantic/redis.rs",
    "status": "docs/current-status.md",
    "roadmap": "docs/roadmap.md",
}
EXPECTED_CHANGED = {FILES[name] for name in (
    "design", "acceptance", "active", "semantic", "seam", "events", "routes",
    "shutdown", "regression", "evidence", "status", "roadmap",
)} | {
    "scripts/stage8b_p1e_r8_design_check.py",
    "scripts/stage8b_p1e_r8_design_negative_harness.py",
    "scripts/stage8b_p1e_r8_design_gate.sh",
    "scripts/stage8b_p1e_r8_design_handoff_safety_check.py",
    "scripts/make_stage8b_p1e_r8_design_handoff.py",
}
ACCEPTANCE_SHA = "1e466f9b0b2c882a1aea0daf8dfdd5aa37e794db96427192053627cb88e380ba"
DESIGN_SHA = "0abea17375838b9c04407ea3ea2f87ead74026bbd384bbfff3433f2f0eb78c70"
ROUTES_SHA = "2c467f2ec214b6e636448d210c6bc98b587527f2a8b6ca686ed237860b59c636"
SEAM_SHA = "b96e0cb3674cfa358255c60ef4ab1c8f5d7e6cc9a14a52a80f4fb4109999f94e"
EVENTS_SHA = "c2514a9a1ea31ae7f50db108366f816dc37a1219ba7b0251344ef62dc61ab3a9"
SHUTDOWN_SHA = "5648347fada16ec2a4ea47fd7e5dd48cbf8967ee7354de6c963bdb10cd11ace9"
REGRESSION_SHA = "ef3780bc52695d3c2e9bfb3ecbb854faf6717c071aa449ab0f637220fd467062"
SUPERSEDED = {
    "P1ER7-002", "P1ER7-003", "P1ER7-004", "P1ER7-005", "P1ER7-006",
    "P1ER7-012", "P1ER7-020", "P1ER7-021", "P1ER7-022", "P1ER7-025",
    "P1ER7-027", "P1ER7-028", "P1ER7-030",
}
REPLACED_SEMANTICS = {
    "redis.TerminalSourceResolution": "five-logical-eleven-variant-exact-lookup-routes-use-one-post-permit-read-only-resolution-verification-and-optional-exact-XACK-with-no-second-delivery-acquisition",
    "shutdown.ContinuationLinearization": "no-select-cancellation-and-thirty-row-exact-next-covering-boundary-before-cause-preserving-latch-recheck",
}
NEW_SEMANTICS = {
    "redis.TerminalResolutionObservation": "post-permit-XINFO-XRANGE-XPENDING-optional-XACK-and-p1d4-revalidation-are-resolution-not-acquisition",
    "shutdown.CauseCarryingIntent": "first-cause-final-exit-class-grace-deadline-and-first-request-sequence-are-monotonic-through-E18-E25",
    "test.RouteVariantBoundaryCoverage": "thirty-exact-route-variant-rows-each-bind-next-boundary-returned-owner-PEL-XACK-timer-and-latch-recheck",
}
ALLOWED_I0 = [
    "crates/runtime-durable-service/src/stage8b_p1_semantic/redis.rs",
    "crates/runtime-durable-service/src/stage8b_p1_semantic.rs",
    "crates/runtime-durable-service/src/lib.rs",
]
REQUIRED_ROUTE_FIELDS = {
    "cell_id", "logical_route_id", "class", "owner", "package_phase",
    "operation_variant", "acquire_entrypoint", "acquisition_redis_sequence",
    "permit_continuation_entrypoint", "continuation_inputs",
    "first_post_permit_effect", "exact_next_covering_boundary",
    "exact_returned_owner_or_disposition", "source_pel_disposition",
    "xack_legality", "timer_disposition", "post_boundary_latch_recheck", "test_id",
}
VARIANTS = {
    "LR01": ["default"], "LR02": ["default"], "LR03": ["default"],
    "LR04": ["default"], "LR05": ["default"], "LR06": ["default"],
    "LR07": ["default"], "LR08": ["default"], "LR09": ["default"],
    "LR10": ["default"], "LR11": ["limit", "expiry", "cancel"],
    "LR12": ["command-source", "candidate-source"], "LR13": ["default"],
    "LR14": ["default"], "LR15": ["s_eval", "s_terminal"],
    "LT01": ["pending", "already-acknowledged"],
    "LT02": ["pending", "already-acknowledged"],
    "LT03": ["pending", "already-acknowledged"],
    "LT04": ["pending", "already-acknowledged"],
    "LT05": ["pending", "already-acknowledged", "due-day-timer"],
}


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


def v7_active_ids() -> set[str]:
    result = (r7.v6_active_ids() - r7.SUPERSEDED) | {f"P1ER7-{n:03d}" for n in range(1, 35)}
    require(len(result) == 265, "V7 active source inventory")
    return result


def v7_semantics() -> dict[str, str]:
    result = r7.v6_semantics()
    result.update(r7.REPLACED_SEMANTICS)
    result.update(r7.NEW_SEMANTICS)
    require(len(result) == 28, "V7 semantic source inventory")
    return result


def validate_active(blobs: dict[str, bytes], active: dict[str, Any]) -> None:
    require(active.get("schema_version") == 8 and active.get("domain") == "moex.stage8b.p1e.active-acceptance-contract.v8", "active identity")
    require(active.get("direct_parent") == BASE and active.get("accepted_predecessor") == ACCEPTED, "active lineage")
    require(active["base_active_contract"] == {
        "path": r7.FILES["active"],
        "sha256": sha256((ROOT / r7.FILES["active"]).read_bytes()),
        "active_rows": 265,
    }, "active V7 binding")
    rows = csv_rows(blobs["acceptance"])
    ids = {row["id"] for row in rows}
    require(len(rows) == len(ids) == 27 and ids == {f"P1ER8-{n:03d}" for n in range(1, 28)}, "R8 matrix inventory")
    require(all(row["status"] == "REQUIRED" for row in rows), "R8 optional row")
    require(sha256(blobs["acceptance"]) == ACCEPTANCE_SHA, "R8 acceptance digest")
    require(active["r8_source"] == {
        "path": FILES["acceptance"], "sha256": ACCEPTANCE_SHA,
        "row_count": 27, "superseded_rows": [],
    }, "R8 source binding")
    base_ids = v7_active_ids()
    require(set(active["superseded_base_rows"]) == SUPERSEDED <= base_ids, "R8 superseded rows")
    final_ids = (base_ids - SUPERSEDED) | ids
    mapped: list[str] = []
    for item in active["supersession_map"]:
        require(item["superseded"] and item["replacement"], "empty R8 supersession")
        require(set(item["replacement"]) <= final_ids, "inactive R8 replacement")
        mapped.extend(item["superseded"])
    require(len(mapped) == len(set(mapped)) and set(mapped) == SUPERSEDED, "R8 supersession exactness")
    require(len(final_ids) == 279 and active["active_contract_expectation"] == {
        "base_v7_active_rows": 265, "base_rows_superseded_by_r8": 13,
        "r8_active_rows": 27, "total_active_rows": 279,
        "all_status_required": True,
    }, "R8 active expectation")


def validate_semantic(blobs: dict[str, bytes], semantic: dict[str, Any]) -> None:
    require(semantic.get("schema_version") == 8 and semantic.get("domain") == "moex.stage8b.p1e.semantic-authority-registry.v8", "semantic identity")
    require(semantic["base_registry"] == {
        "path": r7.FILES["semantic"],
        "sha256": sha256((ROOT / r7.FILES["semantic"]).read_bytes()),
        "active_key_count": 28,
    }, "semantic V7 binding")
    old = v7_semantics()
    require(semantic["superseded_active_authorities"] == {key: old[key] for key in REPLACED_SEMANTICS}, "semantic superseded values")
    require(semantic["replacement_active_authorities"] == REPLACED_SEMANTICS, "semantic replacements")
    require(semantic["required_new_keys"] == list(NEW_SEMANTICS), "semantic new key order")
    require(semantic["new_active_authorities"] == NEW_SEMANTICS, "semantic new values")
    composed = dict(old)
    composed.update(REPLACED_SEMANTICS)
    require(not set(NEW_SEMANTICS) & set(composed), "semantic new-key collision")
    composed.update(NEW_SEMANTICS)
    require(len(composed) == 31 and semantic["composed_expectation"] == {
        "retained_base_keys": 26, "replacement_keys": 2, "new_keys": 3,
        "total_active_keys": 31, "active_conflicts": 0,
    }, "semantic composition")
    expected = {
        "latch_aware_source_seam_v2_sha256": sha256(blobs["seam"]),
        "supervisor_event_matrix_v3_sha256": sha256(blobs["events"]),
        "latch_route_transition_matrix_v1_sha256": sha256(blobs["routes"]),
        "shutdown_intent_v1_sha256": sha256(blobs["shutdown"]),
        "i0_regression_gate_v1_sha256": sha256(blobs["regression"]),
        "acquisition_model_v3_sha256": sha256((ROOT / r7.r6.FILES["acquisition"]).read_bytes()),
        "operational_overlay_v6_sha256": sha256((ROOT / r7.r6.FILES["operational"]).read_bytes()),
        "first_boot_transaction_v5_sha256": sha256((ROOT / r7.r6.FILES["transaction"]).read_bytes()),
    }
    require(semantic["contract_bindings"] == expected, "semantic contract bindings")


def validate_seam(blobs: dict[str, bytes], seam: dict[str, Any]) -> None:
    require(sha256(blobs["seam"]) == SEAM_SHA, "seam exact digest")
    require(seam.get("schema_version") == 2 and seam.get("domain") == "moex.stage8b.p1e.latch-aware-source-seam.v2", "seam identity")
    require(seam["selected_option"] == "A-typed-linear-post-acquisition-owner", "seam option")
    require(seam["supersedes"] == {"path": r7.FILES["seam"], "reason": "replace-absolute-post-owner-Redis-read-ban-with-no-second-delivery-acquisition"}, "seam V1 supersession")
    baseline = seam["current_source_baseline"]
    require(baseline == {"path": FILES["source"], "sha256": SOURCE_SHA, "state": "monolithic-pre-seam", "modified_in_r8_design": False}, "source baseline")
    require(sha256(blobs["source"]) == SOURCE_SHA, "source changed in R8 design")
    auth = seam["i0_authorization_after_independent_r8_acceptance"]
    require(auth == {
        "authorized": True, "allowed_production_files": ALLOWED_I0,
        "cargo_change_allowed": False, "workflow_change_allowed": False,
        "supervisor_binary_allowed": False, "deployment_or_activation_allowed": False,
        "full_p1d4_regression_gate_required": True,
    }, "I0 authorization")
    types = seam["opaque_protocol_types"]
    require(types["acquired_owner"] == "Stage8bP1ePostAcquisitionOwnerV1" and types["continue_permit"] == "Stage8bP1eContinuationPermitV1", "linear types")
    require(types["private_fields"] is True and types["private_inner_route_enum"] is True, "owner privacy")
    require(all(types[key] is False for key in ("clone", "copy", "serialize", "deserialize", "reconstruct", "split", "payload_or_transport_getter")), "owner linear traits")
    latch = seam["latch_protocol"]
    require(latch["latch"] == "monotonic-cause-carrying-Stage8bP1eShutdownIntentV1", "cause-carrying latch")
    require(latch["continuation_permit_route_bound"] is True and latch["continuation_permit_single_use"] is True, "permit linearity")
    require(latch["acquisition_future_select_cancellation_allowed"] is False and latch["continuation_future_select_cancellation_allowed"] is False, "select cancellation")
    ban = set(seam["continuation_body_forbidden_delivery_acquisition"])
    require({"XAUTOCLAIM", "XREADGROUP", "exact_delivery_for_evidence", "exact_delivery_for_binding", "construct-second-Stage8bP1ePostAcquisitionOwnerV1", "claim-or-ownership-transfer"} <= ban, "second delivery acquisition ban")
    require("XPENDING" not in ban and "XRANGE" not in ban and "XINFO" not in ban, "terminal observations accidentally banned")
    terminal = seam["post_permit_terminal_resolution_observations"]
    require(terminal["not_a_delivery_acquisition"] is True and terminal["second_delivery_owner_forbidden"] is True, "terminal observation classification")
    require(terminal["allowed_once_per_terminal_permit"] == ["XINFO-GROUPS", "XRANGE-exact-source", "XPENDING-exact-source", "optional-exact-XACK", "XACK-zero-postcheck-XPENDING-and-XINFO"], "terminal command sequence")
    require(set(terminal["classification"]) == {"pending", "already_acknowledged", "xack_zero_response_loss", "conflict"}, "terminal classification")
    p1d4 = seam["p1d4_terminal_revalidation"]
    require(p1d4["required_after_permit_before_terminal_resolution"] is True and p1d4["before_latch_forbidden"] is True and p1d4["omission_forbidden"] is True, "P1d4 revalidation placement")
    require(p1d4["script"] == "P1D4_COMMAND_PUBLICATION_REVALIDATE_LUA", "P1d4 revalidation script")
    require(seam["route_counts"] == {"reclaim_logical": 15, "terminal_logical": 5, "reclaim_variants": 19, "terminal_variants": 11, "total_variants": 30, "physical_acquire_entrypoints": 21, "physical_continuation_entrypoints": 21}, "route counts")
    obligations = set(seam["future_i0_source_checker_must_prove"])
    require(len(obligations) == 12 and {"all-30-route-variant-boundary-latch-race-tests", "cause-preserving-E18-E25-tests", "full-accepted-P1d4-regression-gate"} <= obligations, "I0 proof obligations")


def validate_routes(blobs: dict[str, bytes], routes: dict[str, Any]) -> None:
    require(sha256(blobs["routes"]) == ROUTES_SHA, "route matrix exact digest")
    require(routes.get("schema_version") == 1 and routes.get("domain") == "moex.stage8b.p1e.latch-route-transition-matrix.v1", "route matrix identity")
    rows = routes["rows"]
    require(routes["row_count"] == len(rows) == 30 and routes["class_counts"] == {"reclaim": 19, "terminal": 11}, "route matrix counts")
    require(len({row["cell_id"] for row in rows}) == len({row["test_id"] for row in rows}) == 30, "route IDs unique")
    require(all(set(row) == REQUIRED_ROUTE_FIELDS for row in rows), "route fields")
    actual_variants: dict[str, list[str]] = {}
    for row in rows:
        actual_variants.setdefault(row["logical_route_id"], []).append(row["operation_variant"])
        require(row["acquisition_redis_sequence"] and row["acquire_entrypoint"].startswith("acquire_stage8b_"), "route acquisition")
        require(row["permit_continuation_entrypoint"] and row["first_post_permit_effect"] and row["exact_next_covering_boundary"], "route boundary")
        require(row["exact_returned_owner_or_disposition"] and row["source_pel_disposition"] and row["post_boundary_latch_recheck"].startswith("required"), "route disposition")
    require(actual_variants == VARIANTS, "material route variant inventory")
    indexed = {row["cell_id"]: row for row in rows}
    for prefix in ("LT01", "LT02", "LT03", "LT04", "LT05"):
        pending = indexed[f"{prefix}-pending"]
        already = indexed[f"{prefix}-already-acknowledged"]
        require("exactly-one-XACK" in pending["xack_legality"] and pending["source_pel_disposition"].startswith("acknowledged"), f"{prefix} pending resolution")
        require(already["xack_legality"].startswith("forbidden-zero-XACK") and "continuous-group-frontier" in already["source_pel_disposition"], f"{prefix} already acknowledged")
    for key in ("LT03-pending", "LT03-already-acknowledged"):
        require(indexed[key]["first_post_permit_effect"].startswith("P1D4_COMMAND_PUBLICATION_REVALIDATE_LUA"), "P1d4 terminal revalidation missing")
    due = indexed["LT05-due-day-timer"]
    require(due["timer_disposition"].startswith("SOURCE_FIRST_TIMER_DEFERRED") and "before-timer-reclassification" in due["xack_legality"], "due timer source-first ordering")
    require(indexed["LR12-candidate-source"]["first_post_permit_effect"].startswith("parse-exact-candidate"), "candidate-source variant")
    require(indexed["LR15-s_terminal"]["operation_variant"] == "s_terminal", "S_terminal variant")
    protocol = routes["test_protocol"]
    require(protocol["preset_latch"].endswith("zero post-acquisition effect") and protocol["clear_latch"].startswith("exactly one"), "route test protocol")
    require(protocol["post_permit_signal"].endswith("exact_next_covering_boundary") and protocol["route_mismatch"].startswith("permit is consumed"), "route race protocol")


def validate_shutdown(blobs: dict[str, bytes], shutdown: dict[str, Any]) -> None:
    require(sha256(blobs["shutdown"]) == SHUTDOWN_SHA, "shutdown exact digest")
    require(shutdown.get("schema_version") == 1 and shutdown.get("type") == "Stage8bP1eShutdownIntentV1", "shutdown identity")
    require(shutdown["fields"] == ["cause", "final_exit_class", "grace_deadline", "first_request_sequence"], "shutdown fields")
    require(shutdown["causes"] == {"ExternalSignal": 0, "OwnerFailure": 70, "TelemetryFailure": 71, "SignalTaskFailure": 73}, "shutdown exit mapping")
    rules = shutdown["rules"]
    require(rules["first_cause"] == "immutable-and-never-downgraded", "first cause monotonic")
    require(rules["second_signal"].startswith("diagnostic-only") and rules["grace_expiry"].startswith("exit-72-overrides"), "shutdown precedence")
    require("preserves-entire-shutdown-intent" in rules["e18"] and "preserves-entire-shutdown-intent" in rules["e25"], "E18/E25 preservation")
    cross = shutdown["required_cross_product_tests"]
    require(cross["case_count"] == len(cross["causes"]) * len(cross["locations"]) == 9, "shutdown cross product")


def validate_events(blobs: dict[str, bytes]) -> None:
    require(sha256(blobs["events"]) == EVENTS_SHA, "event matrix exact digest")
    rows = csv_rows(blobs["events"])
    require(len(rows) == 25 and {row["id"] for row in rows} == {f"E{n:02d}" for n in range(1, 26)}, "event inventory")
    by_id = {row["id"]: row for row in rows}
    require(by_id["E18"]["exit_code"] == "shutdown_intent_final_exit_class" and "preserve_entire_shutdown_intent" in by_id["E18"]["allowed_next_effect"], "E18 cause downgrade")
    require(by_id["E25"]["exit_code"] == "shutdown_intent_final_exit_class_if_completed_before_grace_else_72" and "preserve_shutdown_intent" in by_id["E25"]["allowed_next_effect"], "E25 cause downgrade")
    require(by_id["E07"]["exit_code"] == "unchanged_from_first_shutdown_intent", "second signal override")
    require(by_id["E13"]["exit_code"] == "71" and by_id["E15"]["exit_code"] == "73" and by_id["E08"]["exit_code"] == "70", "failure exits")


def validate_regression(blobs: dict[str, bytes], regression: dict[str, Any]) -> None:
    require(sha256(blobs["regression"]) == REGRESSION_SHA, "regression exact digest")
    require(regression.get("mandatory_before_i0_source_acceptance") is True, "I0 regression optional")
    p1d4 = regression["p1d4_full_gate"]
    require(p1d4["command"] == "bash scripts/stage8b_p1d4_source_gate.sh" and p1d4["positive_sigkill_cells"] == 105 and p1d4["clean_runs"] == 2, "P1d4 full gate")
    require({"standalone-P1d2-absent-discriminator-restart-cells", "standalone-P1d3-limit-cancel-expiry-restart-cells", "strict-clippy-both-crates-all-targets-all-features"} <= set(regression["cross_slice_regressions"]), "cross-slice regressions")


def validate_evidence(blobs: dict[str, bytes], evidence: dict[str, Any]) -> None:
    require(evidence.get("status") == "R8_DESIGN_REVIEW_CANDIDATE" and evidence.get("parent") == BASE and evidence.get("accepted_predecessor") == ACCEPTED, "evidence lineage")
    require(evidence.get("r7_review_sha256") == R7_REVIEW_SHA256, "R7 review binding")
    require(evidence.get("design_only") is True and evidence.get("source_modified") is False and evidence.get("i0_source_seam_authorized_now") is False, "evidence design boundary")
    require(evidence.get("i0_source_seam_authorized_after_independent_acceptance") is True and evidence.get("supervisor_source_implementation_authorized") is False, "evidence authorization boundary")
    require(tuple(evidence.get(key) for key in (
        "base_v7_active_rows", "superseded_v7_rows", "r8_rows", "active_rows",
        "semantic_authority_keys", "reclaim_logical_routes", "terminal_logical_routes",
        "reclaim_route_variants", "terminal_route_variants", "route_transition_rows",
        "supervisor_event_rows", "shutdown_cross_product_cases",
        "operational_composed_rows_inherited", "negative_cases",
    )) == (265, 13, 27, 279, 31, 15, 5, 19, 11, 30, 25, 9, 51, 45), "evidence counts")
    require(evidence.get("accepted_source_baseline_sha256") == SOURCE_SHA, "evidence source baseline")
    require(all(value is False for value in evidence["closed_surfaces"].values()), "closed surface opened")
    expected = {"design", "acceptance", "active", "semantic", "seam", "events", "routes", "shutdown", "regression"}
    require(set(evidence["contract_sha256"]) == expected, "evidence hash inventory")
    for name in expected:
        require(evidence["contract_sha256"][name] == sha256(blobs[name]), f"evidence hash {name}")


def validate(overrides: dict[str, bytes] | None = None, check_scope: bool = False) -> None:
    r7.validate({
        r7.FILES["status"]: subprocess.check_output(
            ["git", "show", f"{BASE}:{r7.FILES['status']}"], cwd=ROOT
        ),
        r7.FILES["roadmap"]: subprocess.check_output(
            ["git", "show", f"{BASE}:{r7.FILES['roadmap']}"], cwd=ROOT
        ),
    })
    blobs = read_all(overrides)
    validate_active(blobs, load_json(blobs, "active"))
    validate_semantic(blobs, load_json(blobs, "semantic"))
    validate_seam(blobs, load_json(blobs, "seam"))
    validate_routes(blobs, load_json(blobs, "routes"))
    validate_shutdown(blobs, load_json(blobs, "shutdown"))
    validate_events(blobs)
    validate_regression(blobs, load_json(blobs, "regression"))
    validate_evidence(blobs, load_json(blobs, "evidence"))
    design = blobs["design"].decode("utf-8")
    require(sha256(blobs["design"]) == DESIGN_SHA, "design exact digest")
    require("Terminal resolution is observation, not acquisition" in design and "all 30 material variants" in design, "design correction text")
    require("Stage8bP1eShutdownIntentV1" in design and "105 real SIGKILL cells in two clean runs" in design, "design shutdown/regression")
    require("P1-e R8 executable latch-seam design candidate" in blobs["status"].decode(), "status R8")
    require("P1-e R8 is the active design candidate" in blobs["roadmap"].decode(), "roadmap R8")
    if check_scope:
        actual = changed_files()
        require(actual == EXPECTED_CHANGED, f"scope mismatch missing={sorted(EXPECTED_CHANGED-actual)} extra={sorted(actual-EXPECTED_CHANGED)}")


def main() -> None:
    try:
        validate(check_scope=True)
    except (CheckFailure, r7.CheckFailure, r7.r6.CheckFailure, r7.r6.r5.CheckFailure, r7.r6.r5.r4.CheckFailure, OSError, ValueError, KeyError, TypeError, json.JSONDecodeError) as error:
        print(f"stage8b-p1e-r8-design-check: FAIL {error}")
        raise SystemExit(1)
    print("PASS stage8b-p1e-r8-design-scope files=17 production_rust=0 cargo=0 workflow=0 active_unit=0")
    print("PASS stage8b-p1e-r8-active-contract rows=279 base=265 superseded=13 r8=27")
    print("PASS stage8b-p1e-r8-semantic-authority keys=31 conflicts=0")
    print("PASS stage8b-p1e-r8-terminal-protocol no_second_acquisition=true observations=XINFO+XRANGE+XPENDING optional_xack=true p1d4_revalidation=post_permit")
    print("PASS stage8b-p1e-r8-route-transitions rows=30 reclaim=19 terminal=11 logical=20")
    print("PASS stage8b-p1e-r8-shutdown-intent cases=9 E18=preserve E25=preserve grace=72")
    print("PASS stage8b-p1e-r8-i0-regression p1d4_sigkill=105x2 p1d2=true p1d3=true")


if __name__ == "__main__":
    main()
