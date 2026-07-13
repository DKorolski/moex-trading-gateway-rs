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
//!
//! ```compile_fail
//! use strategy_runtime_core::{Stage5cSettledPaperStrategy, Stage5cTimerSettlement};
//!
//! let settled: Stage5cSettledPaperStrategy = unreachable!();
//! let _forged = Stage5cTimerSettlement::ReadyForContinuation {
//!     settled,
//!     checkpoint_ts_utc_ms: 0,
//! };
//! ```
//!
//! ```compile_fail
//! use strategy_runtime_core::{Stage5cSettledPaperStrategy, Stage5cTimerSettlement};
//!
//! let settled: Stage5cSettledPaperStrategy = unreachable!();
//! let _forged = Stage5cTimerSettlement::GeneratedIntentBatch(settled);
//! ```
//!
//! ```compile_fail
//! use strategy_runtime_core::{
//!     advance_stage5c_controlled_next_bar, Stage5cAcceptedSemanticBar, Stage5cTimerSettlement,
//! };
//!
//! let settlement: Stage5cTimerSettlement = unreachable!();
//! let accepted: Stage5cAcceptedSemanticBar = unreachable!();
//! let settled = settlement.into_settled();
//! let _ = advance_stage5c_controlled_next_bar(settled, accepted);
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
mod stage5c_paper_host;

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
pub use stage5c_paper_host::{
    accept_stage5c_history_batch, accept_stage5c_pending_recovery_evidence,
    accept_stage5c_semantic_bar, admit_stage5c_paper_host, advance_stage5c_controlled_next_bar,
    advance_stage5c_paper_loop_once, advance_stage5c_timer_settlement_next_bar,
    advance_stage5c_timer_settlement_timer, apply_stage5c_semantic_bar, notify_stage5c_bootstrap,
    notify_stage5c_runtime_state_restored, prepare_stage5c_without_runtime_state,
    prove_stage5c_pending_recovery_claim, recover_stage5c_pending_streams,
    resolve_stage5c_paper_broker_lifecycle, resolve_stage5c_paper_intent_lifecycle,
    resolve_stage5c_paper_timer, restore_stage5c_runtime_state,
    settle_stage5c_broker_lifecycle_result, settle_stage5c_semantic_result,
    settle_stage5c_timer_result, warmup_stage5c_history, Stage5cAcceptedHistoryBatch,
    Stage5cAcceptedPendingRecoveryEvidence, Stage5cAcceptedSemanticBar,
    Stage5cBootstrapNotificationError, Stage5cBootstrapNotificationReceipt,
    Stage5cBootstrappedPaperStrategy, Stage5cBrokerLifecycleResolvedPaperStrategy,
    Stage5cBrokerLifecycleSettlement, Stage5cHistoryBatchInput, Stage5cHistoryWarmupError,
    Stage5cHistoryWarmupReceipt, Stage5cIntentSettlementError, Stage5cLegacyNumericOrderIdPolicy,
    Stage5cNextBarBlocked, Stage5cNextBarLoopError, Stage5cNextBarLoopFailure,
    Stage5cPaperAckOutcome, Stage5cPaperAckRecord, Stage5cPaperBrokerEventKind,
    Stage5cPaperBrokerEventPayload, Stage5cPaperBrokerEventRecord,
    Stage5cPaperBrokerLifecycleBlocked, Stage5cPaperBrokerLifecycleError,
    Stage5cPaperBrokerLifecycleExpectation, Stage5cPaperBrokerLifecycleFailure,
    Stage5cPaperBrokerLifecycleInput, Stage5cPaperHostAdmission, Stage5cPaperHostAdmissionError,
    Stage5cPaperHostAdmissionInput, Stage5cPaperIntentBatch, Stage5cPaperIntentBatchSummary,
    Stage5cPaperIntentLifecycleBlocked, Stage5cPaperIntentLifecycleError,
    Stage5cPaperIntentLifecycleFailure, Stage5cPaperIntentLifecycleInput, Stage5cPaperLoopError,
    Stage5cPaperLoopEvent, Stage5cPaperLoopEventKind, Stage5cPaperLoopFailure,
    Stage5cPaperLoopState, Stage5cPaperLoopStateKind, Stage5cPaperTimerBlocked,
    Stage5cPaperTimerError, Stage5cPaperTimerFailure, Stage5cPaperTimerInput,
    Stage5cPendingRecoveredPaperStrategy, Stage5cPendingRecoveryClaimProof,
    Stage5cPendingRecoveryClaimProofInput, Stage5cPendingRecoveryError,
    Stage5cPendingRecoveryEvent, Stage5cPendingRecoveryEvidenceInput,
    Stage5cPendingRecoveryPayload, Stage5cPendingRecoveryReceipt,
    Stage5cPendingStreamClaimBoundary, Stage5cPendingStreamKind,
    Stage5cResolvedPaperIntentBatchStrategy, Stage5cRuntimeStateLoadedPaperStrategy,
    Stage5cRuntimeStateRestoreError, Stage5cRuntimeStateRestoreInput,
    Stage5cRuntimeStateRestoreReceipt, Stage5cRuntimeStateRestoredPaperStrategy,
    Stage5cSemanticBarError, Stage5cSemanticBarInput, Stage5cSemanticBarResult,
    Stage5cSettledPaperStrategy, Stage5cTimerContinuationBlocked, Stage5cTimerContinuationError,
    Stage5cTimerContinuationFailure, Stage5cTimerResolvedPaperStrategy, Stage5cTimerSettlement,
    Stage5cWarmedPaperStrategy, STAGE5C_PAPER_HOST_ADMISSION_SCHEMA_VERSION,
    STAGE5C_RUNTIME_STATE_RESTORE_SCHEMA_VERSION,
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
