# Stage 5B-2 — integrated wrapper semantic inventory

Status: design/inventory review candidate; wrapper implementation remains
blocked.

Date: 2026-07-11.

## Purpose

This document defines the required semantic inventory for migrating the exact
6,203-line `HybridIntradayRuntimeStrategy` source oracle. It does not compile,
copy into a Cargo target, or invoke the wrapper.

Source oracle:

```text
source-oracles/alor-stage5/hybrid_intraday_runtime.rs
sha256=6e15ab1b7212c56d3ecd8397b2d8991c1feccbde8eaa5e3d0051aec82a55f0aa
```

Stage 5B-2 implementation must receive a separate correspondence manifest. It
must not modify the accepted Stage 5B-1 manifest or bypass the structural
source-set lock.

## Required wrapper domains

The future wrapper migration must account for all of these coupled domains:

- broker-neutral callback surface;
- bootstrap/adoption and runtime-state restore;
- event-time, warmup, session and startup-replay behavior;
- BO/high180 MR override and arbitration;
- exact request-id, pending and deferred lifecycle;
- order, stop-order, position and timer feedback;
- partial-entry distinction between BO and MR bracket ownership;
- protective state, cleanup and residual repair semantics;
- riskgate state/session finalization;
- safe-mode close-only and operator intervention;
- state synchronization and source-compatible restart behavior.

No domain may be omitted merely because real Stop/SLTP/bracket execution remains
disabled. Protective behavior is still required in the paper semantic model.

## Bracket terminal-reconciliation hardening

The selected `6e15...` oracle includes behavior absent from its `970418...`
parent. It is a mandatory Stage 5B-2 semantic contract.

### State and timeout

```text
field: bracket_terminal_reconcile_started_ms: Option<i64>
grace: BRACKET_TERMINAL_RECONCILE_GRACE_MS = 3000
```

The marker is transient process state. It is initialized to `None`, is not
written by `sync_state`, and is not restored by `set_state`.

### Lifecycle transitions

```text
TP filled or SL terminal
  -> start terminal-reconcile grace

position size changes while residual position remains inside grace
  -> update last_position_qty
  -> suppress immediate emergency flatten
  -> retain protective reconciliation state

timer after grace while residual position remains
  -> clear reconcile marker
  -> emit one residual emergency flatten
  -> reason = bracket_terminal_reconcile_timeout
  -> enter safe-mode close-only through existing residual-exit path

flat transition
  -> clear reconcile marker

repeated timer after timeout
  -> no duplicate exit
```

### Source-compatible restart semantics

The marker is deliberately not persisted in the source oracle. Therefore a
restart must not reconstruct or extend a pre-restart three-second grace window.
The migrated runtime starts with the marker absent and relies on fresh broker
truth plus normal bootstrap/adoption/repair policy.

Persisting this marker, recreating its remaining duration, or automatically
granting a fresh grace interval after restart would be a semantic change and
requires a separately reviewed compatibility decision.

### Required fixtures

The source tests are normative:

- `partial_protective_fill_waits_for_terminal_reconcile`;
- `terminal_reconcile_timeout_emits_single_residual_flatten`.

The redacted inventory fixture is:

```text
tests/fixtures/stage5/bracket_terminal_reconciliation.json
```

Static freeze tests prove the lifecycle markers and non-persistence policy are
present in the exact source oracle. Executable migrated-wrapper tests remain a
Stage 5B-2 implementation requirement.

## Stage 5B-2 implementation acceptance matrix

Before implementation can be accepted, the migrated wrapper must prove:

- exact source correspondence against `6e15...`;
- callback-complete host mapping;
- typed `BrokerOrderId(String)` and exact `StrategyRequestId` handling;
- field-complete state restore/round trip;
- active high180 override rather than classic MR entry formula;
- bracket terminal-reconciliation lifecycle above;
- source-compatible transient-marker restart behavior;
- deterministic paper/no-send lifecycle;
- no FINAM DTO, Redis client, HTTP or real endpoint dependency in the semantic
  crate;
- all runtime-live and real execution flags remain false.

## Next gate

Stage 5B-2 implementation remains blocked until Stage 5B-1 structural freeze
and this inventory are accepted. The first implementation slice should define
the wrapper correspondence manifest and broker-neutral callback/state mapping,
not copy the whole wrapper into an untracked module.
