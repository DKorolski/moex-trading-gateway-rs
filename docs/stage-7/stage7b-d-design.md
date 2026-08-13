# Stage 7B-d — durable Redis settlement and paper-service composition

Status: design/entry candidate; implementation is not yet claimed.

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
opaque SettlementAuthorized capability
        |
        v
Redis atomic settlement primitive
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

Rows: B-043 through B-056.

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
-> issue one opaque SettlementAuthorized capability
```

The commitment key remains out-of-band and is borrowed for each seal advance;
it is not cloned, serialized or stored in Redis. Seal generation must increase
exactly by one. A seal failure leaves the command pending and makes composite
readiness false.

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

The marker is derived from command stream, consumer group, Redis entry ID,
settlement kind and canonical ACK/DLQ fingerprint. It is transport-only. The
script and all keys must use one validated paper namespace and one Redis
cluster hash slot. Response loss after script commit is resolved by reading the
marker/output identity; it does not repeat publication or semantic effect.

Permanent poison payloads may enter the redacted DLQ path. Authority holds are
not poison and never enter ACK/DLQ/XACK settlement.

## Stage 7B-d-c — composite service readiness and restart transport

Rows: B-064 through B-070.

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
   advance, seal-before-settlement capability and restart ACK reconstruction.
2. `7B-d-b`: atomic/idempotent ACK and DLQ settlement plus response-loss
   recovery on real isolated Redis.
3. `7B-d-c`: composite readiness, task supervision, new-boot PEL reclaim,
   cursor restart and legacy isolation.
4. Independent Stage 7B-d acceptance.
5. `7B-e`: remaining X01-X20 matrix and aggregate B-001..B-080 closure.

## Closed surfaces

- FINAM POST/DELETE: false;
- broker network dispatch: false;
- runtime-live: false;
- real orders: false;
- protective live orders: false;
- external exactly-once execution claim: false.
