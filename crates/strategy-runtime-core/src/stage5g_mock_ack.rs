//! Stage 5G-b deterministic mock ACK attachment.
//!
//! This module owns only paper/mock ACK admission and correlation. It does not
//! read Redis, call FINAM, dispatch commands, create broker order identifiers,
//! or apply order/trade/position truth. Broker Core remains the ACK-policy
//! authority and Stage 5C-i remains the sole runtime callback authority.

use broker_core::command::CommandAckStatus;
use broker_core::{
    BrokerAccountId, BrokerOrderId, ClientOrderId, CommandAck, HybridRuntimeCommandAck,
    InstrumentId, RuntimeAckLifecycleDecision, RuntimeAckLifecycleIssue,
    RuntimeAckPendingDisposition, RuntimeAckStatusPolicy, RuntimePendingRequestIdentity,
    StrategyRequestId,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::stage5c_paper_host::{
    resolve_stage5c_paper_intent_lifecycle, Stage5cPaperAckRecord, Stage5cPaperIntentBatchSummary,
    Stage5cPaperIntentLifecycleError, Stage5cPaperIntentLifecycleInput,
    Stage5cResolvedPaperIntentBatchStrategy, Stage5cSettledPaperStrategy,
};
use crate::{BrokerNeutralHybridIntentClass, BrokerNeutralOrderSide};

pub const STAGE5G_MOCK_ACK_SCHEMA_VERSION: u16 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage5gMockPlaceKind {
    Market,
    Limit,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Stage5gMockIntentAction {
    Place { place_kind: Stage5gMockPlaceKind },
    Cancel { target_order_id: BrokerOrderId },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stage5gMockIntentBinding {
    pub request_id: StrategyRequestId,
    pub intent_class: BrokerNeutralHybridIntentClass,
    pub action: Stage5gMockIntentAction,
    pub side: Option<BrokerNeutralOrderSide>,
}

pub struct Stage5gMockAckSessionInput {
    pub intent_bindings: Vec<Stage5gMockIntentBinding>,
    pub lifecycle_expires_at_ts_utc: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Stage5gMockAckEvent {
    pub total_sequence: u64,
    pub intent_request_id: StrategyRequestId,
    pub account_id: BrokerAccountId,
    pub instrument: InstrumentId,
    pub action: Stage5gMockIntentAction,
    pub side: Option<BrokerNeutralOrderSide>,
    pub ack: CommandAck,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage5gMockAckSlotState {
    Waiting,
    AwaitingBrokerOrderId,
    ReconciliationPending,
    PriorOutcomeRequired,
    NoSendProofRequired,
    ManualInterventionRequired,
    Resolved,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stage5gMockAckSlotSummary {
    pub request_id: StrategyRequestId,
    pub expected_client_order_id: ClientOrderId,
    pub intent_class: String,
    pub action: Stage5gMockIntentAction,
    pub side: Option<String>,
    pub source_event_ts_utc: i64,
    pub state: Stage5gMockAckSlotState,
    pub latest_status: Option<CommandAckStatus>,
    pub broker_order_id_present: bool,
    pub broker_order_id_len: Option<usize>,
    pub pending_disposition: Option<RuntimeAckPendingDisposition>,
    pub status_policy: Option<RuntimeAckStatusPolicy>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stage5gMockAckSessionSummary {
    pub schema_version: u16,
    pub strategy_id: String,
    pub account_id: BrokerAccountId,
    pub instrument: InstrumentId,
    pub origin_bar_close_ts: i64,
    pub lifecycle_expires_at_ts_utc: i64,
    pub last_total_sequence: Option<u64>,
    pub duplicate_status_count: usize,
    pub resolved_count: usize,
    pub slot_count: usize,
    pub slots: Vec<Stage5gMockAckSlotSummary>,
    pub lifecycle_fingerprint_sha256: String,
    pub mock_feedback_only: bool,
    pub broker_truth_changed: bool,
    pub redis_attached: bool,
    pub finam_transport_attached: bool,
}

struct Stage5gMockAckSlot {
    binding: Stage5gMockIntentBinding,
    expected_client_order_id: ClientOrderId,
    source_event_ts_utc: i64,
    observed_broker_order_id: Option<BrokerOrderId>,
    latest_ack: Option<CommandAck>,
    latest_decision: Option<RuntimeAckLifecycleDecision>,
    canonical_ack: Option<CommandAck>,
    canonical_sequence: Option<u64>,
    state: Stage5gMockAckSlotState,
}

/// Linear paper-only capability. It intentionally implements none of Clone,
/// Copy, Debug, Display, Default, Serialize or Deserialize.
pub struct Stage5gMockAckSession {
    settled: Stage5cSettledPaperStrategy,
    batch_summary: Stage5cPaperIntentBatchSummary,
    slots: Vec<Stage5gMockAckSlot>,
    lifecycle_expires_at_ts_utc: i64,
    last_total_sequence: Option<u64>,
    duplicate_status_count: usize,
}

pub struct Stage5gResolvedMockAckPaperStrategy {
    resolved: Stage5cResolvedPaperIntentBatchStrategy,
    batch_summary: Stage5cPaperIntentBatchSummary,
    slots: Vec<Stage5gMockAckSlot>,
    lifecycle_expires_at_ts_utc: i64,
    last_total_sequence: u64,
    duplicate_status_count: usize,
    pre_callback_lifecycle_fingerprint_sha256: String,
    transition_fingerprint_sha256: String,
}

pub enum Stage5gMockAckTransition {
    Awaiting(Stage5gMockAckSession),
    Resolved(Stage5gResolvedMockAckPaperStrategy),
}

impl Stage5gMockAckTransition {
    pub fn into_awaiting(self) -> Option<Stage5gMockAckSession> {
        match self {
            Self::Awaiting(session) => Some(session),
            Self::Resolved(_) => None,
        }
    }

    pub fn into_resolved(self) -> Option<Stage5gResolvedMockAckPaperStrategy> {
        match self {
            Self::Awaiting(_) => None,
            Self::Resolved(resolved) => Some(resolved),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage5gMockAckAdmissionError {
    EmptyIntentBatch,
    ObservationOnlyBatch,
    InvalidLifecycleExpiry,
    BindingCountMismatch,
    BindingRequestOrderMismatch,
    BindingIntentClassMismatch,
    BindingActionClassMismatch,
    BindingSideShapeMismatch,
    BindingRequestIdentityMismatch,
}

pub struct Stage5gMockAckAdmissionBlocked {
    reason: Stage5gMockAckAdmissionError,
    settled: Stage5cSettledPaperStrategy,
}

impl Stage5gMockAckAdmissionBlocked {
    pub fn reason(&self) -> Stage5gMockAckAdmissionError {
        self.reason
    }

    pub fn into_settled(self) -> Stage5cSettledPaperStrategy {
        self.settled
    }
}

impl std::fmt::Debug for Stage5gMockAckAdmissionBlocked {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Stage5gMockAckAdmissionBlocked")
            .field("reason", &self.reason)
            .field("intent_count", &self.settled.intent_batch().intent_count())
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage5gMockAckError {
    NonMonotonicSequence,
    UnknownIntentRequestId,
    AckRequestIdMismatch,
    AccountMismatch,
    InstrumentMismatch,
    ActionMismatch,
    SideMismatch,
    ClientOrderIdMismatch,
    BrokerOrderIdConflict,
    AckTimestampBeforeIntent,
    AckAfterLifecycleExpiry,
    DuplicateAck,
    TerminalAckTwice,
    DuplicateStatusIdentityMismatch,
    Stage5cPreCallbackBlocked,
    Stage5cCallbackTerminal,
}

pub struct Stage5gMockAckBlocked {
    reason: Stage5gMockAckError,
    session: Stage5gMockAckSession,
}

impl Stage5gMockAckBlocked {
    pub fn reason(&self) -> Stage5gMockAckError {
        self.reason
    }

    pub fn session(&self) -> &Stage5gMockAckSession {
        &self.session
    }

    pub fn into_session(self) -> Stage5gMockAckSession {
        self.session
    }
}

impl std::fmt::Debug for Stage5gMockAckBlocked {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Stage5gMockAckBlocked")
            .field("reason", &self.reason)
            .field("summary", &self.session.summary())
            .finish_non_exhaustive()
    }
}

pub struct Stage5gMockAckTerminal {
    reason: Stage5gMockAckError,
    stage5c_reason: Stage5cPaperIntentLifecycleError,
}

impl Stage5gMockAckTerminal {
    pub fn reason(&self) -> Stage5gMockAckError {
        self.reason
    }

    pub fn stage5c_reason(&self) -> Stage5cPaperIntentLifecycleError {
        self.stage5c_reason
    }
}

impl std::fmt::Debug for Stage5gMockAckTerminal {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Stage5gMockAckTerminal")
            .field("reason", &self.reason)
            .field("stage5c_reason", &self.stage5c_reason)
            .finish()
    }
}

#[derive(Debug)]
pub enum Stage5gMockAckFailure {
    Blocked(Box<Stage5gMockAckBlocked>),
    Terminal(Stage5gMockAckTerminal),
}

impl Stage5gMockAckFailure {
    pub fn reason(&self) -> Stage5gMockAckError {
        match self {
            Self::Blocked(blocked) => blocked.reason(),
            Self::Terminal(terminal) => terminal.reason(),
        }
    }

    pub fn into_blocked(self) -> Option<Stage5gMockAckBlocked> {
        match self {
            Self::Blocked(blocked) => Some(*blocked),
            Self::Terminal(_) => None,
        }
    }
}

pub struct Stage5gResolvedMockAckReplayBlocked {
    reason: Stage5gMockAckError,
    resolved: Stage5gResolvedMockAckPaperStrategy,
}

impl Stage5gResolvedMockAckReplayBlocked {
    pub fn reason(&self) -> Stage5gMockAckError {
        self.reason
    }

    pub fn into_resolved(self) -> Stage5gResolvedMockAckPaperStrategy {
        self.resolved
    }
}

impl std::fmt::Debug for Stage5gResolvedMockAckReplayBlocked {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Stage5gResolvedMockAckReplayBlocked")
            .field("reason", &self.reason)
            .finish_non_exhaustive()
    }
}

pub fn attach_stage5g_mock_ack_session(
    settled: Stage5cSettledPaperStrategy,
    input: Stage5gMockAckSessionInput,
) -> Result<Stage5gMockAckSession, Box<Stage5gMockAckAdmissionBlocked>> {
    let batch = settled.intent_batch();
    if batch.intent_count() == 0 {
        return Err(stage5g_admission_block(
            Stage5gMockAckAdmissionError::EmptyIntentBatch,
            settled,
        ));
    }
    if batch.observation_only() {
        return Err(stage5g_admission_block(
            Stage5gMockAckAdmissionError::ObservationOnlyBatch,
            settled,
        ));
    }
    if input.lifecycle_expires_at_ts_utc <= batch.bar_close_ts() {
        return Err(stage5g_admission_block(
            Stage5gMockAckAdmissionError::InvalidLifecycleExpiry,
            settled,
        ));
    }
    if input.intent_bindings.len() != batch.intent_count() {
        return Err(stage5g_admission_block(
            Stage5gMockAckAdmissionError::BindingCountMismatch,
            settled,
        ));
    }

    let request_ids = batch.request_ids().to_vec();
    let intent_classes = batch.intent_classes();
    let source_timestamps = batch.record_source_event_ts_by_request();
    let strategy_id = batch.strategy_id().to_string();
    let account_id = batch.account_id().clone();
    let instrument = batch.instrument().clone();
    let bar_close_ts = batch.bar_close_ts();
    let batch_summary = stage5g_batch_summary(batch);
    let mut slots = Vec::with_capacity(input.intent_bindings.len());

    for (index, binding) in input.intent_bindings.into_iter().enumerate() {
        if binding.request_id != request_ids[index] {
            return Err(stage5g_admission_block(
                Stage5gMockAckAdmissionError::BindingRequestOrderMismatch,
                settled,
            ));
        }
        if binding.intent_class != intent_classes[index] {
            return Err(stage5g_admission_block(
                Stage5gMockAckAdmissionError::BindingIntentClassMismatch,
                settled,
            ));
        }
        if !stage5g_action_matches_class(&binding.action, binding.intent_class) {
            return Err(stage5g_admission_block(
                Stage5gMockAckAdmissionError::BindingActionClassMismatch,
                settled,
            ));
        }
        if !stage5g_side_shape_is_valid(&binding.action, binding.side) {
            return Err(stage5g_admission_block(
                Stage5gMockAckAdmissionError::BindingSideShapeMismatch,
                settled,
            ));
        }
        if !stage5g_binding_request_identity_matches(
            &strategy_id,
            &account_id,
            &instrument,
            bar_close_ts,
            &binding,
        ) {
            return Err(stage5g_admission_block(
                Stage5gMockAckAdmissionError::BindingRequestIdentityMismatch,
                settled,
            ));
        }
        let source_event_ts_utc = source_timestamps[index].1;
        slots.push(Stage5gMockAckSlot {
            expected_client_order_id: ClientOrderId::from_strategy_request(binding.request_id),
            binding,
            source_event_ts_utc,
            observed_broker_order_id: None,
            latest_ack: None,
            latest_decision: None,
            canonical_ack: None,
            canonical_sequence: None,
            state: Stage5gMockAckSlotState::Waiting,
        });
    }

    Ok(Stage5gMockAckSession {
        settled,
        batch_summary,
        slots,
        lifecycle_expires_at_ts_utc: input.lifecycle_expires_at_ts_utc,
        last_total_sequence: None,
        duplicate_status_count: 0,
    })
}

pub fn apply_stage5g_mock_ack(
    mut session: Stage5gMockAckSession,
    event: Stage5gMockAckEvent,
) -> Result<Stage5gMockAckTransition, Stage5gMockAckFailure> {
    let slot_index = match stage5g_preflight_event(&session, &event) {
        Ok(index) => index,
        Err(reason) => return Err(stage5g_block(reason, session)),
    };

    let disposition = match stage5g_event_disposition(&session.slots[slot_index], &event.ack) {
        Ok(disposition) => disposition,
        Err(reason) => return Err(stage5g_block(reason, session)),
    };

    session.last_total_sequence = Some(event.total_sequence);
    let slot = &mut session.slots[slot_index];
    if let Some(broker_order_id) = &event.ack.broker_order_id {
        slot.observed_broker_order_id = Some(broker_order_id.clone());
    }
    slot.latest_ack = Some(event.ack.clone());
    slot.latest_decision = Some(disposition.decision.clone());

    match disposition.kind {
        Stage5gEventDispositionKind::Canonical => {
            slot.canonical_sequence = Some(event.total_sequence);
            slot.canonical_ack = Some(event.ack);
            slot.state = Stage5gMockAckSlotState::Resolved;
        }
        Stage5gEventDispositionKind::Awaiting(state) => {
            slot.state = state;
            return Ok(Stage5gMockAckTransition::Awaiting(session));
        }
        Stage5gEventDispositionKind::DuplicateNoop => {
            session.duplicate_status_count += 1;
        }
    }

    if session
        .slots
        .iter()
        .any(|slot| slot.canonical_ack.is_none())
    {
        return Ok(Stage5gMockAckTransition::Awaiting(session));
    }

    stage5g_resolve_complete_session(session)
}

pub fn apply_stage5g_duplicate_after_resolution(
    mut resolved: Stage5gResolvedMockAckPaperStrategy,
    event: Stage5gMockAckEvent,
) -> Result<Stage5gResolvedMockAckPaperStrategy, Box<Stage5gResolvedMockAckReplayBlocked>> {
    let block =
        |reason, resolved| Box::new(Stage5gResolvedMockAckReplayBlocked { reason, resolved });
    if event.total_sequence <= resolved.last_total_sequence {
        return Err(block(Stage5gMockAckError::NonMonotonicSequence, resolved));
    }
    let Some(slot_index) = resolved
        .slots
        .iter()
        .position(|slot| slot.binding.request_id == event.intent_request_id)
    else {
        return Err(block(Stage5gMockAckError::UnknownIntentRequestId, resolved));
    };
    if let Err(reason) = stage5g_validate_route(
        &resolved.batch_summary,
        resolved.lifecycle_expires_at_ts_utc,
        &resolved.slots[slot_index],
        &event,
    ) {
        return Err(block(reason, resolved));
    }
    if event.ack.status != CommandAckStatus::Duplicate {
        return Err(block(Stage5gMockAckError::TerminalAckTwice, resolved));
    }
    if !stage5g_duplicate_matches_prior(&resolved.slots[slot_index], &event.ack) {
        return Err(block(
            Stage5gMockAckError::DuplicateStatusIdentityMismatch,
            resolved,
        ));
    }
    resolved.last_total_sequence = event.total_sequence;
    resolved.duplicate_status_count += 1;
    resolved.transition_fingerprint_sha256 = stage5g_resolved_fingerprint(&resolved);
    Ok(resolved)
}

impl Stage5gMockAckSession {
    pub fn summary(&self) -> Stage5gMockAckSessionSummary {
        stage5g_session_summary(self)
    }

    pub fn lifecycle_fingerprint_sha256(&self) -> String {
        stage5g_session_fingerprint(self)
    }

    pub fn intent_sink_attached(&self) -> bool {
        false
    }

    pub fn broker_transport_attached(&self) -> bool {
        false
    }

    pub fn redis_command_stream_attached(&self) -> bool {
        false
    }

    pub fn broker_truth_changed(&self) -> bool {
        false
    }
}

impl Stage5gResolvedMockAckPaperStrategy {
    pub fn batch_summary(&self) -> &Stage5cPaperIntentBatchSummary {
        &self.batch_summary
    }

    pub fn ack_outcomes(&self) -> &[crate::Stage5cPaperAckOutcome] {
        self.resolved.ack_outcomes()
    }

    pub fn post_lifecycle_state_fingerprint(&self) -> String {
        self.resolved.post_lifecycle_state_fingerprint()
    }

    pub fn pre_callback_lifecycle_fingerprint_sha256(&self) -> &str {
        &self.pre_callback_lifecycle_fingerprint_sha256
    }

    pub fn transition_fingerprint_sha256(&self) -> &str {
        &self.transition_fingerprint_sha256
    }

    pub fn duplicate_status_count(&self) -> usize {
        self.duplicate_status_count
    }

    pub fn intent_sink_attached(&self) -> bool {
        false
    }

    pub fn broker_transport_attached(&self) -> bool {
        false
    }

    pub fn redis_command_stream_attached(&self) -> bool {
        false
    }

    pub fn broker_truth_changed(&self) -> bool {
        false
    }
}

struct Stage5gEventDisposition {
    decision: RuntimeAckLifecycleDecision,
    kind: Stage5gEventDispositionKind,
}

enum Stage5gEventDispositionKind {
    Canonical,
    Awaiting(Stage5gMockAckSlotState),
    DuplicateNoop,
}

fn stage5g_event_disposition(
    slot: &Stage5gMockAckSlot,
    ack: &CommandAck,
) -> Result<Stage5gEventDisposition, Stage5gMockAckError> {
    if slot.canonical_ack.is_some() {
        if ack.status != CommandAckStatus::Duplicate {
            return Err(Stage5gMockAckError::TerminalAckTwice);
        }
        if !stage5g_duplicate_matches_prior(slot, ack) {
            return Err(Stage5gMockAckError::DuplicateStatusIdentityMismatch);
        }
        let decision = stage5g_pending_identity(slot).evaluate_ack(ack);
        return Ok(Stage5gEventDisposition {
            decision,
            kind: Stage5gEventDispositionKind::DuplicateNoop,
        });
    }
    if slot
        .latest_ack
        .as_ref()
        .is_some_and(|previous| stage5g_same_ack_semantics(previous, ack))
    {
        return Err(Stage5gMockAckError::DuplicateAck);
    }

    let decision = stage5g_pending_identity(slot).evaluate_ack(ack);
    if decision.issues.iter().any(|issue| {
        matches!(
            issue,
            RuntimeAckLifecycleIssue::RequestIdMismatch
                | RuntimeAckLifecycleIssue::ClientOrderIdOnlyMatchDoesNotClearPending
                | RuntimeAckLifecycleIssue::BrokerOrderIdOnlyMatchDoesNotClearPending
                | RuntimeAckLifecycleIssue::BrokerOrderIdMismatchForMatchingRequest
        )
    }) {
        return Err(Stage5gMockAckError::BrokerOrderIdConflict);
    }

    let kind = if decision
        .issues
        .contains(&RuntimeAckLifecycleIssue::DuplicateAckRequiresPriorOutcome)
    {
        Stage5gEventDispositionKind::Awaiting(Stage5gMockAckSlotState::PriorOutcomeRequired)
    } else if decision
        .issues
        .contains(&RuntimeAckLifecycleIssue::ExpiredAckRequiresNoSendProof)
    {
        Stage5gEventDispositionKind::Awaiting(Stage5gMockAckSlotState::NoSendProofRequired)
    } else if decision
        .issues
        .contains(&RuntimeAckLifecycleIssue::AmbiguousErrorAckDoesNotClearPending)
    {
        Stage5gEventDispositionKind::Awaiting(Stage5gMockAckSlotState::ManualInterventionRequired)
    } else {
        match decision.pending_disposition {
            RuntimeAckPendingDisposition::ClearPending => Stage5gEventDispositionKind::Canonical,
            RuntimeAckPendingDisposition::KeepPendingBrokerOrderId => {
                Stage5gEventDispositionKind::Awaiting(
                    Stage5gMockAckSlotState::AwaitingBrokerOrderId,
                )
            }
            RuntimeAckPendingDisposition::KeepPending => Stage5gEventDispositionKind::Awaiting(
                Stage5gMockAckSlotState::ReconciliationPending,
            ),
        }
    };
    Ok(Stage5gEventDisposition { decision, kind })
}

fn stage5g_preflight_event(
    session: &Stage5gMockAckSession,
    event: &Stage5gMockAckEvent,
) -> Result<usize, Stage5gMockAckError> {
    if session
        .last_total_sequence
        .is_some_and(|last| event.total_sequence <= last)
    {
        return Err(Stage5gMockAckError::NonMonotonicSequence);
    }
    let Some(slot_index) = session
        .slots
        .iter()
        .position(|slot| slot.binding.request_id == event.intent_request_id)
    else {
        return Err(Stage5gMockAckError::UnknownIntentRequestId);
    };
    stage5g_validate_route(
        &session.batch_summary,
        session.lifecycle_expires_at_ts_utc,
        &session.slots[slot_index],
        event,
    )?;
    Ok(slot_index)
}

fn stage5g_validate_route(
    batch: &Stage5cPaperIntentBatchSummary,
    lifecycle_expires_at_ts_utc: i64,
    slot: &Stage5gMockAckSlot,
    event: &Stage5gMockAckEvent,
) -> Result<(), Stage5gMockAckError> {
    if event.ack.request_id != event.intent_request_id {
        return Err(Stage5gMockAckError::AckRequestIdMismatch);
    }
    if event.account_id != batch.account_id {
        return Err(Stage5gMockAckError::AccountMismatch);
    }
    if event.instrument != batch.instrument {
        return Err(Stage5gMockAckError::InstrumentMismatch);
    }
    if event.action != slot.binding.action {
        return Err(Stage5gMockAckError::ActionMismatch);
    }
    if event.side != slot.binding.side {
        return Err(Stage5gMockAckError::SideMismatch);
    }
    if event.ack.client_order_id.as_ref() != Some(&slot.expected_client_order_id) {
        return Err(Stage5gMockAckError::ClientOrderIdMismatch);
    }
    if let (Some(observed), Some(incoming)) =
        (&slot.observed_broker_order_id, &event.ack.broker_order_id)
    {
        if observed != incoming {
            return Err(Stage5gMockAckError::BrokerOrderIdConflict);
        }
    }
    if let Stage5gMockIntentAction::Cancel { target_order_id } = &slot.binding.action {
        if event
            .ack
            .broker_order_id
            .as_ref()
            .is_some_and(|incoming| incoming != target_order_id)
        {
            return Err(Stage5gMockAckError::BrokerOrderIdConflict);
        }
    }
    let ack_ts = event.ack.received_ts.timestamp();
    if ack_ts < slot.source_event_ts_utc {
        return Err(Stage5gMockAckError::AckTimestampBeforeIntent);
    }
    if ack_ts > lifecycle_expires_at_ts_utc {
        return Err(Stage5gMockAckError::AckAfterLifecycleExpiry);
    }
    Ok(())
}

fn stage5g_pending_identity(slot: &Stage5gMockAckSlot) -> RuntimePendingRequestIdentity {
    let broker_order_id = match &slot.binding.action {
        Stage5gMockIntentAction::Cancel { target_order_id } => Some(target_order_id.clone()),
        Stage5gMockIntentAction::Place { .. } => slot.observed_broker_order_id.clone(),
    };
    RuntimePendingRequestIdentity {
        request_id: slot.binding.request_id,
        client_order_id: Some(slot.expected_client_order_id.clone()),
        broker_order_id,
    }
}

fn stage5g_duplicate_matches_prior(slot: &Stage5gMockAckSlot, ack: &CommandAck) -> bool {
    let Some(prior) = &slot.canonical_ack else {
        return false;
    };
    ack.request_id == prior.request_id
        && ack.client_order_id == prior.client_order_id
        && match (&ack.broker_order_id, &prior.broker_order_id) {
            (Some(left), Some(right)) => left == right,
            (None, _) => true,
            (Some(left), None) => slot
                .observed_broker_order_id
                .as_ref()
                .is_some_and(|observed| observed == left),
        }
}

fn stage5g_same_ack_semantics(left: &CommandAck, right: &CommandAck) -> bool {
    left.request_id == right.request_id
        && left.client_order_id == right.client_order_id
        && left.broker_order_id == right.broker_order_id
        && left.status == right.status
        && left.reason == right.reason
}

fn stage5g_resolve_complete_session(
    session: Stage5gMockAckSession,
) -> Result<Stage5gMockAckTransition, Stage5gMockAckFailure> {
    let pre_callback_lifecycle_fingerprint_sha256 = stage5g_session_fingerprint(&session);
    let Stage5gMockAckSession {
        settled,
        batch_summary,
        slots,
        lifecycle_expires_at_ts_utc,
        last_total_sequence,
        duplicate_status_count,
    } = session;
    let ack_records = slots
        .iter()
        .map(|slot| Stage5cPaperAckRecord {
            total_sequence: slot
                .canonical_sequence
                .expect("complete Stage 5G session has canonical sequence"),
            ack: stage5g_to_hybrid_ack(
                slot.canonical_ack
                    .as_ref()
                    .expect("complete Stage 5G session has canonical ACK"),
            ),
        })
        .collect();

    match resolve_stage5c_paper_intent_lifecycle(
        settled,
        Stage5cPaperIntentLifecycleInput { ack_records },
    ) {
        Ok(resolved) => {
            let mut result = Stage5gResolvedMockAckPaperStrategy {
                resolved,
                batch_summary,
                slots,
                lifecycle_expires_at_ts_utc,
                last_total_sequence: last_total_sequence
                    .expect("complete Stage 5G session consumed at least one ACK"),
                duplicate_status_count,
                pre_callback_lifecycle_fingerprint_sha256,
                transition_fingerprint_sha256: String::new(),
            };
            result.transition_fingerprint_sha256 = stage5g_resolved_fingerprint(&result);
            Ok(Stage5gMockAckTransition::Resolved(result))
        }
        Err(failure) => {
            let stage5c_reason = failure.reason();
            if let Some(blocked) = failure.into_blocked() {
                let session = Stage5gMockAckSession {
                    settled: blocked.into_settled(),
                    batch_summary,
                    slots,
                    lifecycle_expires_at_ts_utc,
                    last_total_sequence,
                    duplicate_status_count,
                };
                Err(stage5g_block(
                    Stage5gMockAckError::Stage5cPreCallbackBlocked,
                    session,
                ))
            } else {
                Err(Stage5gMockAckFailure::Terminal(Stage5gMockAckTerminal {
                    reason: Stage5gMockAckError::Stage5cCallbackTerminal,
                    stage5c_reason,
                }))
            }
        }
    }
}

fn stage5g_to_hybrid_ack(ack: &CommandAck) -> HybridRuntimeCommandAck {
    let status = broker_core::map_hybrid_runtime_ack_status(ack.status)
        .expect("only callback-safe ACK statuses become canonical in Stage 5G-b");
    HybridRuntimeCommandAck {
        request_id: ack.request_id,
        status,
        broker_order_id: ack.broker_order_id.clone(),
        error_code: broker_core::map_hybrid_runtime_ack_error_code(
            ack.reason.as_ref().map(|reason| reason.code),
        ),
        error_message: None,
        processed_ts_utc: ack.received_ts.timestamp(),
    }
}

fn stage5g_action_matches_class(
    action: &Stage5gMockIntentAction,
    intent_class: BrokerNeutralHybridIntentClass,
) -> bool {
    match action {
        Stage5gMockIntentAction::Cancel { .. } => {
            intent_class == BrokerNeutralHybridIntentClass::CancelCleanup
        }
        Stage5gMockIntentAction::Place { .. } => {
            intent_class != BrokerNeutralHybridIntentClass::CancelCleanup
        }
    }
}

fn stage5g_side_shape_is_valid(
    action: &Stage5gMockIntentAction,
    side: Option<BrokerNeutralOrderSide>,
) -> bool {
    matches!(
        (action, side),
        (Stage5gMockIntentAction::Place { .. }, Some(_))
            | (Stage5gMockIntentAction::Cancel { .. }, None)
    )
}

fn stage5g_binding_request_identity_matches(
    strategy_id: &str,
    account_id: &BrokerAccountId,
    instrument: &InstrumentId,
    bar_close_ts: i64,
    binding: &Stage5gMockIntentBinding,
) -> bool {
    let (action, sequence) = match (&binding.action, binding.side) {
        (
            Stage5gMockIntentAction::Place {
                place_kind: Stage5gMockPlaceKind::Market,
            },
            Some(BrokerNeutralOrderSide::Buy),
        ) => ("market".to_string(), 3),
        (
            Stage5gMockIntentAction::Place {
                place_kind: Stage5gMockPlaceKind::Market,
            },
            Some(BrokerNeutralOrderSide::Sell),
        ) => ("market".to_string(), 4),
        (
            Stage5gMockIntentAction::Place {
                place_kind: Stage5gMockPlaceKind::Limit,
            },
            Some(_),
        ) => ("place".to_string(), 0),
        (Stage5gMockIntentAction::Cancel { target_order_id }, None) => {
            (format!("cancel:{}", target_order_id.as_str()), 1)
        }
        _ => return false,
    };
    binding.request_id
        == crate::deterministic_request_id(
            strategy_id,
            account_id.as_str(),
            &instrument.symbol,
            &action,
            bar_close_ts,
            sequence,
        )
}

fn stage5g_batch_summary(
    batch: &crate::stage5c_paper_host::Stage5cPaperIntentBatch,
) -> Stage5cPaperIntentBatchSummary {
    let source_timestamps = batch.record_source_event_ts_by_request();
    Stage5cPaperIntentBatchSummary {
        strategy_id: batch.strategy_id().to_string(),
        account_id: batch.account_id().clone(),
        instrument: batch.instrument().clone(),
        origin_bar_close_ts: batch.bar_close_ts(),
        bar_close_ts: batch.bar_close_ts(),
        min_source_event_ts: source_timestamps
            .iter()
            .map(|(_, timestamp)| *timestamp)
            .min()
            .unwrap_or(batch.bar_close_ts()),
        max_source_event_ts: source_timestamps
            .iter()
            .map(|(_, timestamp)| *timestamp)
            .max()
            .unwrap_or(batch.bar_close_ts()),
        state_fingerprint: batch.state_fingerprint().to_string(),
        request_ids: batch.request_ids().to_vec(),
        intent_count: batch.intent_count(),
        observation_only: batch.observation_only(),
    }
}

fn stage5g_session_summary(session: &Stage5gMockAckSession) -> Stage5gMockAckSessionSummary {
    let slots = session.slots.iter().map(stage5g_slot_summary).collect();
    let mut summary = Stage5gMockAckSessionSummary {
        schema_version: STAGE5G_MOCK_ACK_SCHEMA_VERSION,
        strategy_id: session.batch_summary.strategy_id.clone(),
        account_id: session.batch_summary.account_id.clone(),
        instrument: session.batch_summary.instrument.clone(),
        origin_bar_close_ts: session.batch_summary.origin_bar_close_ts,
        lifecycle_expires_at_ts_utc: session.lifecycle_expires_at_ts_utc,
        last_total_sequence: session.last_total_sequence,
        duplicate_status_count: session.duplicate_status_count,
        resolved_count: session
            .slots
            .iter()
            .filter(|slot| slot.state == Stage5gMockAckSlotState::Resolved)
            .count(),
        slot_count: session.slots.len(),
        slots,
        lifecycle_fingerprint_sha256: String::new(),
        mock_feedback_only: true,
        broker_truth_changed: false,
        redis_attached: false,
        finam_transport_attached: false,
    };
    summary.lifecycle_fingerprint_sha256 = stage5g_summary_fingerprint(&summary);
    summary
}

fn stage5g_slot_summary(slot: &Stage5gMockAckSlot) -> Stage5gMockAckSlotSummary {
    Stage5gMockAckSlotSummary {
        request_id: slot.binding.request_id,
        expected_client_order_id: slot.expected_client_order_id.clone(),
        intent_class: format!("{:?}", slot.binding.intent_class),
        action: slot.binding.action.clone(),
        side: slot.binding.side.map(|side| format!("{side:?}")),
        source_event_ts_utc: slot.source_event_ts_utc,
        state: slot.state,
        latest_status: slot.latest_ack.as_ref().map(|ack| ack.status),
        broker_order_id_present: slot.observed_broker_order_id.is_some(),
        broker_order_id_len: slot
            .observed_broker_order_id
            .as_ref()
            .map(|order_id| order_id.as_str().len()),
        pending_disposition: slot
            .latest_decision
            .as_ref()
            .map(|decision| decision.pending_disposition),
        status_policy: slot
            .latest_decision
            .as_ref()
            .map(|decision| decision.status_policy),
    }
}

fn stage5g_session_fingerprint(session: &Stage5gMockAckSession) -> String {
    stage5g_summary_fingerprint(&stage5g_session_summary_without_fingerprint(session))
}

fn stage5g_session_summary_without_fingerprint(
    session: &Stage5gMockAckSession,
) -> Stage5gMockAckSessionSummary {
    Stage5gMockAckSessionSummary {
        schema_version: STAGE5G_MOCK_ACK_SCHEMA_VERSION,
        strategy_id: session.batch_summary.strategy_id.clone(),
        account_id: session.batch_summary.account_id.clone(),
        instrument: session.batch_summary.instrument.clone(),
        origin_bar_close_ts: session.batch_summary.origin_bar_close_ts,
        lifecycle_expires_at_ts_utc: session.lifecycle_expires_at_ts_utc,
        last_total_sequence: session.last_total_sequence,
        duplicate_status_count: session.duplicate_status_count,
        resolved_count: session
            .slots
            .iter()
            .filter(|slot| slot.state == Stage5gMockAckSlotState::Resolved)
            .count(),
        slot_count: session.slots.len(),
        slots: session.slots.iter().map(stage5g_slot_summary).collect(),
        lifecycle_fingerprint_sha256: String::new(),
        mock_feedback_only: true,
        broker_truth_changed: false,
        redis_attached: false,
        finam_transport_attached: false,
    }
}

fn stage5g_summary_fingerprint<T: Serialize>(value: &T) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"moex.stage5g.mock-ack-lifecycle.v1\0");
    hasher.update(serde_json::to_vec(value).expect("Stage 5G summary serializes"));
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn stage5g_resolved_fingerprint(resolved: &Stage5gResolvedMockAckPaperStrategy) -> String {
    #[derive(Serialize)]
    struct Projection<'a> {
        batch: &'a Stage5cPaperIntentBatchSummary,
        pre_callback_lifecycle_fingerprint_sha256: &'a str,
        post_lifecycle_state_fingerprint: String,
        last_total_sequence: u64,
        duplicate_status_count: usize,
        broker_truth_changed: bool,
    }
    stage5g_summary_fingerprint(&Projection {
        batch: &resolved.batch_summary,
        pre_callback_lifecycle_fingerprint_sha256: &resolved
            .pre_callback_lifecycle_fingerprint_sha256,
        post_lifecycle_state_fingerprint: resolved.post_lifecycle_state_fingerprint(),
        last_total_sequence: resolved.last_total_sequence,
        duplicate_status_count: resolved.duplicate_status_count,
        broker_truth_changed: false,
    })
}

fn stage5g_admission_block(
    reason: Stage5gMockAckAdmissionError,
    settled: Stage5cSettledPaperStrategy,
) -> Box<Stage5gMockAckAdmissionBlocked> {
    Box::new(Stage5gMockAckAdmissionBlocked { reason, settled })
}

fn stage5g_block(
    reason: Stage5gMockAckError,
    session: Stage5gMockAckSession,
) -> Stage5gMockAckFailure {
    Stage5gMockAckFailure::Blocked(Box::new(Stage5gMockAckBlocked { reason, session }))
}

#[cfg(test)]
mod tests {
    use broker_core::command::{CommandAckReason, CommandAckReasonCode};
    use broker_core::{Exchange, Market};
    use chrono::{TimeZone, Utc};
    use rust_decimal::Decimal;
    use uuid::Uuid;

    use super::*;
    use crate::hybrid_intraday::{
        HybridOrchestratorConfig, IntradayBreakoutConfig, MeanReversionConfig,
    };
    use crate::hybrid_intraday_runtime::{
        HybridIntradayProfile, HybridIntradayRuntimeConfig, HybridIntradayRuntimeStrategy,
        MeanReversionVariant, MrGatePolicy, RiskGateMode,
    };
    use crate::runtime_compat::{
        BarEvent, DataOrigin, GatewayPhase, MarketBuyAndCloseLiveOrderStyle, PaperExecutionMode,
        Strategy, StrategyCtx, TradeMode,
    };

    struct Fixture {
        session: Stage5gMockAckSession,
        request_id: StrategyRequestId,
        side: BrokerNeutralOrderSide,
        action: Stage5gMockIntentAction,
        account_id: BrokerAccountId,
        instrument: InstrumentId,
        bar_close_ts: i64,
    }

    fn target() -> InstrumentId {
        InstrumentId {
            symbol: "IMOEXF".to_string(),
            venue_symbol: Some("IMOEXF@RTSX".to_string()),
            exchange: Exchange::Moex,
            market: Market::Futures,
        }
    }

    fn strategy() -> HybridIntradayRuntimeStrategy {
        HybridIntradayRuntimeStrategy::new(HybridIntradayRuntimeConfig {
            symbol: "IMOEXF".to_string(),
            profile: HybridIntradayProfile::BaselineRuntimeHybrid,
            mr_variant: MeanReversionVariant::Author41BoundaryShort,
            mr_gate_policy: MrGatePolicy::Disabled,
            risk_gate_mode: RiskGateMode::Disabled,
            risk_gate_seed_file: None,
            risk_gate_ledger_key: None,
            model_session_start_time: None,
            model_session_end_time: None,
            qty: 1.0,
            live_order_style: MarketBuyAndCloseLiveOrderStyle::Market,
            tick_size: 0.5,
            marketable_limit_offset_ticks: 0,
            timezone_offset_hours: 3,
            session_close_hour: 23,
            session_close_minute: 49,
            weekends_off: false,
            stop_end_buffer_sec: 60,
            repair_deadline_sec: 180,
            sl_escalate_timeout_sec: 30,
            max_repair_retries: 3,
            repair_backoff_base_sec: 5,
            repair_backoff_max_sec: 60,
            pending_timeout_sec: 30,
            partial_entry_fill_timeout_ms: 3_000,
            mr_config: MeanReversionConfig::default(),
            breakout_config: IntradayBreakoutConfig::default(),
            orchestrator_config: HybridOrchestratorConfig::default(),
        })
    }

    fn settled_market_batch(bar_close_ts: i64) -> Stage5cSettledPaperStrategy {
        let lifecycle_now = Utc
            .timestamp_opt(bar_close_ts - 30, 0)
            .single()
            .expect("lifecycle time");
        let mut strategy = strategy();
        for (close_time_utc, high, low) in [
            (bar_close_ts - 86_400 - 600, 2630.0, 2570.0),
            (bar_close_ts - 86_400, 2620.0, 2580.0),
        ] {
            let intents = Strategy::on_bar(
                &mut strategy,
                &StrategyCtx {
                    strategy_id: "hybrid_imoexf".to_string(),
                    portfolio: "ACC_TEST_0001".to_string(),
                    exchange: "MOEX".to_string(),
                    symbol: "IMOEXF".to_string(),
                    tick_size: 0.5,
                    trade_mode: TradeMode::Paper,
                    paper_execution_mode: PaperExecutionMode::LiveOnly,
                    allow_live_orders: false,
                    gateway_phase: GatewayPhase::LiveReady,
                    position_qty: Some(0.0),
                    event_ts_utc: close_time_utc,
                    now_ts_utc: close_time_utc,
                    last_bar_ts: Some(close_time_utc),
                },
                &BarEvent {
                    symbol: "IMOEXF".to_string(),
                    close_time_utc,
                    o: 2600.0,
                    h: high,
                    l: low,
                    close: 2600.0,
                    v: 1.0,
                    origin: DataOrigin::Replay,
                },
            );
            assert!(intents.is_empty());
        }
        let bar = broker_core::HybridRuntimeBarEvent {
            instrument: target(),
            close_time_utc: bar_close_ts,
            open: 2601.0,
            high: 2602.0,
            low: 2599.0,
            close: 2601.0,
            volume: 1.0,
            origin: broker_core::HybridRuntimeBarOrigin::Live,
            is_final: true,
            timeframe_sec: 600,
        };
        let (recovered, accepted) =
            crate::stage5c_paper_host::stage5f_test_seams::sequence_inputs_from_owned_strategy(
                strategy,
                "hybrid_imoexf".to_string(),
                BrokerAccountId::new("ACC_TEST_0001"),
                target(),
                0.5,
                Decimal::ZERO,
                lifecycle_now,
                bar_close_ts - 600,
                bar,
            );
        let semantic =
            crate::apply_stage5c_semantic_bar(recovered, accepted).expect("paper semantic bar");
        assert_eq!(
            semantic.captured_intent_count(),
            1,
            "fixture needs one intent"
        );
        crate::settle_stage5c_semantic_result(semantic).expect("settled intent")
    }

    fn make_fixture() -> Fixture {
        let bar_close_ts = Utc::now().timestamp().div_euclid(600) * 600;
        make_fixture_at(bar_close_ts)
    }

    fn make_fixture_at(bar_close_ts: i64) -> Fixture {
        let settled = settled_market_batch(bar_close_ts);
        let request_id = settled.intent_batch().request_ids()[0];
        let buy = crate::deterministic_request_id(
            "hybrid_imoexf",
            "ACC_TEST_0001",
            "IMOEXF",
            "market",
            bar_close_ts,
            3,
        );
        let sell = crate::deterministic_request_id(
            "hybrid_imoexf",
            "ACC_TEST_0001",
            "IMOEXF",
            "market",
            bar_close_ts,
            4,
        );
        let side = if request_id == buy {
            BrokerNeutralOrderSide::Buy
        } else {
            assert_eq!(request_id, sell, "fixture must emit a market intent");
            BrokerNeutralOrderSide::Sell
        };
        let action = Stage5gMockIntentAction::Place {
            place_kind: Stage5gMockPlaceKind::Market,
        };
        let binding = Stage5gMockIntentBinding {
            request_id,
            intent_class: settled.intent_batch().intent_classes()[0],
            action: action.clone(),
            side: Some(side),
        };
        let session = attach_stage5g_mock_ack_session(
            settled,
            Stage5gMockAckSessionInput {
                intent_bindings: vec![binding],
                lifecycle_expires_at_ts_utc: bar_close_ts + 300,
            },
        )
        .expect("mock ACK session");
        Fixture {
            session,
            request_id,
            side,
            action,
            account_id: BrokerAccountId::new("ACC_TEST_0001"),
            instrument: target(),
            bar_close_ts,
        }
    }

    fn event(
        fixture: &Fixture,
        total_sequence: u64,
        status: CommandAckStatus,
        broker_order_id: Option<&str>,
        reason: Option<CommandAckReasonCode>,
    ) -> Stage5gMockAckEvent {
        Stage5gMockAckEvent {
            total_sequence,
            intent_request_id: fixture.request_id,
            account_id: fixture.account_id.clone(),
            instrument: fixture.instrument.clone(),
            action: fixture.action.clone(),
            side: Some(fixture.side),
            ack: CommandAck {
                request_id: fixture.request_id,
                client_order_id: Some(ClientOrderId::from_strategy_request(fixture.request_id)),
                broker_order_id: broker_order_id.map(BrokerOrderId::new),
                status,
                reason: reason.map(CommandAckReason::new),
                received_ts: Utc
                    .timestamp_opt(
                        fixture.bar_close_ts + i64::try_from(total_sequence).unwrap(),
                        0,
                    )
                    .single()
                    .unwrap(),
            },
        }
    }

    fn opposite(side: BrokerNeutralOrderSide) -> BrokerNeutralOrderSide {
        match side {
            BrokerNeutralOrderSide::Buy => BrokerNeutralOrderSide::Sell,
            BrokerNeutralOrderSide::Sell => BrokerNeutralOrderSide::Buy,
        }
    }

    fn expect_blocked(
        result: Result<Stage5gMockAckTransition, Stage5gMockAckFailure>,
    ) -> Stage5gMockAckBlocked {
        match result {
            Err(failure) => failure
                .into_blocked()
                .expect("expected recoverable pre-callback block"),
            Ok(_) => panic!("expected Stage 5G mock ACK block"),
        }
    }

    fn expect_replay_blocked(
        result: Result<
            Stage5gResolvedMockAckPaperStrategy,
            Box<Stage5gResolvedMockAckReplayBlocked>,
        >,
    ) -> Box<Stage5gResolvedMockAckReplayBlocked> {
        match result {
            Err(blocked) => blocked,
            Ok(_) => panic!("expected resolved ACK replay block"),
        }
    }

    #[test]
    fn gack01_place_accepted_exact_ids_resolves_without_broker_truth() {
        let fixture = make_fixture();
        let ack = event(
            &fixture,
            1,
            CommandAckStatus::Accepted,
            Some("FINAM_ORDER_EXACT_STRING_0001"),
            None,
        );
        let resolved = apply_stage5g_mock_ack(fixture.session, ack)
            .expect("accepted ACK")
            .into_resolved()
            .expect("single intent resolves");
        assert_eq!(resolved.ack_outcomes().len(), 1);
        assert_eq!(
            resolved.ack_outcomes()[0]
                .broker_order_id
                .as_ref()
                .map(BrokerOrderId::as_str),
            Some("FINAM_ORDER_EXACT_STRING_0001")
        );
        assert!(!resolved.broker_truth_changed());
        assert!(!resolved.broker_transport_attached());
        assert!(!resolved.redis_command_stream_attached());
    }

    #[test]
    fn submitted_with_exact_broker_order_id_resolves_without_broker_truth() {
        let fixture = make_fixture();
        let ack = event(
            &fixture,
            1,
            CommandAckStatus::Submitted,
            Some("FINAM_SUBMITTED_EXACT_STRING_0001"),
            None,
        );
        let resolved = apply_stage5g_mock_ack(fixture.session, ack)
            .expect("submitted ACK with broker order id")
            .into_resolved()
            .expect("single intent resolves");
        assert_eq!(
            resolved.ack_outcomes()[0]
                .broker_order_id
                .as_ref()
                .map(BrokerOrderId::as_str),
            Some("FINAM_SUBMITTED_EXACT_STRING_0001")
        );
        assert!(!resolved.broker_truth_changed());
    }

    #[test]
    fn gack02_and_gack03_missing_broker_id_waits_then_recovered_resolves() {
        let fixture = make_fixture();
        let submitted = event(&fixture, 1, CommandAckStatus::Submitted, None, None);
        let pending = apply_stage5g_mock_ack(fixture.session, submitted)
            .expect("submitted without id is a typed pending outcome")
            .into_awaiting()
            .expect("broker id is still pending");
        assert_eq!(
            pending.summary().slots[0].state,
            Stage5gMockAckSlotState::AwaitingBrokerOrderId
        );
        let recovered_fixture = Fixture {
            session: pending,
            ..fixture
        };
        let recovered = event(
            &recovered_fixture,
            2,
            CommandAckStatus::Recovered,
            Some("FINAM_RECOVERED_0001"),
            Some(CommandAckReasonCode::RecoveredByBrokerTruth),
        );
        assert!(apply_stage5g_mock_ack(recovered_fixture.session, recovered)
            .expect("recovered exact broker id")
            .into_resolved()
            .is_some());
    }

    #[test]
    fn gack04_rejected_exact_request_clears_pending() {
        let fixture = make_fixture();
        let rejected = event(
            &fixture,
            1,
            CommandAckStatus::Rejected,
            None,
            Some(CommandAckReasonCode::BrokerRejected),
        );
        let resolved = apply_stage5g_mock_ack(fixture.session, rejected)
            .expect("rejected is callback-safe")
            .into_resolved()
            .expect("rejected clears exact pending request");
        assert_eq!(
            resolved.ack_outcomes()[0].status,
            broker_core::HybridRuntimeAckStatus::Rejected
        );
    }

    #[test]
    fn gack05_and_gack06_ambiguous_statuses_keep_pending() {
        for (status, expected_policy) in [
            (
                CommandAckStatus::Timeout,
                RuntimeAckStatusPolicy::KeepPending,
            ),
            (
                CommandAckStatus::UnknownPending,
                RuntimeAckStatusPolicy::KeepPending,
            ),
        ] {
            let fixture = make_fixture();
            let ambiguous = event(&fixture, 1, status, None, None);
            let pending = apply_stage5g_mock_ack(fixture.session, ambiguous)
                .expect("ambiguous outcome is retained")
                .into_awaiting()
                .unwrap();
            let slot = &pending.summary().slots[0];
            assert_eq!(slot.state, Stage5gMockAckSlotState::ReconciliationPending);
            assert_eq!(slot.status_policy, Some(expected_policy));
        }
    }

    #[test]
    fn gack07_duplicate_requires_prior_outcome_and_exact_duplicate_is_noop() {
        let fixture = make_fixture();
        let duplicate_without_prior = event(&fixture, 1, CommandAckStatus::Duplicate, None, None);
        let pending = apply_stage5g_mock_ack(fixture.session, duplicate_without_prior)
            .expect("duplicate without prior is retained")
            .into_awaiting()
            .unwrap();
        assert_eq!(
            pending.summary().slots[0].state,
            Stage5gMockAckSlotState::PriorOutcomeRequired
        );

        let fixture = make_fixture();
        let accepted = event(
            &fixture,
            1,
            CommandAckStatus::Accepted,
            Some("FINAM_DUPLICATE_PRIOR_0001"),
            None,
        );
        let duplicate = event(
            &fixture,
            2,
            CommandAckStatus::Duplicate,
            Some("FINAM_DUPLICATE_PRIOR_0001"),
            Some(CommandAckReasonCode::DuplicateCommand),
        );
        let resolved = apply_stage5g_mock_ack(fixture.session, accepted)
            .unwrap()
            .into_resolved()
            .unwrap();
        let resolved = apply_stage5g_duplicate_after_resolution(resolved, duplicate)
            .expect("exact duplicate status is idempotent");
        assert_eq!(resolved.duplicate_status_count(), 1);
    }

    #[test]
    fn gack08_expired_requires_exact_no_send_proof() {
        let fixture = make_fixture();
        let expired_without_proof = event(&fixture, 1, CommandAckStatus::Expired, None, None);
        let pending = apply_stage5g_mock_ack(fixture.session, expired_without_proof)
            .expect("expired without proof is retained")
            .into_awaiting()
            .unwrap();
        assert_eq!(
            pending.summary().slots[0].state,
            Stage5gMockAckSlotState::NoSendProofRequired
        );
        let fixture = Fixture {
            session: pending,
            ..fixture
        };
        let proof = event(
            &fixture,
            2,
            CommandAckStatus::Expired,
            None,
            Some(CommandAckReasonCode::ExpiredCommand),
        );
        assert!(apply_stage5g_mock_ack(fixture.session, proof)
            .expect("exact no-send proof clears pending")
            .into_resolved()
            .is_some());
    }

    #[test]
    fn error_outcome_requires_manual_intervention_without_callback() {
        let fixture = make_fixture();
        let error = event(
            &fixture,
            1,
            CommandAckStatus::Error,
            None,
            Some(CommandAckReasonCode::ManualInterventionRequired),
        );
        let pending = apply_stage5g_mock_ack(fixture.session, error)
            .expect("error is represented as retained manual state")
            .into_awaiting()
            .unwrap();
        assert_eq!(
            pending.summary().slots[0].state,
            Stage5gMockAckSlotState::ManualInterventionRequired
        );
    }

    #[test]
    fn gack09_wrong_request_and_client_ids_block_atomically() {
        let fixture = make_fixture();
        let mut wrong_request = event(&fixture, 1, CommandAckStatus::Accepted, None, None);
        wrong_request.ack.request_id = StrategyRequestId::from(Uuid::from_u128(0x5a09));
        let blocked = expect_blocked(apply_stage5g_mock_ack(fixture.session, wrong_request));
        assert_eq!(blocked.reason(), Stage5gMockAckError::AckRequestIdMismatch);

        let fixture = Fixture {
            session: blocked.into_session(),
            ..fixture
        };
        let mut wrong_client = event(&fixture, 1, CommandAckStatus::Accepted, None, None);
        wrong_client.ack.client_order_id =
            Some(ClientOrderId::new("CID_WRONG_0000000001").expect("valid wrong client id"));
        assert_eq!(
            expect_blocked(apply_stage5g_mock_ack(fixture.session, wrong_client)).reason(),
            Stage5gMockAckError::ClientOrderIdMismatch
        );
    }

    #[test]
    fn gack10_conflicting_broker_order_id_blocks() {
        let fixture = make_fixture();
        let timeout = event(
            &fixture,
            1,
            CommandAckStatus::Timeout,
            Some("FINAM_OBSERVED_0001"),
            Some(CommandAckReasonCode::TimeoutUnknownPending),
        );
        let pending = apply_stage5g_mock_ack(fixture.session, timeout)
            .expect("timeout retains observed id")
            .into_awaiting()
            .unwrap();
        let fixture = Fixture {
            session: pending,
            ..fixture
        };
        let recovered = event(
            &fixture,
            2,
            CommandAckStatus::Recovered,
            Some("FINAM_CONFLICT_0002"),
            Some(CommandAckReasonCode::RecoveredByBrokerTruth),
        );
        assert_eq!(
            expect_blocked(apply_stage5g_mock_ack(fixture.session, recovered)).reason(),
            Stage5gMockAckError::BrokerOrderIdConflict
        );
    }

    #[test]
    fn wrong_account_instrument_side_and_action_block_before_callback() {
        let fixture = make_fixture();
        let mut wrong_account = event(&fixture, 1, CommandAckStatus::Accepted, None, None);
        wrong_account.account_id = BrokerAccountId::new("ACC_TEST_0002");
        let blocked = expect_blocked(apply_stage5g_mock_ack(fixture.session, wrong_account));
        assert_eq!(blocked.reason(), Stage5gMockAckError::AccountMismatch);

        let fixture = Fixture {
            session: blocked.into_session(),
            ..fixture
        };
        let mut wrong_instrument = event(&fixture, 1, CommandAckStatus::Accepted, None, None);
        wrong_instrument.instrument.symbol = "RTS-9.26".to_string();
        let blocked = expect_blocked(apply_stage5g_mock_ack(fixture.session, wrong_instrument));
        assert_eq!(blocked.reason(), Stage5gMockAckError::InstrumentMismatch);

        let fixture = Fixture {
            session: blocked.into_session(),
            ..fixture
        };
        let mut wrong_side = event(&fixture, 1, CommandAckStatus::Accepted, None, None);
        wrong_side.side = Some(opposite(fixture.side));
        let blocked = expect_blocked(apply_stage5g_mock_ack(fixture.session, wrong_side));
        assert_eq!(blocked.reason(), Stage5gMockAckError::SideMismatch);

        let fixture = Fixture {
            session: blocked.into_session(),
            ..fixture
        };
        let mut wrong_action = event(&fixture, 1, CommandAckStatus::Accepted, None, None);
        wrong_action.action = Stage5gMockIntentAction::Place {
            place_kind: Stage5gMockPlaceKind::Limit,
        };
        assert_eq!(
            expect_blocked(apply_stage5g_mock_ack(fixture.session, wrong_action)).reason(),
            Stage5gMockAckError::ActionMismatch
        );
    }

    #[test]
    fn duplicate_ack_terminal_twice_and_expired_lifecycle_block() {
        let fixture = make_fixture();
        let first = event(&fixture, 1, CommandAckStatus::Timeout, None, None);
        let pending = apply_stage5g_mock_ack(fixture.session, first)
            .unwrap()
            .into_awaiting()
            .unwrap();
        let fixture = Fixture {
            session: pending,
            ..fixture
        };
        let repeated_timeout = event(&fixture, 2, CommandAckStatus::Timeout, None, None);
        assert_eq!(
            expect_blocked(apply_stage5g_mock_ack(fixture.session, repeated_timeout)).reason(),
            Stage5gMockAckError::DuplicateAck
        );

        let fixture = make_fixture();
        let accepted = event(
            &fixture,
            1,
            CommandAckStatus::Accepted,
            Some("FINAM_TERMINAL_0001"),
            None,
        );
        let repeated_accepted = event(
            &fixture,
            2,
            CommandAckStatus::Accepted,
            Some("FINAM_TERMINAL_0001"),
            None,
        );
        let resolved = apply_stage5g_mock_ack(fixture.session, accepted)
            .unwrap()
            .into_resolved()
            .unwrap();
        assert_eq!(
            expect_replay_blocked(apply_stage5g_duplicate_after_resolution(
                resolved,
                repeated_accepted,
            ))
            .reason(),
            Stage5gMockAckError::TerminalAckTwice
        );

        let fixture = make_fixture();
        let mut late = event(&fixture, 1, CommandAckStatus::Accepted, None, None);
        late.ack.received_ts = Utc
            .timestamp_opt(fixture.bar_close_ts + 301, 0)
            .single()
            .unwrap();
        assert_eq!(
            expect_blocked(apply_stage5g_mock_ack(fixture.session, late)).reason(),
            Stage5gMockAckError::AckAfterLifecycleExpiry
        );
    }

    #[test]
    fn cancel_binding_is_exact_and_carries_no_side() {
        let account = BrokerAccountId::new("ACC_TEST_0001");
        let instrument = target();
        let bar_close_ts = 1_786_435_800;
        let target_order_id = BrokerOrderId::new("FINAM_CANCEL_TARGET_0001");
        let request_id = crate::deterministic_request_id(
            "hybrid_imoexf",
            account.as_str(),
            &instrument.symbol,
            &format!("cancel:{}", target_order_id.as_str()),
            bar_close_ts,
            1,
        );
        let binding = Stage5gMockIntentBinding {
            request_id,
            intent_class: BrokerNeutralHybridIntentClass::CancelCleanup,
            action: Stage5gMockIntentAction::Cancel { target_order_id },
            side: None,
        };
        assert!(stage5g_action_matches_class(
            &binding.action,
            binding.intent_class
        ));
        assert!(stage5g_side_shape_is_valid(&binding.action, binding.side));
        assert!(stage5g_binding_request_identity_matches(
            "hybrid_imoexf",
            &account,
            &instrument,
            bar_close_ts,
            &binding,
        ));
    }

    #[test]
    fn lifecycle_fingerprint_is_deterministic_for_same_input() {
        let bar_close_ts = Utc::now().timestamp().div_euclid(600) * 600;
        let left = make_fixture_at(bar_close_ts);
        let right = make_fixture_at(bar_close_ts);
        assert_eq!(
            left.session.lifecycle_fingerprint_sha256(),
            right.session.lifecycle_fingerprint_sha256()
        );
        let left_ack = event(
            &left,
            1,
            CommandAckStatus::Accepted,
            Some("FINAM_FP_0001"),
            None,
        );
        let right_ack = event(
            &right,
            1,
            CommandAckStatus::Accepted,
            Some("FINAM_FP_0001"),
            None,
        );
        let left = apply_stage5g_mock_ack(left.session, left_ack)
            .unwrap()
            .into_resolved()
            .unwrap();
        let right = apply_stage5g_mock_ack(right.session, right_ack)
            .unwrap()
            .into_resolved()
            .unwrap();
        assert_eq!(
            left.transition_fingerprint_sha256(),
            right.transition_fingerprint_sha256()
        );
    }
}
