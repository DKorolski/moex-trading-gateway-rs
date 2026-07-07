# Stage 2A — runtime source migration inventory and plan

Status: active design/prep.

Date: 2026-07-07.

This stage prepares the accepted migration path for the original
strategy-runtime source. It does not implement runtime-live behavior and does
not expand the order-emitting surface.

## Accepted decision

The architecture fork is already closed by
[ADR — runtime source migration vs ALOR-compatible adapter](adr-runtime-compat-adapter-vs-source-migration.md).

Accepted:

- migrate the runtime source to broker-neutral contract v2;
- keep `BrokerOrderId(String)` as the authoritative broker order identity;
- keep `StrategyRequestId` as the runtime pending/ACK identity;
- keep `ClientOrderId` as a broker-correlation id, not a replacement for
  `StrategyRequestId`.

Not allowed without a new ADR:

- `FINAM broker_order_id -> i64` surrogate mapping;
- binary-compatible adapter that hides string broker ids behind local numeric
  ids;
- lossy or non-bijective id mapping.

## Stage 2A boundary

Allowed in Stage 2A:

- inventory of ALOR runtime source assumptions;
- old ALOR field/type to broker-neutral field/type migration matrix;
- schema migration design for runtime state;
- paper/mock-only command and ACK compatibility plan;
- fixture-backed test plan;
- CI/scanner hardening that does not change trading behavior.

Forbidden in Stage 2A:

- real FINAM command consumer;
- strategy intent to real FINAM `POST`/`DELETE`;
- FINAM Runtime `LiveReady`;
- runtime-driven live micro;
- Stop/SLTP/bracket/replace/multi-leg;
- USDRUBF/RI/SessionGap source migration;
- changing BO/MR strategy trading logic.

## Runtime-facing target types

| Runtime-facing concept | Target type | Source of truth | Stage 2A rule |
| --- | --- | --- | --- |
| Runtime pending command id | `StrategyRequestId` | runtime source | Preserve exact UUID/string identity. ACK matching is by exact request id only. |
| Broker account / portfolio | `BrokerAccountId` / `AccountId` alias | broker account map | Replace implicit portfolio strings at the runtime boundary. Preserve legacy config names only as input aliases. |
| Broker order id | `BrokerOrderId(String)` | broker truth / ACK / order snapshot | String is authoritative. Numeric ALOR ids are converted to strings only as legacy data import, never as FINAM surrogate. |
| Broker trade id | `BrokerTradeId(String)` | broker trade snapshot | Preserve broker-native string. |
| Client correlation id | `ClientOrderId` | order-path store | Derived/validated from request id before send; collision blocks locally. |
| Instrument identity | `InstrumentId` + `BrokerSymbol` | instrument registry | Separate internal strategy symbol from broker venue symbol. |
| Order lifecycle | `BrokerOrderLifecycle` / `OrderLifecycleClass` | canonical order mapper | Replace string status checks with active/terminal/unknown lifecycle classification. |
| Command ACK | `CommandAck` + `CommandAckStatus` + `CommandAckReasonCode` | broker-neutral command lifecycle | Paper/mock first. No pending clear on mismatched request id. |
| Bootstrap truth | `RuntimeHostBootstrapSnapshot` | `BrokerTruthSnapshot` | Broker truth is loaded before runtime state is trusted. |
| Runtime state | versioned broker-neutral strategy state | runtime source + migration layer | Old ALOR JSON must deserialize and reserialize without losing pending/deferred/riskgate/manual fields. |

## Migration matrix

