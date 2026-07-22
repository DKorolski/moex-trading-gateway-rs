# Stage 5E-a lifecycle/event-time attachment plan

Status: review candidate, design/inventory-only.

Stage 5E-a is the first controlled step after the accepted Stage 5D aggregate
closure r2. It does not open Redis, FINAM transport, dispatch, runtime-live or
broker execution. Its purpose is to freeze the lifecycle and event-time contract
that later Stage 5E implementation slices must satisfy before any strategy
callback can run after restore.

## Source boundary

Stage 5E-a starts from the accepted Stage 5D aggregate closure r2 source:

```text
9ebbfd29d0346be5149dac746225866f0c8d0257
```

The accepted Stage 5D policy remains unchanged:

- Stage 5C API freeze stays authoritative for paper-host public lifecycle;
- Stage 5D additive freeze stays authoritative for persistence/restart/recovery
  bridges;
- exact numeric persistence and semantic compatibility ADRs are binding entry
  criteria for Stage 5E/6;
- Stage 5E-a adds no runtime callback implementation and no new execution
  surface.

## Required lifecycle chain

Later Stage 5E implementation must attach the strategy only behind this exact
ordered chain:

```text
validated broker truth
 -> runtime state restore
 -> bootstrap notification
 -> restored-state notification
 -> canonical history warmup
 -> pending stream recovery
 -> first eligible strategy callback
```

No callback may occur if any earlier link is blocked, stale, mismatched or
missing.

## Event-time contract

The first eligible callback must be driven by an accepted Stage 3 canonical
final M10 bar whose event time is strictly later than all relevant restore,
bootstrap, warmup and recovery watermarks.

The callback bar must be the first fresh semantic bar after the completed
pre-callback lifecycle. Historical warmup bars and replay/recovery artifacts are
not eligible to produce new trading intents.

## Mandatory gates for later implementation

Stage 5E implementation is not allowed until these conditions are represented
by executable checks:

- callback is impossible before validated broker truth and accepted Stage 5D
  restore;
- canonical final M10 is required; non-final, non-M10 and wrong-instrument bars
  are rejected before state mutation;
- event time is monotonic across restore, bootstrap, history warmup, recovery
  and first callback;
- warmup sufficiency is explicit and source-compatible;
- reconnect/gap proof is accepted before the first fresh bar is allowed;
- session/day rollover and weekend/clearing policy are explicit;
- a blocked Stage 4/Stage 5D report produces zero strategy callbacks;
- replay/restart from the same envelope and same event stream is
  deterministic;
- exact numeric persistence and semantic compatibility ADRs are enforced at the
  Stage 5E entry boundary.

## Closed surfaces

Stage 5E-a keeps these surfaces closed:

```text
Redis integration              closed
FINAM API / WebSocket send      closed
broker transport dispatch       closed
runtime-live                    closed
broker execution                closed
strategy intent sink            closed
autonomous event loop           closed
```

The next acceptable development slice is a no-I/O type-state/inventory
hardening step that turns this design contract into compile-time and fixture
checks without enabling sends or continuous runtime-live.
