#!/usr/bin/env python3
"""Fail-closed scope/content checker for the P1-d4 R3 design correction."""

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
R2_REF = "16fe6dc535fdd744be9b814ab517386d83f52eac"
R1_CELL_MATRIX_SHA256 = "d1d765f1fa6db1dc948725273f58938c1d0cabd614d26076bf3ac2ddd1ad36ed"
R2_DESIGN_SHA256 = "b71c3b04a55c4ab0e11432133e720291e3f32c10c10f4818a640d9d59dc9a884"
R2_CELL_MATRIX_SHA256 = "b54d8d26e5ebb12389946c905f37a029beb85d1005c7ef95edf6a47596bd725a"
R3_DESIGN_SHA256 = "d4c844498b47b53a09fc916e7a110ce856725ad421e4dee1c4b641e85dc70e7a"
GENERAL_MATRIX_SHA256 = "e2a376fda504a691b3dffa5160542c5f184bf59ff9f224ae4159b5bbaf8c06de"
CELL_MATRIX_SHA256 = "8c3122af016e860e3fa54a6f4143c50b9258c2a5a4bd2d15f9847a118a8582cc"

DESIGN = ROOT / "docs/stage-8/stage8b-p1d4-exhaustive-crash-replay-design.md"
R1 = ROOT / "docs/stage-8/stage8b-p1d4-crash-replay-design-r1.md"
R2 = ROOT / "docs/stage-8/stage8b-p1d4-crash-replay-design-r2.md"
R3 = ROOT / "docs/stage-8/stage8b-p1d4-crash-replay-design-r3.md"
MATRIX = ROOT / "docs/stage-8/stage8b-p1d4-crash-replay-acceptance-matrix.csv"
R1_CELL_MATRIX = ROOT / "docs/stage-8/stage8b-p1d4-scenario-frontier-matrix-v1.csv"
R2_CELL_MATRIX = ROOT / "docs/stage-8/stage8b-p1d4-scenario-frontier-matrix-v2.csv"
CELL_MATRIX = ROOT / "docs/stage-8/stage8b-p1d4-scenario-frontier-matrix-v3.csv"
EVIDENCE = ROOT / "docs/stage-8/stage8b-p1d4-crash-replay-evidence.json"
STATUS = ROOT / "docs/current-status.md"
ROADMAP = ROOT / "docs/roadmap.md"

