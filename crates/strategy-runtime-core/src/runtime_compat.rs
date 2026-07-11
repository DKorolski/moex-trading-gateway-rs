use std::collections::HashMap;

use broker_core::{BrokerOrderId, BrokerStopOrderId, StrategyRequestId};
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use crate::hybrid_intraday::{EntryStyle, Owner, ReasonCode, Side};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntentClass {
    Entry,
    Exit,
    CancelCleanup,
    ProtectiveRepair,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderSide {
    Buy,
    Sell,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopLimitCondition {
    More,
    Less,
    MoreOrEqual,
    LessOrEqual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AckStatus {
    Accepted,
    Confirmed,
    Rejected,
    Duplicate,
    Expired,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandAck {
    pub request_id: StrategyRequestId,
    pub status: AckStatus,
    pub broker_order_id: Option<BrokerOrderId>,
    pub error_code: Option<String>,
    pub error_msg: Option<String>,
    pub processed_ts_utc: i64,
}

impl CommandAck {
    fn at_now(
        request_id: StrategyRequestId,
        status: AckStatus,
        broker_order_id: Option<BrokerOrderId>,
        error_code: Option<String>,
        error_msg: Option<String>,
    ) -> Self {
        Self {
            request_id,
            status,
            broker_order_id,
            error_code,
            error_msg,
            processed_ts_utc: chrono::Utc::now().timestamp(),
        }
    }

    pub fn confirmed(
        request_id: StrategyRequestId,
        broker_order_id: Option<BrokerOrderId>,
    ) -> Self {
        Self::at_now(
            request_id,
            AckStatus::Confirmed,
            broker_order_id,
            None,
            None,
        )
    }

    pub fn accepted(request_id: StrategyRequestId) -> Self {
        Self::at_now(request_id, AckStatus::Accepted, None, None, None)
    }

    pub fn duplicate(request_id: StrategyRequestId) -> Self {
        Self::at_now(request_id, AckStatus::Duplicate, None, None, None)
    }

    pub fn rejected(
        request_id: StrategyRequestId,
        error_code: impl Into<String>,
        error_msg: impl Into<String>,
    ) -> Self {
        Self::at_now(
            request_id,
            AckStatus::Rejected,
            None,
            Some(error_code.into()),
            Some(error_msg.into()),
        )
    }

    pub fn expired(request_id: StrategyRequestId, error_msg: impl Into<String>) -> Self {
        Self::at_now(
            request_id,
            AckStatus::Expired,
            None,
            Some("expired".to_string()),
            Some(error_msg.into()),
        )
    }

    pub fn error(
        request_id: StrategyRequestId,
        error_code: impl Into<String>,
        error_msg: impl Into<String>,
    ) -> Self {
        Self::at_now(
            request_id,
            AckStatus::Error,
            None,
            Some(error_code.into()),
            Some(error_msg.into()),
        )
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TradeMode {
    Live,
    Paper,
    Backtest,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PaperExecutionMode {
    LiveOnly,
    HistorySim,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum GatewayPhase {
    #[default]
    SyncingHistory,
    Reconnecting,
    SyncingGap,
    LiveReady,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MarketBuyAndCloseLiveOrderStyle {
    #[default]
    Market,
    MarketableLimit,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Intent {
    Classified {
        intent: Box<Intent>,
        intent_class: IntentClass,
    },
    Routed {
        intent: Box<Intent>,
        symbol: String,
    },
    Place {
        price: f64,
        qty: f64,
        side: OrderSide,
        comment: Option<String>,
    },
    Market {
        qty: f64,
        side: OrderSide,
        fill_price: Option<f64>,
        comment: Option<String>,
    },
    Cancel {
        order_id: BrokerOrderId,
    },
    Replace {
        order_id: BrokerOrderId,
        new_price: f64,
        new_qty: f64,
    },
    CreateStopLimit {
        side: OrderSide,
        qty: f64,
        trigger_price: f64,
        price: f64,
        condition: StopLimitCondition,
        stop_end_unix_time: i64,
        comment: Option<String>,
        instrument_group: Option<String>,
        check_duplicates: Option<bool>,
    },
    DeleteStopLimit {
        order_id: BrokerStopOrderId,
        side: Option<OrderSide>,
        check_duplicates: Option<bool>,
    },
}

impl Intent {
    pub fn with_class(self, intent_class: IntentClass) -> Self {
        Self::Classified {
            intent: Box::new(self),
            intent_class,
        }
    }

    pub fn with_symbol(self, symbol: impl Into<String>) -> Self {
        Self::Routed {
            intent: Box::new(self),
            symbol: symbol.into(),
        }
    }

    pub fn explicit_class(&self) -> Option<IntentClass> {
        match self {
            Self::Classified { intent_class, .. } => Some(*intent_class),
            Self::Routed { intent, .. } => intent.explicit_class(),
            _ => None,
        }
    }

    pub fn base_intent(&self) -> &Intent {
        match self {
            Self::Classified { intent, .. } | Self::Routed { intent, .. } => intent.base_intent(),
            _ => self,
        }
    }
}

#[derive(Debug, Clone)]
pub struct StrategyCtx {
    pub strategy_id: String,
    pub portfolio: String,
    pub exchange: String,
    pub symbol: String,
    pub tick_size: f64,
    pub trade_mode: TradeMode,
    pub paper_execution_mode: PaperExecutionMode,
    pub allow_live_orders: bool,
    pub gateway_phase: GatewayPhase,
    pub position_qty: Option<f64>,
    pub event_ts_utc: i64,
    pub now_ts_utc: i64,
    pub last_bar_ts: Option<i64>,
}

impl StrategyCtx {
    pub fn event_ts_utc(&self) -> i64 {
        self.event_ts_utc
    }

    pub fn now_ts_utc(&self) -> i64 {
        self.now_ts_utc
    }

    pub fn last_bar_ts(&self) -> Option<i64> {
        self.last_bar_ts
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BarEvent {
    pub symbol: String,
    pub close_time_utc: i64,
    #[serde(default, alias = "c")]
    pub close: f64,
    #[serde(default)]
    pub o: f64,
    #[serde(default)]
    pub h: f64,
    #[serde(default)]
    pub l: f64,
    #[serde(default)]
    pub v: f64,
    pub origin: DataOrigin,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DataOrigin {
    History,
    HistoryGap,
    Live,
    Replay,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OrderEvent {
    pub order_id: BrokerOrderId,
    pub request_id: Option<StrategyRequestId>,
    #[serde(default)]
    pub symbol: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub side: String,
    #[serde(default)]
    pub order_type: String,
    #[serde(default)]
    pub qty: f64,
    #[serde(default)]
    pub filled: f64,
    #[serde(default)]
    pub price: f64,
    #[serde(default)]
    pub existing: bool,
    #[serde(default)]
    pub comment: Option<String>,
    #[serde(default)]
    pub ts_utc: i64,
}

impl Default for OrderEvent {
    fn default() -> Self {
        Self {
            order_id: BrokerOrderId::new("UNSET"),
            request_id: None,
            symbol: String::new(),
            status: String::new(),
            side: String::new(),
            order_type: String::new(),
            qty: 0.0,
            filled: 0.0,
            price: 0.0,
            existing: false,
            comment: None,
            ts_utc: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StopOrderEvent {
    pub stop_order_id: BrokerStopOrderId,
    #[serde(default)]
    pub exchange_order_id: Option<BrokerOrderId>,
    #[serde(default)]
    pub symbol: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub side: String,
    #[serde(default)]
    pub qty: f64,
    #[serde(default)]
    pub filled: f64,
    #[serde(default)]
    pub stop_price: f64,
    #[serde(default)]
    pub price: f64,
    #[serde(default)]
    pub existing: bool,
    #[serde(default)]
    pub comment: Option<String>,
    #[serde(default)]
    pub end_time: Option<i64>,
    #[serde(default)]
    pub ts_utc: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PositionEvent {
    pub symbol: String,
    pub qty: f64,
    #[serde(default)]
    pub existing: bool,
    #[serde(default)]
    pub avg_price: f64,
    #[serde(default)]
    pub ts_utc: i64,
}

#[derive(Debug, Clone)]
pub struct BootstrapSnapshot {
    pub positions_strategy: HashMap<String, PositionEvent>,
    pub working_orders_strategy: HashMap<BrokerOrderId, OrderEvent>,
    pub working_stop_orders_strategy: HashMap<BrokerStopOrderId, StopOrderEvent>,
    pub snapshot_ts_utc: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct RuntimeStateRestored {
    pub known_order_ids: Vec<BrokerOrderId>,
    pub pending_requests: Vec<StrategyRequestId>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RiskGateSessionFinalization {
    pub session_date: NaiveDate,
    pub shadow_pnl_points: f64,
    pub shadow_trade_count: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RiskGateRuntimeState {
    pub profile_id: String,
    pub last_finalized_session_date: Option<NaiveDate>,
    pub rolling_sum_lb120: Option<f64>,
    pub mr_enabled_current_session: Option<bool>,
    pub mr_enabled_next_session: Option<bool>,
    pub ledger_rows_count: usize,
}

#[derive(Debug, Clone, Default)]
pub struct StrategyExitRiskStatus {
    pub phase_override: Option<String>,
    pub exit_recovery_active: bool,
    pub operator_intervention_required: bool,
    pub open_risk_position_unflattened: bool,
}

pub trait Strategy: Send + Sync {
    fn on_bar(&mut self, ctx: &StrategyCtx, bar: &BarEvent) -> Vec<Intent>;
    fn on_ack(&mut self, ctx: &StrategyCtx, ack: &CommandAck) -> Vec<Intent>;
    fn on_order(&mut self, ctx: &StrategyCtx, order: &OrderEvent) -> Vec<Intent>;
    fn on_stop_order(&mut self, ctx: &StrategyCtx, order: &StopOrderEvent) -> Vec<Intent>;
    fn on_position(&mut self, ctx: &StrategyCtx, position: &PositionEvent) -> Vec<Intent>;
    fn on_timer(&mut self, ctx: &StrategyCtx, now_ts_utc_ms: i64) -> Vec<Intent>;
    fn on_bootstrap_snapshot(
        &mut self,
        ctx: &StrategyCtx,
        snapshot: &BootstrapSnapshot,
    ) -> Vec<Intent>;
    fn on_runtime_state_restored(
        &mut self,
        ctx: &StrategyCtx,
        state: &RuntimeStateRestored,
    ) -> Vec<Intent>;
    fn risk_gate_session_finalizations(&self) -> Vec<RiskGateSessionFinalization>;
    fn acknowledge_risk_gate_session_finalizations(&mut self, session_dates: &[NaiveDate]);
    fn on_risk_gate_state(&mut self, state: &RiskGateRuntimeState);
    fn warmup_from_history(&mut self, ctx: &StrategyCtx, bars: &[BarEvent]) -> usize;
    fn intent_comment_tag(
        &self,
        ctx: &StrategyCtx,
        created_ts_utc: i64,
        intent_class: IntentClass,
    ) -> Option<String>;
    fn state(&self) -> &StrategyState;
    fn set_state(&mut self, state: StrategyState);
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub enum StrategyState {
    #[default]
    Idle,
    HybridIntradayRuntime {
        #[serde(default)]
        active_cycle_id: Option<String>,
        #[serde(default)]
        next_cycle_seq: u32,
        #[serde(default)]
        last_position_qty: f64,
        #[serde(default)]
        current_owner: Option<Owner>,
        #[serde(default)]
        current_side: Option<Side>,
        #[serde(default)]
        pending_entry_owner: Option<Owner>,
        #[serde(default)]
        pending_entry_side: Option<Side>,
        #[serde(default)]
        pending_entry_cycle_id: Option<String>,
        #[serde(default)]
        pending_entry_request_id: Option<StrategyRequestId>,
        #[serde(default)]
        pending_entry_created_ts_utc: Option<i64>,
        #[serde(default)]
        deferred_entry_owner: Option<Owner>,
        #[serde(default)]
        deferred_entry_side: Option<Side>,
        #[serde(default)]
        deferred_entry_cycle_id: Option<String>,
        #[serde(default)]
        deferred_entry_entry_style: Option<EntryStyle>,
        #[serde(default)]
        deferred_entry_reason: Option<ReasonCode>,
        #[serde(default)]
        deferred_entry_stop_price: Option<f64>,
        #[serde(default)]
        deferred_entry_take_price: Option<f64>,
        #[serde(default)]
        deferred_entry_ts_utc: Option<i64>,
        #[serde(default)]
        deferred_entry_request_id: Option<StrategyRequestId>,
        #[serde(default)]
        pending_exit_request_id: Option<StrategyRequestId>,
        #[serde(default)]
        pending_exit_created_ts_utc: Option<i64>,
        #[serde(default)]
        deferred_exit_owner: Option<Owner>,
        #[serde(default)]
        deferred_exit_reason: Option<ReasonCode>,
        #[serde(default)]
        deferred_exit_cycle_id: Option<String>,
        #[serde(default)]
        deferred_exit_ts_utc: Option<i64>,
        #[serde(default)]
        deferred_exit_request_id: Option<StrategyRequestId>,
        #[serde(default)]
        pending_tp_request_id: Option<StrategyRequestId>,
        #[serde(default)]
        pending_tp_created_ts_utc: Option<i64>,
        #[serde(default)]
        pending_sl_request_id: Option<StrategyRequestId>,
        #[serde(default)]
        pending_sl_created_ts_utc: Option<i64>,
        #[serde(default)]
        tp_order_id: Option<BrokerOrderId>,
        #[serde(default)]
        sl_stop_order_id: Option<BrokerStopOrderId>,
        #[serde(default)]
        sl_exchange_order_id: Option<BrokerOrderId>,
        #[serde(default)]
        sl_triggered_ts: Option<i64>,
        #[serde(default)]
        mr_take_price: Option<f64>,
        #[serde(default)]
        mr_stop_price: Option<f64>,
        #[serde(default)]
        repair_deadline_ts: Option<i64>,
        #[serde(default)]
        next_repair_at_ts: Option<i64>,
        #[serde(default)]
        repair_backoff_level: u32,
        #[serde(default)]
        repair_attempts: u32,
        #[serde(default)]
        safe_mode_close_only: bool,
        #[serde(default)]
        safe_mode_reason: Option<String>,
        #[serde(default)]
        entry_ready: bool,
        #[serde(default)]
        last_bar_close: Option<f64>,
        #[serde(default)]
        prev_day_close: Option<f64>,
        #[serde(default)]
        last_day_local: Option<String>,
        #[serde(default)]
        current_day_high: Option<f64>,
        #[serde(default)]
        current_day_low: Option<f64>,
        #[serde(default)]
        current_day_close: Option<f64>,
        #[serde(default)]
        prev_day_range: Option<f64>,
        #[serde(default)]
        prev_day_return: Option<f64>,
        #[serde(default)]
        day_before_close: Option<f64>,
        #[serde(default)]
        today_start_local: Option<String>,
        #[serde(default)]
        was_long_today: bool,
        #[serde(default)]
        was_short_today: bool,
        #[serde(default)]
        overnight_exit_armed_date: Option<String>,
        #[serde(default)]
        risk_gate_shadow_session_date: Option<String>,
        #[serde(default)]
        risk_gate_shadow_pnl_points: f64,
        #[serde(default)]
        risk_gate_shadow_trade_count: u32,
        #[serde(default)]
        risk_gate_shadow_entry_ts_utc: Option<i64>,
        #[serde(default)]
        risk_gate_shadow_entry_price: Option<f64>,
        #[serde(default)]
        risk_gate_shadow_side: Option<Side>,
        #[serde(default)]
        risk_gate_shadow_target_price: Option<f64>,
        #[serde(default)]
        risk_gate_shadow_stop_price: Option<f64>,
        #[serde(default)]
        risk_gate_pending_session_date: Option<String>,
        #[serde(default)]
        risk_gate_pending_shadow_pnl_points: f64,
        #[serde(default)]
        risk_gate_pending_shadow_trade_count: u32,
        #[serde(default)]
        risk_gate_mr_enabled_current_session: Option<bool>,
        #[serde(default)]
        risk_gate_rolling_sum_lb120: Option<f64>,
        #[serde(default)]
        risk_gate_last_finalized_session_date: Option<String>,
        #[serde(default)]
        risk_gate_ledger_rows_count: usize,
    },
}
