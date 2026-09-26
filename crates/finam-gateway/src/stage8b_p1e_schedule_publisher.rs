//! Stage 8B-P1-e signed broker-neutral schedule publisher.
//!
//! The module accepts only already normalized GET/read evidence. It has no
//! FINAM write endpoint and receives signing capability from its caller. The
//! Redis stream must be provisioned separately: publication uses NOMKSTREAM.

use async_trait::async_trait;
use chrono::{DateTime, FixedOffset, SecondsFormat, Utc};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use redis::aio::ConnectionManager;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use strategy_runtime_core::{
    authenticate_stage8b_p1e_schedule_observation_v3, stage8b_p1e_canonical_json,
    stage8b_p1e_schedule_payload_sha256, stage8b_p1e_schedule_semantic_sha256,
    stage8b_p1e_schedule_unsigned_signature_sha256, verify_stage8b_p1e_schedule_envelope_v3,
    Stage8bP1eScheduleEnvelopeV3, Stage8bP1eSchedulePayloadV2,
    Stage8bP1eScheduleSemanticIdentityV1, Stage8bP1eScheduleVerificationContextV1,
    Stage8bP1eStage4SemanticStateV1, STAGE8B_P1E_SCHEDULE_PUBLIC_KEY_ED25519_HEX,
    STAGE8B_P1E_SCHEDULE_STREAM,
};

