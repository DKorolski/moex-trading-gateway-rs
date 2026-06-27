use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrokerReadiness {
    pub phase: ReadinessPhase,
    pub reasons: Vec<ReadinessReason>,
    pub checked_ts: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReadinessPhase {
    Starting,
    Authenticating,
    SyncingHistory,
    LiveReady,
    Degraded,
    Blocked,
    Stopped,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReadinessReason {
    MissingToken,
    AuthExpired,
    AccountUnavailable,
    PositionsNotLoaded,
    OrdersNotLoaded,
    TradesNotLoaded,
    MarketDataNotLive,
    BrokerMaintenance,
    RateLimited,
    TransportDisconnected,
    OperatorPaused,
    Other(String),
}
