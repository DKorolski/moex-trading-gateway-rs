#!/usr/bin/env python3
"""Integrity and redigested semantic mutations for P1-e R10."""

from __future__ import annotations

import json
from typing import Any, Callable

import stage8b_p1e_r10_design_check as checker


BASE = checker.read_all()


def json_bytes(value: Any) -> bytes:
    return (json.dumps(value, indent=2, sort_keys=True) + "\n").encode()


def mutate_json(blobs: dict[str, bytes], name: str, change: Callable[[dict[str, Any]], None]) -> None:
    value = json.loads(blobs[name])
    change(value)
    blobs[name] = json_bytes(value)


def redigest(blobs: dict[str, bytes]) -> None:
    active = json.loads(blobs["active"])
    active["r10_source"]["sha256"] = checker.sha256(blobs["acceptance"])
    blobs["active"] = json_bytes(active)

    regression = json.loads(blobs["regression"])
    for key, file_key in (
        (checker.FILES["regression_script"], "regression_script"),
        (checker.FILES["scope_script"], "scope_script"),
        (checker.FILES["retained_script"], "retained_script"),
        (checker.FILES["p1d4_regression_script"], "p1d4_regression_script"),
    ):
        if key in regression.get("accepted_script_bindings", {}):
            regression["accepted_script_bindings"][key] = checker.sha256(blobs[file_key])
    blobs["regression"] = json_bytes(regression)

    semantic = json.loads(blobs["semantic"])
    bindings = semantic["contract_bindings"]
    bindings.update({
        "route_outcome_counter_contract_v2_sha256": checker.sha256(blobs["counters"]),
        "source_effect_counter_oracle_v1_sha256": checker.sha256(blobs["source_oracle"]),
        "i0_regression_gate_v3_sha256": checker.sha256(blobs["regression"]),
        "i0_regression_script_sha256": checker.sha256(blobs["regression_script"]),
        "i0_scope_script_sha256": checker.sha256(blobs["scope_script"]),
        "i0_retained_evidence_script_sha256": checker.sha256(blobs["retained_script"]),
    })
    blobs["semantic"] = json_bytes(semantic)

    evidence = json.loads(blobs["evidence"])
    for name in evidence["contract_sha256"]:
        evidence["contract_sha256"][name] = checker.sha256(blobs[name])
    evidence["accepted_script_sha256"] = {
        "regression_gate": checker.sha256(blobs["regression_script"]),
        "scope_check": checker.sha256(blobs["scope_script"]),
        "retained_evidence": checker.sha256(blobs["retained_script"]),
        "p1d4_regression_check": checker.sha256(blobs["p1d4_regression_script"]),
    }
    blobs["evidence"] = json_bytes(evidence)


def amend_fixture(value: dict[str, Any], fixture_id: str, profile: str) -> None:
    value["fixture_profile_amendments"][fixture_id] = profile


def amend_path(value: dict[str, Any], path_id: str, field: str, value_: Any) -> None:
    value["source_paths"][path_id][field] = value_


def mutate_section(blobs: dict[str, bytes], name: str, start: str, end: str, change: Callable[[str], str]) -> None:
    text = blobs[name].decode()
    left = text.index(start)
    right = text.index(end, left + len(start))
    blobs[name] = (text[:left] + change(text[left:right]) + text[right:]).encode()


integrity_cases: list[tuple[str, dict[str, bytes]]] = []
for name in ("design", "acceptance", "active", "semantic", "counters", "source_oracle", "regression", "evidence"):
    blobs = dict(BASE)
    blobs[name] += b"\n"
    integrity_cases.append((f"integrity-{name}-byte-drift", blobs))


semantic_cases: list[tuple[str, dict[str, bytes]]] = []


def add(name: str, change: Callable[[dict[str, bytes]], None], *, digest: bool = True) -> None:
    blobs = dict(BASE)
    change(blobs)
    if digest:
        redigest(blobs)
    semantic_cases.append((name, blobs))


