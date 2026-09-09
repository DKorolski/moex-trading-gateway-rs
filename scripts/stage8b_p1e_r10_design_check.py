#!/usr/bin/env python3
"""Fail-closed cross-contract checker for Stage 8B-P1-e R10 design."""

from __future__ import annotations

import csv
import hashlib
import io
import json
import pathlib
import subprocess
from typing import Any

import stage8b_p1e_r9_design_check as r9


ROOT = pathlib.Path(__file__).resolve().parents[1]
BASE = "4051a7b4d2c810100aaf983bddede62dbd03d96f"
ACCEPTED = "c2a9e1246dfdd59f3a6297268de907dedcb19903"
R9_REVIEW_SHA256 = "6ae8cf12280e5dfcd15928fd6812c5bef991755aa6163c742a0a4e6c7706546b"
DOCS = "docs/stage-8/"
FILES = {
    "design": DOCS + "stage8b-p1e-deployable-supervisor-design-r10.md",
    "acceptance": DOCS + "stage8b-p1e-deployable-supervisor-r10-acceptance-matrix.csv",
    "active": DOCS + "stage8b-p1e-active-acceptance-contract-v10.json",
    "semantic": DOCS + "stage8b-p1e-semantic-authority-registry-v10.json",
    "counters": DOCS + "stage8b-p1e-route-outcome-counter-contract-v2.json",
    "source_oracle": DOCS + "stage8b-p1e-source-effect-counter-oracle-v1.json",
    "regression": DOCS + "stage8b-p1e-i0-regression-gate-v3.json",
    "evidence": DOCS + "stage8b-p1e-deployable-supervisor-r10-design-evidence.json",
    "regression_script": "scripts/stage8b_p1e_i0_regression_gate.sh",
    "scope_script": "scripts/stage8b_p1e_i0_scope_check.py",
    "retained_script": "scripts/stage8b_p1e_i0_retained_evidence.py",
    "p1d4_regression_script": "scripts/stage8b_p1e_i0_p1d4_regression_check.py",
    "redis_source": "crates/runtime-durable-service/src/stage8b_p1_semantic/redis.rs",
    "recovery_source": "crates/runtime-durable-service/src/recovery.rs",
    "status": "docs/current-status.md",
    "roadmap": "docs/roadmap.md",
}
EXPECTED_CHANGED = {
    FILES[name]
    for name in (
        "design", "acceptance", "active", "semantic", "counters", "source_oracle",
        "regression", "evidence", "regression_script", "scope_script",
        "retained_script", "status", "roadmap",
    )
} | {
    "scripts/stage8b_p1e_r10_design_check.py",
    "scripts/stage8b_p1e_r10_design_negative_harness.py",
    "scripts/stage8b_p1e_r10_design_gate.sh",
    "scripts/stage8b_p1e_r10_design_handoff_safety_check.py",
    "scripts/make_stage8b_p1e_r10_design_handoff.py",
}
INTEGRITY_SHA = {
    "design": "962dcd455d148fbe18544918caf7115fae714d3220020b82a32dd2eaa93d762a",
    "acceptance": "280036691808932534e75fc0cd32c47688ebf9e754c134e8e192b666824ed067",
    "active": "13c8f6af413738d79b8ee9602582f49b4ec8b904e7048a7851cdba03b6c63ff6",
    "semantic": "1b79adf8a7d3baef5bdd5e828bea058804dea79680e04041df7b58087346cdf2",
    "counters": "f70bc4f7c0fa732d18647b101789ad6eaa752e87cef37c761a511f61447b5d36",
    "source_oracle": "706385dc3684815db75c70f7e293940d31e946206b7f2ed4737f8f9fbfa5c9fc",
    "regression": "966e6e7bcab460ce8d459d610d3adfc3a1ecefcdf579b5e80e6b89ad352ebb2d",
}
EVIDENCE_SHA = "3ddbde71b26a9cb2b20a36ce5d60246335c5f0de74afb6077eae2d63d9822acd"
NORMALIZED_MODEL_SHA256 = "7024d2500bf1dc315fb8f97d828fd1c1151fbb6f7d2083cbedb86393187a9c0d"
REDIS_SHA = "a870fbbb6ec9fc60b7df9c35a2aca7a5daf81f695b019352d7f20c9b51439d16"
RECOVERY_SHA = "603fecb619e9535431c0c09c4f9b7093371612643960c26ad854c34660e98146"
SCRIPT_SHA = {
    "regression_script": "627451b5edb563b45f4dce82e6e3419603a3fdb20dd90005356ca54afd2c97e4",
    "scope_script": "f2922d69438e37b9758b93ba28ac94e31a6ff732910c2735b7320cb74a2cfa96",
    "retained_script": "42b9e0f77755206271e1dcdda6113715c4cda2e89033097bc3fa262eb233eedf",
    "p1d4_regression_script": "e0eb1f70c63d1981dda1491018e267adf21cc66b70e735a82c1b80de6eb48068",
}
SUPERSEDED = {"P1ER9-019", "P1ER9-020", "P1ER9-021", "P1ER9-022", "P1ER9-023"}
REPLACEMENTS = {
    "test.RouteVariantBoundaryCoverage": "forty-six-V1-fixtures-compose-with-V2-counter-amendments-using-a-defined-permit-to-return-window-and-replacement-seal-counter",
    "regression.I0CurrentSourceEntryPoint": "accepted-R10-relative-three-file-clean-immutable-scope-plus-retained-P1d4-105x2-and-six-execution-verified-exact-tests",
    "test.RouteOutcomeNormalizedOracle": "integrity-and-redigested-semantic-negatives-cover-composed-fixtures-source-effects-retained-evidence-and-exact-test-execution",
}
NEW_SEMANTICS = {
    "test.RouteOutcomeSourceEffectOracle": "accepted-redis-and-recovery-source-sections-prove-FX10-revalidation-and-LR12-LR15-replacement-seal-counts",
    "evidence.I0RetainedEvidence": "fresh-external-output-atomically-retains-pass-or-fail-log-manifests-digest-crash-evidence-and-exact-tested-ref-tree",
}
COUNTER_FIELDS = [
    "replacement_seal_commit_total", "callback_total", "publication_total",
    "publication_revalidation_total", "xack_total", "timer_reclassification_total",
    "timer_execution_total",
]
EXACT_TESTS = [
    "stage8b_p1_semantic::redis::tests::p1d2_market_feedback_commits_ack_then_truth_then_xacks_source",
    "stage8b_p1_semantic::redis::tests::p1d2_subprocess_kill_matrix_recovers_all_six_durable_frontiers",
    "stage8b_p1_semantic::redis::tests::p1d3_read_only_successor_observation_retains_original_source_until_xack",
    "stage8b_p1_semantic::redis::tests::p1d3_actual_host_optional_tcid_completes_recovered_cancel_and_xacks_last",
    "stage8b_p1_semantic::redis::tests::p1d3_optional_tcid_target_first_restart_continues_without_duplicate_effects",
    "stage8b_p1_semantic::redis::tests::p1d3_subprocess_sigkill_brackets_s_cancel_recovered",
]


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
    tracked = subprocess.check_output(["git", "diff", "--name-only", BASE, "--"], cwd=ROOT, text=True).splitlines()
    untracked = subprocess.check_output(["git", "ls-files", "--others", "--exclude-standard"], cwd=ROOT, text=True).splitlines()
    return {path for path in tracked + untracked if path}


