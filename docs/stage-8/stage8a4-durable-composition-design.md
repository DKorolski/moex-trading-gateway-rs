# Stage 8A-4 durable-composition design R2

## Authority and scope

This correction retains the independently accepted pure reducer at
`4caf07c16ddad021add7cffe6e887165e49e1bf0` and the accepted reconciliation
Design R2 at `cc58c10d22db312cd83640f1c1e7fd86861a4594`. It supersedes the
unaccepted durable-composition Design R1 at
`80fe35ef67e335540e0984781f63a99af794bfe1` without reopening either accepted
artifact.

This artifact is design-only. It adds no production Rust, durable apply or
journal mutation, ACK/readiness publication, Redis-live consumer, FINAM
POST/DELETE, retry/resend, broker dispatch, runtime-live or real orders.

## Ownership boundary

`Stage8a4ReconciliationDiagnostic` remains public informational side evidence.
It cannot authorize a transition, ACK, readiness or settlement. One
crate-private composition owner retains a private, opaque, non-Clone,
non-Serialize, linear authoritative outcome. There is no public constructor,
getter or diagnostic-to-authority conversion.

The outcome binds durable request and state, Stage 7 command identity and
payload, the private authoritative reconciliation result, exact-acquisition
state, policy, account safety and the pre-append evidence. It is single-use and
grants no retry, re-arm, resend or transport authority.

## Exact lookup disposition

Exact acquisition has six states with a closed disposition table:

| State | Disposition |
|---|---|
| `NotAttempted` | No exact source; the reducer may use other admitted sources. |
| `Succeeded` | The typed exact observation participates in the reducer. |
| `DocumentedNotFound` | Never no-match; contradiction with an exact source is Conflict, otherwise StillUnknown hold. |
| `Unavailable` | StillUnknown hold. |
| `DecodeFailure` | StillUnknown hold. |
| `Stale` | StillUnknown hold. |

An attempted non-success state cannot be downgraded to `NotAttempted`.
`DocumentedNotFound`, including a documented 404, never becomes
`ProvenNoMatch`; Stage 8A-4 has no `ProvenNoMatch` outcome. Partial list/exact
identity remains conservative Conflict with no material-compatibility merge.

## Account-wide safety

The owner preserves or recomputes account-wide active, unknown-status and
orphan order counts. An exact target result does not clear an account hold and
does not by itself imply account readiness. Existing readiness owners remain
authoritative.

## Stable transition identity and pre-append CAS

The immutable transition key is machine-equivalent to the hash of exactly:

1. `durable_request_binding`;
2. `private_authoritative_reconciliation_outcome_binding`;
3. `transition_kind`.

It contains neither a random nonce nor mutable post-append journal generation,
so it is stable across append and restart.

The append precondition is separate and binds exactly:

1. `expected_stage6_checkpoint_or_frontier_fingerprint`;
2. `expected_recovery_seal_generation`;
3. `expected_recovery_seal_fingerprint`;
4. `expected_request_state_fingerprint`.

Immediately before compare-and-append, the owner also validates Stage 7 command
identity/payload, historical operator-arm provenance and scope, kill-switch
control state and complete account safety. A CAS or identity mismatch consumes
the private outcome and requires fresh reconciliation; it does not mutate the
old key or append under a new generation. Same key plus same payload means an
idempotent existing transition. Same key plus different payload is a hard
conflict.

## Post-effect control semantics

Reconciliation records truth after a possible broker effect; it is not a new
execution attempt.

- Historical operator-arm provenance, account and instrument scope remain
  mandatory. Expiry after possible send does not block reconciliation append.
  Replay never recreates an arm and an arm never authorizes resend.
- `StopRequested` blocks every new send and forces disarm/readiness hold, but
  does not block durable reconciliation append. Stale or unreadable kill-switch
  state blocks readiness and new send, never converts truth to no-match and
  never authorizes a reconciliation send.

Identity substitution or conflicting re-arm invalidates the private outcome;
mere expiry or StopRequested does not erase broker truth.

## Transition and settlement matrix

The exact transition vocabulary is:

- `ExactWorking`;
- `ExactTerminalFilled`;
- `ExactTerminalRejected`;
- `ExactTerminalCancelled`;
- `ExactTerminalExpired`;
- `ReconciliationConflictHold`;
- `ReconciliationStillUnknownHold`.

Exact transitions may derive only the canonical disposition bound to the
durable request kind, endpoint and original identity, and only after both the
transition and its covering recovery seal are durable and validated.

`ReconciliationConflictHold` and `ReconciliationStillUnknownHold` may be
durably recorded, but never produce terminal command ACK, Redis XACK or
terminal settlement. Both keep readiness false/degraded and prohibit
retry/re-arm/resend. Conflict disarms the operator; StillUnknown remains
unresolved/pending. Neither advances order lifecycle.

## Durable append, covering seal and publication

The successful sequence is frozen:

```text
frontier F0 + committed validated seal S0 + current request identity
-> revalidate precondition
-> append/fsync stable transition K
-> obtain receipt and frontier F1
-> construct checkpoint for F1
-> commit authenticated seal S1 covering F1
-> reread and validate S1 HMAC/canonical/checkpoint binding
-> only then consider canonical ACK/readiness/settlement publication
```

The pre-append seal S0 is insufficient after the journal advances. If S1
cannot be committed, the transition remains durable, no second append occurs,
and ACK, readiness success, XACK and terminal settlement remain blocked.

## Three crash boundaries

1. `BeforeDurableTransitionAppend`: no transition exists. Recovery reacquires
   broker truth and reruns the accepted reducer; no private outcome is
   serialized or reused.
2. `AfterTransitionAppendBeforeCoveringSeal`: recovery locates the existing
   transition by stable key, verifies its payload, performs no second append,
   and constructs, commits, rereads and validates S1 for the current frontier.
3. `AfterCoveringSealBeforeDerivedPublication`: recovery observes the existing
   transition and covering S1 and resumes eligible publication idempotently,
   without append or broker send.

Durable append is necessary but not sufficient for settlement. Stage 7B's
seal-before-settlement rule remains inherited unchanged.

## Closed execution boundary and exit rule

Durable-composition implementation, durable apply/journal mutation,
ACK/readiness publication, Redis-live, FINAM POST/DELETE, broker dispatch,
same-request retry/resend, runtime-live, real orders, Stage 8A-5 and Stage 8B
remain closed.

Independent acceptance of this exact Design R2 may open only a separate
Stage 8A-4 durable-composition implementation specification. It does not open
production implementation or execution.
