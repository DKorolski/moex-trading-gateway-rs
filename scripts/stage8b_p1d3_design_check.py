#!/usr/bin/env python3
"""Fail-closed design checker for Stage 8B-P1-d3 working order lifecycle."""

from __future__ import annotations

import csv
import json
import pathlib
import subprocess
import sys


ROOT = pathlib.Path(__file__).resolve().parents[1]
BASE = "bcd8db546104968dd0e48ab041e02acf6869d224"
DESIGN = ROOT / "docs/stage-8/stage8b-p1d3-working-limit-cancel-lifecycle-design.md"
MATRIX = ROOT / "docs/stage-8/stage8b-p1d3-working-limit-cancel-acceptance-matrix.csv"
EVIDENCE = ROOT / "docs/stage-8/stage8b-p1d3-working-limit-cancel-evidence.json"
STATUS = ROOT / "docs/current-status.md"
ROADMAP = ROOT / "docs/roadmap.md"
EXPECTED_CHANGED = {
    "docs/current-status.md",
    "docs/roadmap.md",
    "docs/stage-8/stage8b-p1d3-working-limit-cancel-acceptance-matrix.csv",
    "docs/stage-8/stage8b-p1d3-working-limit-cancel-evidence.json",
    "docs/stage-8/stage8b-p1d3-working-limit-cancel-lifecycle-design.md",
    "scripts/make_stage8b_p1d3_design_handoff.py",
    "scripts/stage8b_p1d3_design_check.py",
    "scripts/stage8b_p1d3_design_gate.sh",
    "scripts/stage8b_p1d3_design_handoff_safety_check.py",
    "scripts/stage8b_p1d3_design_negative_harness.py",
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
    design: str,
    matrix_text: str,
    evidence: dict[str, object],
    status: str,
    roadmap: str,
) -> None:
    required_design = (
        "Status: design-only review candidate",
        BASE,
        "moex.stage8b.p1d3.working-book.v1",
        "moex.stage8b.p1d3.book-transition.v1",
        "1. **request lifecycle**",
        "order lifecycle",
        "terminal\nrequest-processing seal",
        "replacement `S_working` is persisted and reread",
        "inside the existing authenticated replacement package, never as a sidecar",
        "P1D3_MAX_ORDER_RECORDS_PER_GENERATION = 1024",
        "Capacity exhaustion fails closed before a new dispatch.",
        "Stage8bP1d3ScheduleStepAuthority",
        "Stage8bP1d3DayExpiryAuthority",
        "UTC date comparison, wall clock, process time and the\nfirst bar of a new day are not expiry authorities.",
        "resolve the exact already-active order against B[k]",
        "The book evaluator has no M10 XACK method.",
        "records `B[n+1]` as already evaluated",
        "bar.low <= limit -> Filled at min(bar.open, limit)",
        "bar.high >= limit -> Filled at max(bar.open, limit)",
        "exact final execution-bar `close_ts`",
        "Stage6dPaperOutcome::LimitExpired { broker_order_id }",
        "ACK: `Accepted`, reason `None`, exact request-level durable client ID",
        "truth uses `seq_truth = seq_ack + 1`",
        "untouched: order remains Working; no Stage 5G event or total sequence is",
        "Partial fill, a second trade or a\nsecond terminal transition fails closed.",
        "This\nfreezes fill-before-cancel",
        "`CancelCanceled` | `Accepted` / no reason | target `Canceled`",
        "`CancelExecutionObserved` | `Recovered` / `RecoveredByBrokerTruth`",
        "`CancelAlreadyTerminalNonExecution` | `Recovered` / `RecoveredByBrokerTruth`",
        "`Inconclusive` hold | none | none",
        "A target\norder client ID never replaces it.",
        "later autonomous fill or expiry: one next truth sequence",
        "`S_working`, `S_eval` and `S_terminal` are logical phase names",
        "Restart authenticates the exact Stage 5G/6/7 package and P1-d3 book together.",
        "P1-d4 remains responsible for the complete SIGKILL/frontier matrix",
        "current-tree authority remains pinned to accepted P1-d2",
        "P1-d3 production source implementation before design acceptance",
        "operational Redis DB 0 and VPS paper service activation",
        "FINAM HTTP POST/DELETE and broker network dispatch",
    )
    for token in required_design:
        require(token in design, f"missing design invariant: {token}")
    require(design.count("moex.stage8b.p1d3.working-book.v1") == 1, "book domain drifted")
    require(
        design.count("moex.stage8b.p1d3.book-transition.v1") == 1,
        "transition domain drifted",
    )
    require(design.count("P1D3_MAX_ORDER_RECORDS_PER_GENERATION = 1024") == 1, "bound drifted")
    require(design.count("Stage8bP1d3DayExpiryAuthority") == 2, "expiry authority drifted")

    rows = list(csv.DictReader(matrix_text.splitlines()))
    expected_ids = {f"P1D3D-{index:03d}" for index in range(1, 61)}
    require(len(rows) == 60, f"acceptance row count drifted: {len(rows)}")
    require({row.get("id") for row in rows} == expected_ids, "acceptance IDs drifted")
    require(all(row.get("status") == "REQUIRED" for row in rows), "acceptance weakened")
    by_id = {row["id"]: row for row in rows}
    row_tokens = {
        "P1D3D-001": BASE,
        "P1D3D-007": "S_working",
        "P1D3D-018": "before Hybrid callback",
        "P1D3D-026": "LimitExpired",
        "P1D3D-040": "target evaluation precedes cancellation",
        "P1D3D-041": "CancelCanceled",
        "P1D3D-054": "Stage5G domain",
        "P1D3D-057": "P1-d4",
        "P1D3D-060": "accepted P1-d2",
    }
    for row_id, token in row_tokens.items():
        require(token in by_id[row_id]["requirement"], f"acceptance semantics drifted: {row_id}")

    require(evidence.get("schema_version") == 1, "evidence schema drifted")
    require(
        evidence.get("stage")
        == "Stage 8B-P1-d3 working LIMIT/CANCEL/expiry lifecycle design",
        "stage drifted",
    )
    require(evidence.get("status") == "DESIGN_REVIEW_CANDIDATE", "status drifted")
    require(evidence.get("accepted_p1d2_closure_ref") == BASE, "lineage drifted")
    require(
        evidence.get("canonical_book_contract") == "moex.stage8b.p1d3.working-book.v1",
        "book contract drifted",
    )
    require(
        evidence.get("canonical_transition_contract")
        == "moex.stage8b.p1d3.book-transition.v1",
        "transition contract drifted",
    )
    require(evidence.get("acceptance_rows") == 60, "evidence row count drifted")
    require(evidence.get("negative_cases") == 36, "negative count drifted")
    require(evidence.get("design_only") is True, "design-only marker opened")
    require(evidence.get("implementation_authorized") is False, "source opened early")
    require(
        evidence.get("current_tree_authority_remains_p1d2") is True,
        "current-tree authority rebound by design",
    )
    policy = evidence.get("policy")
    require(isinstance(policy, dict) and len(policy) == 8, "policy inventory drifted")
    require(policy.get("active_orders_per_operational_identity") == 1, "active bound drifted")
    require(policy.get("max_order_records_per_generation") == 1024, "registry bound drifted")
    require(policy.get("partial_fills_enabled") is False, "partial fills opened")
    require(
        policy.get("cancel_fill_order") == "target_evaluation_before_cancel",
        "cancel ordering drifted",
    )
    require(
        policy.get("day_expiry_authority") == "opaque_stage5e_source_capability",
        "expiry authority drifted",
    )
    require(
        policy.get("working_source_xack_authority") == "S_working_persisted_and_reread",
        "working XACK authority drifted",
    )
    closed = evidence.get("closed_surfaces")
    require(isinstance(closed, dict) and len(closed) == 12, "closed inventory drifted")
    require(all(value is False for value in closed.values()), "closed surface opened")

    status_tokens = (
        "bcd8db546104968dd0e48ab041e02acf6869d224",
        "active candidate is now the design-only",
        "P1-d3 production source",
        "operational Redis DB0/VPS activation",
    )
    require(all(token in status for token in status_tokens), "current status is not synchronized")
    roadmap_tokens = (
        "bcd8db546104968dd0e48ab041e02acf6869d224",
        "active candidate is the design-only P1-d3",
        "P1-d3 production source",
        "P1-d4 exhaustive crash/replay closure",
    )
    require(all(token in roadmap for token in roadmap_tokens), "roadmap is not synchronized")


