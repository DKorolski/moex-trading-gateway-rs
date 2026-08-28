#!/usr/bin/env python3
"""Executable-aware fail-closed checker for Stage 8B-P R2B Proposal R1."""

from __future__ import annotations

import csv
import hashlib
import json
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BASE = ROOT / "docs/stage-8"
HEX64 = re.compile(r"[0-9a-f]{64}")


def require(condition: bool, message: str) -> None:
    if not condition:
        raise RuntimeError(message)


def text(relative: str) -> str:
    return (ROOT / relative).read_text(encoding="utf-8")


def exact_hash(value: object) -> bool:
    return isinstance(value, str) and HEX64.fullmatch(value) is not None


def main() -> None:
    authority = json.loads((BASE / "stage8b-p-r2b-proposal-authority.json").read_text())
    runtime_contract_path = BASE / "stage8b-p-r2b-runtime-composition-contract.json"
    runtime_contract_bytes = runtime_contract_path.read_bytes()
    runtime_contract = json.loads(runtime_contract_bytes)
    build = json.loads((BASE / "stage8b-p-r2b-r1-build-evidence.json").read_text())
    proposal = text("docs/stage-8/STAGE8B_P_R2B_PROPOSAL_2026-08-27.md")
    closure = text("docs/stage-8/STAGE8B_P_R2A8_R1_ACCEPTANCE_CLOSURE_2026-08-27.md")
    status = text("docs/current-status.md")
    adapter = text("crates/finam-gateway/src/stage8b_r2a7_source_adapter.rs")
    gateway_lib = text("crates/finam-gateway/src/lib.rs")
    gateway_cargo = text("crates/finam-gateway/Cargo.toml")
    writer_bin = text("crates/finam-gateway/src/bin/stage8b-r2a8-production-current-source-writer.rs")
    helper_lib = text("tools/stage8b-readonly-preflight/src/lib.rs")
    helper_pipeline = text("tools/stage8b-readonly-preflight/src/r2a3.rs")
    helper_finalizer = text("tools/stage8b-readonly-preflight/src/r2a5.rs")

    require(authority["schema_version"] == 1, "schema drift")
    require(authority["stage"] == "Stage 8B-P R2B" and authority["revision"] == "R1", "stage/revision drift")
    require(authority["artifact_kind"] == "design_only_readonly_preflight_proposal", "artifact kind drift")
    require(authority["status"] == "PROPOSAL_ONLY_NOT_AUTHORIZED", "proposal status opened")
    require(authority["authorization_status"] == "NOT_ISSUED", "R2B authorization issued")
    require(authority["accepted_predecessor"] == {
        "stage": "Stage 8B-P R2A8-R1",
        "source_ref": "5b2079d7d524d2fa6f084f44f961c4b5958c042a",
        "archive_name": "moex-trading-project-5b2079d.zip",
        "archive_sha256": "903df69b800477706f4b2e95097fe84174f42e89b0a85a4b5fa94430619acb6a",
        "verdict": "ACCEPTED",
    }, "accepted predecessor drift")

    capability = authority["proposed_capability"]
    require(capability["one_shot"] and capability["operation_choices"] == ["PLACE", "CANCEL"], "one-shot operation drift")
    require(capability["selection_count"] == 1, "selection count drift")
    require(not capability["background_loop"] and not capability["unattended_execution"], "unattended surface opened")
    require(capability["result_may_influence_execution"] is False, "read evidence became execution authority")

    network = authority["network_contract"]
    require(network["scheme"] == "https" and network["exact_host"] == "api.finam.ru", "endpoint drift")
    require(network["outbound_destinations"] == ["api.finam.ru:443"], "destination allowlist drift")
    for field in ("dns_or_ip_rebinding_allowed", "redirects_allowed", "proxy_allowed", "automatic_retries_allowed", "order_post_allowed", "order_delete_allowed", "arbitrary_request_allowed"):
        require(network[field] is False, f"network closure drift: {field}")
    require(network["request_timeout_seconds"] == 10 and network["minimum_broker_get_interval_ms"] == 250, "timeout/rate drift")
    require([(x["ordinal"], x["method"], x["route_template"]) for x in network["auth_requests"]] == [
        (1, "POST", "/v1/sessions"), (2, "POST", "/v1/sessions/details")
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
    sequence = [
        "stage8b-r2a8-production-current-source-writer",
        "stage8b-r2a8-current-manifest-issuer",
        "stage8b-r2a7-source-adapter",
        "stage8b-r2a5-authority-producer",
        "stage8b-r2a5-authority-issuer",
        "stage8b-r2a5-package-issuer",
        "accepted-stage8b-r2a5-launcher",
        "accepted-stage8b-readonly-preflight",
    ]
    require(composition["exact_executable_sequence"] == sequence, "exact writer-first sequence drift")
    require(composition["exact_invocation_cardinality"] == {name: (11 if name in {"stage8b-r2a5-authority-producer", "stage8b-r2a5-authority-issuer"} else 1) for name in sequence}, "cardinality drift")
    require(composition["exact_artifact_flow"] == [
        "signed_production_writer_intake->trusted_current_source",
        "trusted_current_source->reader_manifest",
        "reader_manifest->operational_authority_records",
        "operational_authority_records->signed_source_receipts",
        "signed_source_receipts->signed_run_package",
        "signed_run_package->fd_bound_launcher",
        "fd_bound_launcher->readonly_preflight_helper",
        "readonly_preflight_helper->durable_terminal_evidence",
    ], "artifact dependency flow drift")
    require(composition["fixture_features_allowed_in_production"] is False, "fixture feature entered production")
    require(composition["production_may_resolve_controlled_hash_domain"] is False, "controlled hash resolution opened")
    embedded = composition["embedded_runtime_composition_contract"]
    require(embedded == {
        "path": "docs/stage-8/stage8b-p-r2b-runtime-composition-contract.json",
        "sha256": hashlib.sha256(runtime_contract_bytes).hexdigest(),
        "contains_executable_hashes": False,
        "hash_cycle_prevented": True,
    }, "embedded runtime composition binding drift")
    require(runtime_contract["schema_version"] == 1 and runtime_contract["stage"] == "Stage 8B-P R2B" and runtime_contract["revision"] == "R1", "runtime composition identity drift")
    require(runtime_contract["authorization_status"] == "NOT_ISSUED", "embedded runtime composition authorized")
    require(runtime_contract["exact_executable_sequence"] == sequence, "embedded runtime sequence drift")
    require(runtime_contract["closed_surfaces"] == {
        "order_post": False, "order_delete": False, "broker_dispatch": False,
        "redis_live_consumer": False, "runtime_live": False, "real_orders": False,
    }, "embedded runtime closed surfaces drift")

    production_hashes = composition["production_linux_amd64_sha256"]
    controlled_hashes = composition["controlled_qualification_linux_amd64_sha256"]
    require(set(production_hashes) == set(sequence), "production hash inventory drift")
    require(set(controlled_hashes) == {"stage8b-r2a7-source-adapter", "stage8b-r2a8-current-manifest-issuer", "stage8b-r2a7-controlled-seeder"}, "controlled hash inventory drift")
    require(all(exact_hash(value) for value in production_hashes.values()), "production hash missing or malformed")
    require(all(exact_hash(value) for value in controlled_hashes.values()), "controlled hash missing or malformed")
    require(production_hashes["stage8b-r2a7-source-adapter"] != controlled_hashes["stage8b-r2a7-source-adapter"], "adapter hash domains collapsed")
    require(production_hashes["stage8b-r2a8-current-manifest-issuer"] != controlled_hashes["stage8b-r2a8-current-manifest-issuer"], "issuer hash domains collapsed")
    require(build["target"] == "x86_64-unknown-linux-gnu" and build["run_count"] == 2, "Linux build evidence drift")
    require(build["fixture_dependencies_in_production"] is False, "production fixture graph opened")
    require(build["controlled_place_regression"] == build["controlled_cancel_regression"] == build["linux_terminal_evidence_test"] == "PASS", "Linux regression evidence incomplete")
    require(build["authorization_status"] == "NOT_ISSUED", "build evidence authorized R2B")
    for field in ("finam_network_accessed", "finam_credentials_accessed", "order_post_sent", "order_delete_sent", "redis_live_accessed", "runtime_live_entered"):
        require(build[field] is False, f"build evidence opened closed surface: {field}")
    for relative, expected in build["source_sha256"].items():
        require(hashlib.sha256((ROOT / relative).read_bytes()).hexdigest() == expected, f"build source binding drift: {relative}")
    for name in ("stage8b-r2a8-production-current-source-writer", "stage8b-r2a8-current-manifest-issuer", "stage8b-r2a7-source-adapter", "accepted-stage8b-readonly-preflight"):
        record = build["production_binaries"][name]
        require(record["build_a_sha256"] == record["build_b_sha256"] == production_hashes[name] and record["reproducible"] is True, f"production reproducibility drift: {name}")
    for name, digest in controlled_hashes.items():
        record = build["controlled_qualification_binaries"][name]
        require(record["build_a_sha256"] == record["build_b_sha256"] == digest and record["reproducible"] is True, f"controlled reproducibility drift: {name}")

    writer = authority["production_current_source_writer"]
    require(writer["executable"] == sequence[0] and writer["uid"] == 8095 and writer["gid"] == 8095, "writer identity drift")
    require(writer["fixed_input"].endswith("/stage8b-r2a8-production-writer-intake.json"), "writer input drift")
    require(writer["fixed_output"].endswith("/stage8b-r2a8-trusted-current-source.json"), "writer output drift")
    for field in ("caller_arguments_allowed", "caller_paths_allowed", "caller_snapshots_allowed", "network_access_allowed", "finam_credential_access_allowed", "redis_access_allowed", "runtime_live_authority"):
        require(writer[field] is False, f"writer boundary opened: {field}")
    for field in ("signed_fixed_intake_required", "stage7b_owner_restart_required", "exact_durable_request_revalidation_required", "atomic_publication", "file_and_directory_fsync"):
        require(writer[field] is True, f"writer invariant absent: {field}")
    require("std::env::args_os().len() != 1" in writer_bin, "writer accepts arguments")
    require("run_stage8b_r2a8_production_current_source_writer" in writer_bin, "writer entrypoint detached")
    require("pub(crate) fn publish_stage8b_r2a8_trusted_current_source_from_owner(" in adapter, "owner seam is externally callable")
    require("publish_stage8b_r2a8_trusted_current_source_from_owner," not in gateway_lib, "owner seam re-exported")
    writer_body = adapter.split("pub fn run_stage8b_r2a8_production_current_source_writer", 1)[1].split("pub(crate) fn publish_stage8b_r2a8_trusted_current_source_from_owner", 1)[0]
    for marker in ("read_fixed_regular_file(", "Stage7bRecoveryReadyOwner::restart(", "single_exact_dispatch_ready_request()", "identity != intake.durable_request_identity", "publish_stage8b_r2a8_trusted_current_source_from_owner("):
        require(marker in writer_body, f"writer composition missing: {marker}")
    require("reqwest" not in writer_body and "PRODUCTION_CREDENTIALS" not in writer_body, "writer gained network/credentials")
    require('name = "stage8b-r2a8-production-current-source-writer"' in gateway_cargo, "writer Cargo target absent")

    query = authority["freshness_and_validation"]["query_policy"]
    require(query == {
        "policy_id": "stage8b-r2b-trades-single-page-v1", "method": "GET",
        "route_template": "/v1/accounts/{account_id}/trades",
        "trades_query_parameter_names": ["limit", "interval.start_time", "interval.end_time"],
        "trades_limit": 1000, "trades_window_ms": 86400000,
        "window_start_semantics": "request_requested_at_minus_window_inclusive",
        "window_end_semantics": "request_requested_at_inclusive",
        "time_basis": "request_requested_at", "timestamp_encoding": "RFC3339_seconds_UTC",
        "pagination": "single_page_no_cursor", "cursor_parameter": None,
        "full_page_means_incomplete": True, "caller_override_allowed": False,
    }, "query policy authority drift")
    for marker in (
        "pub const TRADES_LIMIT: usize = 1_000;", "pub const TRADES_WINDOW_MS: i64 = 24 * 60 * 60 * 1_000;",
        '.append_pair("limit", &TRADES_LIMIT.to_string())', '"interval.start_time"', '"interval.end_time"',
        "SecondsFormat::Secs", "if parsed.trades.len() >= TRADES_LIMIT",
    ):
        require(marker in helper_lib, f"query implementation drift: {marker}")

    evidence = authority["evidence_contract"]
    require(evidence["fixed_root"] == "/var/lib/moex-trading/stage8b/r2b-evidence", "evidence root drift")
    require(evidence["directory_uid"] == 0 and evidence["directory_gid"] == 8301 and evidence["directory_mode"] == "0730", "evidence directory custody drift")
    require(evidence["file_uid"] == 8301 and evidence["file_gid"] == 8301 and evidence["file_mode"] == "0640", "evidence file custody drift")
    for field in ("durable_local_evidence_required", "create_new", "no_follow", "single_link_required", "file_fsync", "directory_fsync", "one_terminal_record_per_nonce", "partial_attempts_preserved_on_failure"):
        require(evidence[field] is True, f"durable evidence invariant absent: {field}")
    for field in ("raw_body_recorded", "secret_recorded", "token_recorded", "account_id_recorded"):
        require(evidence[field] is False, f"sensitive evidence opened: {field}")
    attempt_schema = evidence["request_attempt_schema"]
    require(all(attempt_schema[field] is True for field in (
        "ordinal_required", "network_class_required", "method_required", "route_template_required",
        "request_started_at_required", "request_finished_at_required",
        "status_optional_on_transport_failure", "response_body_length_optional_on_transport_failure",
        "semantic_receipt_optional_on_transport_failure", "error_category_required_on_failed_attempt",
        "timeout_stage_required_on_timeout",
    )), "terminal request-attempt schema drift")
    require(attempt_schema["raw_body_exported"] is False, "failed attempt raw body opened")
    for marker in (
        "pub struct R2a3PipelineFailure", "pub attempts: Vec<R2a3AttemptEvidence>",
        "pub failed_attempt: Option<R2a3FailedAttemptEvidence>",
        "pub struct R2a3FailedAttemptEvidence", "failed_request = Some(evidence)",
        "execute_r2a3_pipeline_preserving_attempts",
    ):
        require(marker in helper_pipeline, f"partial-attempt preservation absent: {marker}")
    for marker in (
        "pub struct R2bTerminalEvidenceV1", "pub struct R2bRequestAttemptEvidenceV1",
        "R2B_RUNTIME_COMPOSITION_CONTRACT", "stage8b-p-r2b-runtime-composition-contract.json",
        "terminal_failed_attempt", "failure.failed_attempt.as_ref()",
        "fn persist_terminal_evidence_at(", "execute_r2a3_pipeline_preserving_attempts(",
        "let terminal = terminal_evidence(", "persist_terminal_evidence(&terminal)",
        "R2a3Error::EvidencePersistence", "terminal_evidence_is_create_new_single_link_and_non_replayable",
    ):
        require(marker in helper_finalizer, f"terminal evidence implementation absent: {marker}")
    persistence_body = helper_finalizer.split("fn persist_terminal_evidence_at(", 1)[1].split("fn persist_terminal_evidence(", 1)[0]
    for marker in (
        ".create_new(true)", "libc::O_CLOEXEC | libc::O_NOFOLLOW", "\n        || metadata.nlink() != 1",
        "pending.sync_all()", "std::fs::hard_link(&pending_path, &final_path)",
        "std::fs::remove_file(&pending_path)", "directory.sync_all()",
    ):
        require(marker in persistence_body, f"terminal evidence persistence absent: {marker}")
    run_body = helper_finalizer.split("pub async fn run_r2b_one_shot()", 1)[1].split("fn controlled_client_from_fixed_files", 1)[0]
    require(run_body.index("claim_nonce(") < run_body.index("execute_r2a3_pipeline_preserving_attempts(") < run_body.index("terminal_evidence(") < run_body.index("persist_terminal_evidence(") < run_body.index("result.map_err"), "terminal finalization order drift")

    credentials = authority["credential_contract"]
    require(credentials["root"] == "/run/credentials/moex-trading/stage8b/r2a5", "credential root drift")
    for field in ("credential_read_before_signed_package_validation", "credential_export_allowed", "token_export_allowed", "raw_account_export_allowed", "raw_response_export_allowed"):
        require(credentials[field] is False, f"credential closure drift: {field}")
    sandbox = authority["sandbox_contract"]
    require(sandbox["one_shot_service_required"] and sandbox["destination_allowlist_enforced_outside_process"], "sandbox enforcement absent")
    require(sandbox["filesystem_paths_fixed"] and not sandbox["caller_supplied_path_allowed"], "path authority opened")
    require(not sandbox["redis_access_allowed"] and not sandbox["runtime_socket_access_allowed"], "runtime connectivity opened")

    require(all(value is False for value in authority["closed_surfaces"].values()), "effect/live surface opened")
    require(authority["issuance_preconditions"] == {
        "independent_proposal_acceptance": "PENDING", "fresh_public_contract_refresh": "PENDING",
        "exact_linux_build_recheck": "PENDING", "production_service_sandbox_review": "PENDING",
        "credential_custody_review": "PENDING", "operator_selection": "ABSENT",
        "signed_run_package": "ABSENT", "r2b_authorization": "NOT_ISSUED",
    }, "issuance preconditions drift")
    for marker in ("R2A8-R1 is independently accepted", "Stage 8B-P R2B Proposal R1", "NOT_ISSUED", "FINAM network access", "POST/DELETE", "Redis live consumption", "runtime-live"):
        require(marker in status, f"authoritative status stale: {marker}")
    for marker in ("design-only proposal; `NOT_ISSUED`", "production current-source writer", "durable terminal evidence", "does not authorize or perform FINAM network access"):
        require(marker in proposal, f"proposal narrative drift: {marker}")
    require("Status: `ACCEPTED`" in closure and authority["accepted_predecessor"]["source_ref"] in closure, "acceptance closure drift")

    with (BASE / "STAGE8B_P_R2B_PROPOSAL_ACCEPTANCE_MATRIX_2026-08-27.csv").open(newline="") as handle:
        rows = list(csv.DictReader(handle))
    require(len(rows) == 30, "acceptance row count drift")
    require([row["id"] for row in rows] == [f"R2B-P-{index:03d}" for index in range(1, 31)], "acceptance ID drift")
    require(all(row["status"] == "PASS" for row in rows), "acceptance row not PASS")
    for row_id in ("R2B-P-009", "R2B-P-010", "R2B-P-021"):
        row = next(item for item in rows if item["id"] == row_id)
        require(row["evidence"] != "proposal authority", f"{row_id} is declaration-only")

    print("stage8b-p-r2b-proposal-check: PASS revision=R1 rows=30 production_writer=true hash_domains=separate durable_terminal=true authorization=NOT_ISSUED network=false post_delete=false runtime_live=false")


if __name__ == "__main__":
    try:
        main()
    except Exception as error:
        raise SystemExit(f"stage8b-p-r2b-proposal-check: FAIL {error}")