const ENVELOPE_DOMAIN: &str = "moex.stage8b.p1e.schedule-envelope.v3";
const PRODUCER_ID: &str = "finam-readonly-schedule-normalizer-v3";
const PRODUCER_CONTRACT: &str = "finam-rest-schedule-to-broker-neutral-v3";
const SEMANTIC_DOMAIN: &str = "moex.stage8b.p1e.schedule-semantic-identity.v1";
const KEY_ID: &str = "schedule-ed25519-v1";
const STATE_DOMAIN: &str = "moex.stage8b.p1e.schedule-publisher-state.v1";
const FIRST_PUBLICATION_CONFIRMATION: &str = "AUTHORIZE-STAGE8B-P1E-FIRST-PUBLICATION";
const MAXLEN: usize = 4096;
const NORMALIZED_MAX_AGE_MS: i64 = 86_400_000;
static TEMP_COUNTER: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, thiserror::Error)]
pub enum Stage8bP1eSchedulePublisherError {
    #[error("normalized schedule input is invalid")]
    InvalidInput,
    #[error("publisher signing authority does not match the pinned schedule key")]
    SigningAuthorityMismatch,
    #[error("publisher durable high-water is missing or conflicts")]
    DurableStateConflict,
    #[error("publisher durable state I/O failed: {0:?}")]
    StateIo(ErrorKind),
    #[error("schedule signing failed")]
    SigningFailed,
    #[error("Redis schedule publication failed")]
    Redis(#[from] redis::RedisError),
}

impl From<std::io::Error> for Stage8bP1eSchedulePublisherError {
    fn from(value: std::io::Error) -> Self {
        Self::StateIo(value.kind())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stage8bP1eSchedulePublisherInputV1 {
    pub payload: Stage8bP1eSchedulePayloadV2,
    pub operational_identity_sha256: String,
    pub runtime_config_fingerprint_sha256: String,
    pub instrument_map_fingerprint_sha256: String,
    pub published_at_utc: DateTime<Utc>,
}

/// Exact read-only inputs used by the FINAM-to-broker-neutral schedule
/// adapter. The parsed response is cross-checked against the exact response
/// bytes so `raw_response_sha256` cannot be attached to a different DTO.
#[derive(Debug, Clone, PartialEq)]
pub struct Stage8bP1eReadonlyScheduleAdapterInputV1 {
    pub exact_finam_response_bytes: Vec<u8>,
    pub schedule: broker_finam::AssetScheduleResponse,
    pub stage4_report: broker_core::Stage4BootstrapEvidenceReport,
    pub registry_entry: broker_core::InstrumentMapEntry,
    pub registry_version: String,
    pub registry_identity_sha256: String,
    pub operational_identity_sha256: String,
    pub runtime_config_fingerprint_sha256: String,
    pub instrument_map_fingerprint_sha256: String,
    pub source_observed_at_utc: DateTime<Utc>,
    pub published_at_utc: DateTime<Utc>,
    pub evidence_kind: strategy_runtime_core::Stage8bP1eScheduleEvidenceKindV1,
    pub schedule_state: strategy_runtime_core::Stage8bP1eScheduleStateV1,
    pub boundary_proof: Option<strategy_runtime_core::Stage8bP1eDayBoundaryProofV1>,
}

/// Pure source adapter for the accepted I1A schedule publisher. It performs
/// no FINAM request, Redis write, signing or runtime action.
pub fn adapt_stage8b_p1e_readonly_schedule(
    input: Stage8bP1eReadonlyScheduleAdapterInputV1,
) -> Result<Stage8bP1eSchedulePublisherInputV1, Stage8bP1eSchedulePublisherError> {
    let parsed: broker_finam::AssetScheduleResponse =
        serde_json::from_slice(&input.exact_finam_response_bytes)
            .map_err(|_| Stage8bP1eSchedulePublisherError::InvalidInput)?;
    if parsed != input.schedule
        || input.schedule.symbol != "IMOEXF@RTSX"
        || input.registry_entry.internal_symbol.0 != "IMOEXF"
        || input.registry_entry.broker != broker_core::BrokerKind::Finam
        || input.registry_entry.broker_symbol.0 != "IMOEXF@RTSX"
        || input.registry_entry.exchange != broker_core::Exchange::Moex
        || input.registry_entry.market != broker_core::Market::Futures
        || input.registry_entry.price_step != rust_decimal::Decimal::new(5, 1)
        || !input.registry_entry.is_tradable
        || !valid_registry_version(&input.registry_version)
        || !valid_sha256(&input.registry_identity_sha256)
        || !valid_sha256(&input.operational_identity_sha256)
        || !valid_sha256(&input.runtime_config_fingerprint_sha256)
        || !valid_sha256(&input.instrument_map_fingerprint_sha256)
        || input.published_at_utc < input.source_observed_at_utc
    {
        return Err(Stage8bP1eSchedulePublisherError::InvalidInput);
    }

    let moscow =
        FixedOffset::east_opt(3 * 60 * 60).ok_or(Stage8bP1eSchedulePublisherError::InvalidInput)?;
    let mut parsed_sessions = input
        .schedule
        .sessions
        .iter()
        .map(|session| {
            let interval = session
                .interval
                .as_ref()
                .ok_or(Stage8bP1eSchedulePublisherError::InvalidInput)?;
            let start = parse_finam_schedule_timestamp(interval.start_time.as_deref())?;
            let end = parse_finam_schedule_timestamp(interval.end_time.as_deref())?;
            if start >= end {
                return Err(Stage8bP1eSchedulePublisherError::InvalidInput);
            }
            Ok((
                start,
                end,
                map_finam_session_type(session.session_type.as_deref())?,
            ))
        })
        .collect::<Result<Vec<_>, _>>()?;
    if parsed_sessions.is_empty() {
        return Err(Stage8bP1eSchedulePublisherError::InvalidInput);
    }
    let trading_day = parsed_sessions[0].0.with_timezone(&moscow).date_naive();
    if parsed_sessions.iter().any(|(start, end, _)| {
        start.with_timezone(&moscow).date_naive() != trading_day
            || end.with_timezone(&moscow).date_naive() != trading_day
    }) {
        return Err(Stage8bP1eSchedulePublisherError::InvalidInput);
    }
    let mut sessions = parsed_sessions
        .drain(..)
        .map(
            |(start, end, session_type)| strategy_runtime_core::Stage8bP1eScheduleSessionV1 {
                end_utc: canonical_timestamp(end),
                session_type,
                start_utc: canonical_timestamp(start),
            },
        )
        .collect::<Vec<_>>();
    sessions.sort_by(|left, right| {
        (
            &left.start_utc,
            &left.end_utc,
            schedule_type_rank(left.session_type),
        )
            .cmp(&(
                &right.start_utc,
                &right.end_utc,
                schedule_type_rank(right.session_type),
            ))
    });
    if !sessions.iter().any(|session| {
        session.session_type == strategy_runtime_core::Stage8bP1eScheduleSessionTypeV1::TradableOpen
    }) || sessions
        .windows(2)
        .any(|pair| pair[1].start_utc <= pair[0].end_utc)
    {
        return Err(Stage8bP1eSchedulePublisherError::InvalidInput);
    }

    let stage4_report_bytes = stage8b_p1e_canonical_json(&input.stage4_report)
        .map_err(|_| Stage8bP1eSchedulePublisherError::InvalidInput)?;
    let (stage4_observed_at, stage4_expires_at) = stage4_schedule_window(&input.stage4_report)?;
    let normalized_payload_sha256 = sha256_hex(
        &stage8b_p1e_canonical_json(&sessions)
            .map_err(|_| Stage8bP1eSchedulePublisherError::InvalidInput)?,
    );
    let payload = Stage8bP1eSchedulePayloadV2 {
        domain: "moex.stage8b.p1e.schedule-payload.v2".to_string(),
        instrument: strategy_runtime_core::Stage8bP1eScheduleInstrumentV1 {
            board: "FUT".to_string(),
            broker_symbol: "IMOEXF@RTSX".to_string(),
            exchange: "moex".to_string(),
            market: "futures".to_string(),
            symbol: "IMOEXF".to_string(),
            tick_size: "0.5".to_string(),
            venue_mic: "RTSX".to_string(),
        },
        normalized_schedule: strategy_runtime_core::Stage8bP1eNormalizedScheduleV2 {
            normalized_payload_sha256,
            raw_response_sha256: sha256_hex(&input.exact_finam_response_bytes),
            sessions,
            source_expires_at_utc: canonical_timestamp(
                input.source_observed_at_utc
                    + chrono::Duration::milliseconds(NORMALIZED_MAX_AGE_MS),
            ),
            source_observed_at_utc: canonical_timestamp(input.source_observed_at_utc),
        },
        registry: strategy_runtime_core::Stage8bP1eScheduleRegistryV1 {
            registry_identity_sha256: input.registry_identity_sha256,
            registry_version: input.registry_version,
        },
        schema_version: 2,
        stage4_evidence: strategy_runtime_core::Stage8bP1eStage4EvidenceV2 {
            boundary_proof: input.boundary_proof,
            evidence_kind: input.evidence_kind,
            report_canonical_json_hex: encode_lower_hex(&stage4_report_bytes),
            report_sha256: sha256_hex(&stage4_report_bytes),
            schedule_state: input.schedule_state,
            source_expires_at_utc: canonical_timestamp(stage4_expires_at),
            source_observed_at_utc: canonical_timestamp(stage4_observed_at),
        },
        timezone: "Europe/Moscow".to_string(),
        trading_day: trading_day.format("%Y-%m-%d").to_string(),
    };
    Ok(Stage8bP1eSchedulePublisherInputV1 {
        payload,
        operational_identity_sha256: input.operational_identity_sha256,
        runtime_config_fingerprint_sha256: input.runtime_config_fingerprint_sha256,
        instrument_map_fingerprint_sha256: input.instrument_map_fingerprint_sha256,
        published_at_utc: input.published_at_utc,
    })
}

fn parse_finam_schedule_timestamp(
    value: Option<&str>,
) -> Result<DateTime<Utc>, Stage8bP1eSchedulePublisherError> {
    DateTime::parse_from_rfc3339(value.ok_or(Stage8bP1eSchedulePublisherError::InvalidInput)?)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|_| Stage8bP1eSchedulePublisherError::InvalidInput)
}

fn canonical_timestamp(value: DateTime<Utc>) -> String {
    value.to_rfc3339_opts(SecondsFormat::Micros, true)
}

fn map_finam_session_type(
    value: Option<&str>,
) -> Result<strategy_runtime_core::Stage8bP1eScheduleSessionTypeV1, Stage8bP1eSchedulePublisherError>
{
    match value {
        Some("SESSION_TYPE_MAIN") | Some("SESSION_TYPE_MORNING") | Some("SESSION_TYPE_EVENING") => {
            Ok(strategy_runtime_core::Stage8bP1eScheduleSessionTypeV1::TradableOpen)
        }
        Some("SESSION_TYPE_BREAK") | Some("SESSION_TYPE_CLEARING") => {
            Ok(strategy_runtime_core::Stage8bP1eScheduleSessionTypeV1::BreakOrClearing)
        }
        Some("SESSION_TYPE_MAINTENANCE") => {
            Ok(strategy_runtime_core::Stage8bP1eScheduleSessionTypeV1::Maintenance)
        }
        _ => Err(Stage8bP1eSchedulePublisherError::InvalidInput),
    }
}

fn schedule_type_rank(value: strategy_runtime_core::Stage8bP1eScheduleSessionTypeV1) -> u8 {
    match value {
        strategy_runtime_core::Stage8bP1eScheduleSessionTypeV1::TradableOpen => 1,
        strategy_runtime_core::Stage8bP1eScheduleSessionTypeV1::BreakOrClearing => 2,
        strategy_runtime_core::Stage8bP1eScheduleSessionTypeV1::Maintenance => 3,
    }
}

fn stage4_schedule_window(
    report: &broker_core::Stage4BootstrapEvidenceReport,
) -> Result<(DateTime<Utc>, DateTime<Utc>), Stage8bP1eSchedulePublisherError> {
    if report.status != broker_core::Stage4BootstrapEvidenceReportStatus::Accepted {
        return Err(Stage8bP1eSchedulePublisherError::InvalidInput);
    }
    let schedule = report
        .source_sections
        .iter()
        .find(|section| section.section == broker_core::Stage4BrokerTruthFreshnessSection::Schedule)
        .ok_or(Stage8bP1eSchedulePublisherError::InvalidInput)?;
    let schedule_age_ms = schedule
        .age_ms
        .filter(|value| *value >= 0)
        .ok_or(Stage8bP1eSchedulePublisherError::InvalidInput)?;
    let observed = report.checked_ts - chrono::Duration::milliseconds(schedule_age_ms);
    let mut expiry = None;
    for section in report
        .source_sections
        .iter()
        .filter(|section| section.required_for_bootstrap)
    {
        let age_ms = section
            .age_ms
            .filter(|value| *value >= 0)
            .ok_or(Stage8bP1eSchedulePublisherError::InvalidInput)?;
        let age_ms =
            u64::try_from(age_ms).map_err(|_| Stage8bP1eSchedulePublisherError::InvalidInput)?;
        if section.source_status != broker_core::Stage4BrokerTruthSourceStatus::Present
            || section.freshness_status != broker_core::Stage4BrokerTruthFreshnessStatus::Fresh
            || section.blocks_bootstrap
            || age_ms > section.max_age_ms
        {
            return Err(Stage8bP1eSchedulePublisherError::InvalidInput);
        }
        let remaining_ms = section.max_age_ms - age_ms;
        let candidate = report.checked_ts
            + chrono::Duration::milliseconds(
                i64::try_from(remaining_ms)
                    .map_err(|_| Stage8bP1eSchedulePublisherError::InvalidInput)?,
            );
        expiry = Some(expiry.map_or(candidate, |current: DateTime<Utc>| current.min(candidate)));
    }
    Ok((
        observed,
        expiry.ok_or(Stage8bP1eSchedulePublisherError::InvalidInput)?,
    ))
}

fn valid_registry_version(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
}

pub trait Stage8bP1eScheduleSigner {
    fn public_key_ed25519_hex(&self) -> &str;
    fn sign_raw_prehash(
        &self,
        digest: &[u8; 32],
    ) -> Result<[u8; 64], Stage8bP1eSchedulePublisherError>;
}

/// Explicit one-shot authority. Missing state on restart does not create this
/// value and therefore cannot reset sequence or semantic revision.
pub struct Stage8bP1eFirstPublicationAuthorization(());

pub fn authorize_stage8b_p1e_first_publication(
    confirmation: &str,
) -> Result<Stage8bP1eFirstPublicationAuthorization, Stage8bP1eSchedulePublisherError> {
    if confirmation != FIRST_PUBLICATION_CONFIRMATION {
        return Err(Stage8bP1eSchedulePublisherError::DurableStateConflict);
    }
    Ok(Stage8bP1eFirstPublicationAuthorization(()))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage8bP1eSchedulePublisherPhaseV1 {
    Prepared,
    Published,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stage8bP1eSchedulePublisherStateV1 {
    schema_version: u16,
    domain: String,
    phase: Stage8bP1eSchedulePublisherPhaseV1,
    source_generation: String,
    publication_sequence: String,
    semantic_revision: String,
    schedule_semantic_sha256: String,
    published_at_utc: String,
    envelope_sha256: String,
    exact_envelope_hex: String,
    redis_stream_id: Option<String>,
}

impl Stage8bP1eSchedulePublisherStateV1 {
    pub fn phase(&self) -> Stage8bP1eSchedulePublisherPhaseV1 {
        self.phase
    }

    pub fn publication_sequence(&self) -> u64 {
        self.publication_sequence
            .parse()
            .expect("validated publisher sequence")
    }

    pub fn semantic_revision(&self) -> u64 {
        self.semantic_revision
            .parse()
            .expect("validated semantic revision")
    }

    pub fn schedule_semantic_sha256(&self) -> &str {
        &self.schedule_semantic_sha256
    }

    pub fn envelope_sha256(&self) -> &str {
        &self.envelope_sha256
    }

    pub fn redis_stream_id(&self) -> Option<&str> {
        self.redis_stream_id.as_deref()
    }

    pub fn exact_envelope_bytes(&self) -> Result<Vec<u8>, Stage8bP1eSchedulePublisherError> {
        decode_lower_hex(&self.exact_envelope_hex)
    }

    fn validate(&self) -> Result<(), Stage8bP1eSchedulePublisherError> {
        let (envelope, context) = self.validate_structural_state()?;
        authenticate_stage8b_p1e_schedule_observation_v3(&envelope, &context)
            .map_err(|_| Stage8bP1eSchedulePublisherError::DurableStateConflict)?;
        Ok(())
    }

    fn validate_structural_state(
        &self,
    ) -> Result<(Vec<u8>, Stage8bP1eScheduleVerificationContextV1), Stage8bP1eSchedulePublisherError>
    {
        let envelope = self.exact_envelope_bytes()?;
        let parsed: Stage8bP1eScheduleEnvelopeV3 = serde_json::from_slice(&envelope)
            .map_err(|_| Stage8bP1eSchedulePublisherError::DurableStateConflict)?;
        let canonical = stage8b_p1e_canonical_json(&parsed)
            .map_err(|_| Stage8bP1eSchedulePublisherError::DurableStateConflict)?;
        if self.schema_version != 1
            || self.domain != STATE_DOMAIN
            || self.source_generation != "1"
            || parsed.schema_version != 3
            || parsed.domain != ENVELOPE_DOMAIN
            || parsed.producer_id != PRODUCER_ID
            || parsed.producer_contract_version != PRODUCER_CONTRACT
            || parsed.semantic_identity_contract_version != 1
            || parsed.key_id != KEY_ID
            || parsed.key_generation != 2
            || !valid_nonzero_decimal(&self.publication_sequence)
            || !valid_nonzero_decimal(&self.semantic_revision)
            || !valid_sha256(&self.schedule_semantic_sha256)
            || !valid_sha256(&self.envelope_sha256)
            || canonical != envelope
            || sha256_hex(&envelope) != self.envelope_sha256
            || parsed.source_generation != self.source_generation
            || parsed.publication_sequence != self.publication_sequence
            || parsed.semantic_revision != self.semantic_revision
            || parsed.schedule_semantic_sha256 != self.schedule_semantic_sha256
            || parsed.published_at_utc != self.published_at_utc
            || match self.phase {
                Stage8bP1eSchedulePublisherPhaseV1::Prepared => self.redis_stream_id.is_some(),
                Stage8bP1eSchedulePublisherPhaseV1::Published => {
                    match self.redis_stream_id.as_deref() {
                        Some(value) => !valid_redis_stream_id(value),
                        None => true,
                    }
                }
            }
        {
            return Err(Stage8bP1eSchedulePublisherError::DurableStateConflict);
        }
        let published_at = DateTime::parse_from_rfc3339(&parsed.published_at_utc)
            .map_err(|_| Stage8bP1eSchedulePublisherError::DurableStateConflict)?
            .with_timezone(&Utc);
        let context = Stage8bP1eScheduleVerificationContextV1 {
            expected_instrument_map_fingerprint_sha256: parsed
                .instrument_map_fingerprint_sha256
                .clone(),
            expected_operational_identity_sha256: parsed.operational_identity_sha256.clone(),
            expected_registry_identity_sha256: parsed
                .payload
                .registry
                .registry_identity_sha256
                .clone(),
            expected_registry_version: parsed.payload.registry.registry_version.clone(),
            expected_runtime_config_fingerprint_sha256: parsed
                .runtime_config_fingerprint_sha256
                .clone(),
            high_water: None,
            trusted_now: published_at,
        };
        Ok((envelope, context))
    }

    /// Verifies the retained publisher state as fresh route authority at the
    /// caller's admission instant. Unlike `validate`, this uses the accepted
    /// fresh verifier rather than authenticating a historical observation at
    /// its original publication instant.
    pub(crate) fn verify_fresh_envelope(
        &self,
        expected_operational_identity_sha256: &str,
        trusted_now: DateTime<Utc>,
    ) -> Result<Vec<u8>, Stage8bP1eSchedulePublisherError> {
        let (envelope, mut context) = self.validate_structural_state()?;
        if self.phase != Stage8bP1eSchedulePublisherPhaseV1::Published {
            return Err(Stage8bP1eSchedulePublisherError::DurableStateConflict);
        }
        context.expected_operational_identity_sha256 =
            expected_operational_identity_sha256.to_string();
        context.trusted_now = trusted_now;
        verify_stage8b_p1e_schedule_envelope_v3(&envelope, &context)
            .map_err(|_| Stage8bP1eSchedulePublisherError::DurableStateConflict)?;
        Ok(envelope)
    }

    #[cfg(test)]
    pub(crate) fn test_verify_fresh_envelope_with_key(
        &self,
        expected_operational_identity_sha256: &str,
        trusted_now: DateTime<Utc>,
        public_key_hex: &str,
    ) -> Result<Vec<u8>, Stage8bP1eSchedulePublisherError> {
        let (envelope, mut context) = self.validate_structural_state()?;
        if self.phase != Stage8bP1eSchedulePublisherPhaseV1::Published {
            return Err(Stage8bP1eSchedulePublisherError::DurableStateConflict);
        }
        context.expected_operational_identity_sha256 =
            expected_operational_identity_sha256.to_string();
        context.trusted_now = trusted_now;
        strategy_runtime_core::stage8b_p1e_test_verify_schedule_envelope_with_key(
            &envelope,
            &context,
            public_key_hex,
            DateTime::parse_from_rfc3339("2026-01-01T00:00:00.000000Z")
                .expect("fixed fixture trust start")
                .with_timezone(&Utc),
            DateTime::parse_from_rfc3339("2027-01-01T00:00:00.000000Z")
                .expect("fixed fixture trust end")
                .with_timezone(&Utc),
        )
        .map_err(|_| Stage8bP1eSchedulePublisherError::DurableStateConflict)?;
        Ok(envelope)
    }

    #[cfg(test)]
    fn validate_with_fixture_key(
        &self,
        public_key_hex: &str,
    ) -> Result<(), Stage8bP1eSchedulePublisherError> {
        let (envelope, context) = self.validate_structural_state()?;
        strategy_runtime_core::stage8b_p1e_test_authenticate_schedule_observation_with_key(
            &envelope,
            &context,
            public_key_hex,
            DateTime::parse_from_rfc3339("2026-01-01T00:00:00.000000Z")
                .expect("fixed fixture trust start")
                .with_timezone(&Utc),
            DateTime::parse_from_rfc3339("2027-01-01T00:00:00.000000Z")
                .expect("fixed fixture trust end")
                .with_timezone(&Utc),
        )
        .map_err(|_| Stage8bP1eSchedulePublisherError::DurableStateConflict)?;
        Ok(())
    }
}

pub enum Stage8bP1eSchedulePublisherLineage<'a> {
    First(Stage8bP1eFirstPublicationAuthorization),
    Resume(&'a Stage8bP1eSchedulePublisherStateV1),
}

pub fn prepare_stage8b_p1e_schedule_publication(
    input: Stage8bP1eSchedulePublisherInputV1,
    lineage: Stage8bP1eSchedulePublisherLineage<'_>,
    signer: &impl Stage8bP1eScheduleSigner,
) -> Result<Stage8bP1eSchedulePublisherStateV1, Stage8bP1eSchedulePublisherError> {
    prepare_stage8b_p1e_schedule_publication_with_key(
        input,
        lineage,
        signer,
        STAGE8B_P1E_SCHEDULE_PUBLIC_KEY_ED25519_HEX,
        &Stage8bP1eSchedulePublisherStateV1::validate,
    )
}

fn prepare_stage8b_p1e_schedule_publication_with_key(
    input: Stage8bP1eSchedulePublisherInputV1,
    lineage: Stage8bP1eSchedulePublisherLineage<'_>,
    signer: &impl Stage8bP1eScheduleSigner,
    expected_public_key_hex: &str,
    validate_state: &impl Fn(
        &Stage8bP1eSchedulePublisherStateV1,
    ) -> Result<(), Stage8bP1eSchedulePublisherError>,
) -> Result<Stage8bP1eSchedulePublisherStateV1, Stage8bP1eSchedulePublisherError> {
    if signer.public_key_ed25519_hex() != expected_public_key_hex
        || !valid_sha256(&input.operational_identity_sha256)
        || !valid_sha256(&input.runtime_config_fingerprint_sha256)
        || !valid_sha256(&input.instrument_map_fingerprint_sha256)
    {
        return Err(Stage8bP1eSchedulePublisherError::SigningAuthorityMismatch);
    }
    let published_at_utc = input.published_at_utc;
    let semantic_identity = Stage8bP1eScheduleSemanticIdentityV1 {
        domain: SEMANTIC_DOMAIN.to_string(),
        instrument: input.payload.instrument.clone(),
        registry: input.payload.registry.clone(),
        schema_version: 1,
        sessions: input.payload.normalized_schedule.sessions.clone(),
        stage4_semantic_state: Stage8bP1eStage4SemanticStateV1 {
            boundary_proof: input.payload.stage4_evidence.boundary_proof.clone(),
            evidence_kind: input.payload.stage4_evidence.evidence_kind,
            schedule_state: input.payload.stage4_evidence.schedule_state,
        },
        timeframe_sec: 600,
        timezone: input.payload.timezone.clone(),
        trading_day: input.payload.trading_day.clone(),
    };
    let semantic_hash = stage8b_p1e_schedule_semantic_sha256(&semantic_identity)
        .map_err(|_| Stage8bP1eSchedulePublisherError::InvalidInput)?;
    let (publication_sequence, semantic_revision) = match lineage {
        Stage8bP1eSchedulePublisherLineage::First(_) => (1, 1),
        Stage8bP1eSchedulePublisherLineage::Resume(prior) => {
            validate_state(prior)?;
            if prior.phase != Stage8bP1eSchedulePublisherPhaseV1::Published {
                return Err(Stage8bP1eSchedulePublisherError::DurableStateConflict);
            }
            let prior_published_at = DateTime::parse_from_rfc3339(&prior.published_at_utc)
                .map_err(|_| Stage8bP1eSchedulePublisherError::DurableStateConflict)?
                .with_timezone(&Utc);
            if published_at_utc <= prior_published_at {
                return Err(Stage8bP1eSchedulePublisherError::DurableStateConflict);
            }
            let next_sequence = prior
                .publication_sequence()
                .checked_add(1)
                .ok_or(Stage8bP1eSchedulePublisherError::DurableStateConflict)?;
            let next_revision = if prior.schedule_semantic_sha256 == semantic_hash {
                prior.semantic_revision()
            } else {
                prior
                    .semantic_revision()
                    .checked_add(1)
                    .ok_or(Stage8bP1eSchedulePublisherError::DurableStateConflict)?
            };
            (next_sequence, next_revision)
        }
    };
    let payload_sha256 = stage8b_p1e_schedule_payload_sha256(&input.payload)
        .map_err(|_| Stage8bP1eSchedulePublisherError::InvalidInput)?;
    let mut envelope = Stage8bP1eScheduleEnvelopeV3 {
        domain: ENVELOPE_DOMAIN.to_string(),
        instrument_map_fingerprint_sha256: input.instrument_map_fingerprint_sha256,
        key_generation: 2,
        key_id: KEY_ID.to_string(),
        operational_identity_sha256: input.operational_identity_sha256,
        payload: input.payload,
        payload_sha256,
        producer_contract_version: PRODUCER_CONTRACT.to_string(),
        producer_id: PRODUCER_ID.to_string(),
        publication_sequence: publication_sequence.to_string(),
        published_at_utc: input
            .published_at_utc
            .to_rfc3339_opts(SecondsFormat::Micros, true),
        runtime_config_fingerprint_sha256: input.runtime_config_fingerprint_sha256,
        schedule_semantic_sha256: semantic_hash.clone(),
        schema_version: 3,
        semantic_identity,
        semantic_identity_contract_version: 1,
        semantic_revision: semantic_revision.to_string(),
        signature_ed25519_hex: String::new(),
        source_generation: "1".to_string(),
    };
    let signature_digest = stage8b_p1e_schedule_unsigned_signature_sha256(&envelope)
        .map_err(|_| Stage8bP1eSchedulePublisherError::InvalidInput)?;
    let digest = decode_fixed_hex::<32>(&signature_digest)?;
    let signature = signer
        .sign_raw_prehash(&digest)
        .map_err(|_| Stage8bP1eSchedulePublisherError::SigningFailed)?;
    verify_raw_signature(expected_public_key_hex, &signature, &digest)?;
    envelope.signature_ed25519_hex = encode_lower_hex(&signature);
    let exact_envelope = stage8b_p1e_canonical_json(&envelope)
        .map_err(|_| Stage8bP1eSchedulePublisherError::InvalidInput)?;
    let state = Stage8bP1eSchedulePublisherStateV1 {
        schema_version: 1,
        domain: STATE_DOMAIN.to_string(),
        phase: Stage8bP1eSchedulePublisherPhaseV1::Prepared,
        source_generation: "1".to_string(),
        publication_sequence: publication_sequence.to_string(),
        semantic_revision: semantic_revision.to_string(),
        schedule_semantic_sha256: semantic_hash,
        published_at_utc: envelope.published_at_utc,
        envelope_sha256: sha256_hex(&exact_envelope),
        exact_envelope_hex: encode_lower_hex(&exact_envelope),
        redis_stream_id: None,
    };
    validate_state(&state)?;
    Ok(state)
}

#[cfg(test)]
pub(crate) fn test_prepare_stage8b_p1e_schedule_publication_with_key(
    input: Stage8bP1eSchedulePublisherInputV1,
    lineage: Stage8bP1eSchedulePublisherLineage<'_>,
    signer: &impl Stage8bP1eScheduleSigner,
    expected_public_key_hex: &str,
) -> Result<Stage8bP1eSchedulePublisherStateV1, Stage8bP1eSchedulePublisherError> {
    prepare_stage8b_p1e_schedule_publication_with_key(
        input,
        lineage,
        signer,
        expected_public_key_hex,
        &|state| state.validate_with_fixture_key(expected_public_key_hex),
    )
}

pub fn load_stage8b_p1e_schedule_publisher_state(
    path: &Path,
) -> Result<Stage8bP1eSchedulePublisherStateV1, Stage8bP1eSchedulePublisherError> {
    load_stage8b_p1e_schedule_publisher_state_with_validator(
        path,
        &Stage8bP1eSchedulePublisherStateV1::validate,
    )
}

fn load_stage8b_p1e_schedule_publisher_state_with_validator(
    path: &Path,
    validate_state: &impl Fn(
        &Stage8bP1eSchedulePublisherStateV1,
    ) -> Result<(), Stage8bP1eSchedulePublisherError>,
) -> Result<Stage8bP1eSchedulePublisherStateV1, Stage8bP1eSchedulePublisherError> {
    let bytes = fs::read(path)?;
    let state: Stage8bP1eSchedulePublisherStateV1 = serde_json::from_slice(&bytes)
        .map_err(|_| Stage8bP1eSchedulePublisherError::DurableStateConflict)?;
    validate_state(&state)?;
    if stage8b_p1e_canonical_json(&state)
        .map_err(|_| Stage8bP1eSchedulePublisherError::DurableStateConflict)?
        != bytes
    {
        return Err(Stage8bP1eSchedulePublisherError::DurableStateConflict);
    }
    Ok(state)
}

pub fn persist_stage8b_p1e_schedule_publisher_state(
    path: &Path,
    state: &Stage8bP1eSchedulePublisherStateV1,
) -> Result<(), Stage8bP1eSchedulePublisherError> {
    persist_stage8b_p1e_schedule_publisher_state_with_validator(
        path,
        state,
        &Stage8bP1eSchedulePublisherStateV1::validate,
    )
}

fn persist_stage8b_p1e_schedule_publisher_state_with_validator(
    path: &Path,
    state: &Stage8bP1eSchedulePublisherStateV1,
    validate_state: &impl Fn(
        &Stage8bP1eSchedulePublisherStateV1,
    ) -> Result<(), Stage8bP1eSchedulePublisherError>,
) -> Result<(), Stage8bP1eSchedulePublisherError> {
    validate_state(state)?;
    let bytes = stage8b_p1e_canonical_json(state)
        .map_err(|_| Stage8bP1eSchedulePublisherError::DurableStateConflict)?;
    let parent = path
        .parent()
        .ok_or(Stage8bP1eSchedulePublisherError::DurableStateConflict)?;
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or(Stage8bP1eSchedulePublisherError::DurableStateConflict)?;
    let temporary = parent.join(format!(
        ".{name}.{}.{}.tmp",
        std::process::id(),
        TEMP_COUNTER.fetch_add(1, Ordering::SeqCst)
    ));
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temporary)?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    fs::rename(&temporary, path)?;
    File::open(parent)?.sync_all()?;
    Ok(())
}

#[async_trait]
pub trait Stage8bP1eScheduleStreamWriter {
    async fn xadd_nomkstream_maxlen_exact(
        &mut self,
        stream: &str,
        maxlen: usize,
        payload: &[u8],
    ) -> Result<String, Stage8bP1eSchedulePublisherError>;
}

pub struct Stage8bP1eRedisScheduleStreamWriter {
    connection: ConnectionManager,
}

/// Fixed P1F schedule-publisher role. It accepts only the exact DB15 endpoint,
/// stream and MAXLEN contract and retains a hash-only bounded command audit.
pub struct Stage8bP1fSchedulePublisherRedisV1 {
    writer: Stage8bP1eRedisScheduleStreamWriter,
    audit: runtime_durable_service::Stage8bP1fRedisCommandAuditV1,
}

impl Stage8bP1fSchedulePublisherRedisV1 {
    pub async fn connect(redis_url: &str) -> Result<Self, Stage8bP1eSchedulePublisherError> {
        if !matches!(
            redis_url,
            runtime_durable_service::STAGE8B_P1E_REDIS_URL_IPV4
                | runtime_durable_service::STAGE8B_P1E_REDIS_URL_IPV6
        ) {
            return Err(Stage8bP1eSchedulePublisherError::DurableStateConflict);
        }
        Ok(Self {
            writer: Stage8bP1eRedisScheduleStreamWriter::connect(redis_url).await?,
            audit: runtime_durable_service::Stage8bP1fRedisCommandAuditV1::default(),
        })
    }

    pub fn audit_records(
        &self,
    ) -> &std::collections::VecDeque<runtime_durable_service::Stage8bP1fRedisCommandAuditRecordV1>
    {
        self.audit.records()
    }
}

#[async_trait]
impl Stage8bP1eScheduleStreamWriter for Stage8bP1fSchedulePublisherRedisV1 {
    async fn xadd_nomkstream_maxlen_exact(
        &mut self,
        stream: &str,
        maxlen: usize,
        payload: &[u8],
    ) -> Result<String, Stage8bP1eSchedulePublisherError> {
        if stream != STAGE8B_P1E_SCHEDULE_STREAM || maxlen != MAXLEN {
            return Err(Stage8bP1eSchedulePublisherError::DurableStateConflict);
        }
        let result = self
            .writer
            .xadd_nomkstream_maxlen_exact(stream, maxlen, payload)
            .await;
        self.audit
            .record_auxiliary(
                runtime_durable_service::Stage8bP1fRedisRoleV1::SchedulePublisher,
                runtime_durable_service::Stage8bP1fRedisAuxiliaryOperationV1::SchedulePublication,
                None,
                format!(
                    "stream={stream};maxlen={maxlen};payload_sha256={}",
                    encode_lower_hex(&Sha256::digest(payload))
                )
                .as_bytes(),
                if result.is_ok() {
                    runtime_durable_service::Stage8bP1fRedisAuditResultV1::Succeeded
                } else {
                    runtime_durable_service::Stage8bP1fRedisAuditResultV1::Failed
                },
            )
            .map_err(|_| Stage8bP1eSchedulePublisherError::DurableStateConflict)?;
        result
    }
}

impl Stage8bP1eRedisScheduleStreamWriter {
    pub async fn connect(redis_url: &str) -> Result<Self, Stage8bP1eSchedulePublisherError> {
        let client = redis::Client::open(redis_url)?;
        Ok(Self {
            connection: ConnectionManager::new(client).await?,
        })
    }
}

#[async_trait]
impl Stage8bP1eScheduleStreamWriter for Stage8bP1eRedisScheduleStreamWriter {
    async fn xadd_nomkstream_maxlen_exact(
        &mut self,
        stream: &str,
        maxlen: usize,
        payload: &[u8],
    ) -> Result<String, Stage8bP1eSchedulePublisherError> {
        Ok(redis::cmd("XADD")
            .arg(stream)
            .arg("NOMKSTREAM")
            .arg("MAXLEN")
            .arg("=")
            .arg(maxlen)
            .arg("*")
            .arg("payload")
            .arg(payload)
            .query_async(&mut self.connection)
            .await?)
    }
}

/// Persists Prepared before Redis. A response-loss restart may safely replay
/// the same signed bytes; consumers treat equal sequence/equal bytes as
/// idempotent. No later publication can be prepared until Published is
/// durably present.
pub async fn publish_stage8b_p1e_prepared_schedule(
    path: &Path,
    prepared: Stage8bP1eSchedulePublisherStateV1,
    writer: &mut impl Stage8bP1eScheduleStreamWriter,
) -> Result<Stage8bP1eSchedulePublisherStateV1, Stage8bP1eSchedulePublisherError> {
    publish_stage8b_p1e_prepared_schedule_with_validator(
        path,
        prepared,
        writer,
        &Stage8bP1eSchedulePublisherStateV1::validate,
    )
    .await
}

async fn publish_stage8b_p1e_prepared_schedule_with_validator(
    path: &Path,
    mut prepared: Stage8bP1eSchedulePublisherStateV1,
    writer: &mut impl Stage8bP1eScheduleStreamWriter,
    validate_state: &impl Fn(
        &Stage8bP1eSchedulePublisherStateV1,
    ) -> Result<(), Stage8bP1eSchedulePublisherError>,
) -> Result<Stage8bP1eSchedulePublisherStateV1, Stage8bP1eSchedulePublisherError> {
    if prepared.phase != Stage8bP1eSchedulePublisherPhaseV1::Prepared {
        return Err(Stage8bP1eSchedulePublisherError::DurableStateConflict);
    }
    persist_stage8b_p1e_schedule_publisher_state_with_validator(path, &prepared, validate_state)?;
    let payload = prepared.exact_envelope_bytes()?;
    let redis_id = writer
        .xadd_nomkstream_maxlen_exact(STAGE8B_P1E_SCHEDULE_STREAM, MAXLEN, &payload)
        .await?;
    if !valid_redis_stream_id(&redis_id) {
        return Err(Stage8bP1eSchedulePublisherError::DurableStateConflict);
    }
    prepared.phase = Stage8bP1eSchedulePublisherPhaseV1::Published;
    prepared.redis_stream_id = Some(redis_id);
    persist_stage8b_p1e_schedule_publisher_state_with_validator(path, &prepared, validate_state)?;
    Ok(prepared)
}

#[cfg(test)]
pub(crate) async fn test_publish_stage8b_p1e_prepared_schedule_with_key(
    path: &Path,
    prepared: Stage8bP1eSchedulePublisherStateV1,
    writer: &mut impl Stage8bP1eScheduleStreamWriter,
    expected_public_key_hex: &str,
) -> Result<Stage8bP1eSchedulePublisherStateV1, Stage8bP1eSchedulePublisherError> {
    publish_stage8b_p1e_prepared_schedule_with_validator(path, prepared, writer, &|state| {
        state.validate_with_fixture_key(expected_public_key_hex)
    })
    .await
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
        && value != "0".repeat(64)
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn valid_redis_stream_id(value: &str) -> bool {
    let Some((ms, sequence)) = value.split_once('-') else {
        return false;
    };
    !sequence.contains('-')
        && valid_nonzero_decimal(ms)
        && !sequence.is_empty()
        && sequence.len() <= 20
        && (sequence.len() == 1 || sequence.as_bytes()[0] != b'0')
        && sequence.bytes().all(|byte| byte.is_ascii_digit())
        && sequence.parse::<u64>().is_ok()
}

fn verify_raw_signature(
    public_key_hex: &str,
    signature: &[u8; 64],
    digest: &[u8; 32],
) -> Result<(), Stage8bP1eSchedulePublisherError> {
    let public_key = decode_fixed_hex::<32>(public_key_hex)
        .map_err(|_| Stage8bP1eSchedulePublisherError::SigningAuthorityMismatch)?;
    let verifying_key = VerifyingKey::from_bytes(&public_key)
        .map_err(|_| Stage8bP1eSchedulePublisherError::SigningAuthorityMismatch)?;
    verifying_key
        .verify(digest, &Signature::from_bytes(signature))
        .map_err(|_| Stage8bP1eSchedulePublisherError::SigningAuthorityMismatch)
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn encode_lower_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn decode_lower_hex(value: &str) -> Result<Vec<u8>, Stage8bP1eSchedulePublisherError> {
    if value.is_empty()
        || value.len() % 2 != 0
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(Stage8bP1eSchedulePublisherError::DurableStateConflict);
    }
    (0..value.len())
        .step_by(2)
        .map(|index| {
            u8::from_str_radix(&value[index..index + 2], 16)
                .map_err(|_| Stage8bP1eSchedulePublisherError::DurableStateConflict)
        })
        .collect()
}

fn decode_fixed_hex<const N: usize>(
    value: &str,
) -> Result<[u8; N], Stage8bP1eSchedulePublisherError> {
    let bytes = decode_lower_hex(value)?;
    bytes
        .try_into()
        .map_err(|_| Stage8bP1eSchedulePublisherError::InvalidInput)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use broker_core::{
        BrokerAccountId, BrokerInstrumentSpec, BrokerKind, BrokerMarketSessionState, BrokerSymbol,
        BrokerTruthSnapshot, Exchange, InstrumentMapEntry, InternalSymbol, Market, Money,
        Stage4AdoptionDisposition, Stage4BootstrapEvidenceSourceStatusSection,
        Stage4BrokerTruthBootstrapInput, Stage4BrokerTruthFreshnessInput,
        Stage4BrokerTruthSafetyBoundary, Stage4BrokerTruthSourceStatus,
    };
    use ed25519_dalek::{Signer, SigningKey};
    use rust_decimal::Decimal;

    #[tokio::test]
    async fn id_fixed_schedule_role_rejects_non_db15_endpoint_before_connect() {
        assert!(matches!(
            Stage8bP1fSchedulePublisherRedisV1::connect("redis://127.0.0.1:6379/0").await,
            Err(Stage8bP1eSchedulePublisherError::DurableStateConflict)
        ));
    }

    pub(crate) struct FixtureSigner {
        key: SigningKey,
        public_key_hex: String,
    }

    impl FixtureSigner {
        pub(crate) fn new(seed: u8) -> Self {
            let key = SigningKey::from_bytes(&[seed; 32]);
            let public_key_hex = encode_lower_hex(&key.verifying_key().to_bytes());
            Self {
                key,
                public_key_hex,
            }
        }
    }

    impl Stage8bP1eScheduleSigner for FixtureSigner {
        fn public_key_ed25519_hex(&self) -> &str {
            &self.public_key_hex
        }

        fn sign_raw_prehash(
            &self,
            digest: &[u8; 32],
        ) -> Result<[u8; 64], Stage8bP1eSchedulePublisherError> {
            Ok(self.key.sign(digest).to_bytes())
        }
    }

    fn timestamp(value: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(value)
            .unwrap()
            .with_timezone(&Utc)
    }

    fn timestamp_text(value: DateTime<Utc>) -> String {
        value.to_rfc3339_opts(SecondsFormat::Micros, true)
    }

    fn fixture_payload(now: DateTime<Utc>, session_end: &str) -> Stage8bP1eSchedulePayloadV2 {
        let instrument = broker_core::InstrumentId {
            symbol: "IMOEXF".to_string(),
            venue_symbol: Some("IMOEXF@RTSX".to_string()),
            exchange: Exchange::Moex,
            market: Market::Futures,
        };
        let truth = BrokerTruthSnapshot {
            account_id: BrokerAccountId::new("ACC_TEST_0001"),
            orders: Vec::new(),
            positions: Vec::new(),
            cash: None,
            trades: Vec::new(),
            instruments: vec![BrokerInstrumentSpec {
                instrument: InstrumentMapEntry {
                    internal_symbol: InternalSymbol("IMOEXF".to_string()),
                    broker: BrokerKind::Finam,
                    broker_symbol: BrokerSymbol("IMOEXF@RTSX".to_string()),
                    exchange: Exchange::Moex,
                    market: Market::Futures,
                    price_step: Decimal::new(5, 1),
                    qty_step: Decimal::ONE,
                    lot_size: Decimal::ONE,
                    min_qty: Decimal::ONE,
                    step_value: Decimal::new(5, 0),
                    currency: "RUB".to_string(),
                    schedule_id: "RTSX".to_string(),
                    expiration_date: None,
                    is_tradable: true,
                },
                broker_asset_id: Some("ASSET_TEST_1".to_string()),
                board: Some("RTSX".to_string()),
                long_initial_margin: Some(Money::new(5_000, 0)),
                short_initial_margin: Some(Money::new(5_000, 0)),
            }],
            received_ts: now,
        };
        let validated = broker_core::stage4_bootstrap::validate_stage4_broker_truth_bootstrap(
            Stage4BrokerTruthBootstrapInput {
                broker_truth: &truth,
                broker_truth_source_status: Stage4BrokerTruthSourceStatus::Present,
                target_instrument: instrument,
                restored_runtime_state: None,
                freshness: Stage4BrokerTruthFreshnessInput::synthetic_all_sections_fresh_for_tests(
                    now, 60_000,
                ),
                schedule_state: BrokerMarketSessionState::Open,
                adoption: Stage4AdoptionDisposition::default(),
                external_issues: Vec::new(),
                safety_boundary: Stage4BrokerTruthSafetyBoundary::closed(),
                checked_ts: now,
            },
        );
        let sections = validated
            .freshness
            .sections
            .iter()
            .map(|section| Stage4BootstrapEvidenceSourceStatusSection {
                section: section.section,
                source_status: Stage4BrokerTruthSourceStatus::Present,
                required_for_bootstrap: section.required_for_bootstrap,
            })
            .collect::<Vec<_>>();
        let accepted = broker_core::stage4_bootstrap::build_stage4_accepted_paper_host_evidence(
            &validated, &sections,
        )
        .unwrap();
        let report = stage8b_p1e_canonical_json(accepted.report()).unwrap();
        let sessions = vec![strategy_runtime_core::Stage8bP1eScheduleSessionV1 {
            end_utc: session_end.to_string(),
            session_type: strategy_runtime_core::Stage8bP1eScheduleSessionTypeV1::TradableOpen,
            start_utc: "2026-09-14T06:00:00.000000Z".to_string(),
        }];
        Stage8bP1eSchedulePayloadV2 {
            domain: "moex.stage8b.p1e.schedule-payload.v2".to_string(),
            instrument: strategy_runtime_core::Stage8bP1eScheduleInstrumentV1 {
                board: "FUT".to_string(),
                broker_symbol: "IMOEXF@RTSX".to_string(),
                exchange: "moex".to_string(),
                market: "futures".to_string(),
                symbol: "IMOEXF".to_string(),
                tick_size: "0.5".to_string(),
                venue_mic: "RTSX".to_string(),
            },
            normalized_schedule: strategy_runtime_core::Stage8bP1eNormalizedScheduleV2 {
                normalized_payload_sha256: sha256_hex(
                    &stage8b_p1e_canonical_json(&sessions).unwrap(),
                ),
                raw_response_sha256: "1".repeat(64),
                sessions,
                source_expires_at_utc: timestamp_text(now + chrono::Duration::seconds(60)),
                source_observed_at_utc: timestamp_text(now),
            },
            registry: strategy_runtime_core::Stage8bP1eScheduleRegistryV1 {
                registry_identity_sha256: "2".repeat(64),
                registry_version: "imoexf-v1".to_string(),
            },
            schema_version: 2,
            stage4_evidence: strategy_runtime_core::Stage8bP1eStage4EvidenceV2 {
                boundary_proof: None,
                evidence_kind: strategy_runtime_core::Stage8bP1eScheduleEvidenceKindV1::Tradability,
                report_canonical_json_hex: encode_lower_hex(&report),
                report_sha256: sha256_hex(&report),
                schedule_state: strategy_runtime_core::Stage8bP1eScheduleStateV1::Open,
                source_expires_at_utc: timestamp_text(accepted.required_source_expires_at()),
                source_observed_at_utc: timestamp_text(now),
            },
            timezone: "Europe/Moscow".to_string(),
            trading_day: "2026-09-14".to_string(),
        }
    }

    pub(crate) fn fixture_input(
        now: DateTime<Utc>,
        session_end: &str,
    ) -> Stage8bP1eSchedulePublisherInputV1 {
        Stage8bP1eSchedulePublisherInputV1 {
            payload: fixture_payload(now, session_end),
            operational_identity_sha256: "4".repeat(64),
            runtime_config_fingerprint_sha256: "5".repeat(64),
            instrument_map_fingerprint_sha256: "3".repeat(64),
            published_at_utc: now,
        }
    }

    fn close_fixture_input(
        now: DateTime<Utc>,
        session_end: &str,
    ) -> Stage8bP1eSchedulePublisherInputV1 {
        let mut input = fixture_input(now, session_end);
        let boundary = timestamp(session_end);
        input.payload.stage4_evidence.evidence_kind =
            strategy_runtime_core::Stage8bP1eScheduleEvidenceKindV1::DayBoundary;
        input.payload.stage4_evidence.schedule_state =
            strategy_runtime_core::Stage8bP1eScheduleStateV1::Closed;
        input.payload.stage4_evidence.boundary_proof =
            Some(strategy_runtime_core::Stage8bP1eDayBoundaryProofV1 {
                boundary_ts_utc: timestamp_text(boundary),
                last_eligible_m10_close_ts_utc: timestamp_text(boundary),
                last_eligible_m10_open_ts_utc: timestamp_text(
                    boundary - chrono::Duration::seconds(600),
                ),
                trading_day: input.payload.trading_day.clone(),
            });
        input
    }

    fn prepare_fixture(
        input: Stage8bP1eSchedulePublisherInputV1,
        lineage: Stage8bP1eSchedulePublisherLineage<'_>,
        signer: &FixtureSigner,
    ) -> Result<Stage8bP1eSchedulePublisherStateV1, Stage8bP1eSchedulePublisherError> {
        prepare_stage8b_p1e_schedule_publication_with_key(
            input,
            lineage,
            signer,
            &signer.public_key_hex,
            &|state| state.validate_with_fixture_key(&signer.public_key_hex),
        )
    }

    #[derive(Default)]
    pub(crate) struct FixtureWriter {
        calls: Vec<(String, usize, Vec<u8>)>,
        fail_after_write: bool,
    }

    #[async_trait]
    impl Stage8bP1eScheduleStreamWriter for FixtureWriter {
        async fn xadd_nomkstream_maxlen_exact(
            &mut self,
            stream: &str,
            maxlen: usize,
            payload: &[u8],
        ) -> Result<String, Stage8bP1eSchedulePublisherError> {
            self.calls
                .push((stream.to_string(), maxlen, payload.to_vec()));
            if self.fail_after_write {
                return Err(Stage8bP1eSchedulePublisherError::Redis(
                    redis::RedisError::from((redis::ErrorKind::IoError, "fixture response loss")),
                ));
            }
            Ok("1789387800001-0".to_string())
        }
    }

    fn fixture_state_validator<'a>(
        signer: &'a FixtureSigner,
    ) -> impl Fn(&Stage8bP1eSchedulePublisherStateV1) -> Result<(), Stage8bP1eSchedulePublisherError> + 'a
    {
        |state| state.validate_with_fixture_key(&signer.public_key_hex)
    }

    fn temporary_state_path() -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!(
            "stage8b-p1e-publisher-{}-{}",
            std::process::id(),
            TEMP_COUNTER.fetch_add(1, Ordering::SeqCst)
        ));
        fs::create_dir_all(&root).unwrap();
        root.join("publisher-state.json")
    }

