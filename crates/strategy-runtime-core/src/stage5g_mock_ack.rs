//! Stage 5G-b deterministic mock ACK attachment.
//!
//! This module owns only paper/mock ACK admission and correlation. It does not
//! read Redis, call FINAM, dispatch commands, create broker order identifiers,
//! or apply order/trade/position truth. Broker Core remains the ACK-policy
//! authority and Stage 5C-i remains the sole runtime callback authority.

use broker_core::command::{CommandAckReasonCode, CommandAckStatus};
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

pub const STAGE5G_MOCK_ACK_SCHEMA_VERSION: u16 = 4;

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
    pub latest_reason_code: Option<CommandAckReasonCode>,
    pub latest_received_ts_utc: Option<String>,
    pub canonical_total_sequence: Option<u64>,
    pub pending_disposition: Option<RuntimeAckPendingDisposition>,
    pub status_policy: Option<RuntimeAckStatusPolicy>,
    pub broker_order_id_domain_sha256: Option<String>,
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
    pub last_ack_received_ts_utc: Option<String>,
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
    canonical_decision: Option<RuntimeAckLifecycleDecision>,
    canonical_sequence: Option<u64>,
    state: Stage5gMockAckSlotState,
}

/// Pure deterministic lifecycle state. It carries no Stage 5C ownership and
/// cannot invoke a strategy callback. Production sessions wrap this state in
/// a linear `Stage5cSettledPaperStrategy`; focused evidence tests exercise the
/// exact same state machine without depending on Stage 5C's wall-clock facade.
struct Stage5gMockAckState {
    batch_summary: Stage5cPaperIntentBatchSummary,
    slots: Vec<Stage5gMockAckSlot>,
    lifecycle_expires_at_ts_utc: i64,
    last_total_sequence: Option<u64>,
    last_ack_received_ts_utc: Option<chrono::DateTime<chrono::Utc>>,
    duplicate_status_count: usize,
}

struct Stage5gMockAckAdmissionProjection {
    batch_summary: Stage5cPaperIntentBatchSummary,
    intent_classes: Vec<BrokerNeutralHybridIntentClass>,
    source_timestamps: Vec<(StrategyRequestId, i64)>,
}

/// Linear paper-only capability. It intentionally implements none of Clone,
/// Copy, Debug, Display, Default, Serialize or Deserialize.
pub struct Stage5gMockAckSession {
    settled: Stage5cSettledPaperStrategy,
    state: Stage5gMockAckState,
}

pub struct Stage5gResolvedMockAckPaperStrategy {
    resolved: Stage5cResolvedPaperIntentBatchStrategy,
    state: Stage5gMockAckState,
    pre_callback_lifecycle_fingerprint_sha256: String,
    transition_fingerprint_sha256: String,
}

/// Crate-private ownership token used only by the next accepted Stage 5G
/// lifecycle slice. It keeps the accepted ACK state linear while allowing
/// Stage 5G-c to delegate the terminal broker-event vector to Stage 5C-j.
pub(crate) struct Stage5gResolvedMockAckContext {
    state: Stage5gMockAckState,
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
    NotYetSourceAuthenticated,
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
    MissingBrokerOrderIdAfterObservedIdentity,
    AckReasonIncoherent,
    NoSendProofContradictsBrokerIdentity,
    NoSendProofContradictsPriorLifecycleEvidence,
    NonMonotonicAckTime,
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
    let projection = {
        let batch = settled.intent_batch();
        Stage5gMockAckAdmissionProjection {
            batch_summary: stage5g_batch_summary(batch),
            intent_classes: batch.intent_classes(),
            source_timestamps: batch.record_source_event_ts_by_request(),
        }
    };
    match stage5g_build_mock_ack_state(projection, input) {
        Ok(state) => Ok(Stage5gMockAckSession { settled, state }),
        Err(reason) => Err(stage5g_admission_block(reason, settled)),
    }
}

fn stage5g_build_mock_ack_state(
    projection: Stage5gMockAckAdmissionProjection,
    input: Stage5gMockAckSessionInput,
) -> Result<Stage5gMockAckState, Stage5gMockAckAdmissionError> {
    let Stage5gMockAckAdmissionProjection {
        batch_summary,
        intent_classes,
        source_timestamps,
    } = projection;
    if batch_summary.intent_count == 0 {
        return Err(Stage5gMockAckAdmissionError::EmptyIntentBatch);
    }
    if batch_summary.observation_only {
        return Err(Stage5gMockAckAdmissionError::ObservationOnlyBatch);
    }
    let max_source_event_ts = source_timestamps
        .iter()
        .map(|(_, timestamp)| *timestamp)
        .max()
        .unwrap_or(batch_summary.bar_close_ts);
    if input.lifecycle_expires_at_ts_utc <= batch_summary.bar_close_ts
        || input.lifecycle_expires_at_ts_utc < max_source_event_ts
    {
        return Err(Stage5gMockAckAdmissionError::InvalidLifecycleExpiry);
    }
    if input.intent_bindings.len() != batch_summary.intent_count {
        return Err(Stage5gMockAckAdmissionError::BindingCountMismatch);
    }
    if source_timestamps.len() != batch_summary.intent_count
        || intent_classes.len() != batch_summary.intent_count
    {
        return Err(Stage5gMockAckAdmissionError::BindingCountMismatch);
    }

    let mut slots = Vec::with_capacity(input.intent_bindings.len());
    for (index, binding) in input.intent_bindings.into_iter().enumerate() {
        if binding.request_id != batch_summary.request_ids[index]
            || source_timestamps[index].0 != binding.request_id
        {
            return Err(Stage5gMockAckAdmissionError::BindingRequestOrderMismatch);
        }
        if binding.intent_class != intent_classes[index] {
            return Err(Stage5gMockAckAdmissionError::BindingIntentClassMismatch);
        }
        if !stage5g_action_matches_class(&binding.action, binding.intent_class) {
            return Err(Stage5gMockAckAdmissionError::BindingActionClassMismatch);
        }
        if !stage5g_side_shape_is_valid(&binding.action, binding.side) {
            return Err(Stage5gMockAckAdmissionError::BindingSideShapeMismatch);
        }
        if !stage5g_binding_request_identity_matches(
            &batch_summary.strategy_id,
            &batch_summary.account_id,
            &batch_summary.instrument,
            batch_summary.bar_close_ts,
            &binding,
        ) {
            return Err(Stage5gMockAckAdmissionError::BindingRequestIdentityMismatch);
        }
        slots.push(Stage5gMockAckSlot {
            expected_client_order_id: ClientOrderId::from_strategy_request(binding.request_id),
            binding,
            source_event_ts_utc: source_timestamps[index].1,
            observed_broker_order_id: None,
            latest_ack: None,
            latest_decision: None,
            canonical_ack: None,
            canonical_decision: None,
            canonical_sequence: None,
            state: Stage5gMockAckSlotState::Waiting,
        });
    }

    Ok(Stage5gMockAckState {
        batch_summary,
        slots,
        lifecycle_expires_at_ts_utc: input.lifecycle_expires_at_ts_utc,
        last_total_sequence: None,
        last_ack_received_ts_utc: None,
        duplicate_status_count: 0,
    })
}

pub fn apply_stage5g_mock_ack(
    session: Stage5gMockAckSession,
    event: Stage5gMockAckEvent,
) -> Result<Stage5gMockAckTransition, Stage5gMockAckFailure> {
    let Stage5gMockAckSession { settled, state } = session;
    match stage5g_apply_mock_ack_state(state, event) {
        Ok(Stage5gMockAckStateTransition::Awaiting(state)) => {
            Ok(Stage5gMockAckTransition::Awaiting(Stage5gMockAckSession {
                settled,
                state,
            }))
        }
        Ok(Stage5gMockAckStateTransition::Complete(state)) => {
            stage5g_resolve_complete_session(Stage5gMockAckSession { settled, state })
        }
        Err(blocked) => {
            let Stage5gMockAckStateBlocked { reason, state } = *blocked;
            Err(stage5g_block(
                reason,
                Stage5gMockAckSession { settled, state },
            ))
        }
    }
}

enum Stage5gMockAckStateTransition {
    Awaiting(Stage5gMockAckState),
    Complete(Stage5gMockAckState),
}

struct Stage5gMockAckStateBlocked {
    reason: Stage5gMockAckError,
    state: Stage5gMockAckState,
}

fn stage5g_apply_mock_ack_state(
    mut state: Stage5gMockAckState,
    event: Stage5gMockAckEvent,
) -> Result<Stage5gMockAckStateTransition, Box<Stage5gMockAckStateBlocked>> {
    let block = |reason, state| Box::new(Stage5gMockAckStateBlocked { reason, state });
    let slot_index = match stage5g_preflight_event(&state, &event) {
        Ok(index) => index,
        Err(reason) => return Err(block(reason, state)),
    };

    if let Some(reason) = stage5g_no_send_proof_contradiction(&state.slots[slot_index], &event.ack)
    {
        state.last_total_sequence = Some(event.total_sequence);
        state.last_ack_received_ts_utc = Some(event.ack.received_ts);
        let slot = &mut state.slots[slot_index];
        if let Some(broker_order_id) = &event.ack.broker_order_id {
            slot.observed_broker_order_id = Some(broker_order_id.clone());
        }
        slot.latest_ack = Some(event.ack);
        // This blocker is deliberately evaluated before Broker Core policy;
        // do not pair the contradictory ACK with a stale prior decision.
        slot.latest_decision = None;
        slot.state = Stage5gMockAckSlotState::ManualInterventionRequired;
        return Err(block(reason, state));
    }

    if stage5g_terminal_ack_loses_observed_broker_identity(&state.slots[slot_index], &event.ack) {
        state.last_total_sequence = Some(event.total_sequence);
        state.last_ack_received_ts_utc = Some(event.ack.received_ts);
        let slot = &mut state.slots[slot_index];
        slot.latest_ack = Some(event.ack);
        slot.latest_decision = None;
        slot.state = Stage5gMockAckSlotState::ManualInterventionRequired;
        return Err(block(
            Stage5gMockAckError::MissingBrokerOrderIdAfterObservedIdentity,
            state,
        ));
    }

    let disposition = match stage5g_event_disposition(&state.slots[slot_index], &event.ack) {
        Ok(disposition) => disposition,
        Err(reason) => return Err(block(reason, state)),
    };

    state.last_total_sequence = Some(event.total_sequence);
    state.last_ack_received_ts_utc = Some(event.ack.received_ts);
    let slot = &mut state.slots[slot_index];
    if let Some(broker_order_id) = &event.ack.broker_order_id {
        slot.observed_broker_order_id = Some(broker_order_id.clone());
    }
    slot.latest_ack = Some(event.ack.clone());
    slot.latest_decision = Some(disposition.decision.clone());

    match disposition.kind {
        Stage5gEventDispositionKind::Canonical => {
            slot.canonical_sequence = Some(event.total_sequence);
            slot.canonical_ack = Some(event.ack);
            slot.canonical_decision = Some(disposition.decision);
            slot.state = Stage5gMockAckSlotState::Resolved;
        }
        Stage5gEventDispositionKind::Awaiting(slot_state) => {
            slot.state = slot_state;
            return Ok(Stage5gMockAckStateTransition::Awaiting(state));
        }
        Stage5gEventDispositionKind::DuplicateNoop => {
            state.duplicate_status_count += 1;
        }
    }

    if state.slots.iter().any(|slot| slot.canonical_ack.is_none()) {
        Ok(Stage5gMockAckStateTransition::Awaiting(state))
    } else {
        Ok(Stage5gMockAckStateTransition::Complete(state))
    }
}

