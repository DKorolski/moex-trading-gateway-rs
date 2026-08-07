//! Stage 5G-e-d-c linear fresh-truth application boundary.
//!
//! The implementation consumes one accepted reducer result and remains
//! paper-only. Redis, FINAM, broker dispatch and strategy callbacks are not
//! reachable from this module.

#[cfg(not(test))]
use crate::restore_stage5g_clean_restart;
use crate::stage5g_order_position::{
    apply_stage5g_restart_canonical_order_position_state, stage5g_restart_application_class,
    stage5g_restart_application_expectation, stage5g_restart_application_global_invariants,
    stage5g_restart_state_semantic_sha256, Stage5gRestartApplicationClass,
    Stage5gRestartCanonicalApplicationError,
};
#[cfg(test)]
use crate::stage5g_order_position::{
    stage5g_test_fail_restart_canonical_before_commit,
    stage5g_test_mutate_restart_application_state, Stage5gRestartApplicationMismatch,
};
use crate::{Stage5gCleanRestartedCapability, Stage5gLifecycleCommitmentKey};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::reducer::{
    disposition_id, reason_id, Stage5gFreshTruthApplicationParts, Stage5gFreshTruthReduction,
    Stage5gFreshTruthReductionReason,
};
#[cfg(test)]
use super::Stage5gRestartScenarioId;
use super::{
    Stage5gRestartReconciliationDisposition,
    STAGE5G_FRESH_TRUTH_APPLICATION_EVIDENCE_SCHEMA_VERSION,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Stage5gFreshTruthApplicationEvidenceV1 {
    schema_version: u16,
    scenario_id: String,
    disposition: String,
    reason: String,
    operational_identity_commitment_sha256: String,
    command_request_id: String,
    parent_snapshot_id: String,
    parent_snapshot_revision: u64,
    fresh_package_id: String,
    fresh_snapshot_epoch: String,
    fresh_captured_at: DateTime<Utc>,
    fresh_package_fingerprint_sha256: String,
    pre_restart_package_fingerprint_sha256: String,
    reduction_pre_semantic_fingerprint_sha256: String,
    application_source_proof_sha256: String,
    candidate_fingerprint_sha256: String,
    applied_post_semantic_fingerprint_sha256: String,
    restored_post_semantic_fingerprint_sha256: String,
    pre_global_state_fingerprint_sha256: String,
    post_global_state_fingerprint_sha256: String,
    restored_global_state_fingerprint_sha256: String,
    post_restart_package_fingerprint_sha256: String,
    ignored_terminal_order_count: usize,
    ignored_historical_trade_count: usize,
    runtime_transition_applied: bool,
    callback_invoked: bool,
    transport_opened: bool,
    exact_replay_enabled: bool,
    global_state_invariants_proven: bool,
}

impl Stage5gFreshTruthApplicationEvidenceV1 {
    pub(crate) fn scenario_id(&self) -> &str {
        &self.scenario_id
    }

    pub(crate) fn candidate_fingerprint_sha256(&self) -> &str {
        &self.candidate_fingerprint_sha256
    }

    pub(crate) fn parent_snapshot_id(&self) -> &str {
        &self.parent_snapshot_id
    }

    pub(crate) fn parent_snapshot_revision(&self) -> u64 {
        self.parent_snapshot_revision
    }

    pub(crate) fn fresh_package_id(&self) -> &str {
        &self.fresh_package_id
    }

    pub(crate) fn fresh_snapshot_epoch(&self) -> &str {
        &self.fresh_snapshot_epoch
    }

    pub(crate) fn fresh_captured_at(&self) -> DateTime<Utc> {
        self.fresh_captured_at
    }

    pub(crate) fn post_restart_package_fingerprint_sha256(&self) -> &str {
        &self.post_restart_package_fingerprint_sha256
    }

    pub(crate) fn applied_post_semantic_fingerprint_sha256(&self) -> &str {
        &self.applied_post_semantic_fingerprint_sha256
    }

    pub(crate) fn restored_post_semantic_fingerprint_sha256(&self) -> &str {
        &self.restored_post_semantic_fingerprint_sha256
    }

    pub(crate) fn restored_global_state_fingerprint_sha256(&self) -> &str {
        &self.restored_global_state_fingerprint_sha256
    }

    pub(crate) fn post_global_state_fingerprint_sha256(&self) -> &str {
        &self.post_global_state_fingerprint_sha256
    }

    pub(crate) fn runtime_transition_applied(&self) -> bool {
        self.runtime_transition_applied
    }

    pub(crate) fn callback_invoked(&self) -> bool {
        self.callback_invoked
    }

    pub(crate) fn transport_opened(&self) -> bool {
        self.transport_opened
    }

    pub(crate) fn exact_replay_enabled(&self) -> bool {
        self.exact_replay_enabled
    }
}

pub(crate) fn stage5g_application_evidence_is_valid(
    evidence: &Stage5gFreshTruthApplicationEvidenceV1,
) -> bool {
    let sha256 = |value: &str| {
        value.len() == 64
            && value
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    };
    evidence.schema_version == STAGE5G_FRESH_TRUTH_APPLICATION_EVIDENCE_SCHEMA_VERSION
        && !evidence.scenario_id.is_empty()
        && evidence.disposition == "apply_owned_candidate"
        && !evidence.reason.is_empty()
        && !evidence.command_request_id.is_empty()
        && !evidence.parent_snapshot_id.is_empty()
        && evidence.parent_snapshot_revision > 0
        && !evidence.fresh_package_id.is_empty()
        && !evidence.fresh_snapshot_epoch.is_empty()
        && sha256(&evidence.operational_identity_commitment_sha256)
        && sha256(&evidence.fresh_package_fingerprint_sha256)
        && sha256(&evidence.pre_restart_package_fingerprint_sha256)
        && sha256(&evidence.reduction_pre_semantic_fingerprint_sha256)
        && sha256(&evidence.application_source_proof_sha256)
        && evidence.application_source_proof_sha256
            == stage5g_application_source_proof_sha256_from_evidence(evidence)
        && sha256(&evidence.candidate_fingerprint_sha256)
        && sha256(&evidence.applied_post_semantic_fingerprint_sha256)
        && sha256(&evidence.restored_post_semantic_fingerprint_sha256)
        && sha256(&evidence.pre_global_state_fingerprint_sha256)
        && sha256(&evidence.post_global_state_fingerprint_sha256)
        && sha256(&evidence.restored_global_state_fingerprint_sha256)
        && sha256(&evidence.post_restart_package_fingerprint_sha256)
        && evidence.candidate_fingerprint_sha256
            == evidence.applied_post_semantic_fingerprint_sha256
        && evidence.applied_post_semantic_fingerprint_sha256
            == evidence.restored_post_semantic_fingerprint_sha256
        && evidence.post_global_state_fingerprint_sha256
            == evidence.restored_global_state_fingerprint_sha256
        && evidence.runtime_transition_applied
        && !evidence.callback_invoked
        && !evidence.transport_opened
        && !evidence.exact_replay_enabled
        && evidence.global_state_invariants_proven
}

/// Linear proof that lineage values used in e-d-c evidence were read from the
/// consumed e-d-b reduction/restart/fresh-truth authorities. It has private
/// fields and no Clone/Serialize implementation; ordinary callers can neither
/// mint it from strings nor duplicate it.
pub(crate) struct Stage5gFreshTruthApplicationSourceProof {
    scenario_id: String,
    disposition: String,
    reason: String,
    operational_identity_commitment_sha256: String,
    command_request_id: String,
    parent_snapshot_id: String,
    parent_snapshot_revision: u64,
    fresh_package_id: String,
    fresh_snapshot_epoch: String,
    fresh_captured_at: DateTime<Utc>,
    fresh_package_fingerprint_sha256: String,
    pre_restart_package_fingerprint_sha256: String,
    reduction_pre_semantic_fingerprint_sha256: String,
    ignored_terminal_order_count: usize,
    ignored_historical_trade_count: usize,
}

fn stage5g_application_source_proof_sha256_from_evidence(
    evidence: &Stage5gFreshTruthApplicationEvidenceV1,
) -> String {
    #[derive(Serialize)]
    struct SourceProofProjection<'a> {
        domain: &'static str,
        scenario_id: &'a str,
        disposition: &'a str,
        reason: &'a str,
        operational_identity_commitment_sha256: &'a str,
        command_request_id: &'a str,
        parent_snapshot_id: &'a str,
        parent_snapshot_revision: u64,
        fresh_package_id: &'a str,
        fresh_snapshot_epoch: &'a str,
        fresh_captured_at: DateTime<Utc>,
        fresh_package_fingerprint_sha256: &'a str,
        pre_restart_package_fingerprint_sha256: &'a str,
        reduction_pre_semantic_fingerprint_sha256: &'a str,
        ignored_terminal_order_count: usize,
        ignored_historical_trade_count: usize,
    }
    let bytes = serde_json::to_vec(&SourceProofProjection {
        domain: "moex.stage5g.edc.application-source-proof.v1",
        scenario_id: &evidence.scenario_id,
        disposition: &evidence.disposition,
        reason: &evidence.reason,
        operational_identity_commitment_sha256: &evidence.operational_identity_commitment_sha256,
        command_request_id: &evidence.command_request_id,
        parent_snapshot_id: &evidence.parent_snapshot_id,
        parent_snapshot_revision: evidence.parent_snapshot_revision,
        fresh_package_id: &evidence.fresh_package_id,
        fresh_snapshot_epoch: &evidence.fresh_snapshot_epoch,
        fresh_captured_at: evidence.fresh_captured_at,
        fresh_package_fingerprint_sha256: &evidence.fresh_package_fingerprint_sha256,
        pre_restart_package_fingerprint_sha256: &evidence.pre_restart_package_fingerprint_sha256,
        reduction_pre_semantic_fingerprint_sha256: &evidence
            .reduction_pre_semantic_fingerprint_sha256,
        ignored_terminal_order_count: evidence.ignored_terminal_order_count,
        ignored_historical_trade_count: evidence.ignored_historical_trade_count,
    })
    .expect("application source proof evidence projection serializes");
    format!("{:x}", Sha256::digest(bytes))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(clippy::enum_variant_names)]
