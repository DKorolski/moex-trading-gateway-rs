# Stage 5E-b3f — callback validation and paper-settlement escrow design

## Status and baseline

This is the B3F-r2 governance-only closure based on the conditionally accepted
B3F-r1 design:

```text
88204fc858a95a33ee1de2de01f297155594b101
```

The accepted B3E implementation baseline remains
`d04e02903a0a1984f66eecfcc0f412501b97d37c`. No Rust settlement
implementation is added here. The accepted B3E callback boundary remains the
only producer of `Stage5ePaperCallbackResultEscrow`.

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
escrow. Borrowed preflight does not consume ownership and returns exactly one
private decision:

```text
ProceedOk
Terminal(Stage5ePaperSettlementTerminalReason)
```

After either decision, the transition issues exactly one consume seal and
consumes the escrow. `ProceedOk` continues to the Stage 5C oracle;
`Terminal(reason)` immediately constructs the owning terminal receipt.

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
exact pre-callback attribution snapshot fields
exact recovery/admission fields
exact audit-lineage fields
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

After borrowed preflight returns either decision, the top-level transition
issues exactly one `Stage5ePaperSettlementConsumeSeal` and consumes the escrow
once. The consume path privately moves:

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

## Exact owner and escrow topology

The future implementation owner is exactly:

```text
strategy_runtime_core::stage5e_no_io_lifecycle::
    callback_authority::callback_settlement
```

`callback_settlement` is a private child module of the B3E escrow owner. It
owns both settlement seals, both receipt types, the preflight decision, and
the top-level transition. The B3E parent remains owner of the escrow and
callback outcome.

The only permitted escrow bridges are:

```text
Stage5ePaperCallbackResultEscrow::borrow_for_settlement_preflight(
    &self,
    seal: &Stage5ePaperSettlementPreflightSeal
) -> Stage5ePaperSettlementPreflight<'_>

Stage5ePaperCallbackResultEscrow::consume_for_settlement(
    self,
    seal: Stage5ePaperSettlementConsumeSeal
) -> Stage5ePaperSettlementPayload
```

Each method has one definition and one call site. The preflight view is
private to the child module, non-cloneable, non-serializable, and cannot
outlive the escrow borrow.

The consumed payload is private, opaque, non-cloneable, and owns exactly:

```text
mutated_strategy
recovery_receipt
audit_lineage
pre_callback_attribution_snapshot
retained_bar_metadata
callback_invoked_at
callback_authority_id
exactly one callback_outcome
```

The payload has one private consumer in the top-level transition. There are no
raw getters, public fields, generic parts, tuple conversion, alternate
constructor, or second consumer. The transfer is field-for-field; no field is
reconstructed or cloned.

The callback outcome is inspected during borrowed preflight only through the
parent-owned discriminant/count bridge requiring the preflight seal. It is
consumed only through the payload and consume seal. `Ok` moves one exact
intent vector; `ValidationError` moves one exact error value. Neither variant
is exposed outside `callback_authority::callback_settlement`.

## Exact Stage 5C settlement bridge

Stage 5C owns:

```text
Stage5cB3fSettlementSeal
Stage5eStage5cSettlementMaterial
Stage5eStage5cSettlementSuccess
Stage5eStage5cSettlementTerminalMaterial
```

The only bridge is:

```text
settle_stage5e_callback_escrow_material(
    material: Stage5eStage5cSettlementMaterial,
    seal: Stage5cB3fSettlementSeal
) -> Result<
    Stage5eStage5cSettlementSuccess,
    Stage5eStage5cSettlementTerminalMaterial
>
```

The seal is opaque, non-cloneable and non-convertible. Its sole issuer
requires a borrow of the still-owned `Stage5ePaperSettlementConsumeSeal`; its
only call site is immediately before the Stage 5C bridge.

The Stage 5C material owns the mutated strategy, recovery receipt, exact
intent vector, attribution snapshot and retained bar metadata. Inside the
bridge:

1. admission is resolved from the owned recovery receipt;
2. the cleanup ledger from the pre-callback snapshot is passed to
   `stage5cj_expected_generated_attribution_by_request_from_ledger`;
