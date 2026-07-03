# M3g-4 readiness simulation package

M3g-4 combines the M3g readiness inputs into a deterministic simulation report.
It is still a readiness/evidence layer only: it does not enable real FINAM
order endpoint calls, runtime live attachment, `LiveReady`, or strategy
execution.

The simulator combines:

- auth/token/account availability;
- instrument registry validation;
- schedule availability;
- M3f clean reconciliation state;
- own orders freshness;
- own trades freshness;
- positions freshness;
- first-live-bar gate;
- stream/watermark state;
- missing operator live arm.

Readiness behavior:

- all required inputs OK produces `Reconciliation + OperatorLiveArmMissing`;
- all required inputs OK still does not emit `LiveReady`;
- missing/stale auth/account/instrument/schedule/reconciliation blocks
  readiness;
- stale/missing own orders/trades/positions blocks readiness;
- missing first-live-bar gate blocks readiness;
- stale/disconnected stream or snapshot/stream race blocks readiness.

Operator diagnostics:

- `operator_blocker_summary` preserves deterministic blocker kinds;
- each summary row includes affected inputs and count;
- if own orders and own trades are affected by the same transport blocker, the
  count is `2` and both feeds are listed.

Watermark evidence:

- `gap_absence_source` records why `gap_absence_proven` is trusted;
- supported sources are sequence, broker timestamp, replay window, operator
  waiver, or unknown.

Still forbidden:

- real FINAM `POST /orders`;
- real FINAM `DELETE /orders/{id}`;
- non-loopback order endpoint;
- command-consumer-to-real-FINAM transport;
- runtime live attachment;
- `LiveReady`;
- stop/SLTP/brackets/replace/multi-leg.

Evidence:

```bash
python3 scripts/m3g4_readiness_simulation_evidence.py \
  --source-archive reports/handoff/moex-trading-project-<commit>.zip
```
