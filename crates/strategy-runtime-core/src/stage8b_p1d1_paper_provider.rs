//! Stage 8B-P1-d1 deterministic Market paper-provider core.
//!
//! This module deliberately contains no Redis, FINAM, wall-clock acquisition,
//! ACK, broker projection or runtime-live operation.  The production wait
//! state can only be minted inside this crate from the retained Stage 5E
//! schedule projection.  A provider outcome additionally requires the linear
//! Stage 6 durable dispatch receipt, so observing a price bar alone never
//! grants an effect capability.
//!
//! The wait and eligibility states intentionally expose no provider call:
//!
//! ```compile_fail
//! use strategy_runtime_core::Stage8bP1d1AwaitingExecutionBar;
//! let waiting: Stage8bP1d1AwaitingExecutionBar = unreachable!();
//! let _ = waiting.execute();
//! ```
//!
//! ```compile_fail
//! use strategy_runtime_core::Stage8bP1d1ExecutionEligible;
//! let eligible: Stage8bP1d1ExecutionEligible = unreachable!();
//! let _ = eligible.execute();
//! ```
//!
//! Canonical predecessor/candidate authority is also non-constructible and
//! the eligibility transition is not an external API:
//!
//! ```compile_fail
//! use strategy_runtime_core::Stage8bP1d1CommandDecisionBinding;
//! let _ = Stage8bP1d1CommandDecisionBinding {
//!     predecessor_close_ts_utc_ms: 0,
//! };
//! ```
//!
//! ```compile_fail
//! use strategy_runtime_core::Stage8bP1d1CanonicalExecutionAuthority;
//! let _ = Stage8bP1d1CanonicalExecutionAuthority {
//!     is_live: true,
//! };
//! ```
//!
//! ```compile_fail
//! use strategy_runtime_core::{
//!     Stage8bP1d1AwaitingExecutionBar,
//!     Stage8bP1d1CanonicalExecutionAuthority,
//! };
//! let waiting: Stage8bP1d1AwaitingExecutionBar = unreachable!();
//! let candidate: Stage8bP1d1CanonicalExecutionAuthority = unreachable!();
//! let _ = waiting.observe_candidates(vec![candidate]);
//! ```

use broker_core::{
    BrokerCommand, BrokerOrderId, BrokerTradeId, InstrumentId, OrderType, StrategyRequestId,
    TimeInForce,
};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use sha2::{Digest, Sha256};

use crate::stage5e_no_io_lifecycle::schedule_window_evidence::{
    classify_stage8b_p1d1_execution_bar, Stage5eScheduleProjectionBridgeInput,
    Stage8bP1d1ScheduleBridgeBlockReason, Stage8bP1d1ScheduleProjectionApproved,
};
use crate::{
    Stage6DurableCommandSnapshotV1, Stage6DurableRequestIdentityV1, Stage6Sha256Digest,
    Stage6dPaperDispatchReceipt, Stage6dPaperOutcome,
};

pub const STAGE8B_P1D1_EXECUTION_POLICY_DOMAIN: &str = "moex.stage8b.p1d.execution-policy.v1";
pub const STAGE8B_P1D1_ORDER_ID_DOMAIN: &str = "moex.stage8b.p1d.order-id.v1";
pub const STAGE8B_P1D1_TRADE_ID_DOMAIN: &str = "moex.stage8b.p1d.trade-id.v1";

const M10_MILLIS: i64 = 600_000;

/// Source-produced decision-bar and exact accepted-command authority.  The
/// fields are intentionally private, the value is not Clone/serde, and its
/// only production issuer reads one authenticated P1 semantic projection plus
/// the matching Stage6 RequestAccepted record.
pub struct Stage8bP1d1CommandDecisionBinding {
    command: BrokerCommand,
    durable_identity: Stage6DurableRequestIdentityV1,
    durable_command_snapshot: Stage6DurableCommandSnapshotV1,
    accepted_command_payload_sha256: Stage6Sha256Digest,
    operational_identity_sha256: String,
    canonical_command_sha256: String,
    predecessor_redis_id: String,
    predecessor_semantic_id_sha256: String,
    predecessor_payload_sha256: String,
    predecessor_close_ts_utc_ms: i64,
}

/// Opaque exact canonical execution-bar authority.  No external constructor
/// or public field exists.  Until a separately reviewed cross-crate source
/// bridge is added, only this crate can mint it and invoke eligibility.
pub struct Stage8bP1d1CanonicalExecutionAuthority {
    strategy_request_id: StrategyRequestId,
    canonical_command_sha256: String,
    predecessor_redis_id: String,
    predecessor_semantic_id_sha256: String,
    predecessor_payload_sha256: String,
    source_redis_id: String,
    source_canonical_bytes_sha256: String,
    operational_identity_sha256: String,
    instrument: InstrumentId,
    semantic_id_sha256: String,
    payload_sha256: String,
    open_ts_utc_ms: i64,
    close_ts_utc_ms: i64,
    open: Decimal,
    timeframe_sec: u32,
    is_final: bool,
    is_live: bool,
}

/// Read-only canonical M10 evidence supplied by the Redis composition.  It
/// is not an execution capability; only the owned Stage 6 P1 authority may
/// consume it into eligibility.
pub struct Stage8bP1d1CanonicalM10Evidence {
    pub source_redis_id: String,
    pub source_canonical_bytes_sha256: String,
    pub operational_identity_sha256: String,
    pub instrument: InstrumentId,
    pub semantic_id_sha256: String,
    pub payload_sha256: String,
    pub open_ts_utc_ms: i64,
    pub close_ts_utc_ms: i64,
    pub open: Decimal,
}

/// Opaque one-use schedule authority for the P1-d1 execution observation.
///
/// Canonical Redis bytes cannot construct this value.  It owns the exact
/// Stage 5E projection produced from normalized schedule, registry and fresh
/// Stage 4 session evidence.  A later operational source adapter may carry
/// this value across the composition boundary without exposing calendar rows.
pub struct Stage8bP1d1ExecutionScheduleAuthority {
    projection: Stage5eScheduleProjectionBridgeInput,
}

