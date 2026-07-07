# ALOR runtime compatibility contract v1

Status: draft/spec foundation; hard compatibility freeze pending Stage 1B
acceptance.

Purpose: define the runtime-facing semantic contract that FINAM must satisfy
before it can replace the ALOR gateway for existing strategy-runtime systems.
This is not a FINAM live-order enablement document.

## Boundary

Stage 1B hard-freeze scope is intentionally narrow:

- in scope: IMOEXF `HybridIntradayRuntime` paper/shadow parity;
- out of scope: USDRUBF `AlorUsdrubfHybrid`, RI Author41/42,
  `SessionGapStandalone`, generic `CancelSent`/`Done` migration,
  Stop/SLTP/bracket, runtime-live.

Allowed while implementing this contract:

- FINAM read-only and WebSocket market-data shadow;
- FINAM paper runtime;
- ALOR oracle reads for parity/seeding;
- paper/mock ACKs;
- local/CI fixtures and reports.

Still forbidden:

- continuous runtime-live;
- `command-consumer-to-real-FINAM`;
- strategy-driven real FINAM send;
- Stop/SLTP/bracket/replace/multi-leg;
- Runtime `LiveReady` for FINAM.

## Lifecycle order

FINAM runtime attachment must preserve the ALOR startup order:

1. `LoadBrokerTruthSnapshot`;
2. `LoadRuntimeState`;
3. `NotifyBootstrapSnapshot`;
4. `NotifyRuntimeStateRestored`;
5. `WarmupHistory`;
6. `RecoverPendingStreams`.

Acceptance:

- runtime state is never trusted before broker truth;
- warmup runs with live orders disabled;
- pending streams are recovered only after warmup;
- target-symbol active/unknown orders block live readiness unless explicitly
  adopted by strategy policy;
- account-wide rows are diagnostic only.

## Runtime-facing mappings

| Runtime-facing concept | Source of truth | FINAM/BrokerCore source | Required notes |
| --- | --- | --- | --- |
| Strategy symbol | Instrument registry | `InstrumentId` | Must include internal symbol, broker venue symbol, exchange, market. |
| Account/portfolio | Broker account map | `BrokerAccountId` | No implicit portfolio string assumptions. |
| Model bar | Market-data finality layer | canonical M10 `RuntimeBarInput` | Raw M1 must never become strategy model input. |
| Bar close time | Exchange/event time | `RuntimeBarInput.close_ts` | Must match ALOR closed-bar `close_time_utc` convention. |
| Position truth | Broker truth | target-symbol `BrokerPositionSnapshot` | Zero-qty rows are flat; account-wide row count is diagnostic. |
| Working order truth | Broker truth | active/unknown `BrokerOrderSnapshot` | Unknown or orphan orders block readiness. |
| Trade truth | Broker truth | `BrokerTradeSnapshot` | Must reconcile to request/client/broker order IDs. |
| Command request id | Runtime host | `StrategyRequestId` | Strategy pending clears only by exact request id. |
| Broker order id | Broker truth | `BrokerOrderId(String)` | String is source of truth; no lossy i64 unless durable surrogate policy is approved. |
| Client order id | FINAM order path | `ClientOrderId` | Must be durably mapped and collision-checked before send. |
| ACK | Runtime command lifecycle | `CommandAck` / broker-neutral ACK | Must preserve exact request-id parity. |
| Entry/exit class | Runtime host | `RuntimeIntentClass` | Entry gates must not silently block exit/cancel/repair. |
| Riskgate memory | Strategy/riskgate ledger | riskgate ledger/state | Must be real ledger integration or explicit oracle-seeded paper projection. |

## Event/schema field mapping

The following tables are normative for Stage 1B review. `Blocks readiness`
means a mismatch or missing value prevents runtime-driven live readiness. During
M4-3x seeded paper mode it blocks parity closure, not live trading, because live
trading remains forbidden.

### BarEvent / RuntimeBarInput

