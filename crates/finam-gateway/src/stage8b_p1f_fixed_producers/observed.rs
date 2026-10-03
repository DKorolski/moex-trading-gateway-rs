//! Explicit V2 high-water for closed-REST observed M1. Existing V1 files never
//! enter this path. The receipt is retained separately, not copied per candle;
//! restart must supply its independent source binding before state readmission.

use super::*;
use broker_core::observed_m1::ObservedM1Receipt;
use broker_finam::sparse_m10::AdmittedClosedM1Snapshot;
use runtime_durable_service::{Stage8bP1ObservedM10Binding, Stage8bP1fM10FeederRedisV1};

pub(super) const STATE_DOMAIN: &str = "moex.stage8b.p1f.fixed-m10-producer-state.v2";

pub struct Stage8bP1fObservedM10Input<'a> {
    pub phase: Stage8bP1fProducerPhaseV1,
    pub phase_id: String,
    pub nominal_close: DateTime<Utc>,
    pub trusted_now: DateTime<Utc>,
    pub snapshot: &'a AdmittedClosedM1Snapshot,
    pub expected_receipt_sha256: &'a str,
    /// O3 must additionally bind its independently pinned synthetic snapshot.
    /// O4 forbids this field. Neither path accepts live-stream silence as input.
    pub expected_synthetic_snapshot_sha256: Option<&'a str>,
}

/// Non-deserializable checked state. The shared private record layout is reused,
/// but its schema/domain/source generation and canonical wire are explicitly V2.
#[derive(Clone)]
pub struct Stage8bP1fObservedM10State {
    record: Stage8bP1fM10ProducerStateV1,
    binding: Stage8bP1ObservedM10Binding,
}

pub enum Stage8bP1fObservedM10Lineage<'a> {
    First(Stage8bP1fFirstM10AuthorizationV1),
    Resume(&'a Stage8bP1fObservedM10State),
}

pub enum Stage8bP1fObservedM10Outcome {
    Prepared(Stage8bP1fObservedM10State),
    ReplayPrepared(Stage8bP1fObservedM10State),
    IdempotentPublished(Stage8bP1fObservedM10State),
}

/// Bounded in-process handoff, not a serializable authority token. Retain the
/// independently admitted Published inputs; do not reconstruct them from Redis
/// hashes or discover a source by scanning the durable directory.
pub fn observed_published_window(
    predecessor: &Stage8bP1fObservedM10State,
    successor: &Stage8bP1fObservedM10State,
) -> Result<runtime_durable_service::Stage8bP1ObservedPublishedWindow, Stage8bP1fProducerErrorV1> {
    predecessor.validate()?;
    successor.validate()?;
    if predecessor.phase() != Stage8bP1fM10ProducerPhaseV1::Published
        || successor.phase() != Stage8bP1fM10ProducerPhaseV1::Published
        || predecessor.publication_sequence().checked_add(1)
            != Some(successor.publication_sequence())
    {
        return Err(Stage8bP1fProducerErrorV1::InvalidLineage);
    }
    match (
        predecessor.record.producer_phase,
        successor.record.producer_phase,
    ) {
        (a, b) if a == b && predecessor.record.phase_id == successor.record.phase_id => {}
        (Stage8bP1fProducerPhaseV1::O3Synthetic, Stage8bP1fProducerPhaseV1::O4FinamReadOnly)
            if predecessor.record.phase_id != successor.record.phase_id => {}
        _ => return Err(Stage8bP1fProducerErrorV1::InvalidLineage),
    }
    Ok(
        runtime_durable_service::Stage8bP1ObservedPublishedWindow::new(
            predecessor.binding().clone(),
            predecessor.canonical_bytes()?,
            successor.binding().clone(),
            successor.canonical_bytes()?,
        )?,
    )
}

