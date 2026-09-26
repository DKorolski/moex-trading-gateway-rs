//! Fixed Stage 8B-P1F Ic producer composition.
//!
//! This module deliberately adds no provider registry and no operational
//! activation path. O3 synthetic and O4 GET/read-only inputs are separate
//! typed paths. They reuse the accepted P1-e schedule publisher and canonical
//! M10 builder while retaining one crash-safe M10 high-water across O3 -> O4.

use std::{
    fs::{self, File, OpenOptions},
    io::{ErrorKind, Write},
    os::unix::fs::OpenOptionsExt,
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
};

use async_trait::async_trait;
use broker_core::{event::Bar, CanonicalBarAggregator, MarketDataSourceKind};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use strategy_runtime_core::{
    stage8b_p1e_canonical_json, Stage8bP1eScheduleEnvelopeV3, Stage8bP1eScheduleSemanticIdentityV1,
    Stage8bP1eStage4SemanticStateV1,
};

use crate::stage8b_p1e_schedule_publisher::{
    adapt_stage8b_p1e_readonly_schedule, prepare_stage8b_p1e_schedule_publication,
    Stage8bP1eReadonlyScheduleAdapterInputV1, Stage8bP1eSchedulePublisherError,
    Stage8bP1eSchedulePublisherInputV1, Stage8bP1eSchedulePublisherLineage,
    Stage8bP1eSchedulePublisherPhaseV1, Stage8bP1eSchedulePublisherStateV1,
    Stage8bP1eScheduleSigner,
};

