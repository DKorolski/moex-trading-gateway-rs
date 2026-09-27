#!/usr/bin/env python3
"""Create the immutable Stage 8B-P1-f O0 review handoff."""

from __future__ import annotations

import hashlib
import json
import subprocess
import zipfile
from pathlib import Path

import make_stage8b_design_handoff as common
import stage8b_p1f_o0_check as source_check
import stage8b_p1f_o0_handoff_safety_check as safety


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
        raise SystemExit(f"stage8b-p1f-o0-handoff: FAIL dirty worktree\n{status}")
    branch = git("branch", "--show-current").decode().strip()
    if branch != source_check.BRANCH:
        raise SystemExit(f"stage8b-p1f-o0-handoff: FAIL branch={branch}")
    source_ref = git("rev-parse", "HEAD").decode().strip()
    source_parent = git("rev-parse", "HEAD^").decode().strip()
    source_tree = git("rev-parse", "HEAD^{tree}").decode().strip()
    if source_parent != source_check.BASE:
        raise SystemExit("stage8b-p1f-o0-handoff: FAIL source parent drift")
    changed = set(git("diff", "--name-only", source_check.BASE, source_ref, "--").decode().splitlines())
    if changed != source_check.ALLOWED_CHANGES:
        raise SystemExit(f"stage8b-p1f-o0-handoff: FAIL changed paths {sorted(changed ^ source_check.ALLOWED_CHANGES)}")

    review_path = DOWNLOADS / source_check.IE_REVIEW
    review = review_path.read_bytes()
    if sha256(review) != source_check.IE_REVIEW_SHA256:
        raise SystemExit("stage8b-p1f-o0-handoff: FAIL Ie review digest")
    gate = run_capture(["bash", "scripts/stage8b_p1f_o0_gate.sh"])
    if git("rev-parse", "HEAD").decode().strip() != source_ref or git("status", "--porcelain", "--untracked-files=all").decode().strip():
        raise SystemExit("stage8b-p1f-o0-handoff: FAIL source changed during gate")

    short = source_ref[:7]
    archive_name = f"moex-trading-project-{short}-stage8b-p1f-o0-readonly-preflight-review-package.zip"
    archive_path = OUTPUT / archive_name
    manifest, entries = common.source_manifest(source_ref)
    raw_probe = git("show", f"{source_ref}:{source_check.RAW}")
    target_evidence = git("show", f"{source_ref}:{source_check.EVIDENCE}")
    evidence = {
        "schema_version": 1,
        "stage": safety.STAGE,
        "status": "O0_REVIEW_CANDIDATE_NO_REMOTE_MUTATION",
        "source_ref": source_ref,
        "source_parent": source_parent,
        "source_tree": source_tree,
        "branch": branch,
        "archive_name": archive_name,
        "accepted_ie_ref": source_check.ACCEPTED_IE,
        "accepted_ie_review": {"file": source_check.IE_REVIEW, "sha256": source_check.IE_REVIEW_SHA256},
        "changed_paths": sorted(changed),
        "manifest_sha256": sha256(manifest),
        "gate_sha256": sha256(gate),
        "target_evidence_sha256": sha256(target_evidence),
        "raw_probe_sha256": sha256(raw_probe),
        "negative_cases": 20,
        "acceptance_matrix_rows": 20,
        "rust_changes": 0,
        "cargo_changes": 0,
        "remote_mutation_performed": False,
        "o1_authorized": False,
        "closed_surfaces": {
            "p1f_o1_provisioning": False,
            "installation_or_systemd_activation": False,
            "redis_db15_or_db0_mutation": False,
            "paper_provider_execution": False,
            "finam_post_or_delete": False,
            "broker_dispatch": False,
            "runtime_live": False,
            "real_orders": False,
        },
        "next_after_acceptance": "Separate P1F-O1 non-activating provisioning package",
    }
    marker = (
        f"stage={safety.STAGE}\nsource_short_ref={short}\nsource_ref={source_ref}\n"
        f"source_parent={source_parent}\nsource_tree={source_tree}\nbranch={branch}\n"
        f"accepted_ie_ref={source_check.ACCEPTED_IE}\narchive_name={archive_name}\n"
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
    with zipfile.ZipFile(archive_path, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
        for entry in entries:
            archive.writestr(common.zip_info(entry["path"], entry["mode"]), git("show", f"{source_ref}:{entry['path']}"))
        for name, raw in sorted(additions.items()):
            archive.writestr(common.zip_info(name), raw)
    result = safety.check(str(archive_path))
    digest = sha256(archive_path.read_bytes())
    archive_path.with_suffix(".zip.sha256").write_text(f"{digest}  {archive_name}\n")
    archive_path.with_suffix(".zip.safety.json").write_text(json.dumps(result, indent=2, sort_keys=True) + "\n")
    print(f"archive={archive_path}\nsha256={digest}\nsource_ref={source_ref}\nPASS stage8b-p1f-o0-handoff")


if __name__ == "__main__":
    main()
