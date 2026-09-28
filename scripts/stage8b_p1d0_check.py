#!/usr/bin/env python3
"""Fail-closed design/scope checker for the Stage 8B-P1-d0 policy freeze."""

from __future__ import annotations

import csv
import json
import pathlib
import subprocess
import sys


ROOT = pathlib.Path(__file__).resolve().parents[1]
BASE = "3d08f84a4a01d08265120def697584c3e60bcd3c"
POLICY = ROOT / "docs/stage-8/stage8b-p1d0-deterministic-paper-execution-policy.md"
MATRIX = ROOT / "docs/stage-8/stage8b-p1d0-acceptance-matrix.csv"
EVIDENCE = ROOT / "docs/stage-8/stage8b-p1d0-evidence.json"
STATUS = ROOT / "docs/current-status.md"
ROADMAP = ROOT / "docs/roadmap.md"
EXPECTED_CHANGED = {
    "docs/current-status.md",
    "docs/roadmap.md",
    "docs/stage-8/stage8b-p1d0-acceptance-matrix.csv",
    "docs/stage-8/stage8b-p1d0-deterministic-paper-execution-policy.md",
    "docs/stage-8/stage8b-p1d0-evidence.json",
    "scripts/make_stage8b_p1d0_handoff.py",
    "scripts/stage8b_p1d0_check.py",
    "scripts/stage8b_p1d0_gate.sh",
    "scripts/stage8b_p1d0_handoff_safety_check.py",
    "scripts/stage8b_p1d0_negative_harness.py",
}


class CheckFailure(RuntimeError):
    pass


def require(condition: bool, message: str) -> None:
    if not condition:
        raise CheckFailure(message)


def changed_files() -> set[str]:
    tracked = subprocess.run(
        ["git", "diff", "--name-only", BASE],
        cwd=ROOT,
        check=True,
        text=True,
        capture_output=True,
    ).stdout.splitlines()
    untracked = subprocess.run(
        ["git", "ls-files", "--others", "--exclude-standard"],
        cwd=ROOT,
        check=True,
        text=True,
        capture_output=True,
    ).stdout.splitlines()
    return set(tracked) | set(untracked)