/// Sole crate-internal bridge from a source-produced Stage 5E projection to
/// the public opaque authority accepted by the cross-crate P1 composition.
#[allow(
    dead_code,
    reason = "operational schedule source remains a later slice"
)]
pub(crate) fn stage8b_p1d1_schedule_authority_from_stage5e(
    projection: Stage5eScheduleProjectionBridgeInput,
) -> Stage8bP1d1ExecutionScheduleAuthority {
    Stage8bP1d1ExecutionScheduleAuthority { projection }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage8bP1d1ExecutionEligibilityBlockReason {
    MultipleCandidates,
    WrongRequest,
    WrongCommand,
    WrongPredecessor,
    WrongOperationalIdentity,
    WrongInstrument,
    NonCanonicalIdentity,
    HistoryOrWarmup,
    NonFinal,
    InvalidTimeframe,
    InvalidChronology,
    SameBar,
    CrossTradingDay,
    ExecutionBarGap,
    ScheduleNotYetObserved,
    ScheduleExpired,
    CandidateObservedInFuture,
    ScheduleRejected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Stage8bP1d1ProviderError {
    #[error("P1-d1 accepts Market PLACE commands only")]
    UnsupportedCommand,
    #[error("P1-d1 Market v1 accepts Day time-in-force only")]
    UnsupportedTimeInForce,
    #[error("P1-d1 policy forbids command TTL")]
    TtlForbidden,
    #[error("canonical command binding is invalid")]
    CommandBindingMismatch,
    #[error("durable dispatch receipt does not match eligibility")]
    DispatchBindingMismatch,
}

/// Retryable no-effect owner.  Its constructor is crate-private and consumes
/// the opaque Stage 5E schedule projection.  It is neither Clone nor
/// serializable.
pub struct Stage8bP1d1AwaitingExecutionBar {
    schedule_projection: Stage5eScheduleProjectionBridgeInput,
    decision: Stage8bP1d1CommandDecisionBinding,
}

/// Blocked observation keeps the linear wait owner but grants no dispatch or
/// provider method.  A refreshed exact observation may be retried explicitly.
pub struct Stage8bP1d1ExecutionEligibilityBlocked {
    reason: Stage8bP1d1ExecutionEligibilityBlockReason,
    awaiting: Stage8bP1d1AwaitingExecutionBar,
}

impl Stage8bP1d1ExecutionEligibilityBlocked {
    pub fn reason(&self) -> Stage8bP1d1ExecutionEligibilityBlockReason {
        self.reason
    }

    pub fn into_retry(self) -> Stage8bP1d1AwaitingExecutionBar {
        self.awaiting
    }
}

/// Observation has exactly three outcomes.  `NoInput` and `Blocked` retain
/// the wait owner and therefore have no durable-dispatch operation.
pub enum Stage8bP1d1ExecutionObservation {
    NoInput(Box<Stage8bP1d1AwaitingExecutionBar>),
    Eligible(Box<Stage8bP1d1ExecutionEligible>),
    Blocked(Box<Stage8bP1d1ExecutionEligibilityBlocked>),
}

/// Opaque exact next-bar capability.  It exposes no constructor and is
/// consumed when it is bound to the Stage 6 dispatch receipt.
pub struct Stage8bP1d1ExecutionEligible {
    decision: Stage8bP1d1CommandDecisionBinding,
    execution_bar: Stage8bP1d1CanonicalExecutionAuthority,
    schedule_approval: Stage8bP1d1ScheduleProjectionApproved,
}

/// Post-dispatch provider capability.  The receipt remains owned here until
/// the deterministic outcome is derived.
pub struct Stage8bP1d1MarketDispatchReady {
    eligibility: Stage8bP1d1ExecutionEligible,
    dispatch_receipt: Stage6dPaperDispatchReceipt,
}

/// Linear result retained for P1-d2 composition.  It contains the exact
/// Stage 6 outcome plus deterministic fill evidence, but cannot ACK, XACK or
/// mutate Hybrid state in this stage.
pub struct Stage8bP1d1MarketOutcomeBundle {
    dispatch_receipt: Stage6dPaperDispatchReceipt,
    stage6_outcome: Stage6dPaperOutcome,
    strategy_request_id: StrategyRequestId,
    canonical_command_sha256: String,
    accepted_command_payload_sha256: Stage6Sha256Digest,
    execution_bar_semantic_id_sha256: String,
    execution_bar_payload_sha256: String,
    fill_price: Decimal,
    fill_qty: Decimal,
    fill_source_ts_utc_ms: i64,
    fill_received_ts_utc_ms: i64,
    broker_order_id: BrokerOrderId,
    broker_trade_id: BrokerTradeId,
}

pub(crate) struct Stage8bP1d1MarketOutcomeEvidence {
    pub(crate) strategy_request_id: StrategyRequestId,
    pub(crate) canonical_command_sha256: String,
    pub(crate) accepted_command_payload_sha256: Stage6Sha256Digest,
    pub(crate) execution_bar_semantic_id_sha256: String,
    pub(crate) execution_bar_payload_sha256: String,
    pub(crate) fill_price: Decimal,
    pub(crate) fill_qty: Decimal,
    pub(crate) fill_source_ts_utc_ms: i64,
    pub(crate) fill_received_ts_utc_ms: i64,
    pub(crate) broker_order_id: BrokerOrderId,
    pub(crate) broker_trade_id: BrokerTradeId,
}

impl Stage8bP1d1MarketOutcomeBundle {
    pub fn strategy_request_id(&self) -> StrategyRequestId {
        self.strategy_request_id
    }

    pub fn canonical_command_sha256(&self) -> &str {
        &self.canonical_command_sha256
    }

    pub fn execution_bar_semantic_id_sha256(&self) -> &str {
        &self.execution_bar_semantic_id_sha256
    }

    pub fn execution_bar_payload_sha256(&self) -> &str {
        &self.execution_bar_payload_sha256
    }

    pub fn fill_price(&self) -> Decimal {
        self.fill_price
    }

    pub fn fill_qty(&self) -> Decimal {
        self.fill_qty
    }

    pub fn fill_source_ts_utc_ms(&self) -> i64 {
        self.fill_source_ts_utc_ms
    }

    pub fn fill_received_ts_utc_ms(&self) -> i64 {
        self.fill_received_ts_utc_ms
    }

    pub fn broker_order_id(&self) -> &BrokerOrderId {
        &self.broker_order_id
    }

    pub fn broker_trade_id(&self) -> &BrokerTradeId {
        &self.broker_trade_id
    }

    pub fn feedback_application_allowed(&self) -> bool {
        false
    }

    pub fn ack_allowed(&self) -> bool {
        false
    }

    pub fn source_m10_xack_allowed(&self) -> bool {
        false
    }

    #[allow(
        dead_code,
        reason = "opened only by the separately reviewed P1-d2 composition"
    )]
    pub(crate) fn into_stage6_parts(self) -> (Stage6dPaperDispatchReceipt, Stage6dPaperOutcome) {
        (self.dispatch_receipt, self.stage6_outcome)
    }

    pub(crate) fn into_p1d2_parts(
        self,
    ) -> (
        Stage6dPaperDispatchReceipt,
        Stage6dPaperOutcome,
        Stage8bP1d1MarketOutcomeEvidence,
    ) {
        let evidence = Stage8bP1d1MarketOutcomeEvidence {
            strategy_request_id: self.strategy_request_id,
            canonical_command_sha256: self.canonical_command_sha256,
            accepted_command_payload_sha256: self.accepted_command_payload_sha256,
            execution_bar_semantic_id_sha256: self.execution_bar_semantic_id_sha256,
            execution_bar_payload_sha256: self.execution_bar_payload_sha256,
            fill_price: self.fill_price,
            fill_qty: self.fill_qty,
            fill_source_ts_utc_ms: self.fill_source_ts_utc_ms,
            fill_received_ts_utc_ms: self.fill_received_ts_utc_ms,
            broker_order_id: self.broker_order_id,
            broker_trade_id: self.broker_trade_id,
        };
        (self.dispatch_receipt, self.stage6_outcome, evidence)
    }
}

