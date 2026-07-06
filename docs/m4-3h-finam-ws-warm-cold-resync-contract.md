# M4-3h FINAM WS warm/cold resync contract

M4-3h starts the no-live warm/cold resync loop work by wiring the accepted M4-3c5 broker-neutral recovery primitives into the FINAM WS shadow report.

This is intentionally a source/no-live contract slice. It does **not** yet enable real REST Bars replay inside the WS loop.

## Contract

The FINAM WS report now includes:

```text
recovery.schema = m4_3h_warm_cold_resync_contract
recovery.rest_replay_wiring_enabled = false
recovery.recovery_bars_publishable_as_strategy_live = false
recovery.requires_first_fresh_ws_final_after_replay = true
recovery.phase
recovery.blockers
recovery.mode
recovery.last_final_bar_close_ts
recovery.replay_from_ts
recovery.replay_to_ts
recovery.replay_bar_count
recovery.gap_absence_proven
```

The recovery model uses:

- final-bar watermark;
- warm replay window with overlap;
- cold replay window when no watermark exists;
- subscription confirmation from M4-3g;
- first fresh WS final bar after replay;
- data-quality gap diagnostics from M4-3d/M4-3f.

## Safety rules

Until real replay is wired and proven:

```text
rest_replay_wiring_enabled = false
gap_absence_proven may remain false
recovery bars are not strategy-live bars
runtime-live remains disabled
command-consumer-to-real-FINAM remains disabled
order POST/DELETE remains forbidden
```

This mirrors the ALOR operational idea without pretending that reconnect alone equals data recovery.

## Next implementation slice

The next M4-3h slice should add controlled real GET-only REST Bars replay evidence:

1. persist last emitted final bar watermark;
2. compute warm replay window from watermark minus overlap;
3. fetch REST Bars for the replay window;
4. classify replay bars as `Recovery`;
5. dedupe overlap;
6. prove contiguity through at least one bar after the watermark;
7. require first fresh WS final after replay before strategy-ready state;
8. keep all order/live boundaries closed.
