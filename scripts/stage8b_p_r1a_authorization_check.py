#!/usr/bin/env python3
"""Fail-closed checker for the Stage 8B-P R1A corrective contract."""

from __future__ import annotations

import argparse
import csv
import hashlib
import json
import re
import subprocess
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
D = ROOT / "docs/stage-8"
AUTHORITY = D / "stage8b-p-r1a-authorization-authority.json"
FRESHNESS = D / "stage8b-p-r1a-freshness-budget-authority.json"
NETWORK = D / "stage8b-p-r1a-network-policy-authority.json"
R1_AUTHORITY = D / "stage8b-p-r1-authorization-authority.json"
BUILD = D / "stage8b-p-build-identity-2026-08-23.json"
DESIGN = D / "STAGE8B_P_R1A_AUTHORIZATION_CONTRACT_2026-08-25.md"
MATRIX = D / "STAGE8B_P_R1A_ACCEPTANCE_MATRIX_2026-08-25.csv"
NEGATIVE = D / "STAGE8B_P_R1A_NEGATIVE_INVENTORY_2026-08-25.md"

R1_REF = "12a7aeec20824d3b90e18caa5961ba28a3eb7fd6"
MAIN_REF = "16a59bca74f94881c70d9fa39bbdf1c357e65f95"
R1_SHA = "7296c9ef77f2ec6b2f7ef014eab79e147f08753cf6062a78f5a6937da9b09132"
BUILD_SHA = "ca330934df540de69b52d0463a665a1ab0ff89fa13eeb663b162763cc6bc83a0"
FRESHNESS_SHA = "6f50b6e11292c2493c07fca11ad4e257190dad9941cb85ab6b8177091576d00f"
NETWORK_SHA = "02e3cb2429a82e5563a0c3135d7fbf9cd746198cc00f990b813a283ed35f6af8"
RENDERER_SHA = "24bc99b8e794ad85e7c83be7bd7d56cbc7568a01acdd4728785c2de600429d62"

COMMON_FIELDS = [
    "strategy_request_id", "durable_client_order_id", "operation",
    "process_boot_fingerprint_sha256", "keyed_account_binding_hmac_sha256",
    "account_key_generation_id", "execution_build_identity_sha256", "source_ref",
    "source_archive_sha256", "executable_sha256", "config_sha256", "policy_sha256",
    "config_policy_authority_sha256",
    "instrument_contract_sha256", "api_contract_sha256", "endpoint_renderer_sha256",
    "endpoint_identity_sha256", "network_policy_sha256", "stage7b_seal_generation",
    "stage6_checkpoint_fingerprint", "durable_budget_generation",
    "kill_switch_generation", "ownership_lease_fingerprint",
    "freshness_budget_authority_sha256", "run_expires_at_utc",
    "run_identity_sha256", "approved_pre_run_position",
]
PLACE_FIELDS = [
    "instrument", "side", "quantity", "order_type", "time_in_force",
    "limit_price_canonical_decimal", "max_notional_canonical_decimal",
    "place_request_body_sha256",
]
CANCEL_FIELDS = [
    "cancel_target_broker_order_id", "cancel_target_lifecycle_fingerprint",
    "cancel_target_strategy_request_id", "cancel_target_durable_client_order_id",
    "cancel_target_currently_working_proof_sha256", "cancel_request_body_sha256",
]
FORBIDDEN_ARM_FIELDS = [
    "operator_arm_nonce", "operator_arm_id", "issued_at_utc", "arm_expires_at_utc",
    "durable_arm_issuance_record", "covering_arm_seal",
]
SOURCE_BUDGETS = {
    "trusted_clock": {"max_age_ms": 1000, "max_future_skew_ms": 250},
    "readiness": {"max_age_ms": 1000, "max_future_skew_ms": 250},
    "current_control": {"max_age_ms": 1000, "max_future_skew_ms": 250},
    "ownership": {"max_age_ms": 1000, "max_future_skew_ms": 250},
    "schedule": {"max_age_ms": 5000, "max_future_skew_ms": 250},
    "instrument": {"max_age_ms": 5000, "max_future_skew_ms": 250},
    "account_orders": {"max_age_ms": 2000, "max_future_skew_ms": 250},
    "positions": {"max_age_ms": 5000, "max_future_skew_ms": 250},
    "trades": {"max_age_ms": 5000, "max_future_skew_ms": 250},
    "exact_order": {"max_age_ms": 1000, "max_future_skew_ms": 250},
    "api_snapshot": {"max_age_ms": 86400000, "max_future_skew_ms": 300000},
}


def require(value: bool, message: str) -> None:
    if not value:
        raise SystemExit(f"stage8b-p-r1a-authorization-check: FAIL {message}")


