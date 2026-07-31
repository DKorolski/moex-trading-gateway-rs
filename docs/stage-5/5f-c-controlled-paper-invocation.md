# Stage 5F-c — controlled paper invocation

Status: candidate implementation; independent review required before Stage 5F-d
Accepted input base: `86b43c448fb65a3c54b6118d04d3f40e08e74ad7`
Accepted B3F settlement base: `e14654f7129aa61011931306140a3bfefe2fcfbc`
Mode: IMOEXF Hybrid, canonical final M10, paper-only, no send

## Outcome

The minimum seven-row Stage 5F-c characterization matrix now executes through
one source `Strategy::on_bar` call and one canonical B3F settlement attempt.
The observer sees the exact returned vector by immutable borrow and retains
only the redacted ordered projection. Candidate output is deliberately not a
golden: Stage 5F-d remains closed until this code and its two input corrections
are reviewed.

| Row | Disposition | Callback | Observer | Settlement | Source result |
| --- | --- | ---: | ---: | ---: | --- |
| F01 | accepted | 1 | 1 | 1 | zero intents |
| F02 | accepted | 1 | 1 | 1 | BO long market entry |
| F04 | accepted | 1 | 1 | 1 | BO long market exit |
| F24 | blocked before callback | 0 | 0 | 0 | riskgate authority missing |
| F31 | terminal after callback | 1 | 0 | 1 | callback validation error before `on_bar` |
| F32 | terminal after callback | 1 | 1 | 1 | B3F chronology mismatch |
| F33 | terminal after callback | 1 | 1 | 1 | Stage 5C intent validation failure |

Candidate results are in
`stage5f-c-candidate-results.json`. Their results-array SHA-256 is
`1a1d2b39369156ad6f75f68c3218b086047c4470270d04c957427c98dd933910`.

## Executed route

```text
fixture-owned BrokerNeutralHybridStrategy
  -> typed Stage 5C recovered/accepted-bar ownership
  -> existing Stage 5E schedule window, B3B and B3C transitions
  -> issue_stage5e_callback_authority_at
  -> invoke_stage5e_authorized_paper_callback_at
  -> BrokerNeutralHybridStrategy::on_broker_bar
  -> exact Strategy::on_bar expression
  -> test-only immutable observer
  -> original opaque escrow
  -> validate_and_settle_stage5e_paper_callback_escrow
```

The accepted path has exactly one callback invocation site and one settlement
site in the owning harness. Request IDs from the settlement receipt are bound
ordinal-by-ordinal to the observed vector. Accepted rows bind complete pre/post
state fingerprints, ordered vector hash and settlement identity. Terminal rows
cannot export an accepted post-state or source semantic vector.

## Test-only boundary

The implementation adds four marked `cfg(test)` regions:

- private module declaration in `lib.rs`;
- immutable observation immediately after the exact callback expression in
  `hybrid_intraday_runtime.rs`;
- fixture-ownership factory in `stage5c_paper_host.rs`;
- B3C ownership factory and callback-validation seam in
  `stage5e_no_io_lifecycle.rs`.

The checker removes those marked regions and requires every remaining byte in
the four previously accepted files to equal B1. The new observer scope and
result are crate-private, linear, non-serializable and non-debuggable. A second
observation before consume fails closed; dropping an unconsumed scope clears
only its own thread-local generation.

## Source-validity corrections

The accepted B1 input catalogs are immutable. Running source semantics exposed
three input defects, therefore a separate SHA-bound candidate overlay is used:

1. F02 originally closes at 10:10 MSK, but production BO has
   `wait_hours = 3.0`; the same market shape is moved to 12:10 MSK.
2. F04 originally closes at 99.5 while the production long `stop2` is 98.6;
   its low/close become 98.0/98.5 so the declared exit is source-reachable.
3. F04 state uses `bo00000001`, which is ten bytes but fails the production
   ASCII-hex cycle parser; the overlay explicitly maps it to `b000000001`.

Overlay SHA-256:
`3639d59331716e4247860cc3a6aa7f6032e677e63f30fc31b6e4b1eb50902c21`.
No strategy parameter, production source or accepted B1 fixture was rewritten.

## Deliberate review decisions

### Additional ownership seams

The B0 observation design named only `lib.rs`, the exact callback file and a
new private module. Once implemented, Rust ownership/privacy made it
impossible to carry an arbitrary fixture-owned strategy into the already
private Stage 5C/5E type-state chain without one of these choices:

- add production getters/constructors;
- duplicate or reconstruct private capabilities in the harness;
- add narrowly marked crate-private `cfg(test)` ownership factories.

The third option was chosen because it is the smallest reversible surface and
does not expose production API. This is an explicit extension of the B0 design
authority and must be accepted or rejected by the Stage 5F-c reviewer.

### Stage 4/5D composition boundary

The harness materializes the real `HybridIntradayRuntimeStrategy`, applies its
real Stage 5D runtime-private extension API, injects the selected riskgate state
and then enters the real Stage 5C/5E callback chain. It does not re-run the
entire persistence-envelope, broker-truth bootstrap and runtime-restored
constructor sequence for every Stage 5F row. Those transitions retain their
separately accepted Stage 5D proofs.

This was chosen to keep the semantics test atomic and avoid copying a second
large Stage 5D test orchestrator. It is a compositional proof, not a claim that
the full restart path was re-executed in Stage 5F-c. Review must decide whether
this satisfies the technical specification's “real typed Stage 4/5D/5E chain”
requirement or whether a later narrow Stage 5D fixture-entry bridge is required
before golden freeze.

### Historical scanner inheritance

The old scanner freezes the exact Stage 5B source-file inventory and therefore
cannot truthfully accept a new Stage 5F test module. It is not widened. The
functional gate archives immutable B1 and runs the historical scanner plus its
13- and 17-case negative matrices there. The current head is checked by the
Stage 5F-c checker, its 37-case mutation matrix and Rust tests. This preserves
historical authority without turning local functional development into a CI
redesign.

## Closed surfaces

Redis, FINAM transport, HTTP POST/DELETE, dispatch, broker execution,
runtime-live, deployment, ACK/order/trade/position/timer/restart feedback and
protective-order implementation remain closed. No production runtime behavior
depends on the observer.

## Review request

The Stage 5F-c review should explicitly decide:

1. whether the two extra test-only ownership factory regions are acceptable;
2. whether the F02/F04 source-validity overlay can become the basis for future
   golden fixtures;
3. whether the compositional Stage 5D precondition is sufficient or a full
   Stage 4/5D constructor replay is required before Stage 5F-d.

Until all three are resolved, candidate results stay non-golden and Stage 5F-d
does not start.
