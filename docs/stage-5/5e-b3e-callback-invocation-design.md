# Stage 5E-b3e — authorized paper callback and private intent-escrow design

## Status and baseline

This is the B3E implementation-r1 repair review candidate built from the
conditionally accepted implementation:

```text
529d8e42946bb8bebad3cbf5e8fca2727dd95a07
```

The accepted B3E-r7 governance closure remains
`fe4c3f51e64e14ac5ef383b070ead81eb71586b5`.

The immutable Rust implementation predecessor remains:

```text
93d365ae51f2f6ad94954782a27bc49857fe21ff
```

That implementation provides the accepted private, process-local:

```text
Stage5eCallbackAuthorityReadyPaperStrategy
```

This implementation adds only the private process-local callback boundary:
successful invocation mutates the paper strategy once and moves its in-memory
callback result into an opaque escrow. It does not add settlement, extraction,
sink, transport, Redis, FINAM, dispatch, runtime-live, or broker execution.

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

## Exact invocation consume context

Immediately after `callback_now = Utc::now()` and successful borrowed
preflight, the callback-authority owner constructs exactly one linear:

```text
Stage5eB3eInvocationConsumeContext {
    callback_now,
    callback_authority_id,
    issued_at,
    effective_observed_at,
    authority_expires_at,
    full_instrument_id,
    accepted_semantic_bar_identity,
    b3b_event_key_fingerprint,
    b3c_continuation_binding_id,
    sequence_identity_fingerprint,
}
```

The type is owned by `callback_authority`, is `pub(crate)` opaque with private
fields, has one private constructor inside
`invoke_stage5e_authorized_paper_callback`, and is neither cloneable nor
serializable. It is moved once into authority consumption; no caller,
`issued_at`, bar timestamp, observation timestamp, or second clock may
substitute for `callback_now`.

The B3C consume bridge receives and consumes this exact context. Its only
permitted uses are:

```text
callback_now
  → exact Stage 5C materialization argument
  → exact payload callback_invoked_at
callback_authority_id + issued_at + effective_observed_at + authority_expires_at
  → Stage5eAuthorizedCallbackAuditLineage
callback_authority_id
  → exact payload callback_authority_id
identity fields
  → equality checks against owned B3C evidence and audit lineage
```

The Stage 5C callback input `strategy_now_ts_utc` and escrow
`callback_invoked_at` must derive from the same context value and compare
exactly equal. Context fields have no getters, partial extraction, default,
refresh, alternate constructor, or second consumer.

The exact sibling-access seam is:

```text
Stage5eB3eInvocationConsumeContext::consume_for_nested_b3c(
    self,
    nested_consume_capability: &Stage5eB3eNestedConsumeSeal
) -> Stage5eB3eNestedInvocationMaterial
```

The method is owned by `callback_authority`, has one call site in the B3C
consume bridge, destructures the private context internally, and calls exactly
one B3C-owned constructor:

```text
pub(crate) b3c_evidence::construct_nested_invocation_material(
    callback_now,
    callback_authority_id,
    issued_at,
    effective_observed_at,
    authority_expires_at,
    full_instrument_id,
    accepted_semantic_bar_identity,
    b3b_event_key_fingerprint,
    b3c_continuation_binding_id,
    sequence_identity_fingerprint,
    nested_consume_capability: &Stage5eB3eNestedConsumeSeal
) -> Stage5eB3eNestedInvocationMaterial
```

`Stage5eB3eNestedInvocationMaterial` is owned by `b3c_evidence`, is
`pub(crate)` opaque with private fields, has one capability-gated constructor
and one consumer inside `consume_for_authorized_callback`. B3C can therefore
read its own private material and pass the exact `callback_now` scalar to Stage
5C without exposing context fields or getters. The context and nested material
forbid `Clone`, `Copy`, `From`, `Into`, serialization, raw getters, generic
parts, alternate construction, and second consumption.

B3C destructures this private material exactly once. It sends `callback_now`
to the Stage 5C materialization and payload clock paths, then calls exactly
one B3C-owned bridge:

```text
b3c_evidence::construct_audit_lineage_from_consumed_nested_material(
    owned schedule/window identity,
    owned sequence classification/boundary identity/chronology,
    owned B3B evidence,
    owned B3C evidence,
    callback_authority_id,
    issued_at,
    effective_observed_at,
    authority_expires_at,
    full_instrument_id,
    accepted_semantic_bar_identity,
    b3b_event_key_fingerprint,
    b3c_continuation_binding_id,
    sequence_identity_fingerprint,
    nested_consume_capability: &Stage5eB3eNestedConsumeSeal
) -> Stage5eAuthorizedCallbackAuditLineage
```

