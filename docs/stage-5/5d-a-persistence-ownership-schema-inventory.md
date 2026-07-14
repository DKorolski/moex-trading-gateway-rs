# Stage 5D-a — persistence ownership and schema inventory

Status: accepted inventory, pending Stage 5D-a3 enforcement/type-state bridge
design. Scope: design/inventory only.

Stage 5C is formally closed. Stage 5D-a starts the state/riskgate persistence
work without changing the frozen Stage 5C public API. Exact source-hash changes
needed for Stage 5D are handled by the Stage 5D-a2/a3 controlled additive
freeze-extension design.

This slice does not add Redis stream bridges, command consumers, FINAM
execution, broker transport, runtime-live, autonomous loops, or broker-side
Stop/SLTP/bracket execution.

## 1. Goal

Define what must be persisted, what must be derived, and what must remain
broker-truth authoritative before implementing any durable Stage 5D host.

The persistence contract must support deterministic restart for the accepted
paper-only Stage 5C lifecycle:

```text
admission
→ persisted state load or clean prepare
→ broker-truth bootstrap
→ runtime-state-restored callback
→ history warmup
→ pending recovery
→ bounded paper loop
```

## 2. Source inventory

The inventory is based on current source contracts:

| Source | Role |
| --- | --- |
| `crates/strategy-runtime-core/src/runtime_compat.rs::StrategyState` | Source-compatible serialized strategy projection; not sufficient alone for exact deterministic restart. |
| `crates/strategy-runtime-core/src/runtime_compat.rs::RuntimeStateRestored` | Known broker orders and pending requests injected after restore. |
| `crates/strategy-runtime-core/src/runtime_compat.rs::RiskGateRuntimeState` | Materialized riskgate state callback. |
| `crates/strategy-runtime-core/src/runtime_compat.rs::RiskGateSessionFinalization` | Runtime-produced session finalization acknowledgements. |
| `crates/strategy-runtime-core/src/hybrid_intraday/risk_gate.rs` | Runtime-ledger riskgate row, record, identity, materialized state, startup decisions, and validation rules. |
| `crates/broker-core/src/hybrid_strategy_boundary.rs` | Broker-neutral bootstrap/restored/riskgate DTOs. |
| `crates/broker-core/src/paper.rs` | Existing paper ledger and ALOR-seeded runtime projection shapes. |

Stage 5D implementation must not infer persistence semantics from diagnostics
alone. Diagnostics may help observability, but restore must use a versioned
state envelope plus broker-truth bootstrap.

## 2.1 Stage 5D code/API ownership under the Stage 5C freeze

Stage 5C remains closed. Stage 5D must not modify the frozen Stage 5C public
type-state API or the accepted Stage 5C production source semantics.

The implementable persistence seam requires a controlled additive freeze
extension. It is not implementable while simultaneously requiring all frozen
Stage 5C source hashes to remain unchanged.

```text
crates/strategy-runtime-core/src/stage5d_persistence.rs
crates/strategy-runtime-core/src/lib.rs
crates/strategy-runtime-core/src/hybrid_intraday_runtime.rs
crates/strategy-runtime-core/src/stage5c_paper_host.rs
```

The new `stage5d_persistence.rs` module alone is not enough. It must be paired
with narrow crate-private bridge additions in the wrapper and Stage 5C
type-state host so it can snapshot/restore runtime-private fields and preserve
the linear Stage 5C capability chain.

Stage 5D-a2 defines the reviewed extension with these rules:

| Boundary | Policy |
| --- | --- |
| Stage 5C public symbols | The existing 95 Stage 5C symbols, signatures, type-state kinds, fields and methods remain unchanged. |
| Stage 5C source hashes | The previous Stage 5C closure hashes are archived as the closed baseline; a new versioned additive-extension baseline is pinned after review. |
| `lib.rs` change | May add a separate `stage5d_persistence` module and `Stage5d*` exports only as part of the controlled extension. |
| Runtime bridge | `hybrid_intraday_runtime.rs` may add crate-private export/apply methods for a versioned Stage 5D runtime-private snapshot DTO. |
| Type-state bridge | `stage5c_paper_host.rs` may add crate-private Stage 5D mapping/transition helpers that preserve opaque public capabilities. |
| Scanner/checker behavior | Stage 5C checker continues to validate the 95 public Stage 5C symbols exactly; a separate Stage 5D manifest/checker registers Stage 5D exports and pins the additive source baseline. |
| Opaque Stage 5C capabilities | Must not gain public extractors that reveal inner strategy state. |
| Persistence producer | A Stage 5D capability consumes/borrows accepted Stage 5C/host-owned state internally and emits a versioned persistence envelope. |
| Persistence consumer | A Stage 5D restore capability validates envelope + broker truth + riskgate authority before re-entering Stage 5C startup. |

