//! Stage 8B-P1-d3 broker-neutral working LIMIT/CANCEL lifecycle core.
//!
//! This module is deliberately pure. It owns no Redis, filesystem, clock,
//! schedule parser, Hybrid callback, broker transport or source-XACK API.
//! Opaque schedule capabilities are converted to full canonical evidence by
//! the owning composition before any durable or runtime effect is allowed.

use broker_core::command::CommandAckStatus;
use broker_core::{
    BrokerAccountId, BrokerCommand, BrokerOrderId, BrokerOrderLifecycle, BrokerOrderSnapshot,
    BrokerPositionSnapshot, BrokerTradeId, BrokerTradeSnapshot, BrokerTruthSnapshot, CancelOrder,
    ClientOrderId, CommandAck, CommandAckReason, CommandAckReasonCode, HybridRuntimeAttribution,
    InstrumentId, OrderSide, OrderStatus, OrderType, PlaceOrder, StrategyRequestId, TimeInForce,
};
use chrono::{DateTime, TimeZone, Utc};
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

use crate::stage5g_order_position::Stage5gOrderPositionState;

pub const STAGE8B_P1D3_WORKING_BOOK_SCHEMA_VERSION: u16 = 1;
pub const STAGE8B_P1D3_OUTCOME_EVIDENCE_SCHEMA_VERSION: u16 = 1;
pub const P1D3_MAX_ORDER_RECORDS_PER_GENERATION: usize = 1024;

pub const STAGE8B_P1D3_WORKING_BOOK_DOMAIN: &str = "moex.stage8b.p1d3.working-book.v1";
pub const STAGE8B_P1D3_BOOK_TRANSITION_DOMAIN: &str = "moex.stage8b.p1d3.book-transition.v1";
pub const STAGE8B_P1D3_OUTCOME_EVIDENCE_DOMAIN: &str = "moex.stage8b.p1d3.outcome-evidence.v1";
pub const STAGE8B_P1D3_BOOK_GENESIS_DOMAIN: &str = "moex.stage8b.p1d3.book-genesis.v1";
pub const STAGE8B_P1D3_EVALUATION_EVIDENCE_DOMAIN: &str =
    "moex.stage8b.p1d3.evaluation-evidence.v1";

const STAGE8B_P1D3_ORDER_FINGERPRINT_DOMAIN: &str = "moex.stage8b.p1d3.order-fingerprint.v1";
const STAGE8B_P1D3_TRADE_ID_DOMAIN: &str = "moex.stage8b.p1d.trade-id.v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Stage8bP1d3BookPhase {
    Migrated,
    Ack,
    Working,
    Eval,
    Terminal,
    CancelRecovered,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Stage8bP1d3OutcomeKind {
    InitialWorking,
    InitialFilled,
    InitialExpired,
    LaterFilled,
    LaterExpired,
    CancelCanceled,
    CancelExecutionObserved,
    CancelAlreadyTerminalNonExecution,
}

