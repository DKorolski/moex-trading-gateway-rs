# Stage 5G-d R1-a — deterministic bar continuation authority

Status: review candidate. Base: `bc4cabfff42eafee48733296f121a8a6e2f42dd8`.

This narrow authority extension fixes the clock and ordering boundary needed by
Stage 5G-d without changing its wrapper yet. Stage 5G-d R1-b, Stage 5G-e and
Stage 5G-f remain closed.

## Clock policy

- Timer checkpoint: explicit `Stage5cPaperTimerInput.now_ts_utc_ms`.
- Bar checkpoint: the accepted semantic bar's canonical close time multiplied
  by 1,000 with checked arithmetic.
- Bar evaluation time: explicit deterministic event time supplied by the
  owning continuation boundary.
- Process wall clock is not consulted by the new authority.

The accepted bar capability owns its checkpoint. A caller cannot supply a bar
checkpoint independently of that capability. Equal or reversed bar/timer
checkpoints and timestamp overflow are retryable blocks before the existing
Stage 5C callback. Every pre-callback block returns the exact incoming timer
settlement.

## Scope

The extension is crate-private, additive and marker/digest pinned. It delegates
successful execution to the existing Stage 5C next-bar callback path. It adds
no callback, scheduler, I/O, Redis, FINAM, HTTP, broker dispatch or live path.
The normalized Stage 5C public API is unchanged.

R1-b may consume this authority only after independent acceptance. R1-b owns
the separate work to preserve continuation checkpoints through ACK, broker
convergence and bar routes and to harden serialized checkpoint validation.
