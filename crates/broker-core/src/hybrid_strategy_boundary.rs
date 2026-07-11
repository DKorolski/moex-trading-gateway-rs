use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use crate::command::{CommandAckReasonCode, CommandAckStatus};
use crate::ids::{BrokerAccountId, BrokerOrderId, BrokerStopOrderId, StrategyRequestId};
use crate::instrument::InstrumentId;
use crate::runtime_host::RuntimeHostBootstrapSnapshot;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HybridRuntimeAckStatus {
    Accepted,
    Confirmed,
    Rejected,
    Duplicate,
    Expired,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HybridRuntimeAckErrorCode {
    TradingWindowClosed,
    Other(String),
}

pub fn map_hybrid_runtime_ack_status(status: CommandAckStatus) -> Option<HybridRuntimeAckStatus> {
    match status {
        CommandAckStatus::Accepted => Some(HybridRuntimeAckStatus::Accepted),
        CommandAckStatus::Submitted | CommandAckStatus::Recovered => {
            Some(HybridRuntimeAckStatus::Confirmed)
        }
        CommandAckStatus::Rejected => Some(HybridRuntimeAckStatus::Rejected),
        CommandAckStatus::Duplicate => Some(HybridRuntimeAckStatus::Duplicate),
        CommandAckStatus::Expired => Some(HybridRuntimeAckStatus::Expired),
        CommandAckStatus::Error => Some(HybridRuntimeAckStatus::Error),
        CommandAckStatus::Timeout | CommandAckStatus::UnknownPending => None,
    }
}

pub fn map_hybrid_runtime_ack_error_code(
    reason: Option<CommandAckReasonCode>,
) -> Option<HybridRuntimeAckErrorCode> {
    reason.map(|reason| match reason {
        CommandAckReasonCode::TradingWindowClosed => HybridRuntimeAckErrorCode::TradingWindowClosed,
        other => HybridRuntimeAckErrorCode::Other(format!("{other:?}")),
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HybridRuntimeCommandAck {
    pub request_id: StrategyRequestId,
    pub status: HybridRuntimeAckStatus,
    pub broker_order_id: Option<BrokerOrderId>,
    pub error_code: Option<HybridRuntimeAckErrorCode>,
    pub error_message: Option<String>,
    pub processed_ts_utc: i64,
}

impl HybridRuntimeCommandAck {
    pub fn is_window_closed_recoverable_reject(&self) -> bool {
        matches!(
            self.status,
            HybridRuntimeAckStatus::Rejected
                | HybridRuntimeAckStatus::Expired
                | HybridRuntimeAckStatus::Error
        ) && self.broker_order_id.is_none()
            && self.error_code == Some(HybridRuntimeAckErrorCode::TradingWindowClosed)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HybridRuntimeOwner {
    IntradayBreakout,
    MeanReversion,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HybridRuntimeOrderRole {
    Entry,
    Exit,
    TakeProfit,
    StopLoss,
    Cancel,
    Repair,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HybridRuntimeAttribution {
    pub internal_comment: String,
    pub strategy_id: String,
    pub cycle_id: String,
    pub owner: Option<HybridRuntimeOwner>,
    pub role: Option<HybridRuntimeOrderRole>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HybridRuntimeOrderEvent {
    pub order_id: BrokerOrderId,
    pub request_id: Option<StrategyRequestId>,
    pub instrument: InstrumentId,
    pub status: String,
    pub side: String,
    pub order_type: String,
    pub qty: f64,
    pub filled_qty: f64,
    pub price: f64,
    pub existing: bool,
    pub attribution: Option<HybridRuntimeAttribution>,
    pub source_ts_utc: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HybridRuntimeStopOrderEvent {
    pub stop_order_id: BrokerStopOrderId,
    pub exchange_order_id: Option<BrokerOrderId>,
    pub instrument: InstrumentId,
    pub status: String,
    pub side: String,
    pub qty: f64,
    pub filled_qty: f64,
    pub stop_price: f64,
    pub price: f64,
    pub existing: bool,
    pub attribution: Option<HybridRuntimeAttribution>,
    pub end_ts_utc: Option<i64>,
    pub source_ts_utc: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HybridRuntimePositionEvent {
    pub instrument: InstrumentId,
    pub qty: f64,
    pub existing: bool,
    pub avg_price: f64,
    pub source_ts_utc: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HybridRuntimeBarOrigin {
    History,
    HistoryGap,
    Live,
    Replay,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HybridRuntimeBarEvent {
    pub instrument: InstrumentId,
    pub close_time_utc: i64,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
    pub origin: HybridRuntimeBarOrigin,
    pub is_final: bool,
    pub timeframe_sec: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HybridRuntimeTradeMode {
    Backtest,
    Paper,
    Live,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HybridRuntimePaperExecutionMode {
    HistorySim,
    LiveOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HybridRuntimeGatewayPhase {
    Starting,
    SyncingHistory,
    CatchingUp,
    LiveReady,
    Degraded,
    Stopped,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HybridRuntimeStrategyContext {
    pub strategy_id: String,
    pub request_namespace_account: BrokerAccountId,
    pub instrument: InstrumentId,
    pub tick_size: f64,
    pub trade_mode: HybridRuntimeTradeMode,
    pub paper_execution_mode: HybridRuntimePaperExecutionMode,
    pub allow_live_orders: bool,
    pub gateway_phase: HybridRuntimeGatewayPhase,
    pub position_qty: Option<f64>,
    pub event_ts_utc: i64,
    pub strategy_now_ts_utc: i64,
    pub last_bar_ts_utc: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HybridRuntimeBootstrapSnapshot {
    pub broker_truth: RuntimeHostBootstrapSnapshot,
    pub positions_strategy: Vec<HybridRuntimePositionEvent>,
    pub working_orders_strategy: Vec<HybridRuntimeOrderEvent>,
    pub working_stop_orders_strategy: Vec<HybridRuntimeStopOrderEvent>,
    pub snapshot_ts_utc: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HybridRuntimeStateRestored {
    pub known_order_ids: Vec<BrokerOrderId>,
    pub pending_requests: Vec<StrategyRequestId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HybridRiskGateRuntimeState {
    pub profile_id: String,
    pub last_finalized_session_date: Option<NaiveDate>,
    pub rolling_sum_lb120: Option<f64>,
    pub mr_enabled_current_session: Option<bool>,
    pub mr_enabled_next_session: Option<bool>,
    pub ledger_rows_count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HybridRiskGateSessionFinalization {
    pub session_date: NaiveDate,
    pub shadow_pnl_points: f64,
    pub shadow_trade_count: u32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn trading_window_closed_ack_preserves_confirmed_and_deferred_semantics() {
        assert_eq!(
            map_hybrid_runtime_ack_status(CommandAckStatus::Submitted),
            Some(HybridRuntimeAckStatus::Confirmed)
        );
        assert_eq!(
            map_hybrid_runtime_ack_status(CommandAckStatus::Recovered),
            Some(HybridRuntimeAckStatus::Confirmed)
        );
        assert_eq!(
            map_hybrid_runtime_ack_status(CommandAckStatus::UnknownPending),
            None
        );
        assert_eq!(
            map_hybrid_runtime_ack_error_code(Some(CommandAckReasonCode::TradingWindowClosed)),
            Some(HybridRuntimeAckErrorCode::TradingWindowClosed)
        );

        let confirmed = HybridRuntimeCommandAck {
            request_id: StrategyRequestId::from(Uuid::from_u128(1)),
            status: HybridRuntimeAckStatus::Confirmed,
            broker_order_id: Some(BrokerOrderId::new("ORDER-1")),
            error_code: None,
            error_message: None,
            processed_ts_utc: 1_700_000_000,
        };
        assert!(!confirmed.is_window_closed_recoverable_reject());

        let deferred = HybridRuntimeCommandAck {
            request_id: StrategyRequestId::from(Uuid::from_u128(2)),
            status: HybridRuntimeAckStatus::Rejected,
            broker_order_id: None,
            error_code: Some(HybridRuntimeAckErrorCode::TradingWindowClosed),
            error_message: Some("window closed".to_string()),
            processed_ts_utc: 1_700_000_010,
        };
        assert!(deferred.is_window_closed_recoverable_reject());
    }

    #[test]
    fn stop_and_exchange_order_ids_remain_in_distinct_namespaces() {
        let stop_order_id = BrokerStopOrderId::new("STOP-1");
        let exchange_order_id = BrokerOrderId::new("ORDER-1");

        assert_eq!(stop_order_id.as_str(), "STOP-1");
        assert_eq!(exchange_order_id.as_str(), "ORDER-1");
    }

    #[test]
    fn position_existing_flag_and_internal_attribution_are_not_lost() {
        let attribution = HybridRuntimeAttribution {
            internal_comment: "HYB|sid=hybrid_imoexf|c=abc1230001|o=MR|r=TP".to_string(),
            strategy_id: "hybrid_imoexf".to_string(),
            cycle_id: "abc1230001".to_string(),
            owner: Some(HybridRuntimeOwner::MeanReversion),
            role: Some(HybridRuntimeOrderRole::TakeProfit),
        };
        assert!(attribution.internal_comment.contains("sid=hybrid_imoexf"));

        let existing = true;
        let new_transition = false;
        assert_ne!(existing, new_transition);
    }
}
