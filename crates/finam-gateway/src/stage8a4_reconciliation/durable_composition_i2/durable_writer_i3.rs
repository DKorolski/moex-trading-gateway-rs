//! Stage 8A-4 I3 sealed durable writer authority.
//!
//! The public type is opaque and linear. Its only issuer consumes the private
//! I2 candidate plus exact current truth/control and current Stage 6/7 facts.
//! Public V2/read DTOs have no conversion into this authority.

use super::super::{
    account_safety_binding, account_safety_summary, canonical_truth_binding,
    issue_stage8a4_policy_from_frozen_config,
    issue_stage8a4_source_evidence_from_readonly_acquisition, reduce_stage8a4_authoritative,
    Stage8a4DurableRequestContext, Stage8a4FreshTruthAdmission, Stage8a4ReadonlySourceAcquisition,
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
    Stage8a4I3RecoveryPendingOwner,
};
use std::path::Path;
use strategy_runtime_core::{
    stage8a4_writer_entry_attestation_sha256, Stage5gLifecycleCommitmentKey,
    Stage6DurableCommandSnapshotV1, Stage6DurablePlaceOrderShapeV1, Stage6DurableRequestIdentityV1,
    Stage6LifecycleSequence, Stage6Sha256Digest, Stage6Stage8a4DurableBatch,
    Stage6Stage8a4PendingRecovery, Stage6Stage8a4ValidatedWriteEntry,
};

pub(crate) struct Stage8a4ProductionEventWindow {
    possible_effect_at: DateTime<Utc>,
    event_start: DateTime<Utc>,
    event_end: DateTime<Utc>,
}

impl Stage8a4ProductionEventWindow {
    pub(crate) fn from_dispatch_and_acquisition(
        possible_effect_at: DateTime<Utc>,
        event_start: DateTime<Utc>,
        event_end: DateTime<Utc>,
    ) -> Result<Self, Stage8a4DurableCompositionError> {
        if event_start < possible_effect_at || event_end <= event_start {
            return Err(Stage8a4DurableCompositionError::CurrentAuthority);
        }
        Ok(Self {
            possible_effect_at,
            event_start,
            event_end,
        })
    }
}

fn issue_durable_request_context_from_current_authority(
    authority: &runtime_durable_service::Stage7bStage8a1DurableRequestAuthority,
    command: &Stage6DurableCommandSnapshotV1,
    event_window: Stage8a4ProductionEventWindow,
    known_broker_order_id: Option<broker_core::BrokerOrderId>,
    cancel_original_shape: Option<Stage6DurablePlaceOrderShapeV1>,
) -> Result<Stage8a4DurableRequestContext, Stage8a4DurableCompositionError> {
    let stage6 = authority.stage6();
    if command.action() != stage6.identity().action()
        || stage6.canonical_command_sha256() != stage6.accepted_record().canonical_payload_sha256()
    {
        return Err(Stage8a4DurableCompositionError::CurrentAuthority);
    }
    let shape = command
        .place_order_shape()
        .or(cancel_original_shape)
        .ok_or(Stage8a4DurableCompositionError::CurrentAuthority)?;
    Ok(Stage8a4DurableRequestContext {
        request_id: stage6.identity().strategy_request_id(),
        client_order_id: stage6.identity().durable_client_order_id().clone(),
        account_id: stage6.identity().account_id().clone(),
        instrument: stage6.identity().instrument().clone(),
        action: stage6.identity().action(),
        attribution: stage6.identity().attribution().clone(),
        side: shape.side(),
        qty: shape.quantity(),
        order_type: shape.order_type(),
        time_in_force: shape.time_in_force(),
        limit_price: shape.limit_price(),
        known_broker_order_id,
        target_order_client_order_id: stage6.identity().target_order_client_order_id().cloned(),
        accepted_command_payload_sha256: stage6
            .accepted_record()
            .canonical_payload_sha256()
            .as_str()
            .to_string(),
        possible_effect_at: event_window.possible_effect_at,
        event_start: event_window.event_start,
        event_end: event_window.event_end,
        durable_binding_sha256: stage6
            .durable_request_binding_sha256()
            .map_err(|_| Stage8a4DurableCompositionError::CurrentAuthority)?
            .as_str()
            .to_string(),
    })
}

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

