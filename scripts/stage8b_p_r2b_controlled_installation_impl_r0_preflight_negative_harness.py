#!/usr/bin/env python3
"""Targeted mutations for the controlled-installation Implementation R0 preflight."""

from __future__ import annotations

import json
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Callable

ROOT = Path(__file__).resolve().parents[1]
CHECKER = Path("scripts/stage8b_p_r2b_controlled_installation_impl_r0_preflight_check.py")
AUTHORITY = Path("docs/stage-8/stage8b-p-r2b-controlled-installation-impl-r0-preflight-authority.json")
INVENTORY = Path("docs/stage-8/stage8b-p-r2b-controlled-installation-impl-r0-staging-inventory.json")
CEREMONY = Path("docs/stage-8/stage8b-p-r2b-controlled-installation-impl-r0-canary-ceremony.json")
RESET = Path("docs/stage-8/stage8b-p-r2b-controlled-installation-impl-r0-reset-uninstall.json")
MATRIX = Path("docs/stage-8/STAGE8B_P_R2B_CONTROLLED_INSTALLATION_IMPL_R0_PREFLIGHT_ACCEPTANCE_MATRIX_2026-08-30.csv")
FILES = (
    CHECKER, AUTHORITY, INVENTORY, CEREMONY, RESET, MATRIX,
    Path("docs/stage-8/STAGE8B_P_R2B_CONTROLLED_INSTALLATION_IMPL_R0_PREFLIGHT_2026-08-30.md"),
    Path("docs/stage-8/stage8b-p-r2b-controlled-installation-r0-authority.json"),
    Path("docs/stage-8/stage8b-p-r2b-preproduction-supersession.json"),
    Path("docs/stage-8/stage8b-p-r2b-implementation-transaction-contract.json"),
    Path("docs/current-status.md"),
)


def write_json(path: Path, value: dict) -> None:
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def mutate_json(root: Path, relative: Path, keys: tuple[str, ...], replacement: object) -> None:
    path = root / relative
    value = json.loads(path.read_text(encoding="utf-8"))
    cursor = value
    for key in keys[:-1]:
        cursor = cursor[key]
    cursor[keys[-1]] = replacement
    write_json(path, value)


def delete_key(root: Path, relative: Path, keys: tuple[str, ...]) -> None:
    path = root / relative
    value = json.loads(path.read_text(encoding="utf-8"))
    cursor = value
    for key in keys[:-1]:
        cursor = cursor[key]
    del cursor[keys[-1]]
    write_json(path, value)


def append_mount(root: Path) -> None:
    path = root / INVENTORY
    value = json.loads(path.read_text(encoding="utf-8"))
    value["mounts"].append({"source_class": "HOST_ROOT", "destination": "/host", "mode": "rw"})
    write_json(path, value)


def remove_binary(root: Path) -> None:
    path = root / INVENTORY
    value = json.loads(path.read_text(encoding="utf-8"))
    del value["production_linux_amd64_sha256"]["stage8b-r2b-launcher"]
    write_json(path, value)


def reorder_sources(root: Path) -> None:
    path = root / CEREMONY
    value = json.loads(path.read_text(encoding="utf-8"))
    value["key_inventory"]["source_names"][0:2] = reversed(value["key_inventory"]["source_names"][0:2])
    write_json(path, value)


def matrix_row_forgery(root: Path) -> None:
    path = root / MATRIX
    lines = path.read_text(encoding="utf-8").splitlines()
    lines[1] = "CIPF-001,lineage,forged requirement,forged evidence,PASS"
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")


