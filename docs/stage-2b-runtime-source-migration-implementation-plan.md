# Stage 2B — runtime source migration implementation plan

Status: draft plan for review; implementation is not authorized until this plan
is accepted.

Date: 2026-07-07.

Stage 2A is accepted and closed. It inventoried the ALOR-centered runtime
surfaces that must migrate before the FINAM contour can attach to the existing
strategy semantics. Stage 2B defines the controlled implementation order for
that migration.

This document is a plan only. It does not modify runtime behavior, enable
runtime-live, or authorize real FINAM order placement/cancel.

## 1. Scope

Stage 2B covers the broker-neutral source migration needed for the IMOEXF
`HybridIntradayRuntime` subset:

- runtime-facing broker/account/instrument/order/trade id types;
- passive DTO/state migration from numeric ALOR broker ids to
  `BrokerOrderId(String)`;
- legacy serde/import support for old ALOR numeric ids as decimal strings;
- `RuntimeState.orders`, bootstrap snapshots, runtime caches, and ownership
  tracking;
- `CommandAck`, `OrderEvent`, `TradeEvent`, cancel DTOs, and command builders;
- trade ledger order/fill correlation;
- `HybridIntradayRuntime` implementation-owned working/protective ids;
- paper/mock compatibility tests proving state, pending, deferred, riskgate,
  and request-id behavior preservation.

The accepted architecture remains source migration to broker-neutral contract
v2. `BrokerOrderId(String)` is the authoritative broker-order identity.

## 2. Hard non-goals

Stage 2B must not do any of the following:

- real FINAM `POST`/`DELETE`;
- real FINAM command consumer;
- FINAM Runtime `LiveReady`;
- strategy-driven live orders;
- runtime-driven live micro;
- Stop/SLTP/bracket/replace/multi-leg live behavior;
- RI/RTS migration;
- USDRUBF migration;
- `SessionGapStandalone` migration;
- i64 surrogate adapter;
- binary-compatible adapter that hides string broker ids behind local numeric
  ids;
- changing BO/MR strategy trading logic under the name of type migration.

Replace DTO shape can be migrated only as a disabled/future contract. It must
not imply replace support.

## 3. Implementation order

Stage 2B implementation should be split into small reviewable patches. Each
patch must keep tests green and must not open the trading boundary.

1. Introduce broker-neutral runtime-facing id aliases/types.
   - `BrokerOrderId(String)`;
   - `BrokerTradeId(String)`;
   - `BrokerAccountId` / `AccountId` alias;
   - `InstrumentId` / broker symbol map at the runtime boundary.
2. Migrate passive DTO/state types without behavior changes.
   - Add new typed fields and compatibility helpers first.
   - Keep old behavior reachable only through legacy import paths.
3. Add legacy serde migration for old numeric ALOR ids.
   - `i64` order ids import as decimal-string `BrokerOrderId`;
   - no FINAM string id may be converted to a local numeric surrogate.
4. Migrate `RuntimeState.orders` and `BootstrapSnapshot` working orders.
   - Use broker-order string keys or a typed serializable string-key wrapper;
   - target-instrument active orders remain lifecycle truth;
   - account-wide rows remain safety diagnostics.
5. Migrate `CommandAck`, `OrderEvent`, and `TradeEvent`.
   - ACK pending clear remains exact-`StrategyRequestId` only;
   - broker-order id string must flow through ACK/order/trade/cancel paths.
6. Migrate `RuntimeCaches` and ownership tracking.
   - `our_order_ids`, pending trades by order id, and orphan classification use
     broker-order strings;
   - simulator/synthetic ids stay separately typed and cannot masquerade as
     broker ids.
7. Migrate `trade_ledger`.
   - `TradeRecord.order_id`, `OrderRecord.order_id`, ledger map keys, and
     `TradeLedger::order()` use `BrokerOrderId`;
   - reports/exports stay redacted and string-id safe.
