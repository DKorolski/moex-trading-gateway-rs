# TZ — Stage 8A-4 durable-composition implementation specification R1

## Authority and scope

Normative predecessor: accepted/closed Durable Composition Design R2 at
`6ddf54ef9d7f740dc59cd2450e78301be3d068cb`; acceptance-review SHA-256:
`160b674d661982b6dbaa6248c2c4acaf883543cb8be99318ef04b0787492f4ba`.

This is a specification-only artifact. Production Rust, durable mutation,
ACK/readiness publication, Redis-live, FINAM POST/DELETE, broker dispatch,
runtime-live and real orders remain closed.

## Existing Stage 6/7 inventory and schema decision

Stage 6 V1 already provides canonical durable request identity, causal record
IDs/sequences, `BrokerOrderObserved`, `BrokerTradeObserved`,
`CancelOutcomeObserved`, `ReconciliationObserved`, `RequestFinalized`,
`ConflictObserved`, checkpoint/frontier and replay. Stage 7B already provides
the authenticated covering-seal barrier before settlement.

V1 does not persist the Design R2 stable transition key, exact lookup query
evidence, complete lifecycle/fill transition kind or deterministic recovery
suffix. Encoding these in `source_evidence_sha256`, marker records or unknown
fields would be lossy and is forbidden.

Therefore implementation requires an additive canonical
`Stage6JournalRecordV2` event kind `ReconciliationTransitionApplied`.

- V1 bytes, record IDs and semantics remain immutable.
- No historical journal rewrite or migration is allowed.
- V2-aware replay must read mixed V1/V2 history in order.
- A V1-only reader must fail closed on V2; skipping unknown records is
  forbidden.
- Canonical V1 goldens remain byte-identical; new V2 and mixed-replay goldens
  are mandatory.

## V2 transition record

The canonical V2 payload stores:

1. stable transition key SHA-256;
2. durable request and private authoritative outcome bindings;
3. endpoint kind (`Place` or `Cancel`) and exact transition kind;
4. exact lookup evidence;
5. broker-order fact and material trade facts retained by the private owner;
6. lifecycle and orthogonal fill effect;
7. complete account-safety binding;
8. exact pre-append CAS evidence;
9. deterministic suffix manifest containing ordered record kinds and canonical
   payload hashes required after the V2 transition.

The key is the canonical hash of only durable request binding, private
authoritative outcome binding and transition kind. It excludes random values,
frontier and post-append generation.

Every exact lookup non-success binds the account, queried `BrokerOrderId`,
durable request, request/response timing and status category. `NotAttempted`
uses an explicit absent-attempt shape; attempted failure cannot be represented
as `NotAttempted`.

## Deterministic append batch and restart

Under the existing single writer lease:

```text
validate F0 + S0 + request state + historical arm provenance + controls
-> search V2 records for stable key
-> compare-and-append ReconciliationTransitionApplied V2
-> append/fsync the exact deterministic V1 suffix, if any
-> obtain final F1
-> commit S1 covering F1
-> reread/authenticate/canonical/checkpoint-validate S1
-> only then derive eligible publication authority
```

The V2 transition is always the first record in the batch. Its suffix manifest
makes a crash after any individual append recoverable without the lost
process-local outcome. Recovery finds the transition by its persisted stable
key, verifies payload and already-present suffix, appends only the missing
canonical suffix, then creates S1. It never appends a second V2 transition.

Same key and same payload resumes or returns the existing batch. Same key and
different payload is a hard conflict. Seal failure leaves the complete or
partial batch durable and settlement pending; recovery completes suffix/seal
without broker send.

## PLACE disposition table

