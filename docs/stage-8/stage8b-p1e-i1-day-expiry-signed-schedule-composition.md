# Stage 8B-P1-e I1 Day-expiry signed-schedule composition

Status: P1-DEX01 correction candidate on held Day-expiry source checkpoint
`9e6217a40743bdfc35f950927c8a93180732c675`; independently accepted Cancel
predecessor remains `cc1f02c19f35bb06136db2521538c6929f9dc101`.

## Scope

This slice composes only source-free Day expiry for an authenticated Ready
owner whose P1-d3 Working book has evaluated the signed schedule's exact last
eligible M10. It does not implement committed restart, the production owner
loop, operational Redis activation or FINAM execution.

An external due-timer classification may convert Ready into the opaque
`ReadyDayExpiry` schedule route only after its verification context has been
restored from the authenticated durable V4 schedule high-water. A populated
external context must match that value exactly. The private route retains the
exact recovered high-water and rechecks it before every Redis read, but grants
no effect authority. The route must read and verify a fresh signed
`day_boundary/closed` schedule. Open, stale, missing, rollback, conflicting or
identity-mismatched evidence cannot expire an order.

## Exact durable binding

The Day-expiry binding uses four values from authenticated durable state:

- active broker order ID;
- current Working-book transition hash;
- the predecessor M10 from the latest checkpoint-covered Initial/Working V4;
- the same V4 candidate, cross-validated as the Working book's evaluated
  last-eligible M10.

No Redis ID, hash or predecessor is reconstructed from wall-clock time. The
Closed schedule is bound through the existing D/E/F latch sequence and durable
V4 commit+reread. Only the resulting one-use Day-expiry authority reaches the
accepted P1-d3 `expire_working_limit` transition.

The accepted progression itself is one history: durable Open sequence/revision
`1/1` followed by a later-published Closed `2/2`. An equal-sequence semantic
change, an increased sequence with unchanged semantic revision, sequence or
revision rollback, and a conflicting external high-water all fail before a
new V4, seal or expiry effect. The core `validate_progression` contract is
unchanged.

The low-level signed-snapshot Day-expiry bridge is crate-private. The public
process entry can only be constructed through durable high-water admission, so
a preverified snapshot cannot restart progression at Bootstrap.

## Source-free semantics

Day expiry acquires no canonical M10 and owns no source-XACK capability.
Success commits the terminal Expired book and returns the same composition to
Ready. Missing or retryable schedule reads retain the exact linear owner;
the restored durable high-water remains unchanged until binding succeeds.
Redis retention may remove the historical Open row without removing this
protection because V4, rather than Redis history, is the progression source of
truth.

## Executable evidence

The focused real-Redis tests prove:

- durable Open `1/1` followed by fresh Closed `2/2` commits terminal Day expiry
  once and returns Ready;
- the M10 PEL is zero before and after the source-free transition;
- a missing schedule exhausts the bounded policy while retaining
  `ReadyDayExpiry` and the exact durable Open high-water;
- a fresh signed Open schedule is rejected and cannot authorize Day expiry;
- publishing the exact Closed evidence lets that retained owner continue once
  to terminal Ready;
- conflicting Closed `1/1`, Closed `2/1`, sequence rollback and revision
  rollback are rejected before any durable-file mutation;
- a nonempty conflicting external high-water is rejected at admission without
  being overwritten;
- an empty Redis schedule stream cannot turn an existing durable V4 into a
  Bootstrap progression.

Validation completed for this source candidate:

- focused Day-expiry matrix — 7/7 passed;
- `RUST_MIN_STACK=33554432 cargo test -p runtime-durable-service --all-features`
  — library 263 passed / 8 ignored, writer-lock integration 6/6 and doctests
  61/61; the Redis-service integration target passed 3/3 on isolated rerun
  after one unrelated X16 timing barrier miss during the concurrently loaded
  aggregate run;
- `cargo test -p strategy-runtime-core --all-features` — library 1283/1283,
  integration groups 2/2, 6/6, 15/15 and 3/3, doctests 69/69;
- strict all-targets/all-features Clippy with `-D warnings` for both changed
  crates — passed;
- `cargo fmt --all -- --check` and `git diff --check` — passed;
- inherited P1-d4 source negative harness — 60/60 mutations rejected;
- reusable I1A source negative harness — 102/102 mutations rejected.

## Deliberately closed

- committed Cancel/Day-expiry V4 restart and recovery;
- production retention/wakeup of deferred and exhausted owners;
- process signal, panic, SIGKILL and restart acceptance matrix;
- operational Redis DB0/DB15 and VPS activation;
- FINAM POST/DELETE/send, broker dispatch, runtime-live and real orders.

Next roadmap boundary after source acceptance: committed Cancel/Day-expiry
restart/recovery, including explicit intermediate PEL and truth-before-XACK
evidence for the accepted Cancel target-first path.
