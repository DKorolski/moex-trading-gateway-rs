# Stage 8B-P1-e I1 Day-expiry signed-schedule composition

Status: local source implementation candidate on independently accepted Cancel
predecessor `cc1f02c19f35bb06136db2521538c6929f9dc101`.

## Scope

This slice composes only source-free Day expiry for an authenticated Ready
owner whose P1-d3 Working book has evaluated the signed schedule's exact last
eligible M10. It does not implement committed restart, the production owner
loop, operational Redis activation or FINAM execution.

An external due-timer classification may convert Ready into the opaque
`ReadyDayExpiry` schedule route, but grants no effect authority. The route must
read and verify a fresh signed `day_boundary/closed` schedule. Open, stale,
missing, conflicting or identity-mismatched evidence cannot expire an order.

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

## Source-free semantics

Day expiry acquires no canonical M10 and owns no source-XACK capability.
Success commits the terminal Expired book and returns the same composition to
Ready. Missing or retryable schedule reads retain the exact linear owner;
schedule high-water remains unchanged until binding succeeds.

## Executable evidence

The focused real-Redis tests prove:

- a fresh Closed schedule commits terminal Day expiry and returns Ready;
- the M10 PEL is zero before and after the source-free transition;
- a missing schedule exhausts the bounded policy while retaining
  `ReadyDayExpiry` and leaving high-water empty;
- a fresh signed Open schedule is rejected and cannot authorize Day expiry;
- publishing the exact Closed evidence lets that retained owner continue once
  to terminal Ready.

Validation completed for this source candidate:

- `RUST_MIN_STACK=33554432 cargo test -p runtime-durable-service --all-features`
  — library 261 passed / 8 ignored, integration groups 3/3 and 6/6, doctests
  61/61;
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
