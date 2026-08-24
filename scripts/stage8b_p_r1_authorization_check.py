#!/usr/bin/env python3
"""Fail-closed checker for the Stage 8B-P R1 design-only authorization package."""

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
AUTHORITY = D / "stage8b-p-r1-authorization-authority.json"
CONTRACT = D / "stage8b-p-finam-contract-snapshot-2026-08-24.json"
PREVIOUS_CONTRACT = D / "stage8b-p-finam-contract-snapshot-2026-08-23.json"
BUILD = D / "stage8b-p-build-identity-2026-08-23.json"
PRECONDITIONS = D / "stage8b-p-preconditions-authority.json"
GOVERNANCE = D / "stage8b-p-governance-observation-2026-08-23.json"
TLS = D / "stage8b-tls-qualification-authority.json"
DESIGN = D / "STAGE8B_P_R1_AUTHORIZATION_PACKAGE_2026-08-24.md"
MATRIX = D / "STAGE8B_P_R1_ACCEPTANCE_MATRIX_2026-08-24.csv"
NEGATIVE = D / "STAGE8B_P_R1_NEGATIVE_INVENTORY_2026-08-24.md"
REFRESH = ROOT / "scripts/stage8b_p_contract_refresh.py"
GATE = ROOT / "scripts/stage8b_p_r1_authorization_gate.sh"
HANDOFF = ROOT / "scripts/make_stage8b_p_r1_authorization_handoff.py"
HANDOFF_SAFETY = ROOT / "scripts/stage8b_p_r1_authorization_handoff_safety_check.py"

PREDECESSOR_REF = "16a59bca74f94881c70d9fa39bbdf1c357e65f95"
PREDECESSOR_TREE = "cc613dbf15858671eb6a0e5ee1435a2bc2b9f172"
SOURCE_REF = "6cb179509fad97e8be56e31bb930b2a86caefc6a"
SOURCE_TREE = "4900fd38d741ab24f643acf211e7d1f807d23792"
SOURCE_ARCHIVE_SHA = "1066ab44b32451f921f2d3cdd49118471f78b214de7dd848a3273c95e19143b6"
EXECUTABLE_SHA = "677f277defb2591011486a061cb251264e3fd05bbc9f684b3ec9ff6ae55f3f06"
FRESH_CONTRACT_SHA = "758bd1c27a179f83dd0908c2ee371d4c41d5fb8d7c3d62ca86aa045aad65db49"

MANIFEST_FIELDS = [
    "strategy_request_id",
    "durable_client_order_id",
    "operation",
    "account_hmac",
    "account_key_generation_id",
    "instrument",
    "side",
    "quantity",
    "order_type",
    "time_in_force",
    "limit_price_or_cancel_target",
    "source_ref",
    "source_archive_sha256",
    "executable_sha256",
    "config_sha256",
    "policy_sha256",
    "instrument_contract_sha256",
    "api_contract_sha256",
    "endpoint_renderer_sha256",
    "request_body_sha256",
    "stage7b_seal_generation",
    "stage6_checkpoint_fingerprint",
    "durable_budget_generation",
    "kill_switch_generation",
    "ownership_lease_fingerprint",
    "operator_arm_nonce",
    "issued_at_utc",
    "expires_at_utc",
    "approved_pre_run_position",
]

EXPECTED_RESPONSES = {
    "rest_place_order": (23736, "0fc4494e2f06a9bc8aebb10eb0a7de0500b661c9988a9fdfda526364348ff589"),
    "rest_cancel_order": (6727, "595f123796fca321e9027c81ea1dc54d61b85862b9a1031fea73eaa2ef92b63e"),
    "grpc_place_order": (59500, "3df67157308a1add9c27953912bd279ac66e6a715438c5034cec3a5b5d7bca12"),
    "grpc_get_order": (45948, "71cc118c771c9c960594f4e0cc3a0f2466ed76377c8f6e48a87a88d19df74dd8"),
    "rest_get_asset": (6421, "a7292fe5e0948bd926075baba3f1d9f318f380e3531e7d2f5b6698c353f9d6d3"),
    "rest_get_asset_params": (6552, "bb7c07ebadb6b3fdd0ed531ffab64aae91f547687348ffddc02209c46281b98d"),
    "rest_schedule": (5139, "9739d401763845a82c8a401b8e174694a0f6689cc760f54dc3b4792b4c1dd5d7"),
}


