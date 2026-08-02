//! Stage 5G-c deterministic order/trade/position convergence.
//!
//! The coordinator accepts only canonical Broker Core snapshots. Active and
//! partial evidence is accumulated without a strategy callback. A complete
//! terminal vector is mapped into the existing Stage 5C-j facade exactly once.
//! No Redis, FINAM transport, command dispatch, clock read or broker send is
//! reachable from this module.

use std::collections::BTreeMap;

use broker_core::command::CommandAckStatus;
use broker_core::{
    instrument_identity_matches, BrokerOrderId, BrokerOrderLifecycle, BrokerOrderSnapshot,
    BrokerPositionSnapshot, BrokerTradeSnapshot, BrokerTruthSnapshot, HybridRuntimeAttribution,
    HybridRuntimeOrderEvent, HybridRuntimePositionEvent, InstrumentId, OrderSide, OrderStatus,
    OrderType, StrategyRequestId,
};
use chrono::{DateTime, Utc};
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::stage5c_paper_host::{
    resolve_stage5c_paper_broker_lifecycle, settle_stage5c_validated_market_terminal_outcome_r3,
    validate_stage5c_market_terminal_outcome_r3, Stage5cBrokerLifecycleResolvedPaperStrategy,
    Stage5cBrokerLifecycleSettlement, Stage5cMarketTerminalOrderEvidence,
    Stage5cPaperBrokerEventPayload, Stage5cPaperBrokerEventRecord,
    Stage5cPaperBrokerLifecycleError, Stage5cPaperBrokerLifecycleInput, Stage5gSourceBaseAction,
    Stage5gSourceIntentProjection,
};
use crate::stage5g_mock_ack::{
    Stage5gMockAckSlotSummary, Stage5gMockIntentAction, Stage5gMockPlaceKind,
    Stage5gResolvedMockAckPaperStrategy,
};

pub const STAGE5G_ORDER_POSITION_SCHEMA_VERSION: u16 = 3;
const STAGE5G_EVIDENCE_FINGERPRINT_SCHEMA_VERSION: u16 = 2;

