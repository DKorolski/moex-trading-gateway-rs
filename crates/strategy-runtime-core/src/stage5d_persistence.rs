//! Stage 5D additive persistence freeze surface.
//!
//! Stage 5D-b2a adds versioned persistence DTOs and schema validation. It does
//! not implement runtime-private snapshot application, Stage 5C/Stage 5D
//! transitions, Redis, FINAM, transport, dispatch, or runtime-live behavior.

use broker_core::{
    BrokerAccountId, BrokerOrderId, BrokerStopOrderId, BrokerTradeId, ClientOrderId, Exchange,
    InstrumentId, Market, StrategyRequestId,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

/// Stage 5D additive freeze manifest schema version.
pub const STAGE5D_ADDITIVE_FREEZE_SCHEMA_VERSION: u16 = 1;
/// Stage 5D persistence envelope schema version.
pub const STAGE5D_PERSISTENCE_ENVELOPE_SCHEMA_VERSION: u16 = 1;
/// Stage 5D runtime-private extension schema version.
pub const STAGE5D_RUNTIME_PRIVATE_EXTENSION_SCHEMA_VERSION: u16 = 1;
/// Stage 5D riskgate persistence schema version.
pub const STAGE5D_RISKGATE_SCHEMA_VERSION: u16 = 1;
/// Stage 5D semantic strategy-state payload schema version.
pub const STAGE5D_STRATEGY_STATE_PAYLOAD_SCHEMA_VERSION: u16 = 1;

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

/// Opaque proof that the persistence envelope passed strict Stage 5D-b2a
/// schema-only restore-contract validation. Future mutation gates must consume
/// this capability, not a raw deserialized envelope.
pub struct Stage5dValidatedPersistenceEnvelope {
    envelope: Stage5dPersistenceEnvelope,
}

impl Stage5dValidatedPersistenceEnvelope {
    /// Redacted snapshot id for evidence and diagnostics.
    pub fn snapshot_id(&self) -> &str {
        &self.envelope.snapshot_id
    }

    /// Envelope schema version for evidence and diagnostics.
    pub fn schema_version(&self) -> u16 {
        self.envelope.schema_version
    }
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

/// Persistence stage marker. Stage 5D envelopes cannot be silently reused by
/// another persistence generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage5dPersistenceStage {
    Stage5d,
}

/// Runtime strategy kind bound to the persisted snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage5dStrategyKind {
    HybridIntraday,
}

/// Strict Stage 5D-owned instrument binding. This prevents unknown fields in
/// broker-core `InstrumentId` JSON from being silently discarded before
/// checksum validation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stage5dInstrumentBinding {
    pub symbol: String,
    pub venue_symbol: Option<String>,
    pub exchange: Exchange,
    pub market: Market,
}

impl Stage5dInstrumentBinding {
    /// Convert the strict binding into the broker-neutral core instrument id.
    pub fn to_instrument_id(&self) -> InstrumentId {
        InstrumentId {
            symbol: self.symbol.clone(),
            venue_symbol: self.venue_symbol.clone(),
            exchange: self.exchange.clone(),
            market: self.market.clone(),
        }
    }
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
}

/// Stable entry-style enum for Stage 5D persistence schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage5dEntryStyle {
    Market,
    Bracket,
}

/// Stable reason enum for Stage 5D persistence schema.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage5dLifecycleReason {
    MorningMeanReversionLong,
    MorningMeanReversionShort,
    BreakoutLong,
    BreakoutShort,
    BreakoutEodExit,
    BreakoutStop2Long,
    BreakoutStop1Long,
    BreakoutStop2Short,
    BreakoutStop1Short,
    MeanRevTimeCutoff,
    WaitfixOvernightExit,
}

