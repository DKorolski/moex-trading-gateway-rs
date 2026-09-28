#!/usr/bin/env python3
"""Create the immutable Stage 8B-P1-f Ie aggregate source handoff."""

from __future__ import annotations

import hashlib
import json
import subprocess
import zipfile
from pathlib import Path

import make_stage8b_design_handoff as common
import stage8b_p1f_ie_check as source_check
import stage8b_p1f_ie_handoff_safety_check as safety


ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "reports/handoff"
DOWNLOADS = Path("/Users/denisq/Downloads")


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
        raise SystemExit(f"stage8b-p1f-ie-handoff: FAIL dirty worktree\n{status}")
    branch = git("branch", "--show-current").decode().strip()
    if branch != safety.BRANCH:
        raise SystemExit(f"stage8b-p1f-ie-handoff: FAIL branch={branch}")
    source_ref = git("rev-parse", "HEAD").decode().strip()
    source_parent = git("rev-parse", "HEAD^").decode().strip()
    source_tree = git("rev-parse", "HEAD^{tree}").decode().strip()
    if source_parent != source_check.CORRECTION_BASE:
        raise SystemExit("stage8b-p1f-ie-handoff: FAIL source parent drift")
    changed = set(
        git("diff", "--name-only", source_check.CORRECTION_BASE, source_ref, "--")
        .decode()
        .splitlines()
    )
    if changed != source_check.ALLOWED_CHANGES:
        raise SystemExit(
            "stage8b-p1f-ie-handoff: FAIL changed paths "
            f"{sorted(changed ^ source_check.ALLOWED_CHANGES)}"
        )

    reviews: dict[str, bytes] = {}
    for item in source_check.ACCEPTED_SOURCES:
        path = DOWNLOADS / item["review_file"]
        raw = path.read_bytes()
        if sha256(raw) != item["review_sha256"]:
            raise SystemExit(f"stage8b-p1f-ie-handoff: FAIL review digest {path.name}")
        reviews[safety.PREFIX + "reviews/" + path.name] = raw
    correction_review_path = DOWNLOADS / source_check.CORRECTION_REVIEW["file"]
    correction_review = correction_review_path.read_bytes()
    if sha256(correction_review) != source_check.CORRECTION_REVIEW["sha256"]:
        raise SystemExit(
            f"stage8b-p1f-ie-handoff: FAIL review digest {correction_review_path.name}"
        )
    reviews[safety.PREFIX + "reviews/" + correction_review_path.name] = correction_review

    gate = run_capture(["bash", "scripts/stage8b_p1f_ie_gate.sh"])
    if (
        git("rev-parse", "HEAD").decode().strip() != source_ref
        or git("status", "--porcelain", "--untracked-files=all").decode().strip()
    ):
        raise SystemExit("stage8b-p1f-ie-handoff: FAIL source changed during evidence run")

    short = source_ref[:7]
    archive_name = f"moex-trading-project-{short}-stage8b-p1f-ie-linked-correction-review-package.zip"
    archive_path = OUTPUT / archive_name
    manifest, entries = common.source_manifest(source_ref)
    closed_surfaces = {
        "p1f_o0_read_only_target_preflight": False,
        "installation_or_systemd_activation": False,
        "vps_mutation": False,
        "operational_redis_db15_or_db0": False,
        "paper_provider_execution": False,
        "finam_post_or_delete": False,
        "broker_dispatch": False,
        "runtime_live": False,
        "real_orders": False,
    }
    evidence = {
        "schema_version": 1,
        "stage": safety.STAGE,
        "status": "LINKED_COMPOSITION_CORRECTION_REVIEW_CANDIDATE_NO_ACTIVATION",
        "source_ref": source_ref,
        "source_parent": source_parent,
        "source_tree": source_tree,
        "branch": branch,
        "archive_name": archive_name,
        "accepted_sources": list(source_check.ACCEPTED_SOURCES),
        "correction_review": source_check.CORRECTION_REVIEW,
        "changed_paths": sorted(changed),
        "manifest_sha256": sha256(manifest),
        "gate_sha256": sha256(gate),
        "negative_cases": 24,
        "acceptance_matrix_rows": 24,
        "linked_witnesses": 1,
        "aggregate_regression_steps": 9,
        "rust_fixture_files_changed": len(source_check.RUST_FIXTURE_CHANGES),
        "new_production_rust_files": 0,
        "cargo_changes": 0,
        "closed_surfaces": closed_surfaces,
        "next_after_acceptance": "P1F-O0 immutable read-only target preflight; operational activation remains closed",
    }
    marker = (
        f"stage={safety.STAGE}\n"
        f"source_short_ref={short}\n"
        f"source_ref={source_ref}\n"
        f"source_parent={source_parent}\n"
        f"source_tree={source_tree}\n"
        f"branch={branch}\n"
        f"accepted_id_ref={source_check.BASE}\n"
        f"reviewed_ie_ref={source_check.CORRECTION_BASE}\n"
        f"archive_name={archive_name}\n"
    ).encode()
    additions = {
        safety.MARKER: marker,
        safety.MANIFEST: manifest,
        safety.COMMIT_RAW: git("cat-file", "commit", source_ref),
        safety.EVIDENCE: (json.dumps(evidence, indent=2, sort_keys=True) + "\n").encode(),
        safety.GATE: gate,
        **reviews,
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
        "PASS stage8b-p1f-ie-handoff"
    )


if __name__ == "__main__":
    main()
