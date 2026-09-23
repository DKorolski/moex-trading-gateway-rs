# Stage 8B-P1-e I1 process supervision matrix

Status: second correction review candidate for the remaining signed-authority
part of P1-PS05 reported against review target
`e7ae487f9897be297bd9fabcee9ffad302e6dd3e`. P1-PS02 is closed by that
review and is retained unchanged.

## Scope

This slice replaces the former fail-closed `run -> OwnerLoopUnavailable`
placeholder with the production process composition already authorized by the
accepted library boundary. `run` now starts the coordinator, shared latch and
SIGTERM/SIGINT supervision before it spawns startup work. The sole production
owner loads the fixed systemd credential, performs authenticated V5 ordinary-run admission,
classifies the exact restart owner, attaches only the
verify-only DB15 Redis surface and continues as the same linear owner.
Admission validates the immutable adopted marker, receipt V2, canonical root
identity, deployment/config/provenance bindings and the current authenticated
restart package. It rejects both authority temp files and never repairs or
advances administrative adoption.

The protected supervisor config supplies the expected signed-schedule registry
version and registry-identity SHA-256. Those values are validated before
connection and are not learned from the first schedule envelope.

The external process supervisor and startup/owner task share only the first-wins
shutdown latch. They do not clone the Redis lifecycle owner. A committed
Cancel completion is consumed with `into_ready_polling()` and the same owner
continues canonical M10 polling. Every restart-only typed boundary returns a
nonzero process result; only an authenticated owner boundary reached after an
external signal may return exit code 0. A signal won before admission exits
66, while an admitted stop before Redis attachment may exit 0.

## Stable exit classes

| Class | Meaning |
| --- | --- |
| 0 | authenticated SIGTERM/SIGINT shutdown boundary |
| 64 | argv/config/boot identity/systemd credential |
| 66 | durable restart or Redis deployment identity |
| 67 | Redis attach/schedule reader/owner lifecycle |
| 70 | owner task panic or ownerless return |
| 72 | bounded shutdown grace expired |
| 73 | signal task failed |

The binary reports `run-stopped` only for the exit-0 boundary. A panic,
restart-required lifecycle result or unexpected owner return cannot be
converted into successful daemon termination.

## Executable OS-process matrix

All process witnesses use real child PIDs and kernel signals. They invoke the
same production owner/supervision functions used by `run`; test-only code is
limited to isolated Redis, deterministic durable fixtures, schedule trust
material and observable startup barriers. No operational Redis or FINAM
surface is contacted.

0. Production startup supervision: SIGTERM before admission exits 66 and
   cannot impersonate an authenticated stop. SIGTERM after exact V5 admission,
   SIGINT before Redis attach and SIGTERM before S06 acquisition observe one
   already-running grace deadline and exit 0 at the authenticated boundary.
   Separate in-flight controls place a byte-transparent RESP proxy in front of
   the same isolated Redis. The proxy writes its marker only after Redis has
   processed and returned the exact deployment-manifest `GET` or exact
   canonical-M10/group `XPENDING`; it then withholds that server-processed Redis response.
   SIGTERM/SIGINT therefore cancels the unmodified production
   startup select while a concrete reply is in flight. Both cases stop inside
   grace with exit 0, compare durable bytes and a full Redis DUMP/PTTL snapshot,
   verify an empty PEL and prove ordinary re-admission with one owner. Exit 72
   is tested separately by a genuinely noncooperative owner task passed to the
   same supervisor; it is not represented as a network-in-flight outcome.

1. Idle SIGTERM: after signal handlers and the sole owner are live, SIGTERM
   latches shutdown, the bounded Redis wait returns a retained Ready owner,
   the child exits 0, durable bytes remain unchanged and restart is exactly
   Ready with an empty PEL.
2. Idle SIGKILL/restart: the first child is reaped with signal 9; durable bytes
   remain unchanged; a second child acquires the same root and exits 0 only
   after SIGTERM. There is no second writer or owner.
3. Owner panic: a real child owner task panics and the external supervisor
   exits with exact class 70, never 0.
