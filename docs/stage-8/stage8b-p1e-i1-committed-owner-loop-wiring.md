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
typed `CommittedCancelResolved` result. That completion retains the sole Ready
owner and exposes an explicit consuming handoff back to Ready polling. The
completed V4 is not admitted again, while a strictly newer exact canonical M10
may advance from the durable semantic watermark. A repeated restart after a
lost final-XACK response returns `AlreadyAcknowledged` without another XACK or
sequence allocation, then the same consuming handoff reaches the next
canonical M10.

Committed Day-expiry first proves that the canonical M10 PEL is empty. It then
executes the source-free durable continuation and returns a typed `CommittedDayExpiryResolved`
owner. It cannot claim or acknowledge an M10 and
cannot enter a fresh schedule read at this boundary.

## Executable evidence

Real-Redis owner-loop tests cover:

- direct committed Cancel ACK -> truth -> XACK-last;
- target-first Cancel -> restart-required -> recovered truth -> XACK-last;
- an injected XACK response loss after Redis accepted the command, followed by
  restart and `AlreadyAcknowledged`;
- explicit owner handoff and next-canonical-M10 progress after fresh Cancel,
  committed Cancel and target-first response-loss replay;
- exact request-scoped sequence-allocation preservation across replay;
- direct provider/callback/publication/XAUTOCLAIM/XACK/schedule-read counters;
  publication and XACK each expose separate attempt and success counters around
  their real Redis transport calls;
- a real generic Cancel publication positive control (`attempt=1`, `success=1`)
  and transport-error control (`attempt=1`, `success=0`);
- source-free Day-expiry with zero claim, XACK, schedule-read, publication,
  callback and provider effects;
- Day-expiry's one autonomous terminal-truth allocation with no ACK sequence;
- fail-closed Day-expiry when the canonical M10 PEL is non-empty;
- restored signed-schedule high-water without a new schedule read.

The typed Cancel completion deliberately prevents implicit polling: the caller
must consume `into_ready_polling`. The move clears the transient completion
marker and transfers the same authenticated owner, so the old committed V4
does not return to fresh schedule admission while the next canonical M10 can
progress. Day-expiry remains a terminal source-free outcome and carries no
fresh schedule-polling authority.

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
