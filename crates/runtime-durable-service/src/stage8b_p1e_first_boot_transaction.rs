//! Stage 8B-P1-e crash-safe first-boot transaction V5.
//!
//! This module is deliberately filesystem-only. It can create the accepted
//! Stage 7 durable root, journal, recovery seal, authenticated receipt and
//! adopted marker; it owns no Redis or FINAM capability.

use std::{
    ffi::CString,
    fs::{self, File, OpenOptions},
    io::{ErrorKind, Read, Write},
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::ffi::OsStrExt,
        unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt},
    },
    path::{Path, PathBuf},
};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use strategy_runtime_core::{
    authorize_stage6d_first_boot, export_stage5g_clean_restart, restore_stage5g_clean_restart,
    Stage5gCleanRestartSource, Stage5gLifecycleCommitmentKey, Stage6dFirstBootConfig,
};

use crate::{
    recovery::Stage7bP1eAdoptionMaterial,
    stage8b_p1_bootstrap::{restart_stage8b_p1, validate_initial_source},
    stage8b_p1e_first_boot_source::{
        build_stage8b_p1_historical_recovery_source_v1, Stage8bP1eHistoricalSourceBindingV5,
    },
    Stage7bDurableRootAuthority, Stage7bRecoveryReadyOwner, Stage7bRestartOutcome,
    Stage8bP1FirstBootAdminCommand, Stage8bP1ValidatedBootstrapConfig,
    Stage8bP1ePreparedFirstBootV1, Stage8bP1eValidatedSupervisorConfigV1, STAGE7B_JOURNAL_FILE,
    STAGE7B_RECOVERY_SEAL_FILE, STAGE8B_P1E_FIRST_BOOT_SOURCE_PLAN_V2_SHA256,
    STAGE8B_P1E_RUNTIME_PROFILE_SHA256,
};

pub const STAGE8B_P1E_TRANSACTION_MARKER_SCHEMA_VERSION: u16 = 4;
pub const STAGE8B_P1E_TRANSACTION_V5_CONTRACT_VERSION: u16 = 5;
pub const STAGE8B_P1E_FIRST_BOOT_RECEIPT_SCHEMA_VERSION: u16 = 2;
pub const STAGE8B_P1E_ADOPTION_PREDICATE_VERSION: u16 = 1;
pub const STAGE8B_P1E_TRANSACTION_MARKER_FILE: &str = ".stage8b-p1-first-boot-transaction-v4.json";
pub const STAGE8B_P1E_TRANSACTION_MARKER_TEMP_FILE: &str =
    ".stage8b-p1-first-boot-transaction-v4.tmp";
pub const STAGE8B_P1E_FIRST_BOOT_RECEIPT_FILE: &str = ".stage8b-p1-first-boot-receipt-v2.json";
pub const STAGE8B_P1E_FIRST_BOOT_RECEIPT_TEMP_FILE: &str = ".stage8b-p1-first-boot-receipt-v2.tmp";
pub const STAGE8B_P1E_FIRST_BOOT_QUARANTINE_DIRECTORY: &str = ".stage8b-p1-first-boot-quarantine";
pub const STAGE8B_P1E_FIRST_BOOT_RECOVERY_CONFIRMATION: &str =
    "RECOVER_EXISTING_STAGE8B_P1_FIRST_BOOT_V4";
pub const STAGE8B_P1E_DEPLOYMENT_IDENTITY_V2_SHA256: &str =
    "428415fdedd3fd24ac128ee2ca703a6572e57644cad0cb40b30a9c96ea62a038";

const TRANSACTION_MARKER_DOMAIN: &str = "moex.stage8b.p1e.first-boot-transaction.v4";
const TRANSACTION_MARKER_HMAC_DOMAIN: &str = "moex.stage8b.p1e.first-boot-transaction.hmac.v4";
const RECEIPT_DOMAIN: &str = "moex.stage8b.p1e.first-boot-receipt.v2";
const DERIVED_MAGIC: &[u8; 8] = b"M8BP1E01";
const UNBOUND_ROOT_IDENTITY_SHA256: &str =
    "0000000000000000000000000000000000000000000000000000000000000000";
const MAX_AUTHORITY_FILE_BYTES: u64 = 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage8bP1eFirstBootTransactionPhaseV4 {
    Prepared,
    RootPublished,
    JournalDurable,
    SealCommitted,
    Adopted,
}

