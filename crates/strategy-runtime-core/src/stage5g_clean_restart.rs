//! Stage 5G-e-c canonical clean-process reconstruction boundary.
//!
//! Stage 5G contributes a checksummed, versioned extension to the accepted
//! Stage 5D canonical restart package.  Package integrity is authenticated by
//! an operator-managed HMAC key which is supplied out of band and is never
//! serialized into that package.

use broker_core::{BrokerAccountId, InstrumentId};
use chrono::{DateTime, Utc};
use hmac::{Hmac, Mac};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zeroize::Zeroize;

use crate::runtime_compat::Strategy;
use crate::stage5d_persistence::{
    stage5d_bind_stage5g_source_authority_anchor,
    stage5d_decode_canonical_restart_bytes_requiring_stage5g,
    stage5d_export_canonical_envelope_from_runtime,
    stage5d_export_canonical_restart_bytes_from_authenticated_parts,
    stage5d_reconstruct_runtime_from_clean_restart, Stage5dCanonicalEnvelopeExportInput,
    Stage5dCanonicalRestartCheckpointState, Stage5dCleanRestartDecoded,
    STAGE5D_CANONICAL_RESTART_PACKAGE_SCHEMA_VERSION,
};
use crate::stage5g_order_position::{
    stage5g_integral_lot_decimal, Stage5gFreshTruthRestartSlotProjection, Stage5gOrderPositionState,
};
use crate::{
    HybridIntradayRuntimeStrategy, Stage5dEnvelopeValidationError, Stage5dLifecycleWatermarks,
    Stage5dRiskGateLedgerEvidence, Stage5dRiskGatePersistence,
    Stage5gCommittedAwaitingOrderPosition, Stage5gCommittedExactReplaySession,
    Stage5gOrderPositionSession, Stage5gOrderPositionSummary, Stage5gTimerCheckpointEnvelope,
    Stage5gTimerCheckpointError, Stage5gTimerReadyPaperStrategy,
};

pub const STAGE5G_CLEAN_RESTART_EXTENSION_SCHEMA_VERSION: u16 = 1;
const STAGE5G_TIMER_READY_RESTART_PROJECTION_SCHEMA_VERSION: u16 = 1;
const STAGE5G_PACKAGE_INSTANCE_BINDING_SCHEMA_VERSION: u16 = 1;
const STAGE5G_AUTHENTICATED_RESTART_PACKAGE_COMMITMENT_SCHEMA_VERSION: u16 = 1;

pub struct Stage5gLifecycleCommitmentKey([u8; 32]);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage5gLifecycleCommitmentKeyError {
    InvalidLength,
}

impl Stage5gLifecycleCommitmentKey {
    pub fn from_secret_bytes(secret: &[u8]) -> Result<Self, Stage5gLifecycleCommitmentKeyError> {
        let bytes: [u8; 32] = secret
            .try_into()
            .map_err(|_| Stage5gLifecycleCommitmentKeyError::InvalidLength)?;
        Ok(Self(bytes))
    }
}

