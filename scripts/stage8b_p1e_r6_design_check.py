#!/usr/bin/env python3
"""Fail-closed cross-contract checker for Stage 8B-P1-e R6 design."""

from __future__ import annotations

import csv
import hashlib
import io
import json
import pathlib
import subprocess
from typing import Any

import stage8b_p1e_r5_design_check as r5


ROOT = pathlib.Path(__file__).resolve().parents[1]
BASE = "3232c2447fc8d6aec038ca518efa11dc4d7e959e"
ACCEPTED = "c2a9e1246dfdd59f3a6297268de907dedcb19903"
R5_REVIEW_SHA256 = "de1eb40aaadcab098170f839b0acbcfe1f28845b896bb473d106a118f059141f"
DOCS = "docs/stage-8/"
FILES = {
    "design": DOCS + "stage8b-p1e-deployable-supervisor-design-r6.md",
    "acceptance": DOCS + "stage8b-p1e-deployable-supervisor-r6-acceptance-matrix.csv",
    "active": DOCS + "stage8b-p1e-active-acceptance-contract-v6.json",
    "semantic": DOCS + "stage8b-p1e-semantic-authority-registry-v6.json",
    "acquisition": DOCS + "stage8b-p1e-acquisition-model-v3.json",
    "operational": DOCS + "stage8b-p1e-operational-pretransition-overlay-v6.json",
    "transaction": DOCS + "stage8b-p1e-first-boot-transaction-v5.json",
    "precedence": DOCS + "stage8b-p1e-source-timer-precedence-v3.json",
    "evidence": DOCS + "stage8b-p1e-deployable-supervisor-r6-design-evidence.json",
    "source": "crates/runtime-durable-service/src/stage8b_p1_semantic/redis.rs",
    "status": "docs/current-status.md",
    "roadmap": "docs/roadmap.md",
}
EXPECTED_CHANGED = {
    FILES[name] for name in (
        "design", "acceptance", "active", "semantic", "acquisition",
        "operational", "transaction", "precedence", "evidence", "status", "roadmap",
    )
} | {
    "scripts/stage8b_p1e_r6_design_check.py",
    "scripts/stage8b_p1e_r6_design_negative_harness.py",
    "scripts/stage8b_p1e_r6_design_gate.sh",
    "scripts/stage8b_p1e_r6_design_handoff_safety_check.py",
    "scripts/make_stage8b_p1e_r6_design_handoff.py",
}
R6_ACCEPTANCE_SHA = "bfa2e0723bc2d0af1b6ee130006e42d44c7eb3b3e7144fde486ba51ffd7f6dff"
SOURCE_SHA = "a870fbbb6ec9fc60b7df9c35a2aca7a5daf81f695b019352d7f20c9b51439d16"
SUPERSEDED = {
    "P1ER4-033", "P1ER4-034", "P1ER4-040", "P1ER5-002", "P1ER5-004",
    "P1ER5-005", "P1ER5-006", "P1ER5-030", "P1ER5-032", "P1ER5-035",
    "P1ER5-040",
}
EXCLUDED_OPERATIONAL = {
    "OC13", "OC16", "OC23", "OC27", "OC32", "OC33", "OC34", "OC35",
    "OC38", "OC41", "OC48", "OC52", "OC55", "OC56", "OC57", "OC58",
}
TERMINAL_ROUTES = [
    ("P1SemanticZeroIntentAckPending", "p1_semantic_zero_intent_terminal", "resolve_stage8b_p1_zero_intent_ack_with_redis", "exact_delivery_for_evidence"),
    ("P1d2TruthCommitted", "p1d2_s_truth", "resume_stage8b_p1d2_truth_with_redis", "exact_delivery_for_binding"),
    ("P1d4GeneratedMarketTruthCommitted", "p1d4_generated_s_truth", "resume_stage8b_p1d4_truth_with_redis", "exact_delivery_for_evidence"),
    ("P1d3TruthCommitted", "p1d3_s_truth", "resume_stage8b_p1d3_truth_with_redis", "exact_delivery_for_binding"),
    ("P1d3TruthCommitted", "p1d3_s_cancel_recovered", "resume_stage8b_p1d3_truth_with_redis", "exact_delivery_for_binding"),
]
NEW_SEMANTICS = {
    "redis.ReclaimRequiredContinuation": "fifteen-exact-owner-phase-routes-existing-wrapper-sole-reclaim-claim-idle-applies",
    "redis.TerminalSourceResolution": "five-logical-owner-phase-routes-exact-stream-entry-plus-PEL-frontier-XACK-or-AlreadyAcknowledged-zero-XAUTOCLAIM",
    "redis.TerminalClaimIdleDependency": "forbidden-idle-age-does-not-change-terminal-resolution",
    "firstboot.PostReceiptStaleTempDisposition": "valid-final-receipt-plus-any-receipt-temp-is-conflict-before-marker-mutation",
}
REPLACED_SEMANTICS = {
    "redis.NonReadyAcquisitionOwner": "partitioned-reclaim-required-XAUTOCLAIM-vs-terminal-exact-resolution-no-claim",
    "shutdown.PostDeliveryLatch": "after-Ready-delivery-or-reclaim-required-reclaim-or-terminal-exact-lookup-before-parse-continuation-callback-provider-schedule-XACK",
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


def base_v5_active_ids() -> set[str]:
    contract = json.loads((ROOT / r5.FILES["active"]).read_bytes())
    result: set[str] = set()
    for source in contract["sources"]:
        rows = csv_rows((ROOT / source["path"]).read_bytes())
        result |= {row["id"] for row in rows} - set(source["superseded_rows"])
    require(len(result) == 217, "V5 active source inventory")
    return result


def validate_active(blobs: dict[str, bytes], active: dict[str, Any]) -> None:
    require(active.get("schema_version") == 6 and active.get("domain") == "moex.stage8b.p1e.active-acceptance-contract.v6", "active identity")
    require(active.get("direct_parent") == BASE and active.get("accepted_predecessor") == ACCEPTED, "active lineage")
    base = active["base_active_contract"]
    base_data = (ROOT / base["path"]).read_bytes()
    require(sha256(base_data) == base["sha256"] == "d15835c2e33d67d4210bbef0eb88fd81fc29fd5a29b29cfc53a0e7cfa5e89a3a", "active V5 binding")
    require(base["active_rows"] == 217, "active V5 count")
    rows = csv_rows(blobs["acceptance"])
    r6_ids = {row["id"] for row in rows}
    require(len(rows) == len(r6_ids) == 36 and all(row["status"] == "REQUIRED" for row in rows), "R6 matrix")
    source = active["r6_source"]
    require(source["path"] == FILES["acceptance"] and source["row_count"] == 36 and source["superseded_rows"] == [], "R6 source")
    require(source["sha256"] == sha256(blobs["acceptance"]) == R6_ACCEPTANCE_SHA, "R6 acceptance digest")
    base_ids = base_v5_active_ids()
    require(set(active["superseded_base_rows"]) == SUPERSEDED <= base_ids, "R6 superseded rows")
    require(not r6_ids & base_ids, "R6 duplicate IDs")
    mapped: list[str] = []
    final_ids = (base_ids - SUPERSEDED) | r6_ids
    for item in active["supersession_map"]:
        require(item["superseded"] and item["replacement"], "empty R6 supersession")
        require(all(row in final_ids for row in item["replacement"]), "inactive R6 replacement")
        mapped.extend(item["superseded"])
    require(len(mapped) == len(set(mapped)) and set(mapped) == SUPERSEDED, "R6 supersession exactness")
    require(len(final_ids) == 242, "R6 active total")
    require(active["active_contract_expectation"] == {
        "base_v5_active_rows": 217, "base_rows_superseded_by_r6": 11,
        "r6_active_rows": 36, "total_active_rows": 242,
        "all_status_required": True,
    }, "R6 active expectation")


def validate_semantic(blobs: dict[str, bytes], semantic: dict[str, Any]) -> None:
    require(semantic.get("schema_version") == 6 and semantic.get("domain") == "moex.stage8b.p1e.semantic-authority-registry.v6", "semantic identity")
    base = semantic["base_registry"]
    base_data = (ROOT / base["path"]).read_bytes()
    require(sha256(base_data) == base["sha256"] == "9793e5e854538b3699eeca9dbab63b4d6de9b1e0960481792c9be51b74e7f7dc", "semantic V5 binding")
    base_values = json.loads(base_data)["expected_active_values"]
    require(len(base_values) == base["active_key_count"] == 22, "semantic V5 count")
    old = semantic["superseded_active_authorities"]
    require(old == {key: base_values[key] for key in REPLACED_SEMANTICS}, "semantic replacement source")
    require(semantic["replacement_active_authorities"] == REPLACED_SEMANTICS, "semantic replacements")
    require(semantic["required_new_keys"] == list(NEW_SEMANTICS), "semantic new key order")
    require(semantic["new_active_authorities"] == NEW_SEMANTICS, "semantic new values")
    composed = dict(base_values)
    composed.update(REPLACED_SEMANTICS)
    require(not set(NEW_SEMANTICS) & set(composed), "semantic new-key collision")
    composed.update(NEW_SEMANTICS)
    require(len(composed) == 26 and semantic["composed_expectation"] == {
        "retained_base_keys": 20, "replacement_keys": 2, "new_keys": 4,
        "total_active_keys": 26, "active_conflicts": 0,
    }, "semantic composition")
    bindings = semantic["contract_bindings"]
    require(set(bindings) == {
        "acquisition_model_v3_sha256", "operational_overlay_v6_sha256",
        "first_boot_transaction_v5_sha256", "source_timer_precedence_v3_sha256",
    }, "semantic binding inventory")
    for name, key in (
        ("acquisition", "acquisition_model_v3_sha256"),
        ("operational", "operational_overlay_v6_sha256"),
        ("transaction", "first_boot_transaction_v5_sha256"),
        ("precedence", "source_timer_precedence_v3_sha256"),
    ):
        require(bindings[key] == sha256(blobs[name]), f"semantic binding {name}")


def function_body(source: str, name: str) -> str:
    marker = f"pub async fn {name}("
    require(source.count(marker) == 1, f"source function marker {name}")
    start = source.index("{", source.index(marker))
    depth = 0
    for index in range(start, len(source)):
        if source[index] == "{":
            depth += 1
        elif source[index] == "}":
            depth -= 1
            if depth == 0:
                return source[start:index + 1]
    raise CheckFailure(f"unterminated source function {name}")


def validate_acquisition(blobs: dict[str, bytes], acquisition: dict[str, Any]) -> None:
    require(acquisition.get("schema_version") == 3 and acquisition.get("domain") == "moex.stage8b.p1e.acquisition-model.v3", "acquisition identity")
    require(acquisition["selected_model"] == "split-non-ready-reclaim-required-semantic-continuation-vs-terminal-no-claim-source-resolution", "acquisition split")
    oracle = acquisition["accepted_source_oracle"]
    require(oracle == {"path": FILES["source"], "sha256": SOURCE_SHA, "modification_authorized": False}, "source oracle contract")
    require(sha256(blobs["source"]) == SOURCE_SHA, "source oracle digest")
    s06 = acquisition["s06_non_ready_observation"]
    require(set(s06["forbidden_redis_operations"]) == {"XAUTOCLAIM", "XREADGROUP", "XACK"}, "S06 forbidden operations")
    require(s06["observation_contains_payload"] is False and s06["observation_grants_processing_authority"] is False, "S06 authority")
    reclaim = acquisition["reclaim_required_semantic_continuations"]
    reclaim_keys = {(row["owner"], row["phase"]) for row in reclaim}
    require(len(reclaim) == len(reclaim_keys) == 15, "reclaim route inventory")
    require(all("resume_stage8b_" in row["wrapper"] for row in reclaim), "reclaim wrapper binding")
    reclaim_rule = acquisition["reclaim_required_rule"]
    require(reclaim_rule["maximum_successful_reclaims"] == 1 and reclaim_rule["claim_idle_ms_applies"] is True, "reclaim rule")
    require(reclaim_rule["second_acquisition"] == "forbidden" and "Degraded" in reclaim_rule["pending_not_claimable"], "reclaim fallback")

    terminal = acquisition["terminal_source_resolution_continuations"]
    actual_routes = [(row["owner"], row["phase"], row["function"], row["lookup"]) for row in terminal]
    require(actual_routes == TERMINAL_ROUTES, "terminal route inventory")
    terminal_keys = {(row[0], row[1]) for row in actual_routes}
    require(not reclaim_keys & terminal_keys, "acquisition owner overlap")
    source = blobs["source"].decode("utf-8")
    for _, _, name, lookup in TERMINAL_ROUTES:
        body = function_body(source, name)
        require(f".{lookup}(" in body, f"terminal lookup {name}")
        require("reclaim_" not in body and "XAUTOCLAIM" not in body, f"terminal reclaim drift {name}")
        if name == "resume_stage8b_p1d4_truth_with_redis":
            require(".revalidate_p1d4_publication(" in body, "P1d4 publication revalidation")
    rule = acquisition["terminal_resolution_rule"]
    require(rule["XAUTOCLAIM_total"] == 0 and rule["ownership_transfer_total"] == 0 and rule["claim_idle_ms_dependency"] is False, "terminal no-claim rule")
    require("one-XACK" in rule["exact_pending"] and "AlreadyAcknowledged" in rule["already_acknowledged"], "terminal dispositions")
    require(rule["post_lookup_latch"] == "before-XACK-or-AlreadyAcknowledged-return", "terminal post-lookup latch")
    require(rule["conflict"] == "changed-or-missing-entry-binding-or-discontinuous-frontier-fails-closed-with-zero-XACK", "terminal conflict")
    require(rule["truth_replay"] == rule["ACK_replay"] == rule["new_callback"] == "forbidden", "terminal failure/replay")
    require(acquisition["owner_partition"] == {
        "reclaim_required_count": 15, "terminal_logical_count": 5,
        "intersection": [], "unlisted_non_ready_owner": "exit-67-before-transition",
    }, "owner partition")
    require(acquisition["idle_boundary_tests"] == [
        "idle-age-zero", "idle-age-claim_idle_ms-minus-one",
        "idle-age-equal-claim_idle_ms", "idle-age-claim_idle_ms-plus-one",
    ], "idle test inventory")
    require(acquisition["instrumentation"]["terminal_XAUTOCLAIM_total"] == "zero", "terminal instrumentation")


def validate_operational(blobs: dict[str, bytes], operational: dict[str, Any]) -> None:
    require(operational.get("schema_version") == 6 and operational.get("domain") == "moex.stage8b.p1e.operational-pretransition-overlay.v6", "operational identity")
    base = operational["base_matrix"]
    base_data = (ROOT / base["path"]).read_bytes()
    base_rows = csv_rows(base_data)
    require(sha256(base_data) == base["sha256"] == "6e8a9b0bd1625788f6f5c1d961118bb07e5b41bd60eafa7c628ebc560c3488d1", "operational V5 binding")
    require(len(base_rows) == base["row_count"] == 56, "operational V5 count")
    base_ids = {row["id"] for row in base_rows}
    require(set(operational["excluded_base_rows"]) == EXCLUDED_OPERATIONAL <= base_ids, "operational exclusions")
    rows = operational["replacement_rows"]
    ids = {row["id"] for row in rows}
    require(len(rows) == len(ids) == 11 and ids == {f"R6OC{number:02d}" for number in range(1, 12)}, "operational replacements")
    keys = {(row["local_restart_variant"], row["authenticated_package_phase"], row["redis_source_state"], row["timer_state"]) for row in rows}
    require(len(keys) == 11, "operational tuple uniqueness")
    require(all("claimable" not in row["redis_source_state"] for row in rows), "terminal claimability retained")
    defaults = operational["replacement_defaults"]
    require(defaults["XAUTOCLAIM_total"] == 0 and defaults["claim_idle_ms_dependency"] is False, "operational no-claim defaults")
    require(defaults["truth_replay_legality"] == defaults["ack_replay_legality"] == defaults["new_callback_legality"] == "forbidden", "operational replay defaults")
    expected_functions = {(owner, phase): function for owner, phase, function, _ in TERMINAL_ROUTES}
    for row in rows:
        require(row["first_legal_transition"] == expected_functions[(row["local_restart_variant"], row["authenticated_package_phase"])], f"operational function {row['id']}")
        require(row["source_disposition"] in {"one_exact_XACK", "AlreadyAcknowledged-zero-XACK", "one_exact_XACK-before-timer-reclassification"}, f"operational disposition {row['id']}")
        if row["redis_source_state"] == "source_already_acknowledged_with_continuous_frontier":
            require(row["source_disposition"] == "AlreadyAcknowledged-zero-XACK", f"operational AlreadyAcknowledged {row['id']}")
        elif row["timer_state"] == "exact_day_expiry_due":
            require(row["source_disposition"] == "one_exact_XACK-before-timer-reclassification", f"operational timer disposition {row['id']}")
        else:
            require(row["source_disposition"] == "one_exact_XACK", f"operational pending disposition {row['id']}")
        require(not row["paper_ready_legality"].startswith("immediate"), f"operational PaperReady {row['id']}")
    cancel = [row for row in rows if row["authenticated_package_phase"] == "p1d3_s_cancel_recovered"]
    require({row["id"] for row in cancel} == {"R6OC09", "R6OC10", "R6OC11"}, "cancel terminal cells")
    due = next(row for row in cancel if row["id"] == "R6OC11")
    require(due["timer_state"] == "exact_day_expiry_due" and due["timer_derivation"] == "SOURCE_FIRST_TIMER_DEFERRED", "cancel source-first timer")
    require(due["first_legal_transition"] == "resume_stage8b_p1d3_truth_with_redis" and "timer-reclassification" in due["paper_ready_legality"], "cancel due ordering")
    require(operational["composed_contract"] == {
        "retained_base_rows": 40, "replacement_rows": 11, "total_rows": 51,
        "unique_ids": True, "terminal_not_yet_claimable_rows": 0,
        "terminal_claimable_rows": 0, "terminal_pending_rows": 5,
        "terminal_already_acknowledged_rows": 5, "terminal_due_timer_rows": 1,
    }, "operational composition")


def validate_transaction(transaction: dict[str, Any]) -> None:
    require(transaction.get("schema_version") == 5 and transaction.get("domain") == "moex.stage8b.p1e.first-boot-transaction.v5", "transaction identity")
    base = transaction["base_contract"]
    base_data = (ROOT / base["path"]).read_bytes()
    require(sha256(base_data) == base["sha256"] == "f797754c8484b6dbaa7a271a8a52f05a2b7dd7a7043f6a4264bd54f6031830ec", "transaction V4 binding")
    old = json.loads(base_data)
    classes = {row["classification"]: row for row in old["base_classifications"]}
    temps = {row["classification"]: row for row in old["marker_update_temp_classifications"]}
    replacements = transaction["predicate_replacements"]
    receipt = replacements["ReceiptCommittedMarkerUpdatePending"]
    adopted_temp = replacements["SealCommittedToAdoptedMarkerTempPending"]
    require(receipt["old_precondition"] == classes["ReceiptCommittedMarkerUpdatePending"]["precondition"], "receipt old predicate")
    require(adopted_temp["old_exclusive_precondition"] == temps["SealCommittedToAdoptedMarkerTempPending"]["exclusive_precondition"], "adopted temp old predicate")
    require(receipt["new_precondition"].endswith("-and-receipt-temp-absent") and "receipt-temp-absent" in adopted_temp["new_exclusive_precondition"], "receipt temp absence")
    require(receipt["run_allowed"] is False and adopted_temp["run_allowed"] is False, "post-receipt run")
    stale = transaction["stale_receipt_temp_rule"]
    require(stale == {
        "precondition": "valid-final-receipt-plus-any-receipt-temp-plus-canonical-marker-phase-seal_committed",
        "classification": "CorruptOrIdentityMismatch", "exit_code": 66,
        "marker_mutation_allowed": False, "receipt_mutation_allowed": False,
        "ordinary_run_allowed": False, "evidence": "preserve",
    }, "stale receipt temp rule")
    require(transaction["composed_inventory"] == {
        "base_classifications": 11, "marker_update_temp_classifications": 4,
        "classification_model": "pairwise-disjoint-exactly-one-match-no-precedence-mandatory-adopted-marker",
        "all_unmodified_v4_fields_retained": True,
    }, "transaction composition")
    require(len(transaction["required_tests"]) == 4, "transaction tests")


def validate_precedence(precedence: dict[str, Any]) -> None:
    require(precedence.get("schema_version") == 3 and precedence.get("domain") == "moex.stage8b.p1e.source-timer-precedence.v3", "precedence identity")
    base = precedence["base_contract"]
    base_data = (ROOT / base["path"]).read_bytes()
    require(sha256(base_data) == base["sha256"] == "fc6292b844a0e2fa8f207d5ef3a41767cd5a17b8da564dbbd0d75a96e40c5133", "precedence V2 binding")
    amendment = precedence["terminal_resolution_amendment"]
    require(amendment["claimability_terms_superseded"] is True and amendment["claim_idle_ms_dependency"] is False, "precedence no claimability")
    require(amendment["terminal_source_states"] == ["one_exact_pending_source_any_idle_age", "source_already_acknowledged_with_continuous_frontier"], "precedence terminal states")
    cancel = precedence["cancel_recovered_exact_contract"]
    require(cancel["operational_rows"] == ["R6OC09-exact-pending", "R6OC10-already-acknowledged", "R6OC11-exact-pending-plus-due-Day-timer"], "precedence cancel cells")
    require(cancel["sole_wrapper"] == "resume_stage8b_p1d3_truth_with_redis" and cancel["XAUTOCLAIM_total"] == 0, "precedence cancel wrapper")
    require(cancel["truth_replay_allowed"] is False and cancel["ack_replay_allowed"] is False and cancel["new_callback_allowed"] is False, "precedence cancel replay")
    due = precedence["simultaneous_due_timer"]
    require(due["first_transition"] == "resolve-terminal-source-without-reclaim" and due["timer_before_source"] == "forbidden", "precedence source first")
    require(len(precedence["required_tests"]) == 7 and precedence["unlisted_tuple"]["disposition"] == "exit-67-before-transition", "precedence tests/fallback")


def validate_evidence(blobs: dict[str, bytes], evidence: dict[str, Any]) -> None:
    design, status, roadmap = blobs["design"].decode(), blobs["status"].decode(), blobs["roadmap"].decode()
    for token in ("Status: design-only review candidate", "242", "Terminal source resolution without reclaim", "R6OC09", "stale receipt temp"):
        require(token in design, f"design token {token}")
    require("They never call XAUTOCLAIM, never\ntransfer consumer ownership and never depend on `claim_idle_ms`" in design, "design terminal no-claim rule")
    for text, name in ((status, "status"), (roadmap, "roadmap")):
        for token in ("P1-e R6", BASE, "source implementation remains unauthorized"):
            require(token in text, f"{name} token {token}")
    require(evidence.get("status") == "R6_DESIGN_REVIEW_CANDIDATE" and evidence.get("parent") == BASE, "evidence status")
    require(evidence.get("r5_review_sha256") == R5_REVIEW_SHA256, "R5 review binding")
    require(evidence.get("design_only") is True and evidence.get("source_implementation_authorized") is False, "evidence scope")
    require(tuple(evidence.get(key) for key in (
        "active_rows", "semantic_authority_keys", "reclaim_required_routes",
        "terminal_logical_routes", "operational_composed_rows", "negative_cases",
    )) == (242, 26, 15, 5, 51, 40), "evidence counts")
    require(evidence.get("accepted_source_oracle_sha256") == SOURCE_SHA, "evidence source oracle")
    require(all(value is False for value in evidence["closed_surfaces"].values()), "closed surface opened")
    hashes = evidence["contract_sha256"]
    expected = {"design", "acceptance", "active", "semantic", "acquisition", "operational", "transaction", "precedence"}
    require(set(hashes) == expected, "evidence hash inventory")
    for name in expected:
        require(hashes[name] == sha256(blobs[name]), f"evidence hash {name}")


def validate(overrides: dict[str, bytes] | None = None, check_scope: bool = False) -> None:
    r5.validate()
    blobs = read_all(overrides)
    active = load_json(blobs, "active")
    semantic = load_json(blobs, "semantic")
    acquisition = load_json(blobs, "acquisition")
    operational = load_json(blobs, "operational")
    transaction = load_json(blobs, "transaction")
    precedence = load_json(blobs, "precedence")
    evidence = load_json(blobs, "evidence")
    validate_active(blobs, active)
    validate_semantic(blobs, semantic)
    validate_acquisition(blobs, acquisition)
    validate_operational(blobs, operational)
    validate_transaction(transaction)
    validate_precedence(precedence)
    validate_evidence(blobs, evidence)
    if check_scope:
        actual = changed_files()
        require(actual == EXPECTED_CHANGED, f"scope mismatch missing={sorted(EXPECTED_CHANGED-actual)} extra={sorted(actual-EXPECTED_CHANGED)}")


def main() -> None:
    try:
        validate(check_scope=True)
    except (CheckFailure, r5.CheckFailure, r5.r4.CheckFailure, OSError, ValueError, KeyError, TypeError, json.JSONDecodeError) as error:
        print(f"stage8b-p1e-r6-design-check: FAIL {error}")
        raise SystemExit(1)
    print("PASS stage8b-p1e-r6-design-scope files=16 production_rust=0 cargo=0 workflow=0 active_unit=0")
    print("PASS stage8b-p1e-r6-active-contract rows=242 base=217 superseded=11 r6=36")
    print("PASS stage8b-p1e-r6-semantic-authority keys=26 conflicts=0")
    print("PASS stage8b-p1e-r6-acquisition reclaim_required=15 terminal=5 terminal_XAUTOCLAIM=0")
    print("PASS stage8b-p1e-r6-operational base=56 excluded=16 replacements=11 composed=51")
    print("PASS stage8b-p1e-r6-first-boot stale_receipt_temp=conflict-before-marker-mutation")
    print("PASS stage8b-p1e-r6-source-oracle terminal_functions=4 sha256=" + SOURCE_SHA)


if __name__ == "__main__":
    main()