4. Committed Cancel handoff: the child executes ACK/truth/XACK-last, consumes
   the typed completion, acquires the next canonical M10 and remains alive
   until SIGTERM. Restart cannot return the completed Cancel truth and retains
   one terminal allocation.
5. Target-first Cancel truth-before-XACK: process A reaches the typed
   restart-required boundary and exits 67. Process B durably publishes an
   fsynced marker after replacement Cancel truth and before source XACK; the
   parent verifies the original M10 is still pending, sends SIGKILL and reaps
   signal 9. Authenticated restart returns exact `P1d3TruthCommitted`.
   Process C performs XACK-last, consumes the completion, acquires the next
   canonical M10 and exits 0 after SIGTERM. Final restart cannot replay the
   old truth and preserves exactly one target-truth plus recovered-Cancel
   sequence chain.

The final case is the process-level counterpart of the accepted P1-d3/P1-d4
truth-before-XACK rule. The crash marker is observable before the kill, and
the PEL and durable restart type are independently checked on both sides of
the crash.

Wrapper-level executable controls additionally preserve coordinator terminal
classes: authenticated stop before deadline is 0, at/after deadline is 72,
signal-channel failure with a clean authenticated stop is 73, restart-required
is 67, and an owner panic retains fatal class 70 even when a signal or signal
task failure initiated shutdown. An `AuthenticatedStop` returned without any
shutdown intent is an ownerless fatal return and exits 70, never grace-expired
72.

## Corrected V5 predecessor lifecycle

Fresh V5 first boot and historical pre-seal continuation now both export the
existing zero-effect `Stage5gCleanRestartSource::P1BootstrapReady`. The sealed
package therefore authenticates as `P1SemanticReady`, with callback count one,
no semantic commit, no pending request and an empty journal. Adoption predicate
version 2 and its derived owner/receipt digests bind that exact shape.

The positive production-path regression starts from a freshly adopted V5 root,
passes ordinary-run admission and continuously executes a decision M10 plus its
successor through the real Hybrid callback and Market command publication. It
then appends a fixture-signed envelope to the isolated schedule stream, reads it
through the production schedule reader, commits and rereads the exact V4
predecessor/successor/request/command/publication-seal binding, and crashes at
that pre-effect frontier. Authenticated restart returns
`P1eScheduleBindingCommitted`; the retained V4 authority reclaims the exact PEL
entry, revalidates the immutable command marker and successor, passes latches E
and F, and only then reaches the paper provider, replacement truth and source
XACK-last. It performs no Redis reset, no test intent injection and no legacy
`v4_proof: None` schedule-authority construction.

Direct counters prove one callback, one publication attempt/success, one signed
schedule read, one claim, one provider effect and one XACK. The committed-V4
continuation performs zero additional schedule reads. Durable journal counters,
the immutable initial marker/receipt and the cumulative two-M10/one-command
Redis inventory are captured before and after restart. Post-truth/XACK restart
returns the exact P1-d2 truth frontier; its `AlreadyAcknowledged` continuation
performs no second callback, provider invocation, publication, schedule read or
XACK.

A fresh flat paper V5 does not autonomously create a working LIMIT and therefore
cannot legitimately emit Cancel in this witness. Cancel enters the process by
its intended production ingress: an authenticated durable restart outcome with
the exact retained Redis source and signed schedule authority. The committed
Cancel and truth-before-XACK process cases above exercise that ingress. The
legacy helper that constructs a LIMIT/Cancel package through test-only intent
seams and recreates Redis is retained only as an explicitly named isolated
integration fixture; it is not cited as continuous V5 evidence.

Existing predicate-v1 `TimerReady` V5 artifacts are not reinterpreted,
rewritten or deleted. They fail exact receipt/adoption validation; any future
migration requires a separate authenticated administrative authorization.

## Deliberately closed

- operational Redis DB0/DB15 activation and VPS installation;
- paper-provider operational activation;
- FINAM POST/DELETE/send and broker dispatch;
- runtime-live and real orders.

This source slice is not aggregate I1 acceptance. Fixed-path installation,
systemd material, telemetry composition and the remaining aggregate I1 gates
continue as separate review boundaries.