| Runtime field | Type | FINAM/BrokerCore field | ALOR source/oracle field | Conversion rule | Missing/zero policy | Blocks readiness |
| --- | --- | --- | --- | --- | --- | --- |
| `instrument.symbol` | string | `RuntimeBarInput.instrument.symbol` | ALOR bar symbol | Broker-neutral symbol registry. | Missing rejects bar. | yes |
| `instrument.venue_symbol` | string | `InstrumentId.venue_symbol` | ALOR broker symbol | Preserve broker-native venue identity separately. | Missing is diagnostic only if internal symbol is validated. | conditional |
| `open_ts` | UTC timestamp | `RuntimeBarInput.open_ts` | ALOR bar open time | UTC, monotonic per instrument/timeframe. | Missing rejects bar. | yes |
| `close_ts` | UTC timestamp | `RuntimeBarInput.close_ts` | ALOR closed-bar timestamp | Must match ALOR closed-bar convention. | Missing rejects bar. | yes |
| `timeframe_sec` | u32 | `RuntimeBarInput.timeframe_sec` | ALOR timeframe | Strategy input must be `600`. | Non-600 rejects strategy input. | yes |
| `open/high/low/close` | decimal | `RuntimeBarInput` OHLC | ALOR OHLC | Decimal string/Decimal, no lossy float parsing in mappers. | Missing rejects bar. | yes |
| `volume` | decimal | `RuntimeBarInput.volume` | ALOR volume | Decimal string/Decimal. | Missing blocks parity; live policy TBD. | conditional |
| `source_kind` | enum | `MarketDataSourceKind` | ALOR live/history provenance | Only fresh live final bars can advance live readiness. | Unknown blocks entry. | yes |
| `is_final` | bool | finality layer | ALOR closed bar | Raw M1/forming bars never reach strategy model input. | false rejects strategy input. | yes |
| `gap_status` | enum | recovery/data-quality ledger | ALOR reconnect/gap state | Gap must be closed by replay before entry. | Unknown blocks entry. | yes |
| `stale_backlog_flag` | bool | FINAM WS generation gate | ALOR backlog policy | Stale backlog is diagnostic/replay only. | true blocks strategy-live input. | yes |

### BrokerTruthSnapshot -> RuntimeHostBootstrapSnapshot

| Runtime field | Type | FINAM/BrokerCore field | ALOR source/oracle field | Conversion rule | Missing/zero policy | Blocks readiness |
| --- | --- | --- | --- | --- | --- | --- |
| `account_id` | string | `BrokerAccountId` | ALOR portfolio/account | Broker-neutral account map. | Missing blocks. | yes |
| `target_instrument` | `InstrumentId` | instrument registry | ALOR symbol/board | Must include internal + venue identity. | Missing blocks. | yes |
| `target_position_qty` | decimal | `BrokerPositionSnapshot.qty` | ALOR target position row | Target non-zero is non-flat truth. | Zero row = flat. | yes |
| `avg_price` | decimal? | position avg price | ALOR avg price | Diagnostic for adoption/PnL. | Missing blocks adoption, not flat startup. | conditional |
| `cash/free_cash` | decimal? | `BrokerCashSnapshot` | ALOR portfolio cash | Used for margin readiness. | Missing blocks entry. | yes |
| `active_target_orders` | list | active `BrokerOrderSnapshot` | ALOR active target orders | Active/unknown target orders require adoption or manual intervention. | Empty clean. | yes |
| `unknown_target_orders` | list | unknown order snapshots | ALOR unknown statuses | Unknown is blocker. | Empty clean. | yes |
| `orphan_target_trades` | list | trade/order mismatch | ALOR broker trades | Orphan requires reconciliation. | Empty clean. | yes |
| `account_wide_active_orders` | count/list | account-wide active orders | ALOR account active orders | Safety diagnostic; policy may block entry globally. | Empty clean. | conditional |
| `snapshot_ts` | UTC timestamp | received/source timestamp | ALOR snapshot timestamp | Must satisfy freshness SLA. | Stale blocks entry. | yes |
| `manual_intervention_required` | bool/reason | derived canonical truth | ALOR dirty-start evidence | True prevents live readiness. | Missing treated as true if dirty. | yes |

### OrderEvent / TradeEvent / CommandAck

