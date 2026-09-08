# Stage 8B-P1-e deployable paper supervisor design

Status: R0 design-only review candidate. Production implementation and every
operational activation remain unauthorized until this exact design is
independently accepted.

Accepted predecessor: `c2a9e1246dfdd59f3a6297268de907dedcb19903`
(Stage 8B-P1-d4 governance closure R1, CLOSED / ACCEPTED).

Accepted predecessor review SHA-256:
`93180e3f633c256ab9bb2cdfa43dd63c3fe7a1b7380eac435feae45cf8969142`.

## Purpose and boundary

P1-d4 closes the deterministic paper lifecycle and exhaustive crash/replay
proof. P1-e makes that already accepted composition deployable and observable.
It does not add trading semantics, activate a Redis database on a VPS, attach
FINAM transport or authorize a real broker effect.

P1-e owns only:

- one dedicated paper-only process entry point;
- restart-only service startup and a separate explicit first-boot command;
- systemd unit, service identity, filesystem ownership and credential custody;
- redacted health/readiness publication;
- task-failure arbitration and graceful SIGTERM/SIGINT handling;
- source/tests/evidence needed to prove those properties on an ephemeral,
  loopback-only Redis instance.

P1-f owns isolated operational acceptance. Operational Redis DB0/VPS
activation is not part of P1-e.

## Executable and dependency boundary

The implementation adds a dedicated binary target named
`stage8b-p1-paper-supervisor` to the `runtime-durable-service` package. It is
not a `broker-cli` subcommand and does not depend on `broker-finam`,
`finam-gateway`, an HTTP client or a FINAM token type. Its dependency closure
must be checked by the P1-e gate.

The binary has three explicit modes:

1. `validate-config CONFIG` performs no Redis connection and no durable write;
2. `bootstrap CONFIG CONFIRMATION` is an offline, one-shot administrative
   operation using the accepted P1-a first-boot facade and the exact literal
   `CREATE_NEW_STAGE8B_P1_DURABLE_ROOT`;
3. `run CONFIG` is restart-only. It must fail if the identity-derived durable
   root is absent and must never recreate or reseed it.

No mode accepts secret bytes or a caller-selected credential filename/path in
argv, JSON or ordinary environment overrides. `run` and `bootstrap` load exactly
`stage8b-p1-lifecycle.key` through the existing
systemd-provided `CREDENTIALS_DIRECTORY` boundary; that manager-provided
directory pointer is the sole environment exception. Ordinary service startup
invokes only `run`.

The existing P0 units under `deploy/paper-shadow/` remain byte-for-byte
unchanged. They are not upgraded in place and cannot run concurrently against
the P1 namespace.

## Configuration contract

The supervisor consumes one `deny_unknown_fields` JSON configuration. It
contains the accepted `Stage8bP1BootstrapConfig` plus only non-secret
supervisor settings:

- `schema_version = 1`;
- loopback Redis URL with an explicit nonzero database for P1-e/P1-f testing;
- fixed namespace and group values returned by `stage8b_p1_redis_namespace()`;
- `health_interval_ms` in `[1000, 60000]`;
- `shutdown_grace_ms` in `[5000, 90000]` and strictly below the systemd stop
  timeout;
- bounded read/claim settings satisfying accepted P1-c constraints;
- exact runtime configuration fingerprint and deployment generation inherited
  from the P1-a identity.

The checked-in example uses DB15 and placeholder account/public-key material.
For P1-e, DB0, non-loopback Redis, TLS credential files, arbitrary stream
names, arbitrary consumer groups and environment overrides are rejected. A
later P1-f acceptance may authorize one exact isolated deployment config; it
does not silently weaken this schema.

## Credential and filesystem custody

The commitment key is exactly 32 bytes and is unrelated to the Stage 8B
Generation-2 signing ceremony. The existing P1-a loader remains the only key
loader. It requires an absolute canonical credential directory, `O_NOFOLLOW`,
one regular link, mode no wider than `0600`, and root or effective-service-user
ownership. The secret is absent from logs, JSON, Redis, process arguments,
coredumps, handoff archives and environment values.

The systemd unit uses:

