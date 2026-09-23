# Stage 8B-P1-e I1 aggregate closure readiness

Status: **REVIEW CANDIDATE — I1 NOT CLOSED**.

This package is a reconciliation boundary after the independent source
acceptance of process supervision at
`1086b8d95e10514532d1c25c57956eca943b732c`. It does not alter production
Rust, Cargo, deployment files or runtime behaviour. It records what has been
accepted, identifies the remaining work without upgrading design artifacts to
implementation evidence, and proposes the shortest safe route to aggregate I1
acceptance.

## Accepted source baseline

The accepted source lineage now covers:

- the I0 latch-aware acquisition seam and corrected coordinator foundation;
- authenticated signed-schedule acquisition and V4 binding/recovery;
- fixed-path first boot, transaction V5, receipt V2, provenance and pre-seal
  administrative recovery;
- fresh and committed Market, Initial LIMIT, Working LIMIT, Cancel and
  Day-expiry paper lifecycle composition;
- retained owner-loop progression, truth-before-XACK and exact replay;
- process-level SIGTERM/SIGINT/SIGKILL, owner-panic and restart witnesses;
- the continuous V5 Market lineage through production schedule reading,
  durable V4, controlled restart before effect, `S_ack`, `S_truth`, XACK-last
  and readmission.

The exact accepted milestones and review SHA-256 bindings are in
`stage8b-p1e-i1-aggregate-closure-inventory.json`. Held intermediate commits
are historical development points and are not promoted by this package.

The V4 continuous witness performs a controlled owner drop/restart at the
durable pre-effect frontier. Its four explicit restart/readmission observations
are initial adoption, committed V4 before the provider effect,
post-truth/XACK, and repeat admission after `AlreadyAcknowledged`. It is not an
OS-SIGKILL witness for plain-Market V4 and is not described as proof of every
crash frontier in the project.

## What exists but is not yet deployable composition

The source tree already contains the fixed supervisor binary and CLI, strict
fixed-path parsing, protected config and systemd-credential loading, exact
loopback Redis DB15 policy, health/readiness DTOs, deterministic redaction,
readiness classification and `XADD ... NOMKSTREAM MAXLEN = 4096` telemetry
write primitives.

Those pieces are prerequisites, not completion evidence:

- the production process loop does not yet construct and publish periodic and
  transition health/readiness envelopes;
- no tracked P1-e service unit, install/uninstall transaction, sysusers or
  tmpfiles material exists under `deploy/`;
- no target-Linux `systemd-analyze verify`, clean-host install, rollback or
  idempotent reinstall evidence exists for this supervisor;
- aggregate I1 acceptance has not been issued.

## Remaining development slices

### 1. Telemetry composition

Wire the accepted telemetry vocabulary and existing Redis writer into the sole
production owner/supervisor path. The slice must prove starting, paper-ready,
degraded, draining and stopped transitions; failure precedence; bounded
periodic publication; `NOMKSTREAM`; exact retention; redaction; no implicit
stream creation; restart consistency; and no second lifecycle owner. Tests use
isolated Redis only. Operational DB15 and VPS activation remain forbidden.

### 2. Fixed-path installation and systemd material

Add the P1-e-specific service/install boundary only after telemetry semantics
are stable. It must bind the accepted binary, config, lifecycle credential,
state root, loopback DB15 policy and stop timeout; preserve bootstrap/recovery
network isolation; fail closed on ownership/mode/path drift; and provide
target-Linux static verification plus clean install, rollback and idempotence
evidence. Review must not start the service or contact operational Redis.

### 3. Aggregate I1 acceptance

After independent source acceptance of both slices, run the complete inherited
I0/I1A/I1 source, process, Redis integration, doctest, formatting and strict
Clippy gates from one clean immutable commit. The final package must bind the
accepted review lineage, execution logs, source tree, installation/systemd
evidence and telemetry evidence. Only that later review may mark I1 closed.

The efficient order is telemetry composition, then install/systemd material,
then aggregate acceptance. It avoids freezing a service package before its
health/readiness behaviour is final.

## Closed operational surfaces

This package grants no authority for:

- operational Redis DB0 or DB15;
- VPS installation or service start;
- paper-provider operational activation;
- FINAM POST, DELETE or send;
- broker dispatch;
- runtime-live or real orders.

P1-f or any operational paper activation remains downstream of a separately
accepted I1 aggregate closure.
