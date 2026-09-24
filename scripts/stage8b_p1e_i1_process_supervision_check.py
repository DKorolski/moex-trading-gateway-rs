#!/usr/bin/env python3
"""Fail-closed source check for the I1 OS-process supervision slice."""

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
        "process": "crates/runtime-durable-service/src/stage8b_p1e_process.rs",
        "semantic": "crates/runtime-durable-service/src/stage8b_p1_semantic/redis.rs",
        "supervisor": "crates/runtime-durable-service/src/stage8b_p1_supervisor.rs",
        "first_boot": "crates/runtime-durable-service/src/stage8b_p1e_first_boot_source.rs",
        "transaction": "crates/runtime-durable-service/src/stage8b_p1e_first_boot_transaction.rs",
        "core": "crates/strategy-runtime-core/src/stage6d_live_core.rs",
        "binary": "crates/runtime-durable-service/src/bin/stage8b-p1-paper-supervisor.rs",
        "document": "docs/stage-8/stage8b-p1e-i1-process-supervision-matrix.md",
        "checkpoint": "docs/stage-8/stage8b-p1e-i1-process-composition-checkpoint.md",
        "status": "docs/current-status.md",
    }
    return {key: (root / path).read_text() for key, path in paths.items()}


def require_order(text: str, tokens: tuple[str, ...], message: str) -> None:
    cursor = -1
    for token in tokens:
        cursor = text.find(token, cursor + 1)
        require(cursor >= 0, f"{message}: missing or reordered {token}")


def section(text: str, start: str, end: str) -> str:
    begin = text.find(start)
    finish = text.find(end, begin + len(start))
    require(begin >= 0 and finish > begin, f"section missing: {start}")
    return text[begin:finish]


