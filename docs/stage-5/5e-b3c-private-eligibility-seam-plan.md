# Stage 5E-b3c — private eligibility seam design

Baseline: `95861577ce3acc11963104bb5a313a82f6f82bdb`.

Stage 5E-b3b-r2 is immutable: `Stage5eBoundScheduleWindowForObservedLiveBar`
remains an opaque, linear receipt, and its sealed construction boundary remains
hash-pinned. This slice adds no Rust runtime code. It freezes the sole safe
extension shape before any eligibility implementation is attempted.

## Required next construction

The future private no-I/O eligibility receipt must consume, rather than copy:

```text
Stage5eBoundScheduleWindowForObservedLiveBar
+ instrument-scoped fresh Open-session evidence
+ calendar/market-sequence receipt
+ continuation-time clock
-> opaque Stage5ePrivateEligibleObservedLiveBar
```

The result must retain the mandatory Stage 5C strategy and recovery ownership
carried by the b3b receipt. Success is linear and must not return its unbound
inputs. A recoverable blocker must return every consumed input unchanged.

## Sealing and freshness

The existing b2 session observation cannot be used as the future continuation
authority by itself: it records an observed bar close but has no
instrument-bound identity or revalidation lifetime. The implementation must
therefore introduce a new sealed, instrument-bound session evidence receipt;
it must not weaken or repurpose the frozen b2 receipt.

The eligibility continuation must revalidate b3b schedule expiry at its own
clock. It must also prove the session evidence is fresh at that clock. Calendar
and market-sequence inputs must be explicit receipts; inferred exchange
calendar policy, raw timestamps, and a free-form boolean are forbidden.

The existing binding fingerprint remains an event-key identity
(`InstrumentId + bar close + schedule fingerprint`). It is not upgraded to a
full OHLCV digest in this slice. Any audit/persistence or differential-replay
consumer must separately authorize a canonical accepted-bar payload digest.

## Still prohibited

This design is not callback eligibility. It must not add:

- `on_broker_bar` or strategy-state mutation;
- intent construction or an intent sink;
- Redis, FINAM, transport, dispatch, runtime-live or broker execution;
- calendar inference, market-gap inference or autonomous loops.

The first implementation after this design must add a new additive region,
keep the b3b core hash unchanged, and prove `callback_count == 0` and
`intent_count == 0` for success, block and retry paths.