const M10_STATE_DOMAIN: &str = "moex.stage8b.p1f.fixed-m10-producer-state.v1";
const O3_FIXTURE_DOMAIN: &str = "moex.stage8b.p1f.o3-synthetic-fixture.v1";
const M1_SEMANTIC_DOMAIN: &str = "moex.stage8b.p1f.exact-m1.v1";
const M1_BATCH_DOMAIN: &str = "moex.stage8b.p1f.exact-m1-batch.v1";
const FIRST_M10_CONFIRMATION: &str = "AUTHORIZE-STAGE8B-P1F-FIRST-M10";
const SCHEDULE_REFRESH_MAX_MS: i64 = 2_000;
const CROSS_SOURCE_SKEW_MAX_MS: i64 = 5_000;
const O3_OBSERVATION_MAX_AGE_MS: i64 = 2_000;
const O4_OBSERVATION_MAX_AGE_MS: i64 = 30_000;
const M10_TIMEFRAME_SECONDS: u32 = 600;
static TEMP_COUNTER: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, thiserror::Error)]
pub enum Stage8bP1fProducerErrorV1 {
    #[error("fixed producer input is invalid")]
    InvalidInput,
    #[error("fixed producer input is stale or has excessive cross-source skew")]
    Freshness,
    #[error("fixed producer phase or retained lineage is invalid")]
    InvalidLineage,
    #[error("fixed producer retained high-water is missing or conflicting")]
    DurableStateConflict,
    #[error("fixed producer candidate is older than retained high-water")]
    StaleCandidate,
    #[error("fixed producer has an unresolved Prepared publication")]
    PreparedPublicationPending,
    #[error("canonical M10 is outside the signed tradable schedule")]
    OutsideSignedSchedule,
    #[error("fixed producer state I/O failed: {0:?}")]
    StateIo(ErrorKind),
    #[error("accepted schedule publisher rejected the input: {0}")]
    Schedule(#[from] Stage8bP1eSchedulePublisherError),
    #[error("accepted canonical M10 contract rejected the input: {0}")]
    CanonicalM10(#[from] runtime_durable_service::Stage8bP1CanonicalM10Error),
    #[error("fixed Redis role rejected publication: {0}")]
    RedisRole(#[from] runtime_durable_service::Stage8bP1fRedisRoleErrorV1),
}

impl From<std::io::Error> for Stage8bP1fProducerErrorV1 {
    fn from(value: std::io::Error) -> Self {
        Self::StateIo(value.kind())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage8bP1fProducerPhaseV1 {
    O3Synthetic,
    O4FinamReadOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage8bP1fM10ProducerPhaseV1 {
    Prepared,
    Published,
}

/// Exact M1 observation accepted by the fixed O3/O4 feeder. The source bytes
/// must be the canonical JSON serialization of one broker-neutral `Bar`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stage8bP1fExactM1ObservationV1 {
    pub exact_bar_json: Vec<u8>,
    pub observed_at_utc: DateTime<Utc>,
}

/// One complete fixed producer input. O3 requires the manifest-pinned public
/// fixture hash. O4 forbids it and accepts only exact FINAM-derived live M1.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stage8bP1fM10BatchV1 {
    pub phase: Stage8bP1fProducerPhaseV1,
    pub phase_id: String,
    pub expected_synthetic_fixture_sha256: Option<String>,
    pub observations: Vec<Stage8bP1fExactM1ObservationV1>,
    pub trusted_now_utc: DateTime<Utc>,
}

/// Explicit one-shot authority for a genuinely empty O3 publisher state. O4
/// can never consume this value, so a missing O3/O4 high-water cannot silently
/// become a new genesis.
pub struct Stage8bP1fFirstM10AuthorizationV1(());

pub fn authorize_stage8b_p1f_first_m10(
    confirmation: &str,
) -> Result<Stage8bP1fFirstM10AuthorizationV1, Stage8bP1fProducerErrorV1> {
    if confirmation != FIRST_M10_CONFIRMATION {
        return Err(Stage8bP1fProducerErrorV1::InvalidLineage);
    }
    Ok(Stage8bP1fFirstM10AuthorizationV1(()))
}

pub enum Stage8bP1fM10ProducerLineageV1<'a> {
    First(Stage8bP1fFirstM10AuthorizationV1),
    Resume(&'a Stage8bP1fM10ProducerStateV1),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stage8bP1fM10ProducerStateV1 {
    schema_version: u16,
    domain: String,
    phase: Stage8bP1fM10ProducerPhaseV1,
    producer_phase: Stage8bP1fProducerPhaseV1,
    phase_id: String,
    source_generation: String,
    publication_sequence: String,
    operational_identity_sha256: String,
    schedule_envelope_sha256: String,
    source_batch_sha256: String,
    canonical_m10_redis_id: String,
    canonical_m10_semantic_id_sha256: String,
    canonical_m10_payload_sha256: String,
    canonical_m10_sha256: String,
    exact_canonical_m10_hex: String,
    published_redis_id: Option<String>,
}

impl Stage8bP1fM10ProducerStateV1 {
    pub const fn phase(&self) -> Stage8bP1fM10ProducerPhaseV1 {
        self.phase
    }

    pub const fn producer_phase(&self) -> Stage8bP1fProducerPhaseV1 {
        self.producer_phase
    }

    pub fn phase_id(&self) -> &str {
        &self.phase_id
    }

    pub fn publication_sequence(&self) -> u64 {
        self.publication_sequence
            .parse()
            .expect("validated P1F M10 publication sequence")
    }

    pub fn canonical_m10_redis_id(&self) -> &str {
        &self.canonical_m10_redis_id
    }

    pub fn canonical_m10_semantic_id_sha256(&self) -> &str {
        &self.canonical_m10_semantic_id_sha256
    }

    pub fn source_batch_sha256(&self) -> &str {
        &self.source_batch_sha256
    }

    pub fn schedule_envelope_sha256(&self) -> &str {
        &self.schedule_envelope_sha256
    }

    pub fn operational_identity_sha256(&self) -> &str {
        &self.operational_identity_sha256
    }

    pub fn exact_canonical_m10_bytes(&self) -> Result<Vec<u8>, Stage8bP1fProducerErrorV1> {
        decode_lower_hex(&self.exact_canonical_m10_hex)
    }

    fn validate(&self) -> Result<(), Stage8bP1fProducerErrorV1> {
        if self.schema_version != 1
            || self.domain != M10_STATE_DOMAIN
            || self.source_generation != "1"
            || !valid_phase_id(&self.phase_id)
            || !valid_nonzero_decimal(&self.publication_sequence)
            || !valid_sha256(&self.operational_identity_sha256)
            || !valid_sha256(&self.schedule_envelope_sha256)
            || !valid_sha256(&self.source_batch_sha256)
            || !valid_sha256(&self.canonical_m10_semantic_id_sha256)
            || !valid_sha256(&self.canonical_m10_payload_sha256)
            || !valid_sha256(&self.canonical_m10_sha256)
            || match self.phase {
                Stage8bP1fM10ProducerPhaseV1::Prepared => self.published_redis_id.is_some(),
                Stage8bP1fM10ProducerPhaseV1::Published => {
                    self.published_redis_id.as_deref() != Some(&self.canonical_m10_redis_id)
                }
            }
        {
            return Err(Stage8bP1fProducerErrorV1::DurableStateConflict);
        }
        let exact = self.exact_canonical_m10_bytes()?;
        if sha256_hex(&exact) != self.canonical_m10_sha256 {
            return Err(Stage8bP1fProducerErrorV1::DurableStateConflict);
        }
        let parsed = runtime_durable_service::parse_stage8b_p1_canonical_m10(
            &exact,
            &self.operational_identity_sha256,
        )?;
        if parsed.redis_id() != self.canonical_m10_redis_id
            || parsed.semantic_id_sha256() != self.canonical_m10_semantic_id_sha256
            || parsed.payload_sha256() != self.canonical_m10_payload_sha256
        {
            return Err(Stage8bP1fProducerErrorV1::DurableStateConflict);
        }
        Ok(())
    }
}

pub enum Stage8bP1fM10PrepareOutcomeV1 {
    Prepared(Stage8bP1fM10ProducerStateV1),
    ReplayPrepared(Stage8bP1fM10ProducerStateV1),
    IdempotentPublished(Stage8bP1fM10ProducerStateV1),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Stage8bP1fO3ScheduleInputV1 {
    pub phase_id: String,
    pub expected_synthetic_fixture_sha256: String,
    pub publisher_input: Stage8bP1eSchedulePublisherInputV1,
    pub trusted_now_utc: DateTime<Utc>,
}

/// O3 production path. The manifest fixture digest is bound to the schedule
/// semantic identity, while dynamic observation timestamps remain subject to
/// the two-second freshness and five-second cross-source rules.
pub fn prepare_stage8b_p1f_o3_synthetic_schedule(
    input: Stage8bP1fO3ScheduleInputV1,
    lineage: Stage8bP1eSchedulePublisherLineage<'_>,
    signer: &impl Stage8bP1eScheduleSigner,
) -> Result<Stage8bP1eSchedulePublisherStateV1, Stage8bP1fProducerErrorV1> {
    prepare_stage8b_p1f_o3_synthetic_schedule_with(
        input,
        lineage,
        signer,
        prepare_stage8b_p1e_schedule_publication,
    )
}

pub fn stage8b_p1f_synthetic_schedule_fixture_sha256(
    phase_id: &str,
    input: &Stage8bP1eSchedulePublisherInputV1,
) -> Result<String, Stage8bP1fProducerErrorV1> {
    if !valid_phase_id(phase_id) {
        return Err(Stage8bP1fProducerErrorV1::InvalidInput);
    }
    o3_schedule_fixture_sha256(phase_id, input)
}

fn prepare_stage8b_p1f_o3_synthetic_schedule_with<S: Stage8bP1eScheduleSigner>(
    input: Stage8bP1fO3ScheduleInputV1,
    lineage: Stage8bP1eSchedulePublisherLineage<'_>,
    signer: &S,
    prepare: impl FnOnce(
        Stage8bP1eSchedulePublisherInputV1,
        Stage8bP1eSchedulePublisherLineage<'_>,
        &S,
    )
        -> Result<Stage8bP1eSchedulePublisherStateV1, Stage8bP1eSchedulePublisherError>,
) -> Result<Stage8bP1eSchedulePublisherStateV1, Stage8bP1fProducerErrorV1> {
    if !valid_phase_id(&input.phase_id) || !valid_sha256(&input.expected_synthetic_fixture_sha256) {
        return Err(Stage8bP1fProducerErrorV1::InvalidInput);
    }
    validate_schedule_freshness(
        &input.publisher_input,
        input.trusted_now_utc,
        O3_OBSERVATION_MAX_AGE_MS,
    )?;
    let fixture = o3_schedule_fixture_sha256(&input.phase_id, &input.publisher_input)?;
    if fixture != input.expected_synthetic_fixture_sha256 {
        return Err(Stage8bP1fProducerErrorV1::InvalidInput);
    }
    prepare(input.publisher_input, lineage, signer).map_err(Into::into)
}

/// O4 production source adapter. It can only continue a retained Published
/// high-water; no first-publication authority is accepted on this path.
pub fn prepare_stage8b_p1f_o4_readonly_schedule(
    input: Stage8bP1eReadonlyScheduleAdapterInputV1,
    retained: &Stage8bP1eSchedulePublisherStateV1,
    signer: &impl Stage8bP1eScheduleSigner,
    trusted_now_utc: DateTime<Utc>,
) -> Result<Stage8bP1eSchedulePublisherStateV1, Stage8bP1fProducerErrorV1> {
    prepare_stage8b_p1f_o4_readonly_schedule_with(
        input,
        retained,
        signer,
        trusted_now_utc,
        prepare_stage8b_p1e_schedule_publication,
    )
}

fn prepare_stage8b_p1f_o4_readonly_schedule_with<S: Stage8bP1eScheduleSigner>(
    input: Stage8bP1eReadonlyScheduleAdapterInputV1,
    retained: &Stage8bP1eSchedulePublisherStateV1,
    signer: &S,
    trusted_now_utc: DateTime<Utc>,
    prepare: impl FnOnce(
        Stage8bP1eSchedulePublisherInputV1,
        Stage8bP1eSchedulePublisherLineage<'_>,
        &S,
    )
        -> Result<Stage8bP1eSchedulePublisherStateV1, Stage8bP1eSchedulePublisherError>,
) -> Result<Stage8bP1eSchedulePublisherStateV1, Stage8bP1fProducerErrorV1> {
    if retained.phase() != Stage8bP1eSchedulePublisherPhaseV1::Published
        || input.published_at_utc < input.source_observed_at_utc
        || nonnegative_age_ms(trusted_now_utc, input.source_observed_at_utc)?
            > O4_OBSERVATION_MAX_AGE_MS
        || absolute_delta_ms(input.stage4_report.checked_ts, input.source_observed_at_utc)
            > CROSS_SOURCE_SKEW_MAX_MS
    {
        return Err(Stage8bP1fProducerErrorV1::Freshness);
    }
    let publisher_input = adapt_stage8b_p1e_readonly_schedule(input)?;
    validate_schedule_freshness(&publisher_input, trusted_now_utc, O4_OBSERVATION_MAX_AGE_MS)?;
    prepare(
        publisher_input,
        Stage8bP1eSchedulePublisherLineage::Resume(retained),
        signer,
    )
    .map_err(Into::into)
}

/// Returns the digest that an O3 phase manifest must pin for the supplied
/// exact public M1 fixture. Observation receipt times are deliberately not
/// part of the public fixture identity.
pub fn stage8b_p1f_synthetic_m10_fixture_sha256(
    phase_id: &str,
    observations: &[Stage8bP1fExactM1ObservationV1],
) -> Result<String, Stage8bP1fProducerErrorV1> {
    if !valid_phase_id(phase_id) || observations.len() != 10 {
        return Err(Stage8bP1fProducerErrorV1::InvalidInput);
    }
    let bars = observations
        .iter()
        .map(|observation| {
            canonical_exact_bar(observation).map(|(_, bytes)| encode_lower_hex(bytes))
        })
        .collect::<Result<Vec<_>, _>>()?;
    #[derive(Serialize)]
    struct Fixture<'a> {
        domain: &'static str,
        phase_id: &'a str,
        exact_bar_json_hex: Vec<String>,
    }
    let bytes = stage8b_p1e_canonical_json(&Fixture {
        domain: O3_FIXTURE_DOMAIN,
        phase_id,
        exact_bar_json_hex: bars,
    })
    .map_err(|_| Stage8bP1fProducerErrorV1::InvalidInput)?;
    Ok(sha256_hex(&bytes))
}

pub fn prepare_stage8b_p1f_m10(
    batch: Stage8bP1fM10BatchV1,
    operational_identity_sha256: &str,
    schedule: &Stage8bP1eSchedulePublisherStateV1,
    lineage: Stage8bP1fM10ProducerLineageV1<'_>,
) -> Result<Stage8bP1fM10PrepareOutcomeV1, Stage8bP1fProducerErrorV1> {
    prepare_stage8b_p1f_m10_with_schedule_verifier(
        batch,
        operational_identity_sha256,
        schedule,
        lineage,
        &|state, identity, trusted_now| state.verify_fresh_envelope(identity, trusted_now),
    )
}

fn prepare_stage8b_p1f_m10_with_schedule_verifier(
    batch: Stage8bP1fM10BatchV1,
    operational_identity_sha256: &str,
    schedule: &Stage8bP1eSchedulePublisherStateV1,
    lineage: Stage8bP1fM10ProducerLineageV1<'_>,
    verify_schedule: &impl Fn(
        &Stage8bP1eSchedulePublisherStateV1,
        &str,
        DateTime<Utc>,
    ) -> Result<Vec<u8>, Stage8bP1eSchedulePublisherError>,
) -> Result<Stage8bP1fM10PrepareOutcomeV1, Stage8bP1fProducerErrorV1> {
    let candidate = build_m10_candidate(
        &batch,
        operational_identity_sha256,
        schedule,
        verify_schedule,
    )?;
    let publication_sequence = match lineage {
        Stage8bP1fM10ProducerLineageV1::First(_) => {
            if batch.phase != Stage8bP1fProducerPhaseV1::O3Synthetic {
                return Err(Stage8bP1fProducerErrorV1::InvalidLineage);
            }
            1
        }
        Stage8bP1fM10ProducerLineageV1::Resume(prior) => {
            prior.validate()?;
            if prior.operational_identity_sha256 != operational_identity_sha256 {
                return Err(Stage8bP1fProducerErrorV1::DurableStateConflict);
            }
            validate_phase_progression(prior, &batch)?;
            let prior_close = redis_id_millis(&prior.canonical_m10_redis_id)?;
            let candidate_close = redis_id_millis(&candidate.redis_id)?;
            if candidate_close < prior_close {
                return Err(Stage8bP1fProducerErrorV1::StaleCandidate);
            }
            if candidate_close == prior_close {
                if prior.canonical_m10_sha256 != candidate.canonical_sha256
                    || prior.canonical_m10_semantic_id_sha256 != candidate.semantic_id_sha256
                    || prior.canonical_m10_payload_sha256 != candidate.payload_sha256
                    || prior.source_batch_sha256 != candidate.source_batch_sha256
                {
                    return Err(Stage8bP1fProducerErrorV1::DurableStateConflict);
                }
                return Ok(match prior.phase {
                    Stage8bP1fM10ProducerPhaseV1::Prepared => {
                        Stage8bP1fM10PrepareOutcomeV1::ReplayPrepared(prior.clone())
                    }
                    Stage8bP1fM10ProducerPhaseV1::Published => {
                        Stage8bP1fM10PrepareOutcomeV1::IdempotentPublished(prior.clone())
                    }
                });
            }
            if prior.phase == Stage8bP1fM10ProducerPhaseV1::Prepared {
                return Err(Stage8bP1fProducerErrorV1::PreparedPublicationPending);
            }
            prior
                .publication_sequence()
                .checked_add(1)
                .ok_or(Stage8bP1fProducerErrorV1::DurableStateConflict)?
        }
    };
    let state = Stage8bP1fM10ProducerStateV1 {
        schema_version: 1,
        domain: M10_STATE_DOMAIN.to_string(),
        phase: Stage8bP1fM10ProducerPhaseV1::Prepared,
        producer_phase: batch.phase,
        phase_id: batch.phase_id,
        source_generation: "1".to_string(),
        publication_sequence: publication_sequence.to_string(),
        operational_identity_sha256: operational_identity_sha256.to_string(),
        schedule_envelope_sha256: candidate.schedule_envelope_sha256,
        source_batch_sha256: candidate.source_batch_sha256,
        canonical_m10_redis_id: candidate.redis_id,
        canonical_m10_semantic_id_sha256: candidate.semantic_id_sha256,
        canonical_m10_payload_sha256: candidate.payload_sha256,
        canonical_m10_sha256: candidate.canonical_sha256,
        exact_canonical_m10_hex: encode_lower_hex(&candidate.canonical_bytes),
        published_redis_id: None,
    };
    state.validate()?;
    Ok(Stage8bP1fM10PrepareOutcomeV1::Prepared(state))
}

/// Completes the local high-water only after the exact deterministic Redis ID
/// and exact retained bytes have been observed. The Redis role adapter itself
/// remains a P1F-Id concern.
pub fn mark_stage8b_p1f_m10_published(
    mut prepared: Stage8bP1fM10ProducerStateV1,
    observed_redis_id: &str,
    exact_retained_bytes: &[u8],
) -> Result<Stage8bP1fM10ProducerStateV1, Stage8bP1fProducerErrorV1> {
    prepared.validate()?;
    if prepared.phase != Stage8bP1fM10ProducerPhaseV1::Prepared
        || observed_redis_id != prepared.canonical_m10_redis_id
        || exact_retained_bytes != prepared.exact_canonical_m10_bytes()?
    {
        return Err(Stage8bP1fProducerErrorV1::DurableStateConflict);
    }
    prepared.phase = Stage8bP1fM10ProducerPhaseV1::Published;
    prepared.published_redis_id = Some(observed_redis_id.to_string());
    prepared.validate()?;
    Ok(prepared)
}

pub fn load_stage8b_p1f_m10_producer_state(
    path: &Path,
) -> Result<Stage8bP1fM10ProducerStateV1, Stage8bP1fProducerErrorV1> {
    let bytes = fs::read(path)?;
    let state: Stage8bP1fM10ProducerStateV1 = serde_json::from_slice(&bytes)
        .map_err(|_| Stage8bP1fProducerErrorV1::DurableStateConflict)?;
    state.validate()?;
    if stage8b_p1e_canonical_json(&state)
        .map_err(|_| Stage8bP1fProducerErrorV1::DurableStateConflict)?
        != bytes
    {
        return Err(Stage8bP1fProducerErrorV1::DurableStateConflict);
    }
    Ok(state)
}

pub fn persist_stage8b_p1f_m10_producer_state(
    path: &Path,
    state: &Stage8bP1fM10ProducerStateV1,
) -> Result<(), Stage8bP1fProducerErrorV1> {
    state.validate()?;
    let bytes = stage8b_p1e_canonical_json(state)
        .map_err(|_| Stage8bP1fProducerErrorV1::DurableStateConflict)?;
    let parent = path
        .parent()
        .ok_or(Stage8bP1fProducerErrorV1::DurableStateConflict)?;
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or(Stage8bP1fProducerErrorV1::DurableStateConflict)?;
    let temporary = parent.join(format!(
        ".{name}.{}.{}.tmp",
        std::process::id(),
        TEMP_COUNTER.fetch_add(1, Ordering::SeqCst)
    ));
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(&temporary)?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    fs::rename(&temporary, path)?;
    File::open(parent)?.sync_all()?;
    let reread = load_stage8b_p1f_m10_producer_state(path)?;
    if &reread != state {
        return Err(Stage8bP1fProducerErrorV1::DurableStateConflict);
    }
    Ok(())
}

/// Closed publication seam used by Id. Implementations must publish the exact
/// retained Prepared bytes and return proof of an exact Redis reread.
#[async_trait]
pub trait Stage8bP1fM10PublicationPortV1 {
    async fn publish_and_reread_exact_m10(
        &mut self,
        redis_id: &str,
        canonical_bytes: &[u8],
        operational_identity_sha256: &str,
    ) -> Result<
        runtime_durable_service::Stage8bP1fM10RedisPublicationReceiptV1,
        Stage8bP1fProducerErrorV1,
    >;
}

#[async_trait]
impl Stage8bP1fM10PublicationPortV1 for runtime_durable_service::Stage8bP1fM10FeederRedisV1 {
    async fn publish_and_reread_exact_m10(
        &mut self,
        redis_id: &str,
        canonical_bytes: &[u8],
        operational_identity_sha256: &str,
    ) -> Result<
        runtime_durable_service::Stage8bP1fM10RedisPublicationReceiptV1,
        Stage8bP1fProducerErrorV1,
    > {
        Ok(
            runtime_durable_service::Stage8bP1fM10FeederRedisV1::publish_and_reread_exact_m10(
                self,
                redis_id,
                canonical_bytes,
                operational_identity_sha256,
            )
            .await?,
        )
    }
}

/// Persists Prepared before the Redis effect, publishes only those retained
/// exact bytes, requires exact-reread proof, and then persists Published.  If
/// the call or response is lost, the durable state remains Prepared and the
/// next process invocation reuses the same bytes and deterministic Redis ID.
pub async fn publish_stage8b_p1f_prepared_m10(
    path: &Path,
    prepared: Stage8bP1fM10ProducerStateV1,
    publisher: &mut impl Stage8bP1fM10PublicationPortV1,
) -> Result<Stage8bP1fM10ProducerStateV1, Stage8bP1fProducerErrorV1> {
    prepared.validate()?;
    if prepared.phase() != Stage8bP1fM10ProducerPhaseV1::Prepared {
        return Err(Stage8bP1fProducerErrorV1::DurableStateConflict);
    }
    persist_stage8b_p1f_m10_producer_state(path, &prepared)?;
    let redis_id = prepared.canonical_m10_redis_id().to_string();
    let exact_bytes = prepared.exact_canonical_m10_bytes()?;
    let receipt = publisher
        .publish_and_reread_exact_m10(
            &redis_id,
            &exact_bytes,
            prepared.operational_identity_sha256(),
        )
        .await?;
    if receipt.schema_version != 1
        || !matches!(
            receipt.role,
            runtime_durable_service::Stage8bP1fRedisRoleV1::SyntheticM10Feeder
                | runtime_durable_service::Stage8bP1fRedisRoleV1::FinamBarsFeeder
        )
        || receipt.redis_id != redis_id
        || receipt.canonical_bytes_sha256 != sha256_hex(&exact_bytes)
        || !receipt.exact_reread
    {
        return Err(Stage8bP1fProducerErrorV1::DurableStateConflict);
    }
    let published = mark_stage8b_p1f_m10_published(prepared, &redis_id, &exact_bytes)?;
    persist_stage8b_p1f_m10_producer_state(path, &published)?;
    Ok(published)
}

struct M10Candidate {
    redis_id: String,
    semantic_id_sha256: String,
    payload_sha256: String,
    canonical_sha256: String,
    source_batch_sha256: String,
    schedule_envelope_sha256: String,
    canonical_bytes: Vec<u8>,
}

fn build_m10_candidate(
    batch: &Stage8bP1fM10BatchV1,
    operational_identity_sha256: &str,
    schedule: &Stage8bP1eSchedulePublisherStateV1,
    verify_schedule: &impl Fn(
        &Stage8bP1eSchedulePublisherStateV1,
        &str,
        DateTime<Utc>,
    ) -> Result<Vec<u8>, Stage8bP1eSchedulePublisherError>,
) -> Result<M10Candidate, Stage8bP1fProducerErrorV1> {
    if !valid_phase_id(&batch.phase_id)
        || !valid_sha256(operational_identity_sha256)
        || batch.observations.len() != 10
        || schedule.phase() != Stage8bP1eSchedulePublisherPhaseV1::Published
    {
        return Err(Stage8bP1fProducerErrorV1::InvalidInput);
    }
    match batch.phase {
        Stage8bP1fProducerPhaseV1::O3Synthetic => {
            let expected = batch
                .expected_synthetic_fixture_sha256
                .as_deref()
                .filter(|value| valid_sha256(value))
                .ok_or(Stage8bP1fProducerErrorV1::InvalidInput)?;
            if stage8b_p1f_synthetic_m10_fixture_sha256(&batch.phase_id, &batch.observations)?
                != expected
            {
                return Err(Stage8bP1fProducerErrorV1::InvalidInput);
            }
        }
        Stage8bP1fProducerPhaseV1::O4FinamReadOnly => {
            if batch.expected_synthetic_fixture_sha256.is_some() {
                return Err(Stage8bP1fProducerErrorV1::InvalidInput);
            }
        }
    }

    let verified_schedule =
        verify_schedule(schedule, operational_identity_sha256, batch.trusted_now_utc)?;
    let schedule_envelope_sha256 = sha256_hex(&verified_schedule);
    let mut earliest = batch.observations[0].observed_at_utc;
    let mut latest = earliest;
    let mut previous_observed_at = None;
    let mut exact_bars = Vec::with_capacity(10);
    for observation in &batch.observations {
        let age = nonnegative_age_ms(batch.trusted_now_utc, observation.observed_at_utc)?;
        if batch.phase == Stage8bP1fProducerPhaseV1::O3Synthetic && age > O3_OBSERVATION_MAX_AGE_MS
        {
            return Err(Stage8bP1fProducerErrorV1::Freshness);
        }
        if previous_observed_at.is_some_and(|previous| observation.observed_at_utc < previous) {
            return Err(Stage8bP1fProducerErrorV1::Freshness);
        }
        previous_observed_at = Some(observation.observed_at_utc);
        earliest = earliest.min(observation.observed_at_utc);
        latest = latest.max(observation.observed_at_utc);
        exact_bars.push(canonical_exact_bar(observation)?);
    }
    match batch.phase {
        Stage8bP1fProducerPhaseV1::O3Synthetic => {
            if (latest - earliest).num_milliseconds() > CROSS_SOURCE_SKEW_MAX_MS {
                return Err(Stage8bP1fProducerErrorV1::Freshness);
            }
        }
        Stage8bP1fProducerPhaseV1::O4FinamReadOnly => {
            if nonnegative_age_ms(batch.trusted_now_utc, latest)? > O4_OBSERVATION_MAX_AGE_MS {
                return Err(Stage8bP1fProducerErrorV1::Freshness);
            }
        }
    }

    let mut aggregator = CanonicalBarAggregator::new(M10_TIMEFRAME_SECONDS);
    let mut emitted = None;
    for (index, (bar, _)) in exact_bars.iter().enumerate() {
        let expected_source = match batch.phase {
            Stage8bP1fProducerPhaseV1::O3Synthetic => MarketDataSourceKind::ReadOnlyPoll,
            Stage8bP1fProducerPhaseV1::O4FinamReadOnly => MarketDataSourceKind::LiveStream,
        };
        if bar.source_kind != expected_source
            || !exact_imoexf_m1(bar)
            || batch.observations[index].observed_at_utc < bar.close_ts
            || batch.trusted_now_utc < bar.close_ts
        {
            return Err(Stage8bP1fProducerErrorV1::InvalidInput);
        }
        match aggregator.observe_final_source_bar(bar.clone()) {
            broker_core::BarAggregationAction::Buffered { buffered_count, .. }
                if index < 9 && buffered_count == index + 1 => {}
            broker_core::BarAggregationAction::Emitted { emitted: bar } if index == 9 => {
                emitted = Some(bar);
            }
            _ => return Err(Stage8bP1fProducerErrorV1::InvalidInput),
        }
    }
    let emitted = emitted.ok_or(Stage8bP1fProducerErrorV1::InvalidInput)?;
    validate_schedule_window(
        &verified_schedule,
        operational_identity_sha256,
        emitted.open_ts,
        emitted.close_ts,
        batch.trusted_now_utc,
    )?;

    let source_m1 = exact_bars
        .iter()
        .map(
            |(bar, bytes)| runtime_durable_service::Stage8bP1CanonicalM10SourceM1 {
                redis_id: format!("{}-0", bar.close_ts.timestamp_millis()),
                semantic_id_sha256: domain_sha256(M1_SEMANTIC_DOMAIN, bytes),
                payload_sha256: sha256_hex(bytes),
                open_ts_utc_ms: bar.open_ts.timestamp_millis(),
                close_ts_utc_ms: bar.close_ts.timestamp_millis(),
            },
        )
        .collect();
    let canonical_bytes = runtime_durable_service::build_stage8b_p1_canonical_m10(
        runtime_durable_service::Stage8bP1CanonicalM10BuildInput {
            operational_identity_sha256: operational_identity_sha256.to_string(),
            open_ts_utc_ms: emitted.open_ts.timestamp_millis(),
            close_ts_utc_ms: emitted.close_ts.timestamp_millis(),
            open: emitted.open.normalize().to_string(),
            high: emitted.high.normalize().to_string(),
            low: emitted.low.normalize().to_string(),
            close: emitted.close.normalize().to_string(),
            volume: emitted.volume.normalize().to_string(),
            source_m1,
        },
    )?;
    let parsed = runtime_durable_service::parse_stage8b_p1_canonical_m10(
        &canonical_bytes,
        operational_identity_sha256,
    )?;
    let source_batch_sha256 =
        framed_sha256(M1_BATCH_DOMAIN, exact_bars.iter().map(|(_, bytes)| *bytes));
    Ok(M10Candidate {
        redis_id: parsed.redis_id().to_string(),
        semantic_id_sha256: parsed.semantic_id_sha256().to_string(),
        payload_sha256: parsed.payload_sha256().to_string(),
        canonical_sha256: sha256_hex(&canonical_bytes),
        source_batch_sha256,
        schedule_envelope_sha256,
        canonical_bytes,
    })
}

fn validate_phase_progression(
    prior: &Stage8bP1fM10ProducerStateV1,
    batch: &Stage8bP1fM10BatchV1,
) -> Result<(), Stage8bP1fProducerErrorV1> {
    match (prior.producer_phase, batch.phase) {
        (left, right) if left == right && prior.phase_id == batch.phase_id => Ok(()),
        (Stage8bP1fProducerPhaseV1::O3Synthetic, Stage8bP1fProducerPhaseV1::O4FinamReadOnly)
            if prior.phase_id != batch.phase_id =>
        {
            Ok(())
        }
        _ => Err(Stage8bP1fProducerErrorV1::InvalidLineage),
    }
}

fn canonical_exact_bar(
    observation: &Stage8bP1fExactM1ObservationV1,
) -> Result<(Bar, &[u8]), Stage8bP1fProducerErrorV1> {
    let bar: Bar = serde_json::from_slice(&observation.exact_bar_json)
        .map_err(|_| Stage8bP1fProducerErrorV1::InvalidInput)?;
    let canonical =
        serde_json::to_vec(&bar).map_err(|_| Stage8bP1fProducerErrorV1::InvalidInput)?;
    if canonical != observation.exact_bar_json {
        return Err(Stage8bP1fProducerErrorV1::InvalidInput);
    }
    Ok((bar, &observation.exact_bar_json))
}

fn exact_imoexf_m1(bar: &Bar) -> bool {
    bar.instrument.symbol == "IMOEXF"
        && bar.instrument.venue_symbol.as_deref() == Some("IMOEXF@RTSX")
        && bar.instrument.exchange == broker_core::Exchange::Moex
        && bar.instrument.market == broker_core::Market::Futures
        && bar.timeframe_sec == 60
        && bar.is_final
        && (bar.close_ts - bar.open_ts).num_seconds() == 60
}

fn validate_schedule_window(
    verified_schedule: &[u8],
    operational_identity_sha256: &str,
    open: DateTime<Utc>,
    close: DateTime<Utc>,
    trusted_now: DateTime<Utc>,
) -> Result<(), Stage8bP1fProducerErrorV1> {
    let envelope: Stage8bP1eScheduleEnvelopeV3 = serde_json::from_slice(verified_schedule)
        .map_err(|_| Stage8bP1fProducerErrorV1::DurableStateConflict)?;
    let published_at = parse_timestamp(&envelope.published_at_utc)?;
    if envelope.operational_identity_sha256 != operational_identity_sha256
        || nonnegative_age_ms(trusted_now, published_at)? > SCHEDULE_REFRESH_MAX_MS
    {
        return Err(Stage8bP1fProducerErrorV1::Freshness);
    }
    let within_open_session = envelope
        .payload
        .normalized_schedule
        .sessions
        .iter()
        .any(|session| {
            if session.session_type
                != strategy_runtime_core::Stage8bP1eScheduleSessionTypeV1::TradableOpen
            {
                return false;
            }
            match (
                parse_timestamp(&session.start_utc),
                parse_timestamp(&session.end_utc),
            ) {
                (Ok(start), Ok(end)) => open >= start && close <= end,
                _ => false,
            }
        });
    if !within_open_session {
        return Err(Stage8bP1fProducerErrorV1::OutsideSignedSchedule);
    }
    Ok(())
}

fn validate_schedule_freshness(
    input: &Stage8bP1eSchedulePublisherInputV1,
    trusted_now: DateTime<Utc>,
    observation_max_age_ms: i64,
) -> Result<(), Stage8bP1fProducerErrorV1> {
    let normalized_observed =
        parse_timestamp(&input.payload.normalized_schedule.source_observed_at_utc)?;
    let normalized_expires =
        parse_timestamp(&input.payload.normalized_schedule.source_expires_at_utc)?;
    let stage4_observed = parse_timestamp(&input.payload.stage4_evidence.source_observed_at_utc)?;
    let stage4_expires = parse_timestamp(&input.payload.stage4_evidence.source_expires_at_utc)?;
    if input.published_at_utc > normalized_expires
        || input.published_at_utc > stage4_expires
        || nonnegative_age_ms(trusted_now, input.published_at_utc)? > SCHEDULE_REFRESH_MAX_MS
        || nonnegative_age_ms(trusted_now, normalized_observed)? > observation_max_age_ms
        || nonnegative_age_ms(trusted_now, stage4_observed)? > observation_max_age_ms
        || absolute_delta_ms(normalized_observed, stage4_observed) > CROSS_SOURCE_SKEW_MAX_MS
    {
        return Err(Stage8bP1fProducerErrorV1::Freshness);
    }
    Ok(())
}

fn o3_schedule_fixture_sha256(
    phase_id: &str,
    input: &Stage8bP1eSchedulePublisherInputV1,
) -> Result<String, Stage8bP1fProducerErrorV1> {
    #[derive(Serialize)]
    struct Fixture<'a> {
        domain: &'static str,
        phase_id: &'a str,
        semantic_identity: Stage8bP1eScheduleSemanticIdentityV1,
    }
    let semantic_identity = Stage8bP1eScheduleSemanticIdentityV1 {
        domain: "moex.stage8b.p1e.schedule-semantic-identity.v1".to_string(),
        instrument: input.payload.instrument.clone(),
        registry: input.payload.registry.clone(),
        schema_version: 1,
        sessions: input.payload.normalized_schedule.sessions.clone(),
        stage4_semantic_state: Stage8bP1eStage4SemanticStateV1 {
            boundary_proof: input.payload.stage4_evidence.boundary_proof.clone(),
            evidence_kind: input.payload.stage4_evidence.evidence_kind,
            schedule_state: input.payload.stage4_evidence.schedule_state,
        },
        timeframe_sec: M10_TIMEFRAME_SECONDS,
        timezone: input.payload.timezone.clone(),
        trading_day: input.payload.trading_day.clone(),
    };
    let bytes = stage8b_p1e_canonical_json(&Fixture {
        domain: O3_FIXTURE_DOMAIN,
        phase_id,
        semantic_identity,
    })
    .map_err(|_| Stage8bP1fProducerErrorV1::InvalidInput)?;
    Ok(sha256_hex(&bytes))
}

fn parse_timestamp(value: &str) -> Result<DateTime<Utc>, Stage8bP1fProducerErrorV1> {
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|_| Stage8bP1fProducerErrorV1::InvalidInput)
}

fn nonnegative_age_ms(
    now: DateTime<Utc>,
    observed: DateTime<Utc>,
) -> Result<i64, Stage8bP1fProducerErrorV1> {
    let age = (now - observed).num_milliseconds();
    if age < 0 {
        return Err(Stage8bP1fProducerErrorV1::Freshness);
    }
    Ok(age)
}

fn absolute_delta_ms(left: DateTime<Utc>, right: DateTime<Utc>) -> i64 {
    (left - right).num_milliseconds().abs()
}

fn redis_id_millis(value: &str) -> Result<i64, Stage8bP1fProducerErrorV1> {
    let (millis, sequence) = value
        .split_once('-')
        .ok_or(Stage8bP1fProducerErrorV1::DurableStateConflict)?;
    if sequence != "0" || millis.starts_with('0') {
        return Err(Stage8bP1fProducerErrorV1::DurableStateConflict);
    }
    millis
        .parse()
        .map_err(|_| Stage8bP1fProducerErrorV1::DurableStateConflict)
}

fn valid_phase_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
}

fn valid_nonzero_decimal(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 20
        && value.as_bytes()[0] != b'0'
        && value.bytes().all(|byte| byte.is_ascii_digit())
        && value.parse::<u64>().is_ok()
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn domain_sha256(domain: &str, payload: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(domain.as_bytes());
    hasher.update(b"\0");
    hasher.update(payload);
    format!("{:x}", hasher.finalize())
}

fn framed_sha256<'a>(domain: &str, values: impl Iterator<Item = &'a [u8]>) -> String {
    let mut hasher = Sha256::new();
    hasher.update(domain.as_bytes());
    hasher.update(b"\0");
    for value in values {
        hasher.update((value.len() as u64).to_be_bytes());
        hasher.update(value);
    }
    format!("{:x}", hasher.finalize())
}

fn encode_lower_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn decode_lower_hex(value: &str) -> Result<Vec<u8>, Stage8bP1fProducerErrorV1> {
    if value.len() % 2 != 0
        || value
            .bytes()
            .any(|byte| !byte.is_ascii_hexdigit() || byte.is_ascii_uppercase())
    {
        return Err(Stage8bP1fProducerErrorV1::DurableStateConflict);
    }
    (0..value.len())
        .step_by(2)
        .map(|index| {
            u8::from_str_radix(&value[index..index + 2], 16)
                .map_err(|_| Stage8bP1fProducerErrorV1::DurableStateConflict)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::{
        net::TcpListener,
        process::{Child, Command, Stdio},
        time::Duration as StdDuration,
    };

    use super::*;
    use crate::stage8b_p1e_schedule_publisher::{
        authorize_stage8b_p1e_first_publication,
        test_prepare_stage8b_p1e_schedule_publication_with_key,
        test_publish_stage8b_p1e_prepared_schedule_with_key,
        tests::{fixture_adapter_input, fixture_input, FixtureSigner, FixtureWriter},
    };
    use broker_core::{Exchange, InstrumentId, Market};
    use chrono::{Duration, TimeZone};
    use rust_decimal::Decimal;

    #[derive(Default)]
    struct FixtureM10PublicationPort {
        fail_next: bool,
        corrupt_receipt: bool,
        calls: Vec<(String, Vec<u8>, String)>,
    }

    #[async_trait]
    impl Stage8bP1fM10PublicationPortV1 for FixtureM10PublicationPort {
        async fn publish_and_reread_exact_m10(
            &mut self,
            redis_id: &str,
            canonical_bytes: &[u8],
            operational_identity_sha256: &str,
        ) -> Result<
            runtime_durable_service::Stage8bP1fM10RedisPublicationReceiptV1,
            Stage8bP1fProducerErrorV1,
        > {
            self.calls.push((
                redis_id.to_string(),
                canonical_bytes.to_vec(),
                operational_identity_sha256.to_string(),
            ));
            if std::mem::take(&mut self.fail_next) {
                return Err(Stage8bP1fProducerErrorV1::DurableStateConflict);
            }
            Ok(
                runtime_durable_service::Stage8bP1fM10RedisPublicationReceiptV1 {
                    schema_version: 1,
                    role: runtime_durable_service::Stage8bP1fRedisRoleV1::SyntheticM10Feeder,
                    redis_id: redis_id.to_string(),
                    canonical_bytes_sha256: if self.corrupt_receipt {
                        "0".repeat(64)
                    } else {
                        sha256_hex(canonical_bytes)
                    },
                    disposition:
                        runtime_durable_service::Stage8bP1RedisM10PublishDisposition::Published,
                    exact_reread: true,
                },
            )
        }
    }

    struct RedisServer {
        child: Child,
        url: String,
    }

    impl RedisServer {
        async fn start() -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let port = listener.local_addr().unwrap().port();
            drop(listener);
            let mut child = Command::new("redis-server")
                .args([
                    "--bind",
                    "127.0.0.1",
                    "--port",
                    &port.to_string(),
                    "--save",
                    "",
                    "--appendonly",
                    "no",
                ])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .expect("redis-server is required for the linked P1F-Id response-loss proof");
            let url = format!("redis://127.0.0.1:{port}/15");
            for _ in 0..100 {
                if let Ok(client) = redis::Client::open(url.as_str()) {
                    if let Ok(mut connection) = redis::aio::ConnectionManager::new(client).await {
                        let pong: redis::RedisResult<String> =
                            redis::cmd("PING").query_async(&mut connection).await;
                        if pong.as_deref() == Ok("PONG") && child.try_wait().unwrap().is_none() {
                            return Self { child, url };
                        }
                    }
                }
                tokio::time::sleep(StdDuration::from_millis(10)).await;
            }
            let _ = child.kill();
            let _ = child.wait();
            panic!("temporary Redis did not start");
        }
    }

    impl Drop for RedisServer {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }

    struct LoseFirstRedisResponsePort {
        feeder: runtime_durable_service::Stage8bP1fM10FeederRedisV1,
        lose_next_response: bool,
    }

    #[async_trait]
    impl Stage8bP1fM10PublicationPortV1 for LoseFirstRedisResponsePort {
        async fn publish_and_reread_exact_m10(
            &mut self,
            redis_id: &str,
            canonical_bytes: &[u8],
            operational_identity_sha256: &str,
        ) -> Result<
            runtime_durable_service::Stage8bP1fM10RedisPublicationReceiptV1,
            Stage8bP1fProducerErrorV1,
        > {
            let receipt = self
                .feeder
                .publish_and_reread_exact_m10(
                    redis_id,
                    canonical_bytes,
                    operational_identity_sha256,
                )
                .await?;
            if std::mem::take(&mut self.lose_next_response) {
                return Err(Stage8bP1fProducerErrorV1::DurableStateConflict);
            }
            Ok(receipt)
        }
    }

    fn timestamp(hour: u32, minute: u32, second: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 9, 14, hour, minute, second)
            .single()
            .unwrap()
    }

    fn state_path(label: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!(
            "stage8b-p1f-ic-{label}-{}-{}",
            std::process::id(),
            TEMP_COUNTER.fetch_add(1, Ordering::SeqCst)
        ));
        fs::create_dir_all(&root).unwrap();
        root.join("state.json")
    }

