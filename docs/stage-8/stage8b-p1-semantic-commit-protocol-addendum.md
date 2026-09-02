# Stage 8B-P1 semantic-commit protocol addendum

Status: R1 review candidate, 2026-09-02.

P1-a source implementation was independently accepted at
`6647382bca8950cb1a831cf6082a9f0eacb3bdcc`. R1 changes only the future
P1-b/P1-c semantic protocol. It does not change or reopen P1-a.

This addendum freezes the transaction boundary required before P1-b and P1-c
implementation. It does not authorize a Redis consumer, publication into
operational DB 0, a paper provider, FINAM order transport, broker dispatch or
real orders.

## Selected durable input

P1 uses one separately persisted canonical final-M10 entry as one semantic
transaction. It does not retain ownership of ten source M1 entries through the
Stage 5G/6/7 lifecycle. P0 may continue to consume and acknowledge its current
M1 stream independently.

The fixed P1 Redis namespace is produced by
`stage8b_p1_redis_namespace()`:

```text
hash tag:        finam-imoexf-p1
M10 stream:      finam_imoexf_paper:{finam-imoexf-p1}:market-data:m10
M10 group:       finam-imoexf-p1-m10-lifecycle-v1
command stream:  finam_imoexf_paper:{finam-imoexf-p1}:stage7b:commands
ACK stream:      finam_imoexf_paper:{finam-imoexf-p1}:stage7b:acks
DLQ stream:      finam_imoexf_paper:{finam-imoexf-p1}:stage7b:dlq
settlement keys: finam_imoexf_paper:{finam-imoexf-p1}:stage7b:settlement:*
order stream:    finam_imoexf_paper:{finam-imoexf-p1}:broker:orders
trade stream:    finam_imoexf_paper:{finam-imoexf-p1}:broker:trades
position stream: finam_imoexf_paper:{finam-imoexf-p1}:broker:positions
runtime stream:  finam_imoexf_paper:{finam-imoexf-p1}:runtime:state
health stream:   finam_imoexf_paper:{finam-imoexf-p1}:health
readiness stream: finam_imoexf_paper:{finam-imoexf-p1}:readiness
Stage 7B group:  stage7b-paper-command-consumer-v1
```

The M10 group must be created with `MKSTREAM` before the first P1 M10 publish.
Group creation does not activate a consumer. P1-a only freezes this ordering;
neither operation is performed in operational Redis by the current code.

## Canonical M10 identity

The future publisher must emit an envelope whose schema and semantic identity
are frozen as follows:

```text
schema_version = 1
message_type = CanonicalFinalM10
identity_domain = moex.stage8b.p1.canonical-final-m10.v1
instrument = IMOEXF / IMOEXF@RTSX / moex / futures
timeframe_sec = 600
is_final = true
```

Its canonical payload contains, in this order:

1. schema version and identity domain;
2. operational-identity SHA-256;
3. instrument-map SHA-256;
4. broker, internal symbol, venue symbol, exchange and market;
5. M10 open and close timestamps in UTC milliseconds;
6. canonical decimal strings for open, high, low, close and volume;
7. the ordered identities and payload SHA-256 values of exactly ten final,
   contiguous M1 source bars.

`m10_semantic_id_sha256` is SHA-256 over the UTF-8 domain followed by a NUL
byte and compact canonical JSON of that payload. The Redis stream ID is exactly
`<close_ts_utc_ms>-0`.

- the publisher first attempts `XADD` with that exact ID;
- if Redis reports that the ID already exists or is not greater than the
  stream top, the publisher performs `XRANGE <id> <id>` and requires exactly
  one canonical entry at that ID;
- the same Redis ID plus the same semantic ID and payload hash is an
  idempotent existing publication;
- an absent exact ID, malformed existing entry, or the same ID with a
  different semantic ID or payload hash is a terminal collision and must be
  quarantined as `PaperNotReady` without a Hybrid callback;
