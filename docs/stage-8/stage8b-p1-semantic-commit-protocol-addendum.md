# Stage 8B-P1 semantic-commit protocol addendum

Status: review candidate, 2026-09-02.

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

- same Redis ID plus the same semantic ID and payload hash is an idempotent
  duplicate;
- same Redis ID with a different semantic ID or payload hash is a terminal
  collision and must be quarantined without a Hybrid callback;
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

The batch ID must be carried unchanged by the Stage 5G checkpoint and, when
intents exist, by the Stage 6 request set, Stage 7B covering seal and command /
ACK correlation evidence. It cannot be derived from a Redis delivery ID,
consumer name, process ID or boot ID.

## Commit order: zero-intent M10

For an M10 that creates no intent, the only accepted order is:

```text
read M10 under the P1 consumer group
  -> validate M10 identity and prior checkpoint
  -> apply exactly one Hybrid semantic transition
  -> export and authenticate the new Stage 5G checkpoint
  -> durably replace the checkpoint and fsync its parent
  -> publish downstream diagnostic runtime state if configured
  -> XACK the M10 entry last
```

An XACK before the authenticated checkpoint is durable is forbidden. A crash
before XACK re-delivers the M10; exact batch/checkpoint identity makes that
delivery idempotent.

## Commit order: intent-producing M10

For a transition that emits one or more intents, the only accepted order is:

```text
validate M10 and apply exactly one Hybrid transition
  -> bind every StrategyRequestId to the semantic batch
  -> append Stage 6 RequestAccepted
  -> append Stage 6 DispatchAttemptRecorded before provider execution
  -> create the Stage 7B covering/cross-binding seal
  -> publish the canonical Stage 7A BrokerCommand envelope
  -> process the paper-provider result through Stage 7B
  -> atomically publish canonical ACK or DLQ and settle the command entry
  -> apply ACK/order/position feedback to the same Hybrid owner
  -> clear pending state only for the exact StrategyRequestId
  -> export the cross-bound authenticated Stage 5G checkpoint
  -> durably commit and fsync the checkpoint and covering evidence
  -> XACK the M10 entry last
```

`StrategyRequestId`, `ClientOrderId` and `BrokerOrderId(String)` remain
distinct. An exact repeated ACK is idempotent. An ACK with a different request,
client, order, batch or command binding fails closed and cannot clear pending
state.

## Crash/replay matrix

| Crash frontier | Required restart behaviour | M10 XACK allowed |
|---|---|---|
| before Hybrid callback | redeliver and execute once | no |
| after callback, before Stage 5G candidate is durable | restore prior checkpoint and deterministically replay | no |
| after zero-intent checkpoint fsync | recognize committed batch; skip callback | yes, last |
| after Stage 6 RequestAccepted | recover accepted request; do not mint another request ID | no |
| after DispatchAttemptRecorded, before provider result | reconcile as uncertain; do not retry or emit success | no |
| after provider result, before Stage 7B terminal seal | recover from durable provider/Stage 6 evidence | no |
| after canonical ACK, before Hybrid feedback | replay exact ACK once into the same pending request | no |
| after Hybrid feedback, before Stage 5G checkpoint fsync | reconstruct from cross-bound evidence; do not duplicate position | no |
| after cross-bound checkpoint fsync, before M10 XACK | recognize exact committed batch and XACK without callback | yes, last |
| after M10 XACK | continue from committed Stage 5G/6/7 frontier | already done |

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
