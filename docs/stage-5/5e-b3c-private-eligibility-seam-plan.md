# Stage 5E-b3c-r1 — exact private eligibility evidence contract

Baseline: `95861577ce3acc11963104bb5a313a82f6f82bdb`.

Stage 5E-b3b-r2 is immutable: `Stage5eBoundScheduleWindowForObservedLiveBar`
remains an opaque, linear receipt, and its sealed construction boundary remains
hash-pinned. This slice adds no Rust runtime code. It freezes the sole safe
extension shape before any eligibility implementation is attempted.

The JSON inventory is the normative contract. This Markdown file is its human
projection; any contradiction with the inventory is invalid.

## Three sealed evidence contracts

`Stage5eFreshOpenSessionEvidence` is distinct from b3b schedule-window
evidence. It proves that the market session is still `Open` at continuation:
full `InstrumentId`, venue/MIC, source snapshot epoch and fingerprint,
`observed_at`, `expires_at`, and state. Missing, unavailable or unknown source
and non-Open state are blocking.

`Stage5eCalendarEligibilityEvidence` proves one broker-sourced trading-day
classification: full `InstrumentId`/venue, trading day, calendar source and
version/fingerprint, early-close/special-day policy, `observed_at` and
`expires_at`. Calendar eligibility must never be inferred from weekday or a
timestamp.

`Stage5eMarketSequenceEvidence` proves the exact accepted bar identity:
full `InstrumentId`, timeframe, finality/provenance, source epoch and
fingerprint, previous canonical close boundary, gap classification,
`observed_at` and `expires_at`. Market-gap status must come from the accepted
market-data recovery/aggregation contour, never from a caller boolean or a
simple timestamp-delta rule.

## Required next construction

The future private no-I/O eligibility receipt must consume, rather than copy:

```text
Stage5eBoundScheduleWindowForObservedLiveBar
+ Stage5eFreshOpenSessionEvidence
+ Stage5eCalendarEligibilityEvidence
+ Stage5eMarketSequenceEvidence
+ continuation-time clock
-> opaque Stage5eBoundSessionCalendarSequenceForObservedLiveBar
```

All receipts must bind the same full `InstrumentId`, bar identity, venue and
trading-day context; b3b schedule fingerprint is preserved. Continuation must
revalidate every evidence expiry and reject a clock earlier than any
`observed_at`. It must also reject a future bar, a sequence gap/epoch mismatch,
calendar non-trading day and schedule expiry.

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

This receipt is explicitly `callback_ready=false` and `execution_ready=false`.
It is not callback eligibility. It must not add:

- `on_broker_bar` or strategy-state mutation;
- intent construction or an intent sink;
- Redis, FINAM, transport, dispatch, runtime-live or broker execution;
- calendar inference, market-gap inference or autonomous loops.

The first implementation after this design must add a new additive region,
keep the b3b core hash unchanged, and prove `callback_count == 0` and
`intent_count == 0` for success, block and retry paths.
