# M3a-9 durable store prototype

Status: dry/non-network. This document does not authorize FINAM
`POST /orders`, FINAM `DELETE /orders/{order_id}`, real command consumption,
runtime strategy attachment, `LiveReady`, live micro, stop/SLTP, or bracket
behavior.

## What M3a-9 adds

M3a-9 adds the first SQLite/WAL order-path store prototype:

- `SqliteOrderPathStore`;
- `PRAGMA journal_mode = WAL`;
- `PRAGMA synchronous = FULL`;
- `PRAGMA foreign_keys = ON`;
- `PRAGMA busy_timeout = 5000`;
- writes use `BEGIN IMMEDIATE`;
- `request_id`, `client_order_id`, and `broker_order_id` are unique;
- a sidecar writer-lock file rejects a second writer;
- redacted export omits raw account/client/broker ids.

The prototype stores full `OrderPathRecord` payloads locally for durable
reopen/replay tests, while public exports use fingerprints.

## Reconciliation idempotency

Broker-truth polling may return the same fact repeatedly. M3a-9 policy:

| Input | Result |
|---|---|
| same `client_order_id`, same `broker_order_id` after recovery | idempotent `Ok(existing)` |
| same `client_order_id`, different `broker_order_id` | reconciliation mismatch error |
| `broker_order_id` already mapped to another request | duplicate broker id store error |
| pending `client_order_id` with new broker id | set broker id once and transition to `RecoveredByClientOrderId` |

This avoids false operator alerts from repeated read-only reconciliation while
still blocking conflicting broker truth.

## Prototype tests

Covered locally:

- WAL/FULL startup settings;
- second writer rejected by sidecar lock;
- insert/reopen by request and client id;
- `SubmitInFlight` can be reopened and recovered to `TimeoutUnknownPending`;
- `CancelRequested` is preserved after reopen;
- `SubmittedPendingBrokerOrderId` is preserved after reopen;
- corrupt database open blocks store use;
- redacted export does not expose raw account/client/broker ids.

## Still required before real endpoint use

- schema migration/versioning policy;
- protected operator-only full-id view;
- explicit file permissions check;
- recovery policy for stale writer lock after process crash;
- transaction-level integration with future endpoint emitter;
- reconciliation-loop backoff/staleness policy;
- production readiness gate that refuses live orders if SQLite open/migration
  fails.

Until those are accepted, this remains a prototype backend for dry tests.

M3a-10 follow-up status: schema-version guard, writer-lock metadata,
stale-lock policy, read-only diagnostics, transition audit, and SQLite-backed
dry simulator ordering tests are now implemented while still keeping FINAM
order endpoints disabled. See
`docs/m3a10-sqlite-production-hardening.md`.