8. Migrate `HybridIntradayRuntime` implementation-owned ids.
   - `tp_order_id`;
   - `sl_exchange_order_id`;
   - `working_orders`;
   - `emit_cancel_all_protection()`;
   - `emit_partial_entry_timeout_exit()`;
   - `on_order()`;
   - `on_stop_order()`;
   - `on_bootstrap_snapshot()`;
   - `on_runtime_state_restored()`.
9. Migrate command builders.
   - `deterministic_request_id()` must remain stable under account aliasing;
   - `build_place_command()` maps account/instrument aliases without changing
     strategy decision identity;
   - `build_cancel_command()` accepts broker-order strings.
10. Migrate ALOR cancel/replace DTO shape.
    - `CancelOrder.order_id` becomes broker-order string;
    - `ReplaceOrder.order_id` shape migrates, but replace remains disabled.
11. Run paper/mock compatibility tests and scanners after each patch.

## 4. Type-change rules

| Old surface | Stage 2B target | Rule |
| --- | --- | --- |
| `order_id: i64` | `BrokerOrderId(String)` | ALOR numeric ids import as decimal strings; FINAM ids stay exact strings. |
| `broker_order_id: Option<i64>` | `Option<BrokerOrderId>` | String id is primary; conflicting numeric/string legacy fields block. |
| `trade_id: String` | `BrokerTradeId(String)` | Preserve broker-native id exactly. |
| `portfolio: String` | `BrokerAccountId` / alias | Preserve legacy account namespace for request-id stability. |
| `symbol/exchange: String` | `InstrumentId` + broker symbol | Strategy symbol and broker venue symbol are separated. |
| string order status | canonical lifecycle | Unknown status blocks readiness. |
| `HashMap<i64, ...>` / `HashSet<i64>` | string-key map/set | No numeric surrogate for FINAM. |

## 5. State schema migration

Stage 2B must add versioned state migration with these invariants:

1. Old ALOR state snapshots deserialize.
2. Numeric ALOR broker ids convert to decimal-string `BrokerOrderId`.
3. Existing stop/protective string ids are preserved exactly.
4. `StrategyRequestId` values are unchanged.
5. `ClientOrderId` remains separate from `StrategyRequestId`.
6. Pending entry/exit/TP/SL request ids survive old -> new -> restored.
7. Deferred entry/exit state survives old -> new -> restored.
8. Riskgate shadow/pending/summary fields survive old -> new -> restored.
9. Manual-intervention and dirty-start markers survive old -> new -> restored.
10. Unsupported Stop/SLTP/bracket/replace state remains a live blocker, not an
    implicit live feature.

## 6. Redis and serde compatibility

Stage 2B keeps existing semantic channel roles while migrating payload shapes:

- market data remains canonical final M10 strategy input;
- broker snapshots become broker-neutral target/account truth;
- commands become broker-neutral paper/mock commands before any live discussion;
- ACKs use exact request-id matching and broker-order string ids;
- runtime state is versioned and old snapshots remain readable;
- DLQ payloads stay redacted.

Consumer-group behavior must remain ALOR-parity oriented:

- `XREADGROUP` for normal consumption;
- `XAUTOCLAIM` or equivalent stale-pending recovery;
- XACK only after ACK/state publish or DLQ publish;
- bounded retention;
- no raw broker payloads, secrets, live account ids, local paths, or raw logs in
  clean source handoff archives.

## 7. Strategy behavior preservation

The migration must not change BO/MR trading decisions. It may change only the
identity representation and boundary plumbing.

Required preservation points:

- deterministic request-id generation remains stable after account alias
  migration;
- ACK matching still clears pending state only by exact `StrategyRequestId`;
- mismatched ACK request id never clears pending state;
- non-flat, pending, deferred, riskgate, safe-mode, and manual-intervention
  fields remain behaviorally equivalent after restore;
- bootstrap adoption stays target-instrument scoped;
- account-wide positions/orders stay diagnostic/safety guard, not target
  lifecycle truth;