pub fn apply_stage5g_duplicate_after_resolution(
    resolved: Stage5gResolvedMockAckPaperStrategy,
    event: Stage5gMockAckEvent,
) -> Result<Stage5gResolvedMockAckPaperStrategy, Box<Stage5gResolvedMockAckReplayBlocked>> {
    let Stage5gResolvedMockAckPaperStrategy {
        resolved: stage5c_resolved,
        state,
        pre_callback_lifecycle_fingerprint_sha256,
        transition_fingerprint_sha256,
    } = resolved;
    let state = match stage5g_apply_duplicate_to_resolved_state(state, event) {
        Ok(state) => state,
        Err(blocked) => {
            let Stage5gMockAckStateBlocked { reason, state } = *blocked;
            return Err(Box::new(Stage5gResolvedMockAckReplayBlocked {
                reason,
                resolved: Stage5gResolvedMockAckPaperStrategy {
                    resolved: stage5c_resolved,
                    state,
                    pre_callback_lifecycle_fingerprint_sha256,
                    transition_fingerprint_sha256,
                },
            }));
        }
    };
    let mut resolved = Stage5gResolvedMockAckPaperStrategy {
        resolved: stage5c_resolved,
        state,
        pre_callback_lifecycle_fingerprint_sha256,
        transition_fingerprint_sha256: String::new(),
    };
    resolved.transition_fingerprint_sha256 = stage5g_resolved_fingerprint(&resolved);
    Ok(resolved)
}

fn stage5g_apply_duplicate_to_resolved_state(
    mut state: Stage5gMockAckState,
    event: Stage5gMockAckEvent,
) -> Result<Stage5gMockAckState, Box<Stage5gMockAckStateBlocked>> {
    let block = |reason, state| Box::new(Stage5gMockAckStateBlocked { reason, state });
    if event.total_sequence <= state.last_total_sequence.unwrap_or(0) {
        return Err(block(Stage5gMockAckError::NonMonotonicSequence, state));
    }
    if state
        .last_ack_received_ts_utc
        .as_ref()
        .is_some_and(|last| event.ack.received_ts < *last)
    {
        return Err(block(Stage5gMockAckError::NonMonotonicAckTime, state));
    }
    let Some(slot_index) = state
        .slots
        .iter()
        .position(|slot| slot.binding.request_id == event.intent_request_id)
    else {
        return Err(block(Stage5gMockAckError::UnknownIntentRequestId, state));
    };
    if let Err(reason) = stage5g_validate_route(
        &state.batch_summary,
        state.lifecycle_expires_at_ts_utc,
        &state.slots[slot_index],
        &event,
    ) {
        return Err(block(reason, state));
    }
    if event.ack.status != CommandAckStatus::Duplicate {
        return Err(block(Stage5gMockAckError::TerminalAckTwice, state));
    }
    if !stage5g_duplicate_matches_prior(&state.slots[slot_index], &event.ack) {
        return Err(block(
            Stage5gMockAckError::DuplicateStatusIdentityMismatch,
            state,
        ));
    }
    state.last_total_sequence = Some(event.total_sequence);
    state.last_ack_received_ts_utc = Some(event.ack.received_ts);
    state.duplicate_status_count += 1;
    Ok(state)
}

impl Stage5gMockAckSession {
    pub fn summary(&self) -> Stage5gMockAckSessionSummary {
        stage5g_state_summary(&self.state)
    }

