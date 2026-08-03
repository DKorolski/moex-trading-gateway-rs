//! Stage 5G-e-c canonical clean-process reconstruction boundary.
//!
//! The only durable authority is the accepted Stage 5D canonical restart
//! package. Stage 5G contributes a checksummed, versioned extension to that
//! package; it does not define an alternative restart document.

use broker_core::{BrokerAccountId, InstrumentId};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

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
    pub strategy_id: String,
    pub account_id: BrokerAccountId,
    pub instrument_id: InstrumentId,
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
    OrderPositionAwaiting,
    ExactReplaySynchronized,
    NewPackageAwaiting,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Stage5gCleanRestartProjectionV1 {
    pub(crate) schema_version: u16,
    pub(crate) lifecycle_kind: Stage5gCleanRestartLifecycleKind,
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
        strategy_id: input.strategy_id,
        account_id: input.account_id,
        instrument_id: input.instrument_id,
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
    let (runtime, extension_json) =
        stage5d_reconstruct_runtime_from_clean_restart(decoded, fresh_runtime)?;
    let projection: Stage5gCleanRestartProjectionV1 = serde_json::from_str(&extension_json)
        .map_err(|_| Stage5gCleanRestartError::ProjectionDecode)?;
    validate_projection(&projection)?;
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

pub(crate) fn projection_from_source(
    source: &Stage5gCleanRestartSource,
) -> Result<Stage5gCleanRestartProjectionV1, Stage5gCleanRestartError> {
    let strategy_state = serde_json::to_value(Strategy::state(strategy_from_source(source)))
        .map_err(|_| Stage5gCleanRestartError::StrategyStateFingerprintMismatch)?;
    let strategy_state_fingerprint_sha256 =
        crate::stage5c_paper_host::stage5c_semantic_value_fingerprint(&strategy_state)
            .map_err(|_| Stage5gCleanRestartError::StrategyStateFingerprintMismatch)?;
    let (lifecycle_kind, summary, checkpoint, order_position_state) = match source {
        Stage5gCleanRestartSource::TimerReady(value) => (
            Stage5gCleanRestartLifecycleKind::TimerReady,
            value.summary().clone(),
            value.checkpoint(),
            None,
        ),
        Stage5gCleanRestartSource::OrderPositionAwaiting(value) => (
            Stage5gCleanRestartLifecycleKind::OrderPositionAwaiting,
            value.summary(),
            value.stage5g_restart_checkpoint(),
            Some(value.stage5g_restart_state()),
        ),
        Stage5gCleanRestartSource::ExactReplaySynchronized(value) => (
            Stage5gCleanRestartLifecycleKind::ExactReplaySynchronized,
            value.session().summary(),
            value.checkpoint().clone(),
            Some(value.session().stage5g_restart_state()),
        ),
        Stage5gCleanRestartSource::NewPackageAwaiting(value) => (
            Stage5gCleanRestartLifecycleKind::NewPackageAwaiting,
            value.session().summary(),
            value.checkpoint().clone(),
            Some(value.session().stage5g_restart_state()),
        ),
    };
    let projection = Stage5gCleanRestartProjectionV1 {
        schema_version: STAGE5G_CLEAN_RESTART_EXTENSION_SCHEMA_VERSION,
        lifecycle_kind,
        strategy_state_fingerprint_sha256,
        summary,
        checkpoint,
        order_position_state,
    };
    validate_projection(&projection)?;
    Ok(projection)
}

pub(crate) fn validate_projection(
    projection: &Stage5gCleanRestartProjectionV1,
) -> Result<(), Stage5gCleanRestartError> {
    if projection.schema_version != STAGE5G_CLEAN_RESTART_EXTENSION_SCHEMA_VERSION {
        return Err(Stage5gCleanRestartError::UnsupportedProjectionSchema);
    }
    crate::validate_stage5g_timer_checkpoint(&projection.checkpoint)
        .map_err(Stage5gCleanRestartError::ReplayCheckpoint)?;
    match (
        projection.lifecycle_kind,
        projection.order_position_state.as_ref(),
    ) {
        (Stage5gCleanRestartLifecycleKind::TimerReady, None) => Ok(()),
        (Stage5gCleanRestartLifecycleKind::TimerReady, Some(_)) => {
            Err(Stage5gCleanRestartError::UnexpectedOrderPositionState)
        }
        (_, None) => Err(Stage5gCleanRestartError::MissingOrderPositionState),
        (_, Some(state)) => {
            if Stage5gOrderPositionSession::stage5g_restart_projection_is_coherent(
                state,
                &projection.summary,
                &projection.checkpoint,
            ) {
                Ok(())
            } else {
                Err(Stage5gCleanRestartError::ReplayProjectionInconsistent)
            }
        }
    }
}
