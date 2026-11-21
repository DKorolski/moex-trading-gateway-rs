#!/usr/bin/env python3
"""Local O3 component evidence only; no keys, SSH, installation or activation.

The linked witness is fixture-signed. This must NOT be labelled the requested
production-executable O3 proof or used as authority for a VPS launch.
"""
import argparse
import csv
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess


ROOT = Path(__file__).resolve().parents[1]
TESTS = [
    ("finam-gateway", "stage8b_p1f_o3_composition::shutdown_tests::o3_shutdown_handoff_preserves_first_cause_and_deadline"),
    ("runtime-durable-service", "stage8b_p1e_process::tests::o3_retained_stop_rejects_before_admission_and_preserves_intent"),
    ("runtime-durable-service", "stage8b_p1e_process::tests::process_supervisor_observes_telemetry_intent_without_os_signal"),
    ("runtime-durable-service", "stage8b_p1e_process::tests::process_supervisor_uses_retained_telemetry_deadline_without_extension"),
    ("runtime-durable-service", "stage8b_p1e_process::tests::process_wrapper_keeps_owner_panic_fatal_precedence"),
    ("runtime-durable-service", "stage8b_p1e_process::tests::o3_post_market_successor_preserves_position_and_commits_before_xack"),
    ("finam-gateway", "stage8b_p1f_fixed_producers::tests::observed_linked::observed_synthetic_sparse_history_fractional_source_paper_restart"),
    ("finam-gateway", "stage8b_p1f_fixed_producers::tests::observed_linked::observed_flat_local_history_earliest_bo_window_paper_truth_xack_restart"),
    ("finam-gateway", "stage8b_p1f_o3_composition::authority::tests::o3_authority_wait_requires_fresh_active_and_retries_only_lock_contention"),
    ("finam-gateway", "stage8b_p1f_o3_composition::producers::tests::o3_schedule_checkpoint_custody_rejects_missing_links_modes_fifo_and_oversize"),
    ("runtime-durable-service", "stage8b_p1_supervisor::tests::o3_fresh_inventory_uses_accepted_groups_and_refuses_reinitialization"),
    ("runtime-durable-service", "stage8b_p1_supervisor::tests::o3_fresh_inventory_rejects_partial_or_foreign_stream_without_writes"),
    ("runtime-durable-service", "stage8b_p1_supervisor::tests::o3_provisioning_rejects_manifest_or_schedule_before_redis"),
    ("finam-gateway", "stage8b_p1f_o3_signer::service::tests::o3_credential_custody_rejects_permissions_links_fifo_and_malformed_key"),
    ("finam-gateway", "stage8b_p1f_o3_composition::inputs::tests::o3_fixed_inputs_bind_signed_phase_plan_config_identity_and_window"),
    ("finam-gateway", "stage8b_p1f_o3_composition::inputs::tests::o3_synthetic_schedule_uses_ordinary_report_and_stable_semantic_pin"),
    ("runtime-durable-service", "stage8b_p1f_guardian::tests::o3_claim_is_phase_specific_and_preserves_existing_terminal_history"),
    ("finam-gateway", "stage8b_p1f_fixed_producers::tests::observed::observed_o3_retained_window_rejects_missing_conflicting_and_unpublished_state"),
    ("finam-gateway", "stage8b_p1f_o3_signer::tests::o3_signer_policy_rejects_raw_digest_foreign_identity_stale_and_bad_report"),
    ("finam-gateway", "stage8b_p1f_o3_signer::tests::o3_signer_unix_framing_is_bounded_and_does_not_accept_digest_requests"),
    ("finam-gateway", "stage8b_p1f_fixed_producers::tests::observed_linked::observed_real_history_fixed_producer_paper_truth_xack_restart"),
    ("finam-gateway", "stage8b_p1f_fixed_producers::tests::observed::observed_producer_strict_version_binding_schedule_and_pending_lineage"),
    ("finam-gateway", "stage8b_p1f_fixed_producers::tests::observed::observed_producer_real_redis_response_loss_exact_replay_and_o3_o4_sequence"),
    ("finam-gateway", "stage8b_p1f_fixed_producers::tests::observed::observed_producer_corrupt_retained_receipt_blocks_prepared_and_redis_effect"),
    ("finam-gateway", "stage8b_p1e_schedule_publisher::tests::signer_output_is_cryptographically_bound_to_the_exact_digest"),
    ("finam-gateway", "stage8b_p1e_schedule_publisher::tests::prepared_state_survives_response_loss_and_replays_exact_signed_bytes"),
    ("runtime-durable-service", "stage8b_p1f_guardian::tests::local_supervision_starts_after_admission_and_stops_on_sigterm"),
    ("runtime-durable-service", "stage8b_p1e_process::tests::owner_loop_routes_committed_target_first_cancel_to_xack_last"),
]


