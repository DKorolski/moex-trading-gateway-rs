# M4-2e ALOR parity closure matrix / canonical live gate hardening

Status: no-live parity-hardening package.

M4-2e follows the M4-2d review. It does not authorize new live-position tests.
It converts the remaining ALOR parity observations into executable canonical
checks where practical, and into explicit P0 waivers where the project still
needs a later implementation package.

## Trading boundary

This stage must not:

- send live orders;
- open or close a position;
- enable runtime live;
- connect command-consumer to real FINAM;
- enable Stop/SLTP/bracket/replace/multi-leg;
- authorize further live position tests.

Live expansion remains blocked after M4-2e.

## What M4-2e hardens

### 1. Instrument identity

`BrokerInstrumentSpec` now carries broker identity fields beyond
`InstrumentId`:

- `broker_asset_id`;
- `board`;
- `expiration_date` from the embedded `InstrumentMapEntry`;
- broker symbol / MIC / exchange / market / schedule id.

New canonical helper:

```rust
instrument_spec_identity_matches(left, right)
```

M4-2e tests prove that all of these are identity-significant:

- same ticker + same MIC + different expiry => not same instrument;
- same ticker + same MIC + different asset_id => not same instrument;
- same ticker + different board => not same instrument.

This keeps `InstrumentId` useful for target order/position scoping, while
requiring `BrokerInstrumentSpec` for contract-roll and broker-native asset
identity.

### 2. Canonical runtime live gate

`BrokerReadinessSnapshot` now includes the ALOR `LiveReady`-style blockers that
were missing from M4-2d:

- `live_market_data_seen`;
- `subscription_ready`;
- `stream_or_polling_connected`;
- `event_sink_degraded`;
- `stop_order_readiness`.

New readiness enum:

```rust
BrokerStopOrderReadiness::{SupportedFresh, UnsupportedBlocked, Stale, Missing}
```

`live_entry_allowed(...)` now blocks when:

- first live/polled market data has not been seen;
- subscription or polling equivalent is not ready;
- stream/polling path is disconnected;
- event sink is degraded;
- stop-order readiness is missing/stale/unsupported-blocked.

For FINAM M4-2e, Stop/SLTP/bracket remains disabled, so FINAM readiness maps
stop orders as:

```rust
BrokerStopOrderReadiness::UnsupportedBlocked
```

That is intentional: it is a visible waiver and a live-entry blocker, not a
silent pass.

### 3. M4-1c canonical report executable test

`broker-cli` now has an executable golden test for the M4-1c report shape. It
asserts:

- `truth_source = BrokerTruthSnapshot`;
- `positions_count` equals canonical target open position count;
- `account_positions_count` equals canonical account open position count;
- `active_orders_count` equals canonical account active order count;
- `unknown_active_orders_count` equals canonical account unknown order count;
- `target_position_qty = 0`;
- `target_is_flat = true`;
- `final_truth_source = BrokerTruthSnapshot`.

This closes the M4-2d review note that the path was compiled but not asserted by
an executable test.

## Complete ALOR config inventory

Direct ALOR source:

```text
alor_project/bybit_barter_test_sanitized/alor-rs-main/alor-gateway/src/config.rs
```

The `AlorGatewayConfig` surface has 62 fields. M4-2e inventories all of them:

| Group | ALOR fields | Canonical/FINAM parity status |
| --- | --- | --- |
| Scope / identity | `portfolio`, `exchange`, `instrument_group`, `symbols` | Partially mapped to `BrokerScopeConfig`, `BrokerInstrumentSpec`; asset_id/board/expiry identity hardening added in M4-2e |
| Timeframe / history | `tf_sec`, `from_ts`, `skip_history_bars`, `skip_history_positions`, `skip_history_orders`, `history_sessions`, `history_days_back`, `session_rollover_hour_utc`, `cold_start_history_days_back` | Partial; read-only history exists, runtime replay/first-live-bar binding remains blocked |
| Endpoints / auth | `ws_url`, `cws_url`, `oauth_url`, `refresh_token` | FINAM REST/auth exists; CWS/action-scoped equivalent remains one-shot only |
| Data formatting / polling | `split_adjust`, `format`, `frequency_ms` | Partial; FINAM mapper covers typed DTOs, runtime policy still blocked |
| Reconnect / backoff | `backoff_initial_ms`, `backoff_max_ms`, `backoff_multiplier`, `warm_reconnect_max_gap_sec`, `gap_backfill_padding_bars`, `bar_silence_resync_min_sec` | Partial; canonical live gate now has stream/polling blockers, reconnect implementation still P1/P0 for runtime |
| Silence / schedule | `max_silence_bars_sec`, `trading_periods` | Partial; FINAM schedule maps to `BrokerMarketSessionState`, full session policy remains blocked |
| Health / reporting | `health_listen_addr`, `data_report_path`, `bar_dump_path` | Diagnostic parity only; not a live enabler |
| Instrument steps | `price_step`, `volume_step` | Mapped through `BrokerInstrumentSpec` and order preflight; asset_id/board/expiry hardening added |
| Position/cash/order logging | `log_positions_filter`, `log_cash_positions`, `cash_symbols`, `log_existing_snapshot_orders` | Diagnostic parity; canonical cash and order truth exist, economics still partial |
| WS heartbeat / subscribe ACK | `ws_idle_timeout_sec`, `ws_ping_interval_sec`, `ws_ping_timeout_sec`, `subscribe_ack_timeout_ms`, `subscribe_ack_timeout_positions_ms`, `subscribe_ack_retries` | Canonical live gate now models subscription/polling/stream readiness; full FINAM stream implementation blocked |
| Control path stale/recycle | `control_path_stale_after_sec`, `control_path_pre_entry_recycle_enabled`, `control_path_pre_exit_recycle_enabled`, `control_path_recycle_timeout_ms`, `control_path_recycle_timeout_ms_exit`, `control_path_post_recycle_exit_send_window_ms`, `control_path_hardening_log_only`, `control_cws_mode` | One-shot order path has durable gates; continuous runtime control path parity not closed |
| Action-scope enable flags | `action_scope_enable_create_limit`, `action_scope_enable_market`, `action_scope_enable_delete_limit`, `action_scope_enable_replace_limit`, `action_scope_enable_exit` | Broker capability/scope model exists; runtime live remains disabled |
| Action-scope timeouts | `action_scope_open_timeout_ms`, `action_scope_authorize_timeout_ms`, `action_scope_force_token_refresh_before_authorize`, `action_scope_followup_window_ms`, `action_scope_max_session_lifetime_ms`, `action_scope_close_timeout_ms` | One-shot operator gate exists; continuous action-scope session parity not closed |

