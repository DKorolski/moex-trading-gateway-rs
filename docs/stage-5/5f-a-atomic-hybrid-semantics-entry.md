# Stage 5F-a — atomic Hybrid semantics entry and inheritance gate

## Status and predecessor

Stage 5F-a starts only after the accepted Stage 5E-B3F closure at source ref
`e14654f7129aa61011931306140a3bfefe2fcfbc`.

The Stage 5E descriptor remains an immutable closure descriptor for B3F. It is
not repointed to Stage 5F, because the accepted B3F checker deliberately pins
the exact B3F review scope. Stage 5F has its own descriptor and gate; that gate
executes the accepted B3F checker and production UI harness from the exact
accepted snapshot before it accepts Stage 5F-a.

The canonical CI path follows the same inheritance model. It fetches the
accepted predecessor, runs the Stage 5F gate and both Stage 5F negative
harnesses on the current tree, then invokes the 580-case B3F provenance
harness only through the shared detached-snapshot wrapper. The wrapper is
fail-closed when the accepted commit is unavailable, when its checkout is not
exactly `e14654f7129aa61011931306140a3bfefe2fcfbc`, or when the harness does
not report exactly 580 `PASS` cases. The legacy Stage 5E gate must never run on
a Stage 5F head.

Stage 5F-a-r3 seals the canonical CI execution order as well as the inherited
B3F provenance bytes. Its first Stage 5F CI action verifies the exact
snapshot-verifier authority and that verifier immediately checks and executes
the exact detached-snapshot wrapper. No repository-owned executable runs
between wrapper verification and use. The reviewed execution-authority set also
pins the workflow, wrapper, Stage 5F gate, CI verifier and both Stage 5F
negative harnesses. Those current-tree digests remain useful handoff evidence,
but are not an immutable authority root.

Stage 5F-a-r4 therefore adds the external authority boundary required for later
pull requests. `Stage 5F Base Authority` runs only as a protected-base
`pull_request_target` workflow with read-only permissions. It checks out the
exact accepted `8ce0acd60c7cb5cc5d25a27f6553077240658b57` snapshot, executes
the existing checker/verifier and the detached B3F provenance harness only from
that snapshot, and treats the pull-request head only as byte data. Before any
accepted authority executable runs, it rejects a byte or symlink difference in
the frozen R3 authority set; it also compares the workflow and CODEOWNERS file
with the protected pull-request base. The workflow never runs a candidate
script. Branch protection must require this status check and a CODEOWNERS review
for its authority scope. A two-case harness reproduces the coordinated
workflow/verifier/entry-checker/inventory/handoff rebinding shape and proves it
is rejected without executing candidate code.

The CI negative matrix proves that `continue-on-error`, `if: false`, a direct
raw provenance call, a forged PASS producer, a second checkout, suppressed
harness failure, an Actions-only wrapper replacement, a Stage 5F-negative
wrapper replacement, a forged negative case count and a wrapper mutation
before verified execution all fail before a handoff can be accepted. The
handoff builder rechecks the complete execution-authority set immediately
before its source-tree manifest is created, and archive safety rechecks the
same set from archived bytes.

This is a governance and atomic-contract entry slice. It adds no Rust runtime
behavior, does not attach an intent sink, and does not alter the accepted B3F
source files.

## Sole route

Stage 5F may prove only this existing route:

```text
Stage5eStage5cAuthorizedCallbackMaterial::invoke_authorized_callback_once
  -> BrokerNeutralHybridStrategy::on_broker_bar
  -> HybridIntradayRuntimeStrategy::on_bar
  -> high180 update and riskgate shadow/update
  -> HybridOrchestrator::on_bar_with_mr_override / on_bar
  -> ordered broker-neutral semantic intents
  -> accepted B3F settlement
```

There is no alternate direct Stage 5C callback route, second orchestrator,
BO-only acceptance route, or MR-only acceptance route.

## Fixed Stage 5F target

The first atomic semantic attachment is constrained to:

```text
instrument: IMOEXF
profile: imoexf_primary_riskgate_high180_lb120
market input: canonical final M10
execution mode: paper-only
```

The ALOR oracle may later be a comparison input in Stage 5H. It must not become
the decision source of the migrated runtime in Stage 5F.

## Atomic transition contract

One accepted semantic bar has exactly one pre-state and one post-state
fingerprint. Its observable result is only a redacted paper semantic transition:

```text
pre_state_fingerprint
  + accepted canonical final M10 bar
  + exact broker-neutral Hybrid context
  -> one callback
  -> ordered semantic-intent vector
  -> one B3F settlement
  -> post_state_fingerprint
```

The contract preserves exact request identities within one deterministic
contour. It records intent order, owner, cycle, side, pending/deferred state and
applicable riskgate state, but exports neither raw broker payloads nor a live
order command.

## Required full Stage 5F matrix

No internal sub-slice may claim Hybrid parity on its own. Stage 5F acceptance
requires the complete atomic matrix:

- no signal / zero intent;
- BO long and short entry candidates;
- BO exit and no-overnight/EOD behavior;
- high180 MR long and short entry candidates;
- MR time, target and stop exit representatives;
- simultaneous BO/MR candidates with one deterministic winner;
- BO-owned state suppressing MR and MR-owned state suppressing BO;
- one owner, one cycle and no overlap;
- riskgate `normal_append` and missing/inconsistent-state blocking behavior;
- pending/deferred initial state behavior without invented broker feedback;
- terminal callback or B3F settlement results producing no accepted semantic
  transition.

Each accepted row must bind the exact ordered intent vector and pre/post state
fingerprints. ACK, order, position, timer and restart feedback are excluded and
belong to Stage 5G.

## Closed surfaces

Stage 5F-a and every Stage 5F atomic scenario remain paper-only. The following
are closed:

```text
Redis consumption
FINAM transport
dispatch
broker execution
runtime-live
real order endpoints
durable persistence opening
direct Stage 5C callback route
second orchestrator
partial BO-only or MR-only parity acceptance
ACK/order/position/timer feedback
```

## Stage 5F delivery order

1. **5F-a** — this inherited entry contract and gate.
2. **5F-b** — fixture/input and redacted fingerprint schema for the complete
   atomic matrix.
3. **5F-c** — controlled invocation of the sole existing route in paper mode.
4. **5F-d** — complete BO/MR/riskgate/arbitration atomic matrix and negative
   invariants.
5. **5F-e** — Stage 5F acceptance package and review.

Stage 5G may begin only after Stage 5F acceptance. Stage 5H remains the
same-input ALOR differential replay gate.
