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
    instrument_identity_matches, BrokerAccountId, BrokerOrderId, BrokerOrderLifecycle,
    BrokerOrderSnapshot, BrokerPositionSnapshot, BrokerTradeId, BrokerTradeSnapshot,
    BrokerTruthSnapshot, ClientOrderId, HybridRuntimeAttribution, HybridRuntimeOrderEvent,
    HybridRuntimePositionEvent, InstrumentId, OrderSide, OrderStatus, OrderType, StrategyRequestId,
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

pub const STAGE5G_ORDER_POSITION_SCHEMA_VERSION: u16 = 4;
const STAGE5G_EVIDENCE_FINGERPRINT_SCHEMA_VERSION: u16 = 3;
const STAGE5G_BROKER_TRUTH_PACKAGE_IDENTITY_SCHEMA_VERSION: u16 = 1;
const STAGE5G_IMMUTABLE_TRADE_PAYLOAD_SCHEMA_VERSION: u16 = 1;
const STAGE5G_IMMUTABLE_TRADE_PAYLOAD_DOMAIN: &str = "moex.stage5g.immutable-trade-payload.v1";
const STAGE5G_IMMUTABLE_ORDER_PAYLOAD_SCHEMA_VERSION: u16 = 1;
const STAGE5G_IMMUTABLE_ORDER_PAYLOAD_DOMAIN: &str = "moex.stage5g.immutable-order-payload.v1";
const STAGE5G_CANONICAL_DECIMAL_SCHEMA_VERSION: u16 = 1;
const STAGE5G_CANONICAL_DECIMAL_DOMAIN: &str = "moex.stage5g.exact-decimal.v1";

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Stage5gEvidenceCanonicalizationError {
    TradeIdentityConflict,
    EvidenceIdentityGrammarViolation,
}

/// Versioned exact immutable projection for one broker trade. The only
/// deliberately omitted field is `received_ts`, which is an observation
/// receipt rather than broker-native trade identity. In particular,
/// `InstrumentId` is bound structurally; broad correlation helpers are not an
/// immutable-payload equality policy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct Stage5gCanonicalDecimalV1 {
    schema_version: u16,
    domain: &'static str,
    /// `Decimal::serialize()` binds flags (including sign and scale) plus the
    /// complete 96-bit mantissa. Numeric Decimal equality is deliberately not
    /// the immutable evidence policy.
    exact_bytes: [u8; 16],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct Stage5gCanonicalImmutableTradePayloadV1 {
    schema_version: u16,
    domain: &'static str,
    account_id: BrokerAccountId,
    broker_trade_id: BrokerTradeId,
    broker_order_id: Option<BrokerOrderId>,
    client_order_id: Option<ClientOrderId>,
    instrument: InstrumentId,
    side: OrderSide,
    qty: Stage5gCanonicalDecimalV1,
    price: Stage5gCanonicalDecimalV1,
    gross_amount: Option<Stage5gCanonicalDecimalV1>,
    commission: Option<Stage5gCanonicalDecimalV1>,
    broker_asset_id: Option<String>,
    board: Option<String>,
    expiration_date: Option<chrono::NaiveDate>,
    source_ts: DateTime<Utc>,
}

/// Exact immutable broker-order payload. Lifecycle/status/fill progress and
/// observation timestamps are deliberately excluded; every field below must
/// remain stable while one broker order advances.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct Stage5gCanonicalImmutableOrderPayloadV1 {
    schema_version: u16,
    domain: &'static str,
    account_id: BrokerAccountId,
    broker_order_id: Option<BrokerOrderId>,
    client_order_id: Option<ClientOrderId>,
    instrument: InstrumentId,
    side: OrderSide,
    order_type: OrderType,
    time_in_force: Option<broker_core::TimeInForce>,
    qty: Stage5gCanonicalDecimalV1,
    limit_price: Option<Stage5gCanonicalDecimalV1>,
    broker_asset_id: Option<String>,
    board: Option<String>,
    expiration_date: Option<chrono::NaiveDate>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stage5gImmutableTradeMergeError {
    IdentityConflict,
}

/// Source-owned canonical evidence. Construction is restricted to the single
/// pure canonicalization authority below, so active and restart paths cannot
/// fingerprint different projections of one broker package.
#[derive(Debug)]
pub(crate) struct Stage5gCanonicalOrderPositionEvidence {
    evidence: Stage5gOrderPositionEvidence,
    identity: String,
    fingerprint: String,
}

impl Stage5gCanonicalOrderPositionEvidence {
    pub(crate) fn evidence(&self) -> &Stage5gOrderPositionEvidence {
        &self.evidence
    }

    pub(crate) fn identity(&self) -> &str {
        &self.identity
    }

    pub(crate) fn fingerprint(&self) -> &str {
        &self.fingerprint
    }