The bridge is the only call site of the callback-authority-owned lineage
constructor. It transfers exact scalars by move; it does not pass the opaque
material across a sibling privacy boundary and exposes no seed, getter, tuple,
parts object, or second consumer.

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

The cross-module seal topology is exact:

| Seal | Owner | Visibility | Constructor | Authorized use |
| --- | --- | --- | --- | --- |
| `Stage5eB3eNestedPreflightSeal` | callback-authority | `pub(crate)` opaque, private fields | one private owner constructor | borrow B3C callback preflight once |
| `Stage5eB3eNestedConsumeSeal` | callback-authority | `pub(crate)` opaque, private fields | one private owner constructor | authorize B3C consume and Stage 5C material-seal issuance |
| `Stage5cB3eCallbackMaterialSeal` | Stage 5C paper host | `pub(crate)` opaque, private fields | one private Stage 5C constructor | authorize Stage 5C materialization once |
| `Stage5cB3eCallbackExecutionSeal` | callback-authority | `pub(crate)` opaque, private fields | one private owner constructor | authorize one material callback |
| `Stage5eEscrowConstructionSeal` | callback-authority | `pub(crate)` opaque, private fields | one private owner constructor | authorize one post-callback escrow construction |

No seal implements `Clone`, `Copy`, `Default`, serialization, conversion from
another seal, or a public constructor. Each constructor and authorized call
site count is exactly one.

The only linear authority consume method is:

```text
Stage5eCallbackAuthorityReadyPaperStrategy::consume_for_callback(
    Stage5eCallbackInvocationSeal,
    Stage5eB3eInvocationConsumeContext
) -> Result<
    Stage5eAuthorizedPaperCallbackPayload,
    Stage5eCallbackInvocationTerminalBlock
>
```

The authority consume method has one call site, inside
`invoke_stage5e_authorized_paper_callback`, and delegates nested B3C ownership
exactly once through:

```text
Stage5eBoundSessionCalendarSequenceForObservedLiveBar::
    consume_for_authorized_callback(
        Stage5eB3eNestedConsumeSeal,
        Stage5eB3eInvocationConsumeContext
    ) -> Result<
        Stage5eAuthorizedPaperCallbackPayload,
        Stage5eCallbackInvocationTerminalBlock
    >
```

`Stage5eB3eNestedConsumeSeal` has one constructor in the authority consume
method. No sibling module may construct it.

The payload owner is `callback_authority`. The type is crate-private, opaque
with private fields, non-serializable, non-cloneable, and linearly owns:

```text
Stage5eStage5cAuthorizedCallbackMaterial
Stage5eAuthorizedCallbackAuditLineage
callback_invoked_at
callback_authority_id
```

The B3C bridge cannot fill owner-private fields directly. It calls exactly
once:

```text
pub(crate) fn construct_stage5e_authorized_paper_callback_payload(
    material: Stage5eStage5cAuthorizedCallbackMaterial,
    audit_lineage: Stage5eAuthorizedCallbackAuditLineage,
    callback_invoked_at,
    callback_authority_id,
    nested_consume_capability: &Stage5eB3eNestedConsumeSeal
) -> Stage5eAuthorizedPaperCallbackPayload
```

The constructor belongs to `callback_authority`, is unusable without a borrow
of the still-owned nested consume capability, and has one definition and one
call site. The payload has one owner-private consumer:

```text
Stage5eAuthorizedPaperCallbackPayload::invoke_callback_once_in_authority(
    self,
    Stage5cB3eCallbackExecutionSeal
) -> Stage5eAuthorizedPostCallbackPayload
```

That consumer moves the material through its Stage 5C callback consumer and
returns an owner-private:

```text
Stage5eAuthorizedPostCallbackPayload {
    post_callback_material: Stage5eStage5cPostCallbackMaterial,
    audit_lineage: Stage5eAuthorizedCallbackAuditLineage,
    callback_invoked_at,
    callback_authority_id,
}
```

This post-callback payload is consumed exactly once by the sealed escrow
construction path. Neither payload has getters, generic `into_parts`, public
fields, alternate constructors, second consumers, or serialization.

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

`Stage5cB3eCallbackMaterialSeal` is defined in `stage5c_paper_host` as a
`pub(crate)` opaque type with private fields and one private constructor. Its
only issuance surface is:

