#!/usr/bin/env python3
"""Validate an immutable public-only Generation-2 backup/restore handoff."""

from __future__ import annotations

import hashlib
import json
import sys
import tempfile
import zipfile
from pathlib import Path, PurePosixPath

import stage8b_p_r2b_generation2_backup_restore_r0_check as stage_check


EVIDENCE = "handoff-evidence/stage8b-p-r2b-generation2-backup-restore-r0-evidence.json"
GATE = "handoff-evidence/stage8b-p-r2b-generation2-backup-restore-r0-gate.txt"
MANIFEST = "handoff-evidence/source-tree-manifest.json"
GENERATED = {"handoff-commit.txt", EVIDENCE, GATE, MANIFEST}
AUTHORITY = "docs/stage-8/stage8b-p-r2b-generation2-backup-restore-r0-authority.json"
RESTORE = "docs/stage-8/stage8b-p-r2b-generation2-backup-restore-r0-receipt.json"
DESTRUCTION = "docs/stage-8/stage8b-p-r2b-generation2-restore-destruction-r0-receipt.json"
TRUST = "docs/stage-8/stage8b-p-r2b-trust-rebind-generation-2-trust-manifest.json"
ACCOUNT = "docs/stage-8/stage8b-p-r2b-trust-rebind-generation-2-account-key-manifest.json"
OPERATION_SOURCE_REF = "b86cc6be0ff9c7748162d00137ef85ae4f97f168"
OPERATION_SOURCE_TREE = "8ce2be049776c04036e42cedb90629d3688e3485"
ACCEPTED_TRUST_REBIND_REF = "d8c71154d7407358b638af9e0c690578050d1640"
REQUIRED = GENERATED | {
    AUTHORITY,
    RESTORE,
    DESTRUCTION,
    TRUST,
    ACCOUNT,
    "docs/current-status.md",
    "docs/stage-8/STAGE8B_P_R2B_GENERATION2_BACKUP_RESTORE_R0_2026-08-31.md",
    "docs/stage-8/STAGE8B_P_R2B_GENERATION2_BACKUP_RESTORE_R0_ACCEPTANCE_MATRIX_2026-08-31.csv",
    "scripts/stage8b_p_r2b_generation2_backup_identity.py",
    "scripts/stage8b_p_r2b_generation2_backup_restore_r0_operate.py",
    "scripts/stage8b_p_r2b_generation2_backup_restore_r0_check.py",
    "scripts/stage8b_p_r2b_generation2_backup_restore_r0_negative_harness.py",
    "scripts/stage8b_p_r2b_generation2_backup_restore_r0_gate.sh",
    "scripts/stage8b_p_r2b_generation2_backup_restore_r0_handoff_safety_check.py",
    "scripts/stage8b_p_r2b_generation2_backup_restore_r0_handoff_negative_harness.py",
    "scripts/make_stage8b_p_r2b_generation2_backup_restore_r0_handoff.py",
    "tools/stage8b-readonly-preflight/Cargo.lock",
    "tools/stage8b-readonly-preflight/src/r2a5.rs",
    "tools/stage8b-readonly-preflight/src/bin/stage8b-r2b-generation2-backup-restore-attest.rs",
    "tools/stage8b-readonly-preflight/src/bin/stage8b-r2b-generation2-restore-destruction-attest.rs",
}
SECRET_MEMBER_NAMES = {
    "package-authorization.ed25519",
    "helper-acceptance.ed25519",
    "account-binding-generation-2.hex",
    "key.ed25519",
}
CLOSED_SURFACES = {
    "generation_2_active": False,
    "generation_2_public_authority_selected": False,
    "production_binaries_rebuilt": False,
    "helper_acceptance_reissued": False,
    "phase6_rehearsal_rebound": False,
    "production_credentials_installed": False,
    "controlled_installation": False,
    "finam_network": False,
    "auth_service": False,
    "broker_get": False,
    "http_post_delete": False,
    "broker_dispatch": False,
    "redis_live": False,
    "runtime_live": False,
    "real_orders": False,
}


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def mode(item: zipfile.ZipInfo) -> str:
    return f"{(item.external_attr >> 16) & 0o177777:06o}"


def private_markers() -> tuple[bytes, ...]:
    # Split literals keep the safety checker itself free of exported custody paths.
    return (
        b"AGE-SECRET-" + b"KEY-",
        b"/Users/" + b"denisq/.config/moex-trading/stage8b/"
        + b"r2b-trust-rebind-generation-" + b"2-20260830",
        b"/Users/" + b"denisq/Documents/moex-trading-ceremony-" + b"secret",
        b"/Volumes/" + b"TRAN" + b"SCEND",
    )