pub(crate) enum Stage5gFreshTruthApplicationSourceMismatch {
    WrongScenario,
    WrongDisposition,
    WrongReason,
    WrongOperationalIdentity,
    WrongCommandRequest,
    WrongParentSnapshot,
    WrongFreshPackageId,
    WrongFreshEpoch,
    WrongFreshCapturedAt,
    WrongFreshFingerprint,
    WrongPreRestartFingerprint,
    WrongReductionPreFingerprint,
    WrongHistoryCounts,
    WrongSourceProofCommitment,
}

impl Stage5gFreshTruthApplicationSourceProof {
    fn from_application_parts(
        parts: &Stage5gFreshTruthApplicationParts,
        candidate: &super::reducer::Stage5gOwnedReconciliationCandidate,
    ) -> Self {
        let parent = parts.restart.stage5g_application_parent_snapshot_binding();
        Self {
            scenario_id: parts.scenario_id.frozen_id().to_string(),
            disposition: disposition_id(parts.disposition).to_string(),
            reason: reason_id(parts.reason),
            operational_identity_commitment_sha256: parts
                .truth
                .operational_binding_commitment_sha256
                .clone(),
            command_request_id: candidate.command_request_id().to_owned(),
            parent_snapshot_id: parent.0,
            parent_snapshot_revision: parent.1,
            fresh_package_id: parts.truth.package.package_id.as_str().to_string(),
            fresh_snapshot_epoch: parts.truth.package.snapshot_epoch.as_str().to_string(),
            fresh_captured_at: parts.truth.package.captured_at,
            fresh_package_fingerprint_sha256: parts
                .truth
                .package
                .canonical_fingerprint_sha256
                .clone(),
            pre_restart_package_fingerprint_sha256: parts
                .restart
                .stage5g_pre_restart_package_fingerprint_sha256(),
            reduction_pre_semantic_fingerprint_sha256: parts
                .pre_semantic_fingerprint_sha256
                .clone(),
            ignored_terminal_order_count: parts.ignored_unrelated_terminal_order_count,
            ignored_historical_trade_count: parts.ignored_unrelated_historical_trade_count,
        }
    }

