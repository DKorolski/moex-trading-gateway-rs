//! Stage 7B broker-neutral durable storage ownership foundation.
//!
//! The writable authority is linear and cannot be cloned:
//!
//! ```compile_fail
//! use runtime_durable_service::Stage7bWritableDurableAuthority;
//! fn require_clone<T: Clone>() {}
//! require_clone::<Stage7bWritableDurableAuthority>();
//! ```
//!
//! Its raw journal and kernel lock cannot be extracted by downstream callers:
//!
//! ```compile_fail
//! use runtime_durable_service::Stage7bWritableDurableAuthority;
//! fn split(authority: Stage7bWritableDurableAuthority) {
//!     let _ = authority.into_parts();
//! }
//! ```
//!
//! The authority cannot be serialized into restart or diagnostic data:
//!
//! ```compile_fail
//! use runtime_durable_service::Stage7bWritableDurableAuthority;
//! let authority: Stage7bWritableDurableAuthority = unreachable!();
//! let _ = serde_json::to_vec(&authority).unwrap();
//! ```
//!
//! The anchored durable-root capability is also linear and cannot be cloned or
//! serialized:
//!
//! ```compile_fail
//! use runtime_durable_service::Stage7bDurableRootAuthority;
//! fn require_clone<T: Clone>() {}
//! require_clone::<Stage7bDurableRootAuthority>();
//! ```
//!
//! ```compile_fail
//! use runtime_durable_service::Stage7bDurableRootAuthority;
//! let root: Stage7bDurableRootAuthority = unreachable!();
//! let _ = serde_json::to_vec(&root).unwrap();
//! ```
//!
//! The recovery-ready owner is the only object that owns both the recovered
//! Stage 6 runtime and its Stage 7B writer lease. It is linear, non-serializable
//! and does not expose a mutable runtime or raw lease extractor:
//!
//! ```compile_fail
//! use runtime_durable_service::Stage7bRecoveryReadyOwner;
//! fn require_clone<T: Clone>() {}
//! require_clone::<Stage7bRecoveryReadyOwner>();
//! ```
//!
//! ```compile_fail
//! use runtime_durable_service::Stage7bRecoveryReadyOwner;
//! let owner: Stage7bRecoveryReadyOwner = unreachable!();
//! let _ = serde_json::to_vec(&owner).unwrap();
//! ```
//!
//! ```compile_fail
//! use runtime_durable_service::Stage7bRecoveryReadyOwner;
//! fn mutate(owner: &mut Stage7bRecoveryReadyOwner) {
//!     let _runtime = owner.recovered_mut();
//! }
//! ```
//!
//! Public V2/read DTOs and caller-built batches cannot reach the covering S1
//! writer. The former raw Stage7B entry is intentionally absent:
//!
//! ```compile_fail
//! use runtime_durable_service::Stage7bRecoveryReadyOwner;
//! use strategy_runtime_core::{
//!     Stage5gLifecycleCommitmentKey, Stage6DurableCommandSnapshotV1,
//!     Stage6DurableRequestIdentityV1, Stage6Stage8a4DurableBatch,
//! };
//! fn bypass(
//!     owner: &mut Stage7bRecoveryReadyOwner,
//!     key: &Stage5gLifecycleCommitmentKey,
//!     identity: &Stage6DurableRequestIdentityV1,
//!     command: &Stage6DurableCommandSnapshotV1,
//!     batch: Stage6Stage8a4DurableBatch,
//! ) {
//!     owner.append_stage8a4_durable_batch_and_cover(key, identity, command, batch).unwrap();
//! }
//! ```
//!
//! The terminal ACK authority is crate-private:
//!
//! ```compile_fail
//! use runtime_durable_service::Stage7bDurableAckAuthorized;
//! ```
//!
//! The cross-crate I4 terminal authority is nameable but remains opaque and
//! linear. External callers cannot construct or clone it:
//!
//! ```compile_fail
//! use runtime_durable_service::Stage7bStage8a4TerminalAuthority;
//! let _forged = Stage7bStage8a4TerminalAuthority {};
//! ```
//!
//! ```compile_fail
//! use runtime_durable_service::Stage7bStage8a4TerminalAuthority;
//! let authority: Stage7bStage8a4TerminalAuthority = unreachable!();
//! let _copy = authority.clone();
//! ```
//!
//! ```compile_fail
//! use runtime_durable_service::Stage7bStage8a4TerminalAuthority;
//! let authority: Stage7bStage8a4TerminalAuthority = unreachable!();
//! println!("{authority:?}");
//! ```
//!
//! ```compile_fail
//! use runtime_durable_service::Stage7bStage8a4TerminalAuthority;
//! let authority: Stage7bStage8a4TerminalAuthority = unreachable!();
//! let _ = serde_json::to_vec(&authority).unwrap();
//! ```

#[cfg(not(unix))]
compile_error!("runtime-durable-service requires Unix kernel file locking");

mod recovery;

pub use recovery::{
    spawn_stage7b_supervised_task, Stage7bCompositeHealthSnapshot,
    Stage7bCompositeReadinessSnapshot, Stage7bPaperReadinessPhase, Stage7bPaperReadinessReason,
    Stage7bRecoveryBlockReason, Stage7bRecoveryBlocked, Stage7bRecoveryError,
    Stage7bRecoveryReadyOwner, Stage7bRecoverySealV1, Stage7bRedisService,
    Stage7bRedisServiceConfig, Stage7bRedisServiceError, Stage7bRestartOutcome,
    Stage7bServiceRunSummary, Stage7bServiceSupervisor, Stage7bServiceTaskHandle,
    Stage7bServiceTaskOutput, Stage7bStage8a1DurableRequestAuthority,
    Stage7bStage8a4DurableBatchReceipt, Stage7bStage8a4TerminalAuthority,
    Stage7bTaskReadinessHandle, Stage8a4I3RecoveryPendingOwner,
    STAGE7B_RECOVERY_SEAL_SCHEMA_VERSION,
};
#[cfg(feature = "stage8a4-i3-test-fixtures")]
#[doc(hidden)]
pub use recovery::{
    stage8a4_i3_production_test_setup, stage8a4_i3_production_test_setup_in,
    Stage8a4I3ProductionTestSetup,
};
#[cfg(feature = "stage8a4-i3-test-fixtures")]
#[doc(hidden)]
pub use recovery::{
    stage8a4_i3_test_fail_before_covering_seal, stage8a4_i3_test_set_owner_journal_failpoint,
};