def check(path: str) -> dict[str, object]:
    archive_path = Path(path)
    with zipfile.ZipFile(archive_path) as archive:
        infos = archive.infolist()
        names = [item.filename for item in infos]
        members = {item.filename: item for item in infos}
        if len(names) != len(set(names)):
            raise ValueError("duplicate members")
        if missing := REQUIRED - set(names):
            raise ValueError(f"missing members: {sorted(missing)}")
        markers = private_markers()
        identity_literal_guard = "scripts/stage8b_p_r2b_generation2_backup_restore_r0_check.py"
        for item in infos:
            member = PurePosixPath(item.filename)
            item_mode = (item.external_attr >> 16) & 0o177777
            if member.is_absolute() or ".." in member.parts or "" in member.parts:
                raise ValueError(f"unsafe path: {item.filename}")
            if item_mode & 0o170000 == 0o120000:
                raise ValueError(f"symlink: {item.filename}")
            if item_mode & 0o170000 not in {0, 0o100000, 0o040000}:
                raise ValueError(f"special file: {item.filename}")
            if member.parts[0] in {".git", "target", "tmp", "reports", "__MACOSX"}:
                raise ValueError(f"forbidden root: {item.filename}")
            if member.name in SECRET_MEMBER_NAMES:
                raise ValueError(f"private ceremony member: {item.filename}")
            lower_name = member.name.lower()
            if lower_name.endswith((".age", ".agekey", ".pem", ".key", ".sqlite", ".sqlite3", ".log")):
                raise ValueError(f"secret/runtime artifact: {item.filename}")
            if ".env" in member.parts:
                raise ValueError(f"environment artifact: {item.filename}")
            if not item.is_dir():
                data = archive.read(item.filename)
                for index, marker in enumerate(markers):
                    if marker not in data:
                        continue
                    if index == 0 and item.filename == identity_literal_guard:
                        # The operation-bound public checker rejects this exact
                        # private-key prefix and is hash-pinned by both receipts.
                        continue
                    raise ValueError(f"private custody value exported: {item.filename}")

        marker = dict(
            line.split("=", 1)
            for line in archive.read("handoff-commit.txt").decode().splitlines()
            if "=" in line
        )
        evidence = json.loads(archive.read(EVIDENCE))
        authority = json.loads(archive.read(AUTHORITY))
        restore_bytes = archive.read(RESTORE)
        restore = json.loads(restore_bytes)
        destruction_bytes = archive.read(DESTRUCTION)
        destruction = json.loads(destruction_bytes)
        manifest_bytes = archive.read(MANIFEST)
        manifest = json.loads(manifest_bytes)
        source_ref = marker.get("source_ref")
        source_tree = marker.get("source_tree")
        if not source_ref or evidence.get("source_ref") != source_ref or manifest.get("source_ref") != source_ref:
            raise ValueError("source binding mismatch")
        if not source_tree or evidence.get("source_tree") != source_tree or marker.get("source_tree") != source_tree:
            raise ValueError("source-tree binding mismatch")
        if marker.get("archive_name") != archive_path.name or evidence.get("archive_name") != archive_path.name:
            raise ValueError("archive-name binding mismatch")
        if marker.get("operation_source_ref") != OPERATION_SOURCE_REF or evidence.get("operation_source_ref") != OPERATION_SOURCE_REF:
            raise ValueError("operation source-ref drift")
        if marker.get("operation_source_tree") != OPERATION_SOURCE_TREE or evidence.get("operation_source_tree") != OPERATION_SOURCE_TREE:
            raise ValueError("operation source-tree drift")
        if evidence.get("accepted_trust_rebind_ref") != ACCEPTED_TRUST_REBIND_REF:
            raise ValueError("accepted Trust Rebind lineage drift")
        if authority.get("source_ref") != OPERATION_SOURCE_REF or authority.get("source_tree") != OPERATION_SOURCE_TREE:
            raise ValueError("authority operation binding drift")
        if restore.get("source_ref") != OPERATION_SOURCE_REF or destruction.get("source_ref") != OPERATION_SOURCE_REF:
            raise ValueError("receipt operation binding drift")
        if evidence.get("restore_receipt_sha256") != sha256(restore_bytes):
            raise ValueError("restore receipt digest mismatch")
        if evidence.get("destruction_receipt_sha256") != sha256(destruction_bytes):
            raise ValueError("destruction receipt digest mismatch")
        backup = authority.get("backup", {})
        if evidence.get("encrypted_backup") != {
            "file_name": backup.get("encrypted_backup_file_name"),
            "sha256": backup.get("encrypted_backup_sha256"),
            "size_bytes": backup.get("encrypted_backup_size_bytes"),
            "status": "VERIFIED",
            "included_in_handoff": False,
        }:
            raise ValueError("encrypted-backup evidence drift")
        if evidence.get("encryption_recipient_sha256") != restore.get("encryption_recipient_sha256"):
            raise ValueError("recovery recipient binding drift")
        if evidence.get("verified_bindings") != {"signing_seeds": 13, "account_keys": 1}:
            raise ValueError("binding inventory drift")
        if evidence.get("receipts") != {
            "restore_signature_verified": True,
            "destruction_signature_verified": True,
            "disposable_restore_deleted": True,
            "logical_deletion_only": True,
        }:
            raise ValueError("receipt evidence drift")
        if evidence.get("private_material") != {
            "ceremony_in_handoff": False,
            "backup_ciphertext_in_handoff": False,
            "recovery_identity_in_handoff": False,
            "private_values_in_handoff": False,
            "custody_paths_in_handoff": False,
            "primary_or_external_media_required_for_review": False,
        }:
            raise ValueError("private-material policy drift")
        if evidence.get("closed_surfaces") != CLOSED_SURFACES:
            raise ValueError("closed-surface evidence drift")
        if evidence.get("authorization") != "NOT_ISSUED" or authority.get("activation", {}).get("package_authorization") != "NOT_ISSUED":
            raise ValueError("authorization opened")
        if evidence.get("review_status") != "INDEPENDENT_REVIEW_REQUIRED" or authority.get("status") != "INDEPENDENT_REVIEW_REQUIRED":
            raise ValueError("review status drift")
        gate = archive.read(GATE)
        if b"stage8b-generation2-backup-restore-r0-gate: PASS" not in gate or sha256(gate) != evidence.get("gate_sha256"):
            raise ValueError("gate evidence mismatch")
        if sha256(manifest_bytes) != evidence.get("manifest_sha256"):
            raise ValueError("manifest digest mismatch")

        tracked: set[str] = set()
        entries = manifest.get("entries", [])
        if manifest.get("entry_count") != len(entries):
            raise ValueError("manifest count mismatch")
        for entry in entries:
            name = entry["path"]
            if name in tracked or name not in members:
                raise ValueError(f"source member mismatch: {name}")
            tracked.add(name)
            data = archive.read(name)
            if len(data) != entry["size"] or sha256(data) != entry["sha256"] or mode(members[name]) != entry["mode"]:
                raise ValueError(f"source content mismatch: {name}")
        if set(names) - tracked != GENERATED:
            raise ValueError("generated member inventory mismatch")

        with tempfile.TemporaryDirectory(prefix="stage8b-g2-backup-public-handoff-") as temporary:
            extracted = Path(temporary)
            archive.extractall(extracted)
            stage_check.check(extracted)

        return {
            "archive_members": len(names),
            "tracked_members_verified": len(tracked),
            "duplicates": 0,
            "symlinks": 0,
            "unsafe_paths": 0,
            "private_ceremony_members": 0,
            "ciphertext_members": 0,
            "recovery_identity_members": 0,
            "private_custody_markers": 0,
            "source_ref": source_ref,
            "operation_source_ref": OPERATION_SOURCE_REF,
            "generation": 2,
            "backup_status": "VERIFIED",
            "restore_deleted": True,
            "authorization": "NOT_ISSUED",
            "result": "PASS",
        }


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit(
            "usage: stage8b_p_r2b_generation2_backup_restore_r0_handoff_safety_check.py ARCHIVE"
        )
    try:
        result = check(sys.argv[1])
    except (KeyError, OSError, ValueError, zipfile.BadZipFile, json.JSONDecodeError) as error:
        raise SystemExit(f"stage8b-generation2-backup-restore-r0-handoff-safety: FAIL {error}") from error
    print(
        "stage8b-generation2-backup-restore-r0-handoff-safety: PASS "
        + json.dumps(result, sort_keys=True)
    )


if __name__ == "__main__":
    main()
