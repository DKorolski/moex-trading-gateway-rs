# M4-3u — ALOR gateway/runtime contract parity notes

Status: engineering note / parity checklist.

This note records the ALOR gateway/runtime operational contract that must not be
lost while attaching the FINAM contour to the `IMOEXF` hybrid runtime path.

Reviewed ALOR sources:

- `docs/alor-gateway-runbook.md`;
- `docs/strategy-runtime-runbook.md`;
- `docs/redis-runtime-state-and-snapshots.md`;
- `docs/imoexf-hybrid-mr-bo-handoff-2026-04-26.md`;
- `docs/hybrid-stage2-contract-freeze.md`;
- `docs/live-runtime-service-patterns-anti-regression-checklist-2026-05-07.md`;
- `docs/strategy-runtime-refactor/00-current-contract.md`;
- `docs/strategy-runtime-refactor/02-compatibility-checklist.md`;
- ALOR oracle hybrid runtime config for `ALOR_ORACLE_PORTFOLIO`;
- ALOR oracle hybrid gateway action-scoped config for `ALOR_ORACLE_PORTFOLIO`;
- selected runtime/gateway source files around strategy host, live guard,
  health, Redis transport, and hybrid state.

## Important ALOR facts for FINAM parity

### 1. Hybrid IMOEXF is a closed 10m-bar contract

ALOR gateway for the live `ALOR_ORACLE_PORTFOLIO` hybrid contour uses:

```text
tf_sec = 600
runtime bars stream = md.bars.<ALOR_ORACLE_PORTFOLIO>.10m
runtime state = runtime.state.hybrid_intraday.live.riskgate_shadow.imoexf.<ALOR_ORACLE_PORTFOLIO>
```

FINAM currently derives canonical 10m bars from FINAM M1 bars. That is acceptable
only if the resulting closed M10 stream is treated as the strategy input
contract. Strategy parity should not be judged on raw M1 behavior.

### 2. Runtime startup order is part of the contract

ALOR runtime freezes this lifecycle order:

1. load broker snapshots;
2. load persisted runtime state;
3. notify strategy about bootstrap snapshot;
4. notify strategy about restored runtime state;
5. warm up indicators from history with live orders disabled;
6. recover pending streams/events.

FINAM paper/runtime integration must preserve this order before it is allowed to
be more than shadow/paper.

### 3. Broker truth beats strategy assumptions

ALOR patterns:

- startup flat is a broker fact, not a strategy assumption;
- snapshots are symbol-filtered before strategy callbacks;
- non-flat or ambiguous target-symbol position can force safe mode;
- working orders / working stop-orders at bootstrap are operational blockers
  unless explicitly adopted by strategy logic;
- account-wide position row count is diagnostic only, while target-symbol
  non-zero quantity is position truth.

For FINAM this means canonical `BrokerTruthSnapshot` must feed runtime before
strategy state is trusted.

### 4. Live guard and readiness are runtime gates, not only gateway telemetry

ALOR readiness uses:

- `GatewayPhase`: `SyncingHistory`, `Reconnecting`, `SyncingGap`, `LiveReady`;
- `readiness`;
- `ws_connected`;
- `cws_authorized`;
- gateway health age / staleness;
- scheduler/trading-period state;
- first live bar / bars present after restart.

FINAM already has broker-neutral readiness phases, but the runtime attachment
must also evaluate the ALOR-equivalent guard:

```text
allow_live_orders
gateway phase / readiness
fresh gateway health
market data live
post-restart next-bar presence
trading-period state
```

### 5. Entry-only gates must not block exit/cleanup/repair

ALOR Stage-2 freeze says closed-window, break, warmup, and stale-input
restrictions are entry gates. They must not drop:

- exit;
- cancel cleanup;
- protective repair.

This is essential for future stop/SLTP/bracket work. Even before FINAM supports
brackets, the intent model should preserve intent classes rather than treating a
strategy output batch as all-or-nothing.

### 6. Request-id parity is a safety invariant

ALOR hardening moved strategy pending state to the final host-built command id:

- strategy emits intent;
- runtime prepares exact command/request id;
- strategy receives `on_command_prepared`;
- pending entry/exit id is persisted;
- ack handling clears only matching pending id;
- mismatched ack is logged as request-id skew and does not blindly clear state.

FINAM paper/runtime path should keep this seam. Strategy code should not guess
the final emitted request id from bar time.

### 7. If host drops intents, strategy state must roll back or explicitly keep state

ALOR runtime has an explicit blocked-intent disposition model:

- rollback by default;
- keep strategy state only when strategy says so;
- blocked live entry must not leave hidden strategy state believing there is a
  pending live order or position.

FINAM shadow integration must include this before real strategy invocation can
emit paper/live intents.

### 8. Event-time semantics are mandatory

