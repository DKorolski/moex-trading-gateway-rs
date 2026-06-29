use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::broker::BrokerKind;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct InternalSymbol(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BrokerSymbol(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct InstrumentId {
    /// Broker-neutral symbol, for example `TESTFUT` or `INTERNAL_TEST_FUT`.
    pub symbol: String,
    /// Broker/exchange native symbol when it differs, for example Finam `ticker@mic`.
    pub venue_symbol: Option<String>,
    pub exchange: Exchange,
    pub market: Market,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Exchange {
    Moex,
    Other(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Market {
    Futures,
    Options,
    Stocks,
    Currency,
    Funds,
    Other(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Instrument {
    pub id: InstrumentId,
    pub lot_size: Quantity,
    pub price_step: Price,
    pub step_value: Money,
    pub currency: String,
    pub is_tradable: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InstrumentMapEntry {
    pub internal_symbol: InternalSymbol,
    pub broker: BrokerKind,
    pub broker_symbol: BrokerSymbol,
    pub exchange: Exchange,
    pub market: Market,
    pub price_step: Price,
    pub qty_step: Quantity,
    pub lot_size: Quantity,
    pub min_qty: Quantity,
    pub step_value: Money,
    pub currency: String,
    pub schedule_id: String,
    pub expiration_date: Option<chrono::NaiveDate>,
    pub is_tradable: bool,
}

pub type Price = Decimal;
pub type Quantity = Decimal;
pub type Money = Decimal;