use std::{
    ffi::CString,
    fs::{self, File},
    io::ErrorKind,
    os::fd::{AsRawFd, FromRawFd},
    os::unix::{ffi::OsStrExt, fs::MetadataExt},
    path::{Path, PathBuf},
};

use strategy_runtime_core::{
    stage6d_operational_identity_sha256, Stage6FileJournalBackend, Stage6JournalAppendReceipt,
    Stage6JournalBackend, Stage6JournalCheckpointV1, Stage6JournalFrontierV1,
    Stage6JournalRecordV1, Stage6JournalRecordVersioned, Stage6JournalStorageError,
    Stage6Sha256Digest, Stage6dFirstBootAuthorization, Stage6dOperationalIdentityConfig,
};

pub const STAGE7B_JOURNAL_FILE: &str = "stage6.journal";
pub const STAGE7B_WRITER_LOCK_FILE: &str = "stage6.writer.lock";
pub const STAGE7B_RECOVERY_SEAL_FILE: &str = "stage7b-recovery.seal";
pub const STAGE7B_TMP_DIRECTORY: &str = "tmp";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage7bStorageOpenPhase {
    PathValidated,
    WriterLockAcquired,
    PathRevalidated,
    JournalOpened,
    StorageReady,
}

pub const STAGE7B_STORAGE_OPEN_ORDER: [Stage7bStorageOpenPhase; 5] = [
    Stage7bStorageOpenPhase::PathValidated,
    Stage7bStorageOpenPhase::WriterLockAcquired,
    Stage7bStorageOpenPhase::PathRevalidated,
    Stage7bStorageOpenPhase::JournalOpened,
    Stage7bStorageOpenPhase::StorageReady,
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stage7bDurableStorageError {
    InvalidOperationalIdentity,
    RootMustBeAbsolute,
    RootMissing,
    RootIsSymlink,
    RootNotDirectory,
    RootNotCanonical,
    IdentityDirectoryMismatch,
    OperationalIdentityMismatch,
    FirstBootAuthorizationMismatch,
    UnsafeJournalPath,
    UnsafeWriterLockPath,
    UnsafeRecoverySealPath,
    UnsafeTmpPath,
    RootIdentityDrift,
    WriterLockIdentityDrift,
    WriterAlreadyHeld,
    Io(ErrorKind),
    Journal(Stage6JournalStorageError),
}

impl std::fmt::Display for Stage7bDurableStorageError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::InvalidOperationalIdentity => "invalid authenticated operational identity",
            Self::RootMustBeAbsolute => "durable root must be absolute",
            Self::RootMissing => "durable root is missing",
            Self::RootIsSymlink => "durable root cannot be a symlink",
            Self::RootNotDirectory => "durable root is not a directory",
            Self::RootNotCanonical => "durable root is not its canonical path",
            Self::IdentityDirectoryMismatch => "durable directory identity mismatch",
            Self::OperationalIdentityMismatch => {
                "operational identity does not match durable-root authority"
            }
            Self::FirstBootAuthorizationMismatch => "first-boot authorization mismatch",
            Self::UnsafeJournalPath => "unsafe durable journal path",
            Self::UnsafeWriterLockPath => "unsafe writer-lock path",
            Self::UnsafeRecoverySealPath => "unsafe recovery-seal path",
            Self::UnsafeTmpPath => "unsafe durable tmp path",
            Self::RootIdentityDrift => "anchored durable-root identity drift",
            Self::WriterLockIdentityDrift => "writer-lock namespace identity drift",
            Self::WriterAlreadyHeld => "durable writer lock is already held",
            Self::Io(_) => "durable storage I/O failure",
            Self::Journal(_) => "durable journal failure",
        })
    }
}

impl std::error::Error for Stage7bDurableStorageError {}

impl From<std::io::Error> for Stage7bDurableStorageError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error.kind())
    }
}

impl From<Stage6JournalStorageError> for Stage7bDurableStorageError {
    fn from(error: Stage6JournalStorageError) -> Self {
        Self::Journal(error)
    }
}

#[derive(Debug)]
pub struct Stage7bDurableRootAuthority {
    parent_path: PathBuf,
    parent_directory: File,
    parent_dev: u64,
    parent_ino: u64,
    root_path: PathBuf,
    root_directory: File,
    root_dev: u64,
    root_ino: u64,
    operational_identity_sha256: Stage6Sha256Digest,
}

impl Stage7bDurableRootAuthority {
    pub fn expected_directory_name(
        identity: &Stage6dOperationalIdentityConfig,
    ) -> Result<String, Stage7bDurableStorageError> {
        let digest = stage6d_operational_identity_sha256(identity)
            .map_err(|_| Stage7bDurableStorageError::InvalidOperationalIdentity)?;
        Ok(format!("stage7b-{}", digest.as_str()))
    }

