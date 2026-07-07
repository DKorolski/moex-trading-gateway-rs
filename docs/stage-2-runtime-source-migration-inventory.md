# Stage 2A runtime source migration inventory

Status: Stage 2A inventory snapshot.

Date: 2026-07-07.

Source inspected:

```text
alor_project/bybit_barter_test_sanitized/alor-rs-main/
```

This inventory is read-only evidence. It identifies ALOR-centered assumptions
that must be migrated before the FINAM contour can attach to the existing
runtime semantics. It does not authorize runtime-live or real FINAM sends.

## Summary

The existing runtime already has a strong `StrategyRequestId`/UUID discipline.
The main migration risk is not request identity; it is broker-native identity
and transport shape:

- orders are still keyed by `i64` in runtime state, snapshots, trade
  correlation, ACK handling, and bootstrap;
- stop-order identity is partly string already, but exchange order ids remain
  numeric in places;
- command DTOs still carry `portfolio`, `exchange`, and `symbol` as raw strings;
- active/terminal order detection uses string status buckets;
- default stream names encode the legacy portfolio-shaped namespace;
- runtime state restore exposes known order ids as `Vec<i64>`;
- the hybrid runtime has many pending/deferred/protective/riskgate fields that
  must be preserved even when their live execution remains disabled.
- the concrete `HybridIntradayRuntime` implementation owns working-order,
  protective-order, bootstrap, stop-trigger, and restore logic that must migrate
  as behavior, not merely as serialized field types;
- the trade ledger, command builders, and ALOR cancel/replace DTOs still expose
  numeric broker-order identity and must be included in the Stage 2B migration
  plan.

## Inventory table

