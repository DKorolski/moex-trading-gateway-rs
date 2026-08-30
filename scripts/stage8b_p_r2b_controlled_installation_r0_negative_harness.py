#!/usr/bin/env python3
"""Targeted mutations for the controlled-installation R0 design boundary."""

from __future__ import annotations

import copy
import hashlib
import importlib.util
import json
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Callable

ROOT = Path(__file__).resolve().parents[1]
CHECKER_PATH = ROOT / "scripts/stage8b_p_r2b_controlled_installation_r0_check.py"
AUTHORITY = Path("docs/stage-8/stage8b-p-r2b-controlled-installation-r0-authority.json")
SUPERSESSION = Path("docs/stage-8/stage8b-p-r2b-preproduction-supersession.json")
TRANSACTION = Path("docs/stage-8/stage8b-p-r2b-implementation-transaction-contract.json")

spec = importlib.util.spec_from_file_location("controlled_installation_checker", CHECKER_PATH)
if spec is None or spec.loader is None:
    raise SystemExit("cannot load controlled installation checker")
checker = importlib.util.module_from_spec(spec)
spec.loader.exec_module(checker)


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def write_json(path: Path, value: dict) -> None:
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def set_path(value: dict, keys: tuple[str | int, ...], replacement: object) -> None:
    cursor: object = value
    for key in keys[:-1]:
        cursor = cursor[key]  # type: ignore[index]
    cursor[keys[-1]] = replacement  # type: ignore[index]


def rebind(root: Path, relative: Path) -> None:
    authority_path = root / AUTHORITY
    authority = json.loads(authority_path.read_text(encoding="utf-8"))
    key = relative.as_posix()
    if key in authority["design_artifacts"]:
        authority["design_artifacts"][key] = digest(root / relative)
        write_json(authority_path, authority)


def mutate_json(root: Path, relative: Path, keys: tuple[str | int, ...], replacement: object) -> None:
    path = root / relative
    value = json.loads(path.read_text(encoding="utf-8"))
    set_path(value, keys, replacement)
    write_json(path, value)
    rebind(root, relative)


def remove_phase_invocation(root: Path) -> None:
    path = root / TRANSACTION
    value = json.loads(path.read_text(encoding="utf-8"))
    value["phases"][4]["invocations"].pop(0)
    write_json(path, value)
    rebind(root, TRANSACTION)


def reorder_producers(root: Path) -> None:
    path = root / TRANSACTION
    value = json.loads(path.read_text(encoding="utf-8"))
    value["phases"][2]["invocations"][0], value["phases"][2]["invocations"][1] = (
        value["phases"][2]["invocations"][1],
        value["phases"][2]["invocations"][0],
    )
    write_json(path, value)
    rebind(root, TRANSACTION)


def mutate_unit_binding(root: Path) -> None:
    path = root / TRANSACTION
    value = json.loads(path.read_text(encoding="utf-8"))
    key = next(iter(value["unit_file_sha256"]))
    value["unit_file_sha256"][key] = "0" * 64
    write_json(path, value)
    rebind(root, TRANSACTION)


def copy_case_root(destination: Path) -> None:
    for relative in checker.required_paths(ROOT):
        source = ROOT / relative
        target = destination / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source, target)


CASES: list[tuple[str, Callable[[Path], None]]] = [
    ("accepted-predecessor-drift", lambda root: mutate_json(root, AUTHORITY, ("accepted_predecessor", "source_ref"), "0" * 40)),
    ("accepted-archive-drift", lambda root: mutate_json(root, AUTHORITY, ("accepted_predecessor", "archive_sha256"), "0" * 64)),
    ("supersession-old-helper-drift", lambda root: mutate_json(root, SUPERSESSION, ("helper", "old_executable_sha256"), "0" * 64)),
    ("supersession-new-helper-drift", lambda root: mutate_json(root, SUPERSESSION, ("helper", "new_executable_sha256"), "0" * 64)),
    ("supersession-trust-set-drift", lambda root: mutate_json(root, SUPERSESSION, ("trust_set", "new_public_key_set_sha256"), "0" * 64)),
    ("supersession-account-key-drift", lambda root: mutate_json(root, SUPERSESSION, ("account_key_manifest", "new_generation_1_key_sha256"), "0" * 64)),
    ("legacy-path-restored", lambda root: mutate_json(root, SUPERSESSION, ("filesystem_contract", "legacy_paths_authoritative"), True)),
    ("generation-continuity-forged", lambda root: mutate_json(root, SUPERSESSION, ("ceremony_lineage", "same_generation_does_not_assert_key_continuity"), False)),
    ("prior-installation-forged", lambda root: mutate_json(root, SUPERSESSION, ("ceremony_lineage", "production_installation_before_supersession"), True)),
    ("prior-package-issued", lambda root: mutate_json(root, SUPERSESSION, ("ceremony_lineage", "issued_r2b_packages_before_supersession"), 1)),
    ("builder-removed-from-phase5", remove_phase_invocation),
    ("producer-order-drift", reorder_producers),
    ("service-count-weakened", lambda root: mutate_json(root, TRANSACTION, ("service_invocation_count",), 30)),
    ("unit-hash-drift", mutate_unit_binding),
    ("binary-hash-drift", lambda root: mutate_json(root, TRANSACTION, ("production_linux_amd64_sha256", "stage8b-r2b-run-package-draft-builder"), "0" * 64)),
    ("production-host-opened", lambda root: mutate_json(root, AUTHORITY, ("installation_scope", "production_account_host_allowed"), True)),
    ("real-credentials-opened", lambda root: mutate_json(root, AUTHORITY, ("installation_scope", "real_credentials_allowed"), True)),
    ("finam-network-opened", lambda root: mutate_json(root, AUTHORITY, ("installation_scope", "finam_network_allowed"), True)),
    ("authorization-issued", lambda root: mutate_json(root, AUTHORITY, ("authorization",), "ISSUED")),
    ("reset-proof-removed", lambda root: mutate_json(root, TRANSACTION, ("proof_requirements", "transaction_reset_proof_required"), False)),
]


def main() -> None:
    passed = 0
    for name, mutation in CASES:
        with tempfile.TemporaryDirectory(prefix=f"stage8b-r2b-cir0-{name}-") as temporary:
            case_root = Path(temporary)
            copy_case_root(case_root)
            mutation(case_root)
            result = subprocess.run(
                [sys.executable, str(CHECKER_PATH), "--root", str(case_root)],
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                check=False,
            )
            if result.returncode == 0:
                raise SystemExit(f"FAIL mutation accepted: {name}")
        print(f"PASS {name}")
        passed += 1
    print(f"stage8b-p-r2b-controlled-installation-r0-negative: PASS {passed}/{len(CASES)}")


if __name__ == "__main__":
    main()
