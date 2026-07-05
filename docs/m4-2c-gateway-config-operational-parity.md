# M4-2c Gateway Config & Operational Parity Specification

Status: specification and canonical model hardening only. No live calls.

M4-2c is the operational parity checkpoint after M4-1c/M4-2b-1. M4-1c
proved that a FINAM broker account can keep a position row after a flat
round-trip; therefore `row presence != open position`. M4-2b-1 moved FINAM
read-only snapshots toward `BrokerTruthSnapshot`. M4-2c widens the comparison:
the parity target is not only mapper shape, but the full gateway decision model
that decides whether a boundary call is safe.

## Trading boundary

This stage must not:

- send live orders;
- open or close a position;
- enable runtime live;
- connect command-consumer to real FINAM;
- enable Stop/SLTP/bracket/replace/multi-leg;
- enable continuous trading.

Further live-position tests remain blocked until the P0 gaps below are closed
or explicitly waived in a later reviewed package.

## Canonical broker-neutral structures

M4-2c adds or confirms the following broker-neutral contracts in `broker-core`:

- `BrokerTruthSnapshot`;
- `BrokerOrderSnapshot`;
- `BrokerPositionSnapshot`;
- `BrokerCashSnapshot`;
- `BrokerInstrumentSpec`;
- `BrokerTradeSnapshot`;
- `BrokerReadinessSnapshot`;
- `BrokerOperationalConfig`;
- `BrokerCapabilityMatrix`.

Required derived methods are now represented in broker-core as canonical helpers:

- `target_position_qty(instrument)`;
- `target_is_flat(instrument)`;
- `target_active_orders(instrument)`;
- `account_active_orders()`;
- `unknown_orders()`;
- `cash_by_currency(currency)`;
- `margin_sufficiency_for_order(required_margin)`;
- `broker_truth_is_fresh(now)`;
- `live_entry_allowed(now, config, capabilities, scope)`.

The `margin_sufficiency_for_order` helper currently receives the already
computed required margin. The instrument/side/qty/price-to-margin calculation is
a P0 follow-up because it depends on validated instrument specs and broker risk
rules.

## Config parity table

