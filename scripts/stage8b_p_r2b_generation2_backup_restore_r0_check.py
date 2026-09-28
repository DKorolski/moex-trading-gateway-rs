#!/usr/bin/env python3
"""Validate the public-only Generation-2 backup/restore closure."""

from __future__ import annotations

import argparse
import copy
import hashlib
import json
import re
import subprocess
import tempfile
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
BASE = Path("docs/stage-8")
TRUST = BASE / "stage8b-p-r2b-trust-rebind-generation-2-trust-manifest.json"
ACCOUNT = BASE / "stage8b-p-r2b-trust-rebind-generation-2-account-key-manifest.json"
AUTHORITY = BASE / "stage8b-p-r2b-generation2-backup-restore-r0-authority.json"
RESTORE = BASE / "stage8b-p-r2b-generation2-backup-restore-r0-receipt.json"
DESTRUCTION = BASE / "stage8b-p-r2b-generation2-restore-destruction-r0-receipt.json"
DESIGN = BASE / "STAGE8B_P_R2B_GENERATION2_BACKUP_RESTORE_R0_2026-08-31.md"
MATRIX = BASE / "STAGE8B_P_R2B_GENERATION2_BACKUP_RESTORE_R0_ACCEPTANCE_MATRIX_2026-08-31.csv"
OPERATION_SOURCE_REF = "b86cc6be0ff9c7748162d00137ef85ae4f97f168"
OPERATION_SOURCE_TREE = "8ce2be049776c04036e42cedb90629d3688e3485"
SOURCE_DIGEST_DOMAIN = b"stage8b-p-r2b-generation2-backup-restore-source-v1\0"
RESTORE_DOMAIN = "stage8b-p-r2b-generation2-backup-restore-receipt-v1"
DESTRUCTION_DOMAIN = "stage8b-p-r2b-generation2-restore-destruction-receipt-v1"
PACKAGE_DOMAIN = "stage8b-p-r2a5-run-package-ed25519-v1"
SOURCE_FILES = (
    Path("scripts/stage8b_p_r2b_generation2_backup_identity.py"),
    Path("scripts/stage8b_p_r2b_generation2_backup_restore_r0_operate.py"),
    Path("tools/stage8b-readonly-preflight/src/r2a5.rs"),
    Path(
        "tools/stage8b-readonly-preflight/src/bin/"
        "stage8b-r2b-generation2-backup-restore-attest.rs"
    ),
    Path(
        "tools/stage8b-readonly-preflight/src/bin/"
        "stage8b-r2b-generation2-restore-destruction-attest.rs"
    ),
)
RESTORE_KEYS = {
    "schema_version", "stage", "generation", "verification_status", "verified_at_utc",
    "source_ref", "verifier_source_sha256", "verifier_binary_sha256",
    "destruction_attestor_binary_sha256", "cargo_lock_sha256", "rustc_version",
    "cargo_version", "python_version", "age_version", "age_binary_sha256",
    "age_keygen_binary_sha256", "archive_format", "encryption_format",
    "encrypted_backup_file_name", "encrypted_backup_sha256",
    "encrypted_backup_size_bytes", "encryption_recipient_sha256", "media_class",
    "media_filesystem", "external_removable_media_verified",
    "encryption_identity_separate_device_verified", "plaintext_archive_written",
    "extended_acl_absent", "unexpected_file_flags_absent",
    "unexpected_extended_attributes_absent", "trust_manifest_sha256",
    "public_key_set_sha256", "authorization_public_key_sha256",
    "helper_acceptance_public_key_sha256", "account_key_manifest_sha256",
    "source_key_count", "primary_signing_seed_count", "restored_signing_seed_count",
    "primary_account_key_count", "restored_account_key_count",
    "primary_exact_inventory_verified", "restored_exact_inventory_verified",
    "primary_private_public_bindings_verified", "restored_private_public_bindings_verified",
    "primary_account_key_binding_verified", "restored_account_key_binding_verified",
    "public_fingerprints_identical", "private_path_recorded", "private_values_exported",
    "restored_copy_status", "backup_status", "generation_2_active",
    "authorization_status", "signature_domain", "authorization_key_id",
    "authorization_key_generation", "signature_ed25519_hex",
}
DESTRUCTION_KEYS = {
    "schema_version", "stage", "generation", "destruction_status", "destroyed_at_utc",
    "source_ref", "backup_restore_receipt_sha256", "encrypted_backup_sha256",
    "encryption_recipient_sha256", "disposable_restore_absent_verified",
    "logical_deletion_only", "restore_volume_filevault_enabled", "private_path_recorded",
    "private_values_exported", "backup_status", "generation_2_active",
    "authorization_status", "signature_domain", "authorization_key_id",
    "authorization_key_generation", "signature_ed25519_hex",
}


