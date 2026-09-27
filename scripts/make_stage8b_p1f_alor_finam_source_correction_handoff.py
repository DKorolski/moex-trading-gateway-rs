#!/usr/bin/env python3
"""Create immutable ALOR→FINAM source-governance-closure handoff."""

from __future__ import annotations

import hashlib
import json
import subprocess
import zipfile
from pathlib import Path

import make_stage8b_design_handoff as common
import stage8b_p1f_alor_finam_source_correction_handoff_safety_check as safety


ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "reports/handoff"
REVIEW_SOURCE = Path(
    "/Users/denisq/Downloads/FINAM_aacd81c_SOURCE_ACCEPT_REVIEW_2026-09-27.md"
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
    )
    if process.returncode != 0:
        raise SystemExit(process.stdout.decode(errors="replace"))
    return process.stdout


def main() -> None:
    status = git("status", "--porcelain", "--untracked-files=all").decode().strip()
    if status:
        raise SystemExit(
            "stage8b-p1f-alor-finam-source-correction-handoff: FAIL dirty worktree\n"
            + status
        )
    branch = git("branch", "--show-current").decode().strip()
    if branch != safety.BRANCH:
        raise SystemExit(f"stage8b-p1f-alor-finam-source-correction-handoff: FAIL branch={branch}")
    source_ref = git("rev-parse", "HEAD").decode().strip()
    source_parent = git("rev-parse", "HEAD^").decode().strip()
    source_tree = git("rev-parse", "HEAD^{tree}").decode().strip()
    changed = set(
        git("diff", "--name-only", safety.BASELINE, source_ref, "--").decode().splitlines()
    )
    missing = safety.REQUIRED_TRACKED - changed
    if missing:
        raise SystemExit(
            "stage8b-p1f-alor-finam-source-correction-handoff: FAIL missing changed paths "
            + repr(sorted(missing))
        )
    review = REVIEW_SOURCE.read_bytes()
    if sha256(review) != safety.REVIEW_SHA256:
        raise SystemExit(
            "stage8b-p1f-alor-finam-source-correction-handoff: FAIL review digest"
        )

    gate = run_capture(["bash", "scripts/stage8b_p1f_alor_finam_source_correction_gate.sh"])
    current_tree = run_capture(["python3", "scripts/current_tree_authority_check.py"])
    current_tree_negative = run_capture(
        ["python3", "scripts/current_tree_authority_negative_harness.py"]
    )
    multi_uid = run_capture([
        "docker", "run", "--rm", "--user", "0:0",
        "-v", f"{ROOT}:/work:ro", "-w", "/work",
        "rust:1.90-bookworm", "bash", "scripts/stage8b_p1f_multi_uid_custody_harness.sh",
    ])
    fmt = run_capture(["cargo", "fmt", "--all", "--", "--check"])
    if git("rev-parse", "HEAD").decode().strip() != source_ref:
        raise SystemExit("stage8b-p1f-alor-finam-source-correction-handoff: FAIL HEAD changed")
    if git("status", "--porcelain", "--untracked-files=all").decode().strip():
        raise SystemExit("stage8b-p1f-alor-finam-source-correction-handoff: FAIL tree changed")

    short = source_ref[:7]
    archive_name = (
        f"moex-trading-project-{short}-stage8b-p1f-alor-finam-authority-closure-review-package.zip"
    )
    archive_path = OUTPUT / archive_name
    manifest, entries = common.source_manifest(source_ref)
    closed_surfaces = {
        "redis_db0_vps_activation": False,
        "finam_post_delete": False,
        "broker_dispatch": False,
        "runtime_live": False,
        "real_orders": False,
    }
    evidence = {
        "schema_version": 1,
        "stage": safety.STAGE,
        "status": "SOURCE_ACCEPTED_AUTHORITY_REBIND_REVIEW_PENDING_NO_ACTIVATION",
        "source_ref": source_ref,
        "source_parent": source_parent,
        "source_tree": source_tree,
        "branch": branch,
        "baseline_ref": safety.BASELINE,
        "archive_name": archive_name,
        "migration_target": "baseline07_bo_only",
        "parity_rounds": {"expected": 38, "actual": 38},
        "changed_paths": sorted(changed),
        "manifest_sha256": sha256(manifest),
        "gate_sha256": sha256(gate),
        "current_tree_authority_sha256": sha256(current_tree),
        "current_tree_negative_sha256": sha256(current_tree_negative),
        "multi_uid_sha256": sha256(multi_uid),
        "fmt_sha256": sha256(fmt),
        "closed_surfaces": closed_surfaces,
        "accepted_source_ref": "aacd81c3a9181f9d0aa55d891f76cb573b453b8d",
        "source_accept_review_sha256": safety.REVIEW_SHA256,
        "governance_only": True,
        "next_after_acceptance": (
            "rebuild exact O2 identities, source schema/template, binary and "
            "non-activating package for the corrected profile"
        ),
    }
    marker = (
        f"stage={safety.STAGE}\n"
        f"source_short_ref={short}\n"
        f"source_ref={source_ref}\n"
        f"source_parent={source_parent}\n"
        f"source_tree={source_tree}\n"
        f"branch={branch}\n"
        f"baseline_ref={safety.BASELINE}\n"
        f"archive_name={archive_name}\n"
    ).encode()
    additions = {
        safety.MARKER: marker,
        safety.MANIFEST: manifest,
        safety.COMMIT_RAW: git("cat-file", "commit", source_ref),
        safety.EVIDENCE: (json.dumps(evidence, indent=2, sort_keys=True) + "\n").encode(),
        safety.GATE: gate,
        safety.CURRENT_TREE: current_tree,
        safety.CURRENT_TREE_NEGATIVE: current_tree_negative,
        safety.MULTI_UID: multi_uid,
        safety.REVIEW: review,
    }

    OUTPUT.mkdir(parents=True, exist_ok=True)
    archive_path.unlink(missing_ok=True)
    with zipfile.ZipFile(
        archive_path,
        "w",
        compression=zipfile.ZIP_DEFLATED,
        compresslevel=9,
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
    archive_path.with_suffix(".zip.sha256").write_text(
        f"{digest}  {archive_name}\n", encoding="utf-8"
    )
    archive_path.with_suffix(".zip.safety.json").write_text(
        json.dumps(result, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    print(
        f"archive={archive_path}\nsha256={digest}\nsource_ref={source_ref}\n"
        "PASS stage8b-p1f-alor-finam-authority-closure-handoff"
    )


if __name__ == "__main__":
    main()