The full machine-checked list is:

```text
portfolio
exchange
instrument_group
symbols
tf_sec
from_ts
ws_url
cws_url
oauth_url
refresh_token
skip_history_bars
skip_history_positions
skip_history_orders
split_adjust
format
frequency_ms
backoff_initial_ms
backoff_max_ms
backoff_multiplier
max_silence_bars_sec
trading_periods
history_sessions
history_days_back
session_rollover_hour_utc
health_listen_addr
price_step
volume_step
log_positions_filter
log_cash_positions
cash_symbols
log_existing_snapshot_orders
ws_idle_timeout_sec
ws_ping_interval_sec
ws_ping_timeout_sec
subscribe_ack_timeout_ms
subscribe_ack_timeout_positions_ms
subscribe_ack_retries
warm_reconnect_max_gap_sec
gap_backfill_padding_bars
cold_start_history_days_back
bar_silence_resync_min_sec
control_path_stale_after_sec
control_path_pre_entry_recycle_enabled
control_path_pre_exit_recycle_enabled
control_path_recycle_timeout_ms
control_path_recycle_timeout_ms_exit
control_path_post_recycle_exit_send_window_ms
control_path_hardening_log_only
control_cws_mode
action_scope_enable_create_limit
action_scope_enable_market
action_scope_enable_delete_limit
action_scope_enable_replace_limit
action_scope_enable_exit
action_scope_open_timeout_ms
action_scope_authorize_timeout_ms
action_scope_force_token_refresh_before_authorize
action_scope_followup_window_ms
action_scope_max_session_lifetime_ms
action_scope_close_timeout_ms
data_report_path
bar_dump_path
```

## ALOR ↔ FINAM closure matrix

| Area | M4-2e status | Live implication |
| --- | --- | --- |
| Position truth | Closed for target flat: target instrument + non-zero qty | Not sufficient alone for live |
| Order truth | Closed for lifecycle + remaining qty + unknown status blocking | Must remain canonical source |
| Instrument identity | Hardened with board/expiry/asset_id tests | Required before roll/contract expansion |
| Trades/fills | FINAM trades map to canonical snapshots; net-flat test exists | Economics still partial |
| Cash/margin | Canonical cash/free cash exists | Required-margin calculation remains waiver/P0 |
| Readiness freshness | Account/positions/orders/trades/quotes/spec/schedule freshness exists | Must be fed by real read-only artifacts before live |
| First live bar / market data | New gate field `live_market_data_seen` | Blocks live until true |
| Subscription / polling | New gate fields `subscription_ready`, `stream_or_polling_connected` | Blocks live until true |
| Event sink degradation | New gate field `event_sink_degraded` | Blocks live if degraded |
| Stop orders | New `BrokerStopOrderReadiness`; FINAM maps unsupported as blocked | Blocks live while Stop/SLTP disabled |
| Runtime command consumer | Still disabled | No continuous live |
| Action-scope control path | One-shot only | Runtime parity not closed |
| Replace/SLTP/bracket/multi-leg | Still blocked | No expansion |

## Remaining P0 after M4-2e

1. Required margin calculation from instrument + side + qty + price + account
   risk params.
2. Real read-only artifact package proving canonical readiness values immediately
   before any future live test.
3. Runtime command consumer integration remains disabled.
4. Stop/SLTP/bracket/replace/multi-leg remain blocked.
5. Continuous runtime live parity with ALOR action-scope CWS/control path is not
   closed.

## Acceptance

M4-2e is ready for review when:

- all 62 ALOR config fields are inventoried;
- instrument identity tests cover board, expiry, and asset_id;
- canonical live gate tests cover first-live-bar, subscription/polling,
  event-sink degraded, and stop-order unsupported-blocked;
- M4-1c canonical report golden test passes;
- M4-2e evidence reports no live calls and live expansion blocked;
- forbidden-surface scanners remain green.