/// Production-only composition bridge. Both source acquisitions are opaque
/// outputs of the read-only FINAM acquisition layer; orchestration code cannot
/// manufacture their completeness or exact-lookup state.
#[allow(clippy::too_many_arguments)]
pub(crate) fn reconcile_persist_and_cover_stage8a4_from_production_sources(
    capability: &Stage8ExecutionCapability,
    issuer: &Stage8a1OperationalAuthorityIssuer,
    owner: &mut Stage7bRecoveryReadyOwner,
    commitment_key: &Stage5gLifecycleCommitmentKey,
    identity: &Stage6DurableRequestIdentityV1,
    command: &Stage6DurableCommandSnapshotV1,
    event_window: Stage8a4ProductionEventWindow,
    known_broker_order_id: Option<broker_core::BrokerOrderId>,
    cancel_original_shape: Option<Stage6DurablePlaceOrderShapeV1>,
    reconciliation_acquisition: Stage8a4ReadonlySourceAcquisition,
    writer_entry_acquisition: Stage8a4ReadonlySourceAcquisition,
    trusted_now: DateTime<Utc>,
) -> Result<Stage7bStage8a4DurableBatchReceipt, Stage8a4DurableCompositionError> {
    let current = owner
        .authorize_stage8a1_durable_request(commitment_key, identity, command)
        .map_err(|_| Stage8a4DurableCompositionError::CurrentAuthority)?;
    let context = issue_durable_request_context_from_current_authority(
        &current,
        command,
        event_window,
        known_broker_order_id,
        cancel_original_shape,
    )?;
    let policy = issue_stage8a4_policy_from_frozen_config(trusted_now);
    let (truth, evidence) = issue_stage8a4_source_evidence_from_readonly_acquisition(
        reconciliation_acquisition,
        &policy,
    );
    let reconciliation_truth =
        super::super::admit_stage8a4_broker_truth(&context, &policy, truth, evidence)
            .map_err(|_| Stage8a4DurableCompositionError::CurrentTruth)?;
    let (writer_truth, writer_evidence) =
        issue_stage8a4_source_evidence_from_readonly_acquisition(writer_entry_acquisition, &policy);
    let writer_entry_truth =
        super::super::admit_stage8a4_broker_truth(&context, &policy, writer_truth, writer_evidence)
            .map_err(|_| Stage8a4DurableCompositionError::CurrentTruth)?;
    reconcile_persist_and_cover_stage8a4(
        capability,
        issuer,
        owner,
        commitment_key,
        identity,
        command,
        context,
        reconciliation_truth,
        writer_entry_truth,
        policy,
    )
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
        issuer,
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
    entry: Stage6Stage8a4ValidatedWriteEntry,
}

/// Separate linear authority for an already-persisted incomplete I3 batch.
/// It cannot enter the normal new-transition writer path.
pub struct Stage8a4PendingRecoveryWriteAuthority {
    entry: Stage6Stage8a4ValidatedWriteEntry,
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
        owner.append_stage8a4_validated_entry_and_cover(commitment_key, self.entry)
    }
}

impl Stage8a4PendingRecoveryWriteAuthority {
    fn persist_and_cover(
        self,
        owner: Stage8a4I3RecoveryPendingOwner,
        commitment_key: &Stage5gLifecycleCommitmentKey,
    ) -> Result<
        (
            Stage7bStage8a4DurableBatchReceipt,
            Stage7bRecoveryReadyOwner,
        ),
        Stage7bRecoveryError,
    > {
        owner.append_recovery_entry_and_cover(commitment_key, self.entry)
    }
}

/// Production restart entry for a V2 transition whose exact suffix was only
/// partly persisted. It re-derives authority from current Stage7B S0, current
/// broker truth and current post-effect controls; it never needs the lost I2
/// candidate and never appends a second V2.
pub fn recover_persisted_stage8a4_suffix_and_cover(
    mut owner: Stage8a4I3RecoveryPendingOwner,
    commitment_key: &Stage5gLifecycleCommitmentKey,
    authority_root: &Path,
    accepted_config_sha256: &str,
    current_truth: Stage8a4FreshTruthAdmission,
) -> Result<
    (
        Stage7bStage8a4DurableBatchReceipt,
        Stage7bRecoveryReadyOwner,
    ),
    Stage8a4DurableCompositionError,
