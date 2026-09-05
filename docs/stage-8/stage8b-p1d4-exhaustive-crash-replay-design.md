# Stage 8B-P1-d4 exhaustive crash/replay closure design

Status: design-only review candidate. P1-d4 production/test implementation is
not authorized by this document.

Accepted predecessor:
`7dc7c802feca6e79d3a1a9902c181ad7b6afc506` (Stage 8B-P1-d3 governance
closure R1, CLOSED / ACCEPTED).

## Purpose

P1-d3 fixes the deterministic LIMIT/CANCEL/Day-expiry semantics and proves two
subprocess boundaries around `S_cancel_recovered`. P1-d4 closes the remaining
crash/replay proof gap. It does not add an order type, change a strategy, open
an operational service, or broaden broker access.

The implementation authorized only after this design is accepted must prove,
with real child processes and isolated ephemeral Redis, that every durable
effect boundary resumes from exactly one authenticated continuation:

```text
pending source
  -> deterministic observation/outcome
  -> Stage 6 write-ahead outcome
  -> Stage 7 RequestFinalized
  -> ACK
  -> replacement S_ack
  -> truth/book transition
  -> replacement S_working | S_eval | S_terminal | S_cancel_recovered
  -> same-bar callback when required
  -> source XACK last
```

P1-d3 lifecycle semantics remain immutable. P1-d4 may add test-only crash
barriers, subprocess fixtures, evidence extraction and fail-closed scanners.
It may not change fill price, expiry, sequence, projection, identity, callback
ordering, or cancel resolution.

## Test boundary and threat model

Every crash case uses a newly created temporary durable root and a newly
started loopback-only Redis instance on a random port. Redis DB 0 on a VPS is
not a test target. The child is stopped by the parent with an actual process
kill after an fsync-backed marker proves that the named frontier was reached.
An in-process panic is not sufficient evidence for a required crash case.

The threat model includes:

- process death after an in-memory effect but before its covering seal;
- process death after a covering seal but before the next legal effect;
- Redis XACK success followed by loss of the client-visible response;
- delivery reclaim by a new consumer identity after restart;
- exact duplicate input and same-identity conflicting input;
- stale, missing, forged or cross-bound replacement/journal/book material;
- restart attempts that would reacquire schedule/provider/callback authority.

Disk loss, Redis data loss, Redis cluster failover, partial fills, nonzero
fees/slippage and broker-network ambiguity are outside P1-d4.

## Named frontier taxonomy

The implementation must expose these names only through the existing
test/artifact-fixture crash hook. In ordinary builds the hook is a no-op and
environment variables cannot alter production behavior.

| ID | Required boundary | Durable restart disposition |
|---|---|---|
| `F00` | command source pending, before schedule observation/provider execution | exact pre-dispatch continuation; no outcome, ACK, truth or XACK |
| `F01` | outcome/evidence appended and fsynced, before `RequestFinalized` | `P1d3PreAckPending`; reconstruct only from authenticated Stage 6 V3 evidence |
| `F02` | `RequestFinalized` durable, before ACK application | `P1d3PreAckPending`; exact ACK replay only |
| `F03` | ACK applied in memory, before covering `S_ack` | `P1d3PreAckPending`; same ACK identity/bytes/sequence, never a new ACK |
| `F04` | `S_ack` persisted+reread, before truth application | `P1d3AckCommitted`; truth-only continuation |
| `F05` | truth/book transition applied in memory, before its covering seal | `P1d3AckCommitted` or the exact pre-transition owner; replay cannot duplicate truth |
| `F06` | `S_working` or `S_terminal` persisted+reread, before command-source XACK or cancel continuation | `P1d3TruthCommitted` or exact `CancelContinuationPending` |
| `F07` | recovered cancel ACK applied, before `S_cancel_recovered` | `P1d3PreAckPending`/`CancelContinuationPending`; exact recovered ACK replay only |
| `F08` | later-bar evaluation applied, before `S_eval` or `S_terminal` | pre-evaluation package plus authenticated Stage 6 V3 evidence; no second schedule selection |
| `F09` | `S_eval` or later `S_terminal` persisted+reread, before same-bar Hybrid callback | `P1d3SemanticPending`; callback-only continuation |
| `F10` | same-bar callback committed, before bar-source XACK | callback-committed continuation; no callback replay |
| `F11` | Redis accepted XACK but its response was lost | source is already absent from PEL; resume returns `AlreadyAcknowledged` without semantic replay |

No implementation may collapse `F03` into `F04`, `F05` into `F06`, or `F09`
into `F10`: those pairs distinguish an uncovered in-memory effect from a
durably authenticated replacement.

## Required scenario coverage

The machine-readable acceptance matrix is normative. At minimum, the
subprocess suite must cover these semantic families:

1. initial LIMIT -> Working;
2. initial LIMIT -> Filled;
3. initial LIMIT -> Expired;
4. later Working -> untouched `S_eval`;
5. later Working -> Filled;
6. later Working -> Day Expired;
7. CANCEL -> `CancelCanceled`;
8. CANCEL -> `CancelExecutionObserved`, including target-first fill;
9. CANCEL -> `CancelAlreadyTerminalNonExecution`;
10. exact duplicate and conflict paths for all restart dispositions.

Common initial ACK/truth boundaries may share one parameterized harness, but
each of the three initial outcome shapes must execute independently and assert
its exact post-restart projection. A test name or table row without a spawned
child, reached marker, process kill, clean restart and final audit is not
credited as frontier coverage.

