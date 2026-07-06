# M4-3r-h ALOR-style paper runtime consumer groups

Status: consumer-group lifecycle contract / paper-only / no strategy invocation /
no runtime-live attachment / no live orders.

M4-3r-h ports the Redis consumer-group lifecycle pattern from the mature ALOR
runtime/gateway into the FINAM paper runtime contour.

The ALOR oracle pattern is:

```text
XGROUP CREATE <stream> <group> 0 MKSTREAM
XREADGROUP GROUP <group> <consumer> BLOCK <ms> COUNT <n> STREAMS <stream> >
XAUTOCLAIM <stream> <group> <consumer> <idle_ms> 0-0 COUNT <n>
XACK <stream> <group> <message_id>
DLQ on decode/poison failure
```

M4-3r-h does not yet run the continuous loop. It fixes the command/lifecycle
contract that the later runner must follow.

## Source stream

The paper runtime consumes only the FINAM paper WS market-data stream:

```text
finam_imoexf_paper:ws:market_data
```

This stream contains broker-neutral `MarketData` envelopes. The future runner
will extract final live M1 bars, feed them into `PaperRuntimeAdapterLoop`, and
publish paper runtime batches through `PaperRuntimeRedisSink`.

## Consumer group defaults

The default group contract is:

```text
consumer_group = finam-imoexf-paper-runtime-m1
consumer_name  = auto
group_start    = 0
block_ms       = 500
claim_idle_ms  = 5000
claim_batch    = 50
read_count     = 1
```

`consumer_name = auto` normalizes to a paper-runtime name derived from the
configured source. This mirrors ALOR's “auto consumer name” behavior while
remaining deterministic in source-level tests.

## Backfill and tail modes

Two group start modes are explicitly modeled:

```text
Beginning -> 0
Tail      -> $
```

Use `Beginning` for replay/backfill validation. Use `Tail` only when the paper
runtime contour is intentionally started from fresh live input.

## ACK policy

The ACK rule is intentionally conservative:

```text
successfully processed batch -> XACK
decoded poison with DLQ      -> XACK
partial publish failure      -> no XACK / pending retry
unknown processing failure   -> no XACK / pending retry
```

This prevents losing a market-data entry if the paper ledger batch is only
partially published.

## DLQ policy

DLQ records include:

- source;
- consumer group/name;
- original stream/id;
- reason;
- payload SHA256;
- timestamp.

Raw Redis payload text is not exported.

## Config additions

`config/finam-imoexf-hybrid-paper-shadow.vps.example.json` now includes:

```text
paper_runtime_consumer.source_stream
paper_runtime_consumer.consumer_group
paper_runtime_consumer.consumer_name
paper_runtime_consumer.group_start
paper_runtime_consumer.block_ms
paper_runtime_consumer.claim_idle_ms
paper_runtime_consumer.claim_batch
paper_runtime_consumer.read_count
paper_runtime_consumer.xack_policy
paper_runtime_consumer.pending_policy
```

The same config also declares:

```text
finam_imoexf_paper:runtime:publish_batches
finam_imoexf_paper:runtime:health
finam_imoexf_paper:runtime:readiness
finam_imoexf_paper:runtime:dlq
```

## Boundary

Still disabled:

- strategy invocation;
- continuous runtime loop;
- runtime `LiveReady`;
- command-consumer-to-real-FINAM;
- FINAM `POST /orders`;
- FINAM `DELETE /orders/{id}`;
- Stop/SLTP/bracket/replace/multi-leg;
- live broker ACK lifecycle.

## Test coverage

The `finam-gateway` tests cover:

- ALOR-style group lifecycle command plan;
- backfill start-id `0`;
- future tail start-id `$`;
- non-paper source stream rejected before group creation;
- DLQ record redacts raw payload and stores only SHA256;
- ACK disposition only ACKs success or DLQ.

Next stage: M4-3r-i actual local paper runtime Redis runner that uses this
consumer-group contract to read `finam_imoexf_paper:ws:market_data`, process
paper batches, and XACK only after successful batch publish or DLQ.
