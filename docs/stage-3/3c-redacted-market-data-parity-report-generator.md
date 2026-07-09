# Stage 3C — redacted market-data parity report generator

Status: accepted after Stage 3C-1 duplicate bucket hardening.

Date: 2026-07-09.

## Purpose

Stage 3C adds a source/report-only redacted evidence generator for comparing
the ALOR native M10 strategy oracle with FINAM-derived M10 candidates across
multiple buckets:

```text
Vec<ALOR native M10 oracle bars>
  vs
Vec<FINAM final M1 -> derived M10 candidate bars>
  -> redacted Stage 3 evidence report
```

This remains a pure source-level/reporting patch. It does not read Redis, does
not collect active-session evidence, does not attach runtime-live, and does not
enable order routing.

## Report generator

The core entry point is:

```rust
generate_stage3c_redacted_m10_parity_report(...)
```

It produces a `Stage3MarketDataParityReport` with:

- schema/stage/substage markers;
- source-binding placeholders;
- target instrument scope;
- input summaries;
- strategy input gate summary;
- strategy input publication counters;
- comparison policy;
- multi-bucket comparison summary;
- multi-bucket diff summary;
- reconnect recovery placeholder;
- session filtering placeholder;
- safety boundary;
- `raw_payload_exported = false`.

Generated source handoff archives must still exclude runtime reports under
`reports/`. Active-session evidence collection is a later Stage 3D/3E concern.

## Multi-bucket comparison semantics

The report generator compares buckets by canonical `open_ts`:

- input buckets are normalized before comparison and duplicate buckets are not
  silently overwritten;
- exact duplicate M10 buckets are idempotent and counted as diagnostic;
- conflicting duplicate M10 buckets are blocking and counted separately for
  ALOR and FINAM;
- matched buckets are compared on instrument, timeframe, finality, timestamps,
  OHLCV, and diagnostic source kind;
- ALOR-only buckets increment `missing_finam_derived_bar`;
- FINAM-only buckets increment `missing_alor_bar`;
- no overlapping buckets produce `NoOverlappingBuckets`;
- empty inputs produce `EvidenceIncomplete` or a specific `Missing*` status;
- first/last matched bucket timestamps are tracked;
- first/last diff bucket timestamps are tracked;
- max absolute OHLCV deltas are tracked over all matched buckets.

## Publication counter policy

The Stage 3B-1 policy is preserved:

- `finam_derived_m10_published_as_model_bar_count` increments only for buckets
  with no blocking diff;
- blocked/missing candidates increment
  `candidate_bars_rejected_before_strategy_count`;
- raw FINAM M1 bars never count as strategy/model bars.

This means a session-level report can be `BlockedDiff` while still showing how
many individual buckets were synchronized and how many candidate buckets were
rejected before strategy publication.

## Redaction policy

The report exports only summaries and compact diagnostics. It must not include:

- raw Redis entries;
- raw ALOR payloads;
- raw FINAM payloads;
- account ids;
- broker tokens;
- unbounded bar series dumps.

Allowed report content includes counts, status enums, timestamps for matched and
diff buckets, max deltas, source mode labels, and safety-boundary flags.

## Covered tests

Stage 3C source tests cover:

- multi-bucket synchronized report counts;
- first/last matched bucket timestamps;
- JSON shape with schema-aligned top-level sections;
- ALOR-only and FINAM-only bucket counts;
- rejected-before-strategy counts for missing/blocking candidates;
- OHLCV mismatch counts and max deltas;
- timestamp mismatch counts;
- first/last diff bucket timestamps;
- empty/missing streams returning explicit non-panic statuses;
- exact duplicate ALOR buckets are idempotent and counted;
- conflicting duplicate ALOR buckets are blocking;
- exact duplicate FINAM buckets are idempotent and counted;
- conflicting duplicate FINAM buckets are blocking;
- conflicting duplicate buckets do not silently overwrite the previously seen
  bar;
- `raw_payload_exported = false`;
- safety boundary remains closed.

## Still forbidden

Stage 3C does not authorize:

- runtime-live;
- real FINAM command consumer;
- strategy-driven real FINAM orders;
- real FINAM `POST`/`DELETE` from runtime;
- Stop/SLTP/bracket/replace live behavior;
- RI/RTS migration;
- USDRUBF migration;
- `i64` surrogate adapter;
- BO/MR trading logic changes.

## Next expected patch

Stage 3D should prepare controlled active-session evidence collection around
the accepted report shape. It should still require explicit operator/review
authorization before reading live streams or generating dated evidence files.
