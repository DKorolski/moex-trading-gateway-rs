//! Closed market-data identities from the accepted 13-Nov catalog. No string
//! routing, order authority, automatic expiry rollover or network discovery.
use broker_core::{Exchange, InstrumentId, Market};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShadowInstrument {
    Imoexf,
    Usdrubf,
    Riz6,
}
impl ShadowInstrument {
    pub fn symbol(self) -> &'static str {
        match self {
            Self::Imoexf => "IMOEXF",
            Self::Usdrubf => "USDRUBF",
            Self::Riz6 => "RIZ6",
        }
    }
    pub fn venue_symbol(self) -> &'static str {
        match self {
            Self::Imoexf => "IMOEXF@RTSX",
            Self::Usdrubf => "USDRUBF@RTSX",
            Self::Riz6 => "RIZ6@RTSX",
        }
    }
    pub fn asset_id(self) -> &'static str {
        match self {
            Self::Imoexf => "4631091",
            Self::Usdrubf => "3447194",
            Self::Riz6 => "5436517",
        }
    }
    pub fn tick(self) -> Decimal {
        match self {
            Self::Imoexf => Decimal::new(5, 1),
            Self::Usdrubf => Decimal::new(1, 2),
            Self::Riz6 => Decimal::new(10, 0),
        }
    }
    pub fn instrument(self) -> InstrumentId {
        InstrumentId {
            symbol: self.symbol().into(),
            venue_symbol: Some(self.venue_symbol().into()),
            exchange: Exchange::Moex,
            market: Market::Futures,
        }
    }
    pub fn from_venue(value: &str) -> Option<Self> {
        [Self::Imoexf, Self::Usdrubf, Self::Riz6]
            .into_iter()
            .find(|s| s.venue_symbol() == value)
    }
}
