#!/usr/bin/env python3
"""Fail-closed source/scope checker for Stage 8B-P1-d1 R1."""

from __future__ import annotations

import csv
import json
import pathlib
import subprocess
import sys


ROOT = pathlib.Path(__file__).resolve().parents[1]
POLICY_BASE = "0d59d54d42fc29ae7b31359c1ded8efbd3a348fd"
REVIEW_BASE = "61f798d605c5609302ad77e9b14cb6f5e9479f6a"
EXPECTED_CHANGED = {
    "crates/runtime-durable-service/src/recovery.rs",
    "crates/runtime-durable-service/src/stage8b_p1_semantic.rs",
    "crates/runtime-durable-service/src/stage8b_p1_semantic/redis.rs",
    "crates/strategy-runtime-core/src/lib.rs",
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
        ["git", "diff", "--name-only", REVIEW_BASE],
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
        "service": "crates/runtime-durable-service/src/recovery.rs",
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
    service = content["service"]
    document = content["doc"]

    for token in (
        "pub(crate) fn begin_stage8b_p1d1_market_wait",
        "Stage8bP1d1AwaitingExecutionBar",
        "Stage8bP1d1ExecutionEligible",
        "Stage8bP1d1MarketDispatchReady",
        "Stage8bP1d1MarketOutcomeBundle",
        "Stage8bP1d1ExecutionObservation::NoInput",
        "Stage8bP1d1CommandDecisionBinding",
        "Stage8bP1d1CanonicalExecutionAuthority",
        "pub(crate) fn stage8b_p1d1_command_decision_binding_from_source",
        "exact_redis_close_ms(&predecessor_redis_id)",
        "pub(crate) fn observe_candidates",
        "Stage8bP1d1ExecutionEligibilityBlockReason::SameBar",
        "Stage8bP1d1ExecutionEligibilityBlockReason::ExecutionBarGap",
        "Stage8bP1d1ProviderError::TtlForbidden",
        "bind_stage8b_p1d1_market_dispatch",
        "stage8b_p1d1_command_snapshot()",
        "stage8b_p1d1_accepted_payload_sha256()",
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
    require("pub fn observe_candidates" not in provider, "eligibility minting became public")
    for removed in (
        "Stage8bP1d1CanonicalExecutionBar",
        "Stage8bP1d1ExecutionBarObservation",
    ):
        require(removed not in provider, f"caller-constructible authority returned: {removed}")
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

    for token in (
        "pub struct Stage6dPaperDispatchReceipt {\n    identity: Stage6DurableRequestIdentityV1,\n    command_snapshot: Stage6DurableCommandSnapshotV1,\n    accepted_command_payload_sha256: Stage6Sha256Digest,",
        "command_snapshot: Stage6DurableCommandSnapshotV1",
        "accepted_command_payload_sha256: Stage6Sha256Digest",
        "pub fn admit_stage7a_p1d1_market_dispatch",
        "accepted_snapshot != eligibility.durable_command_snapshot()",
        "accepted.canonical_payload_sha256() != eligibility.accepted_command_payload_sha256()",
        "prepare_stage6d_existing_accepted_paper_dispatch(recovered, &accepted, dispatch)",
        "Stage6DispatchSafetyStateV1::ReadyForFirstDispatch",
        "Stage6DispatchSafetyStateV1::ReconciliationRequired",
    ):
        require(token in dispatch, f"eligibility-gated dispatch invariant missing: {token}")
    require(
        "to_stage8b_p1d1_execution_bar" not in canonical,
        "public canonical-M10 DTO adapter returned",
    )
    require(
        "p1d1_command_decision_binding" in redis_source
        and "p1d1_execution_bar_observation" not in redis_source
        and "paper_provider_invocation_allowed(&self) -> bool" in redis_source,
        "P1-c source-sealed decision seam drifted",
    )
    require(
        "pub fn admit_p1d1_eligible_market_dispatch" in service,
        "Stage7 owner eligibility-consuming seam missing",
    )
    require(
        "bind_stage8b_p1d1_market_dispatch" not in content["lib"]
        and "admit_stage7a_p1d1_market_dispatch" in content["lib"],
        "P1-d1 dispatch export boundary drifted",
    )

    for token in (
        "crate-private Stage5E bridge",
        "opaque `CommandDecisionBinding`",
        "P1-specific Stage7 transition",
        "AwaitingExecutionBar",
        "complete `Stage6DurableRequestIdentityV1`",
        "fill_price          = execution_bar.open",
        "P1-d2 source\nmust not start until that annex is accepted",
        "operational Redis DB0",
        "FINAM POST/DELETE",
    ):
        require(token in document, f"implementation document missing: {token}")

    rows = list(csv.DictReader(content["matrix"].splitlines()))
    require(len(rows) == 42, f"acceptance row count drifted: {len(rows)}")
    require(
        {row.get("id") for row in rows} == {f"P1D1-{index:03d}" for index in range(1, 43)},
        "acceptance IDs drifted",
    )
    require(all(row.get("status") == "REQUIRED" for row in rows), "acceptance weakened")

    evidence = json.loads(content["evidence"])
    require(evidence.get("schema_version") == 1, "evidence schema drifted")
    require(evidence.get("stage") == "Stage 8B-P1-d1", "evidence stage drifted")
    require(
        evidence.get("status") == "R1_CROSS_BINDING_REVIEW_CANDIDATE",
        "evidence status drifted",
    )
    require(evidence.get("accepted_p1d0_predecessor_ref") == POLICY_BASE, "policy lineage drifted")
    require(evidence.get("reviewed_p1d1_source_ref") == REVIEW_BASE, "review lineage drifted")
    require(evidence.get("acceptance_rows") == 42, "evidence row count drifted")
    closed = evidence.get("closed_surfaces")
    require(isinstance(closed, dict) and len(closed) == 10, "closed surface count drifted")
    require(all(value is False for value in closed.values()), "closed surface opened")
    require(evidence.get("next_stage_authorized") is False, "P1-d2 opened early")
    require("P1-d1 R1 exact-binding closure" in content["status"], "status not synchronized")
    require("P1-d1 provider core R1 exact-binding closure" in content["roadmap"], "roadmap not synchronized")


def main() -> None:
    try:
        actual = changed_files()
        require(actual == EXPECTED_CHANGED, f"source changed path drift: {sorted(actual)}")
        forbidden_diff = subprocess.run(
            ["git", "diff", "--name-only", REVIEW_BASE, "--", "Cargo.toml", "Cargo.lock", ".github"],
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
    print("PASS stage8b-p1d1-source-scope rows=42 r1=true db0=false finam=false ack=false xack=false")


if __name__ == "__main__":
    main()
