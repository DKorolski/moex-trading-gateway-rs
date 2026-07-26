# Stage 5E-b3c-impl-r1 — trusted-source and clock repair

Baseline: `95861577ce3acc11963104bb5a313a82f6f82bdb`.

Stage 5E-b3b-r2 is immutable: `Stage5eBoundScheduleWindowForObservedLiveBar`
remains an opaque, linear receipt. The original b3b core is still hash-pinned
byte-for-byte: the predecessor checker reconstructs its historical region only
after removing one exact, separately pinned b3c nested region.

`private-no-io-v1` remains nested inside the existing private
`schedule_window_evidence` module. That gives it the only safe way to consume
the opaque b3b receipt without widening b3b visibility, re-exporting its
strategy/recovery ownership, or making an external construction route.

The JSON inventory is the normative contract. It freezes exact field schemas,
source authorities, construction seals, transition input/output, blocker
taxonomy and expected provenance case count. This Markdown file is its human
projection; any contradiction with the inventory is invalid.

## Implemented: private producers and partial conjunctive plumbing

`b3c_evidence` consumes three opaque upstream capabilities and produces exactly
three non-`Clone`, non-`Copy`, non-serializable receipts:

- `Stage5eFreshOpenSessionEvidence` only from accepted Stage 4 schedule
  evidence and the explicitly broker-normalized b3b TradableOpen window;
- `Stage5eCalendarEligibilityEvidence` only from the validated broker-normalized
  instrument schedule that contains that exact TradableOpen window;
- `Stage5eMarketSequenceEvidence` only from an explicitly named private
  `UnverifiedMarketSequenceSource`. It is consistency-checked plumbing, **not**
  an accepted Stage 5C authority and cannot justify trusted eligibility.

The producer code has no visibility beyond its nested private module and no
path to Redis, FINAM, transport, dispatch, runtime-live, strategy callbacks or
intent construction. Its exact source region is pinned by the inventory and
checked together with negative mutations.

The bridge now consumes all four linear inputs:

```text
opaque b3b bound schedule/window receipt
+ fresh Open session receipt
+ trading calendar receipt
+ final, gap-free market-sequence receipt
+ production clock captured inside the transition
-> opaque combined no-I/O eligibility receipt
```

It must revalidate every evidence expiry; rejects a clock before any
observation, an expired b3b schedule, a future b3b bar,
non-Open/non-trading/non-final/gapped evidence, and mismatches in full
instrument identity, venue/day, schedule fingerprint, event-key fingerprint
or continuation epoch. A block returns every consumed input unchanged; success
owns all inputs and exposes only zero callback/intent diagnostics.

## Three sealed evidence contracts

`Stage5eFreshOpenSessionEvidence` is distinct from b3b schedule-window
evidence, but the current source does **not** retain a broker dynamic
`BrokerMarketSessionState::Open`. It is only a checked projection of accepted
Stage 4 schedule evidence plus the b3b TradableOpen window. It must not be
read as proof that the market session is still `Open` at continuation.

`Stage5eCalendarEligibilityEvidence` proves one broker-sourced trading-day
classification: full `InstrumentId`/venue, trading day, calendar source and
version/fingerprint, early-close/special-day policy, `observed_at` and
`expires_at`. Calendar eligibility must never be inferred from weekday or a
timestamp.

`Stage5eMarketSequenceEvidence` currently proves only internal field
consistency for a private unverified source. The required future authority is
the accepted Stage 5C canonical-history to final-live semantic-bar sequence:
full `InstrumentId`, timeframe, finality/provenance, source epoch and
fingerprint, previous canonical close boundary, gap classification,
`observed_at` and `expires_at`. Market-gap status must come from the accepted market-data
recovery/aggregation contour once that authority extension exists, never from a
caller boolean or a simple timestamp-delta rule.

## Binding contract

The private no-I/O eligibility receipt consumes, rather than copies:

```text
Stage5eBoundScheduleWindowForObservedLiveBar
+ Stage5eFreshOpenSessionEvidence
+ Stage5eCalendarEligibilityEvidence
+ Stage5eMarketSequenceEvidence
+ production clock captured inside the transition
-> opaque Stage5eBoundSessionCalendarSequenceForObservedLiveBar
```

All receipts bind the same full `InstrumentId`, event-key fingerprint, venue
and trading-day context; b3b schedule fingerprint is preserved. Continuation
revalidates every evidence expiry and rejects a clock earlier than any
`observed_at`. It also rejects a future b3b bar, a sequence gap/epoch mismatch,
calendar non-trading day and schedule expiry. The current shared
`"epoch-1"` value is a placeholder, not source-derived lineage.

The result must retain the mandatory Stage 5C strategy and recovery ownership
carried by the b3b receipt. Success is linear and must not return its unbound
inputs. A recoverable blocker must return every consumed input unchanged.

## Sealing and freshness

The existing b2 session observation cannot be used as the future continuation
authority by itself: it records an observed bar close but has no
instrument-bound identity or revalidation lifetime. The implementation must
therefore introduce a new sealed, instrument-bound session evidence receipt;
it must not weaken or repurpose the frozen b2 receipt.

The eligibility continuation revalidates b3b schedule expiry at its own clock.
It also proves session, calendar and market-sequence evidence fresh at that
clock. Calendar and market-sequence inputs are explicit receipts; inferred
exchange calendar policy, raw timestamps, and a free-form boolean are
forbidden.

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

The implemented additive region keeps the b3b core hash unchanged after exact
region removal and proves `callback_count == 0` and `intent_count == 0` for
success and blocked-retry paths. It remains a no-I/O facade only.

## Authority gap and separately reviewed extension

The current B3C receipt is **not trusted eligibility**. Before any
callback-adjacent work, a separately reviewed additive Stage 4/Stage 5C
source-authority freeze extension must provide:

- a source-owner sealed dynamic `Open` session receipt;
- a source-owner sealed accepted Stage 5C market-sequence receipt;
- source-derived continuation lineage rather than a string placeholder;
- a production fail-closed rule: unverified sequence input is test-only and
  cannot produce an authoritative receipt.

That extension may not alter B3C, Stage 5C or Stage 4 production source until
its own freeze contract is reviewed. It does not open callbacks, strategy state
mutation, intent construction, Redis, FINAM, transport, dispatch, runtime-live
or broker execution.

## Superseded test enclave after Stage 5E-b3c R6

The original `private-no-io-v1` enclave remains hash-pinned as legacy,
test-only evidence. It is not production-authoritative. The accepted R6
production route is the separately sealed
`schedule_window_evidence::b3c_evidence` transition and consumes only the
owner-issued Stage 5C/B3B receipt chain.