impl Drop for Stage5gLifecycleCommitmentKey {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

#[derive(Debug, Clone)]
pub struct Stage5gCleanRestartExportInput {
    pub snapshot_id: String,
    pub snapshot_revision: u64,
    pub previous_revision: Option<u64>,
    pub write_generation: u64,
    pub persisted_at_ts_utc: DateTime<Utc>,
    pub source_commit_or_build_id: String,
    pub lifecycle_watermarks: Stage5dLifecycleWatermarks,
    pub riskgate: Stage5dRiskGatePersistence,
    pub riskgate_evidence: Stage5dRiskGateLedgerEvidence,
}

/// The only four Stage 5G lifecycle kinds admitted by the initial e-c slice.
/// Each variant is linear because every contained capability is linear.
pub enum Stage5gCleanRestartSource {
    TimerReady(Stage5gTimerReadyPaperStrategy),
    OrderPositionAwaiting(Stage5gOrderPositionSession),
    ExactReplaySynchronized(Stage5gCommittedExactReplaySession),
    NewPackageAwaiting(Stage5gCommittedAwaitingOrderPosition),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage5gCleanRestartLifecycleKind {
    TimerReady,
    OrderPositionAwaitingCommitted,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Stage5gCleanRestartBindingV1 {
    schema_version: u16,
    strategy_id: String,
    account_id: BrokerAccountId,
    instrument_id: InstrumentId,
    stage5c_config_fingerprint: String,
    stage5d_canonical_config_fingerprint: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Stage5gCleanRestartLifecycleProofV1 {
    pub(crate) schema_version: u16,
    pub(crate) authoritative_callback_count: usize,
    pub(crate) zero_intent_ready: bool,
    pub(crate) source_authority_sha256: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Stage5gTimerReadyRestartProjectionV1 {
    pub(crate) schema_version: u16,
    pub(crate) stage5c_settlement: crate::stage5c_paper_host::Stage5cTimerReadyRestartAuthorityV1,
    pub(crate) authoritative_callback_count: usize,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Stage5gPackageInstanceBindingV1 {
    pub(crate) schema_version: u16,
    pub(crate) snapshot_id: String,
    pub(crate) snapshot_revision: u64,
    pub(crate) previous_revision: Option<u64>,
    pub(crate) write_generation: u64,
    pub(crate) persisted_at_ts_utc: DateTime<Utc>,
    pub(crate) stage5d_payload_checksum_sha256: String,
    pub(crate) stage5d_lifecycle_watermarks_sha256: String,
    pub(crate) lifecycle_source_authority_sha256: String,
    pub(crate) stage5g_lifecycle_checkpoint_sha256: String,
    pub(crate) source_lifecycle_commit_sha256: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Stage5gCleanRestartProjectionV1 {
    pub(crate) schema_version: u16,
    pub(crate) binding: Stage5gCleanRestartBindingV1,
    pub(crate) lifecycle_kind: Stage5gCleanRestartLifecycleKind,
    pub(crate) lifecycle_proof: Stage5gCleanRestartLifecycleProofV1,
    pub(crate) strategy_state_fingerprint_sha256: String,
    pub(crate) summary: Stage5gOrderPositionSummary,
    pub(crate) checkpoint: Stage5gTimerCheckpointEnvelope,
    pub(crate) order_position_state: Option<Stage5gOrderPositionState>,
    pub(crate) timer_ready_source: Option<Stage5gTimerReadyRestartProjectionV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) fresh_truth_application_evidence:
        Option<crate::stage5g_fresh_broker_truth::Stage5gFreshTruthApplicationEvidenceV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) fresh_truth_application_authority_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) fresh_truth_application_authority_hmac_sha256: Option<String>,
    pub(crate) package_instance: Stage5gPackageInstanceBindingV1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage5gCleanRestartError {
    Stage5d(Stage5dEnvelopeValidationError),
    ProjectionDecode,
    UnsupportedProjectionSchema,
    UnsupportedLifecycleKind,
    MissingOrderPositionState,
    UnexpectedOrderPositionState,
    ReplayCheckpoint(Stage5gTimerCheckpointError),
    ReplayProjectionInconsistent,
    StrategyStateFingerprintMismatch,
    BindingMismatch,
    LifecycleProofMismatch,
    CallbackAuthorityMismatch,
    ZeroIntentProofMismatch,
    MissingTimerReadySourceAuthority,
    UnexpectedTimerReadySourceAuthority,
    TimerReadySourceAuthorityMismatch,
    PackageInstanceBindingMismatch,
    SourceLifecycleCommitMismatch,
    Stage5dSourceAuthorityAnchorMismatch,
    AuthenticatedLifecycleCommitmentMismatch,
}

impl From<Stage5dEnvelopeValidationError> for Stage5gCleanRestartError {
    fn from(value: Stage5dEnvelopeValidationError) -> Self {
        Self::Stage5d(value)
    }
}

/// Fresh post-byte-boundary capability. It intentionally has no Clone, Copy,
/// Debug, Display, Default, Serialize or Deserialize implementation.
pub struct Stage5gCleanRestartedCapability {
    runtime: HybridIntradayRuntimeStrategy,
    projection: Stage5gCleanRestartProjectionV1,
    reconciliation_authority: Stage5gValidatedReconciliationAuthority,
    continuation_authority: Stage5gCleanRestartContinuationAuthority,
}

#[derive(Clone)]
struct Stage5gCleanRestartContinuationAuthority {
    snapshot_id: String,
    snapshot_revision: u64,
    write_generation: u64,
    persisted_at_ts_utc: DateTime<Utc>,
    source_commit_or_build_id: String,
    lifecycle_watermarks: Stage5dLifecycleWatermarks,
    riskgate: Stage5dRiskGatePersistence,
    riskgate_evidence: Stage5dRiskGateLedgerEvidence,
}

pub(crate) enum Stage5gValidatedReconciliationAuthority {
    TimerReady {
        summary: Stage5gOrderPositionSummary,
        checkpoint: Stage5gTimerCheckpointEnvelope,
        stage5c_settlement: crate::stage5c_paper_host::Stage5cTimerReadyRestartAuthorityV1,
        source_lifecycle_commit_sha256: String,
    },
    OrderPositionAwaitingCommitted {
        summary: Stage5gOrderPositionSummary,
        checkpoint: Stage5gTimerCheckpointEnvelope,
        state: Stage5gOrderPositionState,
        source_lifecycle_commit_sha256: String,
    },
}

impl Stage5gValidatedReconciliationAuthority {
    fn summary(&self) -> &Stage5gOrderPositionSummary {
        match self {
            Self::TimerReady { summary, .. }
            | Self::OrderPositionAwaitingCommitted { summary, .. } => summary,
        }
    }

    fn checkpoint(&self) -> &Stage5gTimerCheckpointEnvelope {
        match self {
            Self::TimerReady { checkpoint, .. }
            | Self::OrderPositionAwaitingCommitted { checkpoint, .. } => checkpoint,
        }
    }
}

#[allow(dead_code)]
pub(crate) struct Stage5gNextReconciliationObservation {
    pub(crate) strategy_id: String,
    pub(crate) account_id: BrokerAccountId,
    pub(crate) instrument_id: InstrumentId,
    pub(crate) lifecycle_kind: Stage5gCleanRestartLifecycleKind,
    pub(crate) callback_count: usize,
    pub(crate) request_count: usize,
    pub(crate) continuation_checkpoint_ts_utc_ms: Option<i64>,
    pub(crate) source_lifecycle_commit_sha256: String,
    pub(crate) lifecycle_source_authority_sha256: String,
}

/// Immutable, consuming-boundary projection for Stage 5G-e-d-b.  It contains
/// only accepted restart facts and no callback, runtime mutation, persistence
/// or transport handle.
#[derive(Serialize)]
pub(crate) struct Stage5gFreshTruthRestartProjection {
    pub(crate) lifecycle_kind: Stage5gCleanRestartLifecycleKind,
    pub(crate) strategy_id: String,
    pub(crate) account_id: BrokerAccountId,
    pub(crate) instrument_id: InstrumentId,
    pub(crate) config_fingerprint_sha256: String,
    pub(crate) strategy_state_fingerprint_sha256: String,
    pub(crate) reconstructed_runtime_state_fingerprint_sha256: String,
    pub(crate) callback_count: usize,
    pub(crate) request_count: usize,
    pub(crate) terminal_request_count: usize,
    pub(crate) source_lifecycle_commit_sha256: String,
    pub(crate) lifecycle_source_authority_sha256: String,
    pub(crate) checkpoint: Stage5gTimerCheckpointEnvelope,
    pub(crate) committed_position_qty: Decimal,
    pub(crate) committed_position_numeric_authority_is_integral: bool,
    pub(crate) slots: Vec<Stage5gFreshTruthRestartSlotProjection>,
    pub(crate) generated_intent_escrow_fingerprint_sha256: Option<String>,
}

trait Stage5gApplicationProjectionSerializer {
    fn serialize(
        &self,
        projection: &Stage5gCleanRestartProjectionV1,
    ) -> Result<String, Stage5gCleanRestartError>;
}

struct Stage5gSerdeApplicationProjectionSerializer;

impl Stage5gApplicationProjectionSerializer for Stage5gSerdeApplicationProjectionSerializer {
    fn serialize(
        &self,
        projection: &Stage5gCleanRestartProjectionV1,
    ) -> Result<String, Stage5gCleanRestartError> {
        #[cfg(test)]
        crate::stage5g_fresh_broker_truth::stage5g_application_trace_mark(
            crate::stage5g_fresh_broker_truth::Stage5gApplicationTracePhase::SerializerCalled,
        );
        serde_json::to_string(projection).map_err(|_| Stage5gCleanRestartError::ProjectionDecode)
    }
}

#[cfg(test)]
struct Stage5gFailingApplicationProjectionSerializer;

#[cfg(test)]
impl Stage5gApplicationProjectionSerializer for Stage5gFailingApplicationProjectionSerializer {
    fn serialize(
        &self,
        _projection: &Stage5gCleanRestartProjectionV1,
    ) -> Result<String, Stage5gCleanRestartError> {
        crate::stage5g_fresh_broker_truth::stage5g_application_trace_mark(
            crate::stage5g_fresh_broker_truth::Stage5gApplicationTracePhase::SerializerCalled,
        );
        Err(Stage5gCleanRestartError::ProjectionDecode)
    }
}

fn stage5g_serialize_application_projection(
    projection: &Stage5gCleanRestartProjectionV1,
    #[cfg(test)] fail_during_serialization: bool,
) -> Result<String, Stage5gCleanRestartError> {
    #[cfg(test)]
    crate::stage5g_fresh_broker_truth::stage5g_application_trace_mark(
        crate::stage5g_fresh_broker_truth::Stage5gApplicationTracePhase::SerializationStarted,
    );
    #[cfg(test)]
    if fail_during_serialization {
        return Stage5gFailingApplicationProjectionSerializer.serialize(projection);
    }
    Stage5gSerdeApplicationProjectionSerializer.serialize(projection)
}

trait Stage5gRuntimeReconstructionAdapter {
    fn reconstruct(
        &self,
        decoded: Stage5dCleanRestartDecoded,
        fresh_runtime: HybridIntradayRuntimeStrategy,
    ) -> Result<(HybridIntradayRuntimeStrategy, String), Stage5dEnvelopeValidationError>;
}

struct Stage5gStage5dRuntimeReconstructionAdapter;

impl Stage5gRuntimeReconstructionAdapter for Stage5gStage5dRuntimeReconstructionAdapter {
    fn reconstruct(
        &self,
        decoded: Stage5dCleanRestartDecoded,
        fresh_runtime: HybridIntradayRuntimeStrategy,
    ) -> Result<(HybridIntradayRuntimeStrategy, String), Stage5dEnvelopeValidationError> {
        #[cfg(test)]
        crate::stage5g_fresh_broker_truth::stage5g_application_trace_mark(
            crate::stage5g_fresh_broker_truth::Stage5gApplicationTracePhase::RuntimeReconstructionCalled,
        );
        stage5d_reconstruct_runtime_from_clean_restart(decoded, fresh_runtime)
    }
}

#[cfg(test)]
struct Stage5gFailingRuntimeReconstructionAdapter;

#[cfg(test)]
impl Stage5gRuntimeReconstructionAdapter for Stage5gFailingRuntimeReconstructionAdapter {
    fn reconstruct(
        &self,
        _decoded: Stage5dCleanRestartDecoded,
        _fresh_runtime: HybridIntradayRuntimeStrategy,
    ) -> Result<(HybridIntradayRuntimeStrategy, String), Stage5dEnvelopeValidationError> {
        crate::stage5g_fresh_broker_truth::stage5g_application_trace_mark(
            crate::stage5g_fresh_broker_truth::Stage5gApplicationTracePhase::RuntimeReconstructionCalled,
        );
        Err(Stage5dEnvelopeValidationError::SemanticStateInvalid)
    }
}

fn stage5g_reconstruct_runtime_from_clean_restart(
    decoded: Stage5dCleanRestartDecoded,
    fresh_runtime: HybridIntradayRuntimeStrategy,
    #[cfg(test)] fail_during_restore: bool,
) -> Result<(HybridIntradayRuntimeStrategy, String), Stage5gCleanRestartError> {
    #[cfg(test)]
    if fail_during_restore {
        return Stage5gFailingRuntimeReconstructionAdapter
            .reconstruct(decoded, fresh_runtime)
            .map_err(Stage5gCleanRestartError::Stage5d);
    }
    Stage5gStage5dRuntimeReconstructionAdapter
        .reconstruct(decoded, fresh_runtime)
        .map_err(Stage5gCleanRestartError::Stage5d)
}

impl Stage5gCleanRestartedCapability {
    pub fn lifecycle_kind(&self) -> Stage5gCleanRestartLifecycleKind {
        self.projection.lifecycle_kind
    }

    pub fn summary(&self) -> &Stage5gOrderPositionSummary {
        self.reconciliation_authority.summary()
    }

    pub fn checkpoint(&self) -> &Stage5gTimerCheckpointEnvelope {
        self.reconciliation_authority.checkpoint()
    }

    pub fn strategy_state_fingerprint_sha256(&self) -> &str {
        &self.projection.strategy_state_fingerprint_sha256
    }

    pub fn reconstructed_runtime_state_fingerprint_sha256(&self) -> String {
        let state = serde_json::to_value(Strategy::state(&self.runtime))
            .expect("validated reconstructed runtime state must remain serializable");
        crate::stage5c_paper_host::stage5c_semantic_value_fingerprint(&state)
            .expect("validated reconstructed runtime state must remain canonical")
    }

    pub fn intent_sink_attached(&self) -> bool {
        false
    }

    pub fn redis_command_stream_attached(&self) -> bool {
        false
    }

    pub fn finam_transport_attached(&self) -> bool {
        false
    }

    pub fn broker_execution_attached(&self) -> bool {
        false
    }

    #[allow(dead_code)]
    pub(crate) fn into_reconciliation_parts(
        self,
    ) -> (
        HybridIntradayRuntimeStrategy,
        Stage5gValidatedReconciliationAuthority,
    ) {
        (self.runtime, self.reconciliation_authority)
    }

    pub(crate) fn stage5g_restart_order_position_state(&self) -> Option<Stage5gOrderPositionState> {
        match &self.reconciliation_authority {
            Stage5gValidatedReconciliationAuthority::OrderPositionAwaitingCommitted {
                state,
                ..
            } => Some(state.clone()),
            Stage5gValidatedReconciliationAuthority::TimerReady { .. } => None,
        }
    }

    pub(crate) fn stage5g_pre_restart_package_fingerprint_sha256(&self) -> String {
        semantic_sha256(&self.projection)
            .expect("validated clean-restart projection remains canonical")
    }

    pub(crate) fn stage5g_application_parent_snapshot_binding(&self) -> (String, u64) {
        (
            self.continuation_authority.snapshot_id.clone(),
            self.continuation_authority.snapshot_revision,
        )
    }

    pub(crate) fn stage5g_fresh_reconstruction_candidate(&self) -> HybridIntradayRuntimeStrategy {
        self.runtime.stage5g_clean_reconstruction_candidate()
    }

    fn stage5g_post_application_package_fingerprint_sha256(
        &self,
        state: &Stage5gOrderPositionState,
        fresh_package_id: &str,
        fresh_snapshot_epoch: &str,
        candidate_fingerprint_sha256: &str,
    ) -> Result<String, Stage5gCleanRestartError> {
        stage5g_post_application_package_fingerprint_sha256_from_parent(
            state,
            &self.continuation_authority.snapshot_id,
            self.continuation_authority.snapshot_revision,
            fresh_package_id,
            fresh_snapshot_epoch,
            candidate_fingerprint_sha256,
        )
    }

    pub(crate) fn stage5g_export_post_application_order_position(
        &self,
        validated: crate::stage5g_fresh_broker_truth::Stage5gValidatedPostApplication,
        commitment_key: &Stage5gLifecycleCommitmentKey,
    ) -> Result<
        (
            Vec<u8>,
            crate::stage5g_fresh_broker_truth::Stage5gFreshTruthApplicationEvidenceV1,
        ),
        Stage5gCleanRestartError,
    > {
        let parts = validated.into_export_parts();
        #[cfg(test)]
        let fail_during_serialization = parts.fails_at(
            crate::stage5g_fresh_broker_truth::Stage5gFreshTruthApplicationFailurePoint::DuringSerialization,
        );
        #[cfg(test)]
        let fail_after_bytes = parts.fails_at(
            crate::stage5g_fresh_broker_truth::Stage5gFreshTruthApplicationFailurePoint::AfterBytesBeforeSourceDrop,
        );
        #[cfg(test)]
        let fail_after_source_drop = parts.fails_at(
            crate::stage5g_fresh_broker_truth::Stage5gFreshTruthApplicationFailurePoint::AfterSourceDropBeforeDecode,
        );
        if !parts.evidence_matches_owned_tuple() {
            return Err(Stage5gCleanRestartError::LifecycleProofMismatch);
        }
        let post_package_fingerprint = self.stage5g_post_application_package_fingerprint_sha256(
            &parts.state,
            &parts.fresh_package_id,
            &parts.fresh_snapshot_epoch,
            parts.candidate_fingerprint_sha256(),
        )?;
        let finalized = parts.finalize_post_restart_package_fingerprint(post_package_fingerprint);
        let parts = finalized.into_export_parts();
        let application_evidence = parts.evidence;
        if !crate::stage5g_fresh_broker_truth::stage5g_application_evidence_is_valid(
            &application_evidence,
        ) {
            return Err(Stage5gCleanRestartError::LifecycleProofMismatch);
        }
        let application_authority_hmac =
            application_authority_hmac_sha256(commitment_key, &parts.authority_commitment_sha256);
        let snapshot_revision = self
            .continuation_authority
            .snapshot_revision
            .checked_add(1)
            .ok_or(Stage5gCleanRestartError::PackageInstanceBindingMismatch)?;
        let write_generation = self
            .continuation_authority
            .write_generation
            .checked_add(1)
            .ok_or(Stage5gCleanRestartError::PackageInstanceBindingMismatch)?;
        let snapshot_identity = semantic_sha256(&(
            "moex.stage5g.fresh-truth-post-snapshot.v1",
            self.continuation_authority.snapshot_id.as_str(),
            parts.fresh_package_id.as_str(),
            parts.fresh_snapshot_epoch.as_str(),
            application_evidence.candidate_fingerprint_sha256(),
        ))?;
        let persisted_at_ts_utc = parts
            .captured_at
            .max(self.continuation_authority.persisted_at_ts_utc);
        let mut lifecycle_watermarks = self.continuation_authority.lifecycle_watermarks.clone();
        lifecycle_watermarks.last_broker_event_ts = Some(
            lifecycle_watermarks
                .last_broker_event_ts
                .map_or(parts.captured_at, |current| current.max(parts.captured_at)),
        );
        let stage5d_input = Stage5dCanonicalEnvelopeExportInput {
            snapshot_id: format!("stage5g-edc-{}", &snapshot_identity[..32]),
            snapshot_revision,
            previous_revision: Some(self.continuation_authority.snapshot_revision),
            write_generation,
            persisted_at_ts_utc,
            strategy_id: self.projection.binding.strategy_id.clone(),
            account_id: self.projection.binding.account_id.clone(),
            instrument_id: self.projection.binding.instrument_id.clone(),
            source_commit_or_build_id: self
                .continuation_authority
                .source_commit_or_build_id
                .clone(),
            lifecycle_watermarks,
            riskgate: self.continuation_authority.riskgate.clone(),
        };
        let (mut envelope, _) =
            stage5d_export_canonical_envelope_from_runtime(&self.runtime, stage5d_input)?;
        let preliminary_projection = projection_from_order_position_state(
            &self.runtime,
            parts.state.clone(),
            &envelope,
            Some(application_evidence.clone()),
            Some(parts.authority_commitment_sha256.clone()),
            Some(application_authority_hmac.clone()),
        )?;
        let anchor = independent_source_authority_sha256(&preliminary_projection)?;
        envelope.stage5g_source_authority_anchor_sha256 = Some(anchor.clone());
        envelope.stage5g_source_authority_hmac_sha256 = None;
        envelope.payload_checksum_sha256 = envelope.compute_payload_checksum_sha256()?;
        let authenticated_commitment = authenticated_restart_package_commitment_sha256(
            &preliminary_projection,
            &envelope,
            &self.continuation_authority.riskgate_evidence,
            STAGE5D_CANONICAL_RESTART_PACKAGE_SCHEMA_VERSION,
            Stage5dCanonicalRestartCheckpointState::Committed,
        )?;
        let hmac = lifecycle_commitment_hmac_sha256(commitment_key, &authenticated_commitment);
        stage5d_bind_stage5g_source_authority_anchor(&mut envelope, &anchor, &hmac)?;
        let projection = projection_from_order_position_state(
            &self.runtime,
            parts.state,
            &envelope,
            Some(application_evidence.clone()),
            Some(parts.authority_commitment_sha256),
            Some(application_authority_hmac),
        )?;
        if independent_source_authority_sha256(&projection)? != anchor {
            return Err(Stage5gCleanRestartError::Stage5dSourceAuthorityAnchorMismatch);
        }
        let extension_json = stage5g_serialize_application_projection(
            &projection,
            #[cfg(test)]
            fail_during_serialization,
        )?;
        let bytes = stage5d_export_canonical_restart_bytes_from_authenticated_parts(
            &self.runtime,
            envelope,
            self.continuation_authority.riskgate_evidence.clone(),
            extension_json,
        )
        .map_err(Stage5gCleanRestartError::Stage5d)?;
        #[cfg(test)]
        crate::stage5g_fresh_broker_truth::stage5g_application_trace_mark(
            crate::stage5g_fresh_broker_truth::Stage5gApplicationTracePhase::BytesProduced,
        );
        #[cfg(test)]
        if fail_after_bytes {
            return Err(Stage5gCleanRestartError::ProjectionDecode);
        }
        drop(projection);
        #[cfg(test)]
        crate::stage5g_fresh_broker_truth::stage5g_application_trace_mark(
            crate::stage5g_fresh_broker_truth::Stage5gApplicationTracePhase::PostStateConsumed,
        );
        #[cfg(test)]
        if fail_after_source_drop {
            return Err(Stage5gCleanRestartError::ProjectionDecode);
        }
        Ok((bytes, application_evidence))
    }

    pub(crate) fn stage5g_application_evidence(
        &self,
    ) -> Option<&crate::stage5g_fresh_broker_truth::Stage5gFreshTruthApplicationEvidenceV1> {
        self.projection.fresh_truth_application_evidence.as_ref()
    }

    #[allow(dead_code)]
    pub(crate) fn next_reconciliation_observation(&self) -> Stage5gNextReconciliationObservation {
        let summary = self.reconciliation_authority.summary();
        let checkpoint = self.reconciliation_authority.checkpoint();
        let (source_lifecycle_commit_sha256, lifecycle_source_authority_sha256) =
            match &self.reconciliation_authority {
                Stage5gValidatedReconciliationAuthority::TimerReady {
                    stage5c_settlement,
                    source_lifecycle_commit_sha256,
                    ..
                } => (
                    source_lifecycle_commit_sha256.clone(),
                    semantic_sha256(stage5c_settlement)
                        .expect("validated TimerReady source remains serializable"),
                ),
                Stage5gValidatedReconciliationAuthority::OrderPositionAwaitingCommitted {
                    state,
                    source_lifecycle_commit_sha256,
                    ..
                } => (
                    source_lifecycle_commit_sha256.clone(),
                    semantic_sha256(state)
                        .expect("validated order-position source remains serializable"),
                ),
            };
        Stage5gNextReconciliationObservation {
            strategy_id: self.projection.binding.strategy_id.clone(),
            account_id: self.projection.binding.account_id.clone(),
            instrument_id: self.projection.binding.instrument_id.clone(),
            lifecycle_kind: self.projection.lifecycle_kind,
            callback_count: self.projection.lifecycle_proof.authoritative_callback_count,
            request_count: summary.request_count,
            continuation_checkpoint_ts_utc_ms: checkpoint
                .payload
                .last_continuation_checkpoint_ts_utc_ms,
            source_lifecycle_commit_sha256,
            lifecycle_source_authority_sha256,
        }
    }

    pub(crate) fn fresh_truth_reducer_projection(&self) -> Stage5gFreshTruthRestartProjection {
        let observation = self.next_reconciliation_observation();
        let slots = match &self.reconciliation_authority {
            Stage5gValidatedReconciliationAuthority::TimerReady { .. } => Vec::new(),
            Stage5gValidatedReconciliationAuthority::OrderPositionAwaitingCommitted {
                state,
                ..
            } => Stage5gOrderPositionSession::stage5g_fresh_truth_restart_slots(state),
        };
        let generated_intent_escrow_fingerprint_sha256 = slots
            .iter()
            .filter(|slot| {
                slot.target_broker_order_id.is_none()
                    && slot.latest_order.is_none()
                    && slot.trades.is_empty()
                    && slot.position.is_none()
            })
            .collect::<Vec<_>>();
        let generated_intent_escrow_fingerprint_sha256 =
            (!generated_intent_escrow_fingerprint_sha256.is_empty()).then(|| {
                semantic_sha256(&generated_intent_escrow_fingerprint_sha256)
                    .expect("validated generated-intent escrow projection remains serializable")
            });
        Stage5gFreshTruthRestartProjection {
            lifecycle_kind: observation.lifecycle_kind,
            strategy_id: observation.strategy_id,
            account_id: observation.account_id,
            instrument_id: observation.instrument_id,
            config_fingerprint_sha256: self.projection.binding.stage5c_config_fingerprint.clone(),
            strategy_state_fingerprint_sha256: self
                .projection
                .strategy_state_fingerprint_sha256
                .clone(),
            reconstructed_runtime_state_fingerprint_sha256: self
                .reconstructed_runtime_state_fingerprint_sha256(),
            callback_count: observation.callback_count,
            request_count: observation.request_count,
            terminal_request_count: self
                .reconciliation_authority
                .summary()
                .terminal_request_count,
            source_lifecycle_commit_sha256: observation.source_lifecycle_commit_sha256,
            lifecycle_source_authority_sha256: observation.lifecycle_source_authority_sha256,
            checkpoint: self.reconciliation_authority.checkpoint().clone(),
            committed_position_qty: stage5g_integral_lot_decimal(
                self.runtime.stage5c_current_position_qty(),
            )
            .unwrap_or(Decimal::ZERO),
            committed_position_numeric_authority_is_integral: stage5g_integral_lot_decimal(
                self.runtime.stage5c_current_position_qty(),
            )
            .is_some(),
            slots,
            generated_intent_escrow_fingerprint_sha256,
        }
    }
}

pub fn export_stage5g_clean_restart(
    source: Stage5gCleanRestartSource,
    input: Stage5gCleanRestartExportInput,
    commitment_key: &Stage5gLifecycleCommitmentKey,
) -> Result<Vec<u8>, Stage5gCleanRestartError> {
    let strategy = strategy_from_source(&source);
    let (strategy_id, account_id, instrument_id) = source_binding(&source);
    let stage5d_input = Stage5dCanonicalEnvelopeExportInput {
        snapshot_id: input.snapshot_id,
        snapshot_revision: input.snapshot_revision,
        previous_revision: input.previous_revision,
        write_generation: input.write_generation,
        persisted_at_ts_utc: input.persisted_at_ts_utc,
        strategy_id: strategy_id.to_string(),
        account_id: account_id.clone(),
        instrument_id: instrument_id.clone(),
        source_commit_or_build_id: input.source_commit_or_build_id,
        lifecycle_watermarks: input.lifecycle_watermarks,
        riskgate: input.riskgate,
    };
    let (mut stage5d_envelope, _) =
        stage5d_export_canonical_envelope_from_runtime(strategy, stage5d_input.clone())?;
    let preliminary_projection = projection_from_source(&source, &stage5d_envelope)?;
    let stage5g_source_authority_anchor_sha256 =
        independent_source_authority_sha256(&preliminary_projection)?;
    stage5d_envelope.stage5g_source_authority_anchor_sha256 =
        Some(stage5g_source_authority_anchor_sha256.clone());
    stage5d_envelope.stage5g_source_authority_hmac_sha256 = None;
    stage5d_envelope.payload_checksum_sha256 =
        stage5d_envelope.compute_payload_checksum_sha256()?;
    let authenticated_package_commitment_sha256 = authenticated_restart_package_commitment_sha256(
        &preliminary_projection,
        &stage5d_envelope,
        &input.riskgate_evidence,
        STAGE5D_CANONICAL_RESTART_PACKAGE_SCHEMA_VERSION,
        Stage5dCanonicalRestartCheckpointState::Committed,
    )?;
    let stage5g_source_authority_hmac_sha256 =
        lifecycle_commitment_hmac_sha256(commitment_key, &authenticated_package_commitment_sha256);
    stage5d_bind_stage5g_source_authority_anchor(
        &mut stage5d_envelope,
        &stage5g_source_authority_anchor_sha256,
        &stage5g_source_authority_hmac_sha256,
    )?;
    let projection = projection_from_source(&source, &stage5d_envelope)?;
    if independent_source_authority_sha256(&projection)? != stage5g_source_authority_anchor_sha256 {
        return Err(Stage5gCleanRestartError::Stage5dSourceAuthorityAnchorMismatch);
    }
    let extension_json = serde_json::to_string(&projection)
        .map_err(|_| Stage5gCleanRestartError::ProjectionDecode)?;
    let bytes = stage5d_export_canonical_restart_bytes_from_authenticated_parts(
        strategy,
        stage5d_envelope,
        input.riskgate_evidence,
        extension_json,
    )?;
    // The borrow above ends before the owning source is destroyed. No source
    // Stage 5G or Stage 5C capability is returned beside the durable bytes.
    drop(source);
    Ok(bytes)
}

pub fn restore_stage5g_clean_restart(
    bytes: &[u8],
    commitment_key: &Stage5gLifecycleCommitmentKey,
    fresh_runtime: HybridIntradayRuntimeStrategy,
) -> Result<Stage5gCleanRestartedCapability, Stage5gCleanRestartError> {
    restore_stage5g_clean_restart_inner(
        bytes,
        commitment_key,
        fresh_runtime,
        #[cfg(test)]
        None,
    )
}

#[cfg(test)]
pub(crate) fn restore_stage5g_clean_restart_with_failure(
    bytes: &[u8],
    commitment_key: &Stage5gLifecycleCommitmentKey,
    fresh_runtime: HybridIntradayRuntimeStrategy,
    failure_point: Option<
        crate::stage5g_fresh_broker_truth::Stage5gFreshTruthApplicationFailurePoint,
    >,
) -> Result<Stage5gCleanRestartedCapability, Stage5gCleanRestartError> {
    restore_stage5g_clean_restart_inner(bytes, commitment_key, fresh_runtime, failure_point)
}

fn restore_stage5g_clean_restart_inner(
    bytes: &[u8],
    commitment_key: &Stage5gLifecycleCommitmentKey,
    fresh_runtime: HybridIntradayRuntimeStrategy,
    #[cfg(test)] failure_point: Option<
        crate::stage5g_fresh_broker_truth::Stage5gFreshTruthApplicationFailurePoint,
    >,
) -> Result<Stage5gCleanRestartedCapability, Stage5gCleanRestartError> {
    #[cfg(test)]
    crate::stage5g_fresh_broker_truth::stage5g_application_trace_mark(
        crate::stage5g_fresh_broker_truth::Stage5gApplicationTracePhase::DecodeStarted,
    );
    let decoded = stage5d_decode_canonical_restart_bytes_requiring_stage5g(bytes)?;
    let projection: Stage5gCleanRestartProjectionV1 =
        serde_json::from_str(&decoded.stage5g_extension_json)
            .map_err(|_| Stage5gCleanRestartError::ProjectionDecode)?;
    validate_projection(&projection)?;
    #[cfg(test)]
    crate::stage5g_fresh_broker_truth::stage5g_application_trace_mark(
        crate::stage5g_fresh_broker_truth::Stage5gApplicationTracePhase::AuthenticationStarted,
    );
    #[cfg(test)]
    let injected_authentication_key;
    #[cfg(test)]
    let verification_key = if failure_point
        == Some(
            crate::stage5g_fresh_broker_truth::Stage5gFreshTruthApplicationFailurePoint::DuringAuthenticationVerification,
        )
    {
        injected_authentication_key = Stage5gLifecycleCommitmentKey::from_secret_bytes(&[0xa7; 32])
            .expect("test-only distinct authentication key");
        &injected_authentication_key
    } else {
        commitment_key
    };
    #[cfg(not(test))]
    let verification_key = commitment_key;
    validate_projection_binding(
        &projection,
        &decoded.envelope,
        decoded.validated_evidence.evidence(),
        decoded.package_schema_version,
        decoded.checkpoint_state,
        verification_key,
        &fresh_runtime,
    )?;
    #[cfg(test)]
    crate::stage5g_fresh_broker_truth::stage5g_application_trace_mark(
        crate::stage5g_fresh_broker_truth::Stage5gApplicationTracePhase::AuthenticationCompleted,
    );
    let continuation_authority = Stage5gCleanRestartContinuationAuthority {
        snapshot_id: decoded.envelope.snapshot_id.clone(),
        snapshot_revision: decoded.envelope.snapshot_revision,
        write_generation: decoded.envelope.write_generation,
        persisted_at_ts_utc: decoded.envelope.persisted_at_ts_utc,
        source_commit_or_build_id: decoded.envelope.binding.source_commit_or_build_id.clone(),
        lifecycle_watermarks: decoded.envelope.lifecycle_watermarks.clone(),
        riskgate: decoded.envelope.riskgate.clone(),
        riskgate_evidence: decoded.validated_evidence.evidence().clone(),
    };
    let reconciliation_authority = validated_reconciliation_authority(&projection)?;
    #[cfg(test)]
    crate::stage5g_fresh_broker_truth::stage5g_application_trace_mark(
        crate::stage5g_fresh_broker_truth::Stage5gApplicationTracePhase::RestoreStarted,
    );
    #[cfg(test)]
    let (runtime, _extension_json) = stage5g_reconstruct_runtime_from_clean_restart(
        decoded,
        fresh_runtime,
        failure_point
            == Some(
                crate::stage5g_fresh_broker_truth::Stage5gFreshTruthApplicationFailurePoint::DuringRestore,
            ),
    )?;
    #[cfg(not(test))]
    let (runtime, _extension_json) =
        stage5g_reconstruct_runtime_from_clean_restart(decoded, fresh_runtime)?;
    let restored_state = serde_json::to_value(Strategy::state(&runtime))
        .map_err(|_| Stage5gCleanRestartError::StrategyStateFingerprintMismatch)?;
    let restored_fingerprint =
        crate::stage5c_paper_host::stage5c_semantic_value_fingerprint(&restored_state)
            .map_err(|_| Stage5gCleanRestartError::StrategyStateFingerprintMismatch)?;
    if restored_fingerprint != projection.strategy_state_fingerprint_sha256 {
        return Err(Stage5gCleanRestartError::StrategyStateFingerprintMismatch);
    }
    let restored = Stage5gCleanRestartedCapability {
        runtime,
        projection,
        reconciliation_authority,
        continuation_authority,
    };
    #[cfg(test)]
    crate::stage5g_fresh_broker_truth::stage5g_application_trace_mark(
        crate::stage5g_fresh_broker_truth::Stage5gApplicationTracePhase::RestoreCompleted,
    );
    Ok(restored)
}

fn strategy_from_source(source: &Stage5gCleanRestartSource) -> &HybridIntradayRuntimeStrategy {
    match source {
        Stage5gCleanRestartSource::TimerReady(value) => value.stage5g_runtime_strategy(),
        Stage5gCleanRestartSource::OrderPositionAwaiting(value) => value.stage5g_runtime_strategy(),
        Stage5gCleanRestartSource::ExactReplaySynchronized(value) => {
            value.stage5g_runtime_strategy()
        }
        Stage5gCleanRestartSource::NewPackageAwaiting(value) => value.stage5g_runtime_strategy(),
    }
}

fn source_binding(source: &Stage5gCleanRestartSource) -> (&str, &BrokerAccountId, &InstrumentId) {
    match source {
        Stage5gCleanRestartSource::TimerReady(value) => value.stage5g_restart_binding(),
        Stage5gCleanRestartSource::OrderPositionAwaiting(value) => value.stage5g_restart_binding(),
        Stage5gCleanRestartSource::ExactReplaySynchronized(value) => {
            value.session().stage5g_restart_binding()
        }
        Stage5gCleanRestartSource::NewPackageAwaiting(value) => {
            value.session().stage5g_restart_binding()
        }
    }
}

#[cfg(test)]
pub(crate) fn stage5g_test_persistence_authority_from_source(
    source: &Stage5gCleanRestartSource,
    persisted_at: DateTime<Utc>,
) -> (Stage5dRiskGatePersistence, Stage5dRiskGateLedgerEvidence) {
    let (strategy_id, _, _) = source_binding(source);
    crate::stage5d_persistence::stage5f_test_seams::stage5g_clean_restart_test_authority(
        strategy_from_source(source),
        strategy_id,
        persisted_at,
    )
}

#[cfg(test)]
pub(crate) fn stage5g_test_projection_from_source(
    source: &Stage5gCleanRestartSource,
) -> Result<Stage5gCleanRestartProjectionV1, Stage5gCleanRestartError> {
    let persisted_at = Utc::now();
    let (riskgate, _) = stage5g_test_persistence_authority_from_source(source, persisted_at);
    let (strategy_id, account_id, instrument_id) = source_binding(source);
    let input = Stage5dCanonicalEnvelopeExportInput {
        snapshot_id: "stage5g-ec-r2-unit-projection".to_string(),
        snapshot_revision: 1,
        previous_revision: None,
        write_generation: 1,
        persisted_at_ts_utc: persisted_at,
        strategy_id: strategy_id.to_string(),
        account_id: account_id.clone(),
        instrument_id: instrument_id.clone(),
        source_commit_or_build_id:
            crate::stage5d_persistence::STAGE5D_RUNTIME_SEMANTIC_COMPATIBILITY_ID.to_string(),
        lifecycle_watermarks: Stage5dLifecycleWatermarks {
            persisted_event_watermark: None,
            last_semantic_bar_ts: None,
            last_broker_event_ts: None,
        },
        riskgate,
    };
    let (envelope, _) =
        stage5d_export_canonical_envelope_from_runtime(strategy_from_source(source), input)?;
    projection_from_source(source, &envelope)
}

pub(crate) fn projection_from_source(
    source: &Stage5gCleanRestartSource,
    stage5d_envelope: &crate::Stage5dPersistenceEnvelope,
) -> Result<Stage5gCleanRestartProjectionV1, Stage5gCleanRestartError> {
    let strategy_state = serde_json::to_value(Strategy::state(strategy_from_source(source)))
        .map_err(|_| Stage5gCleanRestartError::StrategyStateFingerprintMismatch)?;
    let strategy_state_fingerprint_sha256 =
        crate::stage5c_paper_host::stage5c_semantic_value_fingerprint(&strategy_state)
            .map_err(|_| Stage5gCleanRestartError::StrategyStateFingerprintMismatch)?;
    let strategy = strategy_from_source(source);
    let (strategy_id, account_id, instrument_id) = source_binding(source);
    let binding = Stage5gCleanRestartBindingV1 {
        schema_version: 1,
        strategy_id: strategy_id.to_string(),
        account_id: account_id.clone(),
        instrument_id: instrument_id.clone(),
        stage5c_config_fingerprint: strategy.stage5c_config_fingerprint(),
        stage5d_canonical_config_fingerprint: strategy.stage5d_canonical_config_fingerprint(),
    };
    let (
        lifecycle_kind,
        callback_count,
        zero_intent_ready,
        summary,
        checkpoint,
        order_position_state,
        timer_ready_source,
    ) = match source {
        Stage5gCleanRestartSource::TimerReady(value) => {
            let checkpoint = value.checkpoint();
            let summary = value.summary().clone();
            let timer_ready_source = Stage5gTimerReadyRestartProjectionV1 {
                schema_version: STAGE5G_TIMER_READY_RESTART_PROJECTION_SCHEMA_VERSION,
                stage5c_settlement: value
                    .stage5g_restart_stage5c_authority()
                    .ok_or(Stage5gCleanRestartError::MissingTimerReadySourceAuthority)?,
                authoritative_callback_count: 1,
            };
            (
                Stage5gCleanRestartLifecycleKind::TimerReady,
                1,
                value.stage5g_restart_is_zero_intent_ready(),
                summary,
                checkpoint,
                None,
                Some(timer_ready_source),
            )
        }
        Stage5gCleanRestartSource::OrderPositionAwaiting(value) => (
            Stage5gCleanRestartLifecycleKind::OrderPositionAwaitingCommitted,
            0,
            false,
            value.summary(),
            value.stage5g_restart_checkpoint(),
            Some(value.stage5g_restart_state()),
            None,
        ),
        Stage5gCleanRestartSource::ExactReplaySynchronized(value) => (
            Stage5gCleanRestartLifecycleKind::OrderPositionAwaitingCommitted,
            0,
            false,
            value.session().summary(),
            value.checkpoint().clone(),
            Some(value.session().stage5g_restart_state()),
            None,
        ),
        Stage5gCleanRestartSource::NewPackageAwaiting(value) => (
            Stage5gCleanRestartLifecycleKind::OrderPositionAwaitingCommitted,
            0,
            false,
            value.session().summary(),
            value.checkpoint().clone(),
            Some(value.session().stage5g_restart_state()),
            None,
        ),
    };
    let mut projection = Stage5gCleanRestartProjectionV1 {
        schema_version: STAGE5G_CLEAN_RESTART_EXTENSION_SCHEMA_VERSION,
        binding,
        lifecycle_kind,
        lifecycle_proof: Stage5gCleanRestartLifecycleProofV1 {
            schema_version: 1,
            authoritative_callback_count: callback_count,
            zero_intent_ready,
            source_authority_sha256: String::new(),
        },
        strategy_state_fingerprint_sha256,
        summary,
        checkpoint,
        order_position_state,
        timer_ready_source,
        fresh_truth_application_evidence: None,
        fresh_truth_application_authority_sha256: None,
        fresh_truth_application_authority_hmac_sha256: None,
        package_instance: Stage5gPackageInstanceBindingV1 {
            schema_version: STAGE5G_PACKAGE_INSTANCE_BINDING_SCHEMA_VERSION,
            snapshot_id: stage5d_envelope.snapshot_id.clone(),
            snapshot_revision: stage5d_envelope.snapshot_revision,
            previous_revision: stage5d_envelope.previous_revision,
            write_generation: stage5d_envelope.write_generation,
            persisted_at_ts_utc: stage5d_envelope.persisted_at_ts_utc,
            stage5d_payload_checksum_sha256: stage5d_envelope.payload_checksum_sha256.clone(),
            stage5d_lifecycle_watermarks_sha256: semantic_sha256(
                &stage5d_envelope.lifecycle_watermarks,
            )?,
            lifecycle_source_authority_sha256: String::new(),
            stage5g_lifecycle_checkpoint_sha256: String::new(),
            source_lifecycle_commit_sha256: String::new(),
        },
    };
    projection
        .package_instance
        .lifecycle_source_authority_sha256 = lifecycle_source_authority_sha256(&projection)?;
    projection
        .package_instance
        .stage5g_lifecycle_checkpoint_sha256 = lifecycle_checkpoint_sha256(&projection)?;
    projection.package_instance.source_lifecycle_commit_sha256 =
        source_lifecycle_commit_sha256(&projection)?;
    projection.lifecycle_proof.source_authority_sha256 = lifecycle_authority_sha256(&projection)?;
    validate_projection(&projection)?;
    Ok(projection)
}

fn projection_from_order_position_state(
    strategy: &HybridIntradayRuntimeStrategy,
    state: Stage5gOrderPositionState,
    stage5d_envelope: &crate::Stage5dPersistenceEnvelope,
    application_evidence: Option<
        crate::stage5g_fresh_broker_truth::Stage5gFreshTruthApplicationEvidenceV1,
    >,
    application_authority_sha256: Option<String>,
    application_authority_hmac_sha256: Option<String>,
) -> Result<Stage5gCleanRestartProjectionV1, Stage5gCleanRestartError> {
    let strategy_state = serde_json::to_value(Strategy::state(strategy))
        .map_err(|_| Stage5gCleanRestartError::StrategyStateFingerprintMismatch)?;
    let strategy_state_fingerprint_sha256 =
        crate::stage5c_paper_host::stage5c_semantic_value_fingerprint(&strategy_state)
            .map_err(|_| Stage5gCleanRestartError::StrategyStateFingerprintMismatch)?;
    let (strategy_id, account_id, instrument_id) =
        Stage5gOrderPositionSession::stage5g_restart_state_binding(&state);
    let binding = Stage5gCleanRestartBindingV1 {
        schema_version: 1,
        strategy_id: strategy_id.to_string(),
        account_id: account_id.clone(),
        instrument_id: instrument_id.clone(),
        stage5c_config_fingerprint: strategy.stage5c_config_fingerprint(),
        stage5d_canonical_config_fingerprint: strategy.stage5d_canonical_config_fingerprint(),
    };
    let summary = Stage5gOrderPositionSession::stage5g_restart_summary_from_state(&state, 0);
    let checkpoint = Stage5gOrderPositionSession::stage5g_restart_checkpoint_from_state(&state);
    let mut projection = Stage5gCleanRestartProjectionV1 {
        schema_version: STAGE5G_CLEAN_RESTART_EXTENSION_SCHEMA_VERSION,
        binding,
        lifecycle_kind: Stage5gCleanRestartLifecycleKind::OrderPositionAwaitingCommitted,
        lifecycle_proof: Stage5gCleanRestartLifecycleProofV1 {
            schema_version: 1,
            authoritative_callback_count: 0,
            zero_intent_ready: false,
            source_authority_sha256: String::new(),
        },
        strategy_state_fingerprint_sha256,
        summary,
        checkpoint,
        order_position_state: Some(state),
        timer_ready_source: None,
        fresh_truth_application_evidence: application_evidence,
        fresh_truth_application_authority_sha256: application_authority_sha256,
        fresh_truth_application_authority_hmac_sha256: application_authority_hmac_sha256,
        package_instance: Stage5gPackageInstanceBindingV1 {
            schema_version: STAGE5G_PACKAGE_INSTANCE_BINDING_SCHEMA_VERSION,
            snapshot_id: stage5d_envelope.snapshot_id.clone(),
            snapshot_revision: stage5d_envelope.snapshot_revision,
            previous_revision: stage5d_envelope.previous_revision,
            write_generation: stage5d_envelope.write_generation,
            persisted_at_ts_utc: stage5d_envelope.persisted_at_ts_utc,
            stage5d_payload_checksum_sha256: stage5d_envelope.payload_checksum_sha256.clone(),
            stage5d_lifecycle_watermarks_sha256: semantic_sha256(
                &stage5d_envelope.lifecycle_watermarks,
            )?,
            lifecycle_source_authority_sha256: String::new(),
            stage5g_lifecycle_checkpoint_sha256: String::new(),
            source_lifecycle_commit_sha256: String::new(),
        },
    };
    projection
        .package_instance
        .lifecycle_source_authority_sha256 = lifecycle_source_authority_sha256(&projection)?;
    projection
        .package_instance
        .stage5g_lifecycle_checkpoint_sha256 = lifecycle_checkpoint_sha256(&projection)?;
    projection.package_instance.source_lifecycle_commit_sha256 =
        source_lifecycle_commit_sha256(&projection)?;
    projection.lifecycle_proof.source_authority_sha256 = lifecycle_authority_sha256(&projection)?;
    validate_projection(&projection)?;
    Ok(projection)
}

pub(crate) fn validate_projection(
    projection: &Stage5gCleanRestartProjectionV1,
) -> Result<(), Stage5gCleanRestartError> {
    if projection.schema_version != STAGE5G_CLEAN_RESTART_EXTENSION_SCHEMA_VERSION {
        return Err(Stage5gCleanRestartError::UnsupportedProjectionSchema);
    }
    if projection.binding.schema_version != 1
        || projection.binding.strategy_id.is_empty()
        || projection.binding.account_id.as_str().is_empty()
        || projection.binding.instrument_id.symbol.is_empty()
        || projection.binding.stage5c_config_fingerprint.is_empty()
        || projection
            .binding
            .stage5d_canonical_config_fingerprint
            .is_empty()
    {
        return Err(Stage5gCleanRestartError::BindingMismatch);
    }
    match (
        projection.fresh_truth_application_evidence.as_ref(),
        projection
            .fresh_truth_application_authority_sha256
            .as_deref(),
        projection
            .fresh_truth_application_authority_hmac_sha256
            .as_deref(),
        projection.order_position_state.as_ref(),
    ) {
        (None, None, None, _) => {}
        (Some(evidence), Some(authority), Some(authority_hmac), Some(state))
            if crate::stage5g_fresh_broker_truth::stage5g_application_evidence_is_valid(evidence)
                && crate::stage5g_fresh_broker_truth::stage5g_application_authority_sha256(evidence)
                    == authority
                && is_sha256_hex(authority_hmac)
                && crate::stage5g_fresh_broker_truth::stage5g_application_evidence_matches_state(
                    evidence, state,
                )
                && stage5g_application_post_package_fingerprint_matches_projection(
                    evidence, state,
                )? => {}
        _ => return Err(Stage5gCleanRestartError::LifecycleProofMismatch),
    }
    validate_package_instance_internal(projection)?;
    if projection.lifecycle_proof.schema_version != 1
        || projection.lifecycle_proof.source_authority_sha256
            != lifecycle_authority_sha256(projection)?
    {
        return Err(Stage5gCleanRestartError::LifecycleProofMismatch);
    }
    crate::validate_stage5g_timer_checkpoint(&projection.checkpoint)
        .map_err(Stage5gCleanRestartError::ReplayCheckpoint)?;
    match (
        projection.lifecycle_kind,
        projection.order_position_state.as_ref(),
    ) {
        (Stage5gCleanRestartLifecycleKind::TimerReady, None) => {
            if projection.lifecycle_proof.authoritative_callback_count != 1
                || projection.summary.stage5c_callback_count != 1
            {
                return Err(Stage5gCleanRestartError::CallbackAuthorityMismatch);
            }
            if !projection.lifecycle_proof.zero_intent_ready {
                return Err(Stage5gCleanRestartError::ZeroIntentProofMismatch);
            }
            validate_common_projection_binding(projection)?;
            validate_timer_ready_source_authority(projection)
        }
        (Stage5gCleanRestartLifecycleKind::TimerReady, Some(_)) => {
            Err(Stage5gCleanRestartError::UnexpectedOrderPositionState)
        }
        (Stage5gCleanRestartLifecycleKind::OrderPositionAwaitingCommitted, None) => {
            Err(Stage5gCleanRestartError::MissingOrderPositionState)
        }
        (Stage5gCleanRestartLifecycleKind::OrderPositionAwaitingCommitted, Some(state)) => {
            if projection.lifecycle_proof.authoritative_callback_count != 0
                || projection.summary.stage5c_callback_count != 0
            {
                return Err(Stage5gCleanRestartError::CallbackAuthorityMismatch);
            }
            if projection.lifecycle_proof.zero_intent_ready {
                return Err(Stage5gCleanRestartError::ZeroIntentProofMismatch);
            }
            if projection.timer_ready_source.is_some() {
                return Err(Stage5gCleanRestartError::UnexpectedTimerReadySourceAuthority);
            }
            validate_common_projection_binding(projection)?;
            if Stage5gOrderPositionSession::stage5g_restart_projection_is_coherent(
                state,
                &projection.summary,
                &projection.checkpoint,
                projection.lifecycle_proof.authoritative_callback_count,
            ) {
                Ok(())
            } else {
                Err(Stage5gCleanRestartError::ReplayProjectionInconsistent)
            }
        }
    }
}

fn validate_common_projection_binding(
    projection: &Stage5gCleanRestartProjectionV1,
) -> Result<(), Stage5gCleanRestartError> {
    if projection.summary.strategy_id != projection.binding.strategy_id {
        return Err(Stage5gCleanRestartError::BindingMismatch);
    }
    if let Some(state) = projection.order_position_state.as_ref() {
        let (strategy_id, account_id, instrument_id) =
            Stage5gOrderPositionSession::stage5g_restart_state_binding(state);
        if strategy_id != projection.binding.strategy_id
            || account_id != &projection.binding.account_id
            || instrument_id != &projection.binding.instrument_id
        {
            return Err(Stage5gCleanRestartError::BindingMismatch);
        }
    }
    Ok(())
}

fn stage5g_application_post_package_fingerprint_matches_projection(
    evidence: &crate::stage5g_fresh_broker_truth::Stage5gFreshTruthApplicationEvidenceV1,
    state: &Stage5gOrderPositionState,
) -> Result<bool, Stage5gCleanRestartError> {
    let expected = stage5g_post_application_package_fingerprint_sha256_from_parent(
        state,
        evidence.parent_snapshot_id(),
        evidence.parent_snapshot_revision(),
        evidence.fresh_package_id(),
        evidence.fresh_snapshot_epoch(),
        evidence.candidate_fingerprint_sha256(),
    )?;
    Ok(expected == evidence.post_restart_package_fingerprint_sha256())
}

fn validate_summary_checkpoint_projection(
    projection: &Stage5gCleanRestartProjectionV1,
) -> Result<(), Stage5gCleanRestartError> {
    let payload = &projection.checkpoint.payload;
    if projection.summary.duplicate_evidence_count != payload.duplicate_evidence_count
        || projection.summary.last_total_sequence != payload.last_total_sequence
        || !projection.summary.mock_feedback_only
        || projection.summary.redis_attached
        || projection.summary.finam_transport_attached
        || projection.summary.broker_execution_attached
    {
        return Err(Stage5gCleanRestartError::ReplayProjectionInconsistent);
    }
    Ok(())
}

fn validate_timer_ready_source_authority(
    projection: &Stage5gCleanRestartProjectionV1,
) -> Result<(), Stage5gCleanRestartError> {
    let source = projection
        .timer_ready_source
        .as_ref()
        .ok_or(Stage5gCleanRestartError::MissingTimerReadySourceAuthority)?;
    let settlement = &source.stage5c_settlement;
    let outer_checkpoint = projection
        .checkpoint
        .payload
        .last_continuation_checkpoint_ts_utc_ms
        .ok_or(Stage5gCleanRestartError::TimerReadySourceAuthorityMismatch)?;
    if source.schema_version != STAGE5G_TIMER_READY_RESTART_PROJECTION_SCHEMA_VERSION
        || settlement.schema_version
            != crate::stage5c_paper_host::STAGE5C_TIMER_READY_RESTART_AUTHORITY_SCHEMA_VERSION
        || settlement.settlement_kind != "ready_for_continuation"
        || settlement.settled_batch.intent_count != 0
        || !settlement.settled_batch.request_ids.is_empty()
        || settlement.settled_batch.strategy_id != projection.binding.strategy_id
        || settlement.settled_batch.account_id != projection.binding.account_id
        || settlement.settled_batch.instrument != projection.binding.instrument_id
        || !is_sha256_hex(&settlement.recovery_receipt_identity_sha256)
        || settlement.settled_batch.state_fingerprint.is_empty()
        || settlement.settled_batch_history.is_empty()
        || settlement.settled_batch_history.last() != Some(&settlement.settled_batch)
        || !settlement.settled_batch_history.iter().all(|batch| {
            batch.strategy_id == projection.binding.strategy_id
                && batch.account_id == projection.binding.account_id
                && batch.instrument == projection.binding.instrument_id
                && !batch.state_fingerprint.is_empty()
                && batch.intent_count == batch.request_ids.len()
                && batch.min_source_event_ts <= batch.max_source_event_ts
        })
        || !settlement
            .settled_batch_history
            .windows(2)
            .all(|pair| pair[0].bar_close_ts <= pair[1].bar_close_ts)
        || source.authoritative_callback_count != 1
        || projection.summary.stage5c_callback_count != 1
        || settlement.recovery_receipt.schema_version != 1
        || settlement.recovery_receipt_identity_sha256
            != crate::stage5c_paper_host::stage5c_recovery_receipt_projection_sha256(
                &settlement.recovery_receipt,
            )
        || outer_checkpoint < settlement.checkpoint_ts_utc_ms
    {
        return Err(Stage5gCleanRestartError::TimerReadySourceAuthorityMismatch);
    }
    validate_summary_checkpoint_projection(projection)
}

fn is_sha256_hex(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn validate_package_instance_internal(
    projection: &Stage5gCleanRestartProjectionV1,
) -> Result<(), Stage5gCleanRestartError> {
    let binding = &projection.package_instance;
    if binding.schema_version != STAGE5G_PACKAGE_INSTANCE_BINDING_SCHEMA_VERSION
        || binding.snapshot_id.is_empty()
        || binding.snapshot_revision == 0
        || binding.write_generation == 0
        || binding.stage5d_payload_checksum_sha256.len() != 64
        || binding.stage5d_lifecycle_watermarks_sha256.len() != 64
        || binding.source_lifecycle_commit_sha256.len() != 64
        || binding.lifecycle_source_authority_sha256
            != lifecycle_source_authority_sha256(projection)?
        || binding.stage5g_lifecycle_checkpoint_sha256 != lifecycle_checkpoint_sha256(projection)?
    {
        return Err(Stage5gCleanRestartError::PackageInstanceBindingMismatch);
    }
    if binding.source_lifecycle_commit_sha256 != source_lifecycle_commit_sha256(projection)? {
        return Err(Stage5gCleanRestartError::SourceLifecycleCommitMismatch);
    }
    Ok(())
}

fn validate_projection_binding(
    projection: &Stage5gCleanRestartProjectionV1,
    envelope: &crate::Stage5dPersistenceEnvelope,
    riskgate_evidence: &Stage5dRiskGateLedgerEvidence,
    package_schema_version: u16,
    checkpoint_state: Stage5dCanonicalRestartCheckpointState,
    commitment_key: &Stage5gLifecycleCommitmentKey,
    fresh_runtime: &HybridIntradayRuntimeStrategy,
) -> Result<(), Stage5gCleanRestartError> {
    let stage5d_source_anchor = envelope
        .stage5g_source_authority_anchor_sha256
        .as_deref()
        .ok_or(Stage5gCleanRestartError::Stage5dSourceAuthorityAnchorMismatch)?;
    if !is_sha256_hex(stage5d_source_anchor)
        || stage5d_source_anchor != independent_source_authority_sha256(projection)?
    {
        return Err(Stage5gCleanRestartError::Stage5dSourceAuthorityAnchorMismatch);
    }
    let authenticated_commitment = envelope
        .stage5g_source_authority_hmac_sha256
        .as_deref()
        .ok_or(Stage5gCleanRestartError::AuthenticatedLifecycleCommitmentMismatch)?;
    let authenticated_package_commitment_sha256 = authenticated_restart_package_commitment_sha256(
        projection,
        envelope,
        riskgate_evidence,
        package_schema_version,
        checkpoint_state,
    )?;
    if !verify_lifecycle_commitment_hmac(
        commitment_key,
        &authenticated_package_commitment_sha256,
        authenticated_commitment,
    ) {
        return Err(Stage5gCleanRestartError::AuthenticatedLifecycleCommitmentMismatch);
    }
    match (
        projection
            .fresh_truth_application_authority_sha256
            .as_deref(),
        projection
            .fresh_truth_application_authority_hmac_sha256
            .as_deref(),
    ) {
        (None, None) => {}
        (Some(authority), Some(authority_hmac))
            if verify_application_authority_hmac(commitment_key, authority, authority_hmac) => {}
        _ => return Err(Stage5gCleanRestartError::AuthenticatedLifecycleCommitmentMismatch),
    }
    let binding = &projection.binding;
    if binding.strategy_id != envelope.binding.strategy_id
        || binding.account_id != envelope.binding.account_id
        || binding.instrument_id != envelope.binding.instrument_id.to_instrument_id()
        || binding.stage5c_config_fingerprint != envelope.binding.stage5c_compat_config_fingerprint
        || binding.stage5d_canonical_config_fingerprint
            != envelope.binding.stage5d_canonical_config_fingerprint
        || binding.stage5c_config_fingerprint != fresh_runtime.stage5c_config_fingerprint()
        || binding.stage5d_canonical_config_fingerprint
            != fresh_runtime.stage5d_canonical_config_fingerprint()
        || envelope.riskgate.identity.strategy_id != binding.strategy_id
    {
        return Err(Stage5gCleanRestartError::BindingMismatch);
    }
    let instance = &projection.package_instance;
    if instance.snapshot_id != envelope.snapshot_id
        || instance.snapshot_revision != envelope.snapshot_revision
        || instance.previous_revision != envelope.previous_revision
        || instance.write_generation != envelope.write_generation
        || instance.persisted_at_ts_utc != envelope.persisted_at_ts_utc
        || instance.stage5d_payload_checksum_sha256 != envelope.payload_checksum_sha256
        || instance.stage5d_lifecycle_watermarks_sha256
            != semantic_sha256(&envelope.lifecycle_watermarks)?
    {
        return Err(Stage5gCleanRestartError::PackageInstanceBindingMismatch);
    }
    Ok(())
}

fn validated_reconciliation_authority(
    projection: &Stage5gCleanRestartProjectionV1,
) -> Result<Stage5gValidatedReconciliationAuthority, Stage5gCleanRestartError> {
    match projection.lifecycle_kind {
        Stage5gCleanRestartLifecycleKind::TimerReady => {
            let source = projection
                .timer_ready_source
                .as_ref()
                .ok_or(Stage5gCleanRestartError::MissingTimerReadySourceAuthority)?;
            Ok(Stage5gValidatedReconciliationAuthority::TimerReady {
                summary: projection.summary.clone(),
                checkpoint: projection.checkpoint.clone(),
                stage5c_settlement: source.stage5c_settlement.clone(),
                source_lifecycle_commit_sha256: projection
                    .package_instance
                    .source_lifecycle_commit_sha256
                    .clone(),
            })
        }
        Stage5gCleanRestartLifecycleKind::OrderPositionAwaitingCommitted => {
            let state = projection
                .order_position_state
                .as_ref()
                .ok_or(Stage5gCleanRestartError::MissingOrderPositionState)?;
            Ok(
                Stage5gValidatedReconciliationAuthority::OrderPositionAwaitingCommitted {
                    summary: Stage5gOrderPositionSession::stage5g_restart_summary_from_state(
                        state, 0,
                    ),
                    checkpoint: Stage5gOrderPositionSession::stage5g_restart_checkpoint_from_state(
                        state,
                    ),
                    state: state.clone(),
                    source_lifecycle_commit_sha256: projection
                        .package_instance
                        .source_lifecycle_commit_sha256
                        .clone(),
                },
            )
        }
    }
}

fn semantic_sha256<T: Serialize>(value: &T) -> Result<String, Stage5gCleanRestartError> {
    let bytes =
        serde_json::to_vec(value).map_err(|_| Stage5gCleanRestartError::ProjectionDecode)?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

fn stage5g_post_application_package_fingerprint_sha256_from_parent(
    state: &Stage5gOrderPositionState,
    parent_snapshot_id: &str,
    parent_snapshot_revision: u64,
    fresh_package_id: &str,
    fresh_snapshot_epoch: &str,
    candidate_fingerprint_sha256: &str,
) -> Result<String, Stage5gCleanRestartError> {
    #[derive(Serialize)]
    struct PostPackageProjection<'a> {
        domain: &'static str,
        parent_snapshot_id: &'a str,
        parent_snapshot_revision: u64,
        fresh_package_id: &'a str,
        fresh_snapshot_epoch: &'a str,
        candidate_fingerprint_sha256: &'a str,
        order_position_state: &'a Stage5gOrderPositionState,
    }
    semantic_sha256(&PostPackageProjection {
        domain: "moex.stage5g.fresh-truth-post-package.v1",
        parent_snapshot_id,
        parent_snapshot_revision,
        fresh_package_id,
        fresh_snapshot_epoch,
        candidate_fingerprint_sha256,
        order_position_state: state,
    })
}

fn lifecycle_checkpoint_sha256(
    projection: &Stage5gCleanRestartProjectionV1,
) -> Result<String, Stage5gCleanRestartError> {
    #[derive(Serialize)]
    struct LifecycleCheckpoint<'a> {
        domain: &'static str,
        lifecycle_kind: Stage5gCleanRestartLifecycleKind,
        strategy_state_fingerprint_sha256: &'a str,
        summary: &'a Stage5gOrderPositionSummary,
        checkpoint: &'a Stage5gTimerCheckpointEnvelope,
        order_position_state: &'a Option<Stage5gOrderPositionState>,
        timer_ready_source: &'a Option<Stage5gTimerReadyRestartProjectionV1>,
        #[serde(skip_serializing_if = "Option::is_none")]
        fresh_truth_application_evidence:
            &'a Option<crate::stage5g_fresh_broker_truth::Stage5gFreshTruthApplicationEvidenceV1>,
        #[serde(skip_serializing_if = "Option::is_none")]
        fresh_truth_application_authority_sha256: &'a Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        fresh_truth_application_authority_hmac_sha256: &'a Option<String>,
    }
    semantic_sha256(&LifecycleCheckpoint {
        domain: "moex.stage5g.clean-restart.lifecycle-checkpoint.v1",
        lifecycle_kind: projection.lifecycle_kind,
        strategy_state_fingerprint_sha256: &projection.strategy_state_fingerprint_sha256,
        summary: &projection.summary,
        checkpoint: &projection.checkpoint,
        order_position_state: &projection.order_position_state,
        timer_ready_source: &projection.timer_ready_source,
        fresh_truth_application_evidence: &projection.fresh_truth_application_evidence,
        fresh_truth_application_authority_sha256: &projection
            .fresh_truth_application_authority_sha256,
        fresh_truth_application_authority_hmac_sha256: &projection
            .fresh_truth_application_authority_hmac_sha256,
    })
}

fn lifecycle_source_authority_sha256(
    projection: &Stage5gCleanRestartProjectionV1,
) -> Result<String, Stage5gCleanRestartError> {
    #[derive(Serialize)]
    struct SourceAuthority<'a> {
        domain: &'static str,
        lifecycle_kind: Stage5gCleanRestartLifecycleKind,
        timer_ready_source: &'a Option<Stage5gTimerReadyRestartProjectionV1>,
        order_position_state: &'a Option<Stage5gOrderPositionState>,
        #[serde(skip_serializing_if = "Option::is_none")]
        fresh_truth_application_evidence:
            &'a Option<crate::stage5g_fresh_broker_truth::Stage5gFreshTruthApplicationEvidenceV1>,
        #[serde(skip_serializing_if = "Option::is_none")]
        fresh_truth_application_authority_sha256: &'a Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        fresh_truth_application_authority_hmac_sha256: &'a Option<String>,
    }
    semantic_sha256(&SourceAuthority {
        domain: "moex.stage5g.clean-restart.lifecycle-source-authority.v1",
        lifecycle_kind: projection.lifecycle_kind,
        timer_ready_source: &projection.timer_ready_source,
        order_position_state: &projection.order_position_state,
        fresh_truth_application_evidence: &projection.fresh_truth_application_evidence,
        fresh_truth_application_authority_sha256: &projection
            .fresh_truth_application_authority_sha256,
        fresh_truth_application_authority_hmac_sha256: &projection
            .fresh_truth_application_authority_hmac_sha256,
    })
}

fn independent_source_authority_sha256(
    projection: &Stage5gCleanRestartProjectionV1,
) -> Result<String, Stage5gCleanRestartError> {
    #[derive(Serialize)]
    struct IndependentSourceAuthority<'a> {
        domain: &'static str,
        binding: &'a Stage5gCleanRestartBindingV1,
        lifecycle_kind: Stage5gCleanRestartLifecycleKind,
        authoritative_callback_count: usize,
        zero_intent_ready: bool,
        strategy_state_fingerprint_sha256: &'a str,
        summary: &'a Stage5gOrderPositionSummary,
        checkpoint: &'a Stage5gTimerCheckpointEnvelope,
        order_position_state: &'a Option<Stage5gOrderPositionState>,
        timer_ready_source: &'a Option<Stage5gTimerReadyRestartProjectionV1>,
        #[serde(skip_serializing_if = "Option::is_none")]
        fresh_truth_application_evidence:
            &'a Option<crate::stage5g_fresh_broker_truth::Stage5gFreshTruthApplicationEvidenceV1>,
        #[serde(skip_serializing_if = "Option::is_none")]
        fresh_truth_application_authority_sha256: &'a Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        fresh_truth_application_authority_hmac_sha256: &'a Option<String>,
    }
    semantic_sha256(&IndependentSourceAuthority {
        domain: "moex.stage5g.clean-restart.stage5d-source-authority-anchor.v1",
        binding: &projection.binding,
        lifecycle_kind: projection.lifecycle_kind,
        authoritative_callback_count: projection.lifecycle_proof.authoritative_callback_count,
        zero_intent_ready: projection.lifecycle_proof.zero_intent_ready,
        strategy_state_fingerprint_sha256: &projection.strategy_state_fingerprint_sha256,
        summary: &projection.summary,
        checkpoint: &projection.checkpoint,
        order_position_state: &projection.order_position_state,
        timer_ready_source: &projection.timer_ready_source,
        fresh_truth_application_evidence: &projection.fresh_truth_application_evidence,
        fresh_truth_application_authority_sha256: &projection
            .fresh_truth_application_authority_sha256,
        fresh_truth_application_authority_hmac_sha256: &projection
            .fresh_truth_application_authority_hmac_sha256,
    })
}

fn authenticated_restart_package_commitment_sha256(
    projection: &Stage5gCleanRestartProjectionV1,
    envelope: &crate::Stage5dPersistenceEnvelope,
    riskgate_evidence: &Stage5dRiskGateLedgerEvidence,
    package_schema_version: u16,
    checkpoint_state: Stage5dCanonicalRestartCheckpointState,
) -> Result<String, Stage5gCleanRestartError> {
    #[derive(Serialize)]
    struct AuthenticatedPackageInstance<'a> {
        schema_version: u16,
        snapshot_id: &'a str,
        snapshot_revision: u64,
        previous_revision: Option<u64>,
        write_generation: u64,
        persisted_at_ts_utc: DateTime<Utc>,
        stage5d_lifecycle_watermarks_sha256: &'a str,
        lifecycle_source_authority_sha256: &'a str,
        stage5g_lifecycle_checkpoint_sha256: &'a str,
    }

    #[derive(Serialize)]
    struct AuthenticatedStage5gProjection<'a> {
        binding: &'a Stage5gCleanRestartBindingV1,
        lifecycle_kind: Stage5gCleanRestartLifecycleKind,
        authoritative_callback_count: usize,
        zero_intent_ready: bool,
        strategy_state_fingerprint_sha256: &'a str,
        summary: &'a Stage5gOrderPositionSummary,
        checkpoint: &'a Stage5gTimerCheckpointEnvelope,
        order_position_state: &'a Option<Stage5gOrderPositionState>,
        timer_ready_source: &'a Option<Stage5gTimerReadyRestartProjectionV1>,
        #[serde(skip_serializing_if = "Option::is_none")]
        fresh_truth_application_evidence:
            &'a Option<crate::stage5g_fresh_broker_truth::Stage5gFreshTruthApplicationEvidenceV1>,
        #[serde(skip_serializing_if = "Option::is_none")]
        fresh_truth_application_authority_sha256: &'a Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        fresh_truth_application_authority_hmac_sha256: &'a Option<String>,
        package_instance: AuthenticatedPackageInstance<'a>,
    }

