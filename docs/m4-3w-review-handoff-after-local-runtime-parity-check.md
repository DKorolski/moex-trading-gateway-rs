# M4-3w — review handoff after local FINAM paper-runtime parity check

Status: handoff summary / no live order boundary expansion.

This note summarizes the work completed since the previous review package and
records the next planned steps.

## What changed since the previous review

### M4-3s — ALOR-compatible hybrid runtime-state projection

`PaperRuntimeState` now contains an optional `hybrid_intraday` projection. It
exposes ALOR-comparable fields such as:

- `strategy_kind`;
- `active_cycle_id`;
- `last_position_qty`;
- `current_owner`;
- `current_side`;
- pending entry/exit placeholders;
- TP/SL placeholders;
- day feature fields;
- riskgate summary placeholders;
- `projection_source`;
- `strategy_invocation_enabled`.

At this stage, fields derived from the paper ledger and closed M10 bar are
filled. Fields requiring the real hybrid orchestrator remain explicit
placeholders.

### M4-3t — paper-only hybrid strategy shadow boundary

The FINAM paper runtime consumer can enable a paper-only strategy invocation
shadow. It does not emit broker commands. It only marks the strategy boundary as
observed and can set `entry_ready` after a configurable number of complete M10
bars.

Paper intents now have optional strategy tags:

- `strategy_owner`;
- `strategy_cycle_id`.

This lets a future real hybrid orchestrator fill ALOR-like owner/cycle state
without changing the runtime state schema again.

### M4-3u — ALOR gateway/runtime parity notes

The ALOR gateway/runtime documentation and configs were re-read and distilled
into a checklist of invariants that must survive the FINAM migration:

- closed 10m-bar strategy contract;
- broker-truth-first bootstrap;
- runtime state restore and history warmup order;
- live guard/readiness semantics;
- entry-only gates;
- exit/cancel/protective repair passthrough;
- request-id parity;
- blocked-intent rollback;
- event-time semantics;
- silence/gap entry protection;
- action-scoped control-path lessons;
- Redis consumer-group reliability;
- riskgate as model memory;
- weekend/session policy.

### M4-3v — broker-neutral runtime host contract

`broker-core` now has a `runtime_host` module with broker-neutral primitives for
ALOR-compatible runtime hosting:

- lifecycle plan and validator;
- runtime intent classes;
- blocked-intent disposition;
- command-prepared seam;
- monotonic event-time clock;
- target-symbol scoped bootstrap snapshot from `BrokerTruthSnapshot`;
- runtime live guard helper with close-only passthrough when blocked and an open
  target position exists.

### Local FINAM paper-runtime smoke

Local FINAM paper/shadow contour was raised manually:

```text
FINAM WS M1 bars -> paper runtime consumer -> canonical M10 -> PaperRuntimeState
```

Observed result:

```text
last_bar_key = 2026-07-06T18:30:00+00:00/600
last_bar_close = 2185.5
current_day_close = 2185.5
entry_ready = true
paper_only = true
```

The corresponding ALOR oracle runtime state had the same current close for that
bar, while position/owner/riskgate/day-range fields differed as expected because
the FINAM contour is still paper-only, starts without full-day warmup, and does
not yet run the real hybrid orchestrator or riskgate ledger.

The local FINAM loops were then stopped intentionally. Final local readiness was
`Stopped / OperatorPaused`.

## Boundary status

Still disabled:

```text
real FINAM live orders
command-consumer-to-real-FINAM
runtime LiveReady
stop/SLTP/bracket
replace/multileg
```

The work only moves paper/shadow runtime parity forward.

## How much closer the contours are

The contours are materially closer at the operational contract layer:

- FINAM can now produce fresh live M1 bars, derive closed M10 bars, and publish a
  paper runtime state with ALOR-like `hybrid_intraday` shape.
- FINAM paper runtime now uses Redis consumer-group processing with XACK after
  success/DLQ discipline, matching the ALOR reliability pattern.
- The runtime host lifecycle and intent safety contracts are now explicit in
  broker-neutral code.
- We can now compare ALOR oracle and FINAM paper runtime state on the same
  closed M10 timestamp.

Remaining gap:

- FINAM is not yet running the real hybrid BO/MR orchestrator;
- FINAM is not yet warmed from full session history like the ALOR runtime;
- FINAM riskgate ledger/state is not yet attached;
- FINAM broker truth bootstrap is not yet wired into the paper runtime state;
- FINAM does not yet emit ALOR-compatible strategy decisions/owner/cycle
  transitions.

So the contours are close on transport, bar finality, Redis lifecycle, and state
shape. They are not yet close enough on strategy semantics or riskgate state to
promote beyond paper/shadow.

## Recommended next steps

1. Deploy the FINAM paper/shadow contour on VPS before session open:
   - FINAM WS M1 bars;
   - paper runtime consumer;
   - no live order boundary.
2. Let it run from session open to collect a clean full-day high/low/close
   projection.
3. Add a FINAM-to-ALOR-runtime compatibility bridge if we want to run the
   existing ALOR `strategy-runtime` binary directly against FINAM-derived M10
   bars.
4. Wire canonical broker truth bootstrap into the FINAM paper runtime.
5. Add riskgate ledger/state projection.
6. Attach or port the real hybrid BO/MR orchestrator behind the paper-only
   boundary.
7. Produce field-by-field ALOR oracle vs FINAM paper runtime comparison report.

Only after those steps should we discuss any runtime-live or real-order
expansion.
