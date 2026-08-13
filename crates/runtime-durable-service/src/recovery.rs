use super::{
    is_single_linked_regular, open_child_at, Stage7bDurableRootAuthority,
    Stage7bDurableStorageError, Stage7bKernelWriterLease, Stage7bWritableDurableAuthority,
    STAGE7B_JOURNAL_FILE, STAGE7B_RECOVERY_SEAL_FILE,
};
use broker_core::{BrokerCommand, BrokerOrderId, ClientOrderId, StrategyRequestId};
use chrono::{DateTime, Utc};
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
    admit_stage7a_paper_command, advance_stage6d_restart_package, execute_stage6d_paper_outcome,
    finalize_stage7a_paper_request, finalize_stage7a_replayed_paper_request,
    first_boot_stage6d_paper_from_validated_stage5g_seed_with_owned_journal,
    refresh_stage7b_durable_frontier, restart_stage6d_paper_with_owned_journal,
    restore_stage5g_clean_restart, seal_stage6d_restart_package,
    stage6d_operational_identity_sha256, stage7b_finalized_request_facts,
    HybridIntradayRuntimeStrategy, Stage5gLifecycleCommitmentKey, Stage6JournalBackend,
    Stage6JournalCheckpointV1, Stage6OwnedJournalBackend, Stage6RequestFinalDispositionV1,
    Stage6dDurableRuntimeRecovered, Stage6dFirstBootAuthorization, Stage6dLiveCoreError,
    Stage6dOperationalIdentityConfig, Stage6dPaperDispatchReceipt, Stage6dPaperExecutionReport,
    Stage6dPaperOutcome, Stage7aPaperAdmission, Stage7aPaperCommandContext,
    Stage7bFinalizedRequestFacts,
};

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
    canonical_ack_fingerprint_sha256: String,
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

    pub(crate) fn canonical_ack_fingerprint_sha256(&self) -> &str {
        &self.canonical_ack_fingerprint_sha256
    }

    /// Pure d-a classifier. Future d-b supplies only publication knowledge;
    /// it cannot change the durable request or canonical ACK binding.
    pub(crate) fn classify_publication(
        &self,
        known_canonical_ack_fingerprint_sha256: Option<&str>,
    ) -> Stage7bAckPublicationDecision {
        match known_canonical_ack_fingerprint_sha256 {
            None => Stage7bAckPublicationDecision::Canonical,
            Some(known) if known == self.canonical_ack_fingerprint_sha256 => {
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
}

impl Stage7bRecoveryReadyOwner {
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
        if storage
            .validate_checkpoint(committed_seal.stage6_checkpoint())
            .is_err()
        {
            return Ok(Stage7bRestartOutcome::Blocked(Box::new(
                Stage7bRecoveryBlocked::retained(
                    Stage7bRecoveryBlockReason::CheckpointMismatch,
                    storage,
                ),
            )));
        }

        let (journal, writer_lease) = storage.into_recovery_parts();
        let recovered = match restart_stage6d_paper_with_owned_journal(
            &committed_seal.stage6d_authenticated_restart_package,
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
        if validate_recovered_binding(&recovered, &committed_seal, &identity).is_err() {
            drop(recovered);
            drop(writer_lease);
            return Ok(Stage7bRestartOutcome::Blocked(Box::new(
                Stage7bRecoveryBlocked::after_consumed_storage(
                    Stage7bRecoveryBlockReason::OperationalIdentityMismatch,
                ),
            )));
        }
        writer_lease.validate_namespace()?;
        Ok(Stage7bRestartOutcome::Ready(Box::new(Self {
            recovered,
            writer_lease,
            committed_seal,
            seal_commit_uncertain: false,
        })))
    }

    pub fn recovery_ready(&self) -> bool {
        if self.seal_commit_uncertain {
            return false;
        }
        let Some(identity) = self.recovered.authenticated_operational_identity() else {
            return false;
        };
        self.writer_lease.validate_namespace().is_ok()
            && validate_recovered_binding(&self.recovered, &self.committed_seal, identity).is_ok()
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
        refresh_stage7b_durable_frontier(&mut self.recovered)?;
        if self.committed_seal.stage6_checkpoint() != self.recovered.authenticated_checkpoint() {
            self.advance_recovery_seal(commitment_key)?;
        } else {
            let identity = self
                .recovered
                .authenticated_operational_identity()
                .ok_or(Stage7bRecoveryError::SealInvalid)?;
            validate_recovered_binding(&self.recovered, &self.committed_seal, identity)?;
        }
        durable_ack_authority(&self.committed_seal, current)
    }

    fn require_lifecycle_available(&self) -> Result<(), Stage7bRecoveryError> {
        self.writer_lease.validate_namespace()?;
        if self.seal_commit_uncertain {
            return Err(Stage7bRecoveryError::SealCommitUncertain);
        }
        Ok(())
    }

    #[allow(dead_code, reason = "consumed by the closed Stage 7B-d-b seam")]
    fn advance_recovery_seal(
        &mut self,
        commitment_key: &Stage5gLifecycleCommitmentKey,
    ) -> Result<(), Stage7bRecoveryError> {
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

pub enum Stage7bRestartOutcome {
    Ready(Box<Stage7bRecoveryReadyOwner>),
    Blocked(Box<Stage7bRecoveryBlocked>),
}

impl Stage7bRestartOutcome {
    pub fn recovery_ready(&self) -> bool {
        match self {
            Self::Ready(owner) => owner.recovery_ready(),
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
    struct AckFingerprint<'a> {
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

    let stage6_checkpoint_sha256 = sha256_hex(&seal.stage6_checkpoint().encode_canonical());
    let canonical_ack_fingerprint_sha256 = sha256_hex(
        &serde_json::to_vec(&AckFingerprint {
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
        canonical_ack_fingerprint_sha256,
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
        BrokerCommand, BrokerOrderId, BrokerTradeId, Exchange, InstrumentId, Market,
        StrategyRequestId,
    };
    use std::{
        fs,
        path::{Path, PathBuf},
        process::{Command, Stdio},
        thread,
        time::{Duration, Instant},
    };
    use strategy_runtime_core::{
        authorize_stage6d_first_boot, stage6d_test_authenticated_restart_fixture,
        stage7b_test_authenticated_cancel_restart_fixture,
        stage7b_test_authenticated_working_restart_fixture, Stage6DispatchSafetyStateV1,
        Stage6MemoryJournalBackend, Stage6dBootMode, Stage6dFirstBootConfig,
        Stage7bTestExtraStage6History, Stage7bTestRestartFixture,
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
        }
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

    fn kill_stage7b_d_a_child_at(setup: &PreparedWorkingRestart, phase: &str) {
        let marker = setup.parent.join(format!("stage7b-d-a-{phase}.barrier"));
        let mut child = Command::new(std::env::current_exe().unwrap())
            .arg("--ignored")
            .arg("--exact")
            .arg("recovery::tests::stage7b_d_a_crash_barrier_child")
            .arg("--nocapture")
            .env("STAGE7B_D_A_CHILD_ROOT", &setup.root)
            .env("STAGE7B_D_A_CHILD_MARKER", &marker)
            .env("STAGE7B_D_A_CHILD_PHASE", phase)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        wait_for_child(&mut child, &marker);
        child.kill().unwrap();
        child.wait().unwrap();
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
        if phase == "dispatch" || phase == "during-effect" {
            fs::write(&marker, phase.as_bytes()).unwrap();
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
            authority.classify_publication(Some(authority.canonical_ack_fingerprint_sha256())),
            Stage7bAckPublicationDecision::Duplicate
        );
        assert_eq!(
            authority.classify_publication(Some(&"f".repeat(64))),
            Stage7bAckPublicationDecision::Conflict
        );
        assert!(owner.recovery_ready());
        let fingerprint = authority.canonical_ack_fingerprint_sha256().to_string();
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
            reconstructed_authority.canonical_ack_fingerprint_sha256(),
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
        kill_stage7b_d_a_child_at(&setup, phase);
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

    #[test]
    fn stage7b_d_a_b048_sigkill_after_finalization_reconstructs_canonical_ack() {
        assert_post_finalization_restart_is_canonical("finalized", 2);
    }

    #[test]
    fn stage7b_d_a_b051_sigkill_after_seal_reconstructs_without_provider() {
        assert_post_finalization_restart_is_canonical("sealed", 2);
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
}
