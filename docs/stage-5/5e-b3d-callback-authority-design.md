# Stage 5E-b3d — callback-authority design

Status: design-only, pending review.

Baseline:

```text
ff1344f170b8457df91a6038d670087eef3cc1dc
```

The accepted predecessor is the private no-I/O chain ending in:

```text
Stage5eBoundSessionCalendarSequenceForObservedLiveBar
```

This stage defines the next type-state boundary. It does not implement that
boundary and does not invoke a strategy callback.

## Goal

Design one linear transition:

```text
Stage5eBoundSessionCalendarSequenceForObservedLiveBar
→ Stage5eCallbackAuthorityReadyPaperStrategy
```

The future output means only that the exact accepted bar and retained
strategy/recovery state are eligible to cross a separately reviewed callback
boundary. It is not an execution capability.

The authority vector remains:

```text
callback_ready = true
callback_invoked = false
execution_ready = false
calls_strategy = false
mutates_strategy = false
creates_executable_intent = false
intent_count = 0
```

## Linear ownership

The future transition has exactly one crate-private consumer of the B3C
receipt. It consumes the receipt only after a borrowed, non-decomposable
preflight succeeds.

The future callback-authority receipt owns the complete B3C receipt. This
preserves:

- the strategy instance;
- pending recovery state;
- accepted semantic bar;
- normalized schedule and Stage 4 dynamic-open identities;
- sequence identity;
- continuation binding identity;
- effective observation and expiry bounds.

No raw strategy getter, generic `into_parts`, `Clone`, `Copy`, serialization,
deserialization, default construction, conversion trait, or successful
unbinding is permitted.

## Revalidation at the authority boundary

The future production transition captures its own UTC clock internally and
must revalidate, immediately before issuing callback authority:

- `now >= effective_observed_at`;
- `now <= effective_expires_at`;
- accepted semantic bar close is not in the future;
- continuation binding identity is non-zero;
- B3B event key, schedule identities, sequence identity, and ownership
  binding are present and unchanged;
- callback authority has not already been issued for the same consumed
  receipt.

The test clock seam is `cfg(test)` only. A caller-supplied production clock is
forbidden.

The callback seam does not infer a venue trading day from UTC civil date.
Venue timezone, overnight-session mapping, FINAM schedule provider, and
InstrumentRegistry attachment remain separate future gates.

## Block taxonomy and type enforcement

The future result must use distinct, non-interchangeable blocked types:

```text
Stage5eCallbackAuthorityRetryableBlock
Stage5eCallbackAuthorityRefreshEvidenceBlock
Stage5eCallbackAuthorityTerminalBlock
```

Only `Stage5eCallbackAuthorityRetryableBlock` may expose
`into_retry_same_receipt()`.

`Stage5eCallbackAuthorityRefreshEvidenceBlock` may return the retained receipt
only through `into_refresh_input()`. It cannot feed the same authority
transition again until a separately reviewed refresh transition has replaced
the expired evidence.

`Stage5eCallbackAuthorityTerminalBlock` exposes no retry or refresh
conversion.

Required reason classes:

```text
RetrySameReceipt:
  ClockBeforeEffectiveObservation
  AcceptedBarObservedInFuture

RefreshEvidenceRequired:
  EvidenceExpired

TerminalIntegrity:
  ContinuationBindingMissing
  EventKeyMissing
  ScheduleIdentityMissing
  SequenceIdentityMissing
  OwnershipBindingMismatch
  DuplicateAuthorityIssue
```

No autonomous retry loop is authorized by this design.

Before provider/runtime refresh orchestration, the predecessor B3B
`EvidenceExpired` wording must either become `RefreshEvidenceRequired` or be
split into schedule-projection and sequence-evidence expiry. The current
fail-closed predecessor behavior is accepted and unchanged in this stage.

## Future implementation shape

The future implementation review must prove:

1. one private issuer and one consumer;
2. borrowed preflight before linear consume;
3. exact source receipt returned by retryable and refresh blockers;
4. no callback invocation on any blocker;
5. no successful unbinding;
6. production-clock revalidation immediately before authority issuance;
7. callback authority remains distinct from callback invocation;
8. canonical Stage 4 → B3C → callback-authority no-I/O test;
9. strategy/recovery fingerprint equality before and after authority issue;
10. compile-fail evidence for construction, cloning, raw extraction, and
    direct callback use.

The predecessor canonical non-Open test should use an explicit `expect` for
Stage 4 acceptance before asserting that Stage 5E projection blocks. That
test hardening is deferred to the separately reviewed implementation commit.

## Closed surfaces

This design does not add or authorize:

- `on_broker_bar`;
- strategy mutation;
- executable intent construction;
- intent sink or dispatch;
- Redis;
- FINAM I/O;
- transport;
- runtime-live;
- broker execution;
- autonomous event loop;
- schedule/provider attachment;
- venue-calendar inference.

Actual callback invocation requires a separate implementation review after
this design is accepted.