```text
User=moex-p1-paper
Group=moex-p1-paper
UMask=0077
LoadCredential=stage8b-p1-lifecycle.key:/etc/moex-finam-p1-paper/credentials/stage8b-p1-lifecycle.key
ReadWritePaths=/var/lib/moex-finam-p1-paper
```

The config and executable are read-only to the service. The durable parent is
`/var/lib/moex-finam-p1-paper` and the child root name is derived only by the
accepted P1-a identity function. A missing, symlinked, group/world-writable,
wrong-owner or identity-mismatched path fails before Redis attachment.

The unit disables privilege gain, capabilities, writable executable paths,
home access, device access and coredumps. It permits only Unix and loopback
IPv4/IPv6 sockets required by Redis. It uses a persistent service identity,
not `DynamicUser`, because journal ownership must survive restart.

## Startup state machine

`run` follows this exact order:

```text
S00 parse argv and canonical config
S01 load and validate systemd credential
S02 construct the exact fresh Hybrid runtime from accepted config
S03 restart the identity-derived Stage 7B durable root
S04 classify/recover the exact accepted P1-d4 restart disposition
S05 attach to the pre-created Redis namespace in verify-only mode
S06 complete stale-PEL claim scan before any fresh read
S07 publish redacted Degraded/Starting snapshot
S08 perform one successful source poll, including an empty bounded poll
S09 publish PaperReady and enter the single-owner loop
```

No Redis connection, stream creation, group creation, XADD, XACK or consumer
registration is legal before S03 returns a recognized authenticated recovery
state. Normal `run` never calls the P1-c namespace initializer. Redis
initialization is a separate P1-f administrative action.

Every P1-d4 recovered phase remains phase-linear. The supervisor may invoke
only the exact continuation accepted for that phase. It may not reacquire a
provider/schedule/callback authority, fabricate a clean seed, skip a
replacement seal or expose an owner to another task. Unknown or conflicting
recovery material publishes no `PaperReady` and terminates fail-closed.

## Single-owner loop and task supervision

One task owns the mutable P1 composition. Telemetry and signal tasks receive
immutable snapshots/channels only; they cannot obtain a Redis XACK authority,
commitment key, Stage 7 owner, provider or mutable Hybrid runtime.

Only one canonical M10/request lifecycle may be in progress. Fresh reads are
disabled while a recovered or current lifecycle is incomplete. The next read
is allowed only after the accepted terminal replacement is persisted and
reread and, where applicable, the exact source XACK-last result is known.

Any owner-task panic, unexpected return or telemetry-task failure transitions
the aggregate state away from `PaperReady`, requests coordinated shutdown and
causes a nonzero process exit after the current durable phase reaches a safe
restart boundary. Internal task restart is forbidden; systemd is the only
process restart authority.

## Readiness and health streams

P1-e publishes versioned JSON envelopes to the fixed P1 namespace streams:

- `finam_imoexf_paper:{finam-imoexf-p1}:health`;
- `finam_imoexf_paper:{finam-imoexf-p1}:readiness`.

Each stream is retained with exact `MAXLEN = 4096`. Telemetry is diagnostic;
it cannot authorize lifecycle continuation.

Every snapshot contains:

- schema/domain and UTC observation timestamp;
- random diagnostic `boot_id` and restart-only `boot_mode`;
- operational identity digest, deployment generation and durable seal
  generation/commitment digest;
- fresh consumer name, source-poll/claim-scan freshness and PEL count;
- blocked request count and redacted hashes, never raw account IDs;
- last accepted semantic-bar and canonical-ACK timestamps when present;
- shutdown phase and last failure class;
- `paper_only=true`;
- `finam_transport_attached=false`;
- `broker_network_dispatch_attached=false`;
- `runtime_live=false`;
- `real_orders=false`.

The only readiness phases are `Starting`, `PaperReady`, `Degraded`, `Draining`
and `Stopped`. The supervisor must never serialize `LiveReady`.

`PaperReady` requires all of the following in one snapshot:

- live owner task;
- validated durable storage and current recovery seal;
- exact Redis namespace/group verification;
- completed startup claim scan;
- fresh successful source poll;
- healthy settlement path;
- zero unresolved durable/semantic lifecycle;
- zero blocked/conflicting entries;
- no shutdown request.