    fn commitment_sha256(&self) -> String {
        #[derive(Serialize)]
        struct SourceProofProjection<'a> {
            domain: &'static str,
            scenario_id: &'a str,
            disposition: &'a str,
            reason: &'a str,
            operational_identity_commitment_sha256: &'a str,
            command_request_id: &'a str,
            parent_snapshot_id: &'a str,
            parent_snapshot_revision: u64,
            fresh_package_id: &'a str,
            fresh_snapshot_epoch: &'a str,
            fresh_captured_at: DateTime<Utc>,
            fresh_package_fingerprint_sha256: &'a str,
            pre_restart_package_fingerprint_sha256: &'a str,
            reduction_pre_semantic_fingerprint_sha256: &'a str,
            ignored_terminal_order_count: usize,
            ignored_historical_trade_count: usize,
        }
        let bytes = serde_json::to_vec(&SourceProofProjection {
            domain: "moex.stage5g.edc.application-source-proof.v1",
            scenario_id: &self.scenario_id,
            disposition: &self.disposition,
            reason: &self.reason,
            operational_identity_commitment_sha256: &self.operational_identity_commitment_sha256,
            command_request_id: &self.command_request_id,
            parent_snapshot_id: &self.parent_snapshot_id,
            parent_snapshot_revision: self.parent_snapshot_revision,
            fresh_package_id: &self.fresh_package_id,
            fresh_snapshot_epoch: &self.fresh_snapshot_epoch,
            fresh_captured_at: self.fresh_captured_at,
            fresh_package_fingerprint_sha256: &self.fresh_package_fingerprint_sha256,
            pre_restart_package_fingerprint_sha256: &self.pre_restart_package_fingerprint_sha256,
            reduction_pre_semantic_fingerprint_sha256: &self
                .reduction_pre_semantic_fingerprint_sha256,
            ignored_terminal_order_count: self.ignored_terminal_order_count,
            ignored_historical_trade_count: self.ignored_historical_trade_count,
        })
        .expect("private application source proof serializes");
        format!("{:x}", Sha256::digest(bytes))
    }
}

pub(crate) fn validate_stage5g_application_evidence_against_source(
    evidence: &Stage5gFreshTruthApplicationEvidenceV1,
    source: &Stage5gFreshTruthApplicationSourceProof,
) -> Result<(), Stage5gFreshTruthApplicationSourceMismatch> {
    if evidence.scenario_id != source.scenario_id {
        return Err(Stage5gFreshTruthApplicationSourceMismatch::WrongScenario);
    }
    if evidence.disposition != source.disposition {
        return Err(Stage5gFreshTruthApplicationSourceMismatch::WrongDisposition);
    }
    if evidence.reason != source.reason {
        return Err(Stage5gFreshTruthApplicationSourceMismatch::WrongReason);
    }
    if evidence.operational_identity_commitment_sha256
        != source.operational_identity_commitment_sha256
    {
        return Err(Stage5gFreshTruthApplicationSourceMismatch::WrongOperationalIdentity);
    }
    if evidence.command_request_id != source.command_request_id {
        return Err(Stage5gFreshTruthApplicationSourceMismatch::WrongCommandRequest);
    }
    if evidence.parent_snapshot_id != source.parent_snapshot_id
        || evidence.parent_snapshot_revision != source.parent_snapshot_revision
    {
        return Err(Stage5gFreshTruthApplicationSourceMismatch::WrongParentSnapshot);
    }
    if evidence.fresh_package_id != source.fresh_package_id {
        return Err(Stage5gFreshTruthApplicationSourceMismatch::WrongFreshPackageId);
    }
    if evidence.fresh_snapshot_epoch != source.fresh_snapshot_epoch {
        return Err(Stage5gFreshTruthApplicationSourceMismatch::WrongFreshEpoch);
    }
    if evidence.fresh_captured_at != source.fresh_captured_at {
        return Err(Stage5gFreshTruthApplicationSourceMismatch::WrongFreshCapturedAt);
    }
    if evidence.fresh_package_fingerprint_sha256 != source.fresh_package_fingerprint_sha256 {
        return Err(Stage5gFreshTruthApplicationSourceMismatch::WrongFreshFingerprint);
    }
    if evidence.pre_restart_package_fingerprint_sha256
        != source.pre_restart_package_fingerprint_sha256
    {
        return Err(Stage5gFreshTruthApplicationSourceMismatch::WrongPreRestartFingerprint);
    }
    if evidence.reduction_pre_semantic_fingerprint_sha256
        != source.reduction_pre_semantic_fingerprint_sha256
    {
        return Err(Stage5gFreshTruthApplicationSourceMismatch::WrongReductionPreFingerprint);
    }
    if evidence.ignored_terminal_order_count != source.ignored_terminal_order_count
        || evidence.ignored_historical_trade_count != source.ignored_historical_trade_count
    {
        return Err(Stage5gFreshTruthApplicationSourceMismatch::WrongHistoryCounts);
    }
    if evidence.application_source_proof_sha256 != source.commitment_sha256() {
        return Err(Stage5gFreshTruthApplicationSourceMismatch::WrongSourceProofCommitment);
    }
    Ok(())
}

#[cfg(test)]
fn stage5g_test_mutate_application_source_evidence(
    evidence: &mut Stage5gFreshTruthApplicationEvidenceV1,
    mutation: Stage5gFreshTruthApplicationSourceMutation,
) {
    match mutation {
        Stage5gFreshTruthApplicationSourceMutation::FreshPackageFingerprint => {
            evidence.fresh_package_fingerprint_sha256 = "b".repeat(64);
        }
        Stage5gFreshTruthApplicationSourceMutation::PreRestartPackageFingerprint => {
            evidence.pre_restart_package_fingerprint_sha256 = "b".repeat(64);
        }
        Stage5gFreshTruthApplicationSourceMutation::ReductionPreSemanticFingerprint => {
            evidence.reduction_pre_semantic_fingerprint_sha256 = "b".repeat(64);
        }
        Stage5gFreshTruthApplicationSourceMutation::OperationalIdentityCommitment => {
            evidence.operational_identity_commitment_sha256 = "b".repeat(64);
        }
        Stage5gFreshTruthApplicationSourceMutation::FreshPackageId => {
            evidence.fresh_package_id = "R2-FORGED-FRESH-PACKAGE".to_owned();
        }
        Stage5gFreshTruthApplicationSourceMutation::FreshSnapshotEpoch => {
            evidence.fresh_snapshot_epoch = "R2-FORGED-FRESH-EPOCH".to_owned();
        }
        Stage5gFreshTruthApplicationSourceMutation::FreshCapturedAt => {
            evidence.fresh_captured_at += chrono::Duration::seconds(1);
        }
        Stage5gFreshTruthApplicationSourceMutation::SwapFreshIdAndEpoch => {
            std::mem::swap(
                &mut evidence.fresh_package_id,
                &mut evidence.fresh_snapshot_epoch,
            );
        }
        Stage5gFreshTruthApplicationSourceMutation::HistoryCounts => {
            evidence.ignored_terminal_order_count += 1;
            evidence.ignored_historical_trade_count += 1;
        }
        Stage5gFreshTruthApplicationSourceMutation::SourceProofCommitment => {
            evidence.application_source_proof_sha256 = "b".repeat(64);
        }
    }
}

