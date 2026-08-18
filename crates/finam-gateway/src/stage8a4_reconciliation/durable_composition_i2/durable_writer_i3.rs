//! Stage 8A-4 I3 sealed durable writer authority.
//!
//! The public type is opaque and linear. Its only issuer consumes the private
//! I2 candidate plus exact current truth/control and current Stage 6/7 facts.
//! Public V2/read DTOs have no conversion into this authority.

use super::super::{
    account_safety_binding, account_safety_summary, canonical_truth_binding,
    Stage8a4FreshTruthAdmission,
};
use super::{parse_digest, PrivateAccountSafetySummary, Stage8a4I2DurableCandidate};
use crate::stage8a1_execution_capability::{
    Stage8a4PostEffectControlEvidence, Stage8a4PostEffectControlState,
};
use chrono::{DateTime, Utc};
use strategy_runtime_core::{
    Stage6DurableCommandSnapshotV1, Stage6DurableRequestAuthorityV1,
    Stage6DurableRequestIdentityV1, Stage6Sha256Digest, Stage6Stage8a4DurableBatch,
};

#[derive(Debug)]
enum Stage8a4I3Error {
    CommandAuthorityMismatch,
    CurrentTruthMismatch,
    BatchInvalid(strategy_runtime_core::Stage6dLiveCoreError),
}

/// Sole linear I3 authority accepted by the Stage 7B writer. It cannot be
/// constructed from public canonical bytes, diagnostics or read DTOs.
pub struct Stage8a4DurableWriteAuthority {
    identity: Stage6DurableRequestIdentityV1,
    command: Stage6DurableCommandSnapshotV1,
    batch: Stage6Stage8a4DurableBatch,
    operational_identity_sha256: String,
    runtime_config_fingerprint_sha256: String,
    seal_generation: u64,
    seal_commitment_sha256: String,
}

/// Consuming cross-crate transfer object. Obtaining one requires first owning
/// the nonconstructible `Stage8a4DurableWriteAuthority`.
pub struct Stage8a4DurableWriterParts {
    identity: Stage6DurableRequestIdentityV1,
    command: Stage6DurableCommandSnapshotV1,
    batch: Stage6Stage8a4DurableBatch,
    operational_identity_sha256: String,
    runtime_config_fingerprint_sha256: String,
    seal_generation: u64,
    seal_commitment_sha256: String,
}

impl Stage8a4DurableWriteAuthority {
    pub fn into_writer_parts(self) -> Stage8a4DurableWriterParts {
        Stage8a4DurableWriterParts {
            identity: self.identity,
            command: self.command,
            batch: self.batch,
            operational_identity_sha256: self.operational_identity_sha256,
            runtime_config_fingerprint_sha256: self.runtime_config_fingerprint_sha256,
            seal_generation: self.seal_generation,
            seal_commitment_sha256: self.seal_commitment_sha256,
        }
    }
}

impl Stage8a4DurableWriterParts {
    pub fn into_inner(
        self,
    ) -> (
        Stage6DurableRequestIdentityV1,
        Stage6DurableCommandSnapshotV1,
        Stage6Stage8a4DurableBatch,
        String,
        String,
        u64,
        String,
    ) {
        (
            self.identity,
            self.command,
            self.batch,
            self.operational_identity_sha256,
            self.runtime_config_fingerprint_sha256,
            self.seal_generation,
            self.seal_commitment_sha256,
        )
    }
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
    current_stage6: Stage6DurableRequestAuthorityV1,
    current_operational_identity_sha256: &str,
    current_seal_generation: u64,
    current_seal_commitment_sha256: &str,
) -> Result<Stage8a4DurableWriteAuthority, Stage8a4I3Error> {
    let identity = candidate
        .transition_record
        .durable_request_identity()
        .clone();
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
    Ok(Stage8a4DurableWriteAuthority {
        identity,
        command: candidate.durable_command,
        batch,
        operational_identity_sha256: current_operational_identity_sha256.to_string(),
        runtime_config_fingerprint_sha256: current_stage6
            .runtime_config_fingerprint_sha256()
            .to_string(),
        seal_generation: current_seal_generation,
        seal_commitment_sha256: current_seal_commitment_sha256.to_string(),
    })
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
