#!/usr/bin/env python3
"""Create the immutable I1 fixed-path installation review handoff."""

from __future__ import annotations

import hashlib
import json
import os
import shlex
import subprocess
import zipfile
from pathlib import Path
from typing import Any

import make_stage8b_design_handoff as common
import stage8b_p1e_i1_fixed_install_handoff_safety_check as safety


ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "reports/handoff"
REPORTS = ROOT / "reports/stage8b-p1e-i1-fixed-install"
REVIEW_SOURCE = Path(
    "/Users/denisq/Downloads/FINAM_I1_FIXED_INSTALL_REVIEW_37b9d06_2026-09-24.md"
)


def git(*args: str) -> bytes:
    return subprocess.check_output(("git", *args), cwd=ROOT)


def sha256(raw: bytes) -> str:
    return hashlib.sha256(raw).hexdigest()


def run_check(
    name: str,
    command: list[str],
    source_ref: str,
    source_tree: str,
    environment: dict[str, str] | None = None,
) -> tuple[dict[str, Any], bytes]:
    env = os.environ.copy()
    display_prefix = ""
    if environment:
        env.update(environment)
        display_prefix = " ".join(
            f"{key}={shlex.quote(value)}" for key, value in sorted(environment.items())
        ) + " "
    invocation = display_prefix + shlex.join(command)
    print(f"RUN {name}: {invocation}", flush=True)
    completed = subprocess.run(
        command,
        cwd=ROOT,
        env=env,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=False,
    )
    body = (
        f"check_name={name}\nsource_ref={source_ref}\nsource_tree={source_tree}\n"
        f"invocation={invocation}\n--- stdout-stderr ---\n"
    ).encode() + completed.stdout + (
        f"\n--- result ---\nexit_code={completed.returncode}\n"
    ).encode()
    print(f"DONE {name}: exit={completed.returncode}", flush=True)
    if completed.returncode != 0:
        raise SystemExit(body.decode(errors="replace"))
    return (
        {
            "invocation": invocation,
            "exit_code": completed.returncode,
            "log_path": safety.LOGS[name],
            "log_sha256": sha256(body),
            "log_bytes": len(body),
        },
        body,
    )


