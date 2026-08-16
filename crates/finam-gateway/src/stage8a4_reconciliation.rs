//! Stage 8A-4 pure broker-truth admission and reconciliation reducer.
//!
//! The input capabilities are intentionally opaque and have no public
//! constructors. Stage 8A-4 implementation R4 can therefore be exercised by
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

#[allow(dead_code)]
mod durable_composition_i2;

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

/// Opaque exact GET-order observation with its own HTTP acquisition timing.
/// Broker snapshot receipt metadata is not a substitute for request timing.
pub struct Stage8a4ExactOrderObservation {
    order: BrokerOrderSnapshot,
    timing: Stage8a4SourceTiming,
}

/// Opaque source-specific acquisition proof. It is neither a bag of caller
/// booleans nor serializable data.
pub struct Stage8a4SourceEvidence {
    orders: Stage8a4NonPaginatedOrdersSnapshotComplete,
    trades: Stage8a4BoundedTradeHistoryComplete,
    positions: Stage8a4CompletePositionsSnapshot,
    instruments: Stage8a4InstrumentCompletenessEvidence,
    exact_lookup: Stage8a4PrivateExactLookup,
    canonical_truth_payload_sha256: String,
    acquisition_policy_sha256: String,
}

/// Opaque owned canonical truth admitted under a sealed source policy.
pub struct Stage8a4FreshTruthAdmission {
    truth: BrokerTruthSnapshot,
    exact_lookup: Stage8a4PrivateExactLookup,
    admitted_durable_binding_sha256: String,
    admitted_policy_binding_sha256: String,
    source_evidence_binding_sha256: String,
    truth_binding_sha256: String,
    account_active_orders_count: usize,
    target_active_orders_count: usize,
}

struct Stage8a4AdmissionAttemptBinding {
    durable_binding_sha256: String,
    request_id: String,
    policy_binding_sha256: String,
    canonical_truth_sha256: String,
    source_evidence_binding_sha256: String,
}

