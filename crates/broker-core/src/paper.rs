use std::collections::HashSet;
use std::fmt;
use std::str::FromStr;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::bar_aggregation::{
    BarAggregationAction, BarAggregationRejectReason, CanonicalBarAggregator,
};
use crate::envelope::SCHEMA_VERSION;
use crate::event::{Bar, MarketDataSourceKind};
use crate::ids::BrokerAccountId;
use crate::instrument::{InstrumentId, Price, Quantity};
use crate::order::{OrderSide, OrderType, TimeInForce};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RuntimeDecisionId(pub String);

impl RuntimeDecisionId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for RuntimeDecisionId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PaperOrderId(pub String);

impl PaperOrderId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for PaperOrderId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PaperTradeId(pub String);

impl PaperTradeId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for PaperTradeId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PaperExecutionMode {
    LiveOnly,
    HistorySim,
}

impl PaperExecutionMode {
    pub fn can_advance(self, origin: RuntimeBarOrigin) -> bool {
        match self {
            Self::LiveOnly => origin == RuntimeBarOrigin::Live,
            Self::HistorySim => true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RuntimeBarOrigin {
    History,
    HistoryGap,
    Live,
    Replay,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PaperFillPolicy {
    NextFinalBarOpen,
    LimitBarTouch,
    CancelIfWorking,
    ManualReviewRequired(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PaperIntentKind {
    Enter,
    Exit,
    Cancel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PaperOrderStatus {
    Pending,
    Working,
    Filled,
    PartiallyFilled,
    Canceled,
    Rejected,
    Expired,
    ManualReview,
}

impl PaperOrderStatus {
    pub fn is_active(self) -> bool {
        matches!(self, Self::Pending | Self::Working | Self::PartiallyFilled)
    }

    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Filled | Self::Canceled | Self::Rejected | Self::Expired | Self::ManualReview
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PaperAckKind {
    Accepted,
    Filled,
    Canceled,
    Rejected,
    DuplicateIgnored,
    Suppressed,
    ManualReviewRequired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaperSafetyBoundary {
    pub live_orders_enabled: bool,
    pub runtime_live_ready_enabled: bool,
    pub command_consumer_to_real_finam_enabled: bool,
    pub external_order_endpoint_enabled: bool,
    pub stop_sltp_bracket_enabled: bool,
}

impl PaperSafetyBoundary {
    pub fn closed() -> Self {
        Self {
            live_orders_enabled: false,
            runtime_live_ready_enabled: false,
            command_consumer_to_real_finam_enabled: false,
            external_order_endpoint_enabled: false,
            stop_sltp_bracket_enabled: false,
        }
    }

    pub fn is_closed(self) -> bool {
        !self.live_orders_enabled
            && !self.runtime_live_ready_enabled
            && !self.command_consumer_to_real_finam_enabled
            && !self.external_order_endpoint_enabled
            && !self.stop_sltp_bracket_enabled
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuntimeBarInput {
    pub schema_version: u16,
    pub instrument: InstrumentId,
    pub origin: RuntimeBarOrigin,
    pub timeframe_sec: u32,
    pub open_ts: DateTime<Utc>,
    pub close_ts: DateTime<Utc>,
    pub open: Price,
    pub high: Price,
    pub low: Price,
    pub close: Price,
    pub volume: Quantity,
    pub is_final: bool,
    pub source_stream: String,
    pub provenance: String,
}

impl RuntimeBarInput {
    pub fn is_live_final_timeframe(&self, expected_timeframe_sec: u32) -> bool {
        self.origin == RuntimeBarOrigin::Live
            && self.is_final
            && self.timeframe_sec == expected_timeframe_sec
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaperRuntimeBarPublisherConfig {
    pub strategy_id: String,
    pub instrument: InstrumentId,
    pub source_stream: String,
    pub target_stream: String,
    pub source_timeframe_sec: u32,
    pub target_timeframe_sec: u32,
    pub provenance: String,
    pub safety_boundary: PaperSafetyBoundary,
}

impl PaperRuntimeBarPublisherConfig {
    pub fn finam_m1_to_m10_paper(
        strategy_id: impl Into<String>,
        instrument: InstrumentId,
        source_stream: impl Into<String>,
        target_stream: impl Into<String>,
    ) -> Self {
        Self {
            strategy_id: strategy_id.into(),
            instrument,
            source_stream: source_stream.into(),
            target_stream: target_stream.into(),
            source_timeframe_sec: 60,
            target_timeframe_sec: 600,
            provenance: "FinamDerivedM1ToM10".to_string(),
            safety_boundary: PaperSafetyBoundary::closed(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct PaperRuntimeBarPublisher {
    config: PaperRuntimeBarPublisherConfig,
    aggregator: CanonicalBarAggregator,
}

impl PaperRuntimeBarPublisher {
    pub fn new(
        config: PaperRuntimeBarPublisherConfig,
    ) -> Result<Self, PaperRuntimeBarPublishRejectReason> {
        validate_paper_runtime_bar_publisher_config(&config)?;
        Ok(Self {
            aggregator: CanonicalBarAggregator::new(config.target_timeframe_sec),
            config,
        })
    }

    pub fn config(&self) -> &PaperRuntimeBarPublisherConfig {
        &self.config
    }

    pub fn observe_source_bar(&mut self, bar: Bar) -> PaperRuntimeBarPublishOutcome {
        if !self.config.safety_boundary.is_closed() {
            return PaperRuntimeBarPublishOutcome::Rejected {
                reason: PaperRuntimeBarPublishRejectReason::LiveBoundaryOpen,
            };
        }
        if bar.instrument != self.config.instrument {
            return PaperRuntimeBarPublishOutcome::Rejected {
                reason: PaperRuntimeBarPublishRejectReason::InstrumentMismatch,
            };
        }
        if bar.source_kind != MarketDataSourceKind::LiveStream {
            return PaperRuntimeBarPublishOutcome::Rejected {
                reason: PaperRuntimeBarPublishRejectReason::NonLiveSourceKind {
                    actual: bar.source_kind,
                },
            };
        }
        if bar.timeframe_sec != self.config.source_timeframe_sec {
            return PaperRuntimeBarPublishOutcome::Rejected {
                reason: PaperRuntimeBarPublishRejectReason::SourceTimeframeMismatch {
                    expected_sec: self.config.source_timeframe_sec,
                    actual_sec: bar.timeframe_sec,
                },
            };
        }

        match self.aggregator.observe_final_source_bar(bar) {
            BarAggregationAction::Buffered {
                bucket_open_ts,
                buffered_count,
            } => PaperRuntimeBarPublishOutcome::Buffered {
                bucket_open_ts,
                buffered_count,
            },
            BarAggregationAction::DroppedIncompleteBucket {
                bucket_open_ts,
                buffered_count,
            } => PaperRuntimeBarPublishOutcome::DroppedIncompleteBucket {
                bucket_open_ts,
                buffered_count,
            },
            BarAggregationAction::Rejected { reason } => PaperRuntimeBarPublishOutcome::Rejected {
                reason: PaperRuntimeBarPublishRejectReason::AggregationRejected(reason),
            },
            BarAggregationAction::Emitted { emitted } => {
                let runtime_input = RuntimeBarInput {
                    schema_version: SCHEMA_VERSION,
                    instrument: emitted.instrument,
                    origin: RuntimeBarOrigin::Live,
                    timeframe_sec: emitted.timeframe_sec,
                    open_ts: emitted.open_ts,
                    close_ts: emitted.close_ts,
                    open: emitted.open,
                    high: emitted.high,
                    low: emitted.low,
                    close: emitted.close,
                    volume: emitted.volume,
                    is_final: emitted.is_final,
                    source_stream: self.config.target_stream.clone(),
                    provenance: self.config.provenance.clone(),
                };
                PaperRuntimeBarPublishOutcome::Published {
                    target_stream: self.config.target_stream.clone(),
                    runtime_input: Box::new(runtime_input),
                }
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PaperRuntimeBarPublishOutcome {
    Buffered {
        bucket_open_ts: DateTime<Utc>,
        buffered_count: usize,
    },
    Published {
        target_stream: String,
        runtime_input: Box<RuntimeBarInput>,
    },
    DroppedIncompleteBucket {
        bucket_open_ts: DateTime<Utc>,
        buffered_count: usize,
    },
    Rejected {
        reason: PaperRuntimeBarPublishRejectReason,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
pub enum PaperRuntimeBarPublishRejectReason {
    #[error("paper runtime bar publisher safety boundary is open")]
    LiveBoundaryOpen,
    #[error("paper runtime bar publisher instrument mismatch")]
    InstrumentMismatch,
    #[error("paper runtime bar publisher source timeframe mismatch: expected={expected_sec} actual={actual_sec}")]
    SourceTimeframeMismatch { expected_sec: u32, actual_sec: u32 },
    #[error(
        "paper runtime bar publisher target timeframe mismatch: expected 600 actual={actual_sec}"
    )]
    TargetTimeframeMismatch { actual_sec: u32 },
    #[error("paper runtime bar publisher invalid timeframe")]
    InvalidTimeframe,
    #[error("paper runtime bar publisher non-live source kind: {actual:?}")]
    NonLiveSourceKind { actual: MarketDataSourceKind },
    #[error("paper runtime bar publisher aggregation rejected: {0:?}")]
    AggregationRejected(BarAggregationRejectReason),
}

fn validate_paper_runtime_bar_publisher_config(
    config: &PaperRuntimeBarPublisherConfig,
) -> Result<(), PaperRuntimeBarPublishRejectReason> {
    if !config.safety_boundary.is_closed() {
        return Err(PaperRuntimeBarPublishRejectReason::LiveBoundaryOpen);
    }
    if config.source_timeframe_sec == 0 || config.target_timeframe_sec == 0 {
        return Err(PaperRuntimeBarPublishRejectReason::InvalidTimeframe);
    }
    if config.source_timeframe_sec >= config.target_timeframe_sec {
        return Err(PaperRuntimeBarPublishRejectReason::InvalidTimeframe);
    }
    if config.target_timeframe_sec != 600 {
        return Err(
            PaperRuntimeBarPublishRejectReason::TargetTimeframeMismatch {
                actual_sec: config.target_timeframe_sec,
            },
        );
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuntimeDecisionRecord {
    pub schema_version: u16,
    pub decision_id: RuntimeDecisionId,
    pub strategy_id: String,
    pub decision_bar_key: String,
    pub instrument: InstrumentId,
    pub side: OrderSide,
    pub order_type: OrderType,
    pub qty: Quantity,
    pub limit_price: Option<Price>,
    pub time_in_force: Option<TimeInForce>,
    pub fill_policy: PaperFillPolicy,
    pub created_ts: DateTime<Utc>,
    pub source_bar_close_ts: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RuntimeSuppressionReason {
    NonLiveOrigin,
    NonFinalBar,
    TimeframeMismatch { expected_sec: u32, actual_sec: u32 },
    DuplicateDecision,
    RuntimeNotReady,
    ManualPolicy(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuntimeSuppressionRecord {
    pub schema_version: u16,
    pub decision_id: Option<RuntimeDecisionId>,
    pub strategy_id: String,
    pub instrument: InstrumentId,
    pub decision_bar_key: String,
    pub reason: RuntimeSuppressionReason,
    pub created_ts: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PaperIntent {
    pub schema_version: u16,
    pub intent_id: RuntimeDecisionId,
    pub kind: PaperIntentKind,
    pub strategy_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strategy_owner: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strategy_cycle_id: Option<String>,
    pub decision_bar_key: String,
    pub instrument: InstrumentId,
    pub side: Option<OrderSide>,
    pub order_type: Option<OrderType>,
    pub qty: Quantity,
    pub limit_price: Option<Price>,
    pub fill_policy: PaperFillPolicy,
    pub created_ts: DateTime<Utc>,
}

impl PaperIntent {
    pub fn from_decision(kind: PaperIntentKind, decision: RuntimeDecisionRecord) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            intent_id: decision.decision_id,
            kind,
            strategy_id: decision.strategy_id,
            strategy_owner: None,
            strategy_cycle_id: None,
            decision_bar_key: decision.decision_bar_key,
            instrument: decision.instrument,
            side: Some(decision.side),
            order_type: Some(decision.order_type),
            qty: decision.qty,
            limit_price: decision.limit_price,
            fill_policy: decision.fill_policy,
            created_ts: decision.created_ts,
        }
    }

    pub fn with_strategy_tags(
        mut self,
        owner: impl Into<String>,
        cycle_id: impl Into<String>,
    ) -> Self {
        self.strategy_owner = Some(owner.into());
        self.strategy_cycle_id = Some(cycle_id.into());
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PaperOrder {
    pub schema_version: u16,
    pub paper_order_id: PaperOrderId,
    pub intent_id: RuntimeDecisionId,
    pub account_id: Option<BrokerAccountId>,
    pub strategy_id: String,
    pub instrument: InstrumentId,
    pub side: OrderSide,
    pub order_type: OrderType,
    pub status: PaperOrderStatus,
    pub qty: Quantity,
    pub filled_qty: Quantity,
    pub remaining_qty: Quantity,
    pub limit_price: Option<Price>,
    pub fill_policy: PaperFillPolicy,
    pub created_ts: DateTime<Utc>,
    pub updated_ts: DateTime<Utc>,
}

impl PaperOrder {
    pub fn is_active(&self) -> bool {
        self.status.is_active()
    }

    pub fn is_terminal(&self) -> bool {
        self.status.is_terminal()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PaperTrade {
    pub schema_version: u16,
    pub paper_trade_id: PaperTradeId,
    pub paper_order_id: PaperOrderId,
    pub intent_id: RuntimeDecisionId,
    pub strategy_id: String,
    pub instrument: InstrumentId,
    pub side: OrderSide,
    pub qty: Quantity,
    pub price: Price,
    pub commission: Option<Price>,
    pub fill_policy: PaperFillPolicy,
    pub source_bar_key: String,
    pub source_ts: DateTime<Utc>,
    pub received_ts: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PaperPosition {
    pub schema_version: u16,
    pub strategy_id: String,
    pub instrument: InstrumentId,
    pub qty: Quantity,
    pub avg_price: Option<Price>,
    pub updated_ts: DateTime<Utc>,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PaperAck {
    pub schema_version: u16,
    pub intent_id: RuntimeDecisionId,
    pub paper_order_id: Option<PaperOrderId>,
    pub kind: PaperAckKind,
    pub reason: Option<String>,
    pub created_ts: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RiskGatePaperLedgerRecord {
    pub schema_version: u16,
    pub strategy_id: String,
    pub profile_id: String,
    pub session_date: String,
    pub shadow_pnl_points: Price,
    pub shadow_trade_count: u32,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RiskGatePaperState {
    pub schema_version: u16,
    pub strategy_id: String,
    pub profile_id: String,
    pub last_finalized_session_date: Option<String>,
    pub rolling_sum_lb120: Option<Price>,
    pub mr_enabled_current_session: Option<bool>,
    pub mr_enabled_next_session: Option<bool>,
    pub ledger_rows_count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PaperHybridIntradayRuntimeStateProjection {
    pub strategy_kind: String,
    pub active_cycle_id: Option<String>,
    pub next_cycle_seq: u32,
    pub last_position_qty: f64,
    pub current_owner: Option<String>,
    pub current_side: Option<String>,
    pub pending_entry_owner: Option<String>,
    pub pending_entry_side: Option<String>,
    pub pending_entry_cycle_id: Option<String>,
    pub pending_entry_request_id: Option<String>,
    pub pending_exit_request_id: Option<String>,
    pub tp_order_id: Option<String>,
    pub sl_stop_order_id: Option<String>,
    pub mr_take_price: Option<f64>,
    pub mr_stop_price: Option<f64>,
    pub safe_mode_close_only: bool,
    pub safe_mode_reason: Option<String>,
    pub entry_ready: bool,
    pub last_bar_close: Option<f64>,
    pub prev_day_close: Option<f64>,
    pub last_day_local: Option<String>,
    pub current_day_high: Option<f64>,
    pub current_day_low: Option<f64>,
    pub current_day_close: Option<f64>,
    pub prev_day_range: Option<f64>,
    pub prev_day_return: Option<f64>,
    pub day_before_close: Option<f64>,
    pub today_start_local: Option<String>,
    pub was_long_today: bool,
    pub was_short_today: bool,
    pub overnight_exit_armed_date: Option<String>,
    pub risk_gate_shadow_session_date: Option<String>,
    pub risk_gate_shadow_pnl_points: f64,
    pub risk_gate_shadow_trade_count: u32,
    pub risk_gate_mr_enabled_current_session: Option<bool>,
    pub risk_gate_mr_enabled_next_session: Option<bool>,
    pub risk_gate_rolling_sum_lb120: Option<f64>,
    pub risk_gate_last_finalized_session_date: Option<String>,
    pub risk_gate_ledger_rows_count: usize,
    pub projection_source: String,
    pub strategy_invocation_enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct PaperHybridIntradayOracleSeed {
    pub source: String,
    pub active_cycle_id: Option<String>,
    pub next_cycle_seq: Option<u32>,
    pub last_position_qty: Option<Quantity>,
    pub current_owner: Option<String>,
    pub current_side: Option<String>,
    pub prev_day_close: Option<Price>,
    pub prev_day_range: Option<Price>,
    pub prev_day_return: Option<Price>,
    pub day_before_close: Option<Price>,
    pub current_day_utc: Option<String>,
    pub current_day_high: Option<Price>,
    pub current_day_low: Option<Price>,
    pub current_day_close: Option<Price>,
    pub was_long_today: Option<bool>,
    pub was_short_today: Option<bool>,
    pub risk_gate_shadow_session_date: Option<String>,
    pub risk_gate_shadow_pnl_points: Option<Price>,
    pub risk_gate_shadow_trade_count: Option<u32>,
    pub risk_gate_mr_enabled_current_session: Option<bool>,
    pub risk_gate_mr_enabled_next_session: Option<bool>,
    pub risk_gate_rolling_sum_lb120: Option<Price>,
    pub risk_gate_last_finalized_session_date: Option<String>,
    pub risk_gate_ledger_rows_count: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaperHybridStrategyShadowConfig {
    pub enabled: bool,
    pub warmup_bars_required: usize,
    pub projection_source: String,
}

impl PaperHybridStrategyShadowConfig {
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            warmup_bars_required: 0,
            projection_source: "paper_ledger_state_only".to_string(),
        }
    }

    pub fn enabled_shadow(warmup_bars_required: usize) -> Self {
        Self {
            enabled: true,
            warmup_bars_required: warmup_bars_required.max(1),
            projection_source: "paper_hybrid_strategy_shadow_invocation".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaperHybridStrategyShadowState {
    pub config: PaperHybridStrategyShadowConfig,
    pub observed_m10_bars: usize,
    pub entry_ready: bool,
    pub last_bar_key: Option<String>,
    pub active_cycle_id: Option<String>,
    pub current_owner: Option<String>,
    pub current_side: Option<String>,
    pub pending_entry_request_id: Option<String>,
}

impl PaperHybridStrategyShadowState {
    pub fn new(config: PaperHybridStrategyShadowConfig) -> Option<Self> {
        if !config.enabled {
            return None;
        }
        let config = PaperHybridStrategyShadowConfig {
            warmup_bars_required: config.warmup_bars_required.max(1),
            ..config
        };
        Some(Self {
            config,
            observed_m10_bars: 0,
            entry_ready: false,
            last_bar_key: None,
            active_cycle_id: None,
            current_owner: None,
            current_side: None,
            pending_entry_request_id: None,
        })
    }

    pub fn observe_runtime_bar(&mut self, runtime_input: &RuntimeBarInput) {
        self.observed_m10_bars += 1;
        self.last_bar_key = Some(runtime_bar_key(runtime_input));
        self.entry_ready = self.observed_m10_bars >= self.config.warmup_bars_required;
    }

    pub fn observe_ledger(&mut self, ledger: &PaperLedgerSnapshot) {
        let position_qty = ledger.target_position_qty();
        if position_qty.is_zero() {
            self.current_side = None;
            self.current_owner = None;
            self.active_cycle_id = None;
            self.pending_entry_request_id = None;
            return;
        }
        self.current_side = side_label_from_position_qty(position_qty);
        if let Some(intent) = ledger
            .intents
            .iter()
            .rev()
            .find(|intent| matches!(intent.kind, PaperIntentKind::Enter))
        {
            self.current_owner = intent.strategy_owner.clone();
            self.active_cycle_id = intent.strategy_cycle_id.clone();
            self.pending_entry_request_id = None;
        }
    }

    pub fn apply_projection_overlay(
        &self,
        projection: &mut PaperHybridIntradayRuntimeStateProjection,
    ) {
        projection.entry_ready = self.entry_ready;
        projection.active_cycle_id = self
            .active_cycle_id
            .clone()
            .or_else(|| projection.active_cycle_id.clone());
        projection.current_owner = self
            .current_owner
            .clone()
            .or_else(|| projection.current_owner.clone());
        projection.current_side = self
            .current_side
            .clone()
            .or_else(|| projection.current_side.clone());
        projection.pending_entry_request_id = self.pending_entry_request_id.clone();
        projection.projection_source = self.config.projection_source.clone();
        projection.strategy_invocation_enabled = true;
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PaperRuntimeState {
    pub schema_version: u16,
    pub strategy_id: String,
    pub instrument: InstrumentId,
    pub execution_mode: PaperExecutionMode,
    pub safety_boundary: PaperSafetyBoundary,
    pub last_bar_key: Option<String>,
    pub last_decision_id: Option<RuntimeDecisionId>,
    pub position: Option<PaperPosition>,
    pub active_orders_count: usize,
    pub suppressed_count: usize,
    pub updated_ts: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hybrid_intraday: Option<PaperHybridIntradayRuntimeStateProjection>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaperRuntimeStreams {
    pub runtime_state_stream: String,
    pub intents_stream: String,
    pub paper_acks_stream: String,
    pub orders_stream: String,
    pub trades_stream: String,
    pub positions_stream: String,
}

impl PaperRuntimeStreams {
    pub fn finam_imoexf_paper() -> Self {
        Self {
            runtime_state_stream: "finam_imoexf_paper:runtime:state:hybrid_intraday:imoexf"
                .to_string(),
            intents_stream: "finam_imoexf_paper:runtime:intents".to_string(),
            paper_acks_stream: "finam_imoexf_paper:runtime:paper_acks".to_string(),
            orders_stream: "finam_imoexf_paper:runtime:orders_paper_only".to_string(),
            trades_stream: "finam_imoexf_paper:runtime:trades_paper_only".to_string(),
            positions_stream: "finam_imoexf_paper:runtime:positions_paper_only".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaperRuntimeAdapterConfig {
    pub ledger: PaperLedgerExecutorConfig,
    pub streams: PaperRuntimeStreams,
    pub required_provenance: String,
    pub safety_boundary: PaperSafetyBoundary,
    pub hybrid_strategy_shadow: PaperHybridStrategyShadowConfig,
}

impl PaperRuntimeAdapterConfig {
    pub fn finam_imoexf_paper(
        strategy_id: impl Into<String>,
        instrument: InstrumentId,
        execution_mode: PaperExecutionMode,
    ) -> Self {
        Self {
            ledger: PaperLedgerExecutorConfig::new(strategy_id, instrument, execution_mode, 600),
            streams: PaperRuntimeStreams::finam_imoexf_paper(),
            required_provenance: "FinamDerivedM1ToM10".to_string(),
            safety_boundary: PaperSafetyBoundary::closed(),
            hybrid_strategy_shadow: PaperHybridStrategyShadowConfig::disabled(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PaperRuntimePublishPayload {
    RuntimeState(Box<PaperRuntimeState>),
    Intent(Box<PaperIntent>),
    Ack(Box<PaperAck>),
    Order(Box<PaperOrder>),
    Trade(Box<PaperTrade>),
    Position(Box<PaperPosition>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PaperRuntimePublishRecord {
    pub stream: String,
    pub payload: PaperRuntimePublishPayload,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PaperRuntimeAdapterOutcome {
    pub publish_plan: Vec<PaperRuntimePublishRecord>,
    pub filled_count: usize,
    pub duplicate_count: usize,
    pub state_only: bool,
}

#[derive(Debug, Clone)]
pub struct PaperRuntimeAdapter {
    config: PaperRuntimeAdapterConfig,
    ledger: PaperLedgerSnapshot,
    hybrid_strategy_shadow: Option<PaperHybridStrategyShadowState>,
}

impl PaperRuntimeAdapter {
    pub fn new(
        config: PaperRuntimeAdapterConfig,
        received_ts: DateTime<Utc>,
    ) -> Result<Self, PaperRuntimeAdapterError> {
        validate_paper_runtime_adapter_config(&config)?;
        let ledger = PaperLedgerSnapshot::empty(config.ledger.clone(), received_ts);
        let hybrid_strategy_shadow =
            PaperHybridStrategyShadowState::new(config.hybrid_strategy_shadow.clone());
        Ok(Self {
            config,
            ledger,
            hybrid_strategy_shadow,
        })
    }

    pub fn config(&self) -> &PaperRuntimeAdapterConfig {
        &self.config
    }

    pub fn apply_hybrid_intraday_oracle_seed(
        &mut self,
        seed: PaperHybridIntradayOracleSeed,
        received_ts: DateTime<Utc>,
    ) -> Result<(), PaperRuntimeAdapterError> {
        self.ledger
            .apply_hybrid_intraday_oracle_seed(seed, received_ts)?;
        if let Some(shadow) = self.hybrid_strategy_shadow.as_mut() {
            shadow.observe_ledger(&self.ledger);
        }
        Ok(())
    }

    pub fn ledger(&self) -> &PaperLedgerSnapshot {
        &self.ledger
    }

    pub fn observe_runtime_bar(
        &mut self,
        runtime_input: RuntimeBarInput,
        intents: Vec<PaperIntent>,
    ) -> Result<PaperRuntimeAdapterOutcome, PaperRuntimeAdapterError> {
        self.validate_runtime_input(&runtime_input)?;

        let mut publish_plan = Vec::new();
        let mut filled_count = 0usize;
        let mut duplicate_count = 0usize;

        for intent in intents {
            let intent_for_publish = intent.clone();
            match self.ledger.apply_next_bar_open_market_intent(
                &self.config.ledger,
                intent,
                &runtime_input,
            )? {
                PaperLedgerExecutionOutcome::Filled {
                    order,
                    trade,
                    position,
                    ack,
                } => {
                    filled_count += 1;
                    publish_plan.push(PaperRuntimePublishRecord {
                        stream: self.config.streams.intents_stream.clone(),
                        payload: PaperRuntimePublishPayload::Intent(Box::new(intent_for_publish)),
                    });
                    publish_plan.push(PaperRuntimePublishRecord {
                        stream: self.config.streams.orders_stream.clone(),
                        payload: PaperRuntimePublishPayload::Order(order),
                    });
                    publish_plan.push(PaperRuntimePublishRecord {
                        stream: self.config.streams.trades_stream.clone(),
                        payload: PaperRuntimePublishPayload::Trade(trade),
                    });
                    publish_plan.push(PaperRuntimePublishRecord {
                        stream: self.config.streams.positions_stream.clone(),
                        payload: PaperRuntimePublishPayload::Position(position),
                    });
                    publish_plan.push(PaperRuntimePublishRecord {
                        stream: self.config.streams.paper_acks_stream.clone(),
                        payload: PaperRuntimePublishPayload::Ack(ack),
                    });
                }
                PaperLedgerExecutionOutcome::DuplicateIgnored { ack } => {
                    duplicate_count += 1;
                    publish_plan.push(PaperRuntimePublishRecord {
                        stream: self.config.streams.paper_acks_stream.clone(),
                        payload: PaperRuntimePublishPayload::Ack(Box::new(ack)),
                    });
                }
            }
        }

        self.ledger.observe_runtime_bar_features(&runtime_input);
        if let Some(shadow) = self.hybrid_strategy_shadow.as_mut() {
            shadow.observe_runtime_bar(&runtime_input);
            shadow.observe_ledger(&self.ledger);
        }
        let mut runtime_state = self.ledger.to_runtime_state_for_bar(&runtime_input);
        if let (Some(shadow), Some(projection)) = (
            self.hybrid_strategy_shadow.as_ref(),
            runtime_state.hybrid_intraday.as_mut(),
        ) {
            shadow.apply_projection_overlay(projection);
        }
        publish_plan.push(PaperRuntimePublishRecord {
            stream: self.config.streams.runtime_state_stream.clone(),
            payload: PaperRuntimePublishPayload::RuntimeState(Box::new(runtime_state)),
        });

        Ok(PaperRuntimeAdapterOutcome {
            state_only: filled_count == 0 && duplicate_count == 0,
            publish_plan,
            filled_count,
            duplicate_count,
        })
    }

    fn validate_runtime_input(
        &self,
        runtime_input: &RuntimeBarInput,
    ) -> Result<(), PaperRuntimeAdapterError> {
        if !self.config.safety_boundary.is_closed() || !self.ledger.safety_boundary.is_closed() {
            return Err(PaperRuntimeAdapterError::LiveBoundaryOpen);
        }
        if runtime_input.instrument != self.config.ledger.instrument {
            return Err(PaperRuntimeAdapterError::InstrumentMismatch);
        }
        if runtime_input.provenance != self.config.required_provenance {
            return Err(PaperRuntimeAdapterError::ProvenanceMismatch {
                expected: self.config.required_provenance.clone(),
                actual: runtime_input.provenance.clone(),
            });
        }
        if !runtime_input.is_live_final_timeframe(self.config.ledger.expected_timeframe_sec) {
            return Err(PaperRuntimeAdapterError::RuntimeInputNotEligible);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PaperRuntimeAdapterError {
    #[error("paper runtime adapter safety boundary is open")]
    LiveBoundaryOpen,
    #[error("paper runtime adapter instrument mismatch")]
    InstrumentMismatch,
    #[error("paper runtime adapter execution mode mismatch")]
    ExecutionModeMismatch,
    #[error("paper runtime adapter invalid expected timeframe")]
    InvalidExpectedTimeframe,
    #[error("paper runtime adapter runtime input is not eligible")]
    RuntimeInputNotEligible,
    #[error("paper runtime adapter provenance mismatch: expected={expected} actual={actual}")]
    ProvenanceMismatch { expected: String, actual: String },
    #[error("paper ledger invariant failed during adapter seed: {0}")]
    Invariant(#[from] PaperLedgerInvariantError),
    #[error("paper ledger executor failed: {0}")]
    Executor(#[from] PaperLedgerExecutorError),
}

fn validate_paper_runtime_adapter_config(
    config: &PaperRuntimeAdapterConfig,
) -> Result<(), PaperRuntimeAdapterError> {
    if !config.safety_boundary.is_closed() || !config.ledger.safety_boundary.is_closed() {
        return Err(PaperRuntimeAdapterError::LiveBoundaryOpen);
    }
    if config.ledger.expected_timeframe_sec == 0 {
        return Err(PaperRuntimeAdapterError::InvalidExpectedTimeframe);
    }
    if config.ledger.expected_timeframe_sec != 600 {
        return Err(PaperRuntimeAdapterError::InvalidExpectedTimeframe);
    }
    if config.required_provenance.trim().is_empty() {
        return Err(PaperRuntimeAdapterError::ProvenanceMismatch {
            expected: "non-empty".to_string(),
            actual: config.required_provenance.clone(),
        });
    }
    Ok(())
}

pub trait PaperRuntimePublishSink {
    fn publish(
        &mut self,
        record: PaperRuntimePublishRecord,
    ) -> Result<(), PaperRuntimePublishSinkError>;
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PaperRuntimePublishSinkError {
    #[error("paper runtime publish sink rejected record: {0}")]
    Rejected(String),
}

#[derive(Debug, Clone, Default)]
pub struct PaperRuntimeInMemorySink {
    records: Vec<PaperRuntimePublishRecord>,
}

impl PaperRuntimeInMemorySink {
    pub fn new() -> Self {
        Self {
            records: Vec::new(),
        }
    }

    pub fn records(&self) -> &[PaperRuntimePublishRecord] {
        &self.records
    }

    pub fn into_records(self) -> Vec<PaperRuntimePublishRecord> {
        self.records
    }
}

impl PaperRuntimePublishSink for PaperRuntimeInMemorySink {
    fn publish(
        &mut self,
        record: PaperRuntimePublishRecord,
    ) -> Result<(), PaperRuntimePublishSinkError> {
        self.records.push(record);
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct PaperRuntimeAdapterLoop {
    bar_publisher: PaperRuntimeBarPublisher,
    adapter: PaperRuntimeAdapter,
}

impl PaperRuntimeAdapterLoop {
    pub fn new(
        bar_publisher: PaperRuntimeBarPublisher,
        adapter: PaperRuntimeAdapter,
    ) -> Result<Self, PaperRuntimeAdapterLoopError> {
        if bar_publisher.config().instrument != adapter.config().ledger.instrument {
            return Err(PaperRuntimeAdapterLoopError::InstrumentMismatch);
        }
        if !bar_publisher.config().safety_boundary.is_closed()
            || !adapter.config().safety_boundary.is_closed()
        {
            return Err(PaperRuntimeAdapterLoopError::LiveBoundaryOpen);
        }
        Ok(Self {
            bar_publisher,
            adapter,
        })
    }

    pub fn adapter(&self) -> &PaperRuntimeAdapter {
        &self.adapter
    }

    pub fn observe_source_bar<S: PaperRuntimePublishSink>(
        &mut self,
        source_bar: Bar,
        intents: Vec<PaperIntent>,
        sink: &mut S,
    ) -> Result<PaperRuntimeAdapterLoopOutcome, PaperRuntimeAdapterLoopError> {
        match self.bar_publisher.observe_source_bar(source_bar) {
            PaperRuntimeBarPublishOutcome::Buffered {
                bucket_open_ts,
                buffered_count,
            } => Ok(PaperRuntimeAdapterLoopOutcome::Buffered {
                bucket_open_ts,
                buffered_count,
            }),
            PaperRuntimeBarPublishOutcome::DroppedIncompleteBucket {
                bucket_open_ts,
                buffered_count,
            } => Ok(PaperRuntimeAdapterLoopOutcome::DroppedIncompleteBucket {
                bucket_open_ts,
                buffered_count,
            }),
            PaperRuntimeBarPublishOutcome::Rejected { reason } => {
                Ok(PaperRuntimeAdapterLoopOutcome::SourceRejected { reason })
            }
            PaperRuntimeBarPublishOutcome::Published { runtime_input, .. } => {
                let adapter_outcome = self.adapter.observe_runtime_bar(*runtime_input, intents)?;
                let publish_count = adapter_outcome.publish_plan.len();
                for record in adapter_outcome.publish_plan.iter().cloned() {
                    sink.publish(record)?;
                }
                Ok(PaperRuntimeAdapterLoopOutcome::Published {
                    publish_count,
                    adapter_outcome: Box::new(adapter_outcome),
                })
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PaperRuntimeAdapterLoopOutcome {
    Buffered {
        bucket_open_ts: DateTime<Utc>,
        buffered_count: usize,
    },
    Published {
        publish_count: usize,
        adapter_outcome: Box<PaperRuntimeAdapterOutcome>,
    },
    DroppedIncompleteBucket {
        bucket_open_ts: DateTime<Utc>,
        buffered_count: usize,
    },
    SourceRejected {
        reason: PaperRuntimeBarPublishRejectReason,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PaperRuntimeAdapterLoopError {
    #[error("paper runtime adapter loop safety boundary is open")]
    LiveBoundaryOpen,
    #[error("paper runtime adapter loop instrument mismatch")]
    InstrumentMismatch,
    #[error("paper runtime adapter failed: {0}")]
    Adapter(#[from] PaperRuntimeAdapterError),
    #[error("paper runtime publish sink failed: {0}")]
    Sink(#[from] PaperRuntimePublishSinkError),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PaperLedgerSnapshot {
    pub schema_version: u16,
    pub strategy_id: String,
    pub instrument: InstrumentId,
    pub execution_mode: PaperExecutionMode,
    pub safety_boundary: PaperSafetyBoundary,
    pub intents: Vec<PaperIntent>,
    pub orders: Vec<PaperOrder>,
    pub trades: Vec<PaperTrade>,
    pub positions: Vec<PaperPosition>,
    pub acks: Vec<PaperAck>,
    pub suppressions: Vec<RuntimeSuppressionRecord>,
    pub risk_gate_ledger: Vec<RiskGatePaperLedgerRecord>,
    pub risk_gate_state: Option<RiskGatePaperState>,
    pub hybrid_active_cycle_id: Option<String>,
    pub hybrid_next_cycle_seq: Option<u32>,
    pub hybrid_current_owner: Option<String>,
    pub hybrid_current_side: Option<String>,
    pub risk_gate_shadow_session_date: Option<String>,
    pub risk_gate_shadow_pnl_points: Price,
    pub risk_gate_shadow_trade_count: u32,
    pub last_bar_close: Option<Price>,
    pub current_day_utc: Option<String>,
    pub current_day_high: Option<Price>,
    pub current_day_low: Option<Price>,
    pub current_day_close: Option<Price>,
    pub prev_day_close: Option<Price>,
    pub prev_day_range: Option<Price>,
    pub prev_day_return: Option<Price>,
    pub day_before_close: Option<Price>,
    pub was_long_today: bool,
    pub was_short_today: bool,
    pub received_ts: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaperLedgerExecutorConfig {
    pub strategy_id: String,
    pub instrument: InstrumentId,
    pub execution_mode: PaperExecutionMode,
    pub expected_timeframe_sec: u32,
    pub safety_boundary: PaperSafetyBoundary,
}

impl PaperLedgerExecutorConfig {
    pub fn new(
        strategy_id: impl Into<String>,
        instrument: InstrumentId,
        execution_mode: PaperExecutionMode,
        expected_timeframe_sec: u32,
    ) -> Self {
        Self {
            strategy_id: strategy_id.into(),
            instrument,
            execution_mode,
            expected_timeframe_sec,
            safety_boundary: PaperSafetyBoundary::closed(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PaperLedgerExecutionOutcome {
    Filled {
        order: Box<PaperOrder>,
        trade: Box<PaperTrade>,
        position: Box<PaperPosition>,
        ack: Box<PaperAck>,
    },
    DuplicateIgnored {
        ack: PaperAck,
    },
}

impl PaperLedgerSnapshot {
    pub fn empty(config: PaperLedgerExecutorConfig, received_ts: DateTime<Utc>) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            strategy_id: config.strategy_id,
            instrument: config.instrument,
            execution_mode: config.execution_mode,
            safety_boundary: config.safety_boundary,
            intents: Vec::new(),
            orders: Vec::new(),
            trades: Vec::new(),
            positions: Vec::new(),
            acks: Vec::new(),
            suppressions: Vec::new(),
            risk_gate_ledger: Vec::new(),
            risk_gate_state: None,
            hybrid_active_cycle_id: None,
            hybrid_next_cycle_seq: None,
            hybrid_current_owner: None,
            hybrid_current_side: None,
            risk_gate_shadow_session_date: None,
            risk_gate_shadow_pnl_points: Price::ZERO,
            risk_gate_shadow_trade_count: 0,
            last_bar_close: None,
            current_day_utc: None,
            current_day_high: None,
            current_day_low: None,
            current_day_close: None,
            prev_day_close: None,
            prev_day_range: None,
            prev_day_return: None,
            day_before_close: None,
            was_long_today: false,
            was_short_today: false,
            received_ts,
        }
    }

    pub fn validate(&self) -> Result<(), PaperLedgerInvariantError> {
        if !self.safety_boundary.is_closed() {
            return Err(PaperLedgerInvariantError::LiveBoundaryOpen);
        }
        unique_runtime_ids(
            self.intents.iter().map(|intent| &intent.intent_id),
            PaperLedgerInvariantError::DuplicateIntentId,
        )?;
        unique_order_ids(self.orders.iter().map(|order| &order.paper_order_id))?;
        unique_trade_ids(self.trades.iter().map(|trade| &trade.paper_trade_id))?;

        let order_ids: HashSet<&PaperOrderId> = self
            .orders
            .iter()
            .map(|order| &order.paper_order_id)
            .collect();
        let intent_ids: HashSet<&RuntimeDecisionId> = self
            .intents
            .iter()
            .map(|intent| &intent.intent_id)
            .collect();

        for order in &self.orders {
            if order.instrument != self.instrument {
                return Err(PaperLedgerInvariantError::InstrumentMismatch);
            }
            if order.strategy_id != self.strategy_id {
                return Err(PaperLedgerInvariantError::StrategyMismatch);
            }
            if !intent_ids.contains(&order.intent_id) {
                return Err(PaperLedgerInvariantError::OrderReferencesMissingIntent);
            }
            if order.qty < order.filled_qty {
                return Err(PaperLedgerInvariantError::FilledQuantityExceedsOrderQuantity);
            }
            if order.qty - order.filled_qty != order.remaining_qty {
                return Err(PaperLedgerInvariantError::RemainingQuantityMismatch);
            }
        }

        for trade in &self.trades {
            if trade.instrument != self.instrument {
                return Err(PaperLedgerInvariantError::InstrumentMismatch);
            }
            if trade.strategy_id != self.strategy_id {
                return Err(PaperLedgerInvariantError::StrategyMismatch);
            }
            if !order_ids.contains(&trade.paper_order_id) {
                return Err(PaperLedgerInvariantError::TradeReferencesMissingOrder);
            }
            if !intent_ids.contains(&trade.intent_id) {
                return Err(PaperLedgerInvariantError::TradeReferencesMissingIntent);
            }
        }

        for position in &self.positions {
            if position.instrument != self.instrument {
                return Err(PaperLedgerInvariantError::InstrumentMismatch);
            }
            if position.strategy_id != self.strategy_id {
                return Err(PaperLedgerInvariantError::StrategyMismatch);
            }
        }

        for ack in &self.acks {
            if !intent_ids.contains(&ack.intent_id) {
                return Err(PaperLedgerInvariantError::AckReferencesMissingIntent);
            }
            if let Some(order_id) = &ack.paper_order_id {
                if !order_ids.contains(order_id) {
                    return Err(PaperLedgerInvariantError::AckReferencesMissingOrder);
                }
            }
        }

        Ok(())
    }

    pub fn target_position_qty(&self) -> Quantity {
        self.positions
            .iter()
            .filter(|position| position.instrument == self.instrument)
            .map(|position| position.qty)
            .next_back()
            .unwrap_or_default()
    }

    pub fn target_is_flat(&self) -> bool {
        self.target_position_qty().is_zero()
    }

    pub fn apply_hybrid_intraday_oracle_seed(
        &mut self,
        seed: PaperHybridIntradayOracleSeed,
        received_ts: DateTime<Utc>,
    ) -> Result<(), PaperLedgerInvariantError> {
        if let Some(qty) = seed.last_position_qty {
            self.positions.push(PaperPosition {
                schema_version: SCHEMA_VERSION,
                strategy_id: self.strategy_id.clone(),
                instrument: self.instrument.clone(),
                qty,
                avg_price: None,
                updated_ts: received_ts,
                source: format!("paper_oracle_seed:{}", seed.source),
            });
        }

        self.hybrid_active_cycle_id = seed.active_cycle_id;
        self.hybrid_next_cycle_seq = seed.next_cycle_seq;
        self.hybrid_current_owner = seed.current_owner;
        self.hybrid_current_side = seed.current_side;
        self.prev_day_close = seed.prev_day_close;
        self.prev_day_range = seed.prev_day_range;
        self.prev_day_return = seed.prev_day_return;
        self.day_before_close = seed.day_before_close;
        self.current_day_utc = seed.current_day_utc;
        self.current_day_high = seed.current_day_high;
        self.current_day_low = seed.current_day_low;
        self.current_day_close = seed.current_day_close;
        if let Some(value) = seed.was_long_today {
            self.was_long_today = value;
        }
        if let Some(value) = seed.was_short_today {
            self.was_short_today = value;
        }
        self.risk_gate_shadow_session_date = seed.risk_gate_shadow_session_date;
        if let Some(value) = seed.risk_gate_shadow_pnl_points {
            self.risk_gate_shadow_pnl_points = value;
        }
        if let Some(value) = seed.risk_gate_shadow_trade_count {
            self.risk_gate_shadow_trade_count = value;
        }
        self.risk_gate_state = Some(RiskGatePaperState {
            schema_version: SCHEMA_VERSION,
            strategy_id: self.strategy_id.clone(),
            profile_id: "imoexf_primary_high180_lb120".to_string(),
            last_finalized_session_date: seed.risk_gate_last_finalized_session_date,
            rolling_sum_lb120: seed.risk_gate_rolling_sum_lb120,
            mr_enabled_current_session: seed.risk_gate_mr_enabled_current_session,
            mr_enabled_next_session: seed.risk_gate_mr_enabled_next_session,
            ledger_rows_count: seed.risk_gate_ledger_rows_count.unwrap_or_default(),
        });
        self.received_ts = received_ts;
        self.validate()
    }

    pub fn active_orders(&self) -> Vec<&PaperOrder> {
        self.orders
            .iter()
            .filter(|order| order.is_active())
            .collect()
    }

    pub fn apply_next_bar_open_market_intent(
        &mut self,
        config: &PaperLedgerExecutorConfig,
        intent: PaperIntent,
        fill_bar: &RuntimeBarInput,
    ) -> Result<PaperLedgerExecutionOutcome, PaperLedgerExecutorError> {
        self.validate_executor_config(config)?;
        self.validate_intent_for_next_bar_open(&intent)?;
        self.validate_fill_bar(config, &intent, fill_bar)?;

        if self
            .intents
            .iter()
            .any(|existing| existing.intent_id == intent.intent_id)
        {
            let ack = PaperAck {
                schema_version: SCHEMA_VERSION,
                intent_id: intent.intent_id,
                paper_order_id: None,
                kind: PaperAckKind::DuplicateIgnored,
                reason: Some("duplicate_runtime_decision_id".to_string()),
                created_ts: fill_bar.close_ts,
            };
            return Ok(PaperLedgerExecutionOutcome::DuplicateIgnored { ack });
        }

        let side = intent.side.ok_or(PaperLedgerExecutorError::MissingSide)?;
        let order_type = intent
            .order_type
            .ok_or(PaperLedgerExecutorError::MissingOrderType)?;
        if order_type != OrderType::Market {
            return Err(PaperLedgerExecutorError::UnsupportedOrderType(order_type));
        }

        let paper_order_id = paper_order_id_for(&intent.intent_id);
        let paper_trade_id = paper_trade_id_for(&intent.intent_id);
        let price = fill_bar.open;
        let next_position =
            self.next_position_after_fill(&intent, side, price, fill_bar.open_ts)?;

        let order = PaperOrder {
            schema_version: SCHEMA_VERSION,
            paper_order_id: paper_order_id.clone(),
            intent_id: intent.intent_id.clone(),
            account_id: None,
            strategy_id: intent.strategy_id.clone(),
            instrument: intent.instrument.clone(),
            side,
            order_type,
            status: PaperOrderStatus::Filled,
            qty: intent.qty,
            filled_qty: intent.qty,
            remaining_qty: Quantity::ZERO,
            limit_price: None,
            fill_policy: PaperFillPolicy::NextFinalBarOpen,
            created_ts: intent.created_ts,
            updated_ts: fill_bar.open_ts,
        };
        let trade = PaperTrade {
            schema_version: SCHEMA_VERSION,
            paper_trade_id,
            paper_order_id: paper_order_id.clone(),
            intent_id: intent.intent_id.clone(),
            strategy_id: intent.strategy_id.clone(),
            instrument: intent.instrument.clone(),
            side,
            qty: intent.qty,
            price,
            commission: Some(Price::ZERO),
            fill_policy: PaperFillPolicy::NextFinalBarOpen,
            source_bar_key: intent.decision_bar_key.clone(),
            source_ts: fill_bar.open_ts,
            received_ts: fill_bar.close_ts,
        };
        let ack = PaperAck {
            schema_version: SCHEMA_VERSION,
            intent_id: intent.intent_id.clone(),
            paper_order_id: Some(paper_order_id),
            kind: PaperAckKind::Filled,
            reason: Some("next_final_bar_open_proxy".to_string()),
            created_ts: fill_bar.open_ts,
        };

        self.intents.push(intent);
        self.orders.push(order.clone());
        self.trades.push(trade.clone());
        self.positions.push(next_position.clone());
        self.acks.push(ack.clone());
        self.received_ts = fill_bar.close_ts;
        self.validate()?;

        Ok(PaperLedgerExecutionOutcome::Filled {
            order: Box::new(order),
            trade: Box::new(trade),
            position: Box::new(next_position),
            ack: Box::new(ack),
        })
    }

    pub fn to_runtime_state(&self, updated_ts: DateTime<Utc>) -> PaperRuntimeState {
        self.to_runtime_state_inner(updated_ts, None)
    }

    pub fn to_runtime_state_for_bar(&self, runtime_input: &RuntimeBarInput) -> PaperRuntimeState {
        self.to_runtime_state_inner(runtime_input.close_ts, Some(runtime_input))
    }

    fn to_runtime_state_inner(
        &self,
        updated_ts: DateTime<Utc>,
        runtime_input: Option<&RuntimeBarInput>,
    ) -> PaperRuntimeState {
        PaperRuntimeState {
            schema_version: SCHEMA_VERSION,
            strategy_id: self.strategy_id.clone(),
            instrument: self.instrument.clone(),
            execution_mode: self.execution_mode,
            safety_boundary: self.safety_boundary,
            last_bar_key: runtime_input.map(runtime_bar_key).or_else(|| {
                self.intents
                    .last()
                    .map(|intent| intent.decision_bar_key.clone())
            }),
            last_decision_id: self.intents.last().map(|intent| intent.intent_id.clone()),
            position: self.positions.last().cloned(),
            active_orders_count: self.active_orders().len(),
            suppressed_count: self.suppressions.len(),
            updated_ts,
            hybrid_intraday: Some(self.to_hybrid_intraday_runtime_state_projection()),
        }
    }

    pub fn observe_runtime_bar_features(&mut self, runtime_input: &RuntimeBarInput) {
        let day = runtime_input.close_ts.date_naive().to_string();
        if self.current_day_utc.as_deref() != Some(day.as_str()) {
            if self.current_day_utc.is_some() {
                self.day_before_close = self.prev_day_close;
                self.prev_day_close = self.current_day_close;
                self.prev_day_range = match (self.current_day_high, self.current_day_low) {
                    (Some(high), Some(low)) => Some(high - low),
                    _ => None,
                };
                self.prev_day_return = match (self.prev_day_close, self.day_before_close) {
                    (Some(prev), Some(day_before)) if !day_before.is_zero() => {
                        Some((prev - day_before) / day_before)
                    }
                    _ => None,
                };
            }
            self.current_day_utc = Some(day);
            self.current_day_high = None;
            self.current_day_low = None;
            self.current_day_close = None;
            self.was_long_today = false;
            self.was_short_today = false;
        }

        self.last_bar_close = Some(runtime_input.close);
        self.current_day_high = Some(match self.current_day_high {
            Some(current) => current.max(runtime_input.high),
            None => runtime_input.high,
        });
        self.current_day_low = Some(match self.current_day_low {
            Some(current) => current.min(runtime_input.low),
            None => runtime_input.low,
        });
        self.current_day_close = Some(runtime_input.close);
        self.received_ts = runtime_input.close_ts;
    }

    pub fn to_hybrid_intraday_runtime_state_projection(
        &self,
    ) -> PaperHybridIntradayRuntimeStateProjection {
        let last_position_qty = self.target_position_qty();
        let current_side = side_label_from_position_qty(last_position_qty);
        PaperHybridIntradayRuntimeStateProjection {
            strategy_kind: "hybrid_intraday".to_string(),
            active_cycle_id: self.hybrid_active_cycle_id.clone(),
            next_cycle_seq: self.hybrid_next_cycle_seq.unwrap_or_default(),
            last_position_qty: decimal_to_f64(last_position_qty),
            current_owner: self.hybrid_current_owner.clone(),
            current_side: current_side.or_else(|| self.hybrid_current_side.clone()),
            pending_entry_owner: None,
            pending_entry_side: None,
            pending_entry_cycle_id: None,
            pending_entry_request_id: None,
            pending_exit_request_id: None,
            tp_order_id: None,
            sl_stop_order_id: None,
            mr_take_price: None,
            mr_stop_price: None,
            safe_mode_close_only: false,
            safe_mode_reason: None,
            entry_ready: false,
            last_bar_close: option_decimal_to_f64(self.last_bar_close),
            prev_day_close: option_decimal_to_f64(self.prev_day_close),
            last_day_local: self.current_day_utc.clone(),
            current_day_high: option_decimal_to_f64(self.current_day_high),
            current_day_low: option_decimal_to_f64(self.current_day_low),
            current_day_close: option_decimal_to_f64(self.current_day_close),
            prev_day_range: option_decimal_to_f64(self.prev_day_range),
            prev_day_return: option_decimal_to_f64(self.prev_day_return).or_else(|| {
                match (self.prev_day_close, self.day_before_close) {
                    (Some(prev), Some(day_before)) if !day_before.is_zero() => {
                        option_decimal_to_f64(Some((prev - day_before) / day_before))
                    }
                    _ => None,
                }
            }),
            day_before_close: option_decimal_to_f64(self.day_before_close),
            today_start_local: self
                .current_day_utc
                .as_ref()
                .map(|day| format!("{day}T09:00:00")),
            was_long_today: self.was_long_today,
            was_short_today: self.was_short_today,
            overnight_exit_armed_date: None,
            risk_gate_shadow_session_date: self.risk_gate_shadow_session_date.clone(),
            risk_gate_shadow_pnl_points: decimal_to_f64(self.risk_gate_shadow_pnl_points),
            risk_gate_shadow_trade_count: self.risk_gate_shadow_trade_count,
            risk_gate_mr_enabled_current_session: self
                .risk_gate_state
                .as_ref()
                .and_then(|state| state.mr_enabled_current_session),
            risk_gate_mr_enabled_next_session: self
                .risk_gate_state
                .as_ref()
                .and_then(|state| state.mr_enabled_next_session),
            risk_gate_rolling_sum_lb120: self
                .risk_gate_state
                .as_ref()
                .and_then(|state| option_decimal_to_f64(state.rolling_sum_lb120)),
            risk_gate_last_finalized_session_date: self
                .risk_gate_state
                .as_ref()
                .and_then(|state| state.last_finalized_session_date.clone()),
            risk_gate_ledger_rows_count: self
                .risk_gate_state
                .as_ref()
                .map(|state| state.ledger_rows_count)
                .unwrap_or_else(|| self.risk_gate_ledger.len()),
            projection_source: "paper_ledger_state_only".to_string(),
            strategy_invocation_enabled: false,
        }
    }

    fn validate_executor_config(
        &self,
        config: &PaperLedgerExecutorConfig,
    ) -> Result<(), PaperLedgerExecutorError> {
        if !config.safety_boundary.is_closed() || !self.safety_boundary.is_closed() {
            return Err(PaperLedgerExecutorError::LiveBoundaryOpen);
        }
        if config.strategy_id != self.strategy_id {
            return Err(PaperLedgerExecutorError::StrategyMismatch);
        }
        if config.instrument != self.instrument {
            return Err(PaperLedgerExecutorError::InstrumentMismatch);
        }
        if config.execution_mode != self.execution_mode {
            return Err(PaperLedgerExecutorError::ExecutionModeMismatch);
        }
        if config.expected_timeframe_sec == 0 {
            return Err(PaperLedgerExecutorError::InvalidExpectedTimeframe);
        }
        Ok(())
    }

    fn validate_intent_for_next_bar_open(
        &self,
        intent: &PaperIntent,
    ) -> Result<(), PaperLedgerExecutorError> {
        if intent.strategy_id != self.strategy_id {
            return Err(PaperLedgerExecutorError::StrategyMismatch);
        }
        if intent.instrument != self.instrument {
            return Err(PaperLedgerExecutorError::InstrumentMismatch);
        }
        if !matches!(intent.kind, PaperIntentKind::Enter | PaperIntentKind::Exit) {
            return Err(PaperLedgerExecutorError::UnsupportedIntentKind(intent.kind));
        }
        if intent.qty <= Quantity::ZERO {
            return Err(PaperLedgerExecutorError::NonPositiveQuantity);
        }
        if intent.fill_policy != PaperFillPolicy::NextFinalBarOpen {
            return Err(PaperLedgerExecutorError::UnsupportedFillPolicy(
                intent.fill_policy.clone(),
            ));
        }
        Ok(())
    }

    fn validate_fill_bar(
        &self,
        config: &PaperLedgerExecutorConfig,
        intent: &PaperIntent,
        fill_bar: &RuntimeBarInput,
    ) -> Result<(), PaperLedgerExecutorError> {
        if fill_bar.instrument != self.instrument {
            return Err(PaperLedgerExecutorError::InstrumentMismatch);
        }
        if !config.execution_mode.can_advance(fill_bar.origin) {
            return Err(PaperLedgerExecutorError::FillBarNotEligible(
                RuntimeSuppressionReason::NonLiveOrigin,
            ));
        }
        if !fill_bar.is_final {
            return Err(PaperLedgerExecutorError::FillBarNotEligible(
                RuntimeSuppressionReason::NonFinalBar,
            ));
        }
        if fill_bar.timeframe_sec != config.expected_timeframe_sec {
            return Err(PaperLedgerExecutorError::FillBarNotEligible(
                RuntimeSuppressionReason::TimeframeMismatch {
                    expected_sec: config.expected_timeframe_sec,
                    actual_sec: fill_bar.timeframe_sec,
                },
            ));
        }
        if fill_bar.open_ts < intent.created_ts {
            return Err(PaperLedgerExecutorError::FillBarPrecedesDecision);
        }
        Ok(())
    }

    fn next_position_after_fill(
        &self,
        intent: &PaperIntent,
        side: OrderSide,
        fill_price: Price,
        updated_ts: DateTime<Utc>,
    ) -> Result<PaperPosition, PaperLedgerExecutorError> {
        let current_qty = self.target_position_qty();
        let current_avg = self
            .positions
            .last()
            .and_then(|position| position.avg_price)
            .unwrap_or(fill_price);
        let delta = signed_qty(side, intent.qty);
        let next_qty = current_qty + delta;
        let next_avg = next_avg_price(current_qty, current_avg, delta, fill_price);

        Ok(PaperPosition {
            schema_version: SCHEMA_VERSION,
            strategy_id: self.strategy_id.clone(),
            instrument: self.instrument.clone(),
            qty: next_qty,
            avg_price: Some(next_avg),
            updated_ts,
            source: "paper_synthetic_position_feedback".to_string(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PaperLedgerInvariantError {
    #[error("paper safety boundary is open")]
    LiveBoundaryOpen,
    #[error("duplicate paper intent id")]
    DuplicateIntentId,
    #[error("duplicate paper order id")]
    DuplicateOrderId,
    #[error("duplicate paper trade id")]
    DuplicateTradeId,
    #[error("paper record strategy_id does not match ledger strategy_id")]
    StrategyMismatch,
    #[error("paper record instrument does not match ledger instrument")]
    InstrumentMismatch,
    #[error("paper order references missing intent")]
    OrderReferencesMissingIntent,
    #[error("paper trade references missing order")]
    TradeReferencesMissingOrder,
    #[error("paper trade references missing intent")]
    TradeReferencesMissingIntent,
    #[error("paper ack references missing intent")]
    AckReferencesMissingIntent,
    #[error("paper ack references missing order")]
    AckReferencesMissingOrder,
    #[error("filled quantity exceeds order quantity")]
    FilledQuantityExceedsOrderQuantity,
    #[error("remaining quantity does not match qty - filled_qty")]
    RemainingQuantityMismatch,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PaperLedgerExecutorError {
    #[error("paper safety boundary is open")]
    LiveBoundaryOpen,
    #[error("paper executor strategy_id mismatch")]
    StrategyMismatch,
    #[error("paper executor instrument mismatch")]
    InstrumentMismatch,
    #[error("paper executor execution mode mismatch")]
    ExecutionModeMismatch,
    #[error("paper executor expected timeframe must be positive")]
    InvalidExpectedTimeframe,
    #[error("paper intent is missing side")]
    MissingSide,
    #[error("paper intent is missing order type")]
    MissingOrderType,
    #[error("unsupported paper intent kind: {0:?}")]
    UnsupportedIntentKind(PaperIntentKind),
    #[error("unsupported paper order type: {0:?}")]
    UnsupportedOrderType(OrderType),
    #[error("unsupported paper fill policy: {0:?}")]
    UnsupportedFillPolicy(PaperFillPolicy),
    #[error("paper quantity must be positive")]
    NonPositiveQuantity,
    #[error("paper fill bar is not eligible: {0:?}")]
    FillBarNotEligible(RuntimeSuppressionReason),
    #[error("paper fill bar precedes decision timestamp")]
    FillBarPrecedesDecision,
    #[error("paper ledger invariant failed: {0}")]
    Invariant(#[from] PaperLedgerInvariantError),
}

fn paper_order_id_for(intent_id: &RuntimeDecisionId) -> PaperOrderId {
    PaperOrderId::new(format!(
        "PAPER_ORDER_{}",
        stable_id_suffix(intent_id.as_str())
    ))
}

fn paper_trade_id_for(intent_id: &RuntimeDecisionId) -> PaperTradeId {
    PaperTradeId::new(format!(
        "PAPER_TRADE_{}",
        stable_id_suffix(intent_id.as_str())
    ))
}

fn stable_id_suffix(raw: &str) -> String {
    raw.chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_uppercase()
            } else {
                '_'
            }
        })
        .collect()
}

fn signed_qty(side: OrderSide, qty: Quantity) -> Quantity {
    match side {
        OrderSide::Buy => qty,
        OrderSide::Sell => -qty,
    }
}

fn runtime_bar_key(input: &RuntimeBarInput) -> String {
    format!("{}/{}", input.close_ts.to_rfc3339(), input.timeframe_sec)
}

fn decimal_to_f64(value: Price) -> f64 {
    value.to_string().parse::<f64>().unwrap_or_default()
}

fn option_decimal_to_f64(value: Option<Price>) -> Option<f64> {
    value.map(decimal_to_f64)
}

pub fn f64_to_price(value: f64) -> Option<Price> {
    if !value.is_finite() {
        return None;
    }
    Price::from_str(&value.to_string()).ok()
}

fn side_label_from_position_qty(qty: Quantity) -> Option<String> {
    if qty > Quantity::ZERO {
        Some("long".to_string())
    } else if qty < Quantity::ZERO {
        Some("short".to_string())
    } else {
        None
    }
}

fn same_direction_or_flat(left: Quantity, right: Quantity) -> bool {
    left.is_zero() || right.is_zero() || (left > Quantity::ZERO) == (right > Quantity::ZERO)
}

fn next_avg_price(
    current_qty: Quantity,
    current_avg: Price,
    delta_qty: Quantity,
    fill_price: Price,
) -> Price {
    let next_qty = current_qty + delta_qty;
    if next_qty.is_zero() {
        return Price::ZERO;
    }
    if same_direction_or_flat(current_qty, delta_qty) {
        let current_abs = current_qty.abs();
        let delta_abs = delta_qty.abs();
        return ((current_abs * current_avg) + (delta_abs * fill_price)) / next_qty.abs();
    }
    if delta_qty.abs() < current_qty.abs() {
        current_avg
    } else {
        fill_price
    }
}

fn unique_runtime_ids<'a>(
    ids: impl Iterator<Item = &'a RuntimeDecisionId>,
    error: PaperLedgerInvariantError,
) -> Result<(), PaperLedgerInvariantError> {
    let mut seen = HashSet::new();
    for id in ids {
        if !seen.insert(id) {
            return Err(error);
        }
    }
    Ok(())
}

fn unique_order_ids<'a>(
    ids: impl Iterator<Item = &'a PaperOrderId>,
) -> Result<(), PaperLedgerInvariantError> {
    let mut seen = HashSet::new();
    for id in ids {
        if !seen.insert(id) {
            return Err(PaperLedgerInvariantError::DuplicateOrderId);
        }
    }
    Ok(())
}

fn unique_trade_ids<'a>(
    ids: impl Iterator<Item = &'a PaperTradeId>,
) -> Result<(), PaperLedgerInvariantError> {
    let mut seen = HashSet::new();
    for id in ids {
        if !seen.insert(id) {
            return Err(PaperLedgerInvariantError::DuplicateTradeId);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use chrono::Duration;
    use chrono::TimeZone;
    use rust_decimal::Decimal;

    use super::*;
    use crate::instrument::{Exchange, Market};

    fn ts(minute: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 1, 1, 9, minute, 0)
            .single()
            .expect("timestamp")
    }

    fn instrument() -> InstrumentId {
        InstrumentId {
            symbol: "IMOEXF".to_string(),
            venue_symbol: Some("IMOEXF@RTSX".to_string()),
            exchange: Exchange::Moex,
            market: Market::Futures,
        }
    }

    fn decision(id: &str, side: OrderSide, minute: u32) -> RuntimeDecisionRecord {
        RuntimeDecisionRecord {
            schema_version: SCHEMA_VERSION,
            decision_id: RuntimeDecisionId::new(id),
            strategy_id: "hybrid_imoexf_synthetic".to_string(),
            decision_bar_key: format!("IMOEXF:2026-01-01T09:{minute:02}:00Z"),
            instrument: instrument(),
            side,
            order_type: OrderType::Market,
            qty: Decimal::ONE,
            limit_price: None,
            time_in_force: Some(TimeInForce::Day),
            fill_policy: PaperFillPolicy::NextFinalBarOpen,
            created_ts: ts(minute),
            source_bar_close_ts: ts(minute),
        }
    }

    fn valid_flat_snapshot() -> PaperLedgerSnapshot {
        let buy_intent = PaperIntent::from_decision(
            PaperIntentKind::Enter,
            decision("decision-buy", OrderSide::Buy, 10),
        );
        let sell_intent = PaperIntent::from_decision(
            PaperIntentKind::Exit,
            decision("decision-sell", OrderSide::Sell, 20),
        );
        let buy_order_id = PaperOrderId::new("PAPER_ORDER_0001");
        let sell_order_id = PaperOrderId::new("PAPER_ORDER_0002");
        PaperLedgerSnapshot {
            schema_version: SCHEMA_VERSION,
            strategy_id: "hybrid_imoexf_synthetic".to_string(),
            instrument: instrument(),
            execution_mode: PaperExecutionMode::LiveOnly,
            safety_boundary: PaperSafetyBoundary::closed(),
            intents: vec![buy_intent.clone(), sell_intent.clone()],
            orders: vec![
                PaperOrder {
                    schema_version: SCHEMA_VERSION,
                    paper_order_id: buy_order_id.clone(),
                    intent_id: buy_intent.intent_id.clone(),
                    account_id: None,
                    strategy_id: "hybrid_imoexf_synthetic".to_string(),
                    instrument: instrument(),
                    side: OrderSide::Buy,
                    order_type: OrderType::Market,
                    status: PaperOrderStatus::Filled,
                    qty: Decimal::ONE,
                    filled_qty: Decimal::ONE,
                    remaining_qty: Decimal::ZERO,
                    limit_price: None,
                    fill_policy: PaperFillPolicy::NextFinalBarOpen,
                    created_ts: ts(10),
                    updated_ts: ts(10),
                },
                PaperOrder {
                    schema_version: SCHEMA_VERSION,
                    paper_order_id: sell_order_id.clone(),
                    intent_id: sell_intent.intent_id.clone(),
                    account_id: None,
                    strategy_id: "hybrid_imoexf_synthetic".to_string(),
                    instrument: instrument(),
                    side: OrderSide::Sell,
                    order_type: OrderType::Market,
                    status: PaperOrderStatus::Filled,
                    qty: Decimal::ONE,
                    filled_qty: Decimal::ONE,
                    remaining_qty: Decimal::ZERO,
                    limit_price: None,
                    fill_policy: PaperFillPolicy::NextFinalBarOpen,
                    created_ts: ts(20),
                    updated_ts: ts(20),
                },
            ],
            trades: vec![
                PaperTrade {
                    schema_version: SCHEMA_VERSION,
                    paper_trade_id: PaperTradeId::new("PAPER_TRADE_0001"),
                    paper_order_id: buy_order_id,
                    intent_id: buy_intent.intent_id.clone(),
                    strategy_id: "hybrid_imoexf_synthetic".to_string(),
                    instrument: instrument(),
                    side: OrderSide::Buy,
                    qty: Decimal::ONE,
                    price: Decimal::new(1000, 0),
                    commission: Some(Decimal::ZERO),
                    fill_policy: PaperFillPolicy::NextFinalBarOpen,
                    source_bar_key: "IMOEXF:2026-01-01T09:10:00Z".to_string(),
                    source_ts: ts(10),
                    received_ts: ts(10),
                },
                PaperTrade {
                    schema_version: SCHEMA_VERSION,
                    paper_trade_id: PaperTradeId::new("PAPER_TRADE_0002"),
                    paper_order_id: sell_order_id,
                    intent_id: sell_intent.intent_id.clone(),
                    strategy_id: "hybrid_imoexf_synthetic".to_string(),
                    instrument: instrument(),
                    side: OrderSide::Sell,
                    qty: Decimal::ONE,
                    price: Decimal::new(1002, 0),
                    commission: Some(Decimal::ZERO),
                    fill_policy: PaperFillPolicy::NextFinalBarOpen,
                    source_bar_key: "IMOEXF:2026-01-01T09:20:00Z".to_string(),
                    source_ts: ts(20),
                    received_ts: ts(20),
                },
            ],
            positions: vec![PaperPosition {
                schema_version: SCHEMA_VERSION,
                strategy_id: "hybrid_imoexf_synthetic".to_string(),
                instrument: instrument(),
                qty: Decimal::ZERO,
                avg_price: Some(Decimal::ZERO),
                updated_ts: ts(20),
                source: "paper_synthetic_position_feedback".to_string(),
            }],
            acks: vec![
                PaperAck {
                    schema_version: SCHEMA_VERSION,
                    intent_id: buy_intent.intent_id,
                    paper_order_id: Some(PaperOrderId::new("PAPER_ORDER_0001")),
                    kind: PaperAckKind::Filled,
                    reason: None,
                    created_ts: ts(10),
                },
                PaperAck {
                    schema_version: SCHEMA_VERSION,
                    intent_id: sell_intent.intent_id,
                    paper_order_id: Some(PaperOrderId::new("PAPER_ORDER_0002")),
                    kind: PaperAckKind::Filled,
                    reason: None,
                    created_ts: ts(20),
                },
            ],
            suppressions: vec![],
            risk_gate_ledger: vec![],
            risk_gate_state: None,
            hybrid_active_cycle_id: None,
            hybrid_next_cycle_seq: None,
            hybrid_current_owner: None,
            hybrid_current_side: None,
            risk_gate_shadow_session_date: None,
            risk_gate_shadow_pnl_points: Decimal::ZERO,
            risk_gate_shadow_trade_count: 0,
            last_bar_close: None,
            current_day_utc: None,
            current_day_high: None,
            current_day_low: None,
            current_day_close: None,
            prev_day_close: None,
            prev_day_range: None,
            prev_day_return: None,
            day_before_close: None,
            was_long_today: false,
            was_short_today: false,
            received_ts: ts(20),
        }
    }

    fn executor_config() -> PaperLedgerExecutorConfig {
        PaperLedgerExecutorConfig::new(
            "hybrid_imoexf_synthetic",
            instrument(),
            PaperExecutionMode::LiveOnly,
            600,
        )
    }

    fn empty_snapshot() -> PaperLedgerSnapshot {
        PaperLedgerSnapshot::empty(executor_config(), ts(0))
    }

    fn runtime_bar(minute: u32, origin: RuntimeBarOrigin, timeframe_sec: u32) -> RuntimeBarInput {
        RuntimeBarInput {
            schema_version: SCHEMA_VERSION,
            instrument: instrument(),
            origin,
            timeframe_sec,
            open_ts: ts(minute),
            close_ts: ts(minute + 10),
            open: Decimal::new(1000 + i64::from(minute), 0),
            high: Decimal::new(1005 + i64::from(minute), 0),
            low: Decimal::new(999 + i64::from(minute), 0),
            close: Decimal::new(1002 + i64::from(minute), 0),
            volume: Decimal::new(100, 0),
            is_final: true,
            source_stream: "finam_imoexf_paper:md:bars:10m".to_string(),
            provenance: "FinamDerivedM1ToM10".to_string(),
        }
    }

    fn m1_bar(minute: u32, source_kind: MarketDataSourceKind, is_final: bool) -> Bar {
        let open_ts = Utc
            .with_ymd_and_hms(2026, 1, 1, 9, minute, 0)
            .single()
            .expect("timestamp");
        Bar {
            instrument: instrument(),
            source_kind,
            timeframe_sec: 60,
            open_ts,
            close_ts: open_ts + Duration::seconds(60),
            open: Decimal::new(1000 + i64::from(minute), 0),
            high: Decimal::new(1005 + i64::from(minute), 0),
            low: Decimal::new(999 + i64::from(minute), 0),
            close: Decimal::new(1002 + i64::from(minute), 0),
            volume: Decimal::new(10 + i64::from(minute), 0),
            is_final,
        }
    }

    fn paper_bar_publisher() -> PaperRuntimeBarPublisher {
        PaperRuntimeBarPublisher::new(PaperRuntimeBarPublisherConfig::finam_m1_to_m10_paper(
            "hybrid_imoexf_synthetic",
            instrument(),
            "finam_imoexf_paper:ws:market_data",
            "finam_imoexf_paper:md:bars:10m",
        ))
        .expect("valid paper runtime bar publisher")
    }

    fn paper_runtime_adapter() -> PaperRuntimeAdapter {
        PaperRuntimeAdapter::new(
            PaperRuntimeAdapterConfig::finam_imoexf_paper(
                "hybrid_imoexf_synthetic",
                instrument(),
                PaperExecutionMode::LiveOnly,
            ),
            ts(0),
        )
        .expect("valid paper runtime adapter")
    }

    fn paper_runtime_loop() -> PaperRuntimeAdapterLoop {
        PaperRuntimeAdapterLoop::new(paper_bar_publisher(), paper_runtime_adapter())
            .expect("valid paper runtime adapter loop")
    }

    #[test]
    fn paper_execution_mode_matches_alor_live_only_vs_history_sim_gate() {
        assert!(PaperExecutionMode::LiveOnly.can_advance(RuntimeBarOrigin::Live));
        assert!(!PaperExecutionMode::LiveOnly.can_advance(RuntimeBarOrigin::History));
        assert!(!PaperExecutionMode::LiveOnly.can_advance(RuntimeBarOrigin::HistoryGap));
        assert!(!PaperExecutionMode::LiveOnly.can_advance(RuntimeBarOrigin::Replay));
        assert!(PaperExecutionMode::HistorySim.can_advance(RuntimeBarOrigin::History));
        assert!(PaperExecutionMode::HistorySim.can_advance(RuntimeBarOrigin::Replay));
    }

    #[test]
    fn runtime_bar_input_requires_live_final_expected_timeframe() {
        let mut bar = RuntimeBarInput {
            schema_version: SCHEMA_VERSION,
            instrument: instrument(),
            origin: RuntimeBarOrigin::Live,
            timeframe_sec: 600,
            open_ts: ts(0),
            close_ts: ts(10),
            open: Decimal::new(1000, 0),
            high: Decimal::new(1005, 0),
            low: Decimal::new(999, 0),
            close: Decimal::new(1002, 0),
            volume: Decimal::new(100, 0),
            is_final: true,
            source_stream: "finam_imoexf_paper:md:bars:10m".to_string(),
            provenance: "FinamDerivedM1ToM10".to_string(),
        };
        assert!(bar.is_live_final_timeframe(600));
        bar.timeframe_sec = 60;
        assert!(!bar.is_live_final_timeframe(600));
        bar.timeframe_sec = 600;
        bar.origin = RuntimeBarOrigin::HistoryGap;
        assert!(!bar.is_live_final_timeframe(600));
    }

    #[test]
    fn paper_ledger_snapshot_validates_flat_round_trip_and_closed_boundary() {
        let snapshot = valid_flat_snapshot();
        snapshot.validate().expect("valid paper round trip");
        assert!(snapshot.target_is_flat());
        assert_eq!(snapshot.target_position_qty(), Decimal::ZERO);
        assert!(snapshot.active_orders().is_empty());
    }

    #[test]
    fn paper_ledger_rejects_open_live_boundary() {
        let mut snapshot = valid_flat_snapshot();
        snapshot.safety_boundary.live_orders_enabled = true;
        assert_eq!(
            snapshot.validate(),
            Err(PaperLedgerInvariantError::LiveBoundaryOpen)
        );
    }

    #[test]
    fn paper_ledger_rejects_missing_order_reference() {
        let mut snapshot = valid_flat_snapshot();
        snapshot.trades[0].paper_order_id = PaperOrderId::new("PAPER_ORDER_MISSING");
        assert_eq!(
            snapshot.validate(),
            Err(PaperLedgerInvariantError::TradeReferencesMissingOrder)
        );
    }

    #[test]
    fn paper_ledger_rejects_duplicate_intent_id() {
        let mut snapshot = valid_flat_snapshot();
        snapshot.intents[1].intent_id = snapshot.intents[0].intent_id.clone();
        assert_eq!(
            snapshot.validate(),
            Err(PaperLedgerInvariantError::DuplicateIntentId)
        );
    }

    #[test]
    fn paper_order_status_classifies_active_and_terminal() {
        assert!(PaperOrderStatus::Working.is_active());
        assert!(PaperOrderStatus::PartiallyFilled.is_active());
        assert!(!PaperOrderStatus::Filled.is_active());
        assert!(PaperOrderStatus::Filled.is_terminal());
        assert!(PaperOrderStatus::Canceled.is_terminal());
        assert!(!PaperOrderStatus::Working.is_terminal());
    }

    #[test]
    fn paper_executor_fills_market_intent_on_next_final_bar_open() {
        let config = executor_config();
        let mut snapshot = empty_snapshot();
        let intent = PaperIntent::from_decision(
            PaperIntentKind::Enter,
            decision("decision-buy", OrderSide::Buy, 10),
        );
        let outcome = snapshot
            .apply_next_bar_open_market_intent(
                &config,
                intent,
                &runtime_bar(20, RuntimeBarOrigin::Live, 600),
            )
            .expect("market intent fills");

        let PaperLedgerExecutionOutcome::Filled {
            order,
            trade,
            position,
            ack,
        } = outcome
        else {
            panic!("expected filled outcome");
        };
        assert_eq!(order.status, PaperOrderStatus::Filled);
        assert_eq!(order.remaining_qty, Decimal::ZERO);
        assert_eq!(trade.price, Decimal::new(1020, 0));
        assert_eq!(position.qty, Decimal::ONE);
        assert_eq!(position.avg_price, Some(Decimal::new(1020, 0)));
        assert_eq!(ack.kind, PaperAckKind::Filled);
        assert_eq!(snapshot.orders.len(), 1);
        assert_eq!(snapshot.trades.len(), 1);
        assert_eq!(snapshot.positions.len(), 1);
        snapshot.validate().expect("snapshot remains valid");
    }

    #[test]
    fn paper_executor_round_trip_returns_target_flat() {
        let config = executor_config();
        let mut snapshot = empty_snapshot();
        snapshot
            .apply_next_bar_open_market_intent(
                &config,
                PaperIntent::from_decision(
                    PaperIntentKind::Enter,
                    decision("decision-buy", OrderSide::Buy, 10),
                ),
                &runtime_bar(20, RuntimeBarOrigin::Live, 600),
            )
            .expect("entry fills");
        snapshot
            .apply_next_bar_open_market_intent(
                &config,
                PaperIntent::from_decision(
                    PaperIntentKind::Exit,
                    decision("decision-sell", OrderSide::Sell, 20),
                ),
                &runtime_bar(30, RuntimeBarOrigin::Live, 600),
            )
            .expect("exit fills");

        assert!(snapshot.target_is_flat());
        assert_eq!(snapshot.target_position_qty(), Decimal::ZERO);
        assert_eq!(
            snapshot.positions.last().and_then(|pos| pos.avg_price),
            Some(Decimal::ZERO)
        );
        let runtime_state = snapshot.to_runtime_state(ts(30));
        assert_eq!(runtime_state.active_orders_count, 0);
        assert!(runtime_state.position.expect("position").qty.is_zero());
    }

    #[test]
    fn paper_executor_duplicate_decision_is_idempotent_and_does_not_append() {
        let config = executor_config();
        let mut snapshot = empty_snapshot();
        let intent = PaperIntent::from_decision(
            PaperIntentKind::Enter,
            decision("decision-buy", OrderSide::Buy, 10),
        );
        snapshot
            .apply_next_bar_open_market_intent(
                &config,
                intent.clone(),
                &runtime_bar(20, RuntimeBarOrigin::Live, 600),
            )
            .expect("first fill");
        let duplicate = snapshot
            .apply_next_bar_open_market_intent(
                &config,
                intent,
                &runtime_bar(30, RuntimeBarOrigin::Live, 600),
            )
            .expect("duplicate ignored");

        assert!(matches!(
            duplicate,
            PaperLedgerExecutionOutcome::DuplicateIgnored { .. }
        ));
        assert_eq!(snapshot.intents.len(), 1);
        assert_eq!(snapshot.orders.len(), 1);
        assert_eq!(snapshot.trades.len(), 1);
        assert_eq!(snapshot.acks.len(), 1);
    }

    #[test]
    fn paper_executor_rejects_non_live_bar_in_live_only_mode() {
        let config = executor_config();
        let mut snapshot = empty_snapshot();
        let intent = PaperIntent::from_decision(
            PaperIntentKind::Enter,
            decision("decision-buy", OrderSide::Buy, 10),
        );
        assert_eq!(
            snapshot.apply_next_bar_open_market_intent(
                &config,
                intent,
                &runtime_bar(20, RuntimeBarOrigin::HistoryGap, 600),
            ),
            Err(PaperLedgerExecutorError::FillBarNotEligible(
                RuntimeSuppressionReason::NonLiveOrigin
            ))
        );
    }

    #[test]
    fn paper_executor_history_sim_accepts_history_bar() {
        let config = PaperLedgerExecutorConfig::new(
            "hybrid_imoexf_synthetic",
            instrument(),
            PaperExecutionMode::HistorySim,
            600,
        );
        let mut snapshot = PaperLedgerSnapshot::empty(config.clone(), ts(0));
        let intent = PaperIntent::from_decision(
            PaperIntentKind::Enter,
            decision("decision-buy", OrderSide::Buy, 10),
        );
        snapshot
            .apply_next_bar_open_market_intent(
                &config,
                intent,
                &runtime_bar(20, RuntimeBarOrigin::History, 600),
            )
            .expect("history sim can fill from history");
        assert_eq!(
            snapshot.positions.last().expect("position").qty,
            Decimal::ONE
        );
    }

    #[test]
    fn paper_executor_rejects_wrong_timeframe_and_unsupported_order_type() {
        let config = executor_config();
        let mut snapshot = empty_snapshot();
        let intent = PaperIntent::from_decision(
            PaperIntentKind::Enter,
            decision("decision-buy", OrderSide::Buy, 10),
        );
        assert_eq!(
            snapshot.apply_next_bar_open_market_intent(
                &config,
                intent.clone(),
                &runtime_bar(20, RuntimeBarOrigin::Live, 60),
            ),
            Err(PaperLedgerExecutorError::FillBarNotEligible(
                RuntimeSuppressionReason::TimeframeMismatch {
                    expected_sec: 600,
                    actual_sec: 60
                }
            ))
        );

        let mut limit_intent = intent;
        limit_intent.order_type = Some(OrderType::Limit);
        assert_eq!(
            snapshot.apply_next_bar_open_market_intent(
                &config,
                limit_intent,
                &runtime_bar(20, RuntimeBarOrigin::Live, 600),
            ),
            Err(PaperLedgerExecutorError::UnsupportedOrderType(
                OrderType::Limit
            ))
        );
    }

    #[test]
    fn paper_runtime_bar_publisher_buffers_m1_until_complete_m10_and_publishes_runtime_input() {
        let mut publisher = paper_bar_publisher();
        for minute in 0..9 {
            assert!(matches!(
                publisher.observe_source_bar(m1_bar(
                    minute,
                    MarketDataSourceKind::LiveStream,
                    true
                )),
                PaperRuntimeBarPublishOutcome::Buffered { .. }
            ));
        }

        let outcome =
            publisher.observe_source_bar(m1_bar(9, MarketDataSourceKind::LiveStream, true));
        let PaperRuntimeBarPublishOutcome::Published {
            target_stream,
            runtime_input,
        } = outcome
        else {
            panic!("expected published m10 runtime input");
        };
        assert_eq!(target_stream, "finam_imoexf_paper:md:bars:10m");
        assert_eq!(
            runtime_input.source_stream,
            "finam_imoexf_paper:md:bars:10m"
        );
        assert_eq!(runtime_input.provenance, "FinamDerivedM1ToM10");
        assert_eq!(runtime_input.origin, RuntimeBarOrigin::Live);
        assert_eq!(runtime_input.timeframe_sec, 600);
        assert!(runtime_input.is_live_final_timeframe(600));
        assert_eq!(runtime_input.open, Decimal::new(1000, 0));
        assert_eq!(runtime_input.close, Decimal::new(1011, 0));
        assert_eq!(runtime_input.volume, Decimal::new(145, 0));
    }

    #[test]
    fn paper_runtime_bar_publisher_rejects_raw_non_final_non_live_and_native_m10_inputs() {
        let mut publisher = paper_bar_publisher();
        assert_eq!(
            publisher.observe_source_bar(m1_bar(0, MarketDataSourceKind::LiveStream, false)),
            PaperRuntimeBarPublishOutcome::Rejected {
                reason: PaperRuntimeBarPublishRejectReason::AggregationRejected(
                    BarAggregationRejectReason::NonFinalSourceBar
                )
            }
        );
        assert_eq!(
            publisher.observe_source_bar(m1_bar(0, MarketDataSourceKind::Recovery, true)),
            PaperRuntimeBarPublishOutcome::Rejected {
                reason: PaperRuntimeBarPublishRejectReason::NonLiveSourceKind {
                    actual: MarketDataSourceKind::Recovery
                }
            }
        );
        let mut native_m10 = m1_bar(0, MarketDataSourceKind::LiveStream, true);
        native_m10.timeframe_sec = 600;
        native_m10.close_ts = native_m10.open_ts + Duration::seconds(600);
        assert_eq!(
            publisher.observe_source_bar(native_m10),
            PaperRuntimeBarPublishOutcome::Rejected {
                reason: PaperRuntimeBarPublishRejectReason::SourceTimeframeMismatch {
                    expected_sec: 60,
                    actual_sec: 600
                }
            }
        );
    }

    #[test]
    fn paper_runtime_bar_publisher_drops_incomplete_bucket_on_gap() {
        let mut publisher = paper_bar_publisher();
        assert!(matches!(
            publisher.observe_source_bar(m1_bar(0, MarketDataSourceKind::LiveStream, true)),
            PaperRuntimeBarPublishOutcome::Buffered {
                buffered_count: 1,
                ..
            }
        ));
        assert_eq!(
            publisher.observe_source_bar(m1_bar(10, MarketDataSourceKind::LiveStream, true)),
            PaperRuntimeBarPublishOutcome::DroppedIncompleteBucket {
                bucket_open_ts: Utc.with_ymd_and_hms(2026, 1, 1, 9, 0, 0).single().unwrap(),
                buffered_count: 1,
            }
        );
    }

    #[test]
    fn paper_runtime_bar_publisher_rejects_open_safety_boundary() {
        let mut config = PaperRuntimeBarPublisherConfig::finam_m1_to_m10_paper(
            "hybrid_imoexf_synthetic",
            instrument(),
            "finam_imoexf_paper:ws:market_data",
            "finam_imoexf_paper:md:bars:10m",
        );
        config.safety_boundary.live_orders_enabled = true;
        assert_eq!(
            PaperRuntimeBarPublisher::new(config).expect_err("open boundary rejected"),
            PaperRuntimeBarPublishRejectReason::LiveBoundaryOpen
        );
    }

    #[test]
    fn paper_runtime_adapter_state_only_publishes_runtime_state_plan_without_strategy_invocation() {
        let mut adapter = paper_runtime_adapter();
        let outcome = adapter
            .observe_runtime_bar(runtime_bar(20, RuntimeBarOrigin::Live, 600), Vec::new())
            .expect("state-only runtime bar accepted");

        assert!(outcome.state_only);
        assert_eq!(outcome.filled_count, 0);
        assert_eq!(outcome.duplicate_count, 0);
        assert_eq!(outcome.publish_plan.len(), 1);
        assert_eq!(
            outcome.publish_plan[0].stream,
            "finam_imoexf_paper:runtime:state:hybrid_intraday:imoexf"
        );
        let PaperRuntimePublishPayload::RuntimeState(state) = &outcome.publish_plan[0].payload
        else {
            panic!("expected runtime state payload");
        };
        assert_eq!(state.strategy_id, "hybrid_imoexf_synthetic");
        assert!(state.position.is_none());
        assert_eq!(state.active_orders_count, 0);
        let hybrid = state.hybrid_intraday.as_ref().expect("hybrid projection");
        assert_eq!(hybrid.strategy_kind, "hybrid_intraday");
        assert_eq!(hybrid.last_position_qty, 0.0);
        assert_eq!(hybrid.last_bar_close, Some(1022.0));
        assert_eq!(hybrid.current_day_high, Some(1025.0));
        assert_eq!(hybrid.current_day_low, Some(1019.0));
        assert_eq!(hybrid.current_day_close, Some(1022.0));
        assert!(!hybrid.entry_ready);
        assert!(!hybrid.strategy_invocation_enabled);
        assert!(adapter.ledger().target_is_flat());
    }

    #[test]
    fn paper_runtime_adapter_applies_market_intent_and_returns_publish_plan() {
        let mut adapter = paper_runtime_adapter();
        let intent = PaperIntent::from_decision(
            PaperIntentKind::Enter,
            decision("decision-buy", OrderSide::Buy, 10),
        );
        let outcome = adapter
            .observe_runtime_bar(runtime_bar(20, RuntimeBarOrigin::Live, 600), vec![intent])
            .expect("paper intent applied");

        assert!(!outcome.state_only);
        assert_eq!(outcome.filled_count, 1);
        assert_eq!(outcome.duplicate_count, 0);
        assert_eq!(outcome.publish_plan.len(), 6);
        let streams: Vec<&str> = outcome
            .publish_plan
            .iter()
            .map(|record| record.stream.as_str())
            .collect();
        assert_eq!(
            streams,
            vec![
                "finam_imoexf_paper:runtime:intents",
                "finam_imoexf_paper:runtime:orders_paper_only",
                "finam_imoexf_paper:runtime:trades_paper_only",
                "finam_imoexf_paper:runtime:positions_paper_only",
                "finam_imoexf_paper:runtime:paper_acks",
                "finam_imoexf_paper:runtime:state:hybrid_intraday:imoexf",
            ]
        );
        assert_eq!(adapter.ledger().target_position_qty(), Decimal::ONE);
        let PaperRuntimePublishPayload::RuntimeState(state) =
            &outcome.publish_plan.last().expect("state").payload
        else {
            panic!("expected final runtime state");
        };
        assert_eq!(state.position.as_ref().expect("position").qty, Decimal::ONE);
        let hybrid = state.hybrid_intraday.as_ref().expect("hybrid projection");
        assert_eq!(hybrid.last_position_qty, 1.0);
        assert_eq!(hybrid.current_side.as_deref(), Some("long"));
        assert_eq!(hybrid.last_bar_close, Some(1022.0));
    }

    #[test]
    fn paper_runtime_adapter_shadow_invocation_marks_entry_ready_without_orders() {
        let mut config = PaperRuntimeAdapterConfig::finam_imoexf_paper(
            "hybrid_imoexf_synthetic",
            instrument(),
            PaperExecutionMode::LiveOnly,
        );
        config.hybrid_strategy_shadow = PaperHybridStrategyShadowConfig::enabled_shadow(1);
        let mut adapter = PaperRuntimeAdapter::new(config, ts(0)).expect("adapter");
        let outcome = adapter
            .observe_runtime_bar(runtime_bar(20, RuntimeBarOrigin::Live, 600), Vec::new())
            .expect("runtime bar accepted");
        assert!(outcome.state_only);
        assert_eq!(outcome.publish_plan.len(), 1);
        let PaperRuntimePublishPayload::RuntimeState(state) = &outcome.publish_plan[0].payload
        else {
            panic!("expected runtime state");
        };
        let hybrid = state.hybrid_intraday.as_ref().expect("hybrid projection");
        assert!(hybrid.strategy_invocation_enabled);
        assert!(hybrid.entry_ready);
        assert_eq!(
            hybrid.projection_source,
            "paper_hybrid_strategy_shadow_invocation"
        );
        assert!(hybrid.active_cycle_id.is_none());
        assert!(hybrid.current_owner.is_none());
        assert_eq!(state.active_orders_count, 0);
    }

    #[test]
    fn paper_runtime_adapter_applies_alor_oracle_seed_before_live_bar_projection() {
        let mut config = PaperRuntimeAdapterConfig::finam_imoexf_paper(
            "hybrid_imoexf_synthetic",
            instrument(),
            PaperExecutionMode::LiveOnly,
        );
        config.hybrid_strategy_shadow = PaperHybridStrategyShadowConfig::enabled_shadow(1);
        let mut adapter = PaperRuntimeAdapter::new(config, ts(0)).expect("adapter");
        adapter
            .apply_hybrid_intraday_oracle_seed(
                PaperHybridIntradayOracleSeed {
                    source: "alor_runtime_state".to_string(),
                    active_cycle_id: None,
                    next_cycle_seq: Some(18),
                    last_position_qty: Some(Decimal::ZERO),
                    current_owner: None,
                    current_side: None,
                    prev_day_close: Some(Decimal::new(2195, 0)),
                    prev_day_range: Some(Decimal::new(965, 1)),
                    prev_day_return: Some(Decimal::new(-1767733273663012, 17)),
                    day_before_close: Some(Decimal::new(2195, 0)),
                    current_day_utc: Some("2026-01-01".to_string()),
                    current_day_high: Some(Decimal::new(1010, 0)),
                    current_day_low: Some(Decimal::new(990, 0)),
                    current_day_close: Some(Decimal::new(1000, 0)),
                    was_long_today: Some(false),
                    was_short_today: Some(false),
                    risk_gate_shadow_session_date: Some("2026-01-01".to_string()),
                    risk_gate_shadow_pnl_points: Some(Decimal::ZERO),
                    risk_gate_shadow_trade_count: Some(0),
                    risk_gate_mr_enabled_current_session: Some(true),
                    risk_gate_mr_enabled_next_session: None,
                    risk_gate_rolling_sum_lb120: Some(Decimal::new(1586, 1)),
                    risk_gate_last_finalized_session_date: Some("2025-12-31".to_string()),
                    risk_gate_ledger_rows_count: Some(222),
                },
                ts(0),
            )
            .expect("seed accepted");

        let outcome = adapter
            .observe_runtime_bar(runtime_bar(20, RuntimeBarOrigin::Live, 600), Vec::new())
            .expect("runtime bar accepted");
        let PaperRuntimePublishPayload::RuntimeState(state) = &outcome.publish_plan[0].payload
        else {
            panic!("expected runtime state");
        };
        let hybrid = state.hybrid_intraday.as_ref().expect("hybrid projection");
        assert_eq!(hybrid.next_cycle_seq, 18);
        assert_eq!(hybrid.last_position_qty, 0.0);
        assert_eq!(hybrid.prev_day_close, Some(2195.0));
        assert_eq!(hybrid.prev_day_range, Some(96.5));
        assert_eq!(hybrid.prev_day_return, Some(-0.01767733273663012));
        assert_eq!(hybrid.day_before_close, Some(2195.0));
        assert_eq!(hybrid.current_day_high, Some(1025.0));
        assert_eq!(hybrid.current_day_low, Some(990.0));
        assert_eq!(hybrid.current_day_close, Some(1022.0));
        assert_eq!(
            hybrid.risk_gate_shadow_session_date.as_deref(),
            Some("2026-01-01")
        );
        assert_eq!(hybrid.risk_gate_mr_enabled_current_session, Some(true));
        assert_eq!(hybrid.risk_gate_rolling_sum_lb120, Some(158.6));
        assert_eq!(hybrid.risk_gate_ledger_rows_count, 222);
        assert!(hybrid.entry_ready);
        assert!(hybrid.strategy_invocation_enabled);
    }

    #[test]
    fn paper_runtime_adapter_shadow_projection_adopts_tagged_owner_cycle_after_fill() {
        let mut config = PaperRuntimeAdapterConfig::finam_imoexf_paper(
            "hybrid_imoexf_synthetic",
            instrument(),
            PaperExecutionMode::LiveOnly,
        );
        config.hybrid_strategy_shadow = PaperHybridStrategyShadowConfig::enabled_shadow(1);
        let mut adapter = PaperRuntimeAdapter::new(config, ts(0)).expect("adapter");
        let intent = PaperIntent::from_decision(
            PaperIntentKind::Enter,
            decision("decision-bo-short", OrderSide::Sell, 10),
        )
        .with_strategy_tags("intraday_breakout", "6a4badd811");

        let outcome = adapter
            .observe_runtime_bar(runtime_bar(20, RuntimeBarOrigin::Live, 600), vec![intent])
            .expect("runtime bar accepted");
        assert_eq!(outcome.filled_count, 1);
        let PaperRuntimePublishPayload::RuntimeState(state) =
            &outcome.publish_plan.last().expect("state").payload
        else {
            panic!("expected runtime state");
        };
        let hybrid = state.hybrid_intraday.as_ref().expect("hybrid projection");
        assert!(hybrid.strategy_invocation_enabled);
        assert!(hybrid.entry_ready);
        assert_eq!(hybrid.active_cycle_id.as_deref(), Some("6a4badd811"));
        assert_eq!(hybrid.current_owner.as_deref(), Some("intraday_breakout"));
        assert_eq!(hybrid.current_side.as_deref(), Some("short"));
        assert_eq!(hybrid.last_position_qty, -1.0);
        assert!(hybrid.pending_entry_request_id.is_none());
    }

    #[test]
    fn paper_runtime_adapter_duplicate_intent_publishes_duplicate_ack_and_no_second_fill() {
        let mut adapter = paper_runtime_adapter();
        let intent = PaperIntent::from_decision(
            PaperIntentKind::Enter,
            decision("decision-buy", OrderSide::Buy, 10),
        );
        adapter
            .observe_runtime_bar(
                runtime_bar(20, RuntimeBarOrigin::Live, 600),
                vec![intent.clone()],
            )
            .expect("first fill");
        let duplicate = adapter
            .observe_runtime_bar(runtime_bar(30, RuntimeBarOrigin::Live, 600), vec![intent])
            .expect("duplicate is non-fatal");

        assert_eq!(duplicate.filled_count, 0);
        assert_eq!(duplicate.duplicate_count, 1);
        assert_eq!(adapter.ledger().orders.len(), 1);
        assert_eq!(adapter.ledger().trades.len(), 1);
        assert_eq!(duplicate.publish_plan.len(), 2);
        let PaperRuntimePublishPayload::Ack(ack) = &duplicate.publish_plan[0].payload else {
            panic!("expected duplicate ack");
        };
        assert_eq!(ack.kind, PaperAckKind::DuplicateIgnored);
    }

    #[test]
    fn paper_runtime_adapter_rejects_bad_provenance_non_live_and_open_boundary() {
        let mut adapter = paper_runtime_adapter();
        let mut bad_provenance = runtime_bar(20, RuntimeBarOrigin::Live, 600);
        bad_provenance.provenance = "Unknown".to_string();
        assert_eq!(
            adapter.observe_runtime_bar(bad_provenance, Vec::new()),
            Err(PaperRuntimeAdapterError::ProvenanceMismatch {
                expected: "FinamDerivedM1ToM10".to_string(),
                actual: "Unknown".to_string()
            })
        );

        assert_eq!(
            adapter.observe_runtime_bar(
                runtime_bar(20, RuntimeBarOrigin::HistoryGap, 600),
                Vec::new()
            ),
            Err(PaperRuntimeAdapterError::RuntimeInputNotEligible)
        );

        let mut config = PaperRuntimeAdapterConfig::finam_imoexf_paper(
            "hybrid_imoexf_synthetic",
            instrument(),
            PaperExecutionMode::LiveOnly,
        );
        config.safety_boundary.live_orders_enabled = true;
        assert_eq!(
            PaperRuntimeAdapter::new(config, ts(0)).expect_err("open boundary rejected"),
            PaperRuntimeAdapterError::LiveBoundaryOpen
        );
    }

    #[test]
    fn paper_runtime_loop_buffers_without_sink_publish_until_m10_complete() {
        let mut runtime_loop = paper_runtime_loop();
        let mut sink = PaperRuntimeInMemorySink::new();

        for minute in 0..9 {
            let outcome = runtime_loop
                .observe_source_bar(
                    m1_bar(minute, MarketDataSourceKind::LiveStream, true),
                    Vec::new(),
                    &mut sink,
                )
                .expect("buffered");
            assert!(matches!(
                outcome,
                PaperRuntimeAdapterLoopOutcome::Buffered { .. }
            ));
            assert!(sink.records().is_empty());
        }
    }

    #[test]
    fn paper_runtime_loop_publishes_state_only_plan_on_complete_m10() {
        let mut runtime_loop = paper_runtime_loop();
        let mut sink = PaperRuntimeInMemorySink::new();
        for minute in 0..9 {
            runtime_loop
                .observe_source_bar(
                    m1_bar(minute, MarketDataSourceKind::LiveStream, true),
                    Vec::new(),
                    &mut sink,
                )
                .expect("buffered");
        }

        let outcome = runtime_loop
            .observe_source_bar(
                m1_bar(9, MarketDataSourceKind::LiveStream, true),
                Vec::new(),
                &mut sink,
            )
            .expect("published");
        let PaperRuntimeAdapterLoopOutcome::Published {
            publish_count,
            adapter_outcome,
        } = outcome
        else {
            panic!("expected published outcome");
        };
        assert_eq!(publish_count, 1);
        assert!(adapter_outcome.state_only);
        assert_eq!(sink.records().len(), 1);
        assert_eq!(
            sink.records()[0].stream,
            "finam_imoexf_paper:runtime:state:hybrid_intraday:imoexf"
        );
    }

    #[test]
    fn paper_runtime_loop_publishes_intent_order_trade_position_ack_and_state() {
        let mut runtime_loop = paper_runtime_loop();
        let mut sink = PaperRuntimeInMemorySink::new();
        for minute in 10..19 {
            runtime_loop
                .observe_source_bar(
                    m1_bar(minute, MarketDataSourceKind::LiveStream, true),
                    Vec::new(),
                    &mut sink,
                )
                .expect("buffered");
        }
        let intent = PaperIntent::from_decision(
            PaperIntentKind::Enter,
            decision("decision-buy", OrderSide::Buy, 10),
        );
        let outcome = runtime_loop
            .observe_source_bar(
                m1_bar(19, MarketDataSourceKind::LiveStream, true),
                vec![intent],
                &mut sink,
            )
            .expect("published intent path");
        let PaperRuntimeAdapterLoopOutcome::Published {
            publish_count,
            adapter_outcome,
        } = outcome
        else {
            panic!("expected published outcome");
        };
        assert_eq!(publish_count, 6);
        assert_eq!(adapter_outcome.filled_count, 1);
        assert_eq!(sink.records().len(), 6);
        assert_eq!(
            runtime_loop.adapter().ledger().target_position_qty(),
            Decimal::ONE
        );
    }

    #[test]
    fn paper_runtime_loop_returns_source_reject_without_sink_publish() {
        let mut runtime_loop = paper_runtime_loop();
        let mut sink = PaperRuntimeInMemorySink::new();
        let outcome = runtime_loop
            .observe_source_bar(
                m1_bar(0, MarketDataSourceKind::Recovery, true),
                Vec::new(),
                &mut sink,
            )
            .expect("source reject is loop outcome");
        assert!(matches!(
            outcome,
            PaperRuntimeAdapterLoopOutcome::SourceRejected {
                reason: PaperRuntimeBarPublishRejectReason::NonLiveSourceKind { .. }
            }
        ));
        assert!(sink.records().is_empty());
    }

    #[test]
    fn paper_runtime_loop_drops_incomplete_bucket_without_publish() {
        let mut runtime_loop = paper_runtime_loop();
        let mut sink = PaperRuntimeInMemorySink::new();
        runtime_loop
            .observe_source_bar(
                m1_bar(0, MarketDataSourceKind::LiveStream, true),
                Vec::new(),
                &mut sink,
            )
            .expect("first bar buffered");
        let outcome = runtime_loop
            .observe_source_bar(
                m1_bar(10, MarketDataSourceKind::LiveStream, true),
                Vec::new(),
                &mut sink,
            )
            .expect("gap outcome");
        assert!(matches!(
            outcome,
            PaperRuntimeAdapterLoopOutcome::DroppedIncompleteBucket { .. }
        ));
        assert!(sink.records().is_empty());
    }
}
