#!/usr/bin/env python3
"""Create the immutable committed Cancel/Day-expiry owner-loop handoff."""

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
import stage8b_p1e_i1_owner_loop_handoff_safety_check as safety


ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "reports/handoff"


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
        raise SystemExit(f"stage8b-p1e-i1-owner-loop-handoff: FAIL dirty worktree\n{status}")
    branch = git("branch", "--show-current").decode().strip()
    if branch != safety.BRANCH:
        raise SystemExit(f"stage8b-p1e-i1-owner-loop-handoff: FAIL branch={branch}")
    source_ref = git("rev-parse", "HEAD").decode().strip()
    source_short_ref = source_ref[:7]
    source_parent = git("rev-parse", "HEAD^").decode().strip()
    source_tree = git("rev-parse", "HEAD^{tree}").decode().strip()
    if source_parent != safety.PARENT:
        raise SystemExit(
            "stage8b-p1e-i1-owner-loop-handoff: FAIL "
            f"parent={source_parent} expected={safety.PARENT}"
        )

    specifications: list[tuple[str, list[str], dict[str, str] | None]] = [
        ("source_gate", ["bash", "scripts/stage8b_p1e_i1_owner_loop_gate.sh"], None),
        (
            "runtime_lib",
            ["cargo", "test", "-p", "runtime-durable-service", "--lib", "--all-features"],
            {"RUST_MIN_STACK": "33554432"},
        ),
        (
            "runtime_redis_integration",
            [
                "cargo", "test", "-p", "runtime-durable-service", "--all-features",
                "--test", "stage7b_redis_service_subprocess", "--", "--test-threads=1",
            ],
            {"RUST_MIN_STACK": "33554432"},
        ),
        (
            "runtime_writer_integration",
            [
                "cargo", "test", "-p", "runtime-durable-service", "--all-features",
                "--test", "stage7b_writer_lock_subprocess", "--", "--test-threads=1",
            ],
            {"RUST_MIN_STACK": "33554432"},
        ),
        (
            "runtime_doc",
            ["cargo", "test", "-p", "runtime-durable-service", "--doc", "--all-features"],
            None,
        ),
        (
            "core_full",
            ["cargo", "test", "-p", "strategy-runtime-core", "--all-features"],
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
        raise SystemExit("stage8b-p1e-i1-owner-loop-handoff: FAIL source ref changed")
    if git("rev-parse", "HEAD^{tree}").decode().strip() != source_tree:
        raise SystemExit("stage8b-p1e-i1-owner-loop-handoff: FAIL source tree changed")
    if git("status", "--porcelain", "--untracked-files=all").decode().strip():
        raise SystemExit("stage8b-p1e-i1-owner-loop-handoff: FAIL worktree changed during verification")

    archive_name = (
        f"moex-trading-project-{source_short_ref}-"
        "stage8b-p1e-i1-committed-owner-loop-wiring.zip"
    )
    archive_path = OUTPUT / archive_name
    manifest_raw, entries = common.source_manifest(source_ref)
    evidence = {
        "schema_version": 1,
        "stage": "Stage 8B-P1-e I1 committed owner-loop wiring",
        "status": "SOURCE_REVIEW_CANDIDATE",
        "source_ref": source_ref,
        "source_parent": source_parent,
        "source_tree": source_tree,
        "branch": branch,
        "archive_name": archive_name,
        "manifest_sha256": sha256(manifest_raw),
        "commands": records,
        "production_owner_loop_wiring": True,
        "direct_evidence": {
            "actual_sequence_allocations": True,
            "provider_effect_counter": True,
            "callback_effect_counter": True,
            "publication_effect_counter": True,
            "claim_effect_counter": True,
            "xack_effect_counter": True,
            "schedule_read_effect_counter": True,
            "final_xack_response_loss_replay": True,
        },
        "closed_surfaces": {
            "operational_redis_db0_db15": False,
            "vps_activation": False,
            "finam_post_delete_send": False,
            "broker_dispatch": False,
            "runtime_live": False,
            "real_orders": False,
        },
        "next_slice": "separate OS-process SIGTERM/panic/SIGKILL owner-loop matrix",
    }
    evidence_raw = (json.dumps(evidence, indent=2, sort_keys=True) + "\n").encode()
    marker = (
        "stage=Stage 8B-P1-e I1 committed owner-loop wiring\n"
        f"source_short_ref={source_short_ref}\nsource_ref={source_ref}\n"
        f"source_parent={source_parent}\nsource_tree={source_tree}\n"
        f"branch={branch}\narchive_name={archive_name}\n"
    ).encode()
    additions = {
        safety.MARKER: marker,
        safety.MANIFEST: manifest_raw,
        safety.COMMIT_RAW: git("cat-file", "commit", source_ref),
        safety.EVIDENCE: evidence_raw,
        **logs,
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
    archive_path.with_suffix(".zip.sha256").write_text(
        f"{digest}  {archive_name}\n", encoding="utf-8"
    )
    archive_path.with_suffix(".zip.safety.json").write_text(
        json.dumps(result, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    print(
        f"archive={archive_path}\nsha256={digest}\nsource_ref={source_ref}\n"
        "stage8b-p1e-i1-owner-loop-handoff: PASS"
    )


if __name__ == "__main__":
    main()
