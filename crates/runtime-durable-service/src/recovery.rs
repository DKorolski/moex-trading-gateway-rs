use super::{
    is_single_linked_regular, open_child_at, Stage7bDurableRootAuthority,
    Stage7bDurableStorageError, Stage7bKernelWriterLease, Stage7bWritableDurableAuthority,
    STAGE7B_JOURNAL_FILE, STAGE7B_RECOVERY_SEAL_FILE,
};
use broker_core::{BrokerCommand, BrokerOrderId, ClientOrderId, StrategyRequestId};
use chrono::{DateTime, Utc};
use runtime_command_bridge::{
    Stage7aCanonicalCommandIdentity, Stage7aDeterministicRejectionEvidence,
    Stage7aPermanentPoisonEvidence,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    ffi::CString,
    fs::File,
    io::{ErrorKind, Read, Write},
    os::fd::AsRawFd,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};
use strategy_runtime_core::{
    admit_stage7a_paper_command, advance_stage6d_restart_package,
    apply_stage8a4_validated_writer_entry, execute_stage6d_paper_outcome,
    finalize_stage7a_paper_request, finalize_stage7a_replayed_paper_request,
    first_boot_stage6d_paper_from_validated_stage5g_seed_with_owned_journal,
    refresh_stage7b_durable_frontier, restart_stage6d_paper_with_owned_journal,
    restore_stage5g_clean_restart, seal_stage6d_restart_package,
    stage6_frontier_fingerprint_sha256, stage6d_operational_identity_sha256,
    stage7b_finalized_request_facts, HybridIntradayRuntimeStrategy, Stage5gLifecycleCommitmentKey,
    Stage6DurableCommandSnapshotV1, Stage6DurableRequestAuthorityV1,
    Stage6DurableRequestIdentityV1, Stage6JournalBackend, Stage6JournalCheckpointV1,
    Stage6JournalRecordVersioned, Stage6MemoryJournalBackend, Stage6MixedReplayEngineV2,
    Stage6OwnedJournalBackend, Stage6RequestFinalDispositionV1, Stage6Stage8a4PendingRecovery,
    Stage6Stage8a4ValidatedWriteEntry, Stage6dDurableRuntimeRecovered,
    Stage6dFirstBootAuthorization, Stage6dLiveCoreError, Stage6dOperationalIdentityConfig,
    Stage6dPaperDispatchReceipt, Stage6dPaperExecutionReport, Stage6dPaperOutcome,
    Stage7aPaperAdmission, Stage7aPaperCommandContext, Stage7bFinalizedRequestFacts,
};
#[cfg(test)]
use strategy_runtime_core::{Stage6Sha256Digest, Stage6Stage8a4DurableBatch};

mod redis_service;
pub(crate) mod redis_settlement;

pub use redis_service::{
    spawn_stage7b_supervised_task, Stage7bCompositeHealthSnapshot,
    Stage7bCompositeReadinessSnapshot, Stage7bPaperReadinessPhase, Stage7bPaperReadinessReason,
    Stage7bRedisService, Stage7bRedisServiceConfig, Stage7bRedisServiceError,
    Stage7bServiceRunSummary, Stage7bServiceSupervisor, Stage7bServiceTaskHandle,
    Stage7bServiceTaskOutput, Stage7bTaskReadinessHandle,
};

use redis_settlement::{
    Stage7bCanonicalRequestPublicationEvidence, Stage7bPreStage6CommandObservation,
    Stage7bPreStage6PoisonObservation, Stage7bRedisSettlementBackend,
    Stage7bRedisSettlementContext, Stage7bRedisSettlementError, Stage7bRedisSettlementOutcome,
};

/// Feature-gated source-exact setup for downstream integration tests of the
/// production Stage 8A-4 composition. It creates a normal Stage7B durable
/// owner through the accepted restart path; it exposes no journal handle,
/// raw batch writer or authority constructor.
#[cfg(feature = "stage8a4-i3-test-fixtures")]
#[doc(hidden)]
pub struct Stage8a4I3ProductionTestSetup {
    pub parent: std::path::PathBuf,
    pub root: std::path::PathBuf,
    pub operational_identity: Stage6dOperationalIdentityConfig,
    pub commitment_key: Stage5gLifecycleCommitmentKey,
    pub runtime: HybridIntradayRuntimeStrategy,
    pub command: BrokerCommand,
    pub command_context: Stage7aPaperCommandContext,
}

#[cfg(feature = "stage8a4-i3-test-fixtures")]
#[doc(hidden)]
pub fn stage8a4_i3_production_test_setup(
) -> (Stage8a4I3ProductionTestSetup, Stage7bRecoveryReadyOwner) {
    let parent = std::env::temp_dir().join(format!(
        "stage8a4-i3-production-{}-{}",
        std::process::id(),
        STAGE7B_RECOVERY_SEAL_TEMP_COUNTER.fetch_add(1, Ordering::SeqCst)
    ));
    std::fs::create_dir(&parent).expect("create Stage8A4 I3 fixture parent");
    stage8a4_i3_production_test_setup_in(parent)
}

#[cfg(feature = "stage8a4-i3-test-fixtures")]
#[doc(hidden)]
pub fn stage8a4_i3_production_test_setup_in(
    parent: std::path::PathBuf,
) -> (Stage8a4I3ProductionTestSetup, Stage7bRecoveryReadyOwner) {
    use strategy_runtime_core::{
        authorize_stage6d_first_boot, stage7b_test_authenticated_working_restart_fixture,
        Stage6dFirstBootConfig, Stage7bTestExtraStage6History,
    };

    let fixture =
        stage7b_test_authenticated_working_restart_fixture(Stage7bTestExtraStage6History::None);
    let mut command = fixture.command;
    let BrokerCommand::PlaceOrder(place) = &mut command else {
        panic!("Stage8A4 production fixture requires PLACE");
    };
    place.created_ts = Utc::now() - chrono::Duration::seconds(1);
    place.ttl_ms = Some(60_000);
    let request_identity = Stage6DurableRequestIdentityV1::from_place(
        place,
        fixture.command_context.attribution().clone(),
    )
    .expect("Stage8A4 fixture request identity");
    let durable_command = Stage6DurableCommandSnapshotV1::from_place(&request_identity, place)
        .expect("Stage8A4 fixture durable command");
    let accepted = strategy_runtime_core::Stage6JournalRecordV1::request_accepted(
        request_identity.clone(),
        durable_command,
        strategy_runtime_core::Stage6LifecycleSequence::new(1)
            .expect("Stage8A4 fixture accepted sequence"),
        None,
        None,
        strategy_runtime_core::Stage6Sha256Digest::parse("3".repeat(64))
            .expect("Stage8A4 fixture accepted frontier"),
    )
    .expect("Stage8A4 fixture accepted record");
    let dispatch = strategy_runtime_core::Stage6JournalRecordV1::dispatch_attempt_recorded(
        request_identity,
        1,
        accepted.canonical_payload_sha256().clone(),
        strategy_runtime_core::Stage6LifecycleSequence::new(2)
            .expect("Stage8A4 fixture dispatch sequence"),
        Some(accepted.journal_record_id().clone()),
        strategy_runtime_core::Stage6Sha256Digest::parse("4".repeat(64))
            .expect("Stage8A4 fixture dispatch frontier"),
    )
    .expect("Stage8A4 fixture dispatch record");
    let journal_records = [accepted, dispatch];
    let operational_identity = Stage6dOperationalIdentityConfig {
        broker_id: "paper".to_string(),
        strategy_instance_id: "hybrid-imoexf".to_string(),
        deployment_id: "stage8a4-i3-production-test".to_string(),
        deployment_generation: 1,
        gateway_instance_id: "gateway-stage8a4-i3-test".to_string(),
        instrument_map_fingerprint_sha256: "1".repeat(64),
        market_data_generation: 1,
        command_consumer_generation: 1,
        stage8a4_writer_issuer_public_key_hex:
            "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a".to_string(),
    };
    let parent = std::fs::canonicalize(parent).expect("canonical Stage8A4 I3 fixture parent");
    let root = parent.join(
        Stage7bDurableRootAuthority::expected_directory_name(&operational_identity)
            .expect("valid Stage8A4 fixture identity"),
    );
    std::fs::create_dir(&root).expect("create Stage8A4 I3 fixture root");
    let authorization = authorize_stage6d_first_boot(Stage6dFirstBootConfig {
        deployment_id: operational_identity.deployment_id.clone(),
        expected_runtime_config_fingerprint_sha256: fixture
            .fresh_runtime
            .stage5c_config_fingerprint(),
        allow_create_missing_journal: true,
    })
    .expect("authorize Stage8A4 fixture first boot");
    let mut storage = Stage7bWritableDurableAuthority::create_new(
        Stage7bDurableRootAuthority::validate(&root, &operational_identity)
            .expect("validate Stage8A4 fixture root"),
        &operational_identity,
        &authorization,
    )
    .expect("create Stage8A4 fixture storage");
    let s0_checkpoint =
        Stage6JournalCheckpointV1::from_frontier(storage.journal.frontier().clone())
            .expect("Stage8A4 fixture S0 checkpoint");
    for record in &journal_records {
        storage
            .journal
            .append(record)
            .expect("append Stage8A4 fixture accepted/dispatch record");
    }
    let package = seal_stage6d_restart_package(
        &fixture.stage5g_authenticated_package,
        s0_checkpoint.clone(),
        operational_identity.clone(),
        &fixture.commitment_key,
    )
    .expect("seal Stage8A4 fixture package");
    let seal = Stage7bRecoverySealV1::new(
        1,
        package,
        s0_checkpoint,
        stage6d_operational_identity_sha256(&operational_identity)
            .expect("digest Stage8A4 fixture identity")
            .as_str()
            .to_string(),
        &fixture.commitment_key,
    )
    .expect("construct Stage8A4 fixture seal");
    storage
        ._writer_lease
        .commit_recovery_seal(&seal)
        .expect("commit Stage8A4 fixture S0");
    drop(storage);
    let restarted = Stage7bRecoveryReadyOwner::restart(
        Stage7bDurableRootAuthority::validate(&root, &operational_identity)
            .expect("revalidate Stage8A4 fixture root"),
        operational_identity.clone(),
        &fixture.commitment_key,
        fixture.fresh_runtime.clone(),
    )
    .expect("restart Stage8A4 fixture owner");
    let Stage7bRestartOutcome::Ready(owner) = restarted else {
        panic!("Stage8A4 fixture must restart Ready");
    };
    (
        Stage8a4I3ProductionTestSetup {
            parent,
            root,
            operational_identity,
            commitment_key: fixture.commitment_key,
            runtime: fixture.fresh_runtime,
            command,
            command_context: fixture.command_context,
        },
        *owner,
    )
}

#[cfg(feature = "stage8a4-i3-test-fixtures")]
#[doc(hidden)]
pub fn stage8a4_i3_test_set_owner_journal_failpoint(
    owner: &mut Stage7bRecoveryReadyOwner,
    failpoint: strategy_runtime_core::Stage8a4JournalTestFailpoint,
) -> Result<(), Stage7bRecoveryError> {
    strategy_runtime_core::stage8a4_test_set_journal_failpoint(
        &mut owner.recovered,
        Some(failpoint),
    )
    .map_err(Stage7bRecoveryError::Runtime)
}

#[cfg(feature = "stage8a4-i3-test-fixtures")]
#[doc(hidden)]
pub fn stage8a4_i3_test_fail_before_covering_seal(owner: &mut Stage7bRecoveryReadyOwner) {
    owner.stage8a4_test_fail_before_covering_seal = true;
}

pub const STAGE7B_RECOVERY_SEAL_SCHEMA_VERSION: u16 = 1;
const STAGE7B_RECOVERY_SEAL_COMMITMENT_DOMAIN: &str = "moex.stage7b.recovery-seal.commitment.v1";
const STAGE7B_RECOVERY_SEAL_MAX_BYTES: u64 = 16 * 1024 * 1024;
const STAGE7B_RECOVERY_SEAL_TEMP_PREFIX: &str = ".stage7b-recovery.seal.";
static STAGE7B_RECOVERY_SEAL_TEMP_COUNTER: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stage7bRecoveryError {
    Storage(Stage7bDurableStorageError),
    Runtime(Stage6dLiveCoreError),
    RuntimeConfigMismatch,
    SealAlreadyExists,
    SealWithoutJournal,
    SealInvalid,
    SealWriteFailed(ErrorKind),
    SealCommitUncertain,
    SealGenerationOverflow,
    FinalizedBindingMismatch,
    ClockInvalid,
}

impl std::fmt::Display for Stage7bRecoveryError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Storage(_) => "Stage 7B durable storage failure",
            Self::Runtime(_) => "Stage 6 authenticated recovery failure",
            Self::RuntimeConfigMismatch => "first-boot runtime configuration mismatch",
            Self::SealAlreadyExists => "recovery seal already exists at first boot",
            Self::SealWithoutJournal => "recovery seal exists without a journal",
            Self::SealInvalid => "recovery seal is malformed, noncanonical or unbound",
            Self::SealWriteFailed(_) => "atomic recovery seal commit failed",
            Self::SealCommitUncertain => "recovery seal commit outcome requires reconciliation",
            Self::SealGenerationOverflow => "recovery seal generation exhausted",
            Self::FinalizedBindingMismatch => {
                "finalized request binding changed before authorization"
            }
            Self::ClockInvalid => "recovery seal timestamp is unavailable",
        })
    }
}

impl std::error::Error for Stage7bRecoveryError {}