    pub fn validate(
        root: impl AsRef<Path>,
        identity: &Stage6dOperationalIdentityConfig,
    ) -> Result<Self, Stage7bDurableStorageError> {
        let root = root.as_ref();
        if !root.is_absolute() {
            return Err(Stage7bDurableStorageError::RootMustBeAbsolute);
        }
        let metadata = fs::symlink_metadata(root).map_err(|error| {
            if error.kind() == ErrorKind::NotFound {
                Stage7bDurableStorageError::RootMissing
            } else {
                error.into()
            }
        })?;
        if metadata.file_type().is_symlink() {
            return Err(Stage7bDurableStorageError::RootIsSymlink);
        }
        if !metadata.file_type().is_dir() {
            return Err(Stage7bDurableStorageError::RootNotDirectory);
        }
        let canonical = fs::canonicalize(root)?;
        if canonical != root {
            return Err(Stage7bDurableStorageError::RootNotCanonical);
        }
        let digest = stage6d_operational_identity_sha256(identity)
            .map_err(|_| Stage7bDurableStorageError::InvalidOperationalIdentity)?;
        let expected = format!("stage7b-{}", digest.as_str());
        if canonical.file_name().and_then(|value| value.to_str()) != Some(expected.as_str()) {
            return Err(Stage7bDurableStorageError::IdentityDirectoryMismatch);
        }
        let parent_path = canonical
            .parent()
            .ok_or(Stage7bDurableStorageError::RootNotCanonical)?
            .to_path_buf();
        let parent_directory = open_root_directory(&parent_path)?;
        let parent_metadata = parent_directory.metadata()?;
        let root_directory = open_child_at(
            &parent_directory,
            &expected,
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            0,
        )?;
        let opened = root_directory.metadata()?;
        if opened.dev() != metadata.dev() || opened.ino() != metadata.ino() {
            return Err(Stage7bDurableStorageError::RootIdentityDrift);
        }
        validate_optional_regular_at(
            &root_directory,
            STAGE7B_JOURNAL_FILE,
            Stage7bDurableStorageError::UnsafeJournalPath,
        )?;
        validate_optional_regular_at(
            &root_directory,
            STAGE7B_WRITER_LOCK_FILE,
            Stage7bDurableStorageError::UnsafeWriterLockPath,
        )?;
        validate_optional_regular_at(
            &root_directory,
            STAGE7B_RECOVERY_SEAL_FILE,
            Stage7bDurableStorageError::UnsafeRecoverySealPath,
        )?;
        validate_optional_directory_at(
            &root_directory,
            STAGE7B_TMP_DIRECTORY,
            Stage7bDurableStorageError::UnsafeTmpPath,
        )?;
        Ok(Self {
            parent_path,
            parent_directory,
            parent_dev: parent_metadata.dev(),
            parent_ino: parent_metadata.ino(),
            root_path: canonical,
            root_directory,
            root_dev: opened.dev(),
            root_ino: opened.ino(),
            operational_identity_sha256: digest,
        })
    }

    pub fn root(&self) -> &Path {
        &self.root_path
    }

    pub fn operational_identity_sha256(&self) -> &Stage6Sha256Digest {
        &self.operational_identity_sha256
    }

    fn namespace_lock_name(&self) -> String {
        format!(
            ".stage7b-{}.namespace.lock",
            self.operational_identity_sha256.as_str()
        )
    }

    fn validate_bound_identity(
        &self,
        identity: &Stage6dOperationalIdentityConfig,
    ) -> Result<(), Stage7bDurableStorageError> {
        let observed = stage6d_operational_identity_sha256(identity)
            .map_err(|_| Stage7bDurableStorageError::InvalidOperationalIdentity)?;
        if observed != self.operational_identity_sha256 {
            return Err(Stage7bDurableStorageError::OperationalIdentityMismatch);
        }
        Ok(())
    }

    fn validate_external_root_identity(&self) -> Result<(), Stage7bDurableStorageError> {
        let named_parent = fs::symlink_metadata(&self.parent_path)
            .map_err(|_| Stage7bDurableStorageError::RootIdentityDrift)?;
        let opened_parent = self.parent_directory.metadata()?;
        let named = fs::symlink_metadata(&self.root_path)
            .map_err(|_| Stage7bDurableStorageError::RootIdentityDrift)?;
        let opened = self.root_directory.metadata()?;
        if named_parent.file_type().is_symlink()
            || !named_parent.file_type().is_dir()
            || !opened_parent.file_type().is_dir()
            || named_parent.dev() != self.parent_dev
            || named_parent.ino() != self.parent_ino
            || opened_parent.dev() != self.parent_dev
            || opened_parent.ino() != self.parent_ino
            || named.file_type().is_symlink()
            || !named.file_type().is_dir()
            || !opened.file_type().is_dir()
            || named.dev() != self.root_dev
            || named.ino() != self.root_ino
            || opened.dev() != self.root_dev
            || opened.ino() != self.root_ino
        {
            return Err(Stage7bDurableStorageError::RootIdentityDrift);
        }
        Ok(())
    }
}

