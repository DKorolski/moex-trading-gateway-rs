# M3f-1 broker-truth reconciliation contract

M3f-1 starts broker-truth reconciliation after M3e command-consumer closure.
This step is contract/scheduler-only: it models reconciliation work and safe
diagnostics without fetching live data or mutating runtime strategy state.

Modeled read-only inputs:

- `GetOrders`
- `GetOrder`
- `Trades`
- `Positions`

Order-path states that schedule reconciliation:

- `SubmittedPendingBrokerOrderId`
- `TimeoutUnknownPending`
- `CancelSubmitted`
- `CancelTimeoutUnknownPending`

Stable, terminal, rejected, and ordinary submitted states are ignored by the
M3f-1 scheduler.

Safety boundary:

- read-only FINAM surfaces only;
- no real FINAM POST/DELETE;
- no non-loopback order endpoint;
- no runtime/live attachment;
- no `LiveReady`;
- no stop/SLTP/bracket/replace/multi-leg surface;
- exported identity is redacted to presence flags and SHA-256 values only.

Evidence:

```bash
python3 scripts/m3f1_reconciliation_contract_evidence.py \
  --source-archive reports/handoff/moex-trading-project-<commit>.zip
```

Required evidence booleans:

```text
m3f1_reconciliation_contract_ok = true
get_orders_get_order_trades_positions_modeled = true
requests_created_from_required_states = true
redacted_identity_only = true
read_only_finam_surfaces_only = true
real_finam_order_endpoint_used = false
external_order_endpoint_allowed = false
runtime_live_attachment_allowed = false
live_ready_allowed = false
```
