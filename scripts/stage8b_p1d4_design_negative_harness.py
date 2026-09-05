#!/usr/bin/env python3
"""Targeted mutation harness for the P1-d4 R1 design contract."""

from __future__ import annotations

import csv
import io
import json

import stage8b_p1d4_design_check as checker


def replace_once(value: str, old: str, new: str) -> str:
    if old not in value:
        raise RuntimeError(f"mutation anchor missing: {old!r}")
    return value.replace(old, new, 1)


design = checker.DESIGN.read_text(encoding="utf-8")
r1 = checker.R1.read_text(encoding="utf-8")
matrix = checker.MATRIX.read_text(encoding="utf-8")
cells = checker.CELL_MATRIX.read_text(encoding="utf-8")
evidence = json.loads(checker.EVIDENCE.read_text(encoding="utf-8"))
status = checker.STATUS.read_text(encoding="utf-8")
roadmap = checker.ROADMAP.read_text(encoding="utf-8")


def mutate_evidence(path: tuple[str, ...], value: object) -> dict[str, object]:
    result = json.loads(json.dumps(evidence))
    cursor: dict[str, object] = result
    for name in path[:-1]:
        cursor = cursor[name]  # type: ignore[assignment]
    cursor[path[-1]] = value
    return result


Case = tuple[str, str, str, str, str, dict[str, object], str, str]
cases: list[Case] = []

for name, old, new in [
    ("finite-cell-count", "exactly 80 rows with IDs `P1D4C-001..P1D4C-080`", "an implementation-selected row set"),
    ("scenario-frontier-count", "11\nscenario IDs and 20 frontier IDs `F00..F19`", "some scenarios and frontiers"),
    ("matrix-hash", checker.CELL_MATRIX_SHA256, "0" * 64),
    ("initial-expiry-xack", "S03 retains its originating command M10", "S03 has no source XACK"),
    ("day-expiry-source", "Only S07 has no new Redis\nsource", "S07 creates a Redis source"),
    ("zero-intent", "S04 proves a zero-intent same-bar callback", "S04 omits callback shape"),
    ("one-intent", "S05 proves a one-intent callback", "S05 may use zero intent"),
    ("inherited-redis-blob", "5ceca40f8bbb3cb9f2dc61a1ebf43c617fbbd0d9", "0" * 40),
    ("inherited-recovery-blob", "27b0edada9ef05bde8b44ba77f321a57bf729d54", "0" * 40),
    ("pre-wal-frontiers", "F02, F12 and F17 are intentionally pre-write-ahead frontiers", "F02 F12 F17 already have WAL"),
    ("forbid-pre-wal-reexecution", "Phase-scoped deterministic re-execution", "Reexecution is always forbidden"),
    ("reissue-other-source", "new one-use authority only for the\nsame authenticated source/boundary", "new authority for any current source"),
    ("allow-post-wal-reissue", "At and after F03, F13 or F18", "Only after final XACK"),
    ("invent-untouched-v3", "there is no Stage6 V3 autonomous outcome", "untouched has a Stage6 V3 outcome"),
    ("expiry-post-seal-xack", "F19 returns exact Ready without Redis source acquisition", "F19 performs XACK"),
    ("returning-barrier", "Child enters a non-returning barrier", "Child returns from the barrier"),
    ("pid-mismatch", "marker PID equals\n   `child.id()`", "marker PID is diagnostic"),
    ("normal-exit", "status.code() == None", "status.code() may be zero"),
    ("sigterm", "ExitStatusExt::signal() == Some(9)", "any termination signal"),
    ("restart-before-reap", "Only after successful reap", "Restart may begin before wait"),
    ("xack-pre-send-loss", "receive and parse Redis's successful integer XACK\nreply (`1`", "disconnect before XACK send"),
    ("remove-evidence-version", "Stage8bP1d4CrashReplayEvidenceV1", "unversioned evidence map"),
    ("change-evidence-domain", "moex.stage8b.p1d4.crash-replay.evidence.v1", "moex.stage8b.p1d4.crash-replay.evidence.v2"),
    ("exclude-signal-as-volatile", "/cells/*/process/child_pid", "/cells/*/process/exit_signal"),
    ("broad-field-exclusion", "No other field may be removed", "Any unstable field may be removed"),
    ("exclude-pass-audits", "passed`, exit signal/code, normalized marker digest", "only cell IDs remain"),
    ("noncanonical-json", "recursive UTF-8 bytewise lexical object\nkey ordering", "implementation-defined object ordering"),
    ("change-semantic-domain", "moex.stage8b.p1d4.crash-replay.semantic-evidence.v1\\0", "semantic-evidence.v2"),
    ("single-repro-run", "Two clean runs must have identical semantic digests", "One run is enough"),
    ("remove-signal-negative", "SIGTERM, normal exit, returning barrier, kill-before-marker", "panic-only negative"),
]:
    cases.append((name, design, replace_once(r1, old, new), matrix, cells, evidence, status, roadmap))

