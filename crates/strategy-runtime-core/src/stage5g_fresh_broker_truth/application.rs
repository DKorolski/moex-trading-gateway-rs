//! Stage 5G-e-d-c linear fresh-truth application boundary.
//!
//! The implementation consumes one accepted reducer result and remains
//! paper-only. Redis, FINAM, broker dispatch and strategy callbacks are not
//! reachable from this module.

use crate::stage5g_order_position::{
    apply_stage5g_restart_canonical_order_position_state, Stage5gRestartCanonicalApplicationError,
};
use crate::{
    restore_stage5g_clean_restart, Stage5gCleanRestartedCapability, Stage5gLifecycleCommitmentKey,
};

use super::reducer::{
    disposition_id, reason_id, Stage5gFreshTruthApplicationParts, Stage5gFreshTruthReduction,
    Stage5gFreshTruthReductionReason,
};
use super::{
    Stage5gFreshTruthApplicationEvidenceV1, Stage5gRestartReconciliationDisposition,
    STAGE5G_FRESH_TRUTH_APPLICATION_EVIDENCE_SCHEMA_VERSION,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Stage5gFreshTruthApplicationError {
    CandidateDispositionMismatch,
    UnsupportedRestartLifecycle,
    CandidatePreflightFailed,
    CandidateCanonicalizationFailed,
    CanonicalTransitionFailed,
    ExactReplayDisabled,
    PostStateSemanticMismatch,
    PostPackageFailed,
    RestoredPackageMismatch,
    #[cfg(test)]
    InjectedFailure,
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Stage5gFreshTruthApplicationFailurePoint {
    BeforeCandidateExtraction,
    AfterCandidateExtraction,
    AfterPreflightBeforeTransition,
    InsideCanonicalTransition,
    AfterTransitionBeforeEquality,
    AfterEqualityBeforeExport,
    DuringSerialization,
    AfterBytesBeforeSourceDrop,
    AfterSourceDropBeforeDecode,
    DuringAuthenticationVerification,
    DuringRestore,
    AfterRestoreBeforeEvidenceEquality,
    BeforeDisabledReplayProjection,
    AfterDisabledReplayProjectionBeforeAuthentication,
}

pub(crate) struct Stage5gFreshTruthApplied {
    restored: Stage5gCleanRestartedCapability,
    canonical_package_bytes: Vec<u8>,
    evidence: Stage5gFreshTruthApplicationEvidenceV1,
}

impl Stage5gFreshTruthApplied {
    pub(crate) fn restored(&self) -> &Stage5gCleanRestartedCapability {
        &self.restored
    }

    pub(crate) fn canonical_package_bytes(&self) -> &[u8] {
        &self.canonical_package_bytes
    }

    pub(crate) fn evidence(&self) -> &Stage5gFreshTruthApplicationEvidenceV1 {
        &self.evidence
    }

    pub(crate) fn into_restored(self) -> Stage5gCleanRestartedCapability {
        self.restored
    }
}

pub(crate) struct Stage5gFreshTruthContinued {
    restart: Stage5gCleanRestartedCapability,
    scenario_id: &'static str,
    reason: Stage5gFreshTruthReductionReason,
    ignored_terminal_order_count: usize,
    ignored_historical_trade_count: usize,
}

impl Stage5gFreshTruthContinued {
    pub(crate) fn restart(&self) -> &Stage5gCleanRestartedCapability {
        &self.restart
    }

    pub(crate) fn scenario_id(&self) -> &'static str {
        self.scenario_id
    }

    pub(crate) fn reason(&self) -> Stage5gFreshTruthReductionReason {
        self.reason
    }

    pub(crate) fn history_counts(&self) -> (usize, usize) {
        (
            self.ignored_terminal_order_count,
            self.ignored_historical_trade_count,
        )
    }
}

pub(crate) struct Stage5gFreshTruthApplicationBlocked {
    restart: Stage5gCleanRestartedCapability,
    application_error: Option<Stage5gFreshTruthApplicationError>,
    disposition: Stage5gRestartReconciliationDisposition,
    scenario_id: &'static str,
    reason: Stage5gFreshTruthReductionReason,
    ignored_terminal_order_count: usize,
    ignored_historical_trade_count: usize,
}

impl Stage5gFreshTruthApplicationBlocked {
    pub(crate) fn restart(&self) -> &Stage5gCleanRestartedCapability {
        &self.restart
    }

    pub(crate) fn application_error(&self) -> Option<Stage5gFreshTruthApplicationError> {
        self.application_error
    }

    pub(crate) fn disposition(&self) -> Stage5gRestartReconciliationDisposition {
        self.disposition
    }

    pub(crate) fn scenario_id(&self) -> &'static str {
        self.scenario_id
    }

    pub(crate) fn reason(&self) -> Stage5gFreshTruthReductionReason {
        self.reason
    }

    pub(crate) fn history_counts(&self) -> (usize, usize) {
        (
            self.ignored_terminal_order_count,
            self.ignored_historical_trade_count,
        )
    }
}

