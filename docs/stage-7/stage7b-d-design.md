# Stage 7B-d — durable Redis settlement and paper-service composition

Status: Design R1 authority-clarification candidate; implementation is not yet
opened or claimed.

Accepted predecessor:
`c57ae8d5f98bbb11df0a81f78262d3916b276d81`
(Stage 7B-c-R1 / Stage 7B-c CLOSED).

## Objective

Compose the accepted Stage 7A paper Redis transport with the accepted Stage
7B file-backed recovery owner without creating a second lifecycle authority.
Stage 6 remains the only command/order lifecycle authority. Redis entry IDs,
consumer names, claim cursors and settlement markers remain transport-only.

This design targets rows B-043 through B-070. It does not claim them
implemented and does not claim the final
Stage 7B X01-X20 aggregate crash matrix or Stage 7B closure.

## Non-negotiable ownership model

```text
Stage7bRecoveryReadyOwner
  owns Stage6dDurableRuntimeRecovered
  owns file journal
  owns writer/root lease
  owns current authenticated recovery-seal state
        |
        | narrow phase methods; no mutable extractor
        v
Stage 7B-d paper command coordinator
        |
        | after durable finalization and committed seal only
        v
opaque linear DurableAckAuthorized capability
        |
        | combine with one exact Redis source-entry context
        v
opaque linear RedisAckSettlementPlan
        |
        v
Redis atomic settlement primitive

Permanent pre-Stage6 poison follows a separate, mutually exclusive path:

```text
deterministic decode/schema failure
  -> proof of no Stage6 admission or journal/request mutation
  -> opaque linear PoisonDlqAuthorized
  -> Redis atomic redacted DLQ + XACK
```
```

Forbidden designs:

- moving `Stage6dDurableRuntimeRecovered` out of the recovery owner;
- exposing `recovered_mut`, raw journal or writer-lease extractors;
- retaining a competing Stage 6 authority in the Redis consumer;
- treating process-memory ACK maps as restart authority;
- using Redis settlement identity as `StrategyRequestId`, `ClientOrderId` or
  Stage 6 execution identity;
- publishing with best-effort `XADD` followed by a separate `XACK`;
- crossing ACK/DLQ/XACK on `RecoveryBlocked`, identity conflict,
  reconciliation hold or durability uncertainty.

## Stage 7B-d-a — lifecycle and seal barrier

Rows eligible for closure: B-043 through B-051 and B-054 through B-056.
B-052/B-053 remain pending until Stage 7B-d-c supplies their required
real-Redis restart witnesses. Pure semantic helpers for those rows do not close
them.

The recovery owner receives narrow broker-neutral paper lifecycle methods that
delegate directly to accepted Stage 6 functions. A provider callback can be
invoked only after `RequestAccepted` and `DispatchAttemptRecorded` have been
durably synced by the owned file journal.

After `RequestFinalized`, settlement authority is unavailable until the owner
performs this exact sequence:

```text
derive current Stage6 checkpoint from the owned journal frontier
-> authenticate the preceding Stage6 restart package
-> reseal the same Stage5G authority at the current checkpoint
-> build next-generation Stage7B recovery seal
-> temp write
-> temp sync_all
-> atomic rename
-> root-directory sync_all
-> committed bytes reread
-> canonical/HMAC/checkpoint validation against the live owner
-> issue one opaque linear DurableAckAuthorized capability
```

The commitment key remains out-of-band and is borrowed for each seal advance;
it is not cloned, serialized or stored in Redis. Seal generation must increase
exactly by one. A seal failure leaves the command pending and makes composite
readiness false. If rename/fsync outcome is ambiguous, no settlement capability
is minted, readiness is false and cached `generation + 1` retry is forbidden.
The owner must reread and validate the committed on-disk seal or be restarted
and reconstructed before another advance is attempted.

`DurableAckAuthorized` is crate-private, non-Clone, non-Copy,
non-Serialize/non-Deserialize, non-reconstructible from Redis or process input,
and single-consumption/linear. It binds the exact operational identity digest,
`StrategyRequestId`, canonical command digest, durable client/request identity,
Stage6 final disposition and ACK classification, current Stage6
checkpoint/frontier fingerprint, committed seal generation and commitment,
canonical ACK fingerprint, and settlement kind `ACK`. Authority for request A
cannot settle request B.

Combining that durable authority with one validated paper transport context
produces a linear `RedisAckSettlementPlan`. The plan additionally binds command
stream, consumer group, Redis entry ID and validated namespace. These transport
facts remain transport-only and never feed Stage6 execution identity.

Canonical terminal ACK semantics are reconstructed from Stage 6 replay facts.
The first transport publication remains canonical until durable Redis
settlement proves it was published. Only a later exact duplicate may receive
`Duplicate`. Process-local `ack_publications` and
`canonical_ack_recoveries` are not accepted restart evidence.

## Stage 7B-d-b — atomic Redis settlement

Rows: B-057 through B-063.

ACK and redacted DLQ use one reviewed Lua primitive. In one Redis atomic
operation it:

1. validates or creates an entry settlement marker;
2. validates or creates the request-level canonical-publication marker for an
   ACK;
