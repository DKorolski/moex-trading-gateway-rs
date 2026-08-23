# Stable macro-roadmap

Status: accepted.

Reviews may split a macro-stage into sub-stages or patch gates, such as
Stage 2A, Stage 2B, and Stage 2B-N patches, but reviews do not renumber or
replace this macro-roadmap unless an explicit roadmap ADR is accepted.

## Stages

- Stage 0 — Baseline / source import / safety gates.
- Stage 1 — ALOR operational contract extraction.
- Stage 1B — IMOEXF `HybridIntradayRuntime` paper/shadow compatibility freeze.
- Stage 2A — Runtime source migration inventory / plan.
- Stage 2B — Runtime source migration implementation.
- Stage 3 — Market-data parity to strategy input level.
- Stage 4 — Broker-truth bootstrap into runtime.
- Stage 5 — Real strategy semantics attachment.
- Stage 6 — Durable request/client/broker id chain.
- Stage 7 — Runtime command consumer paper/mock.
- Stage 8 — Real FINAM execution under command consumer.
- Stage 9 — Orders/trades/positions reconciliation loop.
- Stage 10 — Runtime-live readiness and observability.
- Stage 11 — Dual-broker shadow parity.
- Stage 12 — First runtime-driven live micro.
- Stage 13 — Stop/SLTP/bracket.

## Current active stage

Stage 8A-2 R1 is independently accepted and closed at
`16180ac4f8eab761b3b055c1f5515f62cd94bfb9`.

Stage 8A-3 R2 is independently accepted and closed at
`012c9bfa51c1d6206fbd9a7e1f06f1fc90fdf30d`.

Stage 8A-4 design R1 retained the architecture but was not accepted. Design R2
was independently accepted and closed at `cc58c10`. It freezes source-specific
completeness, orthogonal lifecycle/fill outcomes and fully bound tier-3
correlation. Implementation R1 at `245fea1`, Implementation R2 at `3c445ae`
and Implementation R3 at `5a846f9` were not accepted. Implementation R4 was
independently accepted and closed at `4caf07c`. Durable-composition Design R1
at `80fe35e` retained the architecture but was not accepted because the stable
transition key, post-append covering seal, post-effect arm/kill-switch and hold
settlement semantics were not fully frozen. Durable-composition Design R2 was
independently accepted and closed at
`6ddf54ef9d7f740dc59cd2450e78301be3d068cb`. Implementation Specification R2
was independently accepted and closed at
`dd01253596527d6cff1db11cc32ae3c3348c96a0`. I1 R1 at
`0678354185c52aff1d6e194b16f80b0a2c84a1f0` was not accepted. I1 R2 is
independently accepted and closed at `113d2827`. I2 R1 at `6527619` was not
accepted. I2 R2 at `e04edea` closed the R1 findings but was not accepted because
the Stage 6/7 action and command payload were not exactly cross-bound. I2 R3 is
independently accepted and closed at `90f4605`; it is the private no-append
candidate that closes that seam while leaving the stable-key formula unchanged.
I3 R1 at `a490bbe` was not accepted. I3 R2 at `62e5e05` preserved the durable
mechanics but was not accepted because it exported a hidden raw append,
regressed accepted Stage8A1 authority, inverted the broker-neutral dependency,
and required the lost I2 object for restart. I3 R3 at `3aa2670` was not accepted
because its public sealer remained forgeable and incomplete restart advanced
ordinary Ready/S1 too early. I3 R4 at `4403068` closed those issues but was not
accepted because recovery retained process-local capability/issuer objects.
I3 R5 at `0d1b14f` removed process-local recovery inputs but retained a strict
control-read dependency. I3 R6 is independently accepted and closed at
`593ff25`; it provides the structurally recovery-only issuer and
SIGKILL+corrupt-control proof without weakening normal execution authority.
I4 Design R1 at `06bb09f` was not accepted with zero P0 and three P1 design
ambiguities. Design R2 at `d1a050a` closed them but was not accepted because a
crate-private terminal authority cannot cross the accepted crate boundary. I4
Design R3 is independently accepted and closed at `81727aa`. It retains timestamp-free
ACK facts, exact reuse of the existing Stage7B terminal identity,
current-readiness issuer/scope/lifetime and read-only/no-effect semantics, while
making only the broker-neutral terminal authority public-opaque and externally
nonconstructible. After Design R3 acceptance the required implementation order is:
broker-neutral complete-transition facts, owner-mediated already-S1-covered
terminal authority with no seal repair, timestamp-free ACK facts, private
current-readiness issuer/facade, then deterministic restart/duplicate/expiry and
negative matrices. Implementation R1 `1da0a65` and R2 `6a7f07c` were not
accepted. I4 Implementation R3 is independently accepted and closed at
`4a11688`; it provides the terminal/root-bound I4-only read-only issuer,
independent process-B current-source sampling and ACK-only fallback, with
explicit journal/S1/arm non-mutation proof.

