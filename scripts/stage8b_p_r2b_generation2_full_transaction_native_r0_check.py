#!/usr/bin/env python3
"""Fail-closed checker for the Generation-2 full transaction rebind R0."""

from __future__ import annotations

import argparse
import csv
import hashlib
import json
from pathlib import Path
from typing import Any

import stage8b_p_r2b_controlled_installation_impl_r0_preflight_check as preflight
import stage8b_p_r2b_controlled_installation_r0_check as legacy
import stage8b_p_r2b_generation2_composition_r0_r1_check as composition


ROOT = Path(__file__).resolve().parents[1]
BASE = Path("docs/stage-8")
CONTRACT = BASE / "stage8b-p-r2b-generation2-full-transaction-contract.json"
DESIGN = BASE / "STAGE8B_P_R2B_GENERATION2_FULL_TRANSACTION_NATIVE_R0_2026-09-01.md"
MATRIX = BASE / "STAGE8B_P_R2B_GENERATION2_FULL_TRANSACTION_NATIVE_R0_ACCEPTANCE_MATRIX_2026-09-01.csv"
AUTHORITY = BASE / "stage8b-p-r2b-generation2-full-transaction-native-r0-authority.json"
HOST_ATTESTATION_EXAMPLE = BASE / "stage8b-p-r2b-generation2-native-host-attestation.example.json"
TERMINAL_ORACLE = Path("scripts/stage8b_p_r2b_generation2_full_transaction_native_r0_terminal_oracle.py")
HOST_PREFLIGHT = Path("scripts/stage8b_p_r2b_generation2_full_transaction_native_r0_host_preflight.py")
CEREMONY_PREFLIGHT = Path("scripts/stage8b_p_r2b_generation2_full_transaction_native_r0_ceremony_preflight.py")
NATIVE_RUNNER = Path("scripts/stage8b_p_r2b_generation2_full_transaction_native_r0_runner.sh")
CONTAINER_RUNNER = Path("scripts/stage8b_p_r2b_generation2_full_transaction_native_r0_container_run.sh")
MANIFEST_MATERIALIZER = Path("scripts/stage8b_p_r2b_generation2_full_transaction_native_r0_materialize_manifest.py")
HANDOFF_MAKER = Path("scripts/make_stage8b_p_r2b_generation2_full_transaction_native_r0_handoff.py")
HANDOFF_SAFETY = Path("scripts/stage8b_p_r2b_generation2_full_transaction_native_r0_handoff_safety_check.py")
VPS_STATIC_REHEARSAL = BASE / "stage8b-p-r2b-generation2-vps-native-static-rehearsal.json"
LEGACY_CONTRACT = legacy.TRANSACTION
PREFLIGHT_AUTHORITY = preflight.AUTHORITY
UPSTREAM_BUILD = BASE / "stage8b-p-r2b-r4-build-evidence.json"
GENERATION2_BUILD = composition.r0.BUILD
GENERATION2_AUTHORITY = composition.r0.PRODUCTION_AUTHORITY

ACCEPTED_COMPOSITION_REF = "c74382a7e3a63d3673dec220ff4e9caaba6b48ee"
ACCEPTED_COMPOSITION_ARCHIVE = "2185e1af518bbfadb7e9f426cacab00d444dcdd8ca37957c1e4f9d3901e09a62"
ACCEPTED_PREFLIGHT_REF = "a2586c428cd97349956efb12409ff37aea1fbe78"
LEGACY_CONTRACT_SHA256 = "3d45203facd2634767d3ad21877d4c16b1bb3f9c7a2856bcf02471e69ad72af9"
PREFLIGHT_SHA256 = "bafa59e0b76eb323b6f4be02f32200c48958854e63fde9ad9aaca9cb0b1f2db1"
UPSTREAM_BUILD_SHA256 = "2ede62b79dc4cdaca66ba21da133ccb6f427106ba3b441f3d33b99434f1967a8"
GENERATION2_BUILD_SHA256 = composition.BUILD_EVIDENCE_SHA256
GENERATION2_AUTHORITY_SHA256 = "d9b19167f545cd40253620dce185dbffe462dbc83f030cc5d5aa358b9424fb3e"

