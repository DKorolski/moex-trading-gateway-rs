# Stage 2B-6 — TradeLedger broker-neutral migration

Status: implementation patch ready for review.

Date: 2026-07-08.

## What changed

Stage 2B-6 adds a broker-neutral `TradeLedger` contract in `broker-core`.

The ledger is based on the mature ALOR `strategy-runtime/src/trade_ledger.rs`
shape, but its identity boundary is migrated to broker-neutral ids:

- `OrderRecord.order_id` uses `BrokerOrderId`;
- `TradeRecord.order_id` uses `BrokerOrderId`;
- `TradeRecord.trade_id`, when present, uses `BrokerTradeId`;
- `TradeLedger` order storage is keyed by exact `BrokerOrderId`;
- legacy ALOR numeric order ids deserialize as decimal-string broker ids;
- broker-native string ids are preserved exactly.

The patch also makes attribution explicit:

- owned strategy orders can receive strategy-attributed fills;
- observed/account-wide orders do not become strategy-owned implicitly;
- trade-before-order records remain pending until the exact broker-order id is
  observed;
- unknown/orphan trade records produce an explicit blocker path instead of being
  silently attributed.

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

- `trade_ledger_preserves_broker_order_id_string`;
- `trade_ledger_records_order_and_fill_with_string_id`;
- `trade_ledger_string_order_id_roundtrip`;
- `legacy_numeric_alor_order_id_imports_as_decimal_string`;
- `trade_with_observed_order_not_strategy_attributed_without_ownership`;
- `trade_before_order_stays_pending_until_exact_broker_order_id_match`;
- `unknown_or_orphan_trade_sets_blocker_or_manual_intervention`;
- `trade_ledger_owned_round_trip_keeps_alor_pnl_semantics`.

## Remaining live blockers

Stage 2B remains paper/mock/local only:

- runtime-live remains disabled;
- real FINAM command consumer remains disabled;
- strategy-driven real FINAM orders remain disabled;
- Stop/SLTP/bracket/replace/multi-leg live behavior remains disabled;
- RI/RTS and USDRUBF remain out of scope;
- `i64` surrogate adapter remains forbidden without a new ADR.
