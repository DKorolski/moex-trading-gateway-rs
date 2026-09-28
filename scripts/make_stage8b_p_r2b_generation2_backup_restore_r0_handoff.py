#!/usr/bin/env python3
"""Create a public-only immutable Generation-2 backup/restore review handoff."""

from __future__ import annotations

import hashlib
import json
import subprocess
import sys
import tempfile
import zipfile
from pathlib import Path

import make_stage8b_design_handoff as common
import stage8b_p_r2b_generation2_backup_restore_r0_handoff_safety_check as safety


ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "reports/handoff"
BRANCH = "stage8b-p-r2b-generation2-backup-restore-r0"
OPERATION_SOURCE_REF = "b86cc6be0ff9c7748162d00137ef85ae4f97f168"
OPERATION_SOURCE_TREE = "8ce2be049776c04036e42cedb90629d3688e3485"
ACCEPTED_TRUST_REBIND_REF = "d8c71154d7407358b638af9e0c690578050d1640"
REDACTION_PREDECESSOR_REF = "14efc5ddcb71e524fa4784bd94c92e35b64e1578"


def run(*arguments: str) -> bytes:
    return subprocess.check_output(arguments, cwd=ROOT)


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def execute(command: list[str], cwd: Path) -> str:
    completed = subprocess.run(
        command,
        cwd=cwd,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=False,
    )
    output = completed.stdout.decode(errors="replace")
    if completed.returncode != 0:
        raise SystemExit(output)
    return output


