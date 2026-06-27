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
    pub reason: Option<String>,
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
