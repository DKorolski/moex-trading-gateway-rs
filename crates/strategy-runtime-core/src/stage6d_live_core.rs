//! Stage 6D live-core restart integration.
//!
//! This module binds the authenticated Stage 5G clean-restart package to the
//! accepted Stage 6B journal checkpoint and then delegates lifecycle recovery
//! to the accepted Stage 6C replay engine.  It deliberately owns no Redis,
//! FINAM, network dispatch, runtime-live or real-order surface.

use crate::runtime_compat::Strategy;
use crate::stage5g_clean_restart::{
    restore_stage5g_clean_restart, Stage5gCleanRestartError, Stage5gCleanRestartedCapability,
    Stage5gLifecycleCommitmentKey,
};
use crate::stage5g_fresh_broker_truth::{
    apply_stage5g_fresh_truth_reduction, authorize_stage5g_fresh_truth_operational_identity,
    bind_stage5g_fresh_truth_to_clean_restart, reduce_stage5g_fresh_broker_truth,
    stage5g_review_operational_identity_for_stage6d, validate_stage5g_fresh_broker_truth_package,
    Stage5gFreshBrokerTruthError, Stage5gFreshBrokerTruthPackageV1,
    Stage5gFreshBrokerTruthValidationContext, Stage5gFreshTruthApplicationResult,
    Stage5gOperationalIdentityInput, Stage5gReconciledFreshPackageIdentity,
    Stage5gValidatedFreshBrokerTruthPackage, STAGE5G_FRESH_BROKER_TRUTH_SCHEMA_VERSION,
};
use crate::stage5g_order_position::stage5g_attribution_fingerprint_sha256;
use crate::stage6_durable_identity::Stage6JournalPayloadV1;
use crate::{
    HybridIntradayRuntimeStrategy, Stage6CancelOutcomeV1, Stage6DurableActionKind,
    Stage6DurableCommandSnapshotV1, Stage6DurableIdentityError, Stage6DurablePlaceOrderShapeV1,
    Stage6DurableRequestIdentityV1, Stage6JournalBackend, Stage6JournalCheckpointV1,
    Stage6JournalEventKind, Stage6JournalFrontierV1, Stage6JournalRecordId, Stage6JournalRecordV1,
    Stage6JournalRecordV2, Stage6JournalRecordVersioned, Stage6JournalStorageError,
    Stage6LifecycleSequence, Stage6MemoryJournalBackend, Stage6MixedReplayEngineV2,
    Stage6OwnedJournalBackend, Stage6ReconciliationBatchCompletionV2,
    Stage6ReconciliationDispositionV1, Stage6ReconciliationV2Error, Stage6RecoveredRequestV1,
    Stage6ReplayEngineV1, Stage6ReplayError, Stage6ReplaySnapshotV1,
    Stage6RequestFinalDispositionV1, Stage6Sha256Digest,
};
use broker_core::{
    BrokerCommand, BrokerOrderId, BrokerOrderSnapshot, BrokerPositionSnapshot, BrokerTradeId,
    BrokerTradeSnapshot, ClientOrderId, HybridRuntimeAttribution, InstrumentId, StrategyRequestId,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::atomic::{AtomicU64, Ordering};

pub const STAGE6D_AUTHENTICATED_RESTART_SCHEMA_VERSION: u16 = 1;
pub const STAGE6D_INTEGRATION_FINGERPRINT_SCHEMA_VERSION: u16 = 3;
pub const STAGE6E_ACCEPTED_FRESH_TRUTH_SCHEMA_VERSION: u16 = 2;

const STAGE6D_RESTART_COMMITMENT_DOMAIN: &str = "moex.stage6d.authenticated-restart-frontier.v1";
const STAGE6D_INTEGRATION_FINGERPRINT_DOMAIN: &str = "moex.stage6e-r1.durable-runtime-recovered.v3";
const STAGE6E_SEMANTIC_CROSS_BINDING_DOMAIN: &str =
    "moex.stage6e.stage5-stage6-semantic-cross-binding.v1";
const STAGE6E_RESTORE_EPOCH_DOMAIN: &str = "moex.stage6e-r1.current-process-restore-epoch.v1";

static STAGE6E_PROCESS_GENERATION_COUNTER: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage6dBootMode {
    FirstBoot,
    Restart,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stage6dLiveCoreError {
    FirstBootNotAuthorized,
    FirstBootRuntimeConfigMismatch,
    FirstBootJournalNotEmpty,
    RestartJournalMissing,
    RestartPackageDecode,
    RestartPackageNonCanonical,
    UnsupportedRestartPackageSchema,
    Stage5gPackageDigestMismatch,
    CheckpointDigestMismatch,
    RestartPackageBindingMismatch,
    RestartCommitmentMismatch,
    RestartAuthenticationFailed,
    Stage5gRestart(Stage5gCleanRestartError),
    Journal(Stage6JournalStorageError),
    Replay(Stage6ReplayError),
    ReconciliationV2(Stage6ReconciliationV2Error),
    IntegrationFingerprint,
    DurableIdentity(Stage6DurableIdentityError),
    AcceptedRecordRequired,
    DispatchAttemptRecordRequired,
    DurableOrderingViolation,
    PaperOutcomeActionMismatch,
    OperationalIdentityInvalid,
    Stage5gFreshTruthRejected,
    RestartRuntimeRequired,
    RestartRequestIdentityMismatch,
    RestartBrokerTruthMismatch,
    RestartSemanticCrossBindingMismatch,
    AcceptedFreshTruthBindingMismatch,
    FreshTruthRequestNotCrossBound,
    FreshTruthTemporalAuthorityMismatch,
}

/// Trusted, process-local context for Stage 7A command admission. Redis may
/// carry the command envelope, but it is never trusted to invent the missing
/// instrument/attribution of a cancel command or to redefine a place command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stage7aPaperCommandContext {
    instrument: InstrumentId,
    attribution: HybridRuntimeAttribution,
}

impl Stage7aPaperCommandContext {
    pub fn new(instrument: InstrumentId, attribution: HybridRuntimeAttribution) -> Self {
        Self {
            instrument,
            attribution,
        }
    }

    pub fn instrument(&self) -> &InstrumentId {
        &self.instrument
    }

    pub fn attribution(&self) -> &HybridRuntimeAttribution {
        &self.attribution
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage7aPaperPolicyRejection {
    Expired,
    UnsupportedCommandShape,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage7aPaperHoldReason {
    IdentityConflict,
    ConflictingDuplicate,
    AnotherLifecycleUnresolved,
    ReconciliationRequired,
    DurableFrontierConflict,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Stage7aPaperAdmissionDecision {
    pub strategy_request_id: StrategyRequestId,
    pub durable_client_order_id: ClientOrderId,
    pub broker_order_id: Option<BrokerOrderId>,
}

/// The only Stage 7A admission result. `DispatchReady` contains the existing
/// linear Stage 6 receipt; no Redis transport identifier appears in this API.
pub enum Stage7aPaperAdmission {
    DispatchReady(Box<Stage6dPaperDispatchReceipt>),
    Duplicate(Stage7aPaperAdmissionDecision),
    PolicyRejected {
        decision: Stage7aPaperAdmissionDecision,
        reason: Stage7aPaperPolicyRejection,
    },
    Hold {
        decision: Stage7aPaperAdmissionDecision,
        reason: Stage7aPaperHoldReason,
    },
}

impl std::fmt::Display for Stage6dLiveCoreError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::FirstBootNotAuthorized => "Stage 6D first boot is not explicitly authorized",
            Self::FirstBootRuntimeConfigMismatch => {
                "Stage 6D first-boot runtime config fingerprint mismatch"
            }
            Self::FirstBootJournalNotEmpty => "Stage 6D first-boot journal is not empty",
            Self::RestartJournalMissing => "Stage 6D restart journal is missing",
            Self::RestartPackageDecode => "Stage 6D restart package decode failed",
            Self::RestartPackageNonCanonical => "Stage 6D restart package is not canonical",
            Self::UnsupportedRestartPackageSchema => "unsupported Stage 6D restart package schema",
            Self::Stage5gPackageDigestMismatch => "Stage 5G restart package digest mismatch",
            Self::CheckpointDigestMismatch => "Stage 6 checkpoint digest mismatch",
            Self::RestartPackageBindingMismatch => {
                "Stage 6D restart package binding does not match the live owner"
            }
            Self::RestartCommitmentMismatch => "Stage 6D restart commitment mismatch",
            Self::RestartAuthenticationFailed => "Stage 6D restart authentication failed",
            Self::Stage5gRestart(_) => "authenticated Stage 5G restart failed",
            Self::Journal(_) => "Stage 6 journal validation failed",
            Self::Replay(_) => "Stage 6 deterministic replay failed",
            Self::ReconciliationV2(_) => "Stage 6 mixed V1/V2 replay failed",
            Self::IntegrationFingerprint => "Stage 6D integration fingerprint failed",
            Self::DurableIdentity(_) => "Stage 6 durable record construction failed",
            Self::AcceptedRecordRequired => "Stage 6D requires RequestAccepted first",
            Self::DispatchAttemptRecordRequired => {
                "Stage 6D requires DispatchAttemptRecorded second"
            }
            Self::DurableOrderingViolation => "Stage 6D durable-before-effect ordering violation",
            Self::PaperOutcomeActionMismatch => "Stage 6D paper outcome action mismatch",
            Self::OperationalIdentityInvalid => "Stage 6D operational identity is invalid",
            Self::Stage5gFreshTruthRejected => "Stage 5G fresh broker truth was rejected",
            Self::RestartRuntimeRequired => {
                "Stage 6D fresh-truth application requires restart authority"
            }
            Self::RestartRequestIdentityMismatch => {
                "Stage 5 and Stage 6 durable request identities do not match"
            }
            Self::RestartBrokerTruthMismatch => {
                "Stage 6 journal facts are not represented by Stage 5 broker truth"
            }
            Self::RestartSemanticCrossBindingMismatch => {
                "Stage 5 and Stage 6 restart authorities are not semantically cross-bound"
            }
            Self::AcceptedFreshTruthBindingMismatch => {
                "accepted fresh broker truth is not bound to the recovered durable authority"
            }
            Self::FreshTruthRequestNotCrossBound => {
                "fresh broker truth request is not an active Stage 5/Stage 6 cross-bound request"
            }
            Self::FreshTruthTemporalAuthorityMismatch => {
                "fresh broker truth is outside the current process restore/validation epoch"
            }
        })
    }
}

impl std::error::Error for Stage6dLiveCoreError {}

impl From<Stage6JournalStorageError> for Stage6dLiveCoreError {
    fn from(value: Stage6JournalStorageError) -> Self {
        Self::Journal(value)
    }
}

impl From<Stage6ReplayError> for Stage6dLiveCoreError {
    fn from(value: Stage6ReplayError) -> Self {
        Self::Replay(value)
    }
}

impl From<Stage6ReconciliationV2Error> for Stage6dLiveCoreError {
    fn from(value: Stage6ReconciliationV2Error) -> Self {
        Self::ReconciliationV2(value)
    }
}

impl From<Stage5gCleanRestartError> for Stage6dLiveCoreError {
    fn from(value: Stage5gCleanRestartError) -> Self {
        Self::Stage5gRestart(value)
    }
}

impl From<Stage6DurableIdentityError> for Stage6dLiveCoreError {
    fn from(value: Stage6DurableIdentityError) -> Self {
        Self::DurableIdentity(value)
    }
}

impl From<Stage5gFreshBrokerTruthError> for Stage6dLiveCoreError {
    fn from(_value: Stage5gFreshBrokerTruthError) -> Self {
        Self::Stage5gFreshTruthRejected
    }
}

#[derive(Debug, Clone)]
pub struct Stage6dFirstBootConfig {
    pub deployment_id: String,
    pub expected_runtime_config_fingerprint_sha256: String,
    pub allow_create_missing_journal: bool,
}

/// Operational identity that is authenticated together with the Stage 5G
/// package and Stage 6 frontier. Account, strategy, instrument and runtime
/// config are deliberately absent: they are derived from the restored Stage
/// 5 authority and cannot be caller-overridden.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stage6dOperationalIdentityConfig {
    pub broker_id: String,
    pub strategy_instance_id: String,
    pub deployment_id: String,
    pub deployment_generation: u64,
    pub gateway_instance_id: String,
    pub instrument_map_fingerprint_sha256: String,
    pub market_data_generation: u64,
    pub command_consumer_generation: u64,
}

/// Returns the canonical digest used to bind durable storage to an already
/// authenticated operational identity. Invalid identity input fails before a
/// storage path or writer lock may be opened.
pub fn stage6d_operational_identity_sha256(
    config: &Stage6dOperationalIdentityConfig,
) -> Result<Stage6Sha256Digest, Stage6dLiveCoreError> {
    validate_operational_identity_config(config)?;
    let bytes =
        serde_json::to_vec(config).map_err(|_| Stage6dLiveCoreError::OperationalIdentityInvalid)?;
    Stage6Sha256Digest::parse(sha256_hex(&bytes))
        .map_err(|_| Stage6dLiveCoreError::OperationalIdentityInvalid)
}

/// Linear authorization proving that journal creation was an explicit boot
/// decision. It has no `Clone`, `Copy`, `Serialize` or `Deserialize`.
pub struct Stage6dFirstBootAuthorization {
    deployment_id: String,
    expected_runtime_config_fingerprint_sha256: Stage6Sha256Digest,
}

impl Stage6dFirstBootAuthorization {
    pub fn authorizes_deployment(&self, deployment_id: &str) -> bool {
        self.deployment_id == deployment_id
    }

    /// Allows a composition owner to preserve the accepted first-boot
    /// runtime/config check while it validates a source-produced Stage 5G
    /// seed through the authenticated restart path.
    pub fn authorizes_runtime_config_fingerprint(&self, fingerprint_sha256: &str) -> bool {
        self.expected_runtime_config_fingerprint_sha256.as_str() == fingerprint_sha256
    }
}

pub fn authorize_stage6d_first_boot(
    config: Stage6dFirstBootConfig,
) -> Result<Stage6dFirstBootAuthorization, Stage6dLiveCoreError> {
    if !config.allow_create_missing_journal || config.deployment_id.trim().is_empty() {
        return Err(Stage6dLiveCoreError::FirstBootNotAuthorized);
    }
    let expected_runtime_config_fingerprint_sha256 =
        Stage6Sha256Digest::parse(config.expected_runtime_config_fingerprint_sha256)
            .map_err(|_| Stage6dLiveCoreError::FirstBootNotAuthorized)?;
    Ok(Stage6dFirstBootAuthorization {
        deployment_id: config.deployment_id,
        expected_runtime_config_fingerprint_sha256,
    })
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Stage6dAuthenticatedRestartPackageV1 {
    schema_version: u16,
    stage5g_restart_package: Vec<u8>,
    stage5g_restart_package_sha256: String,
    stage6_checkpoint: Stage6JournalCheckpointV1,
    stage6_checkpoint_bytes_sha256: String,
    operational_identity: Stage6dOperationalIdentityConfig,
    operational_identity_sha256: String,
    restart_commitment_sha256: String,
    restart_commitment_hmac_sha256: String,
}

#[derive(Serialize)]
struct Stage6dRestartCommitmentV1<'a> {
    schema_version: u16,
    domain: &'static str,
    stage5g_restart_package_sha256: &'a str,
    stage6_checkpoint_bytes_sha256: &'a str,
    operational_identity_sha256: &'a str,
}

/// Adds a versioned authenticated Stage 6 frontier to an already authenticated
/// Stage 5G restart package. The raw key and any unsealed checkpoint are never
/// serialized beside the resulting bytes.
pub fn seal_stage6d_restart_package(
    stage5g_restart_package: &[u8],
    stage6_checkpoint: Stage6JournalCheckpointV1,
    operational_identity: Stage6dOperationalIdentityConfig,
    commitment_key: &Stage5gLifecycleCommitmentKey,
) -> Result<Vec<u8>, Stage6dLiveCoreError> {
    if stage5g_restart_package.is_empty() {
        return Err(Stage6dLiveCoreError::RestartPackageDecode);
    }
    let checkpoint_bytes = stage6_checkpoint.encode_canonical();
    Stage6JournalCheckpointV1::decode_canonical(&checkpoint_bytes)?;
    let stage5g_restart_package_sha256 = sha256_hex(stage5g_restart_package);
    let stage6_checkpoint_bytes_sha256 = sha256_hex(&checkpoint_bytes);
    let operational_identity_sha256 = stage6d_operational_identity_sha256(&operational_identity)?
        .as_str()
        .to_string();
    let restart_commitment_sha256 = restart_commitment_sha256(
        &stage5g_restart_package_sha256,
        &stage6_checkpoint_bytes_sha256,
        &operational_identity_sha256,
    )?;
    let restart_commitment_hmac_sha256 =
        commitment_key.stage6d_hmac_sha256(&restart_commitment_sha256);
    let package = Stage6dAuthenticatedRestartPackageV1 {
        schema_version: STAGE6D_AUTHENTICATED_RESTART_SCHEMA_VERSION,
        stage5g_restart_package: stage5g_restart_package.to_vec(),
        stage5g_restart_package_sha256,
        stage6_checkpoint,
        stage6_checkpoint_bytes_sha256,
        operational_identity,
        operational_identity_sha256,
        restart_commitment_sha256,
        restart_commitment_hmac_sha256,
    };
    serde_json::to_vec(&package).map_err(|_| Stage6dLiveCoreError::RestartPackageDecode)
}

/// Authenticates the currently committed Stage 6 restart package and advances
/// only its journal checkpoint while preserving the exact embedded Stage 5G
/// authority and operational identity. This is the Stage 7B seal-advance seam;
/// callers cannot substitute a different Stage 5 package.
pub fn advance_stage6d_restart_package(
    authenticated_restart_package: &[u8],
    expected_current_checkpoint: &Stage6JournalCheckpointV1,
    next_checkpoint: Stage6JournalCheckpointV1,
    expected_operational_identity: &Stage6dOperationalIdentityConfig,
    commitment_key: &Stage5gLifecycleCommitmentKey,
) -> Result<Vec<u8>, Stage6dLiveCoreError> {
    let current =
        decode_and_authenticate_restart_package(authenticated_restart_package, commitment_key)?;
    if &current.stage6_checkpoint != expected_current_checkpoint
        || &current.operational_identity != expected_operational_identity
    {
        return Err(Stage6dLiveCoreError::RestartPackageBindingMismatch);
    }
    seal_stage6d_restart_package(
        &current.stage5g_restart_package,
        next_checkpoint,
        current.operational_identity,
        commitment_key,
    )
}

enum Stage6dStage5RuntimeAuthority {
    FirstBoot(Box<HybridIntradayRuntimeStrategy>),
    Restart(Box<Stage5gCleanRestartedCapability>),
}

#[derive(Serialize)]
struct Stage6eCrossBoundRequestWitness {
    strategy_request_id: StrategyRequestId,
    durable_client_order_id: broker_core::ClientOrderId,
    account_id: broker_core::BrokerAccountId,
    instrument: broker_core::InstrumentId,
    strategy_definition_id: String,
    attribution_fingerprint_sha256: String,
    action: Stage6DurableActionKind,
    target_broker_order_id: Option<BrokerOrderId>,
    target_order_client_order_id: Option<broker_core::ClientOrderId>,
}

/// Private proof that every current Stage 5G lifecycle slot has an exact
/// semantic peer in the accepted Stage 6 journal. Historical Stage 6 requests
/// may remain outside this set.
struct Stage6eSemanticCrossBinding {
    request_ids: Vec<StrategyRequestId>,
    fingerprint_sha256: Stage6Sha256Digest,
}

/// Current-process temporal authority created only after authenticated Stage 5
/// restore, Stage 6 checkpoint validation, replay and semantic cross-binding
/// have all succeeded. It is never decoded from the prior restart package and
/// is not constructible from broker timestamps.
struct Stage6RestoreEpoch {
    process_generation_id: Stage6Sha256Digest,
    restore_completed_at: DateTime<Utc>,
    fingerprint_sha256: Stage6Sha256Digest,
}

impl Stage6RestoreEpoch {
    fn from_current_host_process() -> Result<Self, Stage6dLiveCoreError> {
        let restore_completed_at = Utc::now();
        let counter = STAGE6E_PROCESS_GENERATION_COUNTER.fetch_add(1, Ordering::SeqCst);
        let generation_material = format!(
            "{STAGE6E_RESTORE_EPOCH_DOMAIN}\0{}\0{}\0{}",
            std::process::id(),
            restore_completed_at
                .timestamp_nanos_opt()
                .ok_or(Stage6dLiveCoreError::IntegrationFingerprint)?,
            counter,
        );
        let process_generation_id =
            Stage6Sha256Digest::parse(sha256_hex(generation_material.as_bytes()))
                .map_err(|_| Stage6dLiveCoreError::IntegrationFingerprint)?;
        Self::build(process_generation_id, restore_completed_at)
    }

    fn build(
        process_generation_id: Stage6Sha256Digest,
        restore_completed_at: DateTime<Utc>,
    ) -> Result<Self, Stage6dLiveCoreError> {
        #[derive(Serialize)]
        struct RestoreEpochFingerprint<'a> {
            schema_version: u16,
            domain: &'static str,
            process_generation_id: &'a Stage6Sha256Digest,
            restore_completed_at: DateTime<Utc>,
        }
        let bytes = serde_json::to_vec(&RestoreEpochFingerprint {
            schema_version: 1,
            domain: STAGE6E_RESTORE_EPOCH_DOMAIN,
            process_generation_id: &process_generation_id,
            restore_completed_at,
        })
        .map_err(|_| Stage6dLiveCoreError::IntegrationFingerprint)?;
        let fingerprint_sha256 = Stage6Sha256Digest::parse(sha256_hex(&bytes))
            .map_err(|_| Stage6dLiveCoreError::IntegrationFingerprint)?;
        Ok(Self {
            process_generation_id,
            restore_completed_at,
            fingerprint_sha256,
        })
    }
}

/// The only Stage 6D post-boot authority. It owns the Stage 5 runtime
/// authority, validated journal and deterministic replay snapshot together.
/// It intentionally has no `Clone`, `Debug`, `Serialize` or `Deserialize`.
pub struct Stage6dDurableRuntimeRecovered {
    boot_mode: Stage6dBootMode,
    stage5_runtime: Stage6dStage5RuntimeAuthority,
    journal: Stage6OwnedJournalBackend,
    replay: Stage6ReplaySnapshotV1,
    authenticated_checkpoint: Stage6JournalCheckpointV1,
    integration_fingerprint_sha256: Stage6Sha256Digest,
    first_boot_deployment_id: Option<String>,
    authenticated_operational_identity: Option<Stage6dOperationalIdentityConfig>,
    semantic_cross_binding: Option<Stage6eSemanticCrossBinding>,
    restore_epoch: Option<Stage6RestoreEpoch>,
}

/// Linear read-only proof that an exact command identity and command snapshot
/// are present in the recovered Stage 6 journal.  It does not expose the
/// journal record or any dispatch operation.
pub struct Stage6DurableRequestAuthorityV1 {
    identity: Stage6DurableRequestIdentityV1,
    canonical_command_sha256: Stage6Sha256Digest,
    accepted_record_id: Stage6JournalRecordId,
    dispatch_record_id: Stage6JournalRecordId,
    dispatch_sequence: u64,
    durable_frontier_sha256: String,
    runtime_config_fingerprint_sha256: String,
    authenticated_checkpoint_sha256: String,
}

#[derive(Serialize)]
struct Stage8a4DurableRequestBindingV1<'a> {
    domain: &'static str,
    identity: &'a Stage6DurableRequestIdentityV1,
    canonical_command_sha256: &'a Stage6Sha256Digest,
    accepted_record_id: &'a Stage6JournalRecordId,
    dispatch_record_id: &'a Stage6JournalRecordId,
    dispatch_sequence: u64,
    runtime_config_fingerprint_sha256: &'a str,
}

impl Stage6DurableRequestAuthorityV1 {
    pub fn identity(&self) -> &Stage6DurableRequestIdentityV1 {
        &self.identity
    }

    pub fn canonical_command_sha256(&self) -> &Stage6Sha256Digest {
        &self.canonical_command_sha256
    }

    pub fn accepted_record_id(&self) -> &Stage6JournalRecordId {
        &self.accepted_record_id
    }

    pub fn dispatch_record_id(&self) -> &Stage6JournalRecordId {
        &self.dispatch_record_id
    }

    pub fn dispatch_sequence(&self) -> u64 {
        self.dispatch_sequence
    }

    pub fn durable_frontier_sha256(&self) -> &str {
        &self.durable_frontier_sha256
    }

    pub fn runtime_config_fingerprint_sha256(&self) -> &str {
        &self.runtime_config_fingerprint_sha256
    }

    pub fn authenticated_checkpoint_sha256(&self) -> &str {
        &self.authenticated_checkpoint_sha256
    }

    /// Stable immutable binding for one accepted command and its sole durable
    /// dispatch attempt. Mutable frontier/checkpoint/seal values deliberately
    /// remain in the separate four-field pre-append CAS.
    pub fn durable_request_binding_sha256(
        &self,
    ) -> Result<Stage6Sha256Digest, Stage6dLiveCoreError> {
        let value = Stage8a4DurableRequestBindingV1 {
            domain: "moex.stage8a4.durable-request-binding.v1",
            identity: &self.identity,
            canonical_command_sha256: &self.canonical_command_sha256,
            accepted_record_id: &self.accepted_record_id,
            dispatch_record_id: &self.dispatch_record_id,
            dispatch_sequence: self.dispatch_sequence,
            runtime_config_fingerprint_sha256: &self.runtime_config_fingerprint_sha256,
        };
        let bytes =
            serde_json::to_vec(&value).map_err(|_| Stage6dLiveCoreError::IntegrationFingerprint)?;
        Stage6Sha256Digest::parse(sha256_hex(&bytes))
            .map_err(|_| Stage6dLiveCoreError::IntegrationFingerprint)
    }
}

/// Owned, non-serializable Stage 8A-4 batch admitted to the sole Stage 6
/// writer. Construction validates the exact V2 manifest against every V1
/// compatibility record but grants no storage authority by itself.
pub struct Stage6Stage8a4DurableBatch {
    transition_record: Stage6JournalRecordV2,
    suffix_records: Vec<Stage6JournalRecordV1>,
    cancel_original_target_shape: Option<Stage6DurablePlaceOrderShapeV1>,
}

impl Stage6Stage8a4DurableBatch {
    pub fn new(
        transition_record: Stage6JournalRecordV2,
        suffix_records: Vec<Stage6JournalRecordV1>,
        cancel_original_target_shape: Option<Stage6DurablePlaceOrderShapeV1>,
    ) -> Result<Self, Stage6dLiveCoreError> {
        let canonical = transition_record.encode_canonical();
        let transition_record = Stage6JournalRecordV2::decode_canonical(&canonical)?;
        let manifest = transition_record.payload().suffix_manifest().entries();
        if manifest.len() != suffix_records.len()
            || manifest
                .iter()
                .zip(&suffix_records)
                .any(|(entry, record)| !entry.matches_record(record))
        {
            return Err(Stage6dLiveCoreError::DurableOrderingViolation);
        }
        Ok(Self {
            transition_record,
            suffix_records,
            cancel_original_target_shape,
        })
    }

    pub fn transition_record(&self) -> &Stage6JournalRecordV2 {
        &self.transition_record
    }
}

/// Durable-only result. It is deliberately insufficient for ACK/readiness;
/// Stage 7B must still commit and reread a covering S1.
pub struct Stage6Stage8a4BatchAppendReceipt {
    checkpoint: Stage6JournalCheckpointV1,
    transition_was_existing: bool,
    appended_suffix_records: usize,
}

impl Stage6Stage8a4BatchAppendReceipt {
    pub fn checkpoint(&self) -> &Stage6JournalCheckpointV1 {
        &self.checkpoint
    }

    pub fn transition_was_existing(&self) -> bool {
        self.transition_was_existing
    }

    pub fn appended_suffix_records(&self) -> usize {
        self.appended_suffix_records
    }
}

impl Stage6dDurableRuntimeRecovered {
    /// Proves that `identity` and `command` are the exact accepted Stage 6
    /// durable request in this recovered owner. A merely well-formed command
    /// cannot obtain this authority.
    pub fn authorize_exact_durable_request(
        &self,
        identity: &Stage6DurableRequestIdentityV1,
        command: &Stage6DurableCommandSnapshotV1,
    ) -> Result<Stage6DurableRequestAuthorityV1, Stage6dLiveCoreError> {
        let accepted = stage7a_accepted_record(self, identity.strategy_request_id())
            .ok_or(Stage6dLiveCoreError::AcceptedRecordRequired)?;
        let accepted_command = match accepted.payload() {
            Stage6JournalPayloadV1::RequestAccepted { command } => command.as_ref(),
            _ => return Err(Stage6dLiveCoreError::AcceptedRecordRequired),
        };
        if accepted.durable_request_identity() != identity || accepted_command != command {
            return Err(Stage6dLiveCoreError::DurableOrderingViolation);
        }
        let replayed = self
            .replay
            .request(identity.strategy_request_id())
            .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?;
        if replayed.durable_client_order_id() != identity.durable_client_order_id()
            || replayed.action() != identity.action()
            || replayed.conflict_observed()
            || replayed.dispatch_attempt_count() != 1
            || replayed.dispatch_safety_state()
                != crate::Stage6DispatchSafetyStateV1::ReconciliationRequired
        {
            return Err(Stage6dLiveCoreError::DurableOrderingViolation);
        }
        let dispatch = self
            .journal
            .records()
            .iter()
            .find(|record| record.journal_record_id() == replayed.last_unique_record_id())
            .ok_or(Stage6dLiveCoreError::DispatchAttemptRecordRequired)?;
        if dispatch.event_kind() != Stage6JournalEventKind::DispatchAttemptRecorded
            || dispatch.durable_request_identity() != identity
            || dispatch.previous_record_id() != Some(accepted.journal_record_id())
            || self.journal_frontier().last_record_id() != Some(dispatch.journal_record_id())
        {
            return Err(Stage6dLiveCoreError::DurableOrderingViolation);
        }
        let runtime_config_fingerprint_sha256 = match &self.stage5_runtime {
            Stage6dStage5RuntimeAuthority::FirstBoot(runtime) => {
                runtime.stage5c_config_fingerprint()
            }
            Stage6dStage5RuntimeAuthority::Restart(restart) => {
                restart.config_fingerprint_sha256().to_string()
            }
        };
        Ok(Stage6DurableRequestAuthorityV1 {
            identity: identity.clone(),
            canonical_command_sha256: accepted.canonical_payload_sha256().clone(),
            accepted_record_id: accepted.journal_record_id().clone(),
            dispatch_record_id: dispatch.journal_record_id().clone(),
            dispatch_sequence: dispatch.lifecycle_sequence().get(),
            durable_frontier_sha256: frontier_fingerprint(self.journal_frontier())?,
            runtime_config_fingerprint_sha256,
            authenticated_checkpoint_sha256: self
                .authenticated_checkpoint
                .checkpoint_sha256()
                .to_string(),
        })
    }