Therefore Stage 5D-b must not start DTO/schema implementation until Stage 5D-a3
selects and freezes both the additive enforcement migration and exact
Stage5c/Stage5d type-state bridge. Stage 5D-a2/a3 are design-only and must
prove the bridge can enter and exit the Stage 5C type-state chain without
opening Redis, FINAM, transport, dispatch, runtime-live, public raw strategy
extractors, or broker execution.

## 3. Ownership classes

### 3.0 Complete `StrategyState::HybridIntradayRuntime` field inventory

The current serialized `StrategyState::HybridIntradayRuntime` has 71 fields.
Stage 5D-a classifies every field below. `StrategyState` is a
source-compatible semantic projection, not by itself a complete deterministic
restart snapshot.

| # | Field | Stage 5D ownership class |
| --- | --- | --- |
| 1 | `active_cycle_id` | Persisted runtime-owned. |
| 2 | `next_cycle_seq` | Persisted runtime-owned. |
| 3 | `last_position_qty` | Persisted runtime-owned, broker-truth verified. |
| 4 | `current_owner` | Persisted runtime-owned, broker-truth verified when non-flat. |
| 5 | `current_side` | Persisted runtime-owned, broker-truth verified when non-flat. |
| 6 | `pending_entry_owner` | Persisted runtime-owned plus Stage 5D extension gap handling. |
| 7 | `pending_entry_side` | Persisted runtime-owned plus Stage 5D extension gap handling. |
| 8 | `pending_entry_cycle_id` | Persisted runtime-owned plus Stage 5D extension gap handling. |
| 9 | `pending_entry_request_id` | Persisted runtime-owned, must reconcile with `pending_requests`. |
| 10 | `pending_entry_created_ts_utc` | Persisted runtime-owned. |
| 11 | `deferred_entry_owner` | Persisted runtime-owned. |
| 12 | `deferred_entry_side` | Persisted runtime-owned. |
| 13 | `deferred_entry_cycle_id` | Persisted runtime-owned. |
| 14 | `deferred_entry_entry_style` | Persisted runtime-owned. |
| 15 | `deferred_entry_reason` | Persisted runtime-owned. |
| 16 | `deferred_entry_stop_price` | Persisted runtime-owned. |
| 17 | `deferred_entry_take_price` | Persisted runtime-owned. |
| 18 | `deferred_entry_ts_utc` | Persisted runtime-owned. |
| 19 | `deferred_entry_request_id` | Persisted runtime-owned, must reconcile with `pending_requests`. |
| 20 | `pending_exit_request_id` | Persisted runtime-owned plus Stage 5D extension gap handling. |
| 21 | `pending_exit_created_ts_utc` | Persisted runtime-owned. |
| 22 | `deferred_exit_owner` | Persisted runtime-owned. |
| 23 | `deferred_exit_reason` | Persisted runtime-owned. |
| 24 | `deferred_exit_cycle_id` | Persisted runtime-owned. |
| 25 | `deferred_exit_ts_utc` | Persisted runtime-owned. |
| 26 | `deferred_exit_request_id` | Persisted runtime-owned, must reconcile with `pending_requests`. |
| 27 | `pending_tp_request_id` | Persisted runtime-owned, bracket lifecycle verified. |
| 28 | `pending_tp_created_ts_utc` | Persisted runtime-owned. |
| 29 | `pending_sl_request_id` | Persisted runtime-owned, bracket lifecycle verified. |
| 30 | `pending_sl_created_ts_utc` | Persisted runtime-owned. |
| 31 | `tp_order_id` | Persisted broker ID hint; broker truth authoritative. |
| 32 | `sl_stop_order_id` | Persisted stop ID hint; broker truth authoritative. |
| 33 | `sl_exchange_order_id` | Persisted broker order ID hint; broker truth authoritative. |
| 34 | `sl_triggered_ts` | Persisted runtime-owned. |
| 35 | `mr_take_price` | Persisted runtime-owned. |
| 36 | `mr_stop_price` | Persisted runtime-owned. |
| 37 | `repair_deadline_ts` | Persisted runtime-owned. |
| 38 | `next_repair_at_ts` | Persisted runtime-owned. |
| 39 | `repair_backoff_level` | Persisted runtime-owned. |
| 40 | `repair_attempts` | Persisted runtime-owned. |
| 41 | `safe_mode_close_only` | Persisted runtime-owned, entry-blocking. |
| 42 | `safe_mode_reason` | Persisted runtime-owned. |
| 43 | `entry_ready` | Recomputable; restore-then-verify against warmup/readiness. |
| 44 | `last_bar_close` | Recomputable; restore-then-verify against canonical history. |
| 45 | `prev_day_close` | Recomputable; restore-then-verify against canonical history. |
| 46 | `last_day_local` | Recomputable; restore-then-verify against session calendar. |
| 47 | `current_day_high` | Recomputable; restore-then-verify. |
| 48 | `current_day_low` | Recomputable; restore-then-verify. |
| 49 | `current_day_close` | Recomputable; restore-then-verify. |
| 50 | `prev_day_range` | Recomputable; restore-then-verify. |
| 51 | `prev_day_return` | Recomputable; restore-then-verify. |
| 52 | `day_before_close` | Recomputable; restore-then-verify. |
| 53 | `today_start_local` | Recomputable; restore-then-verify against session calendar. |
| 54 | `was_long_today` | Persisted runtime-owned session flag, session-calendar verified. |
| 55 | `was_short_today` | Persisted runtime-owned session flag, session-calendar verified. |
| 56 | `overnight_exit_armed_date` | Persisted runtime-owned session flag. |
| 57 | `risk_gate_shadow_session_date` | Persisted runtime-owned current shadow session; riskgate authority reconciled. |
| 58 | `risk_gate_shadow_pnl_points` | Persisted runtime-owned current shadow session; riskgate authority reconciled. |
| 59 | `risk_gate_shadow_trade_count` | Persisted runtime-owned current shadow session; riskgate authority reconciled. |
| 60 | `risk_gate_shadow_entry_ts_utc` | Persisted runtime-owned current shadow open trade. |
| 61 | `risk_gate_shadow_entry_price` | Persisted runtime-owned current shadow open trade. |
| 62 | `risk_gate_shadow_side` | Persisted runtime-owned current shadow open trade. |
| 63 | `risk_gate_shadow_target_price` | Persisted runtime-owned current shadow open trade. |
| 64 | `risk_gate_shadow_stop_price` | Persisted runtime-owned current shadow open trade. |
| 65 | `risk_gate_pending_session_date` | Persisted runtime-owned pending finalization, reconciled with durable ledger. |
| 66 | `risk_gate_pending_shadow_pnl_points` | Persisted runtime-owned pending finalization, reconciled with durable ledger. |
| 67 | `risk_gate_pending_shadow_trade_count` | Persisted runtime-owned pending finalization, reconciled with durable ledger. |
| 68 | `risk_gate_mr_enabled_current_session` | Riskgate cache, not authority; validate/overwrite from materialized state. |
| 69 | `risk_gate_rolling_sum_lb120` | Riskgate cache, not authority; validate/overwrite from materialized state. |
| 70 | `risk_gate_last_finalized_session_date` | Riskgate cache, not authority; validate/overwrite from ledger/materialized state. |
| 71 | `risk_gate_ledger_rows_count` | Riskgate cache, not authority; validate/overwrite from ledger/materialized state. |

