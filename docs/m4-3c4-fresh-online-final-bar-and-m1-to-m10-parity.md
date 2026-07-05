# M4-3c4 fresh-online final bar and M1-to-10m parity

Status: source-only preparation / no-live / no order endpoints.

M4-3c4 follows the accepted M4-3c3 closed-bar finalizer. The goal is to prove
that FINAM WebSocket can provide fresh online final bars during an active
market phase and to make those M1 bars comparable with existing ALOR-centered
10-minute strategy bars.

## What can be done before the next active session

Source-only work:

- keep FINAM WS strategy stream final-only;
- add a canonical `CanonicalBarAggregator`;
- aggregate final M1 bars into strict final 10m buckets;
- reject forming bars;
- reject non-contiguous or non-monotonic M1 bars;
- drop incomplete buckets instead of manufacturing synthetic 10m bars;
- keep runtime-live disabled;
- keep command-consumer-to-real-FINAM disabled;
- keep FINAM POST/DELETE order endpoints out of this stage.

## Canonical M1-to-10m aggregation contract

The aggregator is intentionally strict:

```text
input source bars      = final only
source timeframe       = smaller than target timeframe
target/source ratio    = exact integer
bucket alignment       = UTC epoch-aligned target timeframe
bucket completeness    = all source bars present and contiguous
output finality        = final only
OHLCV                  = open(first), high(max), low(min), close(last), volume(sum)
```

If there is a gap or a forming bar, no 10m parity bar is emitted. This is
deliberate: parity must surface data-quality gaps, not hide them.

## What must wait for active market data

Runtime evidence must be collected during an active market phase and show:

- `moex-finam-ws-shadow.service` active/running;
- release symlink points at the reviewed release;
- FINAM WS receives fresh online final M1 bars;
- `MarketDataStale` is absent;
- `last_final_live_bar_close_ts` is within SLA;
- Redis strategy market-data stream remains final-only;
- derived FINAM 10m bars can be compared with ALOR 10m oracle bars.

## Runtime evidence format

Future runtime evidence should be a single valid JSON document, for example:

```json
{
  "iteration": {},
  "summary": {},
  "systemd": {},
  "redis_sample": {}
}
```

If the tool emits multiple JSON objects, the artifact must be explicitly named
JSONL or wrapped before review.

## Boundary

M4-3c4 must not:

- send FINAM `POST /orders`;
- send FINAM `DELETE /orders/{id}`;
- enable command-consumer-to-real-FINAM;
- enable continuous runtime live;
- attach live strategy runtime;
- open or close a position;
- enable Stop/SLTP/bracket/replace/multi-leg;
- make cutover automatic.

## Acceptance

M4-3c4 is ready for source review when:

- `CanonicalBarAggregator` is exported from `broker-core`;
- tests cover complete M1-to-10m aggregation;
- tests reject forming source bars;
- tests reject gaps/non-contiguous source bars;
- tests drop incomplete buckets;
- forbidden-surface scanners remain green;
- no broker order endpoints are enabled or called.

M4-3c4 is ready for runtime review only after active-session evidence proves
fresh final online bars and ALOR-vs-FINAM 10m parity.
