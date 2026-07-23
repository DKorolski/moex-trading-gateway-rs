# Stage 5E-b — no-I/O lifecycle capability

Status: implementation foundation.

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
 -> Stage 5E-b first-fresh-live eligibility capability
```

The first slice only admits an observation-only first fresh live bar.  It does
not call the strategy. A later separately-reviewed slice may consume that
capability at the callback boundary.

## Invariants

- lifecycle timestamps are checked only against lifecycle timestamps;
- market bar close timestamps are checked only against market bar close
  timestamps;
- `last_history_bar_close < first_fresh_live_bar_close` is strict;
- recovery completion is represented by an opaque causal receipt, not by a
  comparison between recovery wall-clock time and market time;
- the ready capability is linear: successful first-bar admission consumes it,
  so the same recovered state cannot be admitted twice;
- replay, broker execution, intent dispatch, Redis, FINAM and runtime-live
  remain unavailable.

## Explicitly deferred

- invoking `on_broker_bar`;
- converting an eligible bar into an intent batch;
- session, clearing and calendar policy wiring;
- real stream reads, recovery workers and subscriptions.

Those features need their own scope and review after this capability is
accepted.

## Handoff descriptor

Stage 5E-b uses its own inventory, plan and checker in a handoff. The archive
must bind those three selected files to the Stage 5E gate result and must use
the 5E-b baseline, not the historical 5E-a baseline. Archive safety rejects a
mixed 5E-a/5E-b descriptor set.
