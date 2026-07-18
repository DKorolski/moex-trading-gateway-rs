# Stage 5D-final-restart-r3 — discovery and next gate

Status: discovery / review-closure input, no-I/O.

This note records the result of starting the Stage 5D-final-restart-r3 work after
the r2 review hold. The r2 canonical durable package remains the retained
foundation, but r3 is not closed.

The trading boundary remains closed:

- no Redis;
- no FINAM;
- no broker transport;
- no dispatch/send;
- no runtime-live;
- no broker execution;
- no real endpoints or live orders.

## What was attempted

The r3 assignment requires the mandatory positive matrix to execute the real
package path instead of relying on inactive or comment-only evidence:

```text
source lifecycle/callback producer
-> semantic/private export
-> canonical restart package
-> strict package bytes
-> fresh runtime
-> strict package decode
-> private apply
-> broker-truth bootstrap
-> riskgate injection
-> runtime-state-restored
-> Stage 5C continuation
```

When the disabled positive cases were made executable locally, the pending-entry
path exposed a source-runtime restore gap rather than a package-only gap.

## Discovered gap

`HybridIntradayRuntimeStrategy::set_state(...)` reconstructs semantic
`pending_entry` from persisted `StrategyState::HybridIntradayRuntime`, but the
current frozen source implementation restores it as a generic market pending
entry:

- `ReasonCode::MorningMeanReversionLong`;
- `EntryStyle::Market`;
- no stop price;
- no take price.

That is not sufficient for an MR bracket pending entry that has authoritative
`mr_stop_price` and `mr_take_price` in the semantic state. A genuine r3
pending-entry positive case therefore cannot honestly prove exact package
restore semantics while this source restore path degrades the pending-entry
shape.

The intended source-compatible behavior is:

- MR Long -> `MorningMeanReversionLong`;
- MR Short -> `MorningMeanReversionShort`;
- BO Long -> `BreakoutLong`;
- BO Short -> `BreakoutShort`;
- MR pending entry with both `mr_stop_price` and `mr_take_price` restores as
  `EntryStyle::Bracket`;
- bracket stop/take prices are preserved;
- BO and incomplete MR pending entries remain market-style.

## Freeze interaction

A local patch implementing that restore correction was intentionally not kept in
this handoff, because `hybrid_intraday_runtime.rs` is protected by the current
Stage 5D additive freeze checker. The checker correctly fails on direct source
runtime drift unless the change is authorized through an explicit freeze
extension/rebaseline.

Therefore this package does not claim r3 acceptance and does not change the
frozen runtime source.

## Stage 5D-final-restart-r3a result

Stage 5D-final-restart-r3a added executable proof for source-produced pending
entry ownership before resuming full r3 closure:

- MR Long bracket pending entry;
- MR Short bracket pending entry;
- BO Long market pending entry;
- BO Short market pending entry.

Each positive case now runs:

```text
source on_bar producer
-> exact source semantic/private export
-> canonical restart package
-> strict package bytes
-> source runtime drop
-> strict package decode
-> fresh runtime semantic set_state
-> runtime-private apply
-> broker-truth bootstrap
-> riskgate injection
-> runtime-state-restored callback
-> Stage 5C warmup continuation
```

The result localizes ownership:

- raw semantic `set_state(...)` is a placeholder layer and is not the owner of
  exact pending-entry shape;
- runtime-private apply restores the exact MR/BO pending-entry owner, side,
  reason, entry style, request id, target quantity, stop/take and partial timer
  before broker bootstrap and before the restored callback;
- incomplete MR stop/take and owner/side/reason mismatch fail closed after the
  canonical package boundary;
- no `hybrid_intraday_runtime.rs` source correction was required for r3a.

One existing policy remains visible for the later full r3 matrix: after the
restored callback, flat/no-working-order pending tails are cleared by the
existing boot-stale cleanup policy. r3a therefore proves exact shape before the
callback and records the callback policy separately; it does not authorize
runtime-live or active-order ownership mapping.

## Required next micro-stage

The earlier hypothesis below is retained as historical context. It is superseded
by the r3a executable result above: do not patch `set_state(...)` unless a later
accepted test proves a new failure after runtime-private apply.

Before continuing full r3 closure, the next step is to resume r3 proper:

```text
Stage 5D-final-restart-r3 — executable full matrix continuation
```

Scope:

- expand the r3a source-produced pending-entry proof into the full mandatory
  positive package matrix;
- build the real durable crash/checkpoint simulator;
- implement the full package-negative matrix;
- add checked-in pinned golden vectors;
- keep all transport/live surfaces closed.

## Non-goals

This discovery does not:

- close Stage 5D final restart;
- start Stage 5E;
- authorize Redis, FINAM, transport, dispatch, runtime-live or broker execution;
- replace the r3 assignment with a documentation-only closure.
