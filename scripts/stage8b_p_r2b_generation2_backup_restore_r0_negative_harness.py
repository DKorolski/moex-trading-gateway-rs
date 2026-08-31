#!/usr/bin/env python3
"""Negative mutation matrix for Generation-2 backup/restore evidence."""

from __future__ import annotations

import copy
import json
import shutil
import tempfile
from pathlib import Path
from typing import Callable

import stage8b_p_r2b_generation2_backup_restore_r0_check as checker


ROOT = Path(__file__).resolve().parents[1]


def materialize(destination: Path) -> None:
    required = {
        checker.TRUST,
        checker.ACCOUNT,
        checker.AUTHORITY,
        checker.RESTORE,
        checker.DESTRUCTION,
        checker.DESIGN,
        checker.MATRIX,
        Path("tools/stage8b-readonly-preflight/Cargo.lock"),
        *checker.SOURCE_FILES,
    }
    for relative in required:
        target = destination / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(ROOT / relative, target)


def mutate_json(root: Path, relative: Path, path: tuple[str, ...], value: object) -> None:
    document = json.loads((root / relative).read_text(encoding="utf-8"))
    cursor = document
    for key in path[:-1]:
        cursor = cursor[key]
    cursor[path[-1]] = value
    (root / relative).write_text(json.dumps(document, indent=2) + "\n", encoding="utf-8")


def remove_json(root: Path, relative: Path, path: tuple[str, ...]) -> None:
    document = json.loads((root / relative).read_text(encoding="utf-8"))
    cursor = document
    for key in path[:-1]:
        cursor = cursor[key]
    del cursor[path[-1]]
    (root / relative).write_text(json.dumps(document, indent=2) + "\n", encoding="utf-8")


def add_json(root: Path, relative: Path, path: tuple[str, ...], key: str, value: object) -> None:
    document = json.loads((root / relative).read_text(encoding="utf-8"))
    cursor = document
    for component in path:
        cursor = cursor[component]
    cursor[key] = value
    (root / relative).write_text(json.dumps(document, indent=2) + "\n", encoding="utf-8")


def flip_signature(root: Path, relative: Path) -> None:
    document = json.loads((root / relative).read_text(encoding="utf-8"))
    signature = document["signature_ed25519_hex"]
    document["signature_ed25519_hex"] = ("0" if signature[0] != "0" else "1") + signature[1:]
    (root / relative).write_text(json.dumps(document, indent=2) + "\n", encoding="utf-8")


def mutate_text(root: Path, relative: Path) -> None:
    path = root / relative
    path.write_bytes(path.read_bytes() + b"\n")


def add_ciphertext(root: Path) -> None:
    (root / "unexpected.age").write_bytes(b"not-a-real-backup")


