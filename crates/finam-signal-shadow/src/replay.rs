//! Unified OFFLINE coordinator. All outputs are BACKFILL_REPLAY; no live entry.
use crate::{
    feedback::{Fill, Quote, Request},
    model::ShadowModel,
    observer::{Decision, Observer, Step},
    segment::{ObservationMode, SeededSegment},
    ValidatedConfig,
};
use broker_core::{HybridRuntimeBarEvent, HybridRuntimeBarOrigin};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplayError {
    Closed,
    Input,
    Model,
    Evidence,
}

pub struct OfflineReplay {
    config: ValidatedConfig,
    model: ShadowModel,
    observer: Observer,
    last_bar: Option<HybridRuntimeBarEvent>,
    last_clock_ms: Option<i64>,
    closed: bool,
    steps: u64,
}

impl OfflineReplay {
    pub fn create(segment: SeededSegment, output: &Path) -> Result<Self, ReplayError> {
        let header = segment.seed_observation();
        let (config, mut model) = segment.into_replay_parts();
        model.select_backfill();
        let observer =
            Observer::create(output, header, config.config().max_output_bytes, 1_048_576)
                .map_err(|_| ReplayError::Evidence)?;
        Ok(Self {
            config,
            model,
            observer,
            last_bar: None,
            last_clock_ms: None,
            closed: false,
            steps: 0,
        })
    }

    /// Virtual times are replay inputs, not measured live latency. The source
    /// adapter/comparator must separately prove the hash's underlying evidence.
    pub fn step(
        &mut self,
        bar: HybridRuntimeBarEvent,
        raw_provenance_sha256: &str,
        virtual_result_at_ms: i64,
    ) -> Result<Option<Request>, ReplayError> {
        if self.closed {
            return Err(ReplayError::Closed);
        }
        if self.last_bar.as_ref() == Some(&bar) {
            return Ok(None);
        }
        if bar.origin != HybridRuntimeBarOrigin::Replay
            || raw_provenance_sha256.len() != 64
            || !raw_provenance_sha256
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            || self
                .last_clock_ms
                .is_some_and(|last| virtual_result_at_ms < last)
        {
            return self.stop(ReplayError::Input);
        }
        let Some(label) = bar.close_time_utc.checked_mul(1000) else {
            return self.stop(ReplayError::Input);
        };
        let Some(close) = label.checked_add(600_000) else {
            return self.stop(ReplayError::Input);
        };
        let c = self.config.config();
        if label < c.observation_start_utc_ms
            || virtual_result_at_ms < close
            || virtual_result_at_ms >= c.run_deadline_utc_ms
        {
            return self.stop(ReplayError::Input);
        }
        let encoded = serde_json::to_vec(&bar).map_err(|_| ReplayError::Input)?;
        if encoded.len() as u64 > c.max_input_bytes {
            return self.stop(ReplayError::Input);
        }
        let before = self.model.observation();
        let request = match self.model.on_bar(bar.clone(), virtual_result_at_ms) {
            Ok(r) => r,
            Err(_) => return self.stop(ReplayError::Model),
        };
        let after = self.model.observation();
        let decision = Decision::from_model_observation(
            request.as_ref().map(|r| r.action),
            request.as_ref().map_or(0, |r| r.quantity_lots),
            &after,
        )
        .map_err(|_| ReplayError::Model)?;
        let step = Step {
            mode: ObservationMode::BackfillReplay,
            instrument: bar.instrument.symbol.clone(),
            model_label_utc_ms: label,
            nominal_close_utc_ms: close,
            provider_at_ms: None,
            received_at_ms: virtual_result_at_ms,
            canonical_ready_at_ms: virtual_result_at_ms,
            model_start_at_ms: virtual_result_at_ms,
            result_at_ms: virtual_result_at_ms,
            canonical_input_sha256: crate::input::canonical_digest(&bar)
                .map_err(|_| ReplayError::Input)?,
            raw_provenance_sha256: raw_provenance_sha256.into(),
            before,
            after,
            decision,
        };
        if self.observer.record_step(&step).is_err() {
            return self.stop(ReplayError::Evidence);
        }
        if self.config.candle_only() {
            if let Some(request) = &request {
                let fill = match self.model.on_candle_close(
                    request,
                    virtual_result_at_ms,
                    virtual_result_at_ms,
                    raw_provenance_sha256,
                ) {
                    Ok(fill) => fill,
                    Err(_) => return self.stop(ReplayError::Model),
                };
                if self
                    .observer
                    .record_candle_feedback(
                        ObservationMode::BackfillReplay,
                        &fill,
                        &self.model.observation(),
                    )
                    .is_err()
                {
                    return self.stop(ReplayError::Evidence);
                }
            }
        }
        self.last_bar = Some(bar);
        self.last_clock_ms = Some(virtual_result_at_ms);
        self.steps += 1;
        Ok(request)
    }

    pub fn quote(
        &mut self,
        quote: Quote,
        virtual_applied_at_ms: i64,
    ) -> Result<Option<Fill>, ReplayError> {
        if self.closed {
            return Err(ReplayError::Closed);
        }
        if self.config.candle_only() {
            return self.stop(ReplayError::Input);
        }
        if virtual_applied_at_ms >= self.config.config().run_deadline_utc_ms
            || self
                .last_clock_ms
                .map_or(true, |last| virtual_applied_at_ms < last)
        {
            return self.stop(ReplayError::Input);
        }
        let fill = match self.model.on_quote(quote, virtual_applied_at_ms) {
            Ok(v) => v,
            Err(_) => return self.stop(ReplayError::Model),
        };
        if let Some(fill) = &fill {
            if self
                .observer
                .record_feedback(ObservationMode::BackfillReplay, fill)
                .is_err()
            {
                return self.stop(ReplayError::Evidence);
            }
        }
        self.last_clock_ms = Some(virtual_applied_at_ms);
        Ok(fill)
    }

    pub fn finish(&mut self) -> Result<(), ReplayError> {
        if self.closed {
            return Err(ReplayError::Closed);
        }
        let reason = if self.model.pending().is_some() {
            "INCOMPLETE_virtual_pending"
        } else if self.model.position_lots() != 0 {
            "INCOMPLETE_virtual_nonflat"
        } else {
            "replay_end_flat"
        };
        let result = self
            .observer
            .finish(reason)
            .map_err(|_| ReplayError::Evidence);
        self.closed = true;
        result
    }

    pub fn steps(&self) -> u64 {
        self.steps
    }
    pub fn complete_flat(&self) -> bool {
        self.model.pending().is_none() && self.model.position_lots() == 0
    }
    pub fn closed(&self) -> bool {
        self.closed
    }
    pub fn observation(&self) -> serde_json::Value {
        self.model.observation()
    }
    fn stop<T>(&mut self, error: ReplayError) -> Result<T, ReplayError> {
        self.closed = true;
        Err(error)
    }
}