| Old ALOR type/field | Current source file | New broker-neutral type/field | Migration rule | Serialization compatibility rule | Fixture/test | Runtime-live impact | Status |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `portfolio: String` in `StrategyCtx` and `OrderCommand` | `strategy-runtime/src/strategy_host.rs`, `alor-protocol/src/lib.rs` | `BrokerAccountId` / `AccountId` | Treat legacy `portfolio` config as account-id alias at boundary. Runtime internals receive typed account id. | Legacy config key may remain as alias; serialized command v2 uses account id. | config alias test; command serde test. | Blocks live until all command/snapshot paths use typed account. | planned |
| `exchange: String` + `symbol: String` | `strategy_host.rs`, `config.rs`, runtime command builders | `InstrumentId` + `BrokerSymbol` | Convert strategy symbol plus broker venue symbol through registry. No strategy logic change. | Legacy state keeps string symbol fields; new envelope stores instrument identity. | instrument map fixture for IMOEXF paper. | Blocks live if broker venue identity is missing/ambiguous. | planned |
| `Intent::Cancel { order_id: i64 }` | `strategy_host.rs:33` | `CancelOrder { order_id: BrokerOrderId }` | Change cancel target to broker-neutral id. ALOR numeric ids become string values during legacy import. | Old `CancelSent` state migrates `order_id` to string. | cancel intent serde migration test. | Blocks cancel/repair live until migrated. | planned |
| `Intent::Replace { order_id: i64 }` | `strategy_host.rs:36` | future `ReplaceOrder { order_id: BrokerOrderId }` | Keep replace disabled. Migrate type only when replace gate opens. | Legacy replace states are unsupported for FINAM live. | forbidden surface/no-live test. | Replace remains out of scope. | deferred |
| `OrderEvent.order_id: i64` | `strategy_host.rs:257`, `alor-gateway/src/models.rs:42` | `BrokerOrderId(String)` | Mapper emits string broker id; ALOR numeric imported with decimal string. | Old snapshots keyed by integer migrate to string map keys. | order fixture migration: numeric id -> string id preserved. | Blocks order/trade reconciliation until complete. | planned |
| `TradeEvent.order_id: i64` | `strategy_host.rs:283`, `alor-gateway/src/models.rs:29` | `BrokerOrderId(String)` | Trade correlation uses broker-order string, never numeric surrogate. | Old ALOR trade order id migrates to string. | trade-to-order correlation test. | Blocks orphan trade handling until complete. | planned |
| `TradeEvent.trade_id: String` | `strategy_host.rs:283` | `BrokerTradeId(String)` | Wrap existing string. | Preserve raw broker-native trade id string. | broker trade fixture test. | Required for exact trade dedupe. | planned |
| `CommandAck.broker_order_id: Option<i64>` plus `broker_order_id_str` | `alor-protocol/src/lib.rs:127` | `CommandAck.broker_order_id: Option<BrokerOrderId>` | Remove numeric primary id. Keep old numeric field only in legacy deserializer if needed. | Old ACK with numeric id migrates to string; if both exist, string wins and conflict blocks. | ACK legacy decode test. | Blocks pending clear/reconciliation until complete. | planned |
| `HashMap<i64, OrderEvent>` order state | `state.rs:490`, `runtime.rs:77`, `strategy_host.rs:365` | `HashMap<BrokerOrderId, BrokerOrderSnapshot/OrderEvent>` | Change map key to broker order string. Keep runtime event wrapper broker-neutral. | Old JSON object keys parse as broker-order strings. | old state -> migrated state -> restored state. | Blocks live bootstrap until complete. | planned |
| `HashSet<i64> our_order_ids` | `runtime.rs:190` | `HashSet<BrokerOrderId>` | Track broker-native strings. | Old numeric ids convert to decimal strings in migration. | ACK/trade ownership test. | Required for orphan trade classification. | planned |
| `tracked_order_ids() -> Vec<i64>` | `strategy_host.rs:144` | `tracked_broker_order_ids() -> Vec<BrokerOrderId>` | Rename/replace trait method. Keep compatibility shim only inside migration layer. | Legacy strategies compile after source migration. | compile-only trait migration test. | Blocks strategy restore hooks until complete. | planned |
| `RuntimeStateRestored.known_order_ids: Vec<i64>` | `strategy_host.rs:371`, `runtime.rs:3141` | `Vec<BrokerOrderId>` | Preserve count semantics; expose typed ids to strategies. | Old state restores ids as strings. | runtime state restored fixture test. | Required for stale pending cleanup. | planned |
| `tp_order_id: Option<i64>` and `sl_exchange_order_id: Option<i64>` | `state.rs:273`, `state.rs:277`, hybrid runtime repair path | `Option<BrokerOrderId>` | Preserve ids but keep Stop/SLTP/bracket disabled. | Existing ids migrate to strings; missing remains missing. | protective placeholder migration fixture. | Stop/bracket remains blocked. | deferred |
| `sl_stop_order_id: Option<String>` | `state.rs:275` | `Option<BrokerOrderId>` or stop-order-specific id type | Wrap string id; do not enable stop order send. | Preserve exactly. | stop placeholder fixture. | Stop order live remains blocked. | deferred |
| string order status checks | `runtime.rs:50`, `runtime.rs:3189` | `BrokerOrderLifecycle` | Map broker status to active/terminal/unknown once, then use lifecycle. Unknown blocks readiness. | Old status strings accepted only through mapper fixtures. | active/terminal/unknown status table test. | Required before broker-truth bootstrap live. | planned |
| `positions: HashMap<String, PositionEvent>` | `state.rs:493`, `strategy_host.rs:364` | `BrokerPositionSnapshot` keyed by `InstrumentId` | Target-symbol position truth uses instrument identity; account-wide row count diagnostic only. | Legacy symbol-keyed positions migrate through instrument registry. | target flat/non-flat fixture. | Blocks live bootstrap if identity missing. | planned |
| default stream names by portfolio | `config.rs:2285` | broker-neutral channel roles | Keep semantic channel roles; make names configurable per contour. | Existing stream names accepted as ALOR oracle aliases. | stream-role mapping fixture/documentation. | Required for paper/mock runtime consumer groups. | planned |
| ALOR command DTO `OrderCommand` | `alor-protocol/src/lib.rs:66` | `BrokerCommand` | Convert `CommandAction` into broker-core commands behind paper/mock path first. | Legacy command JSON can be decoded by migration harness. | command action mapper tests. | Real send remains forbidden. | planned |
| `CreateStopLimit` / `DeleteStopLimit` action shapes | `strategy_host.rs:41`, `alor-protocol/src/lib.rs:52` | future protective-order contract | Preserve state markers; do not implement FINAM stop/bracket semantics in Stage 2. | Legacy stop fields are retained or classified `future_stop_bracket_only`. | no-live/feature-disabled tests. | Strictly blocked. | deferred |
| `RiskGateRuntimeState` f64 fields | `strategy_host.rs:189` and hybrid state fields | Decimal/string riskgate ledger state | Preserve business values; avoid float hops at broker boundary. | Legacy JSON reads current f64 shape; new snapshots can store Decimal strings later. | riskgate seed fixture with exact field preservation. | Blocks MR parity if missing. | planned |