    pub(crate) fn fixture_adapter_input(
        now: DateTime<Utc>,
    ) -> Stage8bP1eReadonlyScheduleAdapterInputV1 {
        let fixture = fixture_payload(now, "2026-09-14T15:50:00.000000Z");
        let report_bytes = decode_lower_hex(&fixture.stage4_evidence.report_canonical_json_hex)
            .expect("fixture Stage4 bytes");
        let stage4_report = serde_json::from_slice(&report_bytes).expect("fixture Stage4 report");
        let schedule = broker_finam::AssetScheduleResponse {
            sessions: vec![broker_finam::ScheduleSession {
                interval: Some(broker_finam::TimeInterval {
                    start_time: Some("2026-09-14T09:00:00+03:00".to_string()),
                    end_time: Some("2026-09-14T18:50:00+03:00".to_string()),
                }),
                session_type: Some("SESSION_TYPE_MAIN".to_string()),
            }],
            symbol: "IMOEXF@RTSX".to_string(),
        };
        let exact_finam_response_bytes = serde_json::to_vec(&schedule).unwrap();
        Stage8bP1eReadonlyScheduleAdapterInputV1 {
            exact_finam_response_bytes,
            schedule,
            stage4_report,
            registry_entry: InstrumentMapEntry {
                internal_symbol: InternalSymbol("IMOEXF".to_string()),
                broker: BrokerKind::Finam,
                broker_symbol: BrokerSymbol("IMOEXF@RTSX".to_string()),
                exchange: Exchange::Moex,
                market: Market::Futures,
                price_step: Decimal::new(5, 1),
                qty_step: Decimal::ONE,
                lot_size: Decimal::ONE,
                min_qty: Decimal::ONE,
                step_value: Decimal::new(5, 0),
                currency: "RUB".to_string(),
                schedule_id: "MOEX_FUT".to_string(),
                expiration_date: None,
                is_tradable: true,
            },
            registry_version: "imoexf-v1".to_string(),
            registry_identity_sha256: "2".repeat(64),
            operational_identity_sha256: "4".repeat(64),
            runtime_config_fingerprint_sha256: "5".repeat(64),
            instrument_map_fingerprint_sha256: "3".repeat(64),
            source_observed_at_utc: now,
            published_at_utc: now,
            evidence_kind: strategy_runtime_core::Stage8bP1eScheduleEvidenceKindV1::Tradability,
            schedule_state: strategy_runtime_core::Stage8bP1eScheduleStateV1::Open,
            boundary_proof: None,
        }
    }

