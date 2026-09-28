#!/usr/bin/env python3
"""Fail before container creation unless archive, image, ceremony and host are exact."""

from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import json
import platform
import re
import subprocess
from pathlib import Path
from typing import Any

import stage8b_p_r2b_generation2_full_transaction_native_r0_check as contract_check
import stage8b_p_r2b_generation2_full_transaction_native_r0_ceremony_preflight as ceremony_check


ATTESTATION_KEYS = {
    "schema_version", "purpose", "host_id", "machine_id_sha256", "cloud_instance_id_sha256",
    "created_at_utc", "reviewer_approval_sha256", "reviewed_archive_sha256", "container_image_id",
    "disposable_host", "native_linux_amd64", "qemu_or_binfmt_execution", "production_account_host",
    "sensitive_cotenant_present", "broker_credentials_present", "trading_workloads_present",
    "authorized_for_destructive_container_cleanup", "swap_enabled",
}
BINDING_KEYS = {
    "schema_version", "stage", "result", "source_ref", "source_tree", "archive_name", "archive_sha256",
    "reviewer_acceptance_sha256", "fresh_extraction", "source_manifest_verified", "additional_members_rejected",
    "tracked_members_verified", "archive_members_verified", "private_material_members", "native_execution", "authorization",
}
PURPOSE = "Stage 8B-P R2B Generation-2 native controlled installation proof"
HEX40 = re.compile(r"[0-9a-f]{40}")
HEX64 = re.compile(r"[0-9a-f]{64}")
EXPECTED_IMAGE_ID = "sha256:3cc66c640df0444530a626d2acbcfeda9742039b917a747fd023b315ef2c1526"
MAX_ATTESTATION_AGE = dt.timedelta(minutes=15)
MAX_FUTURE_SKEW = dt.timedelta(minutes=1)
SENSITIVE_ROOTS = (
    Path("/opt/trading-hybrid"), Path("/opt/moex-trading-live"),
    Path("/var/lib/moex-trading/production"), Path("/run/credentials/moex-trading/production"),
)


def require(condition: bool, message: str) -> None:
    if not condition:
        raise RuntimeError(message)


def run(*command: str) -> str:
    return subprocess.check_output(command, text=True, stderr=subprocess.DEVNULL).strip()


def load(path: Path) -> dict[str, Any]:
    value = json.loads(path.read_text(encoding="utf-8"))
    require(isinstance(value, dict), f"JSON object required: {path.name}")
    return value


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def host_identity() -> tuple[str, str, str]:
    machine = Path("/etc/machine-id").read_bytes().strip()
    cloud = Path("/sys/class/dmi/id/product_uuid").read_bytes().strip().lower()
    require(bool(machine) and bool(cloud), "host identity source missing")
    machine_digest = hashlib.sha256(machine).hexdigest()
    cloud_digest = hashlib.sha256(cloud).hexdigest()
    host_id = hashlib.sha256(f"{machine_digest}:{cloud_digest}".encode()).hexdigest()
    return host_id, machine_digest, cloud_digest


