//! Stage 5G-c deterministic order/trade/position convergence.
//!
//! The coordinator accepts only canonical Broker Core snapshots. Active and
//! partial evidence is accumulated without a strategy callback. A complete
//! terminal vector is mapped into the existing Stage 5C-j facade exactly once.
//! No Redis, FINAM transport, command dispatch, clock read or broker send is
//! reachable from this module.

use broker_core::command::CommandAckStatus;
use broker_core::{
    instrument_identity_matches, BrokerOrderId, BrokerOrderLifecycle, BrokerOrderSnapshot,
    BrokerPositionSnapshot, BrokerTradeSnapshot, BrokerTruthSnapshot, HybridRuntimeAttribution,
    HybridRuntimeOrderEvent, HybridRuntimePositionEvent, InstrumentId, OrderSide, OrderStatus,
    OrderType, StrategyRequestId,
};
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::stage5c_paper_host::{
    resolve_stage5c_paper_broker_lifecycle, Stage5cBrokerLifecycleResolvedPaperStrategy,
    Stage5cPaperBrokerEventPayload, Stage5cPaperBrokerEventRecord,
    Stage5cPaperBrokerLifecycleError, Stage5cPaperBrokerLifecycleInput,
};
use crate::stage5g_mock_ack::{
    Stage5gMockAckSlotSummary, Stage5gMockIntentAction, Stage5gMockPlaceKind,
    Stage5gResolvedMockAckPaperStrategy,
};

pub const STAGE5G_ORDER_POSITION_SCHEMA_VERSION: u16 = 1;

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
    OrderLifecycleMismatch,
    UnknownOrderStatus,
    InvalidOrderQuantity,
    FilledQuantityRegression,
    OrderTerminalRegression,
    TradeIdentityConflict,
    TradeAccountMismatch,
    TradeInstrumentMismatch,
    TradeSideMismatch,
    TradeQuantityMismatch,
    MissingTargetPosition,
    AmbiguousTargetPosition,
    PositionAccountMismatch,
    PositionSideMismatch,
    PositionOverfill,
    NumericConversionFailed,
    Stage5cPreCallbackBlocked,
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
    broker_order_id: Option<BrokerOrderId>,
    order_events: Vec<CanonicalOrderEvent>,
    trades: Vec<BrokerTradeSnapshot>,
    position: Option<(u64, BrokerPositionSnapshot)>,
    terminal: bool,
}

#[derive(Clone)]
struct EvidenceIdentity {
    identity: String,
    fingerprint: String,
}

struct Stage5gOrderPositionState {
    strategy_id: String,
    account_id: broker_core::BrokerAccountId,
    instrument: InstrumentId,
    slots: Vec<Stage5gOrderPositionSlot>,
    evidence_identities: Vec<EvidenceIdentity>,
    last_total_sequence: Option<u64>,
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

pub enum Stage5gOrderPositionTransition {
    Awaiting(Stage5gOrderPositionSession),
    Converged(Stage5gConvergedPaperStrategy),
}

impl Stage5gOrderPositionTransition {
    pub fn into_awaiting(self) -> Option<Stage5gOrderPositionSession> {
        match self {
            Self::Awaiting(session) => Some(session),
            Self::Converged(_) => None,
        }
    }

    pub fn into_converged(self) -> Option<Stage5gConvergedPaperStrategy> {
        match self {
            Self::Awaiting(_) => None,
            Self::Converged(converged) => Some(converged),
        }
    }
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
            broker_order_id: outcome.broker_order_id.clone(),
            order_events: Vec::new(),
            trades: Vec::new(),
            position: None,
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
            duplicate_evidence_count: 0,
        },
    })
}

