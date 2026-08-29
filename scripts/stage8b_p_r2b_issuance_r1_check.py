#!/usr/bin/env python3
"""Fail-closed checker for Stage 8B-P R2B issuance-package R0-R1."""

from __future__ import annotations

import csv
import hashlib
import json
from pathlib import Path

import stage8b_p_r2b_read_contract_refresh as read_refresh


ROOT = Path(__file__).resolve().parents[1]
AUTHORITY = ROOT / "docs/stage-8/stage8b-p-r2b-issuance-package-r0-r1-authority.json"
MATRIX = ROOT / "docs/stage-8/STAGE8B_P_R2B_ISSUANCE_PACKAGE_R0_R1_ACCEPTANCE_MATRIX_2026-08-29.csv"
DESIGN = ROOT / "docs/stage-8/STAGE8B_P_R2B_ISSUANCE_PACKAGE_R0_R1_2026-08-29.md"
EVIDENCE = ROOT / "docs/stage-8/stage8b-p-r2b-issuance-package-r0-r1-evidence.json"
ABSENT_IMPLEMENTATION = (
    "deploy/stage8b-r2b/moex-stage8b-r2b-issuance.target",
    "deploy/stage8b-r2b/moex-stage8b-r2b-phase1-current-source.target",
    "deploy/stage8b-r2b/moex-stage8b-r2b-phase2-manifest-source.target",
    "deploy/stage8b-r2b/moex-stage8b-r2b-phase3-authority-producers.target",
    "deploy/stage8b-r2b/moex-stage8b-r2b-phase4-authority-issuers.target",
    "deploy/stage8b-r2b/moex-stage8b-r2b-phase5-run-package.target",
    "deploy/stage8b-r2b/moex-stage8b-r2b-phase6-readonly-preflight.target",
    "deploy/stage8b-r2b/moex-stage8b-r2b-run-package-draft-builder.service",
    "deploy/stage8b-r2b/moex-stage8b-r2b-package-issuer.service",
    "tools/stage8b-readonly-preflight/src/bin/stage8b-r2b-run-package-draft-builder.rs",
)
SNAPSHOT_SHA = "7c8e6bcd02f907af93ea1386499d03bff194da76a1eb2b19dd9c2ff1f97403c5"
READ_CONTRACT = {
    "snapshot_path": "docs/stage-8/stage8b-p-r2a3-finam-read-contract-snapshot.json",
    "snapshot_sha256": SNAPSHOT_SHA,
    "document_names": ["auth", "token_details", "get_account", "trades", "get_orders", "get_order"],
    "document_count": 6,
    "helper_embedded_snapshot_sha256": SNAPSHOT_SHA,
    "future_run_package_contract_snapshot_sha256": SNAPSHOT_SHA,
    "refresh_evidence": "docs/stage-8/stage8b-p-r2b-r0-r1-read-contract-refresh-evidence.json",
    "effect_contract_snapshot_sufficient_for_r2b": False,
    "activation_refresh_required": True,
    "activation_max_age_seconds": 1800,
}
FIXED_INPUTS = [
    {"name": "run_nonce", "path": "/run/moex-trading/stage8b/r2a5/run-nonce.sha256", "owner_uid": 0, "owner_gid": 0, "mode": "0644", "producer": "local_operator_activation_preparer", "run_nonce_binding": "SOURCE_OF_CURRENT_RUN_NONCE", "freshness_rule": "MUST_BE_NEW_AND_UNUSED_FOR_THIS_TRANSACTION"},
    {"name": "run_manifest", "path": "/var/lib/moex-trading/stage8b/r2a5/run-manifest.json", "owner_uid": 0, "owner_gid": 0, "mode": "0644", "producer": "local_operator_activation_preparer", "run_nonce_binding": "EXACT_CURRENT_RUN_IDENTITY", "freshness_rule": "MANIFEST_FIELDS_MATCH_CURRENT_NONCE_AUTHORITIES"},
    {"name": "trust_manifest", "path": "/etc/moex-trading/stage8b/r2a5/trust-manifest.json", "owner_uid": 0, "owner_gid": 0, "mode": "0644", "producer": "stage8b-r2a5-trust-ceremony", "run_nonce_binding": "NOT_NONCE_SCOPED", "freshness_rule": "ALL_KEY_GENERATIONS_VALID_AT_TRUSTED_NOW"},
    {"name": "account_key_manifest", "path": "/etc/moex-trading/stage8b/r2a5/account-key-manifest.json", "owner_uid": 0, "owner_gid": 0, "mode": "0644", "producer": "stage8b-r2a5-account-key-ceremony", "run_nonce_binding": "BOUND_BY_RUN_MANIFEST_HMAC_AND_GENERATION", "freshness_rule": "SELECTED_GENERATION_VALID_AT_TRUSTED_NOW"},
    {"name": "operator_decision", "path": "/etc/moex-trading/stage8b/r2a5/operator-decision.json", "owner_uid": 0, "owner_gid": 0, "mode": "0644", "producer": "local_operator_activation_preparer", "run_nonce_binding": "DECISION_HASH_BOUND_IN_CURRENT_DRAFT", "freshness_rule": "DECISION_CREATED_FOR_CURRENT_TRANSACTION_ONLY"},
    {"name": "accepted_helper_authority", "path": "/etc/moex-trading/stage8b/r2a5/accepted-helper-authority.json", "owner_uid": 0, "owner_gid": 0, "mode": "0644", "producer": "stage8b-r2a5-helper-acceptance-issuer", "run_nonce_binding": "NOT_NONCE_SCOPED", "freshness_rule": "SIGNATURE_AND_VALIDITY_WINDOW_CURRENT"},
    {"name": "authority_receipts", "path": "/run/moex-trading/stage8b/r2a5/receipts/{exact_source_name}/receipt.json", "owner_uid": "EXACT_ISSUER_UID_FROM_RECEIPT_INVENTORY", "owner_gid": "EXACT_ISSUER_GID_FROM_RECEIPT_INVENTORY", "mode": "0644", "producer": "EXACT_STAGE8B_R2A5_AUTHORITY_ISSUER_INSTANCE", "run_nonce_binding": "EVERY_RECEIPT_EQUALS_CURRENT_RUN_NONCE", "freshness_rule": "SOURCE_SPECIFIC_MAX_AGE_AND_CURRENT_GENERATION"},
]
SOURCE_DEFINITIONS = (
    ("trusted_clock", "Stage8bTrustedClockIssuer", "stage8b-trusted-clock-v1", 2000),
    ("stage7b_current_recovery_seal", "Stage7bRecoverySealReader", "stage7b-current-recovery-seal-v1", 2000),
    ("stage6_exact_dispatch_ready_command", "Stage6DispatchReadyCommandReader", "stage6-dispatch-ready-command-v1", 2000),
    ("stage8a_root_config_policy_control", "Stage8aCurrentControlIssuer", "stage8a-root-config-policy-control-v1", 2000),
    ("composite_readiness", "Stage8aCompositeReadinessIssuer", "stage8a-composite-readiness-v1", 2000),
    ("kill_switch_run_allowed", "Stage8aPersistentKillSwitchIssuer", "stage8a-kill-switch-run-allowed-v1", 2000),
    ("single_finam_ownership", "Stage8aSingleFinamOwnershipIssuer", "stage8a-single-finam-ownership-v1", 2000),
    ("schedule", "Stage8aScheduleIssuer", "stage8a-schedule-window-v1", 5000),
    ("instrument_specification", "Stage8aInstrumentIssuer", "stage8a-instrument-specification-v1", 5000),
    ("ambiguity_orphan_unresolved_lifecycle", "Stage8aLifecycleAmbiguityIssuer", "stage8a-lifecycle-ambiguity-v1", 2000),
    ("durable_micro_budget", "Stage8aDurableMicroBudgetIssuer", "stage8a-durable-micro-budget-v1", 2000),
)
RECEIPT_SOURCES = [
    {
        "source": source,
        "producer_service": f"stage8b-r2a5-producer@m8p{8101 + index}.service",
        "issuer_service": f"stage8b-r2a5-issuer@m8i{8201 + index}.service",
        "producer_uid": 8101 + index,
        "issuer_uid": 8201 + index,
        "receipt_path": f"/run/moex-trading/stage8b/r2a5/receipts/{source}/receipt.json",
        "authority_type": authority_type,
        "evidence_schema": schema,
        "max_age_ms": max_age,
        "same_run_nonce_required": True,
    }
    for index, (source, authority_type, schema, max_age) in enumerate(SOURCE_DEFINITIONS)
]
SIGNER = {
    "executable": "stage8b-r2a5-package-issuer",
    "service": "moex-stage8b-r2b-package-issuer.service",
    "uid": 0,
    "gid": 0,
    "fixed_input": "/var/lib/moex-trading/stage8b/r2a5/r2b-run-package.unsigned.json",
    "signing_key_path": "/run/credentials/moex-trading/stage8b/r2a5/package-authorization.ed25519",
    "signing_key_owner_uid": 0,
    "signing_key_owner_gid": 0,
    "signing_key_mode": "0600",
    "only_component_with_package_authorization_key": True,
    "requires_current_run_unsigned_draft": True,
    "output": {"path": "/etc/moex-trading/stage8b/r2a5/r2b-run-package.json", "owner_uid": 0, "owner_gid": 0, "mode": "0644", "atomic_publish": True, "file_fsync": True, "directory_fsync": True},
}
PRODUCERS = [item["producer_service"] for item in RECEIPT_SOURCES]
ISSUERS = [item["issuer_service"] for item in RECEIPT_SOURCES]
PHASES = [
    {"ordinal": 1, "name": "current_source_chain", "target": "moex-stage8b-r2b-phase1-current-source.target", "after_target": None, "services": ["moex-stage8b-r2a8-upstream-current-authority-publisher.service", "moex-stage8b-r2a8-authoritative-intake-creator.service", "moex-stage8b-r2a8-production-intake-stager.service", "moex-stage8b-r2a8-production-current-source-writer.service"]},
    {"ordinal": 2, "name": "manifest_and_source_adapter", "target": "moex-stage8b-r2b-phase2-manifest-source.target", "after_target": "moex-stage8b-r2b-phase1-current-source.target", "services": ["stage8b-r2a8-current-manifest-issuer.service", "stage8b-r2a7-source-adapter.service"]},
    {"ordinal": 3, "name": "authority_producers", "target": "moex-stage8b-r2b-phase3-authority-producers.target", "after_target": "moex-stage8b-r2b-phase2-manifest-source.target", "services": PRODUCERS},
    {"ordinal": 4, "name": "authority_issuers", "target": "moex-stage8b-r2b-phase4-authority-issuers.target", "after_target": "moex-stage8b-r2b-phase3-authority-producers.target", "services": ISSUERS},
    {"ordinal": 5, "name": "draft_and_signed_run_package", "target": "moex-stage8b-r2b-phase5-run-package.target", "after_target": "moex-stage8b-r2b-phase4-authority-issuers.target", "services": ["moex-stage8b-r2b-run-package-draft-builder.service", "moex-stage8b-r2b-package-issuer.service"]},
    {"ordinal": 6, "name": "root_admission_and_readonly_preflight", "target": "moex-stage8b-r2b-phase6-readonly-preflight.target", "after_target": "moex-stage8b-r2b-phase5-run-package.target", "services": ["moex-stage8b-r2b-readonly-supervisor.service"]},
]
MATRIX_SHA256 = "6d05f521312e8db547493b367e232dcf145ac0ff5c839aef70c8795478cd2681"
AUTHORITY_KEYS = {
    "schema_version", "stage", "revision", "status", "accepted_predecessor",
    "corrects_r0", "read_contract", "package_formation", "future_activation_target",
    "transaction", "implementation_state", "operator_local_inputs", "authorization",
    "closed_surfaces", "acceptance_rows", "targeted_negative_mutations",
}
CORRECTS_R0 = {
    "source_ref": "928168ed47e5b9dd873cd73815fbccecde7a8981",
    "verdict": "NOT_ACCEPTED_NARROW_CORRECTION_REQUIRED",
    "finding_count": 2,
}
PACKAGE_FORMATION_KEYS = {"selected_model", "builder", "signer"}
BUILDER_KEYS = {
    "executable", "service", "implemented_by_r0_r1", "uid", "gid",
    "signing_key_access", "credential_path_access", "network_allowed", "fixed_inputs",
    "receipt_sources", "required_receipt_count", "required_same_run_nonce",
    "stale_receipts_allowed", "controlled_fixture_producer_allowed", "schema",
    "signature_ed25519_hex", "validity_seconds", "output",
}
BUILDER_OUTPUT_KEYS = {
    "path", "owner_uid", "owner_gid", "mode", "atomic_publish",
    "nofollow_regular_single_link", "file_fsync", "directory_fsync",
    "existing_output_reuse_allowed",
}
FUTURE_ACTIVATION = {
    "unit": "moex-stage8b-r2b-issuance.target",
    "implemented_by_r0_r1": False,
    "installed_by_r0_r1": False,
    "enabled_by_r0_r1": False,
    "manual_start_allowed": False,
    "signed_local_activation_required": True,
}
TRANSACTION_KEYS = {"service_invocation_count", "phase_count", "phases", "barrier_contract"}
BARRIER_CONTRACT = {
    "phase_target_requires_all_phase_services": True,
    "phase_target_after_all_phase_services": True,
    "downstream_requires_previous_phase_target": True,
    "downstream_after_previous_phase_target": True,
    "failed_component_blocks_downstream": True,
    "skipped_component_blocks_downstream": True,
    "condition_skip_semantics_allowed": False,
    "partial_fanout_allowed": False,
    "same_run_nonce_required": True,
    "stale_output_allowed": False,
    "package_issuer_requires_draft_builder": True,
    "supervisor_requires_package_issuer": True,
}
IMPLEMENTATION_STATE = {
    "aggregate_target_present": False,
    "phase_targets_present": False,
    "draft_builder_binary_present": False,
    "draft_builder_unit_present": False,
    "package_issuer_unit_present": False,
    "services_installed": False,
    "services_enabled": False,
    "services_started": False,
}
OPERATOR_LOCAL_INPUTS = {
    "operation": "ABSENT",
    "strategy_request_id": "ABSENT",
    "durable_client_order_id": "ABSENT",
    "account_hmac": "ABSENT_NOT_STORED_IN_GIT",
    "operator_decision": "ABSENT",
    "run_nonce": "ABSENT",
    "signed_run_package": "ABSENT",
    "fresh_kill_switch": "ABSENT",
    "fresh_schedule": "ABSENT",
    "fresh_broker_truth": "ABSENT",
}
AUTHORIZATION = {
    "r2b": "NOT_ISSUED",
    "operator_arm_issued": False,
    "activation_authority_present": False,
    "target_start_allowed": False,
}
CLOSED_SURFACES = {
    "finam_credentials_accessed": False,
    "auth_service_called": False,
    "broker_account_get_sent": False,
    "order_post_sent": False,
    "order_delete_sent": False,
    "dispatch_attempt_recorded": False,
    "transport_entered": False,
    "redis_live_consumer": False,
    "broker_dispatch": False,
    "runtime_live": False,
    "strategy_live": False,
    "real_orders": False,
}
EVIDENCE_KEYS = {
    "schema_version", "stage", "status", "accepted_predecessor", "corrected_r0",
    "finding_count", "read_contract", "package_formation", "acceptance_rows",
    "targeted_negative_mutations", "production_rust_or_cargo_changed",
    "activation_target_implemented", "authorization_status", "finam_credentials_accessed",
    "auth_service_called", "broker_account_get_sent", "order_post_sent",
    "order_delete_sent", "dispatch_attempt_recorded", "transport_entered",
    "redis_live_consumer", "broker_dispatch", "runtime_live", "strategy_live",
    "real_orders", "result",
}
EVIDENCE_READ_KEYS = {
    "document_count", "snapshot_sha256", "fresh_refresh_evidence_sha256",
    "all_http_200", "all_match_embedded_snapshot", "activation_refresh_required",
    "activation_max_age_seconds", "credentials_used", "authservice_called",
    "broker_get_sent", "result",
}
EVIDENCE_FORMATION_KEYS = {
    "model", "service_invocations", "phase_count", "required_receipts",
    "same_run_nonce_required", "stale_receipts_allowed", "signing_key_available_to_builder",
    "draft_builder_implemented", "exact_fixed_input_count", "exact_receipt_source_count",
    "exact_signer_identity_frozen", "exact_phase_transaction_frozen", "result",
}


