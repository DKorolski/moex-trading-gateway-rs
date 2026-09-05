#!/usr/bin/env python3
"""Targeted mutation harness for the P1-d4 design contract."""

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
matrix = checker.MATRIX.read_text(encoding="utf-8")
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


cases: list[tuple[str, str, str, dict[str, object], str, str]] = []
for name, old, new in [
    ("open-implementation", "Status: design-only review candidate", "Status: source implementation"),
    ("change-lineage", checker.BASE, "0" * 40),
    ("allow-p1d3-semantic-change", "P1-d3 lifecycle semantics remain immutable", "P1-d3 semantics may change"),
    ("use-operational-redis", "newly\nstarted loopback-only Redis instance on a random port", "operational Redis DB0"),
    ("panic-instead-of-kill", "An in-process panic is not sufficient evidence", "An in-process panic is sufficient evidence"),
    ("remove-fsync-marker", "write_all` plus `sync_all", "write only"),
    ("collapse-ack-frontiers", "No implementation may collapse `F03` into `F04`", "F03 and F04 may be one frontier"),
    ("restart-provider", "restart does not call the provider, mint schedule authority", "restart may call the provider"),
    ("weaken-cross-binding", "Stage 6 checkpoint and replacement package are mutually bound", "Stage 6 checkpoint is diagnostic"),
    ("xack-before-seal", "XACK is the last external mutation", "XACK may precede the seal"),
    ("pel-only-success", "Missing PEL membership\nwithout the frontier proof is `ExactSourceConflict`", "Missing PEL membership is success"),
    ("replay-callback", "never create a second target truth", "may create a second target truth"),
    ("drop-target-first", "Cancel tests must preserve target-first ordering", "Cancel may win before target evaluation"),
    ("invent-expiry-xack", "must prove seal recovery without\ninventing an XACK", "may invent an XACK"),
    ("accept-table-only", "without a spawned\nchild, reached marker, process kill, clean restart and final audit is not\ncredited", "with a table row is credited"),
    ("allow-new-candidate", "select a new candidate bar", "select a replacement candidate bar"),
    ("weaken-identical-replay", "byte-identical replay is idempotent", "byte-identical replay may allocate a sequence"),
    ("consumer-changes-semantics", "changed consumer name alone must not change semantics", "consumer name changes semantics"),
    ("one-repro-run", "two clean runs over the same source must produce the same\nsemantic digest", "one run is sufficient"),
    ("allow-skipped-cells", "Missing,\nduplicate, skipped, ignored or `not_applicable` required cells fail closed", "Skipped cells are allowed"),
    ("open-vps", "operational Redis DB 0 or VPS activation", "operational Redis DB 0 is allowed"),
    ("open-finam", "FINAM POST/DELETE, broker dispatch", "FINAM POST is allowed"),
    ("open-partials", "partial fills, fees/slippage", "partial fills are enabled"),
    ("skip-independent-acceptance", "A separate independent source acceptance and governance-only authority\nrebind are required", "Implementation may directly open P1-e"),
    ("design-authorizes-source", "Acceptance of this design authorizes only the P1-d4 source/test implementation", "This design authorizes operational deployment"),
]:
    cases.append((name, replace_once(design, old, new), matrix, evidence, status, roadmap))

for name, path, value in [
    ("evidence-opens-implementation", ("implementation_authorized",), True),
    ("evidence-wrong-frontiers", ("frontier_count",), 11),
    ("evidence-fake-kill", ("test_contract", "actual_child_process_kill"), False),
    ("evidence-nonisolated-redis", ("test_contract", "ephemeral_loopback_redis"), False),
    ("evidence-no-fsync", ("test_contract", "fsync_backed_pre_kill_marker"), False),
    ("evidence-mutable-p1d3", ("test_contract", "p1d3_semantics_mutable"), True),
    ("evidence-no-response-loss", ("test_contract", "response_loss_after_real_xack"), False),
    ("evidence-opens-db0", ("closed_surfaces", "operational_redis_db0"), True),
]:
    cases.append((name, design, matrix, mutate_evidence(path, value), status, roadmap))

rows = list(csv.DictReader(matrix.splitlines()))
buffer = io.StringIO()
writer = csv.DictWriter(buffer, fieldnames=["id", "area", "requirement", "status"])
writer.writeheader()
writer.writerows(rows[:-1])
cases.extend(
    [
        ("remove-matrix-row", design, buffer.getvalue(), evidence, status, roadmap),
        ("status-drift", design, matrix, evidence, replace_once(status, "P1-d4 is the active design-only candidate", "P1-d4 is operational"), roadmap),
        ("roadmap-drift", design, matrix, evidence, status, replace_once(roadmap, "active P1-d4 design-only slice", "active P1-e deployment")),
    ]
)

if len(cases) != 36:
    raise SystemExit(f"mutation inventory drifted: {len(cases)}")

escaped: list[str] = []
for name, case_design, case_matrix, case_evidence, case_status, case_roadmap in cases:
    try:
        checker.validate(case_design, case_matrix, case_evidence, case_status, case_roadmap)
    except checker.CheckFailure:
        print(f"PASS {name}")
    else:
        escaped.append(name)
        print(f"FAIL {name}")

if escaped:
    raise SystemExit("mutations escaped: " + ", ".join(escaped))
print("PASS stage8b-p1d4-design-negative-harness 36/36")