```text
pub(crate) fn issue_stage5c_b3e_callback_material_seal(
    nested_consume_capability: &Stage5eB3eNestedConsumeSeal
) -> Stage5cB3eCallbackMaterialSeal
```

The issuer is callable across the sibling boundary but cannot issue without a
borrow of the still-owned nested consume capability. It has one call site,
inside the B3C `consume_for_authorized_callback` implementation, immediately
before that same nested capability is consumed. The Stage 5C seal cannot be
forged from `Stage5eCallbackInvocationSeal`, either B3E preflight seal, or
`Stage5eCallbackAuthorityIssueSeal`.

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

## Exact material callback execution seam

The Stage 5C-owned opaque material is consumed only by:

```text
pub(crate) Stage5eStage5cAuthorizedCallbackMaterial::
    invoke_authorized_callback_once(
        self,
        Stage5cB3eCallbackExecutionSeal
    ) -> Stage5eStage5cPostCallbackMaterial
```

`Stage5cB3eCallbackExecutionSeal` is a `pub(crate)` opaque type owned by the
callback-authority module, with private fields, one private constructor, and
one construction/call site inside
`invoke_stage5e_authorized_paper_callback` after all preflight and linear
consume steps succeed. No conversion exists from any issue, preflight,
material, or escrow seal.

The method is implemented in `stage5c_paper_host`, where the private material
fields are visible. It:

1. moves `callback_input` into
   `BrokerNeutralHybridStrategy::on_broker_bar(&mut strategy, callback_input)`
   exactly once;
2. converts the exact result by move into one `Stage5ePaperCallbackOutcome`;
3. returns one opaque post-callback material even for callback validation
   error;
4. never invokes a legacy Stage 5C apply/loop route.

The output is a `pub(crate)` opaque Stage 5C-owned type with private fields and
one constructor in that callback method:

```text
Stage5eStage5cPostCallbackMaterial {
    mutated_strategy: HybridIntradayRuntimeStrategy,
    recovery_receipt: Stage5cPendingRecoveryReceipt,
    attribution_snapshot: Stage5ePreCallbackAttributionSnapshot,
    retained_bar_metadata: Stage5eAcceptedBarSettlementMetadata,
    callback_outcome: Stage5ePaperCallbackOutcome,
}
```

It has one consumer, no `Clone`, serialization, raw getters, `into_parts`,
alternate constructor, callback retry, second callback consumer, or legacy
route conversion. A panic returns neither post-callback material nor reusable
input.

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

The lineage owner is `callback_authority`; the type is `pub(crate)` opaque
with private fields. B3C creates it through one exact owner constructor:

```text
pub(crate) fn construct_stage5e_authorized_callback_audit_lineage(
    owned_schedule_projection_and_window_identity,
    owned_sequence_classification_boundary_identity_and_chronology,
    owned_b3b_event_key_and_effective_chronology,
    owned_b3c_continuation_binding_and_chronology,
    callback_authority_id,
    issued_at,
    effective_observed_at,
    authority_expires_at,
    full_instrument_id,
    accepted_semantic_bar_identity,
    b3b_event_key_fingerprint,
    b3c_continuation_binding_id,
    sequence_identity_fingerprint,
    nested_consume_capability: &Stage5eB3eNestedConsumeSeal
) -> Stage5eAuthorizedCallbackAuditLineage
```

The constructor has one definition and one B3C bridge call site. B3C owns and
destructures the nested invocation material and passes this exact scalar
vector. The constructor binds all exact fields above and moves the lineage only into
`Stage5eAuthorizedPaperCallbackPayload`. There is no second constructor,
owner change, reconstruction, default, raw getter, generic parts, or alternate
destination.

Every nested-material field has exactly one frozen destination:

| Nested field | Destination |
| --- | --- |
| `callback_now` | Stage 5C materialization and payload `callback_invoked_at` |
| `callback_authority_id` | audit lineage and payload equality proof |
| `issued_at` | audit lineage issuance chronology |
| `effective_observed_at` | audit lineage effective observation chronology |
| `authority_expires_at` | audit lineage exact expiry |
| `full_instrument_id` | audit lineage full instrument identity |
| `accepted_semantic_bar_identity` | audit lineage accepted-bar identity |
| `b3b_event_key_fingerprint` | audit-lineage B3B equality binding |
| `b3c_continuation_binding_id` | audit-lineage B3C equality binding |
| `sequence_identity_fingerprint` | audit-lineage sequence equality binding |

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
MaterializationIntegrityMismatch
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

