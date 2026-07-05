# M4-3c3 FINAM WS closed-bar finalizer

Status: source-only / no-live / no order endpoints.

M4-3c3 closes the remaining market-data parity gap between the FINAM WebSocket
shadow contour and the mature ALOR gateway behavior: strategy bars must be
closed bars. FINAM may emit repeated forming `BARS` updates for the currently
open minute. Those raw forming updates are useful diagnostics, but they must not
be consumed by strategy parity as final candles.

The FINAM WS path now uses a broker-neutral `ClosedBarFinalizer` before
publishing bar events into the Redis market-data stream.

## Contract

`ClosedBarFinalizer` applies this rule:

```text
raw forming bar N        -> buffer only, no strategy publish
updated forming bar N    -> update buffer only, no strategy publish
raw forming bar N+1      -> emit buffered bar N as final, buffer N+1
raw explicit final bar N -> pass through once as final
duplicate final bar N    -> suppress
late final bar N-1       -> suppress without deleting current forming bar N
non-live source bar      -> pass through as read-only/recovery diagnostic source
```

This matches the intended ALOR-style closed-bar contract:

```text
model input = closed bar
live execution intent = immediately after closed signal bar
forming bars = diagnostics / freshness only
```

## FINAM WS shadow behavior

`finam-ws-shadow-loop` still counts raw inbound bars:

```text
bar_event_count
final_bar_event_count
forming_bar_event_count
live_bar_event_count
forming_live_bar_event_count
```

It also reports finalizer diagnostics:

```text
closed_bar_finalized_count
final_bar_passthrough_count
forming_bar_suppressed_count
duplicate_final_suppressed_count
non_monotonic_forming_dropped_count
non_live_bar_passthrough_count
closed_bar_finalizer_enabled = true
strategy_bars_are_final_only = true
raw_forming_bars_published_for_strategy = false
```

Only finalized/canonical bars are published as `MarketDataEvent::Bar` for
strategy parity. Quotes remain diagnostic-only, and order-book/latest-trade
events remain ordinary market-data events if mapped later.

## Readiness

The first-live-final gate is now based on canonical finalized live bars:

```text
first_live_final_bar_seen
first_live_final_bar_close_ts
last_final_live_bar_close_ts
final_live_bar_event_count
```

Receiving raw forming live bars still does not satisfy readiness. Once a next
forming bar allows the previous bar to be closed, market-data lifecycle can move
to `LiveReady`, while gateway readiness remains
`Reconciliation / OperatorLiveArmMissing` because runtime-live and command
consumer to real FINAM are still disabled.

## Boundary

M4-3c3 must not:

- send FINAM `POST /orders`;
- send FINAM `DELETE /orders/{id}`;
- enable command-consumer-to-real-FINAM;
- enable continuous runtime live;
- attach runtime-live;
- open or close a position;
- enable Stop/SLTP/bracket/replace/multi-leg;
- make cutover automatic.

## Acceptance

M4-3c3 is ready for review when:

- `ClosedBarFinalizer` is exported from `broker-core`;
- FINAM WS handler publishes only finalized/canonical bars to Redis;
- raw forming bars are counted but suppressed from the strategy bar stream;
- late duplicate final bars do not erase the current forming bar buffer;
- first-live-final readiness uses canonical finalized bars;
- report JSON exposes finalizer diagnostics;
- forbidden-surface scanners remain green;
- no broker order endpoints are enabled or called.
