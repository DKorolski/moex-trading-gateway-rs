# M4-3r-b broker-neutral paper domain model

Status: source-only implementation / no-send / no live orders.

M4-3r-b introduces the broker-neutral paper/runtime domain model in
`broker-core`. This is the code-level counterpart of the M4-3r-a ALOR oracle
extraction. The model is intentionally not FINAM-specific and not ALOR-specific:
both broker adapters and the future paper runtime executor must use these
contracts when comparing paper runtime truth with the ALOR live oracle.

## Boundary

This stage does not implement execution and does not place orders.

Still disabled:

- FINAM `POST /orders`;
- FINAM `DELETE /orders/{id}`;
- command-consumer-to-real-FINAM;
- runtime `LiveReady`;
- continuous runtime-live attachment;
- Stop/SLTP/bracket/replace/multi-leg;
- deterministic paper fill executor.

## Added core module

The new module is:

```text
crates/broker-core/src/paper.rs
```

It is exported from:

```text
crates/broker-core/src/lib.rs
```

The model deliberately lives in `broker-core` because it is a broker-neutral
contract, not a FINAM gateway implementation detail.

## Domain objects

M4-3r-b adds:

```text
RuntimeBarInput
RuntimeBarOrigin
RuntimeDecisionId
RuntimeDecisionRecord
RuntimeSuppressionRecord
RuntimeSuppressionReason

PaperExecutionMode
PaperFillPolicy
PaperIntent
PaperIntentKind
PaperOrder
PaperOrderId
PaperOrderStatus
PaperTrade
PaperTradeId
PaperPosition
PaperAck
PaperAckKind
PaperLedgerSnapshot
PaperRuntimeState
PaperSafetyBoundary

RiskGatePaperLedgerRecord
RiskGatePaperState
```

These are pure contracts. They do not call Redis, FINAM, ALOR, HTTP, WebSocket,
or any order endpoint.

## ALOR oracle semantics captured

### Paper execution mode

The model keeps the ALOR distinction:

```text
LiveOnly   -> only RuntimeBarOrigin::Live may advance paper execution
HistorySim -> History / HistoryGap / Live / Replay may advance simulation
```

This protects normal paper-shadow operation from emitting paper actions during
history warmup or gap recovery.

### Runtime bar decision gate

`RuntimeBarInput::is_live_final_timeframe(expected)` encodes the strategy-facing
bar gate:

```text
origin == Live
is_final == true
timeframe_sec == expected
```

For IMOEXF hybrid paper-shadow the expected runtime timeframe remains 600
seconds.

### Paper safety boundary

`PaperSafetyBoundary::closed()` is the only accepted default boundary:

```text
live_orders_enabled                    = false
runtime_live_ready_enabled             = false
command_consumer_to_real_finam_enabled = false
external_order_endpoint_enabled        = false
stop_sltp_bracket_enabled              = false
```

`PaperLedgerSnapshot::validate()` rejects any open safety boundary.

### Paper order lifecycle

`PaperOrderStatus` classifies active vs terminal paper orders:

```text
active   = Pending / Working / PartiallyFilled
terminal = Filled / Canceled / Rejected / Expired / ManualReview
```

This mirrors the ALOR operational need to separate working lifecycle truth from
terminal history.

### Paper ledger invariants

`PaperLedgerSnapshot::validate()` checks:

- safety boundary is closed;
- intent ids are unique;
- paper order ids are unique;
- paper trade ids are unique;
- orders reference existing intents;
- trades reference existing orders and intents;
- acks reference existing intents and optional orders;
- strategy id matches the snapshot;
- instrument id matches the snapshot;
- `remaining_qty == qty - filled_qty`;
- filled quantity does not exceed order quantity.

This does not yet compute fills. It makes future fill computation auditable.

### Paper position truth

`PaperLedgerSnapshot::target_position_qty()` and
`PaperLedgerSnapshot::target_is_flat()` are target-instrument scoped. Account-wide
rows are not used as a proxy for target flatness.

This directly addresses the earlier M4 live-test semantic gap: multiple
instruments may exist on one account, so flatness must be instrument-scoped.

### Risk-gate paper state

`RiskGatePaperLedgerRecord` and `RiskGatePaperState` preserve the ALOR split:

```text
long memory -> risk-gate ledger records
derived current state -> materialized risk-gate state
```

No enforced MR blocking is added in M4-3r-b.

## What is intentionally not implemented

M4-3r-b does not yet implement:

- market intent -> paper order;
- next-final-10m-open fill;
- limit bar-touch fill;
- cancel-if-working transition;
- synthetic `on_position` feedback into strategy;
- Redis paper streams;
- restart restore;
- runtime adapter;
- ALOR-vs-FINAM paper comparison.

Those belong to M4-3r-c and later.

## Test coverage

The new `broker-core::paper` tests cover:

- ALOR-style `LiveOnly` vs `HistorySim` execution gate;
- live/final/timeframe strategy bar gate;
- valid buy/sell flat round-trip ledger snapshot;
- closed paper safety boundary requirement;
- missing order reference rejection;
- duplicate intent id rejection;
- active/terminal order status classification.

## Acceptance for M4-3r-b

M4-3r-b is accepted when:

- broker-neutral paper domain types exist in `broker-core`;
- types are exported from `broker-core`;
- ALOR paper execution mode semantics are represented;
- target-instrument flatness is represented;
- safety boundary defaults closed and validates closed;
- ledger invariants reject inconsistent snapshots;
- tests pass;
- live/order boundary remains closed.

Next stage: M4-3r-c deterministic paper ledger executor.
