# M4-3h-b replay wiring into FINAM WS loop

M4-3h-b wires FINAM REST Bars replay into the no-live FINAM WebSocket shadow loop.

The replay path is recovery/diagnostic only:

- recovery bars are fetched with FINAM REST Bars;
- recovery bars are classified as recovery replay input;
- overlap bars are counted as deduped;
- non-overlap recovery bars are counted as `RecoveryNotStrategyLive`;
- recovery bars are not published as strategy-live bars;
- first fresh FINAM WS final bars are still required for strategy-ready live stream evidence.

## Report fields

`finam-ws-shadow-*` reports include:

```text
recovery.rest_replay_wiring_enabled = true
recovery.recovery_bars_publishable_as_strategy_live = false
recovery_replay.attempted
recovery_replay.fetch_ok
recovery_replay.bars_count
recovery_replay.overlap_dedup_bar_count
recovery_replay.recovery_not_strategy_live_bar_count
recovery_replay.gap_absence_proven
recovery_replay.published_to_redis = false
recovery_replay.published_as_strategy_live = false
```

The data-quality ledger also exposes:

```text
ReplayedRecoveryBar
OverlapDeduped
ReplayGapDetected
RecoveryNotStrategyLive
```

## Boundary

Allowed:

```text
FINAM auth
FINAM REST Bars GET
FINAM WebSocket market data
Redis health/readiness/market-data writes for normal WS shadow output
```

Forbidden:

```text
order POST/DELETE
live orders
runtime-live attachment
command-consumer-to-real-FINAM
stop/SLTP/bracket
```

M4-3h-b is still market-data recovery wiring only. It does not authorize runtime cutover.
