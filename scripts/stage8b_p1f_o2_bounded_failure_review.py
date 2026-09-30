#!/usr/bin/env python3
"""Local O2 bounded-failure source gate; reuse the existing immutable ZIP verifier."""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import subprocess

import current_tree_authority_check as authority
import stage8b_p1f_o2_recovery_review as handoff

# Configure only the shared source-package marker. Historical recovery gates
# and their accepted matrices are not modified or reinterpreted.
BASE = "60bc4821dd72126d0b981cc86810c9eb8611cf33"
handoff.BASE = BASE
ROOT = handoff.ROOT
PRODUCTION = {
    "crates/finam-gateway/src/bin/stage8b-p1f-o2-materializer.rs",
    "crates/finam-gateway/src/stage8b_p1f_o2_materializer.rs",
    "crates/runtime-durable-service/src/lib.rs",
    "crates/runtime-durable-service/src/stage8b_p1f_guardian.rs",
    "crates/runtime-durable-service/src/stage8b_p1f_o2_systemd.rs",
}
ALLOWED = PRODUCTION | {
    "deploy/stage8b-p1e/moex-finam-p1f-o2-materializer.service",
    "docs/current-status.md", "docs/roadmap.md",
    "docs/stage-8/stage8b-p1f-o2-bounded-failure-correction.md",
    "scripts/stage8b_p1f_o2_bounded_failure_review.py",
}
COMMANDS = [
    ("fmt", ["cargo", "fmt", "--all", "--", "--check"]),
    ("finam-debug", ["cargo", "test", "--locked", "-p", "finam-gateway", "--all-targets", "--", "--test-threads=1"]),
    ("durable-debug", ["cargo", "test", "--locked", "-p", "runtime-durable-service", "--all-targets", "--all-features", "--", "--test-threads=1"]),
    ("materializer-release", ["cargo", "test", "--locked", "--release", "-p", "finam-gateway", "--all-features", "--lib", "stage8b_p1f_o2_materializer::tests::", "--", "--test-threads=1"]),
    ("cleanup-release", ["cargo", "test", "--locked", "--release", "-p", "runtime-durable-service", "--all-features", "--lib", "o2_", "--", "--test-threads=1"]),
    ("o2-regression", ["bash", "scripts/stage8b_p1f_o2_gate.sh"]),
    ("alor-regression", ["bash", "scripts/stage8b_p1f_alor_finam_source_correction_gate.sh"]),
    ("doctests", ["cargo", "test", "--locked", "-p", "finam-gateway", "-p", "runtime-durable-service", "--all-features", "--doc"]),
    ("clippy", ["cargo", "clippy", "--locked", "-p", "finam-gateway", "-p", "runtime-durable-service", "--all-targets", "--all-features", "--", "-D", "warnings"]),
    ("diff", ["git", "diff", "--check", BASE]),
]


