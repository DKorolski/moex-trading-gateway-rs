# M3f-4 reconciliation runner, reporting, and readiness blockers

M3f-4 closes the first broker-truth reconciliation runner layer. It still uses
modeled/read-only snapshots supplied to the runner; it does not fetch through
live order endpoints and does not attach runtime strategies.

Runner flow:

- load unresolved order-path records from the durable `OrderPathStore`;
- schedule M3f reconciliation requests;
- apply modeled broker-truth order/trade/position snapshots;
- persist safe order-path transitions;
- publish/write a redacted runner report;
- convert reconciliation issues into readiness blockers.

Persistence policies:

- `RecoverByClientOrderIdAndTerminal` persists recovered broker id and terminal
  state atomically in one store update.
- Manual/stale/conflicting cases transition to `ManualInterventionRequired`.
- Keep-pending decisions update local reconciliation freshness without blind
  retry.

Readiness blockers:

- `SameIdentityDifferentRequestId`;
- `OrphanBrokerOrder`;
- `OrphanBrokerTrade`;
- `PositionMismatch`;
- `LocalPendingStale`;
- `ManualInterventionRequired`.

Reports export counts, hashes, blocker kinds, and redacted flags only. Raw
account ids, client order ids, broker order ids, broker payloads, trade
payloads, and position payloads are not exported.

Still forbidden:

- real FINAM `POST /orders`;
- real FINAM `DELETE /orders/{id}`;
- non-loopback order endpoint;
- real order execution from command consumer;
- runtime live attachment;
- `LiveReady`;
- stop/SLTP/brackets/replace/multi-leg.

Evidence:

```bash
python3 scripts/m3f4_reconciliation_runner_evidence.py \
  --source-archive reports/handoff/moex-trading-project-<commit>.zip
```
