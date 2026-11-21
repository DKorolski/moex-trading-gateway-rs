//! Exact local run/seed binding. The pinned calendar is an external reviewed
//! input, not generated from the received bars. No provider truth is inferred.
use broker_core::{HybridRuntimeBarEvent, HybridRuntimeBarOrigin, InstrumentId};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use strategy_runtime_core::HybridIntradayRuntimeStrategy;

use crate::{feedback, profile, ValidatedConfig};

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstrumentMapping {
    pub instrument: InstrumentId,
    pub tick_size: String,
    pub lot_size: u32,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionInventory {
    pub date: String,
    /// Model labels, not close times. Breaks omitted only by the pinned plan.
    pub required_m10_labels_utc: Vec<i64>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CalendarInventory {
    pub sessions: [SessionInventory; 4],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeedError {
    Bound,
    Identity,
    Decode,
    Calendar,
    Coverage,
    Bar,
}

/// Diagnostic receipt only, not an operational readiness or restore capability.
#[derive(Debug, Serialize)]
pub struct SeedReceipt {
    pub run_id: String,
    pub segment_id: String,
    pub config_sha256: String,
    pub semantic_config_sha256: String,
    pub seed_sha256: String,
    pub reference_sha256: String,
    pub calendar_sha256: String,
    pub input_count: usize,
}

pub fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub fn bind_seed(
    validated: &ValidatedConfig,
    reference_bytes: &[u8],
    mapping_bytes: &[u8],
    calendar_bytes: &[u8],
    seed_bytes: &[u8],
) -> Result<SeedReceipt, SeedError> {
    let c = validated.config();
    // A pinned small reference manifest, not a large ZIP, is supplied here.
    let total = [reference_bytes, mapping_bytes, calendar_bytes, seed_bytes]
        .iter()
        .try_fold(0u64, |n, b| n.checked_add(b.len() as u64))
        .ok_or(SeedError::Bound)?;
    if total > c.max_input_bytes || total > 16 * 1024 * 1024 {
        return Err(SeedError::Bound);
    }
    for (bytes, expected) in [
        (reference_bytes, &c.reference_sha256),
        (mapping_bytes, &c.instrument_mapping_sha256),
        (calendar_bytes, &c.calendar_sha256),
        (seed_bytes, &c.seed_sha256),
    ] {
        if digest(bytes) != *expected {
            return Err(SeedError::Identity);
        }
    }
    let fixed = profile::baseline07_config().map_err(|_| SeedError::Identity)?;
    let model = HybridIntradayRuntimeStrategy::new(fixed);
    if model.stage5c_config_fingerprint() != c.model_sha256
        || (feedback::policy_fingerprint() != c.feedback_policy_sha256 && !validated.candle_only())
    {
        return Err(SeedError::Identity);
    }
    let mapping: InstrumentMapping =
        serde_json::from_slice(mapping_bytes).map_err(|_| SeedError::Decode)?;
    if mapping.instrument.symbol != "IMOEXF"
        || mapping.instrument.venue_symbol.as_deref() != Some(c.instrument.as_str())
        || mapping.instrument.exchange != broker_core::Exchange::Moex
        || mapping.instrument.market != broker_core::Market::Futures
        || mapping.tick_size != "0.5"
        || mapping.lot_size != 1
    {
        return Err(SeedError::Identity);
    }
    let calendar: CalendarInventory =
        serde_json::from_slice(calendar_bytes).map_err(|_| SeedError::Decode)?;
    let bars: Vec<HybridRuntimeBarEvent> =
        serde_json::from_slice(seed_bytes).map_err(|_| SeedError::Decode)?;
    if bars.is_empty() || bars.len() > 4096 || bars.len() as u64 > c.max_cache_bars {
        return Err(SeedError::Bound);
    }
    let mut labels = Vec::new();
    for (session, expected) in calendar.sessions.iter().zip(&c.warmup_session_dates) {
        if session.date != *expected || session.required_m10_labels_utc.is_empty() {
            return Err(SeedError::Calendar);
        }
        let date = chrono::NaiveDate::parse_from_str(expected, "%Y-%m-%d")
            .map_err(|_| SeedError::Calendar)?;
        for &label in &session.required_m10_labels_utc {
            let local = chrono::DateTime::from_timestamp(label, 0)
                .and_then(|t| t.checked_add_signed(chrono::Duration::hours(3)))
                .ok_or(SeedError::Calendar)?;
            if local.date_naive() != date
                || label % 600 != 0
                || labels.last().is_some_and(|&prev| label <= prev)
            {
                return Err(SeedError::Calendar);
            }
            labels.push(label);
            if labels.len() > 4096 {
                return Err(SeedError::Bound);
            }
        }
    }
    if labels.len() != bars.len() {
        return Err(SeedError::Coverage);
    }
    for (bar, &label) in bars.iter().zip(&labels) {
        if bar.close_time_utc != label {
            return Err(SeedError::Coverage);
        }
        let open_ms = label.checked_mul(1000).ok_or(SeedError::Bar)?;
        let close_ms = open_ms.checked_add(600_000).ok_or(SeedError::Bar)?;
        if open_ms < c.warmup_range_start_utc_ms
            || close_ms > c.warmup_range_end_utc_ms
            || close_ms > c.observation_start_utc_ms
            || bar.instrument != mapping.instrument
            || bar.origin != HybridRuntimeBarOrigin::History
            || !bar.is_final
            || bar.timeframe_sec != 600
            || [bar.open, bar.high, bar.low, bar.close, bar.volume]
                .iter()
                .any(|n| !n.is_finite())
            || bar.low <= 0.0
            || bar.high < bar.low
            || bar.open < bar.low
            || bar.open > bar.high
            || bar.close < bar.low
            || bar.close > bar.high
            || bar.volume < 0.0
        {
            return Err(SeedError::Bar);
        }
    }
    if labels[0].checked_mul(1000) != Some(c.warmup_range_start_utc_ms)
        || labels
            .last()
            .and_then(|l| l.checked_mul(1000))
            .and_then(|l| l.checked_add(600_000))
            != Some(c.warmup_range_end_utc_ms)
    {
        return Err(SeedError::Coverage);
    }
    Ok(SeedReceipt {
        run_id: c.run_id.clone(),
        segment_id: c.segment_id.clone(),
        config_sha256: validated.fingerprint().into(),
        semantic_config_sha256: validated.semantic_fingerprint().into(),
        seed_sha256: c.seed_sha256.clone(),
        reference_sha256: c.reference_sha256.clone(),
        calendar_sha256: c.calendar_sha256.clone(),
        input_count: bars.len(),
    })
}