def main() -> None:
    try:
        actual = changed_files()
        require(actual == EXPECTED_CHANGED, f"design-only changed path drift: {sorted(actual)}")
        forbidden_diff = subprocess.run(
            [
                "git",
                "diff",
                "--name-only",
                BASE,
                "--",
                "Cargo.toml",
                "Cargo.lock",
                "crates",
                ".github",
                "config",
            ],
            cwd=ROOT,
            check=True,
            text=True,
            capture_output=True,
        ).stdout.strip()
        require(not forbidden_diff, f"production/workflow/config changed: {forbidden_diff}")
        validate(
            DESIGN.read_text(encoding="utf-8"),
            MATRIX.read_text(encoding="utf-8"),
            json.loads(EVIDENCE.read_text(encoding="utf-8")),
            STATUS.read_text(encoding="utf-8"),
            ROADMAP.read_text(encoding="utf-8"),
        )
    except (CheckFailure, OSError, json.JSONDecodeError, subprocess.CalledProcessError) as error:
        print(f"FAIL stage8b-p1d3-design-scope: {error}", file=sys.stderr)
        raise SystemExit(1)
    print(
        "PASS stage8b-p1d3-design-scope rows=60 negatives=36 "
        "design_only=true source=false p1d4=false db0=false finam=false live=false"
    )


if __name__ == "__main__":
    main()