- a replacement Redis ID for the same M10 is forbidden;
- non-final, non-M10, non-contiguous or wrong-identity input must be rejected
  before a callback.

## Semantic batch identity

Every accepted M10 creates one batch identifier:

```text
domain = moex.stage8b.p1.semantic-batch.v1
semantic_batch_id_sha256 = SHA256(
    domain || NUL || compact_canonical_json(
        operational_identity_sha256,
        m10_semantic_id_sha256,
        m10_payload_sha256,
        prior_stage5g_checkpoint_sha256
    )
)
```

The batch ID must be carried unchanged by the Stage5G restart package and,
when exactly one intent exists, by the Stage6 request record, Stage7B recovery
seal and command/ACK correlation evidence. It cannot be derived from a Redis
delivery ID, consumer name, process ID or boot ID.

## Initial supported intent envelope

The first P1-b/P1-c implementation slice preserves the accepted Stage 7
maximum-one-unresolved lifecycle rule:

```text
intent_count = 0  -> supported
intent_count = 1  -> supported
intent_count > 1  -> MultiIntentSemanticBatchNotYetSupported
```

An unsupported multi-intent candidate is not persisted as the next semantic
authority, publishes no command, emits no ACK and leaves the M10 unacknowledged.
The service enters `PaperNotReady`, discards the in-memory candidate and
reconstructs the sole Hybrid owner from the prior Stage 7B recovery seal before
any operator-authorized retry. Diagnostic evidence records the batch ID,
intent count, intent kinds, market context and deterministic would-have-been
request identities, but it has no settlement or replay authority.

A later multi-intent batch lifecycle is a separately reviewed feature. It must
reconcile the multi-slot Stage 5G state with Stage 6 cross-binding and the
Stage 7 maximum-one dispatch frontier; it may not weaken that frontier.

## Single-owner lifecycle allocation

`DispatchAttemptRecorded` is exclusively Stage7B-owned. The semantic M10 path
must never append or mint it before command publication. The one accepted
Stage7B owner also owns any pre-publication `RequestAccepted` transition and
the later continuation to `DispatchAttemptRecorded` when it consumes the exact
canonical command.

If the current Stage7B API cannot prepare a cross-bound pre-publication state,
P1-b may add one narrow same-owner API. That API must:

1. validate the exact canonical `BrokerCommand` and semantic-batch binding;
2. append or idempotently reuse only the compatible `RequestAccepted` state;
3. commit a Stage7B recovery seal containing the authenticated Stage5G
   package, compatible Stage6 checkpoint, operational identity and a pending
   publication descriptor sufficient to reproduce the exact command bytes;
4. return one opaque linear publication capability;
5. never append `DispatchAttemptRecorded`, invoke a provider, publish to Redis
   or create another journal/runtime owner.

There is one persistence topology:

```text
Stage7B recovery seal
  -> compatible Stage6 checkpoint
  -> authenticated Stage5G restart package
  -> exact operational identity
  -> pending command publication descriptor when one intent exists
```

An independent Stage5G checkpoint file or second writable journal is
forbidden.

## Commit order: zero-intent M10

For an M10 that creates no intent, the only accepted order is:

```text
read M10 under the P1 consumer group
  -> validate M10 identity and prior checkpoint
  -> apply exactly one Hybrid semantic transition
  -> export and authenticate the new Stage 5G restart package
  -> commit the next Stage 7B recovery seal embedding that package,
     the current Stage 6 checkpoint and exact operational identity
  -> atomically persist, fsync and reread the seal through Stage 7B authority
  -> publish downstream diagnostic runtime state if configured
  -> XACK the M10 entry last
```

An XACK before the cross-bound recovery seal is durable and reread is
forbidden. A crash before XACK re-delivers the M10; exact batch/package/seal
identity makes that delivery idempotent. No standalone Stage5G durable file is
created.

## Commit order: intent-producing M10

