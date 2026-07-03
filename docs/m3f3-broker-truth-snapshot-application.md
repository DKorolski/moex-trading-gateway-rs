# M3f-3 broker-truth snapshot application

M3f-3 applies simulated/read-only broker-truth snapshots to M3f reconciliation
requests. It is still a read-only reconciliation layer: it does not add real
FINAM order endpoints, runtime live attachment, command-consumer execution, or
stop/SLTP/bracket semantics.

Snapshot inputs:

- `GetOrders` / `GetOrder` order snapshots;
- trade snapshots;
- position consistency snapshots.

Policies added:

- Same account/client/broker/instrument identity with a different
  `request_id` becomes `ManualInterventionRequired`.
- Client-order recovery that finds a terminal broker order is explicit:
  `RecoverByClientOrderIdAndTerminal` in one decision.
- Broker orders not linked to local order-path requests are orphan broker
  orders and require manual intervention.
- Broker trades not linked to local order-path requests are orphan broker
  trades and require manual intervention.
- Position mismatches require manual intervention.
- Local pending requests older than the configured max pending age are marked
  stale and require manual intervention.
- Reports export only hashed identifiers and redacted flags; raw account,
  client-order, broker-order, broker payload, and position payload data are not
  exported.

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
python3 scripts/m3f3_snapshot_application_evidence.py \
  --source-archive reports/handoff/moex-trading-project-<commit>.zip
```
