# Stage 5G-e-b R1 — exact-replay session rebind

Status: implementation review candidate.

Base commit: `cbe4044bbca8303a7852d225364ec5cf89f02386`.

## Continuous replay chain

`Stage5gExactReplayCheckpoint` now owns the exact pre-replay checkpoint,
committed replay checkpoint, canonical exact evidence and prior continuation
watermark. `apply_stage5g_exact_replay_to_session` consumes this proof and the
live Stage 5G-c session.

It first proves that the live session replay equals the pre-replay checkpoint,
then transfers the owned canonical evidence to the accepted Stage 5G-c core.
The exact branch may change only local sequence and duplicate count. Complete
applied replay must equal the already committed classifier projection.

The result owns the synchronized session and the same committed checkpoint, so
the next NewPackage can be classified and applied without out-of-band restore.

## Crash and validation rules

If the exact checkpoint is persisted and the process stops before session
synchronization, that persisted checkpoint remains valid; later clean-process
restore is deferred to the next Stage 5G-e slice. Without a crash, the proof is
consumed to synchronize the live session.

All newly exposed exact and NewPackage checkpoints pass hard runtime
`validate_stage5g_timer_checkpoint` checks. Release builds do not rely on
`debug_assert` for persistence authority.

## Executable coverage

- ExactReplay → synchronized session → NewPackage;
- two ExactReplay packages → synchronized session → NewPackage;
- broker slots, identity ledger, receipt watermarks and callback count remain
  unchanged during exact synchronization;
- stale session blocks with its exact original capability and pre-replay
  checkpoint;
- crash-after-persist checkpoint remains valid;
- inherited seven Stage 5G-e-b NewPackage tests remain green;
- compile-fail witnesses prove exact proof single consumption and absence of a
  newer checkpoint on stale-session block.

## Closed surfaces

Canonical Stage 5D clean-process restore, GRST01–GRST12, Stage 5G-f, Redis live
consumer/groups, FINAM transport, HTTP POST/DELETE, broker execution,
runtime-live, real orders, Stage 6, main merge and deployment remain closed.
