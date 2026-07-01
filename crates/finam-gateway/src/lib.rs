//! FINAM gateway primitives for read-only/shadow and dry order-path modes.
//!
//! This crate intentionally does not contain real FINAM order placement/cancel,
//! real broker ACK lifecycle, stop/SLTP, bracket, or runtime adaptation. It
//! prepares the Redis/shadow publication boundary for health, readiness,
//! broker-truth snapshots, read-only market data, retention, and
//! degraded/stopped status reporting, plus dry runtime-bridge and mock
//! order-path contracts for typed decode, idempotency validation, synthetic ACK
//! publication, and approved-only execution simulation.

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use broker_core::account::PortfolioSnapshot;
use broker_core::command::{
    BrokerCommand, CommandAck, CommandAckReason, CommandAckReasonCode, CommandAckStatus,
};
use broker_core::envelope::{Envelope, MessageType, SCHEMA_VERSION};
use broker_core::event::MarketDataEvent;
use broker_core::ids::StrategyRequestId;
use broker_core::order::Order;
use broker_core::readiness::{BrokerReadiness, ReadinessPhase, ReadinessReason};
use broker_core::{
    OperatorArm, OperatorDisarmDecision, OperatorDisarmSignal, OrderPathErrorKind, OrderPathEvent,
    OrderPathState, OrderPathStore, OrderPathStoreError, OrderPathTransitionError,
    OutgoingOrderComment, PreflightApprovedCancelOrder, PreflightApprovedPlaceOrder,
};
use chrono::{DateTime, Utc};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatewayConfig {
    pub source: String,
    pub redis: RedisStreamConfig,
    pub features: GatewayFeatureSet,
}

