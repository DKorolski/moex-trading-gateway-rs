# Stage 3A — market-data parity evidence schema

Status: accepted.

Date: 2026-07-09.

## Purpose

This document defines the redacted evidence shape for Stage 3 active-session
market-data parity:

```text
ALOR native M10 strategy oracle
  vs
FINAM final M1 -> canonical derived M10 strategy candidate
```

The evidence proves strategy-input bar parity only. It does not authorize
runtime-live, command-consumer-to-real-FINAM, or real FINAM order execution.

## Artifact location

Generated evidence should be written outside clean source archives:

```text
reports/parity/finam-vs-alor-m10/YYYY-MM-DD.json
```

Clean handoff archives may include documentation and source code, but not raw
generated reports unless explicitly requested as redacted evidence.

## Top-level JSON shape

```json
{
  "schema_version": 1,
  "stage": "Stage3MarketDataParity",
  "substage": "Stage3A",
  "generated_at": "2026-07-09T00:00:00Z",
  "source_commit": "short-or-full-sha",
  "source_archive_name": "moex-trading-project-<sha>.zip",
  "source_archive_sha256": "sha256",
  "raw_payload_exported": false,
  "scope": {
    "instrument_symbol": "IMOEXF",
    "timeframe_sec": 600,
    "session_date": "YYYY-MM-DD",
    "exchange": "MOEX"
  },
  "inputs": {},
  "strategy_input_gate": {},
  "strategy_input_publication": {},
  "comparison_policy": {},
  "comparison_summary": {},
  "diff_summary": {},
  "reconnect_recovery": {},
  "session_filtering": {},
  "safety_boundary": {},
  "status": "Pending"
}
```

All fields containing broker/account/runtime identities must use synthetic
aliases or redacted/fingerprinted values.

Stage 3C source-only unit reports may keep `generated_at`, `source_commit`,
`source_archive_name`, and `source_archive_sha256` as `null` placeholders until
an operator evidence package binds the generated report to a source archive.
Stage 3D controlled evidence reports and later active-session evidence packages
must fill those fields. Stage 3D-1 and later require
`source_archive_sha256` to be a 64-character hex string and `session_date` to
use `YYYY-MM-DD`.

## Status enum

Allowed top-level statuses:

- `Pending`;
- `Synchronized`;
- `BlockedDiff`;
- `MissingAlorOracleStream`;
- `MissingFinamDerivedStream`;
- `NoOverlappingBuckets`;
- `RecoveryIncomplete`;
- `SessionScheduleUnknown`;
- `EvidenceIncomplete`;
- `SafetyBoundaryOpen`.

`Synchronized` is allowed only when all required comparison and safety gates
pass.

Stage 3E reconnect/gap recovery evidence uses a dedicated recovery-evidence
status enum:

- `RecoveryComplete`;
- `RecoveryIncomplete`;
- `SafetyBoundaryOpen`.

`RecoveryComplete` means replay/gap proof and first fresh live final evidence
are both present. It does not authorize runtime-live or real order routing.

## Inputs section

```json
{
  "inputs": {
    "alor_oracle": {
      "source_mode": "AlorNativeBarsGetAndSubscribeTf600",
      "stream_role": "StrategyOracleM10",
      "timeframe_sec": 600,
      "timestamp_policy": "bucket_open_from_close_time_utc",
      "bars_seen": 0,
      "exact_duplicate_bucket_count": 0,
      "conflicting_duplicate_bucket_count": 0,
      "complete_buckets": 0
    },
    "finam_candidate": {
      "source_mode": "FinamDerivedM1ToM10",
      "source_timeframe_sec": 60,
      "target_timeframe_sec": 600,
      "bars_seen_m1": 0,
      "duplicate_exact_m1_count": 0,
      "duplicate_conflicting_m1_count": 0,
      "exact_duplicate_m10_bucket_count": 0,
      "conflicting_duplicate_m10_bucket_count": 0,
      "complete_buckets": 0,
      "incomplete_buckets": 0
    },
    "alor_assembled_cross_check": {
      "present": false,
      "source_mode": "AlorStandDerivedM1ToM10",
      "complete_buckets": 0
    }
  }
}
```

