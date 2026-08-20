# Stage 8A-4 durable composition I4 design R1

## Authority and scope

I3 R6 is independently accepted and closed at
`593ff255ef7826a22e66c9aff6f7ea47acf47644`. Its acceptance review SHA-256 is
`1da167c3e7f1266473133d2d8a1412906a26d7f83b5dc026ce84dc7969090257`.
This design opens only a no-I/O derived terminal-ACK/readiness facade. It does
not open ACK publication, Redis XACK, FINAM transport, broker dispatch,
runtime-live, retry, resend, re-arm or real orders.

I4 composes two independent authorities:

1. durable terminal authority reconstructed under the Stage 7B writer lease
   from a complete Stage8A4 V2 transition, its exact V1 suffix, final F1 and an
   authenticated covering S1;
2. current readiness evidence sampled after durable terminal reconstruction.

Historical settlement cannot imply current readiness. Current readiness cannot
create, change or suppress the canonical durable ACK identity.

## Type-state boundary

The implementation must introduce separate non-`Clone`, non-`Copy`,
non-`Debug`, non-`Serialize`, non-`Deserialize` internal types:

- `Stage7bStage8a4TerminalAuthority` — broker-neutral durable authority;
- `Stage8a4I4CurrentReadinessEvidence` — current no-send readiness evidence;
- `Stage8a4I4DerivedAckReadinessFacade` — the consumed composition result.

There is no public constructor, field access, raw journal input, caller-built
seal, caller-built checkpoint or digest-only constructor. The facade exposes at
most a bounded redacted diagnostic. It is not accepted by Redis settlement,
FINAM transport, execution capability minting or a runtime-live attachment.

## Durable terminal derivation

The Stage 7B owner must reread and authenticate its current on-disk seal,
refresh the version-aware mixed replay and prove all of the following:

- exactly one reconciliation V2 exists for the durable request;
- its deterministic suffix is `Complete`;
- the recovered request contains the required `RequestFinalized` record;
- the final request record and full mixed frontier are covered by current S1;
- operational identity and runtime fingerprint remain exact-bound;
- the accepted command payload and request identity remain exact-bound;
- the V2 transition is `Exact`, never either hold variant.

An I3 receipt is not sufficient by itself and is not required after restart.
The authority is reconstructed from durable history. Pending, partial,
complete-but-uncovered, corrupt, conflicting or hold history produces no
terminal authority.

The stable terminal ACK identity excludes unrelated later seal generations so
restart and unrelated durable advancement reconstruct the same identity. A
separate settlement-authority fingerprint may bind the current covering seal.

## Canonical ACK mapping

PLACE:

| Durable transition | Final disposition | Derived status | Reason |
|---|---|---|---|
| ExactWorking | Completed | Recovered | RecoveredByBrokerTruth |
| ExactTerminalFilled | Completed | Recovered | RecoveredByBrokerTruth |
| ExactTerminalCancelled | Completed | Recovered | RecoveredByBrokerTruth |
| ExactTerminalExpired | Completed | Recovered | RecoveredByBrokerTruth |
| ExactTerminalRejected | Rejected | Rejected | BrokerRejected |

CANCEL:

| Durable transition | Required cancel outcome | Derived status | Reason |
|---|---|---|---|
| ExactWorking | none | none | unresolved |
| ExactTerminalFilled | ExecutionObserved | Recovered | RecoveredByBrokerTruth |
| ExactTerminalRejected | AlreadyTerminalNonExecution | Recovered | RecoveredByBrokerTruth |
| ExactTerminalCancelled | Canceled | Recovered | RecoveredByBrokerTruth |
| ExactTerminalExpired | AlreadyTerminalNonExecution | Recovered | RecoveredByBrokerTruth |

`ReconciliationConflictHold` and `ReconciliationStillUnknownHold` derive no
terminal ACK, Redis XACK, settlement, retry, resend or readiness authority.
The command ACK describes command finalization; it does not rewrite the broker
order lifecycle.

## Current readiness derivation

Readiness is sampled independently after terminal authority exists. `Ready`
requires a fresh current control with `RunAllowed`, current Stage 7B composite
readiness, fresh broker truth/readiness, open schedule, exact operational and
runtime identity, no unresolved account unknown/orphan safety and all frozen
Stage8A1 readiness rules.

The following always yield blocked readiness while preserving an already valid
historical terminal ACK:

- `StopRequested`;
- missing, unreadable, malformed or stale current control;
- stale/degraded composite readiness;
- stale broker truth or broker readiness;
- closed/unknown schedule;
- account unknown/orphan ambiguity;
- operational identity or runtime fingerprint mismatch.

No I3 post-effect control snapshot can be reused as current readiness evidence.
No blocked state is normalized to ready.

## Restart and duplicate semantics

Normal completion, immediate duplicate derivation and fresh-process restart
must produce the same canonical terminal ACK identity and semantic payload.
Repeated derivation performs no journal append, no second finalization and no
broker effect. Publication knowledge, when added by a later separately accepted
slice, may classify canonical/duplicate/conflict but cannot change durable ACK
facts.

## Required implementation order

1. Add broker-neutral completed-transition facts to the mixed replay boundary.
2. Add owner-mediated Stage 7B terminal authority after exact S1 validation.
3. Add private FINAM-gateway composition with independently sampled readiness.
4. Add PLACE/CANCEL/hold/Pending/restart/duplicate matrices and compile-fail
   privacy tests.
5. Add I4 implementation checker, negative harness and immutable handoff.

Each step remains no-I/O. Actual ACK publication or Redis XACK requires a later
explicitly accepted slice.

The design gate proves production/Cargo equality directly against accepted I3
R6. It intentionally does not edit or invoke the repository-wide legacy
forbidden scanner whose frozen baseline predates the accepted Stage 6/7
workspace additions; changing that historical scanner is outside this slice.

## Closed surfaces

- Redis command consumption and ACK/XACK settlement;
- Redis live publication;
- FINAM POST/DELETE and all transport sends;
- broker dispatch and retry/resend/re-arm;
- execution-capability minting from I4;
- runtime-live and real strategy orders;
- Stage 8A-5 and Stage 8B.