    fn into_evidence(self) -> Stage5gOrderPositionEvidence {
        self.evidence
    }
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
    BrokerTruthBeforeContinuationCheckpoint,
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
    TargetTradeWithoutOrder,
    TradeIdentityConflict,
    TradeAccountMismatch,
    TradeInstrumentMismatch,
    TradeSideMismatch,
    TradeIdentityMismatch,
    NonPositiveTradeQuantity,
    TradeQuantityMismatch,
    EvidenceIdentityGrammarViolation,
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

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct CanonicalOrderEvent {
    total_sequence: u64,
    order: BrokerOrderSnapshot,
    attribution: Option<HybridRuntimeAttribution>,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct Stage5gOrderPositionSlot {
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct EvidenceIdentity {
    pub(crate) identity: String,
    pub(crate) fingerprint: String,
}

/// Exact broker-package replay projection handed to the accepted Stage 5G-d
/// continuation boundary. This is evidence only: it owns no strategy callback
/// or transport capability.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Stage5gReplayCheckpoint {
    pub(crate) schema_version: u16,
    pub(crate) package_discriminator: Option<String>,
    pub(crate) current_evidence_identity: Option<String>,
    pub(crate) evidence_identities: Vec<EvidenceIdentity>,
    pub(crate) last_broker_truth_received_at: Option<DateTime<Utc>>,
    pub(crate) last_broker_truth_received_ms: Option<i64>,
    pub(crate) duplicate_evidence_count: usize,
    pub(crate) last_total_sequence: Option<u64>,
    pub(crate) last_continuation_checkpoint_ts_utc_ms: Option<i64>,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct Stage5gOrderPositionState {
    strategy_id: String,
    account_id: broker_core::BrokerAccountId,
    instrument: InstrumentId,
    slots: Vec<Stage5gOrderPositionSlot>,
    evidence_identities: Vec<EvidenceIdentity>,
    current_evidence_identity: Option<String>,
    last_total_sequence: Option<u64>,
    last_broker_truth_received_at: Option<DateTime<Utc>>,
    last_broker_truth_received_ms: Option<i64>,
    duplicate_evidence_count: usize,
    last_continuation_checkpoint_ts_utc_ms: Option<i64>,
}

/// Minimal immutable view consumed by the Stage 5G-e-d-b reducer.  It is
/// intentionally produced by the module that owns `Stage5gOrderPositionState`
/// so the reducer never receives mutable state or broad field visibility.
#[derive(Clone, Serialize)]
pub(crate) struct Stage5gFreshTruthRestartSlotProjection {
    pub(crate) command_request_id: String,
    pub(crate) command_client_order_id: ClientOrderId,
    pub(crate) target_broker_order_id: Option<BrokerOrderId>,
    pub(crate) target_order_client_order_id: Option<ClientOrderId>,
    pub(crate) cancel_target_order_authority: Option<Stage5gCancelTargetOrderAuthority>,
    pub(crate) intent_class: Stage5gRestartIntentClass,
    pub(crate) source_action: Stage5gMockIntentAction,
    pub(crate) side: Option<OrderSide>,
    pub(crate) target_qty: Option<Decimal>,
    pub(crate) pre_position_qty: Decimal,
    pub(crate) source_numeric_authority_is_integral: bool,
    pub(crate) expected_attribution_fingerprint_sha256: Option<String>,
    pub(crate) latest_order: Option<BrokerOrderSnapshot>,
    pub(crate) trades: Vec<BrokerTradeSnapshot>,
    pub(crate) position: Option<BrokerPositionSnapshot>,
    pub(crate) terminal: bool,
}

/// Narrow, immutable authority for a Cancel target. The command identity is
/// intentionally absent. Optional client/payload authority exists only after
/// a separately owned target-order observation has been accepted.
#[derive(Clone, Serialize)]
pub(crate) struct Stage5gCancelTargetOrderAuthority {
    pub(crate) target_broker_order_id: BrokerOrderId,
    pub(crate) target_order_client_order_id: Option<ClientOrderId>,
    pub(crate) immutable_order_commitment_sha256: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Stage5gRestartIntentClass {
    Entry,
    Exit,
    ProtectiveRepair,
    CancelCleanup,
}

impl From<crate::BrokerNeutralHybridIntentClass> for Stage5gRestartIntentClass {
    fn from(value: crate::BrokerNeutralHybridIntentClass) -> Self {
        match value {
            crate::BrokerNeutralHybridIntentClass::Entry => Self::Entry,
            crate::BrokerNeutralHybridIntentClass::Exit => Self::Exit,
            crate::BrokerNeutralHybridIntentClass::ProtectiveRepair => Self::ProtectiveRepair,
            crate::BrokerNeutralHybridIntentClass::CancelCleanup => Self::CancelCleanup,
        }
    }
}

/// Shared exact correlation result used by both the accepted Stage 5G
/// order/position core and the restart reducer. Missing identifiers never
/// compare equal, and one matching identifier cannot hide a conflict in the
/// other supplied identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Stage5gTradeOrderLinkage {
    Exact,
    Unrelated,
    Conflict,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Stage5gAccountWideOrderSafety {
    Safe,
    NonOwnedActive,
    NonOwnedUnknown,
    AmbiguousOwned,
    ConflictingOwnedIdentity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Stage5gOrderOwnershipCorrelation {
    ExactOwned,
    ConflictingOwnedIdentity,
    UnrelatedTerminal,
    NonOwnedActive,
    NonOwnedUnknown,
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
    replay_checkpoint: Stage5gReplayCheckpoint,
}

/// A terminal MARKET outcome settled only through the accepted R3 authority.
/// The contained Stage 5C settlement remains opaque and transport-free.
pub struct Stage5gMarketTerminalConvergedPaperStrategy {
    settlement: Stage5cBrokerLifecycleSettlement,
    summary: Stage5gOrderPositionSummary,
    replay_checkpoint: Stage5gReplayCheckpoint,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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

    pub fn into_terminal(self) -> Option<Stage5gOrderPositionTerminal> {
        match self {
            Self::Blocked(_) => None,
            Self::Terminal(terminal) => Some(terminal),
        }
    }
}

pub fn attach_stage5g_order_position_session(
    ack_resolved: Stage5gResolvedMockAckPaperStrategy,
) -> Result<Stage5gOrderPositionSession, Box<Stage5gOrderPositionAdmissionBlocked>> {
    attach_stage5g_order_position_session_with_replay(ack_resolved, None)
}

pub(crate) fn attach_stage5g_order_position_session_with_replay(
    ack_resolved: Stage5gResolvedMockAckPaperStrategy,
    inherited_replay: Option<Stage5gReplayCheckpoint>,
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
    let inherited_replay = inherited_replay.unwrap_or(Stage5gReplayCheckpoint {
        schema_version: STAGE5G_BROKER_TRUTH_PACKAGE_IDENTITY_SCHEMA_VERSION,
        package_discriminator: None,
        current_evidence_identity: None,
        evidence_identities: Vec::new(),
        last_broker_truth_received_at: None,
        last_broker_truth_received_ms: None,
        duplicate_evidence_count: 0,
        last_total_sequence: None,
        last_continuation_checkpoint_ts_utc_ms: None,
    });
    Ok(Stage5gOrderPositionSession {
        ack_resolved,
        state: Stage5gOrderPositionState {
            strategy_id: summary.strategy_id,
            account_id: summary.account_id,
            instrument: summary.instrument,
            slots,
            evidence_identities: inherited_replay.evidence_identities,
            current_evidence_identity: inherited_replay.current_evidence_identity,
            last_total_sequence: inherited_replay.last_total_sequence,
            last_broker_truth_received_at: inherited_replay.last_broker_truth_received_at,
            last_broker_truth_received_ms: inherited_replay.last_broker_truth_received_ms,
            duplicate_evidence_count: inherited_replay.duplicate_evidence_count,
            last_continuation_checkpoint_ts_utc_ms: inherited_replay
                .last_continuation_checkpoint_ts_utc_ms,
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

/// The only Stage 5G evidence canonicalization authority. It is pure, owns its
/// input, and preserves the exact package receipt/account while normalizing
/// all vector-shaped truth before identity/fingerprint classification.
pub(crate) fn canonicalize_stage5g_order_position_evidence(
    mut evidence: Stage5gOrderPositionEvidence,
) -> Result<Stage5gCanonicalOrderPositionEvidence, Stage5gEvidenceCanonicalizationError> {
    let account_id = evidence.broker_truth.account_id.as_str();
    if account_id.is_empty() || account_id.contains(':') {
        return Err(Stage5gEvidenceCanonicalizationError::EvidenceIdentityGrammarViolation);
    }
    canonicalize_broker_truth_snapshot(&mut evidence.broker_truth)?;
    let identity = evidence_identity(&evidence);
    let fingerprint = canonical_evidence_fingerprint(&evidence);
    Ok(Stage5gCanonicalOrderPositionEvidence {
        evidence,
        identity,
        fingerprint,
    })
}

/// FINAM maps every historical trade with the current observation receipt, so
/// receipt is deliberately excluded from immutable trade identity while the
/// newest observation watermark is retained.
fn canonicalize_broker_truth_snapshot(
    truth: &mut BrokerTruthSnapshot,
) -> Result<(), Stage5gEvidenceCanonicalizationError> {
    let mut trades_by_id: BTreeMap<String, BrokerTradeSnapshot> = BTreeMap::new();
    for trade in truth.trades.drain(..) {
        let key = trade.broker_trade_id.as_str().to_string();
        match trades_by_id.get_mut(&key) {
            Some(existing) => merge_canonical_trade_observation_v1(existing, trade).map_err(
                |Stage5gImmutableTradeMergeError::IdentityConflict| {
                    Stage5gEvidenceCanonicalizationError::TradeIdentityConflict
                },
            )?,
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

fn canonical_decimal_v1(value: Decimal) -> Stage5gCanonicalDecimalV1 {
    Stage5gCanonicalDecimalV1 {
        schema_version: STAGE5G_CANONICAL_DECIMAL_SCHEMA_VERSION,
        domain: STAGE5G_CANONICAL_DECIMAL_DOMAIN,
        exact_bytes: value.serialize(),
    }
}

fn canonical_immutable_trade_payload_v1(
    trade: &BrokerTradeSnapshot,
) -> Stage5gCanonicalImmutableTradePayloadV1 {
    Stage5gCanonicalImmutableTradePayloadV1 {
        schema_version: STAGE5G_IMMUTABLE_TRADE_PAYLOAD_SCHEMA_VERSION,
        domain: STAGE5G_IMMUTABLE_TRADE_PAYLOAD_DOMAIN,
        account_id: trade.account_id.clone(),
        broker_trade_id: trade.broker_trade_id.clone(),
        broker_order_id: trade.broker_order_id.clone(),
        client_order_id: trade.client_order_id.clone(),
        instrument: trade.instrument.clone(),
        side: trade.side,
        qty: canonical_decimal_v1(trade.qty),
        price: canonical_decimal_v1(trade.price),
        gross_amount: trade.gross_amount.map(canonical_decimal_v1),
        commission: trade.commission.map(canonical_decimal_v1),
        broker_asset_id: trade.broker_asset_id.clone(),
        board: trade.board.clone(),
        expiration_date: trade.expiration_date,
        source_ts: trade.source_ts,
    }
}

fn canonical_immutable_order_payload_v1(
    order: &BrokerOrderSnapshot,
) -> Stage5gCanonicalImmutableOrderPayloadV1 {
    Stage5gCanonicalImmutableOrderPayloadV1 {
        schema_version: STAGE5G_IMMUTABLE_ORDER_PAYLOAD_SCHEMA_VERSION,
        domain: STAGE5G_IMMUTABLE_ORDER_PAYLOAD_DOMAIN,
        account_id: order.account_id.clone(),
        broker_order_id: order.broker_order_id.clone(),
        client_order_id: order.client_order_id.clone(),
        instrument: order.instrument.clone(),
        side: order.side,
        order_type: order.order_type,
        time_in_force: order.time_in_force,
        qty: canonical_decimal_v1(order.qty),
        limit_price: order.limit_price.map(canonical_decimal_v1),
        broker_asset_id: order.broker_asset_id.clone(),
        board: order.board.clone(),
        expiration_date: order.expiration_date,
    }
}

pub(crate) fn stage5g_immutable_order_payload_matches(
    left: &BrokerOrderSnapshot,
    right: &BrokerOrderSnapshot,
) -> bool {
    canonical_immutable_order_payload_v1(left) == canonical_immutable_order_payload_v1(right)
}

pub(crate) fn stage5g_immutable_order_payload_commitment_sha256(
    order: &BrokerOrderSnapshot,
) -> String {
    let bytes = serde_json::to_vec(&canonical_immutable_order_payload_v1(order))
        .expect("immutable broker-order payload serializes");
    format!("{:x}", Sha256::digest(bytes))
}

fn immutable_trade_payload_matches(
    left: &BrokerTradeSnapshot,
    right: &BrokerTradeSnapshot,
) -> bool {
    canonical_immutable_trade_payload_v1(left) == canonical_immutable_trade_payload_v1(right)
}

/// Exact duplicates retain the complete row with the greatest observation
/// receipt. Since all other fields are bound by the immutable projection,
/// choosing this representative is commutative and independent of vector
/// order. Equal receipts imply structurally equal complete rows.
fn merge_canonical_trade_observation_v1(
    existing: &mut BrokerTradeSnapshot,
    incoming: BrokerTradeSnapshot,
) -> Result<(), Stage5gImmutableTradeMergeError> {
    if !immutable_trade_payload_matches(existing, &incoming) {
        return Err(Stage5gImmutableTradeMergeError::IdentityConflict);
    }
    if incoming.received_ts > existing.received_ts {
        *existing = incoming;
    }
    Ok(())
}

pub fn apply_stage5g_order_position_evidence(
    session: Stage5gOrderPositionSession,
    evidence: Stage5gOrderPositionEvidence,
) -> Result<Stage5gOrderPositionTransition, Stage5gOrderPositionFailure> {
    let canonical_evidence = match canonicalize_stage5g_order_position_evidence(evidence) {
        Ok(canonical) => canonical,
        Err(Stage5gEvidenceCanonicalizationError::TradeIdentityConflict) => {
            return Err(block(
                Stage5gOrderPositionError::TradeIdentityConflict,
                session,
            ));
        }
        Err(Stage5gEvidenceCanonicalizationError::EvidenceIdentityGrammarViolation) => {
            return Err(block(
                Stage5gOrderPositionError::EvidenceIdentityGrammarViolation,
                session,
            ));
        }
    };
    apply_stage5g_canonical_order_position_evidence(session, canonical_evidence)
}

/// The single Stage 5G-c canonical application core. Stage 5G-e-b transfers
/// its owned candidate here so classifier and application use the exact same
/// identity/fingerprint authority without reconstructing raw evidence.
pub(crate) fn apply_stage5g_canonical_order_position_evidence(
    mut session: Stage5gOrderPositionSession,
    canonical_evidence: Stage5gCanonicalOrderPositionEvidence,
) -> Result<Stage5gOrderPositionTransition, Stage5gOrderPositionFailure> {
    let evidence = canonical_evidence.evidence();
    let identity = canonical_evidence.identity().to_string();
    let fingerprint = canonical_evidence.fingerprint().to_string();
    match apply_stage5g_exact_replay_metadata(&mut session, evidence, &identity, &fingerprint) {
        Err(reason) => return Err(block(reason, session)),
        Ok(Stage5gReplayAdmission::ExactReplay) => {
            return Ok(Stage5gOrderPositionTransition::Awaiting(session));
        }
        Ok(Stage5gReplayAdmission::NewPackage) => {}
    }
    let slot_index = match stage5g_order_position_new_package_preflight(&session, evidence) {
        Ok(slot_index) => slot_index,
        Err(reason) => return Err(block(reason, session)),
    };
    let evidence = canonical_evidence.into_evidence();
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
    let safety_slot = &session.state.slots[slot_index];
    let target_client_order_id = slot_target_order_client_id(safety_slot);
    let target_broker_order_id = slot_target_broker_order_id(safety_slot);
    match stage5g_account_wide_order_safety(
        &evidence.broker_truth.orders,
        target_client_order_id,
        target_broker_order_id,
    ) {
        Stage5gAccountWideOrderSafety::Safe => {}
        Stage5gAccountWideOrderSafety::NonOwnedActive => {
            return Err(block(
                Stage5gOrderPositionError::AccountWideActiveOrderSafetyGuard,
                session,
            ));
        }
        Stage5gAccountWideOrderSafety::NonOwnedUnknown => {
            return Err(block(
                Stage5gOrderPositionError::AccountWideUnknownOrderSafetyGuard,
                session,
            ));
        }
        Stage5gAccountWideOrderSafety::AmbiguousOwned => {
            return Err(block(
                Stage5gOrderPositionError::AmbiguousTargetOrder,
                session,
            ));
        }
        Stage5gAccountWideOrderSafety::ConflictingOwnedIdentity => {
            return Err(block(
                Stage5gOrderPositionError::ClientOrderIdMismatch,
                session,
            ));
        }
    }

    let current_slot = &session.state.slots[slot_index];
    let has_target_trade = has_target_correlated_trade(current_slot, &evidence.broker_truth);
    if current_slot.terminal && has_target_trade {
        return Err(block(
            Stage5gOrderPositionError::BrokerEvidenceAfterTerminalAck,
            session,
        ));
    }
    if matches!(
        current_slot.ack.action,
        Stage5gMockIntentAction::Place {
            place_kind: Stage5gMockPlaceKind::Market
        }
    ) && !has_target_correlated_order(current_slot, &evidence.broker_truth)
        && has_target_trade
    {
        // STAGE5G-C-R2CB-R2-POSITION-ONLY-TRADE-BLOCK-BEGIN
        // A position-only Market observation cannot authenticate trade rows.
        // Retry with the target order row before any chronology or ledger
        // state is advanced.
        return Err(block(
            Stage5gOrderPositionError::TargetTradeWithoutOrder,
            session,
        ));
        // STAGE5G-C-R2CB-R2-POSITION-ONLY-TRADE-BLOCK-END
    }

    // Evidence application is transactional: a blocked snapshot must not
    // partially append order/trade/position state before the caller retries
    // with corrected broker truth.
    let pre_candidate_state = session.state.clone();
    let mut next_slot = session.state.slots[slot_index].clone();
    if let Err(reason) = validate_snapshot_chronology(
        session.state.last_broker_truth_received_at,
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
    // STAGE5G-C-R2CB-R2-COMMITTED-TRADE-WATERMARK-BEGIN
    refresh_trade_watermarks_from_committed_ledger(&mut next_slot);
    if !component_watermarks_are_monotonic(&session.state.slots[slot_index], &next_slot) {
        return Err(block(
            Stage5gOrderPositionError::TradeTimeRegression,
            session,
        ));
    }
    // STAGE5G-C-R2CB-R2-COMMITTED-TRADE-WATERMARK-END
    session.state.slots[slot_index] = next_slot;
    session.state.last_total_sequence = Some(evidence.total_sequence);
    session.state.last_broker_truth_received_at = Some(evidence.broker_truth.received_ts);
    session.state.last_broker_truth_received_ms =
        Some(evidence.broker_truth.received_ts.timestamp_millis());
    session.state.last_continuation_checkpoint_ts_utc_ms = Some(
        session
            .state
            .last_continuation_checkpoint_ts_utc_ms
            .unwrap_or(i64::MIN)
            .max(evidence.broker_truth.received_ts.timestamp_millis()),
    );
    session.state.evidence_identities.push(EvidenceIdentity {
        identity: identity.clone(),
        fingerprint,
    });
    session.state.current_evidence_identity = Some(identity);

    if !session.state.slots.iter().all(|slot| slot.terminal) {
        return Ok(Stage5gOrderPositionTransition::Awaiting(session));
    }
    converge_through_stage5c(session, pre_candidate_state)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stage5gReplayAdmission {
    ExactReplay,
    NewPackage,
}

/// The single metadata authority shared by raw and owned canonical evidence.
/// Exact replay classification intentionally precedes every NewPackage-only
/// chronology, slot and broker-state check. A successful exact replay mutates
/// only its local sequence and duplicate counter.
fn apply_stage5g_exact_replay_metadata(
    session: &mut Stage5gOrderPositionSession,
    evidence: &Stage5gOrderPositionEvidence,
    identity: &str,
    fingerprint: &str,
) -> Result<Stage5gReplayAdmission, Stage5gOrderPositionError> {
    if session
        .state
        .last_total_sequence
        .is_some_and(|last| evidence.total_sequence <= last)
    {
        return Err(Stage5gOrderPositionError::NonMonotonicSequence);
    }
    if evidence.broker_truth.account_id != session.state.account_id {
        return Err(Stage5gOrderPositionError::AccountMismatch);
    }
    match classify_evidence_replay(&session.state, identity, fingerprint)? {
        true => {
            session.state.last_total_sequence = Some(evidence.total_sequence);
            session.state.duplicate_evidence_count += 1;
            Ok(Stage5gReplayAdmission::ExactReplay)
        }
        false => Ok(Stage5gReplayAdmission::NewPackage),
    }
}

fn stage5g_order_position_new_package_preflight(
    session: &Stage5gOrderPositionSession,
    evidence: &Stage5gOrderPositionEvidence,
) -> Result<usize, Stage5gOrderPositionError> {
    if session
        .state
        .last_continuation_checkpoint_ts_utc_ms
        .is_some_and(|checkpoint| evidence.broker_truth.received_ts.timestamp_millis() < checkpoint)
    {
        return Err(Stage5gOrderPositionError::BrokerTruthBeforeContinuationCheckpoint);
    }
    let slot_index = session
        .state
        .slots
        .iter()
        .position(|slot| slot.ack.request_id == evidence.request_id)
        .ok_or(Stage5gOrderPositionError::UnknownRequestId)?;
    Ok(slot_index)
}

pub(crate) fn stage5g_order_position_session_replay(
    session: &Stage5gOrderPositionSession,
) -> Stage5gReplayCheckpoint {
    replay_checkpoint(&session.state)
}

pub(crate) fn stage5g_converged_replay(
    converged: &Stage5gConvergedPaperStrategy,
) -> Stage5gReplayCheckpoint {
    converged.replay_checkpoint.clone()
}

pub(crate) fn stage5g_market_terminal_replay(
    converged: &Stage5gMarketTerminalConvergedPaperStrategy,
) -> Stage5gReplayCheckpoint {
    converged.replay_checkpoint.clone()
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
    last_broker_truth_received_at: Option<DateTime<Utc>>,
    instrument: &InstrumentId,
    slot: &mut Stage5gOrderPositionSlot,
    evidence: &Stage5gOrderPositionEvidence,
) -> Result<(), Stage5gOrderPositionError> {
    let snapshot_ts = evidence.broker_truth.received_ts;
    if last_broker_truth_received_at.is_some_and(|last| snapshot_ts < last) {
        return Err(Stage5gOrderPositionError::BrokerTruthTimeRegression);
    }
    let target_order_id = slot_target_broker_order_id(slot).cloned();
    let target_client_order_id = slot_target_order_client_id(slot).cloned();
    let correlated_orders: Vec<_> = evidence
        .broker_truth
        .orders
        .iter()
        .filter(|order| {
            order.account_id == evidence.broker_truth.account_id
                && instrument_identity_matches(&order.instrument, instrument)
                && (order.broker_order_id.as_ref() == target_order_id.as_ref()
                    || target_client_order_id
                        .as_ref()
                        .is_some_and(|expected| order.client_order_id.as_ref() == Some(expected)))
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
                && (trade.broker_order_id.as_ref() == target_order_id.as_ref()
                    || target_client_order_id
                        .as_ref()
                        .is_some_and(|expected| trade.client_order_id.as_ref() == Some(expected)))
        })
        .collect();
    for trade in &correlated_trades {
        validate_component_time(Some(trade.source_ts), trade.received_ts, snapshot_ts)?;
        // STAGE5G-C-R2CB-R1-KNOWN-TRADE-CHRONOLOGY-BEGIN
        if let Some(known) = slot
            .trades
            .iter()
            .find(|known| known.broker_trade_id == trade.broker_trade_id)
        {
            // A FINAM full snapshot repeats historical trades with the fresh
            // package observation receipt. Known immutable history is checked
            // against its own committed observation, not against the newest
            // source timestamp of a different trade.
            if !immutable_trade_payload_matches(known, trade) {
                return Err(Stage5gOrderPositionError::TradeIdentityConflict);
            }
            if trade.received_ts < known.received_ts {
                return Err(Stage5gOrderPositionError::TradeTimeRegression);
            }
            continue;
        }
        // STAGE5G-C-R2CB-R1-KNOWN-TRADE-CHRONOLOGY-END
        // A previously unseen late trade remains fail closed. Admitting an
        // older source identity needs a separately reviewed reorder policy.
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
    Ok(())
}

fn refresh_trade_watermarks_from_committed_ledger(slot: &mut Stage5gOrderPositionSlot) {
    let committed_source_max = slot.trades.iter().map(|trade| trade.source_ts).max();
    let committed_received_max = slot.trades.iter().map(|trade| trade.received_ts).max();
    slot.last_trade_source_ts = [slot.last_trade_source_ts, committed_source_max]
        .into_iter()
        .flatten()
        .max();
    slot.last_trade_received_ts = [slot.last_trade_received_ts, committed_received_max]
        .into_iter()
        .flatten()
        .max();
}

fn component_watermarks_are_monotonic(
    before: &Stage5gOrderPositionSlot,
    after: &Stage5gOrderPositionSlot,
) -> bool {
    fn monotonic(before: Option<DateTime<Utc>>, after: Option<DateTime<Utc>>) -> bool {
        before.map_or(true, |before| after.is_some_and(|after| after >= before))
    }
    monotonic(before.last_order_source_ts, after.last_order_source_ts)
        && monotonic(before.last_order_received_ts, after.last_order_received_ts)
        && monotonic(before.last_trade_source_ts, after.last_trade_source_ts)
        && monotonic(before.last_trade_received_ts, after.last_trade_received_ts)
        && monotonic(
            before.last_position_source_ts,
            after.last_position_source_ts,
        )
        && monotonic(
            before.last_position_received_ts,
            after.last_position_received_ts,
        )
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
        let has_target_order = has_target_correlated_order(slot, &evidence.broker_truth);
        let has_target_trade = has_target_correlated_trade(slot, &evidence.broker_truth);
        let target_position =
            canonical_target_position(account_id, instrument, &evidence.broker_truth)?;
        let contradictory_target_position =
            decimal_f64_differs(target_position.snapshot.qty, slot.source.pre_position_qty);
        if has_target_order || has_target_trade || contradictory_target_position {
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

fn slot_target_broker_order_id(slot: &Stage5gOrderPositionSlot) -> Option<&BrokerOrderId> {
    match &slot.ack.action {
        Stage5gMockIntentAction::Place { .. } => slot.broker_order_id.as_ref(),
        Stage5gMockIntentAction::Cancel { target_order_id } => Some(target_order_id),
    }
}

fn slot_authenticated_target_order(
    slot: &Stage5gOrderPositionSlot,
) -> Option<&BrokerOrderSnapshot> {
    let target_broker_order_id = slot_target_broker_order_id(slot)?;
    slot.order_events.iter().rev().find_map(|event| {
        (event.order.broker_order_id.as_ref() == Some(target_broker_order_id))
            .then_some(&event.order)
    })
}

fn slot_target_order_client_id(slot: &Stage5gOrderPositionSlot) -> Option<&ClientOrderId> {
    match &slot.ack.action {
        Stage5gMockIntentAction::Place { .. } => Some(&slot.ack.expected_client_order_id),
        Stage5gMockIntentAction::Cancel { .. } => slot_authenticated_target_order(slot)
            .and_then(|order| order.client_order_id.as_ref())
            .filter(|client_order_id| *client_order_id != &slot.ack.expected_client_order_id),
    }
}

fn select_target_order(
    slot: &Stage5gOrderPositionSlot,
    truth: &BrokerTruthSnapshot,
) -> Result<BrokerOrderSnapshot, Stage5gOrderPositionError> {
    let expected =
        slot_target_broker_order_id(slot).ok_or(Stage5gOrderPositionError::MissingTargetOrder)?;
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
    let target_broker_order_id = slot_target_broker_order_id(slot);
    let target_client_order_id = slot_target_order_client_id(slot);
    let matches: Vec<_> = truth
        .orders
        .iter()
        .filter(|order| {
            target_broker_order_id
                .is_some_and(|expected| order.broker_order_id.as_ref() == Some(expected))
                || target_client_order_id
                    .is_some_and(|expected| order.client_order_id.as_ref() == Some(expected))
        })
        .cloned()
        .collect();
    match matches.len() {
        0 => Ok(None),
        1 => Ok(matches.into_iter().next()),
        _ => Err(Stage5gOrderPositionError::AmbiguousTargetOrder),
    }
}

fn has_target_correlated_order(
    slot: &Stage5gOrderPositionSlot,
    truth: &BrokerTruthSnapshot,
) -> bool {
    let target_broker_order_id = slot_target_broker_order_id(slot);
    let target_client_order_id = slot_target_order_client_id(slot);
    truth.orders.iter().any(|order| {
        target_broker_order_id
            .is_some_and(|expected| order.broker_order_id.as_ref() == Some(expected))
            || target_client_order_id
                .is_some_and(|expected| order.client_order_id.as_ref() == Some(expected))
    })
}

fn has_target_correlated_trade(
    slot: &Stage5gOrderPositionSlot,
    truth: &BrokerTruthSnapshot,
) -> bool {
    let target_broker_order_id = slot_target_broker_order_id(slot);
    let target_client_order_id = slot_target_order_client_id(slot);
    truth.trades.iter().any(|trade| {
        target_broker_order_id
            .is_some_and(|expected| trade.broker_order_id.as_ref() == Some(expected))
            || target_client_order_id
                .is_some_and(|expected| trade.client_order_id.as_ref() == Some(expected))
    })
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
    if order.broker_order_id.as_ref() != slot_target_broker_order_id(slot) {
        return Err(Stage5gOrderPositionError::BrokerOrderIdMismatch);
    }
    match &slot.ack.action {
        Stage5gMockIntentAction::Place { .. } => {
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
            if !stage5g_order_matches_source_action(&slot.ack.action, order) {
                return Err(Stage5gOrderPositionError::OrderTypeMismatch);
            }
        }
        Stage5gMockIntentAction::Cancel { .. } => {
            if slot_target_order_client_id(slot)
                .is_some_and(|expected| order.client_order_id.as_ref() != Some(expected))
            {
                return Err(Stage5gOrderPositionError::ClientOrderIdMismatch);
            }
        }
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
    if matches!(slot.ack.action, Stage5gMockIntentAction::Place { .. }) {
        let source_qty = slot
            .source
            .target_qty
            .and_then(Decimal::from_f64_retain)
            .ok_or(Stage5gOrderPositionError::SourceOrderMismatch)?;
        if order.qty != source_qty || order.filled_qty > source_qty {
            return Err(Stage5gOrderPositionError::SourceOrderMismatch);
        }
    }
    if let Some(previous) = slot.order_events.last().map(|event| &event.order) {
        if !stage5g_immutable_order_payload_matches(previous, order) {
            return Err(Stage5gOrderPositionError::SourceOrderMismatch);
        }
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
        match stage5g_exact_trade_order_linkage(order, trade) {
            Stage5gTradeOrderLinkage::Exact => {}
            Stage5gTradeOrderLinkage::Unrelated => continue,
            Stage5gTradeOrderLinkage::Conflict => {
                return Err(Stage5gOrderPositionError::TradeIdentityMismatch);
            }
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
            Some(previous) => merge_canonical_trade_observation_v1(previous, trade.clone())
                .map_err(|Stage5gImmutableTradeMergeError::IdentityConflict| {
                    Stage5gOrderPositionError::TradeIdentityConflict
                })?,
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
            merge_canonical_trade_observation_v1(previous, incoming).map_err(
                |Stage5gImmutableTradeMergeError::IdentityConflict| {
                    Stage5gOrderPositionError::TradeIdentityConflict
                },
            )?;
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

pub(crate) fn stage5g_exact_trade_order_linkage(
    order: &BrokerOrderSnapshot,
    trade: &BrokerTradeSnapshot,
) -> Stage5gTradeOrderLinkage {
    let client_match = trade.client_order_id.is_some()
        && order.client_order_id.is_some()
        && trade.client_order_id == order.client_order_id;
    let broker_match = trade.broker_order_id.is_some()
        && order.broker_order_id.is_some()
        && trade.broker_order_id == order.broker_order_id;
    let client_conflict =
        trade
            .client_order_id
            .as_ref()
            .is_some_and(|actual| match order.client_order_id.as_ref() {
                Some(expected) => actual != expected,
                None => true,
            });
    let broker_conflict =
        trade
            .broker_order_id
            .as_ref()
            .is_some_and(|actual| match order.broker_order_id.as_ref() {
                Some(expected) => actual != expected,
                None => true,
            });
    if !client_match && !broker_match {
        Stage5gTradeOrderLinkage::Unrelated
    } else if client_conflict || broker_conflict {
        Stage5gTradeOrderLinkage::Conflict
    } else {
        Stage5gTradeOrderLinkage::Exact
    }
}

pub(crate) fn stage5g_order_ownership_correlation(
    order: &BrokerOrderSnapshot,
    target_client_order_id: Option<&ClientOrderId>,
    target_broker_order_id: Option<&BrokerOrderId>,
) -> Stage5gOrderOwnershipCorrelation {
    let client_match = target_client_order_id
        .is_some_and(|expected| order.client_order_id.as_ref() == Some(expected));
    let broker_match = target_broker_order_id
        .is_some_and(|expected| order.broker_order_id.as_ref() == Some(expected));
    let client_conflict = target_client_order_id.is_some_and(|expected| {
        order
            .client_order_id
            .as_ref()
            .is_some_and(|actual| actual != expected)
    });
    let broker_conflict = target_broker_order_id.is_some_and(|expected| {
        order
            .broker_order_id
            .as_ref()
            .is_some_and(|actual| actual != expected)
    });

    if client_match || broker_match {
        if client_conflict || broker_conflict {
            Stage5gOrderOwnershipCorrelation::ConflictingOwnedIdentity
        } else {
            Stage5gOrderOwnershipCorrelation::ExactOwned
        }
    } else if order.lifecycle == BrokerOrderLifecycle::Unknown {
        Stage5gOrderOwnershipCorrelation::NonOwnedUnknown
    } else if order.is_active_for_lifecycle() {
        Stage5gOrderOwnershipCorrelation::NonOwnedActive
    } else {
        Stage5gOrderOwnershipCorrelation::UnrelatedTerminal
    }
}

pub(crate) fn stage5g_order_matches_source_action(
    action: &Stage5gMockIntentAction,
    order: &BrokerOrderSnapshot,
) -> bool {
    match action {
        Stage5gMockIntentAction::Place {
            place_kind: Stage5gMockPlaceKind::Market,
        } => order.order_type == OrderType::Market && order.limit_price.is_none(),
        Stage5gMockIntentAction::Place {
            place_kind: Stage5gMockPlaceKind::Limit,
        } => {
            order.order_type == OrderType::Limit
                && order.limit_price.is_some_and(|price| price > Decimal::ZERO)
        }
        Stage5gMockIntentAction::Cancel { target_order_id } => {
            order.broker_order_id.as_ref() == Some(target_order_id)
        }
    }
}

pub(crate) fn stage5g_immutable_trade_payload_matches(
    left: &BrokerTradeSnapshot,
    right: &BrokerTradeSnapshot,
) -> bool {
    immutable_trade_payload_matches(left, right)
}

/// Stage 5G-e-d-b accepts only integral MOEX lot quantities until the source
/// runtime model is migrated from `f64` to canonical Decimal authority.
pub(crate) fn stage5g_integral_lot_decimal(value: f64) -> Option<Decimal> {
    const MAX_EXACT_F64_INTEGER: f64 = 9_007_199_254_740_992.0;
    (value.is_finite() && value.fract() == 0.0 && value.abs() <= MAX_EXACT_F64_INTEGER)
        .then(|| Decimal::from_f64_retain(value))
        .flatten()
}

pub(crate) fn stage5g_expected_post_position_qty(
    pre_position_qty: Decimal,
    order: &BrokerOrderSnapshot,
) -> Decimal {
    let signed_fill = match order.side {
        OrderSide::Buy => order.filled_qty,
        OrderSide::Sell => -order.filled_qty,
    };
    pre_position_qty + signed_fill
}

pub(crate) fn stage5g_intent_position_is_compatible(
    intent_class: Stage5gRestartIntentClass,
    pre_position_qty: Decimal,
    expected_post_position_qty: Decimal,
    order: &BrokerOrderSnapshot,
) -> bool {
    match intent_class {
        Stage5gRestartIntentClass::Entry => {
            let direction_matches = match order.side {
                OrderSide::Buy => expected_post_position_qty > Decimal::ZERO,
                OrderSide::Sell => expected_post_position_qty < Decimal::ZERO,
            };
            direction_matches
                && expected_post_position_qty.abs() >= pre_position_qty.abs()
                && order.filled_qty <= order.qty
        }
        Stage5gRestartIntentClass::Exit => {
            expected_post_position_qty == Decimal::ZERO
                || pre_position_qty != Decimal::ZERO
                    && ((expected_post_position_qty > Decimal::ZERO
                        && pre_position_qty > Decimal::ZERO)
                        || (expected_post_position_qty < Decimal::ZERO
                            && pre_position_qty < Decimal::ZERO))
                    && expected_post_position_qty.abs() < pre_position_qty.abs()
        }
        Stage5gRestartIntentClass::ProtectiveRepair => expected_post_position_qty == Decimal::ZERO,
        Stage5gRestartIntentClass::CancelCleanup => {
            order.filled_qty == Decimal::ZERO && expected_post_position_qty == pre_position_qty
        }
    }
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

pub(crate) fn stage5g_account_wide_order_safety(
    orders: &[BrokerOrderSnapshot],
    target_client_order_id: Option<&ClientOrderId>,
    target_broker_order_id: Option<&BrokerOrderId>,
) -> Stage5gAccountWideOrderSafety {
    let correlations = orders
        .iter()
        .map(|order| {
            stage5g_order_ownership_correlation(
                order,
                target_client_order_id,
                target_broker_order_id,
            )
        })
        .collect::<Vec<_>>();
    if correlations
        .iter()
        .filter(|correlation| **correlation == Stage5gOrderOwnershipCorrelation::ExactOwned)
        .count()
        > 1
    {
        return Stage5gAccountWideOrderSafety::AmbiguousOwned;
    }
    if correlations.iter().any(|correlation| {
        *correlation == Stage5gOrderOwnershipCorrelation::ConflictingOwnedIdentity
    }) {
        return Stage5gAccountWideOrderSafety::ConflictingOwnedIdentity;
    }
    if correlations.contains(&Stage5gOrderOwnershipCorrelation::NonOwnedUnknown) {
        return Stage5gAccountWideOrderSafety::NonOwnedUnknown;
    }
    if correlations.contains(&Stage5gOrderOwnershipCorrelation::NonOwnedActive) {
        return Stage5gAccountWideOrderSafety::NonOwnedActive;
    }
    Stage5gAccountWideOrderSafety::Safe
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
    let replay_checkpoint = replay_checkpoint(&session.state);
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
                Stage5gConvergedPaperStrategy {
                    resolved,
                    summary,
                    replay_checkpoint,
                },
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
    let replay_checkpoint = replay_checkpoint(&session.state);
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
                replay_checkpoint,
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

// STAGE5G-C-REPLAY-PACKAGE-IDENTITY-BEGIN
fn broker_truth_received_at_discriminator(received_at: DateTime<Utc>) -> String {
    format!(
        "moex.broker-truth.package.v{}:{}:{:09}",
        STAGE5G_BROKER_TRUTH_PACKAGE_IDENTITY_SCHEMA_VERSION,
        received_at.timestamp(),
        received_at.timestamp_subsec_nanos(),
    )
}

fn broker_truth_package_discriminator(truth: &BrokerTruthSnapshot) -> String {
    // The full-precision receipt belongs to snapshot assembly authority. It is
    // stable across restart and distinguishes packages within one millisecond
    // without using a strategy-controlled lifecycle sequence or payload hash.
    broker_truth_received_at_discriminator(truth.received_ts)
}

fn evidence_identity(evidence: &Stage5gOrderPositionEvidence) -> String {
    format!(
        "moex.stage5g.order-position-evidence-identity.v3:{}:{}:{}",
        evidence.request_id,
        evidence.broker_truth.account_id,
        broker_truth_package_discriminator(&evidence.broker_truth),
    )
}
// STAGE5G-C-REPLAY-PACKAGE-IDENTITY-END

fn canonical_evidence_fingerprint(evidence: &Stage5gOrderPositionEvidence) -> String {
    let projection = serde_json::json!({
        "schema_version": STAGE5G_EVIDENCE_FINGERPRINT_SCHEMA_VERSION,
        "domain": "moex.stage5g.order-position-evidence.v3",
        "request_id": evidence.request_id,
        "broker_truth": &evidence.broker_truth,
        "receipt_watermark_ms": evidence.broker_truth.received_ts.timestamp_millis(),
        "package_discriminator": broker_truth_package_discriminator(&evidence.broker_truth),
        "attribution": evidence
            .order_attribution
            .as_ref()
            .map(HybridRuntimeAttribution::internal_comment),
    });
    let mut hasher = Sha256::new();
    hasher.update(b"moex.stage5g.order-position-evidence.v3\0");
    hasher
        .update(serde_json::to_vec(&projection).expect("canonical Stage 5G-c evidence serializes"));
    format!("{:x}", hasher.finalize())
}

fn replay_checkpoint(state: &Stage5gOrderPositionState) -> Stage5gReplayCheckpoint {
    Stage5gReplayCheckpoint {
        schema_version: STAGE5G_BROKER_TRUTH_PACKAGE_IDENTITY_SCHEMA_VERSION,
        package_discriminator: state
            .last_broker_truth_received_at
            .map(broker_truth_received_at_discriminator),
        current_evidence_identity: state.current_evidence_identity.clone(),
        evidence_identities: state.evidence_identities.clone(),
        last_broker_truth_received_at: state.last_broker_truth_received_at,
        last_broker_truth_received_ms: state.last_broker_truth_received_ms,
        duplicate_evidence_count: state.duplicate_evidence_count,
        last_total_sequence: state.last_total_sequence,
        last_continuation_checkpoint_ts_utc_ms: state.last_continuation_checkpoint_ts_utc_ms,
    }
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
        "last_broker_truth_package_discriminator": state
            .last_broker_truth_received_at
            .map(broker_truth_received_at_discriminator),
        "last_broker_truth_received_ms": state.last_broker_truth_received_ms,
        "last_continuation_checkpoint_ts_utc_ms": state.last_continuation_checkpoint_ts_utc_ms,
        "duplicate_evidence_count": state.duplicate_evidence_count,
        "stage5c_callback_count": callback_count,
    });
    let mut hasher = Sha256::new();
    hasher.update(b"moex.stage5g.order-position-lifecycle.v4\0");
    hasher.update(serde_json::to_vec(&projection).expect("Stage 5G-c v4 state serializes"));
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
    pub(crate) fn stage5g_restart_state_binding(
        state: &Stage5gOrderPositionState,
    ) -> (&str, &broker_core::BrokerAccountId, &InstrumentId) {
        (&state.strategy_id, &state.account_id, &state.instrument)
    }

    pub(crate) fn stage5g_runtime_strategy(&self) -> &crate::HybridIntradayRuntimeStrategy {
        self.ack_resolved.stage5g_runtime_strategy()
    }

    pub(crate) fn stage5g_restart_state(&self) -> Stage5gOrderPositionState {
        self.state.clone()
    }

    pub(crate) fn stage5g_restart_binding(
        &self,
    ) -> (&str, &broker_core::BrokerAccountId, &InstrumentId) {
        (
            &self.state.strategy_id,
            &self.state.account_id,
            &self.state.instrument,
        )
    }

    pub(crate) fn stage5g_restart_checkpoint(&self) -> crate::Stage5gTimerCheckpointEnvelope {
        Self::stage5g_restart_checkpoint_from_state(&self.state)
    }

    pub(crate) fn stage5g_restart_summary_from_state(
        state: &Stage5gOrderPositionState,
        authoritative_callback_count: usize,
    ) -> Stage5gOrderPositionSummary {
        state_summary(state, authoritative_callback_count)
    }

    pub(crate) fn stage5g_restart_checkpoint_from_state(
        state: &Stage5gOrderPositionState,
    ) -> crate::Stage5gTimerCheckpointEnvelope {
        crate::stage5g_timer::checkpoint_envelope(
            &replay_checkpoint(state),
            state.last_continuation_checkpoint_ts_utc_ms,
        )
    }

    pub(crate) fn stage5g_restart_projection_is_coherent(
        state: &Stage5gOrderPositionState,
        summary: &Stage5gOrderPositionSummary,
        checkpoint: &crate::Stage5gTimerCheckpointEnvelope,
        authoritative_callback_count: usize,
    ) -> bool {
        let expected_summary =
            Self::stage5g_restart_summary_from_state(state, authoritative_callback_count);
        let expected_checkpoint = Self::stage5g_restart_checkpoint_from_state(state);
        expected_summary == *summary && expected_checkpoint == *checkpoint
    }

    pub(crate) fn stage5g_fresh_truth_restart_slots(
        state: &Stage5gOrderPositionState,
    ) -> Vec<Stage5gFreshTruthRestartSlotProjection> {
        state
            .slots
            .iter()
            .map(|slot| {
                let (
                    target_broker_order_id,
                    target_order_client_order_id,
                    cancel_target_order_authority,
                    latest_order,
                ) = match &slot.ack.action {
                    Stage5gMockIntentAction::Place { .. } => (
                        slot.broker_order_id.clone(),
                        Some(slot.ack.expected_client_order_id.clone()),
                        None,
                        slot.order_events.last().map(|event| event.order.clone()),
                    ),
                    Stage5gMockIntentAction::Cancel { target_order_id } => {
                        let target_order = slot_authenticated_target_order(slot).cloned();
                        let target_client_order_id = target_order
                            .as_ref()
                            .and_then(|order| order.client_order_id.clone())
                            .filter(|client_order_id| {
                                client_order_id != &slot.ack.expected_client_order_id
                            });
                        let authority = Stage5gCancelTargetOrderAuthority {
                            target_broker_order_id: target_order_id.clone(),
                            target_order_client_order_id: target_client_order_id.clone(),
                            immutable_order_commitment_sha256: target_order
                                .as_ref()
                                .map(stage5g_immutable_order_payload_commitment_sha256),
                        };
                        (
                            Some(target_order_id.clone()),
                            target_client_order_id,
                            Some(authority),
                            target_order,
                        )
                    }
                };
                Stage5gFreshTruthRestartSlotProjection {
                    command_request_id: slot.ack.request_id.to_string(),
                    command_client_order_id: slot.ack.expected_client_order_id.clone(),
                    target_broker_order_id,
                    target_order_client_order_id,
                    cancel_target_order_authority,
                    intent_class: slot.source.intent_class.into(),
                    source_action: slot.ack.action.clone(),
                    side: slot.source.side.map(|side| match side {
                        crate::BrokerNeutralOrderSide::Buy => OrderSide::Buy,
                        crate::BrokerNeutralOrderSide::Sell => OrderSide::Sell,
                    }),
                    target_qty: slot
                        .source
                        .target_qty
                        .and_then(stage5g_integral_lot_decimal),
                    pre_position_qty: stage5g_integral_lot_decimal(slot.source.pre_position_qty)
                        .unwrap_or(Decimal::ZERO),
                    source_numeric_authority_is_integral: stage5g_integral_lot_decimal(
                        slot.source.pre_position_qty,
                    )
                    .is_some()
                        && slot
                            .source
                            .target_qty
                            .map_or(true, |qty| stage5g_integral_lot_decimal(qty).is_some()),
                    expected_attribution_fingerprint_sha256: slot
                        .source
                        .expected_attribution
                        .as_ref()
                        .map(|value| exact_id_hash("attribution", value.internal_comment())),
                    latest_order,
                    trades: slot.trades.clone(),
                    position: slot.position.as_ref().map(|(_, position)| position.clone()),
                    terminal: slot.terminal,
                }
            })
            .collect()
    }

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
    pub(crate) fn into_stage5g_d_parts(
        self,
    ) -> (
        Stage5cBrokerLifecycleResolvedPaperStrategy,
        Stage5gOrderPositionSummary,
        Stage5gReplayCheckpoint,
    ) {
        (self.resolved, self.summary, self.replay_checkpoint)
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
    pub(crate) fn into_stage5g_d_parts(
        self,
    ) -> (
        Stage5cBrokerLifecycleSettlement,
        Stage5gOrderPositionSummary,
        Stage5gReplayCheckpoint,
    ) {
        (self.settlement, self.summary, self.replay_checkpoint)
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
pub(crate) mod tests {
    use broker_core::command::CommandAck;
    use broker_core::{
        BrokerAccountId, BrokerOrderId, BrokerTradeId, ClientOrderId, Exchange, Market, TimeInForce,
    };
    use chrono::{Duration, NaiveTime, TimeZone, Timelike, Utc};

    use super::*;
    use crate::hybrid_intraday::{
        BreakoutEodMode, HybridOrchestratorConfig, IntradayBreakoutConfig, MeanReversionConfig,
        MinRangeMode, Owner, Side,
    };
    use crate::hybrid_intraday_runtime::{
        HybridIntradayProfile, HybridIntradayRuntimeConfig, HybridIntradayRuntimeStrategy,
        MeanReversionVariant, MrGatePolicy, RiskGateMode,
    };
    use crate::runtime_compat::{
        BarEvent, DataOrigin, GatewayPhase, MarketBuyAndCloseLiveOrderStyle, PaperExecutionMode,
        Strategy, StrategyCtx, TradeMode,
    };
    use crate::state::StrategyState;

    fn target() -> InstrumentId {
        InstrumentId {
            symbol: "IMOEXF".to_string(),
            venue_symbol: Some("IMOEXF@RTSX".to_string()),
            exchange: Exchange::Moex,
            market: Market::Futures,
        }
    }

    fn evidence_fingerprint(evidence: &Stage5gOrderPositionEvidence) -> String {
        canonicalize_stage5g_order_position_evidence(evidence.clone())
            .expect("test evidence canonicalizes")
            .fingerprint()
            .to_string()
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
            current_evidence_identity: Some(evidence_identity(event)),
            last_total_sequence: Some(event.total_sequence),
            last_broker_truth_received_at: Some(event.broker_truth.received_ts),
            last_broker_truth_received_ms: Some(event.broker_truth.received_ts.timestamp_millis()),
            duplicate_evidence_count: 0,
            last_continuation_checkpoint_ts_utc_ms: Some(
                event.broker_truth.received_ts.timestamp_millis(),
            ),
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

    fn r2cb_public_runtime_strategy(bar_close_ts: i64) -> HybridIntradayRuntimeStrategy {
        r2cb_public_runtime_strategy_with_riskgate(bar_close_ts, RiskGateMode::Disabled)
    }

    fn r2cb_public_runtime_strategy_with_riskgate(
        bar_close_ts: i64,
        risk_gate_mode: RiskGateMode,
    ) -> HybridIntradayRuntimeStrategy {
        let utc_bar_close = Utc.timestamp_opt(bar_close_ts, 0).single().unwrap();
        let timezone_offset_hours = 9 - i32::try_from(utc_bar_close.hour()).unwrap();
        let local_bar_close = utc_bar_close + Duration::hours(i64::from(timezone_offset_hours));
        HybridIntradayRuntimeStrategy::new(HybridIntradayRuntimeConfig {
            symbol: "IMOEXF".to_string(),
            profile: if risk_gate_mode == RiskGateMode::Disabled {
                HybridIntradayProfile::BaselineRuntimeHybrid
            } else {
                HybridIntradayProfile::ImoexfPrimaryRiskgateHigh180Lb120
            },
            mr_variant: if risk_gate_mode == RiskGateMode::Disabled {
                MeanReversionVariant::Author41BoundaryShort
            } else {
                MeanReversionVariant::High180
            },
            mr_gate_policy: if risk_gate_mode == RiskGateMode::Disabled {
                MrGatePolicy::Disabled
            } else {
                MrGatePolicy::ShadowPnlLb120Positive
            },
            risk_gate_mode,
            risk_gate_seed_file: None,
            risk_gate_ledger_key: None,
            model_session_start_time: Some((local_bar_close - Duration::minutes(10)).time()),
            model_session_end_time: Some((local_bar_close + Duration::hours(1)).time()),
            qty: 1.0,
            live_order_style: MarketBuyAndCloseLiveOrderStyle::Market,
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
                k: 0.53,
                stop1_range: 0.51,
                stop2_range: 0.35,
                big_move_threshold: 0.025,
                min_range: 1.01,
                min_range_mode: MinRangeMode::Absolute,
                exclude_weekends: false,
                wait_hours: 0.0,
            },
            orchestrator_config: HybridOrchestratorConfig {
                breakout_eod_mode: BreakoutEodMode::SameDay,
                breakout_overnight_exit_time: NaiveTime::from_hms_opt(9, 30, 0)
                    .expect("accepted overnight exit time"),
            },
        })
    }

    fn r2cb_public_runtime_session() -> (
        Stage5gOrderPositionSession,
        StrategyRequestId,
        ClientOrderId,
        Option<HybridRuntimeAttribution>,
        Duration,
    ) {
        let bar_close_ts = Utc::now().timestamp().div_euclid(600) * 600 - 600;
        r2cb_public_runtime_session_at(bar_close_ts)
    }

    fn r2cb_public_runtime_session_at(
        bar_close_ts: i64,
    ) -> (
        Stage5gOrderPositionSession,
        StrategyRequestId,
        ClientOrderId,
        Option<HybridRuntimeAttribution>,
        Duration,
    ) {
        r2cb_public_runtime_session_at_with_strategy(
            bar_close_ts,
            r2cb_public_runtime_strategy(bar_close_ts),
        )
    }

    fn r2cb_public_runtime_session_at_with_strategy(
        bar_close_ts: i64,
        strategy: HybridIntradayRuntimeStrategy,
    ) -> (
        Stage5gOrderPositionSession,
        StrategyRequestId,
        ClientOrderId,
        Option<HybridRuntimeAttribution>,
        Duration,
    ) {
        let (session, request_id, client_order_id, attribution, shift, ()) =
            r2cb_public_runtime_session_at_with_strategy_prepared(bar_close_ts, strategy, |_| ());
        (session, request_id, client_order_id, attribution, shift)
    }

    fn r2cb_public_runtime_session_at_with_strategy_prepared<T>(
        bar_close_ts: i64,
        mut strategy: HybridIntradayRuntimeStrategy,
        prepare: impl FnOnce(&mut HybridIntradayRuntimeStrategy) -> T,
    ) -> (
        Stage5gOrderPositionSession,
        StrategyRequestId,
        ClientOrderId,
        Option<HybridRuntimeAttribution>,
        Duration,
        T,
    ) {
        let fixture_poll1_whole_second = 1_785_661_800;
        let golden_time_shift = Duration::seconds(bar_close_ts + 10 - fixture_poll1_whole_second);
        for (close_time_utc, high, low) in [
            (bar_close_ts - 86_400 - 600, 2630.0, 2570.0),
            (bar_close_ts - 86_400, 2620.0, 2580.0),
        ] {
            assert!(Strategy::on_bar(
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
            )
            .is_empty());
        }
        let prepared = prepare(&mut strategy);
        let signal = broker_core::HybridRuntimeBarEvent {
            instrument: target(),
            close_time_utc: bar_close_ts,
            open: 2719.0,
            high: 2721.0,
            low: 2719.0,
            close: 2720.0,
            volume: 10.0,
            origin: broker_core::HybridRuntimeBarOrigin::Live,
            is_final: true,
            timeframe_sec: 600,
        };
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
                signal,
            );
        let semantic = crate::stage5c_paper_host::stage5g_test_apply_stage5c_semantic_bar_at(
            recovered,
            accepted,
            Utc.timestamp_opt(bar_close_ts + 1, 0).single().unwrap(),
        )
        .expect("accepted Stage 5F semantic Market intent");
        let settled = crate::settle_stage5c_semantic_result(semantic)
            .expect("accepted Stage 5F Market intent settlement");
        let request_id = settled.intent_batch().request_ids()[0];
        let source = settled.stage5g_source_intent_projections();
        assert_eq!(source.len(), 1);
        assert_eq!(source[0].base_action, Stage5gSourceBaseAction::Market);
        let side = source[0].side.expect("Market source side");
        let binding = crate::Stage5gMockIntentBinding {
            request_id,
            intent_class: settled.intent_batch().intent_classes()[0],
            action: Stage5gMockIntentAction::Place {
                place_kind: Stage5gMockPlaceKind::Market,
            },
            side: Some(side),
        };
        let ack_session = crate::attach_stage5g_mock_ack_session(
            settled,
            crate::Stage5gMockAckSessionInput {
                intent_bindings: vec![binding],
                lifecycle_expires_at_ts_utc: bar_close_ts + 300,
            },
        )
        .expect("public Stage 5G-b ACK attachment");
        let client_order_id = ClientOrderId::from_strategy_request(request_id);
        let resolved = crate::apply_stage5g_mock_ack(
            ack_session,
            crate::Stage5gMockAckEvent {
                total_sequence: 1,
                intent_request_id: request_id,
                account_id: BrokerAccountId::new("ACC_TEST_0001"),
                instrument: target(),
                action: Stage5gMockIntentAction::Place {
                    place_kind: Stage5gMockPlaceKind::Market,
                },
                side: Some(side),
                ack: CommandAck {
                    request_id,
                    client_order_id: Some(client_order_id.clone()),
                    broker_order_id: Some(BrokerOrderId::new("FINAM-R2CB-ORDER-1")),
                    status: CommandAckStatus::Accepted,
                    reason: None,
                    received_ts: Utc.timestamp_opt(bar_close_ts + 1, 0).single().unwrap(),
                },
            },
        )
        .expect("public accepted Stage 5G-b ACK")
        .into_resolved()
        .expect("accepted ACK resolves one-slot lifecycle");
        let expected_attribution = resolved.source_intent_projections()[0]
            .expected_attribution
            .clone();
        let session = attach_stage5g_order_position_session(resolved)
            .expect("public Stage 5G-c broker-truth attachment");
        (
            session,
            request_id,
            client_order_id,
            expected_attribution,
            golden_time_shift,
            prepared,
        )
    }

    fn r2cb_golden_truth(
        poll: &serde_json::Value,
        client_order_id: &ClientOrderId,
        time_shift: Duration,
    ) -> BrokerTruthSnapshot {
        let parse_ts = |field: &str| {
            chrono::DateTime::parse_from_rfc3339(
                poll[field].as_str().expect("golden timestamp string"),
            )
            .expect("golden timestamp parses")
            .with_timezone(&Utc)
                + time_shift
        };
        let received_ts = parse_ts("received_ts");
        let status = match poll["order_status"].as_str().unwrap() {
            "partially_filled" => OrderStatus::PartiallyFilled,
            "filled" => OrderStatus::Filled,
            other => panic!("unsupported golden order status: {other}"),
        };
        let filled_qty = poll["filled_qty"]
            .as_str()
            .unwrap()
            .parse::<Decimal>()
            .unwrap();
        let order = BrokerOrderSnapshot {
            account_id: BrokerAccountId::new("ACC_TEST_0001"),
            broker_order_id: Some(BrokerOrderId::new("FINAM-R2CB-ORDER-1")),
            client_order_id: Some(client_order_id.clone()),
            instrument: target(),
            side: OrderSide::Buy,
            order_type: OrderType::Market,
            time_in_force: Some(TimeInForce::Day),
            lifecycle: BrokerOrderSnapshot::lifecycle_for(&status),
            status,
            qty: Decimal::ONE,
            filled_qty,
            remaining_qty: Some(Decimal::ONE - filled_qty),
            limit_price: None,
            broker_asset_id: None,
            board: None,
            expiration_date: None,
            source_ts: Some(parse_ts("order_source_ts")),
            received_ts,
        };
        let positions = poll["position_rows"]
            .as_array()
            .unwrap()
            .iter()
            .map(|qty| BrokerPositionSnapshot {
                account_id: BrokerAccountId::new("ACC_TEST_0001"),
                instrument: target(),
                qty: qty.as_str().unwrap().parse::<Decimal>().unwrap(),
                avg_price: Some(Decimal::new(2_210, 0)),
                unrealized_pnl: Some(Decimal::ZERO),
                source_ts: None,
                received_ts,
            })
            .collect();
        let trades = poll["trades"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| BrokerTradeSnapshot {
                account_id: BrokerAccountId::new("ACC_TEST_0001"),
                broker_trade_id: BrokerTradeId::new(row["broker_trade_id"].as_str().unwrap()),
                broker_order_id: Some(BrokerOrderId::new("FINAM-R2CB-ORDER-1")),
                client_order_id: Some(client_order_id.clone()),
                instrument: target(),
                side: OrderSide::Buy,
                qty: row["qty"].as_str().unwrap().parse::<Decimal>().unwrap(),
                price: row["price"].as_str().unwrap().parse::<Decimal>().unwrap(),
                gross_amount: None,
                commission: None,
                broker_asset_id: None,
                board: None,
                expiration_date: None,
                source_ts: chrono::DateTime::parse_from_rfc3339(row["source_ts"].as_str().unwrap())
                    .unwrap()
                    .with_timezone(&Utc)
                    + time_shift,
                received_ts,
            })
            .collect();
        BrokerTruthSnapshot {
            account_id: BrokerAccountId::new("ACC_TEST_0001"),
            orders: vec![order],
            positions,
            cash: None,
            trades,
            instruments: vec![],
            received_ts,
        }
    }

    fn r2cb_public_converged_for_timer() -> Stage5gConvergedPaperStrategy {
        let golden: serde_json::Value = serde_json::from_str(include_str!(
            "../../../fixtures/expected/stage5g_r2cb_three_poll_broker_truth.json"
        ))
        .expect("connector-neutral three-poll golden JSON");
        let polls = golden["polls"].as_array().expect("three golden polls");
        let (mut session, request_id, client_order_id, expected_attribution, time_shift) =
            r2cb_public_runtime_session();
        for (index, poll) in polls.iter().enumerate() {
            let transition = apply_stage5g_order_position_evidence(
                session,
                Stage5gOrderPositionEvidence {
                    total_sequence: u64::try_from(index + 2).unwrap(),
                    request_id,
                    broker_truth: r2cb_golden_truth(poll, &client_order_id, time_shift),
                    order_attribution: expected_attribution.clone(),
                },
            )
            .expect("accepted full-snapshot timer fixture");
            if index + 1 == polls.len() {
                return transition
                    .into_converged()
                    .expect("terminal poll converges before timer attachment");
            }
            session = transition
                .into_awaiting()
                .expect("partial poll remains awaiting");
        }
        unreachable!("three-poll timer fixture has a terminal poll")
    }

    fn stage5ge_b_committed_checkpoint(
        session: &Stage5gOrderPositionSession,
    ) -> crate::Stage5gTimerCheckpointEnvelope {
        let replay = stage5g_order_position_session_replay(session);
        crate::stage5g_timer::checkpoint_envelope(
            &replay,
            replay.last_continuation_checkpoint_ts_utc_ms,
        )
    }

    fn stage5ge_b_candidate(
        checkpoint: &crate::Stage5gTimerCheckpointEnvelope,
        evidence: Stage5gOrderPositionEvidence,
    ) -> crate::Stage5gNewPackageCandidate {
        crate::classify_stage5g_post_checkpoint_evidence(checkpoint, evidence)
            .expect("fresh package classifies")
            .into_new_package()
            .expect("fresh package owns a candidate")
    }

    fn stage5ge_b_after_first_poll() -> (
        Stage5gOrderPositionSession,
        StrategyRequestId,
        ClientOrderId,
        Option<HybridRuntimeAttribution>,
        Duration,
        Vec<serde_json::Value>,
    ) {
        let golden: serde_json::Value = serde_json::from_str(include_str!(
            "../../../fixtures/expected/stage5g_r2cb_three_poll_broker_truth.json"
        ))
        .expect("connector-neutral three-poll golden JSON");
        let polls = golden["polls"].as_array().unwrap().clone();
        let (session, request_id, client_order_id, attribution, time_shift) =
            r2cb_public_runtime_session();
        let session = apply_stage5g_order_position_evidence(
            session,
            Stage5gOrderPositionEvidence {
                total_sequence: 2,
                request_id,
                broker_truth: r2cb_golden_truth(&polls[0], &client_order_id, time_shift),
                order_attribution: attribution.clone(),
            },
        )
        .expect("first partial poll is accepted")
        .into_awaiting()
        .expect("first partial poll remains awaiting");
        (
            session,
            request_id,
            client_order_id,
            attribution,
            time_shift,
            polls,
        )
    }

    fn stage5ge_b_r1_exact_replay(
        checkpoint: &crate::Stage5gTimerCheckpointEnvelope,
        sequence: u64,
        request_id: StrategyRequestId,
        client_order_id: &ClientOrderId,
        attribution: Option<HybridRuntimeAttribution>,
        time_shift: Duration,
        first_poll: &serde_json::Value,
    ) -> crate::Stage5gExactReplayCheckpoint {
        crate::classify_stage5g_post_checkpoint_evidence(
            checkpoint,
            Stage5gOrderPositionEvidence {
                total_sequence: sequence,
                request_id,
                broker_truth: r2cb_golden_truth(first_poll, client_order_id, time_shift),
                order_attribution: attribution,
            },
        )
        .expect("exact package redelivery classifies")
        .into_exact_replay()
        .expect("known canonical package produces exact-replay proof")
    }

    fn stage5ge_b_r2_apply_awaiting_package(
        session: Stage5gOrderPositionSession,
        sequence: u64,
        request_id: StrategyRequestId,
        client_order_id: &ClientOrderId,
        attribution: Option<HybridRuntimeAttribution>,
        time_shift: Duration,
        poll: &serde_json::Value,
    ) -> (
        Stage5gOrderPositionSession,
        crate::Stage5gTimerCheckpointEnvelope,
    ) {
        let checkpoint = stage5ge_b_committed_checkpoint(&session);
        let candidate = stage5ge_b_candidate(
            &checkpoint,
            Stage5gOrderPositionEvidence {
                total_sequence: sequence,
                request_id,
                broker_truth: r2cb_golden_truth(poll, client_order_id, time_shift),
                order_attribution: attribution,
            },
        );
        crate::apply_stage5g_new_package_candidate(session, candidate)
            .expect("partial NewPackage applies")
            .into_awaiting()
            .expect("partial package remains awaiting")
            .into_parts()
    }

    #[test]
    fn stage5ge_b_r1_exact_replay_synchronizes_session_then_new_package_commits() {
        let (session, request_id, client_order_id, attribution, time_shift, polls) =
            stage5ge_b_after_first_poll();
        let before_summary = session.summary();
        let before = stage5ge_b_committed_checkpoint(&session);
        let exact = stage5ge_b_r1_exact_replay(
            &before,
            3,
            request_id,
            &client_order_id,
            attribution.clone(),
            time_shift,
            &polls[0],
        );
        assert_eq!(exact.pre_replay_checkpoint(), &before);
        let classifier_commit = exact.checkpoint().clone();
        assert_eq!(classifier_commit.payload.last_total_sequence, Some(3));
        assert_eq!(classifier_commit.payload.duplicate_evidence_count, 1);

        let synchronized = crate::apply_stage5g_exact_replay_to_session(session, exact)
            .expect("exact proof synchronizes the live Stage 5G-c session");
        assert_eq!(synchronized.checkpoint(), &classifier_commit);
        let after_exact = synchronized.session().summary();
        assert_eq!(after_exact.last_total_sequence, Some(3));
        assert_eq!(after_exact.duplicate_evidence_count, 1);
        assert_eq!(
            after_exact.terminal_request_count,
            before_summary.terminal_request_count
        );
        assert_eq!(
            after_exact.order_transition_count,
            before_summary.order_transition_count
        );
        assert_eq!(
            after_exact.correlated_trade_count,
            before_summary.correlated_trade_count
        );
        assert_eq!(
            after_exact.position_confirmation_count,
            before_summary.position_confirmation_count
        );
        assert_eq!(
            after_exact.stage5c_callback_count,
            before_summary.stage5c_callback_count
        );
        assert_eq!(
            synchronized.checkpoint().payload.evidence_replay_ledger,
            before.payload.evidence_replay_ledger
        );
        assert_eq!(
            synchronized.checkpoint().payload.current_evidence_identity,
            before.payload.current_evidence_identity
        );
        assert_eq!(
            synchronized
                .checkpoint()
                .payload
                .last_broker_truth_received_at,
            before.payload.last_broker_truth_received_at
        );
        assert_eq!(
            synchronized
                .checkpoint()
                .payload
                .last_continuation_checkpoint_ts_utc_ms,
            before.payload.last_continuation_checkpoint_ts_utc_ms
        );

        let (session, exact_checkpoint) = synchronized.into_parts();
        let candidate = stage5ge_b_candidate(
            &exact_checkpoint,
            Stage5gOrderPositionEvidence {
                total_sequence: 4,
                request_id,
                broker_truth: r2cb_golden_truth(&polls[1], &client_order_id, time_shift),
                order_attribution: attribution,
            },
        );
        let committed = crate::apply_stage5g_new_package_candidate(session, candidate)
            .expect("next NewPackage applies to synchronized session");
        assert_eq!(committed.checkpoint().payload.last_total_sequence, Some(4));
        assert_eq!(committed.checkpoint().payload.duplicate_evidence_count, 1);
        assert_eq!(
            committed.checkpoint().payload.evidence_replay_ledger.len(),
            before.payload.evidence_replay_ledger.len() + 1
        );
    }

    #[test]
    fn stage5ge_b_r1_two_exact_replays_then_new_package_form_one_linear_chain() {
        let (mut session, request_id, client_order_id, attribution, time_shift, polls) =
            stage5ge_b_after_first_poll();
        let mut checkpoint = stage5ge_b_committed_checkpoint(&session);
        let baseline = session.summary();
        for sequence in [3, 4] {
            let proof = stage5ge_b_r1_exact_replay(
                &checkpoint,
                sequence,
                request_id,
                &client_order_id,
                attribution.clone(),
                time_shift,
                &polls[0],
            );
            let synchronized = crate::apply_stage5g_exact_replay_to_session(session, proof)
                .expect("each exact replay synchronizes once");
            (session, checkpoint) = synchronized.into_parts();
        }
        assert_eq!(checkpoint.payload.last_total_sequence, Some(4));
        assert_eq!(checkpoint.payload.duplicate_evidence_count, 2);
        assert_eq!(
            session.summary().order_transition_count,
            baseline.order_transition_count
        );
        assert_eq!(
            session.summary().correlated_trade_count,
            baseline.correlated_trade_count
        );
        assert_eq!(
            session.summary().stage5c_callback_count,
            baseline.stage5c_callback_count
        );

        let candidate = stage5ge_b_candidate(
            &checkpoint,
            Stage5gOrderPositionEvidence {
                total_sequence: 5,
                request_id,
                broker_truth: r2cb_golden_truth(&polls[1], &client_order_id, time_shift),
                order_attribution: attribution,
            },
        );
        let committed = crate::apply_stage5g_new_package_candidate(session, candidate)
            .expect("new package follows two exact replays");
        assert_eq!(committed.checkpoint().payload.last_total_sequence, Some(5));
        assert_eq!(committed.checkpoint().payload.duplicate_evidence_count, 2);
    }

    #[test]
    fn stage5ge_b_r1_stale_session_blocks_before_exact_replay_application() {
        let (session, request_id, client_order_id, attribution, time_shift, polls) =
            stage5ge_b_after_first_poll();
        let before = stage5ge_b_committed_checkpoint(&session);
        let proof = stage5ge_b_r1_exact_replay(
            &before,
            3,
            request_id,
            &client_order_id,
            attribution,
            time_shift,
            &polls[0],
        );
        let (stale_session, ..) = r2cb_public_runtime_session();
        let blocked = match crate::apply_stage5g_exact_replay_to_session(stale_session, proof) {
            Err(failure) => failure.into_blocked().expect("stale session is retryable"),
            Ok(_) => panic!("stale session must not accept exact-replay delta"),
        };
        assert_eq!(
            blocked.reason(),
            crate::Stage5gExactReplayApplyBlockReason::SessionCheckpointMismatch
        );
        assert_eq!(blocked.pre_replay_checkpoint(), &before);
        assert_eq!(blocked.session().summary().last_total_sequence, None);
        assert_eq!(blocked.session().summary().stage5c_callback_count, 0);
    }

    #[test]
    fn stage5ge_b_r1_crash_after_exact_persist_keeps_valid_commit_without_candidate() {
        let (session, request_id, client_order_id, attribution, time_shift, polls) =
            stage5ge_b_after_first_poll();
        let before = stage5ge_b_committed_checkpoint(&session);
        let proof = stage5ge_b_r1_exact_replay(
            &before,
            3,
            request_id,
            &client_order_id,
            attribution,
            time_shift,
            &polls[0],
        );
        let persisted = proof.checkpoint().clone();
        crate::validate_stage5g_timer_checkpoint(&persisted).unwrap();
        drop(proof);
        drop(session);
        assert_eq!(persisted.payload.last_total_sequence, Some(3));
        assert_eq!(persisted.payload.duplicate_evidence_count, 1);
        assert_eq!(
            persisted.payload.evidence_replay_ledger,
            before.payload.evidence_replay_ledger
        );
    }

    #[test]
    fn stage5ge_b_r2_historical_a_b_exact_a_then_c_is_continuous() {
        let (session, request_id, client_order_id, attribution, time_shift, polls) =
            stage5ge_b_after_first_poll();
        let (session, checkpoint_b) = stage5ge_b_r2_apply_awaiting_package(
            session,
            3,
            request_id,
            &client_order_id,
            attribution.clone(),
            time_shift,
            &polls[1],
        );
        let exact_a = stage5ge_b_r1_exact_replay(
            &checkpoint_b,
            4,
            request_id,
            &client_order_id,
            attribution.clone(),
            time_shift,
            &polls[0],
        );
        assert_ne!(
            Some(exact_a.canonical_identity()),
            checkpoint_b.payload.current_evidence_identity.as_deref()
        );
        assert!(
            r2cb_golden_truth(&polls[0], &client_order_id, time_shift)
                .received_ts
                .timestamp_millis()
                < checkpoint_b
                    .payload
                    .last_continuation_checkpoint_ts_utc_ms
                    .unwrap()
        );
        let mut expected_exact = checkpoint_b.payload.clone();
        expected_exact.last_total_sequence = Some(4);
        expected_exact.duplicate_evidence_count += 1;
        let synchronized = crate::apply_stage5g_exact_replay_to_session(session, exact_a)
            .expect("historical exact A bypasses NewPackage chronology");
        assert_eq!(synchronized.checkpoint().payload, expected_exact);
        assert_eq!(synchronized.session().summary().stage5c_callback_count, 0);

        let (session, exact_checkpoint) = synchronized.into_parts();
        let candidate_c = stage5ge_b_candidate(
            &exact_checkpoint,
            Stage5gOrderPositionEvidence {
                total_sequence: 5,
                request_id,
                broker_truth: r2cb_golden_truth(&polls[2], &client_order_id, time_shift),
                order_attribution: attribution,
            },
        );
        let committed_c = crate::apply_stage5g_new_package_candidate(session, candidate_c)
            .expect("C applies after historical exact A");
        assert_eq!(
            committed_c.checkpoint().payload.last_total_sequence,
            Some(5)
        );
        assert_eq!(
            committed_c
                .checkpoint()
                .payload
                .evidence_replay_ledger
                .len(),
            checkpoint_b.payload.evidence_replay_ledger.len() + 1
        );
        assert_eq!(
            committed_c
                .into_converged()
                .expect("third poll converges")
                .converged()
                .summary()
                .stage5c_callback_count,
            1
        );
    }

    #[test]
    fn stage5ge_b_r2_raw_historical_exact_uses_the_same_metadata_authority() {
        let (session, request_id, client_order_id, attribution, time_shift, polls) =
            stage5ge_b_after_first_poll();
        let (session, checkpoint_b) = stage5ge_b_r2_apply_awaiting_package(
            session,
            3,
            request_id,
            &client_order_id,
            attribution.clone(),
            time_shift,
            &polls[1],
        );
        let mut expected = checkpoint_b.payload.clone();
        expected.last_total_sequence = Some(4);
        expected.duplicate_evidence_count += 1;
        let session = apply_stage5g_order_position_evidence(
            session,
            Stage5gOrderPositionEvidence {
                total_sequence: 4,
                request_id,
                broker_truth: r2cb_golden_truth(&polls[0], &client_order_id, time_shift),
                order_attribution: attribution,
            },
        )
        .expect("raw historical exact replay bypasses NewPackage preflight")
        .into_awaiting()
        .expect("exact replay cannot converge");
        assert_eq!(stage5ge_b_committed_checkpoint(&session).payload, expected);
        assert_eq!(session.summary().stage5c_callback_count, 0);
    }

    #[test]
    fn stage5ge_b_r2_two_historical_exact_replays_then_new_package() {
        let (session, request_id, client_order_id, attribution, time_shift, polls) =
            stage5ge_b_after_first_poll();
        let (mut session, mut checkpoint) = stage5ge_b_r2_apply_awaiting_package(
            session,
            3,
            request_id,
            &client_order_id,
            attribution.clone(),
            time_shift,
            &polls[1],
        );
        let ledger_before_exact = checkpoint.payload.evidence_replay_ledger.clone();
        for sequence in [4, 5] {
            let exact_a = stage5ge_b_r1_exact_replay(
                &checkpoint,
                sequence,
                request_id,
                &client_order_id,
                attribution.clone(),
                time_shift,
                &polls[0],
            );
            let synchronized = crate::apply_stage5g_exact_replay_to_session(session, exact_a)
                .expect("each historical exact replay updates metadata only");
            (session, checkpoint) = synchronized.into_parts();
        }
        assert_eq!(checkpoint.payload.last_total_sequence, Some(5));
        assert_eq!(checkpoint.payload.duplicate_evidence_count, 2);
        assert_eq!(
            checkpoint.payload.evidence_replay_ledger,
            ledger_before_exact
        );

        let candidate_c = stage5ge_b_candidate(
            &checkpoint,
            Stage5gOrderPositionEvidence {
                total_sequence: 6,
                request_id,
                broker_truth: r2cb_golden_truth(&polls[2], &client_order_id, time_shift),
                order_attribution: attribution,
            },
        );
        let committed_c = crate::apply_stage5g_new_package_candidate(session, candidate_c)
            .expect("new package follows two historical replays");
        assert_eq!(
            committed_c.checkpoint().payload.last_total_sequence,
            Some(6)
        );
    }

    #[test]
    fn stage5ge_b_r2_inherited_older_request_exact_replay_preserves_current_slot() {
        let base_bar_close = Utc::now().timestamp().div_euclid(600) * 600 - 600;
        let (first, request_r1, client_r1, attribution_r1, shift_r1) =
            r2cb_public_runtime_session_at(base_bar_close);
        let golden: serde_json::Value = serde_json::from_str(include_str!(
            "../../../fixtures/expected/stage5g_r2cb_three_poll_broker_truth.json"
        ))
        .unwrap();
        let polls = golden["polls"].as_array().unwrap();
        let first = apply_stage5g_order_position_evidence(
            first,
            Stage5gOrderPositionEvidence {
                total_sequence: 2,
                request_id: request_r1,
                broker_truth: r2cb_golden_truth(&polls[0], &client_r1, shift_r1),
                order_attribution: attribution_r1.clone(),
            },
        )
        .unwrap()
        .into_awaiting()
        .unwrap();
        let inherited_replay = stage5g_order_position_session_replay(&first);

        let (second, request_r2, client_r2, attribution_r2, shift_r2) =
            r2cb_public_runtime_session_at(base_bar_close + 600);
        assert_ne!(request_r1, request_r2);
        let Stage5gOrderPositionSession {
            ack_resolved,
            state: _,
        } = second;
        let second =
            attach_stage5g_order_position_session_with_replay(ack_resolved, Some(inherited_replay))
                .expect("new request inherits the accepted replay ledger");
        assert_eq!(second.state.slots[0].ack.request_id, request_r2);
        let checkpoint = stage5ge_b_committed_checkpoint(&second);
        let exact_r1 = stage5ge_b_r1_exact_replay(
            &checkpoint,
            3,
            request_r1,
            &client_r1,
            attribution_r1,
            shift_r1,
            &polls[0],
        );
        let synchronized = crate::apply_stage5g_exact_replay_to_session(second, exact_r1)
            .expect("inherited R1 identity does not require a current R1 slot");
        assert_eq!(
            synchronized.session().state.slots[0].ack.request_id,
            request_r2
        );
        assert!(synchronized.session().state.slots[0]
            .order_events
            .is_empty());
        assert!(synchronized.session().state.slots[0].trades.is_empty());
        assert_eq!(synchronized.session().summary().stage5c_callback_count, 0);

        let (second, exact_checkpoint) = synchronized.into_parts();
        let next_r2 = stage5ge_b_candidate(
            &exact_checkpoint,
            Stage5gOrderPositionEvidence {
                total_sequence: 4,
                request_id: request_r2,
                broker_truth: r2cb_golden_truth(&polls[0], &client_r2, shift_r2),
                order_attribution: attribution_r2,
            },
        );
        let committed_r2 = crate::apply_stage5g_new_package_candidate(second, next_r2)
            .expect("current R2 package follows inherited historical exact replay");
        assert_eq!(
            committed_r2.checkpoint().payload.last_total_sequence,
            Some(4)
        );
        assert_eq!(
            committed_r2
                .checkpoint()
                .payload
                .evidence_replay_ledger
                .len(),
            exact_checkpoint.payload.evidence_replay_ledger.len() + 1
        );
    }

    #[test]
    fn stage5ge_b_r2_new_identity_before_continuation_still_blocks() {
        let (session, request_id, client_order_id, attribution, time_shift, polls) =
            stage5ge_b_after_first_poll();
        let (session, checkpoint_b) = stage5ge_b_r2_apply_awaiting_package(
            session,
            3,
            request_id,
            &client_order_id,
            attribution.clone(),
            time_shift,
            &polls[1],
        );
        let mut unseen_old = r2cb_golden_truth(&polls[2], &client_order_id, time_shift);
        unseen_old.received_ts = r2cb_golden_truth(&polls[0], &client_order_id, time_shift)
            .received_ts
            + Duration::milliseconds(1);
        assert!(
            unseen_old.received_ts.timestamp_millis()
                < checkpoint_b
                    .payload
                    .last_continuation_checkpoint_ts_utc_ms
                    .unwrap()
        );
        let blocked = match apply_stage5g_order_position_evidence(
            session,
            Stage5gOrderPositionEvidence {
                total_sequence: 4,
                request_id,
                broker_truth: unseen_old,
                order_attribution: attribution,
            },
        ) {
            Err(blocked) => blocked,
            Ok(_) => panic!("new package cannot bypass continuation chronology"),
        };
        assert_eq!(
            blocked.reason(),
            Stage5gOrderPositionError::BrokerTruthBeforeContinuationCheckpoint
        );
    }

    #[test]
    fn stage5ge_b_r2_historical_identity_fingerprint_conflict_still_blocks() {
        let (session, request_id, client_order_id, attribution, time_shift, polls) =
            stage5ge_b_after_first_poll();
        let (session, _) = stage5ge_b_r2_apply_awaiting_package(
            session,
            3,
            request_id,
            &client_order_id,
            attribution.clone(),
            time_shift,
            &polls[1],
        );
        let mut conflicting_a = r2cb_golden_truth(&polls[0], &client_order_id, time_shift);
        conflicting_a.positions[0].qty += Decimal::new(1, 1);
        let blocked = match apply_stage5g_order_position_evidence(
            session,
            Stage5gOrderPositionEvidence {
                total_sequence: 4,
                request_id,
                broker_truth: conflicting_a,
                order_attribution: attribution,
            },
        ) {
            Err(blocked) => blocked,
            Ok(_) => panic!("known identity with another fingerprint remains conflicting"),
        };
        assert_eq!(
            blocked.reason(),
            Stage5gOrderPositionError::ConflictingDuplicateEvidence
        );
    }

    #[test]
    fn stage5ge_b_awaiting_commits_only_after_owned_canonical_application() {
        let (session, request_id, client_order_id, attribution, time_shift, polls) =
            stage5ge_b_after_first_poll();
        let before = stage5ge_b_committed_checkpoint(&session);
        let candidate = stage5ge_b_candidate(
            &before,
            Stage5gOrderPositionEvidence {
                total_sequence: 3,
                request_id,
                broker_truth: r2cb_golden_truth(&polls[1], &client_order_id, time_shift),
                order_attribution: attribution,
            },
        );
        assert_eq!(candidate.pre_candidate_checkpoint(), &before);

        let committed = crate::apply_stage5g_new_package_candidate(session, candidate)
            .expect("owned canonical candidate applies")
            .into_awaiting()
            .expect("second partial poll remains awaiting");
        assert_eq!(committed.checkpoint().payload.last_total_sequence, Some(3));
        assert_eq!(
            committed.checkpoint().payload.evidence_replay_ledger.len(),
            before.payload.evidence_replay_ledger.len() + 1
        );
        assert_eq!(committed.session().summary().stage5c_callback_count, 0);
        crate::validate_stage5g_timer_checkpoint(committed.checkpoint()).unwrap();
    }

    #[test]
    fn stage5ge_b_raw_and_owned_canonical_routes_share_exact_apply_core() {
        let (raw_session, request_id, client_order_id, attribution, time_shift, polls) =
            stage5ge_b_after_first_poll();
        let (
            owned_session,
            owned_request_id,
            owned_client_order_id,
            owned_attribution,
            owned_time_shift,
            owned_polls,
        ) = stage5ge_b_after_first_poll();
        assert_eq!(request_id, owned_request_id);
        let raw_evidence = Stage5gOrderPositionEvidence {
            total_sequence: 3,
            request_id,
            broker_truth: r2cb_golden_truth(&polls[1], &client_order_id, time_shift),
            order_attribution: attribution,
        };
        let owned_evidence = Stage5gOrderPositionEvidence {
            total_sequence: 3,
            request_id: owned_request_id,
            broker_truth: r2cb_golden_truth(
                &owned_polls[1],
                &owned_client_order_id,
                owned_time_shift,
            ),
            order_attribution: owned_attribution,
        };
        let raw = apply_stage5g_order_position_evidence(raw_session, raw_evidence)
            .unwrap()
            .into_awaiting()
            .unwrap();
        let before = stage5ge_b_committed_checkpoint(&owned_session);
        let owned = crate::apply_stage5g_new_package_candidate(
            owned_session,
            stage5ge_b_candidate(&before, owned_evidence),
        )
        .unwrap()
        .into_awaiting()
        .unwrap();
        assert_eq!(stage5ge_b_committed_checkpoint(&raw), *owned.checkpoint());
    }

    #[test]
    fn stage5ge_b_normal_convergence_commits_exact_applied_replay_once() {
        let (session, request_id, client_order_id, attribution, time_shift, polls) =
            stage5ge_b_after_first_poll();
        let before_second = stage5ge_b_committed_checkpoint(&session);
        let second = stage5ge_b_candidate(
            &before_second,
            Stage5gOrderPositionEvidence {
                total_sequence: 3,
                request_id,
                broker_truth: r2cb_golden_truth(&polls[1], &client_order_id, time_shift),
                order_attribution: attribution.clone(),
            },
        );
        let (session, second_checkpoint) =
            crate::apply_stage5g_new_package_candidate(session, second)
                .unwrap()
                .into_awaiting()
                .unwrap()
                .into_parts();
        let third = stage5ge_b_candidate(
            &second_checkpoint,
            Stage5gOrderPositionEvidence {
                total_sequence: 4,
                request_id,
                broker_truth: r2cb_golden_truth(&polls[2], &client_order_id, time_shift),
                order_attribution: attribution,
            },
        );
        let committed = crate::apply_stage5g_new_package_candidate(session, third)
            .expect("terminal filled package applies")
            .into_converged()
            .expect("filled Market order follows normal Stage 5C convergence");
        assert_eq!(committed.checkpoint().payload.last_total_sequence, Some(4));
        assert_eq!(committed.converged().summary().stage5c_callback_count, 1);
        assert_eq!(committed.converged().summary().terminal_request_count, 1);
        crate::validate_stage5g_timer_checkpoint(committed.checkpoint()).unwrap();
    }

    #[test]
    fn stage5ge_b_transactional_block_returns_only_pre_candidate_commit() {
        let (session, request_id, client_order_id, attribution, time_shift, polls) =
            stage5ge_b_after_first_poll();
        let before = stage5ge_b_committed_checkpoint(&session);
        let mut incomplete = r2cb_golden_truth(&polls[1], &client_order_id, time_shift);
        incomplete.orders.clear();
        let candidate = stage5ge_b_candidate(
            &before,
            Stage5gOrderPositionEvidence {
                total_sequence: 3,
                request_id,
                broker_truth: incomplete,
                order_attribution: attribution.clone(),
            },
        );
        let blocked = match crate::apply_stage5g_new_package_candidate(session, candidate) {
            Err(failure) => failure
                .into_blocked()
                .expect("incomplete package is a transactional Stage 5G-c block"),
            Ok(_) => panic!("incomplete package must block"),
        };
        assert_eq!(
            blocked.stage5g_c_reason(),
            Some(Stage5gOrderPositionError::TargetTradeWithoutOrder)
        );
        assert_eq!(blocked.pre_candidate_checkpoint(), &before);
        let replay_after_block = stage5g_order_position_session_replay(blocked.session());
        assert_eq!(
            replay_after_block.evidence_identities.len(),
            before.payload.evidence_replay_ledger.len()
        );

        let corrected = stage5ge_b_candidate(
            &before,
            Stage5gOrderPositionEvidence {
                total_sequence: 3,
                request_id,
                broker_truth: r2cb_golden_truth(&polls[1], &client_order_id, time_shift),
                order_attribution: attribution,
            },
        );
        let committed =
            crate::apply_stage5g_new_package_candidate(blocked.into_session(), corrected)
                .expect("fresh corrected package is classified from the old checkpoint");
        assert_eq!(committed.checkpoint().payload.last_total_sequence, Some(3));
    }

    #[test]
    fn stage5ge_b_drop_before_apply_keeps_old_checkpoint_reclassifiable() {
        let (session, request_id, client_order_id, attribution, time_shift, polls) =
            stage5ge_b_after_first_poll();
        let before = stage5ge_b_committed_checkpoint(&session);
        let next = Stage5gOrderPositionEvidence {
            total_sequence: 3,
            request_id,
            broker_truth: r2cb_golden_truth(&polls[1], &client_order_id, time_shift),
            order_attribution: attribution,
        };
        let first_identity = stage5ge_b_candidate(&before, next.clone())
            .canonical_identity()
            .to_string();
        let replacement = stage5ge_b_candidate(&before, next);
        assert_eq!(replacement.canonical_identity(), first_identity);
        let committed = crate::apply_stage5g_new_package_candidate(session, replacement).unwrap();
        assert_eq!(committed.checkpoint().payload.last_total_sequence, Some(3));
    }

    #[test]
    fn stage5ge_b_session_checkpoint_mismatch_blocks_before_application() {
        let (session, request_id, client_order_id, attribution, time_shift, polls) =
            stage5ge_b_after_first_poll();
        let before = stage5ge_b_committed_checkpoint(&session);
        let candidate = stage5ge_b_candidate(
            &before,
            Stage5gOrderPositionEvidence {
                total_sequence: 3,
                request_id,
                broker_truth: r2cb_golden_truth(&polls[1], &client_order_id, time_shift),
                order_attribution: attribution,
            },
        );
        let (different_session, ..) = r2cb_public_runtime_session();
        let blocked = match crate::apply_stage5g_new_package_candidate(different_session, candidate)
        {
            Err(failure) => failure.into_blocked().unwrap(),
            Ok(_) => panic!("mismatched session must block before application"),
        };
        assert_eq!(
            blocked.reason(),
            crate::Stage5gNewPackageApplyBlockReason::SessionCheckpointMismatch
        );
        assert_eq!(blocked.pre_candidate_checkpoint(), &before);
        assert_eq!(blocked.session().summary().stage5c_callback_count, 0);
    }

    fn stage5gd_bracket_seeded_exit_settled() -> (crate::Stage5cSettledPaperStrategy, i64) {
        let bar_close_ts = Utc::now().timestamp().div_euclid(600) * 600 - 600;
        let mut strategy = r2cb_public_runtime_strategy(bar_close_ts);
        for (close_time_utc, high, low) in [
            (bar_close_ts - 86_400 - 600, 2630.0, 2570.0),
            (bar_close_ts - 86_400, 2620.0, 2580.0),
        ] {
            assert!(Strategy::on_bar(
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
            )
            .is_empty());
        }
        let mut state = Strategy::state(&strategy).clone();
        let StrategyState::HybridIntradayRuntime {
            active_cycle_id,
            last_position_qty,
            current_owner,
            current_side,
            ..
        } = &mut state
        else {
            panic!("bracket timer fixture requires hybrid runtime state")
        };
        *active_cycle_id = Some("abc1230001".to_string());
        *last_position_qty = 1.0;
        *current_owner = Some(Owner::IntradayBreakout);
        *current_side = Some(Side::Long);
        Strategy::set_state(&mut strategy, state);
        let mut extension = strategy
            .stage5d_export_runtime_private_extension()
            .expect("export source-owned bracket timer");
        extension.bracket_reconciliation_timer = Some(
            crate::stage5d_persistence::Stage5dBracketReconciliationTimer {
                bracket_terminal_reconcile_started_ms: (bar_close_ts + 2) * 1_000,
            },
        );
        strategy
            .stage5d_apply_runtime_private_extension(&extension)
            .expect("apply source-owned bracket timer");
        let signal = broker_core::HybridRuntimeBarEvent {
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
        let lifecycle_now = Utc.timestamp_opt(bar_close_ts - 30, 0).single().unwrap();
        let (recovered, accepted) =
            crate::stage5c_paper_host::stage5f_test_seams::sequence_inputs_from_owned_strategy(
                strategy,
                "hybrid_imoexf".to_string(),
                BrokerAccountId::new("ACC_TEST_0001"),
                target(),
                0.5,
                Decimal::ONE,
                lifecycle_now,
                bar_close_ts - 600,
                signal,
            );
        let semantic = crate::apply_stage5c_semantic_bar(recovered, accepted)
            .expect("source-reachable bracket Exit semantic callback");
        (
            crate::settle_stage5c_semantic_result(semantic)
                .expect("source-reachable bracket Exit intent settlement"),
            bar_close_ts,
        )
    }

    struct GeneratedLifecycleFixture {
        session: Stage5gOrderPositionSession,
        projection: crate::stage5c_paper_host::Stage5gSourceIntentProjection,
        request_id: StrategyRequestId,
        client_order_id: ClientOrderId,
        broker_order_id: BrokerOrderId,
        side: crate::BrokerNeutralOrderSide,
        ack_received: DateTime<Utc>,
    }

    fn settled_exit_to_order_position(
        settled: crate::Stage5cSettledPaperStrategy,
        checkpoint_ts_utc_ms: i64,
        broker_order_id: &str,
    ) -> GeneratedLifecycleFixture {
        let projections = settled.stage5g_source_intent_projections();
        assert_eq!(projections.len(), 1);
        let projection = projections[0].clone();
        assert_eq!(projection.base_action, Stage5gSourceBaseAction::Market);
        assert_eq!(
            projection.intent_class,
            crate::BrokerNeutralHybridIntentClass::Exit
        );
        let side = projection.side.expect("source Exit side");
        let action = Stage5gMockIntentAction::Place {
            place_kind: Stage5gMockPlaceKind::Market,
        };
        let request_id = projection.request_id;
        let client_order_id = ClientOrderId::from_strategy_request(request_id);
        let broker_order_id = BrokerOrderId::new(broker_order_id);
        let ack_received = Utc
            .timestamp_millis_opt(checkpoint_ts_utc_ms)
            .single()
            .unwrap();
        let ack_session = crate::attach_stage5g_mock_ack_session(
            settled,
            crate::Stage5gMockAckSessionInput {
                intent_bindings: vec![crate::Stage5gMockIntentBinding {
                    request_id,
                    intent_class: projection.intent_class,
                    action: action.clone(),
                    side: Some(side),
                }],
                lifecycle_expires_at_ts_utc: ack_received.timestamp() + 300,
            },
        )
        .expect("source Exit ACK admission");
        let resolved = crate::apply_stage5g_mock_ack(
            ack_session,
            crate::Stage5gMockAckEvent {
                total_sequence: 1,
                intent_request_id: request_id,
                account_id: BrokerAccountId::new("ACC_TEST_0001"),
                instrument: target(),
                action,
                side: Some(side),
                ack: CommandAck {
                    request_id,
                    client_order_id: Some(client_order_id.clone()),
                    broker_order_id: Some(broker_order_id.clone()),
                    status: CommandAckStatus::Accepted,
                    reason: None,
                    received_ts: ack_received,
                },
            },
        )
        .expect("source Exit ACK")
        .into_resolved()
        .expect("single source Exit resolves on one ACK");
        let session = crate::attach_stage5g_order_position_session(resolved)
            .expect("source Exit enters Stage 5G-c");
        GeneratedLifecycleFixture {
            session,
            projection,
            request_id,
            client_order_id,
            broker_order_id,
            side,
            ack_received,
        }
    }

    fn generated_escrow_to_order_position(
        escrow: crate::Stage5gTimerGeneratedIntentEscrow,
        checkpoint_ts_utc_ms: i64,
        broker_order_id: &str,
    ) -> GeneratedLifecycleFixture {
        let projections = escrow.source_intent_projections();
        assert_eq!(projections.len(), 1);
        let projection = projections[0].clone();
        assert_eq!(projection.base_action, Stage5gSourceBaseAction::Market);
        assert_eq!(
            projection.intent_class,
            crate::BrokerNeutralHybridIntentClass::Exit
        );
        let side = projection.side.expect("generated Exit side");
        let action = Stage5gMockIntentAction::Place {
            place_kind: Stage5gMockPlaceKind::Market,
        };
        let request_id = projection.request_id;
        let client_order_id = ClientOrderId::from_strategy_request(request_id);
        let broker_order_id = BrokerOrderId::new(broker_order_id);
        let ack_received = Utc
            .timestamp_millis_opt(checkpoint_ts_utc_ms + 1_000)
            .single()
            .unwrap();
        let mut ack_session = match crate::attach_stage5g_timer_generated_mock_ack(
            escrow,
            crate::Stage5gMockAckSessionInput {
                intent_bindings: vec![crate::Stage5gMockIntentBinding {
                    request_id,
                    intent_class: projection.intent_class,
                    action: action.clone(),
                    side: Some(side),
                }],
                lifecycle_expires_at_ts_utc: ack_received.timestamp() + 300,
            },
        ) {
            Ok(session) => session,
            Err(blocked) => panic!("generated ACK admission blocked: {:?}", blocked.reason()),
        };
        let ack_event = crate::Stage5gMockAckEvent {
            total_sequence: 1,
            intent_request_id: request_id,
            account_id: BrokerAccountId::new("ACC_TEST_0001"),
            instrument: target(),
            action,
            side: Some(side),
            ack: CommandAck {
                request_id,
                client_order_id: Some(client_order_id.clone()),
                broker_order_id: Some(broker_order_id.clone()),
                status: CommandAckStatus::Accepted,
                reason: None,
                received_ts: ack_received,
            },
        };
        if checkpoint_ts_utc_ms.rem_euclid(1_000) >= 200 {
            let expected_checkpoint = ack_session.checkpoint();
            let mut early = ack_event.clone();
            early.ack.received_ts = Utc
                .timestamp_millis_opt(
                    checkpoint_ts_utc_ms - checkpoint_ts_utc_ms.rem_euclid(1_000) + 100,
                )
                .single()
                .unwrap();
            let blocked = match crate::apply_stage5g_timer_mock_ack(ack_session, early) {
                Err(crate::Stage5gTimerMockAckFailure::Blocked(blocked)) => blocked,
                Ok(_) => panic!("same-second ACK before the continuation checkpoint must block"),
                Err(crate::Stage5gTimerMockAckFailure::Terminal(_)) => {
                    panic!("same-second ACK chronology is retryable")
                }
            };
            assert_eq!(
                blocked.reason(),
                crate::Stage5gTimerMockAckError::AckBeforeContinuationCheckpoint
            );
            ack_session = blocked.into_session();
            assert_eq!(ack_session.checkpoint(), expected_checkpoint);
        }
        let resolved = match match crate::apply_stage5g_timer_mock_ack(ack_session, ack_event) {
            Ok(transition) => transition,
            Err(crate::Stage5gTimerMockAckFailure::Blocked(blocked)) => {
                panic!("generated mock ACK blocked: {:?}", blocked.reason())
            }
            Err(crate::Stage5gTimerMockAckFailure::Terminal(failure)) => {
                panic!("generated mock ACK terminal: {:?}", failure.reason())
            }
        } {
            crate::Stage5gTimerMockAckTransition::Resolved(resolved) => resolved,
            crate::Stage5gTimerMockAckTransition::Awaiting(_) => {
                panic!("single generated intent resolves on one ACK")
            }
        };
        let session = crate::attach_stage5g_timer_order_position_session(resolved)
            .expect("generated checkpoint enters Stage 5G-c");
        GeneratedLifecycleFixture {
            session,
            projection,
            request_id,
            client_order_id,
            broker_order_id,
            side,
            ack_received,
        }
    }

    fn generated_exit_truth(
        fixture: &GeneratedLifecycleFixture,
        received: DateTime<Utc>,
        status: OrderStatus,
        order_qty: Decimal,
        filled_qty: Decimal,
        position_qty: Decimal,
        trade_id: &str,
    ) -> BrokerTruthSnapshot {
        let side = match fixture.side {
            crate::BrokerNeutralOrderSide::Buy => OrderSide::Buy,
            crate::BrokerNeutralOrderSide::Sell => OrderSide::Sell,
        };
        BrokerTruthSnapshot {
            account_id: BrokerAccountId::new("ACC_TEST_0001"),
            orders: vec![BrokerOrderSnapshot {
                account_id: BrokerAccountId::new("ACC_TEST_0001"),
                broker_order_id: Some(fixture.broker_order_id.clone()),
                client_order_id: Some(fixture.client_order_id.clone()),
                instrument: target(),
                side,
                order_type: OrderType::Market,
                time_in_force: Some(TimeInForce::Day),
                lifecycle: BrokerOrderSnapshot::lifecycle_for(&status),
                status,
                qty: order_qty,
                filled_qty,
                remaining_qty: Some(order_qty - filled_qty),
                limit_price: None,
                broker_asset_id: None,
                board: None,
                expiration_date: None,
                source_ts: Some(fixture.ack_received),
                received_ts: received,
            }],
            positions: vec![BrokerPositionSnapshot {
                account_id: BrokerAccountId::new("ACC_TEST_0001"),
                instrument: target(),
                qty: position_qty,
                avg_price: (position_qty != Decimal::ZERO).then_some(Decimal::new(2_720, 0)),
                unrealized_pnl: Some(Decimal::ZERO),
                source_ts: Some(fixture.ack_received),
                received_ts: received,
            }],
            cash: None,
            trades: vec![BrokerTradeSnapshot {
                account_id: BrokerAccountId::new("ACC_TEST_0001"),
                broker_trade_id: BrokerTradeId::new(trade_id),
                broker_order_id: Some(fixture.broker_order_id.clone()),
                client_order_id: Some(fixture.client_order_id.clone()),
                instrument: target(),
                side,
                qty: filled_qty,
                price: Decimal::new(2_720, 0),
                gross_amount: None,
                commission: None,
                broker_asset_id: None,
                board: None,
                expiration_date: None,
                source_ts: fixture.ack_received,
                received_ts: received,
            }],
            instruments: Vec::new(),
            received_ts: received,
        }
    }

    #[test]
    fn stage5ge_b_r3_market_terminal_candidate_commits_without_callback_duplication() {
        let (settled, bar_close_ts) = stage5gd_bracket_seeded_exit_settled();
        let fixture =
            settled_exit_to_order_position(settled, bar_close_ts * 1_000, "FINAM-E-B-R3-ORDER-1");
        let working_received = Utc
            .timestamp_millis_opt(bar_close_ts * 1_000 + 2_500)
            .single()
            .unwrap();
        let terminal_received = Utc
            .timestamp_millis_opt(bar_close_ts * 1_000 + 3_000)
            .single()
            .unwrap();
        let request_id = fixture.request_id;
        let attribution = fixture.projection.expected_attribution.clone();
        let working_truth = generated_exit_truth(
            &fixture,
            working_received,
            OrderStatus::PartiallyFilled,
            Decimal::ONE,
            Decimal::new(4, 1),
            Decimal::new(6, 1),
            "FINAM-E-B-R3-TRADE-1",
        );
        let terminal_truth = generated_exit_truth(
            &fixture,
            terminal_received,
            OrderStatus::Canceled,
            Decimal::ONE,
            Decimal::new(4, 1),
            Decimal::new(6, 1),
            "FINAM-E-B-R3-TRADE-1",
        );
        let session = apply_stage5g_order_position_evidence(
            fixture.session,
            Stage5gOrderPositionEvidence {
                total_sequence: 1,
                request_id,
                broker_truth: working_truth,
                order_attribution: attribution.clone(),
            },
        )
        .unwrap()
        .into_awaiting()
        .expect("partial Exit remains awaiting");
        let before = stage5ge_b_committed_checkpoint(&session);
        let candidate = stage5ge_b_candidate(
            &before,
            Stage5gOrderPositionEvidence {
                total_sequence: 2,
                request_id,
                broker_truth: terminal_truth,
                order_attribution: attribution,
            },
        );
        let committed = crate::apply_stage5g_new_package_candidate(session, candidate)
            .expect("accepted R3 authority settles terminal partial Exit")
            .into_market_terminal()
            .expect("canceled partially filled Market Exit is R3 terminal");
        assert_eq!(committed.checkpoint().payload.last_total_sequence, Some(2));
        assert_eq!(committed.converged().summary().stage5c_callback_count, 1);
        assert_eq!(
            committed.checkpoint().payload.evidence_replay_ledger.len(),
            before.payload.evidence_replay_ledger.len() + 1
        );
        crate::validate_stage5g_timer_checkpoint(committed.checkpoint()).unwrap();
    }

    fn accepted_stage5gd_bar(
        close_time_utc: i64,
        instrument: InstrumentId,
        tick_size: f64,
        low: f64,
        close: f64,
    ) -> crate::Stage5cAcceptedSemanticBar {
        crate::accept_stage5c_semantic_bar(crate::Stage5cSemanticBarInput {
            bar: broker_core::HybridRuntimeBarEvent {
                instrument,
                close_time_utc,
                open: 2_720.0,
                high: 2_721.0,
                low,
                close,
                volume: 10.0,
                origin: broker_core::HybridRuntimeBarOrigin::Live,
                is_final: true,
                timeframe_sec: 600,
            },
            provenance: broker_core::Stage3StrategyBarProvenance::finam_derived_m1_to_m10_complete(
            ),
            tick_size,
        })
        .expect("canonical Stage 5G-d test bar")
    }

    #[test]
    fn stage5gd_public_convergence_timer_is_linear_and_monotonic() {
        let converged = r2cb_public_converged_for_timer();
        let callback_count = converged.summary().stage5c_callback_count;
        let watermark_ms = converged
            .replay_checkpoint
            .last_broker_truth_received_ms
            .unwrap();
        let timer_ts = watermark_ms + 1;
        let session = crate::attach_stage5g_timer_session(converged);
        assert_eq!(
            session
                .checkpoint()
                .payload
                .last_broker_truth_received_at
                .unwrap()
                .timestamp_subsec_nanos(),
            875_000_000
        );
        let ready = match crate::apply_stage5g_timer_checkpoint(
            session,
            crate::Stage5cPaperTimerInput {
                now_ts_utc_ms: timer_ts,
            },
        )
        .expect("one explicit timer advances one converged capability")
        {
            crate::Stage5gTimerTransition::Ready(ready) => ready,
            crate::Stage5gTimerTransition::GeneratedIntent(_) => {
                panic!("golden BO fill has no immediate timer intent")
            }
        };
        assert_eq!(ready.summary().stage5c_callback_count, callback_count);
        assert_eq!(ready.checkpoint_ts_utc_ms(), timer_ts);
        assert!(!ready.intent_sink_attached());
        assert!(!ready.redis_command_stream_attached());
        assert!(!ready.finam_transport_attached());
        assert!(!ready.broker_execution_attached());

        let blocked = crate::continue_stage5g_timer_with_timer(
            ready,
            crate::Stage5cPaperTimerInput {
                now_ts_utc_ms: timer_ts,
            },
        )
        .expect_err("equal checkpoint must fail closed");
        assert_eq!(
            blocked.reason(),
            crate::Stage5gTimerError::NonMonotonicCheckpoint
        );
        let preserved = blocked
            .into_blocked()
            .expect("equal timer is retryable")
            .into_session();
        assert_eq!(
            preserved
                .checkpoint()
                .payload
                .last_continuation_checkpoint_ts_utc_ms,
            Some(timer_ts)
        );
    }

    #[test]
    fn stage5gd_reversed_initial_timer_preserves_exact_checkpoint() {
        let converged = r2cb_public_converged_for_timer();
        let watermark_ms = converged
            .replay_checkpoint
            .last_broker_truth_received_ms
            .unwrap();
        let session = crate::attach_stage5g_timer_session(converged);
        let expected = session.checkpoint();
        let blocked = crate::apply_stage5g_timer_checkpoint(
            session,
            crate::Stage5cPaperTimerInput {
                now_ts_utc_ms: watermark_ms - 1,
            },
        )
        .expect_err("reversed initial timer must fail closed");
        assert_eq!(
            blocked.reason(),
            crate::Stage5gTimerError::NonMonotonicCheckpoint
        );
        assert_eq!(
            blocked
                .into_blocked()
                .expect("reversed timer is retryable")
                .into_session()
                .checkpoint(),
            expected
        );
    }

    #[test]
    fn stage5gd_zero_intent_bar_rearms_timer_and_later_bar_without_callback_loss() {
        let converged = r2cb_public_converged_for_timer();
        let exact_receipt = converged
            .replay_checkpoint
            .last_broker_truth_received_at
            .unwrap();
        let initial_ledger = converged.replay_checkpoint.evidence_identities.clone();
        let broker_ms = exact_receipt.timestamp_millis();
        let ready = match crate::apply_stage5g_timer_checkpoint(
            crate::attach_stage5g_timer_session(converged),
            crate::Stage5cPaperTimerInput {
                now_ts_utc_ms: broker_ms + 1,
            },
        )
        .unwrap()
        {
            crate::Stage5gTimerTransition::Ready(ready) => ready,
            crate::Stage5gTimerTransition::GeneratedIntent(_) => panic!("unexpected timer intent"),
        };
        let next_close = exact_receipt.timestamp() - 10 + 600;
        let continuation = crate::continue_stage5g_timer_with_bar(
            ready,
            accepted_stage5gd_bar(next_close, target(), 0.5, 2_719.0, 2_720.0),
        )
        .expect("mild next bar is accepted transactionally");
        assert_eq!(continuation.intent_count(), 0);
        let retained = match crate::settle_stage5g_bar_continuation(continuation) {
            crate::Stage5gBarContinuationTransition::Ready(retained) => retained,
            crate::Stage5gBarContinuationTransition::GeneratedIntent(_) => {
                panic!("mild next bar must stay zero-intent")
            }
        };
        let checkpoint = retained.checkpoint();
        assert_eq!(
            checkpoint.payload.last_continuation_checkpoint_ts_utc_ms,
            Some(next_close * 1_000)
        );
        assert_eq!(
            checkpoint.payload.evidence_replay_ledger.len(),
            initial_ledger.len()
        );
        for previous in initial_ledger {
            assert!(checkpoint
                .payload
                .evidence_replay_ledger
                .iter()
                .any(|entry| entry.identity == previous.identity
                    && entry.fingerprint_sha256 == previous.fingerprint));
        }
        crate::validate_stage5g_timer_checkpoint(&checkpoint).unwrap();

        let timer_checkpoint_ms = next_close * 1_000 + 1;
        let timer_ready = match crate::continue_stage5g_timer_with_timer(
            retained,
            crate::Stage5cPaperTimerInput {
                now_ts_utc_ms: timer_checkpoint_ms,
            },
        )
        .expect("re-armed zero-intent bar accepts the next explicit timer")
        {
            crate::Stage5gTimerTransition::Ready(ready) => ready,
            crate::Stage5gTimerTransition::GeneratedIntent(_) => {
                panic!("fixture timer remains zero-intent")
            }
        };
        assert_eq!(
            timer_ready
                .checkpoint()
                .payload
                .last_continuation_checkpoint_ts_utc_ms,
            Some(timer_checkpoint_ms)
        );

        let second_close = next_close + 600;
        let second = crate::continue_stage5g_timer_with_bar(
            timer_ready,
            accepted_stage5gd_bar(second_close, target(), 0.5, 2_718.0, 2_719.0),
        )
        .expect("timer-ready state accepts the later explicit bar");
        let second_ready = match crate::settle_stage5g_bar_continuation(second) {
            crate::Stage5gBarContinuationTransition::Ready(ready) => ready,
            crate::Stage5gBarContinuationTransition::GeneratedIntent(_) => {
                panic!("second mild bar remains zero-intent")
            }
        };
        assert_eq!(
            second_ready
                .checkpoint()
                .payload
                .last_continuation_checkpoint_ts_utc_ms,
            Some(second_close * 1_000)
        );
    }

    #[test]
    fn stage5gd_bar_preflight_failure_returns_exact_incoming_checkpoint() {
        let converged = r2cb_public_converged_for_timer();
        let exact_receipt = converged
            .replay_checkpoint
            .last_broker_truth_received_at
            .unwrap();
        let broker_ms = exact_receipt.timestamp_millis();
        let ready = match crate::apply_stage5g_timer_checkpoint(
            crate::attach_stage5g_timer_session(converged),
            crate::Stage5cPaperTimerInput {
                now_ts_utc_ms: broker_ms + 1,
            },
        )
        .unwrap()
        {
            crate::Stage5gTimerTransition::Ready(ready) => ready,
            crate::Stage5gTimerTransition::GeneratedIntent(_) => panic!("unexpected timer intent"),
        };
        let expected = ready.checkpoint();
        let next_close = exact_receipt.timestamp() - 10 + 600;
        let failure = match crate::continue_stage5g_timer_with_bar(
            ready,
            accepted_stage5gd_bar(next_close, target(), 1.0, 2_719.0, 2_720.0),
        ) {
            Ok(_) => panic!("wrong tick size unexpectedly accepted"),
            Err(failure) => failure,
        };
        assert_eq!(
            failure.reason(),
            crate::Stage5gTimerError::Stage5c(crate::Stage5cPaperLoopError::TimerContinuation(
                crate::Stage5cTimerContinuationError::NextBar(
                    crate::Stage5cNextBarLoopError::Semantic(
                        crate::Stage5cSemanticBarError::TickSizeMismatch,
                    ),
                ),
            ))
        );
        let preserved = failure
            .into_blocked()
            .expect("bar preflight failure is retryable")
            .into_session()
            .checkpoint();
        assert_eq!(preserved, expected);
    }

    #[test]
    fn stage5gd_r3_market_terminal_continues_only_through_timer_boundary() {
        let golden: serde_json::Value = serde_json::from_str(include_str!(
            "../../../fixtures/expected/stage5g_r2cb_three_poll_broker_truth.json"
        ))
        .unwrap();
        let (session, request_id, client_order_id, expected_attribution, time_shift) =
            r2cb_public_runtime_session();
        let mut rejected = r2cb_golden_truth(&golden["polls"][0], &client_order_id, time_shift);
        rejected.orders[0].status = OrderStatus::Rejected;
        rejected.orders[0].lifecycle = BrokerOrderLifecycle::Terminal;
        rejected.orders[0].filled_qty = Decimal::ZERO;
        rejected.orders[0].remaining_qty = Some(Decimal::ONE);
        rejected.positions.clear();
        rejected.trades.clear();
        let received_ms = rejected.received_ts.timestamp_millis();
        let terminal = apply_stage5g_order_position_evidence(
            session,
            Stage5gOrderPositionEvidence {
                total_sequence: 2,
                request_id,
                broker_truth: rejected,
                order_attribution: expected_attribution,
            },
        )
        .expect("rejected Market truth is settled by accepted R3 authority")
        .into_market_terminal_converged()
        .expect("R3 market terminal convergence remains distinct");
        assert!(terminal.settlement().is_ready_for_timer());
        let session = crate::attach_stage5g_market_terminal_timer_session(terminal);
        let transition = crate::apply_stage5g_timer_checkpoint(
            session,
            crate::Stage5cPaperTimerInput {
                now_ts_utc_ms: received_ms + 1,
            },
        )
        .expect("R3 terminal settlement reaches the accepted Stage 5C timer");
        match transition {
            crate::Stage5gTimerTransition::Ready(ready) => {
                assert_eq!(ready.checkpoint_ts_utc_ms(), received_ms + 1);
                assert!(!ready.broker_execution_attached());
            }
            crate::Stage5gTimerTransition::GeneratedIntent(escrow) => {
                assert!(escrow.intent_count() > 0);
                assert!(!escrow.broker_execution_attached());
            }
        }
    }

    #[test]
    fn stage5gd_bar_generated_intent_roundtrips_through_ack_truth_and_next_timer() {
        let converged = r2cb_public_converged_for_timer();
        let initial_checkpoint = converged.replay_checkpoint.clone();
        let broker_watermark_ms = initial_checkpoint
            .last_broker_truth_received_ms
            .expect("initial exact broker watermark");
        let initial_ledger = initial_checkpoint.evidence_identities.clone();
        let initial_callback_count = converged.summary().stage5c_callback_count;

        let ready = match crate::apply_stage5g_timer_checkpoint(
            crate::attach_stage5g_timer_session(converged),
            crate::Stage5cPaperTimerInput {
                now_ts_utc_ms: broker_watermark_ms + 1,
            },
        )
        .expect("first post-convergence timer is source reachable")
        {
            crate::Stage5gTimerTransition::Ready(ready) => ready,
            crate::Stage5gTimerTransition::GeneratedIntent(_) => {
                panic!("first post-convergence timer must remain zero-intent")
            }
        };
        let initial_bar_close_ts = initial_checkpoint
            .last_broker_truth_received_at
            .expect("initial exact receipt")
            .timestamp()
            - 10;
        let generated_bar_close_ts = initial_bar_close_ts + 600;
        let accepted = crate::accept_stage5c_semantic_bar(crate::Stage5cSemanticBarInput {
            bar: broker_core::HybridRuntimeBarEvent {
                instrument: target(),
                close_time_utc: generated_bar_close_ts,
                open: 2_720.0,
                high: 2_721.0,
                low: 2_000.0,
                close: 2_000.0,
                volume: 10.0,
                origin: broker_core::HybridRuntimeBarOrigin::Live,
                is_final: true,
                timeframe_sec: 600,
            },
            provenance: broker_core::Stage3StrategyBarProvenance::finam_derived_m1_to_m10_complete(
            ),
            tick_size: 0.5,
        })
        .expect("next exact M10 bar is accepted");
        let generated_checkpoint_ms = generated_bar_close_ts * 1_000;
        let continuation = crate::continue_stage5g_timer_with_bar(ready, accepted)
            .expect("transactional Stage 5G-d bar transition");
        let escrow = match crate::settle_stage5g_bar_continuation(continuation) {
            crate::Stage5gBarContinuationTransition::GeneratedIntent(escrow) => escrow,
            crate::Stage5gBarContinuationTransition::Ready(_) => {
                panic!("large adverse M10 bar must generate the BO Exit")
            }
        };
        assert!(escrow.intent_count() >= 1);
        assert_eq!(
            escrow
                .checkpoint()
                .payload
                .last_continuation_checkpoint_ts_utc_ms,
            Some(generated_checkpoint_ms)
        );
        let projections = escrow.source_intent_projections();
        assert_eq!(projections.len(), 1);
        let projection = projections[0].clone();
        assert_eq!(projection.base_action, Stage5gSourceBaseAction::Market);
        assert_eq!(
            projection.intent_class,
            crate::BrokerNeutralHybridIntentClass::Exit
        );
        let side = projection.side.expect("timer Exit side");
        let action = Stage5gMockIntentAction::Place {
            place_kind: Stage5gMockPlaceKind::Market,
        };
        let request_id = projection.request_id;
        let client_order_id = ClientOrderId::from_strategy_request(request_id);
        let ack_received = Utc
            .timestamp_millis_opt(generated_checkpoint_ms + 1_000)
            .single()
            .unwrap();
        let ack_session = match crate::attach_stage5g_timer_generated_mock_ack(
            escrow,
            crate::Stage5gMockAckSessionInput {
                intent_bindings: vec![crate::Stage5gMockIntentBinding {
                    request_id,
                    intent_class: projection.intent_class,
                    action: action.clone(),
                    side: Some(side),
                }],
                lifecycle_expires_at_ts_utc: ack_received.timestamp() + 300,
            },
        ) {
            Ok(session) => session,
            Err(blocked) => panic!("timer ACK admission blocked: {:?}", blocked.reason()),
        };
        let resolved = match match crate::apply_stage5g_timer_mock_ack(
            ack_session,
            crate::Stage5gMockAckEvent {
                total_sequence: 1,
                intent_request_id: request_id,
                account_id: BrokerAccountId::new("ACC_TEST_0001"),
                instrument: target(),
                action: action.clone(),
                side: Some(side),
                ack: CommandAck {
                    request_id,
                    client_order_id: Some(client_order_id.clone()),
                    broker_order_id: Some(BrokerOrderId::new("FINAM-TIMER-EXIT-1")),
                    status: CommandAckStatus::Accepted,
                    reason: None,
                    received_ts: ack_received,
                },
            },
        ) {
            Ok(transition) => transition,
            Err(crate::Stage5gTimerMockAckFailure::Blocked(blocked)) => {
                panic!("timer-generated mock ACK blocked: {:?}", blocked.reason())
            }
            Err(crate::Stage5gTimerMockAckFailure::Terminal(failure)) => {
                panic!("timer-generated mock ACK terminal: {:?}", failure.reason())
            }
        } {
            crate::Stage5gTimerMockAckTransition::Resolved(resolved) => resolved,
            crate::Stage5gTimerMockAckTransition::Awaiting(_) => {
                panic!("single timer intent must resolve on one canonical ACK")
            }
        };
        let mut order_position = crate::attach_stage5g_timer_order_position_session(resolved)
            .expect("timer checkpoint enters Stage 5G-c without raw settled escape");
        let truth_received = Utc
            .timestamp_millis_opt(generated_checkpoint_ms + 2_000)
            .single()
            .unwrap();
        let broker_order_id = BrokerOrderId::new("FINAM-TIMER-EXIT-1");
        let truth = BrokerTruthSnapshot {
            account_id: BrokerAccountId::new("ACC_TEST_0001"),
            orders: vec![BrokerOrderSnapshot {
                account_id: BrokerAccountId::new("ACC_TEST_0001"),
                broker_order_id: Some(broker_order_id.clone()),
                client_order_id: Some(client_order_id.clone()),
                instrument: target(),
                side: match side {
                    crate::BrokerNeutralOrderSide::Buy => OrderSide::Buy,
                    crate::BrokerNeutralOrderSide::Sell => OrderSide::Sell,
                },
                order_type: OrderType::Market,
                time_in_force: Some(TimeInForce::Day),
                lifecycle: BrokerOrderLifecycle::Terminal,
                status: OrderStatus::Filled,
                qty: Decimal::ONE,
                filled_qty: Decimal::ONE,
                remaining_qty: Some(Decimal::ZERO),
                limit_price: None,
                broker_asset_id: None,
                board: None,
                expiration_date: None,
                source_ts: Some(ack_received),
                received_ts: truth_received,
            }],
            positions: vec![BrokerPositionSnapshot {
                account_id: BrokerAccountId::new("ACC_TEST_0001"),
                instrument: target(),
                qty: Decimal::ZERO,
                avg_price: None,
                unrealized_pnl: Some(Decimal::ZERO),
                source_ts: Some(ack_received),
                received_ts: truth_received,
            }],
            cash: None,
            trades: vec![BrokerTradeSnapshot {
                account_id: BrokerAccountId::new("ACC_TEST_0001"),
                broker_trade_id: BrokerTradeId::new("FINAM-TIMER-EXIT-TRADE-1"),
                broker_order_id: Some(broker_order_id),
                client_order_id: Some(client_order_id),
                instrument: target(),
                side: match side {
                    crate::BrokerNeutralOrderSide::Buy => OrderSide::Buy,
                    crate::BrokerNeutralOrderSide::Sell => OrderSide::Sell,
                },
                qty: Decimal::ONE,
                price: Decimal::new(2_720, 0),
                gross_amount: None,
                commission: None,
                broker_asset_id: None,
                board: None,
                expiration_date: None,
                source_ts: ack_received,
                received_ts: truth_received,
            }],
            instruments: Vec::new(),
            received_ts: truth_received,
        };
        let next_converged = match crate::apply_stage5g_order_position_evidence(
            order_position,
            Stage5gOrderPositionEvidence {
                total_sequence: initial_checkpoint.last_total_sequence.unwrap() + 1,
                request_id,
                broker_truth: truth,
                order_attribution: projection.expected_attribution,
            },
        )
        .expect("timer Exit broker truth converges")
        {
            Stage5gOrderPositionTransition::Converged(converged) => converged,
            Stage5gOrderPositionTransition::Awaiting(session) => {
                order_position = session;
                panic!(
                    "terminal timer Exit must converge, summary={:?}",
                    order_position.summary()
                )
            }
            Stage5gOrderPositionTransition::MarketTerminalConverged(_) => {
                panic!("filled timer Exit is not a market-terminal exception")
            }
        };
        assert_eq!(
            next_converged.summary().stage5c_callback_count,
            initial_callback_count,
            "each one-package lifecycle invokes exactly one callback without duplication"
        );
        let next_session = crate::attach_stage5g_timer_session(next_converged);
        let next_checkpoint = next_session.checkpoint();
        assert_eq!(
            next_checkpoint
                .payload
                .last_continuation_checkpoint_ts_utc_ms,
            Some(truth_received.timestamp_millis())
        );
        assert_eq!(
            next_checkpoint.payload.evidence_replay_ledger.len(),
            initial_ledger.len() + 1
        );
        for previous in initial_ledger {
            assert!(next_checkpoint
                .payload
                .evidence_replay_ledger
                .iter()
                .any(|entry| entry.identity == previous.identity
                    && entry.fingerprint_sha256 == previous.fingerprint));
        }
        assert_eq!(
            next_checkpoint.payload.package_discriminator,
            Some(format!(
                "moex.broker-truth.package.v1:{}:{:09}",
                truth_received.timestamp(),
                truth_received.timestamp_subsec_nanos()
            ))
        );
        crate::validate_stage5g_timer_checkpoint(&next_checkpoint)
            .expect("next Stage 5G-d checkpoint is semantically restorable");
    }

    #[test]
    fn stage5gd_timer_generated_cleanup_roundtrips_through_ack_truth_and_next_session() {
        let initial_ledger_len = 0;
        let (settled, bar_close_ts) = stage5gd_bracket_seeded_exit_settled();
        let bar_checkpoint_ms = bar_close_ts * 1_000;
        let partial =
            settled_exit_to_order_position(settled, bar_checkpoint_ms, "FINAM-BAR-PARTIAL-EXIT-1");
        let working_received = Utc
            .timestamp_millis_opt(bar_checkpoint_ms + 2_500)
            .single()
            .unwrap();
        let partial_received = Utc
            .timestamp_millis_opt(bar_checkpoint_ms + 3_000)
            .single()
            .unwrap();
        let working_truth = generated_exit_truth(
            &partial,
            working_received,
            OrderStatus::PartiallyFilled,
            Decimal::ONE,
            Decimal::new(4, 1),
            Decimal::new(6, 1),
            "FINAM-BAR-PARTIAL-TRADE-1",
        );
        let partial_truth = generated_exit_truth(
            &partial,
            partial_received,
            OrderStatus::Canceled,
            Decimal::ONE,
            Decimal::new(4, 1),
            Decimal::new(6, 1),
            "FINAM-BAR-PARTIAL-TRADE-1",
        );
        let partial_request_id = partial.request_id;
        let partial_attribution = partial.projection.expected_attribution.clone();
        let partial_session = match crate::apply_stage5g_order_position_evidence(
            partial.session,
            Stage5gOrderPositionEvidence {
                total_sequence: 1,
                request_id: partial_request_id,
                broker_truth: working_truth,
                order_attribution: partial_attribution.clone(),
            },
        )
        .expect("working partial Exit remains awaiting")
        {
            Stage5gOrderPositionTransition::Awaiting(session) => session,
            Stage5gOrderPositionTransition::Converged(_)
            | Stage5gOrderPositionTransition::MarketTerminalConverged(_) => {
                panic!("working partial Exit cannot converge")
            }
        };
        let partial_terminal = match crate::apply_stage5g_order_position_evidence(
            partial_session,
            Stage5gOrderPositionEvidence {
                total_sequence: 2,
                request_id: partial_request_id,
                broker_truth: partial_truth,
                order_attribution: partial_attribution,
            },
        )
        .expect("partial Exit terminal evidence is accepted")
        {
            Stage5gOrderPositionTransition::MarketTerminalConverged(terminal) => terminal,
            Stage5gOrderPositionTransition::Converged(_) => {
                panic!("partial terminal Exit must retain timer reconciliation")
            }
            Stage5gOrderPositionTransition::Awaiting(_) => {
                panic!("canceled partial Exit is terminal")
            }
        };
        let partial_checkpoint = partial_terminal.replay_checkpoint.clone();
        assert_eq!(
            partial_checkpoint.last_continuation_checkpoint_ts_utc_ms,
            Some(partial_received.timestamp_millis())
        );
        let timer_ready = match crate::apply_stage5g_timer_checkpoint(
            crate::attach_stage5g_market_terminal_timer_session(partial_terminal),
            crate::Stage5cPaperTimerInput {
                now_ts_utc_ms: partial_received.timestamp_millis() + 1,
            },
        ) {
            Ok(crate::Stage5gTimerTransition::Ready(ready)) => ready,
            Ok(crate::Stage5gTimerTransition::GeneratedIntent(_)) => {
                panic!("cleanup must not fire inside reconciliation grace")
            }
            Err(failure) => {
                panic!(
                    "inside-grace partial Exit must become timer-ready: {:?}",
                    failure.reason()
                )
            }
        };
        let cleanup_checkpoint_ms = partial_received.timestamp_millis() + 4_900;
        let cleanup_escrow = match crate::continue_stage5g_timer_with_timer(
            timer_ready,
            crate::Stage5cPaperTimerInput {
                now_ts_utc_ms: cleanup_checkpoint_ms,
            },
        )
        .expect("explicit post-grace timer generates residual Exit")
        {
            crate::Stage5gTimerTransition::GeneratedIntent(escrow) => escrow,
            crate::Stage5gTimerTransition::Ready(_) => {
                panic!("post-grace timer must generate residual cleanup")
            }
        };
        assert_eq!(
            cleanup_escrow
                .checkpoint()
                .payload
                .last_continuation_checkpoint_ts_utc_ms,
            Some(cleanup_checkpoint_ms)
        );
        let cleanup = generated_escrow_to_order_position(
            cleanup_escrow,
            cleanup_checkpoint_ms,
            "FINAM-TIMER-CLEANUP-EXIT-2",
        );
        assert_eq!(cleanup.projection.target_qty, Some(0.6));
        let cleanup_qty = Decimal::from_f64_retain(
            cleanup
                .projection
                .target_qty
                .expect("timer cleanup source quantity"),
        )
        .expect("finite timer cleanup quantity");
        let cleanup_received = Utc
            .timestamp_millis_opt(cleanup_checkpoint_ms + 2_000)
            .single()
            .unwrap();
        let early_received = Utc
            .timestamp_millis_opt(cleanup_checkpoint_ms - 1)
            .single()
            .unwrap();
        let early_truth = generated_exit_truth(
            &cleanup,
            early_received,
            OrderStatus::Filled,
            cleanup_qty,
            cleanup_qty,
            Decimal::ZERO,
            "FINAM-TIMER-CLEANUP-TRADE-EARLY",
        );
        let cleanup_truth = generated_exit_truth(
            &cleanup,
            cleanup_received,
            OrderStatus::Filled,
            cleanup_qty,
            cleanup_qty,
            Decimal::ZERO,
            "FINAM-TIMER-CLEANUP-TRADE-2",
        );
        let cleanup_attribution = cleanup.projection.expected_attribution.clone();
        let blocked = match crate::apply_stage5g_order_position_evidence(
            cleanup.session,
            Stage5gOrderPositionEvidence {
                total_sequence: 3,
                request_id: cleanup.request_id,
                broker_truth: early_truth,
                order_attribution: cleanup_attribution.clone(),
            },
        ) {
            Err(blocked) => blocked,
            Ok(_) => panic!("BrokerTruth before the inherited timer checkpoint must block"),
        };
        assert_eq!(
            blocked.reason(),
            Stage5gOrderPositionError::BrokerTruthBeforeContinuationCheckpoint
        );
        let cleanup_session = blocked
            .into_blocked()
            .expect("continuation chronology is retryable")
            .into_session();
        let final_converged = match crate::apply_stage5g_order_position_evidence(
            cleanup_session,
            Stage5gOrderPositionEvidence {
                total_sequence: 3,
                request_id: cleanup.request_id,
                broker_truth: cleanup_truth,
                order_attribution: cleanup_attribution,
            },
        )
        .expect("timer cleanup broker truth converges flat")
        {
            Stage5gOrderPositionTransition::Converged(converged) => converged,
            Stage5gOrderPositionTransition::Awaiting(_) => {
                panic!("filled residual cleanup must converge")
            }
            Stage5gOrderPositionTransition::MarketTerminalConverged(_) => {
                panic!("filled residual cleanup is not exceptional")
            }
        };
        let final_checkpoint = crate::attach_stage5g_timer_session(final_converged).checkpoint();
        assert_eq!(
            final_checkpoint
                .payload
                .last_continuation_checkpoint_ts_utc_ms,
            Some(cleanup_received.timestamp_millis())
        );
        assert_eq!(
            final_checkpoint.payload.evidence_replay_ledger.len(),
            initial_ledger_len + 3
        );
        assert_eq!(final_checkpoint.payload.last_total_sequence, Some(3));
        crate::validate_stage5g_timer_checkpoint(&final_checkpoint)
            .expect("full bar/timer/ACK/truth route restores exactly");
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
                current_evidence_identity: None,
                last_total_sequence: None,
                last_broker_truth_received_at: None,
                last_broker_truth_received_ms: None,
                duplicate_evidence_count: 0,
                last_continuation_checkpoint_ts_utc_ms: None,
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
                current_evidence_identity: None,
                last_total_sequence: None,
                last_broker_truth_received_at: None,
                last_broker_truth_received_ms: None,
                duplicate_evidence_count: 0,
                last_continuation_checkpoint_ts_utc_ms: None,
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
    fn r2b_only_committed_correlated_trade_advances_slot_watermark() {
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
            Some(ts(9)),
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
        assert_eq!(slot.last_trade_source_ts, None);
        slot.trades
            .push(market_trade("MARKET_TRADE_1", Decimal::ONE, 10));
        refresh_trade_watermarks_from_committed_ledger(&mut slot);
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
            validate_snapshot_chronology(Some(ts(1)), &target(), &mut chronology_slot, &event,),
            Err(Stage5gOrderPositionError::ComponentTimeAfterSnapshot)
        );
        let event = evidence(3, truth(vec![], vec![], vec![], 1));
        assert_eq!(
            validate_snapshot_chronology(Some(ts(2)), &target(), &mut chronology_slot, &event,),
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
            validate_snapshot_chronology(Some(ts(2)), &target(), &mut order_slot, &regressed,),
            Err(Stage5gOrderPositionError::OrderTimeRegression)
        );

        let mut position_slot = slot();
        let first = evidence(6, truth(vec![], vec![], vec![position(Decimal::ONE, 2)], 2));
        validate_snapshot_chronology(None, &target(), &mut position_slot, &first).unwrap();
        let mut regressed_position = position(Decimal::ONE, 3);
        regressed_position.source_ts = Some(ts(1));
        let regressed = evidence(7, truth(vec![], vec![], vec![regressed_position], 3));
        assert_eq!(
            validate_snapshot_chronology(Some(ts(2)), &target(), &mut position_slot, &regressed,),
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
    // STAGE5G-C-R2CB-R1-THREE-POLL-RUNTIME-WITNESS-BEGIN
    #[test]
    fn r2cb_public_runtime_three_poll_golden_converges_through_stage5c() {
        let golden: serde_json::Value = serde_json::from_str(include_str!(
            "../../../fixtures/expected/stage5g_r2cb_three_poll_broker_truth.json"
        ))
        .expect("connector-neutral three-poll golden JSON");
        let polls = golden["polls"].as_array().expect("three golden polls");
        assert_eq!(polls.len(), 3);
        let (mut session, request_id, client_order_id, expected_attribution, time_shift) =
            r2cb_public_runtime_session();
        let mut fingerprints = Vec::new();

        for (index, poll) in polls.iter().take(2).enumerate() {
            let transition = apply_stage5g_order_position_evidence(
                session,
                Stage5gOrderPositionEvidence {
                    total_sequence: u64::try_from(index + 2).unwrap(),
                    request_id,
                    broker_truth: r2cb_golden_truth(poll, &client_order_id, time_shift),
                    order_attribution: expected_attribution.clone(),
                },
            )
            .expect("partial FINAM full-snapshot poll remains accepted");
            session = transition
                .into_awaiting()
                .expect("partial poll must not invoke the Stage 5C callback");
            let summary = session.summary();
            assert_eq!(summary.stage5c_callback_count, 0);
            assert_eq!(summary.correlated_trade_count, index + 1);
            fingerprints.push(summary.lifecycle_fingerprint_sha256);
        }
        assert_ne!(fingerprints[0], fingerprints[1]);

        let converged = apply_stage5g_order_position_evidence(
            session,
            Stage5gOrderPositionEvidence {
                total_sequence: 4,
                request_id,
                broker_truth: r2cb_golden_truth(&polls[2], &client_order_id, time_shift),
                order_attribution: expected_attribution,
            },
        )
        .expect("filled FINAM full-snapshot poll converges")
        .into_converged()
        .expect("filled Market order converges through the Stage 5C lifecycle");
        assert_eq!(converged.summary().stage5c_callback_count, 1);
        assert_eq!(converged.summary().terminal_request_count, 1);
        assert_eq!(converged.summary().correlated_trade_count, 3);
        assert_eq!(converged.summary().position_confirmation_count, 1);
        assert!(!converged.intent_sink_attached());
        assert!(!converged.redis_command_stream_attached());
        assert!(!converged.broker_transport_attached());
        assert!(!converged.broker_execution_attached());
    }

    #[test]
    fn r2cb_three_poll_full_snapshot_replay_refreshes_history_without_regression() {
        let account = BrokerAccountId::new("ACC_TEST_0001");
        let mut slot = market_slot(
            crate::BrokerNeutralHybridIntentClass::Entry,
            crate::BrokerNeutralOrderSide::Buy,
            1.0,
            0.0,
        );
        let poll1_qty = Decimal::new(2, 1);
        let poll1 = evidence(
            2,
            truth(
                vec![market_order(OrderStatus::PartiallyFilled, poll1_qty, 2)],
                vec![market_trade("FINAM_TRADE_A", poll1_qty, 2)],
                vec![position(poll1_qty, 2)],
                2,
            ),
        );
        validate_snapshot_chronology(None, &target(), &mut slot, &poll1).unwrap();
        apply_to_slot(&account, &target(), &mut slot, &poll1).unwrap();
        refresh_trade_watermarks_from_committed_ledger(&mut slot);
        let poll1_fingerprint = lifecycle_state_fingerprint(
            &Stage5gOrderPositionState {
                strategy_id: "hybrid_imoexf".to_string(),
                account_id: account.clone(),
                instrument: target(),
                slots: vec![slot.clone()],
                evidence_identities: vec![],
                current_evidence_identity: None,
                last_total_sequence: Some(2),
                last_broker_truth_received_at: Some(ts(2)),
                last_broker_truth_received_ms: Some(ts(2).timestamp_millis()),
                duplicate_evidence_count: 0,
                last_continuation_checkpoint_ts_utc_ms: Some(ts(2).timestamp_millis()),
            },
            0,
        );

        let poll2_qty = Decimal::new(4, 1);
        let mut repeated_a = market_trade("FINAM_TRADE_A", poll1_qty, 2);
        repeated_a.received_ts = ts(3);
        let poll2 = evidence(
            3,
            truth(
                vec![market_order(OrderStatus::PartiallyFilled, poll2_qty, 3)],
                vec![repeated_a, market_trade("FINAM_TRADE_B", poll1_qty, 3)],
                vec![position(poll2_qty, 3)],
                3,
            ),
        );
        validate_snapshot_chronology(Some(ts(2)), &target(), &mut slot, &poll2).unwrap();
        apply_to_slot(&account, &target(), &mut slot, &poll2).unwrap();
        refresh_trade_watermarks_from_committed_ledger(&mut slot);
        assert_eq!(slot.trades.len(), 2);
        assert_eq!(slot.trades[0].received_ts, ts(3));
        let poll2_fingerprint = lifecycle_state_fingerprint(
            &Stage5gOrderPositionState {
                strategy_id: "hybrid_imoexf".to_string(),
                account_id: account.clone(),
                instrument: target(),
                slots: vec![slot.clone()],
                evidence_identities: vec![],
                current_evidence_identity: None,
                last_total_sequence: Some(3),
                last_broker_truth_received_at: Some(ts(3)),
                last_broker_truth_received_ms: Some(ts(3).timestamp_millis()),
                duplicate_evidence_count: 0,
                last_continuation_checkpoint_ts_utc_ms: Some(ts(3).timestamp_millis()),
            },
            0,
        );
        assert_ne!(poll1_fingerprint, poll2_fingerprint);

        let mut repeated_a = market_trade("FINAM_TRADE_A", poll1_qty, 2);
        repeated_a.received_ts = ts(4);
        let mut repeated_b = market_trade("FINAM_TRADE_B", poll1_qty, 3);
        repeated_b.received_ts = ts(4);
        let poll3 = evidence(
            4,
            truth(
                vec![market_order(OrderStatus::Filled, Decimal::ONE, 4)],
                vec![
                    repeated_a,
                    repeated_b,
                    market_trade("FINAM_TRADE_C", Decimal::new(6, 1), 4),
                ],
                vec![position(Decimal::ONE, 4)],
                4,
            ),
        );
        validate_snapshot_chronology(Some(ts(3)), &target(), &mut slot, &poll3).unwrap();
        apply_to_slot(&account, &target(), &mut slot, &poll3).unwrap();
        refresh_trade_watermarks_from_committed_ledger(&mut slot);
        assert!(slot.terminal);
        assert_eq!(slot.trades.len(), 3);
        assert!(slot.trades.iter().all(|trade| trade.received_ts == ts(4)));
        assert_eq!(
            slot.trades.iter().map(|trade| trade.qty).sum::<Decimal>(),
            Decimal::ONE
        );
    }

    #[test]
    fn r2cb_known_trade_refresh_and_unseen_late_trade_have_distinct_chronology() {
        let account = BrokerAccountId::new("ACC_TEST_0001");
        let mut slot = market_slot(
            crate::BrokerNeutralHybridIntentClass::Entry,
            crate::BrokerNeutralOrderSide::Buy,
            1.0,
            0.0,
        );
        let qty = Decimal::new(2, 1);
        let first = evidence(
            2,
            truth(
                vec![market_order(OrderStatus::PartiallyFilled, qty, 2)],
                vec![market_trade("FINAM_TRADE_A", qty, 2)],
                vec![position(qty, 2)],
                2,
            ),
        );
        validate_snapshot_chronology(None, &target(), &mut slot, &first).unwrap();
        apply_to_slot(&account, &target(), &mut slot, &first).unwrap();
        refresh_trade_watermarks_from_committed_ledger(&mut slot);

        let mut known_a = market_trade("FINAM_TRADE_A", qty, 2);
        known_a.received_ts = ts(3);
        let second = evidence(
            3,
            truth(
                vec![market_order(
                    OrderStatus::PartiallyFilled,
                    Decimal::new(4, 1),
                    3,
                )],
                vec![known_a, market_trade("FINAM_TRADE_B", qty, 3)],
                vec![position(Decimal::new(4, 1), 3)],
                3,
            ),
        );
        validate_snapshot_chronology(Some(ts(2)), &target(), &mut slot, &second).unwrap();
        apply_to_slot(&account, &target(), &mut slot, &second).unwrap();
        refresh_trade_watermarks_from_committed_ledger(&mut slot);

        let mut refreshed_a = market_trade("FINAM_TRADE_A", qty, 2);
        refreshed_a.received_ts = ts(4);
        let stable = evidence(
            4,
            truth(
                vec![market_order(
                    OrderStatus::PartiallyFilled,
                    Decimal::new(4, 1),
                    4,
                )],
                vec![refreshed_a.clone(), {
                    let mut trade = market_trade("FINAM_TRADE_B", qty, 3);
                    trade.received_ts = ts(4);
                    trade
                }],
                vec![position(Decimal::new(4, 1), 4)],
                4,
            ),
        );
        validate_snapshot_chronology(Some(ts(3)), &target(), &mut slot, &stable).unwrap();
        apply_to_slot(&account, &target(), &mut slot, &stable).unwrap();
        refresh_trade_watermarks_from_committed_ledger(&mut slot);
        assert_eq!(slot.trades.len(), 2);
        assert!(slot.trades.iter().all(|trade| trade.received_ts == ts(4)));

        let mut earlier_a = refreshed_a.clone();
        earlier_a.received_ts = ts(3);
        assert_eq!(
            validate_snapshot_chronology(
                Some(ts(4)),
                &target(),
                &mut slot.clone(),
                &evidence(5, truth(vec![], vec![earlier_a], vec![], 5)),
            ),
            Err(Stage5gOrderPositionError::TradeTimeRegression)
        );

        let mut conflicting_a = refreshed_a;
        conflicting_a.received_ts = ts(5);
        conflicting_a.price += Decimal::new(5, 1);
        assert_eq!(
            validate_snapshot_chronology(
                Some(ts(4)),
                &target(),
                &mut slot.clone(),
                &evidence(6, truth(vec![], vec![conflicting_a], vec![], 5)),
            ),
            Err(Stage5gOrderPositionError::TradeIdentityConflict)
        );

        let mut unseen_late = market_trade("FINAM_TRADE_LATE", qty, 1);
        unseen_late.received_ts = ts(5);
        assert_eq!(
            validate_snapshot_chronology(
                Some(ts(4)),
                &target(),
                &mut slot,
                &evidence(7, truth(vec![], vec![unseen_late], vec![], 5)),
            ),
            Err(Stage5gOrderPositionError::TradeTimeRegression)
        );
    }
    // STAGE5G-C-R2CB-R1-THREE-POLL-RUNTIME-WITNESS-END

    // STAGE5G-C-R2CB-R2-LEDGER-WATERMARK-WITNESSES-BEGIN
    #[test]
    fn r2cb_r2_subset_refresh_preserves_committed_max_and_blocks_unseen_late_trade() {
        let golden: serde_json::Value = serde_json::from_str(include_str!(
            "../../../fixtures/expected/stage5g_r2cb_three_poll_broker_truth.json"
        ))
        .unwrap();
        let polls = golden["polls"].as_array().unwrap();
        let (mut session, request_id, client_order_id, attribution, time_shift) =
            r2cb_public_runtime_session();

        for (sequence, poll) in [(2, &polls[0]), (3, &polls[1])] {
            session = apply_stage5g_order_position_evidence(
                session,
                Stage5gOrderPositionEvidence {
                    total_sequence: sequence,
                    request_id,
                    broker_truth: r2cb_golden_truth(poll, &client_order_id, time_shift),
                    order_attribution: attribution.clone(),
                },
            )
            .unwrap()
            .into_awaiting()
            .unwrap();
        }
        let before_subset = session.state.slots[0].clone();
        let source_a = before_subset.trades[0].source_ts;
        let source_b = before_subset.trades[1].source_ts;
        assert!(source_a < source_b);
        assert_eq!(before_subset.last_trade_source_ts, Some(source_b));

        let mut subset = r2cb_golden_truth(&polls[1], &client_order_id, time_shift);
        let subset_receipt = subset.received_ts + Duration::seconds(1);
        subset.received_ts = subset_receipt;
        subset.orders[0].source_ts = Some(subset_receipt - Duration::milliseconds(50));
        subset.orders[0].received_ts = subset_receipt;
        subset.positions.iter_mut().for_each(|position| {
            position.received_ts = subset_receipt;
        });
        subset.trades.retain(|trade| trade.source_ts == source_a);
        subset.trades[0].received_ts = subset_receipt;
        session = apply_stage5g_order_position_evidence(
            session,
            Stage5gOrderPositionEvidence {
                total_sequence: 4,
                request_id,
                broker_truth: subset,
                order_attribution: attribution.clone(),
            },
        )
        .expect("known-only coherent subset refresh is accepted")
        .into_awaiting()
        .expect("partial order remains awaiting");
        let after_subset = &session.state.slots[0];
        assert_eq!(after_subset.trades.len(), 2);
        assert_eq!(after_subset.last_trade_source_ts, Some(source_b));
        assert_eq!(after_subset.last_trade_received_ts, Some(subset_receipt));
        assert!(component_watermarks_are_monotonic(
            &before_subset,
            after_subset
        ));
        let retained_fingerprint = session.summary().lifecycle_fingerprint_sha256;

        let mut late = r2cb_golden_truth(&polls[1], &client_order_id, time_shift);
        let late_receipt = subset_receipt + Duration::seconds(1);
        late.received_ts = late_receipt;
        late.orders[0].filled_qty = Decimal::new(6, 1);
        late.orders[0].remaining_qty = Some(Decimal::new(4, 1));
        late.orders[0].source_ts = Some(late_receipt - Duration::milliseconds(50));
        late.orders[0].received_ts = late_receipt;
        late.positions = vec![BrokerPositionSnapshot {
            account_id: BrokerAccountId::new("ACC_TEST_0001"),
            instrument: target(),
            qty: Decimal::new(6, 1),
            avg_price: Some(Decimal::new(2_210, 0)),
            unrealized_pnl: None,
            source_ts: None,
            received_ts: late_receipt,
        }];
        let mut unseen_c = late.trades[0].clone();
        unseen_c.broker_trade_id = BrokerTradeId::new("FINAM-R2CB-TRADE-LATE-C");
        unseen_c.source_ts = source_a + Duration::milliseconds(200);
        unseen_c.received_ts = late_receipt;
        assert!(unseen_c.source_ts < source_b);
        late.trades = vec![unseen_c];
        let blocked = match apply_stage5g_order_position_evidence(
            session,
            Stage5gOrderPositionEvidence {
                total_sequence: 5,
                request_id,
                broker_truth: late,
                order_attribution: attribution,
            },
        ) {
            Err(failure) => failure.into_blocked().unwrap(),
            Ok(_) => panic!("unseen late trade must remain fail closed"),
        };
        assert_eq!(
            blocked.reason(),
            Stage5gOrderPositionError::TradeTimeRegression
        );
        assert_eq!(
            blocked.session().summary().lifecycle_fingerprint_sha256,
            retained_fingerprint
        );
    }

    #[test]
    fn r2cb_r2_known_receipt_between_trade_and_global_max_preserves_global_max() {
        let account = BrokerAccountId::new("ACC_TEST_0001");
        let qty = Decimal::new(2, 1);
        let mut slot = market_slot(
            crate::BrokerNeutralHybridIntentClass::Entry,
            crate::BrokerNeutralOrderSide::Buy,
            1.0,
            0.0,
        );
        let mut trade_a = market_trade("FINAM_TRADE_A", qty, 2);
        trade_a.received_ts = ts(2);
        let mut trade_b = market_trade("FINAM_TRADE_B", qty, 4);
        trade_b.received_ts = ts(5);
        slot.trades = vec![trade_a.clone(), trade_b];
        refresh_trade_watermarks_from_committed_ledger(&mut slot);
        assert_eq!(slot.last_trade_received_ts, Some(ts(5)));

        trade_a.received_ts = ts(3);
        let event = evidence(
            2,
            truth(
                vec![market_order(
                    OrderStatus::PartiallyFilled,
                    Decimal::new(4, 1),
                    6,
                )],
                vec![trade_a],
                vec![position(Decimal::new(4, 1), 6)],
                6,
            ),
        );
        validate_snapshot_chronology(None, &target(), &mut slot, &event).unwrap();
        apply_to_slot(&account, &target(), &mut slot, &event).unwrap();
        refresh_trade_watermarks_from_committed_ledger(&mut slot);
        assert_eq!(slot.last_trade_received_ts, Some(ts(5)));
        assert_eq!(slot.trades.len(), 2);
    }

    #[test]
    fn r2cb_r2_position_only_trades_block_transactionally_then_order_snapshot_converges() {
        let golden: serde_json::Value = serde_json::from_str(include_str!(
            "../../../fixtures/expected/stage5g_r2cb_three_poll_broker_truth.json"
        ))
        .unwrap();
        let polls = golden["polls"].as_array().unwrap();
        let (session, request_id, client_order_id, attribution, time_shift) =
            r2cb_public_runtime_session();
        let before = session.summary().lifecycle_fingerprint_sha256;
        let mut position_only = r2cb_golden_truth(&polls[1], &client_order_id, time_shift);
        position_only.orders.clear();
        let blocked = match apply_stage5g_order_position_evidence(
            session,
            Stage5gOrderPositionEvidence {
                total_sequence: 2,
                request_id,
                broker_truth: position_only,
                order_attribution: attribution.clone(),
            },
        ) {
            Err(failure) => failure.into_blocked().unwrap(),
            Ok(_) => panic!("position-only target trades require the target order row"),
        };
        assert_eq!(
            blocked.reason(),
            Stage5gOrderPositionError::TargetTradeWithoutOrder
        );
        assert_eq!(
            blocked.session().summary().lifecycle_fingerprint_sha256,
            before
        );
        assert!(blocked.session().state.slots[0].trades.is_empty());
        assert_eq!(blocked.session().state.slots[0].last_trade_source_ts, None);
        assert_eq!(blocked.session().state.slots[0].position, None);
        assert_eq!(blocked.session().summary().stage5c_callback_count, 0);

        let converged = apply_stage5g_order_position_evidence(
            blocked.into_session(),
            Stage5gOrderPositionEvidence {
                total_sequence: 3,
                request_id,
                broker_truth: r2cb_golden_truth(&polls[2], &client_order_id, time_shift),
                order_attribution: attribution,
            },
        )
        .expect("coherent order-bearing retry converges")
        .into_converged()
        .unwrap();
        assert_eq!(converged.summary().correlated_trade_count, 3);
        assert_eq!(converged.summary().stage5c_callback_count, 1);
    }

    #[test]
    fn r2cb_r2_position_only_contradictory_target_trades_are_never_ignored() {
        let golden: serde_json::Value = serde_json::from_str(include_str!(
            "../../../fixtures/expected/stage5g_r2cb_three_poll_broker_truth.json"
        ))
        .unwrap();
        let poll = &golden["polls"].as_array().unwrap()[0];
        for mutation in 0..6 {
            let (session, request_id, client_order_id, attribution, time_shift) =
                r2cb_public_runtime_session();
            let mut truth = r2cb_golden_truth(poll, &client_order_id, time_shift);
            truth.orders.clear();
            let trade = &mut truth.trades[0];
            match mutation {
                0 => {
                    trade.client_order_id = Some(ClientOrderId::new("CONFLICTINGCLIENT01").unwrap())
                }
                1 => trade.qty = Decimal::ZERO,
                2 => trade.side = OrderSide::Sell,
                3 => trade.instrument = other(),
                4 => trade.account_id = BrokerAccountId::new("ACC_WRONG_0001"),
                5 => trade.price += Decimal::ONE,
                _ => unreachable!(),
            }
            let blocked = match apply_stage5g_order_position_evidence(
                session,
                Stage5gOrderPositionEvidence {
                    total_sequence: 2,
                    request_id,
                    broker_truth: truth,
                    order_attribution: attribution,
                },
            ) {
                Err(failure) => failure.into_blocked().unwrap(),
                Ok(_) => panic!("contradictory position-only target trade was ignored"),
            };
            assert_eq!(
                blocked.reason(),
                Stage5gOrderPositionError::TargetTradeWithoutOrder
            );
            assert!(blocked.session().state.slots[0].trades.is_empty());
        }
    }

    #[test]
    fn r2cb_r2_terminal_slot_rejects_target_trade_without_order() {
        let mut terminal = market_slot(
            crate::BrokerNeutralHybridIntentClass::Entry,
            crate::BrokerNeutralOrderSide::Buy,
            1.0,
            0.0,
        );
        terminal.terminal = true;
        assert_eq!(
            apply_to_slot(
                &BrokerAccountId::new("ACC_TEST_0001"),
                &target(),
                &mut terminal,
                &evidence(
                    2,
                    truth(
                        vec![],
                        vec![market_trade("FINAM_TERMINAL_TRADE", Decimal::ONE, 2)],
                        vec![],
                        2,
                    ),
                ),
            ),
            Err(Stage5gOrderPositionError::BrokerEvidenceAfterTerminalAck)
        );
    }
    // STAGE5G-C-R2CB-R2-LEDGER-WATERMARK-WITNESSES-END

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
    fn stage5gd_r4_exact_duplicate_merge_is_order_independent_and_keeps_max_receipt() {
        let first = trade("FINAM_R4_EXACT", Decimal::ONE, 2);
        let mut refreshed = first.clone();
        refreshed.received_ts = ts(3);
        let mut canonical_rows = Vec::new();
        let mut fingerprints = Vec::new();

        for rows in [
            vec![first.clone(), refreshed.clone()],
            vec![refreshed.clone(), first.clone()],
        ] {
            let raw = evidence(2, truth(vec![], rows, vec![], 3));
            let canonical = canonicalize_stage5g_order_position_evidence(raw).unwrap();
            assert_eq!(canonical.evidence().broker_truth.trades.len(), 1);
            assert_eq!(
                canonical.evidence().broker_truth.trades[0].received_ts,
                ts(3)
            );
            canonical_rows.push(canonical.evidence().broker_truth.trades[0].clone());
            fingerprints.push(canonical.fingerprint().to_string());
        }

        assert_eq!(canonical_rows[0], canonical_rows[1]);
        assert_eq!(fingerprints[0], fingerprints[1]);
    }

    #[test]
    fn stage5gd_r4_optional_venue_permutations_fail_closed_without_first_row_authority() {
        let with_venue = trade("FINAM_R4_VENUE_OPTION", Decimal::ONE, 2);
        let mut without_venue = with_venue.clone();
        without_venue.instrument.venue_symbol = None;
        without_venue.received_ts = ts(3);

        for rows in [
            vec![with_venue.clone(), without_venue.clone()],
            vec![without_venue.clone(), with_venue.clone()],
        ] {
            let raw = evidence(2, truth(vec![], rows, vec![], 3));
            assert_eq!(
                canonicalize_stage5g_order_position_evidence(raw).unwrap_err(),
                Stage5gEvidenceCanonicalizationError::TradeIdentityConflict
            );
        }
    }

    #[test]
    fn stage5gd_r4_same_venue_conflicting_instrument_fields_fail_closed() {
        let canonical = trade("FINAM_R4_VENUE_CONFLICT", Decimal::ONE, 2);
        let mut contradictory = canonical.clone();
        contradictory.instrument.symbol = "CONTRADICTORY".to_string();
        contradictory.instrument.exchange = Exchange::Other("CONTRADICTORY".to_string());
        contradictory.instrument.market = Market::Stocks;
        contradictory.received_ts = ts(3);
        assert_eq!(
            canonical.instrument.venue_symbol,
            contradictory.instrument.venue_symbol
        );

        for rows in [
            vec![canonical.clone(), contradictory.clone()],
            vec![contradictory.clone(), canonical.clone()],
        ] {
            let raw = evidence(2, truth(vec![], rows, vec![], 3));
            assert_eq!(
                canonicalize_stage5g_order_position_evidence(raw).unwrap_err(),
                Stage5gEvidenceCanonicalizationError::TradeIdentityConflict
            );
        }
    }

    #[test]
    fn stage5gd_r4_committed_trade_ledger_uses_exact_instrument_projection() {
        let qty = Decimal::new(4, 1);
        let filled = market_order(OrderStatus::PartiallyFilled, qty, 2);
        let committed = market_trade("FINAM_R4_COMMITTED", qty, 2);
        let mut slot = market_slot(
            crate::BrokerNeutralHybridIntentClass::Entry,
            crate::BrokerNeutralOrderSide::Buy,
            1.0,
            0.0,
        );
        validate_trades(&mut slot, &filled, std::slice::from_ref(&committed)).unwrap();
        let committed_ledger = slot.trades.clone();

        let mut missing_venue = committed.clone();
        missing_venue.instrument.venue_symbol = None;
        missing_venue.received_ts = ts(3);
        assert_eq!(
            validate_trades(&mut slot, &filled, &[missing_venue]),
            Err(Stage5gOrderPositionError::TradeIdentityConflict)
        );
        assert_eq!(slot.trades, committed_ledger);

        let mut contradictory = committed;
        contradictory.instrument.symbol = "CONTRADICTORY".to_string();
        contradictory.instrument.exchange = Exchange::Other("CONTRADICTORY".to_string());
        contradictory.instrument.market = Market::Stocks;
        contradictory.received_ts = ts(3);
        assert_eq!(
            validate_trades(&mut slot, &filled, &[contradictory]),
            Err(Stage5gOrderPositionError::TradeIdentityConflict)
        );
        assert_eq!(slot.trades, committed_ledger);
    }

    #[test]
    fn stage5gd_r5_qty_scale_permutations_fail_closed_under_exact_decimal_policy() {
        let qty_1_0 = Decimal::new(10, 1);
        let qty_1_00 = Decimal::new(100, 2);
        assert_eq!(qty_1_0, qty_1_00);
        assert_ne!(
            canonical_decimal_v1(qty_1_0),
            canonical_decimal_v1(qty_1_00)
        );
        let first = trade("FINAM_R5_QTY_SCALE", qty_1_0, 2);
        let mut changed_scale = first.clone();
        changed_scale.qty = qty_1_00;

        for rows in [
            vec![first.clone(), changed_scale.clone()],
            vec![changed_scale.clone(), first.clone()],
        ] {
            assert_eq!(
                canonicalize_stage5g_order_position_evidence(evidence(
                    2,
                    truth(vec![], rows, vec![], 2),
                ))
                .unwrap_err(),
                Stage5gEvidenceCanonicalizationError::TradeIdentityConflict
            );
        }
    }

    #[test]
    fn stage5gd_r5_price_and_optional_amount_scale_drift_fail_closed() {
        let mut first = trade("FINAM_R5_AMOUNT_SCALE", Decimal::ONE, 2);
        first.price = Decimal::new(1_000, 1);
        first.gross_amount = Some(Decimal::new(1_000, 1));
        first.commission = Some(Decimal::new(10, 1));

        let mut variants = Vec::new();
        let mut price_scale = first.clone();
        price_scale.price = Decimal::new(10_000, 2);
        variants.push(price_scale);
        let mut gross_scale = first.clone();
        gross_scale.gross_amount = Some(Decimal::new(10_000, 2));
        variants.push(gross_scale);
        let mut commission_scale = first.clone();
        commission_scale.commission = Some(Decimal::new(100, 2));
        variants.push(commission_scale);

        for changed_scale in variants {
            assert_eq!(
                canonicalize_stage5g_order_position_evidence(evidence(
                    2,
                    truth(
                        vec![],
                        vec![first.clone(), changed_scale.clone()],
                        vec![],
                        2,
                    ),
                ))
                .unwrap_err(),
                Stage5gEvidenceCanonicalizationError::TradeIdentityConflict
            );
            assert_eq!(
                canonicalize_stage5g_order_position_evidence(evidence(
                    2,
                    truth(vec![], vec![changed_scale, first.clone()], vec![], 2),
                ))
                .unwrap_err(),
                Stage5gEvidenceCanonicalizationError::TradeIdentityConflict
            );
        }
    }

    #[test]
    fn stage5gd_r5_signed_zero_representation_is_explicit_and_fail_closed() {
        let positive_zero = Decimal::from_parts(0, 0, 0, false, 2);
        assert_eq!(
            positive_zero,
            Decimal::from_parts(0, 0, 0, true, 2),
            "the normal constructor canonicalizes zero sign"
        );
        let mut negative_zero_bytes = positive_zero.serialize();
        negative_zero_bytes[3] |= 0x80;
        let negative_zero = Decimal::deserialize(negative_zero_bytes);
        assert_eq!(positive_zero, negative_zero);
        assert_ne!(
            canonical_decimal_v1(positive_zero),
            canonical_decimal_v1(negative_zero)
        );
        let mut positive = trade("FINAM_R5_SIGNED_ZERO", Decimal::ONE, 2);
        positive.commission = Some(positive_zero);
        let mut negative = positive.clone();
        negative.commission = Some(negative_zero);
        assert_eq!(
            canonicalize_stage5g_order_position_evidence(evidence(
                2,
                truth(vec![], vec![positive, negative], vec![], 2),
            ))
            .unwrap_err(),
            Stage5gEvidenceCanonicalizationError::TradeIdentityConflict
        );
    }

    #[test]
    fn stage5gd_r5_exact_decimal_rows_merge_deterministically_at_equal_and_later_receipts() {
        let first = trade("FINAM_R5_EXACT_DECIMAL", Decimal::new(100, 2), 2);
        let exact_equal_receipt = first.clone();
        let mut later = first.clone();
        later.received_ts = ts(3);

        for rows in [
            vec![first.clone(), exact_equal_receipt],
            vec![first.clone(), later.clone()],
            vec![later.clone(), first.clone()],
        ] {
            let expected_receipt = rows.iter().map(|trade| trade.received_ts).max().unwrap();
            let canonical = canonicalize_stage5g_order_position_evidence(evidence(
                2,
                truth(vec![], rows, vec![], 3),
            ))
            .unwrap();
            assert_eq!(canonical.evidence().broker_truth.trades.len(), 1);
            assert_eq!(
                canonical.evidence().broker_truth.trades[0].received_ts,
                expected_receipt
            );
            assert_eq!(
                canonical.evidence().broker_truth.trades[0].qty.serialize(),
                Decimal::new(100, 2).serialize()
            );
        }
    }

    #[test]
    fn stage5gd_r5_committed_trade_ledger_uses_exact_decimal_authority() {
        let qty = Decimal::new(4, 1);
        let filled = market_order(OrderStatus::PartiallyFilled, qty, 2);
        let mut committed = market_trade("FINAM_R5_COMMITTED_DECIMAL", qty, 2);
        committed.gross_amount = Some(Decimal::new(1_000, 1));
        let mut slot = market_slot(
            crate::BrokerNeutralHybridIntentClass::Entry,
            crate::BrokerNeutralOrderSide::Buy,
            1.0,
            0.0,
        );
        validate_trades(&mut slot, &filled, std::slice::from_ref(&committed)).unwrap();
        let committed_ledger = slot.trades.clone();

        let mut changed_qty_scale = committed.clone();
        changed_qty_scale.qty = Decimal::new(40, 2);
        changed_qty_scale.received_ts = ts(3);
        assert_eq!(
            validate_trades(&mut slot, &filled, &[changed_qty_scale]),
            Err(Stage5gOrderPositionError::TradeIdentityConflict)
        );
        assert_eq!(slot.trades, committed_ledger);

        let mut changed_gross_scale = committed;
        changed_gross_scale.gross_amount = Some(Decimal::new(10_000, 2));
        changed_gross_scale.received_ts = ts(3);
        assert_eq!(
            validate_trades(&mut slot, &filled, &[changed_gross_scale]),
            Err(Stage5gOrderPositionError::TradeIdentityConflict)
        );
        assert_eq!(slot.trades, committed_ledger);
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

    #[test]
    fn stage5gd_active_path_stores_single_authority_canonical_fingerprint() {
        let golden: serde_json::Value = serde_json::from_str(include_str!(
            "../../../fixtures/expected/stage5g_r2cb_three_poll_broker_truth.json"
        ))
        .unwrap();
        let poll = &golden["polls"].as_array().unwrap()[0];
        let (session, request_id, client_order_id, attribution, time_shift) =
            r2cb_public_runtime_session();
        let mut truth = r2cb_golden_truth(poll, &client_order_id, time_shift);
        truth.trades.push(truth.trades[0].clone());
        let raw = Stage5gOrderPositionEvidence {
            total_sequence: 2,
            request_id,
            broker_truth: truth,
            order_attribution: attribution,
        };
        let canonical = canonicalize_stage5g_order_position_evidence(raw.clone()).unwrap();
        assert_eq!(canonical.evidence().broker_truth.trades.len(), 1);
        let expected_fingerprint = canonical.fingerprint().to_string();

        let awaiting = apply_stage5g_order_position_evidence(session, raw)
            .expect("active path accepts exact same-snapshot trade duplicates")
            .into_awaiting()
            .expect("first partial FINAM poll remains awaiting");
        assert_eq!(awaiting.state.evidence_identities.len(), 1);
        assert_eq!(
            awaiting.state.evidence_identities[0].fingerprint,
            expected_fingerprint
        );
        assert_eq!(awaiting.state.slots[0].trades.len(), 1);
    }

    #[test]
    fn stage5gd_active_path_rejects_conflicting_trade_identity_before_replay_append() {
        let golden: serde_json::Value = serde_json::from_str(include_str!(
            "../../../fixtures/expected/stage5g_r2cb_three_poll_broker_truth.json"
        ))
        .unwrap();
        let poll = &golden["polls"].as_array().unwrap()[0];
        let (session, request_id, client_order_id, attribution, time_shift) =
            r2cb_public_runtime_session();
        let mut truth = r2cb_golden_truth(poll, &client_order_id, time_shift);
        let mut conflicting = truth.trades[0].clone();
        conflicting.price += Decimal::ONE;
        truth.trades.push(conflicting);
        let blocked = match apply_stage5g_order_position_evidence(
            session,
            Stage5gOrderPositionEvidence {
                total_sequence: 2,
                request_id,
                broker_truth: truth,
                order_attribution: attribution,
            },
        ) {
            Err(failure) => failure.into_blocked().unwrap(),
            Ok(_) => panic!("conflicting trade identity must fail before replay append"),
        };
        assert_eq!(
            blocked.reason(),
            Stage5gOrderPositionError::TradeIdentityConflict
        );
        assert!(blocked.session().state.evidence_identities.is_empty());
        assert_eq!(blocked.session().state.last_total_sequence, None);
    }

    // STAGE5G-C-REPLAY-PACKAGE-IDENTITY-WITNESSES-BEGIN
    #[test]
    fn replay_package_exact_replay_and_restart_identity_are_stable() {
        let original = evidence(
            2,
            truth(
                vec![order(OrderStatus::Working, Decimal::ZERO, 2)],
                vec![],
                vec![],
                2,
            ),
        );
        let state = state_with_evidence(&original);
        let restarted_state = state.clone();
        assert_eq!(
            classify_evidence_replay(
                &restarted_state,
                &evidence_identity(&original),
                &evidence_fingerprint(&original),
            ),
            Ok(true)
        );
        assert_eq!(
            broker_truth_package_discriminator(&original.broker_truth),
            broker_truth_package_discriminator(&original.broker_truth.clone())
        );
    }

    #[test]
    fn replay_package_same_source_identity_with_changed_payload_fails_closed() {
        let original = evidence(
            2,
            truth(
                vec![order(OrderStatus::Working, Decimal::ZERO, 2)],
                vec![],
                vec![],
                2,
            ),
        );
        let state = state_with_evidence(&original);
        let mut changed = original.clone();
        changed.broker_truth.orders[0].status = OrderStatus::PartiallyFilled;
        changed.broker_truth.orders[0].filled_qty = Decimal::new(4, 1);
        assert_eq!(evidence_identity(&original), evidence_identity(&changed));
        assert_ne!(
            evidence_fingerprint(&original),
            evidence_fingerprint(&changed)
        );
        assert_eq!(
            classify_evidence_replay(
                &state,
                &evidence_identity(&changed),
                &evidence_fingerprint(&changed),
            ),
            Err(Stage5gOrderPositionError::ConflictingDuplicateEvidence)
        );
    }

    #[test]
    fn replay_package_two_distinct_same_millisecond_packages_are_both_accepted() {
        let golden: serde_json::Value = serde_json::from_str(include_str!(
            "../../../fixtures/expected/stage5g_r2cb_three_poll_broker_truth.json"
        ))
        .unwrap();
        let polls = golden["polls"].as_array().unwrap();
        let (mut session, request_id, client_order_id, attribution, time_shift) =
            r2cb_public_runtime_session();
        let mut first = r2cb_golden_truth(&polls[0], &client_order_id, time_shift);
        let whole_second = first.received_ts.timestamp();
        let first_receipt = Utc
            .timestamp_opt(whole_second, 123_000_100)
            .single()
            .unwrap();
        let second_receipt = Utc
            .timestamp_opt(whole_second, 123_000_900)
            .single()
            .unwrap();
        assert_eq!(
            first_receipt.timestamp_millis(),
            second_receipt.timestamp_millis()
        );
        first.received_ts = first_receipt;
        first.orders[0].source_ts = Some(first_receipt - Duration::nanoseconds(200));
        first.orders[0].received_ts = first_receipt;
        first.positions.iter_mut().for_each(|position| {
            position.source_ts = Some(first_receipt - Duration::nanoseconds(300));
            position.received_ts = first_receipt;
        });
        first.trades[0].source_ts = first_receipt - Duration::nanoseconds(100);
        first.trades[0].received_ts = first_receipt;
        let first_identity = evidence_identity(&Stage5gOrderPositionEvidence {
            total_sequence: 2,
            request_id,
            broker_truth: first.clone(),
            order_attribution: attribution.clone(),
        });
        session = apply_stage5g_order_position_evidence(
            session,
            Stage5gOrderPositionEvidence {
                total_sequence: 2,
                request_id,
                broker_truth: first.clone(),
                order_attribution: attribution.clone(),
            },
        )
        .unwrap()
        .into_awaiting()
        .unwrap();

        session = apply_stage5g_order_position_evidence(
            session,
            Stage5gOrderPositionEvidence {
                total_sequence: 3,
                request_id,
                broker_truth: first,
                order_attribution: attribution.clone(),
            },
        )
        .expect("exact package replay is idempotent")
        .into_awaiting()
        .unwrap();
        assert_eq!(session.summary().duplicate_evidence_count, 1);
        assert_eq!(session.summary().correlated_trade_count, 1);

        let mut second = r2cb_golden_truth(&polls[1], &client_order_id, time_shift);
        second.received_ts = second_receipt;
        second.orders[0].source_ts = Some(second_receipt - Duration::nanoseconds(50));
        second.orders[0].received_ts = second_receipt;
        second.positions.iter_mut().for_each(|position| {
            position.source_ts = Some(second_receipt - Duration::nanoseconds(300));
            position.received_ts = second_receipt;
        });
        second.trades[0].source_ts = first_receipt - Duration::nanoseconds(100);
        second.trades[0].received_ts = second_receipt;
        second.trades[1].source_ts = second_receipt - Duration::nanoseconds(100);
        second.trades[1].received_ts = second_receipt;
        assert!(second.orders.iter().all(|order| {
            order.received_ts <= second.received_ts
                && order
                    .source_ts
                    .is_none_or(|source| source <= order.received_ts)
        }));
        assert!(second.positions.iter().all(|position| {
            position.received_ts <= second.received_ts
                && position
                    .source_ts
                    .is_none_or(|source| source <= position.received_ts)
        }));
        assert!(second.trades.iter().all(|trade| {
            trade.received_ts <= second.received_ts && trade.source_ts <= trade.received_ts
        }));
        let second_identity = evidence_identity(&Stage5gOrderPositionEvidence {
            total_sequence: 4,
            request_id,
            broker_truth: second.clone(),
            order_attribution: attribution.clone(),
        });
        assert_ne!(first_identity, second_identity);
        session = apply_stage5g_order_position_evidence(
            session,
            Stage5gOrderPositionEvidence {
                total_sequence: 4,
                request_id,
                broker_truth: second.clone(),
                order_attribution: attribution.clone(),
            },
        )
        .expect("second legitimate package in the same millisecond is accepted")
        .into_awaiting()
        .unwrap();
        assert_eq!(session.summary().correlated_trade_count, 2);

        let before_reverse = session.summary().lifecycle_fingerprint_sha256;
        let reversed_receipt = Utc
            .timestamp_opt(whole_second, 123_000_500)
            .single()
            .unwrap();
        second.received_ts = reversed_receipt;
        second.orders[0].received_ts = reversed_receipt;
        second.positions[0].received_ts = reversed_receipt;
        second
            .trades
            .iter_mut()
            .for_each(|trade| trade.received_ts = reversed_receipt);
        let blocked = match apply_stage5g_order_position_evidence(
            session,
            Stage5gOrderPositionEvidence {
                total_sequence: 5,
                request_id,
                broker_truth: second,
                order_attribution: attribution,
            },
        ) {
            Err(failure) => failure.into_blocked().unwrap(),
            Ok(_) => panic!("reversed full-precision package order must block"),
        };
        assert_eq!(
            blocked.reason(),
            Stage5gOrderPositionError::BrokerTruthTimeRegression
        );
        assert_eq!(
            blocked.session().summary().lifecycle_fingerprint_sha256,
            before_reverse
        );
    }

    #[test]
    fn replay_package_missing_source_receipt_is_structurally_rejected() {
        let mut encoded = serde_json::to_value(truth(vec![], vec![], vec![], 2)).unwrap();
        encoded.as_object_mut().unwrap().remove("received_ts");
        assert!(serde_json::from_value::<BrokerTruthSnapshot>(encoded).is_err());
    }

    fn stage5ge_c_timer_ready_source() -> crate::Stage5gCleanRestartSource {
        let converged = r2cb_public_converged_for_timer();
        let watermark_ms = converged
            .replay_checkpoint
            .last_broker_truth_received_ms
            .unwrap();
        let ready = match crate::apply_stage5g_timer_checkpoint(
            crate::attach_stage5g_timer_session(converged),
            crate::Stage5cPaperTimerInput {
                now_ts_utc_ms: watermark_ms + 1,
            },
        )
        .expect("clean-restart timer fixture advances")
        {
            crate::Stage5gTimerTransition::Ready(ready) => ready,
            crate::Stage5gTimerTransition::GeneratedIntent(_) => {
                panic!("clean-restart timer fixture must remain zero-intent")
            }
        };
        crate::Stage5gCleanRestartSource::TimerReady(ready)
    }

    fn stage5ge_c_awaiting_source() -> crate::Stage5gCleanRestartSource {
        let (session, ..) = stage5ge_b_after_first_poll();
        crate::Stage5gCleanRestartSource::OrderPositionAwaiting(session)
    }

    fn stage5ge_c_exact_source() -> crate::Stage5gCleanRestartSource {
        let (session, request_id, client_order_id, attribution, time_shift, polls) =
            stage5ge_b_after_first_poll();
        let checkpoint = stage5ge_b_committed_checkpoint(&session);
        let exact = stage5ge_b_r1_exact_replay(
            &checkpoint,
            3,
            request_id,
            &client_order_id,
            attribution,
            time_shift,
            &polls[0],
        );
        let synchronized = crate::apply_stage5g_exact_replay_to_session(session, exact)
            .expect("exact replay synchronizes before restart");
        crate::Stage5gCleanRestartSource::ExactReplaySynchronized(synchronized)
    }

    fn stage5ge_c_new_package_awaiting_source() -> crate::Stage5gCleanRestartSource {
        let (session, request_id, client_order_id, attribution, time_shift, polls) =
            stage5ge_b_after_first_poll();
        let checkpoint = stage5ge_b_committed_checkpoint(&session);
        let candidate = stage5ge_b_candidate(
            &checkpoint,
            Stage5gOrderPositionEvidence {
                total_sequence: 3,
                request_id,
                broker_truth: r2cb_golden_truth(&polls[1], &client_order_id, time_shift),
                order_attribution: attribution,
            },
        );
        let awaiting = crate::apply_stage5g_new_package_candidate(session, candidate)
            .expect("new package commits before restart")
            .into_awaiting()
            .expect("second poll remains awaiting");
        crate::Stage5gCleanRestartSource::NewPackageAwaiting(awaiting)
    }

    #[derive(Clone, Copy)]
    enum Stage5geCTestSourceKind {
        TimerReady,
        OrderPositionAwaiting,
        BeforeAckNoSlot,
        GeneratedIntentEscrow,
        GeneratedWorkingIntentEscrow,
        GeneratedLimitIntentEscrow,
        GeneratedCancelIntentEscrow,
        GeneratedCancelTargetAuthority,
        GeneratedFractionalIntentEscrow,
        TerminalPositionApplied,
        TerminalRejected,
        TerminalCanceledZero,
        TerminalCanceledPartial,
        TerminalExpiredZero,
        TerminalExpiredPartial,
        TerminalFlatExplicit,
        TerminalFlatAbsent,
        ExactReplaySynchronized,
        NewPackageAwaiting,
    }

    fn stage5ge_c_public_roundtrip_fixture(
        kind: Stage5geCTestSourceKind,
    ) -> (
        crate::Stage5gCleanRestartSource,
        crate::Stage5gCleanRestartExportInput,
        HybridIntradayRuntimeStrategy,
    ) {
        let bar_close_ts = Utc
            .with_ymd_and_hms(2026, 8, 3, 12, 0, 0)
            .single()
            .expect("Stage 5G-e-c fixture timestamp is valid")
            .timestamp();
        let persisted_at = Utc.timestamp_opt(bar_close_ts + 120, 0).single().unwrap();
        let strategy =
            r2cb_public_runtime_strategy_with_riskgate(bar_close_ts, RiskGateMode::NormalAppend);
        let (initial, request_id, client_order_id, attribution, time_shift, _prepared_authority) =
            r2cb_public_runtime_session_at_with_strategy_prepared(
                bar_close_ts,
                strategy,
                |strategy| {
                    crate::stage5d_persistence::stage5f_test_seams::prepare_stage5g_clean_restart_test_authority(
                    strategy,
                    "hybrid_imoexf",
                    persisted_at,
                )
                },
            );
        let golden: serde_json::Value = serde_json::from_str(include_str!(
            "../../../fixtures/expected/stage5g_r2cb_three_poll_broker_truth.json"
        ))
        .unwrap();
        let polls = golden["polls"].as_array().unwrap();
        let source = match kind {
            Stage5geCTestSourceKind::OrderPositionAwaiting => {
                let session = apply_stage5g_order_position_evidence(
                    initial,
                    Stage5gOrderPositionEvidence {
                        total_sequence: 2,
                        request_id,
                        broker_truth: r2cb_golden_truth(&polls[0], &client_order_id, time_shift),
                        order_attribution: attribution.clone(),
                    },
                )
                .unwrap()
                .into_awaiting()
                .unwrap();
                crate::Stage5gCleanRestartSource::OrderPositionAwaiting(session)
            }
            Stage5geCTestSourceKind::GeneratedIntentEscrow
            | Stage5geCTestSourceKind::GeneratedWorkingIntentEscrow
            | Stage5geCTestSourceKind::GeneratedLimitIntentEscrow
            | Stage5geCTestSourceKind::GeneratedCancelIntentEscrow
            | Stage5geCTestSourceKind::GeneratedCancelTargetAuthority
            | Stage5geCTestSourceKind::GeneratedFractionalIntentEscrow => {
                let mut session = apply_stage5g_order_position_evidence(
                    initial,
                    Stage5gOrderPositionEvidence {
                        total_sequence: 2,
                        request_id,
                        broker_truth: r2cb_golden_truth(&polls[0], &client_order_id, time_shift),
                        order_attribution: attribution.clone(),
                    },
                )
                .unwrap()
                .into_awaiting()
                .unwrap();
                for slot in &mut session.state.slots {
                    match kind {
                        Stage5geCTestSourceKind::GeneratedLimitIntentEscrow => {
                            slot.ack.action = Stage5gMockIntentAction::Place {
                                place_kind: Stage5gMockPlaceKind::Limit,
                            };
                            slot.source.base_action = Stage5gSourceBaseAction::Place;
                        }
                        Stage5geCTestSourceKind::GeneratedCancelIntentEscrow
                        | Stage5geCTestSourceKind::GeneratedCancelTargetAuthority => {
                            slot.ack.action = Stage5gMockIntentAction::Cancel {
                                target_order_id: BrokerOrderId::new(
                                    if matches!(
                                        kind,
                                        Stage5geCTestSourceKind::GeneratedCancelTargetAuthority
                                    ) {
                                        "CANCEL-TARGET-R4"
                                    } else {
                                        "CANCEL-TARGET-R2"
                                    },
                                ),
                            };
                            slot.ack.expected_client_order_id =
                                ClientOrderId::new("C-CANCEL").expect("cancel command client");
                            slot.ack.intent_class = "CancelCleanup".to_owned();
                            slot.ack.side = None;
                            slot.source.base_action = Stage5gSourceBaseAction::Cancel;
                            slot.source.intent_class =
                                crate::BrokerNeutralHybridIntentClass::CancelCleanup;
                            slot.source.side = None;
                        }
                        Stage5geCTestSourceKind::GeneratedFractionalIntentEscrow => {
                            slot.source.target_qty = Some(0.1);
                        }
                        _ => {}
                    }
                    slot.broker_order_id = None;
                    slot.order_events.clear();
                    slot.trades.clear();
                    slot.position = None;
                    slot.terminal = false;
                    if matches!(kind, Stage5geCTestSourceKind::GeneratedWorkingIntentEscrow) {
                        slot.broker_order_id = Some(BrokerOrderId::new("WORKING-TARGET-R3"));
                    }
                }
                if matches!(
                    kind,
                    Stage5geCTestSourceKind::GeneratedCancelTargetAuthority
                ) {
                    let mut broker_truth =
                        r2cb_golden_truth(&polls[0], &client_order_id, time_shift);
                    let target = broker_truth
                        .orders
                        .first_mut()
                        .expect("cancel target fixture order");
                    target.broker_order_id = Some(BrokerOrderId::new("CANCEL-TARGET-R4"));
                    target.client_order_id =
                        Some(ClientOrderId::new("C-PLACE").expect("target place client"));
                    target.status = OrderStatus::Working;
                    target.lifecycle = BrokerOrderSnapshot::lifecycle_for(&target.status);
                    target.order_type = OrderType::Limit;
                    target.time_in_force = Some(broker_core::TimeInForce::Day);
                    target.limit_price = Some(Decimal::new(2_210, 0));
                    target.filled_qty = Decimal::ZERO;
                    target.remaining_qty = Some(target.qty);
                    broker_truth.received_ts += chrono::Duration::milliseconds(1);
                    target.received_ts = broker_truth.received_ts;
                    broker_truth.trades.clear();
                    let session = apply_stage5g_order_position_evidence(
                        session,
                        Stage5gOrderPositionEvidence {
                            total_sequence: 3,
                            request_id,
                            broker_truth,
                            order_attribution: attribution,
                        },
                    )
                    .expect("separately owned cancel target is accepted")
                    .into_awaiting()
                    .expect("working cancel target remains awaiting");
                    crate::Stage5gCleanRestartSource::OrderPositionAwaiting(session)
                } else {
                    crate::Stage5gCleanRestartSource::OrderPositionAwaiting(session)
                }
            }
            Stage5geCTestSourceKind::BeforeAckNoSlot => {
                let mut session = apply_stage5g_order_position_evidence(
                    initial,
                    Stage5gOrderPositionEvidence {
                        total_sequence: 2,
                        request_id,
                        broker_truth: r2cb_golden_truth(&polls[0], &client_order_id, time_shift),
                        order_attribution: attribution,
                    },
                )
                .unwrap()
                .into_awaiting()
                .unwrap();
                session.state.slots.clear();
                crate::Stage5gCleanRestartSource::OrderPositionAwaiting(session)
            }
            Stage5geCTestSourceKind::TerminalPositionApplied
            | Stage5geCTestSourceKind::TerminalRejected
            | Stage5geCTestSourceKind::TerminalCanceledZero
            | Stage5geCTestSourceKind::TerminalCanceledPartial
            | Stage5geCTestSourceKind::TerminalExpiredZero
            | Stage5geCTestSourceKind::TerminalExpiredPartial
            | Stage5geCTestSourceKind::TerminalFlatExplicit
            | Stage5geCTestSourceKind::TerminalFlatAbsent => {
                let mut session = apply_stage5g_order_position_evidence(
                    initial,
                    Stage5gOrderPositionEvidence {
                        total_sequence: 2,
                        request_id,
                        broker_truth: r2cb_golden_truth(&polls[0], &client_order_id, time_shift),
                        order_attribution: attribution,
                    },
                )
                .unwrap()
                .into_awaiting()
                .unwrap();
                let slot = session
                    .state
                    .slots
                    .first_mut()
                    .expect("terminal fixture slot");
                let flat = matches!(
                    kind,
                    Stage5geCTestSourceKind::TerminalFlatExplicit
                        | Stage5geCTestSourceKind::TerminalFlatAbsent
                );
                if flat {
                    slot.ack.intent_class = "Exit".to_owned();
                    slot.ack.side = Some("Sell".to_owned());
                    slot.source.intent_class = crate::BrokerNeutralHybridIntentClass::Exit;
                    slot.source.side = Some(crate::BrokerNeutralOrderSide::Sell);
                    slot.source.pre_position_qty = 1.0;
                }
                let order = &mut slot
                    .order_events
                    .last_mut()
                    .expect("terminal fixture order")
                    .order;
                order.status = match kind {
                    Stage5geCTestSourceKind::TerminalRejected => OrderStatus::Rejected,
                    Stage5geCTestSourceKind::TerminalCanceledZero
                    | Stage5geCTestSourceKind::TerminalCanceledPartial => OrderStatus::Canceled,
                    Stage5geCTestSourceKind::TerminalExpiredZero
                    | Stage5geCTestSourceKind::TerminalExpiredPartial => OrderStatus::Expired,
                    _ => OrderStatus::Filled,
                };
                order.lifecycle = BrokerOrderSnapshot::lifecycle_for(&order.status);
                let partial = matches!(
                    kind,
                    Stage5geCTestSourceKind::TerminalCanceledPartial
                        | Stage5geCTestSourceKind::TerminalExpiredPartial
                );
                let zero_fill = matches!(
                    kind,
                    Stage5geCTestSourceKind::TerminalRejected
                        | Stage5geCTestSourceKind::TerminalCanceledZero
                        | Stage5geCTestSourceKind::TerminalExpiredZero
                );
                order.filled_qty = if zero_fill {
                    Decimal::ZERO
                } else if partial {
                    Decimal::new(4, 1)
                } else {
                    order.qty
                };
                order.remaining_qty = Some(order.qty - order.filled_qty);
                if flat {
                    order.side = OrderSide::Sell;
                }
                let fill_qty = order.qty;
                if zero_fill {
                    slot.trades.clear();
                    slot.position = None;
                } else {
                    slot.trades.truncate(1);
                    let trade = slot.trades.first_mut().expect("terminal fixture trade");
                    trade.qty = order.filled_qty;
                    if flat {
                        trade.side = OrderSide::Sell;
                    }
                }
                if matches!(kind, Stage5geCTestSourceKind::TerminalFlatAbsent) {
                    slot.position = None;
                } else if !zero_fill {
                    let (_, position) = slot.position.as_mut().expect("terminal fixture position");
                    position.qty = if flat {
                        Decimal::ZERO
                    } else if partial {
                        order.filled_qty
                    } else {
                        fill_qty
                    };
                }
                slot.terminal = true;
                crate::Stage5gCleanRestartSource::OrderPositionAwaiting(session)
            }
            Stage5geCTestSourceKind::ExactReplaySynchronized => {
                let session = apply_stage5g_order_position_evidence(
                    initial,
                    Stage5gOrderPositionEvidence {
                        total_sequence: 2,
                        request_id,
                        broker_truth: r2cb_golden_truth(&polls[0], &client_order_id, time_shift),
                        order_attribution: attribution.clone(),
                    },
                )
                .unwrap()
                .into_awaiting()
                .unwrap();
                let checkpoint = stage5ge_b_committed_checkpoint(&session);
                let exact = stage5ge_b_r1_exact_replay(
                    &checkpoint,
                    3,
                    request_id,
                    &client_order_id,
                    attribution,
                    time_shift,
                    &polls[0],
                );
                crate::Stage5gCleanRestartSource::ExactReplaySynchronized(
                    crate::apply_stage5g_exact_replay_to_session(session, exact).unwrap(),
                )
            }
            Stage5geCTestSourceKind::NewPackageAwaiting => {
                let session = apply_stage5g_order_position_evidence(
                    initial,
                    Stage5gOrderPositionEvidence {
                        total_sequence: 2,
                        request_id,
                        broker_truth: r2cb_golden_truth(&polls[0], &client_order_id, time_shift),
                        order_attribution: attribution.clone(),
                    },
                )
                .unwrap()
                .into_awaiting()
                .unwrap();
                let checkpoint = stage5ge_b_committed_checkpoint(&session);
                let candidate = stage5ge_b_candidate(
                    &checkpoint,
                    Stage5gOrderPositionEvidence {
                        total_sequence: 3,
                        request_id,
                        broker_truth: r2cb_golden_truth(&polls[1], &client_order_id, time_shift),
                        order_attribution: attribution,
                    },
                );
                crate::Stage5gCleanRestartSource::NewPackageAwaiting(
                    crate::apply_stage5g_new_package_candidate(session, candidate)
                        .unwrap()
                        .into_awaiting()
                        .unwrap(),
                )
            }
            Stage5geCTestSourceKind::TimerReady => {
                let mut session = initial;
                let mut converged = None;
                for (index, poll) in polls.iter().enumerate() {
                    let transition = apply_stage5g_order_position_evidence(
                        session,
                        Stage5gOrderPositionEvidence {
                            total_sequence: u64::try_from(index + 2).unwrap(),
                            request_id,
                            broker_truth: r2cb_golden_truth(poll, &client_order_id, time_shift),
                            order_attribution: attribution.clone(),
                        },
                    )
                    .unwrap();
                    if index + 1 == polls.len() {
                        converged = transition.into_converged();
                        break;
                    }
                    session = transition.into_awaiting().unwrap();
                }
                let converged = converged.unwrap();
                let watermark_ms = converged
                    .replay_checkpoint
                    .last_broker_truth_received_ms
                    .unwrap();
                let ready = match crate::apply_stage5g_timer_checkpoint(
                    crate::attach_stage5g_timer_session(converged),
                    crate::Stage5cPaperTimerInput {
                        now_ts_utc_ms: watermark_ms + 1,
                    },
                )
                .unwrap()
                {
                    crate::Stage5gTimerTransition::Ready(ready) => ready,
                    crate::Stage5gTimerTransition::GeneratedIntent(_) => {
                        panic!("zero-intent timer source remains ready")
                    }
                };
                crate::Stage5gCleanRestartSource::TimerReady(ready)
            }
        };
        let (riskgate, riskgate_evidence) =
            crate::stage5g_clean_restart::stage5g_test_persistence_authority_from_source(
                &source,
                persisted_at,
            );
        let input = crate::Stage5gCleanRestartExportInput {
            snapshot_id: format!("stage5ge-c-r1-{}", bar_close_ts),
            snapshot_revision: 1,
            previous_revision: None,
            write_generation: 1,
            persisted_at_ts_utc: persisted_at,
            source_commit_or_build_id:
                crate::stage5d_persistence::STAGE5D_RUNTIME_SEMANTIC_COMPATIBILITY_ID.to_string(),
            lifecycle_watermarks: crate::Stage5dLifecycleWatermarks {
                persisted_event_watermark: Some(format!("stage5ge-c-r1:{bar_close_ts}")),
                last_semantic_bar_ts: Utc.timestamp_opt(bar_close_ts, 0).single(),
                last_broker_event_ts: Some(persisted_at - Duration::seconds(1)),
            },
            riskgate,
            riskgate_evidence,
        };
        (
            source,
            input,
            r2cb_public_runtime_strategy_with_riskgate(bar_close_ts, RiskGateMode::NormalAppend),
        )
    }

    fn assert_stage5ge_c_public_clean_process_roundtrip(kind: Stage5geCTestSourceKind) {
        let commitment_key = stage5ge_c_commitment_key();
        let (source, input, fresh_runtime) = stage5ge_c_public_roundtrip_fixture(kind);
        let expected = stage5ge_c_projection(&source);
        let bytes = crate::export_stage5g_clean_restart(source, input, &commitment_key)
            .expect("public export consumes the only source authority");
        let copied_bytes = bytes.clone();
        drop(bytes);
        let restored =
            crate::restore_stage5g_clean_restart(&copied_bytes, &commitment_key, fresh_runtime)
                .expect("fresh runtime reconstructs from canonical copied bytes");
        assert_eq!(restored.lifecycle_kind(), expected.lifecycle_kind);
        assert_eq!(restored.summary(), &expected.summary);
        assert_eq!(restored.checkpoint(), &expected.checkpoint);
        assert_eq!(
            restored.reconstructed_runtime_state_fingerprint_sha256(),
            expected.strategy_state_fingerprint_sha256
        );
        let observation = restored.next_reconciliation_observation();
        assert_eq!(observation.strategy_id, "hybrid_imoexf");
        assert_eq!(
            observation.account_id,
            BrokerAccountId::new("ACC_TEST_0001")
        );
        assert_eq!(observation.instrument_id, target());
        assert_eq!(observation.lifecycle_kind, expected.lifecycle_kind);
        assert_eq!(
            observation.callback_count,
            expected.lifecycle_proof.authoritative_callback_count
        );
        assert_eq!(observation.request_count, expected.summary.request_count);
        assert_eq!(
            observation.continuation_checkpoint_ts_utc_ms,
            expected
                .checkpoint
                .payload
                .last_continuation_checkpoint_ts_utc_ms
        );
        assert_eq!(observation.source_lifecycle_commit_sha256.len(), 64);
        assert_eq!(observation.lifecycle_source_authority_sha256.len(), 64);
        assert!(!restored.intent_sink_attached());
        assert!(!restored.redis_command_stream_attached());
        assert!(!restored.finam_transport_attached());
        assert!(!restored.broker_execution_attached());
    }

    fn stage5ge_c_rehash_fixture(
        kind: Stage5geCTestSourceKind,
    ) -> (Vec<u8>, HybridIntradayRuntimeStrategy) {
        let (source, input, fresh_runtime) = stage5ge_c_public_roundtrip_fixture(kind);
        let commitment_key = stage5ge_c_commitment_key();
        (
            crate::export_stage5g_clean_restart(source, input, &commitment_key).unwrap(),
            fresh_runtime,
        )
    }

    fn stage5ge_c_commitment_key() -> crate::Stage5gLifecycleCommitmentKey {
        crate::Stage5gLifecycleCommitmentKey::from_secret_bytes(&[0x5a; 32]).unwrap()
    }

    pub(crate) fn stage5g_edb_restored_timer_ready_fixture(
    ) -> crate::Stage5gCleanRestartedCapability {
        let commitment_key = stage5ge_c_commitment_key();
        let (source, input, fresh_runtime) =
            stage5ge_c_public_roundtrip_fixture(Stage5geCTestSourceKind::TimerReady);
        let bytes = crate::export_stage5g_clean_restart(source, input, &commitment_key)
            .expect("e-d-b fixture exports authenticated restart bytes");
        crate::restore_stage5g_clean_restart(&bytes, &commitment_key, fresh_runtime)
            .expect("e-d-b fixture reconstructs through the accepted byte boundary")
    }

    pub(crate) fn stage5g_edb_restored_awaiting_fixture() -> crate::Stage5gCleanRestartedCapability
    {
        let commitment_key = stage5ge_c_commitment_key();
        let (source, input, fresh_runtime) =
            stage5ge_c_public_roundtrip_fixture(Stage5geCTestSourceKind::OrderPositionAwaiting);
        let bytes = crate::export_stage5g_clean_restart(source, input, &commitment_key)
            .expect("e-d-b fixture exports authenticated restart bytes");
        crate::restore_stage5g_clean_restart(&bytes, &commitment_key, fresh_runtime)
            .expect("e-d-b fixture reconstructs through the accepted byte boundary")
    }

    pub(crate) fn stage5g_edb_restored_generated_escrow_fixture(
    ) -> crate::Stage5gCleanRestartedCapability {
        let commitment_key = stage5ge_c_commitment_key();
        let (source, input, fresh_runtime) =
            stage5ge_c_public_roundtrip_fixture(Stage5geCTestSourceKind::GeneratedIntentEscrow);
        let bytes = crate::export_stage5g_clean_restart(source, input, &commitment_key)
            .expect("e-d-b escrow fixture exports authenticated restart bytes");
        crate::restore_stage5g_clean_restart(&bytes, &commitment_key, fresh_runtime)
            .expect("e-d-b escrow fixture reconstructs through the accepted byte boundary")
    }

    pub(crate) fn stage5g_edb_restored_generated_working_escrow_fixture(
    ) -> crate::Stage5gCleanRestartedCapability {
        let commitment_key = stage5ge_c_commitment_key();
        let (source, input, fresh_runtime) = stage5ge_c_public_roundtrip_fixture(
            Stage5geCTestSourceKind::GeneratedWorkingIntentEscrow,
        );
        let bytes = crate::export_stage5g_clean_restart(source, input, &commitment_key)
            .expect("e-d-b working escrow fixture exports authenticated restart bytes");
        crate::restore_stage5g_clean_restart(&bytes, &commitment_key, fresh_runtime)
            .expect("e-d-b working escrow fixture reconstructs through accepted byte boundary")
    }

    pub(crate) fn stage5g_edb_restored_generated_limit_escrow_fixture(
    ) -> crate::Stage5gCleanRestartedCapability {
        let commitment_key = stage5ge_c_commitment_key();
        let (source, input, fresh_runtime) = stage5ge_c_public_roundtrip_fixture(
            Stage5geCTestSourceKind::GeneratedLimitIntentEscrow,
        );
        let bytes = crate::export_stage5g_clean_restart(source, input, &commitment_key)
            .expect("e-d-b limit escrow exports authenticated restart bytes");
        crate::restore_stage5g_clean_restart(&bytes, &commitment_key, fresh_runtime)
            .expect("e-d-b limit escrow reconstructs through the accepted byte boundary")
    }

    pub(crate) fn stage5g_edb_restored_generated_cancel_escrow_fixture(
    ) -> crate::Stage5gCleanRestartedCapability {
        let commitment_key = stage5ge_c_commitment_key();
        let (source, input, fresh_runtime) = stage5ge_c_public_roundtrip_fixture(
            Stage5geCTestSourceKind::GeneratedCancelIntentEscrow,
        );
        let bytes = crate::export_stage5g_clean_restart(source, input, &commitment_key)
            .expect("e-d-b cancel escrow exports authenticated restart bytes");
        crate::restore_stage5g_clean_restart(&bytes, &commitment_key, fresh_runtime)
            .expect("e-d-b cancel escrow reconstructs through the accepted byte boundary")
    }

    pub(crate) fn stage5g_edb_restored_cancel_target_authority_fixture(
    ) -> crate::Stage5gCleanRestartedCapability {
        let commitment_key = stage5ge_c_commitment_key();
        let (source, input, fresh_runtime) = stage5ge_c_public_roundtrip_fixture(
            Stage5geCTestSourceKind::GeneratedCancelTargetAuthority,
        );
        let bytes = crate::export_stage5g_clean_restart(source, input, &commitment_key)
            .expect("e-d-b cancel target authority exports authenticated restart bytes");
        crate::restore_stage5g_clean_restart(&bytes, &commitment_key, fresh_runtime)
            .expect("e-d-b cancel target authority reconstructs through accepted byte boundary")
    }

    pub(crate) fn stage5g_edb_restored_generated_fractional_escrow_fixture(
    ) -> crate::Stage5gCleanRestartedCapability {
        let commitment_key = stage5ge_c_commitment_key();
        let (source, input, fresh_runtime) = stage5ge_c_public_roundtrip_fixture(
            Stage5geCTestSourceKind::GeneratedFractionalIntentEscrow,
        );
        let bytes = crate::export_stage5g_clean_restart(source, input, &commitment_key)
            .expect("e-d-b fractional escrow exports authenticated restart bytes");
        crate::restore_stage5g_clean_restart(&bytes, &commitment_key, fresh_runtime)
            .expect("e-d-b fractional escrow reconstructs through the accepted byte boundary")
    }

    pub(crate) fn stage5g_edb_restored_before_ack_fixture() -> crate::Stage5gCleanRestartedCapability
    {
        let commitment_key = stage5ge_c_commitment_key();
        let (source, input, fresh_runtime) =
            stage5ge_c_public_roundtrip_fixture(Stage5geCTestSourceKind::BeforeAckNoSlot);
        let bytes = crate::export_stage5g_clean_restart(source, input, &commitment_key)
            .expect("e-d-b pre-ACK fixture exports authenticated restart bytes");
        crate::restore_stage5g_clean_restart(&bytes, &commitment_key, fresh_runtime)
            .expect("e-d-b pre-ACK fixture reconstructs through the accepted byte boundary")
    }

    pub(crate) fn stage5g_edb_restored_terminal_applied_fixture(
    ) -> crate::Stage5gCleanRestartedCapability {
        let commitment_key = stage5ge_c_commitment_key();
        let (source, input, fresh_runtime) =
            stage5ge_c_public_roundtrip_fixture(Stage5geCTestSourceKind::TerminalPositionApplied);
        let bytes = crate::export_stage5g_clean_restart(source, input, &commitment_key)
            .expect("e-d-b terminal fixture exports authenticated restart bytes");
        crate::restore_stage5g_clean_restart(&bytes, &commitment_key, fresh_runtime)
            .expect("e-d-b terminal fixture reconstructs through the accepted byte boundary")
    }

    pub(crate) fn stage5g_edb_restored_terminal_rejected_fixture(
    ) -> crate::Stage5gCleanRestartedCapability {
        stage5g_edb_restored_terminal_status_fixture(Stage5geCTestSourceKind::TerminalRejected)
    }

    pub(crate) fn stage5g_edb_restored_terminal_canceled_fixture(
        partial: bool,
    ) -> crate::Stage5gCleanRestartedCapability {
        stage5g_edb_restored_terminal_status_fixture(if partial {
            Stage5geCTestSourceKind::TerminalCanceledPartial
        } else {
            Stage5geCTestSourceKind::TerminalCanceledZero
        })
    }

    pub(crate) fn stage5g_edb_restored_terminal_expired_fixture(
        partial: bool,
    ) -> crate::Stage5gCleanRestartedCapability {
        stage5g_edb_restored_terminal_status_fixture(if partial {
            Stage5geCTestSourceKind::TerminalExpiredPartial
        } else {
            Stage5geCTestSourceKind::TerminalExpiredZero
        })
    }

    fn stage5g_edb_restored_terminal_status_fixture(
        kind: Stage5geCTestSourceKind,
    ) -> crate::Stage5gCleanRestartedCapability {
        let commitment_key = stage5ge_c_commitment_key();
        let (source, input, fresh_runtime) = stage5ge_c_public_roundtrip_fixture(kind);
        let bytes = crate::export_stage5g_clean_restart(source, input, &commitment_key)
            .expect("e-d-b status terminal fixture exports authenticated restart bytes");
        crate::restore_stage5g_clean_restart(&bytes, &commitment_key, fresh_runtime)
            .expect("e-d-b status terminal fixture reconstructs through accepted byte boundary")
    }

    pub(crate) fn stage5g_edb_restored_terminal_flat_fixture(
        committed_position_row_present: bool,
    ) -> crate::Stage5gCleanRestartedCapability {
        let commitment_key = stage5ge_c_commitment_key();
        let kind = if committed_position_row_present {
            Stage5geCTestSourceKind::TerminalFlatExplicit
        } else {
            Stage5geCTestSourceKind::TerminalFlatAbsent
        };
        let (source, input, fresh_runtime) = stage5ge_c_public_roundtrip_fixture(kind);
        let bytes = crate::export_stage5g_clean_restart(source, input, &commitment_key)
            .expect("e-d-b terminal flat fixture exports authenticated restart bytes");
        crate::restore_stage5g_clean_restart(&bytes, &commitment_key, fresh_runtime)
            .expect("e-d-b terminal flat fixture reconstructs through accepted byte boundary")
    }

    fn assert_stage5ge_c_rehashed_error(
        kind: Stage5geCTestSourceKind,
        mutate_extension: impl FnOnce(&mut serde_json::Value),
        expected: crate::Stage5gCleanRestartError,
    ) {
        let (bytes, fresh) = stage5ge_c_rehash_fixture(kind);
        let forged = crate::stage5d_persistence::stage5g_test_rehash_clean_restart_package(
            &bytes,
            |_| {},
            mutate_extension,
        );
        assert_eq!(
            crate::restore_stage5g_clean_restart(&forged, &stage5ge_c_commitment_key(), fresh,)
                .map(|_| ()),
            Err(expected)
        );
    }

    fn assert_stage5ge_c_full_package_reseal_reaches_hmac(
        mutate_envelope: impl FnOnce(&mut serde_json::Value),
        mutate_evidence: impl FnOnce(&mut serde_json::Value),
        mutate_extension: impl FnOnce(&mut serde_json::Value),
    ) {
        let (bytes, fresh) = stage5ge_c_rehash_fixture(Stage5geCTestSourceKind::TimerReady);
        let forged = crate::stage5d_persistence::stage5g_test_rehash_full_clean_restart_package(
            &bytes,
            mutate_envelope,
            mutate_evidence,
            mutate_extension,
        );
        assert_eq!(
            crate::restore_stage5g_clean_restart(&forged, &stage5ge_c_commitment_key(), fresh,)
                .map(|_| ()),
            Err(crate::Stage5gCleanRestartError::AuthenticatedLifecycleCommitmentMismatch)
        );
    }

    #[test]
    fn stage5ge_c_r1_public_timer_ready_clean_process_roundtrip() {
        assert_stage5ge_c_public_clean_process_roundtrip(Stage5geCTestSourceKind::TimerReady);
    }

    #[test]
    fn stage5ge_c_r1_public_awaiting_clean_process_roundtrip() {
        assert_stage5ge_c_public_clean_process_roundtrip(
            Stage5geCTestSourceKind::OrderPositionAwaiting,
        );
    }

    #[test]
    fn stage5ge_c_r1_public_exact_source_clean_process_roundtrip() {
        assert_stage5ge_c_public_clean_process_roundtrip(
            Stage5geCTestSourceKind::ExactReplaySynchronized,
        );
    }

    #[test]
    fn stage5ge_c_r1_public_new_package_source_clean_process_roundtrip() {
        assert_stage5ge_c_public_clean_process_roundtrip(
            Stage5geCTestSourceKind::NewPackageAwaiting,
        );
    }

    #[test]
    fn stage5ge_c_r1_rehashed_stage5d_account_cross_binding_fails_closed() {
        let (bytes, fresh) =
            stage5ge_c_rehash_fixture(Stage5geCTestSourceKind::OrderPositionAwaiting);
        let forged = crate::stage5d_persistence::stage5g_test_rehash_clean_restart_package(
            &bytes,
            |envelope| {
                envelope["binding"]["account_id"] = serde_json::json!("ACC_FORGED_0002");
            },
            |_| {},
        );
        assert_eq!(
            crate::restore_stage5g_clean_restart(&forged, &stage5ge_c_commitment_key(), fresh,)
                .map(|_| ()),
            Err(crate::Stage5gCleanRestartError::AuthenticatedLifecycleCommitmentMismatch)
        );
    }

    #[test]
    fn stage5ge_c_r1_rehashed_stage5d_instrument_cross_binding_fails_closed() {
        let (bytes, fresh) =
            stage5ge_c_rehash_fixture(Stage5geCTestSourceKind::OrderPositionAwaiting);
        let forged = crate::stage5d_persistence::stage5g_test_rehash_clean_restart_package(
            &bytes,
            |envelope| {
                envelope["binding"]["instrument_id"]["symbol"] = serde_json::json!("RI");
            },
            |_| {},
        );
        assert_eq!(
            crate::restore_stage5g_clean_restart(&forged, &stage5ge_c_commitment_key(), fresh,)
                .map(|_| ()),
            Err(crate::Stage5gCleanRestartError::AuthenticatedLifecycleCommitmentMismatch)
        );
    }

    #[test]
    fn stage5ge_c_r1_rehashed_extension_binding_strategy_fails_closed() {
        let (bytes, fresh) =
            stage5ge_c_rehash_fixture(Stage5geCTestSourceKind::OrderPositionAwaiting);
        let forged = crate::stage5d_persistence::stage5g_test_rehash_clean_restart_package(
            &bytes,
            |_| {},
            |extension| {
                extension["binding"]["strategy_id"] = serde_json::json!("forged_strategy");
            },
        );
        assert_eq!(
            crate::restore_stage5g_clean_restart(&forged, &stage5ge_c_commitment_key(), fresh,)
                .map(|_| ()),
            Err(crate::Stage5gCleanRestartError::BindingMismatch)
        );
    }

    #[test]
    fn stage5ge_c_r1_rehashed_timer_summary_fails_closed() {
        let (bytes, fresh) = stage5ge_c_rehash_fixture(Stage5geCTestSourceKind::TimerReady);
        let forged = crate::stage5d_persistence::stage5g_test_rehash_clean_restart_package(
            &bytes,
            |_| {},
            |extension| {
                extension["summary"]["duplicate_evidence_count"] = serde_json::json!(999);
            },
        );
        assert_eq!(
            crate::restore_stage5g_clean_restart(&forged, &stage5ge_c_commitment_key(), fresh,)
                .map(|_| ()),
            Err(crate::Stage5gCleanRestartError::ReplayProjectionInconsistent)
        );
    }

    #[test]
    fn stage5ge_c_r1_rehashed_timer_checkpoint_graft_fails_closed() {
        let (source_bytes, _) =
            stage5ge_c_rehash_fixture(Stage5geCTestSourceKind::OrderPositionAwaiting);
        let source_json: serde_json::Value = serde_json::from_slice(&source_bytes).unwrap();
        let source_extension: serde_json::Value =
            serde_json::from_str(source_json["stage5g_extension_json"].as_str().unwrap()).unwrap();
        let graft = source_extension["checkpoint"].clone();
        let (bytes, fresh) = stage5ge_c_rehash_fixture(Stage5geCTestSourceKind::TimerReady);
        let forged = crate::stage5d_persistence::stage5g_test_rehash_clean_restart_package(
            &bytes,
            |_| {},
            |extension| extension["checkpoint"] = graft,
        );
        assert_eq!(
            crate::restore_stage5g_clean_restart(&forged, &stage5ge_c_commitment_key(), fresh,)
                .map(|_| ()),
            Err(crate::Stage5gCleanRestartError::TimerReadySourceAuthorityMismatch)
        );
    }

    #[test]
    fn stage5ge_c_r1_rehashed_callback_self_authority_fails_closed() {
        let (bytes, fresh) = stage5ge_c_rehash_fixture(Stage5geCTestSourceKind::NewPackageAwaiting);
        let forged = crate::stage5d_persistence::stage5g_test_rehash_clean_restart_package(
            &bytes,
            |_| {},
            |extension| {
                extension["summary"]["stage5c_callback_count"] = serde_json::json!(7);
                extension["lifecycle_proof"]["authoritative_callback_count"] = serde_json::json!(7);
            },
        );
        assert_eq!(
            crate::restore_stage5g_clean_restart(&forged, &stage5ge_c_commitment_key(), fresh,)
                .map(|_| ()),
            Err(crate::Stage5gCleanRestartError::CallbackAuthorityMismatch)
        );
    }

    #[test]
    fn stage5ge_c_r1_rehashed_lifecycle_kind_swap_fails_closed() {
        let (bytes, fresh) =
            stage5ge_c_rehash_fixture(Stage5geCTestSourceKind::ExactReplaySynchronized);
        let forged = crate::stage5d_persistence::stage5g_test_rehash_clean_restart_package(
            &bytes,
            |_| {},
            |extension| {
                extension["lifecycle_kind"] = serde_json::json!("timer_ready");
            },
        );
        assert_eq!(
            crate::restore_stage5g_clean_restart(&forged, &stage5ge_c_commitment_key(), fresh,)
                .map(|_| ()),
            Err(crate::Stage5gCleanRestartError::UnexpectedOrderPositionState)
        );
    }

    #[test]
    fn stage5ge_c_r1_rehashed_order_position_graft_fails_closed() {
        let (source_bytes, _) =
            stage5ge_c_rehash_fixture(Stage5geCTestSourceKind::OrderPositionAwaiting);
        let source_json: serde_json::Value = serde_json::from_slice(&source_bytes).unwrap();
        let source_extension: serde_json::Value =
            serde_json::from_str(source_json["stage5g_extension_json"].as_str().unwrap()).unwrap();
        let graft = source_extension["order_position_state"].clone();
        let (target_bytes, fresh) =
            stage5ge_c_rehash_fixture(Stage5geCTestSourceKind::NewPackageAwaiting);
        let forged = crate::stage5d_persistence::stage5g_test_rehash_clean_restart_package(
            &target_bytes,
            |_| {},
            |extension| extension["order_position_state"] = graft,
        );
        assert_eq!(
            crate::restore_stage5g_clean_restart(&forged, &stage5ge_c_commitment_key(), fresh,)
                .map(|_| ()),
            Err(crate::Stage5gCleanRestartError::ReplayProjectionInconsistent)
        );
    }

    #[test]
    fn stage5ge_c_r2_fully_resealed_timer_request_count_fails_semantically() {
        assert_stage5ge_c_rehashed_error(
            Stage5geCTestSourceKind::TimerReady,
            |extension| {
                extension["summary"]["request_count"] = serde_json::json!(999);
            },
            crate::Stage5gCleanRestartError::AuthenticatedLifecycleCommitmentMismatch,
        );
    }

    #[test]
    fn stage5ge_c_r2_fully_resealed_timer_lifecycle_fingerprint_fails_semantically() {
        assert_stage5ge_c_rehashed_error(
            Stage5geCTestSourceKind::TimerReady,
            |extension| {
                extension["summary"]["lifecycle_fingerprint_sha256"] =
                    serde_json::json!("0".repeat(64));
            },
            crate::Stage5gCleanRestartError::AuthenticatedLifecycleCommitmentMismatch,
        );
    }

    #[test]
    fn stage5ge_c_r2_fully_resealed_timer_all_lifecycle_counts_fail_semantically() {
        assert_stage5ge_c_rehashed_error(
            Stage5geCTestSourceKind::TimerReady,
            |extension| {
                extension["summary"]["terminal_request_count"] = serde_json::json!(7);
                extension["summary"]["order_transition_count"] = serde_json::json!(8);
                extension["summary"]["correlated_trade_count"] = serde_json::json!(9);
                extension["summary"]["position_confirmation_count"] = serde_json::json!(10);
            },
            crate::Stage5gCleanRestartError::AuthenticatedLifecycleCommitmentMismatch,
        );
    }

    #[test]
    fn stage5ge_c_r2_fully_resealed_valid_checkpoint_graft_with_watermarks_fails() {
        assert_stage5ge_c_rehashed_error(
            Stage5geCTestSourceKind::TimerReady,
            |extension| {
                let current = extension["checkpoint"]["payload"]
                    ["last_continuation_checkpoint_ts_utc_ms"]
                    .as_i64()
                    .unwrap();
                let grafted = serde_json::json!(current + 1);
                extension["checkpoint"]["payload"]["last_continuation_checkpoint_ts_utc_ms"] =
                    grafted;
            },
            crate::Stage5gCleanRestartError::AuthenticatedLifecycleCommitmentMismatch,
        );
    }

    #[test]
    fn stage5ge_c_r2_fully_resealed_inner_settlement_checkpoint_regression_fails() {
        assert_stage5ge_c_rehashed_error(
            Stage5geCTestSourceKind::TimerReady,
            |extension| {
                let outer = extension["checkpoint"]["payload"]
                    ["last_continuation_checkpoint_ts_utc_ms"]
                    .as_i64()
                    .unwrap();
                extension["timer_ready_source"]["stage5c_settlement"]["checkpoint_ts_utc_ms"] =
                    serde_json::json!(outer + 1);
            },
            crate::Stage5gCleanRestartError::TimerReadySourceAuthorityMismatch,
        );
    }

    #[test]
    fn stage5ge_c_r2_fully_resealed_recovery_receipt_graft_fails() {
        assert_stage5ge_c_rehashed_error(
            Stage5geCTestSourceKind::TimerReady,
            |extension| {
                extension["timer_ready_source"]["stage5c_settlement"]["recovery_receipt"]
                    ["processed_bars"] = serde_json::json!(999);
            },
            crate::Stage5gCleanRestartError::AuthenticatedLifecycleCommitmentMismatch,
        );
    }

    #[test]
    fn stage5ge_c_r3_fully_resealed_empty_timer_history_fails_semantically() {
        assert_stage5ge_c_rehashed_error(
            Stage5geCTestSourceKind::TimerReady,
            |extension| {
                extension["timer_ready_source"]["stage5c_settlement"]["settled_batch_history"] =
                    serde_json::json!([]);
            },
            crate::Stage5gCleanRestartError::TimerReadySourceAuthorityMismatch,
        );
    }

    #[test]
    fn stage5ge_c_r3_fully_resealed_timer_history_state_fingerprint_fails_anchor() {
        assert_stage5ge_c_rehashed_error(
            Stage5geCTestSourceKind::TimerReady,
            |extension| {
                extension["timer_ready_source"]["stage5c_settlement"]["settled_batch_history"][0]
                    ["state_fingerprint"] = serde_json::json!("forged-state-fingerprint");
            },
            crate::Stage5gCleanRestartError::AuthenticatedLifecycleCommitmentMismatch,
        );
    }

    #[test]
    fn stage5ge_c_r3_missing_stage5d_source_anchor_fails_closed() {
        let (bytes, fresh) = stage5ge_c_rehash_fixture(Stage5geCTestSourceKind::TimerReady);
        let forged = crate::stage5d_persistence::stage5g_test_rehash_clean_restart_package(
            &bytes,
            |envelope| {
                envelope
                    .as_object_mut()
                    .unwrap()
                    .remove("stage5g_source_authority_anchor_sha256");
            },
            |_| {},
        );
        assert_eq!(
            crate::restore_stage5g_clean_restart(&forged, &stage5ge_c_commitment_key(), fresh,)
                .map(|_| ()),
            Err(crate::Stage5gCleanRestartError::Stage5dSourceAuthorityAnchorMismatch)
        );
    }

    #[test]
    fn stage5ge_c_r4_missing_authenticated_commitment_fails_closed() {
        let (bytes, fresh) = stage5ge_c_rehash_fixture(Stage5geCTestSourceKind::TimerReady);
        let forged = crate::stage5d_persistence::stage5g_test_rehash_clean_restart_package(
            &bytes,
            |envelope| {
                envelope
                    .as_object_mut()
                    .unwrap()
                    .remove("stage5g_source_authority_hmac_sha256");
            },
            |_| {},
        );
        assert_eq!(
            crate::restore_stage5g_clean_restart(&forged, &stage5ge_c_commitment_key(), fresh,)
                .map(|_| ()),
            Err(crate::Stage5gCleanRestartError::AuthenticatedLifecycleCommitmentMismatch)
        );
    }

    #[test]
    fn stage5ge_c_r4_stage5d_authenticated_commitment_is_present_and_canonical() {
        let (bytes, _) = stage5ge_c_rehash_fixture(Stage5geCTestSourceKind::TimerReady);
        let package: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let envelope: serde_json::Value =
            serde_json::from_str(package["envelope_json"].as_str().unwrap()).unwrap();
        let anchor = envelope["stage5g_source_authority_anchor_sha256"]
            .as_str()
            .unwrap();
        assert_eq!(anchor.len(), 64);
        assert!(anchor
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()));
        let commitment = envelope["stage5g_source_authority_hmac_sha256"]
            .as_str()
            .unwrap();
        assert_eq!(commitment.len(), 64);
        assert!(commitment
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()));
    }

    #[test]
    fn stage5ge_c_r4_authenticated_commitment_substitution_fails_closed() {
        let (bytes, fresh) = stage5ge_c_rehash_fixture(Stage5geCTestSourceKind::TimerReady);
        let forged = crate::stage5d_persistence::stage5g_test_rehash_clean_restart_package(
            &bytes,
            |envelope| {
                envelope["stage5g_source_authority_hmac_sha256"] =
                    serde_json::json!("a".repeat(64));
            },
            |_| {},
        );
        assert_eq!(
            crate::restore_stage5g_clean_restart(&forged, &stage5ge_c_commitment_key(), fresh,)
                .map(|_| ()),
            Err(crate::Stage5gCleanRestartError::AuthenticatedLifecycleCommitmentMismatch)
        );
    }

    #[test]
    fn stage5ge_c_r4_wrong_operator_commitment_key_fails_closed() {
        let (bytes, fresh) = stage5ge_c_rehash_fixture(Stage5geCTestSourceKind::TimerReady);
        let wrong_key = crate::Stage5gLifecycleCommitmentKey::from_secret_bytes(&[0x6b; 32])
            .expect("test key has the required length");
        assert_eq!(
            crate::restore_stage5g_clean_restart(&bytes, &wrong_key, fresh).map(|_| ()),
            Err(crate::Stage5gCleanRestartError::AuthenticatedLifecycleCommitmentMismatch)
        );
    }

    #[test]
    fn stage5ge_c_r4_fresh_runtime_config_mismatch_fails_closed() {
        let (source, input, _) =
            stage5ge_c_public_roundtrip_fixture(Stage5geCTestSourceKind::TimerReady);
        let bar_close_ts = input
            .lifecycle_watermarks
            .last_semantic_bar_ts
            .expect("test package has a semantic watermark")
            .timestamp();
        let mismatched_runtime =
            r2cb_public_runtime_strategy_with_riskgate(bar_close_ts, RiskGateMode::Disabled);
        let key = stage5ge_c_commitment_key();
        let bytes = crate::export_stage5g_clean_restart(source, input, &key).unwrap();
        assert_eq!(
            crate::restore_stage5g_clean_restart(&bytes, &key, mismatched_runtime).map(|_| ()),
            Err(crate::Stage5gCleanRestartError::BindingMismatch)
        );
    }

    #[test]
    fn stage5ge_c_r4_old_package_fails_after_operator_key_epoch_rotation() {
        let (bytes, fresh) = stage5ge_c_rehash_fixture(Stage5geCTestSourceKind::TimerReady);
        let newer_epoch_key = crate::Stage5gLifecycleCommitmentKey::from_secret_bytes(&[0x7c; 32])
            .expect("rotated test key has the required length");
        assert_eq!(
            crate::restore_stage5g_clean_restart(&bytes, &newer_epoch_key, fresh).map(|_| ()),
            Err(crate::Stage5gCleanRestartError::AuthenticatedLifecycleCommitmentMismatch)
        );
    }

    #[test]
    fn stage5ge_c_r4_fully_coherent_unkeyed_reseal_cannot_forge_commitment() {
        assert_stage5ge_c_rehashed_error(
            Stage5geCTestSourceKind::TimerReady,
            |extension| {
                extension["summary"]["request_count"] = serde_json::json!(41);
                extension["summary"]["terminal_request_count"] = serde_json::json!(41);
                extension["summary"]["lifecycle_fingerprint_sha256"] =
                    serde_json::json!("b".repeat(64));

                let settlement = &mut extension["timer_ready_source"]["stage5c_settlement"];
                settlement["recovery_receipt"]["processed_bars"] = serde_json::json!(77);
                settlement["settled_batch"]["state_fingerprint"] =
                    serde_json::json!("coherently-forged-state");
                let history = settlement["settled_batch_history"]
                    .as_array_mut()
                    .expect("TimerReady history is an array");
                history.last_mut().expect("TimerReady history is nonempty")["state_fingerprint"] =
                    serde_json::json!("coherently-forged-state");

                let current = extension["checkpoint"]["payload"]
                    ["last_continuation_checkpoint_ts_utc_ms"]
                    .as_i64()
                    .expect("TimerReady continuation checkpoint exists");
                extension["checkpoint"]["payload"]["last_continuation_checkpoint_ts_utc_ms"] =
                    serde_json::json!(current + 1);
            },
            crate::Stage5gCleanRestartError::AuthenticatedLifecycleCommitmentMismatch,
        );
    }

    #[test]
    fn stage5ge_c_r5_persisted_event_watermark_reseal_fails_at_hmac() {
        assert_stage5ge_c_full_package_reseal_reaches_hmac(
            |envelope| {
                envelope["lifecycle_watermarks"]["persisted_event_watermark"] =
                    serde_json::json!("stage5ge-c-r5:forged-watermark");
            },
            |_| {},
            |_| {},
        );
    }

    #[test]
    fn stage5ge_c_r5_semantic_timestamp_watermark_reseal_fails_at_hmac() {
        assert_stage5ge_c_full_package_reseal_reaches_hmac(
            |envelope| {
                let current = envelope["lifecycle_watermarks"]["last_semantic_bar_ts"]
                    .as_str()
                    .unwrap();
                let shifted = DateTime::parse_from_rfc3339(current)
                    .unwrap()
                    .with_timezone(&Utc)
                    + Duration::seconds(1);
                envelope["lifecycle_watermarks"]["last_semantic_bar_ts"] =
                    serde_json::json!(shifted);
            },
            |_| {},
            |_| {},
        );
    }

    #[test]
    fn stage5ge_c_r5_snapshot_revision_reseal_fails_at_hmac() {
        assert_stage5ge_c_full_package_reseal_reaches_hmac(
            |envelope| {
                envelope["snapshot_revision"] = serde_json::json!(2);
                envelope["previous_revision"] = serde_json::json!(1);
            },
            |_| {},
            |_| {},
        );
    }

    #[test]
    fn stage5ge_c_r5_write_generation_reseal_fails_at_hmac() {
        assert_stage5ge_c_full_package_reseal_reaches_hmac(
            |envelope| envelope["write_generation"] = serde_json::json!(2),
            |_| {},
            |_| {},
        );
    }

    #[test]
    fn stage5ge_c_r5_persisted_timestamp_reseal_fails_at_hmac() {
        assert_stage5ge_c_full_package_reseal_reaches_hmac(
            |envelope| {
                let current = envelope["persisted_at_ts_utc"].as_str().unwrap();
                let shifted = DateTime::parse_from_rfc3339(current)
                    .unwrap()
                    .with_timezone(&Utc)
                    + Duration::seconds(1);
                envelope["persisted_at_ts_utc"] = serde_json::json!(shifted);
            },
            |_| {},
            |_| {},
        );
    }

    #[test]
    fn stage5ge_c_r5_compatible_source_build_reseal_fails_at_hmac() {
        assert_stage5ge_c_full_package_reseal_reaches_hmac(
            |envelope| {
                envelope["binding"]["source_commit_or_build_id"] =
                    serde_json::json!("source_commit:92e6e0685b1cbab6f4c6271abe1db8ab690a1ded");
            },
            |_| {},
            |_| {},
        );
    }

    #[test]
    fn stage5ge_c_r5_runtime_private_cleanup_reseal_fails_at_hmac() {
        assert_stage5ge_c_full_package_reseal_reaches_hmac(
            |envelope| {
                envelope["runtime_private_extension"]["cleanup_retry_state"]
                    ["cleanup_stop_retry_attempts"] = serde_json::json!(1);
            },
            |_| {},
            |_| {},
        );
    }

    #[test]
    fn stage5ge_c_r5_recovery_index_reseal_fails_at_hmac() {
        assert_stage5ge_c_full_package_reseal_reaches_hmac(
            |envelope| {
                envelope["recovery_indexes"]["known_trade_ids"]
                    .as_array_mut()
                    .unwrap()
                    .push(serde_json::json!("TRADE_R5_FORGED"));
            },
            |_| {},
            |_| {},
        );
    }

    #[test]
    fn stage5ge_c_r5_riskgate_evidence_reseal_fails_at_hmac() {
        assert_stage5ge_c_full_package_reseal_reaches_hmac(
            |_| {},
            |evidence| {
                evidence["current_shadow_pnl_points"] = serde_json::json!("0.5");
            },
            |_| {},
        );
    }

    #[test]
    fn stage5ge_c_r5_riskgate_persistence_reseal_fails_at_hmac() {
        assert_stage5ge_c_full_package_reseal_reaches_hmac(
            |envelope| {
                envelope["riskgate"]["materialized_state"]["current_shadow_pnl_points"] =
                    serde_json::json!("0.5");
            },
            |_| {},
            |_| {},
        );
    }

    #[test]
    fn stage5ge_c_r5_lifecycle_tag_transplant_to_package_instance_fails_at_hmac() {
        assert_stage5ge_c_full_package_reseal_reaches_hmac(
            |envelope| {
                envelope["snapshot_id"] = serde_json::json!("stage5ge-c-r5-other-instance");
            },
            |_| {},
            |_| {},
        );
    }

    #[test]
    fn stage5ge_c_r5_complete_envelope_extension_reseal_fails_at_hmac() {
        assert_stage5ge_c_full_package_reseal_reaches_hmac(
            |envelope| {
                envelope["lifecycle_watermarks"]["persisted_event_watermark"] =
                    serde_json::json!("stage5ge-c-r5:complete-reseal");
                envelope["runtime_private_extension"]["cleanup_retry_state"]
                    ["cleanup_stop_retry_attempts"] = serde_json::json!(2);
            },
            |_| {},
            |extension| {
                extension["summary"]["request_count"] = serde_json::json!(41);
                extension["summary"]["terminal_request_count"] = serde_json::json!(41);
                extension["summary"]["lifecycle_fingerprint_sha256"] =
                    serde_json::json!("b".repeat(64));
                let settlement = &mut extension["timer_ready_source"]["stage5c_settlement"];
                settlement["recovery_receipt"]["processed_bars"] = serde_json::json!(77);
                settlement["settled_batch"]["state_fingerprint"] =
                    serde_json::json!("coherently-forged-state");
                settlement["settled_batch_history"]
                    .as_array_mut()
                    .unwrap()
                    .last_mut()
                    .unwrap()["state_fingerprint"] = serde_json::json!("coherently-forged-state");
            },
        );
    }

    #[test]
    fn stage5ge_c_r3_resealed_timer_history_must_end_in_settled_batch() {
        assert_stage5ge_c_rehashed_error(
            Stage5geCTestSourceKind::TimerReady,
            |extension| {
                extension["timer_ready_source"]["stage5c_settlement"]["settled_batch_history"]
                    .as_array_mut()
                    .unwrap()
                    .pop();
            },
            crate::Stage5gCleanRestartError::TimerReadySourceAuthorityMismatch,
        );
    }

    #[test]
    fn stage5ge_c_r2_fully_resealed_complete_extension_graft_fails_package_binding() {
        let (donor_bytes, _) =
            stage5ge_c_rehash_fixture(Stage5geCTestSourceKind::OrderPositionAwaiting);
        let donor_package: serde_json::Value = serde_json::from_slice(&donor_bytes).unwrap();
        let donor_extension: serde_json::Value =
            serde_json::from_str(donor_package["stage5g_extension_json"].as_str().unwrap())
                .unwrap();
        let (target_bytes, fresh) =
            stage5ge_c_rehash_fixture(Stage5geCTestSourceKind::NewPackageAwaiting);
        let target_package: serde_json::Value = serde_json::from_slice(&target_bytes).unwrap();
        let target_extension: serde_json::Value =
            serde_json::from_str(target_package["stage5g_extension_json"].as_str().unwrap())
                .unwrap();
        let target_package_instance = target_extension["package_instance"].clone();
        let forged = crate::stage5d_persistence::stage5g_test_rehash_clean_restart_package(
            &target_bytes,
            |_| {},
            move |extension| {
                *extension = donor_extension;
                extension["package_instance"] = target_package_instance;
            },
        );
        assert_eq!(
            crate::restore_stage5g_clean_restart(&forged, &stage5ge_c_commitment_key(), fresh,)
                .map(|_| ()),
            Err(crate::Stage5gCleanRestartError::AuthenticatedLifecycleCommitmentMismatch)
        );
    }

    fn stage5ge_c_projection(
        source: &crate::Stage5gCleanRestartSource,
    ) -> crate::stage5g_clean_restart::Stage5gCleanRestartProjectionV1 {
        crate::stage5g_clean_restart::stage5g_test_projection_from_source(source)
            .expect("accepted clean-restart source projects")
    }

    #[test]
    fn stage5ge_c_timer_ready_zero_intent_projects_through_canonical_boundary() {
        let source = stage5ge_c_timer_ready_source();
        let projection = stage5ge_c_projection(&source);
        assert_eq!(
            projection.lifecycle_kind,
            crate::Stage5gCleanRestartLifecycleKind::TimerReady
        );
        assert!(projection.order_position_state.is_none());
        crate::stage5g_clean_restart::validate_projection(&projection).unwrap();
    }

    #[test]
    fn stage5ge_c_awaiting_order_position_preserves_slots() {
        let source = stage5ge_c_awaiting_source();
        let projection = stage5ge_c_projection(&source);
        assert_eq!(
            projection.lifecycle_kind,
            crate::Stage5gCleanRestartLifecycleKind::OrderPositionAwaitingCommitted
        );
        assert_eq!(projection.summary.request_count, 1);
        assert!(projection.order_position_state.is_some());
    }

    #[test]
    fn stage5ge_c_exact_replay_synchronized_projection_roundtrips() {
        let source = stage5ge_c_exact_source();
        let projection = stage5ge_c_projection(&source);
        let bytes = serde_json::to_vec(&projection).unwrap();
        let decoded = serde_json::from_slice(&bytes).unwrap();
        crate::stage5g_clean_restart::validate_projection(&decoded).unwrap();
        assert_eq!(
            decoded.lifecycle_kind,
            crate::Stage5gCleanRestartLifecycleKind::OrderPositionAwaitingCommitted
        );
    }

    #[test]
    fn stage5ge_c_new_package_awaiting_projection_roundtrips() {
        let source = stage5ge_c_new_package_awaiting_source();
        let projection = stage5ge_c_projection(&source);
        let decoded = serde_json::from_str(&serde_json::to_string(&projection).unwrap()).unwrap();
        crate::stage5g_clean_restart::validate_projection(&decoded).unwrap();
        assert_eq!(
            decoded.lifecycle_kind,
            crate::Stage5gCleanRestartLifecycleKind::OrderPositionAwaitingCommitted
        );
    }

    #[test]
    fn stage5ge_c_historical_replay_ledger_and_counters_survive_bytes() {
        let source = stage5ge_c_exact_source();
        let projection = stage5ge_c_projection(&source);
        let expected = projection.checkpoint.payload.clone();
        let decoded: crate::stage5g_clean_restart::Stage5gCleanRestartProjectionV1 =
            serde_json::from_slice(&serde_json::to_vec(&projection).unwrap()).unwrap();
        assert_eq!(decoded.checkpoint.payload, expected);
        assert!(decoded.checkpoint.payload.duplicate_evidence_count > 0);
    }

    #[test]
    fn stage5ge_c_exact_decimal_representation_survives_byte_roundtrip() {
        let source = stage5ge_c_new_package_awaiting_source();
        let projection = stage5ge_c_projection(&source);
        let first = serde_json::to_string(&projection).unwrap();
        let decoded: crate::stage5g_clean_restart::Stage5gCleanRestartProjectionV1 =
            serde_json::from_str(&first).unwrap();
        let second = serde_json::to_string(&decoded).unwrap();
        assert_eq!(
            first, second,
            "Decimal sign/scale bytes must remain canonical"
        );
    }

    #[test]
    fn stage5ge_c_callback_count_and_state_fingerprint_remain_exact() {
        let source = stage5ge_c_new_package_awaiting_source();
        let projection = stage5ge_c_projection(&source);
        let expected_callback_count = projection.summary.stage5c_callback_count;
        let decoded: crate::stage5g_clean_restart::Stage5gCleanRestartProjectionV1 =
            serde_json::from_slice(&serde_json::to_vec(&projection).unwrap()).unwrap();
        assert_eq!(
            decoded.summary.stage5c_callback_count,
            expected_callback_count
        );
        assert!(!projection.strategy_state_fingerprint_sha256.is_empty());
        crate::stage5g_clean_restart::validate_projection(&decoded).unwrap();
    }

    #[test]
    fn stage5ge_c_missing_replay_projection_fails_closed() {
        let missing = serde_json::json!({
            "schema_version": crate::STAGE5G_CLEAN_RESTART_EXTENSION_SCHEMA_VERSION
        });
        assert!(serde_json::from_value::<
            crate::stage5g_clean_restart::Stage5gCleanRestartProjectionV1,
        >(missing)
        .is_err());
    }

    #[test]
    fn stage5ge_c_regressive_continuation_checkpoint_fails_closed() {
        let source = stage5ge_c_exact_source();
        let mut projection = stage5ge_c_projection(&source);
        let received_ms = projection
            .checkpoint
            .payload
            .last_broker_truth_received_ms
            .unwrap();
        projection
            .checkpoint
            .payload
            .last_continuation_checkpoint_ts_utc_ms = Some(received_ms - 1);
        projection.checkpoint =
            crate::stage5g_timer::stage5g_test_reseal_checkpoint(&projection.checkpoint.payload);
        crate::stage5g_clean_restart::stage5g_test_reseal_lifecycle_authority(&mut projection);
        assert_eq!(
            crate::stage5g_clean_restart::validate_projection(&projection),
            Err(crate::Stage5gCleanRestartError::ReplayCheckpoint(
                crate::Stage5gTimerCheckpointError::ContinuationBeforeBrokerTruth
            ))
        );
    }

    #[test]
    fn stage5ge_c_unsupported_lifecycle_kind_fails_closed() {
        let source = stage5ge_c_exact_source();
        let projection = stage5ge_c_projection(&source);
        let payload = serde_json::to_string(&projection).unwrap().replace(
            "order_position_awaiting_committed",
            "generated_intent_escrow",
        );
        assert!(serde_json::from_str::<
            crate::stage5g_clean_restart::Stage5gCleanRestartProjectionV1,
        >(&payload)
        .is_err());
    }

    #[test]
    fn stage5ge_c_missing_order_position_state_fails_closed() {
        let source = stage5ge_c_exact_source();
        let mut projection = stage5ge_c_projection(&source);
        projection.order_position_state = None;
        crate::stage5g_clean_restart::stage5g_test_reseal_lifecycle_authority(&mut projection);
        assert_eq!(
            crate::stage5g_clean_restart::validate_projection(&projection),
            Err(crate::Stage5gCleanRestartError::MissingOrderPositionState)
        );
    }

    #[test]
    fn stage5ge_c_conflicting_slot_projection_fails_closed() {
        let source = stage5ge_c_new_package_awaiting_source();
        let mut projection = stage5ge_c_projection(&source);
        projection.summary.duplicate_evidence_count += 1;
        crate::stage5g_clean_restart::stage5g_test_reseal_lifecycle_authority(&mut projection);
        assert_eq!(
            crate::stage5g_clean_restart::validate_projection(&projection),
            Err(crate::Stage5gCleanRestartError::ReplayProjectionInconsistent)
        );
    }
    // STAGE5G-C-REPLAY-PACKAGE-IDENTITY-WITNESSES-END
    // STAGE5G-C-R2CB-PARITY-TESTS-END: broker-truth-finam-parity-v1
}
