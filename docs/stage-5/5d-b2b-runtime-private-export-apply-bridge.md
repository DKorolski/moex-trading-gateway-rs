# Stage 5D-b2b-a — runtime-private export/apply bridge foundation

Status: review candidate.

Stage 5D-b2a closed the strict persistence schema and validated-envelope
capability. Stage 5D-b2b-a opens the first controlled implementation slice:
runtime-private extension export/apply, still without Redis, FINAM, broker
transport, command dispatch, runtime-live or real order execution.

## Scope

Implemented:

- public `stage5d_apply_runtime_private_extension(...)` transition;
- input requires `Stage5dValidatedPersistenceEnvelope`;
- output is opaque `Stage5dPrivateStateAppliedPaperStrategy`;
- recoverable block is represented by opaque
  `Stage5dRuntimePrivateApplyBlocked`;
- block exposes only redacted reason and preserves the input loaded capability
  internally;
- wrapper additive region now exports/applies runtime-private DTO fields:
  pending entry payload, partial-entry timer, pending-exit context,
  bracket-reconciliation timer, cleanup retry state, last processed bar
  watermark and full runtime pending riskgate finalization vector.

Not implemented in this slice:

- Stage 5D bootstrap wrapper;
- authoritative riskgate injection;
- final return to `Stage5cRuntimeStateRestoredPaperStrategy`;
- broker working-set authority restoration;
- Redis/FINAM/transport/dispatch/runtime-live.

## Working-set ownership

`expected_working_sets` remains a non-authoritative hint. The apply bridge does
not rehydrate runtime working maps from persistence. Actual active objects must
come from broker truth in a later gate.

## Evidence

Regression tests prove:

- Stage 5C scalar restore initially exposes only the first pending riskgate
  finalization;
- Stage 5D runtime-private apply restores the full pending finalization vector;
- pending-entry private payload and partial-entry timer survive apply/export;
- invalid private-extension shape blocks before mutation and preserves the
  previous runtime export.

The Stage 5D additive manifest now labels this baseline as `5D-b2b-a` and pins
the updated public API surface including the first public Stage 5D transition.
