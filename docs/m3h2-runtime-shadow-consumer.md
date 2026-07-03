# M3h-2 runtime shadow consumer

M3h-2 adds a runtime shadow consumer on top of the M3h-1 broker-neutral input
adapter. It consumes FINAM-normalized gateway streams as shadow runtime inputs
only. It does not attach live strategies, emit broker commands, or enable real
FINAM order endpoints.

Consumer behavior:

- consumes M3h-1 broker-neutral runtime shadow inputs;
- deduplicates bars before runtime decision ticks;
- emits a strategy-decision tick only for `LiveFinal` bars;
- accepts `LiveUpdating`, historical/read-only, recovery, and unknown bars as
  non-decision diagnostics;
- blocks inbound `ReadinessPhase::LiveReady` while live gate is forbidden.

Safety behavior:

- `LiveUpdating` bars cannot trigger strategy decisions;
- historical/read-only bars cannot trigger strategy decisions;
- inbound spoofed/stale `LiveReady` cannot reach runtime as live-ready state;
- no `BrokerCommand` is emitted in M3h-2;
- runtime live attachment remains disabled.

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
python3 scripts/m3h2_runtime_shadow_consumer_evidence.py \
  --source-archive reports/handoff/moex-trading-project-<commit>.zip
```
