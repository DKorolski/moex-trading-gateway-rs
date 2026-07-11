//! Broker-neutral strategy semantic kernels migrated from the accepted ALOR
//! runtime source.
//!
//! This crate contains no FINAM transport, Redis client, command consumer, or
//! real order endpoint.

pub mod hybrid_intraday;
pub mod hybrid_intraday_runtime;
pub mod runtime_compat;

pub use runtime_compat::{
    BootstrapSnapshot, PaperExecutionMode, RuntimeStateRestored, StrategyCtx, TradeMode,
};

pub mod live_guard {
    pub use crate::runtime_compat::GatewayPhase;
}

pub mod state {
    pub use crate::runtime_compat::StrategyState;
}

pub mod strategy_host {
    pub use crate::runtime_compat::OrderEvent;
    pub use crate::runtime_compat::{
        BarEvent, BootstrapSnapshot, CommandAck, DataOrigin, Intent, PositionEvent,
        RiskGateRuntimeState, RiskGateSessionFinalization, RuntimeStateRestored, StopOrderEvent,
        Strategy, StrategyCtx,
    };
}

pub mod strategies {
    pub mod hybrid_intraday {
        pub use crate::hybrid_intraday::*;
    }

    pub mod market_buy_and_close {
        pub use crate::runtime_compat::MarketBuyAndCloseLiveOrderStyle;
    }
}

pub fn deterministic_request_id(
    strategy_id: &str,
    portfolio: &str,
    symbol: &str,
    action: &str,
    bar_ts: i64,
    seq: u8,
) -> broker_core::StrategyRequestId {
    broker_core::deterministic_request_id_from_legacy_parts(
        strategy_id,
        portfolio,
        symbol,
        action,
        bar_ts,
        seq,
    )
}

pub fn deterministic_market_request_id(
    strategy_id: &str,
    portfolio: &str,
    symbol: &str,
    created_ts_utc: i64,
    side: runtime_compat::OrderSide,
) -> broker_core::StrategyRequestId {
    let seq = match side {
        runtime_compat::OrderSide::Buy => 3,
        runtime_compat::OrderSide::Sell => 4,
    };
    deterministic_request_id(
        strategy_id,
        portfolio,
        symbol,
        "market",
        created_ts_utc,
        seq,
    )
}
