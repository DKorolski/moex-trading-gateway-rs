# Stage 8A-4 durable composition I3

## Authority

I2 R3 is independently accepted and closed at
`90f46052cc31cea012437eddb59fb7c3ca5c2320`; the independent review SHA-256 is
`196c2b69161081f9034eb9399f41245f11ccd7eca229fadc3f8ec842cd1231f0`.
This slice implements only the separately opened I3 writer/CAS/append/covering
seal boundary. I4 ACK/readiness and every execution surface remain closed.

## Current durable authority

The sole entry is owned by `Stage7bRecoveryReadyOwner` while it retains the
kernel writer lease. Immediately before mutation it rereads and authenticates
S0, refreshes the file-backed journal and reconstructs one request authority
from the current recovered Stage6 lifecycle. The authority requires the exact
typed `RequestAccepted`, its command payload digest and exactly one linked
`DispatchAttemptRecorded`.

The stable durable-request binding is recomputed from immutable Stage6
identity, accepted record, dispatch record/sequence and runtime configuration.
Mutable frontier/checkpoint/seal fields remain outside that binding and in the
four-field CAS. For CANCEL, the original order shape is resolved from exactly
one historical durable PLACE plus the observed target BrokerOrderId; it is
never reconstructed from the cancel command.

## Four-field compare-and-append

Under the held writer lease I3 exact-compares:

1. Stage6 checkpoint or causal-frontier fingerprint;
2. recovery-seal generation;
3. recovery-seal commitment fingerprint;
4. request-state fingerprint at the original dispatch frontier.

A stale candidate is consumed by value and fails before journal mutation.
Current previous record, previous lifecycle sequence and global journal tail
must all name the same dispatch attempt.

## V2-first durable batch

The append order is fixed:

1. canonical V2 transition append and file sync;
2. each exact missing V1 manifest record append and file sync;
3. refresh mixed V1/V2 replay and require a complete batch;
4. derive F1;
5. atomically commit S1 covering F1;
6. reread, authenticate, decode and compare S1 and recovered identity.

The mixed replay index enforces stable-key behavior before mutation. Same key
and same canonical V2 resumes an existing batch. Same key and different V2 is
a hard conflict. An existing verified suffix prefix permits only the exact
remaining full-record manifest suffix; no second transition is appended.

## Crash and restart behavior

Normal Stage7B checkpoint mismatch remains fail-closed. The only journal-ahead
exception is one canonical I3 V2 immediately after the S0 frontier followed by
zero or more exact manifest-prefix V1 records and no second V2. Its precondition
must bind S0, the old frontier and the old request fingerprint. Restart
reconstructs the current Stage6 authority, validates the uncovered tail, commits
and rereads a covering seal, then returns Ready.

Tests cover V2-only crash and a partial suffix crash with V2 plus one of two
suffix records. A later retry
appends only the exact missing suffix record and advances the seal once more.

## Post-effect control and account safety

Historical one-shot arm provenance is retained as an opaque non-serializable
value. The existing Stage8A-1 issuer checks the pinned arm-registry record and
rereads current control state. `StopRequested`, expired or unreadable current
control remains a send/readiness hold but does not erase already-established
broker truth and does not grant resend authority.

The private FINAM composition recomputes the complete canonical
`BrokerTruthSnapshot::summarize_for_instrument` account safety immediately
before writer entry and requires exact equality with the admitted V2 safety
projection. New unknown/orphan/order/position state rejects the stale outcome.

## Still closed

The I3 receipt is durable-only and cannot authorize ACK, readiness, Redis
settlement or XACK. I4 remains separately review-gated. FINAM POST/DELETE,
broker dispatch, retry/resend/re-arm, runtime-live, real orders, Stage 8A-5+
and Stage 8B remain closed.