def validate_content(content: dict[str, str]) -> None:
    process = content["process"]
    semantic = content["semantic"]
    supervisor = content["supervisor"]
    first_boot = content["first_boot"]
    transaction = content["transaction"]
    core = content["core"]
    binary = content["binary"]
    document = content["document"]
    checkpoint = content["checkpoint"]
    status = content["status"]
    admission = section(
        transaction,
        "pub fn admit_stage8b_p1e_ordinary_run_v1(",
        "fn classify_stage8b_p1e_first_boot_v5_inner(",
    )

    require("OwnerLoopUnavailable" not in process, "run placeholder remains in source")
    require_order(
        process,
        (
            "Stage8bP1eProcessCommandV1::Run => execute_run",
            "async fn run_stage8b_p1e_process_owner_v1",
            "async fn execute_run",
            "async fn supervise_stage8b_p1e_owner_task_v1",
            "fn finish_owner_task",
        ),
        "production process composition",
    )

    run = section(process, "async fn execute_run(", "async fn run_stage8b_p1e_production_owner_v1(")
    production_owner = section(
        process,
        "async fn run_stage8b_p1e_production_owner_v1(",
        "#[cfg(test)]\nasync fn stage8b_p1e_process_startup_test_barrier_v1(",
    )
    owner = section(
        process,
        "async fn run_stage8b_p1e_process_owner_v1(",
        "async fn execute_run(",
    )
    supervision = section(
        process,
        "async fn supervise_stage8b_p1e_owner_task_v1(",
        "fn map_redis_attach_error(",
    )
    finish = section(process, "fn finish_owner_task_at(", "fn terminal_process_result(")
    startup_await = section(
        process,
        "async fn await_stage8b_p1e_startup_operation_v1<F>(",
        "#[cfg(not(test))]\nasync fn stage8b_p1e_process_startup_test_barrier_v1(",
    )
    require_order(
        run,
        (
            "SignalKind::terminate()",
            "SignalKind::interrupt()",
            "let coordinator = Stage8bP1eCoordinatorV1::new()",
            "let latch = coordinator.shutdown_latch()",
            "let owner = tokio::spawn(run_stage8b_p1e_production_owner_v1(",
            "supervise_stage8b_p1e_owner_task_v1",
        ),
        "run supervision order",
    )
    require_order(
        production_owner,
        (
            'stage8b_p1e_process_startup_test_barrier_v1("before-admission", &latch)',
            "load_stage8b_p1_commitment_key_from_systemd_credential()",
            "admit_stage8b_p1e_ordinary_run_v1(",
            ".stage8b_p1e_telemetry_seal_v1()",
            'stage8b_p1e_process_startup_test_barrier_v1("after-admission", &latch)',
            'stage8b_p1e_process_startup_test_barrier_v1("before-redis-attach", &latch)',
            '"inflight-redis-attach"',
            "attach_stage8b_p1e_verified_redis(&attach_plan)",
            ".clean_stale_zero_pending_consumers(&consumer_name)",
            ".telemetry_publisher()",
            'stage8b_p1e_process_startup_test_barrier_v1("before-s06-acquisition", &latch)',
            '"inflight-s06-acquisition"',
            "acquire_stage8b_p1e_startup_owner_v1(attachable, session)",
            "let initial_pel_count = startup",
            "let reader = crate::Stage8bP1eRedisScheduleReader::connect(&redis_url)",
            "tokio::spawn(run_stage8b_p1e_production_telemetry_v1(",
            "run_stage8b_p1e_process_owner_with_telemetry_v1(",
        ),
        "production startup order",
    )
    require(
        "return Err(Stage8bP1eProcessErrorV1::DurableRestart);" in production_owner,
        "pre-admission shutdown may be reported as authenticated success",
    )
    for token in (
        "expected_registry_identity_sha256: supervisor",
        "expected_registry_version: supervisor.schedule_registry_version()",
        "Stage8bP1eProcessSuccessV1::RunStopped",
    ):
        require(token in process, f"process invariant missing: {token}")
    for token in (
        "Stage8bP1eSupervisorEventV1::ExternalSignal",
        "Stage8bP1eSupervisorEventV1::GraceExpired",
    ):
        require(token in supervision, f"supervision invariant missing: {token}")
    require(
        supervision.count("Stage8bP1eSupervisorEventV1::GraceExpired") == 2,
        "both signal-failure and external-signal grace paths must remain explicit",
    )
    require(
        process.count("async fn await_stage8b_p1e_startup_operation_v1<F>(") == 1,
        "startup operation must have one production/test implementation",
    )
    for token in (
        "tokio::pin!(future)",
        "output = &mut future",
        "wait_for_stage8b_p1e_shutdown_v1(latch)",
    ):
        require(token in startup_await, f"startup select invariant missing: {token}")
    for forbidden in ("std::env", "Poll::Pending", "request-pending", "stubborn"):
        require(forbidden not in startup_await, f"startup select contains test fork: {forbidden}")
    for token in (
        "Stage8bP1eSupervisorEventV1::OwnerPanicked",
        "Stage8bP1eSupervisorEventV1::AuthenticatedBoundaryReached",
    ):
        require(token in finish, f"terminal mapping invariant missing: {token}")
    for token in (
        "Stage8bP1eOwnerLoopOutcomeV1::CommittedCancelResolved(resolved)",
        "resolved.into_ready_polling()",
        "Stage8bP1eOwnerTaskBoundaryV1::RestartRequired",
    ):
        require(token in owner, f"owner handoff invariant missing: {token}")

    for error, code in (
        ("Self::RedisAttach | Self::ScheduleReader | Self::OwnerLoop", "67"),
        ("Self::OwnerTaskFailed", "70"),
        ("Self::TelemetryFailed", "71"),
        ("Self::ShutdownGraceExpired", "72"),
        ("Self::SignalTaskFailed", "73"),
    ):
        require(error in process and code in process, f"exit class missing: {error}/{code}")
    require(
        "process_errors_keep_the_accepted_stable_exit_classes" in process,
        "exit-class executable control missing",
    )

    for field in (
        "pub schedule_registry_version: String",
        "pub schedule_registry_identity_sha256: String",
        "valid_registry_version(&config.schedule_registry_version)",
        "is_sha256_hex(&config.schedule_registry_identity_sha256)",
        "pub fn shutdown_latch(&self) -> Arc<Stage8bP1eShutdownLatchV1>",
    ):
        require(field in supervisor, f"supervisor binding missing: {field}")

    for token in (
        "pub fn admit_stage8b_p1e_ordinary_run_v1(",
        "STAGE8B_P1E_TRANSACTION_MARKER_TEMP_FILE",
        "STAGE8B_P1E_FIRST_BOOT_RECEIPT_TEMP_FILE",
        "stage8b_p1e_current_restart_package_audit(commitment_key)",
        "first_boot_provenance_canonical_sha256",
    ):
        require(token in admission, f"ordinary-run admission invariant missing: {token}")

    require(
        "pub const STAGE8B_P1E_ADOPTION_PREDICATE_VERSION: u16 = 2" in transaction,
        "V5 adoption predicate version is not exact v2",
    )
    require(
        transaction.count("Stage5gCleanRestartSource::P1BootstrapReady(source)") == 2
        and "Stage5gCleanRestartSource::TimerReady(source)" not in transaction,
        "fresh and historical V5 exports are not both P1BootstrapReady",
    )
    require(
        transaction.count('ascii_field("authenticated_stage5g_phase", "P1SemanticReady")')
        == 2,
        "adoption digest and its exact test vector do not both bind P1SemanticReady",
    )
    for token in (
        "Stage5gCleanRestartLifecycleKind::P1SemanticReady",
        "restart.stage8b_p1_semantic_commit().is_none()",
        "summary.stage5c_callback_count == 1",
        "self.replay.requests().is_empty()",
        "self.journal.frontier().frame_count() == 0",
    ):
        require(token in core, f"zero-effect initial P1 predicate missing: {token}")

    for test in (
        "ordinary_run_admission_rejects_post_seal_frontiers_without_mutation",
        "ordinary_run_admission_accepts_exact_adopted_authority_and_rejects_temp",
        "ordinary_run_admission_rejects_missing_corrupt_and_foreign_authority_without_mutation",
    ):
        require(test in first_boot, f"ordinary-run admission test missing: {test}")

    for test in (
        "os_process_idle_sigterm_exits_zero_at_authenticated_boundary",
        "os_process_sigkill_then_restart_preserves_single_ready_owner",
        "os_process_owner_panic_exits_exact_class_70",
        "os_process_consumes_committed_cancel_handoff_and_keeps_polling",
        "os_process_sigkill_after_cancel_truth_recovers_xack_last_and_keeps_polling",
        "production_run_signals_cover_admission_attach_and_s06_without_effects",
        "process_wrapper_preserves_coordinator_boundary_exit_classes",
        "process_wrapper_keeps_owner_panic_fatal_precedence",
        "production_run_cancels_server_processed_redis_attach_and_s06_requests",
        "common_supervisor_maps_noncooperative_owner_grace_expiry_to_72",
        "production_v5_bootstrap_runs_continuous_market_lifecycle_and_readmits_exactly",
    ):
        require(test in process, f"process matrix test missing: {test}")

    for token in (
        "Stage8bP1eStartupAwaitV1",
        "await_stage8b_p1e_startup_operation_v1",
        "RedisResponseDelayProxy",
        "RedisResponseDelayTarget::AttachManifestGet",
        "RedisResponseDelayTarget::S06Pending",
        '"redis-response-received-and-withheld:GET:{}"',
        '"redis-response-received-and-withheld:XPENDING:{stream}:{group}"',
        '"noncooperative-owner-live"',
        "std::future::pending::<",
        "Some(72)",
        "p1e_i1_begin_direct_effect_audit",
        "p1e_test_v5_plain_market_published_from_owner",
        '"the fresh V5 Market command must publish exactly once"',
        "Stage8bP1RedisZeroIntentAckDisposition::AlreadyAcknowledged",
    ):
        require(token in process, f"in-flight/V5 process evidence missing: {token}")
    require(
        process.count("match await_stage8b_p1e_startup_operation_v1(") == 2,
        "both Redis attach and S06 acquisition must use the shutdown-aware in-flight wrapper",
    )
    require("PROCESS_INFLIGHT_MODE" not in process, "test-only in-flight mode remains")
    require("p1e_test_v5_cancel_published_from_owner" not in process,
            "process witness still claims an injected Cancel chain")

    continuous_helper = section(
        semantic,
        "pub(crate) async fn p1e_test_v5_plain_market_published_from_owner(",
        "#[cfg(feature = \"stage8a4-i3-test-fixtures\")]\n    pub(crate) async fn p1e_test_cancel_published(",
    )
    for token in (
        "one_intent_pending_from_owner(",
        "publish_canonical_m10(&successor",
        "publish_exact_command()",
    ):
        require(token in continuous_helper, f"continuous V5 helper missing: {token}")
    for forbidden in (
        "FLUSHALL",
        "stage8b_p1d3_test_inject_one_intent",
        "p1d2_test_schedule_authority",
        "stage8b_p1d1_test_schedule_authority",
        "execute_next_canonical_market(",
        "commit_truth(&key)",
        "acknowledge_source()",
    ):
        require(
            forbidden not in continuous_helper,
            f"continuous V5 helper contains authority/effect fixture seam: {forbidden}",
        )
    continuous_test = section(
        process,
        "async fn production_v5_bootstrap_runs_continuous_market_lifecycle_and_readmits_exactly()",
        "async fn os_process_sigkill_then_restart_preserves_single_ready_owner()",
    )
    require_order(
        continuous_test,
        (
            "p1e_test_v5_plain_market_published_from_owner(",
            "p1e_test_open_schedule_envelope_for_last_eligible(",
            ".arg(strategy_runtime_core::STAGE8B_P1E_SCHEDULE_STREAM)",
            "Stage8bP1eRedisScheduleReader::test_connect_with_fixture_trust(",
            ".read_newest_guarded(&fixture.context",
            "Stage8bP1eNewestScheduleReadV1::Verified(snapshot)",
            "p1e_test_commit_plain_market_v4_only(",
            "Stage7bRestartOutcome::P1eScheduleBindingCommitted(committed)",
            "Stage8bP1eRecoveredMarketScheduleOutcomeV1::FeedbackAckCommitted",
            "resume_stage8b_p1e_committed_market_with_redis(",
            ".commit_truth(&key)",
            ".acknowledge_source()",
            "effects.schedule_read_total, 1",
            "Stage7bRestartOutcome::P1d2TruthCommitted(truth)",
            "Stage8bP1RedisZeroIntentAckDisposition::AlreadyAcknowledged",
            "restart_effects.schedule_read_total, 0",
        ),
        "continuous signed-schedule/V4 Market witness",
    )
    require(
        continuous_test.count("binding_reader.test_read_attempts(),") == 2,
        "fresh signed schedule must be read once and never reread after committed V4",
    )
    require(
        continuous_test.count(
            "stage8b_p1e_test_admit_ordinary_run_with_schedule_key_v1("
        )
        == 3,
        "V4, post-truth and final frontiers must pass the same ordinary-run admission",
    )
    for token in (
        "immutable_adoption_before",
        '"committed V4 recovery must not reread signed schedule"',
        '"continuous M10/Market advancement must not rewrite initial adoption authority"',
        '"the fresh V5 Market command must publish exactly once"',
    ):
        require(token in continuous_test, f"continuous restart invariant missing: {token}")
    for forbidden in (
        "FLUSHALL",
        "stage8b_p1d3_test_inject_one_intent",
        "p1d2_test_schedule_authority",
        "stage8b_p1d1_test_schedule_authority",
    ):
        require(
            forbidden not in continuous_test,
            f"continuous signed-schedule witness contains legacy seam: {forbidden}",
        )
    market_recovery = section(
        semantic,
        "pub async fn resume_stage8b_p1e_committed_market_with_redis(",
        "pub async fn resume_stage8b_p1e_committed_generated_market_with_redis(",
    )
    require_order(
        market_recovery,
        (
            "plain_market_restart_material()",
            "resume_stage8b_p1e_committed_schedule_binding",
            "continue_stage8b_p1e_market_schedule",
            ".reclaim_exact_binding(",
            ".revalidate_exact_command_publication(",
            ".exact_first_successor_m10(",
            "execute_next_canonical_market(authority, commitment_key)",
        ),
        "committed plain-Market V4 continuation",
    )
    for forbidden in (
        "Stage8bP1eRedisScheduleReader",
        "publish_exact_command",
        "p1d2_test_schedule_authority",
        "stage8b_p1d1_test_schedule_authority",
    ):
        require(
            forbidden not in market_recovery,
            f"committed Market continuation rereads or forges authority: {forbidden}",
        )
    isolated = section(
        semantic,
        "/// Isolated integration fixture for already-authenticated P1-d3/P1-d4",
        "#[cfg(feature = \"stage8a4-i3-test-fixtures\")]\n    pub(crate) async fn p1e_test_v5_plain_market_published_from_owner(",
    )
    require(
        "test-only intent" in isolated
        and "continuous fresh-V5 production-path witness" in isolated,
        "isolated injected Cancel helper overstates its evidence",
    )

    require(
        "if coordinator.shutdown_intent().is_none()" in finish
        and "Stage8bP1eSupervisorEventV1::OwnerReturnedWithoutOwner" in finish,
        "unexpected authenticated stop is not mapped to fatal owner return",
    )

    crash = section(
        process,
        "async fn os_process_sigkill_after_cancel_truth_recovers_xack_last_and_keeps_polling()",
        "fn fixed_config()",
    )
    for token in (
        "p1d3-after-s-cancel-recovered-before-source-xack",
        'marker["pid"], u64::from(first.id())',
        "pending_at_crash.ids[0].id, cancel_source_redis_id",
        "wait_for_process_exit(&mut first).signal(),\n            Some(libc::SIGKILL)",
        "SIGKILL after durable truth and before XACK must recover exact truth authority",
        "restart did not XACK the truth-covered Cancel source before successor acquisition",
        "cancel_execution_observed",
    ):
        require(token in crash, f"truth-before-XACK process witness missing: {token}")

    require('Stage8bP1eProcessSuccessV1::RunStopped => "run-stopped"' in binary,
            "binary run success mapping missing")
    for token in (
        "real child PIDs and kernel signals",
        "after replacement Cancel truth and before source XACK",
        "Authenticated restart returns exact `P1d3TruthCommitted`",
        "FINAM POST/DELETE/send",
        "not aggregate I1 acceptance or an",
        "authenticated V5 ordinary-run admission",
        "before admission exits 66",
        "Corrected V5 predecessor lifecycle",
        "Existing predicate-v1 `TimerReady` V5 artifacts",
        "server-processed Redis response",
        "fresh flat paper V5",
        "authenticated durable restart outcome",
    ):
        require(token in document, f"process documentation missing: {token}")
    require("composed `run`" in checkpoint, "checkpoint does not identify composed run")
    require("OS-process" in status and "retained command logs" in status,
            "current status does not identify active process evidence slice")


def main() -> None:
    validate_content(load_content())
    print("PASS stage8b-p1e-i1-process-supervision-source-check")


if __name__ == "__main__":
    main()
