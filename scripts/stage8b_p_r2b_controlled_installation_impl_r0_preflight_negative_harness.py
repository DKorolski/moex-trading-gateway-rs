#!/usr/bin/env python3
"""Execution-semantics mutations for Controlled Installation Preflight R1."""

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
TRIGGER = Path("deploy/stage8b-r2b-proof/stage8b-r2b-controlled-proof-trigger.service")
MATRIX = Path("docs/stage-8/STAGE8B_P_R2B_CONTROLLED_INSTALLATION_IMPL_R0_PREFLIGHT_ACCEPTANCE_MATRIX_2026-08-30.csv")
FILES = (
    CHECKER, AUTHORITY, INVENTORY, CEREMONY, RESET, TRIGGER, MATRIX,
    Path("docs/stage-8/STAGE8B_P_R2B_CONTROLLED_INSTALLATION_IMPL_R0_PREFLIGHT_2026-08-30.md"),
    Path("docs/stage-8/stage8b-p-r2b-controlled-installation-r0-authority.json"),
    Path("docs/stage-8/stage8b-p-r2b-preproduction-supersession.json"),
    Path("docs/stage-8/stage8b-p-r2b-implementation-transaction-contract.json"),
    Path("docs/current-status.md"), Path("tools/stage8b-readonly-preflight/src/r2a3.rs"),
)


def write_json(path: Path, value: dict) -> None:
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def mutate_json(root: Path, relative: Path, keys: tuple[str | int, ...], replacement: object) -> None:
    path = root / relative
    value = json.loads(path.read_text(encoding="utf-8"))
    cursor: object = value
    for key in keys[:-1]:
        cursor = cursor[key]  # type: ignore[index]
    cursor[keys[-1]] = replacement  # type: ignore[index]
    write_json(path, value)


def delete_json(root: Path, relative: Path, keys: tuple[str, ...]) -> None:
    path = root / relative
    value = json.loads(path.read_text(encoding="utf-8"))
    cursor = value
    for key in keys[:-1]:
        cursor = cursor[key]
    del cursor[keys[-1]]
    write_json(path, value)


def replace_text(root: Path, relative: Path, old: str, new: str) -> None:
    path = root / relative
    text = path.read_text(encoding="utf-8")
    if old not in text:
        raise RuntimeError(f"mutation source missing: {old}")
    path.write_text(text.replace(old, new, 1), encoding="utf-8")


def remove_list_item(root: Path, relative: Path, key: str, needle: str) -> None:
    path = root / relative
    value = json.loads(path.read_text(encoding="utf-8"))
    value[key] = [item for item in value[key] if needle not in item]
    write_json(path, value)


