# Stage 8B-P1-e I1 production telemetry composition

Status: **SOURCE REVIEW CANDIDATE — I1 NOT CLOSED**.

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
Channel overflow, channel/state failure or Redis publication failure requests
the first-wins `TelemetryFailure` shutdown intent; the task is never restarted
inside the process.

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
transition snapshots are suppressed; periodic snapshots remain enabled.

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

The shared shutdown latch remains first-wins. A telemetry failure initiating
shutdown reaches exit class 71 after bounded owner drain. A previously retained
external SIGTERM/SIGINT remains a clean authenticated exit even if its final
best-effort telemetry write fails. An independently completed owner/Redis error
is not masked by a later diagnostic write failure. Grace expiry remains 72.

## Evidence

Isolated Redis tests prove:

- ordered Starting/PaperReady/Degraded/Draining/Stopped publication;
- immediate plus bounded periodic publication;
- exact NOMKSTREAM missing-stream failure without implicit creation;
- bounded-channel fail-closed and first-wins behaviour;
- a real child production process reaches PaperReady, receives SIGTERM,
  publishes Draining and Stopped, exits zero, leaves the durable root unchanged
  and leaves the M10 PEL empty;
- every child-process payload retains all closed-surface flags and contains no
  forbidden raw-field names;
- all inherited process/restart/cancel/day-expiry tests remain green.

The acceptance matrix is
`stage8b-p1e-i1-telemetry-composition-acceptance-matrix.csv`.

## Next boundary

After independent source acceptance, the next authorized slice is
**fixed-path installation and systemd material** with isolated target-Linux verification.
I1 aggregate acceptance follows only after that slice is independently
accepted. No operational installation or service start is authorized here.