| Runtime field | Type | FINAM/BrokerCore field | ALOR source/oracle field | Conversion rule | Missing/zero policy | Blocks readiness |
| --- | --- | --- | --- | --- | --- | --- |
| `request_id` | string | `StrategyRequestId` | ALOR runtime request id | Exact request id is the pending-state key. | Missing blocks pending clear. | yes |
| `client_order_id` | string | `ClientOrderId` | ALOR client correlation | Durable collision-checked local id. | Missing before send blocks live. | yes |
| `broker_order_id` | string | `BrokerOrderId(String)` | ALOR broker order id | String is authoritative. | Missing after accepted send = reconciliation required. | yes |
| `order_status` | enum | canonical status | ALOR order status | Map to active/terminal/unknown buckets. | Unknown blocks. | yes |
| `lifecycle_class` | enum | command/order path state | ALOR command lifecycle | `entry`, `exit`, `cancel`, `repair` separated. | Missing blocks policy decision. | yes |
| `filled_qty` | decimal | order/trade mapper | ALOR filled qty | Cumulative filled quantity. | Missing is incomplete truth. | conditional |
| `remaining_qty` | decimal | order mapper | ALOR remaining qty | Remaining >0 can be active even if status unclear. | Missing blocks active classification. | yes |
| `avg_price` | decimal? | order/trade mapper | ALOR avg/fill price | Diagnostic/PnL/adoption. | Missing allowed only before fill. | conditional |
| `trade_id` | string | `BrokerTradeSnapshot` | ALOR trade id | Broker-native trade identity. | Missing blocks trade reconciliation. | conditional |
| `trade_qty/trade_price` | decimal | trade mapper | ALOR trade fields | Decimal exactness. | Missing blocks trade truth. | yes |
| `commission/fee` | decimal? | broker fee if available | ALOR fee/commission | Diagnostic until fee parity stage. | Missing does not block Stage 1B. | no |
| `terminality` | enum | canonical order status | ALOR terminal status | Terminal only by explicit bucket. | Unknown blocks. | yes |
| `recoverability` | enum | order-path state machine | ALOR reconciliation policy | Timeout/ambiguous states never blind-retry. | Missing blocks live. | yes |

ACK statuses must include:

| ACK status | Meaning | Pending-state policy |
| --- | --- | --- |
| `Accepted` | Local/paper command accepted. | Does not imply broker fill. |
| `Confirmed` | Broker/order truth confirmed. | May clear matching pending state. |
| `Rejected` | Broker rejected. | Clears only matching pending with rejected outcome. |
| `Duplicate` | Same request already processed. | Replay prior outcome; no second send. |
| `LocalRejected` | Guard/preflight rejected. | Pending rollback only by explicit strategy policy. |
| `TimeoutUnknownPending` | Send or cancel ambiguity. | Manual/reconciliation required; no blind retry. |
| `RecoveredByClientOrderId` | Broker truth recovered via client id. | May bind broker id if unique. |
| `ManualInterventionRequired` | Unsafe/ambiguous truth. | Blocks live readiness. |
| `DLQ` | Invalid/unprocessable input. | XACK only after DLQ publish. |

ACK with mismatched `request_id` must never clear strategy pending state.

### RuntimeState / HybridIntradayRuntimeState

| Runtime field | Type | Source of truth | Missing/zero policy | Blocks parity/live |
| --- | --- | --- | --- | --- |
| `active_cycle_id` | string? | strategy runtime / oracle seed | `None` valid only when flat/no active cycle. | conditional |
| `next_cycle_seq` | u32 | strategy runtime | `0` after a non-empty ALOR state is suspicious. | yes |
| `last_position_qty` | decimal | broker truth + runtime state | `0` = flat. | yes |
| `current_owner/current_side` | string? | strategy runtime | Required when non-flat. | yes |
| `pending_entry_*` | strings? | strategy runtime | Required when pending entry exists. | yes |
| `pending_exit_request_id` | string? | strategy runtime | Required when pending exit exists. | yes |
| `deferred_entry/deferred_exit` | structured/string marker | strategy runtime | Stage 1B seed preserves markers; full execution policy is later. | yes |
| `tp_order_id/sl_stop_order_id` | string? | strategy runtime/broker truth | Future stop/bracket fields; unsupported for live. | yes for stop/bracket |
| `mr_take_price/mr_stop_price` | decimal? | MR strategy state | Required for active MR protective state. | conditional |
| `safe_mode_close_only/reason` | bool/string? | runtime safety state | true allows only exit/cancel/repair. | yes |
| day feature fields | decimals/dates | runtime/riskgate history | Missing blocks BO/MR parity. | yes |
| `overnight_exit_armed_date` | date? | runtime state | Required when armed. | conditional |
| riskgate fields | decimals/bools/count | riskgate ledger/state | Missing blocks MR parity. | yes |