impl Default for GatewayConfig {
    fn default() -> Self {
        Self {
            source: "finam-gateway".to_string(),
            redis: RedisStreamConfig::default(),
            features: GatewayFeatureSet::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RedisStreamConfig {
    pub url: String,
    pub health_stream: String,
    pub readiness_stream: String,
    pub portfolio_stream: String,
    pub order_snapshot_stream: String,
    pub market_data_stream: String,
    #[serde(default = "default_command_ack_stream")]
    pub command_ack_stream: String,
    pub runtime_bridge_dlq_stream: String,
    pub retention: RedisRetentionConfig,
}

impl Default for RedisStreamConfig {
    fn default() -> Self {
        Self {
            url: "redis://127.0.0.1:6379/".to_string(),
            health_stream: "finam:health".to_string(),
            readiness_stream: "finam:readiness".to_string(),
            portfolio_stream: "finam:portfolio".to_string(),
            order_snapshot_stream: "finam:orders:snapshot".to_string(),
            market_data_stream: "finam:market-data".to_string(),
            command_ack_stream: default_command_ack_stream(),
            runtime_bridge_dlq_stream: "finam:runtime-bridge:dlq".to_string(),
            retention: RedisRetentionConfig::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RedisRetentionConfig {
    pub health_maxlen: Option<usize>,
    pub readiness_maxlen: Option<usize>,
    pub portfolio_maxlen: Option<usize>,
    pub order_snapshot_maxlen: Option<usize>,
    pub market_data_maxlen: Option<usize>,
    #[serde(default = "default_command_ack_maxlen")]
    pub command_ack_maxlen: Option<usize>,
    pub runtime_bridge_dlq_maxlen: Option<usize>,
}

impl Default for RedisRetentionConfig {
    fn default() -> Self {
        Self {
            health_maxlen: Some(1_000),
            readiness_maxlen: Some(1_000),
            portfolio_maxlen: Some(1_000),
            order_snapshot_maxlen: Some(1_000),
            market_data_maxlen: Some(10_000),
            command_ack_maxlen: default_command_ack_maxlen(),
            runtime_bridge_dlq_maxlen: Some(1_000),
        }
    }
}

fn default_command_ack_stream() -> String {
    "finam:command-acks".to_string()
}

fn default_command_ack_maxlen() -> Option<usize> {
    Some(1_000)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatewayFeatureSet {
    pub publish_health: bool,
    pub publish_readiness: bool,
    pub publish_snapshots: bool,
    pub publish_market_data_from_readonly: bool,
    pub command_consumer_enabled: bool,
    pub order_placement_enabled: bool,
    pub cancel_enabled: bool,
    pub stop_sltp_bracket_enabled: bool,
}

impl Default for GatewayFeatureSet {
    fn default() -> Self {
        Self {
            publish_health: true,
            publish_readiness: true,
            publish_snapshots: true,
            publish_market_data_from_readonly: true,
            command_consumer_enabled: false,
            order_placement_enabled: false,
            cancel_enabled: false,
            stop_sltp_bracket_enabled: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RealOrderEndpointGateBlock {
    M3a11PreEndpointReviewRequired,
    CommandConsumerEnabled,
    OrderPlacementEnabled,
    CancelEnabled,
    StopSltpBracketEnabled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RuntimeCommandAckIdPolicy {
    RedactedRuntimeAckOnly,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RealOrderEndpointGateDecision {
    pub endpoint_calls_allowed: bool,
    pub blocking_reasons: Vec<RealOrderEndpointGateBlock>,
    pub runtime_ack_id_policy: RuntimeCommandAckIdPolicy,
}

const REAL_ORDER_ENDPOINT_IMPLEMENTATION_REVIEW_ACCEPTED: bool = false;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EndpointGateApproved {
    _private: (),
}

impl EndpointGateApproved {
    pub fn try_from_decision(
        decision: &RealOrderEndpointGateDecision,
    ) -> Result<Self, EndpointGateApprovalError> {
        if !REAL_ORDER_ENDPOINT_IMPLEMENTATION_REVIEW_ACCEPTED {
            let mut blocked_decision = decision.clone();
            blocked_decision.endpoint_calls_allowed = false;
            if !blocked_decision
                .blocking_reasons
                .contains(&RealOrderEndpointGateBlock::M3a11PreEndpointReviewRequired)
            {
                blocked_decision
                    .blocking_reasons
                    .push(RealOrderEndpointGateBlock::M3a11PreEndpointReviewRequired);
            }
            return Err(EndpointGateApprovalError::Blocked {
                decision: blocked_decision,
            });
        }

        if decision.endpoint_calls_allowed && decision.blocking_reasons.is_empty() {
            Ok(Self { _private: () })
        } else {
            Err(EndpointGateApprovalError::Blocked {
                decision: decision.clone(),
            })
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EndpointGateApprovalError {
    #[error("FINAM real order endpoint gate is blocked: {decision:?}")]
    Blocked {
        decision: RealOrderEndpointGateDecision,
    },
}

#[async_trait]
pub trait FinamRealOrderEndpointTransport: Send {
    async fn place_order_endpoint(
        &mut self,
        gate: &EndpointGateApproved,
        spec: broker_finam::FinamPlaceOrderRequestSpec,
    ) -> Result<broker_finam::FinamOrderEndpointMappedResult, broker_finam::FinamOrderExecutionError>;

    async fn cancel_order_endpoint(
        &mut self,
        gate: &EndpointGateApproved,
        spec: broker_finam::FinamCancelOrderRequestSpec,
    ) -> Result<broker_finam::FinamOrderEndpointMappedResult, broker_finam::FinamOrderExecutionError>;
}

impl GatewayFeatureSet {
    pub fn real_order_endpoint_gate_decision(&self) -> RealOrderEndpointGateDecision {
        let mut blocking_reasons = vec![RealOrderEndpointGateBlock::M3a11PreEndpointReviewRequired];
        if self.command_consumer_enabled {
            blocking_reasons.push(RealOrderEndpointGateBlock::CommandConsumerEnabled);
        }
        if self.order_placement_enabled {
            blocking_reasons.push(RealOrderEndpointGateBlock::OrderPlacementEnabled);
        }
        if self.cancel_enabled {
            blocking_reasons.push(RealOrderEndpointGateBlock::CancelEnabled);
        }
        if self.stop_sltp_bracket_enabled {
            blocking_reasons.push(RealOrderEndpointGateBlock::StopSltpBracketEnabled);
        }

        RealOrderEndpointGateDecision {
            endpoint_calls_allowed: false,
            blocking_reasons,
            runtime_ack_id_policy: RuntimeCommandAckIdPolicy::RedactedRuntimeAckOnly,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatewayHealth {
    pub status: GatewayHealthStatus,
    pub checked_ts: DateTime<Utc>,
    pub redis_configured: bool,
    pub command_consumer_enabled: bool,
    pub order_placement_enabled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GatewayHealthStatus {
    Starting,
    ReadOnly,
    Degraded,
    Stopped,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrderSnapshot {
    pub orders: Vec<Order>,
    pub active_orders_count: usize,
    pub terminal_orders_count: usize,
    pub blocking_unknown_status_present: bool,
    pub received_ts: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadonlySnapshotSummary {
    pub cash_count: usize,
    pub positions_count: usize,
    pub orders_count: usize,
    pub active_orders_count: usize,
    pub terminal_orders_count: usize,
    pub blocking_unknown_status_present: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReadonlyReconciliationReport {
    pub health: GatewayHealth,
    pub readiness: BrokerReadiness,
    pub summary: ReadonlySnapshotSummary,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CommandConsumerMode {
    Absent,
    FeatureDisabled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RedisStreamEntry {
    pub stream: String,
    pub payload: String,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeBridgeStreamEntry {
    pub stream: String,
    pub entry_id: String,
    pub payload: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RuntimeBridgePayloadKind {
    Health,
    Readiness,
    PortfolioSnapshot,
    OrderSnapshot,
    MarketData,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RuntimeBridgeDlqReason {
    UnknownStream,
    InvalidJson,
    MissingSchemaVersion,
    UnsupportedSchemaVersion {
        expected: u16,
        actual: u64,
    },
    MissingMessageType,
    MissingPayload,
    UnsupportedMessageType,
    MessageTypeMismatch {
        expected: MessageType,
        actual: Option<MessageType>,
    },
    TypedDecodeFailed {
        expected: RuntimeBridgePayloadKind,
    },
    RawOrderCommentPresent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeBridgeDeadLetter {
    pub stream: String,
    pub entry_id: String,
    pub reason: RuntimeBridgeDlqReason,
    pub payload_len: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeBridgeDlqRecord {
    pub schema_version: u16,
    pub ts_utc: DateTime<Utc>,
    pub source: String,
    pub consumer_group: String,
    pub consumer_name: String,
    pub dead_letter: RuntimeBridgeDeadLetter,
}

impl RuntimeBridgeDlqRecord {
    pub fn new(
        source: impl Into<String>,
        consumer_group: impl Into<String>,
        consumer_name: impl Into<String>,
        dead_letter: RuntimeBridgeDeadLetter,
    ) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            ts_utc: Utc::now(),
            source: source.into(),
            consumer_group: consumer_group.into(),
            consumer_name: consumer_name.into(),
            dead_letter,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RuntimeBridgeConsumeOutcome {
    Accepted {
        kind: RuntimeBridgePayloadKind,
        entry_id: String,
    },
    DuplicateBar {
        entry_id: String,
        bar_key: String,
    },
    DeadLetter(RuntimeBridgeDeadLetter),
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeBridgeConsumerMetrics {
    pub entries_seen: u64,
    pub accepted_count: u64,
    pub duplicate_bar_count: u64,
    pub dlq_count: u64,
    pub health_count: u64,
    pub readiness_count: u64,
    pub portfolio_snapshot_count: u64,
    pub order_snapshot_count: u64,
    pub market_data_count: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RuntimeBridgeDryReadinessPhase {
    WaitingForInputs,
    DryReady,
    Degraded,
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RuntimeBridgeDryReadinessReason {
    HealthMissing,
    HealthNotReadOnly { status: GatewayHealthStatus },
    GatewayReadinessMissing,
    GatewayReadinessNotReconciliation { phase: ReadinessPhase },
    PortfolioSnapshotMissing,
    OrderSnapshotMissing,
    MarketDataMissing,
    UnknownOpenOrders,
    DeadLettersPresent { count: u64 },
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeBridgeReadinessState {
    pub health_seen: bool,
    pub health_status: Option<GatewayHealthStatus>,
    pub gateway_readiness_seen: bool,
    pub gateway_readiness_phase: Option<ReadinessPhase>,
    pub portfolio_snapshot_seen: bool,
    pub order_snapshot_seen: bool,
    pub market_data_seen: bool,
    pub blocking_unknown_orders: bool,
    pub dead_letter_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeBridgeReadinessDecision {
    pub schema_version: u16,
    pub live_ready: bool,
    pub phase: RuntimeBridgeDryReadinessPhase,
    pub reasons: Vec<RuntimeBridgeDryReadinessReason>,
    pub state: RuntimeBridgeReadinessState,
}

#[derive(Debug, Clone)]
pub struct RuntimeBridgeReadinessSimulator {
    config: RedisStreamConfig,
    state: RuntimeBridgeReadinessState,
}

impl RuntimeBridgeReadinessSimulator {
    pub fn new(config: RedisStreamConfig) -> Self {
        Self {
            config,
            state: RuntimeBridgeReadinessState::default(),
        }
    }

    pub fn from_gateway_config(config: &GatewayConfig) -> Self {
        Self::new(config.redis.clone())
    }

    pub fn observe_entry(
        &mut self,
        entry: &RuntimeBridgeStreamEntry,
    ) -> Result<(), RuntimeBridgeDeadLetter> {
        self.try_observe_entry(entry)
            .map_err(|reason| dead_letter(entry, reason))
    }

    pub fn observe_dead_letter(&mut self, _dead_letter: &RuntimeBridgeDeadLetter) {
        self.state.dead_letter_count += 1;
    }

    pub fn state(&self) -> &RuntimeBridgeReadinessState {
        &self.state
    }

    pub fn decision(&self) -> RuntimeBridgeReadinessDecision {
        let mut reasons = Vec::new();
        let mut has_blocker = false;
        let mut has_degraded = false;

        if !self.state.health_seen {
            reasons.push(RuntimeBridgeDryReadinessReason::HealthMissing);
        } else if let Some(status) = self.state.health_status {
            if status != GatewayHealthStatus::ReadOnly {
                reasons.push(RuntimeBridgeDryReadinessReason::HealthNotReadOnly { status });
                match status {
                    GatewayHealthStatus::Degraded | GatewayHealthStatus::Stopped => {
                        has_degraded = true;
                    }
                    GatewayHealthStatus::Starting => {}
                    GatewayHealthStatus::ReadOnly => {}
                }
            }
        }

        if !self.state.gateway_readiness_seen {
            reasons.push(RuntimeBridgeDryReadinessReason::GatewayReadinessMissing);
        } else if let Some(phase) = self.state.gateway_readiness_phase {
            if phase != ReadinessPhase::Reconciliation {
                reasons.push(
                    RuntimeBridgeDryReadinessReason::GatewayReadinessNotReconciliation { phase },
                );
                match phase {
                    ReadinessPhase::Degraded | ReadinessPhase::Stopped => has_degraded = true,
                    ReadinessPhase::Blocked => has_blocker = true,
                    _ => {}
                }
            }
        }

        if !self.state.portfolio_snapshot_seen {
            reasons.push(RuntimeBridgeDryReadinessReason::PortfolioSnapshotMissing);
        }
        if !self.state.order_snapshot_seen {
            reasons.push(RuntimeBridgeDryReadinessReason::OrderSnapshotMissing);
        }
        if !self.state.market_data_seen {
            reasons.push(RuntimeBridgeDryReadinessReason::MarketDataMissing);
        }
        if self.state.blocking_unknown_orders {
            reasons.push(RuntimeBridgeDryReadinessReason::UnknownOpenOrders);
            has_blocker = true;
        }
        if self.state.dead_letter_count > 0 {
            reasons.push(RuntimeBridgeDryReadinessReason::DeadLettersPresent {
                count: self.state.dead_letter_count,
            });
            has_blocker = true;
        }

        let phase = if has_blocker {
            RuntimeBridgeDryReadinessPhase::Blocked
        } else if has_degraded {
            RuntimeBridgeDryReadinessPhase::Degraded
        } else if reasons.is_empty() {
            RuntimeBridgeDryReadinessPhase::DryReady
        } else {
            RuntimeBridgeDryReadinessPhase::WaitingForInputs
        };

        RuntimeBridgeReadinessDecision {
            schema_version: SCHEMA_VERSION,
            live_ready: false,
            phase,
            reasons,
            state: self.state.clone(),
        }
    }

    fn try_observe_entry(
        &mut self,
        entry: &RuntimeBridgeStreamEntry,
    ) -> Result<(), RuntimeBridgeDlqReason> {
        let expected = expected_message_type_for_stream(&self.config, &entry.stream)
            .ok_or(RuntimeBridgeDlqReason::UnknownStream)?;
        let envelope_value: serde_json::Value = serde_json::from_str(&entry.payload)
            .map_err(|_| RuntimeBridgeDlqReason::InvalidJson)?;
        validate_envelope_header(&envelope_value, &expected)?;

        match expected {
            MessageType::Health => {
                let envelope = decode_envelope::<GatewayHealth>(
                    &entry.payload,
                    RuntimeBridgePayloadKind::Health,
                )?;
                self.state.health_seen = true;
                self.state.health_status = Some(envelope.payload.status);
                Ok(())
            }
            MessageType::Readiness => {
                let envelope = decode_envelope::<BrokerReadiness>(
                    &entry.payload,
                    RuntimeBridgePayloadKind::Readiness,
                )?;
                self.state.gateway_readiness_seen = true;
                self.state.gateway_readiness_phase = Some(envelope.payload.phase);
                Ok(())
            }
            MessageType::PortfolioSnapshot => {
                decode_envelope::<PortfolioSnapshot>(
                    &entry.payload,
                    RuntimeBridgePayloadKind::PortfolioSnapshot,
                )?;
                self.state.portfolio_snapshot_seen = true;
                Ok(())
            }
            MessageType::OrderSnapshot => {
                let envelope = decode_envelope::<OrderSnapshot>(
                    &entry.payload,
                    RuntimeBridgePayloadKind::OrderSnapshot,
                )?;
                if order_snapshot_has_raw_comments(&envelope.payload) {
                    return Err(RuntimeBridgeDlqReason::RawOrderCommentPresent);
                }
                self.state.order_snapshot_seen = true;
                self.state.blocking_unknown_orders =
                    envelope.payload.blocking_unknown_status_present;
                Ok(())
            }
            MessageType::MarketData => {
                decode_envelope::<MarketDataEvent>(
                    &entry.payload,
                    RuntimeBridgePayloadKind::MarketData,
                )?;
                self.state.market_data_seen = true;
                Ok(())
            }
            _ => Err(RuntimeBridgeDlqReason::UnsupportedMessageType),
        }
    }
}

#[derive(Debug, Clone)]
pub struct RuntimeBridgeDryConsumer {
    config: RedisStreamConfig,
    seen_bar_keys: HashSet<String>,
    metrics: RuntimeBridgeConsumerMetrics,
}

impl RuntimeBridgeDryConsumer {
    pub fn new(config: RedisStreamConfig) -> Self {
        Self {
            config,
            seen_bar_keys: HashSet::new(),
            metrics: RuntimeBridgeConsumerMetrics::default(),
        }
    }

    pub fn from_gateway_config(config: &GatewayConfig) -> Self {
        Self::new(config.redis.clone())
    }

    pub fn metrics(&self) -> &RuntimeBridgeConsumerMetrics {
        &self.metrics
    }

    pub fn consume_entry(
        &mut self,
        entry: RuntimeBridgeStreamEntry,
    ) -> RuntimeBridgeConsumeOutcome {
        self.metrics.entries_seen += 1;
        match self.try_consume_entry(&entry) {
            Ok(RuntimeBridgeConsumeOutcome::Accepted { kind, entry_id }) => {
                self.metrics.accepted_count += 1;
                self.record_kind(kind);
                RuntimeBridgeConsumeOutcome::Accepted { kind, entry_id }
            }
            Ok(RuntimeBridgeConsumeOutcome::DuplicateBar { entry_id, bar_key }) => {
                self.metrics.duplicate_bar_count += 1;
                RuntimeBridgeConsumeOutcome::DuplicateBar { entry_id, bar_key }
            }
            Ok(RuntimeBridgeConsumeOutcome::DeadLetter(dead_letter)) => {
                self.metrics.dlq_count += 1;
                RuntimeBridgeConsumeOutcome::DeadLetter(dead_letter)
            }
            Err(reason) => {
                self.metrics.dlq_count += 1;
                RuntimeBridgeConsumeOutcome::DeadLetter(dead_letter(&entry, reason))
            }
        }
    }

    fn try_consume_entry(
        &mut self,
        entry: &RuntimeBridgeStreamEntry,
    ) -> Result<RuntimeBridgeConsumeOutcome, RuntimeBridgeDlqReason> {
        let expected = expected_message_type_for_stream(&self.config, &entry.stream)
            .ok_or(RuntimeBridgeDlqReason::UnknownStream)?;
        let envelope_value: serde_json::Value = serde_json::from_str(&entry.payload)
            .map_err(|_| RuntimeBridgeDlqReason::InvalidJson)?;
        validate_envelope_header(&envelope_value, &expected)?;

        match expected {
            MessageType::Health => {
                decode_envelope::<GatewayHealth>(&entry.payload, RuntimeBridgePayloadKind::Health)?;
                Ok(accepted(RuntimeBridgePayloadKind::Health, entry))
            }
            MessageType::Readiness => {
                decode_envelope::<BrokerReadiness>(
                    &entry.payload,
                    RuntimeBridgePayloadKind::Readiness,
                )?;
                Ok(accepted(RuntimeBridgePayloadKind::Readiness, entry))
            }
            MessageType::PortfolioSnapshot => {
                decode_envelope::<PortfolioSnapshot>(
                    &entry.payload,
                    RuntimeBridgePayloadKind::PortfolioSnapshot,
                )?;
                Ok(accepted(RuntimeBridgePayloadKind::PortfolioSnapshot, entry))
            }
            MessageType::OrderSnapshot => {
                let envelope = decode_envelope::<OrderSnapshot>(
                    &entry.payload,
                    RuntimeBridgePayloadKind::OrderSnapshot,
                )?;
                if order_snapshot_has_raw_comments(&envelope.payload) {
                    return Err(RuntimeBridgeDlqReason::RawOrderCommentPresent);
                }
                Ok(accepted(RuntimeBridgePayloadKind::OrderSnapshot, entry))
            }
            MessageType::MarketData => {
                let envelope = decode_envelope::<MarketDataEvent>(
                    &entry.payload,
                    RuntimeBridgePayloadKind::MarketData,
                )?;
                if let MarketDataEvent::Bar(bar) = &envelope.payload {
                    let bar_key = runtime_bridge_bar_key(&envelope.source, bar);
                    if !self.seen_bar_keys.insert(bar_key.clone()) {
                        return Ok(RuntimeBridgeConsumeOutcome::DuplicateBar {
                            entry_id: entry.entry_id.clone(),
                            bar_key,
                        });
                    }
                }
                Ok(accepted(RuntimeBridgePayloadKind::MarketData, entry))
            }
            _ => Err(RuntimeBridgeDlqReason::UnsupportedMessageType),
        }
    }

    fn record_kind(&mut self, kind: RuntimeBridgePayloadKind) {
        match kind {
            RuntimeBridgePayloadKind::Health => self.metrics.health_count += 1,
            RuntimeBridgePayloadKind::Readiness => self.metrics.readiness_count += 1,
            RuntimeBridgePayloadKind::PortfolioSnapshot => {
                self.metrics.portfolio_snapshot_count += 1;
            }
            RuntimeBridgePayloadKind::OrderSnapshot => self.metrics.order_snapshot_count += 1,
            RuntimeBridgePayloadKind::MarketData => self.metrics.market_data_count += 1,
        }
    }
}

#[async_trait]
pub trait RedisStreamSink: Send + Sync {
    async fn publish_json<T: Serialize + Send + Sync>(
        &self,
        stream: &str,
        value: &T,
        maxlen: Option<usize>,
    ) -> Result<(), GatewayError>;
}

#[derive(Debug, Default, Clone)]
pub struct InMemoryRedisStreamSink {
    entries: Arc<Mutex<Vec<RedisStreamEntry>>>,
}

impl InMemoryRedisStreamSink {
    pub fn entries(&self) -> Result<Vec<RedisStreamEntry>, GatewayError> {
        self.entries
            .lock()
            .map(|entries| entries.clone())
            .map_err(|_| GatewayError::InternalState {
                message: "in-memory redis sink mutex poisoned",
            })
    }
}

#[async_trait]
impl RedisStreamSink for InMemoryRedisStreamSink {
    async fn publish_json<T: Serialize + Send + Sync>(
        &self,
        stream: &str,
        value: &T,
        maxlen: Option<usize>,
    ) -> Result<(), GatewayError> {
        let payload = serde_json::to_string(value)?;
        let mut entries = self
            .entries
            .lock()
            .map_err(|_| GatewayError::InternalState {
                message: "in-memory redis sink mutex poisoned",
            })?;
        entries.push(RedisStreamEntry {
            stream: stream.to_string(),
            payload,
        });
        trim_in_memory_stream(&mut entries, stream, normalized_maxlen(maxlen));
        Ok(())
    }
}

#[derive(Clone)]
pub struct RedisConnectionStreamSink {
    manager: redis::aio::ConnectionManager,
}

impl RedisConnectionStreamSink {
    pub async fn connect(url: &str) -> Result<Self, GatewayError> {
        let client = redis::Client::open(url)?;
        let manager = client.get_connection_manager().await?;
        Ok(Self { manager })
    }

    pub fn from_connection_manager(manager: redis::aio::ConnectionManager) -> Self {
        Self { manager }
    }
}

#[async_trait]
impl RedisStreamSink for RedisConnectionStreamSink {
    async fn publish_json<T: Serialize + Send + Sync>(
        &self,
        stream: &str,
        value: &T,
        maxlen: Option<usize>,
    ) -> Result<(), GatewayError> {
        let payload = serde_json::to_string(value)?;
        let mut manager = self.manager.clone();
        let mut command = redis::cmd("XADD");
        command.arg(stream);
        if let Some(maxlen) = normalized_maxlen(maxlen) {
            command.arg("MAXLEN").arg("~").arg(maxlen);
        }
        let _message_id: String = command
            .arg("*")
            .arg("payload")
            .arg(payload)
            .query_async(&mut manager)
            .await?;
        Ok(())
    }
}

fn normalized_maxlen(maxlen: Option<usize>) -> Option<usize> {
    maxlen.filter(|value| *value > 0)
}

fn trim_in_memory_stream(entries: &mut Vec<RedisStreamEntry>, stream: &str, maxlen: Option<usize>) {
    let Some(maxlen) = maxlen else {
        return;
    };
    while entries
        .iter()
        .filter(|entry| entry.stream == stream)
        .count()
        > maxlen
    {
        let Some(index) = entries.iter().position(|entry| entry.stream == stream) else {
            return;
        };
        entries.remove(index);
    }
}

fn expected_message_type_for_stream(
    config: &RedisStreamConfig,
    stream: &str,
) -> Option<MessageType> {
    if stream == config.health_stream {
        Some(MessageType::Health)
    } else if stream == config.readiness_stream {
        Some(MessageType::Readiness)
    } else if stream == config.portfolio_stream {
        Some(MessageType::PortfolioSnapshot)
    } else if stream == config.order_snapshot_stream {
        Some(MessageType::OrderSnapshot)
    } else if stream == config.market_data_stream {
        Some(MessageType::MarketData)
    } else {
        None
    }
}

fn validate_envelope_header(
    value: &serde_json::Value,
    expected: &MessageType,
) -> Result<(), RuntimeBridgeDlqReason> {
    let schema_version = value
        .get("schema_version")
        .and_then(serde_json::Value::as_u64)
        .ok_or(RuntimeBridgeDlqReason::MissingSchemaVersion)?;
    if schema_version != u64::from(SCHEMA_VERSION) {
        return Err(RuntimeBridgeDlqReason::UnsupportedSchemaVersion {
            expected: SCHEMA_VERSION,
            actual: schema_version,
        });
    }

    let msg_type = value
        .get("msg_type")
        .and_then(serde_json::Value::as_str)
        .ok_or(RuntimeBridgeDlqReason::MissingMessageType)?;
    let Some(actual) = parse_known_message_type(msg_type) else {
        return Err(RuntimeBridgeDlqReason::UnsupportedMessageType);
    };
    if &actual != expected {
        return Err(RuntimeBridgeDlqReason::MessageTypeMismatch {
            expected: expected.clone(),
            actual: Some(actual),
        });
    }
    Ok(())
}

fn parse_known_message_type(value: &str) -> Option<MessageType> {
    match value {
        "Health" => Some(MessageType::Health),
        "Readiness" => Some(MessageType::Readiness),
        "PortfolioSnapshot" => Some(MessageType::PortfolioSnapshot),
        "OrderSnapshot" => Some(MessageType::OrderSnapshot),
        "CommandAck" => Some(MessageType::CommandAck),
        "MarketData" => Some(MessageType::MarketData),
        _ => None,
    }
}

fn decode_envelope<T>(
    payload: &str,
    expected: RuntimeBridgePayloadKind,
) -> Result<Envelope<T>, RuntimeBridgeDlqReason>
where
    T: DeserializeOwned,
{
    serde_json::from_str(payload)
        .map_err(|_| RuntimeBridgeDlqReason::TypedDecodeFailed { expected })
}

fn accepted(
    kind: RuntimeBridgePayloadKind,
    entry: &RuntimeBridgeStreamEntry,
) -> RuntimeBridgeConsumeOutcome {
    RuntimeBridgeConsumeOutcome::Accepted {
        kind,
        entry_id: entry.entry_id.clone(),
    }
}

fn dead_letter(
    entry: &RuntimeBridgeStreamEntry,
    reason: RuntimeBridgeDlqReason,
) -> RuntimeBridgeDeadLetter {
    RuntimeBridgeDeadLetter {
        stream: entry.stream.clone(),
        entry_id: entry.entry_id.clone(),
        reason,
        payload_len: entry.payload.len(),
    }
}

fn order_snapshot_has_raw_comments(snapshot: &OrderSnapshot) -> bool {
    snapshot.orders.iter().any(|order| order.comment.is_some())
}

fn runtime_bridge_bar_key(source: &str, bar: &broker_core::event::Bar) -> String {
    let symbol = bar
        .instrument
        .venue_symbol
        .as_deref()
        .unwrap_or(&bar.instrument.symbol);
    format!(
        "{source}|{:?}|{symbol}|{}|{}|{}",
        bar.source_kind,
        bar.timeframe_sec,
        bar.open_ts.to_rfc3339(),
        bar.is_final
    )
}

#[derive(Debug, thiserror::Error)]
pub enum GatewayError {
    #[error("gateway serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("gateway redis error: {0}")]
    Redis(#[from] redis::RedisError),
    #[error("gateway internal state error: {message}")]
    InternalState { message: &'static str },
    #[error("gateway feature disabled: {feature}")]
    FeatureDisabled { feature: &'static str },
    #[error("gateway dry command ACK publisher unsafe mode: {reason}")]
    DryCommandAckPublisherUnsafeMode { reason: &'static str },
    #[error("finam mapper error: {0}")]
    Mapper(#[from] broker_finam::FinamMapperError),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DryOrderExecutionOutcomeKind {
    Submitted,
    SubmittedPendingBrokerOrderId,
    BrokerRejected,
    TimeoutUnknownPending,
    CancelSubmitted,
    CancelAcceptedBrokerOrderIdMismatch,
    CancelRejected,
    CancelTimeoutUnknownPending,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DryOrderExecutionReport {
    pub ack: CommandAck,
    pub state: OrderPathState,
    pub submit_attempt_count: u32,
    pub cancel_attempt_count: u32,
    pub outcome: DryOrderExecutionOutcomeKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EndpointResponseIntegrationOutcomeKind {
    Submitted,
    SubmittedPendingBrokerOrderId,
    BrokerRejected,
    TimeoutUnknownPending,
    CancelSubmitted,
    CancelAcceptedBrokerOrderIdMismatch,
    CancelRejected,
    CancelTimeoutUnknownPending,
    RateLimited,
    Maintenance,
    DecodeError,
    Unauthorized,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EndpointResponseIntegrationReport {
    pub ack: CommandAck,
    pub state: OrderPathState,
    pub submit_attempt_count: u32,
    pub cancel_attempt_count: u32,
    pub outcome: EndpointResponseIntegrationOutcomeKind,
    pub endpoint_response: broker_finam::FinamOrderEndpointResponseDiagnostic,
    pub disarm_decision: Option<OperatorDisarmDecision>,
    pub retry_after_ms: Option<u64>,
}

#[derive(Debug, thiserror::Error)]
pub enum EndpointResponseIntegrationSimulatorError {
    #[error("endpoint response integration missing order-path record: {request_id}")]
    MissingOrderPathRecord { request_id: StrategyRequestId },
    #[error("endpoint response integration missing mapped order-path request id for cancel request: {request_id}")]
    MissingCancelMapping { request_id: StrategyRequestId },
    #[error("endpoint response request build error: {0}")]
    RequestBuild(#[from] broker_finam::FinamOrderRequestBuildError),
    #[error("endpoint response mapper error: {0}")]
    EndpointMapper(#[from] broker_finam::FinamOrderEndpointMapperError),
    #[error("endpoint response order-path store error: {0}")]
    Store(#[from] OrderPathStoreError),
    #[error("endpoint response order-path transition error: {0}")]
    Transition(#[from] OrderPathTransitionError),
}

#[derive(Debug, thiserror::Error)]
pub enum DryOrderExecutionSimulatorError {
    #[error("dry order execution missing order-path record: {request_id}")]
    MissingOrderPathRecord { request_id: StrategyRequestId },
    #[error("dry cancel execution missing mapped order-path request id for cancel request: {request_id}")]
    MissingCancelMapping { request_id: StrategyRequestId },
    #[error("dry order request build error: {0}")]
    RequestBuild(#[from] broker_finam::FinamOrderRequestBuildError),
    #[error("dry order execution client error: {0}")]
    Execution(#[from] broker_finam::FinamOrderExecutionError),
    #[error("dry order-path store error: {0}")]
    Store(#[from] OrderPathStoreError),
    #[error("dry order-path transition error: {0}")]
    Transition(#[from] OrderPathTransitionError),
}

struct EndpointResponseIntegrationInput<'a> {
    endpoint_result: broker_finam::FinamOrderEndpointMappedResult,
    endpoint_response: broker_finam::FinamOrderEndpointResponseDiagnostic,
    operator_arm: Option<&'a mut OperatorArm>,
    begin_ts: DateTime<Utc>,
    outcome_ts: DateTime<Utc>,
}

pub fn simulate_place_order_endpoint_fixture<S>(
    store: &mut S,
    approved: &PreflightApprovedPlaceOrder,
    outgoing_comment: Option<&OutgoingOrderComment>,
    endpoint_fixture: &broker_finam::FinamOrderEndpointFixture,
    operator_arm: Option<&mut OperatorArm>,
    begin_ts: DateTime<Utc>,
    outcome_ts: DateTime<Utc>,
) -> Result<EndpointResponseIntegrationReport, EndpointResponseIntegrationSimulatorError>
where
    S: OrderPathStore,
{
    let diagnostic = endpoint_fixture.redacted_diagnostic();
    let endpoint_result = endpoint_fixture.map_fixture()?;
    simulate_place_order_endpoint_result(
        store,
        approved,
        outgoing_comment,
        EndpointResponseIntegrationInput {
            endpoint_result,
            endpoint_response: diagnostic,
            operator_arm,
            begin_ts,
            outcome_ts,
        },
    )
}

pub fn simulate_cancel_order_endpoint_fixture<S>(
    store: &mut S,
    approved: &PreflightApprovedCancelOrder,
    endpoint_fixture: &broker_finam::FinamOrderEndpointFixture,
    operator_arm: Option<&mut OperatorArm>,
    begin_ts: DateTime<Utc>,
    outcome_ts: DateTime<Utc>,
) -> Result<EndpointResponseIntegrationReport, EndpointResponseIntegrationSimulatorError>
where
    S: OrderPathStore,
{
    let diagnostic = endpoint_fixture.redacted_diagnostic();
    let endpoint_result = endpoint_fixture.map_fixture()?;
    simulate_cancel_order_endpoint_result(
        store,
        approved,
        EndpointResponseIntegrationInput {
            endpoint_result,
            endpoint_response: diagnostic,
            operator_arm,
            begin_ts,
            outcome_ts,
        },
    )
}

pub fn simulate_place_order_endpoint_local_http_response<S>(
    store: &mut S,
    approved: &PreflightApprovedPlaceOrder,
    outgoing_comment: Option<&OutgoingOrderComment>,
    response: &broker_finam::FinamOrderEndpointLocalHttpResponse,
    operator_arm: Option<&mut OperatorArm>,
    begin_ts: DateTime<Utc>,
    outcome_ts: DateTime<Utc>,
) -> Result<EndpointResponseIntegrationReport, EndpointResponseIntegrationSimulatorError>
where
    S: OrderPathStore,
{
    let request_id = approved.order().request_id;
    let _spec = broker_finam::build_place_order_request(approved, outgoing_comment)?;
    let mut record = store
        .load_by_request_id(request_id)
        .ok_or(EndpointResponseIntegrationSimulatorError::MissingOrderPathRecord { request_id })?;

    record.transition(OrderPathEvent::BeginSubmit, begin_ts)?;
    store.update_record(record.clone())?;
    let classified = broker_finam::classify_order_endpoint_local_http_response(response);

    apply_place_order_endpoint_result_after_begin(
        store,
        record,
        classified.result,
        classified.diagnostic,
        operator_arm,
        outcome_ts,
    )
}

pub fn simulate_cancel_order_endpoint_local_http_response<S>(
    store: &mut S,
    approved: &PreflightApprovedCancelOrder,
    response: &broker_finam::FinamOrderEndpointLocalHttpResponse,
    operator_arm: Option<&mut OperatorArm>,
    begin_ts: DateTime<Utc>,
    outcome_ts: DateTime<Utc>,
) -> Result<EndpointResponseIntegrationReport, EndpointResponseIntegrationSimulatorError>
where
    S: OrderPathStore,
{
    let cancel_request_id = approved.cancel().request_id;
    let mapped_request_id = approved.mapped_request_id().ok_or(
        EndpointResponseIntegrationSimulatorError::MissingCancelMapping {
            request_id: cancel_request_id,
        },
    )?;
    let _spec = broker_finam::build_cancel_order_request(approved)?;
    let mut record = store.load_by_request_id(mapped_request_id).ok_or(
        EndpointResponseIntegrationSimulatorError::MissingOrderPathRecord {
            request_id: mapped_request_id,
        },
    )?;

    record.transition(OrderPathEvent::RequestCancel, begin_ts)?;
    store.update_record(record.clone())?;
    let classified = broker_finam::classify_order_endpoint_local_http_response(response);

    apply_cancel_order_endpoint_result_after_begin(
        store,
        record,
        approved,
        classified.result,
        classified.diagnostic,
        operator_arm,
        outcome_ts,
    )
}

fn simulate_place_order_endpoint_result<S>(
    store: &mut S,
    approved: &PreflightApprovedPlaceOrder,
    outgoing_comment: Option<&OutgoingOrderComment>,
    input: EndpointResponseIntegrationInput<'_>,
) -> Result<EndpointResponseIntegrationReport, EndpointResponseIntegrationSimulatorError>
where
    S: OrderPathStore,
{
    let EndpointResponseIntegrationInput {
        endpoint_result,
        endpoint_response,
        operator_arm,
        begin_ts,
        outcome_ts,
    } = input;
    let request_id = approved.order().request_id;
    let _spec = broker_finam::build_place_order_request(approved, outgoing_comment)?;
    let mut record = store
        .load_by_request_id(request_id)
        .ok_or(EndpointResponseIntegrationSimulatorError::MissingOrderPathRecord { request_id })?;

    record.transition(OrderPathEvent::BeginSubmit, begin_ts)?;
    store.update_record(record.clone())?;

    apply_place_order_endpoint_result_after_begin(
        store,
        record,
        endpoint_result,
        endpoint_response,
        operator_arm,
        outcome_ts,
    )
}

fn apply_place_order_endpoint_result_after_begin<S>(
    store: &mut S,
    mut record: broker_core::OrderPathRecord,
    endpoint_result: broker_finam::FinamOrderEndpointMappedResult,
    endpoint_response: broker_finam::FinamOrderEndpointResponseDiagnostic,
    operator_arm: Option<&mut OperatorArm>,
    outcome_ts: DateTime<Utc>,
) -> Result<EndpointResponseIntegrationReport, EndpointResponseIntegrationSimulatorError>
where
    S: OrderPathStore,
{
    if let Some(non_execution) = endpoint_non_execution_policy(&endpoint_result) {
        record.transition(OrderPathEvent::RequireManualIntervention, outcome_ts)?;
        record.last_ack_status = Some(CommandAckStatus::Error);
        record.last_error_kind = Some(non_execution.error_kind);
        store.update_record(record.clone())?;
        let ack = record.synthetic_ack(
            CommandAckStatus::Error,
            Some(CommandAckReason::new(non_execution.reason_code)),
            outcome_ts,
        );
        return Ok(EndpointResponseIntegrationReport {
            ack,
            state: record.state,
            submit_attempt_count: record.submit_attempt_count,
            cancel_attempt_count: record.cancel_attempt_count,
            outcome: non_execution.outcome,
            endpoint_response,
            disarm_decision: operator_arm
                .map(|arm| arm.disarm_for_safety_signal(non_execution.disarm_signal)),
            retry_after_ms: non_execution.retry_after_ms,
        });
    }

    let broker_finam::FinamOrderEndpointMappedResult::Execution(execution_outcome) =
        endpoint_result
    else {
        unreachable!("non-execution endpoint results are handled above");
    };
    let (ack_status, ack_reason, outcome) = match execution_outcome {
        broker_finam::FinamOrderExecutionOutcome::Accepted {
            broker_order_id: Some(broker_order_id),
        } => {
            record.broker_order_id = Some(broker_order_id);
            record.transition(OrderPathEvent::SubmitAccepted, outcome_ts)?;
            (
                CommandAckStatus::Submitted,
                Some(CommandAckReason::synthetic_submitted()),
                EndpointResponseIntegrationOutcomeKind::Submitted,
            )
        }
        broker_finam::FinamOrderExecutionOutcome::Accepted {
            broker_order_id: None,
        } => {
            record.transition(
                OrderPathEvent::SubmitAcceptedWithoutBrokerOrderId,
                outcome_ts,
            )?;
            (
                CommandAckStatus::UnknownPending,
                Some(CommandAckReason::new(
                    CommandAckReasonCode::ReconciliationRequired,
                )),
                EndpointResponseIntegrationOutcomeKind::SubmittedPendingBrokerOrderId,
            )
        }
        broker_finam::FinamOrderExecutionOutcome::Rejected { reason_code } => {
            record.transition(OrderPathEvent::BrokerReject, outcome_ts)?;
            (
                CommandAckStatus::Rejected,
                Some(CommandAckReason::new(reason_code)),
                EndpointResponseIntegrationOutcomeKind::BrokerRejected,
            )
        }
        broker_finam::FinamOrderExecutionOutcome::Timeout => {
            record.transition(OrderPathEvent::SubmitTimedOut, outcome_ts)?;
            (
                CommandAckStatus::Timeout,
                Some(CommandAckReason::new(
                    CommandAckReasonCode::TransportTimeout,
                )),
                EndpointResponseIntegrationOutcomeKind::TimeoutUnknownPending,
            )
        }
    };
    store.update_record(record.clone())?;
    let ack = record.synthetic_ack(ack_status, ack_reason, outcome_ts);

    Ok(EndpointResponseIntegrationReport {
        ack,
        state: record.state,
        submit_attempt_count: record.submit_attempt_count,
        cancel_attempt_count: record.cancel_attempt_count,
        outcome,
        endpoint_response,
        disarm_decision: None,
        retry_after_ms: None,
    })
}

fn simulate_cancel_order_endpoint_result<S>(
    store: &mut S,
    approved: &PreflightApprovedCancelOrder,
    input: EndpointResponseIntegrationInput<'_>,
) -> Result<EndpointResponseIntegrationReport, EndpointResponseIntegrationSimulatorError>
where
    S: OrderPathStore,
{
    let EndpointResponseIntegrationInput {
        endpoint_result,
        endpoint_response,
        operator_arm,
        begin_ts,
        outcome_ts,
    } = input;
    let cancel_request_id = approved.cancel().request_id;
    let mapped_request_id = approved.mapped_request_id().ok_or(
        EndpointResponseIntegrationSimulatorError::MissingCancelMapping {
            request_id: cancel_request_id,
        },
    )?;
    let _spec = broker_finam::build_cancel_order_request(approved)?;
    let mut record = store.load_by_request_id(mapped_request_id).ok_or(
        EndpointResponseIntegrationSimulatorError::MissingOrderPathRecord {
            request_id: mapped_request_id,
        },
    )?;

    record.transition(OrderPathEvent::RequestCancel, begin_ts)?;
    store.update_record(record.clone())?;

    apply_cancel_order_endpoint_result_after_begin(
        store,
        record,
        approved,
        endpoint_result,
        endpoint_response,
        operator_arm,
        outcome_ts,
    )
}

fn apply_cancel_order_endpoint_result_after_begin<S>(
    store: &mut S,
    mut record: broker_core::OrderPathRecord,
    approved: &PreflightApprovedCancelOrder,
    endpoint_result: broker_finam::FinamOrderEndpointMappedResult,
    endpoint_response: broker_finam::FinamOrderEndpointResponseDiagnostic,
    operator_arm: Option<&mut OperatorArm>,
    outcome_ts: DateTime<Utc>,
) -> Result<EndpointResponseIntegrationReport, EndpointResponseIntegrationSimulatorError>
where
    S: OrderPathStore,
{
    if let Some(non_execution) = endpoint_non_execution_policy(&endpoint_result) {
        record.transition(OrderPathEvent::RequireManualIntervention, outcome_ts)?;
        record.last_ack_status = Some(CommandAckStatus::Error);
        record.last_error_kind = Some(non_execution.error_kind);
        store.update_record(record.clone())?;
        let ack = record.synthetic_ack(
            CommandAckStatus::Error,
            Some(CommandAckReason::new(non_execution.reason_code)),
            outcome_ts,
        );
        return Ok(EndpointResponseIntegrationReport {
            ack,
            state: record.state,
            submit_attempt_count: record.submit_attempt_count,
            cancel_attempt_count: record.cancel_attempt_count,
            outcome: non_execution.outcome,
            endpoint_response,
            disarm_decision: operator_arm
                .map(|arm| arm.disarm_for_safety_signal(non_execution.disarm_signal)),
            retry_after_ms: non_execution.retry_after_ms,
        });
    }

    let broker_finam::FinamOrderEndpointMappedResult::Execution(execution_outcome) =
        endpoint_result
    else {
        unreachable!("non-execution endpoint results are handled above");
    };
    let (ack_status, ack_reason, outcome) = match execution_outcome {
        broker_finam::FinamOrderExecutionOutcome::Accepted { broker_order_id } => {
            let returned_id_mismatch = broker_order_id
                .as_ref()
                .is_some_and(|returned_id| returned_id != &approved.cancel().order_id);
            if returned_id_mismatch {
                record.transition(OrderPathEvent::RequireManualIntervention, outcome_ts)?;
                (
                    CommandAckStatus::UnknownPending,
                    Some(CommandAckReason::new(
                        CommandAckReasonCode::ManualInterventionRequired,
                    )),
                    EndpointResponseIntegrationOutcomeKind::CancelAcceptedBrokerOrderIdMismatch,
                )
            } else {
                record.transition(OrderPathEvent::CancelAccepted, outcome_ts)?;
                (
                    CommandAckStatus::Submitted,
                    Some(CommandAckReason::synthetic_submitted()),
                    EndpointResponseIntegrationOutcomeKind::CancelSubmitted,
                )
            }
        }
        broker_finam::FinamOrderExecutionOutcome::Rejected { reason_code } => {
            record.transition(OrderPathEvent::CancelRejected, outcome_ts)?;
            (
                CommandAckStatus::Rejected,
                Some(CommandAckReason::new(reason_code)),
                EndpointResponseIntegrationOutcomeKind::CancelRejected,
            )
        }
        broker_finam::FinamOrderExecutionOutcome::Timeout => {
            record.transition(OrderPathEvent::CancelTimedOut, outcome_ts)?;
            (
                CommandAckStatus::Timeout,
                Some(CommandAckReason::new(
                    CommandAckReasonCode::TransportTimeout,
                )),
                EndpointResponseIntegrationOutcomeKind::CancelTimeoutUnknownPending,
            )
        }
    };
    store.update_record(record.clone())?;
    let ack = record.synthetic_ack(ack_status, ack_reason, outcome_ts);

    Ok(EndpointResponseIntegrationReport {
        ack,
        state: record.state,
        submit_attempt_count: record.submit_attempt_count,
        cancel_attempt_count: record.cancel_attempt_count,
        outcome,
        endpoint_response,
        disarm_decision: None,
        retry_after_ms: None,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct EndpointNonExecutionPolicy {
    outcome: EndpointResponseIntegrationOutcomeKind,
    reason_code: CommandAckReasonCode,
    error_kind: OrderPathErrorKind,
    disarm_signal: OperatorDisarmSignal,
    retry_after_ms: Option<u64>,
}

fn endpoint_non_execution_policy(
    endpoint_result: &broker_finam::FinamOrderEndpointMappedResult,
) -> Option<EndpointNonExecutionPolicy> {
    match endpoint_result {
        broker_finam::FinamOrderEndpointMappedResult::Execution(_) => None,
        broker_finam::FinamOrderEndpointMappedResult::RateLimited { retry_after_ms } => {
            Some(EndpointNonExecutionPolicy {
                outcome: EndpointResponseIntegrationOutcomeKind::RateLimited,
                reason_code: CommandAckReasonCode::RateLimited,
                error_kind: OrderPathErrorKind::RateLimited,
                disarm_signal: OperatorDisarmSignal::OrderEndpointRateLimited,
                retry_after_ms: *retry_after_ms,
            })
        }
        broker_finam::FinamOrderEndpointMappedResult::Maintenance { .. } => {
            Some(EndpointNonExecutionPolicy {
                outcome: EndpointResponseIntegrationOutcomeKind::Maintenance,
                reason_code: CommandAckReasonCode::BrokerMaintenance,
                error_kind: OrderPathErrorKind::BrokerMaintenance,
                disarm_signal: OperatorDisarmSignal::OrderEndpointMaintenance,
                retry_after_ms: None,
            })
        }
        broker_finam::FinamOrderEndpointMappedResult::DecodeError { .. } => {
            Some(EndpointNonExecutionPolicy {
                outcome: EndpointResponseIntegrationOutcomeKind::DecodeError,
                reason_code: CommandAckReasonCode::ResponseDecodeError,
                error_kind: OrderPathErrorKind::ResponseDecodeError,
                disarm_signal: OperatorDisarmSignal::OrderEndpointDecodeError,
                retry_after_ms: None,
            })
        }
        broker_finam::FinamOrderEndpointMappedResult::Unauthorized { .. } => {
            Some(EndpointNonExecutionPolicy {
                outcome: EndpointResponseIntegrationOutcomeKind::Unauthorized,
                reason_code: CommandAckReasonCode::Unauthorized,
                error_kind: OrderPathErrorKind::Unauthorized,
                disarm_signal: OperatorDisarmSignal::OrderEndpointUnauthorized,
                retry_after_ms: None,
            })
        }
    }
}

pub async fn simulate_place_order_approved<S, C>(
    store: &mut S,
    client: &mut C,
    approved: &PreflightApprovedPlaceOrder,
    outgoing_comment: Option<&OutgoingOrderComment>,
    begin_ts: DateTime<Utc>,
    outcome_ts: DateTime<Utc>,
) -> Result<DryOrderExecutionReport, DryOrderExecutionSimulatorError>
where
    S: OrderPathStore,
    C: broker_finam::FinamApprovedOrderExecutionClient,
{
    let request_id = approved.order().request_id;
    let spec = broker_finam::build_place_order_request(approved, outgoing_comment)?;
    let mut record = store
        .load_by_request_id(request_id)
        .ok_or(DryOrderExecutionSimulatorError::MissingOrderPathRecord { request_id })?;

    record.transition(OrderPathEvent::BeginSubmit, begin_ts)?;
    store.update_record(record.clone())?;

    let execution_outcome = client.place_approved(spec).await?;
    let (ack_status, ack_reason, outcome) = match execution_outcome {
        broker_finam::FinamOrderExecutionOutcome::Accepted {
            broker_order_id: Some(broker_order_id),
        } => {
            record.broker_order_id = Some(broker_order_id);
            record.transition(OrderPathEvent::SubmitAccepted, outcome_ts)?;
            (
                CommandAckStatus::Submitted,
                Some(CommandAckReason::synthetic_submitted()),
                DryOrderExecutionOutcomeKind::Submitted,
            )
        }
        broker_finam::FinamOrderExecutionOutcome::Accepted {
            broker_order_id: None,
        } => {
            record.transition(
                OrderPathEvent::SubmitAcceptedWithoutBrokerOrderId,
                outcome_ts,
            )?;
            (
                CommandAckStatus::UnknownPending,
                Some(CommandAckReason::new(
                    CommandAckReasonCode::ReconciliationRequired,
                )),
                DryOrderExecutionOutcomeKind::SubmittedPendingBrokerOrderId,
            )
        }
        broker_finam::FinamOrderExecutionOutcome::Rejected { reason_code } => {
            record.transition(OrderPathEvent::BrokerReject, outcome_ts)?;
            (
                CommandAckStatus::Rejected,
                Some(CommandAckReason::new(reason_code)),
                DryOrderExecutionOutcomeKind::BrokerRejected,
            )
        }
        broker_finam::FinamOrderExecutionOutcome::Timeout => {
            record.transition(OrderPathEvent::SubmitTimedOut, outcome_ts)?;
            (
                CommandAckStatus::Timeout,
                Some(CommandAckReason::new(
                    CommandAckReasonCode::TransportTimeout,
                )),
                DryOrderExecutionOutcomeKind::TimeoutUnknownPending,
            )
        }
    };
    store.update_record(record.clone())?;
    let ack = record.synthetic_ack(ack_status, ack_reason, outcome_ts);

    Ok(DryOrderExecutionReport {
        ack,
        state: record.state,
        submit_attempt_count: record.submit_attempt_count,
        cancel_attempt_count: record.cancel_attempt_count,
        outcome,
    })
}

pub async fn simulate_cancel_order_approved<S, C>(
    store: &mut S,
    client: &mut C,
    approved: &PreflightApprovedCancelOrder,
    begin_ts: DateTime<Utc>,
    outcome_ts: DateTime<Utc>,
) -> Result<DryOrderExecutionReport, DryOrderExecutionSimulatorError>
where
    S: OrderPathStore,
    C: broker_finam::FinamApprovedOrderExecutionClient,
{
    let cancel_request_id = approved.cancel().request_id;
    let mapped_request_id = approved.mapped_request_id().ok_or(
        DryOrderExecutionSimulatorError::MissingCancelMapping {
            request_id: cancel_request_id,
        },
    )?;
    let spec = broker_finam::build_cancel_order_request(approved)?;
    let mut record = store.load_by_request_id(mapped_request_id).ok_or(
        DryOrderExecutionSimulatorError::MissingOrderPathRecord {
            request_id: mapped_request_id,
        },
    )?;

    record.transition(OrderPathEvent::RequestCancel, begin_ts)?;
    store.update_record(record.clone())?;

    let execution_outcome = client.cancel_approved(spec).await?;
    let (ack_status, ack_reason, outcome) = match execution_outcome {
        broker_finam::FinamOrderExecutionOutcome::Accepted { broker_order_id } => {
            let returned_id_mismatch = broker_order_id
                .as_ref()
                .is_some_and(|returned_id| returned_id != &approved.cancel().order_id);
            if returned_id_mismatch {
                record.transition(OrderPathEvent::RequireManualIntervention, outcome_ts)?;
                (
                    CommandAckStatus::UnknownPending,
                    Some(CommandAckReason::new(
                        CommandAckReasonCode::ManualInterventionRequired,
                    )),
                    DryOrderExecutionOutcomeKind::CancelAcceptedBrokerOrderIdMismatch,
                )
            } else {
                record.transition(OrderPathEvent::CancelAccepted, outcome_ts)?;
                (
                    CommandAckStatus::Submitted,
                    Some(CommandAckReason::synthetic_submitted()),
                    DryOrderExecutionOutcomeKind::CancelSubmitted,
                )
            }
        }
        broker_finam::FinamOrderExecutionOutcome::Rejected { reason_code } => {
            record.transition(OrderPathEvent::CancelRejected, outcome_ts)?;
            (
                CommandAckStatus::Rejected,
                Some(CommandAckReason::new(reason_code)),
                DryOrderExecutionOutcomeKind::CancelRejected,
            )
        }
        broker_finam::FinamOrderExecutionOutcome::Timeout => {
            record.transition(OrderPathEvent::CancelTimedOut, outcome_ts)?;
            (
                CommandAckStatus::Timeout,
                Some(CommandAckReason::new(
                    CommandAckReasonCode::TransportTimeout,
                )),
                DryOrderExecutionOutcomeKind::CancelTimeoutUnknownPending,
            )
        }
    };
    store.update_record(record.clone())?;
    let ack = record.synthetic_ack(ack_status, ack_reason, outcome_ts);

    Ok(DryOrderExecutionReport {
        ack,
        state: record.state,
        submit_attempt_count: record.submit_attempt_count,
        cancel_attempt_count: record.cancel_attempt_count,
        outcome,
    })
}

#[derive(Debug, Clone)]
pub struct FinamGateway<S> {
    config: GatewayConfig,
    sink: S,
}

impl<S> FinamGateway<S>
where
    S: RedisStreamSink,
{
    pub fn new(config: GatewayConfig, sink: S) -> Self {
        Self { config, sink }
    }

    pub fn config(&self) -> &GatewayConfig {
        &self.config
    }

    pub fn real_order_endpoint_gate_decision(&self) -> RealOrderEndpointGateDecision {
        self.config.features.real_order_endpoint_gate_decision()
    }

    pub fn command_consumer_mode(&self) -> CommandConsumerMode {
        if self.config.features.command_consumer_enabled {
            CommandConsumerMode::FeatureDisabled
        } else {
            CommandConsumerMode::Absent
        }
    }

    pub async fn publish_health(&self, health: GatewayHealth) -> Result<(), GatewayError> {
        if !self.config.features.publish_health {
            return Err(GatewayError::FeatureDisabled {
                feature: "publish_health",
            });
        }
        self.publish_envelope(
            &self.config.redis.health_stream,
            MessageType::Health,
            health,
            self.config.redis.retention.health_maxlen,
        )
        .await
    }

    pub async fn publish_readiness(&self, readiness: BrokerReadiness) -> Result<(), GatewayError> {
        if !self.config.features.publish_readiness {
            return Err(GatewayError::FeatureDisabled {
                feature: "publish_readiness",
            });
        }
        self.publish_envelope(
            &self.config.redis.readiness_stream,
            MessageType::Readiness,
            readiness,
            self.config.redis.retention.readiness_maxlen,
        )
        .await
    }

    pub async fn publish_portfolio_snapshot(
        &self,
        snapshot: PortfolioSnapshot,
    ) -> Result<(), GatewayError> {
        if !self.config.features.publish_snapshots {
            return Err(GatewayError::FeatureDisabled {
                feature: "publish_snapshots",
            });
        }
        self.publish_envelope(
            &self.config.redis.portfolio_stream,
            MessageType::PortfolioSnapshot,
            snapshot,
            self.config.redis.retention.portfolio_maxlen,
        )
        .await
    }

    pub async fn publish_order_snapshot(
        &self,
        mut snapshot: OrderSnapshot,
    ) -> Result<(), GatewayError> {
        if !self.config.features.publish_snapshots {
            return Err(GatewayError::FeatureDisabled {
                feature: "publish_snapshots",
            });
        }
        redact_order_snapshot_comments(&mut snapshot.orders);
        self.publish_envelope(
            &self.config.redis.order_snapshot_stream,
            MessageType::OrderSnapshot,
            snapshot,
            self.config.redis.retention.order_snapshot_maxlen,
        )
        .await
    }

    pub async fn publish_market_data_event(
        &self,
        event: MarketDataEvent,
    ) -> Result<(), GatewayError> {
        if !self.config.features.publish_market_data_from_readonly {
            return Err(GatewayError::FeatureDisabled {
                feature: "publish_market_data_from_readonly",
            });
        }
        self.publish_envelope(
            &self.config.redis.market_data_stream,
            MessageType::MarketData,
            event,
            self.config.redis.retention.market_data_maxlen,
        )
        .await
    }

    pub async fn publish_readonly_snapshots(
        &self,
        account: &broker_finam::dto::AccountResponse,
        orders: &broker_finam::dto::AccountOrdersResponse,
        received_ts: DateTime<Utc>,
    ) -> Result<ReadonlySnapshotSummary, GatewayError> {
        let portfolio = broker_finam::map_portfolio_snapshot(account, received_ts)?;
        let mapped_orders = orders
            .orders
            .iter()
            .map(|order| broker_finam::map_order_state(order, received_ts))
            .collect::<Result<Vec<_>, _>>()?;
        let order_snapshot = build_order_snapshot(mapped_orders, received_ts);
        let summary = ReadonlySnapshotSummary {
            cash_count: portfolio.cash.len(),
            positions_count: portfolio.positions.len(),
            orders_count: order_snapshot.orders.len(),
            active_orders_count: order_snapshot.active_orders_count,
            terminal_orders_count: order_snapshot.terminal_orders_count,
            blocking_unknown_status_present: order_snapshot.blocking_unknown_status_present,
        };

        self.publish_portfolio_snapshot(portfolio).await?;
        self.publish_order_snapshot(order_snapshot).await?;
        Ok(summary)
    }

    pub async fn run_readonly_reconciliation_once(
        &self,
        account: &broker_finam::dto::AccountResponse,
        orders: &broker_finam::dto::AccountOrdersResponse,
        received_ts: DateTime<Utc>,
    ) -> Result<ReadonlyReconciliationReport, GatewayError> {
        let health = default_readonly_health(&self.config);
        self.publish_health(health.clone()).await?;

        let summary = self
            .publish_readonly_snapshots(account, orders, received_ts)
            .await?;
        let readiness = readiness_from_readonly_summary(&summary);
        self.publish_readiness(readiness.clone()).await?;

        Ok(ReadonlyReconciliationReport {
            health,
            readiness,
            summary,
        })
    }

    pub fn reject_command_feature_disabled(&self, command: &BrokerCommand) -> CommandAck {
        let request_id = command_request_id(command);
        CommandAck {
            request_id,
            client_order_id: command_client_order_id(command),
            broker_order_id: None,
            status: CommandAckStatus::Rejected,
            reason: Some(CommandAckReason::feature_disabled()),
            received_ts: Utc::now(),
        }
    }

    pub async fn publish_dry_command_ack(&self, ack: CommandAck) -> Result<(), GatewayError> {
        self.validate_dry_command_ack_publisher_mode()?;
        self.publish_envelope(
            &self.config.redis.command_ack_stream,
            MessageType::CommandAck,
            redact_command_ack_for_redis(ack),
            self.config.redis.retention.command_ack_maxlen,
        )
        .await
    }

    fn validate_dry_command_ack_publisher_mode(&self) -> Result<(), GatewayError> {
        if self.config.features.command_consumer_enabled {
            return Err(GatewayError::DryCommandAckPublisherUnsafeMode {
                reason: "command_consumer_enabled",
            });
        }
        if self.config.features.order_placement_enabled {
            return Err(GatewayError::DryCommandAckPublisherUnsafeMode {
                reason: "order_placement_enabled",
            });
        }
        if self.config.features.cancel_enabled {
            return Err(GatewayError::DryCommandAckPublisherUnsafeMode {
                reason: "cancel_enabled",
            });
        }
        if self.config.features.stop_sltp_bracket_enabled {
            return Err(GatewayError::DryCommandAckPublisherUnsafeMode {
                reason: "stop_sltp_bracket_enabled",
            });
        }
        Ok(())
    }

    async fn publish_envelope<T: Serialize + Send + Sync>(
        &self,
        stream: &str,
        msg_type: MessageType,
        payload: T,
        maxlen: Option<usize>,
    ) -> Result<(), GatewayError> {
        let envelope = Envelope::new(self.config.source.clone(), msg_type, payload);
        self.sink.publish_json(stream, &envelope, maxlen).await
    }
}

pub fn build_order_snapshot(mut orders: Vec<Order>, received_ts: DateTime<Utc>) -> OrderSnapshot {
    redact_order_snapshot_comments(&mut orders);
    let active_orders_count = broker_finam::active_orders(&orders).count();
    let terminal_orders_count = broker_finam::terminal_orders(&orders).count();
    let blocking_unknown_status_present =
        broker_finam::has_blocking_unknown_order_statuses(&orders);
    OrderSnapshot {
        orders,
        active_orders_count,
        terminal_orders_count,
        blocking_unknown_status_present,
        received_ts,
    }
}

fn redact_order_snapshot_comments(orders: &mut [Order]) {
    for order in orders {
        order.comment = None;
    }
}

pub fn readiness_from_readonly_summary(summary: &ReadonlySnapshotSummary) -> BrokerReadiness {
    if summary.blocking_unknown_status_present {
        BrokerReadiness {
            phase: ReadinessPhase::Blocked,
            reasons: vec![ReadinessReason::UnknownOpenOrders],
            checked_ts: Utc::now(),
        }
    } else {
        BrokerReadiness {
            phase: ReadinessPhase::Reconciliation,
            reasons: vec![ReadinessReason::OperatorLiveArmMissing],
            checked_ts: Utc::now(),
        }
    }
}

pub fn default_readonly_health(config: &GatewayConfig) -> GatewayHealth {
    GatewayHealth {
        status: GatewayHealthStatus::ReadOnly,
        checked_ts: Utc::now(),
        redis_configured: !config.redis.url.is_empty(),
        command_consumer_enabled: config.features.command_consumer_enabled,
        order_placement_enabled: config.features.order_placement_enabled,
    }
}

pub fn degraded_health(config: &GatewayConfig) -> GatewayHealth {
    GatewayHealth {
        status: GatewayHealthStatus::Degraded,
        checked_ts: Utc::now(),
        redis_configured: !config.redis.url.is_empty(),
        command_consumer_enabled: config.features.command_consumer_enabled,
        order_placement_enabled: config.features.order_placement_enabled,
    }
}

pub fn stopped_health(config: &GatewayConfig) -> GatewayHealth {
    GatewayHealth {
        status: GatewayHealthStatus::Stopped,
        checked_ts: Utc::now(),
        redis_configured: !config.redis.url.is_empty(),
        command_consumer_enabled: config.features.command_consumer_enabled,
        order_placement_enabled: config.features.order_placement_enabled,
    }
}

pub fn degraded_readiness(reason: ReadinessReason) -> BrokerReadiness {
    BrokerReadiness {
        phase: ReadinessPhase::Degraded,
        reasons: vec![reason],
        checked_ts: Utc::now(),
    }
}

pub fn stopped_readiness() -> BrokerReadiness {
    BrokerReadiness {
        phase: ReadinessPhase::Stopped,
        reasons: vec![ReadinessReason::OperatorPaused],
        checked_ts: Utc::now(),
    }
}

fn command_request_id(command: &BrokerCommand) -> StrategyRequestId {
    match command {
        BrokerCommand::PlaceOrder(order) => order.request_id,
        BrokerCommand::CancelOrder(order) => order.request_id,
    }
}

fn command_client_order_id(command: &BrokerCommand) -> Option<broker_core::ClientOrderId> {
    match command {
        BrokerCommand::PlaceOrder(order) => Some(order.client_order_id.clone()),
        BrokerCommand::CancelOrder(order) => order.client_order_id.clone(),
    }
}

pub fn redact_command_ack_for_redis(mut ack: CommandAck) -> CommandAck {
    ack.client_order_id = None;
    ack.broker_order_id = None;
    ack
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use broker_core::account::PortfolioSnapshot;
    use broker_core::command::{CommandAckReason, CommandAckReasonCode};
    use broker_core::event::{Bar, MarketDataEvent, MarketDataSourceKind, Quote};
    use broker_core::ids::{BrokerOrderId, ClientOrderId, StrategyRequestId};
    use broker_core::instrument::{Exchange, InstrumentId, Market};
    use broker_core::order::{Order, OrderSide, OrderStatus, OrderType, TimeInForce};
    use broker_core::{
        CancelOrder, CancelPreflightApproval, DryOrderRateLimit, InMemoryOrderPathStore,
        OperatorArm, OperatorDisarmSignal, OrderPathErrorKind, OrderPathEvent, OrderPathRecord,
        OrderPathState, OrderPathStore, OrderPathTransitionError, OrderPreflightPolicy, PlaceOrder,
        SqliteOrderPathReadStore, SqliteOrderPathStore,
    };
    use broker_finam::{
        FinamDryOrderClient, FinamOrderEndpointAcceptedDto, FinamOrderEndpointFixture,
        FinamOrderEndpointLocalHttpResponse, FinamOrderEndpointMaintenanceKind,
        FinamOrderEndpointResponseKind, FinamOrderExecutionOutcome,
        MockFinamApprovedOrderExecutionClient, MockFinamDryOrderClient,
    };
    use chrono::{DateTime, TimeZone};
    use rust_decimal::Decimal;
    use serde::de::DeserializeOwned;
    use uuid::Uuid;

    use super::*;

    #[tokio::test]
    async fn publishes_health_and_readiness_to_configured_streams() {
        let sink = InMemoryRedisStreamSink::default();
        let gateway = FinamGateway::new(GatewayConfig::default(), sink.clone());

        gateway
            .publish_health(default_readonly_health(gateway.config()))
            .await
            .expect("health published");
        gateway
            .publish_readiness(BrokerReadiness {
                phase: ReadinessPhase::Reconciliation,
                reasons: vec![ReadinessReason::OperatorLiveArmMissing],
                checked_ts: Utc::now(),
            })
            .await
            .expect("readiness published");

        let entries = sink.entries().expect("entries");
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].stream, "finam:health");
        assert_eq!(entries[1].stream, "finam:readiness");
        assert!(entries[0].payload.contains("\"schema_version\":2"));
    }

    #[tokio::test]
    async fn publishes_readonly_portfolio_and_order_snapshots() {
        let sink = InMemoryRedisStreamSink::default();
        let gateway = FinamGateway::new(GatewayConfig::default(), sink.clone());
        let account = sample_account();
        let orders = sample_orders();

        let summary = gateway
            .publish_readonly_snapshots(&account, &orders, Utc::now())
            .await
            .expect("snapshots");

        assert_eq!(summary.positions_count, 1);
        assert_eq!(summary.active_orders_count, 1);
        assert_eq!(summary.terminal_orders_count, 0);
        let entries = sink.entries().expect("entries");
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].stream, "finam:portfolio");
        assert_eq!(entries[1].stream, "finam:orders:snapshot");
    }

    #[tokio::test]
    async fn runs_readonly_reconciliation_once_with_snapshots_before_readiness() {
        let sink = InMemoryRedisStreamSink::default();
        let gateway = FinamGateway::new(GatewayConfig::default(), sink.clone());

        let report = gateway
            .run_readonly_reconciliation_once(&sample_account(), &sample_orders(), Utc::now())
            .await
            .expect("reconciliation report");

        assert_eq!(report.health.status, GatewayHealthStatus::ReadOnly);
        assert_eq!(report.readiness.phase, ReadinessPhase::Reconciliation);
        assert_eq!(report.summary.positions_count, 1);

        let streams = sink
            .entries()
            .expect("entries")
            .into_iter()
            .map(|entry| entry.stream)
            .collect::<Vec<_>>();
        assert_eq!(
            streams,
            vec![
                "finam:health",
                "finam:portfolio",
                "finam:orders:snapshot",
                "finam:readiness",
            ]
        );
    }

    #[tokio::test]
    async fn publishes_typed_broker_neutral_envelopes_for_shadow_contract() {
        let sink = InMemoryRedisStreamSink::default();
        let gateway = FinamGateway::new(GatewayConfig::default(), sink.clone());

        gateway
            .publish_health(default_readonly_health(gateway.config()))
            .await
            .expect("health published");
        gateway
            .publish_readonly_snapshots(&sample_account(), &sample_orders(), Utc::now())
            .await
            .expect("snapshots published");
        gateway
            .publish_market_data_event(MarketDataEvent::Quote(Quote {
                instrument: sample_instrument(),
                source_kind: MarketDataSourceKind::ReadOnlyPoll,
                bid: None,
                ask: None,
                last: Some(Decimal::new(5000, 0)),
                source_ts: None,
                received_ts: Utc::now(),
            }))
            .await
            .expect("market data published");
        gateway
            .publish_readiness(BrokerReadiness {
                phase: ReadinessPhase::Reconciliation,
                reasons: vec![ReadinessReason::OperatorLiveArmMissing],
                checked_ts: Utc::now(),
            })
            .await
            .expect("readiness published");

        let entries = sink.entries().expect("entries");
        let health: Envelope<GatewayHealth> =
            decode_stream_payload(&entries, "finam:health", MessageType::Health);
        assert_eq!(health.payload.status, GatewayHealthStatus::ReadOnly);

        let portfolio: Envelope<PortfolioSnapshot> =
            decode_stream_payload(&entries, "finam:portfolio", MessageType::PortfolioSnapshot);
        assert_eq!(portfolio.payload.positions.len(), 1);

        let orders: Envelope<OrderSnapshot> = decode_stream_payload(
            &entries,
            "finam:orders:snapshot",
            MessageType::OrderSnapshot,
        );
        assert_eq!(orders.payload.orders.len(), 1);
        let order = &orders.payload.orders[0];
        assert!(order.comment.is_none());
        assert!(order.comment_fingerprint.is_some());
        let order_snapshot_payload = entries
            .iter()
            .find(|entry| entry.stream == "finam:orders:snapshot")
            .expect("order snapshot payload")
            .payload
            .as_str();
        assert!(!order_snapshot_payload.contains("raw broker note must not leak"));

        let market_data: Envelope<MarketDataEvent> =
            decode_stream_payload(&entries, "finam:market-data", MessageType::MarketData);
        match market_data.payload {
            MarketDataEvent::Quote(quote) => {
                assert_eq!(quote.source_kind, MarketDataSourceKind::ReadOnlyPoll);
            }
            other => panic!("unexpected market data payload: {other:?}"),
        }

        let readiness: Envelope<BrokerReadiness> =
            decode_stream_payload(&entries, "finam:readiness", MessageType::Readiness);
        assert_eq!(readiness.payload.phase, ReadinessPhase::Reconciliation);
    }

    #[tokio::test]
    async fn publish_order_snapshot_redacts_comments_at_gateway_boundary() {
        let sink = InMemoryRedisStreamSink::default();
        let gateway = FinamGateway::new(GatewayConfig::default(), sink.clone());
        let snapshot = OrderSnapshot {
            orders: vec![sample_order_with_comment(Some(
                "raw gateway comment must not leak",
            ))],
            active_orders_count: 1,
            terminal_orders_count: 0,
            blocking_unknown_status_present: false,
            received_ts: Utc::now(),
        };

        gateway
            .publish_order_snapshot(snapshot)
            .await
            .expect("order snapshot published");

        let entries = sink.entries().expect("entries");
        let order_snapshot: Envelope<OrderSnapshot> = decode_stream_payload(
            &entries,
            "finam:orders:snapshot",
            MessageType::OrderSnapshot,
        );
        assert!(order_snapshot.payload.orders[0].comment.is_none());
        assert!(!entries[0]
            .payload
            .contains("raw gateway comment must not leak"));
    }

    #[tokio::test]
    async fn runtime_bridge_dry_consumer_accepts_allowed_shadow_streams() {
        let sink = InMemoryRedisStreamSink::default();
        let config = GatewayConfig::default();
        let gateway = FinamGateway::new(config.clone(), sink.clone());

        gateway
            .publish_health(default_readonly_health(gateway.config()))
            .await
            .expect("health published");
        gateway
            .publish_readonly_snapshots(&sample_account(), &sample_orders(), Utc::now())
            .await
            .expect("snapshots published");
        gateway
            .publish_market_data_event(MarketDataEvent::Quote(Quote {
                instrument: sample_instrument(),
                source_kind: MarketDataSourceKind::ReadOnlyPoll,
                bid: None,
                ask: None,
                last: Some(Decimal::new(5000, 0)),
                source_ts: None,
                received_ts: Utc::now(),
            }))
            .await
            .expect("market data published");
        gateway
            .publish_readiness(BrokerReadiness {
                phase: ReadinessPhase::Reconciliation,
                reasons: vec![ReadinessReason::OperatorLiveArmMissing],
                checked_ts: Utc::now(),
            })
            .await
            .expect("readiness published");

        let mut consumer = RuntimeBridgeDryConsumer::from_gateway_config(&config);
        let outcomes = runtime_entries(sink.entries().expect("entries"))
            .into_iter()
            .map(|entry| consumer.consume_entry(entry))
            .collect::<Vec<_>>();

        assert!(outcomes
            .iter()
            .all(|outcome| matches!(outcome, RuntimeBridgeConsumeOutcome::Accepted { .. })));
        assert_eq!(consumer.metrics().entries_seen, 5);
        assert_eq!(consumer.metrics().accepted_count, 5);
        assert_eq!(consumer.metrics().dlq_count, 0);
        assert_eq!(consumer.metrics().health_count, 1);
        assert_eq!(consumer.metrics().readiness_count, 1);
        assert_eq!(consumer.metrics().portfolio_snapshot_count, 1);
        assert_eq!(consumer.metrics().order_snapshot_count, 1);
        assert_eq!(consumer.metrics().market_data_count, 1);
    }

    #[tokio::test]
    async fn publishes_market_data_event_from_readonly_path() {
        let sink = InMemoryRedisStreamSink::default();
        let gateway = FinamGateway::new(GatewayConfig::default(), sink.clone());

        gateway
            .publish_market_data_event(MarketDataEvent::Quote(Quote {
                instrument: sample_instrument(),
                source_kind: MarketDataSourceKind::ReadOnlyPoll,
                bid: None,
                ask: None,
                last: Some(Decimal::new(5000, 0)),
                source_ts: None,
                received_ts: Utc::now(),
            }))
            .await
            .expect("market data published");

        let entries = sink.entries().expect("entries");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].stream, "finam:market-data");
        assert!(entries[0].payload.contains("\"msg_type\":\"MarketData\""));
    }

    #[tokio::test]
    async fn runtime_bridge_dry_consumer_dedupes_historical_bars() {
        let sink = InMemoryRedisStreamSink::default();
        let config = GatewayConfig::default();
        let gateway = FinamGateway::new(config.clone(), sink.clone());
        let bar = sample_bar(MarketDataSourceKind::HistoricalPoll, true);

        gateway
            .publish_market_data_event(MarketDataEvent::Bar(bar.clone()))
            .await
            .expect("first bar published");
        gateway
            .publish_market_data_event(MarketDataEvent::Bar(bar))
            .await
            .expect("duplicate bar published");

        let mut consumer = RuntimeBridgeDryConsumer::from_gateway_config(&config);
        let outcomes = runtime_entries(sink.entries().expect("entries"))
            .into_iter()
            .map(|entry| consumer.consume_entry(entry))
            .collect::<Vec<_>>();

        assert!(matches!(
            outcomes[0],
            RuntimeBridgeConsumeOutcome::Accepted {
                kind: RuntimeBridgePayloadKind::MarketData,
                ..
            }
        ));
        assert!(matches!(
            outcomes[1],
            RuntimeBridgeConsumeOutcome::DuplicateBar { .. }
        ));
        assert_eq!(consumer.metrics().accepted_count, 1);
        assert_eq!(consumer.metrics().market_data_count, 1);
        assert_eq!(consumer.metrics().duplicate_bar_count, 1);
        assert_eq!(consumer.metrics().dlq_count, 0);
    }

    #[tokio::test]
    async fn runtime_bridge_bar_dedupe_key_includes_source_kind_and_finality() {
        let sink = InMemoryRedisStreamSink::default();
        let config = GatewayConfig::default();
        let gateway = FinamGateway::new(config.clone(), sink.clone());

        gateway
            .publish_market_data_event(MarketDataEvent::Bar(sample_bar(
                MarketDataSourceKind::HistoricalPoll,
                true,
            )))
            .await
            .expect("historical final bar published");
        gateway
            .publish_market_data_event(MarketDataEvent::Bar(sample_bar(
                MarketDataSourceKind::LiveStream,
                true,
            )))
            .await
            .expect("live final bar published");
        gateway
            .publish_market_data_event(MarketDataEvent::Bar(sample_bar(
                MarketDataSourceKind::HistoricalPoll,
                false,
            )))
            .await
            .expect("historical forming bar published");
        gateway
            .publish_market_data_event(MarketDataEvent::Bar(sample_bar(
                MarketDataSourceKind::HistoricalPoll,
                true,
            )))
            .await
            .expect("duplicate historical final bar published");

        let mut consumer = RuntimeBridgeDryConsumer::from_gateway_config(&config);
        let outcomes = runtime_entries(sink.entries().expect("entries"))
            .into_iter()
            .map(|entry| consumer.consume_entry(entry))
            .collect::<Vec<_>>();

        assert!(matches!(
            outcomes[0],
            RuntimeBridgeConsumeOutcome::Accepted { .. }
        ));
        assert!(matches!(
            outcomes[1],
            RuntimeBridgeConsumeOutcome::Accepted { .. }
        ));
        assert!(matches!(
            outcomes[2],
            RuntimeBridgeConsumeOutcome::Accepted { .. }
        ));
        assert!(matches!(
            outcomes[3],
            RuntimeBridgeConsumeOutcome::DuplicateBar { .. }
        ));
        assert_eq!(consumer.metrics().accepted_count, 3);
        assert_eq!(consumer.metrics().duplicate_bar_count, 1);
    }

    #[test]
    fn runtime_bridge_dry_consumer_dlqs_stream_contract_violations() {
        let config = GatewayConfig::default();
        let mut consumer = RuntimeBridgeDryConsumer::from_gateway_config(&config);
        let wrong_type_payload = serde_json::to_string(&Envelope::new(
            config.source.clone(),
            MessageType::MarketData,
            MarketDataEvent::Quote(Quote {
                instrument: sample_instrument(),
                source_kind: MarketDataSourceKind::ReadOnlyPoll,
                bid: None,
                ask: None,
                last: Some(Decimal::new(5000, 0)),
                source_ts: None,
                received_ts: Utc::now(),
            }),
        ))
        .expect("payload");

        let outcome = consumer.consume_entry(RuntimeBridgeStreamEntry {
            stream: config.redis.health_stream.clone(),
            entry_id: "1-0".to_string(),
            payload: wrong_type_payload,
        });

        assert!(matches!(
            outcome,
            RuntimeBridgeConsumeOutcome::DeadLetter(RuntimeBridgeDeadLetter {
                reason: RuntimeBridgeDlqReason::MessageTypeMismatch {
                    expected: MessageType::Health,
                    actual: Some(MessageType::MarketData)
                },
                ..
            })
        ));
        assert_eq!(consumer.metrics().dlq_count, 1);
    }

    #[test]
    fn runtime_bridge_dry_consumer_reports_expected_type_on_decode_failure() {
        let config = GatewayConfig::default();
        let mut consumer = RuntimeBridgeDryConsumer::from_gateway_config(&config);
        let payload = serde_json::json!({
            "schema_version": 2,
            "ts_utc": "2026-06-29T09:10:00Z",
            "source": "finam-gateway",
            "msg_type": "Health",
            "payload": {}
        })
        .to_string();

        let outcome = consumer.consume_entry(RuntimeBridgeStreamEntry {
            stream: config.redis.health_stream.clone(),
            entry_id: "1-0".to_string(),
            payload,
        });

        assert!(matches!(
            outcome,
            RuntimeBridgeConsumeOutcome::DeadLetter(RuntimeBridgeDeadLetter {
                reason: RuntimeBridgeDlqReason::TypedDecodeFailed {
                    expected: RuntimeBridgePayloadKind::Health
                },
                ..
            })
        ));
        assert_eq!(consumer.metrics().dlq_count, 1);
    }

    #[test]
    fn runtime_bridge_dry_consumer_dlqs_raw_order_comments() {
        let config = GatewayConfig::default();
        let snapshot = OrderSnapshot {
            orders: vec![sample_order_with_comment(Some(
                "raw external comment must not leak",
            ))],
            active_orders_count: 1,
            terminal_orders_count: 0,
            blocking_unknown_status_present: false,
            received_ts: Utc::now(),
        };
        let payload = serde_json::to_string(&Envelope::new(
            config.source.clone(),
            MessageType::OrderSnapshot,
            snapshot,
        ))
        .expect("payload");
        let mut consumer = RuntimeBridgeDryConsumer::from_gateway_config(&config);

        let outcome = consumer.consume_entry(RuntimeBridgeStreamEntry {
            stream: config.redis.order_snapshot_stream.clone(),
            entry_id: "1-0".to_string(),
            payload,
        });

        assert!(matches!(
            outcome,
            RuntimeBridgeConsumeOutcome::DeadLetter(RuntimeBridgeDeadLetter {
                reason: RuntimeBridgeDlqReason::RawOrderCommentPresent,
                ..
            })
        ));
        assert_eq!(consumer.metrics().dlq_count, 1);
        assert_eq!(consumer.metrics().accepted_count, 0);
    }

    #[test]
    fn clean_order_snapshot_serialization_omits_raw_comment_field() {
        let snapshot = OrderSnapshot {
            orders: vec![sample_order_with_comment(None)],
            active_orders_count: 1,
            terminal_orders_count: 0,
            blocking_unknown_status_present: false,
            received_ts: Utc::now(),
        };

        let payload = serde_json::to_string(&Envelope::new(
            "finam-gateway",
            MessageType::OrderSnapshot,
            snapshot,
        ))
        .expect("snapshot serializes");

        assert!(!payload.contains("\"comment\""));
        assert!(!payload.contains("\"comment_fingerprint\""));
    }

    #[tokio::test]
    async fn applies_in_memory_stream_retention_per_stream() {
        let sink = InMemoryRedisStreamSink::default();
        let mut config = GatewayConfig::default();
        config.redis.retention.health_maxlen = Some(2);
        let gateway = FinamGateway::new(config, sink.clone());

        gateway
            .publish_health(default_readonly_health(gateway.config()))
            .await
            .expect("health 1");
        gateway
            .publish_health(default_readonly_health(gateway.config()))
            .await
            .expect("health 2");
        gateway
            .publish_health(default_readonly_health(gateway.config()))
            .await
            .expect("health 3");

        let entries = sink.entries().expect("entries");
        assert_eq!(entries.len(), 2);
        assert!(entries.iter().all(|entry| entry.stream == "finam:health"));
    }

    #[tokio::test]
    async fn runtime_bridge_readiness_simulator_reports_dry_ready_for_complete_shadow_inputs() {
        let sink = InMemoryRedisStreamSink::default();
        let config = GatewayConfig::default();
        let gateway = FinamGateway::new(config.clone(), sink.clone());

        gateway
            .publish_health(default_readonly_health(gateway.config()))
            .await
            .expect("health published");
        gateway
            .publish_readonly_snapshots(&sample_account(), &sample_orders(), Utc::now())
            .await
            .expect("snapshots published");
        gateway
            .publish_market_data_event(MarketDataEvent::Quote(Quote {
                instrument: sample_instrument(),
                source_kind: MarketDataSourceKind::ReadOnlyPoll,
                bid: None,
                ask: None,
                last: Some(Decimal::new(5000, 0)),
                source_ts: None,
                received_ts: Utc::now(),
            }))
            .await
            .expect("market data published");
        gateway
            .publish_readiness(BrokerReadiness {
                phase: ReadinessPhase::Reconciliation,
                reasons: vec![ReadinessReason::OperatorLiveArmMissing],
                checked_ts: Utc::now(),
            })
            .await
            .expect("readiness published");

        let mut simulator = RuntimeBridgeReadinessSimulator::from_gateway_config(&config);
        for entry in runtime_entries(sink.entries().expect("entries")) {
            simulator.observe_entry(&entry).expect("valid stream entry");
        }

        let decision = simulator.decision();
        assert_eq!(decision.phase, RuntimeBridgeDryReadinessPhase::DryReady);
        assert!(!decision.live_ready);
        assert!(decision.reasons.is_empty());
        assert!(decision.state.health_seen);
        assert!(decision.state.portfolio_snapshot_seen);
        assert!(decision.state.order_snapshot_seen);
        assert!(decision.state.market_data_seen);
    }

    #[test]
    fn runtime_bridge_readiness_simulator_blocks_on_dlq_and_unknown_orders() {
        let config = GatewayConfig::default();
        let mut simulator = RuntimeBridgeReadinessSimulator::from_gateway_config(&config);
        simulator.state.health_seen = true;
        simulator.state.health_status = Some(GatewayHealthStatus::ReadOnly);
        simulator.state.gateway_readiness_seen = true;
        simulator.state.gateway_readiness_phase = Some(ReadinessPhase::Reconciliation);
        simulator.state.portfolio_snapshot_seen = true;
        simulator.state.order_snapshot_seen = true;
        simulator.state.market_data_seen = true;
        simulator.state.blocking_unknown_orders = true;
        simulator.observe_dead_letter(&RuntimeBridgeDeadLetter {
            stream: config.redis.market_data_stream.clone(),
            entry_id: "1-0".to_string(),
            reason: RuntimeBridgeDlqReason::InvalidJson,
            payload_len: 7,
        });

        let decision = simulator.decision();
        assert_eq!(decision.phase, RuntimeBridgeDryReadinessPhase::Blocked);
        assert!(!decision.live_ready);
        assert!(decision
            .reasons
            .contains(&RuntimeBridgeDryReadinessReason::UnknownOpenOrders));
        assert!(decision
            .reasons
            .contains(&RuntimeBridgeDryReadinessReason::DeadLettersPresent { count: 1 }));
    }

    #[test]
    fn runtime_bridge_readiness_simulator_degrades_on_gateway_health_status() {
        let config = GatewayConfig::default();
        let mut degraded = RuntimeBridgeReadinessSimulator::from_gateway_config(&config);
        degraded.state.health_seen = true;
        degraded.state.health_status = Some(GatewayHealthStatus::Degraded);
        degraded.state.gateway_readiness_seen = true;
        degraded.state.gateway_readiness_phase = Some(ReadinessPhase::Reconciliation);
        degraded.state.portfolio_snapshot_seen = true;
        degraded.state.order_snapshot_seen = true;
        degraded.state.market_data_seen = true;

        let degraded_decision = degraded.decision();
        assert_eq!(
            degraded_decision.phase,
            RuntimeBridgeDryReadinessPhase::Degraded
        );
        assert!(!degraded_decision.live_ready);
        assert!(degraded_decision.reasons.contains(
            &RuntimeBridgeDryReadinessReason::HealthNotReadOnly {
                status: GatewayHealthStatus::Degraded
            }
        ));

        let mut stopped = RuntimeBridgeReadinessSimulator::from_gateway_config(&config);
        stopped.state.health_seen = true;
        stopped.state.health_status = Some(GatewayHealthStatus::Stopped);
        stopped.state.gateway_readiness_seen = true;
        stopped.state.gateway_readiness_phase = Some(ReadinessPhase::Reconciliation);
        stopped.state.portfolio_snapshot_seen = true;
        stopped.state.order_snapshot_seen = true;
        stopped.state.market_data_seen = true;

        let stopped_decision = stopped.decision();
        assert_eq!(
            stopped_decision.phase,
            RuntimeBridgeDryReadinessPhase::Degraded
        );
        assert!(!stopped_decision.live_ready);
        assert!(stopped_decision.reasons.contains(
            &RuntimeBridgeDryReadinessReason::HealthNotReadOnly {
                status: GatewayHealthStatus::Stopped
            }
        ));
    }

    #[test]
    fn runtime_bridge_readiness_simulator_degrades_on_gateway_readiness_phase() {
        let config = GatewayConfig::default();
        let mut degraded = RuntimeBridgeReadinessSimulator::from_gateway_config(&config);
        degraded.state.health_seen = true;
        degraded.state.health_status = Some(GatewayHealthStatus::ReadOnly);
        degraded.state.gateway_readiness_seen = true;
        degraded.state.gateway_readiness_phase = Some(ReadinessPhase::Degraded);
        degraded.state.portfolio_snapshot_seen = true;
        degraded.state.order_snapshot_seen = true;
        degraded.state.market_data_seen = true;

        let degraded_decision = degraded.decision();
        assert_eq!(
            degraded_decision.phase,
            RuntimeBridgeDryReadinessPhase::Degraded
        );
        assert!(!degraded_decision.live_ready);
        assert!(degraded_decision.reasons.contains(
            &RuntimeBridgeDryReadinessReason::GatewayReadinessNotReconciliation {
                phase: ReadinessPhase::Degraded
            }
        ));

        let mut stopped = RuntimeBridgeReadinessSimulator::from_gateway_config(&config);
        stopped.state.health_seen = true;
        stopped.state.health_status = Some(GatewayHealthStatus::ReadOnly);
        stopped.state.gateway_readiness_seen = true;
        stopped.state.gateway_readiness_phase = Some(ReadinessPhase::Stopped);
        stopped.state.portfolio_snapshot_seen = true;
        stopped.state.order_snapshot_seen = true;
        stopped.state.market_data_seen = true;

        let stopped_decision = stopped.decision();
        assert_eq!(
            stopped_decision.phase,
            RuntimeBridgeDryReadinessPhase::Degraded
        );
        assert!(!stopped_decision.live_ready);
        assert!(stopped_decision.reasons.contains(
            &RuntimeBridgeDryReadinessReason::GatewayReadinessNotReconciliation {
                phase: ReadinessPhase::Stopped
            }
        ));
    }

    #[test]
    fn runtime_bridge_dlq_record_serializes_without_raw_payload() {
        let record = RuntimeBridgeDlqRecord::new(
            "finam-gateway",
            "broker-runtime-bridge-dry",
            "dry-consumer-1",
            RuntimeBridgeDeadLetter {
                stream: "broker.market_data".to_string(),
                entry_id: "1-0".to_string(),
                reason: RuntimeBridgeDlqReason::InvalidJson,
                payload_len: 42,
            },
        );

        let payload = serde_json::to_string(&record).expect("DLQ record serializes");

        assert!(payload.contains("\"schema_version\":2"));
        assert!(payload.contains("\"consumer_group\":\"broker-runtime-bridge-dry\""));
        assert!(payload.contains("\"payload_len\":42"));
        assert!(payload.contains("\"InvalidJson\""));
        assert!(!payload.contains("raw Redis payload"));
        assert!(!payload.contains("\"payload\":"));
    }

    #[test]
    fn runtime_bridge_dlq_retention_default_is_bounded() {
        assert_eq!(
            RedisRetentionConfig::default().runtime_bridge_dlq_maxlen,
            Some(1_000)
        );
        assert_eq!(
            RedisStreamConfig::default().runtime_bridge_dlq_stream,
            "finam:runtime-bridge:dlq"
        );
    }

    #[test]
    fn degraded_and_stopped_readiness_are_not_live_ready() {
        let degraded = degraded_readiness(ReadinessReason::RedisUnavailable);
        assert_eq!(degraded.phase, ReadinessPhase::Degraded);
        assert_eq!(degraded.reasons, vec![ReadinessReason::RedisUnavailable]);

        let stopped = stopped_readiness();
        assert_eq!(stopped.phase, ReadinessPhase::Stopped);
        assert_eq!(stopped.reasons, vec![ReadinessReason::OperatorPaused]);
    }

    #[test]
    fn readiness_blocks_on_unknown_order_status() {
        let summary = ReadonlySnapshotSummary {
            cash_count: 1,
            positions_count: 0,
            orders_count: 1,
            active_orders_count: 0,
            terminal_orders_count: 0,
            blocking_unknown_status_present: true,
        };

        let readiness = readiness_from_readonly_summary(&summary);

        assert_eq!(readiness.phase, ReadinessPhase::Blocked);
        assert_eq!(readiness.reasons, vec![ReadinessReason::UnknownOpenOrders]);
    }

    #[test]
    fn command_consumer_is_feature_disabled() {
        let gateway =
            FinamGateway::new(GatewayConfig::default(), InMemoryRedisStreamSink::default());
        let request_id = StrategyRequestId::from(
            Uuid::parse_str("00000000-0000-4000-8000-000000000002").expect("uuid"),
        );
        let command = BrokerCommand::PlaceOrder(broker_core::PlaceOrder {
            request_id,
            created_ts: Utc
                .with_ymd_and_hms(2026, 6, 29, 9, 10, 0)
                .single()
                .expect("timestamp"),
            ttl_ms: Some(1_000),
            account_id: broker_core::BrokerAccountId::new("ACC_TEST_0001"),
            client_order_id: ClientOrderId::new("ABC123").expect("client order id"),
            instrument: sample_instrument(),
            side: OrderSide::Buy,
            order_type: OrderType::Limit,
            qty: Decimal::ONE,
            limit_price: Some(Decimal::new(5000, 0)),
            time_in_force: TimeInForce::Day,
            comment: None,
        });

        let ack = gateway.reject_command_feature_disabled(&command);

        assert_eq!(ack.status, CommandAckStatus::Rejected);
        assert_eq!(
            ack.reason.expect("reason").code,
            CommandAckReasonCode::FeatureDisabled
        );
    }

    #[tokio::test]
    async fn publishes_dry_command_ack_as_redacted_command_ack_envelope() {
        let sink = InMemoryRedisStreamSink::default();
        let gateway = FinamGateway::new(GatewayConfig::default(), sink.clone());
        let request_id = request_id(3);
        let ack = CommandAck {
            request_id,
            client_order_id: Some(ClientOrderId::new("CID000000000000003").expect("client id")),
            broker_order_id: Some(BrokerOrderId::new("BROKER_TEST_3")),
            status: CommandAckStatus::Submitted,
            reason: Some(CommandAckReason::synthetic_submitted()),
            received_ts: Utc
                .with_ymd_and_hms(2026, 6, 30, 9, 10, 0)
                .single()
                .expect("timestamp"),
        };

        gateway
            .publish_dry_command_ack(ack)
            .await
            .expect("dry ACK published");

        let entries = sink.entries().expect("entries");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].stream, "finam:command-acks");
        assert!(entries[0].payload.contains("\"msg_type\":\"CommandAck\""));
        assert!(!entries[0].payload.contains("CID000000000000003"));
        assert!(!entries[0].payload.contains("BROKER_TEST_3"));

        let envelope: Envelope<CommandAck> =
            decode_stream_payload(&entries, "finam:command-acks", MessageType::CommandAck);
        assert_eq!(envelope.payload.request_id, request_id);
        assert_eq!(envelope.payload.status, CommandAckStatus::Submitted);
        assert_eq!(
            envelope.payload.reason.expect("reason").code,
            CommandAckReasonCode::SyntheticSubmitted
        );
        assert!(envelope.payload.client_order_id.is_none());
        assert!(envelope.payload.broker_order_id.is_none());
    }

    #[tokio::test]
    async fn dry_command_ack_publisher_refuses_order_enabled_modes() {
        fn enable_command_consumer(features: &mut GatewayFeatureSet) {
            features.command_consumer_enabled = true;
        }
        fn enable_order_placement(features: &mut GatewayFeatureSet) {
            features.order_placement_enabled = true;
        }
        fn enable_cancel(features: &mut GatewayFeatureSet) {
            features.cancel_enabled = true;
        }
        fn enable_stop_sltp_bracket(features: &mut GatewayFeatureSet) {
            features.stop_sltp_bracket_enabled = true;
        }

        for (configure, reason) in [
            (
                enable_command_consumer as fn(&mut GatewayFeatureSet),
                "command_consumer_enabled",
            ),
            (
                enable_order_placement as fn(&mut GatewayFeatureSet),
                "order_placement_enabled",
            ),
            (
                enable_cancel as fn(&mut GatewayFeatureSet),
                "cancel_enabled",
            ),
            (
                enable_stop_sltp_bracket as fn(&mut GatewayFeatureSet),
                "stop_sltp_bracket_enabled",
            ),
        ] {
            let mut config = GatewayConfig::default();
            configure(&mut config.features);
            let gateway = FinamGateway::new(config, InMemoryRedisStreamSink::default());
            let ack = CommandAck {
                request_id: request_id(4),
                client_order_id: None,
                broker_order_id: None,
                status: CommandAckStatus::Submitted,
                reason: Some(CommandAckReason::synthetic_submitted()),
                received_ts: Utc::now(),
            };

            assert!(matches!(
                gateway
                    .publish_dry_command_ack(ack)
                    .await
                    .expect_err("order mode must be refused"),
                GatewayError::DryCommandAckPublisherUnsafeMode {
                    reason: actual
                } if actual == reason
            ));
        }
    }

    #[test]
    fn real_order_endpoint_gate_remains_blocked_until_post_m3a11_review() {
        let gateway =
            FinamGateway::new(GatewayConfig::default(), InMemoryRedisStreamSink::default());

        let decision = gateway.real_order_endpoint_gate_decision();

        assert!(!decision.endpoint_calls_allowed);
        assert_eq!(
            decision.blocking_reasons,
            vec![RealOrderEndpointGateBlock::M3a11PreEndpointReviewRequired]
        );
        assert_eq!(
            decision.runtime_ack_id_policy,
            RuntimeCommandAckIdPolicy::RedactedRuntimeAckOnly
        );
        assert!(matches!(
            EndpointGateApproved::try_from_decision(&decision)
                .expect_err("M3b-0 marker must not be constructible"),
            EndpointGateApprovalError::Blocked { decision } if decision
                .blocking_reasons
                .contains(&RealOrderEndpointGateBlock::M3a11PreEndpointReviewRequired)
        ));

        let features = GatewayFeatureSet {
            command_consumer_enabled: true,
            order_placement_enabled: true,
            cancel_enabled: true,
            stop_sltp_bracket_enabled: true,
            ..GatewayFeatureSet::default()
        };
        let flagged = features.real_order_endpoint_gate_decision();
        assert!(!flagged.endpoint_calls_allowed);
        assert_eq!(
            flagged.blocking_reasons,
            vec![
                RealOrderEndpointGateBlock::M3a11PreEndpointReviewRequired,
                RealOrderEndpointGateBlock::CommandConsumerEnabled,
                RealOrderEndpointGateBlock::OrderPlacementEnabled,
                RealOrderEndpointGateBlock::CancelEnabled,
                RealOrderEndpointGateBlock::StopSltpBracketEnabled,
            ]
        );
        assert!(EndpointGateApproved::try_from_decision(&flagged).is_err());
    }

    #[test]
    fn endpoint_gate_marker_cannot_be_forged_from_manual_decision() {
        let manual_decision = RealOrderEndpointGateDecision {
            endpoint_calls_allowed: true,
            blocking_reasons: vec![],
            runtime_ack_id_policy: RuntimeCommandAckIdPolicy::RedactedRuntimeAckOnly,
        };

        assert!(matches!(
            EndpointGateApproved::try_from_decision(&manual_decision)
                .expect_err("manual decision must not forge endpoint approval"),
            EndpointGateApprovalError::Blocked { decision } if !decision.endpoint_calls_allowed
                && decision
                    .blocking_reasons
                    .contains(&RealOrderEndpointGateBlock::M3a11PreEndpointReviewRequired)
        ));
    }

    #[test]
    fn future_real_endpoint_transport_signature_requires_gate_marker() {
        fn assert_transport<T: FinamRealOrderEndpointTransport>() {}
        fn assert_place_signature<T: FinamRealOrderEndpointTransport>(
            _transport: &mut T,
            _gate: &EndpointGateApproved,
            _spec: broker_finam::FinamPlaceOrderRequestSpec,
        ) {
        }
        fn assert_cancel_signature<T: FinamRealOrderEndpointTransport>(
            _transport: &mut T,
            _gate: &EndpointGateApproved,
            _spec: broker_finam::FinamCancelOrderRequestSpec,
        ) {
        }

        struct CompileOnlyTransport;

        #[async_trait::async_trait]
        impl FinamRealOrderEndpointTransport for CompileOnlyTransport {
            async fn place_order_endpoint(
                &mut self,
                _gate: &EndpointGateApproved,
                _spec: broker_finam::FinamPlaceOrderRequestSpec,
            ) -> Result<
                broker_finam::FinamOrderEndpointMappedResult,
                broker_finam::FinamOrderExecutionError,
            > {
                Err(broker_finam::FinamOrderExecutionError::MockScriptExhausted)
            }

            async fn cancel_order_endpoint(
                &mut self,
                _gate: &EndpointGateApproved,
                _spec: broker_finam::FinamCancelOrderRequestSpec,
            ) -> Result<
                broker_finam::FinamOrderEndpointMappedResult,
                broker_finam::FinamOrderExecutionError,
            > {
                Err(broker_finam::FinamOrderExecutionError::MockScriptExhausted)
            }
        }

        assert_transport::<CompileOnlyTransport>();
        let mut transport = CompileOnlyTransport;
        let order = sample_place_order(request_id(20), "CID000000000000020", Utc::now());
        let approved = dry_preflight_policy(order.created_ts)
            .approve_place_order(&order, order.created_ts)
            .expect("approved");
        let place_spec = broker_finam::build_place_order_request(&approved, None).expect("spec");
        let (_store, approved_cancel, _) = submitted_store_and_approved_cancel(
            Utc::now(),
            request_id(21),
            request_id(22),
            BrokerOrderId::new("BROKER_TEST_GATE"),
        );
        let cancel_spec = broker_finam::build_cancel_order_request(&approved_cancel).expect("spec");
        let maybe_gate = EndpointGateApproved::try_from_decision(
            &GatewayFeatureSet::default().real_order_endpoint_gate_decision(),
        )
        .ok();
        if let Some(gate) = maybe_gate.as_ref() {
            assert_place_signature(&mut transport, gate, place_spec);
            assert_cancel_signature(&mut transport, gate, cancel_spec);
        }
    }

    #[tokio::test]
    async fn dry_order_path_integrates_preflight_store_request_spec_mock_client_and_ack() {
        let now = Utc
            .with_ymd_and_hms(2026, 6, 30, 9, 10, 0)
            .single()
            .expect("timestamp");
        let order = sample_place_order(request_id(5), "CID000000000000005", now);
        let policy = dry_preflight_policy(now);
        let approved = policy
            .approve_place_order(&order, now)
            .expect("preflight approved");
        let mut rate_limit = DryOrderRateLimit::new(1);
        rate_limit.try_consume(1).expect("dry rate-limit permit");
        let mut store = InMemoryOrderPathStore::default();
        store
            .insert_intent(OrderPathRecord::from_place_order(
                approved.order(),
                now,
                None,
            ))
            .expect("intent persisted before dry request spec");

        let request_spec =
            broker_finam::build_place_order_request(&approved, None).expect("dry spec");
        let mut dry_client = MockFinamDryOrderClient::default();
        let diagnostic = dry_client.record_place_order_request(&request_spec);

        assert_eq!(
            diagnostic.kind,
            broker_finam::FinamDryOrderRequestKind::Place
        );
        let diagnostic_json = serde_json::to_string(dry_client.requests()).expect("diagnostics");
        assert!(!diagnostic_json.contains("ACC_TEST_0001"));
        assert!(!diagnostic_json.contains("CID000000000000005"));
        assert!(!diagnostic_json.contains("TESTFUT@TEST"));

        let mut record = store
            .load_by_request_id(order.request_id)
            .expect("record persisted");
        record
            .transition(
                OrderPathEvent::BeginSubmit,
                now + chrono::Duration::milliseconds(1),
            )
            .expect("begin dry submit");
        record
            .transition(
                OrderPathEvent::SubmitAccepted,
                now + chrono::Duration::milliseconds(2),
            )
            .expect("synthetic submit accepted");
        store
            .update_record(record.clone())
            .expect("persist submitted");
        let ack = record.synthetic_ack(
            CommandAckStatus::Submitted,
            Some(CommandAckReason::synthetic_submitted()),
            now + chrono::Duration::milliseconds(2),
        );
        let sink = InMemoryRedisStreamSink::default();
        let gateway = FinamGateway::new(GatewayConfig::default(), sink.clone());

        gateway
            .publish_dry_command_ack(ack)
            .await
            .expect("dry ack published");

        let entries = sink.entries().expect("entries");
        let ack_envelope: Envelope<CommandAck> =
            decode_stream_payload(&entries, "finam:command-acks", MessageType::CommandAck);
        assert_eq!(ack_envelope.payload.request_id, order.request_id);
        assert_eq!(ack_envelope.payload.status, CommandAckStatus::Submitted);
        assert!(ack_envelope.payload.client_order_id.is_none());
        assert!(entries[0].payload.contains("synthetic_submitted"));
        assert!(!entries[0].payload.contains("ACC_TEST_0001"));
        assert!(!entries[0].payload.contains("CID000000000000005"));
    }

    #[tokio::test]
    async fn execution_simulator_submits_approved_place_and_redacts_published_ack() {
        let now = Utc
            .with_ymd_and_hms(2026, 6, 30, 9, 20, 0)
            .single()
            .expect("timestamp");
        let order = sample_place_order(request_id(6), "CID000000000000006", now);
        let policy = dry_preflight_policy(now);
        let approved = policy
            .approve_place_order(&order, now)
            .expect("preflight approved");
        let mut store = InMemoryOrderPathStore::default();
        store
            .insert_intent(OrderPathRecord::from_place_order(
                approved.order(),
                now,
                None,
            ))
            .expect("intent inserted");
        let mut client = MockFinamApprovedOrderExecutionClient::new(vec![
            FinamOrderExecutionOutcome::Accepted {
                broker_order_id: Some(BrokerOrderId::new("BROKER_TEST_6")),
            },
        ]);

        let report = simulate_place_order_approved(
            &mut store,
            &mut client,
            &approved,
            None,
            now + chrono::Duration::milliseconds(1),
            now + chrono::Duration::milliseconds(2),
        )
        .await
        .expect("simulation report");

        assert_eq!(report.outcome, DryOrderExecutionOutcomeKind::Submitted);
        assert_eq!(report.state, OrderPathState::Submitted);
        assert_eq!(report.submit_attempt_count, 1);
        assert_eq!(report.ack.status, CommandAckStatus::Submitted);
        assert_eq!(
            report.ack.broker_order_id,
            Some(BrokerOrderId::new("BROKER_TEST_6"))
        );
        let loaded = store
            .load_by_request_id(order.request_id)
            .expect("stored submitted record");
        assert_eq!(loaded.state, OrderPathState::Submitted);
        assert_eq!(
            loaded.broker_order_id,
            Some(BrokerOrderId::new("BROKER_TEST_6"))
        );
        let records_json = serde_json::to_string(client.records()).expect("records");
        assert!(!records_json.contains("ACC_TEST_0001"));
        assert!(!records_json.contains("BROKER_TEST_6"));
        assert!(!records_json.contains("CID000000000000006"));

        let sink = InMemoryRedisStreamSink::default();
        let gateway = FinamGateway::new(GatewayConfig::default(), sink.clone());
        gateway
            .publish_dry_command_ack(report.ack)
            .await
            .expect("ack published");
        let entries = sink.entries().expect("entries");
        assert!(!entries[0].payload.contains("BROKER_TEST_6"));
        assert!(!entries[0].payload.contains("CID000000000000006"));
    }

    #[tokio::test]
    async fn sqlite_backed_place_simulator_persists_begin_submit_before_mock_call_and_redacts_ack()
    {
        let now = Utc
            .with_ymd_and_hms(2026, 7, 1, 9, 20, 0)
            .single()
            .expect("timestamp");
        let path = temp_sqlite_path("place_begin_submit");
        cleanup_sqlite_path(&path);
        let order = sample_place_order(request_id(106), "CID000000000000106", now);
        let policy = dry_preflight_policy(now);
        let approved = policy
            .approve_place_order(&order, now)
            .expect("preflight approved");
        let mut store = SqliteOrderPathStore::open(&path).expect("open sqlite store");
        store
            .insert_intent(OrderPathRecord::from_place_order(
                approved.order(),
                now,
                None,
            ))
            .expect("intent inserted");
        let mut client = InspectingExecutionClient::new(
            path.clone(),
            order.request_id,
            OrderPathState::SubmitInFlight,
            FinamOrderExecutionOutcome::Accepted {
                broker_order_id: Some(BrokerOrderId::new("BROKER_TEST_SQLITE_PLACE")),
            },
        );

        let report = simulate_place_order_approved(
            &mut store,
            &mut client,
            &approved,
            None,
            now + chrono::Duration::milliseconds(1),
            now + chrono::Duration::milliseconds(2),
        )
        .await
        .expect("sqlite-backed simulation report");

        assert_eq!(client.call_count, 1);
        assert_eq!(report.outcome, DryOrderExecutionOutcomeKind::Submitted);
        assert_eq!(report.state, OrderPathState::Submitted);
        assert_eq!(report.ack.status, CommandAckStatus::Submitted);
        let loaded = store
            .load_by_request_id(order.request_id)
            .expect("stored submitted record");
        assert_eq!(loaded.state, OrderPathState::Submitted);
        assert_eq!(
            loaded.broker_order_id,
            Some(BrokerOrderId::new("BROKER_TEST_SQLITE_PLACE"))
        );
        let audit = store.transition_audit().expect("transition audit");
        assert_eq!(audit.len(), 3);
        assert_eq!(audit[0].event, "InsertIntent");
        assert_eq!(audit[0].from_state, None);
        assert_eq!(audit[0].to_state, "IntentRecorded");
        assert_eq!(audit[1].event, "BeginSubmit");
        assert_eq!(audit[1].from_state.as_deref(), Some("IntentRecorded"));
        assert_eq!(audit[1].to_state, "SubmitInFlight");
        assert_eq!(audit[2].event, "SubmitAccepted");
        assert_eq!(audit[2].from_state.as_deref(), Some("SubmitInFlight"));
        assert_eq!(audit[2].to_state, "Submitted");
        drop(store);

        {
            let readonly =
                SqliteOrderPathReadStore::open_readonly(&path).expect("open read-only diagnostics");
            assert_eq!(
                readonly
                    .operator_load_by_request_id(order.request_id)
                    .expect("read-only record")
                    .state,
                OrderPathState::Submitted
            );
            assert_eq!(readonly.transition_audit().expect("read audit").len(), 3);
        }

        let sink = InMemoryRedisStreamSink::default();
        let gateway = FinamGateway::new(GatewayConfig::default(), sink.clone());
        gateway
            .publish_dry_command_ack(report.ack)
            .await
            .expect("ack published");
        let entries = sink.entries().expect("entries");
        assert!(!entries[0].payload.contains("BROKER_TEST_SQLITE_PLACE"));
        assert!(!entries[0].payload.contains("CID000000000000106"));
        assert!(!entries[0].payload.contains("ACC_TEST_0001"));
        cleanup_sqlite_path(&path);
    }

    #[tokio::test]
    async fn execution_simulator_maps_rejected_place_to_broker_rejected_state() {
        let now = Utc
            .with_ymd_and_hms(2026, 6, 30, 9, 30, 0)
            .single()
            .expect("timestamp");
        let order = sample_place_order(request_id(7), "CID000000000000007", now);
        let policy = dry_preflight_policy(now);
        let approved = policy
            .approve_place_order(&order, now)
            .expect("preflight approved");
        let mut store = InMemoryOrderPathStore::default();
        store
            .insert_intent(OrderPathRecord::from_place_order(
                approved.order(),
                now,
                None,
            ))
            .expect("intent inserted");
        let mut client = MockFinamApprovedOrderExecutionClient::new(vec![
            FinamOrderExecutionOutcome::Rejected {
                reason_code: CommandAckReasonCode::BrokerRejected,
            },
        ]);

        let report = simulate_place_order_approved(
            &mut store,
            &mut client,
            &approved,
            None,
            now + chrono::Duration::milliseconds(1),
            now + chrono::Duration::milliseconds(2),
        )
        .await
        .expect("simulation report");

        assert_eq!(report.outcome, DryOrderExecutionOutcomeKind::BrokerRejected);
        assert_eq!(report.state, OrderPathState::BrokerRejected);
        assert_eq!(report.ack.status, CommandAckStatus::Rejected);
        assert_eq!(
            report.ack.reason.expect("reason").code,
            CommandAckReasonCode::BrokerRejected
        );
        assert_eq!(
            store
                .load_by_request_id(order.request_id)
                .expect("record")
                .state,
            OrderPathState::BrokerRejected
        );
    }

    #[tokio::test]
    async fn execution_simulator_timeout_blocks_blind_place_retry() {
        let now = Utc
            .with_ymd_and_hms(2026, 6, 30, 9, 40, 0)
            .single()
            .expect("timestamp");
        let order = sample_place_order(request_id(8), "CID000000000000008", now);
        let policy = dry_preflight_policy(now);
        let approved = policy
            .approve_place_order(&order, now)
            .expect("preflight approved");
        let mut store = InMemoryOrderPathStore::default();
        store
            .insert_intent(OrderPathRecord::from_place_order(
                approved.order(),
                now,
                None,
            ))
            .expect("intent inserted");
        let mut client = MockFinamApprovedOrderExecutionClient::new(vec![
            FinamOrderExecutionOutcome::Timeout,
            FinamOrderExecutionOutcome::Accepted {
                broker_order_id: Some(BrokerOrderId::new("BROKER_TEST_RETRY")),
            },
        ]);

        let report = simulate_place_order_approved(
            &mut store,
            &mut client,
            &approved,
            None,
            now + chrono::Duration::milliseconds(1),
            now + chrono::Duration::milliseconds(2),
        )
        .await
        .expect("timeout simulation report");

        assert_eq!(
            report.outcome,
            DryOrderExecutionOutcomeKind::TimeoutUnknownPending
        );
        assert_eq!(report.state, OrderPathState::TimeoutUnknownPending);
        assert_eq!(report.ack.status, CommandAckStatus::Timeout);
        assert_eq!(
            report.ack.reason.expect("reason").code,
            CommandAckReasonCode::TransportTimeout
        );

        let retry_error = simulate_place_order_approved(
            &mut store,
            &mut client,
            &approved,
            None,
            now + chrono::Duration::milliseconds(3),
            now + chrono::Duration::milliseconds(4),
        )
        .await
        .expect_err("blind retry must be blocked before mock client call");

        assert!(matches!(
            retry_error,
            DryOrderExecutionSimulatorError::Transition(
                OrderPathTransitionError::InvalidTransition {
                    from: OrderPathState::TimeoutUnknownPending,
                    event: OrderPathEvent::BeginSubmit
                }
            )
        ));
        assert_eq!(client.records().len(), 1);
        assert_eq!(
            store
                .load_by_request_id(order.request_id)
                .expect("record")
                .state,
            OrderPathState::TimeoutUnknownPending
        );
    }

    #[tokio::test]
    async fn execution_simulator_accepted_without_broker_id_stays_pending_reconciliation() {
        let now = Utc
            .with_ymd_and_hms(2026, 6, 30, 9, 50, 0)
            .single()
            .expect("timestamp");
        let order = sample_place_order(request_id(9), "CID000000000000009", now);
        let policy = dry_preflight_policy(now);
        let approved = policy
            .approve_place_order(&order, now)
            .expect("preflight approved");
        let mut store = InMemoryOrderPathStore::default();
        store
            .insert_intent(OrderPathRecord::from_place_order(
                approved.order(),
                now,
                None,
            ))
            .expect("intent inserted");
        let mut client = MockFinamApprovedOrderExecutionClient::new(vec![
            FinamOrderExecutionOutcome::Accepted {
                broker_order_id: None,
            },
        ]);

        let report = simulate_place_order_approved(
            &mut store,
            &mut client,
            &approved,
            None,
            now + chrono::Duration::milliseconds(1),
            now + chrono::Duration::milliseconds(2),
        )
        .await
        .expect("simulation report");

        assert_eq!(
            report.outcome,
            DryOrderExecutionOutcomeKind::SubmittedPendingBrokerOrderId
        );
        assert_eq!(report.state, OrderPathState::SubmittedPendingBrokerOrderId);
        assert_eq!(report.ack.status, CommandAckStatus::UnknownPending);
        assert_eq!(
            report.ack.reason.expect("reason").code,
            CommandAckReasonCode::ReconciliationRequired
        );
        let loaded = store
            .load_by_request_id(order.request_id)
            .expect("stored pending record");
        assert_eq!(loaded.state, OrderPathState::SubmittedPendingBrokerOrderId);
        assert!(loaded.broker_order_id.is_none());
    }

    #[test]
    fn endpoint_response_integration_maps_place_execution_fixtures() {
        let now = Utc
            .with_ymd_and_hms(2026, 7, 1, 11, 0, 0)
            .single()
            .expect("timestamp");

        let (mut accepted_store, accepted_approved, accepted_order) =
            place_store_and_approved(now, request_id(130), "CID000000000000130");
        let accepted_fixture = FinamOrderEndpointFixture::Accepted(FinamOrderEndpointAcceptedDto {
            broker_order_id: Some("BROKER_TEST_ENDPOINT_130".to_string()),
        });
        let accepted_report = simulate_place_order_endpoint_fixture(
            &mut accepted_store,
            &accepted_approved,
            None,
            &accepted_fixture,
            None,
            now + chrono::Duration::milliseconds(1),
            now + chrono::Duration::milliseconds(2),
        )
        .expect("accepted endpoint fixture");

        assert_eq!(
            accepted_report.outcome,
            EndpointResponseIntegrationOutcomeKind::Submitted
        );
        assert_eq!(accepted_report.state, OrderPathState::Submitted);
        assert_eq!(accepted_report.ack.status, CommandAckStatus::Submitted);
        assert_eq!(
            accepted_report.endpoint_response.kind,
            FinamOrderEndpointResponseKind::Accepted
        );
        assert_eq!(
            accepted_store
                .load_by_request_id(accepted_order.request_id)
                .expect("accepted record")
                .broker_order_id,
            Some(BrokerOrderId::new("BROKER_TEST_ENDPOINT_130"))
        );

        let (mut no_id_store, no_id_approved, _) =
            place_store_and_approved(now, request_id(131), "CID000000000000131");
        let no_id_fixture = FinamOrderEndpointFixture::Accepted(FinamOrderEndpointAcceptedDto {
            broker_order_id: None,
        });
        let no_id_report = simulate_place_order_endpoint_fixture(
            &mut no_id_store,
            &no_id_approved,
            None,
            &no_id_fixture,
            None,
            now + chrono::Duration::milliseconds(3),
            now + chrono::Duration::milliseconds(4),
        )
        .expect("accepted without id endpoint fixture");
        assert_eq!(
            no_id_report.outcome,
            EndpointResponseIntegrationOutcomeKind::SubmittedPendingBrokerOrderId
        );
        assert_eq!(
            no_id_report.state,
            OrderPathState::SubmittedPendingBrokerOrderId
        );
        assert_eq!(no_id_report.ack.status, CommandAckStatus::UnknownPending);
        assert_eq!(
            no_id_report.ack.reason.expect("reason").code,
            CommandAckReasonCode::ReconciliationRequired
        );

        let (mut rejected_store, rejected_approved, _) =
            place_store_and_approved(now, request_id(132), "CID000000000000132");
        let rejected_fixture = FinamOrderEndpointFixture::Rejected {
            reason_code: broker_finam::FinamOrderEndpointRejectedCode::BrokerRejected,
        };
        let rejected_report = simulate_place_order_endpoint_fixture(
            &mut rejected_store,
            &rejected_approved,
            None,
            &rejected_fixture,
            None,
            now + chrono::Duration::milliseconds(5),
            now + chrono::Duration::milliseconds(6),
        )
        .expect("rejected endpoint fixture");
        assert_eq!(
            rejected_report.outcome,
            EndpointResponseIntegrationOutcomeKind::BrokerRejected
        );
        assert_eq!(rejected_report.state, OrderPathState::BrokerRejected);
        assert_eq!(rejected_report.ack.status, CommandAckStatus::Rejected);
        assert_eq!(
            rejected_report.ack.reason.expect("reason").code,
            CommandAckReasonCode::BrokerRejected
        );
    }

    #[tokio::test]
    async fn endpoint_response_rate_limit_disarms_backoffs_redacts_and_blocks_retry() {
        let now = Utc
            .with_ymd_and_hms(2026, 7, 1, 11, 10, 0)
            .single()
            .expect("timestamp");
        let (mut store, approved, order) =
            place_store_and_approved(now, request_id(133), "CID000000000000133");
        let mut arm = dry_preflight_policy(now).operator_arm.clone();
        let rate_limited = FinamOrderEndpointFixture::RateLimited {
            retry_after_ms: Some(2_500),
        };

        let report = simulate_place_order_endpoint_fixture(
            &mut store,
            &approved,
            None,
            &rate_limited,
            Some(&mut arm),
            now + chrono::Duration::milliseconds(1),
            now + chrono::Duration::milliseconds(2),
        )
        .expect("rate limited endpoint fixture");

        assert_eq!(
            report.outcome,
            EndpointResponseIntegrationOutcomeKind::RateLimited
        );
        assert_eq!(report.state, OrderPathState::ManualInterventionRequired);
        assert_eq!(report.ack.status, CommandAckStatus::Error);
        assert_eq!(
            report.ack.reason.as_ref().expect("reason").code,
            CommandAckReasonCode::RateLimited
        );
        assert_eq!(report.retry_after_ms, Some(2_500));
        assert_eq!(
            report.disarm_decision.expect("disarm").signal,
            OperatorDisarmSignal::OrderEndpointRateLimited
        );
        assert_eq!(
            arm.validate(now)
                .expect_err("rate-limit disarms operator arm"),
            broker_core::OrderPreflightError::EndpointNotArmed
        );
        let loaded = store
            .load_by_request_id(order.request_id)
            .expect("manual intervention record");
        assert_eq!(loaded.state, OrderPathState::ManualInterventionRequired);
        assert_eq!(
            loaded.last_error_kind,
            Some(OrderPathErrorKind::RateLimited)
        );

        let accepted_late = FinamOrderEndpointFixture::Accepted(FinamOrderEndpointAcceptedDto {
            broker_order_id: Some("BROKER_TEST_ENDPOINT_RETRY".to_string()),
        });
        let retry_error = simulate_place_order_endpoint_fixture(
            &mut store,
            &approved,
            None,
            &accepted_late,
            None,
            now + chrono::Duration::milliseconds(3),
            now + chrono::Duration::milliseconds(4),
        )
        .expect_err("blind endpoint retry must be blocked");
        assert!(matches!(
            retry_error,
            EndpointResponseIntegrationSimulatorError::Transition(
                OrderPathTransitionError::InvalidTransition {
                    from: OrderPathState::ManualInterventionRequired,
                    event: OrderPathEvent::BeginSubmit
                }
            )
        ));

        let sink = InMemoryRedisStreamSink::default();
        let gateway = FinamGateway::new(GatewayConfig::default(), sink.clone());
        gateway
            .publish_dry_command_ack(report.ack)
            .await
            .expect("rate-limit ack published");
        let entries = sink.entries().expect("entries");
        assert!(entries[0].payload.contains("rate_limited"));
        assert!(!entries[0].payload.contains("CID000000000000133"));
        assert!(!entries[0].payload.contains("ACC_TEST_0001"));
        assert!(!entries[0].payload.contains("BROKER_TEST_ENDPOINT_RETRY"));
    }

    #[test]
    fn endpoint_response_integration_maps_maintenance_and_decode_error() {
        let now = Utc
            .with_ymd_and_hms(2026, 7, 1, 11, 20, 0)
            .single()
            .expect("timestamp");
        let cases = [
            (
                FinamOrderEndpointFixture::Maintenance {
                    maintenance_kind: FinamOrderEndpointMaintenanceKind::ServiceInterval,
                },
                EndpointResponseIntegrationOutcomeKind::Maintenance,
                CommandAckReasonCode::BrokerMaintenance,
                OrderPathErrorKind::BrokerMaintenance,
                OperatorDisarmSignal::OrderEndpointMaintenance,
                FinamOrderEndpointResponseKind::Maintenance,
                request_id(134),
                "CID000000000000134",
            ),
            (
                FinamOrderEndpointFixture::DecodeError {
                    status: Some(502),
                    body_kind: Some("object".to_string()),
                },
                EndpointResponseIntegrationOutcomeKind::DecodeError,
                CommandAckReasonCode::ResponseDecodeError,
                OrderPathErrorKind::ResponseDecodeError,
                OperatorDisarmSignal::OrderEndpointDecodeError,
                FinamOrderEndpointResponseKind::DecodeError,
                request_id(135),
                "CID000000000000135",
            ),
        ];

        for (
            fixture,
            expected_outcome,
            expected_reason,
            expected_error,
            expected_signal,
            expected_response_kind,
            request_id,
            client_order_id,
        ) in cases
        {
            let (mut store, approved, order) =
                place_store_and_approved(now, request_id, client_order_id);
            let mut arm = dry_preflight_policy(now).operator_arm.clone();

            let report = simulate_place_order_endpoint_fixture(
                &mut store,
                &approved,
                None,
                &fixture,
                Some(&mut arm),
                now + chrono::Duration::milliseconds(1),
                now + chrono::Duration::milliseconds(2),
            )
            .expect("non-execution endpoint fixture");

            assert_eq!(report.outcome, expected_outcome);
            assert_eq!(report.state, OrderPathState::ManualInterventionRequired);
            assert_eq!(report.ack.status, CommandAckStatus::Error);
            assert_eq!(
                report.ack.reason.as_ref().expect("reason").code,
                expected_reason
            );
            assert_eq!(report.endpoint_response.kind, expected_response_kind);
            assert_eq!(
                report.disarm_decision.expect("disarm").signal,
                expected_signal
            );
            assert_eq!(
                store
                    .load_by_request_id(order.request_id)
                    .expect("record")
                    .last_error_kind,
                Some(expected_error)
            );
        }
    }

    #[test]
    fn endpoint_response_integration_maps_cancel_rate_limit() {
        let now = Utc
            .with_ymd_and_hms(2026, 7, 1, 11, 30, 0)
            .single()
            .expect("timestamp");
        let (mut store, approved_cancel, place_request_id) = submitted_store_and_approved_cancel(
            now,
            request_id(136),
            request_id(137),
            BrokerOrderId::new("BROKER_TEST_ENDPOINT_CANCEL"),
        );
        let mut arm = dry_preflight_policy(now).operator_arm.clone();
        let fixture = FinamOrderEndpointFixture::RateLimited {
            retry_after_ms: Some(1_000),
        };

        let report = simulate_cancel_order_endpoint_fixture(
            &mut store,
            &approved_cancel,
            &fixture,
            Some(&mut arm),
            now + chrono::Duration::milliseconds(1),
            now + chrono::Duration::milliseconds(2),
        )
        .expect("cancel rate-limit endpoint fixture");

        assert_eq!(
            report.outcome,
            EndpointResponseIntegrationOutcomeKind::RateLimited
        );
        assert_eq!(report.state, OrderPathState::ManualInterventionRequired);
        assert_eq!(report.cancel_attempt_count, 1);
        assert_eq!(report.ack.status, CommandAckStatus::Error);
        assert_eq!(
            report.ack.reason.expect("reason").code,
            CommandAckReasonCode::RateLimited
        );
        assert_eq!(
            report.disarm_decision.expect("disarm").signal,
            OperatorDisarmSignal::OrderEndpointRateLimited
        );
        let loaded = store
            .load_by_request_id(place_request_id)
            .expect("cancel record");
        assert_eq!(loaded.state, OrderPathState::ManualInterventionRequired);
        assert_eq!(
            loaded.last_error_kind,
            Some(OrderPathErrorKind::RateLimited)
        );
    }

    #[tokio::test]
    async fn sqlite_backed_endpoint_rate_limit_persists_safe_audit_and_redacted_ack() {
        let now = Utc
            .with_ymd_and_hms(2026, 7, 1, 11, 40, 0)
            .single()
            .expect("timestamp");
        let path = temp_sqlite_path("endpoint_rate_limit");
        cleanup_sqlite_path(&path);
        let order = sample_place_order(request_id(138), "CID000000000000138", now);
        let policy = dry_preflight_policy(now);
        let approved = policy
            .approve_place_order(&order, now)
            .expect("preflight approved");
        let mut store = SqliteOrderPathStore::open(&path).expect("open sqlite store");
        store
            .insert_intent(OrderPathRecord::from_place_order(
                approved.order(),
                now,
                None,
            ))
            .expect("intent inserted");
        let mut arm = policy.operator_arm.clone();
        let fixture = FinamOrderEndpointFixture::RateLimited {
            retry_after_ms: Some(1_500),
        };

        let report = simulate_place_order_endpoint_fixture(
            &mut store,
            &approved,
            None,
            &fixture,
            Some(&mut arm),
            now + chrono::Duration::milliseconds(1),
            now + chrono::Duration::milliseconds(2),
        )
        .expect("sqlite endpoint rate-limit report");

        assert_eq!(
            report.outcome,
            EndpointResponseIntegrationOutcomeKind::RateLimited
        );
        let audit = store.transition_audit().expect("transition audit");
        assert_eq!(audit.len(), 3);
        assert_eq!(audit[0].event, "InsertIntent");
        assert_eq!(audit[1].event, "BeginSubmit");
        assert_eq!(audit[1].from_state.as_deref(), Some("IntentRecorded"));
        assert_eq!(audit[1].to_state, "SubmitInFlight");
        assert_eq!(audit[2].event, "RequireManualIntervention");
        assert_eq!(audit[2].from_state.as_deref(), Some("SubmitInFlight"));
        assert_eq!(audit[2].to_state, "ManualInterventionRequired");
        assert_eq!(audit[2].reason_code.as_deref(), Some("RateLimited"));
        assert_eq!(audit[2].safe_details, "sqlite_order_path_store");
        drop(store);

        {
            let readonly =
                SqliteOrderPathReadStore::open_readonly(&path).expect("open read-only diagnostics");
            assert_eq!(
                readonly
                    .operator_load_by_request_id(order.request_id)
                    .expect("read-only record")
                    .last_error_kind,
                Some(OrderPathErrorKind::RateLimited)
            );
        }

        let sink = InMemoryRedisStreamSink::default();
        let gateway = FinamGateway::new(GatewayConfig::default(), sink.clone());
        gateway
            .publish_dry_command_ack(report.ack)
            .await
            .expect("sqlite rate-limit ack published");
        let entries = sink.entries().expect("entries");
        assert!(entries[0].payload.contains("rate_limited"));
        assert!(!entries[0].payload.contains("CID000000000000138"));
        assert!(!entries[0].payload.contains("ACC_TEST_0001"));
        cleanup_sqlite_path(&path);
    }

    #[tokio::test]
    async fn local_http_success_response_redacts_ack_ids() {
        let now = Utc
            .with_ymd_and_hms(2026, 7, 1, 11, 50, 0)
            .single()
            .expect("timestamp");
        let (mut store, approved, _) =
            place_store_and_approved(now, request_id(139), "CID000000000000139");
        let response = FinamOrderEndpointLocalHttpResponse::Response {
            status: 200,
            body: serde_json::json!({"broker_order_id": "BROKER_TEST_HTTP_OK"}).to_string(),
            retry_after_ms: None,
        };

        let report = simulate_place_order_endpoint_local_http_response(
            &mut store,
            &approved,
            None,
            &response,
            None,
            now + chrono::Duration::milliseconds(1),
            now + chrono::Duration::milliseconds(2),
        )
        .expect("local http accepted response");

        assert_eq!(
            report.outcome,
            EndpointResponseIntegrationOutcomeKind::Submitted
        );
        assert_eq!(report.ack.status, CommandAckStatus::Submitted);
        assert_eq!(
            report.ack.broker_order_id,
            Some(BrokerOrderId::new("BROKER_TEST_HTTP_OK"))
        );

        let sink = InMemoryRedisStreamSink::default();
        let gateway = FinamGateway::new(GatewayConfig::default(), sink.clone());
        gateway
            .publish_dry_command_ack(report.ack)
            .await
            .expect("accepted ack published");
        let entries = sink.entries().expect("entries");
        assert!(!entries[0].payload.contains("BROKER_TEST_HTTP_OK"));
        assert!(!entries[0].payload.contains("CID000000000000139"));
        assert!(!entries[0].payload.contains("ACC_TEST_0001"));
    }

    #[tokio::test]
    async fn local_http_empty_broker_id_records_decode_error_after_begin_submit() {
        let now = Utc
            .with_ymd_and_hms(2026, 7, 1, 12, 0, 0)
            .single()
            .expect("timestamp");
        let path = temp_sqlite_path("local_http_empty_id");
        cleanup_sqlite_path(&path);
        let order = sample_place_order(request_id(140), "CID000000000000140", now);
        let policy = dry_preflight_policy(now);
        let approved = policy
            .approve_place_order(&order, now)
            .expect("preflight approved");
        let mut store = SqliteOrderPathStore::open(&path).expect("open sqlite store");
        store
            .insert_intent(OrderPathRecord::from_place_order(
                approved.order(),
                now,
                None,
            ))
            .expect("intent inserted");
        let mut arm = policy.operator_arm.clone();
        let response = FinamOrderEndpointLocalHttpResponse::Response {
            status: 200,
            body: serde_json::json!({"broker_order_id": ""}).to_string(),
            retry_after_ms: None,
        };

        let report = simulate_place_order_endpoint_local_http_response(
            &mut store,
            &approved,
            None,
            &response,
            Some(&mut arm),
            now + chrono::Duration::milliseconds(1),
            now + chrono::Duration::milliseconds(2),
        )
        .expect("empty broker id classified after begin submit");

        assert_eq!(
            report.outcome,
            EndpointResponseIntegrationOutcomeKind::DecodeError
        );
        assert_eq!(report.state, OrderPathState::ManualInterventionRequired);
        assert_eq!(report.ack.status, CommandAckStatus::Error);
        assert_eq!(
            report.ack.reason.as_ref().expect("reason").code,
            CommandAckReasonCode::ResponseDecodeError
        );
        assert_eq!(
            report.disarm_decision.expect("disarm").signal,
            OperatorDisarmSignal::OrderEndpointDecodeError
        );
        assert_eq!(
            arm.validate(now).expect_err("decode error disarms arm"),
            broker_core::OrderPreflightError::EndpointNotArmed
        );

        let audit = store.transition_audit().expect("transition audit");
        assert_eq!(audit.len(), 3);
        assert_eq!(audit[0].event, "InsertIntent");
        assert_eq!(audit[1].event, "BeginSubmit");
        assert_eq!(audit[1].to_state, "SubmitInFlight");
        assert_eq!(audit[2].event, "RequireManualIntervention");
        assert_eq!(audit[2].from_state.as_deref(), Some("SubmitInFlight"));
        assert_eq!(audit[2].to_state, "ManualInterventionRequired");
        assert_eq!(audit[2].reason_code.as_deref(), Some("ResponseDecodeError"));
        let stored = store
            .load_by_request_id(order.request_id)
            .expect("stored record");
        assert_eq!(
            stored.last_error_kind,
            Some(OrderPathErrorKind::ResponseDecodeError)
        );

        let sink = InMemoryRedisStreamSink::default();
        let gateway = FinamGateway::new(GatewayConfig::default(), sink.clone());
        gateway
            .publish_dry_command_ack(report.ack)
            .await
            .expect("decode ack published");
        let entries = sink.entries().expect("entries");
        assert!(entries[0].payload.contains("response_decode_error"));
        assert!(!entries[0].payload.contains("CID000000000000140"));
        assert!(!entries[0].payload.contains("ACC_TEST_0001"));
        cleanup_sqlite_path(&path);
    }

    #[test]
    fn local_http_cancel_malformed_json_records_after_request_cancel() {
        let now = Utc
            .with_ymd_and_hms(2026, 7, 1, 12, 10, 0)
            .single()
            .expect("timestamp");
        let (mut store, approved_cancel, place_request_id) = submitted_store_and_approved_cancel(
            now,
            request_id(141),
            request_id(142),
            BrokerOrderId::new("BROKER_TEST_HTTP_CANCEL"),
        );
        let mut arm = dry_preflight_policy(now).operator_arm.clone();
        let response = FinamOrderEndpointLocalHttpResponse::Response {
            status: 200,
            body: "{not-json".to_string(),
            retry_after_ms: None,
        };

        let report = simulate_cancel_order_endpoint_local_http_response(
            &mut store,
            &approved_cancel,
            &response,
            Some(&mut arm),
            now + chrono::Duration::milliseconds(1),
            now + chrono::Duration::milliseconds(2),
        )
        .expect("malformed cancel response classified after request cancel");

        assert_eq!(
            report.outcome,
            EndpointResponseIntegrationOutcomeKind::DecodeError
        );
        assert_eq!(report.state, OrderPathState::ManualInterventionRequired);
        assert_eq!(report.cancel_attempt_count, 1);
        assert_eq!(report.ack.status, CommandAckStatus::Error);
        assert_eq!(
            report.ack.reason.as_ref().expect("reason").code,
            CommandAckReasonCode::ResponseDecodeError
        );
        let stored = store
            .load_by_request_id(place_request_id)
            .expect("stored cancel record");
        assert_eq!(stored.cancel_attempt_count, 1);
        assert_eq!(
            stored.last_error_kind,
            Some(OrderPathErrorKind::ResponseDecodeError)
        );
        assert_eq!(
            report.disarm_decision.expect("disarm").signal,
            OperatorDisarmSignal::OrderEndpointDecodeError
        );
    }

    #[tokio::test]
    async fn local_http_unauthorized_statuses_disarm_and_redact_ack() {
        let now = Utc
            .with_ymd_and_hms(2026, 7, 1, 12, 20, 0)
            .single()
            .expect("timestamp");

        for (status, request_num, client_order_id) in [
            (401, 143, "CID000000000000143"),
            (403, 144, "CID000000000000144"),
        ] {
            let (mut store, approved, order) =
                place_store_and_approved(now, request_id(request_num), client_order_id);
            let mut arm = dry_preflight_policy(now).operator_arm.clone();
            let response = FinamOrderEndpointLocalHttpResponse::Response {
                status,
                body: serde_json::json!({"message": "auth failed and must not leak"}).to_string(),
                retry_after_ms: None,
            };

            let report = simulate_place_order_endpoint_local_http_response(
                &mut store,
                &approved,
                None,
                &response,
                Some(&mut arm),
                now + chrono::Duration::milliseconds(1),
                now + chrono::Duration::milliseconds(2),
            )
            .expect("unauthorized local http response");

            assert_eq!(
                report.outcome,
                EndpointResponseIntegrationOutcomeKind::Unauthorized
            );
            assert_eq!(report.state, OrderPathState::ManualInterventionRequired);
            assert_eq!(report.ack.status, CommandAckStatus::Error);
            assert_eq!(
                report.ack.reason.as_ref().expect("reason").code,
                CommandAckReasonCode::Unauthorized
            );
            assert_eq!(
                report.endpoint_response.kind,
                FinamOrderEndpointResponseKind::Unauthorized
            );
            assert_eq!(report.endpoint_response.status, Some(status));
            assert_eq!(
                report.disarm_decision.expect("disarm").signal,
                OperatorDisarmSignal::OrderEndpointUnauthorized
            );
            assert_eq!(
                store
                    .load_by_request_id(order.request_id)
                    .expect("stored record")
                    .last_error_kind,
                Some(OrderPathErrorKind::Unauthorized)
            );

            let sink = InMemoryRedisStreamSink::default();
            let gateway = FinamGateway::new(GatewayConfig::default(), sink.clone());
            gateway
                .publish_dry_command_ack(report.ack)
                .await
                .expect("unauthorized ack published");
            let entries = sink.entries().expect("entries");
            assert!(entries[0].payload.contains("unauthorized"));
            assert!(!entries[0].payload.contains(client_order_id));
            assert!(!entries[0].payload.contains("ACC_TEST_0001"));
            assert!(!entries[0].payload.contains("auth failed"));
        }
    }

    #[tokio::test]
    async fn cancel_simulator_accepts_approved_cancel_and_redacts_ack() {
        let now = Utc
            .with_ymd_and_hms(2026, 6, 30, 10, 0, 0)
            .single()
            .expect("timestamp");
        let broker_order_id = BrokerOrderId::new("BROKER_TEST_10");
        let (mut store, approved_cancel, place_request_id) = submitted_store_and_approved_cancel(
            now,
            request_id(10),
            request_id(11),
            broker_order_id,
        );
        let mut client = MockFinamApprovedOrderExecutionClient::new(vec![
            FinamOrderExecutionOutcome::Accepted {
                broker_order_id: None,
            },
        ]);

        let report = simulate_cancel_order_approved(
            &mut store,
            &mut client,
            &approved_cancel,
            now + chrono::Duration::milliseconds(1),
            now + chrono::Duration::milliseconds(2),
        )
        .await
        .expect("cancel simulation");

        assert_eq!(
            report.outcome,
            DryOrderExecutionOutcomeKind::CancelSubmitted
        );
        assert_eq!(report.state, OrderPathState::CancelSubmitted);
        assert_eq!(report.ack.status, CommandAckStatus::Submitted);
        assert_eq!(report.cancel_attempt_count, 1);
        assert_eq!(
            store
                .load_by_request_id(place_request_id)
                .expect("record")
                .state,
            OrderPathState::CancelSubmitted
        );
        let records_json = serde_json::to_string(client.records()).expect("records");
        assert!(!records_json.contains("BROKER_TEST_10"));
        assert!(!records_json.contains("ACC_TEST_0001"));

        let sink = InMemoryRedisStreamSink::default();
        let gateway = FinamGateway::new(GatewayConfig::default(), sink.clone());
        gateway
            .publish_dry_command_ack(report.ack)
            .await
            .expect("cancel ack published");
        let entries = sink.entries().expect("entries");
        assert!(!entries[0].payload.contains("BROKER_TEST_10"));
        assert!(!entries[0].payload.contains("CID000000000000010"));
    }

    #[tokio::test]
    async fn cancel_simulator_requires_matching_returned_broker_order_id() {
        let now = Utc
            .with_ymd_and_hms(2026, 6, 30, 10, 5, 0)
            .single()
            .expect("timestamp");
        let (mut store, approved_cancel, place_request_id) = submitted_store_and_approved_cancel(
            now,
            request_id(18),
            request_id(19),
            BrokerOrderId::new("BROKER_TEST_18"),
        );
        let mut client = MockFinamApprovedOrderExecutionClient::new(vec![
            FinamOrderExecutionOutcome::Accepted {
                broker_order_id: Some(BrokerOrderId::new("BROKER_TEST_OTHER_18")),
            },
        ]);

        let report = simulate_cancel_order_approved(
            &mut store,
            &mut client,
            &approved_cancel,
            now + chrono::Duration::milliseconds(1),
            now + chrono::Duration::milliseconds(2),
        )
        .await
        .expect("cancel mismatch simulation");

        assert_eq!(
            report.outcome,
            DryOrderExecutionOutcomeKind::CancelAcceptedBrokerOrderIdMismatch
        );
        assert_eq!(report.state, OrderPathState::ManualInterventionRequired);
        assert_eq!(report.ack.status, CommandAckStatus::UnknownPending);
        assert_eq!(
            report.ack.reason.expect("reason").code,
            CommandAckReasonCode::ManualInterventionRequired
        );
        assert_eq!(
            store
                .load_by_request_id(place_request_id)
                .expect("record")
                .state,
            OrderPathState::ManualInterventionRequired
        );
        let records_json = serde_json::to_string(client.records()).expect("records");
        assert!(!records_json.contains("BROKER_TEST_OTHER_18"));
        assert!(!records_json.contains("BROKER_TEST_18"));
    }

    #[tokio::test]
    async fn sqlite_backed_cancel_simulator_persists_request_cancel_before_mock_call_and_manual_mismatch(
    ) {
        let now = Utc
            .with_ymd_and_hms(2026, 7, 1, 10, 5, 0)
            .single()
            .expect("timestamp");
        let path = temp_sqlite_path("cancel_request_before_call");
        cleanup_sqlite_path(&path);
        let broker_order_id = BrokerOrderId::new("BROKER_TEST_SQLITE_CANCEL");
        let order = sample_place_order(request_id(118), "CID000000000000118", now);
        let existing = submitted_record(&order, now, broker_order_id.clone());
        let cancel = sample_cancel_order(request_id(119), broker_order_id.as_str(), now);
        let policy = dry_preflight_policy(now);
        let approved_cancel = match policy
            .approve_cancel_order(&cancel, now, Some(&existing))
            .expect("cancel approved")
        {
            CancelPreflightApproval::Submit(approved) => approved,
            CancelPreflightApproval::AlreadyTerminal => panic!("expected submit cancel"),
        };
        let mut store = SqliteOrderPathStore::open(&path).expect("open sqlite store");
        store
            .insert_intent(existing)
            .expect("submitted record inserted");
        let mut client = InspectingExecutionClient::new(
            path.clone(),
            order.request_id,
            OrderPathState::CancelRequested,
            FinamOrderExecutionOutcome::Accepted {
                broker_order_id: Some(BrokerOrderId::new("BROKER_TEST_SQLITE_OTHER")),
            },
        );

        let report = simulate_cancel_order_approved(
            &mut store,
            &mut client,
            &approved_cancel,
            now + chrono::Duration::milliseconds(1),
            now + chrono::Duration::milliseconds(2),
        )
        .await
        .expect("sqlite-backed cancel mismatch report");

        assert_eq!(client.call_count, 1);
        assert_eq!(
            report.outcome,
            DryOrderExecutionOutcomeKind::CancelAcceptedBrokerOrderIdMismatch
        );
        assert_eq!(report.state, OrderPathState::ManualInterventionRequired);
        assert_eq!(report.ack.status, CommandAckStatus::UnknownPending);
        assert_eq!(
            report.ack.reason.as_ref().expect("reason").code,
            CommandAckReasonCode::ManualInterventionRequired
        );
        let loaded = store
            .load_by_request_id(order.request_id)
            .expect("manual intervention record");
        assert_eq!(loaded.state, OrderPathState::ManualInterventionRequired);
        assert_eq!(loaded.broker_order_id, Some(broker_order_id));
        let audit = store.transition_audit().expect("transition audit");
        assert_eq!(audit.len(), 3);
        assert_eq!(audit[0].event, "InsertIntent");
        assert_eq!(audit[0].from_state, None);
        assert_eq!(audit[0].to_state, "Submitted");
        assert_eq!(audit[1].event, "RequestCancel");
        assert_eq!(audit[1].from_state.as_deref(), Some("Submitted"));
        assert_eq!(audit[1].to_state, "CancelRequested");
        assert_eq!(audit[2].event, "RequireManualIntervention");
        assert_eq!(audit[2].from_state.as_deref(), Some("CancelRequested"));
        assert_eq!(audit[2].to_state, "ManualInterventionRequired");
        drop(store);

        {
            let readonly =
                SqliteOrderPathReadStore::open_readonly(&path).expect("open read-only diagnostics");
            assert_eq!(
                readonly
                    .operator_load_by_request_id(order.request_id)
                    .expect("read-only record")
                    .state,
                OrderPathState::ManualInterventionRequired
            );
            assert_eq!(readonly.transition_audit().expect("read audit").len(), 3);
        }

        let sink = InMemoryRedisStreamSink::default();
        let gateway = FinamGateway::new(GatewayConfig::default(), sink.clone());
        gateway
            .publish_dry_command_ack(report.ack)
            .await
            .expect("cancel ack published");
        let entries = sink.entries().expect("entries");
        assert!(!entries[0].payload.contains("BROKER_TEST_SQLITE_CANCEL"));
        assert!(!entries[0].payload.contains("BROKER_TEST_SQLITE_OTHER"));
        assert!(!entries[0].payload.contains("CID000000000000118"));
        cleanup_sqlite_path(&path);
    }

    #[tokio::test]
    async fn cancel_simulator_rejection_requires_manual_intervention() {
        let now = Utc
            .with_ymd_and_hms(2026, 6, 30, 10, 10, 0)
            .single()
            .expect("timestamp");
        let (mut store, approved_cancel, place_request_id) = submitted_store_and_approved_cancel(
            now,
            request_id(12),
            request_id(13),
            BrokerOrderId::new("BROKER_TEST_12"),
        );
        let mut client = MockFinamApprovedOrderExecutionClient::new(vec![
            FinamOrderExecutionOutcome::Rejected {
                reason_code: CommandAckReasonCode::BrokerRejected,
            },
        ]);

        let report = simulate_cancel_order_approved(
            &mut store,
            &mut client,
            &approved_cancel,
            now + chrono::Duration::milliseconds(1),
            now + chrono::Duration::milliseconds(2),
        )
        .await
        .expect("cancel rejected simulation");

        assert_eq!(report.outcome, DryOrderExecutionOutcomeKind::CancelRejected);
        assert_eq!(report.state, OrderPathState::ManualInterventionRequired);
        assert_eq!(report.ack.status, CommandAckStatus::Rejected);
        assert_eq!(
            report.ack.reason.expect("reason").code,
            CommandAckReasonCode::BrokerRejected
        );
        assert_eq!(
            store
                .load_by_request_id(place_request_id)
                .expect("record")
                .state,
            OrderPathState::ManualInterventionRequired
        );
    }

    #[tokio::test]
    async fn cancel_simulator_timeout_blocks_blind_retry_and_can_recover_terminal() {
        let now = Utc
            .with_ymd_and_hms(2026, 6, 30, 10, 20, 0)
            .single()
            .expect("timestamp");
        let (mut store, approved_cancel, place_request_id) = submitted_store_and_approved_cancel(
            now,
            request_id(14),
            request_id(15),
            BrokerOrderId::new("BROKER_TEST_14"),
        );
        let mut client = MockFinamApprovedOrderExecutionClient::new(vec![
            FinamOrderExecutionOutcome::Timeout,
            FinamOrderExecutionOutcome::Accepted {
                broker_order_id: None,
            },
        ]);

        let report = simulate_cancel_order_approved(
            &mut store,
            &mut client,
            &approved_cancel,
            now + chrono::Duration::milliseconds(1),
            now + chrono::Duration::milliseconds(2),
        )
        .await
        .expect("cancel timeout simulation");

        assert_eq!(
            report.outcome,
            DryOrderExecutionOutcomeKind::CancelTimeoutUnknownPending
        );
        assert_eq!(report.state, OrderPathState::CancelTimeoutUnknownPending);
        assert_eq!(report.ack.status, CommandAckStatus::Timeout);

        let retry_error = simulate_cancel_order_approved(
            &mut store,
            &mut client,
            &approved_cancel,
            now + chrono::Duration::milliseconds(3),
            now + chrono::Duration::milliseconds(4),
        )
        .await
        .expect_err("blind cancel retry must be blocked before mock client call");
        assert!(matches!(
            retry_error,
            DryOrderExecutionSimulatorError::Transition(
                OrderPathTransitionError::InvalidTransition {
                    from: OrderPathState::CancelTimeoutUnknownPending,
                    event: OrderPathEvent::RequestCancel
                }
            )
        ));
        assert_eq!(client.records().len(), 1);

        let mut recovered = store
            .load_by_request_id(place_request_id)
            .expect("timeout record");
        recovered
            .transition(
                OrderPathEvent::RecoverCancelTerminal,
                now + chrono::Duration::milliseconds(5),
            )
            .expect("recover terminal by broker truth");
        store
            .update_record(recovered.clone())
            .expect("persist recovery");
        assert_eq!(recovered.state, OrderPathState::CancelRecoveredTerminal);
    }

    #[test]
    fn cancel_preflight_already_terminal_does_not_call_mock_client() {
        let now = Utc
            .with_ymd_and_hms(2026, 6, 30, 10, 30, 0)
            .single()
            .expect("timestamp");
        let order = sample_place_order(request_id(16), "CID000000000000016", now);
        let mut existing = submitted_record(&order, now, BrokerOrderId::new("BROKER_TEST_16"));
        existing
            .transition(
                OrderPathEvent::MarkTerminal,
                now + chrono::Duration::milliseconds(1),
            )
            .expect("mark terminal");
        let cancel = sample_cancel_order(request_id(17), "BROKER_TEST_16", now);
        let policy = dry_preflight_policy(now);
        let client = MockFinamApprovedOrderExecutionClient::new(vec![
            FinamOrderExecutionOutcome::Accepted {
                broker_order_id: None,
            },
        ]);

        assert_eq!(
            policy
                .approve_cancel_order(&cancel, now, Some(&existing))
                .expect("terminal cancel decision"),
            CancelPreflightApproval::AlreadyTerminal
        );
        assert!(client.records().is_empty());
    }

    struct InspectingExecutionClient {
        db_path: PathBuf,
        expected_request_id: StrategyRequestId,
        expected_state_before_call: OrderPathState,
        outcome: FinamOrderExecutionOutcome,
        call_count: usize,
    }

    impl InspectingExecutionClient {
        fn new(
            db_path: PathBuf,
            expected_request_id: StrategyRequestId,
            expected_state_before_call: OrderPathState,
            outcome: FinamOrderExecutionOutcome,
        ) -> Self {
            Self {
                db_path,
                expected_request_id,
                expected_state_before_call,
                outcome,
                call_count: 0,
            }
        }

        fn assert_expected_state_is_durable(&self) {
            let readonly =
                SqliteOrderPathReadStore::open_readonly(&self.db_path).expect("read-only sqlite");
            let record = readonly
                .operator_load_by_request_id(self.expected_request_id)
                .expect("expected order-path record");
            assert_eq!(record.state, self.expected_state_before_call);
        }
    }

    #[async_trait::async_trait]
    impl broker_finam::FinamApprovedOrderExecutionClient for InspectingExecutionClient {
        async fn place_approved(
            &mut self,
            spec: broker_finam::FinamPlaceOrderRequestSpec,
        ) -> Result<FinamOrderExecutionOutcome, broker_finam::FinamOrderExecutionError> {
            assert!(!spec.account_id.is_empty());
            self.assert_expected_state_is_durable();
            self.call_count += 1;
            Ok(self.outcome.clone())
        }

        async fn cancel_approved(
            &mut self,
            spec: broker_finam::FinamCancelOrderRequestSpec,
        ) -> Result<FinamOrderExecutionOutcome, broker_finam::FinamOrderExecutionError> {
            assert!(!spec.account_id.is_empty());
            assert!(!spec.order_id.is_empty());
            self.assert_expected_state_is_durable();
            self.call_count += 1;
            Ok(self.outcome.clone())
        }
    }

    fn temp_sqlite_path(name: &str) -> PathBuf {
        let unique = Utc::now()
            .timestamp_nanos_opt()
            .unwrap_or_default()
            .unsigned_abs();
        std::env::temp_dir().join(format!("moex_trading_gateway_{name}_{unique}.sqlite"))
    }

    fn cleanup_sqlite_path(path: &Path) {
        let _ = std::fs::remove_file(path);
        let _ = std::fs::remove_file(sqlite_writer_lock_path(path));
        let _ = std::fs::remove_file(PathBuf::from(format!("{}-wal", path.to_string_lossy())));
        let _ = std::fs::remove_file(PathBuf::from(format!("{}-shm", path.to_string_lossy())));
    }

    fn sqlite_writer_lock_path(path: &Path) -> PathBuf {
        let mut lock_path = path.to_path_buf();
        lock_path.set_extension("writer.lock");
        lock_path
    }

    fn request_id(n: u128) -> StrategyRequestId {
        StrategyRequestId::from(Uuid::from_u128(n))
    }

    fn sample_place_order(
        request_id: StrategyRequestId,
        client_order_id: &str,
        now: DateTime<Utc>,
    ) -> PlaceOrder {
        PlaceOrder {
            request_id,
            created_ts: now,
            ttl_ms: Some(1_000),
            account_id: broker_core::BrokerAccountId::new("ACC_TEST_0001"),
            client_order_id: ClientOrderId::new(client_order_id).expect("client order id"),
            instrument: sample_instrument(),
            side: OrderSide::Buy,
            order_type: OrderType::Limit,
            qty: Decimal::ONE,
            limit_price: Some(Decimal::new(5000, 0)),
            time_in_force: TimeInForce::Day,
            comment: None,
        }
    }

    fn sample_cancel_order(
        request_id: StrategyRequestId,
        broker_order_id: &str,
        now: DateTime<Utc>,
    ) -> CancelOrder {
        CancelOrder {
            request_id,
            created_ts: now,
            ttl_ms: Some(1_000),
            account_id: broker_core::BrokerAccountId::new("ACC_TEST_0001"),
            order_id: BrokerOrderId::new(broker_order_id),
            client_order_id: None,
        }
    }

    fn submitted_record(
        order: &PlaceOrder,
        now: DateTime<Utc>,
        broker_order_id: BrokerOrderId,
    ) -> OrderPathRecord {
        let mut record = OrderPathRecord::from_place_order(order, now, None);
        record.broker_order_id = Some(broker_order_id);
        record
            .transition(OrderPathEvent::BeginSubmit, now)
            .expect("begin submit");
        record
            .transition(
                OrderPathEvent::SubmitAccepted,
                now + chrono::Duration::milliseconds(1),
            )
            .expect("submitted");
        record
    }

    fn submitted_store_and_approved_cancel(
        now: DateTime<Utc>,
        place_request_id: StrategyRequestId,
        cancel_request_id: StrategyRequestId,
        broker_order_id: BrokerOrderId,
    ) -> (
        InMemoryOrderPathStore,
        broker_core::PreflightApprovedCancelOrder,
        StrategyRequestId,
    ) {
        let order = sample_place_order(place_request_id, "CID000000000000010", now);
        let existing = submitted_record(&order, now, broker_order_id.clone());
        let cancel = sample_cancel_order(cancel_request_id, broker_order_id.as_str(), now);
        let policy = dry_preflight_policy(now);
        let approved_cancel = match policy
            .approve_cancel_order(&cancel, now, Some(&existing))
            .expect("cancel approved")
        {
            CancelPreflightApproval::Submit(approved) => approved,
            CancelPreflightApproval::AlreadyTerminal => panic!("expected submit cancel"),
        };
        let mut store = InMemoryOrderPathStore::default();
        store.insert_intent(existing).expect("insert submitted");
        (store, approved_cancel, place_request_id)
    }

    fn place_store_and_approved(
        now: DateTime<Utc>,
        request_id: StrategyRequestId,
        client_order_id: &str,
    ) -> (
        InMemoryOrderPathStore,
        broker_core::PreflightApprovedPlaceOrder,
        PlaceOrder,
    ) {
        let order = sample_place_order(request_id, client_order_id, now);
        let policy = dry_preflight_policy(now);
        let approved = policy
            .approve_place_order(&order, now)
            .expect("preflight approved");
        let mut store = InMemoryOrderPathStore::default();
        store
            .insert_intent(OrderPathRecord::from_place_order(
                approved.order(),
                now,
                None,
            ))
            .expect("intent inserted");
        (store, approved, order)
    }

    fn dry_preflight_policy(now: DateTime<Utc>) -> OrderPreflightPolicy {
        OrderPreflightPolicy {
            allowed_accounts: vec![broker_core::BrokerAccountId::new("ACC_TEST_0001")],
            allowed_venue_symbols: vec!["TESTFUT@TEST".to_string()],
            allowed_order_types: vec![OrderType::Market, OrderType::Limit],
            allowed_time_in_force: vec![TimeInForce::Day],
            min_qty: Decimal::ONE,
            qty_step: Decimal::ONE,
            max_qty: Decimal::new(3, 0),
            price_step: Some(Decimal::new(1, 0)),
            max_market_qty: Decimal::ONE,
            max_notional_per_order: None,
            max_notional_per_run: None,
            max_limit_deviation_bps: None,
            max_reference_age_ms: 1_000,
            allow_cancel_by_broker_order_id_without_mapping: false,
            operator_arm: OperatorArm {
                session_id: "ARM_TEST_1".to_string(),
                armed_until: now + chrono::Duration::minutes(5),
                endpoint_calls_enabled: true,
                one_shot: false,
                endpoint_attempted: false,
                preflight_digest: "digest-test".to_string(),
            },
        }
    }

    fn sample_instrument() -> InstrumentId {
        InstrumentId {
            symbol: "TESTFUT".to_string(),
            venue_symbol: Some("TESTFUT@TEST".to_string()),
            exchange: Exchange::Other("TEST".to_string()),
            market: Market::Futures,
        }
    }

    fn sample_account() -> broker_finam::dto::AccountResponse {
        serde_json::from_value(serde_json::json!({
            "account_id": "ACC_TEST_0001",
            "cash": [{"currency_code": "RUB", "units": "1000", "nanos": 0}],
            "positions": [{
                "symbol": "TESTFUT@TEST",
                "asset_type": "FUTURES",
                "quantity": {"value": "1"}
            }],
            "status": "ACCOUNT_ACTIVE",
            "type": "UNION"
        }))
        .expect("account dto")
    }

    fn sample_orders() -> broker_finam::dto::AccountOrdersResponse {
        serde_json::from_value(serde_json::json!({
            "orders": [{
                "executed_quantity": {"value": "0"},
                "initial_quantity": {"value": "1"},
                "order": {
                    "account_id": "ACC_TEST_0001",
                    "comment": "raw broker note must not leak",
                    "legs": [],
                    "quantity": {"value": "1"},
                    "side": "SIDE_BUY",
                    "symbol": "TESTFUT@TEST",
                    "type": "ORDER_TYPE_LIMIT"
                },
                "status": "ORDER_STATUS_ACTIVE"
            }]
        }))
        .expect("orders dto")
    }

    fn sample_order_with_comment(comment: Option<&str>) -> Order {
        Order {
            account_id: broker_core::BrokerAccountId::new("ACC_TEST_0001"),
            order_id: None,
            client_order_id: Some(ClientOrderId::new("ABC123").expect("client order id")),
            broker_client_order_id_fingerprint: None,
            instrument: sample_instrument(),
            side: OrderSide::Buy,
            order_type: OrderType::Limit,
            status: OrderStatus::Working,
            qty: Decimal::ONE,
            filled_qty: Decimal::ZERO,
            limit_price: Some(Decimal::new(5000, 0)),
            stop_price: None,
            comment_fingerprint: None,
            comment: comment.map(str::to_string),
            source_ts: None,
            received_ts: Utc::now(),
        }
    }

    fn sample_bar(source_kind: MarketDataSourceKind, is_final: bool) -> Bar {
        let open_ts = Utc
            .with_ymd_and_hms(2026, 6, 29, 9, 10, 0)
            .single()
            .expect("timestamp");
        Bar {
            instrument: sample_instrument(),
            source_kind,
            timeframe_sec: 60,
            open_ts,
            close_ts: open_ts + chrono::Duration::seconds(60),
            open: Decimal::new(5000, 0),
            high: Decimal::new(5010, 0),
            low: Decimal::new(4990, 0),
            close: Decimal::new(5005, 0),
            volume: Decimal::new(10, 0),
            is_final,
        }
    }

    fn runtime_entries(entries: Vec<RedisStreamEntry>) -> Vec<RuntimeBridgeStreamEntry> {
        entries
            .into_iter()
            .enumerate()
            .map(|(index, entry)| RuntimeBridgeStreamEntry {
                stream: entry.stream,
                entry_id: format!("{}-0", index + 1),
                payload: entry.payload,
            })
            .collect()
    }

    fn decode_stream_payload<T>(
        entries: &[RedisStreamEntry],
        stream: &str,
        msg_type: MessageType,
    ) -> Envelope<T>
    where
        T: DeserializeOwned,
    {
        let entry = entries
            .iter()
            .find(|entry| entry.stream == stream)
            .unwrap_or_else(|| panic!("missing stream entry: {stream}"));
        let envelope: Envelope<T> =
            serde_json::from_str(&entry.payload).expect("typed envelope decodes");
        assert_eq!(envelope.schema_version, 2);
        assert_eq!(envelope.msg_type, msg_type);
        envelope
    }
}
