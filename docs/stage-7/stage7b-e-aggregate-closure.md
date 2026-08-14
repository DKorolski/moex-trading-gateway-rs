# Stage 7B-e — aggregate durability closure

Status: aggregate acceptance candidate; independent acceptance pending.

Accepted predecessor: Stage 7B-d-c-R2 at
`2b6371adb905654e0ddd8b6714159bcef737b577`.

This slice adds no execution feature. It aggregates the accepted Stage 7B
foundation and d-a/d-b/d-c behavior into the final frozen durability evidence:

- one file-backed Stage 6 lifecycle/journal authority across the complete
  paper command path;
- parent-directory fsync barriers for first journal creation and recovery-seal
  replacement;
- exact X01-X20 fault mapping with real filesystem, Redis, kernel lock,
  subprocess and SIGKILL witnesses;
- focused debug/release and full workspace regression gates;
- the semantic acceptance proof map at 80/80;
- immutable source/evidence manifest and SHA-256 handoff sidecar.

The candidate deliberately records `stage7b_accepted=false`. Only an
independent aggregate review may close Stage 7B and allow preparation of the
Gate 7→8 specification. It does not itself authorize Stage 8.

## Aggregate authority boundary

`Stage6dRecoveredPaperCore` owns one `Stage6OwnedJournalBackend` and the
production recovery owner owns that core linearly. `Stage7bRedisService` owns
the recovery owner but no journal or alternative lifecycle store. Memory
journals remain test/fixture facilities; Redis markers remain transport-only.

The Stage 7B-e checker compares production code before each test-module
boundary with the accepted d-c ref, so the aggregate slice can add witnesses
but cannot silently alter the accepted functional implementation.

## Directory durability

First journal creation is ordered as header write, file sync, parent-directory
sync, then authority return. Recovery-seal replacement is ordered as temporary
write, temporary sync, rename, durable-root directory sync, canonical reread.
X03 and X11 bind these source barriers to the crash policy and the aggregate
negative harness removes each barrier independently.

## Fault and infrastructure evidence

`stage7b-fault-matrix.json` pins exactly X01-X20. New aggregate witnesses add:

- X01 SIGKILL after writer-lock acquisition and before journal open;
- X15 a real Redis DLQ outage retaining PEL and degraded health;
- X16 SIGKILL during `XAUTOCLAIM`, followed by complete reclaim under a fresh
  process consumer identity.

The remaining rows bind to accepted crash/restart, seal, Redis settlement,
external mutation and durability-uncertain witnesses. Both debug and release
logs must contain every executable witness.

## Closed surfaces

- FINAM POST/DELETE: false;
- broker network dispatch: false;
- runtime-live: false;
- real orders: false;
- protective live orders: false;
- external exactly-once execution claim: false.