    /// Reconstructs current request authority for I3 both before the V2 append
    /// and after a crash with an exact V2/suffix prefix already durable.
    pub fn authorize_stage8a4_durable_batch_source(
        &self,
        identity: &Stage6DurableRequestIdentityV1,
        command: &Stage6DurableCommandSnapshotV1,
    ) -> Result<Stage6DurableRequestAuthorityV1, Stage6dLiveCoreError> {
        let accepted = stage7a_accepted_record(self, identity.strategy_request_id())
            .ok_or(Stage6dLiveCoreError::AcceptedRecordRequired)?;
        let accepted_command = match accepted.payload() {
            Stage6JournalPayloadV1::RequestAccepted { command } => command.as_ref(),
            _ => return Err(Stage6dLiveCoreError::AcceptedRecordRequired),
        };
        if accepted.durable_request_identity() != identity || accepted_command != command {
            return Err(Stage6dLiveCoreError::DurableOrderingViolation);
        }
        let replayed = self
            .replay
            .request(identity.strategy_request_id())
            .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?;
        if replayed.durable_client_order_id() != identity.durable_client_order_id()
            || replayed.action() != identity.action()
            || replayed.conflict_observed()
            || replayed.dispatch_attempt_count() != 1
        {
            return Err(Stage6dLiveCoreError::DurableOrderingViolation);
        }
        let mut dispatches = self.journal.records().iter().filter(|record| {
            record.event_kind() == Stage6JournalEventKind::DispatchAttemptRecorded
                && record.durable_request_identity() == identity
        });
        let dispatch = dispatches
            .next()
            .ok_or(Stage6dLiveCoreError::DispatchAttemptRecordRequired)?;
        if dispatches.next().is_some()
            || dispatch.previous_record_id() != Some(accepted.journal_record_id())
        {
            return Err(Stage6dLiveCoreError::DurableOrderingViolation);
        }
        let runtime_config_fingerprint_sha256 = match &self.stage5_runtime {
            Stage6dStage5RuntimeAuthority::FirstBoot(runtime) => {
                runtime.stage5c_config_fingerprint()
            }
            Stage6dStage5RuntimeAuthority::Restart(restart) => {
                restart.config_fingerprint_sha256().to_string()
            }
        };
        Ok(Stage6DurableRequestAuthorityV1 {
            identity: identity.clone(),
            canonical_command_sha256: accepted.canonical_payload_sha256().clone(),
            accepted_record_id: accepted.journal_record_id().clone(),
            dispatch_record_id: dispatch.journal_record_id().clone(),
            dispatch_sequence: dispatch.lifecycle_sequence().get(),
            durable_frontier_sha256: frontier_fingerprint(self.journal_frontier())?,
            runtime_config_fingerprint_sha256,
            authenticated_checkpoint_sha256: self
                .authenticated_checkpoint
                .checkpoint_sha256()
                .to_string(),
        })
    }

    /// Read-only recovery check used by the Stage 7B owner when the V2 record
    /// already exists under an older S0. It accepts only the exact canonical
    /// batch whose verified prefix is the current durable tail.
    pub fn stage8a4_batch_matches_current_tail(
        &self,
        batch: &Stage6Stage8a4DurableBatch,
    ) -> Result<bool, Stage6dLiveCoreError> {
        let mixed = Stage6MixedReplayEngineV2::replay(self.journal.versioned_records())?;
        let transition = batch.transition_record();
        Ok(mixed.reconciliation_batches().iter().any(|existing| {
            existing.transition_record().durable_request_identity()
                == transition.durable_request_identity()
                && existing.canonical_v2_record_sha256() == transition.canonical_record_sha256()
                && existing.stable_transition_key_sha256()
                    == transition.payload().stable_transition_key_sha256()
                && Some(existing.last_mixed_record_id()) == self.journal_frontier().last_record_id()
                && Some(existing.last_mixed_lifecycle_sequence())
                    == self.journal_frontier().last_lifecycle_sequence()
        }))
    }

    /// Validates the exact uncovered I3 tail after restart reconstruction and
    /// before Stage 7B commits the covering S1.
    pub fn validate_stage8a4_current_tail_authority(&self) -> Result<(), Stage6dLiveCoreError> {
        let mixed = Stage6MixedReplayEngineV2::replay(self.journal.versioned_records())?;
        let batch = mixed
            .reconciliation_batches()
            .iter()
            .find(|batch| {
                Some(batch.last_mixed_record_id()) == self.journal_frontier().last_record_id()
                    && Some(batch.last_mixed_lifecycle_sequence())
                        == self.journal_frontier().last_lifecycle_sequence()
            })
            .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?;
        let identity = batch.transition_record().durable_request_identity();
        let accepted = stage7a_accepted_record(self, identity.strategy_request_id())
            .ok_or(Stage6dLiveCoreError::AcceptedRecordRequired)?;
        let command = match accepted.payload() {
            Stage6JournalPayloadV1::RequestAccepted { command } => command.as_ref().clone(),
            _ => return Err(Stage6dLiveCoreError::AcceptedRecordRequired),
        };
        let authority = self.authorize_stage8a4_durable_batch_source(identity, &command)?;
        let transition = batch.transition_record();
        if transition.payload().durable_request_binding_sha256()
            != &authority.durable_request_binding_sha256()?
            || transition.previous_record_id() != Some(authority.dispatch_record_id())
            || transition.lifecycle_sequence().get()
                != authority
                    .dispatch_sequence()
                    .checked_add(1)
                    .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?
            || transition
                .payload()
                .pre_append_precondition()
                .expected_request_state_fingerprint()
                != &initial_request_state_fingerprint(self, &authority)?
        {
            return Err(Stage6dLiveCoreError::DurableOrderingViolation);
        }
        if identity.action() == Stage6DurableActionKind::Cancel {
            let shape = durable_cancel_original_shape(self, identity)?;
            if let Some(order) = transition.payload().broker_order_fact() {
                if order.broker_order_id() != identity.target_broker_order_id()
                    || identity
                        .target_order_client_order_id()
                        .is_some_and(|target| order.client_order_id() != Some(target))
                    || !order.matches_original_place_shape(&shape)
                {
                    return Err(Stage6dLiveCoreError::DurableOrderingViolation);
                }
            }
        }
        Ok(())
    }

    pub fn boot_mode(&self) -> Stage6dBootMode {
        self.boot_mode
    }

    pub fn journal_frontier(&self) -> &Stage6JournalFrontierV1 {
        self.journal.frontier()
    }

    pub fn authenticated_checkpoint(&self) -> &Stage6JournalCheckpointV1 {
        &self.authenticated_checkpoint
    }

    pub fn replay(&self) -> &Stage6ReplaySnapshotV1 {
        &self.replay
    }

    pub fn integration_fingerprint_sha256(&self) -> &Stage6Sha256Digest {
        &self.integration_fingerprint_sha256
    }

    pub fn first_boot_deployment_id(&self) -> Option<&str> {
        self.first_boot_deployment_id.as_deref()
    }

    pub fn authenticated_operational_identity(&self) -> Option<&Stage6dOperationalIdentityConfig> {
        self.authenticated_operational_identity.as_ref()
    }

    pub fn semantic_cross_binding_fingerprint_sha256(&self) -> Option<&Stage6Sha256Digest> {
        self.semantic_cross_binding
            .as_ref()
            .map(|binding| &binding.fingerprint_sha256)
    }

    pub fn active_cross_bound_request_ids(&self) -> &[StrategyRequestId] {
        self.semantic_cross_binding
            .as_ref()
            .map_or(&[], |binding| binding.request_ids.as_slice())
    }

    pub fn current_process_generation_id(&self) -> Option<&str> {
        self.restore_epoch
            .as_ref()
            .map(|epoch| epoch.process_generation_id.as_str())
    }

    pub fn current_restore_completed_at(&self) -> Option<DateTime<Utc>> {
        self.restore_epoch
            .as_ref()
            .map(|epoch| epoch.restore_completed_at)
    }

    pub fn redis_command_consumer_attached(&self) -> bool {
        false
    }

    pub fn finam_transport_attached(&self) -> bool {
        false
    }

    pub fn broker_network_dispatch_attached(&self) -> bool {
        false
    }

    pub fn runtime_live_attached(&self) -> bool {
        false
    }

    pub fn real_orders_enabled(&self) -> bool {
        false
    }

    pub fn journal_is_file_backed(&self) -> bool {
        self.journal.is_file_backed()
    }

    pub(crate) fn journal_mut(&mut self) -> &mut Stage6OwnedJournalBackend {
        &mut self.journal
    }

    pub(crate) fn refresh_after_append(&mut self) -> Result<(), Stage6dLiveCoreError> {
        self.replay = replay_versioned_journal(&self.journal)?;
        self.authenticated_checkpoint =
            Stage6JournalCheckpointV1::from_frontier(self.journal.frontier().clone())?;
        self.semantic_cross_binding = match &self.stage5_runtime {
            Stage6dStage5RuntimeAuthority::Restart(restart) => Some(
                stage6e_semantic_cross_bind_restart(restart, &self.journal, &self.replay)?,
            ),
            Stage6dStage5RuntimeAuthority::FirstBoot(_) => None,
        };
        self.integration_fingerprint_sha256 = integration_fingerprint(
            self.boot_mode,
            &self.stage5_runtime,
            &self.replay,
            &self.authenticated_checkpoint,
            self.semantic_cross_binding.as_ref(),
            self.restore_epoch.as_ref(),
        )?;
        Ok(())
    }
}

fn replay_versioned_journal(
    journal: &Stage6OwnedJournalBackend,
) -> Result<Stage6ReplaySnapshotV1, Stage6dLiveCoreError> {
    let mixed = Stage6MixedReplayEngineV2::replay(journal.versioned_records())?;
    Ok(Stage6ReplaySnapshotV1::from_recovered_requests(
        mixed.into_requests(),
    ))
}

/// Applies one exact I3 durable batch through the sole recovered Stage 6
/// journal owner. The caller must hold the current Stage 7B writer lease and
/// must separately verify S0 before entry and commit/reread S1 after return.
pub fn append_stage8a4_durable_batch(
    recovered: &mut Stage6dDurableRuntimeRecovered,
    authority: Stage6DurableRequestAuthorityV1,
    batch: Stage6Stage8a4DurableBatch,
) -> Result<Stage6Stage8a4BatchAppendReceipt, Stage6dLiveCoreError> {
    append_stage8a4_durable_batch_inner(recovered, authority, batch, None)
}

#[cfg(feature = "stage5g-artifact-fixtures")]
#[doc(hidden)]
pub fn stage8a4_test_append_durable_batch_with_suffix_limit(
    recovered: &mut Stage6dDurableRuntimeRecovered,
    authority: Stage6DurableRequestAuthorityV1,
    batch: Stage6Stage8a4DurableBatch,
    suffix_limit: usize,
) -> Result<Stage6Stage8a4BatchAppendReceipt, Stage6dLiveCoreError> {
    append_stage8a4_durable_batch_inner(recovered, authority, batch, Some(suffix_limit))
}

fn append_stage8a4_durable_batch_inner(
    recovered: &mut Stage6dDurableRuntimeRecovered,
    authority: Stage6DurableRequestAuthorityV1,
    batch: Stage6Stage8a4DurableBatch,
    suffix_limit: Option<usize>,
) -> Result<Stage6Stage8a4BatchAppendReceipt, Stage6dLiveCoreError> {
    let transition = &batch.transition_record;
    if transition.durable_request_identity() != authority.identity()
        || transition.payload().durable_request_binding_sha256()
            != &authority.durable_request_binding_sha256()?
        || authority.durable_frontier_sha256 != frontier_fingerprint(recovered.journal_frontier())?
        || authority.authenticated_checkpoint_sha256
            != recovered.authenticated_checkpoint().checkpoint_sha256()
    {
        return Err(Stage6dLiveCoreError::DurableOrderingViolation);
    }
    validate_cancel_original_target_shape(recovered, &authority, &batch)?;

    let mixed = Stage6MixedReplayEngineV2::replay(recovered.journal.versioned_records())?;
    let request_id = authority.identity.strategy_request_id();
    if let Some(key_match) = mixed.reconciliation_batches().iter().find(|candidate| {
        candidate.stable_transition_key_sha256()
            == transition.payload().stable_transition_key_sha256()
    }) {
        if key_match.canonical_v2_record_sha256() != transition.canonical_record_sha256() {
            return Err(Stage6dLiveCoreError::DurableOrderingViolation);
        }
    }
    let existing = mixed.reconciliation_batches().iter().find(|candidate| {
        candidate
            .transition_record()
            .durable_request_identity()
            .strategy_request_id()
            == request_id
    });
    let precondition = transition.payload().pre_append_precondition();
    let initial_request_fingerprint = initial_request_state_fingerprint(recovered, &authority)?;
    if precondition.expected_request_state_fingerprint() != &initial_request_fingerprint {
        return Err(Stage6dLiveCoreError::DurableOrderingViolation);
    }

    let mut transition_was_existing = false;
    let suffix_prefix = match existing {
        None => {
            let current_request = mixed
                .requests()
                .iter()
                .find(|request| request.strategy_request_id() == request_id)
                .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?;
            let expected_frontier = precondition
                .expected_stage6_checkpoint_or_frontier_fingerprint()
                .as_str();
            if (expected_frontier != authority.durable_frontier_sha256()
                && expected_frontier != authority.authenticated_checkpoint_sha256())
                || current_request.state_fingerprint_sha256() != initial_request_fingerprint
                || transition.previous_record_id() != Some(authority.dispatch_record_id())
                || transition.lifecycle_sequence().get()
                    != authority
                        .dispatch_sequence()
                        .checked_add(1)
                        .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?
                || recovered.journal_frontier().last_record_id()
                    != Some(authority.dispatch_record_id())
            {
                return Err(Stage6dLiveCoreError::DurableOrderingViolation);
            }
            recovered
                .journal_mut()
                .append_versioned(&Stage6JournalRecordVersioned::V2(transition.clone()))?;
            0
        }
        Some(existing) => {
            transition_was_existing = true;
            if existing.canonical_v2_record_sha256() != transition.canonical_record_sha256()
                || existing.stable_transition_key_sha256()
                    != transition.payload().stable_transition_key_sha256()
                || existing.last_mixed_record_id()
                    != recovered
                        .journal_frontier()
                        .last_record_id()
                        .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?
                || existing.last_mixed_lifecycle_sequence()
                    != recovered
                        .journal_frontier()
                        .last_lifecycle_sequence()
                        .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?
            {
                return Err(Stage6dLiveCoreError::DurableOrderingViolation);
            }
            existing.verified_suffix_prefix_length()
        }
    };

    let mut appended_suffix_records = 0;
    let missing_suffix = batch.suffix_records.iter().skip(suffix_prefix);
    for record in missing_suffix.take(suffix_limit.unwrap_or(usize::MAX)) {
        recovered.journal_mut().append(record)?;
        appended_suffix_records += 1;
    }
    recovered.refresh_after_append()?;

    let final_mixed = Stage6MixedReplayEngineV2::replay(recovered.journal.versioned_records())?;
    let final_batch = final_mixed
        .reconciliation_batches()
        .iter()
        .find(|candidate| {
            candidate
                .transition_record()
                .durable_request_identity()
                .strategy_request_id()
                == request_id
        })
        .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?;
    if final_batch.completion() != Stage6ReconciliationBatchCompletionV2::Complete
        || final_batch.verified_suffix_prefix_length() != batch.suffix_records.len()
    {
        return Err(Stage6dLiveCoreError::DurableOrderingViolation);
    }
    Ok(Stage6Stage8a4BatchAppendReceipt {
        checkpoint: recovered.authenticated_checkpoint().clone(),
        transition_was_existing,
        appended_suffix_records,
    })
}

fn validate_cancel_original_target_shape(
    recovered: &Stage6dDurableRuntimeRecovered,
    authority: &Stage6DurableRequestAuthorityV1,
    batch: &Stage6Stage8a4DurableBatch,
) -> Result<(), Stage6dLiveCoreError> {
    match authority.identity().action() {
        Stage6DurableActionKind::Place => {
            if batch.cancel_original_target_shape.is_some() {
                return Err(Stage6dLiveCoreError::DurableOrderingViolation);
            }
            Ok(())
        }
        Stage6DurableActionKind::Cancel => {
            let expected_shape = batch
                .cancel_original_target_shape
                .as_ref()
                .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?;
            if &durable_cancel_original_shape(recovered, authority.identity())? != expected_shape {
                return Err(Stage6dLiveCoreError::DurableOrderingViolation);
            }
            let target_broker_order_id = authority
                .identity()
                .target_broker_order_id()
                .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?;
            let target_client_order_id = authority.identity().target_order_client_order_id();
            if let Some(order) = batch.transition_record.payload().broker_order_fact() {
                if order.broker_order_id() != Some(target_broker_order_id)
                    || target_client_order_id
                        .is_some_and(|target| order.client_order_id() != Some(target))
                    || !order.matches_original_place_shape(expected_shape)
                {
                    return Err(Stage6dLiveCoreError::DurableOrderingViolation);
                }
            }
            Ok(())
        }
    }
}

fn durable_cancel_original_shape(
    recovered: &Stage6dDurableRuntimeRecovered,
    cancel_identity: &Stage6DurableRequestIdentityV1,
) -> Result<Stage6DurablePlaceOrderShapeV1, Stage6dLiveCoreError> {
    let target_broker_order_id = cancel_identity
        .target_broker_order_id()
        .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?;
    let target_client_order_id = cancel_identity.target_order_client_order_id();
    let mut shapes = Vec::new();
    for accepted in recovered.journal.records().iter().filter(|record| {
        record.event_kind() == Stage6JournalEventKind::RequestAccepted
            && record.durable_request_identity().action() == Stage6DurableActionKind::Place
            && record.durable_request_identity().account_id() == cancel_identity.account_id()
            && record.durable_request_identity().instrument() == cancel_identity.instrument()
            && target_client_order_id.map_or(true, |target| {
                record.durable_request_identity().durable_client_order_id() == target
            })
    }) {
        let observed_target = recovered.journal.records().iter().any(|record| {
            record.durable_request_identity() == accepted.durable_request_identity()
                && matches!(
                    record.payload(),
                    Stage6JournalPayloadV1::BrokerOrderObserved { broker_order_id }
                        if broker_order_id == target_broker_order_id
                )
        });
        if observed_target {
            let shape = match accepted.payload() {
                Stage6JournalPayloadV1::RequestAccepted { command } => command.place_order_shape(),
                _ => None,
            }
            .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?;
            shapes.push(shape);
        }
    }
    if shapes.len() != 1 {
        return Err(Stage6dLiveCoreError::DurableOrderingViolation);
    }
    Ok(shapes.remove(0))
}

fn initial_request_state_fingerprint(
    recovered: &Stage6dDurableRuntimeRecovered,
    authority: &Stage6DurableRequestAuthorityV1,
) -> Result<Stage6Sha256Digest, Stage6dLiveCoreError> {
    let mut prefix = Vec::new();
    let mut found_dispatch = false;
    for record in recovered.journal.versioned_records() {
        if let Stage6JournalRecordVersioned::V1(v1) = record {
            prefix.push(v1.clone());
            if v1.journal_record_id() == authority.dispatch_record_id() {
                found_dispatch = true;
                break;
            }
        }
    }
    if !found_dispatch {
        return Err(Stage6dLiveCoreError::DispatchAttemptRecordRequired);
    }
    let replay = Stage6ReplayEngineV1::replay(&prefix)?;
    replay
        .request(authority.identity.strategy_request_id())
        .map(Stage6RecoveredRequestV1::state_fingerprint_sha256)
        .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)
}

pub fn first_boot_stage6d_paper(
    authorization: Stage6dFirstBootAuthorization,
    fresh_runtime: HybridIntradayRuntimeStrategy,
) -> Result<Stage6dDurableRuntimeRecovered, Stage6dLiveCoreError> {
    first_boot_stage6d_paper_with_owned_journal(
        authorization,
        fresh_runtime,
        Stage6OwnedJournalBackend::memory(),
    )
}

/// Stage 7B composition entry for transferring exactly one already-opened
/// journal authority into the recovered paper runtime.
pub fn first_boot_stage6d_paper_with_owned_journal(
    authorization: Stage6dFirstBootAuthorization,
    fresh_runtime: HybridIntradayRuntimeStrategy,
    journal: Stage6OwnedJournalBackend,
) -> Result<Stage6dDurableRuntimeRecovered, Stage6dLiveCoreError> {
    let actual = fresh_runtime.stage5c_config_fingerprint();
    if actual
        != authorization
            .expected_runtime_config_fingerprint_sha256
            .as_str()
    {
        return Err(Stage6dLiveCoreError::FirstBootRuntimeConfigMismatch);
    }
    if journal.frontier().frame_count() != 0 || !journal.records().is_empty() {
        return Err(Stage6dLiveCoreError::FirstBootJournalNotEmpty);
    }
    let replay = replay_versioned_journal(&journal)?;
    let authenticated_checkpoint =
        Stage6JournalCheckpointV1::from_frontier(journal.frontier().clone())?;
    let stage5_runtime = Stage6dStage5RuntimeAuthority::FirstBoot(Box::new(fresh_runtime));
    let integration_fingerprint_sha256 = integration_fingerprint(
        Stage6dBootMode::FirstBoot,
        &stage5_runtime,
        &replay,
        &authenticated_checkpoint,
        None,
        None,
    )?;
    Ok(Stage6dDurableRuntimeRecovered {
        boot_mode: Stage6dBootMode::FirstBoot,
        stage5_runtime,
        journal,
        replay,
        authenticated_checkpoint,
        integration_fingerprint_sha256,
        first_boot_deployment_id: Some(authorization.deployment_id),
        authenticated_operational_identity: None,
        semantic_cross_binding: None,
        restore_epoch: None,
    })
}

/// Stage 7B first-durable-boot entry after a source-produced Stage 5G seed has
/// already strict-decoded, authenticated and reconstructed against the fresh
/// runtime. The validated capability is consumed into the one Stage 6 owner;
/// no transport-derived or fabricated Stage 5 state is accepted here.
pub fn first_boot_stage6d_paper_from_validated_stage5g_seed_with_owned_journal(
    authorization: Stage6dFirstBootAuthorization,
    validated_stage5g_seed: Stage5gCleanRestartedCapability,
    journal: Stage6OwnedJournalBackend,
    operational_identity: Stage6dOperationalIdentityConfig,
) -> Result<Stage6dDurableRuntimeRecovered, Stage6dLiveCoreError> {
    if !authorization.authorizes_deployment(&operational_identity.deployment_id) {
        return Err(Stage6dLiveCoreError::FirstBootNotAuthorized);
    }
    if !authorization
        .authorizes_runtime_config_fingerprint(validated_stage5g_seed.config_fingerprint_sha256())
    {
        return Err(Stage6dLiveCoreError::FirstBootRuntimeConfigMismatch);
    }
    stage6d_operational_identity_sha256(&operational_identity)?;
    if journal.frontier().frame_count() != 0 || !journal.records().is_empty() {
        return Err(Stage6dLiveCoreError::FirstBootJournalNotEmpty);
    }
    let replay = replay_versioned_journal(&journal)?;
    let authenticated_checkpoint =
        Stage6JournalCheckpointV1::from_frontier(journal.frontier().clone())?;
    let stage5_runtime = Stage6dStage5RuntimeAuthority::Restart(Box::new(validated_stage5g_seed));
    let semantic_cross_binding = Some(stage6e_semantic_cross_bind_restart(
        match &stage5_runtime {
            Stage6dStage5RuntimeAuthority::Restart(restart) => restart,
            Stage6dStage5RuntimeAuthority::FirstBoot(_) => unreachable!(),
        },
        &journal,
        &replay,
    )?);
    let integration_fingerprint_sha256 = integration_fingerprint(
        Stage6dBootMode::FirstBoot,
        &stage5_runtime,
        &replay,
        &authenticated_checkpoint,
        semantic_cross_binding.as_ref(),
        None,
    )?;
    Ok(Stage6dDurableRuntimeRecovered {
        boot_mode: Stage6dBootMode::FirstBoot,
        stage5_runtime,
        journal,
        replay,
        authenticated_checkpoint,
        integration_fingerprint_sha256,
        first_boot_deployment_id: Some(authorization.deployment_id),
        authenticated_operational_identity: Some(operational_identity),
        semantic_cross_binding,
        restore_epoch: None,
    })
}

#[cfg(any(test, feature = "stage5g-artifact-fixtures"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[doc(hidden)]
pub enum Stage7bTestExtraStage6History {
    None,
    Finalized,
    UnboundNonFinal,
}

#[cfg(any(test, feature = "stage5g-artifact-fixtures"))]
#[doc(hidden)]
pub struct Stage7bTestRestartFixture {
    pub stage5g_authenticated_package: Vec<u8>,
    pub commitment_key: Stage5gLifecycleCommitmentKey,
    pub fresh_runtime: HybridIntradayRuntimeStrategy,
    pub journal_records: Vec<Stage6JournalRecordV1>,
    pub active_request_id: StrategyRequestId,
    pub command: BrokerCommand,
    pub command_context: Stage7aPaperCommandContext,
}

/// Source-exact Stage 5G/Stage 6 fixture used only to prove the composed
/// Stage 7B file-backed restart boundary. It exposes no execution transport.
#[cfg(any(test, feature = "stage5g-artifact-fixtures"))]
#[doc(hidden)]
pub fn stage7b_test_authenticated_working_restart_fixture(
    extra_history: Stage7bTestExtraStage6History,
) -> Stage7bTestRestartFixture {
    use broker_core::{
        BrokerAccountId, Exchange, Market, OrderSide, OrderType, PlaceOrder, TimeInForce,
    };
    use rust_decimal::Decimal;
    use uuid::Uuid;

    let (package, commitment_key, fresh_runtime, attribution) =
        crate::stage5g_order_position::tests::stage7b_authenticated_working_package_fixture();
    let restored = restore_stage5g_clean_restart(&package, &commitment_key, fresh_runtime.clone())
        .expect("Stage 7B fixture package remains source-authenticated");
    let projection = restored.fresh_truth_reducer_projection();
    let slot = projection
        .slots
        .first()
        .expect("Stage 7B working fixture retains one active slot");
    let active_request_id = StrategyRequestId::from(
        Uuid::parse_str(&slot.command_request_id).expect("Stage 7B request UUID"),
    );
    let command = PlaceOrder {
        request_id: active_request_id,
        created_ts: DateTime::from_timestamp(1_893_456_000, 0).expect("fixture timestamp"),
        ttl_ms: Some(5_000),
        account_id: projection.account_id.clone(),
        client_order_id: slot.command_client_order_id.clone(),
        instrument: projection.instrument_id.clone(),
        side: slot.side.unwrap_or(OrderSide::Buy),
        order_type: OrderType::Market,
        qty: slot.target_qty.unwrap_or(Decimal::ONE),
        limit_price: None,
        time_in_force: TimeInForce::Day,
        comment: Some(attribution.internal_comment().to_string()),
    };
    let command_context =
        Stage7aPaperCommandContext::new(projection.instrument_id.clone(), attribution.clone());
    let identity = Stage6DurableRequestIdentityV1::from_place(&command, attribution)
        .expect("Stage 7B active identity");
    let snapshot = Stage6DurableCommandSnapshotV1::from_place(&identity, &command)
        .expect("Stage 7B active command snapshot");
    let accepted = Stage6JournalRecordV1::request_accepted(
        identity.clone(),
        snapshot,
        Stage6LifecycleSequence::new(1).expect("sequence one"),
        None,
        None,
        Stage6Sha256Digest::parse("d".repeat(64)).expect("digest"),
    )
    .expect("Stage 7B active accepted record");
    let dispatch = Stage6JournalRecordV1::dispatch_attempt_recorded(
        identity,
        1,
        accepted.canonical_payload_sha256().clone(),
        Stage6LifecycleSequence::new(2).expect("sequence two"),
        Some(accepted.journal_record_id().clone()),
        Stage6Sha256Digest::parse("e".repeat(64)).expect("digest"),
    )
    .expect("Stage 7B active dispatch record");

    let mut journal_records = Vec::new();
    if extra_history != Stage7bTestExtraStage6History::None {
        let historical_request = StrategyRequestId::from(
            Uuid::parse_str("80000000-0000-0000-0000-000000000800")
                .expect("historical request UUID"),
        );
        let historical_attribution = HybridRuntimeAttribution::parse_source_comment(
            "HYB|sid=hybrid_imoexf|c=history001|o=BO|r=ENTRY",
        )
        .expect("historical attribution");
        let historical_command = PlaceOrder {
            request_id: historical_request,
            created_ts: DateTime::from_timestamp(1_893_455_000, 0).expect("historical timestamp"),
            ttl_ms: Some(5_000),
            account_id: BrokerAccountId::new("ACC_TEST_HISTORY"),
            client_order_id: ClientOrderId::from_strategy_request(historical_request),
            instrument: InstrumentId {
                symbol: "IMOEXF".to_string(),
                venue_symbol: Some("IMOEXF@RTSX".to_string()),
                exchange: Exchange::Moex,
                market: Market::Futures,
            },
            side: OrderSide::Buy,
            order_type: OrderType::Limit,
            qty: Decimal::ONE,
            limit_price: Some(Decimal::new(2200, 0)),
            time_in_force: TimeInForce::Day,
            comment: Some(historical_attribution.internal_comment().to_string()),
        };
        let historical_identity =
            Stage6DurableRequestIdentityV1::from_place(&historical_command, historical_attribution)
                .expect("historical identity");
        let historical_snapshot =
            Stage6DurableCommandSnapshotV1::from_place(&historical_identity, &historical_command)
                .expect("historical snapshot");
        let historical_accepted = Stage6JournalRecordV1::request_accepted(
            historical_identity.clone(),
            historical_snapshot,
            Stage6LifecycleSequence::new(1).expect("historical sequence one"),
            None,
            None,
            Stage6Sha256Digest::parse("a".repeat(64)).expect("digest"),
        )
        .expect("historical accepted record");
        journal_records.push(historical_accepted.clone());
        if extra_history == Stage7bTestExtraStage6History::Finalized {
            journal_records.push(
                Stage6JournalRecordV1::request_finalized(
                    historical_identity,
                    Stage6RequestFinalDispositionV1::Completed,
                    Stage6LifecycleSequence::new(2).expect("historical sequence two"),
                    Some(historical_accepted.journal_record_id().clone()),
                    Stage6Sha256Digest::parse("f".repeat(64)).expect("digest"),
                )
                .expect("historical finalized record"),
            );
        }
    }
    journal_records.extend([accepted, dispatch]);
    Stage7bTestRestartFixture {
        stage5g_authenticated_package: package,
        commitment_key,
        fresh_runtime,
        journal_records,
        active_request_id,
        command: BrokerCommand::PlaceOrder(command),
        command_context,
    }
}