    #[derive(Serialize)]
    struct Stage5gAuthenticatedRestartPackageCommitmentV1<'a> {
        schema_version: u16,
        domain: &'static str,
        package_schema_version: u16,
        checkpoint_state: Stage5dCanonicalRestartCheckpointState,
        stage5d_envelope_without_transport_integrity: &'a crate::Stage5dPersistenceEnvelope,
        riskgate_evidence: &'a Stage5dRiskGateLedgerEvidence,
        stage5g: AuthenticatedStage5gProjection<'a>,
    }

    let mut normalized_envelope = envelope.clone();
    normalized_envelope.payload_checksum_sha256.clear();
    normalized_envelope.stage5g_source_authority_hmac_sha256 = None;
    let instance = &projection.package_instance;
    let canonical = Stage5gAuthenticatedRestartPackageCommitmentV1 {
        schema_version: STAGE5G_AUTHENTICATED_RESTART_PACKAGE_COMMITMENT_SCHEMA_VERSION,
        domain: "moex.stage5g.clean-restart.authenticated-package.v1",
        package_schema_version,
        checkpoint_state,
        stage5d_envelope_without_transport_integrity: &normalized_envelope,
        riskgate_evidence,
        stage5g: AuthenticatedStage5gProjection {
            binding: &projection.binding,
            lifecycle_kind: projection.lifecycle_kind,
            authoritative_callback_count: projection.lifecycle_proof.authoritative_callback_count,
            zero_intent_ready: projection.lifecycle_proof.zero_intent_ready,
            strategy_state_fingerprint_sha256: &projection.strategy_state_fingerprint_sha256,
            summary: &projection.summary,
            checkpoint: &projection.checkpoint,
            order_position_state: &projection.order_position_state,
            timer_ready_source: &projection.timer_ready_source,
            fresh_truth_application_evidence: &projection.fresh_truth_application_evidence,
            fresh_truth_application_authority_sha256: &projection
                .fresh_truth_application_authority_sha256,
            fresh_truth_application_authority_hmac_sha256: &projection
                .fresh_truth_application_authority_hmac_sha256,
            package_instance: AuthenticatedPackageInstance {
                schema_version: instance.schema_version,
                snapshot_id: &instance.snapshot_id,
                snapshot_revision: instance.snapshot_revision,
                previous_revision: instance.previous_revision,
                write_generation: instance.write_generation,
                persisted_at_ts_utc: instance.persisted_at_ts_utc,
                stage5d_lifecycle_watermarks_sha256: &instance.stage5d_lifecycle_watermarks_sha256,
                lifecycle_source_authority_sha256: &instance.lifecycle_source_authority_sha256,
                stage5g_lifecycle_checkpoint_sha256: &instance.stage5g_lifecycle_checkpoint_sha256,
            },
        },
    };
    semantic_sha256(&canonical)
}

