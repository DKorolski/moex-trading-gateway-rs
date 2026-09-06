//! Stage 8B-P1-d2 deterministic Market feedback source.
//!
//! The module projects one durably finalized P1-d1 Market fill into the
//! already accepted Stage 5G ACK and order/position state machines.  It owns
//! no Redis client, FINAM transport, wall clock or broker-dispatch handle.

use broker_core::command::CommandAckStatus;
use broker_core::{
    BrokerCommand, BrokerOrderLifecycle, BrokerOrderSnapshot, BrokerPositionSnapshot,
    BrokerTradeSnapshot, BrokerTruthSnapshot, CommandAck, HybridRuntimeAttribution, OrderSide,
    OrderStatus, OrderType, StrategyRequestId, TimeInForce,
};
use chrono::{DateTime, TimeZone, Utc};
use rust_decimal::{Decimal, RoundingStrategy};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::stage5g_mock_ack::{
    apply_stage5g_mock_ack, attach_stage5g_mock_ack_session, Stage5gMockAckEvent,
    Stage5gMockAckSessionInput, Stage5gMockAckTransition, Stage5gMockIntentAction,
    Stage5gMockIntentBinding, Stage5gMockPlaceKind,
};
use crate::stage5g_order_position::{
    apply_stage8b_p1d2_restart_truth, attach_stage5g_order_position_session,
    stage5g_integral_lot_decimal, Stage5gOrderPositionSession, Stage5gOrderPositionState,
};
use crate::stage5g_p1_semantic::Stage5gP1SemanticCommitProjectionV1;
use crate::stage8b_p1d1_paper_provider::Stage8bP1d1MarketOutcomeEvidence;
use crate::{
    export_stage5g_clean_restart, restore_stage5g_clean_restart, HybridIntradayRuntimeStrategy,
    Stage5cSettledPaperStrategy, Stage5gCleanRestartExportInput, Stage5gCleanRestartSource,
    Stage5gCleanRestartedCapability, Stage5gLifecycleCommitmentKey, Stage6DurableActionKind,
    Stage6RequestFinalDispositionV1, Stage7bFinalizedRequestFacts,
};

pub const STAGE8B_P1D2_MARKET_FEEDBACK_SCHEMA_VERSION: u16 = 1;
pub const STAGE8B_P1D2_AVG_PRICE_SCALE: u32 = 8;
const STAGE8B_P1D2_MARKET_FEEDBACK_DOMAIN: &str = "moex.stage8b.p1d2.market-feedback.v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Stage8bP1d2FeedbackPhase {
    AckCommitted,
    TruthCommitted,
}

/// Durable projection retained in both replacement Stage 5G packages.  The
/// ACK-stage copy contains the complete future truth, making truth replay
/// independent of process memory while still forbidding its application
/// before the ACK-stage seal has been reread.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Stage8bP1d2MarketFeedbackProjectionV1 {
    pub(crate) schema_version: u16,
    pub(crate) identity_domain: String,
    pub(crate) phase: Stage8bP1d2FeedbackPhase,
    pub(crate) source_m10_redis_id: String,
    pub(crate) source_m10_semantic_id_sha256: String,
    pub(crate) source_m10_payload_sha256: String,
    pub(crate) canonical_command_sha256: String,
    pub(crate) p1d1_execution_bar_semantic_id_sha256: String,
    pub(crate) p1d1_execution_bar_payload_sha256: String,
    pub(crate) stage6_final_record_id: String,
    pub(crate) stage6_final_sequence: u64,
    pub(crate) request_id: StrategyRequestId,
    pub(crate) intent_class: String,
    pub(crate) expected_attribution: HybridRuntimeAttribution,
    pub(crate) seq_ack: u64,
    pub(crate) seq_truth: u64,
    pub(crate) source_ts_utc_ms: i64,
    pub(crate) receipt_ts_utc_ms: i64,
    pub(crate) ack: CommandAck,
    pub(crate) truth: BrokerTruthSnapshot,
    pub(crate) order_projection_sha256: String,
    pub(crate) trade_projection_sha256: String,
    pub(crate) position_projection_sha256: String,
    pub(crate) p1d1_outcome_sha256: String,
    pub(crate) stage6_report_sha256: String,
    pub(crate) feedback_projection_sha256: String,
}

/// Redacted content bound by the authenticated final S_truth package.  This
/// value is evidence only: it grants no ACK, truth, source-resolution or
/// writer capability.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Stage8bP1d2FeedbackAuditCoreV1 {
    pub schema_version: u16,
    pub request_id: StrategyRequestId,
    pub p1d1_outcome_sha256: String,
    pub stage6_report_sha256: String,
    pub stage7_final_record_id: String,
    pub stage7_final_sequence: u64,
    pub source_ts_utc_ms: i64,
    pub receipt_ts_utc_ms: i64,
    pub seq_ack: u64,
    pub seq_truth: u64,
    pub order_projection_sha256: String,
    pub trade_projection_sha256: String,
    pub position_projection_sha256: String,
    pub feedback_projection_sha256: String,
}

/// Opaque linear input minted only after Stage 6 outcome persistence and
/// Stage 7 RequestFinalized reconstruction have succeeded.
pub(crate) struct Stage8bP1d2FinalizedMarketFeedbackInput {
    projection: Stage8bP1d2MarketFeedbackProjectionV1,
}

impl Stage8bP1d2FinalizedMarketFeedbackInput {
    pub(crate) fn into_projection(self) -> Stage8bP1d2MarketFeedbackProjectionV1 {
        self.projection
    }
}

/// ACK-stage source consumed by the clean-restart exporter.  Public type with
/// private fields is required only because the source enum is public; no
/// external constructor exists.
pub struct Stage8bP1d2AckRestartSource {
    session: Stage5gOrderPositionSession,
    feedback: Stage8bP1d2MarketFeedbackProjectionV1,
}

