# Stage 2B-7 — HybridIntradayRuntime-owned ids

Status: implementation patch ready for review.

Date: 2026-07-08.

## What changed

Stage 2B-7 adds a broker-neutral owned-id contract in `broker-core` for the
future `HybridIntradayRuntime` source migration.

The real ALOR `strategy-runtime/src/strategies/hybrid_intraday_runtime.rs` is
not yet part of this repository, so this patch does not pretend to modify that
source file. Instead, it introduces the contract that the runtime source
migration must use:

- `HybridRuntimeOwnedIds.tp_order_id: Option<BrokerOrderId>`;
- `HybridRuntimeOwnedIds.sl_exchange_order_id: Option<BrokerOrderId>`;
- `HybridRuntimeOwnedIds.working_orders: HashSet<BrokerOrderId>`;
- legacy numeric ALOR ids deserialize as decimal-string `BrokerOrderId`;
- broker-native string ids are preserved exactly;
- order updates use present/valid `BrokerOrderId`, not `order_id > 0`;
- bootstrap and restore helpers preserve exact string ids;
- cancel-protection and partial-entry-timeout helpers return
  `BrokerOrderId` targets.

Stop/SLTP/bracket behavior remains future-gated:

- stop-order exchange ids can be preserved as `BrokerOrderId`;
- live Stop/SLTP/bracket execution is still disabled;
- stop/bracket-related helper paths emit an explicit
  `FutureStopBracketOnly` blocker marker.

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
- No request-id, deferred-exit, or riskgate logic changed.

## Tests added

- `hybrid_runtime_working_orders_string_id_migration`;
- `hybrid_runtime_tp_order_id_string_id_migration`;
- `hybrid_runtime_sl_exchange_order_id_string_id_migration`;
- `hybrid_runtime_on_order_non_empty_string_id_replaces_order_id_gt_zero`;
- `hybrid_runtime_bootstrap_working_orders_string_key`;
- `hybrid_runtime_cancel_all_protection_uses_string_broker_order_id`;
- `hybrid_runtime_partial_entry_timeout_preserves_working_order_string_ids`;
- `hybrid_runtime_stop_order_exchange_id_string_marker`;
- `hybrid_runtime_restored_state_preserves_string_order_ids_and_riskgate`;
- `hybrid_runtime_state_writes_new_string_ids`.

## Remaining live blockers

Stage 2B remains paper/mock/local only:

- runtime-live remains disabled;
- real FINAM command consumer remains disabled;
- strategy-driven real FINAM orders remain disabled;
- Stop/SLTP/bracket/replace/multi-leg live behavior remains disabled;
- RI/RTS and USDRUBF remain out of scope;
- `i64` surrogate adapter remains forbidden without a new ADR.
