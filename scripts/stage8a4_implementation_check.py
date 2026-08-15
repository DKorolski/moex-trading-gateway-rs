#!/usr/bin/env python3
"""Fail-closed Stage 8A-4 implementation R1 semantic checker."""

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

AUTHORITY = Path("docs/stage-8/stage8a4-implementation-authority.json")
CONTRACT = Path("docs/stage-8/stage8a4-implementation-contract.md")
DESCRIPTOR = Path("docs/stage-8/stage8a4-implementation-descriptor.json")
MATRIX = Path("docs/stage-8/STAGE8A_4_IMPLEMENTATION_R1_ACCEPTANCE_MATRIX_2026-08-15.csv")
NEGATIVE = Path("docs/stage-8/STAGE8A_4_IMPLEMENTATION_R1_NEGATIVE_INVENTORY_2026-08-15.md")
SOURCE = Path("crates/finam-gateway/src/stage8a4_reconciliation.rs")
TESTS = Path("crates/finam-gateway/src/stage8a4_reconciliation/tests.rs")
LIB = Path("crates/finam-gateway/src/lib.rs")
CURRENT_STATUS = Path("docs/current-status.md")
ROADMAP = Path("docs/roadmap.md")

ALLOWED_CHANGED_PATHS = {
    str(AUTHORITY), str(CONTRACT), str(DESCRIPTOR), str(MATRIX), str(NEGATIVE),
    str(SOURCE), str(TESTS), str(LIB), str(CURRENT_STATUS), str(ROADMAP),
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
    require(authority["stage"] == "8A-4-implementation-R1", "stage drift")
    require(
        authority["status"] == "implementation_r1_independent_acceptance_pending",
        "candidate status drift",
    )
    require(authority["accepted_design_ref"] == BASE, "accepted design ref drift")
    require(authority["accepted_design_review_sha256"] == REVIEW_SHA256, "review hash drift")
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
    require(authority["focused_test_count"] == 16, "focused test count drift")
    require(authority["compile_fail_doctest_count"] == 3, "compile-fail count drift")
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
    require(descriptor["production_module"] == str(SOURCE), "module descriptor drift")
    require(descriptor["focused_tests"] == str(TESTS), "test descriptor drift")
    require(descriptor["pure"] is True and descriptor["deterministic"] is True, "purity drift")
    require(descriptor["restart_replay_stable"] is True, "replay stability drift")
    require(descriptor["diagnostic_only"] is True, "diagnostic boundary opened")
    for field in ("network_calls", "redis_calls", "journal_writes", "send_or_retry_capabilities"):
        require(descriptor[field] == 0, f"descriptor opened {field}")

    contract = (root / CONTRACT).read_text()
    contract_lower = contract.lower()
    for marker in (
        "pure reconciliation reducer", "BrokerTruthSnapshot", "no public constructors",
        "Saturated intervals are never complete", "Exact GET-order observation",
        "BrokerTradeId", "Shuffled source rows",
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
    require(tests.count("#[test]") == 16, "focused test inventory drift")
    admission_binding = source.split('b"stage8a4-complete-admission-v1"', 1)[1].split(");", 1)[0]
    require("context.durable_binding_sha256.as_bytes()" in admission_binding, "durable context not admission-bound")
    require("policy.policy_binding_sha256.as_bytes()" in admission_binding, "policy not admission-bound")
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
    require(source.count("Stage8a4ReconciliationReason::TradeIdentityConflict") == 1, "trade conflict mapping drift")
    require(source.count("Stage8a4ReconciliationReason::ExactIdentityDisagreement") == 4, "identity disagreement mapping drift")
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
    forbidden_source = (
        "reqwest::", "redis::", ".post(", ".delete(", "CancelBrokerTruth",
        "M3d2", "ProvenNoMatch", "dispatch_order", "send_order", "retry_order",
    )
    for token in forbidden_source:
        require(token not in source, f"forbidden implementation token: {token}")
    require(re.search(r"impl Stage8a4(?:DurableRequestContext|ReconciliationPolicy|SourceEvidence)[\s\S]*?pub fn (?:new|from|build|issue)", source) is None, "public input constructor found")

    with (root / MATRIX).open(newline="") as stream:
        rows = list(csv.DictReader(stream))
    require(len(rows) == 72, "acceptance matrix must contain exactly 72 rows")
    require([row["id"] for row in rows] == [f"I{i:03d}" for i in range(1, 73)], "matrix IDs drift")
    negative = (root / NEGATIVE).read_text()
    require(len(re.findall(r"^\d+\.", negative, re.MULTILINE)) == 40, "negative inventory must contain 40 cases")

    for status_path in (CURRENT_STATUS, ROADMAP):
        status = (root / status_path).read_text()
        require(BASE in status or "cc58c10" in status, f"accepted design missing: {status_path}")
        require("implementation R1" in status, f"active implementation missing: {status_path}")
        require("FINAM POST/DELETE" in status and "runtime-live" in status, f"closed surfaces missing: {status_path}")

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
    print("stage8a4-implementation-check: PASS rows=72 focused-tests=16 compile-fail=3")


if __name__ == "__main__":
    main()