def require(value: bool, message: str) -> None:
    if not value:
        raise SystemExit(f"stage8b-p-r1-authorization-check: FAIL {message}")


def sha(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--no-git", action="store_true")
    args = parser.parse_args()

    for path in (
        AUTHORITY, CONTRACT, PREVIOUS_CONTRACT, BUILD, PRECONDITIONS,
        GOVERNANCE, TLS, DESIGN, MATRIX, NEGATIVE, REFRESH, GATE,
        HANDOFF, HANDOFF_SAFETY,
    ):
        require(path.is_file(), f"missing {path.relative_to(ROOT)}")

    authority = json.loads(AUTHORITY.read_text())
    contract = json.loads(CONTRACT.read_text())
    build = json.loads(BUILD.read_text())
    preconditions = json.loads(PRECONDITIONS.read_text())
    governance = json.loads(GOVERNANCE.read_text())
    tls = json.loads(TLS.read_text())

    require(authority.get("schema_version") == 1, "authority schema drift")
    require(authority.get("stage") == "8B-P" and authority.get("revision") == "R1", "stage/revision drift")
    require(authority.get("status") == "design_only_authorization_candidate", "authority status drift")
    require(authority.get("branch") == "stage8b-p-authorization-r1", "branch authority drift")

    predecessor = authority.get("accepted_predecessor", {})
    require(predecessor.get("main_ref") == PREDECESSOR_REF, "predecessor ref drift")
    require(predecessor.get("main_tree") == PREDECESSOR_TREE, "predecessor tree drift")
    require(predecessor.get("gov_p1_status") == "ACCEPTED_SOLO_MODE", "GOV-P1 status drift")
    require(predecessor.get("preconditions_authority_sha256") == sha(PRECONDITIONS), "preconditions digest drift")
    require(predecessor.get("governance_observation_sha256") == sha(GOVERNANCE), "governance digest drift")
    require(preconditions.get("revision") == "R4" and preconditions.get("status") == "gov_p1_solo_mode_accepted", "preconditions not accepted")
    require(governance.get("gov_p1_status") == "ACCEPTED_SOLO_MODE" and governance.get("compliant") is True, "governance not accepted")

    accepted = authority.get("accepted_transport_build", {})
    require(accepted.get("source_ref") == SOURCE_REF and accepted.get("source_tree") == SOURCE_TREE, "accepted source drift")
    require(accepted.get("source_archive") == "moex-trading-project-6cb1795.zip", "archive name drift")
    require(accepted.get("source_archive_sha256") == SOURCE_ARCHIVE_SHA, "archive digest drift")
    require(accepted.get("build_identity_sha256") == sha(BUILD), "build identity digest drift")
    require(accepted.get("tls_authority_sha256") == sha(TLS), "TLS authority digest drift")
    require(accepted.get("executable_sha256") == EXECUTABLE_SHA and accepted.get("executable_size") == 11202608, "executable identity drift")
    require(accepted.get("target_triple") == "aarch64-apple-darwin", "target drift")
    require(accepted.get("rust_release") == "1.95.0" and accepted.get("rust_commit") == "59807616e1fa2540724bfbac14d7976d7e4a3860", "Rust identity drift")
    require(accepted.get("legacy_actual_send_feature_broker_cli") is False, "broker-cli legacy send opened")
    require(accepted.get("legacy_actual_send_feature_finam_gateway") is False, "finam-gateway legacy send opened")
    require(accepted.get("production_code_drift_since_qualification") is False, "production drift claimed")
    require(build.get("source", {}).get("commit") == SOURCE_REF and build.get("source", {}).get("tree") == SOURCE_TREE, "accepted build source mismatch")
    require(build.get("source", {}).get("archive_sha256") == SOURCE_ARCHIVE_SHA, "accepted build archive mismatch")
    require(build.get("build", {}).get("executable_sha256") == EXECUTABLE_SHA, "accepted build executable mismatch")
    require(tls.get("accepted_predecessor_ref") == "14e01a9f838080e196ece5945a7796f2bd2600bc", "TLS lineage drift")

    fresh = authority.get("fresh_contract", {})
    require(fresh.get("snapshot") == CONTRACT.relative_to(ROOT).as_posix(), "fresh snapshot path drift")
    require(fresh.get("snapshot_sha256") == FRESH_CONTRACT_SHA == sha(CONTRACT), "fresh snapshot digest drift")
    require(fresh.get("retrieved_at_utc") == "2026-08-24T18:14:25Z", "fresh retrieval time drift")
    require(fresh.get("official_response_count") == 7 and fresh.get("all_http_200") is True, "fresh response evidence drift")
    require(fresh.get("all_hashes_identical_to_accepted_contract") is True and fresh.get("material_contract_drift") is False, "contract drift claimed")
    require(fresh.get("credentials_used") is False and fresh.get("broker_readonly_get_sent") is False and fresh.get("finam_order_request_sent") is False, "fresh retrieval opened broker surface")
    require(contract.get("snapshot_kind") == "stage8b_p_r1_fresh_normalized_finam_order_contract", "contract kind drift")
    responses = contract.get("retrieval", {}).get("responses", [])
    require(len(responses) == 7, "contract response count drift")
    for response in responses:
        expected = EXPECTED_RESPONSES.get(response.get("name"))
        require(expected is not None, "unknown contract response")
        require(response.get("http_status") == 200, "contract response not 200")
        require((response.get("bytes"), response.get("sha256")) == expected, f"contract response drift: {response.get('name')}")
        require(str(response.get("url", "")).startswith("https://api.finam.ru/docs/"), "non-official contract URL")
    comparison = contract.get("comparison", {})
    require(comparison.get("previous_refresh_sha256") == sha(PREVIOUS_CONTRACT), "previous contract digest drift")
    require(comparison.get("response_count") == 7 and comparison.get("byte_counts_identical") is True and comparison.get("response_sha256_identical") is True, "contract comparison drift")
    require(comparison.get("material_contract_drift") is False, "material contract drift")
    require(contract.get("place_order", {}).get("method") == "POST" and contract.get("place_order", {}).get("path") == "/v1/accounts/{account_id}/orders", "PLACE contract drift")
    require(contract.get("cancel_order", {}).get("method") == "DELETE" and contract.get("cancel_order", {}).get("path") == "/v1/accounts/{account_id}/orders/{order_id}", "CANCEL contract drift")
    require(contract.get("stage8b_p_authorized") is False and contract.get("operator_arm_issued") is False, "contract snapshot opened authorization")
    require(contract.get("broker_readonly_get_sent") is False and contract.get("finam_order_request_sent") is False, "contract snapshot opened broker request")
    refresh_text = REFRESH.read_text()
    for token in ('"curl"', '"--http1.1"', '"--connect-timeout"', '"10"', '"--max-time"', '"30"', '"--retry"', '"2"', '"--retry-all-errors"'):
        require(token in refresh_text, f"bounded public-doc refresh token missing: {token}")
    require("stage8b-p-contract-refresh: PASS responses=7 material_drift=false finam_request_sent=false" in refresh_text, "refresh terminal marker drift")
    gate_text = GATE.read_text()
    require("python3 scripts/stage8b_p_contract_refresh.py \\" in gate_text, "R1 refresh invocation missing")
    require(
        "--snapshot docs/stage-8/stage8b-p-finam-contract-snapshot-2026-08-24.json"
        in gate_text,
        "R1 gate does not refresh the R1 snapshot",
    )

    require(authority.get("exact_run_manifest_required_fields") == MANIFEST_FIELDS, "exact-run manifest inventory drift")
    policy = authority.get("future_exact_run_policy", {})
    require(policy == {
        "operation_count": 1,
        "allowed_operations": ["PLACE", "CANCEL"],
        "place_instrument": "IMOEXF@RTSX",
        "place_order_type": "ORDER_TYPE_LIMIT",
        "place_time_in_force": "TIME_IN_FORCE_DAY",
        "place_max_quantity": "1",
        "cancel_requires_exact_working_order_same_lifecycle": True,
        "market_allowed": False,
        "stop_sltp_bracket_allowed": False,
        "replace_multi_leg_allowed": False,
        "automatic_retry_allowed": False,
        "same_request_resend_allowed": False,
        "limit_cancel_pair_in_one_run_allowed": False,
    }, "future exact-run policy drift")

    preflight = authority.get("future_get_only_preflight", {})
    required_true = {
        "operator_selected_manifest_required", "local_secret_account_binding_required",
        "fresh_stage7b_seal_required", "fresh_stage6_dispatch_ready_command_required",
        "fresh_kill_switch_required", "fresh_schedule_required",
        "fresh_instrument_spec_required", "fresh_account_orders_positions_trades_required",
        "single_broker_ownership_required", "zero_ambiguity_or_unresolved_lifecycle_required",
        "pre_run_position_baseline_required",
    }
    required_false = {
        "caller_built_or_cached_snapshot_allowed", "preflight_may_issue_operator_arm",
        "preflight_may_record_dispatch_attempt", "preflight_may_enter_transport_boundary",
    }
    require(set(preflight) == required_true | required_false, "preflight key inventory drift")
    require(all(preflight.get(key) is True for key in required_true), "preflight requirement weakened")
    require(all(preflight.get(key) is False for key in required_false), "preflight opened effect")

    arm = authority.get("operator_arm_contract", {})
    require(arm == {
        "issued_by_this_package": False,
        "constructible_by_this_package": False,
        "one_shot": True,
        "request_keyed": True,
        "build_and_account_bound": True,
        "expires_before_transport": True,
        "clone_copy_serialize_allowed": False,
        "reconstructible_after_restart": False,
        "second_arm_for_same_request_allowed": False,
    }, "operator arm contract drift")
    authorization = authority.get("authorization", {})
    require(authorization == {
        "status": "NOT_ISSUED",
        "exact_operation_selected": False,
        "exact_run_manifest_present": False,
        "operator_arm_issued": False,
        "operator_go_present": False,
        "stage8b_p_open": False,
        "stage8b_xe_open": False,
        "next_if_accepted": "Stage8B-P-R2 exact operator-selected GET-only preflight package",
    }, "authorization boundary drift")
    closed = authority.get("closed_surfaces", {})
    require(set(closed) == {
        "broker_readonly_get", "operator_arm_issuance", "dispatch_attempt_recording",
        "finam_post_delete", "transport_boundary", "broker_effect",
        "redis_execution_consumer", "broker_dispatch", "runtime_live", "real_orders",
        "stage8b_xe", "stage11_execution_promotion", "stage12",
    }, "closed-surface inventory drift")
    require(all(value is True for value in closed.values()), "closed surface opened")

    with MATRIX.open(newline="") as handle:
        rows = list(csv.DictReader(handle))
    require(len(rows) == 55, "acceptance row count drift")
    require([row["id"] for row in rows] == [f"P1-{index:03d}" for index in range(1, 56)], "acceptance ID drift")
    require(all(row.get("status") == "PASS" for row in rows), "acceptance matrix not green")
    require(len(re.findall(r"^\d+\. ", NEGATIVE.read_text(), flags=re.MULTILINE)) == 48, "negative inventory drift")
    require(authority.get("acceptance_rows") == 55 and authority.get("negative_mutations") == 48, "authority counts drift")
    design = DESIGN.read_text()
    for phrase in ("Status: design-only authorization candidate", "`NOT_ISSUED`", "Stage 8B-P and", "No FINAM credentials were used", "separate R2 GET-only preflight package"):
        require(phrase in design, f"design boundary missing: {phrase}")

    if not args.no_git:
        merge_base = subprocess.run(
            ["git", "merge-base", "HEAD", PREDECESSOR_REF], cwd=ROOT,
            check=True, text=True, capture_output=True,
        ).stdout.strip()
        require(merge_base == PREDECESSOR_REF, "predecessor is not an ancestor")
        changed = subprocess.run(
            ["git", "diff", "--name-only", PREDECESSOR_REF, "--", "Cargo.toml", "Cargo.lock", "crates", "config", ".github/workflows"],
            cwd=ROOT, check=True, text=True, capture_output=True,
        ).stdout.splitlines()
        require(not changed, f"production/config/workflow drift: {changed}")

    print(
        "stage8b-p-r1-authorization-check: PASS rows=55 negatives=48 "
        "contract=7/7 drift=false authorization=NOT_ISSUED stage8b_p=false finam=false"
    )


if __name__ == "__main__":
    main()
