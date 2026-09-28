#!/usr/bin/env python3
"""Targeted mutations for the P1-d4 R4 design correction."""

from __future__ import annotations

import copy
import csv
import io

import stage8b_p1d4_r4_design_check as checker


(
    r3_design,
    r4_design,
    discovery,
    general_matrix,
    r3_matrix,
    r4_matrix,
    r3_evidence,
    evidence,
    status,
    roadmap,
) = checker.read_inputs()


def replace_once(value: str, old: str, new: str) -> str:
    if old not in value:
        raise RuntimeError(f"mutation anchor missing: {old!r}")
    return value.replace(old, new, 1)


def changed_cell(cell_id: str, field: str, value: str) -> str:
    rows = list(csv.DictReader(r4_matrix.splitlines()))
    for row in rows:
        if row["cell_id"] == cell_id:
            row[field] = value
            break
    else:
        raise RuntimeError(f"cell missing: {cell_id}")
    output = io.StringIO(newline="")
    writer = csv.DictWriter(output, fieldnames=list(rows[0]), lineterminator="\n")
    writer.writeheader()
    writer.writerows(rows)
    return output.getvalue()


Case = tuple[
    str, str, str, str, str, str, str, str, dict[str, object], str, str
]
cases: list[Case] = []


def add(
    name: str,
    *,
    case_r4_design: str = r4_design,
    case_discovery: str = discovery,
    case_r4_matrix: str = r4_matrix,
    case_evidence: dict[str, object] = evidence,
    case_status: str = status,
    case_roadmap: str = roadmap,
) -> None:
    cases.append((
        name,
        r3_design,
        case_r4_design,
        case_discovery,
        general_matrix,
        r3_matrix,
        case_r4_matrix,
        r3_evidence,
        case_evidence,
        case_status,
        case_roadmap,
    ))


for name, old, new in (
    ("design-status", "Status: R4 design-only review candidate", "Status: implemented"),
    ("design-r3-lineage", checker.R3_REF, "0" * 40),
    ("design-durable-equivalence", "have the same authenticated durable", "may have different durable"),
    ("design-volatile-ack", "An ACK that existed only in memory cannot participate", "An in-memory ACK participates"),
    ("design-source-independence", "belongs to its own command M10 source", "belongs to the later-bar source"),
    ("design-marker-authority", "reading a crash marker or test environment from production recovery", "reading unrelated metadata"),
):
    add(name, case_r4_design=replace_once(r4_design, old, new))

add(
    "matrix-c039-recoupled",
    case_r4_matrix=changed_cell(
        "P1D4C-039",
        "only_legal_continuation",
        "confirm_exact_existing_publication_then_complete_generated_command_lifecycle_then_bar_xack",
    ),
)
add(
    "matrix-c040-wrong-owner",
    case_r4_matrix=changed_cell("P1D4C-040", "expected_restart_disposition", "P1d3TruthCommitted"),
)
add(
    "matrix-c040-callback-replay",
    case_r4_matrix=changed_cell("P1D4C-040", "only_legal_continuation", "replay_callback_and_publish_again"),
)
add(
    "matrix-c064-volatile-owner",
    case_r4_matrix=changed_cell("P1D4C-064", "expected_restart_disposition", "P1d3CancelContinuationPending"),
)
add(
    "matrix-unrelated-cell-drift",
    case_r4_matrix=changed_cell("P1D4C-063", "expected_restart_disposition", "P1d3PreAckPending"),
)

mutated = copy.deepcopy(evidence)
mutated["status"] = "ACCEPTED"
add("evidence-status", case_evidence=mutated)

mutated = copy.deepcopy(evidence)
mutated["correction_contract"]["exact_changed_fields"].pop()  # type: ignore[index,union-attr]
add("evidence-correction-set", case_evidence=mutated)

mutated = copy.deepcopy(evidence)
mutated["correction_contract"]["durable_equivalence"]["forbidden_restart_inputs"] = ["crash_marker"]  # type: ignore[index]
add("evidence-forbidden-restart-inputs", case_evidence=mutated)

mutated = copy.deepcopy(evidence)
mutated["correction_contract"]["source_independence"]["new_combined_schema_allowed"] = True  # type: ignore[index]
add("evidence-combined-schema", case_evidence=mutated)

mutated = copy.deepcopy(evidence)
mutated["closed_surfaces"]["operational_redis_db0"] = True  # type: ignore[index]
add("evidence-opens-db0", case_evidence=mutated)

add(
    "status-opens-source",
    case_status=replace_once(status, "implementation is paused", "implementation is accepted"),
)
add(
    "roadmap-opens-p1e",
    case_roadmap=replace_once(roadmap, "Redis DB0/VPS", "operational deployment"),
)

if len(cases) != 18:
    raise SystemExit(f"R4 mutation inventory drifted: {len(cases)}")

escaped: list[str] = []
for case in cases:
    name, *values = case
    try:
        checker.validate(*values)
    except checker.CheckFailure:
        print(f"PASS {name}")
    else:
        escaped.append(name)
        print(f"FAIL {name}")

if escaped:
    raise SystemExit("mutations escaped: " + ", ".join(escaped))
print("PASS stage8b-p1d4-r4-design-negative-harness 18/18 inherited_r3=128 aggregate_contract=146")