/// Snapshot binding that prevents a valid envelope from being restored against
/// a different account, instrument, runtime profile or protocol generation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stage5dSnapshotBinding {
    pub stage: Stage5dPersistenceStage,
    pub strategy_kind: Stage5dStrategyKind,
    pub strategy_id: String,
    pub account_id: BrokerAccountId,
    pub instrument_id: Stage5dInstrumentBinding,
    pub profile_binding: String,
    pub broker_protocol_schema_version: u16,
    pub runtime_state_schema_version: u16,
    pub stage5c_compat_config_fingerprint: String,
    pub stage5d_canonical_config_fingerprint: String,
    pub source_commit_or_build_id: String,
    pub created_at_ts_utc: DateTime<Utc>,
}

/// Canonical semantic StrategyState payload. This is stored as JSON to avoid
/// exporting runtime-private source structures through the public Stage 5D API.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stage5dStrategyStatePayload {
    pub schema_version: u16,
    pub strategy_state_json: Value,
}

/// Strict Stage 5D semantic StrategyState payload root.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Stage5dSemanticStrategyStateV1 {
    HybridIntradayRuntime(Stage5dHybridIntradayStrategyStateV1),
}

/// Strict Stage 5D mirror of the accepted HybridIntradayRuntime semantic
/// `StrategyState` fields. This is a public persistence schema, not a runtime
/// private source struct and not a mutation capability.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stage5dHybridIntradayStrategyStateV1 {
    pub active_cycle_id: Option<String>,
    pub next_cycle_seq: u32,
    pub last_position_qty: f64,
    pub current_owner: Option<Stage5dOwner>,
    pub current_side: Option<Stage5dSide>,
    pub pending_entry_owner: Option<Stage5dOwner>,
    pub pending_entry_side: Option<Stage5dSide>,
    pub pending_entry_cycle_id: Option<String>,
    pub pending_entry_request_id: Option<StrategyRequestId>,
    pub pending_entry_created_ts_utc: Option<i64>,
    pub deferred_entry_owner: Option<Stage5dOwner>,
    pub deferred_entry_side: Option<Stage5dSide>,
    pub deferred_entry_cycle_id: Option<String>,
    pub deferred_entry_entry_style: Option<Stage5dEntryStyle>,
    pub deferred_entry_reason: Option<Stage5dLifecycleReason>,
    pub deferred_entry_stop_price: Option<f64>,
    pub deferred_entry_take_price: Option<f64>,
    pub deferred_entry_ts_utc: Option<i64>,
    pub deferred_entry_request_id: Option<StrategyRequestId>,
    pub pending_exit_request_id: Option<StrategyRequestId>,
    pub pending_exit_created_ts_utc: Option<i64>,
    pub deferred_exit_owner: Option<Stage5dOwner>,
    pub deferred_exit_reason: Option<Stage5dLifecycleReason>,
    pub deferred_exit_cycle_id: Option<String>,
    pub deferred_exit_ts_utc: Option<i64>,
    pub deferred_exit_request_id: Option<StrategyRequestId>,
    pub pending_tp_request_id: Option<StrategyRequestId>,
    pub pending_tp_created_ts_utc: Option<i64>,
    pub pending_sl_request_id: Option<StrategyRequestId>,
    pub pending_sl_created_ts_utc: Option<i64>,
    pub tp_order_id: Option<BrokerOrderId>,
    pub sl_stop_order_id: Option<BrokerStopOrderId>,
    pub sl_exchange_order_id: Option<BrokerOrderId>,
    pub sl_triggered_ts: Option<i64>,
    pub mr_take_price: Option<f64>,
    pub mr_stop_price: Option<f64>,
    pub repair_deadline_ts: Option<i64>,
    pub next_repair_at_ts: Option<i64>,
    pub repair_backoff_level: u32,
    pub repair_attempts: u32,
    pub safe_mode_close_only: bool,
    pub safe_mode_reason: Option<String>,
    pub entry_ready: bool,
    pub last_bar_close: Option<f64>,
    pub prev_day_close: Option<f64>,
    pub last_day_local: Option<String>,
    pub current_day_high: Option<f64>,
    pub current_day_low: Option<f64>,
    pub current_day_close: Option<f64>,
    pub prev_day_range: Option<f64>,
    pub prev_day_return: Option<f64>,
    pub day_before_close: Option<f64>,
    pub today_start_local: Option<String>,
    pub was_long_today: bool,
    pub was_short_today: bool,
    pub overnight_exit_armed_date: Option<String>,
    pub risk_gate_shadow_session_date: Option<String>,
    pub risk_gate_shadow_pnl_points: f64,
    pub risk_gate_shadow_trade_count: u32,
    pub risk_gate_shadow_entry_ts_utc: Option<i64>,
    pub risk_gate_shadow_entry_price: Option<f64>,
    pub risk_gate_shadow_side: Option<Stage5dSide>,
    pub risk_gate_shadow_target_price: Option<f64>,
    pub risk_gate_shadow_stop_price: Option<f64>,
    pub risk_gate_pending_session_date: Option<String>,
    pub risk_gate_pending_shadow_pnl_points: f64,
    pub risk_gate_pending_shadow_trade_count: u32,
    pub risk_gate_mr_enabled_current_session: Option<bool>,
    pub risk_gate_rolling_sum_lb120: Option<f64>,
    pub risk_gate_last_finalized_session_date: Option<String>,
    pub risk_gate_ledger_rows_count: usize,
}

