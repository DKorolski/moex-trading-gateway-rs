//! Stage 6D live-core restart integration.
//!
//! This module binds the authenticated Stage 5G clean-restart package to the
//! accepted Stage 6B journal checkpoint and then delegates lifecycle recovery
//! to the accepted Stage 6C replay engine.  It deliberately owns no Redis,
//! FINAM, network dispatch, runtime-live or real-order surface.

use crate::runtime_compat::Strategy;
use crate::stage5g_clean_restart::{
    export_stage5g_clean_restart, restore_stage5g_clean_restart, Stage5gCleanRestartError,
    Stage5gCleanRestartedCapability, Stage5gLifecycleCommitmentKey,
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
use crate::stage5g_p1_semantic::{
    continue_stage5g_p1_semantic, continue_stage8b_p1d3_semantic, export_stage5g_p1_one_intent,
    export_stage5g_p1_zero_intent, export_stage8b_p1d3_one_intent, export_stage8b_p1d3_zero_intent,
    p1_projection_from_one, p1_projection_from_zero, p1d3_projection_from_one,
    p1d3_projection_from_zero, rebind_stage8b_p1d3_one_intent_request_checkpoint,
    Stage5gP1SemanticBindingInput, Stage5gP1SemanticCommitProjectionV1,
    Stage5gP1SemanticTransition, Stage8bP1d3SemanticTransition,
};
use crate::stage6_durable_identity::Stage6JournalPayloadV1;
use crate::{
    HybridIntradayRuntimeStrategy, Stage6CancelOutcomeV1, Stage6DurableActionKind,
    Stage6DurableCommandSnapshotV1, Stage6DurableIdentityError, Stage6DurablePlaceOrderShapeV1,
    Stage6DurableRequestIdentityV1, Stage6JournalBackend, Stage6JournalCheckpointV1,
    Stage6JournalEventKind, Stage6JournalFrontierV1, Stage6JournalRecordId, Stage6JournalRecordV1,
    Stage6JournalRecordV2, Stage6JournalRecordV3, Stage6JournalRecordV4,
    Stage6JournalRecordVersioned, Stage6JournalStorageError, Stage6LifecycleSequence,
    Stage6MemoryJournalBackend, Stage6MixedReplayEngineV2, Stage6OwnedJournalBackend,
    Stage6ReconciliationBatchCompletionV2, Stage6ReconciliationDispositionV1,
    Stage6ReconciliationLifecycleV2, Stage6ReconciliationTransitionKindV2,
    Stage6ReconciliationV2Error, Stage6RecoveredRequestV1, Stage6ReplayEngineV1, Stage6ReplayError,
    Stage6ReplaySnapshotV1, Stage6RequestFinalDispositionV1, Stage6Sha256Digest,
};
use broker_core::{
    BrokerAccountId, BrokerCommand, BrokerOrderId, BrokerOrderSnapshot, BrokerPositionSnapshot,
    BrokerTradeId, BrokerTradeSnapshot, ClientOrderId, HybridRuntimeAttribution, InstrumentId,
    StrategyRequestId,
};
use chrono::{DateTime, TimeZone, Utc};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
#[cfg(feature = "stage5g-artifact-fixtures")]
use rust_decimal::prelude::ToPrimitive;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::atomic::{AtomicU64, Ordering};

pub const STAGE6D_AUTHENTICATED_RESTART_SCHEMA_VERSION: u16 = 1;
pub const STAGE6D_INTEGRATION_FINGERPRINT_SCHEMA_VERSION: u16 = 3;
pub const STAGE6E_ACCEPTED_FRESH_TRUTH_SCHEMA_VERSION: u16 = 2;
pub const STAGE8B_P1_REQUEST_ACCEPTED_BINDING_SCHEMA_VERSION: u16 = 1;

const STAGE6D_RESTART_COMMITMENT_DOMAIN: &str = "moex.stage6d.authenticated-restart-frontier.v1";
const STAGE6D_INTEGRATION_FINGERPRINT_DOMAIN: &str = "moex.stage6e-r1.durable-runtime-recovered.v3";
const STAGE6E_SEMANTIC_CROSS_BINDING_DOMAIN: &str =
    "moex.stage6e.stage5-stage6-semantic-cross-binding.v1";
const STAGE6E_RESTORE_EPOCH_DOMAIN: &str = "moex.stage6e-r1.current-process-restore-epoch.v1";
const STAGE8B_P1_REQUEST_ACCEPTED_BINDING_DOMAIN: &str =
    "moex.stage8b.p1.prepublication-request-accepted.v1";

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
    JournalMutationMayHaveOccurred,
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
    Stage8a4WriteAuthorityInvalid,
    Stage8bP1d2MarketFeedback,
    Stage8bP1d3WorkingLimit,
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
            Self::JournalMutationMayHaveOccurred => {
                "Stage 6 journal mutation may have occurred; restart is required"
            }
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
            Self::Stage8a4WriteAuthorityInvalid => {
                "Stage 8A-4 sealed durable-write authority is invalid"
            }
            Self::Stage8bP1d2MarketFeedback => "Stage 8B-P1-d2 market-feedback transition failed",
            Self::Stage8bP1d3WorkingLimit => "Stage 8B-P1-d3 working-limit transition failed",
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

impl From<crate::Stage8bP1d2MarketFeedbackError> for Stage6dLiveCoreError {
    fn from(_value: crate::Stage8bP1d2MarketFeedbackError) -> Self {
        Self::Stage8bP1d2MarketFeedback
    }
}

impl From<crate::Stage8bP1d3Error> for Stage6dLiveCoreError {
    fn from(_value: crate::Stage8bP1d3Error) -> Self {
        Self::Stage8bP1d3WorkingLimit
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
    /// Ed25519 verification key for the private Stage 8A-4 writer issuer.
    /// It is authenticated by the Stage 6D restart package and therefore
    /// cannot be replaced by a caller presenting otherwise valid public V2
    /// material.
    pub stage8a4_writer_issuer_public_key_hex: String,
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

/// Authenticates both the Stage 6 wrapper and embedded Stage 5G package and
/// returns only the P1-d4 routing discriminator. This is intentionally a
/// read-only probe: it owns no journal, Redis, schedule or provider action.
pub fn authenticate_stage8b_p1d4_restart_package_state(
    bytes: &[u8],
    commitment_key: &Stage5gLifecycleCommitmentKey,
    fresh_runtime: crate::HybridIntradayRuntimeStrategy,
) -> Result<Option<crate::Stage8bP1d4GeneratedMarketPackageState>, Stage6dLiveCoreError> {
    let package = decode_and_authenticate_restart_package(bytes, commitment_key)?;
    let restored = crate::restore_stage5g_clean_restart(
        &package.stage5g_restart_package,
        commitment_key,
        fresh_runtime,
    )
    .map_err(|_| Stage6dLiveCoreError::RestartPackageBindingMismatch)?;
    restored
        .stage8b_p1d4_generated_market_package_state()
        .map_err(|_| Stage6dLiveCoreError::RestartPackageBindingMismatch)
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

/// Linear proof that one exact schedule binding V4 was appended and reread by
/// the existing Stage 6 journal backend. It grants no schedule authority until
/// the Stage 7B owner commits and rereads the expected covering seal.
pub struct Stage6Stage8bP1eScheduleBindingPending {
    candidate: crate::Stage8bP1eScheduleBindingCandidateV1,
    record: Stage6JournalRecordV4,
    post_append_checkpoint: Stage6JournalCheckpointV1,
    prior_covering_seal_generation: u64,
    expected_covering_seal_generation: u64,
}

impl Stage6Stage8bP1eScheduleBindingPending {
    pub fn journal_record_id(&self) -> &Stage6JournalRecordId {
        self.record.journal_record_id()
    }

    pub fn lifecycle_sequence(&self) -> Stage6LifecycleSequence {
        self.record.lifecycle_sequence()
    }

    pub fn post_append_checkpoint(&self) -> &Stage6JournalCheckpointV1 {
        &self.post_append_checkpoint
    }

    pub fn prior_covering_seal_generation(&self) -> u64 {
        self.prior_covering_seal_generation
    }

    pub fn expected_covering_seal_generation(&self) -> u64 {
        self.expected_covering_seal_generation
    }
}

/// Read-only, test-fixture-only counters used by the Stage 8B-P1-d4
/// exhaustive crash/replay evidence collector.  This type carries no journal
/// mutation, callback, provider, Redis or dispatch authority.
#[cfg(any(test, feature = "stage5g-artifact-fixtures"))]
#[doc(hidden)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Stage8bP1d4SequenceAllocationAuditV1 {
    pub outcome_kind: String,
    pub journal_record_index: usize,
    pub stage6_lifecycle_sequence: u64,
    pub sequence_allocation_frontier: u64,
    pub seq_ack: Option<u64>,
    pub seq_truth: Option<u64>,
}

/// Authenticated package phase facts used to derive replacement-commit
/// counts.  The generation is read from the restored package authority; it is
/// never supplied by the matrix runner.
#[cfg(any(test, feature = "stage5g-artifact-fixtures"))]
#[doc(hidden)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Stage8bP1d4PackageAuditV1 {
    pub write_generation: Option<u64>,
    pub p1d3_phase: Option<String>,
    pub generated_market_phase: Option<String>,
}

#[cfg(any(test, feature = "stage5g-artifact-fixtures"))]
#[doc(hidden)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Stage8bP1d4RuntimeAuditV1 {
    pub lifecycle_sequence: u64,
    pub journal_lifecycle_sequences: Vec<u64>,
    pub sequence_pair: Option<(u64, u64)>,
    pub sequence_allocations: Vec<Stage8bP1d4SequenceAllocationAuditV1>,
    pub package: Stage8bP1d4PackageAuditV1,
    pub callback_count: usize,
    pub dispatch_v1_total: usize,
    pub order_v1_total: usize,
    pub trade_v1_total: usize,
    pub request_finalized_v1_total: usize,
    pub durable_outcomes: usize,
    pub truth_bearing_outcomes: usize,
}

/// Immutable Stage 7 seal facts captured before a P1 semantic callback.  The
/// service owner constructs this only from its authenticated current S0; the
/// core rechecks every Stage 6/operational field before accepting it.
pub struct Stage6Stage8bP1SealSourceV1 {
    pub seal_generation: u64,
    pub seal_commitment_sha256: String,
    pub stage6_checkpoint_sha256: String,
    pub stage6_frontier_sha256: String,
    pub operational_identity_sha256: String,
}

/// Redacted durable semantic commit facts.  This is evidence only: it owns no
/// runtime, journal, Redis, publication or provider capability.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Stage6Stage8bP1SemanticCommitEvidenceV1 {
    pub schema_version: u16,
    pub semantic_batch_id_sha256: String,
    pub m10_redis_id: String,
    pub m10_semantic_id_sha256: String,
    pub m10_payload_sha256: String,
    pub intent_count: usize,
    pub strategy_request_id: Option<StrategyRequestId>,
    pub canonical_command_sha256: Option<String>,
    pub request_accepted_record_id: Option<String>,
    pub request_accepted_source_evidence_sha256: Option<String>,
}

/// Read-only exact source identity carried by either authenticated P1-d2
/// replacement package. It grants no Redis or acknowledgement authority.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stage6Stage8bP1d2SourceM10Binding {
    redis_id: String,
    semantic_id_sha256: String,
    payload_sha256: String,
}

impl Stage6Stage8bP1d2SourceM10Binding {
    pub fn redis_id(&self) -> &str {
        &self.redis_id
    }

    pub fn semantic_id_sha256(&self) -> &str {
        &self.semantic_id_sha256
    }

    pub fn payload_sha256(&self) -> &str {
        &self.payload_sha256
    }
}

/// Core result consumed by the sole Stage 7 composition owner.  No variant
/// grants command publication or provider authority.
pub enum Stage6Stage8bP1SemanticTransition {
    ZeroIntent {
        recovered: Stage6dDurableRuntimeRecovered,
        stage5g_restart_package: Vec<u8>,
        evidence: Stage6Stage8bP1SemanticCommitEvidenceV1,
    },
    OneIntentPrepublication {
        recovered: Box<Stage6dDurableRuntimeRecovered>,
        stage5g_restart_package: Vec<u8>,
        evidence: Stage6Stage8bP1SemanticCommitEvidenceV1,
        command: BrokerCommand,
        durable_request_identity: Stage6DurableRequestIdentityV1,
        durable_command_snapshot: Stage6DurableCommandSnapshotV1,
    },
    MultiIntentBlocked {
        semantic_batch_id_sha256: String,
        intent_count: usize,
        request_ids: Vec<StrategyRequestId>,
    },
}

/// First P1-d2 replacement package. The caller must durably commit and reread
/// this S_ack package before consuming the recovered owner into S_truth.
pub struct Stage6Stage8bP1d2AckTransition {
    pub recovered: Stage6dDurableRuntimeRecovered,
    pub stage5g_restart_package: Vec<u8>,
}

/// Second P1-d2 replacement package. Source acknowledgement remains outside
/// this value and may happen only after this S_truth package is committed and
/// reread by the Stage 7 owner.
pub struct Stage6Stage8bP1d2TruthTransition {
    pub recovered: Stage6dDurableRuntimeRecovered,
    pub stage5g_restart_package: Vec<u8>,
}

/// One-time, effect-free conversion from an authenticated quiescent P1-d2
/// truth package into the first P1-d3 working-book generation.  The book
/// generation is minted here rather than accepted from Redis or a caller.
pub struct Stage6Stage8bP1d3MigrationTransition {
    pub recovered: Stage6dDurableRuntimeRecovered,
    pub stage5g_restart_package: Vec<u8>,
}

/// Reservation-bearing generated-Market Prepublication package. The caller
/// must commit and reread its covering recovery seal before the exact-ID
/// Redis publication is allowed.
pub struct Stage6Stage8bP1d4PrepublicationTransition {
    pub recovered: Stage6dDurableRuntimeRecovered,
    pub stage5g_restart_package: Vec<u8>,
}

/// Generated-Market replacement S_ack. The original P1-d3 working book,
/// publication reservation/binding and Market feedback are one authenticated
/// package; source acknowledgement remains outside this value.
pub struct Stage6Stage8bP1d4AckTransition {
    pub recovered: Stage6dDurableRuntimeRecovered,
    pub stage5g_restart_package: Vec<u8>,
}

/// Generated-Market replacement S_truth. The exact Redis source is still
/// pending and may be acknowledged only after the service commits and rereads
/// the covering seal for this package.
pub struct Stage6Stage8bP1d4TruthTransition {
    pub recovered: Stage6dDurableRuntimeRecovered,
    pub stage5g_restart_package: Vec<u8>,
}

/// First P1-d3 request replacement. The service owner must durably commit and
/// reread this S_ack package before the truth-only continuation is reachable.
pub struct Stage6Stage8bP1d3AckTransition {
    pub recovered: Stage6dDurableRuntimeRecovered,
    pub stage5g_restart_package: Vec<u8>,
}

/// Final request-processing replacement for an initial LIMIT observation.
/// A Working result still keeps an active order in the authenticated book,
/// while Filled/Expired results carry a terminal book.
pub struct Stage6Stage8bP1d3TruthTransition {
    pub recovered: Stage6dDurableRuntimeRecovered,
    pub stage5g_restart_package: Vec<u8>,
}

/// Durable replacement produced by one later Working-LIMIT observation.
/// An exact already-evaluated bar is returned without a new package; an
/// untouched bar yields S_eval, while fill/expiry yields S_terminal.
pub enum Stage6Stage8bP1d3LaterTransition {
    AlreadyEvaluated {
        recovered: Stage6dDurableRuntimeRecovered,
    },
    EvaluationCommitted {
        recovered: Stage6dDurableRuntimeRecovered,
        stage5g_restart_package: Vec<u8>,
    },
    TruthCommitted {
        recovered: Stage6dDurableRuntimeRecovered,
        stage5g_restart_package: Vec<u8>,
    },
}

/// Durable result of the first cancel-candidate pass. A Working target yields
/// S_ack; an already-terminal target yields S_cancel_recovered; a target that
/// fills on the candidate yields S_terminal first and must continue through
/// the dedicated no-input cancel continuation after that seal is reread.
pub enum Stage6Stage8bP1d3CancelTransition {
    AckCommitted {
        recovered: Stage6dDurableRuntimeRecovered,
        stage5g_restart_package: Vec<u8>,
    },
    RecoveredCommitted {
        recovered: Stage6dDurableRuntimeRecovered,
        stage5g_restart_package: Vec<u8>,
    },
    TargetTruthCommitted {
        recovered: Stage6dDurableRuntimeRecovered,
        stage5g_restart_package: Vec<u8>,
    },
}

/// Replacement continuation reconstructed from an already durable P1-d3 V3
/// suffix. The variant is derived from authenticated evidence and therefore
/// cannot be selected by a caller after restart.
pub enum Stage6Stage8bP1d3RecoveredTransition {
    Ready(Stage6Stage8bP1d3TruthTransition),
    AckCommitted(Stage6Stage8bP1d3AckTransition),
    TruthCommitted(Stage6Stage8bP1d3TruthTransition),
    CancelContinuationPending(Stage6Stage8bP1d3TruthTransition),
    SemanticCallbackPending(Stage6Stage8bP1d3TruthTransition),
}

/// Authenticated continuation phase reconstructed from the replacement
/// Stage 5G package and the exact Stage 6 checkpoint covered by it.  This is
/// deliberately coarser than the internal working-book phase: callers need
/// only know which linear continuation is legal after restart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage6Stage8bP1d3RestartPhase {
    ReadyForEvaluation,
    AckCommitted,
    TruthCommitted,
    CancelContinuationPending,
    SemanticCallbackPending,
    SemanticCallbackCommitted,
}

/// Authenticated readback of one P1-d3 write-ahead outcome. The binding
/// carries the actual post-append Stage 6 checkpoint separately from the
/// pre-reserved token embedded in the outcome bytes.
pub(crate) struct Stage6Stage8bP1d3OutcomeAppendReceipt {
    pub(crate) outcome_record: Stage6JournalRecordV3,
    pub(crate) recovery_binding:
        crate::stage8b_p1d3_working_limit::Stage8bP1d3Stage6RecoveryBinding,
}

/// Exact read-only description of the one P1 RequestAccepted suffix that may
/// be recoverable under S0.  It is not Ready and carries no writer/provider
/// operation.
pub struct Stage6Stage8bP1JournalAheadCandidate {
    identity: Stage6DurableRequestIdentityV1,
    command: Stage6DurableCommandSnapshotV1,
    record_id: Stage6JournalRecordId,
    source_evidence_sha256: Stage6Sha256Digest,
}

/// Exact Stage 6 suffix recoverable while the committed Stage 5G package is
/// still the P1 pre-ACK authority. It proves one dispatch plus one Market
/// order/trade outcome and an optional terminal RequestFinalized record.
/// The value grants no dispatch or provider operation.
pub struct Stage6Stage8bP1d2JournalAheadCandidate {
    identity: Stage6DurableRequestIdentityV1,
    broker_order_id: BrokerOrderId,
    broker_trade_id: BrokerTradeId,
    request_finalized: bool,
}

/// One exact P1-d3 V3 suffix recoverable under the predecessor replacement
/// seal. It is read-only classification evidence and grants neither provider
/// execution nor source acknowledgement.
pub struct Stage6Stage8bP1d3JournalAheadCandidate {
    outcome_record: Stage6JournalRecordV3,
    request_finalized: bool,
}

/// Exact P1-d3 request whose sole dispatch attempt is durable while no V3
/// outcome exists yet.  This is classification evidence only: it grants no
/// dispatch, provider, schedule, callback or source-resolution capability.
pub struct Stage6Stage8bP1d3DispatchOnlyCandidate {
    identity: Stage6DurableRequestIdentityV1,
    command: BrokerCommand,
    command_snapshot: Stage6DurableCommandSnapshotV1,
    accepted_record_id: Stage6JournalRecordId,
    dispatch_record_id: Stage6JournalRecordId,
    dispatch_sequence: u64,
    predecessor_checkpoint_sha256: String,
}

/// Exact generated-Market V1 suffix found ahead of the reservation-bearing
/// predecessor package. The enum identifies only the missing durable effect;
/// it grants no schedule, provider, ACK or source-resolution authority.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage6Stage8bP1d4JournalAheadKind {
    DispatchOnly,
    OrderOnly,
    PreFinalization,
    PreAck,
}

pub struct Stage6Stage8bP1d4JournalAheadCandidate {
    identity: Stage6DurableRequestIdentityV1,
    kind: Stage6Stage8bP1d4JournalAheadKind,
    broker_order_id: Option<BrokerOrderId>,
    broker_trade_id: Option<BrokerTradeId>,
    dispatch_record_id: Stage6JournalRecordId,
    predecessor_checkpoint_sha256: String,
}

impl Stage6Stage8bP1JournalAheadCandidate {
    pub fn identity(&self) -> &Stage6DurableRequestIdentityV1 {
        &self.identity
    }

    pub fn command(&self) -> &Stage6DurableCommandSnapshotV1 {
        &self.command
    }

    pub fn record_id(&self) -> &Stage6JournalRecordId {
        &self.record_id
    }

    pub fn source_evidence_sha256(&self) -> &Stage6Sha256Digest {
        &self.source_evidence_sha256
    }
}

impl Stage6Stage8bP1d2JournalAheadCandidate {
    pub fn identity(&self) -> &Stage6DurableRequestIdentityV1 {
        &self.identity
    }

    pub fn broker_order_id(&self) -> &BrokerOrderId {
        &self.broker_order_id
    }

    pub fn broker_trade_id(&self) -> &BrokerTradeId {
        &self.broker_trade_id
    }

    pub fn request_finalized(&self) -> bool {
        self.request_finalized
    }
}

impl Stage6Stage8bP1d3JournalAheadCandidate {
    pub fn operational_identity_sha256(&self) -> &str {
        self.outcome_record.operational_identity_sha256()
    }

    pub fn request_finalized(&self) -> bool {
        self.request_finalized
    }

    pub fn is_request_scoped(&self) -> bool {
        crate::stage8b_p1d3_working_limit::Stage8bP1d3OutcomeEvidenceV1::decode_canonical(
            self.outcome_record.outcome_evidence_bytes(),
        )
        .is_ok_and(|evidence| evidence.is_request_scoped())
    }

    pub fn semantic_source_binding(&self) -> Option<crate::Stage8bP1d3SemanticSourceBinding> {
        let evidence =
            crate::stage8b_p1d3_working_limit::Stage8bP1d3OutcomeEvidenceV1::decode_canonical(
                self.outcome_record.outcome_evidence_bytes(),
            )
            .ok()?;
        evidence.semantic_source_binding()
    }
}

impl Stage6Stage8bP1d3DispatchOnlyCandidate {
    pub fn identity(&self) -> &Stage6DurableRequestIdentityV1 {
        &self.identity
    }

    pub fn command(&self) -> &BrokerCommand {
        &self.command
    }

    pub fn command_snapshot(&self) -> &Stage6DurableCommandSnapshotV1 {
        &self.command_snapshot
    }

    pub fn accepted_record_id(&self) -> &Stage6JournalRecordId {
        &self.accepted_record_id
    }

    pub fn dispatch_record_id(&self) -> &Stage6JournalRecordId {
        &self.dispatch_record_id
    }

    pub fn predecessor_checkpoint_sha256(&self) -> &str {
        &self.predecessor_checkpoint_sha256
    }

    pub fn dispatch_sequence(&self) -> u64 {
        self.dispatch_sequence
    }

    pub fn is_limit_place(&self) -> bool {
        matches!(self.command, BrokerCommand::PlaceOrder(_))
    }

    pub fn is_cancel(&self) -> bool {
        matches!(self.command, BrokerCommand::CancelOrder(_))
    }
}

impl Stage6Stage8bP1d4JournalAheadCandidate {
    pub fn identity(&self) -> &Stage6DurableRequestIdentityV1 {
        &self.identity
    }

    pub fn kind(&self) -> Stage6Stage8bP1d4JournalAheadKind {
        self.kind
    }

    pub fn broker_order_id(&self) -> Option<&BrokerOrderId> {
        self.broker_order_id.as_ref()
    }

    pub fn broker_trade_id(&self) -> Option<&BrokerTradeId> {
        self.broker_trade_id.as_ref()
    }

    pub fn dispatch_record_id(&self) -> &Stage6JournalRecordId {
        &self.dispatch_record_id
    }

    pub fn predecessor_checkpoint_sha256(&self) -> &str {
        &self.predecessor_checkpoint_sha256
    }
}

/// Linear read-only proof that an exact command identity and command snapshot
/// are present in the recovered Stage 6 journal. It exposes the authenticated
/// accepted record only as immutable composition input and grants no dispatch
/// or storage operation.
pub struct Stage6DurableRequestAuthorityV1 {
    identity: Stage6DurableRequestIdentityV1,
    canonical_command_sha256: Stage6Sha256Digest,
    accepted_record: Stage6JournalRecordV1,
    accepted_record_id: Stage6JournalRecordId,
    dispatch_record_id: Stage6JournalRecordId,
    dispatch_sequence: u64,
    durable_frontier_sha256: String,
    runtime_config_fingerprint_sha256: String,
    authenticated_checkpoint_sha256: String,
    request_state_fingerprint_sha256: Stage6Sha256Digest,
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

    pub fn accepted_record(&self) -> &Stage6JournalRecordV1 {
        &self.accepted_record
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

    pub fn request_state_fingerprint_sha256(&self) -> &Stage6Sha256Digest {
        &self.request_state_fingerprint_sha256
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

/// Broker-neutral, linear and authenticated Stage 8A-4 storage capability.
///
/// It is deliberately not `Clone`, `Debug` or `Serialize`. Public V2/read
/// material is insufficient to mutate storage: the current lifecycle
/// commitment key must authenticate this exact request, batch and S0 binding.
struct Stage6Stage8a4SealedWriteAuthority {
    identity: Stage6DurableRequestIdentityV1,
    command: Stage6DurableCommandSnapshotV1,
    batch: Stage6Stage8a4DurableBatch,
    operational_identity_sha256: String,
    runtime_config_fingerprint_sha256: String,
    seal_generation: u64,
    seal_commitment_sha256: String,
    source_evidence_binding_sha256: Stage6Sha256Digest,
    writer_truth_binding_sha256: Stage6Sha256Digest,
    control_binding_sha256: Stage6Sha256Digest,
    authority_commitment_sha256: String,
    authority_hmac_sha256: String,
}

/// Broker-neutral one-shot writer-entry evidence. This is not a storage
/// capability: it is revalidated and consumed inside strategy-runtime-core,
/// where the private HMAC authority is minted and immediately applied.
///
/// The type is linear and non-serializable. No API converts public V2/read
/// material directly into the private write capability.
pub struct Stage6Stage8a4ValidatedWriteEntry {
    identity: Stage6DurableRequestIdentityV1,
    command: Stage6DurableCommandSnapshotV1,
    batch: Stage6Stage8a4DurableBatch,
    operational_identity_sha256: String,
    runtime_config_fingerprint_sha256: String,
    seal_generation: u64,
    seal_commitment_sha256: String,
    source_evidence_binding_sha256: Stage6Sha256Digest,
    writer_truth_binding_sha256: Stage6Sha256Digest,
    control_binding_sha256: Stage6Sha256Digest,
    issuer_public_key_hex: String,
}

/// Read-only canonical material for suffix recovery after the original I2
/// process has disappeared. It grants no storage authority.
pub struct Stage6Stage8a4PendingRecovery {
    transition_record: Stage6JournalRecordV2,
    durable_command: Stage6DurableCommandSnapshotV1,
}

impl Stage6Stage8a4PendingRecovery {
    pub fn transition_record(&self) -> &Stage6JournalRecordV2 {
        &self.transition_record
    }

    pub fn durable_command(&self) -> &Stage6DurableCommandSnapshotV1 {
        &self.durable_command
    }

    pub fn into_parts(self) -> (Stage6JournalRecordV2, Stage6DurableCommandSnapshotV1) {
        (self.transition_record, self.durable_command)
    }
}

impl Stage6Stage8a4SealedWriteAuthority {
    #[allow(clippy::too_many_arguments)]
    fn seal(
        commitment_key: &Stage5gLifecycleCommitmentKey,
        identity: Stage6DurableRequestIdentityV1,
        command: Stage6DurableCommandSnapshotV1,
        batch: Stage6Stage8a4DurableBatch,
        operational_identity_sha256: String,
        runtime_config_fingerprint_sha256: String,
        seal_generation: u64,
        seal_commitment_sha256: String,
        source_evidence_binding_sha256: Stage6Sha256Digest,
        writer_truth_binding_sha256: Stage6Sha256Digest,
        control_binding_sha256: Stage6Sha256Digest,
    ) -> Result<Self, Stage6dLiveCoreError> {
        let authority_commitment_sha256 = stage8a4_write_authority_commitment(
            &identity,
            &command,
            &batch,
            &operational_identity_sha256,
            &runtime_config_fingerprint_sha256,
            seal_generation,
            &seal_commitment_sha256,
            &source_evidence_binding_sha256,
            &writer_truth_binding_sha256,
            &control_binding_sha256,
        )?;
        let authority_hmac_sha256 =
            commitment_key.stage8a4_write_authority_hmac_sha256(&authority_commitment_sha256);
        Ok(Self {
            identity,
            command,
            batch,
            operational_identity_sha256,
            runtime_config_fingerprint_sha256,
            seal_generation,
            seal_commitment_sha256,
            source_evidence_binding_sha256,
            writer_truth_binding_sha256,
            control_binding_sha256,
            authority_commitment_sha256,
            authority_hmac_sha256,
        })
    }

    fn verify(
        self,
        commitment_key: &Stage5gLifecycleCommitmentKey,
    ) -> Result<Self, Stage6dLiveCoreError> {
        let current = stage8a4_write_authority_commitment(
            &self.identity,
            &self.command,
            &self.batch,
            &self.operational_identity_sha256,
            &self.runtime_config_fingerprint_sha256,
            self.seal_generation,
            &self.seal_commitment_sha256,
            &self.source_evidence_binding_sha256,
            &self.writer_truth_binding_sha256,
            &self.control_binding_sha256,
        )?;
        if current != self.authority_commitment_sha256
            || !commitment_key
                .stage8a4_verify_write_authority_hmac_sha256(&current, &self.authority_hmac_sha256)
        {
            return Err(Stage6dLiveCoreError::Stage8a4WriteAuthorityInvalid);
        }
        Ok(self)
    }
}

impl Stage6Stage8a4ValidatedWriteEntry {
    #[allow(clippy::too_many_arguments)]
    pub fn verify_issuer_attestation(
        identity: Stage6DurableRequestIdentityV1,
        command: Stage6DurableCommandSnapshotV1,
        batch: Stage6Stage8a4DurableBatch,
        operational_identity_sha256: String,
        runtime_config_fingerprint_sha256: String,
        seal_generation: u64,
        seal_commitment_sha256: String,
        source_evidence_binding_sha256: Stage6Sha256Digest,
        writer_truth_binding_sha256: Stage6Sha256Digest,
        control_binding_sha256: Stage6Sha256Digest,
        issuer_public_key_hex: String,
        issuer_signature_hex: String,
    ) -> Result<Self, Stage6dLiveCoreError> {
        if batch.transition_record().durable_request_identity() != &identity
            || command.action() != identity.action()
            || seal_generation == 0
            || Stage6Sha256Digest::parse(operational_identity_sha256.clone()).is_err()
            || Stage6Sha256Digest::parse(runtime_config_fingerprint_sha256.clone()).is_err()
            || Stage6Sha256Digest::parse(seal_commitment_sha256.clone()).is_err()
        {
            return Err(Stage6dLiveCoreError::Stage8a4WriteAuthorityInvalid);
        }
        let attestation_sha256 = stage8a4_writer_entry_attestation_sha256(
            &identity,
            &command,
            &batch,
            &operational_identity_sha256,
            &runtime_config_fingerprint_sha256,
            seal_generation,
            &seal_commitment_sha256,
            &source_evidence_binding_sha256,
            &writer_truth_binding_sha256,
            &control_binding_sha256,
        )?;
        verify_stage8a4_writer_signature(
            &issuer_public_key_hex,
            &issuer_signature_hex,
            &attestation_sha256,
        )?;
        Ok(Self {
            identity,
            command,
            batch,
            operational_identity_sha256,
            runtime_config_fingerprint_sha256,
            seal_generation,
            seal_commitment_sha256,
            source_evidence_binding_sha256,
            writer_truth_binding_sha256,
            control_binding_sha256,
            issuer_public_key_hex,
        })
    }

    pub fn identity(&self) -> &Stage6DurableRequestIdentityV1 {
        &self.identity
    }

    pub fn command(&self) -> &Stage6DurableCommandSnapshotV1 {
        &self.command
    }

    pub fn operational_identity_sha256(&self) -> &str {
        &self.operational_identity_sha256
    }

    pub fn runtime_config_fingerprint_sha256(&self) -> &str {
        &self.runtime_config_fingerprint_sha256
    }

    pub fn seal_generation(&self) -> u64 {
        self.seal_generation
    }

    pub fn seal_commitment_sha256(&self) -> &str {
        &self.seal_commitment_sha256
    }

    pub fn issuer_public_key_hex(&self) -> &str {
        &self.issuer_public_key_hex
    }

    pub fn expected_recovery_seal_generation(&self) -> u64 {
        self.batch
            .transition_record
            .payload()
            .pre_append_precondition()
            .expected_recovery_seal_generation()
    }

    pub fn expected_recovery_seal_fingerprint(&self) -> &str {
        self.batch
            .transition_record
            .payload()
            .pre_append_precondition()
            .expected_recovery_seal_fingerprint()
            .as_str()
    }

    pub fn matches_current_tail(
        &self,
        recovered: &Stage6dDurableRuntimeRecovered,
    ) -> Result<bool, Stage6dLiveCoreError> {
        recovered.stage8a4_batch_matches_current_tail(&self.batch)
    }

    fn into_sealed(
        self,
        commitment_key: &Stage5gLifecycleCommitmentKey,
    ) -> Result<Stage6Stage8a4SealedWriteAuthority, Stage6dLiveCoreError> {
        Stage6Stage8a4SealedWriteAuthority::seal(
            commitment_key,
            self.identity,
            self.command,
            self.batch,
            self.operational_identity_sha256,
            self.runtime_config_fingerprint_sha256,
            self.seal_generation,
            self.seal_commitment_sha256,
            self.source_evidence_binding_sha256,
            self.writer_truth_binding_sha256,
            self.control_binding_sha256,
        )
    }
}

/// Canonical digest signed by the private Stage 8A-4 writer issuer. Computing
/// this digest grants no authority; the corresponding private Ed25519 key is
/// deliberately absent from strategy-runtime-core and runtime-durable-service.
#[allow(clippy::too_many_arguments)]
pub fn stage8a4_writer_entry_attestation_sha256(
    identity: &Stage6DurableRequestIdentityV1,
    command: &Stage6DurableCommandSnapshotV1,
    batch: &Stage6Stage8a4DurableBatch,
    operational_identity_sha256: &str,
    runtime_config_fingerprint_sha256: &str,
    seal_generation: u64,
    seal_commitment_sha256: &str,
    source_evidence_binding_sha256: &Stage6Sha256Digest,
    writer_truth_binding_sha256: &Stage6Sha256Digest,
    control_binding_sha256: &Stage6Sha256Digest,
) -> Result<Stage6Sha256Digest, Stage6dLiveCoreError> {
    let commitment = stage8a4_write_authority_commitment(
        identity,
        command,
        batch,
        operational_identity_sha256,
        runtime_config_fingerprint_sha256,
        seal_generation,
        seal_commitment_sha256,
        source_evidence_binding_sha256,
        writer_truth_binding_sha256,
        control_binding_sha256,
    )?;
    Stage6Sha256Digest::parse(sha256_hex(
        [
            b"stage8a4-writer-issuer-attestation-v1".as_slice(),
            commitment.as_bytes(),
        ]
        .concat()
        .as_slice(),
    ))
    .map_err(|_| Stage6dLiveCoreError::Stage8a4WriteAuthorityInvalid)
}

fn verify_stage8a4_writer_signature(
    issuer_public_key_hex: &str,
    issuer_signature_hex: &str,
    attestation_sha256: &Stage6Sha256Digest,
) -> Result<(), Stage6dLiveCoreError> {
    let public_key = decode_fixed_hex::<32>(issuer_public_key_hex)?;
    let signature = decode_fixed_hex::<64>(issuer_signature_hex)?;
    let verifying_key = VerifyingKey::from_bytes(&public_key)
        .map_err(|_| Stage6dLiveCoreError::Stage8a4WriteAuthorityInvalid)?;
    verifying_key
        .verify(
            attestation_sha256.as_str().as_bytes(),
            &Signature::from_bytes(&signature),
        )
        .map_err(|_| Stage6dLiveCoreError::Stage8a4WriteAuthorityInvalid)
}

fn decode_fixed_hex<const N: usize>(value: &str) -> Result<[u8; N], Stage6dLiveCoreError> {
    if value.len() != N * 2 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(Stage6dLiveCoreError::Stage8a4WriteAuthorityInvalid);
    }
    let mut decoded = [0_u8; N];
    for (index, byte) in decoded.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16)
            .map_err(|_| Stage6dLiveCoreError::Stage8a4WriteAuthorityInvalid)?;
    }
    Ok(decoded)
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

    /// Reconstructs the exact missing V1 compatibility suffix from a canonical
    /// persisted V2 transition. No process-local I2 candidate is required;
    /// every regenerated record must match the persisted suffix manifest.
    pub fn recover_from_persisted_transition(
        transition_record: Stage6JournalRecordV2,
    ) -> Result<Self, Stage6dLiveCoreError> {
        let (suffix_records, cancel_original_target_shape) =
            crate::stage6_reconciliation_v2::reconstruct_stage8a4_suffix_from_v2(
                &transition_record,
            )?;
        Self::new(
            transition_record,
            suffix_records,
            cancel_original_target_shape,
        )
    }
}

#[allow(clippy::too_many_arguments)]
fn stage8a4_write_authority_commitment(
    identity: &Stage6DurableRequestIdentityV1,
    command: &Stage6DurableCommandSnapshotV1,
    batch: &Stage6Stage8a4DurableBatch,
    operational_identity_sha256: &str,
    runtime_config_fingerprint_sha256: &str,
    seal_generation: u64,
    seal_commitment_sha256: &str,
    source_evidence_binding_sha256: &Stage6Sha256Digest,
    writer_truth_binding_sha256: &Stage6Sha256Digest,
    control_binding_sha256: &Stage6Sha256Digest,
) -> Result<String, Stage6dLiveCoreError> {
    if seal_generation == 0
        || Stage6Sha256Digest::parse(operational_identity_sha256.to_string()).is_err()
        || Stage6Sha256Digest::parse(runtime_config_fingerprint_sha256.to_string()).is_err()
        || Stage6Sha256Digest::parse(seal_commitment_sha256.to_string()).is_err()
    {
        return Err(Stage6dLiveCoreError::Stage8a4WriteAuthorityInvalid);
    }
    let mut hasher = Sha256::new();
    hasher.update(b"moex.stage8a4.sealed-durable-write-authority.v1\0");
    stage8a4_hash_part(
        &mut hasher,
        &serde_json::to_vec(identity)
            .map_err(|_| Stage6dLiveCoreError::Stage8a4WriteAuthorityInvalid)?,
    );
    stage8a4_hash_part(
        &mut hasher,
        &serde_json::to_vec(command)
            .map_err(|_| Stage6dLiveCoreError::Stage8a4WriteAuthorityInvalid)?,
    );
    stage8a4_hash_part(&mut hasher, &batch.transition_record.encode_canonical());
    for record in &batch.suffix_records {
        stage8a4_hash_part(&mut hasher, &record.encode_canonical());
    }
    stage8a4_hash_part(
        &mut hasher,
        &serde_json::to_vec(&batch.cancel_original_target_shape)
            .map_err(|_| Stage6dLiveCoreError::Stage8a4WriteAuthorityInvalid)?,
    );
    stage8a4_hash_part(&mut hasher, operational_identity_sha256.as_bytes());
    stage8a4_hash_part(&mut hasher, runtime_config_fingerprint_sha256.as_bytes());
    stage8a4_hash_part(&mut hasher, &seal_generation.to_be_bytes());
    stage8a4_hash_part(&mut hasher, seal_commitment_sha256.as_bytes());
    stage8a4_hash_part(
        &mut hasher,
        source_evidence_binding_sha256.as_str().as_bytes(),
    );
    stage8a4_hash_part(&mut hasher, writer_truth_binding_sha256.as_str().as_bytes());
    stage8a4_hash_part(&mut hasher, control_binding_sha256.as_str().as_bytes());
    Ok(hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

fn stage8a4_hash_part(hasher: &mut Sha256, bytes: &[u8]) {
    hasher.update((bytes.len() as u64).to_be_bytes());
    hasher.update(bytes);
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
    fn stage8b_p1e_candidate_matches_runtime(
        &self,
        candidate: &crate::Stage8bP1eScheduleBindingCandidateV1,
        expected_stage5_checkpoint_sha256: &str,
        is_recovery: bool,
    ) -> Result<bool, Stage6dLiveCoreError> {
        match candidate.transition_kind() {
            crate::Stage8bP1eScheduleTransitionKindV1::MarketExecution => {
                let decision = self.stage8b_p1d1_command_decision_binding()?;
                let decision_request_id = decision.strategy_request_id().to_string();
                if !decision.matches_stage8b_p1e_market_candidate(candidate) {
                    return Ok(false);
                }
                if is_recovery {
                    return Ok(true);
                }
                Ok(matches!(
                    self.journal.versioned_records().last(),
                    Some(Stage6JournalRecordVersioned::V1(record))
                        if record.event_kind() == Stage6JournalEventKind::RequestAccepted
                            && record
                                .durable_request_identity()
                                .strategy_request_id()
                                .to_string()
                                == decision_request_id
                ))
            }
            crate::Stage8bP1eScheduleTransitionKindV1::WorkingLimitEvaluation
            | crate::Stage8bP1eScheduleTransitionKindV1::CancelStep
            | crate::Stage8bP1eScheduleTransitionKindV1::DayExpiry => {
                let Stage6dStage5RuntimeAuthority::Restart(restart) = &self.stage5_runtime else {
                    return Ok(false);
                };
                let Some(replacement) = restart.stage8b_p1d3_replacement() else {
                    return Ok(false);
                };
                Ok(replacement.authenticated_stage6_checkpoint_sha256()
                    == expected_stage5_checkpoint_sha256
                    && replacement.matches_stage8b_p1e_schedule_candidate(candidate)?)
            }
        }
    }

    fn stage8b_p1e_pre_binding_checkpoint(
        &self,
        record: &crate::Stage6JournalRecordV4,
    ) -> Result<Stage6JournalCheckpointV1, Stage6dLiveCoreError> {
        let records = self.journal.versioned_records();
        if !matches!(
            records.last(),
            Some(Stage6JournalRecordVersioned::V4(tail))
                if tail.encode_canonical() == record.encode_canonical()
        ) {
            return Err(Stage6dLiveCoreError::DurableOrderingViolation);
        }
        let mut prefix = Stage6MemoryJournalBackend::new();
        for historical in records.iter().take(records.len().saturating_sub(1)) {
            prefix.append_versioned(historical)?;
        }
        if prefix.frontier().last_record_id() != Some(record.previous_record_id())
            || prefix
                .frontier()
                .last_lifecycle_sequence()
                .and_then(|sequence| sequence.get().checked_add(1))
                != Some(record.lifecycle_sequence().get())
        {
            return Err(Stage6dLiveCoreError::DurableOrderingViolation);
        }
        Ok(Stage6JournalCheckpointV1::from_frontier(
            prefix.frontier().clone(),
        )?)
    }

    /// Returns the sole authenticated V4 tail that extends the exact
    /// replacement checkpoint by one schedule binding. It never accepts a
    /// non-tail record, a second intervening record or a replacement that did
    /// not match the original schedule candidate.
    fn stage8b_p1e_current_v4_for_replacement<'a>(
        &'a self,
        replacement: &crate::stage8b_p1d3_working_limit::Stage8bP1d3ReplacementProjectionV1,
    ) -> Result<Option<&'a Stage6JournalRecordV4>, Stage6dLiveCoreError> {
        let Some(record) = self.current_stage8b_p1e_schedule_binding_record()? else {
            return Ok(None);
        };
        let predecessor = self.stage8b_p1e_pre_binding_checkpoint(record)?;
        if predecessor.checkpoint_sha256() != replacement.authenticated_stage6_checkpoint_sha256()
            || self.authenticated_checkpoint().frontier() != self.journal_frontier()
            || !replacement.matches_stage8b_p1e_schedule_v4_record(record)?
        {
            return Ok(None);
        }
        Ok(Some(record))
    }

    fn stage8b_p1e_replacement_is_current_or_v4_bound(
        &self,
        replacement: &crate::stage8b_p1d3_working_limit::Stage8bP1d3ReplacementProjectionV1,
    ) -> bool {
        replacement.authenticated_stage6_checkpoint_sha256()
            == self.authenticated_checkpoint().checkpoint_sha256()
            || self
                .stage8b_p1e_current_v4_for_replacement(replacement)
                .ok()
                .flatten()
                .is_some()
    }

    /// Authenticates the one journal-ahead shape in which a covered V4
    /// schedule binding sits between the replacement package and an already
    /// durable V3 outcome.  The V4 must be the exact record covered by the
    /// current recovery seal; only an optional adjacent dispatch may appear
    /// before the exact outcome.  This is deliberately separate from the
    /// ordinary current-checkpoint rule so an arbitrary stale replacement can
    /// never be promoted by merely finding an older V4 in the journal.
    fn stage8b_p1e_schedule_checkpoint_before_journal_ahead_outcome(
        &self,
        replacement: &crate::stage8b_p1d3_working_limit::Stage8bP1d3ReplacementProjectionV1,
        outcome_record: &Stage6JournalRecordV3,
    ) -> Result<Option<String>, Stage6dLiveCoreError> {
        let records = self.journal.versioned_records();
        if self.authenticated_checkpoint().frontier() != self.journal_frontier() {
            return Ok(None);
        }
        let Some(outcome_index) = records.iter().position(|record| {
            matches!(
                record,
                Stage6JournalRecordVersioned::V3(outcome)
                    if outcome.journal_record_id() == outcome_record.journal_record_id()
            )
        }) else {
            return Ok(None);
        };
        let Some((v4_index, v4)) = [1_usize, 2].into_iter().find_map(|distance| {
            let index = outcome_index.checked_sub(distance)?;
            match records.get(index)? {
                Stage6JournalRecordVersioned::V4(v4) => Some((index, v4)),
                _ => None,
            }
        }) else {
            return Ok(None);
        };

        let mut before_v4 = Stage6MemoryJournalBackend::new();
        for record in records.iter().take(v4_index) {
            before_v4.append_versioned(record)?;
        }
        let pre_binding_checkpoint =
            Stage6JournalCheckpointV1::from_frontier(before_v4.frontier().clone())?;
        if before_v4.frontier().last_record_id() != Some(v4.previous_record_id())
            || before_v4
                .frontier()
                .last_lifecycle_sequence()
                .and_then(|sequence| sequence.get().checked_add(1))
                != Some(v4.lifecycle_sequence().get())
            || replacement.authenticated_stage6_checkpoint_sha256()
                != pre_binding_checkpoint.checkpoint_sha256()
            || !replacement.matches_stage8b_p1e_schedule_v4_record(v4)?
        {
            return Ok(None);
        }
        before_v4.append_versioned(&records[v4_index])?;
        let schedule_checkpoint =
            Stage6JournalCheckpointV1::from_frontier(before_v4.frontier().clone())?;
        let evidence =
            crate::stage8b_p1d3_working_limit::Stage8bP1d3OutcomeEvidenceV1::decode_canonical(
                outcome_record.outcome_evidence_bytes(),
            )?;
        let exact_chain = if outcome_index == v4_index + 1 {
            outcome_record.previous_record_id() == v4.journal_record_id()
                && outcome_record.lifecycle_sequence().get()
                    == v4
                        .lifecycle_sequence()
                        .get()
                        .checked_add(1)
                        .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?
                && evidence.stage6_predecessor_checkpoint_sha256()
                    == schedule_checkpoint.checkpoint_sha256()
                && matches!(
                    (v4.transition_kind(), evidence.outcome_kind()),
                    (
                        crate::Stage8bP1eScheduleTransitionKindV1::WorkingLimitEvaluation,
                        crate::stage8b_p1d3_working_limit::Stage8bP1d3OutcomeKind::LaterFilled
                    ) | (
                        crate::Stage8bP1eScheduleTransitionKindV1::DayExpiry,
                        crate::stage8b_p1d3_working_limit::Stage8bP1d3OutcomeKind::LaterExpired
                    )
                )
        } else {
            let Stage6JournalRecordVersioned::V1(dispatch) = &records[v4_index + 1] else {
                return Ok(None);
            };
            if dispatch.event_kind() != Stage6JournalEventKind::DispatchAttemptRecorded
                || dispatch.previous_record_id() != Some(v4.journal_record_id())
                || dispatch.causal_parent_id() != Some(v4.journal_record_id())
                || dispatch.lifecycle_sequence().get()
                    != v4
                        .lifecycle_sequence()
                        .get()
                        .checked_add(1)
                        .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?
            {
                return Ok(None);
            }
            before_v4.append_versioned(&records[v4_index + 1])?;
            let dispatch_checkpoint =
                Stage6JournalCheckpointV1::from_frontier(before_v4.frontier().clone())?;
            outcome_record.previous_record_id() == dispatch.journal_record_id()
                && outcome_record.lifecycle_sequence().get()
                    == dispatch
                        .lifecycle_sequence()
                        .get()
                        .checked_add(1)
                        .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?
                && evidence.stage6_predecessor_checkpoint_sha256()
                    == dispatch_checkpoint.checkpoint_sha256()
                && v4.transition_kind()
                    == crate::Stage8bP1eScheduleTransitionKindV1::CancelStep
                && matches!(
                    evidence.outcome_kind(),
                    crate::stage8b_p1d3_working_limit::Stage8bP1d3OutcomeKind::LaterFilled
                        | crate::stage8b_p1d3_working_limit::Stage8bP1d3OutcomeKind::CancelCanceled
                        | crate::stage8b_p1d3_working_limit::Stage8bP1d3OutcomeKind::CancelExecutionObserved
                        | crate::stage8b_p1d3_working_limit::Stage8bP1d3OutcomeKind::CancelAlreadyTerminalNonExecution
                )
        };
        Ok(exact_chain.then(|| schedule_checkpoint.checkpoint_sha256().to_string()))
    }

    /// Appends and rereads the exact V4 schedule binding using the existing
    /// durable journal. The returned value is deliberately non-authorizing:
    /// the covering recovery seal still has to be committed and reread.
    pub fn append_stage8b_p1e_schedule_binding(
        &mut self,
        candidate: crate::Stage8bP1eScheduleBindingCandidateV1,
        prior_covering_seal_generation: u64,
        bound_at_utc: DateTime<Utc>,
    ) -> Result<Stage6Stage8bP1eScheduleBindingPending, Stage6dLiveCoreError> {
        if self.authenticated_checkpoint.frontier() != self.journal_frontier()
            || self
                .authenticated_operational_identity()
                .map(stage6d_operational_identity_sha256)
                .transpose()?
                .as_ref()
                .map(Stage6Sha256Digest::as_str)
                != Some(candidate.operational_identity_sha256())
        {
            return Err(Stage6dLiveCoreError::DurableOrderingViolation);
        }
        if !self.stage8b_p1e_candidate_matches_runtime(
            &candidate,
            self.authenticated_checkpoint().checkpoint_sha256(),
            false,
        )? {
            return Err(Stage6dLiveCoreError::DurableOrderingViolation);
        }
        let previous_record_id = self
            .journal_frontier()
            .last_record_id()
            .cloned()
            .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?;
        let lifecycle_sequence = self
            .journal_frontier()
            .last_lifecycle_sequence()
            .and_then(|sequence| sequence.get().checked_add(1))
            .and_then(|sequence| Stage6LifecycleSequence::new(sequence).ok())
            .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?;
        let record = Stage6JournalRecordV4::from_stage8b_p1e_candidate(
            &candidate,
            lifecycle_sequence,
            previous_record_id,
            prior_covering_seal_generation,
            bound_at_utc,
        )?;
        let receipt = self
            .journal_mut()
            .append_versioned(&Stage6JournalRecordVersioned::V4(record.clone()))?;
        if receipt.record_id() != record.journal_record_id()
            || receipt.lifecycle_sequence() != record.lifecycle_sequence()
        {
            return Err(Stage6dLiveCoreError::JournalMutationMayHaveOccurred);
        }
        self.refresh_after_append()?;
        let post_append_checkpoint = self.authenticated_checkpoint().clone();
        if post_append_checkpoint.frontier() != receipt.durable_frontier()
            || self.journal_frontier().last_record_id() != Some(record.journal_record_id())
            || self.journal_frontier().last_lifecycle_sequence()
                != Some(record.lifecycle_sequence())
            || self
                .journal
                .versioned_records()
                .last()
                .map(Stage6JournalRecordVersioned::encode_canonical)
                != Some(record.encode_canonical())
        {
            return Err(Stage6dLiveCoreError::JournalMutationMayHaveOccurred);
        }
        Ok(Stage6Stage8bP1eScheduleBindingPending {
            candidate,
            expected_covering_seal_generation: record.expected_covering_seal_generation(),
            record,
            post_append_checkpoint,
            prior_covering_seal_generation,
        })
    }

    /// Converts a non-authorizing append proof into an opaque committed
    /// binding only after the caller has supplied the generation of the
    /// covering seal it has itself committed and reread.
    pub fn complete_stage8b_p1e_schedule_binding(
        &self,
        pending: Stage6Stage8bP1eScheduleBindingPending,
        covering_seal_generation: u64,
    ) -> Result<crate::Stage8bP1eCommittedScheduleBindingV1, Stage6dLiveCoreError> {
        if covering_seal_generation != pending.expected_covering_seal_generation
            || pending.expected_covering_seal_generation
                != pending
                    .prior_covering_seal_generation
                    .checked_add(1)
                    .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?
            || self.authenticated_checkpoint() != &pending.post_append_checkpoint
            || self.journal_frontier().last_record_id() != Some(pending.record.journal_record_id())
            || self.journal_frontier().last_lifecycle_sequence()
                != Some(pending.record.lifecycle_sequence())
        {
            return Err(Stage6dLiveCoreError::DurableOrderingViolation);
        }
        let mixed = Stage6MixedReplayEngineV2::replay(self.journal.versioned_records())?;
        let reread = mixed
            .schedule_binding_records()
            .last()
            .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?;
        if reread.encode_canonical() != pending.record.encode_canonical() {
            return Err(Stage6dLiveCoreError::DurableOrderingViolation);
        }
        pending
            .candidate
            .into_committed_after_v4_reread(reread)
            .map_err(|_| Stage6dLiveCoreError::DurableOrderingViolation)
    }

    /// Returns the current V4 only when it is the exact authenticated journal
    /// tail. This is read-only recovery evidence and grants no route authority.
    pub fn current_stage8b_p1e_schedule_binding_record(
        &self,
    ) -> Result<Option<&crate::Stage6JournalRecordV4>, Stage6dLiveCoreError> {
        let mixed = Stage6MixedReplayEngineV2::replay(self.journal.versioned_records())?;
        let Some(record) = mixed.schedule_binding_records().last() else {
            return Ok(None);
        };
        if self.journal_frontier().last_record_id() != Some(record.journal_record_id())
            || self.journal_frontier().last_lifecycle_sequence()
                != Some(record.lifecycle_sequence())
        {
            return Ok(None);
        }
        self.journal
            .versioned_records()
            .last()
            .and_then(|record| match record {
                Stage6JournalRecordVersioned::V4(record) => Some(record),
                _ => None,
            })
            .map(Some)
            .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)
    }

    /// Reconstructs route authority only from the exact authenticated V4 tail.
    /// The runtime is consumed and returned beside the binding so callers
    /// cannot retain an independently reusable recovered authority.
    pub fn recover_stage8b_p1e_current_schedule_binding(
        self,
        expected_runtime_config_fingerprint_sha256: impl Into<String>,
        expected_instrument_map_fingerprint_sha256: impl Into<String>,
    ) -> Result<(Self, crate::Stage8bP1eCommittedScheduleBindingV1), Stage6dLiveCoreError> {
        let record = self
            .current_stage8b_p1e_schedule_binding_record()?
            .cloned()
            .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?;
        let candidate = crate::recover_stage8b_p1e_schedule_binding_candidate_v4(
            &record,
            expected_runtime_config_fingerprint_sha256,
            expected_instrument_map_fingerprint_sha256,
        )
        .map_err(|_| Stage6dLiveCoreError::DurableOrderingViolation)?;
        let predecessor_checkpoint = self.stage8b_p1e_pre_binding_checkpoint(&record)?;
        if !self.stage8b_p1e_candidate_matches_runtime(
            &candidate,
            predecessor_checkpoint.checkpoint_sha256(),
            true,
        )? {
            return Err(Stage6dLiveCoreError::DurableOrderingViolation);
        }
        let binding = candidate
            .into_committed_after_v4_reread(&record)
            .map_err(|_| Stage6dLiveCoreError::DurableOrderingViolation)?;
        Ok((self, binding))
    }

    /// Fixture-only restart seam that preserves the complete production V4
    /// reconstruction and runtime cross-validation path while replacing only
    /// the pinned verification key with an explicit test trust anchor.
    #[cfg(any(test, feature = "stage5g-artifact-fixtures"))]
    #[doc(hidden)]
    pub fn stage8b_p1e_test_recover_current_schedule_binding_with_key(
        self,
        expected_runtime_config_fingerprint_sha256: impl Into<String>,
        expected_instrument_map_fingerprint_sha256: impl Into<String>,
        public_key_hex: &str,
        key_valid_from: DateTime<Utc>,
        key_valid_until: DateTime<Utc>,
    ) -> Result<(Self, crate::Stage8bP1eCommittedScheduleBindingV1), Stage6dLiveCoreError> {
        let record = self
            .current_stage8b_p1e_schedule_binding_record()?
            .cloned()
            .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?;
        let candidate = crate::stage8b_p1e_test_recover_schedule_binding_candidate_v4_with_key(
            &record,
            expected_runtime_config_fingerprint_sha256,
            expected_instrument_map_fingerprint_sha256,
            public_key_hex,
            key_valid_from,
            key_valid_until,
        )
        .map_err(|_| Stage6dLiveCoreError::DurableOrderingViolation)?;
        let predecessor_checkpoint = self.stage8b_p1e_pre_binding_checkpoint(&record)?;
        if !self.stage8b_p1e_candidate_matches_runtime(
            &candidate,
            predecessor_checkpoint.checkpoint_sha256(),
            true,
        )? {
            return Err(Stage6dLiveCoreError::DurableOrderingViolation);
        }
        let binding = candidate
            .into_committed_after_v4_reread(&record)
            .map_err(|_| Stage6dLiveCoreError::DurableOrderingViolation)?;
        Ok((self, binding))
    }

    pub fn stage8b_p1d4_generated_market_candidate(&self) -> bool {
        matches!(
            &self.stage5_runtime,
            Stage6dStage5RuntimeAuthority::Restart(restart)
                if restart.stage8b_p1d4_generated_market_candidate()
        )
    }

    pub fn stage8b_p1d4_next_write_generation(&self) -> Option<u64> {
        let Stage6dStage5RuntimeAuthority::Restart(restart) = &self.stage5_runtime else {
            return None;
        };
        restart.stage8b_p1d4_next_write_generation().ok()
    }

    pub fn stage8b_p1d4_generated_market_package_state(
        &self,
    ) -> Result<Option<crate::Stage8bP1d4GeneratedMarketPackageState>, Stage6dLiveCoreError> {
        let Stage6dStage5RuntimeAuthority::Restart(restart) = &self.stage5_runtime else {
            return Ok(None);
        };
        restart
            .stage8b_p1d4_generated_market_package_state()
            .map_err(|_| Stage6dLiveCoreError::RestartPackageBindingMismatch)
    }

    pub fn stage8b_p1d3_journal_ahead_uses_command_source(
        &self,
        candidate: &Stage6Stage8bP1d3JournalAheadCandidate,
    ) -> bool {
        if candidate.is_request_scoped() {
            return true;
        }
        let Ok(evidence) =
            crate::stage8b_p1d3_working_limit::Stage8bP1d3OutcomeEvidenceV1::decode_canonical(
                candidate.outcome_record.outcome_evidence_bytes(),
            )
        else {
            return false;
        };
        matches!(
            &self.stage5_runtime,
            Stage6dStage5RuntimeAuthority::Restart(restart)
                if restart.stage8b_p1_semantic_commit().is_some_and(|semantic| {
                    matches!(
                        semantic.canonical_command.as_ref(),
                        Some(BrokerCommand::CancelOrder(cancel))
                            if cancel.order_id == *evidence.broker_order_id()
                    )
                })
        )
    }

    /// Classifies a P1-d3 replacement only when it is bound to the exact
    /// authenticated Stage 6 checkpoint.  A stale or independently supplied
    /// replacement is therefore never promoted into a lifecycle capability.
    pub fn stage8b_p1d3_restart_phase(&self) -> Option<Stage6Stage8bP1d3RestartPhase> {
        let Stage6dStage5RuntimeAuthority::Restart(restart) = &self.stage5_runtime else {
            return None;
        };
        let replacement = restart.stage8b_p1d3_replacement()?;
        if !self.stage8b_p1e_replacement_is_current_or_v4_bound(replacement) {
            return None;
        }
        if replacement.phase() == crate::stage8b_p1d3_working_limit::Stage8bP1d3BookPhase::Ack {
            return Some(Stage6Stage8bP1d3RestartPhase::AckCommitted);
        }
        if replacement
            .completed_command_matches_semantic_commit(restart.stage8b_p1_semantic_commit())
            .ok()?
        {
            return Some(Stage6Stage8bP1d3RestartPhase::TruthCommitted);
        }
        if replacement
            .pending_cancel_after_target_matches_semantic_commit(
                restart.stage8b_p1_semantic_commit(),
            )
            .ok()?
        {
            return Some(Stage6Stage8bP1d3RestartPhase::CancelContinuationPending);
        }
        let Some(pending_semantic) = replacement.pending_semantic_source_binding().ok()? else {
            return Some(Stage6Stage8bP1d3RestartPhase::ReadyForEvaluation);
        };
        Some(
            if restart
                .stage8b_p1_semantic_commit()
                .is_some_and(|semantic| pending_semantic.matches_semantic_commit(semantic))
            {
                Stage6Stage8bP1d3RestartPhase::SemanticCallbackCommitted
            } else {
                Stage6Stage8bP1d3RestartPhase::SemanticCallbackPending
            },
        )
    }

    /// Reports whether the authenticated replacement currently carries a
    /// Working/Eval LIMIT book that must consume the next M10 through the
    /// P1-d3 later-order transition.  It exposes no book fields and mints no
    /// schedule or lifecycle authority.
    pub fn stage8b_p1d3_requires_later_limit_evaluation(&self) -> bool {
        let Stage6dStage5RuntimeAuthority::Restart(restart) = &self.stage5_runtime else {
            return false;
        };
        restart
            .stage8b_p1d3_replacement()
            .is_some_and(|replacement| {
                self.stage8b_p1e_replacement_is_current_or_v4_bound(replacement)
                    && matches!(
                        replacement.phase(),
                        crate::stage8b_p1d3_working_limit::Stage8bP1d3BookPhase::Working
                            | crate::stage8b_p1d3_working_limit::Stage8bP1d3BookPhase::Eval
                    )
            })
    }

    #[cfg(any(test, feature = "stage5g-artifact-fixtures"))]
    pub fn stage8b_p1e_test_working_binding_parts(
        &self,
    ) -> Option<(BrokerOrderId, String, crate::Stage8bP1eM10IdentityV1)> {
        let Stage6dStage5RuntimeAuthority::Restart(restart) = &self.stage5_runtime else {
            return None;
        };
        restart
            .stage8b_p1d3_replacement()?
            .stage8b_p1e_test_working_binding_parts()
    }

    /// Identifies the exact target-sealed predecessor of a recovered CANCEL
    /// journal-ahead suffix. Unlike `stage8b_p1d3_restart_phase`, this narrow
    /// classifier deliberately examines the predecessor package before the
    /// uncovered suffix checkpoint can be replacement-sealed.
    #[doc(hidden)]
    pub fn stage8b_p1d3_has_cancel_continuation_predecessor(&self) -> bool {
        let Stage6dStage5RuntimeAuthority::Restart(restart) = &self.stage5_runtime else {
            return false;
        };
        restart
            .stage8b_p1d3_replacement()
            .and_then(|replacement| {
                replacement
                    .pending_cancel_after_target_matches_semantic_commit(
                        restart.stage8b_p1_semantic_commit(),
                    )
                    .ok()
            })
            .unwrap_or(false)
    }

    /// Exact current M10 whose order evaluation is already covered by the
    /// authenticated replacement package.  It is recovery material only and
    /// cannot authorize schedule selection, provider invocation or XACK.
    pub fn stage8b_p1d3_pending_semantic_source(
        &self,
    ) -> Option<crate::Stage8bP1d3SemanticSourceBinding> {
        let Stage6dStage5RuntimeAuthority::Restart(restart) = &self.stage5_runtime else {
            return None;
        };
        let replacement = restart.stage8b_p1d3_replacement()?;
        if replacement.authenticated_stage6_checkpoint_sha256()
            != self.authenticated_checkpoint.checkpoint_sha256()
        {
            return None;
        }
        replacement.pending_semantic_source_binding().ok().flatten()
    }

    /// Read-only identity recovered from the authenticated Stage 5G package.
    /// First-boot runtimes that did not cross that package boundary have no
    /// such binding.
    pub fn authenticated_stage5_binding(
        &self,
    ) -> Option<(&str, &BrokerAccountId, &InstrumentId, &str)> {
        match &self.stage5_runtime {
            Stage6dStage5RuntimeAuthority::Restart(restart) => {
                Some(restart.stage6d_restart_binding())
            }
            Stage6dStage5RuntimeAuthority::FirstBoot(_) => None,
        }
    }

    /// Selects the sole current request that is eligible for read-only
    /// reconciliation preflight after one durable dispatch attempt.  The
    /// selection is derived exclusively from the authenticated journal/replay
    /// state: callers cannot nominate a request id or supply command bytes.
    ///
    /// Historical terminal requests are ignored.  Zero candidates, multiple
    /// non-terminal candidates, conflicts and any shape that cannot obtain the
    /// existing exact-request authority all fail closed.
    pub fn single_exact_dispatch_ready_request(
        &self,
    ) -> Result<
        (
            Stage6DurableRequestIdentityV1,
            Stage6DurableCommandSnapshotV1,
        ),
        Stage6dLiveCoreError,
    > {
        let mut candidates = self.replay.requests().iter().filter(|request| {
            request.dispatch_attempt_count() == 1
                && request.dispatch_safety_state()
                    == crate::Stage6DispatchSafetyStateV1::ReconciliationRequired
                && request.final_disposition().is_none()
                && !request.conflict_observed()
        });
        let candidate = candidates
            .next()
            .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?;
        if candidates.next().is_some() {
            return Err(Stage6dLiveCoreError::DurableOrderingViolation);
        }
        let accepted = stage7a_accepted_record(self, candidate.strategy_request_id())
            .ok_or(Stage6dLiveCoreError::AcceptedRecordRequired)?;
        let command = match accepted.payload() {
            Stage6JournalPayloadV1::RequestAccepted { command } => command.as_ref().clone(),
            _ => return Err(Stage6dLiveCoreError::AcceptedRecordRequired),
        };
        let identity = accepted.durable_request_identity().clone();
        self.authorize_exact_durable_request(&identity, &command)?;
        Ok((identity, command))
    }

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
            accepted_record: accepted.clone(),
            accepted_record_id: accepted.journal_record_id().clone(),
            dispatch_record_id: dispatch.journal_record_id().clone(),
            dispatch_sequence: dispatch.lifecycle_sequence().get(),
            durable_frontier_sha256: frontier_fingerprint(self.journal_frontier())?,
            runtime_config_fingerprint_sha256,
            authenticated_checkpoint_sha256: self
                .authenticated_checkpoint
                .checkpoint_sha256()
                .to_string(),
            request_state_fingerprint_sha256: replayed.state_fingerprint_sha256(),
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
            || (dispatch.previous_record_id() != Some(accepted.journal_record_id())
                && !stage8b_p1e_dispatch_follows_exact_schedule_binding(
                    self, accepted, identity, dispatch,
                ))
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
            accepted_record: accepted.clone(),
            accepted_record_id: accepted.journal_record_id().clone(),
            dispatch_record_id: dispatch.journal_record_id().clone(),
            dispatch_sequence: dispatch.lifecycle_sequence().get(),
            durable_frontier_sha256: frontier_fingerprint(self.journal_frontier())?,
            runtime_config_fingerprint_sha256,
            authenticated_checkpoint_sha256: self
                .authenticated_checkpoint
                .checkpoint_sha256()
                .to_string(),
            request_state_fingerprint_sha256: replayed.state_fingerprint_sha256(),
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

    pub fn stage8a4_pending_recovery_material(
        &self,
    ) -> Result<Option<Stage6Stage8a4PendingRecovery>, Stage6dLiveCoreError> {
        let mixed = Stage6MixedReplayEngineV2::replay(self.journal.versioned_records())?;
        let Some(batch) = mixed.reconciliation_batches().iter().find(|batch| {
            Some(batch.last_mixed_record_id()) == self.journal_frontier().last_record_id()
                && Some(batch.last_mixed_lifecycle_sequence())
                    == self.journal_frontier().last_lifecycle_sequence()
        }) else {
            return Ok(None);
        };
        let identity = batch.transition_record().durable_request_identity();
        let accepted = stage7a_accepted_record(self, identity.strategy_request_id())
            .ok_or(Stage6dLiveCoreError::AcceptedRecordRequired)?;
        let durable_command = match accepted.payload() {
            Stage6JournalPayloadV1::RequestAccepted { command } => command.as_ref().clone(),
            _ => return Err(Stage6dLiveCoreError::AcceptedRecordRequired),
        };
        self.authorize_stage8a4_durable_batch_source(identity, &durable_command)?;
        Ok(Some(Stage6Stage8a4PendingRecovery {
            transition_record: batch.transition_record().clone(),
            durable_command,
        }))
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

    #[doc(hidden)]
    pub fn stage8b_p1_reconstruction_candidate(
        &self,
    ) -> Result<HybridIntradayRuntimeStrategy, Stage6dLiveCoreError> {
        match &self.stage5_runtime {
            Stage6dStage5RuntimeAuthority::Restart(restart) => {
                Ok(restart.stage5g_fresh_reconstruction_candidate())
            }
            Stage6dStage5RuntimeAuthority::FirstBoot(_) => {
                Err(Stage6dLiveCoreError::RestartRuntimeRequired)
            }
        }
    }

    pub fn stage8b_p1_prepublication_material(
        &self,
    ) -> Option<(Stage6Stage8bP1SemanticCommitEvidenceV1, BrokerCommand)> {
        let restart = match &self.stage5_runtime {
            Stage6dStage5RuntimeAuthority::Restart(restart) => restart,
            Stage6dStage5RuntimeAuthority::FirstBoot(_) => return None,
        };
        let projection = restart.stage8b_p1_semantic_commit()?;
        if projection.intent_count != 1 {
            return None;
        }
        let request_id = projection.request_id?;
        let accepted = stage7a_accepted_record(self, request_id)?;
        let command = projection.canonical_command.as_ref()?.clone();
        Some((
            stage8b_p1_commit_evidence(projection, Some(accepted)),
            command,
        ))
    }

    /// Issues one opaque P1-d1 decision-bar binding from the authenticated P1
    /// semantic projection and its exact RequestAccepted record.  The M10
    /// Redis identity is the sole source of predecessor close time; callers
    /// cannot supply or override that timestamp independently.
    pub fn stage8b_p1d1_command_decision_binding(
        &self,
    ) -> Result<crate::Stage8bP1d1CommandDecisionBinding, Stage6dLiveCoreError> {
        let projection = match &self.stage5_runtime {
            Stage6dStage5RuntimeAuthority::Restart(restart) => restart
                .stage8b_p1_semantic_commit()
                .ok_or(Stage6dLiveCoreError::RestartRuntimeRequired)?,
            Stage6dStage5RuntimeAuthority::FirstBoot(_) => {
                return Err(Stage6dLiveCoreError::RestartRuntimeRequired)
            }
        };
        if !projection.validate() || projection.intent_count != 1 {
            return Err(Stage6dLiveCoreError::DurableOrderingViolation);
        }
        let request_id = projection
            .request_id
            .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?;
        let command = projection
            .canonical_command
            .as_ref()
            .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?
            .clone();
        let identity = projection
            .durable_request_identity
            .as_ref()
            .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?
            .clone();
        let snapshot = projection
            .durable_command_snapshot
            .as_ref()
            .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?
            .clone();
        let canonical_command_sha256 = projection
            .canonical_command_sha256
            .as_ref()
            .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?
            .clone();
        let accepted = stage7a_accepted_record(self, request_id)
            .ok_or(Stage6dLiveCoreError::AcceptedRecordRequired)?;
        let accepted_snapshot = match accepted.payload() {
            Stage6JournalPayloadV1::RequestAccepted { command } => command.as_ref(),
            _ => return Err(Stage6dLiveCoreError::AcceptedRecordRequired),
        };
        let replayed = self
            .replay()
            .request(request_id)
            .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?;
        let direct_accepted_frontier = replayed.last_unique_record_id()
            == accepted.journal_record_id()
            && self.journal_frontier().last_record_id() == Some(accepted.journal_record_id());
        let bound_market_frontier =
            stage8b_p1e_market_binding_tail(self, request_id, &canonical_command_sha256)
                .is_some_and(|record| {
                    replayed.last_unique_record_id() == record.journal_record_id()
                        && replayed.last_unique_sequence() == record.lifecycle_sequence().get()
                });
        if accepted.durable_request_identity() != &identity
            || accepted_snapshot != &snapshot
            || replayed.dispatch_safety_state()
                != crate::Stage6DispatchSafetyStateV1::ReadyForFirstDispatch
            || replayed.dispatch_attempt_count() != 0
            || (!direct_accepted_frontier && !bound_market_frontier)
        {
            return Err(Stage6dLiveCoreError::DurableOrderingViolation);
        }
        crate::stage8b_p1d1_paper_provider::stage8b_p1d1_command_decision_binding_from_source(
            command,
            identity,
            snapshot,
            accepted.canonical_payload_sha256().clone(),
            projection.operational_identity_sha256.clone(),
            canonical_command_sha256,
            projection.m10_redis_id.clone(),
            projection.m10_semantic_id_sha256.clone(),
            projection.m10_payload_sha256.clone(),
        )
        .map_err(|_| Stage6dLiveCoreError::DurableOrderingViolation)
    }

    /// Recovery-only reconstruction of the same decision binding after the
    /// request has become dispatch-forbidden. The caller must separately
    /// prove the exact P1-d2 journal-ahead shape; this method grants neither
    /// dispatch nor provider authority.
    fn stage8b_p1d2_recovery_command_decision_binding(
        &self,
    ) -> Result<crate::Stage8bP1d1CommandDecisionBinding, Stage6dLiveCoreError> {
        let projection = match &self.stage5_runtime {
            Stage6dStage5RuntimeAuthority::Restart(restart) => restart
                .stage8b_p1_semantic_commit()
                .ok_or(Stage6dLiveCoreError::RestartRuntimeRequired)?,
            Stage6dStage5RuntimeAuthority::FirstBoot(_) => {
                return Err(Stage6dLiveCoreError::RestartRuntimeRequired)
            }
        };
        if !projection.validate() || projection.intent_count != 1 {
            return Err(Stage6dLiveCoreError::DurableOrderingViolation);
        }
        let request_id = projection
            .request_id
            .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?;
        let command = projection
            .canonical_command
            .as_ref()
            .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?
            .clone();
        let identity = projection
            .durable_request_identity
            .as_ref()
            .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?
            .clone();
        let snapshot = projection
            .durable_command_snapshot
            .as_ref()
            .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?
            .clone();
        let canonical_command_sha256 = projection
            .canonical_command_sha256
            .as_ref()
            .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?
            .clone();
        let accepted = stage7a_accepted_record(self, request_id)
            .ok_or(Stage6dLiveCoreError::AcceptedRecordRequired)?;
        let accepted_snapshot = match accepted.payload() {
            Stage6JournalPayloadV1::RequestAccepted { command } => command.as_ref(),
            _ => return Err(Stage6dLiveCoreError::AcceptedRecordRequired),
        };
        if accepted.durable_request_identity() != &identity || accepted_snapshot != &snapshot {
            return Err(Stage6dLiveCoreError::DurableOrderingViolation);
        }
        crate::stage8b_p1d1_paper_provider::stage8b_p1d1_command_decision_binding_from_source(
            command,
            identity,
            snapshot,
            accepted.canonical_payload_sha256().clone(),
            projection.operational_identity_sha256.clone(),
            canonical_command_sha256,
            projection.m10_redis_id.clone(),
            projection.m10_semantic_id_sha256.clone(),
            projection.m10_payload_sha256.clone(),
        )
        .map_err(|_| Stage6dLiveCoreError::DurableOrderingViolation)
    }

    /// Converts one Redis-validated canonical successor M10 into the only
    /// P1-d1 eligibility capability. The read-only evidence is insufficient
    /// without this owned authenticated Stage 6 authority.
    pub fn stage8b_p1d1_execution_eligibility(
        &self,
        schedule_authority: crate::Stage8bP1d1ExecutionScheduleAuthority,
        evidence: crate::Stage8bP1d1CanonicalM10Evidence,
    ) -> Result<crate::Stage8bP1d1ExecutionEligible, Stage6dLiveCoreError> {
        let decision = self.stage8b_p1d1_command_decision_binding()?;
        let binding = stage8b_p1e_market_binding_tail(
            self,
            decision.strategy_request_id(),
            decision.canonical_command_sha256(),
        );
        match binding {
            Some(record) if schedule_authority.matches_stage8b_p1e_v4_record(record, &evidence) => {
            }
            Some(_) => return Err(Stage6dLiveCoreError::DurableOrderingViolation),
            None if schedule_authority.has_stage8b_p1e_v4_proof() => {
                return Err(Stage6dLiveCoreError::DurableOrderingViolation)
            }
            None => {}
        }
        crate::stage8b_p1d1_paper_provider::stage8b_p1d1_eligible_from_canonical_m10(
            decision,
            schedule_authority,
            evidence,
        )
        .map_err(|_| Stage6dLiveCoreError::DurableOrderingViolation)
    }

    /// Redacted evidence for a durable zero-intent P1 semantic commit whose
    /// source acknowledgement must be resolved before semantic continuation.
    pub fn stage8b_p1_zero_intent_ack_evidence(
        &self,
    ) -> Option<Stage6Stage8bP1SemanticCommitEvidenceV1> {
        let restart = match &self.stage5_runtime {
            Stage6dStage5RuntimeAuthority::Restart(restart) => restart,
            Stage6dStage5RuntimeAuthority::FirstBoot(_) => return None,
        };
        let projection = restart.stage8b_p1_semantic_commit()?;
        if projection.intent_count != 0 {
            return None;
        }
        Some(stage8b_p1_commit_evidence(projection, None))
    }

    /// Review evidence only; this grants no callback or continuation method.
    pub fn stage8b_p1_stage5c_callback_count(&self) -> Option<usize> {
        let restart = match &self.stage5_runtime {
            Stage6dStage5RuntimeAuthority::Restart(restart) => restart,
            Stage6dStage5RuntimeAuthority::FirstBoot(_) => return None,
        };
        restart
            .stage8b_p1_semantic_commit()
            .map(|_| restart.summary().stage5c_callback_count)
    }

    /// Produces only immutable counters from the already authenticated
    /// replay.  It is intentionally compiled solely for artifact fixtures so
    /// the P1-d4 evidence runner observes journal facts instead of deriving
    /// them from the matrix expectations.
    #[cfg(any(test, feature = "stage5g-artifact-fixtures"))]
    #[doc(hidden)]
    pub fn stage8b_p1d4_test_runtime_audit(&self) -> Stage8bP1d4RuntimeAuditV1 {
        let mut dispatch_v1_total = 0;
        let mut order_v1_total = 0;
        let mut trade_v1_total = 0;
        let mut request_finalized_v1_total = 0;
        let mut durable_outcomes = 0;
        let mut truth_bearing_outcomes = 0;
        let mut sequence_allocations = Vec::new();
        let journal_lifecycle_sequences = self
            .journal
            .versioned_records()
            .iter()
            .map(|record| record.lifecycle_sequence().get())
            .collect::<Vec<_>>();
        // Prefer the request authenticated by the currently embedded
        // semantic package.  In GM00 the reservation exists before the first
        // V1 row, so choosing the latest journal request would accidentally
        // audit the completed predecessor LIMIT instead of the generated
        // Market request.  Non-command/legacy phases retain the journal
        // fallback.
        let audited_request_id = match &self.stage5_runtime {
            Stage6dStage5RuntimeAuthority::Restart(restart) => restart
                .stage8b_p1_semantic_commit()
                .and_then(|projection| projection.request_id),
            Stage6dStage5RuntimeAuthority::FirstBoot(_) => None,
        }
        .or_else(|| {
            self.replay
                .requests()
                .iter()
                .max_by_key(|request| request.last_unique_sequence())
                .map(Stage6RecoveredRequestV1::strategy_request_id)
        });
        for (journal_record_index, record) in self.journal.versioned_records().iter().enumerate() {
            match record {
                Stage6JournalRecordVersioned::V1(record)
                    if Some(record.durable_request_identity().strategy_request_id())
                        == audited_request_id =>
                {
                    match record.event_kind() {
                        Stage6JournalEventKind::DispatchAttemptRecorded => dispatch_v1_total += 1,
                        Stage6JournalEventKind::BrokerOrderObserved => order_v1_total += 1,
                        Stage6JournalEventKind::BrokerTradeObserved => trade_v1_total += 1,
                        Stage6JournalEventKind::RequestFinalized => request_finalized_v1_total += 1,
                        _ => {}
                    }
                }
                Stage6JournalRecordVersioned::V1(_) => {}
                Stage6JournalRecordVersioned::V3(record) => {
                    let evidence = crate::stage8b_p1d3_working_limit::Stage8bP1d3OutcomeEvidenceV1::decode_canonical(
                        record.outcome_evidence_bytes(),
                    )
                    .expect("validated V3 journal rows retain canonical P1-d3 outcome evidence");
                    let (seq_ack, seq_truth) = evidence.reserved_sequences();
                    durable_outcomes += 1;
                    truth_bearing_outcomes += usize::from(seq_truth.is_some());
                    sequence_allocations.push(Stage8bP1d4SequenceAllocationAuditV1 {
                        outcome_kind: evidence.outcome_kind().canonical_name().to_string(),
                        journal_record_index,
                        stage6_lifecycle_sequence: record.lifecycle_sequence().get(),
                        sequence_allocation_frontier: evidence.sequence_allocation_frontier(),
                        seq_ack,
                        seq_truth,
                    });
                }
                Stage6JournalRecordVersioned::V2(_) | Stage6JournalRecordVersioned::V4(_) => {}
            }
        }
        Stage8bP1d4RuntimeAuditV1 {
            lifecycle_sequence: self
                .journal_frontier()
                .last_lifecycle_sequence()
                .map_or(0, Stage6LifecycleSequence::get),
            journal_lifecycle_sequences,
            sequence_pair: match &self.stage5_runtime {
                Stage6dStage5RuntimeAuthority::Restart(restart) => {
                    restart.stage8b_p1d4_test_sequence_pair()
                }
                Stage6dStage5RuntimeAuthority::FirstBoot(_) => None,
            },
            sequence_allocations,
            package: match &self.stage5_runtime {
                Stage6dStage5RuntimeAuthority::Restart(restart) => {
                    let p1d3_phase = restart.stage8b_p1d3_replacement().map(|replacement| {
                        match replacement.phase() {
                            crate::stage8b_p1d3_working_limit::Stage8bP1d3BookPhase::Migrated => "Migrated",
                            crate::stage8b_p1d3_working_limit::Stage8bP1d3BookPhase::Ack => "Ack",
                            crate::stage8b_p1d3_working_limit::Stage8bP1d3BookPhase::Working => "Working",
                            crate::stage8b_p1d3_working_limit::Stage8bP1d3BookPhase::Eval => "Eval",
                            crate::stage8b_p1d3_working_limit::Stage8bP1d3BookPhase::Terminal => "Terminal",
                            crate::stage8b_p1d3_working_limit::Stage8bP1d3BookPhase::CancelRecovered => "CancelRecovered",
                        }
                        .to_string()
                    });
                    let generated_market_phase = restart
                        .stage8b_p1d4_generated_market_package_state()
                        .ok()
                        .flatten()
                        .map(|phase| {
                            match phase {
                                crate::Stage8bP1d4GeneratedMarketPackageState::Prepublication {
                                    ..
                                } => "Prepublication",
                                crate::Stage8bP1d4GeneratedMarketPackageState::AckCommitted {
                                    ..
                                } => "AckCommitted",
                                crate::Stage8bP1d4GeneratedMarketPackageState::TruthCommitted {
                                    ..
                                } => "TruthCommitted",
                            }
                            .to_string()
                        });
                    Stage8bP1d4PackageAuditV1 {
                        write_generation: Some(restart.stage8b_p1d4_test_write_generation()),
                        p1d3_phase,
                        generated_market_phase,
                    }
                }
                Stage6dStage5RuntimeAuthority::FirstBoot(_) => Stage8bP1d4PackageAuditV1 {
                    write_generation: None,
                    p1d3_phase: None,
                    generated_market_phase: None,
                },
            },
            callback_count: match &self.stage5_runtime {
                Stage6dStage5RuntimeAuthority::Restart(restart) => restart
                    .summary()
                    .stage5c_callback_count
                    .max(usize::from(restart.stage8b_p1_semantic_commit().is_some())),
                Stage6dStage5RuntimeAuthority::FirstBoot(_) => 0,
            },
            dispatch_v1_total,
            order_v1_total,
            trade_v1_total,
            request_finalized_v1_total,
            durable_outcomes,
            truth_bearing_outcomes,
        }
    }

    /// True only when the authenticated embedded Stage 5G package is the
    /// P1-d2 ACK-stage replacement package.  A caller cannot infer this phase
    /// from the outer Stage 7 seal generation alone.
    pub fn stage8b_p1d2_ack_frontier_is_authenticated(&self) -> bool {
        matches!(
            &self.stage5_runtime,
            Stage6dStage5RuntimeAuthority::Restart(restart)
                if restart.stage8b_p1d2_validate_ack_frontier().is_ok()
        )
    }

    /// True only when the authenticated embedded Stage 5G package is the
    /// P1-d2 truth-stage replacement package.  This is the sole durable phase
    /// from which source acknowledgement may later be resumed.
    pub fn stage8b_p1d2_truth_frontier_is_authenticated(&self) -> bool {
        matches!(
            &self.stage5_runtime,
            Stage6dStage5RuntimeAuthority::Restart(restart)
                if restart.stage8b_p1d2_validate_truth_frontier().is_ok()
        )
    }

    pub fn stage8b_p1d2_source_m10_binding(&self) -> Option<Stage6Stage8bP1d2SourceM10Binding> {
        let restart = match &self.stage5_runtime {
            Stage6dStage5RuntimeAuthority::Restart(restart) => restart,
            Stage6dStage5RuntimeAuthority::FirstBoot(_) => return None,
        };
        let (redis_id, semantic_id_sha256, payload_sha256) =
            restart.stage8b_p1d2_source_m10_binding()?;
        Some(Stage6Stage8bP1d2SourceM10Binding {
            redis_id: redis_id.to_string(),
            semantic_id_sha256: semantic_id_sha256.to_string(),
            payload_sha256: payload_sha256.to_string(),
        })
    }

    /// Redacted audit facts are available only from an authenticated final
    /// S_truth package. They grant no lifecycle or source-resolution action.
    pub fn stage8b_p1d2_feedback_audit_core(
        &self,
    ) -> Option<crate::Stage8bP1d2FeedbackAuditCoreV1> {
        let restart = match &self.stage5_runtime {
            Stage6dStage5RuntimeAuthority::Restart(restart) => restart,
            Stage6dStage5RuntimeAuthority::FirstBoot(_) => return None,
        };
        restart.stage8b_p1d2_feedback_audit_core()
    }

    /// Redacted audit facts for a generated-Market truth package. This is
    /// deliberately distinct from the standalone P1-d2 accessor because the
    /// embedded order-position state is validated by the P1-d4 matcher.
    pub fn stage8b_p1d4_feedback_audit_core(
        &self,
    ) -> Option<crate::Stage8bP1d2FeedbackAuditCoreV1> {
        let restart = match &self.stage5_runtime {
            Stage6dStage5RuntimeAuthority::Restart(restart) => restart,
            Stage6dStage5RuntimeAuthority::FirstBoot(_) => return None,
        };
        restart.stage8b_p1d4_feedback_audit_core()
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

#[derive(Serialize)]
struct Stage8bP1RequestAcceptedBindingV1<'a> {
    schema_version: u16,
    domain: &'static str,
    source_seal_generation: u64,
    source_seal_commitment_sha256: &'a str,
    source_stage6_checkpoint_sha256: &'a str,
    source_stage6_frontier_sha256: &'a str,
    operational_identity_sha256: &'a str,
    m10_redis_id: &'a str,
    m10_semantic_id_sha256: &'a str,
    m10_payload_sha256: &'a str,
    semantic_batch_id_sha256: &'a str,
    strategy_request_id: StrategyRequestId,
    canonical_command_sha256: &'a str,
    expected_request_accepted_record_id: &'a str,
}

/// Consumes the sole recovered runtime owner and applies exactly one P1 M10
/// semantic transition.  It never appends DispatchAttemptRecorded and never
/// owns Redis, provider or network operations.
pub fn apply_stage8b_p1_semantic_transition(
    mut recovered: Stage6dDurableRuntimeRecovered,
    accepted_bar: crate::Stage5cAcceptedSemanticBar,
    binding: Stage5gP1SemanticBindingInput,
    source: Stage6Stage8bP1SealSourceV1,
    commitment_key: &Stage5gLifecycleCommitmentKey,
) -> Result<Stage6Stage8bP1SemanticTransition, Stage6dLiveCoreError> {
    let operational_identity = recovered
        .authenticated_operational_identity()
        .ok_or(Stage6dLiveCoreError::RestartRuntimeRequired)?;
    let operational_identity_sha256 = stage6d_operational_identity_sha256(operational_identity)?;
    if source.seal_generation == 0
        || Stage6Sha256Digest::parse(source.seal_commitment_sha256.clone()).is_err()
        || source.stage6_checkpoint_sha256
            != recovered.authenticated_checkpoint().checkpoint_sha256()
        || source.stage6_frontier_sha256 != frontier_fingerprint(recovered.journal_frontier())?
        || source.operational_identity_sha256 != operational_identity_sha256.as_str()
        || binding.operational_identity_sha256 != source.operational_identity_sha256
    {
        return Err(Stage6dLiveCoreError::RestartPackageBindingMismatch);
    }

    let (placeholder_runtime, reconstruction_runtime, prior_stage5_checkpoint_sha256) = {
        let current = match &recovered.stage5_runtime {
            Stage6dStage5RuntimeAuthority::Restart(current)
                if current.lifecycle_kind()
                    == crate::Stage5gCleanRestartLifecycleKind::P1SemanticReady =>
            {
                current
            }
            _ => return Err(Stage6dLiveCoreError::RestartRuntimeRequired),
        };
        (
            current.stage5g_fresh_reconstruction_candidate(),
            current.stage5g_fresh_reconstruction_candidate(),
            sha256_hex(
                &serde_json::to_vec(current.checkpoint())
                    .map_err(|_| Stage6dLiveCoreError::IntegrationFingerprint)?,
            ),
        )
    };
    let Stage6dStage5RuntimeAuthority::Restart(current) = std::mem::replace(
        &mut recovered.stage5_runtime,
        Stage6dStage5RuntimeAuthority::FirstBoot(Box::new(placeholder_runtime)),
    ) else {
        unreachable!("P1 source lifecycle was checked above")
    };

    let transition = continue_stage5g_p1_semantic(*current, accepted_bar, binding)
        .map_err(|_| Stage6dLiveCoreError::DurableOrderingViolation)?;
    match transition {
        Stage5gP1SemanticTransition::ZeroIntent(value) => {
            let projection = p1_projection_from_zero(&value).clone();
            validate_stage8b_p1_projection_source(
                &projection,
                &source,
                &prior_stage5_checkpoint_sha256,
                0,
            )?;
            let stage5g_restart_package = export_stage5g_p1_zero_intent(value, commitment_key)
                .map_err(|_| Stage6dLiveCoreError::DurableOrderingViolation)?;
            let restored = restore_stage5g_clean_restart(
                &stage5g_restart_package,
                commitment_key,
                reconstruction_runtime,
            )?;
            if restored.stage8b_p1_semantic_commit() != Some(&projection) {
                return Err(Stage6dLiveCoreError::RestartPackageBindingMismatch);
            }
            recovered.stage5_runtime = Stage6dStage5RuntimeAuthority::Restart(Box::new(restored));
            recovered.refresh_after_append()?;
            Ok(Stage6Stage8bP1SemanticTransition::ZeroIntent {
                recovered,
                stage5g_restart_package,
                evidence: stage8b_p1_commit_evidence(&projection, None),
            })
        }
        Stage5gP1SemanticTransition::OneIntent(value) => {
            let projection = p1_projection_from_one(&value).clone();
            validate_stage8b_p1_projection_source(
                &projection,
                &source,
                &prior_stage5_checkpoint_sha256,
                1,
            )?;
            let identity = projection
                .durable_request_identity
                .as_ref()
                .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?
                .clone();
            let command_snapshot = projection
                .durable_command_snapshot
                .as_ref()
                .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?
                .clone();
            if stage7a_accepted_record(&recovered, identity.strategy_request_id()).is_some()
                || stage7a_has_other_unresolved_lifecycle(&recovered, &identity)
            {
                return Err(Stage6dLiveCoreError::DurableOrderingViolation);
            }
            let sequence = Stage6LifecycleSequence::new(1)?;
            let expected_record_id =
                Stage6JournalRecordId::derive(identity.strategy_request_id(), sequence);
            let source_evidence = stage8b_p1_request_accepted_source_evidence(
                &projection,
                &source,
                &expected_record_id,
            )?;
            let accepted = Stage6JournalRecordV1::request_accepted(
                identity.clone(),
                command_snapshot.clone(),
                sequence,
                None,
                None,
                source_evidence,
            )?;
            if accepted.journal_record_id() != &expected_record_id {
                return Err(Stage6dLiveCoreError::DurableOrderingViolation);
            }

            // Package validation is completed before the first durable write.
            let stage5g_restart_package = export_stage5g_p1_one_intent(value, commitment_key)
                .map_err(|_| Stage6dLiveCoreError::DurableOrderingViolation)?;
            let restored = restore_stage5g_clean_restart(
                &stage5g_restart_package,
                commitment_key,
                reconstruction_runtime,
            )?;
            if restored.stage8b_p1_semantic_commit() != Some(&projection) {
                return Err(Stage6dLiveCoreError::RestartPackageBindingMismatch);
            }

            recovered
                .journal_mut()
                .append(&accepted)
                .map_err(classify_stage8a4_append_error)?;
            recovered.stage5_runtime = Stage6dStage5RuntimeAuthority::Restart(Box::new(restored));
            recovered
                .refresh_after_append()
                .map_err(|_| Stage6dLiveCoreError::JournalMutationMayHaveOccurred)?;
            let command = projection
                .canonical_command
                .as_ref()
                .expect("validated P1 one-intent projection has command")
                .clone();
            let evidence = stage8b_p1_commit_evidence(&projection, Some(&accepted));
            Ok(Stage6Stage8bP1SemanticTransition::OneIntentPrepublication {
                recovered: Box::new(recovered),
                stage5g_restart_package,
                evidence,
                command,
                durable_request_identity: identity,
                durable_command_snapshot: command_snapshot,
            })
        }
        Stage5gP1SemanticTransition::MultiIntentBlocked(blocked) => {
            Ok(Stage6Stage8bP1SemanticTransition::MultiIntentBlocked {
                semantic_batch_id_sha256: blocked.semantic_batch_id_sha256().to_string(),
                intent_count: blocked.intent_count(),
                request_ids: blocked.request_ids().to_vec(),
            })
        }
    }
}

/// P1-d3 variant of the accepted semantic transition.  It differs only in
/// ownership: the authenticated working-book replacement is retained in the
/// exported Stage 5G package while the same Stage 5C Hybrid callback and
/// Stage 6 RequestAccepted chain are reused.
pub fn apply_stage8b_p1d3_semantic_transition(
    mut recovered: Stage6dDurableRuntimeRecovered,
    accepted_bar: crate::Stage5cAcceptedSemanticBar,
    binding: Stage5gP1SemanticBindingInput,
    source: Stage6Stage8bP1SealSourceV1,
    tick_size: f64,
    commitment_key: &Stage5gLifecycleCommitmentKey,
) -> Result<Stage6Stage8bP1SemanticTransition, Stage6dLiveCoreError> {
    let operational_identity = recovered
        .authenticated_operational_identity()
        .ok_or(Stage6dLiveCoreError::RestartRuntimeRequired)?;
    let operational_identity_sha256 = stage6d_operational_identity_sha256(operational_identity)?;
    if source.seal_generation == 0
        || Stage6Sha256Digest::parse(source.seal_commitment_sha256.clone()).is_err()
        || source.stage6_checkpoint_sha256
            != recovered.authenticated_checkpoint().checkpoint_sha256()
        || source.stage6_frontier_sha256 != frontier_fingerprint(recovered.journal_frontier())?
        || source.operational_identity_sha256 != operational_identity_sha256.as_str()
        || binding.operational_identity_sha256 != source.operational_identity_sha256
    {
        return Err(Stage6dLiveCoreError::RestartPackageBindingMismatch);
    }

    let (placeholder_runtime, reconstruction_runtime, prior_stage5_checkpoint_sha256) = {
        let current = match &recovered.stage5_runtime {
            Stage6dStage5RuntimeAuthority::Restart(current)
                if current.stage8b_p1d3_replacement().is_some() =>
            {
                current
            }
            _ => return Err(Stage6dLiveCoreError::RestartRuntimeRequired),
        };
        let expected_semantic_source = current
            .stage8b_p1d3_replacement()
            .and_then(|replacement| replacement.pending_semantic_source_binding().ok().flatten())
            .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?;
        if expected_semantic_source.redis_id() != binding.m10_redis_id
            || expected_semantic_source.semantic_id_sha256() != binding.m10_semantic_id_sha256
            || expected_semantic_source.payload_sha256() != binding.m10_payload_sha256
            || current
                .stage8b_p1_semantic_commit()
                .is_some_and(|semantic| expected_semantic_source.matches_semantic_commit(semantic))
        {
            return Err(Stage6dLiveCoreError::DurableOrderingViolation);
        }
        (
            current.stage5g_fresh_reconstruction_candidate(),
            current.stage5g_fresh_reconstruction_candidate(),
            sha256_hex(
                &serde_json::to_vec(current.checkpoint())
                    .map_err(|_| Stage6dLiveCoreError::IntegrationFingerprint)?,
            ),
        )
    };
    let Stage6dStage5RuntimeAuthority::Restart(current) = std::mem::replace(
        &mut recovered.stage5_runtime,
        Stage6dStage5RuntimeAuthority::FirstBoot(Box::new(placeholder_runtime)),
    ) else {
        unreachable!("P1-d3 source lifecycle was checked above")
    };

    let transition = continue_stage8b_p1d3_semantic(*current, accepted_bar, binding, tick_size)
        .map_err(|_| Stage6dLiveCoreError::DurableOrderingViolation)?;
    match transition {
        Stage8bP1d3SemanticTransition::ZeroIntent(value) => {
            let projection = p1d3_projection_from_zero(&value).clone();
            validate_stage8b_p1_projection_source(
                &projection,
                &source,
                &prior_stage5_checkpoint_sha256,
                0,
            )?;
            let stage5g_restart_package = export_stage8b_p1d3_zero_intent(value, commitment_key)
                .map_err(|_| Stage6dLiveCoreError::DurableOrderingViolation)?;
            let restored = restore_stage5g_clean_restart(
                &stage5g_restart_package,
                commitment_key,
                reconstruction_runtime,
            )?;
            if restored.stage8b_p1_semantic_commit() != Some(&projection)
                || restored.stage8b_p1d3_replacement().is_none()
            {
                return Err(Stage6dLiveCoreError::RestartPackageBindingMismatch);
            }
            recovered.stage5_runtime = Stage6dStage5RuntimeAuthority::Restart(Box::new(restored));
            recovered.refresh_after_append()?;
            Ok(Stage6Stage8bP1SemanticTransition::ZeroIntent {
                recovered,
                stage5g_restart_package,
                evidence: stage8b_p1_commit_evidence(&projection, None),
            })
        }
        Stage8bP1d3SemanticTransition::OneIntent(value) => {
            let projection = p1d3_projection_from_one(&value).clone();
            validate_stage8b_p1_projection_source(
                &projection,
                &source,
                &prior_stage5_checkpoint_sha256,
                1,
            )?;
            let identity = projection
                .durable_request_identity
                .as_ref()
                .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?
                .clone();
            let command_snapshot = projection
                .durable_command_snapshot
                .as_ref()
                .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?
                .clone();
            if stage7a_accepted_record(&recovered, identity.strategy_request_id()).is_some()
                || stage7a_has_other_unresolved_lifecycle(&recovered, &identity)
            {
                return Err(Stage6dLiveCoreError::DurableOrderingViolation);
            }
            let sequence = Stage6LifecycleSequence::new(1)?;
            let expected_record_id =
                Stage6JournalRecordId::derive(identity.strategy_request_id(), sequence);
            let source_evidence = stage8b_p1_request_accepted_source_evidence(
                &projection,
                &source,
                &expected_record_id,
            )?;
            let accepted = Stage6JournalRecordV1::request_accepted(
                identity.clone(),
                command_snapshot.clone(),
                sequence,
                None,
                None,
                source_evidence,
            )?;
            if accepted.journal_record_id() != &expected_record_id {
                return Err(Stage6dLiveCoreError::DurableOrderingViolation);
            }

            let pre_request_checkpoint_sha256 = recovered
                .authenticated_checkpoint()
                .checkpoint_sha256()
                .to_string();
            let post_request_checkpoint_sha256 =
                projected_checkpoint_after_append(&recovered, &accepted)?;
            let value = rebind_stage8b_p1d3_one_intent_request_checkpoint(
                value,
                &pre_request_checkpoint_sha256,
                post_request_checkpoint_sha256.clone(),
            )
            .map_err(|_| Stage6dLiveCoreError::RestartPackageBindingMismatch)?;

            let stage5g_restart_package = export_stage8b_p1d3_one_intent(value, commitment_key)
                .map_err(|_| Stage6dLiveCoreError::DurableOrderingViolation)?;
            let restored = restore_stage5g_clean_restart(
                &stage5g_restart_package,
                commitment_key,
                reconstruction_runtime,
            )?;
            if restored.stage8b_p1_semantic_commit() != Some(&projection)
                || restored.stage8b_p1d3_replacement().is_none()
            {
                return Err(Stage6dLiveCoreError::RestartPackageBindingMismatch);
            }

            recovered
                .journal_mut()
                .append(&accepted)
                .map_err(classify_stage8a4_append_error)?;
            recovered.stage5_runtime = Stage6dStage5RuntimeAuthority::Restart(Box::new(restored));
            recovered
                .refresh_after_append()
                .map_err(|_| Stage6dLiveCoreError::JournalMutationMayHaveOccurred)?;
            if recovered.authenticated_checkpoint().checkpoint_sha256()
                != post_request_checkpoint_sha256
            {
                return Err(Stage6dLiveCoreError::JournalMutationMayHaveOccurred);
            }
            let command = projection
                .canonical_command
                .as_ref()
                .expect("validated P1-d3 one-intent projection has command")
                .clone();
            let evidence = stage8b_p1_commit_evidence(&projection, Some(&accepted));
            Ok(Stage6Stage8bP1SemanticTransition::OneIntentPrepublication {
                recovered: Box::new(recovered),
                stage5g_restart_package,
                evidence,
                command,
                durable_request_identity: identity,
                durable_command_snapshot: command_snapshot,
            })
        }
        Stage8bP1d3SemanticTransition::MultiIntentBlocked(blocked) => {
            Ok(Stage6Stage8bP1SemanticTransition::MultiIntentBlocked {
                semantic_batch_id_sha256: blocked.semantic_batch_id_sha256().to_string(),
                intent_count: blocked.intent_count(),
                request_ids: blocked.request_ids().to_vec(),
            })
        }
    }
}

/// Replaces a generated-Market P1-d3 semantic package with the exact
/// reservation-bearing P1-d4 Prepublication package. This transition performs
/// no journal mutation, Redis operation, provider call, ACK, truth application
/// or source acknowledgement.
pub fn apply_stage8b_p1d4_publication_reservation(
    mut recovered: Stage6dDurableRuntimeRecovered,
    reservation: crate::Stage8bP1d4CommandPublicationReservationV1,
    commitment_key: &Stage5gLifecycleCommitmentKey,
) -> Result<Stage6Stage8bP1d4PrepublicationTransition, Stage6dLiveCoreError> {
    if !recovered.stage8b_p1d4_generated_market_candidate()
        || recovered.stage8b_p1d4_next_write_generation()
            != Some(reservation.prepublication_package_generation())
    {
        return Err(Stage6dLiveCoreError::RestartPackageBindingMismatch);
    }
    let pre_checkpoint = recovered.authenticated_checkpoint().clone();
    let reconstruction_runtime = match &recovered.stage5_runtime {
        Stage6dStage5RuntimeAuthority::Restart(restart) => {
            restart.stage5g_fresh_reconstruction_candidate()
        }
        Stage6dStage5RuntimeAuthority::FirstBoot(_) => {
            return Err(Stage6dLiveCoreError::RestartRuntimeRequired)
        }
    };
    let placeholder_runtime = reconstruction_runtime.stage5g_clean_reconstruction_candidate();
    let Stage6dStage5RuntimeAuthority::Restart(restart) = std::mem::replace(
        &mut recovered.stage5_runtime,
        Stage6dStage5RuntimeAuthority::FirstBoot(Box::new(placeholder_runtime)),
    ) else {
        unreachable!("P1-d4 generated-Market source was checked above")
    };
    let (runtime, state, p1d3, semantic, export_input) = restart
        .into_stage8b_p1d4_prepublication_parts(&reservation)
        .map_err(Stage6dLiveCoreError::from)?;
    let composition =
        crate::stage8b_p1d4_generated_market::Stage8bP1d4GeneratedMarketCompositionV1::prepublication(
            reservation,
        )
        .map_err(|_| Stage6dLiveCoreError::RestartPackageBindingMismatch)?;
    let source = crate::stage8b_p1d4_generated_market::Stage8bP1d4RestartSource::new(
        runtime,
        state,
        p1d3,
        semantic,
        None,
        composition,
    )
    .map_err(|_| Stage6dLiveCoreError::RestartPackageBindingMismatch)?;
    let stage5g_restart_package = export_stage5g_clean_restart(
        crate::Stage5gCleanRestartSource::P1d4(Box::new(source)),
        export_input,
        commitment_key,
    )?;
    let restored = restore_stage5g_clean_restart(
        &stage5g_restart_package,
        commitment_key,
        reconstruction_runtime,
    )?;
    if restored
        .stage8b_p1d4_generated_market_composition()
        .is_none()
    {
        return Err(Stage6dLiveCoreError::RestartPackageBindingMismatch);
    }
    recovered.stage5_runtime = Stage6dStage5RuntimeAuthority::Restart(Box::new(restored));
    recovered.refresh_after_append()?;
    if recovered.authenticated_checkpoint() != &pre_checkpoint {
        return Err(Stage6dLiveCoreError::DurableOrderingViolation);
    }
    Ok(Stage6Stage8bP1d4PrepublicationTransition {
        recovered,
        stage5g_restart_package,
    })
}

/// Applies the generated-Market V1 order/trade/finalization chain and builds
/// the combined S_ack package. The original reservation-bearing package must
/// still be the authenticated runtime authority and the publication binding
/// must name its exact covering seal.
pub fn apply_stage8b_p1d4_ack_transition(
    mut recovered: Stage6dDurableRuntimeRecovered,
    outcome_bundle: crate::Stage8bP1d1MarketOutcomeBundle,
    publication_binding: crate::Stage8bP1d4CommandPublicationBindingV1,
    commitment_key: &Stage5gLifecycleCommitmentKey,
) -> Result<Stage6Stage8bP1d4AckTransition, Stage6dLiveCoreError> {
    let (dispatch_receipt, outcome, evidence) = outcome_bundle.into_p1d2_parts();
    let receipt_ts = Utc
        .timestamp_millis_opt(evidence.fill_received_ts_utc_ms)
        .single()
        .ok_or(Stage6dLiveCoreError::Stage8bP1d2MarketFeedback)?;
    let prepublication_checkpoint_sha256 = match &recovered.stage5_runtime {
        Stage6dStage5RuntimeAuthority::Restart(restart) => restart
            .stage8b_p1d3_replacement()
            .ok_or(Stage6dLiveCoreError::RestartPackageBindingMismatch)?
            .authenticated_stage6_checkpoint_sha256()
            .to_string(),
        Stage6dStage5RuntimeAuthority::FirstBoot(_) => {
            return Err(Stage6dLiveCoreError::RestartRuntimeRequired)
        }
    };
    let report = execute_stage6d_paper_outcome(&mut recovered, dispatch_receipt, outcome)?;
    stage8b_p1d2_test_crash_barrier("p1d2-after-stage6-before-request-finalized");
    let report = finalize_stage7a_paper_request(&mut recovered, report, receipt_ts)?;
    stage8b_p1d4_test_crash_frontier("GM07");
    let facts = stage7b_finalized_request_facts(&recovered, report.strategy_request_id)?;
    if report.strategy_request_id != facts.strategy_request_id()
        || report.durable_client_order_id != *facts.durable_client_order_id()
        || report.account_id != *facts.durable_request_identity().account_id()
        || report.instrument != *facts.durable_request_identity().instrument()
        || report.attribution != *facts.durable_request_identity().attribution()
        || report.action != facts.durable_request_identity().action()
        || report.broker_order_id.as_ref() != facts.broker_order_id()
        || report.broker_trade_ids != facts.broker_trade_ids()
        || report.final_disposition != Some(facts.final_disposition())
        || report.final_record_id != facts.final_record_id().as_str()
        || report.final_sequence != facts.final_sequence()
    {
        return Err(Stage6dLiveCoreError::Stage8bP1d2MarketFeedback);
    }
    stage8b_p1d2_test_crash_barrier("p1d2-after-request-finalized-before-ack");
    complete_stage8b_p1d4_ack_transition(
        recovered,
        facts,
        evidence,
        publication_binding,
        &prepublication_checkpoint_sha256,
        receipt_ts,
        commitment_key,
    )
}

#[allow(clippy::too_many_arguments)]
fn complete_stage8b_p1d4_ack_transition(
    mut recovered: Stage6dDurableRuntimeRecovered,
    facts: Stage7bFinalizedRequestFacts,
    evidence: crate::stage8b_p1d1_paper_provider::Stage8bP1d1MarketOutcomeEvidence,
    publication_binding: crate::Stage8bP1d4CommandPublicationBindingV1,
    prepublication_checkpoint_sha256: &str,
    receipt_ts: DateTime<Utc>,
    commitment_key: &Stage5gLifecycleCommitmentKey,
) -> Result<Stage6Stage8bP1d4AckTransition, Stage6dLiveCoreError> {
    let replacement_runtime = match &recovered.stage5_runtime {
        Stage6dStage5RuntimeAuthority::Restart(restart) => {
            let composition = restart
                .stage8b_p1d4_generated_market_composition()
                .ok_or(Stage6dLiveCoreError::RestartPackageBindingMismatch)?;
            if composition.phase()
                != crate::stage8b_p1d4_generated_market::Stage8bP1d4GeneratedMarketPhase::Prepublication
                || composition.binding().is_some()
                || publication_binding
                    .validate_against(composition.reservation())
                    .is_err()
            {
                return Err(Stage6dLiveCoreError::RestartPackageBindingMismatch);
            }
            restart.stage5g_fresh_reconstruction_candidate()
        }
        Stage6dStage5RuntimeAuthority::FirstBoot(_) => {
            return Err(Stage6dLiveCoreError::RestartRuntimeRequired)
        }
    };
    let Stage6dStage5RuntimeAuthority::Restart(restart) = std::mem::replace(
        &mut recovered.stage5_runtime,
        Stage6dStage5RuntimeAuthority::FirstBoot(Box::new(replacement_runtime)),
    ) else {
        unreachable!("P1-d4 prepublication authority was checked above")
    };
    let (runtime, state, p1d3, semantic, composition, export_input) = restart
        .into_stage8b_p1d4_ack_parts(&publication_binding, receipt_ts)
        .map_err(Stage6dLiveCoreError::from)?;
    if p1d3.authenticated_stage6_checkpoint_sha256() != prepublication_checkpoint_sha256 {
        return Err(Stage6dLiveCoreError::RestartPackageBindingMismatch);
    }
    let p1d3 = p1d3
        .rebind_semantic_request_checkpoint(
            prepublication_checkpoint_sha256,
            recovered
                .authenticated_checkpoint()
                .checkpoint_sha256()
                .to_string(),
        )
        .map_err(|_| Stage6dLiveCoreError::RestartPackageBindingMismatch)?;
    let (q0, avg0) = crate::Stage5gOrderPositionSession::stage8b_p1d3_position_basis(&state)
        .ok_or(Stage6dLiveCoreError::Stage8bP1d2MarketFeedback)?;
    let finalized =
        crate::stage8b_p1d2_market_feedback::mint_stage8b_p1d4_finalized_market_feedback(
            semantic.clone(),
            &facts,
            evidence,
            q0,
            avg0,
        )?;
    let applied = crate::stage8b_p1d4_generated_market::apply_stage8b_p1d4_ack_stage(
        runtime,
        state,
        p1d3,
        semantic,
        composition,
        publication_binding,
        finalized,
        export_input,
        commitment_key,
    )
    .map_err(|_| Stage6dLiveCoreError::RestartPackageBindingMismatch)?;
    recovered.stage5_runtime = Stage6dStage5RuntimeAuthority::Restart(Box::new(applied.restored));
    recovered.refresh_after_append()?;
    Ok(Stage6Stage8bP1d4AckTransition {
        recovered,
        stage5g_restart_package: applied.restart_package,
    })
}

/// Resumes a generated-Market request from one of the four authenticated V1
/// journal-ahead suffixes. The deterministic provider evidence is recomputed
/// from the exact retained successor M10, but no schedule is reacquired and
/// no dispatch row is appended. Only the missing suffix records are emitted.
pub fn apply_stage8b_p1d4_recovered_ack_transition(
    mut recovered: Stage6dDurableRuntimeRecovered,
    candidate: Stage6Stage8bP1d4JournalAheadCandidate,
    canonical_m10: crate::Stage8bP1d1CanonicalM10Evidence,
    publication_binding: crate::Stage8bP1d4CommandPublicationBindingV1,
    commitment_key: &Stage5gLifecycleCommitmentKey,
) -> Result<Stage6Stage8bP1d4AckTransition, Stage6dLiveCoreError> {
    let receipt_ts_utc_ms = canonical_m10.close_ts_utc_ms;
    let decision = recovered.stage8b_p1d2_recovery_command_decision_binding()?;
    let evidence =
        crate::stage8b_p1d1_paper_provider::reconstruct_stage8b_p1d1_market_outcome_evidence(
            decision,
            canonical_m10,
        )
        .map_err(|_| Stage6dLiveCoreError::Stage8bP1d2MarketFeedback)?;
    if candidate.identity().strategy_request_id() != evidence.strategy_request_id
        || publication_binding.prepublication_package_generation() == 0
    {
        return Err(Stage6dLiveCoreError::RestartPackageBindingMismatch);
    }
    let expected_outcome = Stage6dPaperOutcome::MarketFilled {
        broker_order_id: evidence.broker_order_id.clone(),
        broker_trade_id: evidence.broker_trade_id.clone(),
    };
    let outcome_evidence = accepted_paper_evidence(candidate.identity(), &expected_outcome)?;
    let request = recovered
        .replay()
        .request(evidence.strategy_request_id)
        .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?;
    let mut previous = request.last_unique_record_id().clone();
    let mut sequence = request
        .last_unique_sequence()
        .checked_add(1)
        .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?;
    match candidate.kind() {
        Stage6Stage8bP1d4JournalAheadKind::DispatchOnly => {
            if candidate.broker_order_id().is_some()
                || candidate.broker_trade_id().is_some()
                || request.last_unique_record_id() != candidate.dispatch_record_id()
            {
                return Err(Stage6dLiveCoreError::DurableOrderingViolation);
            }
            let order = Stage6JournalRecordV1::broker_order_observed(
                candidate.identity().clone(),
                evidence.broker_order_id.clone(),
                Stage6LifecycleSequence::new(sequence)?,
                Some(previous),
                outcome_evidence.clone(),
            )?;
            sequence = sequence
                .checked_add(1)
                .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?;
            previous = order.journal_record_id().clone();
            let trade = Stage6JournalRecordV1::broker_trade_observed(
                candidate.identity().clone(),
                evidence.broker_trade_id.clone(),
                evidence.broker_order_id.clone(),
                Stage6LifecycleSequence::new(sequence)?,
                Some(previous),
                outcome_evidence,
            )?;
            recovered.journal_mut().append(&order)?;
            recovered.journal_mut().append(&trade)?;
            recovered.refresh_after_append()?;
        }
        Stage6Stage8bP1d4JournalAheadKind::OrderOnly => {
            if candidate.broker_order_id() != Some(&evidence.broker_order_id)
                || candidate.broker_trade_id().is_some()
            {
                return Err(Stage6dLiveCoreError::DurableOrderingViolation);
            }
            let last = recovered
                .journal
                .versioned_records()
                .last()
                .and_then(|record| match record {
                    Stage6JournalRecordVersioned::V1(record) => Some(record),
                    _ => None,
                })
                .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?;
            if last.source_evidence_sha256() != &outcome_evidence {
                return Err(Stage6dLiveCoreError::DurableOrderingViolation);
            }
            let trade = Stage6JournalRecordV1::broker_trade_observed(
                candidate.identity().clone(),
                evidence.broker_trade_id.clone(),
                evidence.broker_order_id.clone(),
                Stage6LifecycleSequence::new(sequence)?,
                Some(previous),
                outcome_evidence,
            )?;
            recovered.journal_mut().append(&trade)?;
            recovered.refresh_after_append()?;
        }
        Stage6Stage8bP1d4JournalAheadKind::PreFinalization
        | Stage6Stage8bP1d4JournalAheadKind::PreAck => {
            if candidate.broker_order_id() != Some(&evidence.broker_order_id)
                || candidate.broker_trade_id() != Some(&evidence.broker_trade_id)
            {
                return Err(Stage6dLiveCoreError::DurableOrderingViolation);
            }
            let records = recovered.journal.versioned_records();
            let mut v1_tail = records.iter().rev().filter_map(|record| match record {
                Stage6JournalRecordVersioned::V1(record) => Some(record),
                _ => None,
            });
            let last = v1_tail
                .next()
                .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?;
            let (trade, order) = if candidate.kind() == Stage6Stage8bP1d4JournalAheadKind::PreAck {
                if !matches!(
                    last.payload(),
                    Stage6JournalPayloadV1::RequestFinalized { .. }
                ) {
                    return Err(Stage6dLiveCoreError::DurableOrderingViolation);
                }
                (
                    v1_tail
                        .next()
                        .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?,
                    v1_tail
                        .next()
                        .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?,
                )
            } else {
                (
                    last,
                    v1_tail
                        .next()
                        .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?,
                )
            };
            if order.source_evidence_sha256() != &outcome_evidence
                || trade.source_evidence_sha256() != &outcome_evidence
            {
                return Err(Stage6dLiveCoreError::DurableOrderingViolation);
            }
        }
    }
    let request = recovered
        .replay()
        .request(evidence.strategy_request_id)
        .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?;
    if request.dispatch_attempt_count() != 1
        || request.known_broker_order_id() != Some(&evidence.broker_order_id)
        || request.observed_broker_trade_ids() != [evidence.broker_trade_id.clone()]
        || request.conflict_observed()
    {
        return Err(Stage6dLiveCoreError::DurableOrderingViolation);
    }
    let receipt_ts = Utc
        .timestamp_millis_opt(receipt_ts_utc_ms)
        .single()
        .ok_or(Stage6dLiveCoreError::Stage8bP1d2MarketFeedback)?;
    if request.final_disposition().is_none() {
        finalize_stage7a_replayed_paper_request(
            &mut recovered,
            evidence.strategy_request_id,
            receipt_ts,
        )?;
    }
    let facts = stage7b_finalized_request_facts(&recovered, evidence.strategy_request_id)?;
    if facts.broker_order_id() != Some(&evidence.broker_order_id)
        || facts.broker_trade_ids() != [evidence.broker_trade_id.clone()]
        || facts.final_disposition() != Stage6RequestFinalDispositionV1::Completed
    {
        return Err(Stage6dLiveCoreError::Stage8bP1d2MarketFeedback);
    }
    complete_stage8b_p1d4_ack_transition(
        recovered,
        facts,
        evidence,
        publication_binding,
        candidate.predecessor_checkpoint_sha256(),
        receipt_ts,
        commitment_key,
    )
}

/// Applies only the truth already embedded in a combined generated-Market
/// S_ack package. It performs no provider call, dispatch or journal append.
pub fn apply_stage8b_p1d4_truth_transition(
    mut recovered: Stage6dDurableRuntimeRecovered,
    commitment_key: &Stage5gLifecycleCommitmentKey,
) -> Result<Stage6Stage8bP1d4TruthTransition, Stage6dLiveCoreError> {
    let pre_checkpoint = recovered.authenticated_checkpoint().clone();
    let replacement_runtime = match &recovered.stage5_runtime {
        Stage6dStage5RuntimeAuthority::Restart(restart) => {
            let composition = restart
                .stage8b_p1d4_generated_market_composition()
                .ok_or(Stage6dLiveCoreError::RestartPackageBindingMismatch)?;
            if composition.phase()
                != crate::stage8b_p1d4_generated_market::Stage8bP1d4GeneratedMarketPhase::AckCommitted
                || composition.binding().is_none()
            {
                return Err(Stage6dLiveCoreError::RestartPackageBindingMismatch);
            }
            restart.stage5g_fresh_reconstruction_candidate()
        }
        Stage6dStage5RuntimeAuthority::FirstBoot(_) => {
            return Err(Stage6dLiveCoreError::RestartRuntimeRequired)
        }
    };
    let Stage6dStage5RuntimeAuthority::Restart(restart) = std::mem::replace(
        &mut recovered.stage5_runtime,
        Stage6dStage5RuntimeAuthority::FirstBoot(Box::new(replacement_runtime)),
    ) else {
        unreachable!("P1-d4 S_ack authority was checked above")
    };
    let (runtime, state, p1d3, semantic, feedback, composition, export_input) = restart
        .into_stage8b_p1d4_truth_parts()
        .map_err(Stage6dLiveCoreError::from)?;
    let applied = crate::stage8b_p1d4_generated_market::apply_stage8b_p1d4_truth_stage(
        runtime,
        state,
        p1d3,
        semantic,
        feedback,
        composition,
        export_input,
        commitment_key,
    )
    .map_err(|_| Stage6dLiveCoreError::RestartPackageBindingMismatch)?;
    recovered.stage5_runtime = Stage6dStage5RuntimeAuthority::Restart(Box::new(applied.restored));
    recovered.refresh_after_append()?;
    if recovered.authenticated_checkpoint() != &pre_checkpoint {
        return Err(Stage6dLiveCoreError::DurableOrderingViolation);
    }
    Ok(Stage6Stage8bP1d4TruthTransition {
        recovered,
        stage5g_restart_package: applied.restart_package,
    })
}

/// Test-only construction of one source-authenticated P1-d3 command package.
///
/// This seam deliberately performs no provider or broker action. It exists so
/// the durable-service subprocess tests can establish a valid terminal LIMIT
/// plus pending CANCEL state before killing the process at the production
/// replacement-seal barriers.
#[cfg(feature = "stage5g-artifact-fixtures")]
#[doc(hidden)]
pub fn stage8b_p1d3_test_inject_one_intent_transition(
    mut recovered: Stage6dDurableRuntimeRecovered,
    binding: Stage5gP1SemanticBindingInput,
    source: Stage6Stage8bP1SealSourceV1,
    command: BrokerCommand,
    expected_attribution: HybridRuntimeAttribution,
    commitment_key: &Stage5gLifecycleCommitmentKey,
) -> Result<Stage6Stage8bP1SemanticTransition, Stage6dLiveCoreError> {
    let operational_identity = recovered
        .authenticated_operational_identity()
        .ok_or(Stage6dLiveCoreError::RestartRuntimeRequired)?;
    let operational_identity_sha256 = stage6d_operational_identity_sha256(operational_identity)?;
    if source.seal_generation == 0
        || Stage6Sha256Digest::parse(source.seal_commitment_sha256.clone()).is_err()
        || source.stage6_checkpoint_sha256
            != recovered.authenticated_checkpoint().checkpoint_sha256()
        || source.stage6_frontier_sha256 != frontier_fingerprint(recovered.journal_frontier())?
        || source.operational_identity_sha256 != operational_identity_sha256.as_str()
        || binding.operational_identity_sha256 != source.operational_identity_sha256
    {
        return Err(Stage6dLiveCoreError::RestartPackageBindingMismatch);
    }

    let (
        placeholder_runtime,
        reconstruction_runtime,
        prior_stage5_checkpoint_sha256,
        instrument,
        pre_position_qty,
    ) = {
        let current = match &recovered.stage5_runtime {
            Stage6dStage5RuntimeAuthority::Restart(current)
                if current.stage8b_p1d3_replacement().is_some() =>
            {
                current
            }
            _ => return Err(Stage6dLiveCoreError::RestartRuntimeRequired),
        };
        (
            current.stage5g_fresh_reconstruction_candidate(),
            current.stage5g_fresh_reconstruction_candidate(),
            sha256_hex(
                &serde_json::to_vec(current.checkpoint())
                    .map_err(|_| Stage6dLiveCoreError::IntegrationFingerprint)?,
            ),
            current
                .stage8b_p1d3_replacement()
                .expect("P1-d3 replacement checked above")
                .working_book()
                .instrument()
                .clone(),
            current
                .stage8b_p1d3_position_basis()
                .and_then(|(qty, _)| qty.to_f64())
                .ok_or(Stage6dLiveCoreError::Stage8bP1d3WorkingLimit)?,
        )
    };
    let request_id = match &command {
        BrokerCommand::PlaceOrder(place) => place.request_id,
        BrokerCommand::CancelOrder(cancel) => cancel.request_id,
    };
    let (intent_class, base_action, side, target_qty, pre_position_qty) = match &command {
        BrokerCommand::PlaceOrder(place) => {
            let target_qty = place.qty.to_f64();
            let closes_existing_position = target_qty.is_some_and(|qty| {
                qty <= pre_position_qty.abs()
                    && matches!(
                        (pre_position_qty.is_sign_positive(), place.side),
                        (true, broker_core::OrderSide::Sell) | (false, broker_core::OrderSide::Buy)
                    )
            });
            (
                if closes_existing_position {
                    crate::BrokerNeutralHybridIntentClass::Exit
                } else {
                    crate::BrokerNeutralHybridIntentClass::Entry
                },
                crate::stage5c_paper_host::Stage5gSourceBaseAction::Place,
                Some(match place.side {
                    broker_core::OrderSide::Buy => crate::BrokerNeutralOrderSide::Buy,
                    broker_core::OrderSide::Sell => crate::BrokerNeutralOrderSide::Sell,
                }),
                target_qty,
                pre_position_qty,
            )
        }
        BrokerCommand::CancelOrder(_) => (
            crate::BrokerNeutralHybridIntentClass::CancelCleanup,
            crate::stage5c_paper_host::Stage5gSourceBaseAction::Cancel,
            None,
            None,
            pre_position_qty,
        ),
    };
    let source_intent = crate::stage5c_paper_host::Stage5gSourceIntentProjection {
        request_id,
        intent_class,
        base_action,
        side,
        target_qty,
        pre_position_qty,
        expected_attribution: Some(expected_attribution.clone()),
    };
    let projection = crate::stage5g_p1_semantic::stage8b_p1d3_test_one_intent_projection(
        binding,
        prior_stage5_checkpoint_sha256,
        command.clone(),
        instrument,
        expected_attribution,
        source_intent,
    )
    .map_err(|_| Stage6dLiveCoreError::DurableOrderingViolation)?;
    let identity = projection
        .durable_request_identity
        .as_ref()
        .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?
        .clone();
    let command_snapshot = projection
        .durable_command_snapshot
        .as_ref()
        .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?
        .clone();
    if stage7a_accepted_record(&recovered, request_id).is_some()
        || stage7a_has_other_unresolved_lifecycle(&recovered, &identity)
    {
        return Err(Stage6dLiveCoreError::DurableOrderingViolation);
    }
    let sequence = Stage6LifecycleSequence::new(1)?;
    let expected_record_id = Stage6JournalRecordId::derive(request_id, sequence);
    let accepted_source =
        stage8b_p1_request_accepted_source_evidence(&projection, &source, &expected_record_id)?;
    let accepted = Stage6JournalRecordV1::request_accepted(
        identity.clone(),
        command_snapshot.clone(),
        sequence,
        None,
        None,
        accepted_source,
    )?;
    let pre_request_checkpoint_sha256 = recovered
        .authenticated_checkpoint()
        .checkpoint_sha256()
        .to_string();
    let post_request_checkpoint_sha256 = projected_checkpoint_after_append(&recovered, &accepted)?;

    let Stage6dStage5RuntimeAuthority::Restart(current) = std::mem::replace(
        &mut recovered.stage5_runtime,
        Stage6dStage5RuntimeAuthority::FirstBoot(Box::new(placeholder_runtime)),
    ) else {
        unreachable!("P1-d3 source lifecycle was checked above")
    };
    let transition_identity = projection.semantic_batch_id_sha256.clone();
    let source_close_ts_utc_ms = projection
        .m10_redis_id
        .strip_suffix("-0")
        .and_then(|value| value.parse::<i64>().ok())
        .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?;
    let export_input = current
        .stage8b_p1_next_export_input(&transition_identity, source_close_ts_utc_ms)
        .map_err(Stage6dLiveCoreError::from)?;
    let (runtime, state, replacement) = current
        .into_stage8b_p1d3_parts()
        .map_err(Stage6dLiveCoreError::from)?;
    let replacement = if matches!(command, BrokerCommand::PlaceOrder(_)) {
        replacement
            .stage8b_p1d3_test_rebind_migrated_attribution(
                projection
                    .expected_attribution
                    .clone()
                    .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?,
            )
            .map_err(|_| Stage6dLiveCoreError::DurableOrderingViolation)?
    } else {
        replacement
    };
    let stage5g_restart_package = export_stage5g_clean_restart(
        crate::Stage5gCleanRestartSource::P1d3(Box::new(
            crate::stage8b_p1d3_working_limit::Stage8bP1d3RestartSource::with_semantic_commit(
                runtime,
                state,
                replacement,
                Some(projection.clone()),
            )
            .and_then(|source| {
                source.rebind_semantic_request_checkpoint(
                    &pre_request_checkpoint_sha256,
                    post_request_checkpoint_sha256.clone(),
                )
            })
            .map_err(|_| Stage6dLiveCoreError::Stage8bP1d3WorkingLimit)?,
        )),
        export_input,
        commitment_key,
    )?;
    let restored = restore_stage5g_clean_restart(
        &stage5g_restart_package,
        commitment_key,
        reconstruction_runtime,
    )?;
    if restored.stage8b_p1_semantic_commit() != Some(&projection)
        || restored.stage8b_p1d3_replacement().is_none()
    {
        return Err(Stage6dLiveCoreError::RestartPackageBindingMismatch);
    }

    recovered
        .journal_mut()
        .append(&accepted)
        .map_err(classify_stage8a4_append_error)?;
    recovered.stage5_runtime = Stage6dStage5RuntimeAuthority::Restart(Box::new(restored));
    recovered.refresh_after_append()?;
    if recovered.authenticated_checkpoint().checkpoint_sha256() != post_request_checkpoint_sha256 {
        return Err(Stage6dLiveCoreError::JournalMutationMayHaveOccurred);
    }
    let evidence = stage8b_p1_commit_evidence(&projection, Some(&accepted));
    Ok(Stage6Stage8bP1SemanticTransition::OneIntentPrepublication {
        recovered: Box::new(recovered),
        stage5g_restart_package,
        evidence,
        command,
        durable_request_identity: identity,
        durable_command_snapshot: command_snapshot,
    })
}

/// Returns the exact broker order named by the latest authenticated P1-d3
/// outcome. This is test-only evidence extraction; it cannot mutate runtime
/// state or mint a lifecycle capability.
#[cfg(feature = "stage5g-artifact-fixtures")]
#[doc(hidden)]
pub fn stage8b_p1d3_test_latest_outcome_broker_order_id(
    recovered: &Stage6dDurableRuntimeRecovered,
) -> Option<BrokerOrderId> {
    let Stage6dStage5RuntimeAuthority::Restart(current) = &recovered.stage5_runtime else {
        return None;
    };
    current
        .stage8b_p1d3_replacement()?
        .latest_outcome_evidence()
        .map(|evidence| evidence.broker_order_id().clone())
}

/// Returns the exact P1-d3 working-book attribution for fixture construction.
/// It exposes no runtime mutation or lifecycle capability.
#[cfg(feature = "stage5g-artifact-fixtures")]
#[doc(hidden)]
pub fn stage8b_p1d3_test_working_book_attribution(
    recovered: &Stage6dDurableRuntimeRecovered,
) -> Option<HybridRuntimeAttribution> {
    let Stage6dStage5RuntimeAuthority::Restart(current) = &recovered.stage5_runtime else {
        return None;
    };
    current
        .stage8b_p1d3_replacement()
        .map(|replacement| replacement.working_book().attribution().clone())
}

/// Applies one deterministic P1-d1 Market result to the Stage 6/7 journal,
/// finalizes it at the execution-bar receipt clock and constructs the first
/// Stage 5G replacement package. No ACK reducer is reachable before the
/// RequestFinalized record has been appended and reread.
pub fn apply_stage8b_p1d2_ack_transition(
    mut recovered: Stage6dDurableRuntimeRecovered,
    outcome_bundle: crate::Stage8bP1d1MarketOutcomeBundle,
    commitment_key: &Stage5gLifecycleCommitmentKey,
) -> Result<Stage6Stage8bP1d2AckTransition, Stage6dLiveCoreError> {
    let (dispatch_receipt, outcome, evidence) = outcome_bundle.into_p1d2_parts();
    let receipt_ts = Utc
        .timestamp_millis_opt(evidence.fill_received_ts_utc_ms)
        .single()
        .ok_or(Stage6dLiveCoreError::Stage8bP1d2MarketFeedback)?;
    let report = execute_stage6d_paper_outcome(&mut recovered, dispatch_receipt, outcome)?;
    stage8b_p1d2_test_crash_barrier("p1d2-after-stage6-before-request-finalized");
    let report = finalize_stage7a_paper_request(&mut recovered, report, receipt_ts)?;
    let facts = stage7b_finalized_request_facts(&recovered, report.strategy_request_id)?;
    if report.strategy_request_id != facts.strategy_request_id()
        || report.durable_client_order_id != *facts.durable_client_order_id()
        || report.account_id != *facts.durable_request_identity().account_id()
        || report.instrument != *facts.durable_request_identity().instrument()
        || report.attribution != *facts.durable_request_identity().attribution()
        || report.action != facts.durable_request_identity().action()
        || report.broker_order_id.as_ref() != facts.broker_order_id()
        || report.broker_trade_ids != facts.broker_trade_ids()
        || report.final_disposition != Some(facts.final_disposition())
        || report.final_record_id != facts.final_record_id().as_str()
        || report.final_sequence != facts.final_sequence()
    {
        return Err(Stage6dLiveCoreError::Stage8bP1d2MarketFeedback);
    }
    stage8b_p1d2_test_crash_barrier("p1d2-after-request-finalized-before-ack");

    let replacement_runtime = match &recovered.stage5_runtime {
        Stage6dStage5RuntimeAuthority::Restart(restart)
            if restart.lifecycle_kind()
                == crate::Stage5gCleanRestartLifecycleKind::P1SemanticPrepublication =>
        {
            restart.stage5g_fresh_reconstruction_candidate()
        }
        _ => return Err(Stage6dLiveCoreError::RestartRuntimeRequired),
    };
    let Stage6dStage5RuntimeAuthority::Restart(restart) = std::mem::replace(
        &mut recovered.stage5_runtime,
        Stage6dStage5RuntimeAuthority::FirstBoot(Box::new(replacement_runtime)),
    ) else {
        unreachable!("P1-d2 restart authority was checked above")
    };
    let (settled, p1, export_input) = restart.into_stage8b_p1d2_pre_ack_parts(receipt_ts)?;
    let finalized =
        crate::stage8b_p1d2_market_feedback::mint_stage8b_p1d2_finalized_market_feedback(
            p1, &facts, evidence,
        )?;
    let applied = crate::stage8b_p1d2_market_feedback::apply_stage8b_p1d2_ack_stage(
        settled,
        finalized,
        export_input,
        commitment_key,
    )?;
    recovered.stage5_runtime = Stage6dStage5RuntimeAuthority::Restart(Box::new(applied.restored));
    recovered.refresh_after_append()?;
    Ok(Stage6Stage8bP1d2AckTransition {
        recovered,
        stage5g_restart_package: applied.restart_package,
    })
}

/// Reconstructs the pre-S_ack transition after a crash from authenticated
/// Stage 6/7 facts plus the exact retained successor M10. It never owns a
/// dispatch receipt and therefore cannot invoke the P1-d1 provider or append
/// the order/trade outcome a second time.
pub fn apply_stage8b_p1d2_recovered_ack_transition(
    mut recovered: Stage6dDurableRuntimeRecovered,
    canonical_m10: crate::Stage8bP1d1CanonicalM10Evidence,
    commitment_key: &Stage5gLifecycleCommitmentKey,
) -> Result<Stage6Stage8bP1d2AckTransition, Stage6dLiveCoreError> {
    let receipt_ts_utc_ms = canonical_m10.close_ts_utc_ms;
    let decision = recovered.stage8b_p1d2_recovery_command_decision_binding()?;
    if let Some(binding) = stage8b_p1e_market_binding_before_recovered_dispatch(
        &recovered,
        decision.strategy_request_id(),
        decision.canonical_command_sha256(),
    )? {
        if !canonical_m10
            .matches_stage8b_p1e_m10_identity(&binding.candidate_or_last_eligible_m10())
        {
            return Err(Stage6dLiveCoreError::DurableOrderingViolation);
        }
    }
    let evidence =
        crate::stage8b_p1d1_paper_provider::reconstruct_stage8b_p1d1_market_outcome_evidence(
            decision,
            canonical_m10,
        )
        .map_err(|_| Stage6dLiveCoreError::Stage8bP1d2MarketFeedback)?;
    let request_id = evidence.strategy_request_id;
    let request = recovered
        .replay()
        .request(request_id)
        .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?;
    if request.action() != Stage6DurableActionKind::Place
        || request.dispatch_attempt_count() != 1
        || request.dispatch_safety_state() != crate::Stage6DispatchSafetyStateV1::DispatchForbidden
        || request.known_broker_order_id() != Some(&evidence.broker_order_id)
        || request.observed_broker_trade_ids() != [evidence.broker_trade_id.clone()]
        || request.cancel_outcome().is_some()
        || request.conflict_observed()
        || !matches!(
            request.final_disposition(),
            None | Some(Stage6RequestFinalDispositionV1::Completed)
        )
    {
        return Err(Stage6dLiveCoreError::DurableOrderingViolation);
    }
    let receipt_ts = Utc
        .timestamp_millis_opt(receipt_ts_utc_ms)
        .single()
        .ok_or(Stage6dLiveCoreError::Stage8bP1d2MarketFeedback)?;
    if request.final_disposition().is_none() {
        finalize_stage7a_replayed_paper_request(&mut recovered, request_id, receipt_ts)?;
    }
    let facts = stage7b_finalized_request_facts(&recovered, request_id)?;
    if facts.broker_order_id() != Some(&evidence.broker_order_id)
        || facts.broker_trade_ids() != [evidence.broker_trade_id.clone()]
        || facts.final_disposition() != Stage6RequestFinalDispositionV1::Completed
    {
        return Err(Stage6dLiveCoreError::Stage8bP1d2MarketFeedback);
    }

    let replacement_runtime = match &recovered.stage5_runtime {
        Stage6dStage5RuntimeAuthority::Restart(restart)
            if restart.lifecycle_kind()
                == crate::Stage5gCleanRestartLifecycleKind::P1SemanticPrepublication =>
        {
            restart.stage5g_fresh_reconstruction_candidate()
        }
        _ => return Err(Stage6dLiveCoreError::RestartRuntimeRequired),
    };
    let Stage6dStage5RuntimeAuthority::Restart(restart) = std::mem::replace(
        &mut recovered.stage5_runtime,
        Stage6dStage5RuntimeAuthority::FirstBoot(Box::new(replacement_runtime)),
    ) else {
        unreachable!("P1-d2 recovery authority was checked above")
    };
    let (settled, p1, export_input) = restart.into_stage8b_p1d2_pre_ack_parts(receipt_ts)?;
    let finalized =
        crate::stage8b_p1d2_market_feedback::mint_stage8b_p1d2_finalized_market_feedback(
            p1, &facts, evidence,
        )?;
    let applied = crate::stage8b_p1d2_market_feedback::apply_stage8b_p1d2_ack_stage(
        settled,
        finalized,
        export_input,
        commitment_key,
    )?;
    recovered.stage5_runtime = Stage6dStage5RuntimeAuthority::Restart(Box::new(applied.restored));
    recovered.refresh_after_append()?;
    Ok(Stage6Stage8bP1d2AckTransition {
        recovered,
        stage5g_restart_package: applied.restart_package,
    })
}

/// Consumes only a restored S_ack owner and constructs S_truth. The exact
/// truth sequence is recovered from the resolved ACK slot in S_ack.
pub fn apply_stage8b_p1d2_truth_transition(
    mut recovered: Stage6dDurableRuntimeRecovered,
    commitment_key: &Stage5gLifecycleCommitmentKey,
) -> Result<Stage6Stage8bP1d2TruthTransition, Stage6dLiveCoreError> {
    let replacement_runtime = match &recovered.stage5_runtime {
        Stage6dStage5RuntimeAuthority::Restart(restart) => {
            restart.stage8b_p1d2_validate_ack_frontier()?;
            restart.stage5g_fresh_reconstruction_candidate()
        }
        Stage6dStage5RuntimeAuthority::FirstBoot(_) => {
            return Err(Stage6dLiveCoreError::RestartRuntimeRequired)
        }
    };
    let Stage6dStage5RuntimeAuthority::Restart(restart) = std::mem::replace(
        &mut recovered.stage5_runtime,
        Stage6dStage5RuntimeAuthority::FirstBoot(Box::new(replacement_runtime)),
    ) else {
        unreachable!("P1-d2 ACK authority was checked above")
    };
    let applied = crate::stage8b_p1d2_market_feedback::apply_stage8b_p1d2_truth_stage(
        *restart,
        commitment_key,
    )?;
    recovered.stage5_runtime = Stage6dStage5RuntimeAuthority::Restart(Box::new(applied.restored));
    recovered.refresh_after_append()?;
    Ok(Stage6Stage8bP1d2TruthTransition {
        recovered,
        stage5g_restart_package: applied.restart_package,
    })
}

/// Consumes the accepted P1-d2 truth runtime and installs the empty P1-d3
/// working book without appending a Stage 6 record or invoking a callback,
/// provider, dispatch, ACK, truth, or source-resolution surface.
///
/// Source resolution is deliberately not represented in core.  The Stage 7
/// linear owner exposes this transition only after the exact P1-d2 source has
/// already been XACKed.
pub fn migrate_stage8b_p1d3_runtime_from_p1d2(
    recovered: Stage6dDurableRuntimeRecovered,
    commitment_key: &Stage5gLifecycleCommitmentKey,
) -> Result<Stage6Stage8bP1d3MigrationTransition, Stage6dLiveCoreError> {
    migrate_stage8b_p1d3_runtime_from_p1d2_inner(recovered, commitment_key, false)
}

#[cfg(feature = "stage5g-artifact-fixtures")]
#[doc(hidden)]
pub fn stage8b_p1d4_test_migrate_position_synced_p1d2(
    recovered: Stage6dDurableRuntimeRecovered,
    commitment_key: &Stage5gLifecycleCommitmentKey,
) -> Result<Stage6Stage8bP1d3MigrationTransition, Stage6dLiveCoreError> {
    migrate_stage8b_p1d3_runtime_from_p1d2_inner(recovered, commitment_key, true)
}

fn migrate_stage8b_p1d3_runtime_from_p1d2_inner(
    mut recovered: Stage6dDurableRuntimeRecovered,
    commitment_key: &Stage5gLifecycleCommitmentKey,
    synchronize_runtime_position_fixture: bool,
) -> Result<Stage6Stage8bP1d3MigrationTransition, Stage6dLiveCoreError> {
    if !recovered.stage8b_p1d2_truth_frontier_is_authenticated() {
        return Err(Stage6dLiveCoreError::RestartRuntimeRequired);
    }
    let operational_identity = recovered
        .authenticated_operational_identity()
        .ok_or(Stage6dLiveCoreError::RestartRuntimeRequired)?;
    let operational_identity_sha256 = stage6d_operational_identity_sha256(operational_identity)?;
    let authenticated_checkpoint_sha256 = recovered
        .authenticated_checkpoint()
        .checkpoint_sha256()
        .to_string();
    let pre_frontier = recovered.journal_frontier().clone();
    let reconstruction_runtime = match &recovered.stage5_runtime {
        Stage6dStage5RuntimeAuthority::Restart(restart) => {
            restart.stage5g_fresh_reconstruction_candidate()
        }
        Stage6dStage5RuntimeAuthority::FirstBoot(_) => {
            return Err(Stage6dLiveCoreError::RestartRuntimeRequired)
        }
    };
    let Stage6dStage5RuntimeAuthority::Restart(restart) = std::mem::replace(
        &mut recovered.stage5_runtime,
        Stage6dStage5RuntimeAuthority::FirstBoot(Box::new(reconstruction_runtime)),
    ) else {
        unreachable!("P1-d3 migration source was checked above")
    };
    let migration = if synchronize_runtime_position_fixture {
        #[cfg(feature = "stage5g-artifact-fixtures")]
        {
            crate::stage8b_p1d3_working_limit::stage8b_p1d4_test_migrate_from_position_synced_p1d2(
                *restart,
                operational_identity_sha256.as_str().to_string(),
                1,
                authenticated_checkpoint_sha256.clone(),
                commitment_key,
            )?
        }
        #[cfg(not(feature = "stage5g-artifact-fixtures"))]
        unreachable!("position-synchronized migration is fixture-only")
    } else {
        crate::stage8b_p1d3_working_limit::migrate_stage8b_p1d3_from_p1d2(
            *restart,
            operational_identity_sha256.as_str().to_string(),
            1,
            authenticated_checkpoint_sha256.clone(),
            commitment_key,
        )?
    };
    recovered.stage5_runtime = Stage6dStage5RuntimeAuthority::Restart(Box::new(migration.restored));
    recovered.refresh_after_append()?;
    if recovered.journal_frontier() != &pre_frontier
        || recovered.authenticated_checkpoint().checkpoint_sha256()
            != authenticated_checkpoint_sha256
    {
        return Err(Stage6dLiveCoreError::DurableOrderingViolation);
    }
    Ok(Stage6Stage8bP1d3MigrationTransition {
        recovered,
        stage5g_restart_package: migration.restart_package,
    })
}

/// Consumes the first exact post-decision schedule observation for one
/// canonical LIMIT command. All request, command, book, position and Stage 6
/// identities are reread from authenticated owners. The observation is
/// validated without effects before the sole dispatch row is appended, then
/// its complete outcome evidence and RequestFinalized row are written before
/// the P1-d3 ACK callback can run.
pub fn apply_stage8b_p1d3_initial_limit_ack_transition(
    mut recovered: Stage6dDurableRuntimeRecovered,
    observation: crate::Stage8bP1d3InitialObservation,
    commitment_key: &Stage5gLifecycleCommitmentKey,
) -> Result<Stage6Stage8bP1d3AckTransition, Stage6dLiveCoreError> {
    let transition_ts_utc_ms = observation.transition_ts_utc_ms();
    let transition_ts = Utc
        .timestamp_millis_opt(transition_ts_utc_ms)
        .single()
        .ok_or(Stage6dLiveCoreError::Stage8bP1d3WorkingLimit)?;
    let (semantic, replacement, position_basis, reconstruction_runtime) =
        match &recovered.stage5_runtime {
            Stage6dStage5RuntimeAuthority::Restart(restart) => {
                let semantic = restart
                    .stage8b_p1_semantic_commit()
                    .filter(|projection| projection.validate() && projection.intent_count == 1)
                    .cloned()
                    .ok_or(Stage6dLiveCoreError::RestartRuntimeRequired)?;
                let replacement = restart
                    .stage8b_p1d3_replacement()
                    .cloned()
                    .ok_or(Stage6dLiveCoreError::RestartRuntimeRequired)?;
                let position_basis = restart
                    .stage8b_p1d3_position_basis()
                    .ok_or(Stage6dLiveCoreError::Stage8bP1d3WorkingLimit)?;
                (
                    semantic,
                    replacement,
                    position_basis,
                    restart.stage5g_fresh_reconstruction_candidate(),
                )
            }
            Stage6dStage5RuntimeAuthority::FirstBoot(_) => {
                return Err(Stage6dLiveCoreError::RestartRuntimeRequired)
            }
        };
    let request_id = semantic
        .request_id
        .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?;
    let command = semantic
        .canonical_command
        .as_ref()
        .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?;
    let identity = semantic
        .durable_request_identity
        .as_ref()
        .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?;
    let command_snapshot = semantic
        .durable_command_snapshot
        .as_ref()
        .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?;
    let canonical_command_sha256 = semantic
        .canonical_command_sha256
        .as_ref()
        .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?;
    let BrokerCommand::PlaceOrder(place) = command else {
        return Err(Stage6dLiveCoreError::DurableOrderingViolation);
    };
    let shape = command_snapshot
        .place_order_shape()
        .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?;
    let decision_m10_close_ts_utc_ms = semantic
        .m10_redis_id
        .strip_suffix("-0")
        .and_then(|value| value.parse::<i64>().ok())
        .filter(|value| *value > 0 && value.rem_euclid(600_000) == 0)
        .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?;
    let decision_m10_open_ts_utc_ms = decision_m10_close_ts_utc_ms
        .checked_sub(600_000)
        .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?;
    let accepted = stage7a_accepted_record(&recovered, request_id)
        .cloned()
        .ok_or(Stage6dLiveCoreError::AcceptedRecordRequired)?;
    let accepted_snapshot = match accepted.payload() {
        Stage6JournalPayloadV1::RequestAccepted { command } => command.as_ref(),
        _ => return Err(Stage6dLiveCoreError::AcceptedRecordRequired),
    };
    let replayed = recovered
        .replay()
        .request(request_id)
        .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?;
    let command_sha256 = sha256_hex(
        &serde_json::to_vec(command).map_err(|_| Stage6dLiveCoreError::IntegrationFingerprint)?,
    );
    if request_id != place.request_id
        || identity.strategy_request_id() != request_id
        || identity.action() != Stage6DurableActionKind::Place
        || identity.account_id() != replacement.working_book().account_id()
        || identity.instrument() != replacement.working_book().instrument()
        || identity.attribution() != replacement.working_book().attribution()
        || accepted.durable_request_identity() != identity
        || accepted_snapshot != command_snapshot
        || command_sha256 != *canonical_command_sha256
        || semantic.operational_identity_sha256
            != replacement.working_book().operational_identity_sha256()
        || place.account_id != *identity.account_id()
        || place.instrument != *identity.instrument()
        || place.client_order_id != *identity.durable_client_order_id()
        || place.comment.as_deref() != Some(identity.attribution().internal_comment())
        || place.order_type != broker_core::OrderType::Limit
        || place.time_in_force != broker_core::TimeInForce::Day
        || place.ttl_ms.is_some()
        || shape.order_type() != broker_core::OrderType::Limit
        || shape.time_in_force() != broker_core::TimeInForce::Day
        || shape.side() != place.side
        || shape.quantity() != place.qty
        || shape.limit_price() != place.limit_price
        || replayed.dispatch_safety_state()
            != crate::Stage6DispatchSafetyStateV1::ReadyForFirstDispatch
        || replayed.last_unique_record_id() != accepted.journal_record_id()
        || recovered.journal_frontier().last_record_id() != Some(accepted.journal_record_id())
    {
        return Err(Stage6dLiveCoreError::DurableOrderingViolation);
    }
    let limit_price = place
        .limit_price
        .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?;
    let dispatch = stage7a_dispatch_record(identity, &accepted, transition_ts)?;
    let dispatch_record_id = dispatch.journal_record_id().as_str().to_string();
    let pre_dispatch_checkpoint_sha256 = recovered
        .authenticated_checkpoint()
        .checkpoint_sha256()
        .to_string();
    let reserved_checkpoint_sha256 = stage8b_p1d3_reservation_sha256(
        &pre_dispatch_checkpoint_sha256,
        &dispatch_record_id,
        request_id,
        transition_ts_utc_ms,
        &semantic.m10_semantic_id_sha256,
    );
    let preliminary = crate::stage8b_p1d3_working_limit::Stage8bP1d3InitialLimitInput {
        operational_identity_sha256: replacement
            .working_book()
            .operational_identity_sha256()
            .to_string(),
        package_generation: replacement.working_book().package_generation(),
        account_id: identity.account_id().clone(),
        instrument: identity.instrument().clone(),
        attribution: identity.attribution().clone(),
        request_id,
        durable_client_order_id: identity.durable_client_order_id().clone(),
        canonical_command_sha256: canonical_command_sha256.clone(),
        accepted_command_payload_sha256: accepted.canonical_payload_sha256().as_str().to_string(),
        accepted_stage6_identity_sha256: "11".repeat(32),
        decision_m10_redis_id: semantic.m10_redis_id.clone(),
        decision_m10_semantic_id_sha256: semantic.m10_semantic_id_sha256.clone(),
        decision_m10_payload_sha256: semantic.m10_payload_sha256.clone(),
        decision_m10_open_ts_utc_ms,
        decision_m10_close_ts_utc_ms,
        side: place.side,
        qty: place.qty,
        limit_price,
        pre_position_qty: position_basis.0,
        pre_position_avg_price: position_basis.1,
        sequence_allocation_frontier: replacement.working_book().total_sequence_frontier(),
        stage6_dispatch_record_id: dispatch_record_id.clone(),
        stage6_predecessor_frontier_sha256: pre_dispatch_checkpoint_sha256,
        stage6_reserved_checkpoint_sha256: reserved_checkpoint_sha256.clone(),
        previous_outcome_evidence_sha256: replacement
            .previous_outcome_evidence_sha256()
            .to_string(),
    };
    crate::stage8b_p1d3_working_limit::preflight_initial_limit_transition(
        replacement.working_book(),
        &preliminary,
        &observation,
    )?;

    let _dispatch_receipt =
        prepare_stage6d_existing_accepted_paper_dispatch(&mut recovered, &accepted, dispatch)?;
    stage8b_p1d2_test_crash_barrier("p1d4-initial-after-dispatch-before-authority-consumption");
    stage8b_p1d4_test_crash_frontier("F20");
    let authority =
        recovered.authorize_stage8a4_durable_batch_source(identity, command_snapshot)?;
    let accepted_stage6_identity_sha256 = authority
        .durable_request_binding_sha256()?
        .as_str()
        .to_string();
    let actual = crate::stage8b_p1d3_working_limit::Stage8bP1d3InitialLimitInput {
        accepted_stage6_identity_sha256,
        stage6_predecessor_frontier_sha256: authority.authenticated_checkpoint_sha256().to_string(),
        ..preliminary
    };
    let plan = crate::stage8b_p1d3_working_limit::build_preflighted_initial_limit_transition(
        replacement.working_book(),
        actual,
        observation,
    )?;
    stage8b_p1d2_test_crash_barrier("p1d4-initial-after-result-before-outcome-wal");
    stage8b_p1d4_test_crash_frontier("F02");
    let receipt = append_stage8b_p1d3_request_outcome(&mut recovered, authority, &plan)?;

    let Stage6dStage5RuntimeAuthority::Restart(restart) = std::mem::replace(
        &mut recovered.stage5_runtime,
        Stage6dStage5RuntimeAuthority::FirstBoot(Box::new(reconstruction_runtime)),
    ) else {
        unreachable!("P1-d3 restart authority was checked above")
    };
    let applied = crate::stage8b_p1d3_working_limit::apply_stage8b_p1d3_request_ack_stage(
        *restart,
        &receipt.outcome_record,
        receipt.recovery_binding,
        commitment_key,
    )?;
    recovered.stage5_runtime = Stage6dStage5RuntimeAuthority::Restart(Box::new(applied.restored));
    recovered.refresh_after_append()?;
    Ok(Stage6Stage8bP1d3AckTransition {
        recovered,
        stage5g_restart_package: applied.restart_package,
    })
}

/// Continues an initial LIMIT after restart from the exact already-durable
/// dispatch row.  The candidate was minted only by the exact P1-d3 classifier;
/// this path never calls a dispatch append helper.
pub fn resume_stage8b_p1d3_dispatch_only_limit_transition(
    mut recovered: Stage6dDurableRuntimeRecovered,
    candidate: Stage6Stage8bP1d3DispatchOnlyCandidate,
    observation: crate::Stage8bP1d3InitialObservation,
    commitment_key: &Stage5gLifecycleCommitmentKey,
) -> Result<Stage6Stage8bP1d3AckTransition, Stage6dLiveCoreError> {
    if !candidate.is_limit_place() {
        return Err(Stage6dLiveCoreError::DurableOrderingViolation);
    }
    let transition_ts_utc_ms = observation.transition_ts_utc_ms();
    let (semantic, replacement, position_basis, reconstruction_runtime) =
        match &recovered.stage5_runtime {
            Stage6dStage5RuntimeAuthority::Restart(restart) => {
                let semantic = restart
                    .stage8b_p1_semantic_commit()
                    .filter(|projection| projection.validate() && projection.intent_count == 1)
                    .cloned()
                    .ok_or(Stage6dLiveCoreError::RestartRuntimeRequired)?;
                let replacement = restart
                    .stage8b_p1d3_replacement()
                    .cloned()
                    .ok_or(Stage6dLiveCoreError::RestartRuntimeRequired)?;
                let position_basis = restart
                    .stage8b_p1d3_position_basis()
                    .ok_or(Stage6dLiveCoreError::Stage8bP1d3WorkingLimit)?;
                (
                    semantic,
                    replacement,
                    position_basis,
                    restart.stage5g_fresh_reconstruction_candidate(),
                )
            }
            Stage6dStage5RuntimeAuthority::FirstBoot(_) => {
                return Err(Stage6dLiveCoreError::RestartRuntimeRequired)
            }
        };
    let identity = semantic
        .durable_request_identity
        .as_ref()
        .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?;
    let command_snapshot = semantic
        .durable_command_snapshot
        .as_ref()
        .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?;
    let command = semantic
        .canonical_command
        .as_ref()
        .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?;
    let BrokerCommand::PlaceOrder(place) = command else {
        return Err(Stage6dLiveCoreError::DurableOrderingViolation);
    };
    let canonical_command_sha256 = semantic
        .canonical_command_sha256
        .as_ref()
        .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?;
    let accepted = stage7a_accepted_record(&recovered, identity.strategy_request_id())
        .cloned()
        .ok_or(Stage6dLiveCoreError::AcceptedRecordRequired)?;
    let decision_m10_close_ts_utc_ms = semantic
        .m10_redis_id
        .strip_suffix("-0")
        .and_then(|value| value.parse::<i64>().ok())
        .filter(|value| *value > 0 && value.rem_euclid(600_000) == 0)
        .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?;
    let decision_m10_open_ts_utc_ms = decision_m10_close_ts_utc_ms
        .checked_sub(600_000)
        .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?;
    let authority =
        recovered.authorize_stage8a4_durable_batch_source(identity, command_snapshot)?;
    if candidate.identity() != identity
        || candidate.command() != command
        || candidate.command_snapshot() != command_snapshot
        || candidate.accepted_record_id() != accepted.journal_record_id()
        || candidate.dispatch_record_id() != authority.dispatch_record_id()
        || candidate.dispatch_sequence() != authority.dispatch_sequence()
        || replacement.authenticated_stage6_checkpoint_sha256()
            != candidate.predecessor_checkpoint_sha256()
        || recovered.journal_frontier().last_record_id() != Some(candidate.dispatch_record_id())
    {
        return Err(Stage6dLiveCoreError::DurableOrderingViolation);
    }
    let limit_price = place
        .limit_price
        .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?;
    let reserved_checkpoint_sha256 = stage8b_p1d3_reservation_sha256(
        candidate.predecessor_checkpoint_sha256(),
        candidate.dispatch_record_id().as_str(),
        identity.strategy_request_id(),
        transition_ts_utc_ms,
        &semantic.m10_semantic_id_sha256,
    );
    let preliminary = crate::stage8b_p1d3_working_limit::Stage8bP1d3InitialLimitInput {
        operational_identity_sha256: replacement
            .working_book()
            .operational_identity_sha256()
            .to_string(),
        package_generation: replacement.working_book().package_generation(),
        account_id: identity.account_id().clone(),
        instrument: identity.instrument().clone(),
        attribution: identity.attribution().clone(),
        request_id: identity.strategy_request_id(),
        durable_client_order_id: identity.durable_client_order_id().clone(),
        canonical_command_sha256: canonical_command_sha256.clone(),
        accepted_command_payload_sha256: accepted.canonical_payload_sha256().as_str().to_string(),
        accepted_stage6_identity_sha256: "11".repeat(32),
        decision_m10_redis_id: semantic.m10_redis_id.clone(),
        decision_m10_semantic_id_sha256: semantic.m10_semantic_id_sha256.clone(),
        decision_m10_payload_sha256: semantic.m10_payload_sha256.clone(),
        decision_m10_open_ts_utc_ms,
        decision_m10_close_ts_utc_ms,
        side: place.side,
        qty: place.qty,
        limit_price,
        pre_position_qty: position_basis.0,
        pre_position_avg_price: position_basis.1,
        sequence_allocation_frontier: replacement.working_book().total_sequence_frontier(),
        stage6_dispatch_record_id: candidate.dispatch_record_id().as_str().to_string(),
        stage6_predecessor_frontier_sha256: candidate.predecessor_checkpoint_sha256().to_string(),
        stage6_reserved_checkpoint_sha256: reserved_checkpoint_sha256,
        previous_outcome_evidence_sha256: replacement
            .previous_outcome_evidence_sha256()
            .to_string(),
    };
    crate::stage8b_p1d3_working_limit::preflight_initial_limit_transition(
        replacement.working_book(),
        &preliminary,
        &observation,
    )?;
    let accepted_stage6_identity_sha256 = authority
        .durable_request_binding_sha256()?
        .as_str()
        .to_string();
    let actual = crate::stage8b_p1d3_working_limit::Stage8bP1d3InitialLimitInput {
        accepted_stage6_identity_sha256,
        stage6_predecessor_frontier_sha256: authority.authenticated_checkpoint_sha256().to_string(),
        ..preliminary
    };
    let plan = crate::stage8b_p1d3_working_limit::build_preflighted_initial_limit_transition(
        replacement.working_book(),
        actual,
        observation,
    )?;
    stage8b_p1d2_test_crash_barrier("p1d4-initial-after-result-before-outcome-wal");
    stage8b_p1d4_test_crash_frontier("F02");
    let receipt = append_stage8b_p1d3_request_outcome(&mut recovered, authority, &plan)?;

    let Stage6dStage5RuntimeAuthority::Restart(restart) = std::mem::replace(
        &mut recovered.stage5_runtime,
        Stage6dStage5RuntimeAuthority::FirstBoot(Box::new(reconstruction_runtime)),
    ) else {
        unreachable!("P1-d4 restart authority was checked above")
    };
    let applied = crate::stage8b_p1d3_working_limit::apply_stage8b_p1d3_request_ack_stage(
        *restart,
        &receipt.outcome_record,
        receipt.recovery_binding,
        commitment_key,
    )?;
    recovered.stage5_runtime = Stage6dStage5RuntimeAuthority::Restart(Box::new(applied.restored));
    recovered.refresh_after_append()?;
    Ok(Stage6Stage8bP1d3AckTransition {
        recovered,
        stage5g_restart_package: applied.restart_package,
    })
}

/// Consumes only a persisted+reread P1-d3 S_ack package. No schedule,
/// provider or Redis authority is accepted by this truth-only continuation.
pub fn apply_stage8b_p1d3_initial_limit_truth_transition(
    mut recovered: Stage6dDurableRuntimeRecovered,
    commitment_key: &Stage5gLifecycleCommitmentKey,
) -> Result<Stage6Stage8bP1d3TruthTransition, Stage6dLiveCoreError> {
    let reconstruction_runtime = match &recovered.stage5_runtime {
        Stage6dStage5RuntimeAuthority::Restart(restart) => {
            let replacement = restart
                .stage8b_p1d3_replacement()
                .ok_or(Stage6dLiveCoreError::RestartRuntimeRequired)?;
            if replacement.phase() != crate::stage8b_p1d3_working_limit::Stage8bP1d3BookPhase::Ack
                || replacement.authenticated_stage6_checkpoint_sha256()
                    != recovered.authenticated_checkpoint().checkpoint_sha256()
            {
                return Err(Stage6dLiveCoreError::DurableOrderingViolation);
            }
            restart.stage5g_fresh_reconstruction_candidate()
        }
        Stage6dStage5RuntimeAuthority::FirstBoot(_) => {
            return Err(Stage6dLiveCoreError::RestartRuntimeRequired)
        }
    };
    let Stage6dStage5RuntimeAuthority::Restart(restart) = std::mem::replace(
        &mut recovered.stage5_runtime,
        Stage6dStage5RuntimeAuthority::FirstBoot(Box::new(reconstruction_runtime)),
    ) else {
        unreachable!("P1-d3 ACK authority was checked above")
    };
    let applied = crate::stage8b_p1d3_working_limit::apply_stage8b_p1d3_truth_after_ack_stage(
        *restart,
        commitment_key,
    )?;
    recovered.stage5_runtime = Stage6dStage5RuntimeAuthority::Restart(Box::new(applied.restored));
    recovered.refresh_after_append()?;
    Ok(Stage6Stage8bP1d3TruthTransition {
        recovered,
        stage5g_restart_package: applied.restart_package,
    })
}

/// Evaluates one authenticated later schedule observation against the active
/// P1-d3 order. All mutable identities and sequence frontiers are recovered
/// from the replacement package. Autonomous fill/expiry is written to Stage
/// 6 before the terminal Stage 5G package is constructed; an untouched bar
/// changes only the replacement package and allocates no sequence.
pub fn apply_stage8b_p1d3_later_limit_transition(
    mut recovered: Stage6dDurableRuntimeRecovered,
    observation: crate::Stage8bP1d3LaterObservation,
    commitment_key: &Stage5gLifecycleCommitmentKey,
) -> Result<Stage6Stage8bP1d3LaterTransition, Stage6dLiveCoreError> {
    let transition_ts_utc_ms = observation.transition_ts_utc_ms();
    let is_day_expiry = matches!(
        &observation,
        crate::Stage8bP1d3LaterObservation::DayExpiry { .. }
    );
    let v4_bound_replacement_checkpoint = match &recovered.stage5_runtime {
        Stage6dStage5RuntimeAuthority::Restart(restart) => restart
            .stage8b_p1d3_replacement()
            .and_then(|replacement| {
                recovered
                    .stage8b_p1e_current_v4_for_replacement(replacement)
                    .ok()
                    .flatten()
            })
            .filter(|record| observation.matches_stage8b_p1e_v4_record(record))
            .map(|_| {
                recovered
                    .authenticated_checkpoint()
                    .checkpoint_sha256()
                    .to_string()
            }),
        Stage6dStage5RuntimeAuthority::FirstBoot(_) => None,
    };
    let (replacement, position_basis, reconstruction_runtime) = match &recovered.stage5_runtime {
        Stage6dStage5RuntimeAuthority::Restart(restart) => {
            let replacement = restart
                .stage8b_p1d3_replacement()
                .filter(|replacement| {
                    matches!(
                        replacement.phase(),
                        crate::stage8b_p1d3_working_limit::Stage8bP1d3BookPhase::Working
                            | crate::stage8b_p1d3_working_limit::Stage8bP1d3BookPhase::Eval
                    ) && (replacement.authenticated_stage6_checkpoint_sha256()
                        == recovered.authenticated_checkpoint().checkpoint_sha256()
                        || v4_bound_replacement_checkpoint.is_some())
                })
                .cloned()
                .ok_or(Stage6dLiveCoreError::RestartRuntimeRequired)?;
            let position_basis = restart
                .stage8b_p1d3_position_basis()
                .ok_or(Stage6dLiveCoreError::Stage8bP1d3WorkingLimit)?;
            (
                replacement,
                position_basis,
                restart.stage5g_fresh_reconstruction_candidate(),
            )
        }
        Stage6dStage5RuntimeAuthority::FirstBoot(_) => {
            return Err(Stage6dLiveCoreError::RestartRuntimeRequired)
        }
    };
    let active = replacement
        .working_book()
        .active_record()
        .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?;
    let predecessor_checkpoint_sha256 = recovered
        .authenticated_checkpoint()
        .checkpoint_sha256()
        .to_string();
    let reservation_sha256 = stage8b_p1d3_reservation_sha256(
        &predecessor_checkpoint_sha256,
        active.broker_order_id().as_str(),
        active.original_request_id(),
        transition_ts_utc_ms,
        replacement.previous_outcome_evidence_sha256(),
    );
    let input = crate::stage8b_p1d3_working_limit::Stage8bP1d3AutonomousInput {
        operational_identity_sha256: replacement
            .working_book()
            .operational_identity_sha256()
            .to_string(),
        package_generation: replacement.working_book().package_generation(),
        account_id: replacement.working_book().account_id().clone(),
        instrument: replacement.working_book().instrument().clone(),
        attribution: replacement.working_book().attribution().clone(),
        pre_position_qty: position_basis.0,
        pre_position_avg_price: position_basis.1,
        sequence_allocation_frontier: replacement.working_book().total_sequence_frontier(),
        stage6_predecessor_frontier_sha256: predecessor_checkpoint_sha256,
        stage6_reserved_checkpoint_sha256: reservation_sha256,
        previous_outcome_evidence_sha256: replacement
            .previous_outcome_evidence_sha256()
            .to_string(),
    };
    let evaluation_result = match observation {
        crate::Stage8bP1d3LaterObservation::Candidate { evidence, schedule } => {
            crate::stage8b_p1d3_working_limit::build_later_limit_transition(
                replacement.working_book(),
                input,
                Some(*evidence),
                Some(schedule),
                None,
            )
        }
        crate::Stage8bP1d3LaterObservation::DayExpiry { authority } => {
            crate::stage8b_p1d3_working_limit::build_later_limit_transition(
                replacement.working_book(),
                input,
                None,
                None,
                Some(authority),
            )
        }
    };
    let evaluation = evaluation_result?;
    match evaluation {
        crate::stage8b_p1d3_working_limit::Stage8bP1d3LaterEvaluationPlan::AlreadyEvaluated {
            book,
        } => {
            if book.encode_canonical()? != replacement.working_book().encode_canonical()? {
                return Err(Stage6dLiveCoreError::DurableOrderingViolation);
            }
            Ok(Stage6Stage8bP1d3LaterTransition::AlreadyEvaluated { recovered })
        }
        evaluation @ crate::stage8b_p1d3_working_limit::Stage8bP1d3LaterEvaluationPlan::Untouched {
            ..
        } => {
            stage8b_p1d4_test_crash_frontier("F12");
            let Stage6dStage5RuntimeAuthority::Restart(restart) = std::mem::replace(
                &mut recovered.stage5_runtime,
                Stage6dStage5RuntimeAuthority::FirstBoot(Box::new(reconstruction_runtime)),
            ) else {
                unreachable!("P1-d3 replacement authority was checked above")
            };
            let applied = match v4_bound_replacement_checkpoint.clone() {
                Some(post_binding_checkpoint) => {
                    crate::stage8b_p1d3_working_limit::apply_stage8b_p1d3_evaluation_stage_after_schedule_binding(
                        *restart,
                        evaluation,
                        replacement
                            .authenticated_stage6_checkpoint_sha256()
                            .to_string(),
                        post_binding_checkpoint,
                        commitment_key,
                    )
                }
                None => crate::stage8b_p1d3_working_limit::apply_stage8b_p1d3_evaluation_stage(
                    *restart,
                    evaluation,
                    commitment_key,
                ),
            }?;
            let crate::stage8b_p1d3_working_limit::Stage8bP1d3EvaluationStageResult::Committed(
                committed,
            ) = applied
            else {
                return Err(Stage6dLiveCoreError::DurableOrderingViolation);
            };
            recovered.stage5_runtime =
                Stage6dStage5RuntimeAuthority::Restart(Box::new(committed.restored));
            recovered.refresh_after_append()?;
            Ok(Stage6Stage8bP1d3LaterTransition::EvaluationCommitted {
                recovered,
                stage5g_restart_package: committed.restart_package,
            })
        }
        crate::stage8b_p1d3_working_limit::Stage8bP1d3LaterEvaluationPlan::Outcome(plan) => {
            stage8b_p1d4_test_crash_frontier(if is_day_expiry { "F17" } else { "F02" });
            let receipt = append_stage8b_p1d3_autonomous_outcome(
                &mut recovered,
                &plan,
                v4_bound_replacement_checkpoint.as_deref(),
            )?;
            stage8b_p1d4_test_crash_frontier(if is_day_expiry { "F18" } else { "F13" });
            let Stage6dStage5RuntimeAuthority::Restart(restart) = std::mem::replace(
                &mut recovered.stage5_runtime,
                Stage6dStage5RuntimeAuthority::FirstBoot(Box::new(reconstruction_runtime)),
            ) else {
                unreachable!("P1-d3 replacement authority was checked above")
            };
            let committed_result = match v4_bound_replacement_checkpoint {
                Some(schedule_checkpoint) => {
                    crate::stage8b_p1d3_working_limit::apply_stage8b_p1d3_autonomous_truth_stage_after_schedule_binding(
                        *restart,
                        &receipt.outcome_record,
                        receipt.recovery_binding,
                        schedule_checkpoint,
                        commitment_key,
                    )
                }
                None => {
                    crate::stage8b_p1d3_working_limit::apply_stage8b_p1d3_autonomous_truth_stage(
                        *restart,
                        &receipt.outcome_record,
                        receipt.recovery_binding,
                        commitment_key,
                    )
                }
            };
            let committed = committed_result?;
            recovered.stage5_runtime =
                Stage6dStage5RuntimeAuthority::Restart(Box::new(committed.restored));
            recovered.refresh_after_append()?;
            stage8b_p1d4_test_crash_frontier("F07");
            Ok(Stage6Stage8bP1d3LaterTransition::TruthCommitted {
                recovered,
                stage5g_restart_package: committed.restart_package,
            })
        }
    }
}

/// Evaluates the first schedule-approved candidate for one authenticated
/// CANCEL command. The target LIMIT is always evaluated first. A same-bar
/// target fill is committed as autonomous target truth and returned for an
/// intermediate replacement seal; all other cancel outcomes are journaled
/// and reduced immediately to S_ack or S_cancel_recovered.
fn canonical_stage8b_p1d3_cancel_target_client_id(
    durable_cancel_client_order_id: &ClientOrderId,
    supplied_target_client_order_id: Option<&ClientOrderId>,
    authenticated_target_client_order_id: &ClientOrderId,
) -> Result<ClientOrderId, Stage6dLiveCoreError> {
    if durable_cancel_client_order_id == authenticated_target_client_order_id
        || supplied_target_client_order_id
            .is_some_and(|supplied| supplied != authenticated_target_client_order_id)
    {
        return Err(Stage6dLiveCoreError::DurableOrderingViolation);
    }
    Ok(authenticated_target_client_order_id.clone())
}

pub fn apply_stage8b_p1d3_cancel_transition(
    mut recovered: Stage6dDurableRuntimeRecovered,
    candidate: crate::Stage8bP1d3CanonicalM10Evidence,
    schedule: crate::Stage8bP1d3ScheduleStepAuthority,
    commitment_key: &Stage5gLifecycleCommitmentKey,
) -> Result<Stage6Stage8bP1d3CancelTransition, Stage6dLiveCoreError> {
    let (semantic, replacement, position_basis, reconstruction_runtime, binding_v4) =
        match &recovered.stage5_runtime {
            Stage6dStage5RuntimeAuthority::Restart(restart) => {
                let semantic = restart
                    .stage8b_p1_semantic_commit()
                    .filter(|projection| projection.validate() && projection.intent_count == 1)
                    .cloned()
                    .ok_or(Stage6dLiveCoreError::RestartRuntimeRequired)?;
                let replacement = restart
                    .stage8b_p1d3_replacement()
                    .cloned()
                    .ok_or(Stage6dLiveCoreError::RestartRuntimeRequired)?;
                if !matches!(
                    replacement.phase(),
                    crate::stage8b_p1d3_working_limit::Stage8bP1d3BookPhase::Working
                        | crate::stage8b_p1d3_working_limit::Stage8bP1d3BookPhase::Eval
                        | crate::stage8b_p1d3_working_limit::Stage8bP1d3BookPhase::Terminal
                ) {
                    return Err(Stage6dLiveCoreError::RestartRuntimeRequired);
                }
                let binding_v4 = recovered
                    .stage8b_p1e_current_v4_for_replacement(&replacement)?
                    .filter(|record| schedule.matches_stage8b_p1e_v4_record(record, &candidate))
                    .cloned();
                if replacement.authenticated_stage6_checkpoint_sha256()
                    != recovered.authenticated_checkpoint().checkpoint_sha256()
                    && binding_v4.is_none()
                {
                    return Err(Stage6dLiveCoreError::RestartRuntimeRequired);
                }
                let position_basis = restart
                    .stage8b_p1d3_position_basis()
                    .ok_or(Stage6dLiveCoreError::Stage8bP1d3WorkingLimit)?;
                (
                    semantic,
                    replacement,
                    position_basis,
                    restart.stage5g_fresh_reconstruction_candidate(),
                    binding_v4,
                )
            }
            Stage6dStage5RuntimeAuthority::FirstBoot(_) => {
                return Err(Stage6dLiveCoreError::RestartRuntimeRequired)
            }
        };
    let request_id = semantic
        .request_id
        .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?;
    let command = semantic
        .canonical_command
        .as_ref()
        .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?;
    let BrokerCommand::CancelOrder(cancel) = command else {
        return Err(Stage6dLiveCoreError::DurableOrderingViolation);
    };
    let identity = semantic
        .durable_request_identity
        .as_ref()
        .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?;
    let command_snapshot = semantic
        .durable_command_snapshot
        .as_ref()
        .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?;
    let canonical_command_sha256 = semantic
        .canonical_command_sha256
        .as_ref()
        .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?;
    let decision_m10_close_ts_utc_ms = semantic
        .m10_redis_id
        .strip_suffix("-0")
        .and_then(|value| value.parse::<i64>().ok())
        .filter(|value| *value > 0 && value.rem_euclid(600_000) == 0)
        .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?;
    let decision_m10_open_ts_utc_ms = decision_m10_close_ts_utc_ms
        .checked_sub(600_000)
        .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?;
    let accepted = stage7a_accepted_record(&recovered, request_id)
        .cloned()
        .ok_or(Stage6dLiveCoreError::AcceptedRecordRequired)?;
    let accepted_snapshot = match accepted.payload() {
        Stage6JournalPayloadV1::RequestAccepted { command } => command.as_ref(),
        _ => return Err(Stage6dLiveCoreError::AcceptedRecordRequired),
    };
    let replayed = recovered
        .replay()
        .request(request_id)
        .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?;
    let command_sha256 = sha256_hex(
        &serde_json::to_vec(command).map_err(|_| Stage6dLiveCoreError::IntegrationFingerprint)?,
    );
    let target_record = replacement
        .working_book()
        .record(&cancel.order_id)
        .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?;
    // The strategy command is allowed to omit the target TCID. Resolve the
    // canonical value from the authenticated P1-d3 registry and reject a
    // truncated cancel-DCID collision before DispatchAttemptRecorded exists.
    let canonical_target_place_client_id = canonical_stage8b_p1d3_cancel_target_client_id(
        identity.durable_client_order_id(),
        cancel.client_order_id.as_ref(),
        target_record.original_client_order_id(),
    )?;
    if request_id != cancel.request_id
        || identity.strategy_request_id() != request_id
        || identity.action() != Stage6DurableActionKind::Cancel
        || identity.account_id() != replacement.working_book().account_id()
        || identity.instrument() != replacement.working_book().instrument()
        || identity.attribution().role() != Some(broker_core::HybridRuntimeOrderRole::Cancel)
        || identity.target_broker_order_id() != Some(&cancel.order_id)
        || identity.target_order_client_order_id() != cancel.client_order_id.as_ref()
        || cancel.account_id != *identity.account_id()
        || cancel.ttl_ms.is_some()
        || cancel
            .client_order_id
            .as_ref()
            .is_some_and(|value| value != target_record.original_client_order_id())
        || accepted.durable_request_identity() != identity
        || accepted_snapshot != command_snapshot
        || command_sha256 != *canonical_command_sha256
        || semantic.operational_identity_sha256
            != replacement.working_book().operational_identity_sha256()
        || replayed.dispatch_safety_state()
            != crate::Stage6DispatchSafetyStateV1::ReadyForFirstDispatch
        || binding_v4.as_ref().map_or_else(
            || {
                replayed.last_unique_record_id() != accepted.journal_record_id()
                    || recovered.journal_frontier().last_record_id()
                        != Some(accepted.journal_record_id())
            },
            |record| {
                replayed.last_unique_record_id() != record.journal_record_id()
                    || recovered.journal_frontier().last_record_id()
                        != Some(record.journal_record_id())
            },
        )
    {
        return Err(Stage6dLiveCoreError::DurableOrderingViolation);
    }
    let target_first = crate::stage8b_p1d3_working_limit::cancel_candidate_requires_target_outcome(
        replacement.working_book(),
        &cancel.order_id,
        Some(&canonical_target_place_client_id),
        decision_m10_close_ts_utc_ms,
        &candidate,
        &schedule,
    )?;
    let transition_ts = Utc
        .timestamp_millis_opt(candidate.close_ts_utc_ms)
        .single()
        .ok_or(Stage6dLiveCoreError::Stage8bP1d3WorkingLimit)?;
    let dispatch = if let Some(record) = binding_v4.as_ref() {
        stage7a_dispatch_record_after(
            identity,
            &accepted,
            record.lifecycle_sequence(),
            record.journal_record_id(),
            transition_ts,
        )?
    } else {
        stage7a_dispatch_record(identity, &accepted, transition_ts)?
    };
    let dispatch_record_id = dispatch.journal_record_id().as_str().to_string();
    let pre_dispatch_checkpoint_sha256 = recovered
        .authenticated_checkpoint()
        .checkpoint_sha256()
        .to_string();
    let target_reservation = target_first.then(|| {
        stage8b_p1d3_reservation_sha256(
            &pre_dispatch_checkpoint_sha256,
            &dispatch_record_id,
            target_record.original_request_id(),
            candidate.close_ts_utc_ms,
            replacement.previous_outcome_evidence_sha256(),
        )
    });
    let cancel_reservation = stage8b_p1d3_reservation_sha256(
        target_reservation
            .as_deref()
            .unwrap_or(pre_dispatch_checkpoint_sha256.as_str()),
        &dispatch_record_id,
        request_id,
        candidate.close_ts_utc_ms,
        replacement.previous_outcome_evidence_sha256(),
    );

    let _dispatch_receipt = if let Some(record) = binding_v4.as_ref() {
        prepare_stage6d_existing_accepted_paper_dispatch_after_schedule_binding(
            &mut recovered,
            &accepted,
            dispatch,
            record,
        )?
    } else {
        prepare_stage6d_existing_accepted_paper_dispatch(&mut recovered, &accepted, dispatch)?
    };
    stage8b_p1d2_test_crash_barrier("p1d4-cancel-after-dispatch-before-authority-consumption");
    stage8b_p1d4_test_crash_frontier("F20");
    let authority =
        recovered.authorize_stage8a4_durable_batch_source(identity, command_snapshot)?;
    let accepted_stage6_identity_sha256 = authority
        .durable_request_binding_sha256()?
        .as_str()
        .to_string();
    let input = crate::stage8b_p1d3_working_limit::Stage8bP1d3CancelInput {
        operational_identity_sha256: replacement
            .working_book()
            .operational_identity_sha256()
            .to_string(),
        package_generation: replacement.working_book().package_generation(),
        account_id: identity.account_id().clone(),
        instrument: identity.instrument().clone(),
        attribution: identity.attribution().clone(),
        request_id,
        durable_request_client_id: identity.durable_client_order_id().clone(),
        canonical_command_sha256: canonical_command_sha256.clone(),
        accepted_command_payload_sha256: accepted.canonical_payload_sha256().as_str().to_string(),
        accepted_stage6_identity_sha256,
        target_broker_order_id: cancel.order_id.clone(),
        target_place_client_id: Some(canonical_target_place_client_id),
        decision_m10_redis_id: semantic.m10_redis_id.clone(),
        decision_m10_semantic_id_sha256: semantic.m10_semantic_id_sha256.clone(),
        decision_m10_payload_sha256: semantic.m10_payload_sha256.clone(),
        decision_m10_open_ts_utc_ms,
        decision_m10_close_ts_utc_ms,
        pre_position_qty: position_basis.0,
        pre_position_avg_price: position_basis.1,
        sequence_allocation_frontier: replacement.working_book().total_sequence_frontier(),
        stage6_dispatch_record_id: dispatch_record_id,
        stage6_dispatch_lifecycle_sequence: authority.dispatch_sequence(),
        stage6_predecessor_frontier_sha256: authority.authenticated_checkpoint_sha256().to_string(),
        target_stage6_reserved_checkpoint_sha256: target_reservation,
        stage6_reserved_checkpoint_sha256: cancel_reservation,
        previous_outcome_evidence_sha256: replacement
            .previous_outcome_evidence_sha256()
            .to_string(),
    };
    let plan = crate::stage8b_p1d3_working_limit::build_cancel_transition(
        replacement.working_book(),
        input,
        candidate,
        schedule,
    )?;
    stage8b_p1d2_test_crash_barrier("p1d4-cancel-after-result-before-outcome-wal");
    stage8b_p1d4_test_crash_frontier("F02");

    match plan {
        crate::stage8b_p1d3_working_limit::Stage8bP1d3CancelTransitionPlan::Ready(plan) => {
            let kind = plan.evidence.outcome_kind();
            let receipt = append_stage8b_p1d3_request_outcome(&mut recovered, authority, &plan)?;
            let Stage6dStage5RuntimeAuthority::Restart(restart) = std::mem::replace(
                &mut recovered.stage5_runtime,
                Stage6dStage5RuntimeAuthority::FirstBoot(Box::new(reconstruction_runtime)),
            ) else {
                unreachable!("P1-d3 restart authority was checked above")
            };
            let applied = crate::stage8b_p1d3_working_limit::apply_stage8b_p1d3_request_ack_stage(
                *restart,
                &receipt.outcome_record,
                receipt.recovery_binding,
                commitment_key,
            )?;
            recovered.stage5_runtime =
                Stage6dStage5RuntimeAuthority::Restart(Box::new(applied.restored));
            recovered.refresh_after_append()?;
            if kind == crate::stage8b_p1d3_working_limit::Stage8bP1d3OutcomeKind::CancelCanceled {
                Ok(Stage6Stage8bP1d3CancelTransition::AckCommitted {
                    recovered,
                    stage5g_restart_package: applied.restart_package,
                })
            } else {
                Ok(Stage6Stage8bP1d3CancelTransition::RecoveredCommitted {
                    recovered,
                    stage5g_restart_package: applied.restart_package,
                })
            }
        }
        crate::stage8b_p1d3_working_limit::Stage8bP1d3CancelTransitionPlan::TargetThenCancel {
            target_transition,
            ..
        } => {
            let receipt = append_stage8b_p1d3_cancel_target_outcome(
                &mut recovered,
                &authority,
                &target_transition,
            )?;
            stage8b_p1d4_test_crash_frontier("F13");
            let Stage6dStage5RuntimeAuthority::Restart(restart) = std::mem::replace(
                &mut recovered.stage5_runtime,
                Stage6dStage5RuntimeAuthority::FirstBoot(Box::new(reconstruction_runtime)),
            ) else {
                unreachable!("P1-d3 restart authority was checked above")
            };
            let applied =
                crate::stage8b_p1d3_working_limit::apply_stage8b_p1d3_cancel_target_truth_stage(
                    *restart,
                    &receipt.outcome_record,
                    receipt.recovery_binding,
                    commitment_key,
                )?;
            recovered.stage5_runtime =
                Stage6dStage5RuntimeAuthority::Restart(Box::new(applied.restored));
            recovered.refresh_after_append()?;
            Ok(Stage6Stage8bP1d3CancelTransition::TargetTruthCommitted {
                recovered,
                stage5g_restart_package: applied.restart_package,
            })
        }
    }
}

/// Continues a canonical CANCEL after restart from its exact already-durable
/// dispatch row.  Target-first settlement deliberately returns the existing
/// two-stage continuation; no path in this function can append a dispatch.
pub fn resume_stage8b_p1d3_dispatch_only_cancel_transition(
    mut recovered: Stage6dDurableRuntimeRecovered,
    candidate: Stage6Stage8bP1d3DispatchOnlyCandidate,
    bar: crate::Stage8bP1d3CanonicalM10Evidence,
    schedule: crate::Stage8bP1d3ScheduleStepAuthority,
    commitment_key: &Stage5gLifecycleCommitmentKey,
) -> Result<Stage6Stage8bP1d3CancelTransition, Stage6dLiveCoreError> {
    if !candidate.is_cancel() {
        return Err(Stage6dLiveCoreError::DurableOrderingViolation);
    }
    let (semantic, replacement, position_basis, reconstruction_runtime) =
        match &recovered.stage5_runtime {
            Stage6dStage5RuntimeAuthority::Restart(restart) => {
                let semantic = restart
                    .stage8b_p1_semantic_commit()
                    .filter(|projection| projection.validate() && projection.intent_count == 1)
                    .cloned()
                    .ok_or(Stage6dLiveCoreError::RestartRuntimeRequired)?;
                let replacement = restart
                    .stage8b_p1d3_replacement()
                    .cloned()
                    .ok_or(Stage6dLiveCoreError::RestartRuntimeRequired)?;
                let position_basis = restart
                    .stage8b_p1d3_position_basis()
                    .ok_or(Stage6dLiveCoreError::Stage8bP1d3WorkingLimit)?;
                (
                    semantic,
                    replacement,
                    position_basis,
                    restart.stage5g_fresh_reconstruction_candidate(),
                )
            }
            Stage6dStage5RuntimeAuthority::FirstBoot(_) => {
                return Err(Stage6dLiveCoreError::RestartRuntimeRequired)
            }
        };
    let identity = semantic
        .durable_request_identity
        .as_ref()
        .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?;
    let command_snapshot = semantic
        .durable_command_snapshot
        .as_ref()
        .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?;
    let command = semantic
        .canonical_command
        .as_ref()
        .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?;
    let BrokerCommand::CancelOrder(cancel) = command else {
        return Err(Stage6dLiveCoreError::DurableOrderingViolation);
    };
    let canonical_command_sha256 = semantic
        .canonical_command_sha256
        .as_ref()
        .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?;
    let accepted = stage7a_accepted_record(&recovered, identity.strategy_request_id())
        .cloned()
        .ok_or(Stage6dLiveCoreError::AcceptedRecordRequired)?;
    let target_record = replacement
        .working_book()
        .record(&cancel.order_id)
        .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?;
    let canonical_target_place_client_id = canonical_stage8b_p1d3_cancel_target_client_id(
        identity.durable_client_order_id(),
        cancel.client_order_id.as_ref(),
        target_record.original_client_order_id(),
    )?;
    let decision_m10_close_ts_utc_ms = semantic
        .m10_redis_id
        .strip_suffix("-0")
        .and_then(|value| value.parse::<i64>().ok())
        .filter(|value| *value > 0 && value.rem_euclid(600_000) == 0)
        .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?;
    let decision_m10_open_ts_utc_ms = decision_m10_close_ts_utc_ms
        .checked_sub(600_000)
        .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?;
    let authority =
        recovered.authorize_stage8a4_durable_batch_source(identity, command_snapshot)?;
    if candidate.identity() != identity
        || candidate.command() != command
        || candidate.command_snapshot() != command_snapshot
        || candidate.accepted_record_id() != accepted.journal_record_id()
        || candidate.dispatch_record_id() != authority.dispatch_record_id()
        || candidate.dispatch_sequence() != authority.dispatch_sequence()
        || replacement.authenticated_stage6_checkpoint_sha256()
            != candidate.predecessor_checkpoint_sha256()
        || recovered.journal_frontier().last_record_id() != Some(candidate.dispatch_record_id())
    {
        return Err(Stage6dLiveCoreError::DurableOrderingViolation);
    }

    let target_first = crate::stage8b_p1d3_working_limit::cancel_candidate_requires_target_outcome(
        replacement.working_book(),
        &cancel.order_id,
        Some(&canonical_target_place_client_id),
        decision_m10_close_ts_utc_ms,
        &bar,
        &schedule,
    )?;
    let target_reservation = target_first.then(|| {
        stage8b_p1d3_reservation_sha256(
            candidate.predecessor_checkpoint_sha256(),
            candidate.dispatch_record_id().as_str(),
            target_record.original_request_id(),
            bar.close_ts_utc_ms,
            replacement.previous_outcome_evidence_sha256(),
        )
    });
    let cancel_reservation = stage8b_p1d3_reservation_sha256(
        target_reservation
            .as_deref()
            .unwrap_or(candidate.predecessor_checkpoint_sha256()),
        candidate.dispatch_record_id().as_str(),
        identity.strategy_request_id(),
        bar.close_ts_utc_ms,
        replacement.previous_outcome_evidence_sha256(),
    );
    let accepted_stage6_identity_sha256 = authority
        .durable_request_binding_sha256()?
        .as_str()
        .to_string();
    let input = crate::stage8b_p1d3_working_limit::Stage8bP1d3CancelInput {
        operational_identity_sha256: replacement
            .working_book()
            .operational_identity_sha256()
            .to_string(),
        package_generation: replacement.working_book().package_generation(),
        account_id: identity.account_id().clone(),
        instrument: identity.instrument().clone(),
        attribution: identity.attribution().clone(),
        request_id: identity.strategy_request_id(),
        durable_request_client_id: identity.durable_client_order_id().clone(),
        canonical_command_sha256: canonical_command_sha256.clone(),
        accepted_command_payload_sha256: accepted.canonical_payload_sha256().as_str().to_string(),
        accepted_stage6_identity_sha256,
        target_broker_order_id: cancel.order_id.clone(),
        target_place_client_id: Some(canonical_target_place_client_id),
        decision_m10_redis_id: semantic.m10_redis_id.clone(),
        decision_m10_semantic_id_sha256: semantic.m10_semantic_id_sha256.clone(),
        decision_m10_payload_sha256: semantic.m10_payload_sha256.clone(),
        decision_m10_open_ts_utc_ms,
        decision_m10_close_ts_utc_ms,
        pre_position_qty: position_basis.0,
        pre_position_avg_price: position_basis.1,
        sequence_allocation_frontier: replacement.working_book().total_sequence_frontier(),
        stage6_dispatch_record_id: candidate.dispatch_record_id().as_str().to_string(),
        stage6_dispatch_lifecycle_sequence: authority.dispatch_sequence(),
        stage6_predecessor_frontier_sha256: authority.authenticated_checkpoint_sha256().to_string(),
        target_stage6_reserved_checkpoint_sha256: target_reservation,
        stage6_reserved_checkpoint_sha256: cancel_reservation,
        previous_outcome_evidence_sha256: replacement
            .previous_outcome_evidence_sha256()
            .to_string(),
    };
    let plan = crate::stage8b_p1d3_working_limit::build_cancel_transition(
        replacement.working_book(),
        input,
        bar,
        schedule,
    )?;
    stage8b_p1d2_test_crash_barrier("p1d4-cancel-after-result-before-outcome-wal");
    stage8b_p1d4_test_crash_frontier("F02");

    match plan {
        crate::stage8b_p1d3_working_limit::Stage8bP1d3CancelTransitionPlan::Ready(plan) => {
            let kind = plan.evidence.outcome_kind();
            let receipt = append_stage8b_p1d3_request_outcome(&mut recovered, authority, &plan)?;
            let Stage6dStage5RuntimeAuthority::Restart(restart) = std::mem::replace(
                &mut recovered.stage5_runtime,
                Stage6dStage5RuntimeAuthority::FirstBoot(Box::new(reconstruction_runtime)),
            ) else {
                unreachable!("P1-d4 restart authority was checked above")
            };
            let applied = crate::stage8b_p1d3_working_limit::apply_stage8b_p1d3_request_ack_stage(
                *restart,
                &receipt.outcome_record,
                receipt.recovery_binding,
                commitment_key,
            )?;
            recovered.stage5_runtime =
                Stage6dStage5RuntimeAuthority::Restart(Box::new(applied.restored));
            recovered.refresh_after_append()?;
            if kind == crate::stage8b_p1d3_working_limit::Stage8bP1d3OutcomeKind::CancelCanceled {
                Ok(Stage6Stage8bP1d3CancelTransition::AckCommitted {
                    recovered,
                    stage5g_restart_package: applied.restart_package,
                })
            } else {
                Ok(Stage6Stage8bP1d3CancelTransition::RecoveredCommitted {
                    recovered,
                    stage5g_restart_package: applied.restart_package,
                })
            }
        }
        crate::stage8b_p1d3_working_limit::Stage8bP1d3CancelTransitionPlan::TargetThenCancel {
            target_transition,
            ..
        } => {
            let receipt = append_stage8b_p1d3_cancel_target_outcome(
                &mut recovered,
                &authority,
                &target_transition,
            )?;
            stage8b_p1d4_test_crash_frontier("F13");
            stage8b_p1d4_test_crash_frontier("F07");
            let Stage6dStage5RuntimeAuthority::Restart(restart) = std::mem::replace(
                &mut recovered.stage5_runtime,
                Stage6dStage5RuntimeAuthority::FirstBoot(Box::new(reconstruction_runtime)),
            ) else {
                unreachable!("P1-d4 restart authority was checked above")
            };
            let applied =
                crate::stage8b_p1d3_working_limit::apply_stage8b_p1d3_cancel_target_truth_stage(
                    *restart,
                    &receipt.outcome_record,
                    receipt.recovery_binding,
                    commitment_key,
                )?;
            recovered.stage5_runtime =
                Stage6dStage5RuntimeAuthority::Restart(Box::new(applied.restored));
            recovered.refresh_after_append()?;
            Ok(Stage6Stage8bP1d3CancelTransition::TargetTruthCommitted {
                recovered,
                stage5g_restart_package: applied.restart_package,
            })
        }
    }
}

/// Continues a cancel whose first successor M10 already filled the target and
/// whose autonomous target truth is covered by a reread `S_terminal`.  Every
/// input needed to construct the recovered cancel outcome is reconstructed
/// from authenticated journal/restart state; callers cannot supply another
/// bar, schedule witness, provider result or dispatch authority.
pub fn continue_stage8b_p1d3_cancel_after_target_transition(
    mut recovered: Stage6dDurableRuntimeRecovered,
    commitment_key: &Stage5gLifecycleCommitmentKey,
) -> Result<Stage6Stage8bP1d3CancelTransition, Stage6dLiveCoreError> {
    let (semantic, replacement, position_basis, reconstruction_runtime) =
        match &recovered.stage5_runtime {
            Stage6dStage5RuntimeAuthority::Restart(restart) => {
                let semantic = restart
                    .stage8b_p1_semantic_commit()
                    .filter(|projection| projection.validate() && projection.intent_count == 1)
                    .cloned()
                    .ok_or(Stage6dLiveCoreError::RestartRuntimeRequired)?;
                let replacement = restart
                    .stage8b_p1d3_replacement()
                    .filter(|replacement| {
                        replacement
                            .pending_cancel_after_target_matches_semantic_commit(Some(&semantic))
                            .unwrap_or(false)
                            && replacement.authenticated_stage6_checkpoint_sha256()
                                == recovered.authenticated_checkpoint().checkpoint_sha256()
                    })
                    .cloned()
                    .ok_or(Stage6dLiveCoreError::RestartRuntimeRequired)?;
                let position_basis = restart
                    .stage8b_p1d3_position_basis()
                    .ok_or(Stage6dLiveCoreError::Stage8bP1d3WorkingLimit)?;
                (
                    semantic,
                    replacement,
                    position_basis,
                    restart.stage5g_fresh_reconstruction_candidate(),
                )
            }
            Stage6dStage5RuntimeAuthority::FirstBoot(_) => {
                return Err(Stage6dLiveCoreError::RestartRuntimeRequired)
            }
        };
    let request_id = semantic
        .request_id
        .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?;
    let command = semantic
        .canonical_command
        .as_ref()
        .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?;
    let BrokerCommand::CancelOrder(cancel) = command else {
        return Err(Stage6dLiveCoreError::DurableOrderingViolation);
    };
    let identity = semantic
        .durable_request_identity
        .as_ref()
        .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?;
    let command_snapshot = semantic
        .durable_command_snapshot
        .as_ref()
        .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?;
    let canonical_command_sha256 = semantic
        .canonical_command_sha256
        .as_ref()
        .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?;
    let accepted = stage7a_accepted_record(&recovered, request_id)
        .cloned()
        .ok_or(Stage6dLiveCoreError::AcceptedRecordRequired)?;
    let accepted_snapshot = match accepted.payload() {
        Stage6JournalPayloadV1::RequestAccepted { command } => command.as_ref(),
        _ => return Err(Stage6dLiveCoreError::AcceptedRecordRequired),
    };
    let target_evidence = replacement
        .latest_outcome_evidence()
        .filter(|evidence| {
            evidence.outcome_kind()
                == crate::stage8b_p1d3_working_limit::Stage8bP1d3OutcomeKind::LaterFilled
                && evidence.request_id().is_none()
        })
        .cloned()
        .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?;
    let target_record = replacement
        .working_book()
        .record(&cancel.order_id)
        .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?;
    let canonical_target_place_client_id = canonical_stage8b_p1d3_cancel_target_client_id(
        identity.durable_client_order_id(),
        cancel.client_order_id.as_ref(),
        target_record.original_client_order_id(),
    )?;
    let authority =
        recovered.authorize_stage8a4_durable_batch_source(identity, command_snapshot)?;
    let accepted_stage6_identity_sha256 = authority
        .durable_request_binding_sha256()?
        .as_str()
        .to_string();
    let command_sha256 = sha256_hex(
        &serde_json::to_vec(command).map_err(|_| Stage6dLiveCoreError::IntegrationFingerprint)?,
    );
    let decision_m10_close_ts_utc_ms = semantic
        .m10_redis_id
        .strip_suffix("-0")
        .and_then(|value| value.parse::<i64>().ok())
        .filter(|value| *value > 0 && value.rem_euclid(600_000) == 0)
        .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?;
    let decision_m10_open_ts_utc_ms = decision_m10_close_ts_utc_ms
        .checked_sub(600_000)
        .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?;
    let target_evidence_sha256 = target_evidence.digest_sha256()?;
    let cancel_reservation = stage8b_p1d3_reservation_sha256(
        target_evidence.stage6_reserved_checkpoint_sha256(),
        authority.dispatch_record_id().as_str(),
        request_id,
        target_evidence.transition_received_ts_utc_ms(),
        target_evidence.previous_outcome_evidence_sha256(),
    );
    if identity.action() != Stage6DurableActionKind::Cancel
        || identity.strategy_request_id() != request_id
        || identity.target_broker_order_id() != Some(&cancel.order_id)
        || accepted.durable_request_identity() != identity
        || accepted_snapshot != command_snapshot
        || command_sha256 != *canonical_command_sha256
        || target_evidence.broker_order_id() != &cancel.order_id
    {
        return Err(Stage6dLiveCoreError::DurableOrderingViolation);
    }
    let input = crate::stage8b_p1d3_working_limit::Stage8bP1d3CancelInput {
        operational_identity_sha256: replacement
            .working_book()
            .operational_identity_sha256()
            .to_string(),
        package_generation: replacement.working_book().package_generation(),
        account_id: identity.account_id().clone(),
        instrument: identity.instrument().clone(),
        attribution: identity.attribution().clone(),
        request_id,
        durable_request_client_id: identity.durable_client_order_id().clone(),
        canonical_command_sha256: canonical_command_sha256.clone(),
        accepted_command_payload_sha256: accepted.canonical_payload_sha256().as_str().to_string(),
        accepted_stage6_identity_sha256,
        target_broker_order_id: cancel.order_id.clone(),
        target_place_client_id: Some(canonical_target_place_client_id),
        decision_m10_redis_id: semantic.m10_redis_id.clone(),
        decision_m10_semantic_id_sha256: semantic.m10_semantic_id_sha256.clone(),
        decision_m10_payload_sha256: semantic.m10_payload_sha256.clone(),
        decision_m10_open_ts_utc_ms,
        decision_m10_close_ts_utc_ms,
        pre_position_qty: position_basis.0,
        pre_position_avg_price: position_basis.1,
        sequence_allocation_frontier: replacement.working_book().total_sequence_frontier(),
        stage6_dispatch_record_id: authority.dispatch_record_id().as_str().to_string(),
        stage6_dispatch_lifecycle_sequence: authority.dispatch_sequence(),
        stage6_predecessor_frontier_sha256: authority.authenticated_checkpoint_sha256().to_string(),
        target_stage6_reserved_checkpoint_sha256: None,
        stage6_reserved_checkpoint_sha256: cancel_reservation,
        previous_outcome_evidence_sha256: target_evidence_sha256,
    };
    let plan = crate::stage8b_p1d3_working_limit::build_cancel_after_target_transition(
        replacement.working_book(),
        input,
        &target_evidence,
    )?;
    let receipt = append_stage8b_p1d3_request_outcome(&mut recovered, authority, &plan)?;
    let Stage6dStage5RuntimeAuthority::Restart(restart) = std::mem::replace(
        &mut recovered.stage5_runtime,
        Stage6dStage5RuntimeAuthority::FirstBoot(Box::new(reconstruction_runtime)),
    ) else {
        unreachable!("P1-d3 cancel continuation authority was checked above")
    };
    let applied = crate::stage8b_p1d3_working_limit::apply_stage8b_p1d3_request_ack_stage(
        *restart,
        &receipt.outcome_record,
        receipt.recovery_binding,
        commitment_key,
    )?;
    recovered.stage5_runtime = Stage6dStage5RuntimeAuthority::Restart(Box::new(applied.restored));
    recovered.refresh_after_append()?;
    Ok(Stage6Stage8bP1d3CancelTransition::RecoveredCommitted {
        recovered,
        stage5g_restart_package: applied.restart_package,
    })
}

/// Completes the sole pre-seal P1-d3 recovery path from an already durable
/// V3 outcome.  The candidate contains the exact embedded outcome evidence;
/// this function never accepts schedule, market-data or provider input and
/// therefore cannot repeat the external decision after a crash.
#[doc(hidden)]
pub fn resume_stage8b_p1d3_initial_limit_ack_transition(
    recovered: Stage6dDurableRuntimeRecovered,
    candidate: Stage6Stage8bP1d3JournalAheadCandidate,
    commitment_key: &Stage5gLifecycleCommitmentKey,
) -> Result<Stage6Stage8bP1d3AckTransition, Stage6dLiveCoreError> {
    let Stage6Stage8bP1d3RecoveredTransition::AckCommitted(transition) =
        resume_stage8b_p1d3_journal_ahead_transition(recovered, candidate, commitment_key)?
    else {
        return Err(Stage6dLiveCoreError::DurableOrderingViolation);
    };
    Ok(transition)
}

/// Reconstructs the exact replacement transition selected by an authenticated
/// P1-d3 journal-ahead suffix. Request outcomes resolve through ACK; an
/// autonomous outcome resolves through truth. A target fill under a sealed
/// cancel semantic remains a no-input cancel continuation after its target
/// replacement has been committed.
pub fn resume_stage8b_p1d3_journal_ahead_transition(
    mut recovered: Stage6dDurableRuntimeRecovered,
    candidate: Stage6Stage8bP1d3JournalAheadCandidate,
    commitment_key: &Stage5gLifecycleCommitmentKey,
) -> Result<Stage6Stage8bP1d3RecoveredTransition, Stage6dLiveCoreError> {
    let (reconstruction_runtime, schedule_checkpoint) = match &recovered.stage5_runtime {
        Stage6dStage5RuntimeAuthority::Restart(restart)
            if restart.stage8b_p1d3_replacement().is_some() =>
        {
            let replacement = restart
                .stage8b_p1d3_replacement()
                .expect("guarded replacement exists");
            (
                restart.stage5g_fresh_reconstruction_candidate(),
                recovered.stage8b_p1e_schedule_checkpoint_before_journal_ahead_outcome(
                    replacement,
                    &candidate.outcome_record,
                )?,
            )
        }
        _ => return Err(Stage6dLiveCoreError::RestartRuntimeRequired),
    };
    let receipt = resume_stage8b_p1d3_journal_ahead_candidate(&mut recovered, candidate)?;
    let evidence =
        crate::stage8b_p1d3_working_limit::Stage8bP1d3OutcomeEvidenceV1::decode_canonical(
            receipt.outcome_record.outcome_evidence_bytes(),
        )?;
    let request_scoped = evidence.is_request_scoped();
    let cancel_target = !request_scoped
        && matches!(
            &recovered.stage5_runtime,
            Stage6dStage5RuntimeAuthority::Restart(restart)
                if restart.stage8b_p1_semantic_commit().is_some_and(|semantic| {
                    matches!(
                        semantic.canonical_command.as_ref(),
                        Some(BrokerCommand::CancelOrder(cancel))
                            if cancel.order_id == *evidence.broker_order_id()
                    )
                })
        );
    let Stage6dStage5RuntimeAuthority::Restart(restart) = std::mem::replace(
        &mut recovered.stage5_runtime,
        Stage6dStage5RuntimeAuthority::FirstBoot(Box::new(reconstruction_runtime)),
    ) else {
        unreachable!("P1-d3 restart authority was checked above")
    };
    let applied = if request_scoped {
        crate::stage8b_p1d3_working_limit::apply_stage8b_p1d3_request_ack_stage(
            *restart,
            &receipt.outcome_record,
            receipt.recovery_binding,
            commitment_key,
        )
    } else if cancel_target {
        crate::stage8b_p1d3_working_limit::apply_stage8b_p1d3_cancel_target_truth_stage(
            *restart,
            &receipt.outcome_record,
            receipt.recovery_binding,
            commitment_key,
        )
    } else if let Some(schedule_checkpoint) = schedule_checkpoint {
        crate::stage8b_p1d3_working_limit::apply_stage8b_p1d3_autonomous_truth_stage_after_schedule_binding(
            *restart,
            &receipt.outcome_record,
            receipt.recovery_binding,
            schedule_checkpoint,
            commitment_key,
        )
    } else {
        crate::stage8b_p1d3_working_limit::apply_stage8b_p1d3_autonomous_truth_stage(
            *restart,
            &receipt.outcome_record,
            receipt.recovery_binding,
            commitment_key,
        )
    };
    let applied = applied?;
    recovered.stage5_runtime = Stage6dStage5RuntimeAuthority::Restart(Box::new(applied.restored));
    recovered.refresh_after_append()?;
    let transition = Stage6Stage8bP1d3TruthTransition {
        recovered,
        stage5g_restart_package: applied.restart_package,
    };
    let restart_phase = transition.recovered.stage8b_p1d3_restart_phase();
    match restart_phase {
        Some(Stage6Stage8bP1d3RestartPhase::ReadyForEvaluation) => {
            Ok(Stage6Stage8bP1d3RecoveredTransition::Ready(transition))
        }
        Some(Stage6Stage8bP1d3RestartPhase::AckCommitted) => Ok(
            Stage6Stage8bP1d3RecoveredTransition::AckCommitted(Stage6Stage8bP1d3AckTransition {
                recovered: transition.recovered,
                stage5g_restart_package: transition.stage5g_restart_package,
            }),
        ),
        Some(Stage6Stage8bP1d3RestartPhase::TruthCommitted) => Ok(
            Stage6Stage8bP1d3RecoveredTransition::TruthCommitted(transition),
        ),
        Some(Stage6Stage8bP1d3RestartPhase::CancelContinuationPending) => {
            Ok(Stage6Stage8bP1d3RecoveredTransition::CancelContinuationPending(transition))
        }
        Some(Stage6Stage8bP1d3RestartPhase::SemanticCallbackPending) => {
            Ok(Stage6Stage8bP1d3RecoveredTransition::SemanticCallbackPending(transition))
        }
        _ => Err(Stage6dLiveCoreError::DurableOrderingViolation),
    }
}

/// Appends one request-scoped P1-d3 outcome as a single V3 write-ahead fact,
/// rereads it through mixed replay, then appends the exact Stage 7
/// `RequestFinalized` record. No Stage 5G ACK/truth reducer is reachable until
/// both records and the resulting checkpoint have been authenticated.
pub(crate) fn append_stage8b_p1d3_request_outcome(
    recovered: &mut Stage6dDurableRuntimeRecovered,
    authority: Stage6DurableRequestAuthorityV1,
    plan: &crate::stage8b_p1d3_working_limit::Stage8bP1d3TransitionPlan,
) -> Result<Stage6Stage8bP1d3OutcomeAppendReceipt, Stage6dLiveCoreError> {
    if plan.evidence.encode_canonical()? != plan.evidence_bytes
        || plan.evidence.digest_sha256()? != plan.evidence_sha256
        || authority.durable_frontier_sha256()
            != frontier_fingerprint(recovered.journal_frontier())?
        || authority.authenticated_checkpoint_sha256()
            != recovered.authenticated_checkpoint().checkpoint_sha256()
        || plan.evidence.stage6_predecessor_checkpoint_sha256()
            != authority.authenticated_checkpoint_sha256()
    {
        return Err(Stage6dLiveCoreError::DurableOrderingViolation);
    }
    plan.evidence.validate_request_authority(&authority)?;

    let prior_record_id = recovered
        .journal_frontier()
        .last_record_id()
        .cloned()
        .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?;
    let prior_sequence = recovered
        .journal_frontier()
        .last_lifecycle_sequence()
        .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?;
    let outcome_sequence = Stage6LifecycleSequence::new(
        prior_sequence
            .get()
            .checked_add(1)
            .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?,
    )?;
    let outcome_record = Stage6JournalRecordV3::from_p1d3_outcome_evidence(
        outcome_sequence,
        prior_record_id,
        plan.evidence_bytes.clone(),
    )?;
    let observed_at = Utc
        .timestamp_millis_opt(plan.evidence.transition_received_ts_utc_ms())
        .single()
        .ok_or(Stage6dLiveCoreError::Stage8bP1d3WorkingLimit)?;
    let finalized = stage7a_request_finalization_record(
        authority.identity().clone(),
        outcome_record.journal_record_id().clone(),
        outcome_sequence.get(),
        observed_at,
        Stage6RequestFinalDispositionV1::Completed,
    )?;
    let Some((expected_finalized_id, expected_finalized_fingerprint)) =
        plan.evidence.stage7_request_finalized_binding()
    else {
        return Err(Stage6dLiveCoreError::DurableOrderingViolation);
    };
    if finalized.journal_record_id().as_str() != expected_finalized_id
        || finalized.source_evidence_sha256().as_str() != expected_finalized_fingerprint
    {
        return Err(Stage6dLiveCoreError::DurableOrderingViolation);
    }

    recovered
        .journal_mut()
        .append_versioned(&Stage6JournalRecordVersioned::V3(outcome_record.clone()))
        .map_err(classify_stage8a4_append_error)?;
    recovered
        .refresh_after_append()
        .map_err(|_| Stage6dLiveCoreError::JournalMutationMayHaveOccurred)?;
    let after_outcome = recovered
        .replay()
        .request(authority.identity().strategy_request_id())
        .ok_or(Stage6dLiveCoreError::JournalMutationMayHaveOccurred)?;
    if after_outcome.last_unique_record_id() != outcome_record.journal_record_id()
        || after_outcome.last_unique_sequence() != outcome_sequence.get()
        || after_outcome.final_disposition().is_some()
        || after_outcome.dispatch_safety_state()
            != crate::Stage6DispatchSafetyStateV1::DispatchForbidden
    {
        return Err(Stage6dLiveCoreError::JournalMutationMayHaveOccurred);
    }
    stage8b_p1d4_test_crash_frontier("F03");

    recovered
        .journal_mut()
        .append(&finalized)
        .map_err(classify_stage8a4_append_error)?;
    recovered
        .refresh_after_append()
        .map_err(|_| Stage6dLiveCoreError::JournalMutationMayHaveOccurred)?;
    let after_finalized = recovered
        .replay()
        .request(authority.identity().strategy_request_id())
        .ok_or(Stage6dLiveCoreError::JournalMutationMayHaveOccurred)?;
    if after_finalized.last_unique_record_id() != finalized.journal_record_id()
        || after_finalized.last_unique_sequence() != finalized.lifecycle_sequence().get()
        || after_finalized.final_disposition() != Some(Stage6RequestFinalDispositionV1::Completed)
    {
        return Err(Stage6dLiveCoreError::JournalMutationMayHaveOccurred);
    }
    stage8b_p1d4_test_crash_frontier("F04");

    let recovery_binding = crate::stage8b_p1d3_working_limit::Stage8bP1d3Stage6RecoveryBinding {
        outcome_record_id: outcome_record.journal_record_id().as_str().to_string(),
        predecessor_frontier_sha256: plan
            .evidence
            .stage6_predecessor_checkpoint_sha256()
            .to_string(),
        reserved_checkpoint_sha256: plan
            .evidence
            .stage6_reserved_checkpoint_sha256()
            .to_string(),
        request_finalized_record_id: Some(finalized.journal_record_id().as_str().to_string()),
        request_finalized_fingerprint_sha256: Some(
            finalized.source_evidence_sha256().as_str().to_string(),
        ),
        authenticated_post_checkpoint_sha256: recovered
            .authenticated_checkpoint()
            .checkpoint_sha256()
            .to_string(),
    };
    outcome_record.authenticate_p1d3_outcome(recovery_binding.clone())?;
    Ok(Stage6Stage8bP1d3OutcomeAppendReceipt {
        outcome_record,
        recovery_binding,
    })
}

/// Appends one autonomous later-fill/expiry outcome. The authenticated
/// Stage5G replacement package supplies the exact pre-book and Stage 6
/// predecessor checkpoint; the outcome carries no request-finalization row.
pub(crate) fn append_stage8b_p1d3_autonomous_outcome(
    recovered: &mut Stage6dDurableRuntimeRecovered,
    plan: &crate::stage8b_p1d3_working_limit::Stage8bP1d3TransitionPlan,
    authenticated_schedule_checkpoint_sha256: Option<&str>,
) -> Result<Stage6Stage8bP1d3OutcomeAppendReceipt, Stage6dLiveCoreError> {
    if plan.evidence.is_request_scoped()
        || plan.evidence.stage7_request_finalized_binding().is_some()
        || plan.evidence.encode_canonical()? != plan.evidence_bytes
        || plan.evidence.digest_sha256()? != plan.evidence_sha256
    {
        return Err(Stage6dLiveCoreError::DurableOrderingViolation);
    }
    let restart = match &recovered.stage5_runtime {
        Stage6dStage5RuntimeAuthority::Restart(restart) => restart,
        Stage6dStage5RuntimeAuthority::FirstBoot(_) => {
            return Err(Stage6dLiveCoreError::RestartRuntimeRequired)
        }
    };
    let replacement = restart
        .stage8b_p1d3_replacement()
        .ok_or(Stage6dLiveCoreError::RestartRuntimeRequired)?;
    let replacement_checkpoint_is_current = replacement.authenticated_stage6_checkpoint_sha256()
        == recovered.authenticated_checkpoint().checkpoint_sha256();
    let replacement_checkpoint_is_exact_v4_predecessor = authenticated_schedule_checkpoint_sha256
        .is_some_and(|checkpoint| {
            checkpoint == recovered.authenticated_checkpoint().checkpoint_sha256()
                && replacement.authenticated_stage6_checkpoint_sha256() != checkpoint
        });
    if !matches!(
        replacement.phase(),
        crate::stage8b_p1d3_working_limit::Stage8bP1d3BookPhase::Working
            | crate::stage8b_p1d3_working_limit::Stage8bP1d3BookPhase::Eval
    ) || replacement.working_book().canonical_sha256()? != plan.evidence.pre_book_sha256()
        || (!replacement_checkpoint_is_current && !replacement_checkpoint_is_exact_v4_predecessor)
        || plan.evidence.stage6_predecessor_checkpoint_sha256()
            != recovered.authenticated_checkpoint().checkpoint_sha256()
    {
        return Err(Stage6dLiveCoreError::DurableOrderingViolation);
    }
    let prior_record_id = recovered
        .journal_frontier()
        .last_record_id()
        .cloned()
        .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?;
    let prior_sequence = recovered
        .journal_frontier()
        .last_lifecycle_sequence()
        .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?;
    let outcome_record = Stage6JournalRecordV3::from_p1d3_outcome_evidence(
        Stage6LifecycleSequence::new(
            prior_sequence
                .get()
                .checked_add(1)
                .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?,
        )?,
        prior_record_id,
        plan.evidence_bytes.clone(),
    )?;
    recovered
        .journal_mut()
        .append_versioned(&Stage6JournalRecordVersioned::V3(outcome_record.clone()))
        .map_err(classify_stage8a4_append_error)?;
    recovered
        .refresh_after_append()
        .map_err(|_| Stage6dLiveCoreError::JournalMutationMayHaveOccurred)?;
    if recovered.journal_frontier().last_record_id() != Some(outcome_record.journal_record_id())
        || recovered.journal_frontier().last_lifecycle_sequence()
            != Some(outcome_record.lifecycle_sequence())
    {
        return Err(Stage6dLiveCoreError::JournalMutationMayHaveOccurred);
    }
    let recovery_binding = crate::stage8b_p1d3_working_limit::Stage8bP1d3Stage6RecoveryBinding {
        outcome_record_id: outcome_record.journal_record_id().as_str().to_string(),
        predecessor_frontier_sha256: plan
            .evidence
            .stage6_predecessor_checkpoint_sha256()
            .to_string(),
        reserved_checkpoint_sha256: plan
            .evidence
            .stage6_reserved_checkpoint_sha256()
            .to_string(),
        request_finalized_record_id: None,
        request_finalized_fingerprint_sha256: None,
        authenticated_post_checkpoint_sha256: recovered
            .authenticated_checkpoint()
            .checkpoint_sha256()
            .to_string(),
    };
    outcome_record.authenticate_p1d3_outcome(recovery_binding.clone())?;
    Ok(Stage6Stage8bP1d3OutcomeAppendReceipt {
        outcome_record,
        recovery_binding,
    })
}

/// Appends only the autonomous target-fill fact that may precede a cancel
/// outcome. The cancel dispatch is already durable, but the target fact does
/// not settle that request. Its replacement S_terminal must be committed and
/// reread before the cancel continuation can be built.
fn append_stage8b_p1d3_cancel_target_outcome(
    recovered: &mut Stage6dDurableRuntimeRecovered,
    cancel_authority: &Stage6DurableRequestAuthorityV1,
    plan: &crate::stage8b_p1d3_working_limit::Stage8bP1d3TransitionPlan,
) -> Result<Stage6Stage8bP1d3OutcomeAppendReceipt, Stage6dLiveCoreError> {
    if plan.evidence.is_request_scoped()
        || plan.evidence.outcome_kind()
            != crate::stage8b_p1d3_working_limit::Stage8bP1d3OutcomeKind::LaterFilled
        || plan.evidence.stage7_request_finalized_binding().is_some()
        || plan.evidence.encode_canonical()? != plan.evidence_bytes
        || plan.evidence.digest_sha256()? != plan.evidence_sha256
        || cancel_authority.identity().action() != Stage6DurableActionKind::Cancel
        || cancel_authority.identity().target_broker_order_id()
            != Some(plan.evidence.broker_order_id())
        || cancel_authority.durable_frontier_sha256()
            != frontier_fingerprint(recovered.journal_frontier())?
        || cancel_authority.authenticated_checkpoint_sha256()
            != recovered.authenticated_checkpoint().checkpoint_sha256()
        || plan.evidence.stage6_predecessor_checkpoint_sha256()
            != cancel_authority.authenticated_checkpoint_sha256()
        || recovered.journal_frontier().last_record_id()
            != Some(cancel_authority.dispatch_record_id())
        || recovered
            .journal_frontier()
            .last_lifecycle_sequence()
            .map(|value| value.get())
            != Some(cancel_authority.dispatch_sequence())
    {
        return Err(Stage6dLiveCoreError::DurableOrderingViolation);
    }
    let restart = match &recovered.stage5_runtime {
        Stage6dStage5RuntimeAuthority::Restart(restart) => restart,
        Stage6dStage5RuntimeAuthority::FirstBoot(_) => {
            return Err(Stage6dLiveCoreError::RestartRuntimeRequired)
        }
    };
    let replacement = restart
        .stage8b_p1d3_replacement()
        .ok_or(Stage6dLiveCoreError::RestartRuntimeRequired)?;
    if replacement.working_book().canonical_sha256()? != plan.evidence.pre_book_sha256()
        || !restart
            .stage8b_p1_semantic_commit()
            .is_some_and(|semantic| {
                matches!(
                    semantic.canonical_command.as_ref(),
                    Some(BrokerCommand::CancelOrder(cancel))
                        if cancel.request_id == cancel_authority.identity().strategy_request_id()
                            && cancel.order_id == *plan.evidence.broker_order_id()
                )
            })
    {
        return Err(Stage6dLiveCoreError::DurableOrderingViolation);
    }
    let outcome_record = Stage6JournalRecordV3::from_p1d3_outcome_evidence(
        Stage6LifecycleSequence::new(
            cancel_authority
                .dispatch_sequence()
                .checked_add(1)
                .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?,
        )?,
        cancel_authority.dispatch_record_id().clone(),
        plan.evidence_bytes.clone(),
    )?;
    recovered
        .journal_mut()
        .append_versioned(&Stage6JournalRecordVersioned::V3(outcome_record.clone()))
        .map_err(classify_stage8a4_append_error)?;
    recovered
        .refresh_after_append()
        .map_err(|_| Stage6dLiveCoreError::JournalMutationMayHaveOccurred)?;
    if recovered.journal_frontier().last_record_id() != Some(outcome_record.journal_record_id())
        || recovered.journal_frontier().last_lifecycle_sequence()
            != Some(outcome_record.lifecycle_sequence())
    {
        return Err(Stage6dLiveCoreError::JournalMutationMayHaveOccurred);
    }
    let recovery_binding = crate::stage8b_p1d3_working_limit::Stage8bP1d3Stage6RecoveryBinding {
        outcome_record_id: outcome_record.journal_record_id().as_str().to_string(),
        predecessor_frontier_sha256: plan
            .evidence
            .stage6_predecessor_checkpoint_sha256()
            .to_string(),
        reserved_checkpoint_sha256: plan
            .evidence
            .stage6_reserved_checkpoint_sha256()
            .to_string(),
        request_finalized_record_id: None,
        request_finalized_fingerprint_sha256: None,
        authenticated_post_checkpoint_sha256: recovered
            .authenticated_checkpoint()
            .checkpoint_sha256()
            .to_string(),
    };
    outcome_record.authenticate_p1d3_outcome(recovery_binding.clone())?;
    Ok(Stage6Stage8bP1d3OutcomeAppendReceipt {
        outcome_record,
        recovery_binding,
    })
}

#[cfg(any(test, feature = "stage5g-artifact-fixtures"))]
fn stage8b_p1d4_pre_kill_audit_sha256(
    root: &std::path::Path,
    marker: &std::path::Path,
    cell_id: &str,
    scenario_id: &str,
    frontier_id: &str,
    phase: &str,
) -> String {
    fn collect(
        root: &std::path::Path,
        directory: &std::path::Path,
        marker: &std::path::Path,
        files: &mut Vec<(String, Vec<u8>)>,
    ) {
        let mut entries = std::fs::read_dir(directory)
            .expect("P1-d4 audit directory must be readable")
            .collect::<Result<Vec<_>, _>>()
            .expect("P1-d4 audit entries must be readable");
        entries.sort_by_key(std::fs::DirEntry::file_name);
        for entry in entries {
            let path = entry.path();
            if path == marker
                || entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".p1d4-marker-")
            {
                continue;
            }
            let file_type = entry
                .file_type()
                .expect("P1-d4 audit file type must be readable");
            assert!(!file_type.is_symlink(), "P1-d4 audit rejects symlinks");
            if file_type.is_dir() {
                collect(root, &path, marker, files);
            } else if file_type.is_file() {
                let relative = path
                    .strip_prefix(root)
                    .expect("P1-d4 audit path must remain under its root")
                    .to_str()
                    .expect("P1-d4 audit path must be UTF-8")
                    .to_string();
                files.push((
                    relative,
                    std::fs::read(&path).expect("P1-d4 audit file must be readable"),
                ));
            } else {
                panic!("P1-d4 audit rejects special files");
            }
        }
    }

    fn frame(hasher: &mut Sha256, bytes: &[u8]) {
        hasher.update((bytes.len() as u64).to_be_bytes());
        hasher.update(bytes);
    }

    let mut files = Vec::new();
    collect(root, root, marker, &mut files);
    files.sort_by(|left, right| left.0.as_bytes().cmp(right.0.as_bytes()));
    let mut hasher = Sha256::new();
    hasher.update(b"moex.stage8b.p1d4.pre-kill-audit.v1\0");
    for value in [cell_id, scenario_id, frontier_id, phase] {
        frame(&mut hasher, value.as_bytes());
    }
    for (path, bytes) in files {
        frame(&mut hasher, path.as_bytes());
        frame(&mut hasher, &bytes);
    }
    format!("{:x}", hasher.finalize())
}

#[cfg(any(test, feature = "stage5g-artifact-fixtures"))]
fn stage8b_p1d2_test_crash_barrier(phase: &str) {
    if std::env::var("STAGE8B_P1_TEST_CRASH_PHASE").as_deref() != Ok(phase) {
        return;
    }
    let marker = std::path::PathBuf::from(
        std::env::var_os("STAGE8B_P1_TEST_CRASH_MARKER")
            .expect("P1-d2 crash child requires a marker path"),
    );
    let parent = marker
        .parent()
        .expect("P1 crash marker requires a parent directory");
    let temporary = parent.join(format!(".p1d4-marker-{}.tmp", std::process::id()));
    let bytes = if let Ok(frontier_id) = std::env::var("STAGE8B_P1D4_FRONTIER_ID") {
        let cell_id = std::env::var("STAGE8B_P1D4_CELL_ID")
            .expect("P1-d4 crash child requires the exact registry cell ID");
        let scenario_id = std::env::var("STAGE8B_P1D4_SCENARIO_ID")
            .expect("P1-d4 crash child requires the exact registry scenario ID");
        let audit_root = std::path::PathBuf::from(
            std::env::var_os("STAGE8B_P1_TEST_PARENT")
                .expect("P1-d4 crash child requires its audit root"),
        );
        let pre_kill_audit_sha256 = stage8b_p1d4_pre_kill_audit_sha256(
            &audit_root,
            &marker,
            &cell_id,
            &scenario_id,
            &frontier_id,
            phase,
        );
        let canonical_cell_id = cell_id
            .strip_prefix("P1D4C-")
            .or_else(|| cell_id.strip_prefix("P1D4GM-"))
            .is_some_and(|suffix| {
                suffix.len() == 3 && suffix.bytes().all(|byte| byte.is_ascii_digit())
            });
        assert!(
            canonical_cell_id,
            "P1-d4 crash marker cell ID must be canonical"
        );
        assert!(
            scenario_id.len() == 3
                && scenario_id.starts_with('S')
                && scenario_id[1..].bytes().all(|byte| byte.is_ascii_digit()),
            "P1-d4 crash marker scenario ID must be canonical"
        );
        let canonical_frontier_id = frontier_id
            .strip_prefix('F')
            .or_else(|| frontier_id.strip_prefix("GM"))
            .is_some_and(|suffix| {
                suffix.len() == 2 && suffix.bytes().all(|byte| byte.is_ascii_digit())
            });
        assert!(
            canonical_frontier_id,
            "P1-d4 crash marker frontier ID must be canonical"
        );
        assert!(
            phase.starts_with("p1d4-")
                && phase
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-'),
            "P1-d4 crash marker hook name must be canonical"
        );
        assert!(
            pre_kill_audit_sha256.len() == 64
                && pre_kill_audit_sha256
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
            "P1-d4 pre-kill audit digest must be lowercase hexadecimal"
        );
        assert_ne!(
            frontier_id, "F16",
            "P1-d4 F16 must be emitted only after the Redis XACK reply is observed"
        );
        format!(
            "{{\"cell_id\":\"{cell_id}\",\"child_pid\":{},\"domain\":\"moex.stage8b.p1d4.crash-marker.v1\",\"frontier_id\":\"{frontier_id}\",\"kill_hook_name\":\"{phase}\",\"pre_kill_audit_sha256\":\"{pre_kill_audit_sha256}\",\"scenario_id\":\"{scenario_id}\",\"schema_version\":1}}",
            std::process::id()
        )
    } else {
        format!("{{\"phase\":\"{phase}\",\"pid\":{}}}\n", std::process::id())
    };
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .expect("P1 crash marker temporary file must be created exactly once");
    std::io::Write::write_all(&mut file, bytes.as_bytes())
        .expect("P1 crash marker bytes must be written completely");
    file.sync_all()
        .expect("P1 crash marker temporary file must be durable");
    std::fs::rename(&temporary, &marker).expect("P1 crash marker rename must be atomic");
    std::fs::File::open(parent)
        .and_then(|directory| directory.sync_all())
        .expect("P1 crash marker parent directory must be durable");
    loop {
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}

#[cfg(any(test, feature = "stage5g-artifact-fixtures"))]
pub(crate) fn stage8b_p1d4_test_crash_frontier(frontier_id: &str) {
    if std::env::var("STAGE8B_P1D4_ARMED").as_deref() != Ok("1")
        || std::env::var("STAGE8B_P1D4_FRONTIER_ID").as_deref() != Ok(frontier_id)
    {
        return;
    }
    let phase = std::env::var("STAGE8B_P1_TEST_CRASH_PHASE")
        .expect("P1-d4 frontier child requires the exact registry hook name");
    stage8b_p1d2_test_crash_barrier(&phase);
}

#[cfg(not(any(test, feature = "stage5g-artifact-fixtures")))]
#[inline(always)]
pub(crate) fn stage8b_p1d4_test_crash_frontier(_: &str) {}

#[cfg(not(any(test, feature = "stage5g-artifact-fixtures")))]
#[inline(always)]
fn stage8b_p1d2_test_crash_barrier(_: &str) {}

fn validate_stage8b_p1_projection_source(
    projection: &Stage5gP1SemanticCommitProjectionV1,
    source: &Stage6Stage8bP1SealSourceV1,
    prior_stage5_checkpoint_sha256: &str,
    expected_intent_count: usize,
) -> Result<(), Stage6dLiveCoreError> {
    if !projection.validate()
        || projection.intent_count != expected_intent_count
        || projection.operational_identity_sha256 != source.operational_identity_sha256
        || projection.prior_stage5g_checkpoint_sha256 != prior_stage5_checkpoint_sha256
    {
        return Err(Stage6dLiveCoreError::DurableOrderingViolation);
    }
    Ok(())
}

fn stage8b_p1_request_accepted_source_evidence(
    projection: &Stage5gP1SemanticCommitProjectionV1,
    source: &Stage6Stage8bP1SealSourceV1,
    expected_record_id: &Stage6JournalRecordId,
) -> Result<Stage6Sha256Digest, Stage6dLiveCoreError> {
    let request_id = projection
        .request_id
        .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?;
    let command_sha256 = projection
        .canonical_command_sha256
        .as_deref()
        .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?;
    let bytes = serde_json::to_vec(&Stage8bP1RequestAcceptedBindingV1 {
        schema_version: STAGE8B_P1_REQUEST_ACCEPTED_BINDING_SCHEMA_VERSION,
        domain: STAGE8B_P1_REQUEST_ACCEPTED_BINDING_DOMAIN,
        source_seal_generation: source.seal_generation,
        source_seal_commitment_sha256: &source.seal_commitment_sha256,
        source_stage6_checkpoint_sha256: &source.stage6_checkpoint_sha256,
        source_stage6_frontier_sha256: &source.stage6_frontier_sha256,
        operational_identity_sha256: &source.operational_identity_sha256,
        m10_redis_id: &projection.m10_redis_id,
        m10_semantic_id_sha256: &projection.m10_semantic_id_sha256,
        m10_payload_sha256: &projection.m10_payload_sha256,
        semantic_batch_id_sha256: &projection.semantic_batch_id_sha256,
        strategy_request_id: request_id,
        canonical_command_sha256: command_sha256,
        expected_request_accepted_record_id: expected_record_id.as_str(),
    })
    .map_err(|_| Stage6dLiveCoreError::IntegrationFingerprint)?;
    Stage6Sha256Digest::parse(sha256_hex(&bytes))
        .map_err(|_| Stage6dLiveCoreError::IntegrationFingerprint)
}

fn projected_checkpoint_after_append(
    recovered: &Stage6dDurableRuntimeRecovered,
    record: &Stage6JournalRecordV1,
) -> Result<String, Stage6dLiveCoreError> {
    let bytes = recovered.journal.framed_bytes()?;
    let mut projected = Stage6MemoryJournalBackend::from_framed_bytes(bytes)?;
    projected.append(record)?;
    Ok(
        Stage6JournalCheckpointV1::from_frontier(projected.frontier().clone())?
            .checkpoint_sha256()
            .to_string(),
    )
}

fn stage8b_p1_commit_evidence(
    projection: &Stage5gP1SemanticCommitProjectionV1,
    accepted: Option<&Stage6JournalRecordV1>,
) -> Stage6Stage8bP1SemanticCommitEvidenceV1 {
    Stage6Stage8bP1SemanticCommitEvidenceV1 {
        schema_version: STAGE8B_P1_REQUEST_ACCEPTED_BINDING_SCHEMA_VERSION,
        semantic_batch_id_sha256: projection.semantic_batch_id_sha256.clone(),
        m10_redis_id: projection.m10_redis_id.clone(),
        m10_semantic_id_sha256: projection.m10_semantic_id_sha256.clone(),
        m10_payload_sha256: projection.m10_payload_sha256.clone(),
        intent_count: projection.intent_count,
        strategy_request_id: projection.request_id,
        canonical_command_sha256: projection.canonical_command_sha256.clone(),
        request_accepted_record_id: accepted
            .map(|record| record.journal_record_id().as_str().to_string()),
        request_accepted_source_evidence_sha256: accepted
            .map(|record| record.source_evidence_sha256().as_str().to_string()),
    }
}

/// Classifies only the structural Stage 6 half of the P1 journal-ahead crash
/// frontier.  The outer P1 owner must still prove the exact pending M10 and
/// reproduce the source-evidence digest before it may commit S1.
pub fn classify_stage8b_p1_journal_ahead_candidate(
    records: &[Stage6JournalRecordVersioned],
    s0_checkpoint: &Stage6JournalCheckpointV1,
) -> Result<Option<Stage6Stage8bP1JournalAheadCandidate>, Stage6dLiveCoreError> {
    let prefix_len: usize = s0_checkpoint
        .frontier()
        .frame_count()
        .try_into()
        .map_err(|_| Stage6dLiveCoreError::DurableOrderingViolation)?;
    if records.len() != prefix_len.saturating_add(1) {
        return Ok(None);
    }
    let mut prefix = Stage6MemoryJournalBackend::new();
    for record in records.iter().take(prefix_len) {
        prefix.append_versioned(record)?;
    }
    if prefix.validate_checkpoint(s0_checkpoint).is_err() {
        return Ok(None);
    }
    let Stage6JournalRecordVersioned::V1(accepted) = &records[prefix_len] else {
        return Ok(None);
    };
    let command = match accepted.payload() {
        Stage6JournalPayloadV1::RequestAccepted { command } => command.as_ref().clone(),
        _ => return Ok(None),
    };
    if accepted.event_kind() != Stage6JournalEventKind::RequestAccepted
        || accepted.lifecycle_sequence().get() != 1
        || accepted.previous_record_id().is_some()
        || accepted.causal_parent_id().is_some()
    {
        return Ok(None);
    }
    let replay = Stage6MixedReplayEngineV2::replay(records)?;
    let request = replay
        .requests()
        .iter()
        .find(|request| {
            request.strategy_request_id()
                == accepted.durable_request_identity().strategy_request_id()
        })
        .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?;
    if request.dispatch_safety_state() != crate::Stage6DispatchSafetyStateV1::ReadyForFirstDispatch
        || request.final_disposition().is_some()
        || request.last_unique_record_id() != accepted.journal_record_id()
        || request.last_unique_sequence() != 1
    {
        return Ok(None);
    }
    Ok(Some(Stage6Stage8bP1JournalAheadCandidate {
        identity: accepted.durable_request_identity().clone(),
        command,
        record_id: accepted.journal_record_id().clone(),
        source_evidence_sha256: accepted.source_evidence_sha256().clone(),
    }))
}

/// Classifies exactly one uncovered Stage 8B-P1-e V4 schedule binding. No
/// other suffix is recoverable through this path: mixed versions, additional
/// records, causal drift and sequence drift fail closed.
pub fn classify_stage8b_p1e_schedule_journal_ahead_candidate(
    records: &[Stage6JournalRecordVersioned],
    predecessor_checkpoint: &Stage6JournalCheckpointV1,
) -> Result<Option<crate::Stage6JournalRecordV4>, Stage6dLiveCoreError> {
    let prefix_len: usize = predecessor_checkpoint
        .frontier()
        .frame_count()
        .try_into()
        .map_err(|_| Stage6dLiveCoreError::DurableOrderingViolation)?;
    if records.len() != prefix_len.saturating_add(1) {
        return Ok(None);
    }
    let mut prefix = Stage6MemoryJournalBackend::new();
    for record in records.iter().take(prefix_len) {
        prefix.append_versioned(record)?;
    }
    if prefix.validate_checkpoint(predecessor_checkpoint).is_err() {
        return Ok(None);
    }
    let Some(previous_record_id) = prefix.frontier().last_record_id() else {
        return Ok(None);
    };
    let Some(previous_sequence) = prefix.frontier().last_lifecycle_sequence() else {
        return Ok(None);
    };
    let Stage6JournalRecordVersioned::V4(candidate) = &records[prefix_len] else {
        return Ok(None);
    };
    if candidate.previous_record_id() != previous_record_id
        || candidate.lifecycle_sequence().get()
            != previous_sequence
                .get()
                .checked_add(1)
                .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?
    {
        return Ok(None);
    }
    let replay = Stage6MixedReplayEngineV2::replay(records)?;
    if replay.schedule_binding_records().len() != 1
        || replay
            .schedule_binding_records()
            .last()
            .map(crate::Stage6JournalRecordV4::encode_canonical)
            != Some(candidate.encode_canonical())
    {
        return Ok(None);
    }
    Ok(Some(candidate.clone()))
}

/// Classifies only the two recoverable pre-S_ack Stage 6 shapes:
/// dispatch/order/trade, with or without the final RequestFinalized record.
/// Any extra, reordered, mixed-version or differently bound record is not a
/// recovery candidate.
pub fn classify_stage8b_p1d2_journal_ahead_candidate(
    records: &[Stage6JournalRecordVersioned],
    s_ack_predecessor_checkpoint: &Stage6JournalCheckpointV1,
) -> Result<Option<Stage6Stage8bP1d2JournalAheadCandidate>, Stage6dLiveCoreError> {
    let prefix_len: usize = s_ack_predecessor_checkpoint
        .frontier()
        .frame_count()
        .try_into()
        .map_err(|_| Stage6dLiveCoreError::DurableOrderingViolation)?;
    let suffix_len = records.len().saturating_sub(prefix_len);
    if !matches!(suffix_len, 3 | 4) || records.len() < prefix_len {
        return Ok(None);
    }
    let mut prefix = Stage6MemoryJournalBackend::new();
    for record in records.iter().take(prefix_len) {
        prefix.append_versioned(record)?;
    }
    if prefix
        .validate_checkpoint(s_ack_predecessor_checkpoint)
        .is_err()
    {
        return Ok(None);
    }
    let Some(previous_record_id) = prefix.frontier().last_record_id().cloned() else {
        return Ok(None);
    };
    let Some(previous_sequence) = prefix.frontier().last_lifecycle_sequence() else {
        return Ok(None);
    };
    let suffix = records
        .get(prefix_len..)
        .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?;
    let mut v1 = Vec::with_capacity(suffix.len());
    for record in suffix {
        let Stage6JournalRecordVersioned::V1(record) = record else {
            return Ok(None);
        };
        v1.push(record);
    }
    let [dispatch, order, trade, rest @ ..] = v1.as_slice() else {
        return Ok(None);
    };
    let identity = dispatch.durable_request_identity();
    let accepted = prefix.records().iter().find(|record| {
        record.event_kind() == Stage6JournalEventKind::RequestAccepted
            && record.durable_request_identity().strategy_request_id()
                == identity.strategy_request_id()
    });
    let Some(accepted) = accepted else {
        return Ok(None);
    };
    let accepted_command_sha256 = accepted.canonical_payload_sha256();
    let dispatch_matches = matches!(
        dispatch.payload(),
        Stage6JournalPayloadV1::DispatchAttemptRecorded {
            attempt_ordinal: 1,
            accepted_request_payload_sha256,
        } if accepted_request_payload_sha256 == accepted_command_sha256
    );
    let broker_order_id = match order.payload() {
        Stage6JournalPayloadV1::BrokerOrderObserved { broker_order_id } => broker_order_id,
        _ => return Ok(None),
    };
    let broker_trade_id = match trade.payload() {
        Stage6JournalPayloadV1::BrokerTradeObserved {
            broker_trade_id,
            broker_order_id: trade_order_id,
        } if trade_order_id == broker_order_id => broker_trade_id,
        _ => return Ok(None),
    };
    let request_finalized = match rest {
        [] => false,
        [finalized]
            if matches!(
                finalized.payload(),
                Stage6JournalPayloadV1::RequestFinalized {
                    disposition: Stage6RequestFinalDispositionV1::Completed,
                }
            ) =>
        {
            true
        }
        _ => return Ok(None),
    };
    let same_identity = v1
        .iter()
        .all(|record| record.durable_request_identity() == identity);
    let exact_chain = v1.iter().enumerate().all(|(index, record)| {
        let expected_sequence = previous_sequence.get() + index as u64 + 1;
        let expected_previous = if index == 0 {
            &previous_record_id
        } else {
            v1[index - 1].journal_record_id()
        };
        record.lifecycle_sequence().get() == expected_sequence
            && record.previous_record_id() == Some(expected_previous)
            && record.causal_parent_id() == Some(expected_previous)
    });
    if !dispatch_matches
        || !same_identity
        || !exact_chain
        || (accepted.journal_record_id() != &previous_record_id
            && !stage8b_p1e_dispatch_follows_exact_schedule_binding_records(
                records, accepted, identity, dispatch,
            ))
        || identity.action() != Stage6DurableActionKind::Place
        || order.source_evidence_sha256() != trade.source_evidence_sha256()
    {
        return Ok(None);
    }
    let replay = Stage6MixedReplayEngineV2::replay(records)?;
    let Some(request) = replay
        .requests()
        .iter()
        .find(|request| request.strategy_request_id() == identity.strategy_request_id())
    else {
        return Ok(None);
    };
    if request.action() != Stage6DurableActionKind::Place
        || request.dispatch_attempt_count() != 1
        || request.dispatch_safety_state() != crate::Stage6DispatchSafetyStateV1::DispatchForbidden
        || request.known_broker_order_id() != Some(broker_order_id)
        || request.observed_broker_trade_ids() != [broker_trade_id.clone()]
        || request.cancel_outcome().is_some()
        || request.conflict_observed()
        || request.final_disposition()
            != request_finalized.then_some(Stage6RequestFinalDispositionV1::Completed)
        || request.last_unique_record_id()
            != v1
                .last()
                .expect("P1-d2 classifier has a non-empty suffix")
                .journal_record_id()
    {
        return Ok(None);
    }
    Ok(Some(Stage6Stage8bP1d2JournalAheadCandidate {
        identity: identity.clone(),
        broker_order_id: broker_order_id.clone(),
        broker_trade_id: broker_trade_id.clone(),
        request_finalized,
    }))
}

/// Classifies the four generated-Market V1 suffixes admitted by the P1-d4
/// package-aware restart contract. The caller must separately authenticate a
/// valid P1-d4 Prepublication package; this function intentionally cannot
/// turn an ordinary P1-d2 package into P1-d4 authority.
pub fn classify_stage8b_p1d4_generated_market_journal_ahead_candidate(
    records: &[Stage6JournalRecordVersioned],
    predecessor_checkpoint: &Stage6JournalCheckpointV1,
) -> Result<Option<Stage6Stage8bP1d4JournalAheadCandidate>, Stage6dLiveCoreError> {
    let prefix_len: usize = predecessor_checkpoint
        .frontier()
        .frame_count()
        .try_into()
        .map_err(|_| Stage6dLiveCoreError::DurableOrderingViolation)?;
    if records.len() < prefix_len || !matches!(records.len() - prefix_len, 1..=4) {
        return Ok(None);
    }
    let mut prefix = Stage6MemoryJournalBackend::new();
    for record in records.iter().take(prefix_len) {
        prefix.append_versioned(record)?;
    }
    if prefix.validate_checkpoint(predecessor_checkpoint).is_err() {
        return Ok(None);
    }
    let Some(accepted) = prefix.records().last() else {
        return Ok(None);
    };
    let Stage6JournalPayloadV1::RequestAccepted { command } = accepted.payload() else {
        return Ok(None);
    };
    let identity = accepted.durable_request_identity();
    let Some(shape) = command.place_order_shape() else {
        return Ok(None);
    };
    if accepted.event_kind() != Stage6JournalEventKind::RequestAccepted
        || identity.action() != Stage6DurableActionKind::Place
        || shape.order_type() != broker_core::OrderType::Market
        || shape.time_in_force() != broker_core::TimeInForce::Day
        || shape.limit_price().is_some()
        || shape.quantity() <= broker_core::Quantity::ZERO
    {
        return Ok(None);
    }
    let suffix = records
        .get(prefix_len..)
        .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?;
    let mut v1 = Vec::with_capacity(suffix.len());
    for record in suffix {
        let Stage6JournalRecordVersioned::V1(record) = record else {
            return Ok(None);
        };
        v1.push(record);
    }
    let dispatch = v1[0];
    let dispatch_matches = matches!(
        dispatch.payload(),
        Stage6JournalPayloadV1::DispatchAttemptRecorded {
            attempt_ordinal: 1,
            accepted_request_payload_sha256,
        } if accepted_request_payload_sha256 == accepted.canonical_payload_sha256()
    );
    let exact_chain = v1.iter().enumerate().all(|(index, record)| {
        let expected_sequence = accepted.lifecycle_sequence().get() + index as u64 + 1;
        let expected_previous = if index == 0 {
            accepted.journal_record_id()
        } else {
            v1[index - 1].journal_record_id()
        };
        record.durable_request_identity() == identity
            && record.lifecycle_sequence().get() == expected_sequence
            && record.previous_record_id() == Some(expected_previous)
            && record.causal_parent_id() == Some(expected_previous)
    });
    if dispatch.event_kind() != Stage6JournalEventKind::DispatchAttemptRecorded
        || !dispatch_matches
        || !exact_chain
    {
        return Ok(None);
    }
    let broker_order_id = match v1.get(1) {
        None => None,
        Some(order) => match order.payload() {
            Stage6JournalPayloadV1::BrokerOrderObserved { broker_order_id } => {
                Some(broker_order_id.clone())
            }
            _ => return Ok(None),
        },
    };
    let broker_trade_id = match v1.get(2) {
        None => None,
        Some(trade) => match trade.payload() {
            Stage6JournalPayloadV1::BrokerTradeObserved {
                broker_trade_id,
                broker_order_id: trade_order_id,
            } if Some(trade_order_id) == broker_order_id.as_ref() => Some(broker_trade_id.clone()),
            _ => return Ok(None),
        },
    };
    if let (Some(order), Some(trade)) = (v1.get(1), v1.get(2)) {
        if order.source_evidence_sha256() != trade.source_evidence_sha256() {
            return Ok(None);
        }
    }
    if let Some(finalized) = v1.get(3) {
        if !matches!(
            finalized.payload(),
            Stage6JournalPayloadV1::RequestFinalized {
                disposition: Stage6RequestFinalDispositionV1::Completed,
            }
        ) {
            return Ok(None);
        }
    }
    let kind = match v1.len() {
        1 => Stage6Stage8bP1d4JournalAheadKind::DispatchOnly,
        2 => Stage6Stage8bP1d4JournalAheadKind::OrderOnly,
        3 => Stage6Stage8bP1d4JournalAheadKind::PreFinalization,
        4 => Stage6Stage8bP1d4JournalAheadKind::PreAck,
        _ => return Ok(None),
    };
    let replay = Stage6MixedReplayEngineV2::replay(records)?;
    let Some(request) = replay
        .requests()
        .iter()
        .find(|request| request.strategy_request_id() == identity.strategy_request_id())
    else {
        return Ok(None);
    };
    let expected_dispatch_state = if kind == Stage6Stage8bP1d4JournalAheadKind::DispatchOnly {
        crate::Stage6DispatchSafetyStateV1::ReconciliationRequired
    } else {
        crate::Stage6DispatchSafetyStateV1::DispatchForbidden
    };
    if request.action() != Stage6DurableActionKind::Place
        || request.dispatch_attempt_count() != 1
        || request.dispatch_safety_state() != expected_dispatch_state
        || request.known_broker_order_id() != broker_order_id.as_ref()
        || request.observed_broker_trade_ids()
            != broker_trade_id.clone().into_iter().collect::<Vec<_>>()
        || request.cancel_outcome().is_some()
        || request.conflict_observed()
        || request.final_disposition()
            != (kind == Stage6Stage8bP1d4JournalAheadKind::PreAck)
                .then_some(Stage6RequestFinalDispositionV1::Completed)
        || request.last_unique_record_id()
            != v1
                .last()
                .expect("P1-d4 generated-Market suffix is non-empty")
                .journal_record_id()
    {
        return Ok(None);
    }
    Ok(Some(Stage6Stage8bP1d4JournalAheadCandidate {
        identity: identity.clone(),
        kind,
        broker_order_id,
        broker_trade_id,
        dispatch_record_id: dispatch.journal_record_id().clone(),
        predecessor_checkpoint_sha256: predecessor_checkpoint.checkpoint_sha256().to_string(),
    }))
}

/// Classifies the narrow crash frontier where a P1-d3 request has exactly one
/// durable dispatch row and no outcome.  Classification is intentionally
/// performed against the reconstructed authenticated runtime so a structurally
/// valid row cannot opt a Market or malformed command into P1-d3 recovery.
pub fn classify_stage8b_p1d3_dispatch_only_candidate(
    recovered: &Stage6dDurableRuntimeRecovered,
    predecessor_checkpoint: &Stage6JournalCheckpointV1,
) -> Result<Option<Stage6Stage8bP1d3DispatchOnlyCandidate>, Stage6dLiveCoreError> {
    let records = recovered.journal.versioned_records();
    let prefix_len: usize = predecessor_checkpoint
        .frontier()
        .frame_count()
        .try_into()
        .map_err(|_| Stage6dLiveCoreError::DurableOrderingViolation)?;
    if records.len() != prefix_len.saturating_add(1) {
        return Ok(None);
    }

    let mut prefix = Stage6MemoryJournalBackend::new();
    for record in records.iter().take(prefix_len) {
        prefix.append_versioned(record)?;
    }
    if prefix.validate_checkpoint(predecessor_checkpoint).is_err() {
        return Ok(None);
    }
    let Some(accepted) = prefix.records().last() else {
        return Ok(None);
    };
    let Stage6JournalRecordVersioned::V1(dispatch) = &records[prefix_len] else {
        return Ok(None);
    };
    let command_snapshot = match accepted.payload() {
        Stage6JournalPayloadV1::RequestAccepted { command } => command.as_ref(),
        _ => return Ok(None),
    };
    let identity = accepted.durable_request_identity();
    let dispatch_payload_matches = matches!(
        dispatch.payload(),
        Stage6JournalPayloadV1::DispatchAttemptRecorded {
            attempt_ordinal: 1,
            accepted_request_payload_sha256,
        } if accepted_request_payload_sha256 == accepted.canonical_payload_sha256()
    );
    if accepted.event_kind() != Stage6JournalEventKind::RequestAccepted
        || dispatch.event_kind() != Stage6JournalEventKind::DispatchAttemptRecorded
        || !dispatch_payload_matches
        || dispatch.durable_request_identity() != identity
        || dispatch.previous_record_id() != Some(accepted.journal_record_id())
        || dispatch.causal_parent_id() != Some(accepted.journal_record_id())
        || dispatch.lifecycle_sequence().get()
            != accepted
                .lifecycle_sequence()
                .get()
                .checked_add(1)
                .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?
        || recovered.journal_frontier().last_record_id() != Some(dispatch.journal_record_id())
    {
        return Ok(None);
    }

    let current_checkpoint =
        Stage6JournalCheckpointV1::from_frontier(recovered.journal_frontier().clone())?;
    if recovered.authenticated_checkpoint() != &current_checkpoint {
        return Ok(None);
    }
    let replay = Stage6MixedReplayEngineV2::replay(records)?;
    let Some(request) = replay
        .requests()
        .iter()
        .find(|request| request.strategy_request_id() == identity.strategy_request_id())
    else {
        return Ok(None);
    };
    if request.durable_client_order_id() != identity.durable_client_order_id()
        || request.action() != identity.action()
        || request.dispatch_attempt_count() != 1
        || request.dispatch_safety_state()
            != crate::Stage6DispatchSafetyStateV1::ReconciliationRequired
        || request.final_disposition().is_some()
        || request.conflict_observed()
        || request.last_unique_record_id() != dispatch.journal_record_id()
    {
        return Ok(None);
    }

    let Stage6dStage5RuntimeAuthority::Restart(restart) = &recovered.stage5_runtime else {
        return Ok(None);
    };
    let Some(semantic) = restart
        .stage8b_p1_semantic_commit()
        .filter(|projection| projection.validate() && projection.intent_count == 1)
    else {
        return Ok(None);
    };
    let Some(replacement) = restart.stage8b_p1d3_replacement() else {
        return Ok(None);
    };
    if replacement.validate().is_err()
        || replacement.authenticated_stage6_checkpoint_sha256()
            != predecessor_checkpoint.checkpoint_sha256()
    {
        return Ok(None);
    }
    let Some(command) = semantic.canonical_command.as_ref() else {
        return Ok(None);
    };
    let Some(semantic_identity) = semantic.durable_request_identity.as_ref() else {
        return Ok(None);
    };
    let Some(semantic_snapshot) = semantic.durable_command_snapshot.as_ref() else {
        return Ok(None);
    };
    let Some(canonical_command_sha256) = semantic.canonical_command_sha256.as_deref() else {
        return Ok(None);
    };
    let operational_identity_sha256 = recovered
        .authenticated_operational_identity()
        .map(stage6d_operational_identity_sha256)
        .transpose()?
        .ok_or(Stage6dLiveCoreError::RestartRuntimeRequired)?;
    let book = replacement.working_book();
    let command_sha256 = sha256_hex(
        &serde_json::to_vec(command).map_err(|_| Stage6dLiveCoreError::IntegrationFingerprint)?,
    );
    if semantic_identity != identity
        || semantic_snapshot != command_snapshot
        || !command_snapshot.matches_broker_command(identity, command)
        || command_sha256 != canonical_command_sha256
        || semantic.request_id != Some(identity.strategy_request_id())
        || semantic.operational_identity_sha256 != operational_identity_sha256.as_str()
        || book.operational_identity_sha256() != operational_identity_sha256.as_str()
        || book.account_id() != identity.account_id()
        || book.instrument() != identity.instrument()
        || book.package_generation() == 0
    {
        return Ok(None);
    }

    let exact_domain = match command {
        BrokerCommand::PlaceOrder(place) => {
            let shape = command_snapshot.place_order_shape();
            identity.action() == Stage6DurableActionKind::Place
                && place.request_id == identity.strategy_request_id()
                && place.account_id == *identity.account_id()
                && place.instrument == *identity.instrument()
                && place.client_order_id == *identity.durable_client_order_id()
                && place.comment.as_deref() == Some(identity.attribution().internal_comment())
                && identity.attribution() == book.attribution()
                && place.order_type == broker_core::OrderType::Limit
                && place.time_in_force == broker_core::TimeInForce::Day
                && place.ttl_ms.is_none()
                && place.qty > broker_core::Quantity::ZERO
                && place.qty.fract().is_zero()
                && place
                    .limit_price
                    .is_some_and(|price| price > broker_core::Price::ZERO)
                && shape.is_some_and(|shape| {
                    shape.order_type() == broker_core::OrderType::Limit
                        && shape.time_in_force() == broker_core::TimeInForce::Day
                        && shape.side() == place.side
                        && shape.quantity() == place.qty
                        && shape.limit_price() == place.limit_price
                })
        }
        BrokerCommand::CancelOrder(cancel) => {
            let Some(target) = book.record(&cancel.order_id) else {
                return Ok(None);
            };
            let target_attribution = book.attribution();
            identity.action() == Stage6DurableActionKind::Cancel
                && identity.attribution().role()
                    == Some(broker_core::HybridRuntimeOrderRole::Cancel)
                && identity.attribution().strategy_id() == target_attribution.strategy_id()
                && identity.attribution().cycle_id() == target_attribution.cycle_id()
                && identity.attribution().owner() == target_attribution.owner()
                && cancel.request_id == identity.strategy_request_id()
                && cancel.account_id == *identity.account_id()
                && cancel.ttl_ms.is_none()
                && identity.target_broker_order_id() == Some(&cancel.order_id)
                && identity.target_order_client_order_id() == cancel.client_order_id.as_ref()
                && cancel.client_order_id.as_ref().map_or(true, |supplied| {
                    supplied == target.original_client_order_id()
                })
                && identity.durable_client_order_id() != target.original_client_order_id()
        }
    };
    if !exact_domain {
        return Ok(None);
    }

    Ok(Some(Stage6Stage8bP1d3DispatchOnlyCandidate {
        identity: identity.clone(),
        command: command.clone(),
        command_snapshot: command_snapshot.clone(),
        accepted_record_id: accepted.journal_record_id().clone(),
        dispatch_record_id: dispatch.journal_record_id().clone(),
        dispatch_sequence: dispatch.lifecycle_sequence().get(),
        predecessor_checkpoint_sha256: predecessor_checkpoint.checkpoint_sha256().to_string(),
    }))
}

/// Classifies exactly one uncovered P1-d3 V3 outcome, optionally followed by
/// its request-scoped Stage 7 finalization. The predecessor checkpoint is
/// rebuilt from the durable prefix; no provider/schedule/Redis input is
/// accepted by this recovery path.
pub fn classify_stage8b_p1d3_journal_ahead_candidate(
    records: &[Stage6JournalRecordVersioned],
    predecessor_checkpoint: &Stage6JournalCheckpointV1,
) -> Result<Option<Stage6Stage8bP1d3JournalAheadCandidate>, Stage6dLiveCoreError> {
    let prefix_len: usize = predecessor_checkpoint
        .frontier()
        .frame_count()
        .try_into()
        .map_err(|_| Stage6dLiveCoreError::DurableOrderingViolation)?;
    if records.len() < prefix_len || !matches!(records.len() - prefix_len, 1..=3) {
        return Ok(None);
    }
    let mut prefix = Stage6MemoryJournalBackend::new();
    for record in records.iter().take(prefix_len) {
        prefix.append_versioned(record)?;
    }
    if prefix.validate_checkpoint(predecessor_checkpoint).is_err() {
        return Ok(None);
    }
    let Some(previous_record_id) = prefix.frontier().last_record_id().cloned() else {
        return Ok(None);
    };
    let Some(previous_sequence) = prefix.frontier().last_lifecycle_sequence() else {
        return Ok(None);
    };
    let suffix = &records[prefix_len..];
    let (dispatch, outcome_record, rest) = match suffix {
        [Stage6JournalRecordVersioned::V3(outcome), rest @ ..] => (None, outcome, rest),
        [Stage6JournalRecordVersioned::V1(dispatch), Stage6JournalRecordVersioned::V3(outcome), rest @ ..]
            if dispatch.event_kind() == Stage6JournalEventKind::DispatchAttemptRecorded =>
        {
            (Some(dispatch), outcome, rest)
        }
        _ => return Ok(None),
    };
    let evidence =
        crate::stage8b_p1d3_working_limit::Stage8bP1d3OutcomeEvidenceV1::decode_canonical(
            outcome_record.outcome_evidence_bytes(),
        )?;

    let request_effect = evidence.stage6_request_replay_effect()?;
    let request_finalized = match request_effect {
        None => {
            if !rest.is_empty() || evidence.stage7_request_finalized_binding().is_some() {
                return Ok(None);
            }
            match dispatch {
                None => {
                    if evidence.stage6_predecessor_checkpoint_sha256()
                        != predecessor_checkpoint.checkpoint_sha256()
                        || outcome_record.previous_record_id() != &previous_record_id
                        || outcome_record.lifecycle_sequence().get()
                            != previous_sequence
                                .get()
                                .checked_add(1)
                                .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?
                    {
                        return Ok(None);
                    }
                }
                Some(dispatch) => {
                    let Some(accepted) = prefix.records().last() else {
                        return Ok(None);
                    };
                    let identity = accepted.durable_request_identity();
                    let dispatch_payload_matches = matches!(
                        dispatch.payload(),
                        Stage6JournalPayloadV1::DispatchAttemptRecorded {
                            attempt_ordinal: 1,
                            accepted_request_payload_sha256,
                        } if accepted_request_payload_sha256 == accepted.canonical_payload_sha256()
                    );
                    if evidence.outcome_kind()
                        != crate::stage8b_p1d3_working_limit::Stage8bP1d3OutcomeKind::LaterFilled
                        || accepted.event_kind() != Stage6JournalEventKind::RequestAccepted
                        || identity.action() != Stage6DurableActionKind::Cancel
                        || identity.target_broker_order_id() != Some(evidence.broker_order_id())
                        || !dispatch_payload_matches
                        || dispatch.durable_request_identity() != identity
                        || dispatch.previous_record_id() != Some(accepted.journal_record_id())
                        || dispatch.causal_parent_id() != Some(accepted.journal_record_id())
                        || dispatch.lifecycle_sequence().get()
                            != accepted
                                .lifecycle_sequence()
                                .get()
                                .checked_add(1)
                                .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?
                        || accepted.journal_record_id() != &previous_record_id
                        || accepted.lifecycle_sequence() != previous_sequence
                        || outcome_record.previous_record_id() != dispatch.journal_record_id()
                        || outcome_record.lifecycle_sequence().get()
                            != dispatch
                                .lifecycle_sequence()
                                .get()
                                .checked_add(1)
                                .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?
                    {
                        return Ok(None);
                    }
                    prefix.append_versioned(&Stage6JournalRecordVersioned::V1(dispatch.clone()))?;
                    let dispatch_checkpoint =
                        Stage6JournalCheckpointV1::from_frontier(prefix.frontier().clone())?;
                    if evidence.stage6_predecessor_checkpoint_sha256()
                        != dispatch_checkpoint.checkpoint_sha256()
                    {
                        return Ok(None);
                    }
                }
            }
            false
        }
        Some(effect) => {
            let Some(accepted) = prefix
                .records()
                .iter()
                .find(|record| {
                    record.event_kind() == Stage6JournalEventKind::RequestAccepted
                        && record.durable_request_identity().strategy_request_id()
                            == effect.request_id
                })
                .cloned()
            else {
                return Ok(None);
            };
            let dispatch_from_prefix = prefix.records().iter().find(|record| {
                record.event_kind() == Stage6JournalEventKind::DispatchAttemptRecorded
                    && record.journal_record_id().as_str() == effect.stage6_dispatch_record_id
            });
            let actual_dispatch = dispatch.or(dispatch_from_prefix);
            let Some(actual_dispatch) = actual_dispatch else {
                return Ok(None);
            };
            let dispatch_payload_matches = matches!(
                actual_dispatch.payload(),
                Stage6JournalPayloadV1::DispatchAttemptRecorded {
                    attempt_ordinal: 1,
                    accepted_request_payload_sha256,
                } if accepted_request_payload_sha256 == accepted.canonical_payload_sha256()
            );
            let (expected_dispatch_predecessor, expected_dispatch_sequence) = if dispatch.is_some()
            {
                (
                    &previous_record_id,
                    previous_sequence
                        .get()
                        .checked_add(1)
                        .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?,
                )
            } else {
                (
                    accepted.journal_record_id(),
                    accepted
                        .lifecycle_sequence()
                        .get()
                        .checked_add(1)
                        .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?,
                )
            };
            let dispatch_follows_predecessor = actual_dispatch.previous_record_id()
                == Some(expected_dispatch_predecessor)
                && actual_dispatch.causal_parent_id() == Some(expected_dispatch_predecessor)
                && actual_dispatch.lifecycle_sequence().get() == expected_dispatch_sequence;
            let dispatch_follows_schedule = dispatch.is_some()
                && stage8b_p1e_dispatch_follows_exact_schedule_binding_records(
                    records,
                    &accepted,
                    accepted.durable_request_identity(),
                    actual_dispatch,
                );
            if !dispatch_payload_matches
                || actual_dispatch.durable_request_identity() != accepted.durable_request_identity()
                || !dispatch_follows_predecessor
                || (dispatch.is_some()
                    && accepted.journal_record_id() != &previous_record_id
                    && !dispatch_follows_schedule)
                || accepted
                    .durable_request_identity()
                    .durable_client_order_id()
                    != &effect.durable_request_client_id
                || accepted.durable_request_identity().account_id() != &effect.account_id
                || accepted.durable_request_identity().instrument() != &effect.instrument
                || accepted.canonical_payload_sha256().as_str()
                    != effect.accepted_command_payload_sha256
                || actual_dispatch.journal_record_id().as_str() != effect.stage6_dispatch_record_id
            {
                return Ok(None);
            }
            if let Some(dispatch) = dispatch {
                if outcome_record.previous_record_id() != dispatch.journal_record_id()
                    || outcome_record.lifecycle_sequence().get()
                        != dispatch
                            .lifecycle_sequence()
                            .get()
                            .checked_add(1)
                            .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?
                {
                    return Ok(None);
                }
                prefix.append_versioned(&Stage6JournalRecordVersioned::V1(dispatch.clone()))?;
                let dispatch_checkpoint =
                    Stage6JournalCheckpointV1::from_frontier(prefix.frontier().clone())?;
                if evidence.stage6_predecessor_checkpoint_sha256()
                    != dispatch_checkpoint.checkpoint_sha256()
                {
                    return Ok(None);
                }
            } else if evidence.stage6_predecessor_checkpoint_sha256()
                != predecessor_checkpoint.checkpoint_sha256()
                || outcome_record.previous_record_id() != &previous_record_id
                || outcome_record.lifecycle_sequence().get()
                    != previous_sequence
                        .get()
                        .checked_add(1)
                        .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?
            {
                return Ok(None);
            }
            let observed_at = Utc
                .timestamp_millis_opt(evidence.transition_received_ts_utc_ms())
                .single()
                .ok_or(Stage6dLiveCoreError::Stage8bP1d3WorkingLimit)?;
            let expected_finalized = stage7a_request_finalization_record(
                accepted.durable_request_identity().clone(),
                outcome_record.journal_record_id().clone(),
                outcome_record.lifecycle_sequence().get(),
                observed_at,
                Stage6RequestFinalDispositionV1::Completed,
            )?;
            let Some((expected_id, expected_fingerprint)) =
                evidence.stage7_request_finalized_binding()
            else {
                return Ok(None);
            };
            if expected_finalized.journal_record_id().as_str() != expected_id
                || expected_finalized.source_evidence_sha256().as_str() != expected_fingerprint
            {
                return Ok(None);
            }
            match rest {
                [] => false,
                [Stage6JournalRecordVersioned::V1(actual)] if actual == &expected_finalized => true,
                _ => return Ok(None),
            }
        }
    };
    Stage6MixedReplayEngineV2::replay(records)?;
    Ok(Some(Stage6Stage8bP1d3JournalAheadCandidate {
        outcome_record: outcome_record.clone(),
        request_finalized,
    }))
}

/// Revalidates a classified journal-ahead candidate against the reconstructed
/// Stage5G authority. If the request finalization was the crash frontier, it
/// appends only that deterministic row; the provider and schedule are never
/// reacquired.
pub(crate) fn resume_stage8b_p1d3_journal_ahead_candidate(
    recovered: &mut Stage6dDurableRuntimeRecovered,
    candidate: Stage6Stage8bP1d3JournalAheadCandidate,
) -> Result<Stage6Stage8bP1d3OutcomeAppendReceipt, Stage6dLiveCoreError> {
    let outcome_record = candidate.outcome_record;
    let evidence =
        crate::stage8b_p1d3_working_limit::Stage8bP1d3OutcomeEvidenceV1::decode_canonical(
            outcome_record.outcome_evidence_bytes(),
        )?;
    let restart = match &recovered.stage5_runtime {
        Stage6dStage5RuntimeAuthority::Restart(restart) => restart,
        Stage6dStage5RuntimeAuthority::FirstBoot(_) => {
            return Err(Stage6dLiveCoreError::RestartRuntimeRequired)
        }
    };
    let replacement = restart
        .stage8b_p1d3_replacement()
        .ok_or(Stage6dLiveCoreError::RestartRuntimeRequired)?;
    let replacement_book_sha256 = replacement.working_book().canonical_sha256()?;
    if replacement_book_sha256 != evidence.pre_book_sha256() {
        return Err(Stage6dLiveCoreError::DurableOrderingViolation);
    }

    let schedule_checkpoint = recovered
        .stage8b_p1e_schedule_checkpoint_before_journal_ahead_outcome(
            replacement,
            &outcome_record,
        )?;
    let request_effect = evidence.stage6_request_replay_effect()?;
    match request_effect.as_ref() {
        None => {
            let ordinary_autonomous_chain = replacement.authenticated_stage6_checkpoint_sha256()
                == evidence.stage6_predecessor_checkpoint_sha256();
            let schedule_bound_autonomous_chain = schedule_checkpoint.is_some();
            let target_first_cancel_chain = (|| -> Result<bool, Stage6dLiveCoreError> {
                if evidence.outcome_kind()
                    != crate::stage8b_p1d3_working_limit::Stage8bP1d3OutcomeKind::LaterFilled
                    || replacement.phase()
                        != crate::stage8b_p1d3_working_limit::Stage8bP1d3BookPhase::Working
                {
                    return Ok(false);
                }
                let Some(semantic) = restart.stage8b_p1_semantic_commit() else {
                    return Ok(false);
                };
                let Some(BrokerCommand::CancelOrder(cancel)) = semantic.canonical_command.as_ref()
                else {
                    return Ok(false);
                };
                let Some(identity) = semantic.durable_request_identity.as_ref() else {
                    return Ok(false);
                };
                if cancel.request_id != identity.strategy_request_id()
                    || identity.action() != Stage6DurableActionKind::Cancel
                    || identity.target_broker_order_id() != Some(evidence.broker_order_id())
                    || cancel.order_id != *evidence.broker_order_id()
                {
                    return Ok(false);
                }
                let records = recovered.journal.versioned_records();
                let Some(dispatch_index) = records.iter().position(|record| {
                    matches!(
                        record,
                        Stage6JournalRecordVersioned::V1(dispatch)
                            if dispatch.journal_record_id() == outcome_record.previous_record_id()
                                && dispatch.event_kind()
                                    == Stage6JournalEventKind::DispatchAttemptRecorded
                    )
                }) else {
                    return Ok(false);
                };
                let Some(Stage6JournalRecordVersioned::V1(dispatch)) = records.get(dispatch_index)
                else {
                    return Ok(false);
                };
                let Some(Stage6JournalRecordVersioned::V1(accepted)) = dispatch_index
                    .checked_sub(1)
                    .and_then(|index| records.get(index))
                else {
                    return Ok(false);
                };
                let dispatch_payload_matches = matches!(
                    dispatch.payload(),
                    Stage6JournalPayloadV1::DispatchAttemptRecorded {
                        attempt_ordinal: 1,
                        accepted_request_payload_sha256,
                    } if accepted_request_payload_sha256 == accepted.canonical_payload_sha256()
                );
                if accepted.event_kind() != Stage6JournalEventKind::RequestAccepted
                    || accepted.durable_request_identity() != identity
                    || dispatch.durable_request_identity() != identity
                    || dispatch.previous_record_id() != Some(accepted.journal_record_id())
                    || dispatch.causal_parent_id() != Some(accepted.journal_record_id())
                    || !dispatch_payload_matches
                {
                    return Ok(false);
                }
                let mut before_dispatch = Stage6MemoryJournalBackend::new();
                for record in records.iter().take(dispatch_index) {
                    before_dispatch.append_versioned(record)?;
                }
                let before_dispatch_checkpoint =
                    Stage6JournalCheckpointV1::from_frontier(before_dispatch.frontier().clone())?;
                before_dispatch.append_versioned(&records[dispatch_index])?;
                let after_dispatch_checkpoint =
                    Stage6JournalCheckpointV1::from_frontier(before_dispatch.frontier().clone())?;
                Ok(replacement.authenticated_stage6_checkpoint_sha256()
                    == before_dispatch_checkpoint.checkpoint_sha256()
                    && evidence.stage6_predecessor_checkpoint_sha256()
                        == after_dispatch_checkpoint.checkpoint_sha256())
            })()?;
            if !ordinary_autonomous_chain
                && !schedule_bound_autonomous_chain
                && !target_first_cancel_chain
            {
                return Err(Stage6dLiveCoreError::DurableOrderingViolation);
            }
        }
        Some(effect) => {
            let dispatch_index = recovered
                .journal
                .versioned_records()
                .iter()
                .position(|record| {
                    matches!(
                        record,
                        Stage6JournalRecordVersioned::V1(record)
                            if record.journal_record_id().as_str()
                                == effect.stage6_dispatch_record_id
                    )
                })
                .ok_or(Stage6dLiveCoreError::DispatchAttemptRecordRequired)?;
            let mut before_dispatch = Stage6MemoryJournalBackend::new();
            for record in recovered
                .journal
                .versioned_records()
                .iter()
                .take(dispatch_index)
            {
                before_dispatch.append_versioned(record)?;
            }
            let before_dispatch_checkpoint =
                Stage6JournalCheckpointV1::from_frontier(before_dispatch.frontier().clone())?;
            before_dispatch.append_versioned(
                recovered
                    .journal
                    .versioned_records()
                    .get(dispatch_index)
                    .ok_or(Stage6dLiveCoreError::DispatchAttemptRecordRequired)?,
            )?;
            let after_dispatch_checkpoint =
                Stage6JournalCheckpointV1::from_frontier(before_dispatch.frontier().clone())?;
            let ordinary_request_chain = replacement.authenticated_stage6_checkpoint_sha256()
                == before_dispatch_checkpoint.checkpoint_sha256()
                && evidence.stage6_predecessor_checkpoint_sha256()
                    == after_dispatch_checkpoint.checkpoint_sha256();
            let schedule_bound_request_chain = schedule_checkpoint.is_some();
            let target_first_recovered_cancel = matches!(
                evidence.outcome_kind(),
                crate::stage8b_p1d3_working_limit::Stage8bP1d3OutcomeKind::CancelExecutionObserved
                    | crate::stage8b_p1d3_working_limit::Stage8bP1d3OutcomeKind::CancelAlreadyTerminalNonExecution
            ) && matches!(
                replacement.phase(),
                crate::stage8b_p1d3_working_limit::Stage8bP1d3BookPhase::Terminal
            ) && replacement.authenticated_stage6_checkpoint_sha256()
                == evidence.stage6_predecessor_checkpoint_sha256();
            if !ordinary_request_chain
                && !schedule_bound_request_chain
                && !target_first_recovered_cancel
            {
                return Err(Stage6dLiveCoreError::DurableOrderingViolation);
            }
        }
    }
    let mut finalized_record = None;
    if let Some(effect) = request_effect {
        let accepted = stage7a_accepted_record(recovered, effect.request_id)
            .ok_or(Stage6dLiveCoreError::AcceptedRecordRequired)?;
        let command = match accepted.payload() {
            Stage6JournalPayloadV1::RequestAccepted { command } => command.as_ref().clone(),
            _ => return Err(Stage6dLiveCoreError::AcceptedRecordRequired),
        };
        let authority = recovered.authorize_stage8a4_durable_batch_source(
            accepted.durable_request_identity(),
            &command,
        )?;
        evidence.validate_request_authority(&authority)?;
        let observed_at = Utc
            .timestamp_millis_opt(evidence.transition_received_ts_utc_ms())
            .single()
            .ok_or(Stage6dLiveCoreError::Stage8bP1d3WorkingLimit)?;
        let finalized = stage7a_request_finalization_record(
            accepted.durable_request_identity().clone(),
            outcome_record.journal_record_id().clone(),
            outcome_record.lifecycle_sequence().get(),
            observed_at,
            Stage6RequestFinalDispositionV1::Completed,
        )?;
        let Some((expected_id, expected_fingerprint)) = evidence.stage7_request_finalized_binding()
        else {
            return Err(Stage6dLiveCoreError::DurableOrderingViolation);
        };
        if finalized.journal_record_id().as_str() != expected_id
            || finalized.source_evidence_sha256().as_str() != expected_fingerprint
        {
            return Err(Stage6dLiveCoreError::DurableOrderingViolation);
        }
        if candidate.request_finalized {
            if recovered.journal_frontier().last_record_id() != Some(finalized.journal_record_id())
                || recovered.journal_frontier().last_lifecycle_sequence()
                    != Some(finalized.lifecycle_sequence())
            {
                return Err(Stage6dLiveCoreError::DurableOrderingViolation);
            }
        } else {
            if recovered.journal_frontier().last_record_id()
                != Some(outcome_record.journal_record_id())
                || recovered.journal_frontier().last_lifecycle_sequence()
                    != Some(outcome_record.lifecycle_sequence())
            {
                return Err(Stage6dLiveCoreError::DurableOrderingViolation);
            }
            recovered
                .journal_mut()
                .append(&finalized)
                .map_err(classify_stage8a4_append_error)?;
            recovered
                .refresh_after_append()
                .map_err(|_| Stage6dLiveCoreError::JournalMutationMayHaveOccurred)?;
        }
        finalized_record = Some(finalized);
    } else {
        let record_matches = recovered.journal_frontier().last_record_id()
            == Some(outcome_record.journal_record_id());
        let sequence_matches = recovered.journal_frontier().last_lifecycle_sequence()
            == Some(outcome_record.lifecycle_sequence());
        if candidate.request_finalized || !record_matches || !sequence_matches {
            return Err(Stage6dLiveCoreError::DurableOrderingViolation);
        }
    }

    let (request_finalized_record_id, request_finalized_fingerprint_sha256) = finalized_record
        .as_ref()
        .map(|record| {
            (
                Some(record.journal_record_id().as_str().to_string()),
                Some(record.source_evidence_sha256().as_str().to_string()),
            )
        })
        .unwrap_or((None, None));
    let recovery_binding = crate::stage8b_p1d3_working_limit::Stage8bP1d3Stage6RecoveryBinding {
        outcome_record_id: outcome_record.journal_record_id().as_str().to_string(),
        predecessor_frontier_sha256: evidence.stage6_predecessor_checkpoint_sha256().to_string(),
        reserved_checkpoint_sha256: evidence.stage6_reserved_checkpoint_sha256().to_string(),
        request_finalized_record_id,
        request_finalized_fingerprint_sha256,
        authenticated_post_checkpoint_sha256: recovered
            .authenticated_checkpoint()
            .checkpoint_sha256()
            .to_string(),
    };
    outcome_record.authenticate_p1d3_outcome(recovery_binding.clone())?;
    Ok(Stage6Stage8bP1d3OutcomeAppendReceipt {
        outcome_record,
        recovery_binding,
    })
}

/// Test-only construction of the forbidden two-record S0-ahead suffix. This
/// proves that the P1 restart exception remains limited to one exact
/// RequestAccepted and cannot absorb a Stage 7B dispatch attempt.
#[cfg(feature = "stage5g-artifact-fixtures")]
#[doc(hidden)]
pub fn stage8b_p1_test_append_dispatch_attempt(
    recovered: &mut Stage6dDurableRuntimeRecovered,
    identity: &Stage6DurableRequestIdentityV1,
) -> Result<(), Stage6dLiveCoreError> {
    let accepted = stage7a_accepted_record(recovered, identity.strategy_request_id())
        .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?;
    let dispatch = Stage6JournalRecordV1::dispatch_attempt_recorded(
        identity.clone(),
        1,
        accepted.canonical_payload_sha256().clone(),
        Stage6LifecycleSequence::new(2)?,
        Some(accepted.journal_record_id().clone()),
        Stage6Sha256Digest::parse("f".repeat(64))?,
    )?;
    recovered.journal_mut().append(&dispatch)?;
    recovered.refresh_after_append()
}

/// Test-only P1-d4 seam that appends the dispatch row for the exact current
/// semantic command without invoking any provider or schedule authority.
/// Production recovery still has to classify the resulting on-disk suffix.
#[cfg(feature = "stage5g-artifact-fixtures")]
#[doc(hidden)]
pub fn stage8b_p1d4_test_append_current_semantic_dispatch(
    recovered: &mut Stage6dDurableRuntimeRecovered,
) -> Result<(), Stage6dLiveCoreError> {
    let Stage6dStage5RuntimeAuthority::Restart(restart) = &recovered.stage5_runtime else {
        return Err(Stage6dLiveCoreError::RestartRuntimeRequired);
    };
    let identity = restart
        .stage8b_p1_semantic_commit()
        .and_then(|projection| projection.durable_request_identity.as_ref())
        .cloned()
        .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?;
    stage8b_p1_test_append_dispatch_attempt(recovered, &identity)
}

fn replay_versioned_journal(
    journal: &Stage6OwnedJournalBackend,
) -> Result<Stage6ReplaySnapshotV1, Stage6dLiveCoreError> {
    let mixed = Stage6MixedReplayEngineV2::replay(journal.versioned_records())?;
    Ok(Stage6ReplaySnapshotV1::from_recovered_requests(
        mixed.into_requests(),
    ))
}

#[cfg(feature = "stage5g-artifact-fixtures")]
#[doc(hidden)]
pub fn stage8a4_test_set_journal_failpoint(
    recovered: &mut Stage6dDurableRuntimeRecovered,
    failpoint: Option<crate::stage6_journal_backend::TestIoFailpoint>,
) -> Result<(), Stage6dLiveCoreError> {
    recovered
        .journal
        .stage8a4_test_set_failpoint(failpoint)
        .map_err(Stage6dLiveCoreError::from)
}

/// Test-only issuer using the RFC 8032 test-vector key whose public half is
/// pinned by Stage 6/7 fixture operational identities.
#[cfg(feature = "stage5g-artifact-fixtures")]
#[doc(hidden)]
#[allow(clippy::too_many_arguments)]
pub fn stage8a4_test_attest_validated_entry(
    identity: Stage6DurableRequestIdentityV1,
    command: Stage6DurableCommandSnapshotV1,
    batch: Stage6Stage8a4DurableBatch,
    operational_identity_sha256: String,
    runtime_config_fingerprint_sha256: String,
    seal_generation: u64,
    seal_commitment_sha256: String,
    source_evidence_binding_sha256: Stage6Sha256Digest,
    writer_truth_binding_sha256: Stage6Sha256Digest,
    control_binding_sha256: Stage6Sha256Digest,
) -> Result<Stage6Stage8a4ValidatedWriteEntry, Stage6dLiveCoreError> {
    use ed25519_dalek::{Signer as _, SigningKey};

    let seed =
        decode_fixed_hex::<32>("9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60")?;
    let signer = SigningKey::from_bytes(&seed);
    let attestation = stage8a4_writer_entry_attestation_sha256(
        &identity,
        &command,
        &batch,
        &operational_identity_sha256,
        &runtime_config_fingerprint_sha256,
        seal_generation,
        &seal_commitment_sha256,
        &source_evidence_binding_sha256,
        &writer_truth_binding_sha256,
        &control_binding_sha256,
    )?;
    let signature = signer.sign(attestation.as_str().as_bytes());
    Stage6Stage8a4ValidatedWriteEntry::verify_issuer_attestation(
        identity,
        command,
        batch,
        operational_identity_sha256,
        runtime_config_fingerprint_sha256,
        seal_generation,
        seal_commitment_sha256,
        source_evidence_binding_sha256,
        writer_truth_binding_sha256,
        control_binding_sha256,
        encode_lower_hex(signer.verifying_key().as_bytes()),
        encode_lower_hex(&signature.to_bytes()),
    )
}

#[cfg(feature = "stage5g-artifact-fixtures")]
fn encode_lower_hex(bytes: &[u8]) -> String {
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        write!(&mut encoded, "{byte:02x}").expect("writing into String cannot fail");
    }
    encoded
}

/// Applies an authenticated broker-neutral I3 authority. Caller-provided V2
/// or batch material cannot enter this boundary directly; Stage 7B still owns
/// S0 validation and the covering S1 commit/reread.
pub fn apply_stage8a4_validated_writer_entry(
    recovered: &mut Stage6dDurableRuntimeRecovered,
    commitment_key: &Stage5gLifecycleCommitmentKey,
    entry: Stage6Stage8a4ValidatedWriteEntry,
) -> Result<Stage6Stage8a4BatchAppendReceipt, Stage6dLiveCoreError> {
    let authenticated_identity = recovered
        .authenticated_operational_identity()
        .ok_or(Stage6dLiveCoreError::Stage8a4WriteAuthorityInvalid)?;
    if authenticated_identity.stage8a4_writer_issuer_public_key_hex != entry.issuer_public_key_hex()
    {
        return Err(Stage6dLiveCoreError::Stage8a4WriteAuthorityInvalid);
    }
    let authority = entry.into_sealed(commitment_key)?.verify(commitment_key)?;
    let Stage6Stage8a4SealedWriteAuthority {
        identity,
        command,
        batch,
        ..
    } = authority;
    let durable = recovered.authorize_stage8a4_durable_batch_source(&identity, &command)?;
    stage8a4_internal_append_durable_batch(recovered, durable, batch)
}

pub(crate) fn stage8a4_internal_append_durable_batch(
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
    let mut mutation_attempted = false;
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
            mutation_attempted = true;
            recovered
                .journal_mut()
                .append_versioned(&Stage6JournalRecordVersioned::V2(transition.clone()))
                .map_err(classify_stage8a4_append_error)?;
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
        mutation_attempted = true;
        recovered
            .journal_mut()
            .append(record)
            .map_err(classify_stage8a4_append_error)?;
        appended_suffix_records += 1;
    }
    recovered.refresh_after_append().map_err(|error| {
        if mutation_attempted {
            Stage6dLiveCoreError::JournalMutationMayHaveOccurred
        } else {
            error
        }
    })?;

    let final_mixed = Stage6MixedReplayEngineV2::replay(recovered.journal.versioned_records())
        .map_err(|error| {
            if mutation_attempted {
                Stage6dLiveCoreError::JournalMutationMayHaveOccurred
            } else {
                Stage6dLiveCoreError::from(error)
            }
        })?;
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
        .ok_or(if mutation_attempted {
            Stage6dLiveCoreError::JournalMutationMayHaveOccurred
        } else {
            Stage6dLiveCoreError::DurableOrderingViolation
        })?;
    if final_batch.completion() != Stage6ReconciliationBatchCompletionV2::Complete
        || final_batch.verified_suffix_prefix_length() != batch.suffix_records.len()
    {
        return Err(if mutation_attempted {
            Stage6dLiveCoreError::JournalMutationMayHaveOccurred
        } else {
            Stage6dLiveCoreError::DurableOrderingViolation
        });
    }
    Ok(Stage6Stage8a4BatchAppendReceipt {
        checkpoint: recovered.authenticated_checkpoint().clone(),
        transition_was_existing,
        appended_suffix_records,
    })
}

fn classify_stage8a4_append_error(error: Stage6JournalStorageError) -> Stage6dLiveCoreError {
    if error == Stage6JournalStorageError::DurabilityUncertain {
        Stage6dLiveCoreError::JournalMutationMayHaveOccurred
    } else {
        Stage6dLiveCoreError::Journal(error)
    }
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
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[doc(hidden)]
pub enum Stage7bTestExtraStage6History {
    None,
    Finalized,
    UnboundNonFinal,
}

#[cfg(any(test, feature = "stage5g-artifact-fixtures"))]
#[allow(dead_code)]
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
#[allow(dead_code)]
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
#[allow(dead_code)]
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
    LimitExpired {
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
    command_snapshot: Stage6DurableCommandSnapshotV1,
    accepted_command_payload_sha256: Stage6Sha256Digest,
    dispatch_record_id: Stage6JournalRecordId,
    dispatch_sequence: Stage6LifecycleSequence,
    durable_frontier_sha256: String,
}

impl Stage6dPaperDispatchReceipt {
    /// Crate-private identity view used by the P1-d1 provider gate.  The
    /// dispatch receipt itself remains linear and opaque to downstream code.
    pub(crate) fn stage8b_p1d1_identity(&self) -> &Stage6DurableRequestIdentityV1 {
        &self.identity
    }

    pub(crate) fn stage8b_p1d1_command_snapshot(&self) -> &Stage6DurableCommandSnapshotV1 {
        &self.command_snapshot
    }

    pub(crate) fn stage8b_p1d1_accepted_payload_sha256(&self) -> &Stage6Sha256Digest {
        &self.accepted_command_payload_sha256
    }
}

#[cfg(test)]
pub(crate) fn stage8b_p1d1_test_dispatch_receipt(
    command: &BrokerCommand,
) -> Stage6dPaperDispatchReceipt {
    let BrokerCommand::PlaceOrder(place) = command else {
        panic!("P1-d1 test receipt supports PLACE only");
    };
    let attribution = HybridRuntimeAttribution::parse_source_comment(
        place.comment.as_deref().expect("test command attribution"),
    )
    .expect("test command attribution must parse");
    let identity = Stage6DurableRequestIdentityV1::from_place(place, attribution)
        .expect("test command identity must validate");
    let command_snapshot = Stage6DurableCommandSnapshotV1::from_place(&identity, place)
        .expect("test command snapshot must validate");
    let accepted_command_payload_sha256 = Stage6Sha256Digest::parse(sha256_hex(
        &serde_json::to_vec(&command_snapshot).expect("test snapshot must serialize"),
    ))
    .expect("test snapshot digest must validate");
    stage8b_p1d1_test_dispatch_receipt_with_payload_sha256(command, accepted_command_payload_sha256)
}

#[cfg(test)]
pub(crate) fn stage8b_p1d1_test_dispatch_receipt_with_payload_sha256(
    command: &BrokerCommand,
    accepted_command_payload_sha256: Stage6Sha256Digest,
) -> Stage6dPaperDispatchReceipt {
    let BrokerCommand::PlaceOrder(place) = command else {
        panic!("P1-d1 test receipt supports PLACE only");
    };
    let attribution = HybridRuntimeAttribution::parse_source_comment(
        place.comment.as_deref().expect("test command attribution"),
    )
    .expect("test command attribution must parse");
    let identity = Stage6DurableRequestIdentityV1::from_place(place, attribution)
        .expect("test command identity must validate");
    let command_snapshot = Stage6DurableCommandSnapshotV1::from_place(&identity, place)
        .expect("test command snapshot must validate");
    Stage6dPaperDispatchReceipt {
        identity,
        command_snapshot,
        accepted_command_payload_sha256,
        dispatch_record_id: Stage6JournalRecordId::derive(
            place.request_id,
            Stage6LifecycleSequence::new(2).expect("test sequence"),
        ),
        dispatch_sequence: Stage6LifecycleSequence::new(2).expect("test sequence"),
        durable_frontier_sha256: "a".repeat(64),
    }
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
    durable_request_identity: Stage6DurableRequestIdentityV1,
    strategy_request_id: StrategyRequestId,
    durable_client_order_id: broker_core::ClientOrderId,
    broker_order_id: Option<BrokerOrderId>,
    broker_trade_ids: Vec<BrokerTradeId>,
    canonical_command_sha256: Stage6Sha256Digest,
    final_disposition: Stage6RequestFinalDispositionV1,
    final_record_id: Stage6JournalRecordId,
    final_sequence: u64,
}

/// Broker-neutral, source-derived terminal facts for one complete Stage 8A-4
/// mixed-replay batch.  This value is deliberately opaque and
/// non-serializable: it proves history shape only and grants no writer,
/// dispatch, publication, or transport authority.
pub struct Stage8a4CompletedTransitionFacts {
    identity: Stage6DurableRequestIdentityV1,
    lifecycle: Stage6ReconciliationLifecycleV2,
    cancel_outcome: Option<Stage6CancelOutcomeV1>,
    final_disposition: Stage6RequestFinalDispositionV1,
    broker_order_id: Option<BrokerOrderId>,
    canonical_command_sha256: Stage6Sha256Digest,
    final_record_id: Stage6JournalRecordId,
    final_sequence: u64,
    runtime_config_fingerprint_sha256: String,
}

impl Stage8a4CompletedTransitionFacts {
    pub fn identity(&self) -> &Stage6DurableRequestIdentityV1 {
        &self.identity
    }

    pub fn lifecycle(&self) -> Stage6ReconciliationLifecycleV2 {
        self.lifecycle
    }

    pub fn cancel_outcome(&self) -> Option<Stage6CancelOutcomeV1> {
        self.cancel_outcome
    }

    pub fn final_disposition(&self) -> Stage6RequestFinalDispositionV1 {
        self.final_disposition
    }

    pub fn broker_order_id(&self) -> Option<&BrokerOrderId> {
        self.broker_order_id.as_ref()
    }

    pub fn canonical_command_sha256(&self) -> &Stage6Sha256Digest {
        &self.canonical_command_sha256
    }

    pub fn final_record_id(&self) -> &Stage6JournalRecordId {
        &self.final_record_id
    }

    pub fn final_sequence(&self) -> u64 {
        self.final_sequence
    }

    pub fn runtime_config_fingerprint_sha256(&self) -> &str {
        &self.runtime_config_fingerprint_sha256
    }
}

impl Stage7bFinalizedRequestFacts {
    pub fn durable_request_identity(&self) -> &Stage6DurableRequestIdentityV1 {
        &self.durable_request_identity
    }

    pub fn strategy_request_id(&self) -> StrategyRequestId {
        self.strategy_request_id
    }

    pub fn durable_client_order_id(&self) -> &broker_core::ClientOrderId {
        &self.durable_client_order_id
    }

    pub fn broker_order_id(&self) -> Option<&BrokerOrderId> {
        self.broker_order_id.as_ref()
    }

    pub fn broker_trade_ids(&self) -> &[BrokerTradeId] {
        &self.broker_trade_ids
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
        durable_request_identity: accepted.durable_request_identity().clone(),
        strategy_request_id: request_id,
        durable_client_order_id: request.durable_client_order_id().clone(),
        broker_order_id: request.known_broker_order_id().cloned(),
        broker_trade_ids: request.observed_broker_trade_ids().to_vec(),
        canonical_command_sha256: accepted.canonical_payload_sha256().clone(),
        final_disposition,
        final_record_id: request.last_unique_record_id().clone(),
        final_sequence: request.last_unique_sequence(),
    })
}

/// Reconstructs one exact, terminal Stage 8A-4 transition solely from the
/// recovered mixed journal. Pending suffixes, duplicate batches, hold
/// transitions, CANCEL/Working and any V2/F1 disagreement fail closed.
pub fn stage8a4_completed_transition_facts(
    recovered: &Stage6dDurableRuntimeRecovered,
    request_id: StrategyRequestId,
) -> Result<Stage8a4CompletedTransitionFacts, Stage6dLiveCoreError> {
    let mixed = Stage6MixedReplayEngineV2::replay(recovered.journal.versioned_records())?;
    let mut matching = mixed.reconciliation_batches().iter().filter(|batch| {
        batch
            .transition_record()
            .durable_request_identity()
            .strategy_request_id()
            == request_id
    });
    let batch = matching
        .next()
        .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?;
    if matching.next().is_some()
        || batch.completion() != Stage6ReconciliationBatchCompletionV2::Complete
        || batch.verified_suffix_prefix_length() != batch.suffix_manifest().entries().len()
        || !batch.missing_suffix_entries().is_empty()
    {
        return Err(Stage6dLiveCoreError::DurableOrderingViolation);
    }
    let Stage6ReconciliationTransitionKindV2::Exact { lifecycle } = *batch.transition_kind() else {
        return Err(Stage6dLiveCoreError::DurableOrderingViolation);
    };
    let identity = batch.transition_record().durable_request_identity();
    if identity.action() == Stage6DurableActionKind::Cancel
        && lifecycle == Stage6ReconciliationLifecycleV2::Working
    {
        return Err(Stage6dLiveCoreError::DurableOrderingViolation);
    }

    let finalized = stage7b_finalized_request_facts(recovered, request_id)?;
    let replayed = mixed
        .requests()
        .iter()
        .find(|request| request.strategy_request_id() == request_id)
        .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?;
    if replayed.durable_client_order_id() != identity.durable_client_order_id()
        || replayed.action() != identity.action()
        || replayed.known_broker_order_id() != finalized.broker_order_id()
        || replayed.cancel_outcome() != finalized_request_cancel_outcome(recovered, request_id)?
        || replayed.final_disposition() != Some(finalized.final_disposition())
        || replayed.last_unique_record_id() != batch.last_mixed_record_id()
        || replayed.last_unique_sequence() != batch.last_mixed_lifecycle_sequence().get()
        || finalized.final_record_id() != batch.last_mixed_record_id()
        || finalized.final_sequence() != batch.last_mixed_lifecycle_sequence().get()
    {
        return Err(Stage6dLiveCoreError::DurableOrderingViolation);
    }

    let expected_disposition = match (identity.action(), lifecycle) {
        (Stage6DurableActionKind::Place, Stage6ReconciliationLifecycleV2::TerminalRejected) => {
            Stage6RequestFinalDispositionV1::Rejected
        }
        (Stage6DurableActionKind::Place, _) | (Stage6DurableActionKind::Cancel, _) => {
            Stage6RequestFinalDispositionV1::Completed
        }
    };
    if finalized.final_disposition() != expected_disposition {
        return Err(Stage6dLiveCoreError::DurableOrderingViolation);
    }

    let runtime_config_fingerprint_sha256 = match &recovered.stage5_runtime {
        Stage6dStage5RuntimeAuthority::FirstBoot(runtime) => runtime.stage5c_config_fingerprint(),
        Stage6dStage5RuntimeAuthority::Restart(restart) => {
            restart.config_fingerprint_sha256().to_string()
        }
    };
    Ok(Stage8a4CompletedTransitionFacts {
        identity: identity.clone(),
        lifecycle,
        cancel_outcome: replayed.cancel_outcome(),
        final_disposition: finalized.final_disposition(),
        broker_order_id: replayed.known_broker_order_id().cloned(),
        canonical_command_sha256: finalized.canonical_command_sha256().clone(),
        final_record_id: finalized.final_record_id().clone(),
        final_sequence: finalized.final_sequence(),
        runtime_config_fingerprint_sha256,
    })
}

fn finalized_request_cancel_outcome(
    recovered: &Stage6dDurableRuntimeRecovered,
    request_id: StrategyRequestId,
) -> Result<Option<Stage6CancelOutcomeV1>, Stage6dLiveCoreError> {
    recovered
        .replay()
        .request(request_id)
        .map(Stage6RecoveredRequestV1::cancel_outcome)
        .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)
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

/// P1-specific pre-effect transition.  It consumes exact next-bar eligibility
/// before the sole Stage6 writer may append DispatchAttemptRecorded.  The
/// accepted command snapshot and full durable identity are re-read from the
/// journal and cross-bound before any mutation.
pub fn admit_stage7a_p1d1_market_dispatch(
    recovered: &mut Stage6dDurableRuntimeRecovered,
    eligibility: crate::Stage8bP1d1ExecutionEligible,
) -> Result<crate::Stage8bP1d1MarketDispatchReady, Stage6dLiveCoreError> {
    let identity = eligibility.durable_identity();
    let request_id = identity.strategy_request_id();
    let accepted = stage7a_accepted_record(recovered, request_id)
        .ok_or(Stage6dLiveCoreError::AcceptedRecordRequired)?
        .clone();
    let accepted_snapshot = match accepted.payload() {
        Stage6JournalPayloadV1::RequestAccepted { command } => command.as_ref(),
        _ => return Err(Stage6dLiveCoreError::AcceptedRecordRequired),
    };
    let replayed = recovered
        .replay()
        .request(request_id)
        .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?;
    let market_binding = stage8b_p1e_market_binding_tail(
        recovered,
        request_id,
        eligibility.canonical_command_sha256(),
    );
    let (predecessor_sequence, predecessor_record_id) = if let Some(record) = market_binding {
        (
            record.lifecycle_sequence(),
            record.journal_record_id().clone(),
        )
    } else if replayed.last_unique_record_id() == accepted.journal_record_id()
        && replayed.last_unique_sequence() == accepted.lifecycle_sequence().get()
        && recovered.journal_frontier().last_record_id() == Some(accepted.journal_record_id())
    {
        (
            accepted.lifecycle_sequence(),
            accepted.journal_record_id().clone(),
        )
    } else {
        return Err(Stage6dLiveCoreError::DurableOrderingViolation);
    };
    if accepted.durable_request_identity() != identity
        || accepted_snapshot != eligibility.durable_command_snapshot()
        || accepted.canonical_payload_sha256() != eligibility.accepted_command_payload_sha256()
        || replayed.dispatch_safety_state()
            != crate::Stage6DispatchSafetyStateV1::ReadyForFirstDispatch
        || replayed.dispatch_attempt_count() != 0
        || replayed.last_unique_record_id() != &predecessor_record_id
        || replayed.last_unique_sequence() != predecessor_sequence.get()
    {
        return Err(Stage6dLiveCoreError::DurableOrderingViolation);
    }
    let observed_at =
        DateTime::<Utc>::from_timestamp_millis(eligibility.execution_close_ts_utc_ms())
            .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?;
    let dispatch = stage7a_dispatch_record_after(
        identity,
        &accepted,
        predecessor_sequence,
        &predecessor_record_id,
        observed_at,
    )?;
    let receipt = prepare_stage6d_existing_accepted_paper_dispatch(recovered, &accepted, dispatch)?;
    crate::stage8b_p1d1_paper_provider::bind_stage8b_p1d1_market_dispatch(eligibility, receipt)
        .map_err(|_| Stage6dLiveCoreError::DurableOrderingViolation)
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

fn stage8b_p1e_market_binding_tail<'a>(
    recovered: &'a Stage6dDurableRuntimeRecovered,
    request_id: StrategyRequestId,
    canonical_command_sha256: &str,
) -> Option<&'a Stage6JournalRecordV4> {
    let accepted = stage7a_accepted_record(recovered, request_id)?;
    let Stage6JournalRecordVersioned::V4(record) = recovered.journal.versioned_records().last()?
    else {
        return None;
    };
    let binding = record.request_or_order_binding();
    let request_id_text = request_id.to_string();
    (record.transition_kind() == crate::Stage8bP1eScheduleTransitionKindV1::MarketExecution
        && record.previous_record_id() == accepted.journal_record_id()
        && record.lifecycle_sequence().get()
            == accepted.lifecycle_sequence().get().checked_add(1)?
        && binding.strategy_request_id.as_deref() == Some(request_id_text.as_str())
        && binding.canonical_command_sha256.as_deref() == Some(canonical_command_sha256)
        && recovered.journal_frontier().last_record_id() == Some(record.journal_record_id())
        && recovered.journal_frontier().last_lifecycle_sequence()
            == Some(record.lifecycle_sequence()))
    .then_some(record)
}

fn stage8b_p1e_market_binding_before_recovered_dispatch<'a>(
    recovered: &'a Stage6dDurableRuntimeRecovered,
    request_id: StrategyRequestId,
    canonical_command_sha256: &str,
) -> Result<Option<&'a Stage6JournalRecordV4>, Stage6dLiveCoreError> {
    let accepted = stage7a_accepted_record(recovered, request_id)
        .ok_or(Stage6dLiveCoreError::AcceptedRecordRequired)?;
    let identity = accepted.durable_request_identity();
    let mut dispatches = recovered.journal.records().iter().filter(|record| {
        record.event_kind() == Stage6JournalEventKind::DispatchAttemptRecorded
            && record.durable_request_identity() == identity
    });
    let dispatch = dispatches
        .next()
        .ok_or(Stage6dLiveCoreError::DispatchAttemptRecordRequired)?;
    if dispatches.next().is_some() {
        return Err(Stage6dLiveCoreError::DurableOrderingViolation);
    }
    if dispatch.previous_record_id() == Some(accepted.journal_record_id()) {
        return Ok(None);
    }
    if !stage8b_p1e_dispatch_follows_exact_schedule_binding(recovered, accepted, identity, dispatch)
    {
        return Err(Stage6dLiveCoreError::DurableOrderingViolation);
    }
    let Some(Stage6JournalRecordVersioned::V4(binding)) = recovered
        .journal
        .versioned_records()
        .iter()
        .find(|record| record.journal_record_id() == dispatch.previous_record_id().unwrap())
    else {
        return Err(Stage6dLiveCoreError::DurableOrderingViolation);
    };
    let request_id_text = request_id.to_string();
    let request_binding = binding.request_or_order_binding();
    if binding.transition_kind() != crate::Stage8bP1eScheduleTransitionKindV1::MarketExecution
        || request_binding.strategy_request_id.as_deref() != Some(request_id_text.as_str())
        || request_binding.canonical_command_sha256.as_deref() != Some(canonical_command_sha256)
    {
        return Err(Stage6dLiveCoreError::DurableOrderingViolation);
    }
    Ok(Some(binding))
}

fn stage8b_p1e_dispatch_follows_exact_schedule_binding(
    recovered: &Stage6dDurableRuntimeRecovered,
    accepted: &Stage6JournalRecordV1,
    identity: &Stage6DurableRequestIdentityV1,
    dispatch: &Stage6JournalRecordV1,
) -> bool {
    if !stage8b_p1e_dispatch_follows_exact_schedule_binding_records(
        recovered.journal.versioned_records(),
        accepted,
        identity,
        dispatch,
    ) {
        return false;
    }
    let Some(previous_record_id) = dispatch.previous_record_id() else {
        return false;
    };
    let Some(Stage6JournalRecordVersioned::V4(binding_record)) = recovered
        .journal
        .versioned_records()
        .iter()
        .find(|record| record.journal_record_id() == previous_record_id)
    else {
        return false;
    };
    if binding_record.transition_kind()
        != crate::Stage8bP1eScheduleTransitionKindV1::MarketExecution
    {
        return true;
    }
    let expected_command_sha256 = match &recovered.stage5_runtime {
        Stage6dStage5RuntimeAuthority::Restart(restart) => restart
            .stage8b_p1_semantic_commit()
            .filter(|semantic| semantic.request_id == Some(identity.strategy_request_id()))
            .and_then(|semantic| semantic.canonical_command_sha256.as_deref()),
        Stage6dStage5RuntimeAuthority::FirstBoot(_) => None,
    };
    binding_record
        .request_or_order_binding()
        .canonical_command_sha256
        .as_deref()
        == expected_command_sha256
}

fn stage8b_p1e_dispatch_follows_exact_schedule_binding_records(
    records: &[Stage6JournalRecordVersioned],
    accepted: &Stage6JournalRecordV1,
    identity: &Stage6DurableRequestIdentityV1,
    dispatch: &Stage6JournalRecordV1,
) -> bool {
    let Some(previous_record_id) = dispatch.previous_record_id() else {
        return false;
    };
    let Some(Stage6JournalRecordVersioned::V4(binding_record)) = records
        .iter()
        .find(|record| record.journal_record_id() == previous_record_id)
    else {
        return false;
    };
    if binding_record.previous_record_id() != accepted.journal_record_id()
        || binding_record.lifecycle_sequence().get()
            != accepted.lifecycle_sequence().get().saturating_add(1)
        || dispatch.lifecycle_sequence().get()
            != binding_record.lifecycle_sequence().get().saturating_add(1)
    {
        return false;
    }
    let binding = binding_record.request_or_order_binding();
    match binding_record.transition_kind() {
        crate::Stage8bP1eScheduleTransitionKindV1::MarketExecution => {
            let request_id = identity.strategy_request_id().to_string();
            identity.action() == Stage6DurableActionKind::Place
                && binding.strategy_request_id.as_deref() == Some(request_id.as_str())
                && binding
                    .canonical_command_sha256
                    .as_deref()
                    .is_some_and(|value| Stage6Sha256Digest::parse(value.to_string()).is_ok())
        }
        crate::Stage8bP1eScheduleTransitionKindV1::CancelStep => {
            identity.action() == Stage6DurableActionKind::Cancel
                && identity.target_broker_order_id().is_some_and(|order_id| {
                    binding.active_broker_order_id.as_deref() == Some(order_id.as_str())
                })
        }
        crate::Stage8bP1eScheduleTransitionKindV1::WorkingLimitEvaluation
        | crate::Stage8bP1eScheduleTransitionKindV1::DayExpiry => false,
    }
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

pub(crate) fn stage7a_request_finalization_record(
    identity: Stage6DurableRequestIdentityV1,
    prior_record_id: Stage6JournalRecordId,
    prior_sequence: u64,
    observed_at: DateTime<Utc>,
    disposition: Stage6RequestFinalDispositionV1,
) -> Result<Stage6JournalRecordV1, Stage6dLiveCoreError> {
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
        strategy_request_id: identity.strategy_request_id(),
        final_record_id: prior_record_id.as_str(),
        disposition,
    })
    .map_err(|_| Stage6dLiveCoreError::IntegrationFingerprint)?;
    Stage6JournalRecordV1::request_finalized(
        identity,
        disposition,
        Stage6LifecycleSequence::new(
            prior_sequence
                .checked_add(1)
                .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?,
        )?,
        Some(prior_record_id),
        Stage6Sha256Digest::parse(sha256_hex(&evidence))?,
    )
    .map_err(Stage6dLiveCoreError::from)
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
    let finalized = stage7a_request_finalization_record(
        identity,
        request.last_unique_record_id().clone(),
        report.final_sequence,
        observed_at,
        disposition,
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
        final_record_id: &'a str,
        disposition: Stage6RequestFinalDispositionV1,
    }
    let evidence = serde_json::to_vec(&ReplayFinalizationEvidence {
        domain: "moex.stage7a.paper-command-finalization.v1",
        observed_at,
        strategy_request_id: request_id,
        final_record_id: previous.as_str(),
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
    stage7a_dispatch_record_after(
        identity,
        accepted,
        accepted.lifecycle_sequence(),
        accepted.journal_record_id(),
        observed_at,
    )
}

fn stage7a_dispatch_record_after(
    identity: &Stage6DurableRequestIdentityV1,
    accepted: &Stage6JournalRecordV1,
    predecessor_sequence: Stage6LifecycleSequence,
    predecessor_record_id: &Stage6JournalRecordId,
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
        Stage6LifecycleSequence::new(predecessor_sequence.get().saturating_add(1))?,
        Some(predecessor_record_id.clone()),
        evidence,
    )?)
}

fn prepare_stage6d_existing_accepted_paper_dispatch(
    recovered: &mut Stage6dDurableRuntimeRecovered,
    accepted: &Stage6JournalRecordV1,
    dispatch_attempt: Stage6JournalRecordV1,
) -> Result<Stage6dPaperDispatchReceipt, Stage6dLiveCoreError> {
    let pre_dispatch = recovered
        .replay()
        .request(accepted.durable_request_identity().strategy_request_id())
        .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?;
    if recovered.journal_frontier().last_record_id() != Some(pre_dispatch.last_unique_record_id())
        || recovered.journal_frontier().last_lifecycle_sequence()
            != Stage6LifecycleSequence::new(pre_dispatch.last_unique_sequence()).ok()
    {
        return Err(Stage6dLiveCoreError::DurableOrderingViolation);
    }
    prepare_stage6d_existing_accepted_paper_dispatch_after_current_frontier(
        recovered,
        accepted,
        dispatch_attempt,
    )
}

fn prepare_stage6d_existing_accepted_paper_dispatch_after_schedule_binding(
    recovered: &mut Stage6dDurableRuntimeRecovered,
    accepted: &Stage6JournalRecordV1,
    dispatch_attempt: Stage6JournalRecordV1,
    binding: &Stage6JournalRecordV4,
) -> Result<Stage6dPaperDispatchReceipt, Stage6dLiveCoreError> {
    if recovered.current_stage8b_p1e_schedule_binding_record()? != Some(binding)
        || recovered.journal_frontier().last_record_id() != Some(binding.journal_record_id())
        || recovered.journal_frontier().last_lifecycle_sequence()
            != Some(binding.lifecycle_sequence())
    {
        return Err(Stage6dLiveCoreError::DurableOrderingViolation);
    }
    prepare_stage6d_existing_accepted_paper_dispatch_after_current_frontier(
        recovered,
        accepted,
        dispatch_attempt,
    )
}

fn prepare_stage6d_existing_accepted_paper_dispatch_after_current_frontier(
    recovered: &mut Stage6dDurableRuntimeRecovered,
    accepted: &Stage6JournalRecordV1,
    dispatch_attempt: Stage6JournalRecordV1,
) -> Result<Stage6dPaperDispatchReceipt, Stage6dLiveCoreError> {
    let command_snapshot = match accepted.payload() {
        Stage6JournalPayloadV1::RequestAccepted { command } => command.as_ref().clone(),
        _ => return Err(Stage6dLiveCoreError::AcceptedRecordRequired),
    };
    let pre_dispatch = recovered
        .replay()
        .request(accepted.durable_request_identity().strategy_request_id())
        .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?;
    if accepted.event_kind() != Stage6JournalEventKind::RequestAccepted
        || dispatch_attempt.event_kind() != Stage6JournalEventKind::DispatchAttemptRecorded
        || accepted.durable_request_identity() != dispatch_attempt.durable_request_identity()
        || pre_dispatch.dispatch_attempt_count() != 0
        || pre_dispatch.dispatch_safety_state()
            != crate::Stage6DispatchSafetyStateV1::ReadyForFirstDispatch
        || dispatch_attempt.previous_record_id() != recovered.journal_frontier().last_record_id()
        || dispatch_attempt.lifecycle_sequence().get()
            != recovered
                .journal_frontier()
                .last_lifecycle_sequence()
                .map(Stage6LifecycleSequence::get)
                .and_then(|value| value.checked_add(1))
                .ok_or(Stage6dLiveCoreError::DurableOrderingViolation)?
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
        command_snapshot,
        accepted_command_payload_sha256: accepted.canonical_payload_sha256().clone(),
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
    let command_snapshot = match accepted.payload() {
        Stage6JournalPayloadV1::RequestAccepted { command } => command.as_ref().clone(),
        _ => return Err(Stage6dLiveCoreError::AcceptedRecordRequired),
    };
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
        command_snapshot,
        accepted_command_payload_sha256: accepted.canonical_payload_sha256().clone(),
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
    let generated_market_fill = matches!(&outcome, Stage6dPaperOutcome::MarketFilled { .. });
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
        Stage6dPaperOutcome::LimitPending { broker_order_id }
        | Stage6dPaperOutcome::LimitExpired { broker_order_id } => {
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

    for (record_index, record) in records.into_iter().enumerate() {
        recovered.journal_mut().append(&record)?;
        recovered.refresh_after_append()?;
        if generated_market_fill {
            match record_index {
                0 => stage8b_p1d4_test_crash_frontier("GM05"),
                1 => stage8b_p1d4_test_crash_frontier("GM06"),
                _ => {}
            }
        }
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
                && identity
                    .target_order_client_order_id()
                    .is_some_and(|supplied| {
                        slot.target_order_client_order_id.as_ref() != Some(supplied)
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
                | Stage6dPaperOutcome::LimitExpired { .. }
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
        || decode_fixed_hex::<32>(&config.stage8a4_writer_issuer_public_key_hex).is_err()
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

fn stage8b_p1d3_reservation_sha256(
    pre_dispatch_checkpoint_sha256: &str,
    dispatch_record_id: &str,
    request_id: StrategyRequestId,
    transition_ts_utc_ms: i64,
    source_m10_semantic_id_sha256: &str,
) -> String {
    let request_id = request_id.to_string();
    let mut hasher = Sha256::new();
    hasher.update(b"moex.stage8b.p1d3.stage6-reservation.v1");
    for field in [
        pre_dispatch_checkpoint_sha256.as_bytes(),
        dispatch_record_id.as_bytes(),
        request_id.as_bytes(),
        source_m10_semantic_id_sha256.as_bytes(),
    ] {
        hasher.update((field.len() as u64).to_be_bytes());
        hasher.update(field);
    }
    hasher.update(transition_ts_utc_ms.to_be_bytes());
    format!("{:x}", hasher.finalize())
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
            stage8a4_writer_issuer_public_key_hex:
                "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a".to_string(),
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
    fn p1e_v4_journal_ahead_classifier_accepts_only_one_exact_successor() {
        let fixture = place_fixture(90_001, OrderType::Limit);
        let snapshot =
            Stage6DurableCommandSnapshotV1::from_place(&fixture.identity, &fixture.command)
                .unwrap();
        let accepted = Stage6JournalRecordV1::request_accepted(
            fixture.identity,
            snapshot,
            Stage6LifecycleSequence::new(1).unwrap(),
            None,
            None,
            digest('1'),
        )
        .unwrap();
        let mut predecessor = Stage6MemoryJournalBackend::new();
        predecessor
            .append_versioned(&Stage6JournalRecordVersioned::V1(accepted.clone()))
            .unwrap();
        let checkpoint =
            Stage6JournalCheckpointV1::from_frontier(predecessor.frontier().clone()).unwrap();
        let candidate = crate::stage8b_p1e_test_working_limit_binding_candidate("d".repeat(64));
        let binding = Stage6JournalRecordV4::from_stage8b_p1e_candidate(
            &candidate,
            Stage6LifecycleSequence::new(2).unwrap(),
            accepted.journal_record_id().clone(),
            7,
            Utc.with_ymd_and_hms(2026, 9, 14, 12, 30, 0).unwrap(),
        )
        .unwrap();
        let exact = vec![
            Stage6JournalRecordVersioned::V1(accepted.clone()),
            Stage6JournalRecordVersioned::V4(binding.clone()),
        ];
        assert_eq!(
            classify_stage8b_p1e_schedule_journal_ahead_candidate(&exact, &checkpoint)
                .unwrap()
                .unwrap()
                .encode_canonical(),
            binding.encode_canonical()
        );

        let mut extra = exact.clone();
        extra.push(Stage6JournalRecordVersioned::V4(binding));
        assert!(
            classify_stage8b_p1e_schedule_journal_ahead_candidate(&extra, &checkpoint)
                .unwrap()
                .is_none()
        );
        let wrong_checkpoint = Stage6JournalCheckpointV1::from_frontier(
            Stage6MemoryJournalBackend::new().frontier().clone(),
        )
        .unwrap();
        assert!(
            classify_stage8b_p1e_schedule_journal_ahead_candidate(&exact, &wrong_checkpoint,)
                .unwrap()
                .is_none()
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

        let receipt = stage8a4_internal_append_durable_batch(
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
        let replay_receipt = stage8a4_internal_append_durable_batch(
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
        let receipt = stage8a4_internal_append_durable_batch(
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
            stage8a4_internal_append_durable_batch(
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
            stage8a4_internal_append_durable_batch(
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
        stage8a4_internal_append_durable_batch(
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
            stage8a4_internal_append_durable_batch(
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
            stage8a4_internal_append_durable_batch(
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
        let receipt = stage8a4_internal_append_durable_batch(
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
    fn stage8b_p1d1_eligibility_is_consumed_before_the_only_dispatch_append() {
        let mut fixture = place_fixture(111, OrderType::Market);
        fixture.command.ttl_ms = None;
        let (accepted, _) = accepted_and_dispatch_place(&fixture);
        let accepted_payload_sha256 = accepted.canonical_payload_sha256().clone();
        let mut recovered = recovered();
        recovered.journal_mut().append(&accepted).unwrap();
        recovered.refresh_after_append().unwrap();

        assert_eq!(recovered.journal_frontier().frame_count(), 1);
        assert_eq!(
            recovered
                .replay()
                .request(fixture.identity.strategy_request_id())
                .unwrap()
                .dispatch_safety_state(),
            crate::Stage6DispatchSafetyStateV1::ReadyForFirstDispatch
        );

        let predecessor_close_ts_utc_ms = 1_788_422_400_000;
        let eligibility = crate::stage8b_p1d1_paper_provider::stage8b_p1d1_test_eligible(
            &BrokerCommand::PlaceOrder(fixture.command.clone()),
            accepted_payload_sha256,
            predecessor_close_ts_utc_ms,
            predecessor_close_ts_utc_ms + 600_000,
        );
        let ready = admit_stage7a_p1d1_market_dispatch(&mut recovered, eligibility).unwrap();

        assert_eq!(recovered.journal_frontier().frame_count(), 2);
        assert_eq!(
            recovered
                .replay()
                .request(fixture.identity.strategy_request_id())
                .unwrap()
                .dispatch_safety_state(),
            crate::Stage6DispatchSafetyStateV1::ReconciliationRequired
        );
        let outcome = ready.execute();
        assert_eq!(outcome.strategy_request_id(), fixture.command.request_id);
        assert!(!outcome.ack_allowed());
        assert!(!outcome.source_m10_xack_allowed());
    }

    #[test]
    fn stage8b_p1d1_mismatched_eligibility_never_appends_dispatch() {
        let mut baseline = place_fixture(112, OrderType::Market);
        baseline.command.ttl_ms = None;
        let (accepted, _) = accepted_and_dispatch_place(&baseline);
        let accepted_payload_sha256 = accepted.canonical_payload_sha256().clone();
        let mut mutations = Vec::new();

        let mut qty = baseline.command.clone();
        qty.qty = Decimal::new(2, 0);
        mutations.push(qty);

        let mut side = baseline.command.clone();
        side.side = OrderSide::Sell;
        mutations.push(side);

        let mut attribution = baseline.command.clone();
        attribution.comment = Some(
            HybridRuntimeAttribution::parse_source_comment(
                "HYB|sid=hybrid_imoexf|c=different-cycle|o=BO|r=ENTRY",
            )
            .unwrap()
            .internal_comment()
            .to_string(),
        );
        mutations.push(attribution);

        let mut created_ts = baseline.command.clone();
        created_ts.created_ts += chrono::Duration::seconds(1);
        mutations.push(created_ts);

        for mutation in mutations {
            let mut recovered = recovered();
            recovered.journal_mut().append(&accepted).unwrap();
            recovered.refresh_after_append().unwrap();
            let predecessor_close_ts_utc_ms = 1_788_422_400_000;
            let eligibility = crate::stage8b_p1d1_paper_provider::stage8b_p1d1_test_eligible(
                &BrokerCommand::PlaceOrder(mutation),
                accepted_payload_sha256.clone(),
                predecessor_close_ts_utc_ms,
                predecessor_close_ts_utc_ms + 600_000,
            );
            assert!(matches!(
                admit_stage7a_p1d1_market_dispatch(&mut recovered, eligibility),
                Err(Stage6dLiveCoreError::DurableOrderingViolation)
            ));
            assert_eq!(recovered.journal_frontier().frame_count(), 1);
            assert_eq!(
                recovered
                    .replay()
                    .request(baseline.identity.strategy_request_id())
                    .unwrap()
                    .dispatch_safety_state(),
                crate::Stage6DispatchSafetyStateV1::ReadyForFirstDispatch
            );
        }
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
