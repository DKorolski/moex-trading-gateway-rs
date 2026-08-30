#!/usr/bin/env python3
"""Fail-closed checker for the controlled-installation Implementation R0 preflight."""

from __future__ import annotations

import argparse
import csv
import hashlib
import json
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
BASE = Path("docs/stage-8")
AUTHORITY = BASE / "stage8b-p-r2b-controlled-installation-impl-r0-preflight-authority.json"
INVENTORY = BASE / "stage8b-p-r2b-controlled-installation-impl-r0-staging-inventory.json"
CEREMONY = BASE / "stage8b-p-r2b-controlled-installation-impl-r0-canary-ceremony.json"
RESET = BASE / "stage8b-p-r2b-controlled-installation-impl-r0-reset-uninstall.json"
MATRIX = BASE / "STAGE8B_P_R2B_CONTROLLED_INSTALLATION_IMPL_R0_PREFLIGHT_ACCEPTANCE_MATRIX_2026-08-30.csv"
DESIGN_DOC = BASE / "STAGE8B_P_R2B_CONTROLLED_INSTALLATION_IMPL_R0_PREFLIGHT_2026-08-30.md"
STATUS = Path("docs/current-status.md")
TRANSACTION = BASE / "stage8b-p-r2b-implementation-transaction-contract.json"

ACCEPTED_DESIGN = "1e4db79288b0809fd5975edfdd0fc14740bcc8c6"
ACCEPTED_DESIGN_ARCHIVE_SHA256 = "5d55ccd8a585d6da780531aa237c9fba215328bce502b1099a8dc5aa3c22faea"
ACCEPTED_IMPLEMENTATION = "6672819e357a3c2a2c1e73e5408c393da01913a1"
ACCEPTED_IMPLEMENTATION_ARCHIVE_SHA256 = "2bfb9653b71d942cdda46f7da6bc53f4f59b01e117e5475ef936f36c66c23d77"
MATRIX_SHA256 = "12dfac10bff90f033f216dc90088f401dd26d897a03aad9dec161d3663cd9f63"

INHERITED = {
    "docs/stage-8/stage8b-p-r2b-controlled-installation-r0-authority.json": "7610ad1f7aa43aa054cc2765a7b680043ef258cddc1383294799f693d7ddd229",
    "docs/stage-8/stage8b-p-r2b-preproduction-supersession.json": "40f962a60cc721512bd07134e641e2ce69bb37f27dbea975fc552640ea3bd7b5",
    "docs/stage-8/stage8b-p-r2b-implementation-transaction-contract.json": "3d45203facd2634767d3ad21877d4c16b1bb3f9c7a2856bcf02471e69ad72af9",
}

SOURCES = [
    "trusted_clock", "stage7b_current_recovery_seal", "stage6_exact_dispatch_ready_command",
    "stage8a_root_config_policy_control", "composite_readiness", "kill_switch_run_allowed",
    "single_finam_ownership", "schedule", "instrument_specification",
    "ambiguity_orphan_unresolved_lifecycle", "durable_micro_budget",
]


def require(value: bool, message: str) -> None:
    if not value:
        raise RuntimeError(message)


def exact_keys(value: dict[str, Any], keys: set[str], label: str) -> None:
    require(set(value) == keys, f"{label} keyset drift: {sorted(set(value) ^ keys)}")


