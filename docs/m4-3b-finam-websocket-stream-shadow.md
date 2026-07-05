# M4-3b FINAM WebSocket stream shadow

Status: streaming market-data shadow / no-live / no order endpoints.

M4-3b moves the FINAM shadow contour from REST-only market-data polling toward
the same operational shape as the mature ALOR contour: live stream input first,
broker truth reconciliation second.

This is still not a runtime/live cutover. ALOR may remain the operational oracle
while FINAM WebSocket data is published into separate Redis streams for parity
inspection.

```text
ALOR live/oracle streams ──────────────┐
                                      ├─ parity comparator / operator review
FINAM WebSocket shadow streams ───────┘

FINAM REST read-only truth snapshots ── reconciliation fallback
```

## Trading boundary

M4-3b must not:

- send FINAM `POST /orders`;
- send FINAM `DELETE /orders/{id}`;
- enable command-consumer-to-real-FINAM;
- enable continuous runtime live;
- open or close a position;
- enable Stop/SLTP/bracket/replace/multi-leg;
- make cutover automatic.

The only broker-facing live channel added by M4-3b is FINAM WebSocket market
data subscription. Order placement and cancel features remain disabled in
`GatewayFeatureSet`.

## Stream scope

The initial FINAM WebSocket shadow subscribes to:

- `QUOTES`;
- `BARS`.

It publishes mapped broker-neutral `MarketDataEvent` values into a dedicated
Redis stream namespace:

- `finam_ws_shadow:health`;
- `finam_ws_shadow:readiness`;
- `finam_ws_shadow:market_data`;
- `finam_ws_shadow:command_acks_disabled`.

`command_acks_disabled` is intentionally present only as a configured disabled
stream name. The WS shadow must not consume command streams and must not publish
real order ACK lifecycle events.

## Bar finality policy

FINAM WebSocket bar payloads are mapped to
`MarketDataSourceKind::LiveStream`.

A streamed bar is marked final only when:

```text
bar.close_ts <= received_ts
```

Forming bars may be useful for diagnostics, but strategy parity must continue
to use final closed bars only.

## Relation to REST shadow

REST shadow stays valuable and should remain available on VPS during this step:

- accounts;
- positions;
- active/terminal orders;
- trades;
- portfolio/cash truth;
- historical fallback bars.

The WebSocket shadow improves freshness and ALOR parity for market-data input,
but it is not itself broker truth for positions or orders.

## 10-minute strategy parity

The current production systems use 10-minute closed-bar behavior. Direct FINAM
REST `TIME_FRAME_M10` characterization previously returned an HTTP 400 for the
tested symbol/timeframe combination, so M4-3b starts with `TIME_FRAME_M1` WS
bars.

The next parity step must derive canonical 10-minute final bars from the M1
stream or separately prove a FINAM-native 10-minute stream/history endpoint.
Until that is reviewed, FINAM WS shadow is an input-quality and freshness stand,
not a strategy attachment.

## VPS rollout shape

Recommended service name:

```text
moex-finam-ws-shadow.service
```

Recommended command:

```bash
cargo run -p broker-cli -- finam-ws-shadow-loop \
  --config config/finam-ws-shadow.vps.example.json
```

On VPS the service may run continuously, but should stay disabled on boot until
operator review accepts the stream stability. The existing REST shadow service
can remain running in parallel because it publishes under the separate
`finam_shadow:*` namespace.

## Acceptance

M4-3b is ready for review when:

- FINAM WS request builders keep the documented `SUBSCRIBE` wire shape;
- QUOTES/BARS envelopes map to `MarketDataSourceKind::LiveStream`;
- finality tests distinguish forming and final bars;
- CLI WS shadow config keeps order/cancel/consumer features disabled;
- example VPS config contains only synthetic placeholders;
- forbidden-surface scanners remain green;
- handoff archive excludes `.env`, `.git`, `target`, `tmp`, `reports`, logs,
  and local deployment overrides.
