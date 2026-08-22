//! Stage 8B-I no-send authority topology and deterministic rehearsal.
//!
//! The only public boundary validates immutable local evidence and returns a
//! redacted diagnostic. It cannot construct an arm, transport permit or broker
//! request, and this module contains no HTTP or Redis operation.

use crate::{
    Stage8a1CurrentlyAuthorizedCapability, Stage8a1DurableRequestAuthority,
    Stage8a2BuilderCompositionDiagnostic, Stage8a2BuilderCompositionError,
    Stage8a2InMemoryNoSendSink, Stage8a3ClassifiedObservation, Stage8a3EndpointContext,
    Stage8a3LocalHttpObservation,
};
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};
use std::ffi::CString;
use std::fs::{self, File, Metadata};
use std::io::{Read, Write};
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::MetadataExt;
use std::path::{Component, Path, PathBuf};
use zeroize::Zeroizing;

const MANIFEST_NAME: &str = "stage8b-run-manifest.json";
const MAX_PACKAGE_BYTES: u64 = 32 * 1024 * 1024;
const MAX_MANIFEST_BYTES: u64 = 256 * 1024;
#[allow(dead_code)]
const MIN_HMAC_KEY_BYTES: usize = 32;

/// Opaque user input. Fields are private so callers cannot reinterpret the
/// boundary as an authority-bearing data structure.
pub struct Stage8bOperatorInvocationRequest {
    invocation_id: String,
    accepted_run_package_path: PathBuf,
    local_manifest_root: PathBuf,
}

impl Stage8bOperatorInvocationRequest {
    pub fn new(
        invocation_id: impl Into<String>,
        accepted_run_package_path: impl Into<PathBuf>,
        local_manifest_root: impl Into<PathBuf>,
    ) -> Self {
        Self {
            invocation_id: invocation_id.into(),
            accepted_run_package_path: accepted_run_package_path.into(),
            local_manifest_root: local_manifest_root.into(),
        }
    }
}

/// Bounded output with no account, path, body, token, client or capability.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Stage8bOperatorDiagnostic {
    pub invocation_binding_sha256: String,
    pub accepted_run_package_sha256: String,
    pub local_manifest_sha256: String,
    pub evidence_files_pinned: u8,
    pub no_send: bool,
    pub authority_constructed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Stage8bOperatorFacadeError {
    #[error("Stage 8B invocation identifier is invalid")]
    InvalidInvocationId,
    #[error("Stage 8B evidence path is unsafe")]
    UnsafeEvidencePath,
    #[error("Stage 8B evidence file is missing or invalid")]
    InvalidEvidenceFile,
    #[error("Stage 8B evidence changed during validation")]
    EvidenceIdentityDrift,
    #[error("Stage 8B evidence exceeds the bounded input limit")]
    EvidenceTooLarge,
    #[error("Stage 8B local evidence could not be read")]
    EvidenceIo,
}

/// Sole public cross-crate Stage 8B entry. Validation ends at a no-send
/// diagnostic; the private authority composition root is intentionally not
/// invoked by Stage 8B-I.
pub fn invoke_stage8b_operator_once(
    request: Stage8bOperatorInvocationRequest,
) -> Result<Stage8bOperatorDiagnostic, Stage8bOperatorFacadeError> {
    validate_invocation_id(&request.invocation_id)?;
    let package = read_pinned_regular_file(&request.accepted_run_package_path, MAX_PACKAGE_BYTES)?;
    let manifest = read_pinned_manifest(&request.local_manifest_root)?;
    let package_sha256 = sha256_hex(&package);
    let manifest_sha256 = sha256_hex(&manifest);
    let invocation_binding_sha256 = digest_parts(
        b"stage8b-i-public-no-send-facade-v1",
        &[
            request.invocation_id.as_bytes(),
            package_sha256.as_bytes(),
            manifest_sha256.as_bytes(),
        ],
    );
    Ok(Stage8bOperatorDiagnostic {
        invocation_binding_sha256,
        accepted_run_package_sha256: package_sha256,
        local_manifest_sha256: manifest_sha256,
        evidence_files_pinned: 2,
        no_send: true,
        authority_constructed: false,
    })
}

fn validate_invocation_id(value: &str) -> Result<(), Stage8bOperatorFacadeError> {
    if value.len() < 16
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(Stage8bOperatorFacadeError::InvalidInvocationId);
    }
    Ok(())
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct FileIdentity {
    device: u64,
    inode: u64,
    length: u64,
    modified_seconds: i64,
    modified_nanoseconds: i64,
    links: u64,
}

impl FileIdentity {
    fn from_metadata(metadata: &Metadata) -> Self {
        Self {
            device: metadata.dev(),
            inode: metadata.ino(),
            length: metadata.len(),
            modified_seconds: metadata.mtime(),
            modified_nanoseconds: metadata.mtime_nsec(),
            links: metadata.nlink(),
        }
    }
}