impl Stage8bP1d3OutcomeKind {
    pub(crate) fn canonical_name(self) -> &'static str {
        match self {
            Self::InitialWorking => "initial_working",
            Self::InitialFilled => "initial_filled",
            Self::InitialExpired => "initial_expired",
            Self::LaterFilled => "later_filled",
            Self::LaterExpired => "later_expired",
            Self::CancelCanceled => "cancel_canceled",
            Self::CancelExecutionObserved => "cancel_execution_observed",
            Self::CancelAlreadyTerminalNonExecution => "cancel_already_terminal_non_execution",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Stage8bP1d3ConsumedWitnessKind {
    ScheduleStep,
    DayExpiry,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Stage8bP1d3LimitDecision {
    Working,
    Filled,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Stage8bP1d3CanonicalM10Evidence {
    pub redis_id: String,
    pub semantic_id_sha256: String,
    pub payload_sha256: String,
    pub canonical_bytes_sha256: String,
    pub operational_identity_sha256: String,
    pub instrument: InstrumentId,
    pub open_ts_utc_ms: i64,
    pub close_ts_utc_ms: i64,
    pub open: Decimal,
    pub high: Decimal,
    pub low: Decimal,
    pub close: Decimal,
}

/// Exact canonical M10 identity whose order evaluation is already durable but
/// whose same-bar Hybrid callback has not necessarily been committed yet.
/// This value is read-only recovery material; it carries no schedule,
/// provider, callback, journal or source-acknowledgement authority.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stage8bP1d3SemanticSourceBinding {
    redis_id: String,
    semantic_id_sha256: String,
    payload_sha256: String,
}

impl Stage8bP1d3SemanticSourceBinding {
    pub(crate) fn from_authenticated_parts(
        redis_id: String,
        semantic_id_sha256: String,
        payload_sha256: String,
    ) -> Self {
        Self {
            redis_id,
            semantic_id_sha256,
            payload_sha256,
        }
    }

    pub fn redis_id(&self) -> &str {
        &self.redis_id
    }

    pub fn semantic_id_sha256(&self) -> &str {
        &self.semantic_id_sha256
    }

    pub fn payload_sha256(&self) -> &str {
        &self.payload_sha256
    }

    pub(crate) fn matches_semantic_commit(
        &self,
        semantic: &crate::stage5g_p1_semantic::Stage5gP1SemanticCommitProjectionV1,
    ) -> bool {
        semantic.validate()
            && semantic.m10_redis_id == self.redis_id
            && semantic.m10_semantic_id_sha256 == self.semantic_id_sha256
            && semantic.m10_payload_sha256 == self.payload_sha256
    }
}

/// One-use authority issued only by the Stage 5E schedule owner. It is
/// intentionally non-Clone and non-serializable.
pub struct Stage8bP1d3ScheduleStepAuthority {
    schedule_fingerprint_sha256: String,
    trading_day_identity: String,
    last_eligible_m10_redis_id: String,
    predecessor_redis_id: String,
    candidate_redis_id: String,
}

/// One-use boundary authority issued only after Stage 5E proves that the
/// exact last eligible M10 has already been evaluated.
pub struct Stage8bP1d3DayExpiryAuthority {
    schedule_fingerprint_sha256: String,
    trading_day_identity: String,
    last_eligible_m10_redis_id: String,
    boundary_ts_utc_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Stage8bP1d3OrderRecordV1 {
    broker_order_id: BrokerOrderId,
    original_request_id: StrategyRequestId,
    original_durable_place_client_id: ClientOrderId,
    canonical_command_sha256: String,
    accepted_stage6_identity_sha256: String,
    deterministic_order_fingerprint_sha256: String,
    account_id: BrokerAccountId,
    instrument: InstrumentId,
    attribution: HybridRuntimeAttribution,
    side: OrderSide,
    qty_decimal_bytes: [u8; 16],
    limit_price_decimal_bytes: [u8; 16],
    status: OrderStatus,
    lifecycle: BrokerOrderLifecycle,
    filled_qty_decimal_bytes: [u8; 16],
    remaining_qty_decimal_bytes: [u8; 16],
    decision_m10_redis_id: String,
    decision_m10_semantic_id_sha256: String,
    decision_m10_payload_sha256: String,
    decision_m10_open_ts_utc_ms: i64,
    decision_m10_close_ts_utc_ms: i64,
    first_observation_redis_id: Option<String>,
    first_observation_semantic_id_sha256: Option<String>,
    first_observation_payload_sha256: Option<String>,
    last_evaluated_m10_redis_id: Option<String>,
    last_evaluated_m10_semantic_id_sha256: Option<String>,
    last_evaluated_m10_payload_sha256: Option<String>,
    last_evaluated_close_ts_utc_ms: Option<i64>,
    schedule_fingerprint_sha256: String,
    trading_day_identity: String,
    latest_order_projection_sha256: String,
    broker_trade_id: Option<BrokerTradeId>,
    total_sequence_frontier: u64,
    transition_ordinal: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Stage8bP1d3WorkingBookProjectionV1 {
    schema_version: u16,
    identity_domain: String,
    transition_domain: String,
    operational_identity_sha256: String,
    package_generation: u64,
    account_id: BrokerAccountId,
    instrument: InstrumentId,
    attribution: HybridRuntimeAttribution,
    records: Vec<Stage8bP1d3OrderRecordV1>,
    active_broker_order_id: Option<BrokerOrderId>,
    transition_ordinal: u64,
    previous_transition_sha256: String,
    latest_transition_sha256: String,
    latest_outcome_evidence_sha256: String,
    total_sequence_frontier: u64,
    phase: Stage8bP1d3BookPhase,
}

/// Fixed-order, map-free write-ahead fact. Decimal values use the exact
/// rust_decimal 16-byte representation and optional values remain explicit
/// JSON nulls.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Stage8bP1d3OutcomeEvidenceV1 {
    schema_version: u16,
    domain: String,
    outcome_kind: Stage8bP1d3OutcomeKind,
    transition_ordinal: u64,
    operational_identity_sha256: String,
    package_generation: u64,
    account_id: BrokerAccountId,
    instrument: InstrumentId,
    attribution_fingerprint_sha256: String,
    request_id: Option<StrategyRequestId>,
    durable_request_client_id: Option<ClientOrderId>,
    canonical_command_sha256: Option<String>,
    accepted_command_payload_sha256: Option<String>,
    accepted_stage6_identity_sha256: String,
    target_place_client_id: Option<ClientOrderId>,
    target_broker_order_id: Option<BrokerOrderId>,
    source_m10_redis_id: Option<String>,
    source_m10_semantic_id_sha256: Option<String>,
    source_m10_payload_sha256: Option<String>,
    source_m10_open_ts_utc_ms: Option<i64>,
    source_m10_close_ts_utc_ms: Option<i64>,
    candidate_m10_redis_id: Option<String>,
    candidate_m10_semantic_id_sha256: Option<String>,
    candidate_m10_payload_sha256: Option<String>,
    candidate_m10_open_ts_utc_ms: Option<i64>,
    candidate_m10_close_ts_utc_ms: Option<i64>,
    consumed_witness_kind: Stage8bP1d3ConsumedWitnessKind,
    schedule_fingerprint_sha256: String,
    trading_day_identity: String,
    last_eligible_m10_redis_id: String,
    boundary_ts_utc_ms: Option<i64>,
    pre_book_generation: u64,
    pre_book_sha256: String,
    expected_post_book_sha256: String,
    broker_order_id: BrokerOrderId,
    broker_trade_id: Option<BrokerTradeId>,
    side: OrderSide,
    qty_decimal_bytes: [u8; 16],
    limit_price_decimal_bytes: [u8; 16],
    fill_price_decimal_bytes: Option<[u8; 16]>,
    pre_position_qty_decimal_bytes: [u8; 16],
    pre_position_avg_price_decimal_bytes: Option<[u8; 16]>,
    transition_source_ts_utc_ms: i64,
    transition_received_ts_utc_ms: i64,
    reserved_seq_ack: Option<u64>,
    reserved_seq_truth: Option<u64>,
    sequence_allocation_frontier: u64,
    stage6_dispatch_record_id: Option<String>,
    stage6_outcome_record_id: String,
    stage6_predecessor_frontier_sha256: String,
    stage6_reserved_checkpoint_sha256: String,
    stage7_request_finalized_record_id: Option<String>,
    stage7_request_finalized_fingerprint_sha256: Option<String>,
    previous_outcome_evidence_sha256: String,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Stage8bP1d3TransitionPlan {
    pub(crate) evidence: Stage8bP1d3OutcomeEvidenceV1,
    pub(crate) evidence_bytes: Vec<u8>,
    pub(crate) evidence_sha256: String,
    pub(crate) post_book: Stage8bP1d3WorkingBookProjectionV1,
    pub(crate) ack: Option<CommandAck>,
    pub(crate) truth: Option<BrokerTruthSnapshot>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Stage8bP1d3Error {
    #[error("P1-d3 identity or binding is invalid")]
    IdentityMismatch,
    #[error("P1-d3 chronology is invalid")]
    InvalidChronology,
    #[error("P1-d3 decimal authority is invalid")]
    InvalidDecimal,
    #[error("P1-d3 registry is invalid or exhausted")]
    InvalidRegistry,
    #[error("P1-d3 transition is invalid")]
    InvalidTransition,
    #[error("P1-d3 sequence allocation is invalid")]
    InvalidSequence,
    #[error("P1-d3 canonical encoding is invalid")]
    InvalidEncoding,
    #[error("P1-d3 authenticated replacement package failed")]
    RestartPackage,
    #[error("P1-d3 canonical Hybrid callback failed")]
    Callback,
}

pub struct Stage8bP1d3MigrationResult {
    pub restored: crate::Stage5gCleanRestartedCapability,
    pub restart_package: Vec<u8>,
}

/// Exact first post-decision observation.  Canonical bar evidence is data;
/// the accompanying schedule values are opaque one-use authorities.
pub enum Stage8bP1d3InitialObservation {
    Candidate {
        evidence: Box<Stage8bP1d3CanonicalM10Evidence>,
        schedule: Stage8bP1d3ScheduleStepAuthority,
    },
    DayExpiry {
        authority: Stage8bP1d3DayExpiryAuthority,
    },
}

/// Exact later observation for an already Working LIMIT.  As with the
/// initial observation, schedule ownership remains opaque and one-use.
pub enum Stage8bP1d3LaterObservation {
    Candidate {
        evidence: Box<Stage8bP1d3CanonicalM10Evidence>,
        schedule: Stage8bP1d3ScheduleStepAuthority,
    },
    DayExpiry {
        authority: Stage8bP1d3DayExpiryAuthority,
    },
}

impl Stage8bP1d3LaterObservation {
    pub(crate) fn transition_ts_utc_ms(&self) -> i64 {
        match self {
            Self::Candidate { evidence, .. } => evidence.close_ts_utc_ms,
            Self::DayExpiry { authority } => authority.boundary_ts_utc_ms,
        }
    }
}

impl Stage8bP1d3InitialObservation {
    pub(crate) fn transition_ts_utc_ms(&self) -> i64 {
        match self {
            Self::Candidate { evidence, .. } => evidence.close_ts_utc_ms,
            Self::DayExpiry { authority } => authority.boundary_ts_utc_ms,
        }
    }
}

pub(crate) struct Stage8bP1d3ReplacementStageResult {
    pub(crate) restored: crate::Stage5gCleanRestartedCapability,
    pub(crate) restart_package: Vec<u8>,
}

pub(crate) enum Stage8bP1d3EvaluationStageResult {
    AlreadyEvaluated,
    Committed(Box<Stage8bP1d3ReplacementStageResult>),
}

/// One-time quiescent conversion of an authenticated accepted P1-d2
/// S_truth package into the empty P1-d3 working-book generation. It consumes
/// the predecessor capability and neither allocates a sequence nor invokes a
/// provider, callback, dispatch or source-XACK surface.
pub(crate) fn migrate_stage8b_p1d3_from_p1d2(
    restored_p1d2_truth: crate::Stage5gCleanRestartedCapability,
    operational_identity_sha256: String,
    package_generation: u64,
    authenticated_stage6_checkpoint_sha256: String,
    commitment_key: &crate::Stage5gLifecycleCommitmentKey,
) -> Result<Stage8bP1d3MigrationResult, Stage8bP1d3Error> {
    if !is_sha256(&authenticated_stage6_checkpoint_sha256) {
        return Err(Stage8bP1d3Error::IdentityMismatch);
    }
    let accepted_p1d2_package_commitment_sha256 =
        restored_p1d2_truth.stage5g_pre_restart_package_fingerprint_sha256();
    let export_input = restored_p1d2_truth
        .stage8b_p1d3_migration_export_input(&accepted_p1d2_package_commitment_sha256)
        .map_err(|_| Stage8bP1d3Error::RestartPackage)?;
    let fresh_runtime = restored_p1d2_truth.stage5g_fresh_reconstruction_candidate();
    let (runtime, state, attribution) = restored_p1d2_truth
        .into_stage8b_p1d3_migration_parts()
        .map_err(|_| Stage8bP1d3Error::RestartPackage)?;
    let (_, account_id, instrument) =
        crate::Stage5gOrderPositionSession::stage5g_restart_state_binding(&state);
    let total_sequence_frontier =
        crate::Stage5gOrderPositionSession::stage8b_p1d3_total_sequence_frontier(&state);
    let book = Stage8bP1d3WorkingBookProjectionV1::migrate_from_p1d2(
        &accepted_p1d2_package_commitment_sha256,
        operational_identity_sha256,
        package_generation,
        account_id.clone(),
        instrument.clone(),
        attribution,
        total_sequence_frontier,
    )?;
    let projection =
        Stage8bP1d3ReplacementProjectionV1::migrated(book, authenticated_stage6_checkpoint_sha256)?;
    let source = Stage8bP1d3RestartSource::new(runtime, state, projection)?;
    let restart_package = crate::export_stage5g_clean_restart(
        crate::Stage5gCleanRestartSource::P1d3(Box::new(source)),
        export_input,
        commitment_key,
    )
    .map_err(|_| Stage8bP1d3Error::RestartPackage)?;
    let restored =
        crate::restore_stage5g_clean_restart(&restart_package, commitment_key, fresh_runtime)
            .map_err(|_| Stage8bP1d3Error::RestartPackage)?;
    if restored
        .stage8b_p1d3_replacement()
        .map_or(true, |replacement| {
            replacement.phase() != Stage8bP1d3BookPhase::Migrated
        })
    {
        return Err(Stage8bP1d3Error::RestartPackage);
    }
    Ok(Stage8bP1d3MigrationResult {
        restored,
        restart_package,
    })
}

#[derive(Debug, Clone)]
pub(crate) struct Stage8bP1d3InitialLimitInput {
    pub operational_identity_sha256: String,
    pub package_generation: u64,
    pub account_id: BrokerAccountId,
    pub instrument: InstrumentId,
    pub attribution: HybridRuntimeAttribution,
    pub request_id: StrategyRequestId,
    pub durable_client_order_id: ClientOrderId,
    pub canonical_command_sha256: String,
    pub accepted_command_payload_sha256: String,
    pub accepted_stage6_identity_sha256: String,
    pub decision_m10_redis_id: String,
    pub decision_m10_semantic_id_sha256: String,
    pub decision_m10_payload_sha256: String,
    pub decision_m10_open_ts_utc_ms: i64,
    pub decision_m10_close_ts_utc_ms: i64,
    pub side: OrderSide,
    pub qty: Decimal,
    pub limit_price: Decimal,
    pub pre_position_qty: Decimal,
    pub pre_position_avg_price: Option<Decimal>,
    pub sequence_allocation_frontier: u64,
    pub stage6_dispatch_record_id: String,
    pub stage6_predecessor_frontier_sha256: String,
    pub stage6_reserved_checkpoint_sha256: String,
    pub previous_outcome_evidence_sha256: String,
}

#[derive(Debug, Clone)]
pub(crate) struct Stage8bP1d3AutonomousInput {
    pub operational_identity_sha256: String,
    pub package_generation: u64,
    pub account_id: BrokerAccountId,
    pub instrument: InstrumentId,
    pub attribution: HybridRuntimeAttribution,
    pub pre_position_qty: Decimal,
    pub pre_position_avg_price: Option<Decimal>,
    pub sequence_allocation_frontier: u64,
    pub stage6_predecessor_frontier_sha256: String,
    pub stage6_reserved_checkpoint_sha256: String,
    pub previous_outcome_evidence_sha256: String,
}

#[derive(Debug, Clone)]
pub(crate) struct Stage8bP1d3CancelInput {
    pub operational_identity_sha256: String,
    pub package_generation: u64,
    pub account_id: BrokerAccountId,
    pub instrument: InstrumentId,
    pub attribution: HybridRuntimeAttribution,
    pub request_id: StrategyRequestId,
    pub durable_request_client_id: ClientOrderId,
    pub canonical_command_sha256: String,
    pub accepted_command_payload_sha256: String,
    pub accepted_stage6_identity_sha256: String,
    pub target_broker_order_id: BrokerOrderId,
    pub target_place_client_id: Option<ClientOrderId>,
    pub decision_m10_redis_id: String,
    pub decision_m10_semantic_id_sha256: String,
    pub decision_m10_payload_sha256: String,
    pub decision_m10_open_ts_utc_ms: i64,
    pub decision_m10_close_ts_utc_ms: i64,
    pub pre_position_qty: Decimal,
    pub pre_position_avg_price: Option<Decimal>,
    pub sequence_allocation_frontier: u64,
    pub stage6_dispatch_record_id: String,
    pub stage6_predecessor_frontier_sha256: String,
    pub target_stage6_reserved_checkpoint_sha256: Option<String>,
    pub stage6_reserved_checkpoint_sha256: String,
    pub previous_outcome_evidence_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Stage8bP1d3EvaluationEvidenceV1 {
    schema_version: u16,
    domain: String,
    operational_identity_sha256: String,
    package_generation: u64,
    account_id: BrokerAccountId,
    instrument: InstrumentId,
    attribution_fingerprint_sha256: String,
    broker_order_id: BrokerOrderId,
    candidate_m10_redis_id: String,
    candidate_m10_semantic_id_sha256: String,
    candidate_m10_payload_sha256: String,
    candidate_m10_canonical_bytes_sha256: String,
    candidate_m10_open_ts_utc_ms: i64,
    candidate_m10_close_ts_utc_ms: i64,
    schedule_fingerprint_sha256: String,
    trading_day_identity: String,
    last_eligible_m10_redis_id: String,
    pre_book_sha256: String,
    expected_post_book_sha256: String,
    sequence_frontier: u64,
    transition_ordinal: u64,
    previous_outcome_evidence_sha256: String,
}

/// Authenticated Stage 5G extension that replaces the prior package.  The
/// working book and its current continuation phase are committed together;
/// this is deliberately not a separately writable book sidecar.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Stage8bP1d3ReplacementProjectionV1 {
    schema_version: u16,
    phase: Stage8bP1d3BookPhase,
    working_book: Stage8bP1d3WorkingBookProjectionV1,
    latest_outcome_evidence: Option<Stage8bP1d3OutcomeEvidenceV1>,
    latest_outcome_evidence_sha256: Option<String>,
    latest_evaluation_evidence: Option<Stage8bP1d3EvaluationEvidenceV1>,
    latest_evaluation_evidence_sha256: Option<String>,
    outcome_chain_head_sha256: String,
    latest_stage6_reservation_sha256: String,
    authenticated_stage6_checkpoint_sha256: String,
}

/// Linear source consumed by the existing Stage 5G exporter.  It owns the
/// runtime and order/position state together with the P1-d3 replacement
/// projection, so export cannot accidentally persist only one half.
pub struct Stage8bP1d3RestartSource {
    runtime: crate::HybridIntradayRuntimeStrategy,
    state: Stage5gOrderPositionState,
    projection: Stage8bP1d3ReplacementProjectionV1,
    semantic_commit: Option<crate::stage5g_p1_semantic::Stage5gP1SemanticCommitProjectionV1>,
}

impl Stage8bP1d3ReplacementProjectionV1 {
    pub(crate) fn migrated(
        working_book: Stage8bP1d3WorkingBookProjectionV1,
        authenticated_stage6_checkpoint_sha256: String,
    ) -> Result<Self, Stage8bP1d3Error> {
        let outcome_chain_head_sha256 = working_book.latest_outcome_evidence_sha256.clone();
        let value = Self {
            schema_version: 1,
            phase: Stage8bP1d3BookPhase::Migrated,
            working_book,
            latest_outcome_evidence: None,
            latest_outcome_evidence_sha256: None,
            latest_evaluation_evidence: None,
            latest_evaluation_evidence_sha256: None,
            outcome_chain_head_sha256,
            latest_stage6_reservation_sha256: authenticated_stage6_checkpoint_sha256.clone(),
            authenticated_stage6_checkpoint_sha256,
        };
        value.validate()?;
        Ok(value)
    }

    pub(crate) fn for_transition(
        phase: Stage8bP1d3BookPhase,
        working_book: Stage8bP1d3WorkingBookProjectionV1,
        evidence: Stage8bP1d3OutcomeEvidenceV1,
        latest_stage6_reservation_sha256: String,
        authenticated_stage6_checkpoint_sha256: String,
    ) -> Result<Self, Stage8bP1d3Error> {
        if phase == Stage8bP1d3BookPhase::Migrated {
            return Err(Stage8bP1d3Error::InvalidTransition);
        }
        let evidence_sha256 = evidence.digest_sha256()?;
        let latest_outcome_evidence_sha256 = Some(evidence_sha256.clone());
        let value = Self {
            schema_version: 1,
            phase,
            working_book,
            latest_outcome_evidence: Some(evidence),
            latest_outcome_evidence_sha256,
            latest_evaluation_evidence: None,
            latest_evaluation_evidence_sha256: None,
            outcome_chain_head_sha256: evidence_sha256,
            latest_stage6_reservation_sha256,
            authenticated_stage6_checkpoint_sha256,
        };
        value.validate()?;
        Ok(value)
    }

    pub(crate) fn for_evaluation(
        working_book: Stage8bP1d3WorkingBookProjectionV1,
        evidence: Stage8bP1d3EvaluationEvidenceV1,
        outcome_chain_head_sha256: String,
        latest_stage6_reservation_sha256: String,
        authenticated_stage6_checkpoint_sha256: String,
    ) -> Result<Self, Stage8bP1d3Error> {
        let latest_evaluation_evidence_sha256 = Some(evidence.digest_sha256()?);
        let value = Self {
            schema_version: 1,
            phase: Stage8bP1d3BookPhase::Eval,
            working_book,
            latest_outcome_evidence: None,
            latest_outcome_evidence_sha256: None,
            latest_evaluation_evidence: Some(evidence),
            latest_evaluation_evidence_sha256,
            outcome_chain_head_sha256,
            latest_stage6_reservation_sha256,
            authenticated_stage6_checkpoint_sha256,
        };
        value.validate()?;
        Ok(value)
    }

    pub(crate) fn validate(&self) -> Result<(), Stage8bP1d3Error> {
        if self.schema_version != 1
            || !is_sha256(&self.latest_stage6_reservation_sha256)
            || !is_sha256(&self.outcome_chain_head_sha256)
            || !is_sha256(&self.authenticated_stage6_checkpoint_sha256)
        {
            return Err(Stage8bP1d3Error::IdentityMismatch);
        }
        self.working_book.validate()?;
        let outcome_pair = (
            self.phase,
            self.latest_outcome_evidence.as_ref(),
            self.latest_outcome_evidence_sha256.as_deref(),
        );
        let evaluation_pair = (
            self.latest_evaluation_evidence.as_ref(),
            self.latest_evaluation_evidence_sha256.as_deref(),
        );
        match (outcome_pair, evaluation_pair) {
            ((Stage8bP1d3BookPhase::Migrated, None, None), (None, None)) => {
                if self.working_book.phase != Stage8bP1d3BookPhase::Migrated
                    || self.outcome_chain_head_sha256
                        != self.working_book.latest_outcome_evidence_sha256
                {
                    return Err(Stage8bP1d3Error::InvalidTransition);
                }
            }
            ((Stage8bP1d3BookPhase::Eval, None, None), (Some(evidence), Some(digest))) => {
                evidence.validate()?;
                if evidence.digest_sha256()? != digest
                    || self.outcome_chain_head_sha256 != evidence.previous_outcome_evidence_sha256
                    || self.outcome_chain_head_sha256
                        != self.working_book.latest_outcome_evidence_sha256
                    || evidence.operational_identity_sha256
                        != self.working_book.operational_identity_sha256
                    || evidence.package_generation != self.working_book.package_generation
                    || evidence.account_id != self.working_book.account_id
                    || evidence.instrument != self.working_book.instrument
                    || self.working_book.phase != Stage8bP1d3BookPhase::Eval
                    || post_book_state_sha256(&self.working_book)?
                        != evidence.expected_post_book_sha256
                    || self.working_book.total_sequence_frontier != evidence.sequence_frontier
                {
                    return Err(Stage8bP1d3Error::InvalidTransition);
                }
            }
            ((Stage8bP1d3BookPhase::Migrated, _, _), _)
            | ((Stage8bP1d3BookPhase::Eval, _, _), _)
            | ((_, None, _), _)
            | ((_, _, None), _)
            | (_, (Some(_), None))
            | (_, (None, Some(_)))
            | (_, (Some(_), Some(_))) => return Err(Stage8bP1d3Error::InvalidTransition),
            ((phase, Some(evidence), Some(digest)), (None, None)) => {
                evidence.validate()?;
                if evidence.digest_sha256()? != digest
                    || self.outcome_chain_head_sha256 != *digest
                    || evidence.operational_identity_sha256
                        != self.working_book.operational_identity_sha256
                    || evidence.package_generation != self.working_book.package_generation
                    || evidence.account_id != self.working_book.account_id
                    || evidence.instrument != self.working_book.instrument
                    || evidence.stage6_reserved_checkpoint_sha256
                        != self.latest_stage6_reservation_sha256
                {
                    return Err(Stage8bP1d3Error::IdentityMismatch);
                }
                let book_sha256 = self.working_book.canonical_sha256()?;
                match phase {
                    Stage8bP1d3BookPhase::Ack => {
                        if book_sha256 != evidence.pre_book_sha256 {
                            return Err(Stage8bP1d3Error::InvalidTransition);
                        }
                    }
                    Stage8bP1d3BookPhase::Working | Stage8bP1d3BookPhase::Terminal => {
                        if post_book_state_sha256(&self.working_book)?
                            != evidence.expected_post_book_sha256
                            || self.working_book.latest_outcome_evidence_sha256 != *digest
                        {
                            return Err(Stage8bP1d3Error::InvalidTransition);
                        }
                    }
                    Stage8bP1d3BookPhase::CancelRecovered => {
                        if book_sha256 != evidence.pre_book_sha256
                            || book_sha256 != evidence.expected_post_book_sha256
                            || !matches!(
                                evidence.outcome_kind,
                                Stage8bP1d3OutcomeKind::CancelExecutionObserved
                                    | Stage8bP1d3OutcomeKind::CancelAlreadyTerminalNonExecution
                            )
                        {
                            return Err(Stage8bP1d3Error::InvalidTransition);
                        }
                    }
                    Stage8bP1d3BookPhase::Eval => unreachable!(),
                    Stage8bP1d3BookPhase::Migrated => unreachable!(),
                }
            }
        }
        Ok(())
    }

    pub(crate) fn phase(&self) -> Stage8bP1d3BookPhase {
        self.phase
    }

    pub(crate) fn working_book(&self) -> &Stage8bP1d3WorkingBookProjectionV1 {
        &self.working_book
    }

    pub(crate) fn authenticated_stage6_checkpoint_sha256(&self) -> &str {
        &self.authenticated_stage6_checkpoint_sha256
    }

    pub(crate) fn rebind_semantic_request_checkpoint(
        mut self,
        expected_pre_checkpoint_sha256: &str,
        authenticated_post_checkpoint_sha256: String,
    ) -> Result<Self, Stage8bP1d3Error> {
        self.validate()?;
        if self.authenticated_stage6_checkpoint_sha256 != expected_pre_checkpoint_sha256
            || !is_sha256(&authenticated_post_checkpoint_sha256)
            || authenticated_post_checkpoint_sha256 == expected_pre_checkpoint_sha256
        {
            return Err(Stage8bP1d3Error::IdentityMismatch);
        }
        self.authenticated_stage6_checkpoint_sha256 = authenticated_post_checkpoint_sha256;
        self.validate()?;
        Ok(self)
    }

    #[allow(dead_code, reason = "retained as a projection audit accessor")]
    pub(crate) fn latest_stage6_reservation_sha256(&self) -> &str {
        &self.latest_stage6_reservation_sha256
    }

    pub(crate) fn previous_outcome_evidence_sha256(&self) -> &str {
        &self.outcome_chain_head_sha256
    }

    pub(crate) fn latest_outcome_evidence(&self) -> Option<&Stage8bP1d3OutcomeEvidenceV1> {
        self.latest_outcome_evidence.as_ref()
    }

    pub(crate) fn pending_semantic_source_binding(
        &self,
    ) -> Result<Option<Stage8bP1d3SemanticSourceBinding>, Stage8bP1d3Error> {
        self.validate()?;
        let binding = match self.phase {
            Stage8bP1d3BookPhase::Eval => {
                let evidence = self
                    .latest_evaluation_evidence
                    .as_ref()
                    .ok_or(Stage8bP1d3Error::InvalidTransition)?;
                Some(Stage8bP1d3SemanticSourceBinding {
                    redis_id: evidence.candidate_m10_redis_id.clone(),
                    semantic_id_sha256: evidence.candidate_m10_semantic_id_sha256.clone(),
                    payload_sha256: evidence.candidate_m10_payload_sha256.clone(),
                })
            }
            Stage8bP1d3BookPhase::Working
            | Stage8bP1d3BookPhase::Terminal
            | Stage8bP1d3BookPhase::CancelRecovered => {
                let evidence = self
                    .latest_outcome_evidence
                    .as_ref()
                    .ok_or(Stage8bP1d3Error::InvalidTransition)?;
                if evidence.candidate_m10_redis_id.is_some() {
                    Some(Stage8bP1d3SemanticSourceBinding {
                        redis_id: evidence
                            .candidate_m10_redis_id
                            .clone()
                            .ok_or(Stage8bP1d3Error::InvalidTransition)?,
                        semantic_id_sha256: evidence
                            .candidate_m10_semantic_id_sha256
                            .clone()
                            .ok_or(Stage8bP1d3Error::InvalidTransition)?,
                        payload_sha256: evidence
                            .candidate_m10_payload_sha256
                            .clone()
                            .ok_or(Stage8bP1d3Error::InvalidTransition)?,
                    })
                } else {
                    None
                }
            }
            Stage8bP1d3BookPhase::Migrated | Stage8bP1d3BookPhase::Ack => None,
        };
        Ok(binding)
    }

    pub(crate) fn pending_cancel_after_target_matches_semantic_commit(
        &self,
        semantic: Option<&crate::stage5g_p1_semantic::Stage5gP1SemanticCommitProjectionV1>,
    ) -> Result<bool, Stage8bP1d3Error> {
        self.validate()?;
        let Some(evidence) = self.latest_outcome_evidence.as_ref() else {
            return Ok(false);
        };
        let Some(semantic) = semantic.filter(|semantic| semantic.validate()) else {
            return Ok(false);
        };
        let Some(BrokerCommand::CancelOrder(cancel)) = semantic.canonical_command.as_ref() else {
            return Ok(false);
        };
        let Some(identity) = semantic.durable_request_identity.as_ref() else {
            return Ok(false);
        };
        Ok(self.phase == Stage8bP1d3BookPhase::Terminal
            && evidence.outcome_kind == Stage8bP1d3OutcomeKind::LaterFilled
            && evidence.request_id.is_none()
            && cancel.request_id == identity.strategy_request_id()
            && cancel.account_id == self.working_book.account_id
            && cancel.order_id == evidence.broker_order_id
            && identity.action() == crate::Stage6DurableActionKind::Cancel
            && identity.target_broker_order_id() == Some(&evidence.broker_order_id)
            && identity.target_order_client_order_id() == evidence.target_place_client_id.as_ref()
            && evidence
                .candidate_m10_close_ts_utc_ms
                .zip(
                    semantic
                        .m10_redis_id
                        .strip_suffix("-0")
                        .and_then(|value| value.parse::<i64>().ok()),
                )
                .is_some_and(|(candidate, cancel_source)| candidate > cancel_source))
    }

    pub(crate) fn completed_command_matches_semantic_commit(
        &self,
        semantic: Option<&crate::stage5g_p1_semantic::Stage5gP1SemanticCommitProjectionV1>,
    ) -> Result<bool, Stage8bP1d3Error> {
        self.validate()?;
        let Some(request_id) = self
            .latest_outcome_evidence
            .as_ref()
            .and_then(|evidence| evidence.request_id)
        else {
            return Ok(false);
        };
        Ok(semantic
            .is_some_and(|semantic| semantic.validate() && semantic.request_id == Some(request_id)))
    }

    fn transition_plan_from_authenticated_package(
        &self,
    ) -> Result<Stage8bP1d3TransitionPlan, Stage8bP1d3Error> {
        self.validate()?;
        if self.phase != Stage8bP1d3BookPhase::Ack {
            return Err(Stage8bP1d3Error::InvalidTransition);
        }
        let evidence = self
            .latest_outcome_evidence
            .clone()
            .ok_or(Stage8bP1d3Error::InvalidTransition)?;
        let canonical_bytes = evidence.encode_canonical()?;
        recover_stage8b_p1d3_outcome_transition(
            &self.working_book,
            Stage8bP1d3AuthenticatedOutcomeEvidence {
                evidence,
                canonical_bytes,
            },
        )
    }
}

impl Stage8bP1d3RestartSource {
    pub(crate) fn new(
        runtime: crate::HybridIntradayRuntimeStrategy,
        state: Stage5gOrderPositionState,
        projection: Stage8bP1d3ReplacementProjectionV1,
    ) -> Result<Self, Stage8bP1d3Error> {
        Self::with_semantic_commit(runtime, state, projection, None)
    }

    pub(crate) fn with_semantic_commit(
        runtime: crate::HybridIntradayRuntimeStrategy,
        state: Stage5gOrderPositionState,
        projection: Stage8bP1d3ReplacementProjectionV1,
        semantic_commit: Option<crate::stage5g_p1_semantic::Stage5gP1SemanticCommitProjectionV1>,
    ) -> Result<Self, Stage8bP1d3Error> {
        projection.validate()?;
        if semantic_commit
            .as_ref()
            .is_some_and(|semantic| !semantic.validate())
        {
            return Err(Stage8bP1d3Error::IdentityMismatch);
        }
        let (_, account_id, instrument) =
            crate::Stage5gOrderPositionSession::stage5g_restart_state_binding(&state);
        if account_id != projection.working_book.account_id()
            || instrument != projection.working_book.instrument()
        {
            return Err(Stage8bP1d3Error::IdentityMismatch);
        }
        Ok(Self {
            runtime,
            state,
            projection,
            semantic_commit,
        })
    }

    pub(crate) fn rebind_semantic_request_checkpoint(
        mut self,
        expected_pre_checkpoint_sha256: &str,
        authenticated_post_checkpoint_sha256: String,
    ) -> Result<Self, Stage8bP1d3Error> {
        self.projection = self.projection.rebind_semantic_request_checkpoint(
            expected_pre_checkpoint_sha256,
            authenticated_post_checkpoint_sha256,
        )?;
        Ok(self)
    }

    pub(crate) fn runtime(&self) -> &crate::HybridIntradayRuntimeStrategy {
        &self.runtime
    }

    pub(crate) fn binding(&self) -> (&str, &BrokerAccountId, &InstrumentId) {
        crate::Stage5gOrderPositionSession::stage5g_restart_state_binding(&self.state)
    }

    pub(crate) fn summary(&self) -> crate::Stage5gOrderPositionSummary {
        crate::Stage5gOrderPositionSession::stage5g_restart_summary_from_state(
            &self.state,
            usize::from(self.semantic_commit.is_some()),
        )
    }

    pub(crate) fn checkpoint(&self) -> crate::Stage5gTimerCheckpointEnvelope {
        crate::Stage5gOrderPositionSession::stage5g_restart_checkpoint_from_state(&self.state)
    }

    pub(crate) fn state(&self) -> Stage5gOrderPositionState {
        self.state.clone()
    }

    pub(crate) fn projection(&self) -> &Stage8bP1d3ReplacementProjectionV1 {
        &self.projection
    }

    pub(crate) fn semantic_commit(
        &self,
    ) -> Option<&crate::stage5g_p1_semantic::Stage5gP1SemanticCommitProjectionV1> {
        self.semantic_commit.as_ref()
    }
}

fn export_stage8b_p1d3_replacement(
    runtime: crate::HybridIntradayRuntimeStrategy,
    state: Stage5gOrderPositionState,
    projection: Stage8bP1d3ReplacementProjectionV1,
    semantic_commit: Option<crate::stage5g_p1_semantic::Stage5gP1SemanticCommitProjectionV1>,
    export_input: crate::Stage5gCleanRestartExportInput,
    fresh_runtime: crate::HybridIntradayRuntimeStrategy,
    commitment_key: &crate::Stage5gLifecycleCommitmentKey,
) -> Result<Stage8bP1d3ReplacementStageResult, Stage8bP1d3Error> {
    let expected_phase = projection.phase();
    let source = Stage8bP1d3RestartSource::with_semantic_commit(
        runtime,
        state,
        projection,
        semantic_commit,
    )?;
    let restart_package = crate::export_stage5g_clean_restart(
        crate::Stage5gCleanRestartSource::P1d3(Box::new(source)),
        export_input,
        commitment_key,
    )
    .map_err(|_| Stage8bP1d3Error::RestartPackage)?;
    let restored =
        crate::restore_stage5g_clean_restart(&restart_package, commitment_key, fresh_runtime)
            .map_err(|_| Stage8bP1d3Error::RestartPackage)?;
    if restored
        .stage8b_p1d3_replacement()
        .map_or(true, |replacement| replacement.phase() != expected_phase)
    {
        return Err(Stage8bP1d3Error::RestartPackage);
    }
    Ok(Stage8bP1d3ReplacementStageResult {
        restored,
        restart_package,
    })
}

/// Applies only the request ACK from an authenticated, reread Stage 6 V3
/// outcome record. Accepted initial/cancel outcomes produce `S_ack`; a
/// recovered cancel produces the distinct terminal `S_cancel_recovered` seal.
pub(crate) fn apply_stage8b_p1d3_request_ack_stage(
    restored_pre_transition: crate::Stage5gCleanRestartedCapability,
    outcome_record: &crate::Stage6JournalRecordV3,
    binding: Stage8bP1d3Stage6RecoveryBinding,
    commitment_key: &crate::Stage5gLifecycleCommitmentKey,
) -> Result<Stage8bP1d3ReplacementStageResult, Stage8bP1d3Error> {
    let authenticated_post_checkpoint_sha256 = binding.authenticated_post_checkpoint_sha256.clone();
    let authenticated = outcome_record.authenticate_p1d3_outcome(binding)?;
    let evidence = authenticated.evidence.clone();
    if !matches!(
        evidence.outcome_kind,
        Stage8bP1d3OutcomeKind::InitialWorking
            | Stage8bP1d3OutcomeKind::InitialFilled
            | Stage8bP1d3OutcomeKind::InitialExpired
            | Stage8bP1d3OutcomeKind::CancelCanceled
            | Stage8bP1d3OutcomeKind::CancelExecutionObserved
            | Stage8bP1d3OutcomeKind::CancelAlreadyTerminalNonExecution
    ) {
        return Err(Stage8bP1d3Error::InvalidTransition);
    }
    let evidence_digest = evidence.digest_sha256()?;
    let receipt_ts = exact_timestamp(evidence.transition_received_ts_utc_ms)?;
    let export_input = restored_pre_transition
        .stage8b_p1d3_export_input(
            match evidence.outcome_kind {
                Stage8bP1d3OutcomeKind::CancelExecutionObserved
                | Stage8bP1d3OutcomeKind::CancelAlreadyTerminalNonExecution => {
                    Stage8bP1d3BookPhase::CancelRecovered
                }
                _ => Stage8bP1d3BookPhase::Ack,
            },
            &evidence_digest,
            receipt_ts,
        )
        .map_err(|_| Stage8bP1d3Error::RestartPackage)?;
    let semantic_commit = restored_pre_transition
        .stage8b_p1_semantic_commit()
        .cloned();
    let fresh_runtime = restored_pre_transition.stage5g_fresh_reconstruction_candidate();
    let (runtime, state, current) = restored_pre_transition
        .into_stage8b_p1d3_parts()
        .map_err(|_| Stage8bP1d3Error::RestartPackage)?;
    let plan = recover_stage8b_p1d3_outcome_transition(current.working_book(), authenticated)?;
    if !plan.evidence.is_request_scoped() {
        return Err(Stage8bP1d3Error::IdentityMismatch);
    }
    let ack = plan
        .ack
        .as_ref()
        .ok_or(Stage8bP1d3Error::InvalidTransition)?;
    let seq_ack = plan
        .evidence
        .reserved_seq_ack
        .ok_or(Stage8bP1d3Error::InvalidSequence)?;
    let kind = match plan.evidence.outcome_kind {
        Stage8bP1d3OutcomeKind::InitialWorking
        | Stage8bP1d3OutcomeKind::InitialFilled
        | Stage8bP1d3OutcomeKind::InitialExpired => {
            crate::stage5g_order_position::Stage8bP1d3AckStateKind::LimitPlace {
                side: plan.evidence.side,
                qty: decimal(plan.evidence.qty_decimal_bytes)?,
                pre_position_qty: decimal(plan.evidence.pre_position_qty_decimal_bytes)?,
                attribution: current.working_book.attribution.clone(),
                source_event_ts_utc: plan
                    .evidence
                    .source_m10_close_ts_utc_ms
                    .ok_or(Stage8bP1d3Error::InvalidChronology)?
                    .div_euclid(1_000),
            }
        }
        Stage8bP1d3OutcomeKind::CancelCanceled
        | Stage8bP1d3OutcomeKind::CancelExecutionObserved
        | Stage8bP1d3OutcomeKind::CancelAlreadyTerminalNonExecution => {
            crate::stage5g_order_position::Stage8bP1d3AckStateKind::Cancel {
                target_broker_order_id: plan.evidence.broker_order_id.clone(),
                attribution: cancel_attribution(&current.working_book.attribution)?,
                source_event_ts_utc: plan
                    .evidence
                    .source_m10_close_ts_utc_ms
                    .ok_or(Stage8bP1d3Error::InvalidChronology)?
                    .div_euclid(1_000),
            }
        }
        _ => return Err(Stage8bP1d3Error::InvalidTransition),
    };
    let state =
        crate::stage5g_order_position::stage8b_p1d3_append_ack_state(state, ack, seq_ack, kind)
            .map_err(|_| Stage8bP1d3Error::InvalidSequence)?;
    let callback = crate::stage5c_paper_host::resolve_stage8b_p1d3_ack_bridge(runtime, ack)
        .map_err(|_| Stage8bP1d3Error::Callback)?;
    if callback.callback_count != 1 || !is_sha256(&callback.post_state_fingerprint_sha256) {
        return Err(Stage8bP1d3Error::Callback);
    }
    let runtime = callback.strategy;
    let phase = if matches!(
        plan.evidence.outcome_kind,
        Stage8bP1d3OutcomeKind::CancelExecutionObserved
            | Stage8bP1d3OutcomeKind::CancelAlreadyTerminalNonExecution
    ) {
        Stage8bP1d3BookPhase::CancelRecovered
    } else {
        Stage8bP1d3BookPhase::Ack
    };
    let replacement = Stage8bP1d3ReplacementProjectionV1::for_transition(
        phase,
        current.working_book.clone(),
        plan.evidence,
        evidence.stage6_reserved_checkpoint_sha256.clone(),
        authenticated_post_checkpoint_sha256,
    )?;
    export_stage8b_p1d3_replacement(
        runtime,
        state,
        replacement,
        semantic_commit,
        export_input,
        fresh_runtime,
        commitment_key,
    )
}

/// Applies the sole truth continuation authorized by a reread `S_ack`.
pub(crate) fn apply_stage8b_p1d3_truth_after_ack_stage(
    restored_ack: crate::Stage5gCleanRestartedCapability,
    commitment_key: &crate::Stage5gLifecycleCommitmentKey,
) -> Result<Stage8bP1d3ReplacementStageResult, Stage8bP1d3Error> {
    let replacement = restored_ack
        .stage8b_p1d3_replacement()
        .cloned()
        .ok_or(Stage8bP1d3Error::RestartPackage)?;
    let plan = replacement.transition_plan_from_authenticated_package()?;
    let truth = plan
        .truth
        .clone()
        .ok_or(Stage8bP1d3Error::InvalidTransition)?;
    let request_id = plan
        .evidence
        .request_id
        .ok_or(Stage8bP1d3Error::IdentityMismatch)?;
    let seq_truth = plan
        .evidence
        .reserved_seq_truth
        .ok_or(Stage8bP1d3Error::InvalidSequence)?;
    let phase = if plan.post_book.active_record().is_some() {
        Stage8bP1d3BookPhase::Working
    } else {
        Stage8bP1d3BookPhase::Terminal
    };
    let export_input = restored_ack
        .stage8b_p1d3_export_input(phase, &plan.evidence_sha256, truth.received_ts)
        .map_err(|_| Stage8bP1d3Error::RestartPackage)?;
    let semantic_commit = restored_ack.stage8b_p1_semantic_commit().cloned();
    let fresh_runtime = restored_ack.stage5g_fresh_reconstruction_candidate();
    let (runtime, state, current) = restored_ack
        .into_stage8b_p1d3_parts()
        .map_err(|_| Stage8bP1d3Error::RestartPackage)?;
    if current.phase != Stage8bP1d3BookPhase::Ack
        || current.latest_outcome_evidence_sha256.as_deref() != Some(plan.evidence_sha256.as_str())
    {
        return Err(Stage8bP1d3Error::InvalidTransition);
    }
    let order_attribution = if plan.evidence.outcome_kind == Stage8bP1d3OutcomeKind::CancelCanceled
    {
        cancel_attribution(&current.working_book.attribution)?
    } else {
        current.working_book.attribution.clone()
    };
    let pre_position_qty = decimal(plan.evidence.pre_position_qty_decimal_bytes)?
        .to_f64()
        .filter(|value| value.is_finite())
        .ok_or(Stage8bP1d3Error::InvalidDecimal)?;
    let callback = crate::stage5c_paper_host::resolve_stage8b_p1d3_broker_truth_bridge(
        runtime,
        crate::stage5c_paper_host::Stage8bP1d3BrokerTruthBridgeInput {
            strategy_id: current.working_book.attribution.strategy_id().to_string(),
            account_id: current.working_book.account_id.clone(),
            instrument: current.working_book.instrument.clone(),
            request_id,
            attribution: order_attribution.clone(),
            pre_position_qty,
            truth: truth.clone(),
        },
    )
    .map_err(|_| Stage8bP1d3Error::Callback)?;
    if callback.callback_count != 1 + usize::from(!truth.positions.is_empty())
        || !is_sha256(&callback.post_state_fingerprint_sha256)
    {
        return Err(Stage8bP1d3Error::Callback);
    }
    let runtime = callback.strategy;
    let state = crate::stage5g_order_position::stage8b_p1d3_apply_truth_state(
        state,
        crate::Stage5gOrderPositionEvidence {
            total_sequence: seq_truth,
            request_id,
            broker_truth: truth,
            order_attribution: Some(order_attribution),
        },
    )
    .map_err(|_| Stage8bP1d3Error::InvalidSequence)?;
    let replacement = Stage8bP1d3ReplacementProjectionV1::for_transition(
        phase,
        plan.post_book,
        plan.evidence,
        current.latest_stage6_reservation_sha256,
        current.authenticated_stage6_checkpoint_sha256,
    )?;
    export_stage8b_p1d3_replacement(
        runtime,
        state,
        replacement,
        semantic_commit,
        export_input,
        fresh_runtime,
        commitment_key,
    )
}

/// Applies one later autonomous fill/expiry from an authenticated Stage 6
/// write-ahead fact. It emits no ACK and commits one `S_terminal` truth.
pub(crate) fn apply_stage8b_p1d3_autonomous_truth_stage(
    restored_working: crate::Stage5gCleanRestartedCapability,
    outcome_record: &crate::Stage6JournalRecordV3,
    binding: Stage8bP1d3Stage6RecoveryBinding,
    commitment_key: &crate::Stage5gLifecycleCommitmentKey,
) -> Result<Stage8bP1d3ReplacementStageResult, Stage8bP1d3Error> {
    apply_stage8b_p1d3_autonomous_truth_stage_inner(
        restored_working,
        outcome_record,
        binding,
        false,
        commitment_key,
    )
}

/// Applies target truth after a cancel dispatch has already advanced Stage 6.
/// The authenticated semantic commit must be the exact cancel for this target;
/// the later cancel outcome remains unavailable until this replacement is
/// durably committed and reread by the service owner.
pub(crate) fn apply_stage8b_p1d3_cancel_target_truth_stage(
    restored_working: crate::Stage5gCleanRestartedCapability,
    outcome_record: &crate::Stage6JournalRecordV3,
    binding: Stage8bP1d3Stage6RecoveryBinding,
    commitment_key: &crate::Stage5gLifecycleCommitmentKey,
) -> Result<Stage8bP1d3ReplacementStageResult, Stage8bP1d3Error> {
    apply_stage8b_p1d3_autonomous_truth_stage_inner(
        restored_working,
        outcome_record,
        binding,
        true,
        commitment_key,
    )
}

fn apply_stage8b_p1d3_autonomous_truth_stage_inner(
    restored_working: crate::Stage5gCleanRestartedCapability,
    outcome_record: &crate::Stage6JournalRecordV3,
    binding: Stage8bP1d3Stage6RecoveryBinding,
    allow_cancel_dispatch_predecessor: bool,
    commitment_key: &crate::Stage5gLifecycleCommitmentKey,
) -> Result<Stage8bP1d3ReplacementStageResult, Stage8bP1d3Error> {
    let authenticated_post_checkpoint_sha256 = binding.authenticated_post_checkpoint_sha256.clone();
    let authenticated = outcome_record.authenticate_p1d3_outcome(binding)?;
    if !matches!(
        authenticated.evidence.outcome_kind,
        Stage8bP1d3OutcomeKind::LaterFilled | Stage8bP1d3OutcomeKind::LaterExpired
    ) {
        return Err(Stage8bP1d3Error::InvalidTransition);
    }
    let evidence_sha256 = authenticated.evidence.digest_sha256()?;
    let stage6_reservation_frontier_sha256 = authenticated
        .evidence
        .stage6_reserved_checkpoint_sha256
        .clone();
    let authenticated_stage6_checkpoint_sha256 = authenticated_post_checkpoint_sha256;
    let fresh_runtime = restored_working.stage5g_fresh_reconstruction_candidate();
    let export_input = restored_working
        .stage8b_p1d3_export_input(
            Stage8bP1d3BookPhase::Terminal,
            &evidence_sha256,
            exact_timestamp(authenticated.evidence.transition_received_ts_utc_ms)?,
        )
        .map_err(|_| Stage8bP1d3Error::RestartPackage)?;
    let semantic_commit = restored_working.stage8b_p1_semantic_commit().cloned();
    let (runtime, state, current) = restored_working
        .into_stage8b_p1d3_parts()
        .map_err(|_| Stage8bP1d3Error::RestartPackage)?;
    if !matches!(
        current.phase,
        Stage8bP1d3BookPhase::Working | Stage8bP1d3BookPhase::Eval
    ) {
        return Err(Stage8bP1d3Error::InvalidTransition);
    }
    let plan = recover_stage8b_p1d3_outcome_transition(current.working_book(), authenticated)?;
    let exact_predecessor = plan.evidence.stage6_predecessor_frontier_sha256
        == current.authenticated_stage6_checkpoint_sha256;
    let exact_cancel_target = allow_cancel_dispatch_predecessor
        && plan.evidence.outcome_kind == Stage8bP1d3OutcomeKind::LaterFilled
        && semantic_commit.as_ref().is_some_and(|semantic| {
            matches!(
                semantic.canonical_command.as_ref(),
                Some(BrokerCommand::CancelOrder(cancel))
                    if cancel.order_id == plan.evidence.broker_order_id
            )
        });
    if !exact_predecessor && !exact_cancel_target {
        return Err(Stage8bP1d3Error::IdentityMismatch);
    }
    let truth = plan
        .truth
        .clone()
        .ok_or(Stage8bP1d3Error::InvalidTransition)?;
    let request_id = plan
        .post_book
        .record(plan.evidence.broker_order_id())
        .map(Stage8bP1d3OrderRecordV1::original_request_id)
        .ok_or(Stage8bP1d3Error::InvalidRegistry)?;
    let sequence = plan
        .evidence
        .reserved_seq_truth
        .ok_or(Stage8bP1d3Error::InvalidSequence)?;
    let pre_position_qty = decimal(plan.evidence.pre_position_qty_decimal_bytes)?
        .to_f64()
        .filter(|value| value.is_finite())
        .ok_or(Stage8bP1d3Error::InvalidDecimal)?;
    let callback = crate::stage5c_paper_host::resolve_stage8b_p1d3_broker_truth_bridge(
        runtime,
        crate::stage5c_paper_host::Stage8bP1d3BrokerTruthBridgeInput {
            strategy_id: current.working_book.attribution.strategy_id().to_string(),
            account_id: current.working_book.account_id.clone(),
            instrument: current.working_book.instrument.clone(),
            request_id,
            attribution: current.working_book.attribution.clone(),
            pre_position_qty,
            truth: truth.clone(),
        },
    )
    .map_err(|_| Stage8bP1d3Error::Callback)?;
    if callback.callback_count != 1 + usize::from(!truth.positions.is_empty())
        || !is_sha256(&callback.post_state_fingerprint_sha256)
    {
        return Err(Stage8bP1d3Error::Callback);
    }
    let runtime = callback.strategy;
    let state = crate::stage5g_order_position::stage8b_p1d3_apply_truth_state(
        state,
        crate::Stage5gOrderPositionEvidence {
            total_sequence: sequence,
            request_id,
            broker_truth: truth,
            order_attribution: Some(current.working_book.attribution.clone()),
        },
    )
    .map_err(|_| Stage8bP1d3Error::InvalidSequence)?;
    let replacement = Stage8bP1d3ReplacementProjectionV1::for_transition(
        Stage8bP1d3BookPhase::Terminal,
        plan.post_book,
        plan.evidence,
        stage6_reservation_frontier_sha256,
        authenticated_stage6_checkpoint_sha256,
    )?;
    export_stage8b_p1d3_replacement(
        runtime,
        state,
        replacement,
        semantic_commit,
        export_input,
        fresh_runtime,
        commitment_key,
    )
}

/// Commits the no-sequence `S_eval` replacement for one newly observed,
/// untouched canonical M10. Replaying the exact already-evaluated bar returns
/// the authenticated package byte-for-byte without creating another seal.
pub(crate) fn apply_stage8b_p1d3_evaluation_stage(
    restored_working: crate::Stage5gCleanRestartedCapability,
    evaluation: Stage8bP1d3LaterEvaluationPlan,
    commitment_key: &crate::Stage5gLifecycleCommitmentKey,
) -> Result<Stage8bP1d3EvaluationStageResult, Stage8bP1d3Error> {
    let current = restored_working
        .stage8b_p1d3_replacement()
        .cloned()
        .ok_or(Stage8bP1d3Error::RestartPackage)?;
    if !matches!(
        current.phase,
        Stage8bP1d3BookPhase::Working | Stage8bP1d3BookPhase::Eval
    ) {
        return Err(Stage8bP1d3Error::InvalidTransition);
    }
    match evaluation {
        Stage8bP1d3LaterEvaluationPlan::AlreadyEvaluated { book } => {
            if book.encode_canonical()? != current.working_book.encode_canonical()? {
                return Err(Stage8bP1d3Error::IdentityMismatch);
            }
            Ok(Stage8bP1d3EvaluationStageResult::AlreadyEvaluated)
        }
        Stage8bP1d3LaterEvaluationPlan::Untouched {
            evidence,
            evidence_bytes,
            post_book,
        } => {
            if evidence.encode_canonical()? != evidence_bytes
                || evidence.pre_book_sha256 != current.working_book.canonical_sha256()?
                || evidence.sequence_frontier != current.working_book.total_sequence_frontier
                || post_book.total_sequence_frontier != current.working_book.total_sequence_frontier
                || post_book_state_sha256(&post_book)? != evidence.expected_post_book_sha256
            {
                return Err(Stage8bP1d3Error::IdentityMismatch);
            }
            let evidence_sha256 = evidence.digest_sha256()?;
            let receipt_ts = exact_timestamp(evidence.candidate_m10_close_ts_utc_ms)?;
            let export_input = restored_working
                .stage8b_p1d3_export_input(Stage8bP1d3BookPhase::Eval, &evidence_sha256, receipt_ts)
                .map_err(|_| Stage8bP1d3Error::RestartPackage)?;
            let semantic_commit = restored_working.stage8b_p1_semantic_commit().cloned();
            let fresh_runtime = restored_working.stage5g_fresh_reconstruction_candidate();
            let (runtime, state, reread_current) = restored_working
                .into_stage8b_p1d3_parts()
                .map_err(|_| Stage8bP1d3Error::RestartPackage)?;
            if reread_current != current {
                return Err(Stage8bP1d3Error::IdentityMismatch);
            }
            let replacement = Stage8bP1d3ReplacementProjectionV1::for_evaluation(
                post_book,
                *evidence,
                current.previous_outcome_evidence_sha256().to_string(),
                current.latest_stage6_reservation_sha256,
                current.authenticated_stage6_checkpoint_sha256,
            )?;
            let committed = export_stage8b_p1d3_replacement(
                runtime,
                state,
                replacement,
                semantic_commit,
                export_input,
                fresh_runtime,
                commitment_key,
            )?;
            Ok(Stage8bP1d3EvaluationStageResult::Committed(Box::new(
                committed,
            )))
        }
        Stage8bP1d3LaterEvaluationPlan::Outcome(_) => Err(Stage8bP1d3Error::InvalidTransition),
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Stage8bP1d3LaterEvaluationPlan {
    AlreadyEvaluated {
        book: Stage8bP1d3WorkingBookProjectionV1,
    },
    Untouched {
        evidence: Box<Stage8bP1d3EvaluationEvidenceV1>,
        evidence_bytes: Vec<u8>,
        post_book: Stage8bP1d3WorkingBookProjectionV1,
    },
    Outcome(Box<Stage8bP1d3TransitionPlan>),
}

pub(crate) enum Stage8bP1d3CancelTransitionPlan {
    Ready(Box<Stage8bP1d3TransitionPlan>),
    TargetThenCancel {
        target_transition: Box<Stage8bP1d3TransitionPlan>,
        #[allow(
            dead_code,
            reason = "pure oracle retained for source tests; durable recovery reconstructs from the target seal"
        )]
        continuation: Box<Stage8bP1d3DeferredCancelAfterTarget>,
    },
}

pub(crate) fn cancel_candidate_requires_target_outcome(
    pre_book: &Stage8bP1d3WorkingBookProjectionV1,
    target_broker_order_id: &BrokerOrderId,
    target_place_client_id: Option<&ClientOrderId>,
    cancel_decision_m10_close_ts_utc_ms: i64,
    candidate: &Stage8bP1d3CanonicalM10Evidence,
    step: &Stage8bP1d3ScheduleStepAuthority,
) -> Result<bool, Stage8bP1d3Error> {
    pre_book.validate()?;
    validate_scoped_bar(pre_book, candidate)?;
    if candidate.close_ts_utc_ms <= cancel_decision_m10_close_ts_utc_ms
        || candidate.redis_id == format!("{cancel_decision_m10_close_ts_utc_ms}-0")
    {
        return Err(Stage8bP1d3Error::InvalidChronology);
    }
    let record = pre_book
        .record(target_broker_order_id)
        .ok_or(Stage8bP1d3Error::InvalidRegistry)?;
    if target_place_client_id.is_some_and(|value| value != &record.original_durable_place_client_id)
    {
        return Err(Stage8bP1d3Error::IdentityMismatch);
    }
    let predecessor = record
        .last_evaluated_m10_redis_id
        .as_deref()
        .unwrap_or(record.decision_m10_redis_id.as_str());
    validate_step(step, predecessor, candidate)?;
    if record.lifecycle != BrokerOrderLifecycle::Active {
        return Ok(false);
    }
    Ok(
        decide_limit(record.side, record.limit_price()?, candidate)?.0
            == Stage8bP1d3LimitDecision::Filled,
    )
}

/// Linear continuation retained after a cancel candidate first fills the
/// target LIMIT. The cancel outcome cannot be constructed before the target
/// V3 record is appended and reread because its predecessor is the actual
/// post-target Stage 6 checkpoint, not the non-recursive reservation token
/// embedded in the target evidence.
#[allow(
    dead_code,
    reason = "the pure two-stage oracle is exercised by source tests; durable composition reconstructs it from the reread target seal"
)]
pub(crate) struct Stage8bP1d3DeferredCancelAfterTarget {
    input: Stage8bP1d3CancelInput,
    candidate: Stage8bP1d3CanonicalM10Evidence,
    step: Stage8bP1d3ScheduleStepAuthority,
    expected_target_post_book_sha256: String,
    target_outcome_evidence_sha256: String,
}

#[allow(
    dead_code,
    reason = "the pure two-stage oracle is exercised by source tests; durable composition reconstructs it from the reread target seal"
)]
impl Stage8bP1d3DeferredCancelAfterTarget {
    pub(crate) fn complete(
        mut self,
        authenticated_target_book: &Stage8bP1d3WorkingBookProjectionV1,
        authenticated_post_target_stage6_checkpoint_sha256: String,
    ) -> Result<Stage8bP1d3TransitionPlan, Stage8bP1d3Error> {
        authenticated_target_book.validate()?;
        if !is_sha256(&authenticated_post_target_stage6_checkpoint_sha256)
            || post_book_state_sha256(authenticated_target_book)?
                != self.expected_target_post_book_sha256
        {
            return Err(Stage8bP1d3Error::IdentityMismatch);
        }
        let record = authenticated_target_book
            .record(&self.input.target_broker_order_id)
            .cloned()
            .ok_or(Stage8bP1d3Error::InvalidRegistry)?;
        if record.lifecycle != BrokerOrderLifecycle::Terminal
            || record.status != OrderStatus::Filled
        {
            return Err(Stage8bP1d3Error::InvalidTransition);
        }
        self.input.sequence_allocation_frontier = authenticated_target_book.total_sequence_frontier;
        self.input.stage6_predecessor_frontier_sha256 =
            authenticated_post_target_stage6_checkpoint_sha256;
        self.input.target_stage6_reserved_checkpoint_sha256 = None;
        self.input.previous_outcome_evidence_sha256 = self.target_outcome_evidence_sha256.clone();
        build_cancel_outcome(
            authenticated_target_book,
            &self.input,
            record,
            Stage8bP1d3OutcomeKind::CancelExecutionObserved,
            &self.candidate,
            &self.step,
            authenticated_target_book.total_sequence_frontier,
            self.target_outcome_evidence_sha256,
            self.input.stage6_predecessor_frontier_sha256.clone(),
            self.input.stage6_reserved_checkpoint_sha256.clone(),
            4,
        )
    }
}

/// Linear recovery authority created only after the enclosing Stage 6 record
/// and its predecessor/reserved checkpoint bindings have been authenticated.
/// It intentionally exposes no constructor outside this private module.
pub(crate) struct Stage8bP1d3AuthenticatedOutcomeEvidence {
    evidence: Stage8bP1d3OutcomeEvidenceV1,
    canonical_bytes: Vec<u8>,
}

#[derive(Debug, Clone)]
pub(crate) struct Stage8bP1d3Stage6RecoveryBinding {
    pub outcome_record_id: String,
    pub predecessor_frontier_sha256: String,
    pub reserved_checkpoint_sha256: String,
    pub request_finalized_record_id: Option<String>,
    pub request_finalized_fingerprint_sha256: Option<String>,
    pub authenticated_post_checkpoint_sha256: String,
}

/// Request-scoped effect reconstructed from one canonical V3 outcome record.
/// It deliberately contains no journal-write or runtime continuation
/// capability; mixed replay uses it only to advance the exact Stage 6 request
/// whose dispatch record is named by the outcome evidence.
pub(crate) struct Stage8bP1d3Stage6RequestReplayEffect {
    pub(crate) request_id: StrategyRequestId,
    pub(crate) durable_request_client_id: ClientOrderId,
    pub(crate) account_id: BrokerAccountId,
    pub(crate) instrument: InstrumentId,
    pub(crate) attribution_fingerprint_sha256: String,
    pub(crate) accepted_command_payload_sha256: String,
    pub(crate) accepted_stage6_identity_sha256: String,
    pub(crate) stage6_dispatch_record_id: String,
    pub(crate) kind: Stage8bP1d3Stage6RequestReplayEffectKind,
}

pub(crate) enum Stage8bP1d3Stage6RequestReplayEffectKind {
    Place {
        broker_order_id: BrokerOrderId,
        broker_trade_id: Option<BrokerTradeId>,
    },
    Cancel {
        target_broker_order_id: BrokerOrderId,
        target_place_client_id: ClientOrderId,
        outcome: crate::Stage6CancelOutcomeV1,
    },
}

impl Stage8bP1d3WorkingBookProjectionV1 {
    pub(crate) fn migrate_from_p1d2(
        accepted_p1d2_package_commitment_sha256: &str,
        operational_identity_sha256: String,
        package_generation: u64,
        account_id: BrokerAccountId,
        instrument: InstrumentId,
        attribution: HybridRuntimeAttribution,
        total_sequence_frontier: u64,
    ) -> Result<Self, Stage8bP1d3Error> {
        if !is_sha256(accepted_p1d2_package_commitment_sha256)
            || !is_sha256(&operational_identity_sha256)
            || package_generation == 0
        {
            return Err(Stage8bP1d3Error::IdentityMismatch);
        }
        let genesis = genesis_sha256(
            accepted_p1d2_package_commitment_sha256,
            &operational_identity_sha256,
            package_generation,
        )?;
        let value = Self {
            schema_version: STAGE8B_P1D3_WORKING_BOOK_SCHEMA_VERSION,
            identity_domain: STAGE8B_P1D3_WORKING_BOOK_DOMAIN.to_string(),
            transition_domain: STAGE8B_P1D3_BOOK_TRANSITION_DOMAIN.to_string(),
            operational_identity_sha256,
            package_generation,
            account_id,
            instrument,
            attribution,
            records: Vec::new(),
            active_broker_order_id: None,
            transition_ordinal: 0,
            previous_transition_sha256: genesis.clone(),
            latest_transition_sha256: genesis.clone(),
            latest_outcome_evidence_sha256: genesis,
            total_sequence_frontier,
            phase: Stage8bP1d3BookPhase::Migrated,
        };
        value.validate()?;
        Ok(value)
    }

    pub(crate) fn encode_canonical(&self) -> Result<Vec<u8>, Stage8bP1d3Error> {
        self.validate()?;
        serde_json::to_vec(self).map_err(|_| Stage8bP1d3Error::InvalidEncoding)
    }

    #[allow(
        dead_code,
        reason = "canonical decoder is exercised by checked-in golden evidence"
    )]
    pub(crate) fn decode_canonical(bytes: &[u8]) -> Result<Self, Stage8bP1d3Error> {
        let value: Self =
            serde_json::from_slice(bytes).map_err(|_| Stage8bP1d3Error::InvalidEncoding)?;
        value.validate()?;
        if serde_json::to_vec(&value).map_err(|_| Stage8bP1d3Error::InvalidEncoding)? != bytes {
            return Err(Stage8bP1d3Error::InvalidEncoding);
        }
        Ok(value)
    }