    #[test]
    fn readonly_adapter_binds_exact_finam_response_stage4_and_registry() {
        let now = timestamp("2026-09-14T12:10:00.000000Z");
        let input = fixture_adapter_input(now);
        let raw_sha256 = sha256_hex(&input.exact_finam_response_bytes);
        let mapped = adapt_stage8b_p1e_readonly_schedule(input).unwrap();
        assert_eq!(mapped.payload.trading_day, "2026-09-14");
        assert_eq!(mapped.payload.instrument.broker_symbol, "IMOEXF@RTSX");
        assert_eq!(
            mapped.payload.normalized_schedule.raw_response_sha256,
            raw_sha256
        );
        assert_eq!(mapped.payload.normalized_schedule.sessions.len(), 1);
        assert_eq!(
            mapped.payload.normalized_schedule.sessions[0].start_utc,
            "2026-09-14T06:00:00.000000Z"
        );
        assert_eq!(
            mapped.payload.normalized_schedule.sessions[0].session_type,
            strategy_runtime_core::Stage8bP1eScheduleSessionTypeV1::TradableOpen
        );
    }

    #[test]
    fn readonly_adapter_derives_trading_day_from_moscow_sessions_not_observation_utc_day() {
        let observed = timestamp("2026-09-13T21:00:00.000000Z");
        let mut input = fixture_adapter_input(observed);
        input.published_at_utc = observed + chrono::Duration::milliseconds(1);
        let mapped = adapt_stage8b_p1e_readonly_schedule(input).unwrap();
        assert_eq!(mapped.payload.trading_day, "2026-09-14");
    }

