#!/usr/bin/env python3
"""Create the immutable Stage 8B-P1-f Id source handoff."""

from __future__ import annotations

import hashlib
import json
import subprocess
import zipfile
from pathlib import Path

import make_stage8b_design_handoff as common
import stage8b_p1f_id_check as source_check
import stage8b_p1f_id_handoff_safety_check as safety


ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "reports/handoff"
REVIEW_SOURCE = Path(
    "/Users/denisq/Downloads/FINAM_P1F_ID_R1_REVIEW_ea2897a_2026-09-26.md"
)


def git(*args: str) -> bytes:
    return subprocess.check_output(("git", *args), cwd=ROOT)


def sha256(raw: bytes) -> str:
    return hashlib.sha256(raw).hexdigest()


def run_capture(command: list[str]) -> bytes:
    process = subprocess.run(
        command,
        cwd=ROOT,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=False,
    )
    if process.returncode != 0:
        raise SystemExit(process.stdout.decode(errors="replace"))
    return process.stdout


def main() -> None:
    status = git("status", "--porcelain", "--untracked-files=all").decode().strip()
    if status:
        raise SystemExit(f"stage8b-p1f-id-handoff: FAIL dirty worktree\n{status}")
    branch = git("branch", "--show-current").decode().strip()
    if branch != safety.BRANCH:
        raise SystemExit(f"stage8b-p1f-id-handoff: FAIL branch={branch}")
    source_ref = git("rev-parse", "HEAD").decode().strip()
    source_parent = git("rev-parse", "HEAD^").decode().strip()
    source_tree = git("rev-parse", "HEAD^{tree}").decode().strip()
    if source_parent != source_check.REVIEWED_ID_CANDIDATE:
        raise SystemExit("stage8b-p1f-id-handoff: FAIL source parent drift")
    changed = set(
        git("diff", "--name-only", source_check.BASE, source_ref, "--")
        .decode()
        .splitlines()
    )
    if changed != source_check.ALLOWED_CHANGES:
        raise SystemExit(
            "stage8b-p1f-id-handoff: FAIL changed paths "
            f"{sorted(changed ^ source_check.ALLOWED_CHANGES)}"
        )
    review = REVIEW_SOURCE.read_bytes()
    if sha256(review) != source_check.CORRECTION_REVIEW_SHA256:
        raise SystemExit("stage8b-p1f-id-handoff: FAIL correction review digest")

    gate = run_capture(["bash", "scripts/stage8b_p1f_id_gate.sh"])
    if (
        git("rev-parse", "HEAD").decode().strip() != source_ref
        or git("status", "--porcelain", "--untracked-files=all").decode().strip()
    ):
        raise SystemExit("stage8b-p1f-id-handoff: FAIL source changed during evidence run")

    short = source_ref[:7]
    archive_name = f"moex-trading-project-{short}-stage8b-p1f-id-r2-review-package.zip"
    archive_path = OUTPUT / archive_name
    manifest, entries = common.source_manifest(source_ref)
    closed_surfaces = {
        "p1f_o0_operational_activation": False,
        "operational_install_or_service_start": False,
        "vps_mutation": False,
        "operational_redis_db15": False,
        "paper_provider_execution": False,
        "finam_post_or_delete": False,
        "broker_dispatch": False,
        "runtime_live": False,
        "real_orders": False,
    }
    evidence = {
        "schema_version": 1,
        "stage": safety.STAGE,
        "status": "FIXED_REDIS_COMPOSITION_CORRECTION_R2_REVIEW_CANDIDATE_NO_ACTIVATION",
        "source_ref": source_ref,
        "source_parent": source_parent,
        "source_tree": source_tree,
        "branch": branch,
        "archive_name": archive_name,
        "accepted_ic_ref": source_check.BASE,
        "accepted_ic_review_sha256": source_check.REVIEW_SHA256,
        "reviewed_id_candidate_ref": source_check.REVIEWED_ID_CANDIDATE,
        "correction_review_sha256": source_check.CORRECTION_REVIEW_SHA256,
        "changed_paths": sorted(changed),
        "manifest_sha256": sha256(manifest),
        "gate_sha256": sha256(gate),
        "source_negative_cases": 34,
        "acceptance_matrix_rows": 30,
        "targeted_rust_tests": 21,
        "fixed_redis_roles": 8,
        "source_operations": 10,
        "pinned_lua_scripts": 8,
        "closed_surfaces": closed_surfaces,
        "next_after_acceptance": "P1F-Ie aggregate source closure; P1F-O0 remains closed",
    }
    marker = (
        f"stage={safety.STAGE}\n"
        f"source_short_ref={short}\n"
        f"source_ref={source_ref}\n"
        f"source_parent={source_parent}\n"
        f"source_tree={source_tree}\n"
        f"branch={branch}\n"
        f"accepted_ic_ref={source_check.BASE}\n"
        f"reviewed_id_candidate_ref={source_check.REVIEWED_ID_CANDIDATE}\n"
        f"archive_name={archive_name}\n"
    ).encode()
    additions = {
        safety.MARKER: marker,
        safety.MANIFEST: manifest,
        safety.COMMIT_RAW: git("cat-file", "commit", source_ref),
        safety.EVIDENCE: (json.dumps(evidence, indent=2, sort_keys=True) + "\n").encode(),
        safety.GATE: gate,
        safety.REVIEW: review,
    }

    OUTPUT.mkdir(parents=True, exist_ok=True)
    archive_path.unlink(missing_ok=True)
    with zipfile.ZipFile(
        archive_path, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9
    ) as archive:
        for entry in entries:
            archive.writestr(
                common.zip_info(entry["path"], entry["mode"]),
                git("show", f"{source_ref}:{entry['path']}"),
            )
        for name, raw in sorted(additions.items()):
            archive.writestr(common.zip_info(name), raw)

    result = safety.check(str(archive_path))
    digest = sha256(archive_path.read_bytes())
    archive_path.with_suffix(".zip.sha256").write_text(f"{digest}  {archive_name}\n")
    archive_path.with_suffix(".zip.safety.json").write_text(
        json.dumps(result, indent=2, sort_keys=True) + "\n"
    )
    print(
        f"archive={archive_path}\nsha256={digest}\nsource_ref={source_ref}\n"
        "PASS stage8b-p1f-id-handoff"
    )


if __name__ == "__main__":
    main()
