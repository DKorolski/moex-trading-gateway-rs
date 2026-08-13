# Stage 7B-d-c — supervised Redis restart/readiness composition

Status: implementation candidate for independent review.

Accepted predecessor: Stage 7B-d-b-R1 at
`e0bf9b7d9eb209e19b875f199511a493ddcd0da9`.

This slice implements frozen rows `B-064..B-070` and supplies the deferred
real-Redis restart evidence for `B-052/B-053`. It attaches only the isolated
paper command consumer. FINAM transport, broker dispatch, runtime-live and
real orders remain closed.

## Authority composition

`Stage7bRedisService` owns one `Stage7bRecoveryReadyOwner`, one Stage 5G
commitment key, the accepted Stage 7A command profile and paper outcome
provider, and the d-b atomic settlement backend. A valid command is decoded
and context-checked, then admitted through the Stage 7B owner. The paper
provider can be called only after Stage 6 has issued its fsync-backed linear
dispatch receipt. Finalization and ACK settlement return through the owner;
Redis entry/group/consumer/cursor facts never enter Stage 6 identity.

Malformed input is classified from the exact payload of the exact consumed
Redis entry. The opaque evidence is immediately consumed by the Stage 7B
observation and DLQ settlement path. There is no independently pairable
poison queue or caller-supplied poison reason.

## Restart transport

Every process boot creates a fresh UUID-based consumer name; two independent
child-process boots prove that the identity is not reused. The process-local
claim cursor starts at `0-0`; bounded `XAUTOCLAIM` pages advance it and reset
only after the scan reaches the tail. The cursor is transport-only. A real
Redis witness proves that old PEL ownership is reclaimed after the configured
idle threshold.

Startup and every iteration query the durable Redis PEL count. Therefore a
fresh process cannot report ready merely because its process-local blocked set
is empty. A response-lost settlement that committed is already absent from
PEL and remains represented by the d-b marker/output; it is not republished.
Redis rollback/failover rollback remains outside the guarantee.

## Composite readiness

`PaperReady` requires all of these at one observation boundary:

- externally supervised consumer task alive;
- writer lease/root namespace valid;
- journal authority and durability state valid;
- current committed recovery seal reread, authenticated and exactly bound to
  the live Stage 6 checkpoint;
- source poll fresh;
- claim scan fresh independently of source polling;
- d-b settlement backend healthy;
- durable PEL count zero;
- no held command or unresolved entry.

Normal return, returned error, panic and explicit task abort all drop the
external liveness guard. Storage/seal uncertainty is sticky and cannot be
healed by later Redis success.

## Restart evidence

The isolated real-Redis test creates old PEL entries under a dead consumer,
then proves bounded one-entry claim pages reach the tail under a new boot
identity. The first command invokes the provider once. After a full owner and
consumer restart:

- an exact duplicate adds no Stage 6 journal record and invokes no provider;
- a conflicting duplicate stays pending, emits no ACK/DLQ, performs no XACK
  and invokes no provider.

Legacy SQLite/M3 state is absent from the dependency and authority graph.
Stage 6 file journal/replay is the only execution authority.

## Still closed

- Stage 7B-d aggregate independent acceptance;
- remaining Stage 7B X01-X20/aggregate closure rows;
- FINAM POST/DELETE and broker network dispatch;
- runtime-live, real orders and protective live orders.