    #[test]
    fn readonly_adapter_rejects_unknown_session_and_detached_raw_response() {
        let now = timestamp("2026-09-14T12:10:00.000000Z");
        let mut unknown = fixture_adapter_input(now);
        unknown.schedule.sessions[0].session_type = Some("SESSION_TYPE_UNSPECIFIED".to_string());
        unknown.exact_finam_response_bytes = serde_json::to_vec(&unknown.schedule).unwrap();
        assert!(matches!(
            adapt_stage8b_p1e_readonly_schedule(unknown),
            Err(Stage8bP1eSchedulePublisherError::InvalidInput)
        ));

        let mut detached = fixture_adapter_input(now);
        detached.schedule.sessions[0]
            .interval
            .as_mut()
            .unwrap()
            .end_time = Some("2026-09-14T19:00:00+03:00".to_string());
        assert!(matches!(
            adapt_stage8b_p1e_readonly_schedule(detached),
            Err(Stage8bP1eSchedulePublisherError::InvalidInput)
        ));

        let mut ambiguous = fixture_adapter_input(now);
        ambiguous
            .schedule
            .sessions
            .push(broker_finam::ScheduleSession {
                interval: Some(broker_finam::TimeInterval {
                    start_time: Some("2026-09-14T18:50:00+03:00".to_string()),
                    end_time: Some("2026-09-14T19:00:00+03:00".to_string()),
                }),
                session_type: Some("SESSION_TYPE_CLEARING".to_string()),
            });
        ambiguous.exact_finam_response_bytes = serde_json::to_vec(&ambiguous.schedule).unwrap();
        assert!(matches!(
            adapt_stage8b_p1e_readonly_schedule(ambiguous),
            Err(Stage8bP1eSchedulePublisherError::InvalidInput)
        ));
    }