Stage 8A-5 aggregate acceptance is independently accepted and closed at
`bf58b47fdef8af774a4107455dfcc6204e594283`; Stage 8A is formally closed. The
Stage 8B-D R1 architecture was retained at `b3358ba` but the transition package
was not frozen because the default current-tree CI still used an obsolete Stage 5
scanner. GOV-CI-1A retired that terminal authority at the history-preserving merge
`1dea519cbf2affc3d99866fdae66bbddbafefa24` without changing canonical `ci.yml`.
GOV-CI-1B was independently accepted at `13f659f` and merged to `main` by
`7bc9fda`; GOV-CI-1 is closed. Stage 8B-D R2 was independently accepted at
`f296d0b` and merged tree-identically to `main` by `50ed538`. Stage 8B-S R1 at
`a675a77` retained the architecture but was not frozen. Corrective Stage 8B-S R2
at `831eec8` closed the R1 findings but was not frozen because adapter
qualification followed exact P authorization. Stage 8B-S R3 was independently
accepted at `afecc2584593570b62cbe7f00ee81f64d4b9b26b` and merged by
`d1581962666aa82b993854d0642e67bd66624032`. Stage 8B-I at `a52fbca` was not
accepted despite preserving the no-send boundary. I-R2 at `21426ee` was also
not accepted. Corrective Stage 8B-I R3 was independently accepted and merged
exactly at `0af222f252cdc2b4c763c9e04935a5cb5f0c6d65`. The first Stage 8B-IT
candidate `e440539` and corrective R2 at `74d07c8` were rejected. Corrective
IT R3 is the active candidate: the Stage 8A-2 extraction itself consumes an
opaque K4 proof, adapter input fields are private to a sibling capsule, reqwest
automatic retries are explicitly disabled, and the Stage 8A-3 classifier
remains mandatory inside the adapter.
Qualification remains numeric-loopback-only, single-attempt and no-effect,
with no redirect, proxy, retry or production endpoint constructor. Production
operator-arm issuance, real FINAM POST/DELETE effects, Redis live consumption,
broker dispatch, runtime-live, real orders, Stage 8B-P/XE and Stage 12 remain
closed. Independent IT R3 acceptance plus controlled TLS evidence may open
only Stage 8B-P.

### Accepted lineage and transition history

Transition Gate 7→8 R3 specification was the authorized planning target. The
initial `4d1106e` package was rejected without discarding its architecture. R1
was docs/scripts-only and added a current FINAM REST contract snapshot, the
existing-builder-only rule, Day-only initial TIF, endpoint-specific status
semantics, closed `ProvenNoMatch`, fail-closed kill-switch wording, 66 mandatory
acceptance rows and 32 exact negative cases. R1 `f7afc1c` was not accepted
because documented CANCEL 401 and same-request re-execution after
`DefinitelyNotSent` were not explicit. R2 was docs/scripts-only, closed
those two gaps and pinned 68 acceptance rows plus 34 exact negatives, but was
not accepted because the post-acceptance transition authority was
contradictory. R3 is docs/scripts-only, closes that final governance gap, and
pins 69 acceptance rows plus 36 exact negatives. Independent R3 acceptance
opens only Stage 8A-0 contract refresh/freeze for docs/evidence/checkers;
production Rust and 8A-1 through 8A-5 remain closed. Stage 7B-e R4 was
independently accepted and Stage 7B formally closed at
`a1044e0dbe324c722b637498ca80ffafd9f0cbee`. The accepted Stage 7B chain rests on
independently accepted
Stage 7B-d-a-R1 at `8418cfb63ecee6702bf8a2873592b7cad1e711ee`. The Redis-free durable lifecycle
and seal-before-settlement authorization remains paper/mock only. The original implementation candidate
`f71eeb926464f6634d485d5720b25c5e026b40d5` was not accepted; R1 closes exact
current on-disk seal revalidation and a real fsynced B-046 effect witness. The original
design candidate `09a22765ae6ee37b304bfed6492bd103da44360d` was not accepted
as frozen; Design R1 was independently accepted/frozen at
`00cead2989493b44e0d86ead29b95d57a7fbcbe2`. Stage 7B-c-R1 is independently
accepted and Stage 7B-c is closed at
`c57ae8d5f98bbb11df0a81f78262d3916b276d81`; Stage 7B-b-R2 is closed at
`ff3fa2e8908440863b40b838991d4716b33caad4`. Stage 7B-d implementation is
split into lifecycle/seal barrier, atomic Redis settlement and composite
readiness/restart transport slices. The accepted d-a R1 implementation covers
B-043..B-051 and B-054..B-056 with file-backed/SIGKILL/fault witnesses.
Stage 7B-d-b-R1 is independently accepted at
`e0bf9b7d9eb209e19b875f199511a493ddcd0da9`. The d-c candidate attaches only
the isolated paper consumer and implements composite storage/seal/Redis
readiness, external task supervision, fresh per-boot consumer identity,
bounded old-PEL reclaim and restart duplicate/conflict evidence for B-052,
B-053 and B-064..B-070. FINAM
transport, runtime-live and real orders remain closed pending later stages.
The original d-c candidate `c427ad1c83a27e6a80f45c7e09311ffcae26c913`
was not accepted. R1 restores accepted deterministic pre-Stage6 rejection
ACKs without Stage 6 mutation and adds integrated real-service `PaperReady`
and child-process Redis `XAUTOCLAIM` witnesses. R1 at
`9b98c360e1153e79971b5935d03fd0a0bdd1f4f4` was not accepted because
marker-only terminal history was checked only after effect. R2 adds the
read-only pre-admission marker veto and exact duplicate-publication path.
Stage 7B-d-c-R2 is accepted at
`2b6371adb905654e0ddd8b6714159bcef737b577`. Stage 7B-e aggregate closure
assembled exact normative X01-X20, the inherited Stage 7A full gate, real
infrastructure evidence and the 80/80 proof map. R1 at
`422bd1a8b45bfd3397aa588f914494cc11f5c401` was not accepted; R2 at
`8cc72f148032bedda6a0ef86f6edda2c1394abc7` closed its gate, fault-semantics
and proof-map findings but retained a stale B-079 proof. R3 at
`d501d62543cde890bfbb8d8ea0dc878e28a711b2` closed the intended prefix
mutation but was not accepted because the prefix model did not cover a
production item after the test module. R4 pins the exact changed-path set and
full-file SHA-256 of every allowed crate delta and adds the 59th,
post-test-module hidden-Stage8 mutation. The independent final review closed
the aggregate stage with 80/80 proof rows, 20/20 fault rows and 59/59 negative
cases. Stage 8 implementation, runtime-live and real orders remain closed while
the specification is prepared and reviewed. Even after gate acceptance, real
FINAM POST/DELETE remains closed during Stage 8A. If R3 is accepted, only
Stage 8A-0 opens; Stage 8A must then proceed in separately reviewed order
8A-0 through 8A-5. Bounded execution requires a
separate accepted Stage 8B and explicit operator authorization.

