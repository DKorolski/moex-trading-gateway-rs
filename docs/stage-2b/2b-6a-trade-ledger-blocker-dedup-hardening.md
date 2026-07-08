# Stage 2B-6a — TradeLedger blocker lifecycle and duplicate fill hardening

Status: implementation patch ready for review.

Date: 2026-07-08.

## What changed

Stage 2B-6a hardens the broker-neutral `TradeLedger` foundation before the next
runtime source migration patch.

Blocker lifecycle:

- `blockers()` now means current active blockers;
- `active_blockers()` is an explicit alias for current blockers;
- `blocker_history()` preserves audit history separately;
- `PendingExactBrokerOrderMatch` is resolved when the exact broker-order id is
  later observed as a strategy-owned order;
- if the exact order is observed/account-wide but not strategy-owned, the
  pending blocker is resolved and replaced by `ObservedOrderNotStrategyOwned`.

Duplicate replay handling:

- fills with `Some(BrokerTradeId)` are deduplicated by
  `(BrokerTradeId, BrokerOrderId)`;
- duplicate replay returns `TradeLedgerFillDisposition::DuplicateIdempotent`;
- duplicate fills do not change position, PnL, pending trades, observed trades,
  or closed trades.

## What did not change

- No `HybridIntradayRuntime` trading behavior changed.
- No BO/MR strategy decision logic changed.
- No command builders changed.
- No real FINAM command consumer was connected.
- No real FINAM `POST`/`DELETE` path was enabled.
- No runtime-live or FINAM `LiveReady` was enabled.
- No Stop/SLTP/bracket/replace/multi-leg live behavior was enabled.
- No RI/RTS or USDRUBF migration was started.
- No `i64` surrogate adapter was introduced.

## Tests added

- `pending_trade_blocker_resolves_after_exact_owned_order_match`;
- `pending_trade_blocker_turns_into_observed_blocker_after_exact_observed_order_match`;
- `duplicate_trade_id_for_owned_order_is_idempotent_and_does_not_double_count_pnl`;
- `duplicate_trade_id_for_pending_trade_is_idempotent`;
- `duplicate_trade_id_for_observed_order_is_idempotent`;
- `trade_before_order_then_exact_order_has_no_active_pending_exact_blocker`.

## Remaining live blockers

Stage 2B remains paper/mock/local only:

- runtime-live remains disabled;
- real FINAM command consumer remains disabled;
- strategy-driven real FINAM orders remain disabled;
- Stop/SLTP/bracket/replace/multi-leg live behavior remains disabled;
- RI/RTS and USDRUBF remain out of scope;
- `i64` surrogate adapter remains forbidden without a new ADR.