/// Truth-stage source consumed by the clean-restart exporter after the exact
/// S_ack slot supplied the reserved truth sequence.
pub struct Stage8bP1d2TruthRestartSource {
    runtime: HybridIntradayRuntimeStrategy,
    state: Stage5gOrderPositionState,
    feedback: Stage8bP1d2MarketFeedbackProjectionV1,
}

pub(crate) struct Stage8bP1d2AckStageResult {
    pub(crate) restored: Stage5gCleanRestartedCapability,
    pub(crate) restart_package: Vec<u8>,
}

pub(crate) struct Stage8bP1d2TruthStageResult {
    pub(crate) restored: Stage5gCleanRestartedCapability,
    pub(crate) restart_package: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Stage8bP1d2MarketFeedbackError {
    #[error("finalized Market feedback authority is inconsistent")]
    FinalizedAuthorityMismatch,
    #[error("P1-d2 chronology is invalid")]
    InvalidChronology,
    #[error("P1-d2 decimal authority is invalid")]
    InvalidDecimalAuthority,
    #[error("P1-d2 average-price arithmetic failed")]
    AveragePriceArithmetic,
    #[error("P1-d2 sequence authority is invalid")]
    InvalidSequenceAuthority,
    #[error("Stage 5G ACK application failed")]
    AckApplicationFailed,
    #[error("Stage 5G order/position attachment failed")]
    OrderPositionAttachmentFailed,
    #[error("Stage 5G truth application failed")]
    TruthApplicationFailed,
    #[error("Stage 5G replacement package failed")]
    RestartPackageFailed,
}

pub(crate) fn mint_stage8b_p1d2_finalized_market_feedback(
    p1: Stage5gP1SemanticCommitProjectionV1,
    facts: &Stage7bFinalizedRequestFacts,
    outcome: Stage8bP1d1MarketOutcomeEvidence,
) -> Result<Stage8bP1d2FinalizedMarketFeedbackInput, Stage8bP1d2MarketFeedbackError> {
    mint_finalized_market_feedback(p1, facts, outcome, Decimal::ZERO, None, true)
}

pub(crate) fn mint_stage8b_p1d4_finalized_market_feedback(
    p1: Stage5gP1SemanticCommitProjectionV1,
    facts: &Stage7bFinalizedRequestFacts,
    outcome: Stage8bP1d1MarketOutcomeEvidence,
    pre_position_qty: Decimal,
    pre_position_avg_price: Option<Decimal>,
) -> Result<Stage8bP1d2FinalizedMarketFeedbackInput, Stage8bP1d2MarketFeedbackError> {
    mint_finalized_market_feedback(
        p1,
        facts,
        outcome,
        pre_position_qty,
        pre_position_avg_price,
        false,
    )
}

fn mint_finalized_market_feedback(
    p1: Stage5gP1SemanticCommitProjectionV1,
    facts: &Stage7bFinalizedRequestFacts,
    outcome: Stage8bP1d1MarketOutcomeEvidence,
    pre_position_qty: Decimal,
    pre_position_avg_price: Option<Decimal>,
    require_first_flat_state: bool,
) -> Result<Stage8bP1d2FinalizedMarketFeedbackInput, Stage8bP1d2MarketFeedbackError> {
    if !p1.validate() || p1.intent_count != 1 {
        return Err(Stage8bP1d2MarketFeedbackError::FinalizedAuthorityMismatch);
    }
    let request_id = p1
        .request_id
        .ok_or(Stage8bP1d2MarketFeedbackError::FinalizedAuthorityMismatch)?;
    let command = p1
        .canonical_command
        .as_ref()
        .ok_or(Stage8bP1d2MarketFeedbackError::FinalizedAuthorityMismatch)?;
    let place = match command {
        BrokerCommand::PlaceOrder(place)
            if place.order_type == OrderType::Market
                && place.time_in_force == TimeInForce::Day
                && place.limit_price.is_none()
                && place.ttl_ms.is_none() =>
        {
            place
        }
        _ => return Err(Stage8bP1d2MarketFeedbackError::FinalizedAuthorityMismatch),
    };
    let source = p1
        .source_intent
        .as_ref()
        .ok_or(Stage8bP1d2MarketFeedbackError::FinalizedAuthorityMismatch)?;
    let attribution = p1
        .expected_attribution
        .as_ref()
        .ok_or(Stage8bP1d2MarketFeedbackError::FinalizedAuthorityMismatch)?;
    let command_sha256 = p1
        .canonical_command_sha256
        .as_ref()
        .ok_or(Stage8bP1d2MarketFeedbackError::FinalizedAuthorityMismatch)?;

    let exact_qty = place.qty.serialize();
    let p1d1_outcome_sha256 = p1d1_outcome_sha256(&outcome);
    let stage6_report_sha256 = stage6_report_sha256(facts);
    let durable_identity = facts.durable_request_identity();
    let durable_trade_matches = facts.broker_trade_ids().len() == 1
        && facts.broker_trade_ids()[0] == outcome.broker_trade_id;
    if request_id != outcome.strategy_request_id
        || request_id != facts.strategy_request_id()
        || command_sha256 != &outcome.canonical_command_sha256
        || &outcome.accepted_command_payload_sha256 != facts.canonical_command_sha256()
        || durable_identity.action() != Stage6DurableActionKind::Place
        || facts.final_disposition() != Stage6RequestFinalDispositionV1::Completed
        || facts.durable_client_order_id() != &place.client_order_id
        || durable_identity.account_id() != &place.account_id
        || durable_identity.instrument() != &place.instrument
        || durable_identity.attribution() != attribution
        || source.request_id != request_id
        || source.expected_attribution.as_ref() != Some(attribution)
        || source.base_action != crate::stage5c_paper_host::Stage5gSourceBaseAction::Market
        || source
            .target_qty
            .and_then(stage5g_integral_lot_decimal)
            .map(|v| v.serialize())
            != Some(exact_qty)
        || outcome.fill_qty.serialize() != exact_qty
        || facts.broker_order_id() != Some(&outcome.broker_order_id)
        || !durable_trade_matches
        || place.qty <= Decimal::ZERO
        || outcome.fill_price <= Decimal::ZERO
    {
        return Err(Stage8bP1d2MarketFeedbackError::FinalizedAuthorityMismatch);
    }

    let source_ts = exact_millisecond_timestamp(outcome.fill_source_ts_utc_ms)?;
    let receipt_ts = exact_millisecond_timestamp(outcome.fill_received_ts_utc_ms)?;
    if outcome
        .fill_received_ts_utc_ms
        .checked_sub(outcome.fill_source_ts_utc_ms)
        != Some(600_000)
        || outcome.fill_source_ts_utc_ms.to_string() + "-0" != p1.m10_redis_id
    {
        return Err(Stage8bP1d2MarketFeedbackError::InvalidChronology);
    }

    let q0 = stage5g_integral_lot_decimal(source.pre_position_qty)
        .ok_or(Stage8bP1d2MarketFeedbackError::InvalidDecimalAuthority)?;
    // Standalone P1-d2 remains the first/empty flat-state path. P1-d4 names
    // an independently authenticated position basis from the existing P1-d3
    // book and must match the callback's pre-position exactly.
    if q0.serialize() != pre_position_qty.serialize()
        || (require_first_flat_state && q0 != Decimal::ZERO)
        || (q0 == Decimal::ZERO && pre_position_avg_price.is_some())
        || (q0 != Decimal::ZERO && pre_position_avg_price.is_none())
    {
        return Err(Stage8bP1d2MarketFeedbackError::InvalidDecimalAuthority);
    }
    let signed_fill = match place.side {
        OrderSide::Buy => place.qty,
        OrderSide::Sell => Decimal::ZERO
            .checked_sub(place.qty)
            .ok_or(Stage8bP1d2MarketFeedbackError::AveragePriceArithmetic)?,
    };
    let (q1, avg_price) =
        resulting_position(q0, pre_position_avg_price, signed_fill, outcome.fill_price)?;
    let order = BrokerOrderSnapshot {
        account_id: place.account_id.clone(),
        broker_order_id: Some(outcome.broker_order_id.clone()),
        client_order_id: Some(place.client_order_id.clone()),
        instrument: place.instrument.clone(),
        side: place.side,
        order_type: OrderType::Market,
        time_in_force: Some(TimeInForce::Day),
        status: OrderStatus::Filled,
        lifecycle: BrokerOrderLifecycle::Terminal,
        qty: place.qty,
        filled_qty: place.qty,
        remaining_qty: Some(Decimal::ZERO),
        limit_price: None,
        broker_asset_id: None,
        board: None,
        expiration_date: None,
        source_ts: Some(source_ts),
        received_ts: receipt_ts,
    };
    let trade = BrokerTradeSnapshot {
        account_id: place.account_id.clone(),
        broker_trade_id: outcome.broker_trade_id,
        broker_order_id: Some(outcome.broker_order_id.clone()),
        client_order_id: Some(place.client_order_id.clone()),
        instrument: place.instrument.clone(),
        side: place.side,
        qty: place.qty,
        price: outcome.fill_price,
        gross_amount: None,
        commission: Some(Decimal::ZERO),
        broker_asset_id: None,
        board: None,
        expiration_date: None,
        source_ts,
        received_ts: receipt_ts,
    };
    let position = BrokerPositionSnapshot {
        account_id: place.account_id.clone(),
        instrument: place.instrument.clone(),
        qty: q1,
        avg_price,
        unrealized_pnl: None,
        source_ts: Some(source_ts),
        received_ts: receipt_ts,
    };
    let ack = CommandAck {
        request_id,
        client_order_id: Some(place.client_order_id.clone()),
        broker_order_id: Some(outcome.broker_order_id),
        status: CommandAckStatus::Accepted,
        reason: None,
        received_ts: receipt_ts,
    };
    let truth = BrokerTruthSnapshot {
        account_id: place.account_id.clone(),
        orders: vec![order.clone()],
        positions: vec![position.clone()],
        cash: None,
        trades: vec![trade.clone()],
        instruments: Vec::new(),
        received_ts: receipt_ts,
    };
    let projection = Stage8bP1d2MarketFeedbackProjectionV1 {
        schema_version: STAGE8B_P1D2_MARKET_FEEDBACK_SCHEMA_VERSION,
        identity_domain: STAGE8B_P1D2_MARKET_FEEDBACK_DOMAIN.to_string(),
        phase: Stage8bP1d2FeedbackPhase::AckCommitted,
        source_m10_redis_id: p1.m10_redis_id,
        source_m10_semantic_id_sha256: p1.m10_semantic_id_sha256,
        source_m10_payload_sha256: p1.m10_payload_sha256,
        canonical_command_sha256: command_sha256.clone(),
        p1d1_execution_bar_semantic_id_sha256: outcome.execution_bar_semantic_id_sha256,
        p1d1_execution_bar_payload_sha256: outcome.execution_bar_payload_sha256,
        stage6_final_record_id: facts.final_record_id().as_str().to_string(),
        stage6_final_sequence: facts.final_sequence(),
        request_id,
        intent_class: encode_intent_class(source.intent_class).to_string(),
        expected_attribution: attribution.clone(),
        // The complete content is validated before effects. The sole linear
        // Stage5G ACK owner allocates and binds the sequence pair immediately
        // before ACK application.
        seq_ack: 0,
        seq_truth: 0,
        source_ts_utc_ms: outcome.fill_source_ts_utc_ms,
        receipt_ts_utc_ms: outcome.fill_received_ts_utc_ms,
        ack,
        truth,
        order_projection_sha256: exact_order_sha256(&order),
        trade_projection_sha256: exact_trade_sha256(&trade),
        position_projection_sha256: exact_position_sha256(&position),
        p1d1_outcome_sha256,
        stage6_report_sha256,
        feedback_projection_sha256: String::new(),
    };
    if !projection.validate_content() {
        return Err(Stage8bP1d2MarketFeedbackError::FinalizedAuthorityMismatch);
    }
    Ok(Stage8bP1d2FinalizedMarketFeedbackInput { projection })
}

pub(crate) fn apply_stage8b_p1d2_ack_stage(
    settled: Stage5cSettledPaperStrategy,
    finalized: Stage8bP1d2FinalizedMarketFeedbackInput,
    export_input: Stage5gCleanRestartExportInput,
    commitment_key: &Stage5gLifecycleCommitmentKey,
) -> Result<Stage8bP1d2AckStageResult, Stage8bP1d2MarketFeedbackError> {
    let mut projection = finalized.projection;
    if !projection.validate_content()
        || projection.seq_ack != 0
        || projection.seq_truth != 0
        || !projection.feedback_projection_sha256.is_empty()
        || projection.phase != Stage8bP1d2FeedbackPhase::AckCommitted
    {
        return Err(Stage8bP1d2MarketFeedbackError::FinalizedAuthorityMismatch);
    }
    let action = Stage5gMockIntentAction::Place {
        place_kind: Stage5gMockPlaceKind::Market,
    };
    let side = projection
        .truth
        .orders
        .first()
        .map(|order| match order.side {
            OrderSide::Buy => crate::BrokerNeutralOrderSide::Buy,
            OrderSide::Sell => crate::BrokerNeutralOrderSide::Sell,
        })
        .ok_or(Stage8bP1d2MarketFeedbackError::FinalizedAuthorityMismatch)?;
    let expires = projection
        .receipt_ts_utc_ms
        .div_euclid(1_000)
        .checked_add(1)
        .ok_or(Stage8bP1d2MarketFeedbackError::InvalidChronology)?;
    let session = attach_stage5g_mock_ack_session(
        settled,
        Stage5gMockAckSessionInput {
            intent_bindings: vec![Stage5gMockIntentBinding {
                request_id: projection.request_id,
                intent_class: decode_intent_class(&projection.intent_class)
                    .ok_or(Stage8bP1d2MarketFeedbackError::FinalizedAuthorityMismatch)?,
                action: action.clone(),
                side: Some(side),
            }],
            lifecycle_expires_at_ts_utc: expires,
        },
    )
    .map_err(|_| Stage8bP1d2MarketFeedbackError::AckApplicationFailed)?;
    let (session, seq_ack, seq_truth) = session
        .stage8b_p1d2_allocate_sequence_pair()
        .ok_or(Stage8bP1d2MarketFeedbackError::InvalidSequenceAuthority)?;
    projection.seq_ack = seq_ack;
    projection.seq_truth = seq_truth;
    projection.feedback_projection_sha256 = feedback_projection_sha256(&projection);
    if !projection.validate() {
        return Err(Stage8bP1d2MarketFeedbackError::FinalizedAuthorityMismatch);
    }
    stage8b_p1d2_test_record_sequence_pair_before_crash(seq_ack, seq_truth);
    stage8b_p1d2_test_crash_barrier("p1d2-after-sequence-pair-before-ack");
    let resolved = match apply_stage5g_mock_ack(
        session,
        Stage5gMockAckEvent {
            total_sequence: projection.seq_ack,
            intent_request_id: projection.request_id,
            account_id: projection.truth.account_id.clone(),
            instrument: projection.truth.orders[0].instrument.clone(),
            action,
            side: Some(side),
            ack: projection.ack.clone(),
        },
    )
    .map_err(|_| Stage8bP1d2MarketFeedbackError::AckApplicationFailed)?
    {
        Stage5gMockAckTransition::Resolved(resolved) => resolved,
        Stage5gMockAckTransition::Awaiting(_) => {
            return Err(Stage8bP1d2MarketFeedbackError::AckApplicationFailed)
        }
    };
    let order_position = attach_stage5g_order_position_session(resolved)
        .map_err(|_| Stage8bP1d2MarketFeedbackError::OrderPositionAttachmentFailed)?
        .stage8b_p1d2_mark_ack_frontier(projection.receipt_ts_utc_ms)
        .ok_or(Stage8bP1d2MarketFeedbackError::OrderPositionAttachmentFailed)?;
    let recovered_truth_sequence =
        Stage5gOrderPositionSession::stage8b_p1d2_truth_sequence_from_ack(
            &order_position.stage5g_restart_state(),
            projection.request_id,
        )
        .ok_or(Stage8bP1d2MarketFeedbackError::InvalidSequenceAuthority)?;
    if recovered_truth_sequence != projection.seq_truth {
        return Err(Stage8bP1d2MarketFeedbackError::InvalidSequenceAuthority);
    }
    let fresh_runtime = order_position
        .stage5g_runtime_strategy()
        .stage5g_clean_reconstruction_candidate();
    let source = Stage8bP1d2AckRestartSource {
        session: order_position,
        feedback: projection,
    };
    let restart_package = export_stage5g_clean_restart(
        Stage5gCleanRestartSource::P1d2Ack(source),
        export_input,
        commitment_key,
    )
    .map_err(|_| Stage8bP1d2MarketFeedbackError::RestartPackageFailed)?;
    let restored = restore_stage5g_clean_restart(&restart_package, commitment_key, fresh_runtime)
        .map_err(|_| Stage8bP1d2MarketFeedbackError::RestartPackageFailed)?;
    restored
        .stage8b_p1d2_validate_ack_frontier()
        .map_err(|_| Stage8bP1d2MarketFeedbackError::RestartPackageFailed)?;
    Ok(Stage8bP1d2AckStageResult {
        restored,
        restart_package,
    })
}

pub(crate) fn apply_stage8b_p1d2_truth_stage(
    restored_ack: Stage5gCleanRestartedCapability,
    commitment_key: &Stage5gLifecycleCommitmentKey,
) -> Result<Stage8bP1d2TruthStageResult, Stage8bP1d2MarketFeedbackError> {
    let (runtime, state, mut feedback, export_input) = restored_ack
        .into_stage8b_p1d2_truth_parts()
        .map_err(|_| Stage8bP1d2MarketFeedbackError::RestartPackageFailed)?;
    let recovered_sequence = Stage5gOrderPositionSession::stage8b_p1d2_truth_sequence_from_ack(
        &state,
        feedback.request_id,
    )
    .ok_or(Stage8bP1d2MarketFeedbackError::InvalidSequenceAuthority)?;
    if recovered_sequence != feedback.seq_truth {
        return Err(Stage8bP1d2MarketFeedbackError::InvalidSequenceAuthority);
    }
    let state = apply_stage8b_p1d2_restart_truth(
        state,
        crate::Stage5gOrderPositionEvidence {
            total_sequence: recovered_sequence,
            request_id: feedback.request_id,
            broker_truth: feedback.truth.clone(),
            order_attribution: Some(feedback.expected_attribution.clone()),
        },
    )
    .map_err(|_| Stage8bP1d2MarketFeedbackError::TruthApplicationFailed)?;
    feedback.phase = Stage8bP1d2FeedbackPhase::TruthCommitted;
    feedback.feedback_projection_sha256 = feedback_projection_sha256(&feedback);
    let fresh_runtime = runtime.stage5g_clean_reconstruction_candidate();
    let source = Stage8bP1d2TruthRestartSource {
        runtime,
        state,
        feedback,
    };
    let restart_package = export_stage5g_clean_restart(
        Stage5gCleanRestartSource::P1d2Truth(source),
        export_input,
        commitment_key,
    )
    .map_err(|_| Stage8bP1d2MarketFeedbackError::RestartPackageFailed)?;
    let restored = restore_stage5g_clean_restart(&restart_package, commitment_key, fresh_runtime)
        .map_err(|_| Stage8bP1d2MarketFeedbackError::RestartPackageFailed)?;
    restored
        .stage8b_p1d2_validate_truth_frontier()
        .map_err(|_| Stage8bP1d2MarketFeedbackError::RestartPackageFailed)?;
    Ok(Stage8bP1d2TruthStageResult {
        restored,
        restart_package,
    })
}

impl Stage8bP1d2MarketFeedbackProjectionV1 {
    fn validate_content(&self) -> bool {
        let Some(order) = self.truth.orders.first() else {
            return false;
        };
        let Some(trade) = self.truth.trades.first() else {
            return false;
        };
        let Some(position) = self.truth.positions.first() else {
            return false;
        };
        self.schema_version == STAGE8B_P1D2_MARKET_FEEDBACK_SCHEMA_VERSION
            && self.identity_domain == STAGE8B_P1D2_MARKET_FEEDBACK_DOMAIN
            && self.truth.orders.len() == 1
            && self.truth.trades.len() == 1
            && self.truth.positions.len() == 1
            && self.truth.cash.is_none()
            && self.truth.instruments.is_empty()
            && decode_intent_class(&self.intent_class).is_some()
            && self.ack.request_id == self.request_id
            && self.ack.status == CommandAckStatus::Accepted
            && self.ack.reason.is_none()
            && self.ack.received_ts.timestamp_millis() == self.receipt_ts_utc_ms
            && self.truth.received_ts.timestamp_millis() == self.receipt_ts_utc_ms
            && order.source_ts.map(|ts| ts.timestamp_millis()) == Some(self.source_ts_utc_ms)
            && order.received_ts.timestamp_millis() == self.receipt_ts_utc_ms
            && trade.source_ts.timestamp_millis() == self.source_ts_utc_ms
            && trade.received_ts.timestamp_millis() == self.receipt_ts_utc_ms
            && position.source_ts.map(|ts| ts.timestamp_millis()) == Some(self.source_ts_utc_ms)
            && position.received_ts.timestamp_millis() == self.receipt_ts_utc_ms
            && self.receipt_ts_utc_ms.checked_sub(self.source_ts_utc_ms) == Some(600_000)
            && order.status == OrderStatus::Filled
            && order.lifecycle == BrokerOrderLifecycle::Terminal
            && order.order_type == OrderType::Market
            && order.time_in_force == Some(TimeInForce::Day)
            && order.remaining_qty.map(|v| v.serialize()) == Some(Decimal::ZERO.serialize())
            && order.qty.serialize() == order.filled_qty.serialize()
            && trade.qty.serialize() == order.qty.serialize()
            && trade.price > Decimal::ZERO
            && trade.gross_amount.is_none()
            && trade.commission.map(|v| v.serialize()) == Some(Decimal::ZERO.serialize())
            && order.broker_order_id == trade.broker_order_id
            && order.broker_order_id == self.ack.broker_order_id
            && order.client_order_id == trade.client_order_id
            && order.client_order_id == self.ack.client_order_id
            && order.account_id == self.truth.account_id
            && trade.account_id == self.truth.account_id
            && position.account_id == self.truth.account_id
            && order.instrument == trade.instrument
            && order.instrument == position.instrument
            && order.side == trade.side
            && position
                .avg_price
                .as_ref()
                .map_or(position.qty == Decimal::ZERO, |avg| {
                    position.qty != Decimal::ZERO && avg.scale() == STAGE8B_P1D2_AVG_PRICE_SCALE
                })
            && is_sha256(&self.source_m10_semantic_id_sha256)
            && is_sha256(&self.source_m10_payload_sha256)
            && is_sha256(&self.canonical_command_sha256)
            && is_sha256(&self.p1d1_execution_bar_semantic_id_sha256)
            && is_sha256(&self.p1d1_execution_bar_payload_sha256)
            && self.order_projection_sha256 == exact_order_sha256(order)
            && self.trade_projection_sha256 == exact_trade_sha256(trade)
            && self.position_projection_sha256 == exact_position_sha256(position)
            && is_sha256(&self.p1d1_outcome_sha256)
            && is_sha256(&self.stage6_report_sha256)
    }