impl Stage5dHybridIntradayStrategyStateV1 {
    fn active_cycle_id_is_valid(&self) -> bool {
        self.active_cycle_id
            .as_deref()
            .map(stage5d_cycle_id_is_valid)
            .unwrap_or(true)
    }
}

/// Lifecycle watermarks that bind restored state to processed data progress.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stage5dLifecycleWatermarks {
    pub persisted_event_watermark: Option<String>,
    pub last_semantic_bar_ts: Option<DateTime<Utc>>,
    pub last_broker_event_ts: Option<DateTime<Utc>>,
}

/// Broker-neutral typed recovery indexes. Namespaces are intentionally
/// separated; `ClientOrderId` never substitutes for `StrategyRequestId`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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
#[serde(deny_unknown_fields)]
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
#[serde(deny_unknown_fields)]
pub struct Stage5dPartialEntryTimer {
    pub partial_started_at_ms: i64,
}

/// Runtime-private pending exit schema.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stage5dPendingExitExtension {
    pub owner: Stage5dOwner,
    pub reason: Stage5dLifecycleReason,
    pub request_id: StrategyRequestId,
}

/// Runtime-private bracket reconciliation timer schema.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stage5dBracketReconciliationTimer {
    pub bracket_terminal_reconcile_started_ms: i64,
}

/// Runtime-private cleanup retry schema.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stage5dCleanupRetryState {
    pub cleanup_stop_retry_attempts: u32,
}

/// Non-authoritative expected broker-object hints. Actual working sets must be
/// rebuilt from broker truth before callbacks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stage5dExpectedWorkingSets {
    pub expected_working_order_ids: Vec<BrokerOrderId>,
    pub expected_working_stop_order_ids: Vec<BrokerStopOrderId>,
}

/// Riskgate finalization outbox record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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
#[serde(deny_unknown_fields)]
pub struct Stage5dRuntimePrivateExtension {
    pub schema_version: u16,
    pub pending_entry: Option<Stage5dPendingEntryExtension>,
    pub partial_entry_timer: Option<Stage5dPartialEntryTimer>,
    pub pending_exit: Option<Stage5dPendingExitExtension>,
    pub bracket_reconciliation_timer: Option<Stage5dBracketReconciliationTimer>,
    pub cleanup_retry_state: Option<Stage5dCleanupRetryState>,
    pub expected_working_sets: Stage5dExpectedWorkingSets,
    pub last_processed_bar_ts: Option<DateTime<Utc>>,
    pub runtime_pending_finalizations: Vec<Stage5dRiskGateFinalizationOutboxRecord>,
}

/// Riskgate identity section.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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
#[serde(deny_unknown_fields)]
pub struct Stage5dRiskGateMaterializedState {
    pub mr_enabled_current_session: bool,
    pub mr_enabled_next_session: bool,
    pub rolling_sum_lb120: String,
    pub last_finalized_session_date: Option<String>,
    pub ledger_rows_count: u64,
}