- replace remains feature-disabled.

## 8. Test sequence

Stage 2B implementation must add or preserve the following paper/mock/local
tests before acceptance:

1. `old_alor_hybrid_flat_clean_state_json_migrates_to_broker_neutral`.
2. `old_alor_hybrid_nonflat_state_preserves_cycle_owner_side_qty`.
3. `old_alor_hybrid_pending_entry_preserves_request_id_and_deferred_fields`.
4. `old_alor_hybrid_pending_exit_preserves_request_id`.
5. `old_alor_hybrid_safe_mode_preserves_manual_intervention_markers`.
6. `old_alor_hybrid_riskgate_state_preserves_summary_and_shadow_fields`.
7. `ack_matching_strategy_request_id_clears_pending`.
8. `ack_mismatched_strategy_request_id_does_not_clear_pending`.
9. `broker_order_id_string_is_preserved_in_ack_order_trade_paths`.
10. `numeric_alor_order_id_imports_as_decimal_string_without_surrogate_policy`.
11. `client_order_id_collision_blocks_locally`.
12. `unknown_order_status_maps_to_unknown_and_blocks_readiness`.
13. `target_symbol_flat_ignores_account_wide_zero_rows`.
14. `target_active_order_blocks_entry_but_account_wide_rows_are_diagnostic`.
15. `paper_mock_runtime_command_flow_publishes_ack_or_dlq_before_xack`.
16. `hybrid_runtime_working_orders_string_id_migration`.
17. `hybrid_runtime_tp_order_id_string_id_migration`.
18. `hybrid_runtime_sl_exchange_order_id_string_id_migration`.
19. `hybrid_runtime_on_order_non_empty_string_id_replaces_order_id_gt_zero`.
20. `hybrid_runtime_bootstrap_working_orders_string_key`.
21. `hybrid_runtime_restored_state_preserves_string_order_ids_and_riskgate`.
22. `trade_ledger_preserves_broker_order_id_string`.
23. `trade_ledger_records_order_and_fill_with_string_id`.
24. `trade_ledger_string_order_id_roundtrip`.
25. `deterministic_request_id_is_stable_after_account_alias_migration`.
26. `legacy_cancel_command_numeric_id_imports_as_string`.
27. `legacy_cancel_order_numeric_id_imports_as_string`.
28. `replace_order_shape_migrated_but_feature_disabled`.

All tests must be paper/mock/local. No real FINAM endpoint call is allowed.

## 9. Rollback and review plan

Each Stage 2B implementation patch should be reversible without changing live
systems:

- keep commits small and ordered by the implementation sequence above;
- avoid mixed patches that combine type migration, behavior migration, and
  observability changes;
- retain legacy fixture coverage before deleting any compatibility helper;
- keep Stage 2B review handoffs source-clean;
- do not deploy runtime migration to VPS until a later paper/mock acceptance
  explicitly permits it.

If a patch discovers that source migration is not feasible, work stops and a
new ADR is required before considering any surrogate/adapter fallback.

## 10. Acceptance criteria

Stage 2B implementation can be accepted only when:

- old strategies compile;
- old state snapshots read successfully;
- old pending request ids are preserved;
- request-id ACK handling is unchanged;
- `BrokerOrderId(String)` passes through ACK/order/trade/cancel paths;
- trade ledger preserves string broker ids;
- deterministic request id is stable after account alias migration;
- pending/deferred/riskgate/manual fields survive old -> new -> restored;
- all tests are paper/mock/local;
- no real FINAM endpoint is called;
- forbidden scanners are green;
- `cargo fmt --all -- --check` is green;
- `cargo test --all` is green;
- `cargo clippy --workspace --all-targets -- -D warnings` is green;
- source handoff remains clean and excludes `.env`, `.git`, `target`, `tmp`,
  `reports`, logs, and raw broker payloads.

Passing Stage 2B still does not authorize runtime-live. It only prepares the
broker-neutral runtime source for later paper/mock parity work.