## State schema migration plan

Stage 2B should introduce a versioned migration layer, but Stage 2A defines the
rules:

1. Deserialize legacy ALOR `StrategyState` / `RuntimeState` JSON.
2. Convert broker ids:
   - ALOR numeric `order_id` -> `BrokerOrderId(order_id.to_string())`;
   - existing string stop ids -> `BrokerOrderId(existing_string)`;
   - FINAM ids remain exact broker strings.
3. Preserve `StrategyRequestId` values exactly.
4. Preserve `ClientOrderId` separately from `StrategyRequestId`.
5. Preserve every `HybridIntradayRuntime` pending/deferred/safe/manual/riskgate
   field either as typed data or as explicit blocker marker.
6. Serialize new state with a schema version and broker-neutral id fields.
7. Re-open the serialized state and prove no pending/deferred/riskgate/manual
   data was lost.

Hybrid fields that must be preserved or explicitly blocked:

- `pending_entry_request_id`, `pending_entry_created_ts_utc`;
- `pending_exit_request_id`, `pending_exit_created_ts_utc`;
- `deferred_entry_*`, `deferred_exit_*`;
- `pending_tp_request_id`, `pending_sl_request_id`;
- `tp_order_id`, `sl_stop_order_id`, `sl_exchange_order_id`;
- `safe_mode_close_only`, `safe_mode_reason`;
- `manual_intervention_required`, `manual_intervention_reason` where present
  in paper/oracle projection;