def require(value: bool, message: str) -> None:
    if not value:
        raise ValueError(message)


def sha256(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def lower_hex(value: object, length: int) -> bool:
    return (
        isinstance(value, str)
        and len(value) == length
        and all(character in "0123456789abcdef" for character in value)
    )


def timestamp(value: object) -> bool:
    return isinstance(value, str) and re.fullmatch(
        r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z", value
    ) is not None


def exact_source_digest(root: Path) -> str:
    digest = hashlib.sha256(SOURCE_DIGEST_DOMAIN)
    for relative in SOURCE_FILES:
        data = (root / relative).read_bytes()
        name = relative.as_posix().encode()
        digest.update(len(name).to_bytes(8, "big"))
        digest.update(name)
        digest.update(len(data).to_bytes(8, "big"))
        digest.update(data)
    return digest.hexdigest()


def signature_preimage(receipt: dict[str, Any], domain: str) -> bytes:
    unsigned = copy.deepcopy(receipt)
    unsigned["signature_ed25519_hex"] = ""
    body = json.dumps(unsigned, ensure_ascii=False, separators=(",", ":")).encode()
    return domain.encode() + b"\0" + body


def verify_signature(receipt: dict[str, Any], domain: str, public_key_hex: str) -> None:
    public_der = bytes.fromhex("302a300506032b6570032100" + public_key_hex)
    with tempfile.TemporaryDirectory(prefix="stage8b-g2-backup-signature-") as temporary:
        directory = Path(temporary)
        key = directory / "public.der"
        message = directory / "message"
        signature = directory / "signature"
        key.write_bytes(public_der)
        message.write_bytes(signature_preimage(receipt, domain))
        signature.write_bytes(bytes.fromhex(receipt["signature_ed25519_hex"]))
        completed = subprocess.run(
            [
                "openssl", "pkeyutl", "-verify", "-pubin", "-keyform", "DER",
                "-inkey", str(key), "-rawin", "-in", str(message), "-sigfile", str(signature),
            ],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            check=False,
        )
    require(completed.returncode == 0, "receipt signature drift")


def validate_restore(
    receipt: dict[str, Any], root: Path, trust: dict[str, Any], trust_bytes: bytes,
    account_bytes: bytes,
) -> None:
    require(set(receipt) == RESTORE_KEYS, "restore receipt keyset drift")
    require(receipt["schema_version"] == 1 and type(receipt["schema_version"]) is int, "restore schema drift")
    require(receipt["stage"] == "Stage 8B-P R2B Generation 2 Encrypted Backup Restore R0", "restore stage drift")
    require(receipt["generation"] == 2 and type(receipt["generation"]) is int, "restore generation drift")
    require(receipt["verification_status"] == "PASS", "restore status drift")
    require(timestamp(receipt["verified_at_utc"]), "restore timestamp drift")
    require(receipt["source_ref"] == OPERATION_SOURCE_REF, "operation source ref drift")
    require(receipt["verifier_source_sha256"] == exact_source_digest(root), "verifier source digest drift")
    for field in (
        "verifier_binary_sha256", "destruction_attestor_binary_sha256", "cargo_lock_sha256",
        "age_binary_sha256", "age_keygen_binary_sha256", "encrypted_backup_sha256",
        "encryption_recipient_sha256", "signature_ed25519_hex",
    ):
        require(lower_hex(receipt[field], 128 if field == "signature_ed25519_hex" else 64), f"hex grammar drift: {field}")
    require(receipt["cargo_lock_sha256"] == sha256((root / "tools/stage8b-readonly-preflight/Cargo.lock").read_bytes()), "Cargo.lock digest drift")
    require(receipt["rustc_version"].startswith("rustc 1.95.0 "), "rustc version drift")
    require(receipt["cargo_version"].startswith("cargo 1.95.0 "), "cargo version drift")
    require(receipt["python_version"].startswith("Python 3."), "python version drift")
    require(receipt["age_version"] == "v1.3.2", "age version drift")
    require(receipt["archive_format"] == "POSIX_PAX_STREAM", "archive format drift")
    require(receipt["encryption_format"] == "age-encryption.org/v1/X25519", "encryption format drift")
    require(receipt["encrypted_backup_file_name"] == f"stage8b-p-r2b-generation2-{OPERATION_SOURCE_REF[:7]}.tar.age", "backup filename drift")
    require(type(receipt["encrypted_backup_size_bytes"]) is int and 0 < receipt["encrypted_backup_size_bytes"] <= 128 * 1024 * 1024, "backup size drift")
    require(receipt["media_class"] == "REMOVABLE_EXTERNAL_MEDIA", "media class drift")
    require(receipt["media_filesystem"] == "FAT32", "media filesystem drift")
    for field in (
        "external_removable_media_verified", "encryption_identity_separate_device_verified",
        "extended_acl_absent", "unexpected_file_flags_absent",
        "unexpected_extended_attributes_absent", "primary_exact_inventory_verified",
        "restored_exact_inventory_verified", "primary_account_key_binding_verified",
        "restored_account_key_binding_verified", "public_fingerprints_identical",
    ):
        require(receipt[field] is True, f"positive receipt proof drift: {field}")
    for field in ("plaintext_archive_written", "private_path_recorded", "private_values_exported", "generation_2_active"):
        require(receipt[field] is False, f"negative receipt proof drift: {field}")
    require(receipt["trust_manifest_sha256"] == sha256(trust_bytes), "trust manifest digest drift")
    require(receipt["public_key_set_sha256"] == trust["public_key_set_sha256"], "public key set drift")
    require(receipt["authorization_public_key_sha256"] == trust["authorization_key"]["public_key_sha256"], "authorization key drift")
    require(receipt["helper_acceptance_public_key_sha256"] == trust["helper_acceptance_key"]["public_key_sha256"], "helper key drift")
    require(receipt["account_key_manifest_sha256"] == sha256(account_bytes), "account manifest digest drift")
    for field, expected in (
        ("source_key_count", 11), ("primary_signing_seed_count", 13),
        ("restored_signing_seed_count", 13), ("primary_account_key_count", 1),
        ("restored_account_key_count", 1), ("primary_private_public_bindings_verified", 13),
        ("restored_private_public_bindings_verified", 13),
    ):
        require(receipt[field] == expected and type(receipt[field]) is int, f"binding count drift: {field}")
    require(receipt["restored_copy_status"] == "VERIFIED_PRESENT_PENDING_DELETION", "restored-copy status drift")
    require(receipt["backup_status"] == "RESTORE_VERIFIED_PENDING_DESTRUCTION", "intermediate backup status drift")
    require(receipt["authorization_status"] == "NOT_ISSUED", "authorization opened")
    require(receipt["signature_domain"] == RESTORE_DOMAIN and RESTORE_DOMAIN != PACKAGE_DOMAIN, "restore signature domain drift")
    require(receipt["authorization_key_id"] == trust["authorization_key"]["key_id"], "restore signer drift")
    require(receipt["authorization_key_generation"] == 2, "restore signer generation drift")
    verify_signature(receipt, RESTORE_DOMAIN, trust["authorization_key"]["public_key_ed25519_hex"])


def validate_destruction(
    receipt: dict[str, Any], restore: dict[str, Any], restore_bytes: bytes,
    trust: dict[str, Any],
) -> None:
    require(set(receipt) == DESTRUCTION_KEYS, "destruction receipt keyset drift")
    require(receipt["schema_version"] == 1 and type(receipt["schema_version"]) is int, "destruction schema drift")
    require(receipt["stage"] == "Stage 8B-P R2B Generation 2 Restore Destruction R0", "destruction stage drift")
    require(receipt["generation"] == 2 and type(receipt["generation"]) is int, "destruction generation drift")
    require(receipt["destruction_status"] == "PASS", "destruction status drift")
    require(timestamp(receipt["destroyed_at_utc"]), "destruction timestamp drift")
    require(receipt["source_ref"] == OPERATION_SOURCE_REF, "destruction source drift")
    require(receipt["backup_restore_receipt_sha256"] == sha256(restore_bytes), "restore receipt digest drift")
    require(receipt["encrypted_backup_sha256"] == restore["encrypted_backup_sha256"], "destruction backup digest drift")
    require(receipt["encryption_recipient_sha256"] == restore["encryption_recipient_sha256"], "destruction recipient drift")
    for field in ("disposable_restore_absent_verified", "logical_deletion_only", "restore_volume_filevault_enabled"):
        require(receipt[field] is True, f"destruction proof drift: {field}")
    for field in ("private_path_recorded", "private_values_exported", "generation_2_active"):
        require(receipt[field] is False, f"destruction negative proof drift: {field}")
    require(receipt["backup_status"] == "VERIFIED", "final backup status drift")
    require(receipt["authorization_status"] == "NOT_ISSUED", "destruction authorization opened")
    require(receipt["signature_domain"] == DESTRUCTION_DOMAIN and DESTRUCTION_DOMAIN != PACKAGE_DOMAIN, "destruction signature domain drift")
    require(receipt["authorization_key_id"] == trust["authorization_key"]["key_id"], "destruction signer drift")
    require(receipt["authorization_key_generation"] == 2, "destruction signer generation drift")
    require(lower_hex(receipt["signature_ed25519_hex"], 128), "destruction signature grammar drift")
    verify_signature(receipt, DESTRUCTION_DOMAIN, trust["authorization_key"]["public_key_ed25519_hex"])


def exact_keys(value: dict[str, Any], keys: set[str], label: str) -> None:
    require(set(value) == keys, f"{label} keyset drift")


def validate_authority(
    authority: dict[str, Any], restore: dict[str, Any], restore_bytes: bytes,
    destruction: dict[str, Any], destruction_bytes: bytes,
) -> None:
    exact_keys(authority, {"schema_version", "stage", "status", "source_ref", "source_tree", "lineage", "backup", "restore", "public_fingerprints", "toolchain", "receipts", "activation", "closed_surfaces"}, "authority")
    require(authority["schema_version"] == 1 and type(authority["schema_version"]) is int, "authority schema drift")
    require(authority["stage"] == "Stage 8B-P R2B Generation 2 Encrypted Backup Restore R0", "authority stage drift")
    require(authority["status"] == "INDEPENDENT_REVIEW_REQUIRED", "authority status drift")
    require(authority["source_ref"] == OPERATION_SOURCE_REF, "authority source ref drift")
    require(authority["source_tree"] == OPERATION_SOURCE_TREE, "authority source tree drift")
    require(authority["lineage"] == {
        "accepted_trust_rebind_r0_r1": "d8c71154d7407358b638af9e0c690578050d1640",
        "ceremony_path_hardening": "b5352fb33e69b4113fe2a8e65d3a0ceed55cce57",
        "merged_main_predecessor": "dd1af77efab89cc66f523bbe96821751465e12aa",
    }, "authority lineage drift")
    backup = authority["backup"]
    exact_keys(backup, {"generation", "status", "encrypted_backup_file_name", "encrypted_backup_sha256", "encrypted_backup_size_bytes", "encryption_format", "archive_format", "media_class", "media_filesystem", "encryption_identity_separate_device_verified", "plaintext_archive_written", "backup_ciphertext_in_git_or_handoff", "private_key_in_git_or_handoff", "private_path_recorded"}, "authority backup")
    require(backup == {
        "generation": 2, "status": "VERIFIED",
        "encrypted_backup_file_name": restore["encrypted_backup_file_name"],
        "encrypted_backup_sha256": restore["encrypted_backup_sha256"],
        "encrypted_backup_size_bytes": restore["encrypted_backup_size_bytes"],
        "encryption_format": "age-encryption.org/v1/X25519", "archive_format": "POSIX_PAX_STREAM",
        "media_class": "REMOVABLE_EXTERNAL_MEDIA", "media_filesystem": "FAT32",
        "encryption_identity_separate_device_verified": True, "plaintext_archive_written": False,
        "backup_ciphertext_in_git_or_handoff": False, "private_key_in_git_or_handoff": False,
        "private_path_recorded": False,
    }, "authority backup drift")
    restore_authority = authority["restore"]
    exact_keys(restore_authority, {"verification_status", "public_fingerprints_identical", "signing_seed_bindings", "account_key_bindings", "restore_receipt_sha256", "destruction_receipt_sha256", "disposable_restore_deleted", "logical_deletion_only", "restore_volume_filevault_enabled"}, "authority restore")
    require(restore_authority == {
        "verification_status": "PASS", "public_fingerprints_identical": True,
        "signing_seed_bindings": 13, "account_key_bindings": 1,
        "restore_receipt_sha256": sha256(restore_bytes),
        "destruction_receipt_sha256": sha256(destruction_bytes),
        "disposable_restore_deleted": True, "logical_deletion_only": True,
        "restore_volume_filevault_enabled": True,
    }, "authority restore drift")
    require(authority["public_fingerprints"] == {
        "trust_manifest_sha256": restore["trust_manifest_sha256"],
        "public_key_set_sha256": restore["public_key_set_sha256"],
        "authorization_public_key_sha256": restore["authorization_public_key_sha256"],
        "helper_acceptance_public_key_sha256": restore["helper_acceptance_public_key_sha256"],
        "account_key_manifest_sha256": restore["account_key_manifest_sha256"],
        "encryption_recipient_sha256": restore["encryption_recipient_sha256"],
    }, "authority public fingerprints drift")
    require(authority["toolchain"] == {
        "verifier_source_sha256": restore["verifier_source_sha256"],
        "verifier_binary_sha256": restore["verifier_binary_sha256"],
        "destruction_attestor_binary_sha256": restore["destruction_attestor_binary_sha256"],
        "cargo_lock_sha256": restore["cargo_lock_sha256"], "rustc_version": restore["rustc_version"],
        "cargo_version": restore["cargo_version"], "python_version": restore["python_version"],
        "age_version": restore["age_version"], "age_binary_sha256": restore["age_binary_sha256"],
        "age_keygen_binary_sha256": restore["age_keygen_binary_sha256"], "clean_cargo_target_dir": True,
    }, "authority toolchain drift")
    require(authority["receipts"] == {
        "backup_restore_signature_domain": RESTORE_DOMAIN,
        "destruction_signature_domain": DESTRUCTION_DOMAIN,
        "authorization_key_generation": 2, "package_authorization_domain_reused": False,
    }, "authority receipt policy drift")
    require(authority["activation"] == {
        "generation_2_active": False, "generation_2_public_authority_selected": False,
        "production_binaries_rebuilt": False, "helper_acceptance_reissued": False,
        "phase6_rehearsal_rebound": False, "production_credentials_installed": False,
        "controlled_installation": False, "package_authorization": "NOT_ISSUED",
    }, "authority activation drift")
    require(set(authority["closed_surfaces"]) == {"finam_network", "auth_service", "broker_get", "http_post_delete", "broker_dispatch", "redis_live", "runtime_live", "real_orders"}, "closed surface inventory drift")
    require(all(value is False for value in authority["closed_surfaces"].values()), "closed surface opened")
    require(destruction["backup_status"] == backup["status"], "authority final status mismatch")


def check(root: Path) -> None:
    for relative in (TRUST, ACCOUNT, AUTHORITY, RESTORE, DESTRUCTION, DESIGN, MATRIX, *SOURCE_FILES):
        require((root / relative).is_file(), f"missing artifact: {relative}")
    trust_bytes = (root / TRUST).read_bytes()
    account_bytes = (root / ACCOUNT).read_bytes()
    restore_bytes = (root / RESTORE).read_bytes()
    destruction_bytes = (root / DESTRUCTION).read_bytes()
    trust = json.loads(trust_bytes)
    restore = json.loads(restore_bytes)
    destruction = json.loads(destruction_bytes)
    authority = json.loads((root / AUTHORITY).read_bytes())
    require(all(isinstance(value, dict) for value in (trust, restore, destruction, authority)), "JSON root drift")
    validate_restore(restore, root, trust, trust_bytes, account_bytes)
    validate_destruction(destruction, restore, restore_bytes, trust)
    validate_authority(authority, restore, restore_bytes, destruction, destruction_bytes)
    for relative in (AUTHORITY, RESTORE, DESTRUCTION):
        text = (root / relative).read_text(encoding="utf-8")
        require("/Users/" not in text and "/Volumes/" not in text, "local custody path exported")
        require("AGE-SECRET-KEY-" not in text, "age private identity exported")
    forbidden_members = [
        path for path in root.rglob("*")
        if path.is_file() and path.suffix in {".age", ".agekey"}
    ]
    require(not forbidden_members, "ciphertext or recovery identity entered source tree")
    matrix_lines = (root / MATRIX).read_text(encoding="utf-8").splitlines()
    require(len(matrix_lines) == 27 and all(line.endswith(",PASS") for line in matrix_lines[1:]), "acceptance matrix drift")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=ROOT)
    arguments = parser.parse_args()
    try:
        check(arguments.root.resolve())
    except (KeyError, OSError, ValueError, json.JSONDecodeError) as error:
        raise SystemExit(f"stage8b-generation2-backup-restore-r0-check: FAIL {error}") from error
    print(
        "stage8b-generation2-backup-restore-r0-check: PASS "
        "generation=2 backup=VERIFIED bindings=13+1 restore_deleted=true "
        "private_material=false active=false authorization=NOT_ISSUED finam=false"
    )


if __name__ == "__main__":
    main()