### 3.1 Persisted runtime-owned fields

These fields are strategy-owned and must survive restart unless explicitly
invalidated by schema/config/migration policy:

| Group | Fields |
| --- | --- |
| Cycle identity | `active_cycle_id`, `next_cycle_seq` |
| Position model | `last_position_qty`, `current_owner`, `current_side` |
| Pending entry | `pending_entry_owner`, `pending_entry_side`, `pending_entry_cycle_id`, `pending_entry_request_id`, `pending_entry_created_ts_utc` |
| Deferred entry | `deferred_entry_owner`, `deferred_entry_side`, `deferred_entry_cycle_id`, `deferred_entry_entry_style`, `deferred_entry_reason`, `deferred_entry_stop_price`, `deferred_entry_take_price`, `deferred_entry_ts_utc`, `deferred_entry_request_id` |
| Pending exit | `pending_exit_request_id`, `pending_exit_created_ts_utc` |
| Deferred exit | `deferred_exit_owner`, `deferred_exit_reason`, `deferred_exit_cycle_id`, `deferred_exit_ts_utc`, `deferred_exit_request_id` |
| Protective order intent state | `pending_tp_request_id`, `pending_tp_created_ts_utc`, `pending_sl_request_id`, `pending_sl_created_ts_utc` |
| Broker object references | `tp_order_id`, `sl_stop_order_id`, `sl_exchange_order_id` |
| MR bracket levels | `mr_take_price`, `mr_stop_price` |
| Stop/repair escalation | `sl_triggered_ts`, `repair_deadline_ts`, `next_repair_at_ts`, `repair_backoff_level`, `repair_attempts` |
| Safe mode | `safe_mode_close_only`, `safe_mode_reason` |
| Session flags | `was_long_today`, `was_short_today`, `overnight_exit_armed_date` |
| Riskgate shadow open session | `risk_gate_shadow_session_date`, `risk_gate_shadow_pnl_points`, `risk_gate_shadow_trade_count`, `risk_gate_shadow_entry_ts_utc`, `risk_gate_shadow_entry_price`, `risk_gate_shadow_side`, `risk_gate_shadow_target_price`, `risk_gate_shadow_stop_price` |
| Riskgate pending finalization | `risk_gate_pending_session_date`, `risk_gate_pending_shadow_pnl_points`, `risk_gate_pending_shadow_trade_count` |
| Riskgate materialized cache in state | `risk_gate_mr_enabled_current_session`, `risk_gate_rolling_sum_lb120`, `risk_gate_last_finalized_session_date`, `risk_gate_ledger_rows_count` are serialized and restored by `set_state`, but are not authoritative; they must be checked against and overwritten by materialized riskgate state. |

