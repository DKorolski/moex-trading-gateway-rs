# Stage 4C — validated broker-truth bootstrap wrapper

Status: implemented for review.

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
- target non-flat without adoption requires manual intervention and cannot
  become clean-flat;
- target active order cannot silently disappear;
- zero-quantity position rows are diagnostic, not open position truth;
- restored runtime state remains state and does not override broker truth;
- unknown/orphan target trade blocks readiness;
- external issue bridge maps target issue to blocker;
- raw payload/live boundary produces `SafetyBoundaryOpen`;
- unknown schedule produces `UnknownSchedule`;
- existing order lifecycle is reused, not duplicated.

## Next slice

After Stage 4C review, the next logical slice remains Stage 4D:
FINAM read-only broker-truth mapper and fixture-backed source normalization into
the Stage 4 validated bootstrap wrapper.
