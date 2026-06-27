use serde::{Deserialize, Serialize};

use crate::account::AccountId;
use crate::ids::{BrokerOrderId, ClientOrderId, StrategyRequestId};
use crate::instrument::{InstrumentId, Price, Quantity};
use crate::order::{OrderSide, OrderType, StopKind, TimeInForce};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BrokerCommand {
    PlaceOrder(PlaceOrder),
    PlaceSltpOrder(PlaceSltpOrder),
    CancelOrder(CancelOrder),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlaceOrder {
    pub request_id: StrategyRequestId,
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
pub struct PlaceSltpOrder {
    pub request_id: StrategyRequestId,
    pub account_id: AccountId,
    pub client_order_id: ClientOrderId,
    pub instrument: InstrumentId,
    pub side: OrderSide,
    pub stop_kind: StopKind,
    pub qty: Quantity,
    pub stop_price: Price,
    pub limit_price: Option<Price>,
    pub valid_before: TimeInForce,
    pub comment: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CancelOrder {
    pub request_id: StrategyRequestId,
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
    Rejected,
    Timeout,
    Unknown,
}