fn open_root_directory(path: &Path) -> Result<File, Stage7bDurableStorageError> {
    let path = CString::new(path.as_os_str().as_bytes())
        .map_err(|_| Stage7bDurableStorageError::Io(ErrorKind::InvalidInput))?;
    // SAFETY: `path` is NUL terminated and the returned descriptor is adopted
    // exactly once on success.
    let fd = unsafe {
        libc::open(
            path.as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    };
    if fd < 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    // SAFETY: `fd` is a fresh descriptor owned by this function.
    Ok(unsafe { File::from_raw_fd(fd) })
}

fn open_child_at(
    root: &File,
    name: &str,
    flags: libc::c_int,
    mode: libc::mode_t,
) -> Result<File, std::io::Error> {
    let name =
        CString::new(name.as_bytes()).map_err(|_| std::io::Error::from(ErrorKind::InvalidInput))?;
    // SAFETY: the root descriptor is live, `name` is NUL terminated, and the
    // returned descriptor is adopted exactly once on success.
    let fd = unsafe { libc::openat(root.as_raw_fd(), name.as_ptr(), flags, mode as libc::c_uint) };
    if fd < 0 {
        return Err(std::io::Error::last_os_error());
    }
    // SAFETY: `fd` is a fresh descriptor owned by this function.
    Ok(unsafe { File::from_raw_fd(fd) })
}

fn validate_optional_regular_at(
    root: &File,
    name: &str,
    unsafe_error: Stage7bDurableStorageError,
) -> Result<(), Stage7bDurableStorageError> {
    match open_child_at(
        root,
        name,
        libc::O_RDONLY | libc::O_NONBLOCK | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        0,
    ) {
        Ok(file) => {
            let metadata = file.metadata()?;
            if metadata.file_type().is_file() && metadata.nlink() == 1 {
                Ok(())
            } else {
                Err(unsafe_error)
            }
        }
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(_) => Err(unsafe_error),
    }
}

fn validate_optional_directory_at(
    root: &File,
    name: &str,
    unsafe_error: Stage7bDurableStorageError,
) -> Result<(), Stage7bDurableStorageError> {
    match open_child_at(
        root,
        name,
        libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        0,
    ) {
        Ok(file) if file.metadata()?.file_type().is_dir() => Ok(()),
        Ok(_) => Err(unsafe_error),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(_) => Err(unsafe_error),
    }
}

struct Stage7bKernelWriterLease {
    root: Stage7bDurableRootAuthority,
    namespace_lock_file: File,
    lock_file: File,
}

impl Stage7bKernelWriterLease {
    fn acquire(root: Stage7bDurableRootAuthority) -> Result<Self, Stage7bDurableStorageError> {
        let namespace_lock_file = open_child_at(
            &root.parent_directory,
            &root.namespace_lock_name(),
            libc::O_RDWR | libc::O_CREAT | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            0o600,
        )?;
        if !is_single_linked_regular(&namespace_lock_file)? {
            return Err(Stage7bDurableStorageError::UnsafeWriterLockPath);
        }
        acquire_nonblocking_exclusive_lock(&namespace_lock_file)?;
        acquire_nonblocking_exclusive_lock(&root.root_directory)?;
        root.validate_external_root_identity()?;
        let lock_file = open_child_at(
            &root.root_directory,
            STAGE7B_WRITER_LOCK_FILE,
            libc::O_RDWR | libc::O_CREAT | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            0o600,
        )?;
        if !is_single_linked_regular(&lock_file)? {
            return Err(Stage7bDurableStorageError::UnsafeWriterLockPath);
        }
        acquire_nonblocking_exclusive_lock(&lock_file)?;
        root.root_directory.sync_all()?;
        root.parent_directory.sync_all()?;
        let lease = Self {
            root,
            namespace_lock_file,
            lock_file,
        };
        lease.validate_namespace()?;
        Ok(lease)
    }

    fn validate_namespace(&self) -> Result<(), Stage7bDurableStorageError> {
        let named_namespace = open_child_at(
            &self.root.parent_directory,
            &self.root.namespace_lock_name(),
            libc::O_RDONLY | libc::O_NONBLOCK | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            0,
        )
        .map_err(|_| Stage7bDurableStorageError::WriterLockIdentityDrift)?;
        if !same_file_identity(&named_namespace, &self.namespace_lock_file)? {
            return Err(Stage7bDurableStorageError::WriterLockIdentityDrift);
        }
        self.root.validate_external_root_identity()?;
        let named = open_child_at(
            &self.root.root_directory,
            STAGE7B_WRITER_LOCK_FILE,
            libc::O_RDONLY | libc::O_NONBLOCK | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            0,
        )
        .map_err(|_| Stage7bDurableStorageError::WriterLockIdentityDrift)?;
        if !same_file_identity(&named, &self.lock_file)? {
            return Err(Stage7bDurableStorageError::WriterLockIdentityDrift);
        }
        Ok(())
    }

    fn open_journal<F>(
        &self,
        create: bool,
        pre_create_sync_observer: F,
    ) -> Result<Stage6FileJournalBackend, Stage7bDurableStorageError>
    where
        F: FnOnce(),
    {
        self.validate_namespace()?;
        let flags = libc::O_RDWR
            | libc::O_NOFOLLOW
            | libc::O_CLOEXEC
            | if create {
                libc::O_CREAT | libc::O_EXCL
            } else {
                0
            };
        let file = open_child_at(
            &self.root.root_directory,
            STAGE7B_JOURNAL_FILE,
            flags,
            0o600,
        )?;
        if !is_single_linked_regular(&file)? {
            return Err(Stage7bDurableStorageError::UnsafeJournalPath);
        }
        let diagnostic_path = self.root.root_path.join(STAGE7B_JOURNAL_FILE);
        let journal = if create {
            let journal =
                Stage6FileJournalBackend::create_new_from_owned_file_with_pre_sync_observer(
                    diagnostic_path,
                    file,
                    pre_create_sync_observer,
                )?;
            self.root.root_directory.sync_all()?;
            journal
        } else {
            Stage6FileJournalBackend::open_existing_from_owned_file(diagnostic_path, file)?
        };
        self.validate_namespace()?;
        Ok(journal)
    }
}

fn acquire_nonblocking_exclusive_lock(file: &File) -> Result<(), Stage7bDurableStorageError> {
    // SAFETY: `file` owns a live descriptor. `flock` does not dereference
    // memory and the descriptor remains live for the entire lease on success.
    let result = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
    if result == 0 {
        return Ok(());
    }
    let error = std::io::Error::last_os_error();
    if matches!(error.raw_os_error(), Some(code) if code == libc::EWOULDBLOCK || code == libc::EAGAIN)
    {
        return Err(Stage7bDurableStorageError::WriterAlreadyHeld);
    }
    Err(error.into())
}

fn is_single_linked_regular(file: &File) -> Result<bool, Stage7bDurableStorageError> {
    let metadata = file.metadata()?;
    Ok(metadata.file_type().is_file() && metadata.nlink() == 1)
}

fn same_file_identity(left: &File, right: &File) -> Result<bool, Stage7bDurableStorageError> {
    let left = left.metadata()?;
    let right = right.metadata()?;
    Ok(left.file_type().is_file()
        && right.file_type().is_file()
        && left.nlink() == 1
        && right.nlink() == 1
        && left.dev() == right.dev()
        && left.ino() == right.ino())
}

impl Drop for Stage7bKernelWriterLease {
    fn drop(&mut self) {
        // SAFETY: this descriptor is owned by the lease and remains valid until
        // `File` is dropped immediately after this method returns.
        let _ = unsafe { libc::flock(self.lock_file.as_raw_fd(), libc::LOCK_UN) };
        let _ = unsafe { libc::flock(self.root.root_directory.as_raw_fd(), libc::LOCK_UN) };
        let _ = unsafe { libc::flock(self.namespace_lock_file.as_raw_fd(), libc::LOCK_UN) };
    }
}

/// Linear writable storage authority. Fields drop in declaration order, so the
/// journal closes before the kernel lease is released. Neither can be
/// separated from this authority.
pub struct Stage7bWritableDurableAuthority {
    journal: Stage6FileJournalBackend,
    _writer_lease: Stage7bKernelWriterLease,
    operational_identity_sha256: Stage6Sha256Digest,
}

impl Stage7bWritableDurableAuthority {
    pub fn create_new(
        root: Stage7bDurableRootAuthority,
        identity: &Stage6dOperationalIdentityConfig,
        authorization: &Stage6dFirstBootAuthorization,
    ) -> Result<Self, Stage7bDurableStorageError> {
        root.validate_bound_identity(identity)?;
        if !authorization.authorizes_deployment(&identity.deployment_id) {
            return Err(Stage7bDurableStorageError::FirstBootAuthorizationMismatch);
        }
        Self::open(root, true, |_| {}, || {})
    }

    pub fn open_existing(
        root: Stage7bDurableRootAuthority,
        identity: &Stage6dOperationalIdentityConfig,
    ) -> Result<Self, Stage7bDurableStorageError> {
        root.validate_bound_identity(identity)?;
        Self::open(root, false, |_| {}, || {})
    }

    /// Opens existing storage while reporting ordered phases. The callback
    /// receives no filesystem handles and every callback boundary is followed
    /// by anchored namespace validation before writable progress continues.
    /// This is primarily a deterministic fault-injection seam for real
    /// filesystem/process acceptance tests.
    #[doc(hidden)]
    pub fn open_existing_with_phase_observer<F>(
        root: Stage7bDurableRootAuthority,
        identity: &Stage6dOperationalIdentityConfig,
        observer: F,
    ) -> Result<Self, Stage7bDurableStorageError>
    where
        F: FnMut(Stage7bStorageOpenPhase),
    {
        root.validate_bound_identity(identity)?;
        Self::open(root, false, observer, || {})
    }

    /// Creates storage through the normal owned production path while
    /// exposing only the frozen X02 boundary: complete journal header written,
    /// first file sync not yet attempted. No file descriptor or authority is
    /// exposed to the observer.
    #[doc(hidden)]
    pub fn create_new_with_pre_journal_sync_observer<F>(
        root: Stage7bDurableRootAuthority,
        identity: &Stage6dOperationalIdentityConfig,
        authorization: &Stage6dFirstBootAuthorization,
        observer: F,
    ) -> Result<Self, Stage7bDurableStorageError>
    where
        F: FnOnce(),
    {
        root.validate_bound_identity(identity)?;
        if !authorization.authorizes_deployment(&identity.deployment_id) {
            return Err(Stage7bDurableStorageError::FirstBootAuthorizationMismatch);
        }
        Self::open(root, true, |_| {}, observer)
    }

    fn open<F, G>(
        root: Stage7bDurableRootAuthority,
        create: bool,
        mut observer: F,
        pre_create_sync_observer: G,
    ) -> Result<Self, Stage7bDurableStorageError>
    where
        F: FnMut(Stage7bStorageOpenPhase),
        G: FnOnce(),
    {
        observer(Stage7bStorageOpenPhase::PathValidated);
        root.validate_external_root_identity()?;
        let writer_lease = Stage7bKernelWriterLease::acquire(root)?;
        observer(Stage7bStorageOpenPhase::WriterLockAcquired);
        writer_lease.validate_namespace()?;
        observer(Stage7bStorageOpenPhase::PathRevalidated);
        writer_lease.validate_namespace()?;
        let journal = writer_lease.open_journal(create, pre_create_sync_observer)?;
        observer(Stage7bStorageOpenPhase::JournalOpened);
        writer_lease.validate_namespace()?;
        observer(Stage7bStorageOpenPhase::StorageReady);
        writer_lease.validate_namespace()?;
        let operational_identity_sha256 = writer_lease.root.operational_identity_sha256.clone();
        Ok(Self {
            journal,
            _writer_lease: writer_lease,
            operational_identity_sha256,
        })
    }

    pub fn operational_identity_sha256(&self) -> &Stage6Sha256Digest {
        &self.operational_identity_sha256
    }
}

impl Stage6JournalBackend for Stage7bWritableDurableAuthority {
    fn append(
        &mut self,
        record: &Stage6JournalRecordV1,
    ) -> Result<Stage6JournalAppendReceipt, Stage6JournalStorageError> {
        self._writer_lease
            .validate_namespace()
            .map_err(|_| Stage6JournalStorageError::ExternalMutationDetected)?;
        self.journal.append(record)
    }

    fn records(&self) -> &[Stage6JournalRecordV1] {
        self.journal.records()
    }

    fn append_versioned(
        &mut self,
        record: &Stage6JournalRecordVersioned,
    ) -> Result<Stage6JournalAppendReceipt, Stage6JournalStorageError> {
        self._writer_lease
            .validate_namespace()
            .map_err(|_| Stage6JournalStorageError::ExternalMutationDetected)?;
        self.journal.append_versioned(record)
    }

    fn versioned_records(&self) -> &[Stage6JournalRecordVersioned] {
        self.journal.versioned_records()
    }

    fn frontier(&self) -> &Stage6JournalFrontierV1 {
        self.journal.frontier()
    }

    fn framed_bytes(&self) -> Result<Vec<u8>, Stage6JournalStorageError> {
        self._writer_lease
            .validate_namespace()
            .map_err(|_| Stage6JournalStorageError::ExternalMutationDetected)?;
        self.journal.framed_bytes()
    }

    fn validate_checkpoint(
        &self,
        checkpoint: &Stage6JournalCheckpointV1,
    ) -> Result<(), Stage6JournalStorageError> {
        self.journal.validate_checkpoint(checkpoint)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        sync::atomic::{AtomicU64, Ordering},
        time::{SystemTime, UNIX_EPOCH},
    };

    static TEST_DIRECTORY_COUNTER: AtomicU64 = AtomicU64::new(1);
    use strategy_runtime_core::{authorize_stage6d_first_boot, Stage6dFirstBootConfig};

    fn identity() -> Stage6dOperationalIdentityConfig {
        Stage6dOperationalIdentityConfig {
            broker_id: "paper".to_string(),
            strategy_instance_id: "hybrid-imoexf".to_string(),
            deployment_id: "stage7b-test".to_string(),
            deployment_generation: 1,
            gateway_instance_id: "gateway-test".to_string(),
            instrument_map_fingerprint_sha256: "1".repeat(64),
            market_data_generation: 1,
            command_consumer_generation: 1,
            stage8a4_writer_issuer_public_key_hex:
                "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a".to_string(),
        }
    }

    fn parent() -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "stage7b-path-test-{}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            TEST_DIRECTORY_COUNTER.fetch_add(1, Ordering::SeqCst)
        ));
        fs::create_dir(&path).unwrap();
        fs::canonicalize(path).unwrap()
    }

    fn durable_root(parent: &Path, identity: &Stage6dOperationalIdentityConfig) -> PathBuf {
        let root =
            parent.join(Stage7bDurableRootAuthority::expected_directory_name(identity).unwrap());
        fs::create_dir(&root).unwrap();
        root
    }

    fn authorization(deployment_id: &str) -> Stage6dFirstBootAuthorization {
        authorize_stage6d_first_boot(Stage6dFirstBootConfig {
            deployment_id: deployment_id.to_string(),
            expected_runtime_config_fingerprint_sha256: "3".repeat(64),
            allow_create_missing_journal: true,
        })
        .unwrap()
    }

    #[test]
    fn stage7b_b_storage_open_order_is_lock_before_journal_and_ready() {
        assert_eq!(
            STAGE7B_STORAGE_OPEN_ORDER,
            [
                Stage7bStorageOpenPhase::PathValidated,
                Stage7bStorageOpenPhase::WriterLockAcquired,
                Stage7bStorageOpenPhase::PathRevalidated,
                Stage7bStorageOpenPhase::JournalOpened,
                Stage7bStorageOpenPhase::StorageReady,
            ]
        );
    }

    #[test]
    fn stage7b_b_validated_identity_path_opens_linear_authority() {
        let identity = identity();
        let parent = parent();
        let root = durable_root(&parent, &identity);
        let paths = Stage7bDurableRootAuthority::validate(&root, &identity).unwrap();
        let authorization = authorization(&identity.deployment_id);
        let authority =
            Stage7bWritableDurableAuthority::create_new(paths, &identity, &authorization).unwrap();
        assert_eq!(
            authority.operational_identity_sha256(),
            &stage6d_operational_identity_sha256(&identity).unwrap()
        );
        drop(authority);
        fs::remove_dir_all(parent).unwrap();
    }

    #[test]
    fn stage7b_b_first_boot_creation_requires_matching_linear_authorization() {
        let identity = identity();
        let parent = parent();
        let root = durable_root(&parent, &identity);
        let paths = Stage7bDurableRootAuthority::validate(&root, &identity).unwrap();
        let wrong = authorization("another-deployment");
        assert!(matches!(
            Stage7bWritableDurableAuthority::create_new(paths, &identity, &wrong),
            Err(Stage7bDurableStorageError::FirstBootAuthorizationMismatch)
        ));
        assert!(!root.join(STAGE7B_WRITER_LOCK_FILE).exists());
        assert!(!root.join(STAGE7B_JOURNAL_FILE).exists());
        fs::remove_dir_all(parent).unwrap();
    }

    #[test]
    fn stage7b_b_wrong_identity_and_noncanonical_alias_fail_before_open() {
        let identity = identity();
        let parent = parent();
        let root = durable_root(&parent, &identity);
        let mut wrong = identity.clone();
        wrong.deployment_generation = 2;
        assert_eq!(
            Stage7bDurableRootAuthority::validate(&root, &wrong).unwrap_err(),
            Stage7bDurableStorageError::IdentityDirectoryMismatch
        );
        let alias = root.join("..").join(root.file_name().unwrap());
        assert_eq!(
            Stage7bDurableRootAuthority::validate(alias, &identity).unwrap_err(),
            Stage7bDurableStorageError::RootNotCanonical
        );
        fs::remove_dir_all(parent).unwrap();
    }

    #[test]
    fn stage7b_b_relative_missing_and_wrong_directory_fail_closed() {
        let identity = identity();
        assert_eq!(
            Stage7bDurableRootAuthority::validate("relative", &identity).unwrap_err(),
            Stage7bDurableStorageError::RootMustBeAbsolute
        );
        let parent = parent();
        let missing =
            parent.join(Stage7bDurableRootAuthority::expected_directory_name(&identity).unwrap());
        assert_eq!(
            Stage7bDurableRootAuthority::validate(&missing, &identity).unwrap_err(),
            Stage7bDurableStorageError::RootMissing
        );
        let wrong = parent.join("wrong-identity");
        fs::create_dir(&wrong).unwrap();
        assert_eq!(
            Stage7bDurableRootAuthority::validate(&wrong, &identity).unwrap_err(),
            Stage7bDurableStorageError::IdentityDirectoryMismatch
        );
        fs::remove_dir_all(parent).unwrap();
    }

    #[test]
    fn stage7b_b_symlink_root_and_authoritative_entries_fail_closed() {
        use std::os::unix::fs::symlink;

        let identity = identity();
        let parent = parent();
        let root = durable_root(&parent, &identity);
        let root_alias = parent.join("root-alias");
        symlink(&root, &root_alias).unwrap();
        assert_eq!(
            Stage7bDurableRootAuthority::validate(&root_alias, &identity).unwrap_err(),
            Stage7bDurableStorageError::RootIsSymlink
        );

        let outside = parent.join("outside");
        fs::write(&outside, b"outside").unwrap();
        for (name, expected) in [
            (
                STAGE7B_JOURNAL_FILE,
                Stage7bDurableStorageError::UnsafeJournalPath,
            ),
            (
                STAGE7B_WRITER_LOCK_FILE,
                Stage7bDurableStorageError::UnsafeWriterLockPath,
            ),
            (
                STAGE7B_RECOVERY_SEAL_FILE,
                Stage7bDurableStorageError::UnsafeRecoverySealPath,
            ),
        ] {
            let path = root.join(name);
            symlink(&outside, &path).unwrap();
            assert_eq!(
                Stage7bDurableRootAuthority::validate(&root, &identity).unwrap_err(),
                expected
            );
            fs::remove_file(path).unwrap();
        }
        symlink(&parent, root.join(STAGE7B_TMP_DIRECTORY)).unwrap();
        assert_eq!(
            Stage7bDurableRootAuthority::validate(&root, &identity).unwrap_err(),
            Stage7bDurableStorageError::UnsafeTmpPath
        );
        fs::remove_dir_all(parent).unwrap();
    }

    #[test]
    fn stage7b_b_hard_linked_authoritative_file_is_rejected_as_alias() {
        let identity = identity();
        let parent = parent();
        let root = durable_root(&parent, &identity);
        let outside = parent.join("outside-hardlink-source");
        fs::write(&outside, b"external-authority").unwrap();
        for (name, expected) in [
            (
                STAGE7B_JOURNAL_FILE,
                Stage7bDurableStorageError::UnsafeJournalPath,
            ),
            (
                STAGE7B_WRITER_LOCK_FILE,
                Stage7bDurableStorageError::UnsafeWriterLockPath,
            ),
            (
                STAGE7B_RECOVERY_SEAL_FILE,
                Stage7bDurableStorageError::UnsafeRecoverySealPath,
            ),
        ] {
            let path = root.join(name);
            fs::hard_link(&outside, &path).unwrap();
            assert_eq!(
                Stage7bDurableRootAuthority::validate(&root, &identity).unwrap_err(),
                expected
            );
            fs::remove_file(path).unwrap();
        }
        fs::remove_dir_all(parent).unwrap();
    }

    #[test]
    fn stage7b_b_anchored_root_fd_is_not_redirected_after_path_rename() {
        let identity = identity();
        let parent = parent();
        let root = durable_root(&parent, &identity);
        let authority = Stage7bDurableRootAuthority::validate(&root, &identity).unwrap();
        let anchored = authority.root_directory.metadata().unwrap();
        let renamed = parent.join("renamed-original-root");
        fs::rename(&root, &renamed).unwrap();
        fs::create_dir(&root).unwrap();

        let witness = open_child_at(
            &authority.root_directory,
            "anchored-root-witness",
            libc::O_RDWR | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            0o600,
        )
        .unwrap();
        drop(witness);
        let still_anchored = authority.root_directory.metadata().unwrap();
        assert_eq!(
            (anchored.dev(), anchored.ino()),
            (still_anchored.dev(), still_anchored.ino())
        );
        assert!(renamed.join("anchored-root-witness").is_file());
        assert!(!root.join("anchored-root-witness").exists());
        assert_eq!(
            authority.validate_external_root_identity().unwrap_err(),
            Stage7bDurableStorageError::RootIdentityDrift
        );
        fs::remove_dir_all(parent).unwrap();
    }

    #[test]
    fn stage7b_b_parent_namespace_lock_is_identity_scoped() {
        let first_identity = identity();
        let mut second_identity = identity();
        second_identity.deployment_id = "stage7b-test-second".to_string();
        let parent = parent();
        let first_root = durable_root(&parent, &first_identity);
        let second_root = durable_root(&parent, &second_identity);
        let first = Stage7bWritableDurableAuthority::create_new(
            Stage7bDurableRootAuthority::validate(&first_root, &first_identity).unwrap(),
            &first_identity,
            &authorization(&first_identity.deployment_id),
        )
        .unwrap();
        let second = Stage7bWritableDurableAuthority::create_new(
            Stage7bDurableRootAuthority::validate(&second_root, &second_identity).unwrap(),
            &second_identity,
            &authorization(&second_identity.deployment_id),
        )
        .unwrap();
        drop((first, second));
        fs::remove_dir_all(parent).unwrap();
    }

    #[test]
    fn stage7b_b_live_authority_rejects_root_drift_before_journal_access() {
        let identity = identity();
        let parent = parent();
        let root = durable_root(&parent, &identity);
        let authority = Stage7bWritableDurableAuthority::create_new(
            Stage7bDurableRootAuthority::validate(&root, &identity).unwrap(),
            &identity,
            &authorization(&identity.deployment_id),
        )
        .unwrap();
        let renamed = parent.join("live-authority-original-root");
        fs::rename(&root, &renamed).unwrap();
        fs::create_dir(&root).unwrap();
        assert_eq!(
            authority.framed_bytes().unwrap_err(),
            Stage6JournalStorageError::ExternalMutationDetected
        );
        drop(authority);
        fs::remove_dir_all(parent).unwrap();
    }

    #[test]
    fn stage7b_b_live_authority_rejects_lock_drift_before_journal_access() {
        let identity = identity();
        let parent = parent();
        let root = durable_root(&parent, &identity);
        let authority = Stage7bWritableDurableAuthority::create_new(
            Stage7bDurableRootAuthority::validate(&root, &identity).unwrap(),
            &identity,
            &authorization(&identity.deployment_id),
        )
        .unwrap();
        fs::remove_file(root.join(STAGE7B_WRITER_LOCK_FILE)).unwrap();
        fs::write(root.join(STAGE7B_WRITER_LOCK_FILE), b"replacement").unwrap();
        assert_eq!(
            authority.framed_bytes().unwrap_err(),
            Stage6JournalStorageError::ExternalMutationDetected
        );
        drop(authority);
        fs::remove_dir_all(parent).unwrap();
    }

    #[test]
    fn stage7b_b_r2_first_boot_rebind_fails_before_any_filesystem_effect() {
        let identity_a = identity();
        let mut identity_b = identity();
        identity_b.deployment_id = "stage7b-rebound-deployment".to_string();
        let parent = parent();
        let root = durable_root(&parent, &identity_a);
        let authority = Stage7bDurableRootAuthority::validate(&root, &identity_a).unwrap();
        let result = Stage7bWritableDurableAuthority::create_new(
            authority,
            &identity_b,
            &authorization(&identity_b.deployment_id),
        );
        assert!(matches!(
            result,
            Err(Stage7bDurableStorageError::OperationalIdentityMismatch)
        ));
        assert!(!parent
            .join(format!(
                ".stage7b-{}.namespace.lock",
                stage6d_operational_identity_sha256(&identity_a)
                    .unwrap()
                    .as_str()
            ))
            .exists());
        assert!(!root.join(STAGE7B_WRITER_LOCK_FILE).exists());
        assert!(!root.join(STAGE7B_JOURNAL_FILE).exists());
        fs::remove_dir_all(parent).unwrap();
    }

    #[test]
    fn stage7b_b_r2_same_deployment_different_generation_cannot_rebind_root() {
        let identity_a = identity();
        let mut identity_b = identity();
        identity_b.deployment_generation += 1;
        assert_eq!(identity_a.deployment_id, identity_b.deployment_id);
        let parent = parent();
        let root = durable_root(&parent, &identity_a);
        let authority = Stage7bDurableRootAuthority::validate(&root, &identity_a).unwrap();
        let result = Stage7bWritableDurableAuthority::create_new(
            authority,
            &identity_b,
            &authorization(&identity_b.deployment_id),
        );
        assert!(matches!(
            result,
            Err(Stage7bDurableStorageError::OperationalIdentityMismatch)
        ));
        assert!(!root.join(STAGE7B_WRITER_LOCK_FILE).exists());
        assert!(!root.join(STAGE7B_JOURNAL_FILE).exists());
        fs::remove_dir_all(parent).unwrap();
    }

    #[test]
    fn stage7b_b_r2_restart_rebind_fails_before_lock_and_preserves_journal() {
        let identity_a = identity();
        let mut identity_b = identity();
        identity_b.gateway_instance_id = "rebound-gateway".to_string();
        let parent = parent();
        let root = durable_root(&parent, &identity_a);
        drop(
            Stage7bWritableDurableAuthority::create_new(
                Stage7bDurableRootAuthority::validate(&root, &identity_a).unwrap(),
                &identity_a,
                &authorization(&identity_a.deployment_id),
            )
            .unwrap(),
        );
        let lock_before = fs::metadata(root.join(STAGE7B_WRITER_LOCK_FILE)).unwrap();
        let journal_before = fs::read(root.join(STAGE7B_JOURNAL_FILE)).unwrap();
        let authority = Stage7bDurableRootAuthority::validate(&root, &identity_a).unwrap();
        let result = Stage7bWritableDurableAuthority::open_existing(authority, &identity_b);
        assert!(matches!(
            result,
            Err(Stage7bDurableStorageError::OperationalIdentityMismatch)
        ));
        let lock_after = fs::metadata(root.join(STAGE7B_WRITER_LOCK_FILE)).unwrap();
        assert_eq!(
            (lock_before.dev(), lock_before.ino()),
            (lock_after.dev(), lock_after.ino())
        );
        assert_eq!(
            fs::read(root.join(STAGE7B_JOURNAL_FILE)).unwrap(),
            journal_before
        );
        fs::remove_dir_all(parent).unwrap();
    }
}
