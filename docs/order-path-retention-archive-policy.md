# Order-path retention and archive policy

Status: draft policy for the M3 dry order-path store. It does not authorize
live order endpoint calls.

## Local SQLite store

The SQLite order-path store is the protected local source of truth for
command-path state. It may contain raw local account/client/broker identifiers
needed for reconciliation and cancel mapping. It must stay local to the gateway
host and must not be included in handoff archives.

Future live-capable deployments must run with `umask 077` in a protected local
runtime directory. The store hardens DB/WAL/SHM/writer-lock file permissions
where the platform supports Unix permissions.

Default policy:

- do not auto-purge terminal records during first live-micro preparation;
- retain terminal and manual-intervention records until operator review;
- archive through redacted exports by default;
- take operator-controlled filesystem backups before any schema migration;
- do not upload SQLite database, WAL, SHM, or writer-lock files in review
  handoff packages.

## Redacted archive/export

Reviewable exports may include:

- `StrategyRequestId`;
- state;
- timestamps;
- safe ACK/reason codes;
- non-reversible length/SHA-256 fingerprints for account/client/broker ids.

Reviewable exports must not include:

- FINAM secret token or JWT;
- raw account ids;
- raw client order ids;
- raw broker order ids;
- raw broker payloads;
- raw outgoing comments;
- local `.env` or runtime logs.

## Writer-lock recovery

A stale writer-lock file is a safety stop, not an automatic cleanup candidate.
If a process crashes and leaves a lock behind, recovery is operator-controlled:

1. confirm no gateway process is still alive for the recorded `pid`;
2. inspect the lock metadata and SQLite file timestamps;
3. back up the SQLite database/WAL/SHM files if present;
4. remove the stale lock manually;
5. reopen in dry/read-only mode first and inspect records/audit;
6. only after review may a future live-capable gateway be re-armed.

Lock uncertainty must disarm order endpoints.

## Redis streams

Redis streams remain runtime/shadow transport, not durable order-path storage.
Retention defaults are documented in `docs/redis-stream-contract.md`. Redis
DLQ and ACK entries must remain redacted; full id correlation belongs to the
protected local SQLite store and broker-truth reconciliation.
