//! Stage 8A-4 I2 private, pure durable-composition candidate builder.
//!
//! This module deliberately has no public exports and no journal backend,
//! append, CAS, seal, ACK, Redis, FINAM or execution dependency. Its output is
//! an owned candidate for the later I3 writer slice, not write authority.
//!
//! ```compile_fail
//! use finam_gateway::Stage8a4I2DurableCandidate;
//! ```
//!
//! ```compile_fail
//! use finam_gateway::Stage8a4ReconciliationDiagnostic;
//! fn diagnostic_is_not_builder_input(value: Stage8a4ReconciliationDiagnostic) {
//!     let _ = build_private_durable_candidate(value);
//! }
//! ```

use super::{
    account_safety_binding, digest_parts, Stage8a4AuthoritativeReconciliationOutcome,
    Stage8a4ExactLifecycle, Stage8a4FillEffect, Stage8a4OutcomeKind, Stage8a4PrivateExactLookup,
};
use broker_core::{BrokerOrderId, BrokerOrderSnapshot, BrokerTradeSnapshot};
use serde::Serialize;
use std::collections::BTreeSet;
use strategy_runtime_core::{
    Stage6CancelOutcomeV1, Stage6DurableActionKind, Stage6DurableRequestIdentityV1,
    Stage6JournalEventKind, Stage6JournalEventKindV2, Stage6JournalRecordId, Stage6JournalRecordV1,
    Stage6JournalRecordV2, Stage6LifecycleSequence, Stage6ReconciliationTransitionPayloadV2,
    Stage6RequestFinalDispositionV1, Stage6Sha256Digest, STAGE6_DURABLE_RECORD_SCHEMA_VERSION_V2,
};