### Riskgate ledger/state

| Runtime field | Type | FINAM/BrokerCore source | ALOR source/oracle field | Conversion rule | Missing policy | Blocks readiness |
| --- | --- | --- | --- | --- | --- | --- |
| `risk_gate_profile_id` | string | seed/config or ledger profile | ALOR riskgate profile/stream suffix | Must be supplied by seed/config, not hardcoded in `broker-core`. | Missing is `paper_oracle_seed_unknown_profile` and blocks hard freeze. | yes |
| `risk_gate_shadow_session_date` | date? | paper/riskgate state | ALOR shadow session date | Preserve date string. | Missing blocks MR parity. | yes |
| `risk_gate_shadow_pnl_points` | decimal | paper/riskgate state | ALOR shadow PnL | Decimal string/Decimal. | Default zero only when ALOR is zero/missing by waiver. | conditional |
| `risk_gate_shadow_trade_count` | u32 | paper/riskgate state | ALOR shadow trade count | Exact integer. | Missing blocks MR parity. | yes |
| `risk_gate_mr_enabled_current_session` | bool? | riskgate state | ALOR MR enabled current | Exact bool. | Missing blocks MR entry parity. | yes |
| `risk_gate_mr_enabled_next_session` | bool? | riskgate state | ALOR MR enabled next | Exact bool. | Missing blocks next-session parity. | conditional |
| `risk_gate_rolling_sum_lb120` | decimal? | riskgate state | ALOR rolling LB120 | Decimal string/Decimal, no float-hop in seed mapping. | Missing blocks MR parity. | yes |
| `risk_gate_last_finalized_session_date` | date? | riskgate state | ALOR finalized session | Preserve date string. | Missing blocks ledger continuity unless waived. | conditional |
| `risk_gate_ledger_rows_count` | usize | riskgate state | ALOR ledger rows count | Exact count for evidence. | Missing/zero when ALOR non-zero is unseeded/blocker. | yes |

Seeded M4-3x parity must classify each unsupported state:

| State shape | Seed bridge policy | Runtime-live policy |
| --- | --- | --- |
| flat clean | Supported. | Still blocked until later gates. |
| non-flat adopted | Supported as state projection. | Requires broker-truth adoption gate. |
| pending entry | Must preserve pending ids/owner/side. | Blocks live until command/ACK parity. |
| pending exit | Must preserve pending exit id. | Exit/repair policy required. |
| deferred exit | Marker preserved and fixture-backed; execution policy not enabled. | Blocks live until later policy gate. |
| safe-mode close-only | Must preserve flag/reason. | Entry blocked, exit/cancel/repair allowed by policy. |
| riskgate state | Seeded projection accepted as bridge. | Real ledger integration or explicit waiver required. |
| stop/bracket fields | Preserve IDs if present, but do not enable stop/bracket. | Stop/bracket remains forbidden. |

### Exact ALOR HybridIntradayRuntime field coverage

This table is the Stage 1B field coverage ledger for the IMOEXF
`HybridIntradayRuntime` shape. "Policy" is intentionally explicit: a field can be
preserved, mapped, or classified as unsupported/future and therefore blocking
for runtime-live.

