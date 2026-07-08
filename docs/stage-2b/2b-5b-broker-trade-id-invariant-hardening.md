# Stage 2B-5b — BrokerTradeId invariant hardening

Status: accepted.

Date: 2026-07-07.

## What changed

Stage 2B-5b hardens `BrokerTradeId` before TradeLedger migration.

Added:

- dedicated `BrokerTradeId` type instead of the generic unchecked
  `string_id!` path;
- `BrokerTradeId::from_broker_native_exact(...)`;
- `BrokerTradeIdImportError`;
- serde `try_from = "String"` validation;
- non-empty constructor invariant matching the `BrokerOrderId` style.

Runtime trade behavior:

- native broker trade id strings are preserved exactly;
- empty trade ids are rejected at serde boundary;
- `RuntimeTradeEvent` cannot deserialize with an empty `trade_id`;
- trade dedup continues to use `(BrokerTradeId, BrokerOrderId)`, but now
  `BrokerTradeId` is guaranteed non-empty.
- production broker DTO mappers must use fallible
  `BrokerTradeId::from_broker_native_exact(...)`; `BrokerTradeId::new(...)` is
  allowed only for trusted/test-created non-empty ids.

## What did not change

- No TradeLedger implementation changed.
- No `HybridIntradayRuntime` behavior changed.
- No BO/MR strategy decision logic changed.
- No implementation-owned `working_orders`, `tp_order_id`, or
  `sl_exchange_order_id` behavior changed.
- No command builders changed.
- No real FINAM command consumer was connected.
- No real FINAM `POST`/`DELETE` path was enabled.
- No runtime-live or FINAM `LiveReady` was enabled.
- No Stop/SLTP/bracket/replace/multi-leg live behavior was enabled.
- No RI/RTS or USDRUBF migration was started.
- No `i64` surrogate adapter was introduced.

## Tests added

- `broker_native_trade_id_string_is_preserved_exactly`;
- `empty_broker_native_trade_id_is_rejected`;
- `broker_trade_id_public_constructor_cannot_create_empty`;
- `broker_trade_id_deserialize_empty_string_rejected`;
- `broker_trade_id_deserialize_nonempty_string_preserved_exact`;
- `runtime_trade_event_rejects_empty_trade_id_at_serde_boundary`;
- `runtime_trade_event_preserves_exact_trade_id_roundtrip`.

Existing duplicate trade-event tests continue to prove that dedup uses valid
non-empty `BrokerTradeId` values.

## Remaining live blockers

Stage 2B remains paper/mock/local only:

- runtime-live remains disabled;
- real FINAM command consumer remains disabled;
- strategy-driven real FINAM orders remain disabled;
- Stop/SLTP/bracket/replace/multi-leg live behavior remains disabled;
- RI/RTS and USDRUBF remain out of scope;
- `i64` surrogate adapter remains forbidden without a new ADR.