/// Crate-internal issuer called only by the authenticated Stage6 recovered
/// owner.  No caller may supply predecessor identity and close time as
/// independent inputs to the wait transition.
#[allow(clippy::too_many_arguments)]
pub(crate) fn stage8b_p1d1_command_decision_binding_from_source(
    command: BrokerCommand,
    durable_identity: Stage6DurableRequestIdentityV1,
    durable_command_snapshot: Stage6DurableCommandSnapshotV1,
    accepted_command_payload_sha256: Stage6Sha256Digest,
    operational_identity_sha256: String,
    canonical_command_sha256: String,
    predecessor_redis_id: String,
    predecessor_semantic_id_sha256: String,
    predecessor_payload_sha256: String,
) -> Result<Stage8bP1d1CommandDecisionBinding, Stage8bP1d1ProviderError> {
    let predecessor_close_ts_utc_ms = exact_redis_close_ms(&predecessor_redis_id)
        .ok_or(Stage8bP1d1ProviderError::CommandBindingMismatch)?;
    let place = match &command {
        BrokerCommand::PlaceOrder(place) if place.order_type == OrderType::Market => place,
        _ => return Err(Stage8bP1d1ProviderError::UnsupportedCommand),
    };
    let exact_identity =
        Stage6DurableRequestIdentityV1::from_place(place, durable_identity.attribution().clone())
            .map_err(|_| Stage8bP1d1ProviderError::CommandBindingMismatch)?;
    let exact_snapshot = Stage6DurableCommandSnapshotV1::from_place(&exact_identity, place)
        .map_err(|_| Stage8bP1d1ProviderError::CommandBindingMismatch)?;
    let command_bytes = serde_json::to_vec(&command)
        .map_err(|_| Stage8bP1d1ProviderError::CommandBindingMismatch)?;
    if exact_identity != durable_identity
        || exact_snapshot != durable_command_snapshot
        || sha256_hex(&command_bytes) != canonical_command_sha256
        || !is_sha256(&operational_identity_sha256)
        || !is_sha256(&canonical_command_sha256)
        || !is_sha256(&predecessor_semantic_id_sha256)
        || !is_sha256(&predecessor_payload_sha256)
        || predecessor_close_ts_utc_ms <= 0
        || predecessor_close_ts_utc_ms.rem_euclid(M10_MILLIS) != 0
    {
        return Err(Stage8bP1d1ProviderError::CommandBindingMismatch);
    }
    Ok(Stage8bP1d1CommandDecisionBinding {
        command,
        durable_identity,
        durable_command_snapshot,
        accepted_command_payload_sha256,
        operational_identity_sha256,
        canonical_command_sha256,
        predecessor_redis_id,
        predecessor_semantic_id_sha256,
        predecessor_payload_sha256,
        predecessor_close_ts_utc_ms,
    })
}

impl Stage8bP1d1ExecutionEligible {
    pub(crate) fn durable_identity(&self) -> &Stage6DurableRequestIdentityV1 {
        &self.decision.durable_identity
    }

    pub(crate) fn durable_command_snapshot(&self) -> &Stage6DurableCommandSnapshotV1 {
        &self.decision.durable_command_snapshot
    }

    pub(crate) fn accepted_command_payload_sha256(&self) -> &Stage6Sha256Digest {
        &self.decision.accepted_command_payload_sha256
    }

    pub(crate) fn execution_close_ts_utc_ms(&self) -> i64 {
        self.execution_bar.close_ts_utc_ms
    }
}

impl Stage8bP1d1AwaitingExecutionBar {
    #[allow(
        dead_code,
        reason = "closed until the canonical P1 cross-crate source bridge is reviewed"
    )]
    pub(crate) fn observe_candidates(
        self,
        candidates: Vec<Stage8bP1d1CanonicalExecutionAuthority>,
        observed_at: DateTime<Utc>,
    ) -> Stage8bP1d1ExecutionObservation {
        if candidates.is_empty() {
            return Stage8bP1d1ExecutionObservation::NoInput(Box::new(self));
        }
        if candidates.len() != 1 {
            return self.block(Stage8bP1d1ExecutionEligibilityBlockReason::MultipleCandidates);
        }
        let candidate = candidates.into_iter().next().expect("length checked");
        if let Err(reason) = validate_execution_candidate(&self.decision, &candidate) {
            return self.block(reason);
        }

        let Stage8bP1d1AwaitingExecutionBar {
            schedule_projection,
            decision,
        } = self;
        match classify_stage8b_p1d1_execution_bar(
            schedule_projection,
            &candidate.instrument,
            decision.predecessor_close_ts_utc_ms.div_euclid(1_000),
            candidate.close_ts_utc_ms.div_euclid(1_000),
            observed_at,
        ) {
            Ok(schedule_approval) => {
                Stage8bP1d1ExecutionObservation::Eligible(Box::new(Stage8bP1d1ExecutionEligible {
                    decision,
                    execution_bar: candidate,
                    schedule_approval,
                }))
            }
            Err(blocked) => {
                let (reason, schedule_projection) = (*blocked).into_parts();
                let reason = map_schedule_block(reason);
                Stage8bP1d1ExecutionObservation::Blocked(Box::new(
                    Stage8bP1d1ExecutionEligibilityBlocked {
                        reason,
                        awaiting: Stage8bP1d1AwaitingExecutionBar {
                            schedule_projection,
                            decision,
                        },
                    },
                ))
            }
        }
    }

    fn block(
        self,
        reason: Stage8bP1d1ExecutionEligibilityBlockReason,
    ) -> Stage8bP1d1ExecutionObservation {
        Stage8bP1d1ExecutionObservation::Blocked(Box::new(Stage8bP1d1ExecutionEligibilityBlocked {
            reason,
            awaiting: self,
        }))
    }
}

fn validate_execution_candidate(
    decision: &Stage8bP1d1CommandDecisionBinding,
    candidate: &Stage8bP1d1CanonicalExecutionAuthority,
) -> Result<(), Stage8bP1d1ExecutionEligibilityBlockReason> {
    if candidate.strategy_request_id != command_request_id(&decision.command) {
        return Err(Stage8bP1d1ExecutionEligibilityBlockReason::WrongRequest);
    }
    if candidate.canonical_command_sha256 != decision.canonical_command_sha256 {
        return Err(Stage8bP1d1ExecutionEligibilityBlockReason::WrongCommand);
    }
    if candidate.predecessor_redis_id != decision.predecessor_redis_id
        || candidate.predecessor_semantic_id_sha256 != decision.predecessor_semantic_id_sha256
        || candidate.predecessor_payload_sha256 != decision.predecessor_payload_sha256
    {
        return Err(Stage8bP1d1ExecutionEligibilityBlockReason::WrongPredecessor);
    }
    if candidate.operational_identity_sha256 != decision.operational_identity_sha256 {
        return Err(Stage8bP1d1ExecutionEligibilityBlockReason::WrongOperationalIdentity);
    }
    let place = match &decision.command {
        BrokerCommand::PlaceOrder(place) => place,
        BrokerCommand::CancelOrder(_) => {
            return Err(Stage8bP1d1ExecutionEligibilityBlockReason::WrongCommand)
        }
    };
    if candidate.instrument != place.instrument {
        return Err(Stage8bP1d1ExecutionEligibilityBlockReason::WrongInstrument);
    }
    if !is_sha256(&candidate.semantic_id_sha256)
        || !is_sha256(&candidate.payload_sha256)
        || !is_sha256(&candidate.operational_identity_sha256)
        || !is_sha256(&candidate.source_canonical_bytes_sha256)
        || candidate.source_redis_id != format!("{}-0", candidate.close_ts_utc_ms)
    {
        return Err(Stage8bP1d1ExecutionEligibilityBlockReason::NonCanonicalIdentity);
    }
    if !candidate.is_live {
        return Err(Stage8bP1d1ExecutionEligibilityBlockReason::HistoryOrWarmup);
    }
    if !candidate.is_final {
        return Err(Stage8bP1d1ExecutionEligibilityBlockReason::NonFinal);
    }
    if candidate.timeframe_sec != 600 {
        return Err(Stage8bP1d1ExecutionEligibilityBlockReason::InvalidTimeframe);
    }
    if candidate.open_ts_utc_ms <= 0
        || candidate.close_ts_utc_ms <= 0
        || candidate.close_ts_utc_ms - candidate.open_ts_utc_ms != M10_MILLIS
        || candidate.close_ts_utc_ms.rem_euclid(M10_MILLIS) != 0
        || candidate.open <= Decimal::ZERO
    {
        return Err(Stage8bP1d1ExecutionEligibilityBlockReason::InvalidChronology);
    }
    if candidate.close_ts_utc_ms == decision.predecessor_close_ts_utc_ms {
        return Err(Stage8bP1d1ExecutionEligibilityBlockReason::SameBar);
    }
    if candidate.close_ts_utc_ms < decision.predecessor_close_ts_utc_ms {
        return Err(Stage8bP1d1ExecutionEligibilityBlockReason::InvalidChronology);
    }
    Ok(())
}

