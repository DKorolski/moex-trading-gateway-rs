# M4-3x — seeded ALOR-oracle FINAM paper parity

Status: active VPS paper-shadow parity step.

This step closes the immediate gap where FINAM paper runtime had fresh M10 bars
but lacked the ALOR hybrid/riskgate historical context required for comparable
paper intents.

## What changed

FINAM paper runtime can optionally read an ALOR-oracle runtime-state seed from
the FINAM paper Redis namespace:

```text
finam_imoexf_paper:oracle:alor_runtime_state
```

The seed is read once on paper consumer startup and applied before processing
new FINAM market-data entries.

For acceptance/evidence runs the seed must be configured as required:

```json
{
  "alor_oracle": {
    "seed_required": true,
    "missing_seed_policy": "BlockParityRun"
  }
}
```

If the seed is missing or cannot be parsed, the parity run must fail or report a
blocked status. Exploratory runs may set `seed_required=false`, but then the
status is only an unseeded bridge diagnostic.

Seeded fields:

- `next_cycle_seq`;
- `last_position_qty`;
- owner/side context;
- pending entry/exit request ids and pending entry owner/side/cycle;
- TP/SL id placeholders when present;
- MR take/stop prices when present;
- safe-mode close-only flag/reason;
- previous-day close/range/return;
- day-before close;
- current-day high/low/close;
- `today_start_local`;
- day position flags;
- riskgate session date;
- riskgate profile id;
- riskgate shadow pnl/trade count;
- MR enabled flag;
- rolling LB120 sum;
- last finalized riskgate session date;
- riskgate ledger row count.

## Boundary

This is paper/shadow only.

Still disabled:

- real FINAM command consumer;
- runtime-live;
- automatic strategy-driven real send;
- Stop/SLTP/bracket/replace/multi-leg.

## VPS operational shape

Current intended processes:

```text
broker-cli finam-ws-shadow-loop
broker-cli finam-paper-runtime-consume
```

FINAM WS continues to publish live market-data. The seeded paper consumer uses a
separate consumer group so it can replay from `0` after the seed is refreshed.

## Evidence observed on 2026-07-07

After seeding from the ALOR runtime-state stream
`runtime.state.hybrid_intraday.live.riskgate_shadow.imoexf.<PORTFOLIO_ID>`,
FINAM paper state matched ALOR on the active IMOEXF hybrid M10 fields:

- `last_bar_close`;
- `current_day_high`;
- `current_day_low`;
- `current_day_close`;
- `prev_day_close`;
- `prev_day_range`;
- `prev_day_return`;
- `day_before_close`;
- `today_start_local`;
- `next_cycle_seq`;
- `last_position_qty`;
- `entry_ready`;
- `risk_gate_shadow_session_date`;
- `risk_gate_mr_enabled_current_session`;
- `risk_gate_rolling_sum_lb120`;
- `risk_gate_last_finalized_session_date`;
- `risk_gate_ledger_rows_count`.

`runtime:dlq` remained empty during the observed check.

## Why this is not final parity

The seed is a bridge, not the final runtime architecture.

Still required:

- full-session FINAM M10 vs ALOR M10 report;
- true broker-truth bootstrap into runtime lifecycle;
- real hybrid BO/MR orchestrator/riskgate attachment;
- command-consumer paper/mock ACK path;
- durable request/client/broker-order-id chain.

## Next evidence report

Produce:

```text
reports/parity/finam-vs-alor-m10/YYYY-MM-DD.json
```

For runtime-state field parity, use:

```bash
python3 scripts/m4_3x_runtime_state_parity_evidence.py \
  --finam-redis-cli-prefix "ssh root@VPS 'docker exec moex-trading-project-redis redis-cli --raw'" \
  --alor-redis-cli-prefix "ssh root@VPS 'docker exec trading-hybrid-redis-1 redis-cli --raw'" \
  --vps-host "<VPS_HOST>" \
  --seed-required \
  --output reports/parity/finam-vs-alor-runtime-state/YYYY-MM-DD.json
```

The report intentionally stores only normalized selected fields, not raw Redis
payloads.

Minimum fields:

- source commit;
- VPS host label;
- FINAM WS source stream;
- FINAM runtime-state stream;
- ALOR runtime-state stream;
- compared M10 bar key/timestamp;
- OHLCV diagnostic deltas where available;
- DLQ count;
- consumer group pending count;
- divergence classification;
- expected/waived/blocker divergence counts;
- safety flags.
- VPS host;
- FINAM WS source stream;
- FINAM paper runtime state stream;
- ALOR oracle runtime state stream;
- compared M10 bar keys;
- OHLCV deltas;
- runtime field deltas;
- riskgate field deltas;
- unexplained divergence count;
- DLQ count;
- consumer group pending/lag;
- safety flags proving no live FINAM order path was enabled.