const STABLE_KEY_DOMAIN: &[u8] = b"stage8a4-stable-transition-key-v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum PrivateEndpointKind {
    Place,
    Cancel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum PrivateTransitionKind {
    Exact { lifecycle: Stage8a4ExactLifecycle },
    ReconciliationConflictHold,
    ReconciliationStillUnknownHold,
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum PrivateFillEffect {
    Zero,
    Partial { filled_qty: broker_core::Quantity },
    Full { filled_qty: broker_core::Quantity },
}

#[derive(Serialize)]
#[serde(deny_unknown_fields)]
struct PrivateExactOrderObservation {
    order: BrokerOrderSnapshot,
    observation_binding_sha256: Stage6Sha256Digest,
}

#[derive(Serialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
#[allow(clippy::large_enum_variant)]
enum PrivateExactLookupEvidence {
    NotAttempted,
    Succeeded {
        account_id: broker_core::BrokerAccountId,
        queried_broker_order_id: BrokerOrderId,
        durable_request_binding_sha256: Stage6Sha256Digest,
        request_started_at: chrono::DateTime<chrono::Utc>,
        response_received_at: chrono::DateTime<chrono::Utc>,
        exact_order_observation_v2: Box<PrivateExactOrderObservation>,
    },
    DocumentedNotFound {
        account_id: broker_core::BrokerAccountId,
        queried_broker_order_id: BrokerOrderId,
        durable_request_binding_sha256: Stage6Sha256Digest,
        request_started_at: chrono::DateTime<chrono::Utc>,
        response_received_at: chrono::DateTime<chrono::Utc>,
        documented_status_category: String,
    },
    Unavailable {
        account_id: broker_core::BrokerAccountId,
        queried_broker_order_id: BrokerOrderId,
        durable_request_binding_sha256: Stage6Sha256Digest,
        request_started_at: chrono::DateTime<chrono::Utc>,
        response_received_at: chrono::DateTime<chrono::Utc>,
        failure_category: String,
    },
    DecodeFailure {
        account_id: broker_core::BrokerAccountId,
        queried_broker_order_id: BrokerOrderId,
        durable_request_binding_sha256: Stage6Sha256Digest,
        request_started_at: chrono::DateTime<chrono::Utc>,
        response_received_at: chrono::DateTime<chrono::Utc>,
        response_status_category: String,
        response_binding_sha256: Stage6Sha256Digest,
    },
    Stale {
        account_id: broker_core::BrokerAccountId,
        queried_broker_order_id: BrokerOrderId,
        durable_request_binding_sha256: Stage6Sha256Digest,
        request_started_at: chrono::DateTime<chrono::Utc>,
        response_received_at: chrono::DateTime<chrono::Utc>,
        stale_observation_binding_sha256: Stage6Sha256Digest,
    },
}

#[derive(Serialize)]
struct PrivateAccountSafetySummary {
    account_active_orders_count: u32,
    account_unknown_orders_count: u32,
    account_orphan_orders_count: u32,
    account_open_positions_count: u32,
    target_active_orders_count: u32,
    target_unknown_orders_count: u32,
    target_terminal_orders_count: u32,
    target_inconsistent_orders_count: u32,
    target_open_positions_count: u32,
    other_symbol_active_orders_count: u32,
    account_safety_binding_sha256: Stage6Sha256Digest,
}

#[derive(Serialize)]
struct PrivatePreAppendPrecondition {
    expected_stage6_checkpoint_or_frontier_fingerprint: Stage6Sha256Digest,
    expected_recovery_seal_generation: u64,
    expected_recovery_seal_fingerprint: Stage6Sha256Digest,
    expected_request_state_fingerprint: Stage6Sha256Digest,
}

#[derive(Serialize)]
struct PrivateSuffixManifestEntry {
    ordinal: u16,
    event_kind: Stage6JournalEventKind,
    journal_record_id: Stage6JournalRecordId,
    lifecycle_sequence: Stage6LifecycleSequence,
    canonical_payload_sha256: Stage6Sha256Digest,
    canonical_record_sha256: Stage6Sha256Digest,
}

#[derive(Serialize)]
struct PrivateSuffixManifest {
    entries: Vec<PrivateSuffixManifestEntry>,
}

#[derive(Serialize)]
struct PrivateTransitionPayload {
    stable_transition_key_sha256: Stage6Sha256Digest,
    durable_request_binding_sha256: Stage6Sha256Digest,
    private_authoritative_outcome_binding_sha256: Stage6Sha256Digest,
    endpoint_kind: PrivateEndpointKind,
    transition_kind: PrivateTransitionKind,
    exact_lookup_evidence: PrivateExactLookupEvidence,
    broker_order_fact: Option<BrokerOrderSnapshot>,
    material_trade_facts: Vec<BrokerTradeSnapshot>,
    fill_effect: PrivateFillEffect,
    account_safety_summary: PrivateAccountSafetySummary,
    pre_append_precondition: PrivatePreAppendPrecondition,
    deterministic_suffix_manifest: PrivateSuffixManifest,
}

#[derive(Serialize)]
struct PrivateJournalRecordV2Wire {
    schema_version: u16,
    journal_record_id: Stage6JournalRecordId,
    lifecycle_sequence: Stage6LifecycleSequence,
    previous_record_id: Option<Stage6JournalRecordId>,
    causal_parent_id: Option<Stage6JournalRecordId>,
    durable_request_identity: Stage6DurableRequestIdentityV1,
    event_kind: Stage6JournalEventKindV2,
    payload: Stage6ReconciliationTransitionPayloadV2,
    canonical_payload_sha256: Stage6Sha256Digest,
    source_evidence_sha256: Stage6Sha256Digest,
}

struct PrivateJournalCursor {
    previous_record_id: Stage6JournalRecordId,
    previous_lifecycle_sequence: Stage6LifecycleSequence,
}

struct PrivatePreAppendEvidence {
    expected_stage6_checkpoint_or_frontier_fingerprint: Stage6Sha256Digest,
    expected_recovery_seal_generation: u64,
    expected_recovery_seal_fingerprint: Stage6Sha256Digest,
    expected_request_state_fingerprint: Stage6Sha256Digest,
}

struct Stage8a4I2CompositionInput {
    identity: Stage6DurableRequestIdentityV1,
    cursor: PrivateJournalCursor,
    pre_append: PrivatePreAppendEvidence,
    outcome: Stage8a4AuthoritativeReconciliationOutcome,
}

struct Stage8a4I2DurableCandidate {
    transition_record: Stage6JournalRecordV2,
    suffix_records: Vec<Stage6JournalRecordV1>,
}

#[derive(Debug)]
enum Stage8a4I2CompositionError {
    IdentityMismatch,
    InvalidDigest,
    InvalidSequence,
    MissingQueriedBrokerOrderId,
    ExactLookupContradiction,
    MaterialTradeBrokerOrderConflict,
    CountOverflow,
    V1ProjectionFailed,
    V2ValidationFailed(strategy_runtime_core::Stage6ReconciliationV2Error),
}

fn build_private_durable_candidate(
    input: Stage8a4I2CompositionInput,
) -> Result<Stage8a4I2DurableCandidate, Stage8a4I2CompositionError> {
    validate_identity_binding(&input.identity, &input.outcome)?;
    let next_sequence = next_sequence(input.cursor.previous_lifecycle_sequence, 1)?;
    let transition_kind = effective_transition_kind(&input.outcome);
    let endpoint_kind = match input.identity.action() {
        Stage6DurableActionKind::Place => PrivateEndpointKind::Place,
        Stage6DurableActionKind::Cancel => PrivateEndpointKind::Cancel,
    };
    let durable_binding = parse_digest(&input.outcome.context.durable_binding_sha256)?;
    let outcome_binding = parse_digest(&input.outcome.private_outcome_binding_sha256)?;
    let transition_bytes =
        serde_json::to_vec(&transition_kind).map_err(|_| invalid_v2_decode_error())?;
    let stable_key = parse_digest(&digest_parts(
        STABLE_KEY_DOMAIN,
        &[
            durable_binding.as_str().as_bytes(),
            outcome_binding.as_str().as_bytes(),
            &transition_bytes,
        ],
    ))?;
    let v2_record_id = derive_record_id(input.identity.strategy_request_id(), next_sequence)?;
    let source_evidence = parse_digest(&input.outcome.source_evidence_binding_sha256)?;
    let suffix_records = build_v1_suffix(
        &input.identity,
        &input.outcome,
        transition_kind,
        next_sequence,
        &v2_record_id,
        &source_evidence,
    )?;
    let suffix_manifest = build_suffix_manifest(&suffix_records)?;
    let payload = PrivateTransitionPayload {
        stable_transition_key_sha256: stable_key,
        durable_request_binding_sha256: durable_binding.clone(),
        private_authoritative_outcome_binding_sha256: outcome_binding,
        endpoint_kind,
        transition_kind,
        exact_lookup_evidence: map_exact_lookup(&input.outcome, durable_binding)?,
        broker_order_fact: input.outcome.selected_order.clone(),
        material_trade_facts: input.outcome.material_trades.clone(),
        fill_effect: map_fill(input.outcome.fill),
        account_safety_summary: map_account_safety(&input.outcome)?,
        pre_append_precondition: PrivatePreAppendPrecondition {
            expected_stage6_checkpoint_or_frontier_fingerprint: input
                .pre_append
                .expected_stage6_checkpoint_or_frontier_fingerprint,
            expected_recovery_seal_generation: input.pre_append.expected_recovery_seal_generation,
            expected_recovery_seal_fingerprint: input.pre_append.expected_recovery_seal_fingerprint,
            expected_request_state_fingerprint: input.pre_append.expected_request_state_fingerprint,
        },
        deterministic_suffix_manifest: suffix_manifest,
    };
    let payload_wire_bytes = serde_json::to_vec(&payload).map_err(|_| invalid_v2_decode_error())?;
    let payload: Stage6ReconciliationTransitionPayloadV2 =
        serde_json::from_slice(&payload_wire_bytes).map_err(|_| invalid_v2_decode_error())?;
    let payload_bytes = serde_json::to_vec(&payload).map_err(|_| invalid_v2_decode_error())?;
    let wire = PrivateJournalRecordV2Wire {
        schema_version: STAGE6_DURABLE_RECORD_SCHEMA_VERSION_V2,
        journal_record_id: v2_record_id,
        lifecycle_sequence: next_sequence,
        previous_record_id: Some(input.cursor.previous_record_id.clone()),
        causal_parent_id: Some(input.cursor.previous_record_id),
        durable_request_identity: input.identity,
        event_kind: Stage6JournalEventKindV2::ReconciliationTransitionApplied,
        payload,
        canonical_payload_sha256: parse_digest(&sha256_hex(&payload_bytes))?,
        source_evidence_sha256: source_evidence,
    };
    let canonical = serde_json::to_vec(&wire).map_err(|_| invalid_v2_decode_error())?;
    let transition_record = Stage6JournalRecordV2::decode_canonical(&canonical)
        .map_err(Stage8a4I2CompositionError::V2ValidationFailed)?;
    Ok(Stage8a4I2DurableCandidate {
        transition_record,
        suffix_records: suffix_records
            .into_iter()
            .map(|(_, record)| record)
            .collect(),
    })
}

fn validate_identity_binding(
    identity: &Stage6DurableRequestIdentityV1,
    outcome: &Stage8a4AuthoritativeReconciliationOutcome,
) -> Result<(), Stage8a4I2CompositionError> {
    let context = &outcome.context;
    if identity.strategy_request_id() != context.request_id
        || identity.durable_client_order_id() != &context.client_order_id
        || identity.account_id() != &context.account_id
        || identity.instrument() != &context.instrument
        || (identity.action() == Stage6DurableActionKind::Cancel
            && identity.target_broker_order_id() != context.known_broker_order_id.as_ref())
    {
        return Err(Stage8a4I2CompositionError::IdentityMismatch);
    }
    Ok(())
}

fn effective_transition_kind(
    outcome: &Stage8a4AuthoritativeReconciliationOutcome,
) -> PrivateTransitionKind {
    match &outcome.exact_lookup {
        Stage8a4PrivateExactLookup::DocumentedNotFound { .. } => {
            if outcome.selected_order.is_some() {
                PrivateTransitionKind::ReconciliationConflictHold
            } else {
                PrivateTransitionKind::ReconciliationStillUnknownHold
            }
        }
        Stage8a4PrivateExactLookup::Unavailable { .. }
        | Stage8a4PrivateExactLookup::DecodeFailure { .. }
        | Stage8a4PrivateExactLookup::Stale { .. } => {
            PrivateTransitionKind::ReconciliationStillUnknownHold
        }
        Stage8a4PrivateExactLookup::NotAttempted | Stage8a4PrivateExactLookup::Succeeded(_) => {
            match outcome.outcome_kind {
                Stage8a4OutcomeKind::ExactOrderState => PrivateTransitionKind::Exact {
                    lifecycle: outcome.lifecycle.expect("exact outcome owns lifecycle"),
                },
                Stage8a4OutcomeKind::Conflict => PrivateTransitionKind::ReconciliationConflictHold,
                Stage8a4OutcomeKind::StillUnknown => {
                    PrivateTransitionKind::ReconciliationStillUnknownHold
                }
            }
        }
    }
}

fn map_exact_lookup(
    outcome: &Stage8a4AuthoritativeReconciliationOutcome,
    durable_binding: Stage6Sha256Digest,
) -> Result<PrivateExactLookupEvidence, Stage8a4I2CompositionError> {
    let account_id = outcome.context.account_id.clone();
    let queried = || {
        outcome
            .context
            .known_broker_order_id
            .clone()
            .ok_or(Stage8a4I2CompositionError::MissingQueriedBrokerOrderId)
    };
    Ok(match &outcome.exact_lookup {
        Stage8a4PrivateExactLookup::NotAttempted => PrivateExactLookupEvidence::NotAttempted,
        Stage8a4PrivateExactLookup::Succeeded(observation) => {
            let broker_order_id = observation
                .order
                .broker_order_id
                .clone()
                .ok_or(Stage8a4I2CompositionError::ExactLookupContradiction)?;
            if outcome.context.known_broker_order_id.as_ref() != Some(&broker_order_id) {
                return Err(Stage8a4I2CompositionError::ExactLookupContradiction);
            }
            PrivateExactLookupEvidence::Succeeded {
                account_id,
                queried_broker_order_id: broker_order_id,
                durable_request_binding_sha256: durable_binding,
                request_started_at: observation.timing.request_started_at,
                response_received_at: observation.timing.response_received_at,
                exact_order_observation_v2: Box::new(PrivateExactOrderObservation {
                    observation_binding_sha256: parse_digest(&digest_parts(
                        b"stage8a4-exact-order-observation-v2",
                        &[&serde_json::to_vec(&observation.order)
                            .map_err(|_| invalid_v2_decode_error())?],
                    ))?,
                    order: observation.order.clone(),
                }),
            }
        }
        Stage8a4PrivateExactLookup::DocumentedNotFound {
            timing,
            documented_status_category,
        } => PrivateExactLookupEvidence::DocumentedNotFound {
            account_id,
            queried_broker_order_id: queried()?,
            durable_request_binding_sha256: durable_binding,
            request_started_at: timing.request_started_at,
            response_received_at: timing.response_received_at,
            documented_status_category: documented_status_category.clone(),
        },
        Stage8a4PrivateExactLookup::Unavailable {
            timing,
            failure_category,
        } => PrivateExactLookupEvidence::Unavailable {
            account_id,
            queried_broker_order_id: queried()?,
            durable_request_binding_sha256: durable_binding,
            request_started_at: timing.request_started_at,
            response_received_at: timing.response_received_at,
            failure_category: failure_category.clone(),
        },
        Stage8a4PrivateExactLookup::DecodeFailure {
            timing,
            response_status_category,
            response_binding_sha256,
        } => PrivateExactLookupEvidence::DecodeFailure {
            account_id,
            queried_broker_order_id: queried()?,
            durable_request_binding_sha256: durable_binding,
            request_started_at: timing.request_started_at,
            response_received_at: timing.response_received_at,
            response_status_category: response_status_category.clone(),
            response_binding_sha256: parse_digest(response_binding_sha256)?,
        },
        Stage8a4PrivateExactLookup::Stale {
            timing,
            stale_observation_binding_sha256,
        } => PrivateExactLookupEvidence::Stale {
            account_id,
            queried_broker_order_id: queried()?,
            durable_request_binding_sha256: durable_binding,
            request_started_at: timing.request_started_at,
            response_received_at: timing.response_received_at,
            stale_observation_binding_sha256: parse_digest(stale_observation_binding_sha256)?,
        },
    })
}

fn map_fill(fill: Option<Stage8a4FillEffect>) -> PrivateFillEffect {
    match fill.unwrap_or(Stage8a4FillEffect::Zero) {
        Stage8a4FillEffect::Zero => PrivateFillEffect::Zero,
        Stage8a4FillEffect::Partial { filled_qty } => PrivateFillEffect::Partial { filled_qty },
        Stage8a4FillEffect::Full { filled_qty } => PrivateFillEffect::Full { filled_qty },
    }
}

fn map_account_safety(
    outcome: &Stage8a4AuthoritativeReconciliationOutcome,
) -> Result<PrivateAccountSafetySummary, Stage8a4I2CompositionError> {
    let value = &outcome.account_safety;
    Ok(PrivateAccountSafetySummary {
        account_active_orders_count: to_u32(value.account_active_orders_count)?,
        account_unknown_orders_count: to_u32(value.account_unknown_orders_count)?,
        account_orphan_orders_count: to_u32(value.account_orphan_orders_count)?,
        account_open_positions_count: to_u32(value.account_open_positions_count)?,
        target_active_orders_count: to_u32(value.target_active_orders_count)?,
        target_unknown_orders_count: to_u32(value.target_unknown_orders_count)?,
        target_terminal_orders_count: to_u32(value.target_terminal_orders_count)?,
        target_inconsistent_orders_count: to_u32(value.target_inconsistent_orders_count)?,
        target_open_positions_count: to_u32(value.target_open_positions_count)?,
        other_symbol_active_orders_count: to_u32(value.other_symbol_active_orders_count)?,
        account_safety_binding_sha256: parse_digest(&account_safety_binding(value))?,
    })
}

fn build_v1_suffix(
    identity: &Stage6DurableRequestIdentityV1,
    outcome: &Stage8a4AuthoritativeReconciliationOutcome,
    transition: PrivateTransitionKind,
    v2_sequence: Stage6LifecycleSequence,
    v2_record_id: &Stage6JournalRecordId,
    source_evidence: &Stage6Sha256Digest,
) -> Result<Vec<(Stage6JournalEventKind, Stage6JournalRecordV1)>, Stage8a4I2CompositionError> {
    let PrivateTransitionKind::Exact { lifecycle } = transition else {
        return Ok(Vec::new());
    };
    if identity.action() == Stage6DurableActionKind::Cancel
        && lifecycle == Stage8a4ExactLifecycle::Working
    {
        return Ok(Vec::new());
    }
    let mut records = Vec::new();
    let mut previous = v2_record_id.clone();
    let mut ordinal = 1_u64;
    match identity.action() {
        Stage6DurableActionKind::Place => {
            let selected_order_id = outcome
                .selected_order
                .as_ref()
                .and_then(|order| order.broker_order_id.clone());
            let mut trades = outcome.material_trades.iter().collect::<Vec<_>>();
            trades.sort_by(|left, right| {
                left.broker_trade_id
                    .as_str()
                    .cmp(right.broker_trade_id.as_str())
            });
            let material_broker_ids = trades
                .iter()
                .filter_map(|trade| trade.broker_order_id.as_ref())
                .map(BrokerOrderId::as_str)
                .collect::<BTreeSet<_>>();
            let projected_trade_order_id = match selected_order_id.as_ref() {
                Some(order_id) => {
                    if material_broker_ids
                        .iter()
                        .any(|candidate| *candidate != order_id.as_str())
                    {
                        return Err(Stage8a4I2CompositionError::MaterialTradeBrokerOrderConflict);
                    }
                    Some(order_id.clone())
                }
                None => {
                    if material_broker_ids.len() > 1 {
                        return Err(Stage8a4I2CompositionError::MaterialTradeBrokerOrderConflict);
                    }
                    trades
                        .iter()
                        .find_map(|trade| trade.broker_order_id.clone())
                }
            };
            if let Some(order_id) = selected_order_id {
                let record = Stage6JournalRecordV1::broker_order_observed(
                    identity.clone(),
                    order_id,
                    next_sequence(v2_sequence, ordinal)?,
                    Some(previous.clone()),
                    source_evidence.clone(),
                )
                .map_err(|_| Stage8a4I2CompositionError::V1ProjectionFailed)?;
                push_suffix_record(
                    &mut records,
                    &mut previous,
                    &mut ordinal,
                    Stage6JournalEventKind::BrokerOrderObserved,
                    record,
                );
            }
            if let Some(order_id) = projected_trade_order_id {
                for trade in trades {
                    if trade.broker_order_id.as_ref() != Some(&order_id) {
                        continue;
                    }
                    let record = Stage6JournalRecordV1::broker_trade_observed(
                        identity.clone(),
                        trade.broker_trade_id.clone(),
                        order_id.clone(),
                        next_sequence(v2_sequence, ordinal)?,
                        Some(previous.clone()),
                        source_evidence.clone(),
                    )
                    .map_err(|_| Stage8a4I2CompositionError::V1ProjectionFailed)?;
                    push_suffix_record(
                        &mut records,
                        &mut previous,
                        &mut ordinal,
                        Stage6JournalEventKind::BrokerTradeObserved,
                        record,
                    );
                }
            }
            let disposition = if lifecycle == Stage8a4ExactLifecycle::TerminalRejected {
                Stage6RequestFinalDispositionV1::Rejected
            } else {
                Stage6RequestFinalDispositionV1::Completed
            };
            let record = Stage6JournalRecordV1::request_finalized(
                identity.clone(),
                disposition,
                next_sequence(v2_sequence, ordinal)?,
                Some(previous),
                source_evidence.clone(),
            )
            .map_err(|_| Stage8a4I2CompositionError::V1ProjectionFailed)?;
            records.push((Stage6JournalEventKind::RequestFinalized, record));
        }
        Stage6DurableActionKind::Cancel => {
            let target = identity
                .target_broker_order_id()
                .cloned()
                .ok_or(Stage8a4I2CompositionError::IdentityMismatch)?;
            let cancel_outcome = match lifecycle {
                Stage8a4ExactLifecycle::TerminalFilled => Stage6CancelOutcomeV1::ExecutionObserved,
                Stage8a4ExactLifecycle::TerminalRejected => {
                    Stage6CancelOutcomeV1::AlreadyTerminalNonExecution
                }
                Stage8a4ExactLifecycle::TerminalCancelled => Stage6CancelOutcomeV1::Canceled,
                Stage8a4ExactLifecycle::TerminalExpired => {
                    Stage6CancelOutcomeV1::AlreadyTerminalNonExecution
                }
                Stage8a4ExactLifecycle::Working => unreachable!("handled above"),
            };
            let record = Stage6JournalRecordV1::cancel_outcome_observed(
                identity.clone(),
                target,
                cancel_outcome,
                next_sequence(v2_sequence, ordinal)?,
                Some(previous.clone()),
                source_evidence.clone(),
            )
            .map_err(|_| Stage8a4I2CompositionError::V1ProjectionFailed)?;
            push_suffix_record(
                &mut records,
                &mut previous,
                &mut ordinal,
                Stage6JournalEventKind::CancelOutcomeObserved,
                record,
            );
            let record = Stage6JournalRecordV1::request_finalized(
                identity.clone(),
                Stage6RequestFinalDispositionV1::Completed,
                next_sequence(v2_sequence, ordinal)?,
                Some(previous),
                source_evidence.clone(),
            )
            .map_err(|_| Stage8a4I2CompositionError::V1ProjectionFailed)?;
            records.push((Stage6JournalEventKind::RequestFinalized, record));
        }
    }
    Ok(records)
}

fn push_suffix_record(
    records: &mut Vec<(Stage6JournalEventKind, Stage6JournalRecordV1)>,
    previous: &mut Stage6JournalRecordId,
    ordinal: &mut u64,
    kind: Stage6JournalEventKind,
    record: Stage6JournalRecordV1,
) {
    *previous = record.journal_record_id().clone();
    *ordinal += 1;
    records.push((kind, record));
}

fn build_suffix_manifest(
    records: &[(Stage6JournalEventKind, Stage6JournalRecordV1)],
) -> Result<PrivateSuffixManifest, Stage8a4I2CompositionError> {
    let entries = records
        .iter()
        .enumerate()
        .map(|(index, (kind, record))| {
            Ok(PrivateSuffixManifestEntry {
                ordinal: u16::try_from(index)
                    .map_err(|_| Stage8a4I2CompositionError::CountOverflow)?,
                event_kind: *kind,
                journal_record_id: record.journal_record_id().clone(),
                lifecycle_sequence: record.lifecycle_sequence(),
                canonical_payload_sha256: record.canonical_payload_sha256().clone(),
                canonical_record_sha256: parse_digest(&sha256_hex(&record.encode_canonical()))?,
            })
        })
        .collect::<Result<Vec<_>, Stage8a4I2CompositionError>>()?;
    Ok(PrivateSuffixManifest { entries })
}

fn next_sequence(
    previous: Stage6LifecycleSequence,
    offset: u64,
) -> Result<Stage6LifecycleSequence, Stage8a4I2CompositionError> {
    previous
        .get()
        .checked_add(offset)
        .ok_or(Stage8a4I2CompositionError::InvalidSequence)
        .and_then(|value| {
            Stage6LifecycleSequence::new(value)
                .map_err(|_| Stage8a4I2CompositionError::InvalidSequence)
        })
}

fn derive_record_id(
    request: broker_core::StrategyRequestId,
    sequence: Stage6LifecycleSequence,
) -> Result<Stage6JournalRecordId, Stage8a4I2CompositionError> {
    let mut bytes = Vec::with_capacity(32);
    bytes.extend_from_slice(b"stage6-journal-record-v1");
    bytes.extend_from_slice(request.as_uuid().as_bytes());
    bytes.extend_from_slice(&sequence.get().to_be_bytes());
    serde_json::from_str(&format!("\"{}\"", sha256_hex(&bytes)))
        .map_err(|_| Stage8a4I2CompositionError::InvalidDigest)
}

fn parse_digest(value: &str) -> Result<Stage6Sha256Digest, Stage8a4I2CompositionError> {
    Stage6Sha256Digest::parse(value).map_err(|_| Stage8a4I2CompositionError::InvalidDigest)
}

fn to_u32(value: usize) -> Result<u32, Stage8a4I2CompositionError> {
    u32::try_from(value).map_err(|_| Stage8a4I2CompositionError::CountOverflow)
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn invalid_v2_decode_error() -> Stage8a4I2CompositionError {
    Stage8a4I2CompositionError::V2ValidationFailed(
        strategy_runtime_core::Stage6ReconciliationV2Error::DecodeFailed,
    )
}

#[cfg(test)]
mod tests;