Hybrid Stage-2 freeze uses event time, not wall clock, for strategy transitions:

- bar close time;
- broker order/position event timestamps;
- monotonic strategy time;
- saturating timeout arithmetic;
- timeout transitions disabled until the first valid event time.

FINAM derived M10 bars must carry the correct close timestamp and runtime should
advance strategy time from event timestamps.

### 9. Silence/gap protection is entry-only

ALOR has `max_silence_bars_sec = 1200` for hybrid. Large bar gaps block entry on
the first bar after the gap, but exits/cancel/repair remain allowed.

This should be preserved for FINAM reconnect/gap recovery. A recovered bar after
Wi-Fi/VPS reconnect should not create a blind entry if the gap is too large.

### 10. Action-scoped control path is the live baseline

ALOR hybrid live config uses:

```text
control_cws_mode = action_scoped
action_scope_force_token_refresh_before_authorize = true
action_scope_enable_market = true
action_scope_enable_exit = true
action_scope_enable_replace = false
```

For FINAM this maps conceptually to: do not keep a casual always-hot live order
surface by default. When real order placement returns, keep an explicit
operator-approved boundary, short-lived capability/request parts, durable audit,
and reconciliation-after-ambiguous-send policy.

### 11. Redis consumer group behavior is part of reliability

ALOR gateway/runtime both use consumer groups:

- create group before `XREADGROUP`;
- process entries;
- publish derived state/ack/evidence;
- `XACK` only after success or DLQ;
- inspect pending with `XPENDING`;
- recover stale pending via claim/reprocess path.

FINAM paper runtime consumer already follows the same broad pattern; future
strategy/runtime attachment should keep it rather than switching to ad-hoc tail
reads.

### 12. Stop-orders/brackets are intentionally not yet in FINAM scope

ALOR Stage-2 requires gateway stop-order support before hybrid runtime with
protective SL/TP/bracket can be enabled. Current FINAM contour is plain
market/limit/cancel + paper runtime only, so M4 must keep:

```text
stop_sltp_bracket_enabled = false
command_consumer_to_real_finam = false
runtime_live_ready = false
```

until a separate stop/bracket capability gate is passed.

### 13. Riskgate is model memory, not only diagnostics

ALOR `ALOR_ORACLE_PORTFOLIO` hybrid reads/appends:

```text
risk_gate_ledger_key = runtime.riskgate.sessions.hybrid_imoexf.imoexf_primary_high180_lb120
mr_gate_policy = shadow_pnl_lb120_positive
risk_gate_mode = normal_append
```

The ledger is model memory and should survive portfolio moves. FINAM parity
needs a riskgate projection and later true ledger integration before MR behavior
can be considered equivalent.

### 14. Weekend/session policy is part of the model contract

Hybrid handoff says:

- raw weekend data may be kept for audit;
- weekend bars must not generate trades;
- weekend bars must not become Monday anchor;
- Monday anchor should use previous regular weekday;
- BO should not carry across weekend/non-tradable gaps under frozen contract.

FINAM historical/live bars must preserve this same policy.

## Current FINAM gap list after M4-3s/M4-3t

Already present:

- FINAM M1 WS/shadow source;
- M1 to canonical closed M10 aggregation;
- Redis consumer group discipline for paper runtime consumer;
- paper-only runtime state stream;
- ALOR-compatible `hybrid_intraday` projection shell;
- paper-only strategy invocation shadow boundary;
- optional strategy owner/cycle tags in paper intents.
- broker-neutral runtime host contract for ALOR-compatible lifecycle,
  intent classes, command-prepared seam, event-time clock, live guard, and
  target-symbol bootstrap snapshot.

Still missing before true parity:

- real hybrid orchestrator invocation;
- true BO/MR decision production from FINAM-derived M10 bars;
- wiring `on_bootstrap_snapshot` equivalent to FINAM canonical broker truth;
- implementation of runtime state restore / warmup order against Redis;
- use of intent class model and blocked-intent rollback semantics in the paper
  runtime adapter;
- use of command-prepared/request-id exactness in the paper runtime adapter;
- riskgate ledger/state integration;
- target-symbol broker truth reconciliation loop;
- stop-order/bracket capability gate;
- ALOR-vs-FINAM field-by-field parity report over the same session window.

## Suggested next implementation order

1. Wire FINAM paper runtime bootstrap to canonical broker truth using the new
   broker-neutral runtime host contract.
2. Add intent class + blocked-intent disposition to paper runtime path.
3. Add command-prepared/request-id parity seam for paper first.
4. Port/reuse the ALOR hybrid orchestrator behind the current FINAM paper-only
   boundary.
5. Add FINAM canonical broker truth bootstrap snapshot into runtime state.
6. Add riskgate state/ledger projection.
7. Run same-session ALOR oracle vs FINAM paper state comparison.

No step above should enable real FINAM live orders.
