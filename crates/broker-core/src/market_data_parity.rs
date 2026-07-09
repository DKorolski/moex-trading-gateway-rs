use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use chrono::{DateTime, Duration, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::bar_aggregation::{
    BarAggregationAction, BarAggregationRejectReason, CanonicalBarAggregator,
};
use crate::event::{Bar, MarketDataSourceKind};
use crate::instrument::{InstrumentId, Price, Quantity};
use crate::PaperSafetyBoundary;

pub const STAGE3_MARKET_DATA_PARITY_SCHEMA_VERSION: u16 = 1;
pub const STAGE3_MARKET_DATA_PARITY_STAGE: &str = "Stage3MarketDataParity";
pub const STAGE3_MARKET_DATA_PARITY_SUBSTAGE_3B: &str = "Stage3B";
pub const STAGE3_MARKET_DATA_PARITY_SUBSTAGE_3C: &str = "Stage3C";
pub const STAGE3_MARKET_DATA_PARITY_SUBSTAGE_3D: &str = "Stage3D";
pub const STAGE3D3_OPERATOR_INPUT_ADAPTER_STAGE: &str = "Stage3D3ControlledOperatorInputAdapter";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Stage3StrategyBarSourceMode {
    AlorNativeBarsGetAndSubscribeTf600,
    AlorStandDerivedM1ToM10,
    FinamDerivedM1ToM10,
    FinamNativeM10,
    RawFinamM1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Stage3StrategyInputRejectReason {
    RawM1NotAllowed,
    FinamNativeM10PendingCharacterization,
    SourceModeNotAllowed,
    NonFinalBar,
    StrategyTimeframeMismatch { expected_sec: u32, actual_sec: u32 },
    SourceTimeframeMismatch { expected_sec: u32, actual_sec: u32 },
    AggregationIncomplete,
    GapAbsenceNotProven,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stage3StrategyBarProvenance {
    pub source_mode: Stage3StrategyBarSourceMode,
    pub source_timeframe_sec: Option<u32>,
    pub target_timeframe_sec: u32,
    pub aggregation_complete: bool,
    pub gap_absence_proven: bool,
}

impl Stage3StrategyBarProvenance {
    pub fn alor_native_m10_oracle() -> Self {
        Self {
            source_mode: Stage3StrategyBarSourceMode::AlorNativeBarsGetAndSubscribeTf600,
            source_timeframe_sec: None,
            target_timeframe_sec: 600,
            aggregation_complete: true,
            gap_absence_proven: true,
        }
    }

    pub fn finam_derived_m1_to_m10_complete() -> Self {
        Self {
            source_mode: Stage3StrategyBarSourceMode::FinamDerivedM1ToM10,
            source_timeframe_sec: Some(60),
            target_timeframe_sec: 600,
            aggregation_complete: true,
            gap_absence_proven: true,
        }
    }

    pub fn raw_finam_m1() -> Self {
        Self {
            source_mode: Stage3StrategyBarSourceMode::RawFinamM1,
            source_timeframe_sec: Some(60),
            target_timeframe_sec: 60,
            aggregation_complete: false,
            gap_absence_proven: false,
        }
    }

    pub fn finam_native_m10_pending() -> Self {
        Self {
            source_mode: Stage3StrategyBarSourceMode::FinamNativeM10,
            source_timeframe_sec: Some(600),
            target_timeframe_sec: 600,
            aggregation_complete: true,
            gap_absence_proven: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stage3StrategyInputGateOutcome {
    pub accepted: bool,
    pub reason: Option<Stage3StrategyInputRejectReason>,
    pub strategy_watermark_advanced: bool,
}

pub fn evaluate_stage3_strategy_input_gate(
    bar: &Bar,
    provenance: &Stage3StrategyBarProvenance,
) -> Stage3StrategyInputGateOutcome {
    let reject = |reason| Stage3StrategyInputGateOutcome {
        accepted: false,
        reason: Some(reason),
        strategy_watermark_advanced: false,
    };

    if provenance.source_mode == Stage3StrategyBarSourceMode::RawFinamM1 || bar.timeframe_sec == 60
    {
        return reject(Stage3StrategyInputRejectReason::RawM1NotAllowed);
    }
    if provenance.source_mode == Stage3StrategyBarSourceMode::FinamNativeM10 {
        return reject(Stage3StrategyInputRejectReason::FinamNativeM10PendingCharacterization);
    }
    if !matches!(
        provenance.source_mode,
        Stage3StrategyBarSourceMode::AlorNativeBarsGetAndSubscribeTf600
            | Stage3StrategyBarSourceMode::AlorStandDerivedM1ToM10
            | Stage3StrategyBarSourceMode::FinamDerivedM1ToM10
    ) {
        return reject(Stage3StrategyInputRejectReason::SourceModeNotAllowed);
    }
    if !bar.is_final {
        return reject(Stage3StrategyInputRejectReason::NonFinalBar);
    }
    if bar.timeframe_sec != 600 {
        return reject(Stage3StrategyInputRejectReason::StrategyTimeframeMismatch {
            expected_sec: 600,
            actual_sec: bar.timeframe_sec,
        });
    }
    if provenance.target_timeframe_sec != 600 {
        return reject(Stage3StrategyInputRejectReason::StrategyTimeframeMismatch {
            expected_sec: 600,
            actual_sec: provenance.target_timeframe_sec,
        });
    }

    if provenance.source_mode == Stage3StrategyBarSourceMode::FinamDerivedM1ToM10 {
        if provenance.source_timeframe_sec != Some(60) {
            return reject(Stage3StrategyInputRejectReason::SourceTimeframeMismatch {
                expected_sec: 60,
                actual_sec: provenance.source_timeframe_sec.unwrap_or_default(),
            });
        }
        if !provenance.aggregation_complete {
            return reject(Stage3StrategyInputRejectReason::AggregationIncomplete);
        }
        if !provenance.gap_absence_proven {
            return reject(Stage3StrategyInputRejectReason::GapAbsenceNotProven);
        }
    }

    Stage3StrategyInputGateOutcome {
        accepted: true,
        reason: None,
        strategy_watermark_advanced: true,
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Stage3AlorNativeM10Input {
    pub instrument: InstrumentId,
    pub close_time_utc: DateTime<Utc>,
    pub open: Price,
    pub high: Price,
    pub low: Price,
    pub close: Price,
    pub volume: Quantity,
    pub is_final: bool,
}

pub fn normalize_stage3_alor_native_m10_oracle(input: Stage3AlorNativeM10Input) -> Bar {
    Bar {
        instrument: input.instrument,
        source_kind: MarketDataSourceKind::LiveStream,
        timeframe_sec: 600,
        open_ts: input.close_time_utc,
        close_ts: input.close_time_utc + Duration::seconds(600),
        open: input.open,
        high: input.high,
        low: input.low,
        close: input.close,
        volume: input.volume,
        is_final: input.is_final,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Stage3FinamM10DerivationStatus {
    Emitted,
    Incomplete,
    Rejected,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Stage3FinamM10DerivationRejectReason {
    DuplicateConflictingM1,
    MissingM1,
    NonM1SourceTimeframe { actual_sec: u32 },
    NonM1SourceDuration { actual_sec: i64 },
    AggregationRejected(BarAggregationRejectReason),
    NoCompleteBucket,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Stage3FinamM10DerivationReport {
    pub status: Stage3FinamM10DerivationStatus,
    pub emitted: Option<Bar>,
    pub bars_seen_m1: usize,
    pub duplicate_exact_m1_count: usize,
    pub duplicate_conflicting_m1_count: usize,
    pub complete_buckets: usize,
    pub incomplete_buckets: usize,
    pub reject_reason: Option<Stage3FinamM10DerivationRejectReason>,
    pub raw_payload_exported: bool,
}

pub fn derive_stage3_finam_m10_from_final_m1(
    source_bars: Vec<Bar>,
) -> Stage3FinamM10DerivationReport {
    let bars_seen_m1 = source_bars.len();
    let mut duplicate_exact_m1_count = 0;
    let mut deduped_by_open_ts = BTreeMap::<DateTime<Utc>, Bar>::new();

    for bar in source_bars {
        if bar.timeframe_sec != 60 {
            return Stage3FinamM10DerivationReport {
                status: Stage3FinamM10DerivationStatus::Rejected,
                emitted: None,
                bars_seen_m1,
                duplicate_exact_m1_count,
                duplicate_conflicting_m1_count: 0,
                complete_buckets: 0,
                incomplete_buckets: 0,
                reject_reason: Some(Stage3FinamM10DerivationRejectReason::NonM1SourceTimeframe {
                    actual_sec: bar.timeframe_sec,
                }),
                raw_payload_exported: false,
            };
        }

        let actual_duration_sec = (bar.close_ts - bar.open_ts).num_seconds();
        if actual_duration_sec != 60 {
            return Stage3FinamM10DerivationReport {
                status: Stage3FinamM10DerivationStatus::Rejected,
                emitted: None,
                bars_seen_m1,
                duplicate_exact_m1_count,
                duplicate_conflicting_m1_count: 0,
                complete_buckets: 0,
                incomplete_buckets: 0,
                reject_reason: Some(Stage3FinamM10DerivationRejectReason::NonM1SourceDuration {
                    actual_sec: actual_duration_sec,
                }),
                raw_payload_exported: false,
            };
        }

        match deduped_by_open_ts.get(&bar.open_ts) {
            Some(existing) if existing == &bar => {
                duplicate_exact_m1_count += 1;
            }
            Some(_) => {
                return Stage3FinamM10DerivationReport {
                    status: Stage3FinamM10DerivationStatus::Rejected,
                    emitted: None,
                    bars_seen_m1,
                    duplicate_exact_m1_count,
                    duplicate_conflicting_m1_count: 1,
                    complete_buckets: 0,
                    incomplete_buckets: 0,
                    reject_reason: Some(
                        Stage3FinamM10DerivationRejectReason::DuplicateConflictingM1,
                    ),
                    raw_payload_exported: false,
                };
            }
            None => {
                deduped_by_open_ts.insert(bar.open_ts, bar);
            }
        }
    }

    let mut aggregator = CanonicalBarAggregator::new(600);
    let mut last_buffered_count = 0;
    for bar in deduped_by_open_ts.into_values() {
        match aggregator.observe_final_source_bar(bar) {
            BarAggregationAction::Buffered { buffered_count, .. } => {
                last_buffered_count = buffered_count;
            }
            BarAggregationAction::Emitted { emitted } => {
                return Stage3FinamM10DerivationReport {
                    status: Stage3FinamM10DerivationStatus::Emitted,
                    emitted: Some(emitted),
                    bars_seen_m1,
                    duplicate_exact_m1_count,
                    duplicate_conflicting_m1_count: 0,
                    complete_buckets: 1,
                    incomplete_buckets: 0,
                    reject_reason: None,
                    raw_payload_exported: false,
                };
            }
            BarAggregationAction::Rejected { reason } => {
                let reject_reason = match reason {
                    BarAggregationRejectReason::NonContiguousSourceBar => {
                        Stage3FinamM10DerivationRejectReason::MissingM1
                    }
                    other => Stage3FinamM10DerivationRejectReason::AggregationRejected(other),
                };
                return Stage3FinamM10DerivationReport {
                    status: Stage3FinamM10DerivationStatus::Rejected,
                    emitted: None,
                    bars_seen_m1,
                    duplicate_exact_m1_count,
                    duplicate_conflicting_m1_count: 0,
                    complete_buckets: 0,
                    incomplete_buckets: usize::from(last_buffered_count > 0),
                    reject_reason: Some(reject_reason),
                    raw_payload_exported: false,
                };
            }
            BarAggregationAction::DroppedIncompleteBucket { .. } => {
                return Stage3FinamM10DerivationReport {
                    status: Stage3FinamM10DerivationStatus::Rejected,
                    emitted: None,
                    bars_seen_m1,
                    duplicate_exact_m1_count,
                    duplicate_conflicting_m1_count: 0,
                    complete_buckets: 0,
                    incomplete_buckets: 1,
                    reject_reason: Some(Stage3FinamM10DerivationRejectReason::MissingM1),
                    raw_payload_exported: false,
                };
            }
        }
    }

    Stage3FinamM10DerivationReport {
        status: Stage3FinamM10DerivationStatus::Incomplete,
        emitted: None,
        bars_seen_m1,
        duplicate_exact_m1_count,
        duplicate_conflicting_m1_count: 0,
        complete_buckets: 0,
        incomplete_buckets: usize::from(last_buffered_count > 0),
        reject_reason: Some(Stage3FinamM10DerivationRejectReason::NoCompleteBucket),
        raw_payload_exported: false,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Stage3MarketDataParityStatus {
    Pending,
    Synchronized,
    BlockedDiff,
    MissingAlorOracleStream,
    MissingFinamDerivedStream,
    NoOverlappingBuckets,
    RecoveryIncomplete,
    SessionScheduleUnknown,
    EvidenceIncomplete,
    SafetyBoundaryOpen,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stage3StrategyInputPublicationCounts {
    pub raw_m1_published_as_model_bar_count: usize,
    pub finam_derived_m10_published_as_model_bar_count: usize,
    pub alor_native_m10_oracle_bars_seen: usize,
    pub candidate_bars_rejected_before_strategy_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stage3ReportScope {
    pub instrument_symbol: String,
    pub timeframe_sec: u32,
    pub session_date: Option<String>,
    pub exchange: String,
}

impl Stage3ReportScope {
    pub fn for_target_instrument(target_instrument: &InstrumentId) -> Self {
        Self {
            instrument_symbol: target_instrument.symbol.clone(),
            timeframe_sec: 600,
            session_date: None,
            exchange: format!("{:?}", target_instrument.exchange),
        }
    }

    pub fn for_target_instrument_and_session(
        target_instrument: &InstrumentId,
        session_date: impl Into<String>,
    ) -> Self {
        Self {
            session_date: Some(session_date.into()),
            ..Self::for_target_instrument(target_instrument)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stage3AlorOracleInputSummary {
    pub source_mode: Stage3StrategyBarSourceMode,
    pub stream_role: String,
    pub timeframe_sec: u32,
    pub timestamp_policy: String,
    pub bars_seen: usize,
    pub exact_duplicate_bucket_count: usize,
    pub conflicting_duplicate_bucket_count: usize,
    pub complete_buckets: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stage3FinamCandidateInputSummary {
    pub source_mode: Stage3StrategyBarSourceMode,
    pub source_timeframe_sec: u32,
    pub target_timeframe_sec: u32,
    pub bars_seen_m1: usize,
    pub duplicate_exact_m1_count: usize,
    pub duplicate_conflicting_m1_count: usize,
    pub exact_duplicate_m10_bucket_count: usize,
    pub conflicting_duplicate_m10_bucket_count: usize,
    pub complete_buckets: usize,
    pub incomplete_buckets: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stage3AlorAssembledCrossCheckSummary {
    pub present: bool,
    pub source_mode: Stage3StrategyBarSourceMode,
    pub complete_buckets: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stage3ReportInputs {
    pub alor_oracle: Stage3AlorOracleInputSummary,
    pub finam_candidate: Stage3FinamCandidateInputSummary,
    pub alor_assembled_cross_check: Stage3AlorAssembledCrossCheckSummary,
}

impl Stage3ReportInputs {
    pub fn from_bucket_counts(
        alor_oracle_bars_seen: usize,
        finam_derived_buckets_seen: usize,
    ) -> Self {
        Self {
            alor_oracle: Stage3AlorOracleInputSummary {
                source_mode: Stage3StrategyBarSourceMode::AlorNativeBarsGetAndSubscribeTf600,
                stream_role: "StrategyOracleM10".to_string(),
                timeframe_sec: 600,
                timestamp_policy: "bucket_open_from_close_time_utc".to_string(),
                bars_seen: alor_oracle_bars_seen,
                exact_duplicate_bucket_count: 0,
                conflicting_duplicate_bucket_count: 0,
                complete_buckets: alor_oracle_bars_seen,
            },
            finam_candidate: Stage3FinamCandidateInputSummary {
                source_mode: Stage3StrategyBarSourceMode::FinamDerivedM1ToM10,
                source_timeframe_sec: 60,
                target_timeframe_sec: 600,
                bars_seen_m1: finam_derived_buckets_seen * 10,
                duplicate_exact_m1_count: 0,
                duplicate_conflicting_m1_count: 0,
                exact_duplicate_m10_bucket_count: 0,
                conflicting_duplicate_m10_bucket_count: 0,
                complete_buckets: finam_derived_buckets_seen,
                incomplete_buckets: 0,
            },
            alor_assembled_cross_check: Stage3AlorAssembledCrossCheckSummary {
                present: false,
                source_mode: Stage3StrategyBarSourceMode::AlorStandDerivedM1ToM10,
                complete_buckets: 0,
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stage3StrategyInputGateSummary {
    pub raw_m1_allowed_as_strategy_input: bool,
    pub finam_native_m10_allowed: bool,
    pub required_source_mode: Stage3StrategyBarSourceMode,
    pub required_target_timeframe_sec: u32,
    pub requires_final_bars: bool,
    pub requires_complete_aggregation: bool,
    pub requires_gap_absence_proven: bool,
    pub requires_session_filter_pass: bool,
    pub requires_first_fresh_live_final_after_replay: bool,
    pub strategy_watermark_advanced_by_raw_m1: bool,
}

impl Stage3StrategyInputGateSummary {
    pub fn strict_finam_m1_to_m10() -> Self {
        Self {
            raw_m1_allowed_as_strategy_input: false,
            finam_native_m10_allowed: false,
            required_source_mode: Stage3StrategyBarSourceMode::FinamDerivedM1ToM10,
            required_target_timeframe_sec: 600,
            requires_final_bars: true,
            requires_complete_aggregation: true,
            requires_gap_absence_proven: true,
            requires_session_filter_pass: true,
            requires_first_fresh_live_final_after_replay: true,
            strategy_watermark_advanced_by_raw_m1: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Stage3ReconnectRecoveryStatus {
    NotRequired,
    NotAttempted,
    AttemptedAndComplete,
    AttemptedAndFailed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stage3ReconnectRecoverySummary {
    pub recovery_required: bool,
    pub recovery_status: Stage3ReconnectRecoveryStatus,
    pub disconnect_observed: bool,
    pub warm_replay_attempted: bool,
    pub cold_replay_attempted: bool,
    pub replay_gap_absence_proven: bool,
    pub first_fresh_live_final_after_replay_observed: bool,
    pub entry_blocked_while_gap_unproven: bool,
}

impl Stage3ReconnectRecoverySummary {
    pub fn not_required() -> Self {
        Self {
            recovery_required: false,
            recovery_status: Stage3ReconnectRecoveryStatus::NotRequired,
            disconnect_observed: false,
            warm_replay_attempted: false,
            cold_replay_attempted: false,
            replay_gap_absence_proven: false,
            first_fresh_live_final_after_replay_observed: false,
            entry_blocked_while_gap_unproven: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stage3ComparisonPolicy {
    pub timestamp_tolerance_sec: u32,
    pub price_tolerance: String,
    pub volume_tolerance: String,
    pub open_ts_policy: String,
    pub close_ts_policy: String,
    pub ohlcv_diff_policy: String,
    pub missing_bar_policy: String,
    pub finality_policy: String,
    pub instrument_identity_policy: String,
}

impl Stage3ComparisonPolicy {
    pub fn strict_exact() -> Self {
        Self {
            timestamp_tolerance_sec: 0,
            price_tolerance: "exact_decimal".to_string(),
            volume_tolerance: "exact_decimal".to_string(),
            open_ts_policy: "bucket_open".to_string(),
            close_ts_policy: "bucket_close".to_string(),
            ohlcv_diff_policy: "blocking_on_any_nonzero_diff".to_string(),
            missing_bar_policy: "blocking".to_string(),
            finality_policy: "final_only".to_string(),
            instrument_identity_policy: "symbol_exchange_timeframe".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stage3ComparisonSummary {
    pub matched_bucket_count: usize,
    pub first_matched_bucket_open_ts: Option<DateTime<Utc>>,
    pub last_matched_bucket_open_ts: Option<DateTime<Utc>>,
    pub alor_only_bucket_count: usize,
    pub finam_only_bucket_count: usize,
    pub blocking_diff_count: usize,
    pub diagnostic_diff_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Stage3DiffCounts {
    pub missing_alor_bar: usize,
    pub missing_finam_derived_bar: usize,
    pub exact_duplicate_alor_bucket: usize,
    pub exact_duplicate_finam_bucket: usize,
    pub conflicting_duplicate_alor_bucket: usize,
    pub conflicting_duplicate_finam_bucket: usize,
    pub timestamp_mismatch: usize,
    pub ohlcv_mismatch: usize,
    pub timeframe_mismatch: usize,
    pub finality_mismatch: usize,
    pub instrument_mismatch: usize,
    pub source_kind_diagnostic: usize,
}

impl Stage3DiffCounts {
    fn blocking_total(&self) -> usize {
        self.missing_alor_bar
            + self.missing_finam_derived_bar
            + self.conflicting_duplicate_alor_bucket
            + self.conflicting_duplicate_finam_bucket
            + self.timestamp_mismatch
            + self.ohlcv_mismatch
            + self.timeframe_mismatch
            + self.finality_mismatch
            + self.instrument_mismatch
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Stage3DiffSummary {
    pub max_abs_open_diff: Price,
    pub max_abs_high_diff: Price,
    pub max_abs_low_diff: Price,
    pub max_abs_close_diff: Price,
    pub max_abs_volume_diff: Quantity,
    pub first_diff_bucket_open_ts: Option<DateTime<Utc>>,
    pub last_diff_bucket_open_ts: Option<DateTime<Utc>>,
    pub diff_counts: Stage3DiffCounts,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stage3SessionFilteringSummary {
    pub schedule_source: String,
    pub schedule_known: bool,
    pub session_state: String,
    pub weekend_filtered: bool,
    pub clearing_break_filtered: bool,
    pub unknown_schedule_blocks: bool,
}

impl Stage3SessionFilteringSummary {
    pub fn source_only_placeholder() -> Self {
        Self {
            schedule_source: "source_only_fixture_or_operator_scope".to_string(),
            schedule_known: true,
            session_state: "NotEvaluatedInStage3CSourceOnly".to_string(),
            weekend_filtered: true,
            clearing_break_filtered: true,
            unknown_schedule_blocks: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stage3SafetyBoundary {
    pub runtime_live_enabled: bool,
    pub real_finam_command_consumer_enabled: bool,
    pub strategy_driven_real_orders_enabled: bool,
    pub real_finam_post_delete_from_runtime_enabled: bool,
    pub stop_sltp_bracket_enabled: bool,
    pub ri_rts_migration_enabled: bool,
    pub usdrubf_migration_enabled: bool,
    pub i64_surrogate_adapter_enabled: bool,
    pub bo_mr_trading_logic_changed: bool,
}

impl Stage3SafetyBoundary {
    pub fn closed() -> Self {
        Self {
            runtime_live_enabled: false,
            real_finam_command_consumer_enabled: false,
            strategy_driven_real_orders_enabled: false,
            real_finam_post_delete_from_runtime_enabled: false,
            stop_sltp_bracket_enabled: false,
            ri_rts_migration_enabled: false,
            usdrubf_migration_enabled: false,
            i64_surrogate_adapter_enabled: false,
            bo_mr_trading_logic_changed: false,
        }
    }

    pub fn is_closed(&self) -> bool {
        !self.runtime_live_enabled
            && !self.real_finam_command_consumer_enabled
            && !self.strategy_driven_real_orders_enabled
            && !self.real_finam_post_delete_from_runtime_enabled
            && !self.stop_sltp_bracket_enabled
            && !self.ri_rts_migration_enabled
            && !self.usdrubf_migration_enabled
            && !self.i64_surrogate_adapter_enabled
            && !self.bo_mr_trading_logic_changed
    }
}

impl From<PaperSafetyBoundary> for Stage3SafetyBoundary {
    fn from(value: PaperSafetyBoundary) -> Self {
        Self {
            runtime_live_enabled: value.runtime_live_ready_enabled,
            real_finam_command_consumer_enabled: value.command_consumer_to_real_finam_enabled,
            strategy_driven_real_orders_enabled: value.live_orders_enabled,
            real_finam_post_delete_from_runtime_enabled: value.external_order_endpoint_enabled,
            stop_sltp_bracket_enabled: value.stop_sltp_bracket_enabled,
            ri_rts_migration_enabled: false,
            usdrubf_migration_enabled: false,
            i64_surrogate_adapter_enabled: false,
            bo_mr_trading_logic_changed: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Stage3MarketDataParityReport {
    pub schema_version: u16,
    pub stage: String,
    pub substage: String,
    pub generated_at: Option<DateTime<Utc>>,
    pub source_commit: Option<String>,
    pub source_archive_name: Option<String>,
    pub source_archive_sha256: Option<String>,
    pub raw_payload_exported: bool,
    pub scope: Stage3ReportScope,
    pub inputs: Stage3ReportInputs,
    pub strategy_input_gate: Stage3StrategyInputGateSummary,
    pub status: Stage3MarketDataParityStatus,
    pub comparison_policy: Stage3ComparisonPolicy,
    pub strategy_input_publication: Stage3StrategyInputPublicationCounts,
    pub reconnect_recovery: Stage3ReconnectRecoverySummary,
    pub session_filtering: Stage3SessionFilteringSummary,
    pub comparison_summary: Stage3ComparisonSummary,
    pub diff_summary: Stage3DiffSummary,
    pub safety_boundary: Stage3SafetyBoundary,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Stage3dControlledEvidenceInput {
    pub generated_at: DateTime<Utc>,
    pub source_commit: String,
    pub source_archive_name: String,
    pub source_archive_sha256: String,
    pub session_date: String,
    pub target_instrument: InstrumentId,
    pub alor_native_m10_oracle_bars: Vec<Bar>,
    pub finam_final_m1_bars: Vec<Bar>,
    pub reconnect_recovery: Stage3ReconnectRecoverySummary,
    pub session_filtering: Stage3SessionFilteringSummary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Stage3d3ApprovedInputSourceKind {
    AlorNativeM10Oracle,
    FinamFinalM1,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Stage3d3ApprovedInputSource {
    pub schema_version: u16,
    pub source_kind: Stage3d3ApprovedInputSourceKind,
    pub source_label: String,
    pub session_date: String,
    pub target_instrument: InstrumentId,
    pub raw_payload_exported: bool,
    pub bars: Vec<Bar>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Stage3d3OperatorRunAdapterConfig {
    pub generated_at: DateTime<Utc>,
    pub source_commit: String,
    pub source_archive_name: String,
    pub source_archive_sha256: String,
    pub session_date: String,
    pub target_instrument: InstrumentId,
    pub alor_source_path: PathBuf,
    pub finam_source_path: PathBuf,
    pub report_output_path: PathBuf,
    pub operator_summary_output_path: PathBuf,
    pub reconnect_recovery: Stage3ReconnectRecoverySummary,
    pub session_filtering: Stage3SessionFilteringSummary,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Stage3d3OperatorRunSummary {
    pub schema_version: u16,
    pub stage: String,
    pub generated_at: DateTime<Utc>,
    pub source_commit: String,
    pub source_archive_name: String,
    pub source_archive_sha256: String,
    pub session_date: String,
    pub instrument_symbol: String,
    pub alor_source_label: String,
    pub finam_source_label: String,
    pub alor_native_m10_bars_seen: usize,
    pub finam_final_m1_bars_seen: usize,
    pub report_status: Stage3MarketDataParityStatus,
    pub report_written: bool,
    pub operator_summary_written: bool,
    pub raw_payload_exported: bool,
    pub runtime_live_enabled: bool,
    pub real_finam_command_consumer_enabled: bool,
    pub strategy_driven_real_orders_enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum Stage3dControlledEvidenceError {
    #[error("source_commit is required")]
    MissingSourceCommit,
    #[error("source_archive_name is required")]
    MissingSourceArchiveName,
    #[error("source_archive_sha256 is required")]
    MissingSourceArchiveSha256,
    #[error("source_archive_sha256 must be 64 lowercase/uppercase hex chars")]
    InvalidSourceArchiveSha256,
    #[error("session_date is required")]
    MissingSessionDate,
    #[error("session_date must use YYYY-MM-DD format")]
    InvalidSessionDate,
    #[error("target instrument symbol is required")]
    MissingInstrumentSymbol,
    #[error("recovery_required and recovery_status are inconsistent")]
    InvalidRecoveryStatus,
    #[error("AttemptedAndComplete recovery requires replay, gap proof, fresh final bar, and blocked entry while gap was unproven")]
    InvalidRecoveryCompletionFlags,
    #[error("recovery attempt flags are inconsistent with recovery_status")]
    InvalidRecoveryAttemptFlags,
    #[error("unknown session schedule must block Stage 3 strategy-input evidence")]
    InvalidSessionFilteringState,
    #[error("ALOR oracle bar must be final")]
    AlorOracleNonFinal,
    #[error("ALOR oracle bar must use M10 timeframe, actual_sec={actual_sec}")]
    AlorOracleNonM10 { actual_sec: u32 },
    #[error("ALOR oracle bar must span 600 seconds, actual_sec={actual_sec}")]
    AlorOracleWrongDuration { actual_sec: i64 },
    #[error("ALOR oracle bar instrument must match target instrument")]
    AlorOracleWrongInstrument,
    #[error("approved source read failed: {0}")]
    ApprovedSourceRead(String),
    #[error("approved source decode failed: {0}")]
    ApprovedSourceDecode(String),
    #[error("approved source kind mismatch")]
    ApprovedSourceKindMismatch,
    #[error("approved source label is required")]
    MissingApprovedSourceLabel,
    #[error("approved source must not export raw payloads")]
    ApprovedSourceRawPayloadExported,
    #[error("approved source session date does not match operator run")]
    ApprovedSourceSessionMismatch,
    #[error("approved source target instrument does not match operator run")]
    ApprovedSourceInstrumentMismatch,
    #[error("approved source bars are required")]
    MissingApprovedSourceBars,
    #[error("approved FINAM source bar must be final M1")]
    FinamSourceBarNotFinalM1,
    #[error("approved FINAM source bar must span 60 seconds, actual_sec={actual_sec}")]
    FinamSourceBarWrongDuration { actual_sec: i64 },
    #[error("operator summary serialization failed: {0}")]
    OperatorSummarySerialization(String),
    #[error("operator summary write failed: {0}")]
    OperatorSummaryWrite(String),
    #[error("report serialization failed: {0}")]
    Serialization(String),
    #[error("report write failed: {0}")]
    Write(String),
}

impl Stage3dControlledEvidenceInput {
    fn validate(&self) -> Result<(), Stage3dControlledEvidenceError> {
        if self.source_commit.trim().is_empty() {
            return Err(Stage3dControlledEvidenceError::MissingSourceCommit);
        }
        if self.source_archive_name.trim().is_empty() {
            return Err(Stage3dControlledEvidenceError::MissingSourceArchiveName);
        }
        if self.source_archive_sha256.trim().is_empty() {
            return Err(Stage3dControlledEvidenceError::MissingSourceArchiveSha256);
        }
        let source_archive_sha256 = self.source_archive_sha256.trim();
        if source_archive_sha256.len() != 64
            || !source_archive_sha256
                .as_bytes()
                .iter()
                .all(u8::is_ascii_hexdigit)
        {
            return Err(Stage3dControlledEvidenceError::InvalidSourceArchiveSha256);
        }
        if self.session_date.trim().is_empty() {
            return Err(Stage3dControlledEvidenceError::MissingSessionDate);
        }
        if NaiveDate::parse_from_str(self.session_date.trim(), "%Y-%m-%d").is_err() {
            return Err(Stage3dControlledEvidenceError::InvalidSessionDate);
        }
        if self.target_instrument.symbol.trim().is_empty() {
            return Err(Stage3dControlledEvidenceError::MissingInstrumentSymbol);
        }
        match (
            self.reconnect_recovery.recovery_required,
            self.reconnect_recovery.recovery_status,
        ) {
            (false, Stage3ReconnectRecoveryStatus::NotRequired)
            | (true, Stage3ReconnectRecoveryStatus::NotAttempted)
            | (true, Stage3ReconnectRecoveryStatus::AttemptedAndComplete)
            | (true, Stage3ReconnectRecoveryStatus::AttemptedAndFailed) => {}
            _ => return Err(Stage3dControlledEvidenceError::InvalidRecoveryStatus),
        }
        match self.reconnect_recovery.recovery_status {
            Stage3ReconnectRecoveryStatus::AttemptedAndComplete => {
                if !self.reconnect_recovery.replay_gap_absence_proven
                    || !self
                        .reconnect_recovery
                        .first_fresh_live_final_after_replay_observed
                    || !(self.reconnect_recovery.warm_replay_attempted
                        || self.reconnect_recovery.cold_replay_attempted)
                    || !self.reconnect_recovery.entry_blocked_while_gap_unproven
                {
                    return Err(Stage3dControlledEvidenceError::InvalidRecoveryCompletionFlags);
                }
            }
            Stage3ReconnectRecoveryStatus::NotAttempted => {
                if self.reconnect_recovery.warm_replay_attempted
                    || self.reconnect_recovery.cold_replay_attempted
                    || self.reconnect_recovery.replay_gap_absence_proven
                    || self
                        .reconnect_recovery
                        .first_fresh_live_final_after_replay_observed
                {
                    return Err(Stage3dControlledEvidenceError::InvalidRecoveryAttemptFlags);
                }
            }
            Stage3ReconnectRecoveryStatus::AttemptedAndFailed => {
                if !(self.reconnect_recovery.warm_replay_attempted
                    || self.reconnect_recovery.cold_replay_attempted)
                    || (self.reconnect_recovery.replay_gap_absence_proven
                        && self
                            .reconnect_recovery
                            .first_fresh_live_final_after_replay_observed)
                {
                    return Err(Stage3dControlledEvidenceError::InvalidRecoveryAttemptFlags);
                }
            }
            Stage3ReconnectRecoveryStatus::NotRequired => {}
        }
        if !self.session_filtering.schedule_known && !self.session_filtering.unknown_schedule_blocks
        {
            return Err(Stage3dControlledEvidenceError::InvalidSessionFilteringState);
        }
        for bar in &self.alor_native_m10_oracle_bars {
            if !bar.is_final {
                return Err(Stage3dControlledEvidenceError::AlorOracleNonFinal);
            }
            if bar.timeframe_sec != 600 {
                return Err(Stage3dControlledEvidenceError::AlorOracleNonM10 {
                    actual_sec: bar.timeframe_sec,
                });
            }
            let actual_duration_sec = (bar.close_ts - bar.open_ts).num_seconds();
            if actual_duration_sec != 600 {
                return Err(Stage3dControlledEvidenceError::AlorOracleWrongDuration {
                    actual_sec: actual_duration_sec,
                });
            }
            if bar.instrument != self.target_instrument {
                return Err(Stage3dControlledEvidenceError::AlorOracleWrongInstrument);
            }
        }
        Ok(())
    }
}

impl Stage3d3ApprovedInputSource {
    fn validate_for_operator_run(
        &self,
        expected_kind: Stage3d3ApprovedInputSourceKind,
        expected_session_date: &str,
        expected_target_instrument: &InstrumentId,
    ) -> Result<(), Stage3dControlledEvidenceError> {
        if self.schema_version != STAGE3_MARKET_DATA_PARITY_SCHEMA_VERSION {
            return Err(Stage3dControlledEvidenceError::ApprovedSourceDecode(
                "unsupported schema_version".to_string(),
            ));
        }
        if self.source_kind != expected_kind {
            return Err(Stage3dControlledEvidenceError::ApprovedSourceKindMismatch);
        }
        if self.source_label.trim().is_empty() {
            return Err(Stage3dControlledEvidenceError::MissingApprovedSourceLabel);
        }
        if self.raw_payload_exported {
            return Err(Stage3dControlledEvidenceError::ApprovedSourceRawPayloadExported);
        }
        if self.session_date != expected_session_date {
            return Err(Stage3dControlledEvidenceError::ApprovedSourceSessionMismatch);
        }
        if self.target_instrument != *expected_target_instrument {
            return Err(Stage3dControlledEvidenceError::ApprovedSourceInstrumentMismatch);
        }
        if self.bars.is_empty() {
            return Err(Stage3dControlledEvidenceError::MissingApprovedSourceBars);
        }
        for bar in &self.bars {
            if bar.instrument != *expected_target_instrument {
                return Err(Stage3dControlledEvidenceError::ApprovedSourceInstrumentMismatch);
            }
            match expected_kind {
                Stage3d3ApprovedInputSourceKind::AlorNativeM10Oracle => {}
                Stage3d3ApprovedInputSourceKind::FinamFinalM1 => {
                    if !bar.is_final || bar.timeframe_sec != 60 {
                        return Err(Stage3dControlledEvidenceError::FinamSourceBarNotFinalM1);
                    }
                    let actual_duration_sec = (bar.close_ts - bar.open_ts).num_seconds();
                    if actual_duration_sec != 60 {
                        return Err(
                            Stage3dControlledEvidenceError::FinamSourceBarWrongDuration {
                                actual_sec: actual_duration_sec,
                            },
                        );
                    }
                }
            }
        }
        Ok(())
    }
}

pub fn compare_stage3_alor_native_m10_to_finam_derived_m10(
    alor_oracle: Option<&Bar>,
    finam_candidate: Option<&Bar>,
    target_instrument: &InstrumentId,
) -> Stage3MarketDataParityReport {
    let safety_boundary = Stage3SafetyBoundary::closed();
    let mut diff_counts = Stage3DiffCounts::default();
    let mut first_diff_bucket_open_ts = None;
    let mut last_diff_bucket_open_ts = None;
    let mut matched_bucket_count = 0;
    let mut first_matched_bucket_open_ts = None;
    let mut last_matched_bucket_open_ts = None;
    let mut alor_only_bucket_count = 0;
    let mut finam_only_bucket_count = 0;
    let mut diagnostic_diff_count = 0;

    match (alor_oracle, finam_candidate) {
        (None, None) => {}
        (None, Some(_)) => {
            diff_counts.missing_alor_bar = 1;
            finam_only_bucket_count = 1;
        }
        (Some(_), None) => {
            diff_counts.missing_finam_derived_bar = 1;
            alor_only_bucket_count = 1;
        }
        (Some(alor), Some(finam)) => {
            matched_bucket_count = 1;
            first_matched_bucket_open_ts = Some(alor.open_ts);
            last_matched_bucket_open_ts = Some(alor.open_ts);
            let mut record_blocking_diff = |open_ts: DateTime<Utc>| {
                if first_diff_bucket_open_ts.is_none() {
                    first_diff_bucket_open_ts = Some(open_ts);
                }
                last_diff_bucket_open_ts = Some(open_ts);
            };

            if alor.instrument != *target_instrument || finam.instrument != *target_instrument {
                diff_counts.instrument_mismatch += 1;
                record_blocking_diff(alor.open_ts);
            }
            if alor.timeframe_sec != 600 || finam.timeframe_sec != 600 {
                diff_counts.timeframe_mismatch += 1;
                record_blocking_diff(alor.open_ts);
            }
            if !alor.is_final || !finam.is_final {
                diff_counts.finality_mismatch += 1;
                record_blocking_diff(alor.open_ts);
            }
            if alor.open_ts != finam.open_ts || alor.close_ts != finam.close_ts {
                diff_counts.timestamp_mismatch += 1;
                record_blocking_diff(alor.open_ts);
            }
            if alor.open != finam.open
                || alor.high != finam.high
                || alor.low != finam.low
                || alor.close != finam.close
                || alor.volume != finam.volume
            {
                diff_counts.ohlcv_mismatch += 1;
                record_blocking_diff(alor.open_ts);
            }
            if alor.source_kind != finam.source_kind {
                diff_counts.source_kind_diagnostic += 1;
                diagnostic_diff_count += 1;
            }
        }
    }

    let blocking_diff_count = diff_counts.blocking_total();
    let status = if !safety_boundary.is_closed() {
        Stage3MarketDataParityStatus::SafetyBoundaryOpen
    } else if alor_oracle.is_none() {
        Stage3MarketDataParityStatus::MissingAlorOracleStream
    } else if finam_candidate.is_none() {
        Stage3MarketDataParityStatus::MissingFinamDerivedStream
    } else if matched_bucket_count == 0 {
        Stage3MarketDataParityStatus::NoOverlappingBuckets
    } else if blocking_diff_count == 0 {
        Stage3MarketDataParityStatus::Synchronized
    } else {
        Stage3MarketDataParityStatus::BlockedDiff
    };
    let finam_derived_m10_published_as_model_bar_count =
        usize::from(status == Stage3MarketDataParityStatus::Synchronized);
    let candidate_bars_rejected_before_strategy_count = usize::from(
        finam_candidate.is_some() && status != Stage3MarketDataParityStatus::Synchronized,
    );

    let zero = Price::ZERO;
    Stage3MarketDataParityReport {
        schema_version: STAGE3_MARKET_DATA_PARITY_SCHEMA_VERSION,
        stage: STAGE3_MARKET_DATA_PARITY_STAGE.to_string(),
        substage: STAGE3_MARKET_DATA_PARITY_SUBSTAGE_3B.to_string(),
        generated_at: None,
        source_commit: None,
        source_archive_name: None,
        source_archive_sha256: None,
        raw_payload_exported: false,
        scope: Stage3ReportScope::for_target_instrument(target_instrument),
        inputs: Stage3ReportInputs::from_bucket_counts(
            usize::from(alor_oracle.is_some()),
            usize::from(finam_candidate.is_some()),
        ),
        strategy_input_gate: Stage3StrategyInputGateSummary::strict_finam_m1_to_m10(),
        status,
        comparison_policy: Stage3ComparisonPolicy::strict_exact(),
        strategy_input_publication: Stage3StrategyInputPublicationCounts {
            raw_m1_published_as_model_bar_count: 0,
            finam_derived_m10_published_as_model_bar_count,
            alor_native_m10_oracle_bars_seen: usize::from(alor_oracle.is_some()),
            candidate_bars_rejected_before_strategy_count,
        },
        reconnect_recovery: Stage3ReconnectRecoverySummary::not_required(),
        session_filtering: Stage3SessionFilteringSummary::source_only_placeholder(),
        comparison_summary: Stage3ComparisonSummary {
            matched_bucket_count,
            first_matched_bucket_open_ts,
            last_matched_bucket_open_ts,
            alor_only_bucket_count,
            finam_only_bucket_count,
            blocking_diff_count,
            diagnostic_diff_count,
        },
        diff_summary: Stage3DiffSummary {
            max_abs_open_diff: alor_oracle
                .zip(finam_candidate)
                .map(|(alor, finam)| (alor.open - finam.open).abs())
                .unwrap_or(zero),
            max_abs_high_diff: alor_oracle
                .zip(finam_candidate)
                .map(|(alor, finam)| (alor.high - finam.high).abs())
                .unwrap_or(zero),
            max_abs_low_diff: alor_oracle
                .zip(finam_candidate)
                .map(|(alor, finam)| (alor.low - finam.low).abs())
                .unwrap_or(zero),
            max_abs_close_diff: alor_oracle
                .zip(finam_candidate)
                .map(|(alor, finam)| (alor.close - finam.close).abs())
                .unwrap_or(zero),
            max_abs_volume_diff: alor_oracle
                .zip(finam_candidate)
                .map(|(alor, finam)| (alor.volume - finam.volume).abs())
                .unwrap_or(zero),
            first_diff_bucket_open_ts,
            last_diff_bucket_open_ts,
            diff_counts,
        },
        safety_boundary,
    }
}

fn record_stage3_diff_bucket(
    first_diff_bucket_open_ts: &mut Option<DateTime<Utc>>,
    last_diff_bucket_open_ts: &mut Option<DateTime<Utc>>,
    bucket_open_ts: DateTime<Utc>,
) {
    if first_diff_bucket_open_ts.is_none() {
        *first_diff_bucket_open_ts = Some(bucket_open_ts);
    }
    *last_diff_bucket_open_ts = Some(bucket_open_ts);
}

fn update_stage3_max_abs_diff(current: &mut Price, candidate: Price) {
    if candidate > *current {
        *current = candidate;
    }
}

struct Stage3NormalizedM10Buckets<'a> {
    by_open_ts: BTreeMap<DateTime<Utc>, &'a Bar>,
    exact_duplicate_bucket_count: usize,
    conflicting_duplicate_bucket_count: usize,
    conflicting_duplicate_bucket_open_ts: BTreeSet<DateTime<Utc>>,
    first_conflicting_duplicate_open_ts: Option<DateTime<Utc>>,
    last_conflicting_duplicate_open_ts: Option<DateTime<Utc>>,
}

fn normalize_stage3_m10_buckets_for_report(bars: &[Bar]) -> Stage3NormalizedM10Buckets<'_> {
    let mut by_open_ts = BTreeMap::new();
    let mut exact_duplicate_bucket_count = 0;
    let mut conflicting_duplicate_bucket_open_ts = BTreeSet::new();

    for bar in bars {
        match by_open_ts.get(&bar.open_ts).copied() {
            None => {
                by_open_ts.insert(bar.open_ts, bar);
            }
            Some(existing) if existing == bar => {
                exact_duplicate_bucket_count += 1;
            }
            Some(_) => {
                conflicting_duplicate_bucket_open_ts.insert(bar.open_ts);
            }
        }
    }

    let first_conflicting_duplicate_open_ts =
        conflicting_duplicate_bucket_open_ts.iter().next().copied();
    let last_conflicting_duplicate_open_ts = conflicting_duplicate_bucket_open_ts
        .iter()
        .next_back()
        .copied();

    Stage3NormalizedM10Buckets {
        by_open_ts,
        exact_duplicate_bucket_count,
        conflicting_duplicate_bucket_count: conflicting_duplicate_bucket_open_ts.len(),
        conflicting_duplicate_bucket_open_ts,
        first_conflicting_duplicate_open_ts,
        last_conflicting_duplicate_open_ts,
    }
}

pub fn generate_stage3c_redacted_m10_parity_report(
    alor_oracle_bars: &[Bar],
    finam_derived_bars: &[Bar],
    target_instrument: &InstrumentId,
) -> Stage3MarketDataParityReport {
    let safety_boundary = Stage3SafetyBoundary::closed();
    let alor_normalized = normalize_stage3_m10_buckets_for_report(alor_oracle_bars);
    let finam_normalized = normalize_stage3_m10_buckets_for_report(finam_derived_bars);
    let all_bucket_open_ts = alor_normalized
        .by_open_ts
        .keys()
        .chain(finam_normalized.by_open_ts.keys())
        .copied()
        .collect::<BTreeSet<_>>();

    let mut matched_bucket_count = 0;
    let mut first_matched_bucket_open_ts = None;
    let mut last_matched_bucket_open_ts = None;
    let mut alor_only_bucket_count = 0;
    let mut finam_only_bucket_count = 0;
    let mut diagnostic_diff_count = 0;
    let mut synchronized_candidate_count = 0;
    let mut rejected_candidate_count = 0;
    let mut diff_counts = Stage3DiffCounts {
        exact_duplicate_alor_bucket: alor_normalized.exact_duplicate_bucket_count,
        exact_duplicate_finam_bucket: finam_normalized.exact_duplicate_bucket_count,
        conflicting_duplicate_alor_bucket: alor_normalized.conflicting_duplicate_bucket_count,
        conflicting_duplicate_finam_bucket: finam_normalized.conflicting_duplicate_bucket_count,
        ..Default::default()
    };
    let mut first_diff_bucket_open_ts = None;
    let mut last_diff_bucket_open_ts = None;
    let mut max_abs_open_diff = Price::ZERO;
    let mut max_abs_high_diff = Price::ZERO;
    let mut max_abs_low_diff = Price::ZERO;
    let mut max_abs_close_diff = Price::ZERO;
    let mut max_abs_volume_diff = Quantity::ZERO;

    for duplicate_open_ts in [
        alor_normalized.first_conflicting_duplicate_open_ts,
        alor_normalized.last_conflicting_duplicate_open_ts,
        finam_normalized.first_conflicting_duplicate_open_ts,
        finam_normalized.last_conflicting_duplicate_open_ts,
    ]
    .into_iter()
    .flatten()
    {
        record_stage3_diff_bucket(
            &mut first_diff_bucket_open_ts,
            &mut last_diff_bucket_open_ts,
            duplicate_open_ts,
        );
    }
    diagnostic_diff_count += alor_normalized.exact_duplicate_bucket_count
        + finam_normalized.exact_duplicate_bucket_count;

    for bucket_open_ts in all_bucket_open_ts {
        match (
            alor_normalized.by_open_ts.get(&bucket_open_ts).copied(),
            finam_normalized.by_open_ts.get(&bucket_open_ts).copied(),
        ) {
            (Some(alor), Some(finam)) => {
                matched_bucket_count += 1;
                if first_matched_bucket_open_ts.is_none() {
                    first_matched_bucket_open_ts = Some(bucket_open_ts);
                }
                last_matched_bucket_open_ts = Some(bucket_open_ts);
                let blocking_before = diff_counts.blocking_total();
                let duplicate_conflict_for_bucket = alor_normalized
                    .conflicting_duplicate_bucket_open_ts
                    .contains(&bucket_open_ts)
                    || finam_normalized
                        .conflicting_duplicate_bucket_open_ts
                        .contains(&bucket_open_ts);

                if alor.instrument != *target_instrument || finam.instrument != *target_instrument {
                    diff_counts.instrument_mismatch += 1;
                    record_stage3_diff_bucket(
                        &mut first_diff_bucket_open_ts,
                        &mut last_diff_bucket_open_ts,
                        bucket_open_ts,
                    );
                }
                if alor.timeframe_sec != 600 || finam.timeframe_sec != 600 {
                    diff_counts.timeframe_mismatch += 1;
                    record_stage3_diff_bucket(
                        &mut first_diff_bucket_open_ts,
                        &mut last_diff_bucket_open_ts,
                        bucket_open_ts,
                    );
                }
                if !alor.is_final || !finam.is_final {
                    diff_counts.finality_mismatch += 1;
                    record_stage3_diff_bucket(
                        &mut first_diff_bucket_open_ts,
                        &mut last_diff_bucket_open_ts,
                        bucket_open_ts,
                    );
                }
                if alor.open_ts != finam.open_ts || alor.close_ts != finam.close_ts {
                    diff_counts.timestamp_mismatch += 1;
                    record_stage3_diff_bucket(
                        &mut first_diff_bucket_open_ts,
                        &mut last_diff_bucket_open_ts,
                        bucket_open_ts,
                    );
                }

                let open_diff = (alor.open - finam.open).abs();
                let high_diff = (alor.high - finam.high).abs();
                let low_diff = (alor.low - finam.low).abs();
                let close_diff = (alor.close - finam.close).abs();
                let volume_diff = (alor.volume - finam.volume).abs();
                update_stage3_max_abs_diff(&mut max_abs_open_diff, open_diff);
                update_stage3_max_abs_diff(&mut max_abs_high_diff, high_diff);
                update_stage3_max_abs_diff(&mut max_abs_low_diff, low_diff);
                update_stage3_max_abs_diff(&mut max_abs_close_diff, close_diff);
                update_stage3_max_abs_diff(&mut max_abs_volume_diff, volume_diff);
                if open_diff != Price::ZERO
                    || high_diff != Price::ZERO
                    || low_diff != Price::ZERO
                    || close_diff != Price::ZERO
                    || volume_diff != Quantity::ZERO
                {
                    diff_counts.ohlcv_mismatch += 1;
                    record_stage3_diff_bucket(
                        &mut first_diff_bucket_open_ts,
                        &mut last_diff_bucket_open_ts,
                        bucket_open_ts,
                    );
                }
                if alor.source_kind != finam.source_kind {
                    diff_counts.source_kind_diagnostic += 1;
                    diagnostic_diff_count += 1;
                }

                if !duplicate_conflict_for_bucket && diff_counts.blocking_total() == blocking_before
                {
                    synchronized_candidate_count += 1;
                } else {
                    rejected_candidate_count += 1;
                }
            }
            (Some(_), None) => {
                alor_only_bucket_count += 1;
                diff_counts.missing_finam_derived_bar += 1;
                record_stage3_diff_bucket(
                    &mut first_diff_bucket_open_ts,
                    &mut last_diff_bucket_open_ts,
                    bucket_open_ts,
                );
            }
            (None, Some(_)) => {
                finam_only_bucket_count += 1;
                rejected_candidate_count += 1;
                diff_counts.missing_alor_bar += 1;
                record_stage3_diff_bucket(
                    &mut first_diff_bucket_open_ts,
                    &mut last_diff_bucket_open_ts,
                    bucket_open_ts,
                );
            }
            (None, None) => {}
        }
    }

    let blocking_diff_count = diff_counts.blocking_total();
    let status = if !safety_boundary.is_closed() {
        Stage3MarketDataParityStatus::SafetyBoundaryOpen
    } else if alor_oracle_bars.is_empty() && finam_derived_bars.is_empty() {
        Stage3MarketDataParityStatus::EvidenceIncomplete
    } else if alor_oracle_bars.is_empty() {
        Stage3MarketDataParityStatus::MissingAlorOracleStream
    } else if finam_derived_bars.is_empty() {
        Stage3MarketDataParityStatus::MissingFinamDerivedStream
    } else if matched_bucket_count == 0 {
        Stage3MarketDataParityStatus::NoOverlappingBuckets
    } else if blocking_diff_count == 0 {
        Stage3MarketDataParityStatus::Synchronized
    } else {
        Stage3MarketDataParityStatus::BlockedDiff
    };
    let mut inputs =
        Stage3ReportInputs::from_bucket_counts(alor_oracle_bars.len(), finam_derived_bars.len());
    inputs.alor_oracle.complete_buckets = alor_normalized.by_open_ts.len();
    inputs.alor_oracle.exact_duplicate_bucket_count = alor_normalized.exact_duplicate_bucket_count;
    inputs.alor_oracle.conflicting_duplicate_bucket_count =
        alor_normalized.conflicting_duplicate_bucket_count;
    inputs.finam_candidate.complete_buckets = finam_normalized.by_open_ts.len();
    inputs.finam_candidate.bars_seen_m1 = finam_normalized.by_open_ts.len() * 10;
    inputs.finam_candidate.exact_duplicate_m10_bucket_count =
        finam_normalized.exact_duplicate_bucket_count;
    inputs
        .finam_candidate
        .conflicting_duplicate_m10_bucket_count =
        finam_normalized.conflicting_duplicate_bucket_count;

    Stage3MarketDataParityReport {
        schema_version: STAGE3_MARKET_DATA_PARITY_SCHEMA_VERSION,
        stage: STAGE3_MARKET_DATA_PARITY_STAGE.to_string(),
        substage: STAGE3_MARKET_DATA_PARITY_SUBSTAGE_3C.to_string(),
        generated_at: None,
        source_commit: None,
        source_archive_name: None,
        source_archive_sha256: None,
        raw_payload_exported: false,
        scope: Stage3ReportScope::for_target_instrument(target_instrument),
        inputs,
        strategy_input_gate: Stage3StrategyInputGateSummary::strict_finam_m1_to_m10(),
        status,
        comparison_policy: Stage3ComparisonPolicy::strict_exact(),
        strategy_input_publication: Stage3StrategyInputPublicationCounts {
            raw_m1_published_as_model_bar_count: 0,
            finam_derived_m10_published_as_model_bar_count: synchronized_candidate_count,
            alor_native_m10_oracle_bars_seen: alor_oracle_bars.len(),
            candidate_bars_rejected_before_strategy_count: rejected_candidate_count,
        },
        reconnect_recovery: Stage3ReconnectRecoverySummary::not_required(),
        session_filtering: Stage3SessionFilteringSummary::source_only_placeholder(),
        comparison_summary: Stage3ComparisonSummary {
            matched_bucket_count,
            first_matched_bucket_open_ts,
            last_matched_bucket_open_ts,
            alor_only_bucket_count,
            finam_only_bucket_count,
            blocking_diff_count,
            diagnostic_diff_count,
        },
        diff_summary: Stage3DiffSummary {
            max_abs_open_diff,
            max_abs_high_diff,
            max_abs_low_diff,
            max_abs_close_diff,
            max_abs_volume_diff,
            first_diff_bucket_open_ts,
            last_diff_bucket_open_ts,
            diff_counts,
        },
        safety_boundary,
    }
}

fn stage3_bucket_open_ts(
    open_ts: DateTime<Utc>,
    target_timeframe_sec: u32,
) -> Option<DateTime<Utc>> {
    let timestamp = open_ts.timestamp();
    let target = i64::from(target_timeframe_sec);
    if target == 0 {
        return None;
    }
    let bucket_timestamp = timestamp - timestamp.rem_euclid(target);
    DateTime::<Utc>::from_timestamp(bucket_timestamp, 0)
}

fn suppress_stage3d_candidate_publication(report: &mut Stage3MarketDataParityReport) {
    let newly_rejected = report
        .strategy_input_publication
        .finam_derived_m10_published_as_model_bar_count;
    report
        .strategy_input_publication
        .candidate_bars_rejected_before_strategy_count += newly_rejected;
    report
        .strategy_input_publication
        .finam_derived_m10_published_as_model_bar_count = 0;
}

fn apply_stage3d_recovery_and_session_gates(report: &mut Stage3MarketDataParityReport) {
    if report.reconnect_recovery.recovery_required
        && report.reconnect_recovery.recovery_status
            != Stage3ReconnectRecoveryStatus::AttemptedAndComplete
    {
        suppress_stage3d_candidate_publication(report);
        report.status = Stage3MarketDataParityStatus::RecoveryIncomplete;
    }

    if !report.session_filtering.schedule_known && report.session_filtering.unknown_schedule_blocks
    {
        suppress_stage3d_candidate_publication(report);
        report.status = Stage3MarketDataParityStatus::SessionScheduleUnknown;
    }
}

pub fn collect_stage3d_controlled_active_session_evidence(
    input: Stage3dControlledEvidenceInput,
) -> Result<Stage3MarketDataParityReport, Stage3dControlledEvidenceError> {
    input.validate()?;

    let mut finam_m1_by_bucket = BTreeMap::<DateTime<Utc>, Vec<Bar>>::new();
    for bar in &input.finam_final_m1_bars {
        let bucket_open_ts = stage3_bucket_open_ts(bar.open_ts, 600).unwrap_or(bar.open_ts);
        finam_m1_by_bucket
            .entry(bucket_open_ts)
            .or_default()
            .push(bar.clone());
    }

    let mut finam_derived_m10_bars = Vec::new();
    let mut duplicate_exact_m1_count = 0;
    let mut duplicate_conflicting_m1_count = 0;
    let mut complete_buckets = 0;
    let mut incomplete_buckets = 0;
    let mut rejected_derivation_bucket_count = 0;

    for (_bucket_open_ts, mut bucket_bars) in finam_m1_by_bucket {
        bucket_bars.sort_by_key(|bar| bar.open_ts);
        let derivation = derive_stage3_finam_m10_from_final_m1(bucket_bars);
        duplicate_exact_m1_count += derivation.duplicate_exact_m1_count;
        duplicate_conflicting_m1_count += derivation.duplicate_conflicting_m1_count;
        complete_buckets += derivation.complete_buckets;
        incomplete_buckets += derivation.incomplete_buckets;
        if let Some(emitted) = derivation.emitted {
            finam_derived_m10_bars.push(emitted);
        } else {
            rejected_derivation_bucket_count += 1;
        }
    }

    let mut report = generate_stage3c_redacted_m10_parity_report(
        &input.alor_native_m10_oracle_bars,
        &finam_derived_m10_bars,
        &input.target_instrument,
    );
    report.substage = STAGE3_MARKET_DATA_PARITY_SUBSTAGE_3D.to_string();
    report.generated_at = Some(input.generated_at);
    report.source_commit = Some(input.source_commit);
    report.source_archive_name = Some(input.source_archive_name);
    report.source_archive_sha256 = Some(input.source_archive_sha256);
    report.scope = Stage3ReportScope::for_target_instrument_and_session(
        &input.target_instrument,
        input.session_date,
    );
    report.reconnect_recovery = input.reconnect_recovery;
    report.session_filtering = input.session_filtering;
    report.inputs.finam_candidate.bars_seen_m1 = input.finam_final_m1_bars.len();
    report.inputs.finam_candidate.duplicate_exact_m1_count = duplicate_exact_m1_count;
    report.inputs.finam_candidate.duplicate_conflicting_m1_count = duplicate_conflicting_m1_count;
    report.inputs.finam_candidate.complete_buckets = complete_buckets;
    report.inputs.finam_candidate.incomplete_buckets = incomplete_buckets;
    report
        .strategy_input_publication
        .candidate_bars_rejected_before_strategy_count += rejected_derivation_bucket_count;
    if rejected_derivation_bucket_count > 0
        && report.status == Stage3MarketDataParityStatus::Synchronized
    {
        report.status = Stage3MarketDataParityStatus::BlockedDiff;
    }
    apply_stage3d_recovery_and_session_gates(&mut report);

    Ok(report)
}

pub fn serialize_stage3d_redacted_evidence_report(
    report: &Stage3MarketDataParityReport,
) -> Result<String, Stage3dControlledEvidenceError> {
    serde_json::to_string_pretty(report)
        .map(|mut json| {
            json.push('\n');
            json
        })
        .map_err(|error| Stage3dControlledEvidenceError::Serialization(error.to_string()))
}

pub fn write_stage3d_redacted_evidence_report(
    path: impl AsRef<Path>,
    report: &Stage3MarketDataParityReport,
) -> Result<(), Stage3dControlledEvidenceError> {
    let path = path.as_ref();
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)
            .map_err(|error| Stage3dControlledEvidenceError::Write(error.to_string()))?;
    }
    let json = serialize_stage3d_redacted_evidence_report(report)?;
    fs::write(path, json).map_err(|error| Stage3dControlledEvidenceError::Write(error.to_string()))
}

fn read_stage3d3_approved_source(
    path: impl AsRef<Path>,
) -> Result<Stage3d3ApprovedInputSource, Stage3dControlledEvidenceError> {
    let raw = fs::read_to_string(path.as_ref())
        .map_err(|error| Stage3dControlledEvidenceError::ApprovedSourceRead(error.to_string()))?;
    serde_json::from_str(&raw)
        .map_err(|error| Stage3dControlledEvidenceError::ApprovedSourceDecode(error.to_string()))
}

pub fn serialize_stage3d3_operator_summary(
    summary: &Stage3d3OperatorRunSummary,
) -> Result<String, Stage3dControlledEvidenceError> {
    serde_json::to_string_pretty(summary)
        .map(|mut json| {
            json.push('\n');
            json
        })
        .map_err(|error| {
            Stage3dControlledEvidenceError::OperatorSummarySerialization(error.to_string())
        })
}

pub fn write_stage3d3_operator_summary(
    path: impl AsRef<Path>,
    summary: &Stage3d3OperatorRunSummary,
) -> Result<(), Stage3dControlledEvidenceError> {
    let path = path.as_ref();
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).map_err(|error| {
            Stage3dControlledEvidenceError::OperatorSummaryWrite(error.to_string())
        })?;
    }
    let json = serialize_stage3d3_operator_summary(summary)?;
    fs::write(path, json)
        .map_err(|error| Stage3dControlledEvidenceError::OperatorSummaryWrite(error.to_string()))
}

pub fn run_stage3d3_controlled_operator_input_adapter(
    config: Stage3d3OperatorRunAdapterConfig,
) -> Result<Stage3d3OperatorRunSummary, Stage3dControlledEvidenceError> {
    let alor_source = read_stage3d3_approved_source(&config.alor_source_path)?;
    let finam_source = read_stage3d3_approved_source(&config.finam_source_path)?;
    alor_source.validate_for_operator_run(
        Stage3d3ApprovedInputSourceKind::AlorNativeM10Oracle,
        &config.session_date,
        &config.target_instrument,
    )?;
    finam_source.validate_for_operator_run(
        Stage3d3ApprovedInputSourceKind::FinamFinalM1,
        &config.session_date,
        &config.target_instrument,
    )?;

    let report =
        collect_stage3d_controlled_active_session_evidence(Stage3dControlledEvidenceInput {
            generated_at: config.generated_at,
            source_commit: config.source_commit.clone(),
            source_archive_name: config.source_archive_name.clone(),
            source_archive_sha256: config.source_archive_sha256.clone(),
            session_date: config.session_date.clone(),
            target_instrument: config.target_instrument.clone(),
            alor_native_m10_oracle_bars: alor_source.bars.clone(),
            finam_final_m1_bars: finam_source.bars.clone(),
            reconnect_recovery: config.reconnect_recovery,
            session_filtering: config.session_filtering,
        })?;
    write_stage3d_redacted_evidence_report(&config.report_output_path, &report)?;

    let summary = Stage3d3OperatorRunSummary {
        schema_version: STAGE3_MARKET_DATA_PARITY_SCHEMA_VERSION,
        stage: STAGE3D3_OPERATOR_INPUT_ADAPTER_STAGE.to_string(),
        generated_at: config.generated_at,
        source_commit: config.source_commit,
        source_archive_name: config.source_archive_name,
        source_archive_sha256: config.source_archive_sha256,
        session_date: config.session_date,
        instrument_symbol: config.target_instrument.symbol,
        alor_source_label: alor_source.source_label,
        finam_source_label: finam_source.source_label,
        alor_native_m10_bars_seen: report.inputs.alor_oracle.bars_seen,
        finam_final_m1_bars_seen: report.inputs.finam_candidate.bars_seen_m1,
        report_status: report.status,
        report_written: true,
        operator_summary_written: true,
        raw_payload_exported: false,
        runtime_live_enabled: false,
        real_finam_command_consumer_enabled: false,
        strategy_driven_real_orders_enabled: false,
    };
    write_stage3d3_operator_summary(&config.operator_summary_output_path, &summary)?;
    Ok(summary)
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, TimeZone};
    use rust_decimal::Decimal;

    use super::*;
    use crate::instrument::{Exchange, InstrumentId, Market};

    fn instrument() -> InstrumentId {
        InstrumentId {
            symbol: "IMOEXF".to_string(),
            venue_symbol: Some("IMOEXF@RTSX".to_string()),
            exchange: Exchange::Moex,
            market: Market::Futures,
        }
    }

    fn bucket_open() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 7, 9, 9, 0, 0)
            .single()
            .expect("valid timestamp")
    }

    fn bucket_open_at(bucket_index: i64) -> DateTime<Utc> {
        bucket_open() + Duration::minutes(bucket_index * 10)
    }

    fn dec(value: i64) -> Decimal {
        Decimal::new(value, 0)
    }

    fn alor_oracle() -> Bar {
        normalize_stage3_alor_native_m10_oracle(Stage3AlorNativeM10Input {
            instrument: instrument(),
            close_time_utc: bucket_open(),
            open: dec(100),
            high: dec(114),
            low: dec(90),
            close: dec(110),
            volume: dec(145),
            is_final: true,
        })
    }

    fn alor_oracle_at(
        bucket_index: i64,
        open: i64,
        high: i64,
        low: i64,
        close: i64,
        volume: i64,
    ) -> Bar {
        normalize_stage3_alor_native_m10_oracle(Stage3AlorNativeM10Input {
            instrument: instrument(),
            close_time_utc: bucket_open_at(bucket_index),
            open: dec(open),
            high: dec(high),
            low: dec(low),
            close: dec(close),
            volume: dec(volume),
            is_final: true,
        })
    }

    fn finam_derived_m10_at(
        bucket_index: i64,
        open: i64,
        high: i64,
        low: i64,
        close: i64,
        volume: i64,
    ) -> Bar {
        let open_ts = bucket_open_at(bucket_index);
        Bar {
            instrument: instrument(),
            source_kind: MarketDataSourceKind::LiveStream,
            timeframe_sec: 600,
            open_ts,
            close_ts: open_ts + Duration::seconds(600),
            open: dec(open),
            high: dec(high),
            low: dec(low),
            close: dec(close),
            volume: dec(volume),
            is_final: true,
        }
    }

    fn finam_m1(minute: i64, open: i64, high: i64, low: i64, close: i64, volume: i64) -> Bar {
        let open_ts = bucket_open() + Duration::minutes(minute);
        Bar {
            instrument: instrument(),
            source_kind: MarketDataSourceKind::LiveStream,
            timeframe_sec: 60,
            open_ts,
            close_ts: open_ts + Duration::seconds(60),
            open: dec(open),
            high: dec(high),
            low: dec(low),
            close: dec(close),
            volume: dec(volume),
            is_final: true,
        }
    }

    fn finam_m5(
        bucket_offset: i64,
        open: i64,
        high: i64,
        low: i64,
        close: i64,
        volume: i64,
    ) -> Bar {
        let open_ts = bucket_open() + Duration::minutes(bucket_offset * 5);
        Bar {
            instrument: instrument(),
            source_kind: MarketDataSourceKind::LiveStream,
            timeframe_sec: 300,
            open_ts,
            close_ts: open_ts + Duration::seconds(300),
            open: dec(open),
            high: dec(high),
            low: dec(low),
            close: dec(close),
            volume: dec(volume),
            is_final: true,
        }
    }

    fn synchronized_finam_m1() -> Vec<Bar> {
        (0..10)
            .map(|minute| {
                finam_m1(
                    minute,
                    100 + minute,
                    105 + minute,
                    99 - minute,
                    101 + minute,
                    10 + minute,
                )
            })
            .collect()
    }

    fn stage3d_generated_at() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 7, 9, 12, 0, 0)
            .single()
            .expect("valid timestamp")
    }

    fn stage3d_input(alor: Vec<Bar>, finam_m1: Vec<Bar>) -> Stage3dControlledEvidenceInput {
        Stage3dControlledEvidenceInput {
            generated_at: stage3d_generated_at(),
            source_commit: "SOURCE_COMMIT_TEST_FULL_SHA".to_string(),
            source_archive_name: "moex-trading-project-SOURCE_TEST.zip".to_string(),
            source_archive_sha256:
                "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".to_string(),
            session_date: "2026-07-09".to_string(),
            target_instrument: instrument(),
            alor_native_m10_oracle_bars: alor,
            finam_final_m1_bars: finam_m1,
            reconnect_recovery: Stage3ReconnectRecoverySummary::not_required(),
            session_filtering: Stage3SessionFilteringSummary {
                session_state: "Open".to_string(),
                ..Stage3SessionFilteringSummary::source_only_placeholder()
            },
        }
    }

    fn stage3d3_approved_source(
        kind: Stage3d3ApprovedInputSourceKind,
        label: &str,
        bars: Vec<Bar>,
    ) -> Stage3d3ApprovedInputSource {
        Stage3d3ApprovedInputSource {
            schema_version: STAGE3_MARKET_DATA_PARITY_SCHEMA_VERSION,
            source_kind: kind,
            source_label: label.to_string(),
            session_date: "2026-07-09".to_string(),
            target_instrument: instrument(),
            raw_payload_exported: false,
            bars,
        }
    }

    fn stage3d3_temp_dir(test_name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("moex-stage3d3-{test_name}-{}", std::process::id()))
    }

    fn write_stage3d3_source(path: &Path, source: &Stage3d3ApprovedInputSource) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create source parent");
        }
        std::fs::write(
            path,
            serde_json::to_string_pretty(source).expect("source serializes"),
        )
        .expect("write source");
    }

    fn stage3d3_config(base_dir: &Path) -> Stage3d3OperatorRunAdapterConfig {
        Stage3d3OperatorRunAdapterConfig {
            generated_at: stage3d_generated_at(),
            source_commit: "SOURCE_COMMIT_TEST_FULL_SHA".to_string(),
            source_archive_name: "moex-trading-project-SOURCE_TEST.zip".to_string(),
            source_archive_sha256:
                "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".to_string(),
            session_date: "2026-07-09".to_string(),
            target_instrument: instrument(),
            alor_source_path: base_dir.join("approved-alor-m10.json"),
            finam_source_path: base_dir.join("approved-finam-m1.json"),
            report_output_path: base_dir
                .join("reports")
                .join("parity")
                .join("finam-vs-alor-m10")
                .join("2026-07-09.json"),
            operator_summary_output_path: base_dir
                .join("reports")
                .join("parity")
                .join("finam-vs-alor-m10")
                .join("2026-07-09.operator-summary.json"),
            reconnect_recovery: Stage3ReconnectRecoverySummary::not_required(),
            session_filtering: Stage3SessionFilteringSummary {
                session_state: "Open".to_string(),
                ..Stage3SessionFilteringSummary::source_only_placeholder()
            },
        }
    }

    #[test]
    fn stage3b_synthetic_alor_native_m10_vs_finam_derived_m10_synchronized_passes() {
        let alor = alor_oracle();
        let derivation = derive_stage3_finam_m10_from_final_m1(synchronized_finam_m1());
        assert_eq!(derivation.status, Stage3FinamM10DerivationStatus::Emitted);
        assert_eq!(derivation.complete_buckets, 1);
        assert!(!derivation.raw_payload_exported);
        let finam = derivation.emitted.as_ref().expect("derived M10");

        let gate = evaluate_stage3_strategy_input_gate(
            finam,
            &Stage3StrategyBarProvenance::finam_derived_m1_to_m10_complete(),
        );
        assert!(gate.accepted);
        assert!(gate.strategy_watermark_advanced);

        let report = compare_stage3_alor_native_m10_to_finam_derived_m10(
            Some(&alor),
            Some(finam),
            &instrument(),
        );

        assert_eq!(report.status, Stage3MarketDataParityStatus::Synchronized);
        assert_eq!(report.comparison_summary.matched_bucket_count, 1);
        assert_eq!(report.comparison_summary.blocking_diff_count, 0);
        assert_eq!(
            report
                .strategy_input_publication
                .raw_m1_published_as_model_bar_count,
            0
        );
        assert_eq!(
            report.reconnect_recovery.recovery_status,
            Stage3ReconnectRecoveryStatus::NotRequired
        );
        assert!(!report.raw_payload_exported);
        assert!(report.safety_boundary.is_closed());
    }

    #[test]
    fn stage3c_multi_bucket_synchronized_report_counts_and_serializes_redacted_shape() {
        let alor = vec![
            alor_oracle_at(0, 100, 110, 90, 105, 1000),
            alor_oracle_at(1, 105, 115, 95, 111, 2000),
        ];
        let finam = vec![
            finam_derived_m10_at(0, 100, 110, 90, 105, 1000),
            finam_derived_m10_at(1, 105, 115, 95, 111, 2000),
        ];

        let report = generate_stage3c_redacted_m10_parity_report(&alor, &finam, &instrument());
        let json = serde_json::to_value(&report).expect("report serializes");

        assert_eq!(report.substage, STAGE3_MARKET_DATA_PARITY_SUBSTAGE_3C);
        assert_eq!(report.status, Stage3MarketDataParityStatus::Synchronized);
        assert_eq!(report.comparison_summary.matched_bucket_count, 2);
        assert_eq!(
            report.comparison_summary.first_matched_bucket_open_ts,
            Some(bucket_open_at(0))
        );
        assert_eq!(
            report.comparison_summary.last_matched_bucket_open_ts,
            Some(bucket_open_at(1))
        );
        assert_eq!(
            report
                .strategy_input_publication
                .finam_derived_m10_published_as_model_bar_count,
            2
        );
        assert_eq!(
            report
                .strategy_input_publication
                .candidate_bars_rejected_before_strategy_count,
            0
        );
        assert!(!report.raw_payload_exported);
        assert_eq!(json["raw_payload_exported"], false);
        assert!(json.get("scope").is_some());
        assert!(json.get("inputs").is_some());
        assert!(json.get("strategy_input_gate").is_some());
        assert!(json.get("strategy_input_publication").is_some());
        assert!(json.get("comparison_summary").is_some());
        assert!(json.get("diff_summary").is_some());
        assert!(json.get("reconnect_recovery").is_some());
        assert!(json.get("session_filtering").is_some());
        assert!(json.get("safety_boundary").is_some());
    }

    #[test]
    fn stage3c_multi_bucket_report_counts_alor_only_finam_only_and_rejected_candidates() {
        let alor = vec![
            alor_oracle_at(0, 100, 110, 90, 105, 1000),
            alor_oracle_at(1, 105, 115, 95, 111, 2000),
        ];
        let finam = vec![
            finam_derived_m10_at(0, 100, 110, 90, 105, 1000),
            finam_derived_m10_at(2, 120, 125, 115, 122, 3000),
        ];

        let report = generate_stage3c_redacted_m10_parity_report(&alor, &finam, &instrument());

        assert_eq!(report.status, Stage3MarketDataParityStatus::BlockedDiff);
        assert_eq!(report.comparison_summary.matched_bucket_count, 1);
        assert_eq!(report.comparison_summary.alor_only_bucket_count, 1);
        assert_eq!(report.comparison_summary.finam_only_bucket_count, 1);
        assert_eq!(report.comparison_summary.blocking_diff_count, 2);
        assert_eq!(report.diff_summary.diff_counts.missing_finam_derived_bar, 1);
        assert_eq!(report.diff_summary.diff_counts.missing_alor_bar, 1);
        assert_eq!(
            report.diff_summary.first_diff_bucket_open_ts,
            Some(bucket_open_at(1))
        );
        assert_eq!(
            report.diff_summary.last_diff_bucket_open_ts,
            Some(bucket_open_at(2))
        );
        assert_eq!(
            report
                .strategy_input_publication
                .finam_derived_m10_published_as_model_bar_count,
            1
        );
        assert_eq!(
            report
                .strategy_input_publication
                .candidate_bars_rejected_before_strategy_count,
            1
        );
    }

    #[test]
    fn stage3c_multi_bucket_report_tracks_ohlcv_timestamp_and_max_deltas() {
        let alor = vec![
            alor_oracle_at(0, 100, 110, 90, 105, 1000),
            alor_oracle_at(1, 105, 115, 95, 111, 2000),
        ];
        let mut finam_second = finam_derived_m10_at(1, 105, 115, 95, 111, 2000);
        finam_second.close_ts += Duration::seconds(60);
        let finam = vec![
            finam_derived_m10_at(0, 100, 112, 90, 104, 1007),
            finam_second,
        ];

        let report = generate_stage3c_redacted_m10_parity_report(&alor, &finam, &instrument());

        assert_eq!(report.status, Stage3MarketDataParityStatus::BlockedDiff);
        assert_eq!(report.comparison_summary.matched_bucket_count, 2);
        assert_eq!(report.comparison_summary.blocking_diff_count, 2);
        assert_eq!(report.diff_summary.diff_counts.ohlcv_mismatch, 1);
        assert_eq!(report.diff_summary.diff_counts.timestamp_mismatch, 1);
        assert_eq!(report.diff_summary.max_abs_high_diff, dec(2));
        assert_eq!(report.diff_summary.max_abs_close_diff, dec(1));
        assert_eq!(report.diff_summary.max_abs_volume_diff, dec(7));
        assert_eq!(
            report.diff_summary.first_diff_bucket_open_ts,
            Some(bucket_open_at(0))
        );
        assert_eq!(
            report.diff_summary.last_diff_bucket_open_ts,
            Some(bucket_open_at(1))
        );
        assert_eq!(
            report
                .strategy_input_publication
                .finam_derived_m10_published_as_model_bar_count,
            0
        );
        assert_eq!(
            report
                .strategy_input_publication
                .candidate_bars_rejected_before_strategy_count,
            2
        );
    }

    #[test]
    fn stage3c_empty_and_missing_stream_reports_are_explicit_not_panics() {
        let empty = generate_stage3c_redacted_m10_parity_report(&[], &[], &instrument());
        assert_eq!(
            empty.status,
            Stage3MarketDataParityStatus::EvidenceIncomplete
        );

        let alor = vec![alor_oracle_at(0, 100, 110, 90, 105, 1000)];
        let missing_finam = generate_stage3c_redacted_m10_parity_report(&alor, &[], &instrument());
        assert_eq!(
            missing_finam.status,
            Stage3MarketDataParityStatus::MissingFinamDerivedStream
        );

        let finam = vec![finam_derived_m10_at(0, 100, 110, 90, 105, 1000)];
        let missing_alor = generate_stage3c_redacted_m10_parity_report(&[], &finam, &instrument());
        assert_eq!(
            missing_alor.status,
            Stage3MarketDataParityStatus::MissingAlorOracleStream
        );
        assert_eq!(
            missing_alor
                .strategy_input_publication
                .candidate_bars_rejected_before_strategy_count,
            1
        );
    }

    #[test]
    fn stage3c_duplicate_exact_alor_bucket_is_idempotent_and_counted() {
        let alor_bucket = alor_oracle_at(0, 100, 110, 90, 105, 1000);
        let alor = vec![alor_bucket.clone(), alor_bucket];
        let finam = vec![finam_derived_m10_at(0, 100, 110, 90, 105, 1000)];

        let report = generate_stage3c_redacted_m10_parity_report(&alor, &finam, &instrument());

        assert_eq!(report.status, Stage3MarketDataParityStatus::Synchronized);
        assert_eq!(report.comparison_summary.matched_bucket_count, 1);
        assert_eq!(report.comparison_summary.blocking_diff_count, 0);
        assert_eq!(report.comparison_summary.diagnostic_diff_count, 1);
        assert_eq!(
            report.diff_summary.diff_counts.exact_duplicate_alor_bucket,
            1
        );
        assert_eq!(report.inputs.alor_oracle.exact_duplicate_bucket_count, 1);
        assert_eq!(report.inputs.alor_oracle.complete_buckets, 1);
    }

    #[test]
    fn stage3c_duplicate_conflicting_alor_bucket_is_blocking() {
        let first = alor_oracle_at(0, 100, 110, 90, 105, 1000);
        let mut conflicting = first.clone();
        conflicting.close += dec(1);
        let alor = vec![first, conflicting];
        let finam = vec![finam_derived_m10_at(0, 100, 110, 90, 105, 1000)];

        let report = generate_stage3c_redacted_m10_parity_report(&alor, &finam, &instrument());

        assert_eq!(report.status, Stage3MarketDataParityStatus::BlockedDiff);
        assert_eq!(
            report
                .diff_summary
                .diff_counts
                .conflicting_duplicate_alor_bucket,
            1
        );
        assert_eq!(report.comparison_summary.blocking_diff_count, 1);
        assert_eq!(
            report.diff_summary.first_diff_bucket_open_ts,
            Some(bucket_open_at(0))
        );
        assert_eq!(
            report
                .strategy_input_publication
                .finam_derived_m10_published_as_model_bar_count,
            0
        );
        assert_eq!(
            report
                .strategy_input_publication
                .candidate_bars_rejected_before_strategy_count,
            1
        );
    }

    #[test]
    fn stage3c_duplicate_exact_finam_bucket_is_idempotent_and_counted() {
        let alor = vec![alor_oracle_at(0, 100, 110, 90, 105, 1000)];
        let finam_bucket = finam_derived_m10_at(0, 100, 110, 90, 105, 1000);
        let finam = vec![finam_bucket.clone(), finam_bucket];

        let report = generate_stage3c_redacted_m10_parity_report(&alor, &finam, &instrument());

        assert_eq!(report.status, Stage3MarketDataParityStatus::Synchronized);
        assert_eq!(report.comparison_summary.matched_bucket_count, 1);
        assert_eq!(report.comparison_summary.blocking_diff_count, 0);
        assert_eq!(report.comparison_summary.diagnostic_diff_count, 1);
        assert_eq!(
            report.diff_summary.diff_counts.exact_duplicate_finam_bucket,
            1
        );
        assert_eq!(
            report
                .inputs
                .finam_candidate
                .exact_duplicate_m10_bucket_count,
            1
        );
        assert_eq!(report.inputs.finam_candidate.complete_buckets, 1);
    }

    #[test]
    fn stage3c_duplicate_conflicting_finam_bucket_is_blocking() {
        let alor = vec![alor_oracle_at(0, 100, 110, 90, 105, 1000)];
        let first = finam_derived_m10_at(0, 100, 110, 90, 105, 1000);
        let mut conflicting = first.clone();
        conflicting.close += dec(1);
        let finam = vec![first, conflicting];

        let report = generate_stage3c_redacted_m10_parity_report(&alor, &finam, &instrument());

        assert_eq!(report.status, Stage3MarketDataParityStatus::BlockedDiff);
        assert_eq!(
            report
                .diff_summary
                .diff_counts
                .conflicting_duplicate_finam_bucket,
            1
        );
        assert_eq!(report.comparison_summary.blocking_diff_count, 1);
        assert_eq!(
            report
                .strategy_input_publication
                .finam_derived_m10_published_as_model_bar_count,
            0
        );
        assert_eq!(
            report
                .strategy_input_publication
                .candidate_bars_rejected_before_strategy_count,
            1
        );
    }

    #[test]
    fn stage3c_duplicate_bucket_does_not_silently_overwrite_previous_bar() {
        let alor = vec![alor_oracle_at(0, 100, 110, 90, 105, 1000)];
        let wrong_first = finam_derived_m10_at(0, 100, 111, 90, 105, 1000);
        let correct_duplicate = finam_derived_m10_at(0, 100, 110, 90, 105, 1000);
        let finam = vec![wrong_first, correct_duplicate];

        let report = generate_stage3c_redacted_m10_parity_report(&alor, &finam, &instrument());

        assert_eq!(report.status, Stage3MarketDataParityStatus::BlockedDiff);
        assert_eq!(
            report
                .diff_summary
                .diff_counts
                .conflicting_duplicate_finam_bucket,
            1
        );
        assert_eq!(report.diff_summary.diff_counts.ohlcv_mismatch, 1);
        assert_eq!(report.diff_summary.max_abs_high_diff, dec(1));
        assert_eq!(report.comparison_summary.blocking_diff_count, 2);
        assert_eq!(
            report
                .strategy_input_publication
                .finam_derived_m10_published_as_model_bar_count,
            0
        );
    }

    #[test]
    fn stage3d_controlled_evidence_populates_metadata_and_derives_finam_m10_from_m1() {
        let input = stage3d_input(vec![alor_oracle()], synchronized_finam_m1());

        let report = collect_stage3d_controlled_active_session_evidence(input)
            .expect("stage3d evidence report");
        let json = serialize_stage3d_redacted_evidence_report(&report).expect("json report");

        assert_eq!(report.substage, STAGE3_MARKET_DATA_PARITY_SUBSTAGE_3D);
        assert_eq!(report.generated_at, Some(stage3d_generated_at()));
        assert_eq!(
            report.source_commit.as_deref(),
            Some("SOURCE_COMMIT_TEST_FULL_SHA")
        );
        assert_eq!(
            report.source_archive_name.as_deref(),
            Some("moex-trading-project-SOURCE_TEST.zip")
        );
        assert_eq!(
            report.source_archive_sha256.as_deref(),
            Some("0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef")
        );
        assert_eq!(report.scope.session_date.as_deref(), Some("2026-07-09"));
        assert_eq!(report.scope.instrument_symbol, "IMOEXF");
        assert_eq!(report.status, Stage3MarketDataParityStatus::Synchronized);
        assert_eq!(report.inputs.alor_oracle.bars_seen, 1);
        assert_eq!(report.inputs.finam_candidate.bars_seen_m1, 10);
        assert_eq!(report.inputs.finam_candidate.complete_buckets, 1);
        assert_eq!(report.inputs.finam_candidate.incomplete_buckets, 0);
        assert_eq!(
            report
                .strategy_input_publication
                .finam_derived_m10_published_as_model_bar_count,
            1
        );
        assert_eq!(
            report
                .strategy_input_publication
                .candidate_bars_rejected_before_strategy_count,
            0
        );
        assert_eq!(report.session_filtering.session_state.as_str(), "Open");
        assert!(!report.raw_payload_exported);
        assert!(json.contains("\"substage\": \"Stage3D\""));
        assert!(json.contains("\"raw_payload_exported\": false"));
        assert!(!json.contains("SECRET_TOKEN"));
        assert!(!json.contains("SYNTHETIC_ACCOUNT_MARKER"));
    }

    #[test]
    fn stage3d_writer_creates_redacted_report_artifact() {
        let input = stage3d_input(vec![alor_oracle()], synchronized_finam_m1());
        let report = collect_stage3d_controlled_active_session_evidence(input)
            .expect("stage3d evidence report");
        let base_dir = std::env::temp_dir().join(format!(
            "moex-stage3d-report-write-test-{}",
            std::process::id()
        ));
        let report_path = base_dir
            .join("reports")
            .join("parity")
            .join("finam-vs-alor-m10")
            .join("2026-07-09.json");
        let _ = std::fs::remove_dir_all(&base_dir);

        write_stage3d_redacted_evidence_report(&report_path, &report)
            .expect("write redacted report");
        let written = std::fs::read_to_string(&report_path).expect("read redacted report");

        assert!(written.contains("\"substage\": \"Stage3D\""));
        assert!(written.contains("\"raw_payload_exported\": false"));
        assert!(written.contains("\"session_date\": \"2026-07-09\""));
        assert!(!written.contains("SECRET_TOKEN"));
        assert!(!written.contains("SYNTHETIC_ACCOUNT_MARKER"));

        let _ = std::fs::remove_dir_all(&base_dir);
    }

    #[test]
    fn stage3d3_operator_adapter_reads_approved_sources_and_writes_redacted_artifacts() {
        let base_dir = stage3d3_temp_dir("happy");
        let _ = std::fs::remove_dir_all(&base_dir);
        let config = stage3d3_config(&base_dir);
        write_stage3d3_source(
            &config.alor_source_path,
            &stage3d3_approved_source(
                Stage3d3ApprovedInputSourceKind::AlorNativeM10Oracle,
                "approved_alor_m10_redacted_fixture",
                vec![alor_oracle()],
            ),
        );
        write_stage3d3_source(
            &config.finam_source_path,
            &stage3d3_approved_source(
                Stage3d3ApprovedInputSourceKind::FinamFinalM1,
                "approved_finam_m1_redacted_fixture",
                synchronized_finam_m1(),
            ),
        );

        let summary = run_stage3d3_controlled_operator_input_adapter(config.clone())
            .expect("operator adapter run");
        let report_json =
            std::fs::read_to_string(&config.report_output_path).expect("report written");
        let summary_json =
            std::fs::read_to_string(&config.operator_summary_output_path).expect("summary written");

        assert_eq!(summary.stage, STAGE3D3_OPERATOR_INPUT_ADAPTER_STAGE);
        assert_eq!(
            summary.report_status,
            Stage3MarketDataParityStatus::Synchronized
        );
        assert_eq!(summary.alor_native_m10_bars_seen, 1);
        assert_eq!(summary.finam_final_m1_bars_seen, 10);
        assert!(summary.report_written);
        assert!(summary.operator_summary_written);
        assert!(!summary.raw_payload_exported);
        assert!(!summary.runtime_live_enabled);
        assert!(!summary.real_finam_command_consumer_enabled);
        assert!(!summary.strategy_driven_real_orders_enabled);
        assert!(report_json.contains("\"raw_payload_exported\": false"));
        assert!(summary_json.contains("\"stage\": \"Stage3D3ControlledOperatorInputAdapter\""));
        assert!(!summary_json.contains("\"bars\""));
        assert!(!summary_json.contains("SECRET_TOKEN"));
        assert!(!summary_json.contains("SYNTHETIC_ACCOUNT_MARKER"));

        let _ = std::fs::remove_dir_all(&base_dir);
    }

    #[test]
    fn stage3d3_operator_adapter_rejects_raw_payload_source() {
        let base_dir = stage3d3_temp_dir("raw_payload");
        let _ = std::fs::remove_dir_all(&base_dir);
        let config = stage3d3_config(&base_dir);
        let mut alor_source = stage3d3_approved_source(
            Stage3d3ApprovedInputSourceKind::AlorNativeM10Oracle,
            "approved_alor_m10_redacted_fixture",
            vec![alor_oracle()],
        );
        alor_source.raw_payload_exported = true;
        write_stage3d3_source(&config.alor_source_path, &alor_source);
        write_stage3d3_source(
            &config.finam_source_path,
            &stage3d3_approved_source(
                Stage3d3ApprovedInputSourceKind::FinamFinalM1,
                "approved_finam_m1_redacted_fixture",
                synchronized_finam_m1(),
            ),
        );

        let err = run_stage3d3_controlled_operator_input_adapter(config)
            .expect_err("raw payload source rejected");

        assert_eq!(
            err,
            Stage3dControlledEvidenceError::ApprovedSourceRawPayloadExported
        );
        let _ = std::fs::remove_dir_all(&base_dir);
    }

    #[test]
    fn stage3d3_operator_adapter_rejects_missing_source_file() {
        let base_dir = stage3d3_temp_dir("missing_source");
        let _ = std::fs::remove_dir_all(&base_dir);
        let config = stage3d3_config(&base_dir);
        write_stage3d3_source(
            &config.finam_source_path,
            &stage3d3_approved_source(
                Stage3d3ApprovedInputSourceKind::FinamFinalM1,
                "approved_finam_m1_redacted_fixture",
                synchronized_finam_m1(),
            ),
        );

        let err = run_stage3d3_controlled_operator_input_adapter(config)
            .expect_err("missing source rejected");

        assert!(matches!(
            err,
            Stage3dControlledEvidenceError::ApprovedSourceRead(_)
        ));
        let _ = std::fs::remove_dir_all(&base_dir);
    }

    #[test]
    fn stage3d3_operator_adapter_rejects_invalid_source_json() {
        let base_dir = stage3d3_temp_dir("invalid_json");
        let _ = std::fs::remove_dir_all(&base_dir);
        let config = stage3d3_config(&base_dir);
        if let Some(parent) = config.alor_source_path.parent() {
            std::fs::create_dir_all(parent).expect("create source parent");
        }
        std::fs::write(&config.alor_source_path, "{not-json").expect("write invalid json");
        write_stage3d3_source(
            &config.finam_source_path,
            &stage3d3_approved_source(
                Stage3d3ApprovedInputSourceKind::FinamFinalM1,
                "approved_finam_m1_redacted_fixture",
                synchronized_finam_m1(),
            ),
        );

        let err = run_stage3d3_controlled_operator_input_adapter(config)
            .expect_err("invalid source rejected");

        assert!(matches!(
            err,
            Stage3dControlledEvidenceError::ApprovedSourceDecode(_)
        ));
        let _ = std::fs::remove_dir_all(&base_dir);
    }

    #[test]
    fn stage3d3_operator_adapter_rejects_source_kind_mismatch() {
        let base_dir = stage3d3_temp_dir("kind_mismatch");
        let _ = std::fs::remove_dir_all(&base_dir);
        let config = stage3d3_config(&base_dir);
        write_stage3d3_source(
            &config.alor_source_path,
            &stage3d3_approved_source(
                Stage3d3ApprovedInputSourceKind::FinamFinalM1,
                "wrong_kind_for_alor_source",
                vec![alor_oracle()],
            ),
        );
        write_stage3d3_source(
            &config.finam_source_path,
            &stage3d3_approved_source(
                Stage3d3ApprovedInputSourceKind::FinamFinalM1,
                "approved_finam_m1_redacted_fixture",
                synchronized_finam_m1(),
            ),
        );

        let err = run_stage3d3_controlled_operator_input_adapter(config)
            .expect_err("source kind mismatch rejected");

        assert_eq!(
            err,
            Stage3dControlledEvidenceError::ApprovedSourceKindMismatch
        );
        let _ = std::fs::remove_dir_all(&base_dir);
    }

    #[test]
    fn stage3d3_operator_adapter_rejects_missing_source_bars() {
        let base_dir = stage3d3_temp_dir("missing_bars");
        let _ = std::fs::remove_dir_all(&base_dir);
        let config = stage3d3_config(&base_dir);
        write_stage3d3_source(
            &config.alor_source_path,
            &stage3d3_approved_source(
                Stage3d3ApprovedInputSourceKind::AlorNativeM10Oracle,
                "approved_alor_m10_redacted_fixture",
                Vec::new(),
            ),
        );
        write_stage3d3_source(
            &config.finam_source_path,
            &stage3d3_approved_source(
                Stage3d3ApprovedInputSourceKind::FinamFinalM1,
                "approved_finam_m1_redacted_fixture",
                synchronized_finam_m1(),
            ),
        );

        let err = run_stage3d3_controlled_operator_input_adapter(config)
            .expect_err("empty source rejected");

        assert_eq!(
            err,
            Stage3dControlledEvidenceError::MissingApprovedSourceBars
        );
        let _ = std::fs::remove_dir_all(&base_dir);
    }

    #[test]
    fn stage3d3_operator_adapter_rejects_invalid_finam_source_shape() {
        let base_dir = stage3d3_temp_dir("bad_finam_shape");
        let _ = std::fs::remove_dir_all(&base_dir);
        let config = stage3d3_config(&base_dir);
        let mut bad_finam = synchronized_finam_m1();
        bad_finam[0].timeframe_sec = 300;
        write_stage3d3_source(
            &config.alor_source_path,
            &stage3d3_approved_source(
                Stage3d3ApprovedInputSourceKind::AlorNativeM10Oracle,
                "approved_alor_m10_redacted_fixture",
                vec![alor_oracle()],
            ),
        );
        write_stage3d3_source(
            &config.finam_source_path,
            &stage3d3_approved_source(
                Stage3d3ApprovedInputSourceKind::FinamFinalM1,
                "approved_finam_m1_redacted_fixture",
                bad_finam,
            ),
        );

        let err = run_stage3d3_controlled_operator_input_adapter(config)
            .expect_err("bad FINAM source rejected");

        assert_eq!(
            err,
            Stage3dControlledEvidenceError::FinamSourceBarNotFinalM1
        );
        let _ = std::fs::remove_dir_all(&base_dir);
    }

    #[test]
    fn stage3d3_operator_adapter_rejects_source_session_mismatch() {
        let base_dir = stage3d3_temp_dir("session_mismatch");
        let _ = std::fs::remove_dir_all(&base_dir);
        let config = stage3d3_config(&base_dir);
        let mut alor_source = stage3d3_approved_source(
            Stage3d3ApprovedInputSourceKind::AlorNativeM10Oracle,
            "approved_alor_m10_redacted_fixture",
            vec![alor_oracle()],
        );
        alor_source.session_date = "2026-07-08".to_string();
        write_stage3d3_source(&config.alor_source_path, &alor_source);
        write_stage3d3_source(
            &config.finam_source_path,
            &stage3d3_approved_source(
                Stage3d3ApprovedInputSourceKind::FinamFinalM1,
                "approved_finam_m1_redacted_fixture",
                synchronized_finam_m1(),
            ),
        );

        let err = run_stage3d3_controlled_operator_input_adapter(config)
            .expect_err("source session mismatch rejected");

        assert_eq!(
            err,
            Stage3dControlledEvidenceError::ApprovedSourceSessionMismatch
        );
        let _ = std::fs::remove_dir_all(&base_dir);
    }

    #[test]
    fn stage3d_incomplete_finam_m1_bucket_is_rejected_before_strategy() {
        let mut finam_m1 = synchronized_finam_m1();
        finam_m1.remove(5);
        let input = stage3d_input(vec![alor_oracle()], finam_m1);

        let report = collect_stage3d_controlled_active_session_evidence(input)
            .expect("stage3d evidence report");

        assert_eq!(
            report.status,
            Stage3MarketDataParityStatus::MissingFinamDerivedStream
        );
        assert_eq!(report.inputs.finam_candidate.bars_seen_m1, 9);
        assert_eq!(report.inputs.finam_candidate.complete_buckets, 0);
        assert_eq!(report.inputs.finam_candidate.incomplete_buckets, 1);
        assert_eq!(
            report
                .strategy_input_publication
                .finam_derived_m10_published_as_model_bar_count,
            0
        );
        assert_eq!(
            report
                .strategy_input_publication
                .candidate_bars_rejected_before_strategy_count,
            1
        );
        assert!(!report.raw_payload_exported);
    }

    #[test]
    fn stage3d_recovery_status_is_passed_through_honestly() {
        let mut input = stage3d_input(vec![alor_oracle()], synchronized_finam_m1());
        input.reconnect_recovery = Stage3ReconnectRecoverySummary {
            recovery_required: true,
            recovery_status: Stage3ReconnectRecoveryStatus::AttemptedAndComplete,
            disconnect_observed: true,
            warm_replay_attempted: true,
            cold_replay_attempted: false,
            replay_gap_absence_proven: true,
            first_fresh_live_final_after_replay_observed: true,
            entry_blocked_while_gap_unproven: true,
        };

        let report = collect_stage3d_controlled_active_session_evidence(input)
            .expect("stage3d evidence report");

        assert!(report.reconnect_recovery.recovery_required);
        assert_eq!(
            report.reconnect_recovery.recovery_status,
            Stage3ReconnectRecoveryStatus::AttemptedAndComplete
        );
        assert!(report.reconnect_recovery.replay_gap_absence_proven);
        assert!(
            report
                .reconnect_recovery
                .first_fresh_live_final_after_replay_observed
        );
        assert_eq!(report.status, Stage3MarketDataParityStatus::Synchronized);
        assert_eq!(
            report
                .strategy_input_publication
                .finam_derived_m10_published_as_model_bar_count,
            1
        );
        assert_eq!(
            report
                .strategy_input_publication
                .candidate_bars_rejected_before_strategy_count,
            0
        );
    }

    #[test]
    fn stage3d_recovery_failed_blocks_synchronized_report() {
        let mut input = stage3d_input(vec![alor_oracle()], synchronized_finam_m1());
        input.reconnect_recovery = Stage3ReconnectRecoverySummary {
            recovery_required: true,
            recovery_status: Stage3ReconnectRecoveryStatus::AttemptedAndFailed,
            disconnect_observed: true,
            warm_replay_attempted: true,
            cold_replay_attempted: false,
            replay_gap_absence_proven: false,
            first_fresh_live_final_after_replay_observed: false,
            entry_blocked_while_gap_unproven: true,
        };

        let report = collect_stage3d_controlled_active_session_evidence(input)
            .expect("stage3d evidence report");

        assert_eq!(
            report.status,
            Stage3MarketDataParityStatus::RecoveryIncomplete
        );
        assert_eq!(
            report
                .strategy_input_publication
                .finam_derived_m10_published_as_model_bar_count,
            0
        );
        assert_eq!(
            report
                .strategy_input_publication
                .candidate_bars_rejected_before_strategy_count,
            1
        );
    }

    #[test]
    fn stage3d_recovery_not_attempted_blocks_publication() {
        let mut input = stage3d_input(vec![alor_oracle()], synchronized_finam_m1());
        input.reconnect_recovery = Stage3ReconnectRecoverySummary {
            recovery_required: true,
            recovery_status: Stage3ReconnectRecoveryStatus::NotAttempted,
            disconnect_observed: true,
            warm_replay_attempted: false,
            cold_replay_attempted: false,
            replay_gap_absence_proven: false,
            first_fresh_live_final_after_replay_observed: false,
            entry_blocked_while_gap_unproven: true,
        };

        let report = collect_stage3d_controlled_active_session_evidence(input)
            .expect("stage3d evidence report");

        assert_eq!(
            report.status,
            Stage3MarketDataParityStatus::RecoveryIncomplete
        );
        assert_eq!(
            report
                .strategy_input_publication
                .finam_derived_m10_published_as_model_bar_count,
            0
        );
        assert_eq!(
            report
                .strategy_input_publication
                .candidate_bars_rejected_before_strategy_count,
            1
        );
    }

    #[test]
    fn stage3d_recovery_required_not_required_status_is_invalid() {
        let mut input = stage3d_input(vec![alor_oracle()], synchronized_finam_m1());
        input.reconnect_recovery.recovery_required = true;
        input.reconnect_recovery.recovery_status = Stage3ReconnectRecoveryStatus::NotRequired;

        let err = collect_stage3d_controlled_active_session_evidence(input)
            .expect_err("inconsistent recovery state rejected");

        assert_eq!(err, Stage3dControlledEvidenceError::InvalidRecoveryStatus);
    }

    #[test]
    fn stage3d_recovery_not_required_with_attempted_status_is_invalid() {
        let mut input = stage3d_input(vec![alor_oracle()], synchronized_finam_m1());
        input.reconnect_recovery.recovery_required = false;
        input.reconnect_recovery.recovery_status =
            Stage3ReconnectRecoveryStatus::AttemptedAndComplete;

        let err = collect_stage3d_controlled_active_session_evidence(input)
            .expect_err("inconsistent recovery state rejected");

        assert_eq!(err, Stage3dControlledEvidenceError::InvalidRecoveryStatus);
    }

    #[test]
    fn stage3d_recovery_complete_requires_gap_absence_proven() {
        let mut input = stage3d_input(vec![alor_oracle()], synchronized_finam_m1());
        input.reconnect_recovery = Stage3ReconnectRecoverySummary {
            recovery_required: true,
            recovery_status: Stage3ReconnectRecoveryStatus::AttemptedAndComplete,
            disconnect_observed: true,
            warm_replay_attempted: true,
            cold_replay_attempted: false,
            replay_gap_absence_proven: false,
            first_fresh_live_final_after_replay_observed: true,
            entry_blocked_while_gap_unproven: true,
        };

        let err = collect_stage3d_controlled_active_session_evidence(input)
            .expect_err("complete recovery without gap proof rejected");

        assert_eq!(
            err,
            Stage3dControlledEvidenceError::InvalidRecoveryCompletionFlags
        );
    }

    #[test]
    fn stage3d_recovery_complete_requires_first_fresh_live_final() {
        let mut input = stage3d_input(vec![alor_oracle()], synchronized_finam_m1());
        input.reconnect_recovery = Stage3ReconnectRecoverySummary {
            recovery_required: true,
            recovery_status: Stage3ReconnectRecoveryStatus::AttemptedAndComplete,
            disconnect_observed: true,
            warm_replay_attempted: true,
            cold_replay_attempted: false,
            replay_gap_absence_proven: true,
            first_fresh_live_final_after_replay_observed: false,
            entry_blocked_while_gap_unproven: true,
        };

        let err = collect_stage3d_controlled_active_session_evidence(input)
            .expect_err("complete recovery without fresh live final rejected");

        assert_eq!(
            err,
            Stage3dControlledEvidenceError::InvalidRecoveryCompletionFlags
        );
    }

    #[test]
    fn stage3d_recovery_complete_requires_replay_attempt() {
        let mut input = stage3d_input(vec![alor_oracle()], synchronized_finam_m1());
        input.reconnect_recovery = Stage3ReconnectRecoverySummary {
            recovery_required: true,
            recovery_status: Stage3ReconnectRecoveryStatus::AttemptedAndComplete,
            disconnect_observed: true,
            warm_replay_attempted: false,
            cold_replay_attempted: false,
            replay_gap_absence_proven: true,
            first_fresh_live_final_after_replay_observed: true,
            entry_blocked_while_gap_unproven: true,
        };

        let err = collect_stage3d_controlled_active_session_evidence(input)
            .expect_err("complete recovery without replay attempt rejected");

        assert_eq!(
            err,
            Stage3dControlledEvidenceError::InvalidRecoveryCompletionFlags
        );
    }

    #[test]
    fn stage3d_recovery_complete_requires_entry_blocked_while_gap_unproven() {
        let mut input = stage3d_input(vec![alor_oracle()], synchronized_finam_m1());
        input.reconnect_recovery = Stage3ReconnectRecoverySummary {
            recovery_required: true,
            recovery_status: Stage3ReconnectRecoveryStatus::AttemptedAndComplete,
            disconnect_observed: true,
            warm_replay_attempted: true,
            cold_replay_attempted: false,
            replay_gap_absence_proven: true,
            first_fresh_live_final_after_replay_observed: true,
            entry_blocked_while_gap_unproven: false,
        };

        let err = collect_stage3d_controlled_active_session_evidence(input)
            .expect_err("complete recovery without blocked entry guard rejected");

        assert_eq!(
            err,
            Stage3dControlledEvidenceError::InvalidRecoveryCompletionFlags
        );
    }

    #[test]
    fn stage3d_recovery_failed_requires_replay_attempt() {
        let mut input = stage3d_input(vec![alor_oracle()], synchronized_finam_m1());
        input.reconnect_recovery = Stage3ReconnectRecoverySummary {
            recovery_required: true,
            recovery_status: Stage3ReconnectRecoveryStatus::AttemptedAndFailed,
            disconnect_observed: true,
            warm_replay_attempted: false,
            cold_replay_attempted: false,
            replay_gap_absence_proven: false,
            first_fresh_live_final_after_replay_observed: false,
            entry_blocked_while_gap_unproven: true,
        };

        let err = collect_stage3d_controlled_active_session_evidence(input)
            .expect_err("failed recovery without replay attempt rejected");

        assert_eq!(
            err,
            Stage3dControlledEvidenceError::InvalidRecoveryAttemptFlags
        );
    }

    #[test]
    fn stage3d_unknown_schedule_blocks_synchronized_report() {
        let mut input = stage3d_input(vec![alor_oracle()], synchronized_finam_m1());
        input.session_filtering.schedule_known = false;
        input.session_filtering.session_state = "Unknown".to_string();
        input.session_filtering.unknown_schedule_blocks = true;

        let report = collect_stage3d_controlled_active_session_evidence(input)
            .expect("stage3d evidence report");

        assert_eq!(
            report.status,
            Stage3MarketDataParityStatus::SessionScheduleUnknown
        );
        assert_eq!(
            report
                .strategy_input_publication
                .finam_derived_m10_published_as_model_bar_count,
            0
        );
        assert_eq!(
            report
                .strategy_input_publication
                .candidate_bars_rejected_before_strategy_count,
            1
        );
    }

    #[test]
    fn stage3d_schedule_unknown_must_block() {
        let mut input = stage3d_input(vec![alor_oracle()], synchronized_finam_m1());
        input.session_filtering.schedule_known = false;
        input.session_filtering.unknown_schedule_blocks = false;

        let err = collect_stage3d_controlled_active_session_evidence(input)
            .expect_err("unknown schedule without block rejected");

        assert_eq!(
            err,
            Stage3dControlledEvidenceError::InvalidSessionFilteringState
        );
    }

    #[test]
    fn stage3d_known_open_session_allows_synchronized_publication() {
        let input = stage3d_input(vec![alor_oracle()], synchronized_finam_m1());

        let report = collect_stage3d_controlled_active_session_evidence(input)
            .expect("stage3d evidence report");

        assert_eq!(report.status, Stage3MarketDataParityStatus::Synchronized);
        assert_eq!(
            report
                .strategy_input_publication
                .finam_derived_m10_published_as_model_bar_count,
            1
        );
        assert_eq!(report.session_filtering.session_state.as_str(), "Open");
    }

    #[test]
    fn stage3d_requires_source_metadata_before_evidence_report() {
        let mut input = stage3d_input(vec![alor_oracle()], synchronized_finam_m1());
        input.source_commit.clear();

        let err = collect_stage3d_controlled_active_session_evidence(input)
            .expect_err("missing source commit rejected");

        assert_eq!(err, Stage3dControlledEvidenceError::MissingSourceCommit);
    }

    #[test]
    fn stage3d_invalid_archive_sha256_rejected() {
        let mut input = stage3d_input(vec![alor_oracle()], synchronized_finam_m1());
        input.source_archive_sha256 = "not-a-sha256".to_string();

        let err = collect_stage3d_controlled_active_session_evidence(input)
            .expect_err("invalid archive sha rejected");

        assert_eq!(
            err,
            Stage3dControlledEvidenceError::InvalidSourceArchiveSha256
        );
    }

    #[test]
    fn stage3d_invalid_session_date_rejected() {
        let mut input = stage3d_input(vec![alor_oracle()], synchronized_finam_m1());
        input.session_date = "09-07-2026".to_string();

        let err = collect_stage3d_controlled_active_session_evidence(input)
            .expect_err("invalid session date rejected");

        assert_eq!(err, Stage3dControlledEvidenceError::InvalidSessionDate);
    }

    #[test]
    fn stage3d_non_final_alor_oracle_bar_rejected() {
        let mut alor = alor_oracle();
        alor.is_final = false;
        let input = stage3d_input(vec![alor], synchronized_finam_m1());

        let err = collect_stage3d_controlled_active_session_evidence(input)
            .expect_err("non-final alor oracle rejected");

        assert_eq!(err, Stage3dControlledEvidenceError::AlorOracleNonFinal);
    }

    #[test]
    fn stage3d_non_m10_alor_oracle_bar_rejected() {
        let mut alor = alor_oracle();
        alor.timeframe_sec = 300;
        let input = stage3d_input(vec![alor], synchronized_finam_m1());

        let err = collect_stage3d_controlled_active_session_evidence(input)
            .expect_err("non-M10 alor oracle rejected");

        assert_eq!(
            err,
            Stage3dControlledEvidenceError::AlorOracleNonM10 { actual_sec: 300 }
        );
    }

    #[test]
    fn stage3d_wrong_duration_alor_oracle_bar_rejected() {
        let mut alor = alor_oracle();
        alor.close_ts = alor.open_ts + Duration::seconds(599);
        let input = stage3d_input(vec![alor], synchronized_finam_m1());

        let err = collect_stage3d_controlled_active_session_evidence(input)
            .expect_err("wrong-duration alor oracle rejected");

        assert_eq!(
            err,
            Stage3dControlledEvidenceError::AlorOracleWrongDuration { actual_sec: 599 }
        );
    }

    #[test]
    fn stage3d_wrong_instrument_alor_oracle_bar_rejected() {
        let mut alor = alor_oracle();
        alor.instrument.symbol = "OTHER_SYNTH_FUT".to_string();
        let input = stage3d_input(vec![alor], synchronized_finam_m1());

        let err = collect_stage3d_controlled_active_session_evidence(input)
            .expect_err("wrong-instrument alor oracle rejected");

        assert_eq!(
            err,
            Stage3dControlledEvidenceError::AlorOracleWrongInstrument
        );
    }

    #[test]
    fn stage3b_alor_close_time_utc_is_bucket_open_for_m10_oracle() {
        let alor = alor_oracle();

        assert_eq!(alor.open_ts, bucket_open());
        assert_eq!(alor.close_ts, bucket_open() + Duration::seconds(600));
        assert_eq!(alor.timeframe_sec, 600);
        assert!(alor.is_final);
    }

    #[test]
    fn stage3b_finam_ten_contiguous_final_m1_bars_assemble_exactly_one_m10() {
        let derivation = derive_stage3_finam_m10_from_final_m1(synchronized_finam_m1());
        let emitted = derivation.emitted.expect("emitted");

        assert_eq!(derivation.status, Stage3FinamM10DerivationStatus::Emitted);
        assert_eq!(derivation.complete_buckets, 1);
        assert_eq!(emitted.open_ts, bucket_open());
        assert_eq!(emitted.close_ts, bucket_open() + Duration::seconds(600));
        assert_eq!(emitted.open, dec(100));
        assert_eq!(emitted.high, dec(114));
        assert_eq!(emitted.low, dec(90));
        assert_eq!(emitted.close, dec(110));
        assert_eq!(emitted.volume, dec(145));
    }

    #[test]
    fn stage3b_missing_m1_makes_bucket_incomplete_or_blocking() {
        let mut bars = synchronized_finam_m1();
        bars.remove(5);

        let derivation = derive_stage3_finam_m10_from_final_m1(bars);

        assert_ne!(derivation.status, Stage3FinamM10DerivationStatus::Emitted);
        assert!(matches!(
            derivation.reject_reason,
            Some(Stage3FinamM10DerivationRejectReason::MissingM1)
                | Some(Stage3FinamM10DerivationRejectReason::NoCompleteBucket)
        ));
        assert!(derivation.emitted.is_none());
    }

    #[test]
    fn stage3b_non_m1_source_bar_is_rejected() {
        let mut bars = synchronized_finam_m1();
        bars[4].timeframe_sec = 120;
        bars[4].close_ts = bars[4].open_ts + Duration::seconds(120);

        let derivation = derive_stage3_finam_m10_from_final_m1(bars);

        assert_eq!(derivation.status, Stage3FinamM10DerivationStatus::Rejected);
        assert_eq!(
            derivation.reject_reason,
            Some(Stage3FinamM10DerivationRejectReason::NonM1SourceTimeframe { actual_sec: 120 })
        );
        assert!(derivation.emitted.is_none());
    }

    #[test]
    fn stage3b_m5_source_bars_cannot_assemble_stage3_finam_m10() {
        let bars = vec![
            finam_m5(0, 100, 105, 99, 101, 1000),
            finam_m5(1, 101, 106, 100, 102, 1001),
        ];

        let derivation = derive_stage3_finam_m10_from_final_m1(bars);

        assert_eq!(derivation.status, Stage3FinamM10DerivationStatus::Rejected);
        assert_eq!(
            derivation.reject_reason,
            Some(Stage3FinamM10DerivationRejectReason::NonM1SourceTimeframe { actual_sec: 300 })
        );
        assert_eq!(derivation.complete_buckets, 0);
        assert!(derivation.emitted.is_none());
    }

    #[test]
    fn stage3b_m1_source_bar_with_wrong_duration_is_rejected() {
        let mut bars = synchronized_finam_m1();
        bars[4].close_ts = bars[4].open_ts + Duration::seconds(59);

        let derivation = derive_stage3_finam_m10_from_final_m1(bars);

        assert_eq!(derivation.status, Stage3FinamM10DerivationStatus::Rejected);
        assert_eq!(
            derivation.reject_reason,
            Some(Stage3FinamM10DerivationRejectReason::NonM1SourceDuration { actual_sec: 59 })
        );
        assert!(derivation.emitted.is_none());
    }

    #[test]
    fn stage3b_duplicate_exact_m1_is_idempotent() {
        let mut bars = synchronized_finam_m1();
        bars.push(bars[3].clone());

        let derivation = derive_stage3_finam_m10_from_final_m1(bars);

        assert_eq!(derivation.status, Stage3FinamM10DerivationStatus::Emitted);
        assert_eq!(derivation.duplicate_exact_m1_count, 1);
        assert_eq!(derivation.duplicate_conflicting_m1_count, 0);
    }

    #[test]
    fn stage3b_duplicate_conflicting_m1_is_blocking() {
        let mut bars = synchronized_finam_m1();
        let mut conflicting = bars[3].clone();
        conflicting.close += dec(1);
        bars.push(conflicting);

        let derivation = derive_stage3_finam_m10_from_final_m1(bars);

        assert_eq!(derivation.status, Stage3FinamM10DerivationStatus::Rejected);
        assert_eq!(derivation.duplicate_conflicting_m1_count, 1);
        assert_eq!(
            derivation.reject_reason,
            Some(Stage3FinamM10DerivationRejectReason::DuplicateConflictingM1)
        );
    }

    #[test]
    fn stage3b_raw_m1_cannot_advance_strategy_watermark() {
        let raw_m1 = finam_m1(0, 100, 101, 99, 100, 10);
        let gate = evaluate_stage3_strategy_input_gate(
            &raw_m1,
            &Stage3StrategyBarProvenance::raw_finam_m1(),
        );

        assert!(!gate.accepted);
        assert_eq!(
            gate.reason,
            Some(Stage3StrategyInputRejectReason::RawM1NotAllowed)
        );
        assert!(!gate.strategy_watermark_advanced);
    }

    #[test]
    fn stage3b_finam_native_m10_candidate_is_rejected_until_characterized() {
        let finam_native = Bar {
            source_kind: MarketDataSourceKind::LiveStream,
            ..alor_oracle()
        };

        let gate = evaluate_stage3_strategy_input_gate(
            &finam_native,
            &Stage3StrategyBarProvenance::finam_native_m10_pending(),
        );

        assert!(!gate.accepted);
        assert_eq!(
            gate.reason,
            Some(Stage3StrategyInputRejectReason::FinamNativeM10PendingCharacterization)
        );
        assert!(!gate.strategy_watermark_advanced);
    }

    #[test]
    fn stage3b_ohlcv_mismatch_produces_blocking_diff() {
        let alor = alor_oracle();
        let mut finam = alor.clone();
        finam.close += dec(1);

        let report = compare_stage3_alor_native_m10_to_finam_derived_m10(
            Some(&alor),
            Some(&finam),
            &instrument(),
        );

        assert_eq!(report.status, Stage3MarketDataParityStatus::BlockedDiff);
        assert_eq!(report.diff_summary.diff_counts.ohlcv_mismatch, 1);
        assert_eq!(report.comparison_summary.blocking_diff_count, 1);
        assert_eq!(
            report
                .strategy_input_publication
                .finam_derived_m10_published_as_model_bar_count,
            0
        );
        assert_eq!(
            report
                .strategy_input_publication
                .candidate_bars_rejected_before_strategy_count,
            1
        );
    }

    #[test]
    fn stage3b_timestamp_mismatch_produces_blocking_diff() {
        let alor = alor_oracle();
        let mut finam = alor.clone();
        finam.open_ts += Duration::seconds(600);
        finam.close_ts += Duration::seconds(600);

        let report = compare_stage3_alor_native_m10_to_finam_derived_m10(
            Some(&alor),
            Some(&finam),
            &instrument(),
        );

        assert_eq!(report.status, Stage3MarketDataParityStatus::BlockedDiff);
        assert_eq!(report.diff_summary.diff_counts.timestamp_mismatch, 1);
        assert_eq!(report.comparison_summary.blocking_diff_count, 1);
    }

    #[test]
    fn stage3b_blocked_diff_candidate_is_rejected_before_strategy_not_published() {
        let alor = alor_oracle();
        let mut finam = alor.clone();
        finam.high += dec(1);

        let report = compare_stage3_alor_native_m10_to_finam_derived_m10(
            Some(&alor),
            Some(&finam),
            &instrument(),
        );

        assert_eq!(report.status, Stage3MarketDataParityStatus::BlockedDiff);
        assert_eq!(
            report
                .strategy_input_publication
                .finam_derived_m10_published_as_model_bar_count,
            0
        );
        assert_eq!(
            report
                .strategy_input_publication
                .candidate_bars_rejected_before_strategy_count,
            1
        );
    }

    #[test]
    fn stage3b_missing_alor_candidate_is_not_published_as_strategy_model_bar() {
        let finam = alor_oracle();

        let report =
            compare_stage3_alor_native_m10_to_finam_derived_m10(None, Some(&finam), &instrument());

        assert_eq!(
            report.status,
            Stage3MarketDataParityStatus::MissingAlorOracleStream
        );
        assert_eq!(
            report
                .strategy_input_publication
                .finam_derived_m10_published_as_model_bar_count,
            0
        );
        assert_eq!(
            report
                .strategy_input_publication
                .candidate_bars_rejected_before_strategy_count,
            1
        );
    }
}
