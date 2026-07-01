# M3a-6/M3a-7 execution simulator and live-boundary decisions

Status: dry/non-network. This document does not authorize FINAM
`POST /orders`, FINAM `DELETE /orders/{order_id}`, real command consumption,
runtime strategy attachment, `LiveReady`, live micro, stop/SLTP, or bracket
behavior.

## Approved-only network boundary

Any future network client must be downstream of preflight and request-spec
building. The approved signatures are:

```rust
async fn place_approved(spec: FinamPlaceOrderRequestSpec)
async fn cancel_approved(spec: FinamCancelOrderRequestSpec)
```

or an equivalent API that accepts `PreflightApprovedPlaceOrder` /
`PreflightApprovedCancelOrder`.

The forbidden shape is:

```rust
async fn place(order: PlaceOrder)
async fn cancel(cancel: CancelOrder)
```

M3a encodes this as `FinamApprovedOrderExecutionClient`. The provided mock
client records only redacted request diagnostics and scripted outcomes. M3a-7
adds a compile-level contract test so the approved client boundary remains
request-spec-based, not raw-command-based.

## Dry execution simulator

`finam-gateway::simulate_place_order_approved()` models the future endpoint
boundary without network:

```text
preflight-approved command
  -> build dry FINAM request spec
  -> load persisted order-path record
  -> persist BeginSubmit
  -> call approved-only mock execution client
  -> apply Accepted / Rejected / Timeout to state machine
  -> return synthetic CommandAck
```

Covered dry outcomes:

- `Accepted` -> `Submitted`;
- `Accepted` without broker order id -> `SubmittedPendingBrokerOrderId` and
  `UnknownPending` / `ReconciliationRequired`;
- `Rejected` -> `BrokerRejected`;
- `Timeout` -> `TimeoutUnknownPending`.

Blind retry from `TimeoutUnknownPending` is blocked before the mock client is
called again.

## Dry cancel simulator

`finam-gateway::simulate_cancel_order_approved()` models the future cancel
endpoint boundary without network:

```text
preflight-approved cancel
  -> build dry FINAM cancel request spec
  -> load mapped place order-path record
  -> persist RequestCancel
  -> call approved-only mock execution client
  -> apply Accepted / Rejected / Timeout to state machine
  -> return synthetic CommandAck
```

Covered dry cancel outcomes:

- `Accepted` -> `CancelSubmitted`;
- `Rejected` -> `ManualInterventionRequired`;
- `Timeout` -> `CancelTimeoutUnknownPending`.

Blind cancel retry from `CancelTimeoutUnknownPending` is blocked before the
mock client is called again. Already-terminal cancel preflight remains a
no-endpoint/no-mock-call recovery path.

## Accepted without broker order id

An accepted place response without a broker order id is ambiguous. The safe
policy is:

- persist `SubmittedPendingBrokerOrderId`;
- publish a redacted `UnknownPending` ACK with `ReconciliationRequired`;
- disarm/operator-surface the condition;
- block cancel until broker truth recovers the broker order id by client order
  id or the operator records manual intervention.

## ACK contract decision

Runtime-facing Redis `CommandAck` stays redacted by default. The ACK stream must
not be the source of raw `ClientOrderId` or raw `BrokerOrderId`.

Correlation path:

```text
StrategyRequestId
  -> durable mapping store
  -> broker-truth order/trade snapshots
```

If a future internal operator tool needs full broker/client ids, it must read
from the protected durable mapping store or a protected broker-truth store, not
from public handoff archives or runtime-facing ACK exports.

## Production durable store decision

The first real endpoint path should use SQLite with WAL and a single-writer
execution model. `JsonFileOrderPathStore` remains a spec/test/restart backend,
not the production live-order store. The implementation ticket is
`docs/sqlite-order-path-store-implementation-ticket.md`.

Required SQLite properties before any real endpoint call:

- transaction commits the intent and `BeginSubmit` before network submit;
- durable sync/WAL settings are explicit and documented;
- single writer lock or actor owns all order-path mutations;
- unique indexes cover `StrategyRequestId`, `ClientOrderId`, and
  `BrokerOrderId`;
- corruption/open failure blocks order emission;
- file permissions are operator-audited;
- export tooling redacts account ids, broker order ids, comments, secrets, and
  raw broker payloads by default.

## Rate-limit/backoff dry policy

M3a-6 adds a dry window/backoff limiter for local tests. Before real endpoints,
the policy must be bound to method/account/instrument and must define:

- window length and capacity;
- separate place/cancel buckets if FINAM behavior requires it;
- 429/transport backoff duration;
- no blind retry after ambiguous placement timeout;
- operator-visible exhausted/backoff state.
