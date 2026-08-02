# ADR: Stage 5G-c R2-c-a R2 deterministic terminal-fill boundary

Status: review candidate

Base: `d1b3116ef0b2bdcedbcfd1888f78b2d301a3c654`

Accepted architectural direction: the R1 validation/state-coherence work is
retained, while the two source-reachable unsafe outcomes identified by the
2026-08-02 review are closed before R2-c-b.

## Context

R1 could process a partially filled Exit through the mature position callback.
That callback used process wall clock to decide whether a persisted bracket
reconciliation timer still owned the residual. Inside the grace interval the
callback intentionally emitted no duplicate Exit, but R1 treated the empty
vector as a terminal failure after consuming the linear capability.

R1 also admitted `Canceled` or `Expired` with `filled_qty == order.qty`. That
status/fill combination is outside the non-full terminal authority and could
turn incomplete restored state into an unowned, unprotected nonzero position.

## Decision

The terminal status/fill matrix is fail closed:

- `Rejected` requires zero fill;
- `Canceled` and `Expired` require `0 <= filled_qty < order.qty`;
- `filled_qty == order.qty` under `Canceled` or `Expired` returns the typed
  `FullFillStatusContradiction` and preserves the original resolved
  capability. Normal full execution remains owned by the existing
  `Filled`/position lifecycle.

Before mutation, R2 proves that the request still belongs to the exact private
source payload. Entry requires the original owner, side and active cycle;
Exit requires an owned nonzero source position, pending Exit payload and active
cycle. A request ID alone cannot authorize a position transition.

Settlement uses an isolated candidate copy of the crate-private runtime. ACK,
position, generated-intent policy, state coherence, owner/cycle invariants and
escrow consistency are checked before commit. Any pre-commit failure returns
the original resolved capability unchanged.

For partial Exit, bracket grace is evaluated from canonical broker evidence
time, never from `Utc::now()`:

- inside grace, exact broker position is applied, the original request is
  terminalized, no duplicate Exit is emitted, the timer is preserved and the
  result is honestly `ReadyForTimer`;
- after grace, the residual emergency Exit is retained in the existing Stage
  5C generated-intent escrow.

The timer callback now synchronizes its already-mutated private pending fields
into `StrategyState` before Stage 5C validates the generated batch. This is a
lifecycle publication fix, not a new trading decision.

Three inherited Stage 5E parity tests used the process date and failed on
weekends before reaching their settlement assertions. A separately
marker/digest-pinned, test-module-local clock shadows only those fixtures with
a fixed weekday. No production clock or callback authority is changed.

## Evidence path

Focused witnesses use the real path:

```text
accepted Stage 5F semantic callback
  -> Stage 5C intent escrow
  -> Stage 5G-b Accepted or Submitted -> Recovered ACK
  -> R2 terminal validation/transactional settlement
  -> ReadyForTimer or generated-intent escrow
```

The matrix covers zero fill, partial Entry, partial Exit both inside and after
bracket grace, full-fill contradictions for Entry/Exit and Accepted/Confirmed,
corrected retries, owner/cycle preflight, candidate rollback and deterministic
replay fingerprints.

## Consequences

The normalized public API remains unchanged. R2 adds no serialization,
transport, Redis consumer, FINAM call, broker dispatch or live authority.
Stage 5G-c R2-c-b, Stage 5G-d, runtime-live, real orders, Stage 6, main merge
and deployment remain closed pending separate review.
