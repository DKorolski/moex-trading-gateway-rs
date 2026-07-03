# M3g-1 readiness contract and stream input model

M3g-1 starts the streams/readiness/first-live-bar stage after M3f broker-truth
closure. It defines the readiness contract only; it does not connect live
streams, attach runtime strategies, or permit `LiveReady`.

Required readiness inputs:

- auth ok and token not stale;
- account available;
- instrument registry validated;
- schedule loaded;
- broker-truth reconciliation clean;
- orders input fresh from stream or documented polling fallback;
- trades input fresh from stream or documented polling fallback;
- positions snapshot fresh;
- first live bar observed from `MarketDataSourceKind::LiveStream`;
- stream transport not stale.

Readiness behavior:

- any reconciliation blocker produces `ReadinessPhase::Blocked`;
- missing/stale orders, trades, positions, schedule, account, auth, or stream
  input produce readiness blockers;
- historical/read-only bars cannot satisfy first-live-bar gate;
- even when every input is present, M3g-1 returns `Reconciliation` with
  `OperatorLiveArmMissing`, not `LiveReady`.

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
python3 scripts/m3g1_readiness_contract_evidence.py \
  --source-archive reports/handoff/moex-trading-project-<commit>.zip
```
