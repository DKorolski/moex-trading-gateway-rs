//! Join a fresh permit to the authenticated current owner, before callback.
use super::*;
use crate::stage8b_o3_readmission::{Stage8bO3DecisionBinding, Stage8bO3PaperPermit};

impl Stage6dDurableRuntimeRecovered {
    pub fn stage8b_o3_decision_binding(
        &self,
        source: &Stage6Stage8bP1SealSourceV1,
        binding: &Stage5gP1SemanticBindingInput,
        observed_source_sha256: &str,
    ) -> Result<Stage8bO3DecisionBinding, Stage6dLiveCoreError> {
        let invalid = Stage6dLiveCoreError::RestartPackageBindingMismatch;
        let Stage6dStage5RuntimeAuthority::Restart(current) = &self.stage5_runtime else {
            return Err(invalid);
        };
        if source.stage6_checkpoint_sha256 != self.authenticated_checkpoint().checkpoint_sha256()
            || source.stage6_frontier_sha256 != frontier_fingerprint(self.journal_frontier())?
            || source.operational_identity_sha256 != binding.operational_identity_sha256
            || self
                .authenticated_operational_identity()
                .map(stage6d_operational_identity_sha256)
                .transpose()?
                .as_ref()
                .map(Stage6Sha256Digest::as_str)
                != Some(source.operational_identity_sha256.as_str())
        {
            return Err(invalid);
        }
        let prior = current
            .stage8b_p1_model_last_bar_label_utc()
            .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
            .ok_or(Stage6dLiveCoreError::RestartPackageBindingMismatch)?
            .timestamp();
        let close_ms = binding
            .m10_redis_id
            .strip_suffix("-0")
            .and_then(|s| s.parse::<i64>().ok())
            .ok_or(Stage6dLiveCoreError::RestartPackageBindingMismatch)?;
        Ok(Stage8bO3DecisionBinding {
            source_seal_generation: source.seal_generation,
            source_seal_commitment_sha256: source.seal_commitment_sha256.clone(),
            source_checkpoint_sha256: source.stage6_checkpoint_sha256.clone(),
            source_frontier_sha256: source.stage6_frontier_sha256.clone(),
            m10_redis_id: binding.m10_redis_id.clone(),
            m10_semantic_id_sha256: binding.m10_semantic_id_sha256.clone(),
            m10_payload_sha256: binding.m10_payload_sha256.clone(),
            observed_source_sha256: observed_source_sha256.into(),
            prior_model_label: prior,
            candidate_model_label: close_ms / 1000 - 600,
        })
    }
}

/// No raw/standalone Stage5C API can attach this permission: the sole durable
/// owner must be consumed and all source/seal/candidate identities must join.
pub fn apply_stage8b_o3_semantic_transition(
    recovered: Stage6dDurableRuntimeRecovered,
    mut accepted: crate::Stage5cAcceptedSemanticBar,
    binding: Stage5gP1SemanticBindingInput,
    source: Stage6Stage8bP1SealSourceV1,
    permit: Stage8bO3PaperPermit,
    commitment_key: &Stage5gLifecycleCommitmentKey,
) -> Result<Stage6Stage8bP1SemanticTransition, Stage6dLiveCoreError> {
    let invalid = || Stage6dLiveCoreError::RestartPackageBindingMismatch;
    let (observed, label) = accepted.p1_observed_source().ok_or_else(invalid)?;
    let expected = recovered.stage8b_o3_decision_binding(&source, &binding, observed.sha256())?;
    let Stage6dStage5RuntimeAuthority::Restart(current) = &recovered.stage5_runtime else {
        return Err(invalid());
    };
    if permit.binding() != &expected
        || permit.scope().operational_identity_sha256 != source.operational_identity_sha256
        || label != expected.candidate_model_label
        || !current.stage8b_o3_join_permit(&permit)
        || permit.execution_time().is_err()
    {
        return Err(invalid());
    }
    let later = recovered.stage8b_p1d2_truth_frontier_is_authenticated()
        || matches!(
            recovered.stage8b_p1d3_restart_phase(),
            Some(
                Stage6Stage8bP1d3RestartPhase::TruthCommitted
                    | Stage6Stage8bP1d3RestartPhase::SemanticCallbackCommitted
            )
        );
    accepted.o3_readmission = Some(Box::new(permit));
    if later {
        apply_stage8b_p1d3_semantic_transition(
            recovered,
            accepted,
            binding,
            source,
            0.5,
            commitment_key,
        )
    } else {
        apply_stage8b_p1_semantic_transition(recovered, accepted, binding, source, commitment_key)
    }
}