Stage 2B is closed as the broker-neutral runtime source migration foundation;
Stages 3, 4 and 5 are accepted/closed. Stage 6 is independently accepted and
closed at `10e357825a701193d964975bb5769bd0745d4986`. Stage 7A is independently
accepted and closed at `2b6d6e90f2350b77fc1d79aa7381e6d9c6566c64`.
Stage 7B-c-R1 composes one file-backed Stage 6 authority, OS locking, recovery
seals and paper restart ownership. Its acceptance opens only the Stage 7B-d
paper-service composition described in
[stage7b-d-design.md](stage-7/stage7b-d-design.md). Stage 8+ remain closed
pending separate acceptance.

## Still blocked

- Runtime-live.
- Redis attachment outside the validated Stage 7A/7B paper namespace.
- FINAM runtime POST/DELETE and broker dispatch.
- Strategy-driven real FINAM orders.
- Stop/SLTP/bracket/replace/multi-leg live behavior.
- RI/RTS and USDRUBF expansion.
- `i64` surrogate adapter without a separate ADR.

## Stage 8 entry sequence

Transition Gate 7→8 R3 and Stage 8A-0 R1 are independently accepted. The active
candidate is Stage 8A-1 protected capability/strict preflight. Stage 8A-0
contract parity remains `MATCH`.

The reviewed sequence remains:

1. 8A-0 — contract/provenance freeze (accepted and closed).
2. 8A-1 R3 — protected capability with current disk-seal, dispatch-ready
   durable authority, trusted issuer root, request-keyed one-arm protection and
   symmetric PLACE/CANCEL continuation revalidation (accepted and closed).
3. 8A-2 through 8A-5 — separately reviewed builder invocation, outcome and
   reconciliation slices, still bounded by their own gates.
4. 8B — separately accepted bounded execution authority.

No step in 8A-0 opens FINAM POST/DELETE, broker dispatch, runtime-live or real
strategy orders.

Stage 8A-0 R1 is independently accepted and closed at `c949d7f`. Stage 8A-1 R3
is independently accepted and closed at `1ff0415`. Its opaque continuation is
the only legal input to the independently accepted Stage 8A-2 R1
builder/no-send composition at `16180ac`. Stage 8A-3 R1 at `aeef1bd` was not
accepted. The active slice is its narrow Stage 8A-3 R2 corrective candidate,
which may contain only a distinct endpoint-specific local HTTP observation
classifier plus the required R1 closure hardening. Acceptance of its exact
artifact may open Stage 8A-4 only. Every
network-send, Redis-live, broker-dispatch, runtime-live and real-order surface
remains closed.

Stage 8A-3 R2 was independently accepted and closed at `012c9bf`. That exact
acceptance opens reconciliation planning only. Stage 8A-4 is split into an
independently reviewed design freeze followed by a separately reviewed pure
implementation and durable-composition closure. The design candidate changes
no production Rust and opens neither Stage 8A-5 nor any execution surface.