fn reject_symlink_components(path: &Path) -> Result<(), Stage8bOperatorFacadeError> {
    if !path.is_absolute() {
        return Err(Stage8bOperatorFacadeError::UnsafeEvidencePath);
    }
    let mut current = PathBuf::new();
    for component in path.components() {
        match component {
            Component::RootDir => current.push(Path::new("/")),
            Component::Normal(value) => current.push(value),
            _ => return Err(Stage8bOperatorFacadeError::UnsafeEvidencePath),
        }
        let metadata = fs::symlink_metadata(&current)
            .map_err(|_| Stage8bOperatorFacadeError::InvalidEvidenceFile)?;
        if metadata.file_type().is_symlink() {
            return Err(Stage8bOperatorFacadeError::UnsafeEvidencePath);
        }
    }
    Ok(())
}

fn open_no_follow(path: &Path, directory: bool) -> Result<File, Stage8bOperatorFacadeError> {
    let bytes = path.as_os_str().as_bytes();
    let c_path = CString::new(bytes).map_err(|_| Stage8bOperatorFacadeError::UnsafeEvidencePath)?;
    let mut flags = libc::O_RDONLY | libc::O_CLOEXEC | libc::O_NOFOLLOW;
    if directory {
        flags |= libc::O_DIRECTORY;
    }
    // SAFETY: `c_path` is NUL-terminated and the returned descriptor is owned
    // exactly once by `File` on success.
    let descriptor = unsafe { libc::open(c_path.as_ptr(), flags) };
    if descriptor < 0 {
        return Err(Stage8bOperatorFacadeError::InvalidEvidenceFile);
    }
    // SAFETY: successful `open` transfers one owned descriptor.
    Ok(unsafe { File::from_raw_fd(descriptor) })
}

fn read_pinned_regular_file(
    path: &Path,
    max_bytes: u64,
) -> Result<Vec<u8>, Stage8bOperatorFacadeError> {
    read_pinned_regular_file_with_hook(path, max_bytes, || {})
}

fn read_pinned_regular_file_with_hook<F>(
    path: &Path,
    max_bytes: u64,
    after_open: F,
) -> Result<Vec<u8>, Stage8bOperatorFacadeError>
where
    F: FnOnce(),
{
    reject_symlink_components(path)?;
    let path_before =
        fs::symlink_metadata(path).map_err(|_| Stage8bOperatorFacadeError::InvalidEvidenceFile)?;
    if !path_before.file_type().is_file() || path_before.nlink() != 1 {
        return Err(Stage8bOperatorFacadeError::UnsafeEvidencePath);
    }
    if path_before.len() > max_bytes {
        return Err(Stage8bOperatorFacadeError::EvidenceTooLarge);
    }
    let mut file = open_no_follow(path, false)?;
    let descriptor_before = file
        .metadata()
        .map_err(|_| Stage8bOperatorFacadeError::EvidenceIo)?;
    if FileIdentity::from_metadata(&path_before) != FileIdentity::from_metadata(&descriptor_before)
    {
        return Err(Stage8bOperatorFacadeError::EvidenceIdentityDrift);
    }
    after_open();
    let mut bytes = Vec::with_capacity(descriptor_before.len() as usize);
    file.read_to_end(&mut bytes)
        .map_err(|_| Stage8bOperatorFacadeError::EvidenceIo)?;
    if bytes.len() as u64 != descriptor_before.len() || bytes.len() as u64 > max_bytes {
        return Err(Stage8bOperatorFacadeError::EvidenceIdentityDrift);
    }
    let descriptor_after = file
        .metadata()
        .map_err(|_| Stage8bOperatorFacadeError::EvidenceIo)?;
    let path_after = fs::symlink_metadata(path)
        .map_err(|_| Stage8bOperatorFacadeError::EvidenceIdentityDrift)?;
    let identity = FileIdentity::from_metadata(&descriptor_before);
    if identity != FileIdentity::from_metadata(&descriptor_after)
        || identity != FileIdentity::from_metadata(&path_after)
    {
        return Err(Stage8bOperatorFacadeError::EvidenceIdentityDrift);
    }
    Ok(bytes)
}

