# Stage 2B-8 — Command builders / CancelOrder / ReplaceOrder DTO shape

Status: accepted.

Date: 2026-07-08.

## What changed

Stage 2B-8 migrates command/cancel/replace DTO shape to the broker-neutral
`BrokerOrderId(String)` contract.

Changed in `broker-core::command`:

- `CancelOrder.order_id` now imports legacy ALOR numeric ids as decimal strings;
- broker-native cancel order id strings are preserved exactly;
- empty cancel order ids are rejected by serde/import;
- `build_cancel_command(...)` accepts and produces `BrokerOrderId`;
- `ReplaceOrder` DTO shape was added with `order_id: BrokerOrderId`;
- legacy numeric replace ids import as decimal strings;
- replace is explicitly represented as feature-disabled through
  `ReplaceOrder::feature_disabled()`.

Replace remains future-gated:

- `BrokerCommand` still has no replace variant;
- no replace command consumer was added;
- no replace execution path was enabled.

## What did not change

- No `HybridIntradayRuntime` trading behavior changed.
- No BO/MR strategy decision logic changed.
- No deterministic request-id generation changed.
- `ClientOrderId` does not replace `StrategyRequestId`.
- No real FINAM command consumer was connected.
- No real FINAM `POST`/`DELETE` path was enabled.
- No runtime-live or FINAM `LiveReady` was enabled.
- No Stop/SLTP/bracket/replace/multi-leg live behavior was enabled.
- No RI/RTS or USDRUBF migration was started.
- No `i64` surrogate adapter was introduced.

## Tests added

- `cancel_order_id_uses_broker_order_id`;
- `legacy_numeric_cancel_id_imports_as_decimal_string`;
- `broker_native_cancel_id_string_preserved_exact`;
- `empty_cancel_order_id_rejected`;
- `build_cancel_command_accepts_broker_order_id_without_numeric_logic`;
- `replace_order_id_uses_broker_order_id_but_replace_remains_disabled`;
- `legacy_numeric_replace_id_imports_as_decimal_string`.

## Remaining live blockers

Stage 2B remains paper/mock/local only:

- runtime-live remains disabled;
- real FINAM command consumer remains disabled;
- strategy-driven real FINAM orders remain disabled;
- Stop/SLTP/bracket/replace/multi-leg live behavior remains disabled;
- RI/RTS and USDRUBF remain out of scope;
- `i64` surrogate adapter remains forbidden without a new ADR.
