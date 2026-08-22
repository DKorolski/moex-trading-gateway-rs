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
use std::collections::BTreeMap;
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
    config_sha256: String,
    policy_sha256: String,
    endpoint_identity_sha256: String,
}
#[allow(dead_code)]
pub(crate) struct Stage8bAcceptedRunSpec {
    run_sha256: String,
    body_sha256: String,
}
#[allow(dead_code)]
pub(crate) struct Stage8bK1ControlApproved {
    control_sha256: String,
}
#[allow(dead_code)]
pub(crate) struct Stage8bAuthenticatedOperatorArm {
    binding_sha256: String,
    expires_at_unix_ms: u64,
    verified_at_unix_ms: u64,
    authenticated_record_sha256: String,
}
#[allow(dead_code)]
struct Stage8bIssuedArmRecord {
    binding_sha256: String,
    expires_at_unix_ms: u64,
}
#[allow(dead_code)]
pub(crate) struct Stage8bK2FreshSources {
    evidence_sha256: String,
    observed_at_unix_ms: u64,
    single_finam_owner: bool,
    ambiguity_count: u32,
    unresolved_lifecycle_count: u32,
    readiness_fresh: bool,
    schedule_open_and_fresh: bool,
    broker_truth_fresh: bool,
    max_one_budget_remaining: u8,
}
#[allow(dead_code)]
pub(crate) struct Stage8bK3CoveringSealApproved {
    seal_sha256: String,
    control_sha256: String,
}
#[allow(dead_code)]
pub(crate) struct Stage8bK4ControlApproved {
    rechecked_attempt_sha256: String,
    control_sha256: String,
}
#[allow(dead_code)]
pub(crate) struct Stage8bK5ReconciliationApproved {
    broker_truth_sha256: String,
    control_sha256: String,
    closure: Stage8bClosureClassification,
}
#[allow(dead_code)]
pub(crate) struct Stage8bFreshPreflightApproved {
    binding_sha256: String,
    capability: Stage8a1CurrentlyAuthorizedCapability,
}
#[allow(dead_code)]
pub(crate) struct Stage8bSealedAttemptCommitted {
    attempt_sha256: String,
    capability: Stage8a1CurrentlyAuthorizedCapability,
}
#[allow(dead_code)]
pub(crate) struct Stage8bExactTransportPermit {
    permit_sha256: String,
    capability: Stage8a1CurrentlyAuthorizedCapability,
}
#[allow(dead_code)]
pub(crate) struct Stage8bPossibleEffectOwner {
    lifecycle_sha256: String,
}
#[allow(dead_code)]
pub(crate) struct Stage8bDurableClosureOwner {
    closure: Stage8bClosureClassification,
    closure_sha256: String,
}
#[allow(dead_code)]
pub(crate) struct Stage8bClosureReceipt {
    receipt_sha256: String,
}

#[allow(dead_code)]
struct Stage8bApprovedRequestParts {
    diagnostic: Stage8a2BuilderCompositionDiagnostic,
    permit_binding_sha256: String,
}

#[allow(dead_code)]
struct Stage8bExecutionBuildEvidence {
    source_ref: String,
    source_archive_sha256: String,
    source_member_manifest_sha256: String,
    cargo_lock_sha256: String,
    cargo_manifests_sha256: String,
    source_tree_before_sha256: String,
    source_tree_after_sha256: String,
    canonical_metadata_sha256: String,
    resolved_feature_graph_sha256: String,
    resolved_features: BTreeMap<String, bool>,
    unknown_feature_count: u32,
    cargo_version: String,
    rustc_release: String,
    rustc_commit_hash: String,
    rustc_commit_date: String,
    rustc_host: String,
    rustc_llvm_version: String,
    target_triple: String,
    profile: String,
    package: String,
    binary_sha256: String,
    config_sha256: String,
    policy_sha256: String,
    instrument_sha256: String,
    api_snapshot_sha256: String,
    endpoint_renderer_sha256: String,
    body_schema_sha256: String,
}

#[derive(Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
enum Stage8bEndpointMethod {
    Post,
    Delete,
}

#[derive(Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
enum Stage8bRouteTemplateId {
    PlaceOrderV1,
    CancelOrderV1,
}

#[allow(dead_code)]
struct Stage8bEndpointIdentity {
    identity_sha256: String,
}

#[allow(dead_code)]
struct Stage8bArmBindingEvidence {
    durable_request_sha256: String,
    run_sha256: String,
    account_binding_sha256: String,
    build_sha256: String,
    config_sha256: String,
    policy_sha256: String,
    endpoint_sha256: String,
    body_sha256: String,
    control_sha256: String,
    k2_sources_sha256: String,
    expires_at_unix_ms: u64,
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
    #[error("Stage 8B exact durable or current-source binding is invalid")]
    InvalidCrossBinding,
    #[error("Stage 8B execution-qualified build evidence is invalid")]
    InvalidBuildEvidence,
    #[error("Stage 8B endpoint identity is invalid")]
    InvalidEndpointIdentity,
    #[error("Stage 8B durable attempt rehearsal failed")]
    DurableAttempt,
}

#[allow(dead_code)]
fn compose_stage8b_private_request_parts_from_stage8a2(
    permit: Stage8bExactTransportPermit,
) -> Result<Stage8bApprovedRequestParts, Stage8bNoSendCompositionError> {
    let mut sink = Stage8a2InMemoryNoSendSink::new();
    let diagnostic = permit.capability.compose_stage8a2_no_send(&mut sink)?;
    Ok(Stage8bApprovedRequestParts {
        diagnostic,
        permit_binding_sha256: permit.permit_sha256,
    })
}

