# Stage 8B-P1-f Id — fixed Redis composition

Status: `REVIEW_CANDIDATE_SOURCE_ONLY_NO_ACTIVATION`.

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
- response-loss/restart replay of the same retained bytes and deterministic
  ID, without rebuilding from a fresh M1 batch;
- five-second bounded resource polling for total PEL, conservative Redis
  memory/evidence use and root free space;
- transfer of a crossed resource bound to the accepted supervisor
  `RedisLifecycleFailed` terminal path;
- a bounded 4096-entry operation audit containing fixed enums, script hashes,
  command fingerprints and results, never raw command material or credentials.

This is a closed composition, not a generic Redis proxy or extensible policy
engine. `PhaseGuardian` and `BrokerTruthObserver` deliberately have no Redis
capability. The read-only resource probe owns a private connection and can
issue only `INFO memory`; PEL inspection remains in the accepted bounded
supervisor control API.

## Publication ordering

The M10 publication transition is:

1. validate and durably persist the exact `Prepared` state;
2. publish only `exact_canonical_m10_bytes` under its deterministic Redis ID;
3. resolve Redis response loss through the accepted exact-ID `XRANGE` path;
4. perform an explicit exact reread and compare ID and bytes;
5. validate the receipt ID, bytes SHA-256 and `exact_reread=true`;
6. mark the retained state `Published`, fsync, rename, parent-fsync and reread.

Any error before step 6 leaves the same durable `Prepared` state. A restart
therefore cannot advance the high-water or synthesize replacement bytes.

## Resource behavior

The frozen limits are PEL `<=64`, Redis/evidence use `<=536870912` bytes, root
free space `>=10737418240` bytes and a poll interval of five seconds. Crossing
a limit stops only P1 through the existing terminal coordinator. Id contains
no trim, delete, Redis configuration, Redis restart or P0 service action.

## Evidence

Targeted Rust tests cover the fixed role/script matrix, forbidden cross-role
authority, hash-only bounded audit, exact boundary values, existing terminal
routing, real isolated-Redis idempotent publication plus exact reread, real
resource reads, response-loss restart with identical bytes, and refusal of a
forged reread receipt. The source checker and mutation harness pin these
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
