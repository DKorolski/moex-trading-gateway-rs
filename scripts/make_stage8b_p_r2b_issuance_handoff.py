#!/usr/bin/env python3
"""Create the immutable Stage 8B-P R2B issuance-package R0 handoff."""

from __future__ import annotations

import hashlib
import json
import subprocess
import zipfile
from pathlib import Path

import make_stage8b_design_handoff as common
import stage8b_p_r2b_issuance_handoff_safety_check as safety


ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "reports/handoff"
BRANCH = "stage8b-p-r2b-issuance-package"
PREDECESSOR = "f24f1044ac0b29c2f588853b817e519cfe8d3d8b"


def run(*args: str) -> bytes:
    return subprocess.check_output(args, cwd=ROOT)


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def main() -> None:
    if run("git", "status", "--porcelain", "--untracked-files=all").decode().strip():
        raise SystemExit("stage8b-p-r2b-issuance-handoff: FAIL dirty worktree")
    branch = run("git", "branch", "--show-current").decode().strip()
    if branch != BRANCH:
        raise SystemExit(f"stage8b-p-r2b-issuance-handoff: FAIL branch={branch}")
    source_ref = run("git", "rev-parse", "HEAD").decode().strip()
    source_tree = run("git", "rev-parse", "HEAD^{tree}").decode().strip()
    if run("git", "merge-base", source_ref, PREDECESSOR).decode().strip() != PREDECESSOR:
        raise SystemExit("stage8b-p-r2b-issuance-handoff: FAIL predecessor drift")

    gate = subprocess.run(
        ["bash", "scripts/stage8b_p_r2b_issuance_gate.sh"],
        cwd=ROOT,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=False,
    )
    marker = b"stage8b-p-r2b-issuance-gate: PASS revision=R0 rows=25"
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
                "stage": "Stage 8B-P R2B Issuance Package R0",
                "status": "DESIGN_CANDIDATE_NOT_ISSUED",
                "source_ref": source_ref,
                "source_tree": source_tree,
                "source_short_ref": short_ref,
                "archive_name": archive_name,
                "branch": branch,
                "accepted_predecessor": PREDECESSOR,
                "gate_sha256": sha(gate.stdout),
                "manifest_sha256": sha(manifest),
                "acceptance_rows": 25,
                "negative_mutations": 16,
                "transaction_service_invocations": 30,
                "shipped_unit_files": 9,
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
    }

    OUTPUT.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(
        archive_path, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9
    ) as archive:
        for entry in entries:
            archive.writestr(
                common.zip_info(entry["path"], entry["mode"]),
                run("git", "show", f"{source_ref}:{entry['path']}"),
            )
        for name, data in sorted(additions.items()):
            archive.writestr(common.zip_info(name), data)

    result = safety.check(str(archive_path))
    digest = sha(archive_path.read_bytes())
    archive_path.with_suffix(".zip.sha256").write_text(
        f"{digest}  {archive_name}\n", encoding="utf-8"
    )
    archive_path.with_suffix(".zip.safety.json").write_text(
        json.dumps(result, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    print(
        f"archive={archive_path}\nsha256={digest}\nsource_ref={source_ref}\n"
        "stage8b-p-r2b-issuance-handoff: PASS"
    )


if __name__ == "__main__":
    main()