/// The only P1-d1 entry seam.  It remains crate-private until a later stage
/// wires the accepted P1-c owner to a source-produced schedule projection.
#[allow(
    dead_code,
    reason = "P1-d1 core is sealed until separately reviewed P1-d2 composition"
)]
pub(crate) fn begin_stage8b_p1d1_market_wait(
    schedule_projection: Stage5eScheduleProjectionBridgeInput,
    decision: Stage8bP1d1CommandDecisionBinding,
) -> Result<Stage8bP1d1AwaitingExecutionBar, Stage8bP1d1ProviderError> {
    let place = match &decision.command {
        BrokerCommand::PlaceOrder(place) if place.order_type == OrderType::Market => place,
        _ => return Err(Stage8bP1d1ProviderError::UnsupportedCommand),
    };
    if place.time_in_force != TimeInForce::Day {
        return Err(Stage8bP1d1ProviderError::UnsupportedTimeInForce);
    }
    if place.ttl_ms.is_some() {
        return Err(Stage8bP1d1ProviderError::TtlForbidden);
    }
    if place.limit_price.is_some()
        || place.qty <= Decimal::ZERO
        || !is_sha256(&decision.operational_identity_sha256)
        || !is_sha256(&decision.canonical_command_sha256)
        || !is_sha256(&decision.predecessor_semantic_id_sha256)
        || !is_sha256(&decision.predecessor_payload_sha256)
        || decision.predecessor_close_ts_utc_ms <= 0
        || decision.predecessor_close_ts_utc_ms.rem_euclid(M10_MILLIS) != 0
        || decision.predecessor_redis_id != format!("{}-0", decision.predecessor_close_ts_utc_ms)
    {
        return Err(Stage8bP1d1ProviderError::CommandBindingMismatch);
    }
    let bytes = serde_json::to_vec(&decision.command)
        .map_err(|_| Stage8bP1d1ProviderError::CommandBindingMismatch)?;
    if sha256_hex(&bytes) != decision.canonical_command_sha256 {
        return Err(Stage8bP1d1ProviderError::CommandBindingMismatch);
    }
    Ok(Stage8bP1d1AwaitingExecutionBar {
        schedule_projection,
        decision,
    })
}

fn canonical_execution_candidate(
    decision: &Stage8bP1d1CommandDecisionBinding,
    evidence: Stage8bP1d1CanonicalM10Evidence,
) -> Stage8bP1d1CanonicalExecutionAuthority {
    Stage8bP1d1CanonicalExecutionAuthority {
        strategy_request_id: command_request_id(&decision.command),
        canonical_command_sha256: decision.canonical_command_sha256.clone(),
        predecessor_redis_id: decision.predecessor_redis_id.clone(),
        predecessor_semantic_id_sha256: decision.predecessor_semantic_id_sha256.clone(),
        predecessor_payload_sha256: decision.predecessor_payload_sha256.clone(),
        source_redis_id: evidence.source_redis_id,
        source_canonical_bytes_sha256: evidence.source_canonical_bytes_sha256,
        operational_identity_sha256: evidence.operational_identity_sha256,
        instrument: evidence.instrument,
        semantic_id_sha256: evidence.semantic_id_sha256,
        payload_sha256: evidence.payload_sha256,
        open_ts_utc_ms: evidence.open_ts_utc_ms,
        close_ts_utc_ms: evidence.close_ts_utc_ms,
        open: evidence.open,
        timeframe_sec: 600,
        is_final: true,
        is_live: true,
    }
}

pub(crate) fn stage8b_p1d1_eligible_from_canonical_m10(
    decision: Stage8bP1d1CommandDecisionBinding,
    schedule_authority: Stage8bP1d1ExecutionScheduleAuthority,
    evidence: Stage8bP1d1CanonicalM10Evidence,
) -> Result<Stage8bP1d1ExecutionEligible, Stage8bP1d1ExecutionEligibilityBlockReason> {
    let waiting = begin_stage8b_p1d1_market_wait(schedule_authority.projection, decision)
        .map_err(|_| Stage8bP1d1ExecutionEligibilityBlockReason::WrongCommand)?;
    let observed_at = DateTime::<Utc>::from_timestamp_millis(evidence.close_ts_utc_ms)
        .ok_or(Stage8bP1d1ExecutionEligibilityBlockReason::InvalidChronology)?;
    let candidate = canonical_execution_candidate(&waiting.decision, evidence);
    match waiting.observe_candidates(vec![candidate], observed_at) {
        Stage8bP1d1ExecutionObservation::Eligible(eligible) => Ok(*eligible),
        Stage8bP1d1ExecutionObservation::Blocked(blocked) => Err(blocked.reason()),
        Stage8bP1d1ExecutionObservation::NoInput(_) => {
            Err(Stage8bP1d1ExecutionEligibilityBlockReason::NonCanonicalIdentity)
        }
    }
}

/// Internal half of the combined eligibility-gated Stage7 transition.  The
/// sole caller appends DispatchAttemptRecorded only after it has consumed the
/// eligibility and validated the exact accepted command snapshot.
pub(crate) fn bind_stage8b_p1d1_market_dispatch(
    eligibility: Stage8bP1d1ExecutionEligible,
    dispatch_receipt: Stage6dPaperDispatchReceipt,
) -> Result<Stage8bP1d1MarketDispatchReady, Stage8bP1d1ProviderError> {
    let place = match &eligibility.decision.command {
        BrokerCommand::PlaceOrder(place) => place,
        BrokerCommand::CancelOrder(_) => {
            return Err(Stage8bP1d1ProviderError::DispatchBindingMismatch)
        }
    };
    if dispatch_receipt.stage8b_p1d1_identity() != &eligibility.decision.durable_identity
        || dispatch_receipt.stage8b_p1d1_command_snapshot()
            != &eligibility.decision.durable_command_snapshot
        || dispatch_receipt.stage8b_p1d1_accepted_payload_sha256()
            != &eligibility.decision.accepted_command_payload_sha256
        || eligibility.decision.durable_identity.strategy_request_id() != place.request_id
    {
        return Err(Stage8bP1d1ProviderError::DispatchBindingMismatch);
    }
    Ok(Stage8bP1d1MarketDispatchReady {
        eligibility,
        dispatch_receipt,
    })
}