def require(value: bool, message: str) -> None:
    if not value:
        raise RuntimeError(message)


def require_exact_keys(document: object, expected: set[str], message: str) -> None:
    require(isinstance(document, dict) and set(document) == expected, message)


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def check(root: Path) -> None:
    read_refresh.verify_evidence(root)
    authority = json.loads((root / AUTHORITY.relative_to(ROOT)).read_text(encoding="utf-8"))
    evidence = json.loads((root / EVIDENCE.relative_to(ROOT)).read_text(encoding="utf-8"))
    matrix_bytes = (root / MATRIX.relative_to(ROOT)).read_bytes()
    rows = list(csv.DictReader(matrix_bytes.decode("utf-8").splitlines()))
    design = " ".join((root / DESIGN.relative_to(ROOT)).read_text(encoding="utf-8").split())

    require_exact_keys(authority, AUTHORITY_KEYS, "authority exact schema drift")
    require(authority["schema_version"] == 1, "authority schema-version drift")
    require(authority["stage"] == "Stage 8B-P R2B Issuance Package", "authority stage drift")
    require(authority["revision"] == "R0-R1A1", "revision drift")
    require(authority["status"] == "STRICT_SCHEMA_FREEZE_CANDIDATE_NOT_ISSUED", "status opened")
    require(authority["accepted_predecessor"] == {
        "stage": "Stage 8B-P R2B R4-R2A Acceptance Closure",
        "source_ref": "f24f1044ac0b29c2f588853b817e519cfe8d3d8b",
        "verdict": "ACCEPTED",
    }, "accepted predecessor drift")
    require(authority["corrects_r0"] == CORRECTS_R0, "R0 correction object drift")

    contract = authority["read_contract"]
    require(contract == READ_CONTRACT, "exact read-contract authority drift")

    formation = authority["package_formation"]
    require_exact_keys(formation, PACKAGE_FORMATION_KEYS, "package formation exact schema drift")
    require(formation["selected_model"] == "SEPARATE_DRAFT_BUILDER_THEN_SIGNER", "draft model drift")
    builder = formation["builder"]
    require_exact_keys(builder, BUILDER_KEYS, "builder exact schema drift")
    require(builder["executable"] == "stage8b-r2b-run-package-draft-builder", "builder executable drift")
    require(builder["service"] == "moex-stage8b-r2b-run-package-draft-builder.service", "builder service drift")
    require(builder["implemented_by_r0_r1"] is False, "builder prematurely implemented")
    require(builder["uid"] == builder["gid"] == 0, "builder identity drift")
    require(builder["signing_key_access"] is False and builder["credential_path_access"] is False, "builder secret access opened")
    require(builder["network_allowed"] is False, "builder network opened")
    require(builder["fixed_inputs"] == FIXED_INPUTS, "builder exact fixed-input inventory drift")
    require(builder["receipt_sources"] == RECEIPT_SOURCES, "builder exact receipt inventory drift")
    require(builder["required_receipt_count"] == 11, "receipt count drift")
    require(builder["required_same_run_nonce"] is True, "mixed nonce allowed")
    require(builder["stale_receipts_allowed"] is False, "stale receipts allowed")
    require(builder["controlled_fixture_producer_allowed"] is False, "controlled producer allowed")
    require(builder["schema"] == "R2a5RunPackage", "draft schema drift")
    require(builder["signature_ed25519_hex"] == "EMPTY", "draft unexpectedly signed")
    require(builder["validity_seconds"] == 30, "draft validity drift")
    output = builder["output"]
    require_exact_keys(output, BUILDER_OUTPUT_KEYS, "builder output exact schema drift")
    require(output["path"] == "/var/lib/moex-trading/stage8b/r2a5/r2b-run-package.unsigned.json", "unsigned path drift")
    require(output["owner_uid"] == output["owner_gid"] == 0 and output["mode"] == "0600", "unsigned custody drift")
    require(all(output[key] is True for key in ("atomic_publish", "nofollow_regular_single_link", "file_fsync", "directory_fsync")), "unsigned durability drift")
    require(output["existing_output_reuse_allowed"] is False, "stale unsigned output allowed")
    signer = formation["signer"]
    require(signer == SIGNER, "exact package signer authority drift")
    require(signer["fixed_input"] == output["path"], "signer input drift")

    activation = authority["future_activation_target"]
    require(activation == FUTURE_ACTIVATION, "future activation exact schema/value drift")
    require(all(not (root / relative).exists() for relative in ABSENT_IMPLEMENTATION), "implementation artifact present in design closure")

    transaction = authority["transaction"]
    require_exact_keys(transaction, TRANSACTION_KEYS, "transaction exact schema drift")
    phases = transaction["phases"]
    require(phases == PHASES, "exact phase transaction drift")
    require(transaction["phase_count"] == len(phases) == 6, "phase count drift")
    services = [service for phase in phases for service in phase["services"]]
    require(transaction["service_invocation_count"] == len(services) == 31, "service cardinality drift")
    require(len(set(services)) == 31, "duplicate service invocation")
    require(len(phases[2]["services"]) == len(phases[3]["services"]) == 11, "authority fanout drift")
    require(phases[4]["services"] == [builder["service"], signer["service"]], "phase 5 order drift")
    require(phases[5]["services"] == ["moex-stage8b-r2b-readonly-supervisor.service"], "terminal supervisor drift")
    barriers = transaction["barrier_contract"]
    require(barriers == BARRIER_CONTRACT, "barrier contract exact schema/value drift")

    require(authority["implementation_state"] == IMPLEMENTATION_STATE, "implementation state exact drift")
    require(authority["operator_local_inputs"] == OPERATOR_LOCAL_INPUTS, "operator input exact drift")
    require(authority["authorization"] == AUTHORIZATION, "authorization exact drift")
    require(authority["closed_surfaces"] == CLOSED_SURFACES, "closed surface exact drift")
    require(sha256(matrix_bytes) == MATRIX_SHA256, "acceptance matrix content drift")
    expected_ids = (
        [f"R2BI-R1-{index:03d}" for index in range(1, 41)]
        + [f"R2BI-R1A-{index:03d}" for index in range(41, 55)]
        + [f"R2BI-R1A1-{index:03d}" for index in range(55, 67)]
    )
    row_ids = [row["id"] for row in rows]
    require(row_ids == expected_ids and len(set(row_ids)) == len(row_ids), "acceptance row identity/order drift")
    require(len(rows) == authority["acceptance_rows"] == 66, "acceptance row count drift")
    require(all(row["expected"] == "PASS" for row in rows), "acceptance expectation drift")
    require(authority["targeted_negative_mutations"] == 66, "negative count drift")
    require_exact_keys(evidence, EVIDENCE_KEYS, "issuance evidence exact schema drift")
    require(evidence["schema_version"] == 1, "evidence schema-version drift")
    require(evidence["stage"] == "Stage 8B-P R2B Issuance Package R0-R1A1", "evidence stage drift")
    require(evidence["status"] == "STRICT_SCHEMA_FREEZE_CANDIDATE_NOT_ISSUED", "evidence status drift")
    require(evidence["accepted_predecessor"] == "f24f1044ac0b29c2f588853b817e519cfe8d3d8b", "evidence predecessor drift")
    require(evidence["corrected_r0"] == CORRECTS_R0["source_ref"] and evidence["finding_count"] == 2, "evidence R0 lineage drift")
    require_exact_keys(evidence["read_contract"], EVIDENCE_READ_KEYS, "issuance read evidence exact schema drift")
    require_exact_keys(evidence["package_formation"], EVIDENCE_FORMATION_KEYS, "issuance formation evidence exact schema drift")
    require(evidence["read_contract"]["document_count"] == 6, "evidence contract count drift")
    require(evidence["read_contract"]["snapshot_sha256"] == SNAPSHOT_SHA, "evidence contract binding drift")
    refresh_path = root / read_refresh.EVIDENCE
    require(evidence["read_contract"]["fresh_refresh_evidence_sha256"] == sha256(refresh_path.read_bytes()), "refresh evidence digest drift")
    require(evidence["package_formation"]["model"] == "SEPARATE_DRAFT_BUILDER_THEN_SIGNER", "evidence builder model drift")
    require(evidence["package_formation"]["service_invocations"] == 31, "evidence service count drift")
    require(evidence["acceptance_rows"] == "66/66" and evidence["targeted_negative_mutations"] == "66/66", "evidence coverage drift")
    require(evidence["production_rust_or_cargo_changed"] is False, "evidence production drift")
    require(evidence["activation_target_implemented"] is False, "evidence target opened")
    require(evidence["authorization_status"] == "NOT_ISSUED", "evidence authorization opened")
    for key in (
        "finam_credentials_accessed", "auth_service_called", "broker_account_get_sent",
        "order_post_sent", "order_delete_sent", "dispatch_attempt_recorded",
        "transport_entered", "redis_live_consumer", "broker_dispatch", "runtime_live",
        "strategy_live", "real_orders",
    ):
        require(evidence[key] is False, f"evidence surface opened: {key}")
    for marker in (
        "31 service invocations", "separate future", "no signing key",
        "1,800 seconds", "failed, skipped or missing", "NOT_ISSUED",
    ):
        require(marker in design, f"design marker missing: {marker}")


def main() -> None:
    check(ROOT)
    print(
        "stage8b-p-r2b-issuance-r1-check: PASS revision=R0-R1A1 rows=66 "
        "read_documents=6 services=31 phases=6 negatives=66 strict_schema=true exact_freeze=true builder=SEPARATE "
        "target_implemented=false authorization=NOT_ISSUED finam=false broker_get=false"
    )


if __name__ == "__main__":
    try:
        main()
    except (OSError, RuntimeError, ValueError, KeyError, json.JSONDecodeError) as error:
        raise SystemExit(f"stage8b-p-r2b-issuance-r1-check: FAIL {error}") from error
