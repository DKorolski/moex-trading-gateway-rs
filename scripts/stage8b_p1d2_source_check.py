#!/usr/bin/env python3
"""Fail-closed source/scope checker for Stage 8B-P1-d2."""

from __future__ import annotations

import csv
import json
import pathlib
import subprocess
import sys


ROOT = pathlib.Path(__file__).resolve().parents[1]
ACCEPTED_ANNEX = "0cf1cd810a6ff479b69afb914db3b2aa2259593a"
EXPECTED_CHANGED = {
    "crates/runtime-durable-service/src/lib.rs",
    "crates/runtime-durable-service/src/recovery.rs",
    "crates/runtime-durable-service/src/stage8b_p1_semantic.rs",
    "crates/runtime-durable-service/src/stage8b_p1_semantic/redis.rs",
    "crates/strategy-runtime-core/src/lib.rs",
    "crates/strategy-runtime-core/src/stage5c_paper_host.rs",
    "crates/strategy-runtime-core/src/stage5e_no_io_lifecycle.rs",
    "crates/strategy-runtime-core/src/stage5g_clean_restart.rs",
    "crates/strategy-runtime-core/src/stage5g_mock_ack.rs",
    "crates/strategy-runtime-core/src/stage5g_order_position.rs",
    "crates/strategy-runtime-core/src/stage5g_timer.rs",
    "crates/strategy-runtime-core/src/stage6d_live_core.rs",
    "crates/strategy-runtime-core/src/stage8b_p1d1_paper_provider.rs",
    "crates/strategy-runtime-core/src/stage8b_p1d2_market_feedback.rs",
    "docs/current-status.md",
    "docs/roadmap.md",
    "docs/stage-8/stage8b-p1d2-market-feedback-source.md",
    "docs/stage-8/stage8b-p1d2-source-acceptance-matrix.csv",
    "docs/stage-8/stage8b-p1d2-source-evidence.json",
    "scripts/make_stage8b_p1d2_source_handoff.py",
    "scripts/stage8b_p1d2_source_check.py",
    "scripts/stage8b_p1d2_source_gate.sh",
    "scripts/stage8b_p1d2_source_handoff_safety_check.py",
    "scripts/stage8b_p1d2_source_negative_harness.py",
}


class CheckFailure(RuntimeError):
    pass


def require(condition: bool, message: str) -> None:
    if not condition:
        raise CheckFailure(message)


def changed_files() -> set[str]:
    tracked = subprocess.run(
        ["git", "diff", "--name-only", ACCEPTED_ANNEX],
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
        "feedback": "crates/strategy-runtime-core/src/stage8b_p1d2_market_feedback.rs",
        "provider": "crates/strategy-runtime-core/src/stage8b_p1d1_paper_provider.rs",
        "schedule": "crates/strategy-runtime-core/src/stage5e_no_io_lifecycle.rs",
        "order_position": "crates/strategy-runtime-core/src/stage5g_order_position.rs",
        "restart": "crates/strategy-runtime-core/src/stage5g_clean_restart.rs",
        "stage6": "crates/strategy-runtime-core/src/stage6d_live_core.rs",
        "journal": "crates/strategy-runtime-core/src/stage6_journal_backend.rs",
        "service": "crates/runtime-durable-service/src/recovery.rs",
        "redis": "crates/runtime-durable-service/src/stage8b_p1_semantic/redis.rs",
        "service_lib": "crates/runtime-durable-service/src/lib.rs",
        "doc": "docs/stage-8/stage8b-p1d2-market-feedback-source.md",
        "matrix": "docs/stage-8/stage8b-p1d2-source-acceptance-matrix.csv",
        "evidence": "docs/stage-8/stage8b-p1d2-source-evidence.json",
        "status": "docs/current-status.md",
        "roadmap": "docs/roadmap.md",
    }
    return {key: (root / path).read_text(encoding="utf-8") for key, path in paths.items()}


def section(text: str, start: str, end: str) -> str:
    begin = text.find(start)
    require(begin >= 0, f"section start missing: {start}")
    finish = text.find(end, begin + len(start))
    require(finish >= 0, f"section end missing: {end}")
    return text[begin:finish]