/// Source-exact Stage 5G CANCEL fixture with the corresponding finalized
/// working PLACE retained as Stage 6 history. The current lifecycle contains
/// only the accepted CANCEL so restart can prove a single safe redelivery.
#[cfg(any(test, feature = "stage5g-artifact-fixtures"))]
#[doc(hidden)]
pub fn stage7b_test_authenticated_cancel_restart_fixture() -> Stage7bTestRestartFixture {
    use broker_core::{CancelOrder, OrderSide, OrderType, PlaceOrder, TimeInForce};
    use rust_decimal::Decimal;
    use uuid::Uuid;

    let (package, commitment_key, fresh_runtime, cancel_attribution) =
        crate::stage5g_order_position::tests::stage7b_authenticated_cancel_package_fixture();
    let restored = restore_stage5g_clean_restart(&package, &commitment_key, fresh_runtime.clone())
        .expect("Stage 7B cancel fixture package remains source-authenticated");
    let projection = restored.fresh_truth_reducer_projection();
    let slot = projection
        .slots
        .first()
        .expect("Stage 7B cancel fixture retains one active slot");
    let active_request_id = StrategyRequestId::from(
        Uuid::parse_str(&slot.command_request_id).expect("Stage 7B cancel request UUID"),
    );
    let target_broker_order_id = match &slot.source_action {
        crate::Stage5gMockIntentAction::Cancel { target_order_id } => target_order_id.clone(),
        crate::Stage5gMockIntentAction::Place { .. } => {
            panic!("Stage 7B cancel fixture action drift")
        }
    };
    let target_request_id = StrategyRequestId::from(
        Uuid::parse_str("80000000-0000-0000-0000-000000000805")
            .expect("fixed Stage 7B target request UUID"),
    );
    let target_client_order_id = slot
        .target_order_client_order_id
        .clone()
        .expect("Stage 7B cancel target retains its durable client id");
    assert_eq!(
        target_client_order_id,
        ClientOrderId::from_strategy_request(target_request_id),
        "Stage 5 target client id remains bound to the historical PLACE"
    );
    let historical_attribution = HybridRuntimeAttribution::parse_source_comment(
        cancel_attribution
            .internal_comment()
            .replace("|r=CANCEL", "|r=ENTRY"),
    )
    .expect("historical PLACE attribution remains canonical");
    let historical_command = PlaceOrder {
        request_id: target_request_id,
        created_ts: DateTime::from_timestamp(1_893_455_000, 0)
            .expect("historical Stage 7B timestamp"),
        ttl_ms: Some(5_000),
        account_id: projection.account_id.clone(),
        client_order_id: target_client_order_id.clone(),
        instrument: projection.instrument_id.clone(),
        side: OrderSide::Buy,
        order_type: OrderType::Limit,
        qty: Decimal::ONE,
        limit_price: Some(Decimal::new(2_210, 0)),
        time_in_force: TimeInForce::Day,
        comment: Some(historical_attribution.internal_comment().to_string()),
    };
    let historical_identity =
        Stage6DurableRequestIdentityV1::from_place(&historical_command, historical_attribution)
            .expect("Stage 7B historical PLACE identity");
    let historical_snapshot =
        Stage6DurableCommandSnapshotV1::from_place(&historical_identity, &historical_command)
            .expect("Stage 7B historical PLACE snapshot");
    let historical_accepted = Stage6JournalRecordV1::request_accepted(
        historical_identity.clone(),
        historical_snapshot,
        Stage6LifecycleSequence::new(1).expect("historical sequence one"),
        None,
        None,
        Stage6Sha256Digest::parse("1".repeat(64)).expect("digest"),
    )
    .expect("Stage 7B historical PLACE accepted record");
    let historical_dispatch = Stage6JournalRecordV1::dispatch_attempt_recorded(
        historical_identity.clone(),
        1,
        historical_accepted.canonical_payload_sha256().clone(),
        Stage6LifecycleSequence::new(2).expect("historical sequence two"),
        Some(historical_accepted.journal_record_id().clone()),
        Stage6Sha256Digest::parse("2".repeat(64)).expect("digest"),
    )
    .expect("Stage 7B historical PLACE dispatch record");
    let historical_order = Stage6JournalRecordV1::broker_order_observed(
        historical_identity.clone(),
        target_broker_order_id.clone(),
        Stage6LifecycleSequence::new(3).expect("historical sequence three"),
        Some(historical_dispatch.journal_record_id().clone()),
        Stage6Sha256Digest::parse("3".repeat(64)).expect("digest"),
    )
    .expect("Stage 7B historical working order record");
    let historical_finalized = Stage6JournalRecordV1::request_finalized(
        historical_identity,
        Stage6RequestFinalDispositionV1::Completed,
        Stage6LifecycleSequence::new(4).expect("historical sequence four"),
        Some(historical_order.journal_record_id().clone()),
        Stage6Sha256Digest::parse("4".repeat(64)).expect("digest"),
    )
    .expect("Stage 7B historical PLACE finalization");

    let cancel = CancelOrder {
        request_id: active_request_id,
        created_ts: DateTime::from_timestamp(1_893_456_000, 0).expect("fixture timestamp"),
        ttl_ms: Some(5_000),
        account_id: projection.account_id.clone(),
        order_id: target_broker_order_id,
        client_order_id: Some(target_client_order_id),
    };
    let command_context = Stage7aPaperCommandContext::new(
        projection.instrument_id.clone(),
        cancel_attribution.clone(),
    );
    let cancel_identity = Stage6DurableRequestIdentityV1::from_cancel(
        &cancel,
        projection.instrument_id.clone(),
        cancel_attribution,
    )
    .expect("Stage 7B current CANCEL identity");
    let cancel_snapshot = Stage6DurableCommandSnapshotV1::from_cancel(&cancel_identity, &cancel)
        .expect("Stage 7B current CANCEL snapshot");
    let cancel_accepted = Stage6JournalRecordV1::request_accepted(
        cancel_identity,
        cancel_snapshot,
        Stage6LifecycleSequence::new(1).expect("cancel sequence one"),
        None,
        None,
        Stage6Sha256Digest::parse("5".repeat(64)).expect("digest"),
    )
    .expect("Stage 7B current CANCEL accepted record");

    Stage7bTestRestartFixture {
        stage5g_authenticated_package: package,
        commitment_key,
        fresh_runtime,
        journal_records: vec![
            historical_accepted,
            historical_dispatch,
            historical_order,
            historical_finalized,
            cancel_accepted,
        ],
        active_request_id,
        command: BrokerCommand::CancelOrder(cancel),
        command_context,
    }
}

/// Restores only from explicitly supplied existing journal bytes. `None`
/// means missing journal and fails before Stage 5 runtime reconstruction.
pub fn restart_stage6d_paper(
    authenticated_restart_package: &[u8],
    commitment_key: &Stage5gLifecycleCommitmentKey,
    fresh_runtime: HybridIntradayRuntimeStrategy,
    existing_journal_framed_bytes: Option<Vec<u8>>,
) -> Result<Stage6dDurableRuntimeRecovered, Stage6dLiveCoreError> {
    let journal_bytes =
        existing_journal_framed_bytes.ok_or(Stage6dLiveCoreError::RestartJournalMissing)?;
    let journal = Stage6OwnedJournalBackend::from_memory(
        Stage6MemoryJournalBackend::from_framed_bytes(journal_bytes)?,
    );
    restart_stage6d_paper_with_owned_journal(
        authenticated_restart_package,
        commitment_key,
        fresh_runtime,
        journal,
    )
}

/// Stage 7B composition entry for restart from one validated journal backend.
/// No journal bytes are copied into a second writable authority.
pub fn restart_stage6d_paper_with_owned_journal(
    authenticated_restart_package: &[u8],
    commitment_key: &Stage5gLifecycleCommitmentKey,
    fresh_runtime: HybridIntradayRuntimeStrategy,
    journal: Stage6OwnedJournalBackend,
) -> Result<Stage6dDurableRuntimeRecovered, Stage6dLiveCoreError> {
    let package =
        decode_and_authenticate_restart_package(authenticated_restart_package, commitment_key)?;
    let restored = restore_stage5g_clean_restart(
        &package.stage5g_restart_package,
        commitment_key,
        fresh_runtime,
    )?;
    recover_stage6d_restart_from_authorities(
        Stage6dStage5RuntimeAuthority::Restart(Box::new(restored)),
        journal,
        package.stage6_checkpoint,
        Some(package.operational_identity),
    )
}

fn recover_stage6d_restart_from_authorities(
    stage5_runtime: Stage6dStage5RuntimeAuthority,
    journal: impl Into<Stage6OwnedJournalBackend>,
    authenticated_checkpoint: Stage6JournalCheckpointV1,
    authenticated_operational_identity: Option<Stage6dOperationalIdentityConfig>,
) -> Result<Stage6dDurableRuntimeRecovered, Stage6dLiveCoreError> {
    let journal = journal.into();
    journal.validate_checkpoint(&authenticated_checkpoint)?;
    let replay = replay_versioned_journal(&journal)?;
    let semantic_cross_binding = match &stage5_runtime {
        Stage6dStage5RuntimeAuthority::Restart(restart) => Some(
            stage6e_semantic_cross_bind_restart(restart, &journal, &replay)?,
        ),
        Stage6dStage5RuntimeAuthority::FirstBoot(_) => None,
    };
    let restore_epoch = Stage6RestoreEpoch::from_current_host_process()?;
    let integration_fingerprint_sha256 = integration_fingerprint(
        Stage6dBootMode::Restart,
        &stage5_runtime,
        &replay,
        &authenticated_checkpoint,
        semantic_cross_binding.as_ref(),
        Some(&restore_epoch),
    )?;
    Ok(Stage6dDurableRuntimeRecovered {
        boot_mode: Stage6dBootMode::Restart,
        stage5_runtime,
        journal,
        replay,
        authenticated_checkpoint,
        integration_fingerprint_sha256,
        first_boot_deployment_id: None,
        authenticated_operational_identity,
        semantic_cross_binding,
        restore_epoch: Some(restore_epoch),
    })
}

/// Explicit deterministic inputs accepted by the Stage 6D paper MVP.  Broker
/// status strings and caller-supplied evidence digests are intentionally not
/// part of this API.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "paper_outcome", rename_all = "snake_case")]
pub enum Stage6dPaperOutcome {
    MarketFilled {
        broker_order_id: BrokerOrderId,
        broker_trade_id: BrokerTradeId,
    },
    LimitPending {
        broker_order_id: BrokerOrderId,
    },
    LimitFilled {
        broker_order_id: BrokerOrderId,
        broker_trade_id: BrokerTradeId,
    },
    PlaceBrokerOrderFound {
        broker_order_id: BrokerOrderId,
    },
    PlaceNoBrokerOrderFound,
    Inconclusive,
    CancelCanceled,
    CancelExecutionObserved,
    CancelRejected,
    CancelAlreadyTerminalNonExecution,
}

/// Linear proof that both pre-effect records were durably appended.  No
/// constructor is exposed and the type is neither Clone nor serializable.
pub struct Stage6dPaperDispatchReceipt {
    identity: Stage6DurableRequestIdentityV1,
    dispatch_record_id: Stage6JournalRecordId,
    dispatch_sequence: Stage6LifecycleSequence,
    durable_frontier_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Stage6dPaperExecutionReport {
    pub strategy_request_id: StrategyRequestId,
    pub durable_client_order_id: broker_core::ClientOrderId,
    pub account_id: broker_core::BrokerAccountId,
    pub instrument: broker_core::InstrumentId,
    pub attribution: broker_core::HybridRuntimeAttribution,
    pub action: Stage6DurableActionKind,
    pub dispatch_record_id: String,
    pub durable_sequence: u64,
    pub final_record_id: String,
    pub final_sequence: u64,
    pub dispatch_safety_state: crate::Stage6DispatchSafetyStateV1,
    pub broker_order_id: Option<BrokerOrderId>,
    pub broker_trade_ids: Vec<BrokerTradeId>,
    pub cancel_outcome: Option<Stage6CancelOutcomeV1>,
    pub final_disposition: Option<crate::Stage6RequestFinalDispositionV1>,
    pub runtime_pre_fingerprint_sha256: String,
    pub runtime_post_fingerprint_sha256: String,
    pub journal_frontier_sha256: String,
    pub integration_fingerprint_sha256: String,
    pub restart_recovery_marker: bool,
}

/// Source-derived, read-only facts for one durably finalized Stage 7A request.
/// This is evidence input to the Stage 7B seal authority, not a settlement
/// capability and not a transport identity.
pub struct Stage7bFinalizedRequestFacts {
    strategy_request_id: StrategyRequestId,
    durable_client_order_id: broker_core::ClientOrderId,
    broker_order_id: Option<BrokerOrderId>,
    canonical_command_sha256: Stage6Sha256Digest,
    final_disposition: Stage6RequestFinalDispositionV1,
    final_record_id: Stage6JournalRecordId,
    final_sequence: u64,
}

impl Stage7bFinalizedRequestFacts {
    pub fn strategy_request_id(&self) -> StrategyRequestId {
        self.strategy_request_id
    }

    pub fn durable_client_order_id(&self) -> &broker_core::ClientOrderId {
        &self.durable_client_order_id
    }

    pub fn broker_order_id(&self) -> Option<&BrokerOrderId> {
        self.broker_order_id.as_ref()
    }

    pub fn canonical_command_sha256(&self) -> &Stage6Sha256Digest {
        &self.canonical_command_sha256
    }

    pub fn final_disposition(&self) -> Stage6RequestFinalDispositionV1 {
        self.final_disposition
    }

    pub fn final_record_id(&self) -> &Stage6JournalRecordId {
        &self.final_record_id
    }

    pub fn final_sequence(&self) -> u64 {
        self.final_sequence
    }
}

/// Reconstructs exact finalized request facts solely from the owned Stage 6
/// journal/replay state. Process-memory ACK caches are not consulted.
pub fn stage7b_finalized_request_facts(
    recovered: &Stage6dDurableRuntimeRecovered,
    request_id: StrategyRequestId,
) -> Result<Stage7bFinalizedRequestFacts, Stage6dLiveCoreError> {
    let accepted = stage7a_accepted_record(recovered, request_id)
        .ok_or(Stage6dLiveCoreError::AcceptedRecordRequired)?;
    let request = recovered
        .replay()
        .request(request_id)
        .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?;
    let final_disposition = request
        .final_disposition()
        .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?;
    Ok(Stage7bFinalizedRequestFacts {
        strategy_request_id: request_id,
        durable_client_order_id: request.durable_client_order_id().clone(),
        broker_order_id: request.known_broker_order_id().cloned(),
        canonical_command_sha256: accepted.canonical_payload_sha256().clone(),
        final_disposition,
        final_record_id: request.last_unique_record_id().clone(),
        final_sequence: request.last_unique_sequence(),
    })
}

/// Replays the already-owned durable journal and promotes its exact current
/// frontier into the recovered authority. It accepts no caller-supplied
/// checkpoint and performs no effect; Stage 7B uses it immediately before
/// committing a covering recovery seal after restart.
pub fn refresh_stage7b_durable_frontier(
    recovered: &mut Stage6dDurableRuntimeRecovered,
) -> Result<(), Stage6dLiveCoreError> {
    recovered.refresh_after_append()
}

impl Stage6dPaperExecutionReport {
    pub fn to_ndjson_line(&self) -> Result<String, Stage6dLiveCoreError> {
        serde_json::to_string(self).map_err(|_| Stage6dLiveCoreError::IntegrationFingerprint)
    }
}

/// Opaque normalized broker-truth capability issued by the process-local
/// paper adapter only after a durable dispatch receipt exists. It cannot be
/// constructed from a digest or raw status and is consumed by record emission.
struct Stage6dAcceptedBrokerTruth {
    receipt: Stage6dPaperDispatchReceipt,
    outcome: Stage6dPaperOutcome,
    evidence: Stage6Sha256Digest,
}

/// Complete broker-neutral truth collected after an authenticated restart.
/// It contains no evidence digest or raw broker status. Stage 6D derives the
/// operational identity from the HMAC-bound restart package and passes these
/// rows through the accepted Stage 5G validator/reducer/application boundary.
#[derive(Debug, Clone)]
pub struct Stage6ePaperFreshBrokerTruthInput {
    pub package_id: String,
    pub snapshot_epoch: String,
    /// Local collector interval start. This is not a broker/source timestamp.
    pub collection_started_at: DateTime<Utc>,
    /// Local collector interval completion passed to the accepted Stage 5G
    /// package as `captured_at`.
    pub captured_at: DateTime<Utc>,
    pub orders_observed_at: DateTime<Utc>,
    pub trades_observed_at: DateTime<Utc>,
    pub positions_observed_at: DateTime<Utc>,
    pub orders_complete: bool,
    pub trades_complete: bool,
    pub positions_complete: bool,
    pub orders: Vec<BrokerOrderSnapshot>,
    pub trades: Vec<BrokerTradeSnapshot>,
    pub positions: Vec<BrokerPositionSnapshot>,
}

/// Opaque, linear fresh-truth authority. It is issued only after the provider
/// input has passed Stage 6 replay/correlation checks and the accepted Stage
/// 5G package validator. It deliberately has no Clone, Debug, Serialize,
/// Deserialize or public constructor.
///
/// Raw observations cannot call the production application boundary:
///
/// ```compile_fail
/// use strategy_runtime_core::{
///     apply_stage6e_accepted_fresh_truth, Stage5gLifecycleCommitmentKey,
///     Stage6dDurableRuntimeRecovered, Stage6ePaperFreshBrokerTruthInput,
/// };
/// fn raw_cannot_apply(
///     recovered: Stage6dDurableRuntimeRecovered,
///     raw: Stage6ePaperFreshBrokerTruthInput,
///     key: &Stage5gLifecycleCommitmentKey,
/// ) {
///     let _ = apply_stage6e_accepted_fresh_truth(recovered, raw, key);
/// }
/// ```
///
/// The accepted capability is linear and non-serializable:
///
/// ```compile_fail
/// use strategy_runtime_core::Stage6eAcceptedFreshBrokerTruth;
/// fn cannot_clone_or_serialize(value: Stage6eAcceptedFreshBrokerTruth) {
///     let duplicate = value.clone();
///     let _ = serde_json::to_vec(&duplicate);
/// }
/// ```
pub struct Stage6eAcceptedFreshBrokerTruth {
    validated: Stage5gValidatedFreshBrokerTruthPackage,
    strategy_request_id: StrategyRequestId,
    package_id: String,
    stage6_replay_fingerprint_sha256: Stage6Sha256Digest,
    journal_frontier_sha256: String,
    authenticated_checkpoint_sha256: String,
    semantic_cross_binding_fingerprint_sha256: Stage6Sha256Digest,
    restore_epoch_fingerprint_sha256: Stage6Sha256Digest,
    validation_observed_at: DateTime<Utc>,
}

/// Broker-neutral collection seam for a future reviewed read-only provider.
/// Implementing this trait does not grant authority to issue an accepted
/// capability; provider admission remains a separate Stage 7+ gate.
pub trait Stage6eFreshBrokerTruthProviderBoundary {
    type Error;

    fn provider_id(&self) -> &str;
    fn collect_normalized_fresh_truth(
        &mut self,
    ) -> Result<Stage6ePaperFreshBrokerTruthInput, Self::Error>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Stage6dFreshTruthApplicationReport {
    pub strategy_request_id: StrategyRequestId,
    pub package_id: String,
    pub scenario_id: String,
    pub disposition: String,
    pub reason: String,
    pub runtime_transition_applied: bool,
    pub already_represented_noop: bool,
    pub stage5_pre_fingerprint_sha256: String,
    pub stage5_post_fingerprint_sha256: String,
    pub stage6_replay_fingerprint_sha256: String,
    pub integration_fingerprint_sha256: String,
}

/// Every variant returns ownership of the single durable/runtime authority.
/// A blocked classification therefore cannot accidentally leave a competing
/// continuation alive.
pub enum Stage6dFreshTruthTransition {
    Applied {
        recovered: Stage6dDurableRuntimeRecovered,
        report: Stage6dFreshTruthApplicationReport,
    },
    AlreadyRepresentedNoop {
        recovered: Stage6dDurableRuntimeRecovered,
        report: Stage6dFreshTruthApplicationReport,
    },
    Blocked {
        recovered: Stage6dDurableRuntimeRecovered,
        report: Stage6dFreshTruthApplicationReport,
    },
}

impl Stage6dFreshTruthTransition {
    pub fn recovered(&self) -> &Stage6dDurableRuntimeRecovered {
        match self {
            Self::Applied { recovered, .. }
            | Self::AlreadyRepresentedNoop { recovered, .. }
            | Self::Blocked { recovered, .. } => recovered,
        }
    }

    pub fn report(&self) -> &Stage6dFreshTruthApplicationReport {
        match self {
            Self::Applied { report, .. }
            | Self::AlreadyRepresentedNoop { report, .. }
            | Self::Blocked { report, .. } => report,
        }
    }