#[cfg(test)]
pub(crate) fn stage5g_test_source_authority_anchor_sha256(
    projection: &Stage5gCleanRestartProjectionV1,
) -> String {
    independent_source_authority_sha256(projection)
        .expect("test Stage 5G source authority remains canonical")
}

#[cfg(test)]
pub(crate) fn stage5g_test_authenticated_restart_hmac_sha256(
    projection: &Stage5gCleanRestartProjectionV1,
    envelope: &crate::Stage5dPersistenceEnvelope,
    riskgate_evidence: &Stage5dRiskGateLedgerEvidence,
    commitment_key: &Stage5gLifecycleCommitmentKey,
) -> String {
    let commitment = authenticated_restart_package_commitment_sha256(
        projection,
        envelope,
        riskgate_evidence,
        STAGE5D_CANONICAL_RESTART_PACKAGE_SCHEMA_VERSION,
        Stage5dCanonicalRestartCheckpointState::Committed,
    )
    .expect("test authenticated commitment remains canonical");
    lifecycle_commitment_hmac_sha256(commitment_key, &commitment)
}

fn lifecycle_commitment_hmac_sha256(
    key: &Stage5gLifecycleCommitmentKey,
    authenticated_package_commitment_sha256: &str,
) -> String {
    let mut mac =
        Hmac::<Sha256>::new_from_slice(&key.0).expect("fixed-size Stage 5G HMAC key is valid");
    mac.update(b"moex.stage5g.clean-restart.full-package-commitment.v1\0");
    mac.update(authenticated_package_commitment_sha256.as_bytes());
    let tag = mac.finalize().into_bytes();
    tag.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn application_authority_hmac_sha256(
    key: &Stage5gLifecycleCommitmentKey,
    application_authority_sha256: &str,
) -> String {
    let mut mac =
        Hmac::<Sha256>::new_from_slice(&key.0).expect("fixed-size Stage 5G HMAC key is valid");
    mac.update(b"moex.stage5g.edc.application-authority.v1\0");
    mac.update(application_authority_sha256.as_bytes());
    let tag = mac.finalize().into_bytes();
    tag.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
pub(crate) fn stage5g_test_application_authority_hmac_sha256(
    key: &Stage5gLifecycleCommitmentKey,
    application_authority_sha256: &str,
) -> String {
    application_authority_hmac_sha256(key, application_authority_sha256)
}

fn verify_application_authority_hmac(
    key: &Stage5gLifecycleCommitmentKey,
    application_authority_sha256: &str,
    expected_hmac_sha256: &str,
) -> bool {
    let Some(tag) = decode_sha256_hex(expected_hmac_sha256) else {
        return false;
    };
    let mut mac =
        Hmac::<Sha256>::new_from_slice(&key.0).expect("fixed-size Stage 5G HMAC key is valid");
    mac.update(b"moex.stage5g.edc.application-authority.v1\0");
    mac.update(application_authority_sha256.as_bytes());
    mac.verify_slice(&tag).is_ok()
}

fn verify_lifecycle_commitment_hmac(
    key: &Stage5gLifecycleCommitmentKey,
    authenticated_package_commitment_sha256: &str,
    expected_hmac_sha256: &str,
) -> bool {
    let Some(tag) = decode_sha256_hex(expected_hmac_sha256) else {
        return false;
    };
    let mut mac =
        Hmac::<Sha256>::new_from_slice(&key.0).expect("fixed-size Stage 5G HMAC key is valid");
    mac.update(b"moex.stage5g.clean-restart.full-package-commitment.v1\0");
    mac.update(authenticated_package_commitment_sha256.as_bytes());
    mac.verify_slice(&tag).is_ok()
}

fn decode_sha256_hex(value: &str) -> Option<[u8; 32]> {
    if !is_sha256_hex(value) {
        return None;
    }
    let mut decoded = [0_u8; 32];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        let high = (pair[0] as char).to_digit(16)? as u8;
        let low = (pair[1] as char).to_digit(16)? as u8;
        decoded[index] = (high << 4) | low;
    }
    Some(decoded)
}

fn source_lifecycle_commit_sha256(
    projection: &Stage5gCleanRestartProjectionV1,
) -> Result<String, Stage5gCleanRestartError> {
    #[derive(Serialize)]
    struct SourceCommit<'a> {
        domain: &'static str,
        snapshot_id: &'a str,
        snapshot_revision: u64,
        previous_revision: Option<u64>,
        write_generation: u64,
        persisted_at_ts_utc: DateTime<Utc>,
        stage5d_payload_checksum_sha256: &'a str,
        stage5d_lifecycle_watermarks_sha256: &'a str,
        lifecycle_source_authority_sha256: &'a str,
        stage5g_lifecycle_checkpoint_sha256: &'a str,
    }
    let binding = &projection.package_instance;
    semantic_sha256(&SourceCommit {
        domain: "moex.stage5g.clean-restart.source-lifecycle-commit.v1",
        snapshot_id: &binding.snapshot_id,
        snapshot_revision: binding.snapshot_revision,
        previous_revision: binding.previous_revision,
        write_generation: binding.write_generation,
        persisted_at_ts_utc: binding.persisted_at_ts_utc,
        stage5d_payload_checksum_sha256: &binding.stage5d_payload_checksum_sha256,
        stage5d_lifecycle_watermarks_sha256: &binding.stage5d_lifecycle_watermarks_sha256,
        lifecycle_source_authority_sha256: &binding.lifecycle_source_authority_sha256,
        stage5g_lifecycle_checkpoint_sha256: &binding.stage5g_lifecycle_checkpoint_sha256,
    })
}