3. that exact map is passed to the accepted
   `stage5c_build_paper_intent_batch`;
4. no fallback map is constructed in Stage 5E;
5. success constructs the canonical `Stage5cSettledPaperStrategy`;
6. failure constructs one Stage 5C terminal material.

The terminal material retains mutated strategy, recovery receipt,
pre-callback attribution snapshot, retained bar metadata, the exact
`Stage5cIntentSettlementError`, and original intent count. The intent vector
is intentionally consumed by the canonical builder and is not recoverable,
logged, formatted, or copied into the terminal receipt.

The exact error mapping is:

| Stage 5C error | Stage 5E terminal reason |
| --- | --- |
| `TooManyIntents` | `IntentCapacityExceeded` |
| `MissingIntentClass` | `Stage5cIntentValidationFailed` |
| `InstrumentNamespaceMismatch` | `Stage5cIntentValidationFailed` |
| `InvalidQuantity` | `Stage5cIntentValidationFailed` |
| `InvalidPrice` | `Stage5cIntentValidationFailed` |
| `PriceNotTickAligned` | `Stage5cIntentValidationFailed` |
| `InvalidStopEnd` | `Stage5cIntentValidationFailed` |
| `ReplayIntentNotExecutable` | `PaperModeMismatch` |
| `MissingPendingRequest` | `Stage5cPendingRequestMismatch` |
| `RequestIdMismatch` | `Stage5cPendingRequestMismatch` |
| `DuplicateRequestId` | `Stage5cIntentValidationFailed` |
| `UnsupportedIntentAction` | `Stage5cIntentValidationFailed` |

`TooManyIntents` and `ReplayIntentNotExecutable` are impossible after a valid
preflight but remain fail-closed terminal mappings. No generic `From`
implementation or wildcard mapping is allowed.

## Canonical proof and settlement identities

B3F-r1 removes invented aggregate recovery/snapshot identity values from the
preflight. It exact-compares the owned source fields named in the preflight
contract. The audit commitment and final settlement identity use SHA-256 with
domain-separated, versioned canonical encoding.

Canonical scalar encoding is:

```text
u8/bool: one byte
i64 bar timestamp: signed big-endian eight bytes
DateTime<Utc>: signed big-endian Unix seconds + big-endian u32 nanoseconds
fixed digest: exact 32 bytes
UUID StrategyRequestId: exact 16 network-order bytes
string: u32 big-endian byte length + UTF-8 bytes
optional value: 0x00 or 0x01 + encoded value
Exchange: 0x01 Moex; 0x7f Other + encoded string
Market: 0x01 Futures, 0x02 Options, 0x03 Stocks, 0x04 Currency,
        0x05 Funds, 0x7f Other + encoded string
schedule classification: 0x01 Contiguous;
                         0x02 ApprovedNonTradableBoundary + exact digest
InstrumentId: symbol, optional venue_symbol, Exchange, Market in that order
vector: u32 big-endian element count + ordered encoded elements
```

Audit commitment domain:

```text
stage5e-b3f-audit-commitment-v1\0
```

It binds, in order, schedule identity, sequence classification, optional
boundary fingerprint, sequence identity and chronology, B3B event key and
chronology, B3C continuation binding and chronology, callback authority ID
and chronology, full instrument ID, accepted semantic-bar identity, B3B/B3C
and sequence fingerprints, owned instrument, and owned bar identity.

Settlement identity domain:

```text
stage5e-b3f-settlement-identity-v1\0
```

It binds, in order:

```text
callback authority ID
callback invocation timestamp
accepted semantic-bar identity
strategy ID
account ID
full InstrumentId
accepted bar close timestamp
Stage 5C batch state fingerprint
ordered StrategyRequestIds
intent count as u8
audit commitment
```

Constant fingerprints, debug/serde hashes, callback-authority aliases, sorted
request IDs, lossy symbol-only identity, native-endian integers, and omitted
optional tags are forbidden.

## Exact receipt topology

