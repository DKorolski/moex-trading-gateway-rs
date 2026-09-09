# Stage 8B-P1-e R8 executable latch-seam design correction

Status: design-only review candidate. Production Rust, Cargo, workflow, active
unit, deployment, Redis and FINAM code are unchanged.

R8 is the direct child of held R7 commit
`7ebdcef45c1c55f4783bd6b2b1502ea78d4d97d7`. The immutable accepted
predecessor remains P1-d4 closure
`c2a9e1246dfdd59f3a6297268de907dedcb19903`. R8 retains Option A, the opaque
linear owner, the single-use route-bound permit, the 15/5 logical acquisition
partition, non-cancellable acquisition/continuation and the exact three-file
I0 allowlist.

## Terminal resolution is observation, not acquisition

The R7 absolute post-owner Redis-read ban is replaced by a narrower rule: no
second delivery acquisition. After a terminal permit, one route-exact
resolution may run the accepted read-only `XINFO GROUPS`, exact `XRANGE` and
exact `XPENDING` checks and, only for an exact pending member, one exact
`XACK`. A zero XACK reply requires the accepted postcheck before response-loss
may classify `AlreadyAcknowledged`.

`XAUTOCLAIM`, `XREADGROUP`, every reclaim function, every
`exact_delivery_for_*` function, a second acquired owner and ownership
transfer remain forbidden after permit construction. The accepted P1-d4
publication-revalidation Lua is explicitly permitted and required after the
permit and before generated-Market terminal resolution. It remains forbidden
before the latch decision.

## Exact route boundaries

The normative `Stage8bP1eLatchRouteTransitionMatrixV1` expands 20 logical
routes into all 30 material variants: 19 reclaim and 11 terminal. Every row
freezes acquisition Redis sequence, continuation input, first post-permit
effect, exact next covering boundary, returned owner/disposition, PEL and XACK
state, timer ordering and the post-boundary latch recheck. Tests derive their
count and IDs from this matrix. Removing candidate-source, S_terminal,
AlreadyAcknowledged or due-timer coverage cannot pass by preserving the
logical route name.

A signal immediately after permit creation does not cancel the continuation.
It completes only the selected row's exact next authenticated covering
boundary, records exact counters and final owner/disposition, then rechecks
the latch. A route mismatch consumes the permit and fails closed.

## Cause-preserving shutdown

`Stage8bP1eShutdownIntentV1` carries the first cause, final exit class, grace
deadline and first request sequence. The first cause is monotonic. External
signal maps to 0 after bounded completion, owner failure to 70, telemetry
failure to 71 and signal-task failure to 73. Grace expiry returns 72 while
retaining the initiating cause diagnostically. A second signal does not
replace the first intent. E18 and E25 are intermediate ownership transitions;
neither may downgrade a failure to clean exit.

## I0 acceptance gate

Independent R8 acceptance may authorize only the I0 source seam in:

```text
crates/runtime-durable-service/src/stage8b_p1_semantic/redis.rs
crates/runtime-durable-service/src/stage8b_p1_semantic.rs
crates/runtime-durable-service/src/lib.rs
```

Because `redis.rs` contains P1-d4, I0 acceptance must rerun the complete
accepted P1-d4 source gate: all 105 real SIGKILL cells in two clean runs, the
retained-evidence checker, source and evidence negative harnesses, core and
service tests/doctests, strict clippy and standalone P1-d2/P1-d3 regressions.

R8 opens no supervisor binary, service installation, operational DB0/VPS,
FINAM POST/DELETE, broker dispatch, runtime-live, real orders, partial fills or
protective orders. The `0 < child_pid <= u32::MAX` item remains deferred.
