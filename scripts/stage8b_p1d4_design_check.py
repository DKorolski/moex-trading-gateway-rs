#!/usr/bin/env python3
"""Fail-closed scope and content checker for the P1-d4 design slice."""

from __future__ import annotations

import csv
import json
import pathlib
import subprocess
import sys


ROOT = pathlib.Path(__file__).resolve().parents[1]
BASE = "7dc7c802feca6e79d3a1a9902c181ad7b6afc506"
DESIGN = ROOT / "docs/stage-8/stage8b-p1d4-exhaustive-crash-replay-design.md"
MATRIX = ROOT / "docs/stage-8/stage8b-p1d4-crash-replay-acceptance-matrix.csv"
EVIDENCE = ROOT / "docs/stage-8/stage8b-p1d4-crash-replay-evidence.json"
STATUS = ROOT / "docs/current-status.md"
ROADMAP = ROOT / "docs/roadmap.md"
EXPECTED_CHANGED = {
    "docs/current-status.md",
    "docs/roadmap.md",
    "docs/stage-8/stage8b-p1d4-crash-replay-acceptance-matrix.csv",
    "docs/stage-8/stage8b-p1d4-crash-replay-evidence.json",
    "docs/stage-8/stage8b-p1d4-exhaustive-crash-replay-design.md",
    "scripts/make_stage8b_p1d4_design_handoff.py",
    "scripts/stage8b_p1d4_design_check.py",
    "scripts/stage8b_p1d4_design_gate.sh",
    "scripts/stage8b_p1d4_design_handoff_safety_check.py",
    "scripts/stage8b_p1d4_design_negative_harness.py",
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
        "P1-d3 lifecycle semantics remain immutable",
        "newly\nstarted loopback-only Redis instance on a random port",
        "actual process\nkill",
        "An in-process panic is not sufficient evidence",
        "write_all` plus `sync_all",
        "`F00`",
        "`F01`",
        "`F02`",
        "`F03`",
        "`F04`",
        "`F05`",
        "`F06`",
        "`F07`",
        "`F08`",
        "`F09`",
        "`F10`",
        "`F11`",
        "No implementation may collapse `F03` into `F04`",
        "P1d3PreAckPending",
        "P1d3AckCommitted",
        "P1d3TruthCommitted",
        "CancelContinuationPending",
        "P1d3SemanticPending",
        "target-first ordering",
        "never create a second target truth",
        "must prove seal recovery without\ninventing an XACK",
        "without a spawned\nchild, reached marker, process kill, clean restart and final audit is not\ncredited",
        "Stage 6 checkpoint and replacement package are mutually bound",
        "restart does not call the provider, mint schedule authority",
        "select a new candidate bar",
        "XACK is the last external mutation",
        "AlreadyAcknowledged",
        "Missing PEL membership\nwithout the frontier proof is `ExactSourceConflict`",
        "byte-identical replay is idempotent",
        "changed consumer name alone must not change semantics",
        "two clean runs over the same source must produce the same\nsemantic digest",
        "Missing,\nduplicate, skipped, ignored or `not_applicable` required cells fail closed",
        "operational Redis DB 0 or VPS activation",
        "FINAM POST/DELETE, broker dispatch",
        "partial fills, fees/slippage",
        "A separate independent source acceptance and governance-only authority\nrebind are required",
        "Acceptance of this design authorizes only the P1-d4 source/test implementation",
    )
    for token in required_design:
        require(token in design, f"missing design invariant: {token}")
    require(design.count("| `F") == 12, "frontier registry drifted")

    rows = list(csv.DictReader(matrix_text.splitlines()))
    expected_ids = [f"P1D4D-{index:03d}" for index in range(1, 73)]
    require(len(rows) == 72, f"acceptance row count drifted: {len(rows)}")
    require([row.get("id") for row in rows] == expected_ids, "acceptance IDs drifted")
    require(all(row.get("status") == "REQUIRED" for row in rows), "acceptance weakened")
    by_id = {row["id"]: row["requirement"] for row in rows}
    for row_id, token in {
        "P1D4D-001": BASE,
        "P1D4D-022": "XACK success",
        "P1D4D-032": "Target-first fill",
        "P1D4D-041": "authenticated group frontier",
        "P1D4D-047": "compare literally",
        "P1D4D-056": "no second ACK truth callback",
        "P1D4D-068": "Two clean runs",
        "P1D4D-072": "P1-e",
    }.items():
        require(token in by_id[row_id], f"acceptance semantics drifted: {row_id}")

    require(evidence.get("stage") == "Stage 8B-P1-d4 exhaustive crash/replay closure design", "stage drifted")
    require(evidence.get("status") == "DESIGN_REVIEW_CANDIDATE", "status drifted")
    require(evidence.get("accepted_p1d3_closure_ref") == BASE, "lineage drifted")
    require(evidence.get("acceptance_rows") == 72, "evidence row count drifted")
    require(evidence.get("negative_cases") == 36, "negative count drifted")
    require(evidence.get("frontier_count") == 12, "frontier count drifted")
    require(evidence.get("minimum_semantic_families") == 10, "family count drifted")
    require(evidence.get("design_only") is True, "design-only marker opened")
    require(evidence.get("implementation_authorized") is False, "implementation opened early")
    require(evidence.get("required_frontiers") == [f"F{index:02d}" for index in range(12)], "frontier evidence drifted")
    test_contract = evidence.get("test_contract")
    require(isinstance(test_contract, dict) and len(test_contract) == 6, "test contract drifted")
    require(test_contract.get("actual_child_process_kill") is True, "real child kill weakened")
    require(test_contract.get("ephemeral_loopback_redis") is True, "Redis isolation weakened")
    require(test_contract.get("fsync_backed_pre_kill_marker") is True, "marker durability weakened")
    require(test_contract.get("p1d3_semantics_mutable") is False, "P1-d3 mutation opened")
    require(test_contract.get("response_loss_after_real_xack") is True, "response loss weakened")
    require(test_contract.get("semantic_evidence_reproducibility_runs") == 2, "reproducibility weakened")
    closed = evidence.get("closed_surfaces")
    require(isinstance(closed, dict) and len(closed) == 11, "closed surface inventory drifted")
    require(all(value is False for value in closed.values()), "closed surface opened")

    for token in (BASE, "P1-d4 is the active design-only candidate", "operational Redis DB0/VPS activation"):
        require(token in status, f"status not synchronized: {token}")
    for token in (BASE, "active P1-d4 design-only slice", "P1-d4 source implementation requires separate design acceptance"):
        require(token in roadmap, f"roadmap not synchronized: {token}")


def main() -> None:
    try:
        actual = changed_files()
        require(actual == EXPECTED_CHANGED, f"design-only changed path drift: {sorted(actual)}")
        validate(
            DESIGN.read_text(encoding="utf-8"),
            MATRIX.read_text(encoding="utf-8"),
            json.loads(EVIDENCE.read_text(encoding="utf-8")),
            STATUS.read_text(encoding="utf-8"),
            ROADMAP.read_text(encoding="utf-8"),
        )
    except (OSError, json.JSONDecodeError, subprocess.CalledProcessError, CheckFailure) as error:
        print(f"stage8b-p1d4-design-check: FAIL {error}", file=sys.stderr)
        raise SystemExit(1)
    print("PASS stage8b-p1d4-design-scope files=10 rows=72 frontiers=12 design_only=true")


if __name__ == "__main__":
    main()