The materialization bridge returns:

```text
Result<
    Stage5eStage5cAuthorizedCallbackMaterial,
    Stage5eStage5cMaterializationTerminalBlock
>
```

An admission instrument/tick mismatch against the owned accepted bar yields
the opaque terminal reason `MaterializationIntegrityMismatch`. This is a
post-consume, fail-closed result: it returns no strategy, recovery receipt,
authority, or alternate success material. It cannot panic, retry, refresh, or
reconstruct the consumed authority chain. The callback count on this path is
zero.

The Stage 5C terminal type is frozen as:

```text
pub(crate) opaque Stage5eStage5cMaterializationTerminalBlock
owner = strategy_runtime_core::stage5c_paper_host
private zero-sized fields
sole constructor =
    construct_stage5e_stage5c_materialization_terminal_block
sole reason = MaterializationIntegrityMismatch
```

It has no raw reason getter and forbids `Debug`, `Clone`, `Copy`, `Default`,
`From`, `Into`, `Serialize`, and `Deserialize`. It owns and returns no
strategy, recovery receipt, authority, or callback material.

The exact propagation path is:

```text
consume_stage5c_for_authorized_callback
  -> Err(Stage5eStage5cMaterializationTerminalBlock)
B3C consume branch
  -> map_stage5c_materialization_terminal_to_callback_terminal(
         block,
         &Stage5eB3eNestedConsumeSeal
     )
  -> Err(Stage5eCallbackInvocationTerminalBlock {
         reason: MaterializationIntegrityMismatch
     })
nested consume
  -> same Err
authority consume
  -> same Err
invoke_stage5e_authorized_paper_callback
  -> same Err to caller
```

`map_stage5c_materialization_terminal_to_callback_terminal` is owned by
`callback_authority`, is `pub(crate)` only for the B3C sibling, consumes the
opaque zero-sized Stage 5C block, requires a borrow of the still-owned nested
consume seal, and has one definition and one B3C call site. Because the source
block represents exactly one reason, the mapper does not inspect private Stage
5C fields. Generic `From`/`Into`, alternate mapping, success mapping,
retryable mapping, panic, swallowing, or a second conversion are forbidden.

On this path callback count and intent count are both zero. The consumed
strategy, recovery receipt, and authority are never returned. The caller sees
only the redacted top-level reason and must rebuild the full chain from fresh
accepted evidence.

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
→ construct one invocation consume context from callback_now and outer authority
→ issue one private invocation seal
→ consume authority and invocation context once
→ consume B3C ownership and the same context once
→ invoke the sole Stage 5C materialization bridge
→ create the sealed pre-callback attribution snapshot and exact callback input
→ construct the owner-private authorized payload with the same callback clock
  and outer authority metadata
→ issue one callback-execution seal
→ consume authorized payload and call BrokerNeutralHybridStrategy::on_broker_bar exactly once inside Stage 5C
→ receive one owner-private authorized post-callback payload
→ issue one escrow-construction seal
→ consume authorized post-callback payload into the sole private escrow constructor
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
pub(crate) struct Stage5ePaperCallbackOutcome {
    inner: PrivateStage5ePaperCallbackOutcome,
}

