#!/usr/bin/env python3
"""Fail-closed source check for committed Cancel/Day-expiry restart recovery."""

from __future__ import annotations

import pathlib


ROOT = pathlib.Path(__file__).resolve().parents[1]


class CheckFailure(RuntimeError):
    pass


def require(condition: bool, message: str) -> None:
    if not condition:
        raise CheckFailure(message)


def load_content(root: pathlib.Path = ROOT) -> dict[str, str]:
    paths = {
        "core": "crates/strategy-runtime-core/src/stage5e_no_io_lifecycle.rs",
        "journal": "crates/strategy-runtime-core/src/stage6_reconciliation_v2.rs",
        "live": "crates/strategy-runtime-core/src/stage6d_live_core.rs",
        "recovery": "crates/runtime-durable-service/src/recovery.rs",
        "redis": "crates/runtime-durable-service/src/stage8b_p1_semantic/redis.rs",
        "process": "crates/runtime-durable-service/src/stage8b_p1e_process.rs",
        "document": "docs/stage-8/stage8b-p1e-i1-committed-cancel-day-expiry-recovery.md",
    }
    return {name: (root / path).read_text(encoding="utf-8") for name, path in paths.items()}


def section(text: str, start: str, end: str) -> str:
    begin = text.find(start)
    require(begin >= 0, f"section start missing: {start}")
    finish = text.find(end, begin + len(start))
    require(finish >= 0, f"section end missing: {end}")
    return text[begin:finish]