fn lifecycle_authority_sha256(
    projection: &Stage5gCleanRestartProjectionV1,
) -> Result<String, Stage5gCleanRestartError> {
    #[derive(Serialize)]
    struct Authority<'a> {
        domain: &'static str,
        binding: &'a Stage5gCleanRestartBindingV1,
        lifecycle_kind: Stage5gCleanRestartLifecycleKind,
        authoritative_callback_count: usize,
        zero_intent_ready: bool,
        strategy_state_fingerprint_sha256: &'a str,
        summary: &'a Stage5gOrderPositionSummary,
        checkpoint: &'a Stage5gTimerCheckpointEnvelope,
        order_position_state: &'a Option<Stage5gOrderPositionState>,
        timer_ready_source: &'a Option<Stage5gTimerReadyRestartProjectionV1>,
        #[serde(skip_serializing_if = "Option::is_none")]
        fresh_truth_application_evidence:
            &'a Option<crate::stage5g_fresh_broker_truth::Stage5gFreshTruthApplicationEvidenceV1>,
        #[serde(skip_serializing_if = "Option::is_none")]
        fresh_truth_application_authority_sha256: &'a Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        fresh_truth_application_authority_hmac_sha256: &'a Option<String>,
        package_instance: &'a Stage5gPackageInstanceBindingV1,
    }
    let bytes = serde_json::to_vec(&Authority {
        domain: "moex.stage5g.clean-restart.source-authority.v1",
        binding: &projection.binding,
        lifecycle_kind: projection.lifecycle_kind,
        authoritative_callback_count: projection.lifecycle_proof.authoritative_callback_count,
        zero_intent_ready: projection.lifecycle_proof.zero_intent_ready,
        strategy_state_fingerprint_sha256: &projection.strategy_state_fingerprint_sha256,
        summary: &projection.summary,
        checkpoint: &projection.checkpoint,
        order_position_state: &projection.order_position_state,
        timer_ready_source: &projection.timer_ready_source,
        fresh_truth_application_evidence: &projection.fresh_truth_application_evidence,
        fresh_truth_application_authority_sha256: &projection
            .fresh_truth_application_authority_sha256,
        fresh_truth_application_authority_hmac_sha256: &projection
            .fresh_truth_application_authority_hmac_sha256,
        package_instance: &projection.package_instance,
    })
    .map_err(|_| Stage5gCleanRestartError::ProjectionDecode)?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

