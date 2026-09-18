#!/usr/bin/env python3
"""Mutation harness for the I1 OS-process supervision source contract."""

from __future__ import annotations

import copy

import stage8b_p1e_i1_process_supervision_check as check


def replace(content: dict[str, str], key: str, old: str, new: str) -> dict[str, str]:
    mutated = copy.deepcopy(content)
    if old not in mutated[key]:
        raise RuntimeError(f"mutation anchor missing: {key}: {old}")
    mutated[key] = mutated[key].replace(old, new, 1)
    return mutated


def main() -> None:
    base = check.load_content()
    check.validate_content(base)
    print("PASS positive-baseline")
    cases = [
        ("run-route", "process", "Stage8bP1eProcessCommandV1::Run => execute_run", "Stage8bP1eProcessCommandV1::Run => removed_execute_run"),
        ("sigterm", "process", "SignalKind::terminate()", "removed_terminate()"),
        ("sigint", "process", "SignalKind::interrupt()", "removed_interrupt()"),
        ("credential", "process", "load_stage8b_p1_commitment_key_from_systemd_credential()", "removed_credential()"),
        ("durable-restart", "process", "restart_stage8b_p1(", "removed_restart("),
        ("redis-attach", "process", "let mut session = attach_stage8b_p1e_verified_redis(&attach_plan)", "let mut session = removed_verified_redis(&attach_plan)"),
        ("stale-cleanup", "process", ".clean_stale_zero_pending_consumers(&consumer_name)", ".removed_stale_cleanup(&consumer_name)"),
        ("startup-owner", "process", "let startup = acquire_stage8b_p1e_startup_owner_v1(attachable, session)", "let startup = removed_startup_owner(attachable, session)"),
        ("schedule-reader", "process", "let reader = crate::Stage8bP1eRedisScheduleReader::connect(&redis_url)", "let reader = crate::RemovedScheduleReader::connect(&redis_url)"),
        ("owner-spawn", "process", "let owner = tokio::spawn(async move {", "let owner = removed_spawn(async move {"),
        ("cancel-handoff", "process", "resolved.into_ready_polling()", "removed_into_ready_polling()"),
        ("restart-boundary", "process", "Stage8bP1eOwnerTaskBoundaryV1::RestartRequired", "RemovedRestartRequired"),
        ("external-signal", "process", "Stage8bP1eSupervisorEventV1::ExternalSignal", "RemovedExternalSignal"),
        ("grace", "process", "Stage8bP1eSupervisorEventV1::GraceExpired", "RemovedGraceExpired"),
        ("panic-event", "process", "Stage8bP1eSupervisorEventV1::OwnerPanicked", "RemovedOwnerPanicked"),
        ("authenticated-stop", "process", "Stage8bP1eSupervisorEventV1::AuthenticatedBoundaryReached", "RemovedAuthenticatedBoundary"),
        ("exit-test", "process", "process_errors_keep_the_accepted_stable_exit_classes", "removed_exit_test"),
        ("registry-version", "supervisor", "pub schedule_registry_version: String", "pub removed_registry_version: String"),
        ("registry-identity", "supervisor", "pub schedule_registry_identity_sha256: String", "pub removed_registry_identity: String"),
        ("registry-validation", "supervisor", "valid_registry_version(&config.schedule_registry_version)", "true"),
        ("shared-latch", "supervisor", "pub fn shutdown_latch(&self) -> Arc<Stage8bP1eShutdownLatchV1>", "fn removed_shutdown_latch(&self)"),
        ("idle-sigterm", "process", "os_process_idle_sigterm_exits_zero_at_authenticated_boundary", "removed_idle_sigterm"),
        ("idle-sigkill", "process", "os_process_sigkill_then_restart_preserves_single_ready_owner", "removed_idle_sigkill"),
        ("panic-test", "process", "os_process_owner_panic_exits_exact_class_70", "removed_panic_test"),
        ("handoff-test", "process", "os_process_consumes_committed_cancel_handoff_and_keeps_polling", "removed_handoff_test"),
        ("truth-crash-test", "process", "os_process_sigkill_after_cancel_truth_recovers_xack_last_and_keeps_polling", "removed_truth_crash_test"),
        ("truth-crash-phase", "process", "p1d3-after-s-cancel-recovered-before-source-xack", "removed-crash-phase"),
        ("truth-crash-pid", "process", 'marker["pid"], u64::from(first.id())', 'marker["pid"], 0'),
        ("truth-crash-pel", "process", "pending_at_crash.ids[0].id, cancel_source_redis_id", "pending_at_crash.ids[0].id, String::new()"),
        ("truth-crash-signal", "process", "wait_for_process_exit(&mut first).signal(),\n            Some(libc::SIGKILL)", "wait_for_process_exit(&mut first).signal(),\n            Some(libc::SIGTERM)"),
        ("truth-restart", "process", "SIGKILL after durable truth and before XACK must recover exact truth authority", "SIGKILL may recover any restart authority"),
        ("binary-status", "binary", 'Stage8bP1eProcessSuccessV1::RunStopped => "run-stopped"', 'Stage8bP1eProcessSuccessV1::RunStopped => "ok"'),
        ("closed-finam", "document", "FINAM POST/DELETE/send", "FINAM send open"),
        ("aggregate-boundary", "document", "not aggregate I1 acceptance", "aggregate I1 accepted"),
    ]
    passed = 0
    for name, key, old, new in cases:
        mutated = replace(base, key, old, new)
        try:
            check.validate_content(mutated)
        except check.CheckFailure:
            passed += 1
            print(f"PASS {name}")
            continue
        raise SystemExit(f"FAIL mutation accepted: {name}")
    print(f"PASS stage8b-p1e-i1-process-supervision-negative-harness {passed}/{len(cases)}")


if __name__ == "__main__":
    main()
