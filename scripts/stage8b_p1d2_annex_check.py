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
        "Status: R1 design-only review candidate",
        "The P1-d1 Market outcome is deterministic but is not, by itself, a durably\nfinalized command-lifecycle authority.",
        "Stage8bP1d2FinalizedMarketFeedbackInput",
        "durably apply the exact Stage6dPaperOutcome",
        "verify Stage6dPaperExecutionReport against request/client/order/trade",
        "finalize the exact Stage 7 request with observed_at = T_receipt",
        "persist and reread the exact RequestFinalized record",
        "Projection construction, sequence allocation and every Stage 5G ACK/truth\nmutation are forbidden before step 6.",
        "Both first execution\nand replayed finalization use exactly `T_receipt`",
        "The recovery issuer does not call the P1-d1 provider again.",
        "read-only selects the unique first schedule-eligible canonical M10",
        "recomputed deterministic\norder/trade IDs equal the finalized journal facts",
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
        "seq_ack   = next_total_sequence",
        "seq_truth = checked_add(seq_ack, 1)",
        "required: seq_truth == seq_ack + 1",
        "Overflow, reuse, reversal, a skipped value or allocation from a different\nsequence domain fails closed before ACK.",
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
        "q0 == authenticated_stage5_pre_position_qty",
        "missing row is allowed only when the authenticated paper-book generation proves\nthe first/empty state",
        "After any earlier\nposition event, including an explicit flat event, absence is not equivalent to\nflat and fails closed.",
        "a present\nnonzero row requires `a0 = Some(_)`",
        "sign(q0) == sign(d)",
        "q1 == 0                                    -> None",
        "P1D2_AVG_PRICE_SCALE    = 8",
        "P1D2_AVG_PRICE_ROUNDING = RoundingStrategy::MidpointNearestEven",
        "candidate.round_dp_with_strategy(8, MidpointNearestEven)",
        ".rescale_exactly_to(8)",
        "compile-time contract constants and cannot be\nconfigured at runtime",
        "repeating long: q0=1 a0=100 d=2 p=100.5 -> 100.33333333",
        "repeating short: q0=-1 a0=100 d=-2 p=100.5 -> 100.33333333",
        "Tie vectors must prove\n`MidpointNearestEven`",
        "`unrealized_pnl` | `None`",
        "including an explicit zero row when the result is flat",
        "event-scoped broker-neutral truth package",
        "`cash` | `None`",
        "`instruments` | empty vector",
        "`status` | `CommandAckStatus::Accepted`",
        "`reason` | `None`",
        "`ClientOrderId` does not\nreplace `StrategyRequestId`",
        "Duplicate byte-identical feedback is idempotent.",
        "The ACK event uses `seq_ack`. The subsequent truth event uses `seq_truth`.",
        "No Stage5G mutation is allowed before durable Stage7 finalization.",
        "crash after Stage6 outcome, before RequestFinalized",
        "crash after RequestFinalized, before ACK",
        "crash after ACK, before truth",
        "crash after truth, before post-feedback seal",
        "crash after post-feedback seal, before M10 XACK",
        "one redacted, non-authoritative feedback audit\ndigest",
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
    require(annex.count("`Decimal::serialize()`") == 3, "exact Decimal binding count drifted")
    require("P1-d2 v1 emits one event-scoped" in annex, "truth scope drifted")
    require("The truth application uses `seq_truth`" in annex, "sequence owner drifted")
    require("no operational DB 0 activation" in annex, "DB0 boundary drifted")

    rows = list(csv.DictReader(matrix_text.splitlines()))
    expected_ids = {f"P1D2A-{index:03d}" for index in range(1, 65)}
    require(len(rows) == 64, f"acceptance row count drifted: {len(rows)}")
    require({row.get("id") for row in rows} == expected_ids, "acceptance IDs drifted")
    require(all(row.get("status") == "REQUIRED" for row in rows), "acceptance weakened")

    require(evidence.get("schema_version") == 1, "evidence schema drifted")
    require(evidence.get("stage") == "Stage 8B-P1-d2 projection annex R1", "stage drifted")
    require(
        evidence.get("status")
        == "R1_DURABLE_AUTHORITY_SEQUENCE_POSITION_REVIEW_CANDIDATE",
        "status drifted",
    )
    require(evidence.get("accepted_p1d1_closure_ref") == BASE, "lineage drifted")
    require(
        evidence.get("canonical_contract") == "moex.stage8b.p1d2.market-feedback.v1",
        "contract domain drifted",
    )
    require(evidence.get("acceptance_rows") == 64, "evidence row count drifted")
    require(evidence.get("negative_cases") == 41, "negative count drifted")
    r1 = evidence.get("r1_closure")
    require(isinstance(r1, dict) and len(r1) == 10, "R1 closure inventory drifted")
    require(r1.get("average_price_scale") == 8, "average scale drifted")
    require(
        r1.get("average_price_rounding") == "MidpointNearestEven",
        "average rounding drifted",
    )
    require(r1.get("crash_scenarios") == 5, "crash matrix drifted")
    require(r1.get("stage7_finalize_observed_at") == "T_receipt", "finalize clock drifted")
    require(r1.get("stage5g_ack_sequence") == "seq_ack", "ACK sequence drifted")
    require(r1.get("stage5g_truth_sequence") == "seq_ack+1", "truth sequence drifted")
    for name in (
        "durable_finalized_input_required",
        "feedback_audit_digest_required",
        "prior_absence_only_initial_flat",
        "stage5_pre_position_cross_binding",
    ):
        require(r1.get(name) is True, f"R1 closure weakened: {name}")
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
        and "P1-d2 projection-field/timestamp annex R1" in status
        and "design-only review candidate" in status,
        "current status is not synchronized",
    )
    require(
        "P1-d1 is formally closed" in roadmap
        and "P1-d2 projection-field/timestamp annex R1" in roadmap
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
    print("PASS stage8b-p1d2-annex-scope rows=64 r1=true design_only=true source=false db0=false")


if __name__ == "__main__":
    main()
