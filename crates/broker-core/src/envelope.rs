use serde::{Deserialize, Serialize};

pub const SCHEMA_VERSION: u16 = 2;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Envelope<T> {
    pub schema_version: u16,
    pub ts_utc: chrono::DateTime<chrono::Utc>,
    pub source: String,
    pub msg_type: MessageType,
    pub payload: T,
}

impl<T> Envelope<T> {
    pub fn new(source: impl Into<String>, msg_type: MessageType, payload: T) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            ts_utc: chrono::Utc::now(),
            source: source.into(),
            msg_type,
            payload,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MessageType {
    Command,
    CommandAck,
    MarketData,
    Order,
    Trade,
    Position,
    PortfolioSnapshot,
    Readiness,
    Subscription,
    Unknown(String),
}
