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

#[cfg(not(unix))]
compile_error!("runtime-durable-service requires Unix kernel file locking");

use std::{
    fs::{self, File, OpenOptions},
    io::ErrorKind,
    os::fd::AsRawFd,
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
};

use strategy_runtime_core::{
    stage6d_operational_identity_sha256, Stage6FileJournalBackend, Stage6JournalAppendReceipt,
    Stage6JournalBackend, Stage6JournalCheckpointV1, Stage6JournalFrontierV1,
    Stage6JournalRecordV1, Stage6JournalStorageError, Stage6Sha256Digest,
    Stage6dFirstBootAuthorization, Stage6dOperationalIdentityConfig,
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
    FirstBootAuthorizationMismatch,
    UnsafeJournalPath,
    UnsafeWriterLockPath,
    UnsafeRecoverySealPath,
    UnsafeTmpPath,
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
            Self::FirstBootAuthorizationMismatch => "first-boot authorization mismatch",
            Self::UnsafeJournalPath => "unsafe durable journal path",
            Self::UnsafeWriterLockPath => "unsafe writer-lock path",
            Self::UnsafeRecoverySealPath => "unsafe recovery-seal path",
            Self::UnsafeTmpPath => "unsafe durable tmp path",
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
pub struct Stage7bDurablePaths {
    root: PathBuf,
    operational_identity_sha256: Stage6Sha256Digest,
}

impl Stage7bDurablePaths {
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
        validate_optional_regular(
            &canonical.join(STAGE7B_JOURNAL_FILE),
            Stage7bDurableStorageError::UnsafeJournalPath,
        )?;
        validate_optional_regular(
            &canonical.join(STAGE7B_WRITER_LOCK_FILE),
            Stage7bDurableStorageError::UnsafeWriterLockPath,
        )?;
        validate_optional_regular(
            &canonical.join(STAGE7B_RECOVERY_SEAL_FILE),
            Stage7bDurableStorageError::UnsafeRecoverySealPath,
        )?;
        validate_optional_directory(
            &canonical.join(STAGE7B_TMP_DIRECTORY),
            Stage7bDurableStorageError::UnsafeTmpPath,
        )?;
        Ok(Self {
            root: canonical,
            operational_identity_sha256: digest,
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn operational_identity_sha256(&self) -> &Stage6Sha256Digest {
        &self.operational_identity_sha256
    }

    fn journal_path(&self) -> PathBuf {
        self.root.join(STAGE7B_JOURNAL_FILE)
    }

    fn writer_lock_path(&self) -> PathBuf {
        self.root.join(STAGE7B_WRITER_LOCK_FILE)
    }

    fn revalidate(
        &self,
        identity: &Stage6dOperationalIdentityConfig,
    ) -> Result<(), Stage7bDurableStorageError> {
        let observed = Self::validate(&self.root, identity)?;
        if observed.operational_identity_sha256 != self.operational_identity_sha256 {
            return Err(Stage7bDurableStorageError::IdentityDirectoryMismatch);
        }
        Ok(())
    }
}

fn validate_optional_regular(
    path: &Path,
    unsafe_error: Stage7bDurableStorageError,
) -> Result<(), Stage7bDurableStorageError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_file() && metadata.nlink() == 1 => Ok(()),
        Ok(_) => Err(unsafe_error),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn validate_optional_directory(
    path: &Path,
    unsafe_error: Stage7bDurableStorageError,
) -> Result<(), Stage7bDurableStorageError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_dir() => Ok(()),
        Ok(_) => Err(unsafe_error),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

struct Stage7bKernelWriterLease {
    file: File,
}

impl Stage7bKernelWriterLease {
    fn acquire(path: &Path) -> Result<Self, Stage7bDurableStorageError> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .mode(0o600)
            .open(path)?;
        if !same_regular_file_identity(path, &file)? {
            return Err(Stage7bDurableStorageError::UnsafeWriterLockPath);
        }
        // SAFETY: `file` owns a live descriptor for the entire lease. `flock`
        // does not dereference memory and the descriptor remains open on success.
        let result = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
        if result != 0 {
            let error = std::io::Error::last_os_error();
            if matches!(error.raw_os_error(), Some(code) if code == libc::EWOULDBLOCK || code == libc::EAGAIN)
            {
                return Err(Stage7bDurableStorageError::WriterAlreadyHeld);
            }
            return Err(error.into());
        }
        if !same_regular_file_identity(path, &file)? {
            return Err(Stage7bDurableStorageError::UnsafeWriterLockPath);
        }
        Ok(Self { file })
    }
}

fn same_regular_file_identity(
    path: &Path,
    file: &File,
) -> Result<bool, Stage7bDurableStorageError> {
    let opened = file.metadata()?;
    let named = fs::symlink_metadata(path)?;
    Ok(opened.file_type().is_file()
        && named.file_type().is_file()
        && opened.nlink() == 1
        && named.nlink() == 1
        && opened.dev() == named.dev()
        && opened.ino() == named.ino())
}

impl Drop for Stage7bKernelWriterLease {
    fn drop(&mut self) {
        // SAFETY: this descriptor is owned by the lease and remains valid until
        // `File` is dropped immediately after this method returns.
        let _ = unsafe { libc::flock(self.file.as_raw_fd(), libc::LOCK_UN) };
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
        paths: Stage7bDurablePaths,
        identity: &Stage6dOperationalIdentityConfig,
        authorization: &Stage6dFirstBootAuthorization,
    ) -> Result<Self, Stage7bDurableStorageError> {
        if !authorization.authorizes_deployment(&identity.deployment_id) {
            return Err(Stage7bDurableStorageError::FirstBootAuthorizationMismatch);
        }
        Self::open(paths, identity, true)
    }

    pub fn open_existing(
        paths: Stage7bDurablePaths,
        identity: &Stage6dOperationalIdentityConfig,
    ) -> Result<Self, Stage7bDurableStorageError> {
        Self::open(paths, identity, false)
    }

    fn open(
        paths: Stage7bDurablePaths,
        identity: &Stage6dOperationalIdentityConfig,
        create: bool,
    ) -> Result<Self, Stage7bDurableStorageError> {
        let writer_lease = Stage7bKernelWriterLease::acquire(&paths.writer_lock_path())?;
        paths.revalidate(identity)?;
        let journal = if create {
            Stage6FileJournalBackend::create_new(paths.journal_path())?
        } else {
            Stage6FileJournalBackend::open_existing(paths.journal_path())?
        };
        Ok(Self {
            journal,
            _writer_lease: writer_lease,
            operational_identity_sha256: paths.operational_identity_sha256,
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
        self.journal.append(record)
    }

    fn records(&self) -> &[Stage6JournalRecordV1] {
        self.journal.records()
    }

    fn frontier(&self) -> &Stage6JournalFrontierV1 {
        self.journal.frontier()
    }

    fn framed_bytes(&self) -> Result<Vec<u8>, Stage6JournalStorageError> {
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
        let root = parent.join(Stage7bDurablePaths::expected_directory_name(identity).unwrap());
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
        let paths = Stage7bDurablePaths::validate(&root, &identity).unwrap();
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
        let paths = Stage7bDurablePaths::validate(&root, &identity).unwrap();
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
            Stage7bDurablePaths::validate(&root, &wrong).unwrap_err(),
            Stage7bDurableStorageError::IdentityDirectoryMismatch
        );
        let alias = root.join("..").join(root.file_name().unwrap());
        assert_eq!(
            Stage7bDurablePaths::validate(alias, &identity).unwrap_err(),
            Stage7bDurableStorageError::RootNotCanonical
        );
        fs::remove_dir_all(parent).unwrap();
    }

    #[test]
    fn stage7b_b_relative_missing_and_wrong_directory_fail_closed() {
        let identity = identity();
        assert_eq!(
            Stage7bDurablePaths::validate("relative", &identity).unwrap_err(),
            Stage7bDurableStorageError::RootMustBeAbsolute
        );
        let parent = parent();
        let missing = parent.join(Stage7bDurablePaths::expected_directory_name(&identity).unwrap());
        assert_eq!(
            Stage7bDurablePaths::validate(&missing, &identity).unwrap_err(),
            Stage7bDurableStorageError::RootMissing
        );
        let wrong = parent.join("wrong-identity");
        fs::create_dir(&wrong).unwrap();
        assert_eq!(
            Stage7bDurablePaths::validate(&wrong, &identity).unwrap_err(),
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
            Stage7bDurablePaths::validate(&root_alias, &identity).unwrap_err(),
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
                Stage7bDurablePaths::validate(&root, &identity).unwrap_err(),
                expected
            );
            fs::remove_file(path).unwrap();
        }
        symlink(&parent, root.join(STAGE7B_TMP_DIRECTORY)).unwrap();
        assert_eq!(
            Stage7bDurablePaths::validate(&root, &identity).unwrap_err(),
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
                Stage7bDurablePaths::validate(&root, &identity).unwrap_err(),
                expected
            );
            fs::remove_file(path).unwrap();
        }
        fs::remove_dir_all(parent).unwrap();
    }
}
