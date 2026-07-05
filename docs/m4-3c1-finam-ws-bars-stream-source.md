# M4-3c1 FINAM WS bars stream source

Status: source-only / no-live / no order endpoints.

M4-3c1 makes the FINAM market-data parity source explicit: strategy parity must
use FINAM WebSocket `BARS` / live stream data, not REST historical bars or REST
quote polling.

```text
FINAM WebSocket BARS LiveStream
        -> finam_ws_shadow:market_data
        -> canonical M1 bars
        -> future M1-to-10m aggregation
        -> ALOR 10m parity comparator
```

REST shadow remains useful for broker truth and diagnostics:

- account/portfolio snapshots;
- order snapshots;
- trades/transactions when needed;
- diagnostic latest quote.

REST shadow must not be the strategy market-data source for 10-minute runtime
parity.

## Boundary

M4-3c1 must not:

- send FINAM `POST /orders`;
- send FINAM `DELETE /orders/{id}`;
- enable command-consumer-to-real-FINAM;
- enable continuous runtime live;
- open or close a position;
- enable Stop/SLTP/bracket/replace/multi-leg;
- make cutover automatic.

## Source policy

`finam-ws-shadow-loop` is the intended live market-data shadow for strategy
parity.

The loop reports:

```text
strategy_market_data_source = FinamWebSocketBarsLiveStream
rest_bars_used_for_strategy = false
rest_market_data_used_for_strategy = false
quotes_role = diagnostic_only | disabled
bars_stream_required_for_strategy_parity = true
```

Quotes may be enabled for diagnostics, but quotes alone do not satisfy strategy
parity readiness.

If `subscribe_bars = true` and the WS iteration receives no bar event, readiness
is degraded with:

```text
ReadinessPhase::Degraded
ReadinessReason::MarketDataNotLive
```

If bar events are present, the WS shadow may reach:

```text
ReadinessPhase::Reconciliation
ReadinessReason::OperatorLiveArmMissing
```

It still must not publish `LiveReady`.

## Loop behavior

The loop defaults are adjusted toward a streaming shape:

- `subscribe_bars = true`;
- `subscribe_quotes = false` for the continuous loop unless explicitly enabled;
- long per-connection duration;
- higher per-connection message budget;
- reconnect only as a stream lifecycle/recovery event, not as bar polling.

JWT/auth renewal may still use the FINAM auth endpoint until gRPC
`SubscribeJwtRenewal` is implemented, but bar data itself must come from WS
`BARS`, not REST `bars_typed`.

## Acceptance

M4-3c1 is ready for review when:

- CLI reports the WS bars live-stream source fields;
- readiness degrades when no BARS arrive;
- quotes remain diagnostic only;
- `finam-gateway-shadow-loop` REST bars remain separate from runtime strategy
  source;
- forbidden-surface scanners remain green;
- no broker API order endpoints are called;
- no runtime/live attachment is enabled.
