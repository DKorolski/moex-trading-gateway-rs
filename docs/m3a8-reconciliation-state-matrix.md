# M3a-8 reconciliation state matrix

Status: dry/non-network. This document does not authorize FINAM
`POST /orders`, FINAM `DELETE /orders/{order_id}`, real command consumption,
runtime strategy attachment, `LiveReady`, live micro, stop/SLTP, or bracket
behavior.

## Purpose

M3a-8 makes the dry order path reconciliation-ready before any real endpoint is
enabled. The goal is to prove the safe state/ACK policy for ambiguous broker
responses and broker-truth recovery.

## Place / recovery matrix

| Input | State result | ACK result | Cancel policy |
|---|---|---|---|
| place accepted with broker order id | `Submitted` | `Submitted` / `SyntheticSubmitted` | allowed after normal cancel preflight |
| place accepted without broker order id | `SubmittedPendingBrokerOrderId` | `UnknownPending` / `ReconciliationRequired` | blocked until broker-truth recovery |
| place timeout | `TimeoutUnknownPending` | `Timeout` / `TransportTimeout` | blocked until broker-truth recovery or manual decision |
| place rejected | `BrokerRejected` | `Rejected` / broker-safe code | terminal/local no cancel submit |
| client-id recovery finds broker id | `RecoveredByClientOrderId` | recovery fact via broker truth | allowed after normal cancel preflight |
| repeated client-id recovery with same broker id | unchanged recovered record | idempotent success | no false operator alert |
| repeated client-id recovery with different broker id | unchanged record | reconciliation mismatch error | operator-visible issue |
| client-id recovery finds duplicate broker id | unchanged pending record | store error, no public raw id | blocked; operator-visible reconciliation issue |
| client-id recovery on non-recoverable state | unchanged record | reconciliation error | blocked unless normal preflight already permits state |

Recovery helper contract:

```text
client_order_id -> broker_order_id
  -> set broker_order_id once
  -> RecoverByClientOrderId
  -> persist through OrderPathStore
```

The helper is allowed only for `SubmittedPendingBrokerOrderId` and
`TimeoutUnknownPending`. It must not overwrite an existing broker id.

## Cancel matrix

| Input | State result | ACK result | Endpoint/mocking policy |
|---|---|---|---|
| cancel active known mapped order | `CancelSubmitted` | `Submitted` / `SyntheticSubmitted` | dry mock only |
| cancel already terminal order | unchanged terminal/recovered | `AlreadyTerminal` preflight branch | no endpoint/mock call |
| cancel requested id mismatches durable mapping | unchanged | local `Rejected` | no endpoint/mock call |
| cancel accepted with no returned broker id | `CancelSubmitted` | `Submitted` / `SyntheticSubmitted` | accepted |
| cancel accepted with matching returned broker id | `CancelSubmitted` | `Submitted` / `SyntheticSubmitted` | accepted |
| cancel accepted with mismatched returned broker id | `ManualInterventionRequired` | `UnknownPending` / `ManualInterventionRequired` | no silent accept |
| cancel rejected by broker | `ManualInterventionRequired` | `Rejected` / broker-safe code | requires broker-truth reconciliation |
| cancel timeout | `CancelTimeoutUnknownPending` | `Timeout` / `TransportTimeout` | no blind retry |
| cancel retry from `CancelTimeoutUnknownPending` | unchanged | transition/preflight error | mock/client not called |
| broker truth proves terminal after cancel timeout | `CancelRecoveredTerminal` | `Recovered` | no duplicate cancel |

## Operator visibility

Dry order path may disarm for these reconciliation safety signals:

- `UnknownPendingOrder`;
- `AcceptedWithoutBrokerOrderId`;
- `CancelBrokerOrderIdMismatch`;
- `CancelTimeoutUnknownPending`;
- `ReconciliationStale`;
- `RestartRecovery`.

## Boundary guard

The approved execution boundary remains request-spec based:

```rust
place_approved(FinamPlaceOrderRequestSpec)
cancel_approved(FinamCancelOrderRequestSpec)
```

Raw command execution boundaries remain forbidden:

```rust
place(order: PlaceOrder)
cancel(cancel: CancelOrder)
```

M3a-8 adds source-scan coverage for this rule across the order-adjacent crates.
M3a-9 expands it to the whole `crates/` Rust source tree so newly added crates
or modules must also preserve the approved-only boundary.

## Durable store note

M3a-9 adds a SQLite/WAL prototype with `BEGIN IMMEDIATE` writes, unique
request/client/broker ids, a sidecar single-writer lock, crash/reopen tests, and
redacted export. It is still dry/non-network and does not authorize real FINAM
order endpoints.
