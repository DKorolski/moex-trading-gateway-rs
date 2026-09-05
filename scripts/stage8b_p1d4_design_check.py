#!/usr/bin/env python3
"""Fail-closed scope/content checker for the P1-d4 R1 design correction."""

from __future__ import annotations

import csv
import hashlib
import json
import pathlib
import subprocess
import sys


ROOT = pathlib.Path(__file__).resolve().parents[1]
BASE = "7dc7c802feca6e79d3a1a9902c181ad7b6afc506"
R0 = "b06c78b46d2a5b7a8209d58f1c327d7cf30ae98f"
CELL_MATRIX_SHA256 = "d1d765f1fa6db1dc948725273f58938c1d0cabd614d26076bf3ac2ddd1ad36ed"
DESIGN = ROOT / "docs/stage-8/stage8b-p1d4-exhaustive-crash-replay-design.md"
R1 = ROOT / "docs/stage-8/stage8b-p1d4-crash-replay-design-r1.md"
MATRIX = ROOT / "docs/stage-8/stage8b-p1d4-crash-replay-acceptance-matrix.csv"
CELL_MATRIX = ROOT / "docs/stage-8/stage8b-p1d4-scenario-frontier-matrix-v1.csv"
EVIDENCE = ROOT / "docs/stage-8/stage8b-p1d4-crash-replay-evidence.json"
STATUS = ROOT / "docs/current-status.md"
ROADMAP = ROOT / "docs/roadmap.md"
EXPECTED_CHANGED = {
    "docs/current-status.md",
    "docs/roadmap.md",
    "docs/stage-8/stage8b-p1d4-crash-replay-acceptance-matrix.csv",
    "docs/stage-8/stage8b-p1d4-crash-replay-design-r1.md",
    "docs/stage-8/stage8b-p1d4-crash-replay-evidence.json",
    "docs/stage-8/stage8b-p1d4-exhaustive-crash-replay-design.md",
    "docs/stage-8/stage8b-p1d4-scenario-frontier-matrix-v1.csv",
    "scripts/make_stage8b_p1d4_design_handoff.py",
    "scripts/stage8b_p1d4_design_check.py",
    "scripts/stage8b_p1d4_design_gate.sh",
    "scripts/stage8b_p1d4_design_handoff_safety_check.py",
    "scripts/stage8b_p1d4_design_negative_harness.py",
}
CELL_FIELDS = [
    "cell_id", "scenario_id", "semantic_family", "source_kind", "frontier_id",
    "precondition", "kill_hook_name", "expected_restart_disposition",
    "only_legal_continuation", "sequence_expectation", "callback_delta",
    "provider_delta", "schedule_authority_delta", "pel_before", "pel_after",
    "xack_expectation", "duplicate_variant_required", "conflict_variant_required",
    "inherited_or_new_test_id",
]


class CheckFailure(RuntimeError):
    pass


def require(condition: bool, message: str) -> None:
    if not condition:
        raise CheckFailure(message)


def changed_files() -> set[str]:
    tracked = subprocess.run(
        ["git", "diff", "--name-only", BASE], cwd=ROOT, check=True, text=True,
        capture_output=True,
    ).stdout.splitlines()
    untracked = subprocess.run(
        ["git", "ls-files", "--others", "--exclude-standard"], cwd=ROOT,
        check=True, text=True, capture_output=True,
    ).stdout.splitlines()
    return set(tracked) | set(untracked)