/// Linear proof that the sole e-d-c owner completed preflight, the canonical
/// state transition and independent candidate/post-state equality. It is
/// deliberately non-Clone/non-Serialize and has no crate-visible constructor.
pub(crate) struct Stage5gValidatedPostApplication {
    state: crate::stage5g_order_position::Stage5gOrderPositionState,
    fresh_package_id: String,
    fresh_snapshot_epoch: String,
    captured_at: DateTime<Utc>,
    evidence: Stage5gFreshTruthApplicationEvidenceV1,
    #[cfg(test)]
    failure_point: Option<Stage5gFreshTruthApplicationFailurePoint>,
}

pub(crate) struct Stage5gValidatedPostApplicationExportParts {
    pub(crate) state: crate::stage5g_order_position::Stage5gOrderPositionState,
    pub(crate) fresh_package_id: String,
    pub(crate) fresh_snapshot_epoch: String,
    pub(crate) captured_at: DateTime<Utc>,
    pub(crate) evidence: Stage5gFreshTruthApplicationEvidenceV1,
    #[cfg(test)]
    pub(crate) failure_point: Option<Stage5gFreshTruthApplicationFailurePoint>,
}

pub(crate) struct Stage5gFinalizedPostApplication {
    state: crate::stage5g_order_position::Stage5gOrderPositionState,
    fresh_package_id: String,
    fresh_snapshot_epoch: String,
    captured_at: DateTime<Utc>,
    evidence: Stage5gFreshTruthApplicationEvidenceV1,
    authority_commitment_sha256: String,
    #[cfg(test)]
    failure_point: Option<Stage5gFreshTruthApplicationFailurePoint>,
}

pub(crate) struct Stage5gFinalizedPostApplicationExportParts {
    pub(crate) state: crate::stage5g_order_position::Stage5gOrderPositionState,
    pub(crate) fresh_package_id: String,
    pub(crate) fresh_snapshot_epoch: String,
    pub(crate) captured_at: DateTime<Utc>,
    pub(crate) evidence: Stage5gFreshTruthApplicationEvidenceV1,
    pub(crate) authority_commitment_sha256: String,
    #[cfg(test)]
    pub(crate) failure_point: Option<Stage5gFreshTruthApplicationFailurePoint>,
}

impl Stage5gValidatedPostApplication {
    fn new(
        state: crate::stage5g_order_position::Stage5gOrderPositionState,
        source_proof: Stage5gFreshTruthApplicationSourceProof,
        evidence: Stage5gFreshTruthApplicationEvidenceV1,
        #[cfg(test)] failure_point: Option<Stage5gFreshTruthApplicationFailurePoint>,
    ) -> Result<Self, Stage5gFreshTruthApplicationSourceMismatch> {
        validate_stage5g_application_evidence_against_source(&evidence, &source_proof)?;
        Ok(Self {
            state,
            fresh_package_id: source_proof.fresh_package_id,
            fresh_snapshot_epoch: source_proof.fresh_snapshot_epoch,
            captured_at: source_proof.fresh_captured_at,
            evidence,
            #[cfg(test)]
            failure_point,
        })
    }

    pub(crate) fn into_export_parts(self) -> Stage5gValidatedPostApplicationExportParts {
        Stage5gValidatedPostApplicationExportParts {
            state: self.state,
            fresh_package_id: self.fresh_package_id,
            fresh_snapshot_epoch: self.fresh_snapshot_epoch,
            captured_at: self.captured_at,
            evidence: self.evidence,
            #[cfg(test)]
            failure_point: self.failure_point,
        }
    }
}

impl Stage5gValidatedPostApplicationExportParts {
    pub(crate) fn finalize_post_restart_package_fingerprint(
        mut self,
        fingerprint: String,
    ) -> Stage5gFinalizedPostApplication {
        self.evidence.post_restart_package_fingerprint_sha256 = fingerprint;
        let authority_commitment_sha256 = stage5g_application_authority_sha256(&self.evidence);
        Stage5gFinalizedPostApplication {
            state: self.state,
            fresh_package_id: self.fresh_package_id,
            fresh_snapshot_epoch: self.fresh_snapshot_epoch,
            captured_at: self.captured_at,
            evidence: self.evidence,
            authority_commitment_sha256,
            #[cfg(test)]
            failure_point: self.failure_point,
        }
    }

    pub(crate) fn evidence_matches_owned_tuple(&self) -> bool {
        self.evidence.fresh_package_id == self.fresh_package_id
            && self.evidence.fresh_snapshot_epoch == self.fresh_snapshot_epoch
    }

    pub(crate) fn candidate_fingerprint_sha256(&self) -> &str {
        &self.evidence.candidate_fingerprint_sha256
    }

    #[cfg(test)]
    pub(crate) fn fails_at(&self, point: Stage5gFreshTruthApplicationFailurePoint) -> bool {
        self.failure_point == Some(point)
    }
}

impl Stage5gFinalizedPostApplication {
    pub(crate) fn into_export_parts(self) -> Stage5gFinalizedPostApplicationExportParts {
        Stage5gFinalizedPostApplicationExportParts {
            state: self.state,
            fresh_package_id: self.fresh_package_id,
            fresh_snapshot_epoch: self.fresh_snapshot_epoch,
            captured_at: self.captured_at,
            evidence: self.evidence,
            authority_commitment_sha256: self.authority_commitment_sha256,
            #[cfg(test)]
            failure_point: self.failure_point,
        }
    }
}

impl Stage5gFinalizedPostApplicationExportParts {
    #[cfg(test)]
    pub(crate) fn fails_at(&self, point: Stage5gFreshTruthApplicationFailurePoint) -> bool {
        self.failure_point == Some(point)
    }
}

