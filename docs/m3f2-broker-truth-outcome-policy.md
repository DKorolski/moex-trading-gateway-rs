# M3f-2 broker-truth outcome application policy

M3f-2 turns scheduled M3f reconciliation work into deterministic recovery
decisions. It is still a policy layer: no live FINAM order endpoints, no runtime
attachment, and no strategy mutation.

Policies added:

- `GetOrder` is feasible only when `broker_order_id` is present.
- Orders without broker id recover through client-order identity using
  `GetOrders` / `Trades` / `Positions`.
- Identity completeness is explicit per reconciliation kind.
- Requests can be deduplicated by request id, client id hash, broker id hash
  when present, account hash, and instrument hash.
- Broker truth outcomes map to conservative actions:
  - recover by client order id;
  - mark submitted/terminal;
  - recover cancel terminal;
  - keep pending for retry;
  - manual intervention for stale/conflicting/insufficient truth.

Still forbidden:

- real FINAM POST/DELETE;
- non-loopback order endpoint;
- real order execution from command consumer;
- runtime live attachment;
- `LiveReady`;
- stop/SLTP/brackets/replace/multi-leg.

Evidence:

```bash
python3 scripts/m3f2_reconciliation_outcome_policy_evidence.py \
  --source-archive reports/handoff/moex-trading-project-<commit>.zip
```
