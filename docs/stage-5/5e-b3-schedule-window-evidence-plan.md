# Stage 5E-b3b-r2 — mandatory Stage 5C ownership in monotonic observed-bar binding

Baseline: `04431096e269daaf9715e253b2354b1ac8fcc3e8`.

This slice creates only the broker-neutral, no-I/O contract for a sealed,
instrument-scoped schedule window. The sole construction chain is:

```text
normalized schedule snapshot
→ validated opaque snapshot
→ sealed exact instrument-registry evidence
+ accepted Stage 4 schedule evidence
+ lifecycle clock
→ selected TradableOpen schedule window evidence
```

No independently constructed schedule definition may reach the mapper. The
registry bridge accepts only the exact instrument, canonical broker symbol,
venue MIC, board and registry-version tuple of the validated snapshot. A
canonical broker symbol is exactly `ticker@mic`: its ticker equals
`InstrumentId.symbol`, its MIC equals `venue_mic`, and the complete string
equals `InstrumentId.venue_symbol`.

Stage 4 and the normalized snapshot are **conjunctive independent evidence**:
Stage 4 supplies lifecycle/freshness acceptance while the normalized snapshot
supplies the exact tradable window. Both expiries are revalidated at the
mapping boundary. Stage 4 `checked_ts` and its derived schedule `observed_at`
must not be later than the lifecycle clock. Production mapping remains a later
separately reviewed broker adapter slice.

The window policy is inclusive: `open_from <= bar_close <= open_until`; mapper
validation requires `open_from < open_until`, and two intervals that share an
endpoint are rejected (`next.start <= previous.end`). The output is non-copyable,
private, carries lifecycle observation/expiry and a deterministic SHA-256
fingerprint over full instrument identity, broker symbol, MIC, board,
registry/source versions, raw and normalized payload hashes, canonical sessions,
selected window and Stage 4 identity. It preserves both independent observation
times plus their conservative effective maximum. Its encoding is tagged and
length-prefixed.

Callback, strategy mutation, intents, Redis, FINAM I/O, transport, dispatch,
runtime-live, autonomous loops and execution remain closed. Binding to an
observed bar is now the only addition in this slice.

## b3b private no-I/O binding

The only binding path consumes exactly these already-issued linear receipts:

```text
Stage5eScheduleWindowEvidence
+ Stage5eObservedLiveBarAfterHistory
+ admission-time LifecycleNow
→ Stage5eBoundScheduleWindowForObservedLiveBar
```

It never accepts raw instrument, venue, bar-close, window, fingerprint or
expiry from a caller. The receipt checks the full `InstrumentId`, requires the
observed bar close to remain inside the inclusive selected window, rejects a
future observed bar, and revalidates the evidence expiry at bind time. It
preserves the schedule fingerprint, full bar identity and the owned observed
receipt, therefore retaining the strategy/recovery state without invoking it.

Every binding blocker returns both linear inputs with its reason; a future
reviewed owner may obtain fresh schedule evidence and retry without state loss.
Callback count and intent count remain zero. This is still not b3b callback
eligibility, a production provider attachment, or an authorization to call
`on_broker_bar`.

The production binding path captures its lifecycle clock internally with
`Utc::now()`. The deterministic `_at` variant is test-only. Consumption also
rejects a clock that precedes the schedule evidence's effective observation,
so stale or rewound time cannot admit an expired window or future bar.

The successful bound receipt is monotonic: it has no unbinding API. Only a
blocked outcome may return the original receipts for a fresh-evidence retry.
The successful receipt additionally carries a tagged deterministic binding
fingerprint over the schedule fingerprint, full bar `InstrumentId`, bar close,
and a versioned binding-domain tag. This is an event-identity fingerprint, not
a replacement for a future full accepted-bar payload digest.

## b3b-r2 mandatory Stage 5C ownership repair

`Stage5eObservedLiveBarAfterHistory` owns non-optional
`HybridIntradayRuntimeStrategy` and `Stage5cPendingRecoveryReceipt`. It has
exactly one constructor, `from_stage5c_context`, which requires the private
`Stage5eNoIoBridgeSeal`; no empty-state, free, or alternate constructor exists.

The b3b consuming tests obtain their observed bar only through the canonical
test-only Stage 5C recovery path:

```text
Stage 5C admission → bootstrap → restore → history warmup
→ pending recovery → accepted live semantic bar
→ sealed Stage5eObservedLiveBarAfterHistory → b3b bind/retry
```

They compare the Stage 5C-owned strategy-state fingerprint and recovery-receipt
identity before binding, after a blocked retry, and after a successful bind.
Callback count and intent count remain zero throughout. The test helper is not
an observed-bar constructor: it invokes the sealed Stage 5C bridge and exists
only under `cfg(test)`.

## Stage 5E-b3c R6 additive implementation compatibility

The accepted R6 implementation keeps normalized sessions inside the schedule
owner and adds the sealed discrete expected-close-grid classifier. The
predecessor receipt remains no-I/O and non-executable; the updated region pin
recognizes only the reviewed additive source-authority bridge.
