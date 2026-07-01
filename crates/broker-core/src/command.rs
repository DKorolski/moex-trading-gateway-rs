use serde::{Deserialize, Serialize};

use crate::account::AccountId;
use crate::ids::{BrokerOrderId, ClientOrderId, StrategyRequestId};
use crate::instrument::{InstrumentId, Price, Quantity};
use crate::order::{OrderSide, OrderType, TimeInForce};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BrokerCommand {
    PlaceOrder(PlaceOrder),
    CancelOrder(CancelOrder),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlaceOrder {
    pub request_id: StrategyRequestId,
    pub created_ts: chrono::DateTime<chrono::Utc>,
    pub ttl_ms: Option<u64>,
    pub account_id: AccountId,
    pub client_order_id: ClientOrderId,
    pub instrument: InstrumentId,
    pub side: OrderSide,
    pub order_type: OrderType,
    pub qty: Quantity,
    pub limit_price: Option<Price>,
    pub time_in_force: TimeInForce,
    pub comment: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CancelOrder {
    pub request_id: StrategyRequestId,
    pub created_ts: chrono::DateTime<chrono::Utc>,
    pub ttl_ms: Option<u64>,
    pub account_id: AccountId,
    pub order_id: BrokerOrderId,
    pub client_order_id: Option<ClientOrderId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CommandAck {
    pub request_id: StrategyRequestId,
    pub client_order_id: Option<ClientOrderId>,
    pub broker_order_id: Option<BrokerOrderId>,
    pub status: CommandAckStatus,
    pub reason: Option<CommandAckReason>,
    pub received_ts: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CommandAckStatus {
    Accepted,
    Submitted,
    Duplicate,
    Expired,
    Recovered,
    Rejected,
    Error,
    Timeout,
    UnknownPending,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandAckReason {
    pub code: CommandAckReasonCode,
}

impl CommandAckReason {
    pub fn new(code: CommandAckReasonCode) -> Self {
        Self { code }
    }

    pub fn feature_disabled() -> Self {
        Self::new(CommandAckReasonCode::FeatureDisabled)
    }

    pub fn synthetic_submitted() -> Self {
        Self::new(CommandAckReasonCode::SyntheticSubmitted)
    }

    pub fn cancel_timeout_unknown_pending() -> Self {
        Self::new(CommandAckReasonCode::CancelTimeoutUnknownPending)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandAckReasonCode {
    FeatureDisabled,
    SyntheticSubmitted,
    LocalValidationRejected,
    BrokerRejected,
    TransportTimeout,
    TimeoutUnknownPending,
    CancelTimeoutUnknownPending,
    RecoveredByBrokerTruth,
    ReconciliationRequired,
    DuplicateCommand,
    ExpiredCommand,
    ManualInterventionRequired,
    DryRunOnly,
    RateLimited,
    BrokerMaintenance,
    ResponseDecodeError,
}