fn read_pinned_manifest(root: &Path) -> Result<Vec<u8>, Stage8bOperatorFacadeError> {
    reject_symlink_components(root)?;
    let root_metadata =
        fs::symlink_metadata(root).map_err(|_| Stage8bOperatorFacadeError::InvalidEvidenceFile)?;
    if !root_metadata.file_type().is_dir() {
        return Err(Stage8bOperatorFacadeError::UnsafeEvidencePath);
    }
    let directory = open_no_follow(root, true)?;
    let identity = FileIdentity::from_metadata(
        &directory
            .metadata()
            .map_err(|_| Stage8bOperatorFacadeError::EvidenceIo)?,
    );
    if identity != FileIdentity::from_metadata(&root_metadata) {
        return Err(Stage8bOperatorFacadeError::EvidenceIdentityDrift);
    }
    let c_name = CString::new(MANIFEST_NAME).expect("static manifest name");
    // SAFETY: directory descriptor and static child name remain valid for the
    // duration of `openat`; successful ownership transfers to `File` once.
    let descriptor = unsafe {
        libc::openat(
            directory.as_raw_fd(),
            c_name.as_ptr(),
            libc::O_RDONLY | libc::O_CLOEXEC | libc::O_NOFOLLOW,
        )
    };
    if descriptor < 0 {
        return Err(Stage8bOperatorFacadeError::InvalidEvidenceFile);
    }
    // SAFETY: successful `openat` returns one owned descriptor.
    let mut file = unsafe { File::from_raw_fd(descriptor) };
    let before = file
        .metadata()
        .map_err(|_| Stage8bOperatorFacadeError::EvidenceIo)?;
    if !before.file_type().is_file() || before.nlink() != 1 {
        return Err(Stage8bOperatorFacadeError::UnsafeEvidencePath);
    }
    if before.len() > MAX_MANIFEST_BYTES {
        return Err(Stage8bOperatorFacadeError::EvidenceTooLarge);
    }
    let mut bytes = Vec::with_capacity(before.len() as usize);
    file.read_to_end(&mut bytes)
        .map_err(|_| Stage8bOperatorFacadeError::EvidenceIo)?;
    let after = file
        .metadata()
        .map_err(|_| Stage8bOperatorFacadeError::EvidenceIo)?;
    if bytes.len() as u64 != before.len()
        || FileIdentity::from_metadata(&before) != FileIdentity::from_metadata(&after)
    {
        return Err(Stage8bOperatorFacadeError::EvidenceIdentityDrift);
    }
    let root_after = fs::symlink_metadata(root)
        .map_err(|_| Stage8bOperatorFacadeError::EvidenceIdentityDrift)?;
    if identity != FileIdentity::from_metadata(&root_after) {
        return Err(Stage8bOperatorFacadeError::EvidenceIdentityDrift);
    }
    Ok(bytes)
}

#[allow(dead_code)]
pub(crate) struct Stage8bExecutionQualifiedBuild {
    identity_sha256: String,
}
#[allow(dead_code)]
pub(crate) struct Stage8bKeyedAccountBinding {
    binding_sha256: String,
}
#[allow(dead_code)]
pub(crate) struct Stage8bFreshContractAuthority {
    contract_sha256: String,
}
#[allow(dead_code)]
pub(crate) struct Stage8bAcceptedRunSpec {
    run_sha256: String,
}
#[allow(dead_code)]
pub(crate) struct Stage8bK1ControlApproved {
    control_sha256: String,
}
#[allow(dead_code)]
pub(crate) struct Stage8bOperatorArm {
    arm_sha256: String,
}
#[allow(dead_code)]
pub(crate) struct Stage8bFreshPreflightApproved {
    binding_sha256: String,
    request_parts: Stage8bApprovedRequestParts,
}
#[allow(dead_code)]
pub(crate) struct Stage8bSealedAttemptCommitted {
    attempt_sha256: String,
}
#[allow(dead_code)]
pub(crate) struct Stage8bExactTransportPermit {
    permit_sha256: String,
}
#[allow(dead_code)]
pub(crate) struct Stage8bPossibleEffectOwner {
    lifecycle_sha256: String,
}
#[allow(dead_code)]
pub(crate) struct Stage8bDurableClosureOwner {
    closure: Stage8bClosureClassification,
}
#[allow(dead_code)]
pub(crate) struct Stage8bClosureReceipt {
    receipt_sha256: String,
}