def main() -> None:
    status = git("status", "--porcelain", "--untracked-files=all").decode().strip()
    if status:
        raise SystemExit(f"stage8b-p1e-i1-fixed-install-handoff: FAIL dirty worktree\n{status}")
    branch = git("branch", "--show-current").decode().strip()
    if branch != safety.BRANCH:
        raise SystemExit(f"stage8b-p1e-i1-fixed-install-handoff: FAIL branch={branch}")
    source_ref = git("rev-parse", "HEAD").decode().strip()
    source_short_ref = source_ref[:7]
    source_parent = git("rev-parse", "HEAD^").decode().strip()
    source_tree = git("rev-parse", "HEAD^{tree}").decode().strip()
    if source_parent != safety.PARENT:
        raise SystemExit(
            f"stage8b-p1e-i1-fixed-install-handoff: FAIL parent={source_parent} expected={safety.PARENT}"
        )
    changed_paths = set(
        git("diff", "--name-only", safety.PARENT, source_ref, "--").decode().splitlines()
    )
    if changed_paths != safety.ALLOWED_CHANGES:
        raise SystemExit("stage8b-p1e-i1-fixed-install-handoff: FAIL changed-path inventory")
    review_raw = REVIEW_SOURCE.read_bytes()
    if sha256(review_raw) != safety.REVIEW_SHA256:
        raise SystemExit("stage8b-p1e-i1-fixed-install-handoff: FAIL predecessor review digest")
    report_files = {path.name for path in REPORTS.iterdir() if path.is_file()}
    if report_files != safety.REPORT_FILES:
        raise SystemExit("stage8b-p1e-i1-fixed-install-handoff: FAIL target evidence inventory")

    specifications: list[tuple[str, list[str], dict[str, str] | None]] = [
        (
            "fixed_install",
            ["python3", "scripts/stage8b_p1e_i1_fixed_install_check.py", "--evidence-dir", str(REPORTS)],
            None,
        ),
        (
            "fixed_install_negative",
            ["python3", "scripts/stage8b_p1e_i1_fixed_install_negative_harness.py"],
            None,
        ),
        (
            "telemetry",
            ["python3", "scripts/stage8b_p1e_i1_telemetry_composition_check.py", "--skip-lineage"],
            None,
        ),
        (
            "telemetry_negative",
            ["python3", "scripts/stage8b_p1e_i1_telemetry_composition_negative_harness.py"],
            None,
        ),
        ("process", ["python3", "scripts/stage8b_p1e_i1_process_supervision_check.py"], None),
        (
            "process_negative",
            ["python3", "scripts/stage8b_p1e_i1_process_supervision_negative_harness.py"],
            None,
        ),
    ]
    records: dict[str, Any] = {}
    logs: dict[str, bytes] = {}
    for name, command, environment in specifications:
        record, log = run_check(name, command, source_ref, source_tree, environment)
        records[name] = record
        logs[safety.LOGS[name]] = log

    if git("rev-parse", "HEAD").decode().strip() != source_ref:
        raise SystemExit("stage8b-p1e-i1-fixed-install-handoff: FAIL source ref changed")
    if git("rev-parse", "HEAD^{tree}").decode().strip() != source_tree:
        raise SystemExit("stage8b-p1e-i1-fixed-install-handoff: FAIL source tree changed")
    if git("status", "--porcelain", "--untracked-files=all").decode().strip():
        raise SystemExit("stage8b-p1e-i1-fixed-install-handoff: FAIL worktree changed during verification")

    archive_name = f"moex-trading-project-{source_short_ref}-stage8b-p1e-i1-fixed-install.zip"
    archive_path = OUTPUT / archive_name
    manifest_raw, entries = common.source_manifest(source_ref)
    report_hashes = {
        name: sha256((REPORTS / name).read_bytes()) for name in sorted(safety.REPORT_FILES)
    }
    evidence = {
        "schema_version": 1,
        "stage": safety.STAGE,
        "status": "SOURCE_MATERIAL_CORRECTION_REVIEW_CANDIDATE_I1_NOT_CLOSED",
        "source_ref": source_ref,
        "source_parent": source_parent,
        "source_tree": source_tree,
        "branch": branch,
        "archive_name": archive_name,
        "manifest_sha256": sha256(manifest_raw),
        "changed_paths": sorted(changed_paths),
        "commands": records,
        "target_evidence_sha256": report_hashes,
        "target_linux": "ubuntu-24.04",
        "target_systemd_version": 255,
        "network_mode": "none",
        "accepted_release_binary_source_ref": safety.ACCEPTED_BINARY_REF,
        "non_activating_install": True,
        "operational_activation_authorized": False,
        "i1_closed": False,
        "next_slice": "independent source/material review followed by aggregate I1 acceptance",
        "closed_surfaces": {
            "operational_redis_db0": False,
            "operational_redis_db15": False,
            "vps_installation_or_service_start": False,
            "paper_provider_operational_activation": False,
            "finam_post_delete_send": False,
            "broker_dispatch": False,
            "runtime_live": False,
            "real_orders": False,
            "p1f_authorized": False,
        },
        "predecessor_review_sha256": safety.REVIEW_SHA256,
    }
    evidence_raw = (json.dumps(evidence, indent=2, sort_keys=True) + "\n").encode()
    marker = (
        f"stage={safety.STAGE}\nsource_short_ref={source_short_ref}\nsource_ref={source_ref}\n"
        f"source_parent={source_parent}\nsource_tree={source_tree}\nbranch={branch}\n"
        f"archive_name={archive_name}\n"
    ).encode()
    additions = {
        safety.MARKER: marker,
        safety.MANIFEST: manifest_raw,
        safety.COMMIT_RAW: git("cat-file", "commit", source_ref),
        safety.EVIDENCE: evidence_raw,
        safety.REVIEW: review_raw,
        **logs,
        **{
            safety.REPORT_PREFIX + name: (REPORTS / name).read_bytes()
            for name in safety.REPORT_FILES
        },
    }

    OUTPUT.mkdir(parents=True, exist_ok=True)
    archive_path.unlink(missing_ok=True)
    with zipfile.ZipFile(archive_path, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
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
        json.dumps(result, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    print(
        f"archive={archive_path}\nsha256={digest}\nsource_ref={source_ref}\n"
        "stage8b-p1e-i1-fixed-install-handoff: PASS"
    )


if __name__ == "__main__":
    main()
