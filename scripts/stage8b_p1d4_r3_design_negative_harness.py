#!/usr/bin/env python3
"""Inherited R2 104 cases plus 24 targeted P1-d4 R3 mutations."""

from __future__ import annotations

import csv
import io
import json

import stage8b_p1d4_r3_design_check as checker


def replace_once(value: str, old: str, new: str) -> str:
    if old not in value:
        raise RuntimeError(f"mutation anchor missing: {old!r}")
    return value.replace(old, new, 1)


def replace_all(value: str, old: str, new: str) -> str:
    if old not in value:
        raise RuntimeError(f"mutation anchor missing: {old!r}")
    return value.replace(old, new)


design = checker.DESIGN.read_text(encoding="utf-8")
r1 = checker.R1.read_text(encoding="utf-8")
r2 = checker.R2.read_text(encoding="utf-8")
r3 = checker.R3.read_text(encoding="utf-8")
matrix = checker.MATRIX.read_text(encoding="utf-8")
r1_cells = checker.R1_CELL_MATRIX.read_text(encoding="utf-8")
r2_cells = checker.R2_CELL_MATRIX.read_text(encoding="utf-8")
cells = checker.CELL_MATRIX.read_text(encoding="utf-8")
evidence = json.loads(checker.EVIDENCE.read_text(encoding="utf-8"))
status = checker.STATUS.read_text(encoding="utf-8")
roadmap = checker.ROADMAP.read_text(encoding="utf-8")

Case = tuple[
    str, str, str, str, str, str, str, str, str,
    dict[str, object], str, str,
]
cases: list[Case] = []


def add(
    name: str,
    *,
    case_design: str = design,
    case_r1: str = r1,
    case_r2: str = r2,
    case_r3: str = r3,
    case_matrix: str = matrix,
    case_r1_cells: str = r1_cells,
    case_r2_cells: str = r2_cells,
    case_cells: str = cells,
    case_evidence: dict[str, object] = evidence,
    case_status: str = status,
    case_roadmap: str = roadmap,
) -> None:
    cases.append((
        name, case_design, case_r1, case_r2, case_r3, case_matrix,
        case_r1_cells, case_r2_cells, case_cells, case_evidence,
        case_status, case_roadmap,
    ))


def mutate_evidence(path: tuple[str, ...], value: object) -> dict[str, object]:
    result = json.loads(json.dumps(evidence))
    cursor: dict[str, object] = result
    for name in path[:-1]:
        cursor = cursor[name]  # type: ignore[assignment]
    cursor[path[-1]] = value
    return result


def mutate_evidence_list(path: tuple[str, ...], index: int, value: object) -> dict[str, object]:
    result = json.loads(json.dumps(evidence))
    cursor: object = result
    for name in path:
        cursor = cursor[name]  # type: ignore[index]
    cursor[index] = value  # type: ignore[index]
    return result


general_rows = list(csv.DictReader(matrix.splitlines()))


def changed_general(row_id: str) -> str:
    rows = [dict(row) for row in general_rows]
    for row in rows:
        if row["id"] == row_id:
            row["requirement"] = "WEAKENED OR LEGACY CONTRACT"
            break
    else:
        raise RuntimeError(f"general row missing: {row_id}")
    buffer = io.StringIO(newline="")
    writer = csv.DictWriter(
        buffer, fieldnames=["id", "area", "requirement", "status"],
        lineterminator="\n",
    )
    writer.writeheader()
    writer.writerows(rows)
    return buffer.getvalue()


cell_rows = list(csv.DictReader(cells.splitlines()))


def write_cells(rows: list[dict[str, str]]) -> str:
    buffer = io.StringIO(newline="")
    writer = csv.DictWriter(buffer, fieldnames=checker.CELL_FIELDS, lineterminator="\n")
    writer.writeheader()
    writer.writerows(rows)
    return buffer.getvalue()


