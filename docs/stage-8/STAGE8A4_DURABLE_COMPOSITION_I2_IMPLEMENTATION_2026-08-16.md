# Stage 8A-4 durable composition I2

## Authority

I1 R2 is independently accepted at
`113d2827ef255e8d2c2597a3acb38fe52dd7e52d`. This slice implements only
`I2_private_linear_composition_and_transition_builder_no_append`.

## Implemented boundary

The accepted Stage 8A-4 reducer now retains one private, opaque, non-Clone and
non-Serialize authoritative outcome. The existing public diagnostic remains a
consuming redacted projection and cannot be converted back to authority.

The nested private I2 builder consumes durable identity, an exact journal
cursor, four-field pre-append evidence and the private outcome. It produces an
in-memory candidate containing:

1. one canonical validated `Stage6JournalRecordV2`;
2. the exact representable deterministic V1 suffix.

It has no journal backend and cannot append, compare-and-append,
write a recovery seal, publish ACK/readiness, XACK Redis, call FINAM or dispatch.

## Stable identity and sequence

The stable transition key uses one fixed domain and exactly:

1. durable request binding;
2. private authoritative outcome binding;
3. transition kind.

Mutable checkpoint, recovery-seal generation/fingerprint and request-state
fingerprint remain in the separate four-field pre-append precondition. V2 is
always sequence `previous + 1`, with both previous and causal links bound to
the supplied cursor. V1 suffix records follow contiguously and the manifest
binds their event kind, record ID, sequence, payload digest and full canonical
record digest.

## Exact lookup policy

- `NotAttempted`: preserve the accepted reducer result.
- `Succeeded`: retain the typed exact observation.
- `DocumentedNotFound`: Conflict only when an admitted exact order
  contradicts not-found; otherwise StillUnknown.
- `Unavailable`, `DecodeFailure`, `Stale`: StillUnknown.

No attempted non-success state can become Exact.

## Endpoint projection

PLACE exact states use the accepted endpoint matrix. A real broker order ID is
required for `BrokerOrderObserved`; a real matching broker order ID is
required for each `BrokerTradeObserved`. Missing IDs are never fabricated.
Finalization remains endpoint-disposition based.

CANCEL Working emits no finalization suffix. Terminal CANCEL states emit
`CancelOutcomeObserved` followed by Completed finalization, with the target
broker order ID taken only from the durable cancel identity.

Conflict and StillUnknown holds emit no V1 suffix and grant no settlement.

## Still closed

I3 writer/CAS/append/seal and I4 ACK/readiness remain closed. Redis live
consumption, FINAM POST/DELETE, broker dispatch, retry/resend/re-arm,
runtime-live, real orders, Stage 8A-5+ and Stage 8B remain closed.