def sha(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--no-git", action="store_true")
    args = parser.parse_args()

    for path in (AUTHORITY, FRESHNESS, NETWORK, R1_AUTHORITY, BUILD, DESIGN, MATRIX, NEGATIVE):
        require(path.is_file(), f"missing {path.relative_to(ROOT)}")

    authority = json.loads(AUTHORITY.read_text())
    freshness = json.loads(FRESHNESS.read_text())
    network = json.loads(NETWORK.read_text())
    build = json.loads(BUILD.read_text())

    require(authority.get("schema_version") == 1, "authority schema drift")
    require(authority.get("stage") == "8B-P" and authority.get("revision") == "R1A", "stage/revision drift")
    require(authority.get("status") == "design_only_corrective_authorization_candidate", "status drift")
    require(authority.get("branch") == "stage8b-p-authorization-r1", "branch drift")

    lineage = authority.get("lineage", {})
    require(lineage == {
        "r1_candidate_ref": R1_REF,
        "r1_authority_sha256": R1_SHA,
        "accepted_main_ref": MAIN_REF,
        "accepted_stage8b_s_r3_ref": "afecc258",
        "r1_negative_cases_inherited": 48,
    }, "lineage drift")
    require(sha(R1_AUTHORITY) == R1_SHA, "R1 authority content drift")

    accepted = authority.get("accepted_execution_build", {})
    require(accepted.get("execution_build_identity_sha256") == BUILD_SHA == sha(BUILD), "full build identity drift")
    require(accepted.get("source_ref") == "6cb179509fad97e8be56e31bb930b2a86caefc6a", "build source drift")
    require(accepted.get("source_archive_sha256") == "1066ab44b32451f921f2d3cdd49118471f78b214de7dd848a3273c95e19143b6", "build archive drift")
    require(accepted.get("executable_sha256") == "677f277defb2591011486a061cb251264e3fd05bbc9f684b3ec9ff6ae55f3f06", "executable drift")
    require(accepted.get("exact_equality_required") is True, "exact build equality weakened")
    require(accepted.get("weaker_subset_reconstruction_allowed") is False, "weaker build reconstruction opened")

    bound = authority.get("bound_authorities", {})
    require(bound.get("freshness_budget_authority_path") == FRESHNESS.relative_to(ROOT).as_posix(), "freshness path drift")
    require(bound.get("freshness_budget_authority_sha256") == FRESHNESS_SHA == sha(FRESHNESS), "freshness digest drift")
    require(bound.get("network_policy_authority_path") == NETWORK.relative_to(ROOT).as_posix(), "network path drift")
    require(bound.get("network_policy_authority_sha256") == NETWORK_SHA == sha(NETWORK), "network digest drift")
    require(bound.get("caller_selected_authority_allowed") is False, "caller-selected authority opened")
    require(bound.get("missing_or_modified_authority_fails_closed") is True, "authority fail-close weakened")

    manifest = authority.get("canonical_manifest", {})
    require(manifest.get("schema_kind") == "closed_discriminated_union", "manifest not discriminated")
    require(manifest.get("operation_discriminator") == "operation", "operation discriminator drift")
    require(manifest.get("operation_values") == ["PLACE", "CANCEL"], "operation inventory drift")
    require(manifest.get("common_required_fields") == COMMON_FIELDS, "common field inventory drift")
    require(manifest.get("place_required_fields") == PLACE_FIELDS, "PLACE field inventory drift")
    require(manifest.get("cancel_required_fields") == CANCEL_FIELDS, "CANCEL field inventory drift")
    for key in ("unknown_fields_allowed", "irrelevant_variant_fields_allowed", "missing_fields_allowed", "flat_place_cancel_union_allowed", "conflated_limit_price_or_cancel_target_allowed"):
        require(manifest.get(key) is False, f"manifest boundary opened: {key}")
    all_fields = COMMON_FIELDS + PLACE_FIELDS + CANCEL_FIELDS
    require(len(all_fields) == len(set(all_fields)), "manifest fields overlap")
    require(not set(FORBIDDEN_ARM_FIELDS) & set(all_fields), "issued arm field in pre-arm manifest")

    boot = authority.get("process_boot_contract", {})
    require(boot.get("field") == "process_boot_fingerprint_sha256", "boot field drift")
    for key in ("exact_current_boot_match_required", "omission_fails_closed", "mutation_fails_closed", "cross_boot_substitution_fails_closed"):
        require(boot.get(key) is True, f"boot protection weakened: {key}")
    require(boot.get("restart_reuse_allowed") is False, "boot restart reuse opened")

    place = authority.get("place_contract", {})
    require(place.get("instrument") == "IMOEXF@RTSX", "PLACE instrument drift")
    require(place.get("order_type") == "ORDER_TYPE_LIMIT", "PLACE type drift")
    require(place.get("time_in_force") == "TIME_IN_FORCE_DAY", "PLACE TIF drift")
    require(place.get("quantity_canonical_decimal") == "1", "PLACE quantity drift")
    require(place.get("canonical_decimal_regex") == r"^(0|[1-9][0-9]*)(\.[0-9]*[1-9])?$", "decimal grammar drift")
    for key in (
        "positive_limit_price_required", "positive_max_notional_required",
        "checked_exact_decimal_multiplication_required", "noncanonical_decimal_fails_closed",
        "decimal_overflow_fails_closed", "price_times_quantity_exceeds_max_notional_fails_closed",
        "notional_check_before_attempt_append", "notional_recheck_immediately_before_k4_transport",
    ):
        require(place.get(key) is True, f"PLACE protection weakened: {key}")

    cancel = authority.get("cancel_contract", {})
    for key in (
        "exact_broker_order_id_required", "same_durable_lifecycle_required",
        "target_request_id_must_equal_common_request_id",
        "target_client_order_id_must_equal_common_client_order_id",
        "currently_working_proof_required",
    ):
        require(cancel.get(key) is True, f"CANCEL protection weakened: {key}")
    for key in ("account_wide_order_selection_allowed", "caller_selected_unproved_order_allowed"):
        require(cancel.get(key) is False, f"CANCEL selection opened: {key}")

    endpoint = authority.get("endpoint_and_network_contract", {})
    for key in ("endpoint_identity_sha256_required", "network_policy_sha256_required", "endpoint_identity_exact_formula_required", "operation_method_route_exact_match_required", "tls_required"):
        require(endpoint.get(key) is True, f"endpoint protection weakened: {key}")
    require(endpoint.get("exact_host") == "api.finam.ru", "endpoint host drift")
    require(endpoint.get("redirect_proxy_alternate_host_arbitrary_request_retry_allowed") is False, "network bypass opened")

    require(network.get("schema_version") == 1 and network.get("revision") == "R1A", "network schema drift")
    transport = network.get("transport", {})
    require(transport == {
        "tls_required": True, "scheme": "https", "exact_host": "api.finam.ru",
        "redirects_allowed": False, "proxy_allowed": False,
        "alternate_host_allowed": False, "arbitrary_request_api_allowed": False,
        "automatic_transport_retry_allowed": False,
    }, "network transport policy drift")
    require(network.get("operations") == {
        "PLACE": {"method": "POST", "route_template_id": "PlaceOrderV1", "route_template": "/v1/accounts/{account_id}/orders"},
        "CANCEL": {"method": "DELETE", "route_template_id": "CancelOrderV1", "route_template": "/v1/accounts/{account_id}/orders/{order_id}"},
    }, "endpoint operation policy drift")
    require(network.get("accepted_endpoint_renderer_sha256") == RENDERER_SHA, "renderer binding drift")
    identity = network.get("endpoint_identity", {})
    require(identity.get("algorithm") == "sha256" and identity.get("domain") == "moex-stage8b-endpoint-identity-v1", "endpoint identity domain drift")
    require(identity.get("canonical_formula") == "sha256(domain_nul_operation_nul_method_nul_route_template_id_nul_keyed_account_binding_hmac_sha256_nul_endpoint_renderer_sha256)", "endpoint identity formula drift")
    require(identity.get("required_components") == ["operation", "method", "route_template_id", "keyed_account_binding_hmac_sha256", "endpoint_renderer_sha256"], "endpoint identity components drift")
    require(identity.get("raw_account_id_in_digest_artifact_allowed") is False and identity.get("rendered_route_export_allowed") is False, "endpoint privacy opened")

    require(freshness.get("schema_version") == 1 and freshness.get("revision") == "R1A", "freshness schema drift")
    clock = freshness.get("clock_semantics", {})
    require(clock.get("age_reference") == "opaque_trusted_clock_now", "clock authority drift")
    require(clock.get("caller_selected_now_allowed") is False and clock.get("caller_selected_budget_allowed") is False, "caller clock/budget opened")
    require(freshness.get("source_budgets") == SOURCE_BUDGETS, "numeric freshness budget drift")
    cross = freshness.get("cross_source_budgets", {})
    require(cross.get("control_sources") == {"members": ["readiness", "current_control", "ownership"], "max_skew_ms": 1000}, "control skew drift")
    require(cross.get("runtime_current_sources") == {"members": ["readiness", "current_control", "ownership", "schedule", "instrument", "account_orders", "positions", "trades", "exact_order"], "max_skew_ms": 5000}, "runtime skew drift")
    require(freshness.get("api_snapshot_cross_source_rule") == "age_bound_independently_not_compared_to_runtime_current_source_skew", "API snapshot skew rule drift")
    validation = freshness.get("validation", {})
    for key in (
        "missing_source_fails_closed", "missing_budget_fails_closed", "unknown_source_fails_closed",
        "zero_or_negative_budget_fails_closed", "stale_source_fails_closed",
        "future_source_beyond_budget_fails_closed", "cross_source_skew_exceeded_fails_closed",
        "modified_authority_digest_fails_closed",
    ):
        require(validation.get(key) is True, f"freshness fail-close weakened: {key}")
    require(validation.get("historical_ack_implies_current_readiness") is False, "historical ACK opened readiness")
    require(validation.get("earlier_successful_get_implies_current_readiness") is False, "historical GET opened readiness")

    pre_arm = authority.get("pre_arm_contract", {})
    require(pre_arm.get("issued_arm_shaped_fields_forbidden") == FORBIDDEN_ARM_FIELDS, "pre-arm forbidden inventory drift")
    require(pre_arm.get("arm_nonce_commitment_optional_non_authority") is True, "commitment semantics drift")
    require(pre_arm.get("actual_arm_metadata_created_only_at_k1") is True, "arm chronology drift")
    require(pre_arm.get("r1a_issues_arm") is False and pre_arm.get("r2_may_issue_arm") is False, "arm issuance opened")

    separation = authority.get("r2_k2_separation", {})
    for key in (
        "r2_readonly_preflight_evidence_equals_k2_fresh_sources",
        "r2_evidence_convertible_to_k2_authority",
        "r2_evidence_satisfies_k1_or_k2_freshness",
        "r2_evidence_carryable_into_xe_as_current_truth",
        "direct_r2_evidence_promotion_to_transport_allowed",
    ):
        require(separation.get(key) is False, f"R2/K2 separation opened: {key}")
    require(separation.get("fresh_reread_and_reduction_after_arm_at_k2_required") is True, "post-arm K2 reread removed")

    authorization = authority.get("authorization", {})
    require(authorization.get("status") == "NOT_ISSUED", "authorization issued")
    for key in ("exact_run_manifest_present", "broker_readonly_get_sent", "operator_arm_issued", "dispatch_attempt_recorded", "transport_entered", "finam_post_delete_sent", "broker_effect", "stage8b_p_open", "stage8b_xe_open"):
        require(authorization.get(key) is False, f"authorization surface opened: {key}")
    require(authorization.get("next_if_independently_accepted") == "Stage8B-P-R2 operator-selected GET-only preflight", "next step drift")
    closed = authority.get("closed_surfaces", {})
    require(len(closed) == 14 and all(value is True for value in closed.values()), "closed surface opened")

    with MATRIX.open(newline="") as handle:
        rows = list(csv.DictReader(handle))
    require(len(rows) == 64, "acceptance row count drift")
    require([row["id"] for row in rows] == [f"P1A-{index:03d}" for index in range(1, 65)], "acceptance ID drift")
    require(all(row.get("status") == "PASS" for row in rows), "acceptance matrix not green")
    require(len(re.findall(r"^\d+\. ", NEGATIVE.read_text(), flags=re.MULTILINE)) == 50, "negative inventory drift")
    require(authority.get("acceptance_rows") == 64, "authority acceptance count drift")
    require(authority.get("r1a_negative_mutations") == 50 and authority.get("inherited_r1_negative_mutations") == 48 and authority.get("total_negative_mutations") == 98, "negative counts drift")

    design = DESIGN.read_text()
    for phrase in (
        "authorization `NOT_ISSUED`", "closed `PLACE | CANCEL` discriminated union",
        "process_boot_fingerprint_sha256", "max_notional_canonical_decimal",
        "attempt append", "immediately before K4",
        "R2ReadOnlyPreflightEvidence != Stage8bK2FreshSources",
        "may issue an arm", "Stage 8B-XE",
    ):
        require(phrase in design, f"design statement missing: {phrase}")

    if not args.no_git:
        merge_base = subprocess.run(["git", "merge-base", "HEAD", MAIN_REF], cwd=ROOT, check=True, text=True, capture_output=True).stdout.strip()
        require(merge_base == MAIN_REF, "accepted main is not ancestor")
        changed = subprocess.run(
            ["git", "diff", "--name-only", MAIN_REF, "--", "Cargo.toml", "Cargo.lock", "crates", "config", ".github/workflows"],
            cwd=ROOT, check=True, text=True, capture_output=True,
        ).stdout.splitlines()
        require(not changed, f"production/config/workflow drift: {changed}")

    print("stage8b-p-r1a-authorization-check: PASS rows=64 new_negatives=50 inherited=48 total=98 authorization=NOT_ISSUED broker_get=false arm=false transport=false finam=false")


if __name__ == "__main__":
    main()
