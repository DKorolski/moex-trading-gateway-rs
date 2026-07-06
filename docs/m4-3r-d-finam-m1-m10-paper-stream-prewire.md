# M4-3r-d FINAM M1-to-M10 paper stream prewire

Status: source-only implementation / no-send / no Redis writes / no live orders.

M4-3r-d adds the broker-neutral in-memory prewire between FINAM WS M1 bars and
the M4-3r-c paper ledger executor. It does not start a runtime daemon and does
not publish to Redis yet. It proves the contract that a later adapter will use:

```text
FINAM WS final M1 bars
  -> CanonicalBarAggregator(target = 600s)
  -> final canonical M10 bar
  -> RuntimeBarInput(origin = Live, provenance = FinamDerivedM1ToM10)
  -> future paper runtime adapter
```

## Boundary

Still disabled:

- FINAM `POST /orders`;
- FINAM `DELETE /orders/{id}`;
- command-consumer-to-real-FINAM;
- runtime `LiveReady`;
- continuous runtime-live attachment;
- Stop/SLTP/bracket/replace/multi-leg;
- Redis paper stream publication;
- strategy invocation;
- live broker acknowledgement lifecycle.

The new code only returns an in-memory `PaperRuntimeBarPublishOutcome`.

## Added contracts

M4-3r-d adds to `broker-core::paper`:

```text
PaperRuntimeBarPublisherConfig
PaperRuntimeBarPublisher
PaperRuntimeBarPublishOutcome
PaperRuntimeBarPublishRejectReason
```

The default FINAM paper config is:

```text
PaperRuntimeBarPublisherConfig::finam_m1_to_m10_paper(...)
source_timeframe_sec = 60
target_timeframe_sec = 600
provenance = FinamDerivedM1ToM10
safety_boundary = PaperSafetyBoundary::closed()
```

## Acceptance gate

The publisher accepts only:

```text
source_kind = LiveStream
source_timeframe_sec = 60
is_final = true
instrument = configured target instrument
safety_boundary = closed
```

It rejects:

- open safety boundary;
- wrong instrument;
- non-live source kind;
- non-final bars;
- native M10 bars;
- wrong source timeframe;
- invalid target timeframe;
- aggregation gaps/non-contiguous bars.

## Output contract

When the 10th contiguous final M1 bar completes a bucket, the publisher returns:

```text
PaperRuntimeBarPublishOutcome::Published {
  target_stream,
  runtime_input: RuntimeBarInput {
    origin = Live,
    timeframe_sec = 600,
    is_final = true,
    source_stream = target_stream,
    provenance = FinamDerivedM1ToM10,
    ...
  }
}
```

Raw M1 bars never become `RuntimeBarInput`. Before the 10-minute bucket is
complete, the outcome is `Buffered`.

## Gap behavior

If a new bucket starts before the current bucket is complete, the publisher
returns:

```text
DroppedIncompleteBucket
```

This preserves the previous M4 market-data recovery contract: incomplete M10
bars must not reach the strategy-facing runtime boundary.

## Relationship to M4-3r-c

M4-3r-c executes paper fills from a `RuntimeBarInput`. M4-3r-d provides the safe
source of those runtime bars:

```text
PaperRuntimeBarPublisher::observe_source_bar(...)
  -> Published(RuntimeBarInput)
  -> PaperLedgerSnapshot::apply_next_bar_open_market_intent(...)
```

The second arrow is not wired automatically in M4-3r-d. That belongs to the next
runtime adapter stage.

## What is intentionally not implemented

M4-3r-d does not yet implement:

- Redis XADD to `finam_imoexf_paper:md:bars:10m`;
- runtime consumer loop;
- strategy invocation;
- paper intent generation from IMOEXF hybrid;
- durable replay/restart state;
- ALOR oracle comparison;
- live order path.

## Test coverage

The `broker-core::paper` tests now cover:

- M1 bars buffer until a complete M10 bucket;
- complete bucket publishes `RuntimeBarInput`;
- output uses `FinamDerivedM1ToM10` provenance;
- raw/non-final bars are rejected;
- non-live/recovery bars are rejected;
- FINAM-native M10 is rejected for this path;
- incomplete buckets are dropped on gap;
- open safety boundary is rejected.

## Acceptance for M4-3r-d

M4-3r-d is accepted when:

- FINAM M1-to-M10 paper prewire exists in broker-neutral core;
- raw M1 cannot reach strategy-facing runtime input;
- only complete final M10 runtime input is produced;
- provenance is explicit;
- boundary remains no-send/no-Redis/no-live;
- tests and forbidden surface scanners remain green.

Next stage: M4-3r-e paper runtime adapter skeleton / Redis prewire, still no live orders.