#[derive(Debug, Clone, PartialEq)]
pub struct Stage5gOrderPositionEvidence {
    pub total_sequence: u64,
    pub request_id: StrategyRequestId,
    pub broker_truth: BrokerTruthSnapshot,
    /// Broker-neutral operational snapshots deliberately do not carry strategy
    /// comments. Paper evidence supplies the parsed attribution separately;
    /// Stage 5C-j remains the authority that validates it against source intent.
    pub order_attribution: Option<HybridRuntimeAttribution>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage5gOrderPositionAdmissionError {
    EmptyAckLifecycle,
    AckLifecycleNotFullyResolved,
    MissingCanonicalAck,
    MissingBrokerOrderId,
    MissingSourceIntentProjection,
    SourceIntentProjectionMismatch,
    InvalidSourceQuantity,
    UnsupportedAction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage5gOrderPositionError {
    NonMonotonicSequence,
    UnknownRequestId,
    AccountMismatch,
    InstrumentMismatch,
    BrokerTruthBeforeAck,
    BrokerTruthTimeRegression,
    ComponentTimeAfterSnapshot,
    ComponentSourceTimeAfterReceipt,
    OrderTimeRegression,
    TradeTimeRegression,
    PositionTimeRegression,
    ConflictingDuplicateEvidence,
    BrokerEvidenceAfterTerminalAck,
    AccountWideActiveOrderSafetyGuard,
    AccountWideUnknownOrderSafetyGuard,
    MissingTargetOrder,
    AmbiguousTargetOrder,
    BrokerOrderIdMismatch,
    ClientOrderIdMismatch,
    OrderSideMismatch,
    OrderTypeMismatch,
    SourceOrderMismatch,
    AttributionMismatch,
    OrderLifecycleMismatch,
    UnknownOrderStatus,
    InvalidOrderQuantity,
    FilledQuantityRegression,
    OrderTerminalRegression,
    TradeIdentityConflict,
    TradeAccountMismatch,
    TradeInstrumentMismatch,
    TradeSideMismatch,
    TradeIdentityMismatch,
    NonPositiveTradeQuantity,
    TradeQuantityMismatch,
    MissingTargetPosition,
    AmbiguousTargetPosition,
    PositionAccountMismatch,
    PositionSideMismatch,
    PositionOverfill,
    PositionIncomplete,
    PositionQuantityRegression,
    OrderPositionIncoherent,
    TargetMarketOrderNonExecution,
    RejectedOrderHasFill,
    SequenceMappingOverflow,
    NumericConversionFailed,
    Stage5cPreCallbackBlocked,
    Stage5cRemainingLifecycle,
    Stage5cCallbackTerminal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stage5gOrderPositionSummary {
    pub schema_version: u16,
    pub strategy_id: String,
    pub request_count: usize,
    pub terminal_request_count: usize,
    pub order_transition_count: usize,
    pub correlated_trade_count: usize,
    pub position_confirmation_count: usize,
    pub duplicate_evidence_count: usize,
    pub last_total_sequence: Option<u64>,
    pub lifecycle_fingerprint_sha256: String,
    pub stage5c_callback_count: usize,
    pub mock_feedback_only: bool,
    pub redis_attached: bool,
    pub finam_transport_attached: bool,
    pub broker_execution_attached: bool,
}

#[derive(Clone)]
struct CanonicalOrderEvent {
    total_sequence: u64,
    order: BrokerOrderSnapshot,
    attribution: Option<HybridRuntimeAttribution>,
}

#[derive(Clone)]
struct Stage5gOrderPositionSlot {
    ack: Stage5gMockAckSlotSummary,
    source: Stage5gSourceIntentProjection,
    broker_order_id: Option<BrokerOrderId>,
    order_events: Vec<CanonicalOrderEvent>,
    trades: Vec<BrokerTradeSnapshot>,
    position: Option<(u64, BrokerPositionSnapshot)>,
    position_derivation: Option<CanonicalPositionDerivation>,
    position_matching_row_count: Option<usize>,
    market_terminal_truth: Option<BrokerTruthSnapshot>,
    last_order_source_ts: Option<DateTime<Utc>>,
    last_order_received_ts: Option<DateTime<Utc>>,
    last_trade_source_ts: Option<DateTime<Utc>>,
    last_trade_received_ts: Option<DateTime<Utc>>,
    last_position_source_ts: Option<DateTime<Utc>>,
    last_position_received_ts: Option<DateTime<Utc>>,
    terminal: bool,
}

#[derive(Clone)]
struct EvidenceIdentity {
    identity: String,
    fingerprint: String,
}

#[derive(Clone)]
struct Stage5gOrderPositionState {
    strategy_id: String,
    account_id: broker_core::BrokerAccountId,
    instrument: InstrumentId,
    slots: Vec<Stage5gOrderPositionSlot>,
    evidence_identities: Vec<EvidenceIdentity>,
    last_total_sequence: Option<u64>,
    last_broker_truth_received_ms: Option<i64>,
    duplicate_evidence_count: usize,
}

/// Linear paper-only capability. It intentionally implements none of Clone,
/// Copy, Debug, Display, Default, Serialize or Deserialize.
pub struct Stage5gOrderPositionSession {
    ack_resolved: Stage5gResolvedMockAckPaperStrategy,
    state: Stage5gOrderPositionState,
}

pub struct Stage5gConvergedPaperStrategy {
    resolved: Stage5cBrokerLifecycleResolvedPaperStrategy,
    summary: Stage5gOrderPositionSummary,
}

/// A terminal MARKET outcome settled only through the accepted R3 authority.
/// The contained Stage 5C settlement remains opaque and transport-free.
pub struct Stage5gMarketTerminalConvergedPaperStrategy {
    settlement: Stage5cBrokerLifecycleSettlement,
    summary: Stage5gOrderPositionSummary,
}

pub enum Stage5gOrderPositionTransition {
    Awaiting(Stage5gOrderPositionSession),
    Converged(Stage5gConvergedPaperStrategy),
    MarketTerminalConverged(Stage5gMarketTerminalConvergedPaperStrategy),
}

impl Stage5gOrderPositionTransition {
    pub fn into_awaiting(self) -> Option<Stage5gOrderPositionSession> {
        match self {
            Self::Awaiting(session) => Some(session),
            Self::Converged(_) | Self::MarketTerminalConverged(_) => None,
        }
    }

    pub fn into_converged(self) -> Option<Stage5gConvergedPaperStrategy> {
        match self {
            Self::Awaiting(_) => None,
            Self::Converged(converged) => Some(converged),
            Self::MarketTerminalConverged(_) => None,
        }
    }

    pub fn into_market_terminal_converged(
        self,
    ) -> Option<Stage5gMarketTerminalConvergedPaperStrategy> {
        match self {
            Self::MarketTerminalConverged(converged) => Some(converged),
            Self::Awaiting(_) | Self::Converged(_) => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum CanonicalPositionDerivation {
    AbsentFlat,
    ExplicitSingle,
    Aggregate,
}

#[derive(Clone)]
struct CanonicalTargetPosition {
    snapshot: BrokerPositionSnapshot,
    derivation: CanonicalPositionDerivation,
    matching_row_count: usize,
}

pub struct Stage5gOrderPositionAdmissionBlocked {
    reason: Stage5gOrderPositionAdmissionError,
    ack_resolved: Stage5gResolvedMockAckPaperStrategy,
}

impl Stage5gOrderPositionAdmissionBlocked {
    pub fn reason(&self) -> Stage5gOrderPositionAdmissionError {
        self.reason
    }

    pub fn into_ack_resolved(self) -> Stage5gResolvedMockAckPaperStrategy {
        self.ack_resolved
    }
}

impl std::fmt::Debug for Stage5gOrderPositionAdmissionBlocked {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Stage5gOrderPositionAdmissionBlocked")
            .field("reason", &self.reason)
            .finish_non_exhaustive()
    }
}

pub struct Stage5gOrderPositionBlocked {
    reason: Stage5gOrderPositionError,
    session: Stage5gOrderPositionSession,
}

impl Stage5gOrderPositionBlocked {
    pub fn reason(&self) -> Stage5gOrderPositionError {
        self.reason
    }

    pub fn session(&self) -> &Stage5gOrderPositionSession {
        &self.session
    }

    pub fn into_session(self) -> Stage5gOrderPositionSession {
        self.session
    }
}

impl std::fmt::Debug for Stage5gOrderPositionBlocked {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Stage5gOrderPositionBlocked")
            .field("reason", &self.reason)
            .field("summary", &self.session.summary())
            .finish_non_exhaustive()
    }
}

pub struct Stage5gOrderPositionTerminal {
    reason: Stage5gOrderPositionError,
    stage5c_reason: Stage5cPaperBrokerLifecycleError,
}

impl Stage5gOrderPositionTerminal {
    pub fn reason(&self) -> Stage5gOrderPositionError {
        self.reason
    }

    pub fn stage5c_reason(&self) -> Stage5cPaperBrokerLifecycleError {
        self.stage5c_reason
    }
}

impl std::fmt::Debug for Stage5gOrderPositionTerminal {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Stage5gOrderPositionTerminal")
            .field("reason", &self.reason)
            .field("stage5c_reason", &self.stage5c_reason)
            .finish()
    }
}

#[derive(Debug)]
pub enum Stage5gOrderPositionFailure {
    Blocked(Box<Stage5gOrderPositionBlocked>),
    Terminal(Stage5gOrderPositionTerminal),
}

impl Stage5gOrderPositionFailure {
    pub fn reason(&self) -> Stage5gOrderPositionError {
        match self {
            Self::Blocked(blocked) => blocked.reason(),
            Self::Terminal(terminal) => terminal.reason(),
        }
    }

    pub fn into_blocked(self) -> Option<Stage5gOrderPositionBlocked> {
        match self {
            Self::Blocked(blocked) => Some(*blocked),
            Self::Terminal(_) => None,
        }
    }
}

pub fn attach_stage5g_order_position_session(
    ack_resolved: Stage5gResolvedMockAckPaperStrategy,
) -> Result<Stage5gOrderPositionSession, Box<Stage5gOrderPositionAdmissionBlocked>> {
    let summary = ack_resolved.lifecycle_summary();
    if summary.slots.is_empty() {
        return Err(admission_block(
            Stage5gOrderPositionAdmissionError::EmptyAckLifecycle,
            ack_resolved,
        ));
    }
    if summary.resolved_count != summary.slot_count {
        return Err(admission_block(
            Stage5gOrderPositionAdmissionError::AckLifecycleNotFullyResolved,
            ack_resolved,
        ));
    }
    let outcomes = ack_resolved.ack_outcomes();
    let source_projections = ack_resolved.source_intent_projections();
    let mut slots = Vec::with_capacity(summary.slots.len());
    for slot in &summary.slots {
        let Some(outcome) = outcomes
            .iter()
            .find(|outcome| outcome.request_id == slot.request_id)
        else {
            return Err(admission_block(
                Stage5gOrderPositionAdmissionError::MissingCanonicalAck,
                ack_resolved,
            ));
        };
        let Some(source) = source_projections
            .iter()
            .find(|source| source.request_id == slot.request_id)
            .cloned()
        else {
            return Err(admission_block(
                Stage5gOrderPositionAdmissionError::MissingSourceIntentProjection,
                ack_resolved,
            ));
        };
        if !source_projection_matches_ack(&source, slot) {
            return Err(admission_block(
                Stage5gOrderPositionAdmissionError::SourceIntentProjectionMismatch,
                ack_resolved,
            ));
        }
        if source
            .target_qty
            .is_some_and(|qty| !qty.is_finite() || qty <= 0.0)
            || !source.pre_position_qty.is_finite()
        {
            return Err(admission_block(
                Stage5gOrderPositionAdmissionError::InvalidSourceQuantity,
                ack_resolved,
            ));
        }
        if matches!(slot.action, Stage5gMockIntentAction::Place { .. })
            && !matches!(
                slot.latest_status,
                Some(
                    CommandAckStatus::Rejected
                        | CommandAckStatus::Expired
                        | CommandAckStatus::Error
                )
            )
            && outcome.broker_order_id.is_none()
        {
            return Err(admission_block(
                Stage5gOrderPositionAdmissionError::MissingBrokerOrderId,
                ack_resolved,
            ));
        }
        let terminal = matches!(
            slot.latest_status,
            Some(CommandAckStatus::Rejected | CommandAckStatus::Expired | CommandAckStatus::Error)
        );
        slots.push(Stage5gOrderPositionSlot {
            ack: slot.clone(),
            source,
            broker_order_id: outcome.broker_order_id.clone(),
            order_events: Vec::new(),
            trades: Vec::new(),
            position: None,
            position_derivation: None,
            position_matching_row_count: None,
            market_terminal_truth: None,
            last_order_source_ts: None,
            last_order_received_ts: None,
            last_trade_source_ts: None,
            last_trade_received_ts: None,
            last_position_source_ts: None,
            last_position_received_ts: None,
            terminal,
        });
    }
    Ok(Stage5gOrderPositionSession {
        ack_resolved,
        state: Stage5gOrderPositionState {
            strategy_id: summary.strategy_id,
            account_id: summary.account_id,
            instrument: summary.instrument,
            slots,
            evidence_identities: Vec::new(),
            last_total_sequence: None,
            last_broker_truth_received_ms: None,
            duplicate_evidence_count: 0,
        },
    })
}

fn source_projection_matches_ack(
    source: &Stage5gSourceIntentProjection,
    ack: &Stage5gMockAckSlotSummary,
) -> bool {
    if format!("{:?}", source.intent_class) != ack.intent_class {
        return false;
    }
    let action_matches = matches!(
        (&source.base_action, &ack.action),
        (
            Stage5gSourceBaseAction::Market,
            Stage5gMockIntentAction::Place {
                place_kind: Stage5gMockPlaceKind::Market
            }
        ) | (
            Stage5gSourceBaseAction::Place,
            Stage5gMockIntentAction::Place {
                place_kind: Stage5gMockPlaceKind::Limit
            }
        ) | (
            Stage5gSourceBaseAction::Cancel,
            Stage5gMockIntentAction::Cancel { .. }
        )
    );
    let source_side = source.side.map(|side| format!("{side:?}"));
    action_matches && source_side.as_deref() == ack.side.as_deref()
}

/// Canonicalizes a full broker poll before replay classification. FINAM maps
/// every historical trade with the current observation receipt, so receipt is
/// deliberately excluded from immutable trade identity while the newest
/// observation watermark is retained.
fn canonicalize_broker_truth_snapshot(
    truth: &mut BrokerTruthSnapshot,
) -> Result<(), Stage5gOrderPositionError> {
    let mut trades_by_id: BTreeMap<String, BrokerTradeSnapshot> = BTreeMap::new();
    for trade in truth.trades.drain(..) {
        let key = trade.broker_trade_id.as_str().to_string();
        match trades_by_id.get_mut(&key) {
            Some(existing) if immutable_trade_payload_matches(existing, &trade) => {
                if trade.received_ts > existing.received_ts {
                    existing.received_ts = trade.received_ts;
                }
            }
            Some(_) => return Err(Stage5gOrderPositionError::TradeIdentityConflict),
            None => {
                trades_by_id.insert(key, trade);
            }
        }
    }
    truth.trades = trades_by_id.into_values().collect();
    canonical_json_sort(&mut truth.orders);
    canonical_json_sort(&mut truth.positions);
    canonical_json_sort(&mut truth.instruments);
    if let Some(cash) = truth.cash.as_mut() {
        canonical_json_sort(&mut cash.cash);
    }
    Ok(())
}

fn canonical_json_sort<T: Serialize>(values: &mut [T]) {
    values.sort_by_cached_key(|value| {
        serde_json::to_vec(value).expect("broker-neutral canonical value serializes")
    });
}

fn immutable_trade_payload_matches(
    left: &BrokerTradeSnapshot,
    right: &BrokerTradeSnapshot,
) -> bool {
    left.account_id == right.account_id
        && left.broker_trade_id == right.broker_trade_id
        && left.broker_order_id == right.broker_order_id
        && left.client_order_id == right.client_order_id
        && instrument_identity_matches(&left.instrument, &right.instrument)
        && left.side == right.side
        && left.qty == right.qty
        && left.price == right.price
        && left.gross_amount == right.gross_amount
        && left.commission == right.commission
        && left.broker_asset_id == right.broker_asset_id
        && left.board == right.board
        && left.expiration_date == right.expiration_date
        && left.source_ts == right.source_ts
}

pub fn apply_stage5g_order_position_evidence(
    mut session: Stage5gOrderPositionSession,
    mut evidence: Stage5gOrderPositionEvidence,
) -> Result<Stage5gOrderPositionTransition, Stage5gOrderPositionFailure> {
    if session
        .state
        .last_total_sequence
        .is_some_and(|last| evidence.total_sequence <= last)
    {
        return Err(block(
            Stage5gOrderPositionError::NonMonotonicSequence,
            session,
        ));
    }
    let Some(slot_index) = session
        .state
        .slots
        .iter()
        .position(|slot| slot.ack.request_id == evidence.request_id)
    else {
        return Err(block(Stage5gOrderPositionError::UnknownRequestId, session));
    };
    if evidence.broker_truth.account_id != session.state.account_id {
        return Err(block(Stage5gOrderPositionError::AccountMismatch, session));
    }
    if let Err(reason) = canonicalize_broker_truth_snapshot(&mut evidence.broker_truth) {
        return Err(block(reason, session));
    }
    let identity = evidence_identity(&evidence);
    let fingerprint = evidence_fingerprint(&evidence);
    match classify_evidence_replay(&session.state, &identity, &fingerprint) {
        Err(reason) => return Err(block(reason, session)),
        Ok(true) => {
            session.state.last_total_sequence = Some(evidence.total_sequence);
            session.state.duplicate_evidence_count += 1;
            return Ok(Stage5gOrderPositionTransition::Awaiting(session));
        }
        Ok(false) => {}
    }
    let ack_ts = session.state.slots[slot_index]
        .ack
        .latest_received_ts_utc
        .as_deref()
        .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
        .map(|value| value.with_timezone(&chrono::Utc));
    if ack_ts.is_some_and(|ack_ts| evidence.broker_truth.received_ts < ack_ts) {
        return Err(block(
            Stage5gOrderPositionError::BrokerTruthBeforeAck,
            session,
        ));
    }
    if has_non_target_active_order(&session, &evidence.broker_truth, slot_index) {
        return Err(block(
            Stage5gOrderPositionError::AccountWideActiveOrderSafetyGuard,
            session,
        ));
    }
    if evidence.broker_truth.orders.iter().any(|order| {
        order.lifecycle == BrokerOrderLifecycle::Unknown
            && order.broker_order_id != session.state.slots[slot_index].broker_order_id
    }) {
        return Err(block(
            Stage5gOrderPositionError::AccountWideUnknownOrderSafetyGuard,
            session,
        ));
    }

    // Evidence application is transactional: a blocked snapshot must not
    // partially append order/trade/position state before the caller retries
    // with corrected broker truth.
    let pre_candidate_state = session.state.clone();
    let mut next_slot = session.state.slots[slot_index].clone();
    if let Err(reason) = validate_snapshot_chronology(
        session.state.last_broker_truth_received_ms,
        &session.state.instrument,
        &mut next_slot,
        &evidence,
    ) {
        return Err(block(reason, session));
    }
    let result = apply_to_slot(
        &session.state.account_id,
        &session.state.instrument,
        &mut next_slot,
        &evidence,
    );
    if let Err(reason) = result {
        return Err(block(reason, session));
    }
    session.state.slots[slot_index] = next_slot;
    session.state.last_total_sequence = Some(evidence.total_sequence);
    session.state.last_broker_truth_received_ms =
        Some(evidence.broker_truth.received_ts.timestamp_millis());
    session.state.evidence_identities.push(EvidenceIdentity {
        identity,
        fingerprint,
    });

    if !session.state.slots.iter().all(|slot| slot.terminal) {
        return Ok(Stage5gOrderPositionTransition::Awaiting(session));
    }
    converge_through_stage5c(session, pre_candidate_state)
}

fn classify_evidence_replay(
    state: &Stage5gOrderPositionState,
    identity: &str,
    fingerprint: &str,
) -> Result<bool, Stage5gOrderPositionError> {
    let Some(previous) = state
        .evidence_identities
        .iter()
        .find(|previous| previous.identity == identity)
    else {
        return Ok(false);
    };
    if previous.fingerprint != fingerprint {
        return Err(Stage5gOrderPositionError::ConflictingDuplicateEvidence);
    }
    Ok(true)
}

fn validate_snapshot_chronology(
    last_broker_truth_received_ms: Option<i64>,
    instrument: &InstrumentId,
    slot: &mut Stage5gOrderPositionSlot,
    evidence: &Stage5gOrderPositionEvidence,
) -> Result<(), Stage5gOrderPositionError> {
    let snapshot_ts = evidence.broker_truth.received_ts;
    let snapshot_ms = snapshot_ts.timestamp_millis();
    if last_broker_truth_received_ms.is_some_and(|last| snapshot_ms < last) {
        return Err(Stage5gOrderPositionError::BrokerTruthTimeRegression);
    }
    let target_order_id = slot.broker_order_id.as_ref();
    let target_client_order_id = &slot.ack.expected_client_order_id;
    let correlated_orders: Vec<_> = evidence
        .broker_truth
        .orders
        .iter()
        .filter(|order| {
            order.account_id == evidence.broker_truth.account_id
                && instrument_identity_matches(&order.instrument, instrument)
                && (order.broker_order_id.as_ref() == target_order_id
                    || order.client_order_id.as_ref() == Some(target_client_order_id))
        })
        .collect();
    for order in &correlated_orders {
        validate_component_time(order.source_ts, order.received_ts, snapshot_ts)?;
        if order
            .source_ts
            .is_some_and(|source| slot.last_order_source_ts.is_some_and(|last| source < last))
            || slot
                .last_order_received_ts
                .is_some_and(|last| order.received_ts < last)
        {
            return Err(Stage5gOrderPositionError::OrderTimeRegression);
        }
    }
    slot.last_order_source_ts = correlated_orders
        .iter()
        .filter_map(|order| order.source_ts)
        .max()
        .or(slot.last_order_source_ts);
    slot.last_order_received_ts = correlated_orders
        .iter()
        .map(|order| order.received_ts)
        .max()
        .or(slot.last_order_received_ts);

    let target_positions: Vec<_> = evidence
        .broker_truth
        .positions
        .iter()
        .filter(|position| {
            position.account_id == evidence.broker_truth.account_id
                && instrument_identity_matches(&position.instrument, instrument)
        })
        .collect();
    for position in &target_positions {
        validate_component_time(position.source_ts, position.received_ts, snapshot_ts)?;
        if position.source_ts.is_some_and(|source| {
            slot.last_position_source_ts
                .is_some_and(|last| source < last)
        }) || slot
            .last_position_received_ts
            .is_some_and(|last| position.received_ts < last)
        {
            return Err(Stage5gOrderPositionError::PositionTimeRegression);
        }
    }
    slot.last_position_source_ts = target_positions
        .iter()
        .filter_map(|position| position.source_ts)
        .max()
        .or(slot.last_position_source_ts);
    slot.last_position_received_ts = target_positions
        .iter()
        .map(|position| position.received_ts)
        .max()
        .or(slot.last_position_received_ts);

    let correlated_trades: Vec<_> = evidence
        .broker_truth
        .trades
        .iter()
        .filter(|trade| {
            trade.account_id == evidence.broker_truth.account_id
                && instrument_identity_matches(&trade.instrument, instrument)
                && (trade.broker_order_id.as_ref() == target_order_id
                    || trade.client_order_id.as_ref() == Some(target_client_order_id))
        })
        .collect();
    for trade in &correlated_trades {
        validate_component_time(Some(trade.source_ts), trade.received_ts, snapshot_ts)?;
        if slot
            .trades
            .iter()
            .any(|known| known.broker_trade_id == trade.broker_trade_id && known == *trade)
        {
            continue;
        }
        if slot
            .last_trade_source_ts
            .is_some_and(|last| trade.source_ts < last)
            || slot
                .last_trade_received_ts
                .is_some_and(|last| trade.received_ts < last)
        {
            return Err(Stage5gOrderPositionError::TradeTimeRegression);
        }
    }
    slot.last_trade_source_ts = correlated_trades
        .iter()
        .map(|trade| trade.source_ts)
        .max()
        .or(slot.last_trade_source_ts);
    slot.last_trade_received_ts = correlated_trades
        .iter()
        .map(|trade| trade.received_ts)
        .max()
        .or(slot.last_trade_received_ts);
    Ok(())
}

fn validate_component_time(
    source_ts: Option<DateTime<Utc>>,
    received_ts: DateTime<Utc>,
    snapshot_ts: DateTime<Utc>,
) -> Result<(), Stage5gOrderPositionError> {
    if received_ts > snapshot_ts {
        return Err(Stage5gOrderPositionError::ComponentTimeAfterSnapshot);
    }
    if source_ts.is_some_and(|source| source > received_ts) {
        return Err(Stage5gOrderPositionError::ComponentSourceTimeAfterReceipt);
    }
    Ok(())
}

fn apply_to_slot(
    account_id: &broker_core::BrokerAccountId,
    instrument: &InstrumentId,
    slot: &mut Stage5gOrderPositionSlot,
    evidence: &Stage5gOrderPositionEvidence,
) -> Result<(), Stage5gOrderPositionError> {
    let mut candidate = slot.clone();
    apply_to_slot_candidate(account_id, instrument, &mut candidate, evidence)?;
    *slot = candidate;
    Ok(())
}

fn apply_to_slot_candidate(
    account_id: &broker_core::BrokerAccountId,
    instrument: &InstrumentId,
    slot: &mut Stage5gOrderPositionSlot,
    evidence: &Stage5gOrderPositionEvidence,
) -> Result<(), Stage5gOrderPositionError> {
    if slot.terminal {
        let has_target_order = evidence.broker_truth.orders.iter().any(|order| {
            order.account_id == *account_id
                && instrument_identity_matches(&order.instrument, instrument)
                && (order.broker_order_id == slot.broker_order_id
                    || order.client_order_id.as_ref() == Some(&slot.ack.expected_client_order_id))
        });
        let target_position =
            canonical_target_position(account_id, instrument, &evidence.broker_truth)?;
        let contradictory_target_position =
            decimal_f64_differs(target_position.snapshot.qty, slot.source.pre_position_qty);
        if has_target_order || contradictory_target_position {
            return Err(Stage5gOrderPositionError::BrokerEvidenceAfterTerminalAck);
        }
        return Ok(());
    }
    match &slot.ack.action {
        Stage5gMockIntentAction::Place {
            place_kind: Stage5gMockPlaceKind::Market,
        } => {
            if let Some(order) = select_optional_target_order(slot, &evidence.broker_truth)? {
                // A concrete broker order is authoritative for lifecycle
                // classification. Position truth is evaluated only after the
                // order status and cumulative fill have been authenticated.
                validate_order(account_id, instrument, slot, &order)?;
                validate_source_order_attribution(slot, evidence.order_attribution.as_ref())?;
                validate_trades(slot, &order, &evidence.broker_truth.trades)?;
                slot.order_events.push(CanonicalOrderEvent {
                    total_sequence: evidence.total_sequence,
                    order: order.clone(),
                    attribution: evidence.order_attribution.clone(),
                });
                slot.order_events.sort_by(|left, right| {
                    left.total_sequence
                        .cmp(&right.total_sequence)
                        .then_with(|| left.order.received_ts.cmp(&right.order.received_ts))
                });
                match order.status {
                    OrderStatus::New | OrderStatus::Working => {
                        if order.filled_qty != Decimal::ZERO {
                            return Err(Stage5gOrderPositionError::OrderPositionIncoherent);
                        }
                        let position = canonical_target_position(
                            account_id,
                            instrument,
                            &evidence.broker_truth,
                        )?;
                        validate_order_position_coherence(slot, &position.snapshot, &order)?;
                        store_canonical_position(slot, evidence.total_sequence, position);
                        slot.terminal = false;
                    }
                    OrderStatus::PartiallyFilled => {
                        if order.filled_qty <= Decimal::ZERO || order.filled_qty >= order.qty {
                            return Err(Stage5gOrderPositionError::InvalidOrderQuantity);
                        }
                        let position = canonical_target_position(
                            account_id,
                            instrument,
                            &evidence.broker_truth,
                        )?;
                        validate_order_position_coherence(slot, &position.snapshot, &order)?;
                        store_canonical_position(slot, evidence.total_sequence, position);
                        slot.terminal = false;
                    }
                    OrderStatus::Filled => {
                        let position = canonical_target_position(
                            account_id,
                            instrument,
                            &evidence.broker_truth,
                        )?;
                        validate_order_position_coherence(slot, &position.snapshot, &order)?;
                        if !validate_source_position(slot, &position.snapshot, Some(&order))? {
                            return Err(Stage5gOrderPositionError::PositionIncomplete);
                        }
                        store_canonical_position(slot, evidence.total_sequence, position);
                        slot.terminal = true;
                    }
                    OrderStatus::Unknown(_) => {
                        return Err(Stage5gOrderPositionError::UnknownOrderStatus);
                    }
                    OrderStatus::Rejected => {
                        if order.filled_qty != Decimal::ZERO {
                            return Err(Stage5gOrderPositionError::RejectedOrderHasFill);
                        }
                        let position = canonical_target_position(
                            account_id,
                            instrument,
                            &evidence.broker_truth,
                        )?;
                        validate_order_position_coherence(slot, &position.snapshot, &order)?;
                        store_canonical_position(slot, evidence.total_sequence, position);
                        slot.market_terminal_truth = Some(evidence.broker_truth.clone());
                        slot.terminal = true;
                    }
                    OrderStatus::Canceled | OrderStatus::Expired => {
                        let position = canonical_target_position(
                            account_id,
                            instrument,
                            &evidence.broker_truth,
                        )?;
                        validate_order_position_coherence(slot, &position.snapshot, &order)?;
                        store_canonical_position(slot, evidence.total_sequence, position);
                        slot.market_terminal_truth = Some(evidence.broker_truth.clone());
                        slot.terminal = true;
                    }
                }
            } else {
                // Position-only Market evidence is permitted only when the
                // broker snapshot has no target order row. Exact source-relative
                // position progress remains authoritative in this mode.
                let position =
                    canonical_target_position(account_id, instrument, &evidence.broker_truth)?;
                let terminal = validate_source_position(slot, &position.snapshot, None)?;
                store_canonical_position(slot, evidence.total_sequence, position);
                slot.terminal = terminal;
            }
        }
        Stage5gMockIntentAction::Place {
            place_kind: Stage5gMockPlaceKind::Limit,
        }
        | Stage5gMockIntentAction::Cancel { .. } => {
            let order = select_target_order(slot, &evidence.broker_truth)?;
            validate_order(account_id, instrument, slot, &order)?;
            validate_source_order_attribution(slot, evidence.order_attribution.as_ref())?;
            validate_trades(slot, &order, &evidence.broker_truth.trades)?;
            let status = order.status.clone();
            slot.order_events.push(CanonicalOrderEvent {
                total_sequence: evidence.total_sequence,
                order: order.clone(),
                attribution: evidence.order_attribution.clone(),
            });
            slot.order_events.sort_by(|left, right| {
                left.total_sequence
                    .cmp(&right.total_sequence)
                    .then_with(|| left.order.received_ts.cmp(&right.order.received_ts))
            });
            match status {
                OrderStatus::New | OrderStatus::Working | OrderStatus::PartiallyFilled => {}
                OrderStatus::Filled => {
                    let position =
                        canonical_target_position(account_id, instrument, &evidence.broker_truth)?;
                    if !validate_source_position(slot, &position.snapshot, Some(&order))? {
                        return Err(Stage5gOrderPositionError::PositionIncomplete);
                    }
                    store_canonical_position(slot, evidence.total_sequence, position);
                    slot.terminal = true;
                }
                OrderStatus::Rejected => {
                    if order.filled_qty > Decimal::ZERO {
                        return Err(Stage5gOrderPositionError::RejectedOrderHasFill);
                    }
                    slot.terminal = true;
                }
                OrderStatus::Canceled | OrderStatus::Expired => {
                    if order.filled_qty > Decimal::ZERO {
                        let position = canonical_target_position(
                            account_id,
                            instrument,
                            &evidence.broker_truth,
                        )?;
                        validate_partial_terminal_position(slot, &position.snapshot, &order)?;
                        store_canonical_position(slot, evidence.total_sequence, position);
                    }
                    slot.terminal = true;
                }
                OrderStatus::Unknown(_) => {
                    return Err(Stage5gOrderPositionError::UnknownOrderStatus);
                }
            }
        }
    }
    Ok(())
}

fn select_target_order(
    slot: &Stage5gOrderPositionSlot,
    truth: &BrokerTruthSnapshot,
) -> Result<BrokerOrderSnapshot, Stage5gOrderPositionError> {
    let expected = slot
        .broker_order_id
        .as_ref()
        .ok_or(Stage5gOrderPositionError::MissingTargetOrder)?;
    let matches: Vec<_> = truth
        .orders
        .iter()
        .filter(|order| order.broker_order_id.as_ref() == Some(expected))
        .cloned()
        .collect();
    match matches.len() {
        0 => Err(Stage5gOrderPositionError::MissingTargetOrder),
        1 => Ok(matches.into_iter().next().expect("one target order")),
        _ => Err(Stage5gOrderPositionError::AmbiguousTargetOrder),
    }
}

fn select_optional_target_order(
    slot: &Stage5gOrderPositionSlot,
    truth: &BrokerTruthSnapshot,
) -> Result<Option<BrokerOrderSnapshot>, Stage5gOrderPositionError> {
    let matches: Vec<_> = truth
        .orders
        .iter()
        .filter(|order| {
            order.broker_order_id.as_ref() == slot.broker_order_id.as_ref()
                || order.client_order_id.as_ref() == Some(&slot.ack.expected_client_order_id)
        })
        .cloned()
        .collect();
    match matches.len() {
        0 => Ok(None),
        1 => Ok(matches.into_iter().next()),
        _ => Err(Stage5gOrderPositionError::AmbiguousTargetOrder),
    }
}

fn canonical_target_position(
    account_id: &broker_core::BrokerAccountId,
    instrument: &InstrumentId,
    truth: &BrokerTruthSnapshot,
) -> Result<CanonicalTargetPosition, Stage5gOrderPositionError> {
    if truth.positions.iter().any(|position| {
        position.matches_instrument(instrument) && &position.account_id != account_id
    }) {
        return Err(Stage5gOrderPositionError::PositionAccountMismatch);
    }
    let matches: Vec<_> = truth
        .positions
        .iter()
        .filter(|position| {
            &position.account_id == account_id && position.matches_instrument(instrument)
        })
        .cloned()
        .collect();
    if matches.is_empty() {
        return Ok(CanonicalTargetPosition {
            snapshot: BrokerPositionSnapshot {
                account_id: account_id.clone(),
                instrument: instrument.clone(),
                qty: Decimal::ZERO,
                avg_price: None,
                unrealized_pnl: None,
                source_ts: None,
                received_ts: truth.received_ts,
            },
            derivation: CanonicalPositionDerivation::AbsentFlat,
            matching_row_count: 0,
        });
    }

    let first_nonzero_sign = matches
        .iter()
        .find(|position| position.qty != Decimal::ZERO)
        .map(|position| position.qty.is_sign_positive());
    if first_nonzero_sign.is_some_and(|sign| {
        matches.iter().any(|position| {
            position.qty != Decimal::ZERO && position.qty.is_sign_positive() != sign
        })
    }) {
        return Err(Stage5gOrderPositionError::PositionSideMismatch);
    }
    let qty: Decimal = matches.iter().map(|position| position.qty).sum();
    let mut weighted_value = Decimal::ZERO;
    let mut weight = Decimal::ZERO;
    for position in matches
        .iter()
        .filter(|position| position.qty != Decimal::ZERO)
    {
        let avg_price = position
            .avg_price
            .ok_or(Stage5gOrderPositionError::PositionIncomplete)?;
        weighted_value += avg_price * position.qty.abs();
        weight += position.qty.abs();
    }
    let avg_price = (weight > Decimal::ZERO).then(|| weighted_value / weight);
    let source_ts = matches
        .iter()
        .filter_map(|position| position.source_ts)
        .max();
    let received_ts = matches
        .iter()
        .map(|position| position.received_ts)
        .max()
        .unwrap_or(truth.received_ts);
    let matching_row_count = matches.len();
    Ok(CanonicalTargetPosition {
        snapshot: BrokerPositionSnapshot {
            account_id: account_id.clone(),
            instrument: instrument.clone(),
            qty,
            avg_price,
            unrealized_pnl: None,
            source_ts,
            received_ts,
        },
        derivation: if matching_row_count == 1 {
            CanonicalPositionDerivation::ExplicitSingle
        } else {
            CanonicalPositionDerivation::Aggregate
        },
        matching_row_count,
    })
}

fn validate_order(
    account_id: &broker_core::BrokerAccountId,
    instrument: &InstrumentId,
    slot: &Stage5gOrderPositionSlot,
    order: &BrokerOrderSnapshot,
) -> Result<(), Stage5gOrderPositionError> {
    if &order.account_id != account_id {
        return Err(Stage5gOrderPositionError::AccountMismatch);
    }
    if !instrument_identity_matches(&order.instrument, instrument) {
        return Err(Stage5gOrderPositionError::InstrumentMismatch);
    }
    if order.broker_order_id != slot.broker_order_id {
        return Err(Stage5gOrderPositionError::BrokerOrderIdMismatch);
    }
    if order.client_order_id.as_ref() != Some(&slot.ack.expected_client_order_id) {
        return Err(Stage5gOrderPositionError::ClientOrderIdMismatch);
    }
    let expected_side = slot
        .ack
        .side
        .as_deref()
        .and_then(parse_side)
        .ok_or(Stage5gOrderPositionError::OrderSideMismatch)?;
    if order.side != expected_side {
        return Err(Stage5gOrderPositionError::OrderSideMismatch);
    }
    match slot.ack.action {
        Stage5gMockIntentAction::Place {
            place_kind: Stage5gMockPlaceKind::Market,
        } if order.order_type != OrderType::Market => {
            return Err(Stage5gOrderPositionError::OrderTypeMismatch);
        }
        Stage5gMockIntentAction::Place {
            place_kind: Stage5gMockPlaceKind::Limit,
        } if order.order_type != OrderType::Limit => {
            return Err(Stage5gOrderPositionError::OrderTypeMismatch);
        }
        Stage5gMockIntentAction::Cancel {
            ref target_order_id,
        } if order.broker_order_id.as_ref() != Some(target_order_id) => {
            return Err(Stage5gOrderPositionError::BrokerOrderIdMismatch);
        }
        _ => {}
    }
    match order.order_type {
        OrderType::Market if order.limit_price.is_some() => {
            return Err(Stage5gOrderPositionError::SourceOrderMismatch);
        }
        OrderType::Limit
            if order
                .limit_price
                .map_or(true, |price| price <= Decimal::ZERO) =>
        {
            return Err(Stage5gOrderPositionError::SourceOrderMismatch);
        }
        _ => {}
    }
    if order.lifecycle != BrokerOrderSnapshot::lifecycle_for(&order.status) {
        return Err(Stage5gOrderPositionError::OrderLifecycleMismatch);
    }
    if matches!(order.status, OrderStatus::Unknown(_)) {
        return Err(Stage5gOrderPositionError::UnknownOrderStatus);
    }
    if order.qty <= Decimal::ZERO
        || order.filled_qty < Decimal::ZERO
        || order.filled_qty > order.qty
        || order.remaining_qty.is_some_and(|remaining| {
            remaining < Decimal::ZERO || remaining != order.qty - order.filled_qty
        })
        || matches!(order.status, OrderStatus::Filled) && order.filled_qty != order.qty
    {
        return Err(Stage5gOrderPositionError::InvalidOrderQuantity);
    }
    let source_qty = slot
        .source
        .target_qty
        .and_then(Decimal::from_f64_retain)
        .ok_or(Stage5gOrderPositionError::SourceOrderMismatch)?;
    if order.qty != source_qty || order.filled_qty > source_qty {
        return Err(Stage5gOrderPositionError::SourceOrderMismatch);
    }
    if let Some(previous) = slot.order_events.last().map(|event| &event.order) {
        if previous.lifecycle == BrokerOrderLifecycle::Terminal && previous.status != order.status {
            return Err(Stage5gOrderPositionError::OrderTerminalRegression);
        }
        if order.filled_qty < previous.filled_qty {
            return Err(Stage5gOrderPositionError::FilledQuantityRegression);
        }
    }
    Ok(())
}

fn validate_source_order_attribution(
    slot: &Stage5gOrderPositionSlot,
    actual: Option<&HybridRuntimeAttribution>,
) -> Result<(), Stage5gOrderPositionError> {
    if actual != slot.source.expected_attribution.as_ref() {
        return Err(Stage5gOrderPositionError::AttributionMismatch);
    }
    Ok(())
}

fn validate_trades(
    slot: &mut Stage5gOrderPositionSlot,
    order: &BrokerOrderSnapshot,
    trades: &[BrokerTradeSnapshot],
) -> Result<(), Stage5gOrderPositionError> {
    let mut incoming_by_id: BTreeMap<String, BrokerTradeSnapshot> = BTreeMap::new();
    for trade in trades {
        let broker_matches =
            trade.broker_order_id.is_some() && trade.broker_order_id == order.broker_order_id;
        let client_matches =
            trade.client_order_id.is_some() && trade.client_order_id == order.client_order_id;
        if !broker_matches && !client_matches {
            continue;
        }
        if trade
            .broker_order_id
            .as_ref()
            .is_some_and(|actual| Some(actual) != order.broker_order_id.as_ref())
            || trade
                .client_order_id
                .as_ref()
                .is_some_and(|actual| Some(actual) != order.client_order_id.as_ref())
        {
            return Err(Stage5gOrderPositionError::TradeIdentityMismatch);
        }
        if trade.qty <= Decimal::ZERO {
            return Err(Stage5gOrderPositionError::NonPositiveTradeQuantity);
        }
        if trade.account_id != order.account_id {
            return Err(Stage5gOrderPositionError::TradeAccountMismatch);
        }
        if !instrument_identity_matches(&trade.instrument, &order.instrument) {
            return Err(Stage5gOrderPositionError::TradeInstrumentMismatch);
        }
        if trade.side != order.side {
            return Err(Stage5gOrderPositionError::TradeSideMismatch);
        }
        let key = trade.broker_trade_id.as_str().to_string();
        match incoming_by_id.get_mut(&key) {
            Some(previous) if immutable_trade_payload_matches(previous, trade) => {
                if trade.received_ts > previous.received_ts {
                    previous.received_ts = trade.received_ts;
                }
            }
            Some(_) => {
                return Err(Stage5gOrderPositionError::TradeIdentityConflict);
            }
            None => {
                incoming_by_id.insert(key, trade.clone());
            }
        }
    }
    for incoming in incoming_by_id.into_values() {
        if let Some(previous) = slot
            .trades
            .iter_mut()
            .find(|previous| previous.broker_trade_id == incoming.broker_trade_id)
        {
            if !immutable_trade_payload_matches(previous, &incoming) {
                return Err(Stage5gOrderPositionError::TradeIdentityConflict);
            }
            if incoming.received_ts > previous.received_ts {
                previous.received_ts = incoming.received_ts;
            }
        } else {
            slot.trades.push(incoming);
        }
    }
    slot.trades.sort_by(|left, right| {
        left.broker_trade_id
            .as_str()
            .cmp(right.broker_trade_id.as_str())
    });
    let trade_qty: Decimal = slot.trades.iter().map(|trade| trade.qty).sum();
    if order.filled_qty > Decimal::ZERO && trade_qty != order.filled_qty {
        return Err(Stage5gOrderPositionError::TradeQuantityMismatch);
    }
    Ok(())
}

fn validate_source_position(
    slot: &Stage5gOrderPositionSlot,
    position: &BrokerPositionSnapshot,
    order: Option<&BrokerOrderSnapshot>,
) -> Result<bool, Stage5gOrderPositionError> {
    let qty = position
        .qty
        .to_f64()
        .ok_or(Stage5gOrderPositionError::NumericConversionFailed)?;
    let target = slot
        .source
        .target_qty
        .ok_or(Stage5gOrderPositionError::PositionIncomplete)?
        .abs();
    match slot.source.intent_class {
        crate::BrokerNeutralHybridIntentClass::Entry => {
            let expected_side = slot
                .source
                .side
                .ok_or(Stage5gOrderPositionError::PositionSideMismatch)?;
            let signed = match expected_side {
                crate::BrokerNeutralOrderSide::Buy => qty > f64::EPSILON,
                crate::BrokerNeutralOrderSide::Sell => qty < -f64::EPSILON,
            };
            if !signed {
                return Err(Stage5gOrderPositionError::PositionSideMismatch);
            }
            if qty.abs() > target + f64::EPSILON
                || order.is_some_and(|order| position.qty.abs() > order.qty)
            {
                return Err(Stage5gOrderPositionError::PositionOverfill);
            }
            let previous = slot
                .position
                .as_ref()
                .and_then(|(_, position)| position.qty.to_f64())
                .unwrap_or(slot.source.pre_position_qty);
            if qty.abs() + f64::EPSILON < previous.abs() {
                return Err(Stage5gOrderPositionError::PositionQuantityRegression);
            }
            Ok((qty.abs() - target).abs() <= f64::EPSILON)
        }
        crate::BrokerNeutralHybridIntentClass::Exit => {
            let previous = slot
                .position
                .as_ref()
                .and_then(|(_, position)| position.qty.to_f64())
                .unwrap_or(slot.source.pre_position_qty);
            if previous.abs() <= f64::EPSILON && qty.abs() > f64::EPSILON {
                return Err(Stage5gOrderPositionError::PositionQuantityRegression);
            }
            if qty.abs() <= f64::EPSILON {
                return Ok(true);
            }
            let pre = slot.source.pre_position_qty;
            if pre.abs() <= f64::EPSILON
                || qty.signum() != pre.signum()
                || qty.abs() > pre.abs() + f64::EPSILON
            {
                return Err(Stage5gOrderPositionError::PositionSideMismatch);
            }
            if qty.abs() > previous.abs() + f64::EPSILON {
                return Err(Stage5gOrderPositionError::PositionQuantityRegression);
            }
            Ok(false)
        }
        crate::BrokerNeutralHybridIntentClass::ProtectiveRepair => {
            if qty.abs() <= f64::EPSILON {
                Ok(true)
            } else {
                Err(Stage5gOrderPositionError::PositionIncomplete)
            }
        }
        crate::BrokerNeutralHybridIntentClass::CancelCleanup => {
            Err(Stage5gOrderPositionError::PositionSideMismatch)
        }
    }
}

fn validate_partial_terminal_position(
    slot: &Stage5gOrderPositionSlot,
    position: &BrokerPositionSnapshot,
    order: &BrokerOrderSnapshot,
) -> Result<(), Stage5gOrderPositionError> {
    let filled = order
        .filled_qty
        .to_f64()
        .ok_or(Stage5gOrderPositionError::NumericConversionFailed)?;
    let signed_fill = match order.side {
        OrderSide::Buy => filled,
        OrderSide::Sell => -filled,
    };
    let expected = slot.source.pre_position_qty + signed_fill;
    if decimal_f64_differs(position.qty, expected) {
        return Err(Stage5gOrderPositionError::PositionIncomplete);
    }
    Ok(())
}

fn validate_order_position_coherence(
    slot: &Stage5gOrderPositionSlot,
    position: &BrokerPositionSnapshot,
    order: &BrokerOrderSnapshot,
) -> Result<(), Stage5gOrderPositionError> {
    let pre_position = Decimal::from_f64_retain(slot.source.pre_position_qty)
        .ok_or(Stage5gOrderPositionError::NumericConversionFailed)?;
    let signed_fill = match order.side {
        OrderSide::Buy => order.filled_qty,
        OrderSide::Sell => -order.filled_qty,
    };
    if position.qty != pre_position + signed_fill {
        return Err(Stage5gOrderPositionError::OrderPositionIncoherent);
    }
    Ok(())
}

fn store_canonical_position(
    slot: &mut Stage5gOrderPositionSlot,
    total_sequence: u64,
    position: CanonicalTargetPosition,
) {
    slot.position = Some((total_sequence, position.snapshot));
    slot.position_derivation = Some(position.derivation);
    slot.position_matching_row_count = Some(position.matching_row_count);
}

fn decimal_f64_differs(value: Decimal, expected: f64) -> bool {
    value
        .to_f64()
        .map(|actual| (actual - expected).abs() > f64::EPSILON)
        .unwrap_or(true)
}

fn has_non_target_active_order(
    session: &Stage5gOrderPositionSession,
    truth: &BrokerTruthSnapshot,
    current_slot: usize,
) -> bool {
    let expected_ids: Vec<_> = session
        .state
        .slots
        .iter()
        .filter_map(|slot| slot.broker_order_id.as_ref())
        .collect();
    truth.orders.iter().any(|order| {
        order.is_active_for_lifecycle()
            && !expected_ids
                .iter()
                .any(|expected| order.broker_order_id.as_ref() == Some(*expected))
            && order.broker_order_id != session.state.slots[current_slot].broker_order_id
    })
}

#[cfg(test)]
fn has_non_target_active_order_for_slots(
    slots: &[Stage5gOrderPositionSlot],
    truth: &BrokerTruthSnapshot,
) -> bool {
    let expected_ids: Vec<_> = slots
        .iter()
        .filter_map(|slot| slot.broker_order_id.as_ref())
        .collect();
    truth.orders.iter().any(|order| {
        order.is_active_for_lifecycle()
            && !expected_ids
                .iter()
                .any(|expected| order.broker_order_id.as_ref() == Some(*expected))
    })
}

fn converge_through_stage5c(
    session: Stage5gOrderPositionSession,
    pre_candidate_state: Stage5gOrderPositionState,
) -> Result<Stage5gOrderPositionTransition, Stage5gOrderPositionFailure> {
    let market_terminal_slots: Vec<_> = session
        .state
        .slots
        .iter()
        .enumerate()
        .filter(|(_, slot)| slot.market_terminal_truth.is_some())
        .map(|(index, _)| index)
        .collect();
    if market_terminal_slots.len() > 1 {
        return Err(block(
            Stage5gOrderPositionError::Stage5cPreCallbackBlocked,
            Stage5gOrderPositionSession {
                ack_resolved: session.ack_resolved,
                state: pre_candidate_state,
            },
        ));
    }
    if let Some(slot_index) = market_terminal_slots.first().copied() {
        return converge_market_terminal_through_r3(session, pre_candidate_state, slot_index);
    }

    let mut records = Vec::new();
    for slot in &session.state.slots {
        for event in slot
            .order_events
            .iter()
            .filter(|_| !matches!(slot.source.base_action, Stage5gSourceBaseAction::Market))
        {
            let total_sequence = event
                .total_sequence
                .checked_mul(2)
                .ok_or_else(sequence_mapping_terminal)?;
            records.push(Stage5cPaperBrokerEventRecord {
                total_sequence,
                request_id: slot.ack.request_id,
                payload: Stage5cPaperBrokerEventPayload::Order(order_event(
                    event,
                    slot.ack.request_id,
                )?),
            });
        }
        if let Some((sequence, position)) = &slot.position {
            let total_sequence = sequence
                .checked_mul(2)
                .and_then(|value| value.checked_add(1))
                .ok_or_else(sequence_mapping_terminal)?;
            records.push(Stage5cPaperBrokerEventRecord {
                total_sequence,
                request_id: slot.ack.request_id,
                payload: Stage5cPaperBrokerEventPayload::Position(position_event(position)?),
            });
        }
    }
    records.sort_by_key(|record| record.total_sequence);
    let summary = state_summary(&session.state, records.len());
    let Stage5gOrderPositionSession {
        ack_resolved,
        state: _,
    } = session;
    let (stage5c_resolved, context) = ack_resolved.into_stage5g_c_parts();
    match resolve_stage5c_paper_broker_lifecycle(
        stage5c_resolved,
        Stage5cPaperBrokerLifecycleInput {
            event_records: records,
        },
    ) {
        Ok(resolved) => {
            if !resolved.remaining_lifecycle_expectations().is_empty() {
                return Err(Stage5gOrderPositionFailure::Terminal(
                    Stage5gOrderPositionTerminal {
                        reason: Stage5gOrderPositionError::Stage5cRemainingLifecycle,
                        stage5c_reason:
                            Stage5cPaperBrokerLifecycleError::MissingExpectedBrokerEvent,
                    },
                ));
            }
            Ok(Stage5gOrderPositionTransition::Converged(
                Stage5gConvergedPaperStrategy { resolved, summary },
            ))
        }
        Err(failure) => {
            let stage5c_reason = failure.reason();
            match failure.into_blocked() {
                Some(blocked) => {
                    let ack_resolved = Stage5gResolvedMockAckPaperStrategy::from_stage5g_c_parts(
                        blocked.into_resolved(),
                        context,
                    );
                    Err(block(
                        Stage5gOrderPositionError::Stage5cPreCallbackBlocked,
                        Stage5gOrderPositionSession {
                            ack_resolved,
                            state: pre_candidate_state,
                        },
                    ))
                }
                None => Err(Stage5gOrderPositionFailure::Terminal(
                    Stage5gOrderPositionTerminal {
                        reason: Stage5gOrderPositionError::Stage5cCallbackTerminal,
                        stage5c_reason,
                    },
                )),
            }
        }
    }
}

fn converge_market_terminal_through_r3(
    session: Stage5gOrderPositionSession,
    pre_candidate_state: Stage5gOrderPositionState,
    slot_index: usize,
) -> Result<Stage5gOrderPositionTransition, Stage5gOrderPositionFailure> {
    let slot = &session.state.slots[slot_index];
    let evidence = Stage5cMarketTerminalOrderEvidence {
        request_id: slot.ack.request_id,
        truth: slot
            .market_terminal_truth
            .clone()
            .expect("R3 convergence requires retained terminal truth"),
        attribution: slot.source.expected_attribution.clone(),
    };
    let summary = state_summary(&session.state, 1);
    let Stage5gOrderPositionSession {
        ack_resolved,
        state: _,
    } = session;
    let (stage5c_resolved, context) = ack_resolved.into_stage5g_c_parts();

    // These exact accepted R3 entry points are the only MARKET terminal
    // authority reachable from R2-c-b.
    let validated = match validate_stage5c_market_terminal_outcome_r3(stage5c_resolved, evidence) {
        Ok(validated) => validated,
        Err(blocked) => {
            let ack_resolved = Stage5gResolvedMockAckPaperStrategy::from_stage5g_c_parts(
                blocked.into_resolved(),
                context,
            );
            return Err(block(
                Stage5gOrderPositionError::Stage5cPreCallbackBlocked,
                Stage5gOrderPositionSession {
                    ack_resolved,
                    state: pre_candidate_state,
                },
            ));
        }
    };
    match settle_stage5c_validated_market_terminal_outcome_r3(validated) {
        Ok(settlement) => Ok(Stage5gOrderPositionTransition::MarketTerminalConverged(
            Stage5gMarketTerminalConvergedPaperStrategy {
                settlement,
                summary,
            },
        )),
        Err(blocked) => {
            let ack_resolved = Stage5gResolvedMockAckPaperStrategy::from_stage5g_c_parts(
                blocked.into_resolved(),
                context,
            );
            Err(block(
                Stage5gOrderPositionError::Stage5cPreCallbackBlocked,
                Stage5gOrderPositionSession {
                    ack_resolved,
                    state: pre_candidate_state,
                },
            ))
        }
    }
}

fn order_event(
    event: &CanonicalOrderEvent,
    request_id: StrategyRequestId,
) -> Result<HybridRuntimeOrderEvent, Stage5gOrderPositionFailure> {
    let order = &event.order;
    Ok(HybridRuntimeOrderEvent {
        order_id: order.broker_order_id.clone().ok_or_else(numeric_terminal)?,
        request_id: Some(request_id),
        instrument: order.instrument.clone(),
        status: order_status_name(&order.status),
        side: side_name(order.side).to_string(),
        order_type: order_type_name(order.order_type).to_string(),
        qty: decimal_to_f64(order.qty)?,
        filled_qty: decimal_to_f64(order.filled_qty)?,
        price: decimal_to_f64(order.limit_price.unwrap_or(Decimal::ZERO))?,
        existing: true,
        attribution: event.attribution.clone(),
        source_ts_utc: order.source_ts.unwrap_or(order.received_ts).timestamp(),
    })
}

fn position_event(
    position: &BrokerPositionSnapshot,
) -> Result<HybridRuntimePositionEvent, Stage5gOrderPositionFailure> {
    Ok(HybridRuntimePositionEvent {
        instrument: position.instrument.clone(),
        qty: decimal_to_f64(position.qty)?,
        existing: true,
        avg_price: decimal_to_f64(position.avg_price.unwrap_or(Decimal::ZERO))?,
        source_ts_utc: position
            .source_ts
            .unwrap_or(position.received_ts)
            .timestamp(),
    })
}

fn decimal_to_f64(value: Decimal) -> Result<f64, Stage5gOrderPositionFailure> {
    value.to_f64().ok_or_else(numeric_terminal)
}

fn numeric_terminal() -> Stage5gOrderPositionFailure {
    Stage5gOrderPositionFailure::Terminal(Stage5gOrderPositionTerminal {
        reason: Stage5gOrderPositionError::NumericConversionFailed,
        stage5c_reason: Stage5cPaperBrokerLifecycleError::CallbackValidationFailed,
    })
}

fn sequence_mapping_terminal() -> Stage5gOrderPositionFailure {
    Stage5gOrderPositionFailure::Terminal(Stage5gOrderPositionTerminal {
        reason: Stage5gOrderPositionError::SequenceMappingOverflow,
        stage5c_reason: Stage5cPaperBrokerLifecycleError::DuplicateSequence,
    })
}

fn parse_side(side: &str) -> Option<OrderSide> {
    match side.to_ascii_lowercase().as_str() {
        "buy" => Some(OrderSide::Buy),
        "sell" => Some(OrderSide::Sell),
        _ => None,
    }
}

fn side_name(side: OrderSide) -> &'static str {
    match side {
        OrderSide::Buy => "buy",
        OrderSide::Sell => "sell",
    }
}

fn order_type_name(order_type: OrderType) -> &'static str {
    match order_type {
        OrderType::Market => "market",
        OrderType::Limit => "limit",
        OrderType::Stop => "stop",
        OrderType::StopLimit => "stop_limit",
        OrderType::TakeProfit => "take_profit",
        OrderType::TakeProfitLimit => "take_profit_limit",
    }
}

fn order_status_name(status: &OrderStatus) -> String {
    match status {
        OrderStatus::New => "new",
        OrderStatus::Working => "working",
        OrderStatus::PartiallyFilled => "partially_filled",
        OrderStatus::Filled => "filled",
        OrderStatus::Canceled => "canceled",
        OrderStatus::Rejected => "rejected",
        OrderStatus::Expired => "expired",
        OrderStatus::Unknown(value) => value,
    }
    .to_string()
}

fn evidence_identity(evidence: &Stage5gOrderPositionEvidence) -> String {
    format!(
        "moex.stage5g.order-position-evidence-identity.v2:{}:{}:{}",
        evidence.request_id,
        evidence.broker_truth.account_id,
        evidence.broker_truth.received_ts.timestamp_millis()
    )
}

fn evidence_fingerprint(evidence: &Stage5gOrderPositionEvidence) -> String {
    let mut truth = evidence.broker_truth.clone();
    canonical_json_sort(&mut truth.orders);
    canonical_json_sort(&mut truth.trades);
    canonical_json_sort(&mut truth.positions);
    canonical_json_sort(&mut truth.instruments);
    if let Some(cash) = truth.cash.as_mut() {
        canonical_json_sort(&mut cash.cash);
    }
    let projection = serde_json::json!({
        "schema_version": STAGE5G_EVIDENCE_FINGERPRINT_SCHEMA_VERSION,
        "domain": "moex.stage5g.order-position-evidence.v2",
        "request_id": evidence.request_id,
        "broker_truth": truth,
        "receipt_watermark_ms": evidence.broker_truth.received_ts.timestamp_millis(),
        "attribution": evidence
            .order_attribution
            .as_ref()
            .map(HybridRuntimeAttribution::internal_comment),
    });
    let mut hasher = Sha256::new();
    hasher.update(b"moex.stage5g.order-position-evidence.v2\0");
    hasher
        .update(serde_json::to_vec(&projection).expect("canonical Stage 5G-c evidence serializes"));
    format!("{:x}", hasher.finalize())
}

fn state_summary(
    state: &Stage5gOrderPositionState,
    callback_count: usize,
) -> Stage5gOrderPositionSummary {
    let mut summary = Stage5gOrderPositionSummary {
        schema_version: STAGE5G_ORDER_POSITION_SCHEMA_VERSION,
        strategy_id: state.strategy_id.clone(),
        request_count: state.slots.len(),
        terminal_request_count: state.slots.iter().filter(|slot| slot.terminal).count(),
        order_transition_count: state.slots.iter().map(|slot| slot.order_events.len()).sum(),
        correlated_trade_count: state.slots.iter().map(|slot| slot.trades.len()).sum(),
        position_confirmation_count: state
            .slots
            .iter()
            .filter(|slot| slot.position.is_some())
            .count(),
        duplicate_evidence_count: state.duplicate_evidence_count,
        last_total_sequence: state.last_total_sequence,
        lifecycle_fingerprint_sha256: String::new(),
        stage5c_callback_count: callback_count,
        mock_feedback_only: true,
        redis_attached: false,
        finam_transport_attached: false,
        broker_execution_attached: false,
    };
    summary.lifecycle_fingerprint_sha256 = lifecycle_state_fingerprint(state, callback_count);
    summary
}

fn lifecycle_state_fingerprint(state: &Stage5gOrderPositionState, callback_count: usize) -> String {
    let slots: Vec<_> = state
        .slots
        .iter()
        .map(|slot| {
            let orders: Vec<_> = slot
                .order_events
                .iter()
                .map(|event| {
                    let order = &event.order;
                    serde_json::json!({
                        "total_sequence": event.total_sequence,
                        "account_id": order.account_id,
                        "broker_order_id_hash": order.broker_order_id.as_ref().map(|id| exact_id_hash("broker-order", id.as_str())),
                        "client_order_id_hash": order.client_order_id.as_ref().map(|id| exact_id_hash("client-order", id.as_str())),
                        "instrument": order.instrument,
                        "side": order.side,
                        "order_type": order.order_type,
                        "time_in_force": order.time_in_force,
                        "status": order.status,
                        "lifecycle": order.lifecycle,
                        "qty": order.qty,
                        "filled_qty": order.filled_qty,
                        "remaining_qty": order.remaining_qty,
                        "limit_price": order.limit_price,
                        "broker_asset_id": order.broker_asset_id,
                        "board": order.board,
                        "expiration_date": order.expiration_date,
                        "source_ts": order.source_ts,
                        "received_ts_ms": order.received_ts.timestamp_millis(),
                        "attribution_hash": event.attribution.as_ref().map(|value| exact_id_hash("attribution", value.internal_comment())),
                    })
                })
                .collect();
            let trades: Vec<_> = slot
                .trades
                .iter()
                .map(|trade| {
                    serde_json::json!({
                        "broker_trade_id_hash": exact_id_hash("broker-trade", trade.broker_trade_id.as_str()),
                        "account_id": trade.account_id,
                        "broker_order_id_hash": trade.broker_order_id.as_ref().map(|id| exact_id_hash("broker-order", id.as_str())),
                        "client_order_id_hash": trade.client_order_id.as_ref().map(|id| exact_id_hash("client-order", id.as_str())),
                        "instrument": trade.instrument,
                        "side": trade.side,
                        "qty": trade.qty,
                        "price": trade.price,
                        "gross_amount": trade.gross_amount,
                        "commission": trade.commission,
                        "broker_asset_id": trade.broker_asset_id,
                        "board": trade.board,
                        "expiration_date": trade.expiration_date,
                        "source_ts": trade.source_ts,
                        "last_observed_received_ms": trade.received_ts.timestamp_millis(),
                    })
                })
                .collect();
            let position = slot.position.as_ref().map(|(sequence, position)| {
                serde_json::json!({
                    "total_sequence": sequence,
                    "account_id": position.account_id,
                    "instrument": position.instrument,
                    "qty": position.qty,
                    "avg_price": position.avg_price,
                    "unrealized_pnl": position.unrealized_pnl,
                    "source_ts": position.source_ts,
                    "received_ts_ms": position.received_ts.timestamp_millis(),
                    "derivation": slot.position_derivation,
                    "matching_row_count": slot.position_matching_row_count,
                })
            });
            serde_json::json!({
                "request_id": slot.ack.request_id,
                "ack_intent_class": slot.ack.intent_class,
                "ack_action": slot.ack.action,
                "ack_side": slot.ack.side,
                "ack_latest_status": slot.ack.latest_status,
                "ack_latest_reason_code": slot.ack.latest_reason_code,
                "ack_latest_received_ts_utc": slot.ack.latest_received_ts_utc,
                "ack_canonical_total_sequence": slot.ack.canonical_total_sequence,
                "expected_client_order_id_hash": exact_id_hash("client-order", slot.ack.expected_client_order_id.as_str()),
                "broker_order_id_hash": slot.broker_order_id.as_ref().map(|id| exact_id_hash("broker-order", id.as_str())),
                "source": {
                    "intent_class": intent_class_name(slot.source.intent_class),
                    "base_action": source_action_name(slot.source.base_action),
                    "side": slot.source.side.map(source_side_name),
                    "target_qty": slot.source.target_qty,
                    "pre_position_qty": slot.source.pre_position_qty,
                    "expected_attribution_hash": slot.source.expected_attribution.as_ref().map(|value| exact_id_hash("attribution", value.internal_comment())),
                },
                "orders": orders,
                "trades": trades,
                "position": position,
                "market_terminal_truth_receipt_ms": slot.market_terminal_truth.as_ref().map(|truth| truth.received_ts.timestamp_millis()),
                "terminal": slot.terminal,
                "last_order_source_ts": slot.last_order_source_ts,
                "last_order_received_ts": slot.last_order_received_ts,
                "last_trade_source_ts": slot.last_trade_source_ts,
                "last_trade_received_ts": slot.last_trade_received_ts,
                "last_position_source_ts": slot.last_position_source_ts,
                "last_position_received_ts": slot.last_position_received_ts,
            })
        })
        .collect();
    let projection = serde_json::json!({
        "schema_version": STAGE5G_ORDER_POSITION_SCHEMA_VERSION,
        "strategy_id": state.strategy_id,
        "account_id": state.account_id,
        "instrument": state.instrument,
        "slots": slots,
        "evidence_replay_ledger": state.evidence_identities.iter().map(|item| (&item.identity, &item.fingerprint)).collect::<Vec<_>>(),
        "last_total_sequence": state.last_total_sequence,
        "last_broker_truth_received_ms": state.last_broker_truth_received_ms,
        "duplicate_evidence_count": state.duplicate_evidence_count,
        "stage5c_callback_count": callback_count,
    });
    let mut hasher = Sha256::new();
    hasher.update(b"moex.stage5g.order-position-lifecycle.v3\0");
    hasher.update(serde_json::to_vec(&projection).expect("Stage 5G-c v3 state serializes"));
    format!("{:x}", hasher.finalize())
}

fn exact_id_hash(domain: &str, value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"moex.stage5g.exact-id.v1\0");
    hasher.update(domain.as_bytes());
    hasher.update(b"\0");
    hasher.update(value.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn intent_class_name(value: crate::BrokerNeutralHybridIntentClass) -> &'static str {
    match value {
        crate::BrokerNeutralHybridIntentClass::Entry => "entry",
        crate::BrokerNeutralHybridIntentClass::Exit => "exit",
        crate::BrokerNeutralHybridIntentClass::CancelCleanup => "cancel_cleanup",
        crate::BrokerNeutralHybridIntentClass::ProtectiveRepair => "protective_repair",
    }
}

fn source_action_name(value: Stage5gSourceBaseAction) -> &'static str {
    match value {
        Stage5gSourceBaseAction::Market => "market",
        Stage5gSourceBaseAction::Place => "place",
        Stage5gSourceBaseAction::Cancel => "cancel",
        Stage5gSourceBaseAction::Replace => "replace",
        Stage5gSourceBaseAction::CreateStopLimit => "create_stop_limit",
        Stage5gSourceBaseAction::DeleteStopLimit => "delete_stop_limit",
    }
}

