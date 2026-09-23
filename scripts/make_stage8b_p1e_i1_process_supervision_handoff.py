#!/usr/bin/env python3
"""Create the immutable I1 process-supervision review handoff."""

from __future__ import annotations

import json
import zipfile

import make_stage8b_design_handoff as common
import make_stage8b_p1e_i1_owner_loop_handoff as runner
import stage8b_p1e_i1_process_supervision_handoff_safety_check as safety


ROOT = runner.ROOT
OUTPUT = runner.OUTPUT


def main() -> None:
    status = runner.git("status", "--porcelain", "--untracked-files=all").decode().strip()
    if status:
        raise SystemExit(f"stage8b-p1e-i1-process-handoff: FAIL dirty worktree\n{status}")
    branch = runner.git("branch", "--show-current").decode().strip()
    if branch != safety.BRANCH:
        raise SystemExit(f"stage8b-p1e-i1-process-handoff: FAIL branch={branch}")
    source_ref = runner.git("rev-parse", "HEAD").decode().strip()
    source_short_ref = source_ref[:7]
    source_parent = runner.git("rev-parse", "HEAD^").decode().strip()
    source_tree = runner.git("rev-parse", "HEAD^{tree}").decode().strip()
    if source_parent != safety.PARENT:
        raise SystemExit(
            f"stage8b-p1e-i1-process-handoff: FAIL parent={source_parent} expected={safety.PARENT}"
        )

    runner.safety = safety
    specifications = [
        ("source_gate", ["bash", "scripts/stage8b_p1e_i1_process_supervision_gate.sh"], None),
        (
            "runtime_process",
            [
                "cargo", "test", "-p", "runtime-durable-service", "--all-features", "--lib",
                "stage8b_p1e_process::tests::os_process_", "--", "--test-threads=1",
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
        ("runtime_doc", ["cargo", "test", "-p", "runtime-durable-service", "--doc", "--all-features"], None),
        ("core_full", ["cargo", "test", "-p", "strategy-runtime-core", "--all-features"], None),
    ]
    records = {}
    logs = {}
    for name, command, environment in specifications:
        record, log = runner.run_check(name, command, source_ref, source_tree, environment)
        records[name] = record
        logs[safety.LOGS[name]] = log

    require_clean = runner.git("status", "--porcelain", "--untracked-files=all").decode().strip()
    if runner.git("rev-parse", "HEAD").decode().strip() != source_ref or require_clean:
        raise SystemExit("stage8b-p1e-i1-process-handoff: FAIL source changed during verification")

    archive_name = f"moex-trading-project-{source_short_ref}-stage8b-p1e-i1-process-supervision.zip"
    archive_path = OUTPUT / archive_name
    manifest_raw, entries = common.source_manifest(source_ref)
    evidence = {
        "schema_version": 1,
        "stage": safety.STAGE,
        "status": "SOURCE_CORRECTION_REVIEW_CANDIDATE",
        "source_ref": source_ref,
        "source_parent": source_parent,
        "source_tree": source_tree,
        "branch": branch,
        "archive_name": archive_name,
        "manifest_sha256": runner.sha256(manifest_raw),
        "commands": records,
        "production_process_composition": True,
        "process_matrix": {
            "case_count": 14,
            "idle_sigterm": True,
            "idle_sigkill_restart": True,
            "owner_panic_exit_70": True,
            "committed_cancel_handoff": True,
            "cancel_truth_before_xack_sigkill_restart": True,
            "production_pre_admission_sigterm_exit_66": True,
            "production_post_admission_sigterm_exit_0": True,
            "production_before_redis_attach_sigint_exit_0": True,
            "production_before_s06_sigterm_exit_0": True,
            "production_inflight_redis_attach_sigterm_exit_0": True,
            "production_inflight_s06_sigint_exit_0": True,
            "common_supervisor_noncooperative_owner_grace_exit_72": True,
            "production_v5_signed_schedule_v4_market_restart_readmission": True,
            "unexpected_authenticated_stop_exit_70": True,
        },
        "ordinary_run_admission": {
            "post_seal_frontiers_rejected_without_mutation": True,
            "missing_corrupt_foreign_authority_rejected_without_mutation": True,
            "adopted_v5_root_accepted": True,
            "authority_temp_files_rejected": True,
            "current_restart_package_provenance_bound": True,
            "advanced_v5_root_readmitted": True,
            "initial_marker_and_receipt_immutable": True,
            "restart_does_not_repeat_command_publication": True,
            "committed_v4_root_readmitted": True,
            "post_truth_root_readmitted": True,
        },
        "terminal_exit_mapping": {
            "authenticated_before_deadline": 0,
            "authenticated_at_or_after_deadline": 72,
            "unexpected_authenticated_stop_without_intent": 70,
            "signal_task_failure": 73,
            "owner_panic_precedence": 70,
            "restart_required": 67,
        },
        "corrected_predecessor_lifecycle": {
            "fresh_v5_export": "P1BootstrapReady",
            "historical_v5_export": "P1BootstrapReady",
            "authenticated_adoption_phase": "P1SemanticReady",
            "adoption_predicate_version": 2,
            "legacy_predicate_v1_timer_ready_rejected": True,
            "legacy_automatic_migration": False,
            "continuous_v5_market_witness_complete": True,
            "continuous_v5_witness_uses_intent_injection_or_redis_reset": False,
            "cancel_production_ingress": "authenticated durable restart outcome",
            "isolated_injected_cancel_fixture_counted_as_continuous": False,
        },
        "signed_market_authority": {
            "fixture_signed_envelope": True,
            "production_schedule_reader": True,
            "fresh_schedule_read_total": 1,
            "v4_restart_before_effect": True,
            "exact_publication_marker_revalidated": True,
            "committed_recovery_schedule_read_total": 0,
            "post_truth_already_acknowledged": True,
            "initial_adoption_marker_receipt_immutable": True,
            "legacy_v4_none_authority_used": False,
        },
        "closed_surfaces": {
            "operational_redis_db0_db15": False,
            "vps_activation": False,
            "paper_provider_activation": False,
            "finam_post_delete_send": False,
            "broker_dispatch": False,
            "runtime_live": False,
            "real_orders": False,
        },
        "next_slice": "independent process-supervision acceptance, then separate aggregate I1 closure",
    }
    evidence_raw = (json.dumps(evidence, indent=2, sort_keys=True) + "\n").encode()
    marker = (
        f"stage={safety.STAGE}\n"
        f"source_short_ref={source_short_ref}\nsource_ref={source_ref}\n"
        f"source_parent={source_parent}\nsource_tree={source_tree}\n"
        f"branch={branch}\narchive_name={archive_name}\n"
    ).encode()
    additions = {
        safety.MARKER: marker,
        safety.MANIFEST: manifest_raw,
        safety.COMMIT_RAW: runner.git("cat-file", "commit", source_ref),
        safety.EVIDENCE: evidence_raw,
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
        "stage8b-p1e-i1-process-handoff: PASS"
    )


if __name__ == "__main__":
    main()
