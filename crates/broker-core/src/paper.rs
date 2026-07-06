use std::collections::HashSet;
use std::fmt;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::envelope::SCHEMA_VERSION;
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
    pub received_ts: DateTime<Utc>,
}

impl PaperLedgerSnapshot {
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

    pub fn active_orders(&self) -> Vec<&PaperOrder> {
        self.orders
            .iter()
            .filter(|order| order.is_active())
            .collect()
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
            received_ts: ts(20),
        }
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
}