impl Stage8bP1eFirstBootTransactionPhaseV4 {
    fn as_str(self) -> &'static str {
        match self {
            Self::Prepared => "prepared",
            Self::RootPublished => "root_published",
            Self::JournalDurable => "journal_durable",
            Self::SealCommitted => "seal_committed",
            Self::Adopted => "adopted",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stage8bP1eFirstBootTransactionMarkerV4 {
    schema_version: u16,
    domain: String,
    transaction_id_sha256: String,
    bootstrap_attempt_generation: u64,
    operational_identity_sha256: String,
    runtime_profile_sha256: String,
    runtime_config_fingerprint_sha256: String,
    source_bundle_sha256: String,
    source_bundle_generation: u64,
    source_plan_sha256: String,
    history_bars_sha256: String,
    riskgate_session_observations_sha256: String,
    candidate_semantic_id_sha256: String,
    canonical_root_identity_sha256: String,
    phase: Stage8bP1eFirstBootTransactionPhaseV4,
    marker_generation: u64,
    marker_hmac_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stage8bP1FirstBootReceiptV2 {
    pub schema_version: u16,
    pub domain: String,
    pub receipt_generation: u64,
    pub transaction_id_sha256: String,
    pub bootstrap_attempt_generation: u64,
    pub deployment_identity_sha256: String,
    pub canonical_root_identity_sha256: String,
    pub restart_package_schema_version: u16,
    pub restart_package_canonical_sha256: String,
    pub first_boot_provenance_canonical_sha256: String,
    pub seal_generation: u64,
    pub seal_commitment_sha256: String,
    pub stage6_checkpoint_sha256: String,
    pub adoption_predicate_version: u16,
    pub adoption_ready_owner_sha256: String,
    pub receipt_hmac_sha256: String,
}

pub struct Stage8bP1eFirstBootTransactionOutcomeV5 {
    owner: Stage7bRecoveryReadyOwner,
    receipt: Stage8bP1FirstBootReceiptV2,
}

impl Stage8bP1eFirstBootTransactionOutcomeV5 {
    pub fn owner(&self) -> &Stage7bRecoveryReadyOwner {
        &self.owner
    }

    pub fn receipt(&self) -> &Stage8bP1FirstBootReceiptV2 {
        &self.receipt
    }

    pub fn into_owner(self) -> Stage7bRecoveryReadyOwner {
        self.owner
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Stage8bP1eFirstBootTransactionError {
    #[error("Stage 8B-P1-e first-boot authorization or source binding is invalid")]
    InvalidAuthority,
    #[error("Stage 8B-P1-e bootstrap generation is invalid")]
    InvalidGeneration,
    #[error("Stage 8B-P1-e first-boot filesystem is not pristine")]
    NotPristine,
    #[error("Stage 8B-P1-e authority file is invalid or noncanonical")]
    InvalidAuthorityFile,
    #[error("Stage 8B-P1-e filesystem mutation failed: {0:?}")]
    Filesystem(ErrorKind),
    #[error("Stage 8B-P1-e authenticated package construction failed")]
    Package,
    #[error("Stage 8B-P1-e durable composition failed")]
    Durable,
    #[error("Stage 8B-P1-e receipt/adoption cross-binding failed")]
    Adoption,
    #[error("Stage 8B-P1-e recovery selector does not match the fresh classification")]
    RecoverySelectorMismatch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage8bP1eAdoptionRecoveryActionV5 {
    RemoveReceiptTempAndAdopt,
    AdoptCommittedRoot,
    StartSealCommittedToAdopted,
    CompleteSealCommittedToAdopted,
}

/// Exact pre-seal selector grammar accepted by the administrative recovery
/// boundary.  The enum is inert until it is bound to a validated deployment,
/// transaction ID and the fixed recovery confirmation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage8bP1ePreSealRecoveryActionV5 {
    RemoveMarkerTemp,
    ResumePrepared,
    QuarantineRoot,
    FinalizeQuarantine,
    CompletePreparedToRootPublished,
    CompleteRootPublishedToJournalDurable,
    CompleteJournalDurableToSealCommitted,
}

impl Stage8bP1ePreSealRecoveryActionV5 {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RemoveMarkerTemp => "remove-marker-temp",
            Self::ResumePrepared => "resume-prepared",
            Self::QuarantineRoot => "quarantine-root",
            Self::FinalizeQuarantine => "finalize-quarantine",
            Self::CompletePreparedToRootPublished => "complete-prepared-to-root-published",
            Self::CompleteRootPublishedToJournalDurable => {
                "complete-root-published-to-journal-durable"
            }
            Self::CompleteJournalDurableToSealCommitted => {
                "complete-journal-durable-to-seal-committed"
            }
        }
    }

    const fn is_administrative(self) -> bool {
        matches!(
            self,
            Self::RemoveMarkerTemp | Self::QuarantineRoot | Self::FinalizeQuarantine
        )
    }
}

/// Opaque one-shot proof of the exact recovery selector.  It is intentionally
/// neither cloneable nor serializable and is rechecked against a fresh disk
/// classification before the first mutation.
pub struct Stage8bP1ePreSealRecoverySelectorV5 {
    operational_identity_sha256: String,
    runtime_config_fingerprint_sha256: String,
    transaction_id_sha256: String,
    action: Stage8bP1ePreSealRecoveryActionV5,
}

/// A pre-seal command either converges to the adopted owner or reaches one
/// exact retained-evidence boundary.  Quarantine finalization deliberately
/// returns no runtime owner.
pub enum Stage8bP1ePreSealRecoveryOutcomeV5 {
    NoRoot(Stage8bP1eFirstBootInspectionV5),
    QuarantinePending(Stage8bP1eFirstBootInspectionV5),
    QuarantineFinalized {
        transaction_id_sha256: String,
        bootstrap_attempt_generation: u64,
    },
    Adopted(Box<Stage8bP1eFirstBootTransactionOutcomeV5>),
}

/// Exhaustive V5 on-disk first-boot classification.  Every non-corrupt
/// variant names one and only one continuation from the frozen V4+V5
/// transaction contract; none of them grants Redis or broker authority.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage8bP1eFirstBootClassificationV5 {
    NoRoot,
    UnpublishedMarkerTemp,
    PreparedWithoutRoot,
    RootWithoutJournal,
    JournalWithoutSeal,
    CommittedRootReceiptTemp,
    CommittedRootResponseLost,
    QuarantinedIncompleteRoot,
    ReceiptCommittedMarkerUpdatePending,
    AdoptedCommittedRoot,
    PreparedToRootPublishedMarkerTempPending,
    RootPublishedToJournalDurableMarkerTempPending,
    JournalDurableToSealCommittedMarkerTempPending,
    SealCommittedToAdoptedMarkerTempPending,
    CorruptOrIdentityMismatch,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Stage8bP1eFirstBootInspectionV5 {
    pub classification: Stage8bP1eFirstBootClassificationV5,
    pub transaction_id_sha256: Option<String>,
    pub required_action: &'static str,
    pub ordinary_run_allowed: bool,
}

impl Stage8bP1eFirstBootInspectionV5 {
    fn new(
        classification: Stage8bP1eFirstBootClassificationV5,
        transaction_id_sha256: Option<String>,
    ) -> Self {
        let (required_action, ordinary_run_allowed) = match classification {
            Stage8bP1eFirstBootClassificationV5::NoRoot => ("ordinary-bootstrap-only", false),
            Stage8bP1eFirstBootClassificationV5::UnpublishedMarkerTemp => {
                ("remove-marker-temp", false)
            }
            Stage8bP1eFirstBootClassificationV5::PreparedWithoutRoot => ("resume-prepared", false),
            Stage8bP1eFirstBootClassificationV5::RootWithoutJournal
            | Stage8bP1eFirstBootClassificationV5::JournalWithoutSeal => ("quarantine-root", false),
            Stage8bP1eFirstBootClassificationV5::CommittedRootReceiptTemp => {
                ("remove-receipt-temp-and-adopt", false)
            }
            Stage8bP1eFirstBootClassificationV5::CommittedRootResponseLost => {
                ("adopt-committed-root", false)
            }
            Stage8bP1eFirstBootClassificationV5::QuarantinedIncompleteRoot => {
                ("finalize-quarantine", false)
            }
            Stage8bP1eFirstBootClassificationV5::ReceiptCommittedMarkerUpdatePending => {
                ("start-seal-committed-to-adopted", false)
            }
            Stage8bP1eFirstBootClassificationV5::AdoptedCommittedRoot => {
                ("ordinary-run-only", true)
            }
            Stage8bP1eFirstBootClassificationV5::PreparedToRootPublishedMarkerTempPending => {
                ("complete-prepared-to-root-published", false)
            }
            Stage8bP1eFirstBootClassificationV5::RootPublishedToJournalDurableMarkerTempPending => {
                ("complete-root-published-to-journal-durable", false)
            }
            Stage8bP1eFirstBootClassificationV5::JournalDurableToSealCommittedMarkerTempPending => {
                ("complete-journal-durable-to-seal-committed", false)
            }
            Stage8bP1eFirstBootClassificationV5::SealCommittedToAdoptedMarkerTempPending => {
                ("complete-seal-committed-to-adopted", false)
            }
            Stage8bP1eFirstBootClassificationV5::CorruptOrIdentityMismatch => ("none", false),
        };
        Self {
            classification,
            transaction_id_sha256,
            required_action,
            ordinary_run_allowed,
        }
    }
}

pub fn authorize_stage8b_p1e_pre_seal_recovery_v5(
    config: &Stage8bP1ValidatedBootstrapConfig,
    transaction_id_sha256: &str,
    action: Stage8bP1ePreSealRecoveryActionV5,
    exact_confirmation: &str,
) -> Result<Stage8bP1ePreSealRecoverySelectorV5, Stage8bP1eFirstBootTransactionError> {
    if exact_confirmation != STAGE8B_P1E_FIRST_BOOT_RECOVERY_CONFIRMATION
        || !is_sha256(transaction_id_sha256)
    {
        return Err(Stage8bP1eFirstBootTransactionError::RecoverySelectorMismatch);
    }
    Ok(Stage8bP1ePreSealRecoverySelectorV5 {
        operational_identity_sha256: config.operational_identity_sha256().to_string(),
        runtime_config_fingerprint_sha256: config.runtime_config_fingerprint_sha256().to_string(),
        transaction_id_sha256: transaction_id_sha256.to_string(),
        action,
    })
}

/// Executes only the three evidence-preserving administrative actions.  This
/// entry point deliberately has no source-bundle or trusted-clock argument:
/// authority comes from the authenticated durable marker, exact selector,
/// deployment identity and a fresh filesystem classification.
pub fn recover_stage8b_p1e_first_boot_pre_seal_administrative_v5(
    config: Stage8bP1ValidatedBootstrapConfig,
    fresh_runtime: strategy_runtime_core::HybridIntradayRuntimeStrategy,
    selector: Stage8bP1ePreSealRecoverySelectorV5,
    commitment_key: &Stage5gLifecycleCommitmentKey,
) -> Result<Stage8bP1ePreSealRecoveryOutcomeV5, Stage8bP1eFirstBootTransactionError> {
    recover_pre_seal_administrative_v5_with_observer(
        config,
        fresh_runtime,
        selector,
        commitment_key,
        |_| {},
    )
}

/// Production recovery composition for an existing V5 transaction.  Safe
/// administrative actions never read F00. Continuations first authenticate
/// the durable marker and then reconstruct only the byte-exact historical
/// bundle bound by that marker; the 300-second rule remains mandatory for new
/// first-boot admission through `build_stage8b_p1_first_boot_source_v1`.
pub fn recover_stage8b_p1e_first_boot_pre_seal_from_supervisor_v5(
    supervisor: Stage8bP1eValidatedSupervisorConfigV1,
    trusted_now: DateTime<Utc>,
    selector: Stage8bP1ePreSealRecoverySelectorV5,
    commitment_key: &Stage5gLifecycleCommitmentKey,
) -> Result<Stage8bP1ePreSealRecoveryOutcomeV5, Stage8bP1eFirstBootTransactionError> {
    if selector.action.is_administrative() {
        let (config, fresh_runtime) = supervisor.into_first_boot_parts();
        return recover_stage8b_p1e_first_boot_pre_seal_administrative_v5(
            config,
            fresh_runtime,
            selector,
            commitment_key,
        );
    }
    let binding = historical_source_binding_v5(supervisor.bootstrap(), &selector, commitment_key)?;
    if supervisor.first_boot_source_bundle_sha256() != binding.source_bundle_sha256 {
        return Err(Stage8bP1eFirstBootTransactionError::RecoverySelectorMismatch);
    }
    let prepared =
        build_stage8b_p1_historical_recovery_source_v1(supervisor, trusted_now, &binding)
            .map_err(|_| Stage8bP1eFirstBootTransactionError::InvalidAuthority)?;
    recover_stage8b_p1e_first_boot_pre_seal_v5(prepared, selector, commitment_key)
}

fn recover_pre_seal_administrative_v5_with_observer<F>(
    config: Stage8bP1ValidatedBootstrapConfig,
    fresh_runtime: strategy_runtime_core::HybridIntradayRuntimeStrategy,
    selector: Stage8bP1ePreSealRecoverySelectorV5,
    commitment_key: &Stage5gLifecycleCommitmentKey,
    mut observer: F,
) -> Result<Stage8bP1ePreSealRecoveryOutcomeV5, Stage8bP1eFirstBootTransactionError>
where
    F: FnMut(&'static str),
{
    if !selector.action.is_administrative()
        || selector.operational_identity_sha256 != config.operational_identity_sha256()
        || selector.runtime_config_fingerprint_sha256 != config.runtime_config_fingerprint_sha256()
        || fresh_runtime.stage5c_config_fingerprint() != config.runtime_config_fingerprint_sha256()
    {
        return Err(Stage8bP1eFirstBootTransactionError::RecoverySelectorMismatch);
    }

    let inspection = classify_stage8b_p1e_first_boot_v5(
        config.duplicate_for_internal_classification(),
        commitment_key,
        fresh_runtime.clone(),
    );
    if inspection.classification == Stage8bP1eFirstBootClassificationV5::CorruptOrIdentityMismatch
        || inspection.transaction_id_sha256.as_deref()
            != Some(selector.transaction_id_sha256.as_str())
        || inspection.required_action != selector.action.as_str()
    {
        return Err(Stage8bP1eFirstBootTransactionError::RecoverySelectorMismatch);
    }

    let parent = config.durable_parent().to_path_buf();
    let root_path = parent.join(config.expected_root_name());
    match selector.action {
        Stage8bP1ePreSealRecoveryActionV5::RemoveMarkerTemp => {
            let initial: Stage8bP1eFirstBootTransactionMarkerV4 = read_canonical_authority(
                &parent,
                STAGE8B_P1E_TRANSACTION_MARKER_TEMP_FILE,
                commitment_key,
                |value: &Stage8bP1eFirstBootTransactionMarkerV4, key| {
                    value.validate_authenticated(key)
                },
            )?;
            if initial.phase != Stage8bP1eFirstBootTransactionPhaseV4::Prepared
                || !marker_matches_durable_recovery(
                    &initial,
                    &config,
                    &selector.transaction_id_sha256,
                )
                || path_exists(parent.join(STAGE8B_P1E_TRANSACTION_MARKER_FILE))?
                || path_exists(root_path)?
                || path_exists(parent.join(STAGE8B_P1E_FIRST_BOOT_RECEIPT_FILE))?
                || path_exists(parent.join(STAGE8B_P1E_FIRST_BOOT_RECEIPT_TEMP_FILE))?
            {
                return Err(Stage8bP1eFirstBootTransactionError::RecoverySelectorMismatch);
            }
            unlink_authority_file(&parent, STAGE8B_P1E_TRANSACTION_MARKER_TEMP_FILE)?;
            observer("after-remove-marker-temp-before-parent-fsync");
            sync_directory(&parent)?;
            let next = classify_stage8b_p1e_first_boot_v5(config, commitment_key, fresh_runtime);
            if next.classification != Stage8bP1eFirstBootClassificationV5::NoRoot {
                return Err(Stage8bP1eFirstBootTransactionError::Adoption);
            }
            Ok(Stage8bP1ePreSealRecoveryOutcomeV5::NoRoot(next))
        }
        Stage8bP1ePreSealRecoveryActionV5::QuarantineRoot => {
            let expected_phase = match inspection.classification {
                Stage8bP1eFirstBootClassificationV5::RootWithoutJournal => {
                    Stage8bP1eFirstBootTransactionPhaseV4::RootPublished
                }
                Stage8bP1eFirstBootClassificationV5::JournalWithoutSeal => {
                    Stage8bP1eFirstBootTransactionPhaseV4::JournalDurable
                }
                _ => return Err(Stage8bP1eFirstBootTransactionError::RecoverySelectorMismatch),
            };
            let marker = read_expected_durable_recovery_marker(
                &config,
                commitment_key,
                &selector.transaction_id_sha256,
                expected_phase,
            )?;
            quarantine_incomplete_root(
                &parent,
                &root_path,
                &marker,
                config.expected_root_name(),
                &mut observer,
            )?;
            let next = classify_stage8b_p1e_first_boot_v5(config, commitment_key, fresh_runtime);
            if next.classification != Stage8bP1eFirstBootClassificationV5::QuarantinedIncompleteRoot
            {
                return Err(Stage8bP1eFirstBootTransactionError::Adoption);
            }
            Ok(Stage8bP1ePreSealRecoveryOutcomeV5::QuarantinePending(next))
        }
        Stage8bP1ePreSealRecoveryActionV5::FinalizeQuarantine => {
            let marker: Stage8bP1eFirstBootTransactionMarkerV4 = read_canonical_authority(
                &parent,
                STAGE8B_P1E_TRANSACTION_MARKER_FILE,
                commitment_key,
                |value: &Stage8bP1eFirstBootTransactionMarkerV4, key| {
                    value.validate_authenticated(key)
                },
            )?;
            if !matches!(
                marker.phase,
                Stage8bP1eFirstBootTransactionPhaseV4::RootPublished
                    | Stage8bP1eFirstBootTransactionPhaseV4::JournalDurable
            ) || !marker_matches_durable_recovery(
                &marker,
                &config,
                &selector.transaction_id_sha256,
            ) {
                return Err(Stage8bP1eFirstBootTransactionError::RecoverySelectorMismatch);
            }
            finalize_quarantine(&parent, &marker, commitment_key, &mut observer)?;
            Ok(Stage8bP1ePreSealRecoveryOutcomeV5::QuarantineFinalized {
                transaction_id_sha256: marker.transaction_id_sha256,
                bootstrap_attempt_generation: marker.bootstrap_attempt_generation,
            })
        }
        Stage8bP1ePreSealRecoveryActionV5::ResumePrepared
        | Stage8bP1ePreSealRecoveryActionV5::CompletePreparedToRootPublished
        | Stage8bP1ePreSealRecoveryActionV5::CompleteRootPublishedToJournalDurable
        | Stage8bP1ePreSealRecoveryActionV5::CompleteJournalDurableToSealCommitted => {
            Err(Stage8bP1eFirstBootTransactionError::RecoverySelectorMismatch)
        }
    }
}

/// Executes exactly one authenticated pre-seal administrative selector and,
/// where the accepted contract requires it, continues the same transaction
/// to adoption.  Classification and selector checks happen before the first
/// filesystem mutation.  The function owns no Redis or broker capability.
pub fn recover_stage8b_p1e_first_boot_pre_seal_v5(
    prepared: Stage8bP1ePreparedFirstBootV1,
    selector: Stage8bP1ePreSealRecoverySelectorV5,
    commitment_key: &Stage5gLifecycleCommitmentKey,
) -> Result<Stage8bP1ePreSealRecoveryOutcomeV5, Stage8bP1eFirstBootTransactionError> {
    recover_stage8b_p1e_first_boot_pre_seal_v5_with_observer(
        prepared,
        selector,
        commitment_key,
        |_| {},
    )
}

fn recover_stage8b_p1e_first_boot_pre_seal_v5_with_observer<F>(
    prepared: Stage8bP1ePreparedFirstBootV1,
    selector: Stage8bP1ePreSealRecoverySelectorV5,
    commitment_key: &Stage5gLifecycleCommitmentKey,
    mut observer: F,
) -> Result<Stage8bP1ePreSealRecoveryOutcomeV5, Stage8bP1eFirstBootTransactionError>
where
    F: FnMut(&'static str),
{
    let (config, source, export_input, fresh_runtime, provenance) = prepared.into_parts();
    if selector.action.is_administrative() {
        return recover_pre_seal_administrative_v5_with_observer(
            config,
            fresh_runtime,
            selector,
            commitment_key,
            observer,
        );
    }
    if selector.operational_identity_sha256 != config.operational_identity_sha256()
        || selector.runtime_config_fingerprint_sha256 != config.runtime_config_fingerprint_sha256()
        || provenance.operational_identity_sha256() != config.operational_identity_sha256()
        || provenance.runtime_config_fingerprint_sha256()
            != config.runtime_config_fingerprint_sha256()
    {
        return Err(Stage8bP1eFirstBootTransactionError::RecoverySelectorMismatch);
    }
    validate_initial_source(&config, &source, &fresh_runtime)
        .map_err(|_| Stage8bP1eFirstBootTransactionError::InvalidAuthority)?;
    let stage5g_seed = export_stage5g_clean_restart(
        Stage5gCleanRestartSource::TimerReady(source),
        export_input,
        commitment_key,
    )
    .map_err(|_| Stage8bP1eFirstBootTransactionError::Package)?;
    drop(
        restore_stage5g_clean_restart(&stage5g_seed, commitment_key, fresh_runtime.clone())
            .map_err(|_| Stage8bP1eFirstBootTransactionError::Package)?,
    );

    let expected_transaction_id = transaction_id_sha256(
        provenance.operational_identity_sha256(),
        provenance.source_bundle_sha256(),
        provenance.source_bundle_generation(),
        recovery_bootstrap_generation(
            config.durable_parent(),
            commitment_key,
            &selector.transaction_id_sha256,
        )?,
    )?;
    if expected_transaction_id != selector.transaction_id_sha256 {
        return Err(Stage8bP1eFirstBootTransactionError::RecoverySelectorMismatch);
    }

    let inspection = classify_stage8b_p1e_first_boot_v5(
        config.duplicate_for_internal_classification(),
        commitment_key,
        fresh_runtime.clone(),
    );
    if inspection.classification == Stage8bP1eFirstBootClassificationV5::CorruptOrIdentityMismatch
        || inspection.transaction_id_sha256.as_deref()
            != Some(selector.transaction_id_sha256.as_str())
        || inspection.required_action != selector.action.as_str()
    {
        return Err(Stage8bP1eFirstBootTransactionError::RecoverySelectorMismatch);
    }

    let parent = config.durable_parent().to_path_buf();
    match selector.action {
        Stage8bP1ePreSealRecoveryActionV5::RemoveMarkerTemp
        | Stage8bP1ePreSealRecoveryActionV5::QuarantineRoot
        | Stage8bP1ePreSealRecoveryActionV5::FinalizeQuarantine => {
            Err(Stage8bP1eFirstBootTransactionError::RecoverySelectorMismatch)
        }
        Stage8bP1ePreSealRecoveryActionV5::ResumePrepared => {
            let marker = read_expected_recovery_marker(
                &config,
                commitment_key,
                &selector.transaction_id_sha256,
                &provenance,
                Stage8bP1eFirstBootTransactionPhaseV4::Prepared,
            )?;
            continue_first_boot_from_prepared(
                config,
                marker,
                &stage5g_seed,
                provenance,
                commitment_key,
                fresh_runtime,
                &mut observer,
            )
            .map(Box::new)
            .map(Stage8bP1ePreSealRecoveryOutcomeV5::Adopted)
        }
        Stage8bP1ePreSealRecoveryActionV5::CompletePreparedToRootPublished => {
            let marker = complete_marker_temp_transition(
                &config,
                commitment_key,
                &selector.transaction_id_sha256,
                &provenance,
                Stage8bP1eFirstBootTransactionPhaseV4::Prepared,
                Stage8bP1eFirstBootTransactionPhaseV4::RootPublished,
                &mut observer,
            )?;
            continue_first_boot_from_root_published(
                config,
                marker,
                &stage5g_seed,
                provenance,
                commitment_key,
                fresh_runtime,
                &mut observer,
            )
            .map(Box::new)
            .map(Stage8bP1ePreSealRecoveryOutcomeV5::Adopted)
        }
        Stage8bP1ePreSealRecoveryActionV5::CompleteRootPublishedToJournalDurable => {
            let marker = complete_marker_temp_transition(
                &config,
                commitment_key,
                &selector.transaction_id_sha256,
                &provenance,
                Stage8bP1eFirstBootTransactionPhaseV4::RootPublished,
                Stage8bP1eFirstBootTransactionPhaseV4::JournalDurable,
                &mut observer,
            )?;
            continue_first_boot_from_existing_journal(
                config,
                marker,
                &stage5g_seed,
                provenance,
                commitment_key,
                fresh_runtime,
                &mut observer,
            )
            .map(Box::new)
            .map(Stage8bP1ePreSealRecoveryOutcomeV5::Adopted)
        }
        Stage8bP1ePreSealRecoveryActionV5::CompleteJournalDurableToSealCommitted => {
            let marker = complete_marker_temp_transition(
                &config,
                commitment_key,
                &selector.transaction_id_sha256,
                &provenance,
                Stage8bP1eFirstBootTransactionPhaseV4::JournalDurable,
                Stage8bP1eFirstBootTransactionPhaseV4::SealCommitted,
                &mut observer,
            )?;
            let restart = restart_stage8b_p1(config, commitment_key, fresh_runtime)
                .map_err(|_| Stage8bP1eFirstBootTransactionError::Adoption)?;
            let Stage7bRestartOutcome::Ready(owner) = restart else {
                return Err(Stage8bP1eFirstBootTransactionError::Adoption);
            };
            adopt_ready_owner(parent, marker, *owner, commitment_key, &mut observer)
                .map(Box::new)
                .map(Stage8bP1ePreSealRecoveryOutcomeV5::Adopted)
        }
    }
}

#[cfg(test)]
pub(crate) fn test_recover_stage8b_p1e_first_boot_pre_seal_v5_with_observer<F>(
    prepared: Stage8bP1ePreparedFirstBootV1,
    selector: Stage8bP1ePreSealRecoverySelectorV5,
    commitment_key: &Stage5gLifecycleCommitmentKey,
    observer: F,
) -> Result<Stage8bP1ePreSealRecoveryOutcomeV5, Stage8bP1eFirstBootTransactionError>
where
    F: FnMut(&'static str),
{
    recover_stage8b_p1e_first_boot_pre_seal_v5_with_observer(
        prepared,
        selector,
        commitment_key,
        observer,
    )
}

#[cfg(test)]
pub(crate) fn test_recover_stage8b_p1e_first_boot_pre_seal_historical_from_bytes_v5(
    config: Stage8bP1ValidatedBootstrapConfig,
    fresh_runtime: strategy_runtime_core::HybridIntradayRuntimeStrategy,
    source_bytes: &[u8],
    trusted_now: DateTime<Utc>,
    selector: Stage8bP1ePreSealRecoverySelectorV5,
    commitment_key: &Stage5gLifecycleCommitmentKey,
) -> Result<Stage8bP1ePreSealRecoveryOutcomeV5, Stage8bP1eFirstBootTransactionError> {
    let binding = historical_source_binding_v5(&config, &selector, commitment_key)?;
    let prepared = crate::stage8b_p1e_first_boot_source::test_build_stage8b_p1_historical_recovery_source_from_bytes_v1(
        config,
        fresh_runtime,
        source_bytes,
        trusted_now,
        &binding,
    )
    .map_err(|_| Stage8bP1eFirstBootTransactionError::InvalidAuthority)?;
    recover_stage8b_p1e_first_boot_pre_seal_v5(prepared, selector, commitment_key)
}

/// Completes only the four post-seal response-loss states.  The exact action
/// and transaction ID are checked against a fresh authenticated disk read
/// before any unlink, receipt write or marker rename.  Pre-seal states remain
/// fail-closed for their separate administrative recovery slice.
pub fn recover_stage8b_p1e_first_boot_adoption_v5(
    config: Stage8bP1ValidatedBootstrapConfig,
    commitment_key: &Stage5gLifecycleCommitmentKey,
    fresh_runtime: strategy_runtime_core::HybridIntradayRuntimeStrategy,
    expected_transaction_id_sha256: &str,
    action: Stage8bP1eAdoptionRecoveryActionV5,
) -> Result<Stage8bP1eFirstBootTransactionOutcomeV5, Stage8bP1eFirstBootTransactionError> {
    if !is_sha256(expected_transaction_id_sha256) {
        return Err(Stage8bP1eFirstBootTransactionError::RecoverySelectorMismatch);
    }
    let parent = config.durable_parent().to_path_buf();
    let root_path = parent.join(config.expected_root_name());
    let mut marker = read_canonical_authority(
        &parent,
        STAGE8B_P1E_TRANSACTION_MARKER_FILE,
        commitment_key,
        |value: &Stage8bP1eFirstBootTransactionMarkerV4, key| value.validate_authenticated(key),
    )?;
    if marker.transaction_id_sha256 != expected_transaction_id_sha256
        || marker.operational_identity_sha256 != config.operational_identity_sha256()
        || marker.phase != Stage8bP1eFirstBootTransactionPhaseV4::SealCommitted
        || canonical_root_identity_sha256(&root_path)? != marker.canonical_root_identity_sha256
        || !path_exists(root_path.join(STAGE7B_JOURNAL_FILE))?
        || !path_exists(root_path.join(STAGE7B_RECOVERY_SEAL_FILE))?
    {
        return Err(Stage8bP1eFirstBootTransactionError::RecoverySelectorMismatch);
    }
    validate_owned_directory(&root_path)?;
    validate_owned_regular_file(&root_path.join(STAGE7B_JOURNAL_FILE))?;
    validate_owned_regular_file(&root_path.join(STAGE7B_RECOVERY_SEAL_FILE))?;

    let marker_temp = optional_authenticated_marker(
        &parent,
        STAGE8B_P1E_TRANSACTION_MARKER_TEMP_FILE,
        commitment_key,
    )?;
    let receipt = optional_authenticated_receipt(
        &parent,
        STAGE8B_P1E_FIRST_BOOT_RECEIPT_FILE,
        commitment_key,
    )?;
    let receipt_temp = optional_authenticated_receipt(
        &parent,
        STAGE8B_P1E_FIRST_BOOT_RECEIPT_TEMP_FILE,
        commitment_key,
    )?;

    let restart = restart_stage8b_p1(config, commitment_key, fresh_runtime)
        .map_err(|_| Stage8bP1eFirstBootTransactionError::Adoption)?;
    let Stage7bRestartOutcome::Ready(mut owner) = restart else {
        return Err(Stage8bP1eFirstBootTransactionError::Adoption);
    };
    let adoption = owner
        .stage8b_p1e_adoption_material(commitment_key)
        .map_err(|_| Stage8bP1eFirstBootTransactionError::Adoption)?;
    if !marker_matches_adoption(&marker, &adoption) {
        return Err(Stage8bP1eFirstBootTransactionError::Adoption);
    }
    let expected_receipt = Stage8bP1FirstBootReceiptV2::from_adoption(
        &marker,
        marker.canonical_root_identity_sha256.clone(),
        &adoption,
        commitment_key,
    )?;

    match action {
        Stage8bP1eAdoptionRecoveryActionV5::RemoveReceiptTempAndAdopt => {
            if marker_temp.is_some()
                || receipt.is_some()
                || receipt_temp.as_ref() != Some(&expected_receipt)
            {
                return Err(Stage8bP1eFirstBootTransactionError::RecoverySelectorMismatch);
            }
            unlink_authority_file(&parent, STAGE8B_P1E_FIRST_BOOT_RECEIPT_TEMP_FILE)?;
            sync_directory(&parent)?;
            commit_receipt(&parent, &expected_receipt, commitment_key, &mut |_| {})?;
            marker = marker.successor(
                Stage8bP1eFirstBootTransactionPhaseV4::Adopted,
                marker.canonical_root_identity_sha256.clone(),
                commitment_key,
            )?;
            replace_marker(&parent, &marker, commitment_key, &mut |_| {})?;
        }
        Stage8bP1eAdoptionRecoveryActionV5::AdoptCommittedRoot => {
            if marker_temp.is_some() || receipt.is_some() || receipt_temp.is_some() {
                return Err(Stage8bP1eFirstBootTransactionError::RecoverySelectorMismatch);
            }
            commit_receipt(&parent, &expected_receipt, commitment_key, &mut |_| {})?;
            marker = marker.successor(
                Stage8bP1eFirstBootTransactionPhaseV4::Adopted,
                marker.canonical_root_identity_sha256.clone(),
                commitment_key,
            )?;
            replace_marker(&parent, &marker, commitment_key, &mut |_| {})?;
        }
        Stage8bP1eAdoptionRecoveryActionV5::StartSealCommittedToAdopted => {
            if marker_temp.is_some()
                || receipt.as_ref() != Some(&expected_receipt)
                || receipt_temp.is_some()
            {
                return Err(Stage8bP1eFirstBootTransactionError::RecoverySelectorMismatch);
            }
            marker = marker.successor(
                Stage8bP1eFirstBootTransactionPhaseV4::Adopted,
                marker.canonical_root_identity_sha256.clone(),
                commitment_key,
            )?;
            replace_marker(&parent, &marker, commitment_key, &mut |_| {})?;
        }
        Stage8bP1eAdoptionRecoveryActionV5::CompleteSealCommittedToAdopted => {
            let Some(next) = marker_temp else {
                return Err(Stage8bP1eFirstBootTransactionError::RecoverySelectorMismatch);
            };
            let expected_next = marker.successor(
                Stage8bP1eFirstBootTransactionPhaseV4::Adopted,
                marker.canonical_root_identity_sha256.clone(),
                commitment_key,
            )?;
            if next != expected_next
                || receipt.as_ref() != Some(&expected_receipt)
                || receipt_temp.is_some()
            {
                return Err(Stage8bP1eFirstBootTransactionError::RecoverySelectorMismatch);
            }
            replace_file(
                &parent,
                STAGE8B_P1E_TRANSACTION_MARKER_TEMP_FILE,
                STAGE8B_P1E_TRANSACTION_MARKER_FILE,
            )?;
            sync_directory(&parent)?;
            let reread: Stage8bP1eFirstBootTransactionMarkerV4 = read_canonical_authority(
                &parent,
                STAGE8B_P1E_TRANSACTION_MARKER_FILE,
                commitment_key,
                |value: &Stage8bP1eFirstBootTransactionMarkerV4, key| {
                    value.validate_authenticated(key)
                },
            )?;
            if reread != next {
                return Err(Stage8bP1eFirstBootTransactionError::Adoption);
            }
            marker = reread;
        }
    }
    cross_validate_adopted(&parent, &marker, &expected_receipt, commitment_key)?;
    Ok(Stage8bP1eFirstBootTransactionOutcomeV5 {
        owner: *owner,
        receipt: expected_receipt,
    })
}

/// Executes the sole ordinary-bootstrap path. Every supplied capability is
/// consumed once and the returned owner exists only after receipt V2 and the
/// mandatory adopted marker have both been persisted, fsynced and reread.
pub fn first_boot_stage8b_p1e_transaction_v5(
    prepared: Stage8bP1ePreparedFirstBootV1,
    admin: Stage8bP1FirstBootAdminCommand,
    bootstrap_attempt_generation: u64,
    commitment_key: &Stage5gLifecycleCommitmentKey,
) -> Result<Stage8bP1eFirstBootTransactionOutcomeV5, Stage8bP1eFirstBootTransactionError> {
    first_boot_stage8b_p1e_transaction_v5_with_observer(
        prepared,
        admin,
        bootstrap_attempt_generation,
        commitment_key,
        |_| {},
    )
}

/// Derives the only admissible generation for a new first-boot attempt from
/// authenticated retained quarantine evidence.  Callers cannot choose or
/// reuse an attempt generation.  The ordinary transaction repeats the same
/// history validation immediately before its first durable mutation.
pub(crate) fn next_stage8b_p1e_bootstrap_attempt_generation_v5(
    config: &Stage8bP1ValidatedBootstrapConfig,
    commitment_key: &Stage5gLifecycleCommitmentKey,
) -> Result<u64, Stage8bP1eFirstBootTransactionError> {
    maximum_authenticated_quarantine_generation(config, commitment_key)?
        .checked_add(1)
        .ok_or(Stage8bP1eFirstBootTransactionError::InvalidGeneration)
}

fn first_boot_stage8b_p1e_transaction_v5_with_observer<F>(
    prepared: Stage8bP1ePreparedFirstBootV1,
    admin: Stage8bP1FirstBootAdminCommand,
    bootstrap_attempt_generation: u64,
    commitment_key: &Stage5gLifecycleCommitmentKey,
    mut observer: F,
) -> Result<Stage8bP1eFirstBootTransactionOutcomeV5, Stage8bP1eFirstBootTransactionError>
where
    F: FnMut(&'static str),
{
    if bootstrap_attempt_generation == 0 {
        return Err(Stage8bP1eFirstBootTransactionError::InvalidGeneration);
    }
    let (config, source, export_input, fresh_runtime, provenance) = prepared.into_parts();
    if !admin.matches(&config)
        || provenance.operational_identity_sha256() != config.operational_identity_sha256()
        || provenance.runtime_config_fingerprint_sha256()
            != config.runtime_config_fingerprint_sha256()
    {
        return Err(Stage8bP1eFirstBootTransactionError::InvalidAuthority);
    }
    validate_initial_source(&config, &source, &fresh_runtime)
        .map_err(|_| Stage8bP1eFirstBootTransactionError::InvalidAuthority)?;

    let stage5g_seed = export_stage5g_clean_restart(
        Stage5gCleanRestartSource::TimerReady(source),
        export_input,
        commitment_key,
    )
    .map_err(|_| Stage8bP1eFirstBootTransactionError::Package)?;
    drop(
        restore_stage5g_clean_restart(&stage5g_seed, commitment_key, fresh_runtime.clone())
            .map_err(|_| Stage8bP1eFirstBootTransactionError::Package)?,
    );

    let state_parent = config.durable_parent().to_path_buf();
    let root_path = state_parent.join(config.expected_root_name());
    require_pristine(&state_parent, &root_path)?;
    require_generation_after_quarantine_history(
        &config,
        bootstrap_attempt_generation,
        commitment_key,
    )?;
    let transaction_id_sha256 = transaction_id_sha256(
        provenance.operational_identity_sha256(),
        provenance.source_bundle_sha256(),
        provenance.source_bundle_generation(),
        bootstrap_attempt_generation,
    )?;
    let marker = Stage8bP1eFirstBootTransactionMarkerV4::new(
        transaction_id_sha256,
        bootstrap_attempt_generation,
        &provenance,
        UNBOUND_ROOT_IDENTITY_SHA256.to_string(),
        Stage8bP1eFirstBootTransactionPhaseV4::Prepared,
        1,
        commitment_key,
    )?;
    commit_initial_marker(&state_parent, &marker, commitment_key, &mut observer)?;
    continue_first_boot_from_prepared(
        config,
        marker,
        &stage5g_seed,
        provenance,
        commitment_key,
        fresh_runtime,
        &mut observer,
    )
}

#[allow(clippy::too_many_arguments)]
fn continue_first_boot_from_prepared<F>(
    config: Stage8bP1ValidatedBootstrapConfig,
    marker: Stage8bP1eFirstBootTransactionMarkerV4,
    stage5g_seed: &[u8],
    provenance: strategy_runtime_core::Stage8bP1eFirstBootProvenanceV1,
    commitment_key: &Stage5gLifecycleCommitmentKey,
    fresh_runtime: strategy_runtime_core::HybridIntradayRuntimeStrategy,
    observer: &mut F,
) -> Result<Stage8bP1eFirstBootTransactionOutcomeV5, Stage8bP1eFirstBootTransactionError>
where
    F: FnMut(&'static str),
{
    if marker.phase != Stage8bP1eFirstBootTransactionPhaseV4::Prepared {
        return Err(Stage8bP1eFirstBootTransactionError::RecoverySelectorMismatch);
    }
    let state_parent = config.durable_parent().to_path_buf();
    let root_path = state_parent.join(config.expected_root_name());
    let mut builder = fs::DirBuilder::new();
    builder.mode(0o700);
    builder
        .create(&root_path)
        .map_err(|error| Stage8bP1eFirstBootTransactionError::Filesystem(error.kind()))?;
    sync_directory(&state_parent)?;
    let root_identity = canonical_root_identity_sha256(&root_path)?;
    let marker = marker.successor(
        Stage8bP1eFirstBootTransactionPhaseV4::RootPublished,
        root_identity,
        commitment_key,
    )?;
    replace_marker(&state_parent, &marker, commitment_key, observer)?;
    observer("after-root-parent-fsync-before-journal-create");
    continue_first_boot_from_root_published(
        config,
        marker,
        stage5g_seed,
        provenance,
        commitment_key,
        fresh_runtime,
        observer,
    )
}

#[allow(clippy::too_many_arguments)]
fn continue_first_boot_from_root_published<F>(
    config: Stage8bP1ValidatedBootstrapConfig,
    marker: Stage8bP1eFirstBootTransactionMarkerV4,
    stage5g_seed: &[u8],
    provenance: strategy_runtime_core::Stage8bP1eFirstBootProvenanceV1,
    commitment_key: &Stage5gLifecycleCommitmentKey,
    fresh_runtime: strategy_runtime_core::HybridIntradayRuntimeStrategy,
    observer: &mut F,
) -> Result<Stage8bP1eFirstBootTransactionOutcomeV5, Stage8bP1eFirstBootTransactionError>
where
    F: FnMut(&'static str),
{
    if marker.phase != Stage8bP1eFirstBootTransactionPhaseV4::RootPublished {
        return Err(Stage8bP1eFirstBootTransactionError::RecoverySelectorMismatch);
    }
    let parent = config.durable_parent().to_path_buf();
    let root_path = parent.join(config.expected_root_name());
    if canonical_root_identity_sha256(&root_path)? != marker.canonical_root_identity_sha256
        || path_exists(root_path.join(STAGE7B_JOURNAL_FILE))?
        || path_exists(root_path.join(STAGE7B_RECOVERY_SEAL_FILE))?
    {
        return Err(Stage8bP1eFirstBootTransactionError::RecoverySelectorMismatch);
    }
    let root = Stage7bDurableRootAuthority::validate(&root_path, config.operational_identity())
        .map_err(|_| Stage8bP1eFirstBootTransactionError::Durable)?;
    let authorization = first_boot_authorization(&config)?;
    let journal_durable = Stage7bRecoveryReadyOwner::begin_stage8b_p1e_first_boot(
        root,
        config.operational_identity().clone(),
        authorization,
        stage5g_seed,
        provenance,
        commitment_key,
        fresh_runtime,
    )
    .map_err(|_| Stage8bP1eFirstBootTransactionError::Durable)?;
    let marker = marker.successor(
        Stage8bP1eFirstBootTransactionPhaseV4::JournalDurable,
        marker.canonical_root_identity_sha256.clone(),
        commitment_key,
    )?;
    replace_marker(&parent, &marker, commitment_key, observer)?;
    observer("after-journal-fsync-before-initial-seal-commit");
    continue_first_boot_from_journal_durable(
        parent,
        marker,
        journal_durable,
        commitment_key,
        observer,
    )
}

#[allow(clippy::too_many_arguments)]
fn continue_first_boot_from_existing_journal<F>(
    config: Stage8bP1ValidatedBootstrapConfig,
    marker: Stage8bP1eFirstBootTransactionMarkerV4,
    stage5g_seed: &[u8],
    provenance: strategy_runtime_core::Stage8bP1eFirstBootProvenanceV1,
    commitment_key: &Stage5gLifecycleCommitmentKey,
    fresh_runtime: strategy_runtime_core::HybridIntradayRuntimeStrategy,
    observer: &mut F,
) -> Result<Stage8bP1eFirstBootTransactionOutcomeV5, Stage8bP1eFirstBootTransactionError>
where
    F: FnMut(&'static str),
{
    if marker.phase != Stage8bP1eFirstBootTransactionPhaseV4::JournalDurable {
        return Err(Stage8bP1eFirstBootTransactionError::RecoverySelectorMismatch);
    }
    let parent = config.durable_parent().to_path_buf();
    let root_path = parent.join(config.expected_root_name());
    if canonical_root_identity_sha256(&root_path)? != marker.canonical_root_identity_sha256
        || !path_exists(root_path.join(STAGE7B_JOURNAL_FILE))?
        || path_exists(root_path.join(STAGE7B_RECOVERY_SEAL_FILE))?
    {
        return Err(Stage8bP1eFirstBootTransactionError::RecoverySelectorMismatch);
    }
    let root = Stage7bDurableRootAuthority::validate(&root_path, config.operational_identity())
        .map_err(|_| Stage8bP1eFirstBootTransactionError::Durable)?;
    let authorization = first_boot_authorization(&config)?;
    let journal_durable = Stage7bRecoveryReadyOwner::resume_stage8b_p1e_journal_durable_first_boot(
        root,
        config.operational_identity().clone(),
        authorization,
        stage5g_seed,
        provenance,
        commitment_key,
        fresh_runtime,
    )
    .map_err(|_| Stage8bP1eFirstBootTransactionError::Durable)?;
    observer("after-existing-journal-reopened-before-initial-seal-commit");
    continue_first_boot_from_journal_durable(
        parent,
        marker,
        journal_durable,
        commitment_key,
        observer,
    )
}

fn continue_first_boot_from_journal_durable<F>(
    parent: PathBuf,
    marker: Stage8bP1eFirstBootTransactionMarkerV4,
    journal_durable: crate::recovery::Stage7bP1eJournalDurableFirstBoot,
    commitment_key: &Stage5gLifecycleCommitmentKey,
    observer: &mut F,
) -> Result<Stage8bP1eFirstBootTransactionOutcomeV5, Stage8bP1eFirstBootTransactionError>
where
    F: FnMut(&'static str),
{
    let owner = journal_durable
        .commit_initial_seal(commitment_key)
        .map_err(|_| Stage8bP1eFirstBootTransactionError::Durable)?;
    let marker = marker.successor(
        Stage8bP1eFirstBootTransactionPhaseV4::SealCommitted,
        marker.canonical_root_identity_sha256.clone(),
        commitment_key,
    )?;
    replace_marker(&parent, &marker, commitment_key, observer)?;
    observer("after-seal-persist-reread-before-bootstrap-success-report");
    adopt_ready_owner(parent, marker, owner, commitment_key, observer)
}

fn adopt_ready_owner<F>(
    parent: PathBuf,
    marker: Stage8bP1eFirstBootTransactionMarkerV4,
    mut owner: Stage7bRecoveryReadyOwner,
    commitment_key: &Stage5gLifecycleCommitmentKey,
    observer: &mut F,
) -> Result<Stage8bP1eFirstBootTransactionOutcomeV5, Stage8bP1eFirstBootTransactionError>
where
    F: FnMut(&'static str),
{
    if marker.phase != Stage8bP1eFirstBootTransactionPhaseV4::SealCommitted {
        return Err(Stage8bP1eFirstBootTransactionError::RecoverySelectorMismatch);
    }
    let adoption = owner
        .stage8b_p1e_adoption_material(commitment_key)
        .map_err(|_| Stage8bP1eFirstBootTransactionError::Adoption)?;
    if !marker_matches_adoption(&marker, &adoption) {
        return Err(Stage8bP1eFirstBootTransactionError::Adoption);
    }
    let receipt = Stage8bP1FirstBootReceiptV2::from_adoption(
        &marker,
        marker.canonical_root_identity_sha256.clone(),
        &adoption,
        commitment_key,
    )?;
    commit_receipt(&parent, &receipt, commitment_key, observer)?;
    observer("after-receipt-rename-parent-fsync-before-adopted-marker-temp-create");
    let marker = marker.successor(
        Stage8bP1eFirstBootTransactionPhaseV4::Adopted,
        receipt.canonical_root_identity_sha256.clone(),
        commitment_key,
    )?;
    replace_marker(&parent, &marker, commitment_key, observer)?;
    cross_validate_adopted(&parent, &marker, &receipt, commitment_key)?;
    Ok(Stage8bP1eFirstBootTransactionOutcomeV5 { owner, receipt })
}

fn first_boot_authorization(
    config: &Stage8bP1ValidatedBootstrapConfig,
) -> Result<strategy_runtime_core::Stage6dFirstBootAuthorization, Stage8bP1eFirstBootTransactionError>
{
    authorize_stage6d_first_boot(Stage6dFirstBootConfig {
        deployment_id: config.operational_identity().deployment_id.clone(),
        expected_runtime_config_fingerprint_sha256: config
            .runtime_config_fingerprint_sha256()
            .to_string(),
        allow_create_missing_journal: true,
    })
    .map_err(|_| Stage8bP1eFirstBootTransactionError::Durable)
}

fn recovery_bootstrap_generation(
    parent: &Path,
    commitment_key: &Stage5gLifecycleCommitmentKey,
    expected_transaction_id_sha256: &str,
) -> Result<u64, Stage8bP1eFirstBootTransactionError> {
    let marker =
        optional_authenticated_marker(parent, STAGE8B_P1E_TRANSACTION_MARKER_FILE, commitment_key)?;
    let marker_temp = optional_authenticated_marker(
        parent,
        STAGE8B_P1E_TRANSACTION_MARKER_TEMP_FILE,
        commitment_key,
    )?;
    let authority = marker
        .as_ref()
        .or(marker_temp.as_ref())
        .ok_or(Stage8bP1eFirstBootTransactionError::RecoverySelectorMismatch)?;
    if authority.transaction_id_sha256 != expected_transaction_id_sha256
        || marker
            .as_ref()
            .is_some_and(|value| value.transaction_id_sha256 != expected_transaction_id_sha256)
        || marker_temp
            .as_ref()
            .is_some_and(|value| value.transaction_id_sha256 != expected_transaction_id_sha256)
    {
        return Err(Stage8bP1eFirstBootTransactionError::RecoverySelectorMismatch);
    }
    Ok(authority.bootstrap_attempt_generation)
}

fn marker_matches_durable_recovery(
    marker: &Stage8bP1eFirstBootTransactionMarkerV4,
    config: &Stage8bP1ValidatedBootstrapConfig,
    expected_transaction_id_sha256: &str,
) -> bool {
    marker.transaction_id_sha256 == expected_transaction_id_sha256
        && marker.operational_identity_sha256 == config.operational_identity_sha256()
        && marker.runtime_profile_sha256 == STAGE8B_P1E_RUNTIME_PROFILE_SHA256
        && marker.runtime_config_fingerprint_sha256 == config.runtime_config_fingerprint_sha256()
        && marker.source_plan_sha256 == STAGE8B_P1E_FIRST_BOOT_SOURCE_PLAN_V2_SHA256
        && transaction_id_sha256(
            &marker.operational_identity_sha256,
            &marker.source_bundle_sha256,
            marker.source_bundle_generation,
            marker.bootstrap_attempt_generation,
        )
        .is_ok_and(|derived| derived == marker.transaction_id_sha256)
}

fn read_expected_durable_recovery_marker(
    config: &Stage8bP1ValidatedBootstrapConfig,
    commitment_key: &Stage5gLifecycleCommitmentKey,
    expected_transaction_id_sha256: &str,
    expected_phase: Stage8bP1eFirstBootTransactionPhaseV4,
) -> Result<Stage8bP1eFirstBootTransactionMarkerV4, Stage8bP1eFirstBootTransactionError> {
    let marker: Stage8bP1eFirstBootTransactionMarkerV4 = read_canonical_authority(
        config.durable_parent(),
        STAGE8B_P1E_TRANSACTION_MARKER_FILE,
        commitment_key,
        |value: &Stage8bP1eFirstBootTransactionMarkerV4, key| value.validate_authenticated(key),
    )?;
    if marker.phase != expected_phase
        || !marker_matches_durable_recovery(&marker, config, expected_transaction_id_sha256)
    {
        return Err(Stage8bP1eFirstBootTransactionError::RecoverySelectorMismatch);
    }
    Ok(marker)
}

fn historical_source_binding_v5(
    config: &Stage8bP1ValidatedBootstrapConfig,
    selector: &Stage8bP1ePreSealRecoverySelectorV5,
    commitment_key: &Stage5gLifecycleCommitmentKey,
) -> Result<Stage8bP1eHistoricalSourceBindingV5, Stage8bP1eFirstBootTransactionError> {
    if selector.action.is_administrative()
        || selector.operational_identity_sha256 != config.operational_identity_sha256()
        || selector.runtime_config_fingerprint_sha256 != config.runtime_config_fingerprint_sha256()
    {
        return Err(Stage8bP1eFirstBootTransactionError::RecoverySelectorMismatch);
    }
    let marker = optional_authenticated_marker(
        config.durable_parent(),
        STAGE8B_P1E_TRANSACTION_MARKER_FILE,
        commitment_key,
    )?;
    let marker_temp = optional_authenticated_marker(
        config.durable_parent(),
        STAGE8B_P1E_TRANSACTION_MARKER_TEMP_FILE,
        commitment_key,
    )?;
    let authority = marker
        .as_ref()
        .or(marker_temp.as_ref())
        .ok_or(Stage8bP1eFirstBootTransactionError::RecoverySelectorMismatch)?;
    if !marker.iter().chain(marker_temp.iter()).all(|value| {
        marker_matches_durable_recovery(value, config, &selector.transaction_id_sha256)
    }) {
        return Err(Stage8bP1eFirstBootTransactionError::RecoverySelectorMismatch);
    }
    if let (Some(old), Some(next)) = (&marker, &marker_temp) {
        let expected = old.successor(
            next.phase,
            next.canonical_root_identity_sha256.clone(),
            commitment_key,
        )?;
        if *next != expected {
            return Err(Stage8bP1eFirstBootTransactionError::RecoverySelectorMismatch);
        }
    }
    Ok(Stage8bP1eHistoricalSourceBindingV5 {
        operational_identity_sha256: authority.operational_identity_sha256.clone(),
        runtime_config_fingerprint_sha256: authority.runtime_config_fingerprint_sha256.clone(),
        source_bundle_sha256: authority.source_bundle_sha256.clone(),
        source_bundle_generation: authority.source_bundle_generation,
        history_bars_sha256: authority.history_bars_sha256.clone(),
        riskgate_session_observations_sha256: authority
            .riskgate_session_observations_sha256
            .clone(),
        candidate_semantic_id_sha256: authority.candidate_semantic_id_sha256.clone(),
    })
}

fn marker_matches_prepared_recovery(
    marker: &Stage8bP1eFirstBootTransactionMarkerV4,
    config: &Stage8bP1ValidatedBootstrapConfig,
    expected_transaction_id_sha256: &str,
    provenance: &strategy_runtime_core::Stage8bP1eFirstBootProvenanceV1,
) -> bool {
    marker_matches_durable_recovery(marker, config, expected_transaction_id_sha256)
        && marker.operational_identity_sha256 == provenance.operational_identity_sha256()
        && marker.runtime_profile_sha256 == provenance.runtime_profile_sha256()
        && marker.runtime_config_fingerprint_sha256
            == provenance.runtime_config_fingerprint_sha256()
        && marker.source_bundle_sha256 == provenance.source_bundle_sha256()
        && marker.source_bundle_generation == provenance.source_bundle_generation()
        && marker.source_plan_sha256 == provenance.source_plan_sha256()
        && marker.history_bars_sha256 == provenance.history_bars_sha256()
        && marker.riskgate_session_observations_sha256
            == provenance.riskgate_session_observations_sha256()
        && marker.candidate_semantic_id_sha256 == provenance.candidate_semantic_id_sha256()
}

fn read_expected_recovery_marker(
    config: &Stage8bP1ValidatedBootstrapConfig,
    commitment_key: &Stage5gLifecycleCommitmentKey,
    expected_transaction_id_sha256: &str,
    provenance: &strategy_runtime_core::Stage8bP1eFirstBootProvenanceV1,
    expected_phase: Stage8bP1eFirstBootTransactionPhaseV4,
) -> Result<Stage8bP1eFirstBootTransactionMarkerV4, Stage8bP1eFirstBootTransactionError> {
    let marker: Stage8bP1eFirstBootTransactionMarkerV4 = read_canonical_authority(
        config.durable_parent(),
        STAGE8B_P1E_TRANSACTION_MARKER_FILE,
        commitment_key,
        |value: &Stage8bP1eFirstBootTransactionMarkerV4, key| value.validate_authenticated(key),
    )?;
    if marker.phase != expected_phase
        || !marker_matches_prepared_recovery(
            &marker,
            config,
            expected_transaction_id_sha256,
            provenance,
        )
    {
        return Err(Stage8bP1eFirstBootTransactionError::RecoverySelectorMismatch);
    }
    Ok(marker)
}

#[allow(clippy::too_many_arguments)]
fn complete_marker_temp_transition<F>(
    config: &Stage8bP1ValidatedBootstrapConfig,
    commitment_key: &Stage5gLifecycleCommitmentKey,
    expected_transaction_id_sha256: &str,
    provenance: &strategy_runtime_core::Stage8bP1eFirstBootProvenanceV1,
    expected_old_phase: Stage8bP1eFirstBootTransactionPhaseV4,
    expected_next_phase: Stage8bP1eFirstBootTransactionPhaseV4,
    observer: &mut F,
) -> Result<Stage8bP1eFirstBootTransactionMarkerV4, Stage8bP1eFirstBootTransactionError>
where
    F: FnMut(&'static str),
{
    let parent = config.durable_parent();
    let old = read_expected_recovery_marker(
        config,
        commitment_key,
        expected_transaction_id_sha256,
        provenance,
        expected_old_phase,
    )?;
    let next: Stage8bP1eFirstBootTransactionMarkerV4 = read_canonical_authority(
        parent,
        STAGE8B_P1E_TRANSACTION_MARKER_TEMP_FILE,
        commitment_key,
        |value: &Stage8bP1eFirstBootTransactionMarkerV4, key| value.validate_authenticated(key),
    )?;
    let expected = old.successor(
        expected_next_phase,
        next.canonical_root_identity_sha256.clone(),
        commitment_key,
    )?;
    if next != expected
        || !marker_matches_prepared_recovery(
            &next,
            config,
            expected_transaction_id_sha256,
            provenance,
        )
    {
        return Err(Stage8bP1eFirstBootTransactionError::RecoverySelectorMismatch);
    }
    validate_marker_transition_effect(config, expected_next_phase, &next)?;
    replace_file(
        parent,
        STAGE8B_P1E_TRANSACTION_MARKER_TEMP_FILE,
        STAGE8B_P1E_TRANSACTION_MARKER_FILE,
    )?;
    observer("after-pre-seal-marker-temp-rename-before-parent-fsync");
    sync_directory(parent)?;
    let reread: Stage8bP1eFirstBootTransactionMarkerV4 = read_canonical_authority(
        parent,
        STAGE8B_P1E_TRANSACTION_MARKER_FILE,
        commitment_key,
        |value: &Stage8bP1eFirstBootTransactionMarkerV4, key| value.validate_authenticated(key),
    )?;
    if reread != next {
        return Err(Stage8bP1eFirstBootTransactionError::InvalidAuthorityFile);
    }
    Ok(reread)
}

fn validate_marker_transition_effect(
    config: &Stage8bP1ValidatedBootstrapConfig,
    phase: Stage8bP1eFirstBootTransactionPhaseV4,
    marker: &Stage8bP1eFirstBootTransactionMarkerV4,
) -> Result<(), Stage8bP1eFirstBootTransactionError> {
    let root = config.durable_parent().join(config.expected_root_name());
    validate_owned_directory(&root)?;
    if canonical_root_identity_sha256(&root)? != marker.canonical_root_identity_sha256 {
        return Err(Stage8bP1eFirstBootTransactionError::RecoverySelectorMismatch);
    }
    let journal = root.join(STAGE7B_JOURNAL_FILE);
    let seal = root.join(STAGE7B_RECOVERY_SEAL_FILE);
    let journal_exists = path_exists(journal.clone())?;
    let seal_exists = path_exists(seal.clone())?;
    if journal_exists {
        validate_owned_regular_file(&journal)?;
    }
    if seal_exists {
        validate_owned_regular_file(&seal)?;
    }
    let exact = match phase {
        Stage8bP1eFirstBootTransactionPhaseV4::RootPublished => !journal_exists && !seal_exists,
        Stage8bP1eFirstBootTransactionPhaseV4::JournalDurable => journal_exists && !seal_exists,
        Stage8bP1eFirstBootTransactionPhaseV4::SealCommitted => journal_exists && seal_exists,
        Stage8bP1eFirstBootTransactionPhaseV4::Prepared
        | Stage8bP1eFirstBootTransactionPhaseV4::Adopted => false,
    };
    if !exact {
        return Err(Stage8bP1eFirstBootTransactionError::RecoverySelectorMismatch);
    }
    Ok(())
}

fn quarantine_incomplete_root<F>(
    parent: &Path,
    root_path: &Path,
    marker: &Stage8bP1eFirstBootTransactionMarkerV4,
    expected_root_name: &str,
    observer: &mut F,
) -> Result<(), Stage8bP1eFirstBootTransactionError>
where
    F: FnMut(&'static str),
{
    if !matches!(
        marker.phase,
        Stage8bP1eFirstBootTransactionPhaseV4::RootPublished
            | Stage8bP1eFirstBootTransactionPhaseV4::JournalDurable
    ) || canonical_root_identity_sha256(root_path)? != marker.canonical_root_identity_sha256
    {
        return Err(Stage8bP1eFirstBootTransactionError::RecoverySelectorMismatch);
    }
    validate_owned_directory(root_path)?;
    let journal = root_path.join(STAGE7B_JOURNAL_FILE);
    let seal = root_path.join(STAGE7B_RECOVERY_SEAL_FILE);
    let journal_exists = path_exists(journal.clone())?;
    if path_exists(seal)?
        || (marker.phase == Stage8bP1eFirstBootTransactionPhaseV4::RootPublished && journal_exists)
        || (marker.phase == Stage8bP1eFirstBootTransactionPhaseV4::JournalDurable
            && !journal_exists)
    {
        return Err(Stage8bP1eFirstBootTransactionError::RecoverySelectorMismatch);
    }
    if journal_exists {
        validate_owned_regular_file(&journal)?;
    }
    let quarantine_parent = parent.join(STAGE8B_P1E_FIRST_BOOT_QUARANTINE_DIRECTORY);
    validate_owned_directory(&quarantine_parent)?;
    rename_noreplace_between(
        parent,
        expected_root_name,
        &quarantine_parent,
        &marker.transaction_id_sha256,
    )?;
    observer("after-quarantine-root-rename-before-parent-fsync");
    sync_directory(parent)?;
    sync_directory(&quarantine_parent)?;
    let quarantine_root = quarantine_parent.join(&marker.transaction_id_sha256);
    if canonical_root_identity_sha256_with_basename(&quarantine_root, expected_root_name)?
        != marker.canonical_root_identity_sha256
    {
        return Err(Stage8bP1eFirstBootTransactionError::InvalidAuthorityFile);
    }
    Ok(())
}

fn finalize_quarantine<F>(
    parent: &Path,
    marker: &Stage8bP1eFirstBootTransactionMarkerV4,
    commitment_key: &Stage5gLifecycleCommitmentKey,
    observer: &mut F,
) -> Result<(), Stage8bP1eFirstBootTransactionError>
where
    F: FnMut(&'static str),
{
    let quarantine_parent = parent.join(STAGE8B_P1E_FIRST_BOOT_QUARANTINE_DIRECTORY);
    let quarantine_root = quarantine_parent.join(&marker.transaction_id_sha256);
    validate_owned_directory(&quarantine_parent)?;
    validate_owned_directory(&quarantine_root)?;
    if path_exists(quarantine_root.join(STAGE7B_RECOVERY_SEAL_FILE))?
        || path_exists(quarantine_root.join(STAGE8B_P1E_TRANSACTION_MARKER_FILE))?
    {
        return Err(Stage8bP1eFirstBootTransactionError::RecoverySelectorMismatch);
    }
    sync_directory(&quarantine_root)?;
    rename_noreplace_between(
        parent,
        STAGE8B_P1E_TRANSACTION_MARKER_FILE,
        &quarantine_root,
        STAGE8B_P1E_TRANSACTION_MARKER_FILE,
    )?;
    observer("after-finalize-quarantine-marker-rename-before-parent-fsync");
    sync_directory(parent)?;
    sync_directory(&quarantine_root)?;
    let retained: Stage8bP1eFirstBootTransactionMarkerV4 = read_canonical_authority(
        &quarantine_root,
        STAGE8B_P1E_TRANSACTION_MARKER_FILE,
        commitment_key,
        |value: &Stage8bP1eFirstBootTransactionMarkerV4, key| value.validate_authenticated(key),
    )?;
    if retained != *marker {
        return Err(Stage8bP1eFirstBootTransactionError::InvalidAuthorityFile);
    }
    Ok(())
}

fn require_generation_after_quarantine_history(
    config: &Stage8bP1ValidatedBootstrapConfig,
    bootstrap_attempt_generation: u64,
    commitment_key: &Stage5gLifecycleCommitmentKey,
) -> Result<(), Stage8bP1eFirstBootTransactionError> {
    let maximum_generation = maximum_authenticated_quarantine_generation(config, commitment_key)?;
    if bootstrap_attempt_generation <= maximum_generation {
        return Err(Stage8bP1eFirstBootTransactionError::InvalidGeneration);
    }
    Ok(())
}

fn maximum_authenticated_quarantine_generation(
    config: &Stage8bP1ValidatedBootstrapConfig,
    commitment_key: &Stage5gLifecycleCommitmentKey,
) -> Result<u64, Stage8bP1eFirstBootTransactionError> {
    let quarantine_parent = config
        .durable_parent()
        .join(STAGE8B_P1E_FIRST_BOOT_QUARANTINE_DIRECTORY);
    if !path_exists(quarantine_parent.clone())? {
        return Ok(0);
    }
    validate_owned_directory(&quarantine_parent)?;
    let mut maximum_generation = 0_u64;
    let entries = fs::read_dir(&quarantine_parent)
        .map_err(|error| Stage8bP1eFirstBootTransactionError::Filesystem(error.kind()))?;
    for entry in entries {
        let entry =
            entry.map_err(|error| Stage8bP1eFirstBootTransactionError::Filesystem(error.kind()))?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| Stage8bP1eFirstBootTransactionError::InvalidAuthorityFile)?;
        if !is_sha256(&name) {
            return Err(Stage8bP1eFirstBootTransactionError::InvalidAuthorityFile);
        }
        let root = entry.path();
        validate_owned_directory(&root)?;
        let marker: Stage8bP1eFirstBootTransactionMarkerV4 = read_canonical_authority(
            &root,
            STAGE8B_P1E_TRANSACTION_MARKER_FILE,
            commitment_key,
            |value: &Stage8bP1eFirstBootTransactionMarkerV4, key| value.validate_authenticated(key),
        )?;
        if marker.transaction_id_sha256 != name
            || marker.operational_identity_sha256 != config.operational_identity_sha256()
            || !matches!(
                marker.phase,
                Stage8bP1eFirstBootTransactionPhaseV4::RootPublished
                    | Stage8bP1eFirstBootTransactionPhaseV4::JournalDurable
            )
            || canonical_root_identity_sha256_with_basename(&root, config.expected_root_name())?
                != marker.canonical_root_identity_sha256
            || path_exists(root.join(STAGE7B_RECOVERY_SEAL_FILE))?
        {
            return Err(Stage8bP1eFirstBootTransactionError::InvalidAuthorityFile);
        }
        maximum_generation = maximum_generation.max(marker.bootstrap_attempt_generation);
    }
    Ok(maximum_generation)
}

fn rename_noreplace_between(
    from_parent: &Path,
    from: &str,
    to_parent: &Path,
    to: &str,
) -> Result<(), Stage8bP1eFirstBootTransactionError> {
    if path_exists(to_parent.join(to))? {
        return Err(Stage8bP1eFirstBootTransactionError::NotPristine);
    }
    #[cfg(target_os = "linux")]
    {
        let from_directory = open_directory(from_parent)?;
        let to_directory = open_directory(to_parent)?;
        let from = CString::new(from)
            .map_err(|_| Stage8bP1eFirstBootTransactionError::InvalidAuthorityFile)?;
        let to = CString::new(to)
            .map_err(|_| Stage8bP1eFirstBootTransactionError::InvalidAuthorityFile)?;
        let result = unsafe {
            libc::syscall(
                libc::SYS_renameat2,
                from_directory.as_raw_fd(),
                from.as_ptr(),
                to_directory.as_raw_fd(),
                to.as_ptr(),
                libc::RENAME_NOREPLACE,
            )
        };
        if result != 0 {
            return Err(Stage8bP1eFirstBootTransactionError::Filesystem(
                std::io::Error::last_os_error().kind(),
            ));
        }
        return Ok(());
    }
    #[cfg(not(target_os = "linux"))]
    {
        fs::rename(from_parent.join(from), to_parent.join(to))
            .map_err(|error| Stage8bP1eFirstBootTransactionError::Filesystem(error.kind()))
    }
}

#[cfg(test)]
pub(crate) fn test_first_boot_stage8b_p1e_transaction_v5_with_observer<F>(
    prepared: Stage8bP1ePreparedFirstBootV1,
    admin: Stage8bP1FirstBootAdminCommand,
    bootstrap_attempt_generation: u64,
    commitment_key: &Stage5gLifecycleCommitmentKey,
    observer: F,
) -> Result<Stage8bP1eFirstBootTransactionOutcomeV5, Stage8bP1eFirstBootTransactionError>
where
    F: FnMut(&'static str),
{
    first_boot_stage8b_p1e_transaction_v5_with_observer(
        prepared,
        admin,
        bootstrap_attempt_generation,
        commitment_key,
        observer,
    )
}

impl Stage8bP1eFirstBootTransactionMarkerV4 {
    pub fn transaction_id_sha256(&self) -> &str {
        &self.transaction_id_sha256
    }

    pub const fn phase(&self) -> Stage8bP1eFirstBootTransactionPhaseV4 {
        self.phase
    }

    #[allow(clippy::too_many_arguments)]
    fn new(
        transaction_id_sha256: String,
        bootstrap_attempt_generation: u64,
        provenance: &strategy_runtime_core::Stage8bP1eFirstBootProvenanceV1,
        canonical_root_identity_sha256: String,
        phase: Stage8bP1eFirstBootTransactionPhaseV4,
        marker_generation: u64,
        key: &Stage5gLifecycleCommitmentKey,
    ) -> Result<Self, Stage8bP1eFirstBootTransactionError> {
        let mut value = Self {
            schema_version: STAGE8B_P1E_TRANSACTION_MARKER_SCHEMA_VERSION,
            domain: TRANSACTION_MARKER_DOMAIN.to_string(),
            transaction_id_sha256,
            bootstrap_attempt_generation,
            operational_identity_sha256: provenance.operational_identity_sha256().to_string(),
            runtime_profile_sha256: provenance.runtime_profile_sha256().to_string(),
            runtime_config_fingerprint_sha256: provenance
                .runtime_config_fingerprint_sha256()
                .to_string(),
            source_bundle_sha256: provenance.source_bundle_sha256().to_string(),
            source_bundle_generation: provenance.source_bundle_generation(),
            source_plan_sha256: provenance.source_plan_sha256().to_string(),
            history_bars_sha256: provenance.history_bars_sha256().to_string(),
            riskgate_session_observations_sha256: provenance
                .riskgate_session_observations_sha256()
                .to_string(),
            candidate_semantic_id_sha256: provenance.candidate_semantic_id_sha256().to_string(),
            canonical_root_identity_sha256,
            phase,
            marker_generation,
            marker_hmac_sha256: String::new(),
        };
        value.validate_fields()?;
        value.marker_hmac_sha256 = key.stage8b_p1e_framed_hmac_sha256(&value.hmac_preimage()?);
        Ok(value)
    }

    fn successor(
        &self,
        phase: Stage8bP1eFirstBootTransactionPhaseV4,
        canonical_root_identity_sha256: String,
        key: &Stage5gLifecycleCommitmentKey,
    ) -> Result<Self, Stage8bP1eFirstBootTransactionError> {
        let expected = match self.phase {
            Stage8bP1eFirstBootTransactionPhaseV4::Prepared => {
                Stage8bP1eFirstBootTransactionPhaseV4::RootPublished
            }
            Stage8bP1eFirstBootTransactionPhaseV4::RootPublished => {
                Stage8bP1eFirstBootTransactionPhaseV4::JournalDurable
            }
            Stage8bP1eFirstBootTransactionPhaseV4::JournalDurable => {
                Stage8bP1eFirstBootTransactionPhaseV4::SealCommitted
            }
            Stage8bP1eFirstBootTransactionPhaseV4::SealCommitted => {
                Stage8bP1eFirstBootTransactionPhaseV4::Adopted
            }
            Stage8bP1eFirstBootTransactionPhaseV4::Adopted => {
                return Err(Stage8bP1eFirstBootTransactionError::InvalidAuthorityFile)
            }
        };
        if phase != expected {
            return Err(Stage8bP1eFirstBootTransactionError::InvalidAuthorityFile);
        }
        let mut next = self.clone();
        next.phase = phase;
        next.marker_generation = next
            .marker_generation
            .checked_add(1)
            .ok_or(Stage8bP1eFirstBootTransactionError::InvalidGeneration)?;
        next.canonical_root_identity_sha256 = canonical_root_identity_sha256;
        next.marker_hmac_sha256.clear();
        next.validate_fields()?;
        next.marker_hmac_sha256 = key.stage8b_p1e_framed_hmac_sha256(&next.hmac_preimage()?);
        Ok(next)
    }

    fn validate_authenticated(
        &self,
        key: &Stage5gLifecycleCommitmentKey,
    ) -> Result<(), Stage8bP1eFirstBootTransactionError> {
        self.validate_fields()?;
        if !is_sha256(&self.marker_hmac_sha256)
            || !key.stage8b_p1e_verify_framed_hmac_sha256(
                &self.hmac_preimage()?,
                &self.marker_hmac_sha256,
            )
        {
            return Err(Stage8bP1eFirstBootTransactionError::InvalidAuthorityFile);
        }
        Ok(())
    }

    fn validate_fields(&self) -> Result<(), Stage8bP1eFirstBootTransactionError> {
        let expected_generation = match self.phase {
            Stage8bP1eFirstBootTransactionPhaseV4::Prepared => 1,
            Stage8bP1eFirstBootTransactionPhaseV4::RootPublished => 2,
            Stage8bP1eFirstBootTransactionPhaseV4::JournalDurable => 3,
            Stage8bP1eFirstBootTransactionPhaseV4::SealCommitted => 4,
            Stage8bP1eFirstBootTransactionPhaseV4::Adopted => 5,
        };
        let digests = [
            &self.transaction_id_sha256,
            &self.operational_identity_sha256,
            &self.runtime_profile_sha256,
            &self.runtime_config_fingerprint_sha256,
            &self.source_bundle_sha256,
            &self.source_plan_sha256,
            &self.history_bars_sha256,
            &self.riskgate_session_observations_sha256,
            &self.candidate_semantic_id_sha256,
            &self.canonical_root_identity_sha256,
        ];
        if self.schema_version != STAGE8B_P1E_TRANSACTION_MARKER_SCHEMA_VERSION
            || self.domain != TRANSACTION_MARKER_DOMAIN
            || self.bootstrap_attempt_generation == 0
            || self.source_bundle_generation == 0
            || self.marker_generation != expected_generation
            || digests.into_iter().any(|value| !is_sha256(value))
            || (self.phase == Stage8bP1eFirstBootTransactionPhaseV4::Prepared
                && self.canonical_root_identity_sha256 != UNBOUND_ROOT_IDENTITY_SHA256)
            || (self.phase != Stage8bP1eFirstBootTransactionPhaseV4::Prepared
                && self.canonical_root_identity_sha256 == UNBOUND_ROOT_IDENTITY_SHA256)
        {
            return Err(Stage8bP1eFirstBootTransactionError::InvalidAuthorityFile);
        }
        Ok(())
    }

    fn hmac_preimage(&self) -> Result<Vec<u8>, Stage8bP1eFirstBootTransactionError> {
        let fields = vec![
            digest_field("transaction_id_sha256", &self.transaction_id_sha256)?,
            u64_field(
                "bootstrap_attempt_generation",
                self.bootstrap_attempt_generation,
            ),
            digest_field(
                "operational_identity_sha256",
                &self.operational_identity_sha256,
            )?,
            digest_field("runtime_profile_sha256", &self.runtime_profile_sha256)?,
            digest_field(
                "runtime_config_fingerprint_sha256",
                &self.runtime_config_fingerprint_sha256,
            )?,
            digest_field("source_bundle_sha256", &self.source_bundle_sha256)?,
            u64_field("source_bundle_generation", self.source_bundle_generation),
            digest_field("source_plan_sha256", &self.source_plan_sha256)?,
            digest_field("history_bars_sha256", &self.history_bars_sha256)?,
            digest_field(
                "riskgate_session_observations_sha256",
                &self.riskgate_session_observations_sha256,
            )?,
            digest_field(
                "candidate_semantic_id_sha256",
                &self.candidate_semantic_id_sha256,
            )?,
            digest_field(
                "canonical_root_identity_sha256",
                &self.canonical_root_identity_sha256,
            )?,
            ascii_field("phase", self.phase.as_str())?,
            u64_field("marker_generation", self.marker_generation),
        ];
        framed_record(
            STAGE8B_P1E_TRANSACTION_MARKER_SCHEMA_VERSION,
            TRANSACTION_MARKER_HMAC_DOMAIN,
            &fields,
        )
    }
}

impl Stage8bP1FirstBootReceiptV2 {
    fn from_adoption(
        marker: &Stage8bP1eFirstBootTransactionMarkerV4,
        canonical_root_identity_sha256: String,
        adoption: &Stage7bP1eAdoptionMaterial,
        key: &Stage5gLifecycleCommitmentKey,
    ) -> Result<Self, Stage8bP1eFirstBootTransactionError> {
        if !matches!(
            marker.phase,
            Stage8bP1eFirstBootTransactionPhaseV4::SealCommitted
                | Stage8bP1eFirstBootTransactionPhaseV4::Adopted
        ) || marker.canonical_root_identity_sha256 != canonical_root_identity_sha256
            || adoption.package.schema_version != 2
            || adoption.package.operational_identity_sha256 != marker.operational_identity_sha256
            || adoption.package.stage6_checkpoint_sha256 != adoption.stage6_checkpoint_sha256
        {
            return Err(Stage8bP1eFirstBootTransactionError::Adoption);
        }
        let adoption_ready_owner_sha256 = adoption_ready_owner_sha256(marker, adoption)?;
        let mut receipt = Self {
            schema_version: STAGE8B_P1E_FIRST_BOOT_RECEIPT_SCHEMA_VERSION,
            domain: RECEIPT_DOMAIN.to_string(),
            receipt_generation: 1,
            transaction_id_sha256: marker.transaction_id_sha256.clone(),
            bootstrap_attempt_generation: marker.bootstrap_attempt_generation,
            deployment_identity_sha256: STAGE8B_P1E_DEPLOYMENT_IDENTITY_V2_SHA256.to_string(),
            canonical_root_identity_sha256,
            restart_package_schema_version: adoption.package.schema_version,
            restart_package_canonical_sha256: adoption.package.canonical_sha256.clone(),
            first_boot_provenance_canonical_sha256: adoption
                .package
                .first_boot_provenance_canonical_sha256
                .clone(),
            seal_generation: adoption.seal_generation,
            seal_commitment_sha256: adoption.seal_commitment_sha256.clone(),
            stage6_checkpoint_sha256: adoption.stage6_checkpoint_sha256.clone(),
            adoption_predicate_version: STAGE8B_P1E_ADOPTION_PREDICATE_VERSION,
            adoption_ready_owner_sha256,
            receipt_hmac_sha256: String::new(),
        };
        receipt.validate_fields()?;
        receipt.receipt_hmac_sha256 = key.stage8b_p1e_framed_hmac_sha256(&receipt.hmac_preimage()?);
        Ok(receipt)
    }

    fn validate_authenticated(
        &self,
        key: &Stage5gLifecycleCommitmentKey,
    ) -> Result<(), Stage8bP1eFirstBootTransactionError> {
        self.validate_fields()?;
        if !is_sha256(&self.receipt_hmac_sha256)
            || !key.stage8b_p1e_verify_framed_hmac_sha256(
                &self.hmac_preimage()?,
                &self.receipt_hmac_sha256,
            )
        {
            return Err(Stage8bP1eFirstBootTransactionError::InvalidAuthorityFile);
        }
        Ok(())
    }

    fn validate_fields(&self) -> Result<(), Stage8bP1eFirstBootTransactionError> {
        let digests = [
            &self.transaction_id_sha256,
            &self.deployment_identity_sha256,
            &self.canonical_root_identity_sha256,
            &self.restart_package_canonical_sha256,
            &self.first_boot_provenance_canonical_sha256,
            &self.seal_commitment_sha256,
            &self.stage6_checkpoint_sha256,
            &self.adoption_ready_owner_sha256,
        ];
        if self.schema_version != STAGE8B_P1E_FIRST_BOOT_RECEIPT_SCHEMA_VERSION
            || self.domain != RECEIPT_DOMAIN
            || self.receipt_generation != 1
            || self.bootstrap_attempt_generation == 0
            || self.restart_package_schema_version != 2
            || self.seal_generation == 0
            || self.adoption_predicate_version != STAGE8B_P1E_ADOPTION_PREDICATE_VERSION
            || digests.into_iter().any(|value| !is_sha256(value))
        {
            return Err(Stage8bP1eFirstBootTransactionError::InvalidAuthorityFile);
        }
        Ok(())
    }

    fn hmac_preimage(&self) -> Result<Vec<u8>, Stage8bP1eFirstBootTransactionError> {
        let fields = vec![
            u64_field("receipt_generation", self.receipt_generation),
            digest_field("transaction_id_sha256", &self.transaction_id_sha256)?,
            u64_field(
                "bootstrap_attempt_generation",
                self.bootstrap_attempt_generation,
            ),
            digest_field(
                "deployment_identity_sha256",
                &self.deployment_identity_sha256,
            )?,
            digest_field(
                "canonical_root_identity_sha256",
                &self.canonical_root_identity_sha256,
            )?,
            u16_field(
                "restart_package_schema_version",
                self.restart_package_schema_version,
            ),
            digest_field(
                "restart_package_canonical_sha256",
                &self.restart_package_canonical_sha256,
            )?,
            digest_field(
                "first_boot_provenance_canonical_sha256",
                &self.first_boot_provenance_canonical_sha256,
            )?,
            u64_field("seal_generation", self.seal_generation),
            digest_field("seal_commitment_sha256", &self.seal_commitment_sha256)?,
            digest_field("stage6_checkpoint_sha256", &self.stage6_checkpoint_sha256)?,
            u16_field(
                "adoption_predicate_version",
                self.adoption_predicate_version,
            ),
            digest_field(
                "adoption_ready_owner_sha256",
                &self.adoption_ready_owner_sha256,
            )?,
        ];
        framed_record(
            STAGE8B_P1E_FIRST_BOOT_RECEIPT_SCHEMA_VERSION,
            "moex.stage8b.p1e.first-boot-receipt.hmac.v2",
            &fields,
        )
    }
}

fn transaction_id_sha256(
    operational_identity_sha256: &str,
    source_bundle_sha256: &str,
    source_bundle_generation: u64,
    bootstrap_attempt_generation: u64,
) -> Result<String, Stage8bP1eFirstBootTransactionError> {
    let fields = vec![
        digest_field("operational_identity_sha256", operational_identity_sha256)?,
        digest_field("source_bundle_sha256", source_bundle_sha256)?,
        u64_field("source_bundle_generation", source_bundle_generation),
        u64_field("bootstrap_attempt_generation", bootstrap_attempt_generation),
    ];
    Ok(sha256_hex(&framed_record(
        1,
        "moex.stage8b.p1e.transaction-id.v1",
        &fields,
    )?))
}

fn canonical_root_identity_sha256(
    root: &Path,
) -> Result<String, Stage8bP1eFirstBootTransactionError> {
    let metadata = fs::symlink_metadata(root)
        .map_err(|error| Stage8bP1eFirstBootTransactionError::Filesystem(error.kind()))?;
    let basename = root
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or(Stage8bP1eFirstBootTransactionError::InvalidAuthorityFile)?;
    if metadata.file_type().is_symlink()
        || !metadata.is_dir()
        || basename.is_empty()
        || basename.len() > 96
        || !basename
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    {
        return Err(Stage8bP1eFirstBootTransactionError::InvalidAuthorityFile);
    }
    let fields = vec![
        u64_field("st_dev", metadata.dev()),
        u64_field("st_ino", metadata.ino()),
        ascii_field("canonical_root_basename", basename)?,
    ];
    Ok(sha256_hex(&framed_record(
        1,
        "moex.stage8b.p1e.canonical-root-identity.v1",
        &fields,
    )?))
}

fn adoption_ready_owner_sha256(
    marker: &Stage8bP1eFirstBootTransactionMarkerV4,
    adoption: &Stage7bP1eAdoptionMaterial,
) -> Result<String, Stage8bP1eFirstBootTransactionError> {
    let fields = vec![
        digest_field(
            "operational_identity_sha256",
            &marker.operational_identity_sha256,
        )?,
        digest_field(
            "restart_package_canonical_sha256",
            &adoption.package.canonical_sha256,
        )?,
        digest_field(
            "first_boot_provenance_canonical_sha256",
            &adoption.package.first_boot_provenance_canonical_sha256,
        )?,
        u64_field("seal_generation", adoption.seal_generation),
        digest_field("seal_commitment_sha256", &adoption.seal_commitment_sha256)?,
        digest_field(
            "stage6_checkpoint_sha256",
            &adoption.stage6_checkpoint_sha256,
        )?,
        ascii_field("authenticated_stage5g_phase", "TimerReady")?,
        u64_field("pending_lifecycle_owner_count", 0),
        u64_field("pending_request_count", 0),
        u64_field("pending_deferred_timer_count", 0),
        bool_field("journal_mutation_uncertain", false),
        bool_field("seal_commit_uncertain", false),
    ];
    Ok(sha256_hex(&framed_record(
        1,
        "moex.stage8b.p1e.adoption-ready-owner.v1",
        &fields,
    )?))
}

#[derive(Clone)]
struct FramedField {
    name: &'static str,
    value: Vec<u8>,
}

fn framed_record(
    schema_version: u16,
    domain: &str,
    fields: &[FramedField],
) -> Result<Vec<u8>, Stage8bP1eFirstBootTransactionError> {
    if !domain.is_ascii() {
        return Err(Stage8bP1eFirstBootTransactionError::InvalidAuthorityFile);
    }
    let domain_len = u16::try_from(domain.len())
        .map_err(|_| Stage8bP1eFirstBootTransactionError::InvalidAuthorityFile)?;
    let field_count = u16::try_from(fields.len())
        .map_err(|_| Stage8bP1eFirstBootTransactionError::InvalidAuthorityFile)?;
    let mut bytes = Vec::new();
    bytes.extend_from_slice(DERIVED_MAGIC);
    bytes.extend_from_slice(&schema_version.to_be_bytes());
    bytes.extend_from_slice(&domain_len.to_be_bytes());
    bytes.extend_from_slice(domain.as_bytes());
    bytes.extend_from_slice(&field_count.to_be_bytes());
    for field in fields {
        let name_len = u16::try_from(field.name.len())
            .map_err(|_| Stage8bP1eFirstBootTransactionError::InvalidAuthorityFile)?;
        let value_len = u32::try_from(field.value.len())
            .map_err(|_| Stage8bP1eFirstBootTransactionError::InvalidAuthorityFile)?;
        bytes.extend_from_slice(&name_len.to_be_bytes());
        bytes.extend_from_slice(field.name.as_bytes());
        bytes.extend_from_slice(&value_len.to_be_bytes());
        bytes.extend_from_slice(&field.value);
    }
    Ok(bytes)
}

fn digest_field(
    name: &'static str,
    value: &str,
) -> Result<FramedField, Stage8bP1eFirstBootTransactionError> {
    Ok(FramedField {
        name,
        value: decode_sha256(value)?,
    })
}

fn u64_field(name: &'static str, value: u64) -> FramedField {
    FramedField {
        name,
        value: value.to_be_bytes().to_vec(),
    }
}

fn u16_field(name: &'static str, value: u16) -> FramedField {
    FramedField {
        name,
        value: value.to_be_bytes().to_vec(),
    }
}

fn bool_field(name: &'static str, value: bool) -> FramedField {
    FramedField {
        name,
        value: vec![u8::from(value)],
    }
}

fn ascii_field(
    name: &'static str,
    value: &str,
) -> Result<FramedField, Stage8bP1eFirstBootTransactionError> {
    if !value.is_ascii() {
        return Err(Stage8bP1eFirstBootTransactionError::InvalidAuthorityFile);
    }
    Ok(FramedField {
        name,
        value: value.as_bytes().to_vec(),
    })
}

fn decode_sha256(value: &str) -> Result<Vec<u8>, Stage8bP1eFirstBootTransactionError> {
    if !is_sha256(value) {
        return Err(Stage8bP1eFirstBootTransactionError::InvalidAuthorityFile);
    }
    (0..64)
        .step_by(2)
        .map(|index| {
            u8::from_str_radix(&value[index..index + 2], 16)
                .map_err(|_| Stage8bP1eFirstBootTransactionError::InvalidAuthorityFile)
        })
        .collect()
}

/// Performs one fresh, fail-closed classification of the complete V5
/// first-boot namespace.  A sealed root is accepted only after the ordinary
/// Stage 7 restart path returns the exact no-effect Ready owner and its V2
/// package/provenance is cross-bound to the authenticated marker.
pub fn classify_stage8b_p1e_first_boot_v5(
    config: Stage8bP1ValidatedBootstrapConfig,
    commitment_key: &Stage5gLifecycleCommitmentKey,
    fresh_runtime: strategy_runtime_core::HybridIntradayRuntimeStrategy,
) -> Stage8bP1eFirstBootInspectionV5 {
    let transaction_id = optional_authenticated_marker(
        config.durable_parent(),
        STAGE8B_P1E_TRANSACTION_MARKER_FILE,
        commitment_key,
    )
    .ok()
    .flatten()
    .or_else(|| {
        optional_authenticated_marker(
            config.durable_parent(),
            STAGE8B_P1E_TRANSACTION_MARKER_TEMP_FILE,
            commitment_key,
        )
        .ok()
        .flatten()
    })
    .map(|marker| marker.transaction_id_sha256);
    match classify_stage8b_p1e_first_boot_v5_inner(config, commitment_key, fresh_runtime) {
        Ok(classification) => {
            Stage8bP1eFirstBootInspectionV5::new(classification.0, classification.1)
        }
        Err(_) => Stage8bP1eFirstBootInspectionV5::new(
            Stage8bP1eFirstBootClassificationV5::CorruptOrIdentityMismatch,
            transaction_id,
        ),
    }
}

/// Authenticates the immutable V5 adoption authority for an ordinary daemon
/// run and returns the *same* linear restart outcome that was inspected.
///
/// Unlike the first-boot classifier, this admission intentionally does not
/// require the current seal/checkpoint/package digest to equal the initial
/// receipt: those values advance during legitimate M10 and lifecycle work.
/// The adopted marker and receipt remain immutable, while the current V2
/// package must preserve their exact first-boot provenance and deployment
/// identity. No authority file is created, removed or repaired here.
pub fn admit_stage8b_p1e_ordinary_run_v1(
    config: Stage8bP1ValidatedBootstrapConfig,
    commitment_key: &Stage5gLifecycleCommitmentKey,
    fresh_runtime: strategy_runtime_core::HybridIntradayRuntimeStrategy,
) -> Result<Stage7bRestartOutcome, Stage8bP1eFirstBootTransactionError> {
    let parent = config.durable_parent().to_path_buf();
    let root_path = parent.join(config.expected_root_name());

    if path_exists(parent.join(STAGE8B_P1E_TRANSACTION_MARKER_TEMP_FILE))?
        || path_exists(parent.join(STAGE8B_P1E_FIRST_BOOT_RECEIPT_TEMP_FILE))?
    {
        return Err(Stage8bP1eFirstBootTransactionError::Adoption);
    }
    let marker: Stage8bP1eFirstBootTransactionMarkerV4 = read_canonical_authority(
        &parent,
        STAGE8B_P1E_TRANSACTION_MARKER_FILE,
        commitment_key,
        |value: &Stage8bP1eFirstBootTransactionMarkerV4, key| value.validate_authenticated(key),
    )?;
    let receipt: Stage8bP1FirstBootReceiptV2 = read_canonical_authority(
        &parent,
        STAGE8B_P1E_FIRST_BOOT_RECEIPT_FILE,
        commitment_key,
        |value: &Stage8bP1FirstBootReceiptV2, key| value.validate_authenticated(key),
    )?;
    validate_owned_directory(&root_path)?;
    validate_owned_regular_file(&root_path.join(STAGE7B_JOURNAL_FILE))?;
    validate_owned_regular_file(&root_path.join(STAGE7B_RECOVERY_SEAL_FILE))?;
    if marker.phase != Stage8bP1eFirstBootTransactionPhaseV4::Adopted
        || marker.operational_identity_sha256 != config.operational_identity_sha256()
        || marker.runtime_profile_sha256 != STAGE8B_P1E_RUNTIME_PROFILE_SHA256
        || marker.runtime_config_fingerprint_sha256 != config.runtime_config_fingerprint_sha256()
        || marker.canonical_root_identity_sha256 != canonical_root_identity_sha256(&root_path)?
        || receipt.deployment_identity_sha256 != STAGE8B_P1E_DEPLOYMENT_IDENTITY_V2_SHA256
    {
        return Err(Stage8bP1eFirstBootTransactionError::Adoption);
    }
    cross_validate_adopted(&parent, &marker, &receipt, commitment_key)?;

    let outcome = restart_stage8b_p1(config, commitment_key, fresh_runtime)
        .map_err(|_| Stage8bP1eFirstBootTransactionError::Durable)?;
    let current = outcome
        .stage8b_p1e_current_restart_package_audit(commitment_key)
        .map_err(|_| Stage8bP1eFirstBootTransactionError::Adoption)?;
    let provenance = &current.first_boot_provenance;
    if current.schema_version != 2
        || current.operational_identity_sha256 != marker.operational_identity_sha256
        || current.first_boot_provenance_canonical_sha256
            != receipt.first_boot_provenance_canonical_sha256
        || provenance.operational_identity_sha256() != marker.operational_identity_sha256
        || provenance.runtime_profile_sha256() != marker.runtime_profile_sha256
        || provenance.runtime_config_fingerprint_sha256()
            != marker.runtime_config_fingerprint_sha256
        || provenance.source_bundle_sha256() != marker.source_bundle_sha256
        || provenance.source_bundle_generation() != marker.source_bundle_generation
        || provenance.source_plan_sha256() != marker.source_plan_sha256
        || provenance.history_bars_sha256() != marker.history_bars_sha256
        || provenance.riskgate_session_observations_sha256()
            != marker.riskgate_session_observations_sha256
        || provenance.candidate_semantic_id_sha256() != marker.candidate_semantic_id_sha256
    {
        return Err(Stage8bP1eFirstBootTransactionError::Adoption);
    }
    Ok(outcome)
}

fn classify_stage8b_p1e_first_boot_v5_inner(
    config: Stage8bP1ValidatedBootstrapConfig,
    commitment_key: &Stage5gLifecycleCommitmentKey,
    fresh_runtime: strategy_runtime_core::HybridIntradayRuntimeStrategy,
) -> Result<
    (Stage8bP1eFirstBootClassificationV5, Option<String>),
    Stage8bP1eFirstBootTransactionError,
> {
    let parent = config.durable_parent().to_path_buf();
    let root_path = parent.join(config.expected_root_name());
    let expected_operational_identity = config.operational_identity_sha256().to_string();
    let expected_root_name = config.expected_root_name().to_string();
    let marker = optional_authenticated_marker(
        &parent,
        STAGE8B_P1E_TRANSACTION_MARKER_FILE,
        commitment_key,
    )?;
    let marker_temp = optional_authenticated_marker(
        &parent,
        STAGE8B_P1E_TRANSACTION_MARKER_TEMP_FILE,
        commitment_key,
    )?;
    let receipt = optional_authenticated_receipt(
        &parent,
        STAGE8B_P1E_FIRST_BOOT_RECEIPT_FILE,
        commitment_key,
    )?;
    let receipt_temp = optional_authenticated_receipt(
        &parent,
        STAGE8B_P1E_FIRST_BOOT_RECEIPT_TEMP_FILE,
        commitment_key,
    )?;
    let transaction_id = marker
        .as_ref()
        .or(marker_temp.as_ref())
        .map(|value| value.transaction_id_sha256.clone());
    let quarantine_root = transaction_id.as_ref().map(|transaction_id| {
        parent
            .join(STAGE8B_P1E_FIRST_BOOT_QUARANTINE_DIRECTORY)
            .join(transaction_id)
    });
    let root_exists = path_exists(root_path.clone())?;
    let quarantine_exists = quarantine_root
        .as_ref()
        .map(|path| path_exists(path.clone()))
        .transpose()?
        .unwrap_or(false);
    if root_exists && quarantine_exists {
        return Err(Stage8bP1eFirstBootTransactionError::InvalidAuthorityFile);
    }
    let journal_exists = root_exists && path_exists(root_path.join(STAGE7B_JOURNAL_FILE))?;
    let seal_exists = root_exists && path_exists(root_path.join(STAGE7B_RECOVERY_SEAL_FILE))?;

    for authority in marker.iter().chain(marker_temp.iter()) {
        if authority.operational_identity_sha256 != expected_operational_identity {
            return Err(Stage8bP1eFirstBootTransactionError::InvalidAuthorityFile);
        }
    }
    if root_exists {
        validate_owned_directory(&root_path)?;
        if journal_exists {
            validate_owned_regular_file(&root_path.join(STAGE7B_JOURNAL_FILE))?;
        }
        if seal_exists {
            validate_owned_regular_file(&root_path.join(STAGE7B_RECOVERY_SEAL_FILE))?;
        }
    }
    if seal_exists && !journal_exists {
        return Err(Stage8bP1eFirstBootTransactionError::InvalidAuthorityFile);
    }

    let root_identity = root_exists
        .then(|| canonical_root_identity_sha256(&root_path))
        .transpose()?;
    let mut quarantine_identity = None;
    let mut quarantined_journal_exists = false;
    let mut quarantined_seal_exists = false;
    if quarantine_exists {
        let path = quarantine_root
            .as_ref()
            .ok_or(Stage8bP1eFirstBootTransactionError::InvalidAuthorityFile)?;
        validate_owned_directory(path)?;
        quarantined_journal_exists = path_exists(path.join(STAGE7B_JOURNAL_FILE))?;
        quarantined_seal_exists = path_exists(path.join(STAGE7B_RECOVERY_SEAL_FILE))?;
        if quarantined_journal_exists {
            validate_owned_regular_file(&path.join(STAGE7B_JOURNAL_FILE))?;
        }
        if quarantined_seal_exists {
            validate_owned_regular_file(&path.join(STAGE7B_RECOVERY_SEAL_FILE))?;
        }
        if quarantined_seal_exists && !quarantined_journal_exists {
            return Err(Stage8bP1eFirstBootTransactionError::InvalidAuthorityFile);
        }
        quarantine_identity = Some(canonical_root_identity_sha256_with_basename(
            path,
            &expected_root_name,
        )?);
    }
    let observed_root_identity = root_identity.as_ref().or(quarantine_identity.as_ref());
    for authority in marker.iter().chain(marker_temp.iter()) {
        if authority.phase != Stage8bP1eFirstBootTransactionPhaseV4::Prepared
            && observed_root_identity != Some(&authority.canonical_root_identity_sha256)
        {
            return Err(Stage8bP1eFirstBootTransactionError::InvalidAuthorityFile);
        }
    }

    let marker_temp_is_successor = match (&marker, &marker_temp) {
        (Some(old), Some(next)) => old
            .successor(
                next.phase,
                next.canonical_root_identity_sha256.clone(),
                commitment_key,
            )
            .is_ok_and(|expected| expected == *next),
        _ => false,
    };

    let mut adoption = None;
    if seal_exists {
        let restart = restart_stage8b_p1(config, commitment_key, fresh_runtime)
            .map_err(|_| Stage8bP1eFirstBootTransactionError::Adoption)?;
        let Stage7bRestartOutcome::Ready(mut owner) = restart else {
            return Err(Stage8bP1eFirstBootTransactionError::Adoption);
        };
        let material = owner
            .stage8b_p1e_adoption_material(commitment_key)
            .map_err(|_| Stage8bP1eFirstBootTransactionError::Adoption)?;
        let authority = marker_temp
            .as_ref()
            .filter(|value| {
                matches!(
                    value.phase,
                    Stage8bP1eFirstBootTransactionPhaseV4::SealCommitted
                        | Stage8bP1eFirstBootTransactionPhaseV4::Adopted
                )
            })
            .or(marker.as_ref())
            .ok_or(Stage8bP1eFirstBootTransactionError::InvalidAuthorityFile)?;
        if !marker_matches_adoption(authority, &material) {
            return Err(Stage8bP1eFirstBootTransactionError::Adoption);
        }
        adoption = Some(material);
    }

    let receipt_marker = marker.as_ref().filter(|value| {
        matches!(
            value.phase,
            Stage8bP1eFirstBootTransactionPhaseV4::SealCommitted
                | Stage8bP1eFirstBootTransactionPhaseV4::Adopted
        )
    });
    let expected_receipt = match (receipt_marker, &adoption) {
        (Some(marker), Some(adoption)) => Some(Stage8bP1FirstBootReceiptV2::from_adoption(
            marker,
            marker.canonical_root_identity_sha256.clone(),
            adoption,
            commitment_key,
        )?),
        _ => None,
    };
    let receipt_exact = receipt
        .as_ref()
        .zip(expected_receipt.as_ref())
        .is_some_and(|(actual, expected)| actual == expected);
    let receipt_temp_exact = receipt_temp
        .as_ref()
        .zip(expected_receipt.as_ref())
        .is_some_and(|(actual, expected)| actual == expected);
    if (receipt.is_some() && !receipt_exact) || (receipt_temp.is_some() && !receipt_temp_exact) {
        return Err(Stage8bP1eFirstBootTransactionError::Adoption);
    }

    let no_receipts = receipt.is_none() && receipt_temp.is_none();
    let no_root_authority = !root_exists && !journal_exists && !seal_exists;
    let no_active_files =
        marker.is_none() && marker_temp.is_none() && receipt.is_none() && receipt_temp.is_none();
    let mut matches = Vec::new();

    if no_root_authority && no_active_files && !quarantine_exists {
        matches.push(Stage8bP1eFirstBootClassificationV5::NoRoot);
    }
    if marker.is_none()
        && marker_temp
            .as_ref()
            .is_some_and(|value| value.phase == Stage8bP1eFirstBootTransactionPhaseV4::Prepared)
        && no_root_authority
        && receipt.is_none()
        && receipt_temp.is_none()
        && !quarantine_exists
    {
        matches.push(Stage8bP1eFirstBootClassificationV5::UnpublishedMarkerTemp);
    }
    if marker
        .as_ref()
        .is_some_and(|value| value.phase == Stage8bP1eFirstBootTransactionPhaseV4::Prepared)
        && marker_temp.is_none()
        && no_root_authority
        && no_receipts
        && !quarantine_exists
    {
        matches.push(Stage8bP1eFirstBootClassificationV5::PreparedWithoutRoot);
    }
    if marker
        .as_ref()
        .is_some_and(|value| value.phase == Stage8bP1eFirstBootTransactionPhaseV4::RootPublished)
        && marker_temp.is_none()
        && root_exists
        && !journal_exists
        && !seal_exists
        && no_receipts
        && !quarantine_exists
    {
        matches.push(Stage8bP1eFirstBootClassificationV5::RootWithoutJournal);
    }
    if marker
        .as_ref()
        .is_some_and(|value| value.phase == Stage8bP1eFirstBootTransactionPhaseV4::JournalDurable)
        && marker_temp.is_none()
        && root_exists
        && journal_exists
        && !seal_exists
        && no_receipts
        && !quarantine_exists
    {
        matches.push(Stage8bP1eFirstBootClassificationV5::JournalWithoutSeal);
    }
    if marker
        .as_ref()
        .is_some_and(|value| value.phase == Stage8bP1eFirstBootTransactionPhaseV4::SealCommitted)
        && marker_temp.is_none()
        && root_exists
        && journal_exists
        && seal_exists
        && receipt.is_none()
        && receipt_temp_exact
        && !quarantine_exists
    {
        matches.push(Stage8bP1eFirstBootClassificationV5::CommittedRootReceiptTemp);
    }
    if marker
        .as_ref()
        .is_some_and(|value| value.phase == Stage8bP1eFirstBootTransactionPhaseV4::SealCommitted)
        && marker_temp.is_none()
        && root_exists
        && journal_exists
        && seal_exists
        && no_receipts
        && !quarantine_exists
    {
        matches.push(Stage8bP1eFirstBootClassificationV5::CommittedRootResponseLost);
    }
    if marker_temp.is_none()
        && !root_exists
        && quarantine_exists
        && no_receipts
        && !quarantined_seal_exists
        && marker.as_ref().is_some_and(|value| {
            (value.phase == Stage8bP1eFirstBootTransactionPhaseV4::RootPublished
                && !quarantined_journal_exists)
                || (value.phase == Stage8bP1eFirstBootTransactionPhaseV4::JournalDurable
                    && quarantined_journal_exists)
        })
    {
        matches.push(Stage8bP1eFirstBootClassificationV5::QuarantinedIncompleteRoot);
    }
    if marker
        .as_ref()
        .is_some_and(|value| value.phase == Stage8bP1eFirstBootTransactionPhaseV4::SealCommitted)
        && marker_temp.is_none()
        && root_exists
        && journal_exists
        && seal_exists
        && receipt_exact
        && receipt_temp.is_none()
        && !quarantine_exists
    {
        matches.push(Stage8bP1eFirstBootClassificationV5::ReceiptCommittedMarkerUpdatePending);
    }
    if marker
        .as_ref()
        .is_some_and(|value| value.phase == Stage8bP1eFirstBootTransactionPhaseV4::Adopted)
        && marker_temp.is_none()
        && root_exists
        && journal_exists
        && seal_exists
        && receipt_exact
        && receipt_temp.is_none()
        && !quarantine_exists
    {
        matches.push(Stage8bP1eFirstBootClassificationV5::AdoptedCommittedRoot);
    }

    if marker_temp_is_successor
        && marker
            .as_ref()
            .is_some_and(|old| old.phase == Stage8bP1eFirstBootTransactionPhaseV4::Prepared)
        && marker_temp
            .as_ref()
            .is_some_and(|next| next.phase == Stage8bP1eFirstBootTransactionPhaseV4::RootPublished)
        && root_exists
        && !journal_exists
        && !seal_exists
        && no_receipts
        && !quarantine_exists
    {
        matches.push(Stage8bP1eFirstBootClassificationV5::PreparedToRootPublishedMarkerTempPending);
    }
    if marker_temp_is_successor
        && marker
            .as_ref()
            .is_some_and(|old| old.phase == Stage8bP1eFirstBootTransactionPhaseV4::RootPublished)
        && marker_temp
            .as_ref()
            .is_some_and(|next| next.phase == Stage8bP1eFirstBootTransactionPhaseV4::JournalDurable)
        && root_exists
        && journal_exists
        && !seal_exists
        && no_receipts
        && !quarantine_exists
    {
        matches.push(
            Stage8bP1eFirstBootClassificationV5::RootPublishedToJournalDurableMarkerTempPending,
        );
    }
    if marker_temp_is_successor
        && marker
            .as_ref()
            .is_some_and(|old| old.phase == Stage8bP1eFirstBootTransactionPhaseV4::JournalDurable)
        && marker_temp
            .as_ref()
            .is_some_and(|next| next.phase == Stage8bP1eFirstBootTransactionPhaseV4::SealCommitted)
        && root_exists
        && journal_exists
        && seal_exists
        && no_receipts
        && !quarantine_exists
    {
        matches.push(
            Stage8bP1eFirstBootClassificationV5::JournalDurableToSealCommittedMarkerTempPending,
        );
    }
    if marker_temp_is_successor
        && marker
            .as_ref()
            .is_some_and(|old| old.phase == Stage8bP1eFirstBootTransactionPhaseV4::SealCommitted)
        && marker_temp
            .as_ref()
            .is_some_and(|next| next.phase == Stage8bP1eFirstBootTransactionPhaseV4::Adopted)
        && root_exists
        && journal_exists
        && seal_exists
        && receipt_exact
        && receipt_temp.is_none()
        && !quarantine_exists
    {
        matches.push(Stage8bP1eFirstBootClassificationV5::SealCommittedToAdoptedMarkerTempPending);
    }

    if matches.len() != 1 {
        return Ok((
            Stage8bP1eFirstBootClassificationV5::CorruptOrIdentityMismatch,
            transaction_id,
        ));
    }
    Ok((matches[0], transaction_id))
}

fn optional_authenticated_marker(
    parent: &Path,
    name: &str,
    key: &Stage5gLifecycleCommitmentKey,
) -> Result<Option<Stage8bP1eFirstBootTransactionMarkerV4>, Stage8bP1eFirstBootTransactionError> {
    if !path_exists(parent.join(name))? {
        return Ok(None);
    }
    read_canonical_authority(parent, name, key, |value, key| {
        Stage8bP1eFirstBootTransactionMarkerV4::validate_authenticated(value, key)
    })
    .map(Some)
}

fn optional_authenticated_receipt(
    parent: &Path,
    name: &str,
    key: &Stage5gLifecycleCommitmentKey,
) -> Result<Option<Stage8bP1FirstBootReceiptV2>, Stage8bP1eFirstBootTransactionError> {
    if !path_exists(parent.join(name))? {
        return Ok(None);
    }
    read_canonical_authority(parent, name, key, |value, key| {
        Stage8bP1FirstBootReceiptV2::validate_authenticated(value, key)
    })
    .map(Some)
}

fn marker_matches_adoption(
    marker: &Stage8bP1eFirstBootTransactionMarkerV4,
    adoption: &Stage7bP1eAdoptionMaterial,
) -> bool {
    let provenance = &adoption.package.first_boot_provenance;
    adoption.package.schema_version == 2
        && adoption.package.operational_identity_sha256 == marker.operational_identity_sha256
        && adoption.package.stage6_checkpoint_sha256 == adoption.stage6_checkpoint_sha256
        && provenance.operational_identity_sha256() == marker.operational_identity_sha256
        && provenance.runtime_profile_sha256() == marker.runtime_profile_sha256
        && provenance.runtime_config_fingerprint_sha256()
            == marker.runtime_config_fingerprint_sha256
        && provenance.source_bundle_sha256() == marker.source_bundle_sha256
        && provenance.source_bundle_generation() == marker.source_bundle_generation
        && provenance.source_plan_sha256() == marker.source_plan_sha256
        && provenance.history_bars_sha256() == marker.history_bars_sha256
        && provenance.riskgate_session_observations_sha256()
            == marker.riskgate_session_observations_sha256
        && provenance.candidate_semantic_id_sha256() == marker.candidate_semantic_id_sha256
}

fn validate_owned_directory(path: &Path) -> Result<(), Stage8bP1eFirstBootTransactionError> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| Stage8bP1eFirstBootTransactionError::Filesystem(error.kind()))?;
    if metadata.file_type().is_symlink()
        || !metadata.is_dir()
        || metadata.uid() != unsafe { libc::geteuid() }
        || metadata.gid() != unsafe { libc::getegid() }
        || metadata.permissions().mode() & 0o777 != 0o700
    {
        return Err(Stage8bP1eFirstBootTransactionError::InvalidAuthorityFile);
    }
    Ok(())
}

fn validate_owned_regular_file(path: &Path) -> Result<(), Stage8bP1eFirstBootTransactionError> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK)
        .open(path)
        .map_err(|error| Stage8bP1eFirstBootTransactionError::Filesystem(error.kind()))?;
    validate_owned_authority_file(&file)
}

fn canonical_root_identity_sha256_with_basename(
    root: &Path,
    basename: &str,
) -> Result<String, Stage8bP1eFirstBootTransactionError> {
    let metadata = fs::symlink_metadata(root)
        .map_err(|error| Stage8bP1eFirstBootTransactionError::Filesystem(error.kind()))?;
    if metadata.file_type().is_symlink()
        || !metadata.is_dir()
        || basename.is_empty()
        || basename.len() > 96
        || !basename
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    {
        return Err(Stage8bP1eFirstBootTransactionError::InvalidAuthorityFile);
    }
    let fields = vec![
        u64_field("st_dev", metadata.dev()),
        u64_field("st_ino", metadata.ino()),
        ascii_field("canonical_root_basename", basename)?,
    ];
    Ok(sha256_hex(&framed_record(
        1,
        "moex.stage8b.p1e.canonical-root-identity.v1",
        &fields,
    )?))
}

fn require_pristine(
    state_parent: &Path,
    root_path: &Path,
) -> Result<(), Stage8bP1eFirstBootTransactionError> {
    for path in [
        root_path.to_path_buf(),
        state_parent.join(STAGE8B_P1E_TRANSACTION_MARKER_FILE),
        state_parent.join(STAGE8B_P1E_TRANSACTION_MARKER_TEMP_FILE),
        state_parent.join(STAGE8B_P1E_FIRST_BOOT_RECEIPT_FILE),
        state_parent.join(STAGE8B_P1E_FIRST_BOOT_RECEIPT_TEMP_FILE),
    ] {
        match fs::symlink_metadata(path) {
            Ok(_) => return Err(Stage8bP1eFirstBootTransactionError::NotPristine),
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(error) => {
                return Err(Stage8bP1eFirstBootTransactionError::Filesystem(
                    error.kind(),
                ))
            }
        }
    }
    Ok(())
}

fn commit_initial_marker<F>(
    parent: &Path,
    marker: &Stage8bP1eFirstBootTransactionMarkerV4,
    key: &Stage5gLifecycleCommitmentKey,
    observer: &mut F,
) -> Result<(), Stage8bP1eFirstBootTransactionError>
where
    F: FnMut(&'static str),
{
    if marker.phase != Stage8bP1eFirstBootTransactionPhaseV4::Prepared {
        return Err(Stage8bP1eFirstBootTransactionError::InvalidAuthorityFile);
    }
    write_authority_temp(
        parent,
        STAGE8B_P1E_TRANSACTION_MARKER_TEMP_FILE,
        &canonical_json(marker)?,
    )?;
    observer("after-prepared-marker-temp-sync-before-rename");
    rename_noreplace(
        parent,
        STAGE8B_P1E_TRANSACTION_MARKER_TEMP_FILE,
        STAGE8B_P1E_TRANSACTION_MARKER_FILE,
    )?;
    sync_directory(parent)?;
    let reread: Stage8bP1eFirstBootTransactionMarkerV4 = read_canonical_authority(
        parent,
        STAGE8B_P1E_TRANSACTION_MARKER_FILE,
        key,
        |value: &Stage8bP1eFirstBootTransactionMarkerV4, key| value.validate_authenticated(key),
    )?;
    if &reread != marker {
        return Err(Stage8bP1eFirstBootTransactionError::InvalidAuthorityFile);
    }
    Ok(())
}

fn replace_marker<F>(
    parent: &Path,
    marker: &Stage8bP1eFirstBootTransactionMarkerV4,
    key: &Stage5gLifecycleCommitmentKey,
    observer: &mut F,
) -> Result<(), Stage8bP1eFirstBootTransactionError>
where
    F: FnMut(&'static str),
{
    let old: Stage8bP1eFirstBootTransactionMarkerV4 = read_canonical_authority(
        parent,
        STAGE8B_P1E_TRANSACTION_MARKER_FILE,
        key,
        |value: &Stage8bP1eFirstBootTransactionMarkerV4, key| value.validate_authenticated(key),
    )?;
    if marker.marker_generation != old.marker_generation + 1
        || marker.transaction_id_sha256 != old.transaction_id_sha256
        || marker.operational_identity_sha256 != old.operational_identity_sha256
    {
        return Err(Stage8bP1eFirstBootTransactionError::InvalidAuthorityFile);
    }
    write_authority_temp(
        parent,
        STAGE8B_P1E_TRANSACTION_MARKER_TEMP_FILE,
        &canonical_json(marker)?,
    )?;
    observer(match marker.phase {
        Stage8bP1eFirstBootTransactionPhaseV4::RootPublished => {
            "after-root-published-marker-temp-sync-before-rename"
        }
        Stage8bP1eFirstBootTransactionPhaseV4::JournalDurable => {
            "after-journal-durable-marker-temp-sync-before-rename"
        }
        Stage8bP1eFirstBootTransactionPhaseV4::SealCommitted => {
            "after-seal-committed-marker-temp-sync-before-rename"
        }
        Stage8bP1eFirstBootTransactionPhaseV4::Adopted => {
            "after-adopted-marker-temp-sync-before-rename"
        }
        Stage8bP1eFirstBootTransactionPhaseV4::Prepared => {
            return Err(Stage8bP1eFirstBootTransactionError::InvalidAuthorityFile)
        }
    });
    replace_file(
        parent,
        STAGE8B_P1E_TRANSACTION_MARKER_TEMP_FILE,
        STAGE8B_P1E_TRANSACTION_MARKER_FILE,
    )?;
    sync_directory(parent)?;
    let reread: Stage8bP1eFirstBootTransactionMarkerV4 = read_canonical_authority(
        parent,
        STAGE8B_P1E_TRANSACTION_MARKER_FILE,
        key,
        |value: &Stage8bP1eFirstBootTransactionMarkerV4, key| value.validate_authenticated(key),
    )?;
    if &reread != marker {
        return Err(Stage8bP1eFirstBootTransactionError::InvalidAuthorityFile);
    }
    Ok(())
}

fn commit_receipt<F>(
    parent: &Path,
    receipt: &Stage8bP1FirstBootReceiptV2,
    key: &Stage5gLifecycleCommitmentKey,
    observer: &mut F,
) -> Result<(), Stage8bP1eFirstBootTransactionError>
where
    F: FnMut(&'static str),
{
    write_authority_temp(
        parent,
        STAGE8B_P1E_FIRST_BOOT_RECEIPT_TEMP_FILE,
        &canonical_json(receipt)?,
    )?;
    observer("after-receipt-temp-sync-before-final-rename");
    rename_noreplace(
        parent,
        STAGE8B_P1E_FIRST_BOOT_RECEIPT_TEMP_FILE,
        STAGE8B_P1E_FIRST_BOOT_RECEIPT_FILE,
    )?;
    sync_directory(parent)?;
    let reread: Stage8bP1FirstBootReceiptV2 = read_canonical_authority(
        parent,
        STAGE8B_P1E_FIRST_BOOT_RECEIPT_FILE,
        key,
        |value: &Stage8bP1FirstBootReceiptV2, key| value.validate_authenticated(key),
    )?;
    if &reread != receipt {
        return Err(Stage8bP1eFirstBootTransactionError::InvalidAuthorityFile);
    }
    Ok(())
}

fn cross_validate_adopted(
    parent: &Path,
    marker: &Stage8bP1eFirstBootTransactionMarkerV4,
    receipt: &Stage8bP1FirstBootReceiptV2,
    key: &Stage5gLifecycleCommitmentKey,
) -> Result<(), Stage8bP1eFirstBootTransactionError> {
    if marker.phase != Stage8bP1eFirstBootTransactionPhaseV4::Adopted
        || marker.transaction_id_sha256 != receipt.transaction_id_sha256
        || marker.bootstrap_attempt_generation != receipt.bootstrap_attempt_generation
        || marker.canonical_root_identity_sha256 != receipt.canonical_root_identity_sha256
        || path_exists(parent.join(STAGE8B_P1E_TRANSACTION_MARKER_TEMP_FILE))?
        || path_exists(parent.join(STAGE8B_P1E_FIRST_BOOT_RECEIPT_TEMP_FILE))?
    {
        return Err(Stage8bP1eFirstBootTransactionError::Adoption);
    }
    let persisted_marker: Stage8bP1eFirstBootTransactionMarkerV4 = read_canonical_authority(
        parent,
        STAGE8B_P1E_TRANSACTION_MARKER_FILE,
        key,
        |value: &Stage8bP1eFirstBootTransactionMarkerV4, key| value.validate_authenticated(key),
    )?;
    let persisted_receipt: Stage8bP1FirstBootReceiptV2 = read_canonical_authority(
        parent,
        STAGE8B_P1E_FIRST_BOOT_RECEIPT_FILE,
        key,
        |value: &Stage8bP1FirstBootReceiptV2, key| value.validate_authenticated(key),
    )?;
    if &persisted_marker != marker || &persisted_receipt != receipt {
        return Err(Stage8bP1eFirstBootTransactionError::Adoption);
    }
    Ok(())
}

fn canonical_json<T: Serialize>(value: &T) -> Result<Vec<u8>, Stage8bP1eFirstBootTransactionError> {
    serde_json::to_vec(value).map_err(|_| Stage8bP1eFirstBootTransactionError::InvalidAuthorityFile)
}

fn write_authority_temp(
    parent: &Path,
    name: &str,
    bytes: &[u8],
) -> Result<(), Stage8bP1eFirstBootTransactionError> {
    if bytes.len() as u64 > MAX_AUTHORITY_FILE_BYTES {
        return Err(Stage8bP1eFirstBootTransactionError::InvalidAuthorityFile);
    }
    let path = parent.join(name);
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)
        .map_err(|error| Stage8bP1eFirstBootTransactionError::Filesystem(error.kind()))?;
    validate_owned_authority_file(&file)?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|error| Stage8bP1eFirstBootTransactionError::Filesystem(error.kind()))?;
    validate_owned_authority_file(&file)
}

fn read_canonical_authority<T, F>(
    parent: &Path,
    name: &str,
    key: &Stage5gLifecycleCommitmentKey,
    validate: F,
) -> Result<T, Stage8bP1eFirstBootTransactionError>
where
    T: for<'de> Deserialize<'de> + Serialize,
    F: FnOnce(
        &T,
        &Stage5gLifecycleCommitmentKey,
    ) -> Result<(), Stage8bP1eFirstBootTransactionError>,
{
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK)
        .open(parent.join(name))
        .map_err(|error| Stage8bP1eFirstBootTransactionError::Filesystem(error.kind()))?;
    validate_owned_authority_file(&file)?;
    if file
        .metadata()
        .map_err(|error| Stage8bP1eFirstBootTransactionError::Filesystem(error.kind()))?
        .len()
        > MAX_AUTHORITY_FILE_BYTES
    {
        return Err(Stage8bP1eFirstBootTransactionError::InvalidAuthorityFile);
    }
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .map_err(|error| Stage8bP1eFirstBootTransactionError::Filesystem(error.kind()))?;
    validate_owned_authority_file(&file)?;
    let value: T = serde_json::from_slice(&bytes)
        .map_err(|_| Stage8bP1eFirstBootTransactionError::InvalidAuthorityFile)?;
    validate(&value, key)?;
    if canonical_json(&value)? != bytes {
        return Err(Stage8bP1eFirstBootTransactionError::InvalidAuthorityFile);
    }
    Ok(value)
}

fn validate_owned_authority_file(file: &File) -> Result<(), Stage8bP1eFirstBootTransactionError> {
    let metadata = file
        .metadata()
        .map_err(|error| Stage8bP1eFirstBootTransactionError::Filesystem(error.kind()))?;
    if !metadata.file_type().is_file()
        || metadata.nlink() != 1
        || metadata.uid() != unsafe { libc::geteuid() }
        || metadata.gid() != unsafe { libc::getegid() }
        || metadata.permissions().mode() & 0o777 != 0o600
    {
        return Err(Stage8bP1eFirstBootTransactionError::InvalidAuthorityFile);
    }
    Ok(())
}

fn sync_directory(path: &Path) -> Result<(), Stage8bP1eFirstBootTransactionError> {
    let directory = open_directory(path)?;
    directory
        .sync_all()
        .map_err(|error| Stage8bP1eFirstBootTransactionError::Filesystem(error.kind()))
}

fn open_directory(path: &Path) -> Result<File, Stage8bP1eFirstBootTransactionError> {
    let path = CString::new(path.as_os_str().as_bytes())
        .map_err(|_| Stage8bP1eFirstBootTransactionError::InvalidAuthorityFile)?;
    let fd = unsafe {
        libc::open(
            path.as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    };
    if fd < 0 {
        return Err(Stage8bP1eFirstBootTransactionError::Filesystem(
            std::io::Error::last_os_error().kind(),
        ));
    }
    Ok(unsafe { File::from_raw_fd(fd) })
}

fn replace_file(
    parent: &Path,
    from: &str,
    to: &str,
) -> Result<(), Stage8bP1eFirstBootTransactionError> {
    let directory = open_directory(parent)?;
    let from = CString::new(from)
        .map_err(|_| Stage8bP1eFirstBootTransactionError::InvalidAuthorityFile)?;
    let to =
        CString::new(to).map_err(|_| Stage8bP1eFirstBootTransactionError::InvalidAuthorityFile)?;
    let result = unsafe {
        libc::renameat(
            directory.as_raw_fd(),
            from.as_ptr(),
            directory.as_raw_fd(),
            to.as_ptr(),
        )
    };
    if result != 0 {
        return Err(Stage8bP1eFirstBootTransactionError::Filesystem(
            std::io::Error::last_os_error().kind(),
        ));
    }
    Ok(())
}

fn unlink_authority_file(
    parent: &Path,
    name: &str,
) -> Result<(), Stage8bP1eFirstBootTransactionError> {
    let directory = open_directory(parent)?;
    let name = CString::new(name)
        .map_err(|_| Stage8bP1eFirstBootTransactionError::InvalidAuthorityFile)?;
    let result = unsafe { libc::unlinkat(directory.as_raw_fd(), name.as_ptr(), 0) };
    if result != 0 {
        return Err(Stage8bP1eFirstBootTransactionError::Filesystem(
            std::io::Error::last_os_error().kind(),
        ));
    }
    Ok(())
}

fn rename_noreplace(
    parent: &Path,
    from: &str,
    to: &str,
) -> Result<(), Stage8bP1eFirstBootTransactionError> {
    if path_exists(parent.join(to))? {
        return Err(Stage8bP1eFirstBootTransactionError::NotPristine);
    }
    #[cfg(target_os = "linux")]
    {
        let directory = open_directory(parent)?;
        let from = CString::new(from)
            .map_err(|_| Stage8bP1eFirstBootTransactionError::InvalidAuthorityFile)?;
        let to = CString::new(to)
            .map_err(|_| Stage8bP1eFirstBootTransactionError::InvalidAuthorityFile)?;
        let result = unsafe {
            libc::syscall(
                libc::SYS_renameat2,
                directory.as_raw_fd(),
                from.as_ptr(),
                directory.as_raw_fd(),
                to.as_ptr(),
                libc::RENAME_NOREPLACE,
            )
        };
        if result != 0 {
            return Err(Stage8bP1eFirstBootTransactionError::Filesystem(
                std::io::Error::last_os_error().kind(),
            ));
        }
        return Ok(());
    }
    #[cfg(not(target_os = "linux"))]
    {
        // Development hosts use the prechecked rename fallback. The deployable
        // Linux binary above always uses kernel-enforced RENAME_NOREPLACE.
        fs::rename(parent.join(from), parent.join(to))
            .map_err(|error| Stage8bP1eFirstBootTransactionError::Filesystem(error.kind()))
    }
}

fn path_exists(path: PathBuf) -> Result<bool, Stage8bP1eFirstBootTransactionError> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(false),
        Err(error) => Err(Stage8bP1eFirstBootTransactionError::Filesystem(
            error.kind(),
        )),
    }
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|byte| format!("{byte:02x}")).collect()
    }

    #[test]
    fn derived_digest_framing_matches_all_accepted_golden_vectors() {
        let sequential = |start: u8| {
            (start..start + 32)
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
        };
        let transaction = transaction_id_sha256(&sequential(0), &sequential(0x20), 7, 11)
            .expect("transaction vector");
        assert_eq!(
            transaction,
            "5b6b595d4f492ea3ed8bb3d91a87ee93be8081707f523fa8f92cea4cee6e8c6b"
        );

        let root_preimage = framed_record(
            1,
            "moex.stage8b.p1e.canonical-root-identity.v1",
            &[
                u64_field("st_dev", 2049),
                u64_field("st_ino", 123_456_789),
                ascii_field("canonical_root_basename", "hybrid-imoexf").unwrap(),
            ],
        )
        .unwrap();
        assert_eq!(
            sha256_hex(&root_preimage),
            "4974c52673995c4c2e2ea5ccdeb06a15f06787f937818a56a0f14eb0f6847d32"
        );

        let adoption_preimage = framed_record(
            1,
            "moex.stage8b.p1e.adoption-ready-owner.v1",
            &[
                digest_field("operational_identity_sha256", &sequential(0)).unwrap(),
                digest_field("restart_package_canonical_sha256", &sequential(0x40)).unwrap(),
                digest_field("first_boot_provenance_canonical_sha256", &sequential(0x60)).unwrap(),
                u64_field("seal_generation", 12),
                digest_field("seal_commitment_sha256", &sequential(0x80)).unwrap(),
                digest_field("stage6_checkpoint_sha256", &sequential(0xa0)).unwrap(),
                ascii_field("authenticated_stage5g_phase", "TimerReady").unwrap(),
                u64_field("pending_lifecycle_owner_count", 0),
                u64_field("pending_request_count", 0),
                u64_field("pending_deferred_timer_count", 0),
                bool_field("journal_mutation_uncertain", false),
                bool_field("seal_commit_uncertain", false),
            ],
        )
        .unwrap();
        let adoption_sha256 = sha256_hex(&adoption_preimage);
        assert_eq!(
            adoption_sha256,
            "abde6f078ba75013a849e73e9a7133a8830413d48437763dc5174b7784212ad2"
        );

        let receipt = Stage8bP1FirstBootReceiptV2 {
            schema_version: 2,
            domain: RECEIPT_DOMAIN.to_string(),
            receipt_generation: 1,
            transaction_id_sha256: transaction,
            bootstrap_attempt_generation: 11,
            deployment_identity_sha256: sequential(0),
            canonical_root_identity_sha256:
                "4974c52673995c4c2e2ea5ccdeb06a15f06787f937818a56a0f14eb0f6847d32".to_string(),
            restart_package_schema_version: 2,
            restart_package_canonical_sha256: sequential(0x40),
            first_boot_provenance_canonical_sha256: sequential(0x60),
            seal_generation: 12,
            seal_commitment_sha256: sequential(0x80),
            stage6_checkpoint_sha256: sequential(0xa0),
            adoption_predicate_version: 1,
            adoption_ready_owner_sha256: adoption_sha256,
            receipt_hmac_sha256: String::new(),
        };
        let key_bytes: Vec<u8> = (0xc0..=0xdf).collect();
        let key = Stage5gLifecycleCommitmentKey::from_secret_bytes(&key_bytes).unwrap();
        assert_eq!(
            key.stage8b_p1e_framed_hmac_sha256(&receipt.hmac_preimage().unwrap()),
            "439683f9ac6c2622cace2b538d1f87346199837728da6bd3ad3db9a465595d94"
        );
        assert!(hex(&receipt.hmac_preimage().unwrap()).starts_with("4d38425031453031"));
    }
}
