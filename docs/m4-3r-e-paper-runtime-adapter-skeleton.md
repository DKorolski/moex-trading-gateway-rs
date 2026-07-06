# M4-3r-e paper runtime adapter skeleton / Redis prewire

Status: source-only implementation / no Redis writes / no strategy invocation /
no live orders.

M4-3r-e adds a broker-neutral paper runtime adapter skeleton. It bridges the
M4-3r-d `RuntimeBarInput` and the M4-3r-c deterministic paper ledger executor,
but still does not run the IMOEXF strategy and does not publish to Redis.

The adapter returns a publish plan:

```text
PaperRuntimePublishRecord {
  stream,
  payload
}
```

This is a Redis prewire contract only. A later adapter can XADD these records,
but M4-3r-e does not do that.

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
- live broker acknowledgement lifecycle.

## Added contracts

M4-3r-e adds to `broker-core::paper`:

```text
PaperRuntimeStreams
PaperRuntimeAdapterConfig
PaperRuntimePublishPayload
PaperRuntimePublishRecord
PaperRuntimeAdapterOutcome
PaperRuntimeAdapter
PaperRuntimeAdapterError
```

The default FINAM paper streams are:

```text
finam_imoexf_paper:runtime:state:hybrid_intraday:imoexf
finam_imoexf_paper:runtime:intents
finam_imoexf_paper:runtime:paper_acks
finam_imoexf_paper:runtime:orders_paper_only
finam_imoexf_paper:runtime:trades_paper_only
finam_imoexf_paper:runtime:positions_paper_only
```

## Adapter input

The adapter accepts:

```text
RuntimeBarInput
Vec<PaperIntent>
```

`Vec<PaperIntent>` is supplied by a future strategy adapter. M4-3r-e does not
invoke strategy code and does not generate strategy decisions.

## Adapter gates

The adapter rejects:

- open safety boundary;
- instrument mismatch;
- provenance other than `FinamDerivedM1ToM10`;
- non-live runtime input;
- non-final runtime input;
- wrong timeframe;
- executor errors from the paper ledger.

## Publish plan behavior

If there are no intents, the adapter returns a state-only publish plan:

```text
runtime_state_stream -> PaperRuntimeState
```

If a market paper intent is supplied, the adapter calls:

```text
PaperLedgerSnapshot::apply_next_bar_open_market_intent(...)
```

and returns publish records for:

```text
intents_stream       -> PaperIntent
orders_stream        -> PaperOrder
trades_stream        -> PaperTrade
positions_stream     -> PaperPosition
paper_acks_stream    -> PaperAck
runtime_state_stream -> PaperRuntimeState
```

If an intent is a duplicate by `RuntimeDecisionId`, the adapter returns:

```text
paper_acks_stream    -> PaperAck(kind = DuplicateIgnored)
runtime_state_stream -> PaperRuntimeState
```

and does not append a second order/trade/position.

## Relationship to ALOR parity

This stage brings the FINAM paper contour closer to the ALOR runtime shape:

- paper state stream exists as a contract;
- intent/order/trade/position/ack stream names exist as a contract;
- synthetic paper position feedback is represented in payloads;
- target-instrument paper position is maintained by the ledger;
- state-only bars can still refresh runtime state without broker action.

It is still not operational parity: no strategy invocation, no Redis loop, no
restart restore, and no ALOR-vs-FINAM runtime comparison are implemented yet.

## What is intentionally not implemented

M4-3r-e does not yet implement:

- Redis XADD;
- runtime consumer group;
- strategy callback invocation;
- warmup/history restore;
- durable ledger restore;
- risk-gate paper ledger append;
- ALOR oracle comparison;
- live order path.

## Test coverage

The `broker-core::paper` tests now cover:

- state-only runtime bar produces only runtime state publish plan;
- market paper intent produces intent/order/trade/position/ack/state publish plan;
- duplicate intent publishes duplicate ack and no second fill;
- bad provenance is rejected;
- non-live runtime input is rejected;
- open safety boundary is rejected.

## Acceptance for M4-3r-e

M4-3r-e is accepted when:

- paper runtime adapter skeleton exists;
- Redis stream names are broker-neutral publish-plan data, not writes;
- state-only path works;
- paper intent path uses deterministic ledger executor;
- duplicate idempotency is preserved;
- strategy invocation remains absent;
- live/order boundary remains closed;
- tests and forbidden surface scanners remain green.

Next stage: M4-3r-f runtime adapter loop / Redis sink abstraction, still no live orders.