pub(crate) enum Stage5gFreshTruthApplicationResult {
    Applied(Box<Stage5gFreshTruthApplied>),
    Continued(Box<Stage5gFreshTruthContinued>),
    Blocked(Box<Stage5gFreshTruthApplicationBlocked>),
}

/// The single owning e-d-c entry point. No overload accepts a candidate,
/// broker rows, package identifiers or an external mutable runtime.
pub(crate) fn apply_stage5g_fresh_truth_reduction(
    reduction: Stage5gFreshTruthReduction,
    commitment_key: &Stage5gLifecycleCommitmentKey,
) -> Stage5gFreshTruthApplicationResult {
    apply_stage5g_fresh_truth_reduction_inner(reduction, commitment_key, None)
}

#[cfg(test)]
pub(crate) fn apply_stage5g_fresh_truth_reduction_with_failure(
    reduction: Stage5gFreshTruthReduction,
    commitment_key: &Stage5gLifecycleCommitmentKey,
    failure_point: Stage5gFreshTruthApplicationFailurePoint,
) -> Stage5gFreshTruthApplicationResult {
    apply_stage5g_fresh_truth_reduction_inner(reduction, commitment_key, Some(failure_point))
}

fn apply_stage5g_fresh_truth_reduction_inner(
    reduction: Stage5gFreshTruthReduction,
    commitment_key: &Stage5gLifecycleCommitmentKey,
    #[cfg(test)] failure_point: Option<Stage5gFreshTruthApplicationFailurePoint>,
    #[cfg(not(test))] _failure_point: Option<()>,
) -> Stage5gFreshTruthApplicationResult {
    let parts = reduction.into_application_parts();
    match parts.disposition {
        Stage5gRestartReconciliationDisposition::ApplyOwnedCandidate => apply_owned_candidate(
            parts,
            commitment_key,
            #[cfg(test)]
            failure_point,
        ),
        Stage5gRestartReconciliationDisposition::ContinueFromCommittedCheckpoint => {
            if parts.candidate.is_some() {
                blocked(
                    parts,
                    Some(Stage5gFreshTruthApplicationError::CandidateDispositionMismatch),
                )
            } else {
                Stage5gFreshTruthApplicationResult::Continued(Box::new(
                    Stage5gFreshTruthContinued {
                        restart: parts.restart,
                        scenario_id: parts.scenario_id.frozen_id(),
                        reason: parts.reason,
                        ignored_terminal_order_count: parts.ignored_unrelated_terminal_order_count,
                        ignored_historical_trade_count: parts
                            .ignored_unrelated_historical_trade_count,
                    },
                ))
            }
        }
        Stage5gRestartReconciliationDisposition::ExactReplay => blocked(
            parts,
            Some(Stage5gFreshTruthApplicationError::ExactReplayDisabled),
        ),
        Stage5gRestartReconciliationDisposition::AwaitFreshBrokerTruth
        | Stage5gRestartReconciliationDisposition::ReconciliationRequired
        | Stage5gRestartReconciliationDisposition::ManualInterventionRequired
        | Stage5gRestartReconciliationDisposition::TerminalInconsistency => {
            if parts.candidate.is_some() {
                blocked(
                    parts,
                    Some(Stage5gFreshTruthApplicationError::CandidateDispositionMismatch),
                )
            } else {
                blocked(parts, None)
            }
        }
    }
}

