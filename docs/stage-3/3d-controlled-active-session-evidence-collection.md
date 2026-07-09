# Stage 3D — controlled active-session evidence collection

Status: Stage 3D accepted as offline collector foundation; Stage 3D-1
recovery/session/input-gate hardening implemented for review.

Date: 2026-07-09.

## Purpose

Stage 3D adds a controlled evidence collection layer around the accepted Stage
3C report generator:

```text
ALOR native M10 oracle bars
FINAM final M1 bars
  -> FINAM M1-to-M10 derivation
  -> Stage 3C multi-bucket report generator
  -> redacted Stage 3D evidence JSON
```

This patch is still offline/source/report-only. It does not read Redis, does not
connect to FINAM or ALOR, does not attach runtime-live, and does not enable any
order path.

## Core entry points

Stage 3D introduces:

```rust
collect_stage3d_controlled_active_session_evidence(...)
serialize_stage3d_redacted_evidence_report(...)
write_stage3d_redacted_evidence_report(...)
```

The collector accepts already-controlled inputs:

- source-bound metadata;
- session date;
- target instrument;
- ALOR native M10 oracle bars;
- FINAM final M1 bars;
- reconnect recovery summary;
- session filtering summary.

It derives FINAM M10 candidates from final M1 bars, invokes the Stage 3C
multi-bucket report generator, fills source/session metadata, and returns a
redacted report.

The writer helper creates parent directories and writes only the redacted JSON
artifact, for example:

```text
reports/parity/finam-vs-alor-m10/YYYY-MM-DD.json
```

## Required metadata

Stage 3D requires:

- `generated_at`;
- `source_commit`;
- `source_archive_name`;
- `source_archive_sha256`;
- `session_date`;
- target instrument.

Missing source/session metadata is rejected before a report is produced. Stage
3D-1 also requires `source_archive_sha256` to be a 64-character hex string and
`session_date` to use `YYYY-MM-DD`.

## FINAM M1 derivation

FINAM input remains strict:

- raw M1 bars are grouped by canonical 10-minute bucket;
- each bucket is passed through the Stage 3B M1-only derivation contract;
- incomplete or rejected M1 buckets do not become strategy/model bars;
- rejected derivation buckets increment `candidate_bars_rejected_before_strategy_count`;
- completed derived M10 buckets are then compared against ALOR native M10.

## Recovery and session fields

Stage 3D does not invent recovery state. The caller must supply the recovery
summary:

- `NotRequired` when no reconnect/gap recovery was needed;
- `NotAttempted` when recovery was required but absent;
- `AttemptedAndComplete` when replay/dedupe/contiguity/fresh-live checks passed;
- `AttemptedAndFailed` when recovery failed.

Session filtering is also caller-supplied and must honestly represent the
controlled evidence window.

Stage 3D-1 applies these fields to the report gate:

- `recovery_required=false` requires `recovery_status=NotRequired`;
- `recovery_required=true` requires `recovery_status` to be one of
  `NotAttempted`, `AttemptedAndComplete`, or `AttemptedAndFailed`;
- `NotAttempted` and `AttemptedAndFailed` force `RecoveryIncomplete`;
- incomplete recovery suppresses FINAM derived M10 publication and moves any
  previously published candidate count into rejected-before-strategy count;
- `schedule_known=false` with `unknown_schedule_blocks=true` forces
  `SessionScheduleUnknown` and suppresses strategy/model-bar publication.

## ALOR oracle input gate

Stage 3D-1 validates ALOR oracle bars before the report is generated:

- final only;
- timeframe must be 600 seconds;
- `close_ts - open_ts` must be 600 seconds;
- instrument must match the controlled target instrument.

Invalid ALOR oracle shape is rejected at the controlled evidence boundary, so a
bad oracle cannot be hidden by a missing FINAM candidate stream.

## Redaction policy

The serialized Stage 3D report keeps:

- `raw_payload_exported = false`;
- compact counts/statuses/timestamps/max deltas;
- source archive binding metadata;
- safety boundary flags.

It must not include:

- raw Redis entries;
- raw ALOR payloads;
- raw FINAM payloads;
- account ids;
- broker tokens;
- unbounded market-data dumps.

## Covered tests

Stage 3D tests cover:

- source/session metadata is populated;
- FINAM final M1 bars derive a synchronized M10 candidate;
- report JSON serializes as redacted Stage 3D output;
- report JSON can be written as a redacted artifact under the expected parity
  path shape;
- incomplete FINAM M1 buckets are rejected before strategy publication;
- recovery status is passed through explicitly;
- failed or not-attempted recovery blocks synchronized publication;
- inconsistent recovery flags are rejected;
- unknown schedule blocks synchronized publication;
- invalid ALOR oracle finality/timeframe/duration/instrument is rejected;
- invalid archive SHA256 and invalid session date are rejected;
- missing source metadata is rejected;
- safety boundary remains closed.

## Still forbidden

Stage 3D does not authorize:

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

After Stage 3D review, the next patch should add a controlled operator-run
adapter for collecting redacted active-session inputs from approved sources.
That future patch must still keep runtime-live and all order paths disabled.