# The first 104 cases preserve the complete R2 mutation inventory against the
# active R3 artifacts. Historical R2 design bytes are immutable, while matrix,
# evidence and status mutations are replayed against the R3 replacements.
for name, old, new in [
    ("inherited-r2-status", "Status: R2 design-only review candidate", "Status: accepted"),
    ("inherited-r2-cell-count", "cells: 92", "cells: implementation-selected"),
    ("inherited-r2-frontier-range", "frontiers: F00..F20", "frontiers: F00..F19"),
    ("inherited-r2-derived-count", "number 92 is derived from the complete reviewed rows", "number 92 is a target"),
    ("inherited-r2-general-alignment", "General requirements P1D4D-035..P1D4D-056 now use the same", "General requirements may use legacy meanings"),
    ("inherited-r2-dispatch-frontier-distinction", "F20 and request-scoped F02 have the same durable journal suffix", "F20 and F02 are one synthetic hook"),
    ("inherited-r2-dispatch-candidate", "Stage6Stage8bP1d3DispatchOnlyCandidate", "GenericCandidate"),
    ("inherited-r2-dispatch-classifier", "classify_stage8b_p1d3_dispatch_only_candidate", "classify_any_suffix"),
    ("inherited-r2-dispatch-owner", "Stage8bP1d3DispatchPendingOwner", "ReadyOwner"),
    ("inherited-r2-dispatch-outcome", "Stage7bRestartOutcome::P1d3DispatchPending", "Stage7bRestartOutcome::Ready"),
    ("inherited-r2-dispatch-resume", "resume_stage8b_p1d3_dispatch_pending", "resume_generic"),
    ("inherited-r2-second-dispatch", "must not call either dispatch-append helper", "may append dispatch again"),
    ("inherited-r2-dispatch-count", "dispatch rows                 1 -> 1", "dispatch rows                 1 -> 2"),
    ("inherited-r2-s06-f07", "P1D4C-088 | S06/F07", "P1D4C-088 | omitted"),
    ("inherited-r2-s09-f04", "P1D4C-092 | S09/F04", "P1D4C-092 | omitted"),
    ("inherited-r2-s11-new-hooks", "P1D4C-078 and P1D4C-079 require new hooks", "P1D4C-078 and P1D4C-079 use inherited completion"),
    ("inherited-r2-marker-schema", "schema_version:             integer 1", "schema_version:             any integer"),
    ("inherited-r2-marker-domain", 'domain:                     "moex.stage8b.p1d4.crash-marker.v1"', 'domain:                     "marker"'),
    ("inherited-r2-marker-normalization", "only `child_pid` is replaced by integer", "all unstable fields are removed"),
    ("inherited-r2-marker-digest-domain", "moex.stage8b.p1d4.crash-marker.normalized.v1\\0", "marker\\0"),
    ("inherited-r2-old-marker-substitution", "Substitution of the old marker", "Use of the old marker"),
    ("inherited-r2-general-row-negatives", "each corrected general row P1D4D-035..P1D4D-056", "some corrected rows"),
    ("inherited-r2-open-db0", "operational Redis DB0/VPS", "deployment surface may open"),
    ("inherited-r2-open-p1e", "P1-e remains unauthorized", "P1-e is authorized"),
    ("inherited-r2-normal-path-mutation", "normal-path ordering remain immutable", "normal path may be reordered"),
]:
    add(name, case_r2=replace_once(r2, old, new))

for index in range(35, 57):
    row_id = f"P1D4D-{index:03d}"
    add(f"inherited-general-frontier-{row_id.lower()}", case_matrix=changed_general(row_id))

for row_id in (
    "P1D4D-011", "P1D4D-012", "P1D4D-027", "P1D4D-028", "P1D4D-030",
    "P1D4D-073", "P1D4D-074", "P1D4D-076", "P1D4D-077", "P1D4D-078",
):
    add(f"inherited-general-contract-{row_id.lower()}", case_matrix=changed_general(row_id))

for index in range(81, 93):
    cell_id = f"P1D4C-{index:03d}"
    add(
        f"inherited-delete-{cell_id.lower()}",
        case_cells=write_cells([row for row in cell_rows if row["cell_id"] != cell_id]),
    )

