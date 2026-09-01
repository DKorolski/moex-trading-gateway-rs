#!/usr/bin/env python3
"""Synthetic host-boundary tests; no Docker container is created."""

from __future__ import annotations

import json
import os
import shutil
import tempfile
from pathlib import Path
from typing import Callable

import stage8b_p_r2b_generation2_full_transaction_native_r0_check as contract
import stage8b_p_r2b_generation2_full_transaction_native_r0_host_preflight as preflight


Mutation = Callable[[dict[str, object]], None]


def baseline() -> dict[str, object]:
    return {
        "schema_version": 2,
        "purpose": preflight.PURPOSE,
        "host_id": "synthetic-disposable-host",
        "machine_id_sha256": "2" * 64,
        "cloud_instance_id_sha256": "3" * 64,
        "created_at_utc": "2026-09-01T00:00:00Z",
        "reviewer_approval_sha256": "4" * 64,
        "reviewed_archive_sha256": "5" * 64,
        "container_image_id": preflight.EXPECTED_IMAGE_ID,
        "disposable_host": True,
        "native_linux_amd64": True,
        "qemu_or_binfmt_execution": False,
        "production_account_host": False,
        "sensitive_cotenant_present": False,
        "broker_credentials_present": False,
        "trading_workloads_present": False,
        "authorized_for_destructive_container_cleanup": True,
    }


CASES: tuple[tuple[str, Mutation], ...] = (
    ("not-disposable", lambda value: value.__setitem__("disposable_host", False)),
    ("native-not-attested", lambda value: value.__setitem__("native_linux_amd64", False)),
    ("qemu", lambda value: value.__setitem__("qemu_or_binfmt_execution", True)),
    ("production-account-host", lambda value: value.__setitem__("production_account_host", True)),
    ("sensitive-cotenant", lambda value: value.__setitem__("sensitive_cotenant_present", True)),
    ("broker-credentials", lambda value: value.__setitem__("broker_credentials_present", True)),
    ("trading-workload", lambda value: value.__setitem__("trading_workloads_present", True)),
    (
        "cleanup-not-authorized",
        lambda value: value.__setitem__("authorized_for_destructive_container_cleanup", False),
    ),
    ("empty-host-id", lambda value: value.__setitem__("host_id", "")),
    ("purpose-drift", lambda value: value.__setitem__("purpose", "another proof")),
    ("schema-extra", lambda value: value.__setitem__("unexpected", False)),
    ("image-id-drift", lambda value: value.__setitem__("container_image_id", "sha256:" + "0" * 64)),
    ("archive-binding-drift", lambda value: value.__setitem__("reviewed_archive_sha256", "0" * 64)),
    ("review-approval-drift", lambda value: value.__setitem__("reviewer_approval_sha256", "0" * 64)),
)


def copy_contract(root: Path) -> None:
    for relative in contract.contract_required_paths(contract.ROOT):
        target = root / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(contract.ROOT / relative, target)