3. publishes exactly one ACK or DLQ record for that settlement identity;
4. performs `XACK` for the command entry;
5. returns the already committed result on an exact retry;
6. rejects marker/fingerprint conflicts without `XACK`.

The stable entry-settlement key is derived only from the validated paper
namespace/hash tag, command stream, consumer group, Redis entry ID and
settlement kind. The key never includes the proposed ACK/DLQ payload
fingerprint. Its value stores schema/version, exact payload fingerprint,
published output stream and Redis output ID, plus canonical/duplicate
classification where applicable. Therefore the same key and same fingerprint
returns the committed result without a second `XADD`, while the same key with a
different fingerprint fails before `XADD` or `XACK`.

The request-level canonical ACK publication marker is stable across Redis
entries carrying the same exact request. It records stable request lookup
identity, canonical terminal ACK fingerprint, canonical output Redis ID and
publication-known state. First publication creates the canonical marker. A
same-entry response-loss retry returns its entry marker. A later exact duplicate
entry receives a Duplicate/DuplicateCommand ACK without changing the canonical
marker. A conflicting duplicate receives no ACK, DLQ or XACK.

For a brand-new entry marker, the Lua primitive validates all key types,
schema, hash slot, arguments, marker conflicts and payload fingerprints, and
proves that the source entry is pending in the expected consumer group before
its first mutation. An exact already-committed marker retry does not require PEL
membership. No expected semantic/type/conflict error is reachable after the
first mutation. Lua atomicity is not treated as rollback.

The script and all keys use one validated paper namespace and one intentional
Redis cluster hash slot. B-059 response-loss idempotency covers a committed
script whose client/process loses the response. Source stream, markers and
ACK/DLQ streams must share one Redis durability/failover domain; Redis storage
rollback after server/storage/failover failure is outside that guarantee and is
not described as exactly-once transport durability.

Permanent pre-Stage6 poison payloads use only `PoisonDlqAuthorized`, which is
entry-bound, reason-bound, redacted-payload-fingerprint-bound, crate-private,
linear and non-serializable. It requires proof that Stage6 admission did not
occur and that no Stage6 journal/request mutation exists for the entry. Because
Stage6 state did not change, poison settlement does not advance or fabricate a
Stage6 recovery seal. IdentityConflict, ConflictingDuplicate,
ReconciliationRequired, RecoveryBlocked, DurabilityUncertain, provider
uncertainty and all post-admission authority holds are never poison and never
enter ACK/DLQ/XACK settlement.

## Stage 7B-d-c — composite service readiness and restart transport

Rows: B-064 through B-070, plus final real-Redis restart closure of B-052 and
B-053.

`PaperReady` requires all of the following at the same observation boundary:

- externally supervised consumer task is alive;
- writer/root/lock namespace remains valid;
- the file journal remains valid and durability-certain;
- the on-disk committed seal exists, rereads canonically, validates its HMAC,
  generation and checkpoint against the live owner;
- Stage 5G/Stage 6 recovery authority is ready;
- Redis source poll and claim scan are independently fresh;
- atomic settlement backend is available and not uncertain;
- no held command or unresolved settlement exists.

Normal return, returned error, panic and explicit abort/cancel all clear the
external liveness authority. Every process boot uses a new transport consumer
name. A fresh process-local claim cursor may start at `0-0`; bounded repeated
`XAUTOCLAIM` must eventually cover the eligible old PEL without changing any
execution identity.

Legacy SQLite/M3 order-path state is excluded from the Stage 7B dependency and
authority graph. Stage 6 file journal/replay remains authoritative.

## Crash and recovery policy

The d implementation must directly cover:

- crash after accepted and before dispatch record;
- crash after dispatch and before provider;
- crash during an uncertain provider effect;
- crash after durable outcome and before finalization;
- crash after finalization and before seal;
- crash after committed seal and before Redis settlement;
- Redis settlement response loss;
- restart with exact and conflicting duplicates;
- sequential correlated cancel after restart.

No test may describe paper-provider exactly-once execution. The accepted claim
is durable at-most-once blind dispatch: after a durable dispatch attempt with
unknown outcome, restart holds for reconciliation.

## Planned implementation order

1. `7B-d-a`: recovery-owner lifecycle facade, authenticated checkpoint
   advance, exact linear seal-before-settlement authority and restart ACK
   reconstruction. It may close B-043..B-051 and B-054..B-056 only.
2. `7B-d-b`: atomic/idempotent ACK and DLQ settlement plus response-loss
   recovery on real isolated Redis; it owns B-057..B-063.
3. `7B-d-c`: composite readiness, task supervision, new-boot PEL reclaim,
   cursor restart and legacy isolation; it owns B-064..B-070 and the required
   real-Redis restart closure of B-052/B-053.
4. Independent Stage 7B-d acceptance.
5. `7B-e`: remaining X01-X20 matrix and aggregate B-001..B-080 closure.

## Closed surfaces

- FINAM POST/DELETE: false;
- broker network dispatch: false;
- runtime-live: false;
- real orders: false;
- protective live orders: false;
- external exactly-once execution claim: false.
