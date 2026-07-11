# Stage 5B-2b — exclusive facade and instrument validation

Status: review candidate.

Date: 2026-07-11.

## Outcome

The accepted mechanical Hybrid wrapper remains unchanged in trading meaning,
but its host boundary is now fail closed:

- source-compatible host, event and state modules are crate-private;
- downstream code can use only the broker-neutral root facade;
- bar, order, stop-order and position callbacks require exact equality between
  context and payload `InstrumentId`;
- all instrument-bearing callbacks require the configured target symbol;
- timer callbacks require the configured context target;
- bars additionally require `is_final = true` and `timeframe_sec = 600`;
- validation returns `HybridRuntimeCallbackValidationError` before source
  callback invocation or state mutation.

Three compile-fail doc tests lock the private source seam. Four regression tests
lock mismatch rejection, no protective/emergency intent on mismatch, unchanged
state after rejection and deterministic request IDs in the validated target
account/symbol namespace.

## Deliberately unchanged

- exact BO/MR/high180 formulas and arbitration;
- riskgate calculations and finalization;
- bracket and residual-repair semantics;
- ACK synthetic context, because the accepted source callback ignores it;
- runtime host and FINAM command-consumer attachment;
- all live/send and real POST/DELETE authorization.

## Next gate

After acceptance, Stage 5C may start fixture-backed parity and paper/mock host
integration through `BrokerNeutralHybridStrategy` only. Stage 5D retains legacy
numeric state-ID migration and restore policy.