/// Single private K2 composition root. It binds and consumes the exact durable
/// authority, arm, run and fresh-source witness, but deliberately performs no
/// Stage 8A-2 request construction. The public facade cannot call this root.
#[allow(clippy::too_many_arguments, dead_code)]
pub(crate) fn compose_stage8b_effect_authority(
    capability: Stage8a1CurrentlyAuthorizedCapability,
    durable: Stage8a1DurableRequestAuthority,
    build: Stage8bExecutionQualifiedBuild,
    account: Stage8bKeyedAccountBinding,
    contract: Stage8bFreshContractAuthority,
    run: Stage8bAcceptedRunSpec,
    control: Stage8bK1ControlApproved,
    arm: Stage8bAuthenticatedOperatorArm,
    k2_sources: Stage8bK2FreshSources,
) -> Result<Stage8bFreshPreflightApproved, Stage8bNoSendCompositionError> {
    if !k2_sources.single_finam_owner
        || k2_sources.ambiguity_count != 0
        || k2_sources.unresolved_lifecycle_count != 0
        || !k2_sources.readiness_fresh
        || !k2_sources.schedule_open_and_fresh
        || !k2_sources.broker_truth_fresh
        || k2_sources.max_one_budget_remaining != 1
        || k2_sources.observed_at_unix_ms == 0
        || !is_lower_sha256(&k2_sources.evidence_sha256)
    {
        return Err(Stage8bNoSendCompositionError::InvalidCrossBinding);
    }
    let durable_request_sha256 = durable
        .into_stage8b_binding_sha256()
        .map_err(|_| Stage8bNoSendCompositionError::InvalidCrossBinding)?;
    let expected_arm_binding = calculate_arm_binding(&Stage8bArmBindingEvidence {
        durable_request_sha256: durable_request_sha256.clone(),
        run_sha256: run.run_sha256.clone(),
        account_binding_sha256: account.binding_sha256.clone(),
        build_sha256: build.identity_sha256.clone(),
        config_sha256: contract.config_sha256.clone(),
        policy_sha256: contract.policy_sha256.clone(),
        endpoint_sha256: contract.endpoint_identity_sha256.clone(),
        body_sha256: run.body_sha256.clone(),
        control_sha256: control.control_sha256.clone(),
        k2_sources_sha256: k2_sources.evidence_sha256.clone(),
        expires_at_unix_ms: arm.expires_at_unix_ms,
    })?;
    validate_authenticated_arm_for_k2(&arm, &expected_arm_binding, &k2_sources)?;
    let binding_sha256 = digest_parts(
        b"stage8b-i-r2-k2-preflight-v1",
        &[
            durable_request_sha256.as_bytes(),
            build.identity_sha256.as_bytes(),
            account.binding_sha256.as_bytes(),
            contract.contract_sha256.as_bytes(),
            contract.config_sha256.as_bytes(),
            contract.policy_sha256.as_bytes(),
            contract.endpoint_identity_sha256.as_bytes(),
            run.run_sha256.as_bytes(),
            run.body_sha256.as_bytes(),
            control.control_sha256.as_bytes(),
            arm.binding_sha256.as_bytes(),
            arm.authenticated_record_sha256.as_bytes(),
            &arm.expires_at_unix_ms.to_be_bytes(),
            &arm.verified_at_unix_ms.to_be_bytes(),
            k2_sources.evidence_sha256.as_bytes(),
            &k2_sources.observed_at_unix_ms.to_be_bytes(),
        ],
    );
    Ok(Stage8bFreshPreflightApproved {
        binding_sha256,
        capability,
    })
}

fn validate_authenticated_arm_for_k2(
    arm: &Stage8bAuthenticatedOperatorArm,
    expected_binding_sha256: &str,
    k2_sources: &Stage8bK2FreshSources,
) -> Result<(), Stage8bNoSendCompositionError> {
    if arm.binding_sha256 != expected_binding_sha256
        || !is_lower_sha256(&arm.authenticated_record_sha256)
        || arm.verified_at_unix_ms != k2_sources.observed_at_unix_ms
        || arm.expires_at_unix_ms <= k2_sources.observed_at_unix_ms
    {
        return Err(Stage8bNoSendCompositionError::InvalidCrossBinding);
    }
    Ok(())
}

#[allow(dead_code)]
fn commit_stage8b_sealed_attempt(
    preflight: Stage8bFreshPreflightApproved,
    k3: Stage8bK3CoveringSealApproved,
    journal: &mut Stage8bNoSendRehearsalJournal,
) -> Result<Stage8bSealedAttemptCommitted, Stage8bNoSendCompositionError> {
    if !is_lower_sha256(&k3.seal_sha256) || !is_lower_sha256(&k3.control_sha256) {
        return Err(Stage8bNoSendCompositionError::InvalidCrossBinding);
    }
    journal
        .append(Stage8bRehearsalRecord::AttemptCommitted)
        .map_err(|_| Stage8bNoSendCompositionError::DurableAttempt)?;
    Ok(Stage8bSealedAttemptCommitted {
        attempt_sha256: digest_parts(
            b"stage8b-i-r2-sealed-attempt-v1",
            &[
                preflight.binding_sha256.as_bytes(),
                k3.seal_sha256.as_bytes(),
                k3.control_sha256.as_bytes(),
            ],
        ),
        capability: preflight.capability,
    })
}

#[allow(dead_code)]
fn authorize_stage8b_exact_transport_permit(
    sealed: Stage8bSealedAttemptCommitted,
    k4: Stage8bK4ControlApproved,
) -> Result<Stage8bExactTransportPermit, Stage8bNoSendCompositionError> {
    if k4.rechecked_attempt_sha256 != sealed.attempt_sha256 || !is_lower_sha256(&k4.control_sha256)
    {
        return Err(Stage8bNoSendCompositionError::InvalidCrossBinding);
    }
    Ok(Stage8bExactTransportPermit {
        permit_sha256: digest_parts(
            b"stage8b-i-r2-exact-transport-permit-v1",
            &[
                sealed.attempt_sha256.as_bytes(),
                k4.control_sha256.as_bytes(),
            ],
        ),
        capability: sealed.capability,
    })
}