impl Stage8bP1fObservedM10State {
    pub fn phase(&self) -> Stage8bP1fM10ProducerPhaseV1 {
        self.record.phase
    }
    pub fn publication_sequence(&self) -> u64 {
        self.record.publication_sequence()
    }
    pub fn redis_id(&self) -> &str {
        &self.record.canonical_m10_redis_id
    }
    pub fn binding(&self) -> &Stage8bP1ObservedM10Binding {
        &self.binding
    }
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, Stage8bP1fProducerErrorV1> {
        self.record.exact_canonical_m10_bytes()
    }
    fn validate(&self) -> Result<(), Stage8bP1fProducerErrorV1> {
        self.record.validate_with_binding(Some(&self.binding))
    }
    pub fn load(
        path: &Path,
        binding: Stage8bP1ObservedM10Binding,
    ) -> Result<Self, Stage8bP1fProducerErrorV1> {
        let bytes = fs::read(path)?;
        let record = serde_json::from_slice(&bytes)
            .map_err(|_| Stage8bP1fProducerErrorV1::DurableStateConflict)?;
        let state = Self { record, binding };
        state.validate()?;
        if state.retained_bytes()? != bytes {
            return Err(Stage8bP1fProducerErrorV1::DurableStateConflict);
        }
        Ok(state)
    }
    fn retained_bytes(&self) -> Result<Vec<u8>, Stage8bP1fProducerErrorV1> {
        self.validate()?;
        stage8b_p1e_canonical_json(&self.record)
            .map_err(|_| Stage8bP1fProducerErrorV1::DurableStateConflict)
    }
    pub fn persist(&self, path: &Path) -> Result<(), Stage8bP1fProducerErrorV1> {
        // The existing single-writer/root ownership contract still applies.
        // This guard forbids using an explicit V2 API to overwrite a V1 or
        // foreign high-water; it is not a cross-process lock or migration tool.
        match fs::read(path) {
            Ok(bytes) => {
                let previous: Stage8bP1fM10ProducerStateV1 = serde_json::from_slice(&bytes)
                    .map_err(|_| Stage8bP1fProducerErrorV1::DurableStateConflict)?;
                if previous.schema_version != 2
                    || previous.domain != STATE_DOMAIN
                    || previous.source_generation != "2"
                    || previous.operational_identity_sha256
                        != self.record.operational_identity_sha256
                {
                    return Err(Stage8bP1fProducerErrorV1::DurableStateConflict);
                }
                let previous_sequence = previous
                    .publication_sequence
                    .parse::<u64>()
                    .ok()
                    .filter(|s| *s > 0)
                    .ok_or(Stage8bP1fProducerErrorV1::DurableStateConflict)?;
                let next_sequence = self.publication_sequence();
                if next_sequence == previous_sequence {
                    let mut allowed = previous.clone();
                    if previous.phase == Stage8bP1fM10ProducerPhaseV1::Prepared
                        && self.phase() == Stage8bP1fM10ProducerPhaseV1::Published
                    {
                        allowed.phase = Stage8bP1fM10ProducerPhaseV1::Published;
                        allowed.published_redis_id = Some(allowed.canonical_m10_redis_id.clone());
                    }
                    if allowed != self.record {
                        return Err(Stage8bP1fProducerErrorV1::DurableStateConflict);
                    }
                } else if previous_sequence.checked_add(1) != Some(next_sequence)
                    || previous.phase != Stage8bP1fM10ProducerPhaseV1::Published
                    || self.phase() != Stage8bP1fM10ProducerPhaseV1::Prepared
                    || redis_id_millis(self.redis_id())?
                        <= redis_id_millis(&previous.canonical_m10_redis_id)?
                {
                    return Err(Stage8bP1fProducerErrorV1::DurableStateConflict);
                }
            }
            Err(e) if e.kind() == ErrorKind::NotFound => {
                if self.publication_sequence() != 1
                    || self.phase() != Stage8bP1fM10ProducerPhaseV1::Prepared
                {
                    return Err(Stage8bP1fProducerErrorV1::DurableStateConflict);
                }
            }
            Err(e) => return Err(e.into()),
        }
        let bytes = self.retained_bytes()?;
        self.binding.persist_retained_receipt(
            path.parent()
                .ok_or(Stage8bP1fProducerErrorV1::DurableStateConflict)?,
        )?;
        persist_m10_bytes(path, &bytes)?;
        let reread = Self::load(path, self.binding.clone())?;
        if reread.record != self.record {
            return Err(Stage8bP1fProducerErrorV1::DurableStateConflict);
        }
        Ok(())
    }
}

pub fn prepare_stage8b_p1f_observed_m10(
    input: Stage8bP1fObservedM10Input<'_>,
    operational_identity: &str,
    schedule: &Stage8bP1eSchedulePublisherStateV1,
    lineage: Stage8bP1fObservedM10Lineage<'_>,
) -> Result<Stage8bP1fObservedM10Outcome, Stage8bP1fProducerErrorV1> {
    prepare_with_verifier(
        input,
        operational_identity,
        schedule,
        lineage,
        &|state, identity, now| state.verify_fresh_envelope(identity, now),
    )
}

