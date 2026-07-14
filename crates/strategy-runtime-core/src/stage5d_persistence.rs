//! Stage 5D additive persistence freeze surface.
//!
//! Stage 5D-b2a adds versioned persistence DTOs and schema validation. It does
//! not implement runtime-private snapshot application, Stage 5C/Stage 5D
//! transitions, Redis, FINAM, transport, dispatch, or runtime-live behavior.

use broker_core::{
    BrokerOrderId, BrokerStopOrderId, BrokerTradeId, ClientOrderId, StrategyRequestId,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Stage 5D additive freeze manifest schema version.
pub const STAGE5D_ADDITIVE_FREEZE_SCHEMA_VERSION: u16 = 1;
/// Stage 5D persistence envelope schema version.
pub const STAGE5D_PERSISTENCE_ENVELOPE_SCHEMA_VERSION: u16 = 1;
/// Stage 5D runtime-private extension schema version.
pub const STAGE5D_RUNTIME_PRIVATE_EXTENSION_SCHEMA_VERSION: u16 = 1;
/// Stage 5D riskgate persistence schema version.
pub const STAGE5D_RISKGATE_SCHEMA_VERSION: u16 = 1;

/// Opaque proof that a validated Stage 5D runtime-private extension has been
/// applied in the persistence-enabled restore path.
pub struct Stage5dPrivateStateAppliedPaperStrategy {
    _private: (),
}

/// Opaque proof that the Stage 5D restore path has passed controlled bootstrap.
pub struct Stage5dBootstrappedPaperStrategy {
    _private: (),
}

/// Opaque proof that authoritative riskgate state has been injected before the
/// runtime-state-restored callback.
pub struct Stage5dRiskGateInjectedPaperStrategy {
    _private: (),
}

/// Opaque validated runtime-private extension marker.
pub struct Stage5dValidatedRuntimePrivateExtension {
    _private: (),
}

/// Redacted blocked-restore marker for future Stage 5D transitions.
pub struct Stage5dRestoreBlocked {
    reason: Stage5dRestoreBlockReason,
}

impl Stage5dRestoreBlocked {
    /// Return the redacted block reason without exposing strategy internals.
    pub fn reason(&self) -> Stage5dRestoreBlockReason {
        self.reason
    }
}

/// Public redacted restore blocker categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage5dRestoreBlockReason {
    PrivateExtension,
    RiskGate,
    BrokerTruth,
    Integrity,
}

/// Redacted evidence that the Stage 5D additive freeze enforcement layer is
/// present. This is not a trading capability.
pub struct Stage5dAdditiveFreezeEvidence {
    schema_version: u16,
}

impl Stage5dAdditiveFreezeEvidence {
    /// Construct redacted local evidence for checker/tests.
    pub fn local() -> Self {
        Self {
            schema_version: STAGE5D_ADDITIVE_FREEZE_SCHEMA_VERSION,
        }
    }

    /// Schema version of the Stage 5D additive freeze surface.
    pub fn schema_version(&self) -> u16 {
        self.schema_version
    }
}

/// Timestamp units used by every numeric timestamp family in the envelope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage5dTimestampUnits {
    Seconds,
    Milliseconds,
}

/// Stable side enum for Stage 5D persistence schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage5dSide {
    Long,
    Short,
}

/// Stable owner enum for Stage 5D persistence schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage5dOwner {
    MeanReversion,
    IntradayBreakout,
    Unknown,
}

/// Stable entry-style enum for Stage 5D persistence schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage5dEntryStyle {
    Market,
    MarketableLimit,
    Bracket,
    Unknown,
}

/// Stable reason enum for Stage 5D persistence schema.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage5dLifecycleReason {
    MorningMeanReversionLong,
    MorningMeanReversionShort,
    IntradayBreakoutLong,
    IntradayBreakoutShort,
    Cleanup,
    Operator,
    Other(String),
}

/// Lifecycle watermarks that bind restored state to processed data progress.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stage5dLifecycleWatermarks {
    pub persisted_event_watermark: Option<String>,
    pub last_semantic_bar_ts: Option<DateTime<Utc>>,
    pub last_broker_event_ts: Option<DateTime<Utc>>,
}

/// Broker-neutral typed recovery indexes. Namespaces are intentionally
/// separated; `ClientOrderId` never substitutes for `StrategyRequestId`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stage5dRecoveryIndexes {
    pub known_order_ids: Vec<BrokerOrderId>,
    pub known_stop_order_ids: Vec<BrokerStopOrderId>,
    pub known_trade_ids: Vec<BrokerTradeId>,
    pub known_client_order_ids: Vec<ClientOrderId>,
    pub pending_requests: Vec<StrategyRequestId>,
}

