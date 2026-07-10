# Stage 4E — runtime bootstrap application evidence

Status: implemented for review with P1 consistency guard.

Date: 2026-07-10.

## Goal

Stage 4E proves the application boundary between validated broker truth and the
runtime bootstrap notification.

This stage does not attach a strategy runtime, does not emit runtime-live, and
does not create any FINAM order path. It only decides whether the already
validated Stage 4C/4D broker-truth bootstrap can be applied as a
`RuntimeHostBootstrapSnapshot`.

## What this stage adds

- `evaluate_stage4_runtime_bootstrap_application(...)`.
- `Stage4RuntimeBootstrapApplicationDecision`.
- `Stage4RuntimeBootstrapApplicationStatus`.
- `Stage4RuntimeBootstrapApplicationBlocker`.
- `Stage4RuntimeBootstrapApplicationBlockerKind`.

The decision is intentionally conservative:

- `BootstrapReady` -> `Applied` with the existing validated
  `RuntimeHostBootstrapSnapshot`, only if the validated report is internally
  consistent.
- Any other Stage 4C status -> `Blocked` with no applied snapshot.
- Contradictory `BootstrapReady` reports -> `Blocked` as
  `ValidatedBootstrapInconsistent`.

## Application contract

Runtime bootstrap notification is allowed only after the broker-truth validator
has produced an internally consistent `BootstrapReady`.

The following statuses are explicitly blocked:

- `BrokerTruthIncomplete`;
- `BrokerTruthStale`;
- `InstrumentMismatch`;
- `UnknownSchedule`;
- `ManualInterventionRequired`;
- `EvidenceIncomplete`;
- `SafetyBoundaryOpen`;
- generic `BootstrapBlocked`.

The application gate also rejects a manually constructed or corrupted
`ValidatedStage4BrokerTruthBootstrap` whose status says `BootstrapReady` while
other evidence contradicts readiness. The defensive consistency check covers:

- schema-version mismatch;
- `blocker_count` not matching the blocker vector;
- readiness blockers present on a supposedly ready report;
- manual-intervention flag contradicting blocker evidence;
- freshness blocking-count mismatch;
- source status other than `Present`;
- unknown schedule state;
- open safety boundary;
- runtime bootstrap snapshot target qty/flat/instrument/order-count mismatch;
- runtime bootstrap snapshot target open-position count mismatch;
- runtime bootstrap snapshot `received_ts` mismatch.

The applied snapshot is copied from
`ValidatedStage4BrokerTruthBootstrap.runtime_bootstrap_snapshot`. Stage 4E does
not rebuild broker truth, does not trust restored runtime state ahead of broker
truth, and does not let restored runtime state overwrite broker-observed
position/order truth.

## Fixture-backed cases

Stage 4E tests cover:

- clean `BootstrapReady` snapshot application;
- all non-ready Stage 4C statuses blocked before runtime notification;
- internally inconsistent `BootstrapReady` reports blocked before runtime
  notification;
- `BootstrapReady` plus readiness blockers blocked;
- `BootstrapReady` plus open safety boundary blocked;
- `BootstrapReady` plus runtime snapshot mismatch blocked;
- restored runtime state accepted only after broker truth and unable to
  overwrite broker truth;
- positive FINAM-style order/trade correlation through restored known order ids;
- target-scoped bootstrap separated from account-wide diagnostics;
- explicit dirty-start/adoption evidence preserved in the application decision.

## Safety boundary

Stage 4E does not authorize:

- runtime-live;
- `command-consumer-to-real-FINAM`;
- strategy-driven real FINAM orders;
- real POST/DELETE order endpoints;
- Stop/SLTP/bracket/replace/multi-leg.

`Stage4RuntimeBootstrapApplicationDecision.no_live_authorization` remains
`true`.

## Follow-up

After Stage 4E review acceptance, the next planned slice remains Stage 4F:
dirty-start / explicit adoption / manual-intervention policy. That stage should
continue to be evidence/paper/mock only until later Stage 4 lifecycle gates are
accepted.