#[cfg(test)]
pub(crate) fn stage5g_test_reseal_lifecycle_authority(
    projection: &mut Stage5gCleanRestartProjectionV1,
) {
    projection
        .package_instance
        .lifecycle_source_authority_sha256 =
        lifecycle_source_authority_sha256(projection).expect("test source authority reseals");
    stage5g_test_reseal_nested_integrity(projection);
}

#[cfg(test)]
pub(crate) fn stage5g_test_reseal_nested_integrity(
    projection: &mut Stage5gCleanRestartProjectionV1,
) {
    projection
        .package_instance
        .stage5g_lifecycle_checkpoint_sha256 =
        lifecycle_checkpoint_sha256(projection).expect("test lifecycle checkpoint reseals");
    projection.package_instance.source_lifecycle_commit_sha256 =
        source_lifecycle_commit_sha256(projection).expect("test source lifecycle commit reseals");
    projection.lifecycle_proof.source_authority_sha256 =
        lifecycle_authority_sha256(projection).expect("test projection authority reseals");
}

#[cfg(test)]
mod authenticated_commitment_tests {
    use super::*;

    #[test]
    fn stage5ge_c_r4_debug_release_commitment_vector_is_deterministic() {
        let key = Stage5gLifecycleCommitmentKey::from_secret_bytes(&[0x5a; 32]).unwrap();
        assert_eq!(
            lifecycle_commitment_hmac_sha256(&key, &"a".repeat(64)),
            "ff49355053b14670675a16f5333dcca7dc45d2f7adcf2718afc869beecbd2a65"
        );
    }
}