CASES: list[tuple[str, Callable[[Path], None]]] = [
    ("production-success-claimed-under-network-none", lambda r: mutate_json(r, AUTHORITY, ("proof_lanes", "lane_a", "aggregate_target_expected_success"), True)),
    ("production-helper-FINAM-base-url-drift", lambda r: replace_text(r, Path("tools/stage8b-readonly-preflight/src/r2a3.rs"), 'pub const PRODUCTION_BASE_URL: &str = "https://api.finam.ru";', 'pub const PRODUCTION_BASE_URL: &str = "https://localhost";')),
    ("controlled-helper-counted-as-production-proof", lambda r: mutate_json(r, AUTHORITY, ("proof_lanes", "lane_b", "counted_as_production_binary_proof"), True)),
    ("expected-phase6-network-failure-not-recorded", lambda r: mutate_json(r, AUTHORITY, ("proof_lanes", "lane_a", "expected_terminal_classes"), [])),
    ("outer-pass-removed", lambda r: mutate_json(r, AUTHORITY, ("proof_lanes", "lane_a", "outer_runner_expected_success"), False)),
    ("new-canary-domain-used-with-production-authority", lambda r: mutate_json(r, CEREMONY, ("lane_a_exact_production", "ceremony_class"), "NEW_RANDOM_CANARY")),
    ("canary-trust-manifest-mismatch", lambda r: mutate_json(r, CEREMONY, ("lane_a_exact_production", "accepted_fingerprints", "trust_manifest_sha256"), "0" * 64)),
    ("canary-package-key-mismatch", lambda r: mutate_json(r, CEREMONY, ("lane_a_exact_production", "accepted_fingerprints", "authorization_public_key_sha256"), "0" * 64)),
    ("canary-source-key-mismatch", lambda r: mutate_json(r, CEREMONY, ("lane_a_exact_production", "accepted_fingerprints", "public_key_set_sha256"), "0" * 64)),
    ("accepted-account-key-mismatch", lambda r: mutate_json(r, CEREMONY, ("lane_a_exact_production", "accepted_fingerprints", "account_key_manifest_sha256"), "0" * 64)),
    ("accepted-offline-ceremony-fingerprint-drift", lambda r: mutate_json(r, CEREMONY, ("lane_a_exact_production", "execution_precondition"), "SKIP_FINGERPRINTS")),
    ("random-production-key-generation", lambda r: mutate_json(r, CEREMONY, ("lane_a_exact_production", "new_random_key_generation_allowed"), True)),
    ("missing-ceremony-allowed", lambda r: mutate_json(r, AUTHORITY, ("proof_lanes", "lane_a", "execution_without_matching_ceremony_allowed"), True)),
    ("aggregate-trigger-missing", lambda r: (r / TRIGGER).unlink()),
    ("aggregate-direct-manual-start", lambda r: mutate_json(r, AUTHORITY, ("trigger", "direct_aggregate_manual_start_allowed"), True)),
    ("trigger-not-bound-to-exact-aggregate", lambda r: replace_text(r, TRIGGER, "Requires=moex-stage8b-r2b-issuance.target", "Requires=network.target")),
    ("trigger-after-edge-removed", lambda r: replace_text(r, TRIGGER, "After=moex-stage8b-r2b-issuance.target", "After=basic.target")),
    ("trigger-enabled", lambda r: mutate_json(r, AUTHORITY, ("trigger", "enabled"), True)),
    ("trigger-not-removed-after-proof", lambda r: remove_list_item(r, RESET, "unit_destinations", "controlled-proof-trigger")),
    ("production-dropin-allowed", lambda r: mutate_json(r, AUTHORITY, ("trigger", "production_dropins_allowed"), True)),
    ("uninstall-unit-inventory-incomplete", lambda r: mutate_json(r, RESET, ("unit_destinations", 0), "/missing")),
    ("uninstall-binary-inventory-incomplete", lambda r: mutate_json(r, RESET, ("binary_destinations", 0), "/missing")),
    ("r2a7-unit-left-installed", lambda r: remove_list_item(r, RESET, "unit_destinations", "stage8b-r2a7-source-adapter")),
    ("r2a8-unit-left-installed", lambda r: remove_list_item(r, RESET, "unit_destinations", "stage8b-r2a8-current-manifest")),
    ("r2a7-binary-left-installed", lambda r: remove_list_item(r, RESET, "binary_destinations", "/stage8b-r2a7/")),
    ("r2a8-binary-left-installed", lambda r: remove_list_item(r, RESET, "binary_destinations", "/stage8b-r2a8/")),
    ("wildcard-cleanup-authority", lambda r: mutate_json(r, RESET, ("post_proof_uninstall", "wildcard_is_cleanup_authority"), True)),
    ("failure-cleanup-optional", lambda r: mutate_json(r, RESET, ("post_proof_uninstall", "required_on_failure"), False)),
    ("private-material-reused", lambda r: mutate_json(r, RESET, ("reset_before_second_run", "reuse_first_run_private_material"), True)),
    ("second-run-claimed-success", lambda r: mutate_json(r, RESET, ("reset_before_second_run", "second_run_expected_result"), "AGGREGATE_SUCCESS")),
    ("final-systemd-image-unpinned", lambda r: mutate_json(r, INVENTORY, ("image", "image_id"), "latest")),
    ("final-systemd-image-rebuild", lambda r: mutate_json(r, INVENTORY, ("image", "rebuild_under_same_tag_allowed"), True)),
    ("systemd-version-drift", lambda r: mutate_json(r, INVENTORY, ("image", "systemd_version"), "255")),
    ("reviewed-source-is-working-tree", lambda r: mutate_json(r, INVENTORY, ("source_mount", "developer_working_tree_allowed"), True)),
    ("source-manifest-optional", lambda r: mutate_json(r, INVENTORY, ("source_mount", "source_manifest_complete_required"), False)),
    ("docker-network-bridge", lambda r: mutate_json(r, INVENTORY, ("docker_run", "exact_flags", 3), "--network=bridge")),
    ("docker-socket-mounted", lambda r: mutate_json(r, INVENTORY, ("docker_run", "docker_socket_mount_allowed"), True)),
    ("artifact-root-optional", lambda r: mutate_json(r, AUTHORITY, ("artifact_root", "required"), False)),
    ("authorization-issued", lambda r: mutate_json(r, AUTHORITY, ("authorization",), "ISSUED")),
    ("closed-surface-removed", lambda r: delete_json(r, AUTHORITY, ("closed_surfaces", "finam_network"))),
]


def main() -> None:
    passed = 0
    for name, mutation in CASES:
        with tempfile.TemporaryDirectory(prefix=f"stage8b-r2b-preflight-r1-{name}-") as temporary:
            root = Path(temporary)
            for relative in FILES:
                target = root / relative
                target.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(ROOT / relative, target)
            mutation(root)
            result = subprocess.run([sys.executable, str(root / CHECKER), "--root", str(root)], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=False)
            if result.returncode == 0:
                raise SystemExit(f"stage8b-p-r2b-controlled-installation-impl-r0-preflight-r1-negative: FAIL accepted {name}")
        print(f"PASS {name}")
        passed += 1
    print(f"stage8b-p-r2b-controlled-installation-impl-r0-preflight-r1-negative: PASS {passed}/{len(CASES)}")


if __name__ == "__main__":
    main()