def r9_active_ids() -> set[str]:
    return (r9.r8_active_ids() - r9.SUPERSEDED) | {f"P1ER9-{number:03d}" for number in range(1, 25)}


def r9_semantics() -> dict[str, str]:
    result = r9.r8_semantics()
    result.update(r9.REPLACEMENTS)
    result.update(r9.NEW_SEMANTICS)
    return result


def section(text: str, start: str, end: str) -> str:
    start_index = text.index(start)
    end_index = text.index(end, start_index + len(start))
    return text[start_index:end_index]


def validate_active(blobs: dict[str, bytes], active: dict[str, Any]) -> None:
    require(active.get("schema_version") == 10 and active.get("domain") == "moex.stage8b.p1e.active-acceptance-contract.v10", "active identity")
    require(active.get("direct_parent") == BASE and active.get("accepted_predecessor") == ACCEPTED, "active lineage")
    require(active.get("base_active_contract") == {"path": r9.FILES["active"], "sha256": r9.INTEGRITY_SHA["active"], "active_rows": 298}, "active V9 binding")
    rows = csv_rows(blobs["acceptance"])
    ids = {row["id"] for row in rows}
    require(len(rows) == len(ids) == 24 and ids == {f"P1ER10-{number:03d}" for number in range(1, 25)}, "R10 acceptance inventory")
    require(all(row["status"] == "REQUIRED" for row in rows), "optional R10 row")
    require(active.get("r10_source") == {"path": FILES["acceptance"], "sha256": sha256(blobs["acceptance"]), "row_count": 24, "superseded_rows": []}, "R10 source binding")
    base_ids = r9_active_ids()
    require(len(base_ids) == 298 and set(active.get("superseded_base_rows", [])) == SUPERSEDED <= base_ids, "R10 superseded rows")
    final_ids = (base_ids - SUPERSEDED) | ids
    mapped: list[str] = []
    for item in active.get("supersession_map", []):
        require(set(item["replacement"]) <= final_ids, "inactive R10 replacement")
        mapped.extend(item["superseded"])
    require(len(mapped) == len(set(mapped)) and set(mapped) == SUPERSEDED, "R10 supersession exactness")
    require(len(final_ids) == 317 and active.get("active_contract_expectation") == {"base_v9_active_rows": 298, "base_rows_superseded_by_r10": 5, "r10_active_rows": 24, "total_active_rows": 317, "all_status_required": True}, "R10 active expectation")