pub(super) fn prepare_with_verifier(
    input: Stage8bP1fObservedM10Input<'_>,
    operational_identity: &str,
    schedule: &Stage8bP1eSchedulePublisherStateV1,
    lineage: Stage8bP1fObservedM10Lineage<'_>,
    verify: &impl Fn(
        &Stage8bP1eSchedulePublisherStateV1,
        &str,
        DateTime<Utc>,
    ) -> Result<Vec<u8>, Stage8bP1eSchedulePublisherError>,
) -> Result<Stage8bP1fObservedM10Outcome, Stage8bP1fProducerErrorV1> {
    if !valid_phase_id(&input.phase_id)
        || schedule.phase() != Stage8bP1eSchedulePublisherPhaseV1::Published
    {
        return Err(Stage8bP1fProducerErrorV1::InvalidInput);
    }
    let max_age = match input.phase {
        Stage8bP1fProducerPhaseV1::O3Synthetic => {
            if input.expected_synthetic_snapshot_sha256 != Some(input.snapshot.sha256()) {
                return Err(Stage8bP1fProducerErrorV1::InvalidInput);
            }
            O3_OBSERVATION_MAX_AGE_MS
        }
        Stage8bP1fProducerPhaseV1::O4FinamReadOnly => {
            if input.expected_synthetic_snapshot_sha256.is_some() {
                return Err(Stage8bP1fProducerErrorV1::InvalidInput);
            }
            O4_OBSERVATION_MAX_AGE_MS
        }
    };
    if nonnegative_age_ms(input.trusted_now, input.snapshot.receipt().received_at())? > max_age
        || nonnegative_age_ms(input.trusted_now, input.nominal_close)? > max_age
    {
        return Err(Stage8bP1fProducerErrorV1::Freshness);
    }
    let candidate = input
        .snapshot
        .candidate_at(input.nominal_close, input.trusted_now)
        .map_err(|_| Stage8bP1fProducerErrorV1::InvalidInput)?;
    let envelope = verify(schedule, operational_identity, input.trusted_now)?;
    validate_schedule_window(
        &envelope,
        operational_identity,
        candidate.bar().open_ts,
        input.nominal_close,
        input.trusted_now,
    )?;
    let source = ObservedM1Receipt::restore(
        input.snapshot.receipt().retained_bytes(),
        input.expected_receipt_sha256,
    )
    .map_err(|_| Stage8bP1fProducerErrorV1::DurableStateConflict)?;
    let binding = Stage8bP1ObservedM10Binding::new(
        operational_identity,
        source,
        input.expected_receipt_sha256,
    )?;
    let bytes = runtime_durable_service::build_stage8b_p1_observed_canonical_m10(
        operational_identity,
        candidate.bar().open_ts.timestamp_millis(),
        binding.source(),
    )?;
    let parsed = binding.parse_exact(&bytes, operational_identity)?;
    let publication_sequence = match lineage {
        Stage8bP1fObservedM10Lineage::First(_) => {
            if input.phase != Stage8bP1fProducerPhaseV1::O3Synthetic {
                return Err(Stage8bP1fProducerErrorV1::InvalidLineage);
            }
            1
        }
        Stage8bP1fObservedM10Lineage::Resume(prior) => {
            prior.validate()?;
            let p = &prior.record;
            if p.operational_identity_sha256 != operational_identity {
                return Err(Stage8bP1fProducerErrorV1::DurableStateConflict);
            }
            match (p.producer_phase, input.phase) {
                (a, b) if a == b && p.phase_id == input.phase_id => {}
                (
                    Stage8bP1fProducerPhaseV1::O3Synthetic,
                    Stage8bP1fProducerPhaseV1::O4FinamReadOnly,
                ) if p.phase_id != input.phase_id => {}
                _ => return Err(Stage8bP1fProducerErrorV1::InvalidLineage),
            }
            let old_close = redis_id_millis(prior.redis_id())?;
            if parsed.close_ts_utc_ms() < old_close {
                return Err(Stage8bP1fProducerErrorV1::StaleCandidate);
            }
            if parsed.close_ts_utc_ms() == old_close {
                if prior.canonical_bytes()? != bytes
                    || p.source_batch_sha256 != input.expected_receipt_sha256
                {
                    return Err(Stage8bP1fProducerErrorV1::DurableStateConflict);
                }
                return Ok(match prior.phase() {
                    Stage8bP1fM10ProducerPhaseV1::Prepared => {
                        Stage8bP1fObservedM10Outcome::ReplayPrepared(prior.clone())
                    }
                    Stage8bP1fM10ProducerPhaseV1::Published => {
                        Stage8bP1fObservedM10Outcome::IdempotentPublished(prior.clone())
                    }
                });
            }
            if prior.phase() == Stage8bP1fM10ProducerPhaseV1::Prepared {
                return Err(Stage8bP1fProducerErrorV1::PreparedPublicationPending);
            }
            // Independent admission of both responses is necessary but does
            // not permit revising already accepted minutes in their overlap.
            // Reject before Prepared persistence or any Redis publication.
            runtime_durable_service::Stage8bP1ObservedM10WindowPair::new(
                prior.binding.clone(),
                binding.clone(),
            )?;
            prior
                .publication_sequence()
                .checked_add(1)
                .ok_or(Stage8bP1fProducerErrorV1::DurableStateConflict)?
        }
    };
    let state = Stage8bP1fObservedM10State {
        binding,
        record: Stage8bP1fM10ProducerStateV1 {
            schema_version: 2,
            domain: STATE_DOMAIN.into(),
            source_generation: "2".into(),
            phase: Stage8bP1fM10ProducerPhaseV1::Prepared,
            producer_phase: input.phase,
            phase_id: input.phase_id,
            publication_sequence: publication_sequence.to_string(),
            operational_identity_sha256: operational_identity.into(),
            schedule_envelope_sha256: sha256_hex(&envelope),
            source_batch_sha256: input.expected_receipt_sha256.into(),
            canonical_m10_redis_id: parsed.redis_id().into(),
            canonical_m10_semantic_id_sha256: parsed.semantic_id_sha256().into(),
            canonical_m10_payload_sha256: parsed.payload_sha256().into(),
            canonical_m10_sha256: sha256_hex(&bytes),
            exact_canonical_m10_hex: encode_lower_hex(&bytes),
            published_redis_id: None,
        },
    };
    state.validate()?;
    Ok(Stage8bP1fObservedM10Outcome::Prepared(state))
}