An empty successful bounded source poll is fresh. A Redis error, stale poll,
stale claim scan, unknown recovery phase, task failure or unresolved lifecycle
is `Degraded`, never ready.

## Graceful shutdown contract

The binary handles SIGTERM and SIGINT directly. The first signal atomically
sets one shutdown latch, stops acquisition of fresh M10/command work and emits
`Draining` when Redis remains available. A second signal does not bypass the
durable protocol.

If the owner is already at an authenticated quiescent boundary, it stops. If a
transition has an uncovered in-memory effect, it continues only far enough to
persist and reread the next accepted P1-d4 replacement package. It must not
start another provider call, schedule selection, Hybrid callback or source
read merely to make shutdown convenient. A source remains pending whenever
the accepted XACK-last preconditions have not been met.

Before a zero exit, the process proves that no uncovered mutable owner remains,
drops the commitment-key owner, attempts one final redacted `Stopped` snapshot
and prints one redacted local summary. Failure to publish final telemetry does
not weaken the durable boundary. Failure to reach an authenticated restart
boundary before the configured grace deadline exits nonzero and lets systemd
apply its final kill policy; accepted P1-d4 restart semantics remain the
recovery authority.

The systemd unit uses `Restart=on-failure`, `KillSignal=SIGTERM`,
`KillMode=control-group`, `TimeoutStopSec=100`, `FinalKillSignal=SIGKILL` and
`SendSIGKILL=yes`. A normal SIGTERM completion is not restarted.

## Implementation and evidence contract

After independent design acceptance, the P1-e implementation slice may add:

- the dedicated binary target and narrow supervisor module;
- one config example;
- one service unit plus sysusers/tmpfiles declarations;
- isolated tests, scanners, evidence and handoff tooling;
- status/roadmap synchronization.

It must not alter accepted P1-d3/P1-d4 lifecycle semantics. Any production
change under their reducers/recovery transitions requires a separate design
amendment.

The source gate must prove at least:

1. config validation and exact fixed namespace;
2. credential success and missing/symlink/mode/size/owner negatives;
3. restart-only refusal of missing or mismatched durable roots;
4. no Redis contact before durable recovery succeeds;
5. verify-only Redis attachment and claim-before-fresh-read;
6. one fresh consumer identity per process boot;
7. `PaperReady` positive and every readiness-reason negative;
8. exact health/readiness retention and redaction;
9. real child-process SIGTERM while idle and at representative durable
   frontiers, with restart convergence and no duplicate callback/provider;
10. task panic/unexpected-return propagation to nonzero exit;
11. systemd unit verification and hardening assertions;
12. dependency/scanner proof that FINAM and live execution surfaces are
    unreachable;
13. existing P1-d4 source gate, workspace debug/release tests, doctests,
    strict clippy, no-Redis smoke and isolated Redis smoke.

Evidence is emitted from two clean isolated runs and binds source ref/tree,
unit/config hashes, process exit/signal observations, pre/post durable audit,
telemetry hashes, PEL/XACK counters and closed-surface flags. Volatile PID,
port, boot ID, temporary path and wall duration are excluded only from the
semantic digest; pass/fail, ordering and durable identities remain bound.

## Deferred nonblocking P2

The accepted P1-d4 review retains one P2: its Python evidence checker should
require `0 < child_pid <= u32::MAX` and one coherently redigested negative.
P1-e design does not alter that accepted checker. It may be closed only in an
explicitly documented source-hardening change; it cannot be used to widen the
P1-e deployment scope.

## Explicitly closed

- operational Redis DB0 or VPS activation;
- any non-loopback or shared Redis deployment;
- P1-f operational acceptance;
- FINAM REST/WebSocket attachment in the P1 supervisor;
- FINAM POST/DELETE, broker dispatch and real orders;
- runtime-live and unattended live execution;
- partial fills, fees/slippage, replace, stop, stop-limit, take-profit,
  bracket and multi-leg behavior;
- Generation-2 activation, signing key or authorization issuance;
- RI and USDRUBF deployment.

Acceptance of this design authorizes only P1-e source implementation. It does not authorize installing or starting the resulting service on a VPS.
