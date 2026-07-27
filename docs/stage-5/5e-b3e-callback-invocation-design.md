# Stage 5E-b3e — authorized paper callback and private intent-escrow design

## Status and baseline

This is the governance-only B3E-r1 closure. Its accepted design predecessor is:

```text
5520ed1ef546bb9801dfa064311dbd0dac256ae4
```

The immutable Rust implementation predecessor remains:

```text
93d365ae51f2f6ad94954782a27bc49857fe21ff
```

That implementation provides the accepted private, process-local:

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

## Exact module and consume topology

The future implementation is owned by:

```text
orchestrator:
  strategy_runtime_core::stage5e_no_io_lifecycle::callback_authority

nested B3C ownership bridge:
  strategy_runtime_core::stage5e_no_io_lifecycle::
    schedule_window_evidence::b3c_evidence

pre-callback attribution snapshot owner:
  strategy_runtime_core::stage5c_paper_host
```

The only linear authority consume method is:

```text
Stage5eCallbackAuthorityReadyPaperStrategy::consume_for_callback(
    Stage5eCallbackInvocationSeal
) -> Stage5eAuthorizedPaperCallbackPayload
```

The authority consume method has one call site, inside
`invoke_stage5e_authorized_paper_callback`, and delegates nested B3C ownership
exactly once through:

```text
Stage5eBoundSessionCalendarSequenceForObservedLiveBar::
    consume_for_authorized_callback(
        Stage5eB3eNestedConsumeSeal
    ) -> Stage5eAuthorizedPaperCallbackPayload
```

`Stage5eB3eNestedConsumeSeal` has one constructor in the authority consume
method. No sibling module may construct it.

The payload is crate-private, opaque, non-serializable, non-cloneable, and has
one constructor in the nested B3C consume bridge. It linearly owns:

```text
HybridIntradayRuntimeStrategy
Stage5cPendingRecoveryReceipt
Stage5cAcceptedSemanticBar
Stage5ePreCallbackAttributionSnapshot
Stage5eAuthorizedCallbackAuditLineage
```

`Stage5eAuthorizedCallbackAuditLineage` owns, without raw getters:

```text
schedule projection and selected-window identity
sequence classification and optional boundary fingerprint
sequence identity, observed_at, and expires_at
B3B event-key fingerprint and effective chronology
B3C continuation binding, bound_at, effective_observed_at, effective_expires_at
callback authority ID, issued_at, and exact authority expiry
accepted semantic-bar identity and full InstrumentId
```

The nested bridge moves the existing strategy, recovery receipt, and accepted
semantic bar. It must not clone/reconstruct them, widen field visibility, add
generic `into_parts`, or add raw getters. The blocked path never calls this
consume bridge.

## Callback-time checks

Before authority ownership is consumed, the future transition must check:

```text
authority.effective_observed_at == owned B3C effective_observed_at
owned B3C effective_observed_at <= issued_at
issued_at <= callback_now
callback_now <= authority_expires_at
authority_expires_at == owned B3C effective_expires_at
accepted_bar_close_ts <= issued_at
accepted_bar_close_ts <= callback_now.timestamp()
full InstrumentId is complete
all frozen identity fields are present and non-zero
all receipt identity fields == identities recomputed/read from owned B3C
recomputed callback_authority_id == owned callback_authority_id
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

## Exact canonical callback input

The future transition constructs exactly:

```text
HybridRuntimeCallbackInput<HybridRuntimeBarEvent>
```

through one private sealed builder in the authority owner module. The context
field vector and authority sources are frozen as:

```text
strategy_id
  = accepted Stage5cPaperHostAdmission.strategy_id
request_namespace_account
  = accepted Stage5cPaperHostAdmission.account_id
instrument
  = accepted Stage5cPaperHostAdmission.target_instrument
tick_size
  = accepted Stage5cPaperHostAdmission.tick_size
trade_mode
  = HybridRuntimeTradeMode::Paper
paper_execution_mode
  = HybridRuntimePaperExecutionMode::LiveOnly
allow_live_orders
  = false
gateway_phase
  = HybridRuntimeGatewayPhase::LiveReady
position_qty
  = Some(pre-callback strategy.stage5c_current_position_qty())
event_ts_utc
  = exact accepted semantic-bar close_time_utc
strategy_now_ts_utc
  = callback_now.timestamp()
last_bar_ts_utc
  = Some(exact accepted semantic-bar close_time_utc)
payload
  = exact owned Stage5cAcceptedSemanticBar.bar moved once
```

The payload must be the accepted final `Live` M10 bar already carried by the
authority chain. It cannot be cloned from scalar fields, reconstructed,
substituted by schedule data, or supplied by the caller. Position is read
before callback mutation. `issued_at`, bar time, and B3C observation time must
not substitute for the callback production clock.

## Pre-callback attribution maturity snapshot

Before invoking the callback, the sealed nested consume bridge creates:

```text
Stage5ePreCallbackAttributionSnapshot
```

using the exact accepted Stage 5C algorithm:

```text
stage5cj_cleanup_attribution_ledger(
    Strategy::state(&pre_callback_strategy),
    accepted_admission.strategy_id()
)
```

The snapshot is created from the pre-callback strategy state, never the
mutated state. It is bound to the exact accepted:

```text
strategy_id
account_id
target InstrumentId
accepted semantic-bar identity
bar close timestamp
```

It owns one canonical representation of:

```text
broker order ID → HybridRuntimeAttribution
stop order ID → HybridRuntimeAttribution
pending entry HybridRuntimeAttribution
```

The type is crate-private, opaque, non-serializable, non-cloneable, and has no
ledger/map/raw attribution getters or sink conversion. It is moved into the
authorized callback payload and then into the result escrow. Its sole future
consumer is the separately reviewed settlement transition, which must use the
accepted Stage 5C expected-attribution algorithm.

## Exact linear callback boundary

After successful preflight only:

```text
borrow preflight
→ validate all callback-time checks
→ issue one private invocation seal
→ consume authority once
→ create the sealed pre-callback attribution snapshot
→ build the exact canonical paper callback input from owned state
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

The callback result has one ownership representation only:

```text
enum Stage5ePaperCallbackOutcome {
    Ok(Vec<BrokerNeutralHybridIntent>),
    ValidationError(HybridRuntimeCallbackValidationError),
}
```

The enum is produced by moving, not cloning, the exact
`BrokerNeutralHybridCallbackResult`. There is no second result field and no
second intent vector.

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
Stage5ePreCallbackAttributionSnapshot
exactly one Stage5ePaperCallbackOutcome
```

A callback validation error remains inside the escrow together with the
post-callback strategy object. It is not converted into a preflight blocker
and does not permit callback retry.

Intent-count enforcement remains the responsibility of the separately
reviewed settlement transition and must retain the accepted Stage 5C
`u8::MAX` limit. Until settlement, escrow is strictly process-local,
non-persisted, non-queued, and has no extraction or scheduling surface.

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
- full effective-observation → issuance → invocation → expiry chronology tests;
- exact identity mismatch tests for every frozen field;
- exact callback-context field-source tests and mutations;
- owned accepted-bar move/no-reconstruction proof;
- pre-callback attribution snapshot parity and post-callback substitution test;
- exact nested consume payload, single seal issuer, and single call-site proof;
- single callback-outcome ownership/no intent clone proof;
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