/// Riskgate persistence DTO. This is schema-only in Stage 5D-b2a.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stage5dRiskGatePersistence {
    pub schema_version: u16,
    pub identity: Stage5dRiskGateIdentity,
    pub materialized_state: Stage5dRiskGateMaterializedState,
    pub ledger_tail_hash: String,
    pub durable_finalization_outbox: Vec<Stage5dRiskGateFinalizationOutboxRecord>,
}

/// Versioned Stage 5D persistence envelope DTO.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stage5dPersistenceEnvelope {
    pub schema_version: u16,
    pub snapshot_id: String,
    pub snapshot_revision: u64,
    pub previous_revision: Option<u64>,
    pub write_generation: u64,
    pub persisted_at_ts_utc: DateTime<Utc>,
    pub timestamp_units: Stage5dTimestampUnits,
    pub canonical_config_fingerprint: String,
    pub binding: Stage5dSnapshotBinding,
    pub strategy_state: Stage5dStrategyStatePayload,
    pub payload_checksum_sha256: String,
    pub lifecycle_watermarks: Stage5dLifecycleWatermarks,
    pub recovery_indexes: Stage5dRecoveryIndexes,
    pub runtime_private_extension: Stage5dRuntimePrivateExtension,
    pub riskgate: Stage5dRiskGatePersistence,
}

impl Stage5dPersistenceEnvelope {
    /// Strictly decode and validate an envelope from JSON. Unknown fields at any
    /// Stage 5D DTO layer fail closed before checksum validation.
    pub fn from_json_str_strict(payload: &str) -> Result<Self, Stage5dEnvelopeValidationError> {
        let envelope: Self = serde_json::from_str(payload)
            .map_err(|_| Stage5dEnvelopeValidationError::DeserializationFailed)?;
        envelope.validate_schema_and_checksum()?;
        Ok(envelope)
    }