#[allow(dead_code)]
struct Stage8bApprovedRequestParts {
    diagnostic: Stage8a2BuilderCompositionDiagnostic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
enum Stage8bClosureClassification {
    Stage8BClosedSafe,
    ResidualWorkingOrder,
    ResidualPosition,
    OutcomeUnknown,
    BrokerTruthConflict,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum Stage8bNoSendCompositionError {
    #[error("existing Stage 8A-2 builder bridge rejected the continuation")]
    Builder(#[from] Stage8a2BuilderCompositionError),
}

fn compose_stage8b_private_request_parts_from_stage8a2(
    capability: Stage8a1CurrentlyAuthorizedCapability,
) -> Result<Stage8bApprovedRequestParts, Stage8bNoSendCompositionError> {
    let mut sink = Stage8a2InMemoryNoSendSink::new();
    let diagnostic = capability.compose_stage8a2_no_send(&mut sink)?;
    Ok(Stage8bApprovedRequestParts { diagnostic })
}

/// Single private authority composition root. Stage 8B-I does not call this
/// from its public facade and cannot produce a transport permit.
#[allow(clippy::too_many_arguments, dead_code)]
pub(crate) fn compose_stage8b_effect_authority(
    capability: Stage8a1CurrentlyAuthorizedCapability,
    _durable: Stage8a1DurableRequestAuthority,
    build: Stage8bExecutionQualifiedBuild,
    account: Stage8bKeyedAccountBinding,
    contract: Stage8bFreshContractAuthority,
    run: Stage8bAcceptedRunSpec,
    control: Stage8bK1ControlApproved,
    arm: Stage8bOperatorArm,
) -> Result<Stage8bFreshPreflightApproved, Stage8bNoSendCompositionError> {
    let request_parts = compose_stage8b_private_request_parts_from_stage8a2(capability)?;
    let binding_sha256 = digest_parts(
        b"stage8b-i-private-root-v1",
        &[
            build.identity_sha256.as_bytes(),
            account.binding_sha256.as_bytes(),
            contract.contract_sha256.as_bytes(),
            run.run_sha256.as_bytes(),
            control.control_sha256.as_bytes(),
            arm.arm_sha256.as_bytes(),
            request_parts.diagnostic.authority_binding_sha256.as_bytes(),
        ],
    );
    Ok(Stage8bFreshPreflightApproved {
        binding_sha256,
        request_parts,
    })
}

#[allow(dead_code)]
fn keyed_account_binding(
    secret: Zeroizing<Vec<u8>>,
    account_utf8: &[u8],
) -> Result<Stage8bKeyedAccountBinding, Stage8bHmacError> {
    if secret.len() < MIN_HMAC_KEY_BYTES || account_utf8.is_empty() {
        return Err(Stage8bHmacError::InvalidInput);
    }
    let mut message = Zeroizing::new(Vec::with_capacity(40 + account_utf8.len()));
    message.extend_from_slice(b"moex-stage8b-account-binding-v1");
    message.push(0);
    let length: u32 = account_utf8
        .len()
        .try_into()
        .map_err(|_| Stage8bHmacError::InvalidInput)?;
    message.extend_from_slice(&length.to_be_bytes());
    message.extend_from_slice(account_utf8);
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_slice())
        .map_err(|_| Stage8bHmacError::InvalidInput)?;
    mac.update(message.as_slice());
    let binding_sha256 = hex_lower(&mac.finalize().into_bytes());
    Ok(Stage8bKeyedAccountBinding { binding_sha256 })
}

#[allow(dead_code)]
fn verify_keyed_account_binding(
    secret: Zeroizing<Vec<u8>>,
    account_utf8: &[u8],
    expected: &[u8],
) -> Result<(), Stage8bHmacError> {
    if secret.len() < MIN_HMAC_KEY_BYTES || account_utf8.is_empty() {
        return Err(Stage8bHmacError::InvalidInput);
    }
    let mut message = Zeroizing::new(Vec::with_capacity(40 + account_utf8.len()));
    message.extend_from_slice(b"moex-stage8b-account-binding-v1");
    message.push(0);
    let length: u32 = account_utf8
        .len()
        .try_into()
        .map_err(|_| Stage8bHmacError::InvalidInput)?;
    message.extend_from_slice(&length.to_be_bytes());
    message.extend_from_slice(account_utf8);
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_slice())
        .map_err(|_| Stage8bHmacError::InvalidInput)?;
    mac.update(message.as_slice());
    mac.verify_slice(expected)
        .map_err(|_| Stage8bHmacError::Mismatch)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
enum Stage8bHmacError {
    InvalidInput,
    Mismatch,
}

#[allow(dead_code)]
fn classify_stage8b_transport_observation_with_stage8a3(
    context: Stage8a3EndpointContext,
    observation: Stage8a3LocalHttpObservation,
) -> Stage8a3ClassifiedObservation {
    context.classify(observation)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
enum Stage8bCrashWindow {
    BeforeAttempt,
    AttemptCommittedNoTransport,
    PossibleSendNoResponse,
    ResponseNoDurableOutcome,
    DurableOutcomeNoPublication,
    RestartAtEveryBoundary,
}

#[allow(dead_code)]
fn rehearse_crash_window(window: Stage8bCrashWindow) -> Stage8bClosureClassification {
    match window {
        Stage8bCrashWindow::BeforeAttempt | Stage8bCrashWindow::AttemptCommittedNoTransport => {
            Stage8bClosureClassification::Stage8BClosedSafe
        }
        Stage8bCrashWindow::PossibleSendNoResponse
        | Stage8bCrashWindow::ResponseNoDurableOutcome => {
            Stage8bClosureClassification::OutcomeUnknown
        }
        Stage8bCrashWindow::DurableOutcomeNoPublication
        | Stage8bCrashWindow::RestartAtEveryBoundary => {
            Stage8bClosureClassification::Stage8BClosedSafe
        }
    }
}

const REHEARSAL_JOURNAL_HEADER: &[u8] = b"STAGE8B-I-NO-SEND-V1\n";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Stage8bRehearsalRecord {
    AttemptCommitted,
    PossibleEffectObserved,
    ResponseObserved,
    DurableOutcomeRecorded,
    PublicationRecorded,
}

impl Stage8bRehearsalRecord {
    fn code(self) -> u8 {
        match self {
            Self::AttemptCommitted => b'A',
            Self::PossibleEffectObserved => b'P',
            Self::ResponseObserved => b'R',
            Self::DurableOutcomeRecorded => b'D',
            Self::PublicationRecorded => b'U',
        }
    }
}

#[allow(dead_code)]
struct Stage8bNoSendRehearsalJournal {
    file: File,
}

#[allow(dead_code)]
impl Stage8bNoSendRehearsalJournal {
    fn create(root: &Path) -> Result<Self, Stage8bRehearsalError> {
        reject_symlink_components(root).map_err(|_| Stage8bRehearsalError::UnsafePath)?;
        let directory =
            open_no_follow(root, true).map_err(|_| Stage8bRehearsalError::UnsafePath)?;
        let name = CString::new("stage8b-i-rehearsal.journal").expect("static name");
        // SAFETY: descriptor/name are valid and O_EXCL prevents replacement.
        let descriptor = unsafe {
            libc::openat(
                directory.as_raw_fd(),
                name.as_ptr(),
                libc::O_RDWR | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC,
                0o600,
            )
        };
        if descriptor < 0 {
            return Err(Stage8bRehearsalError::Io);
        }
        // SAFETY: successful openat returns one owned descriptor.
        let mut file = unsafe { File::from_raw_fd(descriptor) };
        file.write_all(REHEARSAL_JOURNAL_HEADER)
            .map_err(|_| Stage8bRehearsalError::Io)?;
        file.sync_all().map_err(|_| Stage8bRehearsalError::Io)?;
        directory
            .sync_all()
            .map_err(|_| Stage8bRehearsalError::Io)?;
        Ok(Self { file })
    }

    fn append(&mut self, record: Stage8bRehearsalRecord) -> Result<(), Stage8bRehearsalError> {
        self.file
            .write_all(&[record.code(), b'\n'])
            .map_err(|_| Stage8bRehearsalError::Io)?;
        self.file.sync_all().map_err(|_| Stage8bRehearsalError::Io)
    }

    fn recover(root: &Path) -> Result<Stage8bClosureClassification, Stage8bRehearsalError> {
        let path = root.join("stage8b-i-rehearsal.journal");
        let bytes = read_pinned_regular_file(&path, 4 * 1024)
            .map_err(|_| Stage8bRehearsalError::UnsafePath)?;
        if !bytes.starts_with(REHEARSAL_JOURNAL_HEADER) {
            return Err(Stage8bRehearsalError::InvalidSequence);
        }
        let body = &bytes[REHEARSAL_JOURNAL_HEADER.len()..];
        let records = body
            .chunks_exact(2)
            .map(|chunk| {
                if chunk[1] != b'\n' {
                    return Err(Stage8bRehearsalError::InvalidSequence);
                }
                match chunk[0] {
                    b'A' => Ok(Stage8bRehearsalRecord::AttemptCommitted),
                    b'P' => Ok(Stage8bRehearsalRecord::PossibleEffectObserved),
                    b'R' => Ok(Stage8bRehearsalRecord::ResponseObserved),
                    b'D' => Ok(Stage8bRehearsalRecord::DurableOutcomeRecorded),
                    b'U' => Ok(Stage8bRehearsalRecord::PublicationRecorded),
                    _ => Err(Stage8bRehearsalError::InvalidSequence),
                }
            })
            .collect::<Result<Vec<_>, _>>()?;
        if body.len() != records.len() * 2 {
            return Err(Stage8bRehearsalError::InvalidSequence);
        }
        let codes = records
            .iter()
            .map(|record| record.code())
            .collect::<Vec<_>>();
        match codes.as_slice() {
            [] | [b'A'] => Ok(Stage8bClosureClassification::Stage8BClosedSafe),
            [b'A', b'P'] | [b'A', b'P', b'R'] => Ok(Stage8bClosureClassification::OutcomeUnknown),
            [b'A', b'P', b'R', b'D'] | [b'A', b'P', b'R', b'D', b'U'] => {
                Ok(Stage8bClosureClassification::Stage8BClosedSafe)
            }
            _ => Err(Stage8bRehearsalError::InvalidSequence),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stage8bRehearsalError {
    UnsafePath,
    InvalidSequence,
    Io,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
enum Stage8bKillBoundary {
    K1,
    K2,
    K3,
    K4,
    K5,
}

#[allow(dead_code)]
fn inject_kill_boundary(boundary: Stage8bKillBoundary) -> Stage8bClosureClassification {
    match boundary {
        Stage8bKillBoundary::K1 | Stage8bKillBoundary::K2 | Stage8bKillBoundary::K3 => {
            Stage8bClosureClassification::Stage8BClosedSafe
        }
        Stage8bKillBoundary::K4 | Stage8bKillBoundary::K5 => {
            Stage8bClosureClassification::OutcomeUnknown
        }
    }
}

#[allow(dead_code)]
fn issue_rehearsal_arm(
    registry: &Path,
    uniqueness_sha256: &str,
) -> Result<Stage8bOperatorArm, Stage8bArmIssueError> {
    if uniqueness_sha256.len() != 64
        || !uniqueness_sha256
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(Stage8bArmIssueError::InvalidIdentity);
    }
    reject_symlink_components(registry).map_err(|_| Stage8bArmIssueError::UnsafeRegistry)?;
    let directory =
        open_no_follow(registry, true).map_err(|_| Stage8bArmIssueError::UnsafeRegistry)?;
    let filename = format!("arm-{uniqueness_sha256}.record");
    let c_name = CString::new(filename).map_err(|_| Stage8bArmIssueError::InvalidIdentity)?;
    // SAFETY: `directory` and `c_name` remain valid; O_EXCL provides the
    // cross-process one-winner guarantee and ownership transfers once.
    let descriptor = unsafe {
        libc::openat(
            directory.as_raw_fd(),
            c_name.as_ptr(),
            libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            0o600,
        )
    };
    if descriptor < 0 {
        return if std::io::Error::last_os_error().kind() == std::io::ErrorKind::AlreadyExists {
            Err(Stage8bArmIssueError::AlreadyIssued)
        } else {
            Err(Stage8bArmIssueError::Io)
        };
    }
    // SAFETY: successful `openat` returns one owned descriptor.
    let mut file = unsafe { File::from_raw_fd(descriptor) };
    let arm_sha256 = digest_parts(b"stage8b-i-durable-arm-v1", &[uniqueness_sha256.as_bytes()]);
    file.write_all(arm_sha256.as_bytes())
        .map_err(|_| Stage8bArmIssueError::Io)?;
    file.sync_all().map_err(|_| Stage8bArmIssueError::Io)?;
    directory.sync_all().map_err(|_| Stage8bArmIssueError::Io)?;
    Ok(Stage8bOperatorArm { arm_sha256 })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
enum Stage8bArmIssueError {
    InvalidIdentity,
    UnsafeRegistry,
    AlreadyIssued,
    Io,
}

fn sha256_hex(bytes: &[u8]) -> String {
    hex_lower(&Sha256::digest(bytes))
}

fn digest_parts(domain: &[u8], parts: &[&[u8]]) -> String {
    let mut digest = Sha256::new();
    digest.update(domain);
    for part in parts {
        digest.update((part.len() as u64).to_be_bytes());
        digest.update(part);
    }
    hex_lower(&digest.finalize())
}

fn hex_lower(bytes: &[u8]) -> String {
    let mut value = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(value, "{byte:02x}");
    }
    value
}

#[cfg(test)]
mod tests {
    use super::*;
    use broker_core::{AccountId, ClientOrderId, InstrumentId, StrategyRequestId};
    use std::process::Command;
    use uuid::Uuid;

    fn temp_directory(label: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "stage8b-i-{label}-{}-{}",
            std::process::id(),
            Uuid::new_v4()
        ));
        fs::create_dir_all(&path).unwrap();
        path.canonicalize().unwrap()
    }

    #[test]
    fn public_facade_is_redacted_and_no_send() {
        let root = temp_directory("facade");
        let package = root.join("accepted.zip");
        fs::write(&package, b"immutable-package").unwrap();
        fs::write(root.join(MANIFEST_NAME), b"{\"schema_version\":1}").unwrap();
        let result = invoke_stage8b_operator_once(Stage8bOperatorInvocationRequest::new(
            "INVOCATION_TEST_0001",
            &package,
            &root,
        ))
        .unwrap();
        assert!(result.no_send);
        assert!(!result.authority_constructed);
        assert_eq!(result.evidence_files_pinned, 2);
        let serialized = serde_json::to_string(&result).unwrap();
        assert!(!serialized.contains(root.to_string_lossy().as_ref()));
        assert!(!serialized.contains("account"));
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn facade_rejects_symlink_and_hardlink_evidence() {
        use std::fs::hard_link;
        use std::os::unix::fs::symlink;
        let root = temp_directory("links");
        let package = root.join("accepted.zip");
        fs::write(&package, b"immutable-package").unwrap();
        fs::write(root.join(MANIFEST_NAME), b"{}").unwrap();
        let alias = root.join("alias.zip");
        symlink(&package, &alias).unwrap();
        assert_eq!(
            invoke_stage8b_operator_once(Stage8bOperatorInvocationRequest::new(
                "INVOCATION_TEST_0002",
                &alias,
                &root,
            )),
            Err(Stage8bOperatorFacadeError::UnsafeEvidencePath)
        );
        fs::remove_file(&alias).unwrap();
        hard_link(&package, &alias).unwrap();
        assert_eq!(
            invoke_stage8b_operator_once(Stage8bOperatorInvocationRequest::new(
                "INVOCATION_TEST_0003",
                &package,
                &root,
            )),
            Err(Stage8bOperatorFacadeError::UnsafeEvidencePath)
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn package_path_swap_after_open_is_rejected() {
        let root = temp_directory("path-swap");
        let package = root.join("accepted.zip");
        let original = root.join("accepted.original");
        fs::write(&package, b"accepted-original").unwrap();
        let result = read_pinned_regular_file_with_hook(&package, MAX_PACKAGE_BYTES, || {
            fs::rename(&package, &original).unwrap();
            fs::write(&package, b"attacker-replacement").unwrap();
        });
        assert_eq!(
            result,
            Err(Stage8bOperatorFacadeError::EvidenceIdentityDrift)
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn manifest_child_symlink_is_rejected_by_openat() {
        use std::os::unix::fs::symlink;
        let root = temp_directory("manifest-symlink");
        let outside = root.join("outside.json");
        fs::write(&outside, b"{}").unwrap();
        symlink(&outside, root.join(MANIFEST_NAME)).unwrap();
        assert_eq!(
            read_pinned_manifest(&root),
            Err(Stage8bOperatorFacadeError::InvalidEvidenceFile)
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn hmac_golden_vector_and_constant_time_verifier_match() {
        let key: Vec<u8> = (0u8..=31).collect();
        let account = b"ACC_TEST_0001";
        let binding = keyed_account_binding(Zeroizing::new(key.clone()), account).unwrap();
        assert_eq!(
            binding.binding_sha256,
            "60106309bd530bd0cec76c3fa78fa4b7004ef34e44447fb7cd78fdda87444435"
        );
        let expected = (0..32)
            .map(|index| {
                u8::from_str_radix(&binding.binding_sha256[index * 2..index * 2 + 2], 16).unwrap()
            })
            .collect::<Vec<_>>();
        verify_keyed_account_binding(Zeroizing::new(key.clone()), account, &expected).unwrap();
        assert_eq!(
            verify_keyed_account_binding(Zeroizing::new(key), b"ACC_TEST_0002", &expected),
            Err(Stage8bHmacError::Mismatch)
        );
    }

    #[test]
    fn classifier_bridge_uses_accepted_stage8a3_model() {
        let context = Stage8a3EndpointContext::for_place(
            StrategyRequestId::new(Uuid::new_v4()),
            ClientOrderId::new("CLIENT_TEST_0001").unwrap(),
            AccountId::new("ACC_TEST_0001"),
            InstrumentId {
                symbol: "IMOEXF".to_string(),
                venue_symbol: Some("IMOEXF@RTSX".to_string()),
                exchange: broker_core::Exchange::Moex,
                market: broker_core::Market::Futures,
            },
        )
        .unwrap();
        let classified = classify_stage8b_transport_observation_with_stage8a3(
            context,
            Stage8a3LocalHttpObservation::timeout(),
        );
        assert!(classified.diagnostic().reconciliation_required);
    }

    #[test]
    fn all_six_crash_windows_are_deterministic_and_never_retry() {
        let cases = [
            (
                Stage8bCrashWindow::BeforeAttempt,
                Stage8bClosureClassification::Stage8BClosedSafe,
            ),
            (
                Stage8bCrashWindow::AttemptCommittedNoTransport,
                Stage8bClosureClassification::Stage8BClosedSafe,
            ),
            (
                Stage8bCrashWindow::PossibleSendNoResponse,
                Stage8bClosureClassification::OutcomeUnknown,
            ),
            (
                Stage8bCrashWindow::ResponseNoDurableOutcome,
                Stage8bClosureClassification::OutcomeUnknown,
            ),
            (
                Stage8bCrashWindow::DurableOutcomeNoPublication,
                Stage8bClosureClassification::Stage8BClosedSafe,
            ),
            (
                Stage8bCrashWindow::RestartAtEveryBoundary,
                Stage8bClosureClassification::Stage8BClosedSafe,
            ),
        ];
        for (window, expected) in cases {
            assert_eq!(rehearse_crash_window(window), expected);
        }
    }

    #[test]
    fn durable_rehearsal_reopens_every_crash_boundary_without_resend() {
        let records = [
            Stage8bRehearsalRecord::AttemptCommitted,
            Stage8bRehearsalRecord::PossibleEffectObserved,
            Stage8bRehearsalRecord::ResponseObserved,
            Stage8bRehearsalRecord::DurableOutcomeRecorded,
            Stage8bRehearsalRecord::PublicationRecorded,
        ];
        let expected = [
            Stage8bClosureClassification::Stage8BClosedSafe,
            Stage8bClosureClassification::Stage8BClosedSafe,
            Stage8bClosureClassification::OutcomeUnknown,
            Stage8bClosureClassification::OutcomeUnknown,
            Stage8bClosureClassification::Stage8BClosedSafe,
            Stage8bClosureClassification::Stage8BClosedSafe,
        ];
        for (prefix, expected_classification) in expected.iter().enumerate() {
            let root = temp_directory(&format!("restart-{prefix}"));
            let mut journal = Stage8bNoSendRehearsalJournal::create(&root).unwrap();
            for record in records.iter().take(prefix) {
                journal.append(*record).unwrap();
            }
            drop(journal);
            assert_eq!(
                Stage8bNoSendRehearsalJournal::recover(&root).unwrap(),
                *expected_classification
            );
            fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn durable_rehearsal_rejects_impossible_or_corrupt_sequence() {
        let root = temp_directory("restart-invalid");
        let mut journal = Stage8bNoSendRehearsalJournal::create(&root).unwrap();
        journal
            .append(Stage8bRehearsalRecord::DurableOutcomeRecorded)
            .unwrap();
        drop(journal);
        assert_eq!(
            Stage8bNoSendRehearsalJournal::recover(&root),
            Err(Stage8bRehearsalError::InvalidSequence)
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn k1_through_k5_fault_injection_is_fail_closed() {
        for boundary in [
            Stage8bKillBoundary::K1,
            Stage8bKillBoundary::K2,
            Stage8bKillBoundary::K3,
        ] {
            assert_eq!(
                inject_kill_boundary(boundary),
                Stage8bClosureClassification::Stage8BClosedSafe
            );
        }
        for boundary in [Stage8bKillBoundary::K4, Stage8bKillBoundary::K5] {
            assert_eq!(
                inject_kill_boundary(boundary),
                Stage8bClosureClassification::OutcomeUnknown
            );
        }
    }

    #[test]
    fn stage8b_arm_subprocess_worker() {
        let Ok(root) = std::env::var("STAGE8B_I_ARM_WORKER_ROOT") else {
            return;
        };
        let identity = std::env::var("STAGE8B_I_ARM_WORKER_ID").unwrap();
        match issue_rehearsal_arm(Path::new(&root), &identity) {
            Ok(_) => {}
            Err(Stage8bArmIssueError::AlreadyIssued) => std::process::exit(23),
            Err(error) => panic!("unexpected arm error: {error:?}"),
        }
    }

    #[test]
    fn two_processes_cannot_issue_two_arms() {
        if std::env::var_os("STAGE8B_I_ARM_WORKER_ROOT").is_some() {
            return;
        }
        let root = temp_directory("arm-race");
        let identity = "a".repeat(64);
        let executable = std::env::current_exe().unwrap();
        let mut children = Vec::new();
        for _ in 0..2 {
            children.push(
                Command::new(&executable)
                    .arg("--exact")
                    .arg("stage8b_no_send::tests::stage8b_arm_subprocess_worker")
                    .arg("--nocapture")
                    .env("STAGE8B_I_ARM_WORKER_ROOT", &root)
                    .env("STAGE8B_I_ARM_WORKER_ID", &identity)
                    .spawn()
                    .unwrap(),
            );
        }
        let mut codes = children
            .into_iter()
            .map(|mut child| child.wait().unwrap().code().unwrap())
            .collect::<Vec<_>>();
        codes.sort_unstable();
        assert_eq!(codes, vec![0, 23]);
        assert_eq!(
            fs::read_dir(&root).unwrap().filter_map(Result::ok).count(),
            1
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn all_five_closure_classes_remain_distinct() {
        let values = [
            Stage8bClosureClassification::Stage8BClosedSafe,
            Stage8bClosureClassification::ResidualWorkingOrder,
            Stage8bClosureClassification::ResidualPosition,
            Stage8bClosureClassification::OutcomeUnknown,
            Stage8bClosureClassification::BrokerTruthConflict,
        ];
        for (index, left) in values.iter().enumerate() {
            for (other, right) in values.iter().enumerate() {
                assert_eq!(left == right, index == other);
            }
        }
    }
}
