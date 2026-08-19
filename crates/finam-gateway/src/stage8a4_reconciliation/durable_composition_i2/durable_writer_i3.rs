//! Stage 8A-4 I3 sealed durable writer authority.
//!
//! The public type is opaque and linear. Its only issuer consumes the private
//! I2 candidate plus exact current truth/control and current Stage 6/7 facts.
//! Public V2/read DTOs have no conversion into this authority.

use super::super::{
    account_safety_binding, account_safety_summary, canonical_truth_binding,
    reduce_stage8a4_authoritative, Stage8a4DurableRequestContext, Stage8a4FreshTruthAdmission,
    Stage8a4ReconciliationPolicy,
};
use super::{
    accepted_command_authority_from_request_accepted, build_private_durable_candidate,
    parse_digest, PrivateAccountSafetySummary, PrivateJournalCursor, PrivatePreAppendEvidence,
    Stage8a4I2CompositionInput, Stage8a4I2DurableCandidate,
};
use crate::stage8a1_execution_capability::{
    Stage8ExecutionCapability, Stage8a1OperationalAuthorityIssuer,
    Stage8a4PostEffectControlEvidence, Stage8a4PostEffectControlState,
};
use chrono::{DateTime, Utc};
use runtime_durable_service::{
    Stage7bRecoveryError, Stage7bRecoveryReadyOwner, Stage7bStage8a4DurableBatchReceipt,
};
use strategy_runtime_core::{
    Stage5gLifecycleCommitmentKey, Stage6DurableRequestIdentityV1, Stage6LifecycleSequence,
    Stage6Sha256Digest, Stage6Stage8a4DurableBatch, Stage6Stage8a4PendingRecovery,
    Stage6Stage8a4SealedWriteAuthority,
};

#[derive(Debug)]
enum Stage8a4I3Error {
    CommandAuthorityMismatch,
    CurrentTruthMismatch,
    BatchInvalid(strategy_runtime_core::Stage6dLiveCoreError),
}

#[derive(Debug, thiserror::Error)]
pub enum Stage8a4DurableCompositionError {
    #[error("current Stage 8A-4 authority is invalid")]
    CurrentAuthority,
    #[error("current Stage 8A-4 broker truth is invalid")]
    CurrentTruth,
    #[error("Stage 8A-4 durable recovery failed")]
    DurableRecovery,
    #[error("Stage 8A-4 private durable composition failed")]
    PrivateComposition,
}

