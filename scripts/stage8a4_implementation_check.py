#!/usr/bin/env python3
"""Fail-closed Stage 8A-4 implementation R3 semantic checker."""

from __future__ import annotations

import csv
import json
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BASE = "cc58c10d22db312cd83640f1c1e7fd86861a4594"
BRANCH = "stage8a4-reconciliation-implementation"
REVIEW_SHA256 = "43315b4653482998f0d112adbdcfc857afde8d1b68de94b3663b929c1ebad99e"
R1 = "245fea18f3f22bd4233eed4f9207445efd0a6d46"
R1_REVIEW_SHA256 = "acd5c63481409348fef6908541b1f493b096750d53d447e832284b73d7059c9b"
R2_SPEC_SHA256 = "85623d2c2e9ac32f6efd689edf001afd6ce528f0e29ccac188659b056c29309b"
R2 = "3c445aef6dce3f38a81ee477eaa73e56ffdc0a80"
R2_REVIEW_SHA256 = "49140b266c58f165c0645e0b6b4ae49c52886cb0c675993e3a34324f1b672290"
R3_SPEC_SHA256 = "bc10b746b487d47be3edd2ae4b72d1a4405222513855731bd7a22e1f7beab94f"

AUTHORITY = Path("docs/stage-8/stage8a4-implementation-authority.json")
CONTRACT = Path("docs/stage-8/stage8a4-implementation-contract.md")
DESCRIPTOR = Path("docs/stage-8/stage8a4-implementation-descriptor.json")
MATRIX = Path("docs/stage-8/STAGE8A_4_IMPLEMENTATION_R3_ACCEPTANCE_MATRIX_2026-08-15.csv")
NEGATIVE = Path("docs/stage-8/STAGE8A_4_IMPLEMENTATION_R3_NEGATIVE_INVENTORY_2026-08-15.md")
SOURCE = Path("crates/finam-gateway/src/stage8a4_reconciliation.rs")
TESTS = Path("crates/finam-gateway/src/stage8a4_reconciliation/tests.rs")
LIB = Path("crates/finam-gateway/src/lib.rs")
CURRENT_STATUS = Path("docs/current-status.md")
ROADMAP = Path("docs/roadmap.md")

ALLOWED_CHANGED_PATHS = {
    str(AUTHORITY), str(CONTRACT), str(DESCRIPTOR), str(MATRIX), str(NEGATIVE),
    str(SOURCE), str(TESTS), str(LIB), str(CURRENT_STATUS), str(ROADMAP),
    "README.md",
    "scripts/stage8a4_implementation_check.py",
    "scripts/stage8a4_implementation_negative_harness.py",
    "scripts/stage8a4_implementation_proof_map.py",
    "scripts/stage8a4_implementation_gate.sh",
    "scripts/stage8a4_implementation_handoff_safety_check.py",
    "scripts/make_stage8a4_implementation_handoff_archive.py",
}


class CheckFailure(RuntimeError):
    pass


def require(condition: bool, message: str) -> None:
    if not condition:
        raise CheckFailure(message)


def changed_paths() -> set[str]:
    tracked = subprocess.check_output(
        ["git", "diff", "--name-only", BASE, "--"], cwd=ROOT, text=True
    ).splitlines()
    untracked = subprocess.check_output(
        ["git", "ls-files", "--others", "--exclude-standard"], cwd=ROOT, text=True
    ).splitlines()
    return {value for value in tracked + untracked if value}