def validate_content(content: dict[str, str]) -> None:
    feedback = content["feedback"]
    provider = content["provider"]
    schedule = content["schedule"]
    order_position = content["order_position"]
    restart = content["restart"]
    stage6 = content["stage6"]
    journal = content["journal"]
    service = content["service"]
    redis_source = content["redis"]
    service_lib = content["service_lib"]
    document = content["doc"]

    for token in (
        'STAGE8B_P1D2_MARKET_FEEDBACK_DOMAIN: &str = "moex.stage8b.p1d2.market-feedback.v1"',
        "mint_stage8b_p1d2_finalized_market_feedback",
        "facts.final_disposition() != Stage6RequestFinalDispositionV1::Completed",
        "facts.broker_trade_ids().len() == 1",
        ".checked_sub(outcome.fill_source_ts_utc_ms)",
        "Stage5gCleanRestartSource::P1d2Ack(source)",
        "Stage5gCleanRestartSource::P1d2Truth(source)",
        "Stage8bP1d2FeedbackAuditCoreV1",
        "p1d1_outcome_sha256",
        "stage6_report_sha256",
        "RoundingStrategy::MidpointNearestEven",
        "STAGE8B_P1D2_AVG_PRICE_SCALE: u32 = 8",
    ):
        require(token in feedback, f"feedback invariant missing: {token}")
    for forbidden in ("redis::", "reqwest::", "Utc::now()", "f32", "f64"):
        require(forbidden not in feedback, f"feedback authority opened forbidden input: {forbidden}")
    require(
        "stage5g_restart_candidate_context" not in feedback,
        "generic restart sequence helper became P1-d2 authority",
    )
    for token in (
        'std::env::var_os("STAGE8B_P1_TEST_SEQUENCE_PAIR_MARKER")',
        "sequence-pair marker must be durable before crash barrier",
    ):
        require(token in feedback, f"pre-kill pair evidence missing: {token}")

    for token in (
        "pub struct Stage8bP1d1ExecutionScheduleAuthority {",
        "projection: Stage5eScheduleProjectionBridgeInput",
        "pub(crate) fn stage8b_p1d1_schedule_authority_from_stage5e(",
        "schedule_authority: Stage8bP1d1ExecutionScheduleAuthority",
        "begin_stage8b_p1d1_market_wait(schedule_authority.projection, decision)",
    ):
        require(token in provider, f"schedule-authority invariant missing: {token}")
    require(
        "stage8b_p1d1_contiguous_schedule_projection" not in provider
        and "stage8b_p1d1_contiguous_schedule_projection" not in schedule,
        "synthetic Stage5E schedule projection was reintroduced",
    )
    require(
        '#[cfg(any(test, feature = "stage5g-artifact-fixtures"))]\n'
        "    pub(crate) fn stage8b_p1d1_test_schedule_projection(" in schedule,
        "schedule fixture escaped its test/fixture feature boundary",
    )
    recovery_projection = section(
        provider,
        "pub(crate) fn reconstruct_stage8b_p1d1_market_outcome_evidence(",
        "fn deterministic_market_outcome_evidence(",
    )
    for token in ("validate_execution_candidate", ".checked_add(M10_MILLIS)"):
        require(token in recovery_projection, f"recovery source validation missing: {token}")
    for forbidden in ("schedule_authority", "begin_stage8b_p1d1_market_wait"):
        require(
            forbidden not in recovery_projection,
            f"restart improperly reacquires schedule authority: {forbidden}",
        )

    for token in (
        "stage8b_p1d2_truth_sequence_from_ack_state",
        ".canonical_total_sequence",
        ".checked_add(1)",
        "p1d2_truth_sequence_is_derived_only_from_exact_resolved_ack_slot",
        "p1d2_truth_restart_rejects_stale_reversed_and_new_sequences",
    ):
        require(token in order_position, f"sequence invariant missing: {token}")

    for token in (
        "P1d2Ack(crate::stage8b_p1d2_market_feedback::Stage8bP1d2AckRestartSource)",
        "P1d2Truth(crate::stage8b_p1d2_market_feedback::Stage8bP1d2TruthRestartSource)",
        "Stage5gCleanRestartLifecycleKind::OrderPositionAwaitingCommitted",
        "stage8b_p1d2_validate_ack_frontier",
        "stage8b_p1d2_validate_truth_frontier",
    ):
        require(token in restart, f"replacement-package invariant missing: {token}")

    for token in (
        "apply_stage8b_p1d2_ack_transition",
        "apply_stage8b_p1d2_recovered_ack_transition",
        "apply_stage8b_p1d2_truth_transition",
        'stage8b_p1d2_test_crash_barrier("p1d2-after-stage6-before-request-finalized")',
        'stage8b_p1d2_test_crash_barrier("p1d2-after-request-finalized-before-ack")',
    ):
        require(token in stage6, f"Stage6/7 invariant missing: {token}")
    for token in ("self.file.sync_data()", "self.scan = scan_reader(&mut self.file, length)"):
        require(token in journal, f"journal persist/reread invariant missing: {token}")

    for token in (
        "Stage8bP1d2PreAckPendingOwner",
        "Stage8bP1d2AckCommittedOwner",
        "Stage8bP1d2TruthCommittedOwner",
        "restart_stage8b_p1d2_pre_ack_pending",
        "apply_stage8b_p1d2_recovered_ack_transition",
        'stage8b_p1_test_crash_barrier("p1d2-after-ack-before-s-ack")',
        'stage8b_p1_test_crash_barrier("p1d2-after-s-ack-before-truth")',
        'stage8b_p1_test_crash_barrier("p1d2-after-truth-before-s-truth")',
        'stage8b_p1_test_crash_barrier("p1d2-after-s-truth-before-xack")',
        "Stage8bP1d2FeedbackAuditEvidenceV1",
    ):
        require(token in service, f"service lifecycle invariant missing: {token}")
    require(
        service.count("let ready = commit_stage8b_p1_replacement_seal(") >= 3,
        "P1-d2 replacement-seal calls missing",
    )

    ack_impl = section(
        redis_source,
        "impl Stage8bP1RedisFeedbackAckCommitted {",
        "impl Stage8bP1RedisFeedbackTruthCommitted {",
    )
    truth_impl = section(
        redis_source,
        "impl Stage8bP1RedisFeedbackTruthCommitted {",
        "impl Stage8bP1RedisFeedbackResolved {",
    )
    require("pub fn commit_truth(" in ack_impl, "S_ack truth-only continuation missing")
    require("acknowledge_source" not in ack_impl, "S_ack gained early XACK")
    require("replay_ack" not in ack_impl, "S_ack gained ACK replay")
    require("pub async fn acknowledge_source(" in truth_impl, "S_truth XACK continuation missing")
    require("commit_truth" not in truth_impl, "S_truth gained duplicate truth")
    require(
        truth_impl.find("feedback_audit_evidence()?") < truth_impl.find("acknowledge_exact("),
        "audit validation no longer precedes XACK",
    )
    for token in (
        "exact_first_successor_m10",
        ".checked_add(600_000)",
        "schedule_authority: Stage8bP1d1ExecutionScheduleAuthority",
        "p1d2_market_feedback_commits_ack_then_truth_then_xacks_source",
        "p1d2_missing_successor_fails_before_feedback_and_retains_source",
        "p1d2_noncontiguous_successor_fails_before_feedback_and_retains_source",
        "p1d2_subprocess_kill_matrix_recovers_all_six_durable_frontiers",
        "p1d2_sequence_pair_allocation_crash_reconstructs_exact_ack_path",
        "pre_kill_sequence_pair",
        "restart must recover the exact pre-kill sequence pair",
    ):
        require(token in redis_source, f"Redis/recovery evidence missing: {token}")

    for token in (
        "Stage8bP1RedisFeedbackAckCommitted",
        "ack.acknowledge_source()",
        "ack.replay_ack()",
        "Stage8bP1RedisFeedbackTruthCommitted",
        "truth.commit_truth(",
    ):
        require(token in service_lib, f"compile-fail boundary missing: {token}")

    for token in (
        "source implementation review candidate",
        "replacement S_ack",
        "replacement S_truth",
        "source M10 XACK last",
        "separate test-only crash marker",
        "narrowly generalized equivalent allowed by R1A",
        "opaque one-use `Stage8bP1d1ExecutionScheduleAuthority`",
        "No synthetic",
        "never reacquires or",
        "reconstructs schedule evidence",
        "operational Redis DB 0",
        "FINAM POST/DELETE",
    ):
        require(token in document, f"implementation document missing: {token}")

    rows = list(csv.DictReader(content["matrix"].splitlines()))
    require(len(rows) == 44, f"acceptance row count drifted: {len(rows)}")
    require(
        {row.get("id") for row in rows} == {f"P1D2S-{index:03d}" for index in range(1, 45)},
        "acceptance IDs drifted",
    )
    require(all(row.get("status") == "REQUIRED" for row in rows), "acceptance weakened")

    evidence = json.loads(content["evidence"])
    require(evidence.get("schema_version") == 1, "evidence schema drifted")
    require(evidence.get("stage") == "Stage 8B-P1-d2 Market feedback source", "stage drifted")
    require(
        evidence.get("status") == "SOURCE_IMPLEMENTATION_REVIEW_CANDIDATE",
        "candidate status drifted",
    )
    require(evidence.get("accepted_projection_annex_ref") == ACCEPTED_ANNEX, "lineage drifted")
    require(evidence.get("acceptance_rows") == 44, "evidence row count drifted")
    require(evidence.get("negative_cases") == 27, "negative count drifted")
    require(
        evidence.get("implementation", {}).get("opaque_source_schedule_authority_required") is True,
        "source schedule authority evidence drifted",
    )
    require(
        evidence.get("implementation", {}).get("pair_pre_kill_post_restart_exact_comparison")
        is True,
        "exact pre-kill/restart pair evidence drifted",
    )
    require(evidence.get("crash_frontiers") == 6, "crash frontier count drifted")
    require(evidence.get("next_stage_authorized") is False, "next stage opened early")
    closed = evidence.get("closed_surfaces")
    require(isinstance(closed, dict) and len(closed) == 8, "closed surfaces drifted")
    require(all(value is False for value in closed.values()), "closed surface opened")
    require(
        "The P1-d2 Market feedback source" in content["status"]
        and "implementation is independently accepted" in content["status"]
        and "b8f09b5656bedf2c5b5828047a1fbbddbf988126" in content["status"],
        "status drifted",
    )
    require("P1-d2 governance closure" in content["roadmap"], "roadmap drifted")