impl Stage8bP1d1MarketDispatchReady {
    /// Pure deterministic provider call.  IDs, price and source timestamp are
    /// all functions of already durable/validated inputs.
    pub fn execute(self) -> Stage8bP1d1MarketOutcomeBundle {
        let Stage8bP1d1MarketDispatchReady {
            eligibility,
            dispatch_receipt,
        } = self;
        let evidence = deterministic_market_outcome_evidence(
            &eligibility.decision,
            &eligibility.execution_bar,
        );
        let _opaque_schedule_binding = (
            eligibility.schedule_approval.identity_fingerprint(),
            eligibility.schedule_approval.expires_at(),
            &eligibility.decision.predecessor_semantic_id_sha256,
        );
        let stage6_outcome = Stage6dPaperOutcome::MarketFilled {
            broker_order_id: evidence.broker_order_id.clone(),
            broker_trade_id: evidence.broker_trade_id.clone(),
        };
        Stage8bP1d1MarketOutcomeBundle {
            dispatch_receipt,
            stage6_outcome,
            strategy_request_id: evidence.strategy_request_id,
            canonical_command_sha256: evidence.canonical_command_sha256,
            accepted_command_payload_sha256: evidence.accepted_command_payload_sha256,
            execution_bar_semantic_id_sha256: evidence.execution_bar_semantic_id_sha256,
            execution_bar_payload_sha256: evidence.execution_bar_payload_sha256,
            fill_price: evidence.fill_price,
            fill_qty: evidence.fill_qty,
            fill_source_ts_utc_ms: evidence.fill_source_ts_utc_ms,
            fill_received_ts_utc_ms: evidence.fill_received_ts_utc_ms,
            broker_order_id: evidence.broker_order_id,
            broker_trade_id: evidence.broker_trade_id,
        }
    }
}

/// Recovery-only deterministic reconstruction. The durable P1-specific
/// dispatch record proves that the opaque schedule authority was consumed
/// before the original append. Recovery therefore validates the exact
/// retained contiguous successor bytes and finalized journal IDs without
/// inventing or reacquiring calendar evidence, and owns no provider method.
pub(crate) fn reconstruct_stage8b_p1d1_market_outcome_evidence(
    decision: Stage8bP1d1CommandDecisionBinding,
    canonical_m10: Stage8bP1d1CanonicalM10Evidence,
) -> Result<Stage8bP1d1MarketOutcomeEvidence, Stage8bP1d1ProviderError> {
    let candidate = canonical_execution_candidate(&decision, canonical_m10);
    validate_execution_candidate(&decision, &candidate)
        .map_err(|_| Stage8bP1d1ProviderError::CommandBindingMismatch)?;
    let expected_close = decision
        .predecessor_close_ts_utc_ms
        .checked_add(M10_MILLIS)
        .ok_or(Stage8bP1d1ProviderError::CommandBindingMismatch)?;
    if candidate.close_ts_utc_ms != expected_close {
        return Err(Stage8bP1d1ProviderError::CommandBindingMismatch);
    }
    Ok(deterministic_market_outcome_evidence(&decision, &candidate))
}

fn deterministic_market_outcome_evidence(
    decision: &Stage8bP1d1CommandDecisionBinding,
    execution_bar: &Stage8bP1d1CanonicalExecutionAuthority,
) -> Stage8bP1d1MarketOutcomeEvidence {
    let place = match &decision.command {
        BrokerCommand::PlaceOrder(place) => place,
        BrokerCommand::CancelOrder(_) => unreachable!("eligibility is Market PLACE only"),
    };
    let broker_order_id = derive_order_id(
        &decision.operational_identity_sha256,
        place.request_id,
        &decision.canonical_command_sha256,
    );
    let broker_trade_id = derive_trade_id(&broker_order_id, &execution_bar.semantic_id_sha256);
    Stage8bP1d1MarketOutcomeEvidence {
        strategy_request_id: place.request_id,
        canonical_command_sha256: decision.canonical_command_sha256.clone(),
        accepted_command_payload_sha256: decision.accepted_command_payload_sha256.clone(),
        execution_bar_semantic_id_sha256: execution_bar.semantic_id_sha256.clone(),
        execution_bar_payload_sha256: execution_bar.payload_sha256.clone(),
        fill_price: execution_bar.open,
        fill_qty: place.qty,
        fill_source_ts_utc_ms: execution_bar.open_ts_utc_ms,
        fill_received_ts_utc_ms: execution_bar.close_ts_utc_ms,
        broker_order_id,
        broker_trade_id,
    }
}

fn command_request_id(command: &BrokerCommand) -> StrategyRequestId {
    match command {
        BrokerCommand::PlaceOrder(place) => place.request_id,
        BrokerCommand::CancelOrder(cancel) => cancel.request_id,
    }
}

fn map_schedule_block(
    reason: Stage8bP1d1ScheduleBridgeBlockReason,
) -> Stage8bP1d1ExecutionEligibilityBlockReason {
    match reason {
        Stage8bP1d1ScheduleBridgeBlockReason::InstrumentMismatch => {
            Stage8bP1d1ExecutionEligibilityBlockReason::WrongInstrument
        }
        Stage8bP1d1ScheduleBridgeBlockReason::NotYetObserved => {
            Stage8bP1d1ExecutionEligibilityBlockReason::ScheduleNotYetObserved
        }
        Stage8bP1d1ScheduleBridgeBlockReason::Expired => {
            Stage8bP1d1ExecutionEligibilityBlockReason::ScheduleExpired
        }
        Stage8bP1d1ScheduleBridgeBlockReason::CandidateInFuture => {
            Stage8bP1d1ExecutionEligibilityBlockReason::CandidateObservedInFuture
        }
        Stage8bP1d1ScheduleBridgeBlockReason::CrossTradingDay => {
            Stage8bP1d1ExecutionEligibilityBlockReason::CrossTradingDay
        }
        Stage8bP1d1ScheduleBridgeBlockReason::ExecutionBarGap => {
            Stage8bP1d1ExecutionEligibilityBlockReason::ExecutionBarGap
        }
        Stage8bP1d1ScheduleBridgeBlockReason::Rejected => {
            Stage8bP1d1ExecutionEligibilityBlockReason::ScheduleRejected
        }
    }
}

fn derive_order_id(
    operational_identity_sha256: &str,
    request_id: StrategyRequestId,
    canonical_command_sha256: &str,
) -> BrokerOrderId {
    BrokerOrderId::new(format!(
        "P1D-O-{}",
        nul_separated_sha256(&[
            STAGE8B_P1D1_ORDER_ID_DOMAIN,
            operational_identity_sha256,
            &request_id.to_string(),
            canonical_command_sha256,
        ])
    ))
}

fn derive_trade_id(
    broker_order_id: &BrokerOrderId,
    execution_bar_semantic_id_sha256: &str,
) -> BrokerTradeId {
    BrokerTradeId::new(format!(
        "P1D-T-{}",
        nul_separated_sha256(&[
            STAGE8B_P1D1_TRADE_ID_DOMAIN,
            broker_order_id.as_str(),
            execution_bar_semantic_id_sha256,
            "1",
        ])
    ))
}

fn nul_separated_sha256(fields: &[&str]) -> String {
    let mut hasher = Sha256::new();
    for (index, field) in fields.iter().enumerate() {
        if index != 0 {
            hasher.update([0]);
        }
        hasher.update(field.as_bytes());
    }
    hex_digest(hasher.finalize())
}

#[allow(
    dead_code,
    reason = "production caller remains sealed with the P1-d1 entry seam"
)]
fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex_digest(hasher.finalize())
}