### 3.2 Persisted but recomputable market/session features

These fields may be restored for warm continuity but must be checked against
history warmup and session rules:

| Group | Fields |
| --- | --- |
| Entry readiness | `entry_ready` |
| Last known bar feature | `last_bar_close` |
| Previous/current day features | `prev_day_close`, `last_day_local`, `current_day_high`, `current_day_low`, `current_day_close`, `prev_day_range`, `prev_day_return`, `day_before_close`, `today_start_local` |

Stage 5D must define whether these fields are trusted from the persisted
envelope, recomputed from canonical history, or restored then verified. The
default policy should be restore-then-verify, with fail-closed behavior when
canonical warmup contradicts persisted values.

### 3.3 Broker-truth authoritative fields

These values cannot be trusted from persisted strategy state alone:

| Truth | Source |
| --- | --- |
| Actual target position quantity and flat/non-flat state | Broker-truth bootstrap snapshot. |
| Target active orders and account-wide active order guard | Broker-truth bootstrap snapshot. |
| Whether broker order IDs are still live/terminal/orphaned | Broker-truth order snapshots. |
| Whether broker stop IDs are still live/terminal/orphaned | Broker-truth stop/order snapshots when available. |
| Cash, margin, portfolio availability | Broker-truth portfolio/cash snapshots. |
| Unknown active orders for target instrument | Broker-truth safety guard, not runtime state. |

On restart, broker truth must be loaded before runtime state is trusted by the
strategy lifecycle. Persisted broker IDs are identity hints until broker truth
confirms or invalidates them.

### 3.4 Derived/diagnostic fields

These fields may be emitted to Redis/state streams for observability, but should
not be the primary restore source:

