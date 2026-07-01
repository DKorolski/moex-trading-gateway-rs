# SQLite order-path store implementation ticket

Status: design ticket plus M3a-9 dry prototype notes for the first
production-grade durable store. This does not authorize FINAM order endpoints,
real command consumption, runtime strategy attachment, `LiveReady`, live micro,
stop/SLTP, or bracket behavior.

## Goal

Replace the JSON-file spec/test backend for real order emission with a
single-writer SQLite/WAL store that can atomically persist intent and execution
state before any future network call.

## Required schema

Target production table:

```sql
CREATE TABLE order_path_records (
  request_id TEXT PRIMARY KEY,
  client_order_id TEXT NOT NULL UNIQUE,
  broker_order_id TEXT UNIQUE,
  command_kind TEXT NOT NULL,
  account_fingerprint_len INTEGER NOT NULL,
  account_fingerprint_sha256 TEXT NOT NULL,
  instrument_symbol TEXT NOT NULL,
  venue_symbol_fingerprint_len INTEGER,
  venue_symbol_fingerprint_sha256 TEXT,
  side TEXT,
  order_type TEXT,
  qty TEXT,
  limit_price TEXT,
  time_in_force TEXT,
  created_ts TEXT NOT NULL,
  last_update_ts TEXT NOT NULL,
  submit_attempt_count INTEGER NOT NULL,
  cancel_attempt_count INTEGER NOT NULL,
  state TEXT NOT NULL,
  last_ack_status TEXT,
  last_error_kind TEXT,
  last_reconciliation_source TEXT,
  outgoing_comment_fingerprint_len INTEGER,
  outgoing_comment_fingerprint_sha256 TEXT
);
```

Notes:

- `request_id`, `client_order_id`, and `broker_order_id` are unique.
- Raw account ids, venue symbols, broker payloads, secrets, JWTs, and outgoing
  comments must not be exported by default.
- A protected local operator view may join full ids only if file permissions and
  export redaction are explicit.
- M3a-9 prototype uses a smaller indexed table plus serialized
  `OrderPathRecord` payload while the production schema is still being
  finalized.

## Transaction contract

Before a future place submit:

```text
BEGIN IMMEDIATE;
insert/update intent record;
transition IntentRecorded -> SubmitInFlight;
commit;
only then call network endpoint;
```

Before a future cancel submit:

```text
BEGIN IMMEDIATE;
validate mapped broker_order_id and non-terminal/non-unknown state;
transition Submitted/RecoveredByClientOrderId -> CancelRequested;
commit;
only then call network endpoint;
```

If commit fails, endpoint emission is blocked.

## SQLite settings

Candidate startup settings:

```sql
PRAGMA journal_mode = WAL;
PRAGMA synchronous = FULL;
PRAGMA foreign_keys = ON;
PRAGMA busy_timeout = 5000;
```

Any relaxation of `synchronous` must be reviewed separately before live orders.

## Single-writer model

Order-path writes must be owned by one process/actor. Concurrent writers are not
allowed for first live micro. If a second process cannot acquire the writer
lease/lock, order emission is blocked.

M3a-9 prototype implements this as a sidecar writer-lock file and rejects a
second writer in local tests. Stale-lock recovery policy remains a future
review item.

## Recovery behavior

On startup:

- open failure blocks order emission;
- schema migration failure blocks order emission;
- duplicate unique keys block order emission;
- `SubmitInFlight` recovers to unknown-pending before any retry;
- `CancelRequested` / `CancelSubmitted` recover to cancel-unknown-pending or
  require bounded broker-truth reconciliation;
- terminal states cannot be overwritten by non-terminal states.

## Acceptance tests

- insert intent and `BeginSubmit` commit before mocked endpoint call;
- duplicate request/client/broker ids reject;
- crash/reopen preserves `SubmitInFlight` and recovery behavior;
- cancel transaction persists `CancelRequested` before mocked cancel endpoint;
- client-id recovery sets `broker_order_id` once and rejects duplicate broker
  truth ids;
- cancel accepted with returned broker-id mismatch persists manual intervention
  without exposing raw ids in public ACK/export paths;
- corruption/open failure blocks endpoint emission;
- redacted export omits account ids, broker ids, comments, secrets, JWTs, and raw
  broker payload fragments.

## M3a-9 prototype status

Implemented in dry code only:

- SQLite/WAL startup with `synchronous=FULL`;
- `BEGIN IMMEDIATE` write transactions;
- unique request/client/broker ids;
- sidecar single-writer lock;
- crash/reopen tests for `SubmitInFlight`, `CancelRequested`, and
  `SubmittedPendingBrokerOrderId`;
- corrupt database open failure blocks use;
- redacted export tests.

Still required before real endpoint use:

- production schema migration/versioning;
- operator-audited file permissions;
- stale writer-lock recovery policy;
- live endpoint integration gate;
- protected full-id diagnostic view.
