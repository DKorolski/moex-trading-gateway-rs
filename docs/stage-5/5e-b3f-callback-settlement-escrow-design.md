# Stage 5E-b3f — callback validation and paper-settlement escrow design

## Status and baseline

This is a design-only stage based on the accepted Stage 5E-b3e
implementation-r1:

```text
d04e02903a0a1984f66eecfcc0f412501b97d37c
```

No Rust settlement implementation is added here. The accepted B3E callback
boundary remains the only producer of `Stage5ePaperCallbackResultEscrow`.

## Purpose

The next implementation may validate and settle one opaque callback escrow
into the already accepted Stage 5C paper-intent lifecycle. It must not create
a second intent validator, request-ID algorithm, attribution algorithm, or
paper batch representation.

The future transition is:

```text
validate_and_settle_stage5e_paper_callback_escrow(
    Stage5ePaperCallbackResultEscrow
) -> Result<
    Stage5eValidatedPaperSettlementReceipt,
    Stage5ePaperSettlementTerminalReceipt
>
```

The escrow is the sole input. There is no overload accepting raw strategy
state, a callback result, an intent vector, a recovery receipt, attribution,
bar metadata, identity scalars, or caller-created settlement authority.

## Borrowed preflight before consume

The settlement owner defines two distinct, opaque seals:

```text
Stage5ePaperSettlementPreflightSeal
Stage5ePaperSettlementConsumeSeal
```

The top-level transition is the only issuer of both seals. It first borrows a
non-decomposable `Stage5ePaperSettlementPreflight<'_>` from the still-owned
escrow. No ownership is consumed until all preflight checks pass.

The borrowed preflight carries only immutable proof material:

```text
callback outcome discriminant
intent count when the outcome is Ok
strategy ID
account ID
full InstrumentId
accepted semantic-bar identity
accepted bar close timestamp
accepted bar origin
execution eligibility
callback invocation timestamp
callback authority ID
pre-callback attribution snapshot identity
recovery/admission identity
audit-lineage identity
```

It exposes no mutable strategy, recovery receipt, raw intent vector, intent
iterator, generic parts, serialization, or sink conversion. The preflight seal
cannot be converted into the consume seal.

## Exact preflight rules

All of the following are required before the escrow is consumed:

1. the outcome is exactly one of `Ok` or `ValidationError`;
2. `Ok` contains at most `u8::MAX` intents, matching the accepted Stage 5C
   capacity limit;
3. accepted bar origin is `Live`;
4. accepted bar is execution-eligible;
5. admission is paper-only and live orders remain disabled;
6. strategy ID, account ID, full instrument ID, accepted semantic-bar
   identity, and bar close timestamp agree exactly across recovery admission,
   retained bar metadata, pre-callback attribution snapshot, and audit
   lineage;
7. callback invocation time is not earlier than the accepted bar close time
   and agrees with the retained B3E callback chronology;
8. callback authority ID and all frozen fingerprints are non-zero and agree
   with the accepted B3E lineage;
9. no intent has been extracted, cloned, serialized, persisted, queued, or
   offered to a sink.

The preflight may borrow each `Ok` intent only for the accepted Stage 5C
validator. It may not return intent references or a collection view.

## Callback validation-error policy

`ValidationError` is a terminal callback outcome, not an empty successful
batch and not a retry signal. It produces:

```text
Stage5ePaperSettlementTerminalReceipt {
    reason = CallbackValidationError,
    exact post-callback ownership,
    exact recovery ownership,
    exact audit lineage,
    no executable batch,
}
```

The callback is never repeated. The authority is never reconstructed.
Settlement does not convert the error to `Ok([])`, discard the mutated
strategy, or return a reusable escrow.

## One settlement consume

After successful borrowed preflight, the top-level transition issues exactly
one `Stage5ePaperSettlementConsumeSeal` and consumes the escrow once. The
consume path privately moves:

```text
mutated strategy
recovery receipt
pre-callback attribution snapshot
retained accepted-bar metadata
audit lineage
callback invocation timestamp
callback authority ID
exact callback outcome
```

There is no `into_parts`, alternate constructor, second consumer, rollback to
pre-callback state, or reusable capability after consumption.

## Stage 5C oracle reuse

For an `Ok` outcome, the implementation must invoke one crate-private,
seal-gated Stage 5C bridge. That bridge must reuse the accepted
`stage5c_build_paper_intent_batch` path and therefore retain:

- `u8::MAX` intent capacity;
- `validate_stage5cg_intent`;
- source-derived `StrategyRequestId`;
- exact pending request-ID checks;
- duplicate request-ID rejection;
- pre-callback cleanup attribution;
- state fingerprint and accepted paper-batch representation.

The Stage 5E code must not reimplement request-ID derivation, tick/price
validation, pending-state matching, attribution fallback, or batch building.
`ClientOrderId` may not replace `StrategyRequestId`.

The bridge receives the exact mutated strategy, recovery admission, retained
bar identity, exact intents, and pre-callback attribution snapshot. It returns
the canonical settled Stage 5C ownership or a terminal settlement failure.

## Success receipt

Successful settlement returns one private, opaque:

```text
Stage5eValidatedPaperSettlementReceipt
```

It linearly owns:

```text
canonical Stage5cSettledPaperStrategy
exact B3E audit lineage
callback invocation timestamp
callback authority ID
settlement identity fingerprint
```

The receipt proves that the callback ran once and its exact `Ok` result passed
the Stage 5C paper oracle. It is not executable, serializable, cloneable,
persistable, dispatch-ready, or sink-ready. It exposes neither raw intents nor
a command batch.

## Terminal receipt

Every failure after callback is terminal and returns one private, opaque:

```text
Stage5ePaperSettlementTerminalReceipt
```

Reasons are limited to:

```text
CallbackValidationError
IntentCapacityExceeded
IdentityMismatch
ChronologyMismatch
PaperModeMismatch
Stage5cIntentValidationFailed
Stage5cPendingRequestMismatch
Stage5cAttributionMismatch
```

The terminal receipt owns all surviving post-callback material. It is
non-retryable, non-cloneable, non-serializable, and has no strategy, intent,
escrow, or generic-parts extractor. A failure never returns the original
escrow and never permits a second callback or settlement attempt.

## Exactly-once boundary

The exactly-once guarantee remains process-local:

```text
one callback authority
→ one callback invocation
→ one result escrow
→ one borrowed settlement preflight
→ one settlement consume
→ one success or terminal receipt
```

Crash/restart persistence, durable idempotency, and recovery of an in-flight
escrow are explicitly deferred to a later separately reviewed stage.

## Closed surfaces

This design does not implement settlement and does not authorize:

```text
intent extraction or sink
executable intents
Redis
FINAM I/O
transport
dispatch
runtime-live
broker execution
durable persistence
crash/restart recovery
autonomous event loop
schedule provider attachment
venue-calendar inference
```

The next implementation stage requires a separate review and must add only
the private process-local settlement seam described above.