Raw bar payloads must not appear here.

## Strategy input gate

```json
{
  "strategy_input_gate": {
    "raw_m1_allowed_as_strategy_input": false,
    "finam_native_m10_allowed": false,
    "required_source_mode": "FinamDerivedM1ToM10",
    "required_target_timeframe_sec": 600,
    "requires_final_bars": true,
    "requires_complete_aggregation": true,
    "requires_gap_absence_proven": true,
    "requires_session_filter_pass": true,
    "requires_first_fresh_live_final_after_replay": true,
    "strategy_watermark_advanced_by_raw_m1": false
  }
}
```

If any boolean contradicts the safety contract, `status` must be
`SafetyBoundaryOpen` or `EvidenceIncomplete`, not `Synchronized`.

## Strategy input publication

```json
{
  "strategy_input_publication": {
    "raw_m1_published_as_model_bar_count": 0,
    "finam_derived_m10_published_as_model_bar_count": 0,
    "alor_native_m10_oracle_bars_seen": 0,
    "candidate_bars_rejected_before_strategy_count": 0
  }
}
```

`raw_m1_published_as_model_bar_count` must remain zero. Raw FINAM M1 bars are
aggregation inputs only, never strategy/model bars. Rejected candidate bars must
be counted before strategy publication and must not advance the strategy
watermark.

`finam_derived_m10_published_as_model_bar_count` may increment only for
`Synchronized` comparator outcomes. If a FINAM-derived M10 candidate exists but
the comparator status is `BlockedDiff`, `MissingAlorOracleStream`, or any other
non-synchronized status, it must be counted under
`candidate_bars_rejected_before_strategy_count` instead.

## Comparison policy

```json
{
  "comparison_policy": {
    "timestamp_tolerance_sec": 0,
    "price_tolerance": "exact_decimal",
    "volume_tolerance": "exact_decimal",
    "open_ts_policy": "bucket_open",
    "close_ts_policy": "bucket_close",
    "ohlcv_diff_policy": "blocking_on_any_nonzero_diff",
    "missing_bar_policy": "blocking",
    "finality_policy": "final_only",
    "instrument_identity_policy": "symbol_exchange_timeframe"
  }
}
```

Any future tolerance must be explicit and reviewed.

## Comparison summary

```json
{
  "comparison_summary": {
    "matched_bucket_count": 0,
    "first_matched_bucket_open_ts": null,
    "last_matched_bucket_open_ts": null,
    "alor_only_bucket_count": 0,
    "finam_only_bucket_count": 0,
    "blocking_diff_count": 0,
    "diagnostic_diff_count": 0
  }
}
```

`matched_bucket_count > 0` is required before Stage 3 acceptance.

## Diff summary

```json
{
  "diff_summary": {
    "max_abs_open_diff": "0",
    "max_abs_high_diff": "0",
    "max_abs_low_diff": "0",
    "max_abs_close_diff": "0",
    "max_abs_volume_diff": "0",
    "first_diff_bucket_open_ts": null,
    "last_diff_bucket_open_ts": null,
    "diff_counts": {
      "MissingAlorBar": 0,
      "MissingFinamDerivedBar": 0,
      "ExactDuplicateAlorBucket": 0,
      "ExactDuplicateFinamBucket": 0,
      "ConflictingDuplicateAlorBucket": 0,
      "ConflictingDuplicateFinamBucket": 0,
      "TimestampMismatch": 0,
      "OhlcvMismatch": 0,
      "TimeframeMismatch": 0,
      "FinalityMismatch": 0,
      "InstrumentMismatch": 0,
      "SourceKindDiagnostic": 0
    }
  }
}
```

Diff summaries are compact diagnostics only. They must not include full raw bar
series.

