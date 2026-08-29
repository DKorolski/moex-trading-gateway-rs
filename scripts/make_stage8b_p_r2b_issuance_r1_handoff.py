#!/usr/bin/env python3
"""Create the immutable Stage 8B-P R2B issuance R0-R1 handoff."""

from __future__ import annotations

import hashlib
import json
import subprocess
import zipfile
from pathlib import Path

import make_stage8b_design_handoff as common
import stage8b_p_r2b_issuance_r1_handoff_safety_check as safety


ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "reports/handoff"
BRANCH = "stage8b-p-r2b-issuance-package"
R0 = "928168ed47e5b9dd873cd73815fbccecde7a8981"
ACCEPTED_PREDECESSOR = "f24f1044ac0b29c2f588853b817e519cfe8d3d8b"
SNAPSHOT_SHA = "7c8e6bcd02f907af93ea1386499d03bff194da76a1eb2b19dd9c2ff1f97403c5"


def run(*args: str) -> bytes:
    return subprocess.check_output(args, cwd=ROOT)


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def main() -> None:
    if run("git", "status", "--porcelain", "--untracked-files=all").decode().strip():
        raise SystemExit("stage8b-p-r2b-issuance-r1-handoff: FAIL dirty worktree")
    branch = run("git", "branch", "--show-current").decode().strip()
    if branch != BRANCH:
        raise SystemExit(f"stage8b-p-r2b-issuance-r1-handoff: FAIL branch={branch}")
    source_ref = run("git", "rev-parse", "HEAD").decode().strip()
    source_tree = run("git", "rev-parse", "HEAD^{tree}").decode().strip()
    if run("git", "merge-base", source_ref, R0).decode().strip() != R0:
        raise SystemExit("stage8b-p-r2b-issuance-r1-handoff: FAIL R0 lineage drift")

    gate = subprocess.run(
        ["bash", "scripts/stage8b_p_r2b_issuance_r1_gate.sh"], cwd=ROOT,
        stdout=subprocess.PIPE, stderr=subprocess.STDOUT, check=False,
    )
    marker = b"stage8b-p-r2b-issuance-r1-gate: PASS revision=R0-R1A rows=54"
    if gate.returncode != 0 or marker not in gate.stdout:
        raise SystemExit(gate.stdout.decode(errors="replace"))

    short_ref = source_ref[:7]
    archive_name = f"moex-trading-project-{short_ref}.zip"
    archive_path = OUTPUT / archive_name
    manifest, entries = common.source_manifest(source_ref)
    evidence = (
        json.dumps(
            {
                "schema_version": 1,
                "stage": "Stage 8B-P R2B Issuance Package R0-R1A",
                "status": "EXACT_GOVERNANCE_FREEZE_CANDIDATE_NOT_ISSUED",
                "source_ref": source_ref,
                "source_tree": source_tree,
                "source_short_ref": short_ref,
                "archive_name": archive_name,
                "branch": branch,
                "accepted_predecessor": ACCEPTED_PREDECESSOR,
                "corrected_r0": R0,
                "gate_sha256": sha(gate.stdout),
                "manifest_sha256": sha(manifest),
                "read_contract_snapshot_sha256": SNAPSHOT_SHA,
                "read_document_count": 6,
                "acceptance_rows": 54,
                "negative_mutations": 54,
                "r1_negative_mutations": 25,
                "r1a_exact_negative_mutations": 29,
                "exact_governance_freeze": True,
                "fixed_input_count": 7,
                "receipt_source_count": 11,
                "service_invocations": 31,
                "phase_count": 6,
                "draft_builder_model": "SEPARATE_DRAFT_BUILDER_THEN_SIGNER",
                "draft_builder_implemented": False,
                "activation_target_implemented": False,
                "operator_selection": "ABSENT",
                "authorization_status": "NOT_ISSUED",
                "finam_credentials_accessed": False,
                "auth_service_called": False,
                "broker_account_get_sent": False,
                "order_post_sent": False,
                "order_delete_sent": False,
                "dispatch_attempt_recorded": False,
                "transport_entered": False,
                "redis_live_consumer": False,
                "broker_dispatch": False,
                "runtime_live": False,
                "strategy_live": False,
                "real_orders": False,
            }, indent=2, sort_keys=True,
        ) + "\n"
    ).encode()
    additions = {
        "handoff-commit.txt": (
            f"source_short_ref={short_ref}\nsource_ref={source_ref}\n"
            f"source_tree={source_tree}\narchive_name={archive_name}\n"
        ).encode(),
        safety.EVIDENCE: evidence,
        safety.GATE: gate.stdout,
        safety.MANIFEST: manifest,
    }

    OUTPUT.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(archive_path, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
        for entry in entries:
            archive.writestr(common.zip_info(entry["path"], entry["mode"]), run("git", "show", f"{source_ref}:{entry['path']}"))
        for name, data in sorted(additions.items()):
            archive.writestr(common.zip_info(name), data)

    result = safety.check(str(archive_path))
    digest = sha(archive_path.read_bytes())
    archive_path.with_suffix(".zip.sha256").write_text(f"{digest}  {archive_name}\n", encoding="utf-8")
    archive_path.with_suffix(".zip.safety.json").write_text(json.dumps(result, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(
        f"archive={archive_path}\nsha256={digest}\nsource_ref={source_ref}\n"
        "stage8b-p-r2b-issuance-r1-handoff: PASS"
    )


if __name__ == "__main__":
    main()
