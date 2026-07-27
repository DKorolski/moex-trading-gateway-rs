# Stage 5E-b3e — authorized paper callback and private intent-escrow design

## Status and baseline

This is a design-only stage. Its immutable implementation predecessor is:

```text
93d365ae51f2f6ad94954782a27bc49857fe21ff
```

The predecessor provides the accepted private, process-local:

```text
Stage5eCallbackAuthorityReadyPaperStrategy
```

No Rust callback invocation, strategy mutation, intent construction, sink,
transport, Redis, FINAM, dispatch, runtime-live, or broker execution is added
by this stage.

## Purpose

The future implementation may cross the strategy callback boundary exactly
once, only from the accepted B3D authority receipt. The transition must:

1. capture a new production clock internally;
2. borrow a sealed, non-decomposable preflight view;
3. revalidate lifetime, chronology, authority ID, and all owned identities;
4. consume the authority receipt exactly once after successful preflight;
5. invoke `BrokerNeutralHybridStrategy::on_broker_bar` exactly once;
6. retain the mutated paper strategy and the complete callback result in a
   private escrow;
7. expose no intents to a sink, transport, dispatcher, or runtime-live host.

The callback and in-memory intent construction are one indivisible future
boundary because the existing callback returns:

```text
Result<Vec<BrokerNeutralHybridIntent>, HybridRuntimeCallbackValidationError>
```

## Sole future transition

The only permitted future entry point is:

```text
invoke_stage5e_authorized_paper_callback(
    Stage5eCallbackAuthorityReadyPaperStrategy
) -> Result<
    Stage5ePaperCallbackResultEscrow,
    Stage5eCallbackInvocationTerminalBlock
>
```

Production time must be captured inside this transition with `Utc::now()`.
A deterministic `_at` entry may exist only under `#[cfg(test)]`.

No overload may accept raw strategy state, a B3C receipt, a semantic bar,
Stage 5C recovered state, an authority ID, or caller-supplied production time.

## Invocation seal and borrowed preflight

The future implementation must define:

```text
Stage5eCallbackInvocationSeal
Stage5eCallbackInvocationPreflight<'a>
```

The invocation seal:

- is private and opaque;
- has exactly one constructor;
- is issued only inside
  `invoke_stage5e_authorized_paper_callback`;
- is consumed only by the private borrowed-preflight and linear-consume
  bridges on `Stage5eCallbackAuthorityReadyPaperStrategy`.

The preflight view is borrowed from the still-owned authority receipt. It may
expose immutable proof material only to the owner module and may not expose the
strategy, semantic bar, intents, or generic parts.

## Callback-time checks

Before authority ownership is consumed, the future transition must check:

```text
issued_at <= now
now <= authority_expires_at
accepted_bar_close_ts <= now.timestamp()
issued_at <= authority_expires_at
authority_expires_at == owned B3C effective_expires_at
full InstrumentId is complete
all frozen identity fields are present and non-zero
recomputed callback_authority_id == owned callback_authority_id
all receipt identity fields == identities recomputed/read from owned B3C
```

The ID must be recomputed with the accepted B3D domain, field order, canonical
instrument encoding, issuance time, and exact expiry. No grace period,
refresh, ledger lookup, persisted capability, surrogate ownership ID, or
caller assertion is permitted.

All checks happen before the callback. A failed check produces:

```text
Stage5eCallbackInvocationTerminalBlock
```

with one of these closed reasons:

```text
ClockBeforeAuthorityIssue
AuthorityExpired
AcceptedBarObservedInFuture
InvalidAuthorityChronology
InstrumentIdentityMissing
OwnedIdentityMismatch
CallbackAuthorityIdMismatch
```

The terminal block owns no reusable authority receipt and provides no retry,
refresh, reconstruction, or unbinding method. Canonical recovery starts again
from fresh accepted evidence.

## Exact linear callback boundary

After successful preflight only:

```text
borrow preflight
→ validate all callback-time checks
→ issue one private invocation seal
→ consume authority once
→ build the canonical paper callback input from owned state
→ call BrokerNeutralHybridStrategy::on_broker_bar exactly once
→ move every post-callback object into private escrow
```

The future implementation must use the broker-neutral callback directly. The
legacy public Stage 5C routes:

```text
apply_stage5c_semantic_bar
advance_stage5c_paper_loop_once
```

remain paper/oracle compatibility APIs and must not be used by the new Stage
5E invocation path.

Callback panic is not retryable and must not produce an escrow or reusable
authority. The design does not introduce `catch_unwind`.

## Private result escrow

The future:

```text
Stage5ePaperCallbackResultEscrow
```

must be crate-private, opaque, non-serializable, non-cloneable, and
non-persistable. It owns:

```text
mutated HybridIntradayRuntimeStrategy
complete recovery/B3C authority audit lineage
accepted bar and callback metadata
BrokerNeutralHybridCallbackResult
in-memory BrokerNeutralHybridIntent values when callback returned Ok
```

A callback validation error remains inside the escrow together with the
post-callback strategy object. It is not converted into a preflight blocker
and does not permit callback retry.

The escrow has no raw strategy getter, intent getter, iterator, `into_parts`,
serialization, debug dump, sink conversion, command conversion, send-capable
consumer, or execution-ready marker. Its only future consumer must be a
separate, reviewed paper validation/settlement transition.

## Required implementation evidence before opening the callback

The implementation stage is not authorized until a separate review accepts:

- one canonical
  `Stage4AcceptedPaperHostEvidence → B3C → callback authority` test;
- compile-fail construction, clone, extraction, and direct-callback tests;
- callback-time expiry and authority-ID mismatch tests;
- exact identity mismatch tests for every frozen field;
- blocked paths proving callback count `0` and intent count `0`;
- success proof with callback count exactly `1`;
- callback-error escrow proof retaining post-callback ownership;
- legacy Stage 5C route non-reachability;
- no escrow getter/sink/send/transport surface;
- source-bound negative provenance and heavy freeze gates.

## Closed surfaces

This design does not authorize:

```text
actual on_broker_bar invocation through the new path
strategy state mutation through the new path
in-memory intent construction through the new path
escrow validation or settlement
intent extraction or sink
executable intents
Redis
FINAM I/O
transport
dispatch
runtime-live
broker execution
autonomous event loop
schedule provider attachment
venue-calendar inference
```

Any implementation requires a separate accepted assignment and review.
