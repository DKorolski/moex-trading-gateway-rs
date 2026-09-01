#!/usr/bin/env python3
"""Fail before container creation unless the proof host is disposable native amd64."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import platform
import re
import subprocess
from pathlib import Path
from typing import Any

import stage8b_p_r2b_generation2_full_transaction_native_r0_check as contract_check
import stage8b_p_r2b_generation2_full_transaction_native_r0_ceremony_preflight as ceremony_check


ATTESTATION_KEYS = {
    "schema_version",
    "purpose",
    "host_id",
    "disposable_host",
    "native_linux_amd64",
    "qemu_or_binfmt_execution",
    "production_account_host",
    "sensitive_cotenant_present",
    "broker_credentials_present",
    "trading_workloads_present",
    "authorized_for_destructive_container_cleanup",
}
PURPOSE = "Stage 8B-P R2B Generation-2 native controlled installation proof"
HEX40 = re.compile(r"[0-9a-f]{40}")
HEX64 = re.compile(r"[0-9a-f]{64}")


def require(condition: bool, message: str) -> None:
    if not condition:
        raise RuntimeError(message)


def run(*command: str) -> str:
    return subprocess.check_output(command, text=True, stderr=subprocess.DEVNULL).strip()


def load(path: Path) -> dict[str, Any]:
    value = json.loads(path.read_text(encoding="utf-8"))
    require(isinstance(value, dict), "host attestation must be an object")
    return value


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def check(
    root: Path,
    attestation_path: Path,
    handoff_commit_path: Path,
    expected_archive_sha256: str,
    artifact_root: Path,
    proof_tools_root: Path,
    ceremony_root: Path,
) -> dict[str, object]:
    # Contract validation is deliberately first and does not invoke Docker.
    contract_check.check_contract(root)
    require(platform.system() == "Linux", "native proof requires Linux host")
    require(platform.machine() in {"x86_64", "amd64"}, "native proof requires x86_64 kernel")
    require(run("uname", "-m") == "x86_64", "kernel architecture drift")
    require(run("docker", "info", "--format", "{{.Architecture}}") in {"x86_64", "amd64"}, "Docker daemon is not native amd64")

    attestation = load(attestation_path)
    require(set(attestation) == ATTESTATION_KEYS, "host attestation schema drift")
    require(attestation["schema_version"] == 1 and attestation["purpose"] == PURPOSE, "host attestation identity drift")
    require(isinstance(attestation["host_id"], str) and bool(attestation["host_id"].strip()), "host id missing")
    require(attestation["disposable_host"] is True, "host is not disposable")
    require(attestation["native_linux_amd64"] is True, "native host not attested")
    require(attestation["qemu_or_binfmt_execution"] is False, "emulated execution forbidden")
    for field in (
        "production_account_host",
        "sensitive_cotenant_present",
        "broker_credentials_present",
        "trading_workloads_present",
    ):
        require(attestation[field] is False, f"ineligible host boundary: {field}")
    require(attestation["authorized_for_destructive_container_cleanup"] is True, "destructive cleanup not authorized")

    handoff = handoff_commit_path.read_text(encoding="utf-8")
    source_refs = re.findall(r"(?m)^(?:source_ref|full_sha|commit)\s*=\s*([0-9a-f]{40})\s*$", handoff)
    require(len(set(source_refs)) == 1, "handoff commit binding missing or ambiguous")
    source_ref = source_refs[0]
    require(HEX40.fullmatch(source_ref) is not None, "handoff source ref grammar drift")
    require(run("git", "-C", str(root), "rev-parse", "HEAD") == source_ref, "fresh extraction source mismatch")
    require(HEX64.fullmatch(expected_archive_sha256) is not None, "archive SHA-256 grammar drift")

    contract = json.loads((root / contract_check.CONTRACT).read_text(encoding="utf-8"))
    for relative, expected in contract["unit_file_sha256"].items():
        require(digest(root / relative) == expected, f"unit hash drift: {relative}")

    require(artifact_root.is_dir() and artifact_root.resolve(strict=True) == artifact_root, "artifact root must be canonical")
    for name, expected in contract["production_linux_amd64_sha256"].items():
        binary = artifact_root / name
        require(binary.is_file() and not binary.is_symlink(), f"binary missing: {name}")
        require(digest(binary) == expected, f"binary hash drift: {name}")
    require(
        {path.name for path in artifact_root.iterdir() if path.is_file()}
        == set(contract["production_linux_amd64_sha256"]),
        "binary artifact inventory drift",
    )
    require(proof_tools_root.is_dir() and proof_tools_root.resolve(strict=True) == proof_tools_root, "proof-tool root must be canonical")
    for name, expected in contract["proof_tool_linux_amd64_sha256"].items():
        binary = proof_tools_root / name
        require(binary.is_file() and not binary.is_symlink(), f"proof tool missing: {name}")
        require(digest(binary) == expected, f"proof tool hash drift: {name}")
    require(
        {path.name for path in proof_tools_root.iterdir() if path.is_file()}
        == set(contract["proof_tool_linux_amd64_sha256"]),
        "proof-tool inventory drift",
    )
    ceremony = ceremony_check.check(root, ceremony_root)

    # The caller verifies the extracted ELF inventory before invoking this
    # preflight. This public result intentionally contains no ceremony path.
    return {
        "schema_version": 1,
        "stage": "Stage 8B-P R2B Generation-2 native host preflight",
        "result": "PASS",
        "source_ref": source_ref,
        "archive_sha256": expected_archive_sha256,
        "host_id_sha256": hashlib.sha256(attestation["host_id"].encode()).hexdigest(),
        "kernel_architecture": "x86_64",
        "docker_architecture": "amd64",
        "native_execution": True,
        "qemu_emulation": False,
        "disposable_host": True,
        "sensitive_cotenant_present": False,
        "contract_sha256": digest(root / contract_check.CONTRACT),
        "unit_hashes_verified": len(contract["unit_file_sha256"]),
        "binary_hashes_verified": len(contract["production_linux_amd64_sha256"]),
        "proof_tool_hashes_verified": len(contract["proof_tool_linux_amd64_sha256"]),
        **ceremony,
        "container_created": False,
        "authorization": "NOT_ISSUED",
        "external_finam_network": False,
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, required=True)
    parser.add_argument("--attestation", type=Path, required=True)
    parser.add_argument("--handoff-commit", type=Path, required=True)
    parser.add_argument("--archive-sha256", required=True)
    parser.add_argument("--artifact-root", type=Path, required=True)
    parser.add_argument("--proof-tools-root", type=Path, required=True)
    parser.add_argument("--ceremony-root", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    arguments = parser.parse_args()
    if arguments.output.exists() or not arguments.output.parent.is_dir():
        raise SystemExit("stage8b-generation2-native-host-preflight: FAIL unsafe output")
    try:
        result = check(
            arguments.root.resolve(strict=True),
            arguments.attestation.resolve(strict=True),
            arguments.handoff_commit.resolve(strict=True),
            arguments.archive_sha256,
            arguments.artifact_root.resolve(strict=True),
            arguments.proof_tools_root.resolve(strict=True),
            arguments.ceremony_root.resolve(strict=True),
        )
    except (KeyError, OSError, RuntimeError, subprocess.CalledProcessError, json.JSONDecodeError) as error:
        raise SystemExit(f"stage8b-generation2-native-host-preflight: FAIL {error}") from None
    arguments.output.write_text(json.dumps(result, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print("stage8b-generation2-native-host-preflight: PASS native=amd64 container_created=false authorization=NOT_ISSUED")


if __name__ == "__main__":
    main()