def check(root: Path = ROOT, *, git_scope: bool = True) -> None:
    authority = json.loads((root / AUTHORITY).read_text())
    require(authority["schema_version"] == 1, "authority schema drift")
    require(authority["stage"] == "8A-4-implementation-R3", "stage drift")
    require(
        authority["status"] == "implementation_r3_independent_acceptance_pending",
        "candidate status drift",
    )
    require(authority["accepted_design_ref"] == BASE, "accepted design ref drift")
    require(authority["accepted_design_review_sha256"] == REVIEW_SHA256, "review hash drift")
    require(authority["rejected_implementation_r1_ref"] == R1, "R1 baseline drift")
    require(authority["implementation_r1_review_sha256"] == R1_REVIEW_SHA256, "R1 review hash drift")
    require(authority["implementation_r2_correction_spec_sha256"] == R2_SPEC_SHA256, "R2 specification hash drift")
    require(authority["rejected_implementation_r2_ref"] == R2, "R2 baseline drift")
    require(authority["implementation_r2_review_sha256"] == R2_REVIEW_SHA256, "R2 review hash drift")
    require(authority["implementation_r3_correction_spec_sha256"] == R3_SPEC_SHA256, "R3 specification hash drift")
    require(authority["canonical_truth_type"] == "broker_core::BrokerTruthSnapshot", "truth type drift")
    require(authority["implementation_kind"] == "pure_deterministic_reducer", "implementation kind drift")
    for field in (
        "opaque_inputs", "source_completeness_encoded_as_types",
        "source_specific_orders_snapshot", "source_specific_bounded_trade_history",
        "source_specific_instrument_resolution", "deterministic_bounded_interval_split",
        "broker_trade_id_deduplication", "orthogonal_lifecycle_and_fill",
    ):
        require(authority[field] is True, f"authority disabled: {field}")
    require(authority["public_input_constructors"] is False, "public constructor enabled")
    require(authority["exact_order_lookup_replaces_account_safety"] is False, "exact lookup replaced safety")
    for field in (
        "exact_order_lookup_has_typed_request_timing",
        "admission_context_policy_pair_bound",
        "source_evidence_canonical_payload_bound",
        "selected_order_exact_identity_checked_at_every_tier",
        "supporting_trade_secondary_identity_checked",
        "material_trade_summary_excludes_non_material_receipt_time",
        "non_exact_diagnostic_request_bound",
        "supporting_trade_durable_identity_checked",
        "admission_failure_diagnostic_request_bound",
    ):
        require(authority[field] is True, f"R3 invariant disabled: {field}")
    require(
        authority["deterministic_tier_precedence"] == [
            "exact_client_order_id", "known_broker_order_id",
            "fully_bound_order_shape_and_event_window",
        ],
        "tier precedence drift",
    )
    require(authority["outcomes"] == ["ExactOrderState", "Conflict", "StillUnknown"], "outcome algebra drift")
    require(authority["proven_no_match_available"] is False, "ProvenNoMatch opened")
    require(authority["retry_authority_available"] is False, "retry authority opened")
    require(authority["send_authority_available"] is False, "send authority opened")
    require(authority["focused_test_count"] == 30, "focused test count drift")
    require(authority["compile_fail_doctest_count"] == 3, "compile-fail count drift")
    require(authority["negative_case_count"] == 55, "negative count drift")
    expected_closed = {
        "durable_apply_or_journal_bridge", "ack_or_readiness_publication",
        "redis_live_consumer", "broker_dispatch", "finam_post_delete",
        "same_request_retry_or_resend", "runtime_live", "real_orders",
        "stage8a5", "stage8b",
    }
    require(set(authority["closed"]) == expected_closed, "closed surface inventory drift")
    require(all(authority["closed"].values()), "closed surface opened")

    descriptor = json.loads((root / DESCRIPTOR).read_text())
    require(descriptor["stage"] == authority["stage"], "descriptor stage drift")
    require(descriptor["branch"] == BRANCH, "descriptor branch drift")
    require(descriptor["accepted_design_ref"] == BASE, "descriptor design ref drift")
    require(descriptor["rejected_implementation_r1_ref"] == R1, "descriptor R1 ref drift")
    require(descriptor["rejected_implementation_r2_ref"] == R2, "descriptor R2 ref drift")
    require(descriptor["production_module"] == str(SOURCE), "module descriptor drift")
    require(descriptor["focused_tests"] == str(TESTS), "test descriptor drift")
    require(descriptor["pure"] is True and descriptor["deterministic"] is True, "purity drift")
    require(descriptor["restart_replay_stable"] is True, "replay stability drift")
    require(descriptor["diagnostic_only"] is True, "diagnostic boundary opened")
    for field in (
        "linear_authority_tuple_revalidated", "exact_get_timing_typed",
        "identity_conflicts_fail_closed", "duplicate_material_binding_order_independent",
        "durable_trade_identity_conflicts_fail_closed", "admission_failures_are_attempt_bound",
    ):
        require(descriptor[field] is True, f"descriptor R3 invariant disabled: {field}")
    for field in ("network_calls", "redis_calls", "journal_writes", "send_or_retry_capabilities"):
        require(descriptor[field] == 0, f"descriptor opened {field}")

    contract = (root / CONTRACT).read_text()
    contract_lower = contract.lower()
    for marker in (
        "pure reconciliation reducer", "BrokerTruthSnapshot", "no public constructors",
        "Saturated intervals are never complete", "Exact GET-order observation",
        "BrokerTradeId", "Shuffled source rows", "exact durable-binding",
        "HTTP request-start", "material view", "contradictory",
        "FINAM POST/DELETE", "Stage 8A-5", "Stage 8B",
    ):
        require(marker in contract, f"contract marker missing: {marker}")
    require("durable apply/journal" in contract_lower, "contract marker missing: durable apply/journal")

    source = (root / SOURCE).read_text()
    tests = (root / TESTS).read_text()
    lib = (root / LIB).read_text()
    require("mod stage8a4_reconciliation;" in lib, "module is not private")
    require("pub mod stage8a4_reconciliation" not in lib, "module made public")
    require("admit_stage8a4_broker_truth" in lib and "reduce_stage8a4_reconciliation" in lib, "reexports missing")
    for type_name in (
        "Stage8a4DurableRequestContext", "Stage8a4ReconciliationPolicy",
        "Stage8a4SourceTiming", "Stage8a4NonPaginatedOrdersSnapshotComplete",
        "Stage8a4CompletePositionsSnapshot", "Stage8a4InstrumentCompletenessEvidence",
        "Stage8a4BoundedTradeHistoryComplete", "Stage8a4SourceEvidence",
        "Stage8a4FreshTruthAdmission",
    ):
        require(re.search(rf"pub (?:struct|enum) {type_name}\b", source) is not None, f"opaque type missing: {type_name}")
    require(source.count("```compile_fail") == 3, "compile-fail doctest drift")
    require(tests.count("#[test]") == 30, "focused test inventory drift")
    admission_binding = source.split('b"stage8a4-complete-admission-v2"', 1)[1].split(");", 1)[0]
    require("context.durable_binding_sha256.as_bytes()" in admission_binding, "durable context not admission-bound")
    require("policy.policy_binding_sha256.as_bytes()" in admission_binding, "policy not admission-bound")
    require("source_evidence_binding_sha256.as_bytes()" in admission_binding, "source evidence not admission-bound")
    require("admitted_durable_binding_sha256: String" in source, "admitted durable binding missing")
    require("admitted_policy_binding_sha256: String" in source, "admitted policy binding missing")
    require("source_evidence_binding_sha256: String" in source, "source binding missing")
    require("admission.admitted_durable_binding_sha256 != context.durable_binding_sha256" in source, "context cross-pair guard missing")
    require("admission.admitted_policy_binding_sha256 != policy.policy_binding_sha256" in source, "policy cross-pair guard missing")
    require("evidence.canonical_truth_payload_sha256 != canonical_truth_sha256" in source, "canonical payload equality missing")
    require("!= truth.trades.len()" in source, "raw trade row count is not exact")
    require("pub struct Stage8a4ExactOrderObservation" in source, "typed exact-order timing missing")
    require("validate_timing(&exact_source.timing, context, policy, &attempt)?" in source, "exact timing not validated")
    require("received.push(exact.timing.response_received_at)" in source, "exact timing absent from skew")
    require("fn selected_order_identity(" in source, "selected identity validator missing")
    require("Tier3Match::IdentityConflict" in source, "tier3 identity conflict missing")
    require("fn classify_trade_support(" in source, "trade support classifier missing")
    require(
        "if broker_conflict\n        || client_conflict" in source,
        "secondary trade identity unchecked",
    )
    require("|| durable_broker_conflict" in source, "durable broker trade identity unchecked")
    require("|| durable_client_conflict" in source, "durable client trade identity unchecked")
    require(
        "if !(broker_match || client_match || durable_broker_match || durable_client_match)" in source,
        "unrelated trade classification drift",
    )
    require("Stage8a4MaterialTradeBinding" in source, "material trade binding missing")
    material_block = source.split("struct Stage8a4MaterialTradeBinding", 1)[1].split("impl<'a>", 1)[0]
    require("received_ts" not in material_block, "non-material received_ts leaked into summary")
    require('b"stage8a4-deduplicated-material-trades-v2"' in source, "material summary domain missing")
    require("fn canonical_truth_binding(" in source, "canonical truth multiset binding missing")
    source_evidence_block = source.split('b"stage8a4-source-evidence-binding-v2"', 1)[1].split("to_hex", 1)[0]
    require("evidence.canonical_truth_payload_sha256" in source_evidence_block, "source attempt payload claim unbound")
    require('b"stage8a4-bound-non-exact-semantic-v2"' in source, "non-exact binding missing")
    non_exact_block = source.split('b"stage8a4-bound-non-exact-semantic-v2"', 1)[1].split(");", 1)[0]
    require("context.durable_binding_sha256" in non_exact_block, "non-exact context unbound")
    require("policy.policy_binding_sha256" in non_exact_block, "non-exact policy unbound")
    require("admission.truth_binding_sha256" in non_exact_block, "non-exact admission unbound")
    require("admission.source_evidence_binding_sha256" in non_exact_block, "non-exact source unbound")
    require('b"stage8a4-bound-admission-failure-semantic-v3"' in source, "admission failure binding missing")
    admission_failure_block = source.split('b"stage8a4-bound-admission-failure-semantic-v3"', 1)[1].split(");", 1)[0]
    for marker in (
        "attempt.durable_binding_sha256", "attempt.request_id",
        "attempt.policy_binding_sha256", "attempt.canonical_truth_sha256",
        "attempt.source_evidence_binding_sha256",
    ):
        require(marker in admission_failure_block, f"admission failure attempt unbound: {marker}")
    require("returned_count >= interval.requested_limit" in source, "saturation fail-close weakened")
    require("interval.split_depth > policy.max_interval_split_depth" in source, "split-depth guard missing")
    require("fn deterministic_interval_split(" in source, "deterministic split missing")
    require(
        "BTreeMap<String, &BrokerTradeSnapshot>" in source
        and "trade.broker_trade_id.as_str().to_string()" in source,
        "BrokerTradeId dedup map missing",
    )
    require("Stage8a4ExactLifecycle::TerminalCancelled" in source, "cancel lifecycle missing")
    require("Stage8a4ExactLifecycle::TerminalExpired" in source, "expiry lifecycle missing")
    require("Stage8a4ReconciliationReason::UnknownOrderStatus" in source, "unknown status fail-close missing")
    require(source.count("Stage8a4ReconciliationReason::TradeIdentityConflict") >= 2, "trade conflict mapping drift")
    require(source.count("Stage8a4ReconciliationReason::ExactIdentityDisagreement") >= 6, "identity disagreement mapping drift")
    require(source.count("Stage8a4ReconciliationReason::UnknownOrderStatus") == 2, "unknown status mapping drift")
    require("retry_authorized: false" in source and "send_authorized: false" in source, "diagnostic authority opened")
    require("retry_authorized: true" not in source and "send_authorized: true" not in source, "true send/retry authority found")
    require("values.push(exact)" in source and "truth.orders.push(" not in source, "exact lookup replaced account safety")
    require("fn same_material_trade(" in source, "material trade comparison missing")
    require("shuffled_orders_and_duplicate_ordering_are_byte_stable" in tests, "determinism test missing")
    require("exact_lookup_does_not_replace_account_wide_safety_snapshot" in tests, "account safety test missing")
    require("identical_trade_duplicates_count_once_and_conflicting_duplicates_fail" in tests, "trade dedup test missing")
    require("saturated_or_gapped_trade_history_is_not_admitted_even_with_identical_timestamps" in tests, "saturation test missing")
    require("unknown_status_and_missing_exact_shape_remain_unknown" in tests, "unknown/missing shape test missing")
    for test_name in (
        "admission_cannot_be_reduced_with_another_durable_context",
        "admission_cannot_be_reduced_with_another_policy",
        "source_evidence_cannot_be_paired_with_another_canonical_payload",
        "exact_get_request_started_before_possible_effect_is_rejected",
        "exact_get_staleness_and_cross_source_skew_are_rejected",
        "tier1_client_match_cannot_hide_broker_id_contradiction",
        "tier2_broker_match_cannot_hide_client_id_contradiction",
        "tier3_shape_cannot_override_explicit_client_id_contradiction",
        "supporting_trade_secondary_exact_identity_contradiction_is_conflict",
        "equal_material_duplicate_receipt_order_is_byte_stable",
        "identical_context_policy_truth_tuple_replays_byte_stably",
        "non_exact_diagnostic_is_byte_stable_under_canonical_row_reordering",
        "supporting_trade_must_match_durable_ids_even_when_selected_order_omits_one",
        "admission_failure_diagnostic_is_bound_to_request_attempt",
    ):
        require(test_name in tests, f"R3 focused test missing: {test_name}")
    forbidden_source = (
        "reqwest::", "redis::", ".post(", ".delete(", "CancelBrokerTruth",
        "M3d2", "ProvenNoMatch", "dispatch_order", "send_order", "retry_order",
    )
    for token in forbidden_source:
        require(token not in source, f"forbidden implementation token: {token}")
    require(re.search(r"impl Stage8a4(?:DurableRequestContext|ReconciliationPolicy|SourceEvidence)[\s\S]*?pub fn (?:new|from|build|issue)", source) is None, "public input constructor found")

    with (root / MATRIX).open(newline="") as stream:
        rows = list(csv.DictReader(stream))
    require(len(rows) == 90, "acceptance matrix must contain exactly 90 rows")
    require([row["id"] for row in rows] == [f"I{i:03d}" for i in range(1, 91)], "matrix IDs drift")
    negative = (root / NEGATIVE).read_text()
    require(len(re.findall(r"^\d+\.", negative, re.MULTILINE)) == 55, "negative inventory must contain 55 cases")

    status = (root / CURRENT_STATUS).read_text()
    require("## Current accepted boundary" in status, "leading status authority missing")
    leading = status.split("## Current accepted boundary", 1)[1].split("\n## ", 1)[0]
    for marker in (BASE, R1, R2, "Implementation R3", "acceptance is pending",
                   "Durable-composition planning remains", "FINAM POST/DELETE",
                   "retry/resend", "runtime-live", "Stage 8A-5+", "Stage 8B"):
        require(marker in leading, f"leading status authority missing: {marker}")
    require("Design R2 is the only active candidate" not in leading, "stale Design R2 authority restored")
    roadmap = (root / ROADMAP).read_text()
    require("cc58c10" in roadmap and "Implementation R3" in roadmap, "roadmap active slice drift")
    require("FINAM POST/DELETE" in roadmap and "runtime-live" in roadmap, "roadmap closed surfaces missing")

    if git_scope:
        require(subprocess.check_output(["git", "branch", "--show-current"], cwd=ROOT, text=True).strip() == BRANCH, "wrong branch")
        paths = changed_paths()
        require(paths == ALLOWED_CHANGED_PATHS, f"changed-path scope drift: {sorted(paths ^ ALLOWED_CHANGED_PATHS)}")
        require(not any(path.startswith(".github/") for path in paths), "workflow changed")
        require("Cargo.toml" not in paths and "Cargo.lock" not in paths, "Cargo surface changed")


def main() -> None:
    try:
        check()
    except (CheckFailure, KeyError, json.JSONDecodeError, OSError, subprocess.CalledProcessError) as error:
        print(f"stage8a4-implementation-check: FAIL {error}", file=sys.stderr)
        raise SystemExit(1)
    print("stage8a4-implementation-check: PASS stage=R3 rows=90 focused-tests=30 negatives=55 compile-fail=3")


if __name__ == "__main__":
    main()
