# Stage 5E-b3d-r1 — callback-authority governance hardening

Status: governance/design-only, pending review.

Accepted Stage 5E-b3d implementation baseline:

```text
ff1344f170b8457df91a6038d670087eef3cc1dc
```

R1 scoped-review predecessor:

```text
95096b7d28ecd3fafddbbfd3ec91b0611019e0eb
```

This R1 contract replaces the initial B3D proposal. No Rust authority type,
strategy callback, state mutation, intent construction, provider attachment,
or I/O is implemented in this stage.

## Route decision

The project selects **explicit isolated scope**.

The existing public Stage 5C routes:

```text
apply_stage5c_semantic_bar
advance_stage5c_paper_loop_once
```

remain frozen paper/oracle compatibility APIs. They are not callback authority
for a new Stage 5E runtime and must never be attached to a Stage 5E production
or live event loop.

The sole future Stage 5E callback route is:

```text
Stage5eBoundSessionCalendarSequenceForObservedLiveBar
→ issue_stage5e_callback_authority
→ Stage5eCallbackAuthorityReadyPaperStrategy
→ invoke_stage5e_authorized_paper_callback
→ Stage5ePaperCallbackResultEscrow
```

`invoke_stage5e_authorized_paper_callback` may accept only the B3D authority
receipt. It may not accept recovered strategy plus semantic bar as parallel
arguments. The Stage 5E runtime attachment review must include call-graph and
negative-scan evidence proving that neither legacy Stage 5C function is
reachable from the new runtime.

A Stage 5C API freeze extension is not required while the legacy route remains
unchanged and isolated. Any attempt to remove, narrow, or repurpose that public
API is a separate Stage 5C freeze review.

## Exact authority receipt

Future owner module:

```text
strategy_runtime_core::stage5e_no_io_lifecycle::callback_authority
```

Future receipt:

```text
Stage5eCallbackAuthorityReadyPaperStrategy
```

Exact private fields:

```text
b3c_receipt: Stage5eBoundSessionCalendarSequenceForObservedLiveBar
callback_authority_id: Stage5eCallbackAuthorityId([u8; 32])
issued_at: DateTime<Utc>
effective_observed_at: DateTime<Utc>
authority_expires_at: DateTime<Utc>
accepted_bar_close_ts: i64
full_instrument_id: InstrumentId
accepted_semantic_bar_identity: [u8; 32]
event_key_fingerprint: [u8; 32]
continuation_binding_id: Stage5eContinuationBindingId
sequence_identity_fingerprint: [u8; 32]
```

The receipt owns the complete B3C receipt and therefore retains strategy,
recovery, semantic-bar, schedule, and sequence state. The duplicated scalar
identity fields are immutable callback-boundary proof material; they are not
raw strategy or bar extractors.

Forbidden:

```text
Debug
Clone
Copy
Serialize
Deserialize
Default
From
Into
generic into_parts
raw strategy getter
raw semantic-bar getter
successful unbinding
persistence or reconstruction
```

The receipt is in-memory and process-local. Restart never restores it; restart
must rebuild the accepted Stage 4 → Stage 5C → B3B → B3C chain with fresh
evidence.

## Authority identity

`Stage5eCallbackAuthorityId` is derived once, after successful preflight, by:

```text
domain = "stage5e-callback-authority-v1"
algorithm = SHA-256 tagged length-prefixed canonical bytes
fields in order:
  full InstrumentId canonical bytes
  accepted_semantic_bar_identity
  event_key_fingerprint
  continuation_binding_id bytes
  sequence_identity_fingerprint
  issued_at unix milliseconds as signed i64 big-endian
  authority_expires_at unix milliseconds as signed i64 big-endian
```

All identity inputs must be non-zero/non-empty where applicable.

There is no issuance ledger and no `DuplicateAuthorityIssue` runtime blocker.
Exactly-once issuance follows from linear B3C receipt ownership, one issue
seal, one issuer, and one private constructor. A process crash loses the
in-memory capability and requires a fresh chain.

There is no production `ownership_binding_id` and no
`OwnershipBindingMismatch` runtime blocker. Strategy/recovery ownership is
proved by linear type ownership of the complete B3C receipt. Test-only state
fingerprints may prove preservation but cannot become production authority.

## Exact issuing transition

Future types:

```text
Stage5eCallbackAuthorityIssueSeal
Stage5eCallbackAuthorityPreflight<'a>
Stage5eCallbackAuthorityIssueBlocked
```