def validate_semantic(blobs: dict[str, bytes], semantic: dict[str, Any]) -> None:
    require(semantic.get("schema_version") == 10 and semantic.get("domain") == "moex.stage8b.p1e.semantic-authority-registry.v10", "semantic identity")
    require(semantic.get("base_registry") == {"path": r9.FILES["semantic"], "sha256": r9.INTEGRITY_SHA["semantic"], "active_key_count": 34}, "semantic V9 binding")
    old = r9_semantics()
    require(semantic.get("superseded_active_authorities") == {key: old[key] for key in REPLACEMENTS}, "semantic superseded values")
    require(semantic.get("replacement_active_authorities") == REPLACEMENTS, "semantic replacements")
    require(semantic.get("required_new_keys") == list(NEW_SEMANTICS), "semantic new-key order")
    require(semantic.get("new_active_authorities") == NEW_SEMANTICS, "semantic new values")
    composed = dict(old)
    composed.update(REPLACEMENTS)
    require(not set(NEW_SEMANTICS) & set(composed), "semantic new-key collision")
    composed.update(NEW_SEMANTICS)
    require(len(composed) == 36 and semantic.get("composed_expectation") == {"retained_base_keys": 31, "replacement_keys": 3, "new_keys": 2, "total_active_keys": 36, "active_conflicts": 0}, "semantic composition")
    require(semantic.get("contract_bindings") == {
        "route_outcome_counter_contract_v2_sha256": sha256(blobs["counters"]),
        "source_effect_counter_oracle_v1_sha256": sha256(blobs["source_oracle"]),
        "i0_regression_gate_v3_sha256": sha256(blobs["regression"]),
        "i0_regression_script_sha256": sha256(blobs["regression_script"]),
        "i0_scope_script_sha256": sha256(blobs["scope_script"]),
        "i0_retained_evidence_script_sha256": sha256(blobs["retained_script"]),
    }, "semantic contract bindings")


