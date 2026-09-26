# Stage 8B-P1-f Id — fixed Redis composition

Status: `REVIEW_CANDIDATE_SOURCE_ONLY_NO_ACTIVATION`.

Correction baseline: reviewed Id R1 candidate
`ea2897a2831168cc9bfe33ca63161182ab065850`; this final narrow correction
closes the remaining P1-ID02 recovery/terminal-audit gaps without opening Ie.

Immutable predecessor: accepted P1F-Ic commit
`5c2656fbe8691da256b5380dd16ce6f6b6aa1fa8`. Its independent review is bound
by SHA-256
`ee69d58bc70288f447a9ab880d2a2eec01fefdfe6f86ce3be603eb7f5a1b30b3`.

## Scope

P1F-Id implements the narrow source composition authorized by the Ic review:

- eight exhaustive Redis roles and ten existing source operations;
- the eight accepted Lua identities, pinned by SHA-256;
- fixed loopback DB15 production endpoints and no exposed raw connection;
- exact retained `Prepared` M10 publication, exact Redis-ID reread and only
  then durable `Published` high-water;
- post-effect adapter-result loss followed by a controlled restart before
  `Published`, replaying the same retained bytes and deterministic ID without
  rebuilding from a fresh M1 batch;
- one five-second bounded resource task in the existing production owner
  `select`, measuring both fixed P1 groups, conservative Redis memory/evidence
  use and root free space;
- fail-closed transfer of either a crossed bound or probe failure to the
  existing first-wins `RedisLifecycleFailure` latch and bounded terminal path;
- a bounded 4096-entry operation audit containing fixed enums, script hashes,
  command fingerprints and results, never raw command material or credentials.
- execution-point audit wiring for verify-only attach, retention admission,
  schedule read, stale discovery/cleanup, acquire/reclaim, command publication
  and XACK-last; rejected cross-role attempts are retained as `Rejected`;
- shared exact-reclaim auditing for restart/recovery routes, plus process-level
  ownership and terminal emission of the bounded sink after early return or
  owner abort.

This is a closed composition, not a generic Redis proxy or extensible policy
engine. `PhaseGuardian` and `BrokerTruthObserver` deliberately have no Redis
capability. The read-only resource probe owns a private connection and issues
only `INFO memory` plus bounded `XPENDING` reads for the exact canonical-M10
and command groups.

## Publication ordering

The M10 publication transition is:

1. validate and durably persist the exact `Prepared` state;
2. publish only `exact_canonical_m10_bytes` under its deterministic Redis ID;
3. resolve an uncertain publication result through the accepted exact-ID
   `XRANGE` path;
4. perform an explicit exact reread and compare ID and bytes;
5. validate the receipt ID, bytes SHA-256 and `exact_reread=true`;
6. mark the retained state `Published`, fsync, rename, parent-fsync and reread.

Any error before step 6 leaves the same durable `Prepared` state. The linked
real-Redis control executes the effect and exact reread, then loses the adapter
result before the producer persists `Published`. A fresh producer/feeder
instance reloads `Prepared`, retries and proves one stream entry plus unchanged
sequence. This is a controlled post-effect restart witness, not a simulated
network-response loss before reread. A restart cannot advance the high-water
or synthesize replacement bytes.

## Resource behavior

The frozen limits are aggregate PEL `<=64`, Redis/evidence use `<=536870912`
bytes, root free space `>=10737418240` bytes and a poll interval of five
seconds. The PEL scope is exactly the canonical M10 and command groups; command
PEL must remain zero even when the aggregate is at most 64. Crossing a limit
or failing/timing out a probe stops only P1 through the existing latch and
terminal coordinator. Id contains no trim, delete, Redis configuration, Redis
restart or P0 service action.

## Evidence

Targeted Rust tests cover the fixed role/script matrix, retained rejected
cross-role evidence, hash-only bounded audit, exact boundary values, repeated
resource ticks, fail-closed probe loss, real supervisor-operation audit,
recovery reclaim success/failure with no Ready double-accounting, retained
audit after early startup failure and grace-deadline owner abort,
existing terminal routing, real isolated-Redis idempotent publication plus
exact reread, one linked post-effect adapter-result-loss restart, and refusal
of a forged reread receipt. The source checker and mutation harness pin these
properties and all closed surfaces. The aggregate gate also runs the complete
runtime durability suite with all features, the complete FINAM gateway suite
with its normal closed-endpoint feature set, both all-feature doctest suites,
and strict all-target/all-feature Clippy. The FINAM full suite deliberately
uses the normal feature set because its historical negative endpoint test is
incompatible by definition with the separate `m3j16-actual-one-shot` feature;
that feature remains compiled, linted and covered by its dedicated tests.

## Closed surfaces and next boundary

Operational installation/start, target VPS mutation, operational DB15/DB0,
paper-provider execution, FINAM POST/DELETE, broker dispatch, runtime-live and
real orders remain closed. Id acceptance opens only P1F-Ie aggregate source
closure. P1F-O0 still requires a separate immutable operational authorization.
