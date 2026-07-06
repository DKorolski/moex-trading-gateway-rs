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
    AuthPending,
    AuthReady,
    ReferenceDataLoading,
    StreamsConnecting,
    StreamsSubscribed,
    SnapshotsLoading,
    Reconciliation,
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
    ReferenceDataNotLoaded,
    InstrumentMapNotValidated,
    ScheduleNotLoaded,
    PositionsNotLoaded,
    OrdersNotLoaded,
    TradesNotLoaded,
    FirstLiveBarMissing,
    MarketDataNotLive,
    MarketDataSessionUnknown,
    RedisUnavailable,
    ClockSkew,
    ReconciliationStale,
    UnknownOpenOrders,
    OperatorLiveArmMissing,
    BrokerMaintenance,
    RateLimited,
    TransportDisconnected,
    OperatorPaused,
    Other(String),
}