    #[test]
    fn inclusive_session_endpoints_reject_adjacency_and_overlap() {
        let now = timestamp("2026-09-14T12:10:00.000000Z");
        for start_time in ["2026-09-14T18:49:59+03:00", "2026-09-14T18:50:00+03:00"] {
            let mut input = fixture_adapter_input(now);
            input.schedule.sessions.push(broker_finam::ScheduleSession {
                interval: Some(broker_finam::TimeInterval {
                    start_time: Some(start_time.to_string()),
                    end_time: Some("2026-09-14T19:10:00+03:00".to_string()),
                }),
                session_type: Some("SESSION_TYPE_CLEARING".to_string()),
            });
            input.exact_finam_response_bytes = serde_json::to_vec(&input.schedule).unwrap();
            assert!(matches!(
                adapt_stage8b_p1e_readonly_schedule(input),
                Err(Stage8bP1eSchedulePublisherError::InvalidInput)
            ));
        }
    }

    #[test]
    fn first_publication_requires_exact_explicit_confirmation() {
        assert!(matches!(
            authorize_stage8b_p1e_first_publication("close-enough"),
            Err(Stage8bP1eSchedulePublisherError::DurableStateConflict)
        ));
        assert!(authorize_stage8b_p1e_first_publication(FIRST_PUBLICATION_CONFIRMATION).is_ok());
    }

