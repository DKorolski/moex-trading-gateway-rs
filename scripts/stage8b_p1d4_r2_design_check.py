#!/usr/bin/env python3
"""Fail-closed scope/content checker for the P1-d4 R2 design correction."""

from __future__ import annotations

import csv
import hashlib
import json
import pathlib
import subprocess
import sys


ROOT = pathlib.Path(__file__).resolve().parents[1]
BASE = "7dc7c802feca6e79d3a1a9902c181ad7b6afc506"
R0_REF = "b06c78b46d2a5b7a8209d58f1c327d7cf30ae98f"
R1_REF = "3a3f14f595b9672b23d421e7a857117fb2c578d2"
GENERAL_MATRIX_SHA256 = "57f9e83c9a4d8c6b56cb39792c7e08f63e2717c261c508fdd443317b78aac4c0"
R1_CELL_MATRIX_SHA256 = "d1d765f1fa6db1dc948725273f58938c1d0cabd614d26076bf3ac2ddd1ad36ed"
CELL_MATRIX_SHA256 = "b54d8d26e5ebb12389946c905f37a029beb85d1005c7ef95edf6a47596bd725a"
DESIGN = ROOT / "docs/stage-8/stage8b-p1d4-exhaustive-crash-replay-design.md"
R1 = ROOT / "docs/stage-8/stage8b-p1d4-crash-replay-design-r1.md"
R2 = ROOT / "docs/stage-8/stage8b-p1d4-crash-replay-design-r2.md"
MATRIX = ROOT / "docs/stage-8/stage8b-p1d4-crash-replay-acceptance-matrix.csv"
R1_CELL_MATRIX = ROOT / "docs/stage-8/stage8b-p1d4-scenario-frontier-matrix-v1.csv"
CELL_MATRIX = ROOT / "docs/stage-8/stage8b-p1d4-scenario-frontier-matrix-v2.csv"
EVIDENCE = ROOT / "docs/stage-8/stage8b-p1d4-crash-replay-evidence.json"
STATUS = ROOT / "docs/current-status.md"
ROADMAP = ROOT / "docs/roadmap.md"
EXPECTED_CHANGED = {
    "docs/current-status.md",
    "docs/roadmap.md",
    "docs/stage-8/stage8b-p1d4-crash-replay-acceptance-matrix.csv",
    "docs/stage-8/stage8b-p1d4-crash-replay-design-r1.md",
    "docs/stage-8/stage8b-p1d4-crash-replay-design-r2.md",
    "docs/stage-8/stage8b-p1d4-crash-replay-evidence.json",
    "docs/stage-8/stage8b-p1d4-exhaustive-crash-replay-design.md",
    "docs/stage-8/stage8b-p1d4-scenario-frontier-matrix-v1.csv",
    "docs/stage-8/stage8b-p1d4-scenario-frontier-matrix-v2.csv",
    "scripts/make_stage8b_p1d4_design_handoff.py",
    "scripts/stage8b_p1d4_design_check.py",
    "scripts/stage8b_p1d4_design_gate.sh",
    "scripts/stage8b_p1d4_design_handoff_safety_check.py",
    "scripts/stage8b_p1d4_design_negative_harness.py",
    "scripts/stage8b_p1d4_r2_design_check.py",
    "scripts/stage8b_p1d4_r2_design_negative_harness.py",
}
CELL_FIELDS = [
    "cell_id", "scenario_id", "semantic_family", "source_kind", "frontier_id",
    "precondition", "kill_hook_name", "expected_restart_disposition",
    "only_legal_continuation", "sequence_expectation", "callback_delta",
    "provider_delta", "schedule_authority_delta", "pel_before", "pel_after",
    "xack_expectation", "duplicate_variant_required", "conflict_variant_required",
    "inherited_or_new_test_id",
]
REQUEST_SCENARIOS = {"S01", "S02", "S03", "S08", "S09", "S10", "S11"}


class CheckFailure(RuntimeError):
    pass


def require(condition: bool, message: str) -> None:
    if not condition:
        raise CheckFailure(message)


