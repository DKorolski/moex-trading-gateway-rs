//! Broker-neutral strategy semantic kernels migrated from the accepted ALOR
//! runtime source.
//!
//! This crate contains no FINAM transport, Redis client, command consumer, or
//! real order endpoint.
//!
//! The source-compatible ALOR host seam is deliberately private. Downstream
//! hosts must use [`BrokerNeutralHybridStrategy`].
//!
//! ```compile_fail
//! use strategy_runtime_core::StrategyCtx;
//! ```
//!
//! ```compile_fail
//! use strategy_runtime_core::strategy_host::Strategy;
//! ```
//!
//! ```compile_fail
//! use strategy_runtime_core::state::StrategyState;
//! ```

pub mod hybrid_intraday;
// The accepted source wrapper intentionally retains Stage 5C/5D callbacks
// which are sealed from downstream code until their dedicated gates open.
#[allow(dead_code)]
mod hybrid_intraday_runtime;
// Source-compatible DTOs and traits remain complete for oracle correspondence,
// while only approved broker-neutral aliases are exported below.
#[allow(dead_code)]
mod runtime_compat;

pub use hybrid_intraday_runtime::{
    BrokerNeutralHybridCallbackResult, BrokerNeutralHybridStrategy, HybridIntradayProfile,
    HybridIntradayRuntimeConfig, HybridIntradayRuntimeStrategy,
    HybridRuntimeCallbackValidationError, MeanReversionVariant, MrGatePolicy, RiskGateMode,
};
#[allow(unused_imports)]
pub(crate) use runtime_compat::{
    BootstrapSnapshot, PaperExecutionMode, RuntimeStateRestored, StrategyCtx, TradeMode,
};
pub use runtime_compat::{
    Intent as BrokerNeutralHybridIntent, IntentClass as BrokerNeutralHybridIntentClass,
    MarketBuyAndCloseLiveOrderStyle as BrokerNeutralMarketOrderStyle,
    OrderSide as BrokerNeutralOrderSide, StopLimitCondition as BrokerNeutralStopLimitCondition,
};

pub(crate) mod live_guard {
    pub use crate::runtime_compat::GatewayPhase;
}

pub(crate) mod state {
    pub use crate::runtime_compat::StrategyState;
}

pub(crate) mod strategy_host {
    #[allow(unused_imports)]
    pub use crate::runtime_compat::OrderEvent;
    #[allow(unused_imports)]
    pub use crate::runtime_compat::{
        BarEvent, BootstrapSnapshot, CommandAck, DataOrigin, Intent, PositionEvent,
        RiskGateRuntimeState, RiskGateSessionFinalization, RuntimeStateRestored, StopOrderEvent,
        Strategy, StrategyCtx,
    };
}

pub(crate) mod strategies {
    pub mod hybrid_intraday {
        pub use crate::hybrid_intraday::*;
    }

    pub mod market_buy_and_close {
        pub use crate::runtime_compat::MarketBuyAndCloseLiveOrderStyle;
    }
}

pub(crate) fn deterministic_request_id(
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

pub(crate) fn deterministic_market_request_id(
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
