# M4-3b-a VPS WebSocket runtime evidence

Status: runtime evidence package / no-live / no order endpoints.

M4-3b-a closes the runtime-proof gap from the M4-3a/M4-3b review. M4-3b was
accepted as source/no-live WebSocket market-data shadow implementation, but the
uploaded evidence was intentionally source-only. This package captures a
redacted VPS runtime snapshot for the already deployed FINAM WebSocket shadow.

## Boundary

M4-3b-a must not:

- send FINAM `POST /orders`;
- send FINAM `DELETE /orders/{id}`;
- enable command-consumer-to-real-FINAM;
- enable order placement or cancel;
- attach the strategy runtime to FINAM live;
- enable Stop/SLTP/bracket/replace/multi-leg;
- authorize cutover to FINAM-only operation.

The evidence command only reads systemd state, Redis metadata, and one bounded
`finam-ws-shadow-once` JSON report. The one-shot command subscribes to FINAM
WebSocket market data and publishes only `MarketData` shadow events.

## Required runtime proof

The report must include:

- redacted `systemctl` snapshot for `moex-finam-ws-shadow.service`;
- disabled-on-boot status for the WS service;
- active state for the existing REST shadow service;
- Redis `XLEN` and sanitized `XINFO STREAM` metrics for `finam_ws_shadow:*`;
- absence of the disabled command ACK stream;
- one redacted `finam-ws-shadow-once` iteration report;
- `decode_error_count` and `mapper_error_count`;
- `bar_event_count`, `final_bar_event_count`, `forming_bar_event_count`;
- `quote_event_count`;
- latest market-data event shape with `source_kind = LiveStream`;
- explicit no-live flags.

## Collector

Example:

```bash
python3 scripts/m4_3ba_vps_ws_runtime_evidence.py \
  --ssh-host root@VPS_HOST \
  --ssh-key ~/.ssh/id_rsa_lightnode \
  --output reports/m4/m4-3b-a-vps-ws-runtime-evidence.json
```

The collector redacts host identity by default and stores only bounded shapes,
counts, flags, hashes, and stream metadata. It does not store FINAM tokens,
account ids, raw WebSocket payloads, raw Redis market-data values, or raw
instrument prices.

## Acceptance

M4-3b-a is ready for review when:

- `ok = true`;
- `ws_service.active_state = active`;
- `ws_service.enabled_state = disabled`;
- `ws_service.n_restarts = 0`;
- `one_shot_report.live_trading_enabled = false`;
- `one_shot_report.order_placement_enabled = false`;
- `one_shot_report.cancel_enabled = false`;
- `one_shot_report.command_consumer_enabled = false`;
- `one_shot_report.market_data.source_kind = LiveStream`;
- `one_shot_report.metrics.decode_error_count = 0`;
- `one_shot_report.metrics.mapper_error_count = 0`;
- market-data stream length is positive;
- command ACK disabled stream does not exist;
- local forbidden-surface scanners are green.

This still does not authorize M4-3c strategy parity, runtime-live, or FINAM
cutover. The next implementation step remains canonical M1-to-10m aggregation
and ALOR 10m bar parity.