| File | Line/function | Old assumption | Risk | Migration action | Test required |
| --- | --- | --- | --- | --- | --- |
| `strategy-runtime/src/strategy_host.rs` | `Intent::Cancel`, lines 33-35 | Cancel target is `order_id: i64`. | FINAM broker ids are strings; lossy surrogate would break cancel/reconciliation. | Replace with `BrokerOrderId` in source migration. Legacy numeric id imports as decimal string. | Cancel intent serde migration and compile-only strategy trait test. |
| `strategy-runtime/src/strategy_host.rs` | `Intent::Replace`, lines 36-40 | Replace target is `order_id: i64`. | Replace is out of scope; accidental migration could enable unsupported path. | Keep replace disabled; migrate type only behind later replace gate. | Forbidden-surface/no-live test. |
| `strategy-runtime/src/strategy_host.rs` | `CreateStopLimit`, lines 41-50 | ALOR-specific stop-limit fields include `instrument_group`. | FINAM Stop/SLTP/bracket semantics are not approved. | Preserve markers only; classify as `future_stop_bracket_only`. | Feature-disabled protective-order fixture. |
| `strategy-runtime/src/strategy_host.rs` | `tracked_order_ids() -> Vec<i64>`, lines 144-146 | Strategy hook returns numeric broker ids. | Runtime restore/adoption cannot represent FINAM ids. | Replace with typed string broker id list or migrate via compatibility shim inside Stage 2B. | Runtime-state-restored fixture with string ids. |
| `strategy-runtime/src/strategy_host.rs` | `StrategyCtx`, lines 199-210 | Context uses raw `portfolio`, `exchange`, `symbol` strings. | Broker-neutral runtime needs account/instrument identity, not ALOR config names. | Introduce typed `RuntimeStrategyContext`/aliases while preserving legacy config names as input. | Instrument/account alias config test. |
| `strategy-runtime/src/strategy_host.rs` | `OrderEvent`, lines 257-280 | `order_id: i64`, string status/side/type. | String broker ids and canonical status cannot be represented. | Map to broker-neutral order event/snapshot with `BrokerOrderId` and canonical lifecycle. | ALOR order fixture -> canonical order snapshot. |
| `strategy-runtime/src/strategy_host.rs` | `TradeEvent`, lines 283-300 | Trade correlates through `order_id: i64`. | Orphan trade detection cannot match FINAM string ids. | Use `BrokerTradeId` and `BrokerOrderId`. | Trade-to-order correlation fixture. |
| `strategy-runtime/src/strategy_host.rs` | `StopOrderEvent`, lines 303-328 | `stop_order_id: String`, `exchange_order_id: Option<i64>`. | Mixed id shapes can silently lose protective-state identity. | Wrap stop id as broker id or future stop id type; exchange id becomes string if preserved. | Protective placeholder migration fixture. |
| `strategy-runtime/src/strategy_host.rs` | `BootstrapSnapshot`, lines 363-367 | Working orders keyed as `HashMap<i64, OrderEvent>`. | Bootstrap truth cannot represent FINAM active order ids. | Key active orders by `BrokerOrderId`; feed from `RuntimeHostBootstrapSnapshot`. | Broker truth bootstrap fixture. |
| `strategy-runtime/src/strategy_host.rs` | `RuntimeStateRestored`, lines 371-373 | Known order ids are `Vec<i64>`. | Stale pending cleanup and adoption lose FINAM order ids. | Change to `Vec<BrokerOrderId>`; count semantics remain. | Old state -> migrated restored state. |
| `strategy-runtime/src/state.rs` | `StrategyState::Placed`, lines 113-119 | `order_id: Option<i64>`. | Legacy paper/live state stores numeric broker id. | Migrate to string broker id field in versioned state. | Legacy placed-state migration fixture. |
| `strategy-runtime/src/state.rs` | `HybridIntradayRuntime`, lines 229-341 | Pending/deferred/protective/riskgate fields are serialized strategy state. | Dropping any field changes strategy behavior or hides dirty-start risk. | Preserve every field; map ids to typed strings; unsupported execution remains blocker. | Existing Stage 1B fixtures plus old->new->restored tests. |
| `strategy-runtime/src/state.rs` | `tp_order_id`, `sl_exchange_order_id`, lines 273-277 | Protective broker/exchange ids can be numeric. | Stop/bracket repair cannot be safely represented for FINAM later. | Preserve as string ids; live stop/bracket remains disabled. | Protective state fixture. |
| `strategy-runtime/src/state.rs` | `CancelSent`, lines 476-479 | Cancel state uses `order_id: i64`. | Cancel resume/recovery cannot handle FINAM ids. | Migrate to `BrokerOrderId`. | CancelSent legacy migration test. |
| `strategy-runtime/src/state.rs` | `RuntimeState.orders`, lines 488-496 | Orders are `HashMap<i64, OrderEvent>`. | Runtime persisted state cannot store string broker ids. | Use `HashMap<BrokerOrderId, ...>` or a serializable string-key map wrapper. | Old runtime state JSON with numeric key -> string-key state. |
| `strategy-runtime/src/strategies/hybrid_intraday_runtime.rs` | fields lines 169-191 | Runtime implementation stores `tp_order_id: Option<i64>`, `sl_exchange_order_id: Option<i64>`, and `working_orders: HashSet<i64>`. | Source migration that only updates `state.rs` would leave live behavior numeric and break protective cancel/repair tracking. | Migrate implementation-owned ids to `BrokerOrderId`; keep Stop/SLTP/bracket live paths disabled until their separate gate. | `hybrid_runtime_working_orders_string_id_migration`; `hybrid_runtime_tp_order_id_string_id_migration`; `hybrid_runtime_sl_exchange_order_id_string_id_migration`. |
| `strategy-runtime/src/strategies/hybrid_intraday_runtime.rs` | `emit_cancel_all_protection()`, lines 1299-1329 | Protective cancel emits `Intent::Cancel { order_id: i64 }` from TP/SL exchange ids. | Numeric cancel target would force a forbidden FINAM surrogate or drop protective cleanup. | Emit cancel intents with `BrokerOrderId`; retain protective-order feature-disabled boundary for real FINAM. | `hybrid_runtime_cancel_all_protection_uses_string_broker_order_id`. |
| `strategy-runtime/src/strategies/hybrid_intraday_runtime.rs` | `emit_partial_entry_timeout_exit()`, lines 1406-1435 | Partial-entry timeout consults numeric `working_orders`. | Timeout exit/cancel behavior can diverge if string broker ids are not tracked. | Track working orders by broker-order string and preserve timeout semantics. | `hybrid_runtime_partial_entry_timeout_preserves_working_order_string_ids`. |
| `strategy-runtime/src/strategies/hybrid_intraday_runtime.rs` | `on_order()`, lines 5361-5391 | Order adoption uses numeric `ord.order_id`, including TP assignment and `working_orders` insert/remove. | FINAM order ids cannot pass numeric `ord.order_id > 0` style checks; active/terminal cleanup can be wrong. | Replace numeric-positive checks with non-empty `BrokerOrderId` validation and canonical lifecycle mapping. | `hybrid_runtime_on_order_non_empty_string_id_replaces_order_id_gt_zero`. |
| `strategy-runtime/src/strategies/hybrid_intraday_runtime.rs` | `on_stop_order()`, lines 5397-5452 | Stop trigger handling stores `exchange_order_id: Option<i64>` and cancels TP by numeric id. | Stop-order and exchange-order identity becomes mixed and lossy for FINAM. | Preserve stop ids as typed strings/markers; real stop/bracket remains blocked until M4+ protective-order design. | `hybrid_runtime_stop_order_exchange_id_string_marker`. |
| `strategy-runtime/src/strategies/hybrid_intraday_runtime.rs` | `on_bootstrap_snapshot()`, lines 5616-5689 | Bootstrap scans `snapshot.working_orders_strategy` keyed by `i64` and adopts TP/SL ids. | Dirty-start adoption cannot see FINAM active orders without string keys. | Bootstrap from broker-neutral target-instrument active order snapshots keyed by `BrokerOrderId`; account-wide rows diagnostic only. | `hybrid_runtime_bootstrap_working_orders_string_key`. |
| `strategy-runtime/src/strategies/hybrid_intraday_runtime.rs` | `on_runtime_state_restored()`, lines 5690-6004 | Restore copies numeric TP/SL ids and known working-order state. | Restored non-flat/pending/protective state loses FINAM broker ids or silently disables cleanup. | Old numeric ids import as decimal strings; restored state preserves pending/deferred/riskgate/protective markers exactly. | `hybrid_runtime_restored_state_preserves_string_order_ids_and_riskgate`. |
| `strategy-runtime/src/trade_ledger.rs` | `TradeRecord.order_id`, line 12 | Ledger fill record uses `order_id: i64`. | Trade attribution cannot correlate FINAM fills to string broker-order ids. | Use `BrokerOrderId` and legacy numeric import as decimal string. | `trade_ledger_preserves_broker_order_id_string`. |
| `strategy-runtime/src/trade_ledger.rs` | `OrderRecord.order_id`, line 23 | Ledger order record uses `order_id: i64`. | Order/fill lifecycle and PnL attribution remain ALOR-numeric. | Use `BrokerOrderId`; keep report serialization redacted/typed. | `trade_ledger_records_order_and_fill_with_string_id`. |
| `strategy-runtime/src/trade_ledger.rs` | `TradeLedger.orders`, lines 66 and 158 | Ledger orders are `HashMap<i64, OrderRecord>` and lookup by `order(order_id: i64)`. | Ledger map cannot store FINAM ids without forbidden surrogate mapping. | Use `HashMap<BrokerOrderId, OrderRecord>` or a serializable string-key map wrapper; lookup takes broker id. | `trade_ledger_string_order_id_roundtrip`. |
| `strategy-runtime/src/runtime.rs` | constants lines 50-62 | Active/terminal order status is stringly typed. | Broker status differences can misclassify active/terminal/unknown. | Move status mapping to broker-neutral `BrokerOrderLifecycle`. Unknown blocks readiness. | Status matrix test. |
| `strategy-runtime/src/runtime.rs` | `OrdersSnapshot`, lines 77-80 | Snapshot orders keyed by `i64`. | Broker truth snapshots from FINAM cannot be represented. | Use `BrokerTruthSnapshot` and broker id string keys. | FINAM/ALOR snapshot canonical parity test. |
| `strategy-runtime/src/runtime.rs` | runtime fields lines 179-195 | `our_order_ids`, `pending_trades_by_order_id`, `pending_exec`, `next_sim_order_id` are numeric. | ACK/trade ownership and simulator can diverge from FINAM string ids. | Convert real broker ids to `BrokerOrderId`; keep simulator ids synthetic and typed distinctly. | ACK ownership and orphan trade tests. |
| `strategy-runtime/src/runtime.rs` | `handle_ack`, lines 1739-1754 | ACK inserts `ack.broker_order_id` numeric into owned ids. | ACK accepted with FINAM string id would not clear/correlate. | Use broker-core `CommandAck` and exact `BrokerOrderId(String)`. | Matching/mismatched request-id tests. |
| `strategy-runtime/src/runtime.rs` | `handle_ack`, lines 1755-1791 | ALOR ACK statuses drive pending behavior. | Stage 2 must preserve semantics while mapping to broker-core ACK statuses/reasons. | Define status mapping table: accepted/submitted/rejected/duplicate/timeout/unknown. | ACK status mapping fixture. |
| `strategy-runtime/src/runtime.rs` | trade handling lines 1928-1967 | Non-positive numeric order id is ignored; orphan detection uses numeric set. | FINAM ids are not numeric; non-empty string validation is needed. | Replace with broker-id non-empty validation and broker-truth recovery. | Orphan trade fixture with string id. |
| `strategy-runtime/src/runtime.rs` | order ledger lines 2201-2255 | Filled order detection uses `status == "filled"` and numeric id. | Different brokers may use different status vocabulary. | Use canonical filled/terminal lifecycle and filled quantity truth. | Filled/partial/terminal status tests. |
| `strategy-runtime/src/runtime.rs` | bootstrap snapshot filtering lines 1230-1355 | Target filtering by symbol string; active orders keyed by numeric id. | Account-wide rows may be mistaken for target truth; FINAM symbol identity can differ. | Filter by `InstrumentId`; account-wide rows diagnostic; target active/unknown blocks. | Target-symbol active order and account-wide diagnostics tests. |
| `strategy-runtime/src/runtime.rs` | `notify_runtime_state_restored`, lines 3132-3166 | Restored known ids copied from numeric `our_order_ids`. | Restored strategy hooks lose FINAM ids. | Expose `Vec<BrokerOrderId>` and pending `StrategyRequestId`. | Restore hook fixture. |
| `strategy-runtime/src/runtime.rs` | `log_bootstrap_dump`, lines 3205-3250 | Diagnostic order dump contains numeric ids and raw comments. | Review/handoff must avoid raw sensitive comments and support string ids. | Redact comments; use typed ids. | Redaction fixture. |
| `strategy-runtime/src/lib.rs` | `deterministic_request_id()`, lines 795-829 | Request UUID namespace includes raw `portfolio` string: `strategy_id|portfolio|symbol|action|bar_ts|seq`. | Migrating `portfolio` to account alias can accidentally change request ids and break pending/ACK parity. | Preserve the legacy portfolio/account alias string used for UUID namespace, or prove `BrokerAccountId` renders identically for migrated configs. | `deterministic_request_id_is_stable_after_account_alias_migration`. |
| `strategy-runtime/src/lib.rs` | `build_place_command()`, lines 832-866 | Place command builder writes raw `portfolio`, `exchange`, and `symbol`. | Runtime command boundary remains ALOR-shaped even if strategy state migrates. | Build broker-neutral command from typed account/instrument while accepting legacy config aliases. | `build_place_command_maps_account_and_instrument_aliases`. |
| `strategy-runtime/src/lib.rs` | `build_cancel_command()`, lines 869-886 | Cancel builder takes `order_id: i64` and emits ALOR `CancelOrder`. | Cancel path cannot represent FINAM broker ids. | Accept `BrokerOrderId` and emit broker-neutral cancel command; legacy numeric cancel imports as string. | `legacy_cancel_command_numeric_id_imports_as_string`. |
| `strategy-runtime/src/lib.rs` | `RuntimeCaches.orders`, lines 892-893 | Runtime cache stores `HashMap<i64, OrderEvent>`. | Cache ownership and bootstrap truth diverge from string broker ids. | Use broker-order string keys and canonical order snapshots/events. | `runtime_caches_orders_use_string_broker_order_id`. |
| `strategy-runtime/src/config.rs` | `default_streams`, lines 2285-2297 | Default stream names are derived from legacy portfolio strings. | FINAM contour needs broker-neutral roles and isolated namespaces. | Keep role names stable; configure concrete stream names per contour. | Stream role mapping test. |
| `alor-protocol/src/lib.rs` | `OrderCommand`, lines 66-78 | Command carries `portfolio`, `exchange`, `symbol` raw strings. | Command producer/consumer boundary is ALOR-shaped. | Convert into broker-core `BrokerCommand` with typed account/instrument. | Legacy command decode -> broker command mapper test. |
| `alor-protocol/src/lib.rs` | `CommandAck`, lines 127-143 | Primary `broker_order_id` is `Option<i64>`; string id is auxiliary. | This is the opposite of the accepted ADR for FINAM. | Make string broker id primary; legacy numeric only compatibility input. | ACK with both ids conflict test. |
| `alor-protocol/src/lib.rs` | `CancelOrder`, lines 267-269 | Cancel DTO stores `order_id: i64`. | DTO migration hidden inside `CommandAction` would miss direct serde and helper usage. | Migrate cancel DTO to `BrokerOrderId`; old numeric JSON imports as decimal string. | `legacy_cancel_order_numeric_id_imports_as_string`. |
| `alor-protocol/src/lib.rs` | `ReplaceOrder`, lines 272-276 | Replace DTO stores `order_id: i64` plus mutable order fields. | Replace remains unsupported; migrating it accidentally could imply FINAM replace support. | Migrate shape only as disabled/future contract; real replace remains feature-disabled. | `replace_order_shape_migrated_but_feature_disabled`. |
| `alor-gateway/src/models.rs` | `OrderEvent`/`TradeEvent`, lines 29-55 | Gateway publication emits numeric order ids. | FINAM mapper must not mimic numeric ids. | FINAM emits broker-neutral snapshots; ALOR numeric ids imported as strings in oracle mode. | ALOR fixture -> canonical snapshot parity test. |
| `alor-gateway/src/models.rs` | `OrdersSnapshot`, lines 76-79 | Snapshot map key is `i64`. | FINAM snapshots cannot share this shape without surrogate adapter. | Use broker-neutral snapshot vector or string-key map. | Snapshot migration fixture. |
| `alor-gateway/src/services/command_consumer.rs` | command idempotency lines 94-103 | Idempotency keyed by UUID request id. | This part is good and must be preserved. | Keep `StrategyRequestId` exactness. | Duplicate request id test. |
| `alor-gateway/src/services/command_consumer.rs` | CWS ACK handling lines 528-664 | Broker id is taken from ALOR CWS response and request map uses numeric id. | FINAM ACK may not include broker id immediately or may be string. | Use durable request/client/broker chain and reconciliation-required state. | Accepted-without-broker-id test. |