    pub fn into_recovered(self) -> Stage6dDurableRuntimeRecovered {
        match self {
            Self::Applied { recovered, .. }
            | Self::AlreadyRepresentedNoop { recovered, .. }
            | Self::Blocked { recovered, .. } => recovered,
        }
    }
}

/// Deterministic process-local paper issuer. Raw normalized observations are
/// admitted only after exact restart/replay correlation and the accepted
/// Stage 5G validator; the returned authority is opaque and linear.
pub fn issue_stage6e_paper_fresh_broker_truth_for_request(
    recovered: &Stage6dDurableRuntimeRecovered,
    request_id: StrategyRequestId,
    input: Stage6ePaperFreshBrokerTruthInput,
) -> Result<Stage6eAcceptedFreshBrokerTruth, Stage6dLiveCoreError> {
    issue_stage6e_paper_fresh_broker_truth_for_request_at(recovered, request_id, input, Utc::now())
}

fn issue_stage6e_paper_fresh_broker_truth_for_request_at(
    recovered: &Stage6dDurableRuntimeRecovered,
    request_id: StrategyRequestId,
    input: Stage6ePaperFreshBrokerTruthInput,
    validation_observed_at: DateTime<Utc>,
) -> Result<Stage6eAcceptedFreshBrokerTruth, Stage6dLiveCoreError> {
    let operational_config = recovered
        .authenticated_operational_identity
        .clone()
        .ok_or(Stage6dLiveCoreError::RestartRuntimeRequired)?;
    let restart = match &recovered.stage5_runtime {
        Stage6dStage5RuntimeAuthority::Restart(restart) => restart.as_ref(),
        Stage6dStage5RuntimeAuthority::FirstBoot(_) => {
            return Err(Stage6dLiveCoreError::RestartRuntimeRequired);
        }
    };
    let projection = restart.fresh_truth_reducer_projection();
    stage6d_validate_selected_restart_request(&recovered.replay, &projection, request_id)?;
    stage6d_validate_replayed_facts_against_truth(&recovered.replay, request_id, &input)?;
    let semantic_cross_binding_fingerprint_sha256 = recovered
        .semantic_cross_binding_fingerprint_sha256()
        .cloned()
        .ok_or(Stage6dLiveCoreError::RestartSemanticCrossBindingMismatch)?;
    if !recovered
        .active_cross_bound_request_ids()
        .contains(&request_id)
    {
        return Err(Stage6dLiveCoreError::FreshTruthRequestNotCrossBound);
    }
    let restore_epoch = recovered
        .restore_epoch
        .as_ref()
        .ok_or(Stage6dLiveCoreError::FreshTruthTemporalAuthorityMismatch)?;
    validate_stage6e_temporal_authority(&input, restore_epoch, validation_observed_at)?;

    let operational_identity = Stage5gOperationalIdentityInput {
        broker_id: operational_config.broker_id,
        account_id: projection.account_id.clone(),
        strategy_definition_id: projection.strategy_id.clone(),
        strategy_instance_id: operational_config.strategy_instance_id,
        deployment_id: operational_config.deployment_id,
        deployment_generation: operational_config.deployment_generation,
        gateway_instance_id: operational_config.gateway_instance_id,
        config_fingerprint_sha256: projection.config_fingerprint_sha256.clone(),
        instrument_map_fingerprint_sha256: operational_config.instrument_map_fingerprint_sha256,
        market_data_generation: operational_config.market_data_generation,
        command_consumer_generation: operational_config.command_consumer_generation,
        target_instrument: projection.instrument_id.clone(),
    };
    let reviewed =
        stage5g_review_operational_identity_for_stage6d(restart, operational_identity.clone())?;
    let operational_authority =
        authorize_stage5g_fresh_truth_operational_identity(restart, reviewed)?;
    let current_id = projection
        .checkpoint
        .payload
        .current_evidence_identity
        .clone()
        .ok_or(Stage6dLiveCoreError::RestartBrokerTruthMismatch)?;
    let current_epoch = projection
        .checkpoint
        .payload
        .package_discriminator
        .clone()
        .ok_or(Stage6dLiveCoreError::RestartBrokerTruthMismatch)?;
    let current_fingerprint = projection
        .checkpoint
        .payload
        .evidence_replay_ledger
        .iter()
        .find(|entry| entry.identity == current_id)
        .map(|entry| entry.fingerprint_sha256.clone())
        .ok_or(Stage6dLiveCoreError::RestartBrokerTruthMismatch)?;
    let last_reconciled = Stage5gReconciledFreshPackageIdentity::validate(
        current_id.clone(),
        current_epoch.clone(),
        current_fingerprint,
    )?;
    let accepted_history = stage6d_stage5g_accepted_history(&projection, &current_id)?;
    let package_id = input.package_id.clone();
    let validated = validate_stage5g_fresh_broker_truth_package(
        Stage5gFreshBrokerTruthPackageV1 {
            schema_version: STAGE5G_FRESH_BROKER_TRUTH_SCHEMA_VERSION,
            package_id: input.package_id.clone(),
            operational_identity,
            snapshot_epoch: input.snapshot_epoch,
            captured_at: input.captured_at,
            orders_observed_at: input.orders_observed_at,
            trades_observed_at: input.trades_observed_at,
            positions_observed_at: input.positions_observed_at,
            orders_complete: input.orders_complete,
            trades_complete: input.trades_complete,
            positions_complete: input.positions_complete,
            orders: input.orders,
            trades: input.trades,
            positions: input.positions,
        },
        Stage5gFreshBrokerTruthValidationContext {
            operational_authority,
            pre_restart_package_id: &current_id,
            pre_restart_snapshot_epoch: &current_epoch,
            untrusted_last_reconciled_hint: Some(&last_reconciled),
            untrusted_accepted_replay_hints: &accepted_history,
            untrusted_known_historical_hints: &[],
            clean_restore_completed_at: restore_epoch.restore_completed_at,
            validation_observed_at,
        },
    )?;
    Ok(Stage6eAcceptedFreshBrokerTruth {
        validated,
        strategy_request_id: request_id,
        package_id,
        stage6_replay_fingerprint_sha256: recovered.replay.semantic_fingerprint_sha256().clone(),
        journal_frontier_sha256: frontier_fingerprint(recovered.journal_frontier())?,
        authenticated_checkpoint_sha256: sha256_hex(
            &recovered.authenticated_checkpoint.encode_canonical(),
        ),
        semantic_cross_binding_fingerprint_sha256,
        restore_epoch_fingerprint_sha256: restore_epoch.fingerprint_sha256.clone(),
        validation_observed_at,
    })
}

/// Applies only capability-issued broker truth through the accepted Stage 5G
/// reconciliation path. Raw snapshots are not an application input.
pub fn apply_stage6e_accepted_fresh_truth(
    mut recovered: Stage6dDurableRuntimeRecovered,
    accepted: Stage6eAcceptedFreshBrokerTruth,
    commitment_key: &Stage5gLifecycleCommitmentKey,
) -> Result<Stage6dFreshTruthTransition, Stage6dLiveCoreError> {
    let current_binding = recovered
        .semantic_cross_binding_fingerprint_sha256()
        .ok_or(Stage6dLiveCoreError::AcceptedFreshTruthBindingMismatch)?;
    let current_restore_epoch = recovered
        .restore_epoch
        .as_ref()
        .ok_or(Stage6dLiveCoreError::AcceptedFreshTruthBindingMismatch)?;
    if recovered.replay.semantic_fingerprint_sha256() != &accepted.stage6_replay_fingerprint_sha256
        || frontier_fingerprint(recovered.journal_frontier())? != accepted.journal_frontier_sha256
        || sha256_hex(&recovered.authenticated_checkpoint.encode_canonical())
            != accepted.authenticated_checkpoint_sha256
        || current_binding != &accepted.semantic_cross_binding_fingerprint_sha256
        || current_restore_epoch.fingerprint_sha256 != accepted.restore_epoch_fingerprint_sha256
        || accepted.validation_observed_at < current_restore_epoch.restore_completed_at
        || !recovered
            .active_cross_bound_request_ids()
            .contains(&accepted.strategy_request_id)
    {
        return Err(Stage6dLiveCoreError::AcceptedFreshTruthBindingMismatch);
    }
    let restart = match &recovered.stage5_runtime {
        Stage6dStage5RuntimeAuthority::Restart(restart) => restart.as_ref(),
        Stage6dStage5RuntimeAuthority::FirstBoot(_) => {
            return Err(Stage6dLiveCoreError::RestartRuntimeRequired);
        }
    };
    let request_id = accepted.strategy_request_id;
    let package_id = accepted.package_id;
    let bound = bind_stage5g_fresh_truth_to_clean_restart(restart, accepted.validated)?;
    let replacement_runtime = restart.stage5g_fresh_reconstruction_candidate();

    let Stage6dStage5RuntimeAuthority::Restart(restart) = std::mem::replace(
        &mut recovered.stage5_runtime,
        Stage6dStage5RuntimeAuthority::FirstBoot(Box::new(replacement_runtime)),
    ) else {
        unreachable!("restart authority checked above")
    };
    let pre_fingerprint = restart.reconstructed_runtime_state_fingerprint_sha256();
    let reduction = reduce_stage5g_fresh_broker_truth(*restart, bound);
    match apply_stage5g_fresh_truth_reduction(reduction, commitment_key) {
        Stage5gFreshTruthApplicationResult::Applied(applied) => {
            let evidence = applied.evidence().clone();
            if evidence.command_request_id() != request_id.to_string() {
                return Err(Stage6dLiveCoreError::RestartRequestIdentityMismatch);
            }
            let post_fingerprint = applied
                .restored()
                .reconstructed_runtime_state_fingerprint_sha256();
            recovered.stage5_runtime =
                Stage6dStage5RuntimeAuthority::Restart(Box::new(applied.into_restored()));
            recovered.refresh_after_append()?;
            let report = stage6d_fresh_truth_report(
                &recovered,
                request_id,
                package_id,
                evidence.scenario_id(),
                evidence.disposition(),
                evidence.reason(),
                true,
                false,
                pre_fingerprint,
                post_fingerprint,
            );
            Ok(Stage6dFreshTruthTransition::Applied { recovered, report })
        }
        Stage5gFreshTruthApplicationResult::Continued(continued) => {
            let scenario_id = continued.scenario_id().to_string();
            let reason = format!("{:?}", continued.reason());
            recovered.stage5_runtime =
                Stage6dStage5RuntimeAuthority::Restart(Box::new(continued.into_restart()));
            recovered.refresh_after_append()?;
            let post = match &recovered.stage5_runtime {
                Stage6dStage5RuntimeAuthority::Restart(value) => {
                    value.reconstructed_runtime_state_fingerprint_sha256()
                }
                Stage6dStage5RuntimeAuthority::FirstBoot(_) => unreachable!(),
            };
            let report = stage6d_fresh_truth_report(
                &recovered,
                request_id,
                package_id,
                &scenario_id,
                "continue_from_authenticated_checkpoint",
                &reason,
                false,
                true,
                pre_fingerprint,
                post,
            );
            Ok(Stage6dFreshTruthTransition::AlreadyRepresentedNoop { recovered, report })
        }
        Stage5gFreshTruthApplicationResult::Blocked(blocked) => {
            let scenario_id = blocked.scenario_id().to_string();
            let disposition = format!("{:?}", blocked.disposition());
            let reason = format!("{:?}", blocked.reason());
            recovered.stage5_runtime =
                Stage6dStage5RuntimeAuthority::Restart(Box::new(blocked.into_restart()));
            recovered.refresh_after_append()?;
            let post = match &recovered.stage5_runtime {
                Stage6dStage5RuntimeAuthority::Restart(value) => {
                    value.reconstructed_runtime_state_fingerprint_sha256()
                }
                Stage6dStage5RuntimeAuthority::FirstBoot(_) => unreachable!(),
            };
            let report = stage6d_fresh_truth_report(
                &recovered,
                request_id,
                package_id,
                &scenario_id,
                &disposition,
                &reason,
                false,
                false,
                pre_fingerprint,
                post,
            );
            Ok(Stage6dFreshTruthTransition::Blocked { recovered, report })
        }
    }
}

/// Admits one broker-neutral command into the accepted Stage 6 authority.
///
/// The caller supplies trusted strategy context and host-observed time, but
/// cannot supply lifecycle sequences, record links, evidence digests or a
/// dispatch capability. Exact redelivery is deduplicated by Stage 6 identity;
/// ambiguity remains a hold and is never converted into a benign transport
/// poison result.
pub fn admit_stage7a_paper_command(
    recovered: &mut Stage6dDurableRuntimeRecovered,
    command: &BrokerCommand,
    context: &Stage7aPaperCommandContext,
    observed_at: DateTime<Utc>,
) -> Result<Stage7aPaperAdmission, Stage6dLiveCoreError> {
    let request_id = stage7a_request_id(command);
    let fallback_decision = Stage7aPaperAdmissionDecision {
        strategy_request_id: request_id,
        durable_client_order_id: ClientOrderId::from_strategy_request(request_id),
        broker_order_id: None,
    };
    let (identity, snapshot) = match stage7a_identity_and_snapshot(command, context) {
        Ok(value) => value,
        Err(error) => {
            let reason = match error {
                Stage6DurableIdentityError::UnsupportedDurablePlaceOrderType
                | Stage6DurableIdentityError::InvalidDurablePlacePriceShape
                | Stage6DurableIdentityError::InvalidDurablePlaceQuantity => {
                    return Ok(Stage7aPaperAdmission::PolicyRejected {
                        decision: fallback_decision,
                        reason: Stage7aPaperPolicyRejection::UnsupportedCommandShape,
                    });
                }
                _ => Stage7aPaperHoldReason::IdentityConflict,
            };
            return Ok(Stage7aPaperAdmission::Hold {
                decision: fallback_decision,
                reason,
            });
        }
    };
    let decision = Stage7aPaperAdmissionDecision {
        strategy_request_id: identity.strategy_request_id(),
        durable_client_order_id: identity.durable_client_order_id().clone(),
        broker_order_id: None,
    };

    if stage7a_command_expired(command, observed_at) {
        return Ok(Stage7aPaperAdmission::PolicyRejected {
            decision,
            reason: Stage7aPaperPolicyRejection::Expired,
        });
    }

    if let Some(accepted) = stage7a_accepted_record(recovered, request_id).cloned() {
        let exact_snapshot = match accepted.payload() {
            Stage6JournalPayloadV1::RequestAccepted { command } => command.as_ref() == &snapshot,
            _ => false,
        };
        if accepted.durable_request_identity() != &identity || !exact_snapshot {
            return Ok(Stage7aPaperAdmission::Hold {
                decision,
                reason: Stage7aPaperHoldReason::ConflictingDuplicate,
            });
        }
        let replayed = recovered
            .replay()
            .request(request_id)
            .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?;
        let duplicate_decision = Stage7aPaperAdmissionDecision {
            broker_order_id: replayed.known_broker_order_id().cloned(),
            ..decision
        };
        return match replayed.dispatch_safety_state() {
            crate::Stage6DispatchSafetyStateV1::ReadyForFirstDispatch => {
                if recovered.journal_frontier().last_record_id()
                    != Some(accepted.journal_record_id())
                {
                    Ok(Stage7aPaperAdmission::Hold {
                        decision: duplicate_decision,
                        reason: Stage7aPaperHoldReason::DurableFrontierConflict,
                    })
                } else {
                    let dispatch = stage7a_dispatch_record(&identity, &accepted, observed_at)?;
                    let receipt = prepare_stage6d_existing_accepted_paper_dispatch(
                        recovered, &accepted, dispatch,
                    )?;
                    Ok(Stage7aPaperAdmission::DispatchReady(Box::new(receipt)))
                }
            }
            crate::Stage6DispatchSafetyStateV1::DispatchForbidden => {
                Ok(Stage7aPaperAdmission::Duplicate(duplicate_decision))
            }
            crate::Stage6DispatchSafetyStateV1::ReconciliationRequired
            | crate::Stage6DispatchSafetyStateV1::RetryEligibleSameIdentity => {
                Ok(Stage7aPaperAdmission::Hold {
                    decision: duplicate_decision,
                    reason: Stage7aPaperHoldReason::ReconciliationRequired,
                })
            }
        };
    }

    if stage7a_has_other_unresolved_lifecycle(recovered, &identity) {
        return Ok(Stage7aPaperAdmission::Hold {
            decision,
            reason: Stage7aPaperHoldReason::AnotherLifecycleUnresolved,
        });
    }

    let accepted_sequence = Stage6LifecycleSequence::new(1)?;
    let accepted = Stage6JournalRecordV1::request_accepted(
        identity.clone(),
        snapshot,
        accepted_sequence,
        None,
        None,
        stage7a_source_evidence(command, observed_at, "request_accepted")?,
    )?;
    let dispatch = Stage6JournalRecordV1::dispatch_attempt_recorded(
        identity,
        1,
        accepted.canonical_payload_sha256().clone(),
        Stage6LifecycleSequence::new(accepted_sequence.get().saturating_add(1))?,
        Some(accepted.journal_record_id().clone()),
        stage7a_source_evidence(command, observed_at, "dispatch_attempt_recorded")?,
    )?;
    prepare_stage6d_paper_dispatch(recovered, accepted, dispatch)
        .map(Box::new)
        .map(Stage7aPaperAdmission::DispatchReady)
}

/// Resolves CANCEL context only from a Stage 6-correlated paper order. The
/// cancel DTO intentionally carries no strategy attribution, so transport
/// configuration cannot invent a cycle/owner for an unrelated broker order.
pub fn resolve_stage7a_cancel_command_context(
    recovered: &Stage6dDurableRuntimeRecovered,
    command: &broker_core::CancelOrder,
    expected_instrument: &InstrumentId,
    expected_strategy_id: &str,
) -> Option<Stage7aPaperCommandContext> {
    let request = recovered.replay().requests().iter().find(|request| {
        request.action() == Stage6DurableActionKind::Place
            && request.known_broker_order_id() == Some(&command.order_id)
    })?;
    let accepted = stage7a_accepted_record(recovered, request.strategy_request_id())?;
    let identity = accepted.durable_request_identity();
    if identity.account_id() != &command.account_id
        || identity.instrument() != expected_instrument
        || !identity.attribution().belongs_to(expected_strategy_id)
    {
        return None;
    }
    if let Some(target_client_order_id) = command.client_order_id.as_ref() {
        if target_client_order_id != identity.durable_client_order_id() {
            return None;
        }
    }
    let mut role_seen = false;
    let cancel_comment = identity
        .attribution()
        .internal_comment()
        .split('|')
        .map(|part| {
            if part.starts_with("r=") {
                role_seen = true;
                "r=CANCEL"
            } else {
                part
            }
        })
        .collect::<Vec<_>>()
        .join("|");
    if !role_seen {
        return None;
    }
    let attribution = HybridRuntimeAttribution::parse_source_comment(cancel_comment).ok()?;
    Some(Stage7aPaperCommandContext::new(
        expected_instrument.clone(),
        attribution,
    ))
}

fn stage7a_request_id(command: &BrokerCommand) -> StrategyRequestId {
    match command {
        BrokerCommand::PlaceOrder(command) => command.request_id,
        BrokerCommand::CancelOrder(command) => command.request_id,
    }
}

fn stage7a_identity_and_snapshot(
    command: &BrokerCommand,
    context: &Stage7aPaperCommandContext,
) -> Result<
    (
        Stage6DurableRequestIdentityV1,
        Stage6DurableCommandSnapshotV1,
    ),
    Stage6DurableIdentityError,
> {
    match command {
        BrokerCommand::PlaceOrder(command) => {
            if command.instrument != context.instrument {
                return Err(Stage6DurableIdentityError::InstrumentMismatch);
            }
            let identity =
                Stage6DurableRequestIdentityV1::from_place(command, context.attribution.clone())?;
            let snapshot = Stage6DurableCommandSnapshotV1::from_place(&identity, command)?;
            Ok((identity, snapshot))
        }
        BrokerCommand::CancelOrder(command) => {
            let identity = Stage6DurableRequestIdentityV1::from_cancel(
                command,
                context.instrument.clone(),
                context.attribution.clone(),
            )?;
            let snapshot = Stage6DurableCommandSnapshotV1::from_cancel(&identity, command)?;
            Ok((identity, snapshot))
        }
    }
}

fn stage7a_command_expired(command: &BrokerCommand, observed_at: DateTime<Utc>) -> bool {
    let (created_at, ttl_ms) = match command {
        BrokerCommand::PlaceOrder(command) => (command.created_ts, command.ttl_ms),
        BrokerCommand::CancelOrder(command) => (command.created_ts, command.ttl_ms),
    };
    ttl_ms.is_some_and(|ttl_ms| {
        let Ok(ttl_ms) = i64::try_from(ttl_ms) else {
            return false;
        };
        created_at
            .checked_add_signed(chrono::Duration::milliseconds(ttl_ms))
            .is_some_and(|deadline| observed_at > deadline)
    })
}

fn stage7a_accepted_record(
    recovered: &Stage6dDurableRuntimeRecovered,
    request_id: StrategyRequestId,
) -> Option<&Stage6JournalRecordV1> {
    recovered.journal.records().iter().find(|record| {
        record.event_kind() == Stage6JournalEventKind::RequestAccepted
            && record.durable_request_identity().strategy_request_id() == request_id
    })
}

fn stage7a_has_other_unresolved_lifecycle(
    recovered: &Stage6dDurableRuntimeRecovered,
    candidate: &Stage6DurableRequestIdentityV1,
) -> bool {
    recovered.replay().requests().iter().any(|request| {
        if request.strategy_request_id() == candidate.strategy_request_id()
            || request.final_disposition().is_some()
        {
            return false;
        }
        stage7a_accepted_record(recovered, request.strategy_request_id()).is_some_and(|record| {
            let existing = record.durable_request_identity();
            let same_scope = existing.account_id() == candidate.account_id()
                && existing.instrument() == candidate.instrument()
                && existing.attribution().strategy_id() == candidate.attribution().strategy_id();
            same_scope
        })
    })
}

/// Appends the explicit command-request terminal record required by the
/// frozen Stage 7A max-one lifecycle contract. Broker order lifecycle remains
/// independent: a finalized LIMIT PLACE request may still expose a working
/// broker order identity that a later, sequential CANCEL request targets.
pub fn finalize_stage7a_paper_request(
    recovered: &mut Stage6dDurableRuntimeRecovered,
    mut report: Stage6dPaperExecutionReport,
    observed_at: DateTime<Utc>,
) -> Result<Stage6dPaperExecutionReport, Stage6dLiveCoreError> {
    if report.final_disposition.is_some() {
        return Ok(report);
    }
    let accepted = stage7a_accepted_record(recovered, report.strategy_request_id)
        .ok_or(Stage6dLiveCoreError::AcceptedRecordRequired)?;
    let identity = accepted.durable_request_identity().clone();
    let request = recovered
        .replay()
        .request(report.strategy_request_id)
        .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?;
    if request.final_disposition().is_some()
        || request.last_unique_record_id().as_str() != report.final_record_id
        || request.last_unique_sequence() != report.final_sequence
    {
        return Err(Stage6dLiveCoreError::DurableOrderingViolation);
    }
    let disposition = if report.cancel_outcome == Some(Stage6CancelOutcomeV1::Rejected) {
        Stage6RequestFinalDispositionV1::Rejected
    } else {
        Stage6RequestFinalDispositionV1::Completed
    };
    #[derive(Serialize)]
    struct FinalizationEvidence<'a> {
        domain: &'static str,
        observed_at: DateTime<Utc>,
        strategy_request_id: StrategyRequestId,
        final_record_id: &'a str,
        disposition: Stage6RequestFinalDispositionV1,
    }
    let evidence = serde_json::to_vec(&FinalizationEvidence {
        domain: "moex.stage7a.paper-command-finalization.v1",
        observed_at,
        strategy_request_id: report.strategy_request_id,
        final_record_id: &report.final_record_id,
        disposition,
    })
    .map_err(|_| Stage6dLiveCoreError::IntegrationFingerprint)?;
    let finalized = Stage6JournalRecordV1::request_finalized(
        identity,
        disposition,
        Stage6LifecycleSequence::new(report.final_sequence.saturating_add(1))?,
        Some(request.last_unique_record_id().clone()),
        Stage6Sha256Digest::parse(sha256_hex(&evidence))?,
    )?;
    recovered.journal_mut().append(&finalized)?;
    recovered.refresh_after_append()?;
    let request = recovered
        .replay()
        .request(report.strategy_request_id)
        .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?;
    report.final_record_id = request.last_unique_record_id().as_str().to_string();
    report.final_sequence = request.last_unique_sequence();
    report.final_disposition = request.final_disposition();
    report.dispatch_safety_state = request.dispatch_safety_state();
    report.runtime_post_fingerprint_sha256 =
        stage5_runtime_authority_fingerprint(&recovered.stage5_runtime)?;
    report.journal_frontier_sha256 = frontier_fingerprint(recovered.journal_frontier())?;
    report.integration_fingerprint_sha256 = recovered
        .integration_fingerprint_sha256()
        .as_str()
        .to_string();
    Ok(report)
}

/// Completes the same Stage 7A request-finalization step after a same-process
/// redelivery that observes a normalized paper outcome but no final record.
/// It never re-invokes the paper outcome provider.
pub fn finalize_stage7a_replayed_paper_request(
    recovered: &mut Stage6dDurableRuntimeRecovered,
    request_id: StrategyRequestId,
    observed_at: DateTime<Utc>,
) -> Result<(), Stage6dLiveCoreError> {
    let request = recovered
        .replay()
        .request(request_id)
        .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?;
    if request.final_disposition().is_some() {
        return Ok(());
    }
    if request.dispatch_safety_state() != crate::Stage6DispatchSafetyStateV1::DispatchForbidden {
        return Err(Stage6dLiveCoreError::DurableOrderingViolation);
    }
    let previous = request.last_unique_record_id().clone();
    let sequence = request.last_unique_sequence();
    let disposition = if request.cancel_outcome() == Some(Stage6CancelOutcomeV1::Rejected) {
        Stage6RequestFinalDispositionV1::Rejected
    } else {
        Stage6RequestFinalDispositionV1::Completed
    };
    let identity = stage7a_accepted_record(recovered, request_id)
        .ok_or(Stage6dLiveCoreError::AcceptedRecordRequired)?
        .durable_request_identity()
        .clone();
    #[derive(Serialize)]
    struct ReplayFinalizationEvidence<'a> {
        domain: &'static str,
        observed_at: DateTime<Utc>,
        strategy_request_id: StrategyRequestId,
        previous_record_id: &'a str,
        disposition: Stage6RequestFinalDispositionV1,
    }
    let evidence = serde_json::to_vec(&ReplayFinalizationEvidence {
        domain: "moex.stage7a.paper-command-replay-finalization.v1",
        observed_at,
        strategy_request_id: request_id,
        previous_record_id: previous.as_str(),
        disposition,
    })
    .map_err(|_| Stage6dLiveCoreError::IntegrationFingerprint)?;
    let finalized = Stage6JournalRecordV1::request_finalized(
        identity,
        disposition,
        Stage6LifecycleSequence::new(sequence.saturating_add(1))?,
        Some(previous),
        Stage6Sha256Digest::parse(sha256_hex(&evidence))?,
    )?;
    recovered.journal_mut().append(&finalized)?;
    recovered.refresh_after_append()?;
    Ok(())
}

fn stage7a_source_evidence(
    command: &BrokerCommand,
    observed_at: DateTime<Utc>,
    phase: &str,
) -> Result<Stage6Sha256Digest, Stage6dLiveCoreError> {
    #[derive(Serialize)]
    struct Evidence<'a> {
        domain: &'static str,
        phase: &'a str,
        observed_at: DateTime<Utc>,
        command: &'a BrokerCommand,
    }
    let bytes = serde_json::to_vec(&Evidence {
        domain: "moex.stage7a.paper-command-admission.v1",
        phase,
        observed_at,
        command,
    })
    .map_err(|_| Stage6dLiveCoreError::IntegrationFingerprint)?;
    Stage6Sha256Digest::parse(sha256_hex(&bytes)).map_err(Into::into)
}

fn stage7a_dispatch_record(
    identity: &Stage6DurableRequestIdentityV1,
    accepted: &Stage6JournalRecordV1,
    observed_at: DateTime<Utc>,
) -> Result<Stage6JournalRecordV1, Stage6dLiveCoreError> {
    let command = match accepted.payload() {
        Stage6JournalPayloadV1::RequestAccepted { command } => command,
        _ => return Err(Stage6dLiveCoreError::AcceptedRecordRequired),
    };
    let bytes = serde_json::to_vec(command.as_ref())
        .map_err(|_| Stage6dLiveCoreError::IntegrationFingerprint)?;
    let evidence = Stage6Sha256Digest::parse(sha256_hex(
        [
            b"moex.stage7a.resume-dispatch.v1".as_slice(),
            observed_at.to_rfc3339().as_bytes(),
            bytes.as_slice(),
        ]
        .concat()
        .as_slice(),
    ))?;
    Ok(Stage6JournalRecordV1::dispatch_attempt_recorded(
        identity.clone(),
        1,
        accepted.canonical_payload_sha256().clone(),
        Stage6LifecycleSequence::new(accepted.lifecycle_sequence().get().saturating_add(1))?,
        Some(accepted.journal_record_id().clone()),
        evidence,
    )?)
}

fn prepare_stage6d_existing_accepted_paper_dispatch(
    recovered: &mut Stage6dDurableRuntimeRecovered,
    accepted: &Stage6JournalRecordV1,
    dispatch_attempt: Stage6JournalRecordV1,
) -> Result<Stage6dPaperDispatchReceipt, Stage6dLiveCoreError> {
    if accepted.event_kind() != Stage6JournalEventKind::RequestAccepted
        || dispatch_attempt.event_kind() != Stage6JournalEventKind::DispatchAttemptRecorded
        || accepted.durable_request_identity() != dispatch_attempt.durable_request_identity()
        || dispatch_attempt.previous_record_id() != Some(accepted.journal_record_id())
    {
        return Err(Stage6dLiveCoreError::DurableOrderingViolation);
    }
    recovered.journal_mut().append(&dispatch_attempt)?;
    recovered.refresh_after_append()?;
    let request = recovered
        .replay()
        .request(accepted.durable_request_identity().strategy_request_id())
        .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?;
    if request.dispatch_safety_state() != crate::Stage6DispatchSafetyStateV1::ReconciliationRequired
        || request.last_unique_record_id() != dispatch_attempt.journal_record_id()
    {
        return Err(Stage6dLiveCoreError::DurableOrderingViolation);
    }
    Ok(Stage6dPaperDispatchReceipt {
        identity: accepted.durable_request_identity().clone(),
        dispatch_record_id: dispatch_attempt.journal_record_id().clone(),
        dispatch_sequence: dispatch_attempt.lifecycle_sequence(),
        durable_frontier_sha256: frontier_fingerprint(recovered.journal_frontier())?,
    })
}

/// Appends and validates the exact durable pre-effect ordering.  If either
/// append fails no paper adapter capability is returned.
pub fn prepare_stage6d_paper_dispatch(
    recovered: &mut Stage6dDurableRuntimeRecovered,
    accepted: Stage6JournalRecordV1,
    dispatch_attempt: Stage6JournalRecordV1,
) -> Result<Stage6dPaperDispatchReceipt, Stage6dLiveCoreError> {
    if accepted.event_kind() != Stage6JournalEventKind::RequestAccepted {
        return Err(Stage6dLiveCoreError::AcceptedRecordRequired);
    }
    if dispatch_attempt.event_kind() != Stage6JournalEventKind::DispatchAttemptRecorded {
        return Err(Stage6dLiveCoreError::DispatchAttemptRecordRequired);
    }
    if accepted.durable_request_identity() != dispatch_attempt.durable_request_identity()
        || dispatch_attempt.previous_record_id() != Some(accepted.journal_record_id())
        || dispatch_attempt.lifecycle_sequence().get()
            != accepted.lifecycle_sequence().get().saturating_add(1)
    {
        return Err(Stage6dLiveCoreError::DurableOrderingViolation);
    }

    recovered.journal_mut().append(&accepted)?;
    recovered.refresh_after_append()?;
    recovered.journal_mut().append(&dispatch_attempt)?;
    recovered.refresh_after_append()?;

    let request_id = accepted.durable_request_identity().strategy_request_id();
    let request = recovered
        .replay()
        .request(request_id)
        .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?;
    if request.dispatch_safety_state() != crate::Stage6DispatchSafetyStateV1::ReconciliationRequired
        || request.last_unique_record_id() != dispatch_attempt.journal_record_id()
    {
        return Err(Stage6dLiveCoreError::DurableOrderingViolation);
    }

    Ok(Stage6dPaperDispatchReceipt {
        identity: accepted.durable_request_identity().clone(),
        dispatch_record_id: dispatch_attempt.journal_record_id().clone(),
        dispatch_sequence: dispatch_attempt.lifecycle_sequence(),
        durable_frontier_sha256: frontier_fingerprint(recovered.journal_frontier())?,
    })
}

/// Consumes the durable dispatch proof and emits only normalized Stage 6C
/// facts. This is the sole process-local paper effect boundary in Stage 6D.
pub fn execute_stage6d_paper_outcome(
    recovered: &mut Stage6dDurableRuntimeRecovered,
    receipt: Stage6dPaperDispatchReceipt,
    outcome: Stage6dPaperOutcome,
) -> Result<Stage6dPaperExecutionReport, Stage6dLiveCoreError> {
    let runtime_pre_fingerprint_sha256 =
        stage5_runtime_authority_fingerprint(&recovered.stage5_runtime)?;
    let current_frontier = frontier_fingerprint(recovered.journal_frontier())?;
    if current_frontier != receipt.durable_frontier_sha256
        || recovered.journal_frontier().last_record_id() != Some(&receipt.dispatch_record_id)
    {
        return Err(Stage6dLiveCoreError::DurableOrderingViolation);
    }
    validate_paper_outcome_action(receipt.identity.action(), &outcome)?;
    let evidence = accepted_paper_evidence(&receipt.identity, &outcome)?;
    let accepted_truth = Stage6dAcceptedBrokerTruth {
        receipt,
        outcome,
        evidence,
    };
    let Stage6dAcceptedBrokerTruth {
        receipt,
        outcome,
        evidence,
    } = accepted_truth;
    let mut sequence = receipt.dispatch_sequence.get() + 1;
    let mut previous = receipt.dispatch_record_id.clone();
    let mut records = Vec::new();

    match outcome {
        Stage6dPaperOutcome::MarketFilled {
            broker_order_id,
            broker_trade_id,
        }
        | Stage6dPaperOutcome::LimitFilled {
            broker_order_id,
            broker_trade_id,
        } => {
            let order = Stage6JournalRecordV1::broker_order_observed(
                receipt.identity.clone(),
                broker_order_id.clone(),
                Stage6LifecycleSequence::new(sequence)?,
                Some(previous.clone()),
                evidence.clone(),
            )?;
            sequence += 1;
            previous = order.journal_record_id().clone();
            records.push(order);
            records.push(Stage6JournalRecordV1::broker_trade_observed(
                receipt.identity.clone(),
                broker_trade_id,
                broker_order_id,
                Stage6LifecycleSequence::new(sequence)?,
                Some(previous),
                evidence,
            )?);
        }
        Stage6dPaperOutcome::LimitPending { broker_order_id } => {
            records.push(Stage6JournalRecordV1::broker_order_observed(
                receipt.identity.clone(),
                broker_order_id,
                Stage6LifecycleSequence::new(sequence)?,
                Some(previous),
                evidence,
            )?);
        }
        Stage6dPaperOutcome::PlaceBrokerOrderFound { broker_order_id } => {
            records.push(Stage6JournalRecordV1::reconciliation_observed(
                receipt.identity.clone(),
                Stage6ReconciliationDispositionV1::BrokerOrderFound { broker_order_id },
                Stage6LifecycleSequence::new(sequence)?,
                Some(previous),
                evidence,
            )?);
        }
        Stage6dPaperOutcome::PlaceNoBrokerOrderFound => {
            records.push(Stage6JournalRecordV1::reconciliation_observed(
                receipt.identity.clone(),
                Stage6ReconciliationDispositionV1::NoBrokerOrderFound,
                Stage6LifecycleSequence::new(sequence)?,
                Some(previous),
                evidence,
            )?);
        }
        Stage6dPaperOutcome::Inconclusive => {
            records.push(Stage6JournalRecordV1::reconciliation_observed(
                receipt.identity.clone(),
                Stage6ReconciliationDispositionV1::Inconclusive,
                Stage6LifecycleSequence::new(sequence)?,
                Some(previous),
                evidence,
            )?);
        }
        Stage6dPaperOutcome::CancelCanceled
        | Stage6dPaperOutcome::CancelExecutionObserved
        | Stage6dPaperOutcome::CancelRejected
        | Stage6dPaperOutcome::CancelAlreadyTerminalNonExecution => {
            let cancel_outcome = match outcome {
                Stage6dPaperOutcome::CancelCanceled => Stage6CancelOutcomeV1::Canceled,
                Stage6dPaperOutcome::CancelExecutionObserved => {
                    Stage6CancelOutcomeV1::ExecutionObserved
                }
                Stage6dPaperOutcome::CancelRejected => Stage6CancelOutcomeV1::Rejected,
                Stage6dPaperOutcome::CancelAlreadyTerminalNonExecution => {
                    Stage6CancelOutcomeV1::AlreadyTerminalNonExecution
                }
                _ => unreachable!("matched cancel outcomes only"),
            };
            let target = receipt
                .identity
                .target_broker_order_id()
                .cloned()
                .ok_or(Stage6dLiveCoreError::PaperOutcomeActionMismatch)?;
            records.push(Stage6JournalRecordV1::cancel_outcome_observed(
                receipt.identity.clone(),
                target,
                cancel_outcome,
                Stage6LifecycleSequence::new(sequence)?,
                Some(previous),
                evidence,
            )?);
        }
    }

    for record in records {
        recovered.journal_mut().append(&record)?;
        recovered.refresh_after_append()?;
    }
    let request = recovered
        .replay()
        .request(receipt.identity.strategy_request_id())
        .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?;
    let runtime_post_fingerprint_sha256 =
        stage5_runtime_authority_fingerprint(&recovered.stage5_runtime)?;
    Ok(Stage6dPaperExecutionReport {
        strategy_request_id: request.strategy_request_id(),
        durable_client_order_id: receipt.identity.durable_client_order_id().clone(),
        account_id: receipt.identity.account_id().clone(),
        instrument: receipt.identity.instrument().clone(),
        attribution: receipt.identity.attribution().clone(),
        action: request.action(),
        dispatch_record_id: receipt.dispatch_record_id.as_str().to_string(),
        durable_sequence: receipt.dispatch_sequence.get(),
        final_record_id: request.last_unique_record_id().as_str().to_string(),
        final_sequence: request.last_unique_sequence(),
        dispatch_safety_state: request.dispatch_safety_state(),
        broker_order_id: request.known_broker_order_id().cloned(),
        broker_trade_ids: request.observed_broker_trade_ids().to_vec(),
        cancel_outcome: request.cancel_outcome(),
        final_disposition: request.final_disposition(),
        runtime_pre_fingerprint_sha256,
        runtime_post_fingerprint_sha256,
        journal_frontier_sha256: frontier_fingerprint(recovered.journal_frontier())?,
        integration_fingerprint_sha256: recovered
            .integration_fingerprint_sha256()
            .as_str()
            .to_string(),
        restart_recovery_marker: recovered.boot_mode == Stage6dBootMode::Restart,
    })
}

fn stage6e_semantic_cross_bind_restart(
    restart: &Stage5gCleanRestartedCapability,
    journal: &impl Stage6JournalBackend,
    replay: &Stage6ReplaySnapshotV1,
) -> Result<Stage6eSemanticCrossBinding, Stage6dLiveCoreError> {
    #[derive(Serialize)]
    struct CrossBindingV1<'a> {
        schema_version: u16,
        domain: &'static str,
        stage5_source_lifecycle_commit_sha256: &'a str,
        stage5_lifecycle_source_authority_sha256: &'a str,
        current_requests: &'a [Stage6eCrossBoundRequestWitness],
    }

    let projection = restart.fresh_truth_reducer_projection();
    let mut witnesses = Vec::with_capacity(projection.slots.len());
    for slot in &projection.slots {
        if witnesses
            .iter()
            .any(|witness: &Stage6eCrossBoundRequestWitness| {
                witness.strategy_request_id.to_string() == slot.command_request_id
            })
        {
            return Err(Stage6dLiveCoreError::RestartSemanticCrossBindingMismatch);
        }
        let identity = journal
            .records()
            .iter()
            .find(|record| {
                record.event_kind() == Stage6JournalEventKind::RequestAccepted
                    && record
                        .durable_request_identity()
                        .strategy_request_id()
                        .to_string()
                        == slot.command_request_id
            })
            .map(Stage6JournalRecordV1::durable_request_identity)
            .ok_or(Stage6dLiveCoreError::RestartSemanticCrossBindingMismatch)?;
        let replay_request = replay
            .request(identity.strategy_request_id())
            .ok_or(Stage6dLiveCoreError::RestartSemanticCrossBindingMismatch)?;
        let expected_action = match &slot.source_action {
            crate::Stage5gMockIntentAction::Place { .. } => Stage6DurableActionKind::Place,
            crate::Stage5gMockIntentAction::Cancel { .. } => Stage6DurableActionKind::Cancel,
        };
        let expected_cancel_target = match &slot.source_action {
            crate::Stage5gMockIntentAction::Cancel { target_order_id } => Some(target_order_id),
            crate::Stage5gMockIntentAction::Place { .. } => None,
        };
        let attribution_fingerprint_sha256 =
            stage5g_attribution_fingerprint_sha256(identity.attribution());
        if identity.durable_client_order_id() != &slot.command_client_order_id
            || replay_request.durable_client_order_id() != &slot.command_client_order_id
            || identity.account_id() != &projection.account_id
            || identity.instrument() != &projection.instrument_id
            || !identity.attribution().belongs_to(&projection.strategy_id)
            || slot
                .expected_attribution_fingerprint_sha256
                .as_ref()
                .is_some_and(|expected| expected != &attribution_fingerprint_sha256)
            || identity.action() != expected_action
            || replay_request.action() != expected_action
            || identity.target_broker_order_id() != expected_cancel_target
            || (expected_action == Stage6DurableActionKind::Cancel
                && slot
                    .target_order_client_order_id
                    .as_ref()
                    .is_some_and(|expected| {
                        identity.target_order_client_order_id() != Some(expected)
                    }))
        {
            return Err(Stage6dLiveCoreError::RestartSemanticCrossBindingMismatch);
        }
        witnesses.push(Stage6eCrossBoundRequestWitness {
            strategy_request_id: identity.strategy_request_id(),
            durable_client_order_id: identity.durable_client_order_id().clone(),
            account_id: identity.account_id().clone(),
            instrument: identity.instrument().clone(),
            strategy_definition_id: projection.strategy_id.clone(),
            attribution_fingerprint_sha256,
            action: identity.action(),
            target_broker_order_id: identity.target_broker_order_id().cloned(),
            target_order_client_order_id: identity.target_order_client_order_id().cloned(),
        });
    }
    if replay.requests().iter().any(|request| {
        !witnesses
            .iter()
            .any(|witness| witness.strategy_request_id == request.strategy_request_id())
            && request.final_disposition().is_none()
    }) {
        return Err(Stage6dLiveCoreError::RestartSemanticCrossBindingMismatch);
    }
    witnesses.sort_by_key(|witness| witness.strategy_request_id.to_string());
    let bytes = serde_json::to_vec(&CrossBindingV1 {
        schema_version: 1,
        domain: STAGE6E_SEMANTIC_CROSS_BINDING_DOMAIN,
        stage5_source_lifecycle_commit_sha256: &projection.source_lifecycle_commit_sha256,
        stage5_lifecycle_source_authority_sha256: &projection.lifecycle_source_authority_sha256,
        current_requests: &witnesses,
    })
    .map_err(|_| Stage6dLiveCoreError::IntegrationFingerprint)?;
    let fingerprint_sha256 = Stage6Sha256Digest::parse(sha256_hex(&bytes))
        .map_err(|_| Stage6dLiveCoreError::IntegrationFingerprint)?;
    Ok(Stage6eSemanticCrossBinding {
        request_ids: witnesses
            .iter()
            .map(|witness| witness.strategy_request_id)
            .collect(),
        fingerprint_sha256,
    })
}