EXPECTED_BINARIES = {
    "stage8b-r2a8-upstream-current-authority-publisher": "3d697198d77ee8b697d7dca7f5fc14d58b58bb68ad53efaa683efefa87e745f4",
    "stage8b-r2a8-authoritative-intake-creator": "5f4071100a962028d147bcf53772b3619f8e5bb28a41323c7865abff66430c07",
    "stage8b-r2a8-production-intake-stager": "97929be6af7638952fc3cc9d95b32d68417c6d92e17945702545e01a6801abd5",
    "stage8b-r2a8-production-current-source-writer": "d0b86883a431f825c88220660f0d41e95448fd74cede575f0af8bb5b3d94caf9",
    "stage8b-r2a8-current-manifest-issuer": "b7246ca2754725e8519e517d25021492bb99bd3a22bd3ef31ab2a9f226d57330",
    "stage8b-r2a7-source-adapter": "f51d09315d8b02244d7bc218447b588ba3f20f1d7fc2573408ccfb61dbbe1541",
    "stage8b-r2a5-authority-producer": "fa494d0150cb3ed0f5f05378a8e1636f3160499f9f5cc881cbbed862c96229fc",
    "stage8b-r2a5-authority-issuer": "6dc5be078029a833b2e465525498c76e8d5966fa2c8d4733cfa3dce6b5af74e0",
    "stage8b-r2b-run-package-draft-builder": "f171fc282e56d509e30bb92ea40340e559b19dc12ac63f9513bed9a926b72207",
    "stage8b-r2a5-package-issuer": "5aff3f7d4747113546272cb40fc444b5bfa0013116b49d20669e8e757091625c",
    "stage8b-r2b-launcher": "52dfbd0e6bb0d07a92a3104be50c33a60af08905b6cd075aa4bd4a4c373da17e",
    "accepted-stage8b-readonly-preflight": composition.HELPER_SHA256,
}
EXPECTED_PROOF_TOOLS = {
    "stage8b-r2a5-controlled-layout": "38c179dbb6ac227d1cd430e3ec35d7e3f797f6504c8f4565c6dbb5ef869cb098",
    "stage8b-r2b-creator-chain-seeder": "e910ded838b634be1b957e80d367187befb5c7563ce14b99f7d1a60a8fc4e45a",
}
UPSTREAM_NAMES = tuple(list(EXPECTED_BINARIES)[:6])
GENERATION2_BUILD_NAMES = {
    "stage8b-r2a5-authority-producer": "stage8b-r2a5-authority-producer",
    "stage8b-r2a5-authority-issuer": "stage8b-r2a5-authority-issuer",
    "stage8b-r2b-run-package-draft-builder": "stage8b-r2b-run-package-draft-builder",
    "stage8b-r2a5-package-issuer": "stage8b-r2a5-package-issuer",
    "stage8b-r2b-launcher": "stage8b-r2b-launcher",
    "accepted-stage8b-readonly-preflight": "stage8b-readonly-preflight",
}


def require(value: bool, message: str) -> None:
    if not value:
        raise RuntimeError(message)


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def load(root: Path, relative: Path) -> dict[str, Any]:
    value = json.loads((root / relative).read_text(encoding="utf-8"))
    require(isinstance(value, dict), f"JSON object required: {relative}")
    return value


def exact_keys(value: dict[str, Any], expected: set[str], label: str) -> None:
    require(set(value) == expected, f"{label} schema drift")