def load(root: Path, path: Path) -> dict[str, Any]:
    value = json.loads((root / path).read_text(encoding="utf-8"))
    require(isinstance(value, dict), f"{path} is not an object")
    return value


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def check(root: Path) -> None:
    authority = load(root, AUTHORITY)
    inventory = load(root, INVENTORY)
    ceremony = load(root, CEREMONY)
    reset = load(root, RESET)
    transaction = load(root, TRANSACTION)

    exact_keys(authority, {
        "schema_version", "stage", "status", "accepted_design", "accepted_implementation",
        "inherited_contract_sha256", "new_contracts", "artifact_root", "execution_state",
        "authorization", "closed_surfaces",
    }, "authority")
    require(authority["schema_version"] == 1, "authority schema drift")
    require(authority["stage"] == "Stage 8B-P R2B Controlled Installation / Full Transaction Proof — Implementation R0 Preflight", "stage drift")
    require(authority["status"] == "PREFLIGHT_REVIEW_REQUIRED_NOT_EXECUTED_NOT_ISSUED", "status opened")
    for label, value, source_ref, archive_name, archive_sha in (
        ("design", authority["accepted_design"], ACCEPTED_DESIGN, "moex-trading-project-1e4db79.zip", ACCEPTED_DESIGN_ARCHIVE_SHA256),
        ("implementation", authority["accepted_implementation"], ACCEPTED_IMPLEMENTATION, "moex-trading-project-6672819.zip", ACCEPTED_IMPLEMENTATION_ARCHIVE_SHA256),
    ):
        exact_keys(value, {"source_ref", "archive_name", "archive_sha256", "verdict"}, f"accepted {label}")
        require(value == {"source_ref": source_ref, "archive_name": archive_name, "archive_sha256": archive_sha, "verdict": "ACCEPTED"}, f"accepted {label} drift")
    require(authority["inherited_contract_sha256"] == INHERITED, "inherited contract inventory drift")
    for relative, digest in INHERITED.items():
        require(sha256(root / relative) == digest, f"inherited contract changed: {relative}")
    require(authority["new_contracts"] == {
        "staging_inventory": INVENTORY.as_posix(), "canary_ceremony": CEREMONY.as_posix(),
        "reset_uninstall": RESET.as_posix(),
    }, "new contract inventory drift")
    exact_keys(authority["artifact_root"], {"argument", "required", "accepted_predecessor_archive_embedded", "binary_count", "hash_mismatch_action"}, "artifact root")
    require(authority["artifact_root"] == {
        "argument": "--artifact-root", "required": True, "accepted_predecessor_archive_embedded": False,
        "binary_count": 12, "hash_mismatch_action": "ABORT_BEFORE_CONTAINER_CREATE",
    }, "artifact-root contract drift")
    exact_keys(authority["execution_state"], {
        "installation_authorized_by_this_package", "container_created", "units_installed", "units_enabled",
        "units_started", "canary_private_material_created", "proof_executed", "cleanup_executed",
    }, "execution state")
    require(all(value is False for value in authority["execution_state"].values()), "preflight claims execution")
    require(authority["authorization"] == "NOT_ISSUED", "R2B issued")
    require(set(authority["closed_surfaces"]) == {
        "production_account_host", "real_operator", "real_credentials", "finam_network",
        "finam_auth_service", "finam_broker_get", "http_post_delete", "broker_dispatch",
        "redis_live", "runtime_live", "real_orders",
    }, "closed surface keyset drift")
    require(all(value is False for value in authority["closed_surfaces"].values()), "closed surface opened")

    exact_keys(inventory, {"schema_version", "inventory_id", "status", "host", "contour", "mounts", "forbidden_mounts", "installation", "production_linux_amd64_sha256"}, "inventory")
    require(inventory["schema_version"] == 1 and inventory["status"] == "PLANNED_NOT_CREATED", "inventory state drift")
    exact_keys(inventory["host"], {"classification", "production_account_host", "os", "architecture", "production_runtime_mount_allowed"}, "host")
    require(inventory["host"] == {"classification": "DEVELOPER_LOCAL_WORKSTATION", "production_account_host": False, "os": "darwin", "architecture": "arm64", "production_runtime_mount_allowed": False}, "host classification drift")
    exact_keys(inventory["contour"], {"backend", "container_platform", "base_image", "systemd_pid1_required", "ephemeral", "privileged_only_for_systemd_cgroup", "network_mode", "default_route_allowed", "dns_allowed", "finam_route_allowed"}, "contour")
    require(inventory["contour"] == {
        "backend": "docker", "container_platform": "linux/amd64",
        "base_image": "ubuntu@sha256:33ceb71981b602c1a7443a53469e4dba065f7503eab3078a2d7a57a2ab987517",
        "systemd_pid1_required": True, "ephemeral": True, "privileged_only_for_systemd_cgroup": True,
        "network_mode": "none", "default_route_allowed": False, "dns_allowed": False, "finam_route_allowed": False,
    }, "contour drift")
    expected_mounts = [
        {"source_class": "REVIEWED_SOURCE_TREE", "destination": "/work", "mode": "ro"},
        {"source_class": "ACCEPTED_ELF_ARTIFACT_ROOT", "destination": "/accepted-artifacts", "mode": "ro"},
        {"source_class": "DEDICATED_REDACTED_EVIDENCE_OUTPUT", "destination": "/evidence", "mode": "rw"},
        {"source_class": "EPHEMERAL_CANARY_TMPFS", "destination": "/run/stage8b-r2b-canary", "mode": "tmpfs"},
    ]
    require(inventory["mounts"] == expected_mounts, "mount allowlist drift")
    require(inventory["forbidden_mounts"] == ["/", "/var/run/docker.sock", "/opt/trading-hybrid", "/opt/moex-trading", ".env", "BROKER_CREDENTIAL_DIRECTORY", "PRODUCTION_REDIS_DIRECTORY"], "forbidden mount drift")
    exact_keys(inventory["installation"], {"unit_target_file_count", "service_invocation_count", "phase_count", "binary_count", "enablement_allowed", "persistent_host_install_allowed"}, "installation")
    require(inventory["installation"] == {"unit_target_file_count": 18, "service_invocation_count": 31, "phase_count": 6, "binary_count": 12, "enablement_allowed": False, "persistent_host_install_allowed": False}, "installation inventory drift")
    require(inventory["production_linux_amd64_sha256"] == transaction["production_linux_amd64_sha256"], "ELF hash inventory drift")
    require(len(transaction["unit_file_sha256"]) == 18, "unit hash inventory incomplete")

    exact_keys(ceremony, {"schema_version", "ceremony_id", "status", "trust_domain", "generation_interpretation", "materialization", "identity", "key_inventory", "evidence_policy", "current_state"}, "ceremony")
    require(ceremony["schema_version"] == 1, "ceremony schema drift")
    require(ceremony["ceremony_id"] == "stage8b-r2b-canary-offline-20260830-r0", "ceremony identity drift")
    require(ceremony["status"] == "REVIEWED_ID_NOT_MATERIALIZED" and ceremony["trust_domain"] == "CANARY_EPHEMERAL_NO_PRODUCTION_CONTINUITY", "ceremony state drift")
    require(ceremony["generation_interpretation"] == "NEW_CANARY_DOMAIN_INITIAL_GENERATION", "ceremony generation drift")
    exact_keys(ceremony["materialization"], {"allowed_only_after_network_isolation_verified", "location", "storage", "private_material_export_allowed", "host_persistence_allowed", "source_or_handoff_persistence_allowed", "shell_argument_exposure_allowed"}, "materialization")
    require(ceremony["materialization"] == {"allowed_only_after_network_isolation_verified": True, "location": "/run/stage8b-r2b-canary", "storage": "tmpfs", "private_material_export_allowed": False, "host_persistence_allowed": False, "source_or_handoff_persistence_allowed": False, "shell_argument_exposure_allowed": False}, "materialization opened")
    exact_keys(ceremony["identity"], {"operator", "account", "finam_secret", "real_operator_selection_allowed", "real_account_allowed", "real_broker_token_allowed"}, "ceremony identity")
    require(ceremony["identity"] == {"operator": "CANARY_OPERATOR_NOT_A_REAL_PERSON", "account": "CANARY_ACCOUNT_NOT_A_BROKER_ACCOUNT", "finam_secret": "CANARY_NON_TOKEN", "real_operator_selection_allowed": False, "real_account_allowed": False, "real_broker_token_allowed": False}, "real identity opened")
    exact_keys(ceremony["key_inventory"], {"package_authorization_keypairs", "helper_acceptance_keypairs", "account_binding_keys", "source_issuer_keypairs", "source_names"}, "key inventory")
    require(ceremony["key_inventory"] == {"package_authorization_keypairs": 1, "helper_acceptance_keypairs": 1, "account_binding_keys": 1, "source_issuer_keypairs": 11, "source_names": SOURCES}, "source key inventory drift")
    exact_keys(ceremony["evidence_policy"], {"public_key_fingerprints_allowed", "private_key_bytes_allowed", "secret_values_allowed", "redacted_lifecycle_events_required", "destruction_receipt_required"}, "evidence policy")
    require(ceremony["evidence_policy"] == {"public_key_fingerprints_allowed": True, "private_key_bytes_allowed": False, "secret_values_allowed": False, "redacted_lifecycle_events_required": True, "destruction_receipt_required": True}, "evidence policy opened")
    exact_keys(ceremony["current_state"], {"private_material_present", "public_manifest_present", "ceremony_executed", "destruction_receipt_present"}, "ceremony current state")
    require(all(value is False for value in ceremony["current_state"].values()), "ceremony claims execution")

    exact_keys(reset, {"schema_version", "plan_id", "status", "reset_before_second_run", "post_proof_uninstall", "removal_roots", "postconditions", "current_state"}, "reset plan")
    require(reset["schema_version"] == 1 and reset["plan_id"] == "stage8b-r2b-controlled-proof-reset-uninstall-r0", "reset identity drift")
    require(reset["status"] == "PLANNED_NOT_EXECUTED", "reset already executed")
    exact_keys(reset["reset_before_second_run"], {"required", "stop_aggregate_and_phase_targets", "reset_failed_units", "remove_transaction_outputs", "remove_nonce_and_receipts", "remove_current_source_and_intake", "remove_canary_public_projections", "reuse_first_run_private_material", "empty_state_proof_required"}, "reset-before-second-run")
    for key, value in reset["reset_before_second_run"].items():
        if key != "reuse_first_run_private_material":
            require(value is True, f"reset requirement missing: {key}")
    require(reset["reset_before_second_run"]["reuse_first_run_private_material"] is False, "private material reuse opened")
    exact_keys(reset["post_proof_uninstall"], {"required_on_success", "required_on_failure", "stop_units", "disable_units", "remove_unit_and_target_files", "remove_installed_binaries", "remove_state_roots", "remove_runtime_roots", "remove_canary_credentials", "systemd_daemon_reload", "systemd_reset_failed", "destroy_container"}, "post-proof uninstall")
    require(all(value is True for value in reset["post_proof_uninstall"].values()), "uninstall requirement missing")
    require(reset["removal_roots"] == [
        "/etc/systemd/system/moex-stage8b-r2b-*", "/etc/systemd/system/stage8b-r2a5-*",
        "/opt/moex-trading/stage8b-r2b", "/opt/moex-trading/stage8b-r2a5",
        "/etc/moex-trading/stage8b/r2a5", "/run/moex-trading/stage8b",
        "/run/credentials/moex-trading/stage8b", "/run/stage8b-r2b-canary",
        "/var/lib/moex-trading/stage8b", "/var/lib/moex-trading/operational-authorities",
    ], "removal root drift")
    require(reset["postconditions"] == {"loaded_matching_units": 0, "installed_matching_unit_files": 0, "installed_matching_binaries": 0, "transaction_state_files": 0, "canary_private_files": 0, "canary_public_files": 0, "finam_requests": 0, "authorization": "NOT_ISSUED"}, "postcondition drift")
    require(reset["current_state"] == {"reset_executed": False, "uninstall_executed": False, "container_created": False}, "reset current state drift")

    with (root / MATRIX).open(newline="", encoding="utf-8") as handle:
        rows = list(csv.DictReader(handle))
    require(sha256(root / MATRIX) == MATRIX_SHA256, "acceptance matrix content drift")
    require(list(rows[0]) == ["id", "category", "requirement", "evidence", "status"], "matrix columns drift")
    require([row["id"] for row in rows] == [f"CIPF-{index:03d}" for index in range(1, 31)], "matrix identity drift")
    require(all(row["status"] == "PASS" and all(row.values()) for row in rows), "matrix incomplete")

    status = (root / STATUS).read_text(encoding="utf-8")
    for marker in (ACCEPTED_IMPLEMENTATION, ACCEPTED_DESIGN, "Implementation R0 preflight", "NOT_ISSUED", "FINAM network"):
        require(marker in status, f"current status missing marker: {marker}")
    design = (root / DESIGN_DOC).read_text(encoding="utf-8")
    for marker in ("--artifact-root", "12 Linux/amd64 executables", "Docker `--network none`", "cleanup"):
        require(marker in design, f"design doc missing marker: {marker}")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=ROOT)
    args = parser.parse_args()
    try:
        check(args.root.resolve())
    except (KeyError, OSError, RuntimeError, ValueError, json.JSONDecodeError) as error:
        raise SystemExit(f"stage8b-p-r2b-controlled-installation-impl-r0-preflight-check: FAIL {error}") from error
    print("stage8b-p-r2b-controlled-installation-impl-r0-preflight-check: PASS binaries=12 units=18 phases=6 services=31 execution=false authorization=NOT_ISSUED finam=false")


if __name__ == "__main__":
    main()