add("active-total-drift", lambda b: mutate_json(b, "active", lambda v: v["active_contract_expectation"].update(total_active_rows=318)))
add("semantic-source-oracle-authority-drift", lambda b: mutate_json(b, "semantic", lambda v: v["new_active_authorities"].update({"test.RouteOutcomeSourceEffectOracle": "trust-fixtures-without-source"})))
add("counter-window-start-before-permit", lambda b: mutate_json(b, "counters", lambda v: v["measurement_window"].update(start="before-permit")))
add("counter-window-end-before-return", lambda b: mutate_json(b, "counters", lambda v: v["measurement_window"].update(end="before-return")))
add("counter-name-reverted-to-ambiguous", lambda b: mutate_json(b, "counters", lambda v: v["counter_fields"].__setitem__(0, "durable_commit_total")))
add("fx10-profile-attach-zero-revalidation", lambda b: mutate_json(b, "counters", lambda v: amend_fixture(v, "FX10", "ATTACH")))
add("fx10-revalidation-omitted", lambda b: mutate_json(b, "counters", lambda v: v["p1d4_reattachment_rule"].update(publication_revalidation_total=0)))
add("fx10-revalidation-before-permit", lambda b: mutate_json(b, "counters", lambda v: v["p1d4_reattachment_rule"].update(permit_before_revalidation=False)))
add("fx10-double-revalidation", lambda b: mutate_json(b, "counters", lambda v: v["effective_profiles"]["P1D4_ATTACH"].__setitem__(3, 2)))
add("fx20-reconstructed-seal-omitted", lambda b: mutate_json(b, "counters", lambda v: amend_fixture(v, "FX20", "SEMANTIC_READY")))
add("fx21-reconstructed-seal-omitted", lambda b: mutate_json(b, "counters", lambda v: amend_fixture(v, "FX21", "SEMANTIC_PREPUBLICATION")))
add("fx22-imaginary-semantic-seal", lambda b: mutate_json(b, "counters", lambda v: amend_fixture(v, "FX22", "RECONSTRUCTED_SEMANTIC_PREPUBLICATION")))
add("fx26-semantic-seal-omitted", lambda b: mutate_json(b, "counters", lambda v: amend_fixture(v, "FX26", "SEMANTIC_BLOCKED")))
add("fx27-imaginary-semantic-seal", lambda b: mutate_json(b, "counters", lambda v: amend_fixture(v, "FX27", "SEMANTIC_PREPUBLICATION")))
add("fx29-semantic-seal-omitted", lambda b: mutate_json(b, "counters", lambda v: amend_fixture(v, "FX29", "SEMANTIC_BLOCKED")))
add("fx30-imaginary-semantic-seal", lambda b: mutate_json(b, "counters", lambda v: amend_fixture(v, "FX30", "SEMANTIC_PREPUBLICATION")))
add("fx43-recovered-cancel-seal-omitted", lambda b: mutate_json(b, "counters", lambda v: amend_fixture(v, "FX43", "COMMIT")))
add("fx44-recovered-cancel-seal-omitted", lambda b: mutate_json(b, "counters", lambda v: amend_fixture(v, "FX44", "COMMIT")))
add("multi-intent-stop-claims-new-seal", lambda b: mutate_json(b, "counters", lambda v: v["fixture_stop_point_amendments"].update(FX27="durable-covering-seal")))
add("source-binding-fx10-misdirected", lambda b: mutate_json(b, "source_oracle", lambda v: v["fixture_effect_bindings"].update(FX10="LR15_SEMANTIC_MULTI")))
add("source-oracle-lr10-revalidation-omitted", lambda b: mutate_json(b, "source_oracle", lambda v: amend_path(v, "LR10_P1D4_ACK_REATTACH", "publication_revalidation_total", 0)))
add("source-oracle-lr12-zero-one-seal-omitted", lambda b: mutate_json(b, "source_oracle", lambda v: amend_path(v, "LR12_SEMANTIC_ZERO_OR_ONE", "replacement_seal_commit_total", 1)))
add("source-oracle-lr12-multi-imaginary-seal", lambda b: mutate_json(b, "source_oracle", lambda v: amend_path(v, "LR12_SEMANTIC_MULTI", "replacement_seal_commit_total", 2)))
add("source-oracle-lr15-zero-one-seal-omitted", lambda b: mutate_json(b, "source_oracle", lambda v: amend_path(v, "LR15_SEMANTIC_ZERO_OR_ONE", "replacement_seal_commit_total", 0)))
add("source-oracle-lr15-multi-imaginary-seal", lambda b: mutate_json(b, "source_oracle", lambda v: amend_path(v, "LR15_SEMANTIC_MULTI", "replacement_seal_commit_total", 1)))
add("source-oracle-cancel-seal-omitted", lambda b: mutate_json(b, "source_oracle", lambda v: amend_path(v, "LR12_CANCEL_RECOVERED", "replacement_seal_commit_total", 1)))


def omit_lr10_revalidation(blobs: dict[str, bytes]) -> None:
    mutate_section(blobs, "redis_source", "pub async fn resume_stage8b_p1d4_ack_with_redis(", "pub async fn resume_stage8b_p1d4_truth_with_redis(", lambda value: value.replace(".revalidate_p1d4_publication(", ".revalidate_removed(", 1))


def double_lr10_revalidation(blobs: dict[str, bytes]) -> None:
    mutate_section(blobs, "redis_source", "pub async fn resume_stage8b_p1d4_ack_with_redis(", "pub async fn resume_stage8b_p1d4_truth_with_redis(", lambda value: value.replace(".revalidate_p1d4_publication(", ".revalidate_p1d4_publication(/* .revalidate_p1d4_publication( */", 1))