cell_rows = cells.splitlines()
cell_header, cell_data = cell_rows[0], cell_rows[1:]
cell_mutations = [
    ("cell-delete", "\n".join([cell_header] + cell_data[:-1]) + "\n"),
    ("cell-duplicate", "\n".join([cell_header] + cell_data + [cell_data[-1]]) + "\n"),
    ("cell-reorder", "\n".join([cell_header, cell_data[1], cell_data[0]] + cell_data[2:]) + "\n"),
    ("cell-id", replace_once(cells, "P1D4C-030", "P1D4C-999")),
    ("cell-scenario", replace_once(cells, ",S03,initial_limit_expired,", ",S99,initial_limit_expired,")),
    ("cell-frontier", replace_once(cells, ",F16,redis_xack_reply_1", ",F15,redis_xack_reply_1")),
    ("cell-precondition", replace_once(cells, "command_pending_before_observation", "command_already_executed")),
    ("cell-kill-hook", replace_once(cells, "p1d4-s01-f00", "p1d4-generic")),
    ("cell-restart-disposition", replace_once(cells, "P1d3PreAckPending", "Ready")),
    ("cell-continuation", replace_once(cells, "commit_exact_truth_only", "perform_xack_early")),
    ("cell-sequence", replace_once(cells, "exact_reserved_ack_truth_pair_unchanged", "allocate_new_pair")),
    ("cell-callback", replace_once(cells, "+1_on_only_legal_continuation", "+2")),
    ("cell-schedule", replace_once(cells, "+1_equivalent_reissue_only", "unbounded_reissue")),
    ("cell-xack", replace_once(cells, "forbidden_no_source", "xack_allowed")),
    ("cell-inherited-binding", replace_once(cells, "inherited:p1d3_subprocess_sigkill_brackets_s_cancel_recovered@7dc7c80", "inherited:latest")),
]
for name, changed_cells in cell_mutations:
    cases.append((name, design, r1, matrix, changed_cells, evidence, status, roadmap))

for name, path, value in [
    ("evidence-status", ("status",), "ACCEPTED"),
    ("evidence-r0", ("reviewed_r0_ref",), "0" * 40),
    ("evidence-cell-count", ("scenario_frontier_matrix_rows",), 79),
    ("evidence-cell-hash", ("scenario_frontier_matrix_sha256",), "0" * 64),
    ("evidence-frontiers", ("frontier_count",), 19),
    ("evidence-families", ("minimum_semantic_families",), 10),
    ("evidence-opens-source", ("implementation_authorized",), True),
    ("evidence-sigterm", ("test_contract", "kernel_observed_exit_signal"), 15),
    ("evidence-descendants", ("test_contract", "child_has_no_descendants"), False),
    ("evidence-returning", ("test_contract", "non_returning_barrier"), False),
    ("evidence-volatile-pass", ("evidence_contract", "volatile_json_pointers"), ["/cells/*/passed"]),
    ("evidence-opens-db0", ("closed_surfaces", "operational_redis_db0"), True),
]:
    cases.append((name, design, r1, matrix, cells, mutate_evidence(path, value), status, roadmap))

rows = list(csv.DictReader(matrix.splitlines()))
rows[24]["requirement"] = "Initial Expired has no command-source response-loss case"
buffer = io.StringIO()
writer = csv.DictWriter(buffer, fieldnames=["id", "area", "requirement", "status"])
writer.writeheader()
writer.writerows(rows)
cases.extend([
    ("general-initial-expiry-escape", design, r1, buffer.getvalue(), cells, evidence, status, roadmap),
    ("status-drift", design, r1, matrix, cells, evidence, replace_once(status, "active R1 design-only correction", "active implementation"), roadmap),
    ("roadmap-drift", design, r1, matrix, cells, evidence, status, replace_once(roadmap, "active P1-d4 R1 design-only correction", "active P1-e deployment")),
])

if len(cases) != 60:
    raise SystemExit(f"mutation inventory drifted: {len(cases)}")

escaped: list[str] = []
for name, case_design, case_r1, case_matrix, case_cells, case_evidence, case_status, case_roadmap in cases:
    try:
        checker.validate(case_design, case_r1, case_matrix, case_cells, case_evidence, case_status, case_roadmap)
    except checker.CheckFailure:
        print(f"PASS {name}")
    else:
        escaped.append(name)
        print(f"FAIL {name}")

if escaped:
    raise SystemExit("mutations escaped: " + ", ".join(escaped))
print("PASS stage8b-p1d4-r1-design-negative-harness 60/60")
