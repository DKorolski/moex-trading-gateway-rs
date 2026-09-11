#!/usr/bin/env python3
"""Build the immutable Foundation R1 + I1A design review package with full logs."""

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
import stage8b_p1e_i1a_handoff_safety_check as safety


ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "reports/handoff"
BRANCH = "stage8b-paper-shadow-resumption"
DESIGN_REF = safety.DESIGN_REF
FOUNDATION_REF = safety.FOUNDATION_REF
REVIEW_PATH = Path("/Users/denisq/Downloads/FINAM_P1e_I1_FOUNDATION_REVIEW_c7cec79_I1A_DECISION_2026-09-11.md")


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
        display_prefix = " ".join(f"{key}={shlex.quote(value)}" for key, value in sorted(environment.items())) + " "
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
        f"check_name={name}\n"
        f"source_ref={source_ref}\n"
        f"source_tree={source_tree}\n"
        f"invocation={invocation}\n"
        "--- stdout-stderr ---\n"
    ).encode() + completed.stdout + (
        "\n--- result ---\n"
        f"exit_code={completed.returncode}\n"
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
        raise SystemExit(f"stage8b-p1e-i1a-handoff: FAIL dirty worktree\n{status}")
    branch = git("branch", "--show-current").decode().strip()
    if branch != BRANCH:
        raise SystemExit(f"stage8b-p1e-i1a-handoff: FAIL branch={branch}")
    source_ref = git("rev-parse", "HEAD").decode().strip()
    source_short_ref = source_ref[:7]
    source_tree = git("rev-parse", "HEAD^{tree}").decode().strip()
    parent_ref = git("rev-parse", "HEAD^").decode().strip()
    if parent_ref != DESIGN_REF:
        raise SystemExit(f"stage8b-p1e-i1a-handoff: FAIL parent={parent_ref}")
    if git("rev-parse", f"{DESIGN_REF}^").decode().strip() != FOUNDATION_REF:
        raise SystemExit("stage8b-p1e-i1a-handoff: FAIL design/foundation lineage")
    review = REVIEW_PATH.read_bytes()
    if sha256(review) != safety.REVIEW_SHA256:
        raise SystemExit("stage8b-p1e-i1a-handoff: FAIL review digest")

    specifications: list[tuple[str, list[str], dict[str, str] | None]] = [
        ("foundation_gate", ["bash", "scripts/stage8b_p1e_i1_foundation_r1_gate.sh"], None),
        ("i1a_design_gate", ["bash", "scripts/stage8b_p1e_i1a_design_gate.sh"], None),
        ("fmt", ["cargo", "fmt", "--all", "--", "--check"], None),
        ("strategy_runtime_core_lib", ["cargo", "test", "-p", "strategy-runtime-core", "--lib", "--all-features"], None),
        ("runtime_durable_service_lib", ["cargo", "test", "-p", "runtime-durable-service", "--lib", "--all-features"], {"RUST_MIN_STACK": "33554432"}),
        ("strategy_runtime_core_doc", ["cargo", "test", "-p", "strategy-runtime-core", "--doc", "--all-features"], None),
        ("runtime_durable_service_doc", ["cargo", "test", "-p", "runtime-durable-service", "--doc", "--all-features"], None),
        ("strict_clippy", ["cargo", "clippy", "-p", "strategy-runtime-core", "-p", "runtime-durable-service", "--all-targets", "--all-features", "--", "-D", "warnings"], None),
        ("p1d4_negative", ["python3", "scripts/stage8b_p1d4_source_negative_harness.py"], {"PYTHONPATH": "scripts"}),
    ]
    records: dict[str, Any] = {}
    logs: dict[str, bytes] = {}
    for name, command, environment in specifications:
        record, log = run_check(name, command, source_ref, source_tree, environment)
        records[name] = record
        logs[safety.LOGS[name]] = log

    if git("rev-parse", "HEAD").decode().strip() != source_ref or git("rev-parse", "HEAD^{tree}").decode().strip() != source_tree:
        raise SystemExit("stage8b-p1e-i1a-handoff: FAIL source changed during verification")
    if git("status", "--porcelain", "--untracked-files=all").decode().strip():
        raise SystemExit("stage8b-p1e-i1a-handoff: FAIL worktree changed during verification")

    archive_name = f"moex-trading-project-{source_short_ref}-stage8b-p1e-i1-foundation-r1-i1a-design-review-package.zip"
    archive_path = OUTPUT / archive_name
    manifest_raw, entries = common.source_manifest(source_ref)
    manifest = json.loads(manifest_raw)
    manifest["schema_version"] = 3
    manifest["stage"] = "Stage 8B-P1-e I1 foundation R1 + I1A design"
    manifest["source_tree"] = source_tree
    manifest["source_branch"] = branch
    manifest_raw = (json.dumps(manifest, indent=2, sort_keys=True) + "\n").encode()
    index = {
        "schema_version": 1,
        "stage": "Stage 8B-P1-e I1 foundation R1 + I1A design",
        "status": "REVIEW_CANDIDATE_IMPLEMENTATION_NOT_AUTHORIZED",
        "source_ref": source_ref,
        "source_tree": source_tree,
        "source_branch": branch,
        "parent_ref": parent_ref,
        "foundation_ref": FOUNDATION_REF,
        "design_ref": DESIGN_REF,
        "review_sha256": safety.REVIEW_SHA256,
        "source_manifest_sha256": sha256(manifest_raw),
        "all_passed": True,
        "checks": records,
        "foundation_fixes": {
            "retained_shutdown_cause": True,
            "atomic_stale_consumer_cleanup": True,
        },
        "i1a_design": {
            "acceptance_rows": 81,
            "negative_cases": 23,
            "production_implementation_authorized": False,
        },
        "closed_surfaces": {
            "redis_db0_vps_activation": False,
            "redis_db15_schedule_activation": False,
            "schedule_private_key_installation": False,
            "finam_post_delete": False,
            "broker_dispatch": False,
            "runtime_live": False,
            "real_orders": False,
            "generation_2_execution_activation": False,
        },
    }
    index_raw = (json.dumps(index, indent=2, sort_keys=True) + "\n").encode()
    marker = (
        "stage=Stage 8B-P1-e I1 foundation R1 + I1A design\n"
        f"source_short_ref={source_short_ref}\n"
        f"source_ref={source_ref}\n"
        f"source_tree={source_tree}\n"
        f"source_branch={branch}\n"
        f"parent_ref={parent_ref}\n"
        f"design_ref={DESIGN_REF}\n"
        f"foundation_ref={FOUNDATION_REF}\n"
        f"archive_name={archive_name}\n"
    ).encode()
    additions = {
        safety.MARKER: marker,
        safety.MANIFEST: manifest_raw,
        safety.COMMIT_RAW: git("cat-file", "commit", source_ref),
        safety.INDEX: index_raw,
        safety.REVIEW: review,
        **logs,
    }

    OUTPUT.mkdir(parents=True, exist_ok=True)
    archive_path.unlink(missing_ok=True)
    with zipfile.ZipFile(archive_path, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
        for entry in entries:
            archive.writestr(
                common.zip_info(entry["path"], entry["mode"]),
                git("show", f"{source_ref}:{entry['path']}"),
            )
        for name, data in sorted(additions.items()):
            archive.writestr(common.zip_info(name), data)

    result = safety.check(str(archive_path))
    digest = sha256(archive_path.read_bytes())
    sha_path = archive_path.with_suffix(".zip.sha256")
    safety_path = archive_path.with_suffix(".zip.safety.json")
    sha_path.write_text(f"{digest}  {archive_name}\n", encoding="utf-8")
    safety_path.write_text(json.dumps(result, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(
        f"archive={archive_path}\n"
        f"sha256={digest}\n"
        f"safety={safety_path}\n"
        f"source_ref={source_ref}\n"
        f"source_tree={source_tree}\n"
        "stage8b-p1e-i1a-handoff: PASS"
    )


if __name__ == "__main__":
    main()
