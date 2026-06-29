//! FINAM gateway skeleton for M2a read-only/shadow mode.
//!
//! This crate intentionally does not contain order placement, cancel, ACK
//! lifecycle, stop/SLTP, bracket, or runtime adaptation. It only prepares the
//! Redis/shadow publication boundary for health, readiness, broker-truth
//! snapshots, and read-only market data.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use broker_core::account::PortfolioSnapshot;
use broker_core::command::{BrokerCommand, CommandAck, CommandAckStatus};
use broker_core::envelope::{Envelope, MessageType};
use broker_core::event::MarketDataEvent;
use broker_core::ids::StrategyRequestId;
use broker_core::order::Order;
use broker_core::readiness::{BrokerReadiness, ReadinessPhase, ReadinessReason};
use chrono::{DateTime, Utc};
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

#[async_trait]
pub trait RedisStreamSink: Send + Sync {
    async fn publish_json<T: Serialize + Send + Sync>(
        &self,
        stream: &str,
        value: &T,
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
    ) -> Result<(), GatewayError> {
        let payload = serde_json::to_string(value)?;
        self.entries
            .lock()
            .map_err(|_| GatewayError::InternalState {
                message: "in-memory redis sink mutex poisoned",
            })?
            .push(RedisStreamEntry {
                stream: stream.to_string(),
                payload,
            });
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
    ) -> Result<(), GatewayError> {
        let payload = serde_json::to_string(value)?;
        let mut manager = self.manager.clone();
        let _message_id: String = redis::cmd("XADD")
            .arg(stream)
            .arg("*")
            .arg("payload")
            .arg(payload)
            .query_async(&mut manager)
            .await?;
        Ok(())
    }
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
        )
        .await
    }

    pub async fn publish_order_snapshot(
        &self,
        snapshot: OrderSnapshot,
    ) -> Result<(), GatewayError> {
        if !self.config.features.publish_snapshots {
            return Err(GatewayError::FeatureDisabled {
                feature: "publish_snapshots",
            });
        }
        self.publish_envelope(
            &self.config.redis.order_snapshot_stream,
            MessageType::OrderSnapshot,
            snapshot,
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
    ) -> Result<(), GatewayError> {
        let envelope = Envelope::new(self.config.source.clone(), msg_type, payload);
        self.sink.publish_json(stream, &envelope).await
    }
}

pub fn build_order_snapshot(orders: Vec<Order>, received_ts: DateTime<Utc>) -> OrderSnapshot {
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
    use broker_core::event::{MarketDataEvent, Quote};
    use broker_core::ids::{ClientOrderId, StrategyRequestId};
    use broker_core::instrument::{Exchange, InstrumentId, Market};
    use broker_core::order::{OrderSide, OrderType, TimeInForce};
    use chrono::TimeZone;
    use rust_decimal::Decimal;
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
    async fn publishes_market_data_event_from_readonly_path() {
        let sink = InMemoryRedisStreamSink::default();
        let gateway = FinamGateway::new(GatewayConfig::default(), sink.clone());

        gateway
            .publish_market_data_event(MarketDataEvent::Quote(Quote {
                instrument: sample_instrument(),
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
            symbol: "IMOEXF".to_string(),
            venue_symbol: Some("IMOEXF@RTSX".to_string()),
            exchange: Exchange::Moex,
            market: Market::Futures,
        }
    }

    fn sample_account() -> broker_finam::dto::AccountResponse {
        serde_json::from_value(serde_json::json!({
            "account_id": "ACC_TEST_0001",
            "cash": [{"currency_code": "RUB", "units": "1000", "nanos": 0}],
            "positions": [{
                "symbol": "IMOEXF@RTSX",
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
                    "legs": [],
                    "quantity": {"value": "1"},
                    "side": "SIDE_BUY",
                    "symbol": "IMOEXF@RTSX",
                    "type": "ORDER_TYPE_LIMIT"
                },
                "status": "ORDER_STATUS_ACTIVE"
            }]
        }))
        .expect("orders dto")
    }
}