def validate_counter_composition(blobs: dict[str, bytes], counters: dict[str, Any]) -> None:
    require(counters.get("schema_version") == 2 and counters.get("domain") == "moex.stage8b.p1e.route-outcome-counter-contract.v2", "counter identity")
    base_path = ROOT / r9.FILES["outcomes"]
    base_bytes = base_path.read_bytes()
    require(counters.get("base_fixture_matrix") == {"path": r9.FILES["outcomes"], "sha256": r9.INTEGRITY_SHA["outcomes"], "fixture_count": 46}, "counter V1 binding")
    require(counters.get("counter_fields") == COUNTER_FIELDS, "counter fields")
    window = counters.get("measurement_window", {})
    require(window.get("start") == "after-route-bound-continuation-permit-is-consumed" and window.get("end") == "returned-outcome-boundary-inclusive", "counter measurement window")
    require(window.get("replacement_seal_commit_total", "").startswith("number-of-successful-commit_stage8b_p1_replacement_seal-calls"), "replacement seal definition")
    require(all(window.get(key) is True for key in ("journal_transaction_not_separately_counted", "logical_continuation_not_counted_as_commit", "preexisting_authenticated_seal_not_counted", "read_only_revalidation_not_counted_as_seal")), "counter exclusions")
    profiles = counters.get("effective_profiles", {})
    require(all(isinstance(value, list) and len(value) == 7 and all(isinstance(item, int) and item >= 0 for item in value) for value in profiles.values()), "counter profile shape")
    base = json.loads(base_bytes)
    amendments = counters.get("fixture_profile_amendments", {})
    stop_amendments = counters.get("fixture_stop_point_amendments", {})
    effective: dict[str, dict[str, Any]] = {}
    for fixture in base["fixtures"]:
        item = dict(fixture)
        item["effect_profile"] = amendments.get(item["fixture_id"], item["effect_profile"])
        item["stop_point"] = stop_amendments.get(item["fixture_id"], item["stop_point"])
        require(item["effect_profile"] in profiles, f"missing effective profile {item['fixture_id']}")
        item["effect_counters"] = dict(zip(COUNTER_FIELDS, profiles[item["effect_profile"]]))
        effective[item["fixture_id"]] = item
    require(len(effective) == counters.get("effective_fixture_count") == 46, "effective fixture count")
    require(set(profiles) == {item["effect_profile"] for item in effective.values()}, "unused effective profile")
    expected = {
        "FX10": ("P1D4_ATTACH", [0, 0, 0, 1, 0, 0, 0]),
        "FX20": ("RECONSTRUCTED_SEMANTIC_READY", [2, 1, 0, 0, 1, 0, 0]),
        "FX21": ("RECONSTRUCTED_SEMANTIC_PREPUBLICATION", [2, 1, 0, 0, 0, 0, 0]),
        "FX22": ("RECONSTRUCTED_SEMANTIC_BLOCKED", [1, 1, 0, 0, 0, 0, 0]),
        "FX26": ("SEMANTIC_PREPUBLICATION", [1, 1, 0, 0, 0, 0, 0]),
        "FX27": ("SEMANTIC_BLOCKED", [0, 1, 0, 0, 0, 0, 0]),
        "FX29": ("SEMANTIC_PREPUBLICATION", [1, 1, 0, 0, 0, 0, 0]),
        "FX30": ("SEMANTIC_BLOCKED", [0, 1, 0, 0, 0, 0, 0]),
        "FX43": ("RECONSTRUCTED_CANCEL_RECOVERED", [2, 0, 0, 0, 0, 0, 0]),
        "FX44": ("RECONSTRUCTED_CANCEL_RECOVERED", [2, 0, 0, 0, 0, 0, 0]),
    }
    for fixture_id, (profile, values) in expected.items():
        item = effective[fixture_id]
        require(item["effect_profile"] == profile and list(item["effect_counters"].values()) == values, f"corrected fixture {fixture_id}")
    require("no-new-semantic-seal" in effective["FX22"]["stop_point"] and "no-new-semantic-seal" in effective["FX27"]["stop_point"] and "no-new-semantic-seal" in effective["FX30"]["stop_point"], "multi-intent retained authority")
    p1d4 = counters.get("p1d4_reattachment_rule", {})
    require(p1d4 == {"fixture": "FX10", "permit_before_revalidation": True, "publication_revalidation_total": 1, "replacement_seal_commit_total": 0, "double_revalidation_forbidden": True, "revalidation_omission_forbidden": True, "revalidation_before_latch_decision_forbidden": True}, "P1-d4 attachment rule")


