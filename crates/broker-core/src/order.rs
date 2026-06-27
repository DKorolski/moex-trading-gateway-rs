use serde::{Deserialize, Serialize};

use crate::account::AccountId;
use crate::ids::{BrokerOrderId, BrokerTradeId, ClientOrderId};
use crate::instrument::{InstrumentId, Money, Price, Quantity};

pub type OrderId = BrokerOrderId;
pub type TradeId = BrokerTradeId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OrderSide {
    Buy,
    Sell,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OrderType {
    Market,
    Limit,
    Stop,
    StopLimit,
    TakeProfit,
    TakeProfitLimit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TimeInForce {
    Day,
    GoodTillCancel,
    GoodTillDate,
    FillOrKill,
    ImmediateOrCancel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OrderStatus {
    New,
    Working,
    PartiallyFilled,
    Filled,
    Canceled,
    Rejected,
    Expired,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StopKind {
    StopLoss,
    TakeProfit,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Order {
    pub account_id: AccountId,
    pub order_id: Option<OrderId>,
    pub client_order_id: Option<ClientOrderId>,
    pub instrument: InstrumentId,
    pub side: OrderSide,
    pub order_type: OrderType,
    pub status: OrderStatus,
    pub qty: Quantity,
    pub filled_qty: Quantity,
    pub limit_price: Option<Price>,
    pub stop_price: Option<Price>,
    pub comment: Option<String>,
    pub source_ts: Option<chrono::DateTime<chrono::Utc>>,
    pub received_ts: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Trade {
    pub account_id: AccountId,
    pub trade_id: TradeId,
    pub order_id: Option<OrderId>,
    pub client_order_id: Option<ClientOrderId>,
    pub instrument: InstrumentId,
    pub side: OrderSide,
    pub qty: Quantity,
    pub price: Price,
    pub gross_amount: Option<Money>,
    pub commission: Option<Money>,
    pub source_ts: chrono::DateTime<chrono::Utc>,
    pub received_ts: chrono::DateTime<chrono::Utc>,
}
