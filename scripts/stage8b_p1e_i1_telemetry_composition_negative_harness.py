#!/usr/bin/env python3
"""Mutation harness for the I1 production telemetry composition contract."""

from __future__ import annotations

import copy

import stage8b_p1e_i1_telemetry_composition_check as check


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
        ("publisher-connection", "supervisor", "pub struct Stage8bP1eTelemetryPublisherV1 {\n    connection: ConnectionManager", "pub struct Stage8bP1eTelemetryPublisherV1 {\n    removed_connection: ConnectionManager"),
        ("publisher-health", "supervisor", "impl Stage8bP1eTelemetryPublisherV1 {\n    pub async fn publish_health(", "impl Stage8bP1eTelemetryPublisherV1 {\n    async fn removed_publish_health("),
        ("publisher-readiness", "supervisor", "&self.readiness_stream, payload", "&self.health_stream, payload"),
        ("nomkstream", "supervisor", '.arg("NOMKSTREAM")', '.arg("MKSTREAM")'),
        ("exact-maxlen", "supervisor", '.arg("=")', '.arg("~")'),
        ("retention", "supervisor", ".arg(STAGE8B_P1E_TELEMETRY_RETENTION)", ".arg(999999usize)"),
        ("payload-field", "supervisor", '.arg("payload")', '.arg("body")'),
        ("boot-id-retention", "supervisor", "boot_id: self.boot_id", "boot_id: [0_u8; 16]"),
        ("publisher-export", "lib", "Stage8bP1eTelemetryPublisherV1", "RemovedTelemetryPublisherV1"),
        ("restart-seal", "recovery", "pub(crate) fn stage8b_p1e_telemetry_seal_v1(", "fn removed_telemetry_seal("),
        ("blocked-seal", "recovery", "Self::Blocked(_) => Err(Stage7bRecoveryError::SealInvalid)", "Self::Blocked(owner) => Ok(&owner.seal)"),
        ("durable-readiness", "semantic", "self.stage7.validate_composite_readiness(commitment_key)", "true"),
        ("bounded-channel", "process", "tokio::sync::mpsc::channel(32)", "tokio::sync::mpsc::unbounded_channel()"),
        ("nonblocking-report", "process", "self.sender.try_send(snapshot)", "self.sender.blocking_send(snapshot)"),
        ("reporter-failure", "process", "self.request_telemetry_failure();", "return;"),
        ("deduplicate", "process", "if *state == previous", "if false"),
        ("periodic-anchor", "process", "interval_at(tokio::time::Instant::now() + period, period)", "interval(period)"),
        ("missed-tick", "process", "MissedTickBehavior::Skip", "MissedTickBehavior::Burst"),
        ("terminal-stop", "process", "if state.terminal", "if false"),
        ("publisher-derivation", "process", ".telemetry_publisher()", ".clone()"),
        ("s06-before-telemetry", "process", "let initial_pel_count = startup", "let removed_initial_pel_count = startup"),
        ("real-boot-id", "process", "boot_id: settings.boot_id", "boot_id: [0_u8; 16]"),
        ("paper-only", "process", "paper_only: true", "paper_only: false"),
        ("finam-closed", "process", "finam_transport_attached: false", "finam_transport_attached: true"),
        ("broker-closed", "process", "broker_network_dispatch_attached: false", "broker_network_dispatch_attached: true"),
        ("runtime-live-closed", "process", "runtime_live: false", "runtime_live: true"),
        ("real-orders-closed", "process", "real_orders: false", "real_orders: true"),
        ("ready-update", "process", "telemetry.update_ready(", "telemetry.removed_ready("),
        ("degraded-update", "process", "telemetry.update_lifecycle_pending(", "telemetry.removed_pending("),
        ("draining-update", "process", "telemetry.update_draining()", "telemetry.removed_draining()"),
        ("telemetry-exit", "process", "Stage8bP1eProcessErrorV1::TelemetryFailed.exit_code(), 71", "Stage8bP1eProcessErrorV1::TelemetryFailed.exit_code(), 70"),
        ("terminal-71", "process", "Some(71) => Err(Stage8bP1eProcessErrorV1::TelemetryFailed)", "Some(71) => Err(Stage8bP1eProcessErrorV1::OwnerTaskFailed)"),
        ("transition-test", "process", "production_telemetry_publishes_all_transitions_in_order", "removed_transition_test"),
        ("missing-stream-test", "process", "production_telemetry_missing_stream_fails_closed_without_creation", "removed_missing_stream_test"),
        ("periodic-test", "process", "production_telemetry_periodically_republishes_current_snapshot", "removed_periodic_test"),
        ("backpressure-test", "process", "telemetry_reporter_backpressure_is_fail_closed_and_first_wins", "removed_backpressure_test"),
        ("settlement-precedence-test", "process", "production_telemetry_settlement_preserves_first_wins_failure_precedence", "removed_settlement_precedence_test"),
        ("production-process-test", "process", "production_process_publishes_ready_drain_and_stop_telemetry", "removed_production_process_test"),
        ("active-supervisor-latch", "process", "() = wait_for_stage8b_p1e_shutdown_v1(shutdown_latch.as_ref())", "() = std::future::pending::<()>()"),
        ("telemetry-task-observed", "process", "result = telemetry_task.task_mut()", "() = std::future::pending::<()>()"),
        ("telemetry-early-failure", "process", "telemetry_reporter.request_telemetry_failure()", "telemetry_reporter.update_draining()"),
        ("telemetry-task-guard", "process", "Stage8bP1eTelemetryTaskGuardV1::new(telemetry_task)", "removed_telemetry_task_guard(telemetry_task)"),
        ("telemetry-task-abort", "process", "task.abort()", "drop(task)"),
        ("retained-final-deadline", "process", ".map(Stage8bP1eShutdownIntentV1::grace_deadline_utc_ms)", ".map(|_| i64::MAX)"),
        ("retained-final-remaining", "process", "deadline_utc_ms.saturating_sub(now_utc_ms)", "shutdown_grace_ms as i64"),
        ("bounded-exit-class", "process", "intent.bounded_exit_class(now_utc_ms)", "71"),
        ("source-observed-time", "process", "source_poll_observed_at_utc_ms: Option<i64>", "observed_marker: Option<i64>"),
        ("source-fresh-until", "process", "source_poll_fresh_until_utc_ms: Option<i64>", "freshness_marker: Option<i64>"),
        ("source-freshness-window", "process", "health_interval_ms.saturating_mul(2).max(3_000)", "u64::MAX"),
        ("ready-latch-reconciliation", "process", "let shutdown_requested = self.latch.intent().is_some()", "let shutdown_requested = false"),
        ("publisher-live-reconciliation", "process", "state.reconcile_live_process_state(latch.as_ref(), Utc::now().timestamp_millis())", "state.source_poll_fresh = true"),
        ("runtime-semantic-timestamp", "hybrid_runtime", "stage8b_p1e_last_semantic_bar_ts_utc", "removed_last_semantic_bar_ts_utc"),
        ("restart-semantic-timestamp", "clean_restart", "stage8b_p1e_last_semantic_bar_ts_utc", "removed_last_semantic_bar_ts_utc"),
        ("order-ack-timestamp", "order_position", "stage8b_p1e_last_canonical_ack_ts_utc", "removed_last_canonical_ack_ts_utc"),
        ("runtime-audit", "live_core", "stage8b_p1e_telemetry_runtime_audit_v1", "removed_telemetry_runtime_audit_v1"),
        ("durable-snapshot", "recovery", "pub(crate) struct Stage8bP1eTelemetryDurableSnapshotV1", "struct RemovedTelemetryDurableSnapshotV1"),
        ("semantic-snapshot-bridge", "semantic", "pub(crate) fn stage8b_p1e_telemetry_snapshot_v1", "fn removed_telemetry_snapshot_v1"),
        ("freshness-expiry-test", "process", "production_telemetry_expires_source_poll_while_redis_writer_remains_live", "removed_freshness_expiry_test"),
        ("queued-ready-test", "process", "production_telemetry_queued_ready_cannot_override_retained_shutdown", "removed_queued_ready_test"),
        ("intent-supervision-test", "process", "process_supervisor_observes_telemetry_intent_without_os_signal", "removed_intent_supervision_test"),
        ("deadline-supervision-test", "process", "process_supervisor_uses_retained_telemetry_deadline_without_extension", "removed_deadline_supervision_test"),
        ("panic-overflow-process-test", "process", "production_process_telemetry_panic_and_fifo_overflow_exit_71_without_signal", "removed_panic_overflow_process_test"),
        ("redis-error-process-test", "process", "production_process_telemetry_redis_error_exits_71_without_signal", "removed_redis_error_process_test"),
        ("inflight-heartbeat-test", "process", "production_heartbeat_turns_draining_while_ready_poll_response_is_withheld", "removed_inflight_heartbeat_test"),
        ("late-sigterm-test", "process", "late_sigterm_does_not_extend_retained_telemetry_deadline", "removed_late_sigterm_test"),
        ("durable-chain-test", "process", "production_telemetry_payload_tracks_real_signed_market_ack_truth_and_xack", "removed_durable_chain_test"),
        ("retained-snapshot-test", "process", "retained_signed_market_ack_exposes_exact_terminal_snapshot_and_pel", "removed_retained_snapshot_test"),
        ("blocked-inventory-test", "process", "production_telemetry_publishes_typed_blocked_inventory", "removed_blocked_inventory_test"),
        ("document-status", "document", "SOURCE CORRECTION REVIEW CANDIDATE — I1 NOT CLOSED", "ACCEPTED — I1 CLOSED"),
        ("document-next", "document", "fixed-path installation and systemd material", "operational activation"),
        ("status-not-accepted", "status", "source review candidate, not acceptance", "accepted source closure"),
        ("status-correction-not-accepted", "status", "source correction review candidate, not acceptance", "accepted correction closure"),
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
    print(f"PASS stage8b-p1e-i1-telemetry-composition-negative-harness {passed}/{len(cases)}")


if __name__ == "__main__":
    main()
