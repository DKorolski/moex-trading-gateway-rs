# Stage 7B — Production Durability Composition
## File-backed Stage 6 authority, single-writer service, cross-process Redis recovery — paper/mock only

**Project:** broker-neutral MOEX runtime / FINAM migration  
**Accepted predecessor:** `2b6d6e90f2350b77fc1d79aa7381e6d9c6566c64` — Stage 7A CLOSED  
**Accepted Stage 6 base:** `10e357825a701193d964975bb5769bd0745d4986`  
**Target slice:** Stage 7B only  
**Date:** 2026-08-12  
**Normative artifacts:** this specification + `STAGE7B_ACCEPTANCE_MATRIX_2026-08-12.csv`  
**Live status:** FINAM POST/DELETE, broker dispatch, runtime-live, real orders and protective live execution remain CLOSED.

---

# 1. Objective

Stage 7B must turn the accepted Stage 6 durable lifecycle plus the accepted Stage 7A Redis consumer into a **cross-process durable paper command service**.

The required production-paper chain is:

```text
Redis command / PEL
  -> Stage 7A canonical decode + identity/profile policy
  -> one Stage 6 execution authority
  -> file-backed Stage 6 journal
  -> fsync-confirmed RequestAccepted
  -> fsync-confirmed DispatchAttemptRecorded
  -> only then process-local paper/mock provider
  -> fsync-confirmed normalized outcome
  -> fsync-confirmed RequestFinalized
  -> committed authenticated recovery seal
  -> idempotent Redis ACK/DLQ settlement
  -> XACK
```

The central property is:

> **No broker/paper effect may be authorized from process memory alone, and a process restart must never cause a blind semantic redispatch.**

Stage 7B does **not** prove exactly-once external broker execution. A crash after a durable dispatch-attempt but before a proven outcome must remain `ReconciliationRequired`, not be guessed or resent.

---

# 2. Entry conditions

Implementation must start from the exact accepted Stage 7A closure:

```text
2b6d6e90f2350b77fc1d79aa7381e6d9c6566c64
```

Before implementation:

- `current-status.md` records Stage 7A CLOSED at that ref;
- Stage 7B is the only active implementation candidate;
- Stage 8+, FINAM POST/DELETE, broker transport, runtime-live and real orders remain CLOSED.

Any other predecessor requires a new transition review.

---

# 3. Scope

## 3.1 In scope

Stage 7B may add:

1. a production-paper durable service/composition crate (preferred name: `runtime-durable-service`);
2. file-backed ownership of the accepted Stage 6 journal;
3. a minimal backend abstraction/refactor needed so `Stage6dDurableRuntimeRecovered` can own a file backend directly;
4. explicit first-boot vs restart file APIs;
5. OS/kernel single-writer ownership;
6. authenticated recovery-seal persistence;
7. cross-process restart reconstruction;
8. same command path as Stage 7A over real Redis;
9. cross-process canonical ACK reconstruction from Stage 6 durable facts;
10. idempotent Redis ACK/DLQ settlement;
11. composite production-paper readiness/supervision;
12. real filesystem + real Redis + real subprocess crash tests;
13. Stage 7B fault matrix, acceptance proof map, negative harness, gate and handoff tooling.

## 3.2 Explicitly out of scope

Stage 7B MUST NOT enable or implement:

- FINAM runtime `POST`;
- FINAM runtime `DELETE`;
- any external broker order/cancel dispatch;
- runtime-live;
- strategy-driven real orders;
- native Stop / StopLimit / SLTP / bracket / replace / multi-leg execution;
- continuous FINAM broker-truth reconciliation;
- ALOR-vs-FINAM live parity;
- Strategy 2 / Strategy 3 porting;
- a claim of exactly-once external broker effect.

Stage 7B remains **paper/mock only**.

---

# 4. Architecture decision — one Stage 6 journal authority

The accepted Stage 6 lifecycle remains the only execution authority.

Stage 7B MUST NOT introduce:

