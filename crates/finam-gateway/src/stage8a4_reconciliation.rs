//! Stage 8A-4 pure broker-truth admission and reconciliation reducer.
//!
//! The input capabilities are intentionally opaque and have no public
//! constructors. Stage 8A-4 implementation R1 can therefore be exercised by
//! canonical fixtures without opening a durable apply, retry or send path.
//!
//! ```compile_fail
//! use finam_gateway::Stage8a4DurableRequestContext;
//! fn clone_context(context: Stage8a4DurableRequestContext) {
//!     let _ = context.clone();
//! }
//! ```
//!
//! ```compile_fail
//! use finam_gateway::Stage8a4FreshTruthAdmission;
//! fn expose_truth(admission: Stage8a4FreshTruthAdmission) {
//!     let _ = admission.broker_truth();
//! }
//! ```
//!
//! ```compile_fail
//! use finam_gateway::Stage8a4ReconciliationDiagnostic;
//! fn retry(value: Stage8a4ReconciliationDiagnostic) {
//!     let _ = value.retry_authority();
//! }
//! ```

use std::collections::BTreeMap;

use broker_core::{
    BrokerAccountId, BrokerOrderId, BrokerOrderSnapshot, BrokerTradeSnapshot, BrokerTruthSnapshot,
    ClientOrderId, InstrumentId, OrderSide, OrderStatus, OrderType, Price, Quantity,
    StrategyRequestId, TimeInForce,
};
use chrono::{DateTime, Duration, Utc};
use rust_decimal::Decimal;
use serde::Serialize;
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage8a4ExactLifecycle {
    Working,
    TerminalFilled,
    TerminalRejected,
    TerminalCancelled,
    TerminalExpired,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Stage8a4FillEffect {
    Zero,
    Partial { filled_qty: Quantity },
    Full { filled_qty: Quantity },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage8a4OutcomeKind {
    ExactOrderState,
    Conflict,
    StillUnknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage8a4ReconciliationReason {
    ExactTier1ClientIdentity,
    ExactTier2BrokerIdentity,
    ExactTier3BoundShape,
    SourceIncomplete,
    SourceStale,
    SourceIdentityMismatch,
    InstrumentUnresolved,
    ExactLookupUnavailable,
    NoCandidate,
    MissingRequiredShape,
    MultipleCandidates,
    ExactIdentityDisagreement,
    OrderShapeContradiction,
    OrderQuantityContradiction,
    TradeIdentityConflict,
    TradeQuantityContradiction,
    UnknownOrderStatus,
}

/// Bounded redacted result. It contains semantic state and hashes, never raw
/// account, order, trade, request or instrument identity.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Stage8a4ReconciliationDiagnostic {
    pub outcome: Stage8a4OutcomeKind,
    pub reason: Stage8a4ReconciliationReason,
    pub lifecycle: Option<Stage8a4ExactLifecycle>,
    pub fill: Option<Stage8a4FillEffect>,
    pub selected_order_binding_sha256: Option<String>,
    pub trade_summary_binding_sha256: Option<String>,
    pub account_active_orders_count: usize,
    pub target_active_orders_count: usize,
    pub matching_trade_count: usize,
    pub semantic_binding_sha256: String,
    pub retry_authorized: bool,
    pub send_authorized: bool,
}

/// Opaque durable request identity and original order shape. There is no
/// public constructor, getter, Clone, Debug or Serialize implementation.
pub struct Stage8a4DurableRequestContext {
    request_id: StrategyRequestId,
    client_order_id: ClientOrderId,
    account_id: BrokerAccountId,
    instrument: InstrumentId,
    side: OrderSide,
    qty: Quantity,
    order_type: OrderType,
    time_in_force: TimeInForce,
    limit_price: Option<Price>,
    known_broker_order_id: Option<BrokerOrderId>,
    possible_effect_at: DateTime<Utc>,
    event_start: DateTime<Utc>,
    event_end: DateTime<Utc>,
    durable_binding_sha256: String,
}

/// Opaque sealed policy. Callers cannot choose freshness, interval or price
/// matching numbers at reconciliation time.
pub struct Stage8a4ReconciliationPolicy {
    trusted_now: DateTime<Utc>,
    max_source_age: Duration,
    max_cross_source_skew: Duration,
    max_trade_intervals: usize,
    max_interval_split_depth: u8,
    policy_binding_sha256: String,
}

pub struct Stage8a4SourceTiming {
    request_started_at: DateTime<Utc>,
    response_received_at: DateTime<Utc>,
}

pub struct Stage8a4NonPaginatedOrdersSnapshotComplete {
    timing: Stage8a4SourceTiming,
}

pub struct Stage8a4CompletePositionsSnapshot {
    timing: Stage8a4SourceTiming,
}

pub enum Stage8a4InstrumentCompletenessEvidence {
    ExactTargetResolved { timing: Stage8a4SourceTiming },
    FullRegistryCursorExhausted { timing: Stage8a4SourceTiming },
}

struct Stage8a4TradeIntervalProof {
    start_inclusive: DateTime<Utc>,
    end_exclusive: DateTime<Utc>,
    requested_limit: usize,
    returned_count: usize,
    request_started_at: DateTime<Utc>,
    response_received_at: DateTime<Utc>,
    split_depth: u8,
}

type Stage8a4IntervalBounds = (DateTime<Utc>, DateTime<Utc>);
type Stage8a4IntervalSplit = (Stage8a4IntervalBounds, Stage8a4IntervalBounds);

pub struct Stage8a4BoundedTradeHistoryComplete {
    intervals: Vec<Stage8a4TradeIntervalProof>,
    interval_coverage_sha256: String,
}

/// Opaque source-specific acquisition proof. It is neither a bag of caller
/// booleans nor serializable data.
pub struct Stage8a4SourceEvidence {
    orders: Stage8a4NonPaginatedOrdersSnapshotComplete,
    trades: Stage8a4BoundedTradeHistoryComplete,
    positions: Stage8a4CompletePositionsSnapshot,
    instruments: Stage8a4InstrumentCompletenessEvidence,
    exact_order_observation: Option<BrokerOrderSnapshot>,
    acquisition_policy_sha256: String,
}

/// Opaque owned canonical truth admitted under a sealed source policy.
pub struct Stage8a4FreshTruthAdmission {
    truth: BrokerTruthSnapshot,
    exact_order_observation: Option<BrokerOrderSnapshot>,
    truth_binding_sha256: String,
    account_active_orders_count: usize,
    target_active_orders_count: usize,
}

/// Admit canonical broker truth only after source-specific completeness,
/// freshness, exact-account and exact-instrument checks. The input types are
/// externally unconstructible in R1; a future authority bridge is separate.
pub fn admit_stage8a4_broker_truth(
    context: &Stage8a4DurableRequestContext,
    policy: &Stage8a4ReconciliationPolicy,
    truth: BrokerTruthSnapshot,
    evidence: Stage8a4SourceEvidence,
) -> Result<Stage8a4FreshTruthAdmission, Box<Stage8a4ReconciliationDiagnostic>> {
    if !valid_sha256(&context.durable_binding_sha256)
        || !valid_sha256(&policy.policy_binding_sha256)
        || evidence.acquisition_policy_sha256 != policy.policy_binding_sha256
    {
        return Err(admission_error(
            Stage8a4OutcomeKind::StillUnknown,
            Stage8a4ReconciliationReason::SourceIncomplete,
        ));
    }
    if truth.account_id != context.account_id {
        return Err(admission_error(
            Stage8a4OutcomeKind::Conflict,
            Stage8a4ReconciliationReason::SourceIdentityMismatch,
        ));
    }
    if truth
        .orders
        .iter()
        .any(|order| order.account_id != context.account_id)
        || truth
            .trades
            .iter()
            .any(|trade| trade.account_id != context.account_id)
        || truth
            .positions
            .iter()
            .any(|position| position.account_id != context.account_id)
    {
        return Err(admission_error(
            Stage8a4OutcomeKind::Conflict,
            Stage8a4ReconciliationReason::SourceIdentityMismatch,
        ));
    }
    validate_timing(&evidence.orders.timing, context, policy)?;
    validate_timing(&evidence.positions.timing, context, policy)?;
    let instrument_timing = match &evidence.instruments {
        Stage8a4InstrumentCompletenessEvidence::ExactTargetResolved { timing }
        | Stage8a4InstrumentCompletenessEvidence::FullRegistryCursorExhausted { timing } => timing,
    };
    validate_timing(instrument_timing, context, policy)?;
    validate_trade_intervals(&evidence, context, policy)?;
    if evidence
        .trades
        .intervals
        .iter()
        .map(|interval| interval.returned_count)
        .sum::<usize>()
        < truth.trades.len()
    {
        return Err(admission_error(
            Stage8a4OutcomeKind::StillUnknown,
            Stage8a4ReconciliationReason::SourceIncomplete,
        ));
    }
    validate_cross_source_skew(&evidence, policy)?;

    let matching_specs = truth
        .instruments
        .iter()
        .filter(|spec| spec.matches_instrument_id(&context.instrument))
        .count();
    if matching_specs != 1
        || context
            .instrument
            .venue_symbol
            .as_deref()
            .map_or(true, str::is_empty)
    {
        return Err(admission_error(
            if matching_specs > 1 {
                Stage8a4OutcomeKind::Conflict
            } else {
                Stage8a4OutcomeKind::StillUnknown
            },
            Stage8a4ReconciliationReason::InstrumentUnresolved,
        ));
    }

    if let Some(exact) = evidence.exact_order_observation.as_ref() {
        let Some(expected) = context.known_broker_order_id.as_ref() else {
            return Err(admission_error(
                Stage8a4OutcomeKind::Conflict,
                Stage8a4ReconciliationReason::ExactIdentityDisagreement,
            ));
        };
        if exact.broker_order_id.as_ref() != Some(expected)
            || exact.account_id != context.account_id
            || exact.received_ts > policy.trusted_now
            || policy.trusted_now - exact.received_ts > policy.max_source_age
        {
            return Err(admission_error(
                Stage8a4OutcomeKind::Conflict,
                Stage8a4ReconciliationReason::ExactIdentityDisagreement,
            ));
        }
        if let Some(listed) = truth
            .orders
            .iter()
            .find(|order| order.broker_order_id.as_ref() == Some(expected))
        {
            if !same_material_order(listed, exact) {
                return Err(admission_error(
                    Stage8a4OutcomeKind::Conflict,
                    Stage8a4ReconciliationReason::ExactIdentityDisagreement,
                ));
            }
        }
    }

    let canonical_truth_sha256 =
        digest_serializable(b"stage8a4-admitted-canonical-truth-v1", &truth);
    let exact_order_sha256 = evidence
        .exact_order_observation
        .as_ref()
        .map(|order| digest_serializable(b"stage8a4-exact-order-observation-v1", order))
        .unwrap_or_else(|| digest_parts(b"stage8a4-no-exact-order-observation-v1", &[]));
    let truth_binding_sha256 = digest_parts(
        b"stage8a4-complete-admission-v1",
        &[
            canonical_truth_sha256.as_bytes(),
            context.durable_binding_sha256.as_bytes(),
            exact_order_sha256.as_bytes(),
            policy.policy_binding_sha256.as_bytes(),
        ],
    );
    let account_active_orders_count = truth.account_wide_active_order_count();
    let target_active_orders_count = truth.target_active_orders(&context.instrument).len();
    Ok(Stage8a4FreshTruthAdmission {
        truth,
        exact_order_observation: evidence.exact_order_observation,
        truth_binding_sha256,
        account_active_orders_count,
        target_active_orders_count,
    })
}

/// Pure deterministic reducer. It consumes all linear inputs and emits only a
/// redacted semantic result with retry/send explicitly false.
pub fn reduce_stage8a4_reconciliation(
    context: Stage8a4DurableRequestContext,
    admission: Stage8a4FreshTruthAdmission,
    policy: Stage8a4ReconciliationPolicy,
) -> Stage8a4ReconciliationDiagnostic {
    if !valid_sha256(&admission.truth_binding_sha256)
        || !valid_sha256(&policy.policy_binding_sha256)
        || !valid_sha256(&context.durable_binding_sha256)
    {
        return non_exact(
            Stage8a4OutcomeKind::StillUnknown,
            Stage8a4ReconciliationReason::SourceIncomplete,
        );
    }

    let tier1 = admission
        .truth
        .orders
        .iter()
        .filter(|order| order.client_order_id.as_ref() == Some(&context.client_order_id))
        .collect::<Vec<_>>();
    let tier2 = context.known_broker_order_id.as_ref().map(|expected| {
        let mut values = admission
            .truth
            .orders
            .iter()
            .filter(|order| order.broker_order_id.as_ref() == Some(expected))
            .collect::<Vec<_>>();
        if let Some(exact) = admission.exact_order_observation.as_ref() {
            if exact.broker_order_id.as_ref() == Some(expected) && values.is_empty() {
                values.push(exact);
            }
        }
        values
    });

    if tier1.len() > 1 || tier2.as_ref().is_some_and(|values| values.len() > 1) {
        return with_counts(
            non_exact(
                Stage8a4OutcomeKind::Conflict,
                Stage8a4ReconciliationReason::MultipleCandidates,
            ),
            &admission,
        );
    }
    if let (Some(client), Some(broker)) = (
        tier1.first(),
        tier2.as_ref().and_then(|values| values.first()),
    ) {
        if !std::ptr::eq(*client, *broker) {
            return with_counts(
                non_exact(
                    Stage8a4OutcomeKind::Conflict,
                    Stage8a4ReconciliationReason::ExactIdentityDisagreement,
                ),
                &admission,
            );
        }
    }

    let (selected, reason) = if let Some(order) = tier1.first() {
        (
            *order,
            Stage8a4ReconciliationReason::ExactTier1ClientIdentity,
        )
    } else if let Some(order) = tier2.as_ref().and_then(|values| values.first()) {
        (
            *order,
            Stage8a4ReconciliationReason::ExactTier2BrokerIdentity,
        )
    } else {
        let mut candidates = Vec::new();
        let mut missing_required_shape = false;
        for order in &admission.truth.orders {
            match tier3_matches(order, &context) {
                Tier3Match::Match => candidates.push(order),
                Tier3Match::MissingRequiredShape => missing_required_shape = true,
                Tier3Match::NoMatch => {}
            }
        }
        if candidates.len() > 1 {
            return with_counts(
                non_exact(
                    Stage8a4OutcomeKind::Conflict,
                    Stage8a4ReconciliationReason::MultipleCandidates,
                ),
                &admission,
            );
        }
        let Some(order) = candidates.first() else {
            return with_counts(
                non_exact(
                    Stage8a4OutcomeKind::StillUnknown,
                    if missing_required_shape {
                        Stage8a4ReconciliationReason::MissingRequiredShape
                    } else {
                        Stage8a4ReconciliationReason::NoCandidate
                    },
                ),
                &admission,
            );
        };
        (*order, Stage8a4ReconciliationReason::ExactTier3BoundShape)
    };

    match exact_order_shape(selected, &context) {
        Stage8a4ExactShape::Compatible => {}
        Stage8a4ExactShape::MissingRequired => {
            return with_counts(
                non_exact(
                    Stage8a4OutcomeKind::StillUnknown,
                    Stage8a4ReconciliationReason::MissingRequiredShape,
                ),
                &admission,
            )
        }
        Stage8a4ExactShape::Contradiction => {
            return with_counts(
                non_exact(
                    Stage8a4OutcomeKind::Conflict,
                    Stage8a4ReconciliationReason::OrderShapeContradiction,
                ),
                &admission,
            )
        }
    }
    let deduped = match deduplicate_trades(&admission.truth.trades) {
        Ok(value) => value,
        Err(()) => {
            return with_counts(
                non_exact(
                    Stage8a4OutcomeKind::Conflict,
                    Stage8a4ReconciliationReason::TradeIdentityConflict,
                ),
                &admission,
            )
        }
    };
    let matching_trades = deduped
        .values()
        .filter(|trade| trade_matches_selected(trade, selected))
        .collect::<Vec<_>>();
    let trade_qty: Quantity = matching_trades.iter().map(|trade| trade.qty).sum();
    if trade_qty != selected.filled_qty {
        return with_trade_count(
            with_counts(
                non_exact(
                    Stage8a4OutcomeKind::Conflict,
                    Stage8a4ReconciliationReason::TradeQuantityContradiction,
                ),
                &admission,
            ),
            matching_trades.len(),
        );
    }

    let (lifecycle, fill) = match exact_state(selected) {
        Ok(value) => value,
        Err(reason) => {
            let outcome = if reason == Stage8a4ReconciliationReason::UnknownOrderStatus {
                Stage8a4OutcomeKind::StillUnknown
            } else {
                Stage8a4OutcomeKind::Conflict
            };
            return with_trade_count(
                with_counts(non_exact(outcome, reason), &admission),
                matching_trades.len(),
            );
        }
    };
    let selected_order_binding_sha256 =
        digest_serializable(b"stage8a4-selected-order-v1", selected);
    let trade_summary_binding_sha256 = digest_serializable(
        b"stage8a4-deduplicated-matching-trades-v1",
        &matching_trades,
    );
    exact_diagnostic(Stage8a4ExactDiagnosticInput {
        reason,
        lifecycle,
        fill,
        selected_order_binding_sha256,
        trade_summary_binding_sha256,
        account_active_orders_count: admission.account_active_orders_count,
        target_active_orders_count: admission.target_active_orders_count,
        matching_trade_count: matching_trades.len(),
        context: &context,
        policy: &policy,
    })
}

fn validate_timing(
    timing: &Stage8a4SourceTiming,
    context: &Stage8a4DurableRequestContext,
    policy: &Stage8a4ReconciliationPolicy,
) -> Result<(), Box<Stage8a4ReconciliationDiagnostic>> {
    if timing.request_started_at < context.possible_effect_at
        || timing.response_received_at < timing.request_started_at
    {
        return Err(admission_error(
            Stage8a4OutcomeKind::StillUnknown,
            Stage8a4ReconciliationReason::SourceIncomplete,
        ));
    }
    if timing.response_received_at > policy.trusted_now
        || policy.trusted_now - timing.response_received_at > policy.max_source_age
    {
        return Err(admission_error(
            Stage8a4OutcomeKind::StillUnknown,
            Stage8a4ReconciliationReason::SourceStale,
        ));
    }
    Ok(())
}

fn validate_trade_intervals(
    evidence: &Stage8a4SourceEvidence,
    context: &Stage8a4DurableRequestContext,
    policy: &Stage8a4ReconciliationPolicy,
) -> Result<(), Box<Stage8a4ReconciliationDiagnostic>> {
    if evidence.trades.intervals.is_empty()
        || evidence.trades.intervals.len() > policy.max_trade_intervals
        || policy.max_interval_split_depth == 0
    {
        return Err(admission_error(
            Stage8a4OutcomeKind::StillUnknown,
            Stage8a4ReconciliationReason::SourceIncomplete,
        ));
    }
    let mut intervals = evidence.trades.intervals.iter().collect::<Vec<_>>();
    intervals.sort_by_key(|item| (item.start_inclusive, item.end_exclusive));
    if interval_coverage_fingerprint(&intervals) != evidence.trades.interval_coverage_sha256
        || intervals[0].start_inclusive > context.event_start
        || intervals
            .last()
            .map_or(true, |last| last.end_exclusive < context.event_end)
    {
        return Err(admission_error(
            Stage8a4OutcomeKind::StillUnknown,
            Stage8a4ReconciliationReason::SourceIncomplete,
        ));
    }
    let mut covered_end = intervals[0].start_inclusive;
    let mut received_times = Vec::new();
    for interval in intervals {
        if interval.start_inclusive >= interval.end_exclusive
            || interval.requested_limit == 0
            || interval.start_inclusive > covered_end
            || interval.request_started_at < context.possible_effect_at
            || interval.response_received_at < interval.request_started_at
            || interval.response_received_at > policy.trusted_now
            || policy.trusted_now - interval.response_received_at > policy.max_source_age
            || interval.split_depth > policy.max_interval_split_depth
        {
            return Err(admission_error(
                Stage8a4OutcomeKind::StillUnknown,
                Stage8a4ReconciliationReason::SourceIncomplete,
            ));
        }
        if interval.returned_count >= interval.requested_limit {
            // This computes the only deterministic next acquisition split.
            // The saturated observation itself is never admitted as complete.
            let _next_split = deterministic_interval_split(interval, interval.split_depth, policy);
            return Err(admission_error(
                Stage8a4OutcomeKind::StillUnknown,
                Stage8a4ReconciliationReason::SourceIncomplete,
            ));
        }
        covered_end = covered_end.max(interval.end_exclusive);
        received_times.push(interval.response_received_at);
    }
    let min_received = received_times.iter().min().expect("non-empty intervals");
    let max_received = received_times.iter().max().expect("non-empty intervals");
    if *max_received - *min_received > policy.max_cross_source_skew {
        return Err(admission_error(
            Stage8a4OutcomeKind::StillUnknown,
            Stage8a4ReconciliationReason::SourceStale,
        ));
    }
    Ok(())
}

fn validate_cross_source_skew(
    evidence: &Stage8a4SourceEvidence,
    policy: &Stage8a4ReconciliationPolicy,
) -> Result<(), Box<Stage8a4ReconciliationDiagnostic>> {
    let instrument_timing = match &evidence.instruments {
        Stage8a4InstrumentCompletenessEvidence::ExactTargetResolved { timing }
        | Stage8a4InstrumentCompletenessEvidence::FullRegistryCursorExhausted { timing } => timing,
    };
    let mut received = vec![
        evidence.orders.timing.response_received_at,
        evidence.positions.timing.response_received_at,
        instrument_timing.response_received_at,
    ];
    received.extend(
        evidence
            .trades
            .intervals
            .iter()
            .map(|interval| interval.response_received_at),
    );
    if let Some(exact) = evidence.exact_order_observation.as_ref() {
        received.push(exact.received_ts);
    }
    let min = received
        .iter()
        .min()
        .expect("required sources are non-empty");
    let max = received
        .iter()
        .max()
        .expect("required sources are non-empty");
    if *max - *min > policy.max_cross_source_skew {
        return Err(admission_error(
            Stage8a4OutcomeKind::StillUnknown,
            Stage8a4ReconciliationReason::SourceStale,
        ));
    }
    Ok(())
}

fn deterministic_interval_split(
    interval: &Stage8a4TradeIntervalProof,
    depth: u8,
    policy: &Stage8a4ReconciliationPolicy,
) -> Option<Stage8a4IntervalSplit> {
    if depth >= policy.max_interval_split_depth
        || interval.start_inclusive >= interval.end_exclusive
    {
        return None;
    }
    let span = interval.end_exclusive - interval.start_inclusive;
    let micros = span.num_microseconds()?;
    if micros < 2 {
        return None;
    }
    let midpoint = interval.start_inclusive + Duration::microseconds(micros / 2);
    if midpoint <= interval.start_inclusive || midpoint >= interval.end_exclusive {
        return None;
    }
    Some((
        (interval.start_inclusive, midpoint),
        (midpoint, interval.end_exclusive),
    ))
}

#[derive(Clone, Copy)]
enum Tier3Match {
    Match,
    MissingRequiredShape,
    NoMatch,
}

fn tier3_matches(
    order: &BrokerOrderSnapshot,
    context: &Stage8a4DurableRequestContext,
) -> Tier3Match {
    if order.account_id != context.account_id
        || order.side != context.side
        || order.qty != context.qty
    {
        return Tier3Match::NoMatch;
    }
    if order.instrument.venue_symbol.is_none()
        && order.instrument.symbol == context.instrument.symbol
        && order.instrument.exchange == context.instrument.exchange
        && order.instrument.market == context.instrument.market
    {
        return Tier3Match::MissingRequiredShape;
    }
    if !exact_instrument_matches(&order.instrument, &context.instrument) {
        return Tier3Match::NoMatch;
    }
    if order.time_in_force.is_none() {
        return Tier3Match::MissingRequiredShape;
    }
    if order.order_type != context.order_type || order.time_in_force != Some(context.time_in_force)
    {
        return Tier3Match::NoMatch;
    }
    match context.order_type {
        OrderType::Limit if order.limit_price.is_none() => return Tier3Match::MissingRequiredShape,
        OrderType::Limit if order.limit_price != context.limit_price => return Tier3Match::NoMatch,
        OrderType::Limit => {}
        OrderType::Market if order.limit_price.is_some() => return Tier3Match::NoMatch,
        OrderType::Market => {}
        _ => return Tier3Match::NoMatch,
    }
    let event_ts = order.source_ts.unwrap_or(order.received_ts);
    if event_ts < context.event_start || event_ts >= context.event_end {
        Tier3Match::NoMatch
    } else {
        Tier3Match::Match
    }
}

enum Stage8a4ExactShape {
    Compatible,
    MissingRequired,
    Contradiction,
}

fn exact_order_shape(
    order: &BrokerOrderSnapshot,
    context: &Stage8a4DurableRequestContext,
) -> Stage8a4ExactShape {
    if order.account_id != context.account_id
        || order.side != context.side
        || order.qty != context.qty
        || order.order_type != context.order_type
    {
        return Stage8a4ExactShape::Contradiction;
    }
    if order.instrument.venue_symbol.is_none() || order.time_in_force.is_none() {
        return Stage8a4ExactShape::MissingRequired;
    }
    if !exact_instrument_matches(&order.instrument, &context.instrument)
        || order.time_in_force != Some(context.time_in_force)
    {
        return Stage8a4ExactShape::Contradiction;
    }
    match context.order_type {
        OrderType::Limit if order.limit_price.is_none() => Stage8a4ExactShape::MissingRequired,
        OrderType::Limit if order.limit_price == context.limit_price => {
            Stage8a4ExactShape::Compatible
        }
        OrderType::Market if order.limit_price.is_none() => Stage8a4ExactShape::Compatible,
        _ => Stage8a4ExactShape::Contradiction,
    }
}

fn exact_instrument_matches(left: &InstrumentId, right: &InstrumentId) -> bool {
    left.venue_symbol
        .as_deref()
        .is_some_and(|value| !value.is_empty())
        && left.venue_symbol == right.venue_symbol
        && left.symbol == right.symbol
        && left.exchange == right.exchange
        && left.market == right.market
}

fn exact_state(
    order: &BrokerOrderSnapshot,
) -> Result<(Stage8a4ExactLifecycle, Stage8a4FillEffect), Stage8a4ReconciliationReason> {
    if order.qty <= Decimal::ZERO
        || order.filled_qty < Decimal::ZERO
        || order.filled_qty > order.qty
        || order
            .remaining_qty
            .is_some_and(|remaining| remaining != order.qty - order.filled_qty)
        || order.lifecycle != BrokerOrderSnapshot::lifecycle_for(&order.status)
    {
        return Err(Stage8a4ReconciliationReason::OrderQuantityContradiction);
    }
    let fill = if order.filled_qty == Decimal::ZERO {
        Stage8a4FillEffect::Zero
    } else if order.filled_qty == order.qty {
        Stage8a4FillEffect::Full {
            filled_qty: order.filled_qty,
        }
    } else {
        Stage8a4FillEffect::Partial {
            filled_qty: order.filled_qty,
        }
    };
    let lifecycle = match (&order.status, fill) {
        (OrderStatus::New | OrderStatus::Working, Stage8a4FillEffect::Zero)
        | (OrderStatus::PartiallyFilled, Stage8a4FillEffect::Partial { .. }) => {
            Stage8a4ExactLifecycle::Working
        }
        (OrderStatus::Filled, Stage8a4FillEffect::Full { .. }) => {
            Stage8a4ExactLifecycle::TerminalFilled
        }
        (OrderStatus::Rejected, Stage8a4FillEffect::Zero) => {
            Stage8a4ExactLifecycle::TerminalRejected
        }
        (OrderStatus::Canceled, Stage8a4FillEffect::Zero | Stage8a4FillEffect::Partial { .. }) => {
            Stage8a4ExactLifecycle::TerminalCancelled
        }
        (OrderStatus::Expired, Stage8a4FillEffect::Zero | Stage8a4FillEffect::Partial { .. }) => {
            Stage8a4ExactLifecycle::TerminalExpired
        }
        (OrderStatus::Unknown(_), _) => {
            return Err(Stage8a4ReconciliationReason::UnknownOrderStatus)
        }
        _ => return Err(Stage8a4ReconciliationReason::OrderQuantityContradiction),
    };
    Ok((lifecycle, fill))
}

fn deduplicate_trades(
    trades: &[BrokerTradeSnapshot],
) -> Result<BTreeMap<String, &BrokerTradeSnapshot>, ()> {
    let mut unique: BTreeMap<String, &BrokerTradeSnapshot> = BTreeMap::new();
    for trade in trades {
        let id = trade.broker_trade_id.as_str().to_string();
        if let Some(existing) = unique.get(&id) {
            if !same_material_trade(existing, trade) {
                return Err(());
            }
        } else {
            unique.insert(id, trade);
        }
    }
    Ok(unique)
}

fn same_material_trade(left: &BrokerTradeSnapshot, right: &BrokerTradeSnapshot) -> bool {
    left.account_id == right.account_id
        && left.broker_trade_id == right.broker_trade_id
        && left.broker_order_id == right.broker_order_id
        && left.client_order_id == right.client_order_id
        && left.instrument == right.instrument
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

fn same_material_order(left: &BrokerOrderSnapshot, right: &BrokerOrderSnapshot) -> bool {
    left.account_id == right.account_id
        && left.broker_order_id == right.broker_order_id
        && left.client_order_id == right.client_order_id
        && left.instrument == right.instrument
        && left.side == right.side
        && left.order_type == right.order_type
        && left.time_in_force == right.time_in_force
        && left.status == right.status
        && left.lifecycle == right.lifecycle
        && left.qty == right.qty
        && left.filled_qty == right.filled_qty
        && left.remaining_qty == right.remaining_qty
        && left.limit_price == right.limit_price
        && left.broker_asset_id == right.broker_asset_id
        && left.board == right.board
        && left.expiration_date == right.expiration_date
        && left.source_ts == right.source_ts
}

fn trade_matches_selected(trade: &BrokerTradeSnapshot, order: &BrokerOrderSnapshot) -> bool {
    let exact_identity = trade
        .broker_order_id
        .as_ref()
        .zip(order.broker_order_id.as_ref())
        .is_some_and(|(left, right)| left == right)
        || trade
            .client_order_id
            .as_ref()
            .zip(order.client_order_id.as_ref())
            .is_some_and(|(left, right)| left == right);
    exact_identity
        && trade.account_id == order.account_id
        && exact_instrument_matches(&trade.instrument, &order.instrument)
        && trade.side == order.side
}

fn interval_coverage_fingerprint(intervals: &[&Stage8a4TradeIntervalProof]) -> String {
    let mut digest = Sha256::new();
    digest.update(b"stage8a4-trade-interval-coverage-v1");
    for interval in intervals {
        digest.update(interval.start_inclusive.to_rfc3339().as_bytes());
        digest.update(interval.end_exclusive.to_rfc3339().as_bytes());
        digest.update(interval.requested_limit.to_be_bytes());
        digest.update(interval.returned_count.to_be_bytes());
        digest.update([interval.split_depth]);
    }
    to_hex(&digest.finalize())
}

struct Stage8a4ExactDiagnosticInput<'a> {
    reason: Stage8a4ReconciliationReason,
    lifecycle: Stage8a4ExactLifecycle,
    fill: Stage8a4FillEffect,
    selected_order_binding_sha256: String,
    trade_summary_binding_sha256: String,
    account_active_orders_count: usize,
    target_active_orders_count: usize,
    matching_trade_count: usize,
    context: &'a Stage8a4DurableRequestContext,
    policy: &'a Stage8a4ReconciliationPolicy,
}

fn exact_diagnostic(input: Stage8a4ExactDiagnosticInput<'_>) -> Stage8a4ReconciliationDiagnostic {
    let semantic_binding_sha256 = digest_parts(
        b"stage8a4-exact-semantic-binding-v1",
        &[
            input.context.durable_binding_sha256.as_bytes(),
            input.context.request_id.to_string().as_bytes(),
            input.policy.policy_binding_sha256.as_bytes(),
            input.selected_order_binding_sha256.as_bytes(),
            input.trade_summary_binding_sha256.as_bytes(),
            format!("{:?}:{:?}", input.lifecycle, input.fill).as_bytes(),
        ],
    );
    Stage8a4ReconciliationDiagnostic {
        outcome: Stage8a4OutcomeKind::ExactOrderState,
        reason: input.reason,
        lifecycle: Some(input.lifecycle),
        fill: Some(input.fill),
        selected_order_binding_sha256: Some(input.selected_order_binding_sha256),
        trade_summary_binding_sha256: Some(input.trade_summary_binding_sha256),
        account_active_orders_count: input.account_active_orders_count,
        target_active_orders_count: input.target_active_orders_count,
        matching_trade_count: input.matching_trade_count,
        semantic_binding_sha256,
        retry_authorized: false,
        send_authorized: false,
    }
}

fn admission_error(
    outcome: Stage8a4OutcomeKind,
    reason: Stage8a4ReconciliationReason,
) -> Box<Stage8a4ReconciliationDiagnostic> {
    Box::new(non_exact(outcome, reason))
}

fn non_exact(
    outcome: Stage8a4OutcomeKind,
    reason: Stage8a4ReconciliationReason,
) -> Stage8a4ReconciliationDiagnostic {
    let semantic_binding_sha256 = digest_parts(
        b"stage8a4-non-exact-semantic-binding-v1",
        &[format!("{outcome:?}:{reason:?}").as_bytes()],
    );
    Stage8a4ReconciliationDiagnostic {
        outcome,
        reason,
        lifecycle: None,
        fill: None,
        selected_order_binding_sha256: None,
        trade_summary_binding_sha256: None,
        account_active_orders_count: 0,
        target_active_orders_count: 0,
        matching_trade_count: 0,
        semantic_binding_sha256,
        retry_authorized: false,
        send_authorized: false,
    }
}

fn with_counts(
    mut diagnostic: Stage8a4ReconciliationDiagnostic,
    admission: &Stage8a4FreshTruthAdmission,
) -> Stage8a4ReconciliationDiagnostic {
    diagnostic.account_active_orders_count = admission.account_active_orders_count;
    diagnostic.target_active_orders_count = admission.target_active_orders_count;
    diagnostic
}

fn with_trade_count(
    mut diagnostic: Stage8a4ReconciliationDiagnostic,
    matching_trade_count: usize,
) -> Stage8a4ReconciliationDiagnostic {
    diagnostic.matching_trade_count = matching_trade_count;
    diagnostic
}

fn digest_serializable<T: Serialize + ?Sized>(domain: &[u8], value: &T) -> String {
    let encoded = serde_json::to_vec(value).expect("canonical Stage 8A-4 value serializes");
    digest_parts(domain, &[&encoded])
}

fn digest_parts(domain: &[u8], parts: &[&[u8]]) -> String {
    let mut digest = Sha256::new();
    digest.update(domain);
    for part in parts {
        digest.update((part.len() as u64).to_be_bytes());
        digest.update(part);
    }
    to_hex(&digest.finalize())
}

fn to_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests;
