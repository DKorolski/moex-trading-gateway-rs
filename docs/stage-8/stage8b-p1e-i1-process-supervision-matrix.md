# Stage 8B-P1-e I1 process supervision matrix

Status: correction review candidate for findings P1-PS01, P1-PS02 and
P2-PS03 reported against `9e84b06e7440b5bcfe7f5cddf81ac60c2676bf76`.

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
   SIGINT during Redis attach and SIGTERM during delayed S06 acquisition all
   observe one already-running grace deadline and exit 0 at the authenticated
   boundary. Every case compares durable file bytes and a full isolated Redis
   DUMP/PTTL snapshot before and after, then proves a fresh exact V5 admission.
   There is no callback, publication, PEL mutation or XACK.

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
task failure initiated shutdown.

## Discovered predecessor mismatch

The requested positive witness "the same adopted V5 root after real
M10/Cancel/V4 advancement" cannot currently be constructed without changing
an accepted predecessor contract. The accepted V5 transaction exports
`Stage5gCleanRestartSource::TimerReady`, while the production P1 semantic
transition accepts only lifecycle `P1SemanticReady`; the older P1 bootstrap
obtained that lifecycle through `P1BootstrapReady`. A direct production-path
attempt from a freshly adopted V5 root therefore fails closed with
`RestartRuntimeRequired` before the first M10 can advance.

This accepted V5 lifecycle mismatch is recorded rather than hidden behind a
synthetic package or a legacy fixture. Resolving it requires a separately
reviewed predecessor decision: either migrate V5's initial package to the
already-established P1 bootstrap lifecycle or add an authenticated one-time
TimerReady-to-P1SemanticReady transition. This correction does not silently
rewrite the accepted V5 transaction, receipt or adoption predicate.

## Deliberately closed

- operational Redis DB0/DB15 activation and VPS installation;
- paper-provider operational activation;
- FINAM POST/DELETE/send and broker dispatch;
- runtime-live and real orders.

This source slice is not aggregate I1 acceptance. The predecessor lifecycle
decision and its real M10/Cancel/V4 witness remain required. Fixed-path
installation, systemd material, telemetry composition and the remaining
aggregate I1 gates continue as separate review boundaries.