def invoke(root: Path, attestation: dict[str, object], source_ref: str) -> None:
    attestation_path = root / "host-attestation.json"
    binding_path = root / "archive-binding.json"
    attestation_path.write_text(json.dumps(attestation) + "\n", encoding="utf-8")
    binding_path.write_text(json.dumps({
        "schema_version":1,"stage":"synthetic","result":"PASS","source_ref":source_ref,
        "source_tree":"6"*40,"archive_name":"synthetic.zip","archive_sha256":"5"*64,
        "reviewer_acceptance_sha256":"4"*64,"fresh_extraction":True,
        "source_manifest_verified":True,"additional_members_rejected":True,
        "tracked_members_verified":1,"archive_members_verified":1,"private_material_members":0,
        "native_execution":False,"authorization":"NOT_ISSUED",
    }) + "\n", encoding="utf-8")
    (root / "handoff-commit.txt").write_text(
        f"source_ref={source_ref}\nsource_tree={'6'*40}\narchive_name=synthetic.zip\n", encoding="utf-8"
    )
    packaged_artifacts = contract.ROOT / "handoff-evidence/linux-amd64/exact-binaries"
    packaged_tools = contract.ROOT / "handoff-evidence/linux-amd64/proof-tools"
    if packaged_artifacts.is_dir() and packaged_tools.is_dir():
        artifact_root = packaged_artifacts
        proof_tools_root = packaged_tools
    else:
        artifact_root = root / "exact-binaries"
        artifact_root.mkdir()
        for name in contract.EXPECTED_BINARIES:
            source_name = "stage8b-readonly-preflight" if name == "accepted-stage8b-readonly-preflight" else name
            source_root = (
                contract.ROOT / "tmp/stage8b-r2b-r4r2-production-a/release"
                if name in contract.UPSTREAM_NAMES
                else contract.ROOT / "reports/stage8b-p-r2b-generation2-composition-r0/linux-amd64/build-a"
            )
            os.link(source_root / source_name, artifact_root / name)
        proof_tools_root = root / "proof-tools"
        proof_tools_root.mkdir()
        proof_tool_sources = {
            "stage8b-r2a5-controlled-layout": contract.ROOT / "tmp/stage8b-g2-r0-r1-controlled.tB20fg/x86_64-unknown-linux-musl/release/stage8b-r2a5-controlled-layout",
            "stage8b-r2b-creator-chain-seeder": contract.ROOT / "tmp/stage8b-r2b-r4-controlled-a/release/stage8b-r2b-creator-chain-seeder",
            "stage8b-r2b-trust-rebind-key-ceremony-verify": contract.ROOT / "tmp/stage8b-g2-native-r1-verifier-linux-amd64/stage8b-r2b-trust-rebind-key-ceremony-verify",
        }
        for name, source in proof_tool_sources.items():
            os.link(source, proof_tools_root / name)
    preflight.platform.system = lambda: "Linux"
    preflight.platform.machine = lambda: "x86_64"

    def synthetic_run(*command: str) -> str:
        if command[:2] == ("uname", "-m"):
            return "x86_64"
        if command[:2] == ("docker", "info"):
            return "amd64"
        if command[:3] == ("docker", "image", "inspect"):
            return preflight.EXPECTED_IMAGE_ID
        if command[:2] == ("docker", "ps"):
            return ""
        raise RuntimeError("unexpected synthetic command")

    preflight.run = synthetic_run
    preflight.ceremony_check.check = lambda _root, _ceremony: {
        "ceremony_storage": "tmpfs",
        "exact_inventory_verified": True,
        "private_file_metadata_verified": 14,
        "public_manifests_verified": 2,
        "cryptographic_binding_deferred_to_pinned_in_container_verifier": True,
        "trust_manifest_sha256": "2" * 64,
        "account_key_manifest_sha256": "3" * 64,
        "private_path_exported": False,
        "private_value_exported": False,
    }
    preflight.host_identity = lambda: ("synthetic-disposable-host", "2" * 64, "3" * 64)
    preflight.check(
        root,
        attestation_path,
        binding_path,
        artifact_root.resolve(strict=True),
        proof_tools_root.resolve(strict=True),
        root.resolve(strict=True),
    )


def main() -> None:
    source_ref = "1" * 40
    with tempfile.TemporaryDirectory(prefix="stage8b-g2-native-host-positive-") as temporary:
        root = Path(temporary)
        copy_contract(root)
        result = invoke(root, baseline(), source_ref)
        if result is not None:
            pass
        print("PASS synthetic-positive")

    passed = 0
    for name, mutation in CASES:
        with tempfile.TemporaryDirectory(prefix=f"stage8b-g2-native-host-{name}-") as temporary:
            root = Path(temporary)
            copy_contract(root)
            value = baseline()
            mutation(value)
            try:
                invoke(root, value, source_ref)
            except (KeyError, OSError, RuntimeError, ValueError, json.JSONDecodeError):
                passed += 1
                print(f"PASS {name}")
                continue
            raise SystemExit(f"stage8b-generation2-native-host-negative: FAIL accepted={name}")
    print(f"stage8b-generation2-native-host-negative: PASS cases={passed}/{len(CASES)}")


if __name__ == "__main__":
    main()
