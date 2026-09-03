#!/usr/bin/env python3
"""Mutation harness for the Stage 8B-P1-d2 design annex."""

from __future__ import annotations

import csv
import io
import json

import stage8b_p1d2_annex_check as checker


def replace_once(value: str, old: str, new: str) -> str:
    if value.count(old) < 1:
        raise RuntimeError(f"mutation anchor missing for {old!r}")
    return value.replace(old, new, 1)


annex = checker.ANNEX.read_text(encoding="utf-8")
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
removed = rows[:-1]
matrix_buffer = io.StringIO()
writer = csv.DictWriter(matrix_buffer, fieldnames=["id", "area", "requirement", "status"])
writer.writeheader()
writer.writerows(removed)

mutations = [
    ("contract-domain", "moex.stage8b.p1d2.market-feedback.v1", "moex.stage8b.p1d2.market-feedback.v2"),
    ("accepted-lineage", checker.BASE, "0" * 40),
    ("design-only", "Status: design-only review candidate.", "Status: implementation candidate."),
    ("linear-outcome", "one linear, already durable P1-d1 Market\noutcome", "one copyable P1-d1 Market\noutcome"),
    ("allow-wall-clock", "`Utc::now()` and equivalent system-clock reads are\nforbidden", "`Utc::now()` and equivalent system-clock reads are\nallowed"),
    ("source-clock", "T_source  = execution_bar.open_ts", "T_source  = execution_bar.close_ts"),
    ("receipt-clock", "T_receipt = execution_bar.close_ts", "T_receipt = observed_at"),
    ("m10-duration", "T_receipt - T_source = 600 seconds", "T_receipt - T_source = variable"),
    ("order-source-clock", "`BrokerOrderSnapshot` | `Some(T_source)` | `T_receipt`", "`BrokerOrderSnapshot` | `None` | `T_receipt`"),
    ("decimal-bytes", "`Decimal::serialize()`", "Decimal string"),
    ("zero-shape", "`Decimal::ZERO` (positive zero, scale zero)", "any numeric zero"),
    ("order-id-optional", "`broker_order_id` | `Some(P1-d1 BrokerOrderId)`", "`broker_order_id` | `None`"),
    ("order-tif", "`time_in_force` | `Some(TimeInForce::Day)`", "`time_in_force` | `None`"),
    ("order-status", "`status` | `OrderStatus::Filled`", "`status` | `OrderStatus::Working`"),
    ("order-remaining", "`remaining_qty` | `Some(Decimal::ZERO)`", "`remaining_qty` | `None`"),
    ("order-limit", "`limit_price` | `None`", "`limit_price` | `Some(price)`"),
    ("commission-omitted", "`commission` | `Some(Decimal::ZERO)`", "`commission` | `None`"),
    ("gross-inferred", "`gross_amount = None` is intentionally not inferred", "`gross_amount = None` may be inferred"),
    ("signed-delta", "Buy is `+fill_qty`, Sell is `-fill_qty`", "Buy and Sell are `+fill_qty`"),
    ("weighted-average", "sign(q0) == sign(d)", "sign(q0) != sign(d)"),
    ("flat-average", "q1 == 0                                  -> None", "q1 == 0                                  -> Some(0)"),
    ("position-pnl", "`unrealized_pnl` | `None`", "`unrealized_pnl` | `Some(Decimal::ZERO)`"),
    ("drop-flat-row", "including an explicit zero row when the result is flat", "omitting the row when the result is flat"),
    ("full-account-truth", "event-scoped broker-neutral truth package", "fabricated full-account broker truth"),
    ("truth-cash", "`cash` | `None`", "`cash` | synthetic zero cash"),
    ("ack-status", "`status` | `CommandAckStatus::Accepted`", "`status` | `CommandAckStatus::Submitted`"),
    ("request-alias", "`ClientOrderId` does not\nreplace `StrategyRequestId`", "`ClientOrderId` may\nreplace `StrategyRequestId`"),
    ("xack-before-seal", "XACK the originating M10 last.", "XACK the originating M10 first."),
]

cases: list[tuple[str, str, str, dict[str, object], str, str]] = []
for name, old, new in mutations:
    cases.append((name, replace_once(annex, old, new), matrix, evidence, status, roadmap))
cases.extend(
    [
        ("remove-acceptance-row", annex, matrix_buffer.getvalue(), evidence, status, roadmap),
        (
            "open-source-implementation",
            annex,
            matrix,
            mutated_evidence(("implementation_authorized",), True),
            status,
            roadmap,
        ),
    ]
)

failures: list[str] = []
for name, case_annex, case_matrix, case_evidence, case_status, case_roadmap in cases:
    try:
        checker.validate(case_annex, case_matrix, case_evidence, case_status, case_roadmap)
    except checker.CheckFailure:
        print(f"PASS {name}")
    else:
        failures.append(name)
        print(f"FAIL {name}")

if failures:
    print("mutations escaped: " + ", ".join(failures))
    raise SystemExit(1)
print(f"PASS stage8b-p1d2-annex-negative-harness {len(cases)}/{len(cases)}")
