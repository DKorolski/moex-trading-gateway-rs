#!/usr/bin/env python3
"""Semantic mutations for the P1-d4 R5 retained-source contract."""

from __future__ import annotations

import copy
import csv
import io

import stage8b_p1d4_r5_design_check as checker


baseline = checker.read_inputs()


def replace_once(value: str, old: str, new: str) -> str:
    if old not in value:
        raise RuntimeError(f"mutation anchor missing: {old!r}")
    return value.replace(old, new, 1)


def csv_rows(value: str) -> tuple[list[str], list[dict[str, str]]]:
    reader = csv.DictReader(value.splitlines())
    return list(reader.fieldnames or []), list(reader)


def emit_csv(fields: list[str], rows: list[dict[str, str]]) -> str:
    output = io.StringIO(newline="")
    writer = csv.DictWriter(output, fieldnames=fields, lineterminator="\n")
    writer.writeheader()
    writer.writerows(rows)
    return output.getvalue()


def mutate_cell(value: str, key: str, cell_id: str, field: str, replacement: str) -> str:
    fields, rows = csv_rows(value)
    for row in rows:
        if row[key] == cell_id:
            row[field] = replacement
            return emit_csv(fields, rows)
    raise RuntimeError(f"cell missing: {cell_id}")


cases: list[tuple[str, tuple[object, ...]]] = []


def add(name: str, index: int, value: object) -> None:
    inputs = list(baseline)
    inputs[index] = value
    cases.append((name, tuple(inputs)))


design = baseline[5]
for name, old, new in (
    ("design-status-opened", "Status: R5 design-only review candidate", "Status: implemented"),
    ("design-option-b", "Option A", "Option B"),
    ("design-second-source", "There is one later-bar M10 source", "There are two M10 sources"),
    ("design-early-xack", "XACK is last", "XACK may precede truth"),
    ("design-projection-removed", "Stage8bP1d4GeneratedMarketCompositionV1", "RemovedComposition"),
    ("design-projection-embedded", "peer of, not a field inside", "field inside"),
    ("design-raw-stage5c", "contains no raw", "contains raw"),
    ("design-owner-removed", "P1d4GeneratedMarketTruthCommitted", "GenericReady"),
):
    add(name, 5, replace_once(design, old, new))

discovery = baseline[6]
add("discovery-second-source", 6, replace_once(discovery, "does not create a second M10 source", "creates a second M10 source"))
add("discovery-new-journal", 6, replace_once(discovery, "No new Stage6 journal record is required", "A new Stage6 journal record is required"))

base_matrix = baseline[2]
add("base-f15-generic-owner", 2, mutate_cell(base_matrix, "cell_id", "P1D4C-039", "expected_restart_disposition", "P1SemanticPrepublicationReady"))
add("base-f15-early-xack", 2, mutate_cell(base_matrix, "cell_id", "P1D4C-039", "xack_expectation", "only_legal_after_current_covering_seal"))
add("base-f15-source-absent", 2, mutate_cell(base_matrix, "cell_id", "P1D4C-039", "pel_after", "exact_source_absent"))
add("base-f16-wrong-owner", 2, mutate_cell(base_matrix, "cell_id", "P1D4C-040", "expected_restart_disposition", "P1d3TruthCommitted"))
add("base-c1-regression", 2, mutate_cell(base_matrix, "cell_id", "P1D4C-064", "expected_restart_disposition", "P1d3CancelContinuationPending"))

gm_fields, gm_rows = csv_rows(baseline[3])
add("gm-row-missing", 3, emit_csv(gm_fields, gm_rows[:-1]))
add("gm-id-drift", 3, mutate_cell(baseline[3], "cell_id", "P1D4GM-001", "cell_id", "P1D4GM-999"))
add("gm-frontier-drift", 3, mutate_cell(baseline[3], "cell_id", "P1D4GM-003", "frontier_id", "GM99"))
add("gm-parent-drift", 3, mutate_cell(baseline[3], "cell_id", "P1D4GM-004", "parent_scenario_id", "S04"))
add("gm-source-missing", 3, mutate_cell(baseline[3], "cell_id", "P1D4GM-006", "source_pel_before", "exact_source_absent"))
add("gm-early-xack", 3, mutate_cell(baseline[3], "cell_id", "P1D4GM-008", "xack_delta", "+1_exact"))
add("gm-terminal-xack-missing", 3, mutate_cell(baseline[3], "cell_id", "P1D4GM-010", "xack_delta", "0"))
add("gm-terminal-owner", 3, mutate_cell(baseline[3], "cell_id", "P1D4GM-010", "expected_restart_disposition", "Ready"))
add("gm-duplicate-coverage", 3, mutate_cell(baseline[3], "cell_id", "P1D4GM-002", "duplicate_variant_required", "false"))
add("gm-conflict-coverage", 3, mutate_cell(baseline[3], "cell_id", "P1D4GM-009", "conflict_variant_required", "false"))

amend_fields, amend_rows = csv_rows(baseline[4])
add("amendment-row-missing", 4, emit_csv(amend_fields, amend_rows[:-1]))
add("amendment-optional", 4, mutate_cell(baseline[4], "id", "P1D4R5-005", "status", "OPTIONAL"))

for name, path, replacement in (
    ("evidence-option-b", ("composition_contract", "option"), "B_SOURCE_ABSENT"),
    ("evidence-second-source", ("composition_contract", "source_m10_count"), 2),
    ("evidence-raw-stage5c", ("composition_contract", "raw_stage5c_capability_embedded"), True),
    ("evidence-opens-db0", ("closed_surfaces", "operational_redis_db0"), True),
):
    mutated = copy.deepcopy(baseline[7])
    mutated[path[0]][path[1]] = replacement  # type: ignore[index]
    add(name, 7, mutated)

add("status-opens-source", 8, replace_once(baseline[8], "paused pending independent R5 acceptance", "accepted for deployment"))

if len(cases) != 32:
    raise SystemExit(f"R5 mutation inventory drifted: {len(cases)}")

escaped: list[str] = []
for name, inputs in cases:
    try:
        checker.validate(*inputs, verify_hashes=False)
    except checker.CheckFailure:
        print(f"PASS {name}")
    else:
        escaped.append(name)
        print(f"FAIL {name}")

if escaped:
    raise SystemExit("mutations escaped: " + ", ".join(escaped))
print("PASS stage8b-p1d4-r5-design-negative-harness 32/32 inherited_r3=128 aggregate_contract=160")
