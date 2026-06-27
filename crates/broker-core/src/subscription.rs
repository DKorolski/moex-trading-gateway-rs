use serde::{Deserialize, Serialize};

use crate::account::AccountId;
use crate::instrument::InstrumentId;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubscriptionIntent {
    pub kind: SubscriptionKind,
    pub key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SubscriptionKind {
    Bars {
        instrument: InstrumentId,
        timeframe_sec: u32,
    },
    Quotes {
        instrument: InstrumentId,
    },
    OrderBook {
        instrument: InstrumentId,
    },
    LatestTrades {
        instrument: InstrumentId,
    },
    OwnOrders {
        account_id: AccountId,
    },
    OwnTrades {
        account_id: AccountId,
    },
    Positions {
        account_id: AccountId,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubscriptionState {
    pub intent: SubscriptionIntent,
    pub phase: SubscriptionPhase,
    pub last_event_ts: Option<chrono::DateTime<chrono::Utc>>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SubscriptionPhase {
    Requested,
    Connecting,
    SyncingHistory,
    Live,
    Degraded,
    Stopped,
}