def validate(
    policy: str,
    matrix_text: str,
    evidence: dict[str, object],
    status: str,
    roadmap: str,
) -> None:
    required_policy = (
        "moex.stage8b.p1d.execution-policy.v1",
        "Stage 7B remains the sole durable command-lifecycle authority",
        "That command is never evaluated against `B[n]`.",
        "ExecutionBarGap",
        "Stage5eScheduleSequenceClassification::Contiguous",
        "Stage5eScheduleSequenceClassification::ApprovedNonTradableBoundary",
        "Redis composition and the paper\nprovider must not parse calendars",
        "Cross-session Day-order\nexpiry is resolved only by the accepted schedule owner",
        "History and warmup bars never execute operational paper commands.",
        "Fill-before-cancel is the frozen chronology for that case.",
        "fail-closed execution-eligibility preflight before that transition",
        "DispatchAttemptRecorded   forbidden",
        "paper provider call       forbidden",
        "observed read-only from the same canonical M10",
        "must not use `XREADGROUP`, add the bar to a PEL, XACK it or invoke the Hybrid",
        "advances the Hybrid callback exactly once",
        "A crash after that record but before durable outcome remains reconciliation",
        "It must not blindly call the\nprovider again.",
        "fill_price = execution_bar.open",
        "fill_price = min(bar.open, limit)",
        "fill_price = max(bar.open, limit)",
        "Synthetic partial fills are disabled in policy v1.",
        "market slippage adjustment = 0 ticks",
        "commission                 = 0",
        '"P1D-O-" + hex(sha256(',
        '"P1D-T-" + hex(sha256(',
        "Paper IDs are derived from immutable durable identity, never from Redis IDs,",
        "active Working target -> `CancelCanceled`",
        "already Filled target -> `CancelExecutionObserved`",
        "one opaque, linear P1-d outcome bundle",
        "flat sets `avg_price = None`",
        "XACK is always last.",
        "P1-d0 acceptance authorizes only P1-d1 source implementation.",
        "The current-tree authority intentionally remains pinned to accepted P1-c",
        "rebound only after the corresponding P1-d source implementation is\nindependently accepted.",
        "operational Redis DB 0 activation;",
        "FINAM HTTP POST/DELETE;",
        "runtime-live / `LiveReady`;",
    )
    for token in required_policy:
        require(token in policy, f"missing policy invariant: {token}")
    require(
        "P1-d0 is design-only" in policy
        and "production implementations remain byte-identical" in policy,
        "design-only source boundary is not explicit",
    )
    require(
        "Stop, stop-limit, take-profit, replace, bracket, multi-leg" in policy,
        "unsupported command surface is incomplete",
    )
    require(
        "PlaceOrder` with `OrderType::Market` and `TimeInForce::Day`" in policy
        and "PlaceOrder` with `OrderType::Limit` and `TimeInForce::Day`" in policy,
        "Day-only market/limit surface drifted",
    )

    rows = list(csv.DictReader(matrix_text.splitlines()))
    expected_ids = {f"P1D0-{index:03d}" for index in range(1, 45)}
    require(len(rows) == 44, f"acceptance row count drifted: {len(rows)}")
    require({row.get("id") for row in rows} == expected_ids, "acceptance IDs drifted")
    require(all(row.get("status") == "REQUIRED" for row in rows), "acceptance row weakened")

    require(evidence.get("schema_version") == 1, "evidence schema drifted")
    require(evidence.get("stage") == "Stage 8B-P1-d0", "evidence stage drifted")
    require(
        evidence.get("status") == "DESIGN_POLICY_REVIEW_CANDIDATE",
        "evidence status drifted",
    )
    require(evidence.get("accepted_p1c_closure_ref") == BASE, "accepted lineage drifted")
    require(evidence.get("acceptance_rows") == 44, "evidence row count drifted")
    require(evidence.get("negative_cases") == 21, "negative count drifted")
    require(evidence.get("design_only") is True, "design-only marker opened")
    require(
        evidence.get("current_tree_authority_remains_p1c") is True,
        "design candidate rebound current-tree authority",
    )
    require(
        evidence.get("governance_rebind_deferred_until_source_acceptance") is True,
        "governance rebind timing drifted",
    )
    require(
        evidence.get("implementation_authorized") is False,
        "implementation was authorized by design candidate",
    )
    closed = evidence.get("closed_surfaces")
    require(isinstance(closed, dict) and len(closed) == 8, "closed surface inventory drifted")
    require(all(value is False for value in closed.values()), "closed surface opened")

    require(
        "Stage 8B-P1-c is formally closed" in status
        and "Stage 8B-P1-d0" in status
        and "design/policy review candidate" in status,
        "current status is not synchronized",
    )
    require(
        "Stage 8B-P1-d0" in roadmap
        and "P1-d1 provider core" in roadmap
        and "Operational Redis DB 0 remains closed" in roadmap,
        "roadmap is not synchronized",
    )


def main() -> None:
    try:
        actual = changed_files()
        require(actual == EXPECTED_CHANGED, f"design-only changed path drift: {sorted(actual)}")
        rust_or_cargo = subprocess.run(
            ["git", "diff", "--name-only", BASE, "--", "Cargo.toml", "Cargo.lock", "crates", ".github"],
            cwd=ROOT,
            check=True,
            text=True,
            capture_output=True,
        ).stdout.strip()
        require(not rust_or_cargo, f"Rust/Cargo/workflow changed: {rust_or_cargo}")
        validate(
            POLICY.read_text(encoding="utf-8"),
            MATRIX.read_text(encoding="utf-8"),
            json.loads(EVIDENCE.read_text(encoding="utf-8")),
            STATUS.read_text(encoding="utf-8"),
            ROADMAP.read_text(encoding="utf-8"),
        )
    except (CheckFailure, OSError, json.JSONDecodeError, subprocess.CalledProcessError) as error:
        print(f"FAIL stage8b-p1d0-design-policy: {error}", file=sys.stderr)
        raise SystemExit(1)
    print(
        "PASS stage8b-p1d0-design-policy "
        "rows=44 negatives=21 design_only=true implementation=false db0=false "
        "finam=false dispatch=false live=false real_orders=false"
    )


if __name__ == "__main__":
    main()