| ALOR config key / behavior | FINAM config key / behavior | Canonical key | Default or current state | Live safety impact | Gap status | Required action |
| --- | --- | --- | --- | --- | --- | --- |
| `ALOR_WS_IDLE_TIMEOUT_SEC` | `FinamConfig.websocket_endpoint`; no canonical heartbeat freshness yet | `BrokerTimeoutConfig.stream_heartbeat_timeout_ms` | ALOR explicit; FINAM implicit/read-only | stale stream must block live | P0 | bind FINAM quote/order streams or polling freshness into readiness |
| `ALOR_WS_PING_INTERVAL_SEC` | no FINAM gateway canonical ping interval | `BrokerTimeoutConfig.stream_heartbeat_timeout_ms` | ALOR explicit; FINAM absent | missing heartbeat policy can hide feed loss | P1 | define FINAM stream heartbeat policy before continuous runtime |
| `ALOR_WS_PING_TIMEOUT_SEC` | no FINAM gateway canonical ping timeout | `BrokerTimeoutConfig.stream_heartbeat_timeout_ms` | ALOR explicit; FINAM absent | stale/degraded stream detection | P1 | map FINAM stream/poll timeouts to canonical timeout config |
| `ALOR_SUBSCRIBE_ACK_TIMEOUT_MS` | read-only FINAM REST request timeout | `BrokerTimeoutConfig.request_timeout_ms` | FINAM default 10s | slow broker truth must not be treated as fresh | P0 | use bounded request timeouts for all broker truth refreshes |
| `ALOR_SUBSCRIBE_ACK_RETRIES` | FINAM one-shot/order path currently uses no blind retry after ambiguous send | `BrokerLifecycleConfig.blind_retry_after_ambiguous_send_allowed=false` | false for safe live boundary | prevents duplicate orders after ambiguous send | Closed for one-shot; P0 for runtime | keep blind retry forbidden; require reconciliation before retry |
| `ALOR_CONTROL_PATH_STALE_AFTER_SEC` | `CancelBrokerTruthFreshnessPolicy` | `BrokerFreshnessConfig.orders_max_age_ms`, `trades_max_age_ms`, `positions_max_age_ms` | FINAM cancel freshness policy exists | stale cancel truth can cause blind cancel/false flat | P0 | derive all live preflight freshness from canonical config |
| `ALOR_ACTION_SCOPE_ENABLE_MARKET` | `GatewayFeatureSet.order_placement_enabled`; `OrderPreflightPolicy.allowed_order_types` | `BrokerCapabilityMatrix.supports_market_order`; `BrokerScopeConfig.allowed_order_types` | live disabled by default | market entry must be explicitly scoped | Closed for gate; P0 for runtime | keep disabled until canonical preflight consumes readiness |
| `ALOR_ACTION_SCOPE_CREATE_LIMIT` | `GatewayFeatureSet.order_placement_enabled`; `OrderPreflightPolicy.allowed_order_types` | `BrokerCapabilityMatrix.supports_limit_order`; `BrokerScopeConfig.allowed_order_types` | one-shot limit path reviewed; runtime disabled | limit entry/cancel blast radius | Closed for one-shot; P0 for runtime | runtime must use broker-neutral scope + capability matrix |
| `ALOR_ACTION_SCOPE_DELETE_LIMIT` | `GatewayFeatureSet.cancel_enabled`; `CancelPreflightApproval` | `BrokerCapabilityMatrix.supports_cancel` | cancel path used only in reviewed one-shot | emergency cancel must remain possible when entry blocked | P0 | split entry readiness from emergency cancel readiness |
| `ALOR_ACTION_SCOPE_REPLACE_LIMIT` | not enabled | `BrokerCapabilityMatrix.supports_replace` | false | replace is new live risk surface | P1 | block until explicit replace lifecycle design |
| `ALOR_ACTION_SCOPE_EXIT` | manual/one-shot market exit only; runtime disabled | `BrokerOrderIntentKind::Exit` | limited M4-1c evidence only | exit policy must be different from entry policy | P0 | define close-only policy using target position truth |
| ALOR gateway broker capability flags are implicit in action scopes | `FinamApiCapabilities` + `GatewayFeatureSet` | `BrokerCapabilityMatrix` | FINAM API surface discovered, live flags disabled | unsupported broker feature must not be assumed live-capable | P0 | derive capability matrix from API characterization + gateway feature gates |
| ALOR broker truth control policy is distributed across health/control path config | `BrokerTruthGatewayConfig` | `BrokerOperationalConfig` + `BrokerReadinessSnapshot` | FINAM currently has cancel reconciliation sub-policy | broker truth orchestration must be visible before live entry | P0 | promote FINAM broker-truth policy into canonical readiness/preflight inputs |
| ALOR portfolio/exchange/instrument group/symbols | FINAM account id + symbol/MIC/asset id | `BrokerScopeConfig.allowed_accounts`, `allowed_symbols`; `BrokerInstrumentSpec` | account/symbol scoped in preflight; full instrument spec pending | wrong board/MIC/expiry can trade wrong instrument | P0 | require validated `BrokerInstrumentSpec` before live expansion |
| ALOR price/volume steps | FINAM asset params min step/lot size | `BrokerInstrumentSpec` + `OrderPreflightPolicy.price_step`, `qty_step` | local preflight has steps; mapper incomplete | invalid orders or wrong notional/margin | P0 | map FINAM asset params into canonical instrument spec |
| ALOR trading periods / scheduler state | FINAM schedule sessions | `BrokerFreshnessConfig.schedule_max_age_ms`; `BrokerMarketSessionState` | schedule mapper incomplete | market closed must block entry | P0 | map FINAM schedule into canonical readiness |
| ALOR `max_silence_bars_sec` | FINAM quote freshness not canonical | `BrokerFreshnessConfig.quotes_max_age_ms` | pending | stale quote can approve wrong limit/market preflight | P0 | bind quote/bar freshness to `BrokerReadinessSnapshot` |
| ALOR command TTL | `OrderPreflightPolicy.max_reference_age_ms`; command TTL validation | `BrokerTimeoutConfig.order_submit_timeout_ms` and lifecycle policy | partially present | stale commands must be rejected | P0 | ensure all runtime commands pass canonical TTL/freshness gate |
| ALOR reconnect/gap sync phases | FINAM readiness streams + broker truth refresh | `BrokerReadinessSnapshot`; `BrokerFeedFreshness` | broker-core model added | no live during sync/gap/degraded | P0 | feed FINAM gateway state into canonical readiness |
| ALOR event sink degraded gate | FINAM gateway feature flags/health | `BrokerReadinessSnapshot` + future degraded flag | pending | no live if broker truth cannot be published/audited | P1 | add degraded sink/readiness block before continuous runtime |
| ALOR persisted command/control path state | SQLite order path store, one-shot marker, idempotency marker | `BrokerLifecycleConfig` | one-shot persistence present; runtime not attached | crash recovery / duplicate prevention | P0 | require persistence config before runtime live |

