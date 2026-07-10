# Stage 4H — paper/mock runtime-host bootstrap integration tests

Status: implemented for review.

Date: 2026-07-10.

## Goal

Stage 4H proves the first paper/mock runtime-host integration boundary after
Stage 4G lifecycle ordering acceptance.

This stage does not attach the real strategy runtime, does not enable
runtime-live, does not connect the real FINAM command consumer, and does not
add any order POST/DELETE path. It produces a deterministic mock-runtime event
trace only after accepted Stage 4G evidence.

## Integration contract

Stage 4H adds `evaluate_stage4_runtime_bootstrap_integration(...)`.

The integration decision accepts only when the source Stage 4G decision is
accepted and its final `runtime_bootstrap_notification_allowed` flag is true.
Because the source Stage 4G decision is a public serializable DTO, Stage 4H also
defensively checks its internal consistency before emitting any mock runtime
event.

Stage 4H rejects a Stage 4G lifecycle decision when:

- its schema version is not the expected Stage 4G schema;
- `blocker_count` does not match the blocker vector length;
- stored lifecycle issues do not match `validate_runtime_lifecycle_sequence`;
- an `Accepted` lifecycle contains blockers, lifecycle issues, non-ready source
  statuses, false ordering booleans, missing notification permission, or live
  authorization;
- a `Blocked` lifecycle still has final runtime bootstrap notification allowed;
- a `Blocked` lifecycle has no blockers.

When accepted, the mock runtime event trace is exactly:

```text
NotifyBootstrapSnapshot
  -> NotifyRuntimeStateRestored
  -> WarmupHistory
  -> RecoverPendingStreams
```

When blocked, the mock runtime event trace is empty. No partial bootstrap,
restore, warmup, or pending-recovery event is emitted.

## Added API

- `STAGE4_RUNTIME_BOOTSTRAP_INTEGRATION_SCHEMA_VERSION`.
- `Stage4RuntimeBootstrapIntegrationStatus`.
- `Stage4RuntimeBootstrapIntegrationEvent`.
- `Stage4RuntimeBootstrapIntegrationBlockerKind`.
- `Stage4RuntimeBootstrapIntegrationBlocker`.
- `Stage4RuntimeBootstrapIntegrationDecision`.
- `evaluate_stage4_runtime_bootstrap_integration(...)`.

## Acceptance gates covered

- Stage 4E application must be `Applied` before runtime bootstrap notification.
- Stage 4F dirty-start policy must be `Accepted` before runtime bootstrap
  notification.
- Stage 4G lifecycle ordering must be `Accepted` before runtime bootstrap
  notification.
- Mock runtime receives `NotifyBootstrapSnapshot` only after accepted Stage 4G.
- Mock runtime receives `NotifyRuntimeStateRestored` only after bootstrap
  snapshot.
- Warmup/history starts only after bootstrap notification.
- Pending stream recovery starts only after warmup.
- Blocked scenarios emit no mock runtime events.
- Tampered or internally inconsistent Stage 4G DTOs emit no mock runtime events.
- Live authorization attempts block the mock runtime notification path.

## Fixture-backed coverage

Unit tests cover:

- canonical accepted lifecycle emits the exact mock runtime event sequence;
- stale broker truth blocks all mock runtime events;
- unknown schedule blocks all mock runtime events;
- manual intervention blocks all mock runtime events;
- noncanonical dirty-start policy blocks all mock runtime events;
- invalid lifecycle order blocks all mock runtime events;
- live authorization attempt blocks all mock runtime events;
- an accepted lifecycle DTO with injected blockers is rejected;
- an accepted lifecycle DTO with non-ready source status is rejected.

## Safety boundary

Stage 4H keeps these disabled:

- continuous runtime-live;
- `command-consumer-to-real-FINAM`;
- strategy-runtime-to-real-FINAM order routing;
- FINAM `LiveReady`;
- real POST/DELETE order endpoints;
- Stop/SLTP/bracket/replace/multi-leg.

Stage 4H acceptance is not live approval. It only proves that paper/mock
runtime-host bootstrap notifications are gated by accepted Stage 4G evidence.

## Follow-up

After Stage 4H review acceptance, the next safe slice can add a redacted
operator-facing bootstrap evidence report generator or continue toward a
paper/mock runtime-host runbook. Runtime-live and real command consumption
remain explicitly out of scope until later accepted gates.
