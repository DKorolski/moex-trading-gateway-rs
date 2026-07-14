# Stage 5D-a — persistence ownership and schema inventory

Status: review candidate. Scope: design/inventory only.

Stage 5C is formally closed. Stage 5D-a starts the state/riskgate persistence
work without reopening the frozen Stage 5C API or any frozen Stage 5C source.

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
| `crates/strategy-runtime-core/src/runtime_compat.rs::StrategyState` | Full source-compatible strategy runtime state. |
| `crates/strategy-runtime-core/src/runtime_compat.rs::RuntimeStateRestored` | Known broker orders and pending requests injected after restore. |
| `crates/strategy-runtime-core/src/runtime_compat.rs::RiskGateRuntimeState` | Materialized riskgate state callback. |
| `crates/strategy-runtime-core/src/runtime_compat.rs::RiskGateSessionFinalization` | Runtime-produced session finalization acknowledgements. |
| `crates/strategy-runtime-core/src/hybrid_intraday/risk_gate.rs` | Runtime-ledger riskgate row, record, identity, materialized state, startup decisions, and validation rules. |
| `crates/broker-core/src/hybrid_strategy_boundary.rs` | Broker-neutral bootstrap/restored/riskgate DTOs. |
| `crates/broker-core/src/paper.rs` | Existing paper ledger and ALOR-seeded runtime projection shapes. |

Stage 5D implementation must not infer persistence semantics from diagnostics
alone. Diagnostics may help observability, but restore must use a versioned
state envelope plus broker-truth bootstrap.

## 3. Ownership classes

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

## 5. Proposed versioned persistence envelope

Stage 5D implementation should introduce an explicit envelope before writing
or reading durable runtime state:

```text
schema_version
stage = "5D"
strategy_kind
strategy_id
account_id
instrument_id
broker_protocol_schema_version
runtime_state_schema_version
config_fingerprint
profile_binding
created_ts_utc
persisted_ts_utc
source_commit_or_build_id
strategy_state_json
riskgate_identity
riskgate_materialized_state
riskgate_ledger_tail_summary
known_order_ids
pending_requests
migration_policy
```

The envelope must preserve broker-neutral IDs as strings:

- `BrokerOrderId`;
- `BrokerStopOrderId`;
- `BrokerTradeId` if introduced in runtime persistence;
- `StrategyRequestId` remains distinct from broker-native order IDs.

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
- `docs/current-status.md` update marking Stage 5C closed and Stage 5D-a as a
  review candidate.

No production source changes are part of Stage 5D-a.

## 10. Next proposed slices

If Stage 5D-a is accepted:

1. Stage 5D-b — versioned persistence envelope DTO design and JSON fixtures.
2. Stage 5D-c — deterministic restore/migration policy tests.
3. Stage 5D-d — riskgate ledger/materialized-state round-trip fixtures.
4. Stage 5D-e — restart invariant matrix for flat/pending/open/safe-mode cases.
5. Stage 5D-f — durable external broker-event accumulation design for Stage
   5C-n terminal-complete batches.

Implementation should stay no-send and paper-only until a later separately
reviewed gate opens additional surfaces.
