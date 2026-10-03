//! Pure pre-claim comparison for S0 + one RequestAccepted. The protected
//! journal suffix is NOT a replacement seal. This check admits only source
//! lookup; the existing post-permit semantic replay still has to reproduce
//! the complete candidate before S1/publication can be obtained.

use super::*;
use crate::stage5g_clean_restart::{validate_projection, Stage5gCleanRestartProjectionV1};

pub struct Stage8bP1JournalAheadSourceCheck {
    source: Stage6Stage8bP1SealSourceV1,
    prior_stage5g_checkpoint_sha256: String,
    request_id: StrategyRequestId,
    command_sha256: Stage6Sha256Digest,
    record_id: Stage6JournalRecordId,
    expected_source_evidence: Stage6Sha256Digest,
}

impl Stage6Stage8bP1JournalAheadCandidate {
    /// Inspect authenticated S0 without restoring a runtime or invoking a
    /// callback. The caller supplies the already verified outer seal identity.
    pub fn bind_source_check(
        &self,
        authenticated_s0: &[u8],
        commitment_key: &Stage5gLifecycleCommitmentKey,
        source: Stage6Stage8bP1SealSourceV1,
    ) -> Result<Stage8bP1JournalAheadSourceCheck, Stage6dLiveCoreError> {
        let package = decode_and_authenticate_restart_package(authenticated_s0, commitment_key)?;
        if package.stage6_checkpoint.checkpoint_sha256() != source.stage6_checkpoint_sha256
            || crate::stage6_frontier_fingerprint_sha256(package.stage6_checkpoint.frontier())?
                .as_str()
                != source.stage6_frontier_sha256
            || stage6d_operational_identity_sha256(&package.operational_identity)?.as_str()
                != source.operational_identity_sha256
        {
            return Err(Stage6dLiveCoreError::RestartPackageBindingMismatch);
        }
        let decoded =
            crate::stage5d_persistence::stage5d_decode_canonical_restart_bytes_requiring_stage5g(
                &package.stage5g_restart_package,
            )
            .map_err(Stage5gCleanRestartError::from)?;
        let projection: Stage5gCleanRestartProjectionV1 =
            serde_json::from_str(&decoded.stage5g_extension_json)
                .map_err(|_| Stage6dLiveCoreError::RestartPackageDecode)?;
        validate_projection(&projection)?;
        if projection.lifecycle_kind != crate::Stage5gCleanRestartLifecycleKind::P1SemanticReady {
            return Err(Stage6dLiveCoreError::DurableOrderingViolation);
        }
        let prior_stage5g_checkpoint_sha256 = sha256_hex(
            &serde_json::to_vec(&projection.checkpoint)
                .map_err(|_| Stage6dLiveCoreError::IntegrationFingerprint)?,
        );
        Ok(Stage8bP1JournalAheadSourceCheck {
            source,
            prior_stage5g_checkpoint_sha256,
            request_id: self.identity.strategy_request_id(),
            command_sha256: self
                .command
                .canonical_broker_command_sha256(&self.identity)?,
            record_id: self.record_id.clone(),
            expected_source_evidence: self.source_evidence_sha256.clone(),
        })
    }
}

impl Stage8bP1JournalAheadSourceCheck {
    /// Redis metadata is only a probe. Success does not validate candle bytes
    /// or mint callback/Ready authority; canonical + retained source checks and
    /// post-permit replay remain mandatory.
    pub fn matches(&self, binding: &Stage5gP1SemanticBindingInput) -> bool {
        if binding.operational_identity_sha256 != self.source.operational_identity_sha256 {
            return false;
        }
        let batch = crate::stage5g_p1_semantic::semantic_batch_id_sha256(
            &binding.operational_identity_sha256,
            &binding.m10_semantic_id_sha256,
            &binding.m10_payload_sha256,
            &self.prior_stage5g_checkpoint_sha256,
        );
        stage8b_p1_request_accepted_source_evidence_parts(
            &self.source,
            binding,
            &batch,
            self.request_id,
            self.command_sha256.as_str(),
            &self.record_id,
        )
        .is_ok_and(|actual| actual == self.expected_source_evidence)
    }
}
