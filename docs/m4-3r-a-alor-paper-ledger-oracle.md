# M4-3r-a ALOR paper/ledger oracle extraction

Status: source-only oracle spec / no-send / no live orders.

M4-3r-a extracts the mature ALOR runtime paper/replay/ledger semantics into a
broker-neutral oracle. This is the reference for the FINAM paper runtime work
that follows. The goal is not to copy ALOR transport code. The goal is to keep
the operational invariants that made the ALOR contour safe: state restore,
warmup-only history, idempotent decisions, paper ledger accounting, synthetic
feedback into strategy state, and risk-gate ledger separation.

## Boundary

This package must not:

- send FINAM orders;
- enable `POST /orders`;
- enable `DELETE /orders/{id}`;
- enable command-consumer-to-real-FINAM;
- enable runtime `LiveReady`;
- enable Stop/SLTP/bracket/replace/multi-leg;
- require ALOR live credentials or live Redis access.

The current ALOR contour remains the live oracle. FINAM remains a no-send
paper/shadow contour until the broker-neutral runtime layer is implemented and
reviewed.

## ALOR source inventory

The oracle is derived from these ALOR runtime sources:

| Source | Extracted semantics |
| --- | --- |
| `strategy-runtime/src/lib.rs` | `TradeMode`, `PaperExecutionMode`, `PaperConfig`, `ReplayConfig`. |
| `strategy-runtime/src/strategy_host.rs` | Strategy callback contract, `Intent`, `StrategyCtx`, `DataOrigin`, runtime-state restore, history warmup, risk-gate callbacks. |
| `strategy-runtime/src/runtime.rs` | Event loop, duplicate bar guard, paper/history execution gate, synthetic paper fills, synthetic position feedback, paper report persistence. |
| `strategy-runtime/src/trade_ledger.rs` | Order/fill records, closed-trade accounting, realized PnL, report writing. |
| `strategy-runtime/src/risk_gate_store.rs` | Risk-gate startup modes, seed/bootstrap/rebuild/append semantics, ledger identity checks. |
| `strategy-runtime/src/redis_transport.rs` | Redis stream publishing, approximate retention, atomic command+state write. |
| `strategy-runtime/src/state.rs` | Strategy state envelope and compatibility restore. |
| `strategy-runtime/src/strategies/hybrid_intraday_runtime.rs` | IMOEXF hybrid warmup, paper/live decision gates, risk-gate state sync. |

## Core oracle rules

### Trade modes

ALOR separates the runtime mode from the broker transport:

```text
Live     -> broker actions may be emitted only when live guards allow them
Paper    -> strategy can produce paper intents, no broker order is sent
Backtest -> offline simulation/reporting path
```

FINAM must preserve this separation. Paper mode belongs to the runtime/paper
adapter, not to the FINAM transport.

### Paper execution modes

ALOR has two paper execution modes:

```text
LiveOnly   -> only live-origin bars may advance paper execution
HistorySim -> historical/replay bars may advance deterministic simulation
```

For the FINAM IMOEXF paper-shadow contour, the default must be `LiveOnly`.
Historical and gap-recovery bars may warm state, but must not emit paper actions
unless a reviewed `HistorySim` replay run explicitly enables it.

### Data origin

ALOR distinguishes:

```text
History
HistoryGap
Live
Replay
```

The FINAM runtime adapter must preserve the same intent boundary:

- `Live` final canonical 10-minute bars may be decision input when readiness is
  healthy.
- `History`, `HistoryGap`, and `Replay` are warmup/rebuild inputs by default.
- Recovery data must carry provenance, and any paper action from it must require
  an explicit replay/simulation mode.

### State restore and warmup

ALOR restores strategy state through an envelope-compatible state model and then
warms indicators from recent bars with `allow_live_orders = false`.

The oracle rule is:

```text
restore persistent strategy state
  -> run history warmup with live actions disabled
  -> update last processed bar timestamp
  -> publish restored/warmed runtime state
  -> only then accept live-origin decision bars
```

Warmup must be side-effect safe: it can update indicators and runtime state, but
must not send live orders and must not create paper fills in `LiveOnly` mode.

### Duplicate and stale-bar guard

ALOR keeps last processed bar timestamps and rejects duplicate bars before
invoking the strategy. FINAM paper runtime must use the same invariant:

```text
same symbol + same close timestamp = duplicate, do not produce a new decision
```

This is one of the guards against one-bar drift and decision replacement.

## Paper ledger semantics

ALOR paper/backtest ledger records:

- order records;
- fill/trade records;
- open position quantity and cost;
- entry timestamp, entry price, and side;
- closed trades;
- gross/net PnL and commission totals;
- summary/report artifacts.

The key accounting behavior is:

```text
buy  while flat/long  -> increase long quantity/cost
sell while flat/short -> increase short quantity/cost
sell against long     -> close long up to current position, realize PnL
buy  against short    -> cover short up to current position, realize PnL
flip                  -> close old side and open remaining quantity on new side
flat                  -> close current trade and reset entry fields
```

The first FINAM paper ledger must implement these broker-neutral invariants
before it is used to judge strategy parity.

## Synthetic paper feedback

ALOR does not merely write a paper fill to a report. After a synthetic paper
fill, it feeds a synthetic position event back into the strategy. That lets the
strategy state move through the same position-aware lifecycle it would use in
live mode.

FINAM paper runtime must therefore produce:

```text
paper intent
  -> paper order
  -> deterministic paper fill/ack
  -> paper position delta
  -> strategy on_position feedback
  -> updated paper runtime state
```

Without this feedback loop, runtime state parity with ALOR will be misleading.

## Paper fill policy for FINAM

ALOR has the ledger and synthetic feedback machinery. M4-3r-b/M4-3r-c must add a
reviewed deterministic FINAM paper fill policy. The planned policy remains:

```text
market paper entry/exit = next final 10m bar open proxy
limit paper order       = working until deterministic bar-touch rule
cancel paper order      = terminal canceled if still working
```

Every paper fill must carry:

- source decision id;
- source bar bucket;
- fill policy;
- paper order id;
- resulting paper position delta.

## Risk-gate oracle

ALOR keeps long memory in a risk-gate ledger, not in the main strategy snapshot.

The current IMOEXF hybrid oracle requires:

```text
profile        = imoexf_primary_high180_lb120
mr_variant     = high180
mr_gate_policy = shadow_pnl_lb120_positive
risk_gate_mode = normal_append
```

The FINAM paper runtime must preserve:

- ledger stream as canonical historical memory;
- materialized risk-gate state as derived state;
- one-time seed/bootstrap modes separate from normal append;
- dedupe of finalized sessions;
- no enforced MR blocking until reviewed separately.

## Redis/runtime surface shape

ALOR runtime publishes state and broker/runtime events through Redis streams with
bounded retention. For command+state writes, ALOR uses an atomic write path.

FINAM paper runtime should expose the same class of surfaces, but isolated under
paper prefixes:

```text
finam_imoexf_paper:runtime:state:hybrid_intraday:imoexf
finam_imoexf_paper:runtime:intents
finam_imoexf_paper:runtime:paper_acks
finam_imoexf_paper:runtime:orders_paper_only
finam_imoexf_paper:runtime:trades_paper_only
finam_imoexf_paper:runtime:positions_paper_only
finam_imoexf_paper:riskgate:ledger:hybrid_imoexf:imoexf_primary_high180_lb120
finam_imoexf_paper:riskgate:state:hybrid_imoexf:imoexf_primary_high180_lb120
```

The exact stream names may evolve, but the separation must not: live broker
truth and paper runtime truth are different surfaces.

## Synthetic fixture

`fixtures/alor/paper_ledger_synthetic_round.json` is a sanitized fixture that
captures the minimum ledger shape:

- one buy paper fill;
- one sell paper fill;
- flat final position;
- one closed trade;
- no live account, portfolio, token, or broker-native order id.

The fixture is intentionally synthetic. It is not a broker report and must not
be treated as market evidence.

## Acceptance for M4-3r-a

M4-3r-a is accepted when:

- ALOR paper/replay/ledger source inventory is complete;
- trade ledger semantics are documented;
- risk-gate store semantics are documented;
- runtime state restore/warmup is documented;
- paper fill policy boundary is documented;
- sanitized fixture exists;
- evidence script validates source markers and fixture shape;
- live/order boundary remains closed.

Next stage: M4-3r-b broker-neutral paper domain model.
