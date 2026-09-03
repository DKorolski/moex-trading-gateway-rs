#!/usr/bin/env python3
"""Fail-closed design checker for the Stage 8B-P1-d2 projection annex."""

from __future__ import annotations

import csv
import json
import pathlib
import subprocess
import sys


ROOT = pathlib.Path(__file__).resolve().parents[1]
BASE = "4abb2fd9807adeb47f164a4025c7ac44d33679f6"
ANNEX = ROOT / "docs/stage-8/stage8b-p1d2-projection-field-timestamp-annex.md"
MATRIX = ROOT / "docs/stage-8/stage8b-p1d2-projection-annex-acceptance-matrix.csv"
EVIDENCE = ROOT / "docs/stage-8/stage8b-p1d2-projection-annex-evidence.json"
STATUS = ROOT / "docs/current-status.md"
ROADMAP = ROOT / "docs/roadmap.md"
EXPECTED_CHANGED = {
    "docs/current-status.md",
    "docs/roadmap.md",
    "docs/stage-8/stage8b-p1d2-projection-annex-acceptance-matrix.csv",
    "docs/stage-8/stage8b-p1d2-projection-annex-evidence.json",
    "docs/stage-8/stage8b-p1d2-projection-field-timestamp-annex.md",
    "scripts/make_stage8b_p1d2_annex_handoff.py",
    "scripts/stage8b_p1d2_annex_check.py",
    "scripts/stage8b_p1d2_annex_gate.sh",
    "scripts/stage8b_p1d2_annex_handoff_safety_check.py",
    "scripts/stage8b_p1d2_annex_negative_harness.py",
}


class CheckFailure(RuntimeError):
    pass


def require(condition: bool, message: str) -> None:
    if not condition:
        raise CheckFailure(message)


def changed_files() -> set[str]:
    tracked = subprocess.run(
        ["git", "diff", "--name-only", BASE],
        cwd=ROOT,
        check=True,
        text=True,
        capture_output=True,
    ).stdout.splitlines()
    untracked = subprocess.run(
        ["git", "ls-files", "--others", "--exclude-standard"],
        cwd=ROOT,
        check=True,
        text=True,
        capture_output=True,
    ).stdout.splitlines()
    return set(tracked) | set(untracked)


