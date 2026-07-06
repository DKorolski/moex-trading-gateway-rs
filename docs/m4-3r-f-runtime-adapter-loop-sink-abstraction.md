# M4-3r-f runtime adapter loop / sink abstraction

Status: source-only implementation / in-memory sink / no Redis client /
no strategy invocation / no live orders.

M4-3r-f adds the first paper runtime adapter loop. It composes the previous
source-only pieces:

```text
FINAM-style final M1 Bar
  -> PaperRuntimeBarPublisher
  -> RuntimeBarInput when M10 is complete
  -> PaperRuntimeAdapter
  -> PaperRuntimePublishRecord plan
  -> PaperRuntimePublishSink
```

The only implemented sink is `PaperRuntimeInMemorySink`. There is no Redis
client, no XADD, no consumer group, and no runtime daemon.

## Boundary

Still disabled:

- FINAM `POST /orders`;
- FINAM `DELETE /orders/{id}`;
- command-consumer-to-real-FINAM;
- runtime `LiveReady`;
- continuous runtime-live attachment;
- Stop/SLTP/bracket/replace/multi-leg;
- real Redis XADD;
- strategy invocation;
- durable restore;
- live broker acknowledgement lifecycle.

## Added contracts

M4-3r-f adds to `broker-core::paper`:

```text
PaperRuntimePublishSink
PaperRuntimePublishSinkError
PaperRuntimeInMemorySink
PaperRuntimeAdapterLoop
PaperRuntimeAdapterLoopOutcome
PaperRuntimeAdapterLoopError
```

## Loop behavior

The loop accepts:

```text
source_bar: Bar
intents: Vec<PaperIntent>
sink: &mut impl PaperRuntimePublishSink
```

It then:

1. Passes the source bar into `PaperRuntimeBarPublisher`.
2. If the M10 bucket is not complete, returns `Buffered` and publishes nothing.
3. If the source bar is rejected, returns `SourceRejected` and publishes nothing.
4. If an incomplete bucket is dropped, returns `DroppedIncompleteBucket` and
   publishes nothing.
5. If a complete M10 `RuntimeBarInput` is produced, passes it and supplied
   intents into `PaperRuntimeAdapter`.
6. Publishes each `PaperRuntimePublishRecord` into the provided sink.

## Why this matters

M4-3r-e produced publish plans but did not model the loop that connects M1 input
to runtime state. M4-3r-f proves the orchestration contract without starting any
external process:

```text
source M1 stream -> canonical M10 -> paper runtime adapter -> publish sink
```

That is the shape needed before a real Redis sink can be added.

## Test coverage

The `broker-core::paper` tests now cover:

- M1 bars buffer without sink publish until the M10 bucket completes;
- complete M10 with no intents publishes one runtime state record;
- complete M10 with a paper intent publishes intent/order/trade/position/ack/state;
- source reject publishes nothing;
- incomplete bucket drop publishes nothing;
- loop preserves target position in the adapter ledger.

## What is intentionally not implemented

M4-3r-f does not yet implement:

- Redis XADD sink;
- async runtime loop;
- Redis consumer groups;
- strategy callback invocation;
- state restore from storage;
- risk-gate paper ledger append;
- ALOR oracle comparison;
- live order path.

## Acceptance for M4-3r-f

M4-3r-f is accepted when:

- adapter loop exists;
- sink abstraction exists;
- in-memory sink test coverage exists;
- buffered/rejected/gap paths do not publish;
- complete M10 state-only path publishes runtime state;
- complete M10 intent path publishes paper records;
- strategy invocation remains absent;
- real Redis remains absent;
- live/order boundary remains closed;
- tests and forbidden surface scanners remain green.

Next stage: M4-3r-g Redis sink implementation behind explicit no-live paper
runtime boundary, still without strategy invocation and without live orders.