| Derived/diagnostic | Reason |
| --- | --- |
| Account-wide position row count | Diagnostic only; target instrument state is authoritative for strategy lifecycle. |
| Account-wide active order count | Safety guard; target-symbol active orders are lifecycle truth. |
| Runtime-state stream projections | Useful for comparison with ALOR and FINAM paper, but lossy relative to `StrategyState`. |
| Paper ledger projection strings | Good operator diagnostics; not sufficient as canonical persisted strategy state. |

## 3.5 Runtime-private restart-sensitive schema gaps

`StrategyState` alone is not sufficient for exact deterministic restart. The
Stage 5D envelope must explicitly handle runtime-private fields that are not
represented exactly by `StrategyState`.

| Gap | Current risk if only `strategy_state_json` is persisted | Stage 5D-a policy |
| --- | --- | --- |
| Pending entry semantic payload: `reason`, `entry_style`, `stop_price`, `take_price`, `target_qty` | `set_state` reconstructs partial defaults such as `MorningMeanReversionLong`, `Market`, no prices, config qty. This can change BO/MR/marketable-limit semantics. | Persist in Stage 5D extension envelope; if absent for a pending entry, block restore unless broker truth proves the lifecycle is terminal and safe to clear. |
| Partial entry timeout clock: `partial_started_at_ms` | Restart can reset the partial-entry timeout and extend risk. | Persist in Stage 5D extension envelope. If missing while broker truth shows partial/non-flat unresolved entry, block restore or enter safe close-only according to a later reviewed policy. |
| Pending exit owner/reason | `set_state` preserves request ID but not enough internal exit context. Recoverable negative ACK handling may lose owner/reason. | Persist in Stage 5D extension envelope; if missing with `pending_exit_request_id`, block restore unless broker lifecycle terminally resolves before runtime re-entry. |
| Bracket reconciliation timer: `bracket_terminal_reconcile_started_ms` | Restart can forget the 3000-ms grace lifecycle and change residual flatten timing. | Persist in Stage 5D extension envelope; if missing while bracket terminal reconciliation is active, block restore or safe close-only, not silent reset. |
| Cleanup retry count: `cleanup_stop_retry_attempts` | Restart can reset retry limits and repeat already exhausted cleanup attempts. | Persist in Stage 5D extension envelope. Missing value defaults only when no cleanup lifecycle is active. |
| Broker working sets: `working_orders`, `working_stop_orders` | Silent reset can hide active broker objects. | Rebuild from broker truth, not from `StrategyState`. Any unknown target active order blocks restore until classified. |
| Event/history watermark: `last_processed_bar_ts` | Duplicate or skipped semantic bar after restart. | Rebuild from canonical history/warmup and compare with persisted lifecycle watermark. Contradiction blocks restore. |
| Pending riskgate finalizations vector | `StrategyState` stores a scalar pending finalization shape; runtime can hold multiple finalizations. | Persist a Stage 5D riskgate finalization outbox. Scalar `StrategyState` fields are compatibility hints only. |

These gaps mean Stage 5D-b must not claim `strategy_state_json` is a complete
round-trip. The envelope must contain:

```text
strategy_state_json
+ stage5d_runtime_private_extension
+ broker_truth_bootstrap
+ riskgate_authority_section
+ restart_watermarks
```

If the extension is missing for a restart-sensitive active lifecycle, restore
must block before Stage 5C re-entry.

## 4. Riskgate persistence inventory

Riskgate has two related state layers.

### 4.1 Durable ledger records

`RiskGateLedgerRecord` is the durable row-level source of truth:

```text
session_date
shadow_pnl_points
shadow_trade_count
rolling_sum_before_session
mr_enabled_for_session
source
status
profile_id
mr_variant
timeframe
session_policy
rolling_sum_lb120
mr_enabled_next_session
model_version
finalized_at_utc
```

Stage 5D must preserve `RiskGateProfileIdentity`:

```text
strategy_id
profile_id
mr_variant
timeframe
session_policy
model_version
```

Any identity mismatch must block restore/rebuild rather than silently append to
the wrong ledger.

### 4.2 Materialized riskgate state

`RiskGateMaterializedState` is derived from ledger records and current runtime
shadow session:

```text
last_finalized_session_date
rolling_sum_lb120
mr_enabled_current_session
mr_enabled_next_session
seed_loaded
ledger_rows_count
current_shadow_session_date
current_shadow_pnl_points
current_generation
```