    pub(crate) fn canonical_sha256(&self) -> Result<String, Stage8bP1d3Error> {
        Ok(sha256_hex(&self.encode_canonical()?))
    }

    pub(crate) fn operational_identity_sha256(&self) -> &str {
        &self.operational_identity_sha256
    }

    pub(crate) fn package_generation(&self) -> u64 {
        self.package_generation
    }

    pub(crate) fn account_id(&self) -> &BrokerAccountId {
        &self.account_id
    }

    pub(crate) fn instrument(&self) -> &InstrumentId {
        &self.instrument
    }

    pub(crate) fn attribution(&self) -> &HybridRuntimeAttribution {
        &self.attribution
    }

    pub(crate) fn total_sequence_frontier(&self) -> u64 {
        self.total_sequence_frontier
    }

    #[allow(dead_code, reason = "retained as a canonical-book audit accessor")]
    pub(crate) fn latest_outcome_evidence_sha256(&self) -> &str {
        &self.latest_outcome_evidence_sha256
    }

    #[allow(dead_code, reason = "retained as a canonical-book audit accessor")]
    pub(crate) fn phase(&self) -> Stage8bP1d3BookPhase {
        self.phase
    }

    pub(crate) fn active_record(&self) -> Option<&Stage8bP1d3OrderRecordV1> {
        let active = self.active_broker_order_id.as_ref()?;
        self.records
            .iter()
            .find(|record| &record.broker_order_id == active)
    }

    pub(crate) fn record(&self, id: &BrokerOrderId) -> Option<&Stage8bP1d3OrderRecordV1> {
        self.records
            .iter()
            .find(|record| &record.broker_order_id == id)
    }

    fn validate(&self) -> Result<(), Stage8bP1d3Error> {
        if self.schema_version != STAGE8B_P1D3_WORKING_BOOK_SCHEMA_VERSION
            || self.identity_domain != STAGE8B_P1D3_WORKING_BOOK_DOMAIN
            || self.transition_domain != STAGE8B_P1D3_BOOK_TRANSITION_DOMAIN
            || !is_sha256(&self.operational_identity_sha256)
            || !is_sha256(&self.previous_transition_sha256)
            || !is_sha256(&self.latest_transition_sha256)
            || !is_sha256(&self.latest_outcome_evidence_sha256)
            || self.package_generation == 0
            || self.records.len() > P1D3_MAX_ORDER_RECORDS_PER_GENERATION
        {
            return Err(Stage8bP1d3Error::InvalidRegistry);
        }
        let mut order_ids = BTreeSet::new();
        let mut request_ids = BTreeSet::new();
        let mut client_ids = BTreeSet::new();
        let mut fingerprints = BTreeSet::new();
        let mut previous_order_id: Option<&[u8]> = None;
        let mut active_rows = 0_usize;
        for record in &self.records {
            record.validate(self)?;
            let order_id = record.broker_order_id.as_str().as_bytes();
            if previous_order_id.is_some_and(|previous| previous >= order_id)
                || !order_ids.insert(record.broker_order_id.as_str().to_string())
                || !request_ids.insert(record.original_request_id.to_string())
                || !client_ids.insert(record.original_durable_place_client_id.as_str().to_string())
                || !fingerprints.insert(record.deterministic_order_fingerprint_sha256.clone())
            {
                return Err(Stage8bP1d3Error::InvalidRegistry);
            }
            previous_order_id = Some(order_id);
            if record.lifecycle == BrokerOrderLifecycle::Active {
                active_rows += 1;
                if self.active_broker_order_id.as_ref() != Some(&record.broker_order_id) {
                    return Err(Stage8bP1d3Error::InvalidRegistry);
                }
            }
        }
        if active_rows > 1
            || (active_rows == 0) != self.active_broker_order_id.is_none()
            || self
                .active_broker_order_id
                .as_ref()
                .is_some_and(|id| !order_ids.contains(id.as_str()))
        {
            return Err(Stage8bP1d3Error::InvalidRegistry);
        }
        Ok(())
    }

    fn insert_or_replace(
        &mut self,
        record: Stage8bP1d3OrderRecordV1,
    ) -> Result<(), Stage8bP1d3Error> {
        if let Some(existing) = self
            .records
            .iter_mut()
            .find(|existing| existing.broker_order_id == record.broker_order_id)
        {
            *existing = record;
        } else {
            if self.records.len() >= P1D3_MAX_ORDER_RECORDS_PER_GENERATION {
                return Err(Stage8bP1d3Error::InvalidRegistry);
            }
            self.records.push(record);
        }
        self.records.sort_by(|left, right| {
            left.broker_order_id
                .as_str()
                .as_bytes()
                .cmp(right.broker_order_id.as_str().as_bytes())
        });
        self.active_broker_order_id = self
            .records
            .iter()
            .find(|record| record.lifecycle == BrokerOrderLifecycle::Active)
            .map(|record| record.broker_order_id.clone());
        Ok(())
    }

    fn advance(
        &mut self,
        evidence_bytes: &[u8],
        phase: Stage8bP1d3BookPhase,
        total_sequence_frontier: u64,
    ) -> Result<(), Stage8bP1d3Error> {
        let ordinal = self
            .transition_ordinal
            .checked_add(1)
            .ok_or(Stage8bP1d3Error::InvalidTransition)?;
        self.previous_transition_sha256 = self.latest_transition_sha256.clone();
        self.transition_ordinal = ordinal;
        self.total_sequence_frontier = total_sequence_frontier;
        self.phase = phase;
        // The current digest is excluded from the bytes it authenticates.
        // That avoids a recursive fixed point while the predecessor digest,
        // ordinal and all post-transition state remain committed.
        let prior_latest = std::mem::take(&mut self.latest_transition_sha256);
        let post_book_bytes =
            serde_json::to_vec(self).map_err(|_| Stage8bP1d3Error::InvalidEncoding)?;
        self.latest_transition_sha256 = transition_sha256(
            ordinal,
            &self.previous_transition_sha256,
            evidence_bytes,
            &post_book_bytes,
        )?;
        if !prior_latest.is_empty() && prior_latest != self.previous_transition_sha256 {
            return Err(Stage8bP1d3Error::InvalidTransition);
        }
        self.validate()
    }
}

impl Stage8bP1d3OrderRecordV1 {
    fn validate(&self, book: &Stage8bP1d3WorkingBookProjectionV1) -> Result<(), Stage8bP1d3Error> {
        let qty = decimal(self.qty_decimal_bytes)?;
        let limit = decimal(self.limit_price_decimal_bytes)?;
        let filled = decimal(self.filled_qty_decimal_bytes)?;
        let remaining = decimal(self.remaining_qty_decimal_bytes)?;
        if self.account_id != book.account_id
            || self.instrument != book.instrument
            || self.attribution != book.attribution
            || qty <= Decimal::ZERO
            || !is_integral(qty)
            || limit <= Decimal::ZERO
            || filled < Decimal::ZERO
            || remaining < Decimal::ZERO
            || filled.checked_add(remaining) != Some(qty)
            || !is_sha256(&self.canonical_command_sha256)
            || !is_sha256(&self.accepted_stage6_identity_sha256)
            || !is_sha256(&self.deterministic_order_fingerprint_sha256)
            || !is_sha256(&self.decision_m10_semantic_id_sha256)
            || !is_sha256(&self.decision_m10_payload_sha256)
            || self.decision_m10_redis_id != format!("{}-0", self.decision_m10_close_ts_utc_ms)
            || self
                .decision_m10_close_ts_utc_ms
                .checked_sub(self.decision_m10_open_ts_utc_ms)
                != Some(600_000)
            || !is_sha256(&self.schedule_fingerprint_sha256)
            || !is_sha256(&self.latest_order_projection_sha256)
            || self.transition_ordinal == 0
        {
            return Err(Stage8bP1d3Error::InvalidRegistry);
        }
        let active = self.status == OrderStatus::Working
            && self.lifecycle == BrokerOrderLifecycle::Active
            && filled == Decimal::ZERO
            && remaining == qty
            && self.broker_trade_id.is_none();
        let filled_terminal = self.status == OrderStatus::Filled
            && self.lifecycle == BrokerOrderLifecycle::Terminal
            && filled == qty
            && remaining == Decimal::ZERO
            && self.broker_trade_id.is_some();
        let unfilled_terminal = matches!(self.status, OrderStatus::Canceled | OrderStatus::Expired)
            && self.lifecycle == BrokerOrderLifecycle::Terminal
            && filled == Decimal::ZERO
            && remaining == qty
            && self.broker_trade_id.is_none();
        if !(active || filled_terminal || unfilled_terminal) {
            return Err(Stage8bP1d3Error::InvalidRegistry);
        }
        Ok(())
    }

    pub(crate) fn broker_order_id(&self) -> &BrokerOrderId {
        &self.broker_order_id
    }

    #[allow(dead_code, reason = "retained as an order-registry audit accessor")]
    pub(crate) fn accepted_stage6_identity_sha256(&self) -> &str {
        &self.accepted_stage6_identity_sha256
    }

    pub(crate) fn original_request_id(&self) -> StrategyRequestId {
        self.original_request_id
    }

    pub(crate) fn original_client_order_id(&self) -> &ClientOrderId {
        &self.original_durable_place_client_id
    }

    #[allow(dead_code, reason = "retained as an order-registry audit accessor")]
    pub(crate) fn status(&self) -> &OrderStatus {
        &self.status
    }

    #[allow(dead_code, reason = "retained as an order-registry audit accessor")]
    pub(crate) fn lifecycle(&self) -> BrokerOrderLifecycle {
        self.lifecycle
    }

    pub(crate) fn qty(&self) -> Result<Decimal, Stage8bP1d3Error> {
        decimal(self.qty_decimal_bytes)
    }

    pub(crate) fn limit_price(&self) -> Result<Decimal, Stage8bP1d3Error> {
        decimal(self.limit_price_decimal_bytes)
    }
}

impl Stage8bP1d3OutcomeEvidenceV1 {
    pub(crate) fn encode_canonical(&self) -> Result<Vec<u8>, Stage8bP1d3Error> {
        self.validate()?;
        serde_json::to_vec(self).map_err(|_| Stage8bP1d3Error::InvalidEncoding)
    }

    pub(crate) fn decode_canonical(bytes: &[u8]) -> Result<Self, Stage8bP1d3Error> {
        let value: Self =
            serde_json::from_slice(bytes).map_err(|_| Stage8bP1d3Error::InvalidEncoding)?;
        value.validate()?;
        if serde_json::to_vec(&value).map_err(|_| Stage8bP1d3Error::InvalidEncoding)? != bytes {
            return Err(Stage8bP1d3Error::InvalidEncoding);
        }
        Ok(value)
    }

    pub(crate) fn digest_sha256(&self) -> Result<String, Stage8bP1d3Error> {
        let bytes = self.encode_canonical()?;
        let mut hasher = Sha256::new();
        hasher.update(STAGE8B_P1D3_OUTCOME_EVIDENCE_DOMAIN.as_bytes());
        hasher.update([0]);
        hasher.update((bytes.len() as u64).to_be_bytes());
        hasher.update(&bytes);
        Ok(hex_digest(hasher.finalize()))
    }

    pub(crate) fn outcome_kind(&self) -> Stage8bP1d3OutcomeKind {
        self.outcome_kind
    }

    pub(crate) fn broker_order_id(&self) -> &BrokerOrderId {
        &self.broker_order_id
    }

    #[allow(dead_code, reason = "retained as an outcome-evidence audit accessor")]
    pub(crate) fn broker_trade_id(&self) -> Option<&BrokerTradeId> {
        self.broker_trade_id.as_ref()
    }

    pub(crate) fn request_id(&self) -> Option<StrategyRequestId> {
        self.request_id
    }

    #[allow(dead_code, reason = "retained for golden sequence assertions")]
    pub(crate) fn reserved_sequences(&self) -> (Option<u64>, Option<u64>) {
        (self.reserved_seq_ack, self.reserved_seq_truth)
    }

    pub(crate) fn operational_identity_sha256(&self) -> &str {
        &self.operational_identity_sha256
    }

    pub(crate) fn transition_ordinal(&self) -> u64 {
        self.transition_ordinal
    }

    pub(crate) fn stage6_outcome_record_id(&self) -> &str {
        &self.stage6_outcome_record_id
    }

    pub(crate) fn transition_received_ts_utc_ms(&self) -> i64 {
        self.transition_received_ts_utc_ms
    }

    pub(crate) fn stage6_predecessor_checkpoint_sha256(&self) -> &str {
        &self.stage6_predecessor_frontier_sha256
    }

    pub(crate) fn stage6_reserved_checkpoint_sha256(&self) -> &str {
        &self.stage6_reserved_checkpoint_sha256
    }

    pub(crate) fn previous_outcome_evidence_sha256(&self) -> &str {
        &self.previous_outcome_evidence_sha256
    }

    pub(crate) fn stage7_request_finalized_binding(&self) -> Option<(&str, &str)> {
        self.stage7_request_finalized_record_id
            .as_deref()
            .zip(self.stage7_request_finalized_fingerprint_sha256.as_deref())
    }

    pub(crate) fn is_request_scoped(&self) -> bool {
        self.request_id.is_some()
    }

    pub(crate) fn semantic_source_binding(&self) -> Option<Stage8bP1d3SemanticSourceBinding> {
        Some(Stage8bP1d3SemanticSourceBinding::from_authenticated_parts(
            self.candidate_m10_redis_id.clone()?,
            self.candidate_m10_semantic_id_sha256.clone()?,
            self.candidate_m10_payload_sha256.clone()?,
        ))
    }

    pub(crate) fn pre_book_sha256(&self) -> &str {
        &self.pre_book_sha256
    }

    pub(crate) fn validate_request_authority(
        &self,
        authority: &crate::Stage6DurableRequestAuthorityV1,
    ) -> Result<(), Stage8bP1d3Error> {
        let effect = self
            .stage6_request_replay_effect()?
            .ok_or(Stage8bP1d3Error::IdentityMismatch)?;
        let identity = authority.identity();
        let durable_binding = authority
            .durable_request_binding_sha256()
            .map_err(|_| Stage8bP1d3Error::IdentityMismatch)?;
        let action_matches = matches!(
            (&effect.kind, identity.action()),
            (
                Stage8bP1d3Stage6RequestReplayEffectKind::Place { .. },
                crate::Stage6DurableActionKind::Place
            ) | (
                Stage8bP1d3Stage6RequestReplayEffectKind::Cancel { .. },
                crate::Stage6DurableActionKind::Cancel
            )
        );
        let cancel_target_matches = match &effect.kind {
            Stage8bP1d3Stage6RequestReplayEffectKind::Place { .. } => true,
            Stage8bP1d3Stage6RequestReplayEffectKind::Cancel {
                target_broker_order_id,
                target_place_client_id,
                ..
            } => {
                identity.target_broker_order_id() == Some(target_broker_order_id)
                    && identity
                        .target_order_client_order_id()
                        .map_or(true, |value| value == target_place_client_id)
            }
        };
        if effect.request_id != identity.strategy_request_id()
            || effect.durable_request_client_id != *identity.durable_client_order_id()
            || effect.account_id != *identity.account_id()
            || effect.instrument != *identity.instrument()
            || effect.attribution_fingerprint_sha256 != attribution_sha256(identity.attribution())?
            || effect.accepted_command_payload_sha256
                != authority.canonical_command_sha256().as_str()
            || effect.accepted_stage6_identity_sha256 != durable_binding.as_str()
            || effect.stage6_dispatch_record_id != authority.dispatch_record_id().as_str()
            || !action_matches
            || !cancel_target_matches
        {
            return Err(Stage8bP1d3Error::IdentityMismatch);
        }
        Ok(())
    }

    pub(crate) fn stage6_request_replay_effect(
        &self,
    ) -> Result<Option<Stage8bP1d3Stage6RequestReplayEffect>, Stage8bP1d3Error> {
        self.validate()?;
        let kind = match self.outcome_kind {
            Stage8bP1d3OutcomeKind::InitialWorking
            | Stage8bP1d3OutcomeKind::InitialFilled
            | Stage8bP1d3OutcomeKind::InitialExpired => {
                Stage8bP1d3Stage6RequestReplayEffectKind::Place {
                    broker_order_id: self.broker_order_id.clone(),
                    broker_trade_id: self.broker_trade_id.clone(),
                }
            }
            Stage8bP1d3OutcomeKind::CancelCanceled
            | Stage8bP1d3OutcomeKind::CancelExecutionObserved
            | Stage8bP1d3OutcomeKind::CancelAlreadyTerminalNonExecution => {
                let outcome = match self.outcome_kind {
                    Stage8bP1d3OutcomeKind::CancelCanceled => {
                        crate::Stage6CancelOutcomeV1::Canceled
                    }
                    Stage8bP1d3OutcomeKind::CancelExecutionObserved => {
                        crate::Stage6CancelOutcomeV1::ExecutionObserved
                    }
                    Stage8bP1d3OutcomeKind::CancelAlreadyTerminalNonExecution => {
                        crate::Stage6CancelOutcomeV1::AlreadyTerminalNonExecution
                    }
                    _ => unreachable!(),
                };
                Stage8bP1d3Stage6RequestReplayEffectKind::Cancel {
                    target_broker_order_id: self.broker_order_id.clone(),
                    target_place_client_id: self
                        .target_place_client_id
                        .clone()
                        .ok_or(Stage8bP1d3Error::IdentityMismatch)?,
                    outcome,
                }
            }
            Stage8bP1d3OutcomeKind::LaterFilled | Stage8bP1d3OutcomeKind::LaterExpired => {
                return Ok(None)
            }
        };
        Ok(Some(Stage8bP1d3Stage6RequestReplayEffect {
            request_id: self.request_id.ok_or(Stage8bP1d3Error::IdentityMismatch)?,
            durable_request_client_id: self
                .durable_request_client_id
                .clone()
                .ok_or(Stage8bP1d3Error::IdentityMismatch)?,
            account_id: self.account_id.clone(),
            instrument: self.instrument.clone(),
            attribution_fingerprint_sha256: self.attribution_fingerprint_sha256.clone(),
            accepted_command_payload_sha256: self
                .accepted_command_payload_sha256
                .clone()
                .ok_or(Stage8bP1d3Error::IdentityMismatch)?,
            accepted_stage6_identity_sha256: self.accepted_stage6_identity_sha256.clone(),
            stage6_dispatch_record_id: self
                .stage6_dispatch_record_id
                .clone()
                .ok_or(Stage8bP1d3Error::IdentityMismatch)?,
            kind,
        }))
    }

    fn validate(&self) -> Result<(), Stage8bP1d3Error> {
        let pre_position_qty = decimal(self.pre_position_qty_decimal_bytes)?;
        let pre_position_avg = self
            .pre_position_avg_price_decimal_bytes
            .map(decimal)
            .transpose()?;
        if self.schema_version != STAGE8B_P1D3_OUTCOME_EVIDENCE_SCHEMA_VERSION
            || self.domain != STAGE8B_P1D3_OUTCOME_EVIDENCE_DOMAIN
            || self.transition_ordinal == 0
            || self.package_generation == 0
            || !is_sha256(&self.operational_identity_sha256)
            || !is_sha256(&self.attribution_fingerprint_sha256)
            || !is_sha256(&self.pre_book_sha256)
            || !is_sha256(&self.expected_post_book_sha256)
            || !is_sha256(&self.previous_outcome_evidence_sha256)
            || !is_sha256(&self.schedule_fingerprint_sha256)
            || !is_sha256(&self.stage6_predecessor_frontier_sha256)
            || !is_sha256(&self.stage6_reserved_checkpoint_sha256)
            || !is_sha256(&self.stage6_outcome_record_id)
            || !is_sha256(&self.accepted_stage6_identity_sha256)
            || !is_sha256(&self.previous_outcome_evidence_sha256)
            || self.pre_book_generation != self.package_generation
            || self.target_place_client_id.is_none()
            || self.target_broker_order_id.as_ref() != Some(&self.broker_order_id)
            || self.transition_source_ts_utc_ms != self.transition_received_ts_utc_ms
            || decimal(self.qty_decimal_bytes)? <= Decimal::ZERO
            || !is_integral(decimal(self.qty_decimal_bytes)?)
            || decimal(self.limit_price_decimal_bytes)? <= Decimal::ZERO
            || !is_integral(pre_position_qty)
            || pre_position_avg.is_some_and(|value| value <= Decimal::ZERO)
            || (pre_position_qty == Decimal::ZERO && pre_position_avg.is_some())
            || (pre_position_qty != Decimal::ZERO && pre_position_avg.is_none())
        {
            return Err(Stage8bP1d3Error::IdentityMismatch);
        }
        let expected_record_id = crate::Stage6JournalRecordId::derive_stage8b_p1d3_outcome(
            &self.operational_identity_sha256,
            &self.broker_order_id,
            self.transition_ordinal,
            self.outcome_kind.canonical_name(),
        );
        if expected_record_id.as_str() != self.stage6_outcome_record_id {
            return Err(Stage8bP1d3Error::IdentityMismatch);
        }
        let request_scoped = matches!(
            self.outcome_kind,
            Stage8bP1d3OutcomeKind::InitialWorking
                | Stage8bP1d3OutcomeKind::InitialFilled
                | Stage8bP1d3OutcomeKind::InitialExpired
                | Stage8bP1d3OutcomeKind::CancelCanceled
                | Stage8bP1d3OutcomeKind::CancelExecutionObserved
                | Stage8bP1d3OutcomeKind::CancelAlreadyTerminalNonExecution
        );
        if request_scoped
            != (self.request_id.is_some()
                && self.durable_request_client_id.is_some()
                && self
                    .canonical_command_sha256
                    .as_deref()
                    .is_some_and(is_sha256)
                && self
                    .accepted_command_payload_sha256
                    .as_deref()
                    .is_some_and(is_sha256)
                && self
                    .stage6_dispatch_record_id
                    .as_deref()
                    .is_some_and(is_sha256)
                && self
                    .stage7_request_finalized_record_id
                    .as_deref()
                    .is_some_and(is_sha256)
                && self
                    .stage7_request_finalized_fingerprint_sha256
                    .as_deref()
                    .is_some_and(is_sha256))
        {
            return Err(Stage8bP1d3Error::IdentityMismatch);
        }
        let cancel_scoped = matches!(
            self.outcome_kind,
            Stage8bP1d3OutcomeKind::CancelCanceled
                | Stage8bP1d3OutcomeKind::CancelExecutionObserved
                | Stage8bP1d3OutcomeKind::CancelAlreadyTerminalNonExecution
        );
        if cancel_scoped
            && self.durable_request_client_id.as_ref() == self.target_place_client_id.as_ref()
        {
            return Err(Stage8bP1d3Error::IdentityMismatch);
        }
        if !request_scoped
            && (self.stage7_request_finalized_record_id.is_some()
                || self.stage7_request_finalized_fingerprint_sha256.is_some())
        {
            return Err(Stage8bP1d3Error::IdentityMismatch);
        }
        let has_source = self.source_m10_redis_id.is_some();
        if has_source
            != (self.source_m10_semantic_id_sha256.is_some()
                && self.source_m10_payload_sha256.is_some()
                && self.source_m10_open_ts_utc_ms.is_some()
                && self.source_m10_close_ts_utc_ms.is_some())
            || !has_source
            || self
                .source_m10_semantic_id_sha256
                .as_deref()
                .is_some_and(|value| !is_sha256(value))
            || self
                .source_m10_payload_sha256
                .as_deref()
                .is_some_and(|value| !is_sha256(value))
            || self.source_m10_redis_id.as_deref()
                != self
                    .source_m10_close_ts_utc_ms
                    .map(|value| format!("{value}-0"))
                    .as_deref()
            || self
                .source_m10_close_ts_utc_ms
                .zip(self.source_m10_open_ts_utc_ms)
                .map_or(true, |(close, open)| {
                    close.checked_sub(open) != Some(600_000) || close.rem_euclid(600_000) != 0
                })
        {
            return Err(Stage8bP1d3Error::InvalidChronology);
        }
        let has_candidate = self.candidate_m10_redis_id.is_some();
        if has_candidate
            != (self.candidate_m10_semantic_id_sha256.is_some()
                && self.candidate_m10_payload_sha256.is_some()
                && self.candidate_m10_open_ts_utc_ms.is_some()
                && self.candidate_m10_close_ts_utc_ms.is_some())
            || self
                .candidate_m10_semantic_id_sha256
                .as_deref()
                .is_some_and(|value| !is_sha256(value))
            || self
                .candidate_m10_payload_sha256
                .as_deref()
                .is_some_and(|value| !is_sha256(value))
            || self.candidate_m10_redis_id.as_deref()
                != self
                    .candidate_m10_close_ts_utc_ms
                    .map(|value| format!("{value}-0"))
                    .as_deref()
            || self
                .candidate_m10_close_ts_utc_ms
                .zip(self.candidate_m10_open_ts_utc_ms)
                .is_some_and(|(close, open)| {
                    close.checked_sub(open) != Some(600_000) || close.rem_euclid(600_000) != 0
                })
        {
            return Err(Stage8bP1d3Error::InvalidChronology);
        }
        match self.consumed_witness_kind {
            Stage8bP1d3ConsumedWitnessKind::ScheduleStep
                if !has_candidate || self.boundary_ts_utc_ms.is_some() =>
            {
                return Err(Stage8bP1d3Error::InvalidChronology)
            }
            Stage8bP1d3ConsumedWitnessKind::DayExpiry
                if has_candidate
                    || self.boundary_ts_utc_ms != Some(self.transition_source_ts_utc_ms) =>
            {
                return Err(Stage8bP1d3Error::InvalidChronology)
            }
            _ => {}
        }
        let expected_sequences = match self.outcome_kind {
            Stage8bP1d3OutcomeKind::InitialWorking
            | Stage8bP1d3OutcomeKind::InitialFilled
            | Stage8bP1d3OutcomeKind::InitialExpired
            | Stage8bP1d3OutcomeKind::CancelCanceled => {
                let ack = self
                    .reserved_seq_ack
                    .ok_or(Stage8bP1d3Error::InvalidSequence)?;
                let truth = self
                    .reserved_seq_truth
                    .ok_or(Stage8bP1d3Error::InvalidSequence)?;
                ack.checked_add(1) == Some(truth)
            }
            Stage8bP1d3OutcomeKind::LaterFilled | Stage8bP1d3OutcomeKind::LaterExpired => {
                self.reserved_seq_ack.is_none() && self.reserved_seq_truth.is_some()
            }
            Stage8bP1d3OutcomeKind::CancelExecutionObserved
            | Stage8bP1d3OutcomeKind::CancelAlreadyTerminalNonExecution => {
                self.reserved_seq_ack.is_some() && self.reserved_seq_truth.is_none()
            }
        };
        if !expected_sequences {
            return Err(Stage8bP1d3Error::InvalidSequence);
        }
        let exact_sequence = match self.outcome_kind {
            Stage8bP1d3OutcomeKind::InitialWorking
            | Stage8bP1d3OutcomeKind::InitialFilled
            | Stage8bP1d3OutcomeKind::InitialExpired
            | Stage8bP1d3OutcomeKind::CancelCanceled => {
                self.sequence_allocation_frontier
                    .checked_add(1)
                    .zip(self.sequence_allocation_frontier.checked_add(2))
                    == self.reserved_seq_ack.zip(self.reserved_seq_truth)
            }
            Stage8bP1d3OutcomeKind::LaterFilled | Stage8bP1d3OutcomeKind::LaterExpired => {
                self.sequence_allocation_frontier.checked_add(1) == self.reserved_seq_truth
            }
            Stage8bP1d3OutcomeKind::CancelExecutionObserved
            | Stage8bP1d3OutcomeKind::CancelAlreadyTerminalNonExecution => {
                self.sequence_allocation_frontier.checked_add(1) == self.reserved_seq_ack
            }
        };
        if !exact_sequence {
            return Err(Stage8bP1d3Error::InvalidSequence);
        }
        let filled = matches!(
            self.outcome_kind,
            Stage8bP1d3OutcomeKind::InitialFilled | Stage8bP1d3OutcomeKind::LaterFilled
        );
        if filled != self.fill_price_decimal_bytes.is_some()
            || (filled && self.broker_trade_id.is_none())
            || matches!(
                self.outcome_kind,
                Stage8bP1d3OutcomeKind::InitialWorking
                    | Stage8bP1d3OutcomeKind::InitialExpired
                    | Stage8bP1d3OutcomeKind::LaterExpired
                    | Stage8bP1d3OutcomeKind::CancelCanceled
                    | Stage8bP1d3OutcomeKind::CancelAlreadyTerminalNonExecution
            ) && self.broker_trade_id.is_some()
        {
            return Err(Stage8bP1d3Error::InvalidTransition);
        }
        Ok(())
    }
}

pub(crate) fn authenticate_stage8b_p1d3_outcome_evidence(
    canonical_bytes: Vec<u8>,
    binding: Stage8bP1d3Stage6RecoveryBinding,
) -> Result<Stage8bP1d3AuthenticatedOutcomeEvidence, Stage8bP1d3Error> {
    let evidence = Stage8bP1d3OutcomeEvidenceV1::decode_canonical(&canonical_bytes)?;
    if !is_sha256(&binding.outcome_record_id)
        || !is_sha256(&binding.predecessor_frontier_sha256)
        || !is_sha256(&binding.reserved_checkpoint_sha256)
        || !is_sha256(&binding.authenticated_post_checkpoint_sha256)
        || evidence.stage6_outcome_record_id != binding.outcome_record_id
        || evidence.stage6_predecessor_frontier_sha256 != binding.predecessor_frontier_sha256
        || evidence.stage6_reserved_checkpoint_sha256 != binding.reserved_checkpoint_sha256
        || evidence.stage7_request_finalized_record_id != binding.request_finalized_record_id
        || evidence.stage7_request_finalized_fingerprint_sha256
            != binding.request_finalized_fingerprint_sha256
    {
        return Err(Stage8bP1d3Error::IdentityMismatch);
    }
    Ok(Stage8bP1d3AuthenticatedOutcomeEvidence {
        evidence,
        canonical_bytes,
    })
}

