#!/usr/bin/env python3
"""Targeted 104-case mutation harness for the P1-d4 R2 design contract."""

from __future__ import annotations

import csv
import io
import json

import stage8b_p1d4_r2_design_check as checker


def replace_once(value: str, old: str, new: str) -> str:
    if old not in value:
        raise RuntimeError(f"mutation anchor missing: {old!r}")
    return value.replace(old, new, 1)


design = checker.DESIGN.read_text(encoding="utf-8")
r1 = checker.R1.read_text(encoding="utf-8")
r2 = checker.R2.read_text(encoding="utf-8")
matrix = checker.MATRIX.read_text(encoding="utf-8")
r1_cells = checker.R1_CELL_MATRIX.read_text(encoding="utf-8")
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


Case = tuple[str, str, str, str, str, str, dict[str, object], str, str]
cases: list[Case] = []


def add(
    name: str,
    *,
    case_design: str = design,
    case_r1: str = r1,
    case_r2: str = r2,
    case_matrix: str = matrix,
    case_r1_cells: str = r1_cells,
    case_cells: str = cells,
    case_evidence: dict[str, object] = evidence,
    case_status: str = status,
    case_roadmap: str = roadmap,
) -> None:
    cases.append((name, case_design, case_r1, case_r2, case_matrix, case_r1_cells, case_cells, case_evidence, case_status, case_roadmap))  # type: ignore[arg-type]


for name, old, new in [
    ("r2-status", "Status: R2 design-only review candidate", "Status: accepted"),
    ("r2-cell-count", "cells: 92", "cells: implementation-selected"),
    ("r2-frontier-range", "frontiers: F00..F20", "frontiers: F00..F19"),
    ("r2-derived-count", "number 92 is derived from the complete reviewed rows", "number 92 is a target"),
    ("r2-general-alignment", "General requirements P1D4D-035..P1D4D-056 now use the same", "General requirements may use legacy meanings"),
    ("r2-dispatch-frontier-distinction", "F20 and request-scoped F02 have the same durable journal suffix", "F20 and F02 are one synthetic hook"),
    ("r2-dispatch-candidate", "Stage6Stage8bP1d3DispatchOnlyCandidate", "GenericCandidate"),
    ("r2-dispatch-classifier", "classify_stage8b_p1d3_dispatch_only_candidate", "classify_any_suffix"),
    ("r2-dispatch-owner", "Stage8bP1d3DispatchPendingOwner", "ReadyOwner"),
    ("r2-dispatch-outcome", "Stage7bRestartOutcome::P1d3DispatchPending", "Stage7bRestartOutcome::Ready"),
    ("r2-dispatch-resume", "resume_stage8b_p1d3_dispatch_pending", "resume_generic"),
    ("r2-second-dispatch", "must not call either dispatch-append helper", "may append dispatch again"),
    ("r2-dispatch-count", "dispatch rows                 1 -> 1", "dispatch rows                 1 -> 2"),
    ("r2-s06-f07", "P1D4C-088 | S06/F07", "P1D4C-088 | omitted"),
    ("r2-s09-f04", "P1D4C-092 | S09/F04", "P1D4C-092 | omitted"),
    ("r2-s11-new-hooks", "P1D4C-078 and P1D4C-079 require new hooks", "P1D4C-078 and P1D4C-079 use inherited completion"),
    ("r2-marker-schema", "schema_version:             integer 1", "schema_version:             any integer"),
    ("r2-marker-domain", 'domain:                     "moex.stage8b.p1d4.crash-marker.v1"', 'domain:                     "marker"'),
    ("r2-marker-normalization", "only `child_pid` is replaced by integer", "all unstable fields are removed"),
    ("r2-marker-digest-domain", "moex.stage8b.p1d4.crash-marker.normalized.v1\\0", "marker\\0"),
    ("r2-old-marker-substitution", "Substitution of the old marker", "Use of the old marker"),
    ("r2-general-row-negatives", "each corrected general row P1D4D-035..P1D4D-056", "some corrected rows"),
    ("r2-open-db0", "operational Redis DB0/VPS", "deployment surface may open"),
    ("r2-open-p1e", "P1-e remains unauthorized", "P1-e is authorized"),
    ("r2-normal-path-mutation", "normal-path ordering remain immutable", "normal path may be reordered"),
]:
    changed = r2.replace(old, new) if name in {"r2-dispatch-owner", "r2-open-db0"} else replace_once(r2, old, new)
    add(name, case_r2=changed)


general_rows = list(csv.DictReader(matrix.splitlines()))


def changed_general(row_id: str) -> str:
    rows = [dict(row) for row in general_rows]
    for row in rows:
        if row["id"] == row_id:
            row["requirement"] = "WEAKENED OR LEGACY FRONTIER MEANING"
            break
    else:
        raise RuntimeError(f"general row missing: {row_id}")
    buffer = io.StringIO(newline="")
    writer = csv.DictWriter(buffer, fieldnames=["id", "area", "requirement", "status"], lineterminator="\n")
    writer.writeheader()
    writer.writerows(rows)
    return buffer.getvalue()


