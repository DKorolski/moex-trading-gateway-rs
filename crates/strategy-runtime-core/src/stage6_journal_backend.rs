//! Stage 6B isolated append-only journal storage.
//!
//! This module owns physical framing, durability receipts and corruption
//! detection only. It deliberately has no lifecycle replay, runtime callback,
//! Redis, FINAM, broker dispatch, worker or live-execution authority.
//!
//! The filesystem writer is intentionally non-cloneable:
//!
//! ```compile_fail
//! use strategy_runtime_core::Stage6FileJournalBackend;
//! fn require_clone<T: Clone>() {}
//! require_clone::<Stage6FileJournalBackend>();
//! ```

use crate::{
    Stage6DurableIdentityError, Stage6JournalRecordId, Stage6JournalRecordV1,
    Stage6LifecycleSequence,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{File, OpenOptions};
use std::io::{ErrorKind, Read, Seek, SeekFrom, Write};
#[cfg(unix)]
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

pub const STAGE6_JOURNAL_STORAGE_SCHEMA_VERSION: u16 = 1;
pub const STAGE6_JOURNAL_MAX_RECORD_BYTES: usize = 1024 * 1024;

const JOURNAL_MAGIC: &[u8; 8] = b"S6JNLV1\0";
const JOURNAL_HEADER_BYTES: usize = JOURNAL_MAGIC.len() + 2;
const FRAME_MAGIC: &[u8; 4] = b"S6F1";
const FRAME_VERSION: u16 = 1;
const FRAME_PREFIX_BYTES: usize = FRAME_MAGIC.len() + 2 + 4 + 32;
const FRAME_HASH_BYTES: usize = 32;
const FRAME_HASH_DOMAIN: &[u8] = b"stage6-journal-frame-v1";
const FRAME_GENESIS_DOMAIN: &[u8] = b"stage6-journal-frame-genesis-v1";
const CHECKPOINT_HASH_DOMAIN: &[u8] = b"stage6-journal-checkpoint-v1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stage6JournalStorageError {
    UnsupportedStorageSchema { found: u16 },
    InvalidJournalHeader,
    InvalidFrameHeader,
    InvalidFrameLength { declared: u64 },
    TornFrame,
    FrameHashMismatch,
    FrameChainMismatch,
    NonCanonicalRecord,
    RecordDecodeFailed { source: Stage6DurableIdentityError },
    TrailingGarbage,
    DurabilityUncertain,
    CheckpointInvalid,
    ExternalMutationDetected,
    Io { kind: ErrorKind },
}

impl std::fmt::Display for Stage6JournalStorageError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedStorageSchema { found } => {
                write!(
                    formatter,
                    "unsupported Stage 6 journal storage schema {found}"
                )
            }
            Self::InvalidJournalHeader => formatter.write_str("invalid Stage 6 journal header"),
            Self::InvalidFrameHeader => formatter.write_str("invalid Stage 6 journal frame header"),
            Self::InvalidFrameLength { declared } => {
                write!(formatter, "invalid Stage 6 journal frame length {declared}")
            }
            Self::TornFrame => formatter.write_str("torn Stage 6 journal frame"),
            Self::FrameHashMismatch => formatter.write_str("Stage 6 journal frame hash mismatch"),
            Self::FrameChainMismatch => formatter.write_str("Stage 6 journal frame chain mismatch"),
            Self::NonCanonicalRecord => formatter.write_str("non-canonical Stage 6 journal record"),
            Self::RecordDecodeFailed { source } => {
                write!(formatter, "Stage 6 journal record decode failed: {source}")
            }
            Self::TrailingGarbage => formatter.write_str("trailing Stage 6 journal garbage"),
            Self::DurabilityUncertain => {
                formatter.write_str("Stage 6 journal durability uncertain")
            }
            Self::CheckpointInvalid => formatter.write_str("invalid Stage 6 journal checkpoint"),
            Self::ExternalMutationDetected => {
                formatter.write_str("external Stage 6 journal mutation detected")
            }
            Self::Io { kind } => write!(formatter, "Stage 6 journal I/O error: {kind:?}"),
        }
    }
}

impl std::error::Error for Stage6JournalStorageError {}