The durable ledger is authoritative for finalized sessions. The materialized
state can be cached but must be rebuildable from the ledger and current runtime
shadow state.

### 4.3 Finalization acknowledgements

`RiskGateSessionFinalization` and
`acknowledge_risk_gate_session_finalizations(...)` imply a durable
acknowledgement boundary:

- a session finalization must not be lost across restart;
- an acknowledged session must not be duplicated;
- pending finalization state in `StrategyState` must reconcile with durable
  riskgate ledger tail;
- normal append must refuse a ledger behind seed/history.

### 4.4 Riskgate authority precedence

Stage 5D must use one riskgate authority order:

| Layer | Authority policy |
| --- | --- |
| Durable `RiskGateLedgerRecord` rows | Authoritative for finalized sessions. Identity and monotonic session validation are mandatory. |
| `RiskGateMaterializedState` | Deterministic projection from ledger plus current shadow session; may be cached but must be rebuildable. |
| `StrategyState` risk_gate cache fields | Not authoritative. They are compatibility/cache fields and must be validated against or overwritten by materialized state. |
| Runtime shadow current session fields | Runtime-owned current-session state; reconciled with ledger tail before finalization. |
| Redis/operator projections | Diagnostic only. |

If `strategy_state_json.risk_gate_rolling_sum_lb120` contradicts
`riskgate_materialized_state.rolling_sum_lb120`, the materialized state wins and
the contradiction must be recorded as a restore warning or blocker according to
severity. Silent trust of both values is forbidden.

### 4.5 Riskgate startup and injection order

Stage 5D-b must choose this startup order unless a later review changes it:

```text
1. Load persistence envelope and validate schema/config/account/instrument.
2. Load and validate riskgate ledger identity.
3. Rebuild or validate materialized riskgate state from ledger + current shadow session.
4. Reconcile riskgate finalization outbox with ledger tail.
5. Load strategy semantic state and Stage 5D runtime-private extension.
6. Load broker-truth bootstrap.
7. Validate strategy position/order hints against broker truth.
8. Apply broker-truth bootstrap through the controlled Stage 5C startup path.
9. Inject authoritative RiskGateRuntimeState through Stage 5D internal facade.
10. Invoke runtime-state-restored lifecycle.
11. Warm up canonical history and verify recomputable fields/watermarks.
12. Enable bounded paper loop only after all previous gates pass.
```

This sequence requires the Stage 5D internal facade from section 2.1 because
`on_risk_gate_state(...)` is not opened by the Stage 5C public host facade.

### 4.6 Riskgate finalization crash consistency

Finalization idempotency identity:

```text
strategy_id
+ profile_id
+ mr_variant
+ timeframe
+ session_policy
+ model_version
+ session_date
+ generation
```

Finalization states:

| State | Meaning |
| --- | --- |
| `prepared` | Runtime emitted finalization, not yet appended to durable ledger. |
| `ledger_appended` | Durable ledger contains the session row. |
| `materialized_updated` | Materialized state reflects the appended row. |
| `acknowledged_in_runtime` | Runtime has acknowledged the finalization. |

Crash rules:

- crash before `ledger_appended`: retry append using the same idempotency
  identity;
- crash after `ledger_appended` but before runtime acknowledgement: do not
  append duplicate row; rebuild materialized state and re-acknowledge runtime;
- ledger tail ahead of finalization outbox: validate identity and mark
  acknowledged only if values match exactly;
- outbox ahead of ledger with stale config/profile identity: block restore.

## 5. Proposed versioned persistence envelope

Stage 5D implementation should introduce an explicit envelope before writing
or reading durable runtime state:

```text
schema_version
stage = "5D"
snapshot_id
snapshot_revision
previous_revision
write_generation
strategy_kind
strategy_id
account_id
instrument_id
broker_protocol_schema_version
runtime_state_schema_version
stage5c_compat_config_fingerprint
stage5d_canonical_config_fingerprint
profile_binding
created_ts_utc
persisted_ts_utc
source_commit_or_build_id
strategy_state_json
stage5d_runtime_private_extension
riskgate_identity
riskgate_materialized_state
riskgate_ledger_tail_summary
riskgate_ledger_tail_hash
known_order_ids
pending_requests
persisted_event_watermark
last_semantic_bar_ts
last_broker_event_ts
timestamp_units
payload_checksum
migration_policy
```

