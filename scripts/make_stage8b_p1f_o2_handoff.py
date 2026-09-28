#!/usr/bin/env python3
"""Create the immutable Stage 8B-P1-f O2 R1 correction review handoff."""

from __future__ import annotations

import hashlib
import json
import subprocess
import zipfile
from pathlib import Path

import make_stage8b_design_handoff as common
import stage8b_p1f_o2_handoff_safety_check as safety


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
        raise SystemExit(f"stage8b-p1f-o2-handoff: FAIL dirty worktree\n{status}")
    branch = git("branch", "--show-current").decode().strip()
    if branch != safety.BRANCH:
        raise SystemExit(f"stage8b-p1f-o2-handoff: FAIL branch={branch}")
    source_ref = git("rev-parse", "HEAD").decode().strip()
    source_parent = git("rev-parse", "HEAD^").decode().strip()
    source_tree = git("rev-parse", "HEAD^{tree}").decode().strip()
    if source_parent != safety.CONTRACT_REF:
        raise SystemExit("stage8b-p1f-o2-handoff: FAIL source parent drift")
    changed = set(
        git("diff", "--name-only", safety.R0_HANDOFF_REF, source_ref, "--")
        .decode()
        .splitlines()
    )
    if changed != safety.EXPECTED_CHANGES:
        raise SystemExit(
            "stage8b-p1f-o2-handoff: FAIL changed paths "
            f"{sorted(changed ^ safety.EXPECTED_CHANGES)}"
        )
    review = (DOWNLOADS / safety.R0_REVIEW_NAME).read_bytes()
    if sha256(review) != safety.R0_REVIEW_SHA256:
        raise SystemExit("stage8b-p1f-o2-handoff: FAIL R0 HOLD review digest")

    gate = run_capture(["bash", "scripts/stage8b_p1f_o2_gate.sh"])
    if (
        git("rev-parse", "HEAD").decode().strip() != source_ref
        or git("status", "--porcelain", "--untracked-files=all").decode().strip()
    ):
        raise SystemExit("stage8b-p1f-o2-handoff: FAIL source changed during gate")

    short = source_ref[:7]
    archive_name = (
        f"moex-trading-project-{short}-stage8b-p1f-o2-r1-"
        "execution-contract-correction-review-package.zip"
    )
    archive_path = OUTPUT / archive_name
    manifest, entries = common.source_manifest(source_ref)
    evidence = {
        "schema_version": 1,
        "stage": safety.STAGE,
        "status": safety.STATUS,
        "source_ref": source_ref,
        "source_parent": source_parent,
        "source_tree": source_tree,
        "branch": branch,
        "archive_name": archive_name,
        "contract_ref": safety.CONTRACT_REF,
        "r0_handoff_ref": safety.R0_HANDOFF_REF,
        "o1_closure_ref": safety.O1_CLOSURE_REF,
        "r0_review_name": safety.R0_REVIEW_NAME,
        "r0_review_sha256": safety.R0_REVIEW_SHA256,
        "changed_paths": sorted(changed),
        "manifest_sha256": sha256(manifest),
        "gate_sha256": sha256(gate),
        "acceptance_rows": 30,
        "negative_cases": 28,
        "rust_tests": 3,
        "rust_changes": 0,
        "cargo_changes": 0,
        "effects": {
            "remote_mutation_performed": False,
            "key_generation_performed": False,
            "finam_contact_performed": False,
            "redis_contact_performed": False,
            "systemd_reload_or_start_performed": False,
        },
        "closed_surfaces": {
            "o2_execution": False,
            "o3_synthetic_session": False,
            "o4_finam_bars_session": False,
            "ordinary_p1_service": False,
            "redis_db15_or_db0_mutation": False,
            "paper_provider_order_execution": False,
            "finam_post_or_delete": False,
            "broker_dispatch": False,
            "runtime_live": False,
            "real_orders": False,
        },
        "next_after_acceptance": (
            "Build one immutable O2 execution artifact containing only the five "
            "reviewed thin facades; review that artifact before execution"
        ),
    }
    marker = (
        f"stage={safety.STAGE}\n"
        f"source_short_ref={short}\n"
        f"source_ref={source_ref}\n"
        f"source_parent={source_parent}\n"
        f"source_tree={source_tree}\n"
        f"branch={branch}\n"
        f"contract_ref={safety.CONTRACT_REF}\n"
        f"r0_handoff_ref={safety.R0_HANDOFF_REF}\n"
        f"o1_closure_ref={safety.O1_CLOSURE_REF}\n"
        f"archive_name={archive_name}\n"
    ).encode()
    additions = {
        safety.MARKER: marker,
        safety.MANIFEST: manifest,
        safety.COMMIT_RAW: git("cat-file", "commit", source_ref),
        safety.EVIDENCE: (json.dumps(evidence, indent=2, sort_keys=True) + "\n").encode(),
        safety.GATE: gate,
        safety.R0_REVIEW: review,
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
        f"{digest}  {archive_name}\n",
        encoding="utf-8",
    )
    archive_path.with_suffix(".zip.safety.json").write_text(
        json.dumps(result, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    print(
        f"archive={archive_path}\n"
        f"sha256={digest}\n"
        f"source_ref={source_ref}\n"
        "PASS stage8b-p1f-o2-handoff"
    )


if __name__ == "__main__":
    main()
