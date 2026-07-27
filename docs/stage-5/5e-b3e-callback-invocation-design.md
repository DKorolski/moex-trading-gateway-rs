# Stage 5E-b3e — authorized paper callback and private intent-escrow design

## Status and baseline

This is the governance-only B3E-r2 closure. Its accepted design predecessor is:

```text
06107da3bf5809e34504f740e5c260b29a315b9c
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

The B3D issue preflight is not reusable at callback time. The invocation owner
must issue a distinct:

```text
Stage5eB3eNestedPreflightSeal
```

and borrow exactly once:

```text
Stage5eBoundSessionCalendarSequenceForObservedLiveBar::
    borrow_for_authorized_callback_preflight(
        Stage5eB3eNestedPreflightSeal
    ) -> Stage5eB3eNestedPreflight<'_>
```

The nested preflight seal has one constructor and one call site, both inside
`invoke_stage5e_authorized_paper_callback`. It is a different Rust type from
`Stage5eCallbackAuthorityIssueSeal` and cannot be converted from it.

The borrowed, non-decomposable view carries only this immutable proof vector:

```text
full InstrumentId
accepted semantic-bar identity
accepted bar close timestamp
B3B event-key fingerprint
B3C continuation binding ID
schedule-window identity fingerprint
sequence identity fingerprint
B3C bound_at
B3C effective_observed_at
B3C effective_expires_at
```

It exposes no strategy, recovery receipt, accepted bar, schedule object, raw
field getter, or consume method. The view remains borrowed until every
callback-time authority comparison has passed; B3C consumption happens only
afterward.

## Exact module and consume topology

The future implementation is owned by:

```text
orchestrator:
  strategy_runtime_core::stage5e_no_io_lifecycle::callback_authority

nested B3C ownership bridge:
  strategy_runtime_core::stage5e_no_io_lifecycle::
    schedule_window_evidence::b3c_evidence

Stage 5C callback material owner:
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
Stage5eStage5cAuthorizedCallbackMaterial
Stage5eAuthorizedCallbackAuditLineage
```

The nested B3C consume bridge cannot access Stage 5C private fields directly.
It invokes exactly one Stage 5C-owned bridge:

```text
consume_stage5c_for_authorized_callback(
    HybridIntradayRuntimeStrategy,
    Stage5cPendingRecoveryReceipt,
    Stage5cAcceptedSemanticBar,
    Stage5cB3eCallbackMaterialSeal,
    callback_now
) -> Stage5eStage5cAuthorizedCallbackMaterial
```

`Stage5cB3eCallbackMaterialSeal` is defined and constructed only in
`stage5c_paper_host`. Its sole issuer is a private Stage 5C issuer function;
that issuer has one source call site, the nested B3C consume bridge. The seal
cannot be forged from `Stage5eCallbackInvocationSeal`, either B3E nested seal,
or `Stage5eCallbackAuthorityIssueSeal`.

`consume_stage5c_for_authorized_callback` is the sole Stage 5C materialization
entry. It has no raw-admission or raw-bar overload. It consumes all three
linear inputs and returns one opaque:

```text
Stage5eStage5cAuthorizedCallbackMaterial {
    strategy: HybridIntradayRuntimeStrategy,
    recovery_receipt: Stage5cPendingRecoveryReceipt,
    callback_input: HybridRuntimeCallbackInput<HybridRuntimeBarEvent>,
    attribution_snapshot: Stage5ePreCallbackAttributionSnapshot,
    retained_bar_metadata: Stage5eAcceptedBarSettlementMetadata,
}
```

The bridge itself:

1. resolves the accepted admission only through the owned recovery receipt;
2. validates admission instrument/tick against the owned accepted bar;
3. records pre-callback position;
4. creates the cleanup-attribution snapshot from pre-callback strategy state;
5. retains exact bar metadata before moving the accepted bar into callback
   input;
6. builds the exact context vector using `callback_now`;
7. moves the original accepted bar into the callback input.

`Stage5eAcceptedBarSettlementMetadata` contains exactly:

```text
accepted_bar_close_ts
accepted_bar_origin = Live
execution_eligible = true
accepted_semantic_bar_identity
```

The material type has one constructor in the Stage 5C bridge and one consumer
in the authority orchestrator. It has no `Clone`, serialization, generic
`into_parts`, raw strategy/admission/bar/ledger getter, or alternate builder.

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

The nested bridge moves the existing Stage 5C material and audit lineage. It
must not clone/reconstruct them, widen field visibility, add generic
`into_parts`, or add raw getters. The blocked path never calls either consume
bridge.

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

through the sole Stage 5C-owned
`consume_stage5c_for_authorized_callback` materialization bridge. The
authority owner has no sibling builder. The context field vector and authority
sources are frozen as:

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

Before invoking the callback, the sole Stage 5C materialization bridge
creates:

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
borrow authority invocation preflight
→ borrow the distinct B3C nested callback preflight
→ validate all callback-time checks
→ issue one private invocation seal
→ consume authority once
→ consume B3C ownership once
→ invoke the sole Stage 5C materialization bridge
→ create the sealed pre-callback attribution snapshot and exact callback input
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

## Exact payload-to-callback-to-escrow transfer

Every linear input has one named destination:

| Authorized payload input | Callback use | Escrow destination |
| --- | --- | --- |
| `material.strategy` | mutable callback receiver | `mutated_strategy` |
| `material.recovery_receipt` | untouched | `recovery_receipt` |
| `material.callback_input.payload` | moved once into callback | facts retained in `retained_bar_metadata` |
| `material.attribution_snapshot` | untouched | `attribution_snapshot` |
| `material.retained_bar_metadata` | untouched | exact accepted-bar settlement fields |
| `audit_lineage` | untouched | `audit_lineage` |
| `callback_now` | callback production clock | `callback_invoked_at` |
| owned callback authority ID | callback-time equality proof | `callback_authority_id` |
| exact callback result | converted by move once | exactly one `Stage5ePaperCallbackOutcome` |

The recovery receipt may not be dropped, replaced by its fields, or consumed
by the callback. The accepted bar payload may be consumed by the callback only
after its settlement metadata has been retained. No row may disappear, gain a
second owner, or be reconstructed after callback.

The future:

```text
Stage5ePaperCallbackResultEscrow
```

must be crate-private, opaque, non-serializable, non-cloneable, and
non-persistable. It owns:

```text
mutated_strategy: HybridIntradayRuntimeStrategy
recovery_receipt: Stage5cPendingRecoveryReceipt
audit_lineage: Stage5eAuthorizedCallbackAuditLineage
attribution_snapshot: Stage5ePreCallbackAttributionSnapshot
accepted_bar_close_ts
accepted_bar_origin = Live
execution_eligible = true
accepted_semantic_bar_identity
callback_invoked_at
callback_authority_id
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
- exact Stage 5C material seal, sole issuer, sole bridge, and no raw getter
  proof;
- distinct B3C callback-preflight seal proof, including compile-fail reuse of
  the B3D issue seal;
- payload-to-callback-to-escrow transfer proof retaining the recovery receipt,
  callback authority ID, accepted-bar close/origin/eligibility/identity, and
  pre-callback attribution snapshot;
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