- a new command lifecycle state machine;
- a second writable journal;
- legacy M3e/M3h lifecycle authority;
- SQLite `OrderPathStore` as a dispatch/retry/terminal authority;
- file + memory dual-write authority.

## 4.1 Preferred minimal core refactor

The current accepted Stage 6 runtime owns:

```rust
journal: Stage6MemoryJournalBackend
```

The preferred change is an internal non-cloneable owned backend abstraction, for example:

```rust
enum Stage6OwnedJournalBackend {
    Memory(Stage6MemoryJournalBackend),
    File(Stage6FileJournalBackend),
}
```

implementing the existing `Stage6JournalBackend` contract.

Then:

- all accepted Stage 6 / 7A tests continue to use `Memory`;
- Stage 7B production-paper constructors use `File`;
- Stage 6 replay/checkpoint remain derived in-memory views;
- every Stage 6 append goes through exactly one owned backend.

Equivalent designs are acceptable only if they preserve these properties.

### Forbidden shortcut

This is not acceptable:

```text
open file journal
-> read bytes into Stage6MemoryJournalBackend
-> continue Stage 6 writes only in memory
-> separately/best-effort mirror to file
```

That would recreate dual authority.

---

# 5. Preferred Stage 7B crate boundary

Preferred new crate:

```text
crates/runtime-durable-service
```

Allowed dependencies should be limited to the broker-neutral/runtime stack:

```text
broker-core
strategy-runtime-core
runtime-command-bridge
redis
tokio
serde / serde_json
sha2
uuid
chrono
thiserror
one reviewed OS file-lock facility if required
```

Forbidden production dependencies:

```text
broker-finam
finam-gateway
reqwest
rusqlite / OrderPathStore
```

If the programmer chooses a different crate layout, the dependency direction must remain equivalent and be checker-pinned.

---

# 6. Durable storage layout

Use one durable directory bound to one operational identity/deployment generation.

Example:

```text
<durable_root>/
  stage6.journal
  stage6.writer.lock
  stage7b-recovery.seal
  tmp/
```

Exact filenames may differ.

Requirements:

- no path traversal;
- no silent reuse of another account/strategy/deployment identity;
- authoritative journal/seal paths must not be symlink escapes;
- data directory identity is validated against the authenticated operational identity;
- secrets/commitment key are not written into these files.

---

# 7. Single-writer ownership

Stage 7B must obtain a **kernel/OS advisory exclusive lock** before opening the writable journal/recovery state and before attaching the Redis consumer.

Do not use a `create_new()` sentinel file as the lock authority: a crash can leave stale filesystem state.

Required behavior:

```text
process A acquires lock
process B same durable identity -> fail closed

process A is SIGKILLed / child.kill()
OS releases lock
process C -> can acquire lock and recover
```

The lock is held for the full writable service lifetime.

The sidecar lock file may persist; the kernel lock, not file existence, is authority.

---

# 8. Stage 6 file journal requirements

The existing `Stage6FileJournalBackend` is the starting point.

Stage 7B must preserve:

- canonical framed records;
- hash chain;
- external frontier verification;
- `sync_data()` before durable append receipt;
- `DurabilityUncertain` fail-closed behavior;
- no automatic corruption/torn-tail repair.

## 8.1 Explicit create vs open-existing

Production Stage 7B must not use an ambiguous "open or create" restart path.

Required conceptual APIs:

```text
create_new_durable_journal(...)
open_existing_durable_journal(...)
```

Creation requires explicit first-boot authorization.

Restart with a missing journal is an error/RecoveryBlocked, never a new empty journal.

## 8.2 First creation directory barrier

After creating and syncing the journal header, Stage 7B must fsync the parent directory before the service can become ready.

## 8.3 Torn-tail policy

If a process crashes mid-frame and the file no longer scans canonically:

```text
NotReady / ManualIntervention
no auto truncate
no provider
no XACK
```

Stage 7B must not silently delete a potentially meaningful tail.

---