EXPECTED_CHANGED = {
    "docs/current-status.md",
    "docs/roadmap.md",
    "docs/stage-8/stage8b-p1d4-crash-replay-acceptance-matrix.csv",
    "docs/stage-8/stage8b-p1d4-crash-replay-design-r1.md",
    "docs/stage-8/stage8b-p1d4-crash-replay-design-r2.md",
    "docs/stage-8/stage8b-p1d4-crash-replay-design-r3.md",
    "docs/stage-8/stage8b-p1d4-crash-replay-evidence.json",
    "docs/stage-8/stage8b-p1d4-exhaustive-crash-replay-design.md",
    "docs/stage-8/stage8b-p1d4-scenario-frontier-matrix-v1.csv",
    "docs/stage-8/stage8b-p1d4-scenario-frontier-matrix-v2.csv",
    "docs/stage-8/stage8b-p1d4-scenario-frontier-matrix-v3.csv",
    "scripts/make_stage8b_p1d4_design_handoff.py",
    "scripts/stage8b_p1d4_design_check.py",
    "scripts/stage8b_p1d4_design_gate.sh",
    "scripts/stage8b_p1d4_design_handoff_safety_check.py",
    "scripts/stage8b_p1d4_design_negative_harness.py",
    "scripts/stage8b_p1d4_r2_design_check.py",
    "scripts/stage8b_p1d4_r2_design_negative_harness.py",
    "scripts/stage8b_p1d4_r3_design_check.py",
    "scripts/stage8b_p1d4_r3_design_negative_harness.py",
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
SIMPLE_SCENARIOS = ["S01", "S02", "S03", "S08", "S10", "S11"]


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
    r3: str,
    matrix_text: str,
    r1_cell_matrix_text: str,
    r2_cell_matrix_text: str,
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
    require(sha256(r2) == R2_DESIGN_SHA256, "historical R2 design drifted")
    require(sha256(r2_cell_matrix_text) == R2_CELL_MATRIX_SHA256, "historical R2 cell matrix drifted")
    for token in (
        "Status: R2 design-only review candidate", R2_CELL_MATRIX_SHA256,
        "cells: 92", "frontiers: F00..F20", "P1d3DispatchPending",
        "operational Redis DB0/VPS", "P1-e remains unauthorized",
    ):
        require(token in r2, f"R2 history drifted: {token}")

    required_r3 = (
        "Status: R3 design-only review candidate",
        BASE, R0_REF, R1_REF, R2_REF, GENERAL_MATRIX_SHA256, CELL_MATRIX_SHA256,
        "rows: 88", "cells: 92", "P1D4C-001..P1D4C-092",
        "The dispatch-only classifier must not infer P1-d3 membership",
        "exact BrokerCommand::PlaceOrder", "Stage6DurableActionKind::Place",
        "OrderType::Limit", "TimeInForce::Day", "mathematically integral",
        "authenticated and bound to committed seal", "P1-d3 working book",
        "exact BrokerCommand::CancelOrder", "Stage6DurableActionKind::Cancel",
        "exactly one row for that BOID", "absent or exactly canonical",
        "canonical request DCID and distinct from TCID",
        "existing accepted P1-d3 V3 journal-ahead classifier",
        "existing accepted P1-d2 journal-ahead classifier",
        "existing accepted generic P1 journal-ahead classifier",
        "new P1-d3 dispatch-only classifier", "unchanged fail-closed blocked fallback",
        "P1-d2 Market RequestAccepted", "classify_stage8b_p1d3_dispatch_only_candidate == None",
        "P1d3DispatchPending owners minted                 0",
        "P1-d3 V3 outcomes appended                        0",
        "second DispatchAttemptRecorded                    0",
        "The scalar `outcome_append_count` from R2 is removed",
        "Scenarios S01, S02, S03, S08, S10 and S11 require",
        "target LaterFilled V3 delta                  0",
        "request outcome V3 delta                     1",
        "Both P1D4C-061 (S09/F02) and P1D4C-085 (S09/F20)",
        "target LaterFilled V3 delta                  1",
        "recovered CANCEL V3 delta                    1",
        "total P1-d3 V3 delta                         2",
        "P1d3CancelContinuationPending audit",
        "source XACK                                  exactly last",
        "credited as final completion",
        "The final audit must carry both typed V3 record",
        "IDs and evidence hashes in order",
        "operational Redis DB0/VPS", "P1-e remains unauthorized",
    )
    for token in required_r3:
        require(token in r3, f"missing R3 invariant: {token}")
    require(sha256(r3) == R3_DESIGN_SHA256, "R3 design byte hash drifted")

    require(sha256(matrix_text) == GENERAL_MATRIX_SHA256, "general matrix byte hash drifted")
    rows = list(csv.DictReader(matrix_text.splitlines()))
    require(len(rows) == 88, f"general acceptance count drifted: {len(rows)}")
    require([row.get("id") for row in rows] == [f"P1D4D-{i:03d}" for i in range(1, 89)], "general IDs/order drifted")
    require(all(row.get("status") == "REQUIRED" for row in rows), "general acceptance weakened")
    general = {row["id"]: row["requirement"] for row in rows}
    corrected_mapping_tokens = {
        "P1D4D-035": "F00 reclaims", "P1D4D-036": "F01 rebinds",
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
    r3_general_tokens = {
        "P1D4D-011": "92 exact cells", "P1D4D-012": "21 frontier IDs F00 through F20",
        "P1D4D-027": "P1D4C-088", "P1D4D-028": "P1D4C-089",
        "P1D4D-030": "P1D4C-090 through P1D4C-092",
        "P1D4D-073": "exact P1-d3 PLACE or CANCEL command",
        "P1D4D-074": "typed target plus recovered-cancel V3 outcomes for S09",
        "P1D4D-075": "sequence reservation", "P1D4D-076": "exact eight-field schema",
        "P1D4D-077": "only child_pid", "P1D4D-078": "new P1-d4 exact-protocol hooks",
        "P1D4D-079": "P1D4C-088 through P1D4C-092", "P1D4D-080": "Accepted R3",
        "P1D4D-081": "exact Limit Day no TTL positive integral quantity positive limit price",
        "P1D4D-082": "unique BOID registry row canonical optional target TCID distinct cancel DCID",
        "P1D4D-083": "Market non-Day LIMIT LIMIT with TTL",
        "P1D4D-084": "cannot intercept accepted P1-d1 or P1-d2 Market recovery",
        "P1D4D-085": "no P1d3DispatchPending authority V3 second dispatch",
        "P1D4D-086": "exactly one typed request V3 and one RequestFinalized row",
        "P1D4D-087": "one target LaterFilled V3 and one recovered CANCEL V3",
        "P1D4D-088": "P1d3CancelContinuationPending",
    }
    for row_id, token in r3_general_tokens.items():
        require(token in general[row_id], f"general R3 requirement drifted: {row_id}")

    require(sha256(cell_matrix_text) == CELL_MATRIX_SHA256, "R3 cell matrix byte hash drifted")
    cell_rows = list(csv.DictReader(cell_matrix_text.splitlines()))
    require(cell_rows and list(cell_rows[0]) == CELL_FIELDS, "cell fields drifted")
    require(len(cell_rows) == 92, f"cell count drifted: {len(cell_rows)}")
    require([row["cell_id"] for row in cell_rows] == [f"P1D4C-{i:03d}" for i in range(1, 93)], "cell IDs/order drifted")
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
    require(all("without_second_dispatch" in row["only_legal_continuation"] for row in request_f02), "request F02 permits duplicate dispatch")
    require(by_cell["P1D4C-042"]["expected_restart_disposition"] == "Ready", "autonomous F02 owner drifted")
    for cell_id in ("P1D4C-061", "P1D4C-085"):
        row = by_cell[cell_id]
        require("append_target_laterfilled_v3" in row["only_legal_continuation"], f"S09 target outcome missing: {cell_id}")
        require("enter_cancel_continuation" in row["only_legal_continuation"], f"S09 cancel continuation missing: {cell_id}")
        require("append_recovered_cancel_v3" in row["only_legal_continuation"], f"S09 cancel outcome missing: {cell_id}")
        require("without_second_dispatch" in row["only_legal_continuation"], f"S09 second dispatch opened: {cell_id}")
        require(row["sequence_expectation"] == "dispatch_1_to_1_target_v3_delta_1_cancel_v3_delta_1_total_v3_delta_2_request_finalized_delta_1_target_s_terminal_delta_1_s_cancel_recovered_delta_1", f"S09 typed deltas drifted: {cell_id}")
    exact_added = {
        "P1D4C-081": ("S01", "F20", "p1d4-s01-f20"), "P1D4C-082": ("S02", "F20", "p1d4-s02-f20"),
        "P1D4C-083": ("S03", "F20", "p1d4-s03-f20"), "P1D4C-084": ("S08", "F20", "p1d4-s08-f20"),
        "P1D4C-085": ("S09", "F20", "p1d4-s09-f20"), "P1D4C-086": ("S10", "F20", "p1d4-s10-f20"),
        "P1D4C-087": ("S11", "F20", "p1d4-s11-f20"), "P1D4C-088": ("S06", "F07", "p1d4-s06-f07"),
        "P1D4C-089": ("S07", "F07", "p1d4-s07-f07"), "P1D4C-090": ("S09", "F07", "p1d4-s09-target-f07"),
        "P1D4C-091": ("S09", "F03", "p1d4-s09-cancel-f03"), "P1D4C-092": ("S09", "F04", "p1d4-s09-cancel-f04"),
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

    require(evidence.get("stage") == "Stage 8B-P1-d4 exhaustive crash/replay closure design", "stage drifted")
    require(evidence.get("status") == "DESIGN_R3_REVIEW_CANDIDATE", "status drifted")
    require(evidence.get("accepted_p1d3_closure_ref") == BASE, "lineage drifted")
    require(evidence.get("reviewed_r0_ref") == R0_REF and evidence.get("reviewed_r1_ref") == R1_REF and evidence.get("reviewed_r2_ref") == R2_REF, "review lineage drifted")
    require(evidence.get("acceptance_rows") == 88 and evidence.get("acceptance_matrix_sha256") == GENERAL_MATRIX_SHA256, "general evidence drifted")
    require(evidence.get("scenario_frontier_matrix_rows") == 92 and evidence.get("scenario_frontier_matrix_sha256") == CELL_MATRIX_SHA256, "cell evidence drifted")
    require(evidence.get("frontier_count") == 21 and evidence.get("minimum_semantic_families") == 11, "coverage evidence drifted")
    require(evidence.get("negative_cases") == 128, "negative inventory drifted")
    require(evidence.get("design_only") is True and evidence.get("implementation_authorized") is False, "design boundary opened")
    require(evidence.get("design_r3_sha256") == R3_DESIGN_SHA256, "R3 design evidence binding drifted")
    require(evidence.get("required_frontiers") == [f"F{i:02d}" for i in range(21)], "frontier evidence drifted")

    dispatch = evidence.get("dispatch_only_recovery_contract")
    require(isinstance(dispatch, dict), "dispatch contract missing")
    require("outcome_append_count" not in dispatch, "impossible scalar outcome count restored")
    expected_domain = {
        "place_required": [
            "exact_place_snapshot", "order_type_limit", "time_in_force_day", "ttl_none",
            "positive_integral_qty", "positive_limit_price", "authenticated_p1d3_semantic_package_book",
        ],
        "cancel_required": [
            "exact_cancel_snapshot", "ttl_none", "unique_target_boid", "canonical_optional_target_tcid",
            "cancel_dcid_distinct_from_target_tcid", "authenticated_p1d3_semantic_package_book",
        ],
        "excluded_place_shapes": [
            "market", "non_day_limit", "limit_with_ttl", "nonpositive_or_fractional_qty",
            "missing_or_nonpositive_limit_price", "missing_authenticated_p1d3_book",
        ],
        "order": [
            "p1d3_v3_journal_ahead", "p1d2_journal_ahead", "generic_p1_journal_ahead",
            "p1d3_dispatch_only", "blocked_fallback",
        ],
    }
    expected_outcomes = {
        "simple_request": {
            "scenarios": SIMPLE_SCENARIOS, "dispatch_before": 1, "dispatch_after": 1,
            "target_later_filled_v3_delta": 0, "request_outcome_v3_delta": 1,
            "total_v3_delta": 1, "request_finalized_delta": 1,
        },
        "s09_target_first": {
            "scenarios": ["S09"], "dispatch_before": 1, "dispatch_after": 1,
            "target_later_filled_v3_delta": 1, "recovered_cancel_v3_delta": 1,
            "total_v3_delta": 2, "request_finalized_delta": 1,
            "required_intermediate_disposition": "P1d3CancelContinuationPending",
        },
    }
    require(dispatch.get("classifier") == "classify_stage8b_p1d3_dispatch_only_candidate", "dispatch classifier drifted")
    require(dispatch.get("classifier_domain") == expected_domain, "classifier domain drifted")
    require(dispatch.get("scenario_outcome_contracts") == expected_outcomes, "scenario outcome contracts drifted")
    require(dispatch.get("expected_restart_disposition") == "P1d3DispatchPending", "dispatch owner drifted")
    require(dispatch.get("second_dispatch_allowed") is False and dispatch.get("normal_path_changed") is False, "dispatch boundary weakened")
    require(dispatch.get("source_delta_after_r3_acceptance") is True, "R3 source delta hidden")

    marker = evidence.get("marker_contract")
    require(isinstance(marker, dict), "marker contract missing")
    require(marker.get("schema_version") == 1 and marker.get("field_count") == 8, "marker schema drifted")
    require(marker.get("domain") == "moex.stage8b.p1d4.crash-marker.v1", "marker domain drifted")
    require(marker.get("normalized_digest_domain") == "moex.stage8b.p1d4.crash-marker.normalized.v1", "marker digest domain drifted")
    require(marker.get("normalized_fields") == ["/child_pid"], "marker normalization widened")
    test = evidence.get("test_contract")
    require(isinstance(test, dict), "test contract missing")
    for key in ("actual_child_process_kill", "child_has_no_descendants", "ephemeral_loopback_redis", "fsync_backed_pre_kill_marker", "non_returning_barrier", "response_loss_after_real_xack", "wait_and_reap_before_restart", "p1d3_dispatch_only_recovery_delta_after_r3_acceptance"):
        require(test.get(key) is True, f"test contract weakened: {key}")
    require(test.get("kernel_observed_exit_signal") == 9, "SIGKILL drifted")
    require(test.get("p1d3_business_semantics_mutable") is False, "business semantics opened")
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

    for token in (R2_REF, "active R3 design-only correction", "92-cell registry", "two typed V3", "P1d3CancelContinuationPending"):
        require(token in status, f"status drifted: {token}")
    for token in (R2_REF, "active P1-d4 R3 design-only correction", "non-interception of P1-d2 Market", "recovered CANCEL V3 for S09"):
        require(token in roadmap, f"roadmap drifted: {token}")


def main() -> None:
    try:
        actual = changed_files()
        require(actual == EXPECTED_CHANGED, f"R3 design changed path drift: {sorted(actual)}")
        validate(
            DESIGN.read_text(encoding="utf-8"), R1.read_text(encoding="utf-8"),
            R2.read_text(encoding="utf-8"), R3.read_text(encoding="utf-8"),
            MATRIX.read_text(encoding="utf-8"), R1_CELL_MATRIX.read_text(encoding="utf-8"),
            R2_CELL_MATRIX.read_text(encoding="utf-8"), CELL_MATRIX.read_text(encoding="utf-8"),
            json.loads(EVIDENCE.read_text(encoding="utf-8")),
            STATUS.read_text(encoding="utf-8"), ROADMAP.read_text(encoding="utf-8"),
        )
    except (OSError, json.JSONDecodeError, subprocess.CalledProcessError, CheckFailure) as error:
        print(f"stage8b-p1d4-r3-design-check: FAIL {error}", file=sys.stderr)
        raise SystemExit(1)
    print("PASS stage8b-p1d4-r3-design-scope files=20 general_rows=88 cells=92 frontiers=21 scenarios=11")


if __name__ == "__main__":
    main()
