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
        "working": "crates/strategy-runtime-core/src/stage8b_p1d3_working_limit.rs",
        "recovery": "crates/runtime-durable-service/src/recovery.rs",
        "redis": "crates/runtime-durable-service/src/stage8b_p1_semantic/redis.rs",
        "process": "crates/runtime-durable-service/src/stage8b_p1e_process.rs",
        "document": "docs/stage-8/stage8b-p1e-i1-committed-cancel-day-expiry-recovery.md",
        "owner_document": "docs/stage-8/stage8b-p1e-i1-committed-owner-loop-wiring.md",
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
    working = content["working"]
    recovery = content["recovery"]
    redis = content["redis"]
    process = content["process"]
    document = content["document"]
    owner_document = content["owner_document"]

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
    for token in (
        "resume_stage8b_p1e_committed_cancel_with_redis(",
        "resume_stage8b_p1e_committed_day_expiry(",
        "CommittedDayExpiryPelNotEmpty",
        "CommittedCancelRestartRequired",
        "ready.is_committed_cancel_resolution()",
        "ready.into_committed_cancel_resolved()?",
    ):
        require(token in production_process, f"committed owner-loop invariant missing: {token}")

    cancel_completion = section(
        production_process,
        "pub struct Stage8bP1eCommittedCancelResolvedV1 {",
        "/// Terminal composition boundary after source-free committed Day-expiry.",
    )
    for token in (
        "ready: Stage8bP1eReadyPollingV1",
        "pub fn into_ready_polling(self) -> Stage8bP1eReadyPollingV1",
        "self.ready",
    ):
        require(token in cancel_completion, f"Cancel owner handoff missing: {token}")
    ready_conversion = section(
        production_process,
        "    fn into_committed_cancel_resolved(",
        "    /// Reclassifies an externally due day timer",
    )
    require("drop(owner)" not in ready_conversion, "Cancel completion destroys the Ready owner")
    for token in (
        "ready: Stage8bP1eReadyPollingV1",
        "owner,",
        "control,",
        "committed_cancel_disposition: None",
    ):
        require(token in ready_conversion, f"Cancel completion conversion missing: {token}")

    for token in (
        "terminal_semantic_commit_covers_or_follows_source",
        "terminal_semantic_continuation_allows",
        "semantic_ms >= source_ms",
        "> exact_m10_redis_id_ms(&current.m10_redis_id)?",
    ):
        require(token in working, f"terminal M10 continuation invariant missing: {token}")
    require(
        working.count("terminal_semantic_commit_covers_or_follows_source") == 2,
        "terminal semantic watermark definition/use count drift",
    )
    for token in (
        "terminal_semantic_commit_covers_or_follows_source",
        "terminal_semantic_continuation_allows",
    ):
        require(token in live, f"terminal M10 runtime transition missing: {token}")
    require(
        "Stage6Stage8bP1d3RestartPhase::TruthCommitted\n                | Stage6Stage8bP1d3RestartPhase::SemanticCallbackCommitted"
        in recovery,
        "terminal P1-d3 restart phases do not rejoin ordinary Ready-source polling",
    )

    generic_publish = section(
        redis,
        "    pub async fn publish_exact_command(",
        "    /// Generated-Market publication first commits",
    )
    for token in (
        "p1e_i1_observe_publication_attempt();",
        ".publish_exact_command(&self.durable, &self.pending_m10)",
        "p1e_i1_observe_publication_success();",
    ):
        require(token in generic_publish, f"generic publication audit missing: {token}")
    require(
        generic_publish.index("p1e_i1_observe_publication_attempt();")
        < generic_publish.index(".publish_exact_command(&self.durable, &self.pending_m10)")
        < generic_publish.index("p1e_i1_observe_publication_success();"),
        "publication attempt/success counters do not bracket the transport call",
    )
    xack = section(
        redis,
        "    async fn acknowledge_exact(",
        "    async fn publish_exact_command(",
    )
    for token in (
        "p1e_i1_observe_xack_attempt();",
        'redis::cmd("XACK")',
        "p1e_i1_take_xack_response_loss()",
        "p1e_i0_observe_xack();",
    ):
        require(token in xack, f"XACK attempt/success audit missing: {token}")
    require(
        xack.index("p1e_i1_observe_xack_attempt();")
        < xack.index('redis::cmd("XACK")')
        < xack.index("p1e_i1_take_xack_response_loss()")
        < xack.index("p1e_i0_observe_xack();"),
        "XACK attempt, response-loss and success evidence are not ordered around transport",
    )
    require(
        production_process.count(
            "CommittedCancelResolved(Stage8bP1eCommittedCancelResolvedV1)"
        )
        == 2,
        "both schedule-free and combined loops must expose typed Cancel completion",
    )
    require(
        production_process.count(
            "CommittedDayExpiryResolved(Stage8bP1eCommittedDayExpiryResolvedV1)"
        )
        == 1,
        "combined loop must expose exactly one typed Day-expiry completion",
    )
    require(
        "if control.pel_count().await? != 0" in production_process,
        "Day-expiry must fail closed unless the canonical M10 PEL is empty",
    )
    require(
        "Stage8bP1eOwnerLoopEntryV1::Ready(Stage8bP1eReadyPollingV1" not in section(
            production_process,
            "            } else if durable.is_day_expiry() {",
            "            } else if !durable.is_initial_limit() {",
        ),
        "committed Day-expiry cannot regain fresh polling authority",
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
        "production owner-loop wiring was closed at this immutable source boundary",
        "signal/panic/SIGKILL",
        "FINAM POST/DELETE/send",
    ):
        require(token in document, f"review boundary documentation missing: {token}")

    direct_cancel_test = section(
        process,
        "    async fn owner_loop_routes_committed_cancel_ack_truth_to_xack_last()",
        "    async fn owner_loop_routes_committed_target_first_cancel_to_xack_last()",
    )
    target_first_owner_test = section(
        process,
        "    async fn owner_loop_routes_committed_target_first_cancel_to_xack_last()",
        "    async fn owner_loop_routes_committed_day_expiry_source_free_to_terminal()",
    )
    day_expiry_owner_test = section(
        process,
        "    async fn owner_loop_routes_committed_day_expiry_source_free_to_terminal()",
        "    async fn owner_loop_rejects_committed_day_expiry_when_pel_is_not_empty()",
    )
    fresh_cancel_test = section(
        process,
        "    async fn bounded_signed_cancel_cycle_rejoins_ready_and_xacks_source_last()",
        "    async fn signed_cancel_waits_for_successor_without_republish_or_high_water_advance()",
    )
    for token in (
        "Stage8bP1eOwnerLoopOutcomeV1::CommittedCancelResolved",
        "Stage8bP1RedisZeroIntentAckDisposition::AcknowledgedPending",
        'allocation.outcome_kind == "cancel_canceled"',
        "seq_ack.checked_add(1)",
        "effects.provider_total",
        "effects.claim_total",
        "effects.xack_total",
        "effects.schedule_read_total, 0",
    ):
        require(token in direct_cancel_test, f"direct Cancel owner-loop evidence missing: {token}")
    for token in (
        "resolved.into_ready_polling()",
        "fresh CANCEL completion must hand its owner to the next canonical M10",
        "drain_stage8b_p1e_schedule_free_recovery_v1(",
    ):
        require(token in fresh_cancel_test, f"fresh Cancel forward-progress evidence missing: {token}")
    for token in (
        "resolved.into_ready_polling()",
        "recovered CANCEL completion must hand its owner to the next canonical M10",
        "drain_stage8b_p1e_schedule_free_recovery_v1(",
    ):
        require(token in direct_cancel_test, f"committed Cancel forward-progress evidence missing: {token}")
    for token in (
        "Stage8bP1eOwnerLoopOutcomeV1::CommittedCancelRestartRequired",
        "Stage8bP1eOwnerLoopOutcomeV1::CommittedCancelResolved",
        "Stage8bP1RedisZeroIntentAckDisposition::AlreadyAcknowledged",
        "replay_audit.sequence_allocations",
        "exact_sequence_allocations",
        "replay_effects.xack_total, 0",
        "replay_effects.schedule_read_total, 0",
        "replay_reader.test_read_attempts(), 0",
        'allocation.outcome_kind == "later_filled"',
        'allocation.outcome_kind == "cancel_execution_observed"',
        "target_truth_sequence.checked_add(1)",
        "p1e_i1_inject_xack_response_loss_once()",
        "effects.xack_attempt_total, 1",
        "effects.xack_total, 0",
        "replay_resolved.into_ready_polling()",
        "response-loss replay must hand its owner to the next canonical M10",
        "next_effects.callback_total, 1",
        "next_effects.publication_attempt_total, 1",
        "the successor M10 may publish only its own newly generated command",
    ):
        require(token in target_first_owner_test, f"target-first owner-loop evidence missing: {token}")

    cancel_publication_test = section(
        redis,
        "    async fn p1e_i1_cancel_publication_audit_counts_transport_attempt_and_success_separately()",
        "    async fn p1c_command_response_loss_republishes_exactly_once_and_retains_m10()",
    )
    for token in (
        "Stage8bP1ePublishedScheduleRouteV1::Cancel",
        "success.publication_attempt_total, 1",
        "success.publication_total, 1",
        "failure.publication_attempt_total, 1",
        "failure.publication_total, 0",
        "commands_after_failure, 0",
    ):
        require(token in cancel_publication_test, f"Cancel publication counter control missing: {token}")
    for token in (
        "Stage8bP1eOwnerLoopOutcomeV1::CommittedDayExpiryResolved",
        "effects.provider_total, 0",
        "effects.callback_total, 0",
        "effects.publication_total, 0",
        "effects.claim_total, 0",
        "effects.xack_total, 0",
        "effects.schedule_read_total, 0",
        "reader.test_read_attempts(), 0",
        'expiry.outcome_kind == "later_expired"',
        "expiry.seq_ack.is_none()",
        "expiry.sequence_allocation_frontier.checked_add(1)",
    ):
        require(token in day_expiry_owner_test, f"Day-expiry owner-loop evidence missing: {token}")
    for token in (
        "typed `CommittedCancelResolved`",
        "typed `CommittedDayExpiryResolved`",
        "explicit consuming handoff",
        "next canonical M10",
        "attempt and success counters",
        "actual command logs and exit codes",
        "SIGTERM/panic/SIGKILL",
        "FINAM POST/DELETE/send",
    ):
        require(token in owner_document, f"owner-loop documentation missing: {token}")


def main() -> None:
    validate_content(load_content())
    print("PASS stage8b-p1e-i1-committed-restart-source-check")


if __name__ == "__main__":
    main()