/// Recomputes a pre-seal transition from the authenticated Stage 6 fact.
/// Its inputs intentionally contain no provider, Redis selector, callback,
/// schedule issuer, dispatch handle or clock.
pub(crate) fn recover_stage8b_p1d3_outcome_transition(
    pre_book: &Stage8bP1d3WorkingBookProjectionV1,
    authenticated: Stage8bP1d3AuthenticatedOutcomeEvidence,
) -> Result<Stage8bP1d3TransitionPlan, Stage8bP1d3Error> {
    pre_book.validate()?;
    let Stage8bP1d3AuthenticatedOutcomeEvidence {
        evidence,
        canonical_bytes,
    } = authenticated;
    let expected_attribution = if matches!(
        evidence.outcome_kind,
        Stage8bP1d3OutcomeKind::CancelCanceled
            | Stage8bP1d3OutcomeKind::CancelExecutionObserved
            | Stage8bP1d3OutcomeKind::CancelAlreadyTerminalNonExecution
    ) {
        cancel_attribution(&pre_book.attribution)?
    } else {
        pre_book.attribution.clone()
    };
    if evidence.operational_identity_sha256 != pre_book.operational_identity_sha256
        || evidence.package_generation != pre_book.package_generation
        || evidence.pre_book_generation != pre_book.package_generation
        || evidence.account_id != pre_book.account_id
        || evidence.instrument != pre_book.instrument
        || evidence.attribution_fingerprint_sha256 != attribution_sha256(&expected_attribution)?
        || evidence.pre_book_sha256 != pre_book.canonical_sha256()?
    {
        return Err(Stage8bP1d3Error::IdentityMismatch);
    }
    let timestamp = exact_timestamp(evidence.transition_source_ts_utc_ms)?;
    let qty = decimal(evidence.qty_decimal_bytes)?;
    let limit = decimal(evidence.limit_price_decimal_bytes)?;
    let fill = evidence.fill_price_decimal_bytes.map(decimal).transpose()?;
    let pre_position_qty = decimal(evidence.pre_position_qty_decimal_bytes)?;
    let pre_position_avg = evidence
        .pre_position_avg_price_decimal_bytes
        .map(decimal)
        .transpose()?;
    let mut post_book = pre_book.clone();
    let mut ack = None;
    let mut truth = None;
    let mut changed_book = true;
    let phase;

    match evidence.outcome_kind {
        Stage8bP1d3OutcomeKind::InitialWorking
        | Stage8bP1d3OutcomeKind::InitialFilled
        | Stage8bP1d3OutcomeKind::InitialExpired => {
            if pre_book.active_record().is_some()
                || pre_book.record(&evidence.broker_order_id).is_some()
                || evidence.transition_ordinal != pre_book.transition_ordinal + 1
            {
                return Err(Stage8bP1d3Error::InvalidTransition);
            }
            let request_id = evidence
                .request_id
                .ok_or(Stage8bP1d3Error::IdentityMismatch)?;
            let client_id = evidence
                .durable_request_client_id
                .clone()
                .ok_or(Stage8bP1d3Error::IdentityMismatch)?;
            let command_sha256 = evidence
                .canonical_command_sha256
                .as_deref()
                .ok_or(Stage8bP1d3Error::IdentityMismatch)?;
            if client_id != ClientOrderId::from_strategy_request(request_id)
                || evidence.target_place_client_id.as_ref() != Some(&client_id)
                || derive_order_id(
                    &evidence.operational_identity_sha256,
                    request_id,
                    command_sha256,
                ) != evidence.broker_order_id
            {
                return Err(Stage8bP1d3Error::IdentityMismatch);
            }
            let (status, lifecycle, filled, remaining) = match evidence.outcome_kind {
                Stage8bP1d3OutcomeKind::InitialWorking => (
                    OrderStatus::Working,
                    BrokerOrderLifecycle::Active,
                    Decimal::ZERO,
                    qty,
                ),
                Stage8bP1d3OutcomeKind::InitialFilled => (
                    OrderStatus::Filled,
                    BrokerOrderLifecycle::Terminal,
                    qty,
                    Decimal::ZERO,
                ),
                Stage8bP1d3OutcomeKind::InitialExpired => (
                    OrderStatus::Expired,
                    BrokerOrderLifecycle::Terminal,
                    Decimal::ZERO,
                    qty,
                ),
                _ => unreachable!(),
            };
            verify_trade_identity(&evidence)?;
            let order = exact_order_snapshot(
                &evidence.account_id,
                &evidence.broker_order_id,
                &client_id,
                &evidence.instrument,
                evidence.side,
                qty,
                limit,
                status.clone(),
                lifecycle,
                filled,
                remaining,
                timestamp,
            );
            let record = Stage8bP1d3OrderRecordV1 {
                broker_order_id: evidence.broker_order_id.clone(),
                original_request_id: request_id,
                original_durable_place_client_id: client_id.clone(),
                canonical_command_sha256: command_sha256.to_string(),
                accepted_stage6_identity_sha256: evidence.accepted_stage6_identity_sha256.clone(),
                deterministic_order_fingerprint_sha256: order_fingerprint_sha256(
                    &evidence.broker_order_id,
                    request_id,
                    &client_id,
                    command_sha256,
                ),
                account_id: evidence.account_id.clone(),
                instrument: evidence.instrument.clone(),
                attribution: pre_book.attribution.clone(),
                side: evidence.side,
                qty_decimal_bytes: evidence.qty_decimal_bytes,
                limit_price_decimal_bytes: evidence.limit_price_decimal_bytes,
                status,
                lifecycle,
                filled_qty_decimal_bytes: filled.serialize(),
                remaining_qty_decimal_bytes: remaining.serialize(),
                decision_m10_redis_id: evidence
                    .source_m10_redis_id
                    .clone()
                    .ok_or(Stage8bP1d3Error::IdentityMismatch)?,
                decision_m10_semantic_id_sha256: evidence
                    .source_m10_semantic_id_sha256
                    .clone()
                    .ok_or(Stage8bP1d3Error::IdentityMismatch)?,
                decision_m10_payload_sha256: evidence
                    .source_m10_payload_sha256
                    .clone()
                    .ok_or(Stage8bP1d3Error::IdentityMismatch)?,
                decision_m10_open_ts_utc_ms: evidence
                    .source_m10_open_ts_utc_ms
                    .ok_or(Stage8bP1d3Error::IdentityMismatch)?,
                decision_m10_close_ts_utc_ms: evidence
                    .source_m10_close_ts_utc_ms
                    .ok_or(Stage8bP1d3Error::IdentityMismatch)?,
                first_observation_redis_id: evidence.candidate_m10_redis_id.clone(),
                first_observation_semantic_id_sha256: evidence
                    .candidate_m10_semantic_id_sha256
                    .clone(),
                first_observation_payload_sha256: evidence.candidate_m10_payload_sha256.clone(),
                last_evaluated_m10_redis_id: evidence.candidate_m10_redis_id.clone(),
                last_evaluated_m10_semantic_id_sha256: evidence
                    .candidate_m10_semantic_id_sha256
                    .clone(),
                last_evaluated_m10_payload_sha256: evidence.candidate_m10_payload_sha256.clone(),
                last_evaluated_close_ts_utc_ms: evidence.candidate_m10_close_ts_utc_ms,
                schedule_fingerprint_sha256: evidence.schedule_fingerprint_sha256.clone(),
                trading_day_identity: evidence.trading_day_identity.clone(),
                latest_order_projection_sha256: sha256_json(&order)?,
                broker_trade_id: evidence.broker_trade_id.clone(),
                total_sequence_frontier: evidence
                    .reserved_seq_truth
                    .ok_or(Stage8bP1d3Error::InvalidSequence)?,
                transition_ordinal: evidence.transition_ordinal,
            };
            truth = Some(exact_truth_from_record(
                &record,
                &order,
                evidence.broker_trade_id.as_ref(),
                fill,
                pre_position_qty,
                pre_position_avg,
                timestamp,
            )?);
            ack = Some(CommandAck {
                request_id,
                client_order_id: Some(client_id),
                broker_order_id: Some(evidence.broker_order_id.clone()),
                status: CommandAckStatus::Accepted,
                reason: None,
                received_ts: timestamp,
            });
            phase = if lifecycle == BrokerOrderLifecycle::Active {
                Stage8bP1d3BookPhase::Working
            } else {
                Stage8bP1d3BookPhase::Terminal
            };
            post_book.insert_or_replace(record)?;
        }
        Stage8bP1d3OutcomeKind::LaterFilled | Stage8bP1d3OutcomeKind::LaterExpired => {
            if evidence.transition_ordinal != pre_book.transition_ordinal + 1 {
                return Err(Stage8bP1d3Error::InvalidTransition);
            }
            let mut record = exact_bound_record(pre_book, &evidence, true)?;
            verify_trade_identity(&evidence)?;
            let (status, filled, remaining) =
                if evidence.outcome_kind == Stage8bP1d3OutcomeKind::LaterFilled {
                    (OrderStatus::Filled, qty, Decimal::ZERO)
                } else {
                    (OrderStatus::Expired, Decimal::ZERO, qty)
                };
            let order = exact_order_snapshot(
                &record.account_id,
                &record.broker_order_id,
                &record.original_durable_place_client_id,
                &record.instrument,
                record.side,
                qty,
                limit,
                status.clone(),
                BrokerOrderLifecycle::Terminal,
                filled,
                remaining,
                timestamp,
            );
            record.status = status;
            record.lifecycle = BrokerOrderLifecycle::Terminal;
            record.filled_qty_decimal_bytes = filled.serialize();
            record.remaining_qty_decimal_bytes = remaining.serialize();
            if evidence.candidate_m10_redis_id.is_some() {
                record.last_evaluated_m10_redis_id = evidence.candidate_m10_redis_id.clone();
                record.last_evaluated_m10_semantic_id_sha256 =
                    evidence.candidate_m10_semantic_id_sha256.clone();
                record.last_evaluated_m10_payload_sha256 =
                    evidence.candidate_m10_payload_sha256.clone();
                record.last_evaluated_close_ts_utc_ms = evidence.candidate_m10_close_ts_utc_ms;
            }
            record.schedule_fingerprint_sha256 = evidence.schedule_fingerprint_sha256.clone();
            record.trading_day_identity = evidence.trading_day_identity.clone();
            record.latest_order_projection_sha256 = sha256_json(&order)?;
            record.broker_trade_id = evidence.broker_trade_id.clone();
            record.total_sequence_frontier = evidence
                .reserved_seq_truth
                .ok_or(Stage8bP1d3Error::InvalidSequence)?;
            record.transition_ordinal = evidence.transition_ordinal;
            truth = Some(exact_truth_from_record(
                &record,
                &order,
                evidence.broker_trade_id.as_ref(),
                fill,
                pre_position_qty,
                pre_position_avg,
                timestamp,
            )?);
            phase = Stage8bP1d3BookPhase::Terminal;
            post_book.insert_or_replace(record)?;
        }
        Stage8bP1d3OutcomeKind::CancelCanceled => {
            if evidence.transition_ordinal != pre_book.transition_ordinal + 1 {
                return Err(Stage8bP1d3Error::InvalidTransition);
            }
            let request_id = exact_cancel_request_identity(&evidence)?;
            let mut record = exact_bound_record(pre_book, &evidence, true)?;
            let order = exact_order_snapshot(
                &record.account_id,
                &record.broker_order_id,
                &record.original_durable_place_client_id,
                &record.instrument,
                record.side,
                qty,
                limit,
                OrderStatus::Canceled,
                BrokerOrderLifecycle::Terminal,
                Decimal::ZERO,
                qty,
                timestamp,
            );
            record.status = OrderStatus::Canceled;
            record.lifecycle = BrokerOrderLifecycle::Terminal;
            record.last_evaluated_m10_redis_id = evidence.candidate_m10_redis_id.clone();
            record.last_evaluated_m10_semantic_id_sha256 =
                evidence.candidate_m10_semantic_id_sha256.clone();
            record.last_evaluated_m10_payload_sha256 =
                evidence.candidate_m10_payload_sha256.clone();
            record.last_evaluated_close_ts_utc_ms = evidence.candidate_m10_close_ts_utc_ms;
            record.schedule_fingerprint_sha256 = evidence.schedule_fingerprint_sha256.clone();
            record.trading_day_identity = evidence.trading_day_identity.clone();
            record.latest_order_projection_sha256 = sha256_json(&order)?;
            record.total_sequence_frontier = evidence
                .reserved_seq_truth
                .ok_or(Stage8bP1d3Error::InvalidSequence)?;
            record.transition_ordinal = evidence.transition_ordinal;
            truth = Some(BrokerTruthSnapshot {
                account_id: record.account_id.clone(),
                orders: vec![order],
                positions: Vec::new(),
                cash: None,
                trades: Vec::new(),
                instruments: Vec::new(),
                received_ts: timestamp,
            });
            ack = Some(exact_cancel_ack(&evidence, request_id, false, timestamp)?);
            phase = Stage8bP1d3BookPhase::Terminal;
            post_book.insert_or_replace(record)?;
        }
        Stage8bP1d3OutcomeKind::CancelExecutionObserved
        | Stage8bP1d3OutcomeKind::CancelAlreadyTerminalNonExecution => {
            if evidence.transition_ordinal != pre_book.transition_ordinal {
                return Err(Stage8bP1d3Error::InvalidTransition);
            }
            let request_id = exact_cancel_request_identity(&evidence)?;
            let record = exact_bound_record(pre_book, &evidence, false)?;
            let execution =
                evidence.outcome_kind == Stage8bP1d3OutcomeKind::CancelExecutionObserved;
            if execution != (record.status == OrderStatus::Filled)
                || (!execution
                    && !matches!(
                        record.status,
                        OrderStatus::Canceled | OrderStatus::Expired | OrderStatus::Rejected
                    ))
                || evidence.broker_trade_id != record.broker_trade_id
            {
                return Err(Stage8bP1d3Error::InvalidTransition);
            }
            ack = Some(exact_cancel_ack(&evidence, request_id, true, timestamp)?);
            phase = Stage8bP1d3BookPhase::CancelRecovered;
            changed_book = false;
        }
    }

    let actual_post_sha256 = if changed_book {
        post_book_state_sha256(&post_book)?
    } else {
        post_book.canonical_sha256()?
    };
    if actual_post_sha256 != evidence.expected_post_book_sha256 {
        return Err(Stage8bP1d3Error::IdentityMismatch);
    }
    let evidence_sha256 = evidence.digest_sha256()?;
    if changed_book {
        let frontier = evidence
            .reserved_seq_truth
            .ok_or(Stage8bP1d3Error::InvalidSequence)?;
        post_book.latest_outcome_evidence_sha256 = evidence_sha256.clone();
        post_book.advance(&canonical_bytes, phase, frontier)?;
    }
    Ok(Stage8bP1d3TransitionPlan {
        evidence,
        evidence_bytes: canonical_bytes,
        evidence_sha256,
        post_book,
        ack,
        truth,
    })
}

fn exact_bound_record(
    book: &Stage8bP1d3WorkingBookProjectionV1,
    evidence: &Stage8bP1d3OutcomeEvidenceV1,
    require_active: bool,
) -> Result<Stage8bP1d3OrderRecordV1, Stage8bP1d3Error> {
    let record = book
        .record(&evidence.broker_order_id)
        .cloned()
        .ok_or(Stage8bP1d3Error::InvalidRegistry)?;
    if (require_active && record.lifecycle != BrokerOrderLifecycle::Active)
        || record.account_id != evidence.account_id
        || record.instrument != evidence.instrument
        || record.side != evidence.side
        || record.qty_decimal_bytes != evidence.qty_decimal_bytes
        || record.limit_price_decimal_bytes != evidence.limit_price_decimal_bytes
        || record.original_durable_place_client_id
            != evidence
                .target_place_client_id
                .clone()
                .ok_or(Stage8bP1d3Error::IdentityMismatch)?
        || (!matches!(
            evidence.outcome_kind,
            Stage8bP1d3OutcomeKind::CancelCanceled
                | Stage8bP1d3OutcomeKind::CancelExecutionObserved
                | Stage8bP1d3OutcomeKind::CancelAlreadyTerminalNonExecution
        ) && record.accepted_stage6_identity_sha256 != evidence.accepted_stage6_identity_sha256)
    {
        return Err(Stage8bP1d3Error::IdentityMismatch);
    }
    Ok(record)
}

fn verify_trade_identity(evidence: &Stage8bP1d3OutcomeEvidenceV1) -> Result<(), Stage8bP1d3Error> {
    let expected = evidence
        .candidate_m10_semantic_id_sha256
        .as_deref()
        .map(|bar| derive_trade_id(&evidence.broker_order_id, bar));
    let is_fill = matches!(
        evidence.outcome_kind,
        Stage8bP1d3OutcomeKind::InitialFilled | Stage8bP1d3OutcomeKind::LaterFilled
    );
    if is_fill != evidence.broker_trade_id.is_some()
        || (is_fill && expected.as_ref() != evidence.broker_trade_id.as_ref())
    {
        return Err(Stage8bP1d3Error::IdentityMismatch);
    }
    Ok(())
}

fn exact_cancel_request_identity(
    evidence: &Stage8bP1d3OutcomeEvidenceV1,
) -> Result<StrategyRequestId, Stage8bP1d3Error> {
    let request_id = evidence
        .request_id
        .ok_or(Stage8bP1d3Error::IdentityMismatch)?;
    if evidence.durable_request_client_id.as_ref()
        != Some(&ClientOrderId::from_strategy_request(request_id))
        || evidence.durable_request_client_id.as_ref() == evidence.target_place_client_id.as_ref()
    {
        return Err(Stage8bP1d3Error::IdentityMismatch);
    }
    Ok(request_id)
}

fn exact_cancel_ack(
    evidence: &Stage8bP1d3OutcomeEvidenceV1,
    request_id: StrategyRequestId,
    recovered: bool,
    timestamp: DateTime<Utc>,
) -> Result<CommandAck, Stage8bP1d3Error> {
    Ok(CommandAck {
        request_id,
        client_order_id: Some(
            evidence
                .durable_request_client_id
                .clone()
                .ok_or(Stage8bP1d3Error::IdentityMismatch)?,
        ),
        broker_order_id: Some(evidence.broker_order_id.clone()),
        status: if recovered {
            CommandAckStatus::Recovered
        } else {
            CommandAckStatus::Accepted
        },
        reason: recovered
            .then(|| CommandAckReason::new(CommandAckReasonCode::RecoveredByBrokerTruth)),
        received_ts: timestamp,
    })
}

impl Stage8bP1d3EvaluationEvidenceV1 {
    fn encode_canonical(&self) -> Result<Vec<u8>, Stage8bP1d3Error> {
        self.validate()?;
        serde_json::to_vec(self).map_err(|_| Stage8bP1d3Error::InvalidEncoding)
    }

    fn digest_sha256(&self) -> Result<String, Stage8bP1d3Error> {
        Ok(sha256_hex(&self.encode_canonical()?))
    }

    fn validate(&self) -> Result<(), Stage8bP1d3Error> {
        if self.schema_version != 1
            || self.domain != STAGE8B_P1D3_EVALUATION_EVIDENCE_DOMAIN
            || self.package_generation == 0
            || self.transition_ordinal == 0
            || !is_sha256(&self.operational_identity_sha256)
            || !is_sha256(&self.attribution_fingerprint_sha256)
            || !is_sha256(&self.candidate_m10_semantic_id_sha256)
            || !is_sha256(&self.candidate_m10_payload_sha256)
            || !is_sha256(&self.candidate_m10_canonical_bytes_sha256)
            || !is_sha256(&self.schedule_fingerprint_sha256)
            || !is_sha256(&self.pre_book_sha256)
            || !is_sha256(&self.expected_post_book_sha256)
            || self.candidate_m10_redis_id != format!("{}-0", self.candidate_m10_close_ts_utc_ms)
            || self
                .candidate_m10_close_ts_utc_ms
                .checked_sub(self.candidate_m10_open_ts_utc_ms)
                != Some(600_000)
        {
            return Err(Stage8bP1d3Error::IdentityMismatch);
        }
        Ok(())
    }
}

pub(crate) fn build_later_limit_transition(
    pre_book: &Stage8bP1d3WorkingBookProjectionV1,
    input: Stage8bP1d3AutonomousInput,
    candidate: Option<Stage8bP1d3CanonicalM10Evidence>,
    step: Option<Stage8bP1d3ScheduleStepAuthority>,
    expiry: Option<Stage8bP1d3DayExpiryAuthority>,
) -> Result<Stage8bP1d3LaterEvaluationPlan, Stage8bP1d3Error> {
    pre_book.validate()?;
    validate_autonomous_input(pre_book, &input)?;
    let record = pre_book
        .active_record()
        .cloned()
        .ok_or(Stage8bP1d3Error::InvalidTransition)?;
    match (candidate, step, expiry) {
        (Some(candidate), Some(step), None) => {
            validate_scoped_bar(pre_book, &candidate)?;
            if record.last_evaluated_m10_redis_id.as_deref() == Some(candidate.redis_id.as_str()) {
                if record.last_evaluated_m10_semantic_id_sha256.as_deref()
                    != Some(candidate.semantic_id_sha256.as_str())
                    || record.last_evaluated_m10_payload_sha256.as_deref()
                        != Some(candidate.payload_sha256.as_str())
                    || record.last_evaluated_close_ts_utc_ms != Some(candidate.close_ts_utc_ms)
                {
                    return Err(Stage8bP1d3Error::IdentityMismatch);
                }
                return Ok(Stage8bP1d3LaterEvaluationPlan::AlreadyEvaluated {
                    book: pre_book.clone(),
                });
            }
            let predecessor = record
                .last_evaluated_m10_redis_id
                .as_deref()
                .ok_or(Stage8bP1d3Error::InvalidChronology)?;
            validate_step(&step, predecessor, &candidate)?;
            let (decision, fill) = decide_limit(record.side, record.limit_price()?, &candidate)?;
            match decision {
                Stage8bP1d3LimitDecision::Working => build_untouched_evaluation(
                    pre_book,
                    record,
                    candidate,
                    step,
                    input.sequence_allocation_frontier,
                    input.previous_outcome_evidence_sha256,
                ),
                Stage8bP1d3LimitDecision::Filled => Ok(Stage8bP1d3LaterEvaluationPlan::Outcome(
                    Box::new(build_autonomous_outcome(
                        pre_book,
                        input,
                        record,
                        Stage8bP1d3OutcomeKind::LaterFilled,
                        Some(candidate),
                        Some(&step),
                        None,
                        fill,
                    )?),
                )),
            }
        }
        (None, None, Some(expiry)) => {
            validate_expiry(&record, &expiry)?;
            Ok(Stage8bP1d3LaterEvaluationPlan::Outcome(Box::new(
                build_autonomous_outcome(
                    pre_book,
                    input,
                    record,
                    Stage8bP1d3OutcomeKind::LaterExpired,
                    None,
                    None,
                    Some(&expiry),
                    None,
                )?,
            )))
        }
        _ => Err(Stage8bP1d3Error::InvalidTransition),
    }
}

fn build_untouched_evaluation(
    pre_book: &Stage8bP1d3WorkingBookProjectionV1,
    mut record: Stage8bP1d3OrderRecordV1,
    candidate: Stage8bP1d3CanonicalM10Evidence,
    step: Stage8bP1d3ScheduleStepAuthority,
    sequence_frontier: u64,
    previous_outcome_evidence_sha256: String,
) -> Result<Stage8bP1d3LaterEvaluationPlan, Stage8bP1d3Error> {
    record.last_evaluated_m10_redis_id = Some(candidate.redis_id.clone());
    record.last_evaluated_m10_semantic_id_sha256 = Some(candidate.semantic_id_sha256.clone());
    record.last_evaluated_m10_payload_sha256 = Some(candidate.payload_sha256.clone());
    record.last_evaluated_close_ts_utc_ms = Some(candidate.close_ts_utc_ms);
    record.schedule_fingerprint_sha256 = step.schedule_fingerprint_sha256.clone();
    record.trading_day_identity = step.trading_day_identity.clone();
    record.transition_ordinal = pre_book
        .transition_ordinal
        .checked_add(1)
        .ok_or(Stage8bP1d3Error::InvalidTransition)?;
    let mut post_book = pre_book.clone();
    post_book.insert_or_replace(record.clone())?;
    let evidence = Stage8bP1d3EvaluationEvidenceV1 {
        schema_version: 1,
        domain: STAGE8B_P1D3_EVALUATION_EVIDENCE_DOMAIN.to_string(),
        operational_identity_sha256: pre_book.operational_identity_sha256.clone(),
        package_generation: pre_book.package_generation,
        account_id: pre_book.account_id.clone(),
        instrument: pre_book.instrument.clone(),
        attribution_fingerprint_sha256: attribution_sha256(&pre_book.attribution)?,
        broker_order_id: record.broker_order_id,
        candidate_m10_redis_id: candidate.redis_id,
        candidate_m10_semantic_id_sha256: candidate.semantic_id_sha256,
        candidate_m10_payload_sha256: candidate.payload_sha256,
        candidate_m10_canonical_bytes_sha256: candidate.canonical_bytes_sha256,
        candidate_m10_open_ts_utc_ms: candidate.open_ts_utc_ms,
        candidate_m10_close_ts_utc_ms: candidate.close_ts_utc_ms,
        schedule_fingerprint_sha256: step.schedule_fingerprint_sha256,
        trading_day_identity: step.trading_day_identity,
        last_eligible_m10_redis_id: step.last_eligible_m10_redis_id,
        pre_book_sha256: pre_book.canonical_sha256()?,
        expected_post_book_sha256: post_book_state_sha256(&post_book)?,
        sequence_frontier,
        transition_ordinal: pre_book.transition_ordinal + 1,
        previous_outcome_evidence_sha256,
    };
    let evidence_bytes = evidence.encode_canonical()?;
    post_book.advance(
        &evidence_bytes,
        Stage8bP1d3BookPhase::Eval,
        sequence_frontier,
    )?;
    Ok(Stage8bP1d3LaterEvaluationPlan::Untouched {
        evidence: Box::new(evidence),
        evidence_bytes,
        post_book,
    })
}

#[allow(clippy::too_many_arguments)] // Explicit authenticated inputs make the autonomous transition auditable.
fn build_autonomous_outcome(
    pre_book: &Stage8bP1d3WorkingBookProjectionV1,
    input: Stage8bP1d3AutonomousInput,
    mut record: Stage8bP1d3OrderRecordV1,
    kind: Stage8bP1d3OutcomeKind,
    candidate: Option<Stage8bP1d3CanonicalM10Evidence>,
    step: Option<&Stage8bP1d3ScheduleStepAuthority>,
    expiry: Option<&Stage8bP1d3DayExpiryAuthority>,
    fill_price: Option<Decimal>,
) -> Result<Stage8bP1d3TransitionPlan, Stage8bP1d3Error> {
    let (status, filled, remaining, transition_ts, witness, schedule, day, last_eligible) =
        match kind {
            Stage8bP1d3OutcomeKind::LaterFilled => {
                let bar = candidate
                    .as_ref()
                    .ok_or(Stage8bP1d3Error::InvalidTransition)?;
                let fill = fill_price.ok_or(Stage8bP1d3Error::InvalidTransition)?;
                let authority = step.as_ref().ok_or(Stage8bP1d3Error::InvalidTransition)?;
                if fill <= Decimal::ZERO {
                    return Err(Stage8bP1d3Error::InvalidDecimal);
                }
                (
                    OrderStatus::Filled,
                    record.qty()?,
                    Decimal::ZERO,
                    bar.close_ts_utc_ms,
                    Stage8bP1d3ConsumedWitnessKind::ScheduleStep,
                    authority.schedule_fingerprint_sha256.clone(),
                    authority.trading_day_identity.clone(),
                    authority.last_eligible_m10_redis_id.clone(),
                )
            }
            Stage8bP1d3OutcomeKind::LaterExpired => {
                let authority = expiry.as_ref().ok_or(Stage8bP1d3Error::InvalidTransition)?;
                (
                    OrderStatus::Expired,
                    Decimal::ZERO,
                    record.qty()?,
                    authority.boundary_ts_utc_ms,
                    Stage8bP1d3ConsumedWitnessKind::DayExpiry,
                    authority.schedule_fingerprint_sha256.clone(),
                    authority.trading_day_identity.clone(),
                    authority.last_eligible_m10_redis_id.clone(),
                )
            }
            _ => return Err(Stage8bP1d3Error::InvalidTransition),
        };
    let seq_truth = input
        .sequence_allocation_frontier
        .checked_add(1)
        .ok_or(Stage8bP1d3Error::InvalidSequence)?;
    let timestamp = exact_timestamp(transition_ts)?;
    let trade_id = fill_price.map(|_| {
        derive_trade_id(
            &record.broker_order_id,
            &candidate
                .as_ref()
                .expect("filled transition has candidate")
                .semantic_id_sha256,
        )
    });
    let order = exact_order_snapshot(
        &record.account_id,
        &record.broker_order_id,
        &record.original_durable_place_client_id,
        &record.instrument,
        record.side,
        record.qty()?,
        record.limit_price()?,
        status.clone(),
        BrokerOrderLifecycle::Terminal,
        filled,
        remaining,
        timestamp,
    );
    let truth = exact_truth_from_record(
        &record,
        &order,
        trade_id.as_ref(),
        fill_price,
        input.pre_position_qty,
        input.pre_position_avg_price,
        timestamp,
    )?;
    record.status = status;
    record.lifecycle = BrokerOrderLifecycle::Terminal;
    record.filled_qty_decimal_bytes = filled.serialize();
    record.remaining_qty_decimal_bytes = remaining.serialize();
    if let Some(bar) = candidate.as_ref() {
        record.last_evaluated_m10_redis_id = Some(bar.redis_id.clone());
        record.last_evaluated_m10_semantic_id_sha256 = Some(bar.semantic_id_sha256.clone());
        record.last_evaluated_m10_payload_sha256 = Some(bar.payload_sha256.clone());
        record.last_evaluated_close_ts_utc_ms = Some(bar.close_ts_utc_ms);
    }
    record.schedule_fingerprint_sha256 = schedule.clone();
    record.trading_day_identity = day.clone();
    record.latest_order_projection_sha256 = sha256_json(&order)?;
    record.broker_trade_id = trade_id.clone();
    record.total_sequence_frontier = seq_truth;
    record.transition_ordinal = pre_book.transition_ordinal + 1;
    let mut post_book = pre_book.clone();
    post_book.insert_or_replace(record.clone())?;
    let evidence = outcome_evidence(
        pre_book,
        kind,
        None,
        None,
        None,
        None,
        &record,
        candidate.as_ref(),
        expiry,
        witness,
        &schedule,
        &day,
        &last_eligible,
        transition_ts,
        None,
        Some(seq_truth),
        input.sequence_allocation_frontier,
        None,
        crate::Stage6JournalRecordId::derive_stage8b_p1d3_outcome(
            &input.operational_identity_sha256,
            &record.broker_order_id,
            pre_book.transition_ordinal + 1,
            kind.canonical_name(),
        )
        .as_str()
        .to_string(),
        input.stage6_predecessor_frontier_sha256,
        input.stage6_reserved_checkpoint_sha256,
        None,
        None,
        input.previous_outcome_evidence_sha256,
        fill_price,
        input.pre_position_qty,
        input.pre_position_avg_price,
        post_book_state_sha256(&post_book)?,
    )?;
    let evidence_bytes = evidence.encode_canonical()?;
    let evidence_sha256 = evidence.digest_sha256()?;
    post_book.latest_outcome_evidence_sha256 = evidence_sha256.clone();
    post_book.advance(&evidence_bytes, Stage8bP1d3BookPhase::Terminal, seq_truth)?;
    Ok(Stage8bP1d3TransitionPlan {
        evidence,
        evidence_bytes,
        evidence_sha256,
        post_book,
        ack: None,
        truth: Some(truth),
    })
}

pub(crate) fn build_cancel_transition(
    pre_book: &Stage8bP1d3WorkingBookProjectionV1,
    input: Stage8bP1d3CancelInput,
    candidate: Stage8bP1d3CanonicalM10Evidence,
    step: Stage8bP1d3ScheduleStepAuthority,
) -> Result<Stage8bP1d3CancelTransitionPlan, Stage8bP1d3Error> {
    pre_book.validate()?;
    validate_cancel_input(pre_book, &input)?;
    validate_scoped_bar(pre_book, &candidate)?;
    if candidate.close_ts_utc_ms <= input.decision_m10_close_ts_utc_ms
        || candidate.redis_id == input.decision_m10_redis_id
    {
        return Err(Stage8bP1d3Error::InvalidChronology);
    }
    let record = pre_book
        .record(&input.target_broker_order_id)
        .cloned()
        .ok_or(Stage8bP1d3Error::InvalidRegistry)?;
    if input
        .target_place_client_id
        .as_ref()
        .is_some_and(|value| value != &record.original_durable_place_client_id)
    {
        return Err(Stage8bP1d3Error::IdentityMismatch);
    }
    let predecessor = record
        .last_evaluated_m10_redis_id
        .as_deref()
        .unwrap_or(record.decision_m10_redis_id.as_str());
    validate_step(&step, predecessor, &candidate)?;

    if record.lifecycle == BrokerOrderLifecycle::Active {
        let (decision, fill) = decide_limit(record.side, record.limit_price()?, &candidate)?;
        if decision == Stage8bP1d3LimitDecision::Filled {
            let target_stage6_reserved_checkpoint_sha256 = input
                .target_stage6_reserved_checkpoint_sha256
                .clone()
                .ok_or(Stage8bP1d3Error::IdentityMismatch)?;
            let target = build_autonomous_outcome(
                pre_book,
                Stage8bP1d3AutonomousInput {
                    operational_identity_sha256: input.operational_identity_sha256.clone(),
                    package_generation: input.package_generation,
                    account_id: input.account_id.clone(),
                    instrument: input.instrument.clone(),
                    attribution: input.attribution.clone(),
                    pre_position_qty: input.pre_position_qty,
                    pre_position_avg_price: input.pre_position_avg_price,
                    sequence_allocation_frontier: input.sequence_allocation_frontier,
                    stage6_predecessor_frontier_sha256: input
                        .stage6_predecessor_frontier_sha256
                        .clone(),
                    stage6_reserved_checkpoint_sha256: target_stage6_reserved_checkpoint_sha256
                        .clone(),
                    previous_outcome_evidence_sha256: input
                        .previous_outcome_evidence_sha256
                        .clone(),
                },
                record,
                Stage8bP1d3OutcomeKind::LaterFilled,
                Some(candidate.clone()),
                Some(&step),
                None,
                fill,
            )?;
            let continuation = Stage8bP1d3DeferredCancelAfterTarget {
                input,
                candidate,
                step,
                expected_target_post_book_sha256: post_book_state_sha256(&target.post_book)?,
                target_outcome_evidence_sha256: target.evidence_sha256.clone(),
            };
            return Ok(Stage8bP1d3CancelTransitionPlan::TargetThenCancel {
                target_transition: Box::new(target),
                continuation: Box::new(continuation),
            });
        }
        if input.target_stage6_reserved_checkpoint_sha256.is_some() {
            return Err(Stage8bP1d3Error::IdentityMismatch);
        }
        let cancel = build_cancel_outcome(
            pre_book,
            &input,
            record,
            Stage8bP1d3OutcomeKind::CancelCanceled,
            &candidate,
            &step,
            input.sequence_allocation_frontier,
            input.previous_outcome_evidence_sha256.clone(),
            input.stage6_predecessor_frontier_sha256.clone(),
            input.stage6_reserved_checkpoint_sha256.clone(),
            3,
        )?;
        return Ok(Stage8bP1d3CancelTransitionPlan::Ready(Box::new(cancel)));
    }

    let kind = match record.status {
        OrderStatus::Filled => Stage8bP1d3OutcomeKind::CancelExecutionObserved,
        OrderStatus::Canceled | OrderStatus::Expired | OrderStatus::Rejected => {
            Stage8bP1d3OutcomeKind::CancelAlreadyTerminalNonExecution
        }
        _ => return Err(Stage8bP1d3Error::InvalidTransition),
    };
    if input.target_stage6_reserved_checkpoint_sha256.is_some() {
        return Err(Stage8bP1d3Error::IdentityMismatch);
    }
    let cancel = build_cancel_outcome(
        pre_book,
        &input,
        record,
        kind,
        &candidate,
        &step,
        input.sequence_allocation_frontier,
        input.previous_outcome_evidence_sha256.clone(),
        input.stage6_predecessor_frontier_sha256.clone(),
        input.stage6_reserved_checkpoint_sha256.clone(),
        3,
    )?;
    Ok(Stage8bP1d3CancelTransitionPlan::Ready(Box::new(cancel)))
}