def validate_content(content: dict[str, str]) -> None:
    core = content["core"]
    journal = content["journal"]
    live = content["live"]
    recovery = content["recovery"]
    redis = content["redis"]
    process = content["process"]
    document = content["document"]

    for token in (
        "pub fn cancel_publication_seal(&self) -> Option<(u64, &str)>",
        "Stage8bP1eScheduleTransitionKindV1::CancelStep",
        "publication_seal_generation: Some(publication_seal.0)",
        "publication_seal_commitment_sha256: Some(publication_seal.1.into())",
    ):
        require(token in core, f"Cancel V4 binding invariant missing: {token}")
    require(
        core.count("pub fn cancel_publication_seal(&self) -> Option<(u64, &str)>") == 2,
        "candidate and committed-record Cancel publication seal accessors must both remain",
    )

    for token in (
        "Stage8bP1eScheduleTransitionKindV1::CancelStep =>",
        "binding.publication_seal_generation",
        "binding.publication_seal_commitment_sha256",
    ):
        require(token in journal, f"Cancel V4 journal invariant missing: {token}")

    cancel_live = section(
        live,
        "crate::Stage8bP1eScheduleTransitionKindV1::CancelStep => {",
        "crate::Stage8bP1eScheduleTransitionKindV1::WorkingLimitEvaluation",
    )
    for token in (
        "replacement.matches_stage8b_p1e_schedule_candidate(candidate)?",
        "let BrokerCommand::CancelOrder(cancel)",
        "evidence.canonical_command_sha256.as_deref()",
        "binding.active_broker_order_id.as_deref()",
        "evidence.m10_redis_id == candidate.predecessor_m10().redis_id",
        "Stage6JournalEventKind::RequestAccepted",
    ):
        require(token in cancel_live, f"runtime Cancel cross-binding missing: {token}")

    cancel_material = section(
        recovery,
        "    pub(crate) fn cancel_restart_material(",
        "    pub(crate) fn day_expiry_restart_material(",
    )
    for token in (
        "pub(crate) struct Stage8bP1eCancelRestartMaterial",
        "pub(crate) struct Stage8bP1eDayExpiryRestartMaterial",
        "pub(crate) fn cancel_restart_material(",
        "pub(crate) fn day_expiry_restart_material(",
        "publication_seal_generation.checked_add(1)",
        "binding.cancel_publication_seal()",
        "stage8b_p1e_day_expiry_binding_parts()?",
    ):
        require(token in recovery, f"restart material invariant missing: {token}")
    require(
        "publication_seal_generation.checked_add(1)" in cancel_material,
        "Cancel publication seal must be the exact predecessor of the covering V4 seal",
    )

    cancel_resume = section(
        redis,
        "pub async fn resume_stage8b_p1e_committed_cancel_with_redis(",
        "/// Resumes a source-free Day-expiry V4.",
    )
    for token in (
        ".reclaim_exact_binding(",
        ".revalidate_exact_command_publication(",
        ".exact_first_successor_m10(",
        "stage8b_p1e_m10_identity_from_validated(&successor) != material.candidate_m10",
        ".commit_stage8b_p1d3_cancel(",
        "Stage8bP1d3CancelCommitOutcome::CancelContinuationPending",
    ):
        require(token in cancel_resume, f"committed Cancel recovery invariant missing: {token}")
    for forbidden in (
        "Stage8bP1eRedisScheduleReader",
        "read_newest_guarded",
        "publish_exact_command",
        "acknowledge_exact",
    ):
        require(forbidden not in cancel_resume, f"committed Cancel reopened forbidden effect: {forbidden}")

    expiry_resume = section(
        redis,
        "pub fn resume_stage8b_p1e_committed_day_expiry(",
        "/// Binds one already verified signed snapshot",
    )
    for token in (
        ".day_expiry_restart_material()?",
        "continue_stage8b_p1e_day_expiry_schedule",
        ".expire_working_limit(authority, commitment_key)?",
    ):
        require(token in expiry_resume, f"committed Day-expiry invariant missing: {token}")
    for forbidden in (
        "Stage8bP1PendingM10Delivery",
        "reclaim_exact_binding",
        "read_newest_guarded",
        "acknowledge_exact",
    ):
        require(forbidden not in expiry_resume, f"Day-expiry is no longer source-free: {forbidden}")

    production_process = process.split("#[cfg(test)]\nmod tests {", 1)[0]
    require(
        "resume_stage8b_p1e_committed_cancel_with_redis" not in production_process
        and "resume_stage8b_p1e_committed_day_expiry" not in production_process,
        "production owner-loop wiring opened inside the restart-only slice",
    )
    cancel_test = section(
        process,
        "    async fn committed_target_first_cancel_restart_preserves_pel_and_xacks_truth_last()",
        "    async fn committed_day_expiry_restart_is_source_free_and_does_not_reread_schedule()",
    )
    expiry_test = section(
        process,
        "    async fn committed_day_expiry_restart_is_source_free_and_does_not_reread_schedule()",
        "    async fn bounded_schedule_backoff_observes_shutdown_before_another_redis_read()",
    )
    for token in (
        "committed_target_first_cancel_restart_preserves_pel_and_xacks_truth_last",
        "committed_day_expiry_restart_is_source_free_and_does_not_reread_schedule",
    ):
        require(token in process, f"restart acceptance evidence missing: {token}")
    for token in (
        '"target truth cannot acknowledge M10"',
        '"S_truth must precede XACK-last"',
        "assert_eq!(pending_after.count(), 0)",
        '"committed Cancel restart cannot reread signed schedule"',
    ):
        require(token in cancel_test, f"Cancel restart acceptance evidence missing: {token}")
    for token in (
        '"committed Day-expiry restart cannot reread signed schedule"',
        '"source-free restart cannot XACK"',
    ):
        require(token in expiry_test, f"Day-expiry restart acceptance evidence missing: {token}")

    for token in (
        "target-first",
        "truth-before-XACK",
        "source-free",
        "production owner-loop wiring remains closed",
        "signal/panic/SIGKILL",
        "FINAM POST/DELETE/send",
    ):
        require(token in document, f"review boundary documentation missing: {token}")


def main() -> None:
    validate_content(load_content())
    print("PASS stage8b-p1e-i1-committed-restart-source-check")


if __name__ == "__main__":
    main()
