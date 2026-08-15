# Stage 8A-4 durable-composition design R1

## Authority and scope

This design follows the independently accepted Stage 8A-4 pure reducer at
`4caf07c16ddad021add7cffe6e887165e49e1bf0`; its final acceptance review
SHA-256 is
`0f8de37819ccc005bbc609bc21f029f5783ccdd43c0a634b4c09614f507c2a0a`.

This artifact is design-only. It adds no production Rust, durable journal
mutation, Redis command consumption, ACK/readiness publication, FINAM
POST/DELETE, retry/resend, broker dispatch, runtime-live or real orders.

## Ownership boundary

`Stage8a4ReconciliationDiagnostic` remains public informational side evidence.
Its public fields, `Clone` and `Serialize` implementations make it unsuitable
as durable authority. No future apply function may accept that diagnostic or a
caller-provided reconstruction of it.

The implementation design requires one crate-private composition owner. That
owner invokes the accepted reducer internally and retains a new private,
opaque, non-Clone, non-Serialize, linear authoritative outcome. The owner may
emit the existing public diagnostic as side evidence, but only the private
outcome can reach the future transition builder. There is no public constructor,
getter or conversion from diagnostic to authority.

The private outcome binds the durable request, Stage 6 request state, Stage 7
command identity, accepted reconciliation truth/admission binding, policy,
current journal generation and recovery seal. It is single-use and cannot grant
retry, re-arm, resend or transport authority.

## Source-state policy

Exact order acquisition is a typed state, not `Option`:

- `NotAttempted` means no exact acquisition was initiated;
- `Succeeded` owns the typed exact observation and timing;
- `DocumentedNotFound` records an attempted documented 404-like result;
- `Unavailable` records transport/service unavailability;
- `DecodeFailure` records an unusable response;
- `Stale` records an observation outside the accepted timing policy.

Only `Succeeded` supplies exact-order evidence. Every other attempted state is
preserved in the composition input and remains a reconciliation hold where it
matters. `DocumentedNotFound`, `Unavailable`, `DecodeFailure` and `Stale` never
become `ProvenNoMatch`; Stage 8A-4 has no `ProvenNoMatch` outcome.

Compatible list and exact-GET observations with partial identities retain the
accepted reducer's conservative policy: distinct observations conflict. No
material-compatibility merge is introduced by composition. Any future merge is
a separate reconciliation-design change.

## Account-wide safety

Before the linear admission is consumed, the owner preserves or recomputes a
complete account safety summary containing:

- account-wide active order count;
- unknown-status order count;
- orphan order count, meaning broker orders not correlated to a known durable
  request under the accepted identity policy.

An exact request outcome cannot clear an account-level hold. Any unknown or
orphan count greater than zero blocks readiness. Active orders remain safety
data under the separately reviewed readiness policy and cannot be discarded
when the target request is exact.

## Apply-time revalidation

Immediately before a future durable transition append, the composition owner
must revalidate all of the following against current authoritative state:

1. exact durable request identity and binding;
2. Stage 6 request/client/broker identity state;
3. Stage 7 stable command identity and payload binding;
4. journal generation and append frontier;
5. authenticated current recovery seal;
6. operator-arm generation and exact account/instrument scope;
7. kill-switch state;
8. complete account safety summary.

Any mismatch consumes or invalidates the private outcome and returns a
non-authoritative hold diagnostic. It performs no append, ACK, readiness
publication, retry, resend or broker effect.

## Durable transition vocabulary

The future append vocabulary is identity-preserving and finite:

- `ExactWorking`;
- `ExactTerminalFilled`;
- `ExactTerminalRejected`;
- `ExactTerminalCancelled`;
- `ExactTerminalExpired`;
- `ReconciliationConflictHold`;
- `ReconciliationStillUnknownHold`.

Lifecycle and fill remain orthogonal payload dimensions exactly as in the
accepted reducer. Conflict and StillUnknown never advance the order lifecycle,
never imply no-match and never authorize retry. They create or preserve a hold
and require operator disarm before any later separately authorized recovery.

The transition key binds durable request binding, accepted reconciliation truth
binding, transition kind and current journal generation. Replaying the same key
is idempotent. A different payload under an existing key is a hard conflict.

## Crash and replay model

Two mandatory fault boundaries are frozen:

1. `BeforeDurableTransitionAppend`: a crash leaves no transition. Recovery
   reruns broker-truth acquisition and reconciliation; it does not reuse a
   serialized diagnostic or private outcome.
2. `AfterDurableTransitionAppendBeforeDerivedPublication`: recovery observes
   the durable transition by its stable key and performs no second append.
   Derived ACK/readiness publication may later be resumed idempotently from the
   durable transition, never from caller input.

The append must be durable before any ACK or readiness publication. Publication
failure cannot roll back or duplicate the transition. This design does not yet
implement either append or publication.

## Safety and execution boundary

The accepted reconciliation result grants no transport capability. Same-request
retry/resend remains forbidden regardless of Exact, Conflict or StillUnknown.
Operator arm is not recreated by replay. Kill-switch and disarm state are
revalidated rather than cached.

Durable-composition implementation, journal mutation, ACK/readiness, Redis-live,
FINAM POST/DELETE, broker dispatch, runtime-live, real orders, Stage 8A-5 and
Stage 8B remain closed until separately specified and accepted.

## Exit rule

Independent acceptance of this exact design may open only a separate
Stage 8A-4 durable-composition implementation specification. It does not open
production implementation or execution.
