//! Stage 8B-P1-f root-guardian authority source.
//!
//! This module deliberately owns no Redis connection, FINAM client, broker
//! dispatch or child-process launcher.  It implements the local, fail-closed
//! authority boundary which must be accepted before any operational P1-f
//! installation or service start is allowed.

use std::{
    collections::BTreeSet,
    ffi::CString,
    fs::{self, File, OpenOptions},
    io::{ErrorKind, Read, Write},
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::{
            ffi::OsStrExt,
            fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt},
        },
    },
    path::{Component, Path, PathBuf},
    time::{Duration as StdDuration, Instant},
};

use chrono::{DateTime, SecondsFormat, Utc};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

#[cfg(test)]
use std::sync::atomic::{AtomicU8, Ordering as AtomicOrdering};

#[cfg(test)]
static P1F_TEST_FAULT_POINT: AtomicU8 = AtomicU8::new(0);

#[cfg(test)]
fn inject_test_fault(point: u8) -> Result<(), Stage8bP1fAuthorityErrorV1> {
    if P1F_TEST_FAULT_POINT
        .compare_exchange(point, 0, AtomicOrdering::SeqCst, AtomicOrdering::SeqCst)
        .is_ok()
    {
        return Err(Stage8bP1fAuthorityErrorV1::Io(ErrorKind::Interrupted));
    }
    Ok(())
}

#[cfg(not(test))]
#[inline]
fn inject_test_fault(_point: u8) -> Result<(), Stage8bP1fAuthorityErrorV1> {
    Ok(())
}

pub const STAGE8B_P1F_AUTHORITY_SCHEMA_VERSION: u16 = 1;
pub const STAGE8B_P1F_AUTHORITY_CONTROL_ROOT: &str = "/var/lib/moex-finam-p1-paper-control";
pub const STAGE8B_P1F_TARGET_HOST_ID: &str = "stage8b-p1f-isolated-vps-1";
pub const STAGE8B_P1F_TARGET_HOST_SSH_ED25519_SHA256: &str =
    "SHA256:8fOAPkfvZ61LYmldUBs6A5ZC+BFr6O3yCvUrBAWZsAo";
pub const STAGE8B_P1F_SERVICE_USER: &str = "moex-p1-paper";
pub const STAGE8B_P1F_MAX_AUTHORITY_BYTES: u64 = 128 * 1024;
pub const STAGE8B_P1F_CONFIG_ROOT: &str = "/etc/moex-finam-p1-paper";
pub const STAGE8B_P1F_SOURCE_SHA256_TEMPLATE_SENTINEL: &str =
    "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff";

const AUTHORITY_DIRECTORY: &str = "authority";
const EVENTS_DIRECTORY: &str = "events";
const MANIFESTS_DIRECTORY: &str = "manifests";
const LOCK_FILE: &str = ".guardian.lock";
const EXECUTION_LOCK_FILE: &str = ".execution.lock";
const GENESIS_TRANSACTION_FILE: &str = "genesis-transaction.json";
const GENESIS_MANIFEST_FILE: &str = "genesis-manifest.json";
const GENESIS_RECEIPT_FILE: &str = "genesis-receipt.json";
const ACTIVATION_CERTIFICATE_FILE: &str = "activation-certificate.json";
const HISTORY_HEAD_FILE: &str = "history-head.json";
const PENDING_CLAIM_FILE: &str = "pending-claim.json";
const PENDING_TERMINAL_FILE: &str = "pending-terminal.json";
const PENDING_STOPPING_FILE: &str = "pending-stopping.json";
const PENDING_MATERIALIZATION_FILE: &str = "pending-materialization.json";
const MATERIALIZED_SET_RECEIPT_FILE: &str = "materialized-set-receipt.json";
const EXECUTION_OWNER_FILE: &str = "execution-owner.json";
const QUARANTINE_FILE: &str = "restore-quarantine.json";
const ZERO_SHA256: &str = "0000000000000000000000000000000000000000000000000000000000000000";
const GENESIS_DOMAIN: &[u8] = b"moex.stage8b.p1f.authority-genesis.v1\0";
const EVENT_DOMAIN: &[u8] = b"moex.stage8b.p1f.authority-event.v1\0";
const SIGNED_GENESIS_DOMAIN: &[u8] = b"moex.stage8b.p1f.signed-genesis.v1\0";
const SIGNED_ACTIVATION_DOMAIN: &[u8] = b"moex.stage8b.p1f.signed-activation.v1\0";
const SIGNED_PHASE_DOMAIN: &[u8] = b"moex.stage8b.p1f.signed-phase-manifest.v1\0";

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Stage8bP1fAuthorityErrorV1 {
    #[error("P1-f authority operation requires root")]
    RootRequired,
    #[error("P1-f authority path is outside the accepted boundary")]
    InvalidPath,
    #[error("P1-f authority filesystem custody is invalid")]
    InvalidCustody,
    #[error("P1-f authority document is invalid")]
    InvalidDocument,
    #[error("P1-f authority signature is invalid")]
    InvalidSignature,
    #[error("P1-f authority validity window is invalid")]
    InvalidValidityWindow,
    #[error("P1-f authority is not initialized")]
    NotInitialized,
    #[error("P1-f authority is not activated")]
    NotActivated,
    #[error("P1-f authority was already activated")]
    AlreadyActivated,
    #[error("P1-f authority is quarantined pending separately reviewed rebind")]
    Quarantined,
    #[error("P1-f authority generation conflicts with retained state")]
    GenerationConflict,
    #[error("P1-f authority history is inconsistent")]
    HistoryConflict,
    #[error("P1-f phase manifest was already consumed")]
    ManifestSpent,
    #[error("P1-f phase has another active owner")]
    ActiveConflict,
    #[error("P1-f authority deadline has expired")]
    DeadlineExpired,
    #[error("P1-f restore selection overlaps the trusted control root")]
    RestoreOverlapsControlRoot,
    #[error("P1-f guardian lock is already held")]
    ConcurrentGuardian,
    #[error("P1-f phase execution is already owned")]
    ConcurrentExecution,
    #[error("P1-f pending transaction requires exact recovery")]
    PendingRecoveryRequired,
    #[error("P1-f authority I/O failed: {0:?}")]
    Io(ErrorKind),
}

