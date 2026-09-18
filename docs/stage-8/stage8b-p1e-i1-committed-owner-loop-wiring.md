# Stage 8B-P1-e I1 committed owner-loop wiring

Status: source review candidate based on accepted committed-restart source
`efe56a9af6272f13b2d87f4ad14a2709c605cd42`.

## Scope

This slice wires only authenticated committed Cancel and Day-expiry V4 restart
owners into the production owner-loop composition. It does not activate Redis
DB0/VPS, a paper provider, FINAM transport, broker dispatch, runtime-live or
real orders.

Committed Cancel is routed exhaustively through ACK, truth and target-first
continuation owners. Target-first stops at an explicit restart-required
boundary. After truth, the exact source is XACKed last and the loop returns a
typed `CommittedCancelResolved` result instead of re-entering fresh schedule
admission. A repeated restart after a lost final-XACK response returns
`AlreadyAcknowledged` without another XACK or sequence allocation.

Committed Day-expiry first proves that the canonical M10 PEL is empty. It then
executes the source-free durable continuation and returns a typed `CommittedDayExpiryResolved`
owner. It cannot claim or acknowledge an M10 and
cannot enter a fresh schedule read at this boundary.

## Executable evidence

Real-Redis owner-loop tests cover:

- direct committed Cancel ACK -> truth -> XACK-last;
- target-first Cancel -> restart-required -> recovered truth -> XACK-last;
- restart after final-XACK response loss with `AlreadyAcknowledged`;
- exact request-scoped sequence-allocation preservation across replay;
- direct provider/callback/publication/XAUTOCLAIM/XACK/schedule-read invocation
  counters;
- source-free Day-expiry with zero claim, XACK, schedule-read, publication,
  callback and provider effects;
- Day-expiry's one autonomous terminal-truth allocation with no ACK sequence;
- fail-closed Day-expiry when the canonical M10 PEL is non-empty;
- restored signed-schedule high-water without a new schedule read.

The terminal typed outcomes deliberately carry no fresh schedule-polling
authority. This enforces the accepted rule: an already committed V4 does not return to fresh schedule admission in the same recovery invocation.

## Evidence discipline correction

The handoff gate for this slice must retain actual command logs and exit codes.
No full-suite count may be hardcoded as execution evidence. A count is either
parsed from a retained successful run or explicitly labelled
developer-reported and excluded from acceptance.

The runtime package records the complete lib, Redis subprocess, writer-lock
subprocess and doctest targets as separate invocations. This keeps the inherited
ten-minute P1-d4 crash matrix isolated from the timing-sensitive X16 child
barrier while preserving every target's unabridged output and exit status.

## Deliberately closed

- OS-process SIGTERM/panic/SIGKILL/restart matrix; this is the next source and
  evidence slice;
- operational Redis DB0/DB15 and VPS activation;
- paper-provider execution;
- FINAM POST/DELETE/send and broker dispatch;
- runtime-live and real orders.

No deployment or activation decision is implied by this source candidate.
