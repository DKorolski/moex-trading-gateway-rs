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

## Required next micro-stage

Before continuing full r3 closure, open a narrow authorized stage, for example:

```text
Stage 5D-final-restart-r3a — source restore-shape freeze extension
```

Scope:

- patch only `HybridIntradayRuntimeStrategy::set_state(...)` pending-entry
  reconstruction;
- add a focused regression proving MR bracket pending-entry restore preserves
  reason, style, request id, stop and take;
- update the Stage 5D freeze manifest/checker provenance for this explicitly
  authorized source restore correction;
- keep all transport/live surfaces closed.

After that micro-stage is independently accepted, resume r3:

- executable full positive package matrix;
- real durable crash/checkpoint simulator;
- full package-negative matrix;
- checked-in pinned golden vectors;
- inventory rows tied to executable evidence.

## Non-goals

This discovery does not:

- close Stage 5D final restart;
- start Stage 5E;
- authorize Redis, FINAM, transport, dispatch, runtime-live or broker execution;
- replace the r3 assignment with a documentation-only closure.