def contract_required_paths(root: Path) -> set[Path]:
    legacy_contract = load(root, LEGACY_CONTRACT)
    return {
        CONTRACT,
        DESIGN,
        MATRIX,
        AUTHORITY,
        HOST_ATTESTATION_EXAMPLE,
        TERMINAL_ORACLE,
        HOST_PREFLIGHT,
        CEREMONY_PREFLIGHT,
        NATIVE_RUNNER,
        CONTAINER_RUNNER,
        MANIFEST_MATERIALIZER,
        HANDOFF_MAKER,
        HANDOFF_SAFETY,
        VPS_STATIC_REHEARSAL,
        LEGACY_CONTRACT,
        PREFLIGHT_AUTHORITY,
        UPSTREAM_BUILD,
        GENERATION2_BUILD,
        GENERATION2_AUTHORITY,
        *(Path(path) for path in legacy_contract["unit_file_sha256"]),
    }


def check_contract(root: Path) -> None:
    for relative in contract_required_paths(root):
        require((root / relative).is_file(), f"missing contract artifact: {relative}")
    contract = load(root, CONTRACT)
    inherited = load(root, LEGACY_CONTRACT)
    preflight_authority = load(root, PREFLIGHT_AUTHORITY)
    upstream_build = load(root, UPSTREAM_BUILD)
    generation2_build = load(root, GENERATION2_BUILD)
    generation2_authority = load(root, GENERATION2_AUTHORITY)
    authority = load(root, AUTHORITY)
    vps_rehearsal = load(root, VPS_STATIC_REHEARSAL)

    exact_keys(
        contract,
        {
            "schema_version",
            "stage",
            "contract_id",
            "status",
            "accepted_composition_r0_r1",
            "inherited_transaction",
            "inherited_preflight",
            "binary_lineage",
            "generation2_public_authority",
            "source_instances",
            "phases",
            "aggregate_target",
            "service_invocation_count",
            "phase_count",
            "unit_file_sha256",
            "production_linux_amd64_sha256",
            "proof_tool_linux_amd64_sha256",
            "proof_requirements",
            "closed_surfaces",
            "next_allowed_step",
        },
        "Generation-2 transaction contract",
    )
    require(contract["schema_version"] == 2, "contract version drift")
    require(
        contract["contract_id"] == "stage8b-r2b-generation2-full-31-service-transaction-r0",
        "contract identity drift",
    )
    require(
        contract["status"] == "NATIVE_RUNNER_IMPLEMENTED_REVIEW_REQUIRED_NOT_EXECUTED_NOT_ISSUED",
        "contract execution status drift",
    )
    require(
        contract["accepted_composition_r0_r1"]
        == {
            "source_ref": ACCEPTED_COMPOSITION_REF,
            "archive_sha256": ACCEPTED_COMPOSITION_ARCHIVE,
            "verdict": "ACCEPTED",
        },
        "accepted composition lineage drift",
    )
    require(sha256(root / LEGACY_CONTRACT) == LEGACY_CONTRACT_SHA256, "legacy contract drift")
    require(
        contract["inherited_transaction"]
        == {
            "path": LEGACY_CONTRACT.as_posix(),
            "sha256": LEGACY_CONTRACT_SHA256,
            "contract_id": "stage8b-r2b-full-31-service-transaction-r0",
            "topology_changed": False,
        },
        "inherited transaction binding drift",
    )
    require(sha256(root / PREFLIGHT_AUTHORITY) == PREFLIGHT_SHA256, "typed preflight drift")
    require(
        contract["inherited_preflight"]
        == {
            "path": PREFLIGHT_AUTHORITY.as_posix(),
            "sha256": PREFLIGHT_SHA256,
            "accepted_source_ref": ACCEPTED_PREFLIGHT_REF,
            "request_oracle": composition.ORACLE_ID,
        },
        "typed preflight binding drift",
    )
    require(
        preflight_authority["proof_lanes"]["lane_a"]["production_network_boundary_proof"]
        ["outer_runner_evidence_parser"]
        == composition.ORACLE_ID,
        "typed preflight oracle drift",
    )

    require(contract["source_instances"] == inherited["source_instances"] == legacy.SOURCES, "source order drift")
    require(contract["phases"] == inherited["phases"] == legacy.expected_phases(), "phase graph drift")
    require(contract["phase_count"] == 6, "phase count drift")
    require(
        sum(len(phase["invocations"]) for phase in contract["phases"]) == 31
        and contract["service_invocation_count"] == 31,
        "service invocation count drift",
    )
    require(contract["aggregate_target"] == inherited["aggregate_target"], "aggregate target drift")
    require(contract["unit_file_sha256"] == inherited["unit_file_sha256"], "unit topology binding drift")
    require(len(contract["unit_file_sha256"]) == 18, "unit inventory drift")
    for relative, digest in contract["unit_file_sha256"].items():
        require(sha256(root / relative) == digest, f"unit content drift: {relative}")

    require(sha256(root / UPSTREAM_BUILD) == UPSTREAM_BUILD_SHA256, "upstream build evidence drift")
    require(sha256(root / GENERATION2_BUILD) == GENERATION2_BUILD_SHA256, "Generation-2 build evidence drift")
    require(contract["production_linux_amd64_sha256"] == EXPECTED_BINARIES, "binary inventory drift")
    require(len(contract["production_linux_amd64_sha256"]) == 12, "binary count drift")
    require(contract["proof_tool_linux_amd64_sha256"] == EXPECTED_PROOF_TOOLS, "proof-tool inventory drift")
    for name in UPSTREAM_NAMES:
        require(
            inherited["production_linux_amd64_sha256"][name] == EXPECTED_BINARIES[name],
            f"inherited upstream binary drift: {name}",
        )
        record = upstream_build["production_binaries"].get(name)
        require(isinstance(record, dict), f"upstream build record missing: {name}")
        require(
            record.get("build_a_sha256") == EXPECTED_BINARIES[name]
            and record.get("build_b_sha256") == EXPECTED_BINARIES[name],
            f"upstream reproducibility drift: {name}",
        )
    generation2_records = generation2_build.get("binaries")
    require(isinstance(generation2_records, dict), "Generation-2 build inventory missing")
    for contract_name, build_name in GENERATION2_BUILD_NAMES.items():
        record = generation2_records.get(build_name)
        require(isinstance(record, dict), f"Generation-2 build record missing: {build_name}")
        require(
            record.get("build_a_sha256") == EXPECTED_BINARIES[contract_name]
            and record.get("build_b_sha256") == EXPECTED_BINARIES[contract_name],
            f"Generation-2 reproducibility drift: {contract_name}",
        )

    lineage = contract["binary_lineage"]
    exact_keys(lineage, {"phase1_phase2", "phase3_phase6"}, "binary lineage")
    require(
        lineage["phase1_phase2"]
        == {
            "classification": "INHERITED_UNCHANGED_UPSTREAM_BINARIES",
            "build_evidence": UPSTREAM_BUILD.as_posix(),
            "build_evidence_sha256": UPSTREAM_BUILD_SHA256,
            "binary_count": 6,
        },
        "upstream binary lineage drift",
    )
    require(
        lineage["phase3_phase6"]
        == {
            "classification": "ACCEPTED_GENERATION2_BINARIES",
            "build_evidence": GENERATION2_BUILD.as_posix(),
            "build_evidence_sha256": GENERATION2_BUILD_SHA256,
            "build_source_ref": composition.BUILD_SOURCE_REF,
            "build_source_tree": composition.BUILD_SOURCE_TREE,
            "binary_count": 6,
            "production_binaries_rebuilt_by_this_stage": False,
        },
        "Generation-2 binary lineage drift",
    )
    require(sha256(root / GENERATION2_AUTHORITY) == GENERATION2_AUTHORITY_SHA256, "Generation-2 authority drift")
    require(
        contract["generation2_public_authority"]
        == {
            "path": GENERATION2_AUTHORITY.as_posix(),
            "sha256": GENERATION2_AUTHORITY_SHA256,
            "generation": 2,
            "authorization": "NOT_ISSUED",
        },
        "Generation-2 public authority binding drift",
    )
    require(
        generation2_authority.get("authorization_status") == "NOT_ISSUED",
        "Generation-2 authority issued",
    )

    expected_proof = {
        "disposable_linux_amd64_host_required": True,
        "native_execution_required": True,
        "qemu_emulation_allowed": False,
        "production_account_host_allowed": False,
        "sensitive_cotenant_allowed": False,
        "container_network_mode": "none",
        "default_route_allowed": False,
        "dns_allowed": False,
        "finam_network_allowed": False,
        "fresh_review_extraction_required": True,
        "exact_binary_hash_preflight_required": True,
        "exact_unit_hash_preflight_required": True,
        "exact_phase_graph_required": True,
        "phase_failure_propagation_required": True,
        "stale_replay_rejection_required": True,
        "clean_second_run_required": True,
        "raw_redacted_root_terminal_required": True,
        "redacted_helper_journal_required": True,
        "typed_derived_proof_required": True,
        "timeout_stage_exact_request_required": True,
        "reset_before_second_run_required": True,
        "post_proof_uninstall_required": True,
        "private_material_export_allowed": False,
    }
    require(contract["proof_requirements"] == expected_proof, "native proof requirements drift")
    expected_closed = {
        "generation_2_active",
        "authorization_issued",
        "production_credentials_installed",
        "production_host_installation",
        "external_finam_network",
        "broker_get",
        "http_post_delete",
        "broker_dispatch",
        "redis_live",
        "runtime_live",
        "real_orders",
    }
    require(set(contract["closed_surfaces"]) == expected_closed, "closed-surface schema drift")
    require(all(value is False for value in contract["closed_surfaces"].values()), "closed surface opened")
    require(
        contract["next_allowed_step"]
        == "INDEPENDENT_REVIEW_OF_NATIVE_RUNNER_THEN_DISPOSABLE_EXECUTION",
        "next-step drift",
    )

    outer = (root / NATIVE_RUNNER).read_text(encoding="utf-8")
    inner = (root / CONTAINER_RUNNER).read_text(encoding="utf-8")
    materializer = (root / MANIFEST_MATERIALIZER).read_text(encoding="utf-8")
    require(
        outer.index('python3 "$repo_root/scripts/stage8b_p_r2b_generation2_full_transaction_native_r0_host_preflight.py"')
        < outer.index('docker create --privileged'),
        "host preflight must precede container creation",
    )
    for marker in (
        "--network none",
        "--tmpfs /run:",
        "run-1/run-result.json",
        "run-2/run-result.json",
        "container_destroyed",
        "NOT_ISSUED",
    ):
        require(marker in outer, f"native outer runner marker missing: {marker}")
    for forbidden in ("--platform", "/proc/sys/fs/binfmt", "qemu-x86", "--network host"):
        require(forbidden not in outer.lower(), f"native outer runner forbidden marker: {forbidden}")
    for marker in (
        "install_payload",
        "verify_installed_payload",
        "run_transaction run-1",
        "run_transaction run-2",
        "reset_transaction_namespace",
        "uninstall_payload",
        "root-terminal.redacted.json",
        "helper-journal.redacted.txt",
        "native_r0_terminal_oracle.py",
    ):
        require(marker in inner, f"native inner runner marker missing: {marker}")
    for forbidden in ("sed -i", "ExecStart=", ".service.d", "curl ", "wget "):
        require(forbidden not in inner, f"production mutation/network tool forbidden: {forbidden}")
    require(
        'fields["account_key_generation_id"] = "2"' in materializer,
        "Generation-2 manifest binding missing",
    )

    with (root / MATRIX).open(newline="", encoding="utf-8") as handle:
        rows = list(csv.DictReader(handle))
    require(
        len(rows) == 48
        and [row["id"] for row in rows] == [f"G2FTN-{index:03d}" for index in range(1, 49)],
        "acceptance matrix inventory drift",
    )
    require(all(row["status"] == "PASS" and all(row.values()) for row in rows), "acceptance matrix incomplete")
    design = (root / DESIGN).read_text(encoding="utf-8")
    for marker in (
        ACCEPTED_COMPOSITION_REF,
        "31 exact",
        "QEMU, Rosetta and",
        "POST /v1/sessions",
        'timeout_stage == "request"',
        "raw redacted root terminal envelope",
        "R2B authorization: `NOT_ISSUED`",
    ):
        require(marker in design, f"design marker missing: {marker}")

    exact_keys(
        authority,
        {
            "schema_version",
            "stage",
            "status",
            "accepted_predecessor",
            "artifacts",
            "static_rebind",
            "host_assessment",
            "ceremony_custody",
            "execution_state",
            "activation",
            "closed_surfaces",
            "next_allowed_step",
        },
        "native R0 authority",
    )
    require(authority["schema_version"] == 1, "authority version drift")
    require(
        authority["status"]
        == "NATIVE_RUNNER_IMPLEMENTED_REVIEW_REQUIRED_EXECUTION_NOT_STARTED",
        "authority status drift",
    )
    require(
        authority["accepted_predecessor"]
        == {
            "source_ref": ACCEPTED_COMPOSITION_REF,
            "archive_sha256": ACCEPTED_COMPOSITION_ARCHIVE,
            "verdict": "ACCEPTED",
        },
        "authority predecessor drift",
    )
    expected_artifacts = {
        relative.as_posix(): sha256(root / relative)
        for relative in (
            CONTRACT,
            DESIGN,
            MATRIX,
            HOST_ATTESTATION_EXAMPLE,
            TERMINAL_ORACLE,
            HOST_PREFLIGHT,
            CEREMONY_PREFLIGHT,
            NATIVE_RUNNER,
            CONTAINER_RUNNER,
            MANIFEST_MATERIALIZER,
            HANDOFF_MAKER,
            HANDOFF_SAFETY,
        )
    }
    require(authority["artifacts"] == expected_artifacts, "authority artifact binding drift")
    require(
        authority["static_rebind"]
        == {
            "phase_count": 6,
            "service_invocation_count": 31,
            "production_unit_file_count": 18,
            "proof_trigger_file_count": 1,
            "production_binary_count": 12,
            "proof_tool_binary_count": 2,
            "inherited_phase1_phase2_binary_count": 6,
            "generation2_phase3_phase6_binary_count": 6,
            "production_binaries_rebuilt": False,
            "negative_cases": 43,
        },
        "authority static-rebind summary drift",
    )
    require(
        authority["host_assessment"]
        == {
            "developer_docker_daemon_architecture": "aarch64",
            "developer_workstation_native_proof_eligible": False,
            "known_broker_vps_architecture": "x86_64",
            "known_broker_vps_sensitive_trading_host": True,
            "known_broker_vps_native_proof_eligible": False,
            "eligible_disposable_linux_amd64_host_identified": True,
            "eligible_host_contains_trading_workloads": False,
        },
        "authority host assessment drift",
    )
    require(
        authority["ceremony_custody"]
        == {
            "accepted_backup_restore_ref": "3029bab714f8b75daaba3946ed858426515b4165",
            "accepted_backup_restore_archive_sha256": "ee7deefa31dcf6b126408452f4772081ba20999c90ef58cf52df7b873869759f",
            "encrypted_backup_file_sha256": "11970a7b173b20ceff2cee9c4347ce38df4e6d6b973271ba65c8e95b7ef7d8a2",
            "temporary_restore_verified": True,
            "signing_seed_bindings_verified": 13,
            "account_key_bindings_verified": 1,
            "plaintext_ceremony_retained": False,
            "private_material_exported": False,
        },
        "authority ceremony custody drift",
    )
    expected_execution = {
        "native_container_created",
        "units_installed",
        "binaries_installed",
        "credentials_projected",
        "phase_graph_started",
        "run_1_completed",
        "reset_completed",
        "run_2_completed",
        "uninstall_completed",
    }
    require(set(authority["execution_state"]) == expected_execution, "execution-state schema drift")
    require(all(value is False for value in authority["execution_state"].values()), "native execution falsely opened")
    require(
        authority["activation"]
        == {
            "generation": 2,
            "generation_2_active": False,
            "production_credentials_installed": False,
            "controlled_installation_completed": False,
            "authorization": "NOT_ISSUED",
        },
        "authority activation drift",
    )
    require(
        set(authority["closed_surfaces"])
        == {
            "external_finam_network",
            "finam_auth_service",
            "broker_get",
            "http_post_delete",
            "broker_dispatch",
            "redis_live",
            "runtime_live",
            "real_orders",
        }
        and all(value is False for value in authority["closed_surfaces"].values()),
        "authority closed surface opened",
    )
    require(
        authority["next_allowed_step"]
        == "INDEPENDENT_REVIEW_THEN_TWO_RUN_NATIVE_EXECUTION_ON_ATTESTED_DISPOSABLE_HOST",
        "authority next-step drift",
    )
    require(
        vps_rehearsal
        == {
            "schema_version": 1,
            "stage": "Stage 8B-P R2B Generation-2 VPS native static engineering rehearsal",
            "classification": "ENGINEERING_REHEARSAL_NOT_ACCEPTANCE_EVIDENCE",
            "source_ref": "c211c91396fb470fe1113109c6aac5a21b756da6",
            "host_id_sha256": "f6a56abe8959399d8885b02610828fdbfc374a8873f2cd4f99e865ab2755fa7c",
            "kernel_architecture": "x86_64",
            "docker_architecture": "x86_64",
            "sensitive_trading_cotenant_present": True,
            "formal_native_proof_host_eligible": False,
            "separate_project_directory_used": True,
            "generation2_build_a_build_b_artifacts_verified": 16,
            "contract_checker_passed": True,
            "contract_negative_cases_passed": 40,
            "proof_container_created": False,
            "privileged_container_created": False,
            "private_ceremony_transferred": False,
            "credentials_transferred": False,
            "phase_graph_started": False,
            "full_transaction_proof_executed": False,
            "generation_2_active": False,
            "authorization": "NOT_ISSUED",
            "external_finam_network": False,
            "broker_dispatch": False,
            "real_orders": False,
            "result": "PASS_STATIC_ONLY",
            "next_allowed_step": "DISPOSABLE_NATIVE_LINUX_AMD64_HOST_OR_SEPARATELY_REVIEWED_GOVERNANCE_EXCEPTION",
        },
        "VPS static rehearsal drift",
    )


def check(root: Path, artifact_root: Path | None = None) -> None:
    composition.check(root, artifact_root)
    legacy.check(root)
    preflight.check(root)
    check_contract(root)
    print(
        "stage8b-generation2-full-transaction-native-r0-check: PASS "
        "graph=31 phases=6 units=18 binaries=12 generation=2 native_required=true "
        "executed=false active=false authorization=NOT_ISSUED finam=false"
    )


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=ROOT)
    parser.add_argument("--artifact-root", type=Path)
    arguments = parser.parse_args()
    try:
        check(arguments.root.resolve(), arguments.artifact_root)
    except (KeyError, OSError, RuntimeError, ValueError, json.JSONDecodeError) as error:
        raise SystemExit(
            f"stage8b-generation2-full-transaction-native-r0-check: FAIL {error}"
        ) from error


if __name__ == "__main__":
    main()