pub fn apply_stage5g_order_position_evidence(
    mut session: Stage5gOrderPositionSession,
    evidence: Stage5gOrderPositionEvidence,
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
    let mut next_slot = session.state.slots[slot_index].clone();
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
    session.state.evidence_identities.push(EvidenceIdentity {
        identity,
        fingerprint,
    });

    if !session.state.slots.iter().all(|slot| slot.terminal) {
        return Ok(Stage5gOrderPositionTransition::Awaiting(session));
    }
    converge_through_stage5c(session)
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

fn apply_to_slot(
    account_id: &broker_core::BrokerAccountId,
    instrument: &InstrumentId,
    slot: &mut Stage5gOrderPositionSlot,
    evidence: &Stage5gOrderPositionEvidence,
) -> Result<(), Stage5gOrderPositionError> {
    if slot.terminal {
        let has_target_order = evidence.broker_truth.orders.iter().any(|order| {
            order.account_id == *account_id
                && &order.instrument == instrument
                && (order.broker_order_id == slot.broker_order_id
                    || order.client_order_id.as_ref() == Some(&slot.ack.expected_client_order_id))
        });
        let has_target_position = evidence.broker_truth.positions.iter().any(|position| {
            position.account_id == *account_id
                && position.matches_instrument(instrument)
                && !position.qty.is_zero()
        });
        if has_target_order || has_target_position {
            return Err(Stage5gOrderPositionError::BrokerEvidenceAfterTerminalAck);
        }
        return Ok(());
    }
    match &slot.ack.action {
        Stage5gMockIntentAction::Place {
            place_kind: Stage5gMockPlaceKind::Market,
        } => {
            let position = select_target_position(account_id, instrument, &evidence.broker_truth)?;
            validate_position(slot, &position, None)?;
            slot.position = Some((evidence.total_sequence, position));
            slot.terminal = true;
        }
        Stage5gMockIntentAction::Place {
            place_kind: Stage5gMockPlaceKind::Limit,
        }
        | Stage5gMockIntentAction::Cancel { .. } => {
            let order = select_target_order(slot, &evidence.broker_truth)?;
            validate_order(account_id, instrument, slot, &order)?;
            validate_trades(slot, &order, &evidence.broker_truth.trades)?;
            let status = order.status.clone();
            slot.order_events.push(CanonicalOrderEvent {
                total_sequence: evidence.total_sequence,
                order: order.clone(),
                attribution: evidence.order_attribution.clone(),
            });
            match status {
                OrderStatus::New | OrderStatus::Working | OrderStatus::PartiallyFilled => {}
                OrderStatus::Filled => {
                    let position =
                        select_target_position(account_id, instrument, &evidence.broker_truth)?;
                    validate_position(slot, &position, Some(&order))?;
                    slot.position = Some((evidence.total_sequence, position));
                    slot.terminal = true;
                }
                OrderStatus::Canceled | OrderStatus::Rejected | OrderStatus::Expired => {
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

fn select_target_position(
    account_id: &broker_core::BrokerAccountId,
    instrument: &InstrumentId,
    truth: &BrokerTruthSnapshot,
) -> Result<BrokerPositionSnapshot, Stage5gOrderPositionError> {
    let matches: Vec<_> = truth
        .positions
        .iter()
        .filter(|position| {
            &position.account_id == account_id && position.matches_instrument(instrument)
        })
        .cloned()
        .collect();
    match matches.len() {
        0 => Err(Stage5gOrderPositionError::MissingTargetPosition),
        1 => Ok(matches.into_iter().next().expect("one target position")),
        _ => Err(Stage5gOrderPositionError::AmbiguousTargetPosition),
    }
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

fn validate_trades(
    slot: &mut Stage5gOrderPositionSlot,
    order: &BrokerOrderSnapshot,
    trades: &[BrokerTradeSnapshot],
) -> Result<(), Stage5gOrderPositionError> {
    let mut correlated = Vec::new();
    for trade in trades {
        let identity_match = trade.broker_order_id == order.broker_order_id
            || trade.client_order_id == order.client_order_id;
        if !identity_match {
            continue;
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
        if let Some(previous) = slot
            .trades
            .iter()
            .find(|previous| previous.broker_trade_id == trade.broker_trade_id)
        {
            if previous != trade {
                return Err(Stage5gOrderPositionError::TradeIdentityConflict);
            }
        } else {
            correlated.push(trade.clone());
        }
    }
    slot.trades.extend(correlated);
    let trade_qty: Decimal = slot.trades.iter().map(|trade| trade.qty).sum();
    if order.filled_qty > Decimal::ZERO && trade_qty != order.filled_qty {
        return Err(Stage5gOrderPositionError::TradeQuantityMismatch);
    }
    Ok(())
}

fn validate_position(
    slot: &Stage5gOrderPositionSlot,
    position: &BrokerPositionSnapshot,
    order: Option<&BrokerOrderSnapshot>,
) -> Result<(), Stage5gOrderPositionError> {
    let expected_side = slot
        .ack
        .side
        .as_deref()
        .and_then(parse_side)
        .ok_or(Stage5gOrderPositionError::PositionSideMismatch)?;
    let side_matches = match expected_side {
        OrderSide::Buy => position.qty > Decimal::ZERO,
        OrderSide::Sell => position.qty < Decimal::ZERO,
    };
    if !side_matches {
        return Err(Stage5gOrderPositionError::PositionSideMismatch);
    }
    if let Some(order) = order {
        if position.qty.abs() > order.qty {
            return Err(Stage5gOrderPositionError::PositionOverfill);
        }
    }
    Ok(())
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
) -> Result<Stage5gOrderPositionTransition, Stage5gOrderPositionFailure> {
    let mut records = Vec::new();
    for slot in &session.state.slots {
        for event in &slot.order_events {
            records.push(Stage5cPaperBrokerEventRecord {
                total_sequence: event.total_sequence.saturating_mul(2),
                request_id: slot.ack.request_id,
                payload: Stage5cPaperBrokerEventPayload::Order(order_event(
                    event,
                    slot.ack.request_id,
                )?),
            });
        }
        if let Some((sequence, position)) = &slot.position {
            records.push(Stage5cPaperBrokerEventRecord {
                total_sequence: sequence.saturating_mul(2).saturating_add(1),
                request_id: slot.ack.request_id,
                payload: Stage5cPaperBrokerEventPayload::Position(position_event(position)?),
            });
        }
    }
    records.sort_by_key(|record| record.total_sequence);
    let summary = state_summary(&session.state, records.len());
    let Stage5gOrderPositionSession {
        ack_resolved,
        state,
    } = session;
    let (stage5c_resolved, context) = ack_resolved.into_stage5g_c_parts();
    match resolve_stage5c_paper_broker_lifecycle(
        stage5c_resolved,
        Stage5cPaperBrokerLifecycleInput {
            event_records: records,
        },
    ) {
        Ok(resolved) => Ok(Stage5gOrderPositionTransition::Converged(
            Stage5gConvergedPaperStrategy { resolved, summary },
        )),
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
                            state,
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
        "{}:{}",
        evidence.request_id,
        evidence.broker_truth.received_ts.to_rfc3339()
    )
}

fn evidence_fingerprint(evidence: &Stage5gOrderPositionEvidence) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"moex.stage5g.order-position-evidence.v1\0");
    hasher.update(
        serde_json::to_vec(&(
            evidence.request_id,
            &evidence.broker_truth,
            evidence
                .order_attribution
                .as_ref()
                .map(HybridRuntimeAttribution::internal_comment),
        ))
        .expect("canonical Stage 5G-c evidence serializes"),
    );
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
    let mut hasher = Sha256::new();
    hasher.update(b"moex.stage5g.order-position-lifecycle.v1\0");
    hasher.update(serde_json::to_vec(&summary).expect("Stage 5G-c summary serializes"));
    summary.lifecycle_fingerprint_sha256 = format!("{:x}", hasher.finalize());
    summary
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
                intent_class: "entry".to_string(),
                action: Stage5gMockIntentAction::Place {
                    place_kind: Stage5gMockPlaceKind::Limit,
                },
                side: Some("buy".to_string()),
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
            broker_order_id: Some(BrokerOrderId::new("ORDER_TARGET_1")),
            order_events: Vec::new(),
            trades: Vec::new(),
            position: None,
            terminal: false,
        }
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
            Err(Stage5gOrderPositionError::MissingTargetPosition)
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
            validate_position(
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
            validate_position(
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
}
