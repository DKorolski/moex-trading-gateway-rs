//! Composition entry for a fresh comparison segment, not an operational runner.
use broker_core::{
    BrokerAccountId, HybridRuntimeBarEvent, HybridRuntimeGatewayPhase,
    HybridRuntimePaperExecutionMode, HybridRuntimeStrategyContext, HybridRuntimeTradeMode,
};
use chrono::{Datelike, Timelike};
use serde::{Deserialize, Serialize};

use crate::{
    model::ShadowModel,
    seed::{self, InstrumentMapping, SeedReceipt},
    ValidatedConfig,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SegmentError {
    Binding(seed::SeedError),
    Decode,
    UnsupportedStart,
    Bounds,
    Warmup,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ObservationMode {
    Warmup,
    BackfillReplay,
    LiveObserved,
}

/// Read-only initial observation; no readiness or lifecycle authority.
#[derive(Debug, Serialize)]
pub struct SeedObservation {
    pub mode: ObservationMode,
    pub run_id: String,
    pub segment_id: String,
    pub previous_segment_id: Option<String>,
    pub config_sha256: String,
    pub semantic_config_sha256: String,
    pub seed_sha256: String,
    pub model_sha256: String,
    pub reference_sha256: String,
    pub feedback_policy_sha256: String,
    pub last_history_label_utc: i64,
    pub observation_start_utc_ms: i64,
    pub virtual_position_lots: i64,
    pub live_observation_count: u64,
    pub state: serde_json::Value,
}

pub struct SeededSegment {
    config: ValidatedConfig,
    receipt: SeedReceipt,
    model: ShadowModel,
    last_history_label: i64,
}

impl SeededSegment {
    pub(crate) fn into_replay_parts(self) -> (ValidatedConfig, ShadowModel) {
        (self.config, self.model)
    }
    /// Exact same bytes are bound before deserialization/model construction.
    /// Mid-session NEW_SEGMENT requires a separate prefix replay and is refused
    /// here; this entry supports only explicit baseline07 session-boundary seed.
    pub fn from_inputs(
        config: ValidatedConfig,
        reference: &[u8],
        mapping: &[u8],
        calendar: &[u8],
        seed: &[u8],
    ) -> Result<Self, SegmentError> {
        let receipt = seed::bind_seed(&config, reference, mapping, calendar, seed)
            .map_err(SegmentError::Binding)?;
        let c = config.config();
        let local = chrono::DateTime::from_timestamp_millis(c.observation_start_utc_ms)
            .and_then(|t| t.checked_add_signed(chrono::Duration::hours(3)))
            .ok_or(SegmentError::UnsupportedStart)?;
        if (
            local.hour(),
            local.minute(),
            local.second(),
            local.nanosecond(),
        ) != (7, 0, 0, 0)
            || local.weekday().number_from_monday() > 5
        {
            return Err(SegmentError::UnsupportedStart);
        }
        let quote_age = i64::try_from(c.max_quote_age_ms).map_err(|_| SegmentError::Bounds)?;
        let mapping: InstrumentMapping =
            serde_json::from_slice(mapping).map_err(|_| SegmentError::Decode)?;
        let history: Vec<HybridRuntimeBarEvent> =
            serde_json::from_slice(seed).map_err(|_| SegmentError::Decode)?;
        let dates = c
            .warmup_session_dates
            .iter()
            .map(|date| {
                chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d")
                    .map_err(|_| SegmentError::Decode)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let dates = dates.try_into().map_err(|_| SegmentError::Decode)?;
        let now = c.observation_start_utc_ms / 1000;
        let context = HybridRuntimeStrategyContext {
            // Stable per-run identity; segment linkage is recorded separately.
            strategy_id: c.run_id.clone(),
            request_namespace_account: BrokerAccountId(format!("shadow:{}", c.run_id)),
            instrument: mapping.instrument,
            tick_size: 0.5,
            trade_mode: HybridRuntimeTradeMode::Paper,
            paper_execution_mode: HybridRuntimePaperExecutionMode::LiveOnly,
            allow_live_orders: false,
            gateway_phase: HybridRuntimeGatewayPhase::SyncingHistory,
            position_qty: Some(0.0),
            event_ts_utc: now,
            strategy_now_ts_utc: now,
            last_bar_ts_utc: None,
        };
        // The inner constructor hashes the canonical decoded representation;
        // receipt above retains the exact original input-byte hash as well.
        let canonical = serde_json::to_vec(&history).map_err(|_| SegmentError::Decode)?;
        let model = ShadowModel::baseline07_warmed(
            context,
            &history,
            dates,
            &seed::digest(&canonical),
            quote_age,
        )
        .map_err(|_| SegmentError::Warmup)?;
        if model.model_config_fingerprint() != c.model_sha256 {
            return Err(SegmentError::Warmup);
        }
        let last_history_label = history.last().ok_or(SegmentError::Warmup)?.close_time_utc;
        Ok(Self {
            config,
            receipt,
            model,
            last_history_label,
        })
    }

    pub fn seed_observation(&self) -> SeedObservation {
        let c = self.config.config();
        SeedObservation {
            mode: ObservationMode::Warmup,
            run_id: self.receipt.run_id.clone(),
            segment_id: self.receipt.segment_id.clone(),
            previous_segment_id: c.previous_segment_id.clone(),
            config_sha256: self.receipt.config_sha256.clone(),
            semantic_config_sha256: self.receipt.semantic_config_sha256.clone(),
            seed_sha256: self.receipt.seed_sha256.clone(),
            model_sha256: c.model_sha256.clone(),
            reference_sha256: c.reference_sha256.clone(),
            feedback_policy_sha256: c.feedback_policy_sha256.clone(),
            last_history_label_utc: self.last_history_label,
            observation_start_utc_ms: c.observation_start_utc_ms,
            virtual_position_lots: self.model.position_lots(),
            live_observation_count: 0,
            state: self.model.observation(),
        }
    }
}
