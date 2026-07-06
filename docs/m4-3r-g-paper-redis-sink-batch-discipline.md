# M4-3r-g paper Redis sink / batch failure discipline

Status: paper-only Redis sink implementation / no strategy invocation /
no runtime-live attachment / no live orders.

M4-3r-g adds a Redis publication boundary for the paper runtime records produced
by M4-3r-f. The implementation lives in `finam-gateway`, because Redis transport
is already a gateway concern, while `broker-core` remains broker/domain-only.

The intended flow is still paper-only:

```text
FINAM-style final M1 Bar
  -> canonical M10 RuntimeBarInput
  -> PaperRuntimeAdapter
  -> PaperRuntimePublishRecord batch
  -> PaperRuntimeRedisSink
  -> paper Redis streams only
```

## Paper stream allowlist

`PaperRuntimeRedisSink` only accepts records targeting the known paper streams:

```text
finam_imoexf_paper:runtime:intents
finam_imoexf_paper:runtime:paper_acks
finam_imoexf_paper:runtime:orders_paper_only
finam_imoexf_paper:runtime:trades_paper_only
finam_imoexf_paper:runtime:positions_paper_only
finam_imoexf_paper:runtime:state:hybrid_intraday:imoexf
```

It also writes batch markers to:

```text
finam_imoexf_paper:runtime:publish_batches
```

Any non-paper stream is rejected before the first Redis publish. Any
stream/payload mismatch is also rejected before the first Redis publish.

## Batch discipline

M4-3r-g intentionally does not publish records as an unstructured loop. It uses:

- deterministic `batch_id`;
- deterministic `batch_sha256`;
- deterministic payload-level `idempotency_key`;
- pending batch marker;
- paper record envelopes;
- committed batch marker.

The sequence is:

```text
XADD publish_batches Pending(batch_id)
XADD target paper stream record[0]
...
XADD target paper stream record[n]
XADD publish_batches Committed(batch_id)
```

The actual Redis transport is still the existing `RedisStreamSink` /
`RedisConnectionStreamSink` boundary. M4-3r-g adds the paper-only validation and
batch envelope around it.

## Partial failure semantics

If publication fails:

- before the pending marker, no batch is visible;
- after pending marker but before committed marker, the batch is partial;
- the error is reported as `PaperRuntimeRedisSinkError::PartialFailure`;
- the failure report includes batch id/hash, phase, published entry count, and a
  redacted error kind;
- raw Redis errors are not exported;
- retry is marked as requiring reconciliation.

This is deliberately conservative. A future restart/replay step can use
`batch_id`, `batch_sha256`, `idempotency_key`, and marker phase to decide whether
to complete, suppress, or rebuild the batch.

## Boundary

Still disabled:

- FINAM `POST /orders`;
- FINAM `DELETE /orders/{id}`;
- command-consumer-to-real-FINAM;
- runtime `LiveReady`;
- continuous runtime-live attachment;
- strategy invocation;
- Stop/SLTP/bracket/replace/multi-leg;
- live broker acknowledgement lifecycle.

## Added contracts

M4-3r-g adds:

```text
PaperRuntimeRedisSinkConfig
PaperRuntimeRedisSink
PaperRuntimeRedisPayloadKind
PaperRuntimeRedisBatchMarker
PaperRuntimeRedisBatchMarkerPhase
PaperRuntimeRedisRecordEnvelope
PaperRuntimeRedisBatchPlan
PaperRuntimeRedisPublishOutcome
PaperRuntimeRedisPublishFailurePhase
PaperRuntimeRedisPartialFailureReport
PaperRuntimeRedisSinkError
```

## Test coverage

The `finam-gateway` tests cover:

- successful batch publish with pending/committed markers;
- non-paper stream rejected before first publish;
- stream/payload mismatch rejected before first publish;
- partial failure after record publish is reported with redacted failure report;
- retry planning produces stable batch id/hash and idempotency keys.

## What is intentionally not implemented

M4-3r-g does not yet implement:

- strategy callback invocation;
- runtime daemon;
- durable restart replay;
- consumer-side idempotent replay;
- command consumer;
- real FINAM order endpoint calls;
- live trading.

Next stage: M4-3r-h local paper runtime loop wiring, using the Redis sink behind
the same no-live boundary and still without strategy invocation.
