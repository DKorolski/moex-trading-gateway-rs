# Stage 6C — deterministic crash/replay state machine

Stage 6C is the direct successor of accepted Stage 6B-R1 commit
`f0d5e3912243ba85c6f372722c97e815f254a962`. It adds a pure replay projection
over canonical Stage 6A records already admitted by Stage 6B storage. It does
not read journal frames, perform I/O, call a broker, mutate Stage 5 runtime
state, or authorize dispatch.

## Durable lifecycle

The accepted `RequestAccepted` record is the only first record and has
sequence 1 with no previous record. Each later unique record advances the
per-request sequence by one and names the exact previous record. A causal
parent, when present, must have appeared earlier in physical history.

The dispatch safety projection is:

```text
RequestAccepted
  -> ReadyForFirstDispatch

DispatchAttemptRecorded
  -> ReconciliationRequired

ReconciliationObserved::NoBrokerOrderFound
  -> RetryEligibleSameIdentity

BrokerOrderObserved / BrokerTradeObserved /
ReconciliationObserved::BrokerOrderFound
  -> DispatchForbidden
```

Only authoritative `NoBrokerOrderFound` permits the next dispatch-attempt
ordinal. A second attempt while reconciliation is unresolved is rejected as
blind redispatch. Stage 6C never performs the dispatch itself.

## Replay authority

Replay is grouped by exact `StrategyRequestId`. Durable client order identity,
account, instrument, strategy attribution, owner, cycle, role, action and
cancel targets cannot drift. Broker order and trade IDs remain opaque strings.

An exact repeated journal record ID with byte-identical canonical bytes is
idempotent. The same ID with different canonical bytes fails closed. New events
after explicit `RequestFinalized` fail closed; exact duplicates remain
idempotent. No lifecycle state is inferred from OHLC or paper assumptions.

Canonical output uses ordered collections and is hashed in domain
`stage6-replay-snapshot-v1`. Interleaving independent requests does not alter
the semantic fingerprint when each request's own event order is preserved.

## Compatibility and closed surfaces

The Stage 6A schema remains version 1 and is extended only with typed payloads
for its previously reserved Stage 6C event names. Existing Place/Cancel bytes,
the Stage 6A golden manifest, Stage 6B frames/checkpoints and the Stage 6B
backend source are pinned by `stage6c-storage-compatibility-manifest.json`.

Redis, FINAM transport, HTTP POST/DELETE, broker dispatch, runtime callbacks,
runtime-live, workers, schedulers, real orders and native protective orders are
closed. Stage 6D remains closed pending independent Stage 6C acceptance.
