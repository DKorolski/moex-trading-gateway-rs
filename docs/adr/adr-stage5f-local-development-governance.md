# ADR: Stage 5F local functional development before release governance

Status: accepted for development only  
Date: 2026-07-31  
Recovery base: `0fcab80e4c13822891eeae9bceb0f895b4d453a9`  
Accepted B3F predecessor: `e14654f7129aa61011931306140a3bfefe2fcfbc`

## Context

The independently attested administrative recovery restored the tracked tree of
the accepted Stage 5F generation-3 anchor. The rejected post-anchor changes
were rejected for authority-chain and protected-PR reasons, not because of a
runtime regression. Independent review separately accepted the technical
content of the portable scanner, exact-B3F dependency prefetch and parent-owned
handoff cleanup.

Stage 5F is an offline, paper-only semantic stage. It does not consume Redis,
call FINAM, dispatch broker commands, open live runtime or authorize a deploy.
Making repository branch protection and hosted required-check policy a
prerequisite for every local semantic-development commit would add delivery
cost without changing those semantics.

## Decision

1. `main` remains at the independently attested recovery commit while Stage 5F
   functional work is developed on `stage5f-functional`.
2. Development commits and SHA-bound source archives may be pushed for backup
   and review without claiming release or merge authority.
3. The technically reviewed portable scanner, exact-B3F dependency preparation
   and parent-owned handoff cleanup may be reused on the development branch.
4. GitHub PR-only enforcement, required checks, independent-approval policy,
   administrator no-bypass and authority rotation are deferred to an
   integration/release gate before the development branch may update `main`.
5. This deferral does not waive local safety checks. The no-`rg` scanner matrix,
   Stage 5D freeze matrix, inherited B3F provenance/UI gates, Stage 5F gates and
   Rust checks remain required at the relevant functional milestones.
6. Stage 5G, Stage 5H, Redis consumption, FINAM transport, dispatch, live
   runtime and deployment remain closed.

## Stage 5F contract corrections

The external completion specification and 34-row matrix are accepted as design
inputs with the following recorded corrections.

### Fingerprints

The project has two distinct Stage 5C state fingerprints and they must not be
conflated:

- `stage5c_state_fingerprint` is SHA-256 of the complete serialized
  `StrategyState`; it is the canonical callback post-state and B3F batch-state
  fingerprint for Stage 5F;
- `stage5c_semantic_payload_fingerprint` hashes the persisted-owned semantic
  projection and remains the restore/persistence semantic fingerprint.

Both are deterministic evidence, but equality between the two algorithms is
not an invariant.

### Acceptance-matrix reachability

The 16 semantic groups are mandatory. The proposed 34 rows are a design
baseline until a source-reachability audit classifies every row as one of:

- accepted through the sole production callback and settlement route;
- blocked before callback by the existing capability chain;
- terminal after one real callback and one real settlement attempt;
- negative-only because the state is not source-producible;
- deferred to Stage 5G because it requires ACK/order/position/timer/restart
  feedback.

No production hook or alternate callback route may be added merely to force an
unreachable row to pass.

### Observation seam

Any ordered-intent observation seam must be `#[cfg(test)]`, crate-private,
scoped to one scenario and single-consume. It must observe the exact vector
returned by the real callback without cloning a capability, invoking a second
callback, changing control flow or exposing raw account, broker-id or comment
data.

### Review cadence

Functional review is requested after the 5F-b contract, after the 5F-c sole
route attachment and for the aggregate 5F-d/5F-e closure. Internal d1/d2/d3
commits retain separate inventories and results but do not require separate
external reviews unless their diffs expose a new architectural question.

## Reopening deferred governance

Before merge to `main`, deployment, Stage 5G/5H activation or any live-capable
work, the final review must decide one of the following:

- reinstate and complete the reviewed R10a/R10b protected authority sequence;
- accept a replacement repository-governance protocol through a separate ADR;
- reject the development branch without affecting the attested `main` anchor.

Until that decision, this branch is development evidence, not release
authority.