#[derive(Debug, thiserror::Error)]
#[allow(dead_code, reason = "Stage 7B-d-b is attached only by closed d-c")]
pub(crate) enum Stage7bDbError {
    #[error("Stage 7B recovery authority rejected settlement")]
    Recovery(#[from] Stage7bRecoveryError),
    #[error("Stage 7B Redis settlement rejected operation")]
    Settlement(#[from] Stage7bRedisSettlementError),
}

impl From<Stage7bDurableStorageError> for Stage7bRecoveryError {
    fn from(value: Stage7bDurableStorageError) -> Self {
        Self::Storage(value)
    }
}

impl From<Stage6dLiveCoreError> for Stage7bRecoveryError {
    fn from(value: Stage6dLiveCoreError) -> Self {
        Self::Runtime(value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage7bRecoveryBlockReason {
    MissingCommittedSeal,
    CorruptCommittedSeal,
    OperationalIdentityMismatch,
    CheckpointMismatch,
    AuthenticatedRestartRejected,
}

/// Explicit no-effect state. It has no provider, Redis settlement or runtime
/// mutation API. When blocking is discovered before journal ownership is
/// transferred, the kernel writer lease remains retained for diagnostics.
pub struct Stage7bRecoveryBlocked {
    reason: Stage7bRecoveryBlockReason,
    _retained_storage: Option<Stage7bWritableDurableAuthority>,
}

impl Stage7bRecoveryBlocked {
    fn retained(
        reason: Stage7bRecoveryBlockReason,
        storage: Stage7bWritableDurableAuthority,
    ) -> Self {
        Self {
            reason,
            _retained_storage: Some(storage),
        }
    }

    fn after_consumed_storage(reason: Stage7bRecoveryBlockReason) -> Self {
        Self {
            reason,
            _retained_storage: None,
        }
    }

    pub fn reason(&self) -> Stage7bRecoveryBlockReason {
        self.reason
    }

    pub fn recovery_ready(&self) -> bool {
        false
    }

    pub fn paper_provider_invocation_allowed(&self) -> bool {
        false
    }

    pub fn redis_settlement_allowed(&self) -> bool {
        false
    }

    pub fn xack_allowed(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stage7bRecoverySealV1 {
    schema_version: u16,
    seal_generation: u64,
    created_at_ts_utc_ms: i64,
    stage6d_authenticated_restart_package: Vec<u8>,
    stage6d_restart_package_sha256: String,
    stage6_checkpoint: Stage6JournalCheckpointV1,
    stage6_checkpoint_bytes_sha256: String,
    operational_identity_sha256: String,
    seal_commitment_sha256: String,
    seal_commitment_hmac_sha256: String,
}

#[derive(Serialize)]
struct Stage7bRecoverySealCommitmentV1<'a> {
    schema_version: u16,
    domain: &'static str,
    seal_generation: u64,
    created_at_ts_utc_ms: i64,
    stage6d_restart_package_sha256: &'a str,
    stage6_checkpoint_bytes_sha256: &'a str,
    operational_identity_sha256: &'a str,
}

impl Stage7bRecoverySealV1 {
    fn new(
        seal_generation: u64,
        stage6d_authenticated_restart_package: Vec<u8>,
        stage6_checkpoint: Stage6JournalCheckpointV1,
        operational_identity_sha256: String,
        commitment_key: &Stage5gLifecycleCommitmentKey,
    ) -> Result<Self, Stage7bRecoveryError> {
        let created_at_ts_utc_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| Stage7bRecoveryError::ClockInvalid)?
            .as_millis()
            .try_into()
            .map_err(|_| Stage7bRecoveryError::ClockInvalid)?;
        let stage6d_restart_package_sha256 = sha256_hex(&stage6d_authenticated_restart_package);
        let stage6_checkpoint_bytes_sha256 = sha256_hex(&stage6_checkpoint.encode_canonical());
        let seal_commitment_sha256 = seal_commitment_sha256(
            seal_generation,
            created_at_ts_utc_ms,
            &stage6d_restart_package_sha256,
            &stage6_checkpoint_bytes_sha256,
            &operational_identity_sha256,
        )?;
        let seal_commitment_hmac_sha256 =
            commitment_key.stage7b_recovery_seal_hmac_sha256(&seal_commitment_sha256);
        let seal = Self {
            schema_version: STAGE7B_RECOVERY_SEAL_SCHEMA_VERSION,
            seal_generation,
            created_at_ts_utc_ms,
            stage6d_authenticated_restart_package,
            stage6d_restart_package_sha256,
            stage6_checkpoint,
            stage6_checkpoint_bytes_sha256,
            operational_identity_sha256,
            seal_commitment_sha256,
            seal_commitment_hmac_sha256,
        };
        seal.validate_against_identity(&seal.operational_identity_sha256, commitment_key)?;
        Ok(seal)
    }

    pub fn seal_generation(&self) -> u64 {
        self.seal_generation
    }

    pub fn created_at_ts_utc_ms(&self) -> i64 {
        self.created_at_ts_utc_ms
    }

    pub fn stage6d_restart_package_sha256(&self) -> &str {
        &self.stage6d_restart_package_sha256
    }

    pub fn stage6_checkpoint(&self) -> &Stage6JournalCheckpointV1 {
        &self.stage6_checkpoint
    }

    pub fn operational_identity_sha256(&self) -> &str {
        &self.operational_identity_sha256
    }

    pub fn seal_commitment_sha256(&self) -> &str {
        &self.seal_commitment_sha256
    }

    pub fn encode_canonical(&self) -> Result<Vec<u8>, Stage7bRecoveryError> {
        serde_json::to_vec(self).map_err(|_| Stage7bRecoveryError::SealInvalid)
    }

    fn decode_canonical(
        bytes: &[u8],
        expected_operational_identity_sha256: &str,
        commitment_key: &Stage5gLifecycleCommitmentKey,
    ) -> Result<Self, Stage7bRecoveryError> {
        let seal: Self =
            serde_json::from_slice(bytes).map_err(|_| Stage7bRecoveryError::SealInvalid)?;
        seal.validate_against_identity(expected_operational_identity_sha256, commitment_key)?;
        if seal.encode_canonical()? != bytes {
            return Err(Stage7bRecoveryError::SealInvalid);
        }
        Ok(seal)
    }

    fn validate_against_identity(
        &self,
        expected_operational_identity_sha256: &str,
        commitment_key: &Stage5gLifecycleCommitmentKey,
    ) -> Result<(), Stage7bRecoveryError> {
        if self.schema_version != STAGE7B_RECOVERY_SEAL_SCHEMA_VERSION
            || self.seal_generation == 0
            || self.created_at_ts_utc_ms <= 0
            || self.stage6d_authenticated_restart_package.is_empty()
            || !is_sha256(&self.stage6d_restart_package_sha256)
            || !is_sha256(&self.stage6_checkpoint_bytes_sha256)
            || !is_sha256(&self.operational_identity_sha256)
            || !is_sha256(&self.seal_commitment_sha256)
            || !is_sha256(&self.seal_commitment_hmac_sha256)
            || self.operational_identity_sha256 != expected_operational_identity_sha256
            || sha256_hex(&self.stage6d_authenticated_restart_package)
                != self.stage6d_restart_package_sha256
            || sha256_hex(&self.stage6_checkpoint.encode_canonical())
                != self.stage6_checkpoint_bytes_sha256
            || seal_commitment_sha256(
                self.seal_generation,
                self.created_at_ts_utc_ms,
                &self.stage6d_restart_package_sha256,
                &self.stage6_checkpoint_bytes_sha256,
                &self.operational_identity_sha256,
            )? != self.seal_commitment_sha256
            || !commitment_key.stage7b_verify_recovery_seal_hmac_sha256(
                &self.seal_commitment_sha256,
                &self.seal_commitment_hmac_sha256,
            )
        {
            return Err(Stage7bRecoveryError::SealInvalid);
        }
        Stage6JournalCheckpointV1::decode_canonical(&self.stage6_checkpoint.encode_canonical())
            .map_err(|_| Stage7bRecoveryError::SealInvalid)?;
        Ok(())
    }
}

/// Linear proof that one exact Stage 6 request is durably finalized. It is
/// reconstructed only from the owner-held journal and is consumed by seal
/// authorization.
#[allow(dead_code, reason = "consumed by the closed Stage 7B-d-b seam")]
pub(crate) struct Stage7bFinalizedPaperRequest {
    facts: Stage7bFinalizedRequestFacts,
}

#[allow(dead_code, reason = "consumed by the closed Stage 7B-d-b seam")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Stage7bAckPublicationDecision {
    Canonical,
    Duplicate,
    Conflict,
}

/// Exact-bound, linear authority for one terminal ACK. It deliberately has no
/// Clone/Copy/Serialize/Deserialize implementation and no public constructor.
#[allow(dead_code, reason = "consumed by the closed Stage 7B-d-b seam")]
pub(crate) struct Stage7bDurableAckAuthorized {
    operational_identity_sha256: String,
    strategy_request_id: StrategyRequestId,
    durable_client_order_id: ClientOrderId,
    broker_order_id: Option<BrokerOrderId>,
    canonical_command_sha256: String,
    final_disposition: Stage6RequestFinalDispositionV1,
    final_record_id: String,
    final_sequence: u64,
    stage6_checkpoint_sha256: String,
    seal_generation: u64,
    seal_commitment_sha256: String,
    settlement_authority_fingerprint_sha256: String,
    terminal_request_ack_identity_sha256: String,
}

#[allow(dead_code, reason = "consumed by the closed Stage 7B-d-b seam")]
impl Stage7bDurableAckAuthorized {
    pub(crate) fn strategy_request_id(&self) -> StrategyRequestId {
        self.strategy_request_id
    }

    pub(crate) fn durable_client_order_id(&self) -> &ClientOrderId {
        &self.durable_client_order_id
    }

    pub(crate) fn broker_order_id(&self) -> Option<&BrokerOrderId> {
        self.broker_order_id.as_ref()
    }

    pub(crate) fn canonical_command_sha256(&self) -> &str {
        &self.canonical_command_sha256
    }

    pub(crate) fn final_disposition(&self) -> Stage6RequestFinalDispositionV1 {
        self.final_disposition
    }

    pub(crate) fn final_record_id(&self) -> &str {
        &self.final_record_id
    }

    pub(crate) fn final_sequence(&self) -> u64 {
        self.final_sequence
    }

    pub(crate) fn operational_identity_sha256(&self) -> &str {
        &self.operational_identity_sha256
    }

    pub(crate) fn stage6_checkpoint_sha256(&self) -> &str {
        &self.stage6_checkpoint_sha256
    }

    pub(crate) fn seal_generation(&self) -> u64 {
        self.seal_generation
    }

    pub(crate) fn seal_commitment_sha256(&self) -> &str {
        &self.seal_commitment_sha256
    }

    pub(crate) fn settlement_authority_fingerprint_sha256(&self) -> &str {
        &self.settlement_authority_fingerprint_sha256
    }

    pub(crate) fn terminal_request_ack_identity_sha256(&self) -> &str {
        &self.terminal_request_ack_identity_sha256
    }

    /// Pure d-a classifier. Future d-b supplies only publication knowledge;
    /// it cannot change the durable request or canonical ACK binding.
    pub(crate) fn classify_publication(
        &self,
        known_canonical_ack_fingerprint_sha256: Option<&str>,
    ) -> Stage7bAckPublicationDecision {
        match known_canonical_ack_fingerprint_sha256 {
            None => Stage7bAckPublicationDecision::Canonical,
            Some(known) if known == self.terminal_request_ack_identity_sha256 => {
                Stage7bAckPublicationDecision::Duplicate
            }
            Some(_) => Stage7bAckPublicationDecision::Conflict,
        }
    }
}

/// Linear ready owner. Field order is intentional: the Stage 6 runtime (and
/// its file journal) closes before the filesystem writer lease is released.
/// No mutable recovered-runtime extractor is exposed.
pub struct Stage7bRecoveryReadyOwner {
    recovered: Stage6dDurableRuntimeRecovered,
    writer_lease: Stage7bKernelWriterLease,
    committed_seal: Stage7bRecoverySealV1,
    seal_commit_uncertain: bool,
    journal_mutation_uncertain: bool,
    #[cfg(feature = "stage8a4-i3-test-fixtures")]
    stage8a4_test_fail_before_covering_seal: bool,
}

/// Linear owner for one structurally valid but incomplete Stage 8A-4 batch.
///
/// The persisted Stage 7B seal remains the original S0. This owner cannot
/// issue ordinary request/readiness authority and becomes a normal recovery
/// owner only after the exact manifest suffix is complete and final S1 has
/// been committed and reread.
pub struct Stage8a4I3RecoveryPendingOwner {
    recovered: Stage6dDurableRuntimeRecovered,
    writer_lease: Stage7bKernelWriterLease,
    committed_s0: Stage7bRecoverySealV1,
    seal_commit_uncertain: bool,
    journal_mutation_uncertain: bool,
}

/// Linear authority binding one exact Stage 6 durable request to the current
/// authenticated Stage 7B recovery seal. It contains no journal or transport
/// handle and cannot be manufactured without a recovery-ready owner.
pub struct Stage7bStage8a1DurableRequestAuthority {
    stage6: Stage6DurableRequestAuthorityV1,
    operational_identity_sha256: String,
    seal_generation: u64,
    seal_commitment_sha256: String,
}

/// Durable-only I3 receipt. It grants no ACK, readiness, Redis settlement or
/// execution authority.
pub struct Stage7bStage8a4DurableBatchReceipt {
    stage6_checkpoint_sha256: String,
    covering_seal_generation: u64,
    covering_seal_commitment_sha256: String,
    transition_was_existing: bool,
    appended_suffix_records: usize,
}

impl Stage7bStage8a4DurableBatchReceipt {
    pub fn stage6_checkpoint_sha256(&self) -> &str {
        &self.stage6_checkpoint_sha256
    }

    pub fn covering_seal_generation(&self) -> u64 {
        self.covering_seal_generation
    }

    pub fn covering_seal_commitment_sha256(&self) -> &str {
        &self.covering_seal_commitment_sha256
    }

    pub fn transition_was_existing(&self) -> bool {
        self.transition_was_existing
    }

    pub fn appended_suffix_records(&self) -> usize {
        self.appended_suffix_records
    }
}

impl Stage7bStage8a1DurableRequestAuthority {
    pub fn stage6(&self) -> &Stage6DurableRequestAuthorityV1 {
        &self.stage6
    }

    pub fn operational_identity_sha256(&self) -> &str {
        &self.operational_identity_sha256
    }

    pub fn seal_generation(&self) -> u64 {
        self.seal_generation
    }

    pub fn seal_commitment_sha256(&self) -> &str {
        &self.seal_commitment_sha256
    }
}

fn stage8a4_i3_uncovered_checkpoint(
    storage: &Stage7bWritableDurableAuthority,
    committed_seal: &Stage7bRecoverySealV1,
) -> Result<Option<Stage6JournalCheckpointV1>, Stage7bRecoveryError> {
    let records = storage.versioned_records();
    let prefix_len: usize = committed_seal
        .stage6_checkpoint()
        .frontier()
        .frame_count()
        .try_into()
        .map_err(|_| Stage7bRecoveryError::SealInvalid)?;
    if prefix_len >= records.len() {
        return Ok(None);
    }
    let mut prefix = Stage6MemoryJournalBackend::new();
    for record in records.iter().take(prefix_len) {
        prefix
            .append_versioned(record)
            .map_err(Stage6dLiveCoreError::from)?;
    }
    if prefix
        .validate_checkpoint(committed_seal.stage6_checkpoint())
        .is_err()
    {
        return Ok(None);
    }

    let Stage6JournalRecordVersioned::V2(transition) = &records[prefix_len] else {
        return Ok(None);
    };
    if records[prefix_len + 1..]
        .iter()
        .any(|record| matches!(record, Stage6JournalRecordVersioned::V2(_)))
    {
        return Ok(None);
    }
    let mixed = Stage6MixedReplayEngineV2::replay(records).map_err(Stage6dLiveCoreError::from)?;
    let Some(batch) = mixed
        .reconciliation_batches()
        .iter()
        .find(|batch| batch.canonical_v2_record_sha256() == transition.canonical_record_sha256())
    else {
        return Ok(None);
    };
    if batch.verified_suffix_prefix_length() != records.len() - prefix_len - 1
        || Some(batch.last_mixed_record_id()) != storage.frontier().last_record_id()
        || Some(batch.last_mixed_lifecycle_sequence())
            != storage.frontier().last_lifecycle_sequence()
    {
        return Ok(None);
    }

    let precondition = transition.payload().pre_append_precondition();
    let prefix_frontier_fingerprint = stage6_frontier_fingerprint_sha256(prefix.frontier())?;
    if precondition.expected_recovery_seal_generation() != committed_seal.seal_generation()
        || precondition.expected_recovery_seal_fingerprint().as_str()
            != committed_seal.seal_commitment_sha256()
        || (precondition
            .expected_stage6_checkpoint_or_frontier_fingerprint()
            .as_str()
            != committed_seal.stage6_checkpoint().checkpoint_sha256()
            && precondition.expected_stage6_checkpoint_or_frontier_fingerprint()
                != &prefix_frontier_fingerprint)
        || transition.previous_record_id()
            != committed_seal
                .stage6_checkpoint()
                .frontier()
                .last_record_id()
        || transition.lifecycle_sequence().get()
            != committed_seal
                .stage6_checkpoint()
                .frontier()
                .last_lifecycle_sequence()
                .and_then(|sequence| sequence.get().checked_add(1))
                .ok_or(Stage7bRecoveryError::SealInvalid)?
    {
        return Ok(None);
    }
    let prefix_replay = Stage6MixedReplayEngineV2::replay(prefix.versioned_records())
        .map_err(Stage6dLiveCoreError::from)?;
    let Some(request) = prefix_replay.requests().iter().find(|request| {
        request.strategy_request_id() == transition.durable_request_identity().strategy_request_id()
    }) else {
        return Ok(None);
    };
    if request.state_fingerprint_sha256() != *precondition.expected_request_state_fingerprint() {
        return Ok(None);
    }
    Ok(Some(
        Stage6JournalCheckpointV1::from_frontier(storage.frontier().clone())
            .map_err(Stage6dLiveCoreError::from)?,
    ))
}

impl Stage7bRecoveryReadyOwner {
    /// Issues a no-send Stage 8A-1 authority only for the exact dispatch-ready
    /// request covered by a freshly reread authenticated on-disk seal.
    pub fn authorize_stage8a1_durable_request(
        &mut self,
        commitment_key: &Stage5gLifecycleCommitmentKey,
        identity: &Stage6DurableRequestIdentityV1,
        command: &Stage6DurableCommandSnapshotV1,
    ) -> Result<Stage7bStage8a1DurableRequestAuthority, Stage7bRecoveryError> {
        self.require_lifecycle_available()?;
        self.revalidate_cached_committed_seal(commitment_key)?;
        refresh_stage7b_durable_frontier(&mut self.recovered)?;
        let stage6 = self
            .recovered
            .authorize_exact_durable_request(identity, command)?;
        if self.committed_seal.stage6_checkpoint() != self.recovered.authenticated_checkpoint() {
            self.advance_recovery_seal(commitment_key)?;
        }
        // Always cross a final disk/HMAC barrier immediately before issue.
        self.revalidate_cached_committed_seal(commitment_key)?;
        let current_operational_identity = self
            .recovered
            .authenticated_operational_identity()
            .ok_or(Stage7bRecoveryError::SealInvalid)?;
        let current_operational_identity_sha256 =
            stage6d_operational_identity_sha256(current_operational_identity)?;
        if stage6.authenticated_checkpoint_sha256()
            != self.committed_seal.stage6_checkpoint().checkpoint_sha256()
            || current_operational_identity_sha256.as_str()
                != self.committed_seal.operational_identity_sha256()
        {
            return Err(Stage7bRecoveryError::SealInvalid);
        }
        Ok(Stage7bStage8a1DurableRequestAuthority {
            stage6,
            operational_identity_sha256: self
                .committed_seal
                .operational_identity_sha256()
                .to_string(),
            seal_generation: self.committed_seal.seal_generation(),
            seal_commitment_sha256: self.committed_seal.seal_commitment_sha256().to_string(),
        })
    }

    /// Recovery-only equivalent after canonical V2 has advanced the mixed
    /// replay tail. It cannot authorize a new transition: it succeeds only for
    /// the exact persisted pending batch under the current reread S0.
    pub fn authorize_stage8a4_pending_recovery_request(
        &mut self,
        commitment_key: &Stage5gLifecycleCommitmentKey,
        identity: &Stage6DurableRequestIdentityV1,
        command: &Stage6DurableCommandSnapshotV1,
    ) -> Result<Stage7bStage8a1DurableRequestAuthority, Stage7bRecoveryError> {
        self.require_lifecycle_available()?;
        self.revalidate_cached_committed_seal(commitment_key)?;
        refresh_stage7b_durable_frontier(&mut self.recovered)?;
        let stage6 = self
            .recovered
            .authorize_stage8a4_durable_batch_source(identity, command)?;
        if self.committed_seal.stage6_checkpoint() != self.recovered.authenticated_checkpoint() {
            self.advance_recovery_seal(commitment_key)?;
        }
        self.revalidate_cached_committed_seal(commitment_key)?;
        let current_operational_identity = self
            .recovered
            .authenticated_operational_identity()
            .ok_or(Stage7bRecoveryError::SealInvalid)?;
        let current_operational_identity_sha256 =
            stage6d_operational_identity_sha256(current_operational_identity)?;
        if stage6.authenticated_checkpoint_sha256()
            != self.committed_seal.stage6_checkpoint().checkpoint_sha256()
            || current_operational_identity_sha256.as_str()
                != self.committed_seal.operational_identity_sha256()
        {
            return Err(Stage7bRecoveryError::SealInvalid);
        }
        Ok(Stage7bStage8a1DurableRequestAuthority {
            stage6,
            operational_identity_sha256: self
                .committed_seal
                .operational_identity_sha256()
                .to_string(),
            seal_generation: self.committed_seal.seal_generation(),
            seal_commitment_sha256: self.committed_seal.seal_commitment_sha256().to_string(),
        })
    }

    /// Sole I3 writer entry. S0 is reread before mutation, current Stage 6
    /// request authority is reconstructed under the held writer lease, and no
    /// success is returned until covering S1 is committed and reread.
    pub fn append_stage8a4_validated_entry_and_cover(
        &mut self,
        commitment_key: &Stage5gLifecycleCommitmentKey,
        entry: Stage6Stage8a4ValidatedWriteEntry,
    ) -> Result<Stage7bStage8a4DurableBatchReceipt, Stage7bRecoveryError> {
        let expected_operational_identity_sha256 = entry.operational_identity_sha256().to_string();
        let expected_runtime_config_fingerprint_sha256 =
            entry.runtime_config_fingerprint_sha256().to_string();
        let expected_seal_generation = entry.seal_generation();
        let expected_seal_commitment_sha256 = entry.seal_commitment_sha256().to_string();
        let identity = entry.identity().clone();
        let command = entry.command().clone();
        self.require_lifecycle_available()?;
        self.revalidate_cached_committed_seal(commitment_key)?;
        refresh_stage7b_durable_frontier(&mut self.recovered)?;
        if expected_operational_identity_sha256 != self.committed_seal.operational_identity_sha256()
            || expected_seal_generation != self.committed_seal.seal_generation()
            || expected_seal_commitment_sha256 != self.committed_seal.seal_commitment_sha256()
        {
            return Err(Stage7bRecoveryError::SealInvalid);
        }

        let precondition_matches_current_seal = entry.expected_recovery_seal_generation()
            == self.committed_seal.seal_generation()
            && entry.expected_recovery_seal_fingerprint()
                == self.committed_seal.seal_commitment_sha256();
        if !precondition_matches_current_seal && !entry.matches_current_tail(&self.recovered)? {
            return Err(Stage7bRecoveryError::SealInvalid);
        }
        let current_stage6 = self
            .recovered
            .authorize_stage8a4_durable_batch_source(&identity, &command)?;
        if current_stage6.runtime_config_fingerprint_sha256()
            != expected_runtime_config_fingerprint_sha256
        {
            return Err(Stage7bRecoveryError::RuntimeConfigMismatch);
        }
        let appended =
            match apply_stage8a4_validated_writer_entry(&mut self.recovered, commitment_key, entry)
            {
                Ok(receipt) => receipt,
                Err(Stage6dLiveCoreError::JournalMutationMayHaveOccurred) => {
                    self.journal_mutation_uncertain = true;
                    return Err(Stage7bRecoveryError::Runtime(
                        Stage6dLiveCoreError::JournalMutationMayHaveOccurred,
                    ));
                }
                Err(error) => return Err(Stage7bRecoveryError::Runtime(error)),
            };
        #[cfg(feature = "stage8a4-i3-test-fixtures")]
        if self.stage8a4_test_fail_before_covering_seal {
            self.journal_mutation_uncertain = true;
            return Err(Stage7bRecoveryError::Runtime(
                Stage6dLiveCoreError::JournalMutationMayHaveOccurred,
            ));
        }
        if appended.checkpoint() != self.committed_seal.stage6_checkpoint() {
            self.advance_recovery_seal(commitment_key)?;
        } else {
            self.revalidate_cached_committed_seal(commitment_key)?;
        }
        self.revalidate_cached_committed_seal(commitment_key)?;
        let operational_identity = self
            .recovered
            .authenticated_operational_identity()
            .ok_or(Stage7bRecoveryError::SealInvalid)?;
        validate_recovered_binding(&self.recovered, &self.committed_seal, operational_identity)?;
        Ok(Stage7bStage8a4DurableBatchReceipt {
            stage6_checkpoint_sha256: self
                .committed_seal
                .stage6_checkpoint()
                .checkpoint_sha256()
                .to_string(),
            covering_seal_generation: self.committed_seal.seal_generation(),
            covering_seal_commitment_sha256: self
                .committed_seal
                .seal_commitment_sha256()
                .to_string(),
            transition_was_existing: appended.transition_was_existing(),
            appended_suffix_records: appended.appended_suffix_records(),
        })
    }

    /// Returns only canonical persisted recovery material under the current
    /// writer lease and reread S0. The returned value cannot mutate storage.
    pub fn stage8a4_pending_recovery_material(
        &mut self,
        commitment_key: &Stage5gLifecycleCommitmentKey,
    ) -> Result<Option<Stage6Stage8a4PendingRecovery>, Stage7bRecoveryError> {
        self.require_lifecycle_available()?;
        self.revalidate_cached_committed_seal(commitment_key)?;
        refresh_stage7b_durable_frontier(&mut self.recovered)?;
        self.recovered
            .stage8a4_pending_recovery_material()
            .map_err(Stage7bRecoveryError::Runtime)
    }

    #[cfg(test)]
    fn append_stage8a4_test_batch_and_cover(
        &mut self,
        commitment_key: &Stage5gLifecycleCommitmentKey,
        identity: &Stage6DurableRequestIdentityV1,
        command: &Stage6DurableCommandSnapshotV1,
        batch: Stage6Stage8a4DurableBatch,
    ) -> Result<Stage7bStage8a4DurableBatchReceipt, Stage7bRecoveryError> {
        let operational_identity = self
            .committed_seal
            .operational_identity_sha256()
            .to_string();
        let generation = self.committed_seal.seal_generation();
        let commitment = self.committed_seal.seal_commitment_sha256().to_string();
        let runtime_config_fingerprint_sha256 = self
            .recovered
            .authorize_stage8a4_durable_batch_source(identity, command)?
            .runtime_config_fingerprint_sha256()
            .to_string();
        let authority = strategy_runtime_core::stage8a4_test_attest_validated_entry(
            identity.clone(),
            command.clone(),
            batch,
            operational_identity,
            runtime_config_fingerprint_sha256,
            generation,
            commitment,
            Stage6Sha256Digest::parse("33".repeat(32)).unwrap(),
            Stage6Sha256Digest::parse("11".repeat(32)).unwrap(),
            Stage6Sha256Digest::parse("22".repeat(32)).unwrap(),
        )?;
        self.append_stage8a4_validated_entry_and_cover(commitment_key, authority)
    }

    pub fn first_boot(
        root: Stage7bDurableRootAuthority,
        identity: Stage6dOperationalIdentityConfig,
        authorization: Stage6dFirstBootAuthorization,
        stage5g_seed: &[u8],
        commitment_key: &Stage5gLifecycleCommitmentKey,
        fresh_runtime: HybridIntradayRuntimeStrategy,
    ) -> Result<Self, Stage7bRecoveryError> {
        root.validate_bound_identity(&identity)?;
        if root.regular_child_exists(STAGE7B_RECOVERY_SEAL_FILE)? {
            return Err(Stage7bRecoveryError::SealAlreadyExists);
        }
        if stage5g_seed.is_empty() {
            return Err(Stage7bRecoveryError::Runtime(
                Stage6dLiveCoreError::RestartPackageDecode,
            ));
        }
        if !authorization
            .authorizes_runtime_config_fingerprint(&fresh_runtime.stage5c_config_fingerprint())
        {
            return Err(Stage7bRecoveryError::RuntimeConfigMismatch);
        }

        // Authentication and fresh-runtime/config reconstruction happen before
        // journal creation. Invalid seed bytes cannot leave a journal-only
        // first-boot state.
        let validated_stage5g_seed =
            restore_stage5g_clean_restart(stage5g_seed, commitment_key, fresh_runtime).map_err(
                |error| Stage7bRecoveryError::Runtime(Stage6dLiveCoreError::Stage5gRestart(error)),
            )?;
        let empty_journal = Stage6OwnedJournalBackend::memory();
        let checkpoint = Stage6JournalCheckpointV1::from_frontier(empty_journal.frontier().clone())
            .map_err(Stage7bDurableStorageError::from)?;
        let stage6d_package = seal_stage6d_restart_package(
            stage5g_seed,
            checkpoint.clone(),
            identity.clone(),
            commitment_key,
        )?;
        let identity_sha256 = stage6d_operational_identity_sha256(&identity)?;
        let committed_seal = Stage7bRecoverySealV1::new(
            1,
            stage6d_package,
            checkpoint,
            identity_sha256.as_str().to_string(),
            commitment_key,
        )?;

        let storage = Stage7bWritableDurableAuthority::create_new(root, &identity, &authorization)?;
        let (journal, writer_lease) = storage.into_recovery_parts();
        let recovered = first_boot_stage6d_paper_from_validated_stage5g_seed_with_owned_journal(
            authorization,
            validated_stage5g_seed,
            journal,
            identity.clone(),
        )?;
        validate_recovered_binding(&recovered, &committed_seal, &identity)?;
        writer_lease.commit_recovery_seal(&committed_seal)?;
        writer_lease.validate_namespace()?;
        Ok(Self {
            recovered,
            writer_lease,
            committed_seal,
            seal_commit_uncertain: false,
            journal_mutation_uncertain: false,
            #[cfg(feature = "stage8a4-i3-test-fixtures")]
            stage8a4_test_fail_before_covering_seal: false,
        })
    }

    pub fn restart(
        root: Stage7bDurableRootAuthority,
        identity: Stage6dOperationalIdentityConfig,
        commitment_key: &Stage5gLifecycleCommitmentKey,
        fresh_runtime: HybridIntradayRuntimeStrategy,
    ) -> Result<Stage7bRestartOutcome, Stage7bRecoveryError> {
        root.validate_bound_identity(&identity)?;
        let journal_exists = root.regular_child_exists(STAGE7B_JOURNAL_FILE)?;
        let seal_exists = root.regular_child_exists(STAGE7B_RECOVERY_SEAL_FILE)?;
        if seal_exists && !journal_exists {
            return Err(Stage7bRecoveryError::SealWithoutJournal);
        }
        let storage = Stage7bWritableDurableAuthority::open_existing(root, &identity)?;
        let expected_identity = stage6d_operational_identity_sha256(&identity)?;
        let seal_bytes = match storage.read_committed_recovery_seal()? {
            Some(bytes) => bytes,
            None => {
                return Ok(Stage7bRestartOutcome::Blocked(Box::new(
                    Stage7bRecoveryBlocked::retained(
                        Stage7bRecoveryBlockReason::MissingCommittedSeal,
                        storage,
                    ),
                )))
            }
        };
        let committed_seal = match Stage7bRecoverySealV1::decode_canonical(
            &seal_bytes,
            expected_identity.as_str(),
            commitment_key,
        ) {
            Ok(seal) => seal,
            Err(_) => {
                return Ok(Stage7bRestartOutcome::Blocked(Box::new(
                    Stage7bRecoveryBlocked::retained(
                        Stage7bRecoveryBlockReason::CorruptCommittedSeal,
                        storage,
                    ),
                )))
            }
        };
        let mut pending_i3_checkpoint = None;
        let checkpoint_validation = storage.validate_checkpoint(committed_seal.stage6_checkpoint());
        let journal_is_ahead = storage.frontier() != committed_seal.stage6_checkpoint().frontier();
        if checkpoint_validation.is_err() || journal_is_ahead {
            let uncovered_i3 = stage8a4_i3_uncovered_checkpoint(&storage, &committed_seal)?;
            if checkpoint_validation.is_err() && uncovered_i3.is_none() {
                return Ok(Stage7bRestartOutcome::Blocked(Box::new(
                    Stage7bRecoveryBlocked::retained(
                        Stage7bRecoveryBlockReason::CheckpointMismatch,
                        storage,
                    ),
                )));
            }
            if let Some(next_checkpoint) = uncovered_i3 {
                pending_i3_checkpoint = Some(next_checkpoint);
            }
        }

        let restart_package = if let Some(next_checkpoint) = pending_i3_checkpoint.as_ref() {
            // This package is process-local reconstruction material only. It
            // is never committed as the ordinary Stage 7B recovery seal.
            advance_stage6d_restart_package(
                &committed_seal.stage6d_authenticated_restart_package,
                committed_seal.stage6_checkpoint(),
                next_checkpoint.clone(),
                &identity,
                commitment_key,
            )?
        } else {
            committed_seal.stage6d_authenticated_restart_package.clone()
        };

        let (journal, writer_lease) = storage.into_recovery_parts();
        let recovered = match restart_stage6d_paper_with_owned_journal(
            &restart_package,
            commitment_key,
            fresh_runtime,
            journal,
        ) {
            Ok(recovered) => recovered,
            Err(_) => {
                drop(writer_lease);
                return Ok(Stage7bRestartOutcome::Blocked(Box::new(
                    Stage7bRecoveryBlocked::after_consumed_storage(
                        Stage7bRecoveryBlockReason::AuthenticatedRestartRejected,
                    ),
                )));
            }
        };
        let reconstruction_seal = if let Some(next_checkpoint) = pending_i3_checkpoint.as_ref() {
            Stage7bRecoverySealV1::new(
                committed_seal
                    .seal_generation()
                    .checked_add(1)
                    .ok_or(Stage7bRecoveryError::SealGenerationOverflow)?,
                restart_package,
                next_checkpoint.clone(),
                committed_seal.operational_identity_sha256().to_string(),
                commitment_key,
            )?
        } else {
            committed_seal.clone()
        };
        if validate_recovered_binding(&recovered, &reconstruction_seal, &identity).is_err() {
            drop(recovered);
            drop(writer_lease);
            return Ok(Stage7bRestartOutcome::Blocked(Box::new(
                Stage7bRecoveryBlocked::after_consumed_storage(
                    Stage7bRecoveryBlockReason::OperationalIdentityMismatch,
                ),
            )));
        }
        if pending_i3_checkpoint.is_some() {
            recovered.validate_stage8a4_current_tail_authority()?;
            let reread_s0 = writer_lease
                .read_committed_recovery_seal()?
                .ok_or(Stage7bRecoveryError::SealInvalid)?;
            let validated_s0 = Stage7bRecoverySealV1::decode_canonical(
                &reread_s0,
                expected_identity.as_str(),
                commitment_key,
            )?;
            if validated_s0 != committed_seal {
                return Err(Stage7bRecoveryError::SealInvalid);
            }
            writer_lease.validate_namespace()?;
            return Ok(Stage7bRestartOutcome::Stage8a4I3Pending(Box::new(
                Stage8a4I3RecoveryPendingOwner {
                    recovered,
                    writer_lease,
                    committed_s0: committed_seal,
                    seal_commit_uncertain: false,
                    journal_mutation_uncertain: false,
                },
            )));
        }
        writer_lease.validate_namespace()?;
        Ok(Stage7bRestartOutcome::Ready(Box::new(Self {
            recovered,
            writer_lease,
            committed_seal,
            seal_commit_uncertain: false,
            journal_mutation_uncertain: false,
            #[cfg(feature = "stage8a4-i3-test-fixtures")]
            stage8a4_test_fail_before_covering_seal: false,
        })))
    }

    pub fn recovery_ready(&self) -> bool {
        if self.seal_commit_uncertain || self.journal_mutation_uncertain {
            return false;
        }
        let Some(identity) = self.recovered.authenticated_operational_identity() else {
            return false;
        };
        self.writer_lease.validate_namespace().is_ok()
            && validate_recovered_binding(&self.recovered, &self.committed_seal, identity).is_ok()
    }

    /// Revalidates every local durable authority needed by the externally
    /// reported Stage 7B PaperReady state. A failed check is sticky and
    /// prevents a later Redis success from healing storage uncertainty.
    pub(crate) fn validate_composite_readiness(
        &mut self,
        commitment_key: &Stage5gLifecycleCommitmentKey,
    ) -> bool {
        if self.require_lifecycle_available().is_err() {
            return false;
        }
        if self
            .revalidate_cached_committed_seal(commitment_key)
            .is_err()
        {
            return false;
        }
        let Some(identity) = self.recovered.authenticated_operational_identity() else {
            self.seal_commit_uncertain = true;
            return false;
        };
        if validate_recovered_binding(&self.recovered, &self.committed_seal, identity).is_err() {
            self.seal_commit_uncertain = true;
            return false;
        }
        true
    }

    pub fn recovered(&self) -> Result<&Stage6dDurableRuntimeRecovered, Stage7bRecoveryError> {
        self.writer_lease.validate_namespace()?;
        Ok(&self.recovered)
    }

    pub fn committed_seal(&self) -> Result<&Stage7bRecoverySealV1, Stage7bRecoveryError> {
        self.writer_lease.validate_namespace()?;
        Ok(&self.committed_seal)
    }

    /// Delegates command admission to the sole Stage 6 authority while the
    /// owner retains the runtime, journal and writer lease.
    pub fn admit_paper_command(
        &mut self,
        command: &BrokerCommand,
        context: &Stage7aPaperCommandContext,
        observed_at: DateTime<Utc>,
    ) -> Result<Stage7aPaperAdmission, Stage7bRecoveryError> {
        self.require_lifecycle_available()?;
        Ok(admit_stage7a_paper_command(
            &mut self.recovered,
            command,
            context,
            observed_at,
        )?)
    }

    /// Records normalized paper outcome facts only after the caller possesses
    /// the linear fsync-backed dispatch receipt.
    pub fn record_paper_outcome(
        &mut self,
        receipt: Stage6dPaperDispatchReceipt,
        outcome: Stage6dPaperOutcome,
    ) -> Result<Stage6dPaperExecutionReport, Stage7bRecoveryError> {
        self.require_lifecycle_available()?;
        Ok(execute_stage6d_paper_outcome(
            &mut self.recovered,
            receipt,
            outcome,
        )?)
    }

    #[allow(dead_code, reason = "consumed by the closed Stage 7B-d-b seam")]
    pub(crate) fn finalize_paper_request(
        &mut self,
        report: Stage6dPaperExecutionReport,
        observed_at: DateTime<Utc>,
    ) -> Result<(Stage6dPaperExecutionReport, Stage7bFinalizedPaperRequest), Stage7bRecoveryError>
    {
        self.require_lifecycle_available()?;
        let report = finalize_stage7a_paper_request(&mut self.recovered, report, observed_at)?;
        let finalized = self.finalized_request(report.strategy_request_id)?;
        Ok((report, finalized))
    }

    #[allow(dead_code, reason = "consumed by the closed Stage 7B-d-b seam")]
    pub(crate) fn finalize_replayed_paper_request(
        &mut self,
        request_id: StrategyRequestId,
        observed_at: DateTime<Utc>,
    ) -> Result<Stage7bFinalizedPaperRequest, Stage7bRecoveryError> {
        self.require_lifecycle_available()?;
        finalize_stage7a_replayed_paper_request(&mut self.recovered, request_id, observed_at)?;
        self.finalized_request(request_id)
    }

    /// Reconstructs finalized authority after restart without consulting any
    /// process-memory ACK publication map.
    #[allow(dead_code, reason = "consumed by the closed Stage 7B-d-b seam")]
    pub(crate) fn finalized_request(
        &self,
        request_id: StrategyRequestId,
    ) -> Result<Stage7bFinalizedPaperRequest, Stage7bRecoveryError> {
        self.writer_lease.validate_namespace()?;
        Ok(Stage7bFinalizedPaperRequest {
            facts: stage7b_finalized_request_facts(&self.recovered, request_id)?,
        })
    }

    pub fn resolve_cancel_command_context(
        &self,
        command: &broker_core::CancelOrder,
        expected_instrument: &broker_core::InstrumentId,
        expected_strategy_id: &str,
    ) -> Result<Option<Stage7aPaperCommandContext>, Stage7bRecoveryError> {
        self.writer_lease.validate_namespace()?;
        Ok(
            strategy_runtime_core::resolve_stage7a_cancel_command_context(
                &self.recovered,
                command,
                expected_instrument,
                expected_strategy_id,
            ),
        )
    }

    /// Consumes exact finalized facts, advances/revalidates the authenticated
    /// seal, and only then mints terminal ACK authority.
    #[allow(dead_code, reason = "consumed by the closed Stage 7B-d-b seam")]
    pub(crate) fn authorize_finalized_ack(
        &mut self,
        finalized: Stage7bFinalizedPaperRequest,
        commitment_key: &Stage5gLifecycleCommitmentKey,
    ) -> Result<Stage7bDurableAckAuthorized, Stage7bRecoveryError> {
        self.require_lifecycle_available()?;
        let current = stage7b_finalized_request_facts(
            &self.recovered,
            finalized.facts.strategy_request_id(),
        )?;
        if !same_finalized_facts(&current, &finalized.facts) {
            return Err(Stage7bRecoveryError::FinalizedBindingMismatch);
        }
        // Never overwrite or trust through an unexpected on-disk authority.
        // This first exact reread authenticates the cached predecessor before
        // replay may advance the current Stage 6 frontier.
        self.revalidate_cached_committed_seal(commitment_key)?;
        refresh_stage7b_durable_frontier(&mut self.recovered)
            .map_err(Stage7bRecoveryError::from)?;
        if self.committed_seal.stage6_checkpoint() != self.recovered.authenticated_checkpoint() {
            self.advance_recovery_seal(commitment_key)?;
        } else {
            // The already-current path has no seal write whose post-fsync
            // reread could serve as the final barrier. Reread the committed
            // file again immediately before minting ACK authority.
            self.revalidate_cached_committed_seal(commitment_key)?;
            let identity = self
                .recovered
                .authenticated_operational_identity()
                .ok_or(Stage7bRecoveryError::SealInvalid)?;
            validate_recovered_binding(&self.recovered, &self.committed_seal, identity)?;
        }
        durable_ack_authority(&self.committed_seal, current)
    }

    /// Owner-mediated d-b transition. The linear d-a authority is consumed
    /// directly into one exact Redis plan and is never exported or serialized.
    #[allow(dead_code, reason = "Stage 7B-d-b is attached only by closed d-c")]
    pub(crate) async fn settle_finalized_ack(
        &mut self,
        finalized: Stage7bFinalizedPaperRequest,
        commitment_key: &Stage5gLifecycleCommitmentKey,
        context: Stage7bRedisSettlementContext,
        backend: &mut Stage7bRedisSettlementBackend,
    ) -> Result<Stage7bRedisSettlementOutcome, Stage7bDbError> {
        let authority = self.authorize_finalized_ack(finalized, commitment_key)?;
        let plan = redis_settlement::ack_plan(authority, context)?;
        Ok(backend.settle_ack(plan).await?)
    }

    /// Captures the Stage 6 frontier before profile/policy classification.
    /// The observation records whether the request identity already exists so
    /// an established identity conflict can never be converted into an ACK.
    pub(crate) fn observe_pre_stage6_command(
        &mut self,
        commitment_key: &Stage5gLifecycleCommitmentKey,
        request_id: StrategyRequestId,
    ) -> Result<Stage7bPreStage6CommandObservation, Stage7bDbError> {
        self.require_lifecycle_available()?;
        self.revalidate_cached_committed_seal(commitment_key)?;
        refresh_stage7b_durable_frontier(&mut self.recovered)
            .map_err(Stage7bRecoveryError::from)?;
        if self.committed_seal.stage6_checkpoint() != self.recovered.authenticated_checkpoint() {
            self.advance_recovery_seal(commitment_key)?;
        } else {
            self.revalidate_cached_committed_seal(commitment_key)?;
        }
        Ok(redis_settlement::pre_stage6_command_observation(
            request_id,
            sha256_hex(&self.recovered.authenticated_checkpoint().encode_canonical()),
            self.recovered.replay().request(request_id).is_some(),
        ))
    }

    /// Settles one accepted Stage 7A deterministic rejection using the same
    /// atomic ACK+XACK Lua primitive as finalized Stage 6 requests. Authority
    /// is minted only when the Stage 6 checkpoint and request index are still
    /// exactly equal to the pre-admission observation.
    pub(crate) async fn settle_pre_stage6_rejection(
        &mut self,
        commitment_key: &Stage5gLifecycleCommitmentKey,
        observation: Stage7bPreStage6CommandObservation,
        evidence: Stage7aDeterministicRejectionEvidence,
        context: Stage7bRedisSettlementContext,
        backend: &mut Stage7bRedisSettlementBackend,
    ) -> Result<Stage7bRedisSettlementOutcome, Stage7bDbError> {
        self.require_lifecycle_available()?;
        self.revalidate_cached_committed_seal(commitment_key)?;
        refresh_stage7b_durable_frontier(&mut self.recovered)
            .map_err(Stage7bRecoveryError::from)?;
        if self.committed_seal.stage6_checkpoint() != self.recovered.authenticated_checkpoint() {
            return Err(Stage7bRecoveryError::SealInvalid.into());
        }
        self.revalidate_cached_committed_seal(commitment_key)?;
        let checkpoint = sha256_hex(&self.recovered.authenticated_checkpoint().encode_canonical());
        let request_id = evidence.strategy_request_id();
        let authority = redis_settlement::authorize_pre_stage6_rejection(
            observation,
            evidence,
            &checkpoint,
            self.recovered.replay().request(request_id).is_some(),
            self.committed_seal.operational_identity_sha256(),
            self.committed_seal.seal_generation(),
            self.committed_seal.seal_commitment_sha256(),
        )?;
        let plan = redis_settlement::pre_stage6_rejection_ack_plan(authority, context)?;
        Ok(backend.settle_ack(plan).await?)
    }

    /// Reproduces terminal Redis publication history without creating a Stage
    /// 6 request.  The current seal, checkpoint and request absence must still
    /// match the pre-admission observation immediately before atomic XADD/XACK.
    pub(crate) async fn settle_canonical_marker_duplicate(
        &mut self,
        commitment_key: &Stage5gLifecycleCommitmentKey,
        observation: Stage7bPreStage6CommandObservation,
        identity: Stage7aCanonicalCommandIdentity,
        evidence: Stage7bCanonicalRequestPublicationEvidence,
        context: Stage7bRedisSettlementContext,
        backend: &mut Stage7bRedisSettlementBackend,
    ) -> Result<Stage7bRedisSettlementOutcome, Stage7bDbError> {
        self.require_lifecycle_available()?;
        self.revalidate_cached_committed_seal(commitment_key)?;
        refresh_stage7b_durable_frontier(&mut self.recovered)
            .map_err(Stage7bRecoveryError::from)?;
        if self.committed_seal.stage6_checkpoint() != self.recovered.authenticated_checkpoint() {
            return Err(Stage7bRecoveryError::SealInvalid.into());
        }
        self.revalidate_cached_committed_seal(commitment_key)?;
        let checkpoint = sha256_hex(&self.recovered.authenticated_checkpoint().encode_canonical());
        let request_id = identity.strategy_request_id();
        let authority = redis_settlement::authorize_canonical_marker_duplicate(
            observation,
            &identity,
            evidence,
            &checkpoint,
            self.recovered.replay().request(request_id).is_some(),
        )?;
        let plan = redis_settlement::canonical_marker_duplicate_ack_plan(authority, context)?;
        Ok(backend.settle_ack(plan).await?)
    }

    /// Captures the exact checkpoint before malformed input is classified.
    /// No Stage 6 admission API is available through this observation.
    #[allow(dead_code, reason = "Stage 7B-d-b is attached only by closed d-c")]
    pub(crate) fn observe_pre_stage6_poison(
        &mut self,
        commitment_key: &Stage5gLifecycleCommitmentKey,
        context: Stage7bRedisSettlementContext,
        evidence: Stage7aPermanentPoisonEvidence,
    ) -> Result<Stage7bPreStage6PoisonObservation, Stage7bDbError> {
        self.require_lifecycle_available()?;
        self.revalidate_cached_committed_seal(commitment_key)?;
        refresh_stage7b_durable_frontier(&mut self.recovered)
            .map_err(Stage7bRecoveryError::from)?;
        if self.committed_seal.stage6_checkpoint() != self.recovered.authenticated_checkpoint() {
            return Err(Stage7bRecoveryError::SealInvalid.into());
        }
        self.revalidate_cached_committed_seal(commitment_key)?;
        Ok(redis_settlement::poison_observation(
            context,
            evidence,
            sha256_hex(&self.recovered.authenticated_checkpoint().encode_canonical()),
        )?)
    }

    /// Completes a permanent pre-Stage6 poison path only if the journal
    /// checkpoint and redacted payload fingerprint are unchanged.
    #[allow(dead_code, reason = "Stage 7B-d-b is attached only by closed d-c")]
    pub(crate) async fn settle_pre_stage6_poison(
        &mut self,
        commitment_key: &Stage5gLifecycleCommitmentKey,
        observation: Stage7bPreStage6PoisonObservation,
        backend: &mut Stage7bRedisSettlementBackend,
    ) -> Result<Stage7bRedisSettlementOutcome, Stage7bDbError> {
        self.require_lifecycle_available()?;
        self.revalidate_cached_committed_seal(commitment_key)?;
        refresh_stage7b_durable_frontier(&mut self.recovered)
            .map_err(Stage7bRecoveryError::from)?;
        if self.committed_seal.stage6_checkpoint() != self.recovered.authenticated_checkpoint() {
            return Err(Stage7bRecoveryError::SealInvalid.into());
        }
        self.revalidate_cached_committed_seal(commitment_key)?;
        let checkpoint = sha256_hex(&self.recovered.authenticated_checkpoint().encode_canonical());
        let authority = redis_settlement::authorize_poison(observation, &checkpoint)?;
        let plan = redis_settlement::dlq_plan(authority)?;
        Ok(backend.settle_dlq(plan).await?)
    }

    fn require_lifecycle_available(&self) -> Result<(), Stage7bRecoveryError> {
        self.writer_lease.validate_namespace()?;
        if self.journal_mutation_uncertain {
            return Err(Stage7bRecoveryError::Runtime(
                Stage6dLiveCoreError::JournalMutationMayHaveOccurred,
            ));
        }
        if self.seal_commit_uncertain {
            return Err(Stage7bRecoveryError::SealCommitUncertain);
        }
        Ok(())
    }

    #[allow(dead_code, reason = "consumed by the closed Stage 7B-d-b seam")]
    fn revalidate_cached_committed_seal(
        &mut self,
        commitment_key: &Stage5gLifecycleCommitmentKey,
    ) -> Result<(), Stage7bRecoveryError> {
        let result = (|| {
            self.writer_lease.validate_namespace()?;
            let identity = self
                .recovered
                .authenticated_operational_identity()
                .ok_or(Stage7bRecoveryError::SealInvalid)?;
            let expected_identity = stage6d_operational_identity_sha256(identity)?;
            let bytes = self
                .writer_lease
                .read_committed_recovery_seal()?
                .ok_or(Stage7bRecoveryError::SealInvalid)?;
            let on_disk = Stage7bRecoverySealV1::decode_canonical(
                &bytes,
                expected_identity.as_str(),
                commitment_key,
            )?;
            if on_disk != self.committed_seal {
                return Err(Stage7bRecoveryError::SealInvalid);
            }
            Ok(())
        })();
        if result.is_err() {
            self.seal_commit_uncertain = true;
        }
        result
    }

    #[allow(dead_code, reason = "consumed by the closed Stage 7B-d-b seam")]
    fn advance_recovery_seal(
        &mut self,
        commitment_key: &Stage5gLifecycleCommitmentKey,
    ) -> Result<(), Stage7bRecoveryError> {
        self.require_lifecycle_available()?;
        let identity = self
            .recovered
            .authenticated_operational_identity()
            .cloned()
            .ok_or(Stage7bRecoveryError::SealInvalid)?;
        let next_checkpoint = self.recovered.authenticated_checkpoint().clone();
        let next_package = advance_stage6d_restart_package(
            &self.committed_seal.stage6d_authenticated_restart_package,
            self.committed_seal.stage6_checkpoint(),
            next_checkpoint.clone(),
            &identity,
            commitment_key,
        )?;
        let next_generation = self
            .committed_seal
            .seal_generation()
            .checked_add(1)
            .ok_or(Stage7bRecoveryError::SealGenerationOverflow)?;
        let next = Stage7bRecoverySealV1::new(
            next_generation,
            next_package,
            next_checkpoint,
            self.committed_seal.operational_identity_sha256.clone(),
            commitment_key,
        )?;
        if let Err(error) = self.writer_lease.commit_recovery_seal(&next) {
            self.seal_commit_uncertain = true;
            return Err(error);
        }
        let committed = match self.writer_lease.read_committed_recovery_seal() {
            Ok(Some(bytes)) => Stage7bRecoverySealV1::decode_canonical(
                &bytes,
                next.operational_identity_sha256(),
                commitment_key,
            ),
            Ok(None) => Err(Stage7bRecoveryError::SealInvalid),
            Err(error) => Err(error),
        };
        let committed = match committed {
            Ok(value) => value,
            Err(error) => {
                self.seal_commit_uncertain = true;
                return Err(error);
            }
        };
        if let Err(error) = validate_recovered_binding(&self.recovered, &committed, &identity) {
            self.seal_commit_uncertain = true;
            return Err(error);
        }
        self.committed_seal = committed;
        Ok(())
    }

    pub fn redis_consumer_attached(&self) -> bool {
        false
    }

    pub fn finam_transport_attached(&self) -> bool {
        false
    }

    pub fn runtime_live_enabled(&self) -> bool {
        false
    }

    pub fn real_orders_enabled(&self) -> bool {
        false
    }
}

impl Stage8a4I3RecoveryPendingOwner {
    pub fn recovery_ready(&self) -> bool {
        false
    }

    fn require_pending_available(&self) -> Result<(), Stage7bRecoveryError> {
        self.writer_lease.validate_namespace()?;
        if self.journal_mutation_uncertain {
            return Err(Stage7bRecoveryError::Runtime(
                Stage6dLiveCoreError::JournalMutationMayHaveOccurred,
            ));
        }
        if self.seal_commit_uncertain {
            return Err(Stage7bRecoveryError::SealCommitUncertain);
        }
        Ok(())
    }

    fn revalidate_committed_s0(
        &mut self,
        commitment_key: &Stage5gLifecycleCommitmentKey,
    ) -> Result<(), Stage7bRecoveryError> {
        let result = (|| {
            self.require_pending_available()?;
            let identity = self
                .recovered
                .authenticated_operational_identity()
                .ok_or(Stage7bRecoveryError::SealInvalid)?;
            let expected_identity = stage6d_operational_identity_sha256(identity)?;
            let bytes = self
                .writer_lease
                .read_committed_recovery_seal()?
                .ok_or(Stage7bRecoveryError::SealInvalid)?;
            let on_disk = Stage7bRecoverySealV1::decode_canonical(
                &bytes,
                expected_identity.as_str(),
                commitment_key,
            )?;
            if on_disk != self.committed_s0 {
                return Err(Stage7bRecoveryError::SealInvalid);
            }
            Ok(())
        })();
        if result.is_err() {
            self.seal_commit_uncertain = true;
        }
        result
    }

    /// Canonical persisted V2 recovery material. It is read-only and cannot
    /// issue ordinary Stage8A1 or readiness authority.
    pub fn pending_recovery_material(
        &mut self,
        commitment_key: &Stage5gLifecycleCommitmentKey,
    ) -> Result<Stage6Stage8a4PendingRecovery, Stage7bRecoveryError> {
        self.revalidate_committed_s0(commitment_key)?;
        refresh_stage7b_durable_frontier(&mut self.recovered)?;
        self.recovered
            .stage8a4_pending_recovery_material()?
            .ok_or(Stage7bRecoveryError::SealInvalid)
    }

    /// Recovery-only current request authority. It is bound to the original
    /// S0 and cannot authorize a new transition or ordinary command path.
    pub fn authorize_pending_recovery_request(
        &mut self,
        commitment_key: &Stage5gLifecycleCommitmentKey,
        identity: &Stage6DurableRequestIdentityV1,
        command: &Stage6DurableCommandSnapshotV1,
    ) -> Result<Stage7bStage8a1DurableRequestAuthority, Stage7bRecoveryError> {
        self.revalidate_committed_s0(commitment_key)?;
        refresh_stage7b_durable_frontier(&mut self.recovered)?;
        let stage6 = self
            .recovered
            .authorize_stage8a4_durable_batch_source(identity, command)?;
        let current_identity = self
            .recovered
            .authenticated_operational_identity()
            .ok_or(Stage7bRecoveryError::SealInvalid)?;
        let current_identity_sha256 = stage6d_operational_identity_sha256(current_identity)?;
        if current_identity_sha256.as_str() != self.committed_s0.operational_identity_sha256() {
            return Err(Stage7bRecoveryError::SealInvalid);
        }
        Ok(Stage7bStage8a1DurableRequestAuthority {
            stage6,
            operational_identity_sha256: self
                .committed_s0
                .operational_identity_sha256()
                .to_string(),
            seal_generation: self.committed_s0.seal_generation(),
            seal_commitment_sha256: self.committed_s0.seal_commitment_sha256().to_string(),
        })
    }

    /// Completes the exact missing suffix and only then commits/rereads final
    /// S1. Success consumes the pending owner into ordinary RecoveryReady.
    pub fn append_recovery_entry_and_cover(
        mut self,
        commitment_key: &Stage5gLifecycleCommitmentKey,
        entry: Stage6Stage8a4ValidatedWriteEntry,
    ) -> Result<
        (
            Stage7bStage8a4DurableBatchReceipt,
            Stage7bRecoveryReadyOwner,
        ),
        Stage7bRecoveryError,
    > {
        let identity = entry.identity().clone();
        let command = entry.command().clone();
        let expected_runtime = entry.runtime_config_fingerprint_sha256().to_string();
        self.revalidate_committed_s0(commitment_key)?;
        refresh_stage7b_durable_frontier(&mut self.recovered)?;
        if entry.operational_identity_sha256() != self.committed_s0.operational_identity_sha256()
            || entry.seal_generation() != self.committed_s0.seal_generation()
            || entry.seal_commitment_sha256() != self.committed_s0.seal_commitment_sha256()
            || !entry.matches_current_tail(&self.recovered)?
        {
            return Err(Stage7bRecoveryError::SealInvalid);
        }
        let current = self
            .recovered
            .authorize_stage8a4_durable_batch_source(&identity, &command)?;
        if current.runtime_config_fingerprint_sha256() != expected_runtime {
            return Err(Stage7bRecoveryError::RuntimeConfigMismatch);
        }
        let appended =
            match apply_stage8a4_validated_writer_entry(&mut self.recovered, commitment_key, entry)
            {
                Ok(receipt) => receipt,
                Err(Stage6dLiveCoreError::JournalMutationMayHaveOccurred) => {
                    self.journal_mutation_uncertain = true;
                    return Err(Stage7bRecoveryError::Runtime(
                        Stage6dLiveCoreError::JournalMutationMayHaveOccurred,
                    ));
                }
                Err(error) => return Err(Stage7bRecoveryError::Runtime(error)),
            };
        let mut ready = Stage7bRecoveryReadyOwner {
            recovered: self.recovered,
            writer_lease: self.writer_lease,
            committed_seal: self.committed_s0,
            seal_commit_uncertain: self.seal_commit_uncertain,
            journal_mutation_uncertain: self.journal_mutation_uncertain,
            #[cfg(feature = "stage8a4-i3-test-fixtures")]
            stage8a4_test_fail_before_covering_seal: false,
        };
        ready.advance_recovery_seal(commitment_key)?;
        ready.revalidate_cached_committed_seal(commitment_key)?;
        let operational_identity = ready
            .recovered
            .authenticated_operational_identity()
            .ok_or(Stage7bRecoveryError::SealInvalid)?;
        validate_recovered_binding(
            &ready.recovered,
            &ready.committed_seal,
            operational_identity,
        )?;
        let receipt = Stage7bStage8a4DurableBatchReceipt {
            stage6_checkpoint_sha256: ready
                .committed_seal
                .stage6_checkpoint()
                .checkpoint_sha256()
                .to_string(),
            covering_seal_generation: ready.committed_seal.seal_generation(),
            covering_seal_commitment_sha256: ready
                .committed_seal
                .seal_commitment_sha256()
                .to_string(),
            transition_was_existing: appended.transition_was_existing(),
            appended_suffix_records: appended.appended_suffix_records(),
        };
        Ok((receipt, ready))
    }
}

pub enum Stage7bRestartOutcome {
    Ready(Box<Stage7bRecoveryReadyOwner>),
    Stage8a4I3Pending(Box<Stage8a4I3RecoveryPendingOwner>),
    Blocked(Box<Stage7bRecoveryBlocked>),
}

impl Stage7bRestartOutcome {
    pub fn recovery_ready(&self) -> bool {
        match self {
            Self::Ready(owner) => owner.recovery_ready(),
            Self::Stage8a4I3Pending(_) => false,
            Self::Blocked(blocked) => blocked.recovery_ready(),
        }
    }
}

impl Stage7bDurableRootAuthority {
    fn regular_child_exists(&self, name: &str) -> Result<bool, Stage7bRecoveryError> {
        self.validate_external_root_identity()?;
        match open_child_at(
            &self.root_directory,
            name,
            libc::O_RDONLY | libc::O_NONBLOCK | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            0,
        ) {
            Ok(file) if is_single_linked_regular(&file)? => Ok(true),
            Ok(_) => Err(Stage7bRecoveryError::SealInvalid),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(false),
            Err(error) => Err(Stage7bRecoveryError::SealWriteFailed(error.kind())),
        }
    }
}

impl Stage7bWritableDurableAuthority {
    fn into_recovery_parts(self) -> (Stage6OwnedJournalBackend, Stage7bKernelWriterLease) {
        let Self {
            journal,
            _writer_lease,
            operational_identity_sha256: _,
        } = self;
        (Stage6OwnedJournalBackend::from_file(journal), _writer_lease)
    }

    fn read_committed_recovery_seal(&self) -> Result<Option<Vec<u8>>, Stage7bRecoveryError> {
        self._writer_lease.read_committed_recovery_seal()
    }
}

impl Stage7bKernelWriterLease {
    fn read_committed_recovery_seal(&self) -> Result<Option<Vec<u8>>, Stage7bRecoveryError> {
        self.validate_namespace()?;
        let mut file = match open_child_at(
            &self.root.root_directory,
            STAGE7B_RECOVERY_SEAL_FILE,
            libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            0,
        ) {
            Ok(file) => file,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(Stage7bRecoveryError::SealWriteFailed(error.kind())),
        };
        if !is_single_linked_regular(&file)?
            || file
                .metadata()
                .map_err(|error| Stage7bRecoveryError::SealWriteFailed(error.kind()))?
                .len()
                > STAGE7B_RECOVERY_SEAL_MAX_BYTES
        {
            return Err(Stage7bRecoveryError::SealInvalid);
        }
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)
            .map_err(|error| Stage7bRecoveryError::SealWriteFailed(error.kind()))?;
        self.validate_namespace()?;
        Ok(Some(bytes))
    }

    fn commit_recovery_seal(
        &self,
        seal: &Stage7bRecoverySealV1,
    ) -> Result<(), Stage7bRecoveryError> {
        self.commit_recovery_seal_with_pre_rename_observer(seal, || {})
    }

    fn commit_recovery_seal_with_pre_rename_observer<F>(
        &self,
        seal: &Stage7bRecoverySealV1,
        mut after_temp_sync: F,
    ) -> Result<(), Stage7bRecoveryError>
    where
        F: FnMut(),
    {
        self.validate_namespace()?;
        let bytes = seal.encode_canonical()?;
        if bytes.len() as u64 > STAGE7B_RECOVERY_SEAL_MAX_BYTES {
            return Err(Stage7bRecoveryError::SealInvalid);
        }
        let temp_name = format!(
            "{STAGE7B_RECOVERY_SEAL_TEMP_PREFIX}{}.{}.tmp",
            std::process::id(),
            STAGE7B_RECOVERY_SEAL_TEMP_COUNTER.fetch_add(1, Ordering::SeqCst)
        );
        let mut temp = open_child_at(
            &self.root.root_directory,
            &temp_name,
            libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            0o600,
        )
        .map_err(|error| Stage7bRecoveryError::SealWriteFailed(error.kind()))?;
        let result = (|| {
            if !is_single_linked_regular(&temp)? {
                return Err(Stage7bRecoveryError::SealInvalid);
            }
            temp.write_all(&bytes)
                .map_err(|error| Stage7bRecoveryError::SealWriteFailed(error.kind()))?;
            temp.sync_all()
                .map_err(|error| Stage7bRecoveryError::SealWriteFailed(error.kind()))?;
            after_temp_sync();
            self.validate_namespace()?;
            rename_child_at(
                &self.root.root_directory,
                &temp_name,
                STAGE7B_RECOVERY_SEAL_FILE,
            )?;
            self.root
                .root_directory
                .sync_all()
                .map_err(|error| Stage7bRecoveryError::SealWriteFailed(error.kind()))?;
            self.validate_namespace()?;
            let committed = self
                .read_committed_recovery_seal()?
                .ok_or(Stage7bRecoveryError::SealInvalid)?;
            if committed != bytes {
                return Err(Stage7bRecoveryError::SealInvalid);
            }
            Ok(())
        })();
        if result.is_err() {
            unlink_child_at(&self.root.root_directory, &temp_name);
        }
        result
    }
}

fn validate_recovered_binding(
    recovered: &Stage6dDurableRuntimeRecovered,
    seal: &Stage7bRecoverySealV1,
    identity: &Stage6dOperationalIdentityConfig,
) -> Result<(), Stage7bRecoveryError> {
    let expected = stage6d_operational_identity_sha256(identity)?;
    if seal.operational_identity_sha256() != expected.as_str()
        || recovered.authenticated_operational_identity() != Some(identity)
        || recovered.authenticated_checkpoint() != seal.stage6_checkpoint()
        || !recovered.journal_is_file_backed()
    {
        return Err(Stage7bRecoveryError::SealInvalid);
    }
    Ok(())
}

#[allow(dead_code, reason = "consumed by the closed Stage 7B-d-b seam")]
fn same_finalized_facts(
    left: &Stage7bFinalizedRequestFacts,
    right: &Stage7bFinalizedRequestFacts,
) -> bool {
    left.strategy_request_id() == right.strategy_request_id()
        && left.durable_client_order_id() == right.durable_client_order_id()
        && left.broker_order_id() == right.broker_order_id()
        && left.canonical_command_sha256() == right.canonical_command_sha256()
        && left.final_disposition() == right.final_disposition()
        && left.final_record_id() == right.final_record_id()
        && left.final_sequence() == right.final_sequence()
}

#[allow(dead_code, reason = "consumed by the closed Stage 7B-d-b seam")]
fn durable_ack_authority(
    seal: &Stage7bRecoverySealV1,
    facts: Stage7bFinalizedRequestFacts,
) -> Result<Stage7bDurableAckAuthorized, Stage7bRecoveryError> {
    #[derive(Serialize)]
    struct SettlementAuthorityFingerprint<'a> {
        schema_version: u16,
        domain: &'static str,
        operational_identity_sha256: &'a str,
        strategy_request_id: StrategyRequestId,
        durable_client_order_id: &'a ClientOrderId,
        broker_order_id: Option<&'a BrokerOrderId>,
        canonical_command_sha256: &'a str,
        final_disposition: Stage6RequestFinalDispositionV1,
        final_record_id: &'a str,
        final_sequence: u64,
        stage6_checkpoint_sha256: &'a str,
        seal_generation: u64,
        seal_commitment_sha256: &'a str,
        settlement_kind: &'static str,
    }

    #[derive(Serialize)]
    struct TerminalRequestAckIdentity<'a> {
        schema_version: u16,
        domain: &'static str,
        operational_identity_sha256: &'a str,
        strategy_request_id: StrategyRequestId,
        durable_client_order_id: &'a ClientOrderId,
        broker_order_id: Option<&'a BrokerOrderId>,
        canonical_command_sha256: &'a str,
        final_disposition: Stage6RequestFinalDispositionV1,
        final_record_id: &'a str,
        final_sequence: u64,
        terminal_ack_schema: u16,
    }

    let stage6_checkpoint_sha256 = sha256_hex(&seal.stage6_checkpoint().encode_canonical());
    let settlement_authority_fingerprint_sha256 = sha256_hex(
        &serde_json::to_vec(&SettlementAuthorityFingerprint {
            schema_version: 1,
            domain: "moex.stage7b.durable-ack-authority.v1",
            operational_identity_sha256: seal.operational_identity_sha256(),
            strategy_request_id: facts.strategy_request_id(),
            durable_client_order_id: facts.durable_client_order_id(),
            broker_order_id: facts.broker_order_id(),
            canonical_command_sha256: facts.canonical_command_sha256().as_str(),
            final_disposition: facts.final_disposition(),
            final_record_id: facts.final_record_id().as_str(),
            final_sequence: facts.final_sequence(),
            stage6_checkpoint_sha256: &stage6_checkpoint_sha256,
            seal_generation: seal.seal_generation(),
            seal_commitment_sha256: seal.seal_commitment_sha256(),
            settlement_kind: "ack",
        })
        .map_err(|_| Stage7bRecoveryError::SealInvalid)?,
    );
    let terminal_request_ack_identity_sha256 = sha256_hex(
        &serde_json::to_vec(&TerminalRequestAckIdentity {
            schema_version: 1,
            domain: "moex.stage7b.terminal-request-ack-identity.v1",
            operational_identity_sha256: seal.operational_identity_sha256(),
            strategy_request_id: facts.strategy_request_id(),
            durable_client_order_id: facts.durable_client_order_id(),
            broker_order_id: facts.broker_order_id(),
            canonical_command_sha256: facts.canonical_command_sha256().as_str(),
            final_disposition: facts.final_disposition(),
            final_record_id: facts.final_record_id().as_str(),
            final_sequence: facts.final_sequence(),
            terminal_ack_schema: 1,
        })
        .map_err(|_| Stage7bRecoveryError::SealInvalid)?,
    );
    Ok(Stage7bDurableAckAuthorized {
        operational_identity_sha256: seal.operational_identity_sha256().to_string(),
        strategy_request_id: facts.strategy_request_id(),
        durable_client_order_id: facts.durable_client_order_id().clone(),
        broker_order_id: facts.broker_order_id().cloned(),
        canonical_command_sha256: facts.canonical_command_sha256().as_str().to_string(),
        final_disposition: facts.final_disposition(),
        final_record_id: facts.final_record_id().as_str().to_string(),
        final_sequence: facts.final_sequence(),
        stage6_checkpoint_sha256,
        seal_generation: seal.seal_generation(),
        seal_commitment_sha256: seal.seal_commitment_sha256().to_string(),
        settlement_authority_fingerprint_sha256,
        terminal_request_ack_identity_sha256,
    })
}

fn seal_commitment_sha256(
    seal_generation: u64,
    created_at_ts_utc_ms: i64,
    stage6d_restart_package_sha256: &str,
    stage6_checkpoint_bytes_sha256: &str,
    operational_identity_sha256: &str,
) -> Result<String, Stage7bRecoveryError> {
    let bytes = serde_json::to_vec(&Stage7bRecoverySealCommitmentV1 {
        schema_version: STAGE7B_RECOVERY_SEAL_SCHEMA_VERSION,
        domain: STAGE7B_RECOVERY_SEAL_COMMITMENT_DOMAIN,
        seal_generation,
        created_at_ts_utc_ms,
        stage6d_restart_package_sha256,
        stage6_checkpoint_bytes_sha256,
        operational_identity_sha256,
    })
    .map_err(|_| Stage7bRecoveryError::SealInvalid)?;
    Ok(sha256_hex(&bytes))
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn rename_child_at(root: &File, from: &str, to: &str) -> Result<(), Stage7bRecoveryError> {
    let from = CString::new(from).map_err(|_| Stage7bRecoveryError::SealInvalid)?;
    let to = CString::new(to).map_err(|_| Stage7bRecoveryError::SealInvalid)?;
    // SAFETY: both names are NUL-terminated relative child names and the root
    // descriptor remains live for the entire atomic rename.
    if unsafe {
        libc::renameat(
            root.as_raw_fd(),
            from.as_ptr(),
            root.as_raw_fd(),
            to.as_ptr(),
        )
    } != 0
    {
        return Err(Stage7bRecoveryError::SealWriteFailed(
            std::io::Error::last_os_error().kind(),
        ));
    }
    Ok(())
}

fn unlink_child_at(root: &File, name: &str) {
    let Ok(name) = CString::new(name) else {
        return;
    };
    // SAFETY: `name` is a NUL-terminated relative child and this is best-effort
    // cleanup of a non-authoritative temp file only.
    let _ = unsafe { libc::unlinkat(root.as_raw_fd(), name.as_ptr(), 0) };
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Stage7bDurableRootAuthority;
    use broker_core::{
        BrokerAccountId, BrokerCommand, BrokerOrderId, BrokerTradeId, Envelope, Exchange,
        InstrumentId, Market, MessageType, StrategyRequestId, SCHEMA_VERSION,
    };
    use ed25519_dalek::{Signer, SigningKey};
    use redis::aio::ConnectionManager;
    use redis::streams::{StreamPendingCountReply, StreamRangeReply, StreamReadReply};
    use runtime_command_bridge::{
        Stage7aCommandProfile, Stage7aPaperOutcomeProvider, Stage7aPaperProviderError,
    };
    use std::{
        fs::{self, OpenOptions},
        net::TcpListener,
        path::{Path, PathBuf},
        process::{Child, Command, Stdio},
        sync::{
            atomic::{AtomicUsize, Ordering as AtomicOrdering},
            Arc,
        },
        thread,
        time::{Duration, Instant},
    };
    use strategy_runtime_core::{
        authorize_stage6d_first_boot, stage6d_test_authenticated_restart_fixture,
        stage7b_test_authenticated_cancel_restart_fixture,
        stage7b_test_authenticated_working_restart_fixture,
        stage8a4_test_append_durable_batch_with_suffix_limit, stage8a4_test_set_journal_failpoint,
        stage8a4_test_transition_fixture, stage8a4_writer_entry_attestation_sha256,
        Stage6DispatchSafetyStateV1, Stage6JournalRecordV1, Stage6LifecycleSequence,
        Stage6MemoryJournalBackend, Stage6Sha256Digest, Stage6dBootMode, Stage6dFirstBootConfig,
        Stage7bTestExtraStage6History, Stage7bTestRestartFixture, Stage8a4JournalTestFailpoint,
    };

    fn identity() -> Stage6dOperationalIdentityConfig {
        Stage6dOperationalIdentityConfig {
            broker_id: "paper".to_string(),
            strategy_instance_id: "hybrid-imoexf".to_string(),
            deployment_id: "stage7b-c-test".to_string(),
            deployment_generation: 1,
            gateway_instance_id: "gateway-stage7b-c".to_string(),
            instrument_map_fingerprint_sha256: "1".repeat(64),
            market_data_generation: 1,
            command_consumer_generation: 1,
            stage8a4_writer_issuer_public_key_hex:
                "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a".to_string(),
        }
    }

    fn lower_hex(bytes: &[u8]) -> String {
        let mut value = String::with_capacity(bytes.len() * 2);
        for byte in bytes {
            use std::fmt::Write as _;
            write!(&mut value, "{byte:02x}").unwrap();
        }
        value
    }

    fn root(identity: &Stage6dOperationalIdentityConfig) -> (PathBuf, PathBuf) {
        let parent = std::env::temp_dir().join(format!(
            "stage7b-c-{}-{}",
            std::process::id(),
            STAGE7B_RECOVERY_SEAL_TEMP_COUNTER.fetch_add(1, Ordering::SeqCst)
        ));
        fs::create_dir(&parent).unwrap();
        let parent = fs::canonicalize(parent).unwrap();
        let root =
            parent.join(Stage7bDurableRootAuthority::expected_directory_name(identity).unwrap());
        fs::create_dir(&root).unwrap();
        (parent, root)
    }

    fn authorization(
        identity: &Stage6dOperationalIdentityConfig,
        runtime: &HybridIntradayRuntimeStrategy,
    ) -> Stage6dFirstBootAuthorization {
        authorize_stage6d_first_boot(Stage6dFirstBootConfig {
            deployment_id: identity.deployment_id.clone(),
            expected_runtime_config_fingerprint_sha256: runtime.stage5c_config_fingerprint(),
            allow_create_missing_journal: true,
        })
        .unwrap()
    }

    fn first_boot() -> (
        PathBuf,
        PathBuf,
        Stage6dOperationalIdentityConfig,
        Stage7bRecoveryReadyOwner,
    ) {
        let identity = identity();
        let (parent, root) = root(&identity);
        let (seed, key, runtime) = stage6d_test_authenticated_restart_fixture();
        let authorization = authorization(&identity, &runtime);
        let owner = Stage7bRecoveryReadyOwner::first_boot(
            Stage7bDurableRootAuthority::validate(&root, &identity).unwrap(),
            identity.clone(),
            authorization,
            &seed,
            &key,
            runtime,
        )
        .unwrap();
        (parent, root, identity, owner)
    }

    fn instrument() -> InstrumentId {
        InstrumentId {
            symbol: "IMOEXF".to_string(),
            venue_symbol: Some("IMOEXF@RTSX".to_string()),
            exchange: Exchange::Moex,
            market: Market::Futures,
        }
    }

    fn request_id(command: &BrokerCommand) -> StrategyRequestId {
        match command {
            BrokerCommand::PlaceOrder(place) => place.request_id,
            BrokerCommand::CancelOrder(cancel) => cancel.request_id,
        }
    }

    fn command_created_at(command: &BrokerCommand) -> DateTime<Utc> {
        match command {
            BrokerCommand::PlaceOrder(place) => place.created_ts,
            BrokerCommand::CancelOrder(cancel) => cancel.created_ts,
        }
    }

    struct PreparedWorkingRestart {
        parent: PathBuf,
        root: PathBuf,
        identity: Stage6dOperationalIdentityConfig,
        key: Stage5gLifecycleCommitmentKey,
        runtime: HybridIntradayRuntimeStrategy,
        active_request_id: String,
        journal_before_restart: Vec<u8>,
        command: BrokerCommand,
        command_context: Stage7aPaperCommandContext,
    }

    fn prepare_working_restart(
        extra_history: Stage7bTestExtraStage6History,
    ) -> PreparedWorkingRestart {
        let identity = identity();
        let (parent, root) = root(&identity);
        let fixture = stage7b_test_authenticated_working_restart_fixture(extra_history);
        let authorization = authorization(&identity, &fixture.fresh_runtime);
        let mut storage = Stage7bWritableDurableAuthority::create_new(
            Stage7bDurableRootAuthority::validate(&root, &identity).unwrap(),
            &identity,
            &authorization,
        )
        .unwrap();
        let prefix_checkpoint =
            Stage6JournalCheckpointV1::from_frontier(storage.journal.frontier().clone()).unwrap();
        for record in &fixture.journal_records {
            storage.journal.append(record).unwrap();
        }
        let package = seal_stage6d_restart_package(
            &fixture.stage5g_authenticated_package,
            prefix_checkpoint.clone(),
            identity.clone(),
            &fixture.commitment_key,
        )
        .unwrap();
        let seal = Stage7bRecoverySealV1::new(
            1,
            package,
            prefix_checkpoint,
            stage6d_operational_identity_sha256(&identity)
                .unwrap()
                .as_str()
                .to_string(),
            &fixture.commitment_key,
        )
        .unwrap();
        storage._writer_lease.commit_recovery_seal(&seal).unwrap();
        let journal_before_restart = fs::read(root.join(STAGE7B_JOURNAL_FILE)).unwrap();
        drop(storage);
        PreparedWorkingRestart {
            parent,
            root,
            identity,
            key: fixture.commitment_key,
            runtime: fixture.fresh_runtime,
            active_request_id: fixture.active_request_id.to_string(),
            journal_before_restart,
            command: fixture.command,
            command_context: fixture.command_context,
        }
    }

    fn prepare_active_restart_prefix(active_record_count: usize) -> PreparedWorkingRestart {
        prepare_fixture_restart_prefix(
            stage7b_test_authenticated_working_restart_fixture(Stage7bTestExtraStage6History::None),
            active_record_count,
        )
    }

    fn prepare_fixture_restart_prefix(
        fixture: Stage7bTestRestartFixture,
        record_count: usize,
    ) -> PreparedWorkingRestart {
        assert!(record_count <= fixture.journal_records.len());
        let identity = identity();
        let (parent, root) = root(&identity);
        let authorization = authorization(&identity, &fixture.fresh_runtime);
        let mut storage = Stage7bWritableDurableAuthority::create_new(
            Stage7bDurableRootAuthority::validate(&root, &identity).unwrap(),
            &identity,
            &authorization,
        )
        .unwrap();
        let prefix_checkpoint =
            Stage6JournalCheckpointV1::from_frontier(storage.journal.frontier().clone()).unwrap();
        for record in fixture.journal_records.iter().take(record_count) {
            storage.journal.append(record).unwrap();
        }
        let package = seal_stage6d_restart_package(
            &fixture.stage5g_authenticated_package,
            prefix_checkpoint.clone(),
            identity.clone(),
            &fixture.commitment_key,
        )
        .unwrap();
        let seal = Stage7bRecoverySealV1::new(
            1,
            package,
            prefix_checkpoint,
            stage6d_operational_identity_sha256(&identity)
                .unwrap()
                .as_str()
                .to_string(),
            &fixture.commitment_key,
        )
        .unwrap();
        storage._writer_lease.commit_recovery_seal(&seal).unwrap();
        let journal_before_restart = fs::read(root.join(STAGE7B_JOURNAL_FILE)).unwrap();
        drop(storage);
        PreparedWorkingRestart {
            parent,
            root,
            identity,
            key: fixture.commitment_key,
            runtime: fixture.fresh_runtime,
            active_request_id: fixture.active_request_id.to_string(),
            journal_before_restart,
            command: fixture.command,
            command_context: fixture.command_context,
        }
    }

    fn wait_for_child(child: &mut std::process::Child, marker: &Path) {
        let deadline = Instant::now() + Duration::from_secs(20);
        while !marker.exists() && Instant::now() < deadline {
            if let Some(status) = child.try_wait().unwrap() {
                panic!("Stage 7B recovery child exited before barrier: {status}");
            }
            thread::sleep(Duration::from_millis(20));
        }
        assert!(marker.exists(), "Stage 7B recovery child missed barrier");
    }

    fn stage7b_d_a_effect_witness(setup: &PreparedWorkingRestart) -> PathBuf {
        setup.parent.join("stage7b-d-a-provider-effect.count")
    }

    fn commit_stage7b_d_a_provider_effect_witness(path: &Path) {
        let mut witness = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .expect("provider effect witness must be invoked exactly once");
        witness.write_all(b"1\n").unwrap();
        witness.sync_all().unwrap();
        File::open(path.parent().unwrap())
            .unwrap()
            .sync_all()
            .unwrap();
    }

    fn kill_stage7b_d_a_child_at(setup: &PreparedWorkingRestart, phase: &str) -> PathBuf {
        let marker = setup.parent.join(format!("stage7b-d-a-{phase}.barrier"));
        let effect_witness = stage7b_d_a_effect_witness(setup);
        assert!(!marker.exists(), "stale Stage 7B crash barrier marker");
        if phase == "during-effect" {
            assert!(
                !effect_witness.exists(),
                "stale Stage 7B provider-effect witness"
            );
        }
        let mut child = Command::new(std::env::current_exe().unwrap())
            .arg("--ignored")
            .arg("--exact")
            .arg("recovery::tests::stage7b_d_a_crash_barrier_child")
            .arg("--nocapture")
            .env("STAGE7B_D_A_CHILD_ROOT", &setup.root)
            .env("STAGE7B_D_A_CHILD_MARKER", &marker)
            .env("STAGE7B_D_A_CHILD_PHASE", phase)
            .env("STAGE7B_D_A_CHILD_EFFECT_WITNESS", &effect_witness)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        wait_for_child(&mut child, &marker);
        child.kill().unwrap();
        child.wait().unwrap();
        effect_witness
    }

    fn restart_working_setup(setup: &PreparedWorkingRestart) -> Stage7bRecoveryReadyOwner {
        let outcome = Stage7bRecoveryReadyOwner::restart(
            Stage7bDurableRootAuthority::validate(&setup.root, &setup.identity).unwrap(),
            setup.identity.clone(),
            &setup.key,
            setup.runtime.clone(),
        )
        .unwrap();
        let Stage7bRestartOutcome::Ready(owner) = outcome else {
            panic!("Stage 7B d-a crash state must restart Ready");
        };
        *owner
    }

    #[test]
    fn stage8a1_authority_binds_exact_current_stage7b_seal_and_stage6_request() {
        let setup = prepare_active_restart_prefix(2);
        let mut owner = restart_working_setup(&setup);
        let (identity, snapshot) = match &setup.command {
            BrokerCommand::PlaceOrder(command) => {
                let identity = Stage6DurableRequestIdentityV1::from_place(
                    command,
                    setup.command_context.attribution().clone(),
                )
                .unwrap();
                let snapshot =
                    Stage6DurableCommandSnapshotV1::from_place(&identity, command).unwrap();
                (identity, snapshot)
            }
            BrokerCommand::CancelOrder(_) => panic!("working fixture must be PLACE"),
        };
        let authority = owner
            .authorize_stage8a1_durable_request(&setup.key, &identity, &snapshot)
            .unwrap();
        assert_eq!(authority.stage6().identity(), &identity);
        assert_eq!(
            authority.seal_generation(),
            owner.committed_seal().unwrap().seal_generation()
        );
        assert_eq!(
            authority.seal_commitment_sha256(),
            owner.committed_seal().unwrap().seal_commitment_sha256()
        );
        drop(owner);
        fs::remove_dir_all(setup.parent).unwrap();
    }

    #[test]
    fn stage8a4_i3_writer_commits_covering_s1_and_restarts_from_mixed_journal() {
        let setup = prepare_active_restart_prefix(2);
        let source_fixture =
            stage7b_test_authenticated_working_restart_fixture(Stage7bTestExtraStage6History::None);
        let dispatch = source_fixture.journal_records[1].clone();
        let mut owner = restart_working_setup(&setup);
        let (identity, snapshot) =
            stage8a1_identity_and_snapshot(&setup.command, &setup.command_context);
        let source_authority = owner
            .authorize_stage8a1_durable_request(&setup.key, &identity, &snapshot)
            .unwrap();
        assert_eq!(
            source_authority.stage6().dispatch_record_id(),
            dispatch.journal_record_id()
        );
        let seal_generation = source_authority.seal_generation();
        let seal_fingerprint =
            Stage6Sha256Digest::parse(source_authority.seal_commitment_sha256().to_string())
                .unwrap();
        let expected_frontier = Stage6Sha256Digest::parse(
            source_authority
                .stage6()
                .durable_frontier_sha256()
                .to_string(),
        )
        .unwrap();
        let request_fingerprint = owner
            .recovered()
            .unwrap()
            .replay()
            .request(identity.strategy_request_id())
            .unwrap()
            .state_fingerprint_sha256();
        let (transition, suffix) = stage8a4_test_transition_fixture(
            &identity,
            &dispatch,
            source_authority
                .stage6()
                .durable_request_binding_sha256()
                .unwrap(),
            expected_frontier,
            seal_generation,
            seal_fingerprint,
            request_fingerprint,
            1,
        );
        let receipt = owner
            .append_stage8a4_test_batch_and_cover(
                &setup.key,
                &identity,
                &snapshot,
                Stage6Stage8a4DurableBatch::new(transition.clone(), suffix.clone(), None).unwrap(),
            )
            .unwrap();
        assert!(!receipt.transition_was_existing());
        assert_eq!(receipt.appended_suffix_records(), 1);
        assert!(receipt.covering_seal_generation() > seal_generation);
        assert_eq!(
            receipt.stage6_checkpoint_sha256(),
            owner
                .committed_seal()
                .unwrap()
                .stage6_checkpoint()
                .checkpoint_sha256()
        );

        let replay_receipt = owner
            .append_stage8a4_test_batch_and_cover(
                &setup.key,
                &identity,
                &snapshot,
                Stage6Stage8a4DurableBatch::new(transition, suffix, None).unwrap(),
            )
            .unwrap();
        assert!(replay_receipt.transition_was_existing());
        assert_eq!(replay_receipt.appended_suffix_records(), 0);
        assert_eq!(
            replay_receipt.covering_seal_generation(),
            receipt.covering_seal_generation()
        );

        drop(owner);
        let restarted = Stage7bRecoveryReadyOwner::restart(
            Stage7bDurableRootAuthority::validate(&setup.root, &setup.identity).unwrap(),
            setup.identity.clone(),
            &setup.key,
            setup.runtime.clone(),
        )
        .unwrap();
        let Stage7bRestartOutcome::Ready(restarted) = restarted else {
            panic!("covered I3 mixed journal must restart ready");
        };
        assert_eq!(
            restarted
                .committed_seal()
                .unwrap()
                .stage6_checkpoint()
                .checkpoint_sha256(),
            receipt.stage6_checkpoint_sha256()
        );
        drop(restarted);
        fs::remove_dir_all(setup.parent).unwrap();
    }

    #[test]
    fn stage8a4_i3_post_write_fault_matrix_poison_is_sticky_in_process() {
        for failpoint in [
            Stage8a4JournalTestFailpoint::AfterFrameHeaderWrite,
            Stage8a4JournalTestFailpoint::AfterPartialRecordWrite,
            Stage8a4JournalTestFailpoint::AfterFrameHashWrite,
            Stage8a4JournalTestFailpoint::BeforeSync,
            Stage8a4JournalTestFailpoint::SyncFailure,
        ] {
            let setup = prepare_active_restart_prefix(2);
            let source_fixture = stage7b_test_authenticated_working_restart_fixture(
                Stage7bTestExtraStage6History::None,
            );
            let dispatch = source_fixture.journal_records[1].clone();
            let mut owner = restart_working_setup(&setup);
            let (identity, snapshot) =
                stage8a1_identity_and_snapshot(&setup.command, &setup.command_context);
            let source_authority = owner
                .authorize_stage8a1_durable_request(&setup.key, &identity, &snapshot)
                .unwrap();
            let seal_generation = source_authority.seal_generation();
            let seal_fingerprint =
                Stage6Sha256Digest::parse(source_authority.seal_commitment_sha256().to_string())
                    .unwrap();
            let expected_frontier = Stage6Sha256Digest::parse(
                source_authority
                    .stage6()
                    .durable_frontier_sha256()
                    .to_string(),
            )
            .unwrap();
            let request_fingerprint = owner
                .recovered()
                .unwrap()
                .replay()
                .request(identity.strategy_request_id())
                .unwrap()
                .state_fingerprint_sha256();
            let (transition, suffix) = stage8a4_test_transition_fixture(
                &identity,
                &dispatch,
                source_authority
                    .stage6()
                    .durable_request_binding_sha256()
                    .unwrap(),
                expected_frontier,
                seal_generation,
                seal_fingerprint,
                request_fingerprint,
                1,
            );
            stage8a4_test_set_journal_failpoint(&mut owner.recovered, Some(failpoint)).unwrap();
            let result = owner.append_stage8a4_test_batch_and_cover(
                &setup.key,
                &identity,
                &snapshot,
                Stage6Stage8a4DurableBatch::new(transition.clone(), suffix.clone(), None).unwrap(),
            );
            assert!(matches!(
                result,
                Err(Stage7bRecoveryError::Runtime(
                    Stage6dLiveCoreError::JournalMutationMayHaveOccurred
                ))
            ));
            assert!(!owner.recovery_ready());
            assert!(owner
                .authorize_stage8a1_durable_request(&setup.key, &identity, &snapshot)
                .is_err());
            assert!(owner
                .append_stage8a4_test_batch_and_cover(
                    &setup.key,
                    &identity,
                    &snapshot,
                    Stage6Stage8a4DurableBatch::new(transition, suffix, None).unwrap(),
                )
                .is_err());
            assert!(owner.advance_recovery_seal(&setup.key).is_err());
            drop(owner);
            fs::remove_dir_all(setup.parent).unwrap();
        }
    }

    #[test]
    fn stage8a4_i3_suffix_post_write_faults_are_sticky_in_process() {
        for failpoint in [
            Stage8a4JournalTestFailpoint::AfterPartialRecordWrite,
            Stage8a4JournalTestFailpoint::AfterFrameHashWrite,
            Stage8a4JournalTestFailpoint::SyncFailure,
        ] {
            let setup = prepare_active_restart_prefix(2);
            let source_fixture = stage7b_test_authenticated_working_restart_fixture(
                Stage7bTestExtraStage6History::None,
            );
            let dispatch = source_fixture.journal_records[1].clone();
            let mut owner = restart_working_setup(&setup);
            let (identity, snapshot) =
                stage8a1_identity_and_snapshot(&setup.command, &setup.command_context);
            let source_authority = owner
                .authorize_stage8a1_durable_request(&setup.key, &identity, &snapshot)
                .unwrap();
            let seal_generation = source_authority.seal_generation();
            let request_fingerprint = owner
                .recovered()
                .unwrap()
                .replay()
                .request(identity.strategy_request_id())
                .unwrap()
                .state_fingerprint_sha256();
            let (transition, suffix) = stage8a4_test_transition_fixture(
                &identity,
                &dispatch,
                source_authority
                    .stage6()
                    .durable_request_binding_sha256()
                    .unwrap(),
                Stage6Sha256Digest::parse(
                    source_authority
                        .stage6()
                        .durable_frontier_sha256()
                        .to_string(),
                )
                .unwrap(),
                seal_generation,
                Stage6Sha256Digest::parse(source_authority.seal_commitment_sha256().to_string())
                    .unwrap(),
                request_fingerprint,
                1,
            );
            let stage6_authority = owner
                .recovered
                .authorize_stage8a4_durable_batch_source(&identity, &snapshot)
                .unwrap();
            assert!(matches!(
                stage8a4_test_append_durable_batch_with_suffix_limit(
                    &mut owner.recovered,
                    stage6_authority,
                    Stage6Stage8a4DurableBatch::new(transition.clone(), suffix.clone(), None,)
                        .unwrap(),
                    0,
                ),
                Err(Stage6dLiveCoreError::JournalMutationMayHaveOccurred)
            ));
            drop(owner);

            let restarted = Stage7bRecoveryReadyOwner::restart(
                Stage7bDurableRootAuthority::validate(&setup.root, &setup.identity).unwrap(),
                setup.identity.clone(),
                &setup.key,
                setup.runtime.clone(),
            )
            .unwrap();
            let Stage7bRestartOutcome::Stage8a4I3Pending(mut owner) = restarted else {
                panic!("exact uncovered I3 V2 must remain pending before suffix retry");
            };
            assert!(!owner.recovery_ready());
            stage8a4_test_set_journal_failpoint(&mut owner.recovered, Some(failpoint)).unwrap();
            let pending = owner.pending_recovery_material(&setup.key).unwrap();
            let (_, persisted_command) = pending.into_parts();
            let current = owner
                .authorize_pending_recovery_request(&setup.key, &identity, &persisted_command)
                .unwrap();
            let sealed = strategy_runtime_core::stage8a4_test_attest_validated_entry(
                identity.clone(),
                persisted_command,
                Stage6Stage8a4DurableBatch::recover_from_persisted_transition(transition.clone())
                    .unwrap(),
                current.operational_identity_sha256().to_string(),
                current
                    .stage6()
                    .runtime_config_fingerprint_sha256()
                    .to_string(),
                current.seal_generation(),
                current.seal_commitment_sha256().to_string(),
                Stage6Sha256Digest::parse("33".repeat(32)).unwrap(),
                Stage6Sha256Digest::parse("11".repeat(32)).unwrap(),
                Stage6Sha256Digest::parse("22".repeat(32)).unwrap(),
            )
            .unwrap();
            let result = owner.append_recovery_entry_and_cover(&setup.key, sealed);
            assert!(matches!(
                result,
                Err(Stage7bRecoveryError::Runtime(
                    Stage6dLiveCoreError::JournalMutationMayHaveOccurred
                ))
            ));
            drop(suffix);
            fs::remove_dir_all(setup.parent).unwrap();
        }
    }

    #[test]
    fn stage8a4_i3_pre_write_failure_does_not_poison_owner() {
        let setup = prepare_active_restart_prefix(2);
        let mut owner = restart_working_setup(&setup);
        let (identity, snapshot) =
            stage8a1_identity_and_snapshot(&setup.command, &setup.command_context);
        stage8a4_test_set_journal_failpoint(
            &mut owner.recovered,
            Some(Stage8a4JournalTestFailpoint::BeforeFrameWrite),
        )
        .unwrap();
        let source_fixture =
            stage7b_test_authenticated_working_restart_fixture(Stage7bTestExtraStage6History::None);
        let dispatch = source_fixture.journal_records[1].clone();
        let authority = owner
            .authorize_stage8a1_durable_request(&setup.key, &identity, &snapshot)
            .unwrap();
        let request_fingerprint = owner
            .recovered()
            .unwrap()
            .replay()
            .request(identity.strategy_request_id())
            .unwrap()
            .state_fingerprint_sha256();
        let (transition, suffix) = stage8a4_test_transition_fixture(
            &identity,
            &dispatch,
            authority.stage6().durable_request_binding_sha256().unwrap(),
            Stage6Sha256Digest::parse(authority.stage6().durable_frontier_sha256().to_string())
                .unwrap(),
            authority.seal_generation(),
            Stage6Sha256Digest::parse(authority.seal_commitment_sha256().to_string()).unwrap(),
            request_fingerprint,
            1,
        );
        let result = owner.append_stage8a4_test_batch_and_cover(
            &setup.key,
            &identity,
            &snapshot,
            Stage6Stage8a4DurableBatch::new(transition, suffix, None).unwrap(),
        );
        assert!(matches!(
            result,
            Err(Stage7bRecoveryError::Runtime(
                Stage6dLiveCoreError::Journal(_)
            ))
        ));
        stage8a4_test_set_journal_failpoint(&mut owner.recovered, None).unwrap();
        assert!(owner.recovery_ready());
        assert!(owner
            .authorize_stage8a1_durable_request(&setup.key, &identity, &snapshot)
            .is_ok());
        drop(owner);
        fs::remove_dir_all(setup.parent).unwrap();
    }

    #[test]
    fn stage8a4_i3_writer_rejects_stale_seal_cas_before_journal_mutation() {
        let setup = prepare_active_restart_prefix(2);
        let source_fixture =
            stage7b_test_authenticated_working_restart_fixture(Stage7bTestExtraStage6History::None);
        let dispatch = source_fixture.journal_records[1].clone();
        let mut owner = restart_working_setup(&setup);
        let (identity, snapshot) =
            stage8a1_identity_and_snapshot(&setup.command, &setup.command_context);
        let source_authority = owner
            .authorize_stage8a1_durable_request(&setup.key, &identity, &snapshot)
            .unwrap();
        let before = owner.recovered().unwrap().journal_frontier().frame_count();
        let expected_frontier = Stage6Sha256Digest::parse(
            source_authority
                .stage6()
                .durable_frontier_sha256()
                .to_string(),
        )
        .unwrap();
        let request_fingerprint = owner
            .recovered()
            .unwrap()
            .replay()
            .request(identity.strategy_request_id())
            .unwrap()
            .state_fingerprint_sha256();
        let (transition, suffix) = stage8a4_test_transition_fixture(
            &identity,
            &dispatch,
            source_authority
                .stage6()
                .durable_request_binding_sha256()
                .unwrap(),
            expected_frontier,
            source_authority.seal_generation().checked_add(1).unwrap(),
            Stage6Sha256Digest::parse(source_authority.seal_commitment_sha256().to_string())
                .unwrap(),
            request_fingerprint,
            1,
        );

        assert!(matches!(
            owner.append_stage8a4_test_batch_and_cover(
                &setup.key,
                &identity,
                &snapshot,
                Stage6Stage8a4DurableBatch::new(transition, suffix, None).unwrap(),
            ),
            Err(Stage7bRecoveryError::SealInvalid)
        ));
        assert_eq!(
            owner.recovered().unwrap().journal_frontier().frame_count(),
            before
        );
        drop(owner);
        fs::remove_dir_all(setup.parent).unwrap();
    }

    #[test]
    fn stage8a4_i3_rejects_forged_or_wrong_trust_root_attestation_before_append() {
        let setup = prepare_active_restart_prefix(2);
        let source_fixture =
            stage7b_test_authenticated_working_restart_fixture(Stage7bTestExtraStage6History::None);
        let dispatch = source_fixture.journal_records[1].clone();
        let mut owner = restart_working_setup(&setup);
        let (identity, snapshot) =
            stage8a1_identity_and_snapshot(&setup.command, &setup.command_context);
        let current = owner
            .authorize_stage8a1_durable_request(&setup.key, &identity, &snapshot)
            .unwrap();
        let before = owner.recovered().unwrap().journal_frontier().frame_count();
        let request_fingerprint = owner
            .recovered()
            .unwrap()
            .replay()
            .request(identity.strategy_request_id())
            .unwrap()
            .state_fingerprint_sha256();
        let (transition, suffix) = stage8a4_test_transition_fixture(
            &identity,
            &dispatch,
            current.stage6().durable_request_binding_sha256().unwrap(),
            Stage6Sha256Digest::parse(current.stage6().durable_frontier_sha256().to_string())
                .unwrap(),
            current.seal_generation(),
            Stage6Sha256Digest::parse(current.seal_commitment_sha256().to_string()).unwrap(),
            request_fingerprint,
            1,
        );
        let batch = Stage6Stage8a4DurableBatch::new(transition, suffix, None).unwrap();
        let operational = current.operational_identity_sha256().to_string();
        let runtime = current
            .stage6()
            .runtime_config_fingerprint_sha256()
            .to_string();
        let seal = current.seal_commitment_sha256().to_string();
        let source = Stage6Sha256Digest::parse("33".repeat(32)).unwrap();
        let truth = Stage6Sha256Digest::parse("11".repeat(32)).unwrap();
        let control = Stage6Sha256Digest::parse("22".repeat(32)).unwrap();
        let attestation = stage8a4_writer_entry_attestation_sha256(
            &identity,
            &snapshot,
            &batch,
            &operational,
            &runtime,
            current.seal_generation(),
            &seal,
            &source,
            &truth,
            &control,
        )
        .unwrap();
        let attacker = SigningKey::from_bytes(&[42_u8; 32]);
        let public_key = lower_hex(attacker.verifying_key().as_bytes());
        let signature = lower_hex(&attacker.sign(attestation.as_str().as_bytes()).to_bytes());

        let mut malformed_signature = signature.clone();
        malformed_signature.replace_range(0..2, "00");
        assert!(matches!(
            Stage6Stage8a4ValidatedWriteEntry::verify_issuer_attestation(
                identity.clone(),
                snapshot.clone(),
                Stage6Stage8a4DurableBatch::recover_from_persisted_transition(
                    batch.transition_record().clone(),
                )
                .unwrap(),
                operational.clone(),
                runtime.clone(),
                current.seal_generation(),
                seal.clone(),
                source.clone(),
                truth.clone(),
                control.clone(),
                public_key.clone(),
                malformed_signature,
            ),
            Err(Stage6dLiveCoreError::Stage8a4WriteAuthorityInvalid)
        ));

        let entry = Stage6Stage8a4ValidatedWriteEntry::verify_issuer_attestation(
            identity,
            snapshot,
            batch,
            operational,
            runtime,
            current.seal_generation(),
            seal,
            source,
            truth,
            control,
            public_key,
            signature,
        )
        .unwrap();
        assert!(matches!(
            owner.append_stage8a4_validated_entry_and_cover(&setup.key, entry),
            Err(Stage7bRecoveryError::Runtime(
                Stage6dLiveCoreError::Stage8a4WriteAuthorityInvalid
            ))
        ));
        assert_eq!(
            owner.recovered().unwrap().journal_frontier().frame_count(),
            before
        );
        drop(owner);
        fs::remove_dir_all(setup.parent).unwrap();
    }

    #[test]
    fn stage8a4_i3_restart_covers_v2_only_crash_then_repairs_exact_suffix() {
        let setup = prepare_active_restart_prefix(2);
        let source_fixture =
            stage7b_test_authenticated_working_restart_fixture(Stage7bTestExtraStage6History::None);
        let dispatch = source_fixture.journal_records[1].clone();
        let mut owner = restart_working_setup(&setup);
        let (identity, snapshot) =
            stage8a1_identity_and_snapshot(&setup.command, &setup.command_context);
        let source_authority = owner
            .authorize_stage8a1_durable_request(&setup.key, &identity, &snapshot)
            .unwrap();
        let s0_generation = source_authority.seal_generation();
        let s0_fingerprint =
            Stage6Sha256Digest::parse(source_authority.seal_commitment_sha256().to_string())
                .unwrap();
        let expected_frontier = Stage6Sha256Digest::parse(
            source_authority
                .stage6()
                .durable_frontier_sha256()
                .to_string(),
        )
        .unwrap();
        let request_fingerprint = owner
            .recovered()
            .unwrap()
            .replay()
            .request(identity.strategy_request_id())
            .unwrap()
            .state_fingerprint_sha256();
        let (transition, suffix) = stage8a4_test_transition_fixture(
            &identity,
            &dispatch,
            source_authority
                .stage6()
                .durable_request_binding_sha256()
                .unwrap(),
            expected_frontier,
            s0_generation,
            s0_fingerprint,
            request_fingerprint,
            1,
        );
        let authority = owner
            .recovered
            .authorize_stage8a4_durable_batch_source(&identity, &snapshot)
            .unwrap();
        assert!(matches!(
            stage8a4_test_append_durable_batch_with_suffix_limit(
                &mut owner.recovered,
                authority,
                Stage6Stage8a4DurableBatch::new(transition.clone(), suffix.clone(), None,).unwrap(),
                0,
            ),
            Err(Stage6dLiveCoreError::JournalMutationMayHaveOccurred)
        ));
        assert_eq!(
            owner.committed_seal().unwrap().seal_generation(),
            s0_generation
        );
        assert_eq!(
            owner.recovered().unwrap().journal_frontier().frame_count(),
            3
        );
        drop(owner);

        let restarted = Stage7bRecoveryReadyOwner::restart(
            Stage7bDurableRootAuthority::validate(&setup.root, &setup.identity).unwrap(),
            setup.identity.clone(),
            &setup.key,
            setup.runtime.clone(),
        )
        .unwrap();
        let Stage7bRestartOutcome::Stage8a4I3Pending(owner) = restarted else {
            panic!("exact uncovered I3 V2 must remain pending on restart");
        };
        assert!(!owner.recovery_ready());
        drop(owner);
        let restarted_again = Stage7bRecoveryReadyOwner::restart(
            Stage7bDurableRootAuthority::validate(&setup.root, &setup.identity).unwrap(),
            setup.identity.clone(),
            &setup.key,
            setup.runtime.clone(),
        )
        .unwrap();
        let Stage7bRestartOutcome::Stage8a4I3Pending(mut owner) = restarted_again else {
            panic!("second V2-only restart must remain pending without a new seal");
        };
        assert!(!owner.recovery_ready());

        drop(transition);
        drop(suffix);
        let pending = owner.pending_recovery_material(&setup.key).unwrap();
        let (persisted_transition, persisted_command) = pending.into_parts();
        let batch =
            Stage6Stage8a4DurableBatch::recover_from_persisted_transition(persisted_transition)
                .unwrap();
        let current = owner
            .authorize_pending_recovery_request(&setup.key, &identity, &persisted_command)
            .unwrap();
        let sealed = strategy_runtime_core::stage8a4_test_attest_validated_entry(
            identity.clone(),
            persisted_command,
            batch,
            current.operational_identity_sha256().to_string(),
            current
                .stage6()
                .runtime_config_fingerprint_sha256()
                .to_string(),
            current.seal_generation(),
            current.seal_commitment_sha256().to_string(),
            Stage6Sha256Digest::parse("33".repeat(32)).unwrap(),
            Stage6Sha256Digest::parse("11".repeat(32)).unwrap(),
            Stage6Sha256Digest::parse("22".repeat(32)).unwrap(),
        )
        .unwrap();
        let (receipt, owner) = owner
            .append_recovery_entry_and_cover(&setup.key, sealed)
            .unwrap();
        assert!(receipt.transition_was_existing());
        assert_eq!(receipt.appended_suffix_records(), 1);
        assert_eq!(
            owner
                .committed_seal()
                .unwrap()
                .stage6_checkpoint()
                .frontier()
                .frame_count(),
            4
        );
        drop(owner);

        let final_restart = Stage7bRecoveryReadyOwner::restart(
            Stage7bDurableRootAuthority::validate(&setup.root, &setup.identity).unwrap(),
            setup.identity.clone(),
            &setup.key,
            setup.runtime,
        )
        .unwrap();
        assert!(matches!(final_restart, Stage7bRestartOutcome::Ready(_)));
        drop(final_restart);
        fs::remove_dir_all(setup.parent).unwrap();
    }

    #[test]
    fn stage8a4_i3_restart_rejects_unrelated_record_after_uncovered_v2() {
        let setup = prepare_active_restart_prefix(2);
        let source_fixture =
            stage7b_test_authenticated_working_restart_fixture(Stage7bTestExtraStage6History::None);
        let dispatch = source_fixture.journal_records[1].clone();
        let mut owner = restart_working_setup(&setup);
        let (identity, snapshot) =
            stage8a1_identity_and_snapshot(&setup.command, &setup.command_context);
        let source_authority = owner
            .authorize_stage8a1_durable_request(&setup.key, &identity, &snapshot)
            .unwrap();
        let expected_frontier = Stage6Sha256Digest::parse(
            source_authority
                .stage6()
                .durable_frontier_sha256()
                .to_string(),
        )
        .unwrap();
        let request_fingerprint = owner
            .recovered()
            .unwrap()
            .replay()
            .request(identity.strategy_request_id())
            .unwrap()
            .state_fingerprint_sha256();
        let (transition, suffix) = stage8a4_test_transition_fixture(
            &identity,
            &dispatch,
            source_authority
                .stage6()
                .durable_request_binding_sha256()
                .unwrap(),
            expected_frontier,
            source_authority.seal_generation(),
            Stage6Sha256Digest::parse(source_authority.seal_commitment_sha256().to_string())
                .unwrap(),
            request_fingerprint,
            1,
        );
        let unrelated = Stage6JournalRecordV1::broker_order_observed(
            identity.clone(),
            BrokerOrderId::new("ORDER-I3-UNRELATED"),
            Stage6LifecycleSequence::new(
                transition
                    .lifecycle_sequence()
                    .get()
                    .checked_add(1)
                    .unwrap(),
            )
            .unwrap(),
            Some(transition.journal_record_id().clone()),
            Stage6Sha256Digest::parse("c".repeat(64)).unwrap(),
        )
        .unwrap();
        let authority = owner
            .recovered
            .authorize_stage8a4_durable_batch_source(&identity, &snapshot)
            .unwrap();
        assert!(stage8a4_test_append_durable_batch_with_suffix_limit(
            &mut owner.recovered,
            authority,
            Stage6Stage8a4DurableBatch::new(transition, suffix, None).unwrap(),
            0,
        )
        .is_err());
        drop(owner);

        // A canonical but unrelated old Stage 6 record is not a valid I3
        // manifest suffix. The narrow journal-ahead recognizer must refuse to
        // cover it with a new recovery seal.
        let mut storage = Stage7bWritableDurableAuthority::open_existing(
            Stage7bDurableRootAuthority::validate(&setup.root, &setup.identity).unwrap(),
            &setup.identity,
        )
        .unwrap();
        storage.journal.append(&unrelated).unwrap();
        drop(storage);

        let restarted = Stage7bRecoveryReadyOwner::restart(
            Stage7bDurableRootAuthority::validate(&setup.root, &setup.identity).unwrap(),
            setup.identity.clone(),
            &setup.key,
            setup.runtime,
        );
        assert!(!matches!(restarted, Ok(Stage7bRestartOutcome::Ready(_))));
        drop(restarted);
        fs::remove_dir_all(setup.parent).unwrap();
    }

    #[test]
    fn stage8a4_i3_restart_covers_partial_suffix_then_appends_only_missing_record() {
        let setup = prepare_active_restart_prefix(2);
        let source_fixture =
            stage7b_test_authenticated_working_restart_fixture(Stage7bTestExtraStage6History::None);
        let dispatch = source_fixture.journal_records[1].clone();
        let mut owner = restart_working_setup(&setup);
        let (identity, snapshot) =
            stage8a1_identity_and_snapshot(&setup.command, &setup.command_context);
        let source_authority = owner
            .authorize_stage8a1_durable_request(&setup.key, &identity, &snapshot)
            .unwrap();
        let s0_generation = source_authority.seal_generation();
        let expected_frontier = Stage6Sha256Digest::parse(
            source_authority
                .stage6()
                .durable_frontier_sha256()
                .to_string(),
        )
        .unwrap();
        let request_fingerprint = owner
            .recovered()
            .unwrap()
            .replay()
            .request(identity.strategy_request_id())
            .unwrap()
            .state_fingerprint_sha256();
        let (transition, suffix) = stage8a4_test_transition_fixture(
            &identity,
            &dispatch,
            source_authority
                .stage6()
                .durable_request_binding_sha256()
                .unwrap(),
            expected_frontier,
            s0_generation,
            Stage6Sha256Digest::parse(source_authority.seal_commitment_sha256().to_string())
                .unwrap(),
            request_fingerprint,
            2,
        );
        let authority = owner
            .recovered
            .authorize_stage8a4_durable_batch_source(&identity, &snapshot)
            .unwrap();
        assert!(matches!(
            stage8a4_test_append_durable_batch_with_suffix_limit(
                &mut owner.recovered,
                authority,
                Stage6Stage8a4DurableBatch::new(transition.clone(), suffix.clone(), None,).unwrap(),
                1,
            ),
            Err(Stage6dLiveCoreError::JournalMutationMayHaveOccurred)
        ));
        assert_eq!(
            owner.recovered().unwrap().journal_frontier().frame_count(),
            4
        );
        drop(owner);

        let restarted = Stage7bRecoveryReadyOwner::restart(
            Stage7bDurableRootAuthority::validate(&setup.root, &setup.identity).unwrap(),
            setup.identity.clone(),
            &setup.key,
            setup.runtime.clone(),
        )
        .unwrap();
        let Stage7bRestartOutcome::Stage8a4I3Pending(owner) = restarted else {
            panic!("exact I3 partial suffix must remain pending on restart");
        };
        assert!(!owner.recovery_ready());
        drop(owner);
        let restarted_again = Stage7bRecoveryReadyOwner::restart(
            Stage7bDurableRootAuthority::validate(&setup.root, &setup.identity).unwrap(),
            setup.identity.clone(),
            &setup.key,
            setup.runtime.clone(),
        )
        .unwrap();
        let Stage7bRestartOutcome::Stage8a4I3Pending(mut owner) = restarted_again else {
            panic!("second partial-suffix restart must remain pending without a new seal");
        };
        assert!(!owner.recovery_ready());

        drop(transition);
        drop(suffix);
        let pending = owner.pending_recovery_material(&setup.key).unwrap();
        let (persisted_transition, persisted_command) = pending.into_parts();
        let batch =
            Stage6Stage8a4DurableBatch::recover_from_persisted_transition(persisted_transition)
                .unwrap();
        let current = owner
            .authorize_pending_recovery_request(&setup.key, &identity, &persisted_command)
            .unwrap();
        let sealed = strategy_runtime_core::stage8a4_test_attest_validated_entry(
            identity.clone(),
            persisted_command,
            batch,
            current.operational_identity_sha256().to_string(),
            current
                .stage6()
                .runtime_config_fingerprint_sha256()
                .to_string(),
            current.seal_generation(),
            current.seal_commitment_sha256().to_string(),
            Stage6Sha256Digest::parse("33".repeat(32)).unwrap(),
            Stage6Sha256Digest::parse("11".repeat(32)).unwrap(),
            Stage6Sha256Digest::parse("22".repeat(32)).unwrap(),
        )
        .unwrap();
        let (receipt, owner) = owner
            .append_recovery_entry_and_cover(&setup.key, sealed)
            .unwrap();
        assert!(receipt.transition_was_existing());
        assert_eq!(receipt.appended_suffix_records(), 1);
        assert_eq!(
            owner
                .committed_seal()
                .unwrap()
                .stage6_checkpoint()
                .frontier()
                .frame_count(),
            5
        );
        drop(owner);

        let final_restart = Stage7bRecoveryReadyOwner::restart(
            Stage7bDurableRootAuthority::validate(&setup.root, &setup.identity).unwrap(),
            setup.identity.clone(),
            &setup.key,
            setup.runtime,
        )
        .unwrap();
        assert!(matches!(final_restart, Stage7bRestartOutcome::Ready(_)));
        drop(final_restart);
        fs::remove_dir_all(setup.parent).unwrap();
    }

    fn stage8a1_identity_and_snapshot(
        command: &BrokerCommand,
        context: &Stage7aPaperCommandContext,
    ) -> (
        Stage6DurableRequestIdentityV1,
        Stage6DurableCommandSnapshotV1,
    ) {
        match command {
            BrokerCommand::PlaceOrder(place) => {
                let identity = Stage6DurableRequestIdentityV1::from_place(
                    place,
                    context.attribution().clone(),
                )
                .unwrap();
                let snapshot =
                    Stage6DurableCommandSnapshotV1::from_place(&identity, place).unwrap();
                (identity, snapshot)
            }
            BrokerCommand::CancelOrder(cancel) => {
                let identity = Stage6DurableRequestIdentityV1::from_cancel(
                    cancel,
                    context.instrument().clone(),
                    context.attribution().clone(),
                )
                .unwrap();
                let snapshot =
                    Stage6DurableCommandSnapshotV1::from_cancel(&identity, cancel).unwrap();
                (identity, snapshot)
            }
        }
    }

    #[test]
    fn stage8a1_deleted_or_corrupt_current_disk_seal_fails_sticky() {
        for corrupt in [false, true] {
            let setup = prepare_active_restart_prefix(2);
            let mut owner = restart_working_setup(&setup);
            let (identity, snapshot) =
                stage8a1_identity_and_snapshot(&setup.command, &setup.command_context);
            let seal_path = setup.root.join(STAGE7B_RECOVERY_SEAL_FILE);
            if corrupt {
                fs::write(&seal_path, b"corrupt-stage8a1-current-seal").unwrap();
                File::open(&seal_path).unwrap().sync_all().unwrap();
            } else {
                fs::remove_file(&seal_path).unwrap();
            }
            File::open(&setup.root).unwrap().sync_all().unwrap();
            assert!(matches!(
                owner.authorize_stage8a1_durable_request(&setup.key, &identity, &snapshot),
                Err(Stage7bRecoveryError::SealInvalid)
            ));
            assert!(matches!(
                owner.authorize_stage8a1_durable_request(&setup.key, &identity, &snapshot),
                Err(Stage7bRecoveryError::SealCommitUncertain)
            ));
            drop(owner);
            fs::remove_dir_all(setup.parent).unwrap();
        }
    }

    #[test]
    fn stage8a1_forward_dispatch_without_second_restart_advances_covering_seal() {
        let setup = prepare_active_restart_prefix(1);
        let mut owner = restart_working_setup(&setup);
        let observed_at = command_created_at(&setup.command) + chrono::Duration::seconds(1);
        let Stage7aPaperAdmission::DispatchReady(_receipt) = owner
            .admit_paper_command(&setup.command, &setup.command_context, observed_at)
            .unwrap()
        else {
            panic!("accepted-only command must become dispatch-ready without another restart");
        };
        let generation_before = owner.committed_seal().unwrap().seal_generation();
        let (request_identity, snapshot) =
            stage8a1_identity_and_snapshot(&setup.command, &setup.command_context);
        let authority = owner
            .authorize_stage8a1_durable_request(&setup.key, &request_identity, &snapshot)
            .unwrap();
        assert!(authority.seal_generation() > generation_before);
        assert_eq!(
            authority.stage6().dispatch_sequence(),
            owner
                .recovered()
                .unwrap()
                .replay()
                .request(request_identity.strategy_request_id())
                .unwrap()
                .last_unique_sequence()
        );
        drop(owner);
        fs::remove_dir_all(setup.parent).unwrap();
    }

    #[test]
    fn first_boot_requires_stage5g_seed() {
        let identity = identity();
        let (parent, root) = root(&identity);
        let (_, key, runtime) = stage6d_test_authenticated_restart_fixture();
        let authorization = authorization(&identity, &runtime);
        assert!(Stage7bRecoveryReadyOwner::first_boot(
            Stage7bDurableRootAuthority::validate(&root, &identity).unwrap(),
            identity,
            authorization,
            b"",
            &key,
            runtime,
        )
        .is_err());
        assert!(!root.join(STAGE7B_JOURNAL_FILE).exists());
        fs::remove_dir_all(parent).unwrap();
    }

    #[test]
    fn invalid_stage5g_seed_rejected_before_journal_creation() {
        let identity = identity();
        let (parent, root) = root(&identity);
        let (mut seed, key, runtime) = stage6d_test_authenticated_restart_fixture();
        seed[0] ^= 1;
        let authorization = authorization(&identity, &runtime);
        assert!(Stage7bRecoveryReadyOwner::first_boot(
            Stage7bDurableRootAuthority::validate(&root, &identity).unwrap(),
            identity,
            authorization,
            &seed,
            &key,
            runtime,
        )
        .is_err());
        assert!(!root.join(STAGE7B_JOURNAL_FILE).exists());
        assert!(!root.join(STAGE7B_RECOVERY_SEAL_FILE).exists());
        fs::remove_dir_all(parent).unwrap();
    }

    #[test]
    fn initial_recovery_seal_before_ready_and_lease_lifetime() {
        let (parent, root, identity, owner) = first_boot();
        assert!(owner.recovery_ready());
        assert_eq!(owner.committed_seal().unwrap().seal_generation(), 1);
        assert_eq!(
            owner.recovered().unwrap().boot_mode(),
            Stage6dBootMode::FirstBoot
        );
        assert!(root.join(STAGE7B_RECOVERY_SEAL_FILE).is_file());
        assert!(matches!(
            Stage7bWritableDurableAuthority::open_existing(
                Stage7bDurableRootAuthority::validate(&root, &identity).unwrap(),
                &identity,
            ),
            Err(Stage7bDurableStorageError::WriterAlreadyHeld)
        ));
        drop(owner);
        fs::remove_dir_all(parent).unwrap();
    }

    #[test]
    fn recovery_seal_canonical_roundtrip_and_restart() {
        let (parent, root, identity, owner) = first_boot();
        let (_, key, runtime) = stage6d_test_authenticated_restart_fixture();
        let committed = fs::read(root.join(STAGE7B_RECOVERY_SEAL_FILE)).unwrap();
        let decoded = Stage7bRecoverySealV1::decode_canonical(
            &committed,
            stage6d_operational_identity_sha256(&identity)
                .unwrap()
                .as_str(),
            &key,
        )
        .unwrap();
        assert_eq!(decoded.encode_canonical().unwrap(), committed);
        drop(owner);
        let restarted = Stage7bRecoveryReadyOwner::restart(
            Stage7bDurableRootAuthority::validate(&root, &identity).unwrap(),
            identity,
            &key,
            runtime,
        )
        .unwrap();
        assert!(matches!(restarted, Stage7bRestartOutcome::Ready(_)));
        drop(restarted);
        fs::remove_dir_all(parent).unwrap();
    }

    #[test]
    fn stage7b_e_production_authority_is_file_backed_and_single_owned() {
        let (parent, _root, _identity, owner) = first_boot();
        let recovered = owner.recovered().unwrap();
        assert!(recovered.journal_is_file_backed());
        assert!(owner.recovery_ready());
        drop(owner);
        fs::remove_dir_all(parent).unwrap();
    }

    #[test]
    fn recovery_seal_atomic_replace_and_orphan_temp_is_not_authority() {
        let (parent, root, identity, mut owner) = first_boot();
        let (_, key, runtime) = stage6d_test_authenticated_restart_fixture();
        let replacement = Stage7bRecoverySealV1::new(
            2,
            owner
                .committed_seal
                .stage6d_authenticated_restart_package
                .clone(),
            owner.committed_seal.stage6_checkpoint.clone(),
            owner.committed_seal.operational_identity_sha256.clone(),
            &key,
        )
        .unwrap();
        owner
            .writer_lease
            .commit_recovery_seal(&replacement)
            .unwrap();
        owner.committed_seal = replacement;
        fs::write(
            root.join(format!("{STAGE7B_RECOVERY_SEAL_TEMP_PREFIX}orphan.tmp")),
            b"not-authority",
        )
        .unwrap();
        drop(owner);
        let restarted = Stage7bRecoveryReadyOwner::restart(
            Stage7bDurableRootAuthority::validate(&root, &identity).unwrap(),
            identity,
            &key,
            runtime,
        )
        .unwrap();
        let Stage7bRestartOutcome::Ready(restarted) = restarted else {
            panic!("orphan temp must not replace committed seal");
        };
        assert_eq!(restarted.committed_seal().unwrap().seal_generation(), 2);
        drop(restarted);
        fs::remove_dir_all(parent).unwrap();
    }

    #[test]
    #[ignore]
    fn stage7b_d_a_crash_barrier_child() {
        let root = PathBuf::from(std::env::var_os("STAGE7B_D_A_CHILD_ROOT").unwrap());
        let marker = PathBuf::from(std::env::var_os("STAGE7B_D_A_CHILD_MARKER").unwrap());
        let phase = std::env::var("STAGE7B_D_A_CHILD_PHASE").unwrap();
        let effect_witness =
            PathBuf::from(std::env::var_os("STAGE7B_D_A_CHILD_EFFECT_WITNESS").unwrap());
        let identity = identity();
        let fixture =
            stage7b_test_authenticated_working_restart_fixture(Stage7bTestExtraStage6History::None);
        let Stage7bRestartOutcome::Ready(mut owner) = Stage7bRecoveryReadyOwner::restart(
            Stage7bDurableRootAuthority::validate(&root, &identity).unwrap(),
            identity,
            &fixture.commitment_key,
            fixture.fresh_runtime,
        )
        .unwrap() else {
            panic!("d-a child must restart from accepted-only fixture");
        };
        if phase == "accepted" {
            fs::write(&marker, b"request-accepted-durable").unwrap();
            loop {
                thread::sleep(Duration::from_secs(1));
            }
        }
        let observed_at = command_created_at(&fixture.command) + chrono::Duration::seconds(1);
        let Stage7aPaperAdmission::DispatchReady(receipt) = owner
            .admit_paper_command(&fixture.command, &fixture.command_context, observed_at)
            .unwrap()
        else {
            panic!("accepted-only child must append one dispatch attempt");
        };
        if phase == "dispatch" {
            fs::write(&marker, phase.as_bytes()).unwrap();
            loop {
                thread::sleep(Duration::from_secs(1));
            }
        }
        if phase == "during-effect" {
            commit_stage7b_d_a_provider_effect_witness(&effect_witness);
            fs::write(&marker, b"provider-effect-durable-before-outcome").unwrap();
            loop {
                thread::sleep(Duration::from_secs(1));
            }
        }
        let report = owner
            .record_paper_outcome(
                *receipt,
                Stage6dPaperOutcome::MarketFilled {
                    broker_order_id: BrokerOrderId::new("paper-order-stage7b-da-crash"),
                    broker_trade_id: BrokerTradeId::new("paper-trade-stage7b-da-crash"),
                },
            )
            .unwrap();
        if phase == "outcome" {
            fs::write(&marker, b"outcome-durable").unwrap();
            loop {
                thread::sleep(Duration::from_secs(1));
            }
        }
        let (_, finalized) = owner
            .finalize_paper_request(report, observed_at + chrono::Duration::seconds(1))
            .unwrap();
        if phase == "finalized" {
            fs::write(&marker, b"request-finalized-durable").unwrap();
            loop {
                thread::sleep(Duration::from_secs(1));
            }
        }
        let _authority = owner
            .authorize_finalized_ack(finalized, &fixture.commitment_key)
            .unwrap();
        assert_eq!(phase, "sealed");
        fs::write(&marker, b"covering-seal-durable").unwrap();
        loop {
            thread::sleep(Duration::from_secs(1));
        }
    }

    #[test]
    fn stage7b_d_a_b043_b049_b051_b055_b056_seals_before_ack_authority() {
        let setup = prepare_active_restart_prefix(1);
        let Stage7bRestartOutcome::Ready(mut owner) = Stage7bRecoveryReadyOwner::restart(
            Stage7bDurableRootAuthority::validate(&setup.root, &setup.identity).unwrap(),
            setup.identity.clone(),
            &setup.key,
            setup.runtime,
        )
        .unwrap() else {
            panic!("accepted-only source-bound crash state must restart ready");
        };
        let command = setup.command.clone();
        let request_id = request_id(&command);
        let observed_at = command_created_at(&command) + chrono::Duration::seconds(1);

        let Stage7aPaperAdmission::DispatchReady(receipt) = owner
            .admit_paper_command(&command, &setup.command_context, observed_at)
            .unwrap()
        else {
            panic!("new exact command must produce the fsync-backed dispatch receipt");
        };
        assert_eq!(
            owner.recovered().unwrap().journal_frontier().frame_count(),
            2,
            "accepted and dispatch records are both fsync-backed before effect authority"
        );
        assert_eq!(
            owner
                .recovered()
                .unwrap()
                .replay()
                .request(request_id)
                .unwrap()
                .last_unique_sequence(),
            2
        );
        assert!(
            !owner.recovery_ready(),
            "journal-ahead owner is not restart-ready"
        );
        let report = owner
            .record_paper_outcome(
                *receipt,
                Stage6dPaperOutcome::MarketFilled {
                    broker_order_id: BrokerOrderId::new("paper-order-stage7b-da-1"),
                    broker_trade_id: BrokerTradeId::new("paper-trade-stage7b-da-1"),
                },
            )
            .unwrap();
        let (_report, finalized) = owner
            .finalize_paper_request(report, observed_at + chrono::Duration::seconds(1))
            .unwrap();
        assert_eq!(owner.committed_seal().unwrap().seal_generation(), 1);
        assert!(!owner.recovery_ready());

        let initial_seal = owner.committed_seal().unwrap().clone();
        let mut globally_advanced_seal = initial_seal.clone();
        globally_advanced_seal.seal_generation += 1;
        globally_advanced_seal.seal_commitment_sha256 = "e".repeat(64);
        let initial_identity = durable_ack_authority(
            &initial_seal,
            owner.finalized_request(request_id).unwrap().facts,
        )
        .unwrap();
        let advanced_identity = durable_ack_authority(
            &globally_advanced_seal,
            owner.finalized_request(request_id).unwrap().facts,
        )
        .unwrap();
        assert_ne!(
            initial_identity.settlement_authority_fingerprint_sha256(),
            advanced_identity.settlement_authority_fingerprint_sha256(),
            "global seal advancement must rotate short-lived settlement authority"
        );
        assert_eq!(
            initial_identity.terminal_request_ack_identity_sha256(),
            advanced_identity.terminal_request_ack_identity_sha256(),
            "global seal advancement must not change finalized request identity"
        );
        drop(initial_identity);
        drop(advanced_identity);

        let authority = owner
            .authorize_finalized_ack(finalized, &setup.key)
            .unwrap();
        assert_eq!(authority.strategy_request_id(), request_id);
        assert_eq!(authority.seal_generation(), 2);
        assert_eq!(
            authority.classify_publication(None),
            Stage7bAckPublicationDecision::Canonical
        );
        assert_eq!(
            authority.classify_publication(Some(authority.terminal_request_ack_identity_sha256())),
            Stage7bAckPublicationDecision::Duplicate
        );
        assert_eq!(
            authority.classify_publication(Some(&"f".repeat(64))),
            Stage7bAckPublicationDecision::Conflict
        );
        assert!(owner.recovery_ready());
        let fingerprint = authority.terminal_request_ack_identity_sha256().to_string();
        drop(authority);
        drop(owner);

        let runtime =
            stage7b_test_authenticated_working_restart_fixture(Stage7bTestExtraStage6History::None)
                .fresh_runtime;
        let outcome = Stage7bRecoveryReadyOwner::restart(
            Stage7bDurableRootAuthority::validate(&setup.root, &setup.identity).unwrap(),
            setup.identity,
            &setup.key,
            runtime,
        )
        .unwrap();
        let Stage7bRestartOutcome::Ready(mut restarted) = outcome else {
            panic!("committed generation-two seal must restart ready");
        };
        let reconstructed = restarted.finalized_request(request_id).unwrap();
        let reconstructed_authority = restarted
            .authorize_finalized_ack(reconstructed, &setup.key)
            .unwrap();
        assert_eq!(reconstructed_authority.seal_generation(), 2);
        assert_eq!(
            reconstructed_authority.terminal_request_ack_identity_sha256(),
            fingerprint
        );
        assert_eq!(
            reconstructed_authority.classify_publication(None),
            Stage7bAckPublicationDecision::Canonical,
            "without durable Redis publication knowledge the recovered ACK stays canonical"
        );
        drop(reconstructed_authority);
        drop(restarted);
        fs::remove_dir_all(setup.parent).unwrap();
    }

    #[test]
    fn stage7b_d_a_b044_sigkill_after_accepted_recovers_dispatch_once() {
        let setup = prepare_active_restart_prefix(1);
        kill_stage7b_d_a_child_at(&setup, "accepted");
        let mut owner = restart_working_setup(&setup);
        let request_id = request_id(&setup.command);
        let observed_at = command_created_at(&setup.command) + chrono::Duration::seconds(1);
        let Stage7aPaperAdmission::DispatchReady(receipt) = owner
            .admit_paper_command(&setup.command, &setup.command_context, observed_at)
            .unwrap()
        else {
            panic!("accepted-only restart must append the missing dispatch once");
        };
        assert_eq!(
            owner.recovered().unwrap().journal_frontier().frame_count(),
            2
        );
        assert_eq!(owner.recovered().unwrap().replay().requests().len(), 1);
        assert_eq!(
            owner
                .recovered()
                .unwrap()
                .replay()
                .request(request_id)
                .unwrap()
                .last_unique_sequence(),
            2
        );
        drop(receipt);
        drop(owner);
        fs::remove_dir_all(setup.parent).unwrap();
    }

    fn assert_post_dispatch_crash_holds(phase: &str) {
        let setup = prepare_active_restart_prefix(1);
        let effect_witness = kill_stage7b_d_a_child_at(&setup, phase);
        if phase == "during-effect" {
            assert_eq!(fs::read(&effect_witness).unwrap(), b"1\n");
        } else {
            assert!(!effect_witness.exists());
        }
        let before = fs::read(setup.root.join(STAGE7B_JOURNAL_FILE)).unwrap();
        let mut owner = restart_working_setup(&setup);
        let observed_at = command_created_at(&setup.command) + chrono::Duration::seconds(2);
        let admission = owner
            .admit_paper_command(&setup.command, &setup.command_context, observed_at)
            .unwrap();
        assert!(matches!(
            admission,
            Stage7aPaperAdmission::Hold {
                reason: strategy_runtime_core::Stage7aPaperHoldReason::ReconciliationRequired,
                ..
            }
        ));
        assert_eq!(
            owner
                .recovered()
                .unwrap()
                .replay()
                .request(request_id(&setup.command))
                .unwrap()
                .dispatch_safety_state(),
            Stage6DispatchSafetyStateV1::ReconciliationRequired
        );
        assert_eq!(
            fs::read(setup.root.join(STAGE7B_JOURNAL_FILE)).unwrap(),
            before,
            "recovery hold must not append or reinvoke the provider"
        );
        if phase == "during-effect" {
            assert_eq!(
                fs::read(&effect_witness).unwrap(),
                b"1\n",
                "redelivery must not invoke the provider effect a second time"
            );
        }
        drop(owner);
        fs::remove_dir_all(setup.parent).unwrap();
    }

    #[test]
    fn stage7b_d_a_b045_sigkill_after_dispatch_never_blind_redispatches() {
        assert_post_dispatch_crash_holds("dispatch");
    }

    #[test]
    fn stage7b_d_a_b046_sigkill_during_unknown_effect_requires_reconciliation() {
        assert_post_dispatch_crash_holds("during-effect");
    }

    #[test]
    fn stage7b_d_a_b047_sigkill_after_outcome_reconstructs_finalization_and_ack() {
        let setup = prepare_active_restart_prefix(1);
        kill_stage7b_d_a_child_at(&setup, "outcome");
        let mut owner = restart_working_setup(&setup);
        let request_id = request_id(&setup.command);
        assert_eq!(
            owner
                .recovered()
                .unwrap()
                .replay()
                .request(request_id)
                .unwrap()
                .dispatch_safety_state(),
            Stage6DispatchSafetyStateV1::DispatchForbidden
        );
        let finalized = owner
            .finalize_replayed_paper_request(
                request_id,
                command_created_at(&setup.command) + chrono::Duration::seconds(2),
            )
            .unwrap();
        let authority = owner
            .authorize_finalized_ack(finalized, &setup.key)
            .unwrap();
        assert_eq!(authority.strategy_request_id(), request_id);
        assert_eq!(
            authority.broker_order_id(),
            Some(&BrokerOrderId::new("paper-order-stage7b-da-crash"))
        );
        assert_eq!(
            authority.classify_publication(None),
            Stage7bAckPublicationDecision::Canonical
        );
        drop(authority);
        drop(owner);
        fs::remove_dir_all(setup.parent).unwrap();
    }

    fn assert_post_finalization_restart_is_canonical(phase: &str, expected_generation: u64) {
        let setup = prepare_active_restart_prefix(1);
        kill_stage7b_d_a_child_at(&setup, phase);
        let mut owner = restart_working_setup(&setup);
        let request_id = request_id(&setup.command);
        let finalized = owner.finalized_request(request_id).unwrap();
        let authority = owner
            .authorize_finalized_ack(finalized, &setup.key)
            .unwrap();
        assert_eq!(authority.strategy_request_id(), request_id);
        assert_eq!(authority.seal_generation(), expected_generation);
        assert_eq!(
            authority.classify_publication(None),
            Stage7bAckPublicationDecision::Canonical
        );
        drop(authority);
        drop(owner);
        fs::remove_dir_all(setup.parent).unwrap();
    }

    fn prepare_current_finalized_seal_owner() -> (
        PreparedWorkingRestart,
        Stage7bRecoveryReadyOwner,
        StrategyRequestId,
    ) {
        let setup = prepare_active_restart_prefix(1);
        let mut owner = restart_working_setup(&setup);
        let request_id = request_id(&setup.command);
        let observed_at = command_created_at(&setup.command) + chrono::Duration::seconds(1);
        let Stage7aPaperAdmission::DispatchReady(receipt) = owner
            .admit_paper_command(&setup.command, &setup.command_context, observed_at)
            .unwrap()
        else {
            panic!("current-seal fault fixture must dispatch");
        };
        let report = owner
            .record_paper_outcome(
                *receipt,
                Stage6dPaperOutcome::MarketFilled {
                    broker_order_id: BrokerOrderId::new("paper-order-stage7b-da-disk-seal"),
                    broker_trade_id: BrokerTradeId::new("paper-trade-stage7b-da-disk-seal"),
                },
            )
            .unwrap();
        let (_, finalized) = owner
            .finalize_paper_request(report, observed_at + chrono::Duration::seconds(1))
            .unwrap();
        let authority = owner
            .authorize_finalized_ack(finalized, &setup.key)
            .unwrap();
        assert_eq!(authority.seal_generation(), 2);
        drop(authority);
        drop(owner);
        let owner = restart_working_setup(&setup);
        assert_eq!(owner.committed_seal().unwrap().seal_generation(), 2);
        (setup, owner, request_id)
    }

    struct Stage7bDbRedisServer {
        child: Child,
        url: String,
    }

    impl Stage7bDbRedisServer {
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
                .expect("redis-server is required for Stage 7B-d-b tests");
            let url = format!("redis://127.0.0.1:{port}/");
            for _ in 0..100 {
                if let Ok(client) = redis::Client::open(url.as_str()) {
                    if let Ok(mut connection) = ConnectionManager::new(client).await {
                        let pong: redis::RedisResult<String> =
                            redis::cmd("PING").query_async(&mut connection).await;
                        if pong.as_deref() == Ok("PONG") && child.try_wait().unwrap().is_none() {
                            return Self { child, url };
                        }
                    }
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            let _ = child.kill();
            let _ = child.wait();
            panic!("temporary Stage 7B-d-b Redis did not start")
        }
    }

    impl Drop for Stage7bDbRedisServer {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }

    #[tokio::test]
    async fn stage7b_d_b_b057_b062_owner_mediates_only_finalized_ack_settlement() {
        let (setup, mut owner, request_id) = prepare_current_finalized_seal_owner();
        let redis = Stage7bDbRedisServer::start().await;
        let mut inspector =
            ConnectionManager::new(redis::Client::open(redis.url.as_str()).unwrap())
                .await
                .unwrap();
        let source = "finam_imoexf_paper:{owner-mediated}:commands";
        let ack = "finam_imoexf_paper:{owner-mediated}:acks";
        let dlq = "finam_imoexf_paper:{owner-mediated}:dlq";
        let group = "stage7b-owner-mediated";
        let _: () = redis::cmd("XGROUP")
            .arg("CREATE")
            .arg(source)
            .arg(group)
            .arg("0-0")
            .arg("MKSTREAM")
            .query_async(&mut inspector)
            .await
            .unwrap();
        let _: String = redis::cmd("XADD")
            .arg(source)
            .arg("*")
            .arg("payload")
            .arg("redacted-command")
            .query_async(&mut inspector)
            .await
            .unwrap();
        let delivered: StreamReadReply = redis::cmd("XREADGROUP")
            .arg("GROUP")
            .arg(group)
            .arg("consumer-owner-mediated")
            .arg("COUNT")
            .arg(1)
            .arg("STREAMS")
            .arg(source)
            .arg(">")
            .query_async(&mut inspector)
            .await
            .unwrap();
        let context = Stage7bRedisSettlementContext::new(
            "owner-mediated",
            source,
            ack,
            dlq,
            group,
            delivered.keys[0].ids[0].id.clone(),
        )
        .unwrap();
        let finalized = owner.finalized_request(request_id).unwrap();
        let mut backend = Stage7bRedisSettlementBackend::connect(&redis.url)
            .await
            .unwrap();
        let outcome = owner
            .settle_finalized_ack(finalized, &setup.key, context, &mut backend)
            .await
            .unwrap();
        assert_eq!(outcome.classification, "canonical");
        let pending: StreamPendingCountReply = redis::cmd("XPENDING")
            .arg(source)
            .arg(group)
            .arg("-")
            .arg("+")
            .arg(10)
            .query_async(&mut inspector)
            .await
            .unwrap();
        assert!(pending.ids.is_empty());
        let ack_count: i64 = redis::cmd("XLEN")
            .arg(ack)
            .query_async(&mut inspector)
            .await
            .unwrap();
        assert_eq!(ack_count, 1);
        assert!(backend.healthy());
        drop(owner);
        fs::remove_dir_all(setup.parent).unwrap();
    }

    #[tokio::test]
    async fn stage7b_e_x12_sigkill_after_seal_before_redis_settlement_recovers_once() {
        let setup = prepare_active_restart_prefix(1);
        let redis = Stage7bDbRedisServer::start().await;
        let mut inspector =
            ConnectionManager::new(redis::Client::open(redis.url.as_str()).unwrap())
                .await
                .unwrap();
        let source = "finam_imoexf_paper:{stage7b-e-x12}:commands";
        let ack = "finam_imoexf_paper:{stage7b-e-x12}:acks";
        let dlq = "finam_imoexf_paper:{stage7b-e-x12}:dlq";
        let group = "stage7b-e-x12";
        let _: () = redis::cmd("XGROUP")
            .arg("CREATE")
            .arg(source)
            .arg(group)
            .arg("0-0")
            .arg("MKSTREAM")
            .query_async(&mut inspector)
            .await
            .unwrap();
        let _: String = redis::cmd("XADD")
            .arg(source)
            .arg("*")
            .arg("payload")
            .arg(stage7b_d_c_payload(&setup.command))
            .query_async(&mut inspector)
            .await
            .unwrap();
        let delivered: StreamReadReply = redis::cmd("XREADGROUP")
            .arg("GROUP")
            .arg(group)
            .arg("stage7b-e-x12-crashed-consumer")
            .arg("COUNT")
            .arg(1)
            .arg("STREAMS")
            .arg(source)
            .arg(">")
            .query_async(&mut inspector)
            .await
            .unwrap();
        let entry_id = delivered.keys[0].ids[0].id.clone();

        // The existing d-a child follows the production Stage 6 lifecycle and
        // pauses only after the covering recovery seal is durably committed and
        // reread. It has no Redis capability, so killing it at this barrier is
        // exactly before d-b Lua settlement.
        kill_stage7b_d_a_child_at(&setup, "sealed");
        let journal_after_crash = fs::read(setup.root.join(STAGE7B_JOURNAL_FILE)).unwrap();
        let pending: StreamPendingCountReply = redis::cmd("XPENDING")
            .arg(source)
            .arg(group)
            .arg("-")
            .arg("+")
            .arg(10)
            .query_async(&mut inspector)
            .await
            .unwrap();
        assert_eq!(pending.ids.len(), 1);
        assert_eq!(pending.ids[0].id, entry_id);
        let ack_count: i64 = redis::cmd("XLEN")
            .arg(ack)
            .query_async(&mut inspector)
            .await
            .unwrap();
        assert_eq!(ack_count, 0, "crashed process cannot publish an ACK");

        let fixture =
            stage7b_test_authenticated_working_restart_fixture(Stage7bTestExtraStage6History::None);
        let Stage7bRestartOutcome::Ready(owner) = Stage7bRecoveryReadyOwner::restart(
            Stage7bDurableRootAuthority::validate(&setup.root, &setup.identity).unwrap(),
            setup.identity.clone(),
            &fixture.commitment_key,
            fixture.fresh_runtime,
        )
        .unwrap() else {
            panic!("X12 finalized crash state must restart ready");
        };
        let mut owner = *owner;
        let finalized = owner.finalized_request(request_id(&setup.command)).unwrap();
        let context =
            Stage7bRedisSettlementContext::new("stage7b-e-x12", source, ack, dlq, group, entry_id)
                .unwrap();
        let mut backend = Stage7bRedisSettlementBackend::connect(&redis.url)
            .await
            .unwrap();
        let outcome = owner
            .settle_finalized_ack(finalized, &fixture.commitment_key, context, &mut backend)
            .await
            .unwrap();
        assert_eq!(outcome.classification, "canonical");
        assert_eq!(
            fs::read(setup.root.join(STAGE7B_JOURNAL_FILE)).unwrap(),
            journal_after_crash,
            "restart settlement cannot reinvoke provider or append a second effect"
        );
        let pending: StreamPendingCountReply = redis::cmd("XPENDING")
            .arg(source)
            .arg(group)
            .arg("-")
            .arg("+")
            .arg(10)
            .query_async(&mut inspector)
            .await
            .unwrap();
        assert!(pending.ids.is_empty());
        let acks: StreamRangeReply = redis::cmd("XRANGE")
            .arg(ack)
            .arg("-")
            .arg("+")
            .query_async(&mut inspector)
            .await
            .unwrap();
        assert_eq!(
            acks.ids.len(),
            1,
            "canonical ACK must be emitted exactly once"
        );
        let markers: Vec<String> = redis::cmd("KEYS")
            .arg("finam_imoexf_paper:{stage7b-e-x12}:stage7b:settlement:request:*")
            .query_async(&mut inspector)
            .await
            .unwrap();
        assert_eq!(markers.len(), 1);
        drop(owner);
        fs::remove_dir_all(setup.parent).unwrap();
    }

    fn assert_current_seal_drift_blocks_ack(
        setup: &PreparedWorkingRestart,
        owner: &mut Stage7bRecoveryReadyOwner,
        request_id: StrategyRequestId,
    ) {
        let finalized = owner.finalized_request(request_id).unwrap();
        assert!(matches!(
            owner.authorize_finalized_ack(finalized, &setup.key),
            Err(Stage7bRecoveryError::SealInvalid)
        ));
        assert!(!owner.recovery_ready());
        let reconstructed = owner.finalized_request(request_id).unwrap();
        assert!(matches!(
            owner.authorize_finalized_ack(reconstructed, &setup.key),
            Err(Stage7bRecoveryError::SealCommitUncertain)
        ));
    }

    #[test]
    fn stage7b_d_a_b048_sigkill_after_finalization_reconstructs_canonical_ack() {
        assert_post_finalization_restart_is_canonical("finalized", 2);
    }

    #[test]
    fn stage7b_d_a_b051_sigkill_after_seal_reconstructs_without_provider() {
        assert_post_finalization_restart_is_canonical("sealed", 2);
    }

    #[test]
    fn stage7b_d_a_r1_deleted_current_seal_blocks_ack_authority() {
        let (setup, mut owner, request_id) = prepare_current_finalized_seal_owner();
        fs::remove_file(setup.root.join(STAGE7B_RECOVERY_SEAL_FILE)).unwrap();
        File::open(&setup.root).unwrap().sync_all().unwrap();
        assert_current_seal_drift_blocks_ack(&setup, &mut owner, request_id);
        drop(owner);
        fs::remove_dir_all(setup.parent).unwrap();
    }

    #[test]
    fn stage7b_d_a_r1_corrupt_current_seal_blocks_ack_authority() {
        let (setup, mut owner, request_id) = prepare_current_finalized_seal_owner();
        let seal_path = setup.root.join(STAGE7B_RECOVERY_SEAL_FILE);
        fs::write(&seal_path, b"corrupt-current-seal").unwrap();
        File::open(&seal_path).unwrap().sync_all().unwrap();
        File::open(&setup.root).unwrap().sync_all().unwrap();
        assert_current_seal_drift_blocks_ack(&setup, &mut owner, request_id);
        drop(owner);
        fs::remove_dir_all(setup.parent).unwrap();
    }

    #[test]
    fn stage7b_d_a_r1_valid_but_different_current_seal_blocks_ack_authority() {
        let (setup, mut owner, request_id) = prepare_current_finalized_seal_owner();
        let cached = owner.committed_seal.clone();
        let replacement = Stage7bRecoverySealV1::new(
            cached.seal_generation() + 1,
            cached.stage6d_authenticated_restart_package.clone(),
            cached.stage6_checkpoint.clone(),
            cached.operational_identity_sha256.clone(),
            &setup.key,
        )
        .unwrap();
        owner
            .writer_lease
            .commit_recovery_seal(&replacement)
            .unwrap();
        assert_ne!(replacement, cached);
        assert_current_seal_drift_blocks_ack(&setup, &mut owner, request_id);
        drop(owner);
        fs::remove_dir_all(setup.parent).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn stage7b_d_a_b050_seal_failure_blocks_authorization_and_readiness() {
        use std::os::unix::fs::PermissionsExt;

        let setup = prepare_active_restart_prefix(1);
        let mut owner = restart_working_setup(&setup);
        let observed_at = command_created_at(&setup.command) + chrono::Duration::seconds(1);
        let Stage7aPaperAdmission::DispatchReady(receipt) = owner
            .admit_paper_command(&setup.command, &setup.command_context, observed_at)
            .unwrap()
        else {
            panic!("fault fixture must reach dispatch");
        };
        let report = owner
            .record_paper_outcome(
                *receipt,
                Stage6dPaperOutcome::MarketFilled {
                    broker_order_id: BrokerOrderId::new("paper-order-stage7b-da-seal-fault"),
                    broker_trade_id: BrokerTradeId::new("paper-trade-stage7b-da-seal-fault"),
                },
            )
            .unwrap();
        let (_, finalized) = owner
            .finalize_paper_request(report, observed_at + chrono::Duration::seconds(1))
            .unwrap();
        let original_permissions = fs::metadata(&setup.root).unwrap().permissions();
        fs::set_permissions(&setup.root, fs::Permissions::from_mode(0o500)).unwrap();
        let result = owner.authorize_finalized_ack(finalized, &setup.key);
        fs::set_permissions(&setup.root, original_permissions).unwrap();
        assert!(
            result.is_err(),
            "seal commit fault cannot mint ACK authority"
        );
        assert!(!owner.recovery_ready());
        let reconstructed = owner.finalized_request(request_id(&setup.command)).unwrap();
        assert!(matches!(
            owner.authorize_finalized_ack(reconstructed, &setup.key),
            Err(Stage7bRecoveryError::SealCommitUncertain)
        ));
        drop(owner);
        fs::remove_dir_all(setup.parent).unwrap();
    }

    #[test]
    fn stage7b_d_a_b054_sequential_cancel_survives_restart_and_reseals() {
        let setup =
            prepare_fixture_restart_prefix(stage7b_test_authenticated_cancel_restart_fixture(), 5);
        let Stage7bRestartOutcome::Ready(mut owner) = Stage7bRecoveryReadyOwner::restart(
            Stage7bDurableRootAuthority::validate(&setup.root, &setup.identity).unwrap(),
            setup.identity.clone(),
            &setup.key,
            setup.runtime,
        )
        .unwrap() else {
            panic!("source-bound historical PLACE plus accepted CANCEL must restart ready");
        };
        let cancel = match &setup.command {
            BrokerCommand::CancelOrder(cancel) => cancel.clone(),
            BrokerCommand::PlaceOrder(_) => panic!("Stage 7B cancel fixture action drift"),
        };
        let cancel_request_id = cancel.request_id;
        let observed_at = cancel.created_ts + chrono::Duration::seconds(1);
        let context = owner
            .resolve_cancel_command_context(&cancel, &instrument(), "hybrid_imoexf")
            .unwrap()
            .expect("cancel context must derive from Stage 6 working order truth");
        assert_eq!(context, setup.command_context);
        let Stage7aPaperAdmission::DispatchReady(receipt) = owner
            .admit_paper_command(&BrokerCommand::CancelOrder(cancel), &context, observed_at)
            .unwrap()
        else {
            panic!("correlated sequential cancel must dispatch");
        };
        let report = owner
            .record_paper_outcome(*receipt, Stage6dPaperOutcome::CancelCanceled)
            .unwrap();
        let (_, finalized) = owner
            .finalize_paper_request(report, observed_at + chrono::Duration::seconds(1))
            .unwrap();
        let cancel_authority = owner
            .authorize_finalized_ack(finalized, &setup.key)
            .unwrap();
        assert_eq!(cancel_authority.strategy_request_id(), cancel_request_id);
        assert_eq!(cancel_authority.seal_generation(), 2);
        assert!(owner.recovery_ready());
        drop(cancel_authority);
        drop(owner);
        fs::remove_dir_all(setup.parent).unwrap();
    }

    #[test]
    fn stage7b_c_b034_authenticated_checkpoint_ahead_of_file_journal_blocks() {
        let identity = identity();
        let (parent, root) = root(&identity);
        let fixture =
            stage7b_test_authenticated_working_restart_fixture(Stage7bTestExtraStage6History::None);
        let authorization = authorization(&identity, &fixture.fresh_runtime);
        let storage = Stage7bWritableDurableAuthority::create_new(
            Stage7bDurableRootAuthority::validate(&root, &identity).unwrap(),
            &identity,
            &authorization,
        )
        .unwrap();
        let mut ahead = Stage6MemoryJournalBackend::new();
        ahead.append(&fixture.journal_records[0]).unwrap();
        let ahead_checkpoint =
            Stage6JournalCheckpointV1::from_frontier(ahead.frontier().clone()).unwrap();
        let package = seal_stage6d_restart_package(
            &fixture.stage5g_authenticated_package,
            ahead_checkpoint.clone(),
            identity.clone(),
            &fixture.commitment_key,
        )
        .unwrap();
        let seal = Stage7bRecoverySealV1::new(
            1,
            package,
            ahead_checkpoint,
            stage6d_operational_identity_sha256(&identity)
                .unwrap()
                .as_str()
                .to_string(),
            &fixture.commitment_key,
        )
        .unwrap();
        storage._writer_lease.commit_recovery_seal(&seal).unwrap();
        let before = fs::read(root.join(STAGE7B_JOURNAL_FILE)).unwrap();
        drop(storage);
        let outcome = Stage7bRecoveryReadyOwner::restart(
            Stage7bDurableRootAuthority::validate(&root, &identity).unwrap(),
            identity,
            &fixture.commitment_key,
            fixture.fresh_runtime,
        )
        .unwrap();
        let Stage7bRestartOutcome::Blocked(blocked) = outcome else {
            panic!("ahead checkpoint must block Stage 7B recovery");
        };
        assert_eq!(
            blocked.reason(),
            Stage7bRecoveryBlockReason::CheckpointMismatch
        );
        assert!(!blocked.paper_provider_invocation_allowed());
        assert!(!blocked.redis_settlement_allowed());
        assert!(!blocked.xack_allowed());
        assert_eq!(fs::read(root.join(STAGE7B_JOURNAL_FILE)).unwrap(), before);
        drop(blocked);
        fs::remove_dir_all(parent).unwrap();
    }

    #[test]
    fn stage7b_c_b039_finalized_file_journal_ahead_restarts_ready() {
        let setup = prepare_working_restart(Stage7bTestExtraStage6History::Finalized);
        let outcome = Stage7bRecoveryReadyOwner::restart(
            Stage7bDurableRootAuthority::validate(&setup.root, &setup.identity).unwrap(),
            setup.identity.clone(),
            &setup.key,
            setup.runtime,
        )
        .unwrap();
        let Stage7bRestartOutcome::Ready(owner) = outcome else {
            panic!("finalized journal suffix must remain restart-safe");
        };
        let recovered = owner.recovered().unwrap();
        assert_eq!(recovered.replay().requests().len(), 2);
        assert_eq!(
            recovered.active_cross_bound_request_ids()[0].to_string(),
            setup.active_request_id
        );
        assert_eq!(
            recovered
                .replay()
                .requests()
                .iter()
                .filter(|request| request.final_disposition().is_some())
                .count(),
            1
        );
        assert_eq!(
            owner
                .committed_seal()
                .unwrap()
                .stage6_checkpoint()
                .frontier()
                .frame_count(),
            0
        );
        assert_eq!(recovered.journal_frontier().frame_count(), 4);
        assert_eq!(
            fs::read(setup.root.join(STAGE7B_JOURNAL_FILE)).unwrap(),
            setup.journal_before_restart
        );
        assert!(matches!(
            Stage7bWritableDurableAuthority::open_existing(
                Stage7bDurableRootAuthority::validate(&setup.root, &setup.identity).unwrap(),
                &setup.identity,
            ),
            Err(Stage7bDurableStorageError::WriterAlreadyHeld)
        ));
        drop(owner);
        fs::remove_dir_all(setup.parent).unwrap();
    }

    #[test]
    fn stage7b_c_b040_unbound_nonfinal_file_journal_blocks_without_effect() {
        let setup = prepare_working_restart(Stage7bTestExtraStage6History::UnboundNonFinal);
        let outcome = Stage7bRecoveryReadyOwner::restart(
            Stage7bDurableRootAuthority::validate(&setup.root, &setup.identity).unwrap(),
            setup.identity.clone(),
            &setup.key,
            setup.runtime,
        )
        .unwrap();
        let Stage7bRestartOutcome::Blocked(blocked) = outcome else {
            panic!("unbound non-final Stage 6 request must block Stage 7B recovery");
        };
        assert_eq!(
            blocked.reason(),
            Stage7bRecoveryBlockReason::AuthenticatedRestartRejected
        );
        assert!(!blocked.recovery_ready());
        assert!(!blocked.paper_provider_invocation_allowed());
        assert!(!blocked.redis_settlement_allowed());
        assert!(!blocked.xack_allowed());
        assert_eq!(
            fs::read(setup.root.join(STAGE7B_JOURNAL_FILE)).unwrap(),
            setup.journal_before_restart
        );
        drop(blocked);
        fs::remove_dir_all(setup.parent).unwrap();
    }

    #[test]
    fn stage7b_c_b041_cross_bound_active_file_journal_preserves_dispatch_safety() {
        let setup = prepare_working_restart(Stage7bTestExtraStage6History::None);
        let outcome = Stage7bRecoveryReadyOwner::restart(
            Stage7bDurableRootAuthority::validate(&setup.root, &setup.identity).unwrap(),
            setup.identity.clone(),
            &setup.key,
            setup.runtime,
        )
        .unwrap();
        let Stage7bRestartOutcome::Ready(owner) = outcome else {
            panic!("matching active Stage 5G/Stage 6 request must recover Ready");
        };
        let recovered = owner.recovered().unwrap();
        assert_eq!(
            recovered.active_cross_bound_request_ids()[0].to_string(),
            setup.active_request_id
        );
        let active_request_id = recovered.active_cross_bound_request_ids()[0];
        assert_eq!(
            recovered
                .replay()
                .request(active_request_id)
                .unwrap()
                .dispatch_safety_state(),
            Stage6DispatchSafetyStateV1::ReconciliationRequired
        );
        assert!(matches!(
            Stage7bWritableDurableAuthority::open_existing(
                Stage7bDurableRootAuthority::validate(&setup.root, &setup.identity).unwrap(),
                &setup.identity,
            ),
            Err(Stage7bDurableStorageError::WriterAlreadyHeld)
        ));
        drop(owner);
        fs::remove_dir_all(setup.parent).unwrap();
    }

    #[test]
    #[ignore]
    fn stage7b_c_b032_pre_rename_crash_barrier_child() {
        let root = PathBuf::from(std::env::var_os("STAGE7B_C_CHILD_ROOT").unwrap());
        let marker = PathBuf::from(std::env::var_os("STAGE7B_C_CHILD_MARKER").unwrap());
        let identity = identity();
        let (_, key, runtime) = stage6d_test_authenticated_restart_fixture();
        let outcome = Stage7bRecoveryReadyOwner::restart(
            Stage7bDurableRootAuthority::validate(&root, &identity).unwrap(),
            identity,
            &key,
            runtime,
        )
        .unwrap();
        let Stage7bRestartOutcome::Ready(owner) = outcome else {
            panic!("child must recover generation one before replacement");
        };
        let replacement = Stage7bRecoverySealV1::new(
            2,
            owner
                .committed_seal
                .stage6d_authenticated_restart_package
                .clone(),
            owner.committed_seal.stage6_checkpoint.clone(),
            owner.committed_seal.operational_identity_sha256.clone(),
            &key,
        )
        .unwrap();
        owner
            .writer_lease
            .commit_recovery_seal_with_pre_rename_observer(&replacement, || {
                fs::write(&marker, b"temp-synced-before-rename").unwrap();
                loop {
                    thread::sleep(Duration::from_secs(1));
                }
            })
            .unwrap();
    }

    #[test]
    fn stage7b_c_b032_sigkill_after_temp_sync_keeps_old_committed_seal() {
        let (parent, root, identity, owner) = first_boot();
        assert_eq!(owner.committed_seal().unwrap().seal_generation(), 1);
        let old_seal = fs::read(root.join(STAGE7B_RECOVERY_SEAL_FILE)).unwrap();
        drop(owner);
        let marker = parent.join("seal-temp-synced");
        let mut child = Command::new(std::env::current_exe().unwrap())
            .arg("--ignored")
            .arg("--exact")
            .arg("recovery::tests::stage7b_c_b032_pre_rename_crash_barrier_child")
            .arg("--nocapture")
            .env("STAGE7B_C_CHILD_ROOT", &root)
            .env("STAGE7B_C_CHILD_MARKER", &marker)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        wait_for_child(&mut child, &marker);
        child.kill().unwrap();
        child.wait().unwrap();
        assert_eq!(
            fs::read(root.join(STAGE7B_RECOVERY_SEAL_FILE)).unwrap(),
            old_seal
        );
        assert!(fs::read_dir(&root).unwrap().any(|entry| {
            entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(STAGE7B_RECOVERY_SEAL_TEMP_PREFIX)
        }));
        let (_, key, runtime) = stage6d_test_authenticated_restart_fixture();
        let restarted = Stage7bRecoveryReadyOwner::restart(
            Stage7bDurableRootAuthority::validate(&root, &identity).unwrap(),
            identity,
            &key,
            runtime,
        )
        .unwrap();
        let Stage7bRestartOutcome::Ready(restarted) = restarted else {
            panic!("old committed seal must survive child crash before rename");
        };
        assert_eq!(restarted.committed_seal().unwrap().seal_generation(), 1);
        drop(restarted);
        fs::remove_dir_all(parent).unwrap();
    }

    #[test]
    fn corrupt_recovery_seal_rejected_and_blocked_has_zero_effect() {
        let (parent, root, identity, owner) = first_boot();
        drop(owner);
        fs::write(root.join(STAGE7B_RECOVERY_SEAL_FILE), b"corrupt").unwrap();
        let (_, key, runtime) = stage6d_test_authenticated_restart_fixture();
        let outcome = Stage7bRecoveryReadyOwner::restart(
            Stage7bDurableRootAuthority::validate(&root, &identity).unwrap(),
            identity,
            &key,
            runtime,
        )
        .unwrap();
        let Stage7bRestartOutcome::Blocked(blocked) = outcome else {
            panic!("corrupt seal must block recovery");
        };
        assert_eq!(
            blocked.reason(),
            Stage7bRecoveryBlockReason::CorruptCommittedSeal
        );
        assert!(!blocked.recovery_ready());
        assert!(!blocked.paper_provider_invocation_allowed());
        assert!(!blocked.redis_settlement_allowed());
        assert!(!blocked.xack_allowed());
        drop(blocked);
        fs::remove_dir_all(parent).unwrap();
    }

    #[test]
    fn seal_without_journal_rejected_without_creating_journal() {
        let identity = identity();
        let (parent, root) = root(&identity);
        fs::write(root.join(STAGE7B_RECOVERY_SEAL_FILE), b"seal").unwrap();
        let (_, key, runtime) = stage6d_test_authenticated_restart_fixture();
        let result = Stage7bRecoveryReadyOwner::restart(
            Stage7bDurableRootAuthority::validate(&root, &identity).unwrap(),
            identity,
            &key,
            runtime,
        );
        assert!(matches!(
            result,
            Err(Stage7bRecoveryError::SealWithoutJournal)
        ));
        assert!(!root.join(STAGE7B_JOURNAL_FILE).exists());
        fs::remove_dir_all(parent).unwrap();
    }

    #[test]
    fn journal_without_seal_is_explicit_recovery_blocked() {
        let identity = identity();
        let (parent, root) = root(&identity);
        let (_, key, runtime) = stage6d_test_authenticated_restart_fixture();
        let authorization = authorization(&identity, &runtime);
        drop(
            Stage7bWritableDurableAuthority::create_new(
                Stage7bDurableRootAuthority::validate(&root, &identity).unwrap(),
                &identity,
                &authorization,
            )
            .unwrap(),
        );
        let outcome = Stage7bRecoveryReadyOwner::restart(
            Stage7bDurableRootAuthority::validate(&root, &identity).unwrap(),
            identity,
            &key,
            runtime,
        )
        .unwrap();
        let Stage7bRestartOutcome::Blocked(blocked) = outcome else {
            panic!("journal without seal must block");
        };
        assert_eq!(
            blocked.reason(),
            Stage7bRecoveryBlockReason::MissingCommittedSeal
        );
        drop(blocked);
        fs::remove_dir_all(parent).unwrap();
    }

    #[test]
    fn recovery_operational_identity_mismatch_is_blocked() {
        let (parent, root, identity, owner) = first_boot();
        drop(owner);
        let (_, key, runtime) = stage6d_test_authenticated_restart_fixture();
        let path = root.join(STAGE7B_RECOVERY_SEAL_FILE);
        let mut seal: Stage7bRecoverySealV1 =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        seal.operational_identity_sha256 = "2".repeat(64);
        seal.seal_commitment_sha256 = seal_commitment_sha256(
            seal.seal_generation,
            seal.created_at_ts_utc_ms,
            &seal.stage6d_restart_package_sha256,
            &seal.stage6_checkpoint_bytes_sha256,
            &seal.operational_identity_sha256,
        )
        .unwrap();
        seal.seal_commitment_hmac_sha256 =
            key.stage7b_recovery_seal_hmac_sha256(&seal.seal_commitment_sha256);
        fs::write(&path, seal.encode_canonical().unwrap()).unwrap();
        let outcome = Stage7bRecoveryReadyOwner::restart(
            Stage7bDurableRootAuthority::validate(&root, &identity).unwrap(),
            identity,
            &key,
            runtime,
        )
        .unwrap();
        assert!(matches!(outcome, Stage7bRestartOutcome::Blocked(_)));
        drop(outcome);
        fs::remove_dir_all(parent).unwrap();
    }

    #[test]
    fn recovery_hmac_digest_mismatch_is_blocked() {
        let (parent, root, identity, owner) = first_boot();
        drop(owner);
        let (_, key, runtime) = stage6d_test_authenticated_restart_fixture();
        let path = root.join(STAGE7B_RECOVERY_SEAL_FILE);
        let mut seal: Stage7bRecoverySealV1 =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        seal.stage6d_authenticated_restart_package[0] ^= 1;
        seal.stage6d_restart_package_sha256 =
            sha256_hex(&seal.stage6d_authenticated_restart_package);
        seal.seal_commitment_sha256 = seal_commitment_sha256(
            seal.seal_generation,
            seal.created_at_ts_utc_ms,
            &seal.stage6d_restart_package_sha256,
            &seal.stage6_checkpoint_bytes_sha256,
            &seal.operational_identity_sha256,
        )
        .unwrap();
        seal.seal_commitment_hmac_sha256 =
            key.stage7b_recovery_seal_hmac_sha256(&seal.seal_commitment_sha256);
        fs::write(&path, seal.encode_canonical().unwrap()).unwrap();
        let outcome = Stage7bRecoveryReadyOwner::restart(
            Stage7bDurableRootAuthority::validate(&root, &identity).unwrap(),
            identity,
            &key,
            runtime,
        )
        .unwrap();
        let Stage7bRestartOutcome::Blocked(blocked) = outcome else {
            panic!("mutated authenticated package must block");
        };
        assert_eq!(
            blocked.reason(),
            Stage7bRecoveryBlockReason::AuthenticatedRestartRejected
        );
        drop(blocked);
        fs::remove_dir_all(parent).unwrap();
    }

    #[derive(Clone)]
    struct Stage7bDcProvider {
        calls: Arc<AtomicUsize>,
    }

    impl Stage7aPaperOutcomeProvider for Stage7bDcProvider {
        fn paper_outcome(
            &mut self,
            command: &BrokerCommand,
            _observed_at: DateTime<Utc>,
        ) -> Result<Stage6dPaperOutcome, Stage7aPaperProviderError> {
            self.calls.fetch_add(1, AtomicOrdering::SeqCst);
            Ok(match command {
                BrokerCommand::PlaceOrder(_) => Stage6dPaperOutcome::MarketFilled {
                    broker_order_id: BrokerOrderId::new("paper-stage7b-dc-order"),
                    broker_trade_id: BrokerTradeId::new("paper-stage7b-dc-trade"),
                },
                BrokerCommand::CancelOrder(_) => Stage6dPaperOutcome::CancelCanceled,
            })
        }
    }

    fn stage7b_d_c_payload(command: &BrokerCommand) -> String {
        serde_json::to_string(&Envelope {
            schema_version: SCHEMA_VERSION,
            ts_utc: Utc::now(),
            source: "stage7b-d-c-test".to_string(),
            msg_type: MessageType::Command,
            payload: command,
        })
        .unwrap()
    }

    fn stage7b_d_c_profile(command: &BrokerCommand) -> Stage7aCommandProfile {
        let account = match command {
            BrokerCommand::PlaceOrder(command) => command.account_id.clone(),
            BrokerCommand::CancelOrder(command) => command.account_id.clone(),
        };
        Stage7aCommandProfile::new(account, instrument(), "hybrid_imoexf").unwrap()
    }

    async fn stage7b_d_c_xadd(
        connection: &mut ConnectionManager,
        stream: &str,
        payload: &str,
    ) -> String {
        redis::cmd("XADD")
            .arg(stream)
            .arg("*")
            .arg("payload")
            .arg(payload)
            .query_async(connection)
            .await
            .unwrap()
    }

    fn stage7b_d_c_fresh_request(mut command: BrokerCommand) -> BrokerCommand {
        let fresh = StrategyRequestId::new(uuid::Uuid::new_v4());
        match &mut command {
            BrokerCommand::PlaceOrder(command) => {
                command.request_id = fresh;
                command.client_order_id = broker_core::ClientOrderId::from_strategy_request(fresh);
            }
            BrokerCommand::CancelOrder(command) => {
                command.request_id = fresh;
                command.client_order_id =
                    Some(broker_core::ClientOrderId::from_strategy_request(fresh));
            }
        }
        command
    }

    async fn stage7b_d_c_single_request_marker(
        connection: &mut ConnectionManager,
        config: &Stage7bRedisServiceConfig,
    ) -> (String, String) {
        let keys: Vec<String> = redis::cmd("KEYS")
            .arg(format!(
                "finam_imoexf_paper:{{{}}}:stage7b:settlement:request:*",
                config.hash_tag
            ))
            .query_async(connection)
            .await
            .unwrap();
        assert_eq!(keys.len(), 1, "expected exactly one request marker");
        let value: String = redis::cmd("GET")
            .arg(&keys[0])
            .query_async(connection)
            .await
            .unwrap();
        (keys[0].clone(), value)
    }

    #[tokio::test]
    async fn stage7b_d_c_b052_b053_b068_b069_restart_and_old_pel() {
        let setup = prepare_active_restart_prefix(1);
        let owner = restart_working_setup(&setup);
        let redis = Stage7bDbRedisServer::start().await;
        let mut inspector =
            ConnectionManager::new(redis::Client::open(redis.url.as_str()).unwrap())
                .await
                .unwrap();
        let mut first_config = Stage7bRedisServiceConfig::paper_default_auto("dc-restart").unwrap();
        first_config.block_ms = 0;
        first_config.claim_idle_ms = 1;
        first_config.claim_count = 1;
        first_config.max_claim_pages = 1;
        let _: () = redis::cmd("XGROUP")
            .arg("CREATE")
            .arg(&first_config.command_stream)
            .arg(&first_config.consumer_group)
            .arg("0-0")
            .arg("MKSTREAM")
            .query_async(&mut inspector)
            .await
            .unwrap();
        let payload = stage7b_d_c_payload(&setup.command);
        let mut source_entry_ids = Vec::new();
        for _ in 0..3 {
            source_entry_ids.push(
                stage7b_d_c_xadd(&mut inspector, &first_config.command_stream, &payload).await,
            );
        }
        let old_delivery: StreamReadReply = redis::cmd("XREADGROUP")
            .arg("GROUP")
            .arg(&first_config.consumer_group)
            .arg("stage7b-dead-boot")
            .arg("COUNT")
            .arg(3)
            .arg("STREAMS")
            .arg(&first_config.command_stream)
            .arg(">")
            .query_async(&mut inspector)
            .await
            .unwrap();
        assert_eq!(
            old_delivery.keys[0]
                .ids
                .iter()
                .map(|entry| entry.id.clone())
                .collect::<Vec<_>>(),
            source_entry_ids
        );
        tokio::time::sleep(Duration::from_millis(5)).await;

        let calls = Arc::new(AtomicUsize::new(0));
        let profile = stage7b_d_c_profile(&setup.command);
        let first_consumer = first_config.consumer_name.clone();
        let mut service = Stage7bRedisService::connect(
            &redis.url,
            first_config.clone(),
            owner,
            setup.key,
            profile.clone(),
            Stage7bDcProvider {
                calls: calls.clone(),
            },
        )
        .await
        .unwrap();
        let first_run = service.run_bounded(3).await.unwrap();
        assert_eq!(first_run.reclaimed_entries_examined, 3);
        assert_eq!(service.claim_cursor(), "0-0");
        assert_eq!(calls.load(AtomicOrdering::SeqCst), 1);
        let journal_after_first = fs::read(setup.root.join(STAGE7B_JOURNAL_FILE)).unwrap();
        drop(service);

        let key2 =
            stage7b_test_authenticated_working_restart_fixture(Stage7bTestExtraStage6History::None)
                .commitment_key;
        let Stage7bRestartOutcome::Ready(owner2) = Stage7bRecoveryReadyOwner::restart(
            Stage7bDurableRootAuthority::validate(&setup.root, &setup.identity).unwrap(),
            setup.identity.clone(),
            &key2,
            setup.runtime.clone(),
        )
        .unwrap() else {
            panic!("finalized d-c command must restart ready");
        };
        let mut second_config =
            Stage7bRedisServiceConfig::paper_default_auto("dc-restart").unwrap();
        second_config.block_ms = 0;
        second_config.claim_idle_ms = 1;
        second_config.max_claim_pages = 1;
        assert_ne!(first_consumer, second_config.consumer_name);
        stage7b_d_c_xadd(&mut inspector, &second_config.command_stream, &payload).await;
        let mut second = Stage7bRedisService::connect(
            &redis.url,
            second_config.clone(),
            *owner2,
            key2,
            profile.clone(),
            Stage7bDcProvider {
                calls: calls.clone(),
            },
        )
        .await
        .unwrap();
        second.run_bounded(1).await.unwrap();
        assert_eq!(calls.load(AtomicOrdering::SeqCst), 1);
        assert_eq!(
            fs::read(setup.root.join(STAGE7B_JOURNAL_FILE)).unwrap(),
            journal_after_first,
            "exact restart duplicate must not append a second Stage 6 effect"
        );
        drop(second);

        let key3 =
            stage7b_test_authenticated_working_restart_fixture(Stage7bTestExtraStage6History::None)
                .commitment_key;
        let Stage7bRestartOutcome::Ready(owner3) = Stage7bRecoveryReadyOwner::restart(
            Stage7bDurableRootAuthority::validate(&setup.root, &setup.identity).unwrap(),
            setup.identity.clone(),
            &key3,
            setup.runtime.clone(),
        )
        .unwrap() else {
            panic!("exact duplicate settlement must retain restart readiness");
        };
        let mut conflicting = setup.command.clone();
        match &mut conflicting {
            BrokerCommand::PlaceOrder(command) => command.qty += command.qty,
            BrokerCommand::CancelOrder(command) => {
                command.order_id = BrokerOrderId::new("changed-target")
            }
        }
        let conflicting_payload = stage7b_d_c_payload(&conflicting);
        stage7b_d_c_xadd(
            &mut inspector,
            &second_config.command_stream,
            &conflicting_payload,
        )
        .await;
        let mut third_config = Stage7bRedisServiceConfig::paper_default_auto("dc-restart").unwrap();
        third_config.block_ms = 0;
        third_config.claim_idle_ms = 1;
        third_config.max_claim_pages = 1;
        let mut third = Stage7bRedisService::connect(
            &redis.url,
            third_config,
            *owner3,
            key3,
            profile,
            Stage7bDcProvider {
                calls: calls.clone(),
            },
        )
        .await
        .unwrap();
        third.run_bounded(1).await.unwrap();
        assert_eq!(calls.load(AtomicOrdering::SeqCst), 1);
        let pending: StreamPendingCountReply = redis::cmd("XPENDING")
            .arg(&second_config.command_stream)
            .arg(&second_config.consumer_group)
            .arg("-")
            .arg("+")
            .arg(10)
            .query_async(&mut inspector)
            .await
            .unwrap();
        assert_eq!(pending.ids.len(), 1);
        let ack_count: i64 = redis::cmd("XLEN")
            .arg(&second_config.ack_stream)
            .query_async(&mut inspector)
            .await
            .unwrap();
        assert_eq!(ack_count, 4, "conflict cannot publish or XACK");
        let (_, readiness) = third.supervisor().snapshots(
            Utc::now(),
            chrono::Duration::milliseconds(second_config.freshness_ms),
        );
        assert!(readiness
            .reasons
            .contains(&Stage7bPaperReadinessReason::CommandLifecycleBlocked));
        drop(third);
        fs::remove_dir_all(setup.parent).unwrap();
    }

    async fn assert_stage7b_d_c_r1_deterministic_rejection(
        hash_tag: &str,
        mutate_command: impl FnOnce(&mut BrokerCommand),
        mismatch_profile: bool,
        expected_status: &str,
        expected_reason: &str,
        expected_class: &str,
    ) {
        let setup = prepare_active_restart_prefix(1);
        let owner = restart_working_setup(&setup);
        let journal_before = fs::read(setup.root.join(STAGE7B_JOURNAL_FILE)).unwrap();
        let redis = Stage7bDbRedisServer::start().await;
        let mut inspector =
            ConnectionManager::new(redis::Client::open(redis.url.as_str()).unwrap())
                .await
                .unwrap();
        let mut config = Stage7bRedisServiceConfig::paper_default_auto(hash_tag).unwrap();
        config.block_ms = 0;
        config.claim_idle_ms = 1;
        let calls = Arc::new(AtomicUsize::new(0));
        let profile = if mismatch_profile {
            Stage7aCommandProfile::new(
                BrokerAccountId::new("ACC_PROFILE_MISMATCH"),
                instrument(),
                "hybrid_imoexf",
            )
            .unwrap()
        } else {
            stage7b_d_c_profile(&setup.command)
        };
        let mut command = stage7b_d_c_fresh_request(setup.command.clone());
        mutate_command(&mut command);
        let mut service = Stage7bRedisService::connect(
            &redis.url,
            config.clone(),
            owner,
            setup.key,
            profile,
            Stage7bDcProvider {
                calls: calls.clone(),
            },
        )
        .await
        .unwrap();
        service.ensure_group().await.unwrap();
        stage7b_d_c_xadd(
            &mut inspector,
            &config.command_stream,
            &stage7b_d_c_payload(&command),
        )
        .await;
        service.run_bounded(1).await.unwrap();

        assert_eq!(calls.load(AtomicOrdering::SeqCst), 0);
        assert_eq!(
            fs::read(setup.root.join(STAGE7B_JOURNAL_FILE)).unwrap(),
            journal_before,
            "deterministic pre-Stage6 rejection must not mutate the journal"
        );
        let pending: StreamPendingCountReply = redis::cmd("XPENDING")
            .arg(&config.command_stream)
            .arg(&config.consumer_group)
            .arg("-")
            .arg("+")
            .arg(10)
            .query_async(&mut inspector)
            .await
            .unwrap();
        assert!(pending.ids.is_empty());
        let acks: StreamRangeReply = redis::cmd("XRANGE")
            .arg(&config.ack_stream)
            .arg("-")
            .arg("+")
            .query_async(&mut inspector)
            .await
            .unwrap();
        assert_eq!(acks.ids.len(), 1);
        let payload = acks.ids[0].get::<String>("payload").unwrap();
        let payload: serde_json::Value = serde_json::from_str(&payload).unwrap();
        assert_eq!(payload["status"], expected_status);
        assert_eq!(payload["reason_code"], expected_reason);
        assert_eq!(payload["rejection_class"], expected_class);
        assert_eq!(payload["stage6_mutation"], false);
        assert_eq!(payload["publication"], "canonical");
        assert_eq!(payload["broker_order_id"], serde_json::Value::Null);
        drop(service);
        fs::remove_dir_all(setup.parent).unwrap();
    }

    #[tokio::test]
    async fn stage7b_d_c_r1_deterministic_rejections_ack_without_stage6_mutation() {
        assert_stage7b_d_c_r1_deterministic_rejection(
            "dc-r1-expired",
            |command| match command {
                BrokerCommand::PlaceOrder(command) => {
                    command.created_ts = Utc::now() - chrono::Duration::seconds(10);
                    command.ttl_ms = Some(1);
                }
                BrokerCommand::CancelOrder(command) => {
                    command.created_ts = Utc::now() - chrono::Duration::seconds(10);
                    command.ttl_ms = Some(1);
                }
            },
            false,
            "Expired",
            "expired_command",
            "expired",
        )
        .await;
        assert_stage7b_d_c_r1_deterministic_rejection(
            "dc-r1-unsupported",
            |command| match command {
                BrokerCommand::PlaceOrder(command) => command.qty -= command.qty,
                BrokerCommand::CancelOrder(_) => panic!("working fixture must be a place order"),
            },
            false,
            "Rejected",
            "feature_disabled",
            "unsupported_command_shape",
        )
        .await;
        assert_stage7b_d_c_r1_deterministic_rejection(
            "dc-r1-profile",
            |_| {},
            true,
            "Rejected",
            "local_validation_rejected",
            "command_profile_mismatch",
        )
        .await;
    }

    #[tokio::test]
    async fn stage7b_d_c_r1_rejection_restart_is_idempotent_and_established_conflict_stays_pending()
    {
        let setup = prepare_active_restart_prefix(1);
        let owner = restart_working_setup(&setup);
        let journal_before = fs::read(setup.root.join(STAGE7B_JOURNAL_FILE)).unwrap();
        let redis = Stage7bDbRedisServer::start().await;
        let mut inspector =
            ConnectionManager::new(redis::Client::open(redis.url.as_str()).unwrap())
                .await
                .unwrap();
        let mut command = stage7b_d_c_fresh_request(setup.command.clone());
        match &mut command {
            BrokerCommand::PlaceOrder(command) => {
                command.created_ts = Utc::now() - chrono::Duration::seconds(10);
                command.ttl_ms = Some(1);
            }
            BrokerCommand::CancelOrder(command) => {
                command.created_ts = Utc::now() - chrono::Duration::seconds(10);
                command.ttl_ms = Some(1);
            }
        }
        let payload = stage7b_d_c_payload(&command);
        let mut first_config =
            Stage7bRedisServiceConfig::paper_default_auto("dc-r1-duplicate").unwrap();
        first_config.block_ms = 0;
        first_config.claim_idle_ms = 1;
        let calls = Arc::new(AtomicUsize::new(0));
        let mut first = Stage7bRedisService::connect(
            &redis.url,
            first_config.clone(),
            owner,
            setup.key,
            stage7b_d_c_profile(&setup.command),
            Stage7bDcProvider {
                calls: calls.clone(),
            },
        )
        .await
        .unwrap();
        first.ensure_group().await.unwrap();
        stage7b_d_c_xadd(&mut inspector, &first_config.command_stream, &payload).await;
        first.run_bounded(1).await.unwrap();
        let marker_before = stage7b_d_c_single_request_marker(&mut inspector, &first_config).await;
        drop(first);

        let key2 =
            stage7b_test_authenticated_working_restart_fixture(Stage7bTestExtraStage6History::None)
                .commitment_key;
        let Stage7bRestartOutcome::Ready(owner) = Stage7bRecoveryReadyOwner::restart(
            Stage7bDurableRootAuthority::validate(&setup.root, &setup.identity).unwrap(),
            setup.identity.clone(),
            &key2,
            setup.runtime.clone(),
        )
        .unwrap() else {
            panic!("deterministic rejection must retain restart readiness");
        };
        let mut second_config =
            Stage7bRedisServiceConfig::paper_default_auto("dc-r1-duplicate").unwrap();
        second_config.block_ms = 0;
        second_config.claim_idle_ms = 1;
        stage7b_d_c_xadd(&mut inspector, &second_config.command_stream, &payload).await;
        let mut second = Stage7bRedisService::connect(
            &redis.url,
            second_config.clone(),
            *owner,
            key2,
            stage7b_d_c_profile(&setup.command),
            Stage7bDcProvider {
                calls: calls.clone(),
            },
        )
        .await
        .unwrap();
        second.run_bounded(1).await.unwrap();
        assert_eq!(calls.load(AtomicOrdering::SeqCst), 0);
        assert_eq!(
            fs::read(setup.root.join(STAGE7B_JOURNAL_FILE)).unwrap(),
            journal_before
        );
        let acks: StreamRangeReply = redis::cmd("XRANGE")
            .arg(&second_config.ack_stream)
            .arg("-")
            .arg("+")
            .query_async(&mut inspector)
            .await
            .unwrap();
        assert_eq!(acks.ids.len(), 2);
        let duplicate = acks.ids[1].get::<String>("payload").unwrap();
        let duplicate: serde_json::Value = serde_json::from_str(&duplicate).unwrap();
        assert_eq!(duplicate["publication"], "duplicate");
        assert_eq!(
            stage7b_d_c_single_request_marker(&mut inspector, &second_config).await,
            marker_before,
            "exact deterministic duplicate must not rewrite canonical request marker"
        );
        drop(second);
        fs::remove_dir_all(setup.parent).unwrap();

        let setup = prepare_active_restart_prefix(1);
        let owner = restart_working_setup(&setup);
        let redis = Stage7bDbRedisServer::start().await;
        let mut inspector =
            ConnectionManager::new(redis::Client::open(redis.url.as_str()).unwrap())
                .await
                .unwrap();
        let mut config =
            Stage7bRedisServiceConfig::paper_default_auto("dc-r1-established").unwrap();
        config.block_ms = 0;
        config.claim_idle_ms = 1;
        let mut service = Stage7bRedisService::connect(
            &redis.url,
            config.clone(),
            owner,
            setup.key,
            Stage7aCommandProfile::new(
                BrokerAccountId::new("ACC_PROFILE_MISMATCH"),
                instrument(),
                "hybrid_imoexf",
            )
            .unwrap(),
            Stage7bDcProvider {
                calls: Arc::new(AtomicUsize::new(0)),
            },
        )
        .await
        .unwrap();
        service.ensure_group().await.unwrap();
        stage7b_d_c_xadd(
            &mut inspector,
            &config.command_stream,
            &stage7b_d_c_payload(&setup.command),
        )
        .await;
        service.run_bounded(1).await.unwrap();
        let pending: StreamPendingCountReply = redis::cmd("XPENDING")
            .arg(&config.command_stream)
            .arg(&config.consumer_group)
            .arg("-")
            .arg("+")
            .arg(10)
            .query_async(&mut inspector)
            .await
            .unwrap();
        assert_eq!(pending.ids.len(), 1);
        let ack_count: i64 = redis::cmd("XLEN")
            .arg(&config.ack_stream)
            .query_async(&mut inspector)
            .await
            .unwrap();
        assert_eq!(ack_count, 0);
        drop(service);
        fs::remove_dir_all(setup.parent).unwrap();
    }

    #[tokio::test]
    async fn stage7b_d_c_r2_marker_only_changed_identity_blocks_before_stage6_and_provider() {
        let setup = prepare_active_restart_prefix(1);
        let owner = restart_working_setup(&setup);
        let journal_before = fs::read(setup.root.join(STAGE7B_JOURNAL_FILE)).unwrap();
        let redis = Stage7bDbRedisServer::start().await;
        let mut inspector =
            ConnectionManager::new(redis::Client::open(redis.url.as_str()).unwrap())
                .await
                .unwrap();
        let mut rejected = stage7b_d_c_fresh_request(setup.command.clone());
        match &mut rejected {
            BrokerCommand::PlaceOrder(command) => {
                command.created_ts = Utc::now() - chrono::Duration::seconds(10);
                command.ttl_ms = Some(1);
            }
            BrokerCommand::CancelOrder(command) => {
                command.created_ts = Utc::now() - chrono::Duration::seconds(10);
                command.ttl_ms = Some(1);
            }
        }
        let mut first_config =
            Stage7bRedisServiceConfig::paper_default_auto("dc-r2-marker-conflict").unwrap();
        first_config.block_ms = 0;
        first_config.claim_idle_ms = 1;
        let calls = Arc::new(AtomicUsize::new(0));
        let mut first = Stage7bRedisService::connect(
            &redis.url,
            first_config.clone(),
            owner,
            setup.key,
            stage7b_d_c_profile(&setup.command),
            Stage7bDcProvider {
                calls: calls.clone(),
            },
        )
        .await
        .unwrap();
        first.ensure_group().await.unwrap();
        stage7b_d_c_xadd(
            &mut inspector,
            &first_config.command_stream,
            &stage7b_d_c_payload(&rejected),
        )
        .await;
        first.run_bounded(1).await.unwrap();
        let marker_before = stage7b_d_c_single_request_marker(&mut inspector, &first_config).await;
        drop(first);

        let mut changed = rejected.clone();
        match &mut changed {
            BrokerCommand::PlaceOrder(command) => {
                command.created_ts = Utc::now();
                command.ttl_ms = None;
            }
            BrokerCommand::CancelOrder(command) => {
                command.created_ts = Utc::now();
                command.ttl_ms = None;
            }
        }
        let key2 =
            stage7b_test_authenticated_working_restart_fixture(Stage7bTestExtraStage6History::None)
                .commitment_key;
        let Stage7bRestartOutcome::Ready(owner) = Stage7bRecoveryReadyOwner::restart(
            Stage7bDurableRootAuthority::validate(&setup.root, &setup.identity).unwrap(),
            setup.identity.clone(),
            &key2,
            setup.runtime.clone(),
        )
        .unwrap() else {
            panic!("marker-only conflict fixture must restart ready");
        };
        let matching_profile = stage7b_d_c_profile(&setup.command);
        assert!(matches!(
            matching_profile
                .classify_for_recovered(&changed, owner.recovered().unwrap())
                .unwrap(),
            runtime_command_bridge::Stage7aRecoveredProfileClassification::Matched(_)
        ));
        let mut second_config =
            Stage7bRedisServiceConfig::paper_default_auto("dc-r2-marker-conflict").unwrap();
        second_config.block_ms = 0;
        second_config.claim_idle_ms = 1;
        let mut second = Stage7bRedisService::connect(
            &redis.url,
            second_config.clone(),
            *owner,
            key2,
            matching_profile,
            Stage7bDcProvider {
                calls: calls.clone(),
            },
        )
        .await
        .unwrap();
        stage7b_d_c_xadd(
            &mut inspector,
            &second_config.command_stream,
            &stage7b_d_c_payload(&changed),
        )
        .await;
        second.run_bounded(1).await.unwrap();

        assert_eq!(calls.load(AtomicOrdering::SeqCst), 0);
        assert_eq!(
            fs::read(setup.root.join(STAGE7B_JOURNAL_FILE)).unwrap(),
            journal_before
        );
        let pending: StreamPendingCountReply = redis::cmd("XPENDING")
            .arg(&second_config.command_stream)
            .arg(&second_config.consumer_group)
            .arg("-")
            .arg("+")
            .arg(10)
            .query_async(&mut inspector)
            .await
            .unwrap();
        assert_eq!(pending.ids.len(), 1);
        let ack_count: i64 = redis::cmd("XLEN")
            .arg(&second_config.ack_stream)
            .query_async(&mut inspector)
            .await
            .unwrap();
        let dlq_count: i64 = redis::cmd("XLEN")
            .arg(&second_config.dlq_stream)
            .query_async(&mut inspector)
            .await
            .unwrap();
        assert_eq!(ack_count, 1);
        assert_eq!(dlq_count, 0);
        assert_eq!(
            stage7b_d_c_single_request_marker(&mut inspector, &second_config).await,
            marker_before,
            "changed identity must not overwrite canonical request marker"
        );
        drop(second);
        fs::remove_dir_all(setup.parent).unwrap();
    }

    #[tokio::test]
    async fn stage7b_d_c_r2_prior_profile_rejection_now_matching_is_marker_duplicate_only() {
        let setup = prepare_active_restart_prefix(1);
        let owner = restart_working_setup(&setup);
        let journal_before = fs::read(setup.root.join(STAGE7B_JOURNAL_FILE)).unwrap();
        let redis = Stage7bDbRedisServer::start().await;
        let mut inspector =
            ConnectionManager::new(redis::Client::open(redis.url.as_str()).unwrap())
                .await
                .unwrap();
        let command = stage7b_d_c_fresh_request(setup.command.clone());
        let payload = stage7b_d_c_payload(&command);
        let mut first_config =
            Stage7bRedisServiceConfig::paper_default_auto("dc-r2-profile-transition").unwrap();
        first_config.block_ms = 0;
        first_config.claim_idle_ms = 1;
        let calls = Arc::new(AtomicUsize::new(0));
        let mut first = Stage7bRedisService::connect(
            &redis.url,
            first_config.clone(),
            owner,
            setup.key,
            Stage7aCommandProfile::new(
                BrokerAccountId::new("ACC_PROFILE_MISMATCH"),
                instrument(),
                "hybrid_imoexf",
            )
            .unwrap(),
            Stage7bDcProvider {
                calls: calls.clone(),
            },
        )
        .await
        .unwrap();
        first.ensure_group().await.unwrap();
        stage7b_d_c_xadd(&mut inspector, &first_config.command_stream, &payload).await;
        first.run_bounded(1).await.unwrap();
        let marker_before = stage7b_d_c_single_request_marker(&mut inspector, &first_config).await;
        drop(first);

        let key2 =
            stage7b_test_authenticated_working_restart_fixture(Stage7bTestExtraStage6History::None)
                .commitment_key;
        let Stage7bRestartOutcome::Ready(owner) = Stage7bRecoveryReadyOwner::restart(
            Stage7bDurableRootAuthority::validate(&setup.root, &setup.identity).unwrap(),
            setup.identity.clone(),
            &key2,
            setup.runtime.clone(),
        )
        .unwrap() else {
            panic!("profile-transition fixture must restart ready");
        };
        let mut second_config =
            Stage7bRedisServiceConfig::paper_default_auto("dc-r2-profile-transition").unwrap();
        second_config.block_ms = 0;
        second_config.claim_idle_ms = 1;
        stage7b_d_c_xadd(&mut inspector, &second_config.command_stream, &payload).await;
        let mut second = Stage7bRedisService::connect(
            &redis.url,
            second_config.clone(),
            *owner,
            key2,
            stage7b_d_c_profile(&command),
            Stage7bDcProvider {
                calls: calls.clone(),
            },
        )
        .await
        .unwrap();
        second.run_bounded(1).await.unwrap();

        assert_eq!(calls.load(AtomicOrdering::SeqCst), 0);
        assert_eq!(
            fs::read(setup.root.join(STAGE7B_JOURNAL_FILE)).unwrap(),
            journal_before
        );
        let pending: StreamPendingCountReply = redis::cmd("XPENDING")
            .arg(&second_config.command_stream)
            .arg(&second_config.consumer_group)
            .arg("-")
            .arg("+")
            .arg(10)
            .query_async(&mut inspector)
            .await
            .unwrap();
        assert!(pending.ids.is_empty());
        let acks: StreamRangeReply = redis::cmd("XRANGE")
            .arg(&second_config.ack_stream)
            .arg("-")
            .arg("+")
            .query_async(&mut inspector)
            .await
            .unwrap();
        assert_eq!(acks.ids.len(), 2);
        let duplicate: serde_json::Value =
            serde_json::from_str(&acks.ids[1].get::<String>("payload").unwrap()).unwrap();
        assert_eq!(duplicate["publication"], "duplicate");
        assert_eq!(
            stage7b_d_c_single_request_marker(&mut inspector, &second_config).await,
            marker_before,
            "exact marker duplicate must not rewrite canonical request marker"
        );
        drop(second);
        fs::remove_dir_all(setup.parent).unwrap();
    }

    #[tokio::test]
    async fn stage7b_d_c_r1_b066_real_service_reports_ready_only_while_supervised_task_lives() {
        let setup = prepare_active_restart_prefix(1);
        let owner = restart_working_setup(&setup);
        let redis = Stage7bDbRedisServer::start().await;
        let mut config = Stage7bRedisServiceConfig::paper_default_auto("dc-r1-ready").unwrap();
        config.block_ms = 20;
        config.claim_idle_ms = 1;
        config.freshness_ms = 2_000;
        let service = Stage7bRedisService::connect(
            &redis.url,
            config.clone(),
            owner,
            setup.key,
            stage7b_d_c_profile(&setup.command),
            Stage7bDcProvider {
                calls: Arc::new(AtomicUsize::new(0)),
            },
        )
        .await
        .unwrap();
        let (supervisor, join) = service.spawn_supervised_bounded(10_000);
        let mut observed_ready = false;
        for _ in 0..100 {
            let (health, readiness) = supervisor.snapshots(
                Utc::now(),
                chrono::Duration::milliseconds(config.freshness_ms),
            );
            if readiness.phase == Stage7bPaperReadinessPhase::PaperReady {
                assert!(health.command_consumer_alive);
                assert!(health.durable_storage_ready);
                assert!(health.source_poll_fresh);
                assert!(health.claim_scan_fresh);
                assert!(health.settlement_healthy);
                assert_eq!(health.durable_pending_count, 0);
                assert_eq!(health.blocked_entry_count, 0);
                observed_ready = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert!(
            observed_ready,
            "real supervised Redis service never became PaperReady"
        );
        join.abort();
        let aborted = join.await;
        assert!(matches!(aborted, Err(error) if error.is_cancelled()));
        let (health, readiness) = supervisor.snapshots(
            Utc::now(),
            chrono::Duration::milliseconds(config.freshness_ms),
        );
        assert!(!health.command_consumer_alive);
        assert_eq!(readiness.phase, Stage7bPaperReadinessPhase::Stopped);
        assert!(readiness
            .reasons
            .contains(&Stage7bPaperReadinessReason::ConsumerNotAlive));
        fs::remove_dir_all(setup.parent).unwrap();
    }

    #[tokio::test]
    async fn stage7b_d_c_b064_storage_failure_dominates_redis_health() {
        let setup = prepare_active_restart_prefix(1);
        let owner = restart_working_setup(&setup);
        let redis = Stage7bDbRedisServer::start().await;
        let mut config = Stage7bRedisServiceConfig::paper_default_auto("dc-storage").unwrap();
        config.block_ms = 0;
        let supervisor;
        let mut service = Stage7bRedisService::connect(
            &redis.url,
            config.clone(),
            owner,
            setup.key,
            stage7b_d_c_profile(&setup.command),
            Stage7bDcProvider {
                calls: Arc::new(AtomicUsize::new(0)),
            },
        )
        .await
        .unwrap();
        supervisor = service.supervisor();
        service.ensure_group().await.unwrap();
        fs::remove_file(setup.root.join(STAGE7B_RECOVERY_SEAL_FILE)).unwrap();
        File::open(&setup.root).unwrap().sync_all().unwrap();
        assert!(matches!(
            service.run_bounded(1).await,
            Err(Stage7bRedisServiceError::StorageUnavailable)
        ));
        let (_, readiness) = supervisor.snapshots(
            Utc::now(),
            chrono::Duration::milliseconds(config.freshness_ms),
        );
        assert!(readiness
            .reasons
            .contains(&Stage7bPaperReadinessReason::StorageUnavailable));
        assert_ne!(readiness.phase, Stage7bPaperReadinessPhase::PaperReady);
        drop(service);
        fs::remove_dir_all(setup.parent).unwrap();
    }
}