/// Runtime-private pending entry schema. This is a persistence representation,
/// not the source-private runtime struct.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Stage5dPendingEntryExtension {
    pub owner: Stage5dOwner,
    pub side: Stage5dSide,
    pub reason: Stage5dLifecycleReason,
    pub entry_style: Stage5dEntryStyle,
    pub target_qty: String,
    pub stop_price: Option<String>,
    pub take_price: Option<String>,
    pub request_id: Option<StrategyRequestId>,
}

/// Runtime-private partial-entry timer schema.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stage5dPartialEntryTimer {
    pub partial_started_at_ms: i64,
}

/// Runtime-private pending exit schema.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stage5dPendingExitExtension {
    pub owner: Stage5dOwner,
    pub reason: Stage5dLifecycleReason,
    pub request_id: StrategyRequestId,
}

/// Runtime-private bracket reconciliation timer schema.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stage5dBracketReconciliationTimer {
    pub bracket_terminal_reconcile_started_ms: i64,
}

/// Runtime-private cleanup retry schema.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stage5dCleanupRetryState {
    pub cleanup_stop_retry_attempts: u32,
}

/// Non-authoritative expected broker-object hints. Actual working sets must be
/// rebuilt from broker truth before callbacks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stage5dExpectedWorkingSets {
    pub expected_working_order_ids: Vec<BrokerOrderId>,
    pub expected_working_stop_order_ids: Vec<BrokerStopOrderId>,
}

/// Riskgate finalization outbox record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stage5dRiskGateFinalizationOutboxRecord {
    pub session_date: String,
    pub generation: u64,
    pub state: Stage5dRiskGateFinalizationState,
    pub identity_hash: String,
}

/// Riskgate finalization outbox state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage5dRiskGateFinalizationState {
    Prepared,
    LedgerAppended,
    MaterializedUpdated,
    AcknowledgedInRuntime,
}

/// Versioned Stage 5D runtime-private extension DTO. This DTO is schema-only in
/// Stage 5D-b2a and is not applied to the runtime.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Stage5dRuntimePrivateExtension {
    pub schema_version: u16,
    pub pending_entry: Option<Stage5dPendingEntryExtension>,
    pub partial_entry_timer: Option<Stage5dPartialEntryTimer>,
    pub pending_exit: Option<Stage5dPendingExitExtension>,
    pub bracket_reconciliation_timer: Option<Stage5dBracketReconciliationTimer>,
    pub cleanup_retry_state: Option<Stage5dCleanupRetryState>,
    pub expected_working_sets: Stage5dExpectedWorkingSets,
    pub last_processed_bar_ts: Option<DateTime<Utc>>,
    pub pending_riskgate_finalizations: Vec<Stage5dRiskGateFinalizationOutboxRecord>,
}

/// Riskgate identity section.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stage5dRiskGateIdentity {
    pub strategy_id: String,
    pub profile_id: String,
    pub mr_variant: String,
    pub timeframe: String,
    pub session_policy: String,
    pub model_version: String,
}

/// Materialized riskgate projection.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Stage5dRiskGateMaterializedState {
    pub mr_enabled_current_session: bool,
    pub mr_enabled_next_session: bool,
    pub rolling_sum_lb120: String,
    pub last_finalized_session_date: Option<String>,
    pub ledger_rows_count: u64,
}

/// Riskgate persistence DTO. This is schema-only in Stage 5D-b2a.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Stage5dRiskGatePersistence {
    pub schema_version: u16,
    pub identity: Stage5dRiskGateIdentity,
    pub materialized_state: Stage5dRiskGateMaterializedState,
    pub ledger_tail_hash: String,
    pub finalization_outbox: Vec<Stage5dRiskGateFinalizationOutboxRecord>,
}

/// Versioned Stage 5D persistence envelope DTO.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Stage5dPersistenceEnvelope {
    pub schema_version: u16,
    pub snapshot_id: String,
    pub snapshot_revision: u64,
    pub previous_revision: Option<u64>,
    pub write_generation: u64,
    pub persisted_at_ts_utc: DateTime<Utc>,
    pub timestamp_units: Stage5dTimestampUnits,
    pub canonical_config_fingerprint: String,
    pub payload_checksum_sha256: String,
    pub lifecycle_watermarks: Stage5dLifecycleWatermarks,
    pub recovery_indexes: Stage5dRecoveryIndexes,
    pub runtime_private_extension: Stage5dRuntimePrivateExtension,
    pub riskgate: Stage5dRiskGatePersistence,
}