CASES: list[tuple[str, Callable[[Path], None]]] = [
    ("accepted-design-ref-drift", lambda root: mutate_json(root, AUTHORITY, ("accepted_design", "source_ref"), "0" * 40)),
    ("accepted-design-archive-drift", lambda root: mutate_json(root, AUTHORITY, ("accepted_design", "archive_sha256"), "0" * 64)),
    ("accepted-implementation-ref-drift", lambda root: mutate_json(root, AUTHORITY, ("accepted_implementation", "source_ref"), "0" * 40)),
    ("inherited-contract-rebound", lambda root: mutate_json(root, AUTHORITY, ("inherited_contract_sha256", "docs/stage-8/stage8b-p-r2b-implementation-transaction-contract.json"), "0" * 64)),
    ("artifact-root-optional", lambda root: mutate_json(root, AUTHORITY, ("artifact_root", "required"), False)),
    ("artifact-root-embedded", lambda root: mutate_json(root, AUTHORITY, ("artifact_root", "accepted_predecessor_archive_embedded"), True)),
    ("execution-authorized", lambda root: mutate_json(root, AUTHORITY, ("execution_state", "installation_authorized_by_this_package"), True)),
    ("proof-claimed", lambda root: mutate_json(root, AUTHORITY, ("execution_state", "proof_executed"), True)),
    ("authorization-issued", lambda root: mutate_json(root, AUTHORITY, ("authorization",), "ISSUED")),
    ("closed-surface-removed", lambda root: delete_key(root, AUTHORITY, ("closed_surfaces", "finam_network"))),
    ("finam-opened", lambda root: mutate_json(root, AUTHORITY, ("closed_surfaces", "finam_network"), True)),
    ("production-host", lambda root: mutate_json(root, INVENTORY, ("host", "production_account_host"), True)),
    ("network-bridge", lambda root: mutate_json(root, INVENTORY, ("contour", "network_mode"), "bridge")),
    ("default-route", lambda root: mutate_json(root, INVENTORY, ("contour", "default_route_allowed"), True)),
    ("host-root-mount", append_mount),
    ("missing-binary", remove_binary),
    ("binary-hash-drift", lambda root: mutate_json(root, INVENTORY, ("production_linux_amd64_sha256", "stage8b-r2b-launcher"), "0" * 64)),
    ("enablement-opened", lambda root: mutate_json(root, INVENTORY, ("installation", "enablement_allowed"), True)),
    ("ceremony-id-drift", lambda root: mutate_json(root, CEREMONY, ("ceremony_id",), "production")),
    ("ceremony-persistent", lambda root: mutate_json(root, CEREMONY, ("materialization", "storage"), "disk")),
    ("private-export", lambda root: mutate_json(root, CEREMONY, ("materialization", "private_material_export_allowed"), True)),
    ("real-token", lambda root: mutate_json(root, CEREMONY, ("identity", "real_broker_token_allowed"), True)),
    ("source-order-drift", reorder_sources),
    ("private-evidence", lambda root: mutate_json(root, CEREMONY, ("evidence_policy", "private_key_bytes_allowed"), True)),
    ("ceremony-claimed", lambda root: mutate_json(root, CEREMONY, ("current_state", "ceremony_executed"), True)),
    ("reset-optional", lambda root: mutate_json(root, RESET, ("reset_before_second_run", "required"), False)),
    ("private-reuse", lambda root: mutate_json(root, RESET, ("reset_before_second_run", "reuse_first_run_private_material"), True)),
    ("failure-cleanup-optional", lambda root: mutate_json(root, RESET, ("post_proof_uninstall", "required_on_failure"), False)),
    ("container-retained", lambda root: mutate_json(root, RESET, ("post_proof_uninstall", "destroy_container"), False)),
    ("postcondition-units", lambda root: mutate_json(root, RESET, ("postconditions", "installed_matching_unit_files"), 1)),
    ("matrix-row-forgery", matrix_row_forgery),
]


def main() -> None:
    passed = 0
    for name, mutation in CASES:
        with tempfile.TemporaryDirectory(prefix=f"stage8b-r2b-impl-r0-preflight-{name}-") as temporary:
            root = Path(temporary)
            for relative in FILES:
                target = root / relative
                target.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(ROOT / relative, target)
            mutation(root)
            result = subprocess.run(
                [sys.executable, str(root / CHECKER), "--root", str(root)],
                stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=False,
            )
            if result.returncode == 0:
                raise SystemExit(f"stage8b-p-r2b-controlled-installation-impl-r0-preflight-negative: FAIL accepted {name}")
        print(f"PASS {name}")
        passed += 1
    print(f"stage8b-p-r2b-controlled-installation-impl-r0-preflight-negative: PASS {passed}/{len(CASES)}")


if __name__ == "__main__":
    main()
