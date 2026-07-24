# Stage 5E-b — no-I/O lifecycle capability

Status: Stage 5E-b2.1 session classifier hardening, still no-I/O.

Stage 5E-b turns the accepted Stage 5E-a event-time contract into a narrow,
in-process type-state boundary. It deliberately stops before a strategy bar
callback, an intent sink, or any transport attachment.

The current Stage 5D additive freeze also pins `strategy-runtime-core/lib.rs`.
Therefore, before the Rust capability is attached, Stage 5E-b must introduce a
reviewed additive extension point (with its own manifest/checker evidence).
This preserves Stage 5D's accepted source boundary rather than silently
changing it.

## Entry and output

The only entry capability is the already completed Stage 5C pending-recovery
paper state, which is reachable only after the accepted Stage 5D restore and
the Stage 5C chain:

```text
Stage 5D accepted restore
 -> Stage 5C history warmup
 -> Stage 5C pending recovery
 -> Stage 5E-b observed-live-bar-after-history capability
```

The first slice only admits an observation-only live bar after canonical
history. It does not prove a market-data gap and therefore is not named
"first fresh" or callback-ready. It does not call the strategy. A later
separately-reviewed slice may consume a stronger capability at the callback
boundary.

## Invariants

The machine-readable source of truth is
`stage5e-b-no-io-lifecycle-inventory.json.contract_invariants`. This document
is its human-readable projection; a contradictory statement does not extend or
override the JSON contract.

- lifecycle timestamps are checked only against lifecycle timestamps;
- market bar close timestamps are checked only against market bar close
  timestamps;
- `last_history_bar_close < observed_live_bar_close` is strict;
- the target instrument and exact tick-size bits bind the observed bar to the
  recovered admission; admission expiry is checked in lifecycle time, while
  future-bar rejection is checked against lifecycle time;
- recovery completion is represented by an opaque causal receipt, not by a
  comparison between recovery wall-clock time and market time;
- callback count == 0, intent count == 0, and the first live bar is
  observation-only;
- the slice does not call the strategy and does not create an executable
  intent;
- the ready capability is linear: successful first-bar admission consumes it,
  so the same recovered state cannot be admitted twice;
- replay, broker execution, intent dispatch, Redis, FINAM and runtime-live
  remain unavailable.

## Explicitly deferred

- invoking `on_broker_bar`;
- converting an eligible bar into an intent batch;
- calendar policy wiring or inferred exchange timetable;
- real stream reads, recovery workers and subscriptions.

Those features need their own scope and review after this capability is
accepted.

## Handoff descriptor

Stage 5E-b uses its own inventory, plan and checker in a handoff. The archive
must bind those three selected files to the Stage 5E gate result and must use
the 5E-b baseline, not the historical 5E-a baseline. Archive safety rejects a
mixed 5E-a/5E-b descriptor set.

The Stage 5E-b contract is immutable during this slice:

```text
last_history_bar_close < observed_live_bar_close
first slice only admits an observation-only observed live bar after history
and does not prove a market-data gap
callback count == 0
intent count == 0
```

The first slice does not call the strategy and must not create an executable
intent. It does not create an executable intent, attach an intent sink, or open
Redis/FINAM/transport/runtime-live.

## Stage 5E-b1.2 controlled implementation

The reviewed b1 extension is crate-private and linear:

```text
Stage5cPendingRecoveredPaperStrategy + Stage5cAcceptedSemanticBar
  -> crate-private consuming bridge
  -> observation-only Stage5eObservedLiveBarAfterHistory
  | contextual rejection
  -> original recovered state + candidate returned for retry
```

It admits only `HybridRuntimeBarOrigin::Live` and requires
`last_history_bar_close < observed_live_bar_close`, matching instrument,
matching exact tick size, an unexpired admission, and a non-future market bar.
The resulting capability records zero callbacks and zero intents; it deliberately
exposes no public re-export and no continuation into strategy execution. The
recovery receipt is causal ownership only; it is never compared to market-bar
time. Lifecycle time is read at the admission attempt, not retained in a
capability; the deterministic clock seam exists only under `cfg(test)`. A
rejected candidate returns the original recovered state and accepted candidate
unchanged, so a caller may supply a later candidate without rebuilding
recovery. The Stage 5D additive freeze permits the module declaration and
consuming bridge only in its already reviewed crate-private additive regions.

## Stage 5E-b2 observed session eligibility

Stage 5E-b2 adds a separate observation-only receipt for session eligibility.
It consumes neither the recovered strategy nor the observed-bar capability,
and cannot invoke a callback. It takes the existing broker-neutral
`BrokerMarketSessionState` plus the existing Stage 4 schedule-freshness probe
and an explicit broker-observed open interval for the candidate bar.

Only a fresh `Open` state and a bar close inside a valid observed interval are
accepted. `Break` (clearing), `Maintenance`, `Closed`, `Unknown`, unavailable
or stale schedule evidence, and invalid or out-of-window intervals block. The
receipt has callback count == 0 and intent count == 0, does not call the
strategy, and does not create an executable intent.

The observed interval is closed: `open_from_bar_close <= bar_close <=
open_until_bar_close`. `open_until_bar_close` therefore denotes the final
allowed bar close, not the instant at which clearing begins. The receipt is
linear (it implements neither `Clone` nor `Copy`), and its definition,
constructor and zero-side-effect methods form one hash-pinned construction
surface.

This is deliberately not a calendar engine and does not claim market-gap
proof, first-fresh status, callback readiness, or a continuation into strategy
execution. A future separately reviewed bridge must require both the live-bar
and session receipts before it can discuss a callback boundary.

## Stage 5E-b2.1 construction-surface hardening

The b2 session classifier is now explicitly linear: its receipt has no
`Clone`, `Copy`, `Default`, serialization or alternate-constructor surface.
The receipt definition, only checked construction, and proof methods are in
the single hash-pinned session region. The negative harness protects field
visibility, copyability and a forged default receipt. The classifier remains
only a local classification result; it is not yet trusted schedule evidence
and cannot authorize a callback. The next binding slice must introduce the
accepted schedule mapper and exact Stage 4/bar binding rather than widening
this classifier's caller-supplied inputs.
