//! Stage 5G-e-c canonical clean-process reconstruction boundary.
//!
//! The only durable authority is the accepted Stage 5D canonical restart
//! package. Stage 5G contributes a checksummed, versioned extension to that
//! package; it does not define an alternative restart document.

use broker_core::{BrokerAccountId, InstrumentId};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::runtime_compat::Strategy;
use crate::stage5d_persistence::{
    stage5d_decode_canonical_restart_bytes_requiring_stage5g,
    stage5d_export_canonical_restart_bytes_with_stage5g_extension,
    stage5d_reconstruct_runtime_from_clean_restart, Stage5dCanonicalEnvelopeExportInput,
};
use crate::stage5g_order_position::Stage5gOrderPositionState;
use crate::{
    HybridIntradayRuntimeStrategy, Stage5dEnvelopeValidationError, Stage5dLifecycleWatermarks,
    Stage5dRiskGateLedgerEvidence, Stage5dRiskGatePersistence,
    Stage5gCommittedAwaitingOrderPosition, Stage5gCommittedExactReplaySession,
    Stage5gOrderPositionSession, Stage5gOrderPositionSummary, Stage5gTimerCheckpointEnvelope,
    Stage5gTimerCheckpointError, Stage5gTimerReadyPaperStrategy,
};

pub const STAGE5G_CLEAN_RESTART_EXTENSION_SCHEMA_VERSION: u16 = 1;

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
pub(crate) struct Stage5gCleanRestartProjectionV1 {
    pub(crate) schema_version: u16,
    pub(crate) binding: Stage5gCleanRestartBindingV1,
    pub(crate) lifecycle_kind: Stage5gCleanRestartLifecycleKind,
    pub(crate) lifecycle_proof: Stage5gCleanRestartLifecycleProofV1,
    pub(crate) strategy_state_fingerprint_sha256: String,
    pub(crate) summary: Stage5gOrderPositionSummary,
    pub(crate) checkpoint: Stage5gTimerCheckpointEnvelope,
    pub(crate) order_position_state: Option<Stage5gOrderPositionState>,
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
}

impl Stage5gCleanRestartedCapability {
    pub fn lifecycle_kind(&self) -> Stage5gCleanRestartLifecycleKind {
        self.projection.lifecycle_kind
    }

    pub fn summary(&self) -> &Stage5gOrderPositionSummary {
        &self.projection.summary
    }

    pub fn checkpoint(&self) -> &Stage5gTimerCheckpointEnvelope {
        &self.projection.checkpoint
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
        Stage5gCleanRestartProjectionV1,
    ) {
        (self.runtime, self.projection)
    }

    #[allow(dead_code)]
    pub(crate) fn next_reconciliation_observation(&self) -> Stage5gNextReconciliationObservation {
        Stage5gNextReconciliationObservation {
            strategy_id: self.projection.binding.strategy_id.clone(),
            account_id: self.projection.binding.account_id.clone(),
            instrument_id: self.projection.binding.instrument_id.clone(),
            lifecycle_kind: self.projection.lifecycle_kind,
            callback_count: self.projection.lifecycle_proof.authoritative_callback_count,
            request_count: self.projection.summary.request_count,
            continuation_checkpoint_ts_utc_ms: self
                .projection
                .checkpoint
                .payload
                .last_continuation_checkpoint_ts_utc_ms,
        }
    }
}

