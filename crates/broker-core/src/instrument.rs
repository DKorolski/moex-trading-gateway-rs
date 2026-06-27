use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct InstrumentId {
    /// Broker-neutral symbol, for example `IMOEXF`, `RTS-9.26`, or `USDRUBF`.
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

pub type Price = Decimal;
pub type Quantity = Decimal;
pub type Money = Decimal;