def check(
    root: Path,
    attestation_path: Path,
    archive_binding_path: Path,
    artifact_root: Path,
    proof_tools_root: Path,
    ceremony_root: Path,
) -> dict[str, object]:
    contract_check.check_contract(root)
    require(platform.system() == "Linux", "native proof requires Linux host")
    require(platform.machine() in {"x86_64", "amd64"}, "native proof requires x86_64 kernel")
    require(run("uname", "-m") == "x86_64", "kernel architecture drift")
    require(run("docker", "info", "--format", "{{.Architecture}}") in {"x86_64", "amd64"}, "Docker daemon is not native amd64")
    require(run("docker", "image", "inspect", "--format", "{{.Id}}", EXPECTED_IMAGE_ID) == EXPECTED_IMAGE_ID, "container image ID drift")
    require(run("docker", "ps", "-aq") == "", "disposable host already has containers")
    require(not any(path.exists() for path in SENSITIVE_ROOTS), "sensitive runtime root present")
    live_swap = run("swapon", "--show", "--noheadings")
    require(live_swap == "", "host swap is enabled")

    binding = load(archive_binding_path)
    require(set(binding) == BINDING_KEYS, "archive binding schema drift")
    require(binding["schema_version"] == 1 and binding["result"] == "PASS", "archive binding result drift")
    require(HEX40.fullmatch(binding["source_ref"]) is not None and HEX40.fullmatch(binding["source_tree"]) is not None, "source binding grammar drift")
    require(HEX64.fullmatch(binding["archive_sha256"]) is not None, "archive digest grammar drift")
    require(HEX64.fullmatch(binding["reviewer_acceptance_sha256"]) is not None, "review acceptance grammar drift")
    require(binding["fresh_extraction"] is True and binding["source_manifest_verified"] is True, "fresh extraction not established")
    require(binding["additional_members_rejected"] is True and binding["private_material_members"] == 0, "archive inventory opened")
    require(binding["native_execution"] is False and binding["authorization"] == "NOT_ISSUED", "archive execution boundary opened")
    marker = {
        key: value
        for line in (root / "handoff-commit.txt").read_text(encoding="utf-8").splitlines()
        if "=" in line
        for key, value in [line.split("=", 1)]
    }
    require(marker.get("source_ref") == binding["source_ref"], "handoff source-ref mismatch")
    require(marker.get("source_tree") == binding["source_tree"], "handoff source-tree mismatch")
    require(marker.get("archive_name") == binding["archive_name"], "handoff archive-name mismatch")

    attestation = load(attestation_path)
    require(set(attestation) == ATTESTATION_KEYS, "host attestation schema drift")
    require(attestation["schema_version"] == 2 and attestation["purpose"] == PURPOSE, "host attestation identity drift")
    expected_host_id, machine_digest, cloud_digest = host_identity()
    require(attestation["host_id"] == expected_host_id, "host ID binding drift")
    require(attestation["machine_id_sha256"] == machine_digest, "machine-id binding drift")
    require(attestation["cloud_instance_id_sha256"] == cloud_digest, "cloud-instance binding drift")
    require(attestation["container_image_id"] == EXPECTED_IMAGE_ID, "attested image drift")
    require(attestation["reviewed_archive_sha256"] == binding["archive_sha256"], "attested archive drift")
    require(attestation["reviewer_approval_sha256"] == binding["reviewer_acceptance_sha256"], "review approval binding drift")
    created = dt.datetime.fromisoformat(attestation["created_at_utc"].replace("Z", "+00:00"))
    require(created.tzinfo is not None, "attestation timestamp must be UTC")
    now = dt.datetime.now(dt.timezone.utc)
    require(created <= now + MAX_FUTURE_SKEW, "attestation timestamp is in the future")
    require(now - created <= MAX_ATTESTATION_AGE, "host attestation is stale")
    require(attestation["disposable_host"] is True and attestation["native_linux_amd64"] is True, "host is not attested disposable amd64")
    require(attestation["qemu_or_binfmt_execution"] is False, "emulated execution forbidden")
    for field in ("production_account_host", "sensitive_cotenant_present", "broker_credentials_present", "trading_workloads_present"):
        require(attestation[field] is False, f"ineligible host boundary: {field}")
    require(attestation["authorized_for_destructive_container_cleanup"] is True, "destructive cleanup not authorized")
    require(attestation["swap_enabled"] is False, "attestation permits swap")

    contract = load(root / contract_check.CONTRACT)
    for relative, expected in contract["unit_file_sha256"].items():
        require(digest(root / relative) == expected, f"unit hash drift: {relative}")
    require(artifact_root.is_dir() and artifact_root.resolve(strict=True) == artifact_root, "artifact root must be canonical")
    require({path.name for path in artifact_root.iterdir() if path.is_file()} == set(contract["production_linux_amd64_sha256"]), "binary artifact inventory drift")
    for name, expected in contract["production_linux_amd64_sha256"].items():
        binary = artifact_root / name
        require(binary.is_file() and not binary.is_symlink() and digest(binary) == expected, f"binary drift: {name}")
    require(proof_tools_root.is_dir() and proof_tools_root.resolve(strict=True) == proof_tools_root, "proof-tool root must be canonical")
    require({path.name for path in proof_tools_root.iterdir() if path.is_file()} == set(contract["proof_tool_linux_amd64_sha256"]), "proof-tool inventory drift")
    for name, expected in contract["proof_tool_linux_amd64_sha256"].items():
        binary = proof_tools_root / name
        require(binary.is_file() and not binary.is_symlink() and digest(binary) == expected, f"proof tool drift: {name}")
    ceremony = ceremony_check.check(root, ceremony_root)

    return {
        "schema_version": 2,
        "stage": "Stage 8B-P R2B Generation-2 native host preflight R2",
        "result": "PASS",
        "source_ref": binding["source_ref"],
        "source_tree": binding["source_tree"],
        "archive_sha256": binding["archive_sha256"],
        "reviewer_acceptance_sha256": binding["reviewer_acceptance_sha256"],
        "host_id_sha256": hashlib.sha256(expected_host_id.encode()).hexdigest(),
        "container_image_id": EXPECTED_IMAGE_ID,
        "kernel_architecture": "x86_64",
        "docker_architecture": "amd64",
        "native_host_verified": True,
        "native_execution": False,
        "qemu_emulation": False,
        "disposable_host": True,
        "sensitive_cotenant_present": False,
        "host_swap_enabled": False,
        "host_swap_entries": 0,
        "attestation_max_age_seconds": int(MAX_ATTESTATION_AGE.total_seconds()),
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
    parser.add_argument("--archive-binding", type=Path, required=True)
    parser.add_argument("--artifact-root", type=Path, required=True)
    parser.add_argument("--proof-tools-root", type=Path, required=True)
    parser.add_argument("--ceremony-root", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    arguments = parser.parse_args()
    if arguments.output.exists() or not arguments.output.parent.is_dir():
        raise SystemExit("stage8b-generation2-native-host-preflight: FAIL unsafe output")
    try:
        result = check(
            arguments.root.resolve(strict=True), arguments.attestation.resolve(strict=True),
            arguments.archive_binding.resolve(strict=True), arguments.artifact_root.resolve(strict=True),
            arguments.proof_tools_root.resolve(strict=True), arguments.ceremony_root.resolve(strict=True),
        )
    except (KeyError, OSError, RuntimeError, subprocess.CalledProcessError, ValueError, json.JSONDecodeError) as error:
        raise SystemExit(f"stage8b-generation2-native-host-preflight: FAIL {error}") from None
    arguments.output.write_text(json.dumps(result, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print("stage8b-generation2-native-host-preflight: PASS native=amd64 image=pinned container_created=false authorization=NOT_ISSUED")


if __name__ == "__main__":
    main()