Mutation = Callable[[Path], None]
CASES: tuple[tuple[str, Mutation], ...] = (
    ("restore-signature", lambda root: flip_signature(root, checker.RESTORE)),
    ("restore-source-ref", lambda root: mutate_json(root, checker.RESTORE, ("source_ref",), "0" * 40)),
    ("restore-source-digest", lambda root: mutate_json(root, checker.RESTORE, ("verifier_source_sha256",), "0" * 64)),
    ("restore-verifier-binary", lambda root: mutate_json(root, checker.RESTORE, ("verifier_binary_sha256",), "0" * 64)),
    ("restore-destructor-binary", lambda root: mutate_json(root, checker.RESTORE, ("destruction_attestor_binary_sha256",), "0" * 64)),
    ("restore-cargo-lock", lambda root: mutate_json(root, checker.RESTORE, ("cargo_lock_sha256",), "0" * 64)),
    ("restore-media-class", lambda root: mutate_json(root, checker.RESTORE, ("media_class",), "LOCAL_DISK")),
    ("restore-device-separation", lambda root: mutate_json(root, checker.RESTORE, ("encryption_identity_separate_device_verified",), False)),
    ("restore-plaintext-archive", lambda root: mutate_json(root, checker.RESTORE, ("plaintext_archive_written",), True)),
    ("restore-acl-proof", lambda root: mutate_json(root, checker.RESTORE, ("extended_acl_absent",), False)),
    ("restore-xattr-proof", lambda root: mutate_json(root, checker.RESTORE, ("unexpected_extended_attributes_absent",), False)),
    ("restore-trust-hash", lambda root: mutate_json(root, checker.RESTORE, ("trust_manifest_sha256",), "0" * 64)),
    ("restore-seed-count", lambda root: mutate_json(root, checker.RESTORE, ("restored_signing_seed_count",), 12)),
    ("restore-account-count", lambda root: mutate_json(root, checker.RESTORE, ("restored_account_key_count",), 0)),
    ("restore-fingerprint-equality", lambda root: mutate_json(root, checker.RESTORE, ("public_fingerprints_identical",), False)),
    ("restore-private-path", lambda root: mutate_json(root, checker.RESTORE, ("private_path_recorded",), True)),
    ("restore-backup-status", lambda root: mutate_json(root, checker.RESTORE, ("backup_status",), "VERIFIED")),
    ("restore-generation-active", lambda root: mutate_json(root, checker.RESTORE, ("generation_2_active",), True)),
    ("restore-authorization", lambda root: mutate_json(root, checker.RESTORE, ("authorization_status",), "ISSUED")),
    ("restore-package-domain", lambda root: mutate_json(root, checker.RESTORE, ("signature_domain",), checker.PACKAGE_DOMAIN)),
    ("restore-unknown-field", lambda root: add_json(root, checker.RESTORE, (), "private_backup_path", "/redacted")),
    ("restore-missing-field", lambda root: remove_json(root, checker.RESTORE, ("media_filesystem",))),
    ("destruction-signature", lambda root: flip_signature(root, checker.DESTRUCTION)),
    ("destruction-receipt-hash", lambda root: mutate_json(root, checker.DESTRUCTION, ("backup_restore_receipt_sha256",), "0" * 64)),
    ("destruction-backup-hash", lambda root: mutate_json(root, checker.DESTRUCTION, ("encrypted_backup_sha256",), "0" * 64)),
    ("destruction-absence", lambda root: mutate_json(root, checker.DESTRUCTION, ("disposable_restore_absent_verified",), False)),
    ("destruction-logical-only", lambda root: mutate_json(root, checker.DESTRUCTION, ("logical_deletion_only",), False)),
    ("destruction-filevault", lambda root: mutate_json(root, checker.DESTRUCTION, ("restore_volume_filevault_enabled",), False)),
    ("destruction-private-path", lambda root: mutate_json(root, checker.DESTRUCTION, ("private_path_recorded",), True)),
    ("destruction-final-status", lambda root: mutate_json(root, checker.DESTRUCTION, ("backup_status",), "PENDING")),
    ("authority-source-ref", lambda root: mutate_json(root, checker.AUTHORITY, ("source_ref",), "0" * 40)),
    ("authority-backup-status", lambda root: mutate_json(root, checker.AUTHORITY, ("backup", "status"), "REQUIRED_NOT_VERIFIED")),
    ("authority-ciphertext-in-git", lambda root: mutate_json(root, checker.AUTHORITY, ("backup", "backup_ciphertext_in_git_or_handoff"), True)),
    ("authority-receipt-hash", lambda root: mutate_json(root, checker.AUTHORITY, ("restore", "restore_receipt_sha256"), "0" * 64)),
    ("authority-activation", lambda root: mutate_json(root, checker.AUTHORITY, ("activation", "generation_2_active"), True)),
    ("authority-authorization", lambda root: mutate_json(root, checker.AUTHORITY, ("activation", "package_authorization"), "ISSUED")),
    ("authority-finam", lambda root: mutate_json(root, checker.AUTHORITY, ("closed_surfaces", "finam_network"), True)),
    ("authority-unknown-field", lambda root: add_json(root, checker.AUTHORITY, (), "controlled_installation_allowed", True)),
    ("source-drift", lambda root: mutate_text(root, checker.SOURCE_FILES[1])),
    ("cargo-lock-drift", lambda root: mutate_text(root, Path("tools/stage8b-readonly-preflight/Cargo.lock"))),
    ("matrix-drift", lambda root: mutate_text(root, checker.MATRIX)),
    ("ciphertext-in-source", add_ciphertext),
)


def main() -> None:
    passed = 0
    for name, mutation in CASES:
        with tempfile.TemporaryDirectory(prefix="stage8b-g2-backup-negative-") as temporary:
            root = Path(temporary)
            materialize(root)
            mutation(root)
            try:
                checker.check(root)
            except (KeyError, OSError, ValueError, json.JSONDecodeError):
                passed += 1
                print(f"PASS {name}")
                continue
            raise SystemExit(f"stage8b-generation2-backup-negative: FAIL accepted={name}")
    print(f"stage8b-generation2-backup-negative: PASS cases={passed}/{len(CASES)}")


if __name__ == "__main__":
    main()