fn stage6d_validate_selected_restart_request(
    replay: &Stage6ReplaySnapshotV1,
    projection: &crate::stage5g_clean_restart::Stage5gFreshTruthRestartProjection,
    request_id: StrategyRequestId,
) -> Result<(), Stage6dLiveCoreError> {
    let replay_request = replay
        .request(request_id)
        .ok_or(Stage6dLiveCoreError::FreshTruthRequestNotCrossBound)?;
    let slot = projection
        .slots
        .iter()
        .find(|slot| slot.command_request_id == request_id.to_string())
        .ok_or(Stage6dLiveCoreError::FreshTruthRequestNotCrossBound)?;
    if slot.command_client_order_id != *replay_request.durable_client_order_id() {
        return Err(Stage6dLiveCoreError::RestartRequestIdentityMismatch);
    }
    Ok(())
}

fn validate_stage6e_temporal_authority(
    input: &Stage6ePaperFreshBrokerTruthInput,
    restore_epoch: &Stage6RestoreEpoch,
    validation_observed_at: DateTime<Utc>,
) -> Result<(), Stage6dLiveCoreError> {
    let restore_completed_at = restore_epoch.restore_completed_at;
    let sections = [
        input.orders_observed_at,
        input.trades_observed_at,
        input.positions_observed_at,
    ];
    if input.collection_started_at <= restore_completed_at
        || input.captured_at < input.collection_started_at
        || input.captured_at > validation_observed_at
        || sections.iter().any(|observed_at| {
            *observed_at <= restore_completed_at
                || *observed_at < input.collection_started_at
                || *observed_at > input.captured_at
                || *observed_at > validation_observed_at
        })
        || input.orders.iter().any(|row| {
            row.received_ts <= restore_completed_at || row.received_ts > validation_observed_at
        })
        || input.trades.iter().any(|row| {
            row.received_ts <= restore_completed_at || row.received_ts > validation_observed_at
        })
        || input.positions.iter().any(|row| {
            row.received_ts <= restore_completed_at || row.received_ts > validation_observed_at
        })
    {
        return Err(Stage6dLiveCoreError::FreshTruthTemporalAuthorityMismatch);
    }
    Ok(())
}

fn stage6d_validate_replayed_facts_against_truth(
    replay: &Stage6ReplaySnapshotV1,
    request_id: StrategyRequestId,
    input: &Stage6ePaperFreshBrokerTruthInput,
) -> Result<(), Stage6dLiveCoreError> {
    let request = replay
        .request(request_id)
        .ok_or(Stage6dLiveCoreError::RestartRequestIdentityMismatch)?;
    if let Some(expected) = request.known_broker_order_id() {
        let order_present = input
            .orders
            .iter()
            .any(|order| order.broker_order_id.as_ref() == Some(expected));
        let trade_present = input
            .trades
            .iter()
            .any(|trade| trade.broker_order_id.as_ref() == Some(expected));
        if !order_present && !trade_present {
            return Err(Stage6dLiveCoreError::RestartBrokerTruthMismatch);
        }
    }
    if request.observed_broker_trade_ids().iter().any(|expected| {
        !input
            .trades
            .iter()
            .any(|trade| &trade.broker_trade_id == expected)
    }) {
        return Err(Stage6dLiveCoreError::RestartBrokerTruthMismatch);
    }
    Ok(())
}

