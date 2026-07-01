# M3a-11 final pre-endpoint order-path gate

Status: dry/non-network. This document does not authorize FINAM
`POST /orders`, FINAM `DELETE /orders/{order_id}`, real command consumption,
runtime strategy attachment, `LiveReady`, live micro, stop/SLTP, or bracket
behavior.

M3a-11 closes the remaining pre-endpoint order-path hardening items from review
round 29.

## WAL/SHM and runtime file permissions

The SQLite store now hardens permissions for the local runtime files it owns:

- main SQLite database file;
- `-wal` sidecar when present;
- `-shm` sidecar when present;
- writer-lock sidecar.

Deployment policy for any future live-capable process:

```text
umask 077
protected local runtime directory
no SQLite/WAL/SHM/lock files in handoff archives
redacted exports only for review/reporting
```

The code still treats permission-hardening failures as store errors. Store
errors must disarm endpoint-capable modes before any external order call is
attempted.

## Diagnostic API boundary

`SqliteOrderPathReadStore` is explicitly an operator/internal diagnostic
surface. It is SQLite read-only/query-only, but its raw record lookup methods
are now named with the `operator_` prefix:

```text
operator_load_by_request_id
operator_load_by_client_order_id
operator_load_by_broker_order_id
operator_all_records
```

Review/export/reporting code must use `redacted_records()` instead. Runtime ACK
publication must remain redacted and must not expose raw client or broker order
ids.

## Transition audit event names

Transition audit rows now use safe inferred event names instead of only
`UpdateRecord`:

```text
BeginSubmit
SubmitAccepted
SubmitAcceptedWithoutBrokerOrderId
SubmitTimedOut
RecoverByClientOrderId
BrokerReject
RequestCancel
CancelAccepted
CancelRejected
CancelTimedOut
RequireManualIntervention
RecoverCancelTerminal
MarkTerminal
```

The audit remains safe: it stores state names, event names, reason codes,
timestamps, and safe local details, not raw account ids, broker payloads,
secrets, JWTs, or arbitrary broker text.

## Store error to operator disarm mapping

`OrderPathStoreError::operator_disarm_signal()` maps durable-store failures to
operator safety signals:

| Store error | Disarm signal |
|---|---|
| writer lock unavailable | `OrderPathStoreLockUncertain` |
| schema version mismatch | `OrderPathStoreMigrationMismatch` |
| other store/open/serialization/conflict error | `OrderPathStoreUnavailable` |

This is the final pre-endpoint hook: any future endpoint-capable mode must
convert store startup/write failures into operator-visible disarm before
network emission.

## SQLite migration and backup runbook

No automatic schema migration is added in M3a-11. Unknown/newer schemas still
block startup. Future schema changes must follow this operator runbook:

1. Stop the gateway process.
2. Confirm no writer lock belongs to a live process.
3. Back up the SQLite database, `-wal`, `-shm`, and writer-lock metadata if
   present.
4. Record checksums for the backup files.
5. Run the reviewed migration tool/script offline.
6. Open the migrated store through `SqliteOrderPathReadStore::open_readonly`.
7. Inspect redacted records and transition audit.
8. Open the writer store.
9. Only after operator review may a future live-capable gateway be re-armed.

If any step is uncertain, endpoint-capable mode remains disarmed.

## Pre-endpoint FINAM response fixture plan

Before implementing real FINAM order transport, create synthetic or redacted
fixtures for the expected endpoint response classes:

- place accepted with broker order id;
- place accepted without broker order id;
- place rejected with safe reason code;
- place timeout / transport unknown;
- cancel accepted with no returned broker id;
- cancel accepted with matching broker id;
- cancel accepted with mismatched broker id;
- cancel rejected with safe reason code;
- cancel timeout / transport unknown;
- malformed/decode error response shape;
- rate-limit / retry-after style response shape if FINAM exposes one;
- maintenance/session-closed style response shape if FINAM exposes one.

Fixtures must not contain real account ids, real broker order ids, raw broker
payloads, secrets, JWTs, or local `.env` values. Use synthetic values or
redacted shape metadata.

## Real endpoint feature gate design

`GatewayFeatureSet::real_order_endpoint_gate_decision()` and
`FinamGateway::real_order_endpoint_gate_decision()` now expose the current
decision:

```text
endpoint_calls_allowed = false
blocking_reasons includes M3a11PreEndpointReviewRequired
runtime_ack_id_policy = RedactedRuntimeAckOnly
```

Even if adjacent flags are manually enabled in config, M3a-11 keeps real
endpoint calls blocked until a later reviewed implementation deliberately
changes this decision.

## ACK/id policy locked for the next stage

Runtime-facing `CommandAck` publication remains:

```text
RedactedRuntimeAckOnly
```

Raw client/broker ids belong only to the protected local order-path store and
operator/internal diagnostics. Redis ACKs and review exports use
`StrategyRequestId`, safe status/reason codes, and redacted/fingerprinted
diagnostics.

## Still not allowed

- FINAM real PlaceOrder endpoint calls;
- FINAM real CancelOrder endpoint calls;
- real command stream consumer connected to strategies;
- real CommandAck lifecycle against FINAM endpoints;
- strategy runtime adaptation;
- `LiveReady` publication;
- first live micro;
- stop/SLTP/bracket.