pub(crate) fn stage5g_application_authority_sha256(
    evidence: &Stage5gFreshTruthApplicationEvidenceV1,
) -> String {
    #[derive(Serialize)]
    struct Authority<'a> {
        domain: &'static str,
        schema_version: u16,
        scenario_id: &'a str,
        disposition: &'a str,
        reason: &'a str,
        operational_identity_commitment_sha256: &'a str,
        command_request_id: &'a str,
        parent_snapshot_id: &'a str,
        parent_snapshot_revision: u64,
        fresh_package_id: &'a str,
        fresh_snapshot_epoch: &'a str,
        fresh_captured_at: DateTime<Utc>,
        fresh_package_fingerprint_sha256: &'a str,
        pre_restart_package_fingerprint_sha256: &'a str,
        reduction_pre_semantic_fingerprint_sha256: &'a str,
        application_source_proof_sha256: &'a str,
        candidate_fingerprint_sha256: &'a str,
        applied_post_semantic_fingerprint_sha256: &'a str,
        restored_post_semantic_fingerprint_sha256: &'a str,
        pre_global_state_fingerprint_sha256: &'a str,
        post_global_state_fingerprint_sha256: &'a str,
        restored_global_state_fingerprint_sha256: &'a str,
        post_restart_package_fingerprint_sha256: &'a str,
        ignored_terminal_order_count: usize,
        ignored_historical_trade_count: usize,
        runtime_transition_applied: bool,
        callback_invoked: bool,
        transport_opened: bool,
        exact_replay_enabled: bool,
        global_state_invariants_proven: bool,
    }
    let bytes = serde_json::to_vec(&Authority {
        domain: "moex.stage5g.edc.application-authority.v1",
        schema_version: evidence.schema_version,
        scenario_id: &evidence.scenario_id,
        disposition: &evidence.disposition,
        reason: &evidence.reason,
        operational_identity_commitment_sha256: &evidence.operational_identity_commitment_sha256,
        command_request_id: &evidence.command_request_id,
        parent_snapshot_id: &evidence.parent_snapshot_id,
        parent_snapshot_revision: evidence.parent_snapshot_revision,
        fresh_package_id: &evidence.fresh_package_id,
        fresh_snapshot_epoch: &evidence.fresh_snapshot_epoch,
        fresh_captured_at: evidence.fresh_captured_at,
        fresh_package_fingerprint_sha256: &evidence.fresh_package_fingerprint_sha256,
        pre_restart_package_fingerprint_sha256: &evidence.pre_restart_package_fingerprint_sha256,
        reduction_pre_semantic_fingerprint_sha256: &evidence
            .reduction_pre_semantic_fingerprint_sha256,
        application_source_proof_sha256: &evidence.application_source_proof_sha256,
        candidate_fingerprint_sha256: &evidence.candidate_fingerprint_sha256,
        applied_post_semantic_fingerprint_sha256: &evidence
            .applied_post_semantic_fingerprint_sha256,
        restored_post_semantic_fingerprint_sha256: &evidence
            .restored_post_semantic_fingerprint_sha256,
        pre_global_state_fingerprint_sha256: &evidence.pre_global_state_fingerprint_sha256,
        post_global_state_fingerprint_sha256: &evidence.post_global_state_fingerprint_sha256,
        restored_global_state_fingerprint_sha256: &evidence
            .restored_global_state_fingerprint_sha256,
        post_restart_package_fingerprint_sha256: &evidence.post_restart_package_fingerprint_sha256,
        ignored_terminal_order_count: evidence.ignored_terminal_order_count,
        ignored_historical_trade_count: evidence.ignored_historical_trade_count,
        runtime_transition_applied: evidence.runtime_transition_applied,
        callback_invoked: evidence.callback_invoked,
        transport_opened: evidence.transport_opened,
        exact_replay_enabled: evidence.exact_replay_enabled,
        global_state_invariants_proven: evidence.global_state_invariants_proven,
    })
    .expect("private application authority serializes");
    format!("{:x}", Sha256::digest(bytes))
}