Exact duplicate M10 buckets are idempotent but must be counted as diagnostics.
Conflicting duplicate M10 buckets are blocking and must never be silently
overwritten during report generation.

## Reconnect recovery

```json
{
  "reconnect_recovery": {
    "recovery_required": false,
    "recovery_status": "NotRequired",
    "disconnect_observed": false,
    "last_final_strategy_bar_watermark": null,
    "warm_replay_attempted": false,
    "cold_replay_attempted": false,
    "replay_gap_absence_proven": false,
    "first_fresh_live_final_after_replay_observed": false,
    "entry_blocked_while_gap_unproven": true,
    "exit_cancel_repair_policy": "not_enabled_in_stage3_but_must_not_be_blocked_by_entry_gap_policy"
  }
}
```

Allowed `recovery_status` values:

- `NotRequired` — no disconnect/gap/silence interval required recovery;
- `NotAttempted` — recovery was required but was not attempted, so strategy
  entry must remain blocked;
- `AttemptedAndComplete` — replay/dedupe/contiguity checks completed and the
  first fresh live final bar was observed;
- `AttemptedAndFailed` — recovery was attempted but gap absence was not proven.

If recovery is required but not complete, top-level `status` must be
`RecoveryIncomplete`, and derived FINAM candidate bars must not be counted as
published strategy/model bars.

Stage 3D controlled evidence must set `recovery_required` and
`recovery_status` explicitly. `NotRequired` is valid only when no reconnect,
silence, or gap recovery was needed for the controlled evidence window.
Inconsistent combinations are invalid controlled evidence input.

For Stage 3D-2 and later, `AttemptedAndComplete` is valid only when replay was
attempted, gap absence was proven, the first fresh live final bar after replay
was observed, and entry remained blocked while the gap was unproven. These
flags are evidence gates, not merely diagnostics.

Stage 3E adds a dedicated recovery evidence report that wraps the broker-neutral
`MarketDataRecoveryReport` and publication/action-gate counters. The report
must show that replay/recovery bars were not published as strategy/model bars,
overlap replay did not duplicate model bars, entry stayed blocked while the gap
was unproven, and exit/cancel/repair were not falsely blocked by the entry gap
guard.

## Session filtering

```json
{
  "session_filtering": {
    "schedule_source": "broker_or_moex_calendar",
    "schedule_known": true,
    "session_state": "Open",
    "weekend_filtered": true,
    "clearing_break_filtered": true,
    "unknown_schedule_blocks": true
  }
}
```

Unknown schedule is blocking. When `schedule_known=false` and
`unknown_schedule_blocks=true`, top-level `status` must be
`SessionScheduleUnknown`, and derived FINAM candidate bars must not be counted
as published strategy/model bars. Expected session breaks are not evidence of
data loss, but they also cannot produce fresh strategy bars.
For Stage 3D-2 and later, `schedule_known=false` with
`unknown_schedule_blocks=false` is invalid controlled evidence input.

## Safety boundary

```json
{
  "safety_boundary": {
    "runtime_live_enabled": false,
    "real_finam_command_consumer_enabled": false,
    "strategy_driven_real_orders_enabled": false,
    "real_finam_post_delete_from_runtime_enabled": false,
    "stop_sltp_bracket_enabled": false,
    "ri_rts_migration_enabled": false,
    "usdrubf_migration_enabled": false,
    "i64_surrogate_adapter_enabled": false,
    "bo_mr_trading_logic_changed": false
  }
}
```

Any `true` value in this section blocks Stage 3 evidence acceptance unless a
later explicit review changes the safety boundary.

## Stage 3A acceptance

Stage 3A acceptance requires only this schema and plan to be reviewed. It does
not require live/active-session data.

Stage 3 full acceptance later requires:

- `status = Synchronized`;
- `matched_bucket_count > 0`;
- `blocking_diff_count = 0`;
- `raw_payload_exported = false`;
- strategy input gate proves no raw M1 model bars;
- reconnect recovery is complete or not required;
- safety boundary remains closed.
