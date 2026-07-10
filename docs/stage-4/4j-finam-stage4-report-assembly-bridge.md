# Stage 4J — FINAM Stage 4 report assembly bridge

Status: implemented for review.

Date: 2026-07-10.

## Goal

Stage 4J adds a single FINAM-side assembly helper that builds the complete
Stage 4 broker-truth bootstrap evidence report from a FINAM read-only Stage 4D
package.

This stage is a bridge, not a new execution surface. It does not enable
runtime-live, does not connect a real FINAM command consumer, and does not add
POST/DELETE order endpoints.

## Added API

Stage 4J adds:

- `build_finam_stage4_bootstrap_evidence_report(...)`.

The helper takes a `FinamStage4BrokerTruthBootstrapPackage`, then assembles:

```text
FINAM Stage 4D package
  -> Stage 4C validated broker truth
  -> Stage 4E runtime bootstrap application decision
  -> Stage 4F dirty-start/adoption policy decision
  -> Stage 4G runtime lifecycle ordering decision
  -> Stage 4H mock runtime bootstrap integration decision
  -> Stage 4I redacted bootstrap evidence report
```

The lifecycle plan used by this bridge is the accepted ALOR-compatible mock
bootstrap plan.

## Source-evidence path

The FINAM bridge intentionally uses the Stage 4I preferred source-evidence path:

```text
FinamStage4ReadonlySourceEvidenceSet::redacted_stage4i_source_sections()
  -> build_stage4_bootstrap_evidence_report_with_source_evidence(...)
```

It does not use the compatibility builder that supplies synthetic `Present`
source sections.

Required FINAM source evidence remains a report gate:

- `Missing`;
- `Unavailable`;
- `DecodeFailed`;
- `Incomplete`;

all block Stage 4I report acceptance when the section is required for
bootstrap. Blocked reports emit no mock runtime events.

## Redaction policy

The assembled report exports only redacted/operator-safe summaries. It does not
export:

- FINAM account id;
- broker asset id;
- broker order id;
- client order id;
- broker trade id;
- raw order comments;
- raw FINAM payloads.

## Fixture-backed coverage

Unit tests cover:

- accepted FINAM read-only fixture produces an accepted Stage 4I report and the
  deterministic Stage 4H mock event trace;
- non-`Present` required FINAM source statuses (`Missing`, `Unavailable`,
  `DecodeFailed`, `Incomplete`) produce blocked reports with exact source
  status preserved;
- required non-`Present` source evidence emits no runtime events;
- serialized FINAM-assembled reports do not include sensitive fixture values;
- the FINAM bridge uses package source evidence and not the synthetic
  compatibility builder.

## Safety boundary

Stage 4J keeps these disabled:

- continuous runtime-live;
- `command-consumer-to-real-FINAM`;
- strategy-runtime-to-real-FINAM order routing;
- FINAM `LiveReady`;
- real POST/DELETE order endpoints;
- Stop/SLTP/bracket/replace/multi-leg.

Stage 4J acceptance would mean the FINAM read-only Stage 4 package can assemble
the accepted redacted Stage 4 evidence report. It is not live trading approval.
