# TZ — Stage 8A-4 durable-composition implementation specification R2

## Authority and scope

Normative predecessor: accepted/closed Durable Composition Design R2 at
`6ddf54ef9d7f740dc59cd2450e78301be3d068cb`; acceptance-review SHA-256:
`160b674d661982b6dbaa6248c2c4acaf883543cb8be99318ef04b0787492f4ba`.

Implementation specification R1 at
`e3d0ac39dcff25439a7e78f51142b852d8347a2f` was not accepted; its independent
review SHA-256 is
`968f2c61f9c9b01a56e1f8950664d46000b15e038abab74a11089bd91988996b`.
R2 closes only the three P1 schema/replay gaps from that review.

This is a specification-only artifact. Production Rust, I1, durable mutation,
ACK/readiness publication, Redis-live, FINAM POST/DELETE, broker dispatch,
runtime-live and real orders remain closed pending independent acceptance.

## Additive schema and immutable V1

Stage 6 V1 bytes, record IDs, causal rules and replay semantics are immutable.
No journal rewrite or migration is allowed. Stable transition key, exact query
evidence, lifecycle/fill truth and recovery metadata cannot be hidden in
`source_evidence_sha256`, marker records or unknown fields.

The only permitted additive model is:

```text
Stage6JournalRecordVersioned =
    V1(Stage6JournalRecordV1)
  | V2(Stage6JournalRecordV2)
```

Supported top-level record schema versions are exactly `{1, 2}`. A V1-only
binary encountering V2, or any reader encountering an unknown, malformed or
ambiguous discriminator, fails closed. Failed V2 decode never falls back to V1.

## Complete canonical V2 record envelope

The exact top-level record is:

```text
Stage6JournalRecordV2 {
    schema_version: 2,
    journal_record_id: Stage6JournalRecordId,
    lifecycle_sequence: Stage6LifecycleSequence,
    previous_record_id: Option<Stage6JournalRecordId>,
    causal_parent_id: Option<Stage6JournalRecordId>,
    durable_request_identity: Stage6DurableRequestIdentityV1,
    event_kind: Stage6JournalEventKindV2,
    payload: Stage6ReconciliationTransitionPayloadV2,
    canonical_payload_sha256: Stage6Sha256Digest,
    source_evidence_sha256: Stage6Sha256Digest,
}
```

`Stage6JournalEventKindV2` is separate from immutable V1 and contains exactly
`ReconciliationTransitionApplied`. Unknown fields and enum values fail closed.

V2 reuses the accepted V1 record-ID derivation domain
`stage6-journal-record-v1`: SHA-256 over exact strategy request ID and lifecycle
sequence. It uses the exact next per-request lifecycle sequence;
`previous_record_id` is the current request's last record; `causal_parent_id`
equals that previous ID. The durable identity is the exact existing
`Stage6DurableRequestIdentityV1`. Payload digest is SHA-256 of exact canonical
V2 payload bytes; source evidence is a Stage 6 digest bound to authoritative
acquisition evidence.

## Framed journal dispatch

Storage schema version `1`, frame magic `S6F1` and frame version `1` remain
unchanged because framing bytes do not change. After existing frame integrity
validation, the reader inspects the exact top-level record `schema_version`:

```text
1 -> existing Stage6JournalRecordV1 decoder, byte-identical
2 -> Stage6JournalRecordV2 canonical decoder
other/malformed/ambiguous -> fail closed
```

Both versions require exact canonical round-trip. There is no heuristic decode
and no V2-to-V1 fallback.

## Dedicated V2 persistence DTOs

The Stage 6 persistence owner defines versioned durable DTOs:

```text
Stage6ReconciliationEndpointKindV2
Stage6ReconciliationTransitionKindV2
Stage6ReconciliationLifecycleV2
Stage6ReconciliationFillEffectV2
Stage6ExactLookupEvidenceV2
Stage6BrokerOrderFactV2
Stage6MaterialTradeFactV2
Stage6AccountSafetySummaryV2
Stage6PreAppendPreconditionV2
Stage6SuffixManifestV2
Stage6SuffixManifestEntryV2
```

Private `finam-gateway` types and unversioned live domain structs are never
serialized wholesale. DTOs deny unknown fields, use canonical decimal/string/
time encodings, bound collections where a bound exists, and contain no token,
URL, transport or send capability.

The V2 payload fields are exactly stable transition key, durable request and
private outcome bindings, endpoint and transition kinds, exact lookup evidence,
complete broker-order fact, all material trade facts, fill effect, account
safety summary, pre-append precondition and deterministic suffix manifest.

## Exact lookup discriminated union

`Stage6ExactLookupEvidenceV2` has these exact variants:

```text
NotAttempted { state }
Succeeded { state, account_id, queried_broker_order_id,
  durable_request_binding_sha256, request_started_at, response_received_at,
  exact_order_observation_v2 }
DocumentedNotFound { state, account_id, queried_broker_order_id,
  durable_request_binding_sha256, request_started_at, response_received_at,
  documented_status_category }
Unavailable { state, account_id, queried_broker_order_id,
  durable_request_binding_sha256, request_started_at, response_received_at,
  failure_category }
DecodeFailure { state, account_id, queried_broker_order_id,
  durable_request_binding_sha256, request_started_at, response_received_at,
  response_status_category, response_binding_sha256 }
Stale { state, account_id, queried_broker_order_id,
  durable_request_binding_sha256, request_started_at, response_received_at,
  stale_observation_binding_sha256 }
```

Attempted non-success cannot normalize to `NotAttempted`. A successful exact
observation participating in `ConflictHold` remains represented rather than
being reduced to a generic state token.