Both receipts are owned by
`callback_authority::callback_settlement`, are crate-private opaque types with
private fields, and forbid `Debug`, `Clone`, `Copy`, `Default`, `From`, `Into`,
`Serialize`, and `Deserialize`.

The success receipt is constructed only by:

```text
construct_stage5e_validated_paper_settlement_receipt(
    Stage5eStage5cSettlementSuccess,
    audit_lineage,
    callback_invoked_at,
    callback_authority_id,
    settlement_identity,
    Stage5ePaperSettlementSuccessSeal
) -> Stage5eValidatedPaperSettlementReceipt
```

The success seal has one private constructor and one call site. The canonical
`Stage5cSettledPaperStrategy` is a private field. The receipt exposes no
`settled()`, `into_settled()`, batch, intent, request-ID, iterator, generic
parts, deref, borrow, or conversion surface despite the public inspection
methods on the wrapped Stage 5C type.

The terminal receipt is constructed only by:

```text
construct_stage5e_paper_settlement_terminal_receipt(
    exact surviving post-callback ownership,
    reason,
    optional exact Stage5cIntentSettlementError,
    original intent count,
    audit commitment,
    Stage5ePaperSettlementTerminalSeal
) -> Stage5ePaperSettlementTerminalReceipt
```

The terminal seal has one private constructor and one call site. The receipt
has no raw error formatter, strategy/recovery/intent getter, reusable escrow,
generic parts, retry capability, sink conversion, or second constructor.
Sole future consumers of both receipts remain deferred to a separate reviewed
stage.

## R2 exact Stage 5C input construction

The Stage 5C owner defines:

```text
Stage5cB3fSettlementMaterialSeal

issue_stage5c_b3f_settlement_material_seal(
    consume_capability: &Stage5ePaperSettlementConsumeSeal
) -> Stage5cB3fSettlementMaterialSeal
```

The issuer is `pub(crate)` only to cross the sibling boundary, has one
definition and one call site, and cannot issue without borrowing the
still-owned Stage 5E consume capability. The seal has private fields, one
private constructor, no traits or conversions, and one consumer.

After the escrow payload has classified its exact `Ok` outcome, the
callback-settlement owner calls exactly once:

```text
construct_stage5e_stage5c_settlement_material(
    mutated_strategy,
    recovery_receipt,
    pre_callback_attribution_snapshot,
    retained_bar_metadata,
    exact_intent_vector,
    original_intent_count,
    Stage5cB3fSettlementMaterialSeal
) -> Stage5eStage5cSettlementMaterial
```

The function is owned by `stage5c_paper_host`, is `pub(crate)` solely for this
bridge, and has one definition and one call site. The returned type is
`pub(crate)` opaque with private fields. It owns exactly the seven arguments
except the consumed seal. Its sole consumer is
`settle_stage5e_callback_escrow_material`.

No raw constructor, public field, tuple, generic parts, scalar overload,
alternate material, or second call site is permitted. Audit lineage,
callback timestamp, callback authority ID, and audit commitment remain in the
Stage 5E settlement payload; they never enter Stage 5C material.

## R2 exact Stage 5C success return

The Stage 5C success type privately owns the canonical
`Stage5cSettledPaperStrategy`. Before that strategy is moved, Stage 5C creates
one borrowed:

```text
Stage5eStage5cSettlementSuccessProof<'a> {
    strategy_id,
    account_id,
    full_instrument_id,
    accepted_bar_close_timestamp,
    batch_state_fingerprint,
    ordered_strategy_request_ids,
    intent_count_u8,
}
```

The view borrows the canonical batch, is non-decomposable, and has no public
fields, getters, serialization, clone, or conversion. It is created only by:

```text
Stage5eStage5cSettlementSuccess::borrow_identity_proof(
    &self,
    &Stage5cB3fSuccessProofSeal
) -> Stage5eStage5cSettlementSuccessProof<'_>
```

The proof seal is Stage 5C-private, has one constructor, and is issued inside
the sole success transfer method. That method is:

```text
Stage5eStage5cSettlementSuccess::construct_stage5e_success_receipt(
    self,
    audit_lineage,
    callback_invoked_at,
    callback_authority_id,
    accepted_semantic_bar_identity,
    audit_commitment,
    Stage5ePaperSettlementSuccessSeal
) -> Stage5eValidatedPaperSettlementReceipt
```