/// Reconstructs the cancel continuation after an earlier `LaterFilled`
/// target outcome has already been replacement-sealed. The consumed M10 and
/// schedule witness are recovered only from the authenticated target fact;
/// no caller-supplied market data or schedule authority is accepted here.
pub(crate) fn build_cancel_after_target_transition(
    pre_book: &Stage8bP1d3WorkingBookProjectionV1,
    input: Stage8bP1d3CancelInput,
    target_evidence: &Stage8bP1d3OutcomeEvidenceV1,
) -> Result<Stage8bP1d3TransitionPlan, Stage8bP1d3Error> {
    pre_book.validate()?;
    target_evidence.validate()?;
    if target_evidence.outcome_kind != Stage8bP1d3OutcomeKind::LaterFilled
        || target_evidence.request_id.is_some()
        || target_evidence.broker_order_id != input.target_broker_order_id
        || target_evidence.digest_sha256()? != input.previous_outcome_evidence_sha256
        || target_evidence.expected_post_book_sha256 != post_book_state_sha256(pre_book)?
        || target_evidence
            .candidate_m10_redis_id
            .as_deref()
            .map_or(true, |value| {
                value != format!("{}-0", target_evidence.transition_source_ts_utc_ms)
            })
    {
        return Err(Stage8bP1d3Error::IdentityMismatch);
    }
    let candidate_close = target_evidence
        .candidate_m10_close_ts_utc_ms
        .ok_or(Stage8bP1d3Error::InvalidChronology)?;
    let candidate_open = target_evidence
        .candidate_m10_open_ts_utc_ms
        .ok_or(Stage8bP1d3Error::InvalidChronology)?;
    let fill_price = target_evidence
        .fill_price_decimal_bytes
        .map(decimal)
        .transpose()?
        .ok_or(Stage8bP1d3Error::InvalidTransition)?;
    let candidate = Stage8bP1d3CanonicalM10Evidence {
        redis_id: target_evidence
            .candidate_m10_redis_id
            .clone()
            .ok_or(Stage8bP1d3Error::InvalidChronology)?,
        semantic_id_sha256: target_evidence
            .candidate_m10_semantic_id_sha256
            .clone()
            .ok_or(Stage8bP1d3Error::InvalidChronology)?,
        payload_sha256: target_evidence
            .candidate_m10_payload_sha256
            .clone()
            .ok_or(Stage8bP1d3Error::InvalidChronology)?,
        // Canonical source bytes were authenticated before the target fact
        // was written. Their standalone digest is not part of the accepted
        // outcome wire schema and is not consulted by a terminal-target
        // cancel continuation, so bind this internal reconstruction to the
        // authenticated payload digest rather than accepting caller input.
        canonical_bytes_sha256: target_evidence
            .candidate_m10_payload_sha256
            .clone()
            .ok_or(Stage8bP1d3Error::InvalidChronology)?,
        operational_identity_sha256: target_evidence.operational_identity_sha256.clone(),
        instrument: target_evidence.instrument.clone(),
        open_ts_utc_ms: candidate_open,
        close_ts_utc_ms: candidate_close,
        open: fill_price,
        high: fill_price,
        low: fill_price,
        close: fill_price,
    };
    let step = Stage8bP1d3ScheduleStepAuthority {
        schedule_fingerprint_sha256: target_evidence.schedule_fingerprint_sha256.clone(),
        trading_day_identity: target_evidence.trading_day_identity.clone(),
        last_eligible_m10_redis_id: target_evidence.last_eligible_m10_redis_id.clone(),
        predecessor_redis_id: candidate.redis_id.clone(),
        candidate_redis_id: candidate.redis_id.clone(),
    };
    validate_cancel_input(pre_book, &input)?;
    let record = pre_book
        .record(&input.target_broker_order_id)
        .cloned()
        .ok_or(Stage8bP1d3Error::InvalidRegistry)?;
    if record.lifecycle != BrokerOrderLifecycle::Terminal || record.status != OrderStatus::Filled {
        return Err(Stage8bP1d3Error::InvalidTransition);
    }
    build_cancel_outcome(
        pre_book,
        &input,
        record,
        Stage8bP1d3OutcomeKind::CancelExecutionObserved,
        &candidate,
        &step,
        input.sequence_allocation_frontier,
        input.previous_outcome_evidence_sha256.clone(),
        input.stage6_predecessor_frontier_sha256.clone(),
        input.stage6_reserved_checkpoint_sha256.clone(),
        4,
    )
}

#[allow(clippy::too_many_arguments)]
fn build_cancel_outcome(
    pre_book: &Stage8bP1d3WorkingBookProjectionV1,
    input: &Stage8bP1d3CancelInput,
    mut record: Stage8bP1d3OrderRecordV1,
    kind: Stage8bP1d3OutcomeKind,
    candidate: &Stage8bP1d3CanonicalM10Evidence,
    step: &Stage8bP1d3ScheduleStepAuthority,
    sequence_frontier: u64,
    previous_outcome_evidence_sha256: String,
    stage6_predecessor_frontier_sha256: String,
    stage6_reserved_checkpoint_sha256: String,
    stage6_outcome_lifecycle_sequence: u64,
) -> Result<Stage8bP1d3TransitionPlan, Stage8bP1d3Error> {
    let timestamp = exact_timestamp(candidate.close_ts_utc_ms)?;
    let (ack_status, ack_reason, seq_ack, seq_truth, truth, phase, changed_book) = match kind {
        Stage8bP1d3OutcomeKind::CancelCanceled => {
            if record.lifecycle != BrokerOrderLifecycle::Active
                || record.status != OrderStatus::Working
            {
                return Err(Stage8bP1d3Error::InvalidTransition);
            }
            let (seq_ack, seq_truth) = allocate_pair(sequence_frontier)?;
            let order = exact_order_snapshot(
                &record.account_id,
                &record.broker_order_id,
                &record.original_durable_place_client_id,
                &record.instrument,
                record.side,
                record.qty()?,
                record.limit_price()?,
                OrderStatus::Canceled,
                BrokerOrderLifecycle::Terminal,
                Decimal::ZERO,
                record.qty()?,
                timestamp,
            );
            record.status = OrderStatus::Canceled;
            record.lifecycle = BrokerOrderLifecycle::Terminal;
            record.last_evaluated_m10_redis_id = Some(candidate.redis_id.clone());
            record.last_evaluated_m10_semantic_id_sha256 =
                Some(candidate.semantic_id_sha256.clone());
            record.last_evaluated_m10_payload_sha256 = Some(candidate.payload_sha256.clone());
            record.last_evaluated_close_ts_utc_ms = Some(candidate.close_ts_utc_ms);
            record.schedule_fingerprint_sha256 = step.schedule_fingerprint_sha256.clone();
            record.trading_day_identity = step.trading_day_identity.clone();
            record.latest_order_projection_sha256 = sha256_json(&order)?;
            record.total_sequence_frontier = seq_truth;
            record.transition_ordinal = pre_book.transition_ordinal + 1;
            let truth = BrokerTruthSnapshot {
                account_id: record.account_id.clone(),
                orders: vec![order],
                positions: Vec::new(),
                cash: None,
                trades: Vec::new(),
                instruments: Vec::new(),
                received_ts: timestamp,
            };
            (
                CommandAckStatus::Accepted,
                None,
                seq_ack,
                Some(seq_truth),
                Some(truth),
                Stage8bP1d3BookPhase::Terminal,
                true,
            )
        }
        Stage8bP1d3OutcomeKind::CancelExecutionObserved
        | Stage8bP1d3OutcomeKind::CancelAlreadyTerminalNonExecution => {
            if record.lifecycle != BrokerOrderLifecycle::Terminal {
                return Err(Stage8bP1d3Error::InvalidTransition);
            }
            let seq_ack = sequence_frontier
                .checked_add(1)
                .ok_or(Stage8bP1d3Error::InvalidSequence)?;
            (
                CommandAckStatus::Recovered,
                Some(CommandAckReason::new(
                    CommandAckReasonCode::RecoveredByBrokerTruth,
                )),
                seq_ack,
                None,
                None,
                Stage8bP1d3BookPhase::CancelRecovered,
                false,
            )
        }
        _ => return Err(Stage8bP1d3Error::InvalidTransition),
    };
    let ack = CommandAck {
        request_id: input.request_id,
        client_order_id: Some(input.durable_request_client_id.clone()),
        broker_order_id: Some(record.broker_order_id.clone()),
        status: ack_status,
        reason: ack_reason,
        received_ts: timestamp,
    };
    let mut post_book = pre_book.clone();
    if changed_book {
        post_book.insert_or_replace(record.clone())?;
    }
    let expected_post_book_sha256 = if changed_book {
        post_book_state_sha256(&post_book)?
    } else {
        pre_book.canonical_sha256()?
    };
    let outcome_transition_ordinal = if changed_book {
        pre_book.transition_ordinal + 1
    } else {
        pre_book.transition_ordinal
    };
    let outcome_record_id = crate::Stage6JournalRecordId::derive_stage8b_p1d3_outcome(
        &input.operational_identity_sha256,
        &record.broker_order_id,
        outcome_transition_ordinal,
        kind.canonical_name(),
    )
    .as_str()
    .to_string();
    let cancel_command = CancelOrder {
        request_id: input.request_id,
        created_ts: exact_timestamp(input.decision_m10_close_ts_utc_ms)?,
        ttl_ms: None,
        account_id: input.account_id.clone(),
        order_id: input.target_broker_order_id.clone(),
        client_order_id: input.target_place_client_id.clone(),
    };
    let cancel_identity = crate::Stage6DurableRequestIdentityV1::from_cancel(
        &cancel_command,
        input.instrument.clone(),
        input.attribution.clone(),
    )
    .map_err(|_| Stage8bP1d3Error::IdentityMismatch)?;
    let (request_finalized_record_id, request_finalized_fingerprint_sha256) =
        request_finalization_binding(
            cancel_identity,
            &outcome_record_id,
            stage6_outcome_lifecycle_sequence,
            timestamp,
        )?;
    let evidence = Stage8bP1d3OutcomeEvidenceV1 {
        schema_version: STAGE8B_P1D3_OUTCOME_EVIDENCE_SCHEMA_VERSION,
        domain: STAGE8B_P1D3_OUTCOME_EVIDENCE_DOMAIN.to_string(),
        outcome_kind: kind,
        transition_ordinal: outcome_transition_ordinal,
        operational_identity_sha256: input.operational_identity_sha256.clone(),
        package_generation: input.package_generation,
        account_id: input.account_id.clone(),
        instrument: input.instrument.clone(),
        attribution_fingerprint_sha256: attribution_sha256(&input.attribution)?,
        request_id: Some(input.request_id),
        durable_request_client_id: Some(input.durable_request_client_id.clone()),
        canonical_command_sha256: Some(input.canonical_command_sha256.clone()),
        accepted_command_payload_sha256: Some(input.accepted_command_payload_sha256.clone()),
        accepted_stage6_identity_sha256: input.accepted_stage6_identity_sha256.clone(),
        target_place_client_id: Some(record.original_durable_place_client_id.clone()),
        target_broker_order_id: Some(record.broker_order_id.clone()),
        source_m10_redis_id: Some(input.decision_m10_redis_id.clone()),
        source_m10_semantic_id_sha256: Some(input.decision_m10_semantic_id_sha256.clone()),
        source_m10_payload_sha256: Some(input.decision_m10_payload_sha256.clone()),
        source_m10_open_ts_utc_ms: Some(input.decision_m10_open_ts_utc_ms),
        source_m10_close_ts_utc_ms: Some(input.decision_m10_close_ts_utc_ms),
        candidate_m10_redis_id: Some(candidate.redis_id.clone()),
        candidate_m10_semantic_id_sha256: Some(candidate.semantic_id_sha256.clone()),
        candidate_m10_payload_sha256: Some(candidate.payload_sha256.clone()),
        candidate_m10_open_ts_utc_ms: Some(candidate.open_ts_utc_ms),
        candidate_m10_close_ts_utc_ms: Some(candidate.close_ts_utc_ms),
        consumed_witness_kind: Stage8bP1d3ConsumedWitnessKind::ScheduleStep,
        schedule_fingerprint_sha256: step.schedule_fingerprint_sha256.clone(),
        trading_day_identity: step.trading_day_identity.clone(),
        last_eligible_m10_redis_id: step.last_eligible_m10_redis_id.clone(),
        boundary_ts_utc_ms: None,
        pre_book_generation: pre_book.package_generation,
        pre_book_sha256: pre_book.canonical_sha256()?,
        expected_post_book_sha256,
        broker_order_id: record.broker_order_id.clone(),
        broker_trade_id: record.broker_trade_id.clone(),
        side: record.side,
        qty_decimal_bytes: record.qty_decimal_bytes,
        limit_price_decimal_bytes: record.limit_price_decimal_bytes,
        fill_price_decimal_bytes: None,
        pre_position_qty_decimal_bytes: input.pre_position_qty.serialize(),
        pre_position_avg_price_decimal_bytes: input
            .pre_position_avg_price
            .map(|value| value.serialize()),
        transition_source_ts_utc_ms: candidate.close_ts_utc_ms,
        transition_received_ts_utc_ms: candidate.close_ts_utc_ms,
        reserved_seq_ack: Some(seq_ack),
        reserved_seq_truth: seq_truth,
        sequence_allocation_frontier: sequence_frontier,
        stage6_dispatch_record_id: Some(input.stage6_dispatch_record_id.clone()),
        stage6_outcome_record_id: outcome_record_id,
        stage6_predecessor_frontier_sha256,
        stage6_reserved_checkpoint_sha256,
        stage7_request_finalized_record_id: Some(request_finalized_record_id),
        stage7_request_finalized_fingerprint_sha256: Some(request_finalized_fingerprint_sha256),
        previous_outcome_evidence_sha256,
    };
    let evidence_bytes = evidence.encode_canonical()?;
    let evidence_sha256 = evidence.digest_sha256()?;
    if changed_book {
        post_book.latest_outcome_evidence_sha256 = evidence_sha256.clone();
        post_book.advance(
            &evidence_bytes,
            phase,
            seq_truth.ok_or(Stage8bP1d3Error::InvalidSequence)?,
        )?;
    }
    Ok(Stage8bP1d3TransitionPlan {
        evidence,
        evidence_bytes,
        evidence_sha256,
        post_book,
        ack: Some(ack),
        truth,
    })
}

pub(crate) fn decide_limit(
    side: OrderSide,
    limit_price: Decimal,
    bar: &Stage8bP1d3CanonicalM10Evidence,
) -> Result<(Stage8bP1d3LimitDecision, Option<Decimal>), Stage8bP1d3Error> {
    validate_bar(bar)?;
    if limit_price <= Decimal::ZERO {
        return Err(Stage8bP1d3Error::InvalidDecimal);
    }
    match side {
        OrderSide::Buy if bar.low > limit_price => Ok((Stage8bP1d3LimitDecision::Working, None)),
        OrderSide::Buy => Ok((
            Stage8bP1d3LimitDecision::Filled,
            Some(if bar.open < limit_price {
                bar.open
            } else {
                limit_price
            }),
        )),
        OrderSide::Sell if bar.high < limit_price => Ok((Stage8bP1d3LimitDecision::Working, None)),
        OrderSide::Sell => Ok((
            Stage8bP1d3LimitDecision::Filled,
            Some(if bar.open > limit_price {
                bar.open
            } else {
                limit_price
            }),
        )),
    }
}

/// Pure no-effect validation performed before the Stage 6 dispatch row is
/// appended.  It intentionally borrows the one-use schedule authority; the
/// same value is consumed exactly once by the transition builder only after
/// durable dispatch succeeds.
pub(crate) fn preflight_initial_limit_transition(
    pre_book: &Stage8bP1d3WorkingBookProjectionV1,
    input: &Stage8bP1d3InitialLimitInput,
    observation: &Stage8bP1d3InitialObservation,
) -> Result<(), Stage8bP1d3Error> {
    pre_book.validate()?;
    validate_initial_input(pre_book, input)?;
    if pre_book.active_record().is_some()
        || pre_book.records.len() >= P1D3_MAX_ORDER_RECORDS_PER_GENERATION
    {
        return Err(Stage8bP1d3Error::InvalidRegistry);
    }
    match observation {
        Stage8bP1d3InitialObservation::Candidate { evidence, schedule } => {
            validate_scoped_bar(pre_book, evidence)?;
            validate_step(schedule, &input.decision_m10_redis_id, evidence)?;
            if evidence.close_ts_utc_ms <= input.decision_m10_close_ts_utc_ms {
                return Err(Stage8bP1d3Error::InvalidChronology);
            }
            decide_limit(input.side, input.limit_price, evidence)?;
        }
        Stage8bP1d3InitialObservation::DayExpiry { authority } => {
            validate_initial_expiry(input, authority)?;
        }
    }
    Ok(())
}

pub(crate) fn build_preflighted_initial_limit_transition(
    pre_book: &Stage8bP1d3WorkingBookProjectionV1,
    input: Stage8bP1d3InitialLimitInput,
    observation: Stage8bP1d3InitialObservation,
) -> Result<Stage8bP1d3TransitionPlan, Stage8bP1d3Error> {
    match observation {
        Stage8bP1d3InitialObservation::Candidate { evidence, schedule } => {
            build_initial_limit_transition(pre_book, input, Some(*evidence), Some(schedule), None)
        }
        Stage8bP1d3InitialObservation::DayExpiry { authority } => {
            build_initial_limit_transition(pre_book, input, None, None, Some(authority))
        }
    }
}

pub(crate) fn build_initial_limit_transition(
    pre_book: &Stage8bP1d3WorkingBookProjectionV1,
    input: Stage8bP1d3InitialLimitInput,
    candidate: Option<Stage8bP1d3CanonicalM10Evidence>,
    step: Option<Stage8bP1d3ScheduleStepAuthority>,
    expiry: Option<Stage8bP1d3DayExpiryAuthority>,
) -> Result<Stage8bP1d3TransitionPlan, Stage8bP1d3Error> {
    pre_book.validate()?;
    validate_initial_input(pre_book, &input)?;
    if pre_book.active_record().is_some()
        || pre_book.records.len() >= P1D3_MAX_ORDER_RECORDS_PER_GENERATION
    {
        return Err(Stage8bP1d3Error::InvalidRegistry);
    }
    let broker_order_id = derive_order_id(
        &input.operational_identity_sha256,
        input.request_id,
        &input.canonical_command_sha256,
    );
    let order_fingerprint = order_fingerprint_sha256(
        &broker_order_id,
        input.request_id,
        &input.durable_client_order_id,
        &input.canonical_command_sha256,
    );
    let (kind, witness_kind, schedule, transition_ts, status, lifecycle, filled, remaining, fill) =
        match (candidate.as_ref(), step.as_ref(), expiry.as_ref()) {
            (Some(bar), Some(step), None) => {
                validate_scoped_bar(pre_book, bar)?;
                validate_step(step, &input.decision_m10_redis_id, bar)?;
                if bar.close_ts_utc_ms <= input.decision_m10_close_ts_utc_ms {
                    return Err(Stage8bP1d3Error::InvalidChronology);
                }
                let (decision, fill) = decide_limit(input.side, input.limit_price, bar)?;
                match decision {
                    Stage8bP1d3LimitDecision::Working => (
                        Stage8bP1d3OutcomeKind::InitialWorking,
                        Stage8bP1d3ConsumedWitnessKind::ScheduleStep,
                        step.schedule_fingerprint_sha256.clone(),
                        bar.close_ts_utc_ms,
                        OrderStatus::Working,
                        BrokerOrderLifecycle::Active,
                        Decimal::ZERO,
                        input.qty,
                        None,
                    ),
                    Stage8bP1d3LimitDecision::Filled => (
                        Stage8bP1d3OutcomeKind::InitialFilled,
                        Stage8bP1d3ConsumedWitnessKind::ScheduleStep,
                        step.schedule_fingerprint_sha256.clone(),
                        bar.close_ts_utc_ms,
                        OrderStatus::Filled,
                        BrokerOrderLifecycle::Terminal,
                        input.qty,
                        Decimal::ZERO,
                        fill,
                    ),
                }
            }
            (None, None, Some(expiry)) => {
                validate_initial_expiry(&input, expiry)?;
                (
                    Stage8bP1d3OutcomeKind::InitialExpired,
                    Stage8bP1d3ConsumedWitnessKind::DayExpiry,
                    expiry.schedule_fingerprint_sha256.clone(),
                    expiry.boundary_ts_utc_ms,
                    OrderStatus::Expired,
                    BrokerOrderLifecycle::Terminal,
                    Decimal::ZERO,
                    input.qty,
                    None,
                )
            }
            _ => return Err(Stage8bP1d3Error::InvalidTransition),
        };
    let transition_time = exact_timestamp(transition_ts)?;
    let broker_trade_id = fill.map(|_| {
        derive_trade_id(
            &broker_order_id,
            candidate
                .as_ref()
                .expect("fill requires candidate")
                .semantic_id_sha256
                .as_str(),
        )
    });
    let (seq_ack, seq_truth) = allocate_pair(input.sequence_allocation_frontier)?;
    let order = exact_order_snapshot(
        &input.account_id,
        &broker_order_id,
        &input.durable_client_order_id,
        &input.instrument,
        input.side,
        input.qty,
        input.limit_price,
        status.clone(),
        lifecycle,
        filled,
        remaining,
        transition_time,
    );
    let truth = exact_truth(
        &input,
        &order,
        broker_trade_id.as_ref(),
        fill,
        transition_time,
    )?;
    let ack = CommandAck {
        request_id: input.request_id,
        client_order_id: Some(input.durable_client_order_id.clone()),
        broker_order_id: Some(broker_order_id.clone()),
        status: CommandAckStatus::Accepted,
        reason: None,
        received_ts: transition_time,
    };
    let mut post_book = pre_book.clone();
    let record = Stage8bP1d3OrderRecordV1 {
        broker_order_id: broker_order_id.clone(),
        original_request_id: input.request_id,
        original_durable_place_client_id: input.durable_client_order_id.clone(),
        canonical_command_sha256: input.canonical_command_sha256.clone(),
        accepted_stage6_identity_sha256: input.accepted_stage6_identity_sha256.clone(),
        deterministic_order_fingerprint_sha256: order_fingerprint,
        account_id: input.account_id.clone(),
        instrument: input.instrument.clone(),
        attribution: input.attribution.clone(),
        side: input.side,
        qty_decimal_bytes: input.qty.serialize(),
        limit_price_decimal_bytes: input.limit_price.serialize(),
        status,
        lifecycle,
        filled_qty_decimal_bytes: filled.serialize(),
        remaining_qty_decimal_bytes: remaining.serialize(),
        decision_m10_redis_id: input.decision_m10_redis_id.clone(),
        decision_m10_semantic_id_sha256: input.decision_m10_semantic_id_sha256.clone(),
        decision_m10_payload_sha256: input.decision_m10_payload_sha256.clone(),
        decision_m10_open_ts_utc_ms: input.decision_m10_open_ts_utc_ms,
        decision_m10_close_ts_utc_ms: input.decision_m10_close_ts_utc_ms,
        first_observation_redis_id: candidate.as_ref().map(|bar| bar.redis_id.clone()),
        first_observation_semantic_id_sha256: candidate
            .as_ref()
            .map(|bar| bar.semantic_id_sha256.clone()),
        first_observation_payload_sha256: candidate.as_ref().map(|bar| bar.payload_sha256.clone()),
        last_evaluated_m10_redis_id: candidate.as_ref().map(|bar| bar.redis_id.clone()),
        last_evaluated_m10_semantic_id_sha256: candidate
            .as_ref()
            .map(|bar| bar.semantic_id_sha256.clone()),
        last_evaluated_m10_payload_sha256: candidate.as_ref().map(|bar| bar.payload_sha256.clone()),
        last_evaluated_close_ts_utc_ms: candidate.as_ref().map(|bar| bar.close_ts_utc_ms),
        schedule_fingerprint_sha256: schedule,
        trading_day_identity: candidate
            .as_ref()
            .map(|bar| trading_day(bar.close_ts_utc_ms))
            .transpose()?
            .unwrap_or_else(|| {
                expiry
                    .as_ref()
                    .expect("expiry branch")
                    .trading_day_identity
                    .clone()
            }),
        latest_order_projection_sha256: sha256_json(&order)?,
        broker_trade_id: broker_trade_id.clone(),
        total_sequence_frontier: seq_truth,
        transition_ordinal: pre_book.transition_ordinal + 1,
    };
    let outcome_record_id = crate::Stage6JournalRecordId::derive_stage8b_p1d3_outcome(
        &input.operational_identity_sha256,
        &record.broker_order_id,
        pre_book.transition_ordinal + 1,
        kind.canonical_name(),
    )
    .as_str()
    .to_string();
    let place_command = PlaceOrder {
        request_id: input.request_id,
        created_ts: exact_timestamp(input.decision_m10_close_ts_utc_ms)?,
        ttl_ms: None,
        account_id: input.account_id.clone(),
        client_order_id: input.durable_client_order_id.clone(),
        instrument: input.instrument.clone(),
        side: input.side,
        order_type: OrderType::Limit,
        qty: input.qty,
        limit_price: Some(input.limit_price),
        time_in_force: TimeInForce::Day,
        comment: Some(input.attribution.internal_comment().to_string()),
    };
    let place_identity = crate::Stage6DurableRequestIdentityV1::from_place(
        &place_command,
        input.attribution.clone(),
    )
    .map_err(|_| Stage8bP1d3Error::IdentityMismatch)?;
    let (request_finalized_record_id, request_finalized_fingerprint_sha256) =
        request_finalization_binding(place_identity, &outcome_record_id, 3, transition_time)?;
    post_book.insert_or_replace(record)?;
    let expected_post_book_sha256 = post_book_state_sha256(&post_book)?;
    let evidence = Stage8bP1d3OutcomeEvidenceV1 {
        schema_version: STAGE8B_P1D3_OUTCOME_EVIDENCE_SCHEMA_VERSION,
        domain: STAGE8B_P1D3_OUTCOME_EVIDENCE_DOMAIN.to_string(),
        outcome_kind: kind,
        transition_ordinal: pre_book.transition_ordinal + 1,
        operational_identity_sha256: input.operational_identity_sha256,
        package_generation: input.package_generation,
        account_id: input.account_id,
        instrument: input.instrument,
        attribution_fingerprint_sha256: attribution_sha256(&input.attribution)?,
        request_id: Some(input.request_id),
        durable_request_client_id: Some(input.durable_client_order_id.clone()),
        canonical_command_sha256: Some(input.canonical_command_sha256),
        accepted_command_payload_sha256: Some(input.accepted_command_payload_sha256),
        accepted_stage6_identity_sha256: input.accepted_stage6_identity_sha256,
        target_place_client_id: Some(input.durable_client_order_id),
        target_broker_order_id: Some(broker_order_id.clone()),
        source_m10_redis_id: Some(input.decision_m10_redis_id),
        source_m10_semantic_id_sha256: Some(input.decision_m10_semantic_id_sha256),
        source_m10_payload_sha256: Some(input.decision_m10_payload_sha256),
        source_m10_open_ts_utc_ms: Some(input.decision_m10_open_ts_utc_ms),
        source_m10_close_ts_utc_ms: Some(input.decision_m10_close_ts_utc_ms),
        candidate_m10_redis_id: candidate.as_ref().map(|bar| bar.redis_id.clone()),
        candidate_m10_semantic_id_sha256: candidate
            .as_ref()
            .map(|bar| bar.semantic_id_sha256.clone()),
        candidate_m10_payload_sha256: candidate.as_ref().map(|bar| bar.payload_sha256.clone()),
        candidate_m10_open_ts_utc_ms: candidate.as_ref().map(|bar| bar.open_ts_utc_ms),
        candidate_m10_close_ts_utc_ms: candidate.as_ref().map(|bar| bar.close_ts_utc_ms),
        consumed_witness_kind: witness_kind,
        schedule_fingerprint_sha256: match (&step, &expiry) {
            (Some(value), None) => value.schedule_fingerprint_sha256.clone(),
            (None, Some(value)) => value.schedule_fingerprint_sha256.clone(),
            _ => unreachable!(),
        },
        trading_day_identity: match (&step, &expiry) {
            (Some(value), None) => value.trading_day_identity.clone(),
            (None, Some(value)) => value.trading_day_identity.clone(),
            _ => unreachable!(),
        },
        last_eligible_m10_redis_id: match (&step, &expiry) {
            (Some(value), None) => value.last_eligible_m10_redis_id.clone(),
            (None, Some(value)) => value.last_eligible_m10_redis_id.clone(),
            _ => unreachable!(),
        },
        boundary_ts_utc_ms: expiry.as_ref().map(|value| value.boundary_ts_utc_ms),
        pre_book_generation: pre_book.package_generation,
        pre_book_sha256: pre_book.canonical_sha256()?,
        expected_post_book_sha256,
        broker_order_id,
        broker_trade_id,
        side: input.side,
        qty_decimal_bytes: input.qty.serialize(),
        limit_price_decimal_bytes: input.limit_price.serialize(),
        fill_price_decimal_bytes: fill.map(|value| value.serialize()),
        pre_position_qty_decimal_bytes: input.pre_position_qty.serialize(),
        pre_position_avg_price_decimal_bytes: input
            .pre_position_avg_price
            .map(|value| value.serialize()),
        transition_source_ts_utc_ms: transition_ts,
        transition_received_ts_utc_ms: transition_ts,
        reserved_seq_ack: Some(seq_ack),
        reserved_seq_truth: Some(seq_truth),
        sequence_allocation_frontier: input.sequence_allocation_frontier,
        stage6_dispatch_record_id: Some(input.stage6_dispatch_record_id),
        stage6_outcome_record_id: outcome_record_id,
        stage6_predecessor_frontier_sha256: input.stage6_predecessor_frontier_sha256,
        stage6_reserved_checkpoint_sha256: input.stage6_reserved_checkpoint_sha256,
        stage7_request_finalized_record_id: Some(request_finalized_record_id),
        stage7_request_finalized_fingerprint_sha256: Some(request_finalized_fingerprint_sha256),
        previous_outcome_evidence_sha256: input.previous_outcome_evidence_sha256,
    };
    let evidence_bytes = evidence.encode_canonical()?;
    let evidence_sha256 = evidence.digest_sha256()?;
    let phase = if lifecycle == BrokerOrderLifecycle::Active {
        Stage8bP1d3BookPhase::Working
    } else {
        Stage8bP1d3BookPhase::Terminal
    };
    post_book.latest_outcome_evidence_sha256 = evidence_sha256.clone();
    post_book.advance(&evidence_bytes, phase, seq_truth)?;
    Ok(Stage8bP1d3TransitionPlan {
        evidence,
        evidence_bytes,
        evidence_sha256,
        post_book,
        ack: Some(ack),
        truth: Some(truth),
    })
}

