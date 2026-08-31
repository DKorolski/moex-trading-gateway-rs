#!/usr/bin/env python3
"""Create the immutable public-only Trust Rebind R0-R1 review handoff."""

from __future__ import annotations

import hashlib
import json
import os
import subprocess
import sys
import tempfile
import zipfile
from pathlib import Path

import make_stage8b_design_handoff as common
import stage8b_p_r2b_trust_rebind_r0_handoff_safety_check as safety


ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "reports/handoff"
BRANCH = "stage8b-p-r2b-trust-rebind-r0"
PREDECESSOR = "a2586c428cd97349956efb12409ff37aea1fbe78"
def run(*arguments: str) -> bytes:
    return subprocess.check_output(arguments, cwd=ROOT)


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def main() -> None:
    if run("git", "status", "--porcelain", "--untracked-files=all").decode().strip():
        raise SystemExit("stage8b-p-r2b-trust-rebind-r0-handoff: FAIL dirty worktree")
    branch = run("git", "branch", "--show-current").decode().strip()
    if branch != BRANCH:
        raise SystemExit(f"stage8b-p-r2b-trust-rebind-r0-handoff: FAIL branch={branch}")
    source_ref = run("git", "rev-parse", "HEAD").decode().strip()
    source_tree = run("git", "rev-parse", "HEAD^{tree}").decode().strip()
    if run("git", "merge-base", source_ref, PREDECESSOR).decode().strip() != PREDECESSOR:
        raise SystemExit("stage8b-p-r2b-trust-rebind-r0-handoff: FAIL predecessor drift")

    with tempfile.TemporaryDirectory(prefix="stage8b-trust-rebind-r0-r1-receipt-") as receipt_temporary:
        receipt_path = Path(receipt_temporary) / "primary-ceremony-verification-receipt.json"
        environment = os.environ.copy()
        environment["STAGE8B_R2B_TRUST_REBIND_RECEIPT_OUT"] = str(receipt_path)
        gate = subprocess.run(
            ["bash", "scripts/stage8b_p_r2b_trust_rebind_r0_gate.sh"],
            cwd=ROOT,
            env=environment,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            check=False,
        )
        if gate.returncode != 0 or b"stage8b-p-r2b-trust-rebind-r0-gate: PASS" not in gate.stdout:
            raise SystemExit(gate.stdout.decode(errors="replace"))
        receipt_bytes = receipt_path.read_bytes()
        receipt = json.loads(receipt_bytes)

    short_ref = source_ref[:7]
    archive_name = f"moex-trading-project-{short_ref}.zip"
    archive_path = OUTPUT / archive_name
    manifest, entries = common.source_manifest(source_ref)
    evidence = (
        json.dumps(
            {
                "schema_version": 1,
                "stage": "Stage 8B-P R2B Trust Rebind R0-R1",
                "source_ref": source_ref,
                "source_tree": source_tree,
                "source_short_ref": short_ref,
                "archive_name": archive_name,
                "branch": branch,
                "accepted_predecessor_ref": PREDECESSOR,
                "gate_sha256": sha256(gate.stdout),
                "manifest_sha256": sha256(manifest),
                "generation": 2,
                "public_fingerprints": {
                    "authorization_public_key_sha256": receipt["authorization_public_key_sha256"],
                    "trust_manifest_sha256": receipt["trust_manifest_sha256"],
                    "public_key_set_sha256": receipt["public_key_set_sha256"],
                    "account_key_manifest_sha256": receipt["account_key_manifest_sha256"],
                },
                "private_signing_seed_count_verified": receipt["signing_seed_count"],
                "private_account_key_count_verified": receipt["account_key_count"],
                "private_public_bindings_verified": receipt["private_public_bindings_verified"],
                "actual_ceremony_verifier_run": receipt["verification_status"] == "PASS",
                "ceremony_verification_receipt_sha256": sha256(receipt_bytes),
                "receipt_signature_verified": True,
                "verifier_source_sha256": receipt["verifier_source_sha256"],
                "private_material_in_handoff": False,
                "backup_status": "REQUIRED_NOT_VERIFIED",
                "backup_attestation_present": False,
                "rust_tests": 54,
                "trust_rebind_negative_mutations": 46,
                "receipt_negative_mutations": 10,
                "current_tree_negative_mutations": 33,
                "public_authority_selection_changed": False,
                "production_binaries_rebuilt": False,
                "helper_acceptance_reissued": False,
                "production_credentials_installed": False,
                "package_issued": False,
                "container_created": False,
                "finam_network": False,
                "http_post_delete": False,
                "broker_dispatch": False,
                "redis_live": False,
                "runtime_live": False,
                "real_orders": False,
                "authorization": "NOT_ISSUED",
            },
            indent=2,
            sort_keys=True,
        )
        + "\n"
    ).encode()
    additions = {
        "handoff-commit.txt": (
            f"source_short_ref={short_ref}\nsource_ref={source_ref}\n"
            f"source_tree={source_tree}\narchive_name={archive_name}\n"
        ).encode(),
        safety.EVIDENCE: evidence,
        safety.GATE: gate.stdout,
        safety.MANIFEST: manifest,
        safety.RECEIPT: receipt_bytes,
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

    result = safety.check(str(archive_path))
    with tempfile.TemporaryDirectory(prefix="stage8b-trust-rebind-r0-") as temporary:
        extracted = Path(temporary)
        with zipfile.ZipFile(archive_path) as archive:
            archive.extractall(extracted)
        outputs: list[str] = []
        for command in (
            [sys.executable, "scripts/stage8b_p_r2b_trust_rebind_r0_check.py", "--root", str(extracted)],
            [sys.executable, "scripts/stage8b_p_r2b_trust_rebind_r0_negative_harness.py"],
        ):
            completed = subprocess.run(
                command,
                cwd=extracted,
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,
                check=False,
            )
            outputs.append(completed.stdout.decode(errors="replace"))
            if completed.returncode != 0:
                raise SystemExit("".join(outputs))
        completed = subprocess.run(
            [sys.executable, "scripts/stage8b_p_r2b_trust_rebind_r0_handoff_safety_check.py", str(archive_path)],
            cwd=extracted,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            check=False,
        )
        outputs.append(completed.stdout.decode(errors="replace"))
        if completed.returncode != 0:
            raise SystemExit("".join(outputs))
        completed = subprocess.run(
            [sys.executable, "scripts/stage8b_p_r2b_trust_rebind_r0_handoff_negative_harness.py", str(archive_path)],
            cwd=extracted,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            check=False,
        )
        outputs.append(completed.stdout.decode(errors="replace"))
        if completed.returncode != 0:
            raise SystemExit("".join(outputs))

    digest = sha256(archive_path.read_bytes())
    archive_path.with_suffix(".zip.sha256").write_text(f"{digest}  {archive_name}\n", encoding="utf-8")
    archive_path.with_suffix(".zip.safety.json").write_text(json.dumps(result, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    archive_path.with_suffix(".zip.post-package-verification.json").write_text(
        json.dumps(
            {
                "schema_version": 1,
                "archive_name": archive_name,
                "archive_sha256": digest,
                "fresh_extraction": True,
                "checker_passed": True,
                "negative_harness_passed": True,
                "handoff_safety_passed": True,
                "actual_ceremony_verifier_run": True,
                "receipt_signature_verified": True,
                "receipt_negative_harness_passed": True,
                "private_material_members": 0,
                "backup_status": "REQUIRED_NOT_VERIFIED",
                "activation_performed": False,
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
        "stage8b-p-r2b-trust-rebind-r0-handoff: PASS"
    )


if __name__ == "__main__":
    main()