#[allow(dead_code)]
fn invoke_stage8b_local_no_network_boundary(
    parts: Stage8bApprovedRequestParts,
    journal: &mut Stage8bNoSendRehearsalJournal,
) -> Result<Stage8bPossibleEffectOwner, Stage8bNoSendCompositionError> {
    journal
        .append(Stage8bRehearsalRecord::PossibleEffectObserved)
        .map_err(|_| Stage8bNoSendCompositionError::DurableAttempt)?;
    Ok(Stage8bPossibleEffectOwner {
        lifecycle_sha256: digest_parts(
            b"stage8b-i-r2-local-no-network-boundary-v1",
            &[
                parts.permit_binding_sha256.as_bytes(),
                parts.diagnostic.authority_binding_sha256.as_bytes(),
                parts.diagnostic.request_shape_sha256.as_bytes(),
            ],
        ),
    })
}

#[allow(dead_code)]
fn reconcile_stage8b_possible_effect(
    possible: Stage8bPossibleEffectOwner,
    k5: Stage8bK5ReconciliationApproved,
    journal: &mut Stage8bNoSendRehearsalJournal,
) -> Result<Stage8bDurableClosureOwner, Stage8bNoSendCompositionError> {
    if !is_lower_sha256(&k5.broker_truth_sha256) || !is_lower_sha256(&k5.control_sha256) {
        return Err(Stage8bNoSendCompositionError::InvalidCrossBinding);
    }
    journal
        .append(Stage8bRehearsalRecord::ResponseObserved)
        .and_then(|()| journal.append(Stage8bRehearsalRecord::DurableOutcomeRecorded(k5.closure)))
        .map_err(|_| Stage8bNoSendCompositionError::DurableAttempt)?;
    Ok(Stage8bDurableClosureOwner {
        closure: k5.closure,
        closure_sha256: digest_parts(
            b"stage8b-i-r2-durable-closure-v1",
            &[
                possible.lifecycle_sha256.as_bytes(),
                k5.broker_truth_sha256.as_bytes(),
                k5.control_sha256.as_bytes(),
                k5.closure.code().as_bytes(),
            ],
        ),
    })
}

#[allow(dead_code)]
fn publish_stage8b_durable_closure(
    owner: Stage8bDurableClosureOwner,
    journal: &mut Stage8bNoSendRehearsalJournal,
) -> Result<Stage8bClosureReceipt, Stage8bNoSendCompositionError> {
    journal
        .append(Stage8bRehearsalRecord::PublicationRecorded(owner.closure))
        .map_err(|_| Stage8bNoSendCompositionError::DurableAttempt)?;
    Ok(Stage8bClosureReceipt {
        receipt_sha256: digest_parts(
            b"stage8b-i-r2-closure-receipt-v1",
            &[
                owner.closure_sha256.as_bytes(),
                owner.closure.code().as_bytes(),
            ],
        ),
    })
}

#[allow(dead_code)]
fn verify_execution_qualified_build(
    evidence: Stage8bExecutionBuildEvidence,
) -> Result<Stage8bExecutionQualifiedBuild, Stage8bNoSendCompositionError> {
    let required_hashes = [
        evidence.source_archive_sha256.as_str(),
        evidence.source_member_manifest_sha256.as_str(),
        evidence.cargo_lock_sha256.as_str(),
        evidence.cargo_manifests_sha256.as_str(),
        evidence.source_tree_before_sha256.as_str(),
        evidence.source_tree_after_sha256.as_str(),
        evidence.canonical_metadata_sha256.as_str(),
        evidence.resolved_feature_graph_sha256.as_str(),
        evidence.binary_sha256.as_str(),
        evidence.config_sha256.as_str(),
        evidence.policy_sha256.as_str(),
        evidence.instrument_sha256.as_str(),
        evidence.api_snapshot_sha256.as_str(),
        evidence.endpoint_renderer_sha256.as_str(),
        evidence.body_schema_sha256.as_str(),
    ];
    let expected_features = BTreeMap::from([
        ("broker-cli/m3j16-actual-one-shot".to_string(), false),
        ("finam-gateway/m3j16-actual-one-shot".to_string(), false),
    ]);
    if !is_lower_git_ref(&evidence.source_ref)
        || required_hashes.iter().any(|value| !is_lower_sha256(value))
        || evidence.source_tree_before_sha256 != evidence.source_tree_after_sha256
        || evidence.resolved_features != expected_features
        || evidence.unknown_feature_count != 0
        || evidence.profile != "release"
        || evidence.package != "broker-cli"
        || evidence.target_triple != evidence.rustc_host
        || evidence.cargo_version.trim().is_empty()
        || evidence.rustc_release.trim().is_empty()
        || !is_lower_git_ref(&evidence.rustc_commit_hash)
        || evidence.rustc_commit_date.trim().is_empty()
        || evidence.rustc_llvm_version.trim().is_empty()
    {
        return Err(Stage8bNoSendCompositionError::InvalidBuildEvidence);
    }
    let feature_projection = evidence
        .resolved_features
        .iter()
        .map(|(name, enabled)| format!("{name}={enabled}"))
        .collect::<Vec<_>>()
        .join("\n");
    let identity_sha256 = digest_parts(
        b"stage8b-i-r2-execution-qualified-build-v1",
        &[
            evidence.source_ref.as_bytes(),
            evidence.source_archive_sha256.as_bytes(),
            evidence.source_member_manifest_sha256.as_bytes(),
            evidence.cargo_lock_sha256.as_bytes(),
            evidence.cargo_manifests_sha256.as_bytes(),
            evidence.source_tree_before_sha256.as_bytes(),
            evidence.canonical_metadata_sha256.as_bytes(),
            evidence.resolved_feature_graph_sha256.as_bytes(),
            feature_projection.as_bytes(),
            evidence.cargo_version.as_bytes(),
            evidence.rustc_release.as_bytes(),
            evidence.rustc_commit_hash.as_bytes(),
            evidence.rustc_commit_date.as_bytes(),
            evidence.rustc_host.as_bytes(),
            evidence.rustc_llvm_version.as_bytes(),
            evidence.target_triple.as_bytes(),
            evidence.profile.as_bytes(),
            evidence.package.as_bytes(),
            evidence.binary_sha256.as_bytes(),
            evidence.config_sha256.as_bytes(),
            evidence.policy_sha256.as_bytes(),
            evidence.instrument_sha256.as_bytes(),
            evidence.api_snapshot_sha256.as_bytes(),
            evidence.endpoint_renderer_sha256.as_bytes(),
            evidence.body_schema_sha256.as_bytes(),
        ],
    );
    Ok(Stage8bExecutionQualifiedBuild { identity_sha256 })
}

