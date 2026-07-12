# Stage 5C-d — canonical history warmup facade

Status: review candidate.

Date: 2026-07-12.

## Boundary

```text
Stage5cRuntimeStateRestoredPaperStrategy
  -> freshness and lifecycle timestamp checks
  -> opaque Stage3-gated accepted history batch
  -> future/representable timestamp validation
  -> warmup_from_history
  -> Stage5cWarmedPaperStrategy
```

The restored type-state is consumed by value. Warmup uses the same owned
strategy and the same admission/bootstrap/restore receipt chain. Before the
callback, broker-truth evidence must still be fresh and timestamps must satisfy:

```text
checked <= issued <= bootstrap notified <= state restored <= warmup started
```

Raw bars are first converted into an opaque `Stage5cAcceptedHistoryBatch` only
when `evaluate_stage3_strategy_input_gate` accepts every bar and its shared
`Stage3StrategyBarProvenance`. FINAM requires derived M1-to-M10, source TF 60,
target TF 600, complete aggregation and proven gap absence. Raw FINAM M1 and
native FINAM M10 pending characterization are rejected.

Only strictly chronological, unique, timestamp-aligned, final M10 bars for the
exact target `InstrumentId` are accepted. No close timestamp may be in the
future relative to warmup, and every timestamp must be representable in UTC.
Origin must be `History`; OHLC values
must be finite and structurally valid, and volume must be finite/non-negative.
Raw M1, forming, replay/live/gap bars and malformed history fail closed before
warmup mutation.

The callback context remains paper-only with live orders disabled and gateway
phase `SyncingHistory`. A successful transition records the processed count but
does not attach a host or intent sink. The receipt distinguishes input,
processed and session-skipped bars and records the accepted Stage 3 source mode.

## Still closed

- pending-stream recovery;
- semantic bars and timers;
- paper intent sink;
- command consumer and all broker sends;
- runtime-live and real POST/DELETE;
- broker-side Stop/SLTP/bracket execution.
