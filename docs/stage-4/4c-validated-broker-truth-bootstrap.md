# Stage 4C — validated broker-truth bootstrap wrapper

Status: implemented for review after P1 hardening.

Date: 2026-07-10.

## Purpose

Stage 4C adds the first validated bootstrap boundary around the existing
broker-neutral broker-truth/runtime-host types:

```text
BrokerTruthSnapshot
  -> RuntimeHostBootstrapSnapshot::from_broker_truth(...)
  -> ValidatedStage4BrokerTruthBootstrap
```

It follows the accepted Stage 4B decision: do not create a second incompatible
broker-truth domain. The new layer is a wrapper/validation/evidence boundary
around existing canonical types.

## Implemented core surface

Location: `crates/broker-core/src/stage4_bootstrap.rs`.

Public entrypoint:

```rust
validate_stage4_broker_truth_bootstrap(input) -> ValidatedStage4BrokerTruthBootstrap
```

Main types:

- `ValidatedStage4BrokerTruthBootstrap`;
- `Stage4BrokerTruthBootstrapStatus`;
- `Stage4BrokerTruthReadinessBlocker`;
- `Stage4BrokerTruthFreshness`;
- `Stage4BrokerTruthFreshnessSection`;
- `Stage4BrokerTruthOwnershipSummary`;
- `Stage4BrokerTruthTradeCorrelationSummary`;
- `Stage4DirtyStartDisposition`;
- `Stage4AdoptionDisposition`;
- `Stage4ManualInterventionReason`;
- `Stage4BrokerTruthExternalIssue`.

`Stage4BrokerTruthExternalIssue` is intentionally broker-neutral. It mirrors the
M3f/M3g issue meanings without making `broker-core` depend on `finam-gateway`.
The concrete FINAM gateway bridge can map M3f/M3g issues into this enum in a
later slice.

## Status values

The wrapper can produce:

- `BootstrapReady`;
- `BootstrapBlocked`;
- `ManualInterventionRequired`;
- `BrokerTruthIncomplete`;
- `BrokerTruthStale`;
- `InstrumentMismatch`;
- `UnknownSchedule`;
- `EvidenceIncomplete`;
- `SafetyBoundaryOpen`.

`BootstrapReady` is possible only when the target instrument is flat/clean or
explicitly adopted, required freshness sections are fresh, schedule is known,
instrument identity is unambiguous, no target unknown/orphan rows are present,
and the safety boundary is closed.

## Validation covered

Stage 4C validates and reports:

- target-vs-account-wide broker-truth summary;
- `RuntimeHostBootstrapSnapshot` candidate created through the existing
  `from_broker_truth` path;
- per-section freshness for positions, orders, trades, cash, instruments, and
  schedule;
- zero-quantity position-row diagnostics;
- target active order ownership/adoption/unknown-orphan counts;
- target trade correlation counts;
- dirty-start disposition;
- adoption evidence consistency;
- restored `RuntimeBootstrapSnapshotDto` as runtime state, not broker truth;
- broker-neutral external issue bridge into readiness blockers;
- raw-payload/live-order safety boundary.

## Review hardening

The follow-up hardening closes the Stage 4C review findings before Stage 4D:

- `BrokerTruthIncomplete` is now reachable through
  `Stage4BrokerTruthSourceStatus`.
- `Stage4BrokerTruthFreshnessInput::from_broker_truth_received_ts(...)` no
  longer fabricates schedule freshness from `BrokerTruthSnapshot.received_ts`;
  schedule freshness is `Unknown` unless explicit schedule evidence is passed.
- A separate `synthetic_all_sections_fresh_for_tests(...)` helper is used by
  tests that intentionally need all sections fresh.
- Valid position/order adoption can produce `BootstrapReady` with explicit
  adoption disposition rather than contradictory manual-intervention wording.
- Order adoption count is validated strictly against target active-order truth.
- Target trades without proven runtime-owned broker-order correlation become
  unknown/orphan blockers.
- Historical `known_order_ids` are diagnostic unless they are also restored
  working orders.
- Offset non-zero target position rows that net to flat are treated as
  ambiguous broker truth and require manual intervention.
- Terminal orphan orders are not treated as active bootstrap blockers.

## Safety boundary

Stage 4C still forbids:

- runtime-live;
- real FINAM command consumer;
- strategy-driven real FINAM orders;
- real FINAM `POST`/`DELETE` from runtime;
- Stop/SLTP/bracket/replace/multi-leg live behavior;
- RI/RTS migration;
- USDRUBF migration;
- `i64` surrogate adapter without a new ADR.

Any live/order flag or raw payload export attempt produces
`SafetyBoundaryOpen`.

## Test coverage

Added `broker-core` unit tests for:

- clean flat bootstrap is ready without live authorization;
- required stale freshness section blocks as `BrokerTruthStale`;
- missing broker-truth source returns `BrokerTruthIncomplete`;
- missing schedule freshness blocks even when broker truth receive timestamp is
  fresh;
- target non-flat without adoption requires manual intervention and cannot
  become clean-flat;
- valid position adoption can be `BootstrapReady`;
- target active order cannot silently disappear;
- valid order adoption can be `BootstrapReady`;
- invalid order adoption count produces `EvidenceIncomplete`;
- zero-quantity position rows are diagnostic, not open position truth;
- offset non-zero target position rows require manual intervention;
- restored runtime state remains state and does not override broker truth;
- historical known order ids absent from current broker truth are diagnostic;
- unknown/orphan target trade blocks readiness;
- target trade with an unowned broker order id blocks until reconciled;
- external issue bridge maps target issue to blocker;
- raw payload/live boundary produces `SafetyBoundaryOpen`;
- unknown schedule produces `UnknownSchedule`;
- missing/ambiguous instrument identity produces `InstrumentMismatch`;
- unknown target order status blocks bootstrap;
- existing order lifecycle is reused, not duplicated.

## Next slice

After Stage 4C review, the next logical slice remains Stage 4D:
FINAM read-only broker-truth mapper and fixture-backed source normalization into
the Stage 4 validated bootstrap wrapper.

Stage 4D must still close mapper/source gaps, especially zero-quantity position
diagnostics, schedule/session source propagation, per-section freshness
timestamps, source availability/decode failure, and fixture-backed unknown /
orphan order-trade cases.