It has one definition and one call site. While `self` is still owned, it
borrows the proof and calls the named Stage 5E settlement-identity builder
once. After the borrow ends, it moves the canonical settled strategy and all
exact Stage 5E ownership into the sole receipt constructor. It exposes no
proof or settled-strategy getter and cannot transfer twice.

## R2 exact Stage 5C terminal return

The Stage 5C terminal type owns exactly:

```text
mutated_strategy
recovery_receipt
pre_callback_attribution_snapshot
retained_bar_metadata
exact Stage5cIntentSettlementError
original_intent_count
```

Its sole consumer is:

```text
Stage5eStage5cSettlementTerminalMaterial::
    construct_stage5e_terminal_receipt(
        self,
        audit_lineage,
        callback_invoked_at,
        callback_authority_id,
        audit_commitment,
        Stage5ePaperSettlementTerminalSeal
    ) -> Stage5ePaperSettlementTerminalReceipt
```

The method has one definition and one call site. It destructures its private
fields, calls the named exhaustive mapper exactly once, and transfers every
surviving field into the Stage 5E terminal constructor. There is no borrowed
or owned getter, `into_parts`, raw error export, second consumer, or retry
path.

## R2 named authority functions

The callback-settlement owner defines exactly these functions:

```text
construct_stage5e_b3f_audit_commitment(
    lineage: &Stage5eAuthorizedCallbackAuditLineage,
    seal: &Stage5eB3fAuditCommitmentSeal
) -> [u8; 32]

construct_stage5e_b3f_settlement_identity(
    proof scalar vector,
    callback_authority_id,
    callback_invoked_at,
    accepted_semantic_bar_identity,
    audit_commitment,
    seal: &Stage5ePaperSettlementSuccessSeal
) -> [u8; 32]

map_stage5c_settlement_error_exact(
    error: Stage5cIntentSettlementError,
    seal: &Stage5ePaperSettlementTerminalSeal
) -> Stage5ePaperSettlementTerminalReason
```

Each has one definition and one call site. The audit builder is called before
the payload is branched and uses the frozen audit encoding. The settlement
identity builder is called only from the Stage 5C success transfer while its
borrowed proof is alive. The mapper is called only from the Stage 5C terminal
transfer and uses one exhaustive 12-arm `match` with no `_` arm and no generic
conversion.

`Stage5eB3fAuditCommitmentSeal` is private to callback-settlement, has one
constructor and one call site, and is distinct from all preflight, consume,
success, terminal, and Stage 5C seals.

## R2 exact terminal ownership matrix

The private terminal receipt has a private tagged ownership representation
with three exact variants:

### Preflight terminal with `Ok` callback outcome

```text
reason
mutated_strategy
recovery_receipt
audit_lineage
pre_callback_attribution_snapshot
retained_bar_metadata
callback_invoked_at
callback_authority_id
opaque exact Ok callback outcome, including its still-owned intent vector
original_intent_count
audit_commitment
stage5c_error = None
```

### Preflight terminal with callback `ValidationError`

```text
reason = CallbackValidationError
mutated_strategy
recovery_receipt
audit_lineage
pre_callback_attribution_snapshot
retained_bar_metadata
callback_invoked_at
callback_authority_id
opaque exact callback validation error
original_intent_count = 0
audit_commitment
stage5c_error = None
```

### Stage 5C terminal after consumed `Ok` intent vector

```text
mapped reason
mutated_strategy
recovery_receipt
audit_lineage
pre_callback_attribution_snapshot
retained_bar_metadata
callback_invoked_at
callback_authority_id
callback_outcome = None because the exact vector was consumed by Stage 5C
original_intent_count
audit_commitment
stage5c_error = Some(exact error)
```

No variant drops audit lineage, callback error, strategy, recovery ownership,
attribution snapshot, bar metadata, chronology, authority ID, or count. The
private tag and fields cannot be inspected outside the receipt owner. No
variant is retryable or convertible to success.

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
