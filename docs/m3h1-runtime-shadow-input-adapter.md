# M3h-1 runtime shadow input adapter

M3h-1 starts runtime shadow integration without live attachment. It adapts
gateway Redis stream envelopes into broker-neutral runtime shadow inputs.

Scope:

- accept broker-neutral `MarketDataEvent::Bar` as normalized runtime `BarEvent`;
- accept readiness, portfolio snapshot, and order snapshot envelopes;
- classify bar source/finality for runtime guard use;
- keep FINAM DTOs out of runtime-facing input;
- emit no broker commands;
- keep `LiveReady`, runtime live attachment, and real FINAM order endpoints
  disabled.

Bar classification:

- `LiveFinal` for `MarketDataSourceKind::LiveStream` + final bar;
- `LiveUpdating` for live non-final bars;
- `HistoricalOrReadOnly` for historical/read-only poll bars;
- `RecoveryOrUnknown` for recovery/unknown source bars.

Safety behavior:

- only `LiveFinal` is marked as a first-live-bar unlock candidate for shadow
  guard evaluation;
- historical/read-only bars are accepted as shadow data but cannot unlock live
  semantics;
- the adapter is input-only and cannot emit `BrokerCommand` envelopes.

Still forbidden:

- real FINAM `POST /orders`;
- real FINAM `DELETE /orders/{id}`;
- non-loopback order endpoint;
- command-consumer-to-real-FINAM transport;
- runtime live order attachment;
- `LiveReady`;
- stop/SLTP/brackets/replace/multi-leg.

Evidence:

```bash
python3 scripts/m3h1_runtime_shadow_adapter_evidence.py \
  --source-archive reports/handoff/moex-trading-project-<commit>.zip
```
