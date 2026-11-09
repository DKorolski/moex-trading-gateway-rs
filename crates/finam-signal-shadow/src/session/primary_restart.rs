//! Deterministic source replay into a NEW segment, then ordinary guarded entry.
use super::*;
use crate::primary::{
    restart::{CommittedPrefix, RestartError, RestartSource},
    PrimaryProcessor, WsPrimaryConfig,
};
use std::time::Instant;
impl ObservedSession {
    pub fn resume_primary(
        segment: SeededSegment,
        prefix: CommittedPrefix,
        config: &WsPrimaryConfig,
        output: &Path,
        stop: StopHandle,
    ) -> Result<(Self, PrimaryProcessor), RestartError> {
        let started = Instant::now();
        let (wall, mono) = crate::timing::sample_clock();
        let header = segment.seed_observation();
        let (validated, mut model) = segment.into_replay_parts();
        if validated.fingerprint() != prefix.binding.config_sha256
            || config.fingerprint().map_err(|_| RestartError::Binding)? != validated.fingerprint()
            || header.state != prefix.header.state
            || header.last_history_label_utc != prefix.header.last_history_label_utc
            || model.pending().is_some()
            || model.position_lots() != 0
        {
            return Err(RestartError::Binding);
        }
        let check = || {
            let (now, current_mono) = crate::timing::sample_clock();
            if stop.is_stopped() {
                return Err(RestartError::Stopped);
            }
            let elapsed = current_mono
                .checked_duration_since(mono)
                .ok_or(RestartError::Clock)?
                .as_millis();
            if now < wall
                || now < prefix.admitted_at_ms
                || now >= validated.config().run_deadline_utc_ms
                || (i128::from(now) - i128::from(wall) - elapsed as i128).unsigned_abs()
                    > u128::from(validated.config().clock_uncertainty_ms)
            {
                return Err(RestartError::Clock);
            }
            if started.elapsed().as_millis() > u128::from(prefix.binding.max_reconstruction_ms) {
                return Err(RestartError::Bounds);
            }
            Ok(now)
        };
        check()?;
        let instrument = broker_core::InstrumentId {
            symbol: "IMOEXF".into(),
            venue_symbol: Some("IMOEXF@RTSX".into()),
            exchange: broker_core::Exchange::Moex,
            market: broker_core::Market::Futures,
        };
        let mut processor = PrimaryProcessor::from_config(
            config,
            instrument.clone(),
            prefix.required.clone(),
            1,
            wall,
        )
        .map_err(|_| RestartError::Binding)?;
        let adapter =
            ClosedInputAdapter::new(instrument, validated.config().max_cache_bars as usize)
                .map_err(|_| RestartError::Bounds)?;
        let mut observer =
            Observer::create(output, header, validated.config().max_output_bytes, 1048576)
                .map_err(|_| RestartError::Evidence)?;
        model.select_backfill();
        let result = (|| {
            observer.record_run_plan(&serde_json::json!({"mode":ObservationMode::BackfillReplay,"restart_binding":prefix.binding,
                "virtual_model_clock":"RECORDED_CALLBACK_AND_RESULT","actual_reconstruction_clock":"UTC_MONOTONIC",
                "history_proves_live_readiness":false,"broker_execution":false})).map_err(|_|RestartError::Evidence)?;
            let policy = config
                .primary
                .fingerprint()
                .map_err(|_| RestartError::Binding)?;
            for input in &prefix.inputs {
                let actual = check()?;
                let label = input.bar.close_time_utc * 1000;
                let close = label + 600000;
                if model.observation() != input.step.before {
                    return Err(RestartError::Model);
                }
                let source = RestartSource {
                    config_sha256: validated.fingerprint().into(),
                    policy_sha256: policy.clone(),
                    origin: "RESTART_BACKFILL".into(),
                    previous_journal_sha256: prefix.binding.previous_journal_sha256.clone(),
                    previous_source_sha256: input.source_sha256.clone(),
                    label_ms: label,
                    close_ms: close,
                    canonical_digest: input.step.canonical_input_sha256.clone(),
                    minute_bitmap: input.minute_bitmap,
                    actual_m1: input.actual_m1.clone(),
                    virtual_callback_at_ms: input.step.model_start_at_ms,
                    virtual_result_at_ms: input.step.result_at_ms,
                    virtual_received_at_ms: input.step.received_at_ms,
                    virtual_ready_at_ms: input.step.canonical_ready_at_ms,
                    reconstructed_at_ms: actual,
                };
                let hash = observer
                    .record_restart_source(&source)
                    .map_err(|_| RestartError::Evidence)?;
                let mut bar = input.bar.clone();
                bar.origin = HybridRuntimeBarOrigin::Replay;
                let before = model.observation();
                let intent = model
                    .on_recorded_bar(bar, input.step.model_start_at_ms, input.step.result_at_ms)
                    .map_err(|_| RestartError::Model)?;
                let after = model.observation();
                let decision = Decision::from_model_observation(
                    intent.as_ref().map(|r| r.action),
                    intent.as_ref().map_or(0, |r| r.quantity_lots),
                    &after,
                )
                .map_err(|_| RestartError::Model)?;
                if after != input.step.after || decision != input.step.decision {
                    return Err(RestartError::Model);
                }
                observer
                    .record_step(&Step {
                        mode: ObservationMode::BackfillReplay,
                        instrument: "IMOEXF".into(),
                        model_label_utc_ms: label,
                        nominal_close_utc_ms: close,
                        provider_at_ms: None,
                        received_at_ms: input.step.received_at_ms,
                        canonical_ready_at_ms: input.step.canonical_ready_at_ms,
                        model_start_at_ms: input.step.model_start_at_ms,
                        result_at_ms: input.step.result_at_ms,
                        canonical_input_sha256: input.step.canonical_input_sha256.clone(),
                        raw_provenance_sha256: hash.clone(),
                        before,
                        after,
                        decision,
                    })
                    .map_err(|_| RestartError::Evidence)?;
                if let Some(intent) = intent {
                    check()?;
                    let (old, state) = input.feedback.as_ref().ok_or(RestartError::Model)?;
                    let fill = model
                        .on_candle_close(&intent, old.received_at_ms, old.applied_at_ms, &hash)
                        .map_err(|_| RestartError::Model)?;
                    if (
                        fill.request.sequence,
                        fill.request.action,
                        fill.request.quantity_lots,
                        fill.price_ticks,
                        fill.position_lots,
                        fill.average_price_ticks,
                    ) != (
                        old.request.sequence,
                        old.request.action,
                        old.request.quantity_lots,
                        old.price_ticks,
                        old.position_lots,
                        old.average_price_ticks,
                    ) || model.observation() != *state
                    {
                        return Err(RestartError::Model);
                    }
                    observer
                        .record_candle_feedback(
                            ObservationMode::BackfillReplay,
                            &fill,
                            &model.observation(),
                        )
                        .map_err(|_| RestartError::Evidence)?;
                }
                if model.pending().is_some() {
                    return Err(RestartError::Incomplete);
                }
                let result = check()?;
                observer.record_transport(&serde_json::json!({"event":"restart_step_durable_publication","mode":ObservationMode::BackfillReplay,
                    "model_label_utc_ms":label,"actual_model_result_at_ms":result,"virtual_callback_at_ms":input.step.model_start_at_ms,
                    "history_proves_live_readiness":false})).map_err(|_|RestartError::Evidence)?;
                observer
                    .record_canonical_commit(label, &hash, result, check()?)
                    .map_err(|_| RestartError::Evidence)?;
                check()?;
            }
            processor
                .restore_prefix(&prefix)
                .map_err(|_| RestartError::Binding)?;
            observer.record_transport(&serde_json::json!({"event":"primary_restart_handoff","actual_handoff_at_ms":check()?,
                "previous_journal_sha256":prefix.binding.previous_journal_sha256,"next_label_ms":prefix.next_label_ms(),
                "reconstructed_labels":prefix.committed_labels(),"state":model.observation(),"position_lots":model.position_lots(),
                "transport_ready":false,"history_proves_live_readiness":false})).map_err(|_|RestartError::Evidence)?;
            check()?;
            CallbackFence::new(&validated, stop.clone()).map_err(|_| RestartError::Clock)
        })();
        let fence = match result {
            Ok(f) => f,
            Err(error) => {
                stop.stop();
                let _ = observer.finish("INCOMPLETE_primary_restart");
                return Err(error);
            }
        };
        model.select_observed();
        Ok((
            Self {
                config: validated,
                model,
                adapter,
                fence,
                observer,
                stop,
                next_label_ms: prefix.next_label_ms(),
                closed: false,
                last_quote_ms: None,
                prefix_quote_floor_ms: None,
            },
            processor,
        ))
    }
}

#[cfg(test)]
mod tests;