# 9. Authenticated recovery seal

The Stage 6 journal alone is the execution ledger, but restart also needs the accepted Stage 5G/Stage 6 authenticated recovery binding.

Stage 7B must persist a small **recovery seal**, not a second lifecycle database.

A recommended `Stage7bRecoverySealV1` contains:

```text
schema_version
seal_generation
created_at
stage6d_authenticated_restart_package bytes
stage6d_restart_package_sha256
stage6_checkpoint/frontier fingerprint
operational_identity fingerprint
```

The embedded Stage6D package already binds:

```text
Stage5G restart package
Stage6 checkpoint
operational identity
HMAC commitment
```

The raw commitment key is never serialized.

## 9.1 Initial seed

First durable boot requires a **source-produced authenticated Stage 5G clean-restart seed** matching the fresh runtime/config.

Stage 7B must not fabricate Stage 5 state from Redis or Stage 6 transport facts.

Before first `PaperReady`:

```text
journal initialized
-> Stage5G seed validated
-> Stage6 empty/current checkpoint sealed
-> recovery seal durably committed
-> only then Redis consumer may attach
```

## 9.2 Atomic seal replacement

Update protocol:

```text
write temp in same directory
-> sync temp file
-> atomic rename to authoritative seal
-> fsync parent directory
```

No in-place overwrite.

An orphan temp after crash is not authority.

---

# 10. Journal vs recovery-seal lag

`Stage6JournalCheckpointV1` may validly be a prefix of the journal.

Stage 7B must distinguish safe and unsafe lag.

## 10.1 Final historical Stage 6 requests beyond the seal

If all journal requests beyond the Stage5 seed are already final and accepted Stage5/6 cross-binding permits them as historical:

```text
restart may proceed
no truncation
full journal replay remains authority
```

## 10.2 Non-final Stage 6 request absent from Stage5 seed

If the journal contains a non-final request not represented by the Stage5 clean-restart package:

```text
RecoveryBlocked
no provider
no ACK/DLQ/XACK
no blind dispatch
no journal deletion
```

This is a safety state, not a reason to invent ownership.

## 10.3 Cross-bound active request

If the Stage5 restart package represents the same current request, normal accepted Stage5/Stage6 restart/cross-binding may proceed.

---

# 11. Durable command ordering

For a new accepted command, file-backed ordering is normative:

```text
RequestAccepted
  -> fsync / durable receipt

DispatchAttemptRecorded
  -> fsync / durable receipt

ONLY THEN:
paper/mock provider

normalized outcome record
  -> fsync

RequestFinalized
  -> fsync
```

A caller must never receive a linear dispatch capability before the file append receipt is durable.

---

# 12. Cross-process recovery semantics

Stage 7B must test abrupt process death using real subprocesses.

The key rules:

## 12.1 Crash after RequestAccepted

No effect has been authorized.

On restart, the same command may append the missing dispatch-attempt once and then invoke the paper provider.

## 12.2 Crash after DispatchAttemptRecorded

The system can no longer prove that no effect occurred.

Required:

```text
ReconciliationRequired
no blind redispatch
```

Even in paper mode, do not teach the architecture to infer "process died before HTTP call".

## 12.3 Crash after/during provider before outcome record

Same rule:

```text
ReconciliationRequired
no blind redispatch
```

## 12.4 Crash after durable outcome

The outcome is authoritative.

Restart must:

```text
replay outcome
-> finish RequestFinalized if needed
-> reconstruct the first canonical ACK
-> never call provider again
```

## 12.5 Crash after RequestFinalized

Canonical ACK is reconstructed from Stage 6 journal/replay.

It must not depend on the lost Stage 7A process-local:

```text
ack_publications
canonical_ack_recoveries
```

---

# 13. Recovery-seal settlement barrier

Before a finalized command is transport-settled/XACKed, Stage 7B must have a committed recovery seal whose Stage 6 checkpoint covers at least the command's `RequestFinalized` frontier.

Required order:

```text
RequestFinalized fsync
-> recovery seal commit
-> Redis ACK settlement
```

If recovery-seal persistence fails:

```text
entry remains pending
readiness false/degraded
no XACK
```

This gives a simple invariant:

> any command removed from the Redis PEL has a disk recovery package sufficient to reconstruct its Stage 6 final command state.

---

# 14. Cross-process canonical ACK reconstruction

Stage 7B must add a pure/deterministic recovery path from durable Stage 6 replay.

For a finalized request, reconstruct:

- request ID;
- durable client order ID;
- known broker order ID if represented;
- terminal ACK status/reason;
- exact source/identity fields required by the accepted broker-neutral ACK contract.

Rules:

```text
no prior known canonical settlement
-> first recovered ACK is canonical terminal ACK

known canonical settlement / exact later duplicate
-> Duplicate + DuplicateCommand
```

Direct Stage 5G compatibility remains required.

---

# 15. Redis settlement protocol

Stage 7A intentionally tolerated:

```text
ACK XADD
-> crash
-> XACK
```

as an at-least-once window.

Stage 7B should harden the transport layer because all operations are within the same Redis authority.

Implement one reviewed **idempotent settlement primitive** for:

```text
ACK publication + command XACK
DLQ publication + command XACK
```

A Lua script, transaction + deterministic settlement marker, or equivalent is acceptable.

The protocol must have a stable **transport settlement fingerprint** and prove:

- response loss after commit does not publish another canonical ACK;
- retry can determine whether settlement already happened;
- same settlement ID + different payload fingerprint fails closed;
- the settlement ID never participates in Stage 6 execution identity;
- uncertainty/authority holds are never settled.

Do not call this "exactly-once broker execution". It is only Redis transport settlement hardening.

---

# 16. Redis PEL recovery after process restart

A new service boot uses the accepted Stage 7A process-boot-unique consumer name.

After the configured idle threshold:

```text
XAUTOCLAIM old PEL
-> canonical Stage 7B handler
-> Stage 6 file replay/dedupe
```

Redis entry ID / consumer name / claim cursor remain transport-only.

The claim cursor may reset on a new process boot; it must not be persisted as execution identity.

---

# 17. Composite readiness

`PaperReady` requires **all** of:

```text
external service task alive
writer lock held
file journal valid
durability not uncertain
recovery seal valid
Stage5/Stage6 recovery authority usable
Redis source poll fresh
Redis claim scan fresh
ACK/DLQ settlement backend healthy
no unresolved authority/reconciliation block requiring intervention
```

Any missing storage authority dominates Redis health.

Stage 7B must also close the Stage 7A carry-forward for explicit task abort/cancellation: normal return, returned error, panic/JoinError and abort/cancel all clear external liveness.

---

# 18. No "exactly once" claim

Stage 7B MAY claim:

```text
durable identity
fsync-before-effect
cross-process journal replay
no blind redispatch
idempotent Redis settlement
```

Stage 7B MUST NOT claim:

```text
exactly-once external broker effect
```

because after:

```text
DispatchAttemptRecorded
-> external effect may or may not happen
-> crash before outcome
```

the only safe state is `ReconciliationRequired`.

Stage 8A/9 broker-truth reconciliation resolves this class.

The source descriptor should therefore pin:

```json
"cross_process_exactly_once_claimed": false
```

---

# 19. Mandatory Stage 7B cross-process fault matrix

The frozen Stage 7B fault matrix contains exactly 20 points:

| ID | Boundary | Required result |
|---|---|---|
| X01 | Boot/lock: Kill/error after writer lock acquisition but before journal open. | New process reacquires lock; no Redis attach/effect occurred. |
| X02 | Journal create: Crash after new journal header write but before file sync. | No readiness/effect; restart either sees invalid/uncommitted state and fails closed or valid header after explicit recovery policy. |
| X03 | Directory durability: Crash after journal file sync but before parent-directory fsync on first creation. | No PaperReady/consumer attach before directory fsync barrier. |
| X04 | RequestAccepted: Kill immediately after RequestAccepted fsync. | Restart/redelivery has one accepted record; may append missing dispatch once; no duplicate accepted. |
| X05 | DispatchAttempt: Kill immediately after DispatchAttemptRecorded fsync before provider. | Restart is ReconciliationRequired/hold; no blind redispatch. |
| X06 | Provider boundary: Kill during/after provider effect before durable outcome append. | Restart is ReconciliationRequired/hold; provider not blindly reinvoked. |
| X07 | Outcome record: Kill immediately after durable outcome append. | Restart reconstructs finalization/canonical ACK without provider. |
| X08 | RequestFinalized: Kill immediately after RequestFinalized fsync. | Restart reconstructs canonical ACK; no provider. |
| X09 | Seal temp write: Crash while writing replacement recovery-seal temp file. | Old committed seal remains authority; temp ignored. |
| X10 | Seal temp sync: Crash after temp seal sync but before rename. | Old committed seal remains authority; synced temp not auto-promoted. |
| X11 | Seal rename: Crash after atomic rename but before parent-dir fsync. | Restart accepts only a fully valid canonical old/new committed seal; never temp/partial data. |
| X12 | Pre-settlement: Kill after valid recovery seal but before ACK settlement. | PEL remains; restart settles canonical ACK once. |
| X13 | Settlement response loss: Redis settlement commits but client receives injected unknown/error. | Redis PEL/settlement marker resolves outcome; no duplicate canonical ACK. |
| X14 | ACK Redis outage: ACK settlement backend unavailable. | PEL remains; readiness degraded; no XACK. |
| X15 | DLQ Redis outage: DLQ settlement backend unavailable. | Poison entry remains pending; readiness degraded; no XACK. |
| X16 | PEL recovery: Kill new process during XAUTOCLAIM recovery. | Next boot can reclaim again; execution identity unchanged; no blind effect. |
| X17 | Writer death: SIGKILL/abrupt kill of lock holder. | Kernel lock releases; next process acquires and validates storage before Redis attach. |
| X18 | External mutation: Modify journal externally while service owns it. | Next append/read validation detects mutation and blocks effects. |
| X19 | Durability uncertain: Inject journal sync failure after full frame bytes written. | No append receipt/effect; current process locked out; restart validates actual disk state conservatively. |
| X20 | Lagging Stage5 seal: Restart with non-final Stage6 request absent from Stage5 package. | RecoveryBlocked; no effect/ACK/DLQ/XACK; no journal truncation. |


The Stage 7B descriptor must pin:

```text
cross_process_fault_count = 20
```

and the gate must produce a machine-readable report mapping X01–X20 to exact witnesses.

---

# 20. Real infrastructure tests

Mocks alone are insufficient.

Stage 7B must use:

- a real Redis server;
- a real filesystem/temp durable directory;
- a real kernel file lock;
- real child/subprocess boundaries;
- abrupt child termination for crash tests.

Test-only external files may be used to count paper-provider invocations across child processes, but they are test witnesses only and must never become production execution authority.

---

# 21. Preferred service state machine

Recommended high-level states:

```text
Starting
  -> LockAcquired
  -> StorageValidated
  -> RecoveryValidated
  -> PaperReady

or

Starting
  -> RecoveryBlocked
  -> ManualIntervention / waiting for accepted recovery input

PaperReady
  -> Degraded
  -> PaperReady

any
  -> Stopped
```

`RecoveryBlocked` must never invoke provider or settle a pending command.

---

# 22. Changed-path boundary

Preferred changed paths:

```text
Cargo.toml
Cargo.lock
crates/strategy-runtime-core/**
crates/runtime-command-bridge/**        # only narrow composition hooks/regressions
crates/runtime-durable-service/**       # preferred new owner/service
docs/stage-7/**
docs/current-status.md
docs/roadmap.md
docs/reviewer-onboarding-and-roadmap.md
scripts/stage7b_*
.github/workflows/... only if needed for hermetic Redis/subprocess gate
```