pub(crate) fn stage5g_application_evidence_matches_state(
    evidence: &Stage5gFreshTruthApplicationEvidenceV1,
    state: &crate::stage5g_order_position::Stage5gOrderPositionState,
) -> bool {
    let classification_matches =
        match stage5g_restart_application_class(state, &evidence.command_request_id) {
            Some(Stage5gRestartApplicationClass::Working) => {
                matches!(
                    evidence.scenario_id.as_str(),
                    "GRST02_RESTART_AFTER_ACK_BEFORE_ORDER" | "GRST03_RESTART_WITH_WORKING_ORDER"
                ) && evidence.reason
                    == reason_id(Stage5gFreshTruthReductionReason::FreshWorkingOrderMatched)
            }
            Some(Stage5gRestartApplicationClass::PartialFill) => {
                evidence.scenario_id == "GRST04_RESTART_AFTER_PARTIAL_FILL"
                    && evidence.reason
                        == reason_id(Stage5gFreshTruthReductionReason::PartialFillPositionConverged)
            }
            Some(Stage5gRestartApplicationClass::Terminal) => {
                evidence.scenario_id == "GRST11_FRESH_BROKER_TRUTH_OVERRIDES_STALE_HINT"
                    && evidence.reason
                        == reason_id(Stage5gFreshTruthReductionReason::FreshTerminalOrderMatched)
            }
            None => false,
        };
    classification_matches
        && super::reducer::stage5g_application_state_semantic_fingerprint(
            state,
            &evidence.command_request_id,
            &evidence.operational_identity_commitment_sha256,
            evidence.ignored_terminal_order_count,
            evidence.ignored_historical_trade_count,
        )
        .is_some_and(|fingerprint| fingerprint == evidence.applied_post_semantic_fingerprint_sha256)
        && stage5g_restart_state_semantic_sha256(state)
            == evidence.post_global_state_fingerprint_sha256
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Stage5gFreshTruthApplicationError {
    CandidateDispositionMismatch,
    UnsupportedRestartLifecycle,
    CandidatePreflightFailed,
    CandidateCanonicalizationFailed,
    CanonicalTransitionFailed,
    ExactReplayDisabled,
    SourceProofMismatch,
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
    BeforeReplayPolicyClassification,
    AfterReplayPolicyClassifiedDisabledBeforeBlockedResult,
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Stage5gFreshTruthApplicationSourceMutation {
    FreshPackageFingerprint,
    PreRestartPackageFingerprint,
    ReductionPreSemanticFingerprint,
    OperationalIdentityCommitment,
    FreshPackageId,
    FreshSnapshotEpoch,
    FreshCapturedAt,
    SwapFreshIdAndEpoch,
    HistoryCounts,
    SourceProofCommitment,
}

#[cfg(test)]
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct Stage5gApplicationTrace {
    pub(crate) candidate_extracted: usize,
    pub(crate) preflight_completed: usize,
    pub(crate) canonical_transition_started: usize,
    pub(crate) canonical_transition_completed: usize,
    pub(crate) post_equality_completed: usize,
    pub(crate) serialization_started: usize,
    pub(crate) serializer_called: usize,
    pub(crate) bytes_produced: usize,
    pub(crate) post_state_consumed: usize,
    pub(crate) decode_started: usize,
    pub(crate) authentication_started: usize,
    pub(crate) authentication_completed: usize,
    pub(crate) restore_started: usize,
    pub(crate) runtime_reconstruction_called: usize,
    pub(crate) restore_completed: usize,
    pub(crate) final_equality_completed: usize,
    pub(crate) replay_policy_classification_started: usize,
    pub(crate) replay_policy_classified_disabled: usize,
    pub(crate) blocked_result_constructed: usize,
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Stage5gApplicationTracePhase {
    CandidateExtracted,
    PreflightCompleted,
    CanonicalTransitionStarted,
    CanonicalTransitionCompleted,
    PostEqualityCompleted,
    SerializationStarted,
    SerializerCalled,
    BytesProduced,
    PostStateConsumed,
    DecodeStarted,
    AuthenticationStarted,
    AuthenticationCompleted,
    RestoreStarted,
    RuntimeReconstructionCalled,
    RestoreCompleted,
    FinalEqualityCompleted,
    ReplayPolicyClassificationStarted,
    ReplayPolicyClassifiedDisabled,
    BlockedResultConstructed,
}

#[cfg(test)]
thread_local! {
    static STAGE5G_APPLICATION_TRACE: std::cell::RefCell<Stage5gApplicationTrace> =
        std::cell::RefCell::new(Stage5gApplicationTrace::default());
}

#[cfg(test)]
pub(crate) fn stage5g_application_trace_reset() {
    STAGE5G_APPLICATION_TRACE
        .with(|trace| *trace.borrow_mut() = Stage5gApplicationTrace::default());
}

#[cfg(test)]
pub(crate) fn stage5g_application_trace_snapshot() -> Stage5gApplicationTrace {
    STAGE5G_APPLICATION_TRACE.with(|trace| trace.borrow().clone())
}

#[cfg(test)]
pub(crate) fn stage5g_application_trace_mark(phase: Stage5gApplicationTracePhase) {
    STAGE5G_APPLICATION_TRACE.with(|trace| {
        let mut trace = trace.borrow_mut();
        match phase {
            Stage5gApplicationTracePhase::CandidateExtracted => trace.candidate_extracted += 1,
            Stage5gApplicationTracePhase::PreflightCompleted => trace.preflight_completed += 1,
            Stage5gApplicationTracePhase::CanonicalTransitionStarted => {
                trace.canonical_transition_started += 1;
            }
            Stage5gApplicationTracePhase::CanonicalTransitionCompleted => {
                trace.canonical_transition_completed += 1;
            }
            Stage5gApplicationTracePhase::PostEqualityCompleted => {
                trace.post_equality_completed += 1;
            }
            Stage5gApplicationTracePhase::SerializationStarted => {
                trace.serialization_started += 1;
            }
            Stage5gApplicationTracePhase::SerializerCalled => trace.serializer_called += 1,
            Stage5gApplicationTracePhase::BytesProduced => trace.bytes_produced += 1,
            Stage5gApplicationTracePhase::PostStateConsumed => trace.post_state_consumed += 1,
            Stage5gApplicationTracePhase::DecodeStarted => trace.decode_started += 1,
            Stage5gApplicationTracePhase::AuthenticationStarted => {
                trace.authentication_started += 1;
            }
            Stage5gApplicationTracePhase::AuthenticationCompleted => {
                trace.authentication_completed += 1;
            }
            Stage5gApplicationTracePhase::RestoreStarted => trace.restore_started += 1,
            Stage5gApplicationTracePhase::RuntimeReconstructionCalled => {
                trace.runtime_reconstruction_called += 1;
            }
            Stage5gApplicationTracePhase::RestoreCompleted => trace.restore_completed += 1,
            Stage5gApplicationTracePhase::FinalEqualityCompleted => {
                trace.final_equality_completed += 1;
            }
            Stage5gApplicationTracePhase::ReplayPolicyClassificationStarted => {
                trace.replay_policy_classification_started += 1;
            }
            Stage5gApplicationTracePhase::ReplayPolicyClassifiedDisabled => {
                trace.replay_policy_classified_disabled += 1;
            }
            Stage5gApplicationTracePhase::BlockedResultConstructed => {
                trace.blocked_result_constructed += 1;
            }
        }
    });
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
    apply_stage5g_fresh_truth_reduction_inner(
        reduction,
        commitment_key,
        None,
        #[cfg(test)]
        None,
        #[cfg(test)]
        None,
    )
}

#[cfg(test)]
pub(crate) fn apply_stage5g_fresh_truth_reduction_with_failure(
    reduction: Stage5gFreshTruthReduction,
    commitment_key: &Stage5gLifecycleCommitmentKey,
    failure_point: Stage5gFreshTruthApplicationFailurePoint,
) -> Stage5gFreshTruthApplicationResult {
    stage5g_application_trace_reset();
    apply_stage5g_fresh_truth_reduction_inner(
        reduction,
        commitment_key,
        Some(failure_point),
        None,
        None,
    )
}

#[cfg(test)]
pub(crate) fn apply_stage5g_fresh_truth_reduction_with_mismatch(
    reduction: Stage5gFreshTruthReduction,
    commitment_key: &Stage5gLifecycleCommitmentKey,
    mismatch: Stage5gRestartApplicationMismatch,
) -> Stage5gFreshTruthApplicationResult {
    stage5g_application_trace_reset();
    apply_stage5g_fresh_truth_reduction_inner(reduction, commitment_key, None, None, Some(mismatch))
}

#[cfg(test)]
pub(crate) fn apply_stage5g_fresh_truth_reduction_with_source_mutation(
    reduction: Stage5gFreshTruthReduction,
    commitment_key: &Stage5gLifecycleCommitmentKey,
    mutation: Stage5gFreshTruthApplicationSourceMutation,
) -> Stage5gFreshTruthApplicationResult {
    stage5g_application_trace_reset();
    apply_stage5g_fresh_truth_reduction_inner(reduction, commitment_key, None, Some(mutation), None)
}

fn apply_stage5g_fresh_truth_reduction_inner(
    reduction: Stage5gFreshTruthReduction,
    commitment_key: &Stage5gLifecycleCommitmentKey,
    #[cfg(test)] failure_point: Option<Stage5gFreshTruthApplicationFailurePoint>,
    #[cfg(not(test))] _failure_point: Option<()>,
    #[cfg(test)] source_mutation: Option<Stage5gFreshTruthApplicationSourceMutation>,
    #[cfg(test)] mismatch: Option<Stage5gRestartApplicationMismatch>,
) -> Stage5gFreshTruthApplicationResult {
    let parts = reduction.into_application_parts();
    #[cfg(test)]
    if parts.scenario_id == Stage5gRestartScenarioId::Grst09ExactReplayIsIdempotent
        && parts.reason == Stage5gFreshTruthReductionReason::ReplayTupleNotInRestartLedger
    {
        return policy_b_exact_replay_disabled(parts, failure_point);
    }
    #[cfg(not(test))]
    if parts.scenario_id.frozen_id() == "GRST09_EXACT_REPLAY_IS_IDEMPOTENT"
        && parts.reason == Stage5gFreshTruthReductionReason::ReplayTupleNotInRestartLedger
    {
        return policy_b_exact_replay_disabled(parts);
    }
    match parts.disposition {
        Stage5gRestartReconciliationDisposition::ApplyOwnedCandidate => apply_owned_candidate(
            parts,
            commitment_key,
            #[cfg(test)]
            failure_point,
            #[cfg(test)]
            source_mutation,
            #[cfg(test)]
            mismatch,
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
        Stage5gRestartReconciliationDisposition::ExactReplay => policy_b_exact_replay_disabled(
            parts,
            #[cfg(test)]
            failure_point,
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

fn policy_b_exact_replay_disabled(
    parts: Stage5gFreshTruthApplicationParts,
    #[cfg(test)] failure_point: Option<Stage5gFreshTruthApplicationFailurePoint>,
) -> Stage5gFreshTruthApplicationResult {
    #[cfg(test)]
    if parts.scenario_id == Stage5gRestartScenarioId::Grst09ExactReplayIsIdempotent
        && failure_point
            == Some(Stage5gFreshTruthApplicationFailurePoint::BeforeReplayPolicyClassification)
    {
        return blocked(
            parts,
            Some(Stage5gFreshTruthApplicationError::InjectedFailure),
        );
    }
    #[cfg(test)]
    stage5g_application_trace_mark(Stage5gApplicationTracePhase::ReplayPolicyClassificationStarted);
    let exact_replay_is_disabled = true;
    #[cfg(test)]
    stage5g_application_trace_mark(Stage5gApplicationTracePhase::ReplayPolicyClassifiedDisabled);
    #[cfg(test)]
    if parts.scenario_id == Stage5gRestartScenarioId::Grst09ExactReplayIsIdempotent
        && failure_point
            == Some(
                Stage5gFreshTruthApplicationFailurePoint::AfterReplayPolicyClassifiedDisabledBeforeBlockedResult,
            )
    {
        return blocked(
            parts,
            Some(Stage5gFreshTruthApplicationError::InjectedFailure),
        );
    }
    debug_assert!(exact_replay_is_disabled);
    #[cfg(test)]
    stage5g_application_trace_mark(Stage5gApplicationTracePhase::BlockedResultConstructed);
    blocked(
        parts,
        Some(Stage5gFreshTruthApplicationError::ExactReplayDisabled),
    )
}

fn apply_owned_candidate(
    mut parts: Stage5gFreshTruthApplicationParts,
    commitment_key: &Stage5gLifecycleCommitmentKey,
    #[cfg(test)] failure_point: Option<Stage5gFreshTruthApplicationFailurePoint>,
    #[cfg(test)] source_mutation: Option<Stage5gFreshTruthApplicationSourceMutation>,
    #[cfg(test)] mismatch: Option<Stage5gRestartApplicationMismatch>,
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
    stage5g_application_trace_mark(Stage5gApplicationTracePhase::CandidateExtracted);
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
    stage5g_application_trace_mark(Stage5gApplicationTracePhase::PreflightCompleted);
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
    let expected_global_transition = stage5g_restart_application_expectation(&canonical_evidence);
    let pre_state_for_proof = pre_state.clone();
    let pre_global_state_fingerprint = stage5g_restart_state_semantic_sha256(&pre_state_for_proof);
    #[cfg(test)]
    stage5g_application_trace_mark(Stage5gApplicationTracePhase::CanonicalTransitionStarted);
    #[cfg(test)]
    if failure_point == Some(Stage5gFreshTruthApplicationFailurePoint::InsideCanonicalTransition) {
        stage5g_test_fail_restart_canonical_before_commit();
    }
    let transition =
        apply_stage5g_restart_canonical_order_position_state(pre_state, canonical_evidence);
    let post_state = match transition {
        Ok(state) => state,
        Err(Stage5gRestartCanonicalApplicationError::ExactReplayDisabled) => {
            return blocked_with_candidate_dropped(
                parts,
                candidate,
                Stage5gFreshTruthApplicationError::ExactReplayDisabled,
            );
        }
        Err(Stage5gRestartCanonicalApplicationError::OrderPosition(_)) => {
            #[cfg(test)]
            if failure_point
                == Some(Stage5gFreshTruthApplicationFailurePoint::InsideCanonicalTransition)
            {
                return blocked_with_candidate_dropped(
                    parts,
                    candidate,
                    Stage5gFreshTruthApplicationError::InjectedFailure,
                );
            }
            return blocked_with_candidate_dropped(
                parts,
                candidate,
                Stage5gFreshTruthApplicationError::CanonicalTransitionFailed,
            );
        }
    };
    #[cfg(test)]
    let mut post_state = post_state;
    #[cfg(test)]
    stage5g_application_trace_mark(Stage5gApplicationTracePhase::CanonicalTransitionCompleted);
    #[cfg(test)]
    if let Some(mismatch) = mismatch {
        stage5g_test_mutate_restart_application_state(&mut post_state, mismatch);
    }
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
    let candidate_fingerprint = candidate.application_semantic_fingerprint(
        parts.ignored_unrelated_terminal_order_count,
        parts.ignored_unrelated_historical_trade_count,
    );
    let Some(post_state_fingerprint) = candidate.post_state_semantic_fingerprint(
        &post_state,
        parts.ignored_unrelated_terminal_order_count,
        parts.ignored_unrelated_historical_trade_count,
    ) else {
        return blocked_with_candidate_dropped(
            parts,
            candidate,
            Stage5gFreshTruthApplicationError::PostStateSemanticMismatch,
        );
    };
    let global_state_invariants_proven = stage5g_restart_application_global_invariants(
        &pre_state_for_proof,
        &post_state,
        &expected_global_transition,
    );
    if candidate_fingerprint != post_state_fingerprint || !global_state_invariants_proven {
        return blocked_with_candidate_dropped(
            parts,
            candidate,
            Stage5gFreshTruthApplicationError::PostStateSemanticMismatch,
        );
    }
    #[cfg(test)]
    stage5g_application_trace_mark(Stage5gApplicationTracePhase::PostEqualityCompleted);
    let post_global_state_fingerprint = stage5g_restart_state_semantic_sha256(&post_state);
    #[cfg(test)]
    if failure_point == Some(Stage5gFreshTruthApplicationFailurePoint::AfterEqualityBeforeExport) {
        return blocked_with_candidate_dropped(
            parts,
            candidate,
            Stage5gFreshTruthApplicationError::InjectedFailure,
        );
    }

    let source_proof =
        Stage5gFreshTruthApplicationSourceProof::from_application_parts(&parts, &candidate);
    let evidence = Stage5gFreshTruthApplicationEvidenceV1 {
        schema_version: STAGE5G_FRESH_TRUTH_APPLICATION_EVIDENCE_SCHEMA_VERSION,
        scenario_id: source_proof.scenario_id.clone(),
        disposition: source_proof.disposition.clone(),
        reason: source_proof.reason.clone(),
        operational_identity_commitment_sha256: source_proof
            .operational_identity_commitment_sha256
            .clone(),
        command_request_id: source_proof.command_request_id.clone(),
        parent_snapshot_id: source_proof.parent_snapshot_id.clone(),
        parent_snapshot_revision: source_proof.parent_snapshot_revision,
        fresh_package_id: source_proof.fresh_package_id.clone(),
        fresh_snapshot_epoch: source_proof.fresh_snapshot_epoch.clone(),
        fresh_captured_at: source_proof.fresh_captured_at,
        fresh_package_fingerprint_sha256: source_proof.fresh_package_fingerprint_sha256.clone(),
        pre_restart_package_fingerprint_sha256: source_proof
            .pre_restart_package_fingerprint_sha256
            .clone(),
        reduction_pre_semantic_fingerprint_sha256: source_proof
            .reduction_pre_semantic_fingerprint_sha256
            .clone(),
        application_source_proof_sha256: source_proof.commitment_sha256(),
        candidate_fingerprint_sha256: candidate_fingerprint.clone(),
        applied_post_semantic_fingerprint_sha256: post_state_fingerprint.clone(),
        restored_post_semantic_fingerprint_sha256: post_state_fingerprint,
        pre_global_state_fingerprint_sha256: pre_global_state_fingerprint,
        post_global_state_fingerprint_sha256: post_global_state_fingerprint.clone(),
        restored_global_state_fingerprint_sha256: post_global_state_fingerprint,
        post_restart_package_fingerprint_sha256: String::new(),
        ignored_terminal_order_count: parts.ignored_unrelated_terminal_order_count,
        ignored_historical_trade_count: parts.ignored_unrelated_historical_trade_count,
        runtime_transition_applied: true,
        callback_invoked: false,
        transport_opened: false,
        exact_replay_enabled: false,
        global_state_invariants_proven,
    };
    #[cfg(test)]
    let evidence = {
        let mut evidence = evidence;
        if let Some(mutation) = source_mutation {
            stage5g_test_mutate_application_source_evidence(&mut evidence, mutation);
        }
        evidence
    };
    let fresh_runtime = parts.restart.stage5g_fresh_reconstruction_candidate();
    let validated_post_application = match Stage5gValidatedPostApplication::new(
        post_state,
        source_proof,
        evidence,
        #[cfg(test)]
        failure_point,
    ) {
        Ok(value) => value,
        Err(_) => {
            return blocked_with_candidate_dropped(
                parts,
                candidate,
                Stage5gFreshTruthApplicationError::SourceProofMismatch,
            );
        }
    };
    let (package_bytes, evidence) = match parts
        .restart
        .stage5g_export_post_application_order_position(validated_post_application, commitment_key)
    {
        Ok(exported) => exported,
        Err(_) => {
            #[cfg(test)]
            if failure_point == Some(Stage5gFreshTruthApplicationFailurePoint::DuringSerialization)
            {
                return blocked_with_candidate_dropped(
                    parts,
                    candidate,
                    Stage5gFreshTruthApplicationError::InjectedFailure,
                );
            }
            #[cfg(test)]
            if failure_point
                == Some(Stage5gFreshTruthApplicationFailurePoint::AfterBytesBeforeSourceDrop)
            {
                return blocked_with_candidate_dropped(
                    parts,
                    candidate,
                    Stage5gFreshTruthApplicationError::InjectedFailure,
                );
            }
            #[cfg(test)]
            if failure_point
                == Some(Stage5gFreshTruthApplicationFailurePoint::AfterSourceDropBeforeDecode)
            {
                return blocked_with_candidate_dropped(
                    parts,
                    candidate,
                    Stage5gFreshTruthApplicationError::InjectedFailure,
                );
            }
            return blocked_with_candidate_dropped(
                parts,
                candidate,
                Stage5gFreshTruthApplicationError::PostPackageFailed,
            );
        }
    };
    // The source post-state was consumed by export. The pre-application
    // restart remains the rollback authority until fresh decode/verify/restore
    // has succeeded; it is destroyed only at final commit below.
    #[cfg(test)]
    let restored_result = crate::stage5g_clean_restart::restore_stage5g_clean_restart_with_failure(
        &package_bytes,
        commitment_key,
        fresh_runtime,
        failure_point,
    );
    #[cfg(not(test))]
    let restored_result =
        restore_stage5g_clean_restart(&package_bytes, commitment_key, fresh_runtime);
    let restored = match restored_result {
        Ok(restored) => restored,
        Err(_) => {
            #[cfg(test)]
            if failure_point
                == Some(Stage5gFreshTruthApplicationFailurePoint::DuringAuthenticationVerification)
            {
                return blocked_with_candidate_dropped(
                    parts,
                    candidate,
                    Stage5gFreshTruthApplicationError::InjectedFailure,
                );
            }
            #[cfg(test)]
            if failure_point == Some(Stage5gFreshTruthApplicationFailurePoint::DuringRestore) {
                return blocked_with_candidate_dropped(
                    parts,
                    candidate,
                    Stage5gFreshTruthApplicationError::InjectedFailure,
                );
            }
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
            .is_some_and(|state| {
                candidate
                    .restored_state_semantic_fingerprint(
                        &state,
                        parts.ignored_unrelated_terminal_order_count,
                        parts.ignored_unrelated_historical_trade_count,
                    )
                    .is_some_and(|fingerprint| {
                        fingerprint == evidence.restored_post_semantic_fingerprint_sha256()
                    })
                    && stage5g_restart_state_semantic_sha256(&state)
                        == evidence.restored_global_state_fingerprint_sha256()
                    && evidence.post_global_state_fingerprint_sha256()
                        == evidence.restored_global_state_fingerprint_sha256()
            });
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
    #[cfg(test)]
    stage5g_application_trace_mark(Stage5gApplicationTracePhase::FinalEqualityCompleted);
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
