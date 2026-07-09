use std::collections::BTreeMap;

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

use crate::bar_aggregation::{
    BarAggregationAction, BarAggregationRejectReason, CanonicalBarAggregator,
};
use crate::event::{Bar, MarketDataSourceKind};
use crate::instrument::{InstrumentId, Price, Quantity};
use crate::PaperSafetyBoundary;

pub const STAGE3_MARKET_DATA_PARITY_SCHEMA_VERSION: u16 = 1;
pub const STAGE3_MARKET_DATA_PARITY_STAGE: &str = "Stage3MarketDataParity";
pub const STAGE3_MARKET_DATA_PARITY_SUBSTAGE_3B: &str = "Stage3B";

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
    pub alor_only_bucket_count: usize,
    pub finam_only_bucket_count: usize,
    pub blocking_diff_count: usize,
    pub diagnostic_diff_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Stage3DiffCounts {
    pub missing_alor_bar: usize,
    pub missing_finam_derived_bar: usize,
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
    pub raw_payload_exported: bool,
    pub status: Stage3MarketDataParityStatus,
    pub comparison_policy: Stage3ComparisonPolicy,
    pub strategy_input_publication: Stage3StrategyInputPublicationCounts,
    pub reconnect_recovery: Stage3ReconnectRecoverySummary,
    pub comparison_summary: Stage3ComparisonSummary,
    pub diff_summary: Stage3DiffSummary,
    pub safety_boundary: Stage3SafetyBoundary,
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

    let zero = Price::ZERO;
    Stage3MarketDataParityReport {
        schema_version: STAGE3_MARKET_DATA_PARITY_SCHEMA_VERSION,
        stage: STAGE3_MARKET_DATA_PARITY_STAGE.to_string(),
        substage: STAGE3_MARKET_DATA_PARITY_SUBSTAGE_3B.to_string(),
        raw_payload_exported: false,
        status,
        comparison_policy: Stage3ComparisonPolicy::strict_exact(),
        strategy_input_publication: Stage3StrategyInputPublicationCounts {
            raw_m1_published_as_model_bar_count: 0,
            finam_derived_m10_published_as_model_bar_count: usize::from(finam_candidate.is_some()),
            alor_native_m10_oracle_bars_seen: usize::from(alor_oracle.is_some()),
            candidate_bars_rejected_before_strategy_count: 0,
        },
        reconnect_recovery: Stage3ReconnectRecoverySummary::not_required(),
        comparison_summary: Stage3ComparisonSummary {
            matched_bucket_count,
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
}