def validate(
    annex: str,
    matrix_text: str,
    evidence: dict[str, object],
    status: str,
    roadmap: str,
) -> None:
    required_annex = (
        "moex.stage8b.p1d2.market-feedback.v1",
        "4abb2fd9807adeb47f164a4025c7ac44d33679f6",
        "Status: design-only review candidate.",
        "one linear, already durable P1-d1 Market\noutcome",
        "`Utc::now()` and equivalent system-clock reads are\nforbidden",
        "T_source  = execution_bar.open_ts",
        "T_receipt = execution_bar.close_ts",
        "T_receipt - T_source = 600 seconds",
        "`BrokerOrderSnapshot` | `Some(T_source)` | `T_receipt`",
        "`BrokerTradeSnapshot` | `T_source` | `T_receipt`",
        "`BrokerPositionSnapshot` | `Some(T_source)` | `T_receipt`",
        "`BrokerTruthSnapshot` | n/a | `T_receipt`",
        "`CommandAck` | n/a | `T_receipt`",
        "Equal ACK and truth receipt timestamps are intentional.",
        "`Decimal::serialize()`",
        "`Decimal::ZERO` (positive zero, scale zero)",
        "`broker_order_id` | `Some(P1-d1 BrokerOrderId)`",
        "`time_in_force` | `Some(TimeInForce::Day)`",
        "`status` | `OrderStatus::Filled`",
        "`lifecycle` | `BrokerOrderLifecycle::Terminal`",
        "`remaining_qty` | `Some(Decimal::ZERO)`",
        "`limit_price` | `None`",
        "`commission` | `Some(Decimal::ZERO)`",
        "`gross_amount = None` is intentionally not inferred",
        "Buy is `+fill_qty`, Sell is `-fill_qty`",
        "sign(q0) == sign(d)",
        "q1 == 0                                  -> None",
        "`unrealized_pnl` | `None`",
        "including an explicit zero row when the result is flat",
        "event-scoped broker-neutral truth package",
        "`cash` | `None`",
        "`instruments` | empty vector",
        "`status` | `CommandAckStatus::Accepted`",
        "`reason` | `None`",
        "`ClientOrderId` does not\nreplace `StrategyRequestId`",
        "Duplicate byte-identical feedback is idempotent.",
        "XACK the originating M10 last.",
        "current authority remains pinned to accepted P1-d1",
        "P1-d2 production source implementation;",
        "FINAM HTTP POST/DELETE and broker network dispatch;",
    )
    for token in required_annex:
        require(token in annex, f"missing annex invariant: {token}")
    require(
        annex.count("`broker_order_id` | `Some(P1-d1 BrokerOrderId)`") == 3,
        "order/trade/ACK broker-order binding count drifted",
    )
    require("P1-d2 v1 emits one event-scoped" in annex, "truth scope drifted")
    require("one fresh monotonic\n`total_sequence`" in annex, "sequence owner drifted")
    require("no operational DB 0 activation" in annex, "DB0 boundary drifted")

    rows = list(csv.DictReader(matrix_text.splitlines()))
    expected_ids = {f"P1D2A-{index:03d}" for index in range(1, 61)}
    require(len(rows) == 60, f"acceptance row count drifted: {len(rows)}")
    require({row.get("id") for row in rows} == expected_ids, "acceptance IDs drifted")
    require(all(row.get("status") == "REQUIRED" for row in rows), "acceptance weakened")

    require(evidence.get("schema_version") == 1, "evidence schema drifted")
    require(evidence.get("stage") == "Stage 8B-P1-d2 projection annex", "stage drifted")
    require(evidence.get("status") == "DESIGN_ONLY_REVIEW_CANDIDATE", "status drifted")
    require(evidence.get("accepted_p1d1_closure_ref") == BASE, "lineage drifted")
    require(
        evidence.get("canonical_contract") == "moex.stage8b.p1d2.market-feedback.v1",
        "contract domain drifted",
    )
    require(evidence.get("acceptance_rows") == 60, "evidence row count drifted")
    require(evidence.get("negative_cases") == 30, "negative count drifted")
    require(evidence.get("design_only") is True, "design-only marker opened")
    require(evidence.get("implementation_authorized") is False, "source opened early")
    require(
        evidence.get("current_tree_authority_remains_p1d1") is True,
        "current-tree authority rebound by design candidate",
    )
    closed = evidence.get("closed_surfaces")
    require(isinstance(closed, dict) and len(closed) == 10, "closed inventory drifted")
    require(all(value is False for value in closed.values()), "closed surface opened")

    require(
        "Stage 8B-P1-d1 is formally closed" in status
        and "P1-d2 projection-field/timestamp annex" in status
        and "design-only review candidate" in status,
        "current status is not synchronized",
    )
    require(
        "P1-d1 is formally closed" in roadmap
        and "P1-d2 projection-field/timestamp annex" in roadmap
        and "P1-d2 source remains closed" in roadmap,
        "roadmap is not synchronized",
    )


def main() -> None:
    try:
        actual = changed_files()
        require(actual == EXPECTED_CHANGED, f"design-only changed path drift: {sorted(actual)}")
        forbidden_diff = subprocess.run(
            [
                "git",
                "diff",
                "--name-only",
                BASE,
                "--",
                "Cargo.toml",
                "Cargo.lock",
                "crates",
                ".github",
                "config",
            ],
            cwd=ROOT,
            check=True,
            text=True,
            capture_output=True,
        ).stdout.strip()
        require(not forbidden_diff, f"production/workflow/config changed: {forbidden_diff}")
        validate(
            ANNEX.read_text(encoding="utf-8"),
            MATRIX.read_text(encoding="utf-8"),
            json.loads(EVIDENCE.read_text(encoding="utf-8")),
            STATUS.read_text(encoding="utf-8"),
            ROADMAP.read_text(encoding="utf-8"),
        )
    except (CheckFailure, OSError, json.JSONDecodeError, subprocess.CalledProcessError) as error:
        print(f"FAIL stage8b-p1d2-annex-scope: {error}", file=sys.stderr)
        raise SystemExit(1)
    print("PASS stage8b-p1d2-annex-scope rows=60 design_only=true source=false db0=false")


if __name__ == "__main__":
    main()