fn validate_initial_input(
    book: &Stage8bP1d3WorkingBookProjectionV1,
    input: &Stage8bP1d3InitialLimitInput,
) -> Result<(), Stage8bP1d3Error> {
    if input.operational_identity_sha256 != book.operational_identity_sha256
        || input.package_generation != book.package_generation
        || input.account_id != book.account_id
        || input.instrument != book.instrument
        || input.attribution != book.attribution
        || input.durable_client_order_id != ClientOrderId::from_strategy_request(input.request_id)
        || input.qty <= Decimal::ZERO
        || !is_integral(input.qty)
        || input.limit_price <= Decimal::ZERO
        || !is_sha256(&input.canonical_command_sha256)
        || !is_sha256(&input.accepted_command_payload_sha256)
        || !is_sha256(&input.accepted_stage6_identity_sha256)
        || !is_sha256(&input.accepted_stage6_identity_sha256)
        || !is_sha256(&input.decision_m10_semantic_id_sha256)
        || !is_sha256(&input.decision_m10_payload_sha256)
        || input.decision_m10_redis_id != format!("{}-0", input.decision_m10_close_ts_utc_ms)
        || input
            .decision_m10_close_ts_utc_ms
            .checked_sub(input.decision_m10_open_ts_utc_ms)
            != Some(600_000)
        || input.decision_m10_close_ts_utc_ms.rem_euclid(600_000) != 0
        || !is_sha256(&input.stage6_dispatch_record_id)
        || !is_sha256(&input.stage6_predecessor_frontier_sha256)
        || !is_sha256(&input.stage6_reserved_checkpoint_sha256)
        || !is_sha256(&input.previous_outcome_evidence_sha256)
        || input.sequence_allocation_frontier != book.total_sequence_frontier
    {
        return Err(Stage8bP1d3Error::IdentityMismatch);
    }
    Ok(())
}

fn validate_autonomous_input(
    book: &Stage8bP1d3WorkingBookProjectionV1,
    input: &Stage8bP1d3AutonomousInput,
) -> Result<(), Stage8bP1d3Error> {
    if input.operational_identity_sha256 != book.operational_identity_sha256
        || input.package_generation != book.package_generation
        || input.account_id != book.account_id
        || input.instrument != book.instrument
        || input.attribution != book.attribution
        || input.sequence_allocation_frontier != book.total_sequence_frontier
        || !is_sha256(&input.stage6_predecessor_frontier_sha256)
        || !is_sha256(&input.stage6_reserved_checkpoint_sha256)
        || !is_sha256(&input.previous_outcome_evidence_sha256)
        || !is_integral(input.pre_position_qty)
    {
        return Err(Stage8bP1d3Error::IdentityMismatch);
    }
    Ok(())
}

fn validate_cancel_input(
    book: &Stage8bP1d3WorkingBookProjectionV1,
    input: &Stage8bP1d3CancelInput,
) -> Result<(), Stage8bP1d3Error> {
    if input.operational_identity_sha256 != book.operational_identity_sha256
        || input.package_generation != book.package_generation
        || input.account_id != book.account_id
        || input.instrument != book.instrument
        || input.attribution != cancel_attribution(&book.attribution)?
        || input.durable_request_client_id != ClientOrderId::from_strategy_request(input.request_id)
        || input.target_place_client_id.is_none()
        || input.target_place_client_id.as_ref() == Some(&input.durable_request_client_id)
        || input.sequence_allocation_frontier != book.total_sequence_frontier
        || !is_sha256(&input.canonical_command_sha256)
        || !is_sha256(&input.accepted_command_payload_sha256)
        || !is_sha256(&input.decision_m10_semantic_id_sha256)
        || !is_sha256(&input.decision_m10_payload_sha256)
        || input.decision_m10_redis_id != format!("{}-0", input.decision_m10_close_ts_utc_ms)
        || input
            .decision_m10_close_ts_utc_ms
            .checked_sub(input.decision_m10_open_ts_utc_ms)
            != Some(600_000)
        || !is_integral(input.pre_position_qty)
        || !is_sha256(&input.stage6_dispatch_record_id)
        || !is_sha256(&input.stage6_predecessor_frontier_sha256)
        || input
            .target_stage6_reserved_checkpoint_sha256
            .as_ref()
            .is_some_and(|value| !is_sha256(value))
        || !is_sha256(&input.stage6_reserved_checkpoint_sha256)
        || !is_sha256(&input.previous_outcome_evidence_sha256)
    {
        return Err(Stage8bP1d3Error::IdentityMismatch);
    }
    Ok(())
}

fn validate_scoped_bar(
    book: &Stage8bP1d3WorkingBookProjectionV1,
    bar: &Stage8bP1d3CanonicalM10Evidence,
) -> Result<(), Stage8bP1d3Error> {
    validate_bar(bar)?;
    if bar.operational_identity_sha256 != book.operational_identity_sha256
        || bar.instrument != book.instrument
    {
        return Err(Stage8bP1d3Error::IdentityMismatch);
    }
    Ok(())
}

fn request_finalization_binding(
    identity: crate::Stage6DurableRequestIdentityV1,
    outcome_record_id: &str,
    outcome_lifecycle_sequence: u64,
    observed_at: DateTime<Utc>,
) -> Result<(String, String), Stage8bP1d3Error> {
    let outcome_record_id =
        crate::Stage6JournalRecordId::parse_exact(outcome_record_id.to_string())
            .map_err(|_| Stage8bP1d3Error::IdentityMismatch)?;
    let finalized = crate::stage6d_live_core::stage7a_request_finalization_record(
        identity,
        outcome_record_id,
        outcome_lifecycle_sequence,
        observed_at,
        crate::Stage6RequestFinalDispositionV1::Completed,
    )
    .map_err(|_| Stage8bP1d3Error::IdentityMismatch)?;
    Ok((
        finalized.journal_record_id().as_str().to_string(),
        finalized.source_evidence_sha256().as_str().to_string(),
    ))
}

fn validate_expiry(
    record: &Stage8bP1d3OrderRecordV1,
    expiry: &Stage8bP1d3DayExpiryAuthority,
) -> Result<(), Stage8bP1d3Error> {
    if !is_sha256(&expiry.schedule_fingerprint_sha256)
        || expiry.schedule_fingerprint_sha256 != record.schedule_fingerprint_sha256
        || expiry.trading_day_identity != record.trading_day_identity
        || record.last_evaluated_m10_redis_id.as_deref()
            != Some(expiry.last_eligible_m10_redis_id.as_str())
        || record
            .last_evaluated_close_ts_utc_ms
            .map_or(true, |last| expiry.boundary_ts_utc_ms <= last)
        || exact_timestamp(expiry.boundary_ts_utc_ms).is_err()
    {
        return Err(Stage8bP1d3Error::InvalidChronology);
    }
    Ok(())
}

fn validate_initial_expiry(
    input: &Stage8bP1d3InitialLimitInput,
    expiry: &Stage8bP1d3DayExpiryAuthority,
) -> Result<(), Stage8bP1d3Error> {
    if !is_sha256(&expiry.schedule_fingerprint_sha256)
        || expiry.last_eligible_m10_redis_id != input.decision_m10_redis_id
        || expiry.trading_day_identity != trading_day(input.decision_m10_close_ts_utc_ms)?
        || expiry.boundary_ts_utc_ms <= input.decision_m10_close_ts_utc_ms
        || exact_timestamp(expiry.boundary_ts_utc_ms).is_err()
    {
        return Err(Stage8bP1d3Error::InvalidChronology);
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn outcome_evidence(
    pre_book: &Stage8bP1d3WorkingBookProjectionV1,
    outcome_kind: Stage8bP1d3OutcomeKind,
    request_id: Option<StrategyRequestId>,
    durable_request_client_id: Option<ClientOrderId>,
    canonical_command_sha256: Option<String>,
    accepted_command_payload_sha256: Option<String>,
    record: &Stage8bP1d3OrderRecordV1,
    candidate: Option<&Stage8bP1d3CanonicalM10Evidence>,
    expiry: Option<&Stage8bP1d3DayExpiryAuthority>,
    consumed_witness_kind: Stage8bP1d3ConsumedWitnessKind,
    schedule_fingerprint_sha256: &str,
    trading_day_identity: &str,
    last_eligible_m10_redis_id: &str,
    transition_ts_utc_ms: i64,
    reserved_seq_ack: Option<u64>,
    reserved_seq_truth: Option<u64>,
    sequence_allocation_frontier: u64,
    stage6_dispatch_record_id: Option<String>,
    stage6_outcome_record_id: String,
    stage6_predecessor_frontier_sha256: String,
    stage6_reserved_checkpoint_sha256: String,
    stage7_request_finalized_record_id: Option<String>,
    stage7_request_finalized_fingerprint_sha256: Option<String>,
    previous_outcome_evidence_sha256: String,
    fill_price: Option<Decimal>,
    pre_position_qty: Decimal,
    pre_position_avg_price: Option<Decimal>,
    expected_post_book_sha256: String,
) -> Result<Stage8bP1d3OutcomeEvidenceV1, Stage8bP1d3Error> {
    Ok(Stage8bP1d3OutcomeEvidenceV1 {
        schema_version: STAGE8B_P1D3_OUTCOME_EVIDENCE_SCHEMA_VERSION,
        domain: STAGE8B_P1D3_OUTCOME_EVIDENCE_DOMAIN.to_string(),
        outcome_kind,
        transition_ordinal: pre_book.transition_ordinal + 1,
        operational_identity_sha256: pre_book.operational_identity_sha256.clone(),
        package_generation: pre_book.package_generation,
        account_id: pre_book.account_id.clone(),
        instrument: pre_book.instrument.clone(),
        attribution_fingerprint_sha256: attribution_sha256(&pre_book.attribution)?,
        request_id,
        durable_request_client_id,
        canonical_command_sha256,
        accepted_command_payload_sha256,
        accepted_stage6_identity_sha256: record.accepted_stage6_identity_sha256.clone(),
        target_place_client_id: Some(record.original_durable_place_client_id.clone()),
        target_broker_order_id: Some(record.broker_order_id.clone()),
        source_m10_redis_id: Some(record.decision_m10_redis_id.clone()),
        source_m10_semantic_id_sha256: Some(record.decision_m10_semantic_id_sha256.clone()),
        source_m10_payload_sha256: Some(record.decision_m10_payload_sha256.clone()),
        source_m10_open_ts_utc_ms: Some(record.decision_m10_open_ts_utc_ms),
        source_m10_close_ts_utc_ms: Some(record.decision_m10_close_ts_utc_ms),
        candidate_m10_redis_id: candidate.map(|bar| bar.redis_id.clone()),
        candidate_m10_semantic_id_sha256: candidate.map(|bar| bar.semantic_id_sha256.clone()),
        candidate_m10_payload_sha256: candidate.map(|bar| bar.payload_sha256.clone()),
        candidate_m10_open_ts_utc_ms: candidate.map(|bar| bar.open_ts_utc_ms),
        candidate_m10_close_ts_utc_ms: candidate.map(|bar| bar.close_ts_utc_ms),
        consumed_witness_kind,
        schedule_fingerprint_sha256: schedule_fingerprint_sha256.to_string(),
        trading_day_identity: trading_day_identity.to_string(),
        last_eligible_m10_redis_id: last_eligible_m10_redis_id.to_string(),
        boundary_ts_utc_ms: expiry.map(|value| value.boundary_ts_utc_ms),
        pre_book_generation: pre_book.package_generation,
        pre_book_sha256: pre_book.canonical_sha256()?,
        expected_post_book_sha256,
        broker_order_id: record.broker_order_id.clone(),
        broker_trade_id: record.broker_trade_id.clone(),
        side: record.side,
        qty_decimal_bytes: record.qty_decimal_bytes,
        limit_price_decimal_bytes: record.limit_price_decimal_bytes,
        fill_price_decimal_bytes: fill_price.map(|value| value.serialize()),
        pre_position_qty_decimal_bytes: pre_position_qty.serialize(),
        pre_position_avg_price_decimal_bytes: pre_position_avg_price.map(|value| value.serialize()),
        transition_source_ts_utc_ms: transition_ts_utc_ms,
        transition_received_ts_utc_ms: transition_ts_utc_ms,
        reserved_seq_ack,
        reserved_seq_truth,
        sequence_allocation_frontier,
        stage6_dispatch_record_id,
        stage6_outcome_record_id,
        stage6_predecessor_frontier_sha256,
        stage6_reserved_checkpoint_sha256,
        stage7_request_finalized_record_id,
        stage7_request_finalized_fingerprint_sha256,
        previous_outcome_evidence_sha256,
    })
}

fn validate_bar(bar: &Stage8bP1d3CanonicalM10Evidence) -> Result<(), Stage8bP1d3Error> {
    if !is_sha256(&bar.semantic_id_sha256)
        || !is_sha256(&bar.payload_sha256)
        || !is_sha256(&bar.canonical_bytes_sha256)
        || !is_sha256(&bar.operational_identity_sha256)
        || bar.redis_id != format!("{}-0", bar.close_ts_utc_ms)
        || bar.open_ts_utc_ms <= 0
        || bar.close_ts_utc_ms.checked_sub(bar.open_ts_utc_ms) != Some(600_000)
        || bar.close_ts_utc_ms.rem_euclid(600_000) != 0
        || bar.open <= Decimal::ZERO
        || bar.high < bar.open.max(bar.close)
        || bar.low > bar.open.min(bar.close)
        || bar.low > bar.high
    {
        return Err(Stage8bP1d3Error::InvalidChronology);
    }
    Ok(())
}

fn validate_step(
    step: &Stage8bP1d3ScheduleStepAuthority,
    expected_predecessor: &str,
    candidate: &Stage8bP1d3CanonicalM10Evidence,
) -> Result<(), Stage8bP1d3Error> {
    if !is_sha256(&step.schedule_fingerprint_sha256)
        || step.predecessor_redis_id != expected_predecessor
        || step.candidate_redis_id != candidate.redis_id
        || step.last_eligible_m10_redis_id < step.candidate_redis_id
        || step.trading_day_identity != trading_day(candidate.close_ts_utc_ms)?
    {
        return Err(Stage8bP1d3Error::IdentityMismatch);
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)] // Exact broker snapshot construction keeps every canonical field visible.
fn exact_order_snapshot(
    account_id: &BrokerAccountId,
    broker_order_id: &BrokerOrderId,
    client_order_id: &ClientOrderId,
    instrument: &InstrumentId,
    side: OrderSide,
    qty: Decimal,
    limit: Decimal,
    status: OrderStatus,
    lifecycle: BrokerOrderLifecycle,
    filled: Decimal,
    remaining: Decimal,
    timestamp: DateTime<Utc>,
) -> BrokerOrderSnapshot {
    BrokerOrderSnapshot {
        account_id: account_id.clone(),
        broker_order_id: Some(broker_order_id.clone()),
        client_order_id: Some(client_order_id.clone()),
        instrument: instrument.clone(),
        side,
        order_type: OrderType::Limit,
        time_in_force: Some(TimeInForce::Day),
        status,
        lifecycle,
        qty,
        filled_qty: filled,
        remaining_qty: Some(remaining),
        limit_price: Some(limit),
        broker_asset_id: None,
        board: None,
        expiration_date: None,
        source_ts: Some(timestamp),
        received_ts: timestamp,
    }
}

fn exact_truth(
    input: &Stage8bP1d3InitialLimitInput,
    order: &BrokerOrderSnapshot,
    trade_id: Option<&BrokerTradeId>,
    fill_price: Option<Decimal>,
    timestamp: DateTime<Utc>,
) -> Result<BrokerTruthSnapshot, Stage8bP1d3Error> {
    let mut trades = Vec::new();
    let mut positions = Vec::new();
    if let (Some(trade_id), Some(fill_price)) = (trade_id, fill_price) {
        trades.push(BrokerTradeSnapshot {
            account_id: input.account_id.clone(),
            broker_trade_id: trade_id.clone(),
            broker_order_id: order.broker_order_id.clone(),
            client_order_id: Some(input.durable_client_order_id.clone()),
            instrument: input.instrument.clone(),
            side: input.side,
            qty: input.qty,
            price: fill_price,
            gross_amount: None,
            commission: Some(Decimal::ZERO),
            broker_asset_id: None,
            board: None,
            expiration_date: None,
            source_ts: timestamp,
            received_ts: timestamp,
        });
        let signed_fill = match input.side {
            OrderSide::Buy => input.qty,
            OrderSide::Sell => Decimal::ZERO
                .checked_sub(input.qty)
                .ok_or(Stage8bP1d3Error::InvalidDecimal)?,
        };
        let (qty, avg_price) = resulting_position(
            input.pre_position_qty,
            input.pre_position_avg_price,
            signed_fill,
            fill_price,
        )?;
        positions.push(BrokerPositionSnapshot {
            account_id: input.account_id.clone(),
            instrument: input.instrument.clone(),
            qty,
            avg_price,
            unrealized_pnl: None,
            source_ts: Some(timestamp),
            received_ts: timestamp,
        });
    }
    Ok(BrokerTruthSnapshot {
        account_id: input.account_id.clone(),
        orders: vec![order.clone()],
        positions,
        cash: None,
        trades,
        instruments: Vec::new(),
        received_ts: timestamp,
    })
}

fn exact_truth_from_record(
    record: &Stage8bP1d3OrderRecordV1,
    order: &BrokerOrderSnapshot,
    trade_id: Option<&BrokerTradeId>,
    fill_price: Option<Decimal>,
    pre_position_qty: Decimal,
    pre_position_avg_price: Option<Decimal>,
    timestamp: DateTime<Utc>,
) -> Result<BrokerTruthSnapshot, Stage8bP1d3Error> {
    let mut trades = Vec::new();
    let mut positions = Vec::new();
    if let (Some(trade_id), Some(fill_price)) = (trade_id, fill_price) {
        let qty = record.qty()?;
        trades.push(BrokerTradeSnapshot {
            account_id: record.account_id.clone(),
            broker_trade_id: trade_id.clone(),
            broker_order_id: Some(record.broker_order_id.clone()),
            client_order_id: Some(record.original_durable_place_client_id.clone()),
            instrument: record.instrument.clone(),
            side: record.side,
            qty,
            price: fill_price,
            gross_amount: None,
            commission: Some(Decimal::ZERO),
            broker_asset_id: None,
            board: None,
            expiration_date: None,
            source_ts: timestamp,
            received_ts: timestamp,
        });
        let signed_fill = match record.side {
            OrderSide::Buy => qty,
            OrderSide::Sell => Decimal::ZERO
                .checked_sub(qty)
                .ok_or(Stage8bP1d3Error::InvalidDecimal)?,
        };
        let (qty, avg_price) = resulting_position(
            pre_position_qty,
            pre_position_avg_price,
            signed_fill,
            fill_price,
        )?;
        positions.push(BrokerPositionSnapshot {
            account_id: record.account_id.clone(),
            instrument: record.instrument.clone(),
            qty,
            avg_price,
            unrealized_pnl: None,
            source_ts: Some(timestamp),
            received_ts: timestamp,
        });
    }
    Ok(BrokerTruthSnapshot {
        account_id: record.account_id.clone(),
        orders: vec![order.clone()],
        positions,
        cash: None,
        trades,
        instruments: Vec::new(),
        received_ts: timestamp,
    })
}

fn resulting_position(
    q0: Decimal,
    avg0: Option<Decimal>,
    signed_fill: Decimal,
    fill_price: Decimal,
) -> Result<(Decimal, Option<Decimal>), Stage8bP1d3Error> {
    if !is_integral(q0) || !is_integral(signed_fill) {
        return Err(Stage8bP1d3Error::InvalidDecimal);
    }
    crate::stage8b_p1d2_market_feedback::resulting_position(q0, avg0, signed_fill, fill_price)
        .map_err(|_| Stage8bP1d3Error::InvalidDecimal)
}

fn post_book_state_sha256(
    book: &Stage8bP1d3WorkingBookProjectionV1,
) -> Result<String, Stage8bP1d3Error> {
    #[derive(Serialize)]
    struct PostBookState<'a> {
        domain: &'static str,
        operational_identity_sha256: &'a str,
        package_generation: u64,
        account_id: &'a BrokerAccountId,
        instrument: &'a InstrumentId,
        attribution: &'a HybridRuntimeAttribution,
        records: &'a [Stage8bP1d3OrderRecordV1],
        active_broker_order_id: &'a Option<BrokerOrderId>,
        next_transition_ordinal: u64,
        total_sequence_frontier: u64,
    }
    // The seal itself cannot be part of the state that the evidence predicts.
    // Derive the prospective ordinal/frontier from both the enclosing book and
    // its rows so this digest is identical immediately before and after seal.
    let next_transition_ordinal = book
        .records
        .iter()
        .map(|record| record.transition_ordinal)
        .max()
        .unwrap_or(book.transition_ordinal)
        .max(book.transition_ordinal);
    let total_sequence_frontier = book
        .records
        .iter()
        .map(|record| record.total_sequence_frontier)
        .max()
        .unwrap_or(book.total_sequence_frontier)
        .max(book.total_sequence_frontier);
    sha256_json(&PostBookState {
        domain: "moex.stage8b.p1d3.post-book-state.v1",
        operational_identity_sha256: &book.operational_identity_sha256,
        package_generation: book.package_generation,
        account_id: &book.account_id,
        instrument: &book.instrument,
        attribution: &book.attribution,
        records: &book.records,
        active_broker_order_id: &book.active_broker_order_id,
        next_transition_ordinal,
        total_sequence_frontier,
    })
}

fn genesis_sha256(
    accepted_p1d2_package_commitment_sha256: &str,
    operational_identity_sha256: &str,
    package_generation: u64,
) -> Result<String, Stage8bP1d3Error> {
    let mut hasher = Sha256::new();
    hasher.update(STAGE8B_P1D3_BOOK_GENESIS_DOMAIN.as_bytes());
    hasher.update([0]);
    hasher.update(raw_sha256(accepted_p1d2_package_commitment_sha256)?);
    hasher.update(raw_sha256(operational_identity_sha256)?);
    hasher.update(package_generation.to_be_bytes());
    Ok(hex_digest(hasher.finalize()))
}

fn transition_sha256(
    ordinal: u64,
    previous: &str,
    evidence_bytes: &[u8],
    post_book_bytes: &[u8],
) -> Result<String, Stage8bP1d3Error> {
    let mut hasher = Sha256::new();
    hasher.update(STAGE8B_P1D3_BOOK_TRANSITION_DOMAIN.as_bytes());
    hasher.update([0]);
    hasher.update(ordinal.to_be_bytes());
    hasher.update(raw_sha256(previous)?);
    hasher.update((evidence_bytes.len() as u64).to_be_bytes());
    hasher.update(evidence_bytes);
    hasher.update((post_book_bytes.len() as u64).to_be_bytes());
    hasher.update(post_book_bytes);
    Ok(hex_digest(hasher.finalize()))
}

fn derive_order_id(
    operational_identity_sha256: &str,
    request_id: StrategyRequestId,
    canonical_command_sha256: &str,
) -> BrokerOrderId {
    BrokerOrderId::new(format!(
        "P1D-O-{}",
        nul_separated_sha256(&[
            crate::STAGE8B_P1D1_ORDER_ID_DOMAIN,
            operational_identity_sha256,
            &request_id.to_string(),
            canonical_command_sha256,
        ])
    ))
}

fn derive_trade_id(order_id: &BrokerOrderId, bar_semantic_sha256: &str) -> BrokerTradeId {
    BrokerTradeId::new(format!(
        "P1D-T-{}",
        nul_separated_sha256(&[
            STAGE8B_P1D3_TRADE_ID_DOMAIN,
            order_id.as_str(),
            bar_semantic_sha256,
            "1",
        ])
    ))
}

fn order_fingerprint_sha256(
    order_id: &BrokerOrderId,
    request_id: StrategyRequestId,
    client_order_id: &ClientOrderId,
    command_sha256: &str,
) -> String {
    nul_separated_sha256(&[
        STAGE8B_P1D3_ORDER_FINGERPRINT_DOMAIN,
        order_id.as_str(),
        &request_id.to_string(),
        client_order_id.as_str(),
        command_sha256,
    ])
}

fn attribution_sha256(value: &HybridRuntimeAttribution) -> Result<String, Stage8bP1d3Error> {
    sha256_json(value)
}

fn cancel_attribution(
    source: &HybridRuntimeAttribution,
) -> Result<HybridRuntimeAttribution, Stage8bP1d3Error> {
    let (prefix, _) = source
        .internal_comment()
        .rsplit_once("|r=")
        .ok_or(Stage8bP1d3Error::IdentityMismatch)?;
    HybridRuntimeAttribution::parse_source_comment(format!("{prefix}|r=CANCEL"))
        .map_err(|_| Stage8bP1d3Error::IdentityMismatch)
}

fn allocate_pair(frontier: u64) -> Result<(u64, u64), Stage8bP1d3Error> {
    let ack = frontier
        .checked_add(1)
        .ok_or(Stage8bP1d3Error::InvalidSequence)?;
    let truth = ack
        .checked_add(1)
        .ok_or(Stage8bP1d3Error::InvalidSequence)?;
    Ok((ack, truth))
}

fn trading_day(timestamp_ms: i64) -> Result<String, Stage8bP1d3Error> {
    Ok(exact_timestamp(timestamp_ms)?.date_naive().to_string())
}

fn exact_timestamp(value: i64) -> Result<DateTime<Utc>, Stage8bP1d3Error> {
    Utc.timestamp_millis_opt(value)
        .single()
        .ok_or(Stage8bP1d3Error::InvalidChronology)
}

fn is_integral(value: Decimal) -> bool {
    value.fract() == Decimal::ZERO
}

fn decimal(bytes: [u8; 16]) -> Result<Decimal, Stage8bP1d3Error> {
    let value = Decimal::deserialize(bytes);
    value
        .serialize()
        .eq(&bytes)
        .then_some(value)
        .ok_or(Stage8bP1d3Error::InvalidDecimal)
}

fn raw_sha256(value: &str) -> Result<[u8; 32], Stage8bP1d3Error> {
    if !is_sha256(value) {
        return Err(Stage8bP1d3Error::IdentityMismatch);
    }
    let mut output = [0_u8; 32];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        let text = std::str::from_utf8(pair).map_err(|_| Stage8bP1d3Error::IdentityMismatch)?;
        output[index] =
            u8::from_str_radix(text, 16).map_err(|_| Stage8bP1d3Error::IdentityMismatch)?;
    }
    Ok(output)
}

fn sha256_json(value: &impl Serialize) -> Result<String, Stage8bP1d3Error> {
    let bytes = serde_json::to_vec(value).map_err(|_| Stage8bP1d3Error::InvalidEncoding)?;
    Ok(sha256_hex(&bytes))
}

fn sha256_hex(bytes: &[u8]) -> String {
    hex_digest(Sha256::digest(bytes))
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

#[cfg(any(test, feature = "stage5g-artifact-fixtures"))]
#[allow(
    dead_code,
    reason = "exported only for deterministic artifact fixtures"
)]
pub fn stage8b_p1d3_test_step_authority(
    schedule_fingerprint_sha256: String,
    trading_day_identity: String,
    last_eligible_m10_redis_id: String,
    predecessor_redis_id: String,
    candidate_redis_id: String,
) -> Stage8bP1d3ScheduleStepAuthority {
    Stage8bP1d3ScheduleStepAuthority {
        schedule_fingerprint_sha256,
        trading_day_identity,
        last_eligible_m10_redis_id,
        predecessor_redis_id,
        candidate_redis_id,
    }
}