/// Admit canonical broker truth only after source-specific completeness,
/// freshness, exact-account and exact-instrument checks. The input types are
/// externally unconstructible in R4; a future authority bridge is separate.
pub fn admit_stage8a4_broker_truth(
    context: &Stage8a4DurableRequestContext,
    policy: &Stage8a4ReconciliationPolicy,
    truth: BrokerTruthSnapshot,
    evidence: Stage8a4SourceEvidence,
) -> Result<Stage8a4FreshTruthAdmission, Box<Stage8a4ReconciliationDiagnostic>> {
    let canonical_truth_sha256 = canonical_truth_binding(&truth);
    let source_evidence_binding_sha256 =
        source_evidence_binding(&evidence, &canonical_truth_sha256);
    let attempt = Stage8a4AdmissionAttemptBinding {
        durable_binding_sha256: context.durable_binding_sha256.clone(),
        request_id: context.request_id.to_string(),
        policy_binding_sha256: policy.policy_binding_sha256.clone(),
        canonical_truth_sha256: canonical_truth_sha256.clone(),
        source_evidence_binding_sha256: source_evidence_binding_sha256.clone(),
    };
    if !valid_sha256(&context.durable_binding_sha256)
        || !valid_sha256(&policy.policy_binding_sha256)
        || evidence.acquisition_policy_sha256 != policy.policy_binding_sha256
    {
        return Err(admission_error(
            Stage8a4OutcomeKind::StillUnknown,
            Stage8a4ReconciliationReason::SourceIncomplete,
            &attempt,
        ));
    }
    if !valid_sha256(&evidence.canonical_truth_payload_sha256)
        || evidence.canonical_truth_payload_sha256 != canonical_truth_sha256
    {
        return Err(admission_error(
            Stage8a4OutcomeKind::StillUnknown,
            Stage8a4ReconciliationReason::SourceIncomplete,
            &attempt,
        ));
    }
    if truth.account_id != context.account_id {
        return Err(admission_error(
            Stage8a4OutcomeKind::Conflict,
            Stage8a4ReconciliationReason::SourceIdentityMismatch,
            &attempt,
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
            &attempt,
        ));
    }
    validate_timing(&evidence.orders.timing, context, policy, &attempt)?;
    validate_timing(&evidence.positions.timing, context, policy, &attempt)?;
    let instrument_timing = match &evidence.instruments {
        Stage8a4InstrumentCompletenessEvidence::ExactTargetResolved { timing }
        | Stage8a4InstrumentCompletenessEvidence::FullRegistryCursorExhausted { timing } => timing,
    };
    validate_timing(instrument_timing, context, policy, &attempt)?;
    validate_trade_intervals(&evidence, context, policy, &attempt)?;
    if evidence
        .trades
        .intervals
        .iter()
        .map(|interval| interval.returned_count)
        .sum::<usize>()
        != truth.trades.len()
    {
        return Err(admission_error(
            Stage8a4OutcomeKind::StillUnknown,
            Stage8a4ReconciliationReason::SourceIncomplete,
            &attempt,
        ));
    }
    validate_cross_source_skew(&evidence, policy, &attempt)?;

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
            &attempt,
        ));
    }

    if let Some(timing) = evidence.exact_lookup.timing() {
        validate_timing(timing, context, policy, &attempt)?;
    }
    if !evidence.exact_lookup.has_valid_source_shape(context) {
        return Err(admission_error(
            Stage8a4OutcomeKind::StillUnknown,
            Stage8a4ReconciliationReason::SourceIncomplete,
            &attempt,
        ));
    }
    if let Some(exact_source) = evidence.exact_lookup.succeeded_observation() {
        let exact = &exact_source.order;
        let Some(expected) = context.known_broker_order_id.as_ref() else {
            return Err(admission_error(
                Stage8a4OutcomeKind::Conflict,
                Stage8a4ReconciliationReason::ExactIdentityDisagreement,
                &attempt,
            ));
        };
        if exact.broker_order_id.as_ref() != Some(expected)
            || exact.account_id != context.account_id
            || exact.received_ts > exact_source.timing.response_received_at
            || selected_order_identity(exact, context).is_err()
        {
            return Err(admission_error(
                Stage8a4OutcomeKind::Conflict,
                Stage8a4ReconciliationReason::ExactIdentityDisagreement,
                &attempt,
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
                    &attempt,
                ));
            }
        }
    }

    let truth_binding_sha256 = digest_parts(
        b"stage8a4-complete-admission-v2",
        &[
            canonical_truth_sha256.as_bytes(),
            context.durable_binding_sha256.as_bytes(),
            policy.policy_binding_sha256.as_bytes(),
            source_evidence_binding_sha256.as_bytes(),
        ],
    );
    let account_active_orders_count = truth.account_wide_active_order_count();
    let target_active_orders_count = truth.target_active_orders(&context.instrument).len();
    Ok(Stage8a4FreshTruthAdmission {
        truth,
        exact_lookup: evidence.exact_lookup,
        admitted_durable_binding_sha256: context.durable_binding_sha256.clone(),
        admitted_policy_binding_sha256: policy.policy_binding_sha256.clone(),
        source_evidence_binding_sha256,
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
    reduce_stage8a4_authoritative(context, admission, policy).into_diagnostic()
}

struct Stage8a4AuthoritativeReconciliationOutcome {
    context: Stage8a4DurableRequestContext,
    outcome_kind: Stage8a4OutcomeKind,
    reason: Stage8a4ReconciliationReason,
    lifecycle: Option<Stage8a4ExactLifecycle>,
    fill: Option<Stage8a4FillEffect>,
    selected_order_binding_sha256: Option<String>,
    trade_summary_binding_sha256: Option<String>,
    matching_trade_count: usize,
    semantic_binding_sha256: String,
    selected_order: Option<BrokerOrderSnapshot>,
    material_trades: Vec<BrokerTradeSnapshot>,
    exact_lookup: Stage8a4PrivateExactLookup,
    account_safety: Stage8a4PrivateAccountSafety,
    source_evidence_binding_sha256: String,
    private_outcome_binding_sha256: String,
}

impl Stage8a4AuthoritativeReconciliationOutcome {
    fn into_diagnostic(self) -> Stage8a4ReconciliationDiagnostic {
        Stage8a4ReconciliationDiagnostic {
            outcome: self.outcome_kind,
            reason: self.reason,
            lifecycle: self.lifecycle,
            fill: self.fill,
            selected_order_binding_sha256: self.selected_order_binding_sha256,
            trade_summary_binding_sha256: self.trade_summary_binding_sha256,
            account_active_orders_count: self.account_safety.account_active_orders_count,
            target_active_orders_count: self.account_safety.target_active_orders_count,
            matching_trade_count: self.matching_trade_count,
            semantic_binding_sha256: self.semantic_binding_sha256,
            retry_authorized: false,
            send_authorized: false,
        }
    }
}

#[allow(dead_code)]
enum Stage8a4PrivateExactLookup {
    NotAttempted,
    Succeeded(Box<Stage8a4ExactOrderObservation>),
    DocumentedNotFound {
        timing: Stage8a4SourceTiming,
        documented_status_category: String,
    },
    Unavailable {
        timing: Stage8a4SourceTiming,
        failure_category: String,
    },
    DecodeFailure {
        timing: Stage8a4SourceTiming,
        response_status_category: String,
        response_binding_sha256: String,
    },
    Stale {
        timing: Stage8a4SourceTiming,
        stale_observation_binding_sha256: String,
    },
}

impl Stage8a4PrivateExactLookup {
    fn succeeded_observation(&self) -> Option<&Stage8a4ExactOrderObservation> {
        match self {
            Self::Succeeded(observation) => Some(observation),
            Self::NotAttempted
            | Self::DocumentedNotFound { .. }
            | Self::Unavailable { .. }
            | Self::DecodeFailure { .. }
            | Self::Stale { .. } => None,
        }
    }

    fn timing(&self) -> Option<&Stage8a4SourceTiming> {
        match self {
            Self::NotAttempted => None,
            Self::Succeeded(observation) => Some(&observation.timing),
            Self::DocumentedNotFound { timing, .. }
            | Self::Unavailable { timing, .. }
            | Self::DecodeFailure { timing, .. }
            | Self::Stale { timing, .. } => Some(timing),
        }
    }

    fn has_valid_source_shape(&self, context: &Stage8a4DurableRequestContext) -> bool {
        match self {
            Self::NotAttempted | Self::Succeeded(_) => true,
            Self::DocumentedNotFound {
                documented_status_category,
                ..
            } => {
                context.known_broker_order_id.is_some()
                    && !documented_status_category.trim().is_empty()
            }
            Self::Unavailable {
                failure_category, ..
            } => context.known_broker_order_id.is_some() && !failure_category.trim().is_empty(),
            Self::DecodeFailure {
                response_status_category,
                response_binding_sha256,
                ..
            } => {
                context.known_broker_order_id.is_some()
                    && !response_status_category.trim().is_empty()
                    && valid_sha256(response_binding_sha256)
            }
            Self::Stale {
                stale_observation_binding_sha256,
                ..
            } => {
                context.known_broker_order_id.is_some()
                    && valid_sha256(stale_observation_binding_sha256)
            }
        }
    }
}

struct Stage8a4PrivateAccountSafety {
    account_active_orders_count: usize,
    account_unknown_orders_count: usize,
    account_orphan_orders_count: usize,
    account_open_positions_count: usize,
    target_active_orders_count: usize,
    target_unknown_orders_count: usize,
    target_terminal_orders_count: usize,
    target_inconsistent_orders_count: usize,
    target_open_positions_count: usize,
    other_symbol_active_orders_count: usize,
}

fn reduce_stage8a4_authoritative(
    context: Stage8a4DurableRequestContext,
    admission: Stage8a4FreshTruthAdmission,
    policy: Stage8a4ReconciliationPolicy,
) -> Stage8a4AuthoritativeReconciliationOutcome {
    let diagnostic = reduce_stage8a4_diagnostic(&context, &admission, &policy);
    let selected_order = diagnostic
        .selected_order_binding_sha256
        .as_deref()
        .and_then(|binding| {
            admission
                .truth
                .orders
                .iter()
                .chain(
                    admission
                        .exact_lookup
                        .succeeded_observation()
                        .map(|observation| &observation.order),
                )
                .find(|order| digest_serializable(b"stage8a4-selected-order-v1", *order) == binding)
                .cloned()
        });
    let material_trades = selected_order
        .as_ref()
        .and_then(|order| {
            deduplicate_trades(&admission.truth.trades)
                .ok()
                .map(|deduped| (order, deduped))
        })
        .map(|(order, deduped)| {
            deduped
                .into_values()
                .filter(|trade| {
                    matches!(
                        classify_trade_support(trade, order, &context),
                        Stage8a4TradeSupport::CompatibleSupport
                    )
                })
                .cloned()
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let exact_lookup = admission.exact_lookup;
    let account_safety = account_safety_summary(&admission.truth, &context.instrument);
    let private_outcome_binding_sha256 = digest_parts(
        b"stage8a4-private-authoritative-outcome-v1",
        &[
            context.durable_binding_sha256.as_bytes(),
            diagnostic.semantic_binding_sha256.as_bytes(),
            admission.source_evidence_binding_sha256.as_bytes(),
            private_exact_lookup_binding(&exact_lookup).as_bytes(),
            account_safety_binding(&account_safety).as_bytes(),
        ],
    );
    Stage8a4AuthoritativeReconciliationOutcome {
        context,
        outcome_kind: diagnostic.outcome,
        reason: diagnostic.reason,
        lifecycle: diagnostic.lifecycle,
        fill: diagnostic.fill,
        selected_order_binding_sha256: diagnostic.selected_order_binding_sha256,
        trade_summary_binding_sha256: diagnostic.trade_summary_binding_sha256,
        matching_trade_count: diagnostic.matching_trade_count,
        semantic_binding_sha256: diagnostic.semantic_binding_sha256,
        selected_order,
        material_trades,
        exact_lookup,
        account_safety,
        source_evidence_binding_sha256: admission.source_evidence_binding_sha256,
        private_outcome_binding_sha256,
    }
}

// The accepted reducer body predates its I2 borrowed wrapper. Keeping its
// explicit reference call sites makes the semantic diff auditable.
#[allow(clippy::needless_borrow)]
fn reduce_stage8a4_diagnostic(
    context: &Stage8a4DurableRequestContext,
    admission: &Stage8a4FreshTruthAdmission,
    policy: &Stage8a4ReconciliationPolicy,
) -> Stage8a4ReconciliationDiagnostic {
    let canonical_truth_sha256 = canonical_truth_binding(&admission.truth);
    let expected_truth_binding_sha256 = digest_parts(
        b"stage8a4-complete-admission-v2",
        &[
            canonical_truth_sha256.as_bytes(),
            admission.admitted_durable_binding_sha256.as_bytes(),
            admission.admitted_policy_binding_sha256.as_bytes(),
            admission.source_evidence_binding_sha256.as_bytes(),
        ],
    );
    if !valid_sha256(&admission.truth_binding_sha256)
        || !valid_sha256(&admission.source_evidence_binding_sha256)
        || !valid_sha256(&policy.policy_binding_sha256)
        || !valid_sha256(&context.durable_binding_sha256)
        || admission.admitted_durable_binding_sha256 != context.durable_binding_sha256
        || admission.admitted_policy_binding_sha256 != policy.policy_binding_sha256
        || admission.truth_binding_sha256 != expected_truth_binding_sha256
    {
        return reducer_non_exact(
            Stage8a4OutcomeKind::StillUnknown,
            Stage8a4ReconciliationReason::SourceIncomplete,
            &context,
            &policy,
            &admission,
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
        if let Some(exact) = admission.exact_lookup.succeeded_observation() {
            if exact.order.broker_order_id.as_ref() == Some(expected) && values.is_empty() {
                values.push(&exact.order);
            }
        }
        values
    });

    if tier1.len() > 1 || tier2.as_ref().is_some_and(|values| values.len() > 1) {
        return reducer_non_exact(
            Stage8a4OutcomeKind::Conflict,
            Stage8a4ReconciliationReason::MultipleCandidates,
            context,
            policy,
            admission,
        );
    }
    if let (Some(client), Some(broker)) = (
        tier1.first(),
        tier2.as_ref().and_then(|values| values.first()),
    ) {
        if !std::ptr::eq(*client, *broker) {
            return reducer_non_exact(
                Stage8a4OutcomeKind::Conflict,
                Stage8a4ReconciliationReason::ExactIdentityDisagreement,
                &context,
                &policy,
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
                Tier3Match::IdentityConflict => {
                    return reducer_non_exact(
                        Stage8a4OutcomeKind::Conflict,
                        Stage8a4ReconciliationReason::ExactIdentityDisagreement,
                        &context,
                        &policy,
                        &admission,
                    )
                }
                Tier3Match::NoMatch => {}
            }
        }
        if candidates.len() > 1 {
            return reducer_non_exact(
                Stage8a4OutcomeKind::Conflict,
                Stage8a4ReconciliationReason::MultipleCandidates,
                &context,
                &policy,
                &admission,
            );
        }
        let Some(order) = candidates.first() else {
            return reducer_non_exact(
                Stage8a4OutcomeKind::StillUnknown,
                if missing_required_shape {
                    Stage8a4ReconciliationReason::MissingRequiredShape
                } else {
                    Stage8a4ReconciliationReason::NoCandidate
                },
                &context,
                &policy,
                &admission,
            );
        };
        (*order, Stage8a4ReconciliationReason::ExactTier3BoundShape)
    };

    if selected_order_identity(selected, &context).is_err() {
        return reducer_non_exact(
            Stage8a4OutcomeKind::Conflict,
            Stage8a4ReconciliationReason::ExactIdentityDisagreement,
            &context,
            &policy,
            &admission,
        );
    }
    match exact_order_shape(selected, &context) {
        Stage8a4ExactShape::Compatible => {}
        Stage8a4ExactShape::MissingRequired => {
            return reducer_non_exact(
                Stage8a4OutcomeKind::StillUnknown,
                Stage8a4ReconciliationReason::MissingRequiredShape,
                &context,
                &policy,
                &admission,
            )
        }
        Stage8a4ExactShape::Contradiction => {
            return reducer_non_exact(
                Stage8a4OutcomeKind::Conflict,
                Stage8a4ReconciliationReason::OrderShapeContradiction,
                &context,
                &policy,
                &admission,
            )
        }
    }
    let deduped = match deduplicate_trades(&admission.truth.trades) {
        Ok(value) => value,
        Err(()) => {
            return reducer_non_exact(
                Stage8a4OutcomeKind::Conflict,
                Stage8a4ReconciliationReason::TradeIdentityConflict,
                &context,
                &policy,
                &admission,
            )
        }
    };
    let mut matching_trades = Vec::new();
    for trade in deduped.values() {
        match classify_trade_support(trade, selected, &context) {
            Stage8a4TradeSupport::CompatibleSupport => matching_trades.push(*trade),
            Stage8a4TradeSupport::Unrelated => {}
            Stage8a4TradeSupport::IdentityConflict => {
                return reducer_non_exact_with_trade_count(
                    Stage8a4OutcomeKind::Conflict,
                    Stage8a4ReconciliationReason::TradeIdentityConflict,
                    &context,
                    &policy,
                    &admission,
                    matching_trades.len(),
                )
            }
        }
    }
    let trade_qty: Quantity = matching_trades.iter().map(|trade| trade.qty).sum();
    if trade_qty != selected.filled_qty {
        return reducer_non_exact_with_trade_count(
            Stage8a4OutcomeKind::Conflict,
            Stage8a4ReconciliationReason::TradeQuantityContradiction,
            &context,
            &policy,
            &admission,
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
            return reducer_non_exact_with_trade_count(
                outcome,
                reason,
                &context,
                &policy,
                &admission,
                matching_trades.len(),
            );
        }
    };
    let selected_order_binding_sha256 =
        digest_serializable(b"stage8a4-selected-order-v1", selected);
    let material_trades = matching_trades
        .iter()
        .map(|trade| Stage8a4MaterialTradeBinding::from(*trade))
        .collect::<Vec<_>>();
    let trade_summary_binding_sha256 = digest_serializable(
        b"stage8a4-deduplicated-material-trades-v2",
        &material_trades,
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
        context,
        policy,
    })
}

fn account_safety_summary(
    truth: &BrokerTruthSnapshot,
    target: &InstrumentId,
) -> Stage8a4PrivateAccountSafety {
    let summary = truth.summarize_for_instrument(target);
    Stage8a4PrivateAccountSafety {
        account_active_orders_count: summary.account_active_orders_count,
        account_unknown_orders_count: summary.account_unknown_orders_count,
        account_orphan_orders_count: summary.account_orphan_orders_count,
        account_open_positions_count: summary.account_open_positions_count,
        target_active_orders_count: summary.target_active_orders_count,
        target_unknown_orders_count: summary.target_unknown_orders_count,
        target_terminal_orders_count: summary.target_terminal_orders_count,
        target_inconsistent_orders_count: summary.target_inconsistent_orders_count,
        target_open_positions_count: summary.target_open_positions_count,
        other_symbol_active_orders_count: summary.other_symbol_active_orders_count,
    }
}

fn private_exact_lookup_binding(value: &Stage8a4PrivateExactLookup) -> String {
    let parts = match value {
        Stage8a4PrivateExactLookup::NotAttempted => vec!["not_attempted".to_string()],
        Stage8a4PrivateExactLookup::Succeeded(observation) => vec![
            "succeeded".to_string(),
            observation.timing.request_started_at.to_rfc3339(),
            observation.timing.response_received_at.to_rfc3339(),
            digest_serializable(b"stage8a4-exact-order-observation-v2", &observation.order),
        ],
        Stage8a4PrivateExactLookup::DocumentedNotFound {
            timing,
            documented_status_category,
        } => vec![
            "documented_not_found".to_string(),
            timing.request_started_at.to_rfc3339(),
            timing.response_received_at.to_rfc3339(),
            documented_status_category.clone(),
        ],
        Stage8a4PrivateExactLookup::Unavailable {
            timing,
            failure_category,
        } => vec![
            "unavailable".to_string(),
            timing.request_started_at.to_rfc3339(),
            timing.response_received_at.to_rfc3339(),
            failure_category.clone(),
        ],
        Stage8a4PrivateExactLookup::DecodeFailure {
            timing,
            response_status_category,
            response_binding_sha256,
        } => vec![
            "decode_failure".to_string(),
            timing.request_started_at.to_rfc3339(),
            timing.response_received_at.to_rfc3339(),
            response_status_category.clone(),
            response_binding_sha256.clone(),
        ],
        Stage8a4PrivateExactLookup::Stale {
            timing,
            stale_observation_binding_sha256,
        } => vec![
            "stale".to_string(),
            timing.request_started_at.to_rfc3339(),
            timing.response_received_at.to_rfc3339(),
            stale_observation_binding_sha256.clone(),
        ],
    };
    digest_parts(
        b"stage8a4-private-exact-lookup-v1",
        &parts.iter().map(String::as_bytes).collect::<Vec<_>>(),
    )
}

fn account_safety_binding(value: &Stage8a4PrivateAccountSafety) -> String {
    digest_parts(
        b"stage8a4-account-safety-v1",
        &[
            &value.account_active_orders_count.to_be_bytes(),
            &value.account_unknown_orders_count.to_be_bytes(),
            &value.account_orphan_orders_count.to_be_bytes(),
            &value.account_open_positions_count.to_be_bytes(),
            &value.target_active_orders_count.to_be_bytes(),
            &value.target_unknown_orders_count.to_be_bytes(),
            &value.target_terminal_orders_count.to_be_bytes(),
            &value.target_inconsistent_orders_count.to_be_bytes(),
            &value.target_open_positions_count.to_be_bytes(),
            &value.other_symbol_active_orders_count.to_be_bytes(),
        ],
    )
}

fn validate_timing(
    timing: &Stage8a4SourceTiming,
    context: &Stage8a4DurableRequestContext,
    policy: &Stage8a4ReconciliationPolicy,
    attempt: &Stage8a4AdmissionAttemptBinding,
) -> Result<(), Box<Stage8a4ReconciliationDiagnostic>> {
    if timing.request_started_at < context.possible_effect_at
        || timing.response_received_at < timing.request_started_at
    {
        return Err(admission_error(
            Stage8a4OutcomeKind::StillUnknown,
            Stage8a4ReconciliationReason::SourceIncomplete,
            attempt,
        ));
    }
    if timing.response_received_at > policy.trusted_now
        || policy.trusted_now - timing.response_received_at > policy.max_source_age
    {
        return Err(admission_error(
            Stage8a4OutcomeKind::StillUnknown,
            Stage8a4ReconciliationReason::SourceStale,
            attempt,
        ));
    }
    Ok(())
}

fn validate_trade_intervals(
    evidence: &Stage8a4SourceEvidence,
    context: &Stage8a4DurableRequestContext,
    policy: &Stage8a4ReconciliationPolicy,
    attempt: &Stage8a4AdmissionAttemptBinding,
) -> Result<(), Box<Stage8a4ReconciliationDiagnostic>> {
    if evidence.trades.intervals.is_empty()
        || evidence.trades.intervals.len() > policy.max_trade_intervals
        || policy.max_interval_split_depth == 0
    {
        return Err(admission_error(
            Stage8a4OutcomeKind::StillUnknown,
            Stage8a4ReconciliationReason::SourceIncomplete,
            attempt,
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
            attempt,
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
                attempt,
            ));
        }
        if interval.returned_count >= interval.requested_limit {
            // This computes the only deterministic next acquisition split.
            // The saturated observation itself is never admitted as complete.
            let _next_split = deterministic_interval_split(interval, interval.split_depth, policy);
            return Err(admission_error(
                Stage8a4OutcomeKind::StillUnknown,
                Stage8a4ReconciliationReason::SourceIncomplete,
                attempt,
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
            attempt,
        ));
    }
    Ok(())
}

fn validate_cross_source_skew(
    evidence: &Stage8a4SourceEvidence,
    policy: &Stage8a4ReconciliationPolicy,
    attempt: &Stage8a4AdmissionAttemptBinding,
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
    if let Some(timing) = evidence.exact_lookup.timing() {
        received.push(timing.response_received_at);
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
            attempt,
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
    IdentityConflict,
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
        return Tier3Match::NoMatch;
    }
    if selected_order_identity(order, context).is_err() {
        Tier3Match::IdentityConflict
    } else {
        Tier3Match::Match
    }
}

fn selected_order_identity(
    order: &BrokerOrderSnapshot,
    context: &Stage8a4DurableRequestContext,
) -> Result<(), ()> {
    if order
        .client_order_id
        .as_ref()
        .is_some_and(|value| value != &context.client_order_id)
        || context
            .known_broker_order_id
            .as_ref()
            .zip(order.broker_order_id.as_ref())
            .is_some_and(|(expected, observed)| expected != observed)
    {
        Err(())
    } else {
        Ok(())
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

enum Stage8a4TradeSupport {
    CompatibleSupport,
    Unrelated,
    IdentityConflict,
}

fn classify_trade_support(
    trade: &BrokerTradeSnapshot,
    order: &BrokerOrderSnapshot,
    context: &Stage8a4DurableRequestContext,
) -> Stage8a4TradeSupport {
    let broker_match = trade
        .broker_order_id
        .as_ref()
        .zip(order.broker_order_id.as_ref())
        .is_some_and(|(left, right)| left == right);
    let client_match = trade
        .client_order_id
        .as_ref()
        .zip(order.client_order_id.as_ref())
        .is_some_and(|(left, right)| left == right);
    let broker_conflict = trade
        .broker_order_id
        .as_ref()
        .zip(order.broker_order_id.as_ref())
        .is_some_and(|(left, right)| left != right);
    let client_conflict = trade
        .client_order_id
        .as_ref()
        .zip(order.client_order_id.as_ref())
        .is_some_and(|(left, right)| left != right);
    if !(broker_match || client_match) {
        return Stage8a4TradeSupport::Unrelated;
    }
    let durable_broker_conflict = trade
        .broker_order_id
        .as_ref()
        .zip(context.known_broker_order_id.as_ref())
        .is_some_and(|(left, right)| left != right);
    let durable_client_conflict = trade
        .client_order_id
        .as_ref()
        .is_some_and(|value| value != &context.client_order_id);
    if broker_conflict
        || client_conflict
        || durable_broker_conflict
        || durable_client_conflict
        || trade.account_id != order.account_id
        || !exact_instrument_matches(&trade.instrument, &order.instrument)
        || trade.side != order.side
    {
        return Stage8a4TradeSupport::IdentityConflict;
    }
    Stage8a4TradeSupport::CompatibleSupport
}

#[derive(Serialize)]
struct Stage8a4MaterialTradeBinding<'a> {
    account_id: &'a BrokerAccountId,
    broker_trade_id: &'a broker_core::BrokerTradeId,
    broker_order_id: &'a Option<BrokerOrderId>,
    client_order_id: &'a Option<ClientOrderId>,
    instrument: &'a InstrumentId,
    side: OrderSide,
    qty: Quantity,
    price: Price,
    gross_amount: &'a Option<broker_core::Money>,
    commission: &'a Option<broker_core::Money>,
    broker_asset_id: &'a Option<String>,
    board: &'a Option<String>,
    expiration_date: &'a Option<chrono::NaiveDate>,
    source_ts: &'a DateTime<Utc>,
}

impl<'a> From<&'a BrokerTradeSnapshot> for Stage8a4MaterialTradeBinding<'a> {
    fn from(trade: &'a BrokerTradeSnapshot) -> Self {
        Self {
            account_id: &trade.account_id,
            broker_trade_id: &trade.broker_trade_id,
            broker_order_id: &trade.broker_order_id,
            client_order_id: &trade.client_order_id,
            instrument: &trade.instrument,
            side: trade.side,
            qty: trade.qty,
            price: trade.price,
            gross_amount: &trade.gross_amount,
            commission: &trade.commission,
            broker_asset_id: &trade.broker_asset_id,
            board: &trade.board,
            expiration_date: &trade.expiration_date,
            source_ts: &trade.source_ts,
        }
    }
}

fn source_timing_binding(digest: &mut Sha256, timing: &Stage8a4SourceTiming) {
    digest.update(timing.request_started_at.to_rfc3339().as_bytes());
    digest.update(timing.response_received_at.to_rfc3339().as_bytes());
}

fn source_evidence_binding(
    evidence: &Stage8a4SourceEvidence,
    canonical_truth_sha256: &str,
) -> String {
    let mut digest = Sha256::new();
    digest.update(b"stage8a4-source-evidence-binding-v2");
    digest.update(canonical_truth_sha256.as_bytes());
    digest.update(evidence.canonical_truth_payload_sha256.as_bytes());
    digest.update(evidence.acquisition_policy_sha256.as_bytes());
    source_timing_binding(&mut digest, &evidence.orders.timing);
    source_timing_binding(&mut digest, &evidence.positions.timing);
    match &evidence.instruments {
        Stage8a4InstrumentCompletenessEvidence::ExactTargetResolved { timing } => {
            digest.update(b"exact-target-resolved");
            source_timing_binding(&mut digest, timing);
        }
        Stage8a4InstrumentCompletenessEvidence::FullRegistryCursorExhausted { timing } => {
            digest.update(b"full-registry-cursor-exhausted");
            source_timing_binding(&mut digest, timing);
        }
    }
    let mut intervals = evidence.trades.intervals.iter().collect::<Vec<_>>();
    intervals.sort_by_key(|item| (item.start_inclusive, item.end_exclusive));
    digest.update(evidence.trades.interval_coverage_sha256.as_bytes());
    for interval in intervals {
        digest.update(interval.start_inclusive.to_rfc3339().as_bytes());
        digest.update(interval.end_exclusive.to_rfc3339().as_bytes());
        digest.update(interval.requested_limit.to_be_bytes());
        digest.update(interval.returned_count.to_be_bytes());
        digest.update(interval.request_started_at.to_rfc3339().as_bytes());
        digest.update(interval.response_received_at.to_rfc3339().as_bytes());
        digest.update([interval.split_depth]);
    }
    digest.update(private_exact_lookup_binding(&evidence.exact_lookup).as_bytes());
    to_hex(&digest.finalize())
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
    attempt: &Stage8a4AdmissionAttemptBinding,
) -> Box<Stage8a4ReconciliationDiagnostic> {
    let mut diagnostic = non_exact(outcome, reason);
    diagnostic.semantic_binding_sha256 = digest_parts(
        b"stage8a4-bound-admission-failure-semantic-v3",
        &[
            attempt.durable_binding_sha256.as_bytes(),
            attempt.request_id.as_bytes(),
            attempt.policy_binding_sha256.as_bytes(),
            attempt.canonical_truth_sha256.as_bytes(),
            attempt.source_evidence_binding_sha256.as_bytes(),
            format!("{outcome:?}:{reason:?}").as_bytes(),
        ],
    );
    Box::new(diagnostic)
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

fn reducer_non_exact(
    outcome: Stage8a4OutcomeKind,
    reason: Stage8a4ReconciliationReason,
    context: &Stage8a4DurableRequestContext,
    policy: &Stage8a4ReconciliationPolicy,
    admission: &Stage8a4FreshTruthAdmission,
) -> Stage8a4ReconciliationDiagnostic {
    let mut diagnostic = non_exact(outcome, reason);
    diagnostic.account_active_orders_count = admission.account_active_orders_count;
    diagnostic.target_active_orders_count = admission.target_active_orders_count;
    diagnostic.semantic_binding_sha256 = digest_parts(
        b"stage8a4-bound-non-exact-semantic-v2",
        &[
            context.durable_binding_sha256.as_bytes(),
            context.request_id.to_string().as_bytes(),
            policy.policy_binding_sha256.as_bytes(),
            admission.truth_binding_sha256.as_bytes(),
            admission.source_evidence_binding_sha256.as_bytes(),
            format!("{outcome:?}:{reason:?}").as_bytes(),
        ],
    );
    diagnostic
}

fn reducer_non_exact_with_trade_count(
    outcome: Stage8a4OutcomeKind,
    reason: Stage8a4ReconciliationReason,
    context: &Stage8a4DurableRequestContext,
    policy: &Stage8a4ReconciliationPolicy,
    admission: &Stage8a4FreshTruthAdmission,
    matching_trade_count: usize,
) -> Stage8a4ReconciliationDiagnostic {
    let mut diagnostic = reducer_non_exact(outcome, reason, context, policy, admission);
    diagnostic.matching_trade_count = matching_trade_count;
    diagnostic
}

fn digest_serializable<T: Serialize + ?Sized>(domain: &[u8], value: &T) -> String {
    let encoded = serde_json::to_vec(value).expect("canonical Stage 8A-4 value serializes");
    digest_parts(domain, &[&encoded])
}

fn canonical_truth_binding(truth: &BrokerTruthSnapshot) -> String {
    fn sorted_hashes<T: Serialize>(domain: &[u8], values: &[T]) -> Vec<String> {
        let mut hashes = values
            .iter()
            .map(|value| digest_serializable(domain, value))
            .collect::<Vec<_>>();
        hashes.sort();
        hashes
    }

    let orders = sorted_hashes(b"stage8a4-canonical-order-v2", &truth.orders);
    let positions = sorted_hashes(b"stage8a4-canonical-position-v2", &truth.positions);
    let trades = sorted_hashes(b"stage8a4-canonical-raw-trade-v2", &truth.trades);
    let instruments = sorted_hashes(b"stage8a4-canonical-instrument-v2", &truth.instruments);
    digest_serializable(
        b"stage8a4-admitted-canonical-truth-multiset-v2",
        &(
            &truth.account_id,
            &orders,
            &positions,
            &truth.cash,
            &trades,
            &instruments,
            &truth.received_ts,
        ),
    )
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