    #[test]
    fn signer_output_is_cryptographically_bound_to_the_exact_digest() {
        let key = SigningKey::from_bytes(&[0x61; 32]);
        let digest = [0x22; 32];
        let signature = key.sign(&digest).to_bytes();
        let public_key = encode_lower_hex(&key.verifying_key().to_bytes());
        assert!(verify_raw_signature(&public_key, &signature, &digest).is_ok());

        let mut changed_digest = digest;
        changed_digest[0] ^= 1;
        assert!(matches!(
            verify_raw_signature(&public_key, &signature, &changed_digest),
            Err(Stage8bP1eSchedulePublisherError::SigningAuthorityMismatch)
        ));
    }

    #[test]
    fn publisher_hashes_and_stream_ids_are_strictly_canonical() {
        assert!(!valid_sha256(&"0".repeat(64)));
        assert!(valid_sha256(&"a".repeat(64)));
        assert!(valid_redis_stream_id("1789387800000-0"));
        assert!(!valid_redis_stream_id("01789387800000-0"));
        assert!(!valid_redis_stream_id("1789387800000-00"));
        assert!(!valid_redis_stream_id("1789387800000-x"));
        assert!(!valid_redis_stream_id("1789387800000-18446744073709551616"));
    }

    #[test]
    fn fixture_signed_publications_preserve_sequence_and_semantic_revision_rules() {
        let signer = FixtureSigner::new(0x63);
        let first_now = timestamp("2026-09-14T12:10:00.000000Z");
        let first = prepare_fixture(
            fixture_input(first_now, "2026-09-14T12:20:00.000000Z"),
            Stage8bP1eSchedulePublisherLineage::First(
                authorize_stage8b_p1e_first_publication(FIRST_PUBLICATION_CONFIRMATION).unwrap(),
            ),
            &signer,
        )
        .unwrap();
        assert_eq!(first.phase(), Stage8bP1eSchedulePublisherPhaseV1::Prepared);
        assert_eq!(first.publication_sequence(), 1);
        assert_eq!(first.semantic_revision(), 1);
        first
            .validate_with_fixture_key(&signer.public_key_hex)
            .unwrap();

        let mut published = first.clone();
        published.phase = Stage8bP1eSchedulePublisherPhaseV1::Published;
        published.redis_stream_id = Some("1789387800001-0".to_string());
        published
            .validate_with_fixture_key(&signer.public_key_hex)
            .unwrap();

        let heartbeat = prepare_fixture(
            fixture_input(
                first_now + chrono::Duration::seconds(1),
                "2026-09-14T12:20:00.000000Z",
            ),
            Stage8bP1eSchedulePublisherLineage::Resume(&published),
            &signer,
        )
        .unwrap();
        assert_eq!(heartbeat.publication_sequence(), 2);
        assert_eq!(heartbeat.semantic_revision(), 1);
        assert_eq!(
            heartbeat.schedule_semantic_sha256(),
            published.schedule_semantic_sha256()
        );

        let mut heartbeat_published = heartbeat;
        heartbeat_published.phase = Stage8bP1eSchedulePublisherPhaseV1::Published;
        heartbeat_published.redis_stream_id = Some("1789387800002-0".to_string());
        let changed = prepare_fixture(
            fixture_input(
                first_now + chrono::Duration::seconds(2),
                "2026-09-14T12:30:00.000000Z",
            ),
            Stage8bP1eSchedulePublisherLineage::Resume(&heartbeat_published),
            &signer,
        )
        .unwrap();
        assert_eq!(changed.publication_sequence(), 3);
        assert_eq!(changed.semantic_revision(), 2);
        assert_ne!(
            changed.schedule_semantic_sha256(),
            heartbeat_published.schedule_semantic_sha256()
        );
    }