fn source_side_name(value: crate::BrokerNeutralOrderSide) -> &'static str {
    match value {
        crate::BrokerNeutralOrderSide::Buy => "buy",
        crate::BrokerNeutralOrderSide::Sell => "sell",
    }
}

fn admission_block(
    reason: Stage5gOrderPositionAdmissionError,
    ack_resolved: Stage5gResolvedMockAckPaperStrategy,
) -> Box<Stage5gOrderPositionAdmissionBlocked> {
    Box::new(Stage5gOrderPositionAdmissionBlocked {
        reason,
        ack_resolved,
    })
}

fn block(
    reason: Stage5gOrderPositionError,
    session: Stage5gOrderPositionSession,
) -> Stage5gOrderPositionFailure {
    Stage5gOrderPositionFailure::Blocked(Box::new(Stage5gOrderPositionBlocked { reason, session }))
}

impl Stage5gOrderPositionSession {
    pub fn summary(&self) -> Stage5gOrderPositionSummary {
        state_summary(&self.state, 0)
    }

    pub fn intent_sink_attached(&self) -> bool {
        false
    }
    pub fn redis_command_stream_attached(&self) -> bool {
        false
    }
    pub fn broker_transport_attached(&self) -> bool {
        false
    }
    pub fn broker_execution_attached(&self) -> bool {
        false
    }
}