def digest(data):
    return hashlib.sha256(data).hexdigest()


def snapshot():
    names = subprocess.check_output(
        ["git", "ls-files", "--cached", "--others", "--exclude-standard", "-z"], cwd=ROOT
    ).decode().split("\0")
    return {name: digest((ROOT / name).read_bytes()) for name in sorted(set(names)) if name}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path, help="new local evidence directory; never an installation")
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=False)
    source_before = snapshot()
    (args.output / "checked-source.json").write_text(json.dumps(source_before, indent=2) + "\n")
    cargo = shutil.which("cargo")
    if not cargo:
        raise SystemExit("cargo is required")
    env = dict(os.environ, RUST_MIN_STACK="33554432")
    records = []

    def run(name, command, selector=None, timeout=600):
        print(f"RUN {name}", flush=True)
        try:
            result = subprocess.run(command, cwd=ROOT, env=env, stdout=subprocess.PIPE,
                                    stderr=subprocess.STDOUT, timeout=timeout, check=False)
            output, code = result.stdout, result.returncode
        except subprocess.TimeoutExpired as exc:
            output, code = (exc.stdout or b"") + b"\nLOCAL_GATE_TIMEOUT\n", 124
        logfile = args.output / f"{name}.log"
        logfile.write_bytes(output)
        exact = selector is None or (
            # With nocapture + one test thread, test output may occur between
            # the libtest name prefix and its terminal `ok` line.
            f"test {selector} ...".encode() in output
            and re.search(rb"test result: ok\. 1 passed; 0 failed; 0 ignored;", output) is not None
        )
        passed = code == 0 and exact
        records.append({"id": name, "command": command, "selector": selector,
                        "exit_code": code, "exact_selector_pass": exact,
                        "log": logfile.name, "log_sha256": digest(output), "passed": passed})
        print(f"{'PASS' if passed else 'FAIL'} {name}", flush=True)
        return passed

    success = run("fmt", [cargo, "fmt", "--all", "--check"])
    for number, (package, selector) in enumerate(TESTS, 1):
        success = run(f"test-{number:02d}", [cargo, "test", "--offline", "-p", package,
                      "--all-features", "--lib", selector, "--", "--exact", "--nocapture",
                      "--test-threads=1"], selector) and success
    success = run("production-binaries", [cargo, "build", "--offline", "-p", "finam-gateway",
                  "--no-default-features", "--bin", "stage8b-p1f-o3-composition",
                  "--bin", "stage8b-p1f-o3-signer", "--bin", "stage8b-p1f-o3-provisioner"], timeout=1200) and success
    success = run("clippy", [cargo, "clippy", "--offline", "-p", "finam-gateway",
                  "-p", "runtime-durable-service", "--all-features", "--all-targets",
                  "--", "-D", "warnings"], timeout=1200) and success
    success = run("diff-check", ["git", "diff", "--check"]) and success
    with (ROOT / "docs/stage-8/stage8b-p1f-o3-acceptance-matrix.csv").open() as file:
        rows = list(csv.DictReader(file))
    matrix_valid = (len(rows) == 13 and all(
        row["status"] == "NOT_RUN" and row["local_coverage_limit"] and None not in row for row in rows
    ) and rows[11]["local_witness"] == "owner_loop_routes_committed_target_first_cancel_to_xack_last")
    unchanged = source_before == snapshot()
    success = success and unchanged and matrix_valid
    evidence = {"schema_version": 1, "scope": "O3_LOCAL_COMPONENT_REGRESSIONS_ONLY",
                "base_commit": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT).decode().strip(),
                "working_tree": subprocess.check_output(["git", "status", "--short"], cwd=ROOT).decode(),
                "source_unchanged_during_checks": unchanged, "matrix_honesty_check": matrix_valid,
                "fixture_signer_used_in_local_tests": True,
                "production_executable_linked_proof": False, "production_schedule_key_used": False,
                "post_market_continuation_regression": True,
                "component_regression_is_positive_o3_proof": False,
                "vps_mutated": False, "operational_o3_ready": False,
                "records": records, "all_component_checks_passed": success}
    (args.output / "result.json").write_text(json.dumps(evidence, indent=2) + "\n")
    print("COMPONENT_REGRESSIONS_PASS" if success else "COMPONENT_REGRESSIONS_FAIL", flush=True)
    raise SystemExit(0 if success else 1)


if __name__ == "__main__":
    main()