def main() -> None:
    if run("git", "status", "--porcelain", "--untracked-files=all").decode().strip():
        raise SystemExit("stage8b-generation2-backup-restore-r0-handoff: FAIL dirty worktree")
    branch = run("git", "branch", "--show-current").decode().strip()
    if branch != BRANCH:
        raise SystemExit(f"stage8b-generation2-backup-restore-r0-handoff: FAIL branch={branch}")
    source_ref = run("git", "rev-parse", "HEAD").decode().strip()
    source_tree = run("git", "rev-parse", "HEAD^{tree}").decode().strip()
    if run("git", "merge-base", source_ref, OPERATION_SOURCE_REF).decode().strip() != OPERATION_SOURCE_REF:
        raise SystemExit("stage8b-generation2-backup-restore-r0-handoff: FAIL operation lineage drift")
    if run("git", "rev-parse", f"{OPERATION_SOURCE_REF}^{{tree}}").decode().strip() != OPERATION_SOURCE_TREE:
        raise SystemExit("stage8b-generation2-backup-restore-r0-handoff: FAIL operation tree drift")
    if run("git", "merge-base", source_ref, ACCEPTED_TRUST_REBIND_REF).decode().strip() != ACCEPTED_TRUST_REBIND_REF:
        raise SystemExit("stage8b-generation2-backup-restore-r0-handoff: FAIL accepted lineage drift")
    if run("git", "merge-base", source_ref, REDACTION_PREDECESSOR_REF).decode().strip() != REDACTION_PREDECESSOR_REF:
        raise SystemExit("stage8b-generation2-backup-restore-r0-handoff: FAIL redaction predecessor drift")

    gate = subprocess.run(
        ["bash", "scripts/stage8b_p_r2b_generation2_backup_restore_r0_gate.sh"],
        cwd=ROOT,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=False,
    )
    if gate.returncode != 0 or b"stage8b-generation2-backup-restore-r0-gate: PASS" not in gate.stdout:
        raise SystemExit(gate.stdout.decode(errors="replace"))

    authority_bytes = (ROOT / safety.AUTHORITY).read_bytes()
    authority = json.loads(authority_bytes)
    restore_bytes = (ROOT / safety.RESTORE).read_bytes()
    restore = json.loads(restore_bytes)
    destruction_bytes = (ROOT / safety.DESTRUCTION).read_bytes()
    short_ref = source_ref[:7]
    archive_name = f"moex-trading-project-{short_ref}.zip"
    archive_path = OUTPUT / archive_name
    manifest, entries = common.source_manifest(source_ref)
    evidence = (
        json.dumps(
            {
                "schema_version": 1,
                "stage": "Stage 8B-P R2B Generation 2 Backup/Restore R0-R1 Public Handoff Redaction Closure",
                "source_ref": source_ref,
                "source_tree": source_tree,
                "source_short_ref": short_ref,
                "archive_name": archive_name,
                "branch": branch,
                "operation_source_ref": OPERATION_SOURCE_REF,
                "operation_source_tree": OPERATION_SOURCE_TREE,
                "accepted_trust_rebind_ref": ACCEPTED_TRUST_REBIND_REF,
                "redaction_predecessor_ref": REDACTION_PREDECESSOR_REF,
                "gate_sha256": sha256(gate.stdout),
                "manifest_sha256": sha256(manifest),
                "authority_sha256": sha256(authority_bytes),
                "restore_receipt_sha256": sha256(restore_bytes),
                "destruction_receipt_sha256": sha256(destruction_bytes),
                "encrypted_backup": {
                    "file_name": authority["backup"]["encrypted_backup_file_name"],
                    "sha256": authority["backup"]["encrypted_backup_sha256"],
                    "size_bytes": authority["backup"]["encrypted_backup_size_bytes"],
                    "status": "VERIFIED",
                    "included_in_handoff": False,
                },
                "encryption_recipient_sha256": restore["encryption_recipient_sha256"],
                "verified_bindings": {"signing_seeds": 13, "account_keys": 1},
                "receipts": {
                    "restore_signature_verified": True,
                    "destruction_signature_verified": True,
                    "disposable_restore_deleted": True,
                    "logical_deletion_only": True,
                },
                "private_material": {
                    "ceremony_in_handoff": False,
                    "backup_ciphertext_in_handoff": False,
                    "recovery_identity_in_handoff": False,
                    "private_values_in_handoff": False,
                    "custody_paths_in_handoff": False,
                    "primary_or_external_media_required_for_review": False,
                },
                "redaction": safety.REDACTION_EVIDENCE,
                "closed_surfaces": safety.CLOSED_SURFACES,
                "rust_tests": 55,
                "evidence_negative_mutations": 42,
                "handoff_negative_mutations": 32,
                "authorization": "NOT_ISSUED",
                "review_status": "INDEPENDENT_REVIEW_REQUIRED",
            },
            indent=2,
            sort_keys=True,
        )
        + "\n"
    ).encode()
    additions = {
        "handoff-commit.txt": (
            f"source_short_ref={short_ref}\nsource_ref={source_ref}\nsource_tree={source_tree}\n"
            f"operation_source_ref={OPERATION_SOURCE_REF}\n"
            f"operation_source_tree={OPERATION_SOURCE_TREE}\n"
            f"redaction_predecessor_ref={REDACTION_PREDECESSOR_REF}\narchive_name={archive_name}\n"
        ).encode(),
        safety.EVIDENCE: evidence,
        safety.GATE: gate.stdout,
        safety.MANIFEST: manifest,
    }

    OUTPUT.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(archive_path, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
        for entry in entries:
            archive.writestr(
                common.zip_info(entry["path"], entry["mode"]),
                run("git", "show", f"{source_ref}:{entry['path']}"),
            )
        for name, data in sorted(additions.items()):
            archive.writestr(common.zip_info(name, "100644"), data)

    local_redaction = safety.check_local_redaction(str(archive_path))
    result = safety.check(str(archive_path))
    outputs: list[str] = []
    with tempfile.TemporaryDirectory(prefix="stage8b-g2-backup-handoff-") as temporary:
        extracted = Path(temporary)
        with zipfile.ZipFile(archive_path) as archive:
            archive.extractall(extracted)
        for command in (
            [sys.executable, "scripts/stage8b_p_r2b_generation2_backup_restore_r0_check.py", "--root", str(extracted)],
            [sys.executable, "scripts/stage8b_p_r2b_generation2_backup_restore_r0_negative_harness.py"],
            [sys.executable, "scripts/stage8b_p_r2b_generation2_backup_restore_r0_handoff_safety_check.py", str(archive_path)],
            [sys.executable, "scripts/stage8b_p_r2b_generation2_backup_restore_r0_handoff_negative_harness.py", str(archive_path)],
        ):
            outputs.append(execute(command, extracted))

    digest = sha256(archive_path.read_bytes())
    archive_path.with_suffix(".zip.sha256").write_text(f"{digest}  {archive_name}\n", encoding="utf-8")
    archive_path.with_suffix(".zip.safety.json").write_text(
        json.dumps(result, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    archive_path.with_suffix(".zip.post-package-verification.json").write_text(
        json.dumps(
            {
                "schema_version": 1,
                "archive_name": archive_name,
                "archive_sha256": digest,
                "source_ref": source_ref,
                "operation_source_ref": OPERATION_SOURCE_REF,
                "fresh_extraction": True,
                "checker_passed": True,
                "evidence_negative_harness_passed": True,
                "handoff_safety_passed": True,
                "handoff_negative_harness_passed": True,
                "private_material_members": 0,
                "ciphertext_members": 0,
                "recovery_identity_members": 0,
                "local_redaction_scan": local_redaction,
                "external_media_required_for_review": False,
                "generation_2_active": False,
                "authorization": "NOT_ISSUED",
                "output": "".join(outputs),
            },
            indent=2,
            sort_keys=True,
        )
        + "\n",
        encoding="utf-8",
    )
    print(
        f"archive={archive_path}\nsha256={digest}\nsource_ref={source_ref}\n"
        "stage8b-generation2-backup-restore-r0-handoff: PASS"
    )


if __name__ == "__main__":
    main()