`broker-finam` and live endpoint code should have zero production Stage 7B diff.

Any broader Stage 5 semantic change requires separate justification.

---

# 23. Required deliverables

The Stage 7B handoff must include:

1. exact commit SHA and branch;
2. safe source archive + SHA-256 sidecar;
3. source-tree manifest with per-file SHA-256;
4. Stage 7B implementation/closure document;
5. machine-readable Stage 7B descriptor;
6. frozen 80-row acceptance matrix;
7. 80-row semantic proof map;
8. Stage 7B acceptance report;
9. 20-row cross-process fault matrix + report;
10. descriptor-pinned negative mutation harness;
11. static architecture checker;
12. closed-surface checker;
13. preseal checker;
14. full gate log;
15. toolchain log;
16. focused debug/release logs;
17. real Redis integration log;
18. real filesystem/writer-lock subprocess log;
19. restart/crash recovery log;
20. inherited accepted Stage 7A full-gate artifact;
21. updated current-status/roadmap;
22. explicit carry-forward list for Gate 7→8.

---

# 24. Required Stage 7B descriptor fields

At minimum:

```json
{
  "stage": "7B",
  "accepted_predecessor": "2b6d6e90f2350b77fc1d79aa7381e6d9c6566c64",
  "blocking_acceptance_rows": 80,
  "semantic_proof_map_count": 80,
  "cross_process_fault_count": 20,
  "negative_case_count": "<exact pinned integer>",
  "journal_backend": "file",
  "single_writer_required": true,
  "recovery_seal_required": true,
  "inherited_stage7a_gate_required": true,
  "cross_process_exactly_once_claimed": false,
  "finam_post_delete": false,
  "broker_network_dispatch": false,
  "runtime_live": false,
  "real_orders": false
}
```

The checker must compare the negative inventory to the descriptor value rather than deriving expected count from the list itself.

---

# 25. Acceptance proof-map rules

Because Stage 7A showed that "rows exist" is not enough, Stage 7B must use exact semantic proof binding from the first implementation.

For every B-001…B-080:

```text
row id
requirement
proof type
rationale
artifact
exact witness
```

Allowed proof types should be pinned, for example:

```text
git_gate
static_gate
unit
compile_fail
fs_integration
subprocess
subprocess_fault
real_redis
real_redis_fault
restart_integration
ordered_trace
fault_matrix
negative_harness
inherited_gate
governance_gate
artifact_integrity
```

A token from an adjacent test is not sufficient.

---

# 26. Negative harness minimum mutations

The negative harness must at least detect mutations that:

- replace file authority with MemoryJournalBackend;
- introduce file+memory dual write;
- remove explicit first-boot authorization;
- auto-create a missing restart journal;
- remove journal `sync_data`;
- remove parent directory fsync;
- weaken writer lock to a stale sentinel;
- allow second writer;
- remove external frontier validation;
- ignore durability uncertainty;
- invoke provider before durable dispatch receipt;
- auto-truncate torn journal;
- accept corrupt/missing recovery seal;
- write recovery seal in place;
- skip seal parent-dir fsync;
- XACK before committed recovery seal;
- reconstruct ACK only from process cache;
- emit first `Duplicate` after restart with no canonical ACK;
- blind redispatch after durable dispatch-attempt;
- settle a ReconciliationRequired/IdentityConflict entry;
- separate best-effort ACK XADD and XACK without the reviewed Stage 7B idempotent settlement protocol;
- reuse settlement marker as execution identity;
- heal storage failure from a Redis poll;
- omit source/claim freshness;
- omit abort/cancel liveness;
- consult legacy SQLite/M3 authority;
- set `cross_process_exactly_once_claimed=true`;
- add broker-finam/reqwest/FINAM POST/DELETE;
- remove real Redis test;
- remove real subprocess/lock test;
- remove inherited Stage 7A gate;
- delete one X01–X20 fault row;
- delete one B-001–B-080 proof mapping;
- unpin negative-case count;
- weaken handoff manifest/preseal binding.