#[allow(dead_code)]
fn compose_endpoint_identity(
    method: Stage8bEndpointMethod,
    route: Stage8bRouteTemplateId,
    account: &Stage8bKeyedAccountBinding,
    endpoint_renderer_sha256: &str,
) -> Result<Stage8bEndpointIdentity, Stage8bNoSendCompositionError> {
    let pair = match (method, route) {
        (Stage8bEndpointMethod::Post, Stage8bRouteTemplateId::PlaceOrderV1) => {
            (b"POST".as_slice(), b"PlaceOrderV1".as_slice())
        }
        (Stage8bEndpointMethod::Delete, Stage8bRouteTemplateId::CancelOrderV1) => {
            (b"DELETE".as_slice(), b"CancelOrderV1".as_slice())
        }
        _ => return Err(Stage8bNoSendCompositionError::InvalidEndpointIdentity),
    };
    if !is_lower_sha256(&account.binding_sha256) || !is_lower_sha256(endpoint_renderer_sha256) {
        return Err(Stage8bNoSendCompositionError::InvalidEndpointIdentity);
    }
    Ok(Stage8bEndpointIdentity {
        identity_sha256: digest_parts(
            b"stage8b-i-r2-endpoint-identity-v1",
            &[
                pair.0,
                pair.1,
                account.binding_sha256.as_bytes(),
                endpoint_renderer_sha256.as_bytes(),
            ],
        ),
    })
}

fn calculate_arm_binding(
    evidence: &Stage8bArmBindingEvidence,
) -> Result<String, Stage8bNoSendCompositionError> {
    let hashes = [
        evidence.durable_request_sha256.as_str(),
        evidence.run_sha256.as_str(),
        evidence.account_binding_sha256.as_str(),
        evidence.build_sha256.as_str(),
        evidence.config_sha256.as_str(),
        evidence.policy_sha256.as_str(),
        evidence.endpoint_sha256.as_str(),
        evidence.body_sha256.as_str(),
        evidence.control_sha256.as_str(),
        evidence.k2_sources_sha256.as_str(),
    ];
    if hashes.iter().any(|value| !is_lower_sha256(value)) || evidence.expires_at_unix_ms == 0 {
        return Err(Stage8bNoSendCompositionError::InvalidCrossBinding);
    }
    Ok(digest_parts(
        b"stage8b-i-r2-exact-arm-binding-v1",
        &[
            evidence.durable_request_sha256.as_bytes(),
            evidence.run_sha256.as_bytes(),
            evidence.account_binding_sha256.as_bytes(),
            evidence.build_sha256.as_bytes(),
            evidence.config_sha256.as_bytes(),
            evidence.policy_sha256.as_bytes(),
            evidence.endpoint_sha256.as_bytes(),
            evidence.body_sha256.as_bytes(),
            evidence.control_sha256.as_bytes(),
            evidence.k2_sources_sha256.as_bytes(),
            &evidence.expires_at_unix_ms.to_be_bytes(),
        ],
    ))
}

