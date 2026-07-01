# M3a-10 SQLite production-hardening / dry command-to-store integration

Status: dry/non-network. This document does not authorize FINAM
`POST /orders`, FINAM `DELETE /orders/{order_id}`, real command consumption,
runtime strategy attachment, `LiveReady`, live micro, stop/SLTP, or bracket
behavior.

## What M3a-10 adds

M3a-10 hardens the M3a-9 SQLite/WAL prototype while keeping the order path
mock-only:

- the sidecar writer lock now stores safe metadata:
  `instance_id`, `pid`, `created_ts`, and `schema_version`;
- stale/unknown writer locks are not removed automatically;
- if a writer lock is created but SQLite connection open fails, the lock is
  cleaned up before returning the open error;
- startup checks `order_path_schema.schema_version`;
- unknown/newer schema versions block writer startup;
- a read-only diagnostic connection can open alongside the writer;
- read-only diagnostics use SQLite read-only/query-only mode and cannot write;
- the SQLite file permission is hardened locally where the platform supports
  Unix permissions;
- append-only transition audit rows are written in the same transaction as
  record inserts/updates;
- operator disarm signals include store lock uncertainty, migration mismatch,
  and store unavailability.

The SQLite store still keeps the full `OrderPathRecord` payload locally so
future reconciliation can use raw client/broker ids inside the protected local
store. Public/runtime-facing exports remain redacted.

## Transition audit

The SQLite store now writes `order_path_transitions` rows with:

```text
id
request_id
from_state
to_state
event
reason_code
ts
safe_details
```

The audit is intentionally safe by default: it records state names, reason
codes, timestamps, and safe local details, not raw account ids, broker payloads,
or secrets.

## Dry simulator integration proof

M3a-10 adds SQLite-backed dry simulator tests:

- place flow persists `IntentRecorded -> SubmitInFlight` before the mock
  execution client is called;
- accepted place outcome persists `SubmitInFlight -> Submitted`;
- cancel flow persists `Submitted -> CancelRequested` before the mock execution
  client is called;
- returned broker-id mismatch persists
  `CancelRequested -> ManualInterventionRequired`;
- the mock client opens a read-only SQLite diagnostic connection during the
  simulated external call to prove that the pre-call state is already durable;
- published dry ACKs remain redacted and omit raw account/client/broker ids.

This is the key command-to-store ordering proof before any future endpoint
emitter is considered.

## Still required before real endpoint use

Before real FINAM order endpoint calls can be reviewed, the project still needs
at least:

- typed real endpoint request/response characterization without sending orders;
- endpoint feature flags and operator arm wiring around the real transport;
- bounded retry/backoff/rate-limit policy for order endpoints;
- broker-truth reconciliation loop using real order/trade snapshots;
- protected operator-only full-id diagnostic workflow;
- explicit backup/migration process before schema changes;
- live-readiness gate that blocks on SQLite open failure, lock uncertainty,
  schema mismatch, unknown active orders, DLQ/reconnect gaps, or stale
  reconciliation.

Until those are implemented and reviewed, M3a-10 remains a production-hardening
step for the dry order path only.

M3a-11 follow-up status: WAL/SHM/lock permission hardening, operator-only raw
diagnostic method names, safe transition audit event names, store-error disarm
mapping, explicit pre-endpoint gate decision, migration runbook, and
pre-endpoint fixture plan are now documented/implemented while FINAM order
endpoints remain disabled. See `docs/m3a11-final-pre-endpoint-gate.md`.
