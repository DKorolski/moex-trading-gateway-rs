use serde::{Deserialize, Serialize};

use crate::instrument::{InstrumentId, Money, Price, Quantity};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AccountId(pub String);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Position {
    pub account_id: AccountId,
    pub instrument: InstrumentId,
    pub qty: Quantity,
    pub avg_price: Option<Price>,
    pub unrealized_pnl: Option<Money>,
    pub source_ts: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PortfolioSnapshot {
    pub account_id: AccountId,
    pub positions: Vec<Position>,
    pub cash: Vec<CashPosition>,
    pub source_ts: Option<chrono::DateTime<chrono::Utc>>,
    pub received_ts: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CashPosition {
    pub currency: String,
    pub amount: Money,
}