/// Full production no-send composition for a new Stage 8A-4 transition.
///
/// Only opaque reconciliation authorities and the exact broker-neutral
/// request/command enter this operation. It obtains current S0 from Stage7B,
/// constructs the private I2 candidate internally, revalidates current
/// truth/control, seals the batch and returns only after covering S1 is reread.
#[allow(clippy::too_many_arguments)]
pub fn reconcile_persist_and_cover_stage8a4(
    capability: &Stage8ExecutionCapability,
    issuer: &Stage8a1OperationalAuthorityIssuer,
    owner: &mut Stage7bRecoveryReadyOwner,
    commitment_key: &Stage5gLifecycleCommitmentKey,
    identity: &Stage6DurableRequestIdentityV1,
    command: &strategy_runtime_core::Stage6DurableCommandSnapshotV1,
    context: Stage8a4DurableRequestContext,
    reconciliation_truth: Stage8a4FreshTruthAdmission,
    writer_entry_truth: Stage8a4FreshTruthAdmission,
    policy: Stage8a4ReconciliationPolicy,
) -> Result<Stage7bStage8a4DurableBatchReceipt, Stage8a4DurableCompositionError> {
    let current = owner
        .authorize_stage8a1_durable_request(commitment_key, identity, command)
        .map_err(|_| Stage8a4DurableCompositionError::CurrentAuthority)?;
    let stage6 = current.stage6();
    let accepted_command_authority =
        accepted_command_authority_from_request_accepted(stage6.accepted_record())
            .map_err(|_| Stage8a4DurableCompositionError::PrivateComposition)?;
    let previous_lifecycle_sequence = Stage6LifecycleSequence::new(stage6.dispatch_sequence())
        .map_err(|_| Stage8a4DurableCompositionError::PrivateComposition)?;
    let cursor = PrivateJournalCursor {
        previous_record_id: stage6.dispatch_record_id().clone(),
        previous_lifecycle_sequence,
    };
    let pre_append = PrivatePreAppendEvidence {
        expected_stage6_checkpoint_or_frontier_fingerprint: Stage6Sha256Digest::parse(
            stage6.durable_frontier_sha256().to_string(),
        )
        .map_err(|_| Stage8a4DurableCompositionError::PrivateComposition)?,
        expected_recovery_seal_generation: current.seal_generation(),
        expected_recovery_seal_fingerprint: Stage6Sha256Digest::parse(
            current.seal_commitment_sha256().to_string(),
        )
        .map_err(|_| Stage8a4DurableCompositionError::PrivateComposition)?,
        expected_request_state_fingerprint: stage6.request_state_fingerprint_sha256().clone(),
    };
    let outcome = reduce_stage8a4_authoritative(context, reconciliation_truth, policy);
    let candidate = build_private_durable_candidate(Stage8a4I2CompositionInput {
        accepted_command_authority,
        cursor,
        pre_append,
        outcome,
    })
    .map_err(|_| Stage8a4DurableCompositionError::PrivateComposition)?;
    let historical = capability
        .stage8a4_historical_arm_provenance()
        .map_err(|_| Stage8a4DurableCompositionError::CurrentAuthority)?;
    let controls = issuer
        .issue_stage8a4_post_effect_control_evidence(historical)
        .map_err(|_| Stage8a4DurableCompositionError::CurrentAuthority)?;
    let authority = issue_private_durable_write_authority(
        candidate,
        writer_entry_truth,
        controls,
        owner,
        commitment_key,
    )
    .map_err(|error| match error {
        Stage8a4I3Error::CurrentTruthMismatch => Stage8a4DurableCompositionError::CurrentTruth,
        Stage8a4I3Error::CommandAuthorityMismatch => {
            Stage8a4DurableCompositionError::CurrentAuthority
        }
        Stage8a4I3Error::BatchInvalid(_) => Stage8a4DurableCompositionError::PrivateComposition,
    })?;
    authority
        .persist_and_cover(owner, commitment_key)
        .map_err(|_| Stage8a4DurableCompositionError::DurableRecovery)
}

/// Sole linear I3 authority accepted by the Stage 7B writer. It cannot be
/// constructed from public canonical bytes, diagnostics or read DTOs.
pub struct Stage8a4DurableWriteAuthority {
    sealed: Stage6Stage8a4SealedWriteAuthority,
}

impl Stage8a4DurableWriteAuthority {
    /// Production durable composition entry. The broker-specific authority is
    /// consumed here and only its authenticated broker-neutral capability
    /// crosses into the Stage 7B owner.
    pub fn persist_and_cover(
        self,
        owner: &mut Stage7bRecoveryReadyOwner,
        commitment_key: &Stage5gLifecycleCommitmentKey,
    ) -> Result<Stage7bStage8a4DurableBatchReceipt, Stage7bRecoveryError> {
        owner.append_stage8a4_sealed_authority_and_cover(commitment_key, self.sealed)
    }
}