> {
    let pending = owner
        .pending_recovery_material(commitment_key)
        .map_err(|_| Stage8a4DurableCompositionError::DurableRecovery)?;
    let identity = pending
        .transition_record()
        .durable_request_identity()
        .clone();
    let command = pending.durable_command().clone();
    let (issuer, historical) = Stage8a1OperationalAuthorityIssuer::from_stage8a4_pending_owner(
        &mut owner,
        commitment_key,
        &identity,
        &command,
        authority_root,
        accepted_config_sha256,
    )
    .map_err(|_| Stage8a4DurableCompositionError::CurrentAuthority)?;
    let controls = issuer
        .issue_stage8a4_post_effect_control_evidence(historical)
        .map_err(|_| Stage8a4DurableCompositionError::CurrentAuthority)?;
    let authority = issue_persisted_recovery_write_authority(
        pending,
        current_truth,
        controls,
        &issuer,
        &mut owner,
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
        .map_err(|_| Stage8a4DurableCompositionError::DurableRecovery)
}

struct Stage8a4WriterEntryFreshTruth {
    safety: PrivateAccountSafetySummary,
    source_evidence_binding_sha256: Stage6Sha256Digest,
    truth_binding_sha256: Stage6Sha256Digest,
    _validated_at: DateTime<Utc>,
}

fn issue_private_durable_write_authority(
    candidate: Stage8a4I2DurableCandidate,
    current_truth: Stage8a4FreshTruthAdmission,
    controls: Stage8a4PostEffectControlEvidence,
    issuer: &Stage8a1OperationalAuthorityIssuer,
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
    let operational_identity_sha256 = current_operational_identity_sha256.to_string();
    let runtime_config_fingerprint_sha256 = current_stage6
        .runtime_config_fingerprint_sha256()
        .to_string();
    let seal_commitment_sha256 = current_seal_commitment_sha256.to_string();
    let source_evidence_binding_sha256 = writer_truth.source_evidence_binding_sha256;
    let writer_truth_binding_sha256 = writer_truth.truth_binding_sha256;
    let control_binding_sha256 = controls.current_control_binding_sha256().clone();
    let attestation_sha256 = stage8a4_writer_entry_attestation_sha256(
        &identity,
        &candidate.durable_command,
        &batch,
        &operational_identity_sha256,
        &runtime_config_fingerprint_sha256,
        current_seal_generation,
        &seal_commitment_sha256,
        &source_evidence_binding_sha256,
        &writer_truth_binding_sha256,
        &control_binding_sha256,
    )
    .map_err(Stage8a4I3Error::BatchInvalid)?;
    let (issuer_public_key_hex, issuer_signature_hex) = issuer
        .sign_stage8a4_writer_attestation(&attestation_sha256)
        .map_err(|_| Stage8a4I3Error::CommandAuthorityMismatch)?;
    let entry = Stage6Stage8a4ValidatedWriteEntry::verify_issuer_attestation(
        identity,
        candidate.durable_command,
        batch,
        operational_identity_sha256,
        runtime_config_fingerprint_sha256,
        current_seal_generation,
        seal_commitment_sha256,
        source_evidence_binding_sha256,
        writer_truth_binding_sha256,
        control_binding_sha256,
        issuer_public_key_hex,
        issuer_signature_hex,
    )
    .map_err(Stage8a4I3Error::BatchInvalid)?;
    Ok(Stage8a4DurableWriteAuthority { entry })
}

fn issue_persisted_recovery_write_authority(
    pending: Stage6Stage8a4PendingRecovery,
    current_truth: Stage8a4FreshTruthAdmission,
    controls: Stage8a4PostEffectControlEvidence,
    issuer: &Stage8a1OperationalAuthorityIssuer,
    owner: &mut Stage8a4I3RecoveryPendingOwner,
    commitment_key: &Stage5gLifecycleCommitmentKey,
) -> Result<Stage8a4PendingRecoveryWriteAuthority, Stage8a4I3Error> {
    let (transition, command) = pending.into_parts();
    let identity = transition.durable_request_identity().clone();
    let current_stage7 = owner
        .authorize_pending_recovery_request(commitment_key, &identity, &command)
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
    let operational_identity_sha256 = current_stage7.operational_identity_sha256().to_string();
    let runtime_config_fingerprint_sha256 = current_stage6
        .runtime_config_fingerprint_sha256()
        .to_string();
    let seal_generation = current_stage7.seal_generation();
    let seal_commitment_sha256 = current_stage7.seal_commitment_sha256().to_string();
    let source_evidence_binding_sha256 = writer_truth.source_evidence_binding_sha256;
    let writer_truth_binding_sha256 = writer_truth.truth_binding_sha256;
    let control_binding_sha256 = controls.current_control_binding_sha256().clone();
    let attestation_sha256 = stage8a4_writer_entry_attestation_sha256(
        &identity,
        &command,
        &batch,
        &operational_identity_sha256,
        &runtime_config_fingerprint_sha256,
        seal_generation,
        &seal_commitment_sha256,
        &source_evidence_binding_sha256,
        &writer_truth_binding_sha256,
        &control_binding_sha256,
    )
    .map_err(Stage8a4I3Error::BatchInvalid)?;
    let (issuer_public_key_hex, issuer_signature_hex) = issuer
        .sign_stage8a4_writer_attestation(&attestation_sha256)
        .map_err(|_| Stage8a4I3Error::CommandAuthorityMismatch)?;
    let entry = Stage6Stage8a4ValidatedWriteEntry::verify_issuer_attestation(
        identity,
        command,
        batch,
        operational_identity_sha256,
        runtime_config_fingerprint_sha256,
        seal_generation,
        seal_commitment_sha256,
        source_evidence_binding_sha256,
        writer_truth_binding_sha256,
        control_binding_sha256,
        issuer_public_key_hex,
        issuer_signature_hex,
    )
    .map_err(Stage8a4I3Error::BatchInvalid)?;
    Ok(Stage8a4PendingRecoveryWriteAuthority { entry })
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
        source_evidence_binding_sha256,
        truth_binding_sha256,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stage8a1_execution_capability::stage8a4_test_production_place_capability;
    use crate::stage8a4_reconciliation::{
        Stage8a4PrivateExactLookup, Stage8a4SourceTiming, Stage8a4TradeIntervalProof,
    };
    use broker_core::{
        BrokerCommand, BrokerInstrumentSpec, BrokerKind, BrokerOrderId, BrokerOrderSnapshot,
        BrokerSymbol, BrokerTruthSnapshot, InstrumentMapEntry, InternalSymbol, OrderStatus,
    };
    use chrono::Duration;
    use runtime_durable_service::{
        stage8a4_i3_production_test_setup, stage8a4_i3_production_test_setup_in,
        stage8a4_i3_test_fail_before_covering_seal, stage8a4_i3_test_set_owner_journal_failpoint,
        Stage7bDurableRootAuthority, Stage7bRestartOutcome, Stage8a4I3ProductionTestSetup,
        Stage8a4I3RecoveryPendingOwner,
    };
    use rust_decimal::Decimal;
    use std::process::{Command, Stdio};
    use std::thread;
    use std::time::{Duration as StdDuration, Instant};
    use strategy_runtime_core::Stage8a4JournalTestFailpoint;

    enum ProductionCrashPoint {
        JournalAppend(u32),
        BeforeJournalAppend(u32),
        BeforeCoveringSeal,
    }

    fn production_source(
        place: &broker_core::PlaceOrder,
        event_start: DateTime<Utc>,
        event_end: DateTime<Utc>,
        possible_effect_at: DateTime<Utc>,
        trusted_now: DateTime<Utc>,
    ) -> Stage8a4ReadonlySourceAcquisition {
        let request_started_at = trusted_now - Duration::milliseconds(400);
        let response_received_at = trusted_now - Duration::milliseconds(100);
        let venue_symbol = place
            .instrument
            .venue_symbol
            .clone()
            .expect("Stage8A4 production fixture venue symbol");
        let truth = BrokerTruthSnapshot {
            account_id: place.account_id.clone(),
            orders: vec![BrokerOrderSnapshot {
                account_id: place.account_id.clone(),
                broker_order_id: Some(BrokerOrderId::new("STAGE8A4-I3-ORDER-1")),
                client_order_id: Some(place.client_order_id.clone()),
                instrument: place.instrument.clone(),
                side: place.side,
                order_type: place.order_type,
                time_in_force: Some(place.time_in_force),
                status: OrderStatus::New,
                lifecycle: BrokerOrderSnapshot::lifecycle_for(&OrderStatus::New),
                qty: place.qty,
                filled_qty: Decimal::ZERO,
                remaining_qty: Some(place.qty),
                limit_price: place.limit_price,
                broker_asset_id: Some("ASSET_IMOEXF".into()),
                board: Some("RFUD".into()),
                expiration_date: None,
                source_ts: Some(response_received_at),
                received_ts: response_received_at,
            }],
            positions: vec![],
            cash: None,
            trades: vec![],
            instruments: vec![BrokerInstrumentSpec {
                instrument: InstrumentMapEntry {
                    internal_symbol: InternalSymbol(place.instrument.symbol.clone()),
                    broker: BrokerKind::Finam,
                    broker_symbol: BrokerSymbol(venue_symbol),
                    exchange: place.instrument.exchange.clone(),
                    market: place.instrument.market.clone(),
                    price_step: Decimal::new(5, 1),
                    qty_step: Decimal::ONE,
                    lot_size: Decimal::ONE,
                    min_qty: Decimal::ONE,
                    step_value: Decimal::ONE,
                    currency: "RUB".into(),
                    schedule_id: "MOEX_FUT".into(),
                    expiration_date: None,
                    is_tradable: true,
                },
                broker_asset_id: Some("ASSET_IMOEXF".into()),
                board: Some("RFUD".into()),
                long_initial_margin: None,
                short_initial_margin: None,
            }],
            received_ts: response_received_at,
        };
        let timing = || Stage8a4SourceTiming {
            request_started_at,
            response_received_at,
        };
        Stage8a4ReadonlySourceAcquisition {
            truth,
            orders_timing: timing(),
            positions_timing: timing(),
            instrument_timing: timing(),
            trade_intervals: vec![Stage8a4TradeIntervalProof {
                start_inclusive: event_start,
                end_exclusive: event_end,
                requested_limit: 100,
                returned_count: 0,
                request_started_at: request_started_at.max(possible_effect_at),
                response_received_at,
                split_depth: 0,
            }],
            exact_lookup: Stage8a4PrivateExactLookup::NotAttempted,
        }
    }

    #[allow(clippy::type_complexity)]
    fn production_pending_after_crash(
        crash_point: ProductionCrashPoint,
        authority_name: &str,
    ) -> (
        Stage8a4I3ProductionTestSetup,
        Stage6DurableRequestIdentityV1,
        Stage6DurableCommandSnapshotV1,
        broker_core::PlaceOrder,
        std::path::PathBuf,
        String,
        Box<Stage8a4I3RecoveryPendingOwner>,
    ) {
        let (setup, mut owner) = stage8a4_i3_production_test_setup();
        let BrokerCommand::PlaceOrder(place) = &setup.command else {
            panic!("Stage8A4 production fixture must contain PLACE");
        };
        let place = place.clone();
        let identity = Stage6DurableRequestIdentityV1::from_place(
            &place,
            setup.command_context.attribution().clone(),
        )
        .expect("Stage8A4 production identity");
        let command = Stage6DurableCommandSnapshotV1::from_place(&identity, &place)
            .expect("Stage8A4 production command");
        let authority_root = setup.parent.join(authority_name);
        let (issuer, capability) = stage8a4_test_production_place_capability(
            &mut owner,
            &setup.commitment_key,
            &identity,
            &command,
            &place,
            &authority_root,
        )
        .expect("production Stage8A1 capability");
        let accepted_config_sha256 = std::fs::read_to_string(
            authority_root.join("stage8a1-accepted-execution-config.json.sha256"),
        )
        .expect("read accepted config sidecar before simulated process loss");
        match crash_point {
            ProductionCrashPoint::JournalAppend(append_number) => {
                stage8a4_i3_test_set_owner_journal_failpoint(
                    &mut owner,
                    Stage8a4JournalTestFailpoint::AfterFrameHashWriteOnAppend(append_number),
                )
                .expect("install deterministic journal crash point");
            }
            ProductionCrashPoint::BeforeJournalAppend(append_number) => {
                stage8a4_i3_test_set_owner_journal_failpoint(
                    &mut owner,
                    Stage8a4JournalTestFailpoint::BeforeFrameWriteOnAppend(append_number),
                )
                .expect("install deterministic pre-append crash point");
            }
            ProductionCrashPoint::BeforeCoveringSeal => {
                stage8a4_i3_test_fail_before_covering_seal(&mut owner);
            }
        }
        let trusted_now = Utc::now();
        let possible_effect_at = trusted_now - Duration::seconds(3);
        let event_start = trusted_now - Duration::seconds(2);
        let event_end = trusted_now - Duration::seconds(1);
        let result = reconcile_persist_and_cover_stage8a4_from_production_sources(
            &capability,
            &issuer,
            &mut owner,
            &setup.commitment_key,
            &identity,
            &command,
            Stage8a4ProductionEventWindow::from_dispatch_and_acquisition(
                possible_effect_at,
                event_start,
                event_end,
            )
            .expect("valid production event window"),
            None,
            None,
            production_source(
                &place,
                event_start,
                event_end,
                possible_effect_at,
                trusted_now,
            ),
            production_source(
                &place,
                event_start,
                event_end,
                possible_effect_at,
                trusted_now,
            ),
            trusted_now,
        );
        assert!(matches!(
            result,
            Err(Stage8a4DurableCompositionError::DurableRecovery)
        ));
        drop(capability);
        drop(issuer);
        drop(owner);

        let restart = || {
            Stage7bRecoveryReadyOwner::restart(
                Stage7bDurableRootAuthority::validate(&setup.root, &setup.operational_identity)
                    .expect("revalidate production durable root"),
                setup.operational_identity.clone(),
                &setup.commitment_key,
                setup.runtime.clone(),
            )
            .expect("restart production crash state")
        };
        let Stage7bRestartOutcome::Stage8a4I3Pending(first_pending) = restart() else {
            panic!("uncovered production I3 batch must restart Pending");
        };
        assert!(!first_pending.recovery_ready());
        drop(first_pending);
        let Stage7bRestartOutcome::Stage8a4I3Pending(second_pending) = restart() else {
            panic!("repeated restart must remain Pending before production recovery");
        };
        assert!(!second_pending.recovery_ready());
        (
            setup,
            identity,
            command,
            place,
            authority_root,
            accepted_config_sha256,
            second_pending,
        )
    }

    fn current_recovery_admission(
        owner: &mut Stage8a4I3RecoveryPendingOwner,
        commitment_key: &Stage5gLifecycleCommitmentKey,
        identity: &Stage6DurableRequestIdentityV1,
        command: &Stage6DurableCommandSnapshotV1,
        place: &broker_core::PlaceOrder,
    ) -> Stage8a4FreshTruthAdmission {
        let trusted_now = Utc::now();
        let possible_effect_at = trusted_now - Duration::seconds(3);
        let event_start = trusted_now - Duration::seconds(2);
        let event_end = trusted_now - Duration::seconds(1);
        let current = owner
            .authorize_pending_recovery_request(commitment_key, identity, command)
            .expect("current pending recovery request authority");
        let context = issue_durable_request_context_from_current_authority(
            &current,
            command,
            Stage8a4ProductionEventWindow::from_dispatch_and_acquisition(
                possible_effect_at,
                event_start,
                event_end,
            )
            .expect("valid recovery event window"),
            None,
            None,
        )
        .expect("current recovery durable context");
        let policy = issue_stage8a4_policy_from_frozen_config(trusted_now);
        let (truth, evidence) = issue_stage8a4_source_evidence_from_readonly_acquisition(
            production_source(
                place,
                event_start,
                event_end,
                possible_effect_at,
                trusted_now,
            ),
            &policy,
        );
        super::super::super::admit_stage8a4_broker_truth(&context, &policy, truth, evidence)
            .expect("fresh production recovery truth")
    }

    fn assert_production_recovery(
        crash_point: ProductionCrashPoint,
        authority_name: &str,
        expected_appended_suffix_records: usize,
    ) {
        let (setup, identity, command, place, authority_root, accepted_config_sha256, mut pending) =
            production_pending_after_crash(crash_point, authority_name);
        let truth = current_recovery_admission(
            &mut pending,
            &setup.commitment_key,
            &identity,
            &command,
            &place,
        );
        let arm_registry = authority_root.join("stage8a1-arm-nonces");
        let arm_entries_before = std::fs::read_dir(&arm_registry)
            .expect("read historical arm registry before recovery")
            .count();
        assert_eq!(arm_entries_before, 1, "exactly one historical arm exists");
        let (receipt, ready) = recover_persisted_stage8a4_suffix_and_cover(
            *pending,
            &setup.commitment_key,
            &authority_root,
            &accepted_config_sha256,
            truth,
        )
        .expect("actual production recovery entry repairs suffix and covers S1");
        assert_eq!(
            std::fs::read_dir(&arm_registry)
                .expect("read historical arm registry after recovery")
                .count(),
            arm_entries_before,
            "recovery must read the existing arm and never register another",
        );
        assert!(receipt.transition_was_existing());
        assert_eq!(
            receipt.appended_suffix_records(),
            expected_appended_suffix_records
        );
        assert!(receipt.covering_seal_generation() > 1);
        drop(ready);

        let final_restart = Stage7bRecoveryReadyOwner::restart(
            Stage7bDurableRootAuthority::validate(&setup.root, &setup.operational_identity)
                .expect("revalidate repaired durable root"),
            setup.operational_identity.clone(),
            &setup.commitment_key,
            setup.runtime.clone(),
        )
        .expect("restart repaired production journal");
        assert!(matches!(final_restart, Stage7bRestartOutcome::Ready(_)));
        drop(final_restart);
        std::fs::remove_dir_all(&setup.parent).expect("remove recovery fixture");
    }

    #[test]
    fn stage8a4_i3_normal_production_path_persists_exact_batch_covers_s1_and_restarts_ready() {
        let (setup, mut owner) = stage8a4_i3_production_test_setup();
        let BrokerCommand::PlaceOrder(place) = &setup.command else {
            panic!("Stage8A4 production fixture must contain PLACE");
        };
        let identity = Stage6DurableRequestIdentityV1::from_place(
            place,
            setup.command_context.attribution().clone(),
        )
        .expect("Stage8A4 production identity");
        let command = Stage6DurableCommandSnapshotV1::from_place(&identity, place)
            .expect("Stage8A4 production command");
        let authority_root = setup.parent.join("stage8a4-authority-normal");
        let (issuer, capability) = stage8a4_test_production_place_capability(
            &mut owner,
            &setup.commitment_key,
            &identity,
            &command,
            place,
            &authority_root,
        )
        .expect("production Stage8A1 capability");

        let trusted_now = Utc::now();
        let possible_effect_at = trusted_now - Duration::seconds(3);
        let event_start = trusted_now - Duration::seconds(2);
        let event_end = trusted_now - Duration::seconds(1);
        let event_window = Stage8a4ProductionEventWindow::from_dispatch_and_acquisition(
            possible_effect_at,
            event_start,
            event_end,
        )
        .expect("valid production event window");
        let reconciliation_source = production_source(
            place,
            event_start,
            event_end,
            possible_effect_at,
            trusted_now,
        );
        let writer_source = production_source(
            place,
            event_start,
            event_end,
            possible_effect_at,
            trusted_now,
        );
        let receipt = reconcile_persist_and_cover_stage8a4_from_production_sources(
            &capability,
            &issuer,
            &mut owner,
            &setup.commitment_key,
            &identity,
            &command,
            event_window,
            None,
            None,
            reconciliation_source,
            writer_source,
            trusted_now,
        )
        .expect("production I3 persistence and covering S1");
        assert!(!receipt.transition_was_existing());
        assert!(receipt.appended_suffix_records() > 0);
        assert!(receipt.covering_seal_generation() > 1);
        assert_eq!(receipt.stage6_checkpoint_sha256().len(), 64);
        drop(owner);

        let restarted = Stage7bRecoveryReadyOwner::restart(
            Stage7bDurableRootAuthority::validate(&setup.root, &setup.operational_identity)
                .expect("revalidate production durable root"),
            setup.operational_identity.clone(),
            &setup.commitment_key,
            setup.runtime.clone(),
        )
        .expect("restart covered production journal");
        assert!(matches!(restarted, Stage7bRestartOutcome::Ready(_)));
        std::fs::remove_dir_all(&setup.parent).expect("remove production fixture");
    }

    #[test]
    fn stage8a4_i3_production_recovery_repairs_v2_only_crash_and_covers_s1() {
        assert_production_recovery(
            ProductionCrashPoint::JournalAppend(1),
            "stage8a4-authority-v2-only",
            2,
        );
    }

    #[test]
    fn stage8a4_i3_production_recovery_repairs_partial_exact_suffix_and_covers_s1() {
        assert_production_recovery(
            ProductionCrashPoint::BeforeJournalAppend(3),
            "stage8a4-authority-partial-suffix",
            1,
        );
    }

    #[test]
    fn stage8a4_i3_production_recovery_covers_complete_batch_without_s1() {
        assert_production_recovery(
            ProductionCrashPoint::BeforeCoveringSeal,
            "stage8a4-authority-complete-before-s1",
            0,
        );
    }

    fn assert_arm_registry_failure_remains_pending(replace: bool) {
        let (setup, identity, command, place, authority_root, accepted_config_sha256, mut pending) =
            production_pending_after_crash(
                ProductionCrashPoint::JournalAppend(1),
                if replace {
                    "stage8a4-authority-replaced-arm"
                } else {
                    "stage8a4-authority-missing-arm"
                },
            );
        let truth = current_recovery_admission(
            &mut pending,
            &setup.commitment_key,
            &identity,
            &command,
            &place,
        );
        let arm_registry = authority_root.join("stage8a1-arm-nonces");
        let arm_path = std::fs::read_dir(&arm_registry)
            .expect("read arm registry")
            .next()
            .expect("historical arm entry")
            .expect("read historical arm entry")
            .path();
        if replace {
            let mut bytes = std::fs::read(&arm_path).expect("read historical arm fixture");
            let marker = b"\"exact_command_sha256\":\"";
            let value_start = bytes
                .windows(marker.len())
                .position(|window| window == marker)
                .map(|offset| offset + marker.len())
                .expect("exact command field in arm registration");
            bytes[value_start] = if bytes[value_start] == b'f' {
                b'e'
            } else {
                b'f'
            };
            std::fs::write(&arm_path, bytes)
                .expect("replace signed historical arm binding fixture");
        } else {
            std::fs::remove_file(&arm_path).expect("remove historical arm fixture");
        }
        let result = recover_persisted_stage8a4_suffix_and_cover(
            *pending,
            &setup.commitment_key,
            &authority_root,
            &accepted_config_sha256,
            truth,
        );
        assert!(matches!(
            result,
            Err(Stage8a4DurableCompositionError::CurrentAuthority)
        ));
        let restarted = Stage7bRecoveryReadyOwner::restart(
            Stage7bDurableRootAuthority::validate(&setup.root, &setup.operational_identity)
                .expect("revalidate blocked recovery durable root"),
            setup.operational_identity.clone(),
            &setup.commitment_key,
            setup.runtime.clone(),
        )
        .expect("restart blocked recovery state");
        assert!(matches!(
            restarted,
            Stage7bRestartOutcome::Stage8a4I3Pending(_)
        ));
        std::fs::remove_dir_all(&setup.parent).expect("remove blocked recovery fixture");
    }

    #[test]
    fn stage8a4_i3_fresh_process_recovery_rejects_missing_historical_arm() {
        assert_arm_registry_failure_remains_pending(false);
    }

    #[test]
    fn stage8a4_i3_fresh_process_recovery_rejects_replaced_historical_arm() {
        assert_arm_registry_failure_remains_pending(true);
    }

    #[test]
    #[ignore = "spawned explicitly by the fresh-process recovery witness"]
    fn stage8a4_i3_v2_only_process_a_crash_child() {
        let parent = std::path::PathBuf::from(
            std::env::var_os("STAGE8A4_I3_R5_PARENT").expect("child fixture parent"),
        );
        let ready = std::path::PathBuf::from(
            std::env::var_os("STAGE8A4_I3_R5_READY").expect("child ready marker"),
        );
        let (setup, mut owner) = stage8a4_i3_production_test_setup_in(parent);
        let BrokerCommand::PlaceOrder(place) = &setup.command else {
            panic!("Stage8A4 subprocess fixture must contain PLACE");
        };
        let place = place.clone();
        let identity = Stage6DurableRequestIdentityV1::from_place(
            &place,
            setup.command_context.attribution().clone(),
        )
        .expect("subprocess request identity");
        let command = Stage6DurableCommandSnapshotV1::from_place(&identity, &place)
            .expect("subprocess durable command");
        let authority_root = setup.parent.join("stage8a4-authority-subprocess");
        let (issuer, capability) = stage8a4_test_production_place_capability(
            &mut owner,
            &setup.commitment_key,
            &identity,
            &command,
            &place,
            &authority_root,
        )
        .expect("subprocess production Stage8A1 capability");
        stage8a4_i3_test_set_owner_journal_failpoint(
            &mut owner,
            Stage8a4JournalTestFailpoint::AfterFrameHashWriteOnAppend(1),
        )
        .expect("install subprocess V2 crash point");
        let trusted_now = Utc::now();
        let possible_effect_at = trusted_now - Duration::seconds(3);
        let event_start = trusted_now - Duration::seconds(2);
        let event_end = trusted_now - Duration::seconds(1);
        let result = reconcile_persist_and_cover_stage8a4_from_production_sources(
            &capability,
            &issuer,
            &mut owner,
            &setup.commitment_key,
            &identity,
            &command,
            Stage8a4ProductionEventWindow::from_dispatch_and_acquisition(
                possible_effect_at,
                event_start,
                event_end,
            )
            .expect("subprocess event window"),
            None,
            None,
            production_source(
                &place,
                event_start,
                event_end,
                possible_effect_at,
                trusted_now,
            ),
            production_source(
                &place,
                event_start,
                event_end,
                possible_effect_at,
                trusted_now,
            ),
            trusted_now,
        );
        assert!(matches!(
            result,
            Err(Stage8a4DurableCompositionError::DurableRecovery)
        ));
        std::fs::write(&ready, b"v2-durable-arm-registered-owner-still-live")
            .expect("publish subprocess crash barrier");
        loop {
            thread::sleep(StdDuration::from_secs(1));
        }
    }

    fn wait_for_subprocess_barrier(child: &mut std::process::Child, ready: &Path) {
        let deadline = Instant::now() + StdDuration::from_secs(20);
        while !ready.exists() && Instant::now() < deadline {
            if let Some(status) = child.try_wait().expect("poll subprocess") {
                panic!("process A exited before crash barrier: {status}");
            }
            thread::sleep(StdDuration::from_millis(25));
        }
        assert!(ready.exists(), "process A did not reach V2 crash barrier");
    }

    #[test]
    fn stage8a4_i3_v2_only_sigkill_recovers_in_fresh_process_without_precrash_objects() {
        let parent = std::env::temp_dir().join(format!(
            "stage8a4-i3-r5-subprocess-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir(&parent).expect("create subprocess fixture parent");
        let parent = std::fs::canonicalize(parent).expect("canonical subprocess fixture parent");
        let ready = parent.join("process-a-ready");
        let child_test = concat!(
            "stage8a4_reconciliation::durable_composition_i2::durable_writer_i3::tests::",
            "stage8a4_i3_v2_only_process_a_crash_child"
        );
        let mut child = Command::new(std::env::current_exe().expect("current test executable"))
            .arg("--ignored")
            .arg("--exact")
            .arg(child_test)
            .arg("--nocapture")
            .env("STAGE8A4_I3_R5_PARENT", &parent)
            .env("STAGE8A4_I3_R5_READY", &ready)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn process A");
        wait_for_subprocess_barrier(&mut child, &ready);
        child.kill().expect("SIGKILL process A");
        let status = child.wait().expect("reap process A");
        assert!(
            !status.success(),
            "process A must die at the crash boundary"
        );

        let fixture = strategy_runtime_core::stage7b_test_authenticated_working_restart_fixture(
            strategy_runtime_core::Stage7bTestExtraStage6History::None,
        );
        let operational_identity = strategy_runtime_core::Stage6dOperationalIdentityConfig {
            broker_id: "paper".to_string(),
            strategy_instance_id: "hybrid-imoexf".to_string(),
            deployment_id: "stage8a4-i3-production-test".to_string(),
            deployment_generation: 1,
            gateway_instance_id: "gateway-stage8a4-i3-test".to_string(),
            instrument_map_fingerprint_sha256: "1".repeat(64),
            market_data_generation: 1,
            command_consumer_generation: 1,
            stage8a4_writer_issuer_public_key_hex:
                "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a".to_string(),
        };
        let root = parent.join(
            Stage7bDurableRootAuthority::expected_directory_name(&operational_identity)
                .expect("subprocess durable root name"),
        );
        let restart = Stage7bRecoveryReadyOwner::restart(
            Stage7bDurableRootAuthority::validate(&root, &operational_identity)
                .expect("process B validates durable root"),
            operational_identity.clone(),
            &fixture.commitment_key,
            fixture.fresh_runtime.clone(),
        )
        .expect("process B restarts durable owner");
        let Stage7bRestartOutcome::Stage8a4I3Pending(mut pending) = restart else {
            panic!("process B must observe uncovered V2 as Pending");
        };
        let material = pending
            .pending_recovery_material(&fixture.commitment_key)
            .expect("process B reads persisted V2");
        let identity = material
            .transition_record()
            .durable_request_identity()
            .clone();
        let command = material.durable_command().clone();
        let BrokerCommand::PlaceOrder(place) = fixture.command else {
            panic!("fresh process fixture must contain PLACE");
        };
        let truth = current_recovery_admission(
            &mut pending,
            &fixture.commitment_key,
            &identity,
            &command,
            &place,
        );
        let authority_root = parent.join("stage8a4-authority-subprocess");
        let accepted_config_sha256 = std::fs::read_to_string(
            authority_root.join("stage8a1-accepted-execution-config.json.sha256"),
        )
        .expect("process B reads accepted config authority");
        let arm_registry = authority_root.join("stage8a1-arm-nonces");
        let arm_entries_before = std::fs::read_dir(&arm_registry)
            .expect("process B reads historical arm registry")
            .count();
        assert_eq!(arm_entries_before, 1);
        let (receipt, ready_owner) = recover_persisted_stage8a4_suffix_and_cover(
            *pending,
            &fixture.commitment_key,
            &authority_root,
            &accepted_config_sha256,
            truth,
        )
        .expect("process B recovers exact suffix and covering S1");
        assert!(receipt.transition_was_existing());
        assert_eq!(receipt.appended_suffix_records(), 2);
        assert_eq!(
            std::fs::read_dir(&arm_registry)
                .expect("process B rereads historical arm registry")
                .count(),
            arm_entries_before,
            "fresh-process recovery must not register a new arm",
        );
        drop(ready_owner);
        let final_restart = Stage7bRecoveryReadyOwner::restart(
            Stage7bDurableRootAuthority::validate(&root, &operational_identity)
                .expect("validate recovered durable root"),
            operational_identity,
            &fixture.commitment_key,
            fixture.fresh_runtime,
        )
        .expect("restart recovered subprocess state");
        assert!(matches!(final_restart, Stage7bRestartOutcome::Ready(_)));
        std::fs::remove_dir_all(parent).expect("remove subprocess fixture");
    }
}