/// Same Prepared -> exact Redis publication/reread -> Published protocol as V1.
/// On loss of response only the retained bytes may be retried, even when the
/// original source is no longer fresh. No callback, command or XACK is exposed.
pub async fn publish_stage8b_p1f_observed_m10(
    path: &Path,
    mut prepared: Stage8bP1fObservedM10State,
    feeder: &mut Stage8bP1fM10FeederRedisV1,
) -> Result<Stage8bP1fObservedM10State, Stage8bP1fProducerErrorV1> {
    prepared.validate()?;
    if prepared.phase() != Stage8bP1fM10ProducerPhaseV1::Prepared {
        return Err(Stage8bP1fProducerErrorV1::DurableStateConflict);
    }
    let role = match prepared.record.producer_phase {
        Stage8bP1fProducerPhaseV1::O3Synthetic => {
            runtime_durable_service::Stage8bP1fRedisRoleV1::SyntheticM10Feeder
        }
        Stage8bP1fProducerPhaseV1::O4FinamReadOnly => {
            runtime_durable_service::Stage8bP1fRedisRoleV1::FinamBarsFeeder
        }
    };
    if feeder.role() != role {
        return Err(Stage8bP1fProducerErrorV1::DurableStateConflict);
    }
    prepared.persist(path)?;
    let bytes = prepared.canonical_bytes()?;
    let receipt = feeder
        .publish_and_reread_exact_m10(
            prepared.redis_id(),
            &bytes,
            prepared.binding.operational_identity_sha256(),
        )
        .await?;
    if receipt.schema_version != 1
        || receipt.role != role
        || receipt.redis_id != prepared.redis_id()
        || receipt.canonical_bytes_sha256 != sha256_hex(&bytes)
        || !receipt.exact_reread
    {
        return Err(Stage8bP1fProducerErrorV1::DurableStateConflict);
    }
    prepared.record.phase = Stage8bP1fM10ProducerPhaseV1::Published;
    prepared.record.published_redis_id = Some(receipt.redis_id);
    prepared.persist(path)?;
    Ok(prepared)
}
