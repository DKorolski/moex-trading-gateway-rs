#!/usr/bin/env python3
"""Create an immutable committed Cancel/Day-expiry restart review handoff."""

from __future__ import annotations

import hashlib
import json
import subprocess
import zipfile
from pathlib import Path

import make_stage8b_design_handoff as common
import stage8b_p1e_i1_committed_restart_handoff_safety_check as safety


ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "reports/handoff"


def run(*args: str) -> bytes:
    return subprocess.check_output(args, cwd=ROOT)


def sha256(raw: bytes) -> str:
    return hashlib.sha256(raw).hexdigest()


def main() -> None:
    if run("git", "status", "--porcelain", "--untracked-files=all").decode().strip():
        raise SystemExit("stage8b-p1e-i1-committed-restart-handoff: FAIL dirty worktree")
    branch = run("git", "branch", "--show-current").decode().strip()
    if branch != safety.BRANCH:
        raise SystemExit(f"stage8b-p1e-i1-committed-restart-handoff: FAIL branch={branch}")
    source_ref = run("git", "rev-parse", "HEAD").decode().strip()
    source_parent = run("git", "rev-parse", "HEAD^").decode().strip()
    source_tree = run("git", "rev-parse", "HEAD^{tree}").decode().strip()
    if source_parent != safety.PARENT:
        raise SystemExit(
            "stage8b-p1e-i1-committed-restart-handoff: FAIL "
            f"parent={source_parent} expected={safety.PARENT}"
        )

    gate = subprocess.run(
        ["bash", "scripts/stage8b_p1e_i1_committed_restart_gate.sh"],
        cwd=ROOT,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=False,
    )
    if gate.returncode != 0 or b"PASS stage8b-p1e-i1-committed-restart-gate" not in gate.stdout:
        raise SystemExit(gate.stdout.decode(errors="replace"))

    short_ref = source_ref[:7]
    archive_name = (
        f"moex-trading-project-{short_ref}-"
        "stage8b-p1e-i1-committed-cancel-day-expiry-restart.zip"
    )
    archive_path = OUTPUT / archive_name
    manifest, entries = common.source_manifest(source_ref)
    evidence = {
        "schema_version": 1,
        "stage": "Stage 8B-P1-e I1 committed Cancel/Day-expiry restart recovery",
        "status": "SOURCE_REVIEW_CANDIDATE",
        "source_ref": source_ref,
        "source_parent": source_parent,
        "source_tree": source_tree,
        "branch": branch,
        "archive_name": archive_name,
        "gate_sha256": sha256(gate.stdout),
        "manifest_sha256": sha256(manifest),
        "focused_tests": 7,
        "runtime_full_suite": {"passed": 265, "ignored": 8, "failed": 0},
        "runtime_integration": {"passed": 9, "ignored": 6, "failed": 0},
        "runtime_doctests": {"passed": 61, "failed": 0},
        "core_full_suite": {"passed": 1283, "failed": 0},
        "core_doctests": {"passed": 69, "failed": 0},
        "negative_cases": 21,
        "inherited_i1a_negative_cases": 102,
        "inherited_p1d4_negative_cases": 60,
        "production_owner_loop_wiring": False,
        "closed_surfaces": {
            "operational_redis_db0_db15": False,
            "vps_activation": False,
            "finam_post_delete_send": False,
            "broker_dispatch": False,
            "runtime_live": False,
            "real_orders": False,
        },
        "next_slice": "production committed-owner retention and wakeup wiring",
    }
    evidence_bytes = (json.dumps(evidence, indent=2, sort_keys=True) + "\n").encode()
    marker = (
        "stage=Stage 8B-P1-e I1 committed restart\n"
        f"source_short_ref={short_ref}\nsource_ref={source_ref}\n"
        f"source_parent={source_parent}\nsource_tree={source_tree}\n"
        f"branch={branch}\narchive_name={archive_name}\n"
    ).encode()
    additions = {
        safety.MARKER: marker,
        safety.MANIFEST: manifest,
        safety.COMMIT_RAW: run("git", "cat-file", "commit", source_ref),
        safety.EVIDENCE: evidence_bytes,
        safety.GATE: gate.stdout,
    }

    OUTPUT.mkdir(parents=True, exist_ok=True)
    archive_path.unlink(missing_ok=True)
    with zipfile.ZipFile(
        archive_path, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9
    ) as archive:
        for entry in entries:
            archive.writestr(
                common.zip_info(entry["path"], entry["mode"]),
                run("git", "show", f"{source_ref}:{entry['path']}"),
            )
        for name, raw in sorted(additions.items()):
            archive.writestr(common.zip_info(name), raw)

    result = safety.check(str(archive_path))
    digest = sha256(archive_path.read_bytes())
    archive_path.with_suffix(".zip.sha256").write_text(
        f"{digest}  {archive_name}\n", encoding="utf-8"
    )
    archive_path.with_suffix(".zip.safety.json").write_text(
        json.dumps(result, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    print(
        f"archive={archive_path}\nsha256={digest}\nsource_ref={source_ref}\n"
        "stage8b-p1e-i1-committed-restart-handoff: PASS"
    )


if __name__ == "__main__":
    main()