## Operational parity matrix

| Area | ALOR oracle behavior | FINAM current state | Canonical rule | Gap status |
| --- | --- | --- | --- | --- |
| Orders | active/terminal/unknown status is explicit; active command paths depend on readiness | FINAM orders map to `BrokerOrderSnapshot` with lifecycle + remaining qty in M4-2b-1 | working order = target instrument + active lifecycle + blocking remaining quantity | Partially closed; P0 tests remain for missing remaining qty and unknown status readiness |
| Positions | position truth is instrument-keyed and non-zero qty aware | M4-2b-1 maps FINAM position rows and ignores zero target qty for flat | target flat = target instrument + net non-zero qty equals zero | Closed at broker-core level; needs M4 evidence migration |
| Cash/margin | account-wide cash/margin are safety preconditions | FINAM cash/equity/free cash fields map partially | missing cash/margin blocks actual pre-authorization | P0 until margin sufficiency uses validated instrument spec |
| Instruments | symbol identity includes exchange/board/class/contract details where available | FINAM asset/asset params/schedule mapper incomplete | same ticker on different MIC/board/expiry is not the same instrument | P0 |
| Trades/fills | fills reconcile orders and position deltas | FINAM trades are not yet included in aggregate `BrokerTruthSnapshot` | trade/fill evidence must explain position/economics delta | P0 |
| Readiness | `SyncingHistory`, `Reconnecting`, `SyncingGap` block entry; `LiveReady` required | FINAM has health/readiness surfaces but canonical binding is incomplete | stale account/positions/orders/trades/quotes/instrument/schedule blocks live entry | P0 |
| Lifecycle/storage | command and control paths require idempotency and persistence | one-shot path has markers; continuous consumer disabled | live entry requires begin-submit persistence, idempotency marker, one-shot/run marker, crash recovery state | P0 |
| Retries/errors | ambiguous transport state becomes reconciliation/manual intervention, not blind retry | M3j one-shot correctly produced reconciliation-required behavior | blind retry after ambiguous send is forbidden without broker truth reconciliation | Closed for one-shot; P0 for runtime |
| Evidence/redaction | operational evidence avoids leaking secrets and binds to source artifacts | evidence scripts exist; M4-2c script adds config parity checks | review package must prove no live calls and preserve boundary flags | Closed for M4-2c evidence if script passes |

## P0/P1/P2 gap list

### P0 — blocks further live-position tests

1. FINAM trades are not yet mapped into `BrokerTradeSnapshot` inside aggregate
   `BrokerTruthSnapshot`.
2. FINAM asset/asset params/schedule are not yet mapped into
   `BrokerInstrumentSpec` and `BrokerReadinessSnapshot`.
3. Same ticker on different MIC/board/expiry requires canonical identity tests.
4. Cash/margin sufficiency is present only as free-cash comparison against a
   supplied required margin; required margin calculation from instrument + side
   + qty + price is still pending.
