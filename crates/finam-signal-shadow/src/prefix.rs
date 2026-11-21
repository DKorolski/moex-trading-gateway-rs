//! Pinned, single-session NEW_SEGMENT reconstruction, never live evidence.
//! The binding is a trusted local input, not something inferred from the prefix.
//! Re-admission checks retained source bytes; it does not authenticate a provider
//! or prove that an operator-supplied QUOTES inventory omitted no frames.
use crate::{
    feedback::Request,
    input::ClosedInputAdapter,
    model::ShadowModel,
    observer::{Decision, Observer, Step},
    quote::{QuoteInput, QUOTE_DEDUP_CAPACITY},
    segment::{ObservationMode, SeededSegment},
    ValidatedConfig,
};
use broker_finam::sparse_m10::{
    AdmittedClosedM1Snapshot, ClosedM1RequestPlan, ClosedM1SnapshotEvidenceV1,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::{
    path::Path,
    time::{Duration, Instant},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrefixBinding {
    pub schema_version: u32,
    /// Full NEW segment config fingerprint (including predecessor attribution).
    pub config_sha256: String,
    pub prefix_sha256: String,
    /// External pinned inventory identity; the collector does not yet emit a
    /// complete restart inventory. This field alone asserts no completeness.
    pub prior_output_inventory_sha256: String,
    pub previous_segment_id: String,
    pub quote_subscription_key: String,
    pub event_count: usize,
    pub next_label_utc_ms: i64,
    pub frontier_utc_ms: i64,
    /// Cooperative deadline, checked around every admission/callback/durable write.
    /// Individual bounded synchronous calls are not forcibly preempted.
    pub max_elapsed_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PrefixEvent {
    Snapshot {
        evidence: ClosedM1SnapshotEvidenceV1,
        result_at_ms: i64,
    },
    Quotes {
        raw_frame: String,
        received_at_ms: i64,
        applied_at_ms: i64,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrefixError {
    Binding,
    Bounds,
    Clock,
    Coverage,
    Admission,
    Model,
    Evidence,
}

/// No public constructor/Deserialize and no OfflineReplay conversion. Only a
/// complete, bounded source-admitted reconstruction can create this capability.
pub struct ReconstructedPrefix {
    pub(crate) config: ValidatedConfig,
    pub(crate) model: ShadowModel,
    pub(crate) observer: Observer,
    pub(crate) adapter: ClosedInputAdapter,
    binding: PrefixBinding,
}

impl ReconstructedPrefix {
    pub fn binding(&self) -> &PrefixBinding {
        &self.binding
    }
    pub fn binding_sha256(&self) -> String {
        crate::seed::digest(&serde_json::to_vec(&self.binding).expect("serializable binding"))
    }
    pub fn config(&self) -> &ValidatedConfig {
        &self.config
    }
    pub fn next_label_utc_ms(&self) -> i64 {
        self.binding.next_label_utc_ms
    }
    pub fn frontier_utc_ms(&self) -> i64 {
        self.binding.frontier_utc_ms
    }
    pub fn observation(&self) -> serde_json::Value {
        self.model.observation()
    }
    pub fn pending(&self) -> Option<&Request> {
        self.model.pending()
    }
    pub fn position_lots(&self) -> i64 {
        self.model.position_lots()
    }

    /// File-only diagnostic completion. Pending/nonflat is never reported flat.
    pub fn finish(mut self) -> Result<bool, PrefixError> {
        let flat = self.model.pending().is_none() && self.model.position_lots() == 0;
        let reason = if self.model.pending().is_some() {
            "INCOMPLETE_prefix_pending"
        } else if !flat {
            "INCOMPLETE_prefix_nonflat"
        } else {
            "prefix_reconstructed_not_live_readiness"
        };
        self.observer
            .finish(reason)
            .map_err(|_| PrefixError::Evidence)?;
        Ok(flat)
    }
}

/// Reconstruct from the original session-boundary seed. Requires a *new* segment
/// linked to its predecessor; no persisted model state is trusted or deserialized.
pub fn reconstruct(
    segment: SeededSegment,
    binding: PrefixBinding,
    bytes: &[u8],
    output: &Path,
) -> Result<ReconstructedPrefix, PrefixError> {
    let began = Instant::now();
    let wall = Utc::now().timestamp_millis();
    let header = segment.seed_observation();
    let (config, mut model) = segment.into_replay_parts();
    // Old prefixes contain quote-derived fills. They cannot continue a new
    // candle-only simulation; use a new session-boundary seed/segment.
    if config.candle_only() {
        return Err(PrefixError::Binding);
    }
    let c = config.config();
    if bytes.len() as u64 > c.max_input_bytes.min(16 * 1024 * 1024) {
        return Err(PrefixError::Bounds);
    }
    if binding.schema_version != 1
        || binding.config_sha256 != config.fingerprint()
        || c.previous_segment_id.as_deref() != Some(binding.previous_segment_id.as_str())
        || binding.previous_segment_id == c.segment_id
        || binding.previous_segment_id.is_empty()
        || binding.quote_subscription_key.is_empty()
        || binding.quote_subscription_key.len() > 128
        || binding.prior_output_inventory_sha256.len() != 64
        || !binding
            .prior_output_inventory_sha256
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        || crate::seed::digest(bytes) != binding.prefix_sha256
    {
        return Err(PrefixError::Binding);
    }
    if binding.event_count == 0
        || binding.event_count > 4096
        || binding.max_elapsed_ms == 0
        || binding.max_elapsed_ms > 10_000
    {
        return Err(PrefixError::Bounds);
    }
    // Intraday only. No implicit holiday calendar or overnight rollover.
    let end = c
        .observation_start_utc_ms
        .checked_add(100 * 600_000)
        .ok_or(PrefixError::Clock)?;
    if binding.next_label_utc_ms <= c.observation_start_utc_ms
        || binding.next_label_utc_ms > end
        || (binding.next_label_utc_ms - c.observation_start_utc_ms) % 600_000 != 0
        || binding.frontier_utc_ms < binding.next_label_utc_ms
        || binding.frontier_utc_ms >= binding.next_label_utc_ms + 600_000
        || binding.frontier_utc_ms >= c.run_deadline_utc_ms
        || binding.frontier_utc_ms > wall
    {
        return Err(PrefixError::Clock);
    }
    let events: Vec<PrefixEvent> =
        serde_json::from_slice(bytes).map_err(|_| PrefixError::Admission)?;
    if events.len() != binding.event_count {
        return Err(PrefixError::Bounds);
    }
    let budget = Duration::from_millis(binding.max_elapsed_ms);
    let binding_sha256 =
        crate::seed::digest(&serde_json::to_vec(&binding).map_err(|_| PrefixError::Binding)?);
    let check = || {
        if began.elapsed() > budget {
            return Err(PrefixError::Bounds);
        }
        if Utc::now().timestamp_millis() < wall {
            return Err(PrefixError::Clock);
        }
        Ok(())
    };
    check()?;
    let instrument = broker_core::InstrumentId {
        symbol: "IMOEXF".into(),
        venue_symbol: Some(c.instrument.clone()),
        exchange: broker_core::Exchange::Moex,
        market: broker_core::Market::Futures,
    };
    let mut adapter = ClosedInputAdapter::new(
        instrument,
        c.max_cache_bars
            .try_into()
            .map_err(|_| PrefixError::Bounds)?,
    )
    .map_err(|_| PrefixError::Bounds)?;
    let mut quotes = QuoteInput::new(c.max_quote_age_ms, QUOTE_DEDUP_CAPACITY)
        .map_err(|_| PrefixError::Bounds)?;
    let mut observer = Observer::create(output, header, c.max_output_bytes, 1_048_576)
        .map_err(|_| PrefixError::Evidence)?;
    model.select_backfill();
    let result = (|| {
        observer.record_run_plan(&serde_json::json!({"mode":ObservationMode::BackfillReplay,"prefix_binding":binding,"binding_sha256":binding_sha256}))
            .map_err(|_| PrefixError::Evidence)?;
        let mut next = c.observation_start_utc_ms;
        let mut clock = next;
        let mut last_rest_received = None;
        for event in events {
            check()?;
            let at = match &event {
                PrefixEvent::Snapshot { result_at_ms, .. } => *result_at_ms,
                PrefixEvent::Quotes { applied_at_ms, .. } => *applied_at_ms,
            };
            if at < clock || at > binding.frontier_utc_ms || at > wall {
                return Err(PrefixError::Clock);
            }
            match event {
                PrefixEvent::Snapshot {
                    evidence,
                    result_at_ms,
                } => {
                    let start = DateTime::from_timestamp_millis(next).ok_or(PrefixError::Clock)?;
                    let end = start + chrono::Duration::minutes(10);
                    if next >= binding.next_label_utc_ms
                        || evidence.start != start
                        || evidence.end != end
                    {
                        return Err(PrefixError::Coverage);
                    }
                    let plan = ClosedM1RequestPlan::new(
                        start,
                        end,
                        evidence.parts.iter().map(|p| p.end).collect(),
                    )
                    .map_err(|_| PrefixError::Admission)?;
                    let received = evidence
                        .parts
                        .last()
                        .ok_or(PrefixError::Admission)?
                        .received_at
                        .timestamp_millis();
                    // REST and WS receive independently. Only effects share
                    // the global clock; a queued response may predate it.
                    if received < c.observation_start_utc_ms
                        || received > result_at_ms
                        || last_rest_received.is_some_and(|last| received < last)
                    {
                        return Err(PrefixError::Clock);
                    }
                    last_rest_received = Some(received);
                    // Historical admission time is pinned and <= actual current wall.
                    // Outputs remain BACKFILL regardless of historical latency.
                    let snapshot = AdmittedClosedM1Snapshot::admit(
                        evidence,
                        &plan,
                        DateTime::from_timestamp_millis(result_at_ms).ok_or(PrefixError::Clock)?,
                    )
                    .map_err(|_| PrefixError::Admission)?;
                    let mut inputs = adapter
                        .prepare(&snapshot, &[start])
                        .map_err(|_| PrefixError::Admission)?;
                    if inputs.len() != 1 {
                        return Err(PrefixError::Coverage);
                    }
                    let input = inputs.pop().unwrap();
                    observer
                        .record_source(&snapshot)
                        .map_err(|_| PrefixError::Evidence)?;
                    check()?;
                    let before = model.observation();
                    let request = model
                        .on_bar(input.bar.clone(), result_at_ms)
                        .map_err(|_| PrefixError::Model)?;
                    let after = model.observation();
                    let decision = Decision::from_model_observation(
                        request.as_ref().map(|r| r.action),
                        request.as_ref().map_or(0, |r| r.quantity_lots),
                        &after,
                    )
                    .map_err(|_| PrefixError::Model)?;
                    observer
                        .record_step(&Step {
                            mode: ObservationMode::BackfillReplay,
                            instrument: input.bar.instrument.symbol.clone(),
                            model_label_utc_ms: next,
                            nominal_close_utc_ms: next + 600_000,
                            provider_at_ms: None,
                            received_at_ms: received,
                            canonical_ready_at_ms: result_at_ms,
                            model_start_at_ms: result_at_ms,
                            result_at_ms,
                            canonical_input_sha256: crate::input::canonical_digest(&input.bar)
                                .map_err(|_| PrefixError::Admission)?,
                            raw_provenance_sha256: input.snapshot_sha256,
                            before,
                            after,
                            decision,
                        })
                        .map_err(|_| PrefixError::Evidence)?;
                    next += 600_000;
                }
                PrefixEvent::Quotes {
                    raw_frame,
                    received_at_ms,
                    applied_at_ms,
                } => {
                    if raw_frame.len() > 256 * 1024 {
                        return Err(PrefixError::Bounds);
                    }
                    // QuoteInput enforces receive/source order within WS.
                    if received_at_ms < c.observation_start_utc_ms || received_at_ms > applied_at_ms
                    {
                        return Err(PrefixError::Clock);
                    }
                    let envelope: broker_finam::FinamWsEnvelope =
                        serde_json::from_str(&raw_frame).map_err(|_| PrefixError::Admission)?;
                    if envelope.subscription_key.as_deref()
                        != Some(binding.quote_subscription_key.as_str())
                    {
                        return Err(PrefixError::Binding);
                    }
                    let admitted = quotes
                        .receive(
                            &raw_frame,
                            DateTime::from_timestamp_millis(received_at_ms)
                                .ok_or(PrefixError::Clock)?,
                        )
                        .map_err(|_| PrefixError::Admission)?;
                    observer
                        .record_transport(
                            &serde_json::json!({"mode":ObservationMode::BackfillReplay,
                        "event":"prefix_quotes", "raw_frame":raw_frame,"raw_frame_sha256":crate::seed::digest(raw_frame.as_bytes()),
                        "received_at_ms":received_at_ms,"applied_at_ms":applied_at_ms}),
                        )
                        .map_err(|_| PrefixError::Evidence)?;
                    for quote in admitted {
                        check()?;
                        // Flat/no-pending quotes still undergo normalization/dedup.
                        if model.pending().is_some() {
                            if let Some(fill) = model
                                .on_quote(quote, applied_at_ms)
                                .map_err(|_| PrefixError::Model)?
                            {
                                observer
                                    .record_feedback(ObservationMode::BackfillReplay, &fill)
                                    .map_err(|_| PrefixError::Evidence)?;
                            }
                        }
                    }
                }
            }
            clock = at;
            check()?;
        }
        if next != binding.next_label_utc_ms || clock != binding.frontier_utc_ms {
            return Err(PrefixError::Coverage);
        }
        observer.record_transport(&serde_json::json!({"mode":ObservationMode::BackfillReplay,
            "event":"prefix_reconstructed", "next_label_utc_ms":next,"frontier_utc_ms":clock,
            "prefix_binding":binding,
            "binding_sha256":binding_sha256,
            "pending":model.pending(),"position_lots":model.position_lots(),"state":model.observation()}))
            .map_err(|_| PrefixError::Evidence)?;
        check()
    })();
    if let Err(error) = result {
        let _ = observer.finish("INCOMPLETE_prefix_reconstruction_failed");
        return Err(error);
    }
    Ok(ReconstructedPrefix {
        config,
        model,
        observer,
        adapter,
        binding,
    })
}