def validate_source_oracle(blobs: dict[str, bytes], oracle: dict[str, Any]) -> None:
    require(oracle.get("schema_version") == 1 and oracle.get("domain") == "moex.stage8b.p1e.source-effect-counter-oracle.v1", "source oracle identity")
    require(oracle.get("accepted_source") == {"redis_path": FILES["redis_source"], "redis_sha256": REDIS_SHA, "recovery_path": FILES["recovery_source"], "recovery_sha256": RECOVERY_SHA}, "source oracle hashes")
    paths = oracle.get("source_paths", {})
    require(set(paths) == {"LR10_P1D4_ACK_REATTACH", "LR12_RECONSTRUCTED", "LR12_CANCEL_RECOVERED", "LR12_SEMANTIC_ZERO_OR_ONE", "LR12_SEMANTIC_MULTI", "LR15_SEMANTIC_ZERO_OR_ONE", "LR15_SEMANTIC_MULTI"}, "source path inventory")
    bindings = oracle.get("fixture_effect_bindings", {})
    require(bindings == {"FX10": "LR10_P1D4_ACK_REATTACH", "FX20": "LR12_SEMANTIC_ZERO_OR_ONE", "FX21": "LR12_SEMANTIC_ZERO_OR_ONE", "FX22": "LR12_SEMANTIC_MULTI", "FX26": "LR15_SEMANTIC_ZERO_OR_ONE", "FX27": "LR15_SEMANTIC_MULTI", "FX29": "LR15_SEMANTIC_ZERO_OR_ONE", "FX30": "LR15_SEMANTIC_MULTI", "FX43": "LR12_CANCEL_RECOVERED", "FX44": "LR12_CANCEL_RECOVERED"}, "source fixture bindings")
    require(paths["LR10_P1D4_ACK_REATTACH"]["publication_revalidation_total"] == 1 and paths["LR10_P1D4_ACK_REATTACH"]["replacement_seal_commit_total"] == 0, "LR10 source counter")
    require(paths["LR12_SEMANTIC_ZERO_OR_ONE"]["replacement_seal_commit_total"] == 2 and paths["LR12_SEMANTIC_MULTI"]["replacement_seal_commit_total"] == 1, "LR12 semantic source counters")
    require(paths["LR15_SEMANTIC_ZERO_OR_ONE"]["replacement_seal_commit_total"] == 1 and paths["LR15_SEMANTIC_MULTI"]["replacement_seal_commit_total"] == 0, "LR15 semantic source counters")
    require(paths["LR12_CANCEL_RECOVERED"]["replacement_seal_commit_total"] == 2, "LR12 cancel source counter")

    redis = blobs["redis_source"].decode()
    recovery = blobs["recovery_source"].decode()
    lr10 = section(redis, "pub async fn resume_stage8b_p1d4_ack_with_redis(", "pub async fn resume_stage8b_p1d4_truth_with_redis(")
    require(lr10.count(".revalidate_p1d4_publication(") == 1 and "commit_stage8b_p1_replacement_seal(" not in lr10, "LR10 source scan")
    require(lr10.index("reclaim_exact_evidence") < lr10.index(".revalidate_p1d4_publication(") < lr10.index("Ok(Stage8bP1RedisGeneratedMarketAckCommitted"), "LR10 source order")
    lr12 = section(redis, "pub async fn resume_stage8b_p1d3_pre_ack_with_redis(", "pub async fn resume_stage8b_p1d3_dispatch_limit_with_redis(")
    require(lr12.count("commit_reconstructed_transition(") == 1 and lr12.count("complete_stage8b_p1d3_semantic(") == 1 and lr12.index("commit_reconstructed_transition(") < lr12.index("complete_stage8b_p1d3_semantic("), "LR12 source order")
    reconstructed = section(recovery, "pub(crate) fn commit_reconstructed_transition(", "impl Stage8bP1d3DispatchPendingOwner")
    require(reconstructed.count("commit_stage8b_p1_replacement_seal(") == 1, "reconstructed seal source scan")
    recovered_cancel = section(recovery, "pub(crate) fn commit_recovered_cancel(", "impl Stage8bP1d3SemanticPendingOwner")
    require(recovered_cancel.count("commit_stage8b_p1_replacement_seal(") == 1, "recovered cancel seal source scan")
    semantic = section(recovery, "fn commit_stage8b_p1d3_semantic(", "pub fn issue_stage8a4_terminal_authority(")
    require(semantic.count("commit_stage8b_p1_replacement_seal(") == 2, "semantic branch seal source scan")
    zero = section(semantic, "Stage6Stage8bP1SemanticTransition::ZeroIntent", "Stage6Stage8bP1SemanticTransition::OneIntentPrepublication")
    one = section(semantic, "Stage6Stage8bP1SemanticTransition::OneIntentPrepublication", "Stage6Stage8bP1SemanticTransition::MultiIntentBlocked")
    multi = semantic[semantic.index("Stage6Stage8bP1SemanticTransition::MultiIntentBlocked"):]
    require(zero.count("commit_stage8b_p1_replacement_seal(") == 1 and one.count("commit_stage8b_p1_replacement_seal(") == 1 and "commit_stage8b_p1_replacement_seal(" not in multi, "semantic branch source scan")
    require(sha256(blobs["redis_source"]) == REDIS_SHA and sha256(blobs["recovery_source"]) == RECOVERY_SHA, "accepted source drift")


