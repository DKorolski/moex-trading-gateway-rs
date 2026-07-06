# M4-3o local FINAM WS Wi-Fi reconnect runbook

This runbook is a local ARM/Mac manual chaos check for the developing FINAM
WebSocket market-data contour.

It does not enable runtime-live, command consumption, order placement, cancel,
Stop/SLTP/bracket, or any strategy trading. The expected post-recovery gateway
phase is still `Reconciliation` with `OperatorLiveArmMissing`, not production
`LiveReady`.

## Goal

Verify that a local `finam-ws-shadow-loop` run:

- starts with fresh live FINAM market data for the configured symbol;
- publishes health/readiness/market-data to local Redis streams;
- degrades while Wi-Fi/network is unavailable;
- reconnects and resubscribes after Wi-Fi is restored;
- performs warm REST Bars replay/gap diagnostics;
- observes a fresh live final WebSocket bar after recovery;
- keeps all order/live surfaces disabled.

## Local config

Use the local example config so evidence is not mixed with VPS streams:

```bash
config/finam-ws-shadow.arm.example.json
```

It publishes to:

```text
finam_ws_shadow_local:health
finam_ws_shadow_local:readiness
finam_ws_shadow_local:market_data
```

## Preflight

Run from the repository root:

```bash
cd /Users/denisq/Documents/from_mac/projects/strategies_list/moex-trading-project

set -a
source .env
set +a

redis-cli PING
```

Required local env values:

```text
FINAM_SECRET_TOKEN
FINAM_SYMBOL=IMOEXF@RTSX
FINAM_TIMEFRAME=TIME_FRAME_M1
```

## Start the local shadow loop

Use a bounded per-connection duration so reconnect passes are visible in stdout.

```bash
mkdir -p tmp/local-wifi-reconnect

RUST_LOG=info cargo run -p broker-cli -- finam-ws-shadow-loop \
  --config config/finam-ws-shadow.arm.example.json \
  --symbol "${FINAM_SYMBOL}" \
  --timeframe "${FINAM_TIMEFRAME:-TIME_FRAME_M1}" \
  --subscribe-bars \
  --max-messages 10000 \
  --max-duration-seconds 300 \
  --reconnect-delay-seconds 5 \
  2>&1 | tee "tmp/local-wifi-reconnect/finam-ws-shadow-local-$(date -u +%Y%m%dT%H%M%SZ).log"
```

## Wait for baseline market-data readiness

In a second terminal:

```bash
redis-cli XREVRANGE finam_ws_shadow_local:health + - COUNT 3
redis-cli XREVRANGE finam_ws_shadow_local:readiness + - COUNT 5
redis-cli XREVRANGE finam_ws_shadow_local:market_data + - COUNT 20
```

Baseline acceptance:

- stdout report has `finam_ws_shadow = true`;
- `live_trading_enabled = false`;
- `command_consumer_enabled = false`;
- `order_placement_enabled = false`;
- `cancel_enabled = false`;
- `strategy_market_data_source = FinamWebSocketBarsLiveStream`;
- `market_data.first_live_final_bar_seen = true`;
- `market_data.fresh_live_final_bar_seen = true`;
- readiness is `Reconciliation` with `OperatorLiveArmMissing`.

`Reconciliation + OperatorLiveArmMissing` is the expected safe no-live state.
Do not require production `LiveReady` in this runbook.

## Manual Wi-Fi break

After the baseline is visible:

1. record the local wall-clock time;
2. turn off Wi-Fi for 2-4 minutes;
3. keep the CLI running;
4. turn Wi-Fi back on;
5. wait for at least one new stdout report and at least one fresh final bar.

Expected during the break:

- one or more loop iterations may print an error;
- readiness may publish `Degraded` with `MarketDataNotLive`;
- no command ACK/order streams should become active.

Expected after reconnect:

- a new `ws_generation.generation_id` appears;
- subscription confirmation is present for bars;
- recovery report includes warm replay/gap diagnostics;
- `recovery.gap_absence_proven = true` before strategy-live interpretation;
- `market_data.first_live_final_bar_seen = true`;
- `market_data.fresh_live_final_bar_seen = true`;
- final-bar gap counters remain zero, or any gap is explicitly reported as a blocker;
- readiness returns to `Reconciliation + OperatorLiveArmMissing`.

## Stop and collect local evidence

Stop with `Ctrl-C` after recovery is visible.

If the terminal does not react to `Ctrl-C` while the WebSocket receive pass is
active, send a graceful terminate signal to the local process:

```bash
pgrep -fl "broker-cli finam-ws-shadow-loop"
kill -TERM <pid>
```

The loop should publish `Stopped` health/readiness and print a final stopped
summary with `stop_reason = sigterm`.

```bash
mkdir -p tmp/local-wifi-reconnect/redis

redis-cli XREVRANGE finam_ws_shadow_local:health + - COUNT 20 \
  > tmp/local-wifi-reconnect/redis/health.txt

redis-cli XREVRANGE finam_ws_shadow_local:readiness + - COUNT 50 \
  > tmp/local-wifi-reconnect/redis/readiness.txt

redis-cli XREVRANGE finam_ws_shadow_local:market_data + - COUNT 200 \
  > tmp/local-wifi-reconnect/redis/market_data.txt
```

Optional summary:

```bash
grep -E '"readiness_phase"|"readiness_reasons"|"ws_generation"|"recovery"|"market_data"' \
  tmp/local-wifi-reconnect/finam-ws-shadow-local-*.log
```

Keep the `tmp/` evidence local. Handoff archives must not include raw local logs.

## No-go signals

Treat the run as failed/pending if any of these are observed after Wi-Fi returns:

- no new WS generation;
- no bars subscription confirmation;
- no fresh live final bar after reconnect;
- `MarketDataNotLive` persists during an active session;
- recovery reports missing replay/gap evidence;
- final-bar gap is detected but not blocking readiness;
- any order placement/cancel/command-consumer flag becomes enabled.

## Observed local ARM run, 2026-07-06

Manual Wi-Fi break:

```text
wifi_off_window_msk = 15:19-15:21
wifi_off_window_utc = 12:19-12:21
symbol = IMOEXF@RTSX
timeframe = TIME_FRAME_M1
```

Observed outcomes:

- during the break, readiness degraded to `Degraded / MarketDataNotLive`;
- health degraded during the break and returned to `ReadOnly`;
- final 1m bars resumed after reconnect;
- the local stream covered the break window with final bars around
  `12:18-12:19`, `12:19-12:20`, `12:20-12:21`, then continued with
  `12:21-12:22` and later bars;
- readiness returned to safe no-live `Reconciliation / OperatorLiveArmMissing`;
- `ws_generation_id = finam-ws-generation-11`;
- BARS subscription was `DataConfirmed`;
- command consumer, order placement, cancel, and Stop/SLTP/bracket stayed
  disabled.

The run exposed two hardening items that were patched with this runbook:

1. Recovery report semantics must use a fresh live final bar at or after the
   replay tail, not the first fresh final bar observed earlier in the pass.
2. The WS loop must handle `SIGTERM` as graceful shutdown in addition to
   terminal `Ctrl-C`, including while the active WebSocket receive pass is
   running.