for index in range(81, 93):
    cell_id = f"P1D4C-{index:03d}"
    changed = [dict(row) for row in cell_rows]
    next(row for row in changed if row["cell_id"] == cell_id)["kill_hook_name"] = "p1d4-generic-weakened-hook"
    add(f"inherited-weaken-{cell_id.lower()}-hook", case_cells=write_cells(changed))

add(
    "inherited-s11-f09-substitution",
    case_cells=replace_once(cells, "new:p1d4_s11_f09_exact_sigkill", "inherited:p1d3_completion"),
)
add(
    "inherited-s11-f10-substitution",
    case_cells=replace_once(cells, "new:p1d4_s11_f10_exact_sigkill", "inherited:p1d3_completion"),
)
add(
    "inherited-request-f02-ready-owner",
    case_cells=replace_once(cells, "p1d4-s01-f02,P1d3DispatchPending", "p1d4-s01-f02,P1SemanticPrepublicationReady"),
)
add("inherited-cell-delete-last", case_cells=write_cells(cell_rows[:-1]))
add("inherited-cell-duplicate-last", case_cells=write_cells(cell_rows + [dict(cell_rows[-1])]))
add("inherited-cell-reorder", case_cells=write_cells([cell_rows[1], cell_rows[0], *cell_rows[2:]]))

for name, path, value in [
    ("inherited-evidence-status", ("status",), "ACCEPTED"),
    ("inherited-evidence-lineage", ("accepted_p1d3_closure_ref",), "0" * 40),
    ("inherited-evidence-r1", ("reviewed_r1_ref",), "0" * 40),
    ("inherited-evidence-general-hash", ("acceptance_matrix_sha256",), "0" * 64),
    ("inherited-evidence-cell-count", ("scenario_frontier_matrix_rows",), 91),
    ("inherited-evidence-cell-hash", ("scenario_frontier_matrix_sha256",), "0" * 64),
    ("inherited-evidence-frontiers", ("frontier_count",), 20),
    ("inherited-evidence-opens-implementation", ("implementation_authorized",), True),
    ("inherited-evidence-dispatch-classifier", ("dispatch_only_recovery_contract", "classifier"), "generic"),
    ("inherited-evidence-second-dispatch", ("dispatch_only_recovery_contract", "second_dispatch_allowed"), True),
    ("inherited-evidence-marker-domain", ("marker_contract", "domain"), "marker"),
    ("inherited-evidence-marker-normalization", ("marker_contract", "normalized_fields"), ["/child_pid", "/redis/port"]),
    ("inherited-evidence-sigterm", ("test_contract", "kernel_observed_exit_signal"), 15),
    ("inherited-evidence-opens-db0", ("closed_surfaces", "operational_redis_db0"), True),
    ("inherited-evidence-negative-count", ("negative_cases",), 127),
]:
    add(name, case_evidence=mutate_evidence(path, value))

add("inherited-status-drift", case_status=replace_once(status, "active R3 design-only correction", "active implementation"))
add("inherited-roadmap-drift", case_roadmap=replace_once(roadmap, "active P1-d4 R3 design-only correction", "active P1-e deployment"))

if len(cases) != 104:
    raise SystemExit(f"inherited R2 mutation inventory drifted: {len(cases)}")


# Twenty-four additional R3 cases close the two findings from review 16fe6dc.
for name, old, new in [
    ("r3-domain-not-action-only", "The dispatch-only classifier must not infer P1-d3 membership", "The classifier may infer membership"),
    ("r3-place-payload", "exact BrokerCommand::PlaceOrder", "any place-like payload"),
    ("r3-cancel-payload", "exact BrokerCommand::CancelOrder", "any cancel-like payload"),
    ("r3-market-regression", "P1-d2 Market RequestAccepted", "P1-d3 LIMIT RequestAccepted"),
    ("r3-market-none", "classify_stage8b_p1d3_dispatch_only_candidate == None", "classifier returns candidate"),
    ("r3-simple-scenarios", "Scenarios S01, S02, S03, S08, S10 and S11 require", "Some simple scenarios require"),
    ("r3-s09-cells", "Both P1D4C-061 (S09/F02) and P1D4C-085 (S09/F20)", "Only one S09 cell"),
    ("r3-scalar-removal", "The scalar `outcome_append_count` from R2 is removed", "The scalar is retained"),
]:
    add(name, case_r3=replace_all(r3, old, new))

