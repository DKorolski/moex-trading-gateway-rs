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
        "schema_version": 1,
        "purpose": preflight.PURPOSE,
        "host_id": "synthetic-disposable-host",
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
)


def copy_contract(root: Path) -> None:
    for relative in contract.contract_required_paths(contract.ROOT):
        target = root / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(contract.ROOT / relative, target)


def invoke(root: Path, attestation: dict[str, object], source_ref: str) -> None:
    attestation_path = root / "host-attestation.json"
    handoff_path = root / "handoff-commit.txt"
    attestation_path.write_text(json.dumps(attestation) + "\n", encoding="utf-8")
    handoff_path.write_text(f"source_ref={source_ref}\n", encoding="utf-8")
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
    preflight.platform.system = lambda: "Linux"
    preflight.platform.machine = lambda: "x86_64"

    def synthetic_run(*command: str) -> str:
        if command[:2] == ("uname", "-m"):
            return "x86_64"
        if command[:2] == ("docker", "info"):
            return "amd64"
        if command[:3] == ("git", "-C", str(root)):
            return source_ref
        raise RuntimeError("unexpected synthetic command")

    preflight.run = synthetic_run
    preflight.ceremony_check.check = lambda _root, _ceremony: {
        "signing_seed_bindings_verified": 13,
        "account_key_bindings_verified": 1,
        "trust_manifest_sha256": "2" * 64,
        "account_key_manifest_sha256": "3" * 64,
        "private_path_exported": False,
        "private_value_exported": False,
    }
    preflight.check(
        root,
        attestation_path,
        handoff_path,
        "1" * 64,
        artifact_root.resolve(strict=True),
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