impl From<std::io::Error> for Stage8bP1fAuthorityErrorV1 {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value.kind())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Stage8bP1fPhaseV1 {
    O1Provision,
    O2MaterializeBootstrap,
    O3SyntheticPaper,
    O4FinamReadOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Stage8bP1fPhaseStateV1 {
    GenesisPrepared,
    GenesisActivated,
    Active,
    Stopping,
    Completed,
    Failed,
    Expired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stage8bP1fGenesisManifestV1 {
    pub schema_version: u16,
    pub domain: String,
    pub installation_id: String,
    pub target_host_id: String,
    pub target_host_ssh_ed25519_sha256: String,
    pub control_root: String,
    pub authority_generation: u64,
    pub ceremony_nonce_sha256: String,
    pub genesis_head_sha256: String,
    pub not_before_utc: String,
    pub expires_at_utc: String,
    pub issuer_key_id: String,
    pub signature_ed25519_hex: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stage8bP1fGenesisReceiptV1 {
    pub schema_version: u16,
    pub domain: String,
    pub installation_id: String,
    pub target_host_id: String,
    pub control_root: String,
    pub authority_generation: u64,
    pub ceremony_nonce_sha256: String,
    pub genesis_manifest_sha256: String,
    pub genesis_head_sha256: String,
    pub committed_at_utc: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stage8bP1fActivationCertificateV1 {
    pub schema_version: u16,
    pub domain: String,
    pub installation_id: String,
    pub target_host_id: String,
    pub control_root: String,
    pub authority_generation: u64,
    pub ceremony_nonce_sha256: String,
    pub genesis_manifest_sha256: String,
    pub genesis_receipt_sha256: String,
    pub genesis_head_sha256: String,
    pub activated_at_utc: String,
    pub expires_at_utc: String,
    pub issuer_key_id: String,
    pub signature_ed25519_hex: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stage8bP1fPhaseManifestV1 {
    pub schema_version: u16,
    pub domain: String,
    pub installation_id: String,
    pub target_host_id: String,
    pub target_host_ssh_ed25519_sha256: String,
    pub authority_generation: u64,
    pub authority_sequence: u64,
    pub predecessor_event_sha256: String,
    pub accepted_source_tree_sha256: String,
    pub phase: Stage8bP1fPhaseV1,
    pub materialization_policy_sha256: String,
    pub config_template_sha256: String,
    pub installation_sha256: String,
    pub controller_id: String,
    pub not_before_utc: String,
    pub deadline_utc: String,
    pub issuer_key_id: String,
    pub signature_ed25519_hex: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stage8bP1fClaimReceiptV1 {
    pub schema_version: u16,
    pub domain: String,
    pub manifest_sha256: String,
    pub authority_generation: u64,
    pub authority_sequence: u64,
    pub predecessor_event_sha256: String,
    pub phase: Stage8bP1fPhaseV1,
    pub controller_id: String,
    pub claimed_at_utc: String,
    pub deadline_utc: String,
    pub state: Stage8bP1fPhaseStateV1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stage8bP1fTerminalReceiptV1 {
    pub schema_version: u16,
    pub domain: String,
    pub manifest_sha256: String,
    pub authority_generation: u64,
    pub authority_sequence: u64,
    pub predecessor_event_sha256: String,
    pub terminal_state: Stage8bP1fPhaseStateV1,
    pub reason_code: String,
    pub recorded_at_utc: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stage8bP1fStoppingReceiptV1 {
    pub schema_version: u16,
    pub domain: String,
    pub manifest_sha256: String,
    pub authority_generation: u64,
    pub authority_sequence: u64,
    pub predecessor_event_sha256: String,
    pub reason_code: String,
    pub stopping_started_at_utc: String,
    pub force_kill_at_utc: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stage8bP1fMaterializedSetReceiptV1 {
    pub schema_version: u16,
    pub domain: String,
    pub manifest_sha256: String,
    pub authority_generation: u64,
    pub authority_sequence: u64,
    pub predecessor_event_sha256: String,
    pub materialization_policy_sha256: String,
    pub config_template_sha256: String,
    pub source_sha256: String,
    pub final_config_sha256: String,
    pub installation_sha256: String,
    pub claim_receipt_sha256: String,
    pub broker_truth_checked_at_utc: String,
    pub ready_at_utc: String,
    pub state: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stage8bP1fClaimDispositionV1 {
    Claimed(Stage8bP1fClaimReceiptV1),
    ContinuedExisting(Stage8bP1fClaimReceiptV1),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stage8bP1fAuthorityInspectionV1 {
    pub authority_generation: u64,
    pub latest_sequence: u64,
    pub latest_event_sha256: String,
    pub state: Stage8bP1fPhaseStateV1,
    pub active_manifest_sha256: Option<String>,
    pub deadline_utc: Option<String>,
    pub force_kill_at_utc: Option<String>,
}

/// Linear local admission to one already claimed phase.  It grants no Redis,
/// FINAM or broker capability and is deliberately neither Clone nor serde.
#[derive(Debug)]
pub struct Stage8bP1fRunPermitV1 {
    manifest_sha256: String,
    phase: Stage8bP1fPhaseV1,
    claimed_at: DateTime<Utc>,
    deadline: DateTime<Utc>,
    admitted_wall: DateTime<Utc>,
    admitted_monotonic: Instant,
    authority_root: PathBuf,
    expected_uid: u32,
    service_gid: u32,
    execution_lock: File,
    boot_id: String,
    stopping_started_at: Option<DateTime<Utc>>,
    force_kill_at: Option<DateTime<Utc>>,
    force_kill_after_elapsed: Option<StdDuration>,
    stopping_reason_code: Option<String>,
    last_trusted_wall: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage8bP1fDeadlineDecisionV1 {
    Continue,
    BeginStopping,
    ForceKill,
}

/// A local stop request which can only narrow an already admitted linear
/// permit. It cannot create, extend or reacquire phase authority.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage8bP1fOperatorStopCauseV1 {
    Sigterm,
    Sigint,
    SupervisionFailure,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Stage8bP1fLocalStopCauseV1 {
    Sigterm,
    Sigint,
    SupervisionFailure,
}

impl Stage8bP1fLocalStopCauseV1 {
    const fn reason_code(self) -> &'static str {
        match self {
            Self::Sigterm => "external-sigterm",
            Self::Sigint => "external-sigint",
            Self::SupervisionFailure => "supervision-failure",
        }
    }
}

impl Stage8bP1fRunPermitV1 {
    pub fn manifest_sha256(&self) -> &str {
        &self.manifest_sha256
    }

    pub const fn phase(&self) -> Stage8bP1fPhaseV1 {
        self.phase
    }

    pub fn deadline_utc(&self) -> String {
        canonical_timestamp(self.deadline)
    }

    /// Uses monotonic elapsed time to reject wall-clock rollback and enforces
    /// the exact 30-second ordered-stop grace independently of SSH.
    pub fn poll_deadline(
        &mut self,
        trusted_wall_now: DateTime<Utc>,
    ) -> Result<Stage8bP1fDeadlineDecisionV1, Stage8bP1fAuthorityErrorV1> {
        self.poll_deadline_at_elapsed(trusted_wall_now, self.admitted_monotonic.elapsed())
    }

    pub fn request_operator_stop(
        &mut self,
        cause: Stage8bP1fOperatorStopCauseV1,
        trusted_wall_now: DateTime<Utc>,
    ) -> Result<Stage8bP1fDeadlineDecisionV1, Stage8bP1fAuthorityErrorV1> {
        let cause = match cause {
            Stage8bP1fOperatorStopCauseV1::Sigterm => Stage8bP1fLocalStopCauseV1::Sigterm,
            Stage8bP1fOperatorStopCauseV1::Sigint => Stage8bP1fLocalStopCauseV1::Sigint,
            Stage8bP1fOperatorStopCauseV1::SupervisionFailure => {
                Stage8bP1fLocalStopCauseV1::SupervisionFailure
            }
        };
        self.request_local_stop(cause, trusted_wall_now)
    }

    pub fn stopping_reason_code(&self) -> Option<&str> {
        self.stopping_reason_code.as_deref()
    }

    pub(crate) fn request_local_stop(
        &mut self,
        cause: Stage8bP1fLocalStopCauseV1,
        trusted_wall_now: DateTime<Utc>,
    ) -> Result<Stage8bP1fDeadlineDecisionV1, Stage8bP1fAuthorityErrorV1> {
        let elapsed = self.admitted_monotonic.elapsed();
        if self.stopping_started_at.is_some() {
            return self.poll_deadline_at_elapsed(trusted_wall_now, elapsed);
        }

        let expected_wall = self.admitted_wall
            + chrono::Duration::from_std(elapsed).unwrap_or(chrono::Duration::MAX);
        let clock_untrusted = trusted_wall_now + chrono::Duration::seconds(1) < expected_wall
            || trusted_wall_now < self.claimed_at
            || trusted_wall_now < self.last_trusted_wall;
        if trusted_wall_now >= self.deadline || clock_untrusted {
            return self.poll_deadline_at_elapsed(trusted_wall_now, elapsed);
        }

        let force_kill_at = trusted_wall_now + chrono::Duration::seconds(30);
        let reason_code = cause.reason_code();
        let store = Stage8bP1fAuthorityStoreV1::open_at(
            &self.authority_root,
            self.expected_uid,
            self.service_gid,
        )?;
        let receipt = store.begin_stopping_phase(
            &self.manifest_sha256,
            reason_code,
            trusted_wall_now,
            force_kill_at,
            &self.boot_id,
        )?;
        self.stopping_started_at = Some(parse_timestamp(&receipt.stopping_started_at_utc)?);
        self.force_kill_at = Some(parse_timestamp(&receipt.force_kill_at_utc)?);
        self.force_kill_after_elapsed = elapsed.checked_add(StdDuration::from_secs(30));
        self.stopping_reason_code = Some(reason_code.to_string());
        self.last_trusted_wall = trusted_wall_now;
        Ok(Stage8bP1fDeadlineDecisionV1::BeginStopping)
    }

    #[cfg(any(test, feature = "stage8b-p1-test-fixtures"))]
    pub(crate) fn shorten_stop_grace_for_test(&mut self, grace: StdDuration) {
        self.force_kill_after_elapsed = Some(self.admitted_monotonic.elapsed() + grace);
    }

    fn poll_deadline_at_elapsed(
        &mut self,
        trusted_wall_now: DateTime<Utc>,
        elapsed: StdDuration,
    ) -> Result<Stage8bP1fDeadlineDecisionV1, Stage8bP1fAuthorityErrorV1> {
        let _ = self.execution_lock.metadata()?;
        if let (Some(started), Some(force_kill_at)) = (self.stopping_started_at, self.force_kill_at)
        {
            let monotonic_expired = match self.force_kill_after_elapsed {
                Some(limit) => elapsed >= limit,
                None => true,
            };
            let wall_untrusted = trusted_wall_now < started
                || trusted_wall_now < self.last_trusted_wall
                || trusted_wall_now >= force_kill_at;
            if trusted_wall_now > self.last_trusted_wall {
                self.last_trusted_wall = trusted_wall_now;
            }
            return Ok(if monotonic_expired || wall_untrusted {
                Stage8bP1fDeadlineDecisionV1::ForceKill
            } else {
                Stage8bP1fDeadlineDecisionV1::BeginStopping
            });
        }
        let expected_wall = self.admitted_wall
            + chrono::Duration::from_std(elapsed).unwrap_or(chrono::Duration::MAX);
        let clock_untrusted = trusted_wall_now + chrono::Duration::seconds(1) < expected_wall
            || trusted_wall_now < self.claimed_at
            || trusted_wall_now < self.last_trusted_wall;
        if trusted_wall_now < self.deadline && !clock_untrusted {
            self.last_trusted_wall = trusted_wall_now;
            return Ok(Stage8bP1fDeadlineDecisionV1::Continue);
        }
        let stopping_started_at = if clock_untrusted {
            expected_wall
        } else {
            trusted_wall_now
        };
        let force_kill_at = stopping_started_at + chrono::Duration::seconds(30);
        let store = Stage8bP1fAuthorityStoreV1::open_at(
            &self.authority_root,
            self.expected_uid,
            self.service_gid,
        )?;
        let receipt = store.begin_stopping_phase(
            &self.manifest_sha256,
            if clock_untrusted {
                "clock-untrusted"
            } else {
                "deadline-reached"
            },
            stopping_started_at,
            force_kill_at,
            &self.boot_id,
        )?;
        self.stopping_started_at = Some(parse_timestamp(&receipt.stopping_started_at_utc)?);
        self.force_kill_at = Some(parse_timestamp(&receipt.force_kill_at_utc)?);
        self.force_kill_after_elapsed = elapsed.checked_add(StdDuration::from_secs(30));
        self.stopping_reason_code = Some(receipt.reason_code);
        self.last_trusted_wall = trusted_wall_now;
        Ok(Stage8bP1fDeadlineDecisionV1::BeginStopping)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stage8bP1fRestorePlanV1 {
    pub source_roots: Vec<PathBuf>,
    pub target_roots: Vec<PathBuf>,
    pub selected_paths: Vec<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stage8bP1fRestoreWriteV1 {
    pub target_root_index: usize,
    pub relative_path: PathBuf,
    pub bytes: Vec<u8>,
    pub mode: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stage8bP1fQuarantineReceiptV1 {
    pub schema_version: u16,
    pub domain: String,
    pub reason_code: String,
    pub declared_at_utc: String,
    pub rebind_authorized: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct GenesisTransactionV1 {
    schema_version: u16,
    domain: String,
    genesis_manifest_sha256: String,
    authority_generation: u64,
    ceremony_nonce_sha256: String,
    committed_at_utc: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PendingClaimV1 {
    schema_version: u16,
    domain: String,
    manifest_sha256: String,
    authority_sequence: u64,
    predecessor_event_sha256: String,
    claimed_at_utc: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PendingTerminalV1 {
    schema_version: u16,
    domain: String,
    manifest_sha256: String,
    authority_generation: u64,
    authority_sequence: u64,
    predecessor_event_sha256: String,
    terminal_state: Stage8bP1fPhaseStateV1,
    reason_code: String,
    recorded_at_utc: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PendingStoppingV1 {
    schema_version: u16,
    domain: String,
    manifest_sha256: String,
    authority_generation: u64,
    authority_sequence: u64,
    predecessor_event_sha256: String,
    reason_code: String,
    stopping_started_at_utc: String,
    force_kill_at_utc: String,
    boot_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PendingMaterializationV1 {
    schema_version: u16,
    domain: String,
    manifest_sha256: String,
    authority_generation: u64,
    authority_sequence: u64,
    predecessor_event_sha256: String,
    materialization_policy_sha256: String,
    config_template_sha256: String,
    source_sha256: String,
    final_config_sha256: String,
    installation_sha256: String,
    claim_receipt_sha256: String,
    broker_truth_checked_at_utc: String,
    ready_at_utc: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExecutionOwnerV1 {
    schema_version: u16,
    domain: String,
    manifest_sha256: String,
    boot_id: String,
    admitted_at_utc: String,
    phase_deadline_utc: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct AuthorityEventV1 {
    schema_version: u16,
    domain: String,
    authority_generation: u64,
    authority_sequence: u64,
    predecessor_event_sha256: String,
    event_kind: String,
    state: Stage8bP1fPhaseStateV1,
    manifest_sha256: String,
    receipt_sha256: String,
    recorded_at_utc: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct HistoryHeadV1 {
    schema_version: u16,
    domain: String,
    authority_generation: u64,
    latest_sequence: u64,
    latest_event_sha256: String,
    state: Stage8bP1fPhaseStateV1,
    active_manifest_sha256: Option<String>,
    deadline_utc: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    force_kill_at_utc: Option<String>,
}

/// Root guardian bound to one retained control-root inode and exact custody.
pub struct Stage8bP1fAuthorityStoreV1 {
    root: PathBuf,
    root_file: File,
    root_dev: u64,
    root_ino: u64,
    expected_uid: u32,
    service_gid: u32,
}

struct GuardianLease<'a> {
    store: &'a Stage8bP1fAuthorityStoreV1,
    lock: File,
}

impl Stage8bP1fAuthorityStoreV1 {
    /// Opens only the accepted production control root and only as root.
    pub fn open_production(service_gid: u32) -> Result<Self, Stage8bP1fAuthorityErrorV1> {
        if unsafe { libc::geteuid() } != 0 {
            return Err(Stage8bP1fAuthorityErrorV1::RootRequired);
        }
        Self::open_at(
            Path::new(STAGE8B_P1F_AUTHORITY_CONTROL_ROOT),
            0,
            service_gid,
        )
    }

    fn open_at(
        root: &Path,
        expected_uid: u32,
        service_gid: u32,
    ) -> Result<Self, Stage8bP1fAuthorityErrorV1> {
        if !root.is_absolute() {
            return Err(Stage8bP1fAuthorityErrorV1::InvalidPath);
        }
        let metadata = fs::symlink_metadata(root)
            .map_err(|error| Stage8bP1fAuthorityErrorV1::Io(error.kind()))?;
        validate_directory_metadata(&metadata, expected_uid, service_gid, 0o750)?;
        let canonical = fs::canonicalize(root)?;
        if canonical != root {
            return Err(Stage8bP1fAuthorityErrorV1::InvalidPath);
        }
        let root_file = open_directory(root)?;
        let opened = root_file.metadata()?;
        validate_directory_metadata(&opened, expected_uid, service_gid, 0o750)?;
        if opened.dev() != metadata.dev() || opened.ino() != metadata.ino() {
            return Err(Stage8bP1fAuthorityErrorV1::InvalidCustody);
        }
        Ok(Self {
            root: canonical,
            root_file,
            root_dev: opened.dev(),
            root_ino: opened.ino(),
            expected_uid,
            service_gid,
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn initialize_authority(
        &self,
        manifest_bytes: &[u8],
        expected_public_key_hex: &str,
        expected_key_id: &str,
        trusted_now: DateTime<Utc>,
    ) -> Result<Stage8bP1fGenesisReceiptV1, Stage8bP1fAuthorityErrorV1> {
        let _lease = self.acquire_lease()?;
        self.reject_quarantine()?;
        let manifest: Stage8bP1fGenesisManifestV1 = read_signed_document(
            manifest_bytes,
            SIGNED_GENESIS_DOMAIN,
            expected_public_key_hex,
            expected_key_id,
            |value: &Stage8bP1fGenesisManifestV1| &value.issuer_key_id,
            |value: &Stage8bP1fGenesisManifestV1| &value.signature_ed25519_hex,
            |value: &Stage8bP1fGenesisManifestV1| {
                let mut unsigned = value.clone();
                unsigned.signature_ed25519_hex.clear();
                unsigned
            },
        )?;
        self.validate_genesis_manifest(&manifest, trusted_now)?;
        let manifest_sha256 = sha256_hex(manifest_bytes);
        let authority = self.root.join(AUTHORITY_DIRECTORY);
        let mut transaction = GenesisTransactionV1 {
            schema_version: STAGE8B_P1F_AUTHORITY_SCHEMA_VERSION,
            domain: "stage8b-p1f-genesis-transaction-v1".to_string(),
            genesis_manifest_sha256: manifest_sha256.clone(),
            authority_generation: manifest.authority_generation,
            ceremony_nonce_sha256: manifest.ceremony_nonce_sha256.clone(),
            committed_at_utc: canonical_timestamp(trusted_now),
        };

        if authority.exists() && !authority.join(GENESIS_TRANSACTION_FILE).exists() {
            self.validate_directory(&authority)?;
            self.validate_directory(&authority.join(EVENTS_DIRECTORY))?;
            self.validate_directory(&authority.join(MANIFESTS_DIRECTORY))?;
            let prepared_transaction =
                authority.join(format!(".{GENESIS_TRANSACTION_FILE}.p1f-create"));
            let names = fs::read_dir(&authority)?
                .map(|entry| {
                    entry
                        .map_err(Stage8bP1fAuthorityErrorV1::from)
                        .and_then(|entry| {
                            entry
                                .file_name()
                                .into_string()
                                .map_err(|_| Stage8bP1fAuthorityErrorV1::HistoryConflict)
                        })
                })
                .collect::<Result<BTreeSet<_>, _>>()?;
            let expected = BTreeSet::from([
                EVENTS_DIRECTORY.to_string(),
                MANIFESTS_DIRECTORY.to_string(),
            ]);
            let mut expected_with_prepared_temp = expected.clone();
            expected_with_prepared_temp.insert(format!(".{GENESIS_TRANSACTION_FILE}.p1f-create"));
            if (names != expected && names != expected_with_prepared_temp)
                || fs::read_dir(authority.join(EVENTS_DIRECTORY))?
                    .next()
                    .is_some()
                || fs::read_dir(authority.join(MANIFESTS_DIRECTORY))?
                    .next()
                    .is_some()
            {
                return Err(Stage8bP1fAuthorityErrorV1::HistoryConflict);
            }
            if prepared_transaction.exists() {
                let retained: GenesisTransactionV1 =
                    self.read_authority_file(&prepared_transaction, 0o440)?;
                if retained.schema_version != transaction.schema_version
                    || retained.domain != transaction.domain
                    || retained.genesis_manifest_sha256 != transaction.genesis_manifest_sha256
                    || retained.authority_generation != transaction.authority_generation
                    || retained.ceremony_nonce_sha256 != transaction.ceremony_nonce_sha256
                    || parse_timestamp(&retained.committed_at_utc).is_err()
                {
                    return Err(Stage8bP1fAuthorityErrorV1::GenerationConflict);
                }
                transaction = retained;
            }
            self.write_create_new(
                &authority.join(GENESIS_TRANSACTION_FILE),
                &canonical_json(&transaction)?,
                0o440,
            )?;
        }

        if authority.exists() {
            let retained: GenesisTransactionV1 =
                self.read_authority_file(&authority.join(GENESIS_TRANSACTION_FILE), 0o440)?;
            if retained.schema_version != transaction.schema_version
                || retained.domain != transaction.domain
                || retained.genesis_manifest_sha256 != transaction.genesis_manifest_sha256
                || retained.authority_generation != transaction.authority_generation
                || retained.ceremony_nonce_sha256 != transaction.ceremony_nonce_sha256
                || parse_timestamp(&retained.committed_at_utc).is_err()
            {
                return Err(Stage8bP1fAuthorityErrorV1::GenerationConflict);
            }
            transaction = retained;
            if authority.join(ACTIVATION_CERTIFICATE_FILE).exists() {
                return Err(Stage8bP1fAuthorityErrorV1::AlreadyActivated);
            }
        } else {
            self.create_authority_directory(&authority, 0o750)?;
            self.create_authority_directory(&authority.join(EVENTS_DIRECTORY), 0o750)?;
            self.create_authority_directory(&authority.join(MANIFESTS_DIRECTORY), 0o750)?;
            sync_directory(&authority)?;
            sync_directory(&self.root)?;
            inject_test_fault(1)?;
            self.write_create_new(
                &authority.join(GENESIS_TRANSACTION_FILE),
                &canonical_json(&transaction)?,
                0o440,
            )?;
        }

        self.write_or_require_exact(
            &authority.join(GENESIS_MANIFEST_FILE),
            manifest_bytes,
            0o440,
        )?;
        let committed_at = transaction.committed_at_utc;
        let receipt = Stage8bP1fGenesisReceiptV1 {
            schema_version: STAGE8B_P1F_AUTHORITY_SCHEMA_VERSION,
            domain: "stage8b-p1f-genesis-receipt-v1".to_string(),
            installation_id: manifest.installation_id.clone(),
            target_host_id: manifest.target_host_id.clone(),
            control_root: manifest.control_root.clone(),
            authority_generation: manifest.authority_generation,
            ceremony_nonce_sha256: manifest.ceremony_nonce_sha256.clone(),
            genesis_manifest_sha256: manifest_sha256.clone(),
            genesis_head_sha256: manifest.genesis_head_sha256.clone(),
            committed_at_utc: committed_at.clone(),
        };
        let receipt_bytes = canonical_json(&receipt)?;
        self.write_or_require_exact(&authority.join(GENESIS_RECEIPT_FILE), &receipt_bytes, 0o440)?;
        let event = AuthorityEventV1 {
            schema_version: STAGE8B_P1F_AUTHORITY_SCHEMA_VERSION,
            domain: "stage8b-p1f-authority-event-v1".to_string(),
            authority_generation: manifest.authority_generation,
            authority_sequence: 0,
            predecessor_event_sha256: ZERO_SHA256.to_string(),
            event_kind: "GENESIS_PREPARED".to_string(),
            state: Stage8bP1fPhaseStateV1::GenesisPrepared,
            manifest_sha256,
            receipt_sha256: sha256_hex(&receipt_bytes),
            recorded_at_utc: committed_at,
        };
        let event_bytes = canonical_json(&event)?;
        let event_sha256 = event_digest(&event_bytes);
        self.write_or_require_exact(&event_path(&authority, 0), &event_bytes, 0o440)?;
        let head = HistoryHeadV1 {
            schema_version: STAGE8B_P1F_AUTHORITY_SCHEMA_VERSION,
            domain: "stage8b-p1f-history-head-v1".to_string(),
            authority_generation: manifest.authority_generation,
            latest_sequence: 0,
            latest_event_sha256: event_sha256,
            state: Stage8bP1fPhaseStateV1::GenesisPrepared,
            active_manifest_sha256: None,
            deadline_utc: None,
            force_kill_at_utc: None,
        };
        self.write_or_require_exact(
            &authority.join(HISTORY_HEAD_FILE),
            &canonical_json(&head)?,
            0o440,
        )?;
        self.validate_history(false)?;
        Ok(receipt)
    }

    pub fn activate_authority(
        &self,
        certificate_bytes: &[u8],
        expected_public_key_hex: &str,
        expected_key_id: &str,
        trusted_now: DateTime<Utc>,
    ) -> Result<(), Stage8bP1fAuthorityErrorV1> {
        let _lease = self.acquire_lease()?;
        self.reject_quarantine()?;
        let authority = self.root.join(AUTHORITY_DIRECTORY);
        let certificate_path = authority.join(ACTIVATION_CERTIFICATE_FILE);
        let certificate: Stage8bP1fActivationCertificateV1 = read_signed_document(
            certificate_bytes,
            SIGNED_ACTIVATION_DOMAIN,
            expected_public_key_hex,
            expected_key_id,
            |value: &Stage8bP1fActivationCertificateV1| &value.issuer_key_id,
            |value: &Stage8bP1fActivationCertificateV1| &value.signature_ed25519_hex,
            |value: &Stage8bP1fActivationCertificateV1| {
                let mut unsigned = value.clone();
                unsigned.signature_ed25519_hex.clear();
                unsigned
            },
        )?;
        validate_window(
            &certificate.activated_at_utc,
            &certificate.expires_at_utc,
            trusted_now,
        )?;
        let manifest: Stage8bP1fGenesisManifestV1 =
            self.read_authority_file(&authority.join(GENESIS_MANIFEST_FILE), 0o440)?;
        let receipt: Stage8bP1fGenesisReceiptV1 =
            self.read_authority_file(&authority.join(GENESIS_RECEIPT_FILE), 0o440)?;
        let receipt_bytes = canonical_json(&receipt)?;
        if certificate.installation_id != manifest.installation_id
            || certificate.target_host_id != manifest.target_host_id
            || certificate.control_root != manifest.control_root
            || certificate.authority_generation != manifest.authority_generation
            || certificate.ceremony_nonce_sha256 != manifest.ceremony_nonce_sha256
            || certificate.genesis_manifest_sha256 != sha256_hex(&canonical_json(&manifest)?)
            || certificate.genesis_receipt_sha256 != sha256_hex(&receipt_bytes)
            || certificate.genesis_head_sha256 != manifest.genesis_head_sha256
        {
            return Err(Stage8bP1fAuthorityErrorV1::InvalidDocument);
        }
        let mut head: HistoryHeadV1 =
            self.read_authority_file(&authority.join(HISTORY_HEAD_FILE), 0o440)?;
        if certificate_path.exists() {
            if self.read_authority_bytes(&certificate_path, 0o440)? != certificate_bytes {
                return Err(Stage8bP1fAuthorityErrorV1::GenerationConflict);
            }
            if head.state == Stage8bP1fPhaseStateV1::GenesisPrepared {
                head.state = Stage8bP1fPhaseStateV1::GenesisActivated;
                self.replace_exact(
                    &authority.join(HISTORY_HEAD_FILE),
                    &canonical_json(&head)?,
                    0o440,
                )?;
            } else if head.state != Stage8bP1fPhaseStateV1::GenesisActivated {
                return Err(Stage8bP1fAuthorityErrorV1::HistoryConflict);
            }
            self.validate_history(true)?;
            return Ok(());
        }
        self.validate_history(false)?;
        if head.state == Stage8bP1fPhaseStateV1::GenesisPrepared {
            head.state = Stage8bP1fPhaseStateV1::GenesisActivated;
            self.replace_exact(
                &authority.join(HISTORY_HEAD_FILE),
                &canonical_json(&head)?,
                0o440,
            )?;
        } else if head.state != Stage8bP1fPhaseStateV1::GenesisActivated {
            return Err(Stage8bP1fAuthorityErrorV1::HistoryConflict);
        }
        self.write_create_new(&certificate_path, certificate_bytes, 0o440)?;
        self.validate_history(true)?;
        Ok(())
    }

    pub fn claim_phase(
        &self,
        manifest_bytes: &[u8],
        expected_public_key_hex: &str,
        expected_key_id: &str,
        trusted_now: DateTime<Utc>,
    ) -> Result<Stage8bP1fClaimDispositionV1, Stage8bP1fAuthorityErrorV1> {
        let _lease = self.acquire_lease()?;
        self.reject_quarantine()?;
        let manifest: Stage8bP1fPhaseManifestV1 = read_signed_document(
            manifest_bytes,
            SIGNED_PHASE_DOMAIN,
            expected_public_key_hex,
            expected_key_id,
            |value: &Stage8bP1fPhaseManifestV1| &value.issuer_key_id,
            |value: &Stage8bP1fPhaseManifestV1| &value.signature_ed25519_hex,
            |value: &Stage8bP1fPhaseManifestV1| {
                let mut unsigned = value.clone();
                unsigned.signature_ed25519_hex.clear();
                unsigned
            },
        )?;
        self.validate_phase_manifest(&manifest, trusted_now)?;
        let manifest_sha256 = sha256_hex(manifest_bytes);
        let authority = self.root.join(AUTHORITY_DIRECTORY);
        let pending_path = authority.join(PENDING_CLAIM_FILE);
        if pending_path.exists() {
            let pending: PendingClaimV1 = self.read_authority_file(&pending_path, 0o440)?;
            if pending.schema_version != STAGE8B_P1F_AUTHORITY_SCHEMA_VERSION
                || pending.domain != "stage8b-p1f-pending-claim-v1"
                || parse_timestamp(&pending.claimed_at_utc).is_err()
                || pending.manifest_sha256 != manifest_sha256
                || pending.authority_sequence != manifest.authority_sequence
                || pending.predecessor_event_sha256 != manifest.predecessor_event_sha256
            {
                return Err(Stage8bP1fAuthorityErrorV1::ActiveConflict);
            }
            let receipt = self.continue_claim_transaction(
                &manifest,
                manifest_bytes,
                &manifest_sha256,
                &pending,
            )?;
            validate_not_expired(&receipt.deadline_utc, trusted_now)?;
            return Ok(Stage8bP1fClaimDispositionV1::ContinuedExisting(receipt));
        }
        let inspection = self.validate_history(true)?;
        if inspection.state == Stage8bP1fPhaseStateV1::Active {
            if inspection.active_manifest_sha256.as_deref() != Some(&manifest_sha256) {
                return Err(Stage8bP1fAuthorityErrorV1::ActiveConflict);
            }
            let receipt = self.read_claim_receipt(&manifest_sha256)?;
            validate_not_expired(&receipt.deadline_utc, trusted_now)?;
            return Ok(Stage8bP1fClaimDispositionV1::ContinuedExisting(receipt));
        }
        if self
            .root
            .join(AUTHORITY_DIRECTORY)
            .join(MANIFESTS_DIRECTORY)
            .join(&manifest_sha256)
            .exists()
        {
            return Err(Stage8bP1fAuthorityErrorV1::ManifestSpent);
        }
        if manifest.authority_generation != inspection.authority_generation
            || manifest.authority_sequence != inspection.latest_sequence + 1
            || manifest.predecessor_event_sha256 != inspection.latest_event_sha256
        {
            return Err(Stage8bP1fAuthorityErrorV1::HistoryConflict);
        }
        let pending = PendingClaimV1 {
            schema_version: STAGE8B_P1F_AUTHORITY_SCHEMA_VERSION,
            domain: "stage8b-p1f-pending-claim-v1".to_string(),
            manifest_sha256: manifest_sha256.clone(),
            authority_sequence: manifest.authority_sequence,
            predecessor_event_sha256: manifest.predecessor_event_sha256.clone(),
            claimed_at_utc: canonical_timestamp(trusted_now),
        };
        self.write_create_new(
            &authority.join(PENDING_CLAIM_FILE),
            &canonical_json(&pending)?,
            0o440,
        )?;
        let receipt =
            self.continue_claim_transaction(&manifest, manifest_bytes, &manifest_sha256, &pending)?;
        Ok(Stage8bP1fClaimDispositionV1::Claimed(receipt))
    }

    fn continue_claim_transaction(
        &self,
        manifest: &Stage8bP1fPhaseManifestV1,
        manifest_bytes: &[u8],
        manifest_sha256: &str,
        pending: &PendingClaimV1,
    ) -> Result<Stage8bP1fClaimReceiptV1, Stage8bP1fAuthorityErrorV1> {
        let authority = self.root.join(AUTHORITY_DIRECTORY);
        let head: HistoryHeadV1 =
            self.read_authority_file(&authority.join(HISTORY_HEAD_FILE), 0o440)?;
        let manifest_directory = authority.join(MANIFESTS_DIRECTORY).join(manifest_sha256);
        if head.latest_sequence == manifest.authority_sequence
            && head.latest_event_sha256
                == event_digest(&self.read_authority_bytes(
                    &event_path(&authority, manifest.authority_sequence),
                    0o440,
                )?)
            && head.state == Stage8bP1fPhaseStateV1::Active
            && head.active_manifest_sha256.as_deref() == Some(manifest_sha256)
        {
            let receipt = self.read_claim_receipt(manifest_sha256)?;
            fs::remove_file(authority.join(PENDING_CLAIM_FILE))?;
            sync_directory(&authority)?;
            self.validate_history(true)?;
            return Ok(receipt);
        }
        if head.latest_sequence + 1 != pending.authority_sequence
            || head.latest_event_sha256 != pending.predecessor_event_sha256
        {
            return Err(Stage8bP1fAuthorityErrorV1::HistoryConflict);
        }
        self.validate_history_with_pending(true, Some(pending), None)?;
        if manifest_directory.exists() {
            self.validate_directory(&manifest_directory)?;
        } else {
            self.create_authority_directory(&manifest_directory, 0o750)?;
        }
        self.write_or_require_exact(
            &manifest_directory.join("phase-manifest.json"),
            manifest_bytes,
            0o440,
        )?;
        let receipt = Stage8bP1fClaimReceiptV1 {
            schema_version: STAGE8B_P1F_AUTHORITY_SCHEMA_VERSION,
            domain: "stage8b-p1f-claim-receipt-v1".to_string(),
            manifest_sha256: manifest_sha256.to_string(),
            authority_generation: manifest.authority_generation,
            authority_sequence: manifest.authority_sequence,
            predecessor_event_sha256: manifest.predecessor_event_sha256.clone(),
            phase: manifest.phase,
            controller_id: manifest.controller_id.clone(),
            claimed_at_utc: pending.claimed_at_utc.clone(),
            deadline_utc: manifest.deadline_utc.clone(),
            state: Stage8bP1fPhaseStateV1::Active,
        };
        let receipt_bytes = canonical_json(&receipt)?;
        self.write_or_require_exact(
            &manifest_directory.join("claim-receipt.json"),
            &receipt_bytes,
            0o440,
        )?;
        sync_directory(&manifest_directory)?;
        let event = AuthorityEventV1 {
            schema_version: STAGE8B_P1F_AUTHORITY_SCHEMA_VERSION,
            domain: "stage8b-p1f-authority-event-v1".to_string(),
            authority_generation: manifest.authority_generation,
            authority_sequence: manifest.authority_sequence,
            predecessor_event_sha256: manifest.predecessor_event_sha256.clone(),
            event_kind: "PHASE_CLAIMED".to_string(),
            state: Stage8bP1fPhaseStateV1::Active,
            manifest_sha256: manifest_sha256.to_string(),
            receipt_sha256: sha256_hex(&receipt_bytes),
            recorded_at_utc: receipt.claimed_at_utc.clone(),
        };
        let event_bytes = canonical_json(&event)?;
        let event_sha256 = event_digest(&event_bytes);
        self.write_or_require_exact(
            &event_path(&authority, manifest.authority_sequence),
            &event_bytes,
            0o440,
        )?;
        let head = HistoryHeadV1 {
            schema_version: STAGE8B_P1F_AUTHORITY_SCHEMA_VERSION,
            domain: "stage8b-p1f-history-head-v1".to_string(),
            authority_generation: manifest.authority_generation,
            latest_sequence: manifest.authority_sequence,
            latest_event_sha256: event_sha256,
            state: Stage8bP1fPhaseStateV1::Active,
            active_manifest_sha256: Some(manifest_sha256.to_string()),
            deadline_utc: Some(receipt.deadline_utc.clone()),
            force_kill_at_utc: None,
        };
        self.replace_exact(
            &authority.join(HISTORY_HEAD_FILE),
            &canonical_json(&head)?,
            0o440,
        )?;
        fs::remove_file(authority.join(PENDING_CLAIM_FILE))?;
        sync_directory(&authority)?;
        self.validate_history(true)?;
        Ok(receipt)
    }

    pub fn inspect(&self) -> Result<Stage8bP1fAuthorityInspectionV1, Stage8bP1fAuthorityErrorV1> {
        let _lease = self.acquire_lease()?;
        self.validate_history(true)
    }

    pub fn admit_active_phase(
        &self,
        manifest_sha256: &str,
        trusted_now: DateTime<Utc>,
    ) -> Result<Stage8bP1fRunPermitV1, Stage8bP1fAuthorityErrorV1> {
        self.admit_active_phase_at(
            manifest_sha256,
            trusted_now,
            Some(Path::new(STAGE8B_P1F_CONFIG_ROOT)),
        )
    }

    fn admit_active_phase_at(
        &self,
        manifest_sha256: &str,
        trusted_now: DateTime<Utc>,
        config_root: Option<&Path>,
    ) -> Result<Stage8bP1fRunPermitV1, Stage8bP1fAuthorityErrorV1> {
        let lease = self.acquire_lease()?;
        self.reject_quarantine()?;
        let inspection = self.validate_history(true)?;
        if inspection.state != Stage8bP1fPhaseStateV1::Active
            || inspection.active_manifest_sha256.as_deref() != Some(manifest_sha256)
        {
            return Err(Stage8bP1fAuthorityErrorV1::ActiveConflict);
        }
        let execution_lock = self.acquire_execution_lock()?;
        let manifest_path = self
            .root
            .join(AUTHORITY_DIRECTORY)
            .join(MANIFESTS_DIRECTORY)
            .join(manifest_sha256)
            .join("phase-manifest.json");
        let manifest_bytes = self.read_authority_bytes(&manifest_path, 0o440)?;
        if sha256_hex(&manifest_bytes) != manifest_sha256 {
            return Err(Stage8bP1fAuthorityErrorV1::HistoryConflict);
        }
        let manifest: Stage8bP1fPhaseManifestV1 = parse_canonical(&manifest_bytes)?;
        let claim = self.read_claim_receipt(manifest_sha256)?;
        let claimed_at = parse_timestamp(&claim.claimed_at_utc)?;
        let deadline = parse_timestamp(&claim.deadline_utc)?;
        if trusted_now < claimed_at || trusted_now >= deadline {
            return Err(Stage8bP1fAuthorityErrorV1::DeadlineExpired);
        }
        match manifest.phase {
            Stage8bP1fPhaseV1::O1Provision => {
                return Err(Stage8bP1fAuthorityErrorV1::InvalidDocument)
            }
            Stage8bP1fPhaseV1::O2MaterializeBootstrap => {
                let receipt: Stage8bP1fMaterializedSetReceiptV1 = self.read_authority_file(
                    &manifest_path
                        .parent()
                        .ok_or(Stage8bP1fAuthorityErrorV1::InvalidPath)?
                        .join(MATERIALIZED_SET_RECEIPT_FILE),
                    0o440,
                )?;
                let age = trusted_now
                    .signed_duration_since(parse_timestamp(&receipt.broker_truth_checked_at_utc)?)
                    .num_seconds();
                if receipt.state != "ReadyForBootstrap"
                    || receipt.manifest_sha256 != manifest_sha256
                {
                    return Err(Stage8bP1fAuthorityErrorV1::InvalidValidityWindow);
                }
                if !(0..=300).contains(&age) {
                    drop(lease);
                    drop(execution_lock);
                    self.finish_phase(
                        manifest_sha256,
                        Stage8bP1fPhaseStateV1::Failed,
                        "o2-broker-truth-stale",
                        trusted_now,
                    )?;
                    return Err(Stage8bP1fAuthorityErrorV1::InvalidValidityWindow);
                }
                let authority = self.root.join(AUTHORITY_DIRECTORY);
                let event_bytes = self.read_authority_bytes(
                    &event_path(&authority, receipt.authority_sequence),
                    0o440,
                )?;
                let event: AuthorityEventV1 = parse_canonical(&event_bytes)?;
                if event.event_kind != "O2_MATERIALIZED"
                    || event.manifest_sha256 != manifest_sha256
                    || event.receipt_sha256 != sha256_hex(&canonical_json(&receipt)?)
                    || event.state != Stage8bP1fPhaseStateV1::Active
                    || inspection.latest_sequence < receipt.authority_sequence
                {
                    return Err(Stage8bP1fAuthorityErrorV1::HistoryConflict);
                }
                let config_root = config_root.ok_or(Stage8bP1fAuthorityErrorV1::InvalidPath)?;
                if sha256_hex(
                    &self.read_external_bytes(
                        &config_root
                            .join("bootstrap")
                            .join("stage8b-p1-first-boot-source-v1.json"),
                        crate::STAGE8B_P1E_FIRST_BOOT_SOURCE_MAX_BYTES,
                    )?,
                ) != receipt.source_sha256
                    || sha256_hex(&self.read_external_bytes(
                        &config_root.join("supervisor.json"),
                        STAGE8B_P1F_MAX_AUTHORITY_BYTES,
                    )?) != receipt.final_config_sha256
                {
                    return Err(Stage8bP1fAuthorityErrorV1::HistoryConflict);
                }
            }
            Stage8bP1fPhaseV1::O3SyntheticPaper | Stage8bP1fPhaseV1::O4FinamReadOnly => {}
        }
        let boot_id = current_boot_id()?;
        let manifest_directory = manifest_path
            .parent()
            .ok_or(Stage8bP1fAuthorityErrorV1::InvalidPath)?;
        let owner_path = manifest_directory.join(EXECUTION_OWNER_FILE);
        if owner_path.exists() {
            let retained: ExecutionOwnerV1 = self.read_authority_file(&owner_path, 0o440)?;
            if retained.schema_version != STAGE8B_P1F_AUTHORITY_SCHEMA_VERSION
                || retained.domain != "stage8b-p1f-execution-owner-v1"
                || retained.manifest_sha256 != manifest_sha256
                || retained.phase_deadline_utc != claim.deadline_utc
                || parse_timestamp(&retained.admitted_at_utc).is_err()
                || !canonical_token(&retained.boot_id)
            {
                return Err(Stage8bP1fAuthorityErrorV1::HistoryConflict);
            }
            // The old process no longer owns the execution flock, but no
            // trustworthy monotonic witness survives that process boundary.
            // Fail closed with an already elapsed grace instead of issuing a
            // fresh Active permit or extending shutdown after restart.
            drop(lease);
            drop(execution_lock);
            self.begin_stopping_phase(
                manifest_sha256,
                if retained.boot_id == boot_id {
                    "execution-owner-recovered"
                } else {
                    "boot-identity-changed"
                },
                trusted_now - chrono::Duration::seconds(30),
                trusted_now,
                &boot_id,
            )?;
            return Err(Stage8bP1fAuthorityErrorV1::DeadlineExpired);
        }
        let owner = ExecutionOwnerV1 {
            schema_version: STAGE8B_P1F_AUTHORITY_SCHEMA_VERSION,
            domain: "stage8b-p1f-execution-owner-v1".to_string(),
            manifest_sha256: manifest_sha256.to_string(),
            boot_id: boot_id.clone(),
            admitted_at_utc: canonical_timestamp(trusted_now),
            phase_deadline_utc: claim.deadline_utc.clone(),
        };
        self.write_create_new(&owner_path, &canonical_json(&owner)?, 0o440)?;
        Ok(Stage8bP1fRunPermitV1 {
            manifest_sha256: manifest_sha256.to_string(),
            phase: manifest.phase,
            claimed_at,
            deadline,
            admitted_wall: parse_timestamp(&owner.admitted_at_utc)?,
            admitted_monotonic: Instant::now(),
            authority_root: self.root.clone(),
            expected_uid: self.expected_uid,
            service_gid: self.service_gid,
            execution_lock,
            boot_id,
            stopping_started_at: None,
            force_kill_at: None,
            force_kill_after_elapsed: None,
            stopping_reason_code: None,
            last_trusted_wall: trusted_now,
        })
    }

    /// Permanently closes ordinary use after a declared or suspected
    /// whole-host/control-root restore.  This module intentionally exposes no
    /// clear/rebind operation; a new generation requires separate review.
    pub fn declare_restore_incident(
        &self,
        reason_code: &str,
        trusted_now: DateTime<Utc>,
    ) -> Result<Stage8bP1fQuarantineReceiptV1, Stage8bP1fAuthorityErrorV1> {
        let _lease = self.acquire_lease()?;
        if !canonical_token(reason_code) {
            return Err(Stage8bP1fAuthorityErrorV1::InvalidDocument);
        }
        let receipt = Stage8bP1fQuarantineReceiptV1 {
            schema_version: STAGE8B_P1F_AUTHORITY_SCHEMA_VERSION,
            domain: "stage8b-p1f-restore-quarantine-v1".to_string(),
            reason_code: reason_code.to_string(),
            declared_at_utc: canonical_timestamp(trusted_now),
            rebind_authorized: false,
        };
        let path = self.root.join(QUARANTINE_FILE);
        self.write_or_require_exact(&path, &canonical_json(&receipt)?, 0o440)?;
        Ok(receipt)
    }

    /// Finalizes the sole dynamic supervisor field from exact fresh source
    /// bytes and commits the ReadyForBootstrap receipt.  This function has no
    /// network or Redis capability and accepts no production path override.
    pub fn materialize_o2(
        &self,
        manifest_sha256: &str,
        materialization_policy_bytes: &[u8],
        config_template_bytes: &[u8],
        source_bytes: &[u8],
        broker_truth_checked_at_utc: &str,
        trusted_now: DateTime<Utc>,
    ) -> Result<Stage8bP1fMaterializedSetReceiptV1, Stage8bP1fAuthorityErrorV1> {
        if self.root != Path::new(STAGE8B_P1F_AUTHORITY_CONTROL_ROOT) {
            return Err(Stage8bP1fAuthorityErrorV1::InvalidPath);
        }
        self.materialize_o2_at(
            Path::new(STAGE8B_P1F_CONFIG_ROOT),
            manifest_sha256,
            materialization_policy_bytes,
            config_template_bytes,
            source_bytes,
            broker_truth_checked_at_utc,
            trusted_now,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn materialize_o2_at(
        &self,
        config_root: &Path,
        manifest_sha256: &str,
        materialization_policy_bytes: &[u8],
        config_template_bytes: &[u8],
        source_bytes: &[u8],
        broker_truth_checked_at_utc: &str,
        trusted_now: DateTime<Utc>,
    ) -> Result<Stage8bP1fMaterializedSetReceiptV1, Stage8bP1fAuthorityErrorV1> {
        let _lease = self.acquire_lease()?;
        self.reject_quarantine()?;
        if !config_root.is_absolute()
            || !valid_sha256(manifest_sha256)
            || source_bytes.is_empty()
            || source_bytes.len() as u64 > crate::STAGE8B_P1E_FIRST_BOOT_SOURCE_MAX_BYTES
        {
            return Err(Stage8bP1fAuthorityErrorV1::InvalidDocument);
        }
        let checked_at = parse_timestamp(broker_truth_checked_at_utc)?;
        let age = trusted_now.signed_duration_since(checked_at).num_seconds();
        if !(0..=300).contains(&age) {
            return Err(Stage8bP1fAuthorityErrorV1::InvalidValidityWindow);
        }
        let source_value: Value = serde_json::from_slice(source_bytes)
            .map_err(|_| Stage8bP1fAuthorityErrorV1::InvalidDocument)?;
        if !source_value.is_object() {
            return Err(Stage8bP1fAuthorityErrorV1::InvalidDocument);
        }
        let policy_sha256 = sha256_hex(materialization_policy_bytes);
        let template_sha256 = sha256_hex(config_template_bytes);
        let source_sha256 = sha256_hex(source_bytes);
        let template: Value = parse_canonical(config_template_bytes)?;
        let mut final_config = template.clone();
        let template_object = template
            .as_object()
            .ok_or(Stage8bP1fAuthorityErrorV1::InvalidDocument)?;
        if template_object
            .get("first_boot_source_bundle_sha256")
            .and_then(Value::as_str)
            != Some(STAGE8B_P1F_SOURCE_SHA256_TEMPLATE_SENTINEL)
        {
            return Err(Stage8bP1fAuthorityErrorV1::InvalidDocument);
        }
        final_config
            .as_object_mut()
            .ok_or(Stage8bP1fAuthorityErrorV1::InvalidDocument)?
            .insert(
                "first_boot_source_bundle_sha256".to_string(),
                Value::String(source_sha256.clone()),
            );
        let final_config_bytes = canonical_json(&final_config)?;
        let parsed = crate::parse_stage8b_p1e_supervisor_config_v1(&final_config_bytes)
            .map_err(|_| Stage8bP1fAuthorityErrorV1::InvalidDocument)?;
        let validated = crate::validate_stage8b_p1e_supervisor_config_v1(parsed, [0u8; 16])
            .map_err(|_| Stage8bP1fAuthorityErrorV1::InvalidDocument)?;
        let validated_source =
            crate::stage8b_p1e_first_boot_source::parse_stage8b_p1e_first_boot_source_v1(
                source_bytes,
                &source_sha256,
                validated.bootstrap().operational_identity_sha256(),
                validated.bootstrap().account_id().as_str(),
                trusted_now,
            )
            .map_err(|_| Stage8bP1fAuthorityErrorV1::InvalidDocument)?;
        if canonical_timestamp(validated_source.broker_truth_checked_at())
            != broker_truth_checked_at_utc
        {
            return Err(Stage8bP1fAuthorityErrorV1::InvalidDocument);
        }

        let authority = self.root.join(AUTHORITY_DIRECTORY);
        let manifest_directory = authority.join(MANIFESTS_DIRECTORY).join(manifest_sha256);
        let manifest_bytes =
            self.read_authority_bytes(&manifest_directory.join("phase-manifest.json"), 0o440)?;
        let phase_manifest: Stage8bP1fPhaseManifestV1 = parse_canonical(&manifest_bytes)?;
        let claim = self.read_claim_receipt(manifest_sha256)?;
        let claim_bytes = canonical_json(&claim)?;
        if phase_manifest.phase != Stage8bP1fPhaseV1::O2MaterializeBootstrap
            || phase_manifest.materialization_policy_sha256 != policy_sha256
            || phase_manifest.config_template_sha256 != template_sha256
            || claim.state != Stage8bP1fPhaseStateV1::Active
            || claim.manifest_sha256 != manifest_sha256
        {
            return Err(Stage8bP1fAuthorityErrorV1::InvalidDocument);
        }
        validate_not_expired(&claim.deadline_utc, trusted_now)?;

        let pending_path = manifest_directory.join(PENDING_MATERIALIZATION_FILE);
        let existing_receipt_path = manifest_directory.join(MATERIALIZED_SET_RECEIPT_FILE);
        if !pending_path.exists() && existing_receipt_path.exists() {
            let existing: Stage8bP1fMaterializedSetReceiptV1 =
                self.read_authority_file(&existing_receipt_path, 0o440)?;
            if existing.manifest_sha256 != manifest_sha256
                || existing.materialization_policy_sha256 != policy_sha256
                || existing.config_template_sha256 != template_sha256
                || existing.source_sha256 != source_sha256
                || existing.final_config_sha256 != sha256_hex(&final_config_bytes)
                || existing.broker_truth_checked_at_utc != broker_truth_checked_at_utc
                || existing.state != "ReadyForBootstrap"
            {
                return Err(Stage8bP1fAuthorityErrorV1::HistoryConflict);
            }
            self.validate_history(true)?;
            return Ok(existing);
        }
        let pending = if pending_path.exists() {
            let retained: PendingMaterializationV1 =
                self.read_authority_file(&pending_path, 0o440)?;
            if retained.schema_version != STAGE8B_P1F_AUTHORITY_SCHEMA_VERSION
                || retained.domain != "stage8b-p1f-pending-materialization-v1"
                || parse_timestamp(&retained.broker_truth_checked_at_utc).is_err()
                || parse_timestamp(&retained.ready_at_utc).is_err()
                || retained.manifest_sha256 != manifest_sha256
                || retained.materialization_policy_sha256 != policy_sha256
                || retained.config_template_sha256 != template_sha256
                || retained.source_sha256 != source_sha256
                || retained.final_config_sha256 != sha256_hex(&final_config_bytes)
                || retained.broker_truth_checked_at_utc != broker_truth_checked_at_utc
            {
                return Err(Stage8bP1fAuthorityErrorV1::HistoryConflict);
            }
            retained
        } else {
            let inspection = self.validate_history(true)?;
            if inspection.state != Stage8bP1fPhaseStateV1::Active
                || inspection.active_manifest_sha256.as_deref() != Some(manifest_sha256)
            {
                return Err(Stage8bP1fAuthorityErrorV1::ActiveConflict);
            }
            let pending = PendingMaterializationV1 {
                schema_version: STAGE8B_P1F_AUTHORITY_SCHEMA_VERSION,
                domain: "stage8b-p1f-pending-materialization-v1".to_string(),
                manifest_sha256: manifest_sha256.to_string(),
                authority_generation: inspection.authority_generation,
                authority_sequence: inspection.latest_sequence + 1,
                predecessor_event_sha256: inspection.latest_event_sha256,
                materialization_policy_sha256: policy_sha256,
                config_template_sha256: template_sha256,
                source_sha256,
                final_config_sha256: sha256_hex(&final_config_bytes),
                installation_sha256: phase_manifest.installation_sha256.clone(),
                claim_receipt_sha256: sha256_hex(&claim_bytes),
                broker_truth_checked_at_utc: broker_truth_checked_at_utc.to_string(),
                ready_at_utc: canonical_timestamp(trusted_now),
            };
            self.write_create_new(&pending_path, &canonical_json(&pending)?, 0o440)?;
            pending
        };
        self.continue_materialization_transaction(
            config_root,
            source_bytes,
            &final_config_bytes,
            &pending,
        )
    }

    fn continue_materialization_transaction(
        &self,
        config_root: &Path,
        source_bytes: &[u8],
        final_config_bytes: &[u8],
        pending: &PendingMaterializationV1,
    ) -> Result<Stage8bP1fMaterializedSetReceiptV1, Stage8bP1fAuthorityErrorV1> {
        self.validate_directory_at(config_root, 0o750)?;
        let bootstrap = config_root.join("bootstrap");
        self.validate_directory_at(&bootstrap, 0o750)?;
        let authority = self.root.join(AUTHORITY_DIRECTORY);
        let manifest_directory = authority
            .join(MANIFESTS_DIRECTORY)
            .join(&pending.manifest_sha256);
        let receipt = Stage8bP1fMaterializedSetReceiptV1 {
            schema_version: STAGE8B_P1F_AUTHORITY_SCHEMA_VERSION,
            domain: "stage8b-p1f-materialized-set-receipt-v1".to_string(),
            manifest_sha256: pending.manifest_sha256.clone(),
            authority_generation: pending.authority_generation,
            authority_sequence: pending.authority_sequence,
            predecessor_event_sha256: pending.predecessor_event_sha256.clone(),
            materialization_policy_sha256: pending.materialization_policy_sha256.clone(),
            config_template_sha256: pending.config_template_sha256.clone(),
            source_sha256: pending.source_sha256.clone(),
            final_config_sha256: pending.final_config_sha256.clone(),
            installation_sha256: pending.installation_sha256.clone(),
            claim_receipt_sha256: pending.claim_receipt_sha256.clone(),
            broker_truth_checked_at_utc: pending.broker_truth_checked_at_utc.clone(),
            ready_at_utc: pending.ready_at_utc.clone(),
            state: "ReadyForBootstrap".to_string(),
        };
        let receipt_bytes = canonical_json(&receipt)?;
        let event = AuthorityEventV1 {
            schema_version: STAGE8B_P1F_AUTHORITY_SCHEMA_VERSION,
            domain: "stage8b-p1f-authority-event-v1".to_string(),
            authority_generation: pending.authority_generation,
            authority_sequence: pending.authority_sequence,
            predecessor_event_sha256: pending.predecessor_event_sha256.clone(),
            event_kind: "O2_MATERIALIZED".to_string(),
            state: Stage8bP1fPhaseStateV1::Active,
            manifest_sha256: pending.manifest_sha256.clone(),
            receipt_sha256: sha256_hex(&receipt_bytes),
            recorded_at_utc: pending.ready_at_utc.clone(),
        };
        let event_bytes = canonical_json(&event)?;
        let event_sha256 = event_digest(&event_bytes);
        let head: HistoryHeadV1 =
            self.read_authority_file(&authority.join(HISTORY_HEAD_FILE), 0o440)?;
        if head.latest_sequence == pending.authority_sequence {
            if head.latest_event_sha256 != event_sha256
                || head.state != Stage8bP1fPhaseStateV1::Active
                || head.active_manifest_sha256.as_deref() != Some(pending.manifest_sha256.as_str())
            {
                return Err(Stage8bP1fAuthorityErrorV1::HistoryConflict);
            }
        } else {
            if head.latest_sequence + 1 != pending.authority_sequence
                || head.latest_event_sha256 != pending.predecessor_event_sha256
                || head.state != Stage8bP1fPhaseStateV1::Active
                || head.active_manifest_sha256.as_deref() != Some(pending.manifest_sha256.as_str())
            {
                return Err(Stage8bP1fAuthorityErrorV1::HistoryConflict);
            }
            self.validate_history_with_pending(true, None, Some(pending.authority_sequence))?;
        }
        self.write_external_or_require_exact(
            &bootstrap.join("stage8b-p1-first-boot-source-v1.json"),
            source_bytes,
            crate::STAGE8B_P1E_FIRST_BOOT_SOURCE_MAX_BYTES,
        )?;
        self.write_external_or_require_exact(
            &config_root.join("supervisor.json"),
            final_config_bytes,
            STAGE8B_P1F_MAX_AUTHORITY_BYTES,
        )?;
        self.write_or_require_exact(
            &manifest_directory.join(MATERIALIZED_SET_RECEIPT_FILE),
            &receipt_bytes,
            0o440,
        )?;
        inject_test_fault(4)?;
        self.write_or_require_exact(
            &event_path(&authority, pending.authority_sequence),
            &event_bytes,
            0o440,
        )?;
        if head.latest_sequence + 1 == pending.authority_sequence {
            let materialized_head = HistoryHeadV1 {
                schema_version: STAGE8B_P1F_AUTHORITY_SCHEMA_VERSION,
                domain: "stage8b-p1f-history-head-v1".to_string(),
                authority_generation: pending.authority_generation,
                latest_sequence: pending.authority_sequence,
                latest_event_sha256: event_sha256,
                state: Stage8bP1fPhaseStateV1::Active,
                active_manifest_sha256: Some(pending.manifest_sha256.clone()),
                deadline_utc: head.deadline_utc,
                force_kill_at_utc: None,
            };
            self.replace_exact(
                &authority.join(HISTORY_HEAD_FILE),
                &canonical_json(&materialized_head)?,
                0o440,
            )?;
        }
        fs::remove_file(manifest_directory.join(PENDING_MATERIALIZATION_FILE))?;
        sync_directory(&manifest_directory)?;
        self.validate_history(true)?;
        if self.read_external_bytes(
            &bootstrap.join("stage8b-p1-first-boot-source-v1.json"),
            crate::STAGE8B_P1E_FIRST_BOOT_SOURCE_MAX_BYTES,
        )? != source_bytes
            || self.read_external_bytes(
                &config_root.join("supervisor.json"),
                STAGE8B_P1F_MAX_AUTHORITY_BYTES,
            )? != final_config_bytes
        {
            return Err(Stage8bP1fAuthorityErrorV1::InvalidDocument);
        }
        Ok(receipt)
    }

    fn begin_stopping_phase(
        &self,
        manifest_sha256: &str,
        reason_code: &str,
        stopping_started_at: DateTime<Utc>,
        force_kill_at: DateTime<Utc>,
        boot_id: &str,
    ) -> Result<Stage8bP1fStoppingReceiptV1, Stage8bP1fAuthorityErrorV1> {
        let _lease = self.acquire_lease()?;
        self.reject_quarantine()?;
        if !valid_sha256(manifest_sha256)
            || !canonical_token(reason_code)
            || !canonical_token(boot_id)
            || force_kill_at - stopping_started_at != chrono::Duration::seconds(30)
        {
            return Err(Stage8bP1fAuthorityErrorV1::InvalidDocument);
        }
        let authority = self.root.join(AUTHORITY_DIRECTORY);
        let pending_path = authority.join(PENDING_STOPPING_FILE);
        let pending = if pending_path.exists() {
            let retained: PendingStoppingV1 = self.read_authority_file(&pending_path, 0o440)?;
            if retained.schema_version != STAGE8B_P1F_AUTHORITY_SCHEMA_VERSION
                || retained.domain != "stage8b-p1f-pending-stopping-v1"
                || retained.manifest_sha256 != manifest_sha256
                || retained.reason_code != reason_code
                || retained.stopping_started_at_utc != canonical_timestamp(stopping_started_at)
                || retained.force_kill_at_utc != canonical_timestamp(force_kill_at)
                || retained.boot_id != boot_id
            {
                return Err(Stage8bP1fAuthorityErrorV1::PendingRecoveryRequired);
            }
            retained
        } else {
            let inspection = self.validate_history(true)?;
            if inspection.state != Stage8bP1fPhaseStateV1::Active
                || inspection.active_manifest_sha256.as_deref() != Some(manifest_sha256)
            {
                return Err(Stage8bP1fAuthorityErrorV1::ActiveConflict);
            }
            let pending = PendingStoppingV1 {
                schema_version: STAGE8B_P1F_AUTHORITY_SCHEMA_VERSION,
                domain: "stage8b-p1f-pending-stopping-v1".to_string(),
                manifest_sha256: manifest_sha256.to_string(),
                authority_generation: inspection.authority_generation,
                authority_sequence: inspection.latest_sequence + 1,
                predecessor_event_sha256: inspection.latest_event_sha256,
                reason_code: reason_code.to_string(),
                stopping_started_at_utc: canonical_timestamp(stopping_started_at),
                force_kill_at_utc: canonical_timestamp(force_kill_at),
                boot_id: boot_id.to_string(),
            };
            self.write_create_new(&pending_path, &canonical_json(&pending)?, 0o440)?;
            pending
        };
        self.continue_stopping_transaction(&pending)
    }

    fn continue_stopping_transaction(
        &self,
        pending: &PendingStoppingV1,
    ) -> Result<Stage8bP1fStoppingReceiptV1, Stage8bP1fAuthorityErrorV1> {
        let authority = self.root.join(AUTHORITY_DIRECTORY);
        let manifest_directory = authority
            .join(MANIFESTS_DIRECTORY)
            .join(&pending.manifest_sha256);
        let receipt_path = manifest_directory.join(format!(
            "stopping-receipt-{:020}.json",
            pending.authority_sequence
        ));
        let receipt = Stage8bP1fStoppingReceiptV1 {
            schema_version: STAGE8B_P1F_AUTHORITY_SCHEMA_VERSION,
            domain: "stage8b-p1f-stopping-receipt-v1".to_string(),
            manifest_sha256: pending.manifest_sha256.clone(),
            authority_generation: pending.authority_generation,
            authority_sequence: pending.authority_sequence,
            predecessor_event_sha256: pending.predecessor_event_sha256.clone(),
            reason_code: pending.reason_code.clone(),
            stopping_started_at_utc: pending.stopping_started_at_utc.clone(),
            force_kill_at_utc: pending.force_kill_at_utc.clone(),
        };
        let receipt_bytes = canonical_json(&receipt)?;
        let event = AuthorityEventV1 {
            schema_version: STAGE8B_P1F_AUTHORITY_SCHEMA_VERSION,
            domain: "stage8b-p1f-authority-event-v1".to_string(),
            authority_generation: pending.authority_generation,
            authority_sequence: pending.authority_sequence,
            predecessor_event_sha256: pending.predecessor_event_sha256.clone(),
            event_kind: "PHASE_STOPPING".to_string(),
            state: Stage8bP1fPhaseStateV1::Stopping,
            manifest_sha256: pending.manifest_sha256.clone(),
            receipt_sha256: sha256_hex(&receipt_bytes),
            recorded_at_utc: pending.stopping_started_at_utc.clone(),
        };
        let event_bytes = canonical_json(&event)?;
        let event_sha256 = event_digest(&event_bytes);
        let head: HistoryHeadV1 =
            self.read_authority_file(&authority.join(HISTORY_HEAD_FILE), 0o440)?;
        if head.latest_sequence == pending.authority_sequence {
            if head.latest_event_sha256 != event_sha256
                || head.state != Stage8bP1fPhaseStateV1::Stopping
                || head.active_manifest_sha256.as_deref() != Some(pending.manifest_sha256.as_str())
                || head.force_kill_at_utc.as_deref() != Some(&pending.force_kill_at_utc)
            {
                return Err(Stage8bP1fAuthorityErrorV1::HistoryConflict);
            }
        } else {
            if head.latest_sequence + 1 != pending.authority_sequence
                || head.latest_event_sha256 != pending.predecessor_event_sha256
                || head.state != Stage8bP1fPhaseStateV1::Active
                || head.active_manifest_sha256.as_deref() != Some(pending.manifest_sha256.as_str())
            {
                return Err(Stage8bP1fAuthorityErrorV1::HistoryConflict);
            }
            self.validate_history_with_pending(true, None, Some(pending.authority_sequence))?;
            self.write_or_require_exact(&receipt_path, &receipt_bytes, 0o440)?;
            self.write_or_require_exact(
                &event_path(&authority, pending.authority_sequence),
                &event_bytes,
                0o440,
            )?;
            let stopping_head = HistoryHeadV1 {
                schema_version: STAGE8B_P1F_AUTHORITY_SCHEMA_VERSION,
                domain: "stage8b-p1f-history-head-v1".to_string(),
                authority_generation: pending.authority_generation,
                latest_sequence: pending.authority_sequence,
                latest_event_sha256: event_sha256,
                state: Stage8bP1fPhaseStateV1::Stopping,
                active_manifest_sha256: Some(pending.manifest_sha256.clone()),
                deadline_utc: head.deadline_utc,
                force_kill_at_utc: Some(pending.force_kill_at_utc.clone()),
            };
            self.replace_exact(
                &authority.join(HISTORY_HEAD_FILE),
                &canonical_json(&stopping_head)?,
                0o440,
            )?;
        }
        fs::remove_file(authority.join(PENDING_STOPPING_FILE))?;
        sync_directory(&authority)?;
        self.validate_history(true)?;
        Ok(receipt)
    }

    pub fn resume_stopping_phase(
        &self,
        manifest_sha256: &str,
        trusted_now: DateTime<Utc>,
    ) -> Result<Stage8bP1fRunPermitV1, Stage8bP1fAuthorityErrorV1> {
        let _lease = self.acquire_lease()?;
        self.reject_quarantine()?;
        if !valid_sha256(manifest_sha256) {
            return Err(Stage8bP1fAuthorityErrorV1::InvalidDocument);
        }
        let execution_lock = self.acquire_execution_lock()?;
        let authority = self.root.join(AUTHORITY_DIRECTORY);
        let pending_path = authority.join(PENDING_STOPPING_FILE);
        if pending_path.exists() {
            let pending: PendingStoppingV1 = self.read_authority_file(&pending_path, 0o440)?;
            let started = parse_timestamp(&pending.stopping_started_at_utc)?;
            let force = parse_timestamp(&pending.force_kill_at_utc)?;
            if pending.schema_version != STAGE8B_P1F_AUTHORITY_SCHEMA_VERSION
                || pending.domain != "stage8b-p1f-pending-stopping-v1"
                || pending.manifest_sha256 != manifest_sha256
                || !canonical_token(&pending.reason_code)
                || !canonical_token(&pending.boot_id)
                || force - started != chrono::Duration::seconds(30)
            {
                return Err(Stage8bP1fAuthorityErrorV1::PendingRecoveryRequired);
            }
            self.continue_stopping_transaction(&pending)?;
        }
        let inspection = self.validate_history(true)?;
        if inspection.state != Stage8bP1fPhaseStateV1::Stopping
            || inspection.active_manifest_sha256.as_deref() != Some(manifest_sha256)
        {
            return Err(Stage8bP1fAuthorityErrorV1::ActiveConflict);
        }
        let receipt: Stage8bP1fStoppingReceiptV1 = self.read_authority_file(
            &self
                .root
                .join(AUTHORITY_DIRECTORY)
                .join(MANIFESTS_DIRECTORY)
                .join(manifest_sha256)
                .join(format!(
                    "stopping-receipt-{:020}.json",
                    inspection.latest_sequence
                )),
            0o440,
        )?;
        let claim = self.read_claim_receipt(manifest_sha256)?;
        let stopping_started_at = parse_timestamp(&receipt.stopping_started_at_utc)?;
        let force_kill_at = parse_timestamp(&receipt.force_kill_at_utc)?;
        // A public resume runs after the process-local monotonic witness was
        // lost.  UTC can authenticate the retained transaction, but it cannot
        // prove how much real grace remains.  Preserve the original receipt
        // and timestamps while failing closed on the first poll; only the
        // original live permit may consume the bounded 30-second grace.
        let force_kill_after_elapsed = StdDuration::ZERO;
        Ok(Stage8bP1fRunPermitV1 {
            manifest_sha256: manifest_sha256.to_string(),
            phase: claim.phase,
            claimed_at: parse_timestamp(&claim.claimed_at_utc)?,
            deadline: parse_timestamp(&claim.deadline_utc)?,
            admitted_wall: trusted_now,
            admitted_monotonic: Instant::now(),
            authority_root: self.root.clone(),
            expected_uid: self.expected_uid,
            service_gid: self.service_gid,
            execution_lock,
            boot_id: current_boot_id()?,
            stopping_started_at: Some(stopping_started_at),
            force_kill_at: Some(force_kill_at),
            force_kill_after_elapsed: Some(force_kill_after_elapsed),
            stopping_reason_code: Some(receipt.reason_code),
            last_trusted_wall: trusted_now,
        })
    }

    /// Commits one terminal transition for the exact active manifest.  The
    /// create-once marker makes a retry continue only that transaction; a new
    /// phase cannot be claimed until the terminal event and head are durable.
    pub fn finish_phase(
        &self,
        manifest_sha256: &str,
        terminal_state: Stage8bP1fPhaseStateV1,
        reason_code: &str,
        trusted_now: DateTime<Utc>,
    ) -> Result<Stage8bP1fTerminalReceiptV1, Stage8bP1fAuthorityErrorV1> {
        let _lease = self.acquire_lease()?;
        self.reject_quarantine()?;
        if !matches!(
            terminal_state,
            Stage8bP1fPhaseStateV1::Completed
                | Stage8bP1fPhaseStateV1::Failed
                | Stage8bP1fPhaseStateV1::Expired
        ) || !canonical_token(reason_code)
            || !valid_sha256(manifest_sha256)
        {
            return Err(Stage8bP1fAuthorityErrorV1::InvalidDocument);
        }
        let authority = self.root.join(AUTHORITY_DIRECTORY);
        let pending_path = authority.join(PENDING_TERMINAL_FILE);
        let pending = if pending_path.exists() {
            let retained: PendingTerminalV1 = self.read_authority_file(&pending_path, 0o440)?;
            if retained.schema_version != STAGE8B_P1F_AUTHORITY_SCHEMA_VERSION
                || retained.domain != "stage8b-p1f-pending-terminal-v1"
                || parse_timestamp(&retained.recorded_at_utc).is_err()
                || retained.manifest_sha256 != manifest_sha256
                || retained.terminal_state != terminal_state
                || retained.reason_code != reason_code
            {
                return Err(Stage8bP1fAuthorityErrorV1::ActiveConflict);
            }
            retained
        } else {
            let inspection = self.validate_history(true)?;
            if !matches!(
                inspection.state,
                Stage8bP1fPhaseStateV1::Active | Stage8bP1fPhaseStateV1::Stopping
            ) || inspection.active_manifest_sha256.as_deref() != Some(manifest_sha256)
            {
                return Err(Stage8bP1fAuthorityErrorV1::ActiveConflict);
            }
            let deadline = inspection
                .deadline_utc
                .as_deref()
                .ok_or(Stage8bP1fAuthorityErrorV1::HistoryConflict)?;
            let is_expired = trusted_now >= parse_timestamp(deadline)?;
            if (terminal_state == Stage8bP1fPhaseStateV1::Expired) != is_expired {
                return Err(Stage8bP1fAuthorityErrorV1::InvalidValidityWindow);
            }
            let pending = PendingTerminalV1 {
                schema_version: STAGE8B_P1F_AUTHORITY_SCHEMA_VERSION,
                domain: "stage8b-p1f-pending-terminal-v1".to_string(),
                manifest_sha256: manifest_sha256.to_string(),
                authority_generation: inspection.authority_generation,
                authority_sequence: inspection.latest_sequence + 1,
                predecessor_event_sha256: inspection.latest_event_sha256,
                terminal_state,
                reason_code: reason_code.to_string(),
                recorded_at_utc: canonical_timestamp(trusted_now),
            };
            self.write_create_new(&pending_path, &canonical_json(&pending)?, 0o440)?;
            pending
        };
        self.continue_terminal_transaction(&pending)
    }

    /// Continues only an already durable pending-terminal transaction. This
    /// is the runner-loss/lost-response path: it never chooses a new terminal
    /// state, reason or timestamp and therefore cannot reclassify an old
    /// decision against the current wall clock.
    pub fn resume_pending_terminal(
        &self,
        manifest_sha256: &str,
    ) -> Result<Option<Stage8bP1fTerminalReceiptV1>, Stage8bP1fAuthorityErrorV1> {
        let _lease = self.acquire_lease()?;
        self.reject_quarantine()?;
        if !valid_sha256(manifest_sha256) {
            return Err(Stage8bP1fAuthorityErrorV1::InvalidDocument);
        }
        let pending_path = self
            .root
            .join(AUTHORITY_DIRECTORY)
            .join(PENDING_TERMINAL_FILE);
        if !pending_path.exists() {
            return Ok(None);
        }
        let pending: PendingTerminalV1 = self.read_authority_file(&pending_path, 0o440)?;
        if pending.schema_version != STAGE8B_P1F_AUTHORITY_SCHEMA_VERSION
            || pending.domain != "stage8b-p1f-pending-terminal-v1"
            || pending.manifest_sha256 != manifest_sha256
            || parse_timestamp(&pending.recorded_at_utc).is_err()
            || !matches!(
                pending.terminal_state,
                Stage8bP1fPhaseStateV1::Completed
                    | Stage8bP1fPhaseStateV1::Failed
                    | Stage8bP1fPhaseStateV1::Expired
            )
            || !canonical_token(&pending.reason_code)
        {
            return Err(Stage8bP1fAuthorityErrorV1::PendingRecoveryRequired);
        }
        self.continue_terminal_transaction(&pending).map(Some)
    }

    /// Returns the already committed terminal receipt for an exact manifest.
    /// This is a read-only idempotence path for systemd `ExecStopPost`; it
    /// cannot select or commit a terminal outcome.
    pub fn terminal_receipt(
        &self,
        manifest_sha256: &str,
    ) -> Result<Option<Stage8bP1fTerminalReceiptV1>, Stage8bP1fAuthorityErrorV1> {
        if !valid_sha256(manifest_sha256) {
            return Err(Stage8bP1fAuthorityErrorV1::InvalidDocument);
        }
        let inspection = self.validate_history(true)?;
        if !matches!(
            inspection.state,
            Stage8bP1fPhaseStateV1::Completed
                | Stage8bP1fPhaseStateV1::Failed
                | Stage8bP1fPhaseStateV1::Expired
        ) {
            return Ok(None);
        }
        let path = self
            .root
            .join(AUTHORITY_DIRECTORY)
            .join(MANIFESTS_DIRECTORY)
            .join(manifest_sha256)
            .join(format!(
                "terminal-receipt-{:020}.json",
                inspection.latest_sequence
            ));
        if !path.exists() {
            return Ok(None);
        }
        let receipt: Stage8bP1fTerminalReceiptV1 = self.read_authority_file(&path, 0o440)?;
        if receipt.schema_version != STAGE8B_P1F_AUTHORITY_SCHEMA_VERSION
            || receipt.domain != "stage8b-p1f-terminal-receipt-v1"
            || receipt.manifest_sha256 != manifest_sha256
            || receipt.authority_generation != inspection.authority_generation
            || receipt.authority_sequence != inspection.latest_sequence
            || receipt.terminal_state != inspection.state
            || parse_timestamp(&receipt.recorded_at_utc).is_err()
            || !canonical_token(&receipt.reason_code)
        {
            return Err(Stage8bP1fAuthorityErrorV1::HistoryConflict);
        }
        Ok(Some(receipt))
    }

    fn continue_terminal_transaction(
        &self,
        pending: &PendingTerminalV1,
    ) -> Result<Stage8bP1fTerminalReceiptV1, Stage8bP1fAuthorityErrorV1> {
        let authority = self.root.join(AUTHORITY_DIRECTORY);
        let head: HistoryHeadV1 =
            self.read_authority_file(&authority.join(HISTORY_HEAD_FILE), 0o440)?;
        let terminal_name = format!("terminal-receipt-{:020}.json", pending.authority_sequence);
        let terminal_path = authority
            .join(MANIFESTS_DIRECTORY)
            .join(&pending.manifest_sha256)
            .join(terminal_name);
        let receipt = Stage8bP1fTerminalReceiptV1 {
            schema_version: STAGE8B_P1F_AUTHORITY_SCHEMA_VERSION,
            domain: "stage8b-p1f-terminal-receipt-v1".to_string(),
            manifest_sha256: pending.manifest_sha256.clone(),
            authority_generation: pending.authority_generation,
            authority_sequence: pending.authority_sequence,
            predecessor_event_sha256: pending.predecessor_event_sha256.clone(),
            terminal_state: pending.terminal_state,
            reason_code: pending.reason_code.clone(),
            recorded_at_utc: pending.recorded_at_utc.clone(),
        };
        let receipt_bytes = canonical_json(&receipt)?;
        let event = AuthorityEventV1 {
            schema_version: STAGE8B_P1F_AUTHORITY_SCHEMA_VERSION,
            domain: "stage8b-p1f-authority-event-v1".to_string(),
            authority_generation: pending.authority_generation,
            authority_sequence: pending.authority_sequence,
            predecessor_event_sha256: pending.predecessor_event_sha256.clone(),
            event_kind: "PHASE_TERMINAL".to_string(),
            state: pending.terminal_state,
            manifest_sha256: pending.manifest_sha256.clone(),
            receipt_sha256: sha256_hex(&receipt_bytes),
            recorded_at_utc: pending.recorded_at_utc.clone(),
        };
        let event_bytes = canonical_json(&event)?;
        let event_sha256 = event_digest(&event_bytes);
        if head.latest_sequence == pending.authority_sequence {
            if head.latest_event_sha256 != event_sha256
                || head.state != pending.terminal_state
                || head.active_manifest_sha256.is_some()
                || head.deadline_utc.is_some()
                || self.read_authority_bytes(&terminal_path, 0o440)? != receipt_bytes
                || self.read_authority_bytes(
                    &event_path(&authority, pending.authority_sequence),
                    0o440,
                )? != event_bytes
            {
                return Err(Stage8bP1fAuthorityErrorV1::HistoryConflict);
            }
        } else {
            if head.latest_sequence + 1 != pending.authority_sequence
                || head.latest_event_sha256 != pending.predecessor_event_sha256
                || !matches!(
                    head.state,
                    Stage8bP1fPhaseStateV1::Active | Stage8bP1fPhaseStateV1::Stopping
                )
                || head.active_manifest_sha256.as_deref() != Some(pending.manifest_sha256.as_str())
            {
                return Err(Stage8bP1fAuthorityErrorV1::HistoryConflict);
            }
            self.validate_history_with_pending(true, None, Some(pending.authority_sequence))?;
            self.write_or_require_exact(&terminal_path, &receipt_bytes, 0o440)?;
            self.write_or_require_exact(
                &event_path(&authority, pending.authority_sequence),
                &event_bytes,
                0o440,
            )?;
            let terminal_head = HistoryHeadV1 {
                schema_version: STAGE8B_P1F_AUTHORITY_SCHEMA_VERSION,
                domain: "stage8b-p1f-history-head-v1".to_string(),
                authority_generation: pending.authority_generation,
                latest_sequence: pending.authority_sequence,
                latest_event_sha256: event_sha256,
                state: pending.terminal_state,
                active_manifest_sha256: None,
                deadline_utc: None,
                force_kill_at_utc: None,
            };
            self.replace_exact(
                &authority.join(HISTORY_HEAD_FILE),
                &canonical_json(&terminal_head)?,
                0o440,
            )?;
        }
        fs::remove_file(authority.join(PENDING_TERMINAL_FILE))?;
        sync_directory(&authority)?;
        self.validate_history(true)?;
        Ok(receipt)
    }

    fn validate_genesis_manifest(
        &self,
        manifest: &Stage8bP1fGenesisManifestV1,
        trusted_now: DateTime<Utc>,
    ) -> Result<(), Stage8bP1fAuthorityErrorV1> {
        if manifest.schema_version != STAGE8B_P1F_AUTHORITY_SCHEMA_VERSION
            || manifest.domain != "stage8b-p1f-genesis-manifest-v1"
            || manifest.installation_id.is_empty()
            || manifest.target_host_id != STAGE8B_P1F_TARGET_HOST_ID
            || manifest.target_host_ssh_ed25519_sha256 != STAGE8B_P1F_TARGET_HOST_SSH_ED25519_SHA256
            || manifest.control_root != self.root.to_string_lossy()
            || manifest.authority_generation == 0
            || !valid_sha256(&manifest.ceremony_nonce_sha256)
            || !valid_sha256(&manifest.genesis_head_sha256)
        {
            return Err(Stage8bP1fAuthorityErrorV1::InvalidDocument);
        }
        let expected_head = genesis_head_sha256(manifest);
        if manifest.genesis_head_sha256 != expected_head {
            return Err(Stage8bP1fAuthorityErrorV1::InvalidDocument);
        }
        validate_window(
            &manifest.not_before_utc,
            &manifest.expires_at_utc,
            trusted_now,
        )
    }

    fn validate_phase_manifest(
        &self,
        manifest: &Stage8bP1fPhaseManifestV1,
        trusted_now: DateTime<Utc>,
    ) -> Result<(), Stage8bP1fAuthorityErrorV1> {
        if manifest.schema_version != STAGE8B_P1F_AUTHORITY_SCHEMA_VERSION
            || manifest.domain != "stage8b-p1f-phase-manifest-v1"
            || manifest.installation_id.is_empty()
            || manifest.target_host_id != STAGE8B_P1F_TARGET_HOST_ID
            || manifest.target_host_ssh_ed25519_sha256 != STAGE8B_P1F_TARGET_HOST_SSH_ED25519_SHA256
            || manifest.authority_generation == 0
            || manifest.authority_sequence == 0
            || !valid_sha256(&manifest.predecessor_event_sha256)
            || !valid_sha256(&manifest.accepted_source_tree_sha256)
            || !valid_sha256(&manifest.materialization_policy_sha256)
            || !valid_sha256(&manifest.config_template_sha256)
            || !valid_sha256(&manifest.installation_sha256)
            || manifest.controller_id.is_empty()
        {
            return Err(Stage8bP1fAuthorityErrorV1::InvalidDocument);
        }
        validate_window(
            &manifest.not_before_utc,
            &manifest.deadline_utc,
            trusted_now,
        )?;
        let start = parse_timestamp(&manifest.not_before_utc)?;
        let deadline = parse_timestamp(&manifest.deadline_utc)?;
        let duration = deadline.signed_duration_since(start).num_seconds();
        if (manifest.phase == Stage8bP1fPhaseV1::O3SyntheticPaper && duration != 1_800)
            || (manifest.phase == Stage8bP1fPhaseV1::O4FinamReadOnly && duration != 10_800)
        {
            return Err(Stage8bP1fAuthorityErrorV1::InvalidValidityWindow);
        }
        Ok(())
    }

    fn validate_history(
        &self,
        require_activation: bool,
    ) -> Result<Stage8bP1fAuthorityInspectionV1, Stage8bP1fAuthorityErrorV1> {
        self.validate_history_with_pending(require_activation, None, None)
    }

    fn validate_history_with_pending(
        &self,
        require_activation: bool,
        pending: Option<&PendingClaimV1>,
        trailing_event_sequence: Option<u64>,
    ) -> Result<Stage8bP1fAuthorityInspectionV1, Stage8bP1fAuthorityErrorV1> {
        self.validate_root_identity()?;
        self.reject_quarantine()?;
        let authority = self.root.join(AUTHORITY_DIRECTORY);
        if !authority.exists() {
            return Err(Stage8bP1fAuthorityErrorV1::NotInitialized);
        }
        self.validate_directory(&authority)?;
        self.validate_directory(&authority.join(EVENTS_DIRECTORY))?;
        self.validate_directory(&authority.join(MANIFESTS_DIRECTORY))?;
        let mut present_pending = [
            PENDING_CLAIM_FILE,
            PENDING_STOPPING_FILE,
            PENDING_TERMINAL_FILE,
        ]
        .iter()
        .filter_map(|name| {
            let path = authority.join(name);
            path.exists().then(|| ((*name).to_string(), path))
        })
        .collect::<Vec<_>>();
        for entry in fs::read_dir(authority.join(MANIFESTS_DIRECTORY))? {
            let path = entry?.path().join(PENDING_MATERIALIZATION_FILE);
            if path.exists() {
                present_pending.push((PENDING_MATERIALIZATION_FILE.to_string(), path));
            }
        }
        match (pending, trailing_event_sequence) {
            (Some(expected), None) => {
                if present_pending.len() != 1 || present_pending[0].0 != PENDING_CLAIM_FILE {
                    return Err(Stage8bP1fAuthorityErrorV1::PendingRecoveryRequired);
                }
                let retained: PendingClaimV1 =
                    self.read_authority_file(&present_pending[0].1, 0o440)?;
                if &retained != expected {
                    return Err(Stage8bP1fAuthorityErrorV1::PendingRecoveryRequired);
                }
            }
            (None, Some(sequence)) => {
                if present_pending.len() != 1 || present_pending[0].0 == PENDING_CLAIM_FILE {
                    return Err(Stage8bP1fAuthorityErrorV1::PendingRecoveryRequired);
                }
                let bytes = self.read_authority_bytes(&present_pending[0].1, 0o440)?;
                let value: Value = serde_json::from_slice(&bytes)
                    .map_err(|_| Stage8bP1fAuthorityErrorV1::InvalidDocument)?;
                if value.get("authority_sequence").and_then(Value::as_u64) != Some(sequence) {
                    return Err(Stage8bP1fAuthorityErrorV1::PendingRecoveryRequired);
                }
            }
            (None, None) if !present_pending.is_empty() => {
                return Err(Stage8bP1fAuthorityErrorV1::PendingRecoveryRequired)
            }
            _ => {}
        }
        let transaction: GenesisTransactionV1 =
            self.read_authority_file(&authority.join(GENESIS_TRANSACTION_FILE), 0o440)?;
        let manifest: Stage8bP1fGenesisManifestV1 =
            self.read_authority_file(&authority.join(GENESIS_MANIFEST_FILE), 0o440)?;
        let receipt: Stage8bP1fGenesisReceiptV1 =
            self.read_authority_file(&authority.join(GENESIS_RECEIPT_FILE), 0o440)?;
        if transaction.schema_version != STAGE8B_P1F_AUTHORITY_SCHEMA_VERSION
            || transaction.domain != "stage8b-p1f-genesis-transaction-v1"
            || manifest.schema_version != STAGE8B_P1F_AUTHORITY_SCHEMA_VERSION
            || manifest.domain != "stage8b-p1f-genesis-manifest-v1"
            || receipt.schema_version != STAGE8B_P1F_AUTHORITY_SCHEMA_VERSION
            || receipt.domain != "stage8b-p1f-genesis-receipt-v1"
            || parse_timestamp(&transaction.committed_at_utc).is_err()
            || parse_timestamp(&receipt.committed_at_utc).is_err()
            || transaction.genesis_manifest_sha256 != sha256_hex(&canonical_json(&manifest)?)
            || receipt.genesis_manifest_sha256 != transaction.genesis_manifest_sha256
            || transaction.authority_generation != manifest.authority_generation
            || receipt.authority_generation != manifest.authority_generation
            || transaction.ceremony_nonce_sha256 != manifest.ceremony_nonce_sha256
            || receipt.ceremony_nonce_sha256 != manifest.ceremony_nonce_sha256
            || receipt.genesis_head_sha256 != manifest.genesis_head_sha256
        {
            return Err(Stage8bP1fAuthorityErrorV1::HistoryConflict);
        }
        let activated = authority.join(ACTIVATION_CERTIFICATE_FILE).exists();
        if require_activation && !activated {
            return Err(Stage8bP1fAuthorityErrorV1::NotActivated);
        }
        if activated {
            let certificate: Stage8bP1fActivationCertificateV1 =
                self.read_authority_file(&authority.join(ACTIVATION_CERTIFICATE_FILE), 0o440)?;
            if certificate.schema_version != STAGE8B_P1F_AUTHORITY_SCHEMA_VERSION
                || certificate.domain != "stage8b-p1f-activation-certificate-v1"
                || parse_timestamp(&certificate.activated_at_utc).is_err()
                || parse_timestamp(&certificate.expires_at_utc).is_err()
                || certificate.authority_generation != manifest.authority_generation
                || certificate.genesis_manifest_sha256 != transaction.genesis_manifest_sha256
                || certificate.genesis_receipt_sha256 != sha256_hex(&canonical_json(&receipt)?)
                || certificate.genesis_head_sha256 != manifest.genesis_head_sha256
            {
                return Err(Stage8bP1fAuthorityErrorV1::HistoryConflict);
            }
        }
        let head: HistoryHeadV1 =
            self.read_authority_file(&authority.join(HISTORY_HEAD_FILE), 0o440)?;
        if head.schema_version != STAGE8B_P1F_AUTHORITY_SCHEMA_VERSION
            || head.domain != "stage8b-p1f-history-head-v1"
            || head.authority_generation != manifest.authority_generation
            || head.latest_sequence == 0
                && !matches!(
                    head.state,
                    Stage8bP1fPhaseStateV1::GenesisPrepared
                        | Stage8bP1fPhaseStateV1::GenesisActivated
                )
            || activated
                && head.latest_sequence == 0
                && head.state != Stage8bP1fPhaseStateV1::GenesisActivated
        {
            return Err(Stage8bP1fAuthorityErrorV1::HistoryConflict);
        }
        let entries =
            fs::read_dir(authority.join(EVENTS_DIRECTORY))?.collect::<Result<Vec<_>, _>>()?;
        let committed_event_count = usize::try_from(head.latest_sequence + 1)
            .map_err(|_| Stage8bP1fAuthorityErrorV1::HistoryConflict)?;
        let pending_event_sequence = pending
            .map(|value| value.authority_sequence)
            .or(trailing_event_sequence);
        let pending_event_exists = pending_event_sequence
            .map(|sequence| event_path(&authority, sequence).exists())
            .unwrap_or(false);
        let prepared_event_exists = match (pending_event_sequence, present_pending.first()) {
            (Some(sequence), Some((name, path))) => {
                self.validate_prepared_pending_event(&authority, &head, name, path, sequence)?
            }
            _ => false,
        };
        if pending_event_exists && prepared_event_exists {
            return Err(Stage8bP1fAuthorityErrorV1::HistoryConflict);
        }
        let expected_event_count =
            committed_event_count + usize::from(pending_event_exists || prepared_event_exists);
        if entries.len() != expected_event_count {
            return Err(Stage8bP1fAuthorityErrorV1::HistoryConflict);
        }
        let mut predecessor = ZERO_SHA256.to_string();
        let mut manifests = BTreeSet::new();
        let mut latest_hash = String::new();
        let mut projected_state = if activated
            || (!require_activation
                && head.latest_sequence == 0
                && head.state == Stage8bP1fPhaseStateV1::GenesisActivated)
        {
            Stage8bP1fPhaseStateV1::GenesisActivated
        } else {
            Stage8bP1fPhaseStateV1::GenesisPrepared
        };
        let mut projected_active: Option<String> = None;
        let mut projected_deadline: Option<String> = None;
        let mut projected_force_kill: Option<String> = None;
        for sequence in 0..=head.latest_sequence {
            let bytes = self.read_authority_bytes(&event_path(&authority, sequence), 0o440)?;
            let event: AuthorityEventV1 = parse_canonical(&bytes)?;
            if event.schema_version != STAGE8B_P1F_AUTHORITY_SCHEMA_VERSION
                || event.domain != "stage8b-p1f-authority-event-v1"
                || parse_timestamp(&event.recorded_at_utc).is_err()
                || !valid_sha256(&event.manifest_sha256)
                || !valid_sha256(&event.receipt_sha256)
                || event.authority_generation != head.authority_generation
                || event.authority_sequence != sequence
                || event.predecessor_event_sha256 != predecessor
            {
                return Err(Stage8bP1fAuthorityErrorV1::HistoryConflict);
            }
            if event.event_kind == "PHASE_CLAIMED" {
                if matches!(
                    projected_state,
                    Stage8bP1fPhaseStateV1::Active | Stage8bP1fPhaseStateV1::Stopping
                ) || event.state != Stage8bP1fPhaseStateV1::Active
                {
                    return Err(Stage8bP1fAuthorityErrorV1::HistoryConflict);
                }
                manifests.insert(event.manifest_sha256.clone());
                let retained_manifest_path = authority
                    .join(MANIFESTS_DIRECTORY)
                    .join(&event.manifest_sha256)
                    .join("phase-manifest.json");
                let retained_manifest_bytes = self
                    .read_authority_bytes(&retained_manifest_path, 0o440)
                    .map_err(|_| Stage8bP1fAuthorityErrorV1::HistoryConflict)?;
                if sha256_hex(&retained_manifest_bytes) != event.manifest_sha256 {
                    return Err(Stage8bP1fAuthorityErrorV1::HistoryConflict);
                }
                let retained_manifest: Stage8bP1fPhaseManifestV1 =
                    parse_canonical(&retained_manifest_bytes)
                        .map_err(|_| Stage8bP1fAuthorityErrorV1::HistoryConflict)?;
                let claim = self
                    .read_claim_receipt(&event.manifest_sha256)
                    .map_err(|_| Stage8bP1fAuthorityErrorV1::HistoryConflict)?;
                if claim.schema_version != STAGE8B_P1F_AUTHORITY_SCHEMA_VERSION
                    || claim.domain != "stage8b-p1f-claim-receipt-v1"
                    || claim.state != Stage8bP1fPhaseStateV1::Active
                    || parse_timestamp(&claim.claimed_at_utc).is_err()
                    || parse_timestamp(&claim.deadline_utc).is_err()
                    || retained_manifest.authority_generation != event.authority_generation
                    || retained_manifest.authority_sequence != sequence
                    || retained_manifest.predecessor_event_sha256 != predecessor
                    || retained_manifest.phase != claim.phase
                    || retained_manifest.controller_id != claim.controller_id
                    || retained_manifest.deadline_utc != claim.deadline_utc
                    || retained_manifest.target_host_id != STAGE8B_P1F_TARGET_HOST_ID
                    || retained_manifest.target_host_ssh_ed25519_sha256
                        != STAGE8B_P1F_TARGET_HOST_SSH_ED25519_SHA256
                    || claim.manifest_sha256 != event.manifest_sha256
                    || claim.authority_generation != event.authority_generation
                    || claim.authority_sequence != sequence
                    || claim.predecessor_event_sha256 != predecessor
                    || event.receipt_sha256 != sha256_hex(&canonical_json(&claim)?)
                {
                    return Err(Stage8bP1fAuthorityErrorV1::HistoryConflict);
                }
                projected_state = Stage8bP1fPhaseStateV1::Active;
                projected_active = Some(event.manifest_sha256.clone());
                projected_deadline = Some(claim.deadline_utc.clone());
                projected_force_kill = None;
            } else if event.event_kind == "O2_MATERIALIZED" {
                if projected_state != Stage8bP1fPhaseStateV1::Active
                    || projected_active.as_deref() != Some(event.manifest_sha256.as_str())
                {
                    return Err(Stage8bP1fAuthorityErrorV1::HistoryConflict);
                }
                let materialized_path = authority
                    .join(MANIFESTS_DIRECTORY)
                    .join(&event.manifest_sha256)
                    .join(MATERIALIZED_SET_RECEIPT_FILE);
                let materialized: Stage8bP1fMaterializedSetReceiptV1 = self
                    .read_authority_file(&materialized_path, 0o440)
                    .map_err(|_| Stage8bP1fAuthorityErrorV1::HistoryConflict)?;
                if materialized.schema_version != STAGE8B_P1F_AUTHORITY_SCHEMA_VERSION
                    || materialized.domain != "stage8b-p1f-materialized-set-receipt-v1"
                    || parse_timestamp(&materialized.broker_truth_checked_at_utc).is_err()
                    || parse_timestamp(&materialized.ready_at_utc).is_err()
                    || materialized.authority_sequence != sequence
                    || materialized.predecessor_event_sha256 != predecessor
                    || materialized.manifest_sha256 != event.manifest_sha256
                    || materialized.state != "ReadyForBootstrap"
                    || event.state != Stage8bP1fPhaseStateV1::Active
                    || event.receipt_sha256 != sha256_hex(&canonical_json(&materialized)?)
                {
                    return Err(Stage8bP1fAuthorityErrorV1::HistoryConflict);
                }
                let retained_manifest: Stage8bP1fPhaseManifestV1 = self.read_authority_file(
                    &authority
                        .join(MANIFESTS_DIRECTORY)
                        .join(&event.manifest_sha256)
                        .join("phase-manifest.json"),
                    0o440,
                )?;
                let claim_bytes = self.read_authority_bytes(
                    &authority
                        .join(MANIFESTS_DIRECTORY)
                        .join(&event.manifest_sha256)
                        .join("claim-receipt.json"),
                    0o440,
                )?;
                if retained_manifest.phase != Stage8bP1fPhaseV1::O2MaterializeBootstrap
                    || materialized.materialization_policy_sha256
                        != retained_manifest.materialization_policy_sha256
                    || materialized.config_template_sha256
                        != retained_manifest.config_template_sha256
                    || materialized.installation_sha256 != retained_manifest.installation_sha256
                    || materialized.claim_receipt_sha256 != sha256_hex(&claim_bytes)
                {
                    return Err(Stage8bP1fAuthorityErrorV1::HistoryConflict);
                }
            } else if event.event_kind == "PHASE_STOPPING" {
                if projected_state != Stage8bP1fPhaseStateV1::Active
                    || projected_active.as_deref() != Some(event.manifest_sha256.as_str())
                    || event.state != Stage8bP1fPhaseStateV1::Stopping
                {
                    return Err(Stage8bP1fAuthorityErrorV1::HistoryConflict);
                }
                let stopping: Stage8bP1fStoppingReceiptV1 = self.read_authority_file(
                    &authority
                        .join(MANIFESTS_DIRECTORY)
                        .join(&event.manifest_sha256)
                        .join(format!("stopping-receipt-{sequence:020}.json")),
                    0o440,
                )?;
                let started = parse_timestamp(&stopping.stopping_started_at_utc)?;
                let force = parse_timestamp(&stopping.force_kill_at_utc)?;
                if stopping.schema_version != STAGE8B_P1F_AUTHORITY_SCHEMA_VERSION
                    || stopping.domain != "stage8b-p1f-stopping-receipt-v1"
                    || stopping.authority_generation != event.authority_generation
                    || stopping.authority_sequence != sequence
                    || stopping.predecessor_event_sha256 != predecessor
                    || stopping.manifest_sha256 != event.manifest_sha256
                    || !canonical_token(&stopping.reason_code)
                    || force - started != chrono::Duration::seconds(30)
                    || event.receipt_sha256 != sha256_hex(&canonical_json(&stopping)?)
                {
                    return Err(Stage8bP1fAuthorityErrorV1::HistoryConflict);
                }
                projected_state = Stage8bP1fPhaseStateV1::Stopping;
                projected_force_kill = Some(stopping.force_kill_at_utc);
            } else if event.event_kind == "PHASE_TERMINAL" {
                if !matches!(
                    projected_state,
                    Stage8bP1fPhaseStateV1::Active | Stage8bP1fPhaseStateV1::Stopping
                ) || projected_active.as_deref() != Some(event.manifest_sha256.as_str())
                {
                    return Err(Stage8bP1fAuthorityErrorV1::HistoryConflict);
                }
                let terminal_path = authority
                    .join(MANIFESTS_DIRECTORY)
                    .join(&event.manifest_sha256)
                    .join(format!("terminal-receipt-{sequence:020}.json"));
                let terminal: Stage8bP1fTerminalReceiptV1 = self
                    .read_authority_file(&terminal_path, 0o440)
                    .map_err(|_| Stage8bP1fAuthorityErrorV1::HistoryConflict)?;
                if terminal.schema_version != STAGE8B_P1F_AUTHORITY_SCHEMA_VERSION
                    || terminal.domain != "stage8b-p1f-terminal-receipt-v1"
                    || parse_timestamp(&terminal.recorded_at_utc).is_err()
                    || terminal.authority_sequence != sequence
                    || terminal.predecessor_event_sha256 != predecessor
                    || terminal.manifest_sha256 != event.manifest_sha256
                    || terminal.terminal_state != event.state
                    || event.receipt_sha256 != sha256_hex(&canonical_json(&terminal)?)
                {
                    return Err(Stage8bP1fAuthorityErrorV1::HistoryConflict);
                }
                projected_state = event.state;
                projected_active = None;
                projected_deadline = None;
                projected_force_kill = None;
            } else if event.event_kind == "GENESIS_PREPARED" && sequence == 0 {
                if event.state != Stage8bP1fPhaseStateV1::GenesisPrepared
                    || event.manifest_sha256 != transaction.genesis_manifest_sha256
                    || event.receipt_sha256 != sha256_hex(&canonical_json(&receipt)?)
                {
                    return Err(Stage8bP1fAuthorityErrorV1::HistoryConflict);
                }
            } else {
                return Err(Stage8bP1fAuthorityErrorV1::HistoryConflict);
            }
            latest_hash = event_digest(&bytes);
            predecessor = latest_hash.clone();
        }
        if latest_hash != head.latest_event_sha256 {
            return Err(Stage8bP1fAuthorityErrorV1::HistoryConflict);
        }
        if head.state != projected_state
            || head.active_manifest_sha256 != projected_active
            || head.deadline_utc != projected_deadline
            || head.force_kill_at_utc != projected_force_kill
        {
            return Err(Stage8bP1fAuthorityErrorV1::HistoryConflict);
        }
        let manifest_root = authority.join(MANIFESTS_DIRECTORY);
        let actual_manifests = fs::read_dir(&manifest_root)?
            .map(|entry| {
                entry
                    .map_err(Stage8bP1fAuthorityErrorV1::from)
                    .and_then(|entry| {
                        let name = entry
                            .file_name()
                            .into_string()
                            .map_err(|_| Stage8bP1fAuthorityErrorV1::HistoryConflict)?;
                        self.validate_directory(&entry.path())?;
                        Ok(name)
                    })
            })
            .collect::<Result<BTreeSet<_>, _>>()?;
        if let Some(pending) = pending {
            let pending_directory = manifest_root.join(&pending.manifest_sha256);
            if pending_directory.exists() {
                manifests.insert(pending.manifest_sha256.clone());
            }
        }
        if actual_manifests != manifests {
            return Err(Stage8bP1fAuthorityErrorV1::HistoryConflict);
        }
        if matches!(
            head.state,
            Stage8bP1fPhaseStateV1::Active | Stage8bP1fPhaseStateV1::Stopping
        ) {
            let active = head
                .active_manifest_sha256
                .as_ref()
                .ok_or(Stage8bP1fAuthorityErrorV1::HistoryConflict)?;
            if !manifests.contains(active)
                || head.deadline_utc.is_none()
                || (head.state == Stage8bP1fPhaseStateV1::Stopping)
                    != head.force_kill_at_utc.is_some()
            {
                return Err(Stage8bP1fAuthorityErrorV1::HistoryConflict);
            }
        } else if head.active_manifest_sha256.is_some()
            || head.deadline_utc.is_some()
            || head.force_kill_at_utc.is_some()
        {
            return Err(Stage8bP1fAuthorityErrorV1::HistoryConflict);
        }
        Ok(Stage8bP1fAuthorityInspectionV1 {
            authority_generation: head.authority_generation,
            latest_sequence: head.latest_sequence,
            latest_event_sha256: head.latest_event_sha256,
            state: head.state,
            active_manifest_sha256: head.active_manifest_sha256,
            deadline_utc: head.deadline_utc,
            force_kill_at_utc: head.force_kill_at_utc,
        })
    }

    fn validate_prepared_pending_event(
        &self,
        authority: &Path,
        head: &HistoryHeadV1,
        pending_name: &str,
        pending_path: &Path,
        sequence: u64,
    ) -> Result<bool, Stage8bP1fAuthorityErrorV1> {
        let final_path = event_path(authority, sequence);
        let final_name = final_path
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or(Stage8bP1fAuthorityErrorV1::InvalidPath)?;
        let prepared_path = final_path.with_file_name(format!(".{final_name}.p1f-create"));
        if !prepared_path.exists() {
            return Ok(false);
        }
        if final_path.exists()
            || sequence != head.latest_sequence + 1
            || head.latest_event_sha256.is_empty()
        {
            return Err(Stage8bP1fAuthorityErrorV1::HistoryConflict);
        }
        let event: AuthorityEventV1 = self.read_authority_file(&prepared_path, 0o440)?;
        let expected = match pending_name {
            PENDING_CLAIM_FILE => {
                let pending: PendingClaimV1 = self.read_authority_file(pending_path, 0o440)?;
                let receipt = self.read_claim_receipt(&pending.manifest_sha256)?;
                AuthorityEventV1 {
                    schema_version: STAGE8B_P1F_AUTHORITY_SCHEMA_VERSION,
                    domain: "stage8b-p1f-authority-event-v1".to_string(),
                    authority_generation: receipt.authority_generation,
                    authority_sequence: pending.authority_sequence,
                    predecessor_event_sha256: pending.predecessor_event_sha256,
                    event_kind: "PHASE_CLAIMED".to_string(),
                    state: Stage8bP1fPhaseStateV1::Active,
                    manifest_sha256: pending.manifest_sha256,
                    receipt_sha256: sha256_hex(&canonical_json(&receipt)?),
                    recorded_at_utc: pending.claimed_at_utc,
                }
            }
            PENDING_MATERIALIZATION_FILE => {
                let pending: PendingMaterializationV1 =
                    self.read_authority_file(pending_path, 0o440)?;
                let receipt: Stage8bP1fMaterializedSetReceiptV1 = self.read_authority_file(
                    &authority
                        .join(MANIFESTS_DIRECTORY)
                        .join(&pending.manifest_sha256)
                        .join(MATERIALIZED_SET_RECEIPT_FILE),
                    0o440,
                )?;
                AuthorityEventV1 {
                    schema_version: STAGE8B_P1F_AUTHORITY_SCHEMA_VERSION,
                    domain: "stage8b-p1f-authority-event-v1".to_string(),
                    authority_generation: pending.authority_generation,
                    authority_sequence: pending.authority_sequence,
                    predecessor_event_sha256: pending.predecessor_event_sha256,
                    event_kind: "O2_MATERIALIZED".to_string(),
                    state: Stage8bP1fPhaseStateV1::Active,
                    manifest_sha256: pending.manifest_sha256,
                    receipt_sha256: sha256_hex(&canonical_json(&receipt)?),
                    recorded_at_utc: pending.ready_at_utc,
                }
            }
            PENDING_STOPPING_FILE => {
                let pending: PendingStoppingV1 = self.read_authority_file(pending_path, 0o440)?;
                let receipt: Stage8bP1fStoppingReceiptV1 = self.read_authority_file(
                    &authority
                        .join(MANIFESTS_DIRECTORY)
                        .join(&pending.manifest_sha256)
                        .join(format!(
                            "stopping-receipt-{:020}.json",
                            pending.authority_sequence
                        )),
                    0o440,
                )?;
                AuthorityEventV1 {
                    schema_version: STAGE8B_P1F_AUTHORITY_SCHEMA_VERSION,
                    domain: "stage8b-p1f-authority-event-v1".to_string(),
                    authority_generation: pending.authority_generation,
                    authority_sequence: pending.authority_sequence,
                    predecessor_event_sha256: pending.predecessor_event_sha256,
                    event_kind: "PHASE_STOPPING".to_string(),
                    state: Stage8bP1fPhaseStateV1::Stopping,
                    manifest_sha256: pending.manifest_sha256,
                    receipt_sha256: sha256_hex(&canonical_json(&receipt)?),
                    recorded_at_utc: pending.stopping_started_at_utc,
                }
            }
            PENDING_TERMINAL_FILE => {
                let pending: PendingTerminalV1 = self.read_authority_file(pending_path, 0o440)?;
                let receipt: Stage8bP1fTerminalReceiptV1 = self.read_authority_file(
                    &authority
                        .join(MANIFESTS_DIRECTORY)
                        .join(&pending.manifest_sha256)
                        .join(format!(
                            "terminal-receipt-{:020}.json",
                            pending.authority_sequence
                        )),
                    0o440,
                )?;
                AuthorityEventV1 {
                    schema_version: STAGE8B_P1F_AUTHORITY_SCHEMA_VERSION,
                    domain: "stage8b-p1f-authority-event-v1".to_string(),
                    authority_generation: pending.authority_generation,
                    authority_sequence: pending.authority_sequence,
                    predecessor_event_sha256: pending.predecessor_event_sha256,
                    event_kind: "PHASE_TERMINAL".to_string(),
                    state: pending.terminal_state,
                    manifest_sha256: pending.manifest_sha256,
                    receipt_sha256: sha256_hex(&canonical_json(&receipt)?),
                    recorded_at_utc: pending.recorded_at_utc,
                }
            }
            _ => return Err(Stage8bP1fAuthorityErrorV1::PendingRecoveryRequired),
        };
        if event != expected
            || event.authority_generation != head.authority_generation
            || event.authority_sequence != sequence
            || event.predecessor_event_sha256 != head.latest_event_sha256
        {
            return Err(Stage8bP1fAuthorityErrorV1::HistoryConflict);
        }
        Ok(true)
    }

    fn read_claim_receipt(
        &self,
        manifest_sha256: &str,
    ) -> Result<Stage8bP1fClaimReceiptV1, Stage8bP1fAuthorityErrorV1> {
        self.read_authority_file(
            &self
                .root
                .join(AUTHORITY_DIRECTORY)
                .join(MANIFESTS_DIRECTORY)
                .join(manifest_sha256)
                .join("claim-receipt.json"),
            0o440,
        )
    }

    fn acquire_lease(&self) -> Result<GuardianLease<'_>, Stage8bP1fAuthorityErrorV1> {
        if unsafe { libc::geteuid() } != self.expected_uid {
            return Err(Stage8bP1fAuthorityErrorV1::RootRequired);
        }
        self.validate_root_identity()?;
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(self.root.join(LOCK_FILE))?;
        let metadata = lock.metadata()?;
        if !metadata.file_type().is_file()
            || metadata.nlink() != 1
            || metadata.uid() != self.expected_uid
            || metadata.permissions().mode() & 0o777 != 0o600
        {
            return Err(Stage8bP1fAuthorityErrorV1::InvalidCustody);
        }
        let result = unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
        if result != 0 {
            let error = std::io::Error::last_os_error();
            if matches!(error.raw_os_error(), Some(code) if code == libc::EWOULDBLOCK || code == libc::EAGAIN)
            {
                return Err(Stage8bP1fAuthorityErrorV1::ConcurrentGuardian);
            }
            return Err(error.into());
        }
        Ok(GuardianLease { store: self, lock })
    }

    fn acquire_execution_lock(&self) -> Result<File, Stage8bP1fAuthorityErrorV1> {
        if unsafe { libc::geteuid() } != self.expected_uid {
            return Err(Stage8bP1fAuthorityErrorV1::RootRequired);
        }
        self.validate_root_identity()?;
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(self.root.join(EXECUTION_LOCK_FILE))?;
        let metadata = lock.metadata()?;
        if !metadata.file_type().is_file()
            || metadata.nlink() != 1
            || metadata.uid() != self.expected_uid
            || metadata.permissions().mode() & 0o777 != 0o600
        {
            return Err(Stage8bP1fAuthorityErrorV1::InvalidCustody);
        }
        let result = unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
        if result != 0 {
            let error = std::io::Error::last_os_error();
            if matches!(error.raw_os_error(), Some(code) if code == libc::EWOULDBLOCK || code == libc::EAGAIN)
            {
                return Err(Stage8bP1fAuthorityErrorV1::ConcurrentExecution);
            }
            return Err(error.into());
        }
        Ok(lock)
    }

    fn validate_root_identity(&self) -> Result<(), Stage8bP1fAuthorityErrorV1> {
        let named = fs::symlink_metadata(&self.root)?;
        let opened = self.root_file.metadata()?;
        validate_directory_metadata(&named, self.expected_uid, self.service_gid, 0o750)?;
        validate_directory_metadata(&opened, self.expected_uid, self.service_gid, 0o750)?;
        if named.dev() != self.root_dev
            || named.ino() != self.root_ino
            || opened.dev() != self.root_dev
            || opened.ino() != self.root_ino
        {
            return Err(Stage8bP1fAuthorityErrorV1::InvalidCustody);
        }
        Ok(())
    }

    fn reject_quarantine(&self) -> Result<(), Stage8bP1fAuthorityErrorV1> {
        let path = self.root.join(QUARANTINE_FILE);
        if path.exists() {
            let receipt: Stage8bP1fQuarantineReceiptV1 = self.read_authority_file(&path, 0o440)?;
            if receipt.schema_version != STAGE8B_P1F_AUTHORITY_SCHEMA_VERSION
                || receipt.domain != "stage8b-p1f-restore-quarantine-v1"
                || !canonical_token(&receipt.reason_code)
                || parse_timestamp(&receipt.declared_at_utc).is_err()
                || receipt.rebind_authorized
            {
                return Err(Stage8bP1fAuthorityErrorV1::InvalidDocument);
            }
            return Err(Stage8bP1fAuthorityErrorV1::Quarantined);
        }
        Ok(())
    }

    fn validate_directory(&self, path: &Path) -> Result<(), Stage8bP1fAuthorityErrorV1> {
        let metadata = fs::symlink_metadata(path)?;
        validate_directory_metadata(&metadata, self.expected_uid, self.service_gid, 0o750)
    }

    fn create_authority_directory(
        &self,
        path: &Path,
        mode: u32,
    ) -> Result<(), Stage8bP1fAuthorityErrorV1> {
        fs::DirBuilder::new().mode(mode).create(path)?;
        let path_c = CString::new(path.as_os_str().as_bytes())
            .map_err(|_| Stage8bP1fAuthorityErrorV1::InvalidPath)?;
        if unsafe { libc::chown(path_c.as_ptr(), self.expected_uid, self.service_gid) } != 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        fs::set_permissions(path, fs::Permissions::from_mode(mode))?;
        self.validate_directory_at(path, mode)?;
        sync_directory(
            path.parent()
                .ok_or(Stage8bP1fAuthorityErrorV1::InvalidPath)?,
        )?;
        Ok(())
    }

    fn validate_directory_at(
        &self,
        path: &Path,
        mode: u32,
    ) -> Result<(), Stage8bP1fAuthorityErrorV1> {
        let metadata = fs::symlink_metadata(path)?;
        validate_directory_metadata(&metadata, self.expected_uid, self.service_gid, mode)
    }

    fn read_external_bytes(
        &self,
        path: &Path,
        maximum_bytes: u64,
    ) -> Result<Vec<u8>, Stage8bP1fAuthorityErrorV1> {
        let before = fs::symlink_metadata(path)?;
        validate_file_metadata(&before, self.expected_uid, self.service_gid, 0o440)?;
        let mut file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK)
            .open(path)?;
        let opened = file.metadata()?;
        validate_file_metadata(&opened, self.expected_uid, self.service_gid, 0o440)?;
        if opened.len() > maximum_bytes
            || before.dev() != opened.dev()
            || before.ino() != opened.ino()
        {
            return Err(Stage8bP1fAuthorityErrorV1::InvalidCustody);
        }
        let mut bytes = Vec::with_capacity(opened.len() as usize);
        (&mut file)
            .take(maximum_bytes + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() as u64 != opened.len() {
            return Err(Stage8bP1fAuthorityErrorV1::InvalidDocument);
        }
        Ok(bytes)
    }

    fn write_external_or_require_exact(
        &self,
        path: &Path,
        bytes: &[u8],
        maximum_bytes: u64,
    ) -> Result<(), Stage8bP1fAuthorityErrorV1> {
        if bytes.is_empty() || bytes.len() as u64 > maximum_bytes {
            return Err(Stage8bP1fAuthorityErrorV1::InvalidDocument);
        }
        match fs::symlink_metadata(path) {
            Ok(_) if self.read_external_bytes(path, maximum_bytes)? == bytes => return Ok(()),
            Ok(_) => return Err(Stage8bP1fAuthorityErrorV1::HistoryConflict),
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        let parent = path
            .parent()
            .ok_or(Stage8bP1fAuthorityErrorV1::InvalidPath)?;
        self.validate_directory_at(parent, 0o750)?;
        let name = path
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or(Stage8bP1fAuthorityErrorV1::InvalidPath)?;
        let temporary = parent.join(format!(".{name}.p1f-next"));
        if temporary.exists() {
            if self.read_external_bytes(&temporary, maximum_bytes)? != bytes || path.exists() {
                return Err(Stage8bP1fAuthorityErrorV1::HistoryConflict);
            }
            fs::rename(&temporary, path)?;
            sync_directory(parent)?;
            return if self.read_external_bytes(path, maximum_bytes)? == bytes {
                Ok(())
            } else {
                Err(Stage8bP1fAuthorityErrorV1::HistoryConflict)
            };
        }
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o440)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(&temporary)?;
        if unsafe { libc::fchown(file.as_raw_fd(), self.expected_uid, self.service_gid) } != 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        file.write_all(bytes)?;
        file.sync_all()?;
        inject_test_fault(3)?;
        validate_file_metadata(
            &file.metadata()?,
            self.expected_uid,
            self.service_gid,
            0o440,
        )?;
        if path.exists() {
            fs::remove_file(&temporary)?;
            return Err(Stage8bP1fAuthorityErrorV1::HistoryConflict);
        }
        fs::rename(&temporary, path)?;
        sync_directory(parent)?;
        if self.read_external_bytes(path, maximum_bytes)? != bytes {
            return Err(Stage8bP1fAuthorityErrorV1::InvalidDocument);
        }
        Ok(())
    }

    fn read_authority_file<T: DeserializeOwned + Serialize>(
        &self,
        path: &Path,
        mode: u32,
    ) -> Result<T, Stage8bP1fAuthorityErrorV1> {
        parse_canonical(&self.read_authority_bytes(path, mode)?)
    }

    fn read_authority_bytes(
        &self,
        path: &Path,
        mode: u32,
    ) -> Result<Vec<u8>, Stage8bP1fAuthorityErrorV1> {
        let before = fs::symlink_metadata(path)?;
        validate_file_metadata(&before, self.expected_uid, self.service_gid, mode)?;
        let mut file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK)
            .open(path)?;
        let opened = file.metadata()?;
        validate_file_metadata(&opened, self.expected_uid, self.service_gid, mode)?;
        if before.dev() != opened.dev() || before.ino() != opened.ino() {
            return Err(Stage8bP1fAuthorityErrorV1::InvalidCustody);
        }
        if opened.len() > STAGE8B_P1F_MAX_AUTHORITY_BYTES {
            return Err(Stage8bP1fAuthorityErrorV1::InvalidDocument);
        }
        let mut bytes = Vec::with_capacity(opened.len() as usize);
        (&mut file)
            .take(STAGE8B_P1F_MAX_AUTHORITY_BYTES + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() as u64 != opened.len() {
            return Err(Stage8bP1fAuthorityErrorV1::InvalidDocument);
        }
        Ok(bytes)
    }

    fn write_create_new(
        &self,
        path: &Path,
        bytes: &[u8],
        mode: u32,
    ) -> Result<(), Stage8bP1fAuthorityErrorV1> {
        if bytes.len() as u64 > STAGE8B_P1F_MAX_AUTHORITY_BYTES {
            return Err(Stage8bP1fAuthorityErrorV1::InvalidDocument);
        }
        match fs::symlink_metadata(path) {
            Ok(_) if self.read_authority_bytes(path, mode)? == bytes => return Ok(()),
            Ok(_) => return Err(Stage8bP1fAuthorityErrorV1::HistoryConflict),
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        let parent = path
            .parent()
            .ok_or(Stage8bP1fAuthorityErrorV1::InvalidPath)?;
        let name = path
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or(Stage8bP1fAuthorityErrorV1::InvalidPath)?;
        let temporary = parent.join(format!(".{name}.p1f-create"));
        if temporary.exists() {
            if self.read_authority_bytes(&temporary, mode)? != bytes || path.exists() {
                return Err(Stage8bP1fAuthorityErrorV1::HistoryConflict);
            }
            fs::rename(&temporary, path)?;
            sync_directory(parent)?;
            return if self.read_authority_bytes(path, mode)? == bytes {
                Ok(())
            } else {
                Err(Stage8bP1fAuthorityErrorV1::HistoryConflict)
            };
        }
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(mode)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(&temporary)?;
        // Creation mode is filtered by the service umask.  Custody requires
        // the reviewed mode itself, so establish it explicitly before the
        // file can be validated or published.
        if unsafe { libc::fchmod(file.as_raw_fd(), mode as libc::mode_t) } != 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        if unsafe { libc::fchown(file.as_raw_fd(), self.expected_uid, self.service_gid) } != 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        file.write_all(bytes)?;
        file.sync_all()?;
        if path
            .parent()
            .and_then(Path::file_name)
            .is_some_and(|name| name == EVENTS_DIRECTORY)
        {
            inject_test_fault(5)?;
        }
        inject_test_fault(2)?;
        validate_file_metadata(&file.metadata()?, self.expected_uid, self.service_gid, mode)?;
        fs::rename(&temporary, path)?;
        sync_directory(parent)?;
        if self.read_authority_bytes(path, mode)? != bytes {
            return Err(Stage8bP1fAuthorityErrorV1::InvalidDocument);
        }
        Ok(())
    }

    fn write_or_require_exact(
        &self,
        path: &Path,
        bytes: &[u8],
        mode: u32,
    ) -> Result<(), Stage8bP1fAuthorityErrorV1> {
        match fs::symlink_metadata(path) {
            Ok(_) if self.read_authority_bytes(path, mode)? == bytes => Ok(()),
            Ok(_) => Err(Stage8bP1fAuthorityErrorV1::HistoryConflict),
            Err(error) if error.kind() == ErrorKind::NotFound => {
                self.write_create_new(path, bytes, mode)
            }
            Err(error) => Err(error.into()),
        }
    }

    fn replace_exact(
        &self,
        path: &Path,
        bytes: &[u8],
        mode: u32,
    ) -> Result<(), Stage8bP1fAuthorityErrorV1> {
        let parent = path
            .parent()
            .ok_or(Stage8bP1fAuthorityErrorV1::InvalidPath)?;
        let name = path
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or(Stage8bP1fAuthorityErrorV1::InvalidPath)?;
        let temp = parent.join(format!(".{name}.next"));
        if temp.exists() {
            if self.read_authority_bytes(&temp, mode)? != bytes {
                return Err(Stage8bP1fAuthorityErrorV1::HistoryConflict);
            }
        } else {
            self.write_create_new(&temp, bytes, mode)?;
        }
        fs::rename(&temp, path)?;
        sync_directory(parent)?;
        if self.read_authority_bytes(path, mode)? != bytes {
            return Err(Stage8bP1fAuthorityErrorV1::HistoryConflict);
        }
        Ok(())
    }
}

impl Drop for GuardianLease<'_> {
    fn drop(&mut self) {
        let _ = self.store.validate_root_identity();
        let _ = unsafe { libc::flock(self.lock.as_raw_fd(), libc::LOCK_UN) };
    }
}

/// Executes only descriptor-relative writes below already opened target roots.
/// No path-based mutation callback is exposed: every selector is resolved with
/// `openat(O_NOFOLLOW)`, retained for the whole operation, and the trusted
/// control root is excluded before the first write.
pub fn execute_stage8b_p1f_permitted_restore_v1(
    plan: &Stage8bP1fRestorePlanV1,
    writes: &[Stage8bP1fRestoreWriteV1],
) -> Result<(), Stage8bP1fAuthorityErrorV1> {
    execute_restore_at(
        plan,
        writes,
        Path::new(STAGE8B_P1F_AUTHORITY_CONTROL_ROOT),
        || Ok(()),
    )
}

fn execute_restore_at<F>(
    plan: &Stage8bP1fRestorePlanV1,
    writes: &[Stage8bP1fRestoreWriteV1],
    control_root: &Path,
    before_write: F,
) -> Result<(), Stage8bP1fAuthorityErrorV1>
where
    F: FnOnce() -> Result<(), Stage8bP1fAuthorityErrorV1>,
{
    let control = normalize_absolute(control_root)?;
    for path in plan
        .source_roots
        .iter()
        .chain(&plan.target_roots)
        .chain(&plan.selected_paths)
    {
        let candidate = normalize_absolute(path)?;
        if candidate.starts_with(&control) || control.starts_with(&candidate) {
            return Err(Stage8bP1fAuthorityErrorV1::RestoreOverlapsControlRoot);
        }
    }
    let mut source_guards = Vec::new();
    let mut target_guards = Vec::new();
    let mut selected_guards = Vec::new();
    for path in &plan.source_roots {
        let candidate = normalize_absolute(path)?;
        if candidate.starts_with(&control) || control.starts_with(&candidate) {
            return Err(Stage8bP1fAuthorityErrorV1::RestoreOverlapsControlRoot);
        }
        source_guards.push(open_restore_selector(&candidate, false)?);
    }
    for path in &plan.target_roots {
        let candidate = normalize_absolute(path)?;
        if candidate.starts_with(&control) || control.starts_with(&candidate) {
            return Err(Stage8bP1fAuthorityErrorV1::RestoreOverlapsControlRoot);
        }
        target_guards.push(open_restore_selector(&candidate, true)?);
    }
    for path in &plan.selected_paths {
        let candidate = normalize_absolute(path)?;
        if candidate.starts_with(&control) || control.starts_with(&candidate) {
            return Err(Stage8bP1fAuthorityErrorV1::RestoreOverlapsControlRoot);
        }
        selected_guards.push(open_restore_selector(&candidate, false)?);
    }
    before_write()?;
    for write in writes {
        let target = target_guards
            .get(write.target_root_index)
            .ok_or(Stage8bP1fAuthorityErrorV1::InvalidPath)?;
        write_restore_file_at(target, write)?;
    }
    drop((source_guards, selected_guards));
    Ok(())
}

fn open_restore_selector(
    candidate: &Path,
    require_directory: bool,
) -> Result<File, Stage8bP1fAuthorityErrorV1> {
    let components = candidate
        .components()
        .filter_map(|component| match component {
            Component::Normal(value) => Some(value.to_os_string()),
            _ => None,
        })
        .collect::<Vec<_>>();
    let mut current = open_directory(Path::new("/"))?;
    for (index, component) in components.iter().enumerate() {
        let require_component_directory = index + 1 < components.len() || require_directory;
        current =
            openat_restore_component(&current, component.as_os_str(), require_component_directory)?;
    }
    Ok(current)
}

fn openat_restore_component(
    parent: &File,
    name: &std::ffi::OsStr,
    require_directory: bool,
) -> Result<File, Stage8bP1fAuthorityErrorV1> {
    let name =
        CString::new(name.as_bytes()).map_err(|_| Stage8bP1fAuthorityErrorV1::InvalidPath)?;
    let mut flags = libc::O_RDONLY | libc::O_CLOEXEC | libc::O_NOFOLLOW | libc::O_NONBLOCK;
    if require_directory {
        flags |= libc::O_DIRECTORY;
    }
    let fd = unsafe { libc::openat(parent.as_raw_fd(), name.as_ptr(), flags) };
    if fd < 0 {
        let error = std::io::Error::last_os_error();
        if matches!(error.raw_os_error(), Some(code) if code == libc::ELOOP || code == libc::ENOTDIR)
        {
            return Err(Stage8bP1fAuthorityErrorV1::RestoreOverlapsControlRoot);
        }
        return Err(error.into());
    }
    let file = unsafe { File::from_raw_fd(fd) };
    let metadata = file.metadata()?;
    if require_directory && !metadata.is_dir() {
        return Err(Stage8bP1fAuthorityErrorV1::InvalidPath);
    }
    Ok(file)
}

fn write_restore_file_at(
    target_root: &File,
    write: &Stage8bP1fRestoreWriteV1,
) -> Result<(), Stage8bP1fAuthorityErrorV1> {
    if write.bytes.len() as u64 > STAGE8B_P1F_MAX_AUTHORITY_BYTES
        || write.mode & !0o660 != 0
        || write.mode == 0
    {
        return Err(Stage8bP1fAuthorityErrorV1::InvalidDocument);
    }
    let components = normalize_relative(&write.relative_path)?;
    let (leaf, parents) = components
        .split_last()
        .ok_or(Stage8bP1fAuthorityErrorV1::InvalidPath)?;
    let mut directory = target_root.try_clone()?;
    for component in parents {
        directory = openat_restore_component(&directory, component.as_os_str(), true)?;
    }
    let leaf =
        CString::new(leaf.as_bytes()).map_err(|_| Stage8bP1fAuthorityErrorV1::InvalidPath)?;
    let existing_fd = unsafe {
        libc::openat(
            directory.as_raw_fd(),
            leaf.as_ptr(),
            libc::O_WRONLY | libc::O_CLOEXEC | libc::O_NOFOLLOW | libc::O_NONBLOCK,
        )
    };
    let mut file = if existing_fd >= 0 {
        let file = unsafe { File::from_raw_fd(existing_fd) };
        let metadata = file.metadata()?;
        if !metadata.file_type().is_file() || metadata.nlink() != 1 {
            return Err(Stage8bP1fAuthorityErrorV1::InvalidCustody);
        }
        file.set_len(0)?;
        file
    } else {
        let error = std::io::Error::last_os_error();
        if error.kind() != ErrorKind::NotFound {
            if matches!(error.raw_os_error(), Some(code) if code == libc::ELOOP) {
                return Err(Stage8bP1fAuthorityErrorV1::RestoreOverlapsControlRoot);
            }
            return Err(error.into());
        }
        let fd = unsafe {
            libc::openat(
                directory.as_raw_fd(),
                leaf.as_ptr(),
                libc::O_WRONLY | libc::O_CLOEXEC | libc::O_NOFOLLOW | libc::O_CREAT | libc::O_EXCL,
                write.mode,
            )
        };
        if fd < 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        unsafe { File::from_raw_fd(fd) }
    };
    file.set_permissions(fs::Permissions::from_mode(write.mode))?;
    file.write_all(&write.bytes)?;
    file.sync_all()?;
    directory.sync_all()?;
    Ok(())
}

fn normalize_relative(path: &Path) -> Result<Vec<std::ffi::OsString>, Stage8bP1fAuthorityErrorV1> {
    if path.is_absolute() {
        return Err(Stage8bP1fAuthorityErrorV1::InvalidPath);
    }
    let mut components = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(value) => components.push(value.to_os_string()),
            Component::CurDir
            | Component::ParentDir
            | Component::RootDir
            | Component::Prefix(_) => return Err(Stage8bP1fAuthorityErrorV1::InvalidPath),
        }
    }
    Ok(components)
}

fn normalize_absolute(path: &Path) -> Result<PathBuf, Stage8bP1fAuthorityErrorV1> {
    if !path.is_absolute() {
        return Err(Stage8bP1fAuthorityErrorV1::InvalidPath);
    }
    let mut normalized = PathBuf::from("/");
    for component in path.components() {
        match component {
            Component::RootDir => {}
            Component::Normal(value) => normalized.push(value),
            Component::CurDir | Component::ParentDir | Component::Prefix(_) => {
                return Err(Stage8bP1fAuthorityErrorV1::InvalidPath)
            }
        }
    }
    Ok(normalized)
}

fn current_boot_id() -> Result<String, Stage8bP1fAuthorityErrorV1> {
    match fs::read_to_string("/proc/sys/kernel/random/boot_id") {
        Ok(value) => {
            let value = value.trim().to_string();
            if canonical_token(&value) {
                Ok(value)
            } else {
                Err(Stage8bP1fAuthorityErrorV1::InvalidDocument)
            }
        }
        Err(error) => {
            #[cfg(any(test, feature = "stage8b-p1-test-fixtures"))]
            {
                let _ = error;
                Ok("test-boot-id".to_string())
            }
            #[cfg(not(any(test, feature = "stage8b-p1-test-fixtures")))]
            {
                Err(error.into())
            }
        }
    }
}

fn read_signed_document<T, U, K, S, C>(
    bytes: &[u8],
    domain: &[u8],
    expected_public_key_hex: &str,
    expected_key_id: &str,
    key_id: K,
    signature: S,
    unsigned: C,
) -> Result<T, Stage8bP1fAuthorityErrorV1>
where
    T: DeserializeOwned + Serialize,
    U: Serialize,
    K: FnOnce(&T) -> &str,
    S: FnOnce(&T) -> &str,
    C: FnOnce(&T) -> U,
{
    let value: T = parse_canonical(bytes)?;
    if key_id(&value) != expected_key_id {
        return Err(Stage8bP1fAuthorityErrorV1::InvalidSignature);
    }
    let signature = decode_fixed_hex::<64>(signature(&value))?;
    let public_key = decode_fixed_hex::<32>(expected_public_key_hex)?;
    let verifying_key = VerifyingKey::from_bytes(&public_key)
        .map_err(|_| Stage8bP1fAuthorityErrorV1::InvalidSignature)?;
    let mut signed = domain.to_vec();
    signed.extend(canonical_json(&unsigned(&value))?);
    verifying_key
        .verify(&signed, &Signature::from_bytes(&signature))
        .map_err(|_| Stage8bP1fAuthorityErrorV1::InvalidSignature)?;
    Ok(value)
}

/// Returns the lowercase Ed25519 public identity corresponding to an offline
/// authority key. The secret key is never serialized by this API.
pub fn stage8b_p1f_authority_public_key_hex(signing_key: &SigningKey) -> String {
    lower_hex(signing_key.verifying_key().as_bytes())
}

/// Canonically signs one genesis manifest for the accepted guardian domain.
/// Existing signature bytes are always discarded before signing.
pub fn sign_stage8b_p1f_genesis_manifest_v1(
    mut document: Stage8bP1fGenesisManifestV1,
    signing_key: &SigningKey,
) -> Result<Vec<u8>, Stage8bP1fAuthorityErrorV1> {
    document.signature_ed25519_hex.clear();
    if document.schema_version != STAGE8B_P1F_AUTHORITY_SCHEMA_VERSION
        || document.domain != "stage8b-p1f-genesis-manifest-v1"
        || document.genesis_head_sha256 != genesis_head_sha256(&document)
        || !canonical_token(&document.issuer_key_id)
    {
        return Err(Stage8bP1fAuthorityErrorV1::InvalidDocument);
    }
    sign_authority_document(
        SIGNED_GENESIS_DOMAIN,
        &mut document,
        signing_key,
        |value, signature| {
            value.signature_ed25519_hex = signature;
        },
    )
}

/// Canonically signs one activation certificate for the accepted guardian
/// domain. The caller must bind the exact retained genesis receipt hashes.
pub fn sign_stage8b_p1f_activation_certificate_v1(
    mut document: Stage8bP1fActivationCertificateV1,
    signing_key: &SigningKey,
) -> Result<Vec<u8>, Stage8bP1fAuthorityErrorV1> {
    document.signature_ed25519_hex.clear();
    if document.schema_version != STAGE8B_P1F_AUTHORITY_SCHEMA_VERSION
        || document.domain != "stage8b-p1f-activation-certificate-v1"
        || !canonical_token(&document.issuer_key_id)
    {
        return Err(Stage8bP1fAuthorityErrorV1::InvalidDocument);
    }
    sign_authority_document(
        SIGNED_ACTIVATION_DOMAIN,
        &mut document,
        signing_key,
        |value, signature| value.signature_ed25519_hex = signature,
    )
}

/// Canonically signs one phase manifest for the accepted guardian domain.
pub fn sign_stage8b_p1f_phase_manifest_v1(
    mut document: Stage8bP1fPhaseManifestV1,
    signing_key: &SigningKey,
) -> Result<Vec<u8>, Stage8bP1fAuthorityErrorV1> {
    document.signature_ed25519_hex.clear();
    if document.schema_version != STAGE8B_P1F_AUTHORITY_SCHEMA_VERSION
        || document.domain != "stage8b-p1f-phase-manifest-v1"
        || !canonical_token(&document.issuer_key_id)
    {
        return Err(Stage8bP1fAuthorityErrorV1::InvalidDocument);
    }
    sign_authority_document(
        SIGNED_PHASE_DOMAIN,
        &mut document,
        signing_key,
        |value, signature| {
            value.signature_ed25519_hex = signature;
        },
    )
}

fn sign_authority_document<T, F>(
    domain: &[u8],
    value: &mut T,
    signing_key: &SigningKey,
    set_signature: F,
) -> Result<Vec<u8>, Stage8bP1fAuthorityErrorV1>
where
    T: Serialize,
    F: FnOnce(&mut T, String),
{
    let mut payload = domain.to_vec();
    payload.extend(canonical_json(value)?);
    set_signature(value, lower_hex(&signing_key.sign(&payload).to_bytes()));
    canonical_json(value)
}

fn genesis_head_sha256(manifest: &Stage8bP1fGenesisManifestV1) -> String {
    let mut digest = Sha256::new();
    digest.update(GENESIS_DOMAIN);
    digest.update(manifest.installation_id.as_bytes());
    digest.update([0]);
    digest.update(manifest.target_host_id.as_bytes());
    digest.update([0]);
    digest.update(manifest.control_root.as_bytes());
    digest.update([0]);
    digest.update(manifest.authority_generation.to_be_bytes());
    digest.update(manifest.ceremony_nonce_sha256.as_bytes());
    lower_hex(&digest.finalize())
}

fn event_digest(bytes: &[u8]) -> String {
    let mut digest = Sha256::new();
    digest.update(EVENT_DOMAIN);
    digest.update(bytes);
    lower_hex(&digest.finalize())
}

fn event_path(authority: &Path, sequence: u64) -> PathBuf {
    authority
        .join(EVENTS_DIRECTORY)
        .join(format!("{sequence:020}.json"))
}

fn validate_window(
    start: &str,
    end: &str,
    trusted_now: DateTime<Utc>,
) -> Result<(), Stage8bP1fAuthorityErrorV1> {
    let start = parse_timestamp(start)?;
    let end = parse_timestamp(end)?;
    if end <= start || trusted_now < start || trusted_now >= end {
        return Err(Stage8bP1fAuthorityErrorV1::InvalidValidityWindow);
    }
    Ok(())
}

fn validate_not_expired(
    deadline: &str,
    trusted_now: DateTime<Utc>,
) -> Result<(), Stage8bP1fAuthorityErrorV1> {
    if trusted_now >= parse_timestamp(deadline)? {
        Err(Stage8bP1fAuthorityErrorV1::DeadlineExpired)
    } else {
        Ok(())
    }
}

fn parse_timestamp(value: &str) -> Result<DateTime<Utc>, Stage8bP1fAuthorityErrorV1> {
    let parsed = DateTime::parse_from_rfc3339(value)
        .map_err(|_| Stage8bP1fAuthorityErrorV1::InvalidValidityWindow)?
        .with_timezone(&Utc);
    if canonical_timestamp(parsed) != value {
        return Err(Stage8bP1fAuthorityErrorV1::InvalidValidityWindow);
    }
    Ok(parsed)
}

fn canonical_timestamp(value: DateTime<Utc>) -> String {
    value.to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn parse_canonical<T: DeserializeOwned + Serialize>(
    bytes: &[u8],
) -> Result<T, Stage8bP1fAuthorityErrorV1> {
    if bytes.len() as u64 > STAGE8B_P1F_MAX_AUTHORITY_BYTES {
        return Err(Stage8bP1fAuthorityErrorV1::InvalidDocument);
    }
    let value: T =
        serde_json::from_slice(bytes).map_err(|_| Stage8bP1fAuthorityErrorV1::InvalidDocument)?;
    if canonical_json(&value)? != bytes {
        return Err(Stage8bP1fAuthorityErrorV1::InvalidDocument);
    }
    Ok(value)
}

fn canonical_json<T: Serialize>(value: &T) -> Result<Vec<u8>, Stage8bP1fAuthorityErrorV1> {
    serde_json::to_vec(value).map_err(|_| Stage8bP1fAuthorityErrorV1::InvalidDocument)
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn canonical_token(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

fn sha256_hex(bytes: &[u8]) -> String {
    lower_hex(&Sha256::digest(bytes))
}

fn lower_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    output
}

fn decode_fixed_hex<const N: usize>(value: &str) -> Result<[u8; N], Stage8bP1fAuthorityErrorV1> {
    if value.len() != N * 2 || value.bytes().any(|byte| !byte.is_ascii_hexdigit()) {
        return Err(Stage8bP1fAuthorityErrorV1::InvalidSignature);
    }
    let mut output = [0u8; N];
    for (index, slot) in output.iter_mut().enumerate() {
        let offset = index * 2;
        let byte = u8::from_str_radix(&value[offset..offset + 2], 16)
            .map_err(|_| Stage8bP1fAuthorityErrorV1::InvalidSignature)?;
        *slot = byte;
    }
    if lower_hex(&output) != value {
        return Err(Stage8bP1fAuthorityErrorV1::InvalidSignature);
    }
    Ok(output)
}

fn validate_directory_metadata(
    metadata: &fs::Metadata,
    uid: u32,
    gid: u32,
    mode: u32,
) -> Result<(), Stage8bP1fAuthorityErrorV1> {
    if !metadata.file_type().is_dir()
        || metadata.file_type().is_symlink()
        || metadata.uid() != uid
        || metadata.gid() != gid
        || metadata.permissions().mode() & 0o777 != mode
    {
        return Err(Stage8bP1fAuthorityErrorV1::InvalidCustody);
    }
    Ok(())
}

fn validate_file_metadata(
    metadata: &fs::Metadata,
    uid: u32,
    gid: u32,
    mode: u32,
) -> Result<(), Stage8bP1fAuthorityErrorV1> {
    if !metadata.file_type().is_file()
        || metadata.file_type().is_symlink()
        || metadata.nlink() != 1
        || metadata.uid() != uid
        || metadata.gid() != gid
        || metadata.permissions().mode() & 0o777 != mode
    {
        return Err(Stage8bP1fAuthorityErrorV1::InvalidCustody);
    }
    Ok(())
}

fn sync_directory(path: &Path) -> Result<(), Stage8bP1fAuthorityErrorV1> {
    open_directory(path)?.sync_all()?;
    Ok(())
}

fn open_directory(path: &Path) -> Result<File, Stage8bP1fAuthorityErrorV1> {
    let path = CString::new(path.as_os_str().as_bytes())
        .map_err(|_| Stage8bP1fAuthorityErrorV1::InvalidPath)?;
    let fd = unsafe {
        libc::open(
            path.as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    };
    if fd < 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(unsafe { File::from_raw_fd(fd) })
}

/// Isolated O2/O3 authority used only by the P1F-Ie linked-composition
/// witness. The fixture calls the production guardian transactions and keeps
/// every generated artifact outside the operational paths.
#[cfg(feature = "stage8b-p1-test-fixtures")]
pub struct Stage8bP1fIeLinkedFixtureV1 {
    root: PathBuf,
    config_root: PathBuf,
    durable_parent: PathBuf,
    store: Stage8bP1fAuthorityStoreV1,
    signing: SigningKey,
    operational_identity_sha256: String,
    runtime_config_fingerprint_sha256: String,
    source_sha256: String,
    config_sha256: String,
    candidate_close_ts_utc_ms: i64,
}

#[cfg(feature = "stage8b-p1-test-fixtures")]
impl Stage8bP1fIeLinkedFixtureV1 {
    pub fn materialize() -> Result<Self, Stage8bP1fAuthorityErrorV1> {
        use std::sync::atomic::{AtomicU64, Ordering};
        use std::time::{SystemTime, UNIX_EPOCH};

        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| Stage8bP1fAuthorityErrorV1::InvalidDocument)?
            .as_nanos();
        let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let scratch = std::env::current_dir()?
            .join("target/p1f-ie-linked")
            .join(format!("{}-{unique}-{sequence}", std::process::id()));
        fs::create_dir_all(&scratch)?;
        let root = scratch.join("authority-root");
        let config_root = scratch.join("config");
        let bootstrap_dir = config_root.join("bootstrap");
        let durable_parent = scratch.join("durable");
        fs::DirBuilder::new().mode(0o750).create(&root)?;
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o750)
            .create(&bootstrap_dir)?;
        fs::DirBuilder::new().mode(0o755).create(&durable_parent)?;
        fs::set_permissions(&config_root, fs::Permissions::from_mode(0o750))?;
        fs::set_permissions(&bootstrap_dir, fs::Permissions::from_mode(0o750))?;
        let root = fs::canonicalize(root)?;
        let config_root = fs::canonicalize(config_root)?;
        let durable_parent = fs::canonicalize(durable_parent)?;
        let uid = unsafe { libc::geteuid() };
        let gid = unsafe { libc::getegid() };
        let store = Stage8bP1fAuthorityStoreV1::open_at(&root, uid, gid)?;
        let signing = SigningKey::from_bytes(&[0x57; 32]);
        let (_, runtime_config_fingerprint_sha256) =
            crate::Stage8bP1RuntimeProfileV1::build_hybrid_runtime()
                .map_err(|_| Stage8bP1fAuthorityErrorV1::InvalidDocument)?;
        let bootstrap = crate::Stage8bP1BootstrapConfig {
            schema_version: 1,
            broker_id: crate::STAGE8B_P1_BROKER_ID.into(),
            strategy_id: crate::STAGE8B_P1_STRATEGY_ID.into(),
            account_id: "ACC_TEST_0001".into(),
            internal_symbol: crate::STAGE8B_P1_INTERNAL_SYMBOL.into(),
            venue_symbol: crate::STAGE8B_P1_VENUE_SYMBOL.into(),
            exchange: crate::STAGE8B_P1_EXCHANGE.into(),
            market: crate::STAGE8B_P1_MARKET.into(),
            tick_size: crate::STAGE8B_P1_TICK_SIZE.into(),
            runtime_config_fingerprint_sha256: runtime_config_fingerprint_sha256.clone(),
            instrument_map_fingerprint_sha256:
                crate::stage8b_p1_imoexf_instrument_map_fingerprint_sha256(),
            deployment_id: "finam-imoexf-paper-p1".into(),
            deployment_generation: 1,
            gateway_instance_id: "finam-imoexf-paper-gateway-1".into(),
            market_data_generation: 1,
            command_consumer_generation: 1,
            stage8a4_writer_issuer_public_key_hex: "8".repeat(64),
            durable_parent: durable_parent.clone(),
        };
        let validated = crate::validate_stage8b_p1_bootstrap_config(bootstrap)
            .map_err(|_| Stage8bP1fAuthorityErrorV1::InvalidDocument)?;
        let operational_identity_sha256 = validated.operational_identity_sha256().to_string();
        let (source, trusted_now, broker_truth_checked_at_utc) =
            crate::stage8b_p1e_first_boot_source::stage8b_p1f_ie_first_boot_source_fixture_v1(
                &operational_identity_sha256,
                "ACC_TEST_0001",
            );
        let source_value: Value = serde_json::from_slice(&source)
            .map_err(|_| Stage8bP1fAuthorityErrorV1::InvalidDocument)?;
        let candidate_close_ts_utc_ms = source_value["candidate"]["close_ts_utc_ms"]
            .as_i64()
            .ok_or(Stage8bP1fAuthorityErrorV1::InvalidDocument)?;
        let template = canonical_json(&serde_json::json!({
            "schema_version": 1,
            "runtime_profile_id": crate::STAGE8B_P1E_RUNTIME_PROFILE_ID,
            "runtime_profile_sha256": crate::STAGE8B_P1E_RUNTIME_PROFILE_SHA256,
            "first_boot_source_bundle_sha256": STAGE8B_P1F_SOURCE_SHA256_TEMPLATE_SENTINEL,
            "schedule_registry_version": "imoexf-v1",
            "schedule_registry_identity_sha256": "6".repeat(64),
            "redis_url": crate::STAGE8B_P1E_REDIS_URL_IPV4,
            "redis_deployment_manifest_sha256": "7".repeat(64),
            "redis_runtime_policy_id": crate::STAGE8B_P1E_REDIS_RUNTIME_POLICY_ID,
            "redis_runtime_policy_sha256": crate::STAGE8B_P1E_REDIS_RUNTIME_POLICY_SHA256,
            "telemetry_contract_sha256": crate::STAGE8B_P1E_TELEMETRY_CONTRACT_SHA256,
            "health_interval_ms": 1000,
            "shutdown_grace_ms": 30000,
            "bootstrap": {
                "schema_version": 1,
                "broker_id": crate::STAGE8B_P1_BROKER_ID,
                "strategy_id": crate::STAGE8B_P1_STRATEGY_ID,
                "account_id": "ACC_TEST_0001",
                "internal_symbol": crate::STAGE8B_P1_INTERNAL_SYMBOL,
                "venue_symbol": crate::STAGE8B_P1_VENUE_SYMBOL,
                "exchange": crate::STAGE8B_P1_EXCHANGE,
                "market": crate::STAGE8B_P1_MARKET,
                "tick_size": crate::STAGE8B_P1_TICK_SIZE,
                "runtime_config_fingerprint_sha256": runtime_config_fingerprint_sha256,
                "instrument_map_fingerprint_sha256": crate::stage8b_p1_imoexf_instrument_map_fingerprint_sha256(),
                "deployment_id": "finam-imoexf-paper-p1",
                "deployment_generation": 1,
                "gateway_instance_id": "finam-imoexf-paper-gateway-1",
                "market_data_generation": 1,
                "command_consumer_generation": 1,
                "stage8a4_writer_issuer_public_key_hex": "8".repeat(64),
                "durable_parent": durable_parent
            }
        }))?;
        let policy = canonical_json(&serde_json::json!({
            "mode": "isolated-local-composition-witness",
            "redis_attached": true,
            "order_endpoints": false
        }))?;

        let public_key_hex = lower_hex(signing.verifying_key().as_bytes());
        let mut genesis = Stage8bP1fGenesisManifestV1 {
            schema_version: 1,
            domain: "stage8b-p1f-genesis-manifest-v1".into(),
            installation_id: "p1f-ie-linked".into(),
            target_host_id: STAGE8B_P1F_TARGET_HOST_ID.into(),
            target_host_ssh_ed25519_sha256: STAGE8B_P1F_TARGET_HOST_SSH_ED25519_SHA256.into(),
            control_root: root.to_string_lossy().into_owned(),
            authority_generation: 1,
            ceremony_nonce_sha256: "1".repeat(64),
            genesis_head_sha256: String::new(),
            not_before_utc: canonical_timestamp(trusted_now - chrono::Duration::seconds(1)),
            expires_at_utc: canonical_timestamp(trusted_now + chrono::Duration::hours(1)),
            issuer_key_id: "p1f-ie-fixture".into(),
            signature_ed25519_hex: String::new(),
        };
        genesis.genesis_head_sha256 = genesis_head_sha256(&genesis);
        let genesis = ie_sign_document(SIGNED_GENESIS_DOMAIN, &mut genesis, &signing, |v, s| {
            v.signature_ed25519_hex = s;
        })?;
        let genesis_receipt =
            store.initialize_authority(&genesis, &public_key_hex, "p1f-ie-fixture", trusted_now)?;
        let genesis_manifest: Stage8bP1fGenesisManifestV1 = parse_canonical(&genesis)?;
        let mut activation = Stage8bP1fActivationCertificateV1 {
            schema_version: 1,
            domain: "stage8b-p1f-activation-certificate-v1".into(),
            installation_id: genesis_manifest.installation_id,
            target_host_id: genesis_manifest.target_host_id,
            control_root: genesis_manifest.control_root,
            authority_generation: genesis_manifest.authority_generation,
            ceremony_nonce_sha256: genesis_manifest.ceremony_nonce_sha256,
            genesis_manifest_sha256: sha256_hex(&genesis),
            genesis_receipt_sha256: sha256_hex(&canonical_json(&genesis_receipt)?),
            genesis_head_sha256: genesis_manifest.genesis_head_sha256,
            activated_at_utc: canonical_timestamp(trusted_now),
            expires_at_utc: canonical_timestamp(trusted_now + chrono::Duration::hours(1)),
            issuer_key_id: "p1f-ie-fixture".into(),
            signature_ed25519_hex: String::new(),
        };
        let activation = ie_sign_document(
            SIGNED_ACTIVATION_DOMAIN,
            &mut activation,
            &signing,
            |v, s| v.signature_ed25519_hex = s,
        )?;
        store.activate_authority(&activation, &public_key_hex, "p1f-ie-fixture", trusted_now)?;
        let head = store.inspect()?;
        let phase = ie_phase_manifest(
            &root,
            &head,
            Stage8bP1fPhaseV1::O2MaterializeBootstrap,
            &sha256_hex(&policy),
            &sha256_hex(&template),
            trusted_now,
            &signing,
        )?;
        let manifest_sha256 = sha256_hex(&phase);
        store.claim_phase(&phase, &public_key_hex, "p1f-ie-fixture", trusted_now)?;
        let receipt = store.materialize_o2_at(
            &config_root,
            &manifest_sha256,
            &policy,
            &template,
            &source,
            &broker_truth_checked_at_utc,
            trusted_now,
        )?;
        let supervisor_bytes = fs::read(config_root.join("supervisor.json"))?;
        let source_bytes = fs::read(
            config_root
                .join("bootstrap")
                .join("stage8b-p1-first-boot-source-v1.json"),
        )?;
        if sha256_hex(&source_bytes) != receipt.source_sha256
            || sha256_hex(&supervisor_bytes) != receipt.final_config_sha256
        {
            return Err(Stage8bP1fAuthorityErrorV1::HistoryConflict);
        }
        let stopped = crate::stage8b_p1f_o2_systemd::parse_unit_evidence(
            b"ActiveState=inactive\nSubState=dead\nResult=success\nExecMainStatus=0\nMainPID=0\nControlPID=0\nJob=\nControlGroup=\n",
        )
        .map_err(|_| Stage8bP1fAuthorityErrorV1::HistoryConflict)?;
        if !stopped.stopped_proven {
            return Err(Stage8bP1fAuthorityErrorV1::HistoryConflict);
        }
        let terminal = store.finish_phase(
            &manifest_sha256,
            Stage8bP1fPhaseStateV1::Completed,
            "ie-o2-materialized",
            trusted_now + chrono::Duration::milliseconds(1),
        )?;
        if store.terminal_receipt(&manifest_sha256)?.as_ref() != Some(&terminal) {
            return Err(Stage8bP1fAuthorityErrorV1::HistoryConflict);
        }
        Ok(Self {
            root,
            config_root,
            durable_parent,
            store,
            signing,
            operational_identity_sha256,
            runtime_config_fingerprint_sha256,
            source_sha256: receipt.source_sha256,
            config_sha256: receipt.final_config_sha256,
            candidate_close_ts_utc_ms,
        })
    }

    pub fn supervisor_path(&self) -> PathBuf {
        self.config_root.join("supervisor.json")
    }

    pub fn source_path(&self) -> PathBuf {
        self.config_root
            .join("bootstrap")
            .join("stage8b-p1-first-boot-source-v1.json")
    }

    pub fn durable_parent(&self) -> &Path {
        &self.durable_parent
    }

    pub fn operational_identity_sha256(&self) -> &str {
        &self.operational_identity_sha256
    }

    pub fn runtime_config_fingerprint_sha256(&self) -> &str {
        &self.runtime_config_fingerprint_sha256
    }

    pub fn source_sha256(&self) -> &str {
        &self.source_sha256
    }

    pub fn config_sha256(&self) -> &str {
        &self.config_sha256
    }

    pub const fn candidate_close_ts_utc_ms(&self) -> i64 {
        self.candidate_close_ts_utc_ms
    }

    pub async fn supervise_o3_child(
        &self,
        command: String,
        completion_marker: &Path,
    ) -> Result<crate::Stage8bP1fLocalSupervisionResultV1, crate::Stage8bP1fLocalSupervisionErrorV1>
    {
        use crate::stage8b_p1f_local_supervision::test_support;

        let now = Utc::now();
        let head = self
            .store
            .inspect()
            .map_err(crate::Stage8bP1fLocalSupervisionErrorV1::Authority)?;
        let phase = ie_phase_manifest(
            &self.root,
            &head,
            Stage8bP1fPhaseV1::O3SyntheticPaper,
            &"3".repeat(64),
            &"4".repeat(64),
            now,
            &self.signing,
        )
        .map_err(crate::Stage8bP1fLocalSupervisionErrorV1::Authority)?;
        let manifest_sha256 = sha256_hex(&phase);
        let public_key_hex = lower_hex(self.signing.verifying_key().as_bytes());
        self.store
            .claim_phase(&phase, &public_key_hex, "p1f-ie-fixture", now)
            .map_err(crate::Stage8bP1fLocalSupervisionErrorV1::Authority)?;
        let permit = self
            .store
            .admit_active_phase_at(
                &manifest_sha256,
                now + chrono::Duration::milliseconds(1),
                None,
            )
            .map_err(crate::Stage8bP1fLocalSupervisionErrorV1::Authority)?;
        let child = test_support::shell(command);
        let (sender, receiver) = test_support::signals();
        let runner = test_support::run(
            &self.store,
            permit,
            child,
            receiver,
            test_support::policy(StdDuration::from_millis(10), StdDuration::from_millis(100)),
        );
        let trigger = async {
            let deadline = Instant::now() + StdDuration::from_secs(30);
            while !completion_marker.exists() {
                if Instant::now() >= deadline {
                    return Err(crate::Stage8bP1fLocalSupervisionErrorV1::UnexpectedCleanExit);
                }
                tokio::time::sleep(StdDuration::from_millis(10)).await;
            }
            sender
                .send(test_support::sigterm())
                .map_err(|_| crate::Stage8bP1fLocalSupervisionErrorV1::SignalTask)?;
            Ok(())
        };
        let (result, trigger) = tokio::join!(runner, trigger);
        trigger?;
        result
    }
}

#[cfg(feature = "stage8b-p1-test-fixtures")]
impl Drop for Stage8bP1fIeLinkedFixtureV1 {
    fn drop(&mut self) {
        if let Some(scratch) = self.root.parent() {
            let _ = fs::remove_dir_all(scratch);
        }
    }
}

#[cfg(feature = "stage8b-p1-test-fixtures")]
fn ie_sign_document<T, F>(
    domain: &[u8],
    value: &mut T,
    key: &SigningKey,
    set_signature: F,
) -> Result<Vec<u8>, Stage8bP1fAuthorityErrorV1>
where
    T: Serialize,
    F: FnOnce(&mut T, String),
{
    let mut payload = domain.to_vec();
    payload.extend(canonical_json(value)?);
    set_signature(value, lower_hex(&key.sign(&payload).to_bytes()));
    canonical_json(value)
}

#[cfg(feature = "stage8b-p1-test-fixtures")]
#[allow(clippy::too_many_arguments)]
fn ie_phase_manifest(
    root: &Path,
    predecessor: &Stage8bP1fAuthorityInspectionV1,
    phase: Stage8bP1fPhaseV1,
    policy_sha256: &str,
    template_sha256: &str,
    now: DateTime<Utc>,
    signing: &SigningKey,
) -> Result<Vec<u8>, Stage8bP1fAuthorityErrorV1> {
    let mut value = Stage8bP1fPhaseManifestV1 {
        schema_version: 1,
        domain: "stage8b-p1f-phase-manifest-v1".into(),
        installation_id: "p1f-ie-linked".into(),
        target_host_id: STAGE8B_P1F_TARGET_HOST_ID.into(),
        target_host_ssh_ed25519_sha256: STAGE8B_P1F_TARGET_HOST_SSH_ED25519_SHA256.into(),
        authority_generation: predecessor.authority_generation,
        authority_sequence: predecessor.latest_sequence + 1,
        predecessor_event_sha256: predecessor.latest_event_sha256.clone(),
        accepted_source_tree_sha256: "2".repeat(64),
        phase,
        materialization_policy_sha256: policy_sha256.to_string(),
        config_template_sha256: template_sha256.to_string(),
        installation_sha256: sha256_hex(root.to_string_lossy().as_bytes()),
        controller_id: "p1f-ie-controller".into(),
        not_before_utc: canonical_timestamp(now),
        deadline_utc: canonical_timestamp(now + chrono::Duration::minutes(30)),
        issuer_key_id: "p1f-ie-fixture".into(),
        signature_ed25519_hex: String::new(),
    };
    ie_sign_document(SIGNED_PHASE_DOMAIN, &mut value, signing, |v, s| {
        v.signature_ed25519_hex = s;
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    use ed25519_dalek::{Signer, SigningKey};
    use std::{
        sync::atomic::{AtomicU64, Ordering},
        time::{SystemTime, UNIX_EPOCH},
    };

    struct Setup {
        root: PathBuf,
        store: Stage8bP1fAuthorityStoreV1,
        signing: SigningKey,
        now: DateTime<Utc>,
    }

    static TEST_DIRECTORY_SEQUENCE: AtomicU64 = AtomicU64::new(0);

    impl Setup {
        fn new() -> Self {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let sequence = TEST_DIRECTORY_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let scratch = std::env::current_dir().unwrap().join("target/p1f-tests");
            fs::create_dir_all(&scratch).unwrap();
            let root = scratch.join(format!(
                "stage8b-p1f-{unique}-{}-{sequence}",
                std::process::id()
            ));
            fs::DirBuilder::new().mode(0o750).create(&root).unwrap();
            let uid = unsafe { libc::geteuid() };
            let gid = unsafe { libc::getegid() };
            let store = Stage8bP1fAuthorityStoreV1::open_at(&root, uid, gid).unwrap();
            Self {
                root,
                store,
                signing: SigningKey::from_bytes(&[7u8; 32]),
                now: DateTime::parse_from_rfc3339("2026-09-25T12:00:00Z")
                    .unwrap()
                    .with_timezone(&Utc),
            }
        }

        fn new_current() -> Self {
            let mut setup = Self::new();
            setup.now = Utc::now();
            setup
        }

        fn public_key_hex(&self) -> String {
            lower_hex(self.signing.verifying_key().as_bytes())
        }

        fn genesis(&self) -> Vec<u8> {
            let mut value = Stage8bP1fGenesisManifestV1 {
                schema_version: 1,
                domain: "stage8b-p1f-genesis-manifest-v1".into(),
                installation_id: "install-1".into(),
                target_host_id: STAGE8B_P1F_TARGET_HOST_ID.into(),
                target_host_ssh_ed25519_sha256: STAGE8B_P1F_TARGET_HOST_SSH_ED25519_SHA256.into(),
                control_root: self.root.to_string_lossy().into_owned(),
                authority_generation: 1,
                ceremony_nonce_sha256: "1".repeat(64),
                genesis_head_sha256: String::new(),
                not_before_utc: canonical_timestamp(self.now - Duration::seconds(1)),
                expires_at_utc: canonical_timestamp(self.now + Duration::minutes(10)),
                issuer_key_id: "p1f-offline-1".into(),
                signature_ed25519_hex: String::new(),
            };
            value.genesis_head_sha256 = genesis_head_sha256(&value);
            sign(
                SIGNED_GENESIS_DOMAIN,
                &mut value,
                &self.signing,
                |value, signature| value.signature_ed25519_hex = signature,
            )
        }

        fn initialize_and_activate(&self) -> Stage8bP1fAuthorityInspectionV1 {
            let genesis = self.genesis();
            let receipt = self
                .store
                .initialize_authority(&genesis, &self.public_key_hex(), "p1f-offline-1", self.now)
                .unwrap();
            let cert = self.activation(&genesis, &receipt);
            self.store
                .activate_authority(&cert, &self.public_key_hex(), "p1f-offline-1", self.now)
                .unwrap();
            self.store.inspect().unwrap()
        }

        fn activation(&self, genesis: &[u8], receipt: &Stage8bP1fGenesisReceiptV1) -> Vec<u8> {
            let manifest: Stage8bP1fGenesisManifestV1 = parse_canonical(genesis).unwrap();
            let mut certificate = Stage8bP1fActivationCertificateV1 {
                schema_version: 1,
                domain: "stage8b-p1f-activation-certificate-v1".into(),
                installation_id: manifest.installation_id,
                target_host_id: manifest.target_host_id,
                control_root: manifest.control_root,
                authority_generation: manifest.authority_generation,
                ceremony_nonce_sha256: manifest.ceremony_nonce_sha256,
                genesis_manifest_sha256: sha256_hex(genesis),
                genesis_receipt_sha256: sha256_hex(&canonical_json(&receipt).unwrap()),
                genesis_head_sha256: manifest.genesis_head_sha256,
                activated_at_utc: canonical_timestamp(self.now),
                expires_at_utc: canonical_timestamp(self.now + Duration::hours(1)),
                issuer_key_id: "p1f-offline-1".into(),
                signature_ed25519_hex: String::new(),
            };
            sign(
                SIGNED_ACTIVATION_DOMAIN,
                &mut certificate,
                &self.signing,
                |value, signature| value.signature_ed25519_hex = signature,
            )
        }

        fn phase(
            &self,
            predecessor: &Stage8bP1fAuthorityInspectionV1,
            phase: Stage8bP1fPhaseV1,
        ) -> Vec<u8> {
            self.phase_with_hashes(predecessor, phase, &"3".repeat(64), &"4".repeat(64))
        }

        fn phase_with_hashes(
            &self,
            predecessor: &Stage8bP1fAuthorityInspectionV1,
            phase: Stage8bP1fPhaseV1,
            policy_sha256: &str,
            template_sha256: &str,
        ) -> Vec<u8> {
            let duration = if phase == Stage8bP1fPhaseV1::O4FinamReadOnly {
                10_800
            } else {
                1_800
            };
            let mut value = Stage8bP1fPhaseManifestV1 {
                schema_version: 1,
                domain: "stage8b-p1f-phase-manifest-v1".into(),
                installation_id: "install-1".into(),
                target_host_id: STAGE8B_P1F_TARGET_HOST_ID.into(),
                target_host_ssh_ed25519_sha256: STAGE8B_P1F_TARGET_HOST_SSH_ED25519_SHA256.into(),
                authority_generation: predecessor.authority_generation,
                authority_sequence: predecessor.latest_sequence + 1,
                predecessor_event_sha256: predecessor.latest_event_sha256.clone(),
                accepted_source_tree_sha256: "2".repeat(64),
                phase,
                materialization_policy_sha256: policy_sha256.to_string(),
                config_template_sha256: template_sha256.to_string(),
                installation_sha256: "5".repeat(64),
                controller_id: "controller-1".into(),
                not_before_utc: canonical_timestamp(self.now),
                deadline_utc: canonical_timestamp(self.now + Duration::seconds(duration)),
                issuer_key_id: "p1f-offline-1".into(),
                signature_ed25519_hex: String::new(),
            };
            sign(
                SIGNED_PHASE_DOMAIN,
                &mut value,
                &self.signing,
                |value, signature| value.signature_ed25519_hex = signature,
            )
        }
    }

    impl Drop for Setup {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    fn sign<T, F>(domain: &[u8], value: &mut T, key: &SigningKey, set_signature: F) -> Vec<u8>
    where
        T: Serialize,
        F: FnOnce(&mut T, String),
    {
        let mut payload = domain.to_vec();
        payload.extend(canonical_json(value).unwrap());
        let signature = lower_hex(&key.sign(&payload).to_bytes());
        set_signature(value, signature);
        canonical_json(value).unwrap()
    }

    #[test]
    fn genesis_activation_and_exact_same_active_continuation() {
        let setup = Setup::new();
        let head = setup.initialize_and_activate();
        assert_eq!(head.state, Stage8bP1fPhaseStateV1::GenesisActivated);
        let phase = setup.phase(&head, Stage8bP1fPhaseV1::O3SyntheticPaper);
        let claimed = setup
            .store
            .claim_phase(&phase, &setup.public_key_hex(), "p1f-offline-1", setup.now)
            .unwrap();
        let first = match claimed {
            Stage8bP1fClaimDispositionV1::Claimed(value) => value,
            _ => panic!("new claim"),
        };
        let resumed = setup
            .store
            .claim_phase(
                &phase,
                &setup.public_key_hex(),
                "p1f-offline-1",
                setup.now + Duration::seconds(10),
            )
            .unwrap();
        let second = match resumed {
            Stage8bP1fClaimDispositionV1::ContinuedExisting(value) => value,
            _ => panic!("continuation"),
        };
        assert_eq!(first, second);
        assert_eq!(setup.store.inspect().unwrap().latest_sequence, 1);
    }

    #[test]
    fn activation_resumes_exact_transaction_after_head_commit() {
        let setup = Setup::new();
        let genesis = setup.genesis();
        let receipt = setup
            .store
            .initialize_authority(
                &genesis,
                &setup.public_key_hex(),
                "p1f-offline-1",
                setup.now,
            )
            .unwrap();
        let certificate = setup.activation(&genesis, &receipt);
        let authority = setup.root.join(AUTHORITY_DIRECTORY);
        let mut head: HistoryHeadV1 = setup
            .store
            .read_authority_file(&authority.join(HISTORY_HEAD_FILE), 0o440)
            .unwrap();
        head.state = Stage8bP1fPhaseStateV1::GenesisActivated;
        setup
            .store
            .replace_exact(
                &authority.join(HISTORY_HEAD_FILE),
                &canonical_json(&head).unwrap(),
                0o440,
            )
            .unwrap();

        setup
            .store
            .activate_authority(
                &certificate,
                &setup.public_key_hex(),
                "p1f-offline-1",
                setup.now,
            )
            .unwrap();
        assert_eq!(
            setup.store.inspect().unwrap().state,
            Stage8bP1fPhaseStateV1::GenesisActivated
        );
        setup
            .store
            .activate_authority(
                &certificate,
                &setup.public_key_hex(),
                "p1f-offline-1",
                setup.now,
            )
            .unwrap();
    }

    #[test]
    fn active_admission_binds_manifest_and_enforces_monotonic_stop_grace() {
        let setup = Setup::new();
        let head = setup.initialize_and_activate();
        let phase = setup.phase(&head, Stage8bP1fPhaseV1::O3SyntheticPaper);
        let manifest_sha256 = sha256_hex(&phase);
        setup
            .store
            .claim_phase(&phase, &setup.public_key_hex(), "p1f-offline-1", setup.now)
            .unwrap();

        let mut permit = setup
            .store
            .admit_active_phase_at(&manifest_sha256, setup.now + Duration::seconds(1), None)
            .unwrap();
        assert_eq!(permit.manifest_sha256(), manifest_sha256);
        assert_eq!(permit.phase(), Stage8bP1fPhaseV1::O3SyntheticPaper);
        assert_eq!(
            permit
                .poll_deadline(setup.now + Duration::seconds(2))
                .unwrap(),
            Stage8bP1fDeadlineDecisionV1::Continue
        );
        assert_eq!(
            permit
                .poll_deadline(setup.now - Duration::seconds(1))
                .unwrap(),
            Stage8bP1fDeadlineDecisionV1::BeginStopping
        );
        assert_eq!(
            permit
                .poll_deadline(setup.now + Duration::seconds(32))
                .unwrap(),
            Stage8bP1fDeadlineDecisionV1::ForceKill
        );

        assert_eq!(
            setup
                .store
                .admit_active_phase_at(&manifest_sha256, setup.now + Duration::seconds(1_800), None)
                .unwrap_err(),
            Stage8bP1fAuthorityErrorV1::ActiveConflict
        );
    }

    #[test]
    fn concurrent_guardian_and_retained_manifest_tamper_fail_closed() {
        let setup = Setup::new();
        let head = setup.initialize_and_activate();
        let phase = setup.phase(&head, Stage8bP1fPhaseV1::O3SyntheticPaper);
        let manifest_sha256 = sha256_hex(&phase);
        setup
            .store
            .claim_phase(&phase, &setup.public_key_hex(), "p1f-offline-1", setup.now)
            .unwrap();

        let lease = setup.store.acquire_lease().unwrap();
        assert_eq!(
            setup.store.inspect().unwrap_err(),
            Stage8bP1fAuthorityErrorV1::ConcurrentGuardian
        );
        drop(lease);

        let manifest_path = setup
            .root
            .join(AUTHORITY_DIRECTORY)
            .join(MANIFESTS_DIRECTORY)
            .join(&manifest_sha256)
            .join("phase-manifest.json");
        let mut retained: Stage8bP1fPhaseManifestV1 = setup
            .store
            .read_authority_file(&manifest_path, 0o440)
            .unwrap();
        retained.controller_id = "tampered-controller".into();
        setup
            .store
            .replace_exact(&manifest_path, &canonical_json(&retained).unwrap(), 0o440)
            .unwrap();
        assert_eq!(
            setup.store.inspect().unwrap_err(),
            Stage8bP1fAuthorityErrorV1::HistoryConflict
        );
    }

    #[test]
    fn pending_claim_and_terminal_transactions_resume_without_new_authority() {
        let setup = Setup::new();
        let head = setup.initialize_and_activate();
        let phase = setup.phase(&head, Stage8bP1fPhaseV1::O3SyntheticPaper);
        let manifest: Stage8bP1fPhaseManifestV1 = parse_canonical(&phase).unwrap();
        let pending = PendingClaimV1 {
            schema_version: 1,
            domain: "stage8b-p1f-pending-claim-v1".into(),
            manifest_sha256: sha256_hex(&phase),
            authority_sequence: manifest.authority_sequence,
            predecessor_event_sha256: manifest.predecessor_event_sha256,
            claimed_at_utc: canonical_timestamp(setup.now),
        };
        setup
            .store
            .write_create_new(
                &setup
                    .root
                    .join(AUTHORITY_DIRECTORY)
                    .join(PENDING_CLAIM_FILE),
                &canonical_json(&pending).unwrap(),
                0o440,
            )
            .unwrap();
        let resumed = setup
            .store
            .claim_phase(
                &phase,
                &setup.public_key_hex(),
                "p1f-offline-1",
                setup.now + Duration::seconds(1),
            )
            .unwrap();
        assert!(matches!(
            resumed,
            Stage8bP1fClaimDispositionV1::ContinuedExisting(_)
        ));

        let active = setup.store.inspect().unwrap();
        let pending_terminal = PendingTerminalV1 {
            schema_version: 1,
            domain: "stage8b-p1f-pending-terminal-v1".into(),
            manifest_sha256: sha256_hex(&phase),
            authority_generation: active.authority_generation,
            authority_sequence: active.latest_sequence + 1,
            predecessor_event_sha256: active.latest_event_sha256,
            terminal_state: Stage8bP1fPhaseStateV1::Completed,
            reason_code: "bounded-session-complete".into(),
            recorded_at_utc: canonical_timestamp(setup.now + Duration::seconds(2)),
        };
        setup
            .store
            .write_create_new(
                &setup
                    .root
                    .join(AUTHORITY_DIRECTORY)
                    .join(PENDING_TERMINAL_FILE),
                &canonical_json(&pending_terminal).unwrap(),
                0o440,
            )
            .unwrap();
        let terminal = setup
            .store
            .finish_phase(
                &sha256_hex(&phase),
                Stage8bP1fPhaseStateV1::Completed,
                "bounded-session-complete",
                setup.now + Duration::seconds(3),
            )
            .unwrap();
        assert_eq!(terminal.recorded_at_utc, pending_terminal.recorded_at_utc);
        assert_eq!(
            setup.store.inspect().unwrap().state,
            Stage8bP1fPhaseStateV1::Completed
        );
    }

    #[test]
    fn terminal_transition_is_required_before_next_phase_and_expiry_is_exact() {
        let setup = Setup::new();
        let head = setup.initialize_and_activate();
        let phase = setup.phase(&head, Stage8bP1fPhaseV1::O3SyntheticPaper);
        setup
            .store
            .claim_phase(&phase, &setup.public_key_hex(), "p1f-offline-1", setup.now)
            .unwrap();
        let active = setup.store.inspect().unwrap();
        let competing = setup.phase(&active, Stage8bP1fPhaseV1::O4FinamReadOnly);
        assert_eq!(
            setup
                .store
                .claim_phase(
                    &competing,
                    &setup.public_key_hex(),
                    "p1f-offline-1",
                    setup.now + Duration::seconds(1)
                )
                .unwrap_err(),
            Stage8bP1fAuthorityErrorV1::ActiveConflict
        );
        assert_eq!(
            setup
                .store
                .finish_phase(
                    &sha256_hex(&phase),
                    Stage8bP1fPhaseStateV1::Expired,
                    "deadline-expired",
                    setup.now + Duration::seconds(1)
                )
                .unwrap_err(),
            Stage8bP1fAuthorityErrorV1::InvalidValidityWindow
        );
        setup
            .store
            .finish_phase(
                &sha256_hex(&phase),
                Stage8bP1fPhaseStateV1::Expired,
                "deadline-expired",
                setup.now + Duration::seconds(1_800),
            )
            .unwrap();
        let terminal = setup.store.inspect().unwrap();
        assert_eq!(terminal.state, Stage8bP1fPhaseStateV1::Expired);
        let next = setup.phase(&terminal, Stage8bP1fPhaseV1::O4FinamReadOnly);
        assert!(matches!(
            setup
                .store
                .claim_phase(&next, &setup.public_key_hex(), "p1f-offline-1", setup.now)
                .unwrap(),
            Stage8bP1fClaimDispositionV1::Claimed(_)
        ));
    }

    #[test]
    fn o2_materialization_finalizes_only_source_hash_and_replays_exactly() {
        let mut setup = Setup::new();
        let config_root = setup.root.with_extension("config");
        let bootstrap_dir = config_root.join("bootstrap");
        let durable_parent = setup.root.with_extension("durable");
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o750)
            .create(&bootstrap_dir)
            .unwrap();
        fs::DirBuilder::new()
            .mode(0o755)
            .create(&durable_parent)
            .unwrap();
        fs::set_permissions(&config_root, fs::Permissions::from_mode(0o750)).unwrap();
        fs::set_permissions(&bootstrap_dir, fs::Permissions::from_mode(0o750)).unwrap();
        let durable_parent = fs::canonicalize(&durable_parent).unwrap();
        let (_, runtime_fingerprint) =
            crate::Stage8bP1RuntimeProfileV1::build_hybrid_runtime().unwrap();
        let bootstrap = crate::Stage8bP1BootstrapConfig {
            schema_version: 1,
            broker_id: crate::STAGE8B_P1_BROKER_ID.into(),
            strategy_id: crate::STAGE8B_P1_STRATEGY_ID.into(),
            account_id: "ACC_TEST_0001".into(),
            internal_symbol: crate::STAGE8B_P1_INTERNAL_SYMBOL.into(),
            venue_symbol: crate::STAGE8B_P1_VENUE_SYMBOL.into(),
            exchange: crate::STAGE8B_P1_EXCHANGE.into(),
            market: crate::STAGE8B_P1_MARKET.into(),
            tick_size: crate::STAGE8B_P1_TICK_SIZE.into(),
            runtime_config_fingerprint_sha256: runtime_fingerprint.clone(),
            instrument_map_fingerprint_sha256:
                crate::stage8b_p1_imoexf_instrument_map_fingerprint_sha256(),
            deployment_id: "finam-imoexf-paper-p1".into(),
            deployment_generation: 1,
            gateway_instance_id: "finam-imoexf-paper-gateway-1".into(),
            market_data_generation: 1,
            command_consumer_generation: 1,
            stage8a4_writer_issuer_public_key_hex: "8".repeat(64),
            durable_parent: durable_parent.clone(),
        };
        let validated_bootstrap = crate::validate_stage8b_p1_bootstrap_config(bootstrap).unwrap();
        let operational = validated_bootstrap
            .operational_identity_sha256()
            .to_string();
        let (source, trusted_now, _, _) =
            crate::stage8b_p1e_first_boot_source::tests::fixture_for_binding(
                &operational,
                "ACC_TEST_0001",
            );
        setup.now = trusted_now;
        let source_value: Value = serde_json::from_slice(&source).unwrap();
        let checked_at = source_value["broker_truth"]["checked_at_utc"]
            .as_str()
            .unwrap()
            .to_string();
        let template = canonical_json(&serde_json::json!({
            "schema_version": 1,
            "runtime_profile_id": crate::STAGE8B_P1E_RUNTIME_PROFILE_ID,
            "runtime_profile_sha256": crate::STAGE8B_P1E_RUNTIME_PROFILE_SHA256,
            "first_boot_source_bundle_sha256": STAGE8B_P1F_SOURCE_SHA256_TEMPLATE_SENTINEL,
            "schedule_registry_version": "imoexf-v1",
            "schedule_registry_identity_sha256": "6".repeat(64),
            "redis_url": crate::STAGE8B_P1E_REDIS_URL_IPV4,
            "redis_deployment_manifest_sha256": "7".repeat(64),
            "redis_runtime_policy_id": crate::STAGE8B_P1E_REDIS_RUNTIME_POLICY_ID,
            "redis_runtime_policy_sha256": crate::STAGE8B_P1E_REDIS_RUNTIME_POLICY_SHA256,
            "telemetry_contract_sha256": crate::STAGE8B_P1E_TELEMETRY_CONTRACT_SHA256,
            "health_interval_ms": 1000,
            "shutdown_grace_ms": 30000,
            "bootstrap": {
                "schema_version": 1,
                "broker_id": crate::STAGE8B_P1_BROKER_ID,
                "strategy_id": crate::STAGE8B_P1_STRATEGY_ID,
                "account_id": "ACC_TEST_0001",
                "internal_symbol": crate::STAGE8B_P1_INTERNAL_SYMBOL,
                "venue_symbol": crate::STAGE8B_P1_VENUE_SYMBOL,
                "exchange": crate::STAGE8B_P1_EXCHANGE,
                "market": crate::STAGE8B_P1_MARKET,
                "tick_size": crate::STAGE8B_P1_TICK_SIZE,
                "runtime_config_fingerprint_sha256": runtime_fingerprint,
                "instrument_map_fingerprint_sha256": crate::stage8b_p1_imoexf_instrument_map_fingerprint_sha256(),
                "deployment_id": "finam-imoexf-paper-p1",
                "deployment_generation": 1,
                "gateway_instance_id": "finam-imoexf-paper-gateway-1",
                "market_data_generation": 1,
                "command_consumer_generation": 1,
                "stage8a4_writer_issuer_public_key_hex": "8".repeat(64),
                "durable_parent": durable_parent
            }
        })).unwrap();
        let policy = canonical_json(&serde_json::json!({
            "mode": "read-only-finam-get",
            "redis_attached": false,
            "order_endpoints": false
        }))
        .unwrap();
        let head = setup.initialize_and_activate();
        let phase = setup.phase_with_hashes(
            &head,
            Stage8bP1fPhaseV1::O2MaterializeBootstrap,
            &sha256_hex(&policy),
            &sha256_hex(&template),
        );
        setup
            .store
            .claim_phase(&phase, &setup.public_key_hex(), "p1f-offline-1", setup.now)
            .unwrap();
        let manifest_sha256 = sha256_hex(&phase);
        P1F_TEST_FAULT_POINT.store(4, AtomicOrdering::SeqCst);
        assert_eq!(
            setup
                .store
                .materialize_o2_at(
                    &config_root,
                    &manifest_sha256,
                    &policy,
                    &template,
                    &source,
                    &checked_at,
                    trusted_now,
                )
                .unwrap_err(),
            Stage8bP1fAuthorityErrorV1::Io(ErrorKind::Interrupted)
        );
        assert_eq!(
            setup
                .store
                .admit_active_phase_at(&manifest_sha256, trusted_now, Some(&config_root))
                .unwrap_err(),
            Stage8bP1fAuthorityErrorV1::PendingRecoveryRequired
        );
        let first = setup
            .store
            .materialize_o2_at(
                &config_root,
                &manifest_sha256,
                &policy,
                &template,
                &source,
                &checked_at,
                trusted_now,
            )
            .unwrap();
        let sequence = setup.store.inspect().unwrap().latest_sequence;
        let second = setup
            .store
            .materialize_o2_at(
                &config_root,
                &manifest_sha256,
                &policy,
                &template,
                &source,
                &checked_at,
                trusted_now,
            )
            .unwrap();
        assert_eq!(first, second);
        assert_eq!(sequence, setup.store.inspect().unwrap().latest_sequence);
        assert_eq!(first.state, "ReadyForBootstrap");
        let final_config: Value =
            serde_json::from_slice(&fs::read(config_root.join("supervisor.json")).unwrap())
                .unwrap();
        assert_eq!(
            final_config["first_boot_source_bundle_sha256"],
            Value::String(sha256_hex(&source))
        );
        assert_eq!(
            fs::symlink_metadata(config_root.join("supervisor.json"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o440
        );
        let permit = setup
            .store
            .admit_active_phase_at(&manifest_sha256, trusted_now, Some(&config_root))
            .unwrap();
        assert_eq!(permit.phase(), Stage8bP1fPhaseV1::O2MaterializeBootstrap);
        drop(permit);
        assert_eq!(
            setup
                .store
                .admit_active_phase_at(
                    &manifest_sha256,
                    trusted_now + Duration::seconds(301),
                    Some(&config_root),
                )
                .unwrap_err(),
            Stage8bP1fAuthorityErrorV1::InvalidValidityWindow
        );
        assert_eq!(
            setup.store.inspect().unwrap().state,
            Stage8bP1fPhaseStateV1::Failed
        );
        fs::remove_dir_all(config_root).unwrap();
        fs::remove_dir_all(durable_parent).unwrap();
    }

    #[test]
    fn ordinary_claim_is_blocked_before_activation_and_genesis_cannot_repeat() {
        let setup = Setup::new();
        let genesis = setup.genesis();
        setup
            .store
            .initialize_authority(
                &genesis,
                &setup.public_key_hex(),
                "p1f-offline-1",
                setup.now,
            )
            .unwrap();
        let fake = Stage8bP1fAuthorityInspectionV1 {
            authority_generation: 1,
            latest_sequence: 0,
            latest_event_sha256: event_digest(
                &setup
                    .store
                    .read_authority_bytes(
                        &event_path(&setup.root.join(AUTHORITY_DIRECTORY), 0),
                        0o440,
                    )
                    .unwrap(),
            ),
            state: Stage8bP1fPhaseStateV1::GenesisPrepared,
            active_manifest_sha256: None,
            deadline_utc: None,
            force_kill_at_utc: None,
        };
        let phase = setup.phase(&fake, Stage8bP1fPhaseV1::O1Provision);
        assert_eq!(
            setup
                .store
                .claim_phase(&phase, &setup.public_key_hex(), "p1f-offline-1", setup.now)
                .unwrap_err(),
            Stage8bP1fAuthorityErrorV1::NotActivated
        );
        setup.initialize_and_activate();
        assert_eq!(
            setup
                .store
                .initialize_authority(
                    &genesis,
                    &setup.public_key_hex(),
                    "p1f-offline-1",
                    setup.now
                )
                .unwrap_err(),
            Stage8bP1fAuthorityErrorV1::AlreadyActivated
        );
    }

    #[test]
    fn head_rollback_and_claim_directory_loss_fail_closed() {
        let setup = Setup::new();
        let genesis_head = setup.initialize_and_activate();
        let genesis_head_bytes = setup
            .store
            .read_authority_bytes(
                &setup.root.join(AUTHORITY_DIRECTORY).join(HISTORY_HEAD_FILE),
                0o440,
            )
            .unwrap();
        let phase = setup.phase(&genesis_head, Stage8bP1fPhaseV1::O3SyntheticPaper);
        setup
            .store
            .claim_phase(&phase, &setup.public_key_hex(), "p1f-offline-1", setup.now)
            .unwrap();
        setup
            .store
            .replace_exact(
                &setup.root.join(AUTHORITY_DIRECTORY).join(HISTORY_HEAD_FILE),
                &genesis_head_bytes,
                0o440,
            )
            .unwrap();
        assert_eq!(
            setup.store.inspect().unwrap_err(),
            Stage8bP1fAuthorityErrorV1::HistoryConflict
        );

        let setup = Setup::new();
        let head = setup.initialize_and_activate();
        let phase = setup.phase(&head, Stage8bP1fPhaseV1::O3SyntheticPaper);
        setup
            .store
            .claim_phase(&phase, &setup.public_key_hex(), "p1f-offline-1", setup.now)
            .unwrap();
        let manifest_sha = sha256_hex(&phase);
        fs::remove_dir_all(
            setup
                .root
                .join(AUTHORITY_DIRECTORY)
                .join(MANIFESTS_DIRECTORY)
                .join(manifest_sha),
        )
        .unwrap();
        assert_eq!(
            setup.store.inspect().unwrap_err(),
            Stage8bP1fAuthorityErrorV1::HistoryConflict
        );
    }

    #[test]
    fn custody_drift_and_parent_substitution_are_rejected() {
        let setup = Setup::new();
        setup.initialize_and_activate();
        fs::set_permissions(
            setup.root.join(AUTHORITY_DIRECTORY),
            fs::Permissions::from_mode(0o770),
        )
        .unwrap();
        assert_eq!(
            setup.store.inspect().unwrap_err(),
            Stage8bP1fAuthorityErrorV1::InvalidCustody
        );

        let setup = Setup::new();
        setup.initialize_and_activate();
        let moved = setup.root.with_extension("moved");
        fs::rename(&setup.root, &moved).unwrap();
        fs::DirBuilder::new()
            .mode(0o750)
            .create(&setup.root)
            .unwrap();
        assert_eq!(
            setup.store.inspect().unwrap_err(),
            Stage8bP1fAuthorityErrorV1::InvalidCustody
        );
        fs::remove_dir_all(&setup.root).unwrap();
        fs::rename(&moved, &setup.root).unwrap();
    }

    #[test]
    fn restore_overlap_is_rejected_before_mutation() {
        let plan = Stage8bP1fRestorePlanV1 {
            source_roots: vec![PathBuf::from("/backup/p1")],
            target_roots: vec![PathBuf::from("/var/lib")],
            selected_paths: vec![PathBuf::from(STAGE8B_P1F_AUTHORITY_CONTROL_ROOT)],
        };
        let error = execute_stage8b_p1f_permitted_restore_v1(&plan, &[]).unwrap_err();
        assert_eq!(
            error,
            Stage8bP1fAuthorityErrorV1::RestoreOverlapsControlRoot
        );
    }

    #[test]
    fn replay_projection_rejects_terminal_head_relabelled_active() {
        let setup = Setup::new();
        let head = setup.initialize_and_activate();
        let phase = setup.phase(&head, Stage8bP1fPhaseV1::O3SyntheticPaper);
        let manifest_sha256 = sha256_hex(&phase);
        setup
            .store
            .claim_phase(&phase, &setup.public_key_hex(), "p1f-offline-1", setup.now)
            .unwrap();
        let claim = setup.store.read_claim_receipt(&manifest_sha256).unwrap();
        setup
            .store
            .finish_phase(
                &manifest_sha256,
                Stage8bP1fPhaseStateV1::Completed,
                "bounded-session-complete",
                setup.now + Duration::seconds(2),
            )
            .unwrap();
        let head_path = setup.root.join(AUTHORITY_DIRECTORY).join(HISTORY_HEAD_FILE);
        let mut forged: HistoryHeadV1 = setup.store.read_authority_file(&head_path, 0o440).unwrap();
        forged.state = Stage8bP1fPhaseStateV1::Active;
        forged.active_manifest_sha256 = Some(manifest_sha256);
        forged.deadline_utc = Some(claim.deadline_utc);
        setup
            .store
            .replace_exact(&head_path, &canonical_json(&forged).unwrap(), 0o440)
            .unwrap();
        assert_eq!(
            setup.store.inspect().unwrap_err(),
            Stage8bP1fAuthorityErrorV1::HistoryConflict
        );
    }

    #[test]
    fn pending_transactions_block_ordinary_admission() {
        let setup = Setup::new();
        let head = setup.initialize_and_activate();
        let phase = setup.phase(&head, Stage8bP1fPhaseV1::O3SyntheticPaper);
        let manifest_sha256 = sha256_hex(&phase);
        setup
            .store
            .claim_phase(&phase, &setup.public_key_hex(), "p1f-offline-1", setup.now)
            .unwrap();
        let active = setup.store.inspect().unwrap();
        let pending = PendingTerminalV1 {
            schema_version: 1,
            domain: "stage8b-p1f-pending-terminal-v1".into(),
            manifest_sha256: manifest_sha256.clone(),
            authority_generation: active.authority_generation,
            authority_sequence: active.latest_sequence + 1,
            predecessor_event_sha256: active.latest_event_sha256,
            terminal_state: Stage8bP1fPhaseStateV1::Completed,
            reason_code: "pending-terminal".into(),
            recorded_at_utc: canonical_timestamp(setup.now + Duration::seconds(1)),
        };
        setup
            .store
            .write_create_new(
                &setup
                    .root
                    .join(AUTHORITY_DIRECTORY)
                    .join(PENDING_TERMINAL_FILE),
                &canonical_json(&pending).unwrap(),
                0o440,
            )
            .unwrap();
        assert_eq!(
            setup
                .store
                .admit_active_phase_at(&manifest_sha256, setup.now, None)
                .unwrap_err(),
            Stage8bP1fAuthorityErrorV1::PendingRecoveryRequired
        );
    }

    #[test]
    fn execution_owner_is_unique_and_stopping_resume_has_no_new_grace() {
        let setup = Setup::new();
        let head = setup.initialize_and_activate();
        let phase = setup.phase(&head, Stage8bP1fPhaseV1::O3SyntheticPaper);
        let manifest_sha256 = sha256_hex(&phase);
        setup
            .store
            .claim_phase(&phase, &setup.public_key_hex(), "p1f-offline-1", setup.now)
            .unwrap();
        let mut first = setup
            .store
            .admit_active_phase_at(&manifest_sha256, setup.now, None)
            .unwrap();
        assert_eq!(
            setup
                .store
                .admit_active_phase_at(&manifest_sha256, setup.now, None)
                .unwrap_err(),
            Stage8bP1fAuthorityErrorV1::ConcurrentExecution
        );
        assert_eq!(
            first
                .poll_deadline(setup.now - Duration::seconds(1))
                .unwrap(),
            Stage8bP1fDeadlineDecisionV1::BeginStopping
        );
        drop(first);
        assert_eq!(
            setup
                .store
                .admit_active_phase_at(&manifest_sha256, setup.now, None)
                .unwrap_err(),
            Stage8bP1fAuthorityErrorV1::ActiveConflict
        );
        let mut resumed = setup
            .store
            .resume_stopping_phase(&manifest_sha256, setup.now + Duration::seconds(10))
            .unwrap();
        assert_eq!(
            resumed
                .poll_deadline_at_elapsed(setup.now + Duration::seconds(10), StdDuration::ZERO)
                .unwrap(),
            Stage8bP1fDeadlineDecisionV1::ForceKill
        );
        drop(resumed);
        let mut resumed_again = setup
            .store
            .resume_stopping_phase(&manifest_sha256, setup.now + Duration::seconds(10))
            .unwrap();
        assert_eq!(
            resumed_again
                .poll_deadline_at_elapsed(setup.now + Duration::seconds(10), StdDuration::ZERO)
                .unwrap(),
            Stage8bP1fDeadlineDecisionV1::ForceKill
        );
    }

    #[test]
    fn stopping_monotonic_bound_rejects_frozen_and_backward_wall_clock() {
        let setup = Setup::new();
        let head = setup.initialize_and_activate();
        let phase = setup.phase(&head, Stage8bP1fPhaseV1::O3SyntheticPaper);
        let manifest_sha256 = sha256_hex(&phase);
        setup
            .store
            .claim_phase(&phase, &setup.public_key_hex(), "p1f-offline-1", setup.now)
            .unwrap();
        let mut permit = setup
            .store
            .admit_active_phase_at(&manifest_sha256, setup.now, None)
            .unwrap();
        assert_eq!(
            permit
                .poll_deadline_at_elapsed(setup.now - Duration::seconds(1), StdDuration::ZERO)
                .unwrap(),
            Stage8bP1fDeadlineDecisionV1::BeginStopping
        );
        assert_eq!(
            permit
                .poll_deadline_at_elapsed(
                    setup.now + Duration::seconds(10),
                    StdDuration::from_secs(29)
                )
                .unwrap(),
            Stage8bP1fDeadlineDecisionV1::BeginStopping
        );
        assert_eq!(
            permit
                .poll_deadline_at_elapsed(
                    setup.now + Duration::seconds(10),
                    StdDuration::from_secs(30)
                )
                .unwrap(),
            Stage8bP1fDeadlineDecisionV1::ForceKill
        );
        assert_eq!(
            permit
                .poll_deadline_at_elapsed(
                    setup.now + Duration::seconds(10),
                    StdDuration::from_secs(31)
                )
                .unwrap(),
            Stage8bP1fDeadlineDecisionV1::ForceKill
        );

        let setup = Setup::new();
        let head = setup.initialize_and_activate();
        let phase = setup.phase(&head, Stage8bP1fPhaseV1::O3SyntheticPaper);
        let manifest_sha256 = sha256_hex(&phase);
        setup
            .store
            .claim_phase(&phase, &setup.public_key_hex(), "p1f-offline-1", setup.now)
            .unwrap();
        let mut permit = setup
            .store
            .admit_active_phase_at(&manifest_sha256, setup.now, None)
            .unwrap();
        permit
            .poll_deadline_at_elapsed(setup.now - Duration::seconds(1), StdDuration::ZERO)
            .unwrap();
        assert_eq!(
            permit
                .poll_deadline_at_elapsed(
                    setup.now + Duration::seconds(10),
                    StdDuration::from_secs(10)
                )
                .unwrap(),
            Stage8bP1fDeadlineDecisionV1::BeginStopping
        );
        assert_eq!(
            permit
                .poll_deadline_at_elapsed(
                    setup.now + Duration::seconds(9),
                    StdDuration::from_secs(11)
                )
                .unwrap(),
            Stage8bP1fDeadlineDecisionV1::ForceKill
        );
    }

    #[test]
    fn active_readmission_without_monotonic_witness_fails_closed() {
        let setup = Setup::new();
        let head = setup.initialize_and_activate();
        let phase = setup.phase(&head, Stage8bP1fPhaseV1::O3SyntheticPaper);
        let manifest_sha256 = sha256_hex(&phase);
        setup
            .store
            .claim_phase(&phase, &setup.public_key_hex(), "p1f-offline-1", setup.now)
            .unwrap();
        drop(
            setup
                .store
                .admit_active_phase_at(&manifest_sha256, setup.now, None)
                .unwrap(),
        );
        assert_eq!(
            setup
                .store
                .admit_active_phase_at(&manifest_sha256, setup.now + Duration::seconds(600), None)
                .unwrap_err(),
            Stage8bP1fAuthorityErrorV1::DeadlineExpired
        );
        let mut resumed = setup
            .store
            .resume_stopping_phase(&manifest_sha256, setup.now + Duration::seconds(590))
            .unwrap();
        assert_eq!(
            resumed
                .poll_deadline_at_elapsed(setup.now + Duration::seconds(590), StdDuration::ZERO)
                .unwrap(),
            Stage8bP1fDeadlineDecisionV1::ForceKill
        );
        drop(resumed);
        let mut resumed_again = setup
            .store
            .resume_stopping_phase(&manifest_sha256, setup.now + Duration::seconds(590))
            .unwrap();
        assert_eq!(
            resumed_again
                .poll_deadline_at_elapsed(setup.now + Duration::seconds(590), StdDuration::ZERO)
                .unwrap(),
            Stage8bP1fDeadlineDecisionV1::ForceKill
        );
    }

    #[test]
    fn changed_boot_identity_cannot_readmit_active_phase() {
        let setup = Setup::new();
        let head = setup.initialize_and_activate();
        let phase = setup.phase(&head, Stage8bP1fPhaseV1::O3SyntheticPaper);
        let manifest_sha256 = sha256_hex(&phase);
        setup
            .store
            .claim_phase(&phase, &setup.public_key_hex(), "p1f-offline-1", setup.now)
            .unwrap();
        let permit = setup
            .store
            .admit_active_phase_at(&manifest_sha256, setup.now, None)
            .unwrap();
        drop(permit);
        let owner_path = setup
            .root
            .join(AUTHORITY_DIRECTORY)
            .join(MANIFESTS_DIRECTORY)
            .join(&manifest_sha256)
            .join(EXECUTION_OWNER_FILE);
        let mut owner: ExecutionOwnerV1 =
            setup.store.read_authority_file(&owner_path, 0o440).unwrap();
        owner.boot_id = "previous-boot-id".into();
        setup
            .store
            .replace_exact(&owner_path, &canonical_json(&owner).unwrap(), 0o440)
            .unwrap();
        assert_eq!(
            setup
                .store
                .admit_active_phase_at(&manifest_sha256, setup.now + Duration::seconds(1), None)
                .unwrap_err(),
            Stage8bP1fAuthorityErrorV1::DeadlineExpired
        );
        assert_eq!(
            setup.store.inspect().unwrap().state,
            Stage8bP1fPhaseStateV1::Stopping
        );
    }

    #[test]
    fn directory_only_and_exact_temp_crash_frontiers_recover() {
        let setup = Setup::new();
        let authority = setup.root.join(AUTHORITY_DIRECTORY);
        let genesis = setup.genesis();
        P1F_TEST_FAULT_POINT.store(1, AtomicOrdering::SeqCst);
        assert_eq!(
            setup
                .store
                .initialize_authority(
                    &genesis,
                    &setup.public_key_hex(),
                    "p1f-offline-1",
                    setup.now,
                )
                .unwrap_err(),
            Stage8bP1fAuthorityErrorV1::Io(ErrorKind::Interrupted)
        );
        setup
            .store
            .initialize_authority(
                &genesis,
                &setup.public_key_hex(),
                "p1f-offline-1",
                setup.now,
            )
            .unwrap();

        let head_path = authority.join(HISTORY_HEAD_FILE);
        let head_bytes = setup.store.read_authority_bytes(&head_path, 0o440).unwrap();
        P1F_TEST_FAULT_POINT.store(2, AtomicOrdering::SeqCst);
        assert_eq!(
            setup
                .store
                .replace_exact(&head_path, &head_bytes, 0o440)
                .unwrap_err(),
            Stage8bP1fAuthorityErrorV1::Io(ErrorKind::Interrupted)
        );
        setup
            .store
            .replace_exact(&head_path, &head_bytes, 0o440)
            .unwrap();
        assert!(!authority.join(".history-head.json.next").exists());

        let prepared = Setup::new();
        let prepared_genesis = prepared.genesis();
        P1F_TEST_FAULT_POINT.store(2, AtomicOrdering::SeqCst);
        assert_eq!(
            prepared
                .store
                .initialize_authority(
                    &prepared_genesis,
                    &prepared.public_key_hex(),
                    "p1f-offline-1",
                    prepared.now,
                )
                .unwrap_err(),
            Stage8bP1fAuthorityErrorV1::Io(ErrorKind::Interrupted)
        );
        let resumed = prepared
            .store
            .initialize_authority(
                &prepared_genesis,
                &prepared.public_key_hex(),
                "p1f-offline-1",
                prepared.now + Duration::seconds(5),
            )
            .unwrap();
        assert_eq!(resumed.committed_at_utc, canonical_timestamp(prepared.now));

        let external_parent = setup.root.with_extension("external");
        fs::DirBuilder::new()
            .mode(0o750)
            .create(&external_parent)
            .unwrap();
        let final_path = external_parent.join("supervisor.json");
        P1F_TEST_FAULT_POINT.store(3, AtomicOrdering::SeqCst);
        assert_eq!(
            setup
                .store
                .write_external_or_require_exact(&final_path, b"exact", 64)
                .unwrap_err(),
            Stage8bP1fAuthorityErrorV1::Io(ErrorKind::Interrupted)
        );
        setup
            .store
            .write_external_or_require_exact(&final_path, b"exact", 64)
            .unwrap();
        assert_eq!(fs::read(&final_path).unwrap(), b"exact");
        let conflict_path = external_parent.join("conflict.json");
        let conflict_temp = external_parent.join(".conflict.json.p1f-next");
        setup
            .store
            .write_create_new(&conflict_temp, b"partial", 0o440)
            .unwrap();
        assert_eq!(
            setup
                .store
                .write_external_or_require_exact(&conflict_path, b"expected", 64)
                .unwrap_err(),
            Stage8bP1fAuthorityErrorV1::HistoryConflict
        );
        fs::remove_dir_all(external_parent).unwrap();
    }

    #[test]
    fn public_transitions_recover_exact_event_temp_and_pending_stopping() {
        let setup = Setup::new();
        let head = setup.initialize_and_activate();
        let phase = setup.phase(&head, Stage8bP1fPhaseV1::O3SyntheticPaper);
        let manifest_sha256 = sha256_hex(&phase);
        P1F_TEST_FAULT_POINT.store(5, AtomicOrdering::SeqCst);
        assert_eq!(
            setup
                .store
                .claim_phase(&phase, &setup.public_key_hex(), "p1f-offline-1", setup.now)
                .unwrap_err(),
            Stage8bP1fAuthorityErrorV1::Io(ErrorKind::Interrupted)
        );
        let event_temp = setup
            .root
            .join(AUTHORITY_DIRECTORY)
            .join(EVENTS_DIRECTORY)
            .join(".00000000000000000001.json.p1f-create");
        assert!(event_temp.exists());
        assert!(matches!(
            setup
                .store
                .claim_phase(&phase, &setup.public_key_hex(), "p1f-offline-1", setup.now)
                .unwrap(),
            Stage8bP1fClaimDispositionV1::ContinuedExisting(_)
        ));
        assert!(!event_temp.exists());

        let mut permit = setup
            .store
            .admit_active_phase_at(&manifest_sha256, setup.now, None)
            .unwrap();
        P1F_TEST_FAULT_POINT.store(5, AtomicOrdering::SeqCst);
        assert_eq!(
            permit
                .poll_deadline_at_elapsed(setup.now - Duration::seconds(1), StdDuration::ZERO)
                .unwrap_err(),
            Stage8bP1fAuthorityErrorV1::Io(ErrorKind::Interrupted)
        );
        drop(permit);
        assert_eq!(
            setup
                .store
                .admit_active_phase_at(&manifest_sha256, setup.now, None)
                .unwrap_err(),
            Stage8bP1fAuthorityErrorV1::PendingRecoveryRequired
        );
        let mut resumed = setup
            .store
            .resume_stopping_phase(&manifest_sha256, setup.now + Duration::seconds(10))
            .unwrap();
        assert_eq!(
            resumed
                .poll_deadline_at_elapsed(setup.now + Duration::seconds(10), StdDuration::ZERO)
                .unwrap(),
            Stage8bP1fDeadlineDecisionV1::ForceKill
        );
        drop(resumed);
        let mut resumed_again = setup
            .store
            .resume_stopping_phase(&manifest_sha256, setup.now + Duration::seconds(10))
            .unwrap();
        assert_eq!(
            resumed_again
                .poll_deadline_at_elapsed(setup.now + Duration::seconds(10), StdDuration::ZERO)
                .unwrap(),
            Stage8bP1fDeadlineDecisionV1::ForceKill
        );
    }

    #[test]
    fn restore_symlink_alias_is_rejected_before_mutation() {
        let root = std::env::temp_dir().join(format!("p1f-restore-alias-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        let alias = root.join("control-alias");
        std::os::unix::fs::symlink(STAGE8B_P1F_AUTHORITY_CONTROL_ROOT, &alias).unwrap();
        let plan = Stage8bP1fRestorePlanV1 {
            source_roots: vec![],
            target_roots: vec![alias.join("authority")],
            selected_paths: vec![],
        };
        assert_eq!(
            execute_stage8b_p1f_permitted_restore_v1(&plan, &[]).unwrap_err(),
            Stage8bP1fAuthorityErrorV1::RestoreOverlapsControlRoot
        );
        fs::remove_file(alias).unwrap();
        let real_parent = root.join("real-parent");
        fs::create_dir(&real_parent).unwrap();
        let parent_alias = root.join("parent-alias");
        std::os::unix::fs::symlink(&real_parent, &parent_alias).unwrap();
        let parent_plan = Stage8bP1fRestorePlanV1 {
            source_roots: vec![],
            target_roots: vec![],
            selected_paths: vec![parent_alias.join("future-target")],
        };
        assert_eq!(
            execute_stage8b_p1f_permitted_restore_v1(&parent_plan, &[]).unwrap_err(),
            Stage8bP1fAuthorityErrorV1::RestoreOverlapsControlRoot
        );
        fs::remove_file(parent_alias).unwrap();
        fs::remove_dir(real_parent).unwrap();
        fs::remove_dir(root).unwrap();
    }

    #[test]
    fn restore_allows_sibling_and_binds_leaf_and_parent_by_descriptor() {
        let scratch =
            std::env::temp_dir().join(format!("p1f-restore-descriptor-{}", uuid::Uuid::new_v4()));
        let control = scratch.join("moex-finam-p1-paper-control");
        let runtime_parent = scratch.join("moex-finam-p1-paper");
        let target = runtime_parent.join("state");
        fs::create_dir_all(&control).unwrap();
        fs::create_dir_all(&target).unwrap();
        let scratch = fs::canonicalize(&scratch).unwrap();
        let control = scratch.join("moex-finam-p1-paper-control");
        let runtime_parent = scratch.join("moex-finam-p1-paper");
        let target = runtime_parent.join("state");
        let plan = Stage8bP1fRestorePlanV1 {
            source_roots: vec![],
            target_roots: vec![target.clone()],
            selected_paths: vec![],
        };
        let write = Stage8bP1fRestoreWriteV1 {
            target_root_index: 0,
            relative_path: PathBuf::from("restored.json"),
            bytes: b"allowed-sibling".to_vec(),
            mode: 0o640,
        };
        execute_restore_at(&plan, std::slice::from_ref(&write), &control, || Ok(())).unwrap();
        assert_eq!(fs::read(target.join("restored.json")).unwrap(), write.bytes);

        fs::write(control.join("protected.json"), b"unchanged").unwrap();
        let moved_target = runtime_parent.join("state-moved");
        execute_restore_at(
            &plan,
            &[Stage8bP1fRestoreWriteV1 {
                target_root_index: 0,
                relative_path: PathBuf::from("substitution.json"),
                bytes: b"descriptor-bound".to_vec(),
                mode: 0o600,
            }],
            &control,
            || {
                fs::rename(&target, &moved_target)?;
                std::os::unix::fs::symlink(&control, &target)?;
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(
            fs::read(control.join("protected.json")).unwrap(),
            b"unchanged"
        );
        assert!(!control.join("substitution.json").exists());
        assert_eq!(
            fs::read(moved_target.join("substitution.json")).unwrap(),
            b"descriptor-bound"
        );
        fs::remove_file(&target).unwrap();

        let parent_target = runtime_parent.join("parent-state");
        fs::create_dir(&parent_target).unwrap();
        let parent_plan = Stage8bP1fRestorePlanV1 {
            source_roots: vec![],
            target_roots: vec![parent_target.clone()],
            selected_paths: vec![],
        };
        let moved_parent = scratch.join("runtime-parent-moved");
        execute_restore_at(
            &parent_plan,
            &[Stage8bP1fRestoreWriteV1 {
                target_root_index: 0,
                relative_path: PathBuf::from("parent-substitution.json"),
                bytes: b"parent-descriptor-bound".to_vec(),
                mode: 0o600,
            }],
            &control,
            || {
                fs::rename(&runtime_parent, &moved_parent)?;
                std::os::unix::fs::symlink(&control, &runtime_parent)?;
                Ok(())
            },
        )
        .unwrap();
        assert!(!control.join("parent-substitution.json").exists());
        assert_eq!(
            fs::read(
                moved_parent
                    .join("parent-state")
                    .join("parent-substitution.json")
            )
            .unwrap(),
            b"parent-descriptor-bound"
        );
        fs::remove_file(&runtime_parent).unwrap();
        fs::remove_dir_all(&scratch).unwrap();
    }

    #[test]
    fn declared_coherent_restore_incident_quarantines_all_ordinary_use() {
        let setup = Setup::new();
        let head = setup.initialize_and_activate();
        let phase = setup.phase(&head, Stage8bP1fPhaseV1::O3SyntheticPaper);
        let receipt = setup
            .store
            .declare_restore_incident("provider-whole-host-restore", setup.now)
            .unwrap();
        assert!(!receipt.rebind_authorized);
        assert_eq!(
            setup.store.inspect().unwrap_err(),
            Stage8bP1fAuthorityErrorV1::Quarantined
        );
        assert_eq!(
            setup
                .store
                .claim_phase(&phase, &setup.public_key_hex(), "p1f-offline-1", setup.now)
                .unwrap_err(),
            Stage8bP1fAuthorityErrorV1::Quarantined
        );
    }

    fn admitted_o3(setup: &Setup) -> (String, Stage8bP1fRunPermitV1) {
        let head = setup.initialize_and_activate();
        let phase = setup.phase(&head, Stage8bP1fPhaseV1::O3SyntheticPaper);
        let manifest_sha256 = sha256_hex(&phase);
        setup
            .store
            .claim_phase(&phase, &setup.public_key_hex(), "p1f-offline-1", setup.now)
            .unwrap();
        let permit = setup
            .store
            .admit_active_phase_at(
                &manifest_sha256,
                setup.now + Duration::milliseconds(1),
                None,
            )
            .unwrap();
        (manifest_sha256, permit)
    }

    async fn wait_for_file(path: &Path) {
        let deadline = Instant::now() + StdDuration::from_secs(3);
        while !path.exists() {
            assert!(Instant::now() < deadline, "child marker was not created");
            tokio::time::sleep(StdDuration::from_millis(5)).await;
        }
    }

    fn shell_path(path: &Path) -> String {
        format!("'{}'", path.to_string_lossy().replace('\'', "'\\''"))
    }

    #[tokio::test(flavor = "current_thread")]
    async fn local_supervision_starts_after_admission_and_stops_on_sigterm() {
        use crate::stage8b_p1f_local_supervision::test_support;

        let setup = Setup::new_current();
        let (manifest_sha256, permit) = admitted_o3(&setup);
        let started = setup.root.join("sigterm-child-started");
        let child = test_support::shell(format!(
            "trap 'exit 0' TERM INT; echo started > {}; while :; do sleep 0.02; done",
            shell_path(&started)
        ));
        let (sender, receiver) = test_support::signals();
        let runner = test_support::run(
            &setup.store,
            permit,
            child,
            receiver,
            test_support::policy(StdDuration::from_millis(10), StdDuration::from_millis(100)),
        );
        let trigger = async {
            wait_for_file(&started).await;
            sender.send(test_support::sigterm()).unwrap();
        };
        let (result, ()) = tokio::join!(runner, trigger);
        let result = result.unwrap();
        assert_eq!(result.manifest_sha256, manifest_sha256);
        assert_eq!(result.child_starts, 1);
        assert!(!result.force_killed);
        assert_eq!(
            result.disposition,
            crate::Stage8bP1fLocalSupervisionDispositionV1::GracefulStop
        );
        assert_eq!(
            setup.store.inspect().unwrap().state,
            Stage8bP1fPhaseStateV1::Completed
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn retained_pre_spawn_signal_prevents_child_start() {
        use crate::stage8b_p1f_local_supervision::test_support;

        let setup = Setup::new_current();
        let (_manifest_sha256, permit) = admitted_o3(&setup);
        let forbidden = setup.root.join("pre-signal-child-must-not-start");
        let child = test_support::shell(format!("echo started > {}", shell_path(&forbidden)));
        let (sender, receiver) = test_support::signals();
        sender.send(test_support::sigterm()).unwrap();
        let result = test_support::run(
            &setup.store,
            permit,
            child,
            receiver,
            test_support::policy(StdDuration::from_millis(10), StdDuration::from_millis(100)),
        )
        .await
        .unwrap();
        assert_eq!(result.child_starts, 0);
        assert!(!forbidden.exists());
        assert_eq!(
            result.disposition,
            crate::Stage8bP1fLocalSupervisionDispositionV1::GracefulStop
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn local_supervision_restarts_child_under_same_permit_then_handles_sigint() {
        use crate::stage8b_p1f_local_supervision::test_support;

        let setup = Setup::new_current();
        let (_manifest_sha256, permit) = admitted_o3(&setup);
        let count = setup.root.join("restart-count");
        let ready = setup.root.join("restart-ready");
        let child = test_support::shell(format!(
            "count=0; test ! -f {count} || count=$(cat {count}); count=$((count+1)); echo $count > {count}; if test $count -eq 1; then exit 67; fi; trap 'exit 0' TERM INT; echo ready > {ready}; while :; do sleep 0.02; done",
            count = shell_path(&count),
            ready = shell_path(&ready),
        ));
        let (sender, receiver) = test_support::signals();
        let runner = test_support::run(
            &setup.store,
            permit,
            child,
            receiver,
            test_support::policy(StdDuration::from_millis(10), StdDuration::from_millis(100)),
        );
        let trigger = async {
            wait_for_file(&ready).await;
            sender.send(test_support::sigint()).unwrap();
        };
        let (result, ()) = tokio::join!(runner, trigger);
        let result = result.unwrap();
        assert_eq!(result.child_starts, 2);
        assert_eq!(fs::read_to_string(count).unwrap().trim(), "2");
        assert_eq!(
            result.disposition,
            crate::Stage8bP1fLocalSupervisionDispositionV1::GracefulStop
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn local_supervision_force_kills_noncooperative_process_group() {
        use crate::stage8b_p1f_local_supervision::{
            test_support, Stage8bP1fLocalSupervisionErrorV1,
        };

        let setup = Setup::new_current();
        let (_manifest_sha256, permit) = admitted_o3(&setup);
        let parent_pid = setup.root.join("force-parent-pid");
        let child_pid = setup.root.join("force-child-pid");
        let child = test_support::shell(format!(
            "trap '' TERM INT; (trap '' TERM INT; while :; do sleep 1; done) & echo $! > {child}; echo $$ > {parent}; while :; do sleep 1; done",
            child = shell_path(&child_pid),
            parent = shell_path(&parent_pid),
        ));
        let (sender, receiver) = test_support::signals();
        let runner = test_support::run(
            &setup.store,
            permit,
            child,
            receiver,
            test_support::force_policy(),
        );
        let trigger = async {
            wait_for_file(&parent_pid).await;
            wait_for_file(&child_pid).await;
            sender.send(test_support::sigterm()).unwrap();
        };
        let (result, ()) = tokio::join!(runner, trigger);
        assert!(matches!(
            result.unwrap_err(),
            Stage8bP1fLocalSupervisionErrorV1::ForceKilled
        ));
        assert_eq!(
            setup.store.inspect().unwrap().state,
            Stage8bP1fPhaseStateV1::Failed
        );
        for path in [&parent_pid, &child_pid] {
            let pid = fs::read_to_string(path)
                .unwrap()
                .trim()
                .parse::<i32>()
                .unwrap();
            let deadline = Instant::now() + StdDuration::from_secs(2);
            while unsafe { libc::kill(pid, 0) } == 0 && Instant::now() < deadline {
                tokio::time::sleep(StdDuration::from_millis(10)).await;
            }
            assert_ne!(unsafe { libc::kill(pid, 0) }, 0, "process {pid} survived");
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn recovered_guardian_never_starts_a_new_child() {
        use crate::stage8b_p1f_local_supervision::{
            test_support, PreparedRunV1, Stage8bP1fLocalSupervisionErrorV1,
        };

        let setup = Setup::new_current();
        let (manifest_sha256, permit) = admitted_o3(&setup);
        drop(permit);
        let recovered = test_support::prepare(
            &setup.store,
            &manifest_sha256,
            setup.now + Duration::seconds(1),
        )
        .unwrap();
        let permit = match recovered {
            PreparedRunV1::Permit(permit) => permit,
            PreparedRunV1::Expired(_) => panic!("retained owner must enter stopping recovery"),
        };
        let forbidden = setup.root.join("recovered-child-must-not-start");
        let child = test_support::shell(format!("echo started > {}", shell_path(&forbidden)));
        let (_sender, receiver) = test_support::signals();
        let error = test_support::run(
            &setup.store,
            permit,
            child,
            receiver,
            test_support::policy(StdDuration::from_millis(10), StdDuration::from_millis(50)),
        )
        .await
        .unwrap_err();
        assert!(matches!(
            error,
            Stage8bP1fLocalSupervisionErrorV1::RecoveryTerminated
        ));
        assert!(!forbidden.exists());
        assert_eq!(
            setup.store.inspect().unwrap().state,
            Stage8bP1fPhaseStateV1::Failed
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn signal_supervision_loss_is_nonzero_and_leaves_no_child() {
        use crate::stage8b_p1f_local_supervision::{
            test_support, Stage8bP1fLocalSupervisionErrorV1,
        };

        let setup = Setup::new_current();
        let (_manifest_sha256, permit) = admitted_o3(&setup);
        let started = setup.root.join("signal-loss-child-started");
        let pid_path = setup.root.join("signal-loss-child-pid");
        let child = test_support::shell(format!(
            "trap 'exit 0' TERM INT; echo $$ > {pid}; echo started > {started}; while :; do sleep 0.02; done",
            pid = shell_path(&pid_path),
            started = shell_path(&started),
        ));
        let (sender, receiver) = test_support::signals();
        let runner = test_support::run(
            &setup.store,
            permit,
            child,
            receiver,
            test_support::policy(StdDuration::from_millis(10), StdDuration::from_millis(100)),
        );
        let trigger = async {
            wait_for_file(&started).await;
            drop(sender);
        };
        let (result, ()) = tokio::join!(runner, trigger);
        assert!(matches!(
            result.unwrap_err(),
            Stage8bP1fLocalSupervisionErrorV1::SignalTask
        ));
        assert_eq!(
            setup.store.inspect().unwrap().state,
            Stage8bP1fPhaseStateV1::Failed
        );
        let pid = fs::read_to_string(pid_path)
            .unwrap()
            .trim()
            .parse::<i32>()
            .unwrap();
        assert_ne!(unsafe { libc::kill(pid, 0) }, 0);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn child_restart_budget_is_bounded_without_readmission() {
        use crate::stage8b_p1f_local_supervision::{
            test_support, Stage8bP1fLocalSupervisionErrorV1,
        };

        let setup = Setup::new_current();
        let (_manifest_sha256, permit) = admitted_o3(&setup);
        let count = setup.root.join("restart-budget-count");
        let child = test_support::shell(format!(
            "count=0; test ! -f {count} || count=$(cat {count}); count=$((count+1)); echo $count > {count}; exit 67",
            count = shell_path(&count),
        ));
        let (_sender, receiver) = test_support::signals();
        let error = test_support::run(
            &setup.store,
            permit,
            child,
            receiver,
            test_support::policy(StdDuration::from_millis(5), StdDuration::from_millis(50)),
        )
        .await
        .unwrap_err();
        assert!(matches!(
            error,
            Stage8bP1fLocalSupervisionErrorV1::RestartExhausted
        ));
        assert_eq!(fs::read_to_string(count).unwrap().trim(), "5");
        assert_eq!(
            setup.store.inspect().unwrap().state,
            Stage8bP1fPhaseStateV1::Failed
        );
    }

    #[test]
    fn production_signal_registration_precedes_synchronous_admission() {
        use crate::stage8b_p1f_local_supervision::test_support;

        const HELPER_ENV: &str = "STAGE8B_P1F_SIGNAL_REGISTRATION_HELPER";
        const READY_ENV: &str = "STAGE8B_P1F_SIGNAL_REGISTRATION_READY";
        const CHILD_ENV: &str = "STAGE8B_P1F_SIGNAL_REGISTRATION_CHILD";
        if let Ok(mode) = std::env::var(HELPER_ENV) {
            let ready = PathBuf::from(std::env::var_os(READY_ENV).unwrap());
            let child_marker = PathBuf::from(std::env::var_os(CHILD_ENV).unwrap());
            let setup = Setup::new_current();
            let (_manifest_sha256, permit) = admitted_o3(&setup);
            let child = test_support::shell(format!(
                "trap 'exit 0' TERM INT; echo started > {}; while :; do sleep 0.02; done",
                shell_path(&child_marker)
            ));
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            let result = runtime
                .block_on(test_support::run_after_synchronous_startup(
                    &setup.store,
                    permit,
                    child,
                    test_support::policy(
                        StdDuration::from_millis(5),
                        StdDuration::from_millis(100),
                    ),
                    &ready,
                    StdDuration::from_millis(250),
                ))
                .unwrap();
            let expected_starts = u32::from(mode == "CONTROL");
            assert_eq!(result.child_starts, expected_starts);
            assert_eq!(child_marker.exists(), mode == "CONTROL");
            assert_eq!(
                result.terminal_receipt.terminal_state,
                Stage8bP1fPhaseStateV1::Completed
            );
            assert_eq!(
                setup.store.inspect().unwrap().state,
                Stage8bP1fPhaseStateV1::Completed
            );
            return;
        }

        let setup = Setup::new_current();
        for (name, signal) in [("SIGTERM", libc::SIGTERM), ("SIGINT", libc::SIGINT)] {
            let ready = setup.root.join(format!("signal-registration-{name}-ready"));
            let child_marker = setup.root.join(format!("signal-registration-{name}-child"));
            let mut helper = std::process::Command::new(std::env::current_exe().unwrap())
                .arg("--exact")
                .arg(
                    "stage8b_p1f_guardian::tests::production_signal_registration_precedes_synchronous_admission",
                )
                .arg("--nocapture")
                .env(HELPER_ENV, name)
                .env(READY_ENV, &ready)
                .env(CHILD_ENV, &child_marker)
                .spawn()
                .unwrap();
            let deadline = Instant::now() + StdDuration::from_secs(5);
            while !ready.exists() && Instant::now() < deadline {
                std::thread::sleep(StdDuration::from_millis(5));
            }
            assert!(
                ready.exists(),
                "{name} registration helper did not become ready"
            );
            assert_eq!(unsafe { libc::kill(helper.id() as i32, signal) }, 0);
            let deadline = Instant::now() + StdDuration::from_secs(5);
            let status = loop {
                if let Some(status) = helper.try_wait().unwrap() {
                    break status;
                }
                assert!(Instant::now() < deadline, "{name} helper timed out");
                std::thread::sleep(StdDuration::from_millis(5));
            };
            assert!(status.success(), "{name} was not retained before spawn");
            assert!(!child_marker.exists(), "{name} allowed a child spawn");
        }

        let ready = setup.root.join("signal-registration-control-ready");
        let child_marker = setup.root.join("signal-registration-control-child");
        let mut helper = std::process::Command::new(std::env::current_exe().unwrap())
            .arg("--exact")
            .arg(
                "stage8b_p1f_guardian::tests::production_signal_registration_precedes_synchronous_admission",
            )
            .arg("--nocapture")
            .env(HELPER_ENV, "CONTROL")
            .env(READY_ENV, &ready)
            .env(CHILD_ENV, &child_marker)
            .spawn()
            .unwrap();
        let deadline = Instant::now() + StdDuration::from_secs(5);
        while !ready.exists() && Instant::now() < deadline {
            std::thread::sleep(StdDuration::from_millis(5));
        }
        assert!(
            ready.exists(),
            "control registration helper did not become ready"
        );
        let deadline = Instant::now() + StdDuration::from_secs(5);
        while !child_marker.exists() && Instant::now() < deadline {
            std::thread::sleep(StdDuration::from_millis(5));
        }
        assert!(
            child_marker.exists(),
            "no-signal control did not start its child"
        );
        assert_eq!(unsafe { libc::kill(helper.id() as i32, libc::SIGTERM) }, 0);
        let deadline = Instant::now() + StdDuration::from_secs(5);
        let status = loop {
            if let Some(status) = helper.try_wait().unwrap() {
                break status;
            }
            assert!(Instant::now() < deadline, "control helper timed out");
            std::thread::sleep(StdDuration::from_millis(5));
        };
        assert!(status.success(), "no-signal positive control failed");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn pending_stopping_frontier_recovers_through_ib_without_child() {
        use crate::stage8b_p1f_local_supervision::{
            test_support, PreparedRunV1, Stage8bP1fLocalSupervisionErrorV1,
        };

        let setup = Setup::new_current();
        let (manifest_sha256, mut permit) = admitted_o3(&setup);
        P1F_TEST_FAULT_POINT.store(5, AtomicOrdering::SeqCst);
        assert_eq!(
            permit
                .request_local_stop(
                    Stage8bP1fLocalStopCauseV1::Sigterm,
                    setup.now + Duration::seconds(1),
                )
                .unwrap_err(),
            Stage8bP1fAuthorityErrorV1::Io(ErrorKind::Interrupted)
        );
        drop(permit);
        assert_eq!(
            setup.store.inspect().unwrap_err(),
            Stage8bP1fAuthorityErrorV1::PendingRecoveryRequired
        );
        let recovered = test_support::prepare(
            &setup.store,
            &manifest_sha256,
            setup.now + Duration::seconds(2),
        )
        .unwrap();
        let permit = match recovered {
            PreparedRunV1::Permit(permit) => permit,
            PreparedRunV1::Expired(_) => panic!("pending stopping must recover a stop permit"),
        };
        let forbidden = setup.root.join("pending-stopping-child-must-not-start");
        let child = test_support::shell(format!("echo started > {}", shell_path(&forbidden)));
        let (_sender, receiver) = test_support::signals();
        assert!(matches!(
            test_support::run(
                &setup.store,
                permit,
                child,
                receiver,
                test_support::policy(StdDuration::from_millis(5), StdDuration::from_millis(50),),
            )
            .await
            .unwrap_err(),
            Stage8bP1fLocalSupervisionErrorV1::RecoveryTerminated
        ));
        assert!(!forbidden.exists());
        let terminal = setup.store.inspect().unwrap();
        assert_eq!(terminal.state, Stage8bP1fPhaseStateV1::Failed);
        assert_eq!(terminal.latest_sequence, 3);
    }

    #[test]
    fn foreign_pending_stopping_selector_is_rejected_without_advancing_head() {
        use crate::stage8b_p1f_local_supervision::test_support;

        let setup = Setup::new_current();
        let (manifest_sha256, mut permit) = admitted_o3(&setup);
        P1F_TEST_FAULT_POINT.store(5, AtomicOrdering::SeqCst);
        permit
            .request_local_stop(
                Stage8bP1fLocalStopCauseV1::Sigterm,
                setup.now + Duration::seconds(1),
            )
            .unwrap_err();
        drop(permit);
        let authority = setup.root.join(AUTHORITY_DIRECTORY);
        let pending_path = authority.join(PENDING_STOPPING_FILE);
        let mut pending: PendingStoppingV1 = setup
            .store
            .read_authority_file(&pending_path, 0o440)
            .unwrap();
        pending.manifest_sha256 = "f".repeat(64);
        setup
            .store
            .replace_exact(&pending_path, &canonical_json(&pending).unwrap(), 0o440)
            .unwrap();
        let head_before: HistoryHeadV1 = setup
            .store
            .read_authority_file(&authority.join(HISTORY_HEAD_FILE), 0o440)
            .unwrap();
        assert!(matches!(
            test_support::prepare(
                &setup.store,
                &manifest_sha256,
                setup.now + Duration::seconds(2),
            )
            .unwrap_err(),
            crate::stage8b_p1f_local_supervision::Stage8bP1fLocalSupervisionErrorV1::Authority(
                Stage8bP1fAuthorityErrorV1::PendingRecoveryRequired
            )
        ));
        let head_after: HistoryHeadV1 = setup
            .store
            .read_authority_file(&authority.join(HISTORY_HEAD_FILE), 0o440)
            .unwrap();
        assert_eq!(head_before, head_after);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn fatal_i1_exit_classes_remain_nonzero_after_operator_stop() {
        use crate::stage8b_p1f_local_supervision::{
            test_support, Stage8bP1fLocalSupervisionErrorV1,
        };

        for exit_code in [70u8, 71, 72] {
            let setup = Setup::new_current();
            let (_manifest_sha256, permit) = admitted_o3(&setup);
            let started = setup.root.join(format!("fatal-child-{exit_code}-started"));
            let child = test_support::shell(format!(
                "trap 'exit {exit_code}' TERM INT; echo started > {}; while :; do sleep 0.02; done",
                shell_path(&started)
            ));
            let (sender, receiver) = test_support::signals();
            let runner = test_support::run(
                &setup.store,
                permit,
                child,
                receiver,
                test_support::policy(StdDuration::from_millis(5), StdDuration::from_millis(100)),
            );
            let trigger = async {
                wait_for_file(&started).await;
                sender.send(test_support::sigterm()).unwrap();
            };
            let (result, ()) = tokio::join!(runner, trigger);
            let error = result.unwrap_err();
            assert!(matches!(
                error,
                Stage8bP1fLocalSupervisionErrorV1::ChildExit(code) if code == exit_code
            ));
            assert_eq!(error.exit_code(), exit_code);
            assert_eq!(
                setup.store.inspect().unwrap().state,
                Stage8bP1fPhaseStateV1::Failed
            );
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn recovered_external_stop_and_invalid_phase_are_never_successful() {
        use crate::stage8b_p1f_local_supervision::{
            test_support, PreparedRunV1, Stage8bP1fLocalSupervisionErrorV1,
        };

        let recovered_setup = Setup::new_current();
        let (manifest_sha256, mut permit) = admitted_o3(&recovered_setup);
        permit
            .request_local_stop(
                Stage8bP1fLocalStopCauseV1::Sigterm,
                recovered_setup.now + Duration::seconds(1),
            )
            .unwrap();
        drop(permit);
        let recovered = test_support::prepare(
            &recovered_setup.store,
            &manifest_sha256,
            recovered_setup.now + Duration::seconds(2),
        )
        .unwrap();
        let recovered_permit = match recovered {
            PreparedRunV1::Permit(permit) => permit,
            PreparedRunV1::Expired(_) => panic!("external stop must resume"),
        };
        let forbidden = recovered_setup
            .root
            .join("external-stop-child-must-not-start");
        let (_sender, receiver) = test_support::signals();
        let error = test_support::run(
            &recovered_setup.store,
            recovered_permit,
            test_support::shell(format!("echo started > {}", shell_path(&forbidden))),
            receiver,
            test_support::policy(StdDuration::from_millis(5), StdDuration::from_millis(50)),
        )
        .await
        .unwrap_err();
        assert!(matches!(
            error,
            Stage8bP1fLocalSupervisionErrorV1::RecoveryTerminated
        ));
        assert!(!forbidden.exists());
        assert_eq!(
            recovered_setup.store.inspect().unwrap().state,
            Stage8bP1fPhaseStateV1::Failed
        );

        let invalid_setup = Setup::new_current();
        let (_manifest_sha256, mut invalid_permit) = admitted_o3(&invalid_setup);
        invalid_permit.phase = Stage8bP1fPhaseV1::O1Provision;
        let invalid_child = invalid_setup
            .root
            .join("invalid-phase-child-must-not-start");
        let (_sender, receiver) = test_support::signals();
        assert!(matches!(
            test_support::run(
                &invalid_setup.store,
                invalid_permit,
                test_support::shell(format!("echo started > {}", shell_path(&invalid_child))),
                receiver,
                test_support::policy(StdDuration::from_millis(5), StdDuration::from_millis(50),),
            )
            .await
            .unwrap_err(),
            Stage8bP1fLocalSupervisionErrorV1::RecoveryTerminated
        ));
        assert!(!invalid_child.exists());
        assert_eq!(
            invalid_setup.store.inspect().unwrap().state,
            Stage8bP1fPhaseStateV1::Failed
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_guardian_death_signal_kills_child() {
        use crate::stage8b_p1f_local_supervision::test_support;

        const HELPER_ENV: &str = "STAGE8B_P1F_PDEATH_HELPER";
        const MARKER_ENV: &str = "STAGE8B_P1F_PDEATH_MARKER";
        const PID_ENV: &str = "STAGE8B_P1F_PDEATH_PID";
        if std::env::var(HELPER_ENV).as_deref() == Ok("1") {
            let marker = PathBuf::from(std::env::var_os(MARKER_ENV).unwrap());
            let pid = PathBuf::from(std::env::var_os(PID_ENV).unwrap());
            test_support::hold_pdeath_child(&marker, &pid);
        }

        let setup = Setup::new_current();
        let marker = setup.root.join("pdeath-helper-ready");
        let pid_path = setup.root.join("pdeath-child-pid");
        let mut helper = std::process::Command::new(std::env::current_exe().unwrap())
            .arg("--exact")
            .arg("stage8b_p1f_guardian::tests::linux_guardian_death_signal_kills_child")
            .arg("--nocapture")
            .env(HELPER_ENV, "1")
            .env(MARKER_ENV, &marker)
            .env(PID_ENV, &pid_path)
            .spawn()
            .unwrap();
        let deadline = Instant::now() + StdDuration::from_secs(5);
        while (!marker.exists() || !pid_path.exists()) && Instant::now() < deadline {
            std::thread::sleep(StdDuration::from_millis(10));
        }
        assert!(marker.exists() && pid_path.exists());
        let child_pid = fs::read_to_string(&pid_path)
            .unwrap()
            .trim()
            .parse::<i32>()
            .unwrap();
        assert_eq!(unsafe { libc::kill(helper.id() as i32, libc::SIGKILL) }, 0);
        helper.wait().unwrap();
        let deadline = Instant::now() + StdDuration::from_secs(5);
        while unsafe { libc::kill(child_pid, 0) } == 0 && Instant::now() < deadline {
            std::thread::sleep(StdDuration::from_millis(10));
        }
        assert_ne!(
            unsafe { libc::kill(child_pid, 0) },
            0,
            "parent-death child survived guardian SIGKILL"
        );
    }

    #[test]
    fn custody_policy_has_no_service_uid_write_bit() {
        for mode in [0o750, 0o440] {
            assert_eq!(mode & 0o020, 0, "service group must not write");
            assert_eq!(mode & 0o002, 0, "other users must not write");
        }
    }

    #[test]
    #[ignore = "executed only by the root Linux multi-UID evidence harness"]
    fn multi_uid_root_transition_source_probe() {
        let root = PathBuf::from(
            std::env::var("STAGE8B_P1F_MULTI_UID_EVIDENCE_ROOT")
                .expect("evidence root must be provided"),
        );
        let service_gid = std::env::var("STAGE8B_P1F_MULTI_UID_SERVICE_GID")
            .expect("service gid must be provided")
            .parse::<u32>()
            .expect("service gid must be numeric");
        fs::DirBuilder::new().mode(0o750).create(&root).unwrap();
        let root_c = CString::new(root.as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::chown(root_c.as_ptr(), 0, service_gid) }, 0);
        fs::set_permissions(&root, fs::Permissions::from_mode(0o750)).unwrap();
        let store = Stage8bP1fAuthorityStoreV1::open_at(&root, 0, service_gid).unwrap();
        let setup = Setup {
            root,
            store,
            signing: SigningKey::from_bytes(&[7u8; 32]),
            now: DateTime::parse_from_rfc3339("2026-09-25T12:00:00Z")
                .unwrap()
                .with_timezone(&Utc),
        };
        let head = setup.initialize_and_activate();
        let phase = setup.phase(&head, Stage8bP1fPhaseV1::O3SyntheticPaper);
        setup
            .store
            .claim_phase(&phase, &setup.public_key_hex(), "p1f-offline-1", setup.now)
            .unwrap();
        assert_eq!(
            setup.store.inspect().unwrap().state,
            Stage8bP1fPhaseStateV1::Active
        );
        std::mem::forget(setup);
    }
}