The final mutation count must be descriptor-pinned.

---

# 27. Recommended Stage 7B gate

Conceptual order:

```bash
cargo fmt --all -- --check

python3 scripts/stage7b_check.py
python3 scripts/stage7b_closed_surface_check.py
python3 scripts/stage7b_negative_harness.py

# Frozen predecessor authority
detached checkout 2b6d6e90f2350b77fc1d79aa7381e6d9c6566c64
run accepted Stage 7A full gate

# Focused core/service
cargo test -p strategy-runtime-core stage7b_ -- --nocapture
cargo test -p strategy-runtime-core stage7b_ --release -- --nocapture
cargo test -p runtime-durable-service stage7b_ -- --nocapture
cargo test -p runtime-durable-service stage7b_ --release -- --nocapture

# Real infrastructure
cargo test -p runtime-durable-service --test stage7b_real_redis -- --nocapture
cargo test -p runtime-durable-service --test stage7b_writer_lock_subprocess -- --nocapture
cargo test -p runtime-durable-service --test stage7b_restart_crash_matrix -- --nocapture

# Regression
cargo test --workspace --all-targets
cargo test --workspace --doc
cargo clippy --workspace --all-targets --all-features -- -D warnings

python3 scripts/stage7b_fault_matrix_check.py
python3 scripts/stage7b_preseal_check.py
python3 scripts/stage7b_acceptance_report.py
```

Exact package names may differ, but equivalent coverage is mandatory.

---

# 28. Exit criteria

Stage 7B is acceptable only if an independent reviewer can state all of the following:

1. the accepted Stage 6 lifecycle now writes through one file-backed authority in the service path;
2. every dispatch capability is preceded by fsync-confirmed Stage 6 records;
3. journal creation/restart semantics cannot silently create a new empty authority;
4. one kernel writer owns one durable identity at a time;
5. process death releases the lock and a new process validates storage before Redis attach;
6. a committed recovery seal exists before a finalized command is XACKed;
7. final command ACK can be reconstructed after process restart without Stage 7A process caches;
8. a durable dispatch-attempt with unknown outcome is never blindly redispatched;
9. Redis PEL redelivery after restart feeds the same Stage 6 identity;
10. ACK/DLQ settlement is idempotent under response loss;
11. source/claim/storage/settlement/task readiness are independently represented;
12. abort/cancelled service task cannot leave stale readiness;
13. legacy SQLite/M3 state is not execution authority;
14. real Redis, real filesystem, real locks and real subprocess crashes are tested;
15. X01–X20 and B-001–B-080 are semantically proven;
16. inherited accepted Stage 7A remains green;
17. FINAM/live surfaces remain closed.

---

# 29. What Stage 7B acceptance opens

Stage 7B acceptance does **not** open real FINAM orders.

It opens only the next architectural gate:

# Transition Gate 7→8 / Stage 8A specification

That next gate must decide how the durable dispatch capability attaches to the FINAM adapter and how ambiguous broker outcomes are reconciled before any retry.

Promotion remains:

```text
Stage 7B durable paper service
-> independent review
-> Gate 7→8
-> Stage 8A protected FINAM adapter + ambiguous-outcome reconciliation authority
-> independent review
-> bounded Stage 8B real execution
-> Stage 9 continuous broker reconciliation
-> Stage 10 runtime-live readiness
-> Stage 11 paper/shadow + ALOR live-micro parity
-> Stage 12 controlled FINAM live-micro
```

---

# 30. Final programmer instruction

Implement **only Stage 7B**.

Do not opportunistically add FINAM POST/DELETE or Stage 8 adapter logic to the same commit.

The Stage 7B definition of done is:

> A Redis command may survive a hard process crash with its Stage 6 identity, durable lifecycle and settlement safety intact, while any uncertain post-dispatch state remains fail-closed and never causes a blind redispatch.