    fn fixture_prepare<'a>(
        input: Stage8bP1eSchedulePublisherInputV1,
        lineage: Stage8bP1eSchedulePublisherLineage<'a>,
        signer: &FixtureSigner,
    ) -> Result<Stage8bP1eSchedulePublisherStateV1, Stage8bP1eSchedulePublisherError> {
        test_prepare_stage8b_p1e_schedule_publication_with_key(
            input,
            lineage,
            signer,
            signer.public_key_ed25519_hex(),
        )
    }

    fn fixture_prepare_m10(
        signer: &FixtureSigner,
        batch: Stage8bP1fM10BatchV1,
        operational_identity_sha256: &str,
        schedule: &Stage8bP1eSchedulePublisherStateV1,
        lineage: Stage8bP1fM10ProducerLineageV1<'_>,
    ) -> Result<Stage8bP1fM10PrepareOutcomeV1, Stage8bP1fProducerErrorV1> {
        prepare_stage8b_p1f_m10_with_schedule_verifier(
            batch,
            operational_identity_sha256,
            schedule,
            lineage,
            &|state, identity, trusted_now| {
                state.test_verify_fresh_envelope_with_key(
                    identity,
                    trusted_now,
                    signer.public_key_ed25519_hex(),
                )
            },
        )
    }

    async fn o3_published_schedule(
        now: DateTime<Utc>,
        signer: &FixtureSigner,
    ) -> Stage8bP1eSchedulePublisherStateV1 {
        let publisher_input = fixture_input(now, "2026-09-14T15:50:00.000000Z");
        let phase_id = "o3-synthetic-1".to_string();
        let expected_synthetic_fixture_sha256 =
            stage8b_p1f_synthetic_schedule_fixture_sha256(&phase_id, &publisher_input).unwrap();
        let first =
            authorize_stage8b_p1e_first_publication("AUTHORIZE-STAGE8B-P1E-FIRST-PUBLICATION")
                .unwrap();
        let prepared = prepare_stage8b_p1f_o3_synthetic_schedule_with(
            Stage8bP1fO3ScheduleInputV1 {
                phase_id,
                expected_synthetic_fixture_sha256,
                publisher_input,
                trusted_now_utc: now,
            },
            Stage8bP1eSchedulePublisherLineage::First(first),
            signer,
            fixture_prepare,
        )
        .unwrap();
        let path = state_path("schedule-o3");
        let mut writer = FixtureWriter::default();
        test_publish_stage8b_p1e_prepared_schedule_with_key(
            &path,
            prepared,
            &mut writer,
            signer.public_key_ed25519_hex(),
        )
        .await
        .unwrap()
    }

    fn observations(
        bucket_open: DateTime<Utc>,
        observed_at: DateTime<Utc>,
        source_kind: MarketDataSourceKind,
        close_delta: i64,
    ) -> Vec<Stage8bP1fExactM1ObservationV1> {
        (0_i64..10)
            .map(|index| {
                let open_ts = bucket_open + Duration::minutes(index);
                let close = 2_100 + index + if index == 9 { close_delta } else { 0 };
                let bar = Bar {
                    instrument: InstrumentId {
                        symbol: "IMOEXF".to_string(),
                        venue_symbol: Some("IMOEXF@RTSX".to_string()),
                        exchange: Exchange::Moex,
                        market: Market::Futures,
                    },
                    source_kind,
                    timeframe_sec: 60,
                    open_ts,
                    close_ts: open_ts + Duration::minutes(1),
                    open: Decimal::new(2_100 + index, 0),
                    high: Decimal::new(2_102 + index + close_delta.max(0), 0),
                    low: Decimal::new(2_099 + index + close_delta.min(0), 0),
                    close: Decimal::new(close, 0),
                    volume: Decimal::new(100 + index, 0),
                    is_final: true,
                };
                Stage8bP1fExactM1ObservationV1 {
                    exact_bar_json: serde_json::to_vec(&bar).unwrap(),
                    observed_at_utc: observed_at,
                }
            })
            .collect()
    }

    fn streaming_observations(bucket_open: DateTime<Utc>) -> Vec<Stage8bP1fExactM1ObservationV1> {
        let mut values = observations(
            bucket_open,
            bucket_open + Duration::minutes(10),
            MarketDataSourceKind::LiveStream,
            0,
        );
        for (index, observation) in values.iter_mut().enumerate() {
            observation.observed_at_utc = bucket_open
                + Duration::minutes(index as i64 + 1)
                + Duration::seconds(1 + (index as i64 % 2));
        }
        values
    }

    fn mutate_published_schedule(
        schedule: &Stage8bP1eSchedulePublisherStateV1,
        mutation: impl FnOnce(&mut Stage8bP1eScheduleEnvelopeV3),
        rebind_state_hash: bool,
    ) -> Stage8bP1eSchedulePublisherStateV1 {
        let mut envelope: Stage8bP1eScheduleEnvelopeV3 =
            serde_json::from_slice(&schedule.exact_envelope_bytes().unwrap()).unwrap();
        mutation(&mut envelope);
        let exact_envelope = stage8b_p1e_canonical_json(&envelope).unwrap();
        let mut state = serde_json::to_value(schedule).unwrap();
        state["exact_envelope_hex"] = serde_json::Value::String(encode_lower_hex(&exact_envelope));
        if rebind_state_hash {
            state["envelope_sha256"] = serde_json::Value::String(sha256_hex(&exact_envelope));
        }
        serde_json::from_value(state).unwrap()
    }

    fn o3_batch(
        observations: Vec<Stage8bP1fExactM1ObservationV1>,
        now: DateTime<Utc>,
    ) -> Stage8bP1fM10BatchV1 {
        let phase_id = "o3-synthetic-1".to_string();
        let fixture = stage8b_p1f_synthetic_m10_fixture_sha256(&phase_id, &observations).unwrap();
        Stage8bP1fM10BatchV1 {
            phase: Stage8bP1fProducerPhaseV1::O3Synthetic,
            phase_id,
            expected_synthetic_fixture_sha256: Some(fixture),
            observations,
            trusted_now_utc: now,
        }
    }

    fn o4_batch(
        observations: Vec<Stage8bP1fExactM1ObservationV1>,
        now: DateTime<Utc>,
    ) -> Stage8bP1fM10BatchV1 {
        Stage8bP1fM10BatchV1 {
            phase: Stage8bP1fProducerPhaseV1::O4FinamReadOnly,
            phase_id: "o4-finam-1".to_string(),
            expected_synthetic_fixture_sha256: None,
            observations,
            trusted_now_utc: now,
        }
    }

    fn prepared(outcome: Stage8bP1fM10PrepareOutcomeV1) -> Stage8bP1fM10ProducerStateV1 {
        match outcome {
            Stage8bP1fM10PrepareOutcomeV1::Prepared(state) => state,
            _ => panic!("expected new Prepared high-water"),
        }
    }

    fn publish(state: Stage8bP1fM10ProducerStateV1) -> Stage8bP1fM10ProducerStateV1 {
        let id = state.canonical_m10_redis_id().to_string();
        let bytes = state.exact_canonical_m10_bytes().unwrap();
        mark_stage8b_p1f_m10_published(state, &id, &bytes).unwrap()
    }

    #[tokio::test]
    async fn fixed_o3_o4_schedule_uses_one_retained_sequence_and_revision() {
        let signer = FixtureSigner::new(91);
        let o3_now = timestamp(12, 10, 0);
        let o3 = o3_published_schedule(o3_now, &signer).await;
        assert_eq!(o3.publication_sequence(), 1);
        assert_eq!(o3.semantic_revision(), 1);

        let o4_now = timestamp(12, 20, 0);
        let o4 = prepare_stage8b_p1f_o4_readonly_schedule_with(
            fixture_adapter_input(o4_now),
            &o3,
            &signer,
            o4_now,
            fixture_prepare,
        )
        .unwrap();
        assert_eq!(o4.phase(), Stage8bP1eSchedulePublisherPhaseV1::Prepared);
        assert_eq!(o4.publication_sequence(), 2);
        assert_eq!(o4.semantic_revision(), 1);

        let mut stale = fixture_adapter_input(o4_now);
        stale.source_observed_at_utc = o4_now - Duration::seconds(31);
        assert!(matches!(
            prepare_stage8b_p1f_o4_readonly_schedule_with(
                stale,
                &o3,
                &signer,
                o4_now,
                fixture_prepare,
            ),
            Err(Stage8bP1fProducerErrorV1::Freshness)
        ));
    }

    #[tokio::test]
    async fn prepared_and_published_m10_restart_preserve_exact_high_water() {
        let signer = FixtureSigner::new(92);
        let now = timestamp(12, 10, 0);
        let schedule = o3_published_schedule(now, &signer).await;
        let batch = o3_batch(
            observations(
                timestamp(12, 0, 0),
                now,
                MarketDataSourceKind::ReadOnlyPoll,
                0,
            ),
            now,
        );
        let first = authorize_stage8b_p1f_first_m10(FIRST_M10_CONFIRMATION).unwrap();
        let prepared_state = prepared(
            fixture_prepare_m10(
                &signer,
                batch.clone(),
                &"4".repeat(64),
                &schedule,
                Stage8bP1fM10ProducerLineageV1::First(first),
            )
            .unwrap(),
        );
        let path = state_path("m10-prepared");
        persist_stage8b_p1f_m10_producer_state(&path, &prepared_state).unwrap();
        let restarted = load_stage8b_p1f_m10_producer_state(&path).unwrap();
        match fixture_prepare_m10(
            &signer,
            batch.clone(),
            &"4".repeat(64),
            &schedule,
            Stage8bP1fM10ProducerLineageV1::Resume(&restarted),
        )
        .unwrap()
        {
            Stage8bP1fM10PrepareOutcomeV1::ReplayPrepared(replay) => {
                assert_eq!(replay, prepared_state)
            }
            _ => panic!("restart before publication must replay exact Prepared bytes"),
        }

        let published = publish(restarted);
        persist_stage8b_p1f_m10_producer_state(&path, &published).unwrap();
        let restarted = load_stage8b_p1f_m10_producer_state(&path).unwrap();
        match fixture_prepare_m10(
            &signer,
            batch,
            &"4".repeat(64),
            &schedule,
            Stage8bP1fM10ProducerLineageV1::Resume(&restarted),
        )
        .unwrap()
        {
            Stage8bP1fM10PrepareOutcomeV1::IdempotentPublished(same) => {
                assert_eq!(same, published)
            }
            _ => panic!("restart after publication must retain exact Published high-water"),
        }
    }

    #[tokio::test]
    async fn id_publication_replays_retained_prepared_bytes_after_response_loss() {
        let signer = FixtureSigner::new(102);
        let now = timestamp(12, 10, 0);
        let schedule = o3_published_schedule(now, &signer).await;
        let first = authorize_stage8b_p1f_first_m10(FIRST_M10_CONFIRMATION).unwrap();
        let prepared_state = prepared(
            fixture_prepare_m10(
                &signer,
                o3_batch(
                    observations(
                        timestamp(12, 0, 0),
                        now,
                        MarketDataSourceKind::ReadOnlyPoll,
                        0,
                    ),
                    now,
                ),
                &"4".repeat(64),
                &schedule,
                Stage8bP1fM10ProducerLineageV1::First(first),
            )
            .unwrap(),
        );
        let path = state_path("id-response-loss");
        let mut port = FixtureM10PublicationPort {
            fail_next: true,
            ..FixtureM10PublicationPort::default()
        };
        assert!(
            publish_stage8b_p1f_prepared_m10(&path, prepared_state.clone(), &mut port)
                .await
                .is_err()
        );
        let retained = load_stage8b_p1f_m10_producer_state(&path).unwrap();
        assert_eq!(retained, prepared_state);
        assert_eq!(retained.phase(), Stage8bP1fM10ProducerPhaseV1::Prepared);

        let published = publish_stage8b_p1f_prepared_m10(&path, retained, &mut port)
            .await
            .unwrap();
        assert_eq!(published.phase(), Stage8bP1fM10ProducerPhaseV1::Published);
        assert_eq!(port.calls.len(), 2);
        assert_eq!(port.calls[0], port.calls[1]);
        assert_eq!(
            load_stage8b_p1f_m10_producer_state(&path).unwrap(),
            published
        );
    }

    #[tokio::test]
    async fn id_linked_real_redis_response_loss_restarts_prepared_without_duplicate() {
        let redis = RedisServer::start().await;
        runtime_durable_service::initialize_stage8b_p1_redis_namespace(
            &redis.url,
            runtime_durable_service::Stage8bP1RedisConfig::paper_default_auto(),
        )
        .await
        .unwrap();
        let signer = FixtureSigner::new(104);
        let now = timestamp(12, 10, 0);
        let schedule = o3_published_schedule(now, &signer).await;
        let first = authorize_stage8b_p1f_first_m10(FIRST_M10_CONFIRMATION).unwrap();
        let prepared_state = prepared(
            fixture_prepare_m10(
                &signer,
                o3_batch(
                    observations(
                        timestamp(12, 0, 0),
                        now,
                        MarketDataSourceKind::ReadOnlyPoll,
                        0,
                    ),
                    now,
                ),
                &"4".repeat(64),
                &schedule,
                Stage8bP1fM10ProducerLineageV1::First(first),
            )
            .unwrap(),
        );
        let path = state_path("id-linked-real-redis-response-loss");
        let first_feeder =
            runtime_durable_service::Stage8bP1fM10FeederRedisV1::connect_synthetic_local_evidence(
                &redis.url,
                runtime_durable_service::Stage8bP1RedisConfig::paper_default_auto(),
            )
            .await
            .unwrap();
        let mut lossy_port = LoseFirstRedisResponsePort {
            feeder: first_feeder,
            lose_next_response: true,
        };
        assert!(
            publish_stage8b_p1f_prepared_m10(&path, prepared_state.clone(), &mut lossy_port)
                .await
                .is_err()
        );
        drop(lossy_port);
        let retained = load_stage8b_p1f_m10_producer_state(&path).unwrap();
        assert_eq!(retained.phase(), Stage8bP1fM10ProducerPhaseV1::Prepared);
        assert_eq!(retained, prepared_state);

        let mut restarted_port =
            runtime_durable_service::Stage8bP1fM10FeederRedisV1::connect_synthetic_local_evidence(
                &redis.url,
                runtime_durable_service::Stage8bP1RedisConfig::paper_default_auto(),
            )
            .await
            .unwrap();
        let published = publish_stage8b_p1f_prepared_m10(&path, retained, &mut restarted_port)
            .await
            .unwrap();
        assert_eq!(published.phase(), Stage8bP1fM10ProducerPhaseV1::Published);
        assert_eq!(published.publication_sequence(), 1);
        assert_eq!(
            load_stage8b_p1f_m10_producer_state(&path).unwrap(),
            published
        );

        let client = redis::Client::open(redis.url.as_str()).unwrap();
        let mut connection = redis::aio::ConnectionManager::new(client).await.unwrap();
        let namespace = runtime_durable_service::stage8b_p1_redis_namespace();
        let stream_length: u64 = redis::cmd("XLEN")
            .arg(&namespace.canonical_m10_stream)
            .query_async(&mut connection)
            .await
            .unwrap();
        assert_eq!(stream_length, 1);
    }

    #[tokio::test]
    async fn id_publication_refuses_unproven_exact_reread() {
        let signer = FixtureSigner::new(103);
        let now = timestamp(12, 10, 0);
        let schedule = o3_published_schedule(now, &signer).await;
        let first = authorize_stage8b_p1f_first_m10(FIRST_M10_CONFIRMATION).unwrap();
        let prepared_state = prepared(
            fixture_prepare_m10(
                &signer,
                o3_batch(
                    observations(
                        timestamp(12, 0, 0),
                        now,
                        MarketDataSourceKind::ReadOnlyPoll,
                        0,
                    ),
                    now,
                ),
                &"4".repeat(64),
                &schedule,
                Stage8bP1fM10ProducerLineageV1::First(first),
            )
            .unwrap(),
        );
        let path = state_path("id-reread-conflict");
        let mut port = FixtureM10PublicationPort {
            corrupt_receipt: true,
            ..FixtureM10PublicationPort::default()
        };
        assert!(
            publish_stage8b_p1f_prepared_m10(&path, prepared_state.clone(), &mut port)
                .await
                .is_err()
        );
        assert_eq!(
            load_stage8b_p1f_m10_producer_state(&path).unwrap(),
            prepared_state
        );
    }

    #[tokio::test]
    async fn o3_to_o4_m10_continuity_rejects_reset_stale_and_conflict() {
        let signer = FixtureSigner::new(93);
        let o3_now = timestamp(12, 10, 0);
        let o3_schedule = o3_published_schedule(o3_now, &signer).await;
        let o3_observations = observations(
            timestamp(12, 0, 0),
            o3_now,
            MarketDataSourceKind::ReadOnlyPoll,
            0,
        );
        let first = authorize_stage8b_p1f_first_m10(FIRST_M10_CONFIRMATION).unwrap();
        let o3 = publish(prepared(
            fixture_prepare_m10(
                &signer,
                o3_batch(o3_observations.clone(), o3_now),
                &"4".repeat(64),
                &o3_schedule,
                Stage8bP1fM10ProducerLineageV1::First(first),
            )
            .unwrap(),
        ));

        let conflict = o3_batch(
            observations(
                timestamp(12, 0, 0),
                o3_now,
                MarketDataSourceKind::ReadOnlyPoll,
                1,
            ),
            o3_now,
        );
        assert!(matches!(
            fixture_prepare_m10(
                &signer,
                conflict,
                &"4".repeat(64),
                &o3_schedule,
                Stage8bP1fM10ProducerLineageV1::Resume(&o3),
            ),
            Err(Stage8bP1fProducerErrorV1::DurableStateConflict)
        ));

        let stale_observations = observations(
            timestamp(11, 50, 0),
            o3_now,
            MarketDataSourceKind::ReadOnlyPoll,
            0,
        );
        assert!(matches!(
            fixture_prepare_m10(
                &signer,
                o3_batch(stale_observations, o3_now),
                &"4".repeat(64),
                &o3_schedule,
                Stage8bP1fM10ProducerLineageV1::Resume(&o3),
            ),
            Err(Stage8bP1fProducerErrorV1::StaleCandidate)
        ));

        let o4_now = timestamp(12, 20, 0);
        let o4_prepared_schedule = prepare_stage8b_p1f_o4_readonly_schedule_with(
            fixture_adapter_input(o4_now),
            &o3_schedule,
            &signer,
            o4_now,
            fixture_prepare,
        )
        .unwrap();
        let schedule_path = state_path("schedule-o4");
        let mut writer = FixtureWriter::default();
        let o4_schedule = test_publish_stage8b_p1e_prepared_schedule_with_key(
            &schedule_path,
            o4_prepared_schedule,
            &mut writer,
            signer.public_key_ed25519_hex(),
        )
        .await
        .unwrap();
        let o4_observations = observations(
            timestamp(12, 10, 0),
            o4_now,
            MarketDataSourceKind::LiveStream,
            0,
        );
        let o4_prepared = prepared(
            fixture_prepare_m10(
                &signer,
                o4_batch(o4_observations.clone(), o4_now),
                &"4".repeat(64),
                &o4_schedule,
                Stage8bP1fM10ProducerLineageV1::Resume(&o3),
            )
            .unwrap(),
        );
        assert_eq!(o4_prepared.publication_sequence(), 2);
        assert_eq!(
            o4_prepared.producer_phase(),
            Stage8bP1fProducerPhaseV1::O4FinamReadOnly
        );

        let unauthorized_first = authorize_stage8b_p1f_first_m10(FIRST_M10_CONFIRMATION).unwrap();
        assert!(matches!(
            fixture_prepare_m10(
                &signer,
                o4_batch(o4_observations, o4_now),
                &"4".repeat(64),
                &o4_schedule,
                Stage8bP1fM10ProducerLineageV1::First(unauthorized_first),
            ),
            Err(Stage8bP1fProducerErrorV1::InvalidLineage)
        ));
    }

    #[tokio::test]
    async fn newer_candidate_waits_for_exact_prepared_publication() {
        let signer = FixtureSigner::new(94);
        let first_now = timestamp(12, 10, 0);
        let schedule = o3_published_schedule(first_now, &signer).await;
        let first = authorize_stage8b_p1f_first_m10(FIRST_M10_CONFIRMATION).unwrap();
        let pending = prepared(
            fixture_prepare_m10(
                &signer,
                o3_batch(
                    observations(
                        timestamp(12, 0, 0),
                        first_now,
                        MarketDataSourceKind::ReadOnlyPoll,
                        0,
                    ),
                    first_now,
                ),
                &"4".repeat(64),
                &schedule,
                Stage8bP1fM10ProducerLineageV1::First(first),
            )
            .unwrap(),
        );
        let next_now = timestamp(12, 20, 0);
        let o4_prepared_schedule = prepare_stage8b_p1f_o4_readonly_schedule_with(
            fixture_adapter_input(next_now),
            &schedule,
            &signer,
            next_now,
            fixture_prepare,
        )
        .unwrap();
        let schedule_path = state_path("pending-o4-schedule");
        let mut writer = FixtureWriter::default();
        let o4_schedule = test_publish_stage8b_p1e_prepared_schedule_with_key(
            &schedule_path,
            o4_prepared_schedule,
            &mut writer,
            signer.public_key_ed25519_hex(),
        )
        .await
        .unwrap();
        assert!(matches!(
            fixture_prepare_m10(
                &signer,
                o4_batch(
                    observations(
                        timestamp(12, 10, 0),
                        next_now,
                        MarketDataSourceKind::LiveStream,
                        0,
                    ),
                    next_now,
                ),
                &"4".repeat(64),
                &o4_schedule,
                Stage8bP1fM10ProducerLineageV1::Resume(&pending),
            ),
            Err(Stage8bP1fProducerErrorV1::PreparedPublicationPending)
        ));
    }

    #[tokio::test]
    async fn o4_streaming_receipts_use_completed_m10_freshness() {
        let signer = FixtureSigner::new(95);
        let o3_now = timestamp(12, 10, 0);
        let o3_schedule = o3_published_schedule(o3_now, &signer).await;
        let first = authorize_stage8b_p1f_first_m10(FIRST_M10_CONFIRMATION).unwrap();
        let o3 = publish(prepared(
            fixture_prepare_m10(
                &signer,
                o3_batch(
                    observations(
                        timestamp(12, 0, 0),
                        o3_now,
                        MarketDataSourceKind::ReadOnlyPoll,
                        0,
                    ),
                    o3_now,
                ),
                &"4".repeat(64),
                &o3_schedule,
                Stage8bP1fM10ProducerLineageV1::First(first),
            )
            .unwrap(),
        ));

        let o4_now = timestamp(12, 20, 2);
        let o4_prepared_schedule = prepare_stage8b_p1f_o4_readonly_schedule_with(
            fixture_adapter_input(o4_now),
            &o3_schedule,
            &signer,
            o4_now,
            fixture_prepare,
        )
        .unwrap();
        let schedule_path = state_path("streaming-o4-schedule");
        let mut writer = FixtureWriter::default();
        let o4_schedule = test_publish_stage8b_p1e_prepared_schedule_with_key(
            &schedule_path,
            o4_prepared_schedule,
            &mut writer,
            signer.public_key_ed25519_hex(),
        )
        .await
        .unwrap();

        let streaming = streaming_observations(timestamp(12, 10, 0));
        let accepted = prepared(
            fixture_prepare_m10(
                &signer,
                o4_batch(streaming.clone(), o4_now),
                &"4".repeat(64),
                &o4_schedule,
                Stage8bP1fM10ProducerLineageV1::Resume(&o3),
            )
            .unwrap(),
        );
        assert_eq!(accepted.publication_sequence(), 2);

        let mut future = streaming.clone();
        future[9].observed_at_utc = o4_now + Duration::milliseconds(1);
        assert!(matches!(
            fixture_prepare_m10(
                &signer,
                o4_batch(future, o4_now),
                &"4".repeat(64),
                &o4_schedule,
                Stage8bP1fM10ProducerLineageV1::Resume(&o3),
            ),
            Err(Stage8bP1fProducerErrorV1::Freshness)
        ));

        let stale_now = o4_now + Duration::seconds(31);
        let stale_schedule_input = fixture_adapter_input(stale_now);
        let stale_schedule = prepare_stage8b_p1f_o4_readonly_schedule_with(
            stale_schedule_input,
            &o3_schedule,
            &signer,
            stale_now,
            fixture_prepare,
        )
        .unwrap();
        let stale_path = state_path("stale-o4-schedule");
        let mut writer = FixtureWriter::default();
        let stale_schedule = test_publish_stage8b_p1e_prepared_schedule_with_key(
            &stale_path,
            stale_schedule,
            &mut writer,
            signer.public_key_ed25519_hex(),
        )
        .await
        .unwrap();
        assert!(matches!(
            fixture_prepare_m10(
                &signer,
                o4_batch(streaming.clone(), stale_now),
                &"4".repeat(64),
                &stale_schedule,
                Stage8bP1fM10ProducerLineageV1::Resume(&o3),
            ),
            Err(Stage8bP1fProducerErrorV1::Freshness)
        ));

        let mut non_monotonic = streaming;
        non_monotonic[8].observed_at_utc = non_monotonic[7].observed_at_utc - Duration::seconds(1);
        assert!(matches!(
            fixture_prepare_m10(
                &signer,
                o4_batch(non_monotonic, o4_now),
                &"4".repeat(64),
                &o4_schedule,
                Stage8bP1fM10ProducerLineageV1::Resume(&o3),
            ),
            Err(Stage8bP1fProducerErrorV1::Freshness)
        ));
    }

    #[tokio::test]
    async fn m10_admission_requires_fresh_signed_schedule_authority() {
        let signer = FixtureSigner::new(96);
        let now = timestamp(12, 10, 0);
        let schedule = o3_published_schedule(now, &signer).await;
        let batch = o3_batch(
            observations(
                timestamp(12, 0, 0),
                now,
                MarketDataSourceKind::ReadOnlyPoll,
                0,
            ),
            now,
        );

        let bad_signature = mutate_published_schedule(
            &schedule,
            |envelope| envelope.signature_ed25519_hex = "0".repeat(128),
            true,
        );
        let first = authorize_stage8b_p1f_first_m10(FIRST_M10_CONFIRMATION).unwrap();
        assert!(fixture_prepare_m10(
            &signer,
            batch.clone(),
            &"4".repeat(64),
            &bad_signature,
            Stage8bP1fM10ProducerLineageV1::First(first),
        )
        .is_err());

        let bad_payload = mutate_published_schedule(
            &schedule,
            |envelope| {
                envelope.payload.normalized_schedule.sessions[0].end_utc =
                    "2026-09-14T15:40:00.000000Z".to_string()
            },
            true,
        );
        let first = authorize_stage8b_p1f_first_m10(FIRST_M10_CONFIRMATION).unwrap();
        assert!(fixture_prepare_m10(
            &signer,
            batch.clone(),
            &"4".repeat(64),
            &bad_payload,
            Stage8bP1fM10ProducerLineageV1::First(first),
        )
        .is_err());

        let bad_state_hash = mutate_published_schedule(&schedule, |_| {}, false);
        let mut bad_state_hash_value = serde_json::to_value(bad_state_hash).unwrap();
        bad_state_hash_value["envelope_sha256"] = serde_json::Value::String("9".repeat(64));
        let bad_state_hash: Stage8bP1eSchedulePublisherStateV1 =
            serde_json::from_value(bad_state_hash_value).unwrap();
        let first = authorize_stage8b_p1f_first_m10(FIRST_M10_CONFIRMATION).unwrap();
        assert!(fixture_prepare_m10(
            &signer,
            batch.clone(),
            &"4".repeat(64),
            &bad_state_hash,
            Stage8bP1fM10ProducerLineageV1::First(first),
        )
        .is_err());

        let first = authorize_stage8b_p1f_first_m10(FIRST_M10_CONFIRMATION).unwrap();
        assert!(fixture_prepare_m10(
            &signer,
            batch,
            &"8".repeat(64),
            &schedule,
            Stage8bP1fM10ProducerLineageV1::First(first),
        )
        .is_err());

        let mut expired_input = fixture_input(now, "2026-09-14T15:50:00.000000Z");
        expired_input
            .payload
            .normalized_schedule
            .source_expires_at_utc = (now + Duration::milliseconds(500))
            .to_rfc3339_opts(chrono::SecondsFormat::Micros, true);
        let first_schedule =
            authorize_stage8b_p1e_first_publication("AUTHORIZE-STAGE8B-P1E-FIRST-PUBLICATION")
                .unwrap();
        let expired_prepared = fixture_prepare(
            expired_input,
            Stage8bP1eSchedulePublisherLineage::First(first_schedule),
            &signer,
        )
        .unwrap();
        let expired_path = state_path("expired-signed-schedule");
        let mut writer = FixtureWriter::default();
        let expired_schedule = test_publish_stage8b_p1e_prepared_schedule_with_key(
            &expired_path,
            expired_prepared,
            &mut writer,
            signer.public_key_ed25519_hex(),
        )
        .await
        .unwrap();
        let admission_now = now + Duration::seconds(1);
        let expired_batch = o3_batch(
            observations(
                timestamp(12, 0, 0),
                now,
                MarketDataSourceKind::ReadOnlyPoll,
                0,
            ),
            admission_now,
        );
        let first = authorize_stage8b_p1f_first_m10(FIRST_M10_CONFIRMATION).unwrap();
        assert!(fixture_prepare_m10(
            &signer,
            expired_batch,
            &"4".repeat(64),
            &expired_schedule,
            Stage8bP1fM10ProducerLineageV1::First(first),
        )
        .is_err());
    }
}
