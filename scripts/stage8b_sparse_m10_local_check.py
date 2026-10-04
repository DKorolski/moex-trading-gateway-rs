#!/usr/bin/env python3
"""Local source/replay checks; no independent or operational acceptance.

No broker/VPS requests, operational Redis, deployment or authority rebind.
Existing integration tests may spawn disposable loopback Redis servers.
Captures actual output, model trace and the bounded linked source witness;
never claims fixed-process operational closure or independent acceptance.
"""
import hashlib
import argparse
import json
import os
from pathlib import Path
import re
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "reports/stage8b-sparse-m10-local"
COMMANDS = [
    [sys.executable, "scripts/stage8b_sparse_m10_fixture_import.py", "--verify-only"],
    ["cargo", "fmt", "--all", "--", "--check"],
    ["cargo", "test", "--offline", "-p", "broker-core", "--lib"],
    ["cargo", "test", "--offline", "-p", "broker-finam", "--lib"],
    ["cargo", "test", "--offline", "-p", "strategy-runtime-core", "--lib",
     "real_four_day_observed_m10_model_replay", "--", "--nocapture"],
    ["cargo", "test", "--offline", "-p", "strategy-runtime-core", "--lib", "--tests"],
    ["cargo", "test", "--offline", "-p", "finam-gateway", "--lib",
     "observed_real_history_fixed_producer_paper_truth_xack_restart", "--", "--nocapture"],
    ["cargo", "test", "--offline", "-p", "finam-gateway", "--lib"],
    ["cargo", "test", "--offline", "-p", "finam-gateway", "--bin", "stage8b-p1f-o2-materializer"],
    ["cargo", "test", "--offline", "-p", "runtime-durable-service", "--lib", "--", "--test-threads=1"],
    ["cargo", "test", "--offline", "-p", "strategy-runtime-core", "-p", "runtime-durable-service", "--doc"],
    ["cargo", "clippy", "--offline", "-p", "broker-finam", "--all-targets", "--", "-D", "warnings"],
    ["cargo", "clippy", "--offline", "-p", "strategy-runtime-core", "--lib", "--bins", "--", "-D", "warnings"],
    ["cargo", "clippy", "--offline", "-p", "finam-gateway", "-p", "runtime-durable-service",
     "--lib", "--bins", "--features", "finam-gateway/stage8b-r2a7-source-adapter", "--", "-D", "warnings"],
    ["git", "diff", "--check"],
]


def sha(data):
    return hashlib.sha256(data).hexdigest()


