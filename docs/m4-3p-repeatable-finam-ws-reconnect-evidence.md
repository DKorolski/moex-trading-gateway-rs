# M4-3p repeatable FINAM WS reconnect evidence

M4-3p turns the accepted M4-3o reconnect hardening into a post-patch runtime
evidence package.

It remains no-live: no runtime-live attachment, no command consumer, no order
placement, no cancel, no Stop/SLTP/bracket, and no strategy trading.

## Scope

M4-3p must prove on code after `fdc91ee` that:

- a manual local network break degrades readiness to `MarketDataNotLive`;
- FINAM WebSocket reconnects and BARS resubscribe is confirmed;
- REST Bars replay/gap diagnostics close cleanly;
- `recovery.phase = LiveReady`;
- `recovery.gap_absence_proven = true`;
- `recovery.blockers = []`;
- gateway readiness returns to safe no-live
  `Reconciliation / OperatorLiveArmMissing`;
- `SIGTERM` during an active WS receive loop publishes a stopped summary;
- `Ctrl-C` during an active WS receive loop publishes a stopped summary.

## Operator run directory

```bash
cd /Users/denisq/Documents/from_mac/projects/strategies_list/moex-trading-project

set -a
source .env
set +a

mkdir -p tmp/m4-3p
redis-cli PING
```

## Run A: post-patch Wi-Fi reconnect

Terminal 1:

```bash
RUST_LOG=info cargo run -p broker-cli -- finam-ws-shadow-loop \
  --config config/finam-ws-shadow.arm.example.json \
  --symbol "${FINAM_SYMBOL}" \
  --timeframe "${FINAM_TIMEFRAME:-TIME_FRAME_M1}" \
  --subscribe-bars \
  --max-messages 10000 \
  --max-duration-seconds 300 \
  --reconnect-delay-seconds 5 \
  2>&1 | tee tmp/m4-3p/reconnect.log
```

Terminal 2 baseline check:

```bash
redis-cli XREVRANGE finam_ws_shadow_local:market_data + - COUNT 10
redis-cli XREVRANGE finam_ws_shadow_local:health + - COUNT 3
```

After final bars are visible, manually turn Wi-Fi off for 2-4 minutes, then
turn it back on. Record the local MSK and UTC break window.

Wait until stdout prints a successful `finam_ws_shadow = true` report after the
reconnect. Expected post-patch recovery shape:

```text
recovery.phase = LiveReady
recovery.blockers = []
recovery.gap_absence_proven = true
readiness_phase = Reconciliation
readiness_reasons = ["OperatorLiveArmMissing"]
```

Stop after the successful report. `SIGTERM` may be used:

```bash
pgrep -fl "broker-cli finam-ws-shadow-loop"
kill -TERM <pid>
```

Capture Redis summaries:

```bash
redis-cli XREVRANGE finam_ws_shadow_local:readiness + - COUNT 50 \
  > tmp/m4-3p/readiness.txt

redis-cli XREVRANGE finam_ws_shadow_local:health + - COUNT 30 \
  > tmp/m4-3p/health.txt

redis-cli XREVRANGE finam_ws_shadow_local:market_data + - COUNT 200 \
  > tmp/m4-3p/market_data.txt
```

## Run B: SIGTERM graceful shutdown

Terminal 1:

```bash
RUST_LOG=info cargo run -p broker-cli -- finam-ws-shadow-loop \
  --config config/finam-ws-shadow.arm.example.json \
  --symbol "${FINAM_SYMBOL}" \
  --timeframe "${FINAM_TIMEFRAME:-TIME_FRAME_M1}" \
  --subscribe-bars \
  --max-messages 10000 \
  --max-duration-seconds 300 \
  --reconnect-delay-seconds 5 \
  2>&1 | tee tmp/m4-3p/sigterm.log
```

After market data is visibly flowing, Terminal 2:

```bash
pgrep -fl "broker-cli finam-ws-shadow-loop"
kill -TERM <pid>
```

Expected stdout:

```text
finam_ws_shadow_loop = stopped
stop_reason = sigterm
live_trading_enabled = false
```

## Run C: Ctrl-C graceful shutdown

Terminal 1:

```bash
RUST_LOG=info cargo run -p broker-cli -- finam-ws-shadow-loop \
  --config config/finam-ws-shadow.arm.example.json \
  --symbol "${FINAM_SYMBOL}" \
  --timeframe "${FINAM_TIMEFRAME:-TIME_FRAME_M1}" \
  --subscribe-bars \
  --max-messages 10000 \
  --max-duration-seconds 300 \
  --reconnect-delay-seconds 5 \
  2>&1 | tee tmp/m4-3p/ctrlc.log
```

After market data is visibly flowing, press `Ctrl-C`.

Expected stdout:

```text
finam_ws_shadow_loop = stopped
stop_reason = ctrl_c
live_trading_enabled = false
```

## Generate redacted evidence

Use the actual break timestamps from Run A:

```bash
python3 scripts/m4_3p_repeatable_reconnect_evidence.py \
  --reconnect-log tmp/m4-3p/reconnect.log \
  --sigterm-log tmp/m4-3p/sigterm.log \
  --ctrlc-log tmp/m4-3p/ctrlc.log \
  --break-window-msk "YYYY-MM-DD HH:MM-HH:MM" \
  --break-window-utc "YYYY-MM-DDTHH:MM:SSZ/YYYY-MM-DDTHH:MM:SSZ" \
  --readiness-dump tmp/m4-3p/readiness.txt \
  --health-dump tmp/m4-3p/health.txt \
  --market-data-dump tmp/m4-3p/market_data.txt
```

The script writes:

```text
reports/m4/m4-3p-repeatable-reconnect-evidence.json
```

Raw logs stay local under `tmp/` and must not be included in handoff.
