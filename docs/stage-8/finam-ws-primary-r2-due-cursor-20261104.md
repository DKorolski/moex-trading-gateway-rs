# FINAM WS-primary R2 — remaining R3/P2 / 2026-11-04

Reviewed R1 **tree**, not commit: `f3466825d72257829368d593b0d0e55849d71c7b`.
R1/P1, R2/P1 and R4/P2 were closed by that review. Its overall verdict remained
SOURCE HOLD for the IO/readiness/timer race. Exact R2 pins, narrow R1-to-R2 diff
and full extracted-source reproduction are supplied externally in this package.
This document does not declare SOURCE ACCEPT or permit installation/activation.

## Timer invariant

An elapsed deadline is not proof that readiness evaluated it. The processor now
keeps a private `(label, evaluated_at_ms)` cursor at each actual `prepare` call.
`next_action_ms()` selects thresholds strictly after that assessment, not after
a new wall-clock read in writer Update. The getter is pure and cannot consume an
action. If evidence IO crosses due, Update still returns that due; the existing
async owner schedules a prompt tick (minimum 1 ms). The tick uses current clocks
and all existing live/continuity/Lmax fences. No additional DATA/HTTP completion
is necessary. Advancing to a new label discards the previous label's cursor.

This covers nominal grace, proof-request wait, actual boundary+grace and the live
fence, not merely another special case for `Tick::Waiting`. If a tick finds no
boundary or an already-pending proof, its assessment consumes elapsed thresholds
and the next wake is future: no unconditional replay loop of past timers.
Clock validation, grace, exception wait, Lmax and uncertainty values are unchanged.

## Executable regressions

- `r3_io_crossing_keeps_due_until_the_next_readiness_evaluation` first failed on
  the R1 timer implementation: it returned C+9901 instead of unprocessed C+4100.
  It covers equality and a few milliseconds after due, then exactly one WS step.
- `r3_io_crossing_inside_managed_writer_preserves_due_without_external_wakeup`
  drives the actual managed socket/coordinator/blocking-writer/model. One GET is
  issued at C+3000, boundary arrives C+3100, a successful source-admitted response
  is evaluated at C+4099. After the actual durable waiting record, a one-shot
  per-owner **cfg(test)-only** IO seam advances both wall and monotonic samples
  within that same blocking command. It records before/after, so Update genuinely
  sees a different sample from readiness; this is not a fixed command-wide clock.
  The caller synchronizes its own test sample before subsequent commands. No
  provider timestamps or production clocks/config are overwritten.
- MATCH cases advance 1/2/5 ms to C+4100/4101/4104: no further DATA or HTTP result,
  one GET, one WS_LIVE model step/commit, at most one fill, no recovery. MISMATCH
  and STOP at C+4101 produce no callback. Advancing 5802 ms to C+9901 crosses the
  unchanged uncertainty-aware Lmax: no LIVE and no model callback; unresolved
  recovery remains incomplete and is stopped before a new subscription/GET.
- `r3_processed_thresholds_do_not_spin_without_boundary_or_while_proof_is_pending`
  repeats assessment with one pending ticket: future wake, no second GET/model step.
- The late negative exposed an existing Driver retirement panic: connection STOP
  with an already-latched owner continuity fault fell into `unreachable!`.
  The normal stop arm now returns that original fault, not a clean success/panic.
  `r3_lmax_owner_fault_survives_connection_retirement_without_pump_panic` covers
  this small directly-related negative/drain path. It grants no recovery readiness.

The cfg(test) seam is absent from ordinary library/CLI builds; it is neither a
public time override nor an alternate implementation of the timer calculation.
Existing R1/R2/R4, STOP, recovery, restart, shared-runtime, EOD, privacy, model
parity and from-ZIP CLI regressions remain required. Gate logs include explicit
`R3_IO_CASE` witnesses for all six intra-command variants.

## Scope

No strategy, feedback, close-fill, config/profile, policy, auth or REST proof
admission change. No new framework/supervisor, no VPS, FINAM network, real keys,
Redis/DB0, real orders, installation, Linux release or Git push/merge. Source/local
synthetic EOD tests are not provider/native/cgroup or ALOR operational acceptance.
After independent SOURCE ACCEPT: qualify the accepted Linux tree, then the
previously agreed single bounded operational chain, with separate authorization.
