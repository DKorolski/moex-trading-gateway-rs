#!/usr/bin/env python3
"""Fail-closed checker for Controlled Installation Implementation R0 Preflight R1A."""

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
TRANSACTION = BASE / "stage8b-p-r2b-implementation-transaction-contract.json"
TRIGGER = Path("deploy/stage8b-r2b-proof/stage8b-r2b-controlled-proof-trigger.service")
MATRIX = BASE / "STAGE8B_P_R2B_CONTROLLED_INSTALLATION_IMPL_R0_PREFLIGHT_ACCEPTANCE_MATRIX_2026-08-30.csv"
DESIGN = BASE / "STAGE8B_P_R2B_CONTROLLED_INSTALLATION_IMPL_R0_PREFLIGHT_2026-08-30.md"
STATUS = Path("docs/current-status.md")
R2A3 = Path("tools/stage8b-readonly-preflight/src/r2a3.rs")
MATRIX_SHA256 = "8465082fb0f7d02b50d2ec1ad723a78333cabcc1bac0bb867c518c1bd7c10458"
TRIGGER_SHA256 = "c30da9c111a0e681de6cd4cc23bab3b1d58f5b15b86aff885d0838cf43c6cf0f"

INHERITED = {
    "docs/stage-8/stage8b-p-r2b-controlled-installation-r0-authority.json": "7610ad1f7aa43aa054cc2765a7b680043ef258cddc1383294799f693d7ddd229",
    "docs/stage-8/stage8b-p-r2b-preproduction-supersession.json": "40f962a60cc721512bd07134e641e2ce69bb37f27dbea975fc552640ea3bd7b5",
    "docs/stage-8/stage8b-p-r2b-implementation-transaction-contract.json": "3d45203facd2634767d3ad21877d4c16b1bb3f9c7a2856bcf02471e69ad72af9",
}
FINGERPRINTS = {
    "authorization_public_key_sha256": "9149e9620ec0ea7ad3dab389542acf308471aaa0282e4b9020f75de7c13781af",
    "trust_manifest_sha256": "8014eea21ebe0b619122e0c7a332b50d173ff31d1cb2ea91e2505551dd547ef8",
    "public_key_set_sha256": "2e609dcbb6b6e7eb12fabebe4eb5ce62712aea91c2971a4e247194484f23da24",
    "account_key_manifest_sha256": "e40ea1d12ef5ebe4faf8ebaf6897056b9ac45d5efd0bb4c68eb4ff85f8bc7cd7",
}
NETWORK_BOUNDARY_PROOF = {
    "root_admission_required": True,
    "helper_authority_validation_required": True,
    "child_terminal_protocol_valid_required": True,
    "root_terminal_evidence_durable_required": True,
    "failed_attempt_required": True,
    "expected_attempt_ordinal": 1,
    "expected_method": "POST",
    "expected_route_template": "/v1/sessions",
    "allowed_attempt_error_categories": ["NETWORK_CONNECT_FAILURE", "TIMEOUT"],
    "request_timeout_requires_failed_attempt": True,
    "http_status_must_be_absent": True,
    "response_body_must_be_absent": True,
    "root_lifecycle_timeout_allowed": False,
    "auth_session_failure_without_attempt_allowed": False,
    "client_construction_failure_allowed": False,
    "effect_flags_must_be_false": True,
    "broker_dispatch_must_be_false": True,
    "real_order_flags_must_be_false": True,
    "outer_runner_evidence_parser": "EXACT_TYPED_ROOT_TERMINAL_EVIDENCE",
    "string_category_only_match_allowed": False,
}
BINARY_DESTINATIONS = [
    "/opt/moex-trading/stage8b-r2b/bin/stage8b-r2a8-upstream-current-authority-publisher",
    "/opt/moex-trading/stage8b-r2b/bin/stage8b-r2a8-authoritative-intake-creator",
    "/opt/moex-trading/stage8b-r2b/bin/stage8b-r2a8-production-intake-stager",
    "/opt/moex-trading/stage8b-r2b/bin/stage8b-r2a8-production-current-source-writer",
    "/opt/moex-trading/stage8b-r2a8/bin/stage8b-r2a8-current-manifest-issuer",
    "/opt/moex-trading/stage8b-r2a7/bin/stage8b-r2a7-source-adapter",
    "/opt/moex-trading/stage8b-r2a5/bin/stage8b-r2a5-authority-producer",
    "/opt/moex-trading/stage8b-r2a5/bin/stage8b-r2a5-authority-issuer",
    "/opt/moex-trading/stage8b-r2b/bin/stage8b-r2b-run-package-draft-builder",
    "/opt/moex-trading/stage8b-r2a5/bin/stage8b-r2a5-package-issuer",
    "/opt/moex-trading/stage8b-r2b/bin/stage8b-r2b-launcher",
    "/opt/moex-trading/stage8b-r2b/bin/stage8b-readonly-preflight",
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


def expected_unit_destinations(transaction: dict[str, Any]) -> list[str]:
    production = [f"/etc/systemd/system/{Path(path).name}" for path in transaction["unit_file_sha256"]]
    return production + ["/etc/systemd/system/stage8b-r2b-controlled-proof-trigger.service"]


def check(root: Path) -> None:
    authority = load(root, AUTHORITY)
    inventory = load(root, INVENTORY)
    ceremony = load(root, CEREMONY)
    reset = load(root, RESET)
    transaction = load(root, TRANSACTION)

    exact_keys(authority, {"schema_version", "stage", "status", "rejected_predecessor", "rejected_preflight_r1", "accepted_design", "accepted_implementation", "inherited_contract_sha256", "proof_lanes", "trigger", "artifact_root", "execution_state", "authorization", "closed_surfaces"}, "authority")
    require(authority["schema_version"] == 2, "authority schema drift")
    require(authority["stage"].endswith("Implementation R0 Preflight R1A"), "stage drift")
    require(authority["status"] == "PRODUCTION_REQUEST_BOUNDARY_ORACLE_CLOSURE_REVIEW_REQUIRED_NOT_EXECUTED", "status drift")
    require(authority["rejected_predecessor"] == {"source_ref": "b9f0c43f4865ee001b72abaf72c0a6a4dd77a32a", "archive_sha256": "b71ec0160a2968db8f6e8c4107597b67be0d9a2be31d216b51321f457797b98c", "verdict": "NOT_ACCEPTED"}, "rejected predecessor drift")
    require(authority["rejected_preflight_r1"] == {"source_ref": "9fd9fa9e7eea38371bb412a713f0419697671f7c", "archive_sha256": "297897488d2bc53a8b690673a4ec1c177fffbe4f9028128f99d66bf325e02120", "verdict": "NOT_ACCEPTED"}, "rejected R1 drift")
    require(authority["accepted_design"] == {"source_ref": "1e4db79288b0809fd5975edfdd0fc14740bcc8c6", "archive_sha256": "5d55ccd8a585d6da780531aa237c9fba215328bce502b1099a8dc5aa3c22faea", "verdict": "ACCEPTED"}, "accepted design drift")
    require(authority["accepted_implementation"] == {"source_ref": "6672819e357a3c2a2c1e73e5408c393da01913a1", "archive_sha256": "2bfb9653b71d942cdda46f7da6bc53f4f59b01e117e5475ef936f36c66c23d77", "verdict": "ACCEPTED"}, "accepted implementation drift")
    require(authority["inherited_contract_sha256"] == INHERITED, "inherited contract inventory drift")
    for relative, digest in INHERITED.items():
        require(sha256(root / relative) == digest, f"inherited contract changed: {relative}")

    exact_keys(authority["proof_lanes"], {"lane_a", "lane_b"}, "proof lanes")
    lane_a = authority["proof_lanes"]["lane_a"]
    exact_keys(lane_a, {"identity", "production_elf_count", "production_unit_count", "network_mode", "hardcoded_base_url", "aggregate_target_expected_success", "production_network_boundary_proof", "outer_runner_expected_success", "controlled_binary_substitution_allowed", "accepted_public_authority_required", "matching_offline_private_ceremony_required", "execution_without_matching_ceremony_allowed"}, "lane A")
    require(lane_a == {
        "identity": "EXACT_PRODUCTION_EXPECTED_FAIL_CLOSED", "production_elf_count": 12,
        "production_unit_count": 18, "network_mode": "none", "hardcoded_base_url": "https://api.finam.ru",
        "aggregate_target_expected_success": False, "production_network_boundary_proof": NETWORK_BOUNDARY_PROOF,
        "outer_runner_expected_success": True, "controlled_binary_substitution_allowed": False,
        "accepted_public_authority_required": True, "matching_offline_private_ceremony_required": True,
        "execution_without_matching_ceremony_allowed": False,
    }, "Lane A semantic drift")
    lane_b = authority["proof_lanes"]["lane_b"]
    require(lane_b == {"identity": "CONTROLLED_TLS_READ_PIPELINE_SUCCESS", "accepted_tls_ref": "6cb179509fad97e8be56e31bb930b2a86caefc6a", "loopback_only": True, "fresh_canary_domain_allowed": True, "counted_as_production_binary_proof": False, "new_execution_in_this_package": False}, "Lane B semantic drift")
    source = (root / R2A3).read_text(encoding="utf-8")
    require('pub const PRODUCTION_BASE_URL: &str = "https://api.finam.ru";' in source, "production FINAM base URL drift")

    trigger = authority["trigger"]
    require(trigger == {"source": TRIGGER.as_posix(), "destination": "/etc/systemd/system/stage8b-r2b-controlled-proof-trigger.service", "sha256": TRIGGER_SHA256, "production_service_arithmetic_member": False, "enabled": False, "direct_aggregate_manual_start_allowed": False, "production_dropins_allowed": False}, "trigger authority drift")
    require(sha256(root / TRIGGER) == TRIGGER_SHA256, "trigger bytes drift")
    trigger_text = (root / TRIGGER).read_text(encoding="utf-8")
    for line in ("Requires=moex-stage8b-r2b-issuance.target", "After=moex-stage8b-r2b-issuance.target", "Type=oneshot", "ExecStart=/bin/true"):
        require(trigger_text.count(line) == 1, f"trigger contract drift: {line}")
    require("[Install]" not in trigger_text and "RefuseManualStart" not in trigger_text, "trigger activation drift")
    require(authority["artifact_root"] == {"argument": "--artifact-root", "required": True, "binary_count": 12, "hash_mismatch_action": "ABORT_BEFORE_CONTAINER_CREATE"}, "artifact root drift")
    require(all(value is False for value in authority["execution_state"].values()), "R1A claims execution")
    require(authority["authorization"] == "NOT_ISSUED", "R2B issued")
    require(set(authority["closed_surfaces"]) == {"production_account_host", "real_operator", "real_credentials", "finam_network", "finam_auth_service", "finam_broker_get", "http_post_delete", "broker_dispatch", "redis_live", "runtime_live", "real_orders"}, "closed surface inventory drift")
    require(all(value is False for value in authority["closed_surfaces"].values()), "closed surface opened")

    exact_keys(inventory, {"schema_version", "inventory_id", "status", "host", "image", "docker_run", "source_mount", "installation", "production_linux_amd64_sha256"}, "inventory")
    require(inventory["schema_version"] == 2 and inventory["status"] == "PLANNED_NOT_CREATED", "inventory state drift")
    require(inventory["host"] == {"classification": "DEVELOPER_LOCAL_WORKSTATION", "production_account_host": False, "os": "darwin", "architecture": "arm64", "virtualization_boundary": "DOCKER_DESKTOP_LINUX_VM", "sensitive_workloads_in_vm_allowed": False}, "host boundary drift")
    image = inventory["image"]
    exact_keys(image, {"tag", "image_id", "platform", "base_image", "systemd_version", "package_inventory", "build_recipe", "rebuild_under_same_tag_allowed"}, "image")
    require(image["image_id"] == "sha256:3cc66c640df0444530a626d2acbcfeda9742039b917a747fd023b315ef2c1526" and image["systemd_version"] == "255.4-1ubuntu8.17" and image["rebuild_under_same_tag_allowed"] is False, "final image pin drift")
    require(image["package_inventory"] == {"findutils": "4.9.0-5build1", "iproute2": "6.1.0-1ubuntu6.4", "libc6:amd64": "2.39-0ubuntu8.8", "libsystemd0:amd64": "255.4-1ubuntu8.17", "python3": "3.12.3-0ubuntu2.1", "systemd": "255.4-1ubuntu8.17", "util-linux": "2.39.3-9ubuntu6.5"}, "package inventory drift")
    docker = inventory["docker_run"]
    exact_keys(docker, {"exact_flags", "privileged_inside_disposable_docker_desktop_vm", "explicit_cap_add_flags", "explicit_device_flags", "host_root_mount_allowed", "docker_socket_mount_allowed", "default_route_allowed", "dns_allowed", "finam_route_allowed"}, "docker boundary")
    require(docker["exact_flags"] == ["--privileged", "--platform=linux/amd64", "--cgroupns=host", "--network=none", "--tmpfs=/run/stage8b-r2b-canary:rw,nosuid,nodev,noexec,mode=0700", "--mount=type=bind,source=FRESH_REVIEW_EXTRACTION,destination=/work,readonly", "--mount=type=bind,source=ACCEPTED_ARTIFACT_ROOT,destination=/accepted-artifacts,readonly", "--mount=type=bind,source=REDACTED_EVIDENCE_DIR,destination=/evidence", "--mount=type=bind,source=/sys/fs/cgroup,destination=/sys/fs/cgroup"], "docker flags drift")
    require(docker["privileged_inside_disposable_docker_desktop_vm"] is True, "privileged VM boundary hidden")
    require(docker["explicit_cap_add_flags"] == [] and docker["explicit_device_flags"] == [], "explicit capability/device flags opened")
    require(all(docker[key] is False for key in ("host_root_mount_allowed", "docker_socket_mount_allowed", "default_route_allowed", "dns_allowed", "finam_route_allowed")), "Docker boundary opened")
    require(inventory["source_mount"] == {"kind": "FRESH_EXTRACTION_OF_REVIEWED_HANDOFF", "developer_working_tree_allowed": False, "handoff_commit_match_required": True, "source_manifest_complete_required": True, "archive_sha256_match_required": True, "untracked_files_allowed": False, "env_files_allowed": False}, "source mount drift")
    require(inventory["installation"] == {"production_unit_target_file_count": 18, "proof_trigger_file_count": 1, "total_unit_file_count": 19, "service_invocation_count": 31, "phase_count": 6, "binary_count": 12, "enablement_allowed": False, "production_dropins_allowed": False, "persistent_host_install_allowed": False}, "installation inventory drift")
    require(inventory["production_linux_amd64_sha256"] == transaction["production_linux_amd64_sha256"], "production ELF inventory drift")
    require(len(transaction["unit_file_sha256"]) == 18, "production unit inventory drift")

    exact_keys(ceremony, {"schema_version", "contract_id", "status", "lane_a_exact_production", "lane_b_controlled_tls", "materialization", "evidence_policy", "current_state"}, "ceremony")
    require(ceremony["schema_version"] == 2 and ceremony["status"] == "PLANNED_NOT_MATERIALIZED", "ceremony state drift")
    lane_a_ceremony = ceremony["lane_a_exact_production"]
    require(lane_a_ceremony == {"ceremony_class": "EPHEMERAL_MATERIALIZATION_OF_ACCEPTED_PREPRODUCTION_TRUST_SET", "new_random_key_generation_allowed": False, "accepted_fingerprints": FINGERPRINTS, "matching_private_material_source": "SEPARATELY_REVIEWED_OFFLINE_CEREMONY", "matching_private_material_in_repository": False, "matching_private_material_in_handoff": False, "execution_precondition": "ALL_ACCEPTED_PUBLIC_FINGERPRINTS_MATCH_BEFORE_CONTAINER_CREATE", "missing_or_mismatched_action": "ABORT_BEFORE_CONTAINER_CREATE"}, "Lane A ceremony drift")
    require(ceremony["lane_b_controlled_tls"] == {"ceremony_id": "stage8b-r2b-controlled-tls-canary-20260830-r1", "trust_domain": "CONTROLLED_TLS_CANARY_NO_PRODUCTION_CONTINUITY", "fresh_random_key_generation_allowed": True, "accepted_tls_ref": "6cb179509fad97e8be56e31bb930b2a86caefc6a", "loopback_only": True, "production_binary_proof": False, "execution_in_this_package": False}, "Lane B ceremony drift")
    require(ceremony["materialization"] == {"allowed_only_after_network_isolation_verified": True, "location": "/run/stage8b-r2b-canary", "storage": "tmpfs", "private_material_export_allowed": False, "host_persistence_allowed": False, "source_or_handoff_persistence_allowed": False, "shell_argument_exposure_allowed": False}, "materialization opened")
    require(ceremony["evidence_policy"] == {"public_key_fingerprints_allowed": True, "private_key_bytes_allowed": False, "secret_values_allowed": False, "redacted_lifecycle_events_required": True, "destruction_receipt_required": True}, "evidence policy opened")
    require(all(value is False for value in ceremony["current_state"].values()), "ceremony claims execution")

    exact_keys(reset, {"schema_version", "plan_id", "status", "reset_before_second_run", "unit_destinations", "binary_destinations", "state_and_credential_roots", "post_proof_uninstall", "postconditions", "current_state"}, "reset")
    require(reset["schema_version"] == 2 and reset["status"] == "PLANNED_NOT_EXECUTED", "reset state drift")
    require(reset["unit_destinations"] == expected_unit_destinations(transaction), "unit removal inventory drift")
    require(len(reset["unit_destinations"]) == 19 and len(set(reset["unit_destinations"])) == 19, "unit removal uniqueness drift")
    require(reset["binary_destinations"] == BINARY_DESTINATIONS, "binary removal inventory drift")
    require(len(set(reset["binary_destinations"])) == 12, "binary removal uniqueness drift")
    for marker in ("/stage8b-r2a7/", "/stage8b-r2a8/"):
        require(any(marker in path for path in reset["binary_destinations"]), f"binary removal missing: {marker}")
    repeat = reset["reset_before_second_run"]
    require(repeat["reuse_first_run_materialization"] is False, "run-one materialization reuse opened")
    require(repeat["same_accepted_key_identities_required"] is True, "accepted key identity continuity removed")
    require(repeat["fresh_tmpfs_projection_required"] is True, "fresh run-two projection removed")
    require(repeat["run1_projection_destroyed_before_run2"] is True, "run-one projection destruction removed")
    require(reset["reset_before_second_run"]["second_run_expected_result"] == "SAME_EXPECTED_FAIL_CLOSED_LANE_A_OUTER_PASS", "second-run semantics drift")
    require(all(value is True for key, value in reset["post_proof_uninstall"].items() if key != "wildcard_is_cleanup_authority"), "uninstall requirement missing")
    require(reset["post_proof_uninstall"]["wildcard_is_cleanup_authority"] is False, "wildcard cleanup authority restored")
    require(reset["postconditions"] == {"loaded_matching_units": 0, "installed_unit_files": 0, "installed_binaries": 0, "transaction_state_files": 0, "private_material_files": 0, "public_projection_files": 0, "finam_requests": 0, "authorization": "NOT_ISSUED"}, "postconditions drift")
    require(all(value is False for value in reset["current_state"].values()), "reset claims execution")

    require(sha256(root / MATRIX) == MATRIX_SHA256, "acceptance matrix content drift")
    with (root / MATRIX).open(newline="", encoding="utf-8") as handle:
        rows = list(csv.DictReader(handle))
    require([row["id"] for row in rows] == [f"CIPFR1A-{index:03d}" for index in range(1, 51)], "matrix identity drift")
    require(all(row["status"] == "PASS" and all(row.values()) for row in rows), "matrix incomplete")
    design = (root / DESIGN).read_text(encoding="utf-8")
    for marker in ("POST /v1/sessions attempt #1", "EXACT_TYPED_ROOT_TERMINAL_EVIDENCE", "request-level `TIMEOUT`", "aggregate target                   EXPECTED FAILED", "controlled binaries and results may not be counted", "fresh extraction of the reviewed handoff", "19 unit destinations", "12 binary destinations"):
        require(marker in design, f"design marker missing: {marker}")
    status = (root / STATUS).read_text(encoding="utf-8")
    for marker in ("Implementation R0 Preflight R1A", "POST /v1/sessions", "NOT_ISSUED", "FINAM network"):
        require(marker in status, f"current status missing: {marker}")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=ROOT)
    args = parser.parse_args()
    try:
        check(args.root.resolve())
    except (KeyError, OSError, RuntimeError, ValueError, json.JSONDecodeError) as error:
        raise SystemExit(f"stage8b-p-r2b-controlled-installation-impl-r0-preflight-r1a-check: FAIL {error}") from error
    print("stage8b-p-r2b-controlled-installation-impl-r0-preflight-r1a-check: PASS request=POST:/v1/sessions:1 outcomes=NETWORK_CONNECT_FAILURE|TIMEOUT aggregate=expected-failed outer=pass units=19 binaries=12 execution=false authorization=NOT_ISSUED")


if __name__ == "__main__":
    main()