private enum PrivateStage5ePaperCallbackOutcome {
    Ok(Vec<BrokerNeutralHybridIntent>),
    ValidationError(HybridRuntimeCallbackValidationError),
}
```

The wrapper and private enum are owned by `callback_authority`. The inner
variants are not crate-visible. The wrapper is produced by moving, not
cloning, the exact
`BrokerNeutralHybridCallbackResult`. There is no second result field and no
second intent vector.

Both `Stage5ePaperCallbackOutcome` and
`Stage5eStage5cPostCallbackMaterial` explicitly forbid `Debug`, `Clone`,
`Copy`, `Default`, `From`, `Into`, `Serialize`, and `Deserialize`. Intent
contents therefore cannot escape through formatting, conversion, cloning, or
serialization before the separately reviewed settlement capability exists.

The Stage 5C callback consumer may create it only through:

```text
pub(crate) fn move_stage5e_paper_callback_outcome(
    exact_result: BrokerNeutralHybridCallbackResult,
    execution_capability: &Stage5cB3eCallbackExecutionSeal
) -> Stage5ePaperCallbackOutcome
```

This is the sole move constructor and sole call site. It requires a borrow of
the still-owned callback-execution capability. There is no alternate
constructor, result clone, second representation, raw intent getter, or
crate-wide variant inspection.

Future inspection is reserved to one separately reviewed settlement seam:

```text
Stage5ePaperCallbackOutcome::consume_for_settlement(
    self,
    Stage5ePaperCallbackOutcomeInspectionSeal
) -> Stage5eSettlementOwnedCallbackOutcome
```

`Stage5ePaperCallbackOutcomeInspectionSeal` is owned by the future settlement
module, has private fields and no constructor in B3E. The settlement-owned
output is opaque outside that owner. Neither the seal, consume method, nor
settlement output is implemented or constructible in this stage.

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

## Exact post-callback escrow construction seam

The escrow owner is:

```text
strategy_runtime_core::stage5e_no_io_lifecycle::callback_authority
```

It defines a `pub(crate)` opaque `Stage5eEscrowConstructionSeal` with private
fields and one private constructor. The seal is constructed exactly once in
`invoke_stage5e_authorized_paper_callback`, only after one
`Stage5eAuthorizedPostCallbackPayload` has returned.

The owner-private authorized post-callback payload has exactly one consumer:

```text
Stage5eAuthorizedPostCallbackPayload::construct_result_escrow(
    self,
    seal: Stage5eEscrowConstructionSeal
) -> Stage5ePaperCallbackResultEscrow
```

Inside that owner-only method, the opaque Stage 5C post-callback material is
consumed exactly once by its previously frozen crate-private sibling bridge,
with the payload-owned audit lineage, callback time, authority ID, and seal.
That Stage 5C bridge remains the only call site of the escrow owner's sole
constructor:

```text
Stage5eStage5cPostCallbackMaterial::construct_result_escrow(
    self,
    audit_lineage: Stage5eAuthorizedCallbackAuditLineage,
    callback_invoked_at,
    callback_authority_id,
    seal: Stage5eEscrowConstructionSeal
) -> Stage5ePaperCallbackResultEscrow

construct_stage5e_paper_callback_result_escrow(
    mutated_strategy,
    recovery_receipt,
    audit_lineage,
    attribution_snapshot,
    retained_bar_metadata,
    callback_invoked_at,
    callback_authority_id,
    callback_outcome,
    Stage5eEscrowConstructionSeal
) -> Stage5ePaperCallbackResultEscrow
```

The constructor is `pub(crate)` solely for the Stage 5C sibling bridge, but is
unusable without the opaque seal. There is one constructor definition, one
Stage 5C source call site, and one authorized-post-payload consumer call site.
No escrow can be constructed from pre-callback material, without the
authorized post-callback payload, before callback, or without the seal.

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

## Implementation evidence in this review candidate

The callback remains usable only through the private type-state route. This
candidate supplies source-bound checks and Rust tests for:

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
- exact cross-module visibility, owner, private constructor, and one-call-site
  proof for all five seals;
- material callback consumer exactly-once proof, callback-error ownership
  proof, and compile-fail raw-field/`into_parts` access;
- post-callback material and escrow-construction seal proof, including
  before-callback and no-seal construction failures;
- invocation consume context sole constructor/consumer proof and exact
  callback-clock equality from Stage 5C input through escrow;
- outer authority ID/issued/observed/expiry transport into audit lineage;
- authorized payload owner-private constructor and sole pre/post-callback
  consumption proof, including nested-capability construction enforcement;
- callback outcome sole move-constructor and settlement-only inspection proof;
- context-to-B3C nested material capability, private-field, and linearity
  compile-fail proof including `Copy`, `From`, and `Into`;
- opaque outcome wrapper/private-inner-enum proof and no external variant
  construction or inspection;
- audit-lineage owner, exact field vector, sole capability constructor, one
  destination, and no reconstruction proof;
- exact Stage 5C post-callback sibling bridge method/signature and one-call
  proof;
- exact nested consume payload, single seal issuer, and single call-site proof;
- single callback-outcome ownership/no intent clone proof;
- blocked paths proving callback count `0` and intent count `0`;
- success proof with callback count exactly `1`;
- callback-error escrow proof retaining post-callback ownership;
- legacy Stage 5C route non-reachability;
- no escrow getter/sink/send/transport surface;
- source-bound negative provenance and heavy freeze gates.

## Closed surfaces after implementation

This implementation opens only:

```text
actual on_broker_bar invocation through the new path
strategy state mutation through the new path
in-memory intent construction through the new path
```

The result remains sealed in process-local escrow. This stage does not
authorize:

```text
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

Any settlement or external side effect requires a separate accepted assignment
and review.