The envelope must preserve broker-neutral IDs as strings:

- `BrokerOrderId`;
- `BrokerStopOrderId`;
- `BrokerTradeId` if introduced in runtime persistence;
- `StrategyRequestId` remains distinct from broker-native order IDs.

Integrity rules:

- `snapshot_revision` must monotonically advance per strategy/account/instrument;
- `previous_revision` prevents stale overwrite and rollback;
- `write_generation` prevents split-brain writers;
- `payload_checksum` covers the canonical serialized payload excluding the
  checksum field itself;
- `persisted_event_watermark`, `last_semantic_bar_ts`, and
  `last_broker_event_ts` bind runtime state to event/history progress;
- `timestamp_units` must explicitly declare seconds vs milliseconds for every
  timestamp family;
- `riskgate_ledger_tail_hash` binds materialized riskgate state to ledger tail.

## 6. Config/schema fingerprint

A restore attempt must bind persisted state to the runtime configuration:

| Fingerprint input | Required policy |
| --- | --- |
| Strategy kind/profile | Exact match. |
| Instrument identity and tick size | Exact match. |
| Account ID | Exact match. |
| MR variant and gate policy | Exact match. |
| Riskgate mode/profile identity | Exact match. |
| Runtime state schema version | Must be supported by migration table. |
| Broker protocol schema version | Must be compatible with broker-neutral ID semantics. |

Config mismatch should block restore unless a future explicitly reviewed
migration says otherwise.

Stage 5D must define a durable canonical fingerprint. The current Stage 5C
fingerprint based on debug formatting is a compatibility input only:

| Fingerprint | Stage 5D role |
| --- | --- |
| `stage5c_compat_config_fingerprint` | Preserved for compatibility with accepted Stage 5C gates. Not sufficient as the long-term persistence identity. |
| `stage5d_canonical_config_fingerprint` | Authoritative Stage 5D persistence binding. |

Canonical fingerprint requirements:

- `fingerprint_schema_version`;
- `hash_algorithm = sha256`;
- explicit canonical field list;
- canonical enum strings;
- canonical decimal encoding;
- canonical duration/time encoding;
- stable ordering independent of Rust `Debug`;
- normalization rules for optional/default fields;
- profile/riskgate identity included by value, not inferred from names.

## 7. Migration policy

Allowed by default:

- current broker-neutral string IDs;
- accepted legacy ALOR positive numeric IDs converted to decimal strings only
  under explicit legacy import policy;
- missing optional fields that have safe `serde(default)` semantics and are
  covered by restore tests.

Blocked by default:

- zero/negative legacy numeric order IDs;
- lossy FINAM/native ID truncation;
- mapping `ClientOrderId` to `StrategyRequestId`;
- unknown/corrupt schema versions;
- state with broker-owned order IDs contradicted by broker truth;
- state whose position side/quantity contradicts broker truth;
- riskgate ledger identity mismatch.

Per-field legacy ID policy:

| Field | Legacy source type | Accepted input | Target type | Zero/negative/overflow | Broker-truth verification |
| --- | --- | --- | --- | --- | --- |
| `tp_order_id` | ALOR numeric order ID | Positive integer or string | `BrokerOrderId` decimal/string | Reject zero, negative, non-integer, overflow | Must match target active/terminal broker order if present. |
| `sl_exchange_order_id` | ALOR numeric order ID | Positive integer or string | `BrokerOrderId` decimal/string | Reject zero, negative, non-integer, overflow | Must match exchange/broker order truth if present. |
| `sl_stop_order_id` | Broker stop ID namespace | String only | `BrokerStopOrderId` | Numeric import not accepted by default | Must match stop-order truth when available. |
| `pending_*_request_id` | Strategy request UUID | UUID/string accepted by typed parser | `StrategyRequestId` | Numeric import forbidden | Must reconcile with `pending_requests`. |
| `deferred_*_request_id` | Strategy request UUID | UUID/string accepted by typed parser | `StrategyRequestId` | Numeric import forbidden | Must reconcile with deferred lifecycle state. |
| `known_order_ids` | Broker order IDs | String IDs; positive ALOR numeric only if explicitly imported as broker order strings | `Vec<BrokerOrderId>` | Reject zero/negative/overflow | Must be subset/equivalent of broker-truth classified known IDs or marked stale. |
| `BrokerTradeId` | Broker trade IDs | String only until separately reviewed | `BrokerTradeId` | Numeric import not accepted by default | Must match trade truth/ledger if persisted. |
| `ClientOrderId` | Client correlation ID | FINAM-safe string only | `ClientOrderId` | Must not become `StrategyRequestId` | Used for broker correlation only. |

