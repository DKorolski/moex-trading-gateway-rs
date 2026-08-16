//! Stage 8A-4 I3 private durable writer composition.
//!
//! This module can only consume the private I2 candidate. It performs no
//! broker call, repeated send, ACK, readiness publication or Redis operation.

use super::super::{account_safety_binding, account_safety_summary, Stage8a4FreshTruthAdmission};
use super::{parse_digest, PrivateAccountSafetySummary, Stage8a4I2DurableCandidate};
use crate::stage8a1_execution_capability::{
    Stage8a4PostEffectControlEvidence, Stage8a4PostEffectControlState,
};
use runtime_durable_service::{
    Stage7bRecoveryError, Stage7bRecoveryReadyOwner, Stage7bStage8a4DurableBatchReceipt,
};
use strategy_runtime_core::{Stage5gLifecycleCommitmentKey, Stage6Stage8a4DurableBatch};

#[derive(Debug)]
enum Stage8a4I3Error {
    CommandAuthorityMismatch,
    CurrentAccountSafetyMismatch,
    BatchInvalid(strategy_runtime_core::Stage6dLiveCoreError),
    DurableOwner(Stage7bRecoveryError),
}

fn append_private_candidate_and_cover(
    owner: &mut Stage7bRecoveryReadyOwner,
    commitment_key: &Stage5gLifecycleCommitmentKey,
    candidate: Stage8a4I2DurableCandidate,
    current_truth: Stage8a4FreshTruthAdmission,
    controls: Stage8a4PostEffectControlEvidence,
) -> Result<Stage7bStage8a4DurableBatchReceipt, Stage8a4I3Error> {
    let identity = candidate
        .transition_record
        .durable_request_identity()
        .clone();
    if controls.accepted_command_payload_sha256() != &candidate.accepted_command_payload_sha256 {
        return Err(Stage8a4I3Error::CommandAuthorityMismatch);
    }
    let _validated_control_binding = controls.current_control_binding_sha256();
    match controls.current_control_state() {
        Stage8a4PostEffectControlState::RunAllowed
        | Stage8a4PostEffectControlState::StopRequested
        | Stage8a4PostEffectControlState::StaleOrUnreadable => {}
    }

    let current = account_safety_summary(&current_truth.truth, identity.instrument());
    let current_binding = parse_digest(&account_safety_binding(&current))
        .map_err(|_| Stage8a4I3Error::CurrentAccountSafetyMismatch)?;
    let current = PrivateAccountSafetySummary {
        account_active_orders_count: current
            .account_active_orders_count
            .try_into()
            .map_err(|_| Stage8a4I3Error::CurrentAccountSafetyMismatch)?,
        account_unknown_orders_count: current
            .account_unknown_orders_count
            .try_into()
            .map_err(|_| Stage8a4I3Error::CurrentAccountSafetyMismatch)?,
        account_orphan_orders_count: current
            .account_orphan_orders_count
            .try_into()
            .map_err(|_| Stage8a4I3Error::CurrentAccountSafetyMismatch)?,
        account_open_positions_count: current
            .account_open_positions_count
            .try_into()
            .map_err(|_| Stage8a4I3Error::CurrentAccountSafetyMismatch)?,
        target_active_orders_count: current
            .target_active_orders_count
            .try_into()
            .map_err(|_| Stage8a4I3Error::CurrentAccountSafetyMismatch)?,
        target_unknown_orders_count: current
            .target_unknown_orders_count
            .try_into()
            .map_err(|_| Stage8a4I3Error::CurrentAccountSafetyMismatch)?,
        target_terminal_orders_count: current
            .target_terminal_orders_count
            .try_into()
            .map_err(|_| Stage8a4I3Error::CurrentAccountSafetyMismatch)?,
        target_inconsistent_orders_count: current
            .target_inconsistent_orders_count
            .try_into()
            .map_err(|_| Stage8a4I3Error::CurrentAccountSafetyMismatch)?,
        target_open_positions_count: current
            .target_open_positions_count
            .try_into()
            .map_err(|_| Stage8a4I3Error::CurrentAccountSafetyMismatch)?,
        other_symbol_active_orders_count: current
            .other_symbol_active_orders_count
            .try_into()
            .map_err(|_| Stage8a4I3Error::CurrentAccountSafetyMismatch)?,
        account_safety_binding_sha256: current_binding,
    };
    let current =
        serde_json::to_vec(&current).map_err(|_| Stage8a4I3Error::CurrentAccountSafetyMismatch)?;
    let persisted = serde_json::to_vec(
        candidate
            .transition_record
            .payload()
            .account_safety_summary(),
    )
    .map_err(|_| Stage8a4I3Error::CurrentAccountSafetyMismatch)?;
    if current != persisted {
        return Err(Stage8a4I3Error::CurrentAccountSafetyMismatch);
    }

    let batch = Stage6Stage8a4DurableBatch::new(
        candidate.transition_record,
        candidate.suffix_records,
        candidate.cancel_original_target_shape,
    )
    .map_err(Stage8a4I3Error::BatchInvalid)?;
    owner
        .append_stage8a4_durable_batch_and_cover(
            commitment_key,
            &identity,
            &candidate.durable_command,
            batch,
        )
        .map_err(Stage8a4I3Error::DurableOwner)
}