The issue seal is private, non-constructible outside the owner, and has one
issuer/consumer pair. The preflight is borrowed and non-decomposable.

Future transition:

```text
issue_stage5e_callback_authority(
    Stage5eBoundSessionCalendarSequenceForObservedLiveBar
) -> Result<
    Stage5eCallbackAuthorityReadyPaperStrategy,
    Stage5eCallbackAuthorityIssueBlocked
>
```

Production time is captured inside the transition. A deterministic clock seam
is `cfg(test)` only.

Before consuming B3C ownership, preflight must check:

```text
now >= effective_observed_at
now <= effective_expires_at
accepted_bar_close_ts <= now.timestamp()
all identity inputs are present and non-zero
issued_at = now
authority_expires_at = effective_expires_at
issued_at <= authority_expires_at
```

The authority lifetime is exact:

```text
maximum_issue_to_callback_delay =
    authority_expires_at - issued_at
```

No grace period or expiry extension is allowed.

## Exact issue blockers

R1 deliberately has no refresh output. Expired evidence is terminal for this
receipt; a future provider-refresh design must start from newly accepted
evidence rather than unbinding this receipt.

Two distinct blocker types are required:

```text
Stage5eCallbackAuthorityRetryableBlock
Stage5eCallbackAuthorityTerminalBlock
```

Retryable reasons:

```text
ClockBeforeEffectiveObservation
AcceptedBarObservedInFuture
```

Only the retryable blocker exposes:

```text
into_retry_same_receipt()
    -> Stage5eBoundSessionCalendarSequenceForObservedLiveBar
```

Terminal reasons:

```text
EvidenceExpired
InvalidAuthorityChronology
AcceptedSemanticBarIdentityMissing
EventKeyMissing
ContinuationBindingMissing
ScheduleIdentityMissing
SequenceIdentityMissing
InstrumentIdentityMissing
```

The terminal blocker owns the B3C receipt and exposes no retry, refresh, or
unbinding conversion. There is no autonomous retry loop.

## Exact future callback consumer

Future consume seal:

```text
Stage5eCallbackInvocationSeal
```

It is issued only inside:

```text
invoke_stage5e_authorized_paper_callback
```

That future transition must:

1. accept only `Stage5eCallbackAuthorityReadyPaperStrategy`;
2. capture a fresh production clock internally;
3. require `now >= issued_at`;
4. require `now <= authority_expires_at`;
5. require `accepted_bar_close_ts <= now.timestamp()`;
6. recompute and compare `callback_authority_id`;
7. recheck all immutable identity fields against the owned B3C receipt;
8. consume authority exactly once;
9. invoke the callback only after every check succeeds.

Because the current strategy trait returns intents, actual callback invocation
necessarily allows **in-memory paper intent construction**. It does not allow
an intent sink, Redis publication, FINAM send, dispatch, runtime-live, or
broker execution.

The future output is:

```text
Stage5ePaperCallbackResultEscrow
```

It owns the mutated paper strategy, callback result, and in-memory intents.
Those intents remain private paper escrow and require separate validation and
settlement review. No send-capable consumer is authorized by this design.

Actual callback invocation and escrow implementation remain HOLD after B3D-r1;
they require a separate review after the private authority receipt itself is
implemented and accepted.

## Required implementation evidence

The future private authority implementation must prove:

- exact field schema and owner module;
- one issue seal, issuer, constructor, and issue transition;
- borrowed preflight before linear consume;
- exact retry receipt preservation;
- no terminal unbinding;
- authority ID field-by-field sensitivity;
- exact issue-time expiry;
- no persistence/reconstruction;
- canonical Stage 4 → B3C → authority test;
- strategy/recovery test fingerprint unchanged;
- compile-fail construction, clone, extraction, and direct callback tests;
- Stage 5E call graph cannot reach legacy Stage 5C callback routes;
- no actual callback or intent construction in the authority implementation.

## Closed surfaces

B3D-r1 does not add or authorize:

- a new `on_broker_bar` call;
- strategy mutation;
- in-memory intent construction in the authority issuer;
- intent validation or settlement;
- intent sink;
- Redis;
- FINAM I/O;
- transport;
- dispatch;
- runtime-live;
- broker execution;
- autonomous event loop;
- schedule/provider attachment;
- venue-calendar inference.
