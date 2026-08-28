//! Stage 8B-P R2A7 fixed-root, one-shot recovery reader.
//!
//! This module has no FINAM client, credential, arm, dispatch, effect, Redis or
//! background-loop dependency.  It reconstructs the accepted durable owner,
//! derives the sole current Stage 6 request from replay and publishes only the
//! already-reviewed read-only operational records.

use crate::stage8a1_execution_capability::{
    publish_stage8b_r2a7_operational_sources_from_owner, Stage8a1AcceptedExecutionConfigV1,
    Stage8a1OperationalAuthorityIssuer, Stage8a1TrustedCurrentSources,
    Stage8bR2a5SourcePublicationEvidence, STAGE8B_R2A6_SOURCE_ADAPTER_UID,
};
use broker_core::{BrokerReadinessSnapshot, BrokerTruthSnapshot, StrategyRequestId};
use chrono::{Duration, NaiveTime, TimeZone, Timelike, Utc};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use runtime_durable_service::{
    Stage7bCompositeReadinessSnapshot, Stage7bDurableRootAuthority, Stage7bPaperReadinessPhase,
    Stage7bPaperReadinessReason, Stage7bRecoveryReadyOwner, Stage7bRestartOutcome,
};
use serde::{Deserialize, Serialize};
use std::fs::OpenOptions;
use std::io::Read;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use strategy_runtime_core::hybrid_intraday::{
    BreakoutEodMode, HybridOrchestratorConfig, IntradayBreakoutConfig, MeanReversionConfig,
    MinRangeMode,
};
use strategy_runtime_core::{
    BrokerNeutralMarketOrderStyle, HybridIntradayProfile, HybridIntradayRuntimeConfig,
    HybridIntradayRuntimeStrategy, MeanReversionVariant, MrGatePolicy, RiskGateMode,
    Stage5gLifecycleCommitmentKey, Stage6DurableCommandSnapshotV1, Stage6DurableRequestIdentityV1,
    Stage6dOperationalIdentityConfig,
};

const MANIFEST_FILE: &str = "stage8b-r2a7-reader-manifest.json";
const TRUSTED_CURRENT_SOURCE_FILE: &str = "stage8b-r2a8-trusted-current-source.json";
const PRODUCTION_WRITER_INTAKE_FILE: &str = "stage8b-r2a8-production-writer-intake.json";
const PRODUCTION_OWNER_SIGNED_INTAKE_FILE: &str = "stage8b-r2a8-owner-signed-intake.json";
const LIFECYCLE_KEY_FILE: &str = "stage8b-r2a7-lifecycle-key.hex";
const PRODUCTION_WORK_ROOT: &str = "/var/lib/moex-trading/stage8b/r2a7/production";
const PRODUCTION_CURRENT_SOURCE_ROOT: &str = "/var/lib/moex-trading/stage8b/r2a8/current-source";
const PRODUCTION_INTAKE_ROOT: &str = "/var/lib/moex-trading/stage8b/r2a8/intake";
const PRODUCTION_STAGE7B_PARENT: &str = "/var/lib/moex-trading/stage7b";
const PRODUCTION_AUTHORITY_ROOT: &str = "/var/lib/moex-trading/stage8a1-authority";
const PRODUCTION_OUTPUT_ROOT: &str = "/var/lib/moex-trading/operational-authorities";
const CONTROLLED_ROOT: &str = "/var/lib/moex-trading/stage8b/r2a7-controlled";
const RUNTIME_PROFILE_ID: &str = "imoexf-stage5ge-c-normal-append-v1";
const MAX_MANIFEST_BYTES: u64 = 2 * 1024 * 1024;
const MAX_KEY_BYTES: u64 = 256;
const MAX_CURRENT_SOURCE_TTL_SECONDS: i64 = 30;
const CURRENT_SOURCE_COMMITMENT_DOMAIN: &[u8] =
    b"stage8b-r2a8-r1-trusted-current-source-commitment-v2";
const PRODUCTION_WRITER_INTAKE_COMMITMENT_DOMAIN: &[u8] =
    b"stage8b-r2b-proposal-r1-production-writer-intake-v1";
const ACCEPTED_EXECUTION_CONFIG_FILE: &str = "stage8a1-accepted-execution-config.json";
const ACCEPTED_EXECUTION_CONFIG_SHA256_FILE: &str =
    "stage8a1-accepted-execution-config.json.sha256";
pub const STAGE8B_R2A8_CURRENT_SOURCE_INPUT_UID: u32 = 8094;
pub const STAGE8B_R2A8_CURRENT_MANIFEST_ISSUER_UID: u32 = 8096;
const STAGE8B_R2A8_LIFECYCLE_KEY_GID: u32 = 8095;
const STAGE8B_R2A8_LIFECYCLE_KEY_MODE: u32 = 0o640;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage8bR2a7RunMode {
    Production,
    ControlledPlace,
    ControlledCancel,
}

impl Stage8bR2a7RunMode {
    fn adapter_domain(self) -> &'static str {
        match self {
            Self::Production => "production",
            Self::ControlledPlace | Self::ControlledCancel => "controlled_qualification",
        }
    }

    fn layout(self) -> Stage8bR2a7FixedLayout {
        match self {
            Self::Production => Stage8bR2a7FixedLayout {
                work_root: PathBuf::from(PRODUCTION_WORK_ROOT),
                current_source_root: PathBuf::from(PRODUCTION_CURRENT_SOURCE_ROOT),
                stage7b_parent: PathBuf::from(PRODUCTION_STAGE7B_PARENT),
                authority_root: PathBuf::from(PRODUCTION_AUTHORITY_ROOT),
                output_root: PathBuf::from(PRODUCTION_OUTPUT_ROOT),
            },
            Self::ControlledPlace | Self::ControlledCancel => {
                let operation = if self == Self::ControlledPlace {
                    "place"
                } else {
                    "cancel"
                };
                let base = Path::new(CONTROLLED_ROOT).join(operation);
                Stage8bR2a7FixedLayout {
                    work_root: base.join("manifest"),
                    current_source_root: base.join("current-source"),
                    stage7b_parent: base.join("stage7b"),
                    authority_root: base.join("stage8a1-authority"),
                    output_root: base.join("operational-authorities"),
                }
            }
        }
    }
}

