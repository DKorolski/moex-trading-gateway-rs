# FINAM bar finality evidence — 2026-06-30

Status: redacted read-only evidence summary for M2k. The probes used only FINAM
auth and `bars_typed`; order placement, cancel, account order commands, command
consumer, and live trading were not used.

Local redacted JSON outputs were saved under `tmp/`:

- `tmp/finam-bar-finality-2026-06-30-open-window-redacted.json`;
- `tmp/finam-bar-finality-2026-06-30-near-current-redacted.json`;
- `tmp/finam-bar-finality-2026-06-29-boundary-redacted.json`.

The committed summary intentionally omits token values, account ids, order ids,
and the concrete venue symbol. The probe confirmed `symbol_present = true` and
`response_symbol_matches_request = true`.

## Observations

All checks used `TIME_FRAME_M1`.

| Window | Request | Bars | First open | Last open | Last derived close | Delta | Notes |
|---|---:|---:|---:|---:|---:|---:|---|
| Open session | `2026-06-30T06:00:00Z` → `2026-06-30T07:00:00Z` | 61 | `06:00:00Z` | `07:00:00Z` | `07:01:00Z` | 60s | Returned a bar at exact `end_time`. |
| Near-current | lookback 90 minutes | 89 | `10:22:00Z` | `11:50:00Z` | `11:51:00Z` | 60s | Last derived close was before probe time. |
| Boundary sample | `2026-06-29T15:40:00Z` → `2026-06-29T16:20:00Z` | 41 | `15:40:00Z` | `16:20:00Z` | `16:21:00Z` | 60s | Returned a bar at exact `end_time`. |

Common probe results:

- `ok = true`;
- `live_trading_enabled = false`;
- `order_endpoints_used = false`;
- `all_mapped_final_true = true`;
- `close_delta_mismatch_count = 0`;
- `non_monotonic_open_ts_count = 0`;
- `unique_open_deltas_sec = [60]`.

## Current interpretation

The evidence supports the existing shadow mapper assumption that FINAM REST
`bar.timestamp` is the bar open timestamp for M1 bars, with
`close_ts = open_ts + timeframe`.

Important caveat: exact minute-aligned bounded requests returned a bar whose
timestamp equals `end_time`. Treat FINAM historical bars as end-inclusive unless
future probes prove otherwise.

Before using historical polling as a runtime execution input, the gateway must
still enforce an explicit finality policy:

- drop any bar whose derived `close_ts` is after the probe/receive time;
- be careful with request `end_time` because FINAM may include the boundary bar;
- keep runtime `LiveReady` blocked until durable dedupe/watermark and runtime
  consumption policy are implemented.

Acceptance remains `unproven_operator_review_required` at the harness level
because this is evidence for mapper/finality policy, not permission to consume
historical bars for live trading.
