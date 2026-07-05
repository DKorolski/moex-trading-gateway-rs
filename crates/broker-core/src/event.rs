use serde::{Deserialize, Serialize};

use crate::account::{PortfolioSnapshot, Position};
use crate::instrument::{InstrumentId, Price, Quantity};
use crate::order::{Order, Trade};
use crate::readiness::BrokerReadiness;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BrokerEvent {
    Readiness(BrokerReadiness),
    PortfolioSnapshot(PortfolioSnapshot),
    Position(Position),
    Order(Order),
    Trade(Trade),
    MarketData(MarketDataEvent),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum MarketDataEvent {
    Bar(Bar),
    Quote(Quote),
    OrderBook(OrderBook),
    LatestTrade(LatestMarketTrade),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum MarketDataSourceKind {
    #[default]
    Unknown,
    HistoricalPoll,
    ReadOnlyPoll,
    LiveStream,
    Recovery,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Bar {
    pub instrument: InstrumentId,
    #[serde(default)]
    pub source_kind: MarketDataSourceKind,
    pub timeframe_sec: u32,
    pub open_ts: chrono::DateTime<chrono::Utc>,
    pub close_ts: chrono::DateTime<chrono::Utc>,
    pub open: Price,
    pub high: Price,
    pub low: Price,
    pub close: Price,
    pub volume: Quantity,
    pub is_final: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Quote {
    pub instrument: InstrumentId,
    #[serde(default)]
    pub source_kind: MarketDataSourceKind,
    pub bid: Option<Price>,
    pub ask: Option<Price>,
    pub last: Option<Price>,
    pub source_ts: Option<chrono::DateTime<chrono::Utc>>,
    pub received_ts: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrderBook {
    pub instrument: InstrumentId,
    #[serde(default)]
    pub source_kind: MarketDataSourceKind,
    pub bids: Vec<OrderBookLevel>,
    pub asks: Vec<OrderBookLevel>,
    pub source_ts: Option<chrono::DateTime<chrono::Utc>>,
    pub received_ts: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrderBookLevel {
    pub price: Price,
    pub qty: Quantity,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LatestMarketTrade {
    pub instrument: InstrumentId,
    #[serde(default)]
    pub source_kind: MarketDataSourceKind,
    pub price: Price,
    pub qty: Quantity,
    pub source_ts: chrono::DateTime<chrono::Utc>,
    pub received_ts: chrono::DateTime<chrono::Utc>,
}