struct Stage8bR2a7FixedLayout {
    work_root: PathBuf,
    current_source_root: PathBuf,
    stage7b_parent: PathBuf,
    authority_root: PathBuf,
    output_root: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stage8bR2a8TrustedCurrentSourceV2 {
    pub schema_version: u16,
    pub adapter_domain: String,
    pub adapter_mode: String,
    pub source_generation: u64,
    pub source_observed_at: chrono::DateTime<Utc>,
    pub expires_at: chrono::DateTime<Utc>,
    pub runtime_profile_id: String,
    pub operational_identity: Stage6dOperationalIdentityConfig,
    pub accepted_config_sha256: String,
    pub composite_readiness: Stage8bCompositeReadinessAuthorityV1,
    pub broker_truth: BrokerTruthSnapshot,
    pub broker_readiness: BrokerReadinessSnapshot,
    pub current_source_commitment_sha256: String,
    pub current_source_issuer_public_key_hex: String,
    pub current_source_signature_ed25519_hex: String,
}

/// Signed fixed-path intake consumed by the dedicated production writer. It
/// is source data, not an authority constructor: the writer independently
/// restores the Stage7B owner, pins the Stage8A config/key and revalidates the
/// exact durable request before the owner-mediated seam can be called.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stage8bR2a8ProductionWriterIntakeV1 {
    pub schema_version: u16,
    pub adapter_domain: String,
    pub runtime_profile_id: String,
    pub operational_identity: Stage6dOperationalIdentityConfig,
    pub accepted_config_sha256: String,
    pub durable_request_identity: Stage6DurableRequestIdentityV1,
    pub durable_command: Stage6DurableCommandSnapshotV1,
    pub composite_readiness: Stage8bCompositeReadinessAuthorityV1,
    pub broker_truth: BrokerTruthSnapshot,
    pub broker_readiness: BrokerReadinessSnapshot,
    pub observed_at: chrono::DateTime<Utc>,
    pub expires_at: chrono::DateTime<Utc>,
    pub intake_commitment_sha256: String,
    pub issuer_public_key_ed25519_hex: String,
    pub issuer_signature_ed25519_hex: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Stage8bR2a8ProductionWriterEvidence {
    pub schema_version: u16,
    pub writer: &'static str,
    pub fixed_input: &'static str,
    pub fixed_output: &'static str,
    pub durable_request_identity_sha256: String,
    pub trusted_current_source_sha256: String,
    pub network_accessed: bool,
    pub finam_credential_accessed: bool,
    pub caller_supplied_path_accepted: bool,
    pub caller_supplied_snapshot_accepted: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Stage8bR2a8ProductionIntakeStagerEvidence {
    pub schema_version: u16,
    pub stager: &'static str,
    pub fixed_owner_input: &'static str,
    pub fixed_writer_output: &'static str,
    pub intake_commitment_sha256: String,
    pub source_owner_uid: u32,
    pub output_owner_uid: u32,
    pub network_accessed: bool,
    pub finam_credential_accessed: bool,
    pub caller_supplied_input_accepted: bool,
}

#[derive(Debug, Clone, Serialize)]
#[allow(dead_code, reason = "R2B intake creator remains closed until issuance")]
pub struct Stage8bR2a8AuthoritativeIntakeCreatorEvidence {
    pub schema_version: u16,
    pub service: &'static str,
    pub fixed_output: &'static str,
    pub intake_commitment_sha256: String,
    pub opaque_current_sources_required: bool,
    pub recovered_owner_required: bool,
    pub caller_supplied_json_accepted: bool,
    pub caller_supplied_timestamp_accepted: bool,
    pub network_accessed: bool,
    pub finam_credential_accessed: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stage8bR2a7ReaderManifestV2 {
    pub schema_version: u16,
    pub adapter_domain: String,
    pub runtime_profile_id: String,
    pub operational_identity: Stage6dOperationalIdentityConfig,
    pub accepted_config_sha256: String,
    pub composite_readiness: Stage8bCompositeReadinessAuthorityV1,
    pub broker_truth: BrokerTruthSnapshot,
    pub broker_readiness: BrokerReadinessSnapshot,
    pub source_generation: u64,
    pub source_observed_at: chrono::DateTime<Utc>,
    pub expires_at: chrono::DateTime<Utc>,
    pub current_source_commitment_sha256: String,
    pub current_source_issuer_public_key_hex: String,
    pub current_source_signature_ed25519_hex: String,
    pub manifest_hmac_sha256: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage8bCompositeReadinessPhaseV1 {
    PaperReady,
    Degraded,
    Stopped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage8bCompositeReadinessReasonV1 {
    ConsumerNotAlive,
    StorageUnavailable,
    SourcePollStale,
    ClaimScanStale,
    SettlementUnavailable,
    DurablePendingEntries,
    CommandLifecycleBlocked,
}

/// Canonical readiness authority carried unchanged across the signed source
/// and HMAC-bound reader manifest. It deliberately contains the verdict and
/// every blocker, rather than only the observation timestamp.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stage8bCompositeReadinessAuthorityV1 {
    pub phase: Stage8bCompositeReadinessPhaseV1,
    pub reasons: Vec<Stage8bCompositeReadinessReasonV1>,
    pub blocked_entry_ids: Vec<String>,
    pub blocked_request_ids: Vec<StrategyRequestId>,
    pub checked_at: chrono::DateTime<Utc>,
}

impl Stage8bCompositeReadinessAuthorityV1 {
    fn from_snapshot(snapshot: &Stage7bCompositeReadinessSnapshot) -> Self {
        Self {
            phase: match snapshot.phase {
                Stage7bPaperReadinessPhase::PaperReady => {
                    Stage8bCompositeReadinessPhaseV1::PaperReady
                }
                Stage7bPaperReadinessPhase::Degraded => Stage8bCompositeReadinessPhaseV1::Degraded,
                Stage7bPaperReadinessPhase::Stopped => Stage8bCompositeReadinessPhaseV1::Stopped,
            },
            reasons: snapshot
                .reasons
                .iter()
                .map(|reason| match reason {
                    Stage7bPaperReadinessReason::ConsumerNotAlive => {
                        Stage8bCompositeReadinessReasonV1::ConsumerNotAlive
                    }
                    Stage7bPaperReadinessReason::StorageUnavailable => {
                        Stage8bCompositeReadinessReasonV1::StorageUnavailable
                    }
                    Stage7bPaperReadinessReason::SourcePollStale => {
                        Stage8bCompositeReadinessReasonV1::SourcePollStale
                    }
                    Stage7bPaperReadinessReason::ClaimScanStale => {
                        Stage8bCompositeReadinessReasonV1::ClaimScanStale
                    }
                    Stage7bPaperReadinessReason::SettlementUnavailable => {
                        Stage8bCompositeReadinessReasonV1::SettlementUnavailable
                    }
                    Stage7bPaperReadinessReason::DurablePendingEntries => {
                        Stage8bCompositeReadinessReasonV1::DurablePendingEntries
                    }
                    Stage7bPaperReadinessReason::CommandLifecycleBlocked => {
                        Stage8bCompositeReadinessReasonV1::CommandLifecycleBlocked
                    }
                })
                .collect(),
            blocked_entry_ids: snapshot.blocked_entry_ids.clone(),
            blocked_request_ids: snapshot.blocked_request_ids.clone(),
            checked_at: snapshot.checked_at,
        }
    }

    fn to_snapshot(&self) -> Stage7bCompositeReadinessSnapshot {
        Stage7bCompositeReadinessSnapshot {
            phase: match self.phase {
                Stage8bCompositeReadinessPhaseV1::PaperReady => {
                    Stage7bPaperReadinessPhase::PaperReady
                }
                Stage8bCompositeReadinessPhaseV1::Degraded => Stage7bPaperReadinessPhase::Degraded,
                Stage8bCompositeReadinessPhaseV1::Stopped => Stage7bPaperReadinessPhase::Stopped,
            },
            reasons: self
                .reasons
                .iter()
                .map(|reason| match reason {
                    Stage8bCompositeReadinessReasonV1::ConsumerNotAlive => {
                        Stage7bPaperReadinessReason::ConsumerNotAlive
                    }
                    Stage8bCompositeReadinessReasonV1::StorageUnavailable => {
                        Stage7bPaperReadinessReason::StorageUnavailable
                    }
                    Stage8bCompositeReadinessReasonV1::SourcePollStale => {
                        Stage7bPaperReadinessReason::SourcePollStale
                    }
                    Stage8bCompositeReadinessReasonV1::ClaimScanStale => {
                        Stage7bPaperReadinessReason::ClaimScanStale
                    }
                    Stage8bCompositeReadinessReasonV1::SettlementUnavailable => {
                        Stage7bPaperReadinessReason::SettlementUnavailable
                    }
                    Stage8bCompositeReadinessReasonV1::DurablePendingEntries => {
                        Stage7bPaperReadinessReason::DurablePendingEntries
                    }
                    Stage8bCompositeReadinessReasonV1::CommandLifecycleBlocked => {
                        Stage7bPaperReadinessReason::CommandLifecycleBlocked
                    }
                })
                .collect(),
            blocked_entry_ids: self.blocked_entry_ids.clone(),
            blocked_request_ids: self.blocked_request_ids.clone(),
            checked_at: self.checked_at,
        }
    }

    fn validate_ready(&self) -> Result<(), Stage8bR2a7SourceAdapterError> {
        if self.phase != Stage8bCompositeReadinessPhaseV1::PaperReady
            || !self.reasons.is_empty()
            || !self.blocked_entry_ids.is_empty()
            || !self.blocked_request_ids.is_empty()
        {
            return Err(Stage8bR2a7SourceAdapterError::CurrentSourceInvalid);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Stage8bR2a7PublicationEvidence {
    pub schema_version: u16,
    pub adapter_domain: &'static str,
    pub source_count: usize,
    pub source_sha256: std::collections::BTreeMap<String, String>,
    pub publication_root_sha256: String,
    pub execution_authority_granted: bool,
    pub network_accessed: bool,
    pub finam_credential_accessed: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum Stage8bR2a7SourceAdapterError {
    #[error("R2A7 adapter must run as the fixed source-adapter UID")]
    WrongUid,
    #[error("R2A7 fixed reader input is unsafe or invalid")]
    ReaderInputInvalid,
    #[error("R2A8 trusted current-source issuer input is invalid")]
    CurrentSourceInvalid,
    #[error("R2A7 runtime profile is not the accepted fixed profile")]
    RuntimeProfileInvalid,
    #[error("R2A7 Stage 7B restart did not yield one ready owner")]
    RecoveryNotReady,
    #[error("R2A7 durable request selection is not unique and current")]
    DurableRequestInvalid,
    #[error("R2A7 operational source publication failed")]
    PublicationFailed,
}

fn production_writer_intake_commitment_sha256(
    intake: &Stage8bR2a8ProductionWriterIntakeV1,
) -> Result<String, Stage8bR2a7SourceAdapterError> {
    use sha2::{Digest, Sha256};
    let mut unsigned = intake.clone();
    unsigned.intake_commitment_sha256.clear();
    unsigned.issuer_public_key_ed25519_hex.clear();
    unsigned.issuer_signature_ed25519_hex.clear();
    let bytes = serde_json::to_vec(&unsigned)
        .map_err(|_| Stage8bR2a7SourceAdapterError::CurrentSourceInvalid)?;
    let mut hasher = Sha256::new();
    hasher.update(PRODUCTION_WRITER_INTAKE_COMMITMENT_DOMAIN);
    hasher.update(b"\0");
    hasher.update(bytes);
    Ok(format!("{:x}", hasher.finalize()))
}

fn validate_production_writer_intake(
    intake: &Stage8bR2a8ProductionWriterIntakeV1,
    accepted_config: &Stage8a1AcceptedExecutionConfigV1,
    accepted_config_sha256: &str,
) -> Result<(), Stage8bR2a7SourceAdapterError> {
    let now = Utc::now();
    let operational_identity_sha256 =
        strategy_runtime_core::stage6d_operational_identity_sha256(&intake.operational_identity)
            .map_err(|_| Stage8bR2a7SourceAdapterError::CurrentSourceInvalid)?;
    if intake.schema_version != 1
        || intake.adapter_domain != "production"
        || intake.runtime_profile_id != RUNTIME_PROFILE_ID
        || intake.accepted_config_sha256 != accepted_config_sha256
        || operational_identity_sha256.as_str() != accepted_config.operational_identity_sha256
        || intake
            .operational_identity
            .stage8a4_writer_issuer_public_key_hex
            != accepted_config.stage8a4_writer_issuer_public_key_hex
        || intake.observed_at > now + Duration::seconds(1)
        || intake.expires_at <= now
        || intake.expires_at <= intake.observed_at
        || intake.expires_at - intake.observed_at
            > Duration::seconds(MAX_CURRENT_SOURCE_TTL_SECONDS)
        || intake.composite_readiness.validate_ready().is_err()
        || intake.intake_commitment_sha256 != production_writer_intake_commitment_sha256(intake)?
        || intake.issuer_public_key_ed25519_hex
            != accepted_config.stage8a4_writer_issuer_public_key_hex
        || !valid_lower_hex(&intake.issuer_public_key_ed25519_hex, 64)
        || !valid_lower_hex(&intake.issuer_signature_ed25519_hex, 128)
    {
        return Err(Stage8bR2a7SourceAdapterError::CurrentSourceInvalid);
    }
    let public_key = VerifyingKey::from_bytes(&decode_lower_hex::<32>(
        &intake.issuer_public_key_ed25519_hex,
    )?)
    .map_err(|_| Stage8bR2a7SourceAdapterError::CurrentSourceInvalid)?;
    let signature = Signature::from_bytes(&decode_lower_hex::<64>(
        &intake.issuer_signature_ed25519_hex,
    )?);
    public_key
        .verify(intake.intake_commitment_sha256.as_bytes(), &signature)
        .map_err(|_| Stage8bR2a7SourceAdapterError::CurrentSourceInvalid)
}

/// The actual authoritative intake creator is an in-process service boundary,
/// not a JSON-signing CLI. Construction requires the recovered Stage 7B owner,
/// its exact durable request and an opaque current-source bundle minted by the
/// same pinned Stage 8A issuer. Consequently no caller-supplied readiness,
/// broker truth, timestamps or arbitrary signing request can reach this seam.
#[allow(clippy::too_many_arguments)]
#[allow(dead_code, reason = "R2B intake creator remains closed until issuance")]
pub(crate) fn create_stage8b_r2a8_owner_signed_intake_from_owner(
    owner: &mut Stage7bRecoveryReadyOwner,
    identity: &Stage6DurableRequestIdentityV1,
    command: &Stage6DurableCommandSnapshotV1,
    operational_identity: &Stage6dOperationalIdentityConfig,
    accepted_config_sha256: &str,
    issuer: &Stage8a1OperationalAuthorityIssuer,
    current_sources: &Stage8a1TrustedCurrentSources,
) -> Result<Stage8bR2a8AuthoritativeIntakeCreatorEvidence, Stage8bR2a7SourceAdapterError> {
    use sha2::{Digest, Sha256};
    use std::io::Write;
    if unsafe { libc::geteuid() } != STAGE8B_R2A8_CURRENT_SOURCE_INPUT_UID {
        return Err(Stage8bR2a7SourceAdapterError::WrongUid);
    }
    let (recovered_identity, recovered_command) = owner
        .recovered()
        .map_err(|_| Stage8bR2a7SourceAdapterError::DurableRequestInvalid)?
        .single_exact_dispatch_ready_request()
        .map_err(|_| Stage8bR2a7SourceAdapterError::DurableRequestInvalid)?;
    if &recovered_identity != identity || &recovered_command != command {
        return Err(Stage8bR2a7SourceAdapterError::DurableRequestInvalid);
    }
    let (readiness, broker_truth, broker_readiness) = current_sources
        .stage8b_r2a8_current_snapshots(issuer)
        .map_err(|_| Stage8bR2a7SourceAdapterError::CurrentSourceInvalid)?;
    let readiness = Stage8bCompositeReadinessAuthorityV1::from_snapshot(&readiness);
    readiness.validate_ready()?;
    let authority_root = Path::new(PRODUCTION_AUTHORITY_ROOT);
    let config_bytes = read_fixed_regular_file(
        &authority_root.join(ACCEPTED_EXECUTION_CONFIG_FILE),
        MAX_MANIFEST_BYTES,
        0,
    )?;
    let accepted_config: Stage8a1AcceptedExecutionConfigV1 = serde_json::from_slice(&config_bytes)
        .map_err(|_| Stage8bR2a7SourceAdapterError::CurrentSourceInvalid)?;
    if format!("{:x}", Sha256::digest(&config_bytes)) != accepted_config_sha256
        || strategy_runtime_core::stage6d_operational_identity_sha256(operational_identity)
            .map_err(|_| Stage8bR2a7SourceAdapterError::CurrentSourceInvalid)?
            .as_str()
            != accepted_config.operational_identity_sha256
    {
        return Err(Stage8bR2a7SourceAdapterError::CurrentSourceInvalid);
    }
    let observed_at = [
        readiness.checked_at,
        broker_truth.received_ts,
        broker_readiness
            .schedule
            .observed_ts
            .ok_or(Stage8bR2a7SourceAdapterError::CurrentSourceInvalid)?,
        broker_readiness
            .instrument_spec
            .observed_ts
            .ok_or(Stage8bR2a7SourceAdapterError::CurrentSourceInvalid)?,
    ]
    .into_iter()
    .min()
    .ok_or(Stage8bR2a7SourceAdapterError::CurrentSourceInvalid)?;
    let mut intake = Stage8bR2a8ProductionWriterIntakeV1 {
        schema_version: 1,
        adapter_domain: "production".to_owned(),
        runtime_profile_id: RUNTIME_PROFILE_ID.to_owned(),
        operational_identity: operational_identity.clone(),
        accepted_config_sha256: accepted_config_sha256.to_owned(),
        durable_request_identity: identity.clone(),
        durable_command: command.clone(),
        composite_readiness: readiness,
        broker_truth,
        broker_readiness,
        observed_at,
        expires_at: observed_at + Duration::seconds(MAX_CURRENT_SOURCE_TTL_SECONDS),
        intake_commitment_sha256: String::new(),
        issuer_public_key_ed25519_hex: String::new(),
        issuer_signature_ed25519_hex: String::new(),
    };
    intake.intake_commitment_sha256 = production_writer_intake_commitment_sha256(&intake)?;
    let (public_key, signature) = issuer
        .sign_stage8b_r2a8_current_source_commitment(&intake.intake_commitment_sha256)
        .map_err(|_| Stage8bR2a7SourceAdapterError::CurrentSourceInvalid)?;
    intake.issuer_public_key_ed25519_hex = public_key;
    intake.issuer_signature_ed25519_hex = signature;
    validate_production_writer_intake(&intake, &accepted_config, accepted_config_sha256)?;

    let lock_path = authority_root.join("stage8b-r2a8-owner-signed-intake.lock");
    let mut lock = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(libc::O_CLOEXEC | libc::O_NOFOLLOW)
        .open(&lock_path)
        .map_err(|_| Stage8bR2a7SourceAdapterError::PublicationFailed)?;
    lock.write_all(b"stage8b-r2a8-authoritative-intake-creator-lock-v1\n")
        .and_then(|_| lock.sync_all())
        .map_err(|_| Stage8bR2a7SourceAdapterError::PublicationFailed)?;
    std::fs::File::open(authority_root)
        .and_then(|directory| directory.sync_all())
        .map_err(|_| Stage8bR2a7SourceAdapterError::PublicationFailed)?;
    let output = authority_root.join(PRODUCTION_OWNER_SIGNED_INTAKE_FILE);
    atomic_write_fixed(
        &output,
        &serde_json::to_vec(&intake)
            .map_err(|_| Stage8bR2a7SourceAdapterError::CurrentSourceInvalid)?,
        STAGE8B_R2A8_CURRENT_SOURCE_INPUT_UID,
    )?;
    Ok(Stage8bR2a8AuthoritativeIntakeCreatorEvidence {
        schema_version: 1,
        service: "stage8b-r2a8-authoritative-intake-creator-service",
        fixed_output:
            "/var/lib/moex-trading/stage8a1-authority/stage8b-r2a8-owner-signed-intake.json",
        intake_commitment_sha256: intake.intake_commitment_sha256,
        opaque_current_sources_required: true,
        recovered_owner_required: true,
        caller_supplied_json_accepted: false,
        caller_supplied_timestamp_accepted: false,
        network_accessed: false,
        finam_credential_accessed: false,
    })
}

/// Dedicated production current-source writer. It accepts no arguments,
/// paths or snapshots: all paths are compile-time constants and the sole
/// intake is signed by the pinned Stage8A writer issuer. The Stage7B owner and
/// exact durable request are reconstructed independently before publication.
pub fn run_stage8b_r2a8_production_current_source_writer(
) -> Result<Stage8bR2a8ProductionWriterEvidence, Stage8bR2a7SourceAdapterError> {
    if unsafe { libc::geteuid() } != STAGE8B_R2A6_SOURCE_ADAPTER_UID {
        return Err(Stage8bR2a7SourceAdapterError::WrongUid);
    }
    let mode = Stage8bR2a7RunMode::Production;
    let layout = mode.layout();
    let intake_bytes = read_fixed_regular_file(
        &Path::new(PRODUCTION_INTAKE_ROOT).join(PRODUCTION_WRITER_INTAKE_FILE),
        MAX_MANIFEST_BYTES,
        STAGE8B_R2A8_CURRENT_SOURCE_INPUT_UID,
    )?;
    let intake: Stage8bR2a8ProductionWriterIntakeV1 = serde_json::from_slice(&intake_bytes)
        .map_err(|_| Stage8bR2a7SourceAdapterError::CurrentSourceInvalid)?;
    let config_bytes = read_fixed_regular_file(
        &layout.authority_root.join(ACCEPTED_EXECUTION_CONFIG_FILE),
        MAX_MANIFEST_BYTES,
        0,
    )?;
    let config_sha256_bytes = read_fixed_regular_file(
        &layout
            .authority_root
            .join(ACCEPTED_EXECUTION_CONFIG_SHA256_FILE),
        128,
        0,
    )?;
    let accepted_config_sha256 = std::str::from_utf8(&config_sha256_bytes)
        .map_err(|_| Stage8bR2a7SourceAdapterError::CurrentSourceInvalid)?
        .trim_end_matches(['\r', '\n']);
    if !valid_sha256(accepted_config_sha256) {
        return Err(Stage8bR2a7SourceAdapterError::CurrentSourceInvalid);
    }
    use sha2::{Digest, Sha256};
    if format!("{:x}", Sha256::digest(&config_bytes)) != accepted_config_sha256 {
        return Err(Stage8bR2a7SourceAdapterError::CurrentSourceInvalid);
    }
    let accepted_config: Stage8a1AcceptedExecutionConfigV1 = serde_json::from_slice(&config_bytes)
        .map_err(|_| Stage8bR2a7SourceAdapterError::CurrentSourceInvalid)?;
    validate_production_writer_intake(&intake, &accepted_config, accepted_config_sha256)?;

    let key_bytes = read_lifecycle_key_file(&layout.work_root.join(LIFECYCLE_KEY_FILE))?;
    let commitment_key = parse_lifecycle_key(&key_bytes)?;
    let runtime = fixed_runtime_profile(&intake.runtime_profile_id)?;
    let root_name =
        Stage7bDurableRootAuthority::expected_directory_name(&intake.operational_identity)
            .map_err(|_| Stage8bR2a7SourceAdapterError::ReaderInputInvalid)?;
    let durable_root = layout.stage7b_parent.join(root_name);
    let root = Stage7bDurableRootAuthority::validate(&durable_root, &intake.operational_identity)
        .map_err(|_| Stage8bR2a7SourceAdapterError::ReaderInputInvalid)?;
    let restarted = Stage7bRecoveryReadyOwner::restart(
        root,
        intake.operational_identity.clone(),
        &commitment_key,
        runtime,
    )
    .map_err(|_| Stage8bR2a7SourceAdapterError::RecoveryNotReady)?;
    let Stage7bRestartOutcome::Ready(mut owner) = restarted else {
        return Err(Stage8bR2a7SourceAdapterError::RecoveryNotReady);
    };
    let (identity, command) = owner
        .recovered()
        .map_err(|_| Stage8bR2a7SourceAdapterError::DurableRequestInvalid)?
        .single_exact_dispatch_ready_request()
        .map_err(|_| Stage8bR2a7SourceAdapterError::DurableRequestInvalid)?;
    if identity != intake.durable_request_identity || command != intake.durable_command {
        return Err(Stage8bR2a7SourceAdapterError::DurableRequestInvalid);
    }
    let readiness = intake.composite_readiness.to_snapshot();
    publish_stage8b_r2a8_trusted_current_source_from_owner(
        mode,
        &mut owner,
        &commitment_key,
        &identity,
        &command,
        &intake.operational_identity,
        accepted_config_sha256,
        &readiness,
        &intake.broker_truth,
        &intake.broker_readiness,
    )?;
    let output_path = layout.current_source_root.join(TRUSTED_CURRENT_SOURCE_FILE);
    let output_bytes = read_fixed_regular_file(
        &output_path,
        MAX_MANIFEST_BYTES,
        STAGE8B_R2A6_SOURCE_ADAPTER_UID,
    )?;
    Ok(Stage8bR2a8ProductionWriterEvidence {
        schema_version: 1,
        writer: "stage8b-r2a8-production-current-source-writer",
        fixed_input: PRODUCTION_WRITER_INTAKE_FILE,
        fixed_output: TRUSTED_CURRENT_SOURCE_FILE,
        durable_request_identity_sha256: format!(
            "{:x}",
            Sha256::digest(
                serde_json::to_vec(&identity)
                    .map_err(|_| Stage8bR2a7SourceAdapterError::CurrentSourceInvalid)?
            )
        ),
        trusted_current_source_sha256: format!("{:x}", Sha256::digest(output_bytes)),
        network_accessed: false,
        finam_credential_accessed: false,
        caller_supplied_path_accepted: false,
        caller_supplied_snapshot_accepted: false,
    })
}

/// Exact no-network producer for the writer intake. The accepted Stage 8A
/// owner is the only component allowed to create the signed source file. This
/// producer accepts no arguments or caller snapshots, verifies the pinned
/// signature/config/freshness, and atomically stages the exact same bytes for
/// the independently privileged current-source writer.
pub fn run_stage8b_r2a8_production_intake_stager(
) -> Result<Stage8bR2a8ProductionIntakeStagerEvidence, Stage8bR2a7SourceAdapterError> {
    use sha2::{Digest, Sha256};
    if unsafe { libc::geteuid() } != STAGE8B_R2A8_CURRENT_SOURCE_INPUT_UID {
        return Err(Stage8bR2a7SourceAdapterError::WrongUid);
    }
    let authority_root = Path::new(PRODUCTION_AUTHORITY_ROOT);
    let source_path = authority_root.join(PRODUCTION_OWNER_SIGNED_INTAKE_FILE);
    let bytes = read_fixed_regular_file(
        &source_path,
        MAX_MANIFEST_BYTES,
        STAGE8B_R2A8_CURRENT_SOURCE_INPUT_UID,
    )?;
    let intake: Stage8bR2a8ProductionWriterIntakeV1 = serde_json::from_slice(&bytes)
        .map_err(|_| Stage8bR2a7SourceAdapterError::CurrentSourceInvalid)?;
    let config_bytes = read_fixed_regular_file(
        &authority_root.join(ACCEPTED_EXECUTION_CONFIG_FILE),
        MAX_MANIFEST_BYTES,
        0,
    )?;
    let config_sha_bytes = read_fixed_regular_file(
        &authority_root.join(ACCEPTED_EXECUTION_CONFIG_SHA256_FILE),
        128,
        0,
    )?;
    let config_sha = std::str::from_utf8(&config_sha_bytes)
        .map_err(|_| Stage8bR2a7SourceAdapterError::CurrentSourceInvalid)?
        .trim_end_matches(['\r', '\n']);
    if !valid_sha256(config_sha) || format!("{:x}", Sha256::digest(&config_bytes)) != config_sha {
        return Err(Stage8bR2a7SourceAdapterError::CurrentSourceInvalid);
    }
    let config: Stage8a1AcceptedExecutionConfigV1 = serde_json::from_slice(&config_bytes)
        .map_err(|_| Stage8bR2a7SourceAdapterError::CurrentSourceInvalid)?;
    validate_production_writer_intake(&intake, &config, config_sha)?;
    let output = Path::new(PRODUCTION_INTAKE_ROOT).join(PRODUCTION_WRITER_INTAKE_FILE);
    atomic_write_fixed(&output, &bytes, STAGE8B_R2A8_CURRENT_SOURCE_INPUT_UID)?;
    Ok(Stage8bR2a8ProductionIntakeStagerEvidence {
        schema_version: 1,
        stager: "stage8b-r2a8-production-intake-stager",
        fixed_owner_input:
            "/var/lib/moex-trading/stage8a1-authority/stage8b-r2a8-owner-signed-intake.json",
        fixed_writer_output:
            "/var/lib/moex-trading/stage8b/r2a8/intake/stage8b-r2a8-production-writer-intake.json",
        intake_commitment_sha256: intake.intake_commitment_sha256,
        source_owner_uid: STAGE8B_R2A8_CURRENT_SOURCE_INPUT_UID,
        output_owner_uid: STAGE8B_R2A8_CURRENT_SOURCE_INPUT_UID,
        network_accessed: false,
        finam_credential_accessed: false,
        caller_supplied_input_accepted: false,
    })
}

/// Owner-mediated production writer for the sole R2A8 current-source input.
/// Raw snapshots are admitted only while the caller holds the recovered owner
/// and the pinned Stage 8A authority issuer can validate and sign the exact
/// commitment. No caller-supplied path crosses this boundary.
#[allow(clippy::too_many_arguments)]
pub(crate) fn publish_stage8b_r2a8_trusted_current_source_from_owner(
    mode: Stage8bR2a7RunMode,
    owner: &mut Stage7bRecoveryReadyOwner,
    commitment_key: &Stage5gLifecycleCommitmentKey,
    identity: &Stage6DurableRequestIdentityV1,
    command: &Stage6DurableCommandSnapshotV1,
    operational_identity: &Stage6dOperationalIdentityConfig,
    accepted_config_sha256: &str,
    composite_readiness: &Stage7bCompositeReadinessSnapshot,
    broker_truth: &BrokerTruthSnapshot,
    broker_readiness: &BrokerReadinessSnapshot,
) -> Result<(), Stage8bR2a7SourceAdapterError> {
    if unsafe { libc::geteuid() } != STAGE8B_R2A6_SOURCE_ADAPTER_UID {
        return Err(Stage8bR2a7SourceAdapterError::WrongUid);
    }
    let composite_readiness =
        Stage8bCompositeReadinessAuthorityV1::from_snapshot(composite_readiness);
    composite_readiness.validate_ready()?;
    let layout = mode.layout();
    let issuer = Stage8a1OperationalAuthorityIssuer::from_stage7b_owner(
        owner,
        commitment_key,
        identity,
        command,
        &layout.authority_root,
        accepted_config_sha256,
    )
    .map_err(|_| Stage8bR2a7SourceAdapterError::CurrentSourceInvalid)?;
    issuer
        .issue_current_sources(
            &composite_readiness.to_snapshot(),
            broker_truth,
            broker_readiness,
        )
        .map_err(|_| Stage8bR2a7SourceAdapterError::CurrentSourceInvalid)?;
    let source_observed_at = [
        composite_readiness.checked_at,
        broker_truth.received_ts,
        broker_readiness
            .schedule
            .observed_ts
            .ok_or(Stage8bR2a7SourceAdapterError::CurrentSourceInvalid)?,
        broker_readiness
            .instrument_spec
            .observed_ts
            .ok_or(Stage8bR2a7SourceAdapterError::CurrentSourceInvalid)?,
    ]
    .into_iter()
    .min()
    .ok_or(Stage8bR2a7SourceAdapterError::CurrentSourceInvalid)?;
    let source_generation = u64::try_from(source_observed_at.timestamp_millis())
        .ok()
        .filter(|generation| *generation > 0)
        .ok_or(Stage8bR2a7SourceAdapterError::CurrentSourceInvalid)?;
    let mut source = Stage8bR2a8TrustedCurrentSourceV2 {
        schema_version: 2,
        adapter_domain: mode.adapter_domain().to_owned(),
        adapter_mode: "one_shot_recovery_reader".to_owned(),
        source_generation,
        source_observed_at,
        expires_at: source_observed_at + Duration::seconds(MAX_CURRENT_SOURCE_TTL_SECONDS),
        runtime_profile_id: RUNTIME_PROFILE_ID.to_owned(),
        operational_identity: operational_identity.clone(),
        accepted_config_sha256: accepted_config_sha256.to_owned(),
        composite_readiness,
        broker_truth: broker_truth.clone(),
        broker_readiness: broker_readiness.clone(),
        current_source_commitment_sha256: String::new(),
        current_source_issuer_public_key_hex: String::new(),
        current_source_signature_ed25519_hex: String::new(),
    };
    source.current_source_commitment_sha256 = current_source_commitment_sha256(&source)?;
    let (public_key, signature) = issuer
        .sign_stage8b_r2a8_current_source_commitment(&source.current_source_commitment_sha256)
        .map_err(|_| Stage8bR2a7SourceAdapterError::CurrentSourceInvalid)?;
    source.current_source_issuer_public_key_hex = public_key;
    source.current_source_signature_ed25519_hex = signature;
    validate_trusted_current_source(&source, mode)?;
    atomic_write_fixed(
        &layout.current_source_root.join(TRUSTED_CURRENT_SOURCE_FILE),
        &serde_json::to_vec(&source)
            .map_err(|_| Stage8bR2a7SourceAdapterError::CurrentSourceInvalid)?,
        STAGE8B_R2A6_SOURCE_ADAPTER_UID,
    )
}

/// Exact one-shot current-manifest issuer. It accepts only the signed current
/// source emitted above and publishes atomically to the mode-fixed root.
pub fn issue_stage8b_r2a8_reader_manifest(
    mode: Stage8bR2a7RunMode,
) -> Result<(), Stage8bR2a7SourceAdapterError> {
    if unsafe { libc::geteuid() } != STAGE8B_R2A8_CURRENT_MANIFEST_ISSUER_UID {
        return Err(Stage8bR2a7SourceAdapterError::WrongUid);
    }
    let layout = mode.layout();
    let source_bytes = read_fixed_regular_file(
        &layout.current_source_root.join(TRUSTED_CURRENT_SOURCE_FILE),
        MAX_MANIFEST_BYTES,
        STAGE8B_R2A6_SOURCE_ADAPTER_UID,
    )?;
    let source: Stage8bR2a8TrustedCurrentSourceV2 = serde_json::from_slice(&source_bytes)
        .map_err(|_| Stage8bR2a7SourceAdapterError::CurrentSourceInvalid)?;
    validate_trusted_current_source(&source, mode)?;
    let key_bytes = read_lifecycle_key_file(&layout.work_root.join(LIFECYCLE_KEY_FILE))?;
    let commitment_key = parse_lifecycle_key(&key_bytes)?;
    let mut manifest = Stage8bR2a7ReaderManifestV2 {
        schema_version: 3,
        adapter_domain: source.adapter_domain,
        runtime_profile_id: source.runtime_profile_id,
        operational_identity: source.operational_identity,
        accepted_config_sha256: source.accepted_config_sha256,
        composite_readiness: source.composite_readiness,
        broker_truth: source.broker_truth,
        broker_readiness: source.broker_readiness,
        source_generation: source.source_generation,
        source_observed_at: source.source_observed_at,
        expires_at: source.expires_at,
        current_source_commitment_sha256: source.current_source_commitment_sha256,
        current_source_issuer_public_key_hex: source.current_source_issuer_public_key_hex,
        current_source_signature_ed25519_hex: source.current_source_signature_ed25519_hex,
        manifest_hmac_sha256: String::new(),
    };
    manifest.manifest_hmac_sha256 = commitment_key
        .stage8b_r2a7_reader_manifest_hmac_sha256(&reader_manifest_commitment_sha256(&manifest)?);
    atomic_write_fixed(
        &layout.work_root.join(MANIFEST_FILE),
        &serde_json::to_vec(&manifest)
            .map_err(|_| Stage8bR2a7SourceAdapterError::CurrentSourceInvalid)?,
        STAGE8B_R2A8_CURRENT_MANIFEST_ISSUER_UID,
    )
}

/// Runs the exact production-capable source adapter once.  `mode` selects one
/// of three compile-time layouts; no caller-supplied path or request identity
/// crosses this boundary.
pub fn run_stage8b_r2a7_source_adapter(
    mode: Stage8bR2a7RunMode,
) -> Result<Stage8bR2a7PublicationEvidence, Stage8bR2a7SourceAdapterError> {
    if unsafe { libc::geteuid() } != STAGE8B_R2A6_SOURCE_ADAPTER_UID {
        return Err(Stage8bR2a7SourceAdapterError::WrongUid);
    }
    let layout = mode.layout();
    let manifest_bytes = read_fixed_regular_file(
        &layout.work_root.join(MANIFEST_FILE),
        MAX_MANIFEST_BYTES,
        STAGE8B_R2A8_CURRENT_MANIFEST_ISSUER_UID,
    )?;
    let manifest: Stage8bR2a7ReaderManifestV2 = serde_json::from_slice(&manifest_bytes)
        .map_err(|_| Stage8bR2a7SourceAdapterError::ReaderInputInvalid)?;
    let key_bytes = read_lifecycle_key_file(&layout.work_root.join(LIFECYCLE_KEY_FILE))?;
    let commitment_key = parse_lifecycle_key(&key_bytes)?;
    validate_manifest(&manifest, mode, &commitment_key)?;
    let runtime = fixed_runtime_profile(&manifest.runtime_profile_id)?;
    let root_name =
        Stage7bDurableRootAuthority::expected_directory_name(&manifest.operational_identity)
            .map_err(|_| Stage8bR2a7SourceAdapterError::ReaderInputInvalid)?;
    let durable_root = layout.stage7b_parent.join(root_name);
    let root = Stage7bDurableRootAuthority::validate(&durable_root, &manifest.operational_identity)
        .map_err(|_| Stage8bR2a7SourceAdapterError::ReaderInputInvalid)?;
    let restarted = Stage7bRecoveryReadyOwner::restart(
        root,
        manifest.operational_identity,
        &commitment_key,
        runtime,
    )
    .map_err(|_| Stage8bR2a7SourceAdapterError::RecoveryNotReady)?;
    let Stage7bRestartOutcome::Ready(mut owner) = restarted else {
        return Err(Stage8bR2a7SourceAdapterError::RecoveryNotReady);
    };
    let (identity, command) = owner
        .recovered()
        .map_err(|_| Stage8bR2a7SourceAdapterError::DurableRequestInvalid)?
        .single_exact_dispatch_ready_request()
        .map_err(|_| Stage8bR2a7SourceAdapterError::DurableRequestInvalid)?;
    manifest.composite_readiness.validate_ready()?;
    let readiness = manifest.composite_readiness.to_snapshot();
    let evidence = publish_stage8b_r2a7_operational_sources_from_owner(
        &mut owner,
        &commitment_key,
        &identity,
        &command,
        &layout.authority_root,
        &manifest.accepted_config_sha256,
        &readiness,
        &manifest.broker_truth,
        &manifest.broker_readiness,
        &layout.output_root,
        mode.adapter_domain(),
    )
    .map_err(|_| Stage8bR2a7SourceAdapterError::PublicationFailed)?;
    verify_published_domain(&layout.output_root, &evidence, mode.adapter_domain())?;
    Ok(publication_evidence(mode, evidence))
}

fn verify_published_domain(
    output_root: &Path,
    evidence: &Stage8bR2a5SourcePublicationEvidence,
    expected_domain: &str,
) -> Result<(), Stage8bR2a7SourceAdapterError> {
    if !matches!(expected_domain, "production" | "controlled_qualification")
        || evidence.schema_version != 2
        || evidence.source_count != evidence.source_sha256.len()
    {
        return Err(Stage8bR2a7SourceAdapterError::PublicationFailed);
    }
    for file_name in evidence.source_sha256.keys() {
        if file_name.contains('/') || file_name.contains("..") {
            return Err(Stage8bR2a7SourceAdapterError::PublicationFailed);
        }
        let bytes = read_fixed_regular_file(
            &output_root.join(file_name),
            MAX_MANIFEST_BYTES,
            STAGE8B_R2A6_SOURCE_ADAPTER_UID,
        )?;
        let value: serde_json::Value = serde_json::from_slice(&bytes)
            .map_err(|_| Stage8bR2a7SourceAdapterError::PublicationFailed)?;
        if value.get("adapter_domain").and_then(|value| value.as_str()) != Some(expected_domain)
            || value.get("adapter_mode").and_then(|value| value.as_str())
                != Some("one_shot_recovery_reader")
        {
            return Err(Stage8bR2a7SourceAdapterError::PublicationFailed);
        }
    }
    Ok(())
}

fn publication_evidence(
    mode: Stage8bR2a7RunMode,
    evidence: Stage8bR2a5SourcePublicationEvidence,
) -> Stage8bR2a7PublicationEvidence {
    Stage8bR2a7PublicationEvidence {
        schema_version: 1,
        adapter_domain: mode.adapter_domain(),
        source_count: evidence.source_count,
        source_sha256: evidence.source_sha256,
        publication_root_sha256: evidence.publication_root_sha256,
        execution_authority_granted: false,
        network_accessed: false,
        finam_credential_accessed: false,
    }
}

fn validate_manifest(
    manifest: &Stage8bR2a7ReaderManifestV2,
    mode: Stage8bR2a7RunMode,
    commitment_key: &Stage5gLifecycleCommitmentKey,
) -> Result<(), Stage8bR2a7SourceAdapterError> {
    let source = trusted_current_source_from_manifest(manifest);
    if manifest.schema_version != 3
        || manifest.adapter_domain != mode.adapter_domain()
        || manifest.runtime_profile_id != RUNTIME_PROFILE_ID
        || !valid_sha256(&manifest.accepted_config_sha256)
        || manifest.broker_truth.received_ts > Utc::now() + Duration::seconds(1)
        || manifest.composite_readiness.checked_at > Utc::now() + Duration::seconds(1)
        || manifest.composite_readiness.validate_ready().is_err()
        || validate_trusted_current_source(&source, mode).is_err()
        || !commitment_key.stage8b_r2a7_verify_reader_manifest_hmac_sha256(
            &reader_manifest_commitment_sha256(manifest)?,
            &manifest.manifest_hmac_sha256,
        )
    {
        return Err(Stage8bR2a7SourceAdapterError::ReaderInputInvalid);
    }
    Ok(())
}

fn trusted_current_source_from_manifest(
    manifest: &Stage8bR2a7ReaderManifestV2,
) -> Stage8bR2a8TrustedCurrentSourceV2 {
    Stage8bR2a8TrustedCurrentSourceV2 {
        schema_version: 2,
        adapter_domain: manifest.adapter_domain.clone(),
        adapter_mode: "one_shot_recovery_reader".to_owned(),
        source_generation: manifest.source_generation,
        source_observed_at: manifest.source_observed_at,
        expires_at: manifest.expires_at,
        runtime_profile_id: manifest.runtime_profile_id.clone(),
        operational_identity: manifest.operational_identity.clone(),
        accepted_config_sha256: manifest.accepted_config_sha256.clone(),
        composite_readiness: manifest.composite_readiness.clone(),
        broker_truth: manifest.broker_truth.clone(),
        broker_readiness: manifest.broker_readiness.clone(),
        current_source_commitment_sha256: manifest.current_source_commitment_sha256.clone(),
        current_source_issuer_public_key_hex: manifest.current_source_issuer_public_key_hex.clone(),
        current_source_signature_ed25519_hex: manifest.current_source_signature_ed25519_hex.clone(),
    }
}

fn current_source_commitment_sha256(
    source: &Stage8bR2a8TrustedCurrentSourceV2,
) -> Result<String, Stage8bR2a7SourceAdapterError> {
    use sha2::{Digest, Sha256};
    let mut unsigned = source.clone();
    unsigned.current_source_commitment_sha256.clear();
    unsigned.current_source_issuer_public_key_hex.clear();
    unsigned.current_source_signature_ed25519_hex.clear();
    let bytes = serde_json::to_vec(&unsigned)
        .map_err(|_| Stage8bR2a7SourceAdapterError::CurrentSourceInvalid)?;
    let mut hasher = Sha256::new();
    hasher.update(CURRENT_SOURCE_COMMITMENT_DOMAIN);
    hasher.update(b"\0");
    hasher.update(bytes);
    Ok(format!("{:x}", hasher.finalize()))
}

fn validate_trusted_current_source(
    source: &Stage8bR2a8TrustedCurrentSourceV2,
    mode: Stage8bR2a7RunMode,
) -> Result<(), Stage8bR2a7SourceAdapterError> {
    let now = Utc::now();
    if source.schema_version != 2
        || source.adapter_domain != mode.adapter_domain()
        || source.adapter_mode != "one_shot_recovery_reader"
        || source.source_generation == 0
        || source.runtime_profile_id != RUNTIME_PROFILE_ID
        || !valid_sha256(&source.accepted_config_sha256)
        || source.composite_readiness.checked_at > now + Duration::seconds(1)
        || source.composite_readiness.validate_ready().is_err()
        || source.source_observed_at > now + Duration::seconds(1)
        || source.expires_at <= now
        || source.expires_at <= source.source_observed_at
        || source.expires_at - source.source_observed_at
            > Duration::seconds(MAX_CURRENT_SOURCE_TTL_SECONDS)
        || source.current_source_commitment_sha256 != current_source_commitment_sha256(source)?
        || source.current_source_issuer_public_key_hex
            != source
                .operational_identity
                .stage8a4_writer_issuer_public_key_hex
        || !valid_lower_hex(&source.current_source_issuer_public_key_hex, 64)
        || !valid_lower_hex(&source.current_source_signature_ed25519_hex, 128)
    {
        return Err(Stage8bR2a7SourceAdapterError::CurrentSourceInvalid);
    }
    let public_key = VerifyingKey::from_bytes(&decode_lower_hex::<32>(
        &source.current_source_issuer_public_key_hex,
    )?)
    .map_err(|_| Stage8bR2a7SourceAdapterError::CurrentSourceInvalid)?;
    let signature = Signature::from_bytes(&decode_lower_hex::<64>(
        &source.current_source_signature_ed25519_hex,
    )?);
    public_key
        .verify(
            source.current_source_commitment_sha256.as_bytes(),
            &signature,
        )
        .map_err(|_| Stage8bR2a7SourceAdapterError::CurrentSourceInvalid)
}

fn reader_manifest_commitment_sha256(
    manifest: &Stage8bR2a7ReaderManifestV2,
) -> Result<String, Stage8bR2a7SourceAdapterError> {
    use sha2::{Digest, Sha256};
    let mut value = serde_json::to_value(manifest)
        .map_err(|_| Stage8bR2a7SourceAdapterError::ReaderInputInvalid)?;
    value
        .as_object_mut()
        .ok_or(Stage8bR2a7SourceAdapterError::ReaderInputInvalid)?
        .remove("manifest_hmac_sha256");
    let bytes = serde_json::to_vec(&value)
        .map_err(|_| Stage8bR2a7SourceAdapterError::ReaderInputInvalid)?;
    let mut hasher = Sha256::new();
    hasher.update(b"stage8b-r2a8-r1-reader-manifest-commitment-v2");
    hasher.update(b"\0");
    hasher.update(bytes);
    Ok(format!("{:x}", hasher.finalize()))
}

fn read_fixed_regular_file(
    path: &Path,
    max_bytes: u64,
    expected_owner: u32,
) -> Result<Vec<u8>, Stage8bR2a7SourceAdapterError> {
    if !path.is_absolute() {
        return Err(Stage8bR2a7SourceAdapterError::ReaderInputInvalid);
    }
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_CLOEXEC | libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .map_err(|_| Stage8bR2a7SourceAdapterError::ReaderInputInvalid)?;
    let metadata = file
        .metadata()
        .map_err(|_| Stage8bR2a7SourceAdapterError::ReaderInputInvalid)?;
    if !metadata.file_type().is_file()
        || metadata.uid() != expected_owner
        || metadata.nlink() != 1
        || metadata.len() > max_bytes
        || metadata.mode() & 0o022 != 0
    {
        return Err(Stage8bR2a7SourceAdapterError::ReaderInputInvalid);
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.read_to_end(&mut bytes)
        .map_err(|_| Stage8bR2a7SourceAdapterError::ReaderInputInvalid)?;
    if bytes.len() as u64 != metadata.len() {
        return Err(Stage8bR2a7SourceAdapterError::ReaderInputInvalid);
    }
    Ok(bytes)
}

fn lifecycle_key_properties_are_exact(
    is_regular_file: bool,
    uid: u32,
    gid: u32,
    mode: u32,
    nlink: u64,
    len: u64,
) -> bool {
    is_regular_file
        && uid == STAGE8B_R2A8_CURRENT_MANIFEST_ISSUER_UID
        && gid == STAGE8B_R2A8_LIFECYCLE_KEY_GID
        && mode & 0o777 == STAGE8B_R2A8_LIFECYCLE_KEY_MODE
        && nlink == 1
        && matches!(len, 64 | 65)
        && len <= MAX_KEY_BYTES
}

/// Lifecycle-key-specific custody reader. Generic read-only file checks are
/// intentionally insufficient here: both the issuer and adapter rely on the
/// exact owner/group/mode contract to share this single secret safely.
fn read_lifecycle_key_file(path: &Path) -> Result<Vec<u8>, Stage8bR2a7SourceAdapterError> {
    if !path.is_absolute() {
        return Err(Stage8bR2a7SourceAdapterError::ReaderInputInvalid);
    }
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_CLOEXEC | libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .map_err(|_| Stage8bR2a7SourceAdapterError::ReaderInputInvalid)?;
    let metadata = file
        .metadata()
        .map_err(|_| Stage8bR2a7SourceAdapterError::ReaderInputInvalid)?;
    if !lifecycle_key_properties_are_exact(
        metadata.file_type().is_file(),
        metadata.uid(),
        metadata.gid(),
        metadata.mode(),
        metadata.nlink(),
        metadata.len(),
    ) {
        return Err(Stage8bR2a7SourceAdapterError::ReaderInputInvalid);
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.read_to_end(&mut bytes)
        .map_err(|_| Stage8bR2a7SourceAdapterError::ReaderInputInvalid)?;
    if bytes.len() as u64 != metadata.len() {
        return Err(Stage8bR2a7SourceAdapterError::ReaderInputInvalid);
    }
    Ok(bytes)
}

fn atomic_write_fixed(
    path: &Path,
    bytes: &[u8],
    expected_parent_owner: u32,
) -> Result<(), Stage8bR2a7SourceAdapterError> {
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;
    let parent = path
        .parent()
        .ok_or(Stage8bR2a7SourceAdapterError::CurrentSourceInvalid)?;
    let metadata = std::fs::symlink_metadata(parent)
        .map_err(|_| Stage8bR2a7SourceAdapterError::CurrentSourceInvalid)?;
    if !metadata.file_type().is_dir()
        || metadata.file_type().is_symlink()
        || metadata.uid() != expected_parent_owner
        || metadata.mode() & 0o022 != 0
    {
        return Err(Stage8bR2a7SourceAdapterError::CurrentSourceInvalid);
    }
    if std::fs::symlink_metadata(path).is_ok_and(|current| current.file_type().is_symlink()) {
        return Err(Stage8bR2a7SourceAdapterError::CurrentSourceInvalid);
    }
    let temporary = parent.join(format!(".stage8b-r2a8.{}.tmp", uuid::Uuid::new_v4()));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o644)
        .custom_flags(libc::O_CLOEXEC | libc::O_NOFOLLOW)
        .open(&temporary)
        .map_err(|_| Stage8bR2a7SourceAdapterError::CurrentSourceInvalid)?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|_| Stage8bR2a7SourceAdapterError::CurrentSourceInvalid)?;
    std::fs::rename(&temporary, path)
        .map_err(|_| Stage8bR2a7SourceAdapterError::CurrentSourceInvalid)?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o644))
        .map_err(|_| Stage8bR2a7SourceAdapterError::CurrentSourceInvalid)?;
    std::fs::File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|_| Stage8bR2a7SourceAdapterError::CurrentSourceInvalid)
}

fn valid_lower_hex(value: &str, expected_len: usize) -> bool {
    value.len() == expected_len
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn decode_lower_hex<const N: usize>(value: &str) -> Result<[u8; N], Stage8bR2a7SourceAdapterError> {
    if !valid_lower_hex(value, N * 2) {
        return Err(Stage8bR2a7SourceAdapterError::CurrentSourceInvalid);
    }
    let mut decoded = [0u8; N];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        let pair = std::str::from_utf8(pair)
            .map_err(|_| Stage8bR2a7SourceAdapterError::CurrentSourceInvalid)?;
        decoded[index] = u8::from_str_radix(pair, 16)
            .map_err(|_| Stage8bR2a7SourceAdapterError::CurrentSourceInvalid)?;
    }
    Ok(decoded)
}

fn parse_lifecycle_key(
    bytes: &[u8],
) -> Result<Stage5gLifecycleCommitmentKey, Stage8bR2a7SourceAdapterError> {
    if bytes.is_empty() || bytes.contains(&b'\r') || bytes.contains(&0) {
        return Err(Stage8bR2a7SourceAdapterError::ReaderInputInvalid);
    }
    let line = bytes.strip_suffix(b"\n").unwrap_or(bytes);
    if line.len() != 64
        || line.contains(&b'\n')
        || !line
            .iter()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(byte))
    {
        return Err(Stage8bR2a7SourceAdapterError::ReaderInputInvalid);
    }
    let text =
        std::str::from_utf8(line).map_err(|_| Stage8bR2a7SourceAdapterError::ReaderInputInvalid)?;
    let mut decoded = [0_u8; 32];
    for (index, slot) in decoded.iter_mut().enumerate() {
        *slot = u8::from_str_radix(&text[index * 2..index * 2 + 2], 16)
            .map_err(|_| Stage8bR2a7SourceAdapterError::ReaderInputInvalid)?;
    }
    Stage5gLifecycleCommitmentKey::from_secret_bytes(&decoded)
        .map_err(|_| Stage8bR2a7SourceAdapterError::ReaderInputInvalid)
}

fn fixed_runtime_profile(
    profile: &str,
) -> Result<HybridIntradayRuntimeStrategy, Stage8bR2a7SourceAdapterError> {
    if profile != RUNTIME_PROFILE_ID {
        return Err(Stage8bR2a7SourceAdapterError::RuntimeProfileInvalid);
    }
    let bar_close = Utc
        .with_ymd_and_hms(2026, 8, 3, 12, 0, 0)
        .single()
        .ok_or(Stage8bR2a7SourceAdapterError::RuntimeProfileInvalid)?;
    let timezone_offset_hours = 9 - i32::try_from(bar_close.hour())
        .map_err(|_| Stage8bR2a7SourceAdapterError::RuntimeProfileInvalid)?;
    let local_bar_close = bar_close + Duration::hours(i64::from(timezone_offset_hours));
    Ok(HybridIntradayRuntimeStrategy::new(
        HybridIntradayRuntimeConfig {
            symbol: "IMOEXF".to_owned(),
            profile: HybridIntradayProfile::ImoexfPrimaryRiskgateHigh180Lb120,
            mr_variant: MeanReversionVariant::High180,
            mr_gate_policy: MrGatePolicy::ShadowPnlLb120Positive,
            risk_gate_mode: RiskGateMode::NormalAppend,
            risk_gate_seed_file: None,
            risk_gate_ledger_key: None,
            model_session_start_time: Some((local_bar_close - Duration::minutes(10)).time()),
            model_session_end_time: Some((local_bar_close + Duration::hours(1)).time()),
            qty: 1.0,
            live_order_style: BrokerNeutralMarketOrderStyle::Market,
            tick_size: 0.5,
            marketable_limit_offset_ticks: 0,
            timezone_offset_hours,
            session_close_hour: 23,
            session_close_minute: 49,
            weekends_off: false,
            stop_end_buffer_sec: 60,
            repair_deadline_sec: 180,
            sl_escalate_timeout_sec: 30,
            max_repair_retries: 3,
            repair_backoff_base_sec: 5,
            repair_backoff_max_sec: 60,
            pending_timeout_sec: 30,
            partial_entry_fill_timeout_ms: 3_000,
            mr_config: MeanReversionConfig::default(),
            breakout_config: IntradayBreakoutConfig {
                k: 0.53,
                stop1_range: 0.51,
                stop2_range: 0.35,
                big_move_threshold: 0.025,
                min_range: 1.01,
                min_range_mode: MinRangeMode::Absolute,
                exclude_weekends: false,
                wait_hours: 0.0,
            },
            orchestrator_config: HybridOrchestratorConfig {
                breakout_eod_mode: BreakoutEodMode::SameDay,
                breakout_overnight_exit_time: NaiveTime::from_hms_opt(9, 30, 0)
                    .ok_or(Stage8bR2a7SourceAdapterError::RuntimeProfileInvalid)?,
            },
        },
    ))
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

/// Qualification-only setup.  This executable is built separately with test
/// fixtures; the accepted source-adapter executable is built without that
/// feature and only reads the resulting fixed durable layout.
#[cfg(feature = "stage8b-r2a7-controlled-qualification")]
#[doc(hidden)]
pub fn seed_stage8b_r2a7_controlled_reader(
    mode: Stage8bR2a7RunMode,
) -> Result<(), Stage8bR2a7SourceAdapterError> {
    use crate::{
        Stage8KillSwitchState, Stage8a1AcceptedExecutionConfigV1, Stage8a1CurrentControlStateV1,
    };
    use broker_core::{
        BrokerFeedFreshness, BrokerInstrumentSpec, BrokerKind, BrokerMarketSessionState,
        BrokerOrderSnapshot, BrokerStopOrderReadiness, BrokerSymbol, Exchange, InstrumentMapEntry,
        InternalSymbol, Market, OperatorArm, OrderPreflightPolicy, OrderSide, OrderStatus,
        OrderType, TimeInForce,
    };
    use runtime_durable_service::{
        stage8a4_i3_production_test_setup_in, stage8b_r2a6_cancel_production_test_setup_in,
    };
    use sha2::{Digest, Sha256};
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    if unsafe { libc::geteuid() } != STAGE8B_R2A6_SOURCE_ADAPTER_UID {
        return Err(Stage8bR2a7SourceAdapterError::WrongUid);
    }
    if !matches!(
        mode,
        Stage8bR2a7RunMode::ControlledPlace | Stage8bR2a7RunMode::ControlledCancel
    ) {
        return Err(Stage8bR2a7SourceAdapterError::ReaderInputInvalid);
    }
    let layout = mode.layout();
    fs::create_dir_all(&layout.stage7b_parent)
        .map_err(|_| Stage8bR2a7SourceAdapterError::ReaderInputInvalid)?;
    fs::create_dir_all(&layout.current_source_root)
        .map_err(|_| Stage8bR2a7SourceAdapterError::ReaderInputInvalid)?;
    fs::set_permissions(
        &layout.current_source_root,
        fs::Permissions::from_mode(0o755),
    )
    .map_err(|_| Stage8bR2a7SourceAdapterError::ReaderInputInvalid)?;
    let (setup, mut owner) = match mode {
        Stage8bR2a7RunMode::ControlledPlace => {
            stage8a4_i3_production_test_setup_in(layout.stage7b_parent.clone())
        }
        Stage8bR2a7RunMode::ControlledCancel => {
            stage8b_r2a6_cancel_production_test_setup_in(layout.stage7b_parent.clone())
        }
        Stage8bR2a7RunMode::Production => unreachable!("checked above"),
    };
    fs::create_dir_all(&layout.authority_root)
        .map_err(|_| Stage8bR2a7SourceAdapterError::ReaderInputInvalid)?;
    fs::set_permissions(&layout.authority_root, fs::Permissions::from_mode(0o700))
        .map_err(|_| Stage8bR2a7SourceAdapterError::ReaderInputInvalid)?;
    let fixed_runtime = fixed_runtime_profile(RUNTIME_PROFILE_ID)?;
    if fixed_runtime.stage5c_config_fingerprint() != setup.runtime.stage5c_config_fingerprint() {
        return Err(Stage8bR2a7SourceAdapterError::RuntimeProfileInvalid);
    }
    let account_id = match &setup.command {
        broker_core::BrokerCommand::PlaceOrder(place) => place.account_id.clone(),
        broker_core::BrokerCommand::CancelOrder(cancel) => cancel.account_id.clone(),
    };
    let instrument = setup.command_context.instrument().clone();
    let venue_symbol = instrument
        .venue_symbol
        .clone()
        .ok_or(Stage8bR2a7SourceAdapterError::ReaderInputInvalid)?;
    let now = Utc::now();
    let runtime_fingerprint = setup.runtime.stage5c_config_fingerprint();
    let operational_identity_sha256 =
        strategy_runtime_core::stage6d_operational_identity_sha256(&setup.operational_identity)
            .map_err(|_| Stage8bR2a7SourceAdapterError::ReaderInputInvalid)?
            .as_str()
            .to_owned();
    let config = Stage8a1AcceptedExecutionConfigV1 {
        schema_version: 1,
        operational_identity_sha256: operational_identity_sha256.clone(),
        runtime_config_fingerprint_sha256: runtime_fingerprint.clone(),
        broker: BrokerKind::Finam,
        strategy_instance_id: setup.command_context.attribution().strategy_id().to_owned(),
        account_id: account_id.clone(),
        instrument: instrument.clone(),
        broker_policy: OrderPreflightPolicy {
            allowed_accounts: vec![account_id.clone()],
            allowed_venue_symbols: vec![venue_symbol.clone()],
            allowed_order_types: vec![OrderType::Market, OrderType::Limit],
            allowed_time_in_force: vec![TimeInForce::Day],
            min_qty: rust_decimal::Decimal::ONE,
            qty_step: rust_decimal::Decimal::ONE,
            max_qty: rust_decimal::Decimal::new(2, 0),
            price_step: Some(rust_decimal::Decimal::new(5, 1)),
            max_market_qty: rust_decimal::Decimal::ONE,
            max_notional_per_order: Some(rust_decimal::Decimal::new(10_000, 0)),
            max_notional_per_run: Some(rust_decimal::Decimal::new(10_000, 0)),
            max_limit_deviation_bps: Some(1_000),
            max_reference_age_ms: 5_000,
            allow_cancel_by_broker_order_id_without_mapping: false,
            operator_arm: OperatorArm {
                session_id: "STAGE8B_R2A7_CONTROLLED_NO_EFFECT".to_owned(),
                armed_until: now + Duration::seconds(30),
                endpoint_calls_enabled: true,
                one_shot: true,
                endpoint_attempted: false,
                preflight_digest: runtime_fingerprint.clone(),
            },
        },
        build_sha256: "b".repeat(64),
        endpoint_policy_sha256: "e".repeat(64),
        max_arm_ttl_ms: 20_000,
        max_evidence_age_ms: 20_000,
        stage8a4_writer_issuer_public_key_hex: setup
            .operational_identity
            .stage8a4_writer_issuer_public_key_hex
            .clone(),
    };
    let config_bytes = serde_json::to_vec(&config)
        .map_err(|_| Stage8bR2a7SourceAdapterError::ReaderInputInvalid)?;
    let config_sha256 = {
        let mut hasher = Sha256::new();
        hasher.update(b"stage8a1-accepted-config-file-v1");
        hasher.update(b"\0");
        hasher.update(&config_bytes);
        format!("{:x}", hasher.finalize())
    };
    fs::write(
        layout
            .authority_root
            .join("stage8a1-accepted-execution-config.json"),
        config_bytes,
    )
    .map_err(|_| Stage8bR2a7SourceAdapterError::ReaderInputInvalid)?;
    fs::write(
        layout
            .authority_root
            .join("stage8a1-accepted-execution-config.json.sha256"),
        &config_sha256,
    )
    .map_err(|_| Stage8bR2a7SourceAdapterError::ReaderInputInvalid)?;
    let observed = Utc::now();
    let control = Stage8a1CurrentControlStateV1 {
        schema_version: 1,
        operational_identity_sha256,
        runtime_config_fingerprint_sha256: runtime_fingerprint,
        kill_switch: Stage8KillSwitchState::RunAllowed,
        durable_revision: 1,
        active_owner_count: 1,
        reconciliation_required_count: 0,
        max_orders: 1,
        consumed_orders: 0,
        observed_at: observed,
        valid_until: observed + Duration::seconds(20),
    };
    fs::write(
        layout
            .authority_root
            .join("stage8a1-current-control-state.json"),
        serde_json::to_vec(&control)
            .map_err(|_| Stage8bR2a7SourceAdapterError::ReaderInputInvalid)?,
    )
    .map_err(|_| Stage8bR2a7SourceAdapterError::ReaderInputInvalid)?;
    let signing_key_path = layout
        .authority_root
        .join("stage8a4-writer-issuer-signing-key.hex");
    fs::write(
        &signing_key_path,
        b"9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60",
    )
    .map_err(|_| Stage8bR2a7SourceAdapterError::ReaderInputInvalid)?;
    fs::set_permissions(&signing_key_path, fs::Permissions::from_mode(0o600))
        .map_err(|_| Stage8bR2a7SourceAdapterError::ReaderInputInvalid)?;

    let orders = match &setup.command {
        broker_core::BrokerCommand::PlaceOrder(_) => Vec::new(),
        broker_core::BrokerCommand::CancelOrder(cancel) => vec![BrokerOrderSnapshot {
            account_id: account_id.clone(),
            broker_order_id: Some(cancel.order_id.clone()),
            client_order_id: cancel.client_order_id.clone(),
            instrument: instrument.clone(),
            side: OrderSide::Buy,
            order_type: OrderType::Limit,
            time_in_force: Some(TimeInForce::Day),
            status: OrderStatus::New,
            lifecycle: BrokerOrderSnapshot::lifecycle_for(&OrderStatus::New),
            qty: rust_decimal::Decimal::ONE,
            filled_qty: rust_decimal::Decimal::ZERO,
            remaining_qty: Some(rust_decimal::Decimal::ONE),
            limit_price: Some(rust_decimal::Decimal::new(2_210, 0)),
            broker_asset_id: Some("ASSET_IMOEXF".to_owned()),
            board: Some("RFUD".to_owned()),
            expiration_date: None,
            source_ts: Some(observed),
            received_ts: observed,
        }],
    };
    let truth = BrokerTruthSnapshot {
        account_id,
        orders,
        positions: Vec::new(),
        cash: None,
        trades: Vec::new(),
        instruments: vec![BrokerInstrumentSpec {
            instrument: InstrumentMapEntry {
                internal_symbol: InternalSymbol(instrument.symbol.clone()),
                broker: BrokerKind::Finam,
                broker_symbol: BrokerSymbol(venue_symbol),
                exchange: Exchange::Moex,
                market: Market::Futures,
                price_step: rust_decimal::Decimal::new(5, 1),
                qty_step: rust_decimal::Decimal::ONE,
                lot_size: rust_decimal::Decimal::ONE,
                min_qty: rust_decimal::Decimal::ONE,
                step_value: rust_decimal::Decimal::ONE,
                currency: "RUB".to_owned(),
                schedule_id: "MOEX_FUT".to_owned(),
                expiration_date: None,
                is_tradable: true,
            },
            broker_asset_id: Some("ASSET_IMOEXF".to_owned()),
            board: Some("RFUD".to_owned()),
            long_initial_margin: None,
            short_initial_margin: None,
        }],
        received_ts: observed,
    };
    let freshness = || BrokerFeedFreshness {
        observed_ts: Some(observed),
        max_age_ms: 30_000,
    };
    let broker_readiness = BrokerReadinessSnapshot {
        account: freshness(),
        positions: freshness(),
        orders: freshness(),
        trades: freshness(),
        quotes: freshness(),
        instrument_spec: freshness(),
        schedule: freshness(),
        market_session: BrokerMarketSessionState::Open,
        unknown_order_count: 0,
        cash_margin_present: true,
        instrument_spec_validated: true,
        live_market_data_seen: true,
        subscription_ready: true,
        stream_or_polling_connected: true,
        event_sink_degraded: false,
        stop_order_readiness: BrokerStopOrderReadiness::UnsupportedBlocked,
    };
    let (identity, durable_command) = owner
        .recovered()
        .map_err(|_| Stage8bR2a7SourceAdapterError::DurableRequestInvalid)?
        .single_exact_dispatch_ready_request()
        .map_err(|_| Stage8bR2a7SourceAdapterError::DurableRequestInvalid)?;
    let composite_readiness = Stage7bCompositeReadinessSnapshot {
        phase: Stage7bPaperReadinessPhase::PaperReady,
        reasons: Vec::new(),
        blocked_entry_ids: Vec::new(),
        blocked_request_ids: Vec::new(),
        checked_at: observed,
    };
    publish_stage8b_r2a8_trusted_current_source_from_owner(
        mode,
        &mut owner,
        &setup.commitment_key,
        &identity,
        &durable_command,
        &setup.operational_identity,
        &config_sha256,
        &composite_readiness,
        &truth,
        &broker_readiness,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use broker_core::{
        BrokerAccountId, BrokerFeedFreshness, BrokerMarketSessionState, BrokerStopOrderReadiness,
    };
    use ed25519_dalek::{Signer, SigningKey};

    fn lower_hex(bytes: &[u8]) -> String {
        bytes.iter().map(|byte| format!("{byte:02x}")).collect()
    }

    fn ready_authority(checked_at: chrono::DateTime<Utc>) -> Stage8bCompositeReadinessAuthorityV1 {
        Stage8bCompositeReadinessAuthorityV1 {
            phase: Stage8bCompositeReadinessPhaseV1::PaperReady,
            reasons: Vec::new(),
            blocked_entry_ids: Vec::new(),
            blocked_request_ids: Vec::new(),
            checked_at,
        }
    }

    fn freshness(observed_at: chrono::DateTime<Utc>) -> broker_core::BrokerFeedFreshness {
        BrokerFeedFreshness {
            observed_ts: Some(observed_at),
            max_age_ms: 30_000,
        }
    }

    fn sample_broker_readiness(observed_at: chrono::DateTime<Utc>) -> BrokerReadinessSnapshot {
        BrokerReadinessSnapshot {
            account: freshness(observed_at),
            positions: freshness(observed_at),
            orders: freshness(observed_at),
            trades: freshness(observed_at),
            quotes: freshness(observed_at),
            instrument_spec: freshness(observed_at),
            schedule: freshness(observed_at),
            market_session: BrokerMarketSessionState::Open,
            unknown_order_count: 0,
            cash_margin_present: true,
            instrument_spec_validated: true,
            live_market_data_seen: true,
            subscription_ready: true,
            stream_or_polling_connected: true,
            event_sink_degraded: false,
            stop_order_readiness: BrokerStopOrderReadiness::UnsupportedBlocked,
        }
    }

    fn sign_source(source: &mut Stage8bR2a8TrustedCurrentSourceV2, signing_key: &SigningKey) {
        source.current_source_issuer_public_key_hex =
            lower_hex(signing_key.verifying_key().as_bytes());
        source.current_source_signature_ed25519_hex.clear();
        source.current_source_commitment_sha256.clear();
        source.current_source_commitment_sha256 =
            current_source_commitment_sha256(source).expect("source commitment");
        source.current_source_signature_ed25519_hex = lower_hex(
            &signing_key
                .sign(source.current_source_commitment_sha256.as_bytes())
                .to_bytes(),
        );
    }

    fn sample_source() -> (Stage8bR2a8TrustedCurrentSourceV2, SigningKey) {
        let now = Utc::now();
        let signing_key = SigningKey::from_bytes(&[0x31; 32]);
        let public_key = lower_hex(signing_key.verifying_key().as_bytes());
        let mut source = Stage8bR2a8TrustedCurrentSourceV2 {
            schema_version: 2,
            adapter_domain: "controlled_qualification".to_owned(),
            adapter_mode: "one_shot_recovery_reader".to_owned(),
            source_generation: u64::try_from(now.timestamp_millis()).expect("positive timestamp"),
            source_observed_at: now,
            expires_at: now + Duration::seconds(MAX_CURRENT_SOURCE_TTL_SECONDS),
            runtime_profile_id: RUNTIME_PROFILE_ID.to_owned(),
            operational_identity: Stage6dOperationalIdentityConfig {
                broker_id: "finam".to_owned(),
                strategy_instance_id: "stage8b-r2a8-r1-test".to_owned(),
                deployment_id: "stage8b-r2a8-r1-test".to_owned(),
                deployment_generation: 1,
                gateway_instance_id: "stage8b-r2a8-r1-test".to_owned(),
                instrument_map_fingerprint_sha256: "1".repeat(64),
                market_data_generation: 1,
                command_consumer_generation: 1,
                stage8a4_writer_issuer_public_key_hex: public_key,
            },
            accepted_config_sha256: "2".repeat(64),
            composite_readiness: ready_authority(now),
            broker_truth: BrokerTruthSnapshot {
                account_id: BrokerAccountId::new("ACC-R2A8-R1"),
                orders: Vec::new(),
                positions: Vec::new(),
                cash: None,
                trades: Vec::new(),
                instruments: Vec::new(),
                received_ts: now,
            },
            broker_readiness: sample_broker_readiness(now),
            current_source_commitment_sha256: String::new(),
            current_source_issuer_public_key_hex: String::new(),
            current_source_signature_ed25519_hex: String::new(),
        };
        sign_source(&mut source, &signing_key);
        (source, signing_key)
    }

    fn manifest_from_source(
        source: &Stage8bR2a8TrustedCurrentSourceV2,
        key: &Stage5gLifecycleCommitmentKey,
    ) -> Stage8bR2a7ReaderManifestV2 {
        let mut manifest = Stage8bR2a7ReaderManifestV2 {
            schema_version: 3,
            adapter_domain: source.adapter_domain.clone(),
            runtime_profile_id: source.runtime_profile_id.clone(),
            operational_identity: source.operational_identity.clone(),
            accepted_config_sha256: source.accepted_config_sha256.clone(),
            composite_readiness: source.composite_readiness.clone(),
            broker_truth: source.broker_truth.clone(),
            broker_readiness: source.broker_readiness.clone(),
            source_generation: source.source_generation,
            source_observed_at: source.source_observed_at,
            expires_at: source.expires_at,
            current_source_commitment_sha256: source.current_source_commitment_sha256.clone(),
            current_source_issuer_public_key_hex: source
                .current_source_issuer_public_key_hex
                .clone(),
            current_source_signature_ed25519_hex: source
                .current_source_signature_ed25519_hex
                .clone(),
            manifest_hmac_sha256: String::new(),
        };
        manifest.manifest_hmac_sha256 = key.stage8b_r2a7_reader_manifest_hmac_sha256(
            &reader_manifest_commitment_sha256(&manifest).expect("manifest commitment"),
        );
        manifest
    }

    #[test]
    fn fixed_profile_is_stable_and_modes_have_disjoint_roots() {
        let runtime = fixed_runtime_profile(RUNTIME_PROFILE_ID).expect("fixed profile");
        assert_eq!(runtime.stage5c_config_fingerprint().len(), 64);
        let production = Stage8bR2a7RunMode::Production.layout();
        let controlled = Stage8bR2a7RunMode::ControlledPlace.layout();
        assert_ne!(production.output_root, controlled.output_root);
        assert_eq!(
            Stage8bR2a7RunMode::ControlledCancel.adapter_domain(),
            "controlled_qualification"
        );
    }

    #[test]
    fn manifest_hmac_is_not_rebindable_to_another_domain_commitment() {
        let key = Stage5gLifecycleCommitmentKey::from_secret_bytes(&[0x5a; 32]).unwrap();
        let controlled_commitment = "a".repeat(64);
        let production_commitment = "b".repeat(64);
        let hmac = key.stage8b_r2a7_reader_manifest_hmac_sha256(&controlled_commitment);
        assert!(key.stage8b_r2a7_verify_reader_manifest_hmac_sha256(&controlled_commitment, &hmac));
        assert!(!key.stage8b_r2a7_verify_reader_manifest_hmac_sha256(&production_commitment, &hmac));
    }

    #[test]
    fn lifecycle_key_uses_exact_lower_hex_single_line_grammar() {
        let valid = "5a".repeat(32);
        assert!(parse_lifecycle_key(valid.as_bytes()).is_ok());
        assert!(parse_lifecycle_key(format!("{valid}\n").as_bytes()).is_ok());
        for invalid in [
            format!(" {valid}"),
            format!("{valid} "),
            format!("{valid}\n\n"),
            format!("{valid}\r\n"),
            valid.to_uppercase(),
        ] {
            assert!(parse_lifecycle_key(invalid.as_bytes()).is_err());
        }
    }

    #[test]
    fn lifecycle_key_custody_requires_exact_uid_gid_mode_link_and_size() {
        assert!(lifecycle_key_properties_are_exact(
            true, 8096, 8095, 0o100640, 1, 64
        ));
        assert!(lifecycle_key_properties_are_exact(
            true, 8096, 8095, 0o100640, 1, 65
        ));
        for properties in [
            (true, 8095, 8095, 0o100640, 1, 64),
            (true, 8096, 8096, 0o100640, 1, 64),
            (true, 8096, 8095, 0o100644, 1, 64),
            (true, 8096, 8095, 0o100600, 1, 64),
            (true, 8096, 8095, 0o100640, 2, 64),
            (true, 8096, 8095, 0o100640, 1, 63),
            (true, 8096, 8095, 0o100640, 1, 66),
            (false, 8096, 8095, 0o100640, 1, 64),
        ] {
            assert!(!lifecycle_key_properties_are_exact(
                properties.0,
                properties.1,
                properties.2,
                properties.3,
                properties.4,
                properties.5,
            ));
        }
    }

    #[test]
    fn readiness_authority_rejects_every_degraded_or_blocked_semantic() {
        let now = Utc::now();
        for phase in [
            Stage8bCompositeReadinessPhaseV1::Degraded,
            Stage8bCompositeReadinessPhaseV1::Stopped,
        ] {
            let mut readiness = ready_authority(now);
            readiness.phase = phase;
            assert!(readiness.validate_ready().is_err());
        }
        for reason in [
            Stage8bCompositeReadinessReasonV1::ConsumerNotAlive,
            Stage8bCompositeReadinessReasonV1::StorageUnavailable,
            Stage8bCompositeReadinessReasonV1::SourcePollStale,
            Stage8bCompositeReadinessReasonV1::ClaimScanStale,
            Stage8bCompositeReadinessReasonV1::SettlementUnavailable,
            Stage8bCompositeReadinessReasonV1::DurablePendingEntries,
            Stage8bCompositeReadinessReasonV1::CommandLifecycleBlocked,
        ] {
            let mut readiness = ready_authority(now);
            readiness.reasons.push(reason);
            assert!(readiness.validate_ready().is_err());
        }
        let mut blocked_entry = ready_authority(now);
        blocked_entry.blocked_entry_ids.push("entry-1".to_owned());
        assert!(blocked_entry.validate_ready().is_err());
        let mut blocked_request = ready_authority(now);
        blocked_request
            .blocked_request_ids
            .push(StrategyRequestId::new(uuid::Uuid::from_u128(1)));
        assert!(blocked_request.validate_ready().is_err());
    }

    #[test]
    fn signed_readiness_mutations_change_commitment_and_fail_verification() {
        let (source, _) = sample_source();
        assert!(
            validate_trusted_current_source(&source, Stage8bR2a7RunMode::ControlledPlace).is_ok()
        );
        let signed_commitment = source.current_source_commitment_sha256.clone();
        let mutations = [
            {
                let mut value = source.clone();
                value.composite_readiness.phase = Stage8bCompositeReadinessPhaseV1::Degraded;
                value
            },
            {
                let mut value = source.clone();
                value
                    .composite_readiness
                    .reasons
                    .push(Stage8bCompositeReadinessReasonV1::ConsumerNotAlive);
                value
            },
            {
                let mut value = source.clone();
                value
                    .composite_readiness
                    .blocked_entry_ids
                    .push("entry-1".to_owned());
                value
            },
            {
                let mut value = source.clone();
                value
                    .composite_readiness
                    .blocked_request_ids
                    .push(StrategyRequestId::new(uuid::Uuid::from_u128(2)));
                value
            },
        ];
        let public_key = VerifyingKey::from_bytes(
            &decode_lower_hex::<32>(&source.current_source_issuer_public_key_hex).unwrap(),
        )
        .unwrap();
        let signature = Signature::from_bytes(
            &decode_lower_hex::<64>(&source.current_source_signature_ed25519_hex).unwrap(),
        );
        for mutation in mutations {
            let mutated_commitment = current_source_commitment_sha256(&mutation).unwrap();
            assert_ne!(mutated_commitment, signed_commitment);
            assert!(public_key
                .verify(mutated_commitment.as_bytes(), &signature)
                .is_err());
            assert!(validate_trusted_current_source(
                &mutation,
                Stage8bR2a7RunMode::ControlledPlace
            )
            .is_err());
        }
    }

    #[test]
    fn manifest_binds_readiness_and_preserves_exact_semantics() {
        let (source, _) = sample_source();
        let key = Stage5gLifecycleCommitmentKey::from_secret_bytes(&[0x5a; 32]).unwrap();
        let manifest = manifest_from_source(&source, &key);
        assert!(validate_manifest(&manifest, Stage8bR2a7RunMode::ControlledPlace, &key).is_ok());
        let reconstructed = trusted_current_source_from_manifest(&manifest);
        assert_eq!(
            reconstructed.composite_readiness,
            source.composite_readiness
        );
        assert_eq!(
            reconstructed.composite_readiness.to_snapshot(),
            source.composite_readiness.to_snapshot()
        );

        let mut rebound = manifest.clone();
        rebound.composite_readiness.checked_at += Duration::milliseconds(1);
        assert!(validate_manifest(&rebound, Stage8bR2a7RunMode::ControlledPlace, &key).is_err());
    }

    #[test]
    fn cross_source_staleness_is_fail_closed() {
        let (mut stale_composite, signing_key) = sample_source();
        let now = Utc::now();
        stale_composite.composite_readiness.checked_at = now - Duration::seconds(31);
        stale_composite.source_observed_at = stale_composite.composite_readiness.checked_at;
        stale_composite.expires_at =
            stale_composite.source_observed_at + Duration::seconds(MAX_CURRENT_SOURCE_TTL_SECONDS);
        sign_source(&mut stale_composite, &signing_key);
        assert!(validate_trusted_current_source(
            &stale_composite,
            Stage8bR2a7RunMode::ControlledPlace
        )
        .is_err());

        let (mut stale_broker, signing_key) = sample_source();
        stale_broker.broker_truth.received_ts = now - Duration::seconds(31);
        stale_broker.source_observed_at = stale_broker.broker_truth.received_ts;
        stale_broker.expires_at =
            stale_broker.source_observed_at + Duration::seconds(MAX_CURRENT_SOURCE_TTL_SECONDS);
        sign_source(&mut stale_broker, &signing_key);
        assert!(validate_trusted_current_source(
            &stale_broker,
            Stage8bR2a7RunMode::ControlledPlace
        )
        .is_err());
    }
}