def reverse_lr12_order(blobs: dict[str, bytes]) -> None:
    def reverse(value: str) -> str:
        value = value.replace("commit_reconstructed_transition(", "TEMP_RECONSTRUCT(", 1)
        value = value.replace("complete_stage8b_p1d3_semantic(", "commit_reconstructed_transition(", 1)
        return value.replace("TEMP_RECONSTRUCT(", "complete_stage8b_p1d3_semantic(", 1)
    mutate_section(blobs, "redis_source", "pub async fn resume_stage8b_p1d3_pre_ack_with_redis(", "pub async fn resume_stage8b_p1d3_dispatch_limit_with_redis(", reverse)


add("source-lr10-revalidation-omitted", omit_lr10_revalidation, digest=False)
add("source-lr10-double-revalidation", double_lr10_revalidation, digest=False)
add("source-lr12-effect-order-reversed", reverse_lr12_order, digest=False)
add("regression-dirty-source-allowed", lambda b: mutate_json(b, "regression", lambda v: v["tested_source_contract"].update(clean_before=False)))
add("regression-evidence-tree-unbound", lambda b: mutate_json(b, "regression", lambda v: v["tested_source_contract"].update(evidence_ref_tree_must_equal_tested_source=False)))
add("regression-retained-output-optional", lambda b: mutate_json(b, "regression", lambda v: v["retained_evidence"].update(new_absolute_output_argument_required=False)))
add("regression-output-inside-repository", lambda b: mutate_json(b, "regression", lambda v: v["retained_evidence"].update(output_must_be_outside_repository=False)))
add("regression-failure-evidence-discarded", lambda b: mutate_json(b, "regression", lambda v: v["retained_evidence"].update(pass_and_fail_runs_retained=False)))
add("regression-script-binding-omitted", lambda b: mutate_json(b, "regression", lambda v: v["accepted_script_bindings"].pop(checker.FILES["regression_script"])))
add("regression-short-exact-test-name", lambda b: mutate_json(b, "regression", lambda v: v["cross_slice_exact_tests"].__setitem__(0, "p1d2_market_feedback_commits_ack_then_truth_then_xacks_source")))
add("regression-exact-test-missing", lambda b: mutate_json(b, "regression", lambda v: v["cross_slice_exact_tests"].pop()))
add("regression-zero-selected-accepted", lambda b: mutate_json(b, "regression", lambda v: v["exact_test_protocol"].update(selected_count_required=0)))
add("gate-failure-finalization-omitted", lambda b: b.update(regression_script=b["regression_script"].replace(b"--status FAIL", b"--status LOST")))
add("gate-exact-list-check-omitted", lambda b: b.update(regression_script=b["regression_script"].replace(b"-- --list --exact", b"-- --list")))
add("scope-gate-scripts-made-mutable", lambda b: b.update(scope_script=b["scope_script"].replace(b'"docs/stage-8/stage8b-p1e-i0-",', b'"docs/stage-8/stage8b-p1e-i0-",\n    "scripts/stage8b_p1e_i0_",')))
add("retained-atomic-rename-omitted", lambda b: b.update(retained_script=b["retained_script"].replace(b"os.rename(temporary, output)", b"output.mkdir()")))
add("gate-mandatory-source-negative-omitted", lambda b: b.update(regression_script=b["regression_script"].replace(b"run_logged env PYTHONPATH=scripts python3 scripts/stage8b_p1d4_source_negative_harness.py", b"true # source negative omitted")))
add("gate-clean-source-check-omitted", lambda b: b.update(regression_script=b["regression_script"].replace(b"git status --porcelain --untracked-files=all", b"printf clean")))
add("retained-source-manifest-omitted", lambda b: b.update(retained_script=b["retained_script"].replace(b'write_json(temporary / "source-tree-manifest.json", source_manifest)', b"pass # source manifest omitted")))

if len(integrity_cases) != 8 or len(semantic_cases) != 45:
    raise SystemExit(f"R10 mutation inventory drifted: integrity={len(integrity_cases)} semantic={len(semantic_cases)}")

escaped: list[str] = []
for name, blobs in integrity_cases:
    try:
        checker.validate(blobs, check_integrity=True)
    except Exception:
        print(f"PASS integrity {name}")
    else:
        escaped.append(name)
        print(f"FAIL integrity {name}")

for name, blobs in semantic_cases:
    try:
        checker.validate(blobs, check_integrity=False)
    except Exception:
        print(f"PASS semantic-redigested {name}")
    else:
        escaped.append(name)
        print(f"FAIL semantic-redigested {name}")

if escaped:
    raise SystemExit("R10 mutations escaped: " + ", ".join(escaped))
print("PASS stage8b-p1e-r10-design-negative-harness integrity=8/8 semantic-redigested=45/45 total=53/53")