def main() -> None:
    try:
        actual = changed_files()
        require(actual == EXPECTED_CHANGED, f"source changed path drift: {sorted(actual)}")
        forbidden = subprocess.run(
            ["git", "diff", "--name-only", ACCEPTED_ANNEX, "--", "Cargo.toml", "Cargo.lock", ".github"],
            cwd=ROOT,
            check=True,
            text=True,
            capture_output=True,
        ).stdout.strip()
        require(not forbidden, f"Cargo/workflow changed: {forbidden}")
        authority = subprocess.run(
            ["git", "diff", "--name-only", ACCEPTED_ANNEX, "--", "docs/stage-8/gov-ci-1-authority.json"],
            cwd=ROOT,
            check=True,
            text=True,
            capture_output=True,
        ).stdout.strip()
        require(not authority, "current-tree authority rebound before source acceptance")
        validate_content(load_content())
    except (CheckFailure, OSError, json.JSONDecodeError, subprocess.CalledProcessError) as error:
        print(f"FAIL stage8b-p1d2-source-scope: {error}", file=sys.stderr)
        raise SystemExit(1)
    print(
        "PASS stage8b-p1d2-source-scope "
        "rows=44 negatives=27 crash=6 pair_exact=true schedule=source-authority "
        "db0=false finam=false live=false"
    )


if __name__ == "__main__":
    main()
