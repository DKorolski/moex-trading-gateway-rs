# Stage 3D — controlled active-session evidence collection

Status: Stage 3D accepted as offline collector foundation; Stage 3D-1 accepted
as recovery/session/input-gate hardening foundation; Stage 3D-2
recovery/session consistency hardening accepted; Stage 3D-3 controlled
operator-run input adapter implemented for review.

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

Stage 3D-2 tightens consistency:

- `AttemptedAndComplete` requires a warm or cold replay attempt;
- `AttemptedAndComplete` requires `replay_gap_absence_proven=true`;
- `AttemptedAndComplete` requires
  `first_fresh_live_final_after_replay_observed=true`;
- `AttemptedAndComplete` requires
  `entry_blocked_while_gap_unproven=true`;
- `AttemptedAndFailed` must have attempted replay and must not claim both gap
  proof and first fresh live final success;
- `schedule_known=false` must imply `unknown_schedule_blocks=true`.

## ALOR oracle input gate

Stage 3D-1 validates ALOR oracle bars before the report is generated:

- final only;
- timeframe must be 600 seconds;
- `close_ts - open_ts` must be 600 seconds;
- instrument must match the controlled target instrument.

Invalid ALOR oracle shape is rejected at the controlled evidence boundary, so a
bad oracle cannot be hidden by a missing FINAM candidate stream.

## Stage 3D-3 operator-run input adapter

Stage 3D-3 adds an offline adapter for operator-approved evidence files. It is
not a live collector. The adapter reads two already redacted/source-approved
JSON inputs:

- ALOR native M10 oracle source:
  `source_kind = "AlorNativeM10Oracle"`;
- FINAM final M1 source:
  `source_kind = "FinamFinalM1"`.

Each approved input source must contain:

- `schema_version = 2`;
- `source_label`;
- `session_date`;
- `target_instrument`;
- `raw_payload_exported = false`;
- canonical `Bar` records only.

The adapter validates source kind, session date, target instrument, non-empty
bar lists, FINAM M1 finality, FINAM M1 duration, and the existing ALOR M10
oracle rules. It then invokes
`collect_stage3d_controlled_active_session_evidence(...)`, writes the redacted
Stage 3D report, and writes a small operator summary containing counts/statuses
only.

CLI entry point:

```text
broker-cli stage3d3-controlled-evidence --config <config.json>
```

The config points to approved source files and output paths, for example:

```json
{
  "generated_at": "2026-07-09T09:00:00Z",
  "source_commit": "<full-source-commit-sha>",
  "source_archive_name": "moex-trading-project-<sha>.zip",
  "source_archive_sha256": "<64-hex-sha256>",
  "session_date": "2026-07-09",
  "target_instrument": {
    "symbol": "IMOEXF",
    "venue_symbol": "IMOEXF@RTSX",
    "exchange": "Moex",
    "market": "Futures"
  },
  "alor_source_path": "tmp/stage3d3/alor-approved-m10.json",
  "finam_source_path": "tmp/stage3d3/finam-approved-m1.json",
  "report_output_path": "reports/parity/finam-vs-alor-m10/2026-07-09.json",
  "operator_summary_output_path": "reports/parity/finam-vs-alor-m10/2026-07-09.operator-summary.json",
  "reconnect_recovery": {
    "recovery_required": false,
    "recovery_status": "NotRequired",
    "disconnect_observed": false,
    "warm_replay_attempted": false,
    "cold_replay_attempted": false,
    "replay_gap_absence_proven": false,
    "first_fresh_live_final_after_replay_observed": false,
    "entry_blocked_while_gap_unproven": true
  },
  "session_filtering": {
    "schedule_source": "operator_approved_session_scope",
    "schedule_known": true,
    "session_state": "Open",
    "weekend_filtered": true,
    "clearing_break_filtered": true,
    "unknown_schedule_blocks": false
  }
}
```

Generated reports remain local evidence artifacts. Clean handoff archives must
not include `reports/`, raw source payload dumps, secrets, accounts, or
unbounded market-data logs.

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
- internally inconsistent recovery completion or session schedule flags are
  rejected;
- missing source metadata is rejected;
- safety boundary remains closed.
- Stage 3D-3 approved source adapter accepts valid redacted source inputs and
  writes a report plus counts-only operator summary;
- Stage 3D-3 rejects missing source files, invalid source JSON, source-kind
  mismatches, raw-payload-export flags, empty source inputs, invalid FINAM M1
  shape, and session mismatches;
- Stage 3D-3 operator summary does not export raw bars.

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

After Stage 3D-3 review, the next patch should move to Stage 3E
reconnect/gap-recovery evidence for strategy-input bars. It must still keep
runtime-live and all order paths disabled.