def validate(
    design: str,
    r1: str,
    matrix_text: str,
    cell_matrix_text: str,
    evidence: dict[str, object],
    status: str,
    roadmap: str,
) -> None:
    for token in (
        "Status: R0 design retained but not accepted",
        "stage8b-p1d4-crash-replay-design-r1.md",
        "not authorized until R1 is independently accepted",
    ):
        require(token in design, f"R0 status drifted: {token}")

    required_r1 = (
        "Status: R1 design-only review candidate",
        BASE,
        R0,
        "exactly 80 rows with IDs `P1D4C-001..P1D4C-080`",
        "11\nscenario IDs and 20 frontier IDs `F00..F19`",
        CELL_MATRIX_SHA256,
        "S03 retains its originating command M10",
        "Only S07 has no new Redis\nsource",
        "S04 proves a zero-intent same-bar callback",
        "S05 proves a one-intent callback",
        "5ceca40f8bbb3cb9f2dc61a1ebf43c617fbbd0d9",
        "27b0edada9ef05bde8b44ba77f321a57bf729d54",
        "F02, F12 and F17 are intentionally pre-write-ahead frontiers",
        "Phase-scoped deterministic re-execution",
        "new one-use authority only for the\nsame authenticated source/boundary",
        "At and after F03, F13 or F18",
        "there is no Stage6 V3 autonomous outcome",
        "F19 returns exact Ready without Redis source acquisition",
        "Child enters a non-returning barrier",
        "marker PID equals\n   `child.id()`",
        "status.code() == None",
        "ExitStatusExt::signal() == Some(9)",
        "Only after successful reap",
        "receive and parse Redis's successful integer XACK\nreply (`1`",
        "Stage8bP1d4CrashReplayEvidenceV1",
        "moex.stage8b.p1d4.crash-replay.evidence.v1",
        "/cells/*/process/child_pid",
        "/cells/*/filesystem/raw_marker_sha256",
        "No other field may be removed",
        "passed`, exit signal/code, normalized marker digest",
        "recursive UTF-8 bytewise lexical object\nkey ordering",
        "moex.stage8b.p1d4.crash-replay.semantic-evidence.v1\\0",
        "Two clean runs must have identical semantic digests",
        "SIGTERM, normal exit, returning barrier, kill-before-marker",
        "P1-e remains unauthorized",
    )
    for token in required_r1:
        require(token in r1, f"missing R1 invariant: {token}")
    require(r1.count("| F") == 20, "R1 frontier table drifted")

    rows = list(csv.DictReader(matrix_text.splitlines()))
    require(len(rows) == 72, f"general acceptance count drifted: {len(rows)}")
    require([row.get("id") for row in rows] == [f"P1D4D-{i:03d}" for i in range(1, 73)], "general IDs drifted")
    require(all(row.get("status") == "REQUIRED" for row in rows), "general acceptance weakened")
    general = {row["id"]: row["requirement"] for row in rows}
    for row_id, token in {
        "P1D4D-011": "P1D4C-001 through P1D4C-080",
        "P1D4D-014": "equivalent authority reissue",
        "P1D4D-018": "parsed Redis XACK integer reply 1",
        "P1D4D-020": "signal 9",
        "P1D4D-025": "command-source F16",
        "P1D4D-026": "zero and one-intent",
        "P1D4D-028": "forbid XACK",
        "P1D4D-030": "post-S_cancel F10",
        "P1D4D-065": "exact P1D4C registry cell",
    }.items():
        require(token in general[row_id], f"general semantics drifted: {row_id}")

    require(hashlib.sha256(cell_matrix_text.encode()).hexdigest() == CELL_MATRIX_SHA256, "cell matrix byte hash drifted")
    cell_rows = list(csv.DictReader(cell_matrix_text.splitlines()))
    require(cell_rows and list(cell_rows[0]) == CELL_FIELDS, "cell fields drifted")
    require(len(cell_rows) == 80, f"cell count drifted: {len(cell_rows)}")
    require([row["cell_id"] for row in cell_rows] == [f"P1D4C-{i:03d}" for i in range(1, 81)], "cell IDs/order drifted")
    require(len({row["cell_id"] for row in cell_rows}) == 80, "duplicate cell IDs")
    require({row["scenario_id"] for row in cell_rows} == {f"S{i:02d}" for i in range(1, 12)}, "scenario set drifted")
    require({row["frontier_id"] for row in cell_rows} == {f"F{i:02d}" for i in range(20)}, "frontier set drifted")
    require(all(row["duplicate_variant_required"] == "true" and row["conflict_variant_required"] == "true" for row in cell_rows), "duplicate/conflict coverage weakened")
    require(all("not_applicable" not in value and "all_applicable" not in value for row in cell_rows for value in row.values()), "non-finite cell value")
    by_cell = {row["cell_id"]: row for row in cell_rows}
    for cell_id, field, value in (
        ("P1D4C-030", "xack_expectation", "parsed_reply_1_then_AlreadyAcknowledged"),
        ("P1D4C-031", "semantic_family", "later_untouched_zero_intent"),
        ("P1D4C-036", "semantic_family", "later_untouched_one_intent"),
        ("P1D4C-039", "expected_restart_disposition", "P1SemanticPrepublicationReady"),
        ("P1D4C-049", "xack_expectation", "forbidden_no_source"),
        ("P1D4C-064", "frontier_id", "F09"),
        ("P1D4C-065", "frontier_id", "F10"),
        ("P1D4C-078", "inherited_or_new_test_id", "inherited:p1d3_subprocess_sigkill_brackets_s_cancel_recovered@7dc7c80"),
        ("P1D4C-079", "inherited_or_new_test_id", "inherited:p1d3_subprocess_sigkill_brackets_s_cancel_recovered@7dc7c80"),
    ):
        require(by_cell[cell_id][field] == value, f"cell semantic drift: {cell_id}.{field}")

    require(evidence.get("stage") == "Stage 8B-P1-d4 exhaustive crash/replay closure design", "stage drifted")
    require(evidence.get("status") == "DESIGN_R1_REVIEW_CANDIDATE", "status drifted")
    require(evidence.get("accepted_p1d3_closure_ref") == BASE, "lineage drifted")
    require(evidence.get("reviewed_r0_ref") == R0, "R0 lineage drifted")
    require(evidence.get("acceptance_rows") == 72, "general row count drifted")
    require(evidence.get("scenario_frontier_matrix_rows") == 80, "cell count evidence drifted")
    require(evidence.get("scenario_frontier_matrix_sha256") == CELL_MATRIX_SHA256, "cell hash evidence drifted")
    require(evidence.get("negative_cases") == 60, "negative count drifted")
    require(evidence.get("frontier_count") == 20, "frontier count drifted")
    require(evidence.get("minimum_semantic_families") == 11, "family count drifted")
    require(evidence.get("design_only") is True and evidence.get("implementation_authorized") is False, "design boundary opened")
    require(evidence.get("required_frontiers") == [f"F{i:02d}" for i in range(20)], "frontier evidence drifted")
    test = evidence.get("test_contract")
    require(isinstance(test, dict) and len(test) == 9, "test contract drifted")
    for key in ("actual_child_process_kill", "child_has_no_descendants", "ephemeral_loopback_redis", "fsync_backed_pre_kill_marker", "non_returning_barrier", "response_loss_after_real_xack", "wait_and_reap_before_restart"):
        require(test.get(key) is True, f"test contract weakened: {key}")
    require(test.get("kernel_observed_exit_signal") == 9, "SIGKILL drifted")
    require(test.get("p1d3_semantics_mutable") is False, "P1-d3 mutation opened")
    contract = evidence.get("evidence_contract")
    require(isinstance(contract, dict) and len(contract) == 6, "evidence contract drifted")
    require(contract.get("semantic_digest_domain") == "moex.stage8b.p1d4.crash-replay.semantic-evidence.v1", "semantic domain drifted")
    require(contract.get("semantic_reproducibility_runs") == 2, "reproducibility weakened")
    require(contract.get("volatile_json_pointers") == [
        "/run_ordinal", "/cells/*/process/child_pid", "/cells/*/process/wall_duration_ms",
        "/cells/*/filesystem/scratch_root", "/cells/*/filesystem/raw_marker_sha256",
        "/cells/*/redis/port",
    ], "volatile pointer allowlist drifted")
    closed = evidence.get("closed_surfaces")
    require(isinstance(closed, dict) and len(closed) == 11 and all(value is False for value in closed.values()), "closed surface opened")

    for token in (R0, "active R1 design-only correction", "80-cell scenario/frontier registry"):
        require(token in status, f"status drifted: {token}")
    for token in (R0, "active P1-d4 R1 design-only correction", "kernel-observed non-returning SIGKILL protocol"):
        require(token in roadmap, f"roadmap drifted: {token}")


def main() -> None:
    try:
        actual = changed_files()
        require(actual == EXPECTED_CHANGED, f"R1 design changed path drift: {sorted(actual)}")
        validate(
            DESIGN.read_text(encoding="utf-8"), R1.read_text(encoding="utf-8"),
            MATRIX.read_text(encoding="utf-8"), CELL_MATRIX.read_text(encoding="utf-8"),
            json.loads(EVIDENCE.read_text(encoding="utf-8")),
            STATUS.read_text(encoding="utf-8"), ROADMAP.read_text(encoding="utf-8"),
        )
    except (OSError, json.JSONDecodeError, subprocess.CalledProcessError, CheckFailure) as error:
        print(f"stage8b-p1d4-r1-design-check: FAIL {error}", file=sys.stderr)
        raise SystemExit(1)
    print("PASS stage8b-p1d4-r1-design-scope files=12 general_rows=72 cells=80 frontiers=20 scenarios=11")


if __name__ == "__main__":
    main()