#[cfg(any(test, feature = "stage5g-artifact-fixtures"))]
#[allow(
    dead_code,
    reason = "exported only for deterministic artifact fixtures"
)]
pub fn stage8b_p1d3_test_expiry_authority(
    schedule_fingerprint_sha256: String,
    trading_day_identity: String,
    last_eligible_m10_redis_id: String,
    boundary_ts_utc_ms: i64,
) -> Stage8bP1d3DayExpiryAuthority {
    Stage8bP1d3DayExpiryAuthority {
        schedule_fingerprint_sha256,
        trading_day_identity,
        last_eligible_m10_redis_id,
        boundary_ts_utc_ms,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use broker_core::{Exchange, Market, PlaceOrder};
    use uuid::Uuid;

    fn d(value: i64, scale: u32) -> Decimal {
        Decimal::new(value, scale)
    }

    fn request(value: u128) -> StrategyRequestId {
        StrategyRequestId::from(Uuid::from_u128((value << 96) | value))
    }

    fn account() -> BrokerAccountId {
        BrokerAccountId::new("ACC_TEST_0001")
    }

    fn instrument() -> InstrumentId {
        InstrumentId {
            symbol: "IMOEXF".to_string(),
            venue_symbol: Some("IMOEXF@RTSX".to_string()),
            exchange: Exchange::Moex,
            market: Market::Futures,
        }
    }

    fn attribution() -> HybridRuntimeAttribution {
        HybridRuntimeAttribution::parse_source_comment(
            "HYB|sid=hybrid_imoexf|c=cycle0001|o=BO|r=ENTRY",
        )
        .unwrap()
    }

    fn cancel_request_attribution() -> HybridRuntimeAttribution {
        HybridRuntimeAttribution::parse_source_comment(
            "HYB|sid=hybrid_imoexf|c=cycle0001|o=BO|r=CANCEL",
        )
        .unwrap()
    }

    fn book() -> Stage8bP1d3WorkingBookProjectionV1 {
        Stage8bP1d3WorkingBookProjectionV1::migrate_from_p1d2(
            &"1".repeat(64),
            "2".repeat(64),
            7,
            account(),
            instrument(),
            attribution(),
            20,
        )
        .unwrap()
    }

    fn bar(open: Decimal, high: Decimal, low: Decimal) -> Stage8bP1d3CanonicalM10Evidence {
        Stage8bP1d3CanonicalM10Evidence {
            redis_id: "1785000000000-0".to_string(),
            semantic_id_sha256: "3".repeat(64),
            payload_sha256: "4".repeat(64),
            canonical_bytes_sha256: "5".repeat(64),
            operational_identity_sha256: "2".repeat(64),
            instrument: instrument(),
            open_ts_utc_ms: 1_784_999_400_000,
            close_ts_utc_ms: 1_785_000_000_000,
            open,
            high,
            low,
            close: open,
        }
    }

    fn input(side: OrderSide, limit: Decimal) -> Stage8bP1d3InitialLimitInput {
        let request_id = request(1);
        Stage8bP1d3InitialLimitInput {
            operational_identity_sha256: "2".repeat(64),
            package_generation: 7,
            account_id: account(),
            instrument: instrument(),
            attribution: attribution(),
            request_id,
            durable_client_order_id: ClientOrderId::from_strategy_request(request_id),
            canonical_command_sha256: "6".repeat(64),
            accepted_command_payload_sha256: "7".repeat(64),
            accepted_stage6_identity_sha256: "8".repeat(64),
            decision_m10_redis_id: "1784999400000-0".to_string(),
            decision_m10_semantic_id_sha256: "9".repeat(64),
            decision_m10_payload_sha256: "a".repeat(64),
            decision_m10_open_ts_utc_ms: 1_784_998_800_000,
            decision_m10_close_ts_utc_ms: 1_784_999_400_000,
            side,
            qty: Decimal::ONE,
            limit_price: limit,
            pre_position_qty: Decimal::ZERO,
            pre_position_avg_price: None,
            sequence_allocation_frontier: 20,
            stage6_dispatch_record_id: "b".repeat(64),
            stage6_predecessor_frontier_sha256: "d".repeat(64),
            stage6_reserved_checkpoint_sha256: "e".repeat(64),
            previous_outcome_evidence_sha256: "f".repeat(64),
        }
    }

    fn step() -> Stage8bP1d3ScheduleStepAuthority {
        stage8b_p1d3_test_step_authority(
            "1".repeat(64),
            "2026-07-25".to_string(),
            "1785000000000-0".to_string(),
            "1784999400000-0".to_string(),
            "1785000000000-0".to_string(),
        )
    }

    fn later_bar(open: Decimal, high: Decimal, low: Decimal) -> Stage8bP1d3CanonicalM10Evidence {
        Stage8bP1d3CanonicalM10Evidence {
            redis_id: "1785000600000-0".to_string(),
            semantic_id_sha256: "13".repeat(32),
            payload_sha256: "14".repeat(32),
            canonical_bytes_sha256: "15".repeat(32),
            operational_identity_sha256: "2".repeat(64),
            instrument: instrument(),
            open_ts_utc_ms: 1_785_000_000_000,
            close_ts_utc_ms: 1_785_000_600_000,
            open,
            high,
            low,
            close: open,
        }
    }

    fn later_step() -> Stage8bP1d3ScheduleStepAuthority {
        stage8b_p1d3_test_step_authority(
            "1".repeat(64),
            "2026-07-25".to_string(),
            "1785000600000-0".to_string(),
            "1785000000000-0".to_string(),
            "1785000600000-0".to_string(),
        )
    }

    fn autonomous_input(book: &Stage8bP1d3WorkingBookProjectionV1) -> Stage8bP1d3AutonomousInput {
        Stage8bP1d3AutonomousInput {
            operational_identity_sha256: "2".repeat(64),
            package_generation: 7,
            account_id: account(),
            instrument: instrument(),
            attribution: attribution(),
            pre_position_qty: Decimal::ZERO,
            pre_position_avg_price: None,
            sequence_allocation_frontier: book.total_sequence_frontier,
            stage6_predecessor_frontier_sha256: "e".repeat(64),
            stage6_reserved_checkpoint_sha256: "33".repeat(32),
            previous_outcome_evidence_sha256: book.latest_outcome_evidence_sha256.clone(),
        }
    }

    fn cancel_input(
        book: &Stage8bP1d3WorkingBookProjectionV1,
        target: BrokerOrderId,
    ) -> Stage8bP1d3CancelInput {
        let request_id = request(2);
        Stage8bP1d3CancelInput {
            operational_identity_sha256: "2".repeat(64),
            package_generation: 7,
            account_id: account(),
            instrument: instrument(),
            attribution: cancel_request_attribution(),
            request_id,
            durable_request_client_id: ClientOrderId::from_strategy_request(request_id),
            canonical_command_sha256: "21".repeat(32),
            accepted_command_payload_sha256: "22".repeat(32),
            accepted_stage6_identity_sha256: "2a".repeat(32),
            target_broker_order_id: target,
            target_place_client_id: Some(ClientOrderId::from_strategy_request(request(1))),
            decision_m10_redis_id: "1785000000000-0".to_string(),
            decision_m10_semantic_id_sha256: "23".repeat(32),
            decision_m10_payload_sha256: "24".repeat(32),
            decision_m10_open_ts_utc_ms: 1_784_999_400_000,
            decision_m10_close_ts_utc_ms: 1_785_000_000_000,
            pre_position_qty: Decimal::ZERO,
            pre_position_avg_price: None,
            sequence_allocation_frontier: book.total_sequence_frontier,
            stage6_dispatch_record_id: "25".repeat(32),
            stage6_predecessor_frontier_sha256: "e".repeat(64),
            target_stage6_reserved_checkpoint_sha256: None,
            stage6_reserved_checkpoint_sha256: "28".repeat(32),
            previous_outcome_evidence_sha256: book.latest_outcome_evidence_sha256.clone(),
        }
    }

    fn recover(
        pre_book: &Stage8bP1d3WorkingBookProjectionV1,
        fresh: &Stage8bP1d3TransitionPlan,
    ) -> Stage8bP1d3TransitionPlan {
        let binding = Stage8bP1d3Stage6RecoveryBinding {
            outcome_record_id: fresh.evidence.stage6_outcome_record_id.clone(),
            predecessor_frontier_sha256: fresh.evidence.stage6_predecessor_frontier_sha256.clone(),
            reserved_checkpoint_sha256: fresh.evidence.stage6_reserved_checkpoint_sha256.clone(),
            request_finalized_record_id: fresh.evidence.stage7_request_finalized_record_id.clone(),
            request_finalized_fingerprint_sha256: fresh
                .evidence
                .stage7_request_finalized_fingerprint_sha256
                .clone(),
            authenticated_post_checkpoint_sha256: "ab".repeat(32),
        };
        let record = crate::Stage6JournalRecordV3::from_p1d3_outcome_evidence(
            crate::Stage6LifecycleSequence::new(1).unwrap(),
            crate::Stage6JournalRecordId::parse_exact("ab".repeat(32)).unwrap(),
            fresh.evidence_bytes.clone(),
        )
        .unwrap();
        let encoded = record.encode_canonical();
        let reread = crate::Stage6JournalRecordV3::decode_canonical(&encoded).unwrap();
        assert_eq!(reread.outcome_evidence_bytes(), fresh.evidence_bytes);
        let authenticated = reread.authenticate_p1d3_outcome(binding).unwrap();
        recover_stage8b_p1d3_outcome_transition(pre_book, authenticated).unwrap()
    }

    fn initial_filled() -> Stage8bP1d3TransitionPlan {
        build_initial_limit_transition(
            &book(),
            input(OrderSide::Buy, Decimal::new(2_210, 1)),
            Some(bar(
                Decimal::new(2_200, 1),
                Decimal::new(2_220, 1),
                Decimal::new(2_190, 1),
            )),
            Some(step()),
            None,
        )
        .unwrap()
    }

    fn all_eight_shape_plans() -> Vec<(
        &'static str,
        Stage8bP1d3WorkingBookProjectionV1,
        Stage8bP1d3TransitionPlan,
    )> {
        let genesis = book();
        let initial_working = initial_working();
        let initial_filled = initial_filled();
        let initial_expired = build_initial_limit_transition(
            &genesis,
            input(OrderSide::Buy, Decimal::new(2_210, 1)),
            None,
            None,
            Some(stage8b_p1d3_test_expiry_authority(
                "1".repeat(64),
                "2026-07-25".to_string(),
                "1784999400000-0".to_string(),
                1_785_000_600_000,
            )),
        )
        .unwrap();
        let later_filled = build_later_limit_transition(
            &initial_working.post_book,
            autonomous_input(&initial_working.post_book),
            Some(later_bar(
                Decimal::new(2_200, 1),
                Decimal::new(2_220, 1),
                Decimal::new(2_190, 1),
            )),
            Some(later_step()),
            None,
        )
        .unwrap();
        let Stage8bP1d3LaterEvaluationPlan::Outcome(later_filled) = later_filled else {
            panic!("golden later fill must be terminal")
        };
        let untouched = build_later_limit_transition(
            &initial_working.post_book,
            autonomous_input(&initial_working.post_book),
            Some(later_bar(
                Decimal::new(2_230, 1),
                Decimal::new(2_240, 1),
                Decimal::new(2_220, 1),
            )),
            Some(later_step()),
            None,
        )
        .unwrap();
        let Stage8bP1d3LaterEvaluationPlan::Untouched {
            post_book: evaluated,
            ..
        } = untouched
        else {
            panic!("golden later expiry needs an untouched evaluation")
        };
        let later_expired = build_later_limit_transition(
            &evaluated,
            autonomous_input(&evaluated),
            None,
            None,
            Some(stage8b_p1d3_test_expiry_authority(
                "1".repeat(64),
                "2026-07-25".to_string(),
                "1785000600000-0".to_string(),
                1_785_001_200_000,
            )),
        )
        .unwrap();
        let Stage8bP1d3LaterEvaluationPlan::Outcome(later_expired) = later_expired else {
            panic!("golden later expiry must be terminal")
        };

        let working_target = initial_working
            .post_book
            .active_broker_order_id
            .clone()
            .unwrap();
        let cancel_canceled = build_cancel_transition(
            &initial_working.post_book,
            cancel_input(&initial_working.post_book, working_target),
            later_bar(
                Decimal::new(2_230, 1),
                Decimal::new(2_240, 1),
                Decimal::new(2_220, 1),
            ),
            later_step(),
        )
        .unwrap();
        let Stage8bP1d3CancelTransitionPlan::Ready(cancel_canceled) = cancel_canceled else {
            panic!("golden active target must cancel")
        };

        let filled_target = initial_filled.post_book.records[0].broker_order_id.clone();
        let cancel_execution = build_cancel_transition(
            &initial_filled.post_book,
            cancel_input(&initial_filled.post_book, filled_target),
            later_bar(
                Decimal::new(2_230, 1),
                Decimal::new(2_240, 1),
                Decimal::new(2_220, 1),
            ),
            later_step(),
        )
        .unwrap();
        let Stage8bP1d3CancelTransitionPlan::Ready(cancel_execution) = cancel_execution else {
            panic!("golden filled target must recover cancel")
        };

        let expired_target = initial_expired.post_book.records[0].broker_order_id.clone();
        let cancel_nonexecution = build_cancel_transition(
            &initial_expired.post_book,
            cancel_input(&initial_expired.post_book, expired_target),
            later_bar(
                Decimal::new(2_230, 1),
                Decimal::new(2_240, 1),
                Decimal::new(2_220, 1),
            ),
            stage8b_p1d3_test_step_authority(
                "1".repeat(64),
                "2026-07-25".to_string(),
                "1785000600000-0".to_string(),
                "1784999400000-0".to_string(),
                "1785000600000-0".to_string(),
            ),
        )
        .unwrap();
        let Stage8bP1d3CancelTransitionPlan::Ready(cancel_nonexecution) = cancel_nonexecution
        else {
            panic!("golden expired target must recover cancel")
        };

        vec![
            ("initial_working", genesis.clone(), initial_working.clone()),
            ("initial_filled", genesis.clone(), initial_filled.clone()),
            ("initial_expired", genesis, initial_expired.clone()),
            (
                "later_filled",
                initial_working.post_book.clone(),
                *later_filled,
            ),
            ("later_expired", evaluated, *later_expired),
            (
                "cancel_canceled",
                initial_working.post_book,
                *cancel_canceled,
            ),
            (
                "cancel_execution_observed",
                initial_filled.post_book,
                *cancel_execution,
            ),
            (
                "cancel_already_terminal_non_execution",
                initial_expired.post_book,
                *cancel_nonexecution,
            ),
        ]
    }

    #[test]
    fn buy_limit_touch_and_improvement_are_exact() {
        let touch = bar(
            Decimal::new(2_230, 1),
            Decimal::new(2_240, 1),
            Decimal::new(2_210, 1),
        );
        assert_eq!(
            decide_limit(OrderSide::Buy, Decimal::new(2_210, 1), &touch).unwrap(),
            (
                Stage8bP1d3LimitDecision::Filled,
                Some(Decimal::new(2_210, 1))
            )
        );
        let improvement = bar(
            Decimal::new(2_200, 1),
            Decimal::new(2_220, 1),
            Decimal::new(2_190, 1),
        );
        assert_eq!(
            decide_limit(OrderSide::Buy, Decimal::new(2_210, 1), &improvement).unwrap(),
            (
                Stage8bP1d3LimitDecision::Filled,
                Some(Decimal::new(2_200, 1))
            )
        );
    }

    #[test]
    fn sell_limit_touch_and_working_are_exact() {
        let touch = bar(
            Decimal::new(2_200, 1),
            Decimal::new(2_230, 1),
            Decimal::new(2_190, 1),
        );
        assert_eq!(
            decide_limit(OrderSide::Sell, Decimal::new(2_230, 1), &touch).unwrap(),
            (
                Stage8bP1d3LimitDecision::Filled,
                Some(Decimal::new(2_230, 1))
            )
        );
        let working = bar(
            Decimal::new(2_200, 1),
            Decimal::new(2_220, 1),
            Decimal::new(2_190, 1),
        );
        assert_eq!(
            decide_limit(OrderSide::Sell, Decimal::new(2_230, 1), &working).unwrap(),
            (Stage8bP1d3LimitDecision::Working, None)
        );
    }

    #[test]
    fn position_projection_reuses_the_complete_p1d2_arithmetic_matrix() {
        let cases = [
            (
                Decimal::ZERO,
                None,
                d(1, 0),
                d(101, 0),
                d(1, 0),
                "101.00000000",
            ),
            (
                Decimal::ZERO,
                None,
                d(-1, 0),
                d(101, 0),
                d(-1, 0),
                "101.00000000",
            ),
            (
                d(2, 0),
                Some(d(100, 0)),
                d(1, 0),
                d(106, 0),
                d(3, 0),
                "102.00000000",
            ),
            (
                d(-2, 0),
                Some(d(100, 0)),
                d(-1, 0),
                d(106, 0),
                d(-3, 0),
                "102.00000000",
            ),
            (
                d(2, 0),
                Some(d(100, 0)),
                d(-1, 0),
                d(110, 0),
                d(1, 0),
                "100.00000000",
            ),
            (
                d(-2, 0),
                Some(d(100, 0)),
                d(1, 0),
                d(90, 0),
                d(-1, 0),
                "100.00000000",
            ),
            (
                d(1, 0),
                Some(d(100, 0)),
                d(-2, 0),
                d(110, 0),
                d(-1, 0),
                "110.00000000",
            ),
            (
                d(-1, 0),
                Some(d(100, 0)),
                d(2, 0),
                d(90, 0),
                d(1, 0),
                "90.00000000",
            ),
        ];
        for (q0, avg0, delta, price, expected_qty, expected_avg) in cases {
            let (actual_qty, actual_avg) = resulting_position(q0, avg0, delta, price).unwrap();
            let actual_avg = actual_avg.unwrap();
            let expected_avg = Decimal::from_str_exact(expected_avg).unwrap();
            assert_eq!(actual_qty, expected_qty);
            assert_eq!(actual_avg, expected_avg);
            assert_eq!(actual_avg.scale(), crate::STAGE8B_P1D2_AVG_PRICE_SCALE);
            assert_eq!(actual_avg.serialize(), expected_avg.serialize());
        }

        for (q0, avg0, delta) in [
            (d(1, 0), Some(d(100, 0)), d(-1, 0)),
            (d(-1, 0), Some(d(100, 0)), d(1, 0)),
        ] {
            assert_eq!(
                resulting_position(q0, avg0, delta, d(105, 0)).unwrap(),
                (Decimal::ZERO, None)
            );
        }
        assert_eq!(
            resulting_position(Decimal::ONE, None, Decimal::ONE, Decimal::ONE),
            Err(Stage8bP1d3Error::InvalidDecimal)
        );
        assert_eq!(
            resulting_position(
                Decimal::ZERO,
                Some(Decimal::ONE),
                Decimal::ONE,
                Decimal::ONE,
            ),
            Err(Stage8bP1d3Error::InvalidDecimal)
        );
        assert_eq!(
            resulting_position(Decimal::MAX, Some(Decimal::ONE), Decimal::ONE, Decimal::ONE),
            Err(Stage8bP1d3Error::InvalidDecimal)
        );
        assert_eq!(
            resulting_position(
                Decimal::MIN,
                Some(Decimal::ONE),
                -Decimal::ONE,
                Decimal::ONE,
            ),
            Err(Stage8bP1d3Error::InvalidDecimal)
        );
    }

    #[test]
    fn distinct_request_ids_with_the_same_truncated_client_id_fail_closed_for_cancel() {
        let place_request = StrategyRequestId::from(
            Uuid::parse_str("00000000-0000-0000-0000-00000000d301").unwrap(),
        );
        let cancel_request = StrategyRequestId::from(
            Uuid::parse_str("00000000-0000-0000-0000-00000000d302").unwrap(),
        );
        assert_ne!(place_request, cancel_request);
        let colliding_id = ClientOrderId::from_strategy_request(place_request);
        assert_eq!(
            colliding_id,
            ClientOrderId::from_strategy_request(cancel_request)
        );
        assert_eq!(colliding_id.as_str(), "00000000000000000000");

        let mut place = input(OrderSide::Buy, Decimal::new(2_210, 1));
        place.request_id = place_request;
        place.durable_client_order_id = colliding_id.clone();
        let working = build_initial_limit_transition(
            &book(),
            place,
            Some(bar(
                Decimal::new(2_230, 1),
                Decimal::new(2_240, 1),
                Decimal::new(2_220, 1),
            )),
            Some(step()),
            None,
        )
        .unwrap();
        let target = working.post_book.active_broker_order_id.clone().unwrap();
        let mut cancel = cancel_input(&working.post_book, target);
        cancel.request_id = cancel_request;
        cancel.durable_request_client_id = colliding_id.clone();
        cancel.target_place_client_id = Some(colliding_id);
        assert!(matches!(
            build_cancel_transition(
                &working.post_book,
                cancel,
                later_bar(
                    Decimal::new(2_230, 1),
                    Decimal::new(2_240, 1),
                    Decimal::new(2_220, 1),
                ),
                later_step(),
            ),
            Err(Stage8bP1d3Error::IdentityMismatch)
        ));

        let target = initial_filled();
        let broker_order_id = target.post_book.records[0].broker_order_id.clone();
        let Stage8bP1d3CancelTransitionPlan::Ready(valid) = build_cancel_transition(
            &target.post_book,
            cancel_input(&target.post_book, broker_order_id),
            later_bar(
                Decimal::new(2_230, 1),
                Decimal::new(2_240, 1),
                Decimal::new(2_220, 1),
            ),
            later_step(),
        )
        .unwrap() else {
            panic!("terminal target must produce recovered cancel")
        };
        let mut forged = valid.evidence.clone();
        forged.durable_request_client_id = forged.target_place_client_id.clone();
        assert_eq!(forged.validate(), Err(Stage8bP1d3Error::IdentityMismatch));
        assert_eq!(
            forged.encode_canonical(),
            Err(Stage8bP1d3Error::IdentityMismatch)
        );
    }

    #[test]
    fn initial_working_projection_is_exact_and_roundtrips() {
        let plan = build_initial_limit_transition(
            &book(),
            input(OrderSide::Buy, Decimal::new(2_210, 1)),
            Some(bar(
                Decimal::new(2_230, 1),
                Decimal::new(2_240, 1),
                Decimal::new(2_220, 1),
            )),
            Some(step()),
            None,
        )
        .unwrap();
        assert_eq!(
            plan.evidence.outcome_kind(),
            Stage8bP1d3OutcomeKind::InitialWorking
        );
        assert_eq!(plan.evidence.reserved_sequences(), (Some(21), Some(22)));
        assert_eq!(
            plan.truth.as_ref().unwrap().orders[0].status,
            OrderStatus::Working
        );
        assert!(plan.truth.as_ref().unwrap().trades.is_empty());
        assert!(plan.truth.as_ref().unwrap().positions.is_empty());
        let bytes = plan.post_book.encode_canonical().unwrap();
        assert_eq!(
            Stage8bP1d3WorkingBookProjectionV1::decode_canonical(&bytes).unwrap(),
            plan.post_book
        );
        assert_eq!(
            Stage8bP1d3OutcomeEvidenceV1::decode_canonical(&plan.evidence_bytes).unwrap(),
            plan.evidence
        );
    }

    #[test]
    fn initial_expiry_uses_boundary_clock_and_no_trade_or_position() {
        let expiry = stage8b_p1d3_test_expiry_authority(
            "1".repeat(64),
            "2026-07-25".to_string(),
            "1784999400000-0".to_string(),
            1_785_000_600_000,
        );
        let plan = build_initial_limit_transition(
            &book(),
            input(OrderSide::Buy, Decimal::new(2_210, 1)),
            None,
            None,
            Some(expiry),
        )
        .unwrap();
        let truth = plan.truth.unwrap();
        assert_eq!(
            plan.evidence.outcome_kind(),
            Stage8bP1d3OutcomeKind::InitialExpired
        );
        assert_eq!(truth.received_ts.timestamp_millis(), 1_785_000_600_000);
        assert_eq!(truth.orders[0].status, OrderStatus::Expired);
        assert!(truth.trades.is_empty());
        assert!(truth.positions.is_empty());
    }

    #[test]
    fn later_untouched_then_expiry_allocates_only_terminal_truth_sequence() {
        let initial = initial_working();
        let untouched = build_later_limit_transition(
            &initial.post_book,
            autonomous_input(&initial.post_book),
            Some(later_bar(
                Decimal::new(2_230, 1),
                Decimal::new(2_240, 1),
                Decimal::new(2_220, 1),
            )),
            Some(later_step()),
            None,
        )
        .unwrap();
        let Stage8bP1d3LaterEvaluationPlan::Untouched { post_book, .. } = untouched else {
            panic!("expected untouched S_eval")
        };
        assert_eq!(post_book.total_sequence_frontier, 22);
        let expiry = stage8b_p1d3_test_expiry_authority(
            "1".repeat(64),
            "2026-07-25".to_string(),
            "1785000600000-0".to_string(),
            1_785_001_200_000,
        );
        let expired = build_later_limit_transition(
            &post_book,
            autonomous_input(&post_book),
            None,
            None,
            Some(expiry),
        )
        .unwrap();
        let Stage8bP1d3LaterEvaluationPlan::Outcome(expired) = expired else {
            panic!("expected terminal expiry")
        };
        assert_eq!(expired.evidence.reserved_sequences(), (None, Some(23)));
        assert_eq!(
            expired.truth.unwrap().orders[0].status,
            OrderStatus::Expired
        );
    }

    #[test]
    fn later_fill_emits_one_trade_and_position() {
        let initial = initial_working();
        let filled = build_later_limit_transition(
            &initial.post_book,
            autonomous_input(&initial.post_book),
            Some(later_bar(
                Decimal::new(2_200, 1),
                Decimal::new(2_220, 1),
                Decimal::new(2_190, 1),
            )),
            Some(later_step()),
            None,
        )
        .unwrap();
        let Stage8bP1d3LaterEvaluationPlan::Outcome(filled) = filled else {
            panic!("expected fill")
        };
        let truth = filled.truth.unwrap();
        assert_eq!(filled.evidence.reserved_sequences(), (None, Some(23)));
        assert_eq!(truth.orders[0].status, OrderStatus::Filled);
        assert_eq!(truth.trades.len(), 1);
        assert_eq!(truth.positions.len(), 1);
        assert_eq!(truth.trades[0].price, Decimal::new(2_200, 1));
    }

    #[test]
    fn cancel_working_allocates_ack_truth_pair_and_never_trade() {
        let initial = initial_working();
        let target = initial.post_book.active_broker_order_id.clone().unwrap();
        let cancel = build_cancel_transition(
            &initial.post_book,
            cancel_input(&initial.post_book, target),
            later_bar(
                Decimal::new(2_230, 1),
                Decimal::new(2_240, 1),
                Decimal::new(2_220, 1),
            ),
            later_step(),
        )
        .unwrap();
        let Stage8bP1d3CancelTransitionPlan::Ready(cancel) = cancel else {
            panic!("working target must produce an immediate cancel outcome")
        };
        assert_eq!(cancel.evidence.reserved_sequences(), (Some(23), Some(24)));
        let truth = cancel.truth.unwrap();
        assert_eq!(truth.orders[0].status, OrderStatus::Canceled);
        assert!(truth.trades.is_empty());
        assert!(truth.positions.is_empty());
    }

    #[test]
    fn cancel_rejects_entry_attribution_before_any_transition() {
        let initial = initial_working();
        let target = initial.post_book.active_broker_order_id.clone().unwrap();
        let mut input = cancel_input(&initial.post_book, target);
        input.attribution = attribution();

        let result = build_cancel_transition(
            &initial.post_book,
            input,
            later_bar(
                Decimal::new(2_230, 1),
                Decimal::new(2_240, 1),
                Decimal::new(2_220, 1),
            ),
            later_step(),
        );

        assert!(matches!(result, Err(Stage8bP1d3Error::IdentityMismatch)));
    }

    #[test]
    fn fill_before_cancel_seals_fill_then_recovered_ack() {
        let initial = initial_working();
        let target = initial.post_book.active_broker_order_id.clone().unwrap();
        let mut input = cancel_input(&initial.post_book, target);
        input.target_stage6_reserved_checkpoint_sha256 = Some("27".repeat(32));
        let transition = build_cancel_transition(
            &initial.post_book,
            input,
            later_bar(
                Decimal::new(2_200, 1),
                Decimal::new(2_220, 1),
                Decimal::new(2_190, 1),
            ),
            later_step(),
        )
        .unwrap();
        let Stage8bP1d3CancelTransitionPlan::TargetThenCancel {
            target_transition: fill,
            continuation,
        } = transition
        else {
            panic!("filled candidate must defer cancel until the target append is reread")
        };
        let cancel = continuation
            .complete(&fill.post_book, "44".repeat(32))
            .unwrap();
        assert_eq!(fill.evidence.reserved_sequences(), (None, Some(23)));
        assert_eq!(
            fill.evidence.stage6_reserved_checkpoint_sha256,
            "27".repeat(32)
        );
        assert_eq!(
            cancel.evidence.stage6_predecessor_frontier_sha256,
            "44".repeat(32)
        );
        assert_eq!(cancel.evidence.reserved_sequences(), (Some(24), None));
        assert_eq!(cancel.ack.unwrap().status, CommandAckStatus::Recovered);
        assert!(cancel.truth.is_none());
        assert_eq!(cancel.post_book, fill.post_book);
    }

    #[test]
    fn all_eight_outcome_shapes_recover_byte_identically() {
        let genesis = book();

        let initial_working = initial_working();
        assert_eq!(recover(&genesis, &initial_working), initial_working);

        let initial_filled = initial_filled();
        assert_eq!(recover(&genesis, &initial_filled), initial_filled);

        let expiry_authority = stage8b_p1d3_test_expiry_authority(
            "1".repeat(64),
            "2026-07-25".to_string(),
            "1784999400000-0".to_string(),
            1_785_000_600_000,
        );
        let initial_expired = build_initial_limit_transition(
            &genesis,
            input(OrderSide::Buy, Decimal::new(2_210, 1)),
            None,
            None,
            Some(expiry_authority),
        )
        .unwrap();
        assert_eq!(recover(&genesis, &initial_expired), initial_expired);

        let later_filled = build_later_limit_transition(
            &initial_working.post_book,
            autonomous_input(&initial_working.post_book),
            Some(later_bar(
                Decimal::new(2_200, 1),
                Decimal::new(2_220, 1),
                Decimal::new(2_190, 1),
            )),
            Some(later_step()),
            None,
        )
        .unwrap();
        let Stage8bP1d3LaterEvaluationPlan::Outcome(later_filled) = later_filled else {
            panic!("expected later fill")
        };
        assert_eq!(
            recover(&initial_working.post_book, &later_filled),
            *later_filled
        );

        let untouched = build_later_limit_transition(
            &initial_working.post_book,
            autonomous_input(&initial_working.post_book),
            Some(later_bar(
                Decimal::new(2_230, 1),
                Decimal::new(2_240, 1),
                Decimal::new(2_220, 1),
            )),
            Some(later_step()),
            None,
        )
        .unwrap();
        let Stage8bP1d3LaterEvaluationPlan::Untouched {
            post_book: evaluated,
            ..
        } = untouched
        else {
            panic!("expected untouched evaluation")
        };
        let later_expired = build_later_limit_transition(
            &evaluated,
            autonomous_input(&evaluated),
            None,
            None,
            Some(stage8b_p1d3_test_expiry_authority(
                "1".repeat(64),
                "2026-07-25".to_string(),
                "1785000600000-0".to_string(),
                1_785_001_200_000,
            )),
        )
        .unwrap();
        let Stage8bP1d3LaterEvaluationPlan::Outcome(later_expired) = later_expired else {
            panic!("expected later expiry")
        };
        assert_eq!(recover(&evaluated, &later_expired), *later_expired);

        let target = initial_working
            .post_book
            .active_broker_order_id
            .clone()
            .unwrap();
        let Stage8bP1d3CancelTransitionPlan::Ready(cancel_canceled) = build_cancel_transition(
            &initial_working.post_book,
            cancel_input(&initial_working.post_book, target),
            later_bar(
                Decimal::new(2_230, 1),
                Decimal::new(2_240, 1),
                Decimal::new(2_220, 1),
            ),
            later_step(),
        )
        .unwrap() else {
            panic!("working target must cancel directly")
        };
        assert_eq!(
            recover(&initial_working.post_book, &cancel_canceled),
            *cancel_canceled
        );

        let filled_target = initial_filled
            .post_book
            .records
            .first()
            .unwrap()
            .broker_order_id
            .clone();
        let Stage8bP1d3CancelTransitionPlan::Ready(cancel_execution) = build_cancel_transition(
            &initial_filled.post_book,
            cancel_input(&initial_filled.post_book, filled_target),
            later_bar(
                Decimal::new(2_230, 1),
                Decimal::new(2_240, 1),
                Decimal::new(2_220, 1),
            ),
            later_step(),
        )
        .unwrap() else {
            panic!("terminal filled target must recover cancel directly")
        };
        assert_eq!(
            recover(&initial_filled.post_book, &cancel_execution),
            *cancel_execution
        );

        let expired_target = initial_expired.post_book.records[0].broker_order_id.clone();
        let expired_cancel_step = stage8b_p1d3_test_step_authority(
            "1".repeat(64),
            "2026-07-25".to_string(),
            "1785000600000-0".to_string(),
            "1784999400000-0".to_string(),
            "1785000600000-0".to_string(),
        );
        let Stage8bP1d3CancelTransitionPlan::Ready(cancel_nonexecution) = build_cancel_transition(
            &initial_expired.post_book,
            cancel_input(&initial_expired.post_book, expired_target),
            later_bar(
                Decimal::new(2_230, 1),
                Decimal::new(2_240, 1),
                Decimal::new(2_220, 1),
            ),
            expired_cancel_step,
        )
        .unwrap() else {
            panic!("terminal expired target must recover cancel directly")
        };
        assert_eq!(
            recover(&initial_expired.post_book, &cancel_nonexecution),
            *cancel_nonexecution
        );
    }

    #[derive(Serialize)]
    struct Stage8bP1d3GoldenShape {
        shape: &'static str,
        pre_book_sha256: String,
        post_book_sha256: String,
        fresh_canonical_bytes_hex: String,
        fresh_canonical_bytes_sha256: String,
        fresh_domain_digest_sha256: String,
        recovery_canonical_bytes_hex: String,
        recovery_canonical_bytes_sha256: String,
        recovery_domain_digest_sha256: String,
    }

    #[derive(Serialize)]
    struct Stage8bP1d3GoldenManifest {
        schema_version: u16,
        domain: &'static str,
        shape_count: usize,
        fresh_recovery_byte_identical: bool,
        shapes: Vec<Stage8bP1d3GoldenShape>,
    }

    #[derive(Debug, Clone, PartialEq, Eq, Serialize)]
    struct Stage8bP1d3ProjectionComponentGolden {
        canonical_bytes_hex: String,
        canonical_bytes_sha256: String,
    }

    #[derive(Debug, Clone, PartialEq, Eq, Serialize)]
    struct Stage8bP1d3ProjectionComponentsGolden {
        ack: Stage8bP1d3ProjectionComponentGolden,
        orders: Stage8bP1d3ProjectionComponentGolden,
        trades: Stage8bP1d3ProjectionComponentGolden,
        positions: Stage8bP1d3ProjectionComponentGolden,
        truth: Stage8bP1d3ProjectionComponentGolden,
        complete_projection: Stage8bP1d3ProjectionComponentGolden,
    }

    #[derive(Serialize)]
    struct Stage8bP1d3ProjectionGoldenShape {
        shape: &'static str,
        pre_book_sha256: String,
        post_book_sha256: String,
        reserved_seq_ack: Option<u64>,
        reserved_seq_truth: Option<u64>,
        sequence_allocation_frontier: u64,
        fresh: Stage8bP1d3ProjectionComponentsGolden,
        recovery: Stage8bP1d3ProjectionComponentsGolden,
        fresh_recovery_byte_identical: bool,
    }

    #[derive(Serialize)]
    struct Stage8bP1d3ProjectionGoldenManifest {
        schema_version: u16,
        domain: &'static str,
        shape_count: usize,
        fresh_and_recovery_each_checked_against_oracle: bool,
        shapes: Vec<Stage8bP1d3ProjectionGoldenShape>,
    }

    fn projection_component(value: &serde_json::Value) -> Stage8bP1d3ProjectionComponentGolden {
        let bytes = serde_json::to_vec(value).unwrap();
        Stage8bP1d3ProjectionComponentGolden {
            canonical_bytes_hex: hex_digest(&bytes),
            canonical_bytes_sha256: sha256_hex(&bytes),
        }
    }

    fn optional_decimal_bytes(value: Option<Decimal>) -> Option<[u8; 16]> {
        value.map(|value| value.serialize())
    }

    fn exact_order_rows(truth: Option<&BrokerTruthSnapshot>) -> serde_json::Value {
        serde_json::to_value(
            truth
                .map(|truth| {
                    truth
                        .orders
                        .iter()
                        .map(|row| {
                            serde_json::json!({
                                "snapshot": row,
                                "decimal_bytes": {
                                    "qty": row.qty.serialize(),
                                    "filled_qty": row.filled_qty.serialize(),
                                    "remaining_qty": optional_decimal_bytes(row.remaining_qty),
                                    "limit_price": optional_decimal_bytes(row.limit_price),
                                }
                            })
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default(),
        )
        .unwrap()
    }

    fn exact_trade_rows(truth: Option<&BrokerTruthSnapshot>) -> serde_json::Value {
        serde_json::to_value(
            truth
                .map(|truth| {
                    truth
                        .trades
                        .iter()
                        .map(|row| {
                            serde_json::json!({
                                "snapshot": row,
                                "decimal_bytes": {
                                    "qty": row.qty.serialize(),
                                    "price": row.price.serialize(),
                                    "gross_amount": optional_decimal_bytes(row.gross_amount),
                                    "commission": optional_decimal_bytes(row.commission),
                                }
                            })
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default(),
        )
        .unwrap()
    }

    fn exact_position_rows(truth: Option<&BrokerTruthSnapshot>) -> serde_json::Value {
        serde_json::to_value(
            truth
                .map(|truth| {
                    truth
                        .positions
                        .iter()
                        .map(|row| {
                            serde_json::json!({
                                "snapshot": row,
                                "decimal_bytes": {
                                    "qty": row.qty.serialize(),
                                    "avg_price": optional_decimal_bytes(row.avg_price),
                                    "unrealized_pnl": optional_decimal_bytes(row.unrealized_pnl),
                                }
                            })
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default(),
        )
        .unwrap()
    }

    fn exact_cash(truth: &BrokerTruthSnapshot) -> serde_json::Value {
        truth.cash.as_ref().map_or(serde_json::Value::Null, |cash| {
            serde_json::json!({
                "snapshot": cash,
                "decimal_bytes": {
                    "cash": cash.cash.iter().map(|row| serde_json::json!({
                        "currency": row.currency,
                        "amount": row.amount.serialize(),
                    })).collect::<Vec<_>>(),
                    "equity": optional_decimal_bytes(cash.equity),
                    "free_cash": optional_decimal_bytes(cash.free_cash),
                    "initial_margin": optional_decimal_bytes(cash.initial_margin),
                    "maintenance_margin": optional_decimal_bytes(cash.maintenance_margin),
                }
            })
        })
    }

    fn exact_instruments(truth: &BrokerTruthSnapshot) -> serde_json::Value {
        serde_json::to_value(
            truth
                .instruments
                .iter()
                .map(|row| {
                    serde_json::json!({
                        "snapshot": row,
                        "decimal_bytes": {
                            "price_step": row.instrument.price_step.serialize(),
                            "qty_step": row.instrument.qty_step.serialize(),
                            "lot_size": row.instrument.lot_size.serialize(),
                            "min_qty": row.instrument.min_qty.serialize(),
                            "step_value": row.instrument.step_value.serialize(),
                            "long_initial_margin": optional_decimal_bytes(row.long_initial_margin),
                            "short_initial_margin": optional_decimal_bytes(row.short_initial_margin),
                        }
                    })
                })
                .collect::<Vec<_>>(),
        )
        .unwrap()
    }

    fn exact_truth(truth: Option<&BrokerTruthSnapshot>) -> serde_json::Value {
        truth.map_or(serde_json::Value::Null, |truth| {
            serde_json::json!({
                "account_id": truth.account_id,
                "orders": exact_order_rows(Some(truth)),
                "positions": exact_position_rows(Some(truth)),
                "cash": exact_cash(truth),
                "trades": exact_trade_rows(Some(truth)),
                "instruments": exact_instruments(truth),
                "received_ts": truth.received_ts,
            })
        })
    }

    fn complete_projection_components(
        pre_book: &Stage8bP1d3WorkingBookProjectionV1,
        plan: &Stage8bP1d3TransitionPlan,
    ) -> Stage8bP1d3ProjectionComponentsGolden {
        let ack = serde_json::to_value(&plan.ack).unwrap();
        let orders = exact_order_rows(plan.truth.as_ref());
        let trades = exact_trade_rows(plan.truth.as_ref());
        let positions = exact_position_rows(plan.truth.as_ref());
        let truth = exact_truth(plan.truth.as_ref());
        let projection = serde_json::json!({
            "domain": "moex.stage8b.p1d3.complete-projection.v1",
            "outcome_kind": plan.evidence.outcome_kind.canonical_name(),
            "outcome_evidence_sha256": plan.evidence_sha256,
            "reserved_seq_ack": plan.evidence.reserved_seq_ack,
            "reserved_seq_truth": plan.evidence.reserved_seq_truth,
            "sequence_allocation_frontier": plan.evidence.sequence_allocation_frontier,
            "pre_book_sha256": pre_book.canonical_sha256().unwrap(),
            "post_book_sha256": plan.post_book.canonical_sha256().unwrap(),
            "ack": ack,
            "orders": orders,
            "trades": trades,
            "positions": positions,
            "truth": truth,
        });
        Stage8bP1d3ProjectionComponentsGolden {
            ack: projection_component(&projection["ack"]),
            orders: projection_component(&projection["orders"]),
            trades: projection_component(&projection["trades"]),
            positions: projection_component(&projection["positions"]),
            truth: projection_component(&projection["truth"]),
            complete_projection: projection_component(&projection),
        }
    }

    #[test]
    fn all_eight_fresh_and_recovery_paths_match_checked_in_canonical_goldens() {
        let mut shapes = Vec::new();
        for (shape, pre_book, fresh) in all_eight_shape_plans() {
            let recovery = recover(&pre_book, &fresh);
            let fresh_bytes = fresh.evidence.encode_canonical().unwrap();
            let recovery_bytes = recovery.evidence.encode_canonical().unwrap();
            assert_eq!(fresh_bytes, recovery_bytes, "{shape}");
            assert_eq!(fresh.post_book, recovery.post_book, "{shape}");
            shapes.push(Stage8bP1d3GoldenShape {
                shape,
                pre_book_sha256: pre_book.canonical_sha256().unwrap(),
                post_book_sha256: fresh.post_book.canonical_sha256().unwrap(),
                fresh_canonical_bytes_hex: hex_digest(&fresh_bytes),
                fresh_canonical_bytes_sha256: sha256_hex(&fresh_bytes),
                fresh_domain_digest_sha256: fresh.evidence.digest_sha256().unwrap(),
                recovery_canonical_bytes_hex: hex_digest(&recovery_bytes),
                recovery_canonical_bytes_sha256: sha256_hex(&recovery_bytes),
                recovery_domain_digest_sha256: recovery.evidence.digest_sha256().unwrap(),
            });
        }
        let manifest = Stage8bP1d3GoldenManifest {
            schema_version: 1,
            domain: "moex.stage8b.p1d3.outcome-golden.v1",
            shape_count: shapes.len(),
            fresh_recovery_byte_identical: true,
            shapes,
        };
        let actual = serde_json::to_string_pretty(&manifest).unwrap() + "\n";
        let fixture_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/stage8b-p1d3/outcome-golden-v1.json");
        if std::env::var_os("STAGE8B_P1D3_UPDATE_GOLDEN").is_some() {
            std::fs::create_dir_all(fixture_path.parent().unwrap()).unwrap();
            std::fs::write(&fixture_path, &actual).unwrap();
        }
        let expected = std::fs::read_to_string(&fixture_path)
            .expect("checked-in P1-d3 golden manifest must exist");
        assert_eq!(actual, expected);

        let mut projection_shapes = Vec::new();
        for (shape, pre_book, fresh) in all_eight_shape_plans() {
            let recovery = recover(&pre_book, &fresh);
            let fresh_projection = complete_projection_components(&pre_book, &fresh);
            let recovery_projection = complete_projection_components(&pre_book, &recovery);
            assert_eq!(fresh_projection, recovery_projection, "{shape}");
            projection_shapes.push(Stage8bP1d3ProjectionGoldenShape {
                shape,
                pre_book_sha256: pre_book.canonical_sha256().unwrap(),
                post_book_sha256: fresh.post_book.canonical_sha256().unwrap(),
                reserved_seq_ack: fresh.evidence.reserved_seq_ack,
                reserved_seq_truth: fresh.evidence.reserved_seq_truth,
                sequence_allocation_frontier: fresh.evidence.sequence_allocation_frontier,
                fresh: fresh_projection,
                recovery: recovery_projection,
                fresh_recovery_byte_identical: true,
            });
        }
        let projection_manifest = Stage8bP1d3ProjectionGoldenManifest {
            schema_version: 1,
            domain: "moex.stage8b.p1d3.projection-golden.v1",
            shape_count: projection_shapes.len(),
            fresh_and_recovery_each_checked_against_oracle: true,
            shapes: projection_shapes,
        };
        let projection_actual = serde_json::to_string_pretty(&projection_manifest).unwrap() + "\n";
        let projection_fixture_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/stage8b-p1d3/projection-golden-v1.json");
        if std::env::var_os("STAGE8B_P1D3_UPDATE_GOLDEN").is_some() {
            std::fs::write(&projection_fixture_path, &projection_actual).unwrap();
        }
        let projection_expected = std::fs::read_to_string(&projection_fixture_path)
            .expect("checked-in complete P1-d3 projection golden manifest must exist");
        assert_eq!(projection_actual, projection_expected);
    }

    #[test]
    fn canonical_terminal_registry_is_independent_of_in_memory_insertion_order() {
        let first = initial_filled();
        let mut second_input = input(OrderSide::Buy, Decimal::new(2_215, 1));
        second_input.request_id = request(3);
        second_input.durable_client_order_id = ClientOrderId::from_strategy_request(request(3));
        second_input.canonical_command_sha256 = "36".repeat(32);
        second_input.accepted_command_payload_sha256 = "37".repeat(32);
        second_input.accepted_stage6_identity_sha256 = "38".repeat(32);
        second_input.stage6_dispatch_record_id = "3b".repeat(32);
        second_input.stage6_predecessor_frontier_sha256 = "3d".repeat(32);
        second_input.stage6_reserved_checkpoint_sha256 = "3e".repeat(32);
        second_input.sequence_allocation_frontier = first.post_book.total_sequence_frontier;
        second_input.previous_outcome_evidence_sha256 =
            first.post_book.latest_outcome_evidence_sha256.clone();
        let second = build_initial_limit_transition(
            &first.post_book,
            second_input,
            None,
            None,
            Some(stage8b_p1d3_test_expiry_authority(
                "1".repeat(64),
                "2026-07-25".to_string(),
                "1784999400000-0".to_string(),
                1_785_000_600_000,
            )),
        )
        .unwrap();
        let canonical = second.post_book;
        let mut permuted = canonical.clone();
        permuted.records.reverse();
        let records = std::mem::take(&mut permuted.records);
        for record in records {
            permuted.insert_or_replace(record).unwrap();
        }
        assert_eq!(canonical.records, permuted.records);
        assert_eq!(
            canonical.encode_canonical().unwrap(),
            permuted.encode_canonical().unwrap()
        );
    }

    #[test]
    fn authenticated_recovery_rejects_changed_stage6_binding_before_projection() {
        let fresh = initial_filled();
        let record = crate::Stage6JournalRecordV3::from_p1d3_outcome_evidence(
            crate::Stage6LifecycleSequence::new(3).unwrap(),
            crate::Stage6JournalRecordId::parse_exact("aa".repeat(32)).unwrap(),
            fresh.evidence_bytes,
        )
        .unwrap();
        let result = record.authenticate_p1d3_outcome(Stage8bP1d3Stage6RecoveryBinding {
            outcome_record_id: "ab".repeat(32),
            predecessor_frontier_sha256: fresh.evidence.stage6_predecessor_frontier_sha256,
            reserved_checkpoint_sha256: fresh.evidence.stage6_reserved_checkpoint_sha256,
            request_finalized_record_id: fresh.evidence.stage7_request_finalized_record_id,
            request_finalized_fingerprint_sha256: fresh
                .evidence
                .stage7_request_finalized_fingerprint_sha256,
            authenticated_post_checkpoint_sha256: "ab".repeat(32),
        });
        assert!(matches!(result, Err(Stage8bP1d3Error::IdentityMismatch)));
    }

    #[test]
    fn full_outcome_evidence_is_inside_one_canonical_stage6_record() {
        use crate::Stage6JournalBackend;

        let fresh = initial_working();
        let previous = crate::Stage6JournalRecordId::parse_exact("aa".repeat(32)).unwrap();
        let record = crate::Stage6JournalRecordV3::from_p1d3_outcome_evidence(
            crate::Stage6LifecycleSequence::new(3).unwrap(),
            previous,
            fresh.evidence_bytes.clone(),
        )
        .unwrap();
        assert_eq!(record.outcome_evidence_bytes(), fresh.evidence_bytes);
        assert_eq!(
            record.outcome_evidence_sha256().as_str(),
            fresh.evidence_sha256
        );
        assert_eq!(
            record.journal_record_id().as_str(),
            fresh.evidence.stage6_outcome_record_id()
        );

        let canonical = record.encode_canonical();
        assert_eq!(
            crate::Stage6JournalRecordV3::decode_canonical(&canonical).unwrap(),
            record
        );
        let mut backend = crate::Stage6MemoryJournalBackend::new();
        backend
            .append_versioned(&crate::Stage6JournalRecordVersioned::V3(record.clone()))
            .unwrap();
        assert_eq!(backend.versioned_records().len(), 1);
        assert_eq!(backend.versioned_records()[0].encode_canonical(), canonical);

        let mut changed: serde_json::Value = serde_json::from_slice(&canonical).unwrap();
        changed["outcome_evidence_bytes"][0] = serde_json::json!(0);
        let changed = serde_json::to_vec(&changed).unwrap();
        assert!(crate::Stage6JournalRecordV3::decode_canonical(&changed).is_err());
    }

    fn initial_working() -> Stage8bP1d3TransitionPlan {
        build_initial_limit_transition(
            &book(),
            input(OrderSide::Buy, Decimal::new(2_210, 1)),
            Some(bar(
                Decimal::new(2_230, 1),
                Decimal::new(2_240, 1),
                Decimal::new(2_220, 1),
            )),
            Some(step()),
            None,
        )
        .unwrap()
    }

    fn migrated_restart_package() -> (
        crate::Stage5gCleanRestartedCapability,
        crate::Stage5gLifecycleCommitmentKey,
    ) {
        let (ready, export_input, key, fresh_runtime) =
            crate::stage5g_timer::stage8b_p1_test_first_boot_material();
        let runtime = ready.stage5g_runtime_strategy().clone();
        let state = crate::stage5g_order_position::tests::stage8b_p1d3_quiescent_state_fixture(20);
        let replacement =
            Stage8bP1d3ReplacementProjectionV1::migrated(book(), "d".repeat(64)).unwrap();
        let source = crate::Stage5gCleanRestartSource::P1d3(Box::new(
            Stage8bP1d3RestartSource::new(runtime, state, replacement).unwrap(),
        ));
        let bytes = crate::export_stage5g_clean_restart(source, export_input, &key).unwrap();
        let restored = crate::restore_stage5g_clean_restart(&bytes, &key, fresh_runtime).unwrap();
        (restored, key)
    }

    fn stage6_record_and_binding(
        plan: &Stage8bP1d3TransitionPlan,
    ) -> (
        crate::Stage6JournalRecordV3,
        Stage8bP1d3Stage6RecoveryBinding,
    ) {
        stage6_record_and_binding_with_post_checkpoint(plan, "ab".repeat(32))
    }

    fn stage6_record_and_binding_with_post_checkpoint(
        plan: &Stage8bP1d3TransitionPlan,
        authenticated_post_checkpoint_sha256: String,
    ) -> (
        crate::Stage6JournalRecordV3,
        Stage8bP1d3Stage6RecoveryBinding,
    ) {
        let record = crate::Stage6JournalRecordV3::from_p1d3_outcome_evidence(
            crate::Stage6LifecycleSequence::new(3).unwrap(),
            crate::Stage6JournalRecordId::parse_exact("ab".repeat(32)).unwrap(),
            plan.evidence_bytes.clone(),
        )
        .unwrap();
        let binding = Stage8bP1d3Stage6RecoveryBinding {
            outcome_record_id: plan.evidence.stage6_outcome_record_id.clone(),
            predecessor_frontier_sha256: plan.evidence.stage6_predecessor_frontier_sha256.clone(),
            reserved_checkpoint_sha256: plan.evidence.stage6_reserved_checkpoint_sha256.clone(),
            request_finalized_record_id: plan.evidence.stage7_request_finalized_record_id.clone(),
            request_finalized_fingerprint_sha256: plan
                .evidence
                .stage7_request_finalized_fingerprint_sha256
                .clone(),
            authenticated_post_checkpoint_sha256,
        };
        (record, binding)
    }

    fn commit_request_plan(
        plan: &Stage8bP1d3TransitionPlan,
    ) -> (
        Stage8bP1d3ReplacementStageResult,
        crate::Stage5gLifecycleCommitmentKey,
    ) {
        let (record, binding) = stage6_record_and_binding(plan);
        let (migrated, key) = migrated_restart_package();
        let ack = apply_stage8b_p1d3_request_ack_stage(migrated, &record, binding, &key).unwrap();
        let truth = apply_stage8b_p1d3_truth_after_ack_stage(ack.restored, &key).unwrap();
        (truth, key)
    }

    #[test]
    fn genesis_changes_with_generation_and_identity() {
        let first = genesis_sha256(&"1".repeat(64), &"2".repeat(64), 7).unwrap();
        let generation = genesis_sha256(&"1".repeat(64), &"2".repeat(64), 8).unwrap();
        let identity = genesis_sha256(&"1".repeat(64), &"3".repeat(64), 7).unwrap();
        assert_ne!(first, generation);
        assert_ne!(first, identity);
    }

    #[test]
    fn replacement_projection_is_inside_authenticated_stage5g_package() {
        let (ready, export_input, key, fresh_runtime) =
            crate::stage5g_timer::stage8b_p1_test_first_boot_material();
        let runtime = ready.stage5g_runtime_strategy().clone();
        let (_, state) = crate::stage5g_order_position::tests::stage8b_p1d3_restart_fixture();
        let book = Stage8bP1d3WorkingBookProjectionV1::migrate_from_p1d2(
            &"1".repeat(64),
            "2".repeat(64),
            7,
            account(),
            instrument(),
            attribution(),
            0,
        )
        .unwrap();
        let replacement =
            Stage8bP1d3ReplacementProjectionV1::migrated(book.clone(), "a".repeat(64)).unwrap();
        let source = crate::Stage5gCleanRestartSource::P1d3(Box::new(
            Stage8bP1d3RestartSource::new(runtime, state, replacement).unwrap(),
        ));
        let bytes = crate::export_stage5g_clean_restart(source, export_input, &key).unwrap();
        let restored = crate::restore_stage5g_clean_restart(&bytes, &key, fresh_runtime).unwrap();
        let actual = restored.stage8b_p1d3_replacement().unwrap();
        assert_eq!(actual.phase(), Stage8bP1d3BookPhase::Migrated);
        assert_eq!(
            actual.working_book().encode_canonical().unwrap(),
            book.encode_canonical().unwrap()
        );

        let mut package: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let extension = package["stage5g_extension_json"].as_str().unwrap();
        let mut projection: serde_json::Value = serde_json::from_str(extension).unwrap();
        projection["p1d3_replacement"]["authenticated_stage6_checkpoint_sha256"] =
            serde_json::json!("b".repeat(64));
        package["stage5g_extension_json"] = serde_json::json!(projection.to_string());
        let forged = serde_json::to_vec(&package).unwrap();
        assert!(crate::restore_stage5g_clean_restart(
            &forged,
            &key,
            restored.stage5g_fresh_reconstruction_candidate(),
        )
        .is_err());
    }

    #[test]
    fn authenticated_initial_working_commits_ack_then_truth_replacement() {
        let plan = initial_working();
        let (record, binding) = stage6_record_and_binding(&plan);
        let (migrated, key) = migrated_restart_package();
        let state = migrated.stage5g_restart_order_position_state().unwrap();
        assert_eq!(
            crate::Stage5gOrderPositionSession::stage8b_p1d3_total_sequence_frontier(&state),
            20
        );
        assert_eq!(plan.evidence.reserved_seq_ack, Some(21));
        let pre_feedback_runtime_sha256 = migrated.reconstructed_runtime_state_fingerprint_sha256();

        let ack = apply_stage8b_p1d3_request_ack_stage(migrated, &record, binding, &key).unwrap();
        assert_eq!(
            ack.restored.stage8b_p1d3_replacement().unwrap().phase(),
            Stage8bP1d3BookPhase::Ack
        );
        assert_eq!(
            ack.restored
                .stage8b_p1d3_replacement()
                .unwrap()
                .authenticated_stage6_checkpoint_sha256,
            "ab".repeat(32)
        );
        assert_eq!(
            ack.restored
                .stage8b_p1d3_replacement()
                .unwrap()
                .latest_stage6_reservation_sha256,
            plan.evidence.stage6_reserved_checkpoint_sha256
        );

        let truth = apply_stage8b_p1d3_truth_after_ack_stage(ack.restored, &key).unwrap();
        let replacement = truth.restored.stage8b_p1d3_replacement().unwrap();
        assert_eq!(replacement.phase(), Stage8bP1d3BookPhase::Working);
        assert_eq!(
            replacement.working_book().encode_canonical().unwrap(),
            plan.post_book.encode_canonical().unwrap()
        );
        assert_eq!(
            truth.restored.checkpoint().payload.last_total_sequence,
            Some(22)
        );
        assert_eq!(
            truth
                .restored
                .reconstructed_runtime_state_fingerprint_sha256(),
            pre_feedback_runtime_sha256,
            "a Working broker-order callback may be state-neutral for Hybrid; the authoritative order remains in the authenticated P1-d3 book"
        );
        assert_eq!(
            truth.restored.strategy_state_fingerprint_sha256(),
            truth
                .restored
                .reconstructed_runtime_state_fingerprint_sha256()
        );
    }

    #[test]
    fn initial_filled_and_expired_commit_terminal_replacements() {
        let filled = build_initial_limit_transition(
            &book(),
            input(OrderSide::Buy, Decimal::new(2_210, 1)),
            Some(bar(
                Decimal::new(2_200, 1),
                Decimal::new(2_220, 1),
                Decimal::new(2_190, 1),
            )),
            Some(step()),
            None,
        )
        .unwrap();
        let (filled, _) = commit_request_plan(&filled);
        assert_eq!(
            filled.restored.stage8b_p1d3_replacement().unwrap().phase(),
            Stage8bP1d3BookPhase::Terminal
        );
        assert!(!filled.restart_package.is_empty());

        let expired = build_initial_limit_transition(
            &book(),
            input(OrderSide::Buy, Decimal::new(2_210, 1)),
            None,
            None,
            Some(stage8b_p1d3_test_expiry_authority(
                "1".repeat(64),
                "2026-07-25".to_string(),
                "1784999400000-0".to_string(),
                1_785_000_600_000,
            )),
        )
        .unwrap();
        let (expired, _) = commit_request_plan(&expired);
        assert_eq!(
            expired.restored.stage8b_p1d3_replacement().unwrap().phase(),
            Stage8bP1d3BookPhase::Terminal
        );
    }

    #[test]
    fn untouched_evaluation_seals_without_sequence_and_exact_replay_is_noop() {
        let initial = initial_working();
        let (working, key) = commit_request_plan(&initial);
        let evaluation = build_later_limit_transition(
            working
                .restored
                .stage8b_p1d3_replacement()
                .unwrap()
                .working_book(),
            autonomous_input(
                working
                    .restored
                    .stage8b_p1d3_replacement()
                    .unwrap()
                    .working_book(),
            ),
            Some(later_bar(
                Decimal::new(2_230, 1),
                Decimal::new(2_240, 1),
                Decimal::new(2_220, 1),
            )),
            Some(later_step()),
            None,
        )
        .unwrap();
        let Stage8bP1d3EvaluationStageResult::Committed(evaluated) =
            apply_stage8b_p1d3_evaluation_stage(working.restored, evaluation, &key).unwrap()
        else {
            panic!("expected replacement S_eval")
        };
        assert_eq!(
            evaluated
                .restored
                .stage8b_p1d3_replacement()
                .unwrap()
                .phase(),
            Stage8bP1d3BookPhase::Eval
        );
        assert_eq!(
            evaluated.restored.checkpoint().payload.last_total_sequence,
            Some(22)
        );

        let current = evaluated.restored.stage8b_p1d3_replacement().unwrap();
        let replay = build_later_limit_transition(
            current.working_book(),
            autonomous_input(current.working_book()),
            Some(later_bar(
                Decimal::new(2_230, 1),
                Decimal::new(2_240, 1),
                Decimal::new(2_220, 1),
            )),
            Some(later_step()),
            None,
        )
        .unwrap();
        assert!(matches!(
            apply_stage8b_p1d3_evaluation_stage(evaluated.restored, replay, &key).unwrap(),
            Stage8bP1d3EvaluationStageResult::AlreadyEvaluated
        ));
    }

    #[test]
    fn later_fill_commits_terminal_truth_without_ack() {
        let initial = initial_working();
        let (working, key) = commit_request_plan(&initial);
        let current = working.restored.stage8b_p1d3_replacement().unwrap();
        let mut input = autonomous_input(current.working_book());
        input.stage6_predecessor_frontier_sha256 =
            current.authenticated_stage6_checkpoint_sha256.clone();
        let transition = build_later_limit_transition(
            current.working_book(),
            input,
            Some(later_bar(
                Decimal::new(2_200, 1),
                Decimal::new(2_220, 1),
                Decimal::new(2_190, 1),
            )),
            Some(later_step()),
            None,
        )
        .unwrap();
        let Stage8bP1d3LaterEvaluationPlan::Outcome(plan) = transition else {
            panic!("expected later fill")
        };
        let (record, binding) = stage6_record_and_binding(&plan);
        let terminal =
            apply_stage8b_p1d3_autonomous_truth_stage(working.restored, &record, binding, &key)
                .unwrap();
        assert_eq!(
            terminal
                .restored
                .stage8b_p1d3_replacement()
                .unwrap()
                .phase(),
            Stage8bP1d3BookPhase::Terminal
        );
        assert_eq!(
            terminal.restored.checkpoint().payload.last_total_sequence,
            Some(23)
        );
    }

    #[test]
    fn cancel_working_commits_ack_then_terminal_truth() {
        let initial = initial_working();
        let (working, key) = commit_request_plan(&initial);
        let current = working.restored.stage8b_p1d3_replacement().unwrap();
        let target = current
            .working_book()
            .active_broker_order_id
            .clone()
            .unwrap();
        let mut input = cancel_input(current.working_book(), target);
        input.stage6_predecessor_frontier_sha256 =
            current.authenticated_stage6_checkpoint_sha256.clone();
        let Stage8bP1d3CancelTransitionPlan::Ready(cancel) = build_cancel_transition(
            current.working_book(),
            input,
            later_bar(
                Decimal::new(2_230, 1),
                Decimal::new(2_240, 1),
                Decimal::new(2_220, 1),
            ),
            later_step(),
        )
        .unwrap() else {
            panic!("working target must cancel directly")
        };
        let (record, binding) = stage6_record_and_binding(&cancel);
        let ack =
            apply_stage8b_p1d3_request_ack_stage(working.restored, &record, binding, &key).unwrap();
        assert_eq!(
            ack.restored.stage8b_p1d3_replacement().unwrap().phase(),
            Stage8bP1d3BookPhase::Ack
        );
        let terminal = apply_stage8b_p1d3_truth_after_ack_stage(ack.restored, &key).unwrap();
        assert_eq!(
            terminal
                .restored
                .stage8b_p1d3_replacement()
                .unwrap()
                .phase(),
            Stage8bP1d3BookPhase::Terminal
        );
        assert_eq!(
            terminal.restored.checkpoint().payload.last_total_sequence,
            Some(24)
        );
    }

    #[test]
    fn recovered_cancel_commits_distinct_ack_only_terminal_seal() {
        let filled = build_initial_limit_transition(
            &book(),
            input(OrderSide::Buy, Decimal::new(2_210, 1)),
            Some(bar(
                Decimal::new(2_200, 1),
                Decimal::new(2_220, 1),
                Decimal::new(2_190, 1),
            )),
            Some(step()),
            None,
        )
        .unwrap();
        let filled_book = filled.post_book.clone();
        let (terminal, key) = commit_request_plan(&filled);
        let target = filled_book.records[0].broker_order_id.clone();
        let current = terminal.restored.stage8b_p1d3_replacement().unwrap();
        let mut input = cancel_input(&filled_book, target);
        input.stage6_predecessor_frontier_sha256 =
            current.authenticated_stage6_checkpoint_sha256.clone();
        let Stage8bP1d3CancelTransitionPlan::Ready(cancel) = build_cancel_transition(
            &filled_book,
            input,
            later_bar(
                Decimal::new(2_230, 1),
                Decimal::new(2_240, 1),
                Decimal::new(2_220, 1),
            ),
            later_step(),
        )
        .unwrap() else {
            panic!("terminal filled target must recover cancel directly")
        };
        let (record, binding) = stage6_record_and_binding(&cancel);
        let recovered =
            apply_stage8b_p1d3_request_ack_stage(terminal.restored, &record, binding, &key)
                .unwrap();
        let replacement = recovered.restored.stage8b_p1d3_replacement().unwrap();
        assert_eq!(replacement.phase(), Stage8bP1d3BookPhase::CancelRecovered);
        assert_eq!(
            replacement.working_book().encode_canonical().unwrap(),
            filled_book.encode_canonical().unwrap()
        );
        assert!(apply_stage8b_p1d3_truth_after_ack_stage(recovered.restored, &key).is_err());
    }

    #[test]
    fn fill_before_cancel_commits_two_ordered_stage6_checkpoint_replacements() {
        let initial = initial_working();
        let (working, key) = commit_request_plan(&initial);
        let current = working.restored.stage8b_p1d3_replacement().unwrap();
        assert_eq!(current.phase(), Stage8bP1d3BookPhase::Working);
        assert_eq!(
            current.authenticated_stage6_checkpoint_sha256,
            "ab".repeat(32)
        );
        assert_eq!(current.latest_stage6_reservation_sha256, "e".repeat(64));

        let target = current
            .working_book()
            .active_broker_order_id
            .clone()
            .unwrap();
        let mut input = cancel_input(current.working_book(), target);
        input.stage6_predecessor_frontier_sha256 =
            current.authenticated_stage6_checkpoint_sha256.clone();
        input.target_stage6_reserved_checkpoint_sha256 = Some("27".repeat(32));
        let transitions = build_cancel_transition(
            current.working_book(),
            input,
            later_bar(
                Decimal::new(2_200, 1),
                Decimal::new(2_220, 1),
                Decimal::new(2_190, 1),
            ),
            later_step(),
        )
        .unwrap();
        let Stage8bP1d3CancelTransitionPlan::TargetThenCancel {
            target_transition: fill,
            continuation,
        } = transitions
        else {
            panic!("filled candidate must defer cancel until target reread")
        };
        assert_eq!(
            fill.evidence.stage6_predecessor_frontier_sha256,
            "ab".repeat(32)
        );
        assert_eq!(
            fill.evidence.stage6_reserved_checkpoint_sha256,
            "27".repeat(32)
        );
        let (fill_record, fill_binding) =
            stage6_record_and_binding_with_post_checkpoint(&fill, "44".repeat(32));
        let terminal = apply_stage8b_p1d3_autonomous_truth_stage(
            working.restored,
            &fill_record,
            fill_binding,
            &key,
        )
        .unwrap();
        let terminal_projection = terminal.restored.stage8b_p1d3_replacement().unwrap();
        assert_eq!(terminal_projection.phase(), Stage8bP1d3BookPhase::Terminal);
        assert_eq!(
            terminal_projection.authenticated_stage6_checkpoint_sha256,
            "44".repeat(32)
        );
        assert_eq!(
            terminal.restored.checkpoint().payload.last_total_sequence,
            Some(23)
        );

        let cancel = continuation
            .complete(terminal_projection.working_book(), "44".repeat(32))
            .unwrap();
        assert_eq!(
            cancel.evidence.stage6_predecessor_frontier_sha256,
            "44".repeat(32)
        );
        assert_eq!(
            cancel.evidence.stage6_reserved_checkpoint_sha256,
            "28".repeat(32)
        );
        let (cancel_record, cancel_binding) =
            stage6_record_and_binding_with_post_checkpoint(&cancel, "55".repeat(32));
        let recovered = apply_stage8b_p1d3_request_ack_stage(
            terminal.restored,
            &cancel_record,
            cancel_binding,
            &key,
        )
        .unwrap();
        let recovered_projection = recovered.restored.stage8b_p1d3_replacement().unwrap();
        assert_eq!(
            recovered_projection.phase(),
            Stage8bP1d3BookPhase::CancelRecovered
        );
        assert_eq!(
            recovered_projection.authenticated_stage6_checkpoint_sha256,
            "55".repeat(32)
        );
        let recovered_state = recovered
            .restored
            .stage5g_restart_order_position_state()
            .unwrap();
        assert_eq!(
            recovered.restored.checkpoint().payload.last_total_sequence,
            Some(23)
        );
        assert_eq!(
            crate::Stage5gOrderPositionSession::stage8b_p1d3_total_sequence_frontier(
                &recovered_state
            ),
            24
        );
        assert!(apply_stage8b_p1d3_truth_after_ack_stage(recovered.restored, &key).is_err());
    }

    #[test]
    fn mixed_stage6_replay_applies_atomic_v3_request_outcome_then_finalization() {
        use crate::Stage6JournalBackend;

        let mut exact_input = input(OrderSide::Buy, Decimal::new(2_210, 1));
        let command = PlaceOrder {
            request_id: exact_input.request_id,
            created_ts: Utc
                .timestamp_millis_opt(exact_input.decision_m10_close_ts_utc_ms)
                .single()
                .unwrap(),
            ttl_ms: Some(5_000),
            account_id: exact_input.account_id.clone(),
            client_order_id: exact_input.durable_client_order_id.clone(),
            instrument: exact_input.instrument.clone(),
            side: exact_input.side,
            order_type: OrderType::Limit,
            qty: exact_input.qty,
            limit_price: Some(exact_input.limit_price),
            time_in_force: TimeInForce::Day,
            comment: Some(exact_input.attribution.internal_comment().to_string()),
        };
        let identity = crate::Stage6DurableRequestIdentityV1::from_place(
            &command,
            exact_input.attribution.clone(),
        )
        .unwrap();
        let snapshot =
            crate::Stage6DurableCommandSnapshotV1::from_place(&identity, &command).unwrap();
        let accepted = crate::Stage6JournalRecordV1::request_accepted(
            identity.clone(),
            snapshot,
            crate::Stage6LifecycleSequence::new(1).unwrap(),
            None,
            None,
            crate::Stage6Sha256Digest::parse("31".repeat(32)).unwrap(),
        )
        .unwrap();
        let dispatch = crate::Stage6JournalRecordV1::dispatch_attempt_recorded(
            identity.clone(),
            1,
            accepted.canonical_payload_sha256().clone(),
            crate::Stage6LifecycleSequence::new(2).unwrap(),
            Some(accepted.journal_record_id().clone()),
            crate::Stage6Sha256Digest::parse("32".repeat(32)).unwrap(),
        )
        .unwrap();
        exact_input.accepted_command_payload_sha256 =
            accepted.canonical_payload_sha256().as_str().to_string();
        exact_input.stage6_dispatch_record_id = dispatch.journal_record_id().as_str().to_string();
        let plan = build_initial_limit_transition(
            &book(),
            exact_input,
            Some(bar(
                Decimal::new(2_230, 1),
                Decimal::new(2_240, 1),
                Decimal::new(2_220, 1),
            )),
            Some(step()),
            None,
        )
        .unwrap();
        let outcome = crate::Stage6JournalRecordV3::from_p1d3_outcome_evidence(
            crate::Stage6LifecycleSequence::new(3).unwrap(),
            dispatch.journal_record_id().clone(),
            plan.evidence_bytes,
        )
        .unwrap();
        let finalized = crate::Stage6JournalRecordV1::request_finalized(
            identity,
            crate::Stage6RequestFinalDispositionV1::Completed,
            crate::Stage6LifecycleSequence::new(4).unwrap(),
            Some(outcome.journal_record_id().clone()),
            crate::Stage6Sha256Digest::parse("2b".repeat(32)).unwrap(),
        )
        .unwrap();
        assert_eq!(
            finalized.journal_record_id().as_str(),
            plan.evidence
                .stage7_request_finalized_record_id
                .as_deref()
                .unwrap()
        );

        let mut journal = crate::Stage6MemoryJournalBackend::new();
        journal.append(&accepted).unwrap();
        journal.append(&dispatch).unwrap();
        journal
            .append_versioned(&crate::Stage6JournalRecordVersioned::V3(outcome.clone()))
            .unwrap();
        journal.append(&finalized).unwrap();
        let replay = crate::Stage6MixedReplayEngineV2::replay(journal.versioned_records()).unwrap();
        let request = replay
            .requests()
            .iter()
            .find(|request| request.strategy_request_id() == command.request_id)
            .unwrap();
        assert_eq!(request.last_unique_sequence(), 4);
        assert_eq!(
            request.last_unique_record_id(),
            finalized.journal_record_id()
        );
        assert_eq!(
            request.known_broker_order_id(),
            Some(plan.evidence.broker_order_id())
        );
        assert_eq!(
            request.final_disposition(),
            Some(crate::Stage6RequestFinalDispositionV1::Completed)
        );
        assert_eq!(replay.p1d3_outcome_records(), &[outcome]);
    }
}