    pub(crate) fn validate(&self) -> bool {
        self.validate_content()
            && self.seq_ack > 0
            && self.seq_ack.checked_add(1) == Some(self.seq_truth)
            && self.feedback_projection_sha256 == feedback_projection_sha256(self)
    }

    pub(crate) fn phase(&self) -> Stage8bP1d2FeedbackPhase {
        self.phase
    }

    pub(crate) fn expected_attribution(&self) -> &HybridRuntimeAttribution {
        &self.expected_attribution
    }

    pub(crate) fn request_id(&self) -> StrategyRequestId {
        self.request_id
    }

    pub(crate) fn seq_ack(&self) -> u64 {
        self.seq_ack
    }

    pub(crate) fn seq_truth(&self) -> u64 {
        self.seq_truth
    }

    pub(crate) fn receipt_ts_utc_ms(&self) -> i64 {
        self.receipt_ts_utc_ms
    }

    pub(crate) fn source_m10_binding(&self) -> (&str, &str, &str) {
        (
            &self.source_m10_redis_id,
            &self.source_m10_semantic_id_sha256,
            &self.source_m10_payload_sha256,
        )
    }

    pub(crate) fn audit_core(&self) -> Option<Stage8bP1d2FeedbackAuditCoreV1> {
        self.validate().then(|| Stage8bP1d2FeedbackAuditCoreV1 {
            schema_version: STAGE8B_P1D2_MARKET_FEEDBACK_SCHEMA_VERSION,
            request_id: self.request_id,
            p1d1_outcome_sha256: self.p1d1_outcome_sha256.clone(),
            stage6_report_sha256: self.stage6_report_sha256.clone(),
            stage7_final_record_id: self.stage6_final_record_id.clone(),
            stage7_final_sequence: self.stage6_final_sequence,
            source_ts_utc_ms: self.source_ts_utc_ms,
            receipt_ts_utc_ms: self.receipt_ts_utc_ms,
            seq_ack: self.seq_ack,
            seq_truth: self.seq_truth,
            order_projection_sha256: self.order_projection_sha256.clone(),
            trade_projection_sha256: self.trade_projection_sha256.clone(),
            position_projection_sha256: self.position_projection_sha256.clone(),
            feedback_projection_sha256: self.feedback_projection_sha256.clone(),
        })
    }
}

#[cfg(any(test, feature = "stage5g-artifact-fixtures"))]
pub(crate) fn stage8b_p1d2_test_record_sequence_pair_before_crash(seq_ack: u64, seq_truth: u64) {
    if std::env::var("STAGE8B_P1_TEST_CRASH_PHASE").as_deref()
        != Ok("p1d2-after-sequence-pair-before-ack")
    {
        return;
    }
    let marker = std::env::var_os("STAGE8B_P1_TEST_SEQUENCE_PAIR_MARKER")
        .expect("sequence-pair crash child requires a pair marker path");
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(marker)
        .expect("sequence-pair marker must be created once");
    std::io::Write::write_all(
        &mut file,
        format!("seq_ack={seq_ack}\nseq_truth={seq_truth}\n").as_bytes(),
    )
    .expect("sequence-pair marker must be writable");
    file.sync_all()
        .expect("sequence-pair marker must be durable before crash barrier");
}

#[cfg(not(any(test, feature = "stage5g-artifact-fixtures")))]
#[inline(always)]
pub(crate) fn stage8b_p1d2_test_record_sequence_pair_before_crash(_: u64, _: u64) {}

#[cfg(any(test, feature = "stage5g-artifact-fixtures"))]
pub(crate) fn stage8b_p1d2_test_crash_barrier(phase: &str) {
    if std::env::var("STAGE8B_P1_TEST_CRASH_PHASE").as_deref() != Ok(phase) {
        return;
    }
    let marker = std::env::var_os("STAGE8B_P1_TEST_CRASH_MARKER")
        .expect("P1-d2 crash child requires a marker path");
    std::fs::write(marker, phase.as_bytes()).expect("P1-d2 crash marker must be writable");
    loop {
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}

#[cfg(not(any(test, feature = "stage5g-artifact-fixtures")))]
#[inline(always)]
pub(crate) fn stage8b_p1d2_test_crash_barrier(_: &str) {}

impl Stage8bP1d2AckRestartSource {
    pub(crate) fn runtime(&self) -> &HybridIntradayRuntimeStrategy {
        self.session.stage5g_runtime_strategy()
    }
    pub(crate) fn binding(
        &self,
    ) -> (
        &str,
        &broker_core::BrokerAccountId,
        &broker_core::InstrumentId,
    ) {
        self.session.stage5g_restart_binding()
    }
    pub(crate) fn summary(&self) -> crate::Stage5gOrderPositionSummary {
        self.session.summary()
    }
    pub(crate) fn checkpoint(&self) -> crate::Stage5gTimerCheckpointEnvelope {
        self.session.stage5g_restart_checkpoint()
    }
    pub(crate) fn state(&self) -> Stage5gOrderPositionState {
        self.session.stage5g_restart_state()
    }
    pub(crate) fn feedback(&self) -> &Stage8bP1d2MarketFeedbackProjectionV1 {
        &self.feedback
    }
}

impl Stage8bP1d2TruthRestartSource {
    pub(crate) fn runtime(&self) -> &HybridIntradayRuntimeStrategy {
        &self.runtime
    }
    pub(crate) fn binding(
        &self,
    ) -> (
        &str,
        &broker_core::BrokerAccountId,
        &broker_core::InstrumentId,
    ) {
        Stage5gOrderPositionSession::stage5g_restart_state_binding(&self.state)
    }
    pub(crate) fn summary(&self) -> crate::Stage5gOrderPositionSummary {
        Stage5gOrderPositionSession::stage5g_restart_summary_from_state(&self.state, 0)
    }
    pub(crate) fn checkpoint(&self) -> crate::Stage5gTimerCheckpointEnvelope {
        Stage5gOrderPositionSession::stage5g_restart_checkpoint_from_state(&self.state)
    }
    pub(crate) fn state(&self) -> Stage5gOrderPositionState {
        self.state.clone()
    }
    pub(crate) fn feedback(&self) -> &Stage8bP1d2MarketFeedbackProjectionV1 {
        &self.feedback
    }
}

pub(crate) fn resulting_position(
    q0: Decimal,
    a0: Option<Decimal>,
    delta: Decimal,
    price: Decimal,
) -> Result<(Decimal, Option<Decimal>), Stage8bP1d2MarketFeedbackError> {
    if delta == Decimal::ZERO || price <= Decimal::ZERO {
        return Err(Stage8bP1d2MarketFeedbackError::InvalidDecimalAuthority);
    }
    if (q0 == Decimal::ZERO && a0.is_some()) || (q0 != Decimal::ZERO && a0.is_none()) {
        return Err(Stage8bP1d2MarketFeedbackError::InvalidDecimalAuthority);
    }
    let q1 = q0
        .checked_add(delta)
        .ok_or(Stage8bP1d2MarketFeedbackError::AveragePriceArithmetic)?;
    if q1 == Decimal::ZERO {
        return Ok((q1, None));
    }
    let candidate = if q0 == Decimal::ZERO {
        price
    } else if q0.is_sign_positive() == delta.is_sign_positive() {
        let prior = q0
            .abs()
            .checked_mul(a0.ok_or(Stage8bP1d2MarketFeedbackError::InvalidDecimalAuthority)?)
            .ok_or(Stage8bP1d2MarketFeedbackError::AveragePriceArithmetic)?;
        let fill = delta
            .abs()
            .checked_mul(price)
            .ok_or(Stage8bP1d2MarketFeedbackError::AveragePriceArithmetic)?;
        prior
            .checked_add(fill)
            .and_then(|sum| sum.checked_div(q1.abs()))
            .ok_or(Stage8bP1d2MarketFeedbackError::AveragePriceArithmetic)?
    } else if q1.is_sign_positive() == q0.is_sign_positive() {
        a0.ok_or(Stage8bP1d2MarketFeedbackError::InvalidDecimalAuthority)?
    } else {
        price
    };
    let mut canonical = candidate.round_dp_with_strategy(
        STAGE8B_P1D2_AVG_PRICE_SCALE,
        RoundingStrategy::MidpointNearestEven,
    );
    canonical.rescale(STAGE8B_P1D2_AVG_PRICE_SCALE);
    if canonical.scale() != STAGE8B_P1D2_AVG_PRICE_SCALE || canonical <= Decimal::ZERO {
        return Err(Stage8bP1d2MarketFeedbackError::AveragePriceArithmetic);
    }
    Ok((q1, Some(canonical)))
}

fn exact_millisecond_timestamp(
    value: i64,
) -> Result<DateTime<Utc>, Stage8bP1d2MarketFeedbackError> {
    Utc.timestamp_millis_opt(value)
        .single()
        .filter(|ts| ts.timestamp_millis() == value)
        .ok_or(Stage8bP1d2MarketFeedbackError::InvalidChronology)
}

#[derive(Serialize)]
struct ExactDecimal([u8; 16]);

fn exact_order_sha256(order: &BrokerOrderSnapshot) -> String {
    sha256(&(
        "moex.stage8b.p1d2.order.v1",
        order,
        ExactDecimal(order.qty.serialize()),
        ExactDecimal(order.filled_qty.serialize()),
        order.remaining_qty.map(|v| ExactDecimal(v.serialize())),
    ))
}

fn exact_trade_sha256(trade: &BrokerTradeSnapshot) -> String {
    sha256(&(
        "moex.stage8b.p1d2.trade.v1",
        trade,
        ExactDecimal(trade.qty.serialize()),
        ExactDecimal(trade.price.serialize()),
        trade.commission.map(|v| ExactDecimal(v.serialize())),
    ))
}

fn exact_position_sha256(position: &BrokerPositionSnapshot) -> String {
    sha256(&(
        "moex.stage8b.p1d2.position.v1",
        position,
        ExactDecimal(position.qty.serialize()),
        position.avg_price.map(|v| ExactDecimal(v.serialize())),
    ))
}

fn p1d1_outcome_sha256(outcome: &Stage8bP1d1MarketOutcomeEvidence) -> String {
    sha256(&(
        "moex.stage8b.p1d1.market-outcome.audit.v1",
        outcome.strategy_request_id,
        &outcome.canonical_command_sha256,
        &outcome.accepted_command_payload_sha256,
        &outcome.execution_bar_semantic_id_sha256,
        &outcome.execution_bar_payload_sha256,
        ExactDecimal(outcome.fill_price.serialize()),
        ExactDecimal(outcome.fill_qty.serialize()),
        outcome.fill_source_ts_utc_ms,
        outcome.fill_received_ts_utc_ms,
        &outcome.broker_order_id,
        &outcome.broker_trade_id,
    ))
}

fn stage6_report_sha256(facts: &Stage7bFinalizedRequestFacts) -> String {
    #[derive(Serialize)]
    struct Report<'a> {
        domain: &'static str,
        durable_request_identity: &'a crate::Stage6DurableRequestIdentityV1,
        strategy_request_id: StrategyRequestId,
        durable_client_order_id: &'a broker_core::ClientOrderId,
        broker_order_id: Option<&'a broker_core::BrokerOrderId>,
        broker_trade_ids: &'a [broker_core::BrokerTradeId],
        canonical_command_sha256: &'a crate::Stage6Sha256Digest,
        final_disposition: Stage6RequestFinalDispositionV1,
        final_record_id: &'a crate::Stage6JournalRecordId,
        final_sequence: u64,
    }
    sha256(&Report {
        domain: "moex.stage8b.p1d2.stage6-finalized-report.audit.v1",
        durable_request_identity: facts.durable_request_identity(),
        strategy_request_id: facts.strategy_request_id(),
        durable_client_order_id: facts.durable_client_order_id(),
        broker_order_id: facts.broker_order_id(),
        broker_trade_ids: facts.broker_trade_ids(),
        canonical_command_sha256: facts.canonical_command_sha256(),
        final_disposition: facts.final_disposition(),
        final_record_id: facts.final_record_id(),
        final_sequence: facts.final_sequence(),
    })
}

pub(crate) fn feedback_projection_sha256(value: &Stage8bP1d2MarketFeedbackProjectionV1) -> String {
    #[derive(Serialize)]
    struct Projection<'a> {
        domain: &'static str,
        schema_version: u16,
        identity_domain: &'a str,
        phase: Stage8bP1d2FeedbackPhase,
        source_m10_redis_id: &'a str,
        source_m10_semantic_id_sha256: &'a str,
        source_m10_payload_sha256: &'a str,
        canonical_command_sha256: &'a str,
        p1d1_execution_bar_semantic_id_sha256: &'a str,
        p1d1_execution_bar_payload_sha256: &'a str,
        stage6_final_record_id: &'a str,
        stage6_final_sequence: u64,
        request_id: StrategyRequestId,
        intent_class: &'a str,
        expected_attribution: &'a HybridRuntimeAttribution,
        seq_ack: u64,
        seq_truth: u64,
        source_ts_utc_ms: i64,
        receipt_ts_utc_ms: i64,
        order_projection_sha256: &'a str,
        trade_projection_sha256: &'a str,
        position_projection_sha256: &'a str,
        p1d1_outcome_sha256: &'a str,
        stage6_report_sha256: &'a str,
    }
    sha256(&Projection {
        domain: STAGE8B_P1D2_MARKET_FEEDBACK_DOMAIN,
        schema_version: value.schema_version,
        identity_domain: &value.identity_domain,
        phase: value.phase,
        source_m10_redis_id: &value.source_m10_redis_id,
        source_m10_semantic_id_sha256: &value.source_m10_semantic_id_sha256,
        source_m10_payload_sha256: &value.source_m10_payload_sha256,
        canonical_command_sha256: &value.canonical_command_sha256,
        p1d1_execution_bar_semantic_id_sha256: &value.p1d1_execution_bar_semantic_id_sha256,
        p1d1_execution_bar_payload_sha256: &value.p1d1_execution_bar_payload_sha256,
        stage6_final_record_id: &value.stage6_final_record_id,
        stage6_final_sequence: value.stage6_final_sequence,
        request_id: value.request_id,
        intent_class: &value.intent_class,
        expected_attribution: &value.expected_attribution,
        seq_ack: value.seq_ack,
        seq_truth: value.seq_truth,
        source_ts_utc_ms: value.source_ts_utc_ms,
        receipt_ts_utc_ms: value.receipt_ts_utc_ms,
        order_projection_sha256: &value.order_projection_sha256,
        trade_projection_sha256: &value.trade_projection_sha256,
        position_projection_sha256: &value.position_projection_sha256,
        p1d1_outcome_sha256: &value.p1d1_outcome_sha256,
        stage6_report_sha256: &value.stage6_report_sha256,
    })
}