## Stage 2B implementation order recommended by inventory

1. Introduce broker-neutral runtime event/id aliases and migration helpers.
2. Migrate `CommandAck` and pending-clear path to exact `StrategyRequestId`
   plus `BrokerOrderId(String)`.
3. Migrate persisted `RuntimeState.orders`, `known_order_ids`, and
   `tracked_order_ids`.
4. Migrate bootstrap snapshot and order/trade correlation.
5. Replace string status checks with canonical lifecycle mapping.
6. Add paper/mock command consumer tests.
7. Only then consider implementation review for a constrained IMOEXF
   `HybridIntradayRuntime` subset.

## Stage 2A-final test backlog additions

The Stage 2B implementation plan must explicitly schedule these tests before
runtime-live can be discussed:

- `hybrid_runtime_working_orders_string_id_migration`;
- `hybrid_runtime_tp_order_id_string_id_migration`;
- `hybrid_runtime_sl_exchange_order_id_string_id_migration`;
- `hybrid_runtime_on_order_non_empty_string_id_replaces_order_id_gt_zero`;
- `hybrid_runtime_bootstrap_working_orders_string_key`;
- `hybrid_runtime_restored_state_preserves_string_order_ids_and_riskgate`;
- `trade_ledger_preserves_broker_order_id_string`;
- `trade_ledger_records_order_and_fill_with_string_id`;
- `trade_ledger_string_order_id_roundtrip`;
- `deterministic_request_id_is_stable_after_account_alias_migration`;
- `legacy_cancel_command_numeric_id_imports_as_string`;
- `legacy_cancel_order_numeric_id_imports_as_string`;
- `replace_order_shape_migrated_but_feature_disabled`.

These are paper/mock/source-migration tests only. They do not authorize
real FINAM order placement, cancel, replace, Stop/SLTP/bracket, or Runtime
`LiveReady`.

## Explicit blockers

The following remain blockers after Stage 2A:

- real FINAM command consumer;
- FINAM Runtime `LiveReady`;
- strategy-driven real FINAM sends;
- Stop/SLTP/bracket/replace/multi-leg;
- RI/RTS/USDRUBF source migration;
- i64 surrogate adapter without a new ADR.
