#!/usr/bin/env python3
"""Fail-closed checks for the Gate 7->8 R2 specification correction."""

from __future__ import annotations

import argparse
import csv
import hashlib
import json
import subprocess
from pathlib import Path

BASE_REJECTED = "4d1106e72bc1437d990a8bd949db4867d41c09b6"
R1_REVIEWED_NOT_ACCEPTED = "f7afc1c612c25de608783850ab2e8c0ae14b0687"
PREDECESSOR = R1_REVIEWED_NOT_ACCEPTED
ACCEPTED_STAGE7B = "a1044e0dbe324c722b637498ca80ffafd9f0cbee"
STAGE7B_CLOSURE = "7c3ffffcfec012f3c96c65a3fcaf366c1740b88e"
BRANCH = "gate7-to-8-spec"
REVIEW_SHA = "e66d87ae88cf1c1a8f2ec12ac5d4338374c26bfcc417624b0fa1f007a5c81bf2"
CLOSURE_DESCRIPTOR_SHA = "ee3e3555def13b5da6f699c4be62c2fff2c9bca1b82487036f7d9816b0a2c003"
ACCEPTANCE_RECORD_SHA = "f50b45318132124af0516d32df4f4fa4d358719a07e5cbe6603bead9caa52b1d"

DESCRIPTOR = Path("docs/stage-8/transition-gate-7-to-8-descriptor.json")
SPEC = Path("docs/stage-8/transition-gate-7-to-8-specification.md")
MATRIX = Path("docs/stage-8/GATE7_TO_8_R2_ACCEPTANCE_MATRIX_2026-08-14.csv")
OLD_MATRICES = {
    Path("docs/stage-8/GATE7_TO_8_R1_ACCEPTANCE_MATRIX_2026-08-14.csv"),
    Path("docs/stage-8/TRANSITION_GATE_7_TO_8_ACCEPTANCE_MATRIX_2026-08-14.csv"),
}
SLICE_PLAN = Path("docs/stage-8/stage8-slice-plan.md")
CONTRACT_SNAPSHOT = Path("docs/stage-8/finam-rest-order-contract-snapshot-2026-08-14.json")
CONTRACT_EVIDENCE = Path("docs/stage-8/finam-rest-order-contract-evidence-2026-08-14.json")
ORDER_REQUEST_SOURCE = Path("crates/broker-finam/src/order_request.rs")
ORDER_ENUM_FIXTURE = Path("crates/broker-finam/tests/fixtures/finam_spec/order_contract_enums_v2026_07_03.json")
CLOSURE_DESCRIPTOR = Path("docs/stage-7/stage7b-closure-descriptor.json")
ACCEPTANCE_RECORD = Path("docs/stage-7/stage7b-final-acceptance-record.md")

SNAPSHOT_SHA = "bf885782ffda757b2c2b9bdb01822c925ce08df983b7ff9779811f5365886bc6"
EVIDENCE_SHA = "5a434b0474844296566b3ad6e1a610d6b0fdd99d2ae90ac06de2cb4c9ce5d870"
MATRIX_SHA = "04667acfbf5df93e9937545cbb229fda0949aac0264e71f1bbd7ce8d8c994aec"
ORDER_REQUEST_SHA = "e57789a15d4a33fad08b93580d50c5efa8aba92ea4f547a45a898c6e300b80e6"
ORDER_ENUM_FIXTURE_SHA = "212ab404fbccc0a7bcb77a43a2ec73f460c7d90629b14daf00020eb9f6041dcf"

ALLOWED_DELTA = {
    "docs/current-status.md",
    "docs/roadmap.md",
    str(DESCRIPTOR),
    str(SPEC),
    str(MATRIX),
    *(str(path) for path in OLD_MATRICES),
    str(SLICE_PLAN),
    str(CONTRACT_SNAPSHOT),
    str(CONTRACT_EVIDENCE),
    "scripts/transition_gate_7_to_8_check.py",
    "scripts/transition_gate_7_to_8_negative_harness.py",
    "scripts/transition_gate_7_to_8.sh",
    "scripts/make_transition_gate_7_to_8_handoff_archive.py",
}


class GateFailure(ValueError):
    pass


def require(value: bool, message: str) -> None:
    if not value:
        raise GateFailure(message)