    /// Strictly decode, validate checksum/schema and prove schema-only
    /// restore-contract consistency without mutating runtime state.
    pub fn validated_from_json_str_strict(
        payload: &str,
    ) -> Result<Stage5dValidatedPersistenceEnvelope, Stage5dEnvelopeValidationError> {
        let envelope = Self::from_json_str_strict(payload)?;
        envelope.validate_restore_contract_schema_only()
    }

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
        if self.strategy_state.schema_version != STAGE5D_STRATEGY_STATE_PAYLOAD_SCHEMA_VERSION {
            return Err(Stage5dEnvelopeValidationError::StrategyStateSchemaMismatch);
        }
        if self.snapshot_id.is_empty()
            || self.canonical_config_fingerprint.is_empty()
            || self.binding.strategy_id.is_empty()
            || self.binding.profile_binding.is_empty()
            || self.binding.stage5c_compat_config_fingerprint.is_empty()
            || self.binding.stage5d_canonical_config_fingerprint.is_empty()
            || self.binding.source_commit_or_build_id.is_empty()
            || self.strategy_state.strategy_state_json.is_null()
            || self.riskgate.identity.strategy_id.is_empty()
            || self.riskgate.identity.profile_id.is_empty()
            || self.riskgate.ledger_tail_hash.is_empty()
        {
            return Err(Stage5dEnvelopeValidationError::RequiredFieldEmpty);
        }
        if self.binding.broker_protocol_schema_version == 0
            || self.binding.runtime_state_schema_version == 0
        {
            return Err(Stage5dEnvelopeValidationError::RequiredFieldEmpty);
        }
        let expected = self.compute_payload_checksum_sha256()?;
        if self.payload_checksum_sha256 != expected {
            return Err(Stage5dEnvelopeValidationError::PayloadChecksumMismatch);
        }
        Ok(())
    }

    /// Validate semantic payload and cross-section consistency required before
    /// any future restore mutation gate can consume this envelope.
    pub fn validate_restore_contract_schema_only(
        &self,
    ) -> Result<Stage5dValidatedPersistenceEnvelope, Stage5dEnvelopeValidationError> {
        self.validate_schema_and_checksum()?;
        if self.canonical_config_fingerprint != self.binding.stage5d_canonical_config_fingerprint {
            return Err(Stage5dEnvelopeValidationError::BindingMismatch);
        }
        if self.binding.instrument_id.symbol.is_empty()
            || self.binding.account_id.as_str().is_empty()
            || self.riskgate.identity.strategy_id != self.binding.strategy_id
            || self.riskgate.identity.profile_id != self.binding.profile_binding
        {
            return Err(Stage5dEnvelopeValidationError::BindingMismatch);
        }

        let semantic: Stage5dSemanticStrategyStateV1 =
            serde_json::from_value(self.strategy_state.strategy_state_json.clone())
                .map_err(|_| Stage5dEnvelopeValidationError::SemanticStateInvalid)?;
        let canonical = serde_json::to_value(&semantic)
            .map_err(|_| Stage5dEnvelopeValidationError::SerializationFailed)?;
        if canonical != self.strategy_state.strategy_state_json {
            return Err(Stage5dEnvelopeValidationError::SemanticStateInvalid);
        }

        let Stage5dSemanticStrategyStateV1::HybridIntradayRuntime(state) = &semantic;
        self.validate_hybrid_state_consistency(state)?;

        Ok(Stage5dValidatedPersistenceEnvelope {
            envelope: self.clone(),
        })
    }

    fn validate_hybrid_state_consistency(
        &self,
        state: &Stage5dHybridIntradayStrategyStateV1,
    ) -> Result<(), Stage5dEnvelopeValidationError> {
        if !state.active_cycle_id_is_valid()
            || state
                .pending_entry_cycle_id
                .as_deref()
                .is_some_and(|cycle| !stage5d_cycle_id_is_valid(cycle))
            || state
                .deferred_entry_cycle_id
                .as_deref()
                .is_some_and(|cycle| !stage5d_cycle_id_is_valid(cycle))
            || state
                .deferred_exit_cycle_id
                .as_deref()
                .is_some_and(|cycle| !stage5d_cycle_id_is_valid(cycle))
        {
            return Err(Stage5dEnvelopeValidationError::SemanticStateInvalid);
        }

        let pending_requests = &self.recovery_indexes.pending_requests;
        if let Some(pending_entry) = &self.runtime_private_extension.pending_entry {
            let Some(request_id) = pending_entry.request_id else {
                return Err(Stage5dEnvelopeValidationError::PendingStateInconsistent);
            };
            if state.pending_entry_request_id != Some(request_id)
                || state.pending_entry_owner != Some(pending_entry.owner)
                || state.pending_entry_side != Some(pending_entry.side)
                || state.pending_entry_cycle_id.is_none()
                || state.pending_entry_created_ts_utc.is_none()
                || !pending_requests.contains(&request_id)
            {
                return Err(Stage5dEnvelopeValidationError::PendingStateInconsistent);
            }

            if self.runtime_private_extension.partial_entry_timer.is_some() {
                let target_qty: f64 = pending_entry
                    .target_qty
                    .parse()
                    .map_err(|_| Stage5dEnvelopeValidationError::PendingStateInconsistent)?;
                let filled_qty = state.last_position_qty.abs();
                if !(filled_qty > 0.0 && filled_qty < target_qty) {
                    return Err(Stage5dEnvelopeValidationError::PendingStateInconsistent);
                }
            }
        }

        if let Some(pending_exit) = &self.runtime_private_extension.pending_exit {
            if state.pending_exit_request_id != Some(pending_exit.request_id)
                || state.pending_exit_created_ts_utc.is_none()
                || !pending_requests.contains(&pending_exit.request_id)
            {
                return Err(Stage5dEnvelopeValidationError::PendingStateInconsistent);
            }
        } else if state.pending_exit_request_id.is_some() {
            return Err(Stage5dEnvelopeValidationError::PendingStateInconsistent);
        }

        Ok(())
    }
}

