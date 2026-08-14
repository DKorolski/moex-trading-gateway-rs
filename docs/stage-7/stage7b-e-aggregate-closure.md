# Stage 7B-e — aggregate durability closure

Status: R2 aggregate acceptance candidate; independent acceptance pending.

R1 at `422bd1a8b45bfd3397aa588f914494cc11f5c401` was not accepted. R2 restores
the mandatory inherited Stage 7A full gate, pins the descriptor's inherited
single-writer and recovery-seal requirements, and freezes the exact normative
X01-X20 semantics and proof types against the accepted Stage 7B TZ.

Accepted predecessor: Stage 7B-d-c-R2 at
`2b6371adb905654e0ddd8b6714159bcef737b577`.

This slice adds no execution feature. It aggregates the accepted Stage 7B
foundation and d-a/d-b/d-c behavior into the final frozen durability evidence:

- one file-backed Stage 6 lifecycle/journal authority across the complete
  paper command path;
- parent-directory fsync barriers for first journal creation and recovery-seal
  replacement;
- exact normative X01-X20 fault mapping with real filesystem, Redis, kernel lock,
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

`stage7b-fault-matrix-normative.json` pins exactly X01-X20. New aggregate
witnesses add:

- X01 SIGKILL after writer-lock acquisition and before journal open;
- X02 SIGKILL after the complete new-journal header write and before its first
  file sync, followed by conservative restart validation;
- X12 real-Redis SIGKILL after a committed/reread seal and before settlement;
  PEL survives and restart emits one canonical ACK without provider repeat;
- X15 a real Redis DLQ outage retaining PEL and degraded health;
- X16 SIGKILL during `XAUTOCLAIM`, followed by complete reclaim under a fresh
  process consumer identity.
- X19 a full-frame sync failure with no receipt/effect, current-process
  lockout and conservative reopen of the same file.

The remaining rows bind to accepted crash/restart, seal, Redis settlement,
external mutation and durability-uncertain witnesses. Both debug and release
logs must contain every executable witness.

X03 and X11 are the two explicit static power-loss exceptions: their exact
file/rename-to-parent-directory-sync order is source-gated and mutation-tested,
while restart authority is covered separately. The R2 aggregate also executes
the inherited Stage 7A gate at
`2b6d6e90f2350b77fc1d79aa7381e6d9c6566c64` as a standalone mandatory gate.

## Closed surfaces

- FINAM POST/DELETE: false;
- broker network dispatch: false;
- runtime-live: false;
- real orders: false;
- protective live orders: false;
- external exactly-once execution claim: false.
