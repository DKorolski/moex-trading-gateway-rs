#!/usr/bin/env python3
"""Targeted mutation matrix for Generation-2 Composition Rebuild R0."""

from __future__ import annotations

import json
import shutil
import subprocess
import tempfile
from pathlib import Path
from typing import Callable

import stage8b_p_r2b_generation2_composition_r0_check as checker


ROOT = Path(__file__).resolve().parents[1]


def required_files() -> set[Path]:
    return {
        checker.TRUST, checker.ACCOUNT, checker.SOURCE_ADAPTER,
        checker.PRODUCTION_AUTHORITY, checker.HELPER_AUTHORITY, checker.HELPER_PIN,
        checker.BUILD, checker.REHEARSAL, checker.AUTHORITY, checker.DESIGN,
        checker.MATRIX, checker.STATUS, checker.CORE, checker.LAUNCHER,
        checker.ISSUER, checker.ISSUE_SCRIPT, checker.BUILD_SCRIPT,
        checker.MATERIALIZER, checker.RUNNER, checker.BASE_PHASE6,
        *(Path(path) for path in checker.ACCEPTED_BACKUP_HASHES),
    }


def materialize(destination: Path) -> None:
    for relative in required_files():
        target = destination / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(ROOT / relative, target)


def mutate_json(root: Path, relative: Path, keys: tuple[str, ...], value: object) -> None:
    document = json.loads((root / relative).read_text(encoding="utf-8"))
    cursor = document
    for key in keys[:-1]:
        cursor = cursor[int(key)] if isinstance(cursor, list) else cursor[key]
    if isinstance(cursor, list):
        cursor[int(keys[-1])] = value
    else:
        cursor[keys[-1]] = value
    (root / relative).write_text(json.dumps(document, indent=2) + "\n", encoding="utf-8")


def add_json(root: Path, relative: Path, key: str, value: object) -> None:
    document = json.loads((root / relative).read_text(encoding="utf-8"))
    document[key] = value
    (root / relative).write_text(json.dumps(document, indent=2) + "\n", encoding="utf-8")


def mutate_text(root: Path, relative: Path, old: str, new: str) -> None:
    path = root / relative
    text = path.read_text(encoding="utf-8")
    if text.count(old) != 1:
        raise RuntimeError(f"fixture cardinality drift: {relative}: {old}")
    path.write_text(text.replace(old, new), encoding="utf-8")


def append_text(root: Path, relative: Path, value: str) -> None:
    path = root / relative
    path.write_text(path.read_text(encoding="utf-8") + value, encoding="utf-8")