| ALOR field/group | Seed field | Paper projection field | Stage 2 runtime source field | Policy | Blocks Stage 2B/live |
| --- | --- | --- | --- | --- | --- |
| `active_cycle_id` | `active_cycle_id` | `active_cycle_id` | same semantic field | preserve | conditional |
| `next_cycle_seq` | `next_cycle_seq` | `next_cycle_seq` | same semantic field | preserve | yes |
| `last_position_qty` | `last_position_qty` | `last_position_qty` | broker truth + runtime state | preserve/map | yes |
| `current_owner` | `current_owner` | `current_owner` | same semantic field | preserve | conditional |
| `current_side` | `current_side` | `current_side` | same semantic field | preserve | conditional |
| `pending_entry_owner` | `pending_entry_owner` | `pending_entry_owner` | same semantic field | preserve | yes |
| `pending_entry_side` | `pending_entry_side` | `pending_entry_side` | same semantic field | preserve | yes |
| `pending_entry_cycle_id` | `pending_entry_cycle_id` | `pending_entry_cycle_id` | same semantic field | preserve | yes |
| `pending_entry_request_id` | `pending_entry_request_id` | `pending_entry_request_id` | `StrategyRequestId` | preserve/map | yes |
| `pending_entry_created_ts_utc` | not yet represented | not yet represented | pending entry timestamp | unsupported_blocks_live | yes |
| `deferred_entry_owner` | `deferred_entry_state` marker only | `deferred_entry_state` marker only | structured deferred entry | unsupported_blocks_live until structured mapping | yes |
| `deferred_entry_side` | `deferred_entry_state` marker only | `deferred_entry_state` marker only | structured deferred entry | unsupported_blocks_live until structured mapping | yes |
| `deferred_entry_cycle_id` | `deferred_entry_state` marker only | `deferred_entry_state` marker only | structured deferred entry | unsupported_blocks_live until structured mapping | yes |
| `deferred_entry_entry_style` | `deferred_entry_state` marker only | `deferred_entry_state` marker only | structured deferred entry | unsupported_blocks_live until structured mapping | yes |
| `deferred_entry_reason` | `deferred_entry_state` marker only | `deferred_entry_state` marker only | structured deferred entry | unsupported_blocks_live until structured mapping | yes |
| `deferred_entry_stop_price` | `deferred_entry_state` marker only | `deferred_entry_state` marker only | structured deferred entry | future_stop_bracket_only | yes |
| `deferred_entry_take_price` | `deferred_entry_state` marker only | `deferred_entry_state` marker only | structured deferred entry | future_stop_bracket_only | yes |
| `deferred_entry_ts_utc` | `deferred_entry_state` marker only | `deferred_entry_state` marker only | structured deferred entry | unsupported_blocks_live until structured mapping | yes |
| `deferred_entry_request_id` | `deferred_entry_state` marker only | `deferred_entry_state` marker only | `StrategyRequestId` | unsupported_blocks_live until structured mapping | yes |
| `pending_exit_request_id` | `pending_exit_request_id` | `pending_exit_request_id` | `StrategyRequestId` | preserve/map | yes |
| `pending_exit_created_ts_utc` | not yet represented | not yet represented | pending exit timestamp | unsupported_blocks_live | yes |
| `deferred_exit_owner` | `deferred_exit_state` marker only | `deferred_exit_state` marker only | structured deferred exit | marker_preserved_full_mapping_pending | yes |
| `deferred_exit_reason` | `deferred_exit_state` marker only | `deferred_exit_state` marker only | structured deferred exit | marker_preserved_full_mapping_pending | yes |
| `deferred_exit_cycle_id` | `deferred_exit_state` marker only | `deferred_exit_state` marker only | structured deferred exit | marker_preserved_full_mapping_pending | yes |
| `deferred_exit_ts_utc` | `deferred_exit_state` marker only | `deferred_exit_state` marker only | structured deferred exit | marker_preserved_full_mapping_pending | yes |
| `deferred_exit_request_id` | `deferred_exit_state` marker only | `deferred_exit_state` marker only | `StrategyRequestId` | marker_preserved_full_mapping_pending | yes |
| `pending_tp_request_id` | not yet represented | not yet represented | future protective order state | future_stop_bracket_only | yes |
| `pending_tp_created_ts_utc` | not yet represented | not yet represented | future protective order state | future_stop_bracket_only | yes |
| `pending_sl_request_id` | not yet represented | not yet represented | future protective order state | future_stop_bracket_only | yes |
| `pending_sl_created_ts_utc` | not yet represented | not yet represented | future protective order state | future_stop_bracket_only | yes |
| `tp_order_id` | `tp_order_id` | `tp_order_id` | future protective order state | preserve_id_but_feature_disabled | yes for stop/bracket |
| `sl_stop_order_id` | `sl_stop_order_id` | `sl_stop_order_id` | future protective order state | preserve_id_but_feature_disabled | yes for stop/bracket |
| `sl_exchange_order_id` | not yet represented | not yet represented | future protective order state | future_stop_bracket_only | yes |
| `sl_triggered_ts` | not yet represented | not yet represented | future protective order state | future_stop_bracket_only | yes |
| `mr_take_price` | `mr_take_price` | `mr_take_price` | MR state | preserve | conditional |
| `mr_stop_price` | `mr_stop_price` | `mr_stop_price` | MR state | preserve | conditional |
| `safe_mode_close_only` | `safe_mode_close_only` | `safe_mode_close_only` | runtime safety state | preserve | yes |
| `safe_mode_reason` | `safe_mode_reason` | `safe_mode_reason` | runtime safety state | preserve | yes |
| `position_adoption_state` | `position_adoption_state` | `position_adoption_state` | broker truth adoption state | marker_preserved_full_mapping_pending | yes |
| `dirty_start_marker` | `dirty_start_marker` | `dirty_start_marker` | dirty-start audit state | marker_preserved_full_mapping_pending | yes |
| `manual_intervention_required` | `manual_intervention_required` | `manual_intervention_required` | runtime guard state | preserve | yes |
| `manual_intervention_reason` | `manual_intervention_reason` | `manual_intervention_reason` | runtime guard state | preserve | yes |
| `repair_deadline_ts` | not yet represented | not yet represented | future repair state | future_repair_only | yes |
| `next_repair_at_ts` | not yet represented | not yet represented | future repair state | future_repair_only | yes |
| `repair_backoff_level` | not yet represented | not yet represented | future repair state | future_repair_only | yes |
| `repair_attempts` | not yet represented | not yet represented | future repair state | future_repair_only | yes |
| day feature fields | day feature seed fields | day feature projection fields | runtime day state | preserve | yes |
| `overnight_exit_armed_date` | `overnight_exit_armed_date` | `overnight_exit_armed_date` | runtime overnight state | preserve | conditional |
| `risk_gate_shadow_entry_ts_utc` | not yet represented | not yet represented | riskgate shadow entry state | unsupported_blocks_live | yes |
| `risk_gate_shadow_entry_price` | not yet represented | not yet represented | riskgate shadow entry state | unsupported_blocks_live | yes |
| `risk_gate_shadow_side` | not yet represented | not yet represented | riskgate shadow entry state | unsupported_blocks_live | yes |
| `risk_gate_shadow_target_price` | not yet represented | not yet represented | riskgate shadow entry state | unsupported_blocks_live | yes |
| `risk_gate_shadow_stop_price` | not yet represented | not yet represented | riskgate shadow entry state | unsupported_blocks_live | yes |
| `risk_gate_pending_session_date` | not yet represented | not yet represented | riskgate pending state | unsupported_blocks_live | yes |
| `risk_gate_pending_shadow_pnl_points` | not yet represented | not yet represented | riskgate pending state | unsupported_blocks_live | yes |
| `risk_gate_pending_shadow_trade_count` | not yet represented | not yet represented | riskgate pending state | unsupported_blocks_live | yes |
| riskgate summary fields | riskgate seed fields | riskgate projection fields | riskgate ledger/state | preserve as bridge | yes |

