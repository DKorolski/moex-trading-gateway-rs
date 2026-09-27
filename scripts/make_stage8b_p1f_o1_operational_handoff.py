#!/usr/bin/env python3
"""Create the immutable Stage 8B-P1-f O1 operational review handoff."""

from __future__ import annotations

import hashlib
import json
import subprocess
import zipfile
from pathlib import Path

import make_stage8b_design_handoff as common
import stage8b_p1f_o1_operational_check as source_check
import stage8b_p1f_o1_operational_handoff_safety_check as safety


ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "reports/handoff"
DOWNLOADS = Path("/Users/denisq/Downloads")


def git(*args: str) -> bytes:
    return subprocess.check_output(("git", *args), cwd=ROOT)


def sha256(raw: bytes) -> str:
    return hashlib.sha256(raw).hexdigest()


def run_capture(command: list[str]) -> bytes:
    process = subprocess.run(command, cwd=ROOT, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, check=False)
    if process.returncode != 0:
        raise SystemExit(process.stdout.decode(errors="replace"))
    return process.stdout


def main() -> None:
    status = git("status", "--porcelain", "--untracked-files=all").decode().strip()
    if status:
        raise SystemExit(f"stage8b-p1f-o1-operational-handoff: FAIL dirty worktree\n{status}")
    branch = git("branch", "--show-current").decode().strip()
    if branch != source_check.BRANCH:
        raise SystemExit(f"stage8b-p1f-o1-operational-handoff: FAIL branch={branch}")
    source_ref = git("rev-parse", "HEAD").decode().strip()
    source_parent = git("rev-parse", "HEAD^").decode().strip()
    source_tree = git("rev-parse", "HEAD^{tree}").decode().strip()
    if source_parent != source_check.BASE:
        raise SystemExit("stage8b-p1f-o1-operational-handoff: FAIL source parent drift")
    changed = set(git("diff", "--name-only", source_check.BASE, source_ref, "--").decode().splitlines())
    if changed != source_check.ALLOWED_CHANGES:
        raise SystemExit(f"stage8b-p1f-o1-operational-handoff: FAIL changed paths {sorted(changed ^ source_check.ALLOWED_CHANGES)}")
    package_review = (DOWNLOADS / source_check.REVIEW).read_bytes()
    if sha256(package_review) != source_check.REVIEW_SHA256:
        raise SystemExit("stage8b-p1f-o1-operational-handoff: FAIL acceptance review digest")
    hold_review = (DOWNLOADS / source_check.HOLD_REVIEW).read_bytes()
    if sha256(hold_review) != source_check.HOLD_REVIEW_SHA256:
        raise SystemExit("stage8b-p1f-o1-operational-handoff: FAIL HOLD review digest")
    gate = run_capture(["bash", source_check.GATE])
    if git("rev-parse", "HEAD").decode().strip() != source_ref or git("status", "--porcelain", "--untracked-files=all").decode().strip():
        raise SystemExit("stage8b-p1f-o1-operational-handoff: FAIL source changed during gate")

    short = source_ref[:7]
    archive_name = f"moex-trading-project-{short}-stage8b-p1f-o1-operational-evidence-r1-correction-review-package.zip"
    archive_path = OUTPUT / archive_name
    manifest, entries = common.source_manifest(source_ref)
    operational = git("show", f"{source_ref}:{source_check.EVIDENCE}")
    pre_raw = git("show", f"{source_ref}:{source_check.PRE_RAW}")
    post_raw = git("show", f"{source_ref}:{source_check.POST_RAW}")
    evidence = {
        "schema_version": 1,
        "stage": safety.STAGE,
        "status": "O1_OPERATIONAL_EVIDENCE_R1_CORRECTION_REVIEW_CANDIDATE",
        "source_ref": source_ref,
        "source_parent": source_parent,
        "source_tree": source_tree,
        "branch": branch,
        "archive_name": archive_name,
        "accepted_package_ref": source_check.PACKAGE_REF,
        "package_review_sha256": source_check.REVIEW_SHA256,
        "held_operational_ref": source_check.BASE,
        "hold_review_sha256": source_check.HOLD_REVIEW_SHA256,
        "changed_paths": sorted(changed),
        "manifest_sha256": sha256(manifest),
        "gate_sha256": sha256(gate),
        "operational_evidence_sha256": sha256(operational),
        "pre_raw_sha256": sha256(pre_raw),
        "post_raw_sha256": sha256(post_raw),
        "required_checks": 15,
        "negative_cases": 29,
        "systemd_behavioral_controls": 7,
        "acceptance_rows": 20,
        "rust_changes": 0,
        "cargo_changes": 0,
        "remote_mutation_performed": True,
        "activation_performed": False,
        "o2_authorized": False,
    }
    marker = (
        f"stage={safety.STAGE}\nsource_short_ref={short}\nsource_ref={source_ref}\n"
        f"source_parent={source_parent}\nsource_tree={source_tree}\nbranch={branch}\n"
        f"accepted_package_ref={source_check.PACKAGE_REF}\narchive_name={archive_name}\n"
    ).encode()
    additions = {
        safety.MARKER: marker,
        safety.MANIFEST: manifest,
        safety.COMMIT_RAW: git("cat-file", "commit", source_ref),
        safety.EVIDENCE: (json.dumps(evidence, indent=2, sort_keys=True) + "\n").encode(),
        safety.GATE: gate,
        safety.PACKAGE_REVIEW: package_review,
        safety.HOLD_REVIEW: hold_review,
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
    print(f"archive={archive_path}\nsha256={digest}\nsource_ref={source_ref}\nPASS stage8b-p1f-o1-operational-handoff")


if __name__ == "__main__":
    main()
