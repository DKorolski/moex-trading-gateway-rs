# Stage 8B-P1-e I1 supervisor foundation R2 review boundary

Status: source correction candidate for the residual Foundation F01 finding;
not I1 acceptance and not operational activation.

Direct reviewed predecessor: `d19891092a1afb78905875b57e1d4b0c33a2a1e0`.
The accepted I0/R10/P1-d2/d3/d4 baselines and the accepted F02 Redis-atomic
cleanup semantics are unchanged.

## Fatal owner-loss precedence

The coordinator now distinguishes two concepts:

- the first-wins `Stage8bP1eShutdownIntentV1`, retained exclusively as the
  initiating cause/deadline/request-sequence diagnostics;
- `Stage8bP1eTerminalFailureV1`, which describes a later fatal process outcome.

`OwnerPanicked` and `OwnerReturnedWithoutOwner` are terminal `OwnerLost`
outcomes. They always produce exit 70, Degraded readiness, and no bounded
drain. An earlier ExternalSignal or TelemetryFailure remains visible as the
retained initiating cause and is never rewritten to manufacture the exit.

An observed owner-loss event takes precedence over a grace deadline that has
elapsed but has not yet been consumed as a `GraceExpired` event. If
`GraceExpired` is consumed first, the process completes with 72 and the
coordinator must not receive later events. This event-order rule avoids both a
silent exit 0 after actual owner loss and retroactive changes to an already
completed process outcome.

Normal ExternalSignal followed by an authenticated boundary remains exit 0.
Telemetry/SignalTask/OwnerReturnedUnexpectedly with an available owner may
still drain to a boundary and preserve 71/73/70 respectively. Redis lifecycle
failure remains an immediate terminal 67 and is represented separately as
`RedisLifecycle`.

## Executable coverage

Sequence tests prove:

- ExternalSignal → OwnerPanicked = 70 with original diagnostics;
- ExternalSignal → OwnerReturnedWithoutOwner = 70;
- TelemetryFailed → OwnerPanicked = 70 with TelemetryFailure diagnostics;
- owner loss without a previous request records OwnerFailure and exits 70;
- owner loss observed after an unprocessed elapsed deadline still exits 70;
- ExternalSignal → authenticated boundary remains 0;
- explicit GraceExpired remains 72;
- F02 cleanup and its exact PEL race test remain in the inherited R1 gate.

No owner loop, schedule facade/publisher, Redis DB0/VPS activation, DB15
schedule activation, private-key installation, FINAM POST/DELETE, broker
dispatch, runtime-live, or real-order surface is opened by this correction.
