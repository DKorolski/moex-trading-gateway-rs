# Stage 2B-5 — RuntimeCaches / ownership tracking

Status: implementation patch ready for review.

Date: 2026-07-07.

## What changed

Stage 2B-5 adds passive broker-neutral runtime cache primitives:

- `RuntimeCaches`;
- `RuntimePendingPath`;
- `RuntimeCacheApplyDisposition`;
- `RuntimeCacheLifecycleBlocker`;
- `RuntimeTradeCacheTarget`;
- `RuntimeCacheOrderApplyOutcome`;
- `RuntimeCacheTradeApplyOutcome`.

The cache model stores:

- `orders: HashMap<BrokerOrderId, RuntimeOrderEvent>`;
- `owned_order_ids: HashSet<BrokerOrderId>`;
- `trades_by_order_id: HashMap<BrokerOrderId, Vec<RuntimeTradeEvent>>`;
- `pending_trades_by_order_id: HashMap<BrokerOrderId, Vec<RuntimeTradeEvent>>`;
- passive pending entry/exit identities for ACK-policy evaluation.

Runtime cache behavior:

- broker order ids remain exact `BrokerOrderId(String)`;
- legacy numeric ALOR ids import as decimal strings through existing state/DTO
  migration helpers;
- `tracked_order_ids()` returns `Vec<BrokerOrderId>`, not `Vec<i64>`;
- duplicate order/trade events are idempotent at the DTO cache layer;
- unknown order lifecycle is represented as a blocker, not terminal/clean;
- trade-before-order events stay pending until an exact broker order id appears;
- pending ACK helpers reuse the accepted Stage 2B-4a ACK status policy.

## What did not change

- No `HybridIntradayRuntime` behavior changed.
- No BO/MR strategy decision logic changed.
- No implementation-owned `working_orders`, `tp_order_id`, or
  `sl_exchange_order_id` behavior changed.
- No trade ledger implementation changed.
- No command builders changed.
- No real FINAM command consumer was connected.
- No real FINAM `POST`/`DELETE` path was enabled.
- No runtime-live or FINAM `LiveReady` was enabled.
- No Stop/SLTP/bracket/replace/multi-leg live behavior was enabled.
- No RI/RTS or USDRUBF migration was started.
- No `i64` surrogate adapter was introduced.

## Tests added

- `runtime_caches_orders_use_string_broker_order_id_and_lookup_exact`;
- `runtime_caches_import_legacy_numeric_alor_ids_as_decimal_strings`;
- `duplicate_order_event_is_idempotent_and_does_not_create_duplicate_cache_entries`;
- `unknown_lifecycle_order_event_stays_blocking_not_terminal_clean`;
- `trade_event_before_order_event_is_pending_then_reconciled_by_exact_broker_id`;
- `runtime_cache_pending_ack_helper_respects_stage_2b4a_policy`.

## Remaining live blockers

Stage 2B remains paper/mock/local only:

- runtime-live remains disabled;
- real FINAM command consumer remains disabled;
- strategy-driven real FINAM orders remain disabled;
- Stop/SLTP/bracket/replace/multi-leg live behavior remains disabled;
- RI/RTS and USDRUBF remain out of scope;
- `i64` surrogate adapter remains forbidden without a new ADR.