Stage 2B implementation cannot start until every `unsupported_blocks_live`
or `marker_preserved_full_mapping_pending` field has either a structured mapping,
an accepted waiver, or is explicitly proven out of scope for the target runtime
deployment.

### Redis envelope / DLQ record

| Field | Type | Rule |
| --- | --- | --- |
| `schema_version` | integer | Must be present and checked. |
| `ts_utc` | timestamp | UTC event envelope time. |
| `source` | string | Stable service/source id, no secrets. |
| `msg_type` | enum | Must match payload variant. |
| `payload` | object | Broker-neutral payload only. |
| DLQ `reason` | enum/string | Redacted and deterministic. |
| DLQ `payload_sha256` | string | Raw payload hash allowed; raw payload export forbidden. |
| DLQ `raw_payload_exported` | bool | Must be false by default. |

## Redis stream and consumer-group mapping

The Stage 1B contract uses the stream names below as the current IMOEXF paper
parity shape. Real deployment may override names in ignored local config, but
the semantic roles must remain stable.

| Role | FINAM paper stream | ALOR oracle stream | Consumer group / policy |
| --- | --- | --- | --- |
| FINAM WS market data | `finam_imoexf_paper:ws:market_data` | ALOR live bar stream | `finam-imoexf-paper-runtime-m1`; XACK after successful publish or DLQ. |
| Canonical M10 bars | `finam_imoexf_paper:md:bars:10m` | ALOR native/assembled M10 oracle | No raw M1 strategy input. |
| Runtime state | `finam_imoexf_paper:runtime:state:hybrid_intraday:imoexf` | `runtime.state.hybrid_intraday.live.riskgate_shadow.imoexf.<PORTFOLIO_ID>` | Latest state compared field-by-field. |
| Paper intents | `finam_imoexf_paper:runtime:intents` | `cmd.orders.<PORTFOLIO_ID>` | Paper/mock only in Stage 1B. |
| Paper ACKs | `finam_imoexf_paper:runtime:paper_acks` | `cmd.acks.<PORTFOLIO_ID>` | Exact request-id parity required. |
| Paper orders | `finam_imoexf_paper:runtime:orders_paper_only` | ALOR broker order snapshots | Paper-only, no endpoint send. |
| Paper trades | `finam_imoexf_paper:runtime:trades_paper_only` | ALOR broker trades | Paper-only. |
| Paper positions | `finam_imoexf_paper:runtime:positions_paper_only` | ALOR broker snapshots/positions | Target-symbol truth only. |
| Publish batches | `finam_imoexf_paper:runtime:publish_batches` | n/a | Idempotent batch markers. |
| Runtime DLQ | `finam_imoexf_paper:runtime:dlq` | n/a | Redacted DLQ, XACK after DLQ publish. |
| Riskgate ledger | `finam_imoexf_paper:runtime:riskgate:sessions:*` | ALOR riskgate session stream | Seed bridge now; real ledger integration later. |

