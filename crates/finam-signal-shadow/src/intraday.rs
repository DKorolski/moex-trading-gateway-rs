//! Fresh candle-only segment reconstructed from session open, NOT state import.
//! Real REST bytes/times are retained. Replay callbacks use a separately declared
//! virtual nominal-close clock; they are never latency/live-readiness evidence.
use crate::{
    input::ClosedInputAdapter,
    model::ShadowModel,
    observer::{Decision, Observer, Step},
    runner::RunPlan,
    segment::{ObservationMode, SeededSegment},
    ValidatedConfig,
};
use broker_finam::sparse_m10::{
    AdmittedClosedM1Snapshot, ClosedM1RequestPlan, ClosedM1SnapshotEvidenceV1,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::{path::Path, time::Instant};

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IntradayRunPlan {
    pub schema_version: u32,
    /// Full ordinary EOD tail beginning exactly at next_label_utc_ms.
    pub plan: RunPlan,
    /// Operator-pinned raw history file, not a caller-supplied model state.
    pub history_sha256: String,
    /// Fixed before collection, not inferred from returned bars/metadata.
    pub history_part_ends: Vec<DateTime<Utc>>,
    pub next_label_utc_ms: i64,
    pub start_not_before_utc_ms: i64,
    pub start_before_utc_ms: i64,
    pub max_reconstruction_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntradayError {
    Binding,
    Window,
    Bounds,
    Admission,
    Coverage,
    Model,
    Evidence,
}

impl IntradayRunPlan {
    pub fn validate(&self, config: &ValidatedConfig) -> Result<(), IntradayError> {
        let c = config.config();
        let span = self
            .next_label_utc_ms
            .checked_sub(c.observation_start_utc_ms)
            .ok_or(IntradayError::Window)?;
        let close = self
            .next_label_utc_ms
            .checked_add(600_000)
            .ok_or(IntradayError::Window)?;
        let session_close = c
            .observation_start_utc_ms
            .checked_add(101 * 600_000)
            .ok_or(IntradayError::Window)?;
        let eod = self.plan.eod_check_at.timestamp_millis();
        if !config.candle_only()
            || c.previous_segment_id.is_some()
            || self.schema_version != 1
            || self.history_sha256.len() != 64
            || !self
                .history_sha256
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(IntradayError::Binding);
        }
        // Same model session, at least one reconstructed and one remaining M10.
        // Session open stays 07:00; it is NOT moved to the connection time.
        if !(600_000..=100 * 600_000).contains(&span)
            || span % 600_000 != 0
            || self.start_not_before_utc_ms < self.next_label_utc_ms
            || self.start_not_before_utc_ms >= self.start_before_utc_ms
            || self.start_before_utc_ms > close
            || self.start_before_utc_ms > c.run_deadline_utc_ms
            || eod <= session_close
            || eod
                >= session_close
                    .checked_add(600_000)
                    .ok_or(IntradayError::Window)?
        {
            return Err(IntradayError::Window);
        }
        if self.max_reconstruction_ms == 0 || self.max_reconstruction_ms > 10_000 {
            return Err(IntradayError::Bounds);
        }
        self.history_request(config)?;
        self.plan
            .validate_from(config, self.next_label_utc_ms)
            .map_err(|_| IntradayError::Coverage)
    }

    pub fn validate_now(&self, config: &ValidatedConfig, now: i64) -> Result<(), IntradayError> {
        self.validate(config)?;
        if now < self.start_not_before_utc_ms || now >= self.start_before_utc_ms {
            return Err(IntradayError::Window);
        }
        Ok(())
    }

    pub fn history_request(
        &self,
        config: &ValidatedConfig,
    ) -> Result<ClosedM1RequestPlan, IntradayError> {
        ClosedM1RequestPlan::new(
            time(config.config().observation_start_utc_ms)?,
            time(self.next_label_utc_ms)?,
            self.history_part_ends.clone(),
        )
        .map_err(|_| IntradayError::Coverage)
    }

    pub(crate) fn digest(&self) -> String {
        crate::seed::digest(&serde_json::to_vec(self).expect("serializable intraday plan"))
    }

    pub(crate) fn validate_reconstructed(
        &self,
        day: &ReconstructedCandleDay,
    ) -> Result<(), IntradayError> {
        self.validate(&day.config)?;
        if self.digest() != day.plan_sha256 || self.next_label_utc_ms != day.next_label_ms {
            return Err(IntradayError::Binding);
        }
        Ok(())
    }
}

fn time(ms: i64) -> Result<DateTime<Utc>, IntradayError> {
    DateTime::from_timestamp_millis(ms).ok_or(IntradayError::Window)
}

/// No public construction or Deserialize; only complete source admission can
/// grant continuation. Model, virtual book and dedup adapter are moved, not reset.
pub struct ReconstructedCandleDay {
    pub(crate) config: ValidatedConfig,
    pub(crate) model: ShadowModel,
    pub(crate) observer: Observer,
    pub(crate) adapter: ClosedInputAdapter,
    pub(crate) next_label_ms: i64,
    pub(crate) plan_sha256: String,
}

impl ReconstructedCandleDay {
    pub fn observation(&self) -> serde_json::Value {
        self.model.observation()
    }
    pub fn position_lots(&self) -> i64 {
        self.model.position_lots()
    }
    pub fn next_label_utc_ms(&self) -> i64 {
        self.next_label_ms
    }
}

pub fn reconstruct(
    segment: SeededSegment,
    plan: &IntradayRunPlan,
    bytes: &[u8],
    output: &Path,
) -> Result<ReconstructedCandleDay, IntradayError> {
    reconstruct_with_clock(segment, plan, bytes, output, || {
        Utc::now().timestamp_millis()
    })
}

// Clock injection is private and only used by deterministic file-only tests.
fn reconstruct_with_clock(
    segment: SeededSegment,
    plan: &IntradayRunPlan,
    bytes: &[u8],
    output: &Path,
    mut clock: impl FnMut() -> i64,
) -> Result<ReconstructedCandleDay, IntradayError> {
    let began = Instant::now();
    let wall = clock();
    let header = segment.seed_observation();
    let (config, mut model) = segment.into_replay_parts();
    plan.validate_now(&config, wall)?;
    let c = config.config();
    if bytes.len() as u64 > c.max_input_bytes.min(16 * 1024 * 1024) {
        return Err(IntradayError::Bounds);
    }
    if crate::seed::digest(bytes) != plan.history_sha256 {
        return Err(IntradayError::Binding);
    }
    let request = plan.history_request(&config)?;
    request
        .preflight(time(wall)?)
        .map_err(|_| IntradayError::Admission)?;
    let evidence: ClosedM1SnapshotEvidenceV1 =
        serde_json::from_slice(bytes).map_err(|_| IntradayError::Admission)?;
    // Admission uses REAL current/receive times, not virtual callback time.
    // Bounds, completeness, raw hashes, timestamps, sparse OHLCV and mapping
    // remain the accepted production admission; no invented missing minutes.
    let snapshot = AdmittedClosedM1Snapshot::admit(evidence, &request, time(wall)?)
        .map_err(|_| IntradayError::Admission)?;
    let received = snapshot
        .evidence()
        .parts
        .iter()
        .map(|p| p.received_at.timestamp_millis())
        .max()
        .ok_or(IntradayError::Admission)?;
    let mut adapter = ClosedInputAdapter::new(
        broker_core::InstrumentId {
            symbol: "IMOEXF".into(),
            venue_symbol: Some(c.instrument.clone()),
            exchange: broker_core::Exchange::Moex,
            market: broker_core::Market::Futures,
        },
        c.max_cache_bars as usize,
    )
    .map_err(|_| IntradayError::Bounds)?;
    let labels: Vec<_> = (c.observation_start_utc_ms..plan.next_label_utc_ms)
        .step_by(600_000)
        .map(time)
        .collect::<Result<_, _>>()?;
    let inputs = adapter
        .prepare(&snapshot, &labels)
        .map_err(|_| IntradayError::Coverage)?;
    if inputs.len() != labels.len() {
        return Err(IntradayError::Coverage);
    }
    // Complete input is validated before the first model callback/output.
    let mut observer = Observer::create(output, header, c.max_output_bytes, 1_048_576)
        .map_err(|_| IntradayError::Evidence)?;
    let plan_sha256 = plan.digest();
    let mut previous_wall = wall;
    let mut check = || {
        let now = clock();
        plan.validate_now(&config, now)?;
        if now < previous_wall
            || (i128::from(now) - i128::from(wall) - began.elapsed().as_millis() as i128)
                .unsigned_abs()
                > u128::from(c.clock_uncertainty_ms)
        {
            return Err(IntradayError::Window);
        }
        previous_wall = now;
        if began.elapsed().as_millis() > u128::from(plan.max_reconstruction_ms) {
            return Err(IntradayError::Bounds);
        }
        Ok(now)
    };
    model.select_backfill();
    let result = (|| {
        check()?;
        observer
            .record_run_plan(&serde_json::json!({
                "mode":ObservationMode::BackfillReplay,"intraday_plan_sha256":plan_sha256,
                "history_sha256":plan.history_sha256,"reconstructed_m10":labels.len(),
                "historical_step_and_fill_clock":"VIRTUAL_NOMINAL_CLOSE",
                "actual_request_receive_times":"unchanged in source_snapshot evidence",
                "history_proves_live_readiness":false,"broker_execution":false,
            }))
            .map_err(|_| IntradayError::Evidence)?;
        observer
            .record_source(&snapshot)
            .map_err(|_| IntradayError::Evidence)?;
        for input in inputs {
            let replayed_at = check()?;
            let label = input.bar.close_time_utc * 1000;
            let close = label + 600_000;
            let before = model.observation();
            let intent = model
                .on_bar(input.bar.clone(), close)
                .map_err(|_| IntradayError::Model)?;
            let after = model.observation();
            let decision = Decision::from_model_observation(
                intent.as_ref().map(|r| r.action),
                intent.as_ref().map_or(0, |r| r.quantity_lots),
                &after,
            )
            .map_err(|_| IntradayError::Model)?;
            observer
                .record_transport(&serde_json::json!({
                    "event":"intraday_replay_clock","mode":ObservationMode::BackfillReplay,
                    "model_label_utc_ms":label,"virtual_callback_at_ms":close,
                    "snapshot_received_at_ms":received,"replayed_at_ms":replayed_at,
                    "snapshot_sha256":input.snapshot_sha256,"minute_bitmap":input.minute_bitmap,
                }))
                .map_err(|_| IntradayError::Evidence)?;
            observer
                .record_step(&Step {
                    mode: ObservationMode::BackfillReplay,
                    instrument: input.bar.instrument.symbol.clone(),
                    model_label_utc_ms: label,
                    nominal_close_utc_ms: close,
                    provider_at_ms: None,
                    received_at_ms: close,
                    canonical_ready_at_ms: close,
                    model_start_at_ms: close,
                    result_at_ms: close,
                    canonical_input_sha256: crate::input::canonical_digest(&input.bar)
                        .map_err(|_| IntradayError::Admission)?,
                    raw_provenance_sha256: input.snapshot_sha256.clone(),
                    before,
                    after,
                    decision,
                })
                .map_err(|_| IntradayError::Evidence)?;
            // Durable decision precedes the same accepted close-fill callback.
            if let Some(intent) = intent {
                let fill = model
                    .on_candle_close(&intent, close, close, &input.snapshot_sha256)
                    .map_err(|_| IntradayError::Model)?;
                observer
                    .record_candle_feedback(
                        ObservationMode::BackfillReplay,
                        &fill,
                        &model.observation(),
                    )
                    .map_err(|_| IntradayError::Evidence)?;
            }
            check()?;
        }
        if model.pending().is_some() {
            return Err(IntradayError::Model);
        }
        observer.record_transport(&serde_json::json!({
            "event":"intraday_reconstructed","mode":ObservationMode::BackfillReplay,
            "at_ms":check()?,"intraday_plan_sha256":plan_sha256,"history_sha256":plan.history_sha256,
            "last_processed_label_utc_ms":plan.next_label_utc_ms-600_000,
            "next_label_utc_ms":plan.next_label_utc_ms,"position_lots":model.position_lots(),
            "state":model.observation(),"pending":false,"live_observations":0,
        })).map_err(|_| IntradayError::Evidence)?;
        check()?;
        Ok(())
    })();
    if let Err(error) = result {
        let _ = observer.finish("INCOMPLETE_intraday_reconstruction_failed");
        return Err(error);
    }
    Ok(ReconstructedCandleDay {
        config,
        model,
        observer,
        adapter,
        next_label_ms: plan.next_label_utc_ms,
        plan_sha256,
    })
}

#[cfg(test)]
#[path = "intraday_tests.rs"]
pub(crate) mod tests;
