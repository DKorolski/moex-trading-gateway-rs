#!/usr/bin/env python3
"""Targeted mutation harness for the Stage 8B-P1-d3 design contract."""

from __future__ import annotations

import csv
import io
import json

import stage8b_p1d3_design_check as checker


def replace_once(value: str, old: str, new: str) -> str:
    if old not in value:
        raise RuntimeError(f"mutation anchor missing for {old!r}")
    return value.replace(old, new, 1)


design = checker.DESIGN.read_text(encoding="utf-8")
matrix = checker.MATRIX.read_text(encoding="utf-8")
evidence = json.loads(checker.EVIDENCE.read_text(encoding="utf-8"))
status = checker.STATUS.read_text(encoding="utf-8")
roadmap = checker.ROADMAP.read_text(encoding="utf-8")


def mutated_evidence(path: tuple[str, ...], value: object) -> dict[str, object]:
    result = json.loads(json.dumps(evidence))
    cursor: dict[str, object] = result
    for name in path[:-1]:
        cursor = cursor[name]  # type: ignore[assignment]
    cursor[path[-1]] = value
    return result


rows = list(csv.DictReader(matrix.splitlines()))
matrix_buffer = io.StringIO()
writer = csv.DictWriter(matrix_buffer, fieldnames=["id", "area", "requirement", "status"])
writer.writeheader()
writer.writerows(rows[:-1])

mutations = [
    ("book-domain", "moex.stage8b.p1d3.working-book.v1", "moex.stage8b.p1d3.working-book.v2"),
    ("transition-domain", "moex.stage8b.p1d3.book-transition.v1", "moex.stage8b.p1d3.book-transition.v2"),
    ("accepted-lineage", checker.BASE, "0" * 40),
    ("design-only", "Status: design-only review candidate", "Status: source implementation candidate"),
    ("merge-request-order-lifecycle", "1. **request lifecycle**", "1. **combined lifecycle**"),
    ("working-xack-before-seal", "replacement `S_working` is persisted and reread", "in-memory Working callback returns"),
    ("book-sidecar", "inside the existing authenticated replacement package, never as a sidecar", "inside a Redis-derived sidecar"),
    ("registry-bound", "P1D3_MAX_ORDER_RECORDS_PER_GENERATION = 1024", "P1D3_MAX_ORDER_RECORDS_PER_GENERATION = runtime_config.max"),
    ("silent-capacity-eviction", "Capacity exhaustion fails closed before a new dispatch.", "Capacity exhaustion evicts the oldest row."),
    ("forge-step-authority", "Stage8bP1d3ScheduleStepAuthority", "RedisScheduleStepAuthority"),
    ("forge-expiry-authority", "Stage8bP1d3DayExpiryAuthority", "UtcDayExpiryAuthority"),
    ("wall-clock-expiry", "UTC date comparison, wall clock, process time and the\nfirst bar of a new day are not expiry authorities.", "UTC date and wall clock may authorize expiry."),
    ("callback-before-active-resolution", "resolve the exact already-active order against B[k]", "invoke the Hybrid callback before resolving the active order"),
    ("book-evaluator-xack", "The book evaluator has no M10 XACK method.", "The book evaluator may XACK the M10."),
    ("double-evaluate-first-candidate", "records `B[n+1]` as already evaluated", "does not record `B[n+1]` as evaluated"),
    ("buy-fill-price", "bar.low <= limit -> Filled at min(bar.open, limit)", "bar.low <= limit -> Filled at bar.close"),
    ("sell-fill-price", "bar.high >= limit -> Filled at max(bar.open, limit)", "bar.high >= limit -> Filled at bar.close"),
    ("intrabar-fill-clock", "exact final execution-bar `close_ts`", "inferred intrabar touch timestamp"),
    ("expiry-as-pending", "Stage6dPaperOutcome::LimitExpired { broker_order_id }", "Stage6dPaperOutcome::LimitPending"),
    ("target-client-as-request-client", "ACK: `Accepted`, reason `None`, exact request-level durable client ID", "ACK: `Accepted`, reason `None`, target order client ID"),
    ("nonconsecutive-initial-truth", "truth uses `seq_truth = seq_ack + 1`", "truth reuses `seq_ack`"),
    ("untouched-allocates-sequence", "untouched: order remains Working; no Stage 5G event or total sequence is", "untouched: order remains Working and allocates a Stage 5G sequence"),
    ("allow-partial-fill", "Partial fill, a second trade or a\nsecond terminal transition fails closed.", "Partial fill and multiple trades are allowed."),
    ("cancel-before-target-evaluation", "This\nfreezes fill-before-cancel", "This applies cancel-before-fill"),
    ("cancel-working-rejected", "`CancelCanceled` | `Accepted` / no reason | target `Canceled`", "`CancelRejected` | `Rejected` | none"),
    ("duplicate-filled-truth", "`CancelExecutionObserved` | `Recovered` / `RecoveredByBrokerTruth`", "`CancelExecutionObserved` | `Accepted` / no reason"),
    ("terminal-nonexecution-rejected", "`CancelAlreadyTerminalNonExecution` | `Recovered` / `RecoveredByBrokerTruth`", "`CancelAlreadyTerminalNonExecution` | `Rejected` / conflict"),
    ("finalize-inconclusive", "`Inconclusive` hold | none | none", "`Inconclusive` | `Accepted` | synthetic truth"),
    ("alias-target-client-id", "A target\norder client ID never replaces it.", "A target\norder client ID replaces it."),
    ("later-fill-sequence-pair", "later autonomous fill or expiry: one next truth sequence", "later autonomous fill or expiry: an ACK/truth pair"),
    ("restart-provider-replay", "Restart authenticates the exact Stage 5G/6/7 package and P1-d3 book together.", "Restart calls the provider and rebuilds the book."),
]

cases: list[tuple[str, str, str, dict[str, object], str, str]] = []
for name, old, new in mutations:
    cases.append((name, replace_once(design, old, new), matrix, evidence, status, roadmap))

cases.extend(
    [
        ("remove-acceptance-row", design, matrix_buffer.getvalue(), evidence, status, roadmap),
        (
            "open-source-implementation",
            design,
            matrix,
            mutated_evidence(("implementation_authorized",), True),
            status,
            roadmap,
        ),
        (
            "open-operational-db0",
            design,
            matrix,
            mutated_evidence(("closed_surfaces", "operational_redis_db0"), True),
            status,
            roadmap,
        ),
        (
            "status-drift",
            design,
            matrix,
            evidence,
            replace_once(status, "active candidate is now the design-only", "active candidate is now the source"),
            roadmap,
        ),
        (
            "roadmap-opens-p1d4",
            design,
            matrix,
            evidence,
            status,
            replace_once(roadmap, "P1-d4 exhaustive crash/replay closure", "P1-d4 active crash/replay implementation"),
        ),
    ]
)

if len(cases) != 36:
    raise SystemExit(f"mutation inventory drifted: {len(cases)}")

failures: list[str] = []
for name, case_design, case_matrix, case_evidence, case_status, case_roadmap in cases:
    try:
        checker.validate(case_design, case_matrix, case_evidence, case_status, case_roadmap)
    except checker.CheckFailure:
        print(f"PASS {name}")
    else:
        failures.append(name)
        print(f"FAIL {name}")

if failures:
    print("mutations escaped: " + ", ".join(failures))
    raise SystemExit(1)
print("PASS stage8b-p1d3-design-negative-harness 36/36")
