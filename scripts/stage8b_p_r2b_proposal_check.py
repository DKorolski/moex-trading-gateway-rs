#!/usr/bin/env python3
"""Fail-closed checker for the design-only Stage 8B-P R2B proposal."""

from __future__ import annotations

import csv
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def require(condition: bool, message: str) -> None:
    if not condition:
        raise RuntimeError(message)


def main() -> None:
    base = ROOT / "docs/stage-8"
    authority = json.loads((base / "stage8b-p-r2b-proposal-authority.json").read_text())
    evidence = json.loads((base / "stage8b-p-r2a8-r1-causal-build-evidence.json").read_text())
    proposal = (base / "STAGE8B_P_R2B_PROPOSAL_2026-08-27.md").read_text()
    closure = (base / "STAGE8B_P_R2A8_R1_ACCEPTANCE_CLOSURE_2026-08-27.md").read_text()
    r2a3 = (ROOT / "tools/stage8b-readonly-preflight/src/r2a3.rs").read_text()
    r2a5 = (ROOT / "tools/stage8b-readonly-preflight/src/r2a5.rs").read_text()

    require(authority["schema_version"] == 1, "schema drift")
    require(authority["stage"] == "Stage 8B-P R2B", "stage drift")
    require(authority["artifact_kind"] == "design_only_readonly_preflight_proposal", "artifact kind drift")
    require(authority["status"] == "PROPOSAL_ONLY_NOT_AUTHORIZED", "proposal status opened")
    require(authority["authorization_status"] == "NOT_ISSUED", "R2B authorization issued")

    predecessor = authority["accepted_predecessor"]
    require(predecessor == {
        "stage": "Stage 8B-P R2A8-R1",
        "source_ref": "5b2079d7d524d2fa6f084f44f961c4b5958c042a",
        "archive_name": "moex-trading-project-5b2079d.zip",
        "archive_sha256": "903df69b800477706f4b2e95097fe84174f42e89b0a85a4b5fa94430619acb6a",
        "verdict": "ACCEPTED",
    }, "accepted predecessor drift")

    capability = authority["proposed_capability"]
    require(capability["one_shot"] is True, "one-shot boundary absent")
    require(capability["operation_choices"] == ["PLACE", "CANCEL"], "operation set drift")
    require(capability["selection_count"] == 1, "selection count drift")
    require(not capability["background_loop"] and not capability["unattended_execution"], "unattended surface opened")
    require(capability["result_may_influence_execution"] is False, "read evidence became execution authority")

    network = authority["network_contract"]
    require(network["scheme"] == "https" and network["exact_host"] == "api.finam.ru", "endpoint drift")
    require(network["outbound_destinations"] == ["api.finam.ru:443"], "destination allowlist drift")
    for field in ("dns_or_ip_rebinding_allowed", "redirects_allowed", "proxy_allowed", "automatic_retries_allowed", "order_post_allowed", "order_delete_allowed", "arbitrary_request_allowed"):
        require(network[field] is False, f"network closure drift: {field}")
    require(network["request_timeout_seconds"] == 10, "timeout drift")
    require(network["minimum_broker_get_interval_ms"] == 250, "rate-limit drift")
    require([(x["ordinal"], x["method"], x["route_template"]) for x in network["auth_requests"]] == [
        (1, "POST", "/v1/sessions"),
        (2, "POST", "/v1/sessions/details"),
    ], "authentication plan drift")
    require([(x["ordinal"], x["method"], x["route_template"]) for x in network["place_broker_truth_gets"]] == [
        (3, "GET", "/v1/accounts/{account_id}/orders"),
        (4, "GET", "/v1/accounts/{account_id}/trades"),
        (5, "GET", "/v1/accounts/{account_id}"),
    ], "PLACE GET plan drift")
    require([(x["ordinal"], x["method"], x["route_template"]) for x in network["cancel_broker_truth_gets"]] == [
        (3, "GET", "/v1/accounts/{account_id}/orders/{order_id}"),
        (4, "GET", "/v1/accounts/{account_id}/orders"),
        (5, "GET", "/v1/accounts/{account_id}/trades"),
        (6, "GET", "/v1/accounts/{account_id}"),
    ], "CANCEL GET plan drift")

    composition = authority["production_composition"]
    require(composition["fixture_features_allowed"] is False, "fixture feature entered production")
    require(len(composition["source_writer_sequence"]) == 7, "composition length drift")
    hashes = composition["exact_candidate_linux_amd64_sha256"]
    evidence_hashes = evidence["controlled_linux_amd64_binaries"]
    require(
        hashes == {name: evidence_hashes[name] for name in hashes}
        and "stage8b-r2a7-controlled-seeder" not in hashes,
        "candidate executable hash drift",
    )
    require(composition["source_adapter_build_a_sha256"] == composition["source_adapter_build_b_sha256"] == evidence["production_binaries"]["stage8b-r2a7-source-adapter"]["build_a_sha256"], "adapter reproducibility drift")
    require(composition["current_manifest_issuer_build_a_sha256"] == composition["current_manifest_issuer_build_b_sha256"] == evidence["production_binaries"]["stage8b-r2a8-current-manifest-issuer"]["build_a_sha256"], "issuer reproducibility drift")

    credentials = authority["credential_contract"]
    require(credentials["root"] == "/run/credentials/moex-trading/stage8b/r2a5", "credential root drift")
    require(credentials["account_id_file"] == "account-id" and credentials["readonly_secret_file"] == "finam-readonly-secret", "credential filename drift")
    for field in ("credential_read_before_signed_package_validation", "credential_export_allowed", "token_export_allowed", "raw_account_export_allowed", "raw_response_export_allowed"):
        require(credentials[field] is False, f"credential closure drift: {field}")

    sandbox = authority["sandbox_contract"]
    require(sandbox["one_shot_service_required"] is True and sandbox["destination_allowlist_enforced_outside_process"] is True, "sandbox enforcement absent")
    require(sandbox["filesystem_paths_fixed"] is True and sandbox["caller_supplied_path_allowed"] is False, "path authority opened")
    require(sandbox["redis_access_allowed"] is False and sandbox["runtime_socket_access_allowed"] is False, "runtime connectivity opened")

    freshness = authority["freshness_and_validation"]
    for field in ("signed_package_required", "typed_operator_decision_required", "fresh_public_contract_refresh_required", "exact_executable_hash_required", "trusted_current_source_required", "full_readiness_semantics_required", "token_account_binding_required", "strict_dto_decode_required", "orders_snapshot_complete_required", "full_trades_page_means_incomplete", "target_instrument_position_required", "unknown_status_fails_closed"):
        require(freshness[field] is True, f"validation requirement removed: {field}")
    require(freshness["manifest_expiry_seconds"] == 30 and freshness["trades_limit"] == 1000, "freshness/completeness budget drift")

    errors = authority["error_and_rate_limit_contract"]
    require(errors["http_non_200"] == "FAIL_CLOSED_NO_RETRY" and errors["timeout"] == "FAIL_CLOSED_NO_RETRY", "retry opened")
    require(errors["redirect"] == "FAIL_CLOSED" and errors["dto_error"] == "FAIL_CLOSED" and errors["incomplete_snapshot"] == "FAIL_CLOSED" and errors["ambiguous_lifecycle"] == "FAIL_CLOSED", "error taxonomy opened")

    redacted = authority["evidence_contract"]
    require(redacted["durable_local_evidence_required"] is True and redacted["immutable_handoff_required"] is True, "durable evidence absent")
    for field in ("raw_body_recorded", "secret_recorded", "token_recorded", "account_id_recorded"):
        require(redacted[field] is False, f"sensitive evidence opened: {field}")

    closed = authority["closed_surfaces"]
    require(all(value is False for value in closed.values()), "effect/live surface opened")
    issuance = authority["issuance_preconditions"]
    require(issuance == {
        "independent_proposal_acceptance": "PENDING",
        "fresh_public_contract_refresh": "PENDING",
        "exact_linux_build_recheck": "PENDING",
        "production_service_sandbox_review": "PENDING",
        "credential_custody_review": "PENDING",
        "operator_selection": "ABSENT",
        "signed_run_package": "ABSENT",
        "r2b_authorization": "NOT_ISSUED",
    }, "issuance preconditions drift")

    for marker in ("design-only proposal; `NOT_ISSUED`", "POST /v1/sessions", "GET-only", "remain forbidden", "does not authorize or perform FINAM network access"):
        require(marker in proposal, f"proposal narrative drift: {marker}")
    require("Status: `ACCEPTED`" in closure and predecessor["source_ref"] in closure, "acceptance closure drift")
    for marker in ('pub const PRODUCTION_BASE_URL: &str = "https://api.finam.ru"', 'template: "/v1/sessions"', 'template: "/v1/sessions/details"', "MIN_BROKER_GET_INTERVAL_MS"):
        require(marker in r2a3, f"qualified pipeline marker absent: {marker}")
    require('PRODUCTION_CREDENTIALS: &str = "/run/credentials/moex-trading/stage8b/r2a5"' in r2a5, "qualified credential root drift")

    with (base / "STAGE8B_P_R2B_PROPOSAL_ACCEPTANCE_MATRIX_2026-08-27.csv").open(newline="") as handle:
        rows = list(csv.DictReader(handle))
    require(len(rows) == 30, "acceptance row count drift")
    require([row["id"] for row in rows] == [f"R2B-P-{index:03d}" for index in range(1, 31)], "acceptance ID drift")
    require(all(row["status"] == "PASS" for row in rows), "acceptance row not PASS")

    print("stage8b-p-r2b-proposal-check: PASS rows=30 status=PROPOSAL_ONLY_NOT_AUTHORIZED authorization=NOT_ISSUED network=false post_delete=false runtime_live=false")


if __name__ == "__main__":
    try:
        main()
    except Exception as error:
        raise SystemExit(f"stage8b-p-r2b-proposal-check: FAIL {error}")