/// Production restart entry for a V2 transition whose exact suffix was only
/// partly persisted. It re-derives authority from current Stage7B S0, current
/// broker truth and current post-effect controls; it never needs the lost I2
/// candidate and never appends a second V2.
pub fn recover_persisted_stage8a4_suffix_and_cover(
    capability: &Stage8ExecutionCapability,
    issuer: &Stage8a1OperationalAuthorityIssuer,
    owner: &mut Stage7bRecoveryReadyOwner,
    commitment_key: &Stage5gLifecycleCommitmentKey,
    current_truth: Stage8a4FreshTruthAdmission,
) -> Result<Option<Stage7bStage8a4DurableBatchReceipt>, Stage8a4DurableCompositionError> {
    let Some(pending) = owner
        .stage8a4_pending_recovery_material(commitment_key)
        .map_err(|_| Stage8a4DurableCompositionError::DurableRecovery)?
    else {
        return Ok(None);
    };
    let historical = capability
        .stage8a4_historical_arm_provenance()
        .map_err(|_| Stage8a4DurableCompositionError::CurrentAuthority)?;
    let controls = issuer
        .issue_stage8a4_post_effect_control_evidence(historical)
        .map_err(|_| Stage8a4DurableCompositionError::CurrentAuthority)?;
    let authority = issue_persisted_recovery_write_authority(
        pending,
        current_truth,
        controls,
        owner,
        commitment_key,
    )
    .map_err(|error| match error {
        Stage8a4I3Error::CurrentTruthMismatch => Stage8a4DurableCompositionError::CurrentTruth,
        Stage8a4I3Error::CommandAuthorityMismatch => {
            Stage8a4DurableCompositionError::CurrentAuthority
        }
        Stage8a4I3Error::BatchInvalid(_) => Stage8a4DurableCompositionError::DurableRecovery,
    })?;
    authority
        .persist_and_cover(owner, commitment_key)
        .map(Some)
        .map_err(|_| Stage8a4DurableCompositionError::DurableRecovery)
}

struct Stage8a4WriterEntryFreshTruth {
    safety: PrivateAccountSafetySummary,
    _source_evidence_binding_sha256: Stage6Sha256Digest,
    _truth_binding_sha256: Stage6Sha256Digest,
    _validated_at: DateTime<Utc>,
}

fn issue_private_durable_write_authority(
    candidate: Stage8a4I2DurableCandidate,
    current_truth: Stage8a4FreshTruthAdmission,
    controls: Stage8a4PostEffectControlEvidence,
    owner: &mut Stage7bRecoveryReadyOwner,
    commitment_key: &Stage5gLifecycleCommitmentKey,
) -> Result<Stage8a4DurableWriteAuthority, Stage8a4I3Error> {
    let identity = candidate
        .transition_record
        .durable_request_identity()
        .clone();
    let current_stage7 = owner
        .authorize_stage8a1_durable_request(commitment_key, &identity, &candidate.durable_command)
        .map_err(|_| Stage8a4I3Error::CommandAuthorityMismatch)?;
    let current_stage6 = current_stage7.stage6();
    let current_operational_identity_sha256 = current_stage7.operational_identity_sha256();
    let current_seal_generation = current_stage7.seal_generation();
    let current_seal_commitment_sha256 = current_stage7.seal_commitment_sha256();
    if current_stage6.identity() != &identity
        || current_stage6.canonical_command_sha256() != &candidate.accepted_command_payload_sha256
        || controls.accepted_command_payload_sha256() != &candidate.accepted_command_payload_sha256
        || controls.operational_identity_sha256() != current_operational_identity_sha256
        || controls.runtime_config_fingerprint_sha256()
            != current_stage6.runtime_config_fingerprint_sha256()
        || current_seal_generation == 0
        || parse_digest(current_operational_identity_sha256).is_err()
        || parse_digest(current_seal_commitment_sha256).is_err()
        || parse_digest(controls.authority_scope_sha256()).is_err()
        || parse_digest(controls.arm_registration_sha256()).is_err()
        || parse_digest(controls.current_control_binding_sha256().as_str()).is_err()
    {
        return Err(Stage8a4I3Error::CommandAuthorityMismatch);
    }
    match controls.current_control_state() {
        Stage8a4PostEffectControlState::RunAllowed
        | Stage8a4PostEffectControlState::StopRequested
        | Stage8a4PostEffectControlState::StaleOrUnreadable => {}
    }

    let writer_truth = issue_writer_entry_fresh_truth(
        current_truth,
        &identity,
        candidate
            .transition_record
            .payload()
            .durable_request_binding_sha256(),
        Utc::now(),
    )?;
    let current = serde_json::to_vec(&writer_truth.safety)
        .map_err(|_| Stage8a4I3Error::CurrentTruthMismatch)?;
    let persisted = serde_json::to_vec(
        candidate
            .transition_record
            .payload()
            .account_safety_summary(),
    )
    .map_err(|_| Stage8a4I3Error::CurrentTruthMismatch)?;
    if current != persisted {
        return Err(Stage8a4I3Error::CurrentTruthMismatch);
    }

    let batch = Stage6Stage8a4DurableBatch::new(
        candidate.transition_record,
        candidate.suffix_records,
        candidate.cancel_original_target_shape,
    )
    .map_err(Stage8a4I3Error::BatchInvalid)?;
    let sealed = Stage6Stage8a4SealedWriteAuthority::seal(
        commitment_key,
        identity,
        candidate.durable_command,
        batch,
        current_operational_identity_sha256.to_string(),
        current_stage6
            .runtime_config_fingerprint_sha256()
            .to_string(),
        current_seal_generation,
        current_seal_commitment_sha256.to_string(),
    )
    .map_err(Stage8a4I3Error::BatchInvalid)?;
    Ok(Stage8a4DurableWriteAuthority { sealed })
}