Stage 5D must not generalize the Stage 5C legacy numeric conversion beyond the
explicitly accepted order-ID fields without a new review.

### 7.1 `known_order_ids` and `pending_requests` ownership

`known_order_ids` and `pending_requests` are host-owned recovery indexes. They
do not replace strategy lifecycle fields and do not authorize broker actions.

| Index | Required relationship |
| --- | --- |
| `pending_requests` | Must include active `pending_entry_request_id`, `pending_exit_request_id`, `pending_tp_request_id`, `pending_sl_request_id`, and deferred request IDs when those lifecycles are active. |
| Extra pending request | Blocks restore unless explained by Stage 5D extension or terminal broker truth. |
| Missing pending request | Blocks restore when corresponding `StrategyState` pending/deferred field is active. |
| `known_order_ids` | Recovery index of broker order IDs previously observed by the runtime. |
| Extra known order ID | Allowed only if broker truth classifies it terminal/stale or Stage 5D extension owns it. |
| Missing known order ID | Blocks restore if `tp_order_id`, `sl_exchange_order_id`, or active broker lifecycle references it. |
| Deletion | Allowed only after broker truth and lifecycle state agree the ID is terminal/stale. |

## 8. Restart invariants to fixture-test

Stage 5D-b/5D-c should add deterministic fixtures for these restart shapes:

| Scenario | Expected invariant |
| --- | --- |
| Flat clean | Restores cleanly, no pending orders, entry may proceed only after warmup/readiness. |
| Pending entry | Pending request preserved; broker truth must resolve active/terminal/unknown before new entry. |
| Partial entry | Position/order truth reconciles target quantity; unknown partial state blocks. |
| Open BO position | Cycle/owner/side/qty restored and verified against broker truth. |
| Open MR bracket | TP/SL broker IDs restored as hints, broker truth confirms active/terminal before lifecycle proceeds. |
| Pending exit | Pending exit request preserved; no duplicate exit until ACK/broker lifecycle resolves. |
| Active protection | Protective state restored; mismatch enters safe close-only or blocks according to accepted policy. |
| Deferred entry/exit | Deferred action state survives restart and remains request-id scoped. |
| Safe mode | `safe_mode_close_only` and reason survive restart; entry remains blocked. |
| Riskgate pending finalization | Finalization is idempotent and not lost or duplicated. |
| Ledger behind seed/history | Restore/rebuild blocks rather than silently appending. |

## 9. Stage 5D-a deliverables

This slice delivers only:

- this ownership/schema inventory;
- `docs/stage-5/5d-a2-controlled-additive-freeze-extension.md` as the
  controlled additive-extension principle;
- `docs/stage-5/5d-a3-additive-freeze-enforcement-and-type-state-bridge.md` as
  the follow-up design-only answer to the enforcement/type-state HOLD;
- `docs/current-status.md` update marking Stage 5C closed and Stage 5D-a as a
  held inventory with Stage 5D-a3 as the next gate.

No production source changes are part of Stage 5D-a.

## 10. Next proposed slices

If Stage 5D-a3 is accepted:

1. Stage 5D-b — Stage 5D manifest/checker plus versioned envelope DTO/API.
2. Stage 5D-c — runtime-private snapshot DTO fixtures and corruption gates.
3. Stage 5D-d — riskgate ledger/materialized-state round-trip fixtures.
4. Stage 5D-e — restore invariant matrix for flat/pending/open/safe-mode cases.
5. Stage 5D-f — durable external broker-event accumulation design for Stage
   5C-n terminal-complete batches.

Implementation should stay no-send and paper-only until a later separately
reviewed gate opens additional surfaces.