- day feature fields;
- riskgate shadow/pending/summary fields;
- repair placeholders.

Policy classes:

| Class | Meaning |
| --- | --- |
| `preserve_exactly` | Field must survive old -> new -> restored unchanged. |
| `map_to_typed_field` | Field maps to a broker-neutral newtype or canonical enum. |
| `unsupported_blocks_live` | Field can be retained as marker in paper, but blocks runtime-live. |
| `future_stop_bracket_only` | Field is retained for future protective-order work; live remains disabled. |
| `future_repair_only` | Field is retained for future repair policy; live remains disabled. |
| `waived_for_imoexf_flat_clean_only` | Allowed only for flat-clean paper fixture, not for non-flat/pending states. |

## Request/client/broker id chain

Stage 2B implementation must preserve this chain:

```text
StrategyRequestId
  -> ClientOrderId
  -> BrokerOrderId(String)
```

Rules:

- `StrategyRequestId` is generated/owned by runtime.
- `ClientOrderId` is deterministic or durably allocated before send.
- `BrokerOrderId(String)` comes only from broker truth/ACK/reconciliation.
- Duplicate `StrategyRequestId` replays the prior outcome and must not send
  again.
- Duplicate `ClientOrderId` for another request blocks locally.
- Missing broker order id after accepted send becomes reconciliation-required,
  not blind cancel/retry.
- ACK with mismatched request id never clears pending strategy state.

## Redis stream compatibility plan

Stage 2 keeps the existing semantic channel roles, while making the payloads
broker-neutral:

| Role | Legacy ALOR shape | Stage 2 target shape | Rule |
| --- | --- | --- | --- |
| Market data | bar envelopes from ALOR stream | canonical final M10 `RuntimeBarInput` | Raw M1 never reaches strategy model input. |
| Broker snapshots | orders/trades/positions keyed by portfolio/symbol | `BrokerTruthSnapshot` + `RuntimeHostBootstrapSnapshot` | Broker truth before runtime state. |
| Commands | `OrderCommand` with `portfolio/exchange/symbol/action` | `BrokerCommand` with account/instrument/request/client ids | Paper/mock first. |
| ACKs | ALOR `CommandAck` with optional numeric id | broker-core `CommandAck` with string broker id | Exact request-id matching only. |
| Runtime state | legacy runtime state stream | versioned broker-neutral state | Old snapshots remain readable. |
| DLQ | ad hoc/stream-specific | redacted broker-neutral DLQ | XACK only after ACK/state publish or DLQ publish. |

Consumer-group requirements:

- use `XREADGROUP` for normal consumption;
- evidence must include `XPENDING`/idle recovery policy;
- use `XAUTOCLAIM` or equivalent for stale pending recovery;
- bound stream retention;
- no raw broker payloads, local paths, secrets, live account ids, or raw logs in
  source handoff.

## Compatibility test plan

Minimum Stage 2B test set, prepared by Stage 2A:

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

All tests must be paper/mock/local. No real FINAM send is allowed.

## Stage 2A acceptance

Stage 2A can be reviewed when:

- this plan is present;
- `docs/stage-2-runtime-source-migration-inventory.md` is present in the source
  archive;
- `reports/stage-2/runtime-source-migration-inventory.md` exists locally for
  operator/reviewer handoff outside the source archive;
- CI runs the forbidden-surface scan, negative harness, Rust checks, and
  no-Redis evidence smoke;
- safety docs still say runtime-live and strategy-driven real FINAM send are
  disabled.

Stage 2A does not by itself authorize Stage 2B. Stage 2B needs a separate
implementation plan and acceptance criteria for the allowed IMOEXF
`HybridIntradayRuntime` subset.
