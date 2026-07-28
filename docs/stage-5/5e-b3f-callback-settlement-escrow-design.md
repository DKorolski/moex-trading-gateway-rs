# Stage 5E-b3f — callback validation and paper-settlement escrow design

## Status and baseline

This is the B3F-r3 governance-only cross-contract correction based on the
rejected B3F-r2 design:

```text
ee23d6b4231c3c1483bcfacdb9189392183e2963
```

The accepted B3E implementation baseline remains
`d04e02903a0a1984f66eecfcc0f412501b97d37c`. No Rust settlement
implementation is added here. The accepted B3E callback boundary remains the
only producer of `Stage5ePaperCallbackResultEscrow`. B3F-r3 corrects the
linear-capability liveness, private Stage 5C preflight access, source-relation
matrix, canonical settlement path, terminal vector disposition, intent-count
binding, and terminal-reason producer topology before implementation.

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

Both are `pub(crate)` opaque types with private fields only because the
Stage 5C sibling bridges must name borrowed references to them; neither has a
crate-wide constructor or inspection surface. The top-level transition is the
only issuer of both seals. It first borrows a
non-decomposable `Stage5ePaperSettlementPreflight<'_>` from the still-owned
escrow. Borrowed preflight does not consume ownership and returns exactly one
private decision:

```text
ProceedOk
Terminal(Stage5ePaperSettlementTerminalReason)
```

After either decision, the transition issues exactly one stack-local consume
seal and borrows it for the sole escrow consume. The seal remains owned by the
transition. `ProceedOk` may later borrow the same seal once for the material
seal and once for the settlement seal; `Terminal(reason)` does neither and
immediately constructs the owning terminal receipt. Every branch drops the
consume seal before return.

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
6. strategy ID and account ID agree between recovery admission and the
   pre-callback attribution snapshot;
7. full `InstrumentId` agrees across recovery admission, attribution snapshot,
   audit `_full_instrument_id`, and audit `_owned_instrument`;
8. semantic-bar identity agrees across attribution snapshot, retained
   metadata, audit `_accepted_semantic_bar_identity`, and audit
   `_owned_bar_identity`;
9. bar close agrees between attribution snapshot and retained metadata;
10. the B3B event key is recomputed from the audit schedule identity, full
    instrument identity, retained bar close, and sequence identity, and equals
    both frozen audit event-key fields;
11. callback invocation time is not earlier than the accepted bar close time
   and agrees with the retained B3E callback chronology;
12. callback authority ID and all frozen fingerprints are non-zero and agree
   with the accepted B3E lineage;
13. no intent has been extracted, cloned, serialized, persisted, queued, or
   offered to a sink.

The preflight may borrow each `Ok` intent only for the accepted Stage 5C
validator. It may not return intent references or a collection view.

## Stage 5C-owned borrowed preflight bridge

Private Stage 5C recovery, attribution-snapshot, and retained-bar fields are
validated only by one Stage 5C-owned production bridge:

```text
validate_stage5e_b3f_stage5c_preflight_binding(
    recovery_receipt: &Stage5cPendingRecoveryReceipt,
    attribution_snapshot: &Stage5ePreCallbackAttributionSnapshot,
    retained_bar_metadata: &Stage5eAcceptedBarSettlementMetadata,
    expected_binding: &Stage5eB3fStage5cExpectedPreflightBinding,
    seal: &Stage5ePaperSettlementPreflightSeal
) -> Result<
    Stage5eStage5cPreflightValidatedProof,
    Stage5eStage5cPreflightMismatch
>
```

The bridge has one definition and one call site. It takes immutable borrows,
does not extract intents, and does not mutate strategy, recovery, snapshot, or
metadata. Its proof and mismatch are opaque, non-decomposable, non-cloneable,
and non-serializable.

The expected binding is constructed exactly once by
`construct_stage5e_b3f_stage5c_expected_preflight_binding` under the preflight
seal. It is `pub(crate)` opaque with private fields and carries field-for-field
only: audit schedule identity, sequence identity, both event-key fingerprints,
full and owned instrument IDs, and accepted and owned bar identities. There
are no production raw getters, tuple exports, generic parts, scalar overloads,
or reuse/generalization of the existing `#[cfg(test)]` inspection helpers.

The Stage 5C proof is then combined with the Stage 5E-owned audit-authority,
fingerprint, and chronology checks. All preflight borrows end before escrow
consumption.

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
issues exactly one `Stage5ePaperSettlementConsumeSeal` and borrows it while
consuming the escrow once. The consume path privately moves:

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
pre-callback state, or reusable capability after return. The consume
capability is not moved into the payload, stored in either receipt, returned,
cloned, reconstructed, or converted.

The exact safe-Rust liveness sequence is:

```text
issue one stack-local consume capability
→ borrow it for the sole escrow consume
→ on ProceedOk classify the exact Ok outcome
→ borrow it for the sole material-seal issuer
→ borrow it for the sole settlement-seal issuer immediately before Stage 5C
→ drop it before every success or terminal return
```

A second escrow consume is impossible because the first call moves `self`.
Compile-fail coverage pins the absence of `Clone`, reconstruction, payload or
receipt storage, and a second consume.

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
    seal: &Stage5ePaperSettlementConsumeSeal
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

The seal is opaque, non-cloneable and non-convertible. Its sole named issuer
is:

```text
issue_stage5c_b3f_settlement_seal(
    consume_capability: &Stage5ePaperSettlementConsumeSeal
) -> Stage5cB3fSettlementSeal
```

The issuer has one definition and one call site. It is called after exact
`Ok` classification and material construction, immediately before the Stage
5C bridge. The seal cannot be pre-issued, stored, returned, or reconstructed.

The Stage 5C material owns the mutated strategy, recovery receipt, exact
intent vector, attribution snapshot and retained bar metadata. Inside the
bridge:

1. admission is resolved from the owned recovery receipt;
2. the cleanup ledger from the pre-callback snapshot is passed to
   `stage5cj_expected_generated_attribution_by_request_from_ledger`;
3. Stage 5C privately constructs the exact internal
   `Stage5cSemanticBarResult`, including that attribution map;
4. the new private `settle_stage5c_semantic_result_owning_core` is called
   exactly once by the B3F bridge;
5. the existing `settle_stage5c_semantic_result` delegates to that same owning
   core, preserving its public signature and legacy error projection;
6. the shared complete canonical core calls
   `stage5c_build_paper_intent_batch` and
   constructs
   `settled_batch_history == [stage5ch_batch_summary(&canonical_batch)]`;
7. no fallback map or parallel settlement algorithm is built in Stage 5E;
8. success returns the canonical `Stage5cSettledPaperStrategy` with the
   one-entry history;
9. B3F failure returns exact error plus surviving ownership in one private
   terminal material; the unchanged legacy public entrypoint returns the exact
   error and drops ownership exactly as before.

Directly invoking the old public entrypoint from B3F is forbidden because its
`Err` type cannot return consumed strategy/recovery ownership. The shared
owning core is the single algorithmic authority and has exactly two call sites:
the legacy wrapper and the B3F bridge.

The terminal material retains mutated strategy, recovery receipt,
pre-callback attribution snapshot, retained bar metadata, the exact
`Stage5cIntentSettlementError`, and the original count derived inside the
material constructor from `exact_intent_vector.len()` before the vector moves.
The caller never supplies this count.

On every Stage 5C terminal path the exact vector is either consumed by the
canonical settlement path or explicitly and irreversibly dropped inside the
sole Stage 5C bridge. In particular, an attribution/request-ID derivation
failure before batch construction drops the still-owned vector before
terminal material is returned. The vector is never returned, logged,
formatted, cloned, reconstructed, or made retryable.

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

Every terminal reason has an exact producer:

- `CallbackValidationError`: exact callback `ValidationError`;
- `IntentCapacityExceeded`: preflight count above `u8::MAX` or fail-closed
  `TooManyIntents`;
- `IdentityMismatch`: a named Stage 5C binding mismatch or Stage 5E
  audit/authority identity mismatch;
- `ChronologyMismatch`: Stage 5E callback/audit chronology;
- `PaperModeMismatch`: Stage 5C paper/origin/eligibility mismatch or
  fail-closed `ReplayIntentNotExecutable`;
- `Stage5cIntentValidationFailed`: the eight named validation errors in the
  exhaustive mapper;
- `Stage5cPendingRequestMismatch`: `MissingPendingRequest` or
  `RequestIdMismatch`.

There is no ungoverned attribution terminal reason.

## Canonical proof and settlement identities

B3F-r3 replaces the unsatisfiable Cartesian source claim with this exact
field/source/recomputation matrix:

| Field or relation | Required sources and check |
| --- | --- |
| strategy ID | recovery admission == pre-callback attribution snapshot |
| account ID | recovery admission == pre-callback attribution snapshot |
| full `InstrumentId` | admission == attribution snapshot == audit full instrument == audit owned instrument |
| semantic-bar identity | attribution snapshot == retained metadata == audit accepted identity == audit owned identity |
| bar close | attribution snapshot == retained metadata |
| bar close to audit event key | recompute from audit schedule identity, full instrument, retained close and sequence identity; compare with both audit event-key fields |
| callback chronology | callback time >= retained close and accepted B3E chronology holds |
| callback authority and fingerprints | payload == audit; all required digests are non-zero and internally recomputed/equal |
| paper/live closure | admission is paper-only, live-order/intent-sink authority absent, origin `Live`, execution eligible |