    #[test]
    fn publisher_open_to_closed_changes_revision_with_unchanged_sessions() {
        let signer = FixtureSigner::new(0x65);
        let first_now = timestamp("2026-09-14T12:10:00.000000Z");
        let first = prepare_fixture(
            fixture_input(first_now, "2026-09-14T12:20:00.000000Z"),
            Stage8bP1eSchedulePublisherLineage::First(
                authorize_stage8b_p1e_first_publication(FIRST_PUBLICATION_CONFIRMATION).unwrap(),
            ),
            &signer,
        )
        .unwrap();
        let mut published = first;
        published.phase = Stage8bP1eSchedulePublisherPhaseV1::Published;
        published.redis_stream_id = Some("1789387800001-0".to_string());
        let closed = prepare_fixture(
            close_fixture_input(
                first_now + chrono::Duration::seconds(1),
                "2026-09-14T12:20:00.000000Z",
            ),
            Stage8bP1eSchedulePublisherLineage::Resume(&published),
            &signer,
        )
        .unwrap();
        assert_eq!(closed.publication_sequence(), 2);
        assert_eq!(closed.semantic_revision(), 2);
        assert_ne!(
            closed.schedule_semantic_sha256(),
            published.schedule_semantic_sha256()
        );
        closed
            .validate_with_fixture_key(&signer.public_key_hex)
            .unwrap();
    }

    #[tokio::test]
    async fn prepared_state_survives_response_loss_and_replays_exact_signed_bytes() {
        let signer = FixtureSigner::new(0x64);
        let validator = fixture_state_validator(&signer);
        let path = temporary_state_path();
        let prepared = prepare_fixture(
            fixture_input(
                timestamp("2026-09-14T12:10:00.000000Z"),
                "2026-09-14T12:20:00.000000Z",
            ),
            Stage8bP1eSchedulePublisherLineage::First(
                authorize_stage8b_p1e_first_publication(FIRST_PUBLICATION_CONFIRMATION).unwrap(),
            ),
            &signer,
        )
        .unwrap();
        let exact_envelope = prepared.exact_envelope_bytes().unwrap();

        let mut response_loss = FixtureWriter {
            fail_after_write: true,
            ..FixtureWriter::default()
        };
        assert!(publish_stage8b_p1e_prepared_schedule_with_validator(
            &path,
            prepared,
            &mut response_loss,
            &validator,
        )
        .await
        .is_err());
        assert_eq!(response_loss.calls.len(), 1);
        assert_eq!(response_loss.calls[0].0, STAGE8B_P1E_SCHEDULE_STREAM);
        assert_eq!(response_loss.calls[0].1, MAXLEN);
        assert_eq!(response_loss.calls[0].2, exact_envelope);

        let retained =
            load_stage8b_p1e_schedule_publisher_state_with_validator(&path, &validator).unwrap();
        assert_eq!(
            retained.phase(),
            Stage8bP1eSchedulePublisherPhaseV1::Prepared
        );
        assert_eq!(retained.exact_envelope_bytes().unwrap(), exact_envelope);

        let mut retry = FixtureWriter::default();
        let published = publish_stage8b_p1e_prepared_schedule_with_validator(
            &path, retained, &mut retry, &validator,
        )
        .await
        .unwrap();
        assert_eq!(retry.calls.len(), 1);
        assert_eq!(retry.calls[0].2, exact_envelope);
        assert_eq!(
            published.phase(),
            Stage8bP1eSchedulePublisherPhaseV1::Published
        );
        assert_eq!(published.redis_stream_id(), Some("1789387800001-0"));
        let reread =
            load_stage8b_p1e_schedule_publisher_state_with_validator(&path, &validator).unwrap();
        assert_eq!(reread, published);
        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }
}
