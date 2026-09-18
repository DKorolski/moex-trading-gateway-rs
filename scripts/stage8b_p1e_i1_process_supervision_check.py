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
        "supervisor": "crates/runtime-durable-service/src/stage8b_p1_supervisor.rs",
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
    supervisor = content["supervisor"]
    binary = content["binary"]
    document = content["document"]
    checkpoint = content["checkpoint"]
    status = content["status"]

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

    run = section(process, "async fn execute_run(", "async fn supervise_stage8b_p1e_owner_task_v1(")
    owner = section(
        process,
        "async fn run_stage8b_p1e_process_owner_v1(",
        "async fn execute_run(",
    )
    require_order(
        run,
        (
            "SignalKind::terminate()",
            "SignalKind::interrupt()",
            "load_stage8b_p1_commitment_key_from_systemd_credential()",
            "restart_stage8b_p1(",
            "let mut session = attach_stage8b_p1e_verified_redis(&attach_plan)",
            ".clean_stale_zero_pending_consumers(&consumer_name)",
            "let startup = acquire_stage8b_p1e_startup_owner_v1(attachable, session)",
            "let reader = crate::Stage8bP1eRedisScheduleReader::connect(&redis_url)",
            "let owner = tokio::spawn(async move {",
            "supervise_stage8b_p1e_owner_task_v1",
        ),
        "run order",
    )
    for token in (
        "expected_registry_identity_sha256: supervisor",
        "expected_registry_version: supervisor.schedule_registry_version()",
        "Stage8bP1eSupervisorEventV1::ExternalSignal",
        "Stage8bP1eSupervisorEventV1::GraceExpired",
        "Stage8bP1eSupervisorEventV1::OwnerPanicked",
        "Stage8bP1eSupervisorEventV1::AuthenticatedBoundaryReached",
        "Stage8bP1eProcessSuccessV1::RunStopped",
    ):
        require(token in process, f"process invariant missing: {token}")
    for token in (
        "Stage8bP1eOwnerLoopOutcomeV1::CommittedCancelResolved(resolved)",
        "resolved.into_ready_polling()",
        "Stage8bP1eOwnerTaskBoundaryV1::RestartRequired",
    ):
        require(token in owner, f"owner handoff invariant missing: {token}")

    for error, code in (
        ("Self::RedisAttach | Self::ScheduleReader | Self::OwnerLoop", "67"),
        ("Self::OwnerTaskFailed", "70"),
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

    for test in (
        "os_process_idle_sigterm_exits_zero_at_authenticated_boundary",
        "os_process_sigkill_then_restart_preserves_single_ready_owner",
        "os_process_owner_panic_exits_exact_class_70",
        "os_process_consumes_committed_cancel_handoff_and_keeps_polling",
        "os_process_sigkill_after_cancel_truth_recovers_xack_last_and_keeps_polling",
    ):
        require(test in process, f"process matrix test missing: {test}")

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
        "not aggregate I1 acceptance",
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