for name, path, index, value in [
    ("r3-evidence-place-predicate", ("dispatch_only_recovery_contract", "classifier_domain", "place_required"), 1, "order_type_market"),
    ("r3-evidence-cancel-predicate", ("dispatch_only_recovery_contract", "classifier_domain", "cancel_required"), 2, "any_target_boid"),
    ("r3-evidence-excluded-market", ("dispatch_only_recovery_contract", "classifier_domain", "excluded_place_shapes"), 0, "market_allowed"),
    ("r3-evidence-classifier-order", ("dispatch_only_recovery_contract", "classifier_domain", "order"), 0, "p1d3_dispatch_only"),
]:
    add(name, case_evidence=mutate_evidence_list(path, index, value))

scalar = json.loads(json.dumps(evidence))
scalar["dispatch_only_recovery_contract"]["outcome_append_count"] = 1  # type: ignore[index]
add("r3-evidence-restores-global-scalar", case_evidence=scalar)

for name, path, value in [
    ("r3-evidence-simple-scenarios", ("dispatch_only_recovery_contract", "scenario_outcome_contracts", "simple_request", "scenarios"), ["S01"]),
    ("r3-evidence-simple-total", ("dispatch_only_recovery_contract", "scenario_outcome_contracts", "simple_request", "total_v3_delta"), 2),
    ("r3-evidence-s09-target", ("dispatch_only_recovery_contract", "scenario_outcome_contracts", "s09_target_first", "target_later_filled_v3_delta"), 0),
    ("r3-evidence-s09-cancel", ("dispatch_only_recovery_contract", "scenario_outcome_contracts", "s09_target_first", "recovered_cancel_v3_delta"), 0),
    ("r3-evidence-s09-total", ("dispatch_only_recovery_contract", "scenario_outcome_contracts", "s09_target_first", "total_v3_delta"), 1),
    ("r3-evidence-s09-finalized", ("dispatch_only_recovery_contract", "scenario_outcome_contracts", "s09_target_first", "request_finalized_delta"), 0),
    ("r3-evidence-s09-intermediate", ("dispatch_only_recovery_contract", "scenario_outcome_contracts", "s09_target_first", "required_intermediate_disposition"), "Ready"),
    ("r3-evidence-design-hash", ("design_r3_sha256",), "0" * 64),
    ("r3-evidence-reviewed-r2", ("reviewed_r2_ref",), "0" * 40),
]:
    add(name, case_evidence=mutate_evidence(path, value))

for cell_id in ("P1D4C-061", "P1D4C-085"):
    changed = [dict(row) for row in cell_rows]
    row = next(row for row in changed if row["cell_id"] == cell_id)
    row["sequence_expectation"] = "dispatch_1_to_1_total_v3_delta_1_request_finalized_delta_1"
    add(f"r3-s09-one-outcome-{cell_id.lower()}", case_cells=write_cells(changed))

if len(cases) != 128:
    raise SystemExit(f"R3 mutation inventory drifted: {len(cases)}")
if evidence.get("negative_cases") != len(cases):
    raise SystemExit(f"evidence negative count drifted: {evidence.get('negative_cases')} != {len(cases)}")

escaped: list[str] = []
for (
    name, case_design, case_r1, case_r2, case_r3, case_matrix,
    case_r1_cells, case_r2_cells, case_cells, case_evidence,
    case_status, case_roadmap,
) in cases:
    try:
        checker.validate(
            case_design, case_r1, case_r2, case_r3, case_matrix,
            case_r1_cells, case_r2_cells, case_cells, case_evidence,
            case_status, case_roadmap,
        )
    except checker.CheckFailure:
        print(f"PASS {name}")
    else:
        escaped.append(name)
        print(f"FAIL {name}")

if escaped:
    raise SystemExit("mutations escaped: " + ", ".join(escaped))
print("PASS stage8b-p1d4-r3-design-negative-harness 128/128 inherited_r2=104 targeted_r3=24")
