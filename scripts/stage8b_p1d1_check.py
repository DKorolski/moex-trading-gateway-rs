#!/usr/bin/env python3
"""Fail-closed source/scope checker for Stage 8B-P1-d1."""

from __future__ import annotations

import csv
import json
import pathlib
import subprocess
import sys


ROOT = pathlib.Path(__file__).resolve().parents[1]
BASE = "0d59d54d42fc29ae7b31359c1ded8efbd3a348fd"
EXPECTED_CHANGED = {
    "crates/runtime-durable-service/src/stage8b_p1_semantic.rs",
    "crates/runtime-durable-service/src/stage8b_p1_semantic/redis.rs",
    "crates/strategy-runtime-core/src/lib.rs",
    "crates/strategy-runtime-core/src/stage5e_no_io_lifecycle.rs",
    "crates/strategy-runtime-core/src/stage6d_live_core.rs",
    "crates/strategy-runtime-core/src/stage8b_p1d1_paper_provider.rs",
    "docs/current-status.md",
    "docs/roadmap.md",
    "docs/stage-8/stage8b-p1d1-acceptance-matrix.csv",
    "docs/stage-8/stage8b-p1d1-evidence.json",
    "docs/stage-8/stage8b-p1d1-market-provider-core.md",
    "scripts/make_stage8b_p1d1_handoff.py",
    "scripts/stage8b_p1d1_check.py",
    "scripts/stage8b_p1d1_gate.sh",
    "scripts/stage8b_p1d1_handoff_safety_check.py",
    "scripts/stage8b_p1d1_negative_harness.py",
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


def load_content(root: pathlib.Path = ROOT) -> dict[str, str]:
    paths = {
        "provider": "crates/strategy-runtime-core/src/stage8b_p1d1_paper_provider.rs",
        "schedule": "crates/strategy-runtime-core/src/stage5e_no_io_lifecycle.rs",
        "dispatch": "crates/strategy-runtime-core/src/stage6d_live_core.rs",
        "canonical": "crates/runtime-durable-service/src/stage8b_p1_semantic.rs",
        "redis": "crates/runtime-durable-service/src/stage8b_p1_semantic/redis.rs",
        "lib": "crates/strategy-runtime-core/src/lib.rs",
        "doc": "docs/stage-8/stage8b-p1d1-market-provider-core.md",
        "matrix": "docs/stage-8/stage8b-p1d1-acceptance-matrix.csv",
        "evidence": "docs/stage-8/stage8b-p1d1-evidence.json",
        "status": "docs/current-status.md",
        "roadmap": "docs/roadmap.md",
    }
    return {key: (root / path).read_text(encoding="utf-8") for key, path in paths.items()}


def validate_content(content: dict[str, str]) -> None:
    provider = content["provider"]
    schedule = content["schedule"]
    dispatch = content["dispatch"]
    canonical = content["canonical"]
    redis_source = content["redis"]
    document = content["doc"]

    for token in (
        "pub(crate) fn begin_stage8b_p1d1_market_wait",
        "Stage8bP1d1AwaitingExecutionBar",
        "Stage8bP1d1ExecutionEligible",
        "Stage8bP1d1MarketDispatchReady",
        "Stage8bP1d1MarketOutcomeBundle",
        "Stage8bP1d1ExecutionObservation::NoInput",
        "Stage8bP1d1ExecutionEligibilityBlockReason::SameBar",
        "Stage8bP1d1ExecutionEligibilityBlockReason::ExecutionBarGap",
        "Stage8bP1d1ProviderError::TtlForbidden",
        "bind_stage8b_p1d1_market_dispatch",
        "stage8b_p1d1_identity()",
        "fill_price: eligibility.execution_bar.open",
        "fill_source_ts_utc_ms: eligibility.execution_bar.open_ts_utc_ms",
        'STAGE8B_P1D1_ORDER_ID_DOMAIN: &str = "moex.stage8b.p1d.order-id.v1"',
        'STAGE8B_P1D1_TRADE_ID_DOMAIN: &str = "moex.stage8b.p1d.trade-id.v1"',
        "feedback_application_allowed",
        "source_m10_xack_allowed",
    ):
        require(token in provider, f"provider invariant missing: {token}")
    require(
        "pub fn begin_stage8b_p1d1_market_wait" not in provider,
        "P1-d1 entry seam became public",
    )
    for forbidden in (
        "redis::",
        "reqwest::",
        "BrokerOrderSnapshot",
        "BrokerTradeSnapshot",
        "BrokerPositionSnapshot",
        "CommandAck",
        "Utc::now()",
    ):
        require(forbidden not in provider, f"closed provider surface opened: {forbidden}")

    bridge = "pub(crate) fn classify_stage8b_p1d1_execution_bar"
    require(schedule.count(bridge) == 1, "Stage5E eligibility bridge count drifted")
    for token in (
        "Stage5eScheduleSequenceClassification::Contiguous",
        "Stage5eScheduleSequenceClassification::ApprovedNonTradableBoundary",
        "Stage8bP1d1ScheduleBridgeBlockReason::ExecutionBarGap",
        "Stage8bP1d1ScheduleBridgeBlockReason::CrossTradingDay",
        "Stage8bP1d1ScheduleBridgeBlockReason::Expired",
        "_projection: projection",
    ):
        require(token in schedule, f"schedule invariant missing: {token}")

    require(
        "pub(crate) fn stage8b_p1d1_identity" in dispatch,
        "linear dispatch receipt binding missing",
    )
    require(
        "to_stage8b_p1d1_execution_bar" in canonical
        and "validated canonical decimal remains parseable" in canonical,
        "canonical M10 exact adapter missing",
    )
    require(
        "p1d1_execution_bar_observation" in redis_source
        and "paper_provider_invocation_allowed(&self) -> bool" in redis_source,
        "P1-c read-only observation seam missing",
    )
    require(
        "bind_stage8b_p1d1_market_dispatch" in content["lib"],
        "P1-d1 public opaque result exports missing",
    )

    for token in (
        "crate-private Stage5E bridge",
        "AwaitingExecutionBar",
        "exact linear Stage6dPaperDispatchReceipt",
        "fill_price          = execution_bar.open",
        "P1-d2 source\nmust not start until that annex is accepted",
        "operational Redis DB0",
        "FINAM POST/DELETE",
    ):
        require(token in document, f"implementation document missing: {token}")

    rows = list(csv.DictReader(content["matrix"].splitlines()))
    require(len(rows) == 30, f"acceptance row count drifted: {len(rows)}")
    require(
        {row.get("id") for row in rows} == {f"P1D1-{index:03d}" for index in range(1, 31)},
        "acceptance IDs drifted",
    )
    require(all(row.get("status") == "REQUIRED" for row in rows), "acceptance weakened")

    evidence = json.loads(content["evidence"])
    require(evidence.get("schema_version") == 1, "evidence schema drifted")
    require(evidence.get("stage") == "Stage 8B-P1-d1", "evidence stage drifted")
    require(
        evidence.get("status") == "SOURCE_IMPLEMENTATION_REVIEW_CANDIDATE",
        "evidence status drifted",
    )
    require(evidence.get("accepted_p1d0_predecessor_ref") == BASE, "lineage drifted")
    require(evidence.get("acceptance_rows") == 30, "evidence row count drifted")
    closed = evidence.get("closed_surfaces")
    require(isinstance(closed, dict) and len(closed) == 10, "closed surface count drifted")
    require(all(value is False for value in closed.values()), "closed surface opened")
    require(evidence.get("next_stage_authorized") is False, "P1-d2 opened early")
    require("P1-d1 provider core" in content["status"], "status not synchronized")
    require("P1-d1 provider core" in content["roadmap"], "roadmap not synchronized")


def main() -> None:
    try:
        actual = changed_files()
        require(actual == EXPECTED_CHANGED, f"source changed path drift: {sorted(actual)}")
        forbidden_diff = subprocess.run(
            ["git", "diff", "--name-only", BASE, "--", "Cargo.toml", "Cargo.lock", ".github"],
            cwd=ROOT,
            check=True,
            text=True,
            capture_output=True,
        ).stdout.strip()
        require(not forbidden_diff, f"Cargo/workflow changed: {forbidden_diff}")
        validate_content(load_content())
    except (CheckFailure, OSError, json.JSONDecodeError, subprocess.CalledProcessError) as error:
        print(f"FAIL stage8b-p1d1-source-scope: {error}", file=sys.stderr)
        raise SystemExit(1)
    print("PASS stage8b-p1d1-source-scope rows=30 db0=false finam=false ack=false xack=false")


if __name__ == "__main__":
    main()