fn issue_persisted_recovery_write_authority(
    pending: Stage6Stage8a4PendingRecovery,
    current_truth: Stage8a4FreshTruthAdmission,
    controls: Stage8a4PostEffectControlEvidence,
    owner: &mut Stage7bRecoveryReadyOwner,
    commitment_key: &Stage5gLifecycleCommitmentKey,
) -> Result<Stage8a4DurableWriteAuthority, Stage8a4I3Error> {
    let (transition, command) = pending.into_parts();
    let identity = transition.durable_request_identity().clone();
    let current_stage7 = owner
        .authorize_stage8a4_pending_recovery_request(commitment_key, &identity, &command)
        .map_err(|_| Stage8a4I3Error::CommandAuthorityMismatch)?;
    let current_stage6 = current_stage7.stage6();
    if controls.accepted_command_payload_sha256() != current_stage6.canonical_command_sha256()
        || controls.operational_identity_sha256() != current_stage7.operational_identity_sha256()
        || controls.runtime_config_fingerprint_sha256()
            != current_stage6.runtime_config_fingerprint_sha256()
        || parse_digest(controls.authority_scope_sha256()).is_err()
        || parse_digest(controls.arm_registration_sha256()).is_err()
        || parse_digest(controls.current_control_binding_sha256().as_str()).is_err()
    {
        return Err(Stage8a4I3Error::CommandAuthorityMismatch);
    }
    match controls.current_control_state() {
        Stage8a4PostEffectControlState::RunAllowed
        | Stage8a4PostEffectControlState::StopRequested
        | Stage8a4PostEffectControlState::StaleOrUnreadable => {}
    }
    let writer_truth = issue_writer_entry_fresh_truth(
        current_truth,
        &identity,
        transition.payload().durable_request_binding_sha256(),
        Utc::now(),
    )?;
    let current = serde_json::to_vec(&writer_truth.safety)
        .map_err(|_| Stage8a4I3Error::CurrentTruthMismatch)?;
    let persisted = serde_json::to_vec(transition.payload().account_safety_summary())
        .map_err(|_| Stage8a4I3Error::CurrentTruthMismatch)?;
    if current != persisted {
        return Err(Stage8a4I3Error::CurrentTruthMismatch);
    }
    let batch = Stage6Stage8a4DurableBatch::recover_from_persisted_transition(transition)
        .map_err(Stage8a4I3Error::BatchInvalid)?;
    let sealed = Stage6Stage8a4SealedWriteAuthority::seal(
        commitment_key,
        identity,
        command,
        batch,
        current_stage7.operational_identity_sha256().to_string(),
        current_stage6
            .runtime_config_fingerprint_sha256()
            .to_string(),
        current_stage7.seal_generation(),
        current_stage7.seal_commitment_sha256().to_string(),
    )
    .map_err(Stage8a4I3Error::BatchInvalid)?;
    Ok(Stage8a4DurableWriteAuthority { sealed })
}