fn is_lower_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn is_lower_git_ref(value: &str) -> bool {
    value.len() == 40
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
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

const REHEARSAL_JOURNAL_HEADER: &[u8] = b"STAGE8B-I-NO-SEND-V2\n";

#[derive(Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
enum Stage8bRehearsalRecord {
    AttemptCommitted,
    PossibleEffectObserved,
    ResponseObserved,
    DurableOutcomeRecorded(Stage8bClosureClassification),
    PublicationRecorded(Stage8bClosureClassification),
}

impl Stage8bRehearsalRecord {
    fn encoded(self) -> String {
        match self {
            Self::AttemptCommitted => "A".to_string(),
            Self::PossibleEffectObserved => "P".to_string(),
            Self::ResponseObserved => "R".to_string(),
            Self::DurableOutcomeRecorded(closure) => format!("D:{}", closure.code()),
            Self::PublicationRecorded(closure) => format!("U:{}", closure.code()),
        }
    }
}

impl Stage8bClosureClassification {
    fn code(self) -> &'static str {
        match self {
            Self::Stage8BClosedSafe => "closed-safe",
            Self::ResidualWorkingOrder => "residual-working-order",
            Self::ResidualPosition => "residual-position",
            Self::OutcomeUnknown => "outcome-unknown",
            Self::BrokerTruthConflict => "broker-truth-conflict",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value {
            "closed-safe" => Some(Self::Stage8BClosedSafe),
            "residual-working-order" => Some(Self::ResidualWorkingOrder),
            "residual-position" => Some(Self::ResidualPosition),
            "outcome-unknown" => Some(Self::OutcomeUnknown),
            "broker-truth-conflict" => Some(Self::BrokerTruthConflict),
            _ => None,
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
        let encoded = record.encoded();
        self.file
            .write_all(format!("{encoded}\n").as_bytes())
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
        if !body.is_empty() && !body.ends_with(b"\n") {
            return Err(Stage8bRehearsalError::InvalidSequence);
        }
        let lines = std::str::from_utf8(body)
            .map_err(|_| Stage8bRehearsalError::InvalidSequence)?
            .lines()
            .collect::<Vec<_>>();
        match lines.as_slice() {
            [] | ["A"] => Ok(Stage8bClosureClassification::Stage8BClosedSafe),
            ["A", "P"] | ["A", "P", "R"] => Ok(Stage8bClosureClassification::OutcomeUnknown),
            ["A", "P", "R", durable] => durable
                .strip_prefix("D:")
                .and_then(Stage8bClosureClassification::parse)
                .ok_or(Stage8bRehearsalError::InvalidSequence),
            ["A", "P", "R", durable, publication] => {
                let durable = durable
                    .strip_prefix("D:")
                    .and_then(Stage8bClosureClassification::parse)
                    .ok_or(Stage8bRehearsalError::InvalidSequence)?;
                let publication = publication
                    .strip_prefix("U:")
                    .and_then(Stage8bClosureClassification::parse)
                    .ok_or(Stage8bRehearsalError::InvalidSequence)?;
                if durable == publication {
                    Ok(durable)
                } else {
                    Err(Stage8bRehearsalError::InvalidSequence)
                }
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
    binding: Stage8bCanonicalBindingDigest,
    expires_at_unix_ms: u64,
    observed_at_unix_ms: u64,
    authentication_key: Zeroizing<Vec<u8>>,
) -> Result<Stage8bIssuedArmRecord, Stage8bArmIssueError> {
    if expires_at_unix_ms <= observed_at_unix_ms || authentication_key.len() < MIN_HMAC_KEY_BYTES {
        return Err(Stage8bArmIssueError::InvalidIdentity);
    }
    reject_symlink_components(registry).map_err(|_| Stage8bArmIssueError::UnsafeRegistry)?;
    let directory =
        open_no_follow(registry, true).map_err(|_| Stage8bArmIssueError::UnsafeRegistry)?;
    let binding_sha256 = binding.to_lower_hex();
    let filename = format!("arm-{binding_sha256}.record");
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
    let mut mac = Hmac::<Sha256>::new_from_slice(authentication_key.as_slice())
        .map_err(|_| Stage8bArmIssueError::InvalidIdentity)?;
    mac.update(b"stage8b-i-r2-durable-arm-record-v1");
    mac.update(binding.as_bytes());
    mac.update(&expires_at_unix_ms.to_be_bytes());
    let tag = hex_lower(&mac.finalize().into_bytes());
    let record = format!(
        "STAGE8B-I-R2-ARM-V1\nbinding={binding_sha256}\nexpires_at_unix_ms={expires_at_unix_ms}\nstate=issued\ntag={tag}\n"
    );
    file.write_all(record.as_bytes())
        .map_err(|_| Stage8bArmIssueError::Io)?;
    file.sync_all().map_err(|_| Stage8bArmIssueError::Io)?;
    directory.sync_all().map_err(|_| Stage8bArmIssueError::Io)?;
    Ok(Stage8bIssuedArmRecord {
        binding_sha256,
        expires_at_unix_ms,
    })
}

#[allow(dead_code)]
fn verify_rehearsal_arm_record(
    registry: &Path,
    binding: Stage8bCanonicalBindingDigest,
    expected_expiry: u64,
    observed_at_unix_ms: u64,
    authentication_key: Zeroizing<Vec<u8>>,
) -> Result<Stage8bAuthenticatedOperatorArm, Stage8bArmIssueError> {
    if expected_expiry <= observed_at_unix_ms || authentication_key.len() < MIN_HMAC_KEY_BYTES {
        return Err(Stage8bArmIssueError::InvalidIdentity);
    }
    let binding_sha256 = binding.to_lower_hex();
    let path = registry.join(format!("arm-{binding_sha256}.record"));
    let bytes =
        read_pinned_regular_file(&path, 1024).map_err(|_| Stage8bArmIssueError::UnsafeRegistry)?;
    let text = std::str::from_utf8(&bytes).map_err(|_| Stage8bArmIssueError::Authentication)?;
    let fields = text
        .lines()
        .skip(1)
        .filter_map(|line| line.split_once('='))
        .collect::<BTreeMap<_, _>>();
    let expected_expiry_text = expected_expiry.to_string();
    if text.lines().next() != Some("STAGE8B-I-R2-ARM-V1")
        || fields.get("binding") != Some(&binding_sha256.as_str())
        || fields.get("expires_at_unix_ms") != Some(&expected_expiry_text.as_str())
        || fields.get("state") != Some(&"issued")
    {
        return Err(Stage8bArmIssueError::Authentication);
    }
    let expected_tag = fields
        .get("tag")
        .ok_or(Stage8bArmIssueError::Authentication)?;
    let expected_tag =
        decode_lower_sha256(expected_tag).ok_or(Stage8bArmIssueError::Authentication)?;
    let mut mac = Hmac::<Sha256>::new_from_slice(authentication_key.as_slice())
        .map_err(|_| Stage8bArmIssueError::InvalidIdentity)?;
    mac.update(b"stage8b-i-r2-durable-arm-record-v1");
    mac.update(binding.as_bytes());
    mac.update(&expected_expiry.to_be_bytes());
    mac.verify_slice(&expected_tag)
        .map_err(|_| Stage8bArmIssueError::Authentication)?;
    let issued_record_sha256 = sha256_hex(&bytes);
    let directory =
        open_no_follow(registry, false).map_err(|_| Stage8bArmIssueError::UnsafeRegistry)?;
    let consumed_filename = format!("arm-{binding_sha256}.consumed");
    let consumed_name =
        CString::new(consumed_filename).map_err(|_| Stage8bArmIssueError::InvalidIdentity)?;
    // SAFETY: the pinned directory and C string remain valid. O_EXCL makes
    // authenticated arm consumption durable and single-winner across restart.
    let consumed_descriptor = unsafe {
        libc::openat(
            directory.as_raw_fd(),
            consumed_name.as_ptr(),
            libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            0o600,
        )
    };
    if consumed_descriptor < 0 {
        return if std::io::Error::last_os_error().kind() == std::io::ErrorKind::AlreadyExists {
            Err(Stage8bArmIssueError::AlreadyConsumed)
        } else {
            Err(Stage8bArmIssueError::Io)
        };
    }
    let mut consumed_mac = Hmac::<Sha256>::new_from_slice(authentication_key.as_slice())
        .map_err(|_| Stage8bArmIssueError::InvalidIdentity)?;
    consumed_mac.update(b"stage8b-i-r2-durable-arm-consumed-v1");
    consumed_mac.update(binding.as_bytes());
    consumed_mac.update(&expected_expiry.to_be_bytes());
    consumed_mac.update(issued_record_sha256.as_bytes());
    let consumed_tag = hex_lower(&consumed_mac.finalize().into_bytes());
    let consumed_record = format!(
        "STAGE8B-I-R2-ARM-CONSUMED-V1\nbinding={binding_sha256}\nexpires_at_unix_ms={expected_expiry}\nissued_record_sha256={issued_record_sha256}\nstate=consumed\ntag={consumed_tag}\n"
    );
    // SAFETY: successful openat transfers one owned descriptor.
    let mut consumed_file = unsafe { File::from_raw_fd(consumed_descriptor) };
    consumed_file
        .write_all(consumed_record.as_bytes())
        .map_err(|_| Stage8bArmIssueError::Io)?;
    consumed_file
        .sync_all()
        .map_err(|_| Stage8bArmIssueError::Io)?;
    directory.sync_all().map_err(|_| Stage8bArmIssueError::Io)?;
    Ok(Stage8bAuthenticatedOperatorArm {
        binding_sha256,
        expires_at_unix_ms: expected_expiry,
        verified_at_unix_ms: observed_at_unix_ms,
        authenticated_record_sha256: sha256_hex(consumed_record.as_bytes()),
    })
}

struct Stage8bCanonicalBindingDigest([u8; 32]);

impl Stage8bCanonicalBindingDigest {
    #[allow(dead_code)]
    fn from_lower_hex(value: &str) -> Result<Self, Stage8bArmIssueError> {
        decode_lower_sha256(value)
            .map(Self)
            .ok_or(Stage8bArmIssueError::InvalidIdentity)
    }

    fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    fn to_lower_hex(&self) -> String {
        hex_lower(&self.0)
    }
}

fn decode_lower_sha256(value: &str) -> Option<[u8; 32]> {
    if !is_lower_sha256(value) {
        return None;
    }
    let mut output = [0u8; 32];
    for (index, byte) in output.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16).ok()?;
    }
    Some(output)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
enum Stage8bArmIssueError {
    InvalidIdentity,
    UnsafeRegistry,
    AlreadyIssued,
    AlreadyConsumed,
    Authentication,
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

    fn build_evidence_fixture() -> Stage8bExecutionBuildEvidence {
        let hash = |label: &str| sha256_hex(label.as_bytes());
        Stage8bExecutionBuildEvidence {
            source_ref: "1".repeat(40),
            source_archive_sha256: hash("source-archive"),
            source_member_manifest_sha256: hash("member-mode-manifest"),
            cargo_lock_sha256: hash("cargo-lock"),
            cargo_manifests_sha256: hash("cargo-manifests"),
            source_tree_before_sha256: hash("source-tree"),
            source_tree_after_sha256: hash("source-tree"),
            canonical_metadata_sha256: hash("canonical-metadata-no-local-paths"),
            resolved_feature_graph_sha256: hash("fully-resolved-feature-graph"),
            resolved_features: BTreeMap::from([
                ("broker-cli/m3j16-actual-one-shot".to_string(), false),
                ("finam-gateway/m3j16-actual-one-shot".to_string(), false),
            ]),
            unknown_feature_count: 0,
            cargo_version: "cargo 1.95.0".to_string(),
            rustc_release: "1.95.0".to_string(),
            rustc_commit_hash: "2".repeat(40),
            rustc_commit_date: "2026-07-01".to_string(),
            rustc_host: "aarch64-apple-darwin".to_string(),
            rustc_llvm_version: "21.1.0".to_string(),
            target_triple: "aarch64-apple-darwin".to_string(),
            profile: "release".to_string(),
            package: "broker-cli".to_string(),
            binary_sha256: hash("broker-cli-release-binary"),
            config_sha256: hash("runtime-config"),
            policy_sha256: hash("execution-policy"),
            instrument_sha256: hash("IMOEXF@RTSX"),
            api_snapshot_sha256: hash("accepted-finam-api"),
            endpoint_renderer_sha256: hash("accepted-endpoint-renderer"),
            body_schema_sha256: hash("accepted-body-schema"),
        }
    }

    #[test]
    fn execution_build_verifier_binds_source_features_metadata_toolchain_and_binary() {
        let accepted = verify_execution_qualified_build(build_evidence_fixture()).unwrap();
        assert!(is_lower_sha256(&accepted.identity_sha256));

        let mut legacy_enabled = build_evidence_fixture();
        legacy_enabled
            .resolved_features
            .insert("finam-gateway/m3j16-actual-one-shot".to_string(), true);
        assert!(verify_execution_qualified_build(legacy_enabled).is_err());

        let mut unknown = build_evidence_fixture();
        unknown.unknown_feature_count = 1;
        assert!(verify_execution_qualified_build(unknown).is_err());

        let mut tree_drift = build_evidence_fixture();
        tree_drift.source_tree_after_sha256 = sha256_hex(b"different-tree");
        assert!(verify_execution_qualified_build(tree_drift).is_err());

        let mut local_target_drift = build_evidence_fixture();
        local_target_drift.target_triple = "x86_64-unknown-linux-gnu".to_string();
        assert!(verify_execution_qualified_build(local_target_drift).is_err());
    }

    #[test]
    fn endpoint_identity_binds_method_template_account_and_renderer() {
        let account = Stage8bKeyedAccountBinding {
            binding_sha256: sha256_hex(b"keyed-account"),
        };
        let renderer = sha256_hex(b"endpoint-renderer");
        let place = compose_endpoint_identity(
            Stage8bEndpointMethod::Post,
            Stage8bRouteTemplateId::PlaceOrderV1,
            &account,
            &renderer,
        )
        .unwrap();
        let cancel = compose_endpoint_identity(
            Stage8bEndpointMethod::Delete,
            Stage8bRouteTemplateId::CancelOrderV1,
            &account,
            &renderer,
        )
        .unwrap();
        assert_ne!(place.identity_sha256, cancel.identity_sha256);
        assert!(compose_endpoint_identity(
            Stage8bEndpointMethod::Post,
            Stage8bRouteTemplateId::CancelOrderV1,
            &account,
            &renderer,
        )
        .is_err());
        let other_account = Stage8bKeyedAccountBinding {
            binding_sha256: sha256_hex(b"other-keyed-account"),
        };
        assert_ne!(
            place.identity_sha256,
            compose_endpoint_identity(
                Stage8bEndpointMethod::Post,
                Stage8bRouteTemplateId::PlaceOrderV1,
                &other_account,
                &renderer,
            )
            .unwrap()
            .identity_sha256
        );
    }

    #[test]
    fn arm_binding_changes_for_each_exact_durable_run_and_k2_component() {
        let hash = |label: &str| sha256_hex(label.as_bytes());
        let baseline = Stage8bArmBindingEvidence {
            durable_request_sha256: hash("durable"),
            run_sha256: hash("run"),
            account_binding_sha256: hash("account"),
            build_sha256: hash("build"),
            config_sha256: hash("config"),
            policy_sha256: hash("policy"),
            endpoint_sha256: hash("endpoint"),
            body_sha256: hash("body"),
            control_sha256: hash("control"),
            k2_sources_sha256: hash("k2-sources"),
            expires_at_unix_ms: 2_000,
        };
        let expected = calculate_arm_binding(&baseline).unwrap();
        for replacement in [
            "durable_request_sha256",
            "run_sha256",
            "account_binding_sha256",
            "build_sha256",
            "config_sha256",
            "policy_sha256",
            "endpoint_sha256",
            "body_sha256",
            "control_sha256",
            "k2_sources_sha256",
        ] {
            let mut changed = Stage8bArmBindingEvidence {
                durable_request_sha256: baseline.durable_request_sha256.clone(),
                run_sha256: baseline.run_sha256.clone(),
                account_binding_sha256: baseline.account_binding_sha256.clone(),
                build_sha256: baseline.build_sha256.clone(),
                config_sha256: baseline.config_sha256.clone(),
                policy_sha256: baseline.policy_sha256.clone(),
                endpoint_sha256: baseline.endpoint_sha256.clone(),
                body_sha256: baseline.body_sha256.clone(),
                control_sha256: baseline.control_sha256.clone(),
                k2_sources_sha256: baseline.k2_sources_sha256.clone(),
                expires_at_unix_ms: baseline.expires_at_unix_ms,
            };
            let value = hash(&format!("changed-{replacement}"));
            match replacement {
                "durable_request_sha256" => changed.durable_request_sha256 = value,
                "run_sha256" => changed.run_sha256 = value,
                "account_binding_sha256" => changed.account_binding_sha256 = value,
                "build_sha256" => changed.build_sha256 = value,
                "config_sha256" => changed.config_sha256 = value,
                "policy_sha256" => changed.policy_sha256 = value,
                "endpoint_sha256" => changed.endpoint_sha256 = value,
                "body_sha256" => changed.body_sha256 = value,
                "control_sha256" => changed.control_sha256 = value,
                "k2_sources_sha256" => changed.k2_sources_sha256 = value,
                _ => unreachable!(),
            }
            assert_ne!(calculate_arm_binding(&changed).unwrap(), expected);
        }
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
            Stage8bRehearsalRecord::DurableOutcomeRecorded(
                Stage8bClosureClassification::Stage8BClosedSafe,
            ),
            Stage8bRehearsalRecord::PublicationRecorded(
                Stage8bClosureClassification::Stage8BClosedSafe,
            ),
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
            .append(Stage8bRehearsalRecord::DurableOutcomeRecorded(
                Stage8bClosureClassification::Stage8BClosedSafe,
            ))
            .unwrap();
        drop(journal);
        assert_eq!(
            Stage8bNoSendRehearsalJournal::recover(&root),
            Err(Stage8bRehearsalError::InvalidSequence)
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn every_closure_class_survives_pre_and_post_publication_restart() {
        for closure in [
            Stage8bClosureClassification::Stage8BClosedSafe,
            Stage8bClosureClassification::ResidualWorkingOrder,
            Stage8bClosureClassification::ResidualPosition,
            Stage8bClosureClassification::OutcomeUnknown,
            Stage8bClosureClassification::BrokerTruthConflict,
        ] {
            let root = temp_directory(closure.code());
            let mut journal = Stage8bNoSendRehearsalJournal::create(&root).unwrap();
            for record in [
                Stage8bRehearsalRecord::AttemptCommitted,
                Stage8bRehearsalRecord::PossibleEffectObserved,
                Stage8bRehearsalRecord::ResponseObserved,
                Stage8bRehearsalRecord::DurableOutcomeRecorded(closure),
            ] {
                journal.append(record).unwrap();
            }
            drop(journal);
            assert_eq!(Stage8bNoSendRehearsalJournal::recover(&root), Ok(closure));
            let path = root.join("stage8b-i-rehearsal.journal");
            let mut journal = std::fs::OpenOptions::new().append(true).open(path).unwrap();
            journal
                .write_all(format!("U:{}\n", closure.code()).as_bytes())
                .unwrap();
            journal.sync_all().unwrap();
            drop(journal);
            assert_eq!(Stage8bNoSendRehearsalJournal::recover(&root), Ok(closure));
            fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn corrupt_unknown_or_mismatched_closure_payload_fails_closed() {
        for suffix in [
            "A\nP\nR\nD:not-a-closure\n",
            "A\nP\nR\nD:residual-position",
            "A\nP\nR\nD:residual-position\nU:closed-safe\n",
        ] {
            let root = temp_directory("closure-corrupt");
            fs::write(
                root.join("stage8b-i-rehearsal.journal"),
                [REHEARSAL_JOURNAL_HEADER, suffix.as_bytes()].concat(),
            )
            .unwrap();
            assert_eq!(
                Stage8bNoSendRehearsalJournal::recover(&root),
                Err(Stage8bRehearsalError::InvalidSequence)
            );
            fs::remove_dir_all(root).unwrap();
        }
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
        let binding = Stage8bCanonicalBindingDigest::from_lower_hex(&identity).unwrap();
        match issue_rehearsal_arm(
            Path::new(&root),
            binding,
            2_000,
            1_000,
            Zeroizing::new(vec![7; 32]),
        ) {
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
    fn arm_identity_is_canonical_authenticated_and_expiry_bound() {
        let root = temp_directory("arm-binding");
        let identity = "ab".repeat(32);
        assert!(Stage8bCanonicalBindingDigest::from_lower_hex(&identity).is_ok());
        assert_eq!(
            Stage8bCanonicalBindingDigest::from_lower_hex(&identity.to_uppercase()).err(),
            Some(Stage8bArmIssueError::InvalidIdentity)
        );
        let arm = issue_rehearsal_arm(
            &root,
            Stage8bCanonicalBindingDigest::from_lower_hex(&identity).unwrap(),
            2_000,
            1_000,
            Zeroizing::new(vec![9; 32]),
        )
        .unwrap();
        assert_eq!(arm.binding_sha256, identity);
        assert_eq!(arm.expires_at_unix_ms, 2_000);
        assert_eq!(
            verify_rehearsal_arm_record(
                &root,
                Stage8bCanonicalBindingDigest::from_lower_hex(&identity).unwrap(),
                2_000,
                1_500,
                Zeroizing::new(vec![8; 32]),
            )
            .err(),
            Some(Stage8bArmIssueError::Authentication)
        );
        assert_eq!(
            verify_rehearsal_arm_record(
                &root,
                Stage8bCanonicalBindingDigest::from_lower_hex(&identity).unwrap(),
                2_000,
                2_000,
                Zeroizing::new(vec![9; 32]),
            )
            .err(),
            Some(Stage8bArmIssueError::InvalidIdentity)
        );
        let authenticated = verify_rehearsal_arm_record(
            &root,
            Stage8bCanonicalBindingDigest::from_lower_hex(&identity).unwrap(),
            2_000,
            1_500,
            Zeroizing::new(vec![9; 32]),
        )
        .unwrap();
        assert_eq!(authenticated.verified_at_unix_ms, 1_500);
        assert!(is_lower_sha256(&authenticated.authenticated_record_sha256));
        assert_eq!(
            verify_rehearsal_arm_record(
                &root,
                Stage8bCanonicalBindingDigest::from_lower_hex(&identity).unwrap(),
                2_000,
                1_500,
                Zeroizing::new(vec![9; 32]),
            )
            .err(),
            Some(Stage8bArmIssueError::AlreadyConsumed)
        );
        assert_eq!(
            issue_rehearsal_arm(
                &root,
                Stage8bCanonicalBindingDigest::from_lower_hex(&identity).unwrap(),
                3_000,
                1_000,
                Zeroizing::new(vec![9; 32]),
            )
            .err(),
            Some(Stage8bArmIssueError::AlreadyIssued)
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn k2_accepts_only_fresh_authenticated_arm_capability() {
        let identity = sha256_hex(b"exact-complete-arm-binding");
        let record = sha256_hex(b"authenticated-arm-record");
        let sources = Stage8bK2FreshSources {
            evidence_sha256: sha256_hex(b"fresh-k2-sources"),
            observed_at_unix_ms: 1_500,
            single_finam_owner: true,
            ambiguity_count: 0,
            unresolved_lifecycle_count: 0,
            readiness_fresh: true,
            schedule_open_and_fresh: true,
            broker_truth_fresh: true,
            max_one_budget_remaining: 1,
        };
        let authenticated = Stage8bAuthenticatedOperatorArm {
            binding_sha256: identity.clone(),
            expires_at_unix_ms: 2_000,
            verified_at_unix_ms: 1_500,
            authenticated_record_sha256: record.clone(),
        };
        validate_authenticated_arm_for_k2(&authenticated, &identity, &sources).unwrap();

        let expired = Stage8bAuthenticatedOperatorArm {
            binding_sha256: identity.clone(),
            expires_at_unix_ms: 1_500,
            verified_at_unix_ms: 1_500,
            authenticated_record_sha256: record.clone(),
        };
        assert!(validate_authenticated_arm_for_k2(&expired, &identity, &sources).is_err());

        let stale_verification = Stage8bAuthenticatedOperatorArm {
            binding_sha256: identity.clone(),
            expires_at_unix_ms: 2_000,
            verified_at_unix_ms: 1_499,
            authenticated_record_sha256: record,
        };
        assert!(
            validate_authenticated_arm_for_k2(&stale_verification, &identity, &sources).is_err()
        );
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