impl Stage5dPersistenceEnvelope {
    /// Compute the canonical payload checksum with the checksum field cleared.
    pub fn compute_payload_checksum_sha256(
        &self,
    ) -> Result<String, Stage5dEnvelopeValidationError> {
        let mut canonical = self.clone();
        canonical.payload_checksum_sha256.clear();
        let payload = serde_json::to_vec(&canonical)
            .map_err(|_| Stage5dEnvelopeValidationError::SerializationFailed)?;
        Ok(format!("{:x}", Sha256::digest(payload)))
    }

    /// Validate schema versions, required identity fields and payload checksum.
    pub fn validate_schema_and_checksum(&self) -> Result<(), Stage5dEnvelopeValidationError> {
        if self.schema_version != STAGE5D_PERSISTENCE_ENVELOPE_SCHEMA_VERSION {
            return Err(Stage5dEnvelopeValidationError::EnvelopeSchemaMismatch);
        }
        if self.runtime_private_extension.schema_version
            != STAGE5D_RUNTIME_PRIVATE_EXTENSION_SCHEMA_VERSION
        {
            return Err(Stage5dEnvelopeValidationError::RuntimePrivateSchemaMismatch);
        }
        if self.riskgate.schema_version != STAGE5D_RISKGATE_SCHEMA_VERSION {
            return Err(Stage5dEnvelopeValidationError::RiskGateSchemaMismatch);
        }
        if self.snapshot_id.is_empty() || self.canonical_config_fingerprint.is_empty() {
            return Err(Stage5dEnvelopeValidationError::RequiredFieldEmpty);
        }
        let expected = self.compute_payload_checksum_sha256()?;
        if self.payload_checksum_sha256 != expected {
            return Err(Stage5dEnvelopeValidationError::PayloadChecksumMismatch);
        }
        Ok(())
    }
}

/// Stage 5D envelope validation errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage5dEnvelopeValidationError {
    EnvelopeSchemaMismatch,
    RuntimePrivateSchemaMismatch,
    RiskGateSchemaMismatch,
    RequiredFieldEmpty,
    PayloadChecksumMismatch,
    SerializationFailed,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn valid_fixture() -> Stage5dPersistenceEnvelope {
        serde_json::from_str(include_str!(
            "../../../tests/fixtures/stage5/stage5d_b2a_persistence_envelope.json"
        ))
        .expect("valid Stage 5D-b2a fixture must deserialize")
    }

    fn corrupt_checksum_fixture() -> Stage5dPersistenceEnvelope {
        serde_json::from_str(include_str!(
            "../../../tests/fixtures/stage5/stage5d_b2a_persistence_envelope_corrupt_checksum.json"
        ))
        .expect("corrupt Stage 5D-b2a checksum fixture must deserialize")
    }

    fn bad_version_fixture() -> Stage5dPersistenceEnvelope {
        serde_json::from_str(include_str!(
            "../../../tests/fixtures/stage5/stage5d_b2a_persistence_envelope_bad_version.json"
        ))
        .expect("bad-version Stage 5D-b2a fixture must deserialize")
    }

    fn empty_config_fixture() -> Stage5dPersistenceEnvelope {
        serde_json::from_str(include_str!(
            "../../../tests/fixtures/stage5/stage5d_b2a_persistence_envelope_empty_config.json"
        ))
        .expect("empty-config Stage 5D-b2a fixture must deserialize")
    }

    #[test]
    fn stage5d_b2a_valid_fixture_roundtrips_and_validates_checksum() {
        let envelope = valid_fixture();

        envelope
            .validate_schema_and_checksum()
            .expect("fixture checksum must match canonical payload");

        let serialized = serde_json::to_value(&envelope).expect("fixture must serialize");
        let expected: Value = serde_json::from_str(include_str!(
            "../../../tests/fixtures/stage5/stage5d_b2a_persistence_envelope.json"
        ))
        .expect("fixture JSON must parse");
        assert_eq!(serialized, expected);
    }

    #[test]
    fn stage5d_b2a_corrupt_checksum_fixture_is_rejected() {
        let envelope = corrupt_checksum_fixture();

        assert_eq!(
            envelope.validate_schema_and_checksum(),
            Err(Stage5dEnvelopeValidationError::PayloadChecksumMismatch)
        );
    }

    #[test]
    fn stage5d_b2a_schema_mismatch_is_rejected() {
        let envelope = bad_version_fixture();

        assert_eq!(
            envelope.validate_schema_and_checksum(),
            Err(Stage5dEnvelopeValidationError::RuntimePrivateSchemaMismatch)
        );
    }

    #[test]
    fn stage5d_b2a_required_identity_fields_are_rejected() {
        let envelope = empty_config_fixture();

        assert_eq!(
            envelope.validate_schema_and_checksum(),
            Err(Stage5dEnvelopeValidationError::RequiredFieldEmpty)
        );
    }
}