| Transition | Ordered durable semantic suffix | Request disposition | Canonical ACK after S1 |
|---|---|---|---|
| `ExactWorking` | broker order + any material trade facts | `Completed` | `Recovered / RecoveredByBrokerTruth` |
| `ExactTerminalFilled` | broker order + material trade facts | `Completed` | `Recovered / RecoveredByBrokerTruth` |
| `ExactTerminalRejected` | broker order fact | `Rejected` | `Rejected / BrokerRejected` |
| `ExactTerminalCancelled` | broker order + any material trade facts | `Completed` | `Recovered / RecoveredByBrokerTruth` |
| `ExactTerminalExpired` | broker order + any material trade facts | `Completed` | `Recovered / RecoveredByBrokerTruth` |
| `ReconciliationConflictHold` | none | none | forbidden; no XACK |
| `ReconciliationStillUnknownHold` | none | none | forbidden; no XACK |

PLACE command finalization is independent from the broker order remaining
working. This preserves the accepted Stage 7 rule that a finalized LIMIT PLACE
may expose a working order for a later sequential CANCEL.

## CANCEL disposition table

| Target transition | Ordered durable semantic suffix | Request disposition | Canonical ACK after S1 |
|---|---|---|---|
| `ExactWorking` | none | none; unresolved | forbidden; no XACK |
| `ExactTerminalFilled` | `CancelOutcomeObserved::ExecutionObserved` | `Completed` | `Recovered / RecoveredByBrokerTruth` |
| `ExactTerminalRejected` | `CancelOutcomeObserved::AlreadyTerminalNonExecution` | `Completed` | `Recovered / RecoveredByBrokerTruth` |
| `ExactTerminalCancelled` | `CancelOutcomeObserved::Canceled` | `Completed` | `Recovered / RecoveredByBrokerTruth` |
| `ExactTerminalExpired` | `CancelOutcomeObserved::AlreadyTerminalNonExecution` | `Completed` | `Recovered / RecoveredByBrokerTruth` |
| `ReconciliationConflictHold` | none | none | forbidden; no XACK |
| `ReconciliationStillUnknownHold` | none | none | forbidden; no XACK |

`ExactWorking` means the target order is still active; it cannot prove the
CANCEL command terminal. A target `TerminalRejected` is not a CANCEL rejection:
it is already-terminal non-execution. Explicit CANCEL endpoint rejection
remains a separate endpoint-observation path.

## Private owner and authority flow

The implementation owner invokes the accepted reducer internally and consumes
the admitted request/truth/policy values. It retains the selected broker order,
material trades, exact lookup evidence and account safety in a private opaque
non-Clone/non-Serialize linear outcome. The public diagnostic is emitted only
as side evidence and cannot reach the transition builder.

At apply time, the owner validates the exact durable request/state, Stage 7
command payload, F0, S0 generation/fingerprint, historical arm provenance and
scope, account safety and control state. Identity substitution or conflicting
re-arm invalidates the outcome. Arm expiry, `StopRequested`, or stale/unreadable
kill-switch state do not block reconciliation append; they block new send and
readiness. Reconciliation has no transport capability.

## Publication and readiness

No terminal ACK authority exists until `RequestFinalized` is durable and S1
covers its frontier. Holds and CANCEL `ExactWorking` remain unresolved and
cannot produce terminal ACK, XACK or readiness success. Exact target truth does
not clear account-wide active/unknown/orphan holds or existing readiness owners.

The public diagnostic, V2 record alone, append receipt alone and pre-append S0
are each insufficient publication authority.

## Implementation slices after specification acceptance

Acceptance opens only one slice at a time:

1. **I1 — additive schema/codec/replay:** V2 canonical types, mixed V1/V2
   decoder/replay and goldens; no writer path or durable apply.
2. **I2 — private composition/builder:** private linear owner and deterministic
   transition/suffix construction; no append.
3. **I3 — durable batch/seal/recovery:** CAS append, suffix recovery and S1;
   no Redis or broker transport.
4. **I4 — derived ACK/readiness facade:** capability derivation only after
   finalization + S1; Redis-live remains closed.

Acceptance of this specification opens only I1. Each later slice requires its
own immutable handoff and independent acceptance.

## Closed surfaces

No implementation slice authorizes FINAM POST/DELETE, same-request resend,
re-arm, Redis-live command consumption, broker dispatch, runtime-live, real
orders, Stage 8A-5 or Stage 8B. Those require later explicit gates.
