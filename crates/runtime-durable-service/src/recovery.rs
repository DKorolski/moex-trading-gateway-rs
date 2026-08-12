use super::{
    is_single_linked_regular, open_child_at, Stage7bDurableRootAuthority,
    Stage7bDurableStorageError, Stage7bKernelWriterLease, Stage7bWritableDurableAuthority,
    STAGE7B_JOURNAL_FILE, STAGE7B_RECOVERY_SEAL_FILE,
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
    first_boot_stage6d_paper_from_validated_stage5g_seed_with_owned_journal,
    restart_stage6d_paper_with_owned_journal, restore_stage5g_clean_restart,
    seal_stage6d_restart_package, stage6d_operational_identity_sha256,
    HybridIntradayRuntimeStrategy, Stage5gLifecycleCommitmentKey, Stage6JournalBackend,
    Stage6JournalCheckpointV1, Stage6OwnedJournalBackend, Stage6dDurableRuntimeRecovered,
    Stage6dFirstBootAuthorization, Stage6dLiveCoreError, Stage6dOperationalIdentityConfig,
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

/// Linear ready owner. Field order is intentional: the Stage 6 runtime (and
/// its file journal) closes before the filesystem writer lease is released.
/// No mutable recovered-runtime extractor is exposed.
pub struct Stage7bRecoveryReadyOwner {
    recovered: Stage6dDurableRuntimeRecovered,
    writer_lease: Stage7bKernelWriterLease,
    committed_seal: Stage7bRecoverySealV1,
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
        })))
    }

    pub fn recovery_ready(&self) -> bool {
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
    use std::{fs, path::PathBuf};
    use strategy_runtime_core::{
        authorize_stage6d_first_boot, stage6d_test_authenticated_restart_fixture, Stage6dBootMode,
        Stage6dFirstBootConfig,
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