For a transition that emits exactly one intent, the only accepted order is:

```text
1. read and validate the canonical M10 under the P1 group
2. apply exactly one Hybrid transition and obtain the exact
   StrategyRequestId and canonical BrokerCommand bytes
3. export the authenticated Stage5G package containing that pending request
4. use the same Stage7B owner to append/reuse compatible RequestAccepted and
   commit the pre-publication recovery seal; do not mint
   DispatchAttemptRecorded
5. fsync and reread that seal before any command publication
6. publish the exact canonical Stage7A BrokerCommand envelope
7. let Stage7B consume that command and exclusively own the transition from
   RequestAccepted to DispatchAttemptRecorded
8. let Stage7B invoke the paper provider only after that transition
9. let Stage7B durably settle the provider outcome and atomically publish the
   canonical ACK or DLQ while settling its command entry
10. apply exact ACK/order/position feedback to the same Hybrid owner and clear
    pending state only for the matching StrategyRequestId
11. export the authenticated post-feedback Stage5G package
12. commit, fsync and reread the next Stage7B recovery seal embedding that
    package and the terminal compatible Stage6 checkpoint
13. XACK the originating M10 entry last
```

The pre-publication seal carries an exact pending-publication identity and
command payload hash/bytes. A restart after that seal but before a confirmed
`XADD` republishes the same bytes with the same StrategyRequestId. It never
repeats the Hybrid callback or creates a new request identity. An exact
duplicate command remains subject to the accepted Stage7B idempotency rules.

`StrategyRequestId`, `ClientOrderId` and `BrokerOrderId(String)` remain
distinct. An exact repeated ACK is idempotent. An ACK with a different request,
client, order, batch or command binding fails closed and cannot clear pending
state.

## Crash/replay matrix

| Crash frontier | Required restart behaviour | M10 XACK allowed |
|---|---|---|
| after M10 read, before Hybrid callback | redeliver the same M10 and deterministically execute one callback | no |
| after callback, before a cross-bound seal | discard the in-memory candidate, restore the prior seal and deterministically replay; publish no command | no |
| after zero-intent seal fsync/reread | recognize the committed batch and skip the callback | yes, last |
| after intent pre-publication seal, before BrokerCommand XADD | recover the pending publication descriptor and republish the exact command bytes with no new request ID | no |
| after BrokerCommand XADD, before Stage7B consumption | leave the Redis command consumable; do not repeat the semantic transition | no |
| after RequestAccepted, before DispatchAttemptRecorded | Stage7B continues the same accepted request; first dispatch remains allowed | no |
| after DispatchAttemptRecorded, before durable provider outcome | do not blindly invoke the provider again; enter reconciliation/uncertain disposition | no |
| after durable provider settlement or ACK publication, before Hybrid feedback | replay the exact canonical ACK into the existing pending Stage5 request; do not execute the provider again | no |
| after Hybrid feedback, before post-feedback seal | recover the prior pending state and replay exact durable feedback without duplicating the provider effect | no |
| after post-feedback seal fsync/reread, before M10 XACK | recognize the post-feedback committed batch and XACK without a new callback/request | yes, last |
| after M10 XACK | transaction is terminal; continue from the committed Stage5G/6/7 frontier | already done |

Conflicting duplicates, missing cross-bindings, non-monotonic checkpoints and
ambiguous provider effects remain unresolved/blocked. They are never converted
to a successful ACK or an XACK.

## Deferred fill policy

This addendum does not select execution prices. Before P1-d implementation a
separate accepted policy must freeze next-bar timing, market and limit fills,
cancel ordering, partial fills, fees, slippage and deterministic paper
order/trade IDs.

## Authorization boundary

P1-b and P1-c remain on hold until this addendum is independently accepted.
P1 DB0 publication and consumer activation remain closed until the complete P1
service passes independent operational acceptance. Existing P0 read-only
market-data/runtime projection in DB0 may remain active.