def gate(output: Path) -> None:
    ref = handoff.clean_ref()
    changed = set(handoff.git("diff", "--name-only", BASE, ref).decode().splitlines())
    handoff.require(PRODUCTION <= changed <= ALLOWED, "unexpected source scope")
    pinned = json.loads((ROOT / authority.AUTHORITY).read_text())["production_code_manifest"]["entries"]
    actual = authority.file_inventory(ROOT, authority.production_files(ROOT))
    drift = {p for p in set(actual) | set(pinned) if actual.get(p) != pinned.get(p)}
    handoff.require(drift == PRODUCTION, "unexpected production authority drift")
    output.mkdir(parents=True, exist_ok=False)
    env = dict(os.environ, RUST_MIN_STACK="33554432", CARGO_TERM_COLOR="never")
    logs = {}
    # Reproduce only the observed pre-existing feature/test incompatibility,
    # not a fresh audit of accepted history. No production fix or test skipping.
    baseline = ROOT / "tmp/o2-bounded-failure-baseline"
    handoff.require(subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=baseline).decode().strip() == BASE,
                    "prepare exact detached baseline worktree at tmp/o2-bounded-failure-baseline")
    handoff.require(not subprocess.check_output(["git", "status", "--porcelain"], cwd=baseline).strip(), "dirty baseline")
    known_command = ["cargo", "test", "--locked", "-p", "finam-gateway", "--all-features", "--lib",
                     "tests::endpoint_gate_marker_cannot_be_forged_from_manual_decision", "--", "--exact"]
    print("RUN baseline all-features compatibility diagnostic (expected FAIL, not an acceptance gate)", flush=True)
    path = output / "baseline-all-features-known-failure.txt"
    with path.open("wb") as stream:
        result = subprocess.run(known_command, cwd=baseline,
                                env=dict(env, CARGO_TARGET_DIR=str(baseline / "target")),
                                stdout=stream, stderr=subprocess.STDOUT)
    raw = path.read_bytes()
    handoff.require(result.returncode == 101 and b"manual decision must not forge endpoint approval" in raw
                    and b"0 passed; 1 failed" in raw, "baseline incompatibility not reproduced")
    logs[path.name] = handoff.digest(raw)
    for name, command in COMMANDS:
        print(f"RUN {name}: {' '.join(command)}", flush=True)
        path = output / f"{name}.txt"
        with path.open("wb") as stream:
            result = subprocess.run(command, cwd=ROOT, env=env, stdout=stream, stderr=subprocess.STDOUT)
        handoff.require(result.returncode == 0, f"FAIL {name}; see {path}")
        raw = path.read_bytes()
        if name == "materializer-release":
            handoff.require(b"5 passed; 0 failed" in raw, "diagnostic tests not executed")
        if name == "cleanup-release":
            handoff.require(b"16 passed; 0 failed" in raw and b"pending_terminal_durable_frontiers" in raw,
                            "cleanup regressions not executed")
        logs[path.name] = handoff.digest(raw)
        print(f"PASS {name}", flush=True)
    result = subprocess.run(["python3", "scripts/current_tree_authority_check.py"], cwd=ROOT,
                            stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    handoff.require(result.returncode != 0 and b"production entry drift" in result.stdout,
                    "unexpected authority result; rebind must remain separate")
    (output / "authority-pending.txt").write_bytes(result.stdout)
    logs["authority-pending.txt"] = handoff.digest(result.stdout)
    handoff.require(handoff.clean_ref() == ref, "source changed during gate")
    summary = {
        "stage": "Stage 8B-P1-f O2 bounded-failure source correction",
        "status": "SOURCE_REVIEW_PENDING", "source_gate_passed": True,
        "source_ref": ref, "source_tree": handoff.git("rev-parse", "HEAD^{tree}").decode().strip(),
        "baseline_ref": BASE, "commands": COMMANDS, "logs_sha256": logs,
        "findings": ["P1-O2REC01", "P2-O2DIAG01"],
        "known_baseline_failure": {"command": known_command, "baseline_ref": BASE,
                                   "exit_code": 101, "feature": "m3j16-actual-one-shot",
                                   "test": "endpoint_gate_marker_cannot_be_forged_from_manual_decision"},
        "authority_status": "ACCEPTED_BASELINE_REBIND_PENDING", "authority_drift_paths": sorted(drift),
        "merge_ready": False, "execution_authorized": False, "vps_contacted": False,
        "finam_contacted": False, "operational_redis_activated": False,
        "old_phase_state": "ACTIVE_IN_RETAINED_EVIDENCE_NO_NEW_OBSERVATION",
        "limitations": ["real local store, substituted systemd observations and clock",
                        "simulated durable terminal frontiers, no new SIGKILL matrix",
                        "macOS source tests, native Linux mounts/delivery not executed",
                        "full FINAM all-features conflicts with legacy one-shot gate test on unchanged baseline; default-feature suite used as in prior accepted source gate",
                        "old BarsTruth root cause unknown; no fabricated raw response evidence"],
    }
    (output / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    print(f"PASS source gate; independent review and authority rebind pending: {ref}", flush=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=["gate", "package", "check"])
    parser.add_argument("path", type=Path)
    args = parser.parse_args()
    if args.action == "gate":
        gate(args.path.resolve())
    elif args.action == "package":
        handoff.package(args.path.resolve())
    else:
        print(json.dumps(handoff.check_archive(args.path), indent=2))
