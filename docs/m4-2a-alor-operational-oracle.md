# M4-2a ALOR operational oracle extraction

Status: design/test extraction only. No live calls.

M4-2a turns the mature ALOR gateway behavior into an executable parity oracle
for the broker-neutral FINAM gateway work. The goal is not to port ALOR code
directly. The goal is to preserve the operational semantics that kept ALOR safe:
instrument-keyed positions, explicit order lifecycle classification, feed
freshness, readiness gates, and separation between target lifecycle truth and
account-wide safety.

## Sources reviewed

- `alor-types/src/strategy.rs`
- `alor-gateway/src/models.rs`
- `alor-gateway/src/state/positions_manager.rs`
- `alor-gateway/src/state/orders_manager.rs`
- `alor-gateway/src/services/command_consumer.rs`
- `alor-gateway/src/supervisor.rs`

## Extracted oracle rules

### Positions

- Position state is keyed by instrument/symbol.
- A broker-native row is not open risk by itself.
- Open risk requires target instrument identity and non-zero normalized qty.
- Other-instrument positions on the same account are account context, not target
  lifecycle truth.

### Orders

- Target lifecycle active orders are target-instrument scoped.
- Account-wide active/unknown/orphan orders remain a live safety guard.
- Terminal statuses include `filled`, `cancelled`, `canceled`, and `rejected`.
- Unknown statuses are not silently terminal.
- Active-looking statuses with `remaining_qty = 0` are inconsistent diagnostics,
  not working active orders.
- Active-looking statuses with missing `remaining_qty` remain blocking.

### Readiness

- Live admission requires synced broker truth feeds.
- Positions, orders, and stop-order feed sync are readiness inputs.
- Market entry requires an open trading window.
- Cancel/close-only paths can have a different emergency policy, but must still
  pass identity and broker-truth guardrails.

### Instruments

- Instrument identity must include venue/board/MIC/contract identity where the
  broker exposes it.
- Same ticker on a different board/MIC/contract is not automatically the same
  instrument.
- Missing lot size, step, tradability, expiration, or schedule data blocks live
  expansion until explicitly waived or fixed.

### Cash/margin/economics

- Cash and margin are account-wide truth.
- Position tests require explicit cash/margin sufficiency before entry.
- Fees/trades must reconcile into position and economics reports before runtime
  attachment.

## M4-2a fixture inventory

Committed synthetic fixtures:

- `fixtures/alor/positions_snapshot_zero_qty.json`
- `fixtures/alor/orders_active_terminal.json`
- `fixtures/alor/readiness_snapshot_synced.json`
- `fixtures/finam/equivalent_positions_snapshot_zero_qty.json`
- `fixtures/finam/equivalent_orders_active_terminal.json`
- `fixtures/expected/canonical_truth_zero_qty_flat_summary.json`
- `fixtures/expected/canonical_truth_order_summary.json`

These fixtures are intentionally synthetic. They encode business invariants,
not live account data.

## M4-2a acceptance

- no live calls;
- broker-neutral order truth is status + instrument + remaining-qty aware;
- target lifecycle counters and account-wide safety counters are structured;
- zero-quantity target position rows are flat;
- other-instrument open positions do not make target non-flat;
- unknown order statuses remain blocking truth;
- missing FINAM/ALOR parity areas are explicitly listed for M4-2b.