## Mixed V1/V2 replay and pending batch

A valid V2 record validates exact durable request identity, exact next sequence
and exact previous/causal link, then advances mixed replay `last_sequence` and
`last_record_id`. It registers one pending reconciliation batch containing:

```text
stable_transition_key_sha256
transition_kind
canonical_v2_record_sha256
deterministic_suffix_manifest
verified_suffix_prefix_length
batch_completion_state
last_mixed_record_id
last_mixed_lifecycle_sequence
```

V2 itself does not synthesize or apply V1 suffix semantics, finalize the
request, authorize settlement, send or retry. Each following V1 record is
validated against the next full manifest entry and then applied through normal
V1 replay semantics in order. A partial exact prefix remains pending with the
first missing suffix entry known; completion marks the batch complete. An
unexpected in-batch record is a hard conflict. Restart does not require the
lost process-local private outcome.

Same stable key plus the exact same canonical V2 record in its valid causal
position is idempotent existing-batch evidence. Same key with different payload
or full record is a hard conflict. A second distinct V2 transition for the same
batch is forbidden. V2 after an already finalized request fails closed.

## Full-record deterministic V1 suffix manifest

Each expected compatibility record is represented by:

```text
Stage6SuffixManifestEntryV2 {
    ordinal,
    event_kind,
    journal_record_id,
    lifecycle_sequence,
    canonical_payload_sha256,
    canonical_record_sha256,
}
```

`canonical_record_sha256` is SHA-256 of the exact
`Stage6JournalRecordV1::encode_canonical()` bytes. It therefore binds previous
and causal IDs, durable identity, sequence/record ID, payload and source
evidence—not merely kind and payload.

Exact full record marks an entry complete. Same record ID with another record
hash, or same payload hash with another full record, is a hard conflict. A
missing next entry stays pending for exact reconstruction by the later writer
slice; an unexpected extra in-batch record is a hard conflict.

## Complete V2 facts and lossless compatibility projection

V2 is the complete durable reconciliation authority. It retains the selected
broker-order fact even when `broker_order_id = None`, and retains every material
trade including client-ID-linked trades whose broker order ID is absent.

The V1 suffix is a lossless compatibility projection of the subset V1 can
represent:

- append `BrokerOrderObserved` only for a real `BrokerOrderId`;
- append `BrokerTradeObserved` only for a real compatible broker order ID;
- never fabricate either ID;
- never drop the complete fact from V2 merely because V1 cannot represent it;
- finalization follows endpoint disposition, never an invented legacy ID.

The manifest describes exactly that representable V1 subset. Absence of an ID
does not silently change the accepted transition disposition.

## Endpoint dispositions and canonical ACK

PLACE `ExactWorking`, terminal filled/cancelled/expired complete the PLACE
request and later yield `Recovered / RecoveredByBrokerTruth` after S1. PLACE
terminal rejected finalizes Rejected and later yields
`Rejected / BrokerRejected`. PLACE holds do not finalize or ACK.

CANCEL `ExactWorking` remains unresolved. CANCEL terminal filled maps to
`ExecutionObserved`; terminal rejected/expired map to
`AlreadyTerminalNonExecution`; terminal cancelled maps to `Canceled`. These
terminal cases complete only after their exact V1 suffix and S1. CANCEL holds
do not finalize, ACK or XACK.

No terminal ACK authority exists until `RequestFinalized` is durable and the
authenticated reread covering seal S1 covers final frontier F1. V2 alone,
append receipt, public diagnostic or pre-append S0 is insufficient.

## CAS, controls and covering seal

Apply-time CAS binds exact Stage 6 frontier/checkpoint fingerprint, recovery
seal generation/fingerprint and request-state fingerprint. Under the existing
single writer lease the future flow is V2 first, exact V1 suffix, F1, S1, then
authenticated canonical/checkpoint validation.

Expired arm, `StopRequested` or stale/unreadable kill switch do not block
post-effect reconciliation append; they block new send and readiness. Replay
does not recreate an arm. Reconciliation has no transport capability. Seal
failure leaves the batch durable and settlement pending.

## I1 golden and negative acceptance

I1 must provide exact canonical goldens for: PLACE working with ID present and
absent; PLACE terminal rejected; partial fill with trade ID present and absent;
CANCEL working and terminal cancelled; both holds; all six lookup variants,
including Succeeded observation; mixed V1/V2, partial suffix and complete
suffix; unknown schema fail closed; and immutable V1 bytes.

Specification R2 retains all 40 R1 negative concepts and adds at least 17:
missing V2 causal/identity fields, mutable V1 event enum, unknown-version skip,
decode fallback, ignored mixed frontier, V2 direct finalization, lost pending
batch, payload-only manifest, source/causal mutation, fabricated IDs, dropped
client-linked trade, lost successful lookup observation and changed V1 golden.

## Implementation slices after independent acceptance

1. **I1 — additive schema/codec/replay:** dedicated V2 canonical types,
   version dispatch, mixed V1/V2 replay and goldens; no writer or apply API.
2. **I2 — private composition/builder:** private linear owner and deterministic
   transition/suffix construction; no append.
3. **I3 — durable batch/seal/recovery:** CAS append, suffix recovery and S1;
   no Redis or broker transport.
4. **I4 — derived ACK/readiness facade:** capability derivation only after
   finalization plus S1; Redis-live remains closed.

Independent acceptance of this specification opens only I1. FINAM POST/DELETE,
same-request resend/re-arm, Redis-live consumption, broker dispatch,
runtime-live, real orders, Stage 8A-5 and Stage 8B remain closed.