def sha256(value: str) -> str:
    return hashlib.sha256(value.encode()).hexdigest()


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
    r2: str,
    matrix_text: str,
    r1_cell_matrix_text: str,
    cell_matrix_text: str,
    evidence: dict[str, object],
    status: str,
    roadmap: str,
) -> None:
    for token in ("Status: R0 design retained but not accepted", "stage8b-p1d4-crash-replay-design-r1.md"):
        require(token in design, f"R0 history drifted: {token}")
    for token in (
        "Status: R1 design-only review candidate",
        "exactly 80 rows with IDs `P1D4C-001..P1D4C-080`",
        R1_CELL_MATRIX_SHA256,
    ):
        require(token in r1, f"R1 history drifted: {token}")
    require(sha256(r1_cell_matrix_text) == R1_CELL_MATRIX_SHA256, "historical R1 cell matrix drifted")

    required_r2 = (
        "Status: R2 design-only review candidate",
        BASE,
        R0_REF,
        R1_REF,
        GENERAL_MATRIX_SHA256,
        CELL_MATRIX_SHA256,
        "cells: 92",
        "frontiers: F00..F20",
        "number 92 is derived from the complete reviewed rows",
        "General requirements P1D4D-035..P1D4D-056 now use the same",
        "F20 and request-scoped F02 have the same durable journal suffix",
        "Stage6Stage8bP1d3DispatchOnlyCandidate",
        "classify_stage8b_p1d3_dispatch_only_candidate",
        "Stage8bP1d3DispatchPendingOwner",
        "Stage7bRestartOutcome::P1d3DispatchPending",
        "resume_stage8b_p1d3_dispatch_pending",
        "must not call either dispatch-append helper",
        "dispatch rows                 1 -> 1",
        "P1D4C-088 | S06/F07",
        "P1D4C-092 | S09/F04",
        "P1D4C-078 and P1D4C-079 require new hooks",
        "schema_version:             integer 1",
        'domain:                     "moex.stage8b.p1d4.crash-marker.v1"',
        "only `child_pid` is replaced by integer",
        "moex.stage8b.p1d4.crash-marker.normalized.v1\\0",
        "Substitution of the old marker",
        "each corrected general row P1D4D-035..P1D4D-056",
        "normal-path ordering remain immutable",
        "operational Redis DB0/VPS",
        "P1-e remains unauthorized",
    )
    for token in required_r2:
        require(token in r2, f"missing R2 invariant: {token}")
    require(r2.count("| F") == 21, "R2 frontier table drifted")

    require(sha256(matrix_text) == GENERAL_MATRIX_SHA256, "general matrix byte hash drifted")
    rows = list(csv.DictReader(matrix_text.splitlines()))
    require(len(rows) == 80, f"general acceptance count drifted: {len(rows)}")
    require([row.get("id") for row in rows] == [f"P1D4D-{i:03d}" for i in range(1, 81)], "general IDs/order drifted")
    require(all(row.get("status") == "REQUIRED" for row in rows), "general acceptance weakened")
    general = {row["id"]: row["requirement"] for row in rows}
    corrected_mapping_tokens = {
        "P1D4D-035": "F00 reclaims",
        "P1D4D-036": "F01 rebinds",
        "P1D4D-037": "F20 and request-scoped F02 recover only as P1d3DispatchPending",
        "P1D4D-038": "F03 recovers as P1d3PreAckPending",
        "P1D4D-039": "F04 and F05 permit only exact ACK replay",
        "P1D4D-040": "F06 recovers as P1d3AckCommitted",
        "P1D4D-041": "F07 resumes from the preceding WAL or seal",
        "P1D4D-042": "F08 permits only exact source XACK or the exact bound cancel continuation",
        "P1D4D-043": "F09 permits only exact recovered-cancel ACK replay",
        "P1D4D-044": "F10 permits only exact command-source XACK",
        "P1D4D-045": "F11 reclaims the exact later M10",
        "P1D4D-046": "F12 deterministically reevaluates",
        "P1D4D-047": "F13 reconstructs the exact autonomous later truth",
        "P1D4D-048": "F14 invokes the exact same-bar callback once",
        "P1D4D-049": "F15 permits only source XACK",
        "P1D4D-050": "F16 resolves AlreadyAcknowledged",
        "P1D4D-051": "F17 may reissue one equivalent Day-boundary authority",
        "P1D4D-052": "F18 reconstructs exact Day-expiry truth",
        "P1D4D-053": "F19 returns directly Ready",
        "P1D4D-054": "F01 F02 F11 F12 F17 F20",
        "P1D4D-055": "never reads wall clock",
        "P1D4D-056": "dispatch-only suffix are mutually cross-validated",
    }
    for row_id, token in corrected_mapping_tokens.items():
        require(token in general[row_id], f"general frontier mapping drifted: {row_id}")
    for row_id, token in {
        "P1D4D-011": "92 exact cells",
        "P1D4D-012": "21 frontier IDs F00 through F20",
        "P1D4D-027": "P1D4C-088",
        "P1D4D-028": "P1D4C-089",
        "P1D4D-030": "P1D4C-090 through P1D4C-092",
        "P1D4D-073": "exactly one valid DispatchAttemptRecorded suffix",
        "P1D4D-074": "no-second-dispatch",
        "P1D4D-076": "exact eight-field schema",
        "P1D4D-077": "only child_pid",
        "P1D4D-078": "new P1-d4 exact-protocol hooks",
    }.items():
        require(token in general[row_id], f"general R2 requirement drifted: {row_id}")

    require(sha256(cell_matrix_text) == CELL_MATRIX_SHA256, "R2 cell matrix byte hash drifted")
    cell_rows = list(csv.DictReader(cell_matrix_text.splitlines()))
    require(cell_rows and list(cell_rows[0]) == CELL_FIELDS, "cell fields drifted")
    require(len(cell_rows) == 92, f"cell count drifted: {len(cell_rows)}")
    require([row["cell_id"] for row in cell_rows] == [f"P1D4C-{i:03d}" for i in range(1, 93)], "cell IDs/order drifted")
    require(len({row["cell_id"] for row in cell_rows}) == 92, "duplicate cell IDs")
    require({row["scenario_id"] for row in cell_rows} == {f"S{i:02d}" for i in range(1, 12)}, "scenario set drifted")
    require({row["frontier_id"] for row in cell_rows} == {f"F{i:02d}" for i in range(21)}, "frontier set drifted")
    require(all(row["duplicate_variant_required"] == "true" and row["conflict_variant_required"] == "true" for row in cell_rows), "duplicate/conflict coverage weakened")
    require(all("not_applicable" not in value and "all_applicable" not in value for row in cell_rows for value in row.values()), "non-finite cell value")
    by_cell = {row["cell_id"]: row for row in cell_rows}
    f20 = [row for row in cell_rows if row["frontier_id"] == "F20"]
    require({row["scenario_id"] for row in f20} == REQUEST_SCENARIOS and len(f20) == 7, "F20 scenario set drifted")
    request_f02 = [row for row in cell_rows if row["frontier_id"] == "F02" and row["scenario_id"] in REQUEST_SCENARIOS]
    require(len(request_f02) == 7, "request F02 count drifted")
    require(all(row["expected_restart_disposition"] == "P1d3DispatchPending" for row in request_f02), "request F02 owner weakened")
    require(all("without_second_dispatch" in row["only_legal_continuation"] for row in request_f02), "request F02 allows duplicate dispatch")
    require(by_cell["P1D4C-042"]["expected_restart_disposition"] == "Ready", "autonomous F02 owner drifted")
    exact_added = {
        "P1D4C-081": ("S01", "F20", "p1d4-s01-f20"),
        "P1D4C-082": ("S02", "F20", "p1d4-s02-f20"),
        "P1D4C-083": ("S03", "F20", "p1d4-s03-f20"),
        "P1D4C-084": ("S08", "F20", "p1d4-s08-f20"),
        "P1D4C-085": ("S09", "F20", "p1d4-s09-f20"),
        "P1D4C-086": ("S10", "F20", "p1d4-s10-f20"),
        "P1D4C-087": ("S11", "F20", "p1d4-s11-f20"),
        "P1D4C-088": ("S06", "F07", "p1d4-s06-f07"),
        "P1D4C-089": ("S07", "F07", "p1d4-s07-f07"),
        "P1D4C-090": ("S09", "F07", "p1d4-s09-target-f07"),
        "P1D4C-091": ("S09", "F03", "p1d4-s09-cancel-f03"),
        "P1D4C-092": ("S09", "F04", "p1d4-s09-cancel-f04"),
    }
    for cell_id, expected in exact_added.items():
        row = by_cell[cell_id]
        require((row["scenario_id"], row["frontier_id"], row["kill_hook_name"]) == expected, f"added cell drifted: {cell_id}")
    for cell_id, hook, test_id in (
        ("P1D4C-078", "p1d4-s11-f09", "new:p1d4_s11_f09_exact_sigkill"),
        ("P1D4C-079", "p1d4-s11-f10", "new:p1d4_s11_f10_exact_sigkill"),
    ):
        row = by_cell[cell_id]
        require(row["kill_hook_name"] == hook and row["inherited_or_new_test_id"] == test_id, f"exact S11 witness drifted: {cell_id}")
        require("inherited:" not in row["inherited_or_new_test_id"], f"inherited test completes R2 cell: {cell_id}")

    require(evidence.get("stage") == "Stage 8B-P1-d4 exhaustive crash/replay closure design", "stage drifted")
    require(evidence.get("status") == "DESIGN_R2_REVIEW_CANDIDATE", "status drifted")
    require(evidence.get("accepted_p1d3_closure_ref") == BASE, "lineage drifted")
    require(evidence.get("reviewed_r0_ref") == R0_REF and evidence.get("reviewed_r1_ref") == R1_REF, "review lineage drifted")
    require(evidence.get("acceptance_rows") == 80 and evidence.get("acceptance_matrix_sha256") == GENERAL_MATRIX_SHA256, "general evidence drifted")
    require(evidence.get("scenario_frontier_matrix_rows") == 92 and evidence.get("scenario_frontier_matrix_sha256") == CELL_MATRIX_SHA256, "cell evidence drifted")
    require(evidence.get("frontier_count") == 21 and evidence.get("minimum_semantic_families") == 11, "coverage evidence drifted")
    require(evidence.get("negative_cases") == 104, "negative inventory drifted")
    require(evidence.get("design_only") is True and evidence.get("implementation_authorized") is False, "design boundary opened")
    require(evidence.get("required_frontiers") == [f"F{i:02d}" for i in range(21)], "frontier evidence drifted")
    dispatch = evidence.get("dispatch_only_recovery_contract")
    require(isinstance(dispatch, dict), "dispatch contract missing")
    require(dispatch.get("classifier") == "classify_stage8b_p1d3_dispatch_only_candidate", "dispatch classifier drifted")
    require(dispatch.get("expected_restart_disposition") == "P1d3DispatchPending", "dispatch owner drifted")
    require(dispatch.get("second_dispatch_allowed") is False and dispatch.get("outcome_append_count") == 1, "dispatch mutation weakened")
    marker = evidence.get("marker_contract")
    require(isinstance(marker, dict), "marker contract missing")
    require(marker.get("schema_version") == 1 and marker.get("field_count") == 8, "marker schema drifted")
    require(marker.get("domain") == "moex.stage8b.p1d4.crash-marker.v1", "marker domain drifted")
    require(marker.get("normalized_digest_domain") == "moex.stage8b.p1d4.crash-marker.normalized.v1", "marker digest domain drifted")
    require(marker.get("normalized_fields") == ["/child_pid"], "marker normalization widened")
    test = evidence.get("test_contract")
    require(isinstance(test, dict), "test contract missing")
    for key in ("actual_child_process_kill", "child_has_no_descendants", "ephemeral_loopback_redis", "fsync_backed_pre_kill_marker", "non_returning_barrier", "response_loss_after_real_xack", "wait_and_reap_before_restart"):
        require(test.get(key) is True, f"test contract weakened: {key}")
    require(test.get("kernel_observed_exit_signal") == 9, "SIGKILL drifted")
    require(test.get("p1d3_business_semantics_mutable") is False, "business semantics opened")
    require(test.get("p1d3_dispatch_only_recovery_delta_after_acceptance") is True, "dispatch recovery delta hidden")
    contract = evidence.get("evidence_contract")
    require(isinstance(contract, dict), "evidence contract missing")
    require(contract.get("semantic_digest_domain") == "moex.stage8b.p1d4.crash-replay.semantic-evidence.v1", "semantic domain drifted")
    require(contract.get("semantic_reproducibility_runs") == 2, "reproducibility weakened")
    require(contract.get("volatile_json_pointers") == [
        "/run_ordinal", "/cells/*/process/child_pid", "/cells/*/process/wall_duration_ms",
        "/cells/*/filesystem/scratch_root", "/cells/*/filesystem/raw_marker_sha256",
        "/cells/*/redis/port",
    ], "volatile pointer allowlist drifted")
    closed = evidence.get("closed_surfaces")
    require(isinstance(closed, dict) and len(closed) == 11 and all(value is False for value in closed.values()), "closed surface opened")

    for token in (R1_REF, "active R2 design-only correction", "92-cell scenario/frontier registry", "P1d3DispatchPending"):
        require(token in status, f"status drifted: {token}")
    for token in (R1_REF, "active P1-d4 R2 design-only correction", "dispatch-only recovery composition", "92-cell"):
        require(token in roadmap, f"roadmap drifted: {token}")


def main() -> None:
    try:
        actual = changed_files()
        require(actual == EXPECTED_CHANGED, f"R2 design changed path drift: {sorted(actual)}")
        validate(
            DESIGN.read_text(encoding="utf-8"), R1.read_text(encoding="utf-8"),
            R2.read_text(encoding="utf-8"), MATRIX.read_text(encoding="utf-8"),
            R1_CELL_MATRIX.read_text(encoding="utf-8"), CELL_MATRIX.read_text(encoding="utf-8"),
            json.loads(EVIDENCE.read_text(encoding="utf-8")),
            STATUS.read_text(encoding="utf-8"), ROADMAP.read_text(encoding="utf-8"),
        )
    except (OSError, json.JSONDecodeError, subprocess.CalledProcessError, CheckFailure) as error:
        print(f"stage8b-p1d4-r2-design-check: FAIL {error}", file=sys.stderr)
        raise SystemExit(1)
    print("PASS stage8b-p1d4-r2-design-scope files=16 general_rows=80 cells=92 frontiers=21 scenarios=11")


if __name__ == "__main__":
    main()
