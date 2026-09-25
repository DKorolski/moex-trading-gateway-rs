#!/usr/bin/env python3
"""Create immutable Stage 8B-P1-f I source review handoff."""

from __future__ import annotations

import hashlib
import json
import subprocess
import zipfile
from pathlib import Path

import make_stage8b_design_handoff as common
import stage8b_p1f_source_check as source_check
import stage8b_p1f_source_handoff_safety_check as safety


ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "reports/handoff"
REVIEW_SOURCE = Path("/Users/denisq/Downloads/FINAM_P1F_R4_DESIGN_REVIEW_5d81b8e_2026-09-25.md")


def git(*args: str) -> bytes:
    return subprocess.check_output(("git", *args), cwd=ROOT)


def sha256(raw: bytes) -> str:
    return hashlib.sha256(raw).hexdigest()


def run_capture(command: list[str]) -> bytes:
    process = subprocess.run(command, cwd=ROOT, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    if process.returncode != 0:
        raise SystemExit(process.stdout.decode(errors="replace"))
    return process.stdout


def main() -> None:
    status = git("status", "--porcelain", "--untracked-files=all").decode().strip()
    if status:
        raise SystemExit(f"stage8b-p1f-source-handoff: FAIL dirty worktree\n{status}")
    branch = git("branch", "--show-current").decode().strip()
    if branch != safety.BRANCH:
        raise SystemExit(f"stage8b-p1f-source-handoff: FAIL branch={branch}")
    source_ref = git("rev-parse", "HEAD").decode().strip()
    source_parent = git("rev-parse", "HEAD^").decode().strip()
    source_tree = git("rev-parse", "HEAD^{tree}").decode().strip()
    if source_parent != safety.PARENT:
        raise SystemExit("stage8b-p1f-source-handoff: FAIL source parent drift")
    changed = set(git("diff", "--name-only", safety.PARENT, source_ref, "--").decode().splitlines())
    if changed != source_check.ALLOWED_CHANGES:
        raise SystemExit(f"stage8b-p1f-source-handoff: FAIL changed paths {sorted(changed ^ source_check.ALLOWED_CHANGES)}")
    review = REVIEW_SOURCE.read_bytes()
    if sha256(review) != safety.REVIEW_SHA256:
        raise SystemExit("stage8b-p1f-source-handoff: FAIL R4 review digest")

    gate = run_capture(["bash", "scripts/stage8b_p1f_source_gate.sh"])
    multi_uid = run_capture([
        "docker", "run", "--rm", "--platform", "linux/arm64",
        "-v", f"{ROOT}:/workspace:ro", "-w", "/workspace",
        "debian:bookworm-slim", "bash", "scripts/stage8b_p1f_multi_uid_custody_harness.sh",
    ])
    if git("rev-parse", "HEAD").decode().strip() != source_ref or git("status", "--porcelain", "--untracked-files=all").decode().strip():
        raise SystemExit("stage8b-p1f-source-handoff: FAIL source changed during evidence run")

    short = source_ref[:7]
    archive_name = f"moex-trading-project-{short}-stage8b-p1f-source-review-package.zip"
    archive_path = OUTPUT / archive_name
    manifest, entries = common.source_manifest(source_ref)
    closed_surfaces = {
        "operational_install_or_service_start": False,
        "vps_mutation": False,
        "redis_db15_or_db0_mutation": False,
        "paper_provider_execution": False,
        "finam_attachment_or_send": False,
        "broker_dispatch": False,
        "runtime_live": False,
        "real_orders": False,
        "authority_rebind": False,
    }
    evidence = {
        "schema_version": 1,
        "stage": safety.STAGE,
        "status": "SOURCE_REVIEW_CANDIDATE_NO_ACTIVATION",
        "source_ref": source_ref,
        "source_parent": source_parent,
        "source_tree": source_tree,
        "branch": branch,
        "archive_name": archive_name,
        "changed_paths": sorted(changed),
        "manifest_sha256": sha256(manifest),
        "gate_sha256": sha256(gate),
        "multi_uid_sha256": sha256(multi_uid),
        "source_negative_cases": 19,
        "guardian_tests": 13,
        "closed_surfaces": closed_surfaces,
        "next_after_acceptance": "P1F-O0 immutable read-only target preflight",
    }
    marker = (
        f"stage={safety.STAGE}\nsource_short_ref={short}\nsource_ref={source_ref}\n"
        f"source_parent={source_parent}\nsource_tree={source_tree}\nbranch={branch}\n"
        f"accepted_design_ref={safety.PARENT}\narchive_name={archive_name}\n"
    ).encode()
    additions = {
        safety.MARKER: marker,
        safety.MANIFEST: manifest,
        safety.COMMIT_RAW: git("cat-file", "commit", source_ref),
        safety.EVIDENCE: (json.dumps(evidence, indent=2, sort_keys=True) + "\n").encode(),
        safety.GATE: gate,
        safety.MULTI_UID: multi_uid,
        safety.REVIEW: review,
    }

    OUTPUT.mkdir(parents=True, exist_ok=True)
    archive_path.unlink(missing_ok=True)
    with zipfile.ZipFile(archive_path, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
        for entry in entries:
            archive.writestr(common.zip_info(entry["path"], entry["mode"]), git("show", f"{source_ref}:{entry['path']}"))
        for name, raw in sorted(additions.items()):
            archive.writestr(common.zip_info(name), raw)

    result = safety.check(str(archive_path))
    digest = sha256(archive_path.read_bytes())
    archive_path.with_suffix(".zip.sha256").write_text(f"{digest}  {archive_name}\n")
    archive_path.with_suffix(".zip.safety.json").write_text(json.dumps(result, indent=2, sort_keys=True) + "\n")
    print(f"archive={archive_path}\nsha256={digest}\nsource_ref={source_ref}\nPASS stage8b-p1f-source-handoff")


if __name__ == "__main__":
    main()