Later-bar `S_eval`/`S_terminal` tests must use the same canonical M10 Redis
delivery before and after restart. A changed payload under the same Redis ID
must fail before callback, sequence allocation and XACK. Day expiry has no
Redis source at the boundary and therefore must prove seal recovery without
inventing an XACK.

Cancel tests must preserve target-first ordering. If target evaluation fills
the order, its `S_terminal` is durable before the recovered cancel ACK. Restart
must never create a second target truth. If the target is already terminal,
only the exact recovered ACK and `S_cancel_recovered` are legal.

## Restart invariants

For every frontier, the parent records a pre-kill audit marker with
`write_all` plus `sync_all`. The marker contains only non-secret evidence:
case ID, frontier ID, source Redis ID, expected restart disposition, seal
generation/hash, Stage 6 checkpoint hash, book transition ordinal/hash,
expected `(seq_ack, seq_truth)` or exact single-sequence value, callback count,
command/truth fingerprints and expected PEL state.

After restart, the test must compare the marker literally with the
authenticated recovered audit. Derived counts alone are insufficient. The
following properties are mandatory:

- operational identity, generation and commitment key remain exact;
- Stage 6 checkpoint and replacement package are mutually bound;
- sequence values are neither skipped, reused nor reallocated;
- broker order/trade IDs and all eight accepted projection shapes are stable;
- book transition ordinal and previous/current hashes are stable;
- callback count is unchanged before `F09`, increases exactly once while
  completing `F09`, and never increases again at `F10`/`F11`;
- command and bar source stay pending until their respective final seal;
- XACK is the last external mutation and is impossible from earlier owners;
- restart does not call the provider, mint schedule authority, reread wall
  clock, select a new candidate bar, or invoke Hybrid except at `F09`;
- final restart is Ready with no pending semantic request and the expected
  active/terminal working-book row.

All restart APIs remain phase-linear. The test suite must assert forbidden
methods structurally where possible and behaviorally through negative cases.

## XACK response-loss contract

`F11` is tested by allowing the isolated Redis server to execute the exact
XACK while the test transport deliberately discards the successful response.
The child then terminates before converting the result to a ready owner.

On restart, exact source lookup must prove both:

```text
XPENDING has no matching entry
AND
the authenticated consumer-group frontier is at least the exact source ID
```

Only then may the disposition be `AlreadyAcknowledged`. Missing PEL membership
without the frontier proof is `ExactSourceConflict`, not success. No second
ACK, truth, callback, book transition, sequence allocation or command
publication is permitted.

## Duplicate and conflict matrix

Each recovered disposition must be exercised twice:

- byte-identical replay is idempotent and converges to the same final audit;
- mutation of one bound field fails before any new effect.

Bound-field mutations include source Redis ID, semantic hash, payload hash,
schedule fingerprint, candidate close time, request/client/order/trade ID,
limit/quantity/side, outcome evidence bytes, Stage 6 checkpoint, replacement
seal, book generation/ordinal/transition hash, sequence pair and target order
identity. A changed consumer name alone must not change semantics after an
exact reclaim.

Duplicate/conflict cases are ordinary restart tests in addition to, not a
replacement for, the required subprocess crash cases.

## Evidence contract

The implementation gate must emit one deterministic JSON result with:

- accepted P1-d3 predecessor and tested source/tree refs;
- exact frontier registry and scenario registry;
- one result per required matrix cell;
- child PID/exit classification, marker digest and isolated Redis endpoint
  classification (never credentials);
- pre-kill and post-restart audit digests;
- exact restart disposition and final disposition;
- PEL before/after counts and XACK disposition when applicable;
- callback/provider/schedule/sequence/order/trade/truth counters;
- aggregate required/passed/failed counts;
- explicit closed-surface flags.

Evidence ordering is lexicographic by case ID, JSON is canonicalized by the
project checker, and two clean runs over the same source must produce the same
semantic digest after volatile PID/port/path fields are excluded. Missing,
duplicate, skipped, ignored or `not_applicable` required cells fail closed.

The implementation acceptance gate must run the P1-d4 subprocess matrix with
`--ignored --test-threads=1` where required, the P1-d4 negative harness, all
P1-d3 source gates, workspace debug/release tests, doctests, strict clippy,
no-Redis smoke and Redis-shadow smoke. The immutable handoff must bind complete
logs and generated evidence to one commit and source-tree manifest.

## Allowed implementation delta after design acceptance

- test-only crash-barrier names and fsync-backed audit markers;
- subprocess/SIGKILL and Redis response-loss fixtures;
- read-only test audit accessors with no production authority;
- P1-d4 checker, negative harness, evidence generator and handoff tooling;
- status/roadmap synchronization.

Any non-test change to P1-d3 lifecycle semantics requires a new reviewed
design amendment. Cargo feature changes, workflow changes, deployment files,
VPS configuration and live credentials are outside this slice.

## Explicitly closed

- operational Redis DB 0 or VPS activation;
- deployable supervisor and operational schedule adapter (P1-e);
- isolated operational paper acceptance (P1-f);
- FINAM POST/DELETE, broker dispatch and broker-network execution;
- runtime-live, real orders and unattended execution;
- partial fills, fees/slippage, replace, stop, stop-limit, take-profit,
  bracket and multi-leg behavior;
- Generation-2 production authorization.

Acceptance of this design authorizes only the P1-d4 source/test implementation
slice. A separate independent source acceptance and governance-only authority
rebind are required before P1-e can open.