def validate_regression(blobs: dict[str, bytes], regression: dict[str, Any]) -> None:
    require(regression.get("schema_version") == 3 and regression.get("domain") == "moex.stage8b.p1e.i0-regression-gate.v3", "regression identity")
    require(regression.get("entrypoint") == "bash scripts/stage8b_p1e_i0_regression_gate.sh ACCEPTED_R10_COMMIT NEW_ABSOLUTE_RETAINED_OUTPUT", "regression entrypoint")
    require(regression.get("accepted_r10_ref_contract") == {"argument_required": True, "must_be_full_commit": True, "must_resolve_to_commit": True, "must_be_ancestor_of_i0_head": True, "i0_head_must_differ": True}, "accepted R10 contract")
    tested = regression.get("tested_source_contract", {})
    require(tested and all(tested.values()), "clean immutable tested source contract")
    scope = regression.get("current_i0_scope", {})
    require(scope.get("production_allowlist") == r9.PRODUCTION_ALLOWLIST and scope.get("helper_prefix_allowlist") == ["docs/stage-8/stage8b-p1e-i0-"] and scope.get("explicit_shared_files") == ["docs/current-status.md", "docs/roadmap.md"], "I0 scope")
    require(scope.get("accepted_gate_scripts_are_not_i0_mutable_helpers") is True and all(scope.get(key) is False for key in ("cargo_allowed", "workflow_allowed", "config_or_unit_allowed", "deployment_allowed")), "I0 closed helper scope")
    retained = regression.get("retained_evidence", {})
    require(all(retained.get(key) is True for key in ("new_absolute_output_argument_required", "output_must_be_outside_repository", "output_must_not_preexist", "temporary_and_output_share_parent", "atomic_publish_by_rename", "pass_and_fail_runs_retained", "handoff_must_bundle_or_exact-digest-link_output")), "retained evidence policy")
    require(set(retained.get("required_files", [])) == {"gate.log", "run-result.json", "source-tree-manifest.json", "artifact-manifest.json", "artifact-manifest.sha256", "crash-evidence/"}, "retained file inventory")
    expected_bindings = {FILES[key]: SCRIPT_SHA[key] for key in SCRIPT_SHA}
    require(regression.get("accepted_script_bindings") == expected_bindings, "accepted script bindings")
    for key, expected in SCRIPT_SHA.items():
        require(sha256(blobs[key]) == expected, f"accepted script drift {key}")
    require(regression.get("cross_slice_exact_tests") == EXACT_TESTS, "full exact test names")
    protocol = regression.get("exact_test_protocol", {})
    require(protocol == {"list_with_exact_first": True, "selected_count_required": 1, "selected_full_name_must_match": True, "execute_with_exact_after_list": True, "passed_count_required": 1, "zero_test_success_forbidden": True, "renamed_or_missing_test_must_fail": True}, "exact test protocol")
    historical = regression.get("historical_p1d4", {})
    require(historical == {"historical_gate_invoked_directly_by_i0": False, "reusable_content_validator_required": True, "source_negative_harness_required": True, "positive_sigkill_cells": 105, "clean_runs": 2, "retained_evidence_checker_required": True, "evidence_negative_harness_required": True}, "P1-d4 reusable suite")
    require(set(regression.get("closed_surfaces", [])) == {"operational-DB0", "VPS-activation", "FINAM-POST-DELETE", "broker-dispatch", "runtime-live", "real-orders"}, "regression closed surfaces")

    gate = blobs["regression_script"].decode()
    require('if [[ "$#" -ne 2 ]]' in gate and "ACCEPTED_R10_COMMIT NEW_ABSOLUTE_RETAINED_OUTPUT" in gate, "gate arguments")
    require('git status --porcelain --untracked-files=all' in gate and gate.count("git rev-parse 'HEAD^{tree}'") >= 2, "gate source binding")
    require("rm -rf" not in gate and "--status FAIL" in gate and "--status PASS" in gate and "os.rename" not in gate, "gate retained lifecycle")
    require("-- --list --exact" in gate and "selected=1 passed=1" in gate and "test result: ok\\. 1 passed; 0 failed;" in gate, "gate execution-aware exact tests")
    for name in EXACT_TESTS:
        require(gate.count(name) == 1, f"gate exact test {name}")
    scope_text = blobs["scope_script"].decode()
    require('"git", "diff", "--name-only", base, "HEAD", "--"' in scope_text and '"status", "--porcelain", "--untracked-files=all"' in scope_text, "scope committed clean source")
    require('"scripts/stage8b_p1e_i0_"' not in scope_text and '"reports/stage8b-p1e-i0-"' not in scope_text, "scope helper script mutability")
    retained_text = blobs["retained_script"].decode()
    require("os.rename(temporary, output)" in retained_text and 'choices=("PASS", "FAIL")' in retained_text, "retained atomic pass/fail")
    require("artifact-manifest.json" in retained_text and "source-tree-manifest.json" in retained_text and "same_source_before_after" in retained_text, "retained manifests and source")