def inventory():
    files = subprocess.check_output(
        ["git", "ls-files", "--cached", "--others", "--exclude-standard", "-z"], cwd=ROOT,
    ).decode().split("\0")
    return {name: sha((ROOT / name).read_bytes()) for name in sorted(set(files)) if name}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--scope", choices=["full", "receipt-firstboot", "producer-redis", "transaction-seal", "rolling-lifecycle", "fixed-materializer", "fixed-recovery", "journal-ahead", "published-window"], default="full")
    args = parser.parse_args()
    out = OUT if args.scope == "full" else OUT / args.scope
    commands = []
    for command in COMMANDS:
        if args.scope != "full" and command[:6] == ["cargo", "test", "--offline", "-p", "runtime-durable-service", "--lib"]:
            # Targeted admission/legacy first-boot regressions. This does NOT
            # claim a new full Redis/process crash-matrix run.
            names = ["canonical", "observed_", "stage8b_p1e_first_boot_source"]
            if args.scope in ["producer-redis", "transaction-seal", "rolling-lifecycle", "fixed-materializer", "fixed-recovery"]:
                names += ["p1c_", "stage8b_p1f_fixed_redis"]
            if args.scope in ["transaction-seal", "rolling-lifecycle", "fixed-materializer", "fixed-recovery"]:
                names += ["stage8b_p1_bootstrap", "stage8b_p1e_first_boot_transaction"]
            if args.scope in ["rolling-lifecycle", "fixed-materializer", "fixed-recovery"]:
                names += ["p1d2_market_feedback_commits_ack_then_truth_then_xacks_source"]
            if args.scope == "fixed-recovery":
                names += ["stage8b_p1_supervisor", "p1d2_", "p1d3_",
                          "stage8b_p1f_staged_source", "stage8b_p1f_guardian"]
            if args.scope in ["journal-ahead", "published-window"]:
                names += ["journal_ahead", "p1c_", "stage8b_p1_supervisor"]
            for name in names:
                commands.append(command[:6] + [name, "--", "--test-threads=1"])
        else:
            commands.append(command)
        if args.scope in ["fixed-recovery", "full"] and command[:6] == ["cargo", "test", "--offline", "-p", "runtime-durable-service", "--lib"]:
            commands.append(command[:6] + ["--features", "stage8a4-i3-test-fixtures,stage8b-p1-test-fixtures",
                "stage8b_p1e_process", "--", "--test-threads=1"])
        if args.scope in ["journal-ahead", "published-window"] and command[:6] == ["cargo", "test", "--offline", "-p", "runtime-durable-service", "--lib"]:
            commands.append(command[:6] + ["--features", "stage8a4-i3-test-fixtures,stage8b-p1-test-fixtures",
                "p1e_i0_parse_exact_is_transitively_after_permit", "--", "--test-threads=1"])
    out.mkdir(parents=True, exist_ok=True)
    before = inventory()
    env = dict(os.environ, RUST_MIN_STACK="33554432", CARGO_NET_OFFLINE="true")
    for key in list(env):
        if "REDIS" in key:
            env.pop(key)
    records = []
    replay = None
    linked = None
    journal_tests = {
        "observed_journal_ahead_resolves_new_receipt_from_protected_source_digest",
        "observed_journal_ahead_refuses_unbound_source_before_claim",
        "observed_journal_ahead_preset_latch_prevents_callback_and_s1",
    }
    journal_tests_passed = set()
    window_tests = {
        "observed_published_window_is_exact_bounded_and_cannot_rebase_initial",
        "observed_supervisor_published_window_is_atomic_and_exact",
    }
    window_tests_passed = set()
    for i, command in enumerate(commands):
        name = "{:02d}.log".format(i)
        print("RUN " + " ".join(command), flush=True)
        with (out / name).open("wb") as log:
            result = subprocess.run(command, cwd=ROOT, env=env, stdout=log, stderr=subprocess.STDOUT)
        raw = (out / name).read_bytes()
        text = raw.decode(errors="replace")
        if result.returncode == 0:
            journal_tests_passed.update(name for name in journal_tests if
                re.search(r"test [\w:]*::" + name + r" \.\.\. ok(?:\r?\n|$)", text))
            window_tests_passed.update(name for name in window_tests if
                re.search(r"test [\w:]*::" + name + r" \.\.\. ok(?:\r?\n|$)", text))
        records.append({"command": command, "exit_code": result.returncode,
                        "log": name, "sha256": sha(raw),
                        "test_results": re.findall(r"test result: .*", text)})
        print(("PASS " if result.returncode == 0 else "FAIL ") + name, flush=True)
        if result.returncode:
            print(text[-4000:])
            break
        if "real_four_day_observed_m10_model_replay" in command:
            lines = [line for line in text.splitlines() if line.startswith("SPARSE_PARITY_EVIDENCE=")]
            if len(lines) != 1:
                raise RuntimeError("missing or ambiguous runtime replay evidence")
            replay = json.loads(lines[0].split("=", 1)[1])
            (out / "model-replay.json").write_text(json.dumps(replay, indent=2, sort_keys=True) + "\n")
        if "observed_real_history_fixed_producer_paper_truth_xack_restart" in command:
            lines = [line for line in text.splitlines() if line.startswith("SPARSE_LINKED_EVIDENCE=")]
            if len(lines) != 1 or "test result: ok. 1 passed;" not in text:
                raise RuntimeError("missing, ambiguous or unexecuted linked evidence")
            linked = json.loads(lines[0].split("=", 1)[1])
            check_linked_evidence(linked)
            (out / "linked-source-lifecycle.json").write_text(json.dumps(linked, indent=2, sort_keys=True) + "\n")
    unchanged = before == inventory()
    passed = unchanged and replay is not None and linked is not None and len(records) == len(commands) and all(r["exit_code"] == 0 for r in records)
    if args.scope in ["published-window", "full"]:
        passed = passed and window_tests_passed == window_tests and journal_tests_passed == journal_tests
    # Record the actual pinned-authority status, never refresh pins or report
    # source acceptance as governance acceptance. Drift is expected pre-review.
    authority = subprocess.run([sys.executable, "scripts/current_tree_authority_check.py"],
        cwd=ROOT, env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    (out / "current-tree-authority.txt").write_bytes(authority.stdout)
    summary = {
        "scope": args.scope,
        "description": "local observed source/canonical/producer/Redis + fixed materializer V3/source V4 + raw/template replay + staged consumer and transactional guardian + sealed retained-receipt recovery + verify-only S05 context + real History with synthetic current-session controls to paper ACK/truth/XACK/restart + offline model replay",
        "full_durable_regression_run": args.scope == "full",
        "source_inventory": before,
        "source_unchanged_during_check": unchanged,
        "local_checks_passed": passed,
        "commands": records,
        "replay": None if replay is None else {
            "path": "model-replay.json", "sha256": sha((out / "model-replay.json").read_bytes()),
            "warmup_bars": replay["warmup_bars"], "replayed_bars_per_source": len(replay["alor"]["trace"]),
            "input_differences": len(replay["input_differences"]),
            "state_differences": len(replay["state_differences"]),
            "decision_differences": len(replay["decision_differences"]),
            "paper_rounds_equal": replay["paper_rounds_equal"],
            "paper_round_count": len(replay["alor"]["paper_rounds"]),
        },
        "source_correction_complete": False,
        "source_review_gate_passed": passed and args.scope == "full",
        "shared_startup_window_admission_proven": passed and window_tests_passed == window_tests,
        "authority_check_exit_code": authority.returncode,
        "authority_log_sha256": sha(authority.stdout),
        "governance_rebind": "PENDING_SOURCE_ACCEPTANCE",
        "published_window_s05_admission_proven": passed and window_tests_passed == window_tests,
        "published_window_tests_passed": sorted(window_tests_passed),
        "journal_ahead_source_digest_recovery_proven": passed and journal_tests_passed == journal_tests,
        "journal_ahead_tests_passed": sorted(journal_tests_passed),
        "linked_sparse_durable_path_proven": passed,
        "linked": None if linked is None else {
            "path": "linked-source-lifecycle.json",
            "sha256": sha((out / "linked-source-lifecycle.json").read_bytes()),
            "real_history_m10_count": linked["real_history_m10_count"],
            "real_sparse_history_m10_count": linked["real_sparse_history_m10_count"],
            "current_session_controls": linked["current_session_controls"],
            "restart_kind": "clean readmission, not SIGKILL",
            "composition_kind": "existing feature-gated library witness, not installed fixed-process run",
        },
        "initial_receipt_seal_recovery_proven": passed and args.scope in ["transaction-seal", "rolling-lifecycle", "fixed-materializer", "fixed-recovery", "full"],
        "synthetic_sparse_rolling_paper_lifecycle_proven": passed and args.scope in ["rolling-lifecycle", "fixed-materializer", "fixed-recovery", "full"],
        "fixed_materializer_policy_and_retained_snapshot_replay_proven": passed and args.scope in ["fixed-materializer", "fixed-recovery", "full"],
        "sealed_triplet_retained_receipt_recovery_proven": passed and args.scope in ["fixed-recovery", "full"],
        "observed_verify_only_s05_context_proven": passed and args.scope in ["fixed-recovery", "full"],
        "staged_consumer_and_guardian_recovery_proven": passed and args.scope in ["fixed-recovery", "full"],
        "fixed_sparse_process_end_to_end_proven": False,
        "live_observed_http_collection_proven": False,
        "fresh_truth_renewal_or_continuous_loop_proven": False,
        "independent_acceptance": "NOT_CLAIMED",
        "o2_verdict": "HOLD", "operational_activation": False,
        "remaining": ["independent source review and governance binding after acceptance",
                      "artifact/installation gates; no automatic rolling-source discovery or new CLI input",
                      "provider source-semantics confirmation and separately authorized bounded O2"],
    }
    (out / "progress-evidence.json").write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n")
    print("LOCAL_CHECKS={}; SOURCE_CORRECTION=IN_PROGRESS; O2=HOLD".format("PASS" if passed else "FAIL"))
    return 0 if passed else 1


def check_linked_evidence(linked):
    """Require measured postconditions, not just the presence of a log marker."""
    composition = linked["composition"]
    if not (
        type(linked["real_history_m10_count"]) is int
        and linked["real_history_m10_count"] == 404
        and linked["real_sparse_history_m10_count"] == 18
        and linked["producer_sequence_after_disk_restore"] == 2
        and set(linked["overlap_negatives"]) == {
            "changed_volume", "changed_prices", "lost_minute", "added_minute"}
        and len(set(linked["independently_admitted_receipt_hashes"])) == 2
        and all(re.fullmatch(r"[0-9a-f]{64}", h) for h in linked["independently_admitted_receipt_hashes"])
        and all(composition[key] is True for key in [
            "bytes_mismatch_rejected", "durable_truth_committed", "source_xack_last",
            "readmission_already_acknowledged", "duplicate_command_absent"])
        and composition["command_stream_length_before_restart"] == 1
        and composition["command_stream_length_after_restart"] == 1
        and composition["retained_m10_stream_length"] == 2
        and composition["final_m10_pel_count"] == 0
        and composition["resource_poll_m10_pel_count"] == 1
    ):
        raise RuntimeError("linked source witness postconditions failed")


if __name__ == "__main__":
    sys.exit(main())
