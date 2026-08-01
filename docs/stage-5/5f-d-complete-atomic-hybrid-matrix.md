# Stage 5F-d complete atomic Hybrid matrix

Status: frozen review candidate  
Accepted predecessor: `e9bcc05deca93e6683abca9b9688b1a814839120`  
Target: IMOEXF Hybrid, final M10, paper-only, no send

## Outcome

Stage 5F-d executes the complete source-valid atomic Hybrid matrix through the
sole accepted callback and Stage 5C/B3F settlement route. The frozen matrix has
34 rows in 16 official groups:

- 26 accepted callback transitions;
- 1 active-profile structural invariant;
- 3 typed blockers before callback;
- 4 typed terminal outcomes after callback.

Every accepted row pins the exact pre-state fingerprint, ordered redacted
intent vector, request ID order, accepted post-state fingerprint and settlement
identity. Blocked and terminal rows pin their exact callback/observer/settlement
cardinality and typed reason.

The machine-readable authorities are:

- `docs/stage-5/stage5f-d-scenario-inventory.json`;
- `docs/stage-5/stage5f-d-golden-results.json`;
- `tests/fixtures/stage5/stage5f/v2/scenarios/atomic-hybrid-scenarios.json`.

## Delivery slices

The single review package keeps the requested internal separation:

- 5F-d1 (`F01`–`F15`): no-signal, BO and MR entry/exit semantics;
- 5F-d2 (`F16`–`F22`): arbitration, owner and cycle invariants;
- 5F-d3 (`F23`–`F34`): riskgate, pending/deferred and terminal semantics.

The slices share one golden authority but remain separately inventoried. No
slice claims Stage 5F aggregate closure.

## Source-validity decisions retained

The accepted R2/R3 corrections remain normative:

- `F12`–`F15` prove favorable/adverse bar extremes and no bar-owned protective
  completion; target/stop completion remains Stage 5G lifecycle work;
- `F16` proves from the active configuration that BO and High180 entry windows
  are disjoint, so a synthetic simultaneous-winner callback is not fabricated;
- `F19` retains its paired source-valid BO control and MR-owner suppression
  proof;
- `F26` consumes a real canonical working-order snapshot through restored
  private state before proving stale pending-entry preservation.

No production formula, strategy parameter or callback route was changed.

The legacy forbidden-surface scanner remains sealed to its accepted B1 source
inventory. Stage 5F-d does not rebaseline it to recognize the later test-only
module; the handoff executes that scanner through the immutable B1 snapshot,
then applies the current-tree Stage 5F-d checker and mutation matrix. This
preserves both authorities without weakening either one.

## Riskgate and terminal semantics

`F23` verifies `normal_append` shadow advancement and explicitly does not treat
the gate as entry-enforced. `F24`, `F25` and `F30` fail before callback on exact
authoritative-evidence defects. `F31`–`F34` each invoke the callback once and
settle once into their exact terminal class; none is reclassified as accepted.

## Determinism

The Rust harness:

1. materializes each row from strict typed fixtures;
2. invokes only the accepted callback route;
3. observes a redacted ordered vector once;
4. performs at most one settlement attempt;
5. compares all 34 source results with the frozen golden JSON;
6. repeats the full matrix and requires byte-identical output.

The frozen results array SHA-256 is
`e85f15912e3dd97e2a41a3d2617bc9b560769aa964e158b0129bb0d2c89e0f17`.

## Closed surfaces

Stage 5F-d does not open Redis, FINAM transport, HTTP order endpoints,
dispatch, broker send, runtime-live, real orders or protective-order
lifecycle. ACK/order/position/timer/restart feedback remains owned by Stage 5G.

## Acceptance boundary

This package may be accepted as Stage 5F-d only after the current-tree
Stage 5F-d checker and negative mutation matrix pass together with the
immutable R3 snapshot gate, debug/release focused tests and project regression
gates. Stage 5F itself remains open until Stage 5F-e aggregate acceptance.
