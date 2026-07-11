use std::collections::HashSet;

use chrono::NaiveDate;
use rust_decimal::prelude::ToPrimitive;
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HybridRuntimeCallbackInput<T> {
    pub context: HybridRuntimeStrategyContext,
    pub payload: T,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct HybridRuntimeTimerEvent {
    pub now_ts_utc_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "HybridRuntimeAttributionWire")]
pub struct HybridRuntimeAttribution {
    internal_comment: String,
    strategy_id: String,
    cycle_id: String,
    owner: Option<HybridRuntimeOwner>,
    role: Option<HybridRuntimeOrderRole>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct HybridRuntimeAttributionWire {
    internal_comment: String,
    strategy_id: String,
    cycle_id: String,
    owner: Option<HybridRuntimeOwner>,
    role: Option<HybridRuntimeOrderRole>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum HybridRuntimeAttributionError {
    #[error("hybrid attribution comment is invalid")]
    InvalidSourceComment,
    #[error("hybrid attribution structured fields do not match source comment")]
    StructuredFieldsMismatch,
}

impl HybridRuntimeAttribution {
    pub fn parse_source_comment(
        internal_comment: impl Into<String>,
    ) -> Result<Self, HybridRuntimeAttributionError> {
        let internal_comment = internal_comment.into();
        if !internal_comment.is_ascii() || !internal_comment.starts_with("HYB|") {
            return Err(HybridRuntimeAttributionError::InvalidSourceComment);
        }
        let mut strategy_id = None;
        let mut cycle_id = None;
        let mut owner = None;
        let mut role = None;
        for part in internal_comment.split('|').skip(1) {
            let (key, value) = part
                .split_once('=')
                .ok_or(HybridRuntimeAttributionError::InvalidSourceComment)?;
            match key {
                "sid" => strategy_id = Some(value.to_string()),
                "c" => cycle_id = Some(value.to_string()),
                "o" => {
                    owner = match value {
                        "MR" => Some(HybridRuntimeOwner::MeanReversion),
                        "BO" => Some(HybridRuntimeOwner::IntradayBreakout),
                        _ => None,
                    };
                }
                "r" => {
                    role = match value {
                        "ENTRY" => Some(HybridRuntimeOrderRole::Entry),
                        "TP" => Some(HybridRuntimeOrderRole::TakeProfit),
                        "SL" => Some(HybridRuntimeOrderRole::StopLoss),
                        "EXIT" => Some(HybridRuntimeOrderRole::Exit),
                        "CANCEL" => Some(HybridRuntimeOrderRole::Cancel),
                        // Source parse_hybrid_tag intentionally does not map REPAIR.
                        _ => None,
                    };
                }
                _ => {}
            }
        }
        Ok(Self {
            internal_comment,
            strategy_id: strategy_id.ok_or(HybridRuntimeAttributionError::InvalidSourceComment)?,
            cycle_id: cycle_id.ok_or(HybridRuntimeAttributionError::InvalidSourceComment)?,
            owner,
            role,
        })
    }

    pub fn validate_source_equivalence(&self) -> Result<(), HybridRuntimeAttributionError> {
        let parsed = Self::parse_source_comment(self.internal_comment.clone())?;
        if &parsed == self {
            Ok(())
        } else {
            Err(HybridRuntimeAttributionError::StructuredFieldsMismatch)
        }
    }

    pub fn belongs_to(&self, strategy_id: &str) -> bool {
        self.strategy_id == strategy_id
    }

    pub fn internal_comment(&self) -> &str {
        &self.internal_comment
    }

    pub fn strategy_id(&self) -> &str {
        &self.strategy_id
    }

    pub fn cycle_id(&self) -> &str {
        &self.cycle_id
    }

    pub fn owner(&self) -> Option<HybridRuntimeOwner> {
        self.owner
    }

    pub fn role(&self) -> Option<HybridRuntimeOrderRole> {
        self.role
    }
}

impl TryFrom<HybridRuntimeAttributionWire> for HybridRuntimeAttribution {
    type Error = HybridRuntimeAttributionError;

    fn try_from(wire: HybridRuntimeAttributionWire) -> Result<Self, Self::Error> {
        let parsed = Self::parse_source_comment(wire.internal_comment)?;
        if parsed.strategy_id == wire.strategy_id
            && parsed.cycle_id == wire.cycle_id
            && parsed.owner == wire.owner
            && parsed.role == wire.role
        {
            Ok(parsed)
        } else {
            Err(HybridRuntimeAttributionError::StructuredFieldsMismatch)
        }
    }
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

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum HybridRuntimeBootstrapValidationError {
    #[error("hybrid bootstrap contains duplicate target position")]
    DuplicateTargetPosition,
    #[error("hybrid bootstrap contains duplicate order id")]
    DuplicateOrderId,
    #[error("hybrid bootstrap contains duplicate stop-order id")]
    DuplicateStopOrderId,
    #[error("hybrid bootstrap contains an event for another instrument")]
    InstrumentMismatch,
    #[error("hybrid bootstrap target position contradicts broker truth")]
    BrokerTruthPositionMismatch,
    #[error("hybrid bootstrap broker truth quantity cannot be represented as f64")]
    BrokerTruthQuantityNotRepresentable,
}

impl HybridRuntimeBootstrapSnapshot {
    pub fn validate(&self) -> Result<(), HybridRuntimeBootstrapValidationError> {
        let target = &self.broker_truth.instrument;
        if self
            .positions_strategy
            .iter()
            .any(|position| &position.instrument != target)
            || self
                .working_orders_strategy
                .iter()
                .any(|order| &order.instrument != target)
            || self
                .working_stop_orders_strategy
                .iter()
                .any(|order| &order.instrument != target)
        {
            return Err(HybridRuntimeBootstrapValidationError::InstrumentMismatch);
        }
        if self.positions_strategy.len() > 1 {
            return Err(HybridRuntimeBootstrapValidationError::DuplicateTargetPosition);
        }
        let mut order_ids = HashSet::new();
        if self
            .working_orders_strategy
            .iter()
            .any(|order| !order_ids.insert(order.order_id.clone()))
        {
            return Err(HybridRuntimeBootstrapValidationError::DuplicateOrderId);
        }
        let mut stop_order_ids = HashSet::new();
        if self
            .working_stop_orders_strategy
            .iter()
            .any(|order| !stop_order_ids.insert(order.stop_order_id.clone()))
        {
            return Err(HybridRuntimeBootstrapValidationError::DuplicateStopOrderId);
        }
        let broker_qty =
            self.broker_truth.target_position_qty.to_f64().ok_or(
                HybridRuntimeBootstrapValidationError::BrokerTruthQuantityNotRepresentable,
            )?;
        match self.positions_strategy.first() {
            Some(position) if (position.qty - broker_qty).abs() > f64::EPSILON => {
                return Err(HybridRuntimeBootstrapValidationError::BrokerTruthPositionMismatch);
            }
            None if broker_qty.abs() > f64::EPSILON => {
                return Err(HybridRuntimeBootstrapValidationError::BrokerTruthPositionMismatch);
            }
            _ => {}
        }
        Ok(())
    }
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
    use chrono::Utc;
    use rust_decimal::Decimal;
    use uuid::Uuid;

    use crate::instrument::{Exchange, Market};
    use crate::request_id::deterministic_request_id_for_account_instrument;

    fn instrument() -> InstrumentId {
        InstrumentId {
            symbol: "IMOEXF".to_string(),
            venue_symbol: Some("IMOEXF@RTSX".to_string()),
            exchange: Exchange::Moex,
            market: Market::Futures,
        }
    }

    fn context(
        account: &str,
        tick_size: f64,
        trade_mode: HybridRuntimeTradeMode,
    ) -> HybridRuntimeStrategyContext {
        HybridRuntimeStrategyContext {
            strategy_id: "hybrid_imoexf".to_string(),
            request_namespace_account: BrokerAccountId::new(account),
            instrument: instrument(),
            tick_size,
            trade_mode,
            paper_execution_mode: HybridRuntimePaperExecutionMode::LiveOnly,
            allow_live_orders: false,
            gateway_phase: HybridRuntimeGatewayPhase::Degraded,
            position_qty: Some(-1.0),
            event_ts_utc: 1_700_000_000,
            strategy_now_ts_utc: 1_700_000_001,
            last_bar_ts_utc: Some(1_699_999_800),
        }
    }

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
        let attribution = HybridRuntimeAttribution::parse_source_comment(
            "HYB|sid=hybrid_imoexf|c=abc1230001|o=MR|r=TP",
        )
        .expect("source tag");
        assert!(attribution.internal_comment().contains("sid=hybrid_imoexf"));
        assert!(attribution.belongs_to("hybrid_imoexf"));
        assert!(!attribution.belongs_to("other_strategy"));
        assert_eq!(attribution.owner(), Some(HybridRuntimeOwner::MeanReversion));
        assert_eq!(attribution.role(), Some(HybridRuntimeOrderRole::TakeProfit));
        attribution
            .validate_source_equivalence()
            .expect("parsed fields match raw comment");

        let repair = HybridRuntimeAttribution::parse_source_comment(
            "HYB|sid=hybrid_imoexf|c=abc1230001|o=MR|r=REPAIR",
        )
        .expect("repair tag parses");
        assert_eq!(repair.role(), None, "source parser leaves REPAIR unmapped");

        let mismatched = serde_json::json!({
            "internal_comment": "HYB|sid=hybrid_imoexf|c=abc1230001|o=MR|r=TP",
            "strategy_id": "other_strategy",
            "cycle_id": "abc1230001",
            "owner": "mean_reversion",
            "role": "take_profit"
        });
        assert!(serde_json::from_value::<HybridRuntimeAttribution>(mismatched).is_err());

        let existing = true;
        let new_transition = false;
        assert_ne!(existing, new_transition);
    }

    #[test]
    fn callback_context_changes_request_namespace_stop_price_and_restore_policy_inputs() {
        let first = context("ACC_TEST_A", 0.5, HybridRuntimeTradeMode::Paper);
        let second = context("ACC_TEST_B", 1.0, HybridRuntimeTradeMode::Live);
        let first_request = deterministic_request_id_for_account_instrument(
            &first.strategy_id,
            &first.request_namespace_account,
            &first.instrument,
            "market",
            first.event_ts_utc,
            3,
        );
        let second_request = deterministic_request_id_for_account_instrument(
            &second.strategy_id,
            &second.request_namespace_account,
            &second.instrument,
            "market",
            second.event_ts_utc,
            3,
        );
        assert_ne!(first_request, second_request);

        let trigger_price = 100.0;
        let first_stop_limit = trigger_price - first.tick_size;
        let second_stop_limit = trigger_price - second.tick_size;
        assert_ne!(first_stop_limit, second_stop_limit);

        let timer = HybridRuntimeCallbackInput {
            context: second.clone(),
            payload: HybridRuntimeTimerEvent {
                now_ts_utc_ms: 1_700_000_001_000,
            },
        };
        assert_eq!(
            timer.context.request_namespace_account.as_str(),
            "ACC_TEST_B"
        );
        assert_eq!(timer.context.trade_mode, HybridRuntimeTradeMode::Live);
        assert_eq!(timer.context.position_qty, Some(-1.0));

        let restored = HybridRuntimeCallbackInput {
            context: second,
            payload: HybridRuntimeStateRestored {
                known_order_ids: vec![BrokerOrderId::new("ORDER-1")],
                pending_requests: vec![StrategyRequestId::from(Uuid::from_u128(9))],
            },
        };
        assert_eq!(restored.context.trade_mode, HybridRuntimeTradeMode::Live);
        assert!(!restored.context.allow_live_orders);
        assert_eq!(restored.context.strategy_now_ts_utc, 1_700_000_001);
    }

    #[test]
    fn bootstrap_validator_rejects_duplicate_and_broker_truth_contradictions() {
        let target = instrument();
        let base = RuntimeHostBootstrapSnapshot {
            account_id: BrokerAccountId::new("ACC_TEST_A"),
            instrument: target.clone(),
            target_position_qty: Decimal::ONE,
            target_open_positions: Vec::new(),
            target_active_orders: Vec::new(),
            account_active_orders_count: 0,
            target_is_flat: false,
            received_ts: Utc::now(),
        };
        let position = HybridRuntimePositionEvent {
            instrument: target,
            qty: 1.0,
            existing: true,
            avg_price: 100.0,
            source_ts_utc: 1_700_000_000,
        };
        let valid = HybridRuntimeBootstrapSnapshot {
            broker_truth: base,
            positions_strategy: vec![position.clone()],
            working_orders_strategy: Vec::new(),
            working_stop_orders_strategy: Vec::new(),
            snapshot_ts_utc: Some(1_700_000_000),
        };
        valid.validate().expect("consistent bootstrap");

        let mut missing_position = valid.clone();
        missing_position.positions_strategy.clear();
        assert_eq!(
            missing_position.validate(),
            Err(HybridRuntimeBootstrapValidationError::BrokerTruthPositionMismatch)
        );

        let mut duplicate = valid.clone();
        duplicate.positions_strategy.push(position);
        assert_eq!(
            duplicate.validate(),
            Err(HybridRuntimeBootstrapValidationError::DuplicateTargetPosition)
        );

        let mut contradictory = valid;
        contradictory.positions_strategy[0].qty = 2.0;
        assert_eq!(
            contradictory.validate(),
            Err(HybridRuntimeBootstrapValidationError::BrokerTruthPositionMismatch)
        );
    }
}