Operational policies:

- use `XREADGROUP` for normal consumption;
- use `XAUTOCLAIM` for stale pending recovery;
- `XPENDING` count must be part of evidence;
- stream retention must be bounded with `MAXLEN`/configured limits;
- no raw payloads, secrets, live account ids, or local paths in source handoff.

## Market data contract

Strategy input is closed M10 bars.

Required:

- FINAM WS `BARS` M1 final bars are the online source;
- canonical M10 is built only from complete, final M1 buckets;
- stale backlog bars are not published as strategy live bars;
- reconnect/gap recovery must complete before accepting fresh live strategy
  bars;
- first fresh final live bar after restart is required before readiness can
  progress beyond market-data degraded states;
- weekend/non-tradable bars must not become strategy trading anchors.

Acceptance:

- FINAM canonical M10 and ALOR oracle M10 have matching close timestamps and
  OHLCV for the active session, or each divergence is classified;
- raw M1 is visible only for diagnostics/aggregation;
- gap after silence blocks entry but preserves exit/cancel/repair allowance.

## Broker-truth bootstrap contract

`BrokerTruthSnapshot` must be converted to `RuntimeHostBootstrapSnapshot` before
the strategy is trusted.

Required target-symbol rules:

- target non-zero qty is non-flat truth;
- target zero qty is flat even if a broker keeps a zero row;
- target active order is an adoption/manual-intervention candidate;
- target unknown status is a blocker;
- account-wide active orders are safety diagnostics, not target position truth.

Dirty-start policy:

- target flat + no active/unknown target orders: startup can continue;
- target non-flat + strategy can adopt: adopt with explicit state/audit;
- target non-flat + strategy cannot adopt: `manual_intervention_required`;
- orphan/unknown order/trade: readiness blocked until reconciled.

## Runtime state restore contract

Existing ALOR strategy state fields must keep their meaning:

- `active_cycle_id`;
- `next_cycle_seq`;
- `last_position_qty`;
- `current_owner`;
- `current_side`;
- pending entry/exit request ids;
- deferred entry/exit state;
- TP/SL ids when that future capability is enabled;
- day features;
- riskgate session/ledger summary.

