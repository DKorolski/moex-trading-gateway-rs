# Stage 5F-b0 — source reachability and fingerprint audit

Status: complete development audit; not release authority  
Date: 2026-07-31  
Development base: `0fcab80e4c13822891eeae9bceb0f895b4d453a9`  
Accepted B3F source: `e14654f7129aa61011931306140a3bfefe2fcfbc`

## Outcome

All 34 rows and all 16 groups from the external Stage 5F design matrix remain
in scope. The audit does not treat every row as the same kind of evidence:

- 27 rows are source-reachable accepted transitions through one real Hybrid
  callback and one canonical B3F settlement;
- 3 rows are existing capability-chain blockers before callback;
- 4 rows are negative-only terminal proofs after one callback and one
  settlement attempt;
- no row needs ACK, order, position, timer, restart or other Stage 5G feedback.

This classification preserves the requested 34-row coverage without adding a
production seam to manufacture an impossible runtime state.

The machine-readable authority for this audit is
`stage5f-b0-source-reachability-inventory.json`.

## Decisions that resolve specification ambiguity

### F21, F22, F28 and F29 remain accepted source scenarios

These rows do not require feedback during the Stage 5F invocation. Their
pre-state is loaded through the already accepted Stage 5D restore contour and
the scenario invokes one final live M10 bar:

- F21 seeds `last_processed_bar_ts`; the existing `Strategy::on_bar` duplicate
  guard returns an empty vector and cannot create a second cycle;
- F22 seeds a terminal flat/clean state; a later bar can create the next cycle
  through the ordinary orchestrator path;
- F28 and F29 seed complete deferred-entry/deferred-exit tuples; the existing
  `maybe_reissue_deferred_entry` / `maybe_reissue_deferred_exit` branches are
  evaluated by the ordinary bar callback.

The fixture describes accepted initial state. It does not replay an ACK or
invent broker feedback inside Stage 5F.

### F31–F34 are negative terminal proofs

F31–F34 are retained, but they are not positive source-produced trading
scenarios:

- F31 uses the existing test-only accepted-bar corruption seam to exercise the
  callback-validation terminal;
- F32 mutates retained B3F chronology or identity after the real callback and
  before the real settlement preflight;
- F33 mutates the retained callback intent only in test code so Stage 5C intent
  validation fails;
- F34 creates a test-only mismatch between the retained intent and final
  pending-request state so Stage 5C fails with the typed pending mismatch.

Each proof still requires `callback_count == 1`,
`settlement_attempt_count == 1`, no accepted transition and a null accepted
post-state fingerprint. None may be used as a golden source semantic vector.

### Fingerprints are intentionally distinct

The transition fingerprint used by Stage 5F is exactly:

```text
sha256(serde_json::to_vec(StrategyState))
```

It is implemented by `stage5c_state_fingerprint` and binds callback/batch
state. The separately existing `stage5c_semantic_payload_fingerprint` binds
the persisted-owned semantic projection. The two digests have different input
domains and are never required to be equal.

## Controlled observation authority for 5F-b1/5F-c

No observer is implemented by this audit. The only approved future test-only
surface is:

1. one `#[cfg(test)]` module declaration in
   `crates/strategy-runtime-core/src/lib.rs`;
2. one mutually exclusive `#[cfg(test)]` branch in
   `BrokerNeutralHybridStrategy::on_broker_bar` immediately around the existing
   `Strategy::on_bar(self, &context, &bar)` expression;
3. one new private module
   `crates/strategy-runtime-core/src/stage5f_atomic_hybrid_semantics.rs`.

The non-test branch must retain the accepted callback expression. The test
branch may invoke that same expression exactly once, pass the returned vector
by immutable borrow to the observer and return the same vector unchanged.

The observer contract is:

- crate-private and compiled only under `cfg(test)`;
- thread-local or explicitly scenario-scoped;
- reset before callback and consumed exactly once after callback;
- no `Clone`, `Copy`, `Serialize`, `Deserialize`, `Debug`, `Display`, `Default`
  or public constructor on its capability/result;
- no raw account, broker order ID, stop ID, cycle ID or comment in exported
  evidence;
- no control-flow decision and no dependency from production code;
- no second callback and no alternate Stage 5C/B3F route.

Changing frozen `stage5c_paper_host.rs` or `stage5e_no_io_lifecycle.rs` to add
an observer/getter is not authorized.

## Why GitHub governance is not part of this substage

This branch is development evidence, not release authority. Hosted required
checks and protected-PR authority are intentionally deferred by
`adr-stage5f-local-development-governance.md`. The b0 audit therefore protects
the semantic boundary locally and records exact source provenance, while the
release-governance decision remains a separate final-review item.

## Closed surfaces

Redis consumption, FINAM transport, HTTP POST/DELETE, command dispatch, broker
send, runtime-live, deployment, protective-order implementation and Stage 5G
feedback all remain closed.