5. Canonical readiness/freshness is not yet the single preflight source for
   M4/M5 evidence.
6. Unknown order status must block readiness in the live preflight path.
7. Account-wide active orders must remain an account safety guard while target
   lifecycle truth remains instrument-scoped.
8. Stale account/positions/orders/trades/quotes/instrument/schedule must block
   live entry.
9. Trade/fill evidence must explain position delta and round-trip net flatness.
10. Runtime command consumer must remain disabled until it consumes canonical
    readiness, lifecycle storage, and broker truth summaries.

### P1 — blocks continuous runtime

1. FINAM stream heartbeat/reconnect/polling fallback policy needs canonical
   timeout and degraded-state representation.
2. Replace order lifecycle is not designed.
3. Close-only/emergency-exit policy must be separated from entry policy.
4. Event publication sink degradation should block runtime live.
5. Fee/commission/variation margin reporting needs economics reconciliation.

### P2 — technical debt

1. Harmonize naming between existing `OrderPreflightPolicy` and new
   `BrokerOperationalConfig`.
2. Replace ad-hoc evidence counters with canonical `BrokerTruthSnapshot`
   summaries wherever possible.
3. Add richer sanitized ALOR fixtures for cash/margin and schedule once
   available.

## Broker-neutral operational rules

- target lifecycle truth is instrument-scoped;
- account-wide truth is a safety guard;
- zero-quantity position rows are diagnostic, not open position;
- active orders require active lifecycle and `remaining_qty > 0` or missing
  remaining quantity that blocks as unknown/incomplete truth;
- unknown order status blocks readiness;
- cash/margin sufficiency is required before entry;
- stale broker truth blocks live entry;
- market closed blocks entry, while emergency cancel may remain allowed through
  a separate cancel-only policy;
- blind retry after ambiguous place/cancel send is forbidden without
  reconciliation.

## Required P0 tests / TODO-tests

Current broker-core coverage includes executable tests for:

1. target zero-qty position row => flat;
2. other instrument non-zero position does not make target non-flat;
3. target active status with remaining quantity zero is not working active;
4. target active status with missing remaining quantity remains blocking;
5. unknown order status is separate blocking truth;
6. account-wide active orders remain safety guard;
7. cash/margin missing blocks margin sufficiency;
8. stale positions/orders/quote/account blocks live entry;
9. market closed blocks entry but not emergency cancel.

TODO before further live expansion:

1. same ticker different MIC/board/expiry is not same instrument;
2. trade/fill explains position delta;
3. round-trip buy/sell net qty=0 => flat when target non-zero quantity is zero;
4. FINAM schedule/session freshness drives `BrokerMarketSessionState`;
5. M4/M5 preflight uses canonical readiness and broker truth summaries, not
   local counters.

## FINAM mapper requirements after M4-2c

The FINAM mapper must produce or feed the canonical model:

- `BrokerTruthSnapshot`;
- `BrokerCashSnapshot`;
- `BrokerInstrumentSpec`;
- `BrokerTradeSnapshot`;
- `BrokerReadinessSnapshot`.

M4/M5 evidence should not use local counters directly when the canonical
snapshot can provide the same business truth. Local counters may remain only as
diagnostic fields.

## ALOR oracle extraction after M4-2c

The ALOR gateway remains the operational oracle for:

- positions by symbol;
- zero-quantity semantics;
- active/terminal/unknown order statuses;
- synced positions/orders/stop-orders readiness;
- cash/margin where available;
- instrument identity;
- command lifecycle and idempotency;
- no-entry behavior during history sync, reconnect, gap sync, stale control
  path, degraded event sink, or closed market.

## Acceptance for M4-2c

M4-2c is ready for review when:

- this specification is present;
- broker-core exposes the canonical config/capability/readiness structures;
- broker-core operational tests pass;
- broker-finam M4-2b mapper tests still pass;
- the M4-2c evidence script reports no live calls and live expansion blocked;
- handoff archive excludes `.env`, `.git`, `target`, `tmp`, reports, logs, and
  development artifacts.