fn stage6d_stage5g_accepted_history(
    projection: &crate::stage5g_clean_restart::Stage5gFreshTruthRestartProjection,
    current_id: &str,
) -> Result<Vec<Stage5gReconciledFreshPackageIdentity>, Stage6dLiveCoreError> {
    projection
        .checkpoint
        .payload
        .evidence_replay_ledger
        .iter()
        .filter(|entry| entry.identity != current_id)
        .map(|entry| {
            let epoch = entry
                .identity
                .splitn(4, ':')
                .nth(3)
                .ok_or(Stage6dLiveCoreError::RestartBrokerTruthMismatch)?;
            Stage5gReconciledFreshPackageIdentity::validate(
                entry.identity.clone(),
                epoch,
                entry.fingerprint_sha256.clone(),
            )
            .map_err(Stage6dLiveCoreError::from)
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn stage6d_fresh_truth_report(
    recovered: &Stage6dDurableRuntimeRecovered,
    strategy_request_id: StrategyRequestId,
    package_id: String,
    scenario_id: &str,
    disposition: &str,
    reason: &str,
    runtime_transition_applied: bool,
    already_represented_noop: bool,
    stage5_pre_fingerprint_sha256: String,
    stage5_post_fingerprint_sha256: String,
) -> Stage6dFreshTruthApplicationReport {
    Stage6dFreshTruthApplicationReport {
        strategy_request_id,
        package_id,
        scenario_id: scenario_id.to_string(),
        disposition: disposition.to_string(),
        reason: reason.to_string(),
        runtime_transition_applied,
        already_represented_noop,
        stage5_pre_fingerprint_sha256,
        stage5_post_fingerprint_sha256,
        stage6_replay_fingerprint_sha256: recovered
            .replay()
            .semantic_fingerprint_sha256()
            .as_str()
            .to_string(),
        integration_fingerprint_sha256: recovered
            .integration_fingerprint_sha256()
            .as_str()
            .to_string(),
    }
}

fn validate_paper_outcome_action(
    action: Stage6DurableActionKind,
    outcome: &Stage6dPaperOutcome,
) -> Result<(), Stage6dLiveCoreError> {
    let valid = match action {
        Stage6DurableActionKind::Place => matches!(
            outcome,
            Stage6dPaperOutcome::MarketFilled { .. }
                | Stage6dPaperOutcome::LimitPending { .. }
                | Stage6dPaperOutcome::LimitFilled { .. }
                | Stage6dPaperOutcome::PlaceBrokerOrderFound { .. }
                | Stage6dPaperOutcome::PlaceNoBrokerOrderFound
                | Stage6dPaperOutcome::Inconclusive
        ),
        Stage6DurableActionKind::Cancel => matches!(
            outcome,
            Stage6dPaperOutcome::CancelCanceled
                | Stage6dPaperOutcome::CancelExecutionObserved
                | Stage6dPaperOutcome::CancelRejected
                | Stage6dPaperOutcome::CancelAlreadyTerminalNonExecution
                | Stage6dPaperOutcome::Inconclusive
        ),
    };
    if valid {
        Ok(())
    } else {
        Err(Stage6dLiveCoreError::PaperOutcomeActionMismatch)
    }
}

fn accepted_paper_evidence(
    identity: &Stage6DurableRequestIdentityV1,
    outcome: &Stage6dPaperOutcome,
) -> Result<Stage6Sha256Digest, Stage6dLiveCoreError> {
    #[derive(Serialize)]
    struct AcceptedPaperEvidenceV1<'a> {
        schema_version: u16,
        domain: &'static str,
        strategy_request_id: StrategyRequestId,
        durable_client_order_id: &'a broker_core::ClientOrderId,
        account_id: &'a broker_core::BrokerAccountId,
        instrument: &'a broker_core::InstrumentId,
        attribution: &'a broker_core::HybridRuntimeAttribution,
        action: Stage6DurableActionKind,
        target_broker_order_id: Option<&'a BrokerOrderId>,
        outcome: &'a Stage6dPaperOutcome,
    }
    let authority = AcceptedPaperEvidenceV1 {
        schema_version: 1,
        domain: "moex.stage6d.accepted-paper-broker-truth.v1",
        strategy_request_id: identity.strategy_request_id(),
        durable_client_order_id: identity.durable_client_order_id(),
        account_id: identity.account_id(),
        instrument: identity.instrument(),
        attribution: identity.attribution(),
        action: identity.action(),
        target_broker_order_id: identity.target_broker_order_id(),
        outcome,
    };
    let bytes =
        serde_json::to_vec(&authority).map_err(|_| Stage6dLiveCoreError::IntegrationFingerprint)?;
    Stage6Sha256Digest::parse(sha256_hex(&bytes))
        .map_err(|_| Stage6dLiveCoreError::IntegrationFingerprint)
}

fn frontier_fingerprint(
    frontier: &Stage6JournalFrontierV1,
) -> Result<String, Stage6dLiveCoreError> {
    let bytes =
        serde_json::to_vec(frontier).map_err(|_| Stage6dLiveCoreError::IntegrationFingerprint)?;
    Ok(sha256_hex(&bytes))
}

pub fn stage6_frontier_fingerprint_sha256(
    frontier: &Stage6JournalFrontierV1,
) -> Result<Stage6Sha256Digest, Stage6dLiveCoreError> {
    Stage6Sha256Digest::parse(frontier_fingerprint(frontier)?)
        .map_err(|_| Stage6dLiveCoreError::IntegrationFingerprint)
}

fn decode_and_authenticate_restart_package(
    bytes: &[u8],
    commitment_key: &Stage5gLifecycleCommitmentKey,
) -> Result<Stage6dAuthenticatedRestartPackageV1, Stage6dLiveCoreError> {
    let package: Stage6dAuthenticatedRestartPackageV1 =
        serde_json::from_slice(bytes).map_err(|_| Stage6dLiveCoreError::RestartPackageDecode)?;
    if package.schema_version != STAGE6D_AUTHENTICATED_RESTART_SCHEMA_VERSION {
        return Err(Stage6dLiveCoreError::UnsupportedRestartPackageSchema);
    }
    let canonical =
        serde_json::to_vec(&package).map_err(|_| Stage6dLiveCoreError::RestartPackageDecode)?;
    if canonical != bytes {
        return Err(Stage6dLiveCoreError::RestartPackageNonCanonical);
    }
    if sha256_hex(&package.stage5g_restart_package) != package.stage5g_restart_package_sha256 {
        return Err(Stage6dLiveCoreError::Stage5gPackageDigestMismatch);
    }
    let checkpoint_bytes = package.stage6_checkpoint.encode_canonical();
    if sha256_hex(&checkpoint_bytes) != package.stage6_checkpoint_bytes_sha256 {
        return Err(Stage6dLiveCoreError::CheckpointDigestMismatch);
    }
    validate_operational_identity_config(&package.operational_identity)?;
    let operational_identity_bytes = serde_json::to_vec(&package.operational_identity)
        .map_err(|_| Stage6dLiveCoreError::OperationalIdentityInvalid)?;
    if sha256_hex(&operational_identity_bytes) != package.operational_identity_sha256 {
        return Err(Stage6dLiveCoreError::OperationalIdentityInvalid);
    }
    let commitment = restart_commitment_sha256(
        &package.stage5g_restart_package_sha256,
        &package.stage6_checkpoint_bytes_sha256,
        &package.operational_identity_sha256,
    )?;
    if commitment != package.restart_commitment_sha256 {
        return Err(Stage6dLiveCoreError::RestartCommitmentMismatch);
    }
    if !commitment_key
        .stage6d_verify_hmac_sha256(&commitment, &package.restart_commitment_hmac_sha256)
    {
        return Err(Stage6dLiveCoreError::RestartAuthenticationFailed);
    }
    Ok(package)
}

fn restart_commitment_sha256(
    stage5g_restart_package_sha256: &str,
    stage6_checkpoint_bytes_sha256: &str,
    operational_identity_sha256: &str,
) -> Result<String, Stage6dLiveCoreError> {
    Stage6Sha256Digest::parse(stage5g_restart_package_sha256.to_string())
        .map_err(|_| Stage6dLiveCoreError::RestartCommitmentMismatch)?;
    Stage6Sha256Digest::parse(stage6_checkpoint_bytes_sha256.to_string())
        .map_err(|_| Stage6dLiveCoreError::RestartCommitmentMismatch)?;
    Stage6Sha256Digest::parse(operational_identity_sha256.to_string())
        .map_err(|_| Stage6dLiveCoreError::RestartCommitmentMismatch)?;
    let input = Stage6dRestartCommitmentV1 {
        schema_version: STAGE6D_AUTHENTICATED_RESTART_SCHEMA_VERSION,
        domain: STAGE6D_RESTART_COMMITMENT_DOMAIN,
        stage5g_restart_package_sha256,
        stage6_checkpoint_bytes_sha256,
        operational_identity_sha256,
    };
    let bytes =
        serde_json::to_vec(&input).map_err(|_| Stage6dLiveCoreError::RestartCommitmentMismatch)?;
    Ok(sha256_hex(&bytes))
}

fn validate_operational_identity_config(
    config: &Stage6dOperationalIdentityConfig,
) -> Result<(), Stage6dLiveCoreError> {
    let canonical_token = |value: &str| {
        !value.is_empty()
            && value.len() <= 128
            && value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    };
    if !canonical_token(&config.broker_id)
        || !canonical_token(&config.strategy_instance_id)
        || !canonical_token(&config.deployment_id)
        || config.deployment_generation == 0
        || !canonical_token(&config.gateway_instance_id)
        || config.market_data_generation == 0
        || config.command_consumer_generation == 0
        || Stage6Sha256Digest::parse(config.instrument_map_fingerprint_sha256.clone()).is_err()
    {
        return Err(Stage6dLiveCoreError::OperationalIdentityInvalid);
    }
    Ok(())
}

fn integration_fingerprint(
    boot_mode: Stage6dBootMode,
    stage5_runtime: &Stage6dStage5RuntimeAuthority,
    replay: &Stage6ReplaySnapshotV1,
    checkpoint: &Stage6JournalCheckpointV1,
    semantic_cross_binding: Option<&Stage6eSemanticCrossBinding>,
    restore_epoch: Option<&Stage6RestoreEpoch>,
) -> Result<Stage6Sha256Digest, Stage6dLiveCoreError> {
    #[derive(Serialize)]
    struct IntegrationFingerprintV1<'a> {
        schema_version: u16,
        domain: &'static str,
        boot_mode: Stage6dBootMode,
        stage5_runtime_semantic_fingerprint_sha256: &'a str,
        stage6_replay_semantic_fingerprint_sha256: &'a Stage6Sha256Digest,
        stage6_checkpoint: &'a Stage6JournalCheckpointV1,
        recovered_requests: &'a [crate::Stage6RecoveredRequestV1],
        active_cross_bound_request_identity_sha256: Option<&'a Stage6Sha256Digest>,
        current_process_restore_epoch_sha256: Option<&'a Stage6Sha256Digest>,
    }

    let stage5_semantic_authority = stage5_runtime_authority_fingerprint(stage5_runtime)?;
    let stage5_fingerprint = sha256_hex(stage5_semantic_authority.as_bytes());
    let input = IntegrationFingerprintV1 {
        schema_version: STAGE6D_INTEGRATION_FINGERPRINT_SCHEMA_VERSION,
        domain: STAGE6D_INTEGRATION_FINGERPRINT_DOMAIN,
        boot_mode,
        stage5_runtime_semantic_fingerprint_sha256: &stage5_fingerprint,
        stage6_replay_semantic_fingerprint_sha256: replay.semantic_fingerprint_sha256(),
        stage6_checkpoint: checkpoint,
        recovered_requests: replay.requests(),
        active_cross_bound_request_identity_sha256: semantic_cross_binding
            .map(|binding| &binding.fingerprint_sha256),
        current_process_restore_epoch_sha256: restore_epoch.map(|epoch| &epoch.fingerprint_sha256),
    };
    let bytes =
        serde_json::to_vec(&input).map_err(|_| Stage6dLiveCoreError::IntegrationFingerprint)?;
    Stage6Sha256Digest::parse(sha256_hex(&bytes))
        .map_err(|_| Stage6dLiveCoreError::IntegrationFingerprint)
}

fn stage5_runtime_authority_fingerprint(
    stage5_runtime: &Stage6dStage5RuntimeAuthority,
) -> Result<String, Stage6dLiveCoreError> {
    match stage5_runtime {
        Stage6dStage5RuntimeAuthority::FirstBoot(runtime) => runtime_semantic_fingerprint(runtime),
        Stage6dStage5RuntimeAuthority::Restart(restored) => {
            Ok(restored.stage5g_pre_restart_package_fingerprint_sha256())
        }
    }
}

fn runtime_semantic_fingerprint(
    runtime: &HybridIntradayRuntimeStrategy,
) -> Result<String, Stage6dLiveCoreError> {
    let state = serde_json::to_value(Strategy::state(runtime))
        .map_err(|_| Stage6dLiveCoreError::IntegrationFingerprint)?;
    crate::stage5c_paper_host::stage5c_semantic_value_fingerprint(&state)
        .map_err(|_| Stage6dLiveCoreError::IntegrationFingerprint)
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hybrid_intraday::{
        HybridOrchestratorConfig, IntradayBreakoutConfig, MeanReversionConfig,
    };
    use crate::{
        BrokerNeutralMarketOrderStyle, HybridIntradayProfile, HybridIntradayRuntimeConfig,
        MeanReversionVariant, MrGatePolicy, RiskGateMode, Stage6DurableCommandSnapshotV1,
        Stage6RequestFinalDispositionV1,
    };
    use broker_core::{
        BrokerAccountId, CancelOrder, ClientOrderId, Exchange, HybridRuntimeAttribution,
        InstrumentId, Market, OrderSide, OrderStatus, OrderType, PlaceOrder, TimeInForce,
    };
    use chrono::{TimeZone, Utc};
    use rust_decimal::Decimal;
    use uuid::Uuid;

    static STAGE7B_TEST_FILE_COUNTER: AtomicU64 = AtomicU64::new(1);

    #[derive(Clone)]
    struct PlaceFixture {
        command: PlaceOrder,
        identity: Stage6DurableRequestIdentityV1,
    }

    #[derive(Clone)]
    struct CancelFixture {
        command: CancelOrder,
        identity: Stage6DurableRequestIdentityV1,
    }

    fn runtime() -> HybridIntradayRuntimeStrategy {
        HybridIntradayRuntimeStrategy::new(HybridIntradayRuntimeConfig {
            symbol: "IMOEXF".to_string(),
            profile: HybridIntradayProfile::BaselineRuntimeHybrid,
            mr_variant: MeanReversionVariant::ClassicPrevDayRange,
            mr_gate_policy: MrGatePolicy::Disabled,
            risk_gate_mode: RiskGateMode::Disabled,
            risk_gate_seed_file: None,
            risk_gate_ledger_key: None,
            model_session_start_time: None,
            model_session_end_time: None,
            qty: 1.0,
            live_order_style: BrokerNeutralMarketOrderStyle::Market,
            tick_size: 0.5,
            marketable_limit_offset_ticks: 0,
            timezone_offset_hours: 3,
            session_close_hour: 23,
            session_close_minute: 49,
            weekends_off: true,
            stop_end_buffer_sec: 60,
            repair_deadline_sec: 180,
            sl_escalate_timeout_sec: 30,
            max_repair_retries: 3,
            repair_backoff_base_sec: 5,
            repair_backoff_max_sec: 60,
            pending_timeout_sec: 30,
            partial_entry_fill_timeout_ms: 3_000,
            mr_config: MeanReversionConfig::default(),
            breakout_config: IntradayBreakoutConfig::default(),
            orchestrator_config: HybridOrchestratorConfig::default(),
        })
    }

    fn first_boot_config(runtime: &HybridIntradayRuntimeStrategy) -> Stage6dFirstBootConfig {
        Stage6dFirstBootConfig {
            deployment_id: "paper-imoexf-stage6d".to_string(),
            expected_runtime_config_fingerprint_sha256: runtime.stage5c_config_fingerprint(),
            allow_create_missing_journal: true,
        }
    }

    fn operational_config() -> Stage6dOperationalIdentityConfig {
        Stage6dOperationalIdentityConfig {
            broker_id: "finam-paper".to_string(),
            strategy_instance_id: "hybrid-imoexf-stage6d".to_string(),
            deployment_id: "stage6d-paper".to_string(),
            deployment_generation: 1,
            gateway_instance_id: "paper-gateway-stage6d".to_string(),
            instrument_map_fingerprint_sha256: "b".repeat(64),
            market_data_generation: 1,
            command_consumer_generation: 1,
        }
    }

    fn recovered() -> Stage6dDurableRuntimeRecovered {
        let runtime = runtime();
        let authority = authorize_stage6d_first_boot(first_boot_config(&runtime)).unwrap();
        first_boot_stage6d_paper(authority, runtime).unwrap()
    }

    fn request(number: u128) -> StrategyRequestId {
        StrategyRequestId::from(Uuid::from_u128((number << 96) | number))
    }

    fn instrument() -> InstrumentId {
        InstrumentId {
            symbol: "IMOEXF".to_string(),
            venue_symbol: Some("IMOEXF@RTSX".to_string()),
            exchange: Exchange::Moex,
            market: Market::Futures,
        }
    }

    fn attribution(role: &str) -> HybridRuntimeAttribution {
        HybridRuntimeAttribution::parse_source_comment(format!(
            "HYB|sid=hybrid_imoexf|c=cycle0001|o=BO|r={role}"
        ))
        .unwrap()
    }

    fn digest(byte: char) -> Stage6Sha256Digest {
        Stage6Sha256Digest::parse(byte.to_string().repeat(64)).unwrap()
    }

    fn place_fixture(number: u128, order_type: OrderType) -> PlaceFixture {
        let request_id = request(number);
        let attribution = attribution("ENTRY");
        let command = PlaceOrder {
            request_id,
            created_ts: Utc.with_ymd_and_hms(2026, 8, 11, 9, 0, 0).unwrap(),
            ttl_ms: Some(5_000),
            account_id: BrokerAccountId::new("ACC_TEST_0001"),
            client_order_id: ClientOrderId::from_strategy_request(request_id),
            instrument: instrument(),
            side: OrderSide::Buy,
            order_type,
            qty: Decimal::ONE,
            limit_price: (order_type == OrderType::Limit).then_some(Decimal::new(2210, 1)),
            time_in_force: TimeInForce::Day,
            comment: Some(attribution.internal_comment().to_string()),
        };
        let identity = Stage6DurableRequestIdentityV1::from_place(&command, attribution).unwrap();
        PlaceFixture { command, identity }
    }

    fn cancel_fixture(number: u128, target: &str) -> CancelFixture {
        let command = CancelOrder {
            request_id: request(number),
            created_ts: Utc.with_ymd_and_hms(2026, 8, 11, 9, 1, 0).unwrap(),
            ttl_ms: Some(5_000),
            account_id: BrokerAccountId::new("ACC_TEST_0001"),
            order_id: BrokerOrderId::new(target),
            client_order_id: Some(ClientOrderId::from_strategy_request(request(1))),
        };
        let identity = Stage6DurableRequestIdentityV1::from_cancel(
            &command,
            instrument(),
            attribution("CANCEL"),
        )
        .unwrap();
        CancelFixture { command, identity }
    }

    fn accepted_and_dispatch_place(
        fixture: &PlaceFixture,
    ) -> (Stage6JournalRecordV1, Stage6JournalRecordV1) {
        let snapshot =
            Stage6DurableCommandSnapshotV1::from_place(&fixture.identity, &fixture.command)
                .unwrap();
        let accepted = Stage6JournalRecordV1::request_accepted(
            fixture.identity.clone(),
            snapshot,
            Stage6LifecycleSequence::new(1).unwrap(),
            None,
            None,
            digest('1'),
        )
        .unwrap();
        let dispatch = Stage6JournalRecordV1::dispatch_attempt_recorded(
            fixture.identity.clone(),
            1,
            accepted.canonical_payload_sha256().clone(),
            Stage6LifecycleSequence::new(2).unwrap(),
            Some(accepted.journal_record_id().clone()),
            digest('2'),
        )
        .unwrap();
        (accepted, dispatch)
    }

    fn accepted_and_dispatch_cancel(
        fixture: &CancelFixture,
    ) -> (Stage6JournalRecordV1, Stage6JournalRecordV1) {
        let snapshot =
            Stage6DurableCommandSnapshotV1::from_cancel(&fixture.identity, &fixture.command)
                .unwrap();
        let accepted = Stage6JournalRecordV1::request_accepted(
            fixture.identity.clone(),
            snapshot,
            Stage6LifecycleSequence::new(1).unwrap(),
            None,
            None,
            digest('3'),
        )
        .unwrap();
        let dispatch = Stage6JournalRecordV1::dispatch_attempt_recorded(
            fixture.identity.clone(),
            1,
            accepted.canonical_payload_sha256().clone(),
            Stage6LifecycleSequence::new(2).unwrap(),
            Some(accepted.journal_record_id().clone()),
            digest('4'),
        )
        .unwrap();
        (accepted, dispatch)
    }

    #[test]
    fn stage8a1_exact_durable_authority_is_journal_backed() {
        let fixture = place_fixture(91, OrderType::Limit);
        let snapshot =
            Stage6DurableCommandSnapshotV1::from_place(&fixture.identity, &fixture.command)
                .unwrap();
        let mut owner = recovered();
        let (accepted, dispatch) = accepted_and_dispatch_place(&fixture);
        prepare_stage6d_paper_dispatch(&mut owner, accepted, dispatch).unwrap();

        let authority = owner
            .authorize_exact_durable_request(&fixture.identity, &snapshot)
            .unwrap();
        assert_eq!(authority.identity(), &fixture.identity);
        assert_eq!(
            authority.authenticated_checkpoint_sha256(),
            owner.authenticated_checkpoint().checkpoint_sha256()
        );
    }

    #[test]
    fn stage8a1_exact_durable_authority_rejects_absent_or_changed_command() {
        let fixture = place_fixture(92, OrderType::Limit);
        let snapshot =
            Stage6DurableCommandSnapshotV1::from_place(&fixture.identity, &fixture.command)
                .unwrap();
        let mut owner = recovered();
        assert!(matches!(
            owner.authorize_exact_durable_request(&fixture.identity, &snapshot),
            Err(Stage6dLiveCoreError::AcceptedRecordRequired)
        ));

        let (accepted, dispatch) = accepted_and_dispatch_place(&fixture);
        prepare_stage6d_paper_dispatch(&mut owner, accepted, dispatch).unwrap();
        let mut changed = fixture.command.clone();
        changed.qty = Decimal::new(2, 0);
        let changed_snapshot =
            Stage6DurableCommandSnapshotV1::from_place(&fixture.identity, &changed).unwrap();
        assert!(matches!(
            owner.authorize_exact_durable_request(&fixture.identity, &changed_snapshot),
            Err(Stage6dLiveCoreError::DurableOrderingViolation)
        ));
    }

    #[test]
    fn stage8a1_exact_durable_authority_rejects_accepted_only_request() {
        let fixture = place_fixture(93, OrderType::Limit);
        let snapshot =
            Stage6DurableCommandSnapshotV1::from_place(&fixture.identity, &fixture.command)
                .unwrap();
        let mut owner = recovered();
        let (accepted, _) = accepted_and_dispatch_place(&fixture);
        owner.journal_mut().append(&accepted).unwrap();
        owner.refresh_after_append().unwrap();

        assert!(matches!(
            owner.authorize_exact_durable_request(&fixture.identity, &snapshot),
            Err(Stage6dLiveCoreError::DurableOrderingViolation)
                | Err(Stage6dLiveCoreError::DispatchAttemptRecordRequired)
        ));
    }

    #[test]
    fn stage8a4_i3_appends_v2_then_exact_suffix_and_is_idempotent() {
        let fixture = place_fixture(94, OrderType::Limit);
        let snapshot =
            Stage6DurableCommandSnapshotV1::from_place(&fixture.identity, &fixture.command)
                .unwrap();
        let mut owner = recovered();
        let (accepted, dispatch) = accepted_and_dispatch_place(&fixture);
        prepare_stage6d_paper_dispatch(&mut owner, accepted, dispatch.clone()).unwrap();
        let authority = owner
            .authorize_stage8a4_durable_batch_source(&fixture.identity, &snapshot)
            .unwrap();
        let request_fingerprint = initial_request_state_fingerprint(&owner, &authority).unwrap();
        let (transition, suffix) = crate::stage6_reconciliation_v2::tests::i3_batch_fixture(
            &fixture.identity,
            &dispatch,
            authority.durable_request_binding_sha256().unwrap(),
            Stage6Sha256Digest::parse(authority.durable_frontier_sha256().to_string()).unwrap(),
            1,
            digest('f'),
            request_fingerprint,
            2,
        );

        let receipt = append_stage8a4_durable_batch(
            &mut owner,
            authority,
            Stage6Stage8a4DurableBatch::new(transition.clone(), suffix.clone(), None).unwrap(),
        )
        .unwrap();
        assert!(!receipt.transition_was_existing());
        assert_eq!(receipt.appended_suffix_records(), 2);
        assert_eq!(owner.journal.versioned_records().len(), 5);

        let authority = owner
            .authorize_stage8a4_durable_batch_source(&fixture.identity, &snapshot)
            .unwrap();
        let replay_receipt = append_stage8a4_durable_batch(
            &mut owner,
            authority,
            Stage6Stage8a4DurableBatch::new(transition, suffix, None).unwrap(),
        )
        .unwrap();
        assert!(replay_receipt.transition_was_existing());
        assert_eq!(replay_receipt.appended_suffix_records(), 0);
        assert_eq!(owner.journal.versioned_records().len(), 5);
    }

    #[test]
    fn stage8a4_i3_repairs_only_missing_suffix_after_v2_crash_boundary() {
        let fixture = place_fixture(95, OrderType::Limit);
        let snapshot =
            Stage6DurableCommandSnapshotV1::from_place(&fixture.identity, &fixture.command)
                .unwrap();
        let mut owner = recovered();
        let (accepted, dispatch) = accepted_and_dispatch_place(&fixture);
        prepare_stage6d_paper_dispatch(&mut owner, accepted, dispatch.clone()).unwrap();
        let authority = owner
            .authorize_stage8a4_durable_batch_source(&fixture.identity, &snapshot)
            .unwrap();
        let request_fingerprint = initial_request_state_fingerprint(&owner, &authority).unwrap();
        let (transition, suffix) = crate::stage6_reconciliation_v2::tests::i3_batch_fixture(
            &fixture.identity,
            &dispatch,
            authority.durable_request_binding_sha256().unwrap(),
            Stage6Sha256Digest::parse(authority.durable_frontier_sha256().to_string()).unwrap(),
            1,
            digest('f'),
            request_fingerprint,
            2,
        );

        owner
            .journal_mut()
            .append_versioned(&Stage6JournalRecordVersioned::V2(transition.clone()))
            .unwrap();
        owner.refresh_after_append().unwrap();
        let authority = owner
            .authorize_stage8a4_durable_batch_source(&fixture.identity, &snapshot)
            .unwrap();
        let receipt = append_stage8a4_durable_batch(
            &mut owner,
            authority,
            Stage6Stage8a4DurableBatch::new(transition, suffix, None).unwrap(),
        )
        .unwrap();
        assert!(receipt.transition_was_existing());
        assert_eq!(receipt.appended_suffix_records(), 2);
        assert_eq!(owner.journal.versioned_records().len(), 5);
    }

    #[test]
    fn stage8a4_i3_rejects_stale_frontier_and_request_state_before_append() {
        let fixture = place_fixture(951, OrderType::Limit);
        let snapshot =
            Stage6DurableCommandSnapshotV1::from_place(&fixture.identity, &fixture.command)
                .unwrap();
        let mut owner = recovered();
        let (accepted, dispatch) = accepted_and_dispatch_place(&fixture);
        prepare_stage6d_paper_dispatch(&mut owner, accepted, dispatch.clone()).unwrap();
        let authority = owner
            .authorize_stage8a4_durable_batch_source(&fixture.identity, &snapshot)
            .unwrap();
        let before = owner.journal.versioned_records().len();

        let (stale_frontier, no_suffix) = crate::stage6_reconciliation_v2::tests::i3_batch_fixture(
            &fixture.identity,
            &dispatch,
            authority.durable_request_binding_sha256().unwrap(),
            digest('9'),
            1,
            digest('f'),
            initial_request_state_fingerprint(&owner, &authority).unwrap(),
            0,
        );
        assert!(matches!(
            append_stage8a4_durable_batch(
                &mut owner,
                authority,
                Stage6Stage8a4DurableBatch::new(stale_frontier, no_suffix, None).unwrap(),
            ),
            Err(Stage6dLiveCoreError::DurableOrderingViolation)
        ));
        assert_eq!(owner.journal.versioned_records().len(), before);

        let authority = owner
            .authorize_stage8a4_durable_batch_source(&fixture.identity, &snapshot)
            .unwrap();
        let (stale_request, no_suffix) = crate::stage6_reconciliation_v2::tests::i3_batch_fixture(
            &fixture.identity,
            &dispatch,
            authority.durable_request_binding_sha256().unwrap(),
            Stage6Sha256Digest::parse(authority.durable_frontier_sha256().to_string()).unwrap(),
            1,
            digest('f'),
            digest('9'),
            0,
        );
        assert!(matches!(
            append_stage8a4_durable_batch(
                &mut owner,
                authority,
                Stage6Stage8a4DurableBatch::new(stale_request, no_suffix, None).unwrap(),
            ),
            Err(Stage6dLiveCoreError::DurableOrderingViolation)
        ));
        assert_eq!(owner.journal.versioned_records().len(), before);
    }

    #[test]
    fn stage8a4_i3_same_stable_key_with_different_v2_payload_is_hard_conflict() {
        let fixture = place_fixture(952, OrderType::Limit);
        let snapshot =
            Stage6DurableCommandSnapshotV1::from_place(&fixture.identity, &fixture.command)
                .unwrap();
        let mut owner = recovered();
        let (accepted, dispatch) = accepted_and_dispatch_place(&fixture);
        prepare_stage6d_paper_dispatch(&mut owner, accepted, dispatch.clone()).unwrap();
        let authority = owner
            .authorize_stage8a4_durable_batch_source(&fixture.identity, &snapshot)
            .unwrap();
        let request_fingerprint = initial_request_state_fingerprint(&owner, &authority).unwrap();
        let durable_binding = authority.durable_request_binding_sha256().unwrap();
        let expected_frontier =
            Stage6Sha256Digest::parse(authority.durable_frontier_sha256().to_string()).unwrap();
        let (first_transition, first_suffix) =
            crate::stage6_reconciliation_v2::tests::i3_batch_fixture(
                &fixture.identity,
                &dispatch,
                durable_binding.clone(),
                expected_frontier.clone(),
                1,
                digest('f'),
                request_fingerprint.clone(),
                1,
            );
        append_stage8a4_durable_batch(
            &mut owner,
            authority,
            Stage6Stage8a4DurableBatch::new(first_transition, first_suffix, None).unwrap(),
        )
        .unwrap();
        let before = owner.journal.versioned_records().len();

        // The fixture's stable key is intentionally constant. Changing the
        // suffix manifest therefore produces the same key with different V2
        // canonical bytes and must fail before a second append.
        let authority = owner
            .authorize_stage8a4_durable_batch_source(&fixture.identity, &snapshot)
            .unwrap();
        let (conflicting_transition, conflicting_suffix) =
            crate::stage6_reconciliation_v2::tests::i3_batch_fixture(
                &fixture.identity,
                &dispatch,
                durable_binding,
                expected_frontier,
                1,
                digest('f'),
                request_fingerprint,
                2,
            );
        assert!(matches!(
            append_stage8a4_durable_batch(
                &mut owner,
                authority,
                Stage6Stage8a4DurableBatch::new(conflicting_transition, conflicting_suffix, None,)
                    .unwrap(),
            ),
            Err(Stage6dLiveCoreError::DurableOrderingViolation)
        ));
        assert_eq!(owner.journal.versioned_records().len(), before);
    }

    #[test]
    fn stage8a4_i3_cancel_requires_exact_durable_original_place_shape() {
        let mut owner = recovered();
        let original = place_fixture(1, OrderType::Limit);
        let original_snapshot =
            Stage6DurableCommandSnapshotV1::from_place(&original.identity, &original.command)
                .unwrap();
        let (original_accepted, original_dispatch) = accepted_and_dispatch_place(&original);
        let original_observed = Stage6JournalRecordV1::broker_order_observed(
            original.identity.clone(),
            BrokerOrderId::new("ORDER-I3-1"),
            Stage6LifecycleSequence::new(3).unwrap(),
            Some(original_dispatch.journal_record_id().clone()),
            digest('7'),
        )
        .unwrap();
        let original_finalized = Stage6JournalRecordV1::request_finalized(
            original.identity.clone(),
            Stage6RequestFinalDispositionV1::Completed,
            Stage6LifecycleSequence::new(4).unwrap(),
            Some(original_observed.journal_record_id().clone()),
            digest('8'),
        )
        .unwrap();
        for record in [
            original_accepted,
            original_dispatch,
            original_observed,
            original_finalized,
        ] {
            owner.journal_mut().append(&record).unwrap();
        }
        owner.refresh_after_append().unwrap();

        let cancel = cancel_fixture(96, "ORDER-I3-1");
        let cancel_snapshot =
            Stage6DurableCommandSnapshotV1::from_cancel(&cancel.identity, &cancel.command).unwrap();
        let (cancel_accepted, cancel_dispatch) = accepted_and_dispatch_cancel(&cancel);
        prepare_stage6d_paper_dispatch(&mut owner, cancel_accepted, cancel_dispatch.clone())
            .unwrap();
        let authority = owner
            .authorize_stage8a4_durable_batch_source(&cancel.identity, &cancel_snapshot)
            .unwrap();
        let request_fingerprint = initial_request_state_fingerprint(&owner, &authority).unwrap();
        let (transition, suffix) = crate::stage6_reconciliation_v2::tests::i3_batch_fixture(
            &cancel.identity,
            &cancel_dispatch,
            authority.durable_request_binding_sha256().unwrap(),
            Stage6Sha256Digest::parse(authority.durable_frontier_sha256().to_string()).unwrap(),
            1,
            digest('f'),
            request_fingerprint,
            0,
        );
        let before = owner.journal.versioned_records().len();
        let wrong_shape = Stage6DurablePlaceOrderShapeV1::new(
            broker_core::OrderSide::Buy,
            OrderType::Limit,
            Decimal::new(2, 0),
            Some(Decimal::new(2210, 1)),
            broker_core::TimeInForce::Day,
        )
        .unwrap();
        assert!(matches!(
            append_stage8a4_durable_batch(
                &mut owner,
                authority,
                Stage6Stage8a4DurableBatch::new(
                    transition.clone(),
                    suffix.clone(),
                    Some(wrong_shape),
                )
                .unwrap(),
            ),
            Err(Stage6dLiveCoreError::DurableOrderingViolation)
        ));
        assert_eq!(owner.journal.versioned_records().len(), before);

        let authority = owner
            .authorize_stage8a4_durable_batch_source(&cancel.identity, &cancel_snapshot)
            .unwrap();
        let receipt = append_stage8a4_durable_batch(
            &mut owner,
            authority,
            Stage6Stage8a4DurableBatch::new(
                transition,
                suffix,
                original_snapshot.place_order_shape(),
            )
            .unwrap(),
        )
        .unwrap();
        assert!(!receipt.transition_was_existing());
        assert_eq!(receipt.appended_suffix_records(), 0);
    }

    fn restart_from_test_authority(
        journal: Stage6MemoryJournalBackend,
        checkpoint: Stage6JournalCheckpointV1,
    ) -> Result<Stage6dDurableRuntimeRecovered, Stage6dLiveCoreError> {
        recover_stage6d_restart_from_authorities(
            Stage6dStage5RuntimeAuthority::FirstBoot(Box::new(runtime())),
            journal,
            checkpoint,
            None,
        )
    }

    fn stage6d_stage5g_working_restart_fixture() -> (
        Stage6dDurableRuntimeRecovered,
        Stage6ePaperFreshBrokerTruthInput,
        Stage5gLifecycleCommitmentKey,
    ) {
        let (restart, attribution) = crate::stage5g_order_position::tests::
            stage6e_restored_generated_working_fixture_with_attribution();
        let projection = restart.fresh_truth_reducer_projection();
        let slot = projection.slots.first().expect("working Stage 5G slot");
        let request_uuid = Uuid::parse_str(&slot.command_request_id).expect("request UUID");
        let request_id = StrategyRequestId::from(request_uuid);
        let side = slot.side.unwrap_or(OrderSide::Buy);
        let qty = slot.target_qty.unwrap_or(Decimal::ONE);
        let order_type = match slot.source_action {
            crate::Stage5gMockIntentAction::Place {
                place_kind: crate::Stage5gMockPlaceKind::Market,
            } => OrderType::Market,
            crate::Stage5gMockIntentAction::Place {
                place_kind: crate::Stage5gMockPlaceKind::Limit,
            } => OrderType::Limit,
            crate::Stage5gMockIntentAction::Cancel { .. } => {
                panic!("working fixture must be Place")
            }
        };
        let command = PlaceOrder {
            request_id,
            created_ts: Utc.with_ymd_and_hms(2030, 1, 1, 0, 0, 0).unwrap(),
            ttl_ms: Some(5_000),
            account_id: projection.account_id.clone(),
            client_order_id: slot.command_client_order_id.clone(),
            instrument: projection.instrument_id.clone(),
            side,
            order_type,
            qty,
            limit_price: (order_type == OrderType::Limit).then_some(Decimal::new(2200, 0)),
            time_in_force: TimeInForce::Day,
            comment: Some(attribution.internal_comment().to_string()),
        };
        let identity = Stage6DurableRequestIdentityV1::from_place(&command, attribution).unwrap();
        let snapshot = Stage6DurableCommandSnapshotV1::from_place(&identity, &command).unwrap();
        let accepted = Stage6JournalRecordV1::request_accepted(
            identity.clone(),
            snapshot,
            Stage6LifecycleSequence::new(1).unwrap(),
            None,
            None,
            digest('8'),
        )
        .unwrap();
        let dispatch = Stage6JournalRecordV1::dispatch_attempt_recorded(
            identity,
            1,
            accepted.canonical_payload_sha256().clone(),
            Stage6LifecycleSequence::new(2).unwrap(),
            Some(accepted.journal_record_id().clone()),
            digest('9'),
        )
        .unwrap();
        let mut journal = Stage6MemoryJournalBackend::new();
        journal.append(&accepted).unwrap();
        journal.append(&dispatch).unwrap();
        let checkpoint =
            Stage6JournalCheckpointV1::from_frontier(journal.frontier().clone()).unwrap();

        let observed_at = Utc.with_ymd_and_hms(2030, 1, 1, 0, 0, 10).unwrap();
        let broker_order_id = slot
            .target_broker_order_id
            .clone()
            .unwrap_or_else(|| BrokerOrderId::new("PAPER-STAGE5G-WORKING-1"));
        let order = BrokerOrderSnapshot {
            account_id: projection.account_id.clone(),
            broker_order_id: Some(broker_order_id),
            client_order_id: slot.target_order_client_order_id.clone(),
            instrument: projection.instrument_id.clone(),
            side,
            order_type,
            time_in_force: Some(TimeInForce::Day),
            status: OrderStatus::Working,
            lifecycle: BrokerOrderSnapshot::lifecycle_for(&OrderStatus::Working),
            qty,
            filled_qty: Decimal::ZERO,
            remaining_qty: Some(qty),
            limit_price: (order_type == OrderType::Limit).then_some(Decimal::new(2200, 0)),
            broker_asset_id: None,
            board: None,
            expiration_date: None,
            source_ts: Some(observed_at),
            received_ts: observed_at,
        };
        let input = Stage6ePaperFreshBrokerTruthInput {
            package_id: "stage6d-working-package-1".to_string(),
            snapshot_epoch: "stage6d-working-epoch-1".to_string(),
            collection_started_at: observed_at,
            captured_at: observed_at,
            orders_observed_at: observed_at,
            trades_observed_at: observed_at,
            positions_observed_at: observed_at,
            orders_complete: true,
            trades_complete: true,
            positions_complete: true,
            orders: vec![order],
            trades: vec![],
            positions: vec![],
        };
        let recovered = recover_stage6d_restart_from_authorities(
            Stage6dStage5RuntimeAuthority::Restart(Box::new(restart)),
            journal,
            checkpoint,
            Some(operational_config()),
        )
        .unwrap();
        let key = Stage5gLifecycleCommitmentKey::from_secret_bytes(&[0x5a; 32]).unwrap();
        (recovered, input, key)
    }

    fn stage6d_stage5g_terminal_restart_fixture() -> (
        Stage6dDurableRuntimeRecovered,
        Stage6ePaperFreshBrokerTruthInput,
        Stage5gLifecycleCommitmentKey,
    ) {
        let (restart, attribution) = crate::stage5g_order_position::tests::
            stage6e_restored_terminal_fixture_with_attribution();
        let projection = restart.fresh_truth_reducer_projection();
        let slot = projection.slots.first().expect("terminal Stage 5G slot");
        let request_id = StrategyRequestId::from(
            Uuid::parse_str(&slot.command_request_id).expect("request UUID"),
        );
        let side = slot.side.unwrap_or(OrderSide::Buy);
        let qty = slot.target_qty.unwrap_or(Decimal::ONE);
        let order_type = slot
            .latest_order
            .as_ref()
            .map(|order| order.order_type)
            .unwrap_or(OrderType::Market);
        let command = PlaceOrder {
            request_id,
            created_ts: Utc.with_ymd_and_hms(2031, 1, 1, 0, 0, 0).unwrap(),
            ttl_ms: Some(5_000),
            account_id: projection.account_id.clone(),
            client_order_id: slot.command_client_order_id.clone(),
            instrument: projection.instrument_id.clone(),
            side,
            order_type,
            qty,
            limit_price: slot
                .latest_order
                .as_ref()
                .and_then(|order| order.limit_price),
            time_in_force: TimeInForce::Day,
            comment: Some(attribution.internal_comment().to_string()),
        };
        let identity = Stage6DurableRequestIdentityV1::from_place(&command, attribution).unwrap();
        let snapshot = Stage6DurableCommandSnapshotV1::from_place(&identity, &command).unwrap();
        let accepted = Stage6JournalRecordV1::request_accepted(
            identity.clone(),
            snapshot,
            Stage6LifecycleSequence::new(1).unwrap(),
            None,
            None,
            digest('a'),
        )
        .unwrap();
        let dispatch = Stage6JournalRecordV1::dispatch_attempt_recorded(
            identity,
            1,
            accepted.canonical_payload_sha256().clone(),
            Stage6LifecycleSequence::new(2).unwrap(),
            Some(accepted.journal_record_id().clone()),
            digest('b'),
        )
        .unwrap();
        let mut journal = Stage6MemoryJournalBackend::new();
        journal.append(&accepted).unwrap();
        journal.append(&dispatch).unwrap();
        let checkpoint =
            Stage6JournalCheckpointV1::from_frontier(journal.frontier().clone()).unwrap();

        let observed_at = Utc.with_ymd_and_hms(2031, 1, 1, 0, 0, 10).unwrap();
        let mut orders = slot.latest_order.clone().into_iter().collect::<Vec<_>>();
        for order in &mut orders {
            order.received_ts = observed_at;
        }
        let mut trades = slot.trades.clone();
        for trade in &mut trades {
            trade.received_ts = observed_at;
        }
        let mut positions = slot.position.clone().into_iter().collect::<Vec<_>>();
        for position in &mut positions {
            position.received_ts = observed_at;
        }
        let input = Stage6ePaperFreshBrokerTruthInput {
            package_id: "stage6d-terminal-exact-package".to_string(),
            snapshot_epoch: "stage6d-terminal-exact-epoch".to_string(),
            collection_started_at: observed_at,
            captured_at: observed_at,
            orders_observed_at: observed_at,
            trades_observed_at: observed_at,
            positions_observed_at: observed_at,
            orders_complete: true,
            trades_complete: true,
            positions_complete: true,
            orders,
            trades,
            positions,
        };
        let recovered = recover_stage6d_restart_from_authorities(
            Stage6dStage5RuntimeAuthority::Restart(Box::new(restart)),
            journal,
            checkpoint,
            Some(operational_config()),
        )
        .unwrap();
        let key = Stage5gLifecycleCommitmentKey::from_secret_bytes(&[0x5a; 32]).unwrap();
        (recovered, input, key)
    }

    fn issue_first_stage6e_fixture(
        recovered: &Stage6dDurableRuntimeRecovered,
        input: Stage6ePaperFreshBrokerTruthInput,
    ) -> Result<Stage6eAcceptedFreshBrokerTruth, Stage6dLiveCoreError> {
        let request_id = *recovered
            .active_cross_bound_request_ids()
            .first()
            .expect("Stage 6E fixture has one active cross-bound request");
        let validation_observed_at = input.captured_at;
        issue_stage6e_paper_fresh_broker_truth_for_request_at(
            recovered,
            request_id,
            input,
            validation_observed_at,
        )
    }

    fn retime_stage6e_input_after_current_restore(
        recovered: &Stage6dDurableRuntimeRecovered,
        mut input: Stage6ePaperFreshBrokerTruthInput,
    ) -> (Stage6ePaperFreshBrokerTruthInput, DateTime<Utc>) {
        let restore = recovered
            .current_restore_completed_at()
            .expect("restart fixture owns current-process restore epoch");
        let collection_started_at = restore + chrono::Duration::seconds(1);
        let section_observed_at = restore + chrono::Duration::seconds(2);
        let captured_at = restore + chrono::Duration::seconds(3);
        let validation_observed_at = restore + chrono::Duration::seconds(4);
        input.collection_started_at = collection_started_at;
        input.orders_observed_at = section_observed_at;
        input.trades_observed_at = section_observed_at;
        input.positions_observed_at = section_observed_at;
        input.captured_at = captured_at;
        for order in &mut input.orders {
            order.received_ts = section_observed_at;
            order.source_ts = Some(section_observed_at);
        }
        for trade in &mut input.trades {
            trade.received_ts = section_observed_at;
            trade.source_ts = section_observed_at;
        }
        for position in &mut input.positions {
            position.received_ts = section_observed_at;
            position.source_ts = Some(section_observed_at);
        }
        (input, validation_observed_at)
    }

    #[derive(Clone, Copy)]
    enum Stage6eWorkingBindingMutation {
        None,
        Account,
        Instrument,
        Attribution,
        Action,
    }

    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Stage6eExtraStage6History {
        None,
        Finalized,
        Unresolved,
    }

    fn stage6e_working_cross_binding_recovery(
        mutation: Stage6eWorkingBindingMutation,
        extra_history: Stage6eExtraStage6History,
    ) -> Result<Stage6dDurableRuntimeRecovered, Stage6dLiveCoreError> {
        let (restart, source_attribution) = crate::stage5g_order_position::tests::
            stage6e_restored_generated_working_fixture_with_attribution();
        let projection = restart.fresh_truth_reducer_projection();
        let slot = projection.slots.first().expect("working Stage 5G slot");
        let request_id = StrategyRequestId::from(
            Uuid::parse_str(&slot.command_request_id).expect("request UUID"),
        );
        let mut account_id = projection.account_id.clone();
        let mut target_instrument = projection.instrument_id.clone();
        let mut command_attribution = source_attribution;
        match mutation {
            Stage6eWorkingBindingMutation::Account => {
                account_id = BrokerAccountId::new("ACC_WRONG_0001")
            }
            Stage6eWorkingBindingMutation::Instrument => {
                target_instrument.symbol = "RTS-9.26".to_string();
                target_instrument.venue_symbol = Some("RTS-9.26@RTSX".to_string());
            }
            Stage6eWorkingBindingMutation::Attribution => {
                command_attribution = HybridRuntimeAttribution::parse_source_comment(
                    command_attribution
                        .internal_comment()
                        .replace("|c=", "|c=drift"),
                )
                .expect("drifted attribution remains structurally valid");
            }
            Stage6eWorkingBindingMutation::None | Stage6eWorkingBindingMutation::Action => {}
        }

        let (accepted, dispatch) = if matches!(mutation, Stage6eWorkingBindingMutation::Action) {
            let command = CancelOrder {
                request_id,
                created_ts: Utc.with_ymd_and_hms(2030, 1, 1, 0, 0, 0).unwrap(),
                ttl_ms: Some(5_000),
                account_id,
                order_id: BrokerOrderId::new("UNRELATED-CANCEL-TARGET"),
                client_order_id: None,
            };
            let cancel_attribution = HybridRuntimeAttribution::parse_source_comment(
                command_attribution
                    .internal_comment()
                    .replace("|r=ENTRY", "|r=CANCEL"),
            )
            .expect("cancel attribution remains canonical");
            let identity = Stage6DurableRequestIdentityV1::from_cancel(
                &command,
                target_instrument,
                cancel_attribution,
            )?;
            let snapshot = Stage6DurableCommandSnapshotV1::from_cancel(&identity, &command)?;
            let accepted = Stage6JournalRecordV1::request_accepted(
                identity.clone(),
                snapshot,
                Stage6LifecycleSequence::new(1)?,
                None,
                None,
                digest('d'),
            )?;
            let dispatch = Stage6JournalRecordV1::dispatch_attempt_recorded(
                identity,
                1,
                accepted.canonical_payload_sha256().clone(),
                Stage6LifecycleSequence::new(2)?,
                Some(accepted.journal_record_id().clone()),
                digest('e'),
            )?;
            (accepted, dispatch)
        } else {
            let command = PlaceOrder {
                request_id,
                created_ts: Utc.with_ymd_and_hms(2030, 1, 1, 0, 0, 0).unwrap(),
                ttl_ms: Some(5_000),
                account_id,
                client_order_id: slot.command_client_order_id.clone(),
                instrument: target_instrument,
                side: slot.side.unwrap_or(OrderSide::Buy),
                order_type: OrderType::Market,
                qty: slot.target_qty.unwrap_or(Decimal::ONE),
                limit_price: None,
                time_in_force: TimeInForce::Day,
                comment: Some(command_attribution.internal_comment().to_string()),
            };
            let identity =
                Stage6DurableRequestIdentityV1::from_place(&command, command_attribution)?;
            let snapshot = Stage6DurableCommandSnapshotV1::from_place(&identity, &command)?;
            let accepted = Stage6JournalRecordV1::request_accepted(
                identity.clone(),
                snapshot,
                Stage6LifecycleSequence::new(1)?,
                None,
                None,
                digest('d'),
            )?;
            let dispatch = Stage6JournalRecordV1::dispatch_attempt_recorded(
                identity,
                1,
                accepted.canonical_payload_sha256().clone(),
                Stage6LifecycleSequence::new(2)?,
                Some(accepted.journal_record_id().clone()),
                digest('e'),
            )?;
            (accepted, dispatch)
        };

        let mut journal = Stage6MemoryJournalBackend::new();
        if extra_history != Stage6eExtraStage6History::None {
            let historical = place_fixture(800, OrderType::Limit);
            let (historical_accepted, _) = accepted_and_dispatch_place(&historical);
            journal.append(&historical_accepted)?;
            if extra_history == Stage6eExtraStage6History::Finalized {
                let historical_finalized = Stage6JournalRecordV1::request_finalized(
                    historical.identity,
                    Stage6RequestFinalDispositionV1::Completed,
                    Stage6LifecycleSequence::new(2)?,
                    Some(historical_accepted.journal_record_id().clone()),
                    digest('f'),
                )?;
                journal.append(&historical_finalized)?;
            }
        }
        journal.append(&accepted)?;
        journal.append(&dispatch)?;
        let checkpoint = Stage6JournalCheckpointV1::from_frontier(journal.frontier().clone())?;
        recover_stage6d_restart_from_authorities(
            Stage6dStage5RuntimeAuthority::Restart(Box::new(restart)),
            journal,
            checkpoint,
            Some(operational_config()),
        )
    }

    fn stage6e_cancel_cross_binding_recovery(
        drift_target: bool,
    ) -> Result<Stage6dDurableRuntimeRecovered, Stage6dLiveCoreError> {
        let (restart, attribution) =
            crate::stage5g_order_position::tests::stage6e_restored_cancel_fixture_with_attribution(
            );
        let projection = restart.fresh_truth_reducer_projection();
        let slot = projection.slots.first().expect("cancel Stage 5G slot");
        let request_id = StrategyRequestId::from(
            Uuid::parse_str(&slot.command_request_id).expect("request UUID"),
        );
        let expected_target = match &slot.source_action {
            crate::Stage5gMockIntentAction::Cancel { target_order_id } => target_order_id.clone(),
            crate::Stage5gMockIntentAction::Place { .. } => panic!("cancel fixture action drift"),
        };
        let command = CancelOrder {
            request_id,
            created_ts: Utc.with_ymd_and_hms(2030, 1, 1, 0, 1, 0).unwrap(),
            ttl_ms: Some(5_000),
            account_id: projection.account_id.clone(),
            order_id: if drift_target {
                BrokerOrderId::new("WRONG-CANCEL-TARGET")
            } else {
                expected_target
            },
            client_order_id: slot.target_order_client_order_id.clone(),
        };
        let identity = Stage6DurableRequestIdentityV1::from_cancel(
            &command,
            projection.instrument_id.clone(),
            attribution,
        )?;
        let snapshot = Stage6DurableCommandSnapshotV1::from_cancel(&identity, &command)?;
        let accepted = Stage6JournalRecordV1::request_accepted(
            identity.clone(),
            snapshot,
            Stage6LifecycleSequence::new(1)?,
            None,
            None,
            digest('6'),
        )?;
        let dispatch = Stage6JournalRecordV1::dispatch_attempt_recorded(
            identity,
            1,
            accepted.canonical_payload_sha256().clone(),
            Stage6LifecycleSequence::new(2)?,
            Some(accepted.journal_record_id().clone()),
            digest('7'),
        )?;
        let mut journal = Stage6MemoryJournalBackend::new();
        journal.append(&accepted)?;
        journal.append(&dispatch)?;
        let checkpoint = Stage6JournalCheckpointV1::from_frontier(journal.frontier().clone())?;
        recover_stage6d_restart_from_authorities(
            Stage6dStage5RuntimeAuthority::Restart(Box::new(restart)),
            journal,
            checkpoint,
            Some(operational_config()),
        )
    }

    fn stage6e_two_place_cross_binding_recovery() -> (
        Stage6dDurableRuntimeRecovered,
        Vec<StrategyRequestId>,
        Stage6ePaperFreshBrokerTruthInput,
        Stage5gLifecycleCommitmentKey,
    ) {
        let (restart, attributions) = crate::stage5g_order_position::tests::
            stage6e_restored_two_place_fixture_with_attributions();
        let projection = restart.fresh_truth_reducer_projection();
        assert_eq!(projection.slots.len(), 2);
        assert_eq!(attributions.len(), 2);
        assert_ne!(
            stage5g_attribution_fingerprint_sha256(&attributions[0]),
            stage5g_attribution_fingerprint_sha256(&attributions[1]),
            "two current requests retain distinct source attribution authority"
        );
        let mut journal = Stage6MemoryJournalBackend::new();
        let mut request_ids = Vec::new();
        for (index, (slot, attribution)) in projection.slots.iter().zip(attributions).enumerate() {
            let request_id = StrategyRequestId::from(
                Uuid::parse_str(&slot.command_request_id).expect("request UUID"),
            );
            request_ids.push(request_id);
            let command = PlaceOrder {
                request_id,
                created_ts: Utc
                    .with_ymd_and_hms(2032, 1, 1, 0, 0, index as u32)
                    .unwrap(),
                ttl_ms: Some(5_000),
                account_id: projection.account_id.clone(),
                client_order_id: slot.command_client_order_id.clone(),
                instrument: projection.instrument_id.clone(),
                side: slot.side.unwrap_or(OrderSide::Buy),
                order_type: OrderType::Market,
                qty: slot.target_qty.unwrap_or(Decimal::ONE),
                limit_price: None,
                time_in_force: TimeInForce::Day,
                comment: Some(attribution.internal_comment().to_string()),
            };
            let identity = Stage6DurableRequestIdentityV1::from_place(&command, attribution)
                .expect("two-place durable identity");
            let snapshot = Stage6DurableCommandSnapshotV1::from_place(&identity, &command)
                .expect("two-place command snapshot");
            let accepted = Stage6JournalRecordV1::request_accepted(
                identity.clone(),
                snapshot,
                Stage6LifecycleSequence::new(1).unwrap(),
                None,
                None,
                digest(if index == 0 { '1' } else { '3' }),
            )
            .unwrap();
            let dispatch = Stage6JournalRecordV1::dispatch_attempt_recorded(
                identity,
                1,
                accepted.canonical_payload_sha256().clone(),
                Stage6LifecycleSequence::new(2).unwrap(),
                Some(accepted.journal_record_id().clone()),
                digest(if index == 0 { '2' } else { '4' }),
            )
            .unwrap();
            journal.append(&accepted).unwrap();
            journal.append(&dispatch).unwrap();
        }
        let checkpoint = Stage6JournalCheckpointV1::from_frontier(journal.frontier().clone())
            .expect("two-place checkpoint");
        let recovered = recover_stage6d_restart_from_authorities(
            Stage6dStage5RuntimeAuthority::Restart(Box::new(restart)),
            journal,
            checkpoint,
            Some(operational_config()),
        )
        .expect("two-place cross-binding recovery");
        let observed_at = Utc.with_ymd_and_hms(2032, 1, 1, 0, 1, 0).unwrap();
        let input = Stage6ePaperFreshBrokerTruthInput {
            package_id: "stage6e-r1-two-place-package".to_string(),
            snapshot_epoch: "stage6e-r1-two-place-epoch".to_string(),
            collection_started_at: observed_at,
            captured_at: observed_at,
            orders_observed_at: observed_at,
            trades_observed_at: observed_at,
            positions_observed_at: observed_at,
            orders_complete: true,
            trades_complete: true,
            positions_complete: true,
            orders: vec![],
            trades: vec![],
            positions: vec![],
        };
        let key = Stage5gLifecycleCommitmentKey::from_secret_bytes(&[0x5a; 32]).unwrap();
        (recovered, request_ids, input, key)
    }

    fn stage6e_mixed_place_cancel_cross_binding_recovery(
        drift_cancel_target: bool,
    ) -> Result<(Stage6dDurableRuntimeRecovered, Vec<StrategyRequestId>), Stage6dLiveCoreError>
    {
        let (restart, attributions) = crate::stage5g_order_position::tests::
            stage6e_restored_mixed_place_cancel_fixture_with_attributions();
        let projection = restart.fresh_truth_reducer_projection();
        if projection.slots.len() != 2 || attributions.len() != 2 {
            return Err(Stage6dLiveCoreError::RestartSemanticCrossBindingMismatch);
        }
        let mut journal = Stage6MemoryJournalBackend::new();
        let mut request_ids = Vec::new();
        for (index, (slot, attribution)) in projection.slots.iter().zip(attributions).enumerate() {
            let request_id = StrategyRequestId::from(
                Uuid::parse_str(&slot.command_request_id).expect("mixed request UUID"),
            );
            request_ids.push(request_id);
            let identity_and_snapshot = match &slot.source_action {
                crate::Stage5gMockIntentAction::Place { .. } => {
                    let command = PlaceOrder {
                        request_id,
                        created_ts: Utc
                            .with_ymd_and_hms(2033, 1, 1, 0, 0, index as u32)
                            .unwrap(),
                        ttl_ms: Some(5_000),
                        account_id: projection.account_id.clone(),
                        client_order_id: slot.command_client_order_id.clone(),
                        instrument: projection.instrument_id.clone(),
                        side: slot.side.unwrap_or(OrderSide::Buy),
                        order_type: OrderType::Market,
                        qty: slot.target_qty.unwrap_or(Decimal::ONE),
                        limit_price: None,
                        time_in_force: TimeInForce::Day,
                        comment: Some(attribution.internal_comment().to_string()),
                    };
                    let identity =
                        Stage6DurableRequestIdentityV1::from_place(&command, attribution)?;
                    let snapshot = Stage6DurableCommandSnapshotV1::from_place(&identity, &command)?;
                    (identity, snapshot)
                }
                crate::Stage5gMockIntentAction::Cancel { target_order_id } => {
                    let command = CancelOrder {
                        request_id,
                        created_ts: Utc
                            .with_ymd_and_hms(2033, 1, 1, 0, 0, index as u32)
                            .unwrap(),
                        ttl_ms: Some(5_000),
                        account_id: projection.account_id.clone(),
                        order_id: if drift_cancel_target {
                            BrokerOrderId::new("STAGE6E-R1-WRONG-MIXED-TARGET")
                        } else {
                            target_order_id.clone()
                        },
                        client_order_id: slot.target_order_client_order_id.clone(),
                    };
                    let identity = Stage6DurableRequestIdentityV1::from_cancel(
                        &command,
                        projection.instrument_id.clone(),
                        attribution,
                    )?;
                    let snapshot =
                        Stage6DurableCommandSnapshotV1::from_cancel(&identity, &command)?;
                    (identity, snapshot)
                }
            };
            let (identity, snapshot) = identity_and_snapshot;
            let accepted = Stage6JournalRecordV1::request_accepted(
                identity.clone(),
                snapshot,
                Stage6LifecycleSequence::new(1)?,
                None,
                None,
                digest(if index == 0 { '5' } else { '7' }),
            )?;
            let dispatch = Stage6JournalRecordV1::dispatch_attempt_recorded(
                identity,
                1,
                accepted.canonical_payload_sha256().clone(),
                Stage6LifecycleSequence::new(2)?,
                Some(accepted.journal_record_id().clone()),
                digest(if index == 0 { '6' } else { '8' }),
            )?;
            journal.append(&accepted)?;
            journal.append(&dispatch)?;
        }
        let checkpoint = Stage6JournalCheckpointV1::from_frontier(journal.frontier().clone())?;
        let recovered = recover_stage6d_restart_from_authorities(
            Stage6dStage5RuntimeAuthority::Restart(Box::new(restart)),
            journal,
            checkpoint,
            Some(operational_config()),
        )?;
        Ok((recovered, request_ids))
    }

    #[test]
    fn stage6d_first_boot_requires_explicit_create_authority() {
        let runtime = runtime();
        let mut config = first_boot_config(&runtime);
        config.allow_create_missing_journal = false;
        assert!(matches!(
            authorize_stage6d_first_boot(config),
            Err(Stage6dLiveCoreError::FirstBootNotAuthorized)
        ));
    }

    #[test]
    fn stage6d_first_boot_rejects_runtime_config_drift() {
        let runtime = runtime();
        let mut config = first_boot_config(&runtime);
        config.expected_runtime_config_fingerprint_sha256 = "a".repeat(64);
        let authority = authorize_stage6d_first_boot(config).unwrap();
        assert!(matches!(
            first_boot_stage6d_paper(authority, runtime),
            Err(Stage6dLiveCoreError::FirstBootRuntimeConfigMismatch)
        ));
    }

    #[test]
    fn stage6d_first_boot_creates_exact_empty_journal() {
        let runtime = runtime();
        let authority = authorize_stage6d_first_boot(first_boot_config(&runtime)).unwrap();
        let recovered = first_boot_stage6d_paper(authority, runtime).unwrap();
        assert_eq!(recovered.boot_mode(), Stage6dBootMode::FirstBoot);
        assert_eq!(recovered.journal_frontier().frame_count(), 0);
        assert!(recovered.replay().requests().is_empty());
        assert_eq!(
            recovered.first_boot_deployment_id(),
            Some("paper-imoexf-stage6d")
        );
    }

    #[test]
    fn stage7b_first_boot_transfers_single_file_journal_authority() {
        let path = std::env::temp_dir().join(format!(
            "stage7b-owned-runtime-{}-{}.journal",
            std::process::id(),
            STAGE7B_TEST_FILE_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let runtime = runtime();
        let authority = authorize_stage6d_first_boot(first_boot_config(&runtime)).unwrap();
        let journal = Stage6OwnedJournalBackend::from_file(
            crate::Stage6FileJournalBackend::create_new(&path).unwrap(),
        );
        let recovered =
            first_boot_stage6d_paper_with_owned_journal(authority, runtime, journal).unwrap();
        assert!(recovered.journal_is_file_backed());
        assert_eq!(recovered.journal_frontier().frame_count(), 0);
        drop(recovered);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn stage6d_first_boot_fingerprint_is_deterministic() {
        let runtime_a = runtime();
        let auth_a = authorize_stage6d_first_boot(first_boot_config(&runtime_a)).unwrap();
        let a = first_boot_stage6d_paper(auth_a, runtime_a).unwrap();
        let runtime_b = runtime();
        let auth_b = authorize_stage6d_first_boot(first_boot_config(&runtime_b)).unwrap();
        let b = first_boot_stage6d_paper(auth_b, runtime_b).unwrap();
        assert_eq!(
            a.integration_fingerprint_sha256(),
            b.integration_fingerprint_sha256()
        );
        assert_eq!(a.current_process_generation_id(), None);
        assert_eq!(b.current_process_generation_id(), None);
    }

    #[test]
    fn stage6d_restart_missing_journal_fails_before_package_decode() {
        let key = Stage5gLifecycleCommitmentKey::from_secret_bytes(&[0x61; 32]).unwrap();
        assert!(matches!(
            restart_stage6d_paper(b"not-json", &key, runtime(), None),
            Err(Stage6dLiveCoreError::RestartJournalMissing)
        ));
    }

    #[test]
    fn stage6d_restart_wrapper_binds_stage5_bytes_and_checkpoint() {
        let key = Stage5gLifecycleCommitmentKey::from_secret_bytes(&[0x62; 32]).unwrap();
        let journal = Stage6MemoryJournalBackend::new();
        let checkpoint =
            Stage6JournalCheckpointV1::from_frontier(journal.frontier().clone()).unwrap();
        let bytes = seal_stage6d_restart_package(
            b"authenticated-stage5g-bytes",
            checkpoint,
            operational_config(),
            &key,
        )
        .unwrap();
        let decoded = decode_and_authenticate_restart_package(&bytes, &key).unwrap();
        assert_eq!(
            decoded.stage5g_restart_package,
            b"authenticated-stage5g-bytes"
        );
    }

    #[test]
    fn stage6d_authenticated_stage5g_and_exact_journal_restart_end_to_end() {
        let (stage5g_bytes, key, fresh_runtime) =
            crate::stage5g_protective_completion::stage6d_test_authenticated_restart_fixture();
        let journal = Stage6MemoryJournalBackend::new();
        let checkpoint =
            Stage6JournalCheckpointV1::from_frontier(journal.frontier().clone()).unwrap();
        let journal_bytes = journal.framed_bytes().unwrap();
        let package =
            seal_stage6d_restart_package(&stage5g_bytes, checkpoint, operational_config(), &key)
                .unwrap();
        let recovered =
            restart_stage6d_paper(&package, &key, fresh_runtime, Some(journal_bytes)).unwrap();
        assert_eq!(recovered.boot_mode(), Stage6dBootMode::Restart);
        assert_eq!(recovered.journal_frontier().frame_count(), 0);
        assert!(recovered.replay().requests().is_empty());
        assert!(recovered.first_boot_deployment_id().is_none());
    }

    #[test]
    fn stage6d_restart_wrapper_wrong_key_fails_closed() {
        let key = Stage5gLifecycleCommitmentKey::from_secret_bytes(&[0x63; 32]).unwrap();
        let wrong = Stage5gLifecycleCommitmentKey::from_secret_bytes(&[0x64; 32]).unwrap();
        let journal = Stage6MemoryJournalBackend::new();
        let checkpoint =
            Stage6JournalCheckpointV1::from_frontier(journal.frontier().clone()).unwrap();
        let bytes = seal_stage6d_restart_package(
            b"authenticated-stage5g-bytes",
            checkpoint,
            operational_config(),
            &key,
        )
        .unwrap();
        assert!(matches!(
            decode_and_authenticate_restart_package(&bytes, &wrong),
            Err(Stage6dLiveCoreError::RestartAuthenticationFailed)
        ));
    }

    #[test]
    fn stage6d_restart_wrapper_stage5_bytes_tamper_fails_closed() {
        let key = Stage5gLifecycleCommitmentKey::from_secret_bytes(&[0x65; 32]).unwrap();
        let journal = Stage6MemoryJournalBackend::new();
        let checkpoint =
            Stage6JournalCheckpointV1::from_frontier(journal.frontier().clone()).unwrap();
        let bytes = seal_stage6d_restart_package(
            b"authenticated-stage5g-bytes",
            checkpoint,
            operational_config(),
            &key,
        )
        .unwrap();
        let mut package: Stage6dAuthenticatedRestartPackageV1 =
            serde_json::from_slice(&bytes).unwrap();
        package.stage5g_restart_package[0] = 0;
        let forged = serde_json::to_vec(&package).unwrap();
        assert!(matches!(
            decode_and_authenticate_restart_package(&forged, &key),
            Err(Stage6dLiveCoreError::Stage5gPackageDigestMismatch)
        ));
    }

    #[test]
    fn stage6d_restart_wrapper_checkpoint_tamper_fails_closed() {
        let key = Stage5gLifecycleCommitmentKey::from_secret_bytes(&[0x66; 32]).unwrap();
        let journal = Stage6MemoryJournalBackend::new();
        let checkpoint =
            Stage6JournalCheckpointV1::from_frontier(journal.frontier().clone()).unwrap();
        let bytes = seal_stage6d_restart_package(
            b"authenticated-stage5g-bytes",
            checkpoint,
            operational_config(),
            &key,
        )
        .unwrap();
        let mut package: Stage6dAuthenticatedRestartPackageV1 =
            serde_json::from_slice(&bytes).unwrap();
        package.stage6_checkpoint_bytes_sha256 = "a".repeat(64);
        let forged = serde_json::to_vec(&package).unwrap();
        assert!(matches!(
            decode_and_authenticate_restart_package(&forged, &key),
            Err(Stage6dLiveCoreError::CheckpointDigestMismatch)
        ));
    }

    #[test]
    fn stage6d_restart_wrapper_operational_identity_tamper_fails_closed() {
        let key = Stage5gLifecycleCommitmentKey::from_secret_bytes(&[0x67; 32]).unwrap();
        let journal = Stage6MemoryJournalBackend::new();
        let checkpoint =
            Stage6JournalCheckpointV1::from_frontier(journal.frontier().clone()).unwrap();
        let bytes = seal_stage6d_restart_package(
            b"authenticated-stage5g-bytes",
            checkpoint,
            operational_config(),
            &key,
        )
        .unwrap();
        let mut package: Stage6dAuthenticatedRestartPackageV1 =
            serde_json::from_slice(&bytes).unwrap();
        package.operational_identity.deployment_generation += 1;
        let forged = serde_json::to_vec(&package).unwrap();
        assert!(matches!(
            decode_and_authenticate_restart_package(&forged, &key),
            Err(Stage6dLiveCoreError::OperationalIdentityInvalid)
        ));
    }

    #[test]
    fn stage6d_closed_surfaces_remain_false() {
        let runtime = runtime();
        let authority = authorize_stage6d_first_boot(first_boot_config(&runtime)).unwrap();
        let recovered = first_boot_stage6d_paper(authority, runtime).unwrap();
        assert!(!recovered.redis_command_consumer_attached());
        assert!(!recovered.finam_transport_attached());
        assert!(!recovered.broker_network_dispatch_attached());
        assert!(!recovered.runtime_live_attached());
        assert!(!recovered.real_orders_enabled());
    }

    #[test]
    fn stage6d_market_fill_obeys_durable_before_effect() {
        let fixture = place_fixture(1, OrderType::Market);
        let (accepted, dispatch) = accepted_and_dispatch_place(&fixture);
        let mut recovered = recovered();
        let receipt = prepare_stage6d_paper_dispatch(&mut recovered, accepted, dispatch).unwrap();
        assert_eq!(recovered.journal_frontier().frame_count(), 2);
        let report = execute_stage6d_paper_outcome(
            &mut recovered,
            receipt,
            Stage6dPaperOutcome::MarketFilled {
                broker_order_id: BrokerOrderId::new("PAPER-ORDER-1"),
                broker_trade_id: BrokerTradeId::new("PAPER-TRADE-1"),
            },
        )
        .unwrap();
        assert_eq!(report.final_sequence, 4);
        assert_eq!(
            report.dispatch_safety_state,
            crate::Stage6DispatchSafetyStateV1::DispatchForbidden
        );
        assert_eq!(report.broker_trade_ids.len(), 1);
        let ndjson = report.to_ndjson_line().unwrap();
        assert!(ndjson.contains("PAPER-ORDER-1"));
        assert!(ndjson.contains("runtime_pre_fingerprint_sha256"));
        assert!(ndjson.contains("journal_frontier_sha256"));
        assert!(!report.restart_recovery_marker);
    }

    #[test]
    fn stage6d_limit_pending_records_broker_order_only() {
        let fixture = place_fixture(2, OrderType::Limit);
        let (accepted, dispatch) = accepted_and_dispatch_place(&fixture);
        let mut recovered = recovered();
        let receipt = prepare_stage6d_paper_dispatch(&mut recovered, accepted, dispatch).unwrap();
        let report = execute_stage6d_paper_outcome(
            &mut recovered,
            receipt,
            Stage6dPaperOutcome::LimitPending {
                broker_order_id: BrokerOrderId::new("PAPER-LIMIT-2"),
            },
        )
        .unwrap();
        assert_eq!(report.final_sequence, 3);
        assert!(report.broker_trade_ids.is_empty());
        assert_eq!(
            report.dispatch_safety_state,
            crate::Stage6DispatchSafetyStateV1::DispatchForbidden
        );
    }

    #[test]
    fn stage6d_place_no_order_enables_same_identity_retry() {
        let fixture = place_fixture(3, OrderType::Limit);
        let expected_request = fixture.identity.strategy_request_id();
        let expected_client = fixture.identity.durable_client_order_id().clone();
        let (accepted, dispatch) = accepted_and_dispatch_place(&fixture);
        let mut recovered = recovered();
        let receipt = prepare_stage6d_paper_dispatch(&mut recovered, accepted, dispatch).unwrap();
        let report = execute_stage6d_paper_outcome(
            &mut recovered,
            receipt,
            Stage6dPaperOutcome::PlaceNoBrokerOrderFound,
        )
        .unwrap();
        assert_eq!(report.strategy_request_id, expected_request);
        assert_eq!(
            report.dispatch_safety_state,
            crate::Stage6DispatchSafetyStateV1::RetryEligibleSameIdentity
        );
        let replayed = recovered.replay().request(expected_request).unwrap();
        assert_eq!(replayed.durable_client_order_id(), &expected_client);
    }

    #[test]
    fn stage6d_d3_lost_place_response_recovers_broker_order_and_forbids_dispatch() {
        let fixture = place_fixture(31, OrderType::Limit);
        let (accepted, dispatch) = accepted_and_dispatch_place(&fixture);
        let mut recovered = recovered();
        let receipt = prepare_stage6d_paper_dispatch(&mut recovered, accepted, dispatch).unwrap();
        let report = execute_stage6d_paper_outcome(
            &mut recovered,
            receipt,
            Stage6dPaperOutcome::PlaceBrokerOrderFound {
                broker_order_id: BrokerOrderId::new("RECOVERED-PAPER-ORDER-31"),
            },
        )
        .unwrap();
        assert_eq!(
            report.broker_order_id,
            Some(BrokerOrderId::new("RECOVERED-PAPER-ORDER-31"))
        );
        assert_eq!(
            report.dispatch_safety_state,
            crate::Stage6DispatchSafetyStateV1::DispatchForbidden
        );
    }

    #[test]
    fn stage6d_unknown_dispatch_remains_reconciliation_required() {
        let fixture = place_fixture(4, OrderType::Market);
        let (accepted, dispatch) = accepted_and_dispatch_place(&fixture);
        let mut recovered = recovered();
        let receipt = prepare_stage6d_paper_dispatch(&mut recovered, accepted, dispatch).unwrap();
        let report = execute_stage6d_paper_outcome(
            &mut recovered,
            receipt,
            Stage6dPaperOutcome::Inconclusive,
        )
        .unwrap();
        assert_eq!(
            report.dispatch_safety_state,
            crate::Stage6DispatchSafetyStateV1::ReconciliationRequired
        );
    }

    #[test]
    fn stage6d_cancel_canceled_uses_normalized_cancel_truth() {
        let fixture = cancel_fixture(5, "TARGET-5");
        let (accepted, dispatch) = accepted_and_dispatch_cancel(&fixture);
        let mut recovered = recovered();
        let receipt = prepare_stage6d_paper_dispatch(&mut recovered, accepted, dispatch).unwrap();
        let report = execute_stage6d_paper_outcome(
            &mut recovered,
            receipt,
            Stage6dPaperOutcome::CancelCanceled,
        )
        .unwrap();
        assert_eq!(report.cancel_outcome, Some(Stage6CancelOutcomeV1::Canceled));
        assert_eq!(
            report.dispatch_safety_state,
            crate::Stage6DispatchSafetyStateV1::DispatchForbidden
        );
    }

    #[test]
    fn stage6d_cancel_execution_race_is_preserved() {
        let fixture = cancel_fixture(6, "TARGET-6");
        let (accepted, dispatch) = accepted_and_dispatch_cancel(&fixture);
        let mut recovered = recovered();
        let receipt = prepare_stage6d_paper_dispatch(&mut recovered, accepted, dispatch).unwrap();
        let report = execute_stage6d_paper_outcome(
            &mut recovered,
            receipt,
            Stage6dPaperOutcome::CancelExecutionObserved,
        )
        .unwrap();
        assert_eq!(
            report.cancel_outcome,
            Some(Stage6CancelOutcomeV1::ExecutionObserved)
        );
    }

    #[test]
    fn stage6d_d6_cancel_response_lost_restarts_unresolved_without_redispatch() {
        let fixture = cancel_fixture(61, "TARGET-61");
        let (accepted, dispatch) = accepted_and_dispatch_cancel(&fixture);
        let mut journal = Stage6MemoryJournalBackend::new();
        journal.append(&accepted).unwrap();
        let checkpoint =
            Stage6JournalCheckpointV1::from_frontier(journal.frontier().clone()).unwrap();
        journal.append(&dispatch).unwrap();
        let recovered = restart_from_test_authority(journal, checkpoint).unwrap();
        let request = recovered
            .replay()
            .request(fixture.identity.strategy_request_id())
            .unwrap();
        assert_eq!(
            request.dispatch_safety_state(),
            crate::Stage6DispatchSafetyStateV1::ReconciliationRequired
        );
        assert_eq!(request.dispatch_attempt_count(), 1);
        assert!(request.cancel_outcome().is_none());
    }

    #[test]
    fn stage6d_d7_cancel_execution_observed_survives_restart() {
        let fixture = cancel_fixture(62, "TARGET-62");
        let (accepted, dispatch) = accepted_and_dispatch_cancel(&fixture);
        let cancel = Stage6JournalRecordV1::cancel_outcome_observed(
            fixture.identity.clone(),
            BrokerOrderId::new("TARGET-62"),
            Stage6CancelOutcomeV1::ExecutionObserved,
            Stage6LifecycleSequence::new(3).unwrap(),
            Some(dispatch.journal_record_id().clone()),
            digest('c'),
        )
        .unwrap();
        let mut journal = Stage6MemoryJournalBackend::new();
        journal.append(&accepted).unwrap();
        journal.append(&dispatch).unwrap();
        let checkpoint =
            Stage6JournalCheckpointV1::from_frontier(journal.frontier().clone()).unwrap();
        journal.append(&cancel).unwrap();
        let recovered = restart_from_test_authority(journal, checkpoint).unwrap();
        let request = recovered
            .replay()
            .request(fixture.identity.strategy_request_id())
            .unwrap();
        assert_eq!(
            request.cancel_outcome(),
            Some(Stage6CancelOutcomeV1::ExecutionObserved)
        );
        assert_eq!(
            request.dispatch_safety_state(),
            crate::Stage6DispatchSafetyStateV1::DispatchForbidden
        );
    }

    #[test]
    fn stage6d_cancel_rejects_generic_place_outcome() {
        let fixture = cancel_fixture(7, "TARGET-7");
        let (accepted, dispatch) = accepted_and_dispatch_cancel(&fixture);
        let mut recovered = recovered();
        let receipt = prepare_stage6d_paper_dispatch(&mut recovered, accepted, dispatch).unwrap();
        assert!(matches!(
            execute_stage6d_paper_outcome(
                &mut recovered,
                receipt,
                Stage6dPaperOutcome::PlaceNoBrokerOrderFound,
            ),
            Err(Stage6dLiveCoreError::PaperOutcomeActionMismatch)
        ));
        assert_eq!(recovered.journal_frontier().frame_count(), 2);
    }

    #[test]
    fn stage6d_dispatch_ordering_rejects_non_dispatch_second_record() {
        let fixture = place_fixture(8, OrderType::Market);
        let (accepted, _dispatch) = accepted_and_dispatch_place(&fixture);
        let mut recovered = recovered();
        assert!(matches!(
            prepare_stage6d_paper_dispatch(&mut recovered, accepted.clone(), accepted),
            Err(Stage6dLiveCoreError::DispatchAttemptRecordRequired)
        ));
        assert_eq!(recovered.journal_frontier().frame_count(), 0);
    }

    #[test]
    fn stage6d_d1_restart_after_accepted_is_ready_for_first_dispatch() {
        let fixture = place_fixture(11, OrderType::Market);
        let (accepted, _) = accepted_and_dispatch_place(&fixture);
        let mut journal = Stage6MemoryJournalBackend::new();
        journal.append(&accepted).unwrap();
        let checkpoint =
            Stage6JournalCheckpointV1::from_frontier(journal.frontier().clone()).unwrap();
        let recovered = restart_from_test_authority(journal, checkpoint).unwrap();
        assert_eq!(
            recovered
                .replay()
                .request(fixture.identity.strategy_request_id())
                .unwrap()
                .dispatch_safety_state(),
            crate::Stage6DispatchSafetyStateV1::ReadyForFirstDispatch
        );
    }

    #[test]
    fn stage6d_d2_restart_after_dispatch_requires_reconciliation() {
        let fixture = place_fixture(12, OrderType::Market);
        let (accepted, dispatch) = accepted_and_dispatch_place(&fixture);
        let mut journal = Stage6MemoryJournalBackend::new();
        journal.append(&accepted).unwrap();
        journal.append(&dispatch).unwrap();
        let checkpoint =
            Stage6JournalCheckpointV1::from_frontier(journal.frontier().clone()).unwrap();
        let recovered = restart_from_test_authority(journal, checkpoint).unwrap();
        assert_eq!(
            recovered
                .replay()
                .request(fixture.identity.strategy_request_id())
                .unwrap()
                .dispatch_safety_state(),
            crate::Stage6DispatchSafetyStateV1::ReconciliationRequired
        );
    }

    #[test]
    fn stage6d_d5_trade_in_valid_suffix_is_preserved_once() {
        let fixture = place_fixture(13, OrderType::Market);
        let (accepted, dispatch) = accepted_and_dispatch_place(&fixture);
        let mut journal = Stage6MemoryJournalBackend::new();
        journal.append(&accepted).unwrap();
        journal.append(&dispatch).unwrap();
        let checkpoint =
            Stage6JournalCheckpointV1::from_frontier(journal.frontier().clone()).unwrap();
        let order = Stage6JournalRecordV1::broker_order_observed(
            fixture.identity.clone(),
            BrokerOrderId::new("SUFFIX-ORDER-13"),
            Stage6LifecycleSequence::new(3).unwrap(),
            Some(dispatch.journal_record_id().clone()),
            digest('5'),
        )
        .unwrap();
        let trade = Stage6JournalRecordV1::broker_trade_observed(
            fixture.identity.clone(),
            BrokerTradeId::new("SUFFIX-TRADE-13"),
            BrokerOrderId::new("SUFFIX-ORDER-13"),
            Stage6LifecycleSequence::new(4).unwrap(),
            Some(order.journal_record_id().clone()),
            digest('6'),
        )
        .unwrap();
        journal.append(&order).unwrap();
        journal.append(&trade).unwrap();
        let recovered = restart_from_test_authority(journal, checkpoint).unwrap();
        let request = recovered
            .replay()
            .request(fixture.identity.strategy_request_id())
            .unwrap();
        assert_eq!(request.observed_broker_trade_ids().len(), 1);
        assert_eq!(
            request.observed_broker_trade_ids()[0],
            BrokerTradeId::new("SUFFIX-TRADE-13")
        );
    }

    #[test]
    fn stage6d_d8_checkpoint_ahead_of_journal_fails_closed() {
        let fixture = place_fixture(14, OrderType::Market);
        let (accepted, dispatch) = accepted_and_dispatch_place(&fixture);
        let mut full = Stage6MemoryJournalBackend::new();
        full.append(&accepted).unwrap();
        full.append(&dispatch).unwrap();
        let checkpoint = Stage6JournalCheckpointV1::from_frontier(full.frontier().clone()).unwrap();
        let mut shorter = Stage6MemoryJournalBackend::new();
        shorter.append(&accepted).unwrap();
        assert!(matches!(
            restart_from_test_authority(shorter, checkpoint),
            Err(Stage6dLiveCoreError::Journal(
                Stage6JournalStorageError::CheckpointInvalid
            ))
        ));
    }

    #[test]
    fn stage6d_same_length_checkpoint_hash_mismatch_fails_closed() {
        let fixture_a = place_fixture(15, OrderType::Market);
        let fixture_b = place_fixture(16, OrderType::Market);
        let (accepted_a, _) = accepted_and_dispatch_place(&fixture_a);
        let (accepted_b, _) = accepted_and_dispatch_place(&fixture_b);
        let mut a = Stage6MemoryJournalBackend::new();
        a.append(&accepted_a).unwrap();
        let checkpoint = Stage6JournalCheckpointV1::from_frontier(a.frontier().clone()).unwrap();
        let mut b = Stage6MemoryJournalBackend::new();
        b.append(&accepted_b).unwrap();
        assert!(matches!(
            restart_from_test_authority(b, checkpoint),
            Err(Stage6dLiveCoreError::Journal(
                Stage6JournalStorageError::CheckpointInvalid
            ))
        ));
    }

    #[test]
    fn stage6d_d9_longer_valid_suffix_is_accepted_deterministically() {
        let fixture = place_fixture(17, OrderType::Market);
        let (accepted, dispatch) = accepted_and_dispatch_place(&fixture);
        let mut journal = Stage6MemoryJournalBackend::new();
        journal.append(&accepted).unwrap();
        let checkpoint =
            Stage6JournalCheckpointV1::from_frontier(journal.frontier().clone()).unwrap();
        journal.append(&dispatch).unwrap();
        let bytes = journal.framed_bytes().unwrap();
        let a = restart_from_test_authority(
            Stage6MemoryJournalBackend::from_framed_bytes(bytes.clone()).unwrap(),
            checkpoint.clone(),
        )
        .unwrap();
        let b = restart_from_test_authority(
            Stage6MemoryJournalBackend::from_framed_bytes(bytes).unwrap(),
            checkpoint,
        )
        .unwrap();
        assert_eq!(
            a.replay().semantic_fingerprint_sha256(),
            b.replay().semantic_fingerprint_sha256()
        );
        assert_ne!(
            a.integration_fingerprint_sha256(),
            b.integration_fingerprint_sha256()
        );
        assert_ne!(
            a.current_process_generation_id(),
            b.current_process_generation_id()
        );
        assert_eq!(a.journal_frontier().frame_count(), 2);
    }

    #[test]
    fn stage6e_matching_stage5_stage6_pair_is_cross_bound_before_capability() {
        let recovered = stage6e_working_cross_binding_recovery(
            Stage6eWorkingBindingMutation::None,
            Stage6eExtraStage6History::None,
        )
        .unwrap();
        assert_eq!(recovered.active_cross_bound_request_ids().len(), 1);
        assert_eq!(
            recovered
                .semantic_cross_binding_fingerprint_sha256()
                .unwrap()
                .as_str()
                .len(),
            64
        );
    }

    #[test]
    fn stage6e_account_mismatch_is_rejected_during_restart() {
        assert!(matches!(
            stage6e_working_cross_binding_recovery(
                Stage6eWorkingBindingMutation::Account,
                Stage6eExtraStage6History::None,
            ),
            Err(Stage6dLiveCoreError::RestartSemanticCrossBindingMismatch)
        ));
    }

    #[test]
    fn stage6e_instrument_mismatch_is_rejected_during_restart() {
        assert!(matches!(
            stage6e_working_cross_binding_recovery(
                Stage6eWorkingBindingMutation::Instrument,
                Stage6eExtraStage6History::None,
            ),
            Err(Stage6dLiveCoreError::RestartSemanticCrossBindingMismatch)
        ));
    }

    #[test]
    fn stage6e_attribution_mismatch_is_rejected_during_restart() {
        assert!(matches!(
            stage6e_working_cross_binding_recovery(
                Stage6eWorkingBindingMutation::Attribution,
                Stage6eExtraStage6History::None,
            ),
            Err(Stage6dLiveCoreError::RestartSemanticCrossBindingMismatch)
        ));
    }

    #[test]
    fn stage6e_place_cancel_action_mismatch_is_rejected_during_restart() {
        assert!(matches!(
            stage6e_working_cross_binding_recovery(
                Stage6eWorkingBindingMutation::Action,
                Stage6eExtraStage6History::None,
            ),
            Err(Stage6dLiveCoreError::RestartSemanticCrossBindingMismatch)
        ));
    }

    #[test]
    fn stage6e_exact_cancel_target_is_cross_bound() {
        let recovered = stage6e_cancel_cross_binding_recovery(false).unwrap();
        assert_eq!(recovered.active_cross_bound_request_ids().len(), 1);
    }

    #[test]
    fn stage6e_r1_two_active_place_requests_are_cross_bound() {
        let (recovered, request_ids, _, _) = stage6e_two_place_cross_binding_recovery();
        assert_eq!(request_ids.len(), 2);
        assert_eq!(recovered.active_cross_bound_request_ids().len(), 2);
        assert!(request_ids.iter().all(|request_id| recovered
            .active_cross_bound_request_ids()
            .contains(request_id)));
    }

    #[test]
    fn stage6e_r1_request_scoped_issuer_selects_each_of_two_current_requests() {
        let (recovered, request_ids, input, _) = stage6e_two_place_cross_binding_recovery();
        let validation_observed_at = input.captured_at;
        for request_id in request_ids {
            let accepted = issue_stage6e_paper_fresh_broker_truth_for_request_at(
                &recovered,
                request_id,
                input.clone(),
                validation_observed_at,
            )
            .expect("each current request has independent issuance authority");
            assert_eq!(accepted.strategy_request_id, request_id);
        }
    }

    #[test]
    fn stage6e_r1_finalized_only_request_cannot_be_selected() {
        let recovered = stage6e_working_cross_binding_recovery(
            Stage6eWorkingBindingMutation::None,
            Stage6eExtraStage6History::Finalized,
        )
        .unwrap();
        let (_, input, _) = stage6d_stage5g_working_restart_fixture();
        let finalized = recovered
            .replay()
            .requests()
            .iter()
            .find(|request| {
                !recovered
                    .active_cross_bound_request_ids()
                    .contains(&request.strategy_request_id())
            })
            .expect("fixture has finalized historical request")
            .strategy_request_id();
        assert!(matches!(
            issue_stage6e_paper_fresh_broker_truth_for_request_at(
                &recovered,
                finalized,
                input.clone(),
                input.captured_at,
            ),
            Err(Stage6dLiveCoreError::FreshTruthRequestNotCrossBound)
        ));
    }

    #[test]
    fn stage6e_r1_current_request_with_finalized_history_can_be_issued() {
        let recovered = stage6e_working_cross_binding_recovery(
            Stage6eWorkingBindingMutation::None,
            Stage6eExtraStage6History::Finalized,
        )
        .unwrap();
        let (_, input, _) = stage6d_stage5g_working_restart_fixture();
        let request_id = recovered.active_cross_bound_request_ids()[0];
        issue_stage6e_paper_fresh_broker_truth_for_request_at(
            &recovered,
            request_id,
            input.clone(),
            input.captured_at,
        )
        .expect("finalized history cannot make current request selection ambiguous");
    }

    #[test]
    fn stage6e_r1_selected_request_apply_is_deterministic_with_two_current_slots() {
        let (recovered, request_ids, input, key) = stage6e_two_place_cross_binding_recovery();
        let selected = request_ids[1];
        let accepted = issue_stage6e_paper_fresh_broker_truth_for_request_at(
            &recovered,
            selected,
            input.clone(),
            input.captured_at,
        )
        .unwrap();
        let transition = apply_stage6e_accepted_fresh_truth(recovered, accepted, &key).unwrap();
        assert_eq!(transition.report().strategy_request_id, selected);
        assert_eq!(
            transition
                .recovered()
                .active_cross_bound_request_ids()
                .len(),
            2
        );
    }

    #[test]
    fn stage6e_r1_mixed_current_place_cancel_exact_target_is_cross_bound() {
        let (recovered, request_ids) =
            stage6e_mixed_place_cancel_cross_binding_recovery(false).unwrap();
        assert_eq!(request_ids.len(), 2);
        assert_eq!(recovered.active_cross_bound_request_ids().len(), 2);
        assert!(request_ids.iter().all(|request_id| recovered
            .active_cross_bound_request_ids()
            .contains(request_id)));
    }

    #[test]
    fn stage6e_r1_mixed_current_place_cancel_target_mismatch_fails_closed() {
        assert!(matches!(
            stage6e_mixed_place_cancel_cross_binding_recovery(true),
            Err(Stage6dLiveCoreError::RestartSemanticCrossBindingMismatch)
        ));
    }

    #[test]
    fn stage6e_r1_valid_package_is_strictly_after_current_restore() {
        let (recovered, input, _) = stage6d_stage5g_working_restart_fixture();
        let request_id = recovered.active_cross_bound_request_ids()[0];
        let (input, validation_observed_at) =
            retime_stage6e_input_after_current_restore(&recovered, input);
        issue_stage6e_paper_fresh_broker_truth_for_request_at(
            &recovered,
            request_id,
            input,
            validation_observed_at,
        )
        .expect("all local collection sections are post-restore and pre-validation");
    }

    #[test]
    fn stage6e_r1_public_issuer_uses_host_validation_clock() {
        let (recovered, mut input, _) = stage6d_stage5g_working_restart_fixture();
        std::thread::sleep(std::time::Duration::from_millis(1));
        let observed_at = Utc::now();
        input.collection_started_at = observed_at;
        input.captured_at = observed_at;
        input.orders_observed_at = observed_at;
        input.trades_observed_at = observed_at;
        input.positions_observed_at = observed_at;
        for order in &mut input.orders {
            order.received_ts = observed_at;
            order.source_ts = Some(observed_at);
        }
        let request_id = recovered.active_cross_bound_request_ids()[0];
        issue_stage6e_paper_fresh_broker_truth_for_request(&recovered, request_id, input)
            .expect("production issuer observes validation time from the host boundary");
    }

    #[test]
    fn stage6e_r1_package_before_current_restore_is_rejected() {
        let (recovered, input, _) = stage6d_stage5g_working_restart_fixture();
        let request_id = recovered.active_cross_bound_request_ids()[0];
        let restore = recovered.current_restore_completed_at().unwrap();
        let (mut input, validation_observed_at) =
            retime_stage6e_input_after_current_restore(&recovered, input);
        input.captured_at = restore - chrono::Duration::nanoseconds(1);
        assert!(matches!(
            issue_stage6e_paper_fresh_broker_truth_for_request_at(
                &recovered,
                request_id,
                input,
                validation_observed_at,
            ),
            Err(Stage6dLiveCoreError::FreshTruthTemporalAuthorityMismatch)
        ));
    }

    #[test]
    fn stage6e_r1_package_equal_to_current_restore_is_rejected() {
        let (recovered, input, _) = stage6d_stage5g_working_restart_fixture();
        let request_id = recovered.active_cross_bound_request_ids()[0];
        let restore = recovered.current_restore_completed_at().unwrap();
        let (mut input, validation_observed_at) =
            retime_stage6e_input_after_current_restore(&recovered, input);
        input.collection_started_at = restore;
        input.captured_at = restore;
        assert!(matches!(
            issue_stage6e_paper_fresh_broker_truth_for_request_at(
                &recovered,
                request_id,
                input,
                validation_observed_at,
            ),
            Err(Stage6dLiveCoreError::FreshTruthTemporalAuthorityMismatch)
        ));
    }

    #[test]
    fn stage6e_r1_orders_section_before_current_restore_is_rejected() {
        let (recovered, input, _) = stage6d_stage5g_working_restart_fixture();
        let request_id = recovered.active_cross_bound_request_ids()[0];
        let restore = recovered.current_restore_completed_at().unwrap();
        let (mut input, validation_observed_at) =
            retime_stage6e_input_after_current_restore(&recovered, input);
        input.orders_observed_at = restore;
        assert!(matches!(
            issue_stage6e_paper_fresh_broker_truth_for_request_at(
                &recovered,
                request_id,
                input,
                validation_observed_at,
            ),
            Err(Stage6dLiveCoreError::FreshTruthTemporalAuthorityMismatch)
        ));
    }

    #[test]
    fn stage6e_r1_trades_section_before_current_restore_is_rejected() {
        let (recovered, input, _) = stage6d_stage5g_terminal_restart_fixture();
        let request_id = recovered.active_cross_bound_request_ids()[0];
        let restore = recovered.current_restore_completed_at().unwrap();
        let (mut input, validation_observed_at) =
            retime_stage6e_input_after_current_restore(&recovered, input);
        input.trades_observed_at = restore;
        assert!(matches!(
            issue_stage6e_paper_fresh_broker_truth_for_request_at(
                &recovered,
                request_id,
                input,
                validation_observed_at,
            ),
            Err(Stage6dLiveCoreError::FreshTruthTemporalAuthorityMismatch)
        ));
    }

    #[test]
    fn stage6e_r1_positions_section_before_current_restore_is_rejected() {
        let (recovered, input, _) = stage6d_stage5g_terminal_restart_fixture();
        let request_id = recovered.active_cross_bound_request_ids()[0];
        let restore = recovered.current_restore_completed_at().unwrap();
        let (mut input, validation_observed_at) =
            retime_stage6e_input_after_current_restore(&recovered, input);
        input.positions_observed_at = restore;
        assert!(matches!(
            issue_stage6e_paper_fresh_broker_truth_for_request_at(
                &recovered,
                request_id,
                input,
                validation_observed_at,
            ),
            Err(Stage6dLiveCoreError::FreshTruthTemporalAuthorityMismatch)
        ));
    }

    #[test]
    fn stage6e_r1_mixed_stale_section_is_rejected() {
        let (recovered, input, _) = stage6d_stage5g_terminal_restart_fixture();
        let request_id = recovered.active_cross_bound_request_ids()[0];
        let restore = recovered.current_restore_completed_at().unwrap();
        let (mut input, validation_observed_at) =
            retime_stage6e_input_after_current_restore(&recovered, input);
        input.positions_observed_at = restore - chrono::Duration::nanoseconds(1);
        assert!(matches!(
            issue_stage6e_paper_fresh_broker_truth_for_request_at(
                &recovered,
                request_id,
                input,
                validation_observed_at,
            ),
            Err(Stage6dLiveCoreError::FreshTruthTemporalAuthorityMismatch)
        ));
    }

    #[test]
    fn stage6e_r1_future_package_beyond_trusted_validation_is_rejected() {
        let (recovered, input, _) = stage6d_stage5g_working_restart_fixture();
        let request_id = recovered.active_cross_bound_request_ids()[0];
        let (mut input, validation_observed_at) =
            retime_stage6e_input_after_current_restore(&recovered, input);
        input.captured_at = validation_observed_at + chrono::Duration::nanoseconds(1);
        assert!(matches!(
            issue_stage6e_paper_fresh_broker_truth_for_request_at(
                &recovered,
                request_id,
                input,
                validation_observed_at,
            ),
            Err(Stage6dLiveCoreError::FreshTruthTemporalAuthorityMismatch)
        ));
    }

    #[test]
    fn stage6e_r1_row_received_in_trusted_future_is_rejected() {
        let (recovered, input, _) = stage6d_stage5g_working_restart_fixture();
        let request_id = recovered.active_cross_bound_request_ids()[0];
        let (mut input, validation_observed_at) =
            retime_stage6e_input_after_current_restore(&recovered, input);
        input.orders[0].received_ts = validation_observed_at + chrono::Duration::nanoseconds(1);
        assert!(matches!(
            issue_stage6e_paper_fresh_broker_truth_for_request_at(
                &recovered,
                request_id,
                input,
                validation_observed_at,
            ),
            Err(Stage6dLiveCoreError::FreshTruthTemporalAuthorityMismatch)
        ));
    }

    #[test]
    fn stage6e_r1_prior_process_capability_is_rejected_after_new_restart() {
        let (first, first_input, _) = stage6d_stage5g_working_restart_fixture();
        let first_request = first.active_cross_bound_request_ids()[0];
        let (first_input, first_validation) =
            retime_stage6e_input_after_current_restore(&first, first_input);
        let accepted = issue_stage6e_paper_fresh_broker_truth_for_request_at(
            &first,
            first_request,
            first_input,
            first_validation,
        )
        .unwrap();
        let (second, _, key) = stage6d_stage5g_working_restart_fixture();
        assert_ne!(
            first.current_process_generation_id(),
            second.current_process_generation_id()
        );
        assert!(matches!(
            apply_stage6e_accepted_fresh_truth(second, accepted, &key),
            Err(Stage6dLiveCoreError::AcceptedFreshTruthBindingMismatch)
        ));
    }

    #[test]
    fn stage6e_cancel_target_mismatch_is_rejected_during_restart() {
        assert!(matches!(
            stage6e_cancel_cross_binding_recovery(true),
            Err(Stage6dLiveCoreError::RestartSemanticCrossBindingMismatch)
        ));
    }

    #[test]
    fn stage6e_extra_finalized_stage6_history_does_not_need_current_stage5_slot() {
        let recovered = stage6e_working_cross_binding_recovery(
            Stage6eWorkingBindingMutation::None,
            Stage6eExtraStage6History::Finalized,
        )
        .unwrap();
        assert_eq!(recovered.replay().requests().len(), 2);
        assert_eq!(recovered.active_cross_bound_request_ids().len(), 1);
        assert_eq!(
            recovered
                .replay()
                .requests()
                .iter()
                .filter(|request| request.final_disposition().is_some())
                .count(),
            1
        );
    }

    #[test]
    fn stage6e_extra_unresolved_stage6_authority_is_rejected() {
        assert!(matches!(
            stage6e_working_cross_binding_recovery(
                Stage6eWorkingBindingMutation::None,
                Stage6eExtraStage6History::Unresolved,
            ),
            Err(Stage6dLiveCoreError::RestartSemanticCrossBindingMismatch)
        ));
    }

    #[test]
    fn stage6e_cross_binding_is_deterministic_but_process_epoch_is_unique() {
        let a = stage6e_working_cross_binding_recovery(
            Stage6eWorkingBindingMutation::None,
            Stage6eExtraStage6History::None,
        )
        .unwrap();
        let b = stage6e_working_cross_binding_recovery(
            Stage6eWorkingBindingMutation::None,
            Stage6eExtraStage6History::None,
        )
        .unwrap();
        assert_eq!(
            a.semantic_cross_binding_fingerprint_sha256(),
            b.semantic_cross_binding_fingerprint_sha256()
        );
        assert_ne!(
            a.integration_fingerprint_sha256(),
            b.integration_fingerprint_sha256()
        );
        assert_ne!(
            a.current_process_generation_id(),
            b.current_process_generation_id()
        );
        assert!(a.current_restore_completed_at().is_some());
        assert!(b.current_restore_completed_at().is_some());
    }

    #[test]
    fn stage6d_restart_truth_uses_accepted_stage5g_application_boundary_once() {
        let (recovered, input, key) = stage6d_stage5g_working_restart_fixture();
        let before = recovered.integration_fingerprint_sha256().clone();
        let accepted = issue_first_stage6e_fixture(&recovered, input).unwrap();
        let transition = apply_stage6e_accepted_fresh_truth(recovered, accepted, &key).unwrap();
        let report = transition.report();
        assert!(
            matches!(transition, Stage6dFreshTruthTransition::Applied { .. }),
            "unexpected transition: {} / {} / {}",
            report.scenario_id,
            report.disposition,
            report.reason
        );
        assert!(report.runtime_transition_applied);
        assert!(!report.already_represented_noop);
        assert_ne!(report.stage5_pre_fingerprint_sha256, "");
        assert_ne!(report.stage5_post_fingerprint_sha256, "");
        assert_ne!(
            transition.recovered().integration_fingerprint_sha256(),
            &before
        );
        assert_eq!(
            transition.recovered().journal_frontier().frame_count(),
            2,
            "Stage 5G application must not synthesize a second durable dispatch"
        );
    }

    #[test]
    fn stage6e_paper_issuer_rejects_known_broker_order_absent_from_fresh_truth() {
        let (mut recovered, input, _) = stage6d_stage5g_working_restart_fixture();
        let identity = recovered
            .journal
            .records()
            .iter()
            .find(|record| record.event_kind() == Stage6JournalEventKind::RequestAccepted)
            .unwrap()
            .durable_request_identity()
            .clone();
        let previous = recovered
            .replay()
            .request(identity.strategy_request_id())
            .unwrap()
            .last_unique_record_id()
            .clone();
        let observed = Stage6JournalRecordV1::broker_order_observed(
            identity,
            BrokerOrderId::new("BROKER-ORDER-NOT-IN-TRUTH"),
            Stage6LifecycleSequence::new(3).unwrap(),
            Some(previous),
            digest('c'),
        )
        .unwrap();
        recovered.journal_mut().append(&observed).unwrap();
        recovered.refresh_after_append().unwrap();
        assert!(matches!(
            issue_first_stage6e_fixture(&recovered, input),
            Err(Stage6dLiveCoreError::RestartBrokerTruthMismatch)
        ));
    }

    #[test]
    fn stage6e_paper_issuer_rejects_known_broker_trade_absent_from_fresh_truth() {
        let (mut recovered, input, _) = stage6d_stage5g_working_restart_fixture();
        let identity = recovered
            .journal
            .records()
            .iter()
            .find(|record| record.event_kind() == Stage6JournalEventKind::RequestAccepted)
            .unwrap()
            .durable_request_identity()
            .clone();
        let broker_order_id = input.orders[0].broker_order_id.clone().unwrap();
        let previous = recovered
            .replay()
            .request(identity.strategy_request_id())
            .unwrap()
            .last_unique_record_id()
            .clone();
        let order = Stage6JournalRecordV1::broker_order_observed(
            identity.clone(),
            broker_order_id.clone(),
            Stage6LifecycleSequence::new(3).unwrap(),
            Some(previous),
            digest('c'),
        )
        .unwrap();
        let trade = Stage6JournalRecordV1::broker_trade_observed(
            identity,
            BrokerTradeId::new("BROKER-TRADE-NOT-IN-TRUTH"),
            broker_order_id,
            Stage6LifecycleSequence::new(4).unwrap(),
            Some(order.journal_record_id().clone()),
            digest('a'),
        )
        .unwrap();
        recovered.journal_mut().append(&order).unwrap();
        recovered.journal_mut().append(&trade).unwrap();
        recovered.refresh_after_append().unwrap();
        assert!(matches!(
            issue_first_stage6e_fixture(&recovered, input),
            Err(Stage6dLiveCoreError::RestartBrokerTruthMismatch)
        ));
    }

    #[test]
    fn stage6e_accepted_truth_is_bound_to_exact_replay_and_frontier() {
        let (mut recovered, input, key) = stage6d_stage5g_working_restart_fixture();
        let broker_order_id = input.orders[0].broker_order_id.clone().unwrap();
        let accepted_truth = issue_first_stage6e_fixture(&recovered, input).unwrap();
        let identity = recovered
            .journal
            .records()
            .iter()
            .find(|record| record.event_kind() == Stage6JournalEventKind::RequestAccepted)
            .unwrap()
            .durable_request_identity()
            .clone();
        let previous = recovered
            .replay()
            .request(identity.strategy_request_id())
            .unwrap()
            .last_unique_record_id()
            .clone();
        let order = Stage6JournalRecordV1::broker_order_observed(
            identity,
            broker_order_id,
            Stage6LifecycleSequence::new(3).unwrap(),
            Some(previous),
            digest('c'),
        )
        .unwrap();
        recovered.journal_mut().append(&order).unwrap();
        recovered.refresh_after_append().unwrap();
        assert!(matches!(
            apply_stage6e_accepted_fresh_truth(recovered, accepted_truth, &key),
            Err(Stage6dLiveCoreError::AcceptedFreshTruthBindingMismatch)
        ));
    }

    #[test]
    fn stage6e_restart_rejects_stage6_request_identity_drift_before_capability() {
        let restart = crate::stage5g_order_position::tests::
            stage5g_edb_restored_generated_working_escrow_fixture();
        let fixture = place_fixture(99, OrderType::Market);
        let (accepted, dispatch) = accepted_and_dispatch_place(&fixture);
        let mut journal = Stage6MemoryJournalBackend::new();
        journal.append(&accepted).unwrap();
        journal.append(&dispatch).unwrap();
        let checkpoint =
            Stage6JournalCheckpointV1::from_frontier(journal.frontier().clone()).unwrap();
        let result = recover_stage6d_restart_from_authorities(
            Stage6dStage5RuntimeAuthority::Restart(Box::new(restart)),
            journal,
            checkpoint,
            Some(operational_config()),
        );
        assert!(matches!(
            result,
            Err(Stage6dLiveCoreError::RestartSemanticCrossBindingMismatch)
        ));
    }

    #[test]
    fn stage6d_already_applied_terminal_truth_is_noop_through_stage5g() {
        let (recovered, input, key) = stage6d_stage5g_terminal_restart_fixture();
        let before = recovered.integration_fingerprint_sha256().clone();
        let accepted = issue_first_stage6e_fixture(&recovered, input).unwrap();
        let transition = apply_stage6e_accepted_fresh_truth(recovered, accepted, &key).unwrap();
        assert!(
            matches!(
                transition,
                Stage6dFreshTruthTransition::AlreadyRepresentedNoop { .. }
            ),
            "unexpected transition: {} / {} / {}",
            transition.report().scenario_id,
            transition.report().disposition,
            transition.report().reason
        );
        assert!(!transition.report().runtime_transition_applied);
        assert!(transition.report().already_represented_noop);
        assert_eq!(
            transition.recovered().integration_fingerprint_sha256(),
            &before
        );
    }

    #[test]
    fn stage7a_admission_deduplicates_exact_command_without_second_effect() {
        let mut recovered = recovered();
        let fixture = place_fixture(701, OrderType::Limit);
        let command = BrokerCommand::PlaceOrder(fixture.command.clone());
        let context = Stage7aPaperCommandContext::new(instrument(), attribution("ENTRY"));
        let observed_at = Utc.with_ymd_and_hms(2026, 8, 11, 9, 0, 1).unwrap();

        let receipt =
            match admit_stage7a_paper_command(&mut recovered, &command, &context, observed_at)
                .unwrap()
            {
                Stage7aPaperAdmission::DispatchReady(receipt) => *receipt,
                _ => panic!("first exact command must enter the Stage 6 effect boundary"),
            };
        assert_eq!(recovered.journal.records().len(), 2);
        assert!(matches!(
            admit_stage7a_paper_command(&mut recovered, &command, &context, observed_at).unwrap(),
            Stage7aPaperAdmission::Hold {
                reason: Stage7aPaperHoldReason::ReconciliationRequired,
                ..
            }
        ));
        assert_eq!(recovered.journal.records().len(), 2);

        execute_stage6d_paper_outcome(
            &mut recovered,
            receipt,
            Stage6dPaperOutcome::LimitPending {
                broker_order_id: BrokerOrderId::new("PAPER-IMOEXF-701"),
            },
        )
        .unwrap();
        let records_after_effect = recovered.journal.records().len();
        assert!(matches!(
            admit_stage7a_paper_command(&mut recovered, &command, &context, observed_at).unwrap(),
            Stage7aPaperAdmission::Duplicate(Stage7aPaperAdmissionDecision {
                broker_order_id: Some(_),
                ..
            })
        ));
        assert_eq!(recovered.journal.records().len(), records_after_effect);
    }

    #[test]
    fn stage7a_conflicting_duplicate_is_held_without_mutation() {
        let mut recovered = recovered();
        let fixture = place_fixture(702, OrderType::Market);
        let command = BrokerCommand::PlaceOrder(fixture.command.clone());
        let context = Stage7aPaperCommandContext::new(instrument(), attribution("ENTRY"));
        let observed_at = Utc.with_ymd_and_hms(2026, 8, 11, 9, 0, 1).unwrap();
        let _receipt =
            match admit_stage7a_paper_command(&mut recovered, &command, &context, observed_at)
                .unwrap()
            {
                Stage7aPaperAdmission::DispatchReady(receipt) => *receipt,
                _ => panic!("first command must dispatch"),
            };
        let before = recovered.journal.records().len();
        let mut conflict = fixture.command;
        conflict.account_id = BrokerAccountId::new("ACC_CONFLICT_0001");
        assert!(matches!(
            admit_stage7a_paper_command(
                &mut recovered,
                &BrokerCommand::PlaceOrder(conflict),
                &context,
                observed_at,
            )
            .unwrap(),
            Stage7aPaperAdmission::Hold {
                reason: Stage7aPaperHoldReason::ConflictingDuplicate,
                ..
            }
        ));
        assert_eq!(recovered.journal.records().len(), before);
    }

    #[test]
    fn stage7a_expired_command_and_second_unresolved_are_fail_closed() {
        let mut recovered = recovered();
        let first = place_fixture(703, OrderType::Market);
        let context = Stage7aPaperCommandContext::new(instrument(), attribution("ENTRY"));
        let expired_at = Utc.with_ymd_and_hms(2026, 8, 11, 9, 0, 6).unwrap();
        assert!(matches!(
            admit_stage7a_paper_command(
                &mut recovered,
                &BrokerCommand::PlaceOrder(first.command.clone()),
                &context,
                expired_at,
            )
            .unwrap(),
            Stage7aPaperAdmission::PolicyRejected {
                reason: Stage7aPaperPolicyRejection::Expired,
                ..
            }
        ));
        assert!(recovered.journal.records().is_empty());

        let live_at = Utc.with_ymd_and_hms(2026, 8, 11, 9, 0, 1).unwrap();
        let _receipt = match admit_stage7a_paper_command(
            &mut recovered,
            &BrokerCommand::PlaceOrder(first.command),
            &context,
            live_at,
        )
        .unwrap()
        {
            Stage7aPaperAdmission::DispatchReady(receipt) => *receipt,
            _ => panic!("first live command must dispatch"),
        };
        let second = place_fixture(704, OrderType::Market);
        assert!(matches!(
            admit_stage7a_paper_command(
                &mut recovered,
                &BrokerCommand::PlaceOrder(second.command),
                &context,
                live_at,
            )
            .unwrap(),
            Stage7aPaperAdmission::Hold {
                reason: Stage7aPaperHoldReason::AnotherLifecycleUnresolved,
                ..
            }
        ));
        assert_eq!(recovered.journal.records().len(), 2);
    }

    #[test]
    fn stage7a_resumes_only_the_dispatch_after_accepted_crash_window() {
        let mut recovered = recovered();
        let fixture = place_fixture(705, OrderType::Market);
        let snapshot =
            Stage6DurableCommandSnapshotV1::from_place(&fixture.identity, &fixture.command)
                .unwrap();
        let accepted = Stage6JournalRecordV1::request_accepted(
            fixture.identity,
            snapshot,
            Stage6LifecycleSequence::new(1).unwrap(),
            None,
            None,
            digest('9'),
        )
        .unwrap();
        recovered.journal_mut().append(&accepted).unwrap();
        recovered.refresh_after_append().unwrap();
        let context = Stage7aPaperCommandContext::new(instrument(), attribution("ENTRY"));
        assert!(matches!(
            admit_stage7a_paper_command(
                &mut recovered,
                &BrokerCommand::PlaceOrder(fixture.command),
                &context,
                Utc.with_ymd_and_hms(2026, 8, 11, 9, 0, 1).unwrap(),
            )
            .unwrap(),
            Stage7aPaperAdmission::DispatchReady(_)
        ));
        assert_eq!(recovered.journal.records().len(), 2);
        assert_eq!(
            recovered.journal.records()[1].event_kind(),
            Stage6JournalEventKind::DispatchAttemptRecorded
        );
    }

    fn assert_stage7a_nonfinal_place_blocks_second_place(
        first_number: u128,
        outcome: Stage6dPaperOutcome,
    ) {
        let mut recovered = recovered();
        let first = place_fixture(first_number, OrderType::Limit);
        let first_command = BrokerCommand::PlaceOrder(first.command);
        let context = Stage7aPaperCommandContext::new(instrument(), attribution("ENTRY"));
        let observed_at = Utc.with_ymd_and_hms(2026, 8, 11, 9, 0, 1).unwrap();
        let receipt = match admit_stage7a_paper_command(
            &mut recovered,
            &first_command,
            &context,
            observed_at,
        )
        .unwrap()
        {
            Stage7aPaperAdmission::DispatchReady(receipt) => *receipt,
            _ => panic!("first command must dispatch"),
        };
        execute_stage6d_paper_outcome(&mut recovered, receipt, outcome).unwrap();
        assert!(recovered
            .replay()
            .request(stage7a_request_id(&first_command))
            .unwrap()
            .final_disposition()
            .is_none());

        let second = place_fixture(first_number + 1, OrderType::Market);
        assert!(matches!(
            admit_stage7a_paper_command(
                &mut recovered,
                &BrokerCommand::PlaceOrder(second.command),
                &context,
                observed_at,
            )
            .unwrap(),
            Stage7aPaperAdmission::Hold {
                reason: Stage7aPaperHoldReason::AnotherLifecycleUnresolved,
                ..
            }
        ));
    }

    #[test]
    fn stage7a_limit_pending_blocks_second_new_place() {
        assert_stage7a_nonfinal_place_blocks_second_place(
            710,
            Stage6dPaperOutcome::LimitPending {
                broker_order_id: BrokerOrderId::new("PAPER-WORKING-710"),
            },
        );
    }

    #[test]
    fn stage7a_nonfinal_place_blocks_source_correlated_cancel() {
        let mut recovered = recovered();
        let place = place_fixture(715, OrderType::Limit);
        let place_client_order_id = place.command.client_order_id.clone();
        let place_command = BrokerCommand::PlaceOrder(place.command);
        let observed_at = Utc.with_ymd_and_hms(2026, 8, 11, 9, 0, 1).unwrap();
        let receipt = match admit_stage7a_paper_command(
            &mut recovered,
            &place_command,
            &Stage7aPaperCommandContext::new(instrument(), attribution("ENTRY")),
            observed_at,
        )
        .unwrap()
        {
            Stage7aPaperAdmission::DispatchReady(receipt) => *receipt,
            _ => panic!("first PLACE must dispatch"),
        };
        let target = BrokerOrderId::new("PAPER-WORKING-715");
        execute_stage6d_paper_outcome(
            &mut recovered,
            receipt,
            Stage6dPaperOutcome::LimitPending {
                broker_order_id: target.clone(),
            },
        )
        .unwrap();
        let mut cancel = cancel_fixture(716, target.as_str());
        cancel.command.client_order_id = Some(place_client_order_id);
        assert!(matches!(
            admit_stage7a_paper_command(
                &mut recovered,
                &BrokerCommand::CancelOrder(cancel.command),
                &Stage7aPaperCommandContext::new(instrument(), attribution("CANCEL")),
                observed_at,
            )
            .unwrap(),
            Stage7aPaperAdmission::Hold {
                reason: Stage7aPaperHoldReason::AnotherLifecycleUnresolved,
                ..
            }
        ));
        assert_eq!(
            recovered
                .replay()
                .requests()
                .iter()
                .filter(|request| request.final_disposition().is_none())
                .count(),
            1
        );
    }

    #[test]
    fn stage7a_market_filled_nonfinal_blocks_second_new_place() {
        assert_stage7a_nonfinal_place_blocks_second_place(
            720,
            Stage6dPaperOutcome::MarketFilled {
                broker_order_id: BrokerOrderId::new("PAPER-FILLED-720"),
                broker_trade_id: BrokerTradeId::new("PAPER-TRADE-720"),
            },
        );
    }

    #[test]
    fn stage7a_broker_order_found_nonfinal_blocks_second_new_place() {
        assert_stage7a_nonfinal_place_blocks_second_place(
            730,
            Stage6dPaperOutcome::PlaceBrokerOrderFound {
                broker_order_id: BrokerOrderId::new("PAPER-FOUND-730"),
            },
        );
    }
}