def sha(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def validate_descriptor(value: dict) -> None:
    require(value.get("schema_version") == 3, "descriptor schema drift")
    require(value.get("gate") == "Transition Gate 7->8", "gate identity drift")
    require(
        value.get("status") == "r2_specification_candidate_pending_independent_review",
        "Gate R2 self-accepted or status drift",
    )
    require(value.get("base_rejected_candidate") == BASE_REJECTED, "base candidate drift")
    require(value.get("reviewed_r1_not_accepted") == R1_REVIEWED_NOT_ACCEPTED, "reviewed R1 binding drift")
    binding = value.get("source_ref_binding", {})
    require(binding.get("required_branch") == BRANCH, "required branch drift")
    require(binding.get("required_predecessor") == PREDECESSOR, "R2 predecessor drift")
    accepted = value.get("accepted_stage7b", {})
    require(accepted.get("accepted_source_ref") == ACCEPTED_STAGE7B, "accepted Stage 7B ref drift")
    require(accepted.get("closure_record_ref") == STAGE7B_CLOSURE, "closure record ref drift")
    require(accepted.get("closure_descriptor_sha256") == CLOSURE_DESCRIPTOR_SHA, "closure descriptor binding drift")
    require(accepted.get("acceptance_record_sha256") == ACCEPTANCE_RECORD_SHA, "acceptance record binding drift")
    require(accepted.get("independent_review_sha256") == REVIEW_SHA, "independent review binding drift")

    contracts = value.get("contracts", {})
    require(contracts.get("specification") == str(SPEC), "specification path drift")
    require(contracts.get("acceptance_matrix") == {"path": str(MATRIX), "sha256": MATRIX_SHA}, "matrix binding drift")
    require(contracts.get("stage8_slice_plan") == str(SLICE_PLAN), "slice plan path drift")
    require(contracts.get("finam_contract_snapshot") == {"path": str(CONTRACT_SNAPSHOT), "sha256": SNAPSHOT_SHA}, "snapshot binding drift")
    require(contracts.get("finam_contract_evidence") == {"path": str(CONTRACT_EVIDENCE), "sha256": EVIDENCE_SHA}, "contract evidence binding drift")
    require(contracts.get("sole_place_serializer") == "broker_finam::build_place_order_request", "place serializer authority drift")
    require(contracts.get("sole_cancel_serializer") == "broker_finam::build_cancel_order_request", "cancel serializer authority drift")

    decision = value.get("decision_after_independent_acceptance", {})
    require(decision.get("stage8a_protected_adapter_and_reconciliation") == "implementation_authorized_no_send", "Stage 8A network boundary opened")
    require(decision.get("stage8b_bounded_real_execution") == "closed_pending_stage8a5_and_separate_acceptance", "Stage 8B opened")
    require(decision.get("stage9_continuous_reconciliation") == "closed", "Stage 9 opened")
    require(decision.get("stage10_runtime_live") == "closed", "Stage 10 opened")
    require(value.get("stage8a_slice_order") == ["8A-0", "8A-1", "8A-2", "8A-3", "8A-4", "8A-5"], "Stage 8A order drift")
    require(value.get("allowed_initial_commands") == ["PLACE_MARKET", "PLACE_LIMIT", "CANCEL"], "initial command allowlist drift")
    require(value.get("allowed_initial_time_in_force") == ["TIME_IN_FORCE_DAY"], "initial TIF is not Day-only")
    require(value.get("forbidden_initial_commands") == ["REPLACE", "STOP", "STOP_LIMIT", "SLTP", "BRACKET", "MULTI_LEG"], "forbidden command list drift")

    safety = value.get("safety_invariants", {})
    expected_safety = {
        "stable_client_order_id_required": True,
        "client_order_id_explicit_nonempty_max_20": True,
        "broker_generated_client_order_id_fallback": False,
        "durable_attempt_before_possible_send": True,
        "blind_retry_after_ambiguous_outcome": False,
        "fresh_broker_truth_required_for_reconciliation": True,
        "proven_no_match_constructible_in_stage8a": False,
        "max_nonfinal_lifecycles_per_strategy": 1,
        "max_live_engineering_micro_commands": 1,
        "autonomous_strategy_live_attachment": False,
        "simultaneous_alor_finam_live_for_same_strategy": False,
        "operator_arm_one_shot": True,
        "kill_switch_mechanism_required": True,
        "kill_switch_required_place_state": "RunAllowed",
        "kill_switch_unreadable_or_stale_fails_closed": True,
        "generic_all_4xx_classifier_allowed": False,
        "same_durable_request_reexecution_after_dispatch_allowed": False,
        "second_stage8_serializer_allowed": False,
    }
    require(safety == expected_safety, "safety invariant drift")
    require(value.get("endpoint_outcome_invariants") == {"cancel_401": "auth_readiness_block_disarm_target_unresolved_reconciliation_hold_fresh_truth_no_same_request_retry"}, "CANCEL 401 invariant drift")
    require(value.get("acceptance_row_count") == 68, "acceptance row count drift")
    require(value.get("negative_case_count") == 34, "negative case count drift")
    require(value.get("independent_acceptance_required") is True, "independent acceptance removed")
    surfaces = value.get("currently_open_surfaces", {})
    require(len(surfaces) == 9 and not any(surfaces.values()), "execution surface opened")


def validate_contract_snapshot(value: dict) -> None:
    require(value.get("schema_version") == 1, "contract snapshot schema drift")
    require(value.get("retrieved_at_utc") == "2026-08-14T15:00:32Z", "contract retrieval timestamp drift")
    source = value.get("official_source", {})
    require(source.get("rest_documentation_url") == "https://api.finam.ru/docs/rest/", "official REST source removed")
    place = value.get("place_order", {})
    cancel = value.get("cancel_order", {})
    require((place.get("method"), place.get("path")) == ("POST", "/v1/accounts/{account_id}/orders"), "PLACE endpoint drift")
    require((cancel.get("method"), cancel.get("path")) == ("DELETE", "/v1/accounts/{account_id}/orders/{order_id}"), "CANCEL endpoint drift")
    required_fields = {"symbol", "quantity", "side", "type", "time_in_force", "limit_price", "stop_price", "stop_condition", "legs", "client_order_id", "valid_before", "comment"}
    require(set(place.get("documented_body_fields", [])) == required_fields, "PLACE documented fields drift")
    statuses = ["200", "400", "401", "404", "429", "500", "503", "504", "default"]
    require(place.get("documented_response_statuses") == statuses, "PLACE response statuses drift")
    require(cancel.get("documented_response_statuses") == statuses, "CANCEL response statuses drift")
    client = place.get("client_order_id", {})
    require(client == {"documented_optional": True, "broker_auto_generates_when_omitted": True, "maximum_characters": 20}, "official client_order_id contract drift")
    policy = value.get("stage8_initial_policy", {})
    require(policy.get("allowed_time_in_force") == ["TIME_IN_FORCE_DAY"], "Stage 8 contract policy is not Day-only")
    require(policy.get("client_order_id_rule") == "explicit_exact_durable_nonempty_max_20_no_generated_fallback", "Stage 8 client_order_id rule drift")
    require(set(policy.get("forbidden_place_fields", [])) == {"stop_price", "stop_condition", "legs", "valid_before"}, "conditional field closure drift")
    require(policy.get("material_contract_drift_action") == "block_gate_and_require_separate_review", "material drift no longer blocks")


def validate_contract_evidence(value: dict, root: Path) -> None:
    require(value.get("evidence_kind") == "gate7_to_8_r1_finam_contract_refresh", "contract evidence kind drift")
    require(value.get("normalized_snapshot") == {"path": str(CONTRACT_SNAPSHOT), "sha256": SNAPSHOT_SHA}, "evidence snapshot binding drift")
    require(value.get("material_contract_drift") is False, "material FINAM contract drift detected")
    require(value.get("gate_decision") == "contract_refresh_passed_for_specification_only_no_stage8_code_authorized", "contract refresh opened implementation")
    contracts = {item.get("path"): item for item in value.get("project_contracts_reviewed", [])}
    require(contracts.get(str(ORDER_REQUEST_SOURCE), {}).get("sha256") == ORDER_REQUEST_SHA, "order request source evidence drift")
    require(contracts.get(str(ORDER_ENUM_FIXTURE), {}).get("sha256") == ORDER_ENUM_FIXTURE_SHA, "enum fixture evidence drift")
    findings = value.get("parity_findings", {})
    require(findings.get("place_path_matches") is True and findings.get("cancel_path_matches") is True, "endpoint parity failed")
    require(findings.get("client_order_id_is_explicitly_serialized") is True, "client_order_id explicit serialization lost")
    require(findings.get("required_stage8_action") == "enforce_TimeInForce_Day_in_capability_preflight_before_calling_existing_builder", "Day-only composition rule drift")
    require(findings.get("second_stage8_serializer_allowed") is False, "second serializer authorized")
    require(sha(root / ORDER_REQUEST_SOURCE) == ORDER_REQUEST_SHA, "vetted order builder changed")
    require(sha(root / ORDER_ENUM_FIXTURE) == ORDER_ENUM_FIXTURE_SHA, "pinned enum fixture changed")
    source = (root / ORDER_REQUEST_SOURCE).read_text()
    require("pub fn build_place_order_request(" in source, "sole PLACE builder missing")
    require("pub fn build_cancel_order_request(" in source, "sole CANCEL builder missing")


SPEC_TOKENS = [
    "Status: Gate 7→8 R2 specification candidate pending independent review.",
    "f7afc1c612c25de608783850ab2e8c0ae14b0687",
    "4d1106e72bc1437d990a8bd949db4867d41c09b6",
    "Stage8ExecutionCapability",
    "not implement `Clone`, `Copy`, `Serialize` or `Deserialize`",
    "broker_finam::build_place_order_request()",
    "broker_finam::build_cancel_order_request()",
    "A second Stage 8 FINAM JSON/request serializer is forbidden.",
    "TimeInForce::Day -> TIME_IN_FORCE_DAY",
    "non-empty, at most 20 characters",
    "PLACE classification is endpoint-specific:",
    "malformed or contradictory 400",
    "documented 404 account/instrument not found",
    "malformed, truncated or unknown 2xx",
    "CANCEL classification is separately endpoint-specific:",
    "documented 400 already executed",
    "documented 401 expired/invalid authentication",
    "target order remains unresolved",
    "no blind or same-request CANCEL retry",
    "documented 404 account/order not found",
    "undocumented 409 or 410",
    "A generic `all 4xx -> BrokerRejected` classifier is forbidden.",
    "Only a pre-send/local connect failure with proof that no bytes could leave",
    "that durable request's execution allowance is consumed permanently",
    "it never permits a second send-capable capability, arm or execution attempt for the same `StrategyRequestId` or durable request",
    "NEW `StrategyRequestId`, NEW derived `ClientOrderId`, NEW operator arm and NEW `Stage8ExecutionCapability`",
    "Same-request retry remains CLOSED",
    "`ProvenNoMatch` is CLOSED and unconstructible throughout Stage 8A.",
    "Empty, missing, stale or merely absent truth always remains `StillUnknown`",
    "Reconciliation never redispatches an old ambiguous request",
    "Conflict and still-unknown states block new live commands",
    "persistent kill-switch mechanism must be available, fresh and readable",
    "exactly `RunAllowed` before PLACE",
    "`StopRequested`, stale state, unreadable state or a generation conflict blocks PLACE",
    "immediately before transport",
    "same kill-switch mechanism",
    "ALOR and FINAM must not both have live execution authority",
    "historical Stage 5 `forbidden_surface_scan.sh` is not rebaselined here",
    "Stage 8-specific closed-surface scanner",
    "all 68 mandatory rows",
    "all exact 34 negative",
    "Stage 8 implementation CLOSED",
    "FINAM POST/DELETE CLOSED",
]


def validate_spec(text: str) -> None:
    text = " ".join(text.split())
    for token in SPEC_TOKENS:
        require(token in text, f"specification token missing: {token}")
    require("It does not authorize real FINAM POST/DELETE" in text, "real endpoint prohibition missing")
    require("No outcome after a possible send is automatically retried." in text, "blind retry prohibition missing")
    require("multiple candidates" in text and "no new live command" in text, "multiple-candidate closure missing")
    require(text.count("429, 500, 503, 504 or default | `ReconciliationRequired`") == 2, "PLACE/CANCEL ambiguous status tables drift")
    require("malformed, truncated or unknown 2xx, or 2xx without usable broker order identity | `ReconciliationRequired`" in text, "PLACE malformed 2xx classification drift")
    require("documented 401 expired/invalid authentication | disarm and authentication/readiness block; target order remains unresolved; `ReconciliationRequired` hold until fresh read-only broker truth; no blind or same-request CANCEL retry" in text, "CANCEL 401 endpoint policy drift")
    require("`DefinitelyNotSent` may prove that the attempt caused no broker effect, but it never permits a second send-capable capability" in text, "DefinitelyNotSent same-request closure drift")
    for forbidden in [
        "CANCEL 401 -> BrokerRejected",
        "same cancel request may be retried",
        "new arm may reuse the same durable request",
        "resend the same durable request",
    ]:
        require(forbidden not in text, f"unsafe R2 interpretation present: {forbidden}")


def validate_matrix(path: Path) -> None:
    with path.open(newline="") as handle:
        rows = list(csv.DictReader(handle))
    header = ["id", "category", "requirement", "expected", "mandatory", "evidence", "negative_mutation"]
    require(len(rows) == 68, "acceptance matrix is not 68 rows")
    require(list(rows[0]) == header, "matrix header drift")
    require([row["id"] for row in rows] == [f"G78R2-{index:03d}" for index in range(1, 69)], "matrix IDs missing reordered or duplicated")
    require(all(row["mandatory"] == "YES" for row in rows), "optional acceptance row introduced")
    require(all(all(row[field].strip() for field in header) for row in rows), "empty acceptance field")
    required_categories = {"predecessor", "scope", "finam_contract", "capability", "operator", "kill_switch", "ownership", "mapping", "outcome_place", "outcome_cancel", "outcome", "reconciliation", "limits", "micro", "evidence", "governance"}
    require({row["category"] for row in rows} == required_categories, "acceptance category drift")


def validate_slice_plan(text: str) -> None:
    normalized = " ".join(text.split())
    for token in [
        "Stage 8 implementation remains CLOSED",
        "8A-0 — current contract refresh",
        "8A-1 — protected capability",
        "8A-2 — builder composition",
        "8A-3 — endpoint classifier",
        "8A-4 — reconciliation",
        "8A-5 — aggregate acceptance",
        "It does not authorize a real FINAM POST/DELETE",
        "A second Stage 8 serializer is forbidden",
        "`ProvenNoMatch` unconstructible",
        "same fail-closed kill switch",
        "No later stage is opened by this plan.",
    ]:
        require(" ".join(token.split()) in normalized, f"slice plan token missing: {token}")


def git_output(root: Path, *args: str) -> str:
    return subprocess.check_output(["git", *args], cwd=root, text=True).strip()


def validate_git_scope(root: Path) -> None:
    require(git_output(root, "merge-base", "--is-ancestor", PREDECESSOR, "HEAD") == "", "reviewed R1 is not ancestor")
    tracked = set(filter(None, git_output(root, "diff", "--name-only", PREDECESSOR).splitlines()))
    untracked = set(filter(None, git_output(root, "ls-files", "--others", "--exclude-standard").splitlines()))
    changed = tracked | untracked
    require(changed, "empty Gate R2 correction delta")
    require(not (changed - ALLOWED_DELTA), f"out-of-scope path changed: {sorted(changed - ALLOWED_DELTA)}")
    require(not any(path.startswith(("crates/", ".github/")) or path in {"Cargo.toml", "Cargo.lock"} for path in changed), "production/Cargo/CI delta present")


def check(root: Path, *, check_git_scope: bool = True) -> None:
    require(sha(root / CLOSURE_DESCRIPTOR) == CLOSURE_DESCRIPTOR_SHA, "accepted closure descriptor changed")
    require(sha(root / ACCEPTANCE_RECORD) == ACCEPTANCE_RECORD_SHA, "accepted acceptance record changed")
    require(sha(root / CONTRACT_SNAPSHOT) == SNAPSHOT_SHA, "normalized FINAM snapshot hash drift")
    require(sha(root / CONTRACT_EVIDENCE) == EVIDENCE_SHA, "FINAM evidence hash drift")
    require(sha(root / MATRIX) == MATRIX_SHA, "acceptance matrix hash drift")
    require(not any((root / path).exists() for path in OLD_MATRICES), "obsolete pre-R2 matrix remains authoritative")
    validate_descriptor(json.loads((root / DESCRIPTOR).read_text()))
    validate_contract_snapshot(json.loads((root / CONTRACT_SNAPSHOT).read_text()))
    validate_contract_evidence(json.loads((root / CONTRACT_EVIDENCE).read_text()), root)
    validate_spec((root / SPEC).read_text())
    validate_matrix(root / MATRIX)
    validate_slice_plan((root / SLICE_PLAN).read_text())
    status = " ".join((root / "docs/current-status.md").read_text().split())
    roadmap = " ".join((root / "docs/roadmap.md").read_text().split())
    require("Transition Gate 7→8 R2 specification" in status, "current status not moved to R2")
    require("Stage 8 implementation remains CLOSED" in status, "current status opened Stage 8")
    require("Transition Gate 7→8 R2 specification" in roadmap, "roadmap R2 target missing")
    require("FINAM POST/DELETE remains closed" in roadmap, "roadmap endpoint boundary missing")
    if check_git_scope:
        validate_git_scope(root)
    print("transition-gate-7-to-8-check: PASS r2 rows=68 contract=current stage8a=no-send stage8b=closed")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path.cwd())
    parser.add_argument("--skip-git-scope", action="store_true")
    args = parser.parse_args()
    try:
        check(args.root.resolve(), check_git_scope=not args.skip_git_scope)
    except (GateFailure, FileNotFoundError, json.JSONDecodeError, csv.Error) as error:
        raise SystemExit(f"transition-gate-7-to-8-check: FAIL: {error}") from error


if __name__ == "__main__":
    main()
