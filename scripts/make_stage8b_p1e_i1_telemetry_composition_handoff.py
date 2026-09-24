#!/usr/bin/env python3
"""Create the immutable I1 production telemetry correction handoff."""

from __future__ import annotations

import json
import zipfile
from pathlib import Path

import make_stage8b_design_handoff as common
import make_stage8b_p1e_i1_owner_loop_handoff as runner
import stage8b_p1e_i1_telemetry_composition_check as telemetry_check
import stage8b_p1e_i1_telemetry_composition_handoff_safety_check as safety


ROOT = runner.ROOT
OUTPUT = runner.OUTPUT
REVIEW_SOURCE = Path("/Users/denisq/Downloads/FINAM_I1_TELEMETRY_REVIEW_22ad2d5_2026-09-24.md")


def main() -> None:
    status = runner.git("status", "--porcelain", "--untracked-files=all").decode().strip()
    if status:
        raise SystemExit(f"stage8b-p1e-i1-telemetry-handoff: FAIL dirty worktree\n{status}")
    branch = runner.git("branch", "--show-current").decode().strip()
    if branch != safety.BRANCH:
        raise SystemExit(f"stage8b-p1e-i1-telemetry-handoff: FAIL branch={branch}")
    source_ref = runner.git("rev-parse", "HEAD").decode().strip()
    source_short_ref = source_ref[:7]
    source_parent = runner.git("rev-parse", "HEAD^").decode().strip()
    source_tree = runner.git("rev-parse", "HEAD^{tree}").decode().strip()
    if source_parent != safety.PARENT:
        raise SystemExit(
            f"stage8b-p1e-i1-telemetry-handoff: FAIL parent={source_parent} expected={safety.PARENT}"
        )
    changed_paths = runner.git(
        "diff", "--name-only", telemetry_check.BASE, source_ref, "--"
    ).decode().splitlines()
    if set(changed_paths) != telemetry_check.ALLOWED_CHANGES:
        raise SystemExit("stage8b-p1e-i1-telemetry-handoff: FAIL changed-path inventory")
    review_raw = REVIEW_SOURCE.read_bytes()
    if runner.sha256(review_raw) != safety.REVIEW_SHA256:
        raise SystemExit("stage8b-p1e-i1-telemetry-handoff: FAIL predecessor review digest")

    runner.safety = safety
    specifications = [
        ("source_gate", ["bash", "scripts/stage8b_p1e_i1_telemetry_composition_gate.sh"], None),
        (
            "runtime_process",
            [
                "cargo", "test", "-p", "runtime-durable-service", "--all-features",
                "stage8b_p1e_process::tests::", "--", "--test-threads=1",
            ],
            {"RUST_MIN_STACK": "33554432"},
        ),
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
            {"RUST_MIN_STACK": "33554432"},
        ),
    ]
    records = {}
    logs = {}
    for name, command, environment in specifications:
        record, log = runner.run_check(name, command, source_ref, source_tree, environment)
        records[name] = record
        logs[safety.LOGS[name]] = log

    if runner.git("rev-parse", "HEAD").decode().strip() != source_ref:
        raise SystemExit("stage8b-p1e-i1-telemetry-handoff: FAIL source ref changed")
    if runner.git("rev-parse", "HEAD^{tree}").decode().strip() != source_tree:
        raise SystemExit("stage8b-p1e-i1-telemetry-handoff: FAIL source tree changed")
    if runner.git("status", "--porcelain", "--untracked-files=all").decode().strip():
        raise SystemExit("stage8b-p1e-i1-telemetry-handoff: FAIL worktree changed during verification")

    archive_name = (
        f"moex-trading-project-{source_short_ref}-"
        "stage8b-p1e-i1-telemetry-correction.zip"
    )
    archive_path = OUTPUT / archive_name
    manifest_raw, entries = common.source_manifest(source_ref)
    evidence = {
        "schema_version": 1,
        "stage": safety.STAGE,
        "status": "SOURCE_CORRECTION_REVIEW_CANDIDATE_I1_NOT_CLOSED",
        "source_ref": source_ref,
        "source_parent": source_parent,
        "source_tree": source_tree,
        "branch": branch,
        "archive_name": archive_name,
        "manifest_sha256": runner.sha256(manifest_raw),
        "changed_paths": changed_paths,
        "commands": records,
        "telemetry_evidence": {
            "write_only_publisher": True,
            "real_durable_seal_binding": True,
            "real_boot_and_operational_identity": True,
            "starting_paper_ready_degraded_draining_stopped": True,
            "immediate_and_periodic": True,
            "bounded_fifo_fail_closed": True,
            "nomkstream_exact_maxlen_4096": True,
            "missing_stream_no_create": True,
            "first_wins_failure_precedence": True,
            "active_telemetry_intent_supervision": True,
            "telemetry_task_panic_and_early_completion_supervised": True,
            "single_retained_deadline": True,
            "exit_71_before_deadline_and_72_at_expiry": True,
            "latch_reconciled_at_publication": True,
            "source_poll_freshness_expires": True,
            "queued_ready_cannot_restore_paper_ready": True,
            "inflight_poll_draining_heartbeat": True,
            "authenticated_semantic_and_ack_timestamps": True,
            "ack_truth_xack_seal_and_pel_tracking": True,
            "retained_terminal_snapshot": True,
            "typed_blocked_inventory": True,
            "production_child_sigterm_exit_zero": True,
            "durable_root_unchanged": True,
            "m10_pel_empty": True,
        },
        "i1_closed": False,
        "next_slice": "fixed-path installation and systemd material",
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
        "correction_review_sha256": safety.REVIEW_SHA256,
    }
    evidence_raw = (json.dumps(evidence, indent=2, sort_keys=True) + "\n").encode()
    marker = (
        f"stage={safety.STAGE}\nsource_short_ref={source_short_ref}\nsource_ref={source_ref}\n"
        f"source_parent={source_parent}\nsource_tree={source_tree}\n"
        f"branch={branch}\narchive_name={archive_name}\n"
    ).encode()
    additions = {
        safety.MARKER: marker,
        safety.MANIFEST: manifest_raw,
        safety.COMMIT_RAW: runner.git("cat-file", "commit", source_ref),
        safety.EVIDENCE: evidence_raw,
        safety.REVIEW: review_raw,
        **logs,
    }

    OUTPUT.mkdir(parents=True, exist_ok=True)
    archive_path.unlink(missing_ok=True)
    with zipfile.ZipFile(archive_path, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
        for entry in entries:
            archive.writestr(
                common.zip_info(entry["path"], entry["mode"]),
                runner.git("show", f"{source_ref}:{entry['path']}"),
            )
        for name, raw in sorted(additions.items()):
            archive.writestr(common.zip_info(name), raw)

    result = safety.check(str(archive_path))
    digest = runner.sha256(archive_path.read_bytes())
    archive_path.with_suffix(".zip.sha256").write_text(f"{digest}  {archive_name}\n")
    archive_path.with_suffix(".zip.safety.json").write_text(
        json.dumps(result, indent=2, sort_keys=True) + "\n"
    )
    print(
        f"archive={archive_path}\nsha256={digest}\nsource_ref={source_ref}\n"
        "stage8b-p1e-i1-telemetry-handoff: PASS"
    )


if __name__ == "__main__":
    main()