for index in range(35, 57):
    row_id = f"P1D4D-{index:03d}"
    add(f"general-frontier-mapping-{row_id.lower()}", case_matrix=changed_general(row_id))

for row_id in ("P1D4D-011", "P1D4D-012", "P1D4D-027", "P1D4D-028", "P1D4D-030", "P1D4D-073", "P1D4D-074", "P1D4D-076", "P1D4D-077", "P1D4D-078"):
    add(f"general-r2-contract-{row_id.lower()}", case_matrix=changed_general(row_id))


cell_lines = cells.splitlines()
header = cell_lines[0]
data = cell_lines[1:]
for index in range(81, 93):
    cell_id = f"P1D4C-{index:03d}"
    remaining = [line for line in data if not line.startswith(cell_id + ",")]
    add(f"delete-{cell_id.lower()}", case_cells="\n".join([header] + remaining) + "\n")

for index in range(81, 93):
    cell_id = f"P1D4C-{index:03d}"
    original = next(line for line in data if line.startswith(cell_id + ","))
    fields = original.split(",")
    fields[6] = "p1d4-generic-weakened-hook"
    add(f"weaken-{cell_id.lower()}-hook", case_cells=replace_once(cells, original, ",".join(fields)))

add(
    "s11-f09-inherited-substitution",
    case_cells=replace_once(cells, "new:p1d4_s11_f09_exact_sigkill", "inherited:p1d3_subprocess_sigkill_brackets_s_cancel_recovered@7dc7c80"),
)
add(
    "s11-f10-inherited-substitution",
    case_cells=replace_once(cells, "new:p1d4_s11_f10_exact_sigkill", "inherited:p1d3_subprocess_sigkill_brackets_s_cancel_recovered@7dc7c80"),
)
add(
    "request-f02-ready-owner",
    case_cells=replace_once(cells, "p1d4-s01-f02,P1d3DispatchPending", "p1d4-s01-f02,P1SemanticPrepublicationReady"),
)
add("cell-delete-last", case_cells="\n".join([header] + data[:-1]) + "\n")
add("cell-duplicate-last", case_cells="\n".join([header] + data + [data[-1]]) + "\n")
add("cell-reorder", case_cells="\n".join([header, data[1], data[0]] + data[2:]) + "\n")

for name, path, value in [
    ("evidence-status", ("status",), "ACCEPTED"),
    ("evidence-lineage", ("accepted_p1d3_closure_ref",), "0" * 40),
    ("evidence-r1", ("reviewed_r1_ref",), "0" * 40),
    ("evidence-general-hash", ("acceptance_matrix_sha256",), "0" * 64),
    ("evidence-cell-count", ("scenario_frontier_matrix_rows",), 91),
    ("evidence-cell-hash", ("scenario_frontier_matrix_sha256",), "0" * 64),
    ("evidence-frontiers", ("frontier_count",), 20),
    ("evidence-opens-implementation", ("implementation_authorized",), True),
    ("evidence-dispatch-classifier", ("dispatch_only_recovery_contract", "classifier"), "generic"),
    ("evidence-second-dispatch", ("dispatch_only_recovery_contract", "second_dispatch_allowed"), True),
    ("evidence-marker-domain", ("marker_contract", "domain"), "marker"),
    ("evidence-marker-normalization", ("marker_contract", "normalized_fields"), ["/child_pid", "/pre_kill_audit_sha256"]),
    ("evidence-sigterm", ("test_contract", "kernel_observed_exit_signal"), 15),
    ("evidence-opens-db0", ("closed_surfaces", "operational_redis_db0"), True),
    ("evidence-negative-count", ("negative_cases",), 103),
]:
    add(name, case_evidence=mutate_evidence(path, value))

add("status-drift", case_status=replace_once(status, "active R2 design-only correction", "active implementation"))
add("roadmap-drift", case_roadmap=replace_once(roadmap, "active P1-d4 R2 design-only correction", "active P1-e deployment"))

if len(cases) != 104:
    raise SystemExit(f"mutation inventory drifted: {len(cases)}")
if evidence.get("negative_cases") != len(cases):
    raise SystemExit(f"evidence negative count drifted: {evidence.get('negative_cases')} != {len(cases)}")

escaped: list[str] = []
for name, case_design, case_r1, case_r2, case_matrix, case_r1_cells, case_cells, case_evidence, case_status, case_roadmap in cases:
    try:
        checker.validate(
            case_design, case_r1, case_r2, case_matrix, case_r1_cells,
            case_cells, case_evidence, case_status, case_roadmap,
        )
    except checker.CheckFailure:
        print(f"PASS {name}")
    else:
        escaped.append(name)
        print(f"FAIL {name}")

if escaped:
    raise SystemExit("mutations escaped: " + ", ".join(escaped))
print("PASS stage8b-p1d4-r2-design-negative-harness 104/104")