Paper projection may be used only as a parity bridge. It must not be mistaken
for real strategy semantics until the real hybrid BO/MR orchestrator is attached.

## Request-id / client-order-id / broker-order-id chain

Before any strategy-driven real send:

```text
StrategyRequestId
  <-> ClientOrderId
  <-> BrokerOrderId(String)
```

must be durable and crash-safe.

Acceptance:

- duplicate `StrategyRequestId` does not create a second broker order;
- `ClientOrderId` collision blocks locally and emits manual-intervention
  diagnostics;
- ACK with mismatched request id never clears pending strategy state;
- broker order id string remains the authoritative broker identifier;
- restart restores the full mapping.

## Runtime command consumer contract

The next command-consumer stage must be paper/mock first.

Required flow:

```text
runtime command stream
  -> broker-neutral command
  -> live guard / instrument / account / risk preflight
  -> paper/mock ACK
  -> runtime-compatible ACK stream
```

Acceptance:

- runtime emits an intent;
- adapter receives it with exact request id;
- blocked entry rolls back or keeps state only by explicit strategy policy;
- exit/cancel/protective repair is classified separately from entry;
- XACK happens only after successful ACK/state publication or DLQ.

## Real FINAM command consumer gate

Not allowed until the paper/mock command consumer and broker-truth bootstrap are
accepted.

When eventually allowed, first scope is only:

- `MARKET`;
- `LIMIT`;
- `CANCEL`.

Still excluded from first runtime-live:

- stop;
- stop-limit;
- SLTP;
- bracket;
- replace;
- multi-leg.

## Observability contract

Every long-running FINAM runtime service must publish:

- health phase/reason;
- readiness phase/reason;
- market-data freshness;
- broker-truth freshness;
- order/trade stream freshness or polling SLA;
- token expiry/refresh diagnostics;
- rate-limit state;
- operator arm state;
- exact LiveReady blockers.

Daemon paths must not panic on recoverable failures. Internal failures must
degrade readiness, revoke operator live arm if applicable, and emit audit events.

## Acceptance for v1

Stage 1A is accepted as a spec foundation. Stage 1B hard compatibility freeze is
accepted only when:

- this field-level mapping is reviewed against ALOR runtime sources;
- at least one fixture maps ALOR runtime state into broker-core/paper runtime
  state without losing day/riskgate/cycle fields;
- FINAM paper state and ALOR runtime state can be compared field-by-field for
  IMOEXF hybrid;
- all divergences are classified as expected, implementation gap, or blocker.

Minimum fixture set:

```text
tests/fixtures/alor_runtime_compat/
  hybrid_flat_clean_runtime_state.json
  hybrid_nonflat_runtime_state.json
  hybrid_pending_entry_runtime_state.json
  hybrid_pending_exit_runtime_state.json
  hybrid_deferred_exit_runtime_state.json
  hybrid_safe_mode_runtime_state.json
  hybrid_riskgate_state.json
  expected_paper_oracle_seed_flat_clean.json
```

Minimum automated checks:

- ALOR runtime-state fixture -> `PaperHybridIntradayOracleSeed`;
- seed -> `PaperLedgerSnapshot`;
- seed + FINAM canonical M10 -> `PaperRuntimeState`;
- selected `PaperRuntimeState.hybrid_intraday` fields match ALOR oracle fields;
- pending/safe-mode/riskgate fields are preserved or explicitly classified as
  unsupported blockers;
- JSON decimal values are parsed without an `as_f64()` round trip.

Minimum evidence report fields:

- `source_commit`;
- `vps_host`;
- FINAM WS source stream;
- FINAM runtime-state stream;
- ALOR runtime-state stream;
- compared bar key/timestamp;
- OHLCV diagnostic deltas where available;
- DLQ count;
- consumer group `XPENDING` summary;
- consumer group `lag` from `XINFO GROUPS` where Redis provides it;
- safety flags;
- divergence classification;
- expected/waived/blocker divergence counts;
- final status from:
  `Synchronized`, `ExpectedDivergenceOnly`, `BlockedDivergence`, `Unseeded`,
  `SafetyBoundaryOpen`, `EvidenceIncomplete`.