    pub fn lifecycle_fingerprint_sha256(&self) -> String {
        stage5g_state_fingerprint(&self.state)
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
    pub(crate) fn source_intent_projections(
        &self,
    ) -> Vec<crate::stage5c_paper_host::Stage5gSourceIntentProjection> {
        self.resolved.stage5g_source_intent_projections()
    }

    pub fn lifecycle_summary(&self) -> Stage5gMockAckSessionSummary {
        stage5g_state_summary(&self.state)
    }

    pub fn batch_summary(&self) -> &Stage5cPaperIntentBatchSummary {
        &self.state.batch_summary
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
        self.state.duplicate_status_count
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

    pub(crate) fn into_stage5g_c_parts(
        self,
    ) -> (
        Stage5cResolvedPaperIntentBatchStrategy,
        Stage5gResolvedMockAckContext,
    ) {
        (
            self.resolved,
            Stage5gResolvedMockAckContext {
                state: self.state,
                pre_callback_lifecycle_fingerprint_sha256: self
                    .pre_callback_lifecycle_fingerprint_sha256,
                transition_fingerprint_sha256: self.transition_fingerprint_sha256,
            },
        )
    }

    pub(crate) fn from_stage5g_c_parts(
        resolved: Stage5cResolvedPaperIntentBatchStrategy,
        context: Stage5gResolvedMockAckContext,
    ) -> Self {
        Self {
            resolved,
            state: context.state,
            pre_callback_lifecycle_fingerprint_sha256: context
                .pre_callback_lifecycle_fingerprint_sha256,
            transition_fingerprint_sha256: context.transition_fingerprint_sha256,
        }
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
    state: &Stage5gMockAckState,
    event: &Stage5gMockAckEvent,
) -> Result<usize, Stage5gMockAckError> {
    if state
        .last_total_sequence
        .is_some_and(|last| event.total_sequence <= last)
    {
        return Err(Stage5gMockAckError::NonMonotonicSequence);
    }
    if state
        .last_ack_received_ts_utc
        .as_ref()
        .is_some_and(|last| event.ack.received_ts < *last)
    {
        return Err(Stage5gMockAckError::NonMonotonicAckTime);
    }
    let Some(slot_index) = state
        .slots
        .iter()
        .position(|slot| slot.binding.request_id == event.intent_request_id)
    else {
        return Err(Stage5gMockAckError::UnknownIntentRequestId);
    };
    stage5g_validate_route(
        &state.batch_summary,
        state.lifecycle_expires_at_ts_utc,
        &state.slots[slot_index],
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
    if !stage5g_ack_reason_is_coherent(&event.ack) {
        return Err(Stage5gMockAckError::AckReasonIncoherent);
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
    let expected_broker_order_id = slot
        .observed_broker_order_id
        .as_ref()
        .or(prior.broker_order_id.as_ref());
    ack.request_id == prior.request_id
        && ack.client_order_id == prior.client_order_id
        && ack.reason.as_ref().map(|reason| reason.code)
            == Some(CommandAckReasonCode::DuplicateCommand)
        && match (ack.broker_order_id.as_ref(), expected_broker_order_id) {
            (Some(incoming), Some(expected)) => incoming == expected,
            (None, None) => true,
            _ => false,
        }
}

fn stage5g_no_send_proof_contradiction(
    slot: &Stage5gMockAckSlot,
    ack: &CommandAck,
) -> Option<Stage5gMockAckError> {
    if ack.status != CommandAckStatus::Expired {
        return None;
    }
    if slot.observed_broker_order_id.is_some() || ack.broker_order_id.is_some() {
        return Some(Stage5gMockAckError::NoSendProofContradictsBrokerIdentity);
    }
    let exact_proof =
        ack.reason.as_ref().map(|reason| reason.code) == Some(CommandAckReasonCode::ExpiredCommand);
    if !exact_proof {
        return None;
    }
    let direct_waiting =
        slot.state == Stage5gMockAckSlotState::Waiting && slot.latest_ack.is_none();
    let prior_unproved_expiry = slot.state == Stage5gMockAckSlotState::NoSendProofRequired
        && slot.latest_ack.as_ref().is_some_and(|prior| {
            prior.status == CommandAckStatus::Expired
                && prior.reason.is_none()
                && prior.broker_order_id.is_none()
        });
    if direct_waiting || prior_unproved_expiry {
        None
    } else {
        Some(Stage5gMockAckError::NoSendProofContradictsPriorLifecycleEvidence)
    }
}

fn stage5g_terminal_ack_loses_observed_broker_identity(
    slot: &Stage5gMockAckSlot,
    ack: &CommandAck,
) -> bool {
    slot.observed_broker_order_id.is_some()
        && ack.broker_order_id.is_none()
        && matches!(
            ack.status,
            CommandAckStatus::Accepted | CommandAckStatus::Recovered | CommandAckStatus::Rejected
        )
}

fn stage5g_ack_reason_is_coherent(ack: &CommandAck) -> bool {
    let reason = ack.reason.as_ref().map(|reason| reason.code);
    match ack.status {
        CommandAckStatus::Accepted => reason.is_none(),
        CommandAckStatus::Submitted => {
            reason.is_none() || reason == Some(CommandAckReasonCode::SyntheticSubmitted)
        }
        CommandAckStatus::Recovered => reason == Some(CommandAckReasonCode::RecoveredByBrokerTruth),
        CommandAckStatus::Rejected => matches!(
            reason,
            Some(
                CommandAckReasonCode::FeatureDisabled
                    | CommandAckReasonCode::LocalValidationRejected
                    | CommandAckReasonCode::BrokerRejected
                    | CommandAckReasonCode::RateLimited
                    | CommandAckReasonCode::BrokerMaintenance
                    | CommandAckReasonCode::TradingWindowClosed
                    | CommandAckReasonCode::Unauthorized
            )
        ),
        CommandAckStatus::Duplicate => reason == Some(CommandAckReasonCode::DuplicateCommand),
        CommandAckStatus::Expired => {
            reason.is_none() || reason == Some(CommandAckReasonCode::ExpiredCommand)
        }
        CommandAckStatus::Error => matches!(
            reason,
            Some(
                CommandAckReasonCode::ManualInterventionRequired
                    | CommandAckReasonCode::ResponseDecodeError
                    | CommandAckReasonCode::ReconciliationRequired
                    | CommandAckReasonCode::RateLimited
                    | CommandAckReasonCode::BrokerMaintenance
                    | CommandAckReasonCode::Unauthorized
            )
        ),
        CommandAckStatus::Timeout => matches!(
            reason,
            None | Some(
                CommandAckReasonCode::TransportTimeout
                    | CommandAckReasonCode::TimeoutUnknownPending
                    | CommandAckReasonCode::CancelTimeoutUnknownPending
            )
        ),
        CommandAckStatus::UnknownPending => matches!(
            reason,
            None | Some(
                CommandAckReasonCode::TimeoutUnknownPending
                    | CommandAckReasonCode::CancelTimeoutUnknownPending
                    | CommandAckReasonCode::ReconciliationRequired
            )
        ),
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
    let pre_callback_lifecycle_fingerprint_sha256 = stage5g_state_fingerprint(&session.state);
    let Stage5gMockAckSession { settled, state } = session;
    let ack_records = state
        .slots
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
                state,
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
                    state,
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

fn stage5g_state_summary(state: &Stage5gMockAckState) -> Stage5gMockAckSessionSummary {
    let slots = state.slots.iter().map(stage5g_slot_summary).collect();
    let mut summary = Stage5gMockAckSessionSummary {
        schema_version: STAGE5G_MOCK_ACK_SCHEMA_VERSION,
        strategy_id: state.batch_summary.strategy_id.clone(),
        account_id: state.batch_summary.account_id.clone(),
        instrument: state.batch_summary.instrument.clone(),
        origin_bar_close_ts: state.batch_summary.origin_bar_close_ts,
        lifecycle_expires_at_ts_utc: state.lifecycle_expires_at_ts_utc,
        last_total_sequence: state.last_total_sequence,
        last_ack_received_ts_utc: state
            .last_ack_received_ts_utc
            .as_ref()
            .map(stage5g_ack_timestamp),
        duplicate_status_count: state.duplicate_status_count,
        resolved_count: state
            .slots
            .iter()
            .filter(|slot| slot.state == Stage5gMockAckSlotState::Resolved)
            .count(),
        slot_count: state.slots.len(),
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
        latest_reason_code: slot
            .latest_ack
            .as_ref()
            .and_then(|ack| ack.reason.as_ref().map(|reason| reason.code)),
        latest_received_ts_utc: slot
            .latest_ack
            .as_ref()
            .map(|ack| stage5g_ack_timestamp(&ack.received_ts)),
        canonical_total_sequence: slot.canonical_sequence,
        pending_disposition: slot
            .latest_decision
            .as_ref()
            .map(|decision| decision.pending_disposition),
        status_policy: slot
            .latest_decision
            .as_ref()
            .map(|decision| decision.status_policy),
        broker_order_id_domain_sha256: slot
            .observed_broker_order_id
            .as_ref()
            .map(stage5g_broker_order_id_domain_sha256),
    }
}

fn stage5g_state_fingerprint(state: &Stage5gMockAckState) -> String {
    stage5g_summary_fingerprint(&stage5g_state_summary_without_fingerprint(state))
}

fn stage5g_state_summary_without_fingerprint(
    state: &Stage5gMockAckState,
) -> Stage5gMockAckSessionSummary {
    Stage5gMockAckSessionSummary {
        schema_version: STAGE5G_MOCK_ACK_SCHEMA_VERSION,
        strategy_id: state.batch_summary.strategy_id.clone(),
        account_id: state.batch_summary.account_id.clone(),
        instrument: state.batch_summary.instrument.clone(),
        origin_bar_close_ts: state.batch_summary.origin_bar_close_ts,
        lifecycle_expires_at_ts_utc: state.lifecycle_expires_at_ts_utc,
        last_total_sequence: state.last_total_sequence,
        last_ack_received_ts_utc: state
            .last_ack_received_ts_utc
            .as_ref()
            .map(stage5g_ack_timestamp),
        duplicate_status_count: state.duplicate_status_count,
        resolved_count: state
            .slots
            .iter()
            .filter(|slot| slot.state == Stage5gMockAckSlotState::Resolved)
            .count(),
        slot_count: state.slots.len(),
        slots: state.slots.iter().map(stage5g_slot_summary).collect(),
        lifecycle_fingerprint_sha256: String::new(),
        mock_feedback_only: true,
        broker_truth_changed: false,
        redis_attached: false,
        finam_transport_attached: false,
    }
}

fn stage5g_summary_fingerprint<T: Serialize>(value: &T) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"moex.stage5g.mock-ack-lifecycle.v4\0");
    hasher.update(serde_json::to_vec(value).expect("Stage 5G summary serializes"));
    stage5g_sha256_hex(hasher.finalize())
}

fn stage5g_resolved_fingerprint(resolved: &Stage5gResolvedMockAckPaperStrategy) -> String {
    stage5g_transition_fingerprint(
        &resolved.state,
        &resolved.pre_callback_lifecycle_fingerprint_sha256,
        &resolved.post_lifecycle_state_fingerprint(),
    )
}

fn stage5g_transition_fingerprint(
    state: &Stage5gMockAckState,
    pre_callback_lifecycle_fingerprint_sha256: &str,
    post_lifecycle_state_fingerprint: &str,
) -> String {
    #[derive(Serialize)]
    struct Projection<'a> {
        batch: &'a Stage5cPaperIntentBatchSummary,
        pre_callback_lifecycle_fingerprint_sha256: &'a str,
        current_lifecycle_fingerprint_sha256: String,
        post_lifecycle_state_fingerprint: String,
        ordered_canonical_ack_projection: Vec<Stage5gCanonicalAckFingerprintProjection>,
        last_total_sequence: u64,
        duplicate_status_count: usize,
        broker_truth_changed: bool,
    }
    stage5g_summary_fingerprint(&Projection {
        batch: &state.batch_summary,
        pre_callback_lifecycle_fingerprint_sha256,
        current_lifecycle_fingerprint_sha256: stage5g_state_fingerprint(state),
        post_lifecycle_state_fingerprint: post_lifecycle_state_fingerprint.to_string(),
        ordered_canonical_ack_projection: state
            .slots
            .iter()
            .map(stage5g_canonical_ack_fingerprint_projection)
            .collect(),
        last_total_sequence: state
            .last_total_sequence
            .expect("complete Stage 5G state consumed at least one ACK"),
        duplicate_status_count: state.duplicate_status_count,
        broker_truth_changed: false,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct Stage5gCanonicalAckFingerprintProjection {
    request_id: StrategyRequestId,
    expected_client_order_id: ClientOrderId,
    intent_class: String,
    action: Stage5gMockIntentAction,
    side: Option<String>,
    status: CommandAckStatus,
    reason_code: Option<CommandAckReasonCode>,
    received_ts_utc: String,
    canonical_total_sequence: u64,
    pending_disposition: RuntimeAckPendingDisposition,
    status_policy: RuntimeAckStatusPolicy,
    broker_order_id_domain_sha256: Option<String>,
}

fn stage5g_canonical_ack_fingerprint_projection(
    slot: &Stage5gMockAckSlot,
) -> Stage5gCanonicalAckFingerprintProjection {
    let ack = slot
        .canonical_ack
        .as_ref()
        .expect("resolved Stage 5G slot has canonical ACK");
    let decision = slot
        .canonical_decision
        .as_ref()
        .expect("resolved Stage 5G slot has Broker Core decision");
    Stage5gCanonicalAckFingerprintProjection {
        request_id: ack.request_id,
        expected_client_order_id: slot.expected_client_order_id.clone(),
        intent_class: format!("{:?}", slot.binding.intent_class),
        action: slot.binding.action.clone(),
        side: slot.binding.side.map(|side| format!("{side:?}")),
        status: ack.status,
        reason_code: ack.reason.as_ref().map(|reason| reason.code),
        received_ts_utc: stage5g_ack_timestamp(&ack.received_ts),
        canonical_total_sequence: slot
            .canonical_sequence
            .expect("resolved Stage 5G slot has canonical sequence"),
        pending_disposition: decision.pending_disposition,
        status_policy: decision.status_policy,
        broker_order_id_domain_sha256: ack
            .broker_order_id
            .as_ref()
            .map(stage5g_broker_order_id_domain_sha256),
    }
}

fn stage5g_broker_order_id_domain_sha256(order_id: &BrokerOrderId) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"moex.stage5g.broker-order-id.v1\0");
    hasher.update(order_id.as_str().as_bytes());
    stage5g_sha256_hex(hasher.finalize())
}

fn stage5g_ack_timestamp(timestamp: &chrono::DateTime<chrono::Utc>) -> String {
    timestamp.to_rfc3339_opts(chrono::SecondsFormat::Nanos, true)
}

fn stage5g_sha256_hex(digest: impl AsRef<[u8]>) -> String {
    digest
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
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
    use chrono::{Duration, NaiveDate, NaiveTime, TimeZone, Timelike, Utc};
    use rust_decimal::Decimal;
    use uuid::Uuid;

    use super::*;
    use crate::hybrid_intraday::{
        BreakoutEodMode, HybridOrchestratorConfig, IntradayBreakoutConfig, MeanReversionConfig,
        MinRangeMode,
    };
    use crate::hybrid_intraday_runtime::{
        HybridIntradayProfile, HybridIntradayRuntimeConfig, HybridIntradayRuntimeStrategy,
        MeanReversionVariant, MrGatePolicy, RiskGateMode,
    };
    use crate::runtime_compat::{
        BarEvent, DataOrigin, GatewayPhase, MarketBuyAndCloseLiveOrderStyle, PaperExecutionMode,
        RiskGateRuntimeState, Strategy, StrategyCtx, TradeMode,
    };

    const ACCEPTED_STAGE5F_BAR_CLOSE_TS: i64 = 1_767_679_800;
    const DETERMINISTIC_POST_LIFECYCLE_FINGERPRINT: &str = "stage5g-r1-fixed-no-callback-evidence";

    struct Fixture {
        session: TestSession,
        request_id: StrategyRequestId,
        side: BrokerNeutralOrderSide,
        action: Stage5gMockIntentAction,
        account_id: BrokerAccountId,
        instrument: InstrumentId,
        bar_close_ts: i64,
    }

    struct ProductionFixture {
        session: Stage5gMockAckSession,
        request_id: StrategyRequestId,
        side: BrokerNeutralOrderSide,
        action: Stage5gMockIntentAction,
        bar_close_ts: i64,
        source_target_qty: f64,
        source_pre_position_qty: f64,
    }

    struct TestSession {
        state: Stage5gMockAckState,
    }

    struct TestResolved {
        state: Stage5gMockAckState,
        pre_callback_lifecycle_fingerprint_sha256: String,
        transition_fingerprint_sha256: String,
    }

    enum TestTransition {
        Awaiting(TestSession),
        Resolved(TestResolved),
    }

    impl TestTransition {
        fn into_awaiting(self) -> Option<TestSession> {
            match self {
                Self::Awaiting(session) => Some(session),
                Self::Resolved(_) => None,
            }
        }

        fn into_resolved(self) -> Option<TestResolved> {
            match self {
                Self::Awaiting(_) => None,
                Self::Resolved(resolved) => Some(resolved),
            }
        }
    }

    impl TestSession {
        fn summary(&self) -> Stage5gMockAckSessionSummary {
            stage5g_state_summary(&self.state)
        }

        fn lifecycle_fingerprint_sha256(&self) -> String {
            stage5g_state_fingerprint(&self.state)
        }
    }

    impl TestResolved {
        fn pre_callback_lifecycle_fingerprint_sha256(&self) -> &str {
            &self.pre_callback_lifecycle_fingerprint_sha256
        }

        fn transition_fingerprint_sha256(&self) -> &str {
            &self.transition_fingerprint_sha256
        }

        fn duplicate_status_count(&self) -> usize {
            self.state.duplicate_status_count
        }

        fn canonical_ack(&self) -> &CommandAck {
            self.state.slots[0]
                .canonical_ack
                .as_ref()
                .expect("resolved deterministic state has a canonical ACK")
        }
    }

    struct TestBlocked {
        reason: Stage5gMockAckError,
        session: TestSession,
    }

    impl TestBlocked {
        fn reason(&self) -> Stage5gMockAckError {
            self.reason
        }

        fn session(&self) -> &TestSession {
            &self.session
        }

        fn into_session(self) -> TestSession {
            self.session
        }
    }

    impl std::fmt::Debug for TestBlocked {
        fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter
                .debug_struct("TestBlocked")
                .field("reason", &self.reason)
                .finish_non_exhaustive()
        }
    }

    struct TestReplayBlocked {
        reason: Stage5gMockAckError,
        _resolved: TestResolved,
    }

    impl TestReplayBlocked {
        fn reason(&self) -> Stage5gMockAckError {
            self.reason
        }
    }

    impl std::fmt::Debug for TestReplayBlocked {
        fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter
                .debug_struct("TestReplayBlocked")
                .field("reason", &self.reason)
                .finish_non_exhaustive()
        }
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
        let mean_reversion = MeanReversionConfig {
            exit_offset: Duration::minutes(10),
            ..MeanReversionConfig::default()
        };
        let config = HybridIntradayRuntimeConfig {
            symbol: "IMOEXF".to_string(),
            profile: HybridIntradayProfile::ImoexfPrimaryRiskgateHigh180Lb120,
            mr_variant: MeanReversionVariant::High180,
            mr_gate_policy: MrGatePolicy::ShadowPnlLb120Positive,
            risk_gate_mode: RiskGateMode::NormalAppend,
            risk_gate_seed_file: None,
            risk_gate_ledger_key: None,
            model_session_start_time: NaiveTime::from_hms_opt(9, 0, 0),
            model_session_end_time: NaiveTime::from_hms_opt(23, 49, 59),
            qty: 3.0,
            live_order_style: MarketBuyAndCloseLiveOrderStyle::Market,
            tick_size: 0.5,
            marketable_limit_offset_ticks: 0,
            timezone_offset_hours: 3,
            session_close_hour: 23,
            session_close_minute: 49,
            weekends_off: true,
            stop_end_buffer_sec: 60,
            repair_deadline_sec: 180,
            sl_escalate_timeout_sec: 30,
            max_repair_retries: 3,
            repair_backoff_base_sec: 5,
            repair_backoff_max_sec: 60,
            pending_timeout_sec: 60,
            partial_entry_fill_timeout_ms: 3_000,
            mr_config: mean_reversion,
            breakout_config: IntradayBreakoutConfig {
                k: 0.53,
                stop1_range: 0.51,
                stop2_range: 0.35,
                big_move_threshold: 0.025,
                min_range: 1.01,
                min_range_mode: MinRangeMode::Absolute,
                exclude_weekends: true,
                wait_hours: 3.0,
            },
            orchestrator_config: HybridOrchestratorConfig {
                breakout_eod_mode: BreakoutEodMode::SameDay,
                breakout_overnight_exit_time: NaiveTime::from_hms_opt(9, 30, 0)
                    .expect("accepted Stage 5F overnight exit time"),
            },
        };
        let strategy = HybridIntradayRuntimeStrategy::new(config);
        assert_eq!(
            strategy.stage5d_canonical_config_fingerprint(),
            "stage5d_cfg_sha256:56141846cb180b8a224a1db7e1f5188c99c28f0fab88a27ebe65fbcb9d7cf626",
            "Stage 5G-b fixture must use the accepted Stage 5F target config"
        );
        strategy
    }

    fn production_integration_strategy(bar_close_ts: i64) -> HybridIntradayRuntimeStrategy {
        production_integration_strategy_with_style(
            bar_close_ts,
            MarketBuyAndCloseLiveOrderStyle::Market,
        )
    }

    fn production_integration_strategy_with_style(
        bar_close_ts: i64,
        live_order_style: MarketBuyAndCloseLiveOrderStyle,
    ) -> HybridIntradayRuntimeStrategy {
        let utc_bar_close = Utc
            .timestamp_opt(bar_close_ts, 0)
            .single()
            .expect("production bar close");
        let timezone_offset_hours = 9 - i32::try_from(utc_bar_close.hour()).unwrap();
        let local_bar_close = utc_bar_close + Duration::hours(i64::from(timezone_offset_hours));
        HybridIntradayRuntimeStrategy::new(HybridIntradayRuntimeConfig {
            symbol: "IMOEXF".to_string(),
            profile: HybridIntradayProfile::BaselineRuntimeHybrid,
            mr_variant: MeanReversionVariant::Author41BoundaryShort,
            mr_gate_policy: MrGatePolicy::Disabled,
            risk_gate_mode: RiskGateMode::Disabled,
            risk_gate_seed_file: None,
            risk_gate_ledger_key: None,
            model_session_start_time: Some((local_bar_close - Duration::minutes(10)).time()),
            model_session_end_time: Some((local_bar_close + Duration::hours(1)).time()),
            qty: 1.0,
            live_order_style,
            tick_size: 0.5,
            marketable_limit_offset_ticks: 0,
            timezone_offset_hours,
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
            breakout_config: IntradayBreakoutConfig {
                exclude_weekends: false,
                ..IntradayBreakoutConfig::default()
            },
            orchestrator_config: HybridOrchestratorConfig::default(),
        })
    }

    fn warm_production_strategy(strategy: &mut HybridIntradayRuntimeStrategy, bar_close_ts: i64) {
        for (close_time_utc, high, low) in [
            (bar_close_ts - 86_400 - 600, 2630.0, 2570.0),
            (bar_close_ts - 86_400, 2620.0, 2580.0),
        ] {
            assert!(Strategy::on_bar(
                strategy,
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
            )
            .is_empty());
        }
    }

    fn production_signal_bar(bar_close_ts: i64) -> broker_core::HybridRuntimeBarEvent {
        broker_core::HybridRuntimeBarEvent {
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
        }
    }

    fn accepted_stage5f_market_projection(
        bar_close_ts: i64,
    ) -> (
        Stage5gMockAckAdmissionProjection,
        Stage5gMockIntentBinding,
        BrokerNeutralOrderSide,
    ) {
        let mut strategy = strategy();
        Strategy::on_risk_gate_state(
            &mut strategy,
            &RiskGateRuntimeState {
                profile_id: "imoexf_primary_high180_lb120".to_string(),
                last_finalized_session_date: NaiveDate::from_ymd_opt(2026, 1, 5),
                rolling_sum_lb120: Some(158.6),
                mr_enabled_current_session: Some(true),
                mr_enabled_next_session: Some(true),
                ledger_rows_count: 221,
            },
        );
        for (close_time_utc, high, low) in [
            (bar_close_ts - 86_400 - 600, 102.0, 98.0),
            (bar_close_ts - 86_400, 101.0, 99.0),
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
                    o: 100.0,
                    h: high,
                    l: low,
                    close: 100.0,
                    v: 1.0,
                    origin: DataOrigin::Replay,
                },
            );
            assert!(intents.is_empty());
        }
        let bar = broker_core::HybridRuntimeBarEvent {
            instrument: target(),
            close_time_utc: bar_close_ts,
            open: 99.7,
            high: 102.0,
            low: 99.7,
            close: 99.7,
            volume: 10.0,
            origin: broker_core::HybridRuntimeBarOrigin::Live,
            is_final: true,
            timeframe_sec: 600,
        };
        let intents = crate::BrokerNeutralHybridStrategy::on_broker_bar(
            &mut strategy,
            broker_core::HybridRuntimeCallbackInput {
                context: broker_core::HybridRuntimeStrategyContext {
                    strategy_id: "hybrid_imoexf".to_string(),
                    request_namespace_account: BrokerAccountId::new("ACC_TEST_0001"),
                    instrument: target(),
                    tick_size: 0.5,
                    trade_mode: broker_core::HybridRuntimeTradeMode::Paper,
                    paper_execution_mode: broker_core::HybridRuntimePaperExecutionMode::LiveOnly,
                    allow_live_orders: false,
                    gateway_phase: broker_core::HybridRuntimeGatewayPhase::LiveReady,
                    position_qty: Some(0.0),
                    event_ts_utc: bar_close_ts,
                    strategy_now_ts_utc: bar_close_ts,
                    last_bar_ts_utc: Some(bar_close_ts - 600),
                },
                payload: bar,
            },
        )
        .expect("accepted Stage 5F broker-neutral market callback");
        assert_eq!(intents.len(), 1, "fixture needs one intent");
        let intent = &intents[0];
        let intent_class = intent
            .explicit_class()
            .expect("accepted Stage 5F intent is classified");
        let side = match intent.base_intent() {
            crate::BrokerNeutralHybridIntent::Market { side, .. } => *side,
            _ => panic!("accepted Stage 5F fixture must emit a market intent"),
        };
        let sequence = match side {
            BrokerNeutralOrderSide::Buy => 3,
            BrokerNeutralOrderSide::Sell => 4,
        };
        let request_id = crate::deterministic_request_id(
            "hybrid_imoexf",
            "ACC_TEST_0001",
            "IMOEXF",
            "market",
            bar_close_ts,
            sequence,
        );
        let action = Stage5gMockIntentAction::Place {
            place_kind: Stage5gMockPlaceKind::Market,
        };
        let binding = Stage5gMockIntentBinding {
            request_id,
            intent_class,
            action,
            side: Some(side),
        };
        let projection = Stage5gMockAckAdmissionProjection {
            batch_summary: Stage5cPaperIntentBatchSummary {
                strategy_id: "hybrid_imoexf".to_string(),
                account_id: BrokerAccountId::new("ACC_TEST_0001"),
                instrument: target(),
                origin_bar_close_ts: bar_close_ts,
                bar_close_ts,
                min_source_event_ts: bar_close_ts,
                max_source_event_ts: bar_close_ts,
                state_fingerprint:
                    crate::stage5c_paper_host::stage5e_test_owned_strategy_state_fingerprint(
                        &strategy,
                    ),
                request_ids: vec![request_id],
                intent_count: 1,
                observation_only: false,
            },
            intent_classes: vec![intent_class],
            source_timestamps: vec![(request_id, bar_close_ts)],
        };
        (projection, binding, side)
    }

    fn production_fixture(bar_close_ts: i64) -> ProductionFixture {
        let mut strategy = production_integration_strategy(bar_close_ts);
        warm_production_strategy(&mut strategy, bar_close_ts);
        let bar = production_signal_bar(bar_close_ts);
        let lifecycle_now = Utc.timestamp_opt(bar_close_ts - 30, 0).single().unwrap();
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
        let semantic = crate::apply_stage5c_semantic_bar(recovered, accepted)
            .expect("production Stage 5C semantic bar");
        let settled = crate::settle_stage5c_semantic_result(semantic)
            .expect("production Stage 5C settled Market batch");
        let request_id = settled.intent_batch().request_ids()[0];
        let source = settled.stage5g_source_intent_projections();
        let side = source[0]
            .side
            .expect("production Market source has an exact side");
        let source_target_qty = source[0]
            .target_qty
            .expect("production Market source has an exact target quantity");
        let source_pre_position_qty = source[0].pre_position_qty;
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
        .expect("public Stage 5G production attachment");
        ProductionFixture {
            session,
            request_id,
            side,
            action,
            bar_close_ts,
            source_target_qty,
            source_pre_position_qty,
        }
    }

    fn production_fixture_from_stage5f_row(row_id: &str) -> ProductionFixture {
        let settled =
            crate::stage5f_atomic_hybrid_semantics::stage5g_source_settled_fixture(row_id);
        let request_id = settled.intent_batch().request_ids()[0];
        let bar_close_ts = settled.intent_batch().bar_close_ts();
        let source = settled.stage5g_source_intent_projections();
        let side = source[0]
            .side
            .unwrap_or_else(|| panic!("{row_id}: Market source must have a side"));
        let source_target_qty = source[0]
            .target_qty
            .unwrap_or_else(|| panic!("{row_id}: Market source must have a target quantity"));
        let source_pre_position_qty = source[0].pre_position_qty;
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
        .unwrap_or_else(|_| panic!("{row_id}: Stage 5G-b attachment must pass"));
        ProductionFixture {
            session,
            request_id,
            side,
            action,
            bar_close_ts,
            source_target_qty,
            source_pre_position_qty,
        }
    }

    fn production_position(
        fixture: &ProductionFixture,
        qty: Decimal,
        received_offset_seconds: i64,
    ) -> broker_core::BrokerPositionSnapshot {
        let received_ts = Utc
            .timestamp_opt(fixture.bar_close_ts + received_offset_seconds, 0)
            .single()
            .unwrap();
        broker_core::BrokerPositionSnapshot {
            account_id: BrokerAccountId::new("ACC_TEST_0001"),
            instrument: target(),
            qty,
            avg_price: Some(Decimal::new(2_210, 0)),
            unrealized_pnl: None,
            source_ts: Some(received_ts),
            received_ts,
        }
    }

    fn production_truth(
        positions: Vec<broker_core::BrokerPositionSnapshot>,
        received_ts: chrono::DateTime<Utc>,
    ) -> broker_core::BrokerTruthSnapshot {
        broker_core::BrokerTruthSnapshot {
            account_id: BrokerAccountId::new("ACC_TEST_0001"),
            orders: Vec::new(),
            positions,
            cash: None,
            trades: Vec::new(),
            instruments: Vec::new(),
            received_ts,
        }
    }

    fn production_bar_close_ts() -> i64 {
        Utc::now().timestamp() - 10
    }

    fn production_event(
        fixture: &ProductionFixture,
        sequence: u64,
        status: CommandAckStatus,
        broker_order_id: Option<&str>,
        reason: Option<CommandAckReasonCode>,
    ) -> Stage5gMockAckEvent {
        Stage5gMockAckEvent {
            total_sequence: sequence,
            intent_request_id: fixture.request_id,
            account_id: BrokerAccountId::new("ACC_TEST_0001"),
            instrument: target(),
            action: fixture.action.clone(),
            side: Some(fixture.side),
            ack: CommandAck {
                request_id: fixture.request_id,
                client_order_id: Some(ClientOrderId::from_strategy_request(fixture.request_id)),
                broker_order_id: broker_order_id.map(BrokerOrderId::new),
                status,
                reason: reason.map(CommandAckReason::new),
                received_ts: Utc
                    .timestamp_opt(fixture.bar_close_ts + i64::try_from(sequence).unwrap(), 0)
                    .single()
                    .unwrap(),
            },
        }
    }

    fn make_fixture() -> Fixture {
        make_fixture_at(ACCEPTED_STAGE5F_BAR_CLOSE_TS)
    }

    fn make_fixture_at(bar_close_ts: i64) -> Fixture {
        let (projection, binding, side) = accepted_stage5f_market_projection(bar_close_ts);
        let request_id = binding.request_id;
        let action = binding.action.clone();
        let state = stage5g_build_mock_ack_state(
            projection,
            Stage5gMockAckSessionInput {
                intent_bindings: vec![binding],
                lifecycle_expires_at_ts_utc: bar_close_ts + 300,
            },
        )
        .expect("mock ACK session");
        Fixture {
            session: TestSession { state },
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

    fn resolve_test_duplicate_at(
        received_offset_seconds: i64,
    ) -> (TestResolved, Stage5gMockAckEvent) {
        let fixture = make_fixture();
        let accepted = event(
            &fixture,
            1,
            CommandAckStatus::Accepted,
            Some("FINAM_DUP_CONTINUATION_0001"),
            None,
        );
        let mut duplicate = event(
            &fixture,
            2,
            CommandAckStatus::Duplicate,
            Some("FINAM_DUP_CONTINUATION_0001"),
            Some(CommandAckReasonCode::DuplicateCommand),
        );
        duplicate.ack.received_ts = Utc
            .timestamp_opt(fixture.bar_close_ts + received_offset_seconds, 0)
            .single()
            .unwrap();
        let mut continuation = event(
            &fixture,
            3,
            CommandAckStatus::Duplicate,
            Some("FINAM_DUP_CONTINUATION_0001"),
            Some(CommandAckReasonCode::DuplicateCommand),
        );
        continuation.ack.received_ts = Utc
            .timestamp_opt(fixture.bar_close_ts + 25, 0)
            .single()
            .unwrap();
        let resolved = apply_test_mock_ack(fixture.session, accepted)
            .unwrap()
            .into_resolved()
            .unwrap();
        let resolved = apply_test_duplicate_after_resolution(resolved, duplicate).unwrap();
        (resolved, continuation)
    }

    fn resolve_single_ack(
        status: CommandAckStatus,
        broker_order_id: Option<&str>,
        reason: Option<CommandAckReasonCode>,
        total_sequence: u64,
        received_offset_seconds: i64,
    ) -> TestResolved {
        let fixture = make_fixture();
        let mut ack = event(&fixture, total_sequence, status, broker_order_id, reason);
        ack.ack.received_ts = Utc
            .timestamp_opt(fixture.bar_close_ts + received_offset_seconds, 0)
            .single()
            .expect("fixed ACK timestamp");
        apply_test_mock_ack(fixture.session, ack)
            .expect("canonical mock ACK")
            .into_resolved()
            .expect("single accepted Stage 5F market intent resolves")
    }

    fn opposite(side: BrokerNeutralOrderSide) -> BrokerNeutralOrderSide {
        match side {
            BrokerNeutralOrderSide::Buy => BrokerNeutralOrderSide::Sell,
            BrokerNeutralOrderSide::Sell => BrokerNeutralOrderSide::Buy,
        }
    }

    fn apply_test_mock_ack(
        session: TestSession,
        event: Stage5gMockAckEvent,
    ) -> Result<TestTransition, Box<TestBlocked>> {
        match stage5g_apply_mock_ack_state(session.state, event) {
            Ok(Stage5gMockAckStateTransition::Awaiting(state)) => {
                Ok(TestTransition::Awaiting(TestSession { state }))
            }
            Ok(Stage5gMockAckStateTransition::Complete(state)) => {
                let pre_callback_lifecycle_fingerprint_sha256 = stage5g_state_fingerprint(&state);
                let transition_fingerprint_sha256 = stage5g_transition_fingerprint(
                    &state,
                    &pre_callback_lifecycle_fingerprint_sha256,
                    DETERMINISTIC_POST_LIFECYCLE_FINGERPRINT,
                );
                Ok(TestTransition::Resolved(TestResolved {
                    state,
                    pre_callback_lifecycle_fingerprint_sha256,
                    transition_fingerprint_sha256,
                }))
            }
            Err(blocked) => Err(Box::new(TestBlocked {
                reason: blocked.reason,
                session: TestSession {
                    state: blocked.state,
                },
            })),
        }
    }

    fn apply_test_duplicate_after_resolution(
        resolved: TestResolved,
        event: Stage5gMockAckEvent,
    ) -> Result<TestResolved, Box<TestReplayBlocked>> {
        let TestResolved {
            state,
            pre_callback_lifecycle_fingerprint_sha256,
            transition_fingerprint_sha256,
        } = resolved;
        match stage5g_apply_duplicate_to_resolved_state(state, event) {
            Ok(state) => {
                let transition_fingerprint_sha256 = stage5g_transition_fingerprint(
                    &state,
                    &pre_callback_lifecycle_fingerprint_sha256,
                    DETERMINISTIC_POST_LIFECYCLE_FINGERPRINT,
                );
                Ok(TestResolved {
                    state,
                    pre_callback_lifecycle_fingerprint_sha256,
                    transition_fingerprint_sha256,
                })
            }
            Err(blocked) => Err(Box::new(TestReplayBlocked {
                reason: blocked.reason,
                _resolved: TestResolved {
                    state: blocked.state,
                    pre_callback_lifecycle_fingerprint_sha256,
                    transition_fingerprint_sha256,
                },
            })),
        }
    }

    fn expect_blocked(result: Result<TestTransition, Box<TestBlocked>>) -> TestBlocked {
        match result {
            Err(blocked) => *blocked,
            Ok(_) => panic!("expected Stage 5G mock ACK block"),
        }
    }

    fn expect_replay_blocked(
        result: Result<TestResolved, Box<TestReplayBlocked>>,
    ) -> TestReplayBlocked {
        match result {
            Err(blocked) => *blocked,
            Ok(_) => panic!("expected resolved ACK replay block"),
        }
    }

    #[test]
    fn production_public_attach_apply_accepted_resolves_stage5c_once() {
        let fixture = production_fixture(production_bar_close_ts());
        let ack = production_event(
            &fixture,
            1,
            CommandAckStatus::Accepted,
            Some("FINAM_PRODUCTION_ACCEPTED_0001"),
            None,
        );
        let resolved = apply_stage5g_mock_ack(fixture.session, ack)
            .unwrap()
            .into_resolved()
            .unwrap();
        assert_eq!(resolved.ack_outcomes().len(), 1);
        assert_eq!(
            resolved.ack_outcomes()[0]
                .broker_order_id
                .as_ref()
                .map(BrokerOrderId::as_str),
            Some("FINAM_PRODUCTION_ACCEPTED_0001")
        );
        assert!(!resolved.broker_truth_changed());
    }

    #[test]
    fn stage5gc_public_terminal_ack_converges_without_broker_callback() {
        let fixture = production_fixture(production_bar_close_ts());
        let received_ts = Utc
            .timestamp_opt(fixture.bar_close_ts + 2, 0)
            .single()
            .unwrap();
        let ack = production_event(
            &fixture,
            1,
            CommandAckStatus::Rejected,
            None,
            Some(CommandAckReasonCode::BrokerRejected),
        );
        let request_id = fixture.request_id;
        let resolved = apply_stage5g_mock_ack(fixture.session, ack)
            .expect("terminal ACK")
            .into_resolved()
            .expect("one terminal ACK resolves");
        let session =
            crate::attach_stage5g_order_position_session(resolved).expect("Stage 5G-c attachment");
        let transition = crate::apply_stage5g_order_position_evidence(
            session,
            crate::Stage5gOrderPositionEvidence {
                total_sequence: 2,
                request_id,
                broker_truth: broker_core::BrokerTruthSnapshot {
                    account_id: BrokerAccountId::new("ACC_TEST_0001"),
                    orders: Vec::new(),
                    positions: Vec::new(),
                    cash: None,
                    trades: Vec::new(),
                    instruments: Vec::new(),
                    received_ts,
                },
                order_attribution: None,
            },
        )
        .expect("terminal ACK requires no broker callback");
        let converged = transition.into_converged().expect("terminal convergence");
        assert_eq!(converged.summary().stage5c_callback_count, 0);
        assert_eq!(converged.summary().order_transition_count, 0);
        assert_eq!(converged.summary().position_confirmation_count, 0);
        assert!(!converged.redis_command_stream_attached());
        assert!(!converged.broker_transport_attached());
        assert!(!converged.broker_execution_attached());
    }

    #[test]
    fn stage5gc_r1_public_market_entry_exact_position_converges() {
        let fixture = production_fixture_from_stage5f_row("F02");
        let request_id = fixture.request_id;
        let target_qty = Decimal::from_f64_retain(fixture.source_target_qty).unwrap();
        let signed_target = match fixture.side {
            BrokerNeutralOrderSide::Buy => target_qty,
            BrokerNeutralOrderSide::Sell => -target_qty,
        };
        let ack = production_event(
            &fixture,
            1,
            CommandAckStatus::Accepted,
            Some("FINAM_PRODUCTION_R1_ENTRY_EXACT"),
            None,
        );
        let position = production_position(&fixture, signed_target, 2);
        let resolved = apply_stage5g_mock_ack(fixture.session, ack)
            .unwrap()
            .into_resolved()
            .unwrap();
        let session = crate::attach_stage5g_order_position_session(resolved).unwrap();
        let truth_ts = position.received_ts;
        let converged = crate::apply_stage5g_order_position_evidence(
            session,
            crate::Stage5gOrderPositionEvidence {
                total_sequence: 2,
                request_id,
                broker_truth: production_truth(vec![position], truth_ts),
                order_attribution: None,
            },
        )
        .unwrap()
        .into_converged()
        .expect("exact source target position must converge");
        assert_eq!(converged.summary().stage5c_callback_count, 1);
        assert_eq!(converged.summary().position_confirmation_count, 1);
    }

    #[test]
    fn stage5gc_r1_public_market_entry_partial_then_exact_converges() {
        let fixture = production_fixture_from_stage5f_row("F02");
        let request_id = fixture.request_id;
        let target_qty = Decimal::from_f64_retain(fixture.source_target_qty).unwrap();
        let partial = match fixture.side {
            BrokerNeutralOrderSide::Buy => target_qty / Decimal::new(2, 0),
            BrokerNeutralOrderSide::Sell => -target_qty / Decimal::new(2, 0),
        };
        let exact = match fixture.side {
            BrokerNeutralOrderSide::Buy => target_qty,
            BrokerNeutralOrderSide::Sell => -target_qty,
        };
        let ack = production_event(
            &fixture,
            1,
            CommandAckStatus::Accepted,
            Some("FINAM_PRODUCTION_R1_ENTRY_PARTIAL"),
            None,
        );
        let partial_position = production_position(&fixture, partial, 2);
        let exact_position = production_position(&fixture, exact, 3);
        let resolved = apply_stage5g_mock_ack(fixture.session, ack)
            .unwrap()
            .into_resolved()
            .unwrap();
        let session = crate::attach_stage5g_order_position_session(resolved).unwrap();
        let partial_ts = partial_position.received_ts;
        let awaiting = crate::apply_stage5g_order_position_evidence(
            session,
            crate::Stage5gOrderPositionEvidence {
                total_sequence: 2,
                request_id,
                broker_truth: production_truth(vec![partial_position], partial_ts),
                order_attribution: None,
            },
        )
        .unwrap()
        .into_awaiting()
        .expect("partial source target position must remain awaiting");
        let exact_ts = exact_position.received_ts;
        let converged = crate::apply_stage5g_order_position_evidence(
            awaiting,
            crate::Stage5gOrderPositionEvidence {
                total_sequence: 3,
                request_id,
                broker_truth: production_truth(vec![exact_position], exact_ts),
                order_attribution: None,
            },
        )
        .unwrap()
        .into_converged()
        .expect("later exact source target position must converge");
        assert_eq!(converged.summary().stage5c_callback_count, 1);
        assert_eq!(converged.summary().position_confirmation_count, 1);
    }

    #[test]
    fn stage5gc_r1_public_stage5f_f04_market_exit_flat_converges() {
        let fixture = production_fixture_from_stage5f_row("F04");
        let request_id = fixture.request_id;
        let ack = production_event(
            &fixture,
            1,
            CommandAckStatus::Accepted,
            Some("FINAM_PRODUCTION_R1_EXIT_FLAT"),
            None,
        );
        let position = production_position(&fixture, Decimal::ZERO, 2);
        let resolved = apply_stage5g_mock_ack(fixture.session, ack)
            .unwrap()
            .into_resolved()
            .unwrap();
        let session = crate::attach_stage5g_order_position_session(resolved).unwrap();
        let truth_ts = position.received_ts;
        let converged = crate::apply_stage5g_order_position_evidence(
            session,
            crate::Stage5gOrderPositionEvidence {
                total_sequence: 2,
                request_id,
                broker_truth: production_truth(vec![position], truth_ts),
                order_attribution: None,
            },
        )
        .unwrap()
        .into_converged()
        .expect("source-reachable Market Exit must converge only at flat");
        assert_eq!(converged.summary().stage5c_callback_count, 1);
    }

    #[test]
    fn stage5gc_r1_public_rejected_exit_preserves_existing_position() {
        let fixture = production_fixture_from_stage5f_row("F04");
        let request_id = fixture.request_id;
        let ack = production_event(
            &fixture,
            1,
            CommandAckStatus::Rejected,
            None,
            Some(CommandAckReasonCode::BrokerRejected),
        );
        let existing_qty = Decimal::from_f64_retain(fixture.source_pre_position_qty).unwrap();
        let existing = production_position(&fixture, existing_qty, 2);
        let resolved = apply_stage5g_mock_ack(fixture.session, ack)
            .unwrap()
            .into_resolved()
            .unwrap();
        let session = crate::attach_stage5g_order_position_session(resolved).unwrap();
        let truth_ts = existing.received_ts;
        let converged = crate::apply_stage5g_order_position_evidence(
            session,
            crate::Stage5gOrderPositionEvidence {
                total_sequence: 2,
                request_id,
                broker_truth: production_truth(vec![existing], truth_ts),
                order_attribution: None,
            },
        )
        .unwrap()
        .into_converged()
        .expect("rejected exit must retain the source pre-position");
        assert_eq!(converged.summary().stage5c_callback_count, 0);
    }

    #[test]
    fn stage5gc_r1_public_stage5c_preflight_block_restores_retryable_session() {
        let fixture = production_fixture_from_stage5f_row("F02");
        let request_id = fixture.request_id;
        let target_qty = Decimal::from_f64_retain(fixture.source_target_qty).unwrap();
        let signed_target = match fixture.side {
            BrokerNeutralOrderSide::Buy => target_qty,
            BrokerNeutralOrderSide::Sell => -target_qty,
        };
        let ack = production_event(
            &fixture,
            1,
            CommandAckStatus::Accepted,
            Some("FINAM_PRODUCTION_R1_PREFLIGHT_RETRY"),
            None,
        );
        let mut stale_source_position = production_position(&fixture, signed_target, 2);
        stale_source_position.source_ts = Utc.timestamp_opt(fixture.bar_close_ts, 0).single();
        let stale_received_ts = stale_source_position.received_ts;
        let corrected_position = production_position(&fixture, signed_target, 2);
        let corrected_received_ts = corrected_position.received_ts;
        let resolved = apply_stage5g_mock_ack(fixture.session, ack)
            .unwrap()
            .into_resolved()
            .unwrap();
        let session = crate::attach_stage5g_order_position_session(resolved).unwrap();
        let initial_fingerprint = session.summary().lifecycle_fingerprint_sha256;
        let failure = match crate::apply_stage5g_order_position_evidence(
            session,
            crate::Stage5gOrderPositionEvidence {
                total_sequence: 2,
                request_id,
                broker_truth: production_truth(vec![stale_source_position], stale_received_ts),
                order_attribution: None,
            },
        ) {
            Err(failure) => failure,
            Ok(_) => panic!("Stage 5C must reject broker source time before ACK"),
        };
        assert_eq!(
            failure.reason(),
            crate::Stage5gOrderPositionError::Stage5cPreCallbackBlocked
        );
        let blocked = failure
            .into_blocked()
            .expect("pre-callback failure remains retryable");
        assert_eq!(blocked.session().summary().last_total_sequence, None);
        assert_eq!(
            blocked.session().summary().lifecycle_fingerprint_sha256,
            initial_fingerprint,
            "failed terminal candidate must not mutate continuation state"
        );
        let converged = crate::apply_stage5g_order_position_evidence(
            blocked.into_session(),
            crate::Stage5gOrderPositionEvidence {
                total_sequence: 2,
                request_id,
                broker_truth: production_truth(vec![corrected_position], corrected_received_ts),
                order_attribution: None,
            },
        )
        .unwrap()
        .into_converged()
        .expect("corrected source timestamp must converge exactly once");
        assert_eq!(converged.summary().stage5c_callback_count, 1);
    }

    #[test]
    fn production_public_submitted_then_recovered_resolves_stage5c_once() {
        let fixture = production_fixture(production_bar_close_ts());
        let submitted = production_event(&fixture, 1, CommandAckStatus::Submitted, None, None);
        let pending = apply_stage5g_mock_ack(fixture.session, submitted)
            .unwrap()
            .into_awaiting()
            .unwrap();
        let fixture = ProductionFixture {
            session: pending,
            ..fixture
        };
        let recovered = production_event(
            &fixture,
            2,
            CommandAckStatus::Recovered,
            Some("FINAM_PRODUCTION_RECOVERED_0001"),
            Some(CommandAckReasonCode::RecoveredByBrokerTruth),
        );
        let resolved = apply_stage5g_mock_ack(fixture.session, recovered)
            .unwrap()
            .into_resolved()
            .unwrap();
        assert_eq!(resolved.ack_outcomes().len(), 1);
    }

    #[test]
    fn production_public_pre_callback_block_retains_linear_session() {
        let fixture = production_fixture(production_bar_close_ts());
        let mut wrong = production_event(
            &fixture,
            1,
            CommandAckStatus::Accepted,
            Some("FINAM_PRODUCTION_BLOCKED_0001"),
            None,
        );
        wrong.account_id = BrokerAccountId::new("ACC_TEST_WRONG");
        let failure = match apply_stage5g_mock_ack(fixture.session, wrong) {
            Err(failure) => failure,
            Ok(_) => panic!("wrong account must block before Stage 5C"),
        };
        let blocked = failure
            .into_blocked()
            .expect("pre-callback block retains session");
        assert_eq!(blocked.reason(), Stage5gMockAckError::AccountMismatch);
        assert_eq!(blocked.session().summary().resolved_count, 0);
        let _retained = blocked.into_session();
    }

    #[test]
    fn production_public_contradiction_blocks_and_duplicate_is_idempotent() {
        let fixture = production_fixture(production_bar_close_ts());
        let submitted = production_event(&fixture, 1, CommandAckStatus::Submitted, None, None);
        let pending = apply_stage5g_mock_ack(fixture.session, submitted)
            .unwrap()
            .into_awaiting()
            .unwrap();
        let fixture = ProductionFixture {
            session: pending,
            ..fixture
        };
        let proof = production_event(
            &fixture,
            2,
            CommandAckStatus::Expired,
            None,
            Some(CommandAckReasonCode::ExpiredCommand),
        );
        let failure = match apply_stage5g_mock_ack(fixture.session, proof) {
            Err(failure) => failure,
            Ok(_) => panic!("contradictory no-send proof must block"),
        };
        let blocked = failure.into_blocked().unwrap();
        assert_eq!(
            blocked.reason(),
            Stage5gMockAckError::NoSendProofContradictsPriorLifecycleEvidence
        );

        let fixture = production_fixture(production_bar_close_ts());
        let accepted = production_event(
            &fixture,
            1,
            CommandAckStatus::Accepted,
            Some("FINAM_PRODUCTION_DUPLICATE_0001"),
            None,
        );
        let duplicate = production_event(
            &fixture,
            2,
            CommandAckStatus::Duplicate,
            Some("FINAM_PRODUCTION_DUPLICATE_0001"),
            Some(CommandAckReasonCode::DuplicateCommand),
        );
        let resolved = apply_stage5g_mock_ack(fixture.session, accepted)
            .unwrap()
            .into_resolved()
            .unwrap();
        let resolved = apply_stage5g_duplicate_after_resolution(resolved, duplicate).unwrap();
        assert_eq!(resolved.ack_outcomes().len(), 1);
        assert_eq!(resolved.duplicate_status_count(), 1);
    }

    #[test]
    fn production_public_duplicate_time_changes_transition_fingerprint_without_callback_replay() {
        let bar_close_ts = production_bar_close_ts();
        let resolve_with_duplicate_at = |received_offset_seconds: i64| {
            let fixture = production_fixture(bar_close_ts);
            let accepted = production_event(
                &fixture,
                1,
                CommandAckStatus::Accepted,
                Some("FINAM_PRODUCTION_FP_DUPLICATE_0001"),
                None,
            );
            let mut duplicate = production_event(
                &fixture,
                2,
                CommandAckStatus::Duplicate,
                Some("FINAM_PRODUCTION_FP_DUPLICATE_0001"),
                Some(CommandAckReasonCode::DuplicateCommand),
            );
            duplicate.ack.received_ts = Utc
                .timestamp_opt(bar_close_ts + received_offset_seconds, 0)
                .single()
                .unwrap();
            let resolved = apply_stage5g_mock_ack(fixture.session, accepted)
                .unwrap()
                .into_resolved()
                .unwrap();
            let post_stage5c_fingerprint = resolved.post_lifecycle_state_fingerprint();
            let resolved = apply_stage5g_duplicate_after_resolution(resolved, duplicate).unwrap();
            assert_eq!(resolved.ack_outcomes().len(), 1);
            assert_eq!(resolved.duplicate_status_count(), 1);
            assert_eq!(
                resolved.post_lifecycle_state_fingerprint(),
                post_stage5c_fingerprint,
                "duplicate replay must not invoke Stage 5C again"
            );
            resolved
        };

        let earlier = resolve_with_duplicate_at(20);
        let later = resolve_with_duplicate_at(30);
        assert_ne!(
            earlier.transition_fingerprint_sha256(),
            later.transition_fingerprint_sha256(),
            "duplicate ACK receive time is continuation-relevant transition identity"
        );
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
        let resolved = apply_test_mock_ack(fixture.session, ack)
            .expect("accepted ACK")
            .into_resolved()
            .expect("single intent resolves");
        assert_eq!(
            resolved
                .canonical_ack()
                .broker_order_id
                .as_ref()
                .map(BrokerOrderId::as_str),
            Some("FINAM_ORDER_EXACT_STRING_0001")
        );
        let summary = stage5g_state_summary(&resolved.state);
        assert!(!summary.broker_truth_changed);
        assert!(!summary.redis_attached);
        assert!(!summary.finam_transport_attached);
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
        let resolved = apply_test_mock_ack(fixture.session, ack)
            .expect("submitted ACK with broker order id")
            .into_resolved()
            .expect("single intent resolves");
        assert_eq!(
            resolved
                .canonical_ack()
                .broker_order_id
                .as_ref()
                .map(BrokerOrderId::as_str),
            Some("FINAM_SUBMITTED_EXACT_STRING_0001")
        );
        assert!(!stage5g_state_summary(&resolved.state).broker_truth_changed);
    }

    #[test]
    fn gack02_and_gack03_missing_broker_id_waits_then_recovered_resolves() {
        let fixture = make_fixture();
        let submitted = event(&fixture, 1, CommandAckStatus::Submitted, None, None);
        let pending = apply_test_mock_ack(fixture.session, submitted)
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
        assert!(apply_test_mock_ack(recovered_fixture.session, recovered)
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
        let resolved = apply_test_mock_ack(fixture.session, rejected)
            .expect("rejected is callback-safe")
            .into_resolved()
            .expect("rejected clears exact pending request");
        assert_eq!(resolved.canonical_ack().status, CommandAckStatus::Rejected);
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
            let pending = apply_test_mock_ack(fixture.session, ambiguous)
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
        let duplicate_without_prior = event(
            &fixture,
            1,
            CommandAckStatus::Duplicate,
            None,
            Some(CommandAckReasonCode::DuplicateCommand),
        );
        let pending = apply_test_mock_ack(fixture.session, duplicate_without_prior)
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
        let resolved = apply_test_mock_ack(fixture.session, accepted)
            .unwrap()
            .into_resolved()
            .unwrap();
        let resolved = apply_test_duplicate_after_resolution(resolved, duplicate)
            .expect("exact duplicate status is idempotent");
        assert_eq!(resolved.duplicate_status_count(), 1);
    }

    #[test]
    fn duplicate_requires_exact_broker_identity_and_coherent_reason() {
        let resolve = |broker_order_id: Option<&str>, reason| {
            let fixture = make_fixture();
            let duplicate = event(
                &fixture,
                2,
                CommandAckStatus::Duplicate,
                broker_order_id,
                reason,
            );
            let accepted = event(
                &fixture,
                1,
                CommandAckStatus::Accepted,
                Some("FINAM_DUPLICATE_EXACT_0001"),
                None,
            );
            let resolved = apply_test_mock_ack(fixture.session, accepted)
                .unwrap()
                .into_resolved()
                .unwrap();
            (resolved, duplicate)
        };

        let (resolved, missing_id) = resolve(None, Some(CommandAckReasonCode::DuplicateCommand));
        assert_eq!(
            expect_replay_blocked(apply_test_duplicate_after_resolution(resolved, missing_id))
                .reason(),
            Stage5gMockAckError::DuplicateStatusIdentityMismatch
        );

        let (resolved, wrong_id) = resolve(
            Some("FINAM_DUPLICATE_OTHER_0002"),
            Some(CommandAckReasonCode::DuplicateCommand),
        );
        assert_eq!(
            expect_replay_blocked(apply_test_duplicate_after_resolution(resolved, wrong_id))
                .reason(),
            Stage5gMockAckError::BrokerOrderIdConflict
        );

        let (resolved, missing_reason) = resolve(Some("FINAM_DUPLICATE_EXACT_0001"), None);
        assert_eq!(
            expect_replay_blocked(apply_test_duplicate_after_resolution(
                resolved,
                missing_reason,
            ))
            .reason(),
            Stage5gMockAckError::AckReasonIncoherent
        );
    }

    #[test]
    fn resolved_duplicate_rejects_reversed_ack_time() {
        let fixture = make_fixture();
        let mut accepted = event(
            &fixture,
            1,
            CommandAckStatus::Accepted,
            Some("FINAM_DUP_TIME_0001"),
            None,
        );
        accepted.ack.received_ts = Utc
            .timestamp_opt(fixture.bar_close_ts + 10, 0)
            .single()
            .unwrap();
        let mut duplicate = event(
            &fixture,
            2,
            CommandAckStatus::Duplicate,
            Some("FINAM_DUP_TIME_0001"),
            Some(CommandAckReasonCode::DuplicateCommand),
        );
        duplicate.ack.received_ts = Utc
            .timestamp_opt(fixture.bar_close_ts + 9, 0)
            .single()
            .unwrap();
        let resolved = apply_test_mock_ack(fixture.session, accepted)
            .unwrap()
            .into_resolved()
            .unwrap();
        assert_eq!(
            expect_replay_blocked(apply_test_duplicate_after_resolution(resolved, duplicate))
                .reason(),
            Stage5gMockAckError::NonMonotonicAckTime
        );
    }

    #[test]
    fn duplicate_timestamp_changes_transition_fingerprint() {
        let (earlier, _) = resolve_test_duplicate_at(20);
        let (later, _) = resolve_test_duplicate_at(30);
        assert_ne!(
            earlier.transition_fingerprint_sha256(),
            later.transition_fingerprint_sha256()
        );
    }

    #[test]
    fn duplicate_timestamp_changes_continuation_semantics() {
        let (earlier, continuation_after_earlier) = resolve_test_duplicate_at(20);
        let (later, continuation_after_later) = resolve_test_duplicate_at(30);
        let continued = apply_test_duplicate_after_resolution(earlier, continuation_after_earlier)
            .expect("T+25 is valid after the T+20 watermark");
        assert_eq!(continued.duplicate_status_count(), 2);
        assert_eq!(
            expect_replay_blocked(apply_test_duplicate_after_resolution(
                later,
                continuation_after_later,
            ))
            .reason(),
            Stage5gMockAckError::NonMonotonicAckTime
        );
    }

    #[test]
    fn gack08_expired_requires_exact_no_send_proof() {
        let fixture = make_fixture();
        let expired_without_proof = event(&fixture, 1, CommandAckStatus::Expired, None, None);
        let pending = apply_test_mock_ack(fixture.session, expired_without_proof)
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
        assert!(apply_test_mock_ack(fixture.session, proof)
            .expect("exact no-send proof clears pending")
            .into_resolved()
            .is_some());
    }

    #[test]
    fn no_send_proof_cannot_follow_observed_broker_identity() {
        let fixture = make_fixture();
        let contradiction = event(
            &fixture,
            1,
            CommandAckStatus::Expired,
            Some("FINAM_EXPIRED_OBSERVED_0001"),
            None,
        );
        let blocked = expect_blocked(apply_test_mock_ack(fixture.session, contradiction));
        assert_eq!(
            blocked.reason(),
            Stage5gMockAckError::NoSendProofContradictsBrokerIdentity
        );
        assert_eq!(
            blocked.session().summary().slots[0].state,
            Stage5gMockAckSlotState::ManualInterventionRequired
        );
        assert!(blocked.session().summary().slots[0]
            .broker_order_id_domain_sha256
            .is_some());

        let fixture = Fixture {
            session: blocked.into_session(),
            ..fixture
        };
        let later_no_id_proof = event(
            &fixture,
            2,
            CommandAckStatus::Expired,
            None,
            Some(CommandAckReasonCode::ExpiredCommand),
        );
        assert_eq!(
            expect_blocked(apply_test_mock_ack(fixture.session, later_no_id_proof,)).reason(),
            Stage5gMockAckError::NoSendProofContradictsBrokerIdentity
        );

        let fixture = make_fixture();
        let timeout_with_identity = event(
            &fixture,
            1,
            CommandAckStatus::Timeout,
            Some("FINAM_TIMEOUT_OBSERVED_0001"),
            Some(CommandAckReasonCode::TimeoutUnknownPending),
        );
        let pending = apply_test_mock_ack(fixture.session, timeout_with_identity)
            .unwrap()
            .into_awaiting()
            .unwrap();
        let fixture = Fixture {
            session: pending,
            ..fixture
        };
        let proof_after_timeout = event(
            &fixture,
            2,
            CommandAckStatus::Expired,
            None,
            Some(CommandAckReasonCode::ExpiredCommand),
        );
        assert_eq!(
            expect_blocked(apply_test_mock_ack(fixture.session, proof_after_timeout,)).reason(),
            Stage5gMockAckError::NoSendProofContradictsBrokerIdentity
        );

        let fixture = make_fixture();
        let proof_with_identity = event(
            &fixture,
            1,
            CommandAckStatus::Expired,
            Some("FINAM_PROOF_CONTRADICTION_0001"),
            Some(CommandAckReasonCode::ExpiredCommand),
        );
        assert_eq!(
            expect_blocked(apply_test_mock_ack(fixture.session, proof_with_identity,)).reason(),
            Stage5gMockAckError::NoSendProofContradictsBrokerIdentity
        );
    }

    #[test]
    fn no_send_proof_requires_clean_waiting_or_unproved_expiry_provenance() {
        for (status, reason) in [
            (CommandAckStatus::Submitted, None),
            (CommandAckStatus::Accepted, None),
            (
                CommandAckStatus::Recovered,
                Some(CommandAckReasonCode::RecoveredByBrokerTruth),
            ),
            (
                CommandAckStatus::Timeout,
                Some(CommandAckReasonCode::TimeoutUnknownPending),
            ),
            (
                CommandAckStatus::UnknownPending,
                Some(CommandAckReasonCode::ReconciliationRequired),
            ),
            (
                CommandAckStatus::Error,
                Some(CommandAckReasonCode::ManualInterventionRequired),
            ),
        ] {
            let fixture = make_fixture();
            let prior = event(&fixture, 1, status, None, reason);
            let pending = apply_test_mock_ack(fixture.session, prior)
                .expect("prior lifecycle evidence is retained")
                .into_awaiting()
                .expect("prior lifecycle must remain pending");
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
            let blocked = expect_blocked(apply_test_mock_ack(fixture.session, proof));
            assert_eq!(
                blocked.reason(),
                Stage5gMockAckError::NoSendProofContradictsPriorLifecycleEvidence
            );
            assert_eq!(
                blocked.session().summary().slots[0].state,
                Stage5gMockAckSlotState::ManualInterventionRequired
            );
        }
    }

    #[test]
    fn observed_broker_identity_cannot_be_lost_by_terminal_ack() {
        for terminal in [
            CommandAckStatus::Accepted,
            CommandAckStatus::Recovered,
            CommandAckStatus::Rejected,
        ] {
            let fixture = make_fixture();
            let observed = event(
                &fixture,
                1,
                CommandAckStatus::Timeout,
                Some("FINAM_CONTINUITY_A_0001"),
                Some(CommandAckReasonCode::TimeoutUnknownPending),
            );
            let pending = apply_test_mock_ack(fixture.session, observed)
                .unwrap()
                .into_awaiting()
                .unwrap();
            let fixture = Fixture {
                session: pending,
                ..fixture
            };
            let reason = match terminal {
                CommandAckStatus::Recovered => Some(CommandAckReasonCode::RecoveredByBrokerTruth),
                CommandAckStatus::Rejected => Some(CommandAckReasonCode::BrokerRejected),
                _ => None,
            };
            let missing = event(&fixture, 2, terminal, None, reason);
            let blocked = expect_blocked(apply_test_mock_ack(fixture.session, missing));
            assert_eq!(
                blocked.reason(),
                Stage5gMockAckError::MissingBrokerOrderIdAfterObservedIdentity
            );
            assert!(blocked.session().summary().slots[0]
                .broker_order_id_domain_sha256
                .is_some());
        }
    }

    #[test]
    fn ack_time_watermark_is_non_decreasing_and_fingerprinted() {
        let fixture = make_fixture();
        let mut first = event(&fixture, 1, CommandAckStatus::Timeout, None, None);
        first.ack.received_ts = Utc
            .timestamp_opt(fixture.bar_close_ts + 10, 0)
            .single()
            .unwrap();
        let pending = apply_test_mock_ack(fixture.session, first)
            .unwrap()
            .into_awaiting()
            .unwrap();
        let before = pending.lifecycle_fingerprint_sha256();
        let fixture = Fixture {
            session: pending,
            ..fixture
        };
        let mut reversed = event(&fixture, 2, CommandAckStatus::UnknownPending, None, None);
        reversed.ack.received_ts = Utc
            .timestamp_opt(fixture.bar_close_ts + 9, 0)
            .single()
            .unwrap();
        assert_eq!(
            expect_blocked(apply_test_mock_ack(fixture.session, reversed)).reason(),
            Stage5gMockAckError::NonMonotonicAckTime
        );

        let fixture = make_fixture();
        let mut first = event(&fixture, 1, CommandAckStatus::Timeout, None, None);
        first.ack.received_ts = Utc
            .timestamp_opt(fixture.bar_close_ts + 10, 0)
            .single()
            .unwrap();
        let pending = apply_test_mock_ack(fixture.session, first)
            .unwrap()
            .into_awaiting()
            .unwrap();
        let fixture = Fixture {
            session: pending,
            ..fixture
        };
        let mut equal = event(&fixture, 2, CommandAckStatus::UnknownPending, None, None);
        equal.ack.received_ts = Utc
            .timestamp_opt(fixture.bar_close_ts + 10, 0)
            .single()
            .unwrap();
        let equal = apply_test_mock_ack(fixture.session, equal)
            .unwrap()
            .into_awaiting()
            .unwrap();
        assert_ne!(before, equal.lifecycle_fingerprint_sha256());
        assert_eq!(
            equal.summary().last_ack_received_ts_utc.as_deref(),
            Some("2026-01-06T06:10:10.000000000Z")
        );
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
        let pending = apply_test_mock_ack(fixture.session, error)
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
        let blocked = expect_blocked(apply_test_mock_ack(fixture.session, wrong_request));
        assert_eq!(blocked.reason(), Stage5gMockAckError::AckRequestIdMismatch);

        let fixture = Fixture {
            session: blocked.into_session(),
            ..fixture
        };
        let mut wrong_client = event(&fixture, 1, CommandAckStatus::Accepted, None, None);
        wrong_client.ack.client_order_id =
            Some(ClientOrderId::new("CID_WRONG_0000000001").expect("valid wrong client id"));
        assert_eq!(
            expect_blocked(apply_test_mock_ack(fixture.session, wrong_client)).reason(),
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
        let pending = apply_test_mock_ack(fixture.session, timeout)
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
            expect_blocked(apply_test_mock_ack(fixture.session, recovered)).reason(),
            Stage5gMockAckError::BrokerOrderIdConflict
        );
    }

    #[test]
    fn wrong_account_instrument_side_and_action_block_before_callback() {
        let fixture = make_fixture();
        let mut wrong_account = event(&fixture, 1, CommandAckStatus::Accepted, None, None);
        wrong_account.account_id = BrokerAccountId::new("ACC_TEST_0002");
        let blocked = expect_blocked(apply_test_mock_ack(fixture.session, wrong_account));
        assert_eq!(blocked.reason(), Stage5gMockAckError::AccountMismatch);

        let fixture = Fixture {
            session: blocked.into_session(),
            ..fixture
        };
        let mut wrong_instrument = event(&fixture, 1, CommandAckStatus::Accepted, None, None);
        wrong_instrument.instrument.symbol = "RTS-9.26".to_string();
        let blocked = expect_blocked(apply_test_mock_ack(fixture.session, wrong_instrument));
        assert_eq!(blocked.reason(), Stage5gMockAckError::InstrumentMismatch);

        let fixture = Fixture {
            session: blocked.into_session(),
            ..fixture
        };
        let mut wrong_side = event(&fixture, 1, CommandAckStatus::Accepted, None, None);
        wrong_side.side = Some(opposite(fixture.side));
        let blocked = expect_blocked(apply_test_mock_ack(fixture.session, wrong_side));
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
            expect_blocked(apply_test_mock_ack(fixture.session, wrong_action)).reason(),
            Stage5gMockAckError::ActionMismatch
        );
    }

    #[test]
    fn duplicate_ack_terminal_twice_and_expired_lifecycle_block() {
        let fixture = make_fixture();
        let first = event(&fixture, 1, CommandAckStatus::Timeout, None, None);
        let pending = apply_test_mock_ack(fixture.session, first)
            .unwrap()
            .into_awaiting()
            .unwrap();
        let fixture = Fixture {
            session: pending,
            ..fixture
        };
        let repeated_timeout = event(&fixture, 2, CommandAckStatus::Timeout, None, None);
        assert_eq!(
            expect_blocked(apply_test_mock_ack(fixture.session, repeated_timeout)).reason(),
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
        let resolved = apply_test_mock_ack(fixture.session, accepted)
            .unwrap()
            .into_resolved()
            .unwrap();
        assert_eq!(
            expect_replay_blocked(apply_test_duplicate_after_resolution(
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
            expect_blocked(apply_test_mock_ack(fixture.session, late)).reason(),
            Stage5gMockAckError::AckAfterLifecycleExpiry
        );
    }

    #[test]
    fn limit_and_cancel_cannot_spoof_a_market_source_request_identity() {
        for action in [
            Stage5gMockIntentAction::Place {
                place_kind: Stage5gMockPlaceKind::Limit,
            },
            Stage5gMockIntentAction::Cancel {
                target_order_id: BrokerOrderId::new("FINAM_CANCEL_TARGET_0001"),
            },
        ] {
            let (projection, mut binding, _) =
                accepted_stage5f_market_projection(ACCEPTED_STAGE5F_BAR_CLOSE_TS);
            if matches!(action, Stage5gMockIntentAction::Cancel { .. }) {
                binding.side = None;
            }
            binding.action = action;
            let blocked = match stage5g_build_mock_ack_state(
                projection,
                Stage5gMockAckSessionInput {
                    intent_bindings: vec![binding],
                    lifecycle_expires_at_ts_utc: ACCEPTED_STAGE5F_BAR_CLOSE_TS + 300,
                },
            ) {
                Err(reason) => reason,
                Ok(_) => {
                    panic!("Limit/Cancel must not reuse a Market source request identity")
                }
            };
            assert!(matches!(
                blocked,
                Stage5gMockAckAdmissionError::BindingRequestIdentityMismatch
                    | Stage5gMockAckAdmissionError::BindingActionClassMismatch
            ));
        }
    }

    #[test]
    fn lifecycle_fingerprint_is_deterministic_for_same_input() {
        let bar_close_ts = ACCEPTED_STAGE5F_BAR_CLOSE_TS;
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
        let left = apply_test_mock_ack(left.session, left_ack)
            .unwrap()
            .into_resolved()
            .unwrap();
        let right = apply_test_mock_ack(right.session, right_ack)
            .unwrap()
            .into_resolved()
            .unwrap();
        assert_eq!(
            left.transition_fingerprint_sha256(),
            right.transition_fingerprint_sha256()
        );
    }

    #[test]
    fn lifecycle_fingerprint_v4_binds_exact_redacted_ack_identity() {
        let left = resolve_single_ack(
            CommandAckStatus::Accepted,
            Some("FINAM_SAME_LEN_A_0001"),
            None,
            1,
            1,
        );
        let right = resolve_single_ack(
            CommandAckStatus::Accepted,
            Some("FINAM_SAME_LEN_B_0002"),
            None,
            1,
            1,
        );
        assert_eq!("FINAM_SAME_LEN_A_0001".len(), "FINAM_SAME_LEN_B_0002".len());
        assert_ne!(
            left.pre_callback_lifecycle_fingerprint_sha256(),
            right.pre_callback_lifecycle_fingerprint_sha256()
        );
        assert_ne!(
            left.transition_fingerprint_sha256(),
            right.transition_fingerprint_sha256()
        );
        let left_hash =
            stage5g_broker_order_id_domain_sha256(&BrokerOrderId::new("FINAM_SAME_LEN_A_0001"));
        assert!(!left_hash.contains("FINAM"));
        assert_eq!(left_hash.len(), 64);
    }

    #[test]
    fn lifecycle_fingerprint_v4_binds_reason_timestamp_and_sequence() {
        let broker_rejected = resolve_single_ack(
            CommandAckStatus::Rejected,
            None,
            Some(CommandAckReasonCode::BrokerRejected),
            1,
            1,
        );
        let locally_rejected = resolve_single_ack(
            CommandAckStatus::Rejected,
            None,
            Some(CommandAckReasonCode::LocalValidationRejected),
            1,
            1,
        );
        assert_ne!(
            broker_rejected.transition_fingerprint_sha256(),
            locally_rejected.transition_fingerprint_sha256()
        );

        let first_timestamp = resolve_single_ack(
            CommandAckStatus::Accepted,
            Some("FINAM_FP_TIME_0001"),
            None,
            1,
            1,
        );
        let second_timestamp = resolve_single_ack(
            CommandAckStatus::Accepted,
            Some("FINAM_FP_TIME_0001"),
            None,
            1,
            2,
        );
        assert_ne!(
            first_timestamp.transition_fingerprint_sha256(),
            second_timestamp.transition_fingerprint_sha256()
        );

        let first_sequence = resolve_single_ack(
            CommandAckStatus::Accepted,
            Some("FINAM_FP_SEQUENCE_0001"),
            None,
            1,
            1,
        );
        let second_sequence = resolve_single_ack(
            CommandAckStatus::Accepted,
            Some("FINAM_FP_SEQUENCE_0001"),
            None,
            2,
            1,
        );
        assert_ne!(
            first_sequence.transition_fingerprint_sha256(),
            second_sequence.transition_fingerprint_sha256()
        );
    }

    #[test]
    fn canonical_ack_vector_order_changes_transition_evidence() {
        let first = resolve_single_ack(
            CommandAckStatus::Accepted,
            Some("FINAM_VECTOR_FIRST_0001"),
            None,
            1,
            1,
        );
        let second = resolve_single_ack(
            CommandAckStatus::Accepted,
            Some("FINAM_VECTOR_SECOND_002"),
            None,
            1,
            1,
        );
        let first_projection = stage5g_canonical_ack_fingerprint_projection(&first.state.slots[0]);
        let second_projection =
            stage5g_canonical_ack_fingerprint_projection(&second.state.slots[0]);
        assert_ne!(
            stage5g_summary_fingerprint(
                &vec![first_projection.clone(), second_projection.clone(),]
            ),
            stage5g_summary_fingerprint(&vec![second_projection, first_projection])
        );
    }

    #[test]
    fn accepted_stage5f_market_fixture_evidence_hash_is_frozen() {
        let resolved = resolve_single_ack(
            CommandAckStatus::Accepted,
            Some("FINAM_STAGE5F_FIXED_0001"),
            None,
            1,
            1,
        );
        assert_eq!(
            resolved.transition_fingerprint_sha256(),
            "9e009c1c4e00809b94c3af7291f6aa4411dd67c65bd6a2bd1b5108d85256bf38"
        );
    }
}