def normalized_model(blobs: dict[str, bytes]) -> dict[str, Any]:
    return {
        "acceptance": csv_rows(blobs["acceptance"]),
        "active": load_json(blobs, "active"),
        "semantic": load_json(blobs, "semantic"),
        "counters": load_json(blobs, "counters"),
        "source_oracle": load_json(blobs, "source_oracle"),
        "regression": load_json(blobs, "regression"),
    }


def normalized_sha(blobs: dict[str, bytes]) -> str:
    return sha256(json.dumps(normalized_model(blobs), sort_keys=True, separators=(",", ":")).encode())


def validate_evidence(blobs: dict[str, bytes], evidence: dict[str, Any]) -> None:
    require(evidence.get("status") == "R10_DESIGN_REVIEW_CANDIDATE" and evidence.get("parent") == BASE and evidence.get("accepted_predecessor") == ACCEPTED, "evidence lineage")
    require(evidence.get("r9_review_sha256") == R9_REVIEW_SHA256, "R9 review binding")
    require(evidence.get("design_checker_only") is True and evidence.get("source_modified") is False and evidence.get("i0_source_seam_authorized_now") is False, "evidence design boundary")
    require(evidence.get("i0_source_seam_authorized_after_independent_acceptance") is True and evidence.get("supervisor_source_implementation_authorized") is False, "evidence authorization")
    require(tuple(evidence.get(key) for key in ("base_v9_active_rows", "superseded_v9_rows", "r10_rows", "active_rows", "semantic_authority_keys", "route_outcome_fixtures", "corrected_fixture_bindings", "counter_fields", "source_effect_paths", "p1d4_positive_sigkill_cells", "p1d4_clean_runs", "p1d2_p1d3_exact_tests", "negative_cases", "integrity_negative_cases", "semantic_negative_cases")) == (298, 5, 24, 317, 36, 46, 10, 7, 7, 105, 2, 6, 53, 8, 45), "evidence counts")
    require(evidence.get("accepted_source_sha256") == {"redis": REDIS_SHA, "recovery": RECOVERY_SHA}, "evidence source hashes")
    require(evidence.get("normalized_semantic_model_sha256") == normalized_sha(blobs) == NORMALIZED_MODEL_SHA256, "evidence normalized model")
    require(evidence.get("contract_sha256") == {name: sha256(blobs[name]) for name in INTEGRITY_SHA}, "evidence contract hashes")
    require(evidence.get("accepted_script_sha256") == {"regression_gate": sha256(blobs["regression_script"]), "scope_check": sha256(blobs["scope_script"]), "retained_evidence": sha256(blobs["retained_script"]), "p1d4_regression_check": sha256(blobs["p1d4_regression_script"])}, "evidence script hashes")
    require(all(value is False for value in evidence.get("closed_surfaces", {}).values()), "closed surface opened")


