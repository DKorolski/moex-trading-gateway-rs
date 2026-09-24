# Stage 8B-P1-e I1 production telemetry composition

Status: **SOURCE CORRECTION REVIEW CANDIDATE — I1 NOT CLOSED**.

Accepted predecessor: aggregate-readiness governance boundary
`896ad1b2f85ea47a59212001eb713befaea26832`. The production process baseline
within that boundary remains `1086b8d95e10514532d1c25c57956eca943b732c`.

This slice composes the already accepted telemetry DTOs, readiness classifier
and Redis write primitive into the sole P1-e production owner. It does not add
deployment material and does not authorize operational Redis, a VPS service,
paper-provider activation, FINAM transport, broker dispatch, runtime-live or
real orders.

## Composition boundary

Telemetry starts only after all of the following have succeeded:

1. protected config and lifecycle credential validation;
2. authenticated ordinary-run durable admission;
3. verify-only Redis namespace/manifest/group attachment;
4. stale zero-pending consumer hygiene;
5. S06 pending acquisition and claim scan;
6. schedule-reader attachment.

The initial snapshot binds the real boot ID, operational identity, deployment
generation, runtime-config fingerprint, current durable seal generation and
commitment, fresh consumer name and actual PEL count. No placeholder identity
or reconstructed seal is accepted.

The owner has a reporter only. The telemetry task receives a separate
write-only `Stage8bP1eTelemetryPublisherV1` derived from the already verified
control plane. That capability exposes only health/readiness publication; it
has no source read, claim, command publication, consumer cleanup or XACK
authority. There remains one lifecycle owner.

## State and publication contract

The task publishes immediately, on each distinct transition and periodically
at the validated `health_interval_ms`. Tokio missed ticks are skipped rather
than replayed. Transition delivery uses a bounded FIFO channel of 32 snapshots.
Channel overflow, channel/state failure, telemetry-task completion/panic or
Redis publication failure requests the first-wins `TelemetryFailure` shutdown
intent; the task is never restarted inside the process. The common process
supervisor actively observes that retained intent even when no OS signal
arrives. A child-task guard aborts an armed telemetry task if its parent exits
or unwinds.

The exact externally visible sequence is:

```text
Starting
  -> PaperReady after one successful empty bounded source poll and durable
     readiness revalidation
  -> Degraded while one acquired/recovered lifecycle is unresolved
  -> PaperReady after the exact lifecycle returns to Ready
  -> Draining after the first retained shutdown intent
  -> Stopped only at an authenticated owner boundary
```

A restart-only terminal is `Degraded`, not `Stopped`. Duplicate unchanged
transition snapshots are suppressed; periodic snapshots remain enabled. Each
publication reconciles its copied state with the current shared shutdown latch.
Therefore an old queued Ready transition cannot publish `PaperReady` after the
first retained shutdown intent or move `Draining` back to `Running`.

Source freshness is not inferred from heartbeat activity. A successful owner
poll records an observation time and a freshness deadline of
`max(2 * health_interval, 3000 ms)`. Periodic publication expires that evidence
independently while the telemetry Redis writer remains healthy and publishes a
degraded readiness result until a later successful owner observation.

Each health/readiness pair uses the same observation timestamp and exact
contract domains. Redis mutation remains:

```text
XADD <fixed-stream> NOMKSTREAM MAXLEN = 4096 * payload <canonical-json>
```

Missing streams fail closed and remain absent. The task cannot create or
repair Redis keys. Raw Redis URLs, paths, account IDs, credentials, tokens,
command payloads and raw error text are not serialized. Every production
snapshot fixes `paper_only=true` and all FINAM/broker/runtime-live/real-order
flags to `false`; `LiveReady` does not exist in this contract.

## Failure precedence

The shared shutdown latch remains first-wins. The deadline stored by the first
intent is the only deadline used by the common supervisor, owner drain and
final telemetry settlement; a later OS signal or final publication cannot
extend it. A telemetry failure initiating shutdown reaches exit class 71 only
when the authenticated boundary is reached before that deadline. At or after
the retained deadline it exits 72. A previously retained external
SIGTERM/SIGINT remains a clean authenticated exit if its boundary is reached in
time even if its final best-effort telemetry write fails. An independently
completed owner/Redis error or panic is not masked by a later diagnostic write
failure.

## Durable diagnostic truth

Telemetry obtains read-only diagnostic snapshots from the actual authenticated
owner at Ready, ACK, truth, retained-recovery and terminal boundaries. The
snapshot carries the current durable seal generation and commitment together
with the runtime's last semantic-bar and canonical-ACK timestamps. PEL is read
from Redis at retained and terminal boundaries; an ACK/truth/XACK lifecycle is
therefore represented as PEL `1`, PEL `1`, then PEL `0` with the corresponding
replacement seals. Typed blocked recovery publishes its actual request count
and semantic-batch hash. None of these diagnostic bridges can claim, advance or
XACK source work.

## Evidence

Isolated Redis tests prove:

- ordered Starting/PaperReady/Degraded/Draining/Stopped publication;
- immediate plus bounded periodic publication;
- exact NOMKSTREAM missing-stream failure without implicit creation;
- bounded-channel fail-closed and first-wins behaviour;
- telemetry Redis failure, telemetry-task panic and FIFO overflow without an
  OS signal actively terminate through the common supervisor with exit 71;
- a non-cooperative owner reaches exit 72 at the one retained deadline, and a
  later SIGTERM does not extend it;
- queued Ready cannot override a retained shutdown and source freshness expires
  while the telemetry writer remains live;
- a real child production process reaches PaperReady, receives SIGTERM,
  while its next source-poll response is deliberately withheld, publishes
  Draining before that response is released, never publishes a later
  PaperReady, then publishes Stopped and exits zero;
- a real M10 to signed-V4 to paper-effect to S_ack to S_truth to source-XACK
  chain publishes exact semantic/ACK timestamps, replacement seals and PEL;
- retained S_ack and typed blocked-recovery witnesses expose exact terminal
  snapshot/PEL and blocked inventory;
- the idle process witness leaves the durable root unchanged and leaves the M10
  PEL empty;
- every child-process payload retains all closed-surface flags and contains no
  forbidden raw-field names;
- all inherited process/restart/cancel/day-expiry tests remain green.

The correction boundary also passes the canonical
`RUST_MIN_STACK=33554432` full regressions: runtime library `304 passed / 14
ignored`, Redis subprocess `3 passed / 3 ignored`, writer-lock subprocess `6
passed / 3 ignored`, runtime doctests `61/61`, strategy-runtime-core unit tests
`1283/1283` and its integration/doctest targets. The larger stack is the
pre-existing accepted P1-d4 registry test-runner contract, not a production
runtime setting.

The acceptance matrix is
`stage8b-p1e-i1-telemetry-composition-acceptance-matrix.csv`.

## Next boundary

After independent source acceptance, the next authorized slice is
**fixed-path installation and systemd material** with isolated target-Linux verification.
I1 aggregate acceptance follows only after that slice is independently
accepted. No operational installation or service start is authorized here.
