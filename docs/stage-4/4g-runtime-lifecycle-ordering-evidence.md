# Stage 4G — runtime lifecycle ordering evidence

Status: implemented for review.

Date: 2026-07-10.

## Goal

Stage 4G proves the broker-truth/runtime bootstrap ordering before any later
runtime attachment can treat the bootstrap sequence as acceptable.

This stage is still evidence/paper/mock only. It does not enable runtime-live,
does not connect the real FINAM command consumer, does not allow strategy-driven
real orders, and does not add POST/DELETE order paths.

## Ordering contract

The accepted lifecycle remains ALOR-compatible:

```text
LoadBrokerTruthSnapshot
  -> LoadRuntimeState
  -> NotifyBootstrapSnapshot
  -> NotifyRuntimeStateRestored
  -> WarmupHistory
  -> RecoverPendingStreams
```

Stage 4G adds `evaluate_stage4_runtime_lifecycle_ordering(...)`, which accepts
only when all of these are true:

- broker truth is loaded before runtime state is trusted;
- Stage 4E runtime bootstrap application evidence is canonical and `Applied`;
- Stage 4F dirty-start policy is canonical and `Accepted`;
- `NotifyBootstrapSnapshot` is allowed only after accepted broker-truth
  application/policy evidence;
- `NotifyRuntimeStateRestored` cannot occur before bootstrap notification;
- restored runtime state cannot overwrite broker truth;
- warmup/history recovery cannot occur before accepted bootstrap notification;
- pending stream recovery remains after warmup;
- no live authorization is present in the application, policy, or lifecycle
  plan.

## Added API

- `STAGE4_RUNTIME_LIFECYCLE_ORDERING_SCHEMA_VERSION`.
- `Stage4RuntimeLifecycleOrderingStatus`.
- `Stage4RuntimeLifecycleOrderingBlockerKind`.
- `Stage4RuntimeLifecycleOrderingBlocker`.
- `Stage4RuntimeLifecycleOrderingDecision`.
- `evaluate_stage4_runtime_lifecycle_ordering(...)`.

The implementation reuses the existing broker-neutral runtime-host lifecycle
types:

- `RuntimeHostLifecyclePlan`;
- `RuntimeHostLifecycleStep`;
- `RuntimeHostLifecycleIssue`;
- `validate_runtime_lifecycle_sequence(...)`.

## Blocking cases

Stage 4G blocks the runtime lifecycle when:

- the lifecycle plan is missing, duplicated, or reorders required steps;
- Stage 4E application evidence is missing, blocked, or tampered;
- Stage 4F policy is missing, blocked, or tampered;
- bootstrap notification is attempted before broker-truth application evidence;
- runtime-state restored notification precedes bootstrap notification;
- restored runtime state attempts to overwrite broker truth;
- warmup precedes bootstrap notification;
- pending stream recovery precedes warmup;
- warmup/live order authorization is attempted.

## Fixture-backed coverage

Unit tests cover:

- canonical Stage 4E + Stage 4F + ALOR-compatible lifecycle accepted;
- bootstrap notification before broker-truth application blocked;
- non-applied Stage 4E application blocked;
- non-canonical or blocked Stage 4F policy blocked;
- runtime-state restore before bootstrap notification blocked;
- restored runtime-state overwrite of broker truth blocked;
- warmup before bootstrap notification blocked;
- pending recovery before warmup blocked;
- live authorization attempt blocked.

## Safety boundary

Stage 4G keeps these disabled:

- continuous runtime-live;
- `command-consumer-to-real-FINAM`;
- strategy-runtime-to-real-FINAM order routing;
- FINAM `LiveReady`;
- real POST/DELETE order endpoints;
- Stop/SLTP/bracket/replace/multi-leg.

`Stage4RuntimeLifecycleOrderingDecision.no_live_authorization` must remain
`true` for acceptance.

## Follow-up

After Stage 4G review acceptance, the next safe slice can move toward
operator-facing runtime lifecycle runbook/evidence packaging or a further
paper/mock runtime-host bootstrap integration gate. Live runtime attachment and
real command consumption remain explicitly out of scope until later accepted
gates.