impl Stage5gConvergedPaperStrategy {
    pub fn summary(&self) -> &Stage5gOrderPositionSummary {
        &self.summary
    }
    pub fn broker_lifecycle(&self) -> &Stage5cBrokerLifecycleResolvedPaperStrategy {
        &self.resolved
    }
    pub fn intent_sink_attached(&self) -> bool {
        false
    }
    pub fn redis_command_stream_attached(&self) -> bool {
        false
    }
    pub fn broker_transport_attached(&self) -> bool {
        false
    }
    pub fn broker_execution_attached(&self) -> bool {
        false
    }
}

impl Stage5gMarketTerminalConvergedPaperStrategy {
    pub fn summary(&self) -> &Stage5gOrderPositionSummary {
        &self.summary
    }
    pub fn settlement(&self) -> &Stage5cBrokerLifecycleSettlement {
        &self.settlement
    }
    pub fn intent_sink_attached(&self) -> bool {
        false
    }
    pub fn redis_command_stream_attached(&self) -> bool {
        false
    }
    pub fn broker_transport_attached(&self) -> bool {
        false
    }
    pub fn broker_execution_attached(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use broker_core::{
        BrokerAccountId, BrokerOrderId, BrokerTradeId, ClientOrderId, Exchange, Market, TimeInForce,
    };
    use chrono::{TimeZone, Utc};

    use super::*;

    fn target() -> InstrumentId {
        InstrumentId {
            symbol: "IMOEXF".to_string(),
            venue_symbol: Some("IMOEXF@RTSX".to_string()),
            exchange: Exchange::Moex,
            market: Market::Futures,
        }
    }

    fn other() -> InstrumentId {
        InstrumentId {
            symbol: "USDRUBF".to_string(),
            venue_symbol: Some("USDRUBF@RTSX".to_string()),
            exchange: Exchange::Moex,
            market: Market::Futures,
        }
    }

    fn ts(second: i64) -> chrono::DateTime<Utc> {
        Utc.timestamp_opt(1_767_679_800 + second, 0).unwrap()
    }

    fn slot() -> Stage5gOrderPositionSlot {
        let request_id = StrategyRequestId::new(uuid::Uuid::from_u128(1));
        Stage5gOrderPositionSlot {
            ack: Stage5gMockAckSlotSummary {
                request_id,
                expected_client_order_id: ClientOrderId::from_strategy_request(request_id),
                intent_class: "Entry".to_string(),
                action: Stage5gMockIntentAction::Place {
                    place_kind: Stage5gMockPlaceKind::Limit,
                },
                side: Some("Buy".to_string()),
                source_event_ts_utc: ts(0).timestamp(),
                state: crate::Stage5gMockAckSlotState::Resolved,
                latest_status: Some(CommandAckStatus::Accepted),
                latest_reason_code: None,
                latest_received_ts_utc: Some(ts(1).to_rfc3339()),
                canonical_total_sequence: Some(1),
                pending_disposition: None,
                status_policy: None,
                broker_order_id_domain_sha256: Some("redacted".to_string()),
            },
            source: Stage5gSourceIntentProjection {
                request_id,
                intent_class: crate::BrokerNeutralHybridIntentClass::Entry,
                base_action: Stage5gSourceBaseAction::Place,
                side: Some(crate::BrokerNeutralOrderSide::Buy),
                target_qty: Some(1.0),
                pre_position_qty: 0.0,
                expected_attribution: None,
            },
            broker_order_id: Some(BrokerOrderId::new("ORDER_TARGET_1")),
            order_events: Vec::new(),
            trades: Vec::new(),
            position: None,
            position_derivation: None,
            position_matching_row_count: None,
            market_terminal_truth: None,
            last_order_source_ts: None,
            last_order_received_ts: None,
            last_trade_source_ts: None,
            last_trade_received_ts: None,
            last_position_source_ts: None,
            last_position_received_ts: None,
            terminal: false,
        }
    }

    fn market_slot(
        intent_class: crate::BrokerNeutralHybridIntentClass,
        side: crate::BrokerNeutralOrderSide,
        target_qty: f64,
        pre_position_qty: f64,
    ) -> Stage5gOrderPositionSlot {
        let mut slot = slot();
        slot.ack.intent_class = intent_class_name(intent_class).to_string();
        slot.ack.action = Stage5gMockIntentAction::Place {
            place_kind: Stage5gMockPlaceKind::Market,
        };
        slot.ack.side = Some(source_side_name(side).to_string());
        slot.source.intent_class = intent_class;
        slot.source.base_action = Stage5gSourceBaseAction::Market;
        slot.source.side = Some(side);
        slot.source.target_qty = Some(target_qty);
        slot.source.pre_position_qty = pre_position_qty;
        slot.broker_order_id = Some(BrokerOrderId::new("ORDER_MARKET_1"));
        slot
    }

    fn state_with_evidence(event: &Stage5gOrderPositionEvidence) -> Stage5gOrderPositionState {
        Stage5gOrderPositionState {
            strategy_id: "hybrid_imoexf".to_string(),
            account_id: BrokerAccountId::new("ACC_TEST_0001"),
            instrument: target(),
            slots: vec![slot()],
            evidence_identities: vec![EvidenceIdentity {
                identity: evidence_identity(event),
                fingerprint: evidence_fingerprint(event),
            }],
            last_total_sequence: Some(event.total_sequence),
            last_broker_truth_received_ms: Some(event.broker_truth.received_ts.timestamp_millis()),
            duplicate_evidence_count: 0,
        }
    }

    fn order(status: OrderStatus, filled_qty: Decimal, second: i64) -> BrokerOrderSnapshot {
        let qty = Decimal::ONE;
        BrokerOrderSnapshot {
            account_id: BrokerAccountId::new("ACC_TEST_0001"),
            broker_order_id: Some(BrokerOrderId::new("ORDER_TARGET_1")),
            client_order_id: Some(slot().ack.expected_client_order_id),
            instrument: target(),
            side: OrderSide::Buy,
            order_type: OrderType::Limit,
            time_in_force: Some(TimeInForce::Day),
            lifecycle: BrokerOrderSnapshot::lifecycle_for(&status),
            status,
            qty,
            filled_qty,
            remaining_qty: Some(qty - filled_qty),
            limit_price: Some(Decimal::new(2_210, 0)),
            broker_asset_id: None,
            board: None,
            expiration_date: None,
            source_ts: Some(ts(second)),
            received_ts: ts(second),
        }
    }

    fn trade(id: &str, qty: Decimal, second: i64) -> BrokerTradeSnapshot {
        BrokerTradeSnapshot {
            account_id: BrokerAccountId::new("ACC_TEST_0001"),
            broker_trade_id: BrokerTradeId::new(id),
            broker_order_id: Some(BrokerOrderId::new("ORDER_TARGET_1")),
            client_order_id: Some(slot().ack.expected_client_order_id),
            instrument: target(),
            side: OrderSide::Buy,
            qty,
            price: Decimal::new(2_210, 0),
            gross_amount: None,
            commission: None,
            broker_asset_id: None,
            board: None,
            expiration_date: None,
            source_ts: ts(second),
            received_ts: ts(second),
        }
    }

    fn market_order(status: OrderStatus, filled_qty: Decimal, second: i64) -> BrokerOrderSnapshot {
        let mut order = order(status, filled_qty, second);
        let market = market_slot(
            crate::BrokerNeutralHybridIntentClass::Entry,
            crate::BrokerNeutralOrderSide::Buy,
            1.0,
            0.0,
        );
        order.broker_order_id = market.broker_order_id;
        order.client_order_id = Some(market.ack.expected_client_order_id);
        order.order_type = OrderType::Market;
        order.limit_price = None;
        order
    }

    fn market_trade(id: &str, qty: Decimal, second: i64) -> BrokerTradeSnapshot {
        let mut trade = trade(id, qty, second);
        let market = market_slot(
            crate::BrokerNeutralHybridIntentClass::Entry,
            crate::BrokerNeutralOrderSide::Buy,
            1.0,
            0.0,
        );
        trade.broker_order_id = market.broker_order_id;
        trade.client_order_id = Some(market.ack.expected_client_order_id);
        trade
    }

    fn position(qty: Decimal, second: i64) -> BrokerPositionSnapshot {
        BrokerPositionSnapshot {
            account_id: BrokerAccountId::new("ACC_TEST_0001"),
            instrument: target(),
            qty,
            avg_price: Some(Decimal::new(2_210, 0)),
            unrealized_pnl: None,
            source_ts: Some(ts(second)),
            received_ts: ts(second),
        }
    }

    fn truth(
        orders: Vec<BrokerOrderSnapshot>,
        trades: Vec<BrokerTradeSnapshot>,
        positions: Vec<BrokerPositionSnapshot>,
        second: i64,
    ) -> BrokerTruthSnapshot {
        BrokerTruthSnapshot {
            account_id: BrokerAccountId::new("ACC_TEST_0001"),
            orders,
            positions,
            cash: None,
            trades,
            instruments: Vec::new(),
            received_ts: ts(second),
        }
    }

    fn evidence(sequence: u64, truth: BrokerTruthSnapshot) -> Stage5gOrderPositionEvidence {
        Stage5gOrderPositionEvidence {
            total_sequence: sequence,
            request_id: slot().ack.request_id,
            broker_truth: truth,
            order_attribution: None,
        }
    }

    #[test]
    fn gop01_working_order_remains_active() {
        let mut slot = slot();
        let evidence = evidence(
            2,
            truth(
                vec![order(OrderStatus::Working, Decimal::ZERO, 2)],
                vec![],
                vec![],
                2,
            ),
        );
        apply_to_slot(
            &BrokerAccountId::new("ACC_TEST_0001"),
            &target(),
            &mut slot,
            &evidence,
        )
        .unwrap();
        assert!(!slot.terminal);
        assert_eq!(slot.order_events.len(), 1);
    }

    #[test]
    fn gop02_partial_fill_advances_monotonically() {
        let mut slot = slot();
        let first_qty = Decimal::new(4, 1);
        let first = evidence(
            2,
            truth(
                vec![order(OrderStatus::PartiallyFilled, first_qty, 2)],
                vec![trade("TRADE_1", first_qty, 2)],
                vec![],
                2,
            ),
        );
        apply_to_slot(
            &BrokerAccountId::new("ACC_TEST_0001"),
            &target(),
            &mut slot,
            &first,
        )
        .unwrap();
        let second_qty = Decimal::new(7, 1);
        let second = evidence(
            3,
            truth(
                vec![order(OrderStatus::PartiallyFilled, second_qty, 3)],
                vec![
                    trade("TRADE_1", first_qty, 2),
                    trade("TRADE_2", Decimal::new(3, 1), 3),
                ],
                vec![],
                3,
            ),
        );
        apply_to_slot(
            &BrokerAccountId::new("ACC_TEST_0001"),
            &target(),
            &mut slot,
            &second,
        )
        .unwrap();
        assert_eq!(
            slot.order_events.last().unwrap().order.filled_qty,
            second_qty
        );
    }

    #[test]
    fn gop03_partial_fill_regression_blocks() {
        let mut slot = slot();
        let first_qty = Decimal::new(7, 1);
        let first = evidence(
            2,
            truth(
                vec![order(OrderStatus::PartiallyFilled, first_qty, 2)],
                vec![trade("TRADE_1", first_qty, 2)],
                vec![],
                2,
            ),
        );
        apply_to_slot(
            &BrokerAccountId::new("ACC_TEST_0001"),
            &target(),
            &mut slot,
            &first,
        )
        .unwrap();
        let regressed = Decimal::new(4, 1);
        let second = evidence(
            3,
            truth(
                vec![order(OrderStatus::PartiallyFilled, regressed, 3)],
                vec![trade("TRADE_1", regressed, 3)],
                vec![],
                3,
            ),
        );
        assert_eq!(
            apply_to_slot(
                &BrokerAccountId::new("ACC_TEST_0001"),
                &target(),
                &mut slot,
                &second
            ),
            Err(Stage5gOrderPositionError::FilledQuantityRegression)
        );
    }

    #[test]
    fn gop04_filled_requires_target_position_confirmation() {
        let mut slot = slot();
        let missing = evidence(
            2,
            truth(
                vec![order(OrderStatus::Filled, Decimal::ONE, 2)],
                vec![trade("TRADE_1", Decimal::ONE, 2)],
                vec![],
                2,
            ),
        );
        assert_eq!(
            apply_to_slot(
                &BrokerAccountId::new("ACC_TEST_0001"),
                &target(),
                &mut slot,
                &missing
            ),
            Err(Stage5gOrderPositionError::PositionSideMismatch)
        );
    }

    #[test]
    fn gop05_canceled_terminates_without_position_change() {
        let mut slot = slot();
        let event = evidence(
            2,
            truth(
                vec![order(OrderStatus::Canceled, Decimal::ZERO, 2)],
                vec![],
                vec![],
                2,
            ),
        );
        apply_to_slot(
            &BrokerAccountId::new("ACC_TEST_0001"),
            &target(),
            &mut slot,
            &event,
        )
        .unwrap();
        assert!(slot.terminal);
        assert!(slot.position.is_none());
    }

    #[test]
    fn gop06_rejected_terminates_without_position_change() {
        let mut slot = slot();
        let event = evidence(
            2,
            truth(
                vec![order(OrderStatus::Rejected, Decimal::ZERO, 2)],
                vec![],
                vec![],
                2,
            ),
        );
        apply_to_slot(
            &BrokerAccountId::new("ACC_TEST_0001"),
            &target(),
            &mut slot,
            &event,
        )
        .unwrap();
        assert!(slot.terminal);
    }

    #[test]
    fn gop07_expired_terminates_without_position_change() {
        let mut slot = slot();
        let event = evidence(
            2,
            truth(
                vec![order(OrderStatus::Expired, Decimal::ZERO, 2)],
                vec![],
                vec![],
                2,
            ),
        );
        apply_to_slot(
            &BrokerAccountId::new("ACC_TEST_0001"),
            &target(),
            &mut slot,
            &event,
        )
        .unwrap();
        assert!(slot.terminal);
    }

    #[test]
    fn gop08_unknown_order_status_blocks() {
        let mut slot = slot();
        let event = evidence(
            2,
            truth(
                vec![order(
                    OrderStatus::Unknown("broker_new".to_string()),
                    Decimal::ZERO,
                    2,
                )],
                vec![],
                vec![],
                2,
            ),
        );
        assert_eq!(
            apply_to_slot(
                &BrokerAccountId::new("ACC_TEST_0001"),
                &target(),
                &mut slot,
                &event
            ),
            Err(Stage5gOrderPositionError::UnknownOrderStatus)
        );
    }

    #[test]
    fn gop09_identical_event_replay_is_idempotent() {
        let event = evidence(
            2,
            truth(
                vec![order(OrderStatus::Working, Decimal::ZERO, 2)],
                vec![],
                vec![],
                2,
            ),
        );
        let state = state_with_evidence(&event);
        assert_eq!(
            classify_evidence_replay(
                &state,
                &evidence_identity(&event),
                &evidence_fingerprint(&event)
            ),
            Ok(true)
        );
    }

    #[test]
    fn gop10_conflicting_duplicate_event_is_detectable() {
        let first = evidence(
            2,
            truth(
                vec![order(OrderStatus::Working, Decimal::ZERO, 2)],
                vec![],
                vec![],
                2,
            ),
        );
        let second = evidence(
            3,
            truth(
                vec![order(OrderStatus::PartiallyFilled, Decimal::new(4, 1), 2)],
                vec![trade("TRADE_1", Decimal::new(4, 1), 2)],
                vec![],
                2,
            ),
        );
        let state = state_with_evidence(&first);
        assert_eq!(
            classify_evidence_replay(
                &state,
                &evidence_identity(&second),
                &evidence_fingerprint(&second)
            ),
            Err(Stage5gOrderPositionError::ConflictingDuplicateEvidence)
        );
    }

    #[test]
    fn gop11_non_target_event_cannot_settle_target() {
        let mut non_target = order(OrderStatus::Filled, Decimal::ONE, 2);
        non_target.instrument = other();
        non_target.broker_order_id = Some(BrokerOrderId::new("ORDER_OTHER_1"));
        assert_eq!(
            select_target_order(&slot(), &truth(vec![non_target], vec![], vec![], 2)),
            Err(Stage5gOrderPositionError::MissingTargetOrder)
        );
    }

    #[test]
    fn gop12_account_wide_active_order_is_safety_guard() {
        let mut non_target = order(OrderStatus::Working, Decimal::ZERO, 2);
        non_target.instrument = other();
        non_target.broker_order_id = Some(BrokerOrderId::new("ORDER_OTHER_1"));
        assert!(has_non_target_active_order_for_slots(
            &[slot()],
            &truth(vec![non_target], vec![], vec![], 2)
        ));
    }

    #[test]
    fn gop13_target_position_side_mismatch_blocks() {
        assert_eq!(
            validate_source_position(
                &slot(),
                &position(-Decimal::ONE, 2),
                Some(&order(OrderStatus::Filled, Decimal::ONE, 2))
            ),
            Err(Stage5gOrderPositionError::PositionSideMismatch)
        );
    }

    #[test]
    fn gop14_target_position_overfill_blocks() {
        assert_eq!(
            validate_source_position(
                &slot(),
                &position(Decimal::new(2, 0), 2),
                Some(&order(OrderStatus::Filled, Decimal::ONE, 2))
            ),
            Err(Stage5gOrderPositionError::PositionOverfill)
        );
    }

    #[test]
    fn gop15_correlated_trade_supports_fill_truth() {
        let mut slot = slot();
        let filled = order(OrderStatus::Filled, Decimal::ONE, 2);
        validate_trades(&mut slot, &filled, &[trade("TRADE_1", Decimal::ONE, 2)]).unwrap();
        assert_eq!(slot.trades.len(), 1);
    }

    #[test]
    fn gop16_trade_identity_or_quantity_mismatch_blocks() {
        let mut slot = slot();
        let filled = order(OrderStatus::Filled, Decimal::ONE, 2);
        assert_eq!(
            validate_trades(
                &mut slot,
                &filled,
                &[trade("TRADE_1", Decimal::new(5, 1), 2)]
            ),
            Err(Stage5gOrderPositionError::TradeQuantityMismatch)
        );
    }

    #[test]
    fn r1_market_entry_partial_awaits_and_exact_position_is_terminal() {
        let mut slot = market_slot(
            crate::BrokerNeutralHybridIntentClass::Entry,
            crate::BrokerNeutralOrderSide::Buy,
            1.0,
            0.0,
        );
        assert!(!validate_source_position(&slot, &position(Decimal::new(4, 1), 2), None).unwrap());
        let partial = evidence(
            2,
            truth(vec![], vec![], vec![position(Decimal::new(4, 1), 2)], 2),
        );
        apply_to_slot(
            &BrokerAccountId::new("ACC_TEST_0001"),
            &target(),
            &mut slot,
            &partial,
        )
        .unwrap();
        assert!(!slot.terminal);
        let exact = evidence(3, truth(vec![], vec![], vec![position(Decimal::ONE, 3)], 3));
        apply_to_slot(
            &BrokerAccountId::new("ACC_TEST_0001"),
            &target(),
            &mut slot,
            &exact,
        )
        .unwrap();
        assert!(slot.terminal);
    }

    #[test]
    fn r2b_market_entry_position_progress_is_monotonic() {
        let mut slot = market_slot(
            crate::BrokerNeutralHybridIntentClass::Entry,
            crate::BrokerNeutralOrderSide::Buy,
            1.0,
            0.0,
        );
        apply_to_slot(
            &BrokerAccountId::new("ACC_TEST_0001"),
            &target(),
            &mut slot,
            &evidence(
                2,
                truth(vec![], vec![], vec![position(Decimal::new(7, 1), 2)], 2),
            ),
        )
        .unwrap();
        assert_eq!(
            apply_to_slot(
                &BrokerAccountId::new("ACC_TEST_0001"),
                &target(),
                &mut slot,
                &evidence(
                    3,
                    truth(vec![], vec![], vec![position(Decimal::new(5, 1), 3)], 3),
                ),
            ),
            Err(Stage5gOrderPositionError::PositionQuantityRegression)
        );
        apply_to_slot(
            &BrokerAccountId::new("ACC_TEST_0001"),
            &target(),
            &mut slot,
            &evidence(4, truth(vec![], vec![], vec![position(Decimal::ONE, 4)], 4)),
        )
        .unwrap();
        assert!(slot.terminal);
    }

    #[test]
    fn r2b_market_exit_position_progress_is_monotonic_until_flat() {
        let mut slot = market_slot(
            crate::BrokerNeutralHybridIntentClass::Exit,
            crate::BrokerNeutralOrderSide::Sell,
            1.0,
            1.0,
        );
        apply_to_slot(
            &BrokerAccountId::new("ACC_TEST_0001"),
            &target(),
            &mut slot,
            &evidence(
                2,
                truth(vec![], vec![], vec![position(Decimal::new(4, 1), 2)], 2),
            ),
        )
        .unwrap();
        assert_eq!(
            apply_to_slot(
                &BrokerAccountId::new("ACC_TEST_0001"),
                &target(),
                &mut slot,
                &evidence(
                    3,
                    truth(vec![], vec![], vec![position(Decimal::new(6, 1), 3)], 3),
                ),
            ),
            Err(Stage5gOrderPositionError::PositionQuantityRegression)
        );
        apply_to_slot(
            &BrokerAccountId::new("ACC_TEST_0001"),
            &target(),
            &mut slot,
            &evidence(
                4,
                truth(vec![], vec![], vec![position(Decimal::ZERO, 4)], 4),
            ),
        )
        .unwrap();
        assert!(slot.terminal);
    }

    #[test]
    fn r2b_target_market_order_status_is_authoritative_when_present() {
        let account = BrokerAccountId::new("ACC_TEST_0001");
        let exact = position(Decimal::ONE, 2);

        let mut working_slot = market_slot(
            crate::BrokerNeutralHybridIntentClass::Entry,
            crate::BrokerNeutralOrderSide::Buy,
            1.0,
            0.0,
        );
        apply_to_slot(
            &account,
            &target(),
            &mut working_slot,
            &evidence(
                2,
                truth(
                    vec![market_order(OrderStatus::Working, Decimal::ZERO, 2)],
                    vec![],
                    vec![],
                    2,
                ),
            ),
        )
        .unwrap();
        assert!(
            !working_slot.terminal,
            "position cannot mask a working order"
        );

        let mut unknown_slot = market_slot(
            crate::BrokerNeutralHybridIntentClass::Entry,
            crate::BrokerNeutralOrderSide::Buy,
            1.0,
            0.0,
        );
        assert_eq!(
            apply_to_slot(
                &account,
                &target(),
                &mut unknown_slot,
                &evidence(
                    2,
                    truth(
                        vec![market_order(
                            OrderStatus::Unknown("broker_new".to_string()),
                            Decimal::ZERO,
                            2,
                        )],
                        vec![],
                        vec![],
                        2,
                    ),
                ),
            ),
            Err(Stage5gOrderPositionError::UnknownOrderStatus)
        );

        let mut rejected_slot = market_slot(
            crate::BrokerNeutralHybridIntentClass::Entry,
            crate::BrokerNeutralOrderSide::Buy,
            1.0,
            0.0,
        );
        apply_to_slot(
            &account,
            &target(),
            &mut rejected_slot,
            &evidence(
                2,
                truth(
                    vec![market_order(OrderStatus::Rejected, Decimal::ZERO, 2)],
                    vec![],
                    vec![],
                    2,
                ),
            ),
        )
        .unwrap();
        assert!(rejected_slot.terminal);
        assert!(rejected_slot.market_terminal_truth.is_some());

        let mut filled_slot = market_slot(
            crate::BrokerNeutralHybridIntentClass::Entry,
            crate::BrokerNeutralOrderSide::Buy,
            1.0,
            0.0,
        );
        apply_to_slot(
            &account,
            &target(),
            &mut filled_slot,
            &evidence(
                2,
                truth(
                    vec![market_order(OrderStatus::Filled, Decimal::ONE, 2)],
                    vec![market_trade("MARKET_TRADE_1", Decimal::ONE, 2)],
                    vec![exact],
                    2,
                ),
            ),
        )
        .unwrap();
        assert!(filled_slot.terminal);
    }

    #[test]
    fn r2b_source_authentication_blocks_before_retention() {
        let mut slot = market_slot(
            crate::BrokerNeutralHybridIntentClass::Entry,
            crate::BrokerNeutralOrderSide::Buy,
            1.0,
            0.0,
        );
        let before = lifecycle_state_fingerprint(
            &Stage5gOrderPositionState {
                strategy_id: "hybrid_imoexf".to_string(),
                account_id: BrokerAccountId::new("ACC_TEST_0001"),
                instrument: target(),
                slots: vec![slot.clone()],
                evidence_identities: vec![],
                last_total_sequence: None,
                last_broker_truth_received_ms: None,
                duplicate_evidence_count: 0,
            },
            0,
        );
        let mut mismatched = market_order(OrderStatus::Working, Decimal::ZERO, 2);
        mismatched.qty = Decimal::new(2, 0);
        mismatched.remaining_qty = Some(Decimal::new(2, 0));
        assert_eq!(
            apply_to_slot(
                &BrokerAccountId::new("ACC_TEST_0001"),
                &target(),
                &mut slot,
                &evidence(
                    2,
                    truth(
                        vec![mismatched],
                        vec![],
                        vec![position(Decimal::new(4, 1), 2)],
                        2,
                    ),
                ),
            ),
            Err(Stage5gOrderPositionError::SourceOrderMismatch)
        );
        let after = lifecycle_state_fingerprint(
            &Stage5gOrderPositionState {
                strategy_id: "hybrid_imoexf".to_string(),
                account_id: BrokerAccountId::new("ACC_TEST_0001"),
                instrument: target(),
                slots: vec![slot],
                evidence_identities: vec![],
                last_total_sequence: None,
                last_broker_truth_received_ms: None,
                duplicate_evidence_count: 0,
            },
            0,
        );
        assert_eq!(before, after);
    }

    #[test]
    fn r2b_chronology_and_evidence_are_vector_order_independent() {
        let mut first = market_trade("MARKET_TRADE_1", Decimal::new(4, 1), 2);
        let mut second = market_trade("MARKET_TRADE_2", Decimal::new(6, 1), 3);
        first.received_ts = ts(3);
        second.received_ts = ts(3);
        let left = evidence(
            2,
            truth(vec![], vec![first.clone(), second.clone()], vec![], 3),
        );
        let right = evidence(2, truth(vec![], vec![second, first], vec![], 3));
        let mut left_slot = market_slot(
            crate::BrokerNeutralHybridIntentClass::Entry,
            crate::BrokerNeutralOrderSide::Buy,
            1.0,
            0.0,
        );
        let mut right_slot = left_slot.clone();
        validate_snapshot_chronology(None, &target(), &mut left_slot, &left).unwrap();
        validate_snapshot_chronology(None, &target(), &mut right_slot, &right).unwrap();
        assert_eq!(
            left_slot.last_trade_source_ts,
            right_slot.last_trade_source_ts
        );
        assert_eq!(
            left_slot.last_trade_received_ts,
            right_slot.last_trade_received_ts
        );
        assert_eq!(evidence_fingerprint(&left), evidence_fingerprint(&right));
    }

    #[test]
    fn r2b_only_exact_correlated_trade_advances_slot_watermark() {
        let mut slot = market_slot(
            crate::BrokerNeutralHybridIntentClass::Entry,
            crate::BrokerNeutralOrderSide::Buy,
            1.0,
            0.0,
        );
        let mut unrelated = market_trade("UNRELATED_TRADE", Decimal::ONE, 9);
        unrelated.broker_order_id = Some(BrokerOrderId::new("ORDER_UNRELATED"));
        unrelated.client_order_id = Some(ClientOrderId::new("CLIENT_UNRELATED").unwrap());
        validate_snapshot_chronology(
            None,
            &target(),
            &mut slot,
            &evidence(2, truth(vec![], vec![unrelated], vec![], 9)),
        )
        .unwrap();
        assert_eq!(slot.last_trade_source_ts, None);
        assert_eq!(slot.last_trade_received_ts, None);

        validate_snapshot_chronology(
            Some(ts(9).timestamp_millis()),
            &target(),
            &mut slot,
            &evidence(
                3,
                truth(
                    vec![],
                    vec![market_trade("MARKET_TRADE_1", Decimal::ONE, 10)],
                    vec![],
                    10,
                ),
            ),
        )
        .unwrap();
        assert_eq!(slot.last_trade_source_ts, Some(ts(10)));
    }

    #[test]
    fn r1_market_exit_requires_flat_not_sell_side_position() {
        let slot = market_slot(
            crate::BrokerNeutralHybridIntentClass::Exit,
            crate::BrokerNeutralOrderSide::Sell,
            1.0,
            1.0,
        );
        assert_eq!(
            validate_source_position(&slot, &position(Decimal::ZERO, 2), None),
            Ok(true)
        );
        assert_eq!(
            validate_source_position(&slot, &position(-Decimal::ONE, 2), None),
            Err(Stage5gOrderPositionError::PositionSideMismatch)
        );
    }

    #[test]
    fn r1_rejected_exit_ack_allows_unchanged_existing_position() {
        let mut slot = market_slot(
            crate::BrokerNeutralHybridIntentClass::Exit,
            crate::BrokerNeutralOrderSide::Sell,
            1.0,
            1.0,
        );
        slot.terminal = true;
        let unchanged = evidence(2, truth(vec![], vec![], vec![position(Decimal::ONE, 2)], 2));
        assert_eq!(
            apply_to_slot(
                &BrokerAccountId::new("ACC_TEST_0001"),
                &target(),
                &mut slot,
                &unchanged
            ),
            Ok(())
        );
    }

    #[test]
    fn r1_partial_cancel_requires_exact_position_and_rejected_fill_blocks() {
        let filled = Decimal::new(4, 1);
        let mut canceled = order(OrderStatus::Canceled, filled, 2);
        canceled.remaining_qty = Some(Decimal::new(6, 1));
        let missing = evidence(
            2,
            truth(
                vec![canceled.clone()],
                vec![trade("TRADE_1", filled, 2)],
                vec![],
                2,
            ),
        );
        assert_eq!(
            apply_to_slot(
                &BrokerAccountId::new("ACC_TEST_0001"),
                &target(),
                &mut slot(),
                &missing
            ),
            Err(Stage5gOrderPositionError::PositionIncomplete)
        );
        let mut rejected = canceled;
        rejected.status = OrderStatus::Rejected;
        rejected.lifecycle = BrokerOrderLifecycle::Terminal;
        let rejected_event = evidence(
            2,
            truth(
                vec![rejected],
                vec![trade("TRADE_1", filled, 2)],
                vec![position(filled, 2)],
                2,
            ),
        );
        assert_eq!(
            apply_to_slot(
                &BrokerAccountId::new("ACC_TEST_0001"),
                &target(),
                &mut slot(),
                &rejected_event
            ),
            Err(Stage5gOrderPositionError::RejectedOrderHasFill)
        );
    }

    #[test]
    fn r1_trade_every_present_identity_must_match_and_qty_is_positive() {
        let filled = order(OrderStatus::Filled, Decimal::ONE, 2);
        let mut contradictory = trade("TRADE_BAD_ID", Decimal::ONE, 2);
        contradictory.broker_order_id = Some(BrokerOrderId::new("ORDER_CONFLICT"));
        assert_eq!(
            validate_trades(&mut slot(), &filled, &[contradictory]),
            Err(Stage5gOrderPositionError::TradeIdentityMismatch)
        );
        assert_eq!(
            validate_trades(
                &mut slot(),
                &filled,
                &[trade("TRADE_ZERO", Decimal::ZERO, 2)]
            ),
            Err(Stage5gOrderPositionError::NonPositiveTradeQuantity)
        );
    }

    #[test]
    fn r1_broker_truth_and_component_time_regression_block() {
        let mut chronology_slot = slot();
        let mut future_order = order(OrderStatus::Working, Decimal::ZERO, 3);
        future_order.received_ts = ts(3);
        let event = evidence(2, truth(vec![future_order], vec![], vec![], 2));
        assert_eq!(
            validate_snapshot_chronology(
                Some(ts(1).timestamp_millis()),
                &target(),
                &mut chronology_slot,
                &event,
            ),
            Err(Stage5gOrderPositionError::ComponentTimeAfterSnapshot)
        );
        let event = evidence(3, truth(vec![], vec![], vec![], 1));
        assert_eq!(
            validate_snapshot_chronology(
                Some(ts(2).timestamp_millis()),
                &target(),
                &mut chronology_slot,
                &event,
            ),
            Err(Stage5gOrderPositionError::BrokerTruthTimeRegression)
        );

        let mut order_slot = slot();
        let first = evidence(
            4,
            truth(
                vec![order(OrderStatus::Working, Decimal::ZERO, 2)],
                vec![],
                vec![],
                2,
            ),
        );
        validate_snapshot_chronology(None, &target(), &mut order_slot, &first).unwrap();
        let mut regressed_order = order(OrderStatus::Working, Decimal::ZERO, 3);
        regressed_order.source_ts = Some(ts(1));
        let regressed = evidence(5, truth(vec![regressed_order], vec![], vec![], 3));
        assert_eq!(
            validate_snapshot_chronology(
                Some(ts(2).timestamp_millis()),
                &target(),
                &mut order_slot,
                &regressed,
            ),
            Err(Stage5gOrderPositionError::OrderTimeRegression)
        );

        let mut position_slot = slot();
        let first = evidence(6, truth(vec![], vec![], vec![position(Decimal::ONE, 2)], 2));
        validate_snapshot_chronology(None, &target(), &mut position_slot, &first).unwrap();
        let mut regressed_position = position(Decimal::ONE, 3);
        regressed_position.source_ts = Some(ts(1));
        let regressed = evidence(7, truth(vec![], vec![], vec![regressed_position], 3));
        assert_eq!(
            validate_snapshot_chronology(
                Some(ts(2).timestamp_millis()),
                &target(),
                &mut position_slot,
                &regressed,
            ),
            Err(Stage5gOrderPositionError::PositionTimeRegression)
        );
    }

    #[test]
    fn r1_fingerprint_v2_separates_partial_state_and_continuation() {
        let mut low_slot = slot();
        let low_qty = Decimal::new(4, 1);
        apply_to_slot(
            &BrokerAccountId::new("ACC_TEST_0001"),
            &target(),
            &mut low_slot,
            &evidence(
                2,
                truth(
                    vec![order(OrderStatus::PartiallyFilled, low_qty, 2)],
                    vec![trade("TRADE_LOW", low_qty, 2)],
                    vec![],
                    2,
                ),
            ),
        )
        .unwrap();
        let mut high_slot = slot();
        let high_qty = Decimal::new(7, 1);
        apply_to_slot(
            &BrokerAccountId::new("ACC_TEST_0001"),
            &target(),
            &mut high_slot,
            &evidence(
                2,
                truth(
                    vec![order(OrderStatus::PartiallyFilled, high_qty, 2)],
                    vec![trade("TRADE_HIGH", high_qty, 2)],
                    vec![],
                    2,
                ),
            ),
        )
        .unwrap();
        let mut low_state = state_with_evidence(&evidence(2, truth(vec![], vec![], vec![], 2)));
        low_state.slots = vec![low_slot.clone()];
        let mut high_state = low_state.clone();
        high_state.slots = vec![high_slot.clone()];
        assert_ne!(
            lifecycle_state_fingerprint(&low_state, 0),
            lifecycle_state_fingerprint(&high_state, 0)
        );
        let next_qty = Decimal::new(5, 1);
        let next = evidence(
            3,
            truth(
                vec![order(OrderStatus::PartiallyFilled, next_qty, 3)],
                vec![trade("TRADE_LOW", next_qty, 3)],
                vec![],
                3,
            ),
        );
        assert_ne!(
            validate_order(
                &BrokerAccountId::new("ACC_TEST_0001"),
                &target(),
                &low_slot,
                &next.broker_truth.orders[0]
            ),
            Err(Stage5gOrderPositionError::FilledQuantityRegression)
        );
        assert_eq!(
            validate_order(
                &BrokerAccountId::new("ACC_TEST_0001"),
                &target(),
                &high_slot,
                &next.broker_truth.orders[0]
            ),
            Err(Stage5gOrderPositionError::FilledQuantityRegression)
        );
    }

    // STAGE5G-C-R2CB-PARITY-TESTS-BEGIN: broker-truth-finam-parity-v1
    #[test]
    fn r2cb_finam_full_snapshot_replay_refreshes_trade_receipt_without_conflict() {
        let account = BrokerAccountId::new("ACC_TEST_0001");
        let mut slot = market_slot(
            crate::BrokerNeutralHybridIntentClass::Entry,
            crate::BrokerNeutralOrderSide::Buy,
            1.0,
            0.0,
        );
        let partial_qty = Decimal::new(4, 1);
        let partial = evidence(
            2,
            truth(
                vec![market_order(OrderStatus::PartiallyFilled, partial_qty, 2)],
                vec![market_trade("FINAM_TRADE_A", partial_qty, 2)],
                vec![position(partial_qty, 2)],
                2,
            ),
        );
        apply_to_slot(&account, &target(), &mut slot, &partial).unwrap();

        let mut repeated = market_trade("FINAM_TRADE_A", partial_qty, 2);
        repeated.received_ts = ts(3);
        let filled = evidence(
            3,
            truth(
                vec![market_order(OrderStatus::Filled, Decimal::ONE, 3)],
                vec![
                    repeated,
                    market_trade("FINAM_TRADE_B", Decimal::new(6, 1), 3),
                ],
                vec![position(Decimal::ONE, 3)],
                3,
            ),
        );
        apply_to_slot(&account, &target(), &mut slot, &filled).unwrap();
        assert!(slot.terminal);
        assert_eq!(slot.trades.len(), 2);
        assert_eq!(slot.trades[0].received_ts, ts(3));
    }

    #[test]
    fn r2cb_same_snapshot_trade_id_is_deduplicated_or_conflicts() {
        let qty = Decimal::new(4, 1);
        let filled = market_order(OrderStatus::PartiallyFilled, qty, 2);
        let first = market_trade("FINAM_TRADE_DUP", qty, 2);
        let mut exact_refresh = first.clone();
        exact_refresh.received_ts = ts(3);
        let mut slot = market_slot(
            crate::BrokerNeutralHybridIntentClass::Entry,
            crate::BrokerNeutralOrderSide::Buy,
            1.0,
            0.0,
        );
        validate_trades(&mut slot, &filled, &[first.clone(), exact_refresh]).unwrap();
        assert_eq!(slot.trades.len(), 1);
        assert_eq!(slot.trades[0].received_ts, ts(3));

        let mut conflicting = first.clone();
        conflicting.price += Decimal::ONE;
        assert_eq!(
            validate_trades(
                &mut market_slot(
                    crate::BrokerNeutralHybridIntentClass::Entry,
                    crate::BrokerNeutralOrderSide::Buy,
                    1.0,
                    0.0,
                ),
                &filled,
                &[first, conflicting]
            ),
            Err(Stage5gOrderPositionError::TradeIdentityConflict)
        );
    }

    #[test]
    fn r2cb_absent_flat_and_aggregate_positions_are_canonical() {
        let account = BrokerAccountId::new("ACC_TEST_0001");
        let flat =
            canonical_target_position(&account, &target(), &truth(vec![], vec![], vec![], 2))
                .unwrap();
        assert_eq!(flat.snapshot.qty, Decimal::ZERO);
        assert_eq!(flat.derivation, CanonicalPositionDerivation::AbsentFlat);

        let aggregate = canonical_target_position(
            &account,
            &target(),
            &truth(
                vec![],
                vec![],
                vec![
                    position(Decimal::new(4, 1), 2),
                    position(Decimal::new(6, 1), 3),
                ],
                3,
            ),
        )
        .unwrap();
        assert_eq!(aggregate.snapshot.qty, Decimal::ONE);
        assert_eq!(aggregate.matching_row_count, 2);
        assert_eq!(aggregate.derivation, CanonicalPositionDerivation::Aggregate);
        assert_eq!(aggregate.snapshot.received_ts, ts(3));
    }

    #[test]
    fn r2cb_market_order_fill_and_position_progress_must_be_exact() {
        let mut slot = market_slot(
            crate::BrokerNeutralHybridIntentClass::Entry,
            crate::BrokerNeutralOrderSide::Buy,
            1.0,
            0.0,
        );
        let filled = Decimal::new(4, 1);
        let event = evidence(
            2,
            truth(
                vec![market_order(OrderStatus::PartiallyFilled, filled, 2)],
                vec![market_trade("FINAM_TRADE_COHERENCE", filled, 2)],
                vec![position(Decimal::new(8, 1), 2)],
                2,
            ),
        );
        assert_eq!(
            apply_to_slot(
                &BrokerAccountId::new("ACC_TEST_0001"),
                &target(),
                &mut slot,
                &event,
            ),
            Err(Stage5gOrderPositionError::OrderPositionIncoherent)
        );
    }

    #[test]
    fn r2cb_terminal_guard_uses_canonical_instrument_identity() {
        let mut terminal = market_slot(
            crate::BrokerNeutralHybridIntentClass::Entry,
            crate::BrokerNeutralOrderSide::Buy,
            1.0,
            0.0,
        );
        terminal.terminal = true;
        let mut compatible = market_order(OrderStatus::Working, Decimal::ZERO, 2);
        compatible.instrument.venue_symbol = None;
        assert_eq!(
            apply_to_slot(
                &BrokerAccountId::new("ACC_TEST_0001"),
                &target(),
                &mut terminal,
                &evidence(2, truth(vec![compatible], vec![], vec![], 2)),
            ),
            Err(Stage5gOrderPositionError::BrokerEvidenceAfterTerminalAck)
        );
    }

    #[test]
    fn r2cb_evidence_fingerprint_is_permutation_stable_and_receipt_exact() {
        use broker_core::account::CashPosition;
        use broker_core::broker::BrokerKind;
        use broker_core::{
            BrokerCashSnapshot, BrokerInstrumentSpec, BrokerSymbol, InstrumentMapEntry,
            InternalSymbol,
        };

        let spec = |symbol: &str| BrokerInstrumentSpec {
            instrument: InstrumentMapEntry {
                internal_symbol: InternalSymbol(symbol.to_string()),
                broker: BrokerKind::Finam,
                broker_symbol: BrokerSymbol(format!("{symbol}@RTSX")),
                exchange: Exchange::Moex,
                market: Market::Futures,
                price_step: Decimal::new(5, 1),
                qty_step: Decimal::ONE,
                lot_size: Decimal::ONE,
                min_qty: Decimal::ONE,
                step_value: Decimal::new(5, 0),
                currency: "RUB".to_string(),
                schedule_id: "MOEX_FUT".to_string(),
                expiration_date: None,
                is_tradable: true,
            },
            broker_asset_id: Some(format!("ASSET_{symbol}")),
            board: Some("RTSX".to_string()),
            long_initial_margin: Some(Decimal::new(5_000, 0)),
            short_initial_margin: Some(Decimal::new(5_000, 0)),
        };
        let mut left = evidence(2, truth(vec![], vec![], vec![], 2));
        left.broker_truth.instruments = vec![spec("IMOEXF"), spec("USDRUBF")];
        left.broker_truth.cash = Some(BrokerCashSnapshot {
            account_id: BrokerAccountId::new("ACC_TEST_0001"),
            cash: vec![
                CashPosition {
                    currency: "RUB".to_string(),
                    amount: Decimal::new(6_000, 0),
                },
                CashPosition {
                    currency: "USD".to_string(),
                    amount: Decimal::ZERO,
                },
            ],
            equity: Some(Decimal::new(6_000, 0)),
            free_cash: Some(Decimal::new(6_000, 0)),
            initial_margin: Some(Decimal::ZERO),
            maintenance_margin: Some(Decimal::ZERO),
            source_ts: Some(ts(2)),
            received_ts: ts(2),
        });
        let mut right = left.clone();
        right.broker_truth.instruments.reverse();
        right.broker_truth.cash.as_mut().unwrap().cash.reverse();
        assert_eq!(evidence_fingerprint(&left), evidence_fingerprint(&right));

        right.broker_truth.received_ts += chrono::Duration::milliseconds(1);
        assert_ne!(evidence_identity(&left), evidence_identity(&right));
        assert_ne!(evidence_fingerprint(&left), evidence_fingerprint(&right));
    }
    // STAGE5G-C-R2CB-PARITY-TESTS-END: broker-truth-finam-parity-v1
}