fn apply_owned_candidate(
    mut parts: Stage5gFreshTruthApplicationParts,
    commitment_key: &Stage5gLifecycleCommitmentKey,
    #[cfg(test)] failure_point: Option<Stage5gFreshTruthApplicationFailurePoint>,
) -> Stage5gFreshTruthApplicationResult {
    #[cfg(test)]
    if failure_point == Some(Stage5gFreshTruthApplicationFailurePoint::BeforeCandidateExtraction) {
        return blocked(
            parts,
            Some(Stage5gFreshTruthApplicationError::InjectedFailure),
        );
    }
    let Some(candidate) = parts.candidate.take() else {
        return blocked(
            parts,
            Some(Stage5gFreshTruthApplicationError::CandidateDispositionMismatch),
        );
    };
    #[cfg(test)]
    if failure_point == Some(Stage5gFreshTruthApplicationFailurePoint::AfterCandidateExtraction) {
        return blocked_with_candidate_dropped(
            parts,
            candidate,
            Stage5gFreshTruthApplicationError::InjectedFailure,
        );
    }
    let Some(pre_state) = parts.restart.stage5g_restart_order_position_state() else {
        return blocked_with_candidate_dropped(
            parts,
            candidate,
            Stage5gFreshTruthApplicationError::UnsupportedRestartLifecycle,
        );
    };
    if !candidate.application_preflight_matches(
        &parts.restart,
        &parts.truth,
        &parts.post_candidate_fingerprint_sha256,
    ) {
        return blocked_with_candidate_dropped(
            parts,
            candidate,
            Stage5gFreshTruthApplicationError::CandidatePreflightFailed,
        );
    }
    #[cfg(test)]
    if failure_point
        == Some(Stage5gFreshTruthApplicationFailurePoint::AfterPreflightBeforeTransition)
    {
        return blocked_with_candidate_dropped(
            parts,
            candidate,
            Stage5gFreshTruthApplicationError::InjectedFailure,
        );
    }
    let canonical_evidence =
        match candidate.canonical_order_position_evidence(&pre_state, &parts.truth) {
            Ok(evidence) => evidence,
            Err(_) => {
                return blocked_with_candidate_dropped(
                    parts,
                    candidate,
                    Stage5gFreshTruthApplicationError::CandidateCanonicalizationFailed,
                );
            }
        };
    #[cfg(test)]
    if matches!(
        failure_point,
        Some(Stage5gFreshTruthApplicationFailurePoint::InsideCanonicalTransition)
            | Some(Stage5gFreshTruthApplicationFailurePoint::BeforeDisabledReplayProjection)
    ) {
        return blocked_with_candidate_dropped(
            parts,
            candidate,
            Stage5gFreshTruthApplicationError::InjectedFailure,
        );
    }
    let post_state =
        match apply_stage5g_restart_canonical_order_position_state(pre_state, canonical_evidence) {
            Ok(state) => state,
            Err(Stage5gRestartCanonicalApplicationError::ExactReplayDisabled) => {
                return blocked_with_candidate_dropped(
                    parts,
                    candidate,
                    Stage5gFreshTruthApplicationError::ExactReplayDisabled,
                );
            }
            Err(Stage5gRestartCanonicalApplicationError::OrderPosition(_)) => {
                return blocked_with_candidate_dropped(
                    parts,
                    candidate,
                    Stage5gFreshTruthApplicationError::CanonicalTransitionFailed,
                );
            }
        };
    #[cfg(test)]
    if failure_point
        == Some(Stage5gFreshTruthApplicationFailurePoint::AfterTransitionBeforeEquality)
    {
        return blocked_with_candidate_dropped(
            parts,
            candidate,
            Stage5gFreshTruthApplicationError::InjectedFailure,
        );
    }
    if !candidate.post_state_matches(&post_state) {
        return blocked_with_candidate_dropped(
            parts,
            candidate,
            Stage5gFreshTruthApplicationError::PostStateSemanticMismatch,
        );
    }
    #[cfg(test)]
    if matches!(
        failure_point,
        Some(Stage5gFreshTruthApplicationFailurePoint::AfterEqualityBeforeExport)
            | Some(Stage5gFreshTruthApplicationFailurePoint::DuringSerialization)
            | Some(
                Stage5gFreshTruthApplicationFailurePoint::AfterDisabledReplayProjectionBeforeAuthentication
            )
    ) {
        return blocked_with_candidate_dropped(
            parts,
            candidate,
            Stage5gFreshTruthApplicationError::InjectedFailure,
        );
    }

    let candidate_fingerprint = candidate.fingerprint();
    let fresh_package_id = parts.truth.package.package_id.as_str().to_string();
    let fresh_snapshot_epoch = parts.truth.package.snapshot_epoch.as_str().to_string();
    let post_restart_package_fingerprint = match parts
        .restart
        .stage5g_post_application_package_fingerprint_sha256(
            &post_state,
            &fresh_package_id,
            &fresh_snapshot_epoch,
            &candidate_fingerprint,
        ) {
        Ok(value) => value,
        Err(_) => {
            return blocked_with_candidate_dropped(
                parts,
                candidate,
                Stage5gFreshTruthApplicationError::PostPackageFailed,
            );
        }
    };
    let evidence = Stage5gFreshTruthApplicationEvidenceV1 {
        schema_version: STAGE5G_FRESH_TRUTH_APPLICATION_EVIDENCE_SCHEMA_VERSION,
        scenario_id: parts.scenario_id.frozen_id().to_string(),
        disposition: disposition_id(parts.disposition).to_string(),
        reason: reason_id(parts.reason),
        operational_identity_commitment_sha256: parts
            .truth
            .operational_binding_commitment_sha256
            .clone(),
        fresh_package_id: fresh_package_id.clone(),
        fresh_snapshot_epoch: fresh_snapshot_epoch.clone(),
        fresh_package_fingerprint_sha256: parts.truth.package.canonical_fingerprint_sha256.clone(),
        pre_restart_package_fingerprint_sha256: parts
            .restart
            .stage5g_pre_restart_package_fingerprint_sha256(),
        reduction_pre_semantic_fingerprint_sha256: parts.pre_semantic_fingerprint_sha256.clone(),
        candidate_fingerprint_sha256: candidate_fingerprint.clone(),
        applied_post_semantic_fingerprint_sha256: candidate_fingerprint,
        post_restart_package_fingerprint_sha256: post_restart_package_fingerprint,
        ignored_terminal_order_count: parts.ignored_unrelated_terminal_order_count,
        ignored_historical_trade_count: parts.ignored_unrelated_historical_trade_count,
        runtime_transition_applied: true,
        callback_invoked: false,
        transport_opened: false,
        exact_replay_enabled: false,
    };
    let fresh_runtime = parts.restart.stage5g_fresh_reconstruction_candidate();
    let package_bytes = match parts
        .restart
        .stage5g_export_post_application_order_position(
            post_state,
            &fresh_package_id,
            &fresh_snapshot_epoch,
            parts.truth.package.captured_at,
            evidence.clone(),
            commitment_key,
        ) {
        Ok(bytes) => bytes,
        Err(_) => {
            return blocked_with_candidate_dropped(
                parts,
                candidate,
                Stage5gFreshTruthApplicationError::PostPackageFailed,
            );
        }
    };
    #[cfg(test)]
    if matches!(
        failure_point,
        Some(Stage5gFreshTruthApplicationFailurePoint::AfterBytesBeforeSourceDrop)
            | Some(Stage5gFreshTruthApplicationFailurePoint::AfterSourceDropBeforeDecode)
            | Some(Stage5gFreshTruthApplicationFailurePoint::DuringAuthenticationVerification)
            | Some(Stage5gFreshTruthApplicationFailurePoint::DuringRestore)
    ) {
        return blocked_with_candidate_dropped(
            parts,
            candidate,
            Stage5gFreshTruthApplicationError::InjectedFailure,
        );
    }

    // The source post-state was consumed by export. The pre-application
    // restart remains the rollback authority until fresh decode/verify/restore
    // has succeeded; it is destroyed only at final commit below.
    let restored =
        match restore_stage5g_clean_restart(&package_bytes, commitment_key, fresh_runtime) {
            Ok(restored) => restored,
            Err(_) => {
                return blocked_with_candidate_dropped(
                    parts,
                    candidate,
                    Stage5gFreshTruthApplicationError::PostPackageFailed,
                );
            }
        };
    let restored_matches = restored.stage5g_application_evidence() == Some(&evidence)
        && restored
            .stage5g_restart_order_position_state()
            .is_some_and(|state| candidate.post_state_matches(&state));
    #[cfg(test)]
    if failure_point
        == Some(Stage5gFreshTruthApplicationFailurePoint::AfterRestoreBeforeEvidenceEquality)
    {
        drop(restored);
        return blocked_with_candidate_dropped(
            parts,
            candidate,
            Stage5gFreshTruthApplicationError::InjectedFailure,
        );
    }
    if !restored_matches {
        drop(restored);
        return blocked_with_candidate_dropped(
            parts,
            candidate,
            Stage5gFreshTruthApplicationError::RestoredPackageMismatch,
        );
    }
    drop(candidate);
    drop(parts.truth);
    drop(parts.restart);
    Stage5gFreshTruthApplicationResult::Applied(Box::new(Stage5gFreshTruthApplied {
        restored,
        canonical_package_bytes: package_bytes,
        evidence,
    }))
}

fn blocked(
    parts: Stage5gFreshTruthApplicationParts,
    application_error: Option<Stage5gFreshTruthApplicationError>,
) -> Stage5gFreshTruthApplicationResult {
    Stage5gFreshTruthApplicationResult::Blocked(Box::new(Stage5gFreshTruthApplicationBlocked {
        restart: parts.restart,
        application_error,
        disposition: parts.disposition,
        scenario_id: parts.scenario_id.frozen_id(),
        reason: parts.reason,
        ignored_terminal_order_count: parts.ignored_unrelated_terminal_order_count,
        ignored_historical_trade_count: parts.ignored_unrelated_historical_trade_count,
    }))
}

fn blocked_with_candidate_dropped(
    parts: Stage5gFreshTruthApplicationParts,
    candidate: super::reducer::Stage5gOwnedReconciliationCandidate,
    error: Stage5gFreshTruthApplicationError,
) -> Stage5gFreshTruthApplicationResult {
    drop(candidate);
    blocked(parts, Some(error))
}