fn issue_writer_entry_fresh_truth(
    admission: Stage8a4FreshTruthAdmission,
    identity: &Stage6DurableRequestIdentityV1,
    durable_binding: &Stage6Sha256Digest,
    now: DateTime<Utc>,
) -> Result<Stage8a4WriterEntryFreshTruth, Stage8a4I3Error> {
    if admission.admitted_request_id != identity.strategy_request_id()
        || &admission.admitted_account_id != identity.account_id()
        || &admission.admitted_instrument != identity.instrument()
        || admission.truth.account_id != *identity.account_id()
        || admission.admitted_durable_binding_sha256 != durable_binding.as_str()
        || admission.admitted_canonical_truth_sha256 != canonical_truth_binding(&admission.truth)
        || admission.writer_entry_valid_until <= now
    {
        return Err(Stage8a4I3Error::CurrentTruthMismatch);
    }
    let source_evidence_binding_sha256 = parse_digest(&admission.source_evidence_binding_sha256)
        .map_err(|_| Stage8a4I3Error::CurrentTruthMismatch)?;
    let truth_binding_sha256 = parse_digest(&admission.truth_binding_sha256)
        .map_err(|_| Stage8a4I3Error::CurrentTruthMismatch)?;
    let current = account_safety_summary(&admission.truth, identity.instrument());
    let current_binding = parse_digest(&account_safety_binding(&current))
        .map_err(|_| Stage8a4I3Error::CurrentTruthMismatch)?;
    let safety = PrivateAccountSafetySummary {
        account_active_orders_count: to_u32(current.account_active_orders_count)?,
        account_unknown_orders_count: to_u32(current.account_unknown_orders_count)?,
        account_orphan_orders_count: to_u32(current.account_orphan_orders_count)?,
        account_open_positions_count: to_u32(current.account_open_positions_count)?,
        target_active_orders_count: to_u32(current.target_active_orders_count)?,
        target_unknown_orders_count: to_u32(current.target_unknown_orders_count)?,
        target_terminal_orders_count: to_u32(current.target_terminal_orders_count)?,
        target_inconsistent_orders_count: to_u32(current.target_inconsistent_orders_count)?,
        target_open_positions_count: to_u32(current.target_open_positions_count)?,
        other_symbol_active_orders_count: to_u32(current.other_symbol_active_orders_count)?,
        account_safety_binding_sha256: current_binding,
    };
    Ok(Stage8a4WriterEntryFreshTruth {
        safety,
        _source_evidence_binding_sha256: source_evidence_binding_sha256,
        _truth_binding_sha256: truth_binding_sha256,
        _validated_at: now,
    })
}

fn to_u32(value: usize) -> Result<u32, Stage8a4I3Error> {
    value
        .try_into()
        .map_err(|_| Stage8a4I3Error::CurrentTruthMismatch)
}

#[cfg(test)]
pub(super) fn test_writer_entry_safety(
    admission: Stage8a4FreshTruthAdmission,
    identity: &Stage6DurableRequestIdentityV1,
    durable_binding: &Stage6Sha256Digest,
    now: DateTime<Utc>,
) -> Option<Vec<u8>> {
    issue_writer_entry_fresh_truth(admission, identity, durable_binding, now)
        .ok()
        .and_then(|truth| serde_json::to_vec(&truth.safety).ok())
}
