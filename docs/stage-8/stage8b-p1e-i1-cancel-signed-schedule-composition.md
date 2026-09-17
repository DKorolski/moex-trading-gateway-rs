# Stage 8B-P1-e I1 Cancel signed-schedule composition

Status: local source implementation candidate after independent SOURCE ACCEPT
of generated-Market correction
`ff6639ef45f1504decb4be2f6d8981bb3ba172e7`.

## Scope

This slice composes only a freshly published paper `CancelOrder` with the
accepted signed-schedule and P1-d3 lifecycle. It does not implement committed
Cancel restart, Day-expiry composition, production `run`, or operational
activation.

The process now classifies CANCEL separately from unsupported publications.
Before any cancel effect it verifies:

- the immutable command bytes and command-publication receipt;
- the exact pending source M10 and its first canonical successor;
- the active broker order named by the durable Working book;
- equality between the command target and active broker order;
- equality between the command decision M10 and the Working-book predecessor;
- one verified signed schedule and its monotonic high-water;
- the durable V4 binding, covering seal and reread at latches D, E and F.

Only the resulting one-use schedule authority reaches the inherited P1-d3
cancel transition. Its three possible typed results remain separate:

- `S_ack` continues to replacement truth;
- already committed truth continues only to source resolution;
- target-first race continuation commits the recovered CANCEL before source
  resolution.

Every row returns through the existing shutdown-latch boundary. Source XACK is
available only after replacement truth.

## Missing successor

An absent future M10 is recoverable, not terminal. The same published owner is
returned through `CommandPublishedCancel`; schedule high-water is not advanced,
the source stays in the PEL, and the command is not republished. A later bounded
cycle rereads the signed schedule and can complete after the exact successor
arrives. Missing predecessor, M10 gap, identity mismatch, target mismatch and
schedule conflict remain fail closed.

## Executable evidence

The focused real-Redis tests prove:

- signed CANCEL reaches exact `Ready` through replacement truth and XACK-last;
- command-stream length remains one;
- missing successor retains the owner and source PEL;
- high-water remains unchanged during the wait;
- adding the exact successor lets the retained owner finish without callback or
  publication replay.

The inherited P1-d3 suite continues to cover ACK, direct-truth and target-first
cancel-race semantics. This slice adds their process-level typed routing; the
separate process crash/restart matrix remains later work.

Validation completed for this source candidate:

- `RUST_MIN_STACK=33554432 cargo test -p runtime-durable-service --all-features`
  — library 258 passed / 8 ignored, integration groups 3/3 and 6/6, doctests
  61/61;
- the two focused signed-Cancel process tests — 2/2 passed;
- strict all-targets/all-features Clippy with `-D warnings` — passed;
- `cargo fmt --all -- --check` and `git diff --check` — passed;
- inherited P1-d4 source negative harness — 60/60 mutations rejected.

## Deliberately closed

- committed Cancel V4 restart/recovery;
- Day-expiry signed-schedule composition and restart;
- production owner-loop wiring for exhausted/deferred ownership;
- operational Redis DB0/DB15 and VPS activation;
- FINAM POST/DELETE/send, broker dispatch, runtime-live and real orders.

Next roadmap slice after independent acceptance: Day-expiry signed-schedule
composition. Cancel and Day-expiry committed restart/recovery follow as a
separate boundary.
