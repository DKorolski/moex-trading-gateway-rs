# M4-3g-a active-session fresh final evidence

M4-3g-a captures a real active-session FINAM WebSocket market-data proof after the MOEX open.

The purpose is to close the runtime evidence gap left by the source-only M4-3g package:

- FINAM WS connects with the operator-provided token;
- `BARS` and `QUOTES` subscriptions are confirmed by `DATA`;
- stale startup backlog is suppressed;
- at least one fresh final M1 bar is published to Redis as strategy-eligible market data;
- data-quality accounting remains balanced;
- readiness reaches `Reconciliation` with `OperatorLiveArmMissing`;
- live trading, command consumer, order placement, cancel, stop/SLTP/bracket remain disabled.

## Evidence command

```bash
python3 scripts/m4_3ga_active_session_fresh_final_evidence.py
```

The script reads `.env` locally, but it does not print or persist the FINAM secret or JWT.

It performs a bounded market-data-only probe:

```text
broker-cli finam-ws-shadow-once
  --symbol IMOEXF@RTSX
  --timeframe TIME_FRAME_M1
  --subscribe-bars
  --subscribe-quotes
  --max-duration-seconds 35
  --max-messages 120
```

It also reads Redis streams:

```text
finam:health
finam:readiness
finam:market-data
```

## Required active-session conditions

```text
ws_generation.active_subscriptions contains BARS and QUOTES
BARS status = DataConfirmed
QUOTES status = DataConfirmed
fresh_live_final_bar_seen = true
published_strategy_bar_count > 0
data_quality.imbalances = []
data_quality.bars.balanced = true
latest Redis Bar is final M1 LiveStream for IMOEXF@RTSX
latest Redis Bar close_ts >= configured min close timestamp
health.command_consumer_enabled = false
health.order_placement_enabled = false
live_trading_enabled = false
order_placement_enabled = false
cancel_enabled = false
stop_sltp_bracket_enabled = false
```

## Boundary

This stage intentionally allows real FINAM WebSocket market-data and Redis reads/writes.

It still forbids:

```text
live orders
POST /orders
DELETE /orders/{id}
runtime-live attachment
command-consumer-to-real-FINAM
stop/SLTP/bracket
```

M4-3g-a is evidence only. It does not authorize strategy runtime cutover.
