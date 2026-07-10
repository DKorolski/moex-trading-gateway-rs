# Stage 4D — FINAM read-only broker-truth source normalization

Status: implemented for review.

Date: 2026-07-10.

## Goal

Stage 4D connects the FINAM read-only mapper to the accepted Stage 4C bootstrap
validator without expanding the live execution boundary.

The slice normalizes FINAM read-only evidence into broker-neutral
`BrokerTruthSnapshot` plus explicit source/freshness metadata. It is still a
bootstrap/readiness layer, not runtime-live.

## What this stage adds

- `FinamStage4ReadonlySourceEvidence` and
  `FinamStage4ReadonlySourceEvidenceSet` for explicit per-source status:
  `Present`, `Missing`, `Unavailable`, `DecodeFailed`, `Incomplete`.
- `build_finam_stage4_broker_truth_bootstrap(...)`, a FINAM read-only wrapper
  around `validate_stage4_broker_truth_bootstrap(...)`.
- Per-section freshness propagation for positions, orders, trades, cash,
  instruments, and schedule.
- Explicit schedule/session evidence: schedule state is only derived from a
  present schedule source that matches the target instrument. It is never
  inferred from `BrokerTruthSnapshot.received_ts`, and a schedule from another
  symbol is treated as incomplete source evidence plus `UnknownSchedule`.
- The returned package preserves the original
  `FinamStage4ReadonlySourceEvidenceSet` for auditability instead of exposing
  only the aggregate source status.
- Placeholder `BrokerTruthSnapshot` semantics when the FINAM account/order
  read-only source cannot be constructed because source is missing,
  unavailable, or decode-failed. The placeholder allows the Stage 4C validator
  to produce structured blockers instead of requiring raw source.
- FINAM broker-truth mapping now preserves zero-quantity position rows for
  Stage 4 diagnostics. The portfolio snapshot mapper still keeps its existing
  open-position-only behavior.

## Fixture-backed cases

The Stage 4D test matrix covers:

- all broker-truth source statuses;
- zero-quantity position diagnostics;
- explicit missing schedule/source evidence;
- active target order without restored runtime/adoption;
- unknown target order status;
- terminal target order as diagnostic, not active blocker;
- target trade with broker order id but without restored runtime correlation;
- target trade without broker order id / runtime correlation;
- missing and ambiguous instrument identity;
- schedule symbol mismatch;
- stale positions/orders/trades freshness;
- unavailable and decode-failed source using placeholder broker truth.

## Safety boundary

Stage 4D does not authorize:

- continuous runtime-live trading;
- `command-consumer-to-real-FINAM`;
- strategy-driven real FINAM orders;
- runtime `LiveReady`;
- real POST/DELETE order endpoints;
- Stop/SLTP/bracket/replace/multi-leg.

`FinamStage4BrokerTruthBootstrapPackage.no_live_authorization` remains `true`.
All Stage 4D checks are read-only/source-normalization checks.

## Acceptance checklist

- Schedule source is target-bound and a wrong schedule symbol cannot produce
  `BootstrapReady`.
- Per-section source evidence is preserved in the Stage 4D package.
- Zero-quantity FINAM positions remain visible in broker-truth diagnostics.
- Missing, unavailable, decode-failed, and incomplete sources produce structured
  Stage 4C blockers.
- No live/execution surface changes are introduced.

## Follow-up

Stage 4D has been accepted. The next planned slice is Stage 4E:
`BrokerTruthSnapshot` -> `RuntimeHostBootstrapSnapshot` application evidence.
That should still remain paper/mock/read-only until the later Stage 4 lifecycle
gates are accepted.