fn hex_digest(bytes: impl AsRef<[u8]>) -> String {
    bytes
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value != "0".repeat(64)
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn exact_redis_close_ms(value: &str) -> Option<i64> {
    let (millis, sequence) = value.split_once('-')?;
    if sequence != "0" || millis.is_empty() || (millis.len() > 1 && millis.starts_with('0')) {
        return None;
    }
    let parsed = millis.parse::<i64>().ok()?;
    (parsed > 0 && value == format!("{parsed}-0")).then_some(parsed)
}

#[cfg(any(test, feature = "stage5g-artifact-fixtures"))]
#[doc(hidden)]
pub fn stage8b_p1d1_test_eligible(
    command: &BrokerCommand,
    accepted_command_payload_sha256: Stage6Sha256Digest,
    predecessor_close_ts_utc_ms: i64,
    candidate_close_ts_utc_ms: i64,
) -> Stage8bP1d1ExecutionEligible {
    let BrokerCommand::PlaceOrder(place) = command else {
        panic!("P1-d1 test eligibility supports PLACE only");
    };
    let attribution = broker_core::HybridRuntimeAttribution::parse_source_comment(
        place.comment.as_deref().expect("test command attribution"),
    )
    .expect("test attribution must validate");
    let identity = Stage6DurableRequestIdentityV1::from_place(place, attribution)
        .expect("test identity must validate");
    let snapshot = Stage6DurableCommandSnapshotV1::from_place(&identity, place)
        .expect("test snapshot must validate");
    let command_sha256 =
        sha256_hex(&serde_json::to_vec(command).expect("test command serialization must succeed"));
    let decision = stage8b_p1d1_command_decision_binding_from_source(
        command.clone(),
        identity,
        snapshot,
        accepted_command_payload_sha256,
        "1".repeat(64),
        command_sha256.clone(),
        format!("{predecessor_close_ts_utc_ms}-0"),
        "2".repeat(64),
        "7".repeat(64),
    )
    .expect("test decision binding must validate");
    stage8b_p1d1_test_eligible_from_decision(decision, candidate_close_ts_utc_ms)
}

#[cfg(any(test, feature = "stage5g-artifact-fixtures"))]
#[doc(hidden)]
pub fn stage8b_p1d1_test_schedule_authority(
    instrument: InstrumentId,
    predecessor_close_ts_utc_ms: i64,
    candidate_close_ts_utc_ms: i64,
) -> Stage8bP1d1ExecutionScheduleAuthority {
    let projection = crate::stage5e_no_io_lifecycle::schedule_window_evidence::
        stage8b_p1d1_test_schedule_projection(
            instrument,
            predecessor_close_ts_utc_ms.div_euclid(1_000),
            candidate_close_ts_utc_ms.div_euclid(1_000),
            false,
        );
    stage8b_p1d1_schedule_authority_from_stage5e(projection)
}

#[cfg(any(test, feature = "stage5g-artifact-fixtures"))]
#[doc(hidden)]
pub fn stage8b_p1d1_test_eligible_from_decision(
    decision: Stage8bP1d1CommandDecisionBinding,
    candidate_close_ts_utc_ms: i64,
) -> Stage8bP1d1ExecutionEligible {
    let BrokerCommand::PlaceOrder(place) = &decision.command else {
        panic!("P1-d1 test eligibility supports PLACE only");
    };
    let predecessor_close_ts_utc_ms = decision.predecessor_close_ts_utc_ms;
    let command_sha256 = decision.canonical_command_sha256.clone();
    let operational_identity_sha256 = decision.operational_identity_sha256.clone();
    let predecessor_redis_id = decision.predecessor_redis_id.clone();
    let predecessor_semantic_id_sha256 = decision.predecessor_semantic_id_sha256.clone();
    let predecessor_payload_sha256 = decision.predecessor_payload_sha256.clone();
    let instrument = place.instrument.clone();
    let request_id = place.request_id;
    let schedule_projection = crate::stage5e_no_io_lifecycle::schedule_window_evidence::
        stage8b_p1d1_test_schedule_projection(
            instrument.clone(),
            predecessor_close_ts_utc_ms.div_euclid(1_000),
            candidate_close_ts_utc_ms.div_euclid(1_000),
            false,
        );
    let waiting = begin_stage8b_p1d1_market_wait(schedule_projection, decision)
        .expect("test wait must validate");
    let candidate = Stage8bP1d1CanonicalExecutionAuthority {
        strategy_request_id: request_id,
        canonical_command_sha256: command_sha256,
        predecessor_redis_id,
        predecessor_semantic_id_sha256,
        predecessor_payload_sha256,
        source_redis_id: format!("{candidate_close_ts_utc_ms}-0"),
        source_canonical_bytes_sha256: "8".repeat(64),
        operational_identity_sha256,
        instrument,
        semantic_id_sha256: "3".repeat(64),
        payload_sha256: "4".repeat(64),
        open_ts_utc_ms: candidate_close_ts_utc_ms - M10_MILLIS,
        close_ts_utc_ms: candidate_close_ts_utc_ms,
        open: Decimal::new(2_175, 1),
        timeframe_sec: 600,
        is_final: true,
        is_live: true,
    };
    match waiting.observe_candidates(
        vec![candidate],
        DateTime::<Utc>::from_timestamp_millis(candidate_close_ts_utc_ms)
            .expect("test timestamp must validate"),
    ) {
        Stage8bP1d1ExecutionObservation::Eligible(eligible) => *eligible,
        _ => panic!("test candidate must be eligible"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use broker_core::{
        BrokerAccountId, ClientOrderId, Exchange, HybridRuntimeAttribution, Market, OrderSide,
        PlaceOrder,
    };
    use chrono::TimeZone;
    use uuid::Uuid;

    fn instrument() -> InstrumentId {
        InstrumentId {
            symbol: "IMOEXF".to_string(),
            venue_symbol: Some("IMOEXF@RTSX".to_string()),
            exchange: Exchange::Moex,
            market: Market::Futures,
        }
    }

    fn command(ttl_ms: Option<u64>) -> BrokerCommand {
        let request_id = StrategyRequestId::new(
            Uuid::parse_str("12345678-1234-5678-9234-567812345678").unwrap(),
        );
        let attribution = HybridRuntimeAttribution::parse_source_comment(
            "HYB|sid=hybrid_imoexf|c=cycle-p1d1|o=BO|r=ENTRY",
        )
        .unwrap();
        BrokerCommand::PlaceOrder(PlaceOrder {
            request_id,
            created_ts: Utc.with_ymd_and_hms(2026, 9, 3, 8, 50, 0).single().unwrap(),
            ttl_ms,
            account_id: BrokerAccountId::new("ACC_TEST_0001"),
            client_order_id: ClientOrderId::from_strategy_request(request_id),
            instrument: instrument(),
            side: OrderSide::Buy,
            order_type: OrderType::Market,
            qty: Decimal::ONE,
            limit_price: None,
            time_in_force: TimeInForce::Day,
            comment: Some(attribution.internal_comment().to_string()),
        })
    }

    fn command_sha(command: &BrokerCommand) -> String {
        sha256_hex(&serde_json::to_vec(command).unwrap())
    }

    fn decision(
        command: &BrokerCommand,
        predecessor_close_ms: i64,
    ) -> Stage8bP1d1CommandDecisionBinding {
        let BrokerCommand::PlaceOrder(place) = command else {
            panic!("P1-d1 fixture supports PLACE only");
        };
        let attribution = HybridRuntimeAttribution::parse_source_comment(
            place.comment.as_deref().expect("fixture attribution"),
        )
        .unwrap();
        let identity = Stage6DurableRequestIdentityV1::from_place(place, attribution).unwrap();
        let snapshot = Stage6DurableCommandSnapshotV1::from_place(&identity, place).unwrap();
        let accepted_payload_sha256 =
            Stage6Sha256Digest::parse(sha256_hex(&serde_json::to_vec(&snapshot).unwrap())).unwrap();
        stage8b_p1d1_command_decision_binding_from_source(
            command.clone(),
            identity,
            snapshot,
            accepted_payload_sha256,
            "1".repeat(64),
            command_sha(command),
            format!("{predecessor_close_ms}-0"),
            "2".repeat(64),
            "7".repeat(64),
        )
        .unwrap()
    }

    fn candidate_authority(
        command: &BrokerCommand,
        predecessor_close_ms: i64,
        candidate_close_ms: i64,
    ) -> Stage8bP1d1CanonicalExecutionAuthority {
        Stage8bP1d1CanonicalExecutionAuthority {
            strategy_request_id: command_request_id(command),
            canonical_command_sha256: command_sha(command),
            predecessor_redis_id: format!("{predecessor_close_ms}-0"),
            predecessor_semantic_id_sha256: "2".repeat(64),
            predecessor_payload_sha256: "7".repeat(64),
            source_redis_id: format!("{candidate_close_ms}-0"),
            source_canonical_bytes_sha256: "8".repeat(64),
            operational_identity_sha256: "1".repeat(64),
            instrument: instrument(),
            semantic_id_sha256: "3".repeat(64),
            payload_sha256: "4".repeat(64),
            open_ts_utc_ms: candidate_close_ms - M10_MILLIS,
            close_ts_utc_ms: candidate_close_ms,
            open: Decimal::new(2_175, 1),
            timeframe_sec: 600,
            is_final: true,
            is_live: true,
        }
    }

    fn waiting(
        predecessor_close_ms: i64,
        candidate_close_ms: i64,
        boundary: bool,
    ) -> Stage8bP1d1AwaitingExecutionBar {
        let command = command(None);
        waiting_for(&command, predecessor_close_ms, candidate_close_ms, boundary)
    }

    fn waiting_for(
        command: &BrokerCommand,
        predecessor_close_ms: i64,
        candidate_close_ms: i64,
        boundary: bool,
    ) -> Stage8bP1d1AwaitingExecutionBar {
        let projection = crate::stage5e_no_io_lifecycle::schedule_window_evidence::
            stage8b_p1d1_test_schedule_projection(
                instrument(),
                predecessor_close_ms.div_euclid(1_000),
                candidate_close_ms.div_euclid(1_000),
                boundary,
            );
        begin_stage8b_p1d1_market_wait(projection, decision(command, predecessor_close_ms)).unwrap()
    }

    fn observed_at(candidate_close_ms: i64) -> DateTime<Utc> {
        Utc.timestamp_millis_opt(candidate_close_ms)
            .single()
            .unwrap()
    }

    fn eligible(
        predecessor_close_ms: i64,
        candidate_close_ms: i64,
        boundary: bool,
    ) -> Stage8bP1d1ExecutionEligible {
        eligible_at(
            predecessor_close_ms,
            candidate_close_ms,
            boundary,
            candidate_close_ms,
        )
    }

    fn eligible_at(
        predecessor_close_ms: i64,
        candidate_close_ms: i64,
        boundary: bool,
        observed_at_ms: i64,
    ) -> Stage8bP1d1ExecutionEligible {
        let command = command(None);
        let wait = waiting_for(&command, predecessor_close_ms, candidate_close_ms, boundary);
        match wait.observe_candidates(
            vec![candidate_authority(
                &command,
                predecessor_close_ms,
                candidate_close_ms,
            )],
            observed_at(observed_at_ms),
        ) {
            Stage8bP1d1ExecutionObservation::Eligible(value) => *value,
            _ => panic!("fixture must be eligible"),
        }
    }

    #[test]
    fn p1d1_no_input_and_same_bar_never_mint_eligibility() {
        let predecessor = 1_788_422_400_000;
        let candidate = predecessor + M10_MILLIS;
        let wait = waiting(predecessor, candidate, false);
        let wait = match wait.observe_candidates(Vec::new(), observed_at(candidate)) {
            Stage8bP1d1ExecutionObservation::NoInput(wait) => *wait,
            _ => panic!("empty observation must remain pending"),
        };
        let command = command(None);
        let blocked = match wait.observe_candidates(
            vec![candidate_authority(&command, predecessor, predecessor)],
            observed_at(candidate),
        ) {
            Stage8bP1d1ExecutionObservation::Blocked(blocked) => blocked,
            _ => panic!("same-bar execution must be blocked"),
        };
        assert_eq!(
            blocked.reason(),
            Stage8bP1d1ExecutionEligibilityBlockReason::SameBar
        );
    }

    #[test]
    fn p1d1_contiguous_and_clearing_boundary_are_the_only_positive_paths() {
        let predecessor = 1_788_422_400_000;
        let command = command(None);
        assert!(matches!(
            waiting(predecessor, predecessor + M10_MILLIS, false).observe_candidates(
                vec![candidate_authority(
                    &command,
                    predecessor,
                    predecessor + M10_MILLIS
                )],
                observed_at(predecessor + M10_MILLIS),
            ),
            Stage8bP1d1ExecutionObservation::Eligible(_)
        ));
        assert!(matches!(
            waiting(predecessor, predecessor + 3 * M10_MILLIS, true).observe_candidates(
                vec![candidate_authority(
                    &command,
                    predecessor,
                    predecessor + 3 * M10_MILLIS
                )],
                observed_at(predecessor + 3 * M10_MILLIS),
            ),
            Stage8bP1d1ExecutionObservation::Eligible(_)
        ));
        let blocked = match waiting(predecessor, predecessor + 3 * M10_MILLIS, false)
            .observe_candidates(
                vec![candidate_authority(
                    &command,
                    predecessor,
                    predecessor + 3 * M10_MILLIS,
                )],
                observed_at(predecessor + 3 * M10_MILLIS),
            ) {
            Stage8bP1d1ExecutionObservation::Blocked(value) => value,
            _ => panic!("skipped tradable M10 must be a gap"),
        };
        assert_eq!(
            blocked.reason(),
            Stage8bP1d1ExecutionEligibilityBlockReason::ExecutionBarGap
        );
    }

    #[test]
    fn p1d1_wrong_binding_history_and_multiple_candidates_fail_closed() {
        let predecessor = 1_788_422_400_000;
        let candidate = predecessor + M10_MILLIS;
        let command = command(None);
        let cases = [
            (
                {
                    let mut value = candidate_authority(&command, predecessor, candidate);
                    value.strategy_request_id = StrategyRequestId::new(
                        Uuid::parse_str("87654321-4321-8765-9321-876543218765").unwrap(),
                    );
                    value
                },
                Stage8bP1d1ExecutionEligibilityBlockReason::WrongRequest,
            ),
            (
                {
                    let mut value = candidate_authority(&command, predecessor, candidate);
                    value.canonical_command_sha256 = "5".repeat(64);
                    value
                },
                Stage8bP1d1ExecutionEligibilityBlockReason::WrongCommand,
            ),
            (
                {
                    let mut value = candidate_authority(&command, predecessor, candidate);
                    value.predecessor_semantic_id_sha256 = "6".repeat(64);
                    value
                },
                Stage8bP1d1ExecutionEligibilityBlockReason::WrongPredecessor,
            ),
            (
                {
                    let mut value = candidate_authority(&command, predecessor, candidate);
                    value.is_live = false;
                    value
                },
                Stage8bP1d1ExecutionEligibilityBlockReason::HistoryOrWarmup,
            ),
        ];
        for (candidate_authority, expected) in cases {
            let blocked = match waiting(predecessor, candidate, false)
                .observe_candidates(vec![candidate_authority], observed_at(candidate))
            {
                Stage8bP1d1ExecutionObservation::Blocked(value) => value,
                _ => panic!("mutation must block"),
            };
            assert_eq!(blocked.reason(), expected);
        }

        let first = candidate_authority(&command, predecessor, candidate);
        let second = candidate_authority(&command, predecessor, candidate);
        let blocked = match waiting(predecessor, candidate, false)
            .observe_candidates(vec![first, second], observed_at(candidate))
        {
            Stage8bP1d1ExecutionObservation::Blocked(value) => value,
            _ => panic!("multiple candidates must block"),
        };
        assert_eq!(
            blocked.reason(),
            Stage8bP1d1ExecutionEligibilityBlockReason::MultipleCandidates
        );
    }

    #[test]
    fn p1d1_ttl_is_fail_closed_before_wait_authority() {
        let command = command(Some(30_000));
        let predecessor: i64 = 1_788_422_400_000;
        let projection = crate::stage5e_no_io_lifecycle::schedule_window_evidence::
            stage8b_p1d1_test_schedule_projection(
                instrument(),
                predecessor.div_euclid(1_000),
                (predecessor + M10_MILLIS).div_euclid(1_000),
                false,
            );
        assert!(matches!(
            begin_stage8b_p1d1_market_wait(projection, decision(&command, predecessor),),
            Err(Stage8bP1d1ProviderError::TtlForbidden)
        ));
    }

    #[test]
    fn p1d1_schedule_freshness_future_and_cross_day_fail_closed() {
        let predecessor: i64 = 1_788_422_400_000;
        let candidate = predecessor + M10_MILLIS;
        let command = command(None);

        let not_yet = match waiting(predecessor, candidate, false).observe_candidates(
            vec![candidate_authority(&command, predecessor, candidate)],
            observed_at(predecessor - 1_000),
        ) {
            Stage8bP1d1ExecutionObservation::Blocked(value) => value,
            _ => panic!("schedule used before observation must block"),
        };
        assert_eq!(
            not_yet.reason(),
            Stage8bP1d1ExecutionEligibilityBlockReason::ScheduleNotYetObserved
        );

        let future = match waiting(predecessor, candidate, false).observe_candidates(
            vec![candidate_authority(&command, predecessor, candidate)],
            observed_at(candidate - 1_000),
        ) {
            Stage8bP1d1ExecutionObservation::Blocked(value) => value,
            _ => panic!("future candidate must block"),
        };
        assert_eq!(
            future.reason(),
            Stage8bP1d1ExecutionEligibilityBlockReason::CandidateObservedInFuture
        );

        let expired = match waiting(predecessor, candidate, false).observe_candidates(
            vec![candidate_authority(&command, predecessor, candidate)],
            observed_at(candidate + 3_601_000),
        ) {
            Stage8bP1d1ExecutionObservation::Blocked(value) => value,
            _ => panic!("expired schedule must block"),
        };
        assert_eq!(
            expired.reason(),
            Stage8bP1d1ExecutionEligibilityBlockReason::ScheduleExpired
        );

        let midnight = Utc
            .with_ymd_and_hms(2026, 9, 4, 0, 0, 0)
            .single()
            .unwrap()
            .timestamp_millis();
        let prior_day = midnight - M10_MILLIS;
        let cross_day = match waiting(prior_day, midnight, false).observe_candidates(
            vec![candidate_authority(&command, prior_day, midnight)],
            observed_at(midnight),
        ) {
            Stage8bP1d1ExecutionObservation::Blocked(value) => value,
            _ => panic!("cross-day sequence must block"),
        };
        assert_eq!(
            cross_day.reason(),
            Stage8bP1d1ExecutionEligibilityBlockReason::CrossTradingDay
        );
    }

    #[test]
    fn p1d1_market_result_is_exact_and_deterministic_after_dispatch() {
        let predecessor = 1_788_422_400_000;
        let candidate = predecessor + M10_MILLIS;
        let first = eligible(predecessor, candidate, false);
        let second = eligible_at(predecessor, candidate, false, candidate + 30_000);
        let command = command(None);
        let first_receipt = crate::stage6d_live_core::stage8b_p1d1_test_dispatch_receipt(&command);
        let second_receipt = crate::stage6d_live_core::stage8b_p1d1_test_dispatch_receipt(&command);
        let first = bind_stage8b_p1d1_market_dispatch(first, first_receipt)
            .unwrap()
            .execute();
        let second = bind_stage8b_p1d1_market_dispatch(second, second_receipt)
            .unwrap()
            .execute();
        assert_eq!(first.fill_price(), Decimal::new(2_175, 1));
        assert_eq!(first.fill_source_ts_utc_ms(), candidate - M10_MILLIS);
        assert_eq!(first.fill_qty(), Decimal::ONE);
        assert_eq!(first.broker_order_id(), second.broker_order_id());
        assert_eq!(first.broker_trade_id(), second.broker_trade_id());
        assert_eq!(first.broker_order_id().as_str().len(), 70);
        assert_eq!(first.broker_trade_id().as_str().len(), 70);
        assert!(!first.feedback_application_allowed());
        assert!(!first.ack_allowed());
        assert!(!first.source_m10_xack_allowed());
    }

    #[test]
    fn p1d1_full_dispatch_snapshot_mismatches_fail_closed() {
        let predecessor = 1_788_422_400_000;
        let candidate_close = predecessor + M10_MILLIS;
        let baseline = command(None);
        let mut mutations = Vec::new();

        let mut qty = baseline.clone();
        let BrokerCommand::PlaceOrder(place) = &mut qty else {
            unreachable!()
        };
        place.qty = Decimal::new(2, 0);
        mutations.push(qty);

        let mut side = baseline.clone();
        let BrokerCommand::PlaceOrder(place) = &mut side else {
            unreachable!()
        };
        place.side = OrderSide::Sell;
        mutations.push(side);

        let mut attribution = baseline.clone();
        let BrokerCommand::PlaceOrder(place) = &mut attribution else {
            unreachable!()
        };
        place.comment = Some("HYB|sid=hybrid_imoexf|c=different-cycle|o=BO|r=ENTRY".to_string());
        mutations.push(attribution);

        let mut created_ts = baseline.clone();
        let BrokerCommand::PlaceOrder(place) = &mut created_ts else {
            unreachable!()
        };
        place.created_ts += chrono::Duration::seconds(1);
        mutations.push(created_ts);

        for mutation in mutations {
            let eligibility = eligible(predecessor, candidate_close, false);
            let receipt = crate::stage6d_live_core::stage8b_p1d1_test_dispatch_receipt(&mutation);
            assert!(matches!(
                bind_stage8b_p1d1_market_dispatch(eligibility, receipt),
                Err(Stage8bP1d1ProviderError::DispatchBindingMismatch)
            ));
        }

        let eligibility = eligible(predecessor, candidate_close, false);
        let receipt =
            crate::stage6d_live_core::stage8b_p1d1_test_dispatch_receipt_with_payload_sha256(
                &baseline,
                Stage6Sha256Digest::parse("f".repeat(64)).unwrap(),
            );
        assert!(matches!(
            bind_stage8b_p1d1_market_dispatch(eligibility, receipt),
            Err(Stage8bP1d1ProviderError::DispatchBindingMismatch)
        ));
    }
}
