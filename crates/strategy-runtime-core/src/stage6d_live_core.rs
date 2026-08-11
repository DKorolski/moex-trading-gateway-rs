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
    STAGE5G_FRESH_BROKER_TRUTH_SCHEMA_VERSION,
};
use crate::{
    HybridIntradayRuntimeStrategy, Stage6CancelOutcomeV1, Stage6DurableActionKind,
    Stage6DurableIdentityError, Stage6DurableRequestIdentityV1, Stage6JournalBackend,
    Stage6JournalCheckpointV1, Stage6JournalEventKind, Stage6JournalFrontierV1,
    Stage6JournalRecordId, Stage6JournalRecordV1, Stage6JournalStorageError,
    Stage6LifecycleSequence, Stage6MemoryJournalBackend, Stage6ReconciliationDispositionV1,
    Stage6ReplayEngineV1, Stage6ReplayError, Stage6ReplaySnapshotV1, Stage6Sha256Digest,
};
use broker_core::{
    BrokerOrderId, BrokerOrderSnapshot, BrokerPositionSnapshot, BrokerTradeId, BrokerTradeSnapshot,
    StrategyRequestId,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const STAGE6D_AUTHENTICATED_RESTART_SCHEMA_VERSION: u16 = 1;
pub const STAGE6D_INTEGRATION_FINGERPRINT_SCHEMA_VERSION: u16 = 1;

const STAGE6D_RESTART_COMMITMENT_DOMAIN: &str = "moex.stage6d.authenticated-restart-frontier.v1";
const STAGE6D_INTEGRATION_FINGERPRINT_DOMAIN: &str = "moex.stage6d.durable-runtime-recovered.v1";

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
    RestartCommitmentMismatch,
    RestartAuthenticationFailed,
    Stage5gRestart(Stage5gCleanRestartError),
    Journal(Stage6JournalStorageError),
    Replay(Stage6ReplayError),
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
            Self::RestartCommitmentMismatch => "Stage 6D restart commitment mismatch",
            Self::RestartAuthenticationFailed => "Stage 6D restart authentication failed",
            Self::Stage5gRestart(_) => "authenticated Stage 5G restart failed",
            Self::Journal(_) => "Stage 6 journal validation failed",
            Self::Replay(_) => "Stage 6 deterministic replay failed",
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

/// Linear authorization proving that journal creation was an explicit boot
/// decision. It has no `Clone`, `Copy`, `Serialize` or `Deserialize`.
pub struct Stage6dFirstBootAuthorization {
    deployment_id: String,
    expected_runtime_config_fingerprint_sha256: Stage6Sha256Digest,
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
    validate_operational_identity_config(&operational_identity)?;
    let operational_identity_sha256 = sha256_hex(
        &serde_json::to_vec(&operational_identity)
            .map_err(|_| Stage6dLiveCoreError::OperationalIdentityInvalid)?,
    );
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

enum Stage6dStage5RuntimeAuthority {
    FirstBoot(Box<HybridIntradayRuntimeStrategy>),
    Restart(Box<Stage5gCleanRestartedCapability>),
}

/// The only Stage 6D post-boot authority. It owns the Stage 5 runtime
/// authority, validated journal and deterministic replay snapshot together.
/// It intentionally has no `Clone`, `Debug`, `Serialize` or `Deserialize`.
pub struct Stage6dDurableRuntimeRecovered {
    boot_mode: Stage6dBootMode,
    stage5_runtime: Stage6dStage5RuntimeAuthority,
    journal: Stage6MemoryJournalBackend,
    replay: Stage6ReplaySnapshotV1,
    authenticated_checkpoint: Stage6JournalCheckpointV1,
    integration_fingerprint_sha256: Stage6Sha256Digest,
    first_boot_deployment_id: Option<String>,
    authenticated_operational_identity: Option<Stage6dOperationalIdentityConfig>,
}

impl Stage6dDurableRuntimeRecovered {
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

    pub(crate) fn journal_mut(&mut self) -> &mut Stage6MemoryJournalBackend {
        &mut self.journal
    }

    pub(crate) fn refresh_after_append(&mut self) -> Result<(), Stage6dLiveCoreError> {
        self.replay = Stage6ReplayEngineV1::replay(self.journal.records())?;
        self.authenticated_checkpoint =
            Stage6JournalCheckpointV1::from_frontier(self.journal.frontier().clone())?;
        self.integration_fingerprint_sha256 = integration_fingerprint(
            self.boot_mode,
            &self.stage5_runtime,
            &self.replay,
            &self.authenticated_checkpoint,
        )?;
        Ok(())
    }
}

pub fn first_boot_stage6d_paper(
    authorization: Stage6dFirstBootAuthorization,
    fresh_runtime: HybridIntradayRuntimeStrategy,
) -> Result<Stage6dDurableRuntimeRecovered, Stage6dLiveCoreError> {
    let actual = fresh_runtime.stage5c_config_fingerprint();
    if actual
        != authorization
            .expected_runtime_config_fingerprint_sha256
            .as_str()
    {
        return Err(Stage6dLiveCoreError::FirstBootRuntimeConfigMismatch);
    }
    let journal = Stage6MemoryJournalBackend::new();
    if journal.frontier().frame_count() != 0 || !journal.records().is_empty() {
        return Err(Stage6dLiveCoreError::FirstBootJournalNotEmpty);
    }
    let replay = Stage6ReplayEngineV1::replay(journal.records())?;
    let authenticated_checkpoint =
        Stage6JournalCheckpointV1::from_frontier(journal.frontier().clone())?;
    let stage5_runtime = Stage6dStage5RuntimeAuthority::FirstBoot(Box::new(fresh_runtime));
    let integration_fingerprint_sha256 = integration_fingerprint(
        Stage6dBootMode::FirstBoot,
        &stage5_runtime,
        &replay,
        &authenticated_checkpoint,
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
    })
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
    let package =
        decode_and_authenticate_restart_package(authenticated_restart_package, commitment_key)?;
    let journal = Stage6MemoryJournalBackend::from_framed_bytes(journal_bytes)?;
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
    journal: Stage6MemoryJournalBackend,
    authenticated_checkpoint: Stage6JournalCheckpointV1,
    authenticated_operational_identity: Option<Stage6dOperationalIdentityConfig>,
) -> Result<Stage6dDurableRuntimeRecovered, Stage6dLiveCoreError> {
    journal.validate_checkpoint(&authenticated_checkpoint)?;
    let replay = Stage6ReplayEngineV1::replay(journal.records())?;
    let integration_fingerprint_sha256 = integration_fingerprint(
        Stage6dBootMode::Restart,
        &stage5_runtime,
        &replay,
        &authenticated_checkpoint,
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
pub struct Stage6dFreshBrokerTruthInput {
    pub package_id: String,
    pub snapshot_epoch: String,
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

/// Applies restart broker truth through the already accepted Stage 5G
/// reconciliation path. The function consumes the single recovered authority
/// and returns it in every classified outcome; no direct runtime field is
/// reachable here.
pub fn apply_stage6d_restart_fresh_truth(
    mut recovered: Stage6dDurableRuntimeRecovered,
    input: Stage6dFreshBrokerTruthInput,
    commitment_key: &Stage5gLifecycleCommitmentKey,
) -> Result<Stage6dFreshTruthTransition, Stage6dLiveCoreError> {
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
    let request_id = stage6d_match_restart_request(&recovered.replay, &projection)?;
    stage6d_validate_replayed_facts_against_truth(&recovered.replay, request_id, &input)?;

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
    let clean_restore_floor = projection
        .checkpoint
        .payload
        .last_broker_truth_received_at
        .unwrap_or(DateTime::<Utc>::MIN_UTC);
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
            clean_restore_completed_at: clean_restore_floor,
            validation_observed_at: input.captured_at,
        },
    )?;
    let bound = bind_stage5g_fresh_truth_to_clean_restart(restart, validated)?;
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
                input.package_id,
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
                input.package_id,
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
                input.package_id,
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

fn stage6d_match_restart_request(
    replay: &Stage6ReplaySnapshotV1,
    projection: &crate::stage5g_clean_restart::Stage5gFreshTruthRestartProjection,
) -> Result<StrategyRequestId, Stage6dLiveCoreError> {
    let matches = replay
        .requests()
        .iter()
        .filter(|request| {
            projection.slots.iter().any(|slot| {
                slot.command_request_id == request.strategy_request_id().to_string()
                    && slot.command_client_order_id == *request.durable_client_order_id()
            })
        })
        .map(|request| request.strategy_request_id())
        .collect::<Vec<_>>();
    if matches.len() == 1 {
        Ok(matches[0])
    } else {
        Err(Stage6dLiveCoreError::RestartRequestIdentityMismatch)
    }
}

fn stage6d_validate_replayed_facts_against_truth(
    replay: &Stage6ReplaySnapshotV1,
    request_id: StrategyRequestId,
    input: &Stage6dFreshBrokerTruthInput,
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
    };
    use broker_core::{
        BrokerAccountId, CancelOrder, ClientOrderId, Exchange, HybridRuntimeAttribution,
        InstrumentId, Market, OrderSide, OrderStatus, OrderType, PlaceOrder, TimeInForce,
    };
    use chrono::{TimeZone, Utc};
    use rust_decimal::Decimal;
    use uuid::Uuid;

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
        Stage6dFreshBrokerTruthInput,
        Stage5gLifecycleCommitmentKey,
    ) {
        let restart = crate::stage5g_order_position::tests::
            stage5g_edb_restored_generated_working_escrow_fixture();
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
        let attribution = attribution("ENTRY");
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
        let input = Stage6dFreshBrokerTruthInput {
            package_id: "stage6d-working-package-1".to_string(),
            snapshot_epoch: "stage6d-working-epoch-1".to_string(),
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
        Stage6dFreshBrokerTruthInput,
        Stage5gLifecycleCommitmentKey,
    ) {
        let restart =
            crate::stage5g_order_position::tests::stage5g_edb_restored_terminal_applied_fixture();
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
        let attribution = attribution("ENTRY");
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
        let input = Stage6dFreshBrokerTruthInput {
            package_id: "stage6d-terminal-exact-package".to_string(),
            snapshot_epoch: "stage6d-terminal-exact-epoch".to_string(),
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
            a.integration_fingerprint_sha256(),
            b.integration_fingerprint_sha256()
        );
        assert_eq!(a.journal_frontier().frame_count(), 2);
    }

    #[test]
    fn stage6d_restart_truth_uses_accepted_stage5g_application_boundary_once() {
        let (recovered, input, key) = stage6d_stage5g_working_restart_fixture();
        let before = recovered.integration_fingerprint_sha256().clone();
        let transition = apply_stage6d_restart_fresh_truth(recovered, input, &key).unwrap();
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
    fn stage6d_restart_truth_rejects_stage6_request_identity_drift() {
        let restart = crate::stage5g_order_position::tests::
            stage5g_edb_restored_generated_working_escrow_fixture();
        let fixture = place_fixture(99, OrderType::Market);
        let (accepted, dispatch) = accepted_and_dispatch_place(&fixture);
        let mut journal = Stage6MemoryJournalBackend::new();
        journal.append(&accepted).unwrap();
        journal.append(&dispatch).unwrap();
        let checkpoint =
            Stage6JournalCheckpointV1::from_frontier(journal.frontier().clone()).unwrap();
        let recovered = recover_stage6d_restart_from_authorities(
            Stage6dStage5RuntimeAuthority::Restart(Box::new(restart)),
            journal,
            checkpoint,
            Some(operational_config()),
        )
        .unwrap();
        let observed_at = Utc.with_ymd_and_hms(2030, 1, 1, 0, 0, 10).unwrap();
        let input = Stage6dFreshBrokerTruthInput {
            package_id: "stage6d-wrong-request".to_string(),
            snapshot_epoch: "stage6d-wrong-request-epoch".to_string(),
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
        assert!(matches!(
            apply_stage6d_restart_fresh_truth(recovered, input, &key),
            Err(Stage6dLiveCoreError::RestartRequestIdentityMismatch)
        ));
    }

    #[test]
    fn stage6d_already_applied_terminal_truth_is_noop_through_stage5g() {
        let (recovered, input, key) = stage6d_stage5g_terminal_restart_fixture();
        let before = recovered.integration_fingerprint_sha256().clone();
        let transition = apply_stage6d_restart_fresh_truth(recovered, input, &key).unwrap();
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
}
