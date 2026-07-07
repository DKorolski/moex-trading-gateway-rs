# Stage 2B-3 — RuntimeState.orders and bootstrap order-map validation

Status: implementation patch ready for review.

Date: 2026-07-07.

## What changed

Stage 2B-3 adds validated-import contracts for the passive runtime DTO/state
layer introduced in Stage 2B-2:

- `RuntimeStateSnapshot::validate_for_runtime_restore()`;
- `RuntimeBootstrapSnapshotDto::validate_for_bootstrap()`;
- `ValidatedRuntimeStateSnapshot`;
- `ValidatedRuntimeBootstrapSnapshotDto`;
- `RuntimeStateReadinessBlocker`;
- `RuntimeStateValidationError`.

The validators enforce:

- `RuntimeState.orders` map key must equal payload `order_id`;
- `BootstrapSnapshot.working_orders` map key must equal payload `order_id`;
- `BootstrapSnapshot.working_orders_strategy` map key must equal payload
  `order_id`;
- `known_order_ids` cannot contain duplicates after legacy import;
- known order ids that are not present in `orders` become readiness blockers
  and require manual intervention before live readiness.

## What did not change

- No `HybridIntradayRuntime` behavior changed.
- No BO/MR strategy decision logic changed.
- No implementation-owned `working_orders` / `tp_order_id` /
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

- `runtime_state_orders_map_key_must_match_order_event_id`;
- `bootstrap_working_orders_key_must_match_order_event_id`;
- `working_orders_strategy_key_must_match_order_event_id`;
- `known_order_ids_cannot_contain_empty_zero_negative_null_or_duplicates`;
- `known_order_id_missing_from_orders_blocks_readiness_without_losing_state`;
- `new_state_serializes_broker_order_id_keys_as_exact_strings`.

## Remaining live blockers

Stage 2B remains paper/mock/local only:

- runtime-live remains disabled;
- real FINAM command consumer remains disabled;
- strategy-driven real FINAM orders remain disabled;
- Stop/SLTP/bracket/replace/multi-leg live behavior remains disabled;
- RI/RTS and USDRUBF remain out of scope;
- `i64` surrogate adapter remains forbidden without a new ADR.
