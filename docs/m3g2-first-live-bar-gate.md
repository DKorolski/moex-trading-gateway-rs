# M3g-2 market-data first-live-bar gate

M3g-2 hardens the market-data side of readiness after M3g-1. It is still a
contract/simulator layer: it does not connect real live streams, enable
`LiveReady`, attach runtime strategies, or permit FINAM order endpoints.

First-live-bar acceptance requires:

- `MarketDataSourceKind::LiveStream`;
- final bar;
- fresh close timestamp within the configured max age;
- bar interval inside the current tradable session;
- monotonic open timestamp relative to the accepted live watermark;
- dedupe key not previously accepted.

Rejected cases:

- missing bar;
- historical/read-only/backfill source;
- non-final bar;
- stale bar;
- out-of-session bar;
- duplicate bar;
- non-monotonic bar.

Readiness behavior:

- missing/rejected first-live-bar gate hard-blocks readiness;
- stale live bar also marks stream stale;
- accepted first live bar can satisfy only the market-data gate;
- `LiveReady` remains forbidden and the readiness phase stays
  `Reconciliation` with `OperatorLiveArmMissing` until later explicit live
  gates.

Still forbidden:

- real FINAM `POST /orders`;
- real FINAM `DELETE /orders/{id}`;
- non-loopback order endpoint;
- real command-consumer-to-FINAM transport;
- runtime live attachment;
- `LiveReady`;
- stop/SLTP/brackets/replace/multi-leg.

Evidence:

```bash
python3 scripts/m3g2_first_live_bar_gate_evidence.py \
  --source-archive reports/handoff/moex-trading-project-<commit>.zip
```