pub fn export_stage5g_clean_restart(
    source: Stage5gCleanRestartSource,
    input: Stage5gCleanRestartExportInput,
) -> Result<Vec<u8>, Stage5gCleanRestartError> {
    let projection = projection_from_source(&source)?;
    let strategy = strategy_from_source(&source);
    let extension_json = serde_json::to_string(&projection)
        .map_err(|_| Stage5gCleanRestartError::ProjectionDecode)?;
    let stage5d_input = Stage5dCanonicalEnvelopeExportInput {
        snapshot_id: input.snapshot_id,
        snapshot_revision: input.snapshot_revision,
        previous_revision: input.previous_revision,
        write_generation: input.write_generation,
        persisted_at_ts_utc: input.persisted_at_ts_utc,
        strategy_id: projection.binding.strategy_id.clone(),
        account_id: projection.binding.account_id.clone(),
        instrument_id: projection.binding.instrument_id.clone(),
        source_commit_or_build_id: input.source_commit_or_build_id,
        lifecycle_watermarks: input.lifecycle_watermarks,
        riskgate: input.riskgate,
    };
    let bytes = stage5d_export_canonical_restart_bytes_with_stage5g_extension(
        strategy,
        stage5d_input,
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
    fresh_runtime: HybridIntradayRuntimeStrategy,
) -> Result<Stage5gCleanRestartedCapability, Stage5gCleanRestartError> {
    let decoded = stage5d_decode_canonical_restart_bytes_requiring_stage5g(bytes)?;
    let projection: Stage5gCleanRestartProjectionV1 =
        serde_json::from_str(&decoded.stage5g_extension_json)
            .map_err(|_| Stage5gCleanRestartError::ProjectionDecode)?;
    validate_projection(&projection)?;
    validate_projection_binding(&projection, &decoded.envelope, &fresh_runtime)?;
    let (runtime, _extension_json) =
        stage5d_reconstruct_runtime_from_clean_restart(decoded, fresh_runtime)?;
    let restored_state = serde_json::to_value(Strategy::state(&runtime))
        .map_err(|_| Stage5gCleanRestartError::StrategyStateFingerprintMismatch)?;
    let restored_fingerprint =
        crate::stage5c_paper_host::stage5c_semantic_value_fingerprint(&restored_state)
            .map_err(|_| Stage5gCleanRestartError::StrategyStateFingerprintMismatch)?;
    if restored_fingerprint != projection.strategy_state_fingerprint_sha256 {
        return Err(Stage5gCleanRestartError::StrategyStateFingerprintMismatch);
    }
    Ok(Stage5gCleanRestartedCapability {
        runtime,
        projection,
    })
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

pub(crate) fn projection_from_source(
    source: &Stage5gCleanRestartSource,
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
    ) = match source {
        Stage5gCleanRestartSource::TimerReady(value) => (
            Stage5gCleanRestartLifecycleKind::TimerReady,
            1,
            value.stage5g_restart_is_zero_intent_ready(),
            value.summary().clone(),
            value.checkpoint(),
            None,
        ),
        Stage5gCleanRestartSource::OrderPositionAwaiting(value) => (
            Stage5gCleanRestartLifecycleKind::OrderPositionAwaitingCommitted,
            0,
            false,
            value.summary(),
            value.stage5g_restart_checkpoint(),
            Some(value.stage5g_restart_state()),
        ),
        Stage5gCleanRestartSource::ExactReplaySynchronized(value) => (
            Stage5gCleanRestartLifecycleKind::OrderPositionAwaitingCommitted,
            0,
            false,
            value.session().summary(),
            value.checkpoint().clone(),
            Some(value.session().stage5g_restart_state()),
        ),
        Stage5gCleanRestartSource::NewPackageAwaiting(value) => (
            Stage5gCleanRestartLifecycleKind::OrderPositionAwaitingCommitted,
            0,
            false,
            value.session().summary(),
            value.checkpoint().clone(),
            Some(value.session().stage5g_restart_state()),
        ),
    };
    let projection = Stage5gCleanRestartProjectionV1 {
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
    };
    let mut projection = projection;
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
            validate_summary_checkpoint_projection(projection)
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

fn validate_projection_binding(
    projection: &Stage5gCleanRestartProjectionV1,
    envelope: &crate::Stage5dPersistenceEnvelope,
    fresh_runtime: &HybridIntradayRuntimeStrategy,
) -> Result<(), Stage5gCleanRestartError> {
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
    Ok(())
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
    })
    .map_err(|_| Stage5gCleanRestartError::ProjectionDecode)?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

#[cfg(test)]
pub(crate) fn stage5g_test_reseal_lifecycle_authority(
    projection: &mut Stage5gCleanRestartProjectionV1,
) {
    projection.lifecycle_proof.source_authority_sha256 =
        lifecycle_authority_sha256(projection).expect("test projection authority reseals");
}