Each relation edge is independently checker-pinned. No source is required to
contain a field absent from its actual schema. The audit commitment and final
settlement identity use SHA-256 with domain-separated, versioned canonical
encoding.

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
    true original intent count as usize,
    audit commitment,
    Stage5ePaperSettlementTerminalSeal
) -> Stage5ePaperSettlementTerminalReceipt
```

The terminal seal has one private constructor and one call site. The receipt
has no raw error formatter, strategy/recovery/intent getter, reusable escrow,
generic parts, retry capability, sink conversion, or second constructor.
Sole future consumers of both receipts remain deferred to a separate reviewed
stage.

## R3 exact Stage 5C input construction

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
    Stage5cB3fSettlementMaterialSeal
) -> Stage5eStage5cSettlementMaterial
```

The function is owned by `stage5c_paper_host`, is `pub(crate)` solely for this
bridge, and has one definition and one call site. Before moving the vector, it
derives `derived_original_intent_count: usize` from
`exact_intent_vector.len()`. No caller-supplied count exists, no truncation is
possible, and the over-capacity preflight terminal retains its true `usize`
count. The returned type is `pub(crate)` opaque with private fields. Its sole
consumer is
`settle_stage5e_callback_escrow_material`.

No raw constructor, public field, tuple, generic parts, scalar overload,
alternate material, or second call site is permitted. Audit lineage,
callback timestamp, callback authority ID, and audit commitment remain in the
Stage 5E settlement payload; they never enter Stage 5C material.

## R3 exact Stage 5C success return

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
    settled_batch_history_length,
    canonical_first_batch_summary,
}
```

The view borrows the canonical batch and history, proves history length is one
and its first entry exactly equals
`stage5ch_batch_summary(&canonical_batch)`, is non-decomposable, and has no
public fields, getters, serialization, clone, or conversion. It is created
only by:

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

## R3 exact Stage 5C terminal return

The Stage 5C terminal type owns exactly:

```text
mutated_strategy
recovery_receipt
pre_callback_attribution_snapshot
retained_bar_metadata
exact Stage5cIntentSettlementError
derived_original_intent_count: usize
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

## R3 named authority functions

Stage 5C additionally defines the sole settlement-seal issuer:

```text
issue_stage5c_b3f_settlement_seal(
    consume_capability: &Stage5ePaperSettlementConsumeSeal
) -> Stage5cB3fSettlementSeal
```

It has one definition and one call site, and that call is immediately before
`settle_stage5e_callback_escrow_material`.

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

## R3 exact terminal ownership matrix

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
true_preflight_intent_count: usize
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
true_preflight_intent_count: usize = 0
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
derived_original_intent_count: usize
audit_commitment
stage5c_error = Some(exact error)
```

No variant drops audit lineage, callback error, strategy, recovery ownership,
attribution snapshot, bar metadata, chronology, authority ID, or count. The
private tag and fields cannot be inspected outside the receipt owner. No
variant is retryable or convertible to success.

## R3 realizable cross-module order

The implementation order is frozen exactly:

```text
1. issue one preflight seal
2. borrow the escrow preflight
3. construct one opaque Stage 5E expected-binding carrier
4. invoke the sole Stage 5C borrowed preflight validator
5. combine its proof with Stage 5E audit/authority/chronology checks
6. obtain ProceedOk or Terminal and end every preflight borrow
7. issue one stack-local consume capability
8. consume escrow by borrowing the capability
9. construct the audit commitment exactly once
10a. Terminal: move exact callback outcome and all survivors into terminal
10b. ProceedOk: classify and move the exact Ok vector once
11. issue the material seal by borrowing the consume capability
12. construct material and derive its usize count from vector.len()
13. issue the named settlement seal by borrowing the consume capability
    immediately before the bridge
14. call the Stage 5C bridge once; it invokes the shared owning canonical core
    once, also used by the legacy settle_stage5c_semantic_result wrapper
15a. success: prove canonical batch and one-entry canonical history, build
     settlement identity once, move canonical settled ownership into success
15b. failure: consume or irreversibly drop the vector inside Stage 5C, retain
     exact error and all surviving ownership, map once, construct terminal
16. drop the consume capability; no return carries capability, escrow, raw
    vector, or retry authority
```

The checker rejects a by-value consume seal, pre-issued downstream seals,
second issuers, early settlement-seal issuance, capability storage or
reconstruction, raw Stage 5C getters, a flat Cartesian identity claim,
non-canonical settlement/history, caller-supplied count, recoverable
early-error vectors, and terminal reasons without exact producers.

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
