# M4-3d FINAM WS freshness and gap diagnostics

M4-3d hardens the FINAM WebSocket shadow report after the first local IMOEXF
checks showed that messages can arrive through WS while their bar/quote
timestamps are stale.

This step does not change trading boundaries:

- no live orders;
- no FINAM order POST/DELETE;
- no command-consumer-to-real-FINAM;
- no runtime-live attachment;
- no Stop/SLTP/bracket.

## Motivation

`MarketDataSourceKind::LiveStream` means transport source: the event arrived via
WebSocket. It does not by itself prove that the data is fresh enough for strategy
readiness.

Observed local shape:

- WS connected;
- `Quote` messages were received at current wall-clock time;
- quote `source_ts` was much older;
- `Bar` events were final M1 bars, but the latest close time was about an hour
  behind receive time;
- readiness correctly remained degraded, but stdout did not clearly explain the
  stale/live distinction.

## Added diagnostics

`finam-ws-shadow-*` reports now expose:

- `freshness_threshold_sec`;
- `fresh_live_final_bar_seen`;
- `first_fresh_live_final_bar_close_ts`;
- `last_fresh_live_final_bar_close_ts`;
- `latest_ws_bar_close_ts`;
- `latest_ws_final_bar_close_ts`;
- `latest_live_final_bar_stale_for_sec`;
- `max_live_final_bar_stale_for_sec`;
- `stale_live_final_bar_count`;
- `final_bar_gap_detected_count`;
- `first_final_bar_gap_expected_close_ts`;
- `first_final_bar_gap_actual_close_ts`;
- `last_final_bar_gap_expected_close_ts`;
- `last_final_bar_gap_actual_close_ts`;
- `ws_backlog_or_stale_bars_detected`;
- `fresh_live_readiness_evidence_missing`.

The freshness threshold is the same family as lifecycle staleness:

```text
max(3 * timeframe_sec, 60)
```

For M1 this is 180 seconds.

## Gap rule

For final live bars, the expected next close timestamp is:

```text
previous_final_close_ts + timeframe_sec
```

If the next emitted final close timestamp is later than expected, the report
records a gap. This is diagnostic hardening for the future M4-3d/M4-3e recovery
loop where REST replay must close the gap before readiness can advance.

## Interpretation

`source_kind = LiveStream` means "arrived via FINAM WS".

`fresh_live_final_bar_seen = true` means "a final WS bar was fresh relative to
the observation timestamp and timeframe threshold".

Runtime parity must depend on the second condition, not merely the first.

## Future wiring

Next implementation step:

1. feed these diagnostics into the M4-3c5 recovery contract;
2. use REST Bars replay after reconnect;
3. dedupe overlap;
4. block readiness until gap absence and first fresh live final bar are proven;
5. collect active-session evidence.