fn sha256<T: Serialize>(value: &T) -> String {
    let bytes = serde_json::to_vec(value).expect("P1-d2 canonical projection serializes");
    format!("{:x}", Sha256::digest(bytes))
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn encode_intent_class(value: crate::BrokerNeutralHybridIntentClass) -> &'static str {
    match value {
        crate::BrokerNeutralHybridIntentClass::Entry => "entry",
        crate::BrokerNeutralHybridIntentClass::Exit => "exit",
        crate::BrokerNeutralHybridIntentClass::CancelCleanup => "cancel_cleanup",
        crate::BrokerNeutralHybridIntentClass::ProtectiveRepair => "protective_repair",
    }
}

fn decode_intent_class(value: &str) -> Option<crate::BrokerNeutralHybridIntentClass> {
    match value {
        "entry" => Some(crate::BrokerNeutralHybridIntentClass::Entry),
        "exit" => Some(crate::BrokerNeutralHybridIntentClass::Exit),
        "cancel_cleanup" => Some(crate::BrokerNeutralHybridIntentClass::CancelCleanup),
        "protective_repair" => Some(crate::BrokerNeutralHybridIntentClass::ProtectiveRepair),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(value: i64, scale: u32) -> Decimal {
        Decimal::new(value, scale)
    }

    #[test]
    fn canonical_average_golden_vectors() {
        let cases = [
            (d(1, 0), Some(d(100, 0)), d(1, 0), d(102, 0), "101.00000000"),
            (
                d(1, 0),
                Some(d(100, 0)),
                d(2, 0),
                d(1005, 1),
                "100.33333333",
            ),
            (
                d(-1, 0),
                Some(d(100, 0)),
                d(-2, 0),
                d(1005, 1),
                "100.33333333",
            ),
            (d(1, 0), Some(d(100, 0)), d(-2, 0), d(995, 1), "99.50000000"),
        ];
        for (q0, a0, delta, price, expected) in cases {
            let (_, actual) = resulting_position(q0, a0, delta, price).unwrap();
            let actual = actual.unwrap();
            assert_eq!(actual.to_string(), expected);
            assert_eq!(actual.scale(), STAGE8B_P1D2_AVG_PRICE_SCALE);
        }
        assert_eq!(
            resulting_position(d(1, 0), Some(d(100, 0)), d(-1, 0), d(99, 0)).unwrap(),
            (Decimal::ZERO, None)
        );
    }

    #[test]
    fn nearest_even_ties_are_explicit() {
        let even = resulting_position(
            Decimal::ZERO,
            None,
            Decimal::ONE,
            Decimal::from_str_exact("1.000000005").unwrap(),
        )
        .unwrap()
        .1
        .unwrap();
        let odd = resulting_position(
            Decimal::ZERO,
            None,
            Decimal::ONE,
            Decimal::from_str_exact("1.000000015").unwrap(),
        )
        .unwrap()
        .1
        .unwrap();
        assert_eq!(even.to_string(), "1.00000000");
        assert_eq!(odd.to_string(), "1.00000002");
    }

    #[test]
    fn average_input_shape_fails_closed() {
        assert_eq!(
            resulting_position(Decimal::ONE, None, Decimal::ONE, Decimal::ONE),
            Err(Stage8bP1d2MarketFeedbackError::InvalidDecimalAuthority)
        );
        assert_eq!(
            resulting_position(
                Decimal::ZERO,
                Some(Decimal::ONE),
                Decimal::ONE,
                Decimal::ONE
            ),
            Err(Stage8bP1d2MarketFeedbackError::InvalidDecimalAuthority)
        );
    }
}