Mutation = Callable[[Path], None]
CASES: tuple[tuple[str, Mutation], ...] = (
    ("accepted-backup-authority", lambda r: append_text(r, Path(next(iter(checker.ACCEPTED_BACKUP_HASHES))), "\n")),
    ("trust-byte", lambda r: append_text(r, checker.TRUST, "\n")),
    ("trust-mixed-generation", lambda r: mutate_json(r, checker.TRUST, ("authorization_key", "generation"), 1)),
    ("trust-public-set", lambda r: mutate_json(r, checker.TRUST, ("public_key_set_sha256",), "0" * 64)),
    ("account-generation", lambda r: mutate_json(r, checker.ACCOUNT, ("entries", "0", "generation_id"), "1")),
    ("account-path", lambda r: mutate_json(r, checker.ACCOUNT, ("entries", "0", "relative_key_path"), "generation-1.hex")),
    ("source-adapter", lambda r: append_text(r, checker.SOURCE_ADAPTER, "\n")),
    ("production-authority-generation", lambda r: mutate_json(r, checker.PRODUCTION_AUTHORITY, ("revision",), "R2B-G1")),
    ("production-authority-trust", lambda r: mutate_json(r, checker.PRODUCTION_AUTHORITY, ("trust_manifest_sha256",), "0" * 64)),
    ("production-authority-issued", lambda r: mutate_json(r, checker.PRODUCTION_AUTHORITY, ("authorization_status",), "ISSUED")),
    ("production-authority-unknown", lambda r: add_json(r, checker.PRODUCTION_AUTHORITY, "active", True)),
    ("helper-pin", lambda r: (r / checker.HELPER_PIN).write_text("0" * 64 + "\n", encoding="utf-8")),
    ("helper-signature", lambda r: mutate_json(r, checker.HELPER_AUTHORITY, ("signature_ed25519_hex",), "0" * 128)),
    ("helper-key-id", lambda r: mutate_json(r, checker.HELPER_AUTHORITY, ("acceptance_key_id",), "wrong")),
    ("helper-field-order", lambda r: mutate_json(r, checker.HELPER_AUTHORITY, ("status",), "REORDERED")),
    ("core-authority-include", lambda r: mutate_text(r, checker.CORE, "stage8b-p-r2b-generation2-production-authority.json", "stage8b-p-r2a5-authority.json")),
    ("core-validator-call", lambda r: mutate_text(r, checker.CORE, "fn validate_generation2_composition(", "fn validate_generation2_composition_unchecked(")),
    ("launcher-pin", lambda r: mutate_text(r, checker.LAUNCHER, "stage8b-p-r2b-generation2-accepted-helper-sha256.txt", "stage8b-p-r2b-accepted-helper-sha256.txt")),
    ("issuer-seam", lambda r: mutate_text(r, checker.ISSUER, "create_generation2_helper_acceptance_authority", "create_helper_acceptance_unchecked")),
    ("helper-canonicalization", lambda r: mutate_text(r, checker.ISSUE_SCRIPT, "json.dumps(authority, indent=2)", "json.dumps(authority, indent=2, sort_keys=True)")),
    ("helper-private-path", lambda r: append_text(r, checker.ISSUE_SCRIPT, "\n# /Users/review-fixture/private-ceremony\n")),
    ("build-image", lambda r: mutate_json(r, checker.BUILD, ("container_image",), "mutable:latest")),
    ("build-source", lambda r: mutate_json(r, checker.BUILD, ("source_ref",), "0" * 40)),
    ("build-reproducibility", lambda r: mutate_json(r, checker.BUILD, ("all_hashes_identical",), False)),
    ("build-binary-hash", lambda r: mutate_json(r, checker.BUILD, ("binaries", "stage8b-readonly-preflight", "build_a_sha256"), "0" * 64)),
    ("build-classification", lambda r: mutate_json(r, checker.BUILD, ("binaries", "stage8b-r2b-launcher", "classification"), "OFFLINE_PUBLIC_AUTHORITY_TOOL")),
    ("phase6-base-pin", lambda r: mutate_text(r, checker.MATERIALIZER, checker.BASE_PHASE6_SHA256, "0" * 64)),
    ("phase6-residue-gate", lambda r: mutate_text(r, checker.MATERIALIZER, "generation-1 residue", "generation-one residue")),
    ("phase6-network", lambda r: mutate_text(r, checker.RUNNER, "--network none", "--network bridge")),
    ("rehearsal-generation", lambda r: mutate_json(r, checker.REHEARSAL, ("generation",), 1)),
    ("rehearsal-network", lambda r: mutate_json(r, checker.REHEARSAL, ("container_network_mode",), "bridge")),
    ("rehearsal-finam", lambda r: mutate_json(r, checker.REHEARSAL, ("finam_endpoint_called",), True)),
    ("rehearsal-authorization", lambda r: mutate_json(r, checker.REHEARSAL, ("production_authorization",), "ISSUED")),
    ("authority-predecessor", lambda r: mutate_json(r, checker.AUTHORITY, ("accepted_predecessor", "source_ref"), "0" * 40)),
    ("authority-helper", lambda r: mutate_json(r, checker.AUTHORITY, ("public_composition", "helper_acceptance_sha256"), "0" * 64)),
    ("authority-active", lambda r: mutate_json(r, checker.AUTHORITY, ("activation", "generation_2_active"), True)),
    ("authority-installed", lambda r: mutate_json(r, checker.AUTHORITY, ("activation", "production_credentials_installed"), True)),
    ("authority-issued", lambda r: mutate_json(r, checker.AUTHORITY, ("activation", "package_authorization"), "ISSUED")),
    ("authority-finam", lambda r: mutate_json(r, checker.AUTHORITY, ("closed_surfaces", "finam_network"), True)),
    ("status-drift", lambda r: mutate_text(r, checker.STATUS, checker.STAGE, "Stage 8B-P R2B stale")),
    ("matrix-drift", lambda r: mutate_text(r, checker.MATRIX, "G2CR-R0-037", "G2CR-R0-999")),
)


def main() -> None:
    artifact_root = ROOT / checker.DEFAULT_ARTIFACT_ROOT
    if not artifact_root.is_dir():
        raise SystemExit("stage8b-generation2-composition-negative: FAIL artifact root missing")
    passed = 0
    for name, mutation in CASES:
        with tempfile.TemporaryDirectory(prefix=f"stage8b-g2-composition-{name}-") as temporary:
            root = Path(temporary)
            materialize(root)
            mutation(root)
            try:
                checker.check(root, artifact_root)
            except (RuntimeError, KeyError, IndexError, ValueError, OSError, json.JSONDecodeError, subprocess.SubprocessError):
                passed += 1
                print(f"PASS {name}")
                continue
            raise SystemExit(f"stage8b-generation2-composition-negative: FAIL accepted={name}")
    print(f"stage8b-generation2-composition-negative: PASS cases={passed}/{len(CASES)}")


if __name__ == "__main__":
    main()