def validate(overrides: dict[str, bytes] | None = None, check_scope: bool = False, check_integrity: bool = True) -> None:
    r9.validate({
        r9.FILES["status"]: subprocess.check_output(["git", "show", f"{BASE}:{r9.FILES['status']}"], cwd=ROOT),
        r9.FILES["roadmap"]: subprocess.check_output(["git", "show", f"{BASE}:{r9.FILES['roadmap']}"], cwd=ROOT),
    })
    blobs = read_all(overrides)
    if check_integrity:
        for name, expected in INTEGRITY_SHA.items():
            require(sha256(blobs[name]) == expected, f"integrity digest {name}")
        require(sha256(blobs["evidence"]) == EVIDENCE_SHA, "integrity digest evidence")
    validate_active(blobs, load_json(blobs, "active"))
    validate_semantic(blobs, load_json(blobs, "semantic"))
    validate_counter_composition(blobs, load_json(blobs, "counters"))
    validate_source_oracle(blobs, load_json(blobs, "source_oracle"))
    validate_regression(blobs, load_json(blobs, "regression"))
    validate_evidence(blobs, load_json(blobs, "evidence"))
    design = blobs["design"].decode()
    require("replacement_seal_commit_total" in design and "clean committed direct descendant" in design and "Zero-test success" in design, "design correction text")
    require("P1-e R10 is the active narrow design/checker candidate" in blobs["status"].decode(), "status R10")
    require("R10 is the active design/checker correction before I0" in blobs["roadmap"].decode(), "roadmap R10")
    if check_scope:
        actual = changed_files()
        require(actual == EXPECTED_CHANGED, f"scope mismatch missing={sorted(EXPECTED_CHANGED-actual)} extra={sorted(actual-EXPECTED_CHANGED)}")


def main() -> None:
    try:
        validate(check_scope=True)
    except Exception as error:
        print(f"stage8b-p1e-r10-design-check: FAIL {error}")
        raise SystemExit(1)
    print("PASS stage8b-p1e-r10-design-scope files=18 production_rust=0 cargo=0 workflow=0 config=0")
    print("PASS stage8b-p1e-r10-active-contract rows=317 base=298 superseded=5 r10=24")
    print("PASS stage8b-p1e-r10-semantic-authority keys=36 conflicts=0")
    print("PASS stage8b-p1e-r10-route-outcomes fixtures=46 corrected=10 replacement_seal_counter=true")
    print("PASS stage8b-p1e-r10-source-effects paths=7 p1d4_revalidation=1 lr12_lr15_seals=true")
    print("PASS stage8b-p1e-r10-i0-regression clean_immutable=true retained_pass_fail=true exact_tests=6 zero_test_rejected=true")
    print(f"PASS stage8b-p1e-r10-normalized-semantic-oracle sha256={NORMALIZED_MODEL_SHA256}")


if __name__ == "__main__":
    main()
