//! FINAM gateway primitives for M2 read-only/shadow mode.
//!
//! This crate intentionally does not contain order placement, cancel, ACK
//! lifecycle, stop/SLTP, bracket, or runtime adaptation. It prepares the
//! Redis/shadow publication boundary for health, readiness, broker-truth
//! snapshots, read-only market data, retention, and degraded/stopped status
//! reporting, plus a dry runtime-bridge consumer contract for typed decode and
//! idempotency validation.

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use broker_core::account::PortfolioSnapshot;
use broker_core::command::{BrokerCommand, CommandAck, CommandAckStatus};
use broker_core::envelope::{Envelope, MessageType, SCHEMA_VERSION};
use broker_core::event::MarketDataEvent;
use broker_core::ids::StrategyRequestId;
use broker_core::order::Order;
use broker_core::readiness::{BrokerReadiness, ReadinessPhase, ReadinessReason};
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
            runtime_bridge_dlq_maxlen: Some(1_000),
        }
    }
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
    #[error("finam mapper error: {0}")]
    Mapper(#[from] broker_finam::FinamMapperError),
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
            reason: Some("FeatureDisabled: command consumer/order placement disabled in M2a read-only gateway".to_string()),
            received_ts: Utc::now(),
        }
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

#[cfg(test)]
mod tests {
    use broker_core::account::PortfolioSnapshot;
    use broker_core::event::{Bar, MarketDataEvent, MarketDataSourceKind, Quote};
    use broker_core::ids::{ClientOrderId, StrategyRequestId};
    use broker_core::instrument::{Exchange, InstrumentId, Market};
    use broker_core::order::{Order, OrderSide, OrderStatus, OrderType, TimeInForce};
    use chrono::TimeZone;
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
            Uuid::parse_str("019eda4d-48c1-7491-9f3d-3243ebcd52c5").expect("uuid"),
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
        assert!(ack
            .reason
            .as_deref()
            .expect("reason")
            .contains("FeatureDisabled"));
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
