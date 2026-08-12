# Stage 7B-a — durability composition foundation

Status: implementation candidate; independent acceptance pending.

Accepted predecessor:
`2b6d6e90f2350b77fc1d79aa7381e6d9c6566c64` (Stage 7A CLOSED).

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
