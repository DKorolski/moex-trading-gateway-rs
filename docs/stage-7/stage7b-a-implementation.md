# Stage 7B-a-R1 — durability composition foundation repair

Status: review closure candidate; independent acceptance pending.

Accepted predecessor:
`2b6d6e90f2350b77fc1d79aa7381e6d9c6566c64` (Stage 7A CLOSED).

Rejected predecessor:
`73189245da2c8ab101cd9d897ddf4c187c34d6a0` (Stage 7B-a; architecture
retained, narrow durability/evidence repair required).

## Scope

This first Stage 7B slice changes only the journal ownership and file-open
boundary needed by the later durable service:

- `Stage6OwnedJournalBackend` is the one journal authority held by
  `Stage6dDurableRuntimeRecovered`;
- the owner is non-cloneable and delegates the accepted Stage 6 append,
  records, frontier, framed-bytes and checkpoint semantics;
- existing memory-backed Stage 6/7A entry points remain compatibility wrappers;
- new composition entry points transfer one owned backend into first boot or
  restart without constructing a second writable journal;
- production-capable file APIs are explicit: `create_new` and
  `open_existing`;
- create writes and syncs the canonical header, then syncs the parent
  directory before returning;
- open-existing never creates, truncates or repairs a missing/corrupt journal;
- focused tests prove memory/file/reopen parity and file authority ownership.

## R1 closure repairs

- every file append performs a complete scan of the current pre-existing
  journal and compares records, all frontiers, current frontier and final
  digest with the cached authority before writing any bytes;
- same-length mutation of an earlier or final record body, with the stored tail
  hash left untouched, returns `ExternalMutationDetected` and the failed append
  leaves the externally modified file byte-identical;
- memory/file and pre/post-reopen parity directly compare canonical checkpoint
  bytes and `Stage6ReplaySnapshotV1::semantic_fingerprint_sha256` using a
  replay-valid record sequence;
- the negative harness derives its reported case count from an explicit
  inventory and requires equality with the descriptor pin;
- proof-map witnesses refer to real functions and exact R1 tests.

## Frozen matrix byte policy

The supplied CSV used CRLF and had SHA-256
`a665d8638f4dfdfea6e13b680c8e5dce23f76811bf208c22f809668a8cd24b5c`.
The repository canonical form uses LF and has SHA-256
`083cc6e1e0925f11efa4bc093fd7c2d3d4cbeb05fd275f68ed71be3bdac1931d`.
Normalizing the supplied bytes from CRLF to LF produces the committed bytes;
there is no semantic row drift.

## Intentionally not implemented in 7B-a

- durable path identity/alias/symlink validation;
- OS advisory single-writer lock;
- recovery seal and Stage 5/Stage 6 cross-process binding;
- Redis ACK/DLQ settlement transaction;
- service readiness and task-abort guard;
- real subprocess X01-X20 fault execution;
- aggregate B-001..B-080 acceptance.

Those items remain inside Stage 7B and are split into reviewable follow-up
slices. Stage 7B-a must not be described as Stage 7B closure.

## Planned sequence

1. Stage 7B-a — owned backend and explicit create/open foundation.
2. Stage 7B-b — durable path validation and kernel single-writer lock.
3. Stage 7B-c — canonical recovery seal and restart composition.
4. Stage 7B-d — idempotent Redis settlement and durable paper service.
5. Stage 7B-e — X01-X20 subprocess/Redis matrix and aggregate closure.

## Closed surfaces

FINAM POST/DELETE, broker dispatch, runtime-live, real orders, unattended
execution and protective live orders remain false. Ambiguous post-dispatch
outcomes remain `ReconciliationRequired`; Stage 7B never claims external
exactly-once execution.