impl From<std::io::Error> for Stage6JournalStorageError {
    fn from(error: std::io::Error) -> Self {
        Self::Io { kind: error.kind() }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Stage6JournalFrontierV1 {
    storage_schema_version: u16,
    frame_count: u64,
    journal_byte_length: u64,
    last_frame_sha256: String,
    last_record_id: Option<Stage6JournalRecordId>,
    last_lifecycle_sequence: Option<Stage6LifecycleSequence>,
}

impl Stage6JournalFrontierV1 {
    fn empty() -> Self {
        Self {
            storage_schema_version: STAGE6_JOURNAL_STORAGE_SCHEMA_VERSION,
            frame_count: 0,
            journal_byte_length: JOURNAL_HEADER_BYTES as u64,
            last_frame_sha256: hex_digest(&genesis_digest()),
            last_record_id: None,
            last_lifecycle_sequence: None,
        }
    }

    fn validate(&self) -> Result<(), Stage6JournalStorageError> {
        if self.storage_schema_version != STAGE6_JOURNAL_STORAGE_SCHEMA_VERSION {
            return Err(Stage6JournalStorageError::UnsupportedStorageSchema {
                found: self.storage_schema_version,
            });
        }
        if !is_lower_hex_digest(&self.last_frame_sha256) {
            return Err(Stage6JournalStorageError::CheckpointInvalid);
        }
        if self.frame_count == 0 {
            if self.journal_byte_length != JOURNAL_HEADER_BYTES as u64
                || self.last_frame_sha256 != hex_digest(&genesis_digest())
                || self.last_record_id.is_some()
                || self.last_lifecycle_sequence.is_some()
            {
                return Err(Stage6JournalStorageError::CheckpointInvalid);
            }
        } else if self.journal_byte_length <= JOURNAL_HEADER_BYTES as u64
            || self.last_record_id.is_none()
            || self.last_lifecycle_sequence.is_none()
        {
            return Err(Stage6JournalStorageError::CheckpointInvalid);
        }
        Ok(())
    }

    pub fn storage_schema_version(&self) -> u16 {
        self.storage_schema_version
    }
    pub fn frame_count(&self) -> u64 {
        self.frame_count
    }
    pub fn journal_byte_length(&self) -> u64 {
        self.journal_byte_length
    }
    pub fn last_frame_sha256(&self) -> &str {
        &self.last_frame_sha256
    }
    pub fn last_record_id(&self) -> Option<&Stage6JournalRecordId> {
        self.last_record_id.as_ref()
    }
    pub fn last_lifecycle_sequence(&self) -> Option<Stage6LifecycleSequence> {
        self.last_lifecycle_sequence
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Stage6JournalFrontierWireV1 {
    storage_schema_version: u16,
    frame_count: u64,
    journal_byte_length: u64,
    last_frame_sha256: String,
    last_record_id: Option<Stage6JournalRecordId>,
    last_lifecycle_sequence: Option<Stage6LifecycleSequence>,
}

impl<'de> Deserialize<'de> for Stage6JournalFrontierV1 {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let wire = Stage6JournalFrontierWireV1::deserialize(deserializer)?;
        let value = Self {
            storage_schema_version: wire.storage_schema_version,
            frame_count: wire.frame_count,
            journal_byte_length: wire.journal_byte_length,
            last_frame_sha256: wire.last_frame_sha256,
            last_record_id: wire.last_record_id,
            last_lifecycle_sequence: wire.last_lifecycle_sequence,
        };
        value.validate().map_err(serde::de::Error::custom)?;
        Ok(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Stage6JournalCheckpointV1 {
    storage_schema_version: u16,
    frontier: Stage6JournalFrontierV1,
    checkpoint_sha256: String,
}

impl Stage6JournalCheckpointV1 {
    pub fn from_frontier(
        frontier: Stage6JournalFrontierV1,
    ) -> Result<Self, Stage6JournalStorageError> {
        frontier.validate()?;
        let checkpoint_sha256 = checkpoint_digest(&frontier);
        Ok(Self {
            storage_schema_version: STAGE6_JOURNAL_STORAGE_SCHEMA_VERSION,
            frontier,
            checkpoint_sha256,
        })
    }

    pub fn encode_canonical(&self) -> Vec<u8> {
        serde_json::to_vec(self).expect("fixed Stage 6B checkpoint serializes")
    }

    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, Stage6JournalStorageError> {
        let value: Self = serde_json::from_slice(bytes)
            .map_err(|_| Stage6JournalStorageError::CheckpointInvalid)?;
        value.validate()?;
        if value.encode_canonical() != bytes {
            return Err(Stage6JournalStorageError::CheckpointInvalid);
        }
        Ok(value)
    }

    fn validate(&self) -> Result<(), Stage6JournalStorageError> {
        if self.storage_schema_version != STAGE6_JOURNAL_STORAGE_SCHEMA_VERSION {
            return Err(Stage6JournalStorageError::UnsupportedStorageSchema {
                found: self.storage_schema_version,
            });
        }
        self.frontier.validate()?;
        if self.checkpoint_sha256 != checkpoint_digest(&self.frontier) {
            return Err(Stage6JournalStorageError::CheckpointInvalid);
        }
        Ok(())
    }

    pub fn frontier(&self) -> &Stage6JournalFrontierV1 {
        &self.frontier
    }
    pub fn checkpoint_sha256(&self) -> &str {
        &self.checkpoint_sha256
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Stage6JournalCheckpointWireV1 {
    storage_schema_version: u16,
    frontier: Stage6JournalFrontierV1,
    checkpoint_sha256: String,
}

impl<'de> Deserialize<'de> for Stage6JournalCheckpointV1 {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let wire = Stage6JournalCheckpointWireV1::deserialize(deserializer)?;
        let value = Self {
            storage_schema_version: wire.storage_schema_version,
            frontier: wire.frontier,
            checkpoint_sha256: wire.checkpoint_sha256,
        };
        value.validate().map_err(serde::de::Error::custom)?;
        Ok(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stage6JournalAppendReceipt {
    frame_index: u64,
    frame_start_offset: u64,
    frame_end_offset: u64,
    record_id: Stage6JournalRecordId,
    lifecycle_sequence: Stage6LifecycleSequence,
    frame_sha256: String,
    durable_frontier: Stage6JournalFrontierV1,
}

impl Stage6JournalAppendReceipt {
    pub fn frame_index(&self) -> u64 {
        self.frame_index
    }
    pub fn frame_start_offset(&self) -> u64 {
        self.frame_start_offset
    }
    pub fn frame_end_offset(&self) -> u64 {
        self.frame_end_offset
    }
    pub fn record_id(&self) -> &Stage6JournalRecordId {
        &self.record_id
    }
    pub fn lifecycle_sequence(&self) -> Stage6LifecycleSequence {
        self.lifecycle_sequence
    }
    pub fn frame_sha256(&self) -> &str {
        &self.frame_sha256
    }
    pub fn durable_frontier(&self) -> &Stage6JournalFrontierV1 {
        &self.durable_frontier
    }
}

pub trait Stage6JournalBackend {
    fn append(
        &mut self,
        record: &Stage6JournalRecordV1,
    ) -> Result<Stage6JournalAppendReceipt, Stage6JournalStorageError>;
    fn records(&self) -> &[Stage6JournalRecordV1];
    fn frontier(&self) -> &Stage6JournalFrontierV1;
    fn framed_bytes(&self) -> Result<Vec<u8>, Stage6JournalStorageError>;
    fn validate_checkpoint(
        &self,
        checkpoint: &Stage6JournalCheckpointV1,
    ) -> Result<(), Stage6JournalStorageError>;
}

#[derive(Debug)]
pub struct Stage6MemoryJournalBackend {
    bytes: Vec<u8>,
    scan: ScannedJournal,
}

impl Stage6MemoryJournalBackend {
    pub fn new() -> Self {
        let bytes = journal_header().to_vec();
        Self {
            scan: scan_bytes(&bytes).expect("fixed empty Stage 6B journal is valid"),
            bytes,
        }
    }

    pub fn from_framed_bytes(bytes: Vec<u8>) -> Result<Self, Stage6JournalStorageError> {
        let scan = scan_bytes(&bytes)?;
        Ok(Self { bytes, scan })
    }
}

impl Default for Stage6MemoryJournalBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl Stage6JournalBackend for Stage6MemoryJournalBackend {
    fn append(
        &mut self,
        record: &Stage6JournalRecordV1,
    ) -> Result<Stage6JournalAppendReceipt, Stage6JournalStorageError> {
        let record_bytes = validate_record_for_storage(record)?;
        let start = self.scan.frontier.journal_byte_length;
        let previous = self.scan.last_frame_digest;
        let frame = encode_frame(&record_bytes, previous)?;
        self.bytes.extend_from_slice(&frame.bytes);
        self.scan = scan_bytes(&self.bytes)?;
        Ok(append_receipt(
            record,
            start,
            &frame.digest,
            &self.scan.frontier,
        ))
    }

    fn records(&self) -> &[Stage6JournalRecordV1] {
        &self.scan.records
    }
    fn frontier(&self) -> &Stage6JournalFrontierV1 {
        &self.scan.frontier
    }
    fn framed_bytes(&self) -> Result<Vec<u8>, Stage6JournalStorageError> {
        Ok(self.bytes.clone())
    }
    fn validate_checkpoint(
        &self,
        checkpoint: &Stage6JournalCheckpointV1,
    ) -> Result<(), Stage6JournalStorageError> {
        validate_checkpoint_against_scan(checkpoint, &self.scan)
    }
}

/// The single journal authority owned by a recovered runtime.
///
/// The enum is deliberately non-cloneable and non-serializable. Selecting a
/// variant transfers one backend into the runtime; it does not create a
/// memory/file mirror or a dual-write path.
#[derive(Debug)]
pub enum Stage6OwnedJournalBackend {
    Memory(Stage6MemoryJournalBackend),
    File(Stage6FileJournalBackend),
}

impl Stage6OwnedJournalBackend {
    pub fn memory() -> Self {
        Self::Memory(Stage6MemoryJournalBackend::new())
    }

    pub fn from_memory(backend: Stage6MemoryJournalBackend) -> Self {
        Self::Memory(backend)
    }

    pub fn from_file(backend: Stage6FileJournalBackend) -> Self {
        Self::File(backend)
    }

    pub fn is_file_backed(&self) -> bool {
        matches!(self, Self::File(_))
    }
}

impl From<Stage6MemoryJournalBackend> for Stage6OwnedJournalBackend {
    fn from(backend: Stage6MemoryJournalBackend) -> Self {
        Self::from_memory(backend)
    }
}

impl From<Stage6FileJournalBackend> for Stage6OwnedJournalBackend {
    fn from(backend: Stage6FileJournalBackend) -> Self {
        Self::from_file(backend)
    }
}

impl Stage6JournalBackend for Stage6OwnedJournalBackend {
    fn append(
        &mut self,
        record: &Stage6JournalRecordV1,
    ) -> Result<Stage6JournalAppendReceipt, Stage6JournalStorageError> {
        match self {
            Self::Memory(backend) => backend.append(record),
            Self::File(backend) => backend.append(record),
        }
    }

    fn records(&self) -> &[Stage6JournalRecordV1] {
        match self {
            Self::Memory(backend) => backend.records(),
            Self::File(backend) => backend.records(),
        }
    }

    fn frontier(&self) -> &Stage6JournalFrontierV1 {
        match self {
            Self::Memory(backend) => backend.frontier(),
            Self::File(backend) => backend.frontier(),
        }
    }

    fn framed_bytes(&self) -> Result<Vec<u8>, Stage6JournalStorageError> {
        match self {
            Self::Memory(backend) => backend.framed_bytes(),
            Self::File(backend) => backend.framed_bytes(),
        }
    }

    fn validate_checkpoint(
        &self,
        checkpoint: &Stage6JournalCheckpointV1,
    ) -> Result<(), Stage6JournalStorageError> {
        match self {
            Self::Memory(backend) => backend.validate_checkpoint(checkpoint),
            Self::File(backend) => backend.validate_checkpoint(checkpoint),
        }
    }
}

#[derive(Debug)]
pub struct Stage6FileJournalBackend {
    _diagnostic_path: PathBuf,
    file: File,
    scan: ScannedJournal,
    durability_uncertain: bool,
    #[cfg(test)]
    failpoint: Option<TestIoFailpoint>,
}

impl Stage6FileJournalBackend {
    /// Creates a new journal and fails if the path already exists.
    pub fn create_new(path: impl AsRef<Path>) -> Result<Self, Stage6JournalStorageError> {
        let path = path.as_ref().to_path_buf();
        let mut options = OpenOptions::new();
        options.read(true).write(true).create_new(true);
        #[cfg(unix)]
        options
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .mode(0o600);
        let mut file = options.open(&path)?;
        validate_open_file_identity(&path, &file)?;
        file.write_all(&journal_header())?;
        file.sync_data()?;
        sync_parent_directory(&path)?;
        Self::from_validated_file(path, file)
    }

    /// Opens and validates an existing journal without creating or repairing
    /// any bytes. Missing, empty, torn and corrupt journals fail closed.
    pub fn open_existing(path: impl AsRef<Path>) -> Result<Self, Stage6JournalStorageError> {
        let path = path.as_ref().to_path_buf();
        let mut options = OpenOptions::new();
        options.read(true).write(true);
        #[cfg(unix)]
        options.custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
        let file = options.open(&path)?;
        validate_open_file_identity(&path, &file)?;
        Self::from_validated_file(path, file)
    }

    /// Adopts an already-opened, exclusively-created journal capability.
    ///
    /// The caller owns pathname resolution and must have opened `file` with
    /// create-new semantics. This constructor verifies that the descriptor is
    /// a single-linked regular file, initializes the canonical journal header,
    /// and durably syncs the file. The caller remains responsible for syncing
    /// the directory capability through which the file was created.
    pub fn create_new_from_owned_file(
        diagnostic_path: impl Into<PathBuf>,
        mut file: File,
    ) -> Result<Self, Stage6JournalStorageError> {
        validate_owned_regular_file(&file)?;
        file.write_all(&journal_header())?;
        file.sync_data()?;
        Self::from_validated_file(diagnostic_path.into(), file)
    }

    /// Adopts an already-opened existing journal capability without resolving
    /// its diagnostic pathname again.
    pub fn open_existing_from_owned_file(
        diagnostic_path: impl Into<PathBuf>,
        file: File,
    ) -> Result<Self, Stage6JournalStorageError> {
        validate_owned_regular_file(&file)?;
        Self::from_validated_file(diagnostic_path.into(), file)
    }

    #[cfg(test)]
    fn open_for_test(path: impl AsRef<Path>) -> Result<Self, Stage6JournalStorageError> {
        let path = path.as_ref();
        if path.exists() {
            Self::open_existing(path)
        } else {
            Self::create_new(path)
        }
    }

    fn from_validated_file(
        path: PathBuf,
        mut file: File,
    ) -> Result<Self, Stage6JournalStorageError> {
        let length = file.metadata()?.len();
        file.seek(SeekFrom::Start(0))?;
        let scan = scan_reader(&mut file, length)?;
        file.seek(SeekFrom::End(0))?;
        Ok(Self {
            _diagnostic_path: path,
            file,
            scan,
            durability_uncertain: false,
            #[cfg(test)]
            failpoint: None,
        })
    }

    fn verify_external_frontier(&mut self) -> Result<(), Stage6JournalStorageError> {
        let actual = self.file.metadata()?.len();
        if actual != self.scan.frontier.journal_byte_length {
            return Err(Stage6JournalStorageError::ExternalMutationDetected);
        }

        // Length plus the stored tail digest is not sufficient: an external
        // writer can alter an earlier record without changing either. Rescan
        // the complete pre-existing authority before writing a single byte.
        self.file.seek(SeekFrom::Start(0))?;
        let observed = scan_reader(&mut self.file, actual).map_err(|error| match error {
            Stage6JournalStorageError::Io { .. } => error,
            _ => Stage6JournalStorageError::ExternalMutationDetected,
        })?;
        if observed.records != self.scan.records
            || observed.frontiers != self.scan.frontiers
            || observed.frontier != self.scan.frontier
            || observed.last_frame_digest != self.scan.last_frame_digest
        {
            return Err(Stage6JournalStorageError::ExternalMutationDetected);
        }
        self.file.seek(SeekFrom::End(0))?;
        Ok(())
    }
}

fn validate_open_file_identity(path: &Path, file: &File) -> Result<(), Stage6JournalStorageError> {
    #[cfg(unix)]
    {
        let opened = file.metadata()?;
        let named = std::fs::symlink_metadata(path)?;
        if !opened.file_type().is_file()
            || !named.file_type().is_file()
            || opened.nlink() != 1
            || named.nlink() != 1
            || opened.dev() != named.dev()
            || opened.ino() != named.ino()
        {
            return Err(Stage6JournalStorageError::ExternalMutationDetected);
        }
    }
    Ok(())
}

fn validate_owned_regular_file(file: &File) -> Result<(), Stage6JournalStorageError> {
    let opened = file.metadata()?;
    if !opened.file_type().is_file() {
        return Err(Stage6JournalStorageError::ExternalMutationDetected);
    }
    #[cfg(unix)]
    if opened.nlink() != 1 {
        return Err(Stage6JournalStorageError::ExternalMutationDetected);
    }
    Ok(())
}

fn sync_parent_directory(path: &Path) -> Result<(), Stage6JournalStorageError> {
    let parent = path.parent().ok_or(Stage6JournalStorageError::Io {
        kind: ErrorKind::InvalidInput,
    })?;
    File::open(parent)?.sync_all()?;
    Ok(())
}

impl Stage6JournalBackend for Stage6FileJournalBackend {
    fn append(
        &mut self,
        record: &Stage6JournalRecordV1,
    ) -> Result<Stage6JournalAppendReceipt, Stage6JournalStorageError> {
        if self.durability_uncertain {
            return Err(Stage6JournalStorageError::DurabilityUncertain);
        }
        self.verify_external_frontier()?;
        let record_bytes = validate_record_for_storage(record)?;
        let start = self.scan.frontier.journal_byte_length;
        let frame = encode_frame(&record_bytes, self.scan.last_frame_digest)?;

        self.file.write_all(&frame.prefix)?;
        #[cfg(test)]
        if self.failpoint == Some(TestIoFailpoint::AfterFrameHeaderWrite) {
            return Err(Stage6JournalStorageError::Io {
                kind: ErrorKind::Other,
            });
        }
        #[cfg(test)]
        if self.failpoint == Some(TestIoFailpoint::AfterPartialRecordWrite) {
            let partial = record_bytes.len().div_ceil(2);
            self.file.write_all(&record_bytes[..partial])?;
            return Err(Stage6JournalStorageError::Io {
                kind: ErrorKind::Other,
            });
        }
        self.file.write_all(&record_bytes)?;
        self.file.write_all(&frame.hash_bytes)?;
        #[cfg(test)]
        if self.failpoint == Some(TestIoFailpoint::AfterFrameHashWrite) {
            return Err(Stage6JournalStorageError::Io {
                kind: ErrorKind::Other,
            });
        }
        #[cfg(test)]
        if self.failpoint == Some(TestIoFailpoint::BeforeSync) {
            return Err(Stage6JournalStorageError::Io {
                kind: ErrorKind::Other,
            });
        }
        #[cfg(test)]
        if self.failpoint == Some(TestIoFailpoint::SyncFailure) {
            self.durability_uncertain = true;
            return Err(Stage6JournalStorageError::DurabilityUncertain);
        }
        if self.file.sync_data().is_err() {
            self.durability_uncertain = true;
            return Err(Stage6JournalStorageError::DurabilityUncertain);
        }

        let length = self.file.metadata()?.len();
        self.file.seek(SeekFrom::Start(0))?;
        self.scan = scan_reader(&mut self.file, length)?;
        self.file.seek(SeekFrom::End(0))?;
        Ok(append_receipt(
            record,
            start,
            &frame.digest,
            &self.scan.frontier,
        ))
    }

    fn records(&self) -> &[Stage6JournalRecordV1] {
        &self.scan.records
    }
    fn frontier(&self) -> &Stage6JournalFrontierV1 {
        &self.scan.frontier
    }
    fn framed_bytes(&self) -> Result<Vec<u8>, Stage6JournalStorageError> {
        // Read through the already-owned file capability. Re-resolving
        // `self.path` here would allow a renamed/replaced durable namespace to
        // redirect a read to a different inode.
        let mut file = self.file.try_clone()?;
        file.seek(SeekFrom::Start(0))?;
        let length = file.metadata()?.len();
        if length != self.scan.frontier.journal_byte_length {
            return Err(Stage6JournalStorageError::ExternalMutationDetected);
        }
        let scanned = scan_reader(&mut file, length)?;
        if scanned.frontier != self.scan.frontier {
            return Err(Stage6JournalStorageError::ExternalMutationDetected);
        }
        file.seek(SeekFrom::Start(0))?;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)?;
        Ok(bytes)
    }
    fn validate_checkpoint(
        &self,
        checkpoint: &Stage6JournalCheckpointV1,
    ) -> Result<(), Stage6JournalStorageError> {
        validate_checkpoint_against_scan(checkpoint, &self.scan)
    }
}

#[derive(Debug)]
struct EncodedFrame {
    prefix: Vec<u8>,
    hash_bytes: [u8; FRAME_HASH_BYTES],
    digest: [u8; FRAME_HASH_BYTES],
    bytes: Vec<u8>,
}

#[derive(Debug)]
struct ScannedJournal {
    records: Vec<Stage6JournalRecordV1>,
    frontiers: Vec<Stage6JournalFrontierV1>,
    frontier: Stage6JournalFrontierV1,
    last_frame_digest: [u8; FRAME_HASH_BYTES],
}

fn journal_header() -> [u8; JOURNAL_HEADER_BYTES] {
    let mut header = [0_u8; JOURNAL_HEADER_BYTES];
    header[..JOURNAL_MAGIC.len()].copy_from_slice(JOURNAL_MAGIC);
    header[JOURNAL_MAGIC.len()..]
        .copy_from_slice(&STAGE6_JOURNAL_STORAGE_SCHEMA_VERSION.to_be_bytes());
    header
}

fn genesis_digest() -> [u8; FRAME_HASH_BYTES] {
    Sha256::digest(FRAME_GENESIS_DOMAIN).into()
}

fn frame_digest(
    record_length: u32,
    previous: &[u8; FRAME_HASH_BYTES],
    record_bytes: &[u8],
) -> [u8; FRAME_HASH_BYTES] {
    let mut hasher = Sha256::new();
    hasher.update(FRAME_HASH_DOMAIN);
    hasher.update(FRAME_VERSION.to_be_bytes());
    hasher.update(record_length.to_be_bytes());
    hasher.update(previous);
    hasher.update(record_bytes);
    hasher.finalize().into()
}

fn encode_frame(
    record_bytes: &[u8],
    previous: [u8; FRAME_HASH_BYTES],
) -> Result<EncodedFrame, Stage6JournalStorageError> {
    let length = validate_record_length(record_bytes.len() as u64)?;
    let mut prefix = Vec::with_capacity(FRAME_PREFIX_BYTES);
    prefix.extend_from_slice(FRAME_MAGIC);
    prefix.extend_from_slice(&FRAME_VERSION.to_be_bytes());
    prefix.extend_from_slice(&length.to_be_bytes());
    prefix.extend_from_slice(&previous);
    let digest = frame_digest(length, &previous, record_bytes);
    let mut bytes = Vec::with_capacity(prefix.len() + record_bytes.len() + digest.len());
    bytes.extend_from_slice(&prefix);
    bytes.extend_from_slice(record_bytes);
    bytes.extend_from_slice(&digest);
    Ok(EncodedFrame {
        prefix,
        hash_bytes: digest,
        digest,
        bytes,
    })
}

fn validate_record_for_storage(
    record: &Stage6JournalRecordV1,
) -> Result<Vec<u8>, Stage6JournalStorageError> {
    let bytes = record.encode_canonical();
    validate_record_length(bytes.len() as u64)?;
    decode_persisted_record(&bytes)?;
    Ok(bytes)
}

fn validate_record_length(length: u64) -> Result<u32, Stage6JournalStorageError> {
    if length == 0 || length > STAGE6_JOURNAL_MAX_RECORD_BYTES as u64 || length > u32::MAX as u64 {
        return Err(Stage6JournalStorageError::InvalidFrameLength { declared: length });
    }
    Ok(length as u32)
}

fn decode_persisted_record(
    bytes: &[u8],
) -> Result<Stage6JournalRecordV1, Stage6JournalStorageError> {
    Stage6JournalRecordV1::decode_canonical(bytes).map_err(|source| {
        if source == Stage6DurableIdentityError::NonCanonicalEncoding {
            Stage6JournalStorageError::NonCanonicalRecord
        } else {
            Stage6JournalStorageError::RecordDecodeFailed { source }
        }
    })
}

fn scan_bytes(bytes: &[u8]) -> Result<ScannedJournal, Stage6JournalStorageError> {
    scan_reader(&mut std::io::Cursor::new(bytes), bytes.len() as u64)
}

fn scan_reader(
    reader: &mut (impl Read + Seek),
    total_length: u64,
) -> Result<ScannedJournal, Stage6JournalStorageError> {
    if total_length < JOURNAL_HEADER_BYTES as u64 {
        return Err(Stage6JournalStorageError::InvalidJournalHeader);
    }
    reader.seek(SeekFrom::Start(0))?;
    let mut header = [0_u8; JOURNAL_HEADER_BYTES];
    reader
        .read_exact(&mut header)
        .map_err(|_| Stage6JournalStorageError::InvalidJournalHeader)?;
    if &header[..JOURNAL_MAGIC.len()] != JOURNAL_MAGIC {
        return Err(Stage6JournalStorageError::InvalidJournalHeader);
    }
    let storage_version = u16::from_be_bytes(
        header[JOURNAL_MAGIC.len()..]
            .try_into()
            .expect("fixed storage version width"),
    );
    if storage_version != STAGE6_JOURNAL_STORAGE_SCHEMA_VERSION {
        return Err(Stage6JournalStorageError::UnsupportedStorageSchema {
            found: storage_version,
        });
    }

    let mut offset = JOURNAL_HEADER_BYTES as u64;
    let mut previous = genesis_digest();
    let mut records = Vec::new();
    let mut frontiers = Vec::new();
    while offset < total_length {
        let remaining = total_length - offset;
        if remaining < (FRAME_PREFIX_BYTES + FRAME_HASH_BYTES) as u64 {
            let mut tail = vec![0_u8; remaining as usize];
            reader
                .read_exact(&mut tail)
                .map_err(|_| Stage6JournalStorageError::TornFrame)?;
            if FRAME_MAGIC.starts_with(&tail[..tail.len().min(FRAME_MAGIC.len())]) {
                return Err(Stage6JournalStorageError::TornFrame);
            }
            return Err(Stage6JournalStorageError::TrailingGarbage);
        }

        let mut prefix = [0_u8; FRAME_PREFIX_BYTES];
        reader
            .read_exact(&mut prefix)
            .map_err(|_| Stage6JournalStorageError::TornFrame)?;
        if &prefix[..FRAME_MAGIC.len()] != FRAME_MAGIC {
            return Err(Stage6JournalStorageError::InvalidFrameHeader);
        }
        let frame_version =
            u16::from_be_bytes(prefix[4..6].try_into().expect("fixed frame version width"));
        if frame_version != FRAME_VERSION {
            return Err(Stage6JournalStorageError::InvalidFrameHeader);
        }
        let declared = u32::from_be_bytes(prefix[6..10].try_into().expect("fixed length width"));
        let record_length = validate_record_length(u64::from(declared))? as usize;
        let frame_total = FRAME_PREFIX_BYTES
            .checked_add(record_length)
            .and_then(|value| value.checked_add(FRAME_HASH_BYTES))
            .ok_or(Stage6JournalStorageError::InvalidFrameLength {
                declared: u64::from(declared),
            })?;
        if frame_total as u64 > remaining {
            return Err(Stage6JournalStorageError::TornFrame);
        }
        let stored_previous: [u8; FRAME_HASH_BYTES] = prefix[10..42]
            .try_into()
            .expect("fixed previous digest width");
        if stored_previous != previous {
            return Err(Stage6JournalStorageError::FrameChainMismatch);
        }

        let mut record_bytes = vec![0_u8; record_length];
        reader
            .read_exact(&mut record_bytes)
            .map_err(|_| Stage6JournalStorageError::TornFrame)?;
        let mut stored_hash = [0_u8; FRAME_HASH_BYTES];
        reader
            .read_exact(&mut stored_hash)
            .map_err(|_| Stage6JournalStorageError::TornFrame)?;
        let computed = frame_digest(declared, &stored_previous, &record_bytes);
        if stored_hash != computed {
            return Err(Stage6JournalStorageError::FrameHashMismatch);
        }
        let record = decode_persisted_record(&record_bytes)?;
        offset = offset.checked_add(frame_total as u64).ok_or(
            Stage6JournalStorageError::InvalidFrameLength {
                declared: u64::from(declared),
            },
        )?;
        previous = computed;
        records.push(record);
        let last = records.last().expect("record was just appended");
        frontiers.push(Stage6JournalFrontierV1 {
            storage_schema_version: STAGE6_JOURNAL_STORAGE_SCHEMA_VERSION,
            frame_count: records.len() as u64,
            journal_byte_length: offset,
            last_frame_sha256: hex_digest(&computed),
            last_record_id: Some(last.journal_record_id().clone()),
            last_lifecycle_sequence: Some(last.lifecycle_sequence()),
        });
    }
    let frontier = frontiers
        .last()
        .cloned()
        .unwrap_or_else(Stage6JournalFrontierV1::empty);
    frontier.validate()?;
    Ok(ScannedJournal {
        records,
        frontiers,
        frontier,
        last_frame_digest: previous,
    })
}

fn append_receipt(
    record: &Stage6JournalRecordV1,
    start: u64,
    digest: &[u8; FRAME_HASH_BYTES],
    frontier: &Stage6JournalFrontierV1,
) -> Stage6JournalAppendReceipt {
    Stage6JournalAppendReceipt {
        frame_index: frontier.frame_count - 1,
        frame_start_offset: start,
        frame_end_offset: frontier.journal_byte_length,
        record_id: record.journal_record_id().clone(),
        lifecycle_sequence: record.lifecycle_sequence(),
        frame_sha256: hex_digest(digest),
        durable_frontier: frontier.clone(),
    }
}

fn checkpoint_digest(frontier: &Stage6JournalFrontierV1) -> String {
    #[derive(Serialize)]
    struct DigestInput<'a> {
        storage_schema_version: u16,
        frontier: &'a Stage6JournalFrontierV1,
    }
    let input = DigestInput {
        storage_schema_version: STAGE6_JOURNAL_STORAGE_SCHEMA_VERSION,
        frontier,
    };
    let mut hasher = Sha256::new();
    hasher.update(CHECKPOINT_HASH_DOMAIN);
    hasher.update(serde_json::to_vec(&input).expect("fixed checkpoint digest input serializes"));
    hex_digest(&hasher.finalize().into())
}

fn validate_checkpoint_against_scan(
    checkpoint: &Stage6JournalCheckpointV1,
    scan: &ScannedJournal,
) -> Result<(), Stage6JournalStorageError> {
    checkpoint.validate()?;
    let count = checkpoint.frontier.frame_count;
    if count > scan.frontier.frame_count {
        return Err(Stage6JournalStorageError::CheckpointInvalid);
    }
    let expected = if count == 0 {
        Stage6JournalFrontierV1::empty()
    } else {
        scan.frontiers[(count - 1) as usize].clone()
    };
    if checkpoint.frontier != expected {
        return Err(Stage6JournalStorageError::CheckpointInvalid);
    }
    Ok(())
}

fn hex_digest(bytes: &[u8; FRAME_HASH_BYTES]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn is_lower_hex_digest(value: &str) -> bool {
    value.len() == 64
        && value != "0".repeat(64)
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TestIoFailpoint {
    AfterFrameHeaderWrite,
    AfterPartialRecordWrite,
    AfterFrameHashWrite,
    BeforeSync,
    SyncFailure,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Stage6DurableCommandSnapshotV1, Stage6DurableRequestIdentityV1, Stage6JournalCheckpointV1,
        Stage6ReplayEngineV1, Stage6Sha256Digest,
    };
    use broker_core::{
        BrokerAccountId, BrokerOrderId, BrokerTradeId, CancelOrder, ClientOrderId, Exchange,
        HybridRuntimeAttribution, InstrumentId, Market, OrderSide, OrderType, PlaceOrder,
        StrategyRequestId, TimeInForce,
    };
    use chrono::{TimeZone, Utc};
    use rust_decimal::Decimal;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};
    use uuid::Uuid;

    static TEMP_COUNTER: AtomicU64 = AtomicU64::new(1);

    fn place_record() -> Stage6JournalRecordV1 {
        let command = place_command(1);
        let identity = Stage6DurableRequestIdentityV1::from_place(
            &command,
            HybridRuntimeAttribution::parse_source_comment(
                "HYB|sid=hybrid_imoexf|c=cycle0001|o=BO|r=ENTRY",
            )
            .unwrap(),
        )
        .unwrap();
        let snapshot = Stage6DurableCommandSnapshotV1::from_place(&identity, &command).unwrap();
        Stage6JournalRecordV1::request_accepted(
            identity,
            snapshot,
            Stage6LifecycleSequence::new(1).unwrap(),
            None,
            None,
            Stage6Sha256Digest::parse("1".repeat(64)).unwrap(),
        )
        .unwrap()
    }

    fn cancel_record() -> Stage6JournalRecordV1 {
        let request_id = StrategyRequestId::from(Uuid::from_u128((2_u128 << 96) | 2));
        let command = CancelOrder {
            request_id,
            created_ts: Utc.with_ymd_and_hms(2026, 8, 9, 9, 1, 0).unwrap(),
            ttl_ms: Some(5000),
            account_id: BrokerAccountId::new("ACC_TEST_0001"),
            order_id: BrokerOrderId::new("ORDER/NON_NUMERIC"),
            client_order_id: Some(ClientOrderId::from_strategy_request(
                StrategyRequestId::from(Uuid::from_u128((1_u128 << 96) | 1)),
            )),
        };
        let attribution = HybridRuntimeAttribution::parse_source_comment(
            "HYB|sid=hybrid_imoexf|c=cycle0001|o=BO|r=CANCEL",
        )
        .unwrap();
        let identity =
            Stage6DurableRequestIdentityV1::from_cancel(&command, instrument(), attribution)
                .unwrap();
        let snapshot = Stage6DurableCommandSnapshotV1::from_cancel(&identity, &command).unwrap();
        Stage6JournalRecordV1::request_accepted(
            identity,
            snapshot,
            Stage6LifecycleSequence::new(1).unwrap(),
            None,
            None,
            Stage6Sha256Digest::parse("1".repeat(64)).unwrap(),
        )
        .unwrap()
    }

    fn instrument() -> InstrumentId {
        InstrumentId {
            symbol: "IMOEXF".into(),
            venue_symbol: Some("IMOEXF@RTSX".into()),
            exchange: Exchange::Moex,
            market: Market::Futures,
        }
    }

    fn place_command(request_number: u128) -> PlaceOrder {
        let request_id =
            StrategyRequestId::from(Uuid::from_u128((request_number << 96) | request_number));
        PlaceOrder {
            request_id,
            created_ts: Utc.with_ymd_and_hms(2026, 8, 9, 9, 0, 0).unwrap(),
            ttl_ms: Some(5000),
            account_id: BrokerAccountId::new("ACC_TEST_0001"),
            client_order_id: ClientOrderId::from_strategy_request(request_id),
            instrument: instrument(),
            side: OrderSide::Buy,
            order_type: OrderType::Limit,
            qty: Decimal::ONE,
            limit_price: Some(Decimal::new(2210, 1)),
            time_in_force: TimeInForce::Day,
            comment: Some("HYB|sid=hybrid_imoexf|c=cycle0001|o=BO|r=ENTRY".to_string()),
        }
    }

    fn place_identity(request_number: u128) -> Stage6DurableRequestIdentityV1 {
        let mut command = place_command(request_number);
        let attribution = HybridRuntimeAttribution::parse_source_comment(
            "HYB|sid=hybrid_imoexf|c=cycle0001|o=BO|r=ENTRY",
        )
        .unwrap();
        command.comment = Some(attribution.internal_comment().to_string());
        Stage6DurableRequestIdentityV1::from_place(&command, attribution).unwrap()
    }

    fn broker_order_record() -> Stage6JournalRecordV1 {
        Stage6JournalRecordV1::broker_order_observed(
            place_identity(11),
            BrokerOrderId::new("ORDER-OPAQUE-11"),
            Stage6LifecycleSequence::new(2).unwrap(),
            None,
            Stage6Sha256Digest::parse("2".repeat(64)).unwrap(),
        )
        .unwrap()
    }

    fn broker_trade_record() -> Stage6JournalRecordV1 {
        Stage6JournalRecordV1::broker_trade_observed(
            place_identity(12),
            BrokerTradeId::new("TRADE-OPAQUE-12"),
            BrokerOrderId::new("ORDER-OPAQUE-12"),
            Stage6LifecycleSequence::new(3).unwrap(),
            None,
            Stage6Sha256Digest::parse("3".repeat(64)).unwrap(),
        )
        .unwrap()
    }

    fn temp_path(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "stage6b-{label}-{}-{}.journal",
            std::process::id(),
            TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
        ))
    }

    fn assert_failed_existing_open_preserves_bytes(
        label: &str,
        bytes: &[u8],
    ) -> Stage6JournalStorageError {
        let path = temp_path(label);
        fs::write(&path, bytes).unwrap();
        let before = fs::read(&path).unwrap();
        let error = Stage6FileJournalBackend::open_for_test(&path).unwrap_err();
        assert_eq!(fs::read(&path).unwrap(), before);
        fs::remove_file(path).unwrap();
        error
    }

    fn one_frame_bytes() -> Vec<u8> {
        let mut backend = Stage6MemoryJournalBackend::new();
        backend.append(&place_record()).unwrap();
        backend.framed_bytes().unwrap()
    }

    fn three_frame_bytes() -> Vec<u8> {
        let mut backend = Stage6MemoryJournalBackend::new();
        backend.append(&place_record()).unwrap();
        backend.append(&cancel_record()).unwrap();
        backend.append(&broker_order_record()).unwrap();
        backend.framed_bytes().unwrap()
    }

    fn frame_range(bytes: &[u8], frame_index: usize) -> std::ops::Range<usize> {
        let mut start = JOURNAL_HEADER_BYTES;
        for index in 0..=frame_index {
            let length = u32::from_be_bytes(bytes[start + 6..start + 10].try_into().unwrap());
            let end = start + FRAME_PREFIX_BYTES + length as usize + FRAME_HASH_BYTES;
            if index == frame_index {
                return start..end;
            }
            start = end;
        }
        unreachable!()
    }

    fn mutate_record_body_without_changing_length_or_tail_hash(path: &Path, frame_index: usize) {
        let before = fs::read(path).unwrap();
        let range = frame_range(&before, frame_index);
        let body_offset = range.start + FRAME_PREFIX_BYTES;
        let tail_hash = before[before.len() - FRAME_HASH_BYTES..].to_vec();
        let mut external = OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
            .unwrap();
        external.seek(SeekFrom::Start(body_offset as u64)).unwrap();
        external.write_all(&[before[body_offset] ^ 0x01]).unwrap();
        external.sync_data().unwrap();
        drop(external);
        let after = fs::read(path).unwrap();
        assert_eq!(after.len(), before.len());
        assert_eq!(&after[after.len() - FRAME_HASH_BYTES..], tail_hash);
        assert_ne!(after, before);
    }

    fn checkpoint_and_replay_fingerprints(
        backend: &impl Stage6JournalBackend,
    ) -> (Vec<u8>, Stage6Sha256Digest) {
        let checkpoint =
            Stage6JournalCheckpointV1::from_frontier(backend.frontier().clone()).unwrap();
        let replay = Stage6ReplayEngineV1::replay(backend.records()).unwrap();
        (
            checkpoint.encode_canonical(),
            replay.semantic_fingerprint_sha256().clone(),
        )
    }

    fn framed_single_record(record_bytes: &[u8]) -> Vec<u8> {
        let frame = encode_frame(record_bytes, genesis_digest()).unwrap();
        let mut bytes = journal_header().to_vec();
        bytes.extend_from_slice(&frame.bytes);
        bytes
    }

    fn rehashed_json_record(
        rehash_payload: bool,
        mutator: impl FnOnce(&mut serde_json::Value),
    ) -> Vec<u8> {
        let mut value = serde_json::to_value(place_record()).unwrap();
        mutator(&mut value);
        if rehash_payload && value["payload"].is_object() {
            let payload_bytes = serde_json::to_vec(&value["payload"]).unwrap();
            value["canonical_payload_sha256"] = serde_json::json!(Sha256::digest(payload_bytes)
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>());
        }
        framed_single_record(&serde_json::to_vec(&value).unwrap())
    }

    #[test]
    fn stage6b_memory_journal_opens_empty() {
        let backend = Stage6MemoryJournalBackend::new();
        assert!(backend.records().is_empty());
        assert_eq!(backend.frontier(), &Stage6JournalFrontierV1::empty());
    }

    #[test]
    fn stage6b_filesystem_journal_opens_empty() {
        let path = temp_path("empty");
        let backend = Stage6FileJournalBackend::open_for_test(&path).unwrap();
        assert!(backend.records().is_empty());
        assert_eq!(backend.frontier(), &Stage6JournalFrontierV1::empty());
        drop(backend);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn stage7b_create_new_and_open_existing_are_explicit_and_disjoint() {
        let path = temp_path("stage7b-explicit-open");
        assert!(!path.exists());

        let created = Stage6FileJournalBackend::create_new(&path).unwrap();
        assert!(created.records().is_empty());
        drop(created);

        assert_eq!(
            Stage6FileJournalBackend::create_new(&path).unwrap_err(),
            Stage6JournalStorageError::Io {
                kind: ErrorKind::AlreadyExists,
            }
        );
        let reopened = Stage6FileJournalBackend::open_existing(&path).unwrap();
        assert!(reopened.records().is_empty());
        drop(reopened);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn stage7b_open_existing_never_creates_missing_journal() {
        let path = temp_path("stage7b-open-missing");
        assert_eq!(
            Stage6FileJournalBackend::open_existing(&path).unwrap_err(),
            Stage6JournalStorageError::Io {
                kind: ErrorKind::NotFound,
            }
        );
        assert!(!path.exists());
    }

    #[test]
    fn stage7b_b_journal_hard_link_alias_fails_closed() {
        let path = temp_path("stage7b-hard-link-journal");
        let alias = temp_path("stage7b-hard-link-journal-alias");
        drop(Stage6FileJournalBackend::create_new(&path).unwrap());
        fs::hard_link(&path, &alias).unwrap();
        assert_eq!(
            Stage6FileJournalBackend::open_existing(&path).unwrap_err(),
            Stage6JournalStorageError::ExternalMutationDetected
        );
        fs::remove_file(alias).unwrap();
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn stage7b_owned_backend_preserves_memory_file_and_reopen_parity() {
        let path = temp_path("stage7b-owned-parity");
        let mut memory = Stage6OwnedJournalBackend::memory();
        let mut file = Stage6OwnedJournalBackend::from_file(
            Stage6FileJournalBackend::create_new(&path).unwrap(),
        );
        assert!(!memory.is_file_backed());
        assert!(file.is_file_backed());

        for record in [place_record(), cancel_record(), broker_order_record()] {
            assert_eq!(
                memory.append(&record).unwrap(),
                file.append(&record).unwrap()
            );
        }
        assert_eq!(memory.records(), file.records());
        assert_eq!(memory.frontier(), file.frontier());
        assert_eq!(memory.framed_bytes().unwrap(), file.framed_bytes().unwrap());
        drop(file);

        let reopened = Stage6OwnedJournalBackend::from_file(
            Stage6FileJournalBackend::open_existing(&path).unwrap(),
        );
        assert_eq!(reopened.records(), memory.records());
        assert_eq!(reopened.frontier(), memory.frontier());
        assert_eq!(
            reopened.framed_bytes().unwrap(),
            memory.framed_bytes().unwrap()
        );
        drop(reopened);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn stage7b_memory_file_checkpoint_and_replay_fingerprints_are_identical() {
        let path = temp_path("stage7b-checkpoint-replay-parity");
        let mut memory = Stage6MemoryJournalBackend::new();
        let mut file = Stage6FileJournalBackend::create_new(&path).unwrap();
        for record in [place_record(), cancel_record()] {
            memory.append(&record).unwrap();
            file.append(&record).unwrap();
        }

        assert_eq!(
            checkpoint_and_replay_fingerprints(&memory),
            checkpoint_and_replay_fingerprints(&file)
        );
        drop(file);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn stage7b_file_reopen_checkpoint_and_replay_fingerprints_are_identical() {
        let path = temp_path("stage7b-reopen-checkpoint-replay-parity");
        let mut file = Stage6FileJournalBackend::create_new(&path).unwrap();
        for record in [place_record(), cancel_record()] {
            file.append(&record).unwrap();
        }
        let before = checkpoint_and_replay_fingerprints(&file);
        drop(file);

        let reopened = Stage6FileJournalBackend::open_existing(&path).unwrap();
        assert_eq!(checkpoint_and_replay_fingerprints(&reopened), before);
        drop(reopened);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn stage7b_same_length_earlier_record_mutation_is_detected_before_append() {
        let path = temp_path("stage7b-earlier-external-mutation");
        let mut backend = Stage6FileJournalBackend::create_new(&path).unwrap();
        backend.append(&place_record()).unwrap();
        backend.append(&cancel_record()).unwrap();
        mutate_record_body_without_changing_length_or_tail_hash(&path, 0);
        let externally_mutated = fs::read(&path).unwrap();

        assert_eq!(
            backend.append(&place_record()).unwrap_err(),
            Stage6JournalStorageError::ExternalMutationDetected
        );
        assert_eq!(fs::read(&path).unwrap(), externally_mutated);
        drop(backend);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn stage7b_same_length_last_record_mutation_is_detected_before_append() {
        let path = temp_path("stage7b-last-external-mutation");
        let mut backend = Stage6FileJournalBackend::create_new(&path).unwrap();
        backend.append(&place_record()).unwrap();
        backend.append(&cancel_record()).unwrap();
        mutate_record_body_without_changing_length_or_tail_hash(&path, 1);
        let externally_mutated = fs::read(&path).unwrap();

        assert_eq!(
            backend.append(&place_record()).unwrap_err(),
            Stage6JournalStorageError::ExternalMutationDetected
        );
        assert_eq!(fs::read(&path).unwrap(), externally_mutated);
        drop(backend);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn stage6b_r1_absent_path_creates_exact_empty_journal() {
        let path = temp_path("r1-absent-create");
        assert!(!path.exists());
        let backend = Stage6FileJournalBackend::open_for_test(&path).unwrap();
        assert_eq!(backend.framed_bytes().unwrap(), journal_header());
        drop(backend);
        assert_eq!(fs::read(&path).unwrap(), journal_header());
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn stage6b_r1_existing_header_only_opens_without_mutation() {
        let path = temp_path("r1-existing-header");
        fs::write(&path, journal_header()).unwrap();
        let before = fs::read(&path).unwrap();
        let backend = Stage6FileJournalBackend::open_for_test(&path).unwrap();
        assert!(backend.records().is_empty());
        drop(backend);
        assert_eq!(fs::read(&path).unwrap(), before);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn stage6b_r1_existing_zero_length_fails_closed() {
        let error = assert_failed_existing_open_preserves_bytes("r1-zero-fails", &[]);
        assert_eq!(error, Stage6JournalStorageError::InvalidJournalHeader);
    }

    #[test]
    fn stage6b_r1_existing_zero_length_remains_unchanged() {
        let path = temp_path("r1-zero-unchanged");
        File::create(&path).unwrap();
        assert_eq!(fs::metadata(&path).unwrap().len(), 0);
        assert!(Stage6FileJournalBackend::open_for_test(&path).is_err());
        assert_eq!(fs::metadata(&path).unwrap().len(), 0);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn stage6b_r1_existing_one_byte_remains_unchanged() {
        assert_eq!(
            assert_failed_existing_open_preserves_bytes("r1-one-byte", b"S"),
            Stage6JournalStorageError::InvalidJournalHeader
        );
    }

    #[test]
    fn stage6b_r1_existing_nine_byte_header_remains_unchanged() {
        assert_eq!(
            assert_failed_existing_open_preserves_bytes(
                "r1-nine-byte",
                &journal_header()[..JOURNAL_HEADER_BYTES - 1],
            ),
            Stage6JournalStorageError::InvalidJournalHeader
        );
    }

    #[test]
    fn stage6b_r1_existing_bad_magic_remains_unchanged() {
        let mut bytes = journal_header();
        bytes[0] ^= 0xff;
        assert_eq!(
            assert_failed_existing_open_preserves_bytes("r1-bad-magic", &bytes),
            Stage6JournalStorageError::InvalidJournalHeader
        );
    }

    #[test]
    fn stage6b_r1_existing_corrupt_nonempty_frame_remains_unchanged() {
        let bytes = one_frame_bytes();
        assert_eq!(
            assert_failed_existing_open_preserves_bytes(
                "r1-corrupt-frame",
                &bytes[..bytes.len() - 1],
            ),
            Stage6JournalStorageError::TornFrame
        );
    }

    #[test]
    fn stage6b_r1_valid_nonempty_reopen_does_not_rewrite() {
        let path = temp_path("r1-valid-nonempty");
        let bytes = one_frame_bytes();
        fs::write(&path, &bytes).unwrap();
        let backend = Stage6FileJournalBackend::open_for_test(&path).unwrap();
        assert_eq!(backend.records(), &[place_record()]);
        drop(backend);
        assert_eq!(fs::read(&path).unwrap(), bytes);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn stage6b_r1_repeated_valid_empty_open_remains_exact() {
        let path = temp_path("r1-repeat-empty");
        fs::write(&path, journal_header()).unwrap();
        for _ in 0..2 {
            let backend = Stage6FileJournalBackend::open_for_test(&path).unwrap();
            assert_eq!(backend.frontier(), &Stage6JournalFrontierV1::empty());
            drop(backend);
            assert_eq!(fs::read(&path).unwrap(), journal_header());
        }
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn stage6b_appends_valid_place_record() {
        let mut backend = Stage6MemoryJournalBackend::new();
        let receipt = backend.append(&place_record()).unwrap();
        assert_eq!(receipt.frame_index(), 0);
        assert_eq!(backend.records(), &[place_record()]);
    }

    #[test]
    fn stage6b_appends_valid_cancel_record() {
        let mut backend = Stage6MemoryJournalBackend::new();
        backend.append(&cancel_record()).unwrap();
        assert_eq!(backend.records(), &[cancel_record()]);
    }

    #[test]
    fn stage6b_appends_broker_order_observed_record() {
        let mut backend = Stage6MemoryJournalBackend::new();
        backend.append(&broker_order_record()).unwrap();
        assert_eq!(backend.records(), &[broker_order_record()]);
    }

    #[test]
    fn stage6b_appends_broker_trade_observed_record() {
        let mut backend = Stage6MemoryJournalBackend::new();
        backend.append(&broker_trade_record()).unwrap();
        assert_eq!(backend.records(), &[broker_trade_record()]);
    }

    #[test]
    fn stage6b_file_receipt_is_returned_after_sync_path() {
        let path = temp_path("receipt");
        let mut backend = Stage6FileJournalBackend::open_for_test(&path).unwrap();
        let receipt = backend.append(&place_record()).unwrap();
        assert_eq!(receipt.durable_frontier(), backend.frontier());
        assert_eq!(
            receipt.frame_end_offset(),
            fs::metadata(&path).unwrap().len()
        );
        drop(backend);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn stage6b_reopen_reads_exact_canonical_record() {
        let path = temp_path("reopen");
        let mut backend = Stage6FileJournalBackend::open_for_test(&path).unwrap();
        backend.append(&place_record()).unwrap();
        drop(backend);
        let reopened = Stage6FileJournalBackend::open_for_test(&path).unwrap();
        assert_eq!(
            reopened.records()[0].encode_canonical(),
            place_record().encode_canonical()
        );
        drop(reopened);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn stage6b_multiple_records_preserve_physical_order() {
        let mut backend = Stage6MemoryJournalBackend::new();
        let expected = vec![place_record(), cancel_record(), broker_order_record()];
        for record in &expected {
            backend.append(record).unwrap();
        }
        assert_eq!(backend.records(), expected);
    }

    #[test]
    fn stage6b_memory_and_file_framed_bytes_are_identical() {
        let path = temp_path("parity-bytes");
        let mut memory = Stage6MemoryJournalBackend::new();
        let mut file = Stage6FileJournalBackend::open_for_test(&path).unwrap();
        for record in [place_record(), cancel_record(), broker_order_record()] {
            memory.append(&record).unwrap();
            file.append(&record).unwrap();
        }
        assert_eq!(memory.framed_bytes().unwrap(), file.framed_bytes().unwrap());
        drop(file);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn stage6b_memory_and_file_frontiers_are_identical() {
        let path = temp_path("parity-frontier");
        let mut memory = Stage6MemoryJournalBackend::new();
        let mut file = Stage6FileJournalBackend::open_for_test(&path).unwrap();
        for record in [place_record(), cancel_record()] {
            assert_eq!(
                memory.append(&record).unwrap(),
                file.append(&record).unwrap()
            );
        }
        assert_eq!(memory.frontier(), file.frontier());
        drop(file);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn stage6b_reopen_reproduces_exact_frontier() {
        let path = temp_path("frontier");
        let mut backend = Stage6FileJournalBackend::open_for_test(&path).unwrap();
        for record in [place_record(), cancel_record(), broker_trade_record()] {
            backend.append(&record).unwrap();
        }
        let expected = backend.frontier().clone();
        drop(backend);
        let reopened = Stage6FileJournalBackend::open_for_test(&path).unwrap();
        assert_eq!(reopened.frontier(), &expected);
        drop(reopened);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn stage6b_empty_frontier_is_deterministic() {
        assert_eq!(
            serde_json::to_vec(&Stage6JournalFrontierV1::empty()).unwrap(),
            serde_json::to_vec(&Stage6JournalFrontierV1::empty()).unwrap()
        );
    }

    #[test]
    fn stage6b_nonempty_frontier_is_deterministic() {
        let first = Stage6MemoryJournalBackend::from_framed_bytes(one_frame_bytes()).unwrap();
        let second = Stage6MemoryJournalBackend::from_framed_bytes(one_frame_bytes()).unwrap();
        assert_eq!(first.frontier(), second.frontier());
    }

    #[test]
    fn stage6b_stage6a_typed_place_bytes_match_accepted_golden() {
        assert_eq!(
            place_record().encode_canonical(),
            include_bytes!("../../../fixtures/stage6a/place-request-accepted-v1.json")
                .strip_suffix(b"\n")
                .unwrap()
        );
    }

    #[test]
    fn stage6b_one_frame_bytes_match_exact_golden_hex() {
        let actual: String = one_frame_bytes()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        assert_eq!(
            actual,
            include_str!("../../../fixtures/stage6b/place-one-frame-v1.hex").trim_end()
        );
    }

    #[test]
    fn stage6b_empty_checkpoint_matches_exact_golden() {
        let checkpoint =
            Stage6JournalCheckpointV1::from_frontier(Stage6JournalFrontierV1::empty()).unwrap();
        let expected = include_bytes!("../../../fixtures/stage6b/empty-checkpoint-v1.json")
            .strip_suffix(b"\n")
            .unwrap();
        assert_eq!(checkpoint.encode_canonical(), expected);
        assert_eq!(
            Stage6JournalCheckpointV1::decode_canonical(expected).unwrap(),
            checkpoint
        );
    }

    #[test]
    fn stage6b_nonempty_checkpoint_matches_exact_golden() {
        let backend = Stage6MemoryJournalBackend::from_framed_bytes(one_frame_bytes()).unwrap();
        let checkpoint =
            Stage6JournalCheckpointV1::from_frontier(backend.frontier().clone()).unwrap();
        let expected = include_bytes!("../../../fixtures/stage6b/one-frame-checkpoint-v1.json")
            .strip_suffix(b"\n")
            .unwrap();
        assert_eq!(checkpoint.encode_canonical(), expected);
        assert_eq!(
            Stage6JournalCheckpointV1::decode_canonical(expected).unwrap(),
            checkpoint
        );
    }

    #[test]
    fn stage6b_checkpoint_roundtrip_is_strict_and_deterministic() {
        let backend = Stage6MemoryJournalBackend::from_framed_bytes(one_frame_bytes()).unwrap();
        let checkpoint =
            Stage6JournalCheckpointV1::from_frontier(backend.frontier().clone()).unwrap();
        let bytes = checkpoint.encode_canonical();
        assert_eq!(
            Stage6JournalCheckpointV1::decode_canonical(&bytes).unwrap(),
            checkpoint
        );
        assert_eq!(bytes, checkpoint.encode_canonical());
    }

    #[test]
    fn stage6b_stale_checkpoint_validates_against_longer_journal() {
        let mut backend = Stage6MemoryJournalBackend::new();
        backend.append(&place_record()).unwrap();
        let stale = Stage6JournalCheckpointV1::from_frontier(backend.frontier().clone()).unwrap();
        backend.append(&cancel_record()).unwrap();
        backend.validate_checkpoint(&stale).unwrap();
        assert_eq!(backend.records().len(), 2);
    }

    #[test]
    fn stage6b_bad_journal_magic_fails_closed() {
        let mut bytes = journal_header().to_vec();
        bytes[0] ^= 1;
        assert_eq!(
            scan_bytes(&bytes).unwrap_err(),
            Stage6JournalStorageError::InvalidJournalHeader
        );
    }

    #[test]
    fn stage6b_bad_storage_version_fails_closed() {
        let mut bytes = journal_header().to_vec();
        bytes[8..10].copy_from_slice(&2_u16.to_be_bytes());
        assert_eq!(
            scan_bytes(&bytes).unwrap_err(),
            Stage6JournalStorageError::UnsupportedStorageSchema { found: 2 }
        );
    }

    #[test]
    fn stage6b_truncated_journal_header_fails_closed() {
        assert_eq!(
            scan_bytes(&journal_header()[..9]).unwrap_err(),
            Stage6JournalStorageError::InvalidJournalHeader
        );
    }

    #[test]
    fn stage6b_truncated_frame_header_fails_closed() {
        let bytes = one_frame_bytes();
        assert_eq!(
            scan_bytes(&bytes[..JOURNAL_HEADER_BYTES + 3]).unwrap_err(),
            Stage6JournalStorageError::TornFrame
        );
    }

    #[test]
    fn stage6b_truncated_record_payload_fails_closed() {
        let bytes = one_frame_bytes();
        assert_eq!(
            scan_bytes(&bytes[..bytes.len() - 40]).unwrap_err(),
            Stage6JournalStorageError::TornFrame
        );
    }

    #[test]
    fn stage6b_truncated_frame_hash_fails_closed() {
        let bytes = one_frame_bytes();
        assert_eq!(
            scan_bytes(&bytes[..bytes.len() - 1]).unwrap_err(),
            Stage6JournalStorageError::TornFrame
        );
    }

    #[test]
    fn stage6b_trailing_garbage_fails_closed() {
        let mut bytes = one_frame_bytes();
        bytes.extend_from_slice(b"garbage");
        assert_eq!(
            scan_bytes(&bytes).unwrap_err(),
            Stage6JournalStorageError::TrailingGarbage
        );
    }

    #[test]
    fn stage6b_zero_frame_length_fails_closed() {
        let mut bytes = one_frame_bytes();
        bytes[16..20].copy_from_slice(&0_u32.to_be_bytes());
        assert_eq!(
            scan_bytes(&bytes).unwrap_err(),
            Stage6JournalStorageError::InvalidFrameLength { declared: 0 }
        );
    }

    #[test]
    fn stage6b_oversized_frame_length_fails_before_allocation() {
        let mut bytes = one_frame_bytes();
        let oversized = STAGE6_JOURNAL_MAX_RECORD_BYTES as u32 + 1;
        bytes[16..20].copy_from_slice(&oversized.to_be_bytes());
        assert_eq!(
            scan_bytes(&bytes).unwrap_err(),
            Stage6JournalStorageError::InvalidFrameLength {
                declared: u64::from(oversized)
            }
        );
    }

    #[test]
    fn stage6b_u32_max_frame_length_fails_before_allocation() {
        let mut bytes = one_frame_bytes();
        bytes[16..20].copy_from_slice(&u32::MAX.to_be_bytes());
        assert_eq!(
            scan_bytes(&bytes).unwrap_err(),
            Stage6JournalStorageError::InvalidFrameLength {
                declared: u64::from(u32::MAX)
            }
        );
    }

    #[test]
    fn stage6b_record_bit_flip_fails_outer_hash() {
        let mut bytes = one_frame_bytes();
        bytes[FRAME_PREFIX_BYTES + JOURNAL_HEADER_BYTES + 5] ^= 1;
        assert_eq!(
            scan_bytes(&bytes).unwrap_err(),
            Stage6JournalStorageError::FrameHashMismatch
        );
    }

    #[test]
    fn stage6b_previous_hash_bit_flip_fails_chain() {
        let mut bytes = one_frame_bytes();
        bytes[JOURNAL_HEADER_BYTES + 10] ^= 1;
        assert_eq!(
            scan_bytes(&bytes).unwrap_err(),
            Stage6JournalStorageError::FrameChainMismatch
        );
    }

    #[test]
    fn stage6b_frame_hash_bit_flip_fails_closed() {
        let mut bytes = one_frame_bytes();
        let last = bytes.len() - 1;
        bytes[last] ^= 1;
        assert_eq!(
            scan_bytes(&bytes).unwrap_err(),
            Stage6JournalStorageError::FrameHashMismatch
        );
    }

    #[test]
    fn stage6b_delete_middle_frame_fails_chain() {
        let bytes = three_frame_bytes();
        let middle = frame_range(&bytes, 1);
        let mut changed = bytes[..middle.start].to_vec();
        changed.extend_from_slice(&bytes[middle.end..]);
        assert_eq!(
            scan_bytes(&changed).unwrap_err(),
            Stage6JournalStorageError::FrameChainMismatch
        );
    }

    #[test]
    fn stage6b_swap_two_frames_fails_chain() {
        let bytes = three_frame_bytes();
        let first = frame_range(&bytes, 0);
        let second = frame_range(&bytes, 1);
        let mut changed = bytes[..JOURNAL_HEADER_BYTES].to_vec();
        changed.extend_from_slice(&bytes[second.clone()]);
        changed.extend_from_slice(&bytes[first.clone()]);
        changed.extend_from_slice(&bytes[second.end..]);
        assert_eq!(
            scan_bytes(&changed).unwrap_err(),
            Stage6JournalStorageError::FrameChainMismatch
        );
    }

    #[test]
    fn stage6b_duplicate_frame_with_stale_previous_hash_fails_chain() {
        let bytes = one_frame_bytes();
        let frame = bytes[JOURNAL_HEADER_BYTES..].to_vec();
        let mut changed = bytes;
        changed.extend_from_slice(&frame);
        assert_eq!(
            scan_bytes(&changed).unwrap_err(),
            Stage6JournalStorageError::FrameChainMismatch
        );
    }

    #[test]
    fn stage6b_outer_rehash_does_not_admit_noncanonical_json() {
        let mut record = place_record().encode_canonical();
        record.push(b'\n');
        assert_eq!(
            scan_bytes(&framed_single_record(&record)).unwrap_err(),
            Stage6JournalStorageError::NonCanonicalRecord
        );
    }

    #[test]
    fn stage6b_outer_rehash_does_not_admit_payload_digest_tamper() {
        let bytes = rehashed_json_record(false, |value| {
            value["canonical_payload_sha256"] = serde_json::json!("4".repeat(64));
        });
        assert!(matches!(
            scan_bytes(&bytes).unwrap_err(),
            Stage6JournalStorageError::RecordDecodeFailed { .. }
                | Stage6JournalStorageError::NonCanonicalRecord
        ));
    }

    #[test]
    fn stage6b_outer_rehash_does_not_admit_record_id_tamper() {
        let bytes = rehashed_json_record(true, |value| {
            value["journal_record_id"] = serde_json::json!("5".repeat(64));
        });
        assert!(matches!(
            scan_bytes(&bytes).unwrap_err(),
            Stage6JournalStorageError::RecordDecodeFailed { .. }
                | Stage6JournalStorageError::NonCanonicalRecord
        ));
    }

    #[test]
    fn stage6b_outer_rehash_does_not_admit_unsupported_place_shape() {
        let bytes = rehashed_json_record(true, |value| {
            value["payload"]["command"]["order_type"] = serde_json::json!("Stop");
        });
        assert!(matches!(
            scan_bytes(&bytes).unwrap_err(),
            Stage6JournalStorageError::RecordDecodeFailed { .. }
                | Stage6JournalStorageError::NonCanonicalRecord
        ));
    }

    #[test]
    fn stage6b_outer_rehash_does_not_admit_unknown_json_field() {
        let bytes = rehashed_json_record(true, |value| {
            value["unknown_stage6b"] = serde_json::json!(true);
        });
        assert!(matches!(
            scan_bytes(&bytes).unwrap_err(),
            Stage6JournalStorageError::RecordDecodeFailed { .. }
                | Stage6JournalStorageError::NonCanonicalRecord
        ));
    }

    #[test]
    fn stage6b_sync_failure_returns_durability_uncertain_without_receipt() {
        let path = temp_path("sync-failure");
        let mut backend = Stage6FileJournalBackend::open_for_test(&path).unwrap();
        backend.failpoint = Some(TestIoFailpoint::SyncFailure);
        assert_eq!(
            backend.append(&place_record()).unwrap_err(),
            Stage6JournalStorageError::DurabilityUncertain
        );
        assert!(backend.records().is_empty());
        assert_eq!(
            backend.append(&place_record()).unwrap_err(),
            Stage6JournalStorageError::DurabilityUncertain
        );
        drop(backend);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn stage6b_before_sync_failure_returns_no_receipt() {
        let path = temp_path("before-sync");
        let mut backend = Stage6FileJournalBackend::open_for_test(&path).unwrap();
        backend.failpoint = Some(TestIoFailpoint::BeforeSync);
        assert!(backend.append(&place_record()).is_err());
        assert!(backend.records().is_empty());
        drop(backend);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn stage6b_torn_write_failpoints_leave_reopen_fail_closed() {
        for failpoint in [
            TestIoFailpoint::AfterFrameHeaderWrite,
            TestIoFailpoint::AfterPartialRecordWrite,
        ] {
            let path = temp_path("torn-write");
            let mut backend = Stage6FileJournalBackend::open_for_test(&path).unwrap();
            backend.failpoint = Some(failpoint);
            assert!(backend.append(&place_record()).is_err());
            drop(backend);
            assert!(Stage6FileJournalBackend::open_for_test(&path).is_err());
            fs::remove_file(path).unwrap();
        }
    }

    #[test]
    fn stage6b_complete_frame_before_sync_is_decided_by_reopen_scan() {
        let path = temp_path("complete-unsynced");
        let mut backend = Stage6FileJournalBackend::open_for_test(&path).unwrap();
        backend.failpoint = Some(TestIoFailpoint::AfterFrameHashWrite);
        assert!(backend.append(&place_record()).is_err());
        drop(backend);
        let reopened = Stage6FileJournalBackend::open_for_test(&path).unwrap();
        assert_eq!(reopened.records(), &[place_record()]);
        drop(reopened);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn stage6b_checkpoint_digest_corruption_fails_closed() {
        let checkpoint =
            Stage6JournalCheckpointV1::from_frontier(Stage6JournalFrontierV1::empty()).unwrap();
        let mut value = serde_json::to_value(checkpoint).unwrap();
        value["checkpoint_sha256"] = serde_json::json!("6".repeat(64));
        assert!(
            Stage6JournalCheckpointV1::decode_canonical(&serde_json::to_vec(&value).unwrap())
                .is_err()
        );
    }

    #[test]
    fn stage6b_checkpoint_ahead_of_journal_fails_closed() {
        let backend = Stage6MemoryJournalBackend::from_framed_bytes(one_frame_bytes()).unwrap();
        let mut ahead = backend.frontier().clone();
        ahead.frame_count += 1;
        let checkpoint = Stage6JournalCheckpointV1::from_frontier(ahead).unwrap();
        assert_eq!(
            backend.validate_checkpoint(&checkpoint).unwrap_err(),
            Stage6JournalStorageError::CheckpointInvalid
        );
    }

    #[test]
    fn stage6b_checkpoint_frame_hash_mismatch_fails_closed() {
        let backend = Stage6MemoryJournalBackend::from_framed_bytes(one_frame_bytes()).unwrap();
        let mut changed = backend.frontier().clone();
        changed.last_frame_sha256 = "7".repeat(64);
        let checkpoint = Stage6JournalCheckpointV1::from_frontier(changed).unwrap();
        assert_eq!(
            backend.validate_checkpoint(&checkpoint).unwrap_err(),
            Stage6JournalStorageError::CheckpointInvalid
        );
    }

    #[test]
    fn stage6b_external_file_length_mutation_blocks_append() {
        let path = temp_path("external-length");
        let mut backend = Stage6FileJournalBackend::open_for_test(&path).unwrap();
        OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap()
            .write_all(b"x")
            .unwrap();
        assert_eq!(
            backend.append(&place_record()).unwrap_err(),
            Stage6JournalStorageError::ExternalMutationDetected
        );
        drop(backend);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn stage6b_corrupt_journal_is_never_auto_repaired() {
        let path = temp_path("no-repair");
        let bytes = one_frame_bytes();
        fs::write(&path, &bytes[..bytes.len() - 1]).unwrap();
        let before = fs::read(&path).unwrap();
        assert!(Stage6FileJournalBackend::open_for_test(&path).is_err());
        assert_eq!(fs::read(&path).unwrap(), before);
        fs::remove_file(path).unwrap();
    }
}
