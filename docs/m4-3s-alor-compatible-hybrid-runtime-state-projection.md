# M4-3s — ALOR-compatible hybrid runtime state projection

Status: implementation slice / paper-only.

This step adds an ALOR-comparable `hybrid_intraday` projection inside
`PaperRuntimeState`. It does not enable live orders, command consumer to real
FINAM, stop/SLTP/bracket, or continuous runtime-live.

## Why

ALOR `HybridIntradayRuntime` exposes strategy-level fields used for operational
parity checks:

- cycle/owner/side state;
- pending/deferred entry/exit state;
- TP/SL and safe-mode state;
- daily feature state;
- riskgate summary.

The previous FINAM paper runtime state was only a paper-ledger heartbeat:

- strategy id;
- instrument;
- safety boundary;
- last decision / position / active order counters.

That was not enough to compare the FINAM paper contour with the ALOR hybrid
runtime state stream.

## Added projection

`PaperRuntimeState` now includes:

```text
hybrid_intraday: Option<PaperHybridIntradayRuntimeStateProjection>
```

The projection currently fills fields that can be safely derived from the paper
ledger and closed M10 runtime bar:

- `strategy_kind = "hybrid_intraday"`;
- `last_position_qty`;
- `current_side` from signed position quantity;
- `safe_mode_close_only = false`;
- `last_bar_close`;
- `current_day_high`;
- `current_day_low`;
- `current_day_close`;
- `last_day_local` / `today_start_local` as the current UTC date;
- riskgate summary from `RiskGatePaperState` when available;
- `projection_source = "paper_ledger_state_only"`;
- `strategy_invocation_enabled = false`.

Fields that require the actual hybrid strategy runtime/orchestrator remain
explicitly unset:

- `active_cycle_id`;
- `current_owner`;
- pending/deferred entry/exit fields;
- TP/SL order ids;
- MR take/stop prices;
- `entry_ready`;
- BO/MR ownership semantics.

## Runtime behavior

On every complete derived M10 bar, the paper runtime adapter now updates
paper-ledger bar/day features before publishing `PaperRuntimeState`.

The Redis payload remains on the runtime state stream and stays paper-only.

## Verified

Relative checks:

```text
cargo check -p broker-cli
cargo test -p broker-core paper_runtime -- --nocapture
cargo test -p finam-gateway m4_3r -- --nocapture
```

Synthetic Redis smoke produced a runtime state with:

```text
hybrid_intraday.strategy_kind = hybrid_intraday
hybrid_intraday.last_position_qty = 0.0
hybrid_intraday.last_bar_close = 2209.0
hybrid_intraday.current_day_high = 2210.0
hybrid_intraday.current_day_low = 2199.0
hybrid_intraday.projection_source = paper_ledger_state_only
hybrid_intraday.strategy_invocation_enabled = false
```

## Next

The next parity step is not broker/order work. It is strategy-runtime integration:

1. feed FINAM-derived M10 bars into the real hybrid strategy runtime logic;
2. project true `active_cycle_id`, `current_owner`, `entry_ready`, pending
   request ids, and BO/MR ownership;
3. attach riskgate ledger/state projection;
4. compare FINAM paper state vs ALOR live oracle state field-by-field.

See also `docs/m4-3u-alor-gateway-runtime-contract-parity-notes.md` for the
ALOR gateway/runtime invariants that must be preserved before the FINAM contour
is promoted beyond paper/shadow.

## M4-3t boundary slice

The follow-up implementation adds an explicitly gated paper-only hybrid strategy
shadow boundary:

- CLI/config can enable `strategy_invocation_shadow`;
- no broker commands are emitted;
- no real FINAM order endpoint is reachable;
- no live-ready flag is opened;
- the shadow boundary only observes complete derived M10 bars;
- after `strategy_warmup_bars` complete M10 bars it can set
  `hybrid_intraday.entry_ready = true`;
- projection source becomes `paper_hybrid_strategy_shadow_invocation`.

This is intentionally not the full ALOR hybrid strategy port. It is the safe
attachment point where the real hybrid orchestrator can later fill:

- `active_cycle_id`;
- `current_owner`;
- `pending_entry_request_id`;
- `pending_exit_request_id`;
- BO/MR owner semantics;
- riskgate ledger/state.

The paper intent model now also carries optional strategy tags:

- `strategy_owner`;
- `strategy_cycle_id`.

That means the future real hybrid strategy/orchestrator attachment can publish
ALOR-like ownership state without changing the Redis state schema again. When a
paper enter intent is tagged, the projection can adopt:

- `active_cycle_id`;
- `current_owner`;
- `current_side`.

## Local FINAM M1 smoke

Local Redis smoke against `finam_ws_shadow_local:market_data` produced one
complete derived M10 runtime state with the shadow boundary enabled:

```text
source_stream = finam_ws_shadow_local:market_data
bars_seen = 50
bars_published = 1
runtime_records_published = 1
runtime_state_stream = finam_imoexf_paper:runtime:state:m4_3s_1783360312

hybrid_intraday.entry_ready = true
hybrid_intraday.last_bar_key = 2026-07-06T13:40:00+00:00/600
hybrid_intraday.last_bar_close = 2194.5
hybrid_intraday.current_day_high = 2207.0
hybrid_intraday.current_day_low = 2188.5
hybrid_intraday.projection_source = paper_hybrid_strategy_shadow_invocation
hybrid_intraday.strategy_invocation_enabled = true
```

The smoke remained paper-only:

```text
order_placement_enabled = false
command_consumer_to_real_finam_enabled = false
live_ready_allowed = false
```
