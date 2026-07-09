# Stage 3E — reconnect/gap recovery evidence for strategy-input bars

Status: implemented for review.

Date: 2026-07-09.

## Purpose

Stage 3E proves the strategy-input recovery contract after a disconnect, gap,
or restart:

```text
last final strategy-bar watermark
  -> replay window with overlap
  -> replay/gap proof
  -> first fresh live final bar after replay
  -> strategy input can resume
```

This is still offline/evidence/report-only. It does not read Redis live streams,
does not connect to FINAM or ALOR, does not attach runtime-live, and does not
enable any order path.

## Core entry points

Stage 3E introduces:

```rust
collect_stage3e_reconnect_gap_recovery_evidence(...)
serialize_stage3e_recovery_evidence_report(...)
write_stage3e_recovery_evidence_report(...)
```

The collector wraps the broker-neutral `MarketDataRecoveryReport` and adds
strategy-input evidence:

- source/archive/session metadata;
- approved `session_window_utc`;
- reconnect recovery summary;
- action gate evidence;
- publication counters;
- closed safety boundary.

## Recovery acceptance contract

`RecoveryComplete` requires all of the following:

- `recovery_required = true`;
- `recovery_status = AttemptedAndComplete`;
- warm or cold replay attempted;
- `replay_gap_absence_proven = true`;
- `first_fresh_live_final_after_replay_observed = true`;
- entry stayed blocked while the gap was unproven;
- `MarketDataRecoveryReport.phase = LiveReady`;
- recovery blockers are empty;
- recovery report has `gap_absence_proven = true`;
- first fresh live final bar after replay is present.

`NotAttempted` and `AttemptedAndFailed` produce `RecoveryIncomplete` and must
not allow strategy/model publication.

## Publication and action safety

Stage 3E keeps the distinction between recovery data and strategy input:

- replay/recovery bars are not publishable as strategy/model bars;
- overlap replay dedupe must not create duplicate model bars;
- post-recovery model publication is allowed only after complete recovery;
- entry is blocked while gap proof is missing;
- exit/cancel/repair are not falsely blocked by the entry gap guard.

Any violation of these rules yields `SafetyBoundaryOpen` in the Stage 3E report.

## Redaction policy

The Stage 3E report may include timestamps, counts, statuses, blockers, source
archive binding metadata, and approved session window metadata. It must not
include raw Redis payloads, raw broker payloads, secrets, account ids, broker
tokens, or unbounded market-data dumps.

## Covered tests

Stage 3E tests cover:

- complete recovery allows strategy input only after gap proof and first fresh
  live final bar;
- `NotAttempted` recovery blocks strategy publication;
- `AttemptedAndFailed` recovery blocks strategy publication;
- `AttemptedAndComplete` must match a `LiveReady` recovery report;
- entry must stay blocked while gap is unproven;
- exit/cancel/repair must remain allowed while entry is blocked by the gap
  guard;
- recovery bars never become model bars;
- overlap replay never creates duplicate model bars;
- incomplete recovery cannot publish a post-recovery model bar;
- redacted Stage 3E report serialization/writing.

## Still forbidden

Stage 3E does not authorize:

- runtime-live;
- real FINAM command consumer;
- strategy-driven real FINAM orders;
- real FINAM `POST`/`DELETE` from runtime;
- Stop/SLTP/bracket/replace live behavior;
- RI/RTS migration;
- USDRUBF migration;
- `i64` surrogate adapter;
- BO/MR trading logic changes.