fn stage5d_cycle_id_is_valid(value: &str) -> bool {
    value.len() == 10 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// Stage 5D envelope validation errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage5dEnvelopeValidationError {
    DeserializationFailed,
    EnvelopeSchemaMismatch,
    RuntimePrivateSchemaMismatch,
    RiskGateSchemaMismatch,
    StrategyStateSchemaMismatch,
    RequiredFieldEmpty,
    PayloadChecksumMismatch,
    SemanticStateInvalid,
    BindingMismatch,
    PendingStateInconsistent,
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

        assert_eq!(envelope.binding.stage, Stage5dPersistenceStage::Stage5d);
        assert_eq!(
            envelope.binding.strategy_kind,
            Stage5dStrategyKind::HybridIntraday
        );
        assert!(envelope
            .strategy_state
            .strategy_state_json
            .get("HybridIntradayRuntime")
            .is_some());
        let semantic: Stage5dSemanticStrategyStateV1 =
            serde_json::from_value(envelope.strategy_state.strategy_state_json.clone())
                .expect("semantic state must be strict Stage 5D state");
        let Stage5dSemanticStrategyStateV1::HybridIntradayRuntime(state) = semantic;
        assert_eq!(state.active_cycle_id.as_deref(), Some("6a4badd811"));
        assert_eq!(state.last_position_qty, 0.5);
        assert_eq!(
            state.pending_entry_request_id,
            Some(envelope.recovery_indexes.pending_requests[0])
        );
        assert_eq!(
            envelope
                .runtime_private_extension
                .pending_entry
                .as_ref()
                .expect("fixture pending entry")
                .entry_style,
            Stage5dEntryStyle::Market
        );
        assert!(envelope.runtime_private_extension.pending_exit.is_none());

        envelope
            .validate_schema_and_checksum()
            .expect("fixture checksum must match canonical payload");
        let validated = envelope
            .validate_restore_contract_schema_only()
            .expect("fixture must pass schema-only restore contract");
        assert_eq!(validated.snapshot_id(), "SNAP_STAGE5D_B2A_0001");

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

    fn strict_fixture_with_inserted_field(anchor: &str, inserted_field: &str) -> String {
        let payload =
            include_str!("../../../tests/fixtures/stage5/stage5d_b2a_persistence_envelope.json");
        payload.replacen(anchor, &format!("{anchor}\n{inserted_field}"), 1)
    }

    #[test]
    fn stage5d_b2a_unknown_root_field_is_rejected_before_checksum() {
        let payload = strict_fixture_with_inserted_field(
            "  \"schema_version\": 1,",
            "  \"unsupported_runtime_state\": {\"pending_real_order\": true},",
        );

        assert_eq!(
            Stage5dPersistenceEnvelope::from_json_str_strict(&payload),
            Err(Stage5dEnvelopeValidationError::DeserializationFailed)
        );
    }

    #[test]
    fn stage5d_b2a_unknown_runtime_private_field_is_rejected_before_checksum() {
        let payload = strict_fixture_with_inserted_field(
            "  \"runtime_private_extension\": {\n    \"schema_version\": 1,",
            "    \"unsupported_runtime_private\": true,",
        );

        assert_eq!(
            Stage5dPersistenceEnvelope::from_json_str_strict(&payload),
            Err(Stage5dEnvelopeValidationError::DeserializationFailed)
        );
    }

    #[test]
    fn stage5d_b2a_unknown_riskgate_field_is_rejected_before_checksum() {
        let payload = strict_fixture_with_inserted_field(
            "  \"riskgate\": {\n    \"schema_version\": 1,",
            "    \"unsupported_riskgate_state\": true,",
        );

        assert_eq!(
            Stage5dPersistenceEnvelope::from_json_str_strict(&payload),
            Err(Stage5dEnvelopeValidationError::DeserializationFailed)
        );
    }

    #[test]
    fn stage5d_b2a_unknown_nested_outbox_field_is_rejected_before_checksum() {
        let payload = strict_fixture_with_inserted_field(
            "        \"session_date\": \"2026-07-14\",",
            "        \"unsupported_outbox_field\": true,",
        );

        assert_eq!(
            Stage5dPersistenceEnvelope::from_json_str_strict(&payload),
            Err(Stage5dEnvelopeValidationError::DeserializationFailed)
        );
    }

    #[test]
    fn stage5d_b2a_unknown_nested_instrument_field_is_rejected_before_checksum() {
        let payload = strict_fixture_with_inserted_field(
            "      \"symbol\": \"IMOEXF\",",
            "      \"unsupported_instrument_binding\": true,",
        );

        assert_eq!(
            Stage5dPersistenceEnvelope::from_json_str_strict(&payload),
            Err(Stage5dEnvelopeValidationError::DeserializationFailed)
        );
    }

    fn fixture_with_mutated_state(mutator: impl FnOnce(&mut Value)) -> Stage5dPersistenceEnvelope {
        let mut envelope = valid_fixture();
        mutator(&mut envelope.strategy_state.strategy_state_json);
        envelope.payload_checksum_sha256 = envelope
            .compute_payload_checksum_sha256()
            .expect("checksum recomputation must succeed");
        envelope
    }

    #[test]
    fn stage5d_b2a_scalar_strategy_state_payload_is_rejected() {
        let mut envelope = valid_fixture();
        envelope.strategy_state.strategy_state_json = Value::Bool(true);
        envelope.payload_checksum_sha256 = envelope
            .compute_payload_checksum_sha256()
            .expect("checksum recomputation must succeed");

        assert_eq!(
            envelope.validate_restore_contract_schema_only().map(|_| ()),
            Err(Stage5dEnvelopeValidationError::SemanticStateInvalid)
        );
    }

    #[test]
    fn stage5d_b2a_unknown_semantic_state_field_is_rejected() {
        let envelope = fixture_with_mutated_state(|state| {
            state["HybridIntradayRuntime"]["pending_real_order"] = Value::Bool(true);
        });

        assert_eq!(
            envelope.validate_restore_contract_schema_only().map(|_| ()),
            Err(Stage5dEnvelopeValidationError::SemanticStateInvalid)
        );
    }

    #[test]
    fn stage5d_b2a_wrong_semantic_state_variant_is_rejected() {
        let mut envelope = valid_fixture();
        envelope.strategy_state.strategy_state_json = serde_json::json!({"Idle": {}});
        envelope.payload_checksum_sha256 = envelope
            .compute_payload_checksum_sha256()
            .expect("checksum recomputation must succeed");

        assert_eq!(
            envelope.validate_restore_contract_schema_only().map(|_| ()),
            Err(Stage5dEnvelopeValidationError::SemanticStateInvalid)
        );
    }

    #[test]
    fn stage5d_b2a_misspelled_semantic_state_field_is_rejected() {
        let envelope = fixture_with_mutated_state(|state| {
            if let Value::Object(fields) = &mut state["HybridIntradayRuntime"] {
                let active_cycle = fields
                    .remove("active_cycle_id")
                    .expect("fixture has active cycle");
                fields.insert("misspelled_active_cycle".to_string(), active_cycle);
            }
        });

        assert_eq!(
            envelope.validate_restore_contract_schema_only().map(|_| ()),
            Err(Stage5dEnvelopeValidationError::SemanticStateInvalid)
        );
    }

    #[test]
    fn stage5d_b2a_invalid_semantic_field_type_is_rejected() {
        let envelope = fixture_with_mutated_state(|state| {
            state["HybridIntradayRuntime"]["last_position_qty"] =
                Value::String("not-a-number".to_string());
        });

        assert_eq!(
            envelope.validate_restore_contract_schema_only().map(|_| ()),
            Err(Stage5dEnvelopeValidationError::SemanticStateInvalid)
        );
    }

    #[test]
    fn stage5d_b2a_inconsistent_pending_entry_is_rejected() {
        let envelope = fixture_with_mutated_state(|state| {
            state["HybridIntradayRuntime"]["pending_entry_cycle_id"] = Value::Null;
        });

        assert_eq!(
            envelope.validate_restore_contract_schema_only().map(|_| ()),
            Err(Stage5dEnvelopeValidationError::PendingStateInconsistent)
        );
    }
}
