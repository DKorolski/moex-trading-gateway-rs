# Stage 4I — redacted operator-facing bootstrap evidence report

Status: implemented for review.

Date: 2026-07-10.

## Goal

Stage 4I adds a deterministic, redacted operator-facing report over the accepted
Stage 4 broker-truth bootstrap chain.

It does not introduce a new runtime lifecycle and does not authorize live
execution. The report is a review/operator evidence bundle that answers:

- whether Stage 4C broker-truth validation is ready or blocked;
- what Stage 4D source sections were fresh, stale, unknown, or unavailable;
- whether Stage 4E application evidence was applied;
- whether Stage 4F dirty-start/adoption policy was accepted;
- whether Stage 4G lifecycle ordering was accepted;
- whether Stage 4H mock runtime events may be emitted;
- why a blocked report is blocked.

## Added API

Stage 4I adds:

- `STAGE4_BOOTSTRAP_EVIDENCE_REPORT_SCHEMA_VERSION`;
- `Stage4BootstrapEvidenceReportStatus`;
- `Stage4BootstrapEvidenceReportStage`;
- `Stage4BootstrapEvidenceReportBlockerKind`;
- `Stage4BootstrapEvidenceReportBlocker`;
- `Stage4BootstrapEvidenceSourceSection`;
- `Stage4BootstrapEvidenceRedaction`;
- `Stage4BootstrapEvidenceReport`;
- `build_stage4_bootstrap_evidence_report(...)`.

The report is deterministic for the same validated broker-truth chain. It uses
the validated Stage 4C `checked_ts`; it does not stamp a fresh wall-clock time
inside report construction.

## Evidence chain

`build_stage4_bootstrap_evidence_report(...)` requires the caller to pass the
full chain:

```text
Stage 4C ValidatedStage4BrokerTruthBootstrap
  -> Stage 4E Stage4RuntimeBootstrapApplicationDecision
  -> Stage 4F Stage4DirtyStartPolicyDecision
  -> Stage 4G Stage4RuntimeLifecycleOrderingDecision
  -> Stage 4H Stage4RuntimeBootstrapIntegrationDecision
  -> Stage 4I Stage4BootstrapEvidenceReport
```

The builder recomputes the canonical Stage 4E/4F/4G/4H decisions for the same
validated report and lifecycle plan. If any provided decision differs from the
canonical chain, the Stage 4I report is blocked with
`EvidenceChainInconsistent`.

## Blocked-report semantics

Blocked reports include an explicit reason chain. Reasons are typed by stage and
kind:

- `BrokerTruthValidationBlocked`;
- `RuntimeBootstrapApplicationBlocked`;
- `DirtyStartPolicyBlocked`;
- `RuntimeLifecycleOrderingBlocked`;
- `RuntimeBootstrapIntegrationBlocked`;
- `EvidenceChainInconsistent`;
- `RedactionBoundaryOpen`;
- `LiveAuthorizationAttempted`.

Every reason in the chain blocks runtime events. When the Stage 4I report is
blocked, `runtime_events_emitted=false` and `mock_runtime_events=[]`, even if a
tampered downstream DTO contains events.

## Redaction policy

The report exports only redacted/operator-safe summaries:

- per-section source status and freshness bounds;
- stage statuses and blocker kinds;
- target instrument identity;
- target/account active-order counts;
- safety-boundary flags.

It does not export tokens, raw broker payloads, broker-native account dumps,
account ids, raw order comments, client order ids, broker order ids, or broker
asset ids. If the underlying Stage 4C safety boundary indicates raw payload
export, Stage 4I blocks with `RedactionBoundaryOpen`.

## Fixture-backed coverage

Unit tests cover:

- accepted reports are deterministic, redacted, and include the Stage 4H mock
  runtime event trace;
- stale required broker-truth sections produce a blocked report with a reason
  chain and no runtime events;
- noncanonical/tampered application evidence blocks the report even when
  downstream integration would otherwise contain accepted mock events;
- serialized reports do not include broker-sensitive fixture values.

## Safety boundary

Stage 4I keeps these disabled:

- continuous runtime-live;
- `command-consumer-to-real-FINAM`;
- strategy-runtime-to-real-FINAM order routing;
- FINAM `LiveReady`;
- real POST/DELETE order endpoints;
- Stop/SLTP/bracket/replace/multi-leg.

Stage 4I is report/evidence only. It is not approval for live runtime trading.
