//! Stage 5G-e-d-b deterministic fresh BrokerTruth reducer.
//!
//! This child module consumes only the two accepted linear authorities.  It
//! classifies and constructs an opaque in-memory candidate; it cannot mutate a
//! runtime, invoke a callback, persist state, publish Redis data or dispatch a
//! broker command.

use broker_core::{
    instrument_identity_matches, BrokerOrderId, BrokerOrderLifecycle, BrokerOrderSnapshot,
    BrokerPositionSnapshot, BrokerTradeSnapshot, ClientOrderId, OrderSide, OrderStatus,
};
use rust_decimal::Decimal;
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::stage5g_clean_restart::{
    Stage5gCleanRestartLifecycleKind, Stage5gFreshTruthRestartProjection,
};
use crate::stage5g_order_position::{
    stage5g_account_wide_order_safety, stage5g_exact_trade_order_linkage,
    stage5g_expected_post_position_qty, stage5g_immutable_order_payload_commitment_sha256,
    stage5g_immutable_order_payload_matches, stage5g_immutable_trade_payload_matches,
    stage5g_intent_position_is_compatible, stage5g_order_matches_source_action,
    stage5g_order_ownership_correlation, Stage5gAccountWideOrderSafety,
    Stage5gFreshTruthRestartSlotProjection, Stage5gOrderOwnershipCorrelation,
    Stage5gRestartIntentClass, Stage5gTradeOrderLinkage,
};
use crate::Stage5gCleanRestartedCapability;

use super::{
    stage5g_operational_binding_commitment, stage5g_restart_replay_commitment,
    Stage5gFreshPackageLineage, Stage5gRestartBoundFreshBrokerTruthPackage,
    Stage5gRestartReconciliationDisposition, Stage5gRestartScenarioId,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Stage5gFreshTruthReductionReason {
    FreshWorkingOrderMatched,
    FreshTerminalOrderMatched,
    PartialFillPositionConverged,
    TerminalPositionAlreadyApplied,
    TimerCheckpointExact,
    GeneratedIntentEscrowRetained,
    OrdersTruthIncomplete,
    TradesTruthIncomplete,
    PositionsTruthIncomplete,
    AuthoritativeOrderMissing,
    ClientOrderIdentityConflict,
    BrokerOrderIdentityConflict,
    TradeIdentityConflict,
    PositionQuantityMismatch,
    PositionDirectionMismatch,
    UnexpectedTargetPosition,
    ReplayFingerprintConflict,
    HistoricalReplayNotAccepted,
    ReplayTupleNotInRestartLedger,
    AccountWideActiveOrderConflict,
    AccountWideUnknownOrderConflict,
    AmbiguousOwnedOrderSet,
    SourceOrderActionConflict,
    SourceLimitPriceAuthorityUnsupported,
    SourceLimitPriceMismatch,
    CancelTargetClientIdentityConflict,
    TargetOrderIdentityConflict,
    OrderTerminalRegression,
    FilledQuantityRegression,
    CommittedTradeMissing,
    CommittedTradePayloadConflict,
    TargetInstrumentIdentityConflict,
    SourceNumericAuthorityUnsupported,
    OperationalIdentityConflict,
    UnsupportedLifecycleCombination,
    TerminalContradiction,
}

/// Opaque candidate for the separately reviewed e-d-c application/evidence
/// boundary.  Deliberately not Clone, Debug, Serialize or Deserialize.
pub(crate) struct Stage5gOwnedReconciliationCandidate {
    operational_binding_commitment_sha256: String,
    command_request_id: String,
    command_client_order_id: ClientOrderId,
    target_broker_order_id: Option<BrokerOrderId>,
    target_order_client_order_id: Option<ClientOrderId>,
    intent_class: Stage5gRestartIntentClass,
    source_action: crate::Stage5gMockIntentAction,
    side: Option<OrderSide>,
    target_qty: Option<Decimal>,
    pre_position_qty: Decimal,
    expected_attribution_fingerprint_sha256: Option<String>,
    order: BrokerOrderSnapshot,
    trades: Vec<BrokerTradeSnapshot>,
    position: Option<BrokerPositionSnapshot>,
    positions_complete: bool,
    account_wide_safety_proven: bool,
    global_account_history_partition_proven: bool,
    canonical_position_semantics_proven: bool,
    source_monotonicity_proven: bool,
    immutable_target_order_monotonicity_proven: bool,
    target_order_immutable_commitment_sha256: Option<String>,
}

/// Linear reduction result. Ownership of both accepted inputs is retained so
/// no competing continuation can be created after classification.
pub(crate) struct Stage5gFreshTruthReduction {
    scenario_id: Stage5gRestartScenarioId,
    disposition: Stage5gRestartReconciliationDisposition,
    reason: Stage5gFreshTruthReductionReason,
    pre_semantic_fingerprint_sha256: String,
    post_candidate_fingerprint_sha256: String,
    ignored_unrelated_terminal_order_count: usize,
    ignored_unrelated_historical_trade_count: usize,
    candidate: Option<Stage5gOwnedReconciliationCandidate>,
    _restart: Stage5gCleanRestartedCapability,
    _truth: Stage5gRestartBoundFreshBrokerTruthPackage,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct Stage5gFreshTruthReductionEvidence {
    pub(crate) scenario_id: &'static str,
    pub(crate) disposition: &'static str,
    pub(crate) reason: Stage5gFreshTruthReductionReason,
    pub(crate) package_fingerprint_sha256: String,
    pub(crate) package_identity_commitment_sha256: String,
    pub(crate) operational_binding_commitment_sha256: String,
    pub(crate) restart_replay_commitment_sha256: String,
    pub(crate) pre_semantic_fingerprint_sha256: String,
    pub(crate) post_candidate_fingerprint_sha256: String,
    pub(crate) orders_complete: bool,
    pub(crate) trades_complete: bool,
    pub(crate) positions_complete: bool,
    pub(crate) candidate_present: bool,
    pub(crate) ignored_unrelated_terminal_order_count: usize,
    pub(crate) ignored_unrelated_historical_trade_count: usize,
    pub(crate) callback_count_before: usize,
    pub(crate) callback_count_after: usize,
    pub(crate) runtime_mutated: bool,
    pub(crate) callback_invoked: bool,
    pub(crate) transport_opened: bool,
}

impl Stage5gFreshTruthReduction {
    pub(crate) fn evidence(&self) -> Stage5gFreshTruthReductionEvidence {
        let restart = self._restart.fresh_truth_reducer_projection();
        Stage5gFreshTruthReductionEvidence {
            scenario_id: self.scenario_id.frozen_id(),
            disposition: disposition_id(self.disposition),
            reason: self.reason,
            package_fingerprint_sha256: self._truth.package.canonical_fingerprint_sha256.clone(),
            package_identity_commitment_sha256: package_identity_commitment(&self._truth),
            operational_binding_commitment_sha256: self
                ._truth
                .operational_binding_commitment_sha256
                .clone(),
            restart_replay_commitment_sha256: self._truth.restart_replay_commitment_sha256.clone(),
            pre_semantic_fingerprint_sha256: self.pre_semantic_fingerprint_sha256.clone(),
            post_candidate_fingerprint_sha256: self.post_candidate_fingerprint_sha256.clone(),
            orders_complete: self._truth.package.orders_complete,
            trades_complete: self._truth.package.trades_complete,
            positions_complete: self._truth.package.positions_complete,
            candidate_present: self.candidate.is_some(),
            ignored_unrelated_terminal_order_count: self.ignored_unrelated_terminal_order_count,
            ignored_unrelated_historical_trade_count: self.ignored_unrelated_historical_trade_count,
            callback_count_before: restart.callback_count,
            callback_count_after: restart.callback_count,
            runtime_mutated: false,
            callback_invoked: false,
            transport_opened: false,
        }
    }
}

/// The single owning e-d-b reducer entry point.
pub(crate) fn reduce_stage5g_fresh_broker_truth(
    restart: Stage5gCleanRestartedCapability,
    truth: Stage5gRestartBoundFreshBrokerTruthPackage,
) -> Stage5gFreshTruthReduction {
    let restart_projection = restart.fresh_truth_reducer_projection();
    let pre_semantic_fingerprint_sha256 = semantic_sha256(&restart_projection);
    let classified = classify(&restart_projection, &truth);
    let post_candidate_fingerprint_sha256 = classified
        .candidate
        .as_ref()
        .map(candidate_fingerprint)
        .unwrap_or_else(|| pre_semantic_fingerprint_sha256.clone());
    Stage5gFreshTruthReduction {
        scenario_id: classified.scenario_id,
        disposition: classified.disposition,
        reason: classified.reason,
        pre_semantic_fingerprint_sha256,
        post_candidate_fingerprint_sha256,
        ignored_unrelated_terminal_order_count: classified.ignored_unrelated_terminal_order_count,
        ignored_unrelated_historical_trade_count: classified
            .ignored_unrelated_historical_trade_count,
        candidate: classified.candidate,
        _restart: restart,
        _truth: truth,
    }
}

struct Classified {
    scenario_id: Stage5gRestartScenarioId,
    disposition: Stage5gRestartReconciliationDisposition,
    reason: Stage5gFreshTruthReductionReason,
    candidate: Option<Stage5gOwnedReconciliationCandidate>,
    ignored_unrelated_terminal_order_count: usize,
    ignored_unrelated_historical_trade_count: usize,
}

impl Classified {
    fn with_history_counts(
        mut self,
        ignored_unrelated_terminal_order_count: usize,
        ignored_unrelated_historical_trade_count: usize,
    ) -> Self {
        self.ignored_unrelated_terminal_order_count = ignored_unrelated_terminal_order_count;
        self.ignored_unrelated_historical_trade_count = ignored_unrelated_historical_trade_count;
        self
    }
}

fn classify(
    restart: &Stage5gFreshTruthRestartProjection,
    bound_truth: &Stage5gRestartBoundFreshBrokerTruthPackage,
) -> Classified {
    let truth = &bound_truth.package;
    if !cross_binding_matches(restart, bound_truth) {
        return blocked(
            Stage5gRestartScenarioId::Grst10ConflictingReplayBlocks,
            Stage5gRestartReconciliationDisposition::ReconciliationRequired,
            Stage5gFreshTruthReductionReason::OperationalIdentityConflict,
        );
    }

    match truth.lineage {
        Stage5gFreshPackageLineage::ReplayTupleNotInRestartLedger => {
            return blocked(
                replay_scenario(restart),
                Stage5gRestartReconciliationDisposition::ReconciliationRequired,
                Stage5gFreshTruthReductionReason::ReplayTupleNotInRestartLedger,
            );
        }
        Stage5gFreshPackageLineage::HistoricalReplayNotAccepted => {
            return blocked(
                replay_scenario(restart),
                Stage5gRestartReconciliationDisposition::ManualInterventionRequired,
                Stage5gFreshTruthReductionReason::HistoricalReplayNotAccepted,
            );
        }
        Stage5gFreshPackageLineage::ReplayFingerprintConflict => {
            return replay_conflict();
        }
        Stage5gFreshPackageLineage::NewFresh => {}
    }

    if !truth.orders_complete {
        return incomplete(
            restart,
            Stage5gFreshTruthReductionReason::OrdersTruthIncomplete,
        );
    }
    if !truth.trades_complete {
        return incomplete(
            restart,
            Stage5gFreshTruthReductionReason::TradesTruthIncomplete,
        );
    }

    if !restart.committed_position_numeric_authority_is_integral
        || restart
            .slots
            .iter()
            .any(|slot| !slot.source_numeric_authority_is_integral)
    {
        return blocked(
            Stage5gRestartScenarioId::Grst12MissingOrAmbiguousTruthRequiresReconciliation,
            Stage5gRestartReconciliationDisposition::ManualInterventionRequired,
            Stage5gFreshTruthReductionReason::SourceNumericAuthorityUnsupported,
        );
    }

    let owned_slot = (restart.slots.len() == 1).then(|| &restart.slots[0]);
    match stage5g_account_wide_order_safety(
        &truth.orders,
        owned_slot.and_then(|slot| slot.target_order_client_order_id.as_ref()),
        owned_slot.and_then(|slot| slot.target_broker_order_id.as_ref()),
    ) {
        Stage5gAccountWideOrderSafety::Safe => {}
        Stage5gAccountWideOrderSafety::NonOwnedActive => {
            return blocked(
                Stage5gRestartScenarioId::Grst12MissingOrAmbiguousTruthRequiresReconciliation,
                Stage5gRestartReconciliationDisposition::ManualInterventionRequired,
                Stage5gFreshTruthReductionReason::AccountWideActiveOrderConflict,
            );
        }
        Stage5gAccountWideOrderSafety::NonOwnedUnknown => {
            return blocked(
                Stage5gRestartScenarioId::Grst12MissingOrAmbiguousTruthRequiresReconciliation,
                Stage5gRestartReconciliationDisposition::ManualInterventionRequired,
                Stage5gFreshTruthReductionReason::AccountWideUnknownOrderConflict,
            );
        }
        Stage5gAccountWideOrderSafety::AmbiguousOwned => {
            return blocked(
                Stage5gRestartScenarioId::Grst10ConflictingReplayBlocks,
                Stage5gRestartReconciliationDisposition::ManualInterventionRequired,
                Stage5gFreshTruthReductionReason::AmbiguousOwnedOrderSet,
            );
        }
        Stage5gAccountWideOrderSafety::ConflictingOwnedIdentity => {
            return blocked(
                Stage5gRestartScenarioId::Grst10ConflictingReplayBlocks,
                Stage5gRestartReconciliationDisposition::ManualInterventionRequired,
                Stage5gFreshTruthReductionReason::TargetOrderIdentityConflict,
            );
        }
    }

    let target_identity_conflict = truth.orders.iter().any(|row| {
        instrument_identity_matches(&row.instrument, &restart.instrument_id)
            && row.instrument != restart.instrument_id
    }) || truth.trades.iter().any(|row| {
        instrument_identity_matches(&row.instrument, &restart.instrument_id)
            && row.instrument != restart.instrument_id
    }) || truth.positions.iter().any(|row| {
        instrument_identity_matches(&row.instrument, &restart.instrument_id)
            && row.instrument != restart.instrument_id
    });
    if target_identity_conflict {
        return blocked(
            Stage5gRestartScenarioId::Grst10ConflictingReplayBlocks,
            Stage5gRestartReconciliationDisposition::ManualInterventionRequired,
            Stage5gFreshTruthReductionReason::TargetInstrumentIdentityConflict,
        );
    }

    let target_orders = truth
        .orders
        .iter()
        .filter(|row| row.instrument == restart.instrument_id)
        .collect::<Vec<_>>();
    let target_trades = truth
        .trades
        .iter()
        .filter(|row| row.instrument == restart.instrument_id)
        .collect::<Vec<_>>();
    let target_positions = truth
        .positions
        .iter()
        .filter(|row| row.instrument == restart.instrument_id)
        .collect::<Vec<_>>();

    let global_history =
        stage5g_global_history_partition(owned_slot.is_some(), &target_orders, &target_trades);

    if target_positions.len() > 1 {
        return blocked(
            Stage5gRestartScenarioId::Grst12MissingOrAmbiguousTruthRequiresReconciliation,
            Stage5gRestartReconciliationDisposition::TerminalInconsistency,
            Stage5gFreshTruthReductionReason::UnexpectedTargetPosition,
        )
        .with_history_counts(
            global_history.ignored_unrelated_terminal_order_count,
            global_history.ignored_unrelated_historical_trade_count,
        );
    }

    if restart.lifecycle_kind == Stage5gCleanRestartLifecycleKind::TimerReady {
        if !truth.positions_complete {
            return incomplete(
                restart,
                Stage5gFreshTruthReductionReason::PositionsTruthIncomplete,
            );
        }
        let observed_position = observed_complete_position_qty(&target_positions);
        return if global_history.all_rows_classified
            && observed_position == Some(restart.committed_position_qty)
        {
            blocked(
                Stage5gRestartScenarioId::Grst07RestartAtTimerCheckpoint,
                Stage5gRestartReconciliationDisposition::ContinueFromCommittedCheckpoint,
                Stage5gFreshTruthReductionReason::TimerCheckpointExact,
            )
            .with_history_counts(
                global_history.ignored_unrelated_terminal_order_count,
                global_history.ignored_unrelated_historical_trade_count,
            )
        } else {
            blocked(
                Stage5gRestartScenarioId::Grst12MissingOrAmbiguousTruthRequiresReconciliation,
                Stage5gRestartReconciliationDisposition::TerminalInconsistency,
                Stage5gFreshTruthReductionReason::UnexpectedTargetPosition,
            )
            .with_history_counts(
                global_history.ignored_unrelated_terminal_order_count,
                global_history.ignored_unrelated_historical_trade_count,
            )
        };
    }

    if restart.slots.is_empty() {
        if !truth.positions_complete {
            return incomplete(
                restart,
                Stage5gFreshTruthReductionReason::PositionsTruthIncomplete,
            );
        }
        let target_position_matches = observed_complete_position_qty(&target_positions)
            == Some(restart.committed_position_qty);
        return if global_history.all_rows_classified && target_position_matches {
            blocked(
                Stage5gRestartScenarioId::Grst01RestartBeforeAck,
                Stage5gRestartReconciliationDisposition::AwaitFreshBrokerTruth,
                Stage5gFreshTruthReductionReason::AuthoritativeOrderMissing,
            )
            .with_history_counts(
                global_history.ignored_unrelated_terminal_order_count,
                global_history.ignored_unrelated_historical_trade_count,
            )
        } else {
            blocked(
                Stage5gRestartScenarioId::Grst12MissingOrAmbiguousTruthRequiresReconciliation,
                Stage5gRestartReconciliationDisposition::TerminalInconsistency,
                Stage5gFreshTruthReductionReason::UnexpectedTargetPosition,
            )
            .with_history_counts(
                global_history.ignored_unrelated_terminal_order_count,
                global_history.ignored_unrelated_historical_trade_count,
            )
        };
    }

    if restart.slots.len() != 1 {
        return blocked(
            Stage5gRestartScenarioId::Grst12MissingOrAmbiguousTruthRequiresReconciliation,
            Stage5gRestartReconciliationDisposition::ReconciliationRequired,
            Stage5gFreshTruthReductionReason::UnsupportedLifecycleCombination,
        );
    }
    let slot = &restart.slots[0];
    if matches!(
        slot.source_action,
        crate::Stage5gMockIntentAction::Place {
            place_kind: crate::Stage5gMockPlaceKind::Limit
        }
    ) {
        return blocked(
            Stage5gRestartScenarioId::Grst12MissingOrAmbiguousTruthRequiresReconciliation,
            Stage5gRestartReconciliationDisposition::ManualInterventionRequired,
            Stage5gFreshTruthReductionReason::SourceLimitPriceAuthorityUnsupported,
        );
    }

    let order_correlations = target_orders
        .iter()
        .map(|order| {
            (
                *order,
                stage5g_order_ownership_correlation(
                    order,
                    slot.target_order_client_order_id.as_ref(),
                    slot.target_broker_order_id.as_ref(),
                ),
            )
        })
        .collect::<Vec<_>>();
    let ignored_unrelated_terminal_order_count = order_correlations
        .iter()
        .filter(|(_, correlation)| {
            *correlation == Stage5gOrderOwnershipCorrelation::UnrelatedTerminal
        })
        .count();
    if order_correlations.iter().any(|(_, correlation)| {
        *correlation == Stage5gOrderOwnershipCorrelation::ConflictingOwnedIdentity
    }) {
        return blocked(
            Stage5gRestartScenarioId::Grst10ConflictingReplayBlocks,
            Stage5gRestartReconciliationDisposition::ManualInterventionRequired,
            Stage5gFreshTruthReductionReason::TargetOrderIdentityConflict,
        )
        .with_history_counts(ignored_unrelated_terminal_order_count, 0);
    }
    let matching = order_correlations
        .iter()
        .filter_map(|(order, correlation)| {
            (*correlation == Stage5gOrderOwnershipCorrelation::ExactOwned).then_some(*order)
        })
        .collect::<Vec<_>>();
    if matching.len() > 1 {
        return blocked(
            Stage5gRestartScenarioId::Grst10ConflictingReplayBlocks,
            Stage5gRestartReconciliationDisposition::ManualInterventionRequired,
            Stage5gFreshTruthReductionReason::AmbiguousOwnedOrderSet,
        )
        .with_history_counts(ignored_unrelated_terminal_order_count, 0);
    }
    let Some(order) = matching.first().copied() else {
        let missing = if restart.generated_intent_escrow_fingerprint_sha256.is_some() {
            blocked(
                Stage5gRestartScenarioId::Grst08RestartWithGeneratedIntentEscrow,
                Stage5gRestartReconciliationDisposition::ReconciliationRequired,
                Stage5gFreshTruthReductionReason::GeneratedIntentEscrowRetained,
            )
        } else {
            blocked(
                Stage5gRestartScenarioId::Grst12MissingOrAmbiguousTruthRequiresReconciliation,
                Stage5gRestartReconciliationDisposition::ReconciliationRequired,
                Stage5gFreshTruthReductionReason::AuthoritativeOrderMissing,
            )
        };
        return missing
            .with_history_counts(ignored_unrelated_terminal_order_count, target_trades.len());
    };

    match &slot.source_action {
        crate::Stage5gMockIntentAction::Place { .. } => {
            if order.client_order_id.as_ref() != Some(&slot.command_client_order_id) {
                return blocked(
                    Stage5gRestartScenarioId::Grst10ConflictingReplayBlocks,
                    Stage5gRestartReconciliationDisposition::ReconciliationRequired,
                    Stage5gFreshTruthReductionReason::ClientOrderIdentityConflict,
                );
            }
        }
        crate::Stage5gMockIntentAction::Cancel { .. } => {
            if order.broker_order_id.as_ref() != slot.target_broker_order_id.as_ref() {
                return blocked(
                    Stage5gRestartScenarioId::Grst10ConflictingReplayBlocks,
                    Stage5gRestartReconciliationDisposition::ManualInterventionRequired,
                    Stage5gFreshTruthReductionReason::BrokerOrderIdentityConflict,
                );
            }
            if slot
                .target_order_client_order_id
                .as_ref()
                .is_some_and(|expected| order.client_order_id.as_ref() != Some(expected))
            {
                return blocked(
                    Stage5gRestartScenarioId::Grst10ConflictingReplayBlocks,
                    Stage5gRestartReconciliationDisposition::ManualInterventionRequired,
                    Stage5gFreshTruthReductionReason::CancelTargetClientIdentityConflict,
                );
            }
        }
    }
    if slot.side.is_some_and(|side| side != order.side) {
        return blocked(
            Stage5gRestartScenarioId::Grst10ConflictingReplayBlocks,
            Stage5gRestartReconciliationDisposition::TerminalInconsistency,
            Stage5gFreshTruthReductionReason::TerminalContradiction,
        );
    }
    if !stage5g_order_matches_source_action(&slot.source_action, order) {
        return blocked(
            Stage5gRestartScenarioId::Grst10ConflictingReplayBlocks,
            Stage5gRestartReconciliationDisposition::TerminalInconsistency,
            Stage5gFreshTruthReductionReason::SourceOrderActionConflict,
        );
    }

    let mut linked_trades = Vec::with_capacity(target_trades.len());
    let mut ignored_unrelated_historical_trade_count = 0usize;
    for trade in target_trades {
        match stage5g_exact_trade_order_linkage(order, trade) {
            Stage5gTradeOrderLinkage::Exact => linked_trades.push(trade),
            Stage5gTradeOrderLinkage::Unrelated => {
                ignored_unrelated_historical_trade_count += 1;
            }
            Stage5gTradeOrderLinkage::Conflict => {
                return blocked(
                    Stage5gRestartScenarioId::Grst10ConflictingReplayBlocks,
                    Stage5gRestartReconciliationDisposition::ManualInterventionRequired,
                    Stage5gFreshTruthReductionReason::TradeIdentityConflict,
                )
                .with_history_counts(
                    ignored_unrelated_terminal_order_count,
                    ignored_unrelated_historical_trade_count,
                );
            }
        }
    }
    if linked_trades.iter().any(|trade| trade.side != order.side) {
        return blocked(
            Stage5gRestartScenarioId::Grst10ConflictingReplayBlocks,
            Stage5gRestartReconciliationDisposition::TerminalInconsistency,
            Stage5gFreshTruthReductionReason::TradeIdentityConflict,
        );
    }
    if slot
        .target_qty
        .is_some_and(|expected| expected != order.qty)
    {
        return blocked(
            Stage5gRestartScenarioId::Grst10ConflictingReplayBlocks,
            Stage5gRestartReconciliationDisposition::TerminalInconsistency,
            Stage5gFreshTruthReductionReason::TerminalContradiction,
        )
        .with_history_counts(
            ignored_unrelated_terminal_order_count,
            ignored_unrelated_historical_trade_count,
        );
    }
    if slot.terminal && !truth.positions_complete {
        return incomplete(
            restart,
            Stage5gFreshTruthReductionReason::PositionsTruthIncomplete,
        );
    }
    if matches!(order.status, OrderStatus::Rejected)
        && (order.filled_qty > Decimal::ZERO || !linked_trades.is_empty())
    {
        return terminal_contradiction().with_history_counts(
            ignored_unrelated_terminal_order_count,
            ignored_unrelated_historical_trade_count,
        );
    }

    let progress = source_to_fresh_progress(
        slot,
        order,
        &linked_trades,
        target_positions.first().copied(),
        truth.positions_complete,
    );
    if let Stage5gSourceFreshProgress::Conflict(reason) = progress {
        return blocked(
            Stage5gRestartScenarioId::Grst10ConflictingReplayBlocks,
            Stage5gRestartReconciliationDisposition::TerminalInconsistency,
            reason,
        )
        .with_history_counts(
            ignored_unrelated_terminal_order_count,
            ignored_unrelated_historical_trade_count,
        );
    }

    let trade_sum = linked_trades
        .iter()
        .fold(Decimal::ZERO, |sum, trade| sum + trade.qty);
    if trade_sum != order.filled_qty {
        return blocked(
            Stage5gRestartScenarioId::Grst10ConflictingReplayBlocks,
            Stage5gRestartReconciliationDisposition::ReconciliationRequired,
            Stage5gFreshTruthReductionReason::TradeIdentityConflict,
        )
        .with_history_counts(
            ignored_unrelated_terminal_order_count,
            ignored_unrelated_historical_trade_count,
        );
    }

    let expected_position = stage5g_expected_post_position_qty(slot.pre_position_qty, order);
    let observed_position = truth
        .positions_complete
        .then(|| observed_complete_position_qty(&target_positions))
        .flatten();
    let position_is_exact = observed_position == Some(expected_position);
    let intent_is_compatible = stage5g_intent_position_is_compatible(
        slot.intent_class,
        slot.pre_position_qty,
        expected_position,
        order,
    );
    let exact_terminal_shape_is_compatible = match order.status {
        OrderStatus::Rejected => {
            order.filled_qty == Decimal::ZERO
                && linked_trades.is_empty()
                && observed_position == Some(slot.pre_position_qty)
        }
        OrderStatus::Filled | OrderStatus::Canceled | OrderStatus::Expired => {
            position_is_exact && (order.filled_qty == Decimal::ZERO || intent_is_compatible)
        }
        _ => false,
    };

    if slot.terminal
        && progress == Stage5gSourceFreshProgress::ExactCommittedTerminal
        && exact_terminal_shape_is_compatible
        && matches!(
            order.status,
            OrderStatus::Filled
                | OrderStatus::Rejected
                | OrderStatus::Canceled
                | OrderStatus::Expired
        )
    {
        return blocked(
            Stage5gRestartScenarioId::Grst06RestartAfterTerminalPositionApplied,
            Stage5gRestartReconciliationDisposition::ContinueFromCommittedCheckpoint,
            Stage5gFreshTruthReductionReason::TerminalPositionAlreadyApplied,
        )
        .with_history_counts(
            ignored_unrelated_terminal_order_count,
            ignored_unrelated_historical_trade_count,
        );
    }

    match order.status {
        OrderStatus::New | OrderStatus::Working => {
            if order.filled_qty > Decimal::ZERO || !linked_trades.is_empty() {
                return terminal_contradiction().with_history_counts(
                    ignored_unrelated_terminal_order_count,
                    ignored_unrelated_historical_trade_count,
                );
            }
            if !truth.positions_complete {
                return incomplete(
                    restart,
                    Stage5gFreshTruthReductionReason::PositionsTruthIncomplete,
                )
                .with_history_counts(
                    ignored_unrelated_terminal_order_count,
                    ignored_unrelated_historical_trade_count,
                );
            }
            if !position_is_exact {
                return position_conflict(expected_position, observed_position)
                    .with_history_counts(
                        ignored_unrelated_terminal_order_count,
                        ignored_unrelated_historical_trade_count,
                    );
            }
            candidate(
                if slot.target_broker_order_id.is_none() {
                    Stage5gRestartScenarioId::Grst02RestartAfterAckBeforeOrder
                } else {
                    Stage5gRestartScenarioId::Grst03RestartWithWorkingOrder
                },
                Stage5gFreshTruthReductionReason::FreshWorkingOrderMatched,
                bound_truth,
                slot,
                CandidateBrokerRows {
                    order,
                    trades: linked_trades,
                    position: target_positions.first().copied(),
                    positions_complete: true,
                    ignored_unrelated_terminal_order_count,
                    ignored_unrelated_historical_trade_count,
                },
            )
        }
        OrderStatus::PartiallyFilled => {
            if !truth.positions_complete {
                return incomplete(
                    restart,
                    Stage5gFreshTruthReductionReason::PositionsTruthIncomplete,
                )
                .with_history_counts(
                    ignored_unrelated_terminal_order_count,
                    ignored_unrelated_historical_trade_count,
                );
            }
            if order.filled_qty <= Decimal::ZERO
                || order.filled_qty >= order.qty
                || !position_is_exact
                || !intent_is_compatible
            {
                return position_conflict(expected_position, observed_position)
                    .with_history_counts(
                        ignored_unrelated_terminal_order_count,
                        ignored_unrelated_historical_trade_count,
                    );
            }
            candidate(
                Stage5gRestartScenarioId::Grst04RestartAfterPartialFill,
                Stage5gFreshTruthReductionReason::PartialFillPositionConverged,
                bound_truth,
                slot,
                CandidateBrokerRows {
                    order,
                    trades: linked_trades,
                    position: target_positions.first().copied(),
                    positions_complete: true,
                    ignored_unrelated_terminal_order_count,
                    ignored_unrelated_historical_trade_count,
                },
            )
        }
        OrderStatus::Filled => {
            if !truth.positions_complete {
                return blocked(
                    Stage5gRestartScenarioId::Grst05RestartFilledBeforePosition,
                    Stage5gRestartReconciliationDisposition::AwaitFreshBrokerTruth,
                    Stage5gFreshTruthReductionReason::PositionsTruthIncomplete,
                )
                .with_history_counts(
                    ignored_unrelated_terminal_order_count,
                    ignored_unrelated_historical_trade_count,
                );
            }
            if !position_is_exact || order.filled_qty > Decimal::ZERO && !intent_is_compatible {
                return position_conflict(expected_position, observed_position)
                    .with_history_counts(
                        ignored_unrelated_terminal_order_count,
                        ignored_unrelated_historical_trade_count,
                    );
            }
            candidate(
                Stage5gRestartScenarioId::Grst11FreshBrokerTruthOverridesStaleHint,
                Stage5gFreshTruthReductionReason::FreshTerminalOrderMatched,
                bound_truth,
                slot,
                CandidateBrokerRows {
                    order,
                    trades: linked_trades,
                    position: target_positions.first().copied(),
                    positions_complete: true,
                    ignored_unrelated_terminal_order_count,
                    ignored_unrelated_historical_trade_count,
                },
            )
        }
        OrderStatus::Rejected => {
            if !truth.positions_complete {
                return incomplete(
                    restart,
                    Stage5gFreshTruthReductionReason::PositionsTruthIncomplete,
                )
                .with_history_counts(
                    ignored_unrelated_terminal_order_count,
                    ignored_unrelated_historical_trade_count,
                );
            }
            if observed_position != Some(slot.pre_position_qty) {
                return position_conflict(slot.pre_position_qty, observed_position)
                    .with_history_counts(
                        ignored_unrelated_terminal_order_count,
                        ignored_unrelated_historical_trade_count,
                    );
            }
            candidate(
                Stage5gRestartScenarioId::Grst11FreshBrokerTruthOverridesStaleHint,
                Stage5gFreshTruthReductionReason::FreshTerminalOrderMatched,
                bound_truth,
                slot,
                CandidateBrokerRows {
                    order,
                    trades: linked_trades,
                    position: target_positions.first().copied(),
                    positions_complete: true,
                    ignored_unrelated_terminal_order_count,
                    ignored_unrelated_historical_trade_count,
                },
            )
        }
        OrderStatus::Canceled | OrderStatus::Expired => {
            if !truth.positions_complete {
                return incomplete(
                    restart,
                    Stage5gFreshTruthReductionReason::PositionsTruthIncomplete,
                )
                .with_history_counts(
                    ignored_unrelated_terminal_order_count,
                    ignored_unrelated_historical_trade_count,
                );
            }
            if !position_is_exact || order.filled_qty > Decimal::ZERO && !intent_is_compatible {
                return position_conflict(expected_position, observed_position)
                    .with_history_counts(
                        ignored_unrelated_terminal_order_count,
                        ignored_unrelated_historical_trade_count,
                    );
            }
            candidate(
                Stage5gRestartScenarioId::Grst11FreshBrokerTruthOverridesStaleHint,
                Stage5gFreshTruthReductionReason::FreshTerminalOrderMatched,
                bound_truth,
                slot,
                CandidateBrokerRows {
                    order,
                    trades: linked_trades,
                    position: target_positions.first().copied(),
                    positions_complete: true,
                    ignored_unrelated_terminal_order_count,
                    ignored_unrelated_historical_trade_count,
                },
            )
        }
        OrderStatus::Unknown(_) => blocked(
            Stage5gRestartScenarioId::Grst12MissingOrAmbiguousTruthRequiresReconciliation,
            Stage5gRestartReconciliationDisposition::ReconciliationRequired,
            Stage5gFreshTruthReductionReason::UnsupportedLifecycleCombination,
        )
        .with_history_counts(
            ignored_unrelated_terminal_order_count,
            ignored_unrelated_historical_trade_count,
        ),
    }
}

fn cross_binding_matches(
    restart: &Stage5gFreshTruthRestartProjection,
    truth: &Stage5gRestartBoundFreshBrokerTruthPackage,
) -> bool {
    let package = &truth.package;
    restart.account_id == package.operational_identity.account_id
        && restart.strategy_id == package.operational_identity.strategy_definition_id.as_str()
        && restart.config_fingerprint_sha256
            == package
                .operational_identity
                .config_fingerprint_sha256
                .as_str()
        && restart.instrument_id == package.operational_identity.target_instrument
        && restart.strategy_state_fingerprint_sha256
            == restart.reconstructed_runtime_state_fingerprint_sha256
        && truth.operational_binding_commitment_sha256
            == stage5g_operational_binding_commitment(
                restart,
                &package.operational_identity,
                &package.replay_hints,
            )
        && truth.restart_replay_commitment_sha256
            == stage5g_restart_replay_commitment(restart, &package.replay_hints)
}

fn replay_conflict() -> Classified {
    blocked(
        Stage5gRestartScenarioId::Grst10ConflictingReplayBlocks,
        Stage5gRestartReconciliationDisposition::ReconciliationRequired,
        Stage5gFreshTruthReductionReason::ReplayFingerprintConflict,
    )
}

fn replay_scenario(restart: &Stage5gFreshTruthRestartProjection) -> Stage5gRestartScenarioId {
    if restart.lifecycle_kind == Stage5gCleanRestartLifecycleKind::TimerReady {
        Stage5gRestartScenarioId::Grst07RestartAtTimerCheckpoint
    } else {
        Stage5gRestartScenarioId::Grst09ExactReplayIsIdempotent
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stage5gSourceFreshProgress {
    NoCommittedOrder,
    MonotonicAdvance,
    ExactCommittedTerminal,
    Conflict(Stage5gFreshTruthReductionReason),
}

pub(crate) fn stage5g_semantic_terminal_order_matches(
    committed: &BrokerOrderSnapshot,
    fresh: &BrokerOrderSnapshot,
) -> bool {
    stage5g_immutable_order_payload_matches(committed, fresh)
        && committed.status == fresh.status
        && committed.lifecycle == fresh.lifecycle
        && committed.filled_qty == fresh.filled_qty
        && committed.remaining_qty == fresh.remaining_qty
}

pub(crate) fn stage5g_semantic_position_matches(
    committed: &BrokerPositionSnapshot,
    fresh: &BrokerPositionSnapshot,
) -> bool {
    let same_identity = committed.account_id == fresh.account_id
        && committed.instrument == fresh.instrument
        && committed.qty == fresh.qty;
    same_identity && (committed.qty == Decimal::ZERO || committed.avg_price == fresh.avg_price)
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Stage5gCanonicalPositionObservation<'a> {
    Flat,
    NonFlat(&'a BrokerPositionSnapshot),
}

fn stage5g_canonical_position_observation(
    section_complete: bool,
    position: Option<&BrokerPositionSnapshot>,
) -> Option<Stage5gCanonicalPositionObservation<'_>> {
    if !section_complete {
        return None;
    }
    match position {
        None => Some(Stage5gCanonicalPositionObservation::Flat),
        Some(position) if position.qty == Decimal::ZERO => {
            Some(Stage5gCanonicalPositionObservation::Flat)
        }
        Some(position) => Some(Stage5gCanonicalPositionObservation::NonFlat(position)),
    }
}

fn stage5g_semantic_position_observation_matches(
    committed: Option<&BrokerPositionSnapshot>,
    fresh: Option<&BrokerPositionSnapshot>,
    fresh_section_complete: bool,
) -> bool {
    let committed = stage5g_canonical_position_observation(true, committed);
    let fresh = stage5g_canonical_position_observation(fresh_section_complete, fresh);
    match (committed, fresh) {
        (
            Some(Stage5gCanonicalPositionObservation::Flat),
            Some(Stage5gCanonicalPositionObservation::Flat),
        ) => true,
        (
            Some(Stage5gCanonicalPositionObservation::NonFlat(committed)),
            Some(Stage5gCanonicalPositionObservation::NonFlat(fresh)),
        ) => stage5g_semantic_position_matches(committed, fresh),
        _ => false,
    }
}

fn source_to_fresh_progress(
    slot: &Stage5gFreshTruthRestartSlotProjection,
    fresh_order: &BrokerOrderSnapshot,
    fresh_trades: &[&BrokerTradeSnapshot],
    fresh_position: Option<&BrokerPositionSnapshot>,
    fresh_positions_complete: bool,
) -> Stage5gSourceFreshProgress {
    let Some(committed_order) = slot.latest_order.as_ref() else {
        return if slot.trades.is_empty() && !slot.terminal {
            Stage5gSourceFreshProgress::NoCommittedOrder
        } else {
            Stage5gSourceFreshProgress::Conflict(
                Stage5gFreshTruthReductionReason::CommittedTradeMissing,
            )
        };
    };
    if committed_order.client_order_id != fresh_order.client_order_id
        || committed_order.broker_order_id != fresh_order.broker_order_id
    {
        return Stage5gSourceFreshProgress::Conflict(
            Stage5gFreshTruthReductionReason::BrokerOrderIdentityConflict,
        );
    }
    if !stage5g_immutable_order_payload_matches(committed_order, fresh_order) {
        return Stage5gSourceFreshProgress::Conflict(
            Stage5gFreshTruthReductionReason::TerminalContradiction,
        );
    }
    if committed_order.lifecycle == BrokerOrderLifecycle::Terminal
        && (fresh_order.lifecycle != BrokerOrderLifecycle::Terminal
            || fresh_order.status != committed_order.status)
    {
        return Stage5gSourceFreshProgress::Conflict(
            Stage5gFreshTruthReductionReason::OrderTerminalRegression,
        );
    }
    if fresh_order.filled_qty < committed_order.filled_qty {
        return Stage5gSourceFreshProgress::Conflict(
            Stage5gFreshTruthReductionReason::FilledQuantityRegression,
        );
    }
    for committed in &slot.trades {
        let Some(fresh) = fresh_trades
            .iter()
            .copied()
            .find(|fresh| fresh.broker_trade_id == committed.broker_trade_id)
        else {
            return Stage5gSourceFreshProgress::Conflict(
                Stage5gFreshTruthReductionReason::CommittedTradeMissing,
            );
        };
        if !stage5g_immutable_trade_payload_matches(committed, fresh) {
            return Stage5gSourceFreshProgress::Conflict(
                Stage5gFreshTruthReductionReason::CommittedTradePayloadConflict,
            );
        }
    }
    if slot.terminal {
        let exact_trades = slot.trades.len() == fresh_trades.len();
        let exact_position = stage5g_semantic_position_observation_matches(
            slot.position.as_ref(),
            fresh_position,
            fresh_positions_complete,
        );
        if stage5g_semantic_terminal_order_matches(committed_order, fresh_order)
            && exact_trades
            && exact_position
        {
            return Stage5gSourceFreshProgress::ExactCommittedTerminal;
        }

        let same_status_terminal_advance_allowed = matches!(
            committed_order.status,
            OrderStatus::Canceled | OrderStatus::Expired
        ) && fresh_order.status
            == committed_order.status
            && fresh_order.lifecycle == BrokerOrderLifecycle::Terminal;
        let fresh_trade_ids_are_unique = fresh_trades.iter().enumerate().all(|(index, trade)| {
            fresh_trades[index + 1..]
                .iter()
                .all(|other| other.broker_trade_id != trade.broker_trade_id)
        });
        let committed_trade_sum = slot
            .trades
            .iter()
            .fold(Decimal::ZERO, |sum, trade| sum + trade.qty);
        let fresh_trade_sum = fresh_trades
            .iter()
            .fold(Decimal::ZERO, |sum, trade| sum + trade.qty);
        let expected_fresh_position =
            stage5g_expected_post_position_qty(slot.pre_position_qty, fresh_order);
        let observed_fresh_position = fresh_positions_complete.then_some(
            fresh_position
                .map(|position| position.qty)
                .unwrap_or(Decimal::ZERO),
        );
        let added_trade_set_is_complete = fresh_trades.len() > slot.trades.len()
            && committed_trade_sum == committed_order.filled_qty
            && fresh_trade_sum == fresh_order.filled_qty;
        let fresh_fill_shape_is_exact = fresh_order.filled_qty > committed_order.filled_qty
            && fresh_order.filled_qty < fresh_order.qty
            && fresh_order.remaining_qty == Some(fresh_order.qty - fresh_order.filled_qty);
        let source_intent_remains_compatible = stage5g_intent_position_is_compatible(
            slot.intent_class,
            slot.pre_position_qty,
            expected_fresh_position,
            fresh_order,
        );

        if same_status_terminal_advance_allowed
            && fresh_fill_shape_is_exact
            && fresh_trade_ids_are_unique
            && added_trade_set_is_complete
            && observed_fresh_position == Some(expected_fresh_position)
            && source_intent_remains_compatible
        {
            Stage5gSourceFreshProgress::MonotonicAdvance
        } else {
            Stage5gSourceFreshProgress::Conflict(
                Stage5gFreshTruthReductionReason::TerminalContradiction,
            )
        }
    } else {
        Stage5gSourceFreshProgress::MonotonicAdvance
    }
}

fn incomplete(
    restart: &Stage5gFreshTruthRestartProjection,
    reason: Stage5gFreshTruthReductionReason,
) -> Classified {
    if restart.generated_intent_escrow_fingerprint_sha256.is_some() {
        blocked(
            Stage5gRestartScenarioId::Grst08RestartWithGeneratedIntentEscrow,
            Stage5gRestartReconciliationDisposition::AwaitFreshBrokerTruth,
            Stage5gFreshTruthReductionReason::GeneratedIntentEscrowRetained,
        )
    } else {
        blocked(
            Stage5gRestartScenarioId::Grst12MissingOrAmbiguousTruthRequiresReconciliation,
            Stage5gRestartReconciliationDisposition::AwaitFreshBrokerTruth,
            reason,
        )
    }
}

fn blocked(
    scenario_id: Stage5gRestartScenarioId,
    disposition: Stage5gRestartReconciliationDisposition,
    reason: Stage5gFreshTruthReductionReason,
) -> Classified {
    Classified {
        scenario_id,
        disposition,
        reason,
        candidate: None,
        ignored_unrelated_terminal_order_count: 0,
        ignored_unrelated_historical_trade_count: 0,
    }
}

struct CandidateBrokerRows<'a> {
    order: &'a BrokerOrderSnapshot,
    trades: Vec<&'a BrokerTradeSnapshot>,
    position: Option<&'a BrokerPositionSnapshot>,
    positions_complete: bool,
    ignored_unrelated_terminal_order_count: usize,
    ignored_unrelated_historical_trade_count: usize,
}

fn candidate(
    scenario_id: Stage5gRestartScenarioId,
    reason: Stage5gFreshTruthReductionReason,
    truth: &Stage5gRestartBoundFreshBrokerTruthPackage,
    slot: &Stage5gFreshTruthRestartSlotProjection,
    rows: CandidateBrokerRows<'_>,
) -> Classified {
    let order = rows.order;
    let candidate = Stage5gOwnedReconciliationCandidate {
        operational_binding_commitment_sha256: truth.operational_binding_commitment_sha256.clone(),
        command_request_id: slot.command_request_id.clone(),
        command_client_order_id: slot.command_client_order_id.clone(),
        target_broker_order_id: slot
            .target_broker_order_id
            .clone()
            .or_else(|| order.broker_order_id.clone()),
        target_order_client_order_id: slot.target_order_client_order_id.clone(),
        intent_class: slot.intent_class,
        source_action: slot.source_action.clone(),
        side: slot.side,
        target_qty: slot.target_qty,
        pre_position_qty: slot.pre_position_qty,
        expected_attribution_fingerprint_sha256: slot
            .expected_attribution_fingerprint_sha256
            .clone(),
        order: order.clone(),
        trades: rows.trades.into_iter().cloned().collect(),
        position: rows.position.cloned(),
        positions_complete: rows.positions_complete,
        account_wide_safety_proven: true,
        global_account_history_partition_proven: true,
        canonical_position_semantics_proven: true,
        source_monotonicity_proven: true,
        immutable_target_order_monotonicity_proven: true,
        target_order_immutable_commitment_sha256: slot
            .cancel_target_order_authority
            .as_ref()
            .and_then(|authority| authority.immutable_order_commitment_sha256.clone()),
    };
    if !candidate_is_self_consistent(&candidate) {
        return blocked(
            Stage5gRestartScenarioId::Grst10ConflictingReplayBlocks,
            Stage5gRestartReconciliationDisposition::TerminalInconsistency,
            Stage5gFreshTruthReductionReason::TerminalContradiction,
        );
    }
    Classified {
        scenario_id,
        disposition: Stage5gRestartReconciliationDisposition::ApplyOwnedCandidate,
        reason,
        candidate: Some(candidate),
        ignored_unrelated_terminal_order_count: rows.ignored_unrelated_terminal_order_count,
        ignored_unrelated_historical_trade_count: rows.ignored_unrelated_historical_trade_count,
    }
}

fn candidate_is_self_consistent(candidate: &Stage5gOwnedReconciliationCandidate) -> bool {
    let order = &candidate.order;
    let expected_position = stage5g_expected_post_position_qty(candidate.pre_position_qty, order);
    let observed_position = candidate
        .position
        .as_ref()
        .map(|position| position.qty)
        .unwrap_or(Decimal::ZERO);
    let trades_are_exact = candidate.trades.iter().all(|trade| {
        trade.side == order.side
            && stage5g_exact_trade_order_linkage(order, trade) == Stage5gTradeOrderLinkage::Exact
    });
    let trade_sum = candidate
        .trades
        .iter()
        .fold(Decimal::ZERO, |sum, trade| sum + trade.qty);
    let position_is_exact = !candidate.positions_complete || observed_position == expected_position;
    let source_price_authority_is_supported = !matches!(
        candidate.source_action,
        crate::Stage5gMockIntentAction::Place {
            place_kind: crate::Stage5gMockPlaceKind::Limit
        }
    );
    let status_is_consistent = match order.status {
        OrderStatus::New | OrderStatus::Working => {
            order.filled_qty == Decimal::ZERO
                && candidate.trades.is_empty()
                && candidate.positions_complete
                && position_is_exact
        }
        OrderStatus::PartiallyFilled => {
            order.filled_qty > Decimal::ZERO
                && order.filled_qty < order.qty
                && candidate.positions_complete
                && observed_position == expected_position
        }
        OrderStatus::Filled => {
            order.filled_qty == order.qty
                && candidate.positions_complete
                && observed_position == expected_position
        }
        OrderStatus::Rejected => {
            order.filled_qty == Decimal::ZERO
                && candidate.trades.is_empty()
                && candidate.positions_complete
                && observed_position == candidate.pre_position_qty
        }
        OrderStatus::Canceled | OrderStatus::Expired => {
            candidate.positions_complete && observed_position == expected_position
        }
        OrderStatus::Unknown(_) => false,
    };
    let target_identity_is_exact = match &candidate.source_action {
        crate::Stage5gMockIntentAction::Place { .. } => {
            order.client_order_id.as_ref() == Some(&candidate.command_client_order_id)
                && candidate.target_order_client_order_id.as_ref()
                    == Some(&candidate.command_client_order_id)
                && order.broker_order_id == candidate.target_broker_order_id
        }
        crate::Stage5gMockIntentAction::Cancel { .. } => {
            order.broker_order_id == candidate.target_broker_order_id
                && candidate
                    .target_order_client_order_id
                    .as_ref()
                    .map(|expected| order.client_order_id.as_ref() == Some(expected))
                    .unwrap_or(true)
        }
    };
    let immutable_target_order_authority_is_exact = candidate
        .target_order_immutable_commitment_sha256
        .as_ref()
        .map(|expected| stage5g_immutable_order_payload_commitment_sha256(order) == *expected)
        .unwrap_or(true);
    !candidate.command_request_id.is_empty()
        && candidate.operational_binding_commitment_sha256.len() == 64
        && target_identity_is_exact
        && source_price_authority_is_supported
        && stage5g_order_matches_source_action(&candidate.source_action, order)
        && candidate.account_wide_safety_proven
        && candidate.global_account_history_partition_proven
        && candidate.canonical_position_semantics_proven
        && candidate.source_monotonicity_proven
        && candidate.immutable_target_order_monotonicity_proven
        && immutable_target_order_authority_is_exact
        && candidate
            .side
            .map(|side| side == order.side)
            .unwrap_or(true)
        && candidate
            .target_qty
            .map(|qty| qty == order.qty)
            .unwrap_or(true)
        && trades_are_exact
        && trade_sum == order.filled_qty
        && (order.filled_qty == Decimal::ZERO
            || stage5g_intent_position_is_compatible(
                candidate.intent_class,
                candidate.pre_position_qty,
                expected_position,
                order,
            ))
        && status_is_consistent
        && match candidate.position.as_ref() {
            Some(position) => {
                position.account_id == order.account_id && position.instrument == order.instrument
            }
            None => true,
        }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Stage5gGlobalHistoryPartition {
    all_rows_classified: bool,
    ignored_unrelated_terminal_order_count: usize,
    ignored_unrelated_historical_trade_count: usize,
}

fn stage5g_global_history_partition(
    owned_slot_present: bool,
    target_orders: &[&BrokerOrderSnapshot],
    target_trades: &[&BrokerTradeSnapshot],
) -> Stage5gGlobalHistoryPartition {
    if owned_slot_present {
        return Stage5gGlobalHistoryPartition {
            all_rows_classified: true,
            ignored_unrelated_terminal_order_count: 0,
            ignored_unrelated_historical_trade_count: 0,
        };
    }
    let terminal_order_count = target_orders
        .iter()
        .filter(|order| order.lifecycle == BrokerOrderLifecycle::Terminal)
        .count();
    Stage5gGlobalHistoryPartition {
        all_rows_classified: terminal_order_count == target_orders.len(),
        ignored_unrelated_terminal_order_count: terminal_order_count,
        ignored_unrelated_historical_trade_count: target_trades.len(),
    }
}

fn observed_complete_position_qty(positions: &[&BrokerPositionSnapshot]) -> Option<Decimal> {
    match positions {
        [] => Some(Decimal::ZERO),
        [position] => Some(position.qty),
        _ => None,
    }
}

fn position_conflict(expected: Decimal, observed: Option<Decimal>) -> Classified {
    let direction_conflict = observed.is_some_and(|actual| {
        actual != Decimal::ZERO
            && expected != Decimal::ZERO
            && ((actual > Decimal::ZERO && expected < Decimal::ZERO)
                || (actual < Decimal::ZERO && expected > Decimal::ZERO))
    });
    blocked(
        Stage5gRestartScenarioId::Grst10ConflictingReplayBlocks,
        Stage5gRestartReconciliationDisposition::TerminalInconsistency,
        if direction_conflict {
            Stage5gFreshTruthReductionReason::PositionDirectionMismatch
        } else {
            Stage5gFreshTruthReductionReason::PositionQuantityMismatch
        },
    )
}

fn terminal_contradiction() -> Classified {
    blocked(
        Stage5gRestartScenarioId::Grst10ConflictingReplayBlocks,
        Stage5gRestartReconciliationDisposition::TerminalInconsistency,
        Stage5gFreshTruthReductionReason::TerminalContradiction,
    )
}

fn package_identity_commitment(truth: &Stage5gRestartBoundFreshBrokerTruthPackage) -> String {
    #[derive(Serialize)]
    struct PackageIdentityProjection<'a> {
        domain: &'static str,
        package_id: &'a str,
        snapshot_epoch: &'a str,
        canonical_fingerprint_sha256: &'a str,
    }
    semantic_sha256(&PackageIdentityProjection {
        domain: "moex.stage5g.fresh-truth-package-identity.v1",
        package_id: truth.package.package_id.as_str(),
        snapshot_epoch: truth.package.snapshot_epoch.as_str(),
        canonical_fingerprint_sha256: &truth.package.canonical_fingerprint_sha256,
    })
}

fn semantic_sha256<T: Serialize>(value: &T) -> String {
    let bytes = serde_json::to_vec(value).expect("accepted reducer projection is serializable");
    format!("{:x}", Sha256::digest(bytes))
}

fn candidate_fingerprint(candidate: &Stage5gOwnedReconciliationCandidate) -> String {
    #[derive(Serialize)]
    struct CandidateProjection<'a> {
        domain: &'static str,
        operational_binding_commitment_sha256: &'a str,
        command_request_id: &'a str,
        command_client_order_id: &'a ClientOrderId,
        target_broker_order_id: &'a Option<BrokerOrderId>,
        target_order_client_order_id: &'a Option<ClientOrderId>,
        intent_class: Stage5gRestartIntentClass,
        source_action: &'a crate::Stage5gMockIntentAction,
        side: Option<OrderSide>,
        target_qty: &'a Option<Decimal>,
        pre_position_qty: Decimal,
        expected_attribution_fingerprint_sha256: &'a Option<String>,
        order: &'a BrokerOrderSnapshot,
        trades: &'a [BrokerTradeSnapshot],
        position: &'a Option<BrokerPositionSnapshot>,
        positions_complete: bool,
        account_wide_safety_proven: bool,
        source_monotonicity_proven: bool,
        immutable_target_order_monotonicity_proven: bool,
        target_order_immutable_commitment_sha256: &'a Option<String>,
    }
    semantic_sha256(&CandidateProjection {
        domain: "moex.stage5g.fresh-truth-candidate.v1",
        operational_binding_commitment_sha256: &candidate.operational_binding_commitment_sha256,
        command_request_id: &candidate.command_request_id,
        command_client_order_id: &candidate.command_client_order_id,
        target_broker_order_id: &candidate.target_broker_order_id,
        target_order_client_order_id: &candidate.target_order_client_order_id,
        intent_class: candidate.intent_class,
        source_action: &candidate.source_action,
        side: candidate.side,
        target_qty: &candidate.target_qty,
        pre_position_qty: candidate.pre_position_qty,
        expected_attribution_fingerprint_sha256: &candidate.expected_attribution_fingerprint_sha256,
        order: &candidate.order,
        trades: &candidate.trades,
        position: &candidate.position,
        positions_complete: candidate.positions_complete,
        account_wide_safety_proven: candidate.account_wide_safety_proven,
        source_monotonicity_proven: candidate.source_monotonicity_proven,
        immutable_target_order_monotonicity_proven: candidate
            .immutable_target_order_monotonicity_proven,
        target_order_immutable_commitment_sha256: &candidate
            .target_order_immutable_commitment_sha256,
    })
}

const fn disposition_id(disposition: Stage5gRestartReconciliationDisposition) -> &'static str {
    match disposition {
        Stage5gRestartReconciliationDisposition::ExactReplay => "exact_replay",
        Stage5gRestartReconciliationDisposition::ContinueFromCommittedCheckpoint => {
            "continue_from_committed_checkpoint"
        }
        Stage5gRestartReconciliationDisposition::ApplyOwnedCandidate => "apply_owned_candidate",
        Stage5gRestartReconciliationDisposition::AwaitFreshBrokerTruth => {
            "await_fresh_broker_truth"
        }
        Stage5gRestartReconciliationDisposition::ReconciliationRequired => {
            "reconciliation_required"
        }
        Stage5gRestartReconciliationDisposition::ManualInterventionRequired => {
            "manual_intervention_required"
        }
        Stage5gRestartReconciliationDisposition::TerminalInconsistency => "terminal_inconsistency",
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::thread;

    use broker_core::{
        BrokerAccountId, BrokerOrderId, BrokerTradeId, ClientOrderId, Exchange, InstrumentId,
        Market, OrderType, TimeInForce,
    };
    use chrono::{TimeZone, Utc};

    use super::*;
    use crate::stage5g_clean_restart::Stage5gFreshTruthRestartProjection;
    use crate::stage5g_fresh_broker_truth::{
        bind_stage5g_fresh_truth_to_clean_restart, validate_stage5g_fresh_broker_truth_package,
        Stage5gBrokerId, Stage5gDeploymentGeneration, Stage5gDeploymentId, Stage5gFeedGeneration,
        Stage5gFreshBrokerTruthPackageV1, Stage5gFreshBrokerTruthValidationContext,
        Stage5gGatewayInstanceId, Stage5gOperationalIdentityInput, Stage5gOperationalIdentityV1,
        Stage5gPackageId, Stage5gReconciledFreshPackageIdentity, Stage5gSha256,
        Stage5gSnapshotEpoch, Stage5gStrategyDefinitionId, Stage5gStrategyInstanceId,
        Stage5gValidatedFreshBrokerTruthPackage, STAGE5G_FRESH_BROKER_TRUTH_SCHEMA_VERSION,
    };
    use crate::stage5g_order_position::{
        Stage5gFreshTruthRestartSlotProjection, Stage5gRestartIntentClass,
    };
    use crate::stage5g_timer::{Stage5gTimerCheckpointEnvelope, Stage5gTimerCheckpointPayload};

    const SHA: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    struct Case {
        restart: Stage5gFreshTruthRestartProjection,
        truth: Stage5gValidatedFreshBrokerTruthPackage,
        expected_scenario: Stage5gRestartScenarioId,
        expected_disposition: Stage5gRestartReconciliationDisposition,
        expected_reason: Stage5gFreshTruthReductionReason,
    }

    fn instrument() -> InstrumentId {
        InstrumentId {
            symbol: "IMOEXF".to_owned(),
            venue_symbol: Some("IMOEXF@RTSX".to_owned()),
            exchange: Exchange::Moex,
            market: Market::Futures,
        }
    }

    fn checkpoint() -> Stage5gTimerCheckpointEnvelope {
        Stage5gTimerCheckpointEnvelope {
            payload: Stage5gTimerCheckpointPayload {
                schema_version: 1,
                package_discriminator: Some("restart-package".to_owned()),
                current_evidence_identity: Some("restart-evidence".to_owned()),
                evidence_replay_ledger: Vec::new(),
                last_broker_truth_received_at: None,
                last_broker_truth_received_ms: None,
                duplicate_evidence_count: 0,
                last_total_sequence: None,
                last_continuation_checkpoint_ts_utc_ms: Some(1_767_000_000_000),
            },
            payload_sha256: SHA.to_owned(),
        }
    }

    fn restart(
        lifecycle_kind: Stage5gCleanRestartLifecycleKind,
        slot: Option<Stage5gFreshTruthRestartSlotProjection>,
        escrow: bool,
    ) -> Stage5gFreshTruthRestartProjection {
        Stage5gFreshTruthRestartProjection {
            lifecycle_kind,
            strategy_id: "hybrid_imoexf".to_owned(),
            account_id: BrokerAccountId::new("ACC_TEST_0001"),
            instrument_id: instrument(),
            config_fingerprint_sha256: SHA.to_owned(),
            strategy_state_fingerprint_sha256: SHA.to_owned(),
            reconstructed_runtime_state_fingerprint_sha256: SHA.to_owned(),
            callback_count: usize::from(
                lifecycle_kind == Stage5gCleanRestartLifecycleKind::TimerReady,
            ),
            request_count: usize::from(slot.is_some()),
            terminal_request_count: slot.as_ref().is_some_and(|slot| slot.terminal) as usize,
            source_lifecycle_commit_sha256: SHA.to_owned(),
            lifecycle_source_authority_sha256: SHA.to_owned(),
            checkpoint: checkpoint(),
            committed_position_qty: Decimal::ZERO,
            committed_position_numeric_authority_is_integral: true,
            slots: slot.into_iter().collect(),
            generated_intent_escrow_fingerprint_sha256: escrow.then(|| SHA.to_owned()),
        }
    }

    fn slot(
        broker_order_id: Option<&str>,
        terminal: bool,
    ) -> Stage5gFreshTruthRestartSlotProjection {
        Stage5gFreshTruthRestartSlotProjection {
            command_request_id: "00000000-0000-0000-0000-000000000001".to_owned(),
            command_client_order_id: ClientOrderId::new("CLIENT-1").expect("client id"),
            target_broker_order_id: broker_order_id.map(BrokerOrderId::new),
            target_order_client_order_id: Some(ClientOrderId::new("CLIENT-1").expect("client id")),
            cancel_target_order_authority: None,
            intent_class: Stage5gRestartIntentClass::Entry,
            source_action: crate::Stage5gMockIntentAction::Place {
                place_kind: crate::Stage5gMockPlaceKind::Market,
            },
            side: Some(OrderSide::Buy),
            target_qty: Some(Decimal::ONE),
            pre_position_qty: Decimal::ZERO,
            source_numeric_authority_is_integral: true,
            expected_attribution_fingerprint_sha256: None,
            latest_order: None,
            trades: Vec::new(),
            position: None,
            terminal,
        }
    }

    fn operational_identity() -> Stage5gOperationalIdentityV1 {
        Stage5gOperationalIdentityV1 {
            broker_id: Stage5gBrokerId::parse("mock").expect("broker"),
            account_id: BrokerAccountId::new("ACC_TEST_0001"),
            strategy_definition_id: Stage5gStrategyDefinitionId::parse("hybrid_imoexf")
                .expect("strategy"),
            strategy_instance_id: Stage5gStrategyInstanceId::parse("instance-1").expect("instance"),
            deployment_id: Stage5gDeploymentId::parse("deployment-1").expect("deployment"),
            deployment_generation: Stage5gDeploymentGeneration::parse(1).expect("generation"),
            gateway_instance_id: Stage5gGatewayInstanceId::parse("gateway-1").expect("gateway"),
            config_fingerprint_sha256: Stage5gSha256::parse(SHA).expect("config hash"),
            instrument_map_fingerprint_sha256: Stage5gSha256::parse(SHA).expect("instrument hash"),
            market_data_generation: Stage5gFeedGeneration::parse(1).expect("market generation"),
            command_consumer_generation: Stage5gFeedGeneration::parse(1)
                .expect("command generation"),
            target_instrument: instrument(),
        }
    }

    fn order(
        status: OrderStatus,
        broker_order_id: &str,
        filled_qty: Decimal,
    ) -> BrokerOrderSnapshot {
        let qty = if status == OrderStatus::PartiallyFilled {
            Decimal::new(2, 0)
        } else {
            Decimal::ONE
        };
        BrokerOrderSnapshot {
            account_id: BrokerAccountId::new("ACC_TEST_0001"),
            broker_order_id: Some(BrokerOrderId::new(broker_order_id)),
            client_order_id: Some(ClientOrderId::new("CLIENT-1").expect("client id")),
            instrument: instrument(),
            side: OrderSide::Buy,
            order_type: OrderType::Market,
            time_in_force: None,
            lifecycle: BrokerOrderSnapshot::lifecycle_for(&status),
            status,
            qty,
            filled_qty,
            remaining_qty: Some(qty - filled_qty),
            limit_price: None,
            broker_asset_id: None,
            board: None,
            expiration_date: None,
            source_ts: Some(Utc.timestamp_millis_opt(1_767_000_000_100).unwrap()),
            received_ts: Utc.timestamp_millis_opt(1_767_000_000_200).unwrap(),
        }
    }

    fn trade(qty: Decimal, broker_order_id: &str) -> BrokerTradeSnapshot {
        BrokerTradeSnapshot {
            account_id: BrokerAccountId::new("ACC_TEST_0001"),
            broker_trade_id: BrokerTradeId::new("TRADE-1"),
            broker_order_id: Some(BrokerOrderId::new(broker_order_id)),
            client_order_id: Some(ClientOrderId::new("CLIENT-1").expect("client id")),
            instrument: instrument(),
            side: OrderSide::Buy,
            qty,
            price: Decimal::new(2200, 0),
            gross_amount: None,
            commission: None,
            broker_asset_id: None,
            board: None,
            expiration_date: None,
            source_ts: Utc.timestamp_millis_opt(1_767_000_000_100).unwrap(),
            received_ts: Utc.timestamp_millis_opt(1_767_000_000_200).unwrap(),
        }
    }

    fn position(qty: Decimal) -> BrokerPositionSnapshot {
        BrokerPositionSnapshot {
            account_id: BrokerAccountId::new("ACC_TEST_0001"),
            instrument: instrument(),
            qty,
            avg_price: Some(Decimal::new(2200, 0)),
            unrealized_pnl: None,
            source_ts: Some(Utc.timestamp_millis_opt(1_767_000_000_100).unwrap()),
            received_ts: Utc.timestamp_millis_opt(1_767_000_000_200).unwrap(),
        }
    }

    fn truth(
        lineage: Stage5gFreshPackageLineage,
        orders_complete: bool,
        trades_complete: bool,
        positions_complete: bool,
        orders: Vec<BrokerOrderSnapshot>,
        trades: Vec<BrokerTradeSnapshot>,
        positions: Vec<BrokerPositionSnapshot>,
    ) -> Stage5gValidatedFreshBrokerTruthPackage {
        Stage5gValidatedFreshBrokerTruthPackage {
            package_id: Stage5gPackageId::parse("fresh-package-2").expect("package"),
            snapshot_epoch: Stage5gSnapshotEpoch::parse("snapshot-epoch-2").expect("epoch"),
            operational_identity: operational_identity(),
            operational_authority_commitment_sha256: SHA.to_owned(),
            captured_at: Utc.timestamp_millis_opt(1_767_000_001_000).unwrap(),
            orders_observed_at: Utc.timestamp_millis_opt(1_767_000_000_500).unwrap(),
            trades_observed_at: Utc.timestamp_millis_opt(1_767_000_000_500).unwrap(),
            positions_observed_at: Utc.timestamp_millis_opt(1_767_000_000_500).unwrap(),
            orders_complete,
            trades_complete,
            positions_complete,
            orders,
            trades,
            positions,
            lineage,
            canonical_fingerprint_sha256: SHA.to_owned(),
            replay_hints: super::super::Stage5gFreshTruthReplayHintsV1 {
                pre_restart_package_id: "restart-evidence".to_owned(),
                pre_restart_snapshot_epoch: "restart-package".to_owned(),
                untrusted_last_reconciled_hint: Some(
                    super::super::Stage5gReconciledFreshPackageIdentity::validate(
                        "fresh-package-2",
                        "snapshot-epoch-2",
                        SHA,
                    )
                    .expect("replay identity"),
                ),
                untrusted_accepted_replay_hints: Vec::new(),
                untrusted_known_historical_hints: Vec::new(),
            },
        }
    }

    fn clone_truth(
        truth: &Stage5gValidatedFreshBrokerTruthPackage,
    ) -> Stage5gValidatedFreshBrokerTruthPackage {
        Stage5gValidatedFreshBrokerTruthPackage {
            package_id: truth.package_id.clone(),
            snapshot_epoch: truth.snapshot_epoch.clone(),
            operational_identity: truth.operational_identity.clone(),
            operational_authority_commitment_sha256: truth
                .operational_authority_commitment_sha256
                .clone(),
            captured_at: truth.captured_at,
            orders_observed_at: truth.orders_observed_at,
            trades_observed_at: truth.trades_observed_at,
            positions_observed_at: truth.positions_observed_at,
            orders_complete: truth.orders_complete,
            trades_complete: truth.trades_complete,
            positions_complete: truth.positions_complete,
            orders: truth.orders.clone(),
            trades: truth.trades.clone(),
            positions: truth.positions.clone(),
            lineage: truth.lineage,
            canonical_fingerprint_sha256: truth.canonical_fingerprint_sha256.clone(),
            replay_hints: truth.replay_hints.clone(),
        }
    }

    fn raw_package_from_validated(
        value: &Stage5gValidatedFreshBrokerTruthPackage,
    ) -> Stage5gFreshBrokerTruthPackageV1 {
        let identity = &value.operational_identity;
        Stage5gFreshBrokerTruthPackageV1 {
            schema_version: STAGE5G_FRESH_BROKER_TRUTH_SCHEMA_VERSION,
            package_id: value.package_id.as_str().to_owned(),
            operational_identity: Stage5gOperationalIdentityInput {
                broker_id: identity.broker_id.as_str().to_owned(),
                account_id: identity.account_id.clone(),
                strategy_definition_id: identity.strategy_definition_id.as_str().to_owned(),
                strategy_instance_id: identity.strategy_instance_id.as_str().to_owned(),
                deployment_id: identity.deployment_id.as_str().to_owned(),
                deployment_generation: identity.deployment_generation.0,
                gateway_instance_id: identity.gateway_instance_id.as_str().to_owned(),
                config_fingerprint_sha256: identity.config_fingerprint_sha256.as_str().to_owned(),
                instrument_map_fingerprint_sha256: identity
                    .instrument_map_fingerprint_sha256
                    .as_str()
                    .to_owned(),
                market_data_generation: identity.market_data_generation.0,
                command_consumer_generation: identity.command_consumer_generation.0,
                target_instrument: identity.target_instrument.clone(),
            },
            snapshot_epoch: value.snapshot_epoch.as_str().to_owned(),
            captured_at: value.captured_at,
            orders_observed_at: value.orders_observed_at,
            trades_observed_at: value.trades_observed_at,
            positions_observed_at: value.positions_observed_at,
            orders_complete: value.orders_complete,
            trades_complete: value.trades_complete,
            positions_complete: value.positions_complete,
            orders: value.orders.clone(),
            trades: value.trades.clone(),
            positions: value.positions.clone(),
        }
    }

    fn bound_truth(
        restart: &Stage5gFreshTruthRestartProjection,
        truth: &Stage5gValidatedFreshBrokerTruthPackage,
    ) -> Stage5gRestartBoundFreshBrokerTruthPackage {
        Stage5gRestartBoundFreshBrokerTruthPackage {
            operational_binding_commitment_sha256: stage5g_operational_binding_commitment(
                restart,
                &truth.operational_identity,
                &truth.replay_hints,
            ),
            restart_replay_commitment_sha256: stage5g_restart_replay_commitment(
                restart,
                &truth.replay_hints,
            ),
            package: clone_truth(truth),
        }
    }

    fn classify_case(
        restart: &Stage5gFreshTruthRestartProjection,
        truth: &Stage5gValidatedFreshBrokerTruthPackage,
    ) -> Classified {
        classify(restart, &bound_truth(restart, truth))
    }

    struct OwningTruthRows {
        orders_complete: bool,
        trades_complete: bool,
        positions_complete: bool,
        orders: Vec<BrokerOrderSnapshot>,
        trades: Vec<BrokerTradeSnapshot>,
        positions: Vec<BrokerPositionSnapshot>,
    }

    impl OwningTruthRows {
        fn complete(
            orders: Vec<BrokerOrderSnapshot>,
            trades: Vec<BrokerTradeSnapshot>,
            positions: Vec<BrokerPositionSnapshot>,
        ) -> Self {
            Self {
                orders_complete: true,
                trades_complete: true,
                positions_complete: true,
                orders,
                trades,
                positions,
            }
        }
    }

    fn validated_owning_package(
        restart: &Stage5gCleanRestartedCapability,
        package_id: &str,
        snapshot_epoch: &str,
        rows: OwningTruthRows,
    ) -> Stage5gValidatedFreshBrokerTruthPackage {
        let projection = restart.fresh_truth_reducer_projection();
        let identity_input = operational_identity_input_for_restart(restart);
        let reviewed_authority =
            super::super::stage5g_test_reviewed_operational_identity_authority(
                identity_input.clone(),
            )
            .expect("test deployment identity passes reviewed authority issuer");
        let operational_authority =
            super::super::authorize_stage5g_fresh_truth_operational_identity(
                restart,
                reviewed_authority,
            )
            .expect("reviewed identity is authorized by authenticated restart");
        let current_id = projection
            .checkpoint
            .payload
            .current_evidence_identity
            .clone()
            .expect("authenticated checkpoint current identity");
        let current_epoch = projection
            .checkpoint
            .payload
            .package_discriminator
            .clone()
            .expect("authenticated checkpoint package discriminator");
        let current_fingerprint = projection
            .checkpoint
            .payload
            .evidence_replay_ledger
            .iter()
            .find(|entry| entry.identity == current_id)
            .expect("current replay row")
            .fingerprint_sha256
            .clone();
        let last = Stage5gReconciledFreshPackageIdentity::validate(
            current_id.clone(),
            current_epoch.clone(),
            current_fingerprint,
        )
        .expect("current replay tuple");
        let accepted = projection
            .checkpoint
            .payload
            .evidence_replay_ledger
            .iter()
            .filter(|entry| entry.identity != current_id)
            .map(|entry| {
                let epoch = entry
                    .identity
                    .splitn(4, ':')
                    .nth(3)
                    .expect("canonical replay identity epoch");
                Stage5gReconciledFreshPackageIdentity::validate(
                    entry.identity.clone(),
                    epoch,
                    entry.fingerprint_sha256.clone(),
                )
                .expect("historical replay tuple")
            })
            .collect::<Vec<_>>();
        let restore_completed_at = Utc.with_ymd_and_hms(2020, 1, 1, 0, 0, 0).unwrap();
        let captured_at = Utc.with_ymd_and_hms(2030, 1, 1, 0, 0, 10).unwrap();
        validate_stage5g_fresh_broker_truth_package(
            Stage5gFreshBrokerTruthPackageV1 {
                schema_version: STAGE5G_FRESH_BROKER_TRUTH_SCHEMA_VERSION,
                package_id: package_id.to_owned(),
                operational_identity: identity_input,
                snapshot_epoch: snapshot_epoch.to_owned(),
                captured_at,
                orders_observed_at: captured_at,
                trades_observed_at: captured_at,
                positions_observed_at: captured_at,
                orders_complete: rows.orders_complete,
                trades_complete: rows.trades_complete,
                positions_complete: rows.positions_complete,
                orders: rows.orders,
                trades: rows.trades,
                positions: rows.positions,
            },
            Stage5gFreshBrokerTruthValidationContext {
                operational_authority,
                pre_restart_package_id: &current_id,
                pre_restart_snapshot_epoch: &current_epoch,
                untrusted_last_reconciled_hint: Some(&last),
                untrusted_accepted_replay_hints: &accepted,
                untrusted_known_historical_hints: &[],
                clean_restore_completed_at: restore_completed_at,
                validation_observed_at: captured_at,
            },
        )
        .expect("fresh package validates after authenticated reconstruction")
    }

    fn operational_identity_input_for_restart(
        restart: &Stage5gCleanRestartedCapability,
    ) -> Stage5gOperationalIdentityInput {
        let projection = restart.fresh_truth_reducer_projection();
        Stage5gOperationalIdentityInput {
            broker_id: "finam-mock".to_owned(),
            account_id: projection.account_id.clone(),
            strategy_definition_id: projection.strategy_id.clone(),
            strategy_instance_id: "hybrid-imoexf-paper-1".to_owned(),
            deployment_id: "stage5g-edb-r1".to_owned(),
            deployment_generation: 11,
            gateway_instance_id: "mock-gateway-edb-r1".to_owned(),
            config_fingerprint_sha256: projection.config_fingerprint_sha256.clone(),
            instrument_map_fingerprint_sha256: "b".repeat(64),
            market_data_generation: 7,
            command_consumer_generation: 9,
            target_instrument: projection.instrument_id.clone(),
        }
    }

    fn bound_owning_package(
        restart: &Stage5gCleanRestartedCapability,
        package_id: &str,
        snapshot_epoch: &str,
        rows: OwningTruthRows,
    ) -> Stage5gRestartBoundFreshBrokerTruthPackage {
        let validated = validated_owning_package(restart, package_id, snapshot_epoch, rows);
        bind_stage5g_fresh_truth_to_clean_restart(restart, validated)
            .expect("validated fresh truth binds to authenticated restart")
    }

    fn discovered_order_for_slot(
        slot: &Stage5gFreshTruthRestartSlotProjection,
        status: OrderStatus,
        broker_order_id: &str,
    ) -> BrokerOrderSnapshot {
        let mut discovered = order(status, broker_order_id, Decimal::ZERO);
        discovered.client_order_id = slot.target_order_client_order_id.clone();
        discovered.side = slot.side.unwrap_or(OrderSide::Buy);
        discovered.qty = slot.target_qty.unwrap_or(Decimal::ONE);
        discovered.remaining_qty = Some(discovered.qty);
        match &slot.source_action {
            crate::Stage5gMockIntentAction::Place {
                place_kind: crate::Stage5gMockPlaceKind::Market,
            } => {
                discovered.order_type = broker_core::OrderType::Market;
                discovered.limit_price = None;
            }
            crate::Stage5gMockIntentAction::Place {
                place_kind: crate::Stage5gMockPlaceKind::Limit,
            } => {
                discovered.order_type = broker_core::OrderType::Limit;
                discovered.limit_price = Some(Decimal::new(2200, 0));
            }
            crate::Stage5gMockIntentAction::Cancel { target_order_id } => {
                discovered.broker_order_id = Some(target_order_id.clone());
            }
        }
        discovered
    }

    fn unrelated_instrument() -> InstrumentId {
        InstrumentId {
            symbol: "RTS-9.26".to_owned(),
            venue_symbol: Some("RTS-9.26@RTSX".to_owned()),
            exchange: Exchange::Moex,
            market: Market::Futures,
        }
    }

    #[test]
    fn stage5g_edb_r2_twelve_prebind_operational_authority_mismatches_fail() {
        for field_index in 0..12 {
            let restart =
                crate::stage5g_order_position::tests::stage5g_edb_restored_awaiting_fixture();
            let validated = validated_owning_package(
                &restart,
                &format!("stage5g-edb-r2-authority-{field_index}"),
                &format!("stage5g-edb-r2-authority-epoch-{field_index}"),
                OwningTruthRows::complete(vec![], vec![], vec![]),
            );
            let mut raw = raw_package_from_validated(&validated);
            let reviewed_identity = raw.operational_identity.clone();
            match field_index {
                0 => raw.operational_identity.broker_id = "other-broker".to_owned(),
                1 => raw.operational_identity.account_id = BrokerAccountId::new("ACC_TEST_0002"),
                2 => raw.operational_identity.strategy_definition_id = "other-strategy".to_owned(),
                3 => raw.operational_identity.strategy_instance_id = "other-instance".to_owned(),
                4 => raw.operational_identity.deployment_id = "other-deployment".to_owned(),
                5 => raw.operational_identity.deployment_generation += 1,
                6 => raw.operational_identity.gateway_instance_id = "other-gateway".to_owned(),
                7 => raw.operational_identity.config_fingerprint_sha256 = "c".repeat(64),
                8 => raw.operational_identity.instrument_map_fingerprint_sha256 = "d".repeat(64),
                9 => raw.operational_identity.market_data_generation += 1,
                10 => raw.operational_identity.command_consumer_generation += 1,
                11 => raw.operational_identity.target_instrument.venue_symbol = None,
                _ => unreachable!(),
            }
            let reviewed_authority =
                super::super::stage5g_test_reviewed_operational_identity_authority(
                    reviewed_identity,
                )
                .expect("independent reviewed identity passes the test-only issuer");
            let operational_authority =
                super::super::authorize_stage5g_fresh_truth_operational_identity(
                    &restart,
                    reviewed_authority,
                )
                .expect("reviewed authority is independently restart-bound");
            let projection = restart.fresh_truth_reducer_projection();
            let current_id = projection
                .checkpoint
                .payload
                .current_evidence_identity
                .as_deref()
                .expect("authenticated current identity");
            let current_epoch = projection
                .checkpoint
                .payload
                .package_discriminator
                .as_deref()
                .expect("authenticated current epoch");
            let result = validate_stage5g_fresh_broker_truth_package(
                raw,
                Stage5gFreshBrokerTruthValidationContext {
                    operational_authority,
                    pre_restart_package_id: current_id,
                    pre_restart_snapshot_epoch: current_epoch,
                    untrusted_last_reconciled_hint: None,
                    untrusted_accepted_replay_hints: &[],
                    untrusted_known_historical_hints: &[],
                    clean_restore_completed_at: Utc.with_ymd_and_hms(2020, 1, 1, 0, 0, 0).unwrap(),
                    validation_observed_at: Utc.with_ymd_and_hms(2030, 1, 1, 0, 0, 10).unwrap(),
                },
            );
            assert_eq!(
                result.err(),
                Some(super::super::Stage5gFreshBrokerTruthError::OperationalIdentityMismatch),
                "pre-bind field {field_index} survived"
            );
        }
    }

    #[test]
    fn stage5g_edb_r2_account_wide_order_safety_is_owning_and_fail_closed() {
        for (status, expected_reason) in [
            (
                OrderStatus::Working,
                Stage5gFreshTruthReductionReason::AccountWideActiveOrderConflict,
            ),
            (
                OrderStatus::Unknown("broker-native-new-state".to_owned()),
                Stage5gFreshTruthReductionReason::AccountWideUnknownOrderConflict,
            ),
        ] {
            let restart =
                crate::stage5g_order_position::tests::stage5g_edb_restored_awaiting_fixture();
            let projection = restart.fresh_truth_reducer_projection();
            let slot = projection.slots.first().expect("awaiting slot");
            let mut unrelated = order(status.clone(), "UNRELATED-ORDER", Decimal::ZERO);
            unrelated.instrument = unrelated_instrument();
            unrelated.client_order_id =
                Some(ClientOrderId::new("UNRELATED-CLIENT").expect("client id"));
            unrelated.lifecycle = BrokerOrderSnapshot::lifecycle_for(&status);
            let mut orders = slot.latest_order.clone().into_iter().collect::<Vec<_>>();
            orders.push(unrelated);
            let bound = bound_owning_package(
                &restart,
                "stage5g-edb-r2-account-wide-block",
                "stage5g-edb-r2-account-wide-block-epoch",
                OwningTruthRows::complete(
                    orders,
                    slot.trades.clone(),
                    slot.position.clone().into_iter().collect(),
                ),
            );
            let evidence = reduce_stage5g_fresh_broker_truth(restart, bound).evidence();
            assert_eq!(evidence.reason, expected_reason);
            assert!(!evidence.candidate_present);
        }

        let restart = crate::stage5g_order_position::tests::stage5g_edb_restored_awaiting_fixture();
        let projection = restart.fresh_truth_reducer_projection();
        let slot = projection.slots.first().expect("awaiting slot");
        let source_order = slot.latest_order.clone().expect("source order");
        let mut client_owned = source_order.clone();
        client_owned.broker_order_id = Some(BrokerOrderId::new("CLIENT-OWNED-ORDER"));
        let mut broker_owned = source_order.clone();
        broker_owned.client_order_id =
            Some(ClientOrderId::new("BROKER-OWNED-CLIENT").expect("client id"));
        let bound = bound_owning_package(
            &restart,
            "stage5g-edb-r2-ambiguous-owned",
            "stage5g-edb-r2-ambiguous-owned-epoch",
            OwningTruthRows::complete(
                vec![client_owned, broker_owned],
                slot.trades.clone(),
                slot.position.clone().into_iter().collect(),
            ),
        );
        let evidence = reduce_stage5g_fresh_broker_truth(restart, bound).evidence();
        assert_eq!(
            evidence.reason,
            Stage5gFreshTruthReductionReason::TargetOrderIdentityConflict
        );
        assert!(!evidence.candidate_present);

        let restart = crate::stage5g_order_position::tests::stage5g_edb_restored_awaiting_fixture();
        let projection = restart.fresh_truth_reducer_projection();
        let slot = projection.slots.first().expect("awaiting slot");
        let mut harmless_terminal = order(OrderStatus::Canceled, "OLD-ORDER", Decimal::ZERO);
        harmless_terminal.instrument = unrelated_instrument();
        harmless_terminal.client_order_id =
            Some(ClientOrderId::new("OLD-CLIENT").expect("client id"));
        let mut orders = slot.latest_order.clone().into_iter().collect::<Vec<_>>();
        orders.push(harmless_terminal);
        let bound = bound_owning_package(
            &restart,
            "stage5g-edb-r2-harmless-terminal",
            "stage5g-edb-r2-harmless-terminal-epoch",
            OwningTruthRows::complete(
                orders,
                slot.trades.clone(),
                slot.position.clone().into_iter().collect(),
            ),
        );
        let evidence = reduce_stage5g_fresh_broker_truth(restart, bound).evidence();
        assert_ne!(
            evidence.reason,
            Stage5gFreshTruthReductionReason::AccountWideActiveOrderConflict
        );
        assert_ne!(
            evidence.reason,
            Stage5gFreshTruthReductionReason::AccountWideUnknownOrderConflict
        );
    }

    #[test]
    fn stage5g_edb_r2_source_market_limit_and_cancel_actions_are_owning() {
        let restart =
            crate::stage5g_order_position::tests::stage5g_edb_restored_generated_escrow_fixture();
        let projection = restart.fresh_truth_reducer_projection();
        let slot = projection.slots.first().expect("market source slot");
        let mut limit = discovered_order_for_slot(slot, OrderStatus::Working, "MARKET-AS-LIMIT");
        limit.order_type = broker_core::OrderType::Limit;
        limit.limit_price = Some(Decimal::new(2200, 0));
        let bound = bound_owning_package(
            &restart,
            "stage5g-edb-r2-market-as-limit",
            "stage5g-edb-r2-market-as-limit-epoch",
            OwningTruthRows::complete(vec![limit], vec![], vec![]),
        );
        let evidence = reduce_stage5g_fresh_broker_truth(restart, bound).evidence();
        assert_eq!(
            evidence.reason,
            Stage5gFreshTruthReductionReason::SourceOrderActionConflict
        );

        let restart = crate::stage5g_order_position::tests::
            stage5g_edb_restored_generated_limit_escrow_fixture();
        let projection = restart.fresh_truth_reducer_projection();
        let slot = projection.slots.first().expect("limit source slot");
        let mut market = discovered_order_for_slot(slot, OrderStatus::Working, "LIMIT-AS-MARKET");
        market.order_type = broker_core::OrderType::Market;
        market.limit_price = None;
        let bound = bound_owning_package(
            &restart,
            "stage5g-edb-r2-limit-as-market",
            "stage5g-edb-r2-limit-as-market-epoch",
            OwningTruthRows::complete(vec![market], vec![], vec![]),
        );
        let evidence = reduce_stage5g_fresh_broker_truth(restart, bound).evidence();
        assert_eq!(
            evidence.reason,
            Stage5gFreshTruthReductionReason::SourceLimitPriceAuthorityUnsupported
        );

        for (package_id, order_type) in [
            (
                "stage5g-edb-r2-cancel-wrong-target",
                broker_core::OrderType::Limit,
            ),
            (
                "stage5g-edb-r2-cancel-as-place",
                broker_core::OrderType::Market,
            ),
        ] {
            let restart = crate::stage5g_order_position::tests::
                stage5g_edb_restored_generated_cancel_escrow_fixture();
            let projection = restart.fresh_truth_reducer_projection();
            let slot = projection.slots.first().expect("cancel source slot");
            let mut row = discovered_order_for_slot(slot, OrderStatus::Working, "IGNORED");
            row.broker_order_id = Some(BrokerOrderId::new("WRONG-CANCEL-TARGET"));
            row.order_type = order_type;
            row.limit_price =
                (order_type == broker_core::OrderType::Limit).then_some(Decimal::new(2200, 0));
            let bound = bound_owning_package(
                &restart,
                package_id,
                &format!("{package_id}-epoch"),
                OwningTruthRows::complete(vec![row], vec![], vec![]),
            );
            let evidence = reduce_stage5g_fresh_broker_truth(restart, bound).evidence();
            assert_eq!(
                evidence.reason,
                Stage5gFreshTruthReductionReason::AccountWideActiveOrderConflict
            );
            assert!(!evidence.candidate_present);
        }

        let restart = crate::stage5g_order_position::tests::
            stage5g_edb_restored_generated_cancel_escrow_fixture();
        let projection = restart.fresh_truth_reducer_projection();
        let slot = projection.slots.first().expect("cancel source slot");
        let row = discovered_order_for_slot(slot, OrderStatus::Working, "IGNORED");
        let bound = bound_owning_package(
            &restart,
            "stage5g-edb-r2-cancel-target-exact",
            "stage5g-edb-r2-cancel-target-exact-epoch",
            OwningTruthRows::complete(vec![row], vec![], vec![]),
        );
        let reduction = reduce_stage5g_fresh_broker_truth(restart, bound);
        assert!(matches!(
            reduction
                .candidate
                .as_ref()
                .map(|candidate| &candidate.source_action),
            Some(crate::Stage5gMockIntentAction::Cancel { target_order_id })
                if target_order_id.as_str() == "CANCEL-TARGET-R2"
        ));
    }

    #[test]
    fn stage5g_edb_r2_source_fresh_monotonicity_is_owning() {
        let restart =
            crate::stage5g_order_position::tests::stage5g_edb_restored_terminal_applied_fixture();
        let projection = restart.fresh_truth_reducer_projection();
        let slot = projection.slots.first().expect("terminal slot");
        let mut regressed = slot.latest_order.clone().expect("terminal source order");
        regressed.status = OrderStatus::Working;
        regressed.lifecycle = BrokerOrderSnapshot::lifecycle_for(&regressed.status);
        regressed.filled_qty -= Decimal::new(1, 1);
        regressed.remaining_qty = Some(regressed.qty - regressed.filled_qty);
        let mut regressed_trades = slot.trades.clone();
        regressed_trades[0].qty = regressed.filled_qty;
        let bound = bound_owning_package(
            &restart,
            "stage5g-edb-r2-terminal-regression",
            "stage5g-edb-r2-terminal-regression-epoch",
            OwningTruthRows::complete(
                vec![regressed],
                regressed_trades,
                slot.position.clone().into_iter().collect(),
            ),
        );
        let evidence = reduce_stage5g_fresh_broker_truth(restart, bound).evidence();
        assert_eq!(
            evidence.reason,
            Stage5gFreshTruthReductionReason::OrderTerminalRegression
        );

        for payload_conflict in [false, true] {
            let restart =
                crate::stage5g_order_position::tests::stage5g_edb_restored_awaiting_fixture();
            let projection = restart.fresh_truth_reducer_projection();
            let slot = projection.slots.first().expect("partial source slot");
            let trades = if payload_conflict {
                let mut changed = slot.trades.clone();
                changed[0].price += Decimal::ONE;
                changed
            } else {
                Vec::new()
            };
            let bound = bound_owning_package(
                &restart,
                if payload_conflict {
                    "stage5g-edb-r2-trade-payload-conflict"
                } else {
                    "stage5g-edb-r2-committed-trade-missing"
                },
                if payload_conflict {
                    "stage5g-edb-r2-trade-payload-conflict-epoch"
                } else {
                    "stage5g-edb-r2-committed-trade-missing-epoch"
                },
                OwningTruthRows::complete(
                    slot.latest_order.clone().into_iter().collect(),
                    trades,
                    slot.position.clone().into_iter().collect(),
                ),
            );
            let evidence = reduce_stage5g_fresh_broker_truth(restart, bound).evidence();
            assert_eq!(
                evidence.reason,
                if payload_conflict {
                    Stage5gFreshTruthReductionReason::CommittedTradePayloadConflict
                } else {
                    Stage5gFreshTruthReductionReason::CommittedTradeMissing
                }
            );
            assert!(!evidence.candidate_present);
        }

        let restart = crate::stage5g_order_position::tests::stage5g_edb_restored_awaiting_fixture();
        let projection = restart.fresh_truth_reducer_projection();
        let slot = projection.slots.first().expect("partial source slot");
        let mut filled = slot.latest_order.clone().expect("source order");
        filled.status = OrderStatus::Filled;
        filled.lifecycle = BrokerOrderSnapshot::lifecycle_for(&filled.status);
        filled.filled_qty = filled.qty;
        filled.remaining_qty = Some(Decimal::ZERO);
        let mut added = slot.trades.first().cloned().expect("committed trade");
        added.broker_trade_id = BrokerTradeId::new("R2-MONOTONIC-TRADE");
        added.qty = filled.qty - slot.trades[0].qty;
        let expected_position = stage5g_expected_post_position_qty(slot.pre_position_qty, &filled);
        let bound = bound_owning_package(
            &restart,
            "stage5g-edb-r2-monotonic-supersession",
            "stage5g-edb-r2-monotonic-supersession-epoch",
            OwningTruthRows::complete(
                vec![filled],
                vec![slot.trades[0].clone(), added],
                vec![position(expected_position)],
            ),
        );
        let evidence = reduce_stage5g_fresh_broker_truth(restart, bound).evidence();
        assert_eq!(
            evidence.scenario_id,
            Stage5gRestartScenarioId::Grst11FreshBrokerTruthOverridesStaleHint.frozen_id()
        );
        assert!(evidence.candidate_present);

        let restart =
            crate::stage5g_order_position::tests::stage5g_edb_restored_terminal_applied_fixture();
        let projection = restart.fresh_truth_reducer_projection();
        let slot = projection.slots.first().expect("terminal slot");
        let bound = bound_owning_package(
            &restart,
            "stage5g-edb-r2-exact-terminal",
            "stage5g-edb-r2-exact-terminal-epoch",
            OwningTruthRows::complete(
                slot.latest_order.clone().into_iter().collect(),
                slot.trades.clone(),
                slot.position.clone().into_iter().collect(),
            ),
        );
        let evidence = reduce_stage5g_fresh_broker_truth(restart, bound).evidence();
        assert_eq!(
            evidence.scenario_id,
            Stage5gRestartScenarioId::Grst06RestartAfterTerminalPositionApplied.frozen_id()
        );
        assert!(!evidence.candidate_present);
    }

    #[test]
    fn stage5g_edb_r2_semantic_instrument_conflicts_and_fractional_source_block() {
        for row_kind in 0..3 {
            let restart =
                crate::stage5g_order_position::tests::stage5g_edb_restored_awaiting_fixture();
            let projection = restart.fresh_truth_reducer_projection();
            let slot = projection.slots.first().expect("awaiting slot");
            let mut orders = slot.latest_order.clone().into_iter().collect::<Vec<_>>();
            let mut trades = slot.trades.clone();
            let mut positions = slot.position.clone().into_iter().collect::<Vec<_>>();
            match row_kind {
                0 => orders[0].instrument.venue_symbol = None,
                1 => trades[0].instrument.venue_symbol = None,
                2 => positions[0].instrument.venue_symbol = None,
                _ => unreachable!(),
            }
            let bound = bound_owning_package(
                &restart,
                &format!("stage5g-edb-r2-semantic-instrument-{row_kind}"),
                &format!("stage5g-edb-r2-semantic-instrument-{row_kind}-epoch"),
                OwningTruthRows::complete(orders, trades, positions),
            );
            let evidence = reduce_stage5g_fresh_broker_truth(restart, bound).evidence();
            assert_eq!(
                evidence.reason,
                Stage5gFreshTruthReductionReason::TargetInstrumentIdentityConflict
            );
            assert!(!evidence.candidate_present);
        }

        let restart = crate::stage5g_order_position::tests::
            stage5g_edb_restored_generated_fractional_escrow_fixture();
        let projection = restart.fresh_truth_reducer_projection();
        assert!(!projection.slots[0].source_numeric_authority_is_integral);
        let slot = &projection.slots[0];
        let mut fractional_order =
            discovered_order_for_slot(slot, OrderStatus::Working, "FRACTIONAL-ORDER");
        fractional_order.qty = Decimal::new(1, 1);
        fractional_order.remaining_qty = Some(Decimal::new(1, 1));
        let bound = bound_owning_package(
            &restart,
            "stage5g-edb-r2-fractional-source",
            "stage5g-edb-r2-fractional-source-epoch",
            OwningTruthRows::complete(vec![fractional_order], vec![], vec![]),
        );
        let evidence = reduce_stage5g_fresh_broker_truth(restart, bound).evidence();
        assert_eq!(
            evidence.reason,
            Stage5gFreshTruthReductionReason::SourceNumericAuthorityUnsupported
        );
        assert!(!evidence.candidate_present);
    }

    fn cases() -> Vec<Case> {
        let working = order(OrderStatus::Working, "ORDER-1", Decimal::ZERO);
        let partial = order(OrderStatus::PartiallyFilled, "ORDER-1", Decimal::ONE);
        let filled = order(OrderStatus::Filled, "ORDER-1", Decimal::ONE);
        let canceled = order(OrderStatus::Canceled, "ORDER-1", Decimal::ZERO);

        let mut terminal_slot = slot(Some("ORDER-1"), true);
        terminal_slot.latest_order = Some(filled.clone());
        terminal_slot.trades = vec![trade(Decimal::ONE, "ORDER-1")];
        terminal_slot.position = Some(position(Decimal::ONE));

        vec![
            Case {
                restart: restart(
                    Stage5gCleanRestartLifecycleKind::OrderPositionAwaitingCommitted,
                    None,
                    false,
                ),
                truth: truth(
                    Stage5gFreshPackageLineage::NewFresh,
                    true,
                    true,
                    true,
                    vec![],
                    vec![],
                    vec![],
                ),
                expected_scenario: Stage5gRestartScenarioId::Grst01RestartBeforeAck,
                expected_disposition:
                    Stage5gRestartReconciliationDisposition::AwaitFreshBrokerTruth,
                expected_reason: Stage5gFreshTruthReductionReason::AuthoritativeOrderMissing,
            },
            Case {
                restart: restart(
                    Stage5gCleanRestartLifecycleKind::OrderPositionAwaitingCommitted,
                    Some(slot(None, false)),
                    false,
                ),
                truth: truth(
                    Stage5gFreshPackageLineage::NewFresh,
                    true,
                    true,
                    true,
                    vec![working.clone()],
                    vec![],
                    vec![],
                ),
                expected_scenario: Stage5gRestartScenarioId::Grst02RestartAfterAckBeforeOrder,
                expected_disposition: Stage5gRestartReconciliationDisposition::ApplyOwnedCandidate,
                expected_reason: Stage5gFreshTruthReductionReason::FreshWorkingOrderMatched,
            },
            Case {
                restart: restart(
                    Stage5gCleanRestartLifecycleKind::OrderPositionAwaitingCommitted,
                    Some(slot(Some("ORDER-1"), false)),
                    false,
                ),
                truth: truth(
                    Stage5gFreshPackageLineage::NewFresh,
                    true,
                    true,
                    true,
                    vec![working],
                    vec![],
                    vec![],
                ),
                expected_scenario: Stage5gRestartScenarioId::Grst03RestartWithWorkingOrder,
                expected_disposition: Stage5gRestartReconciliationDisposition::ApplyOwnedCandidate,
                expected_reason: Stage5gFreshTruthReductionReason::FreshWorkingOrderMatched,
            },
            Case {
                restart: {
                    let mut restart = restart(
                        Stage5gCleanRestartLifecycleKind::OrderPositionAwaitingCommitted,
                        Some(slot(Some("ORDER-1"), false)),
                        false,
                    );
                    restart.slots[0].target_qty = Some(Decimal::new(2, 0));
                    restart
                },
                truth: truth(
                    Stage5gFreshPackageLineage::NewFresh,
                    true,
                    true,
                    true,
                    vec![partial],
                    vec![trade(Decimal::ONE, "ORDER-1")],
                    vec![position(Decimal::ONE)],
                ),
                expected_scenario: Stage5gRestartScenarioId::Grst04RestartAfterPartialFill,
                expected_disposition: Stage5gRestartReconciliationDisposition::ApplyOwnedCandidate,
                expected_reason: Stage5gFreshTruthReductionReason::PartialFillPositionConverged,
            },
            Case {
                restart: restart(
                    Stage5gCleanRestartLifecycleKind::OrderPositionAwaitingCommitted,
                    Some(slot(Some("ORDER-1"), false)),
                    false,
                ),
                truth: truth(
                    Stage5gFreshPackageLineage::NewFresh,
                    true,
                    true,
                    false,
                    vec![filled.clone()],
                    vec![trade(Decimal::ONE, "ORDER-1")],
                    vec![],
                ),
                expected_scenario: Stage5gRestartScenarioId::Grst05RestartFilledBeforePosition,
                expected_disposition:
                    Stage5gRestartReconciliationDisposition::AwaitFreshBrokerTruth,
                expected_reason: Stage5gFreshTruthReductionReason::PositionsTruthIncomplete,
            },
            Case {
                restart: restart(
                    Stage5gCleanRestartLifecycleKind::OrderPositionAwaitingCommitted,
                    Some(terminal_slot),
                    false,
                ),
                truth: truth(
                    Stage5gFreshPackageLineage::NewFresh,
                    true,
                    true,
                    true,
                    vec![filled.clone()],
                    vec![trade(Decimal::ONE, "ORDER-1")],
                    vec![position(Decimal::ONE)],
                ),
                expected_scenario:
                    Stage5gRestartScenarioId::Grst06RestartAfterTerminalPositionApplied,
                expected_disposition:
                    Stage5gRestartReconciliationDisposition::ContinueFromCommittedCheckpoint,
                expected_reason: Stage5gFreshTruthReductionReason::TerminalPositionAlreadyApplied,
            },
            Case {
                restart: restart(Stage5gCleanRestartLifecycleKind::TimerReady, None, false),
                truth: truth(
                    Stage5gFreshPackageLineage::NewFresh,
                    true,
                    true,
                    true,
                    vec![],
                    vec![],
                    vec![],
                ),
                expected_scenario: Stage5gRestartScenarioId::Grst07RestartAtTimerCheckpoint,
                expected_disposition:
                    Stage5gRestartReconciliationDisposition::ContinueFromCommittedCheckpoint,
                expected_reason: Stage5gFreshTruthReductionReason::TimerCheckpointExact,
            },
            Case {
                restart: restart(
                    Stage5gCleanRestartLifecycleKind::OrderPositionAwaitingCommitted,
                    Some(slot(None, false)),
                    true,
                ),
                truth: truth(
                    Stage5gFreshPackageLineage::NewFresh,
                    true,
                    true,
                    true,
                    vec![],
                    vec![],
                    vec![],
                ),
                expected_scenario: Stage5gRestartScenarioId::Grst08RestartWithGeneratedIntentEscrow,
                expected_disposition:
                    Stage5gRestartReconciliationDisposition::ReconciliationRequired,
                expected_reason: Stage5gFreshTruthReductionReason::GeneratedIntentEscrowRetained,
            },
            Case {
                restart: restart(
                    Stage5gCleanRestartLifecycleKind::OrderPositionAwaitingCommitted,
                    Some(slot(Some("ORDER-1"), false)),
                    false,
                ),
                truth: truth(
                    Stage5gFreshPackageLineage::ReplayTupleNotInRestartLedger,
                    true,
                    true,
                    true,
                    vec![],
                    vec![],
                    vec![],
                ),
                expected_scenario: Stage5gRestartScenarioId::Grst09ExactReplayIsIdempotent,
                expected_disposition:
                    Stage5gRestartReconciliationDisposition::ReconciliationRequired,
                expected_reason: Stage5gFreshTruthReductionReason::ReplayTupleNotInRestartLedger,
            },
            Case {
                restart: restart(
                    Stage5gCleanRestartLifecycleKind::OrderPositionAwaitingCommitted,
                    Some(slot(Some("ORDER-EXPECTED"), false)),
                    false,
                ),
                truth: truth(
                    Stage5gFreshPackageLineage::NewFresh,
                    true,
                    true,
                    true,
                    vec![order(OrderStatus::Working, "ORDER-CONFLICT", Decimal::ZERO)],
                    vec![],
                    vec![],
                ),
                expected_scenario: Stage5gRestartScenarioId::Grst10ConflictingReplayBlocks,
                expected_disposition:
                    Stage5gRestartReconciliationDisposition::ManualInterventionRequired,
                expected_reason: Stage5gFreshTruthReductionReason::TargetOrderIdentityConflict,
            },
            Case {
                restart: restart(
                    Stage5gCleanRestartLifecycleKind::OrderPositionAwaitingCommitted,
                    Some(slot(Some("ORDER-1"), false)),
                    false,
                ),
                truth: truth(
                    Stage5gFreshPackageLineage::NewFresh,
                    true,
                    true,
                    true,
                    vec![canceled],
                    vec![],
                    vec![],
                ),
                expected_scenario:
                    Stage5gRestartScenarioId::Grst11FreshBrokerTruthOverridesStaleHint,
                expected_disposition: Stage5gRestartReconciliationDisposition::ApplyOwnedCandidate,
                expected_reason: Stage5gFreshTruthReductionReason::FreshTerminalOrderMatched,
            },
            Case {
                restart: restart(
                    Stage5gCleanRestartLifecycleKind::OrderPositionAwaitingCommitted,
                    None,
                    false,
                ),
                truth: truth(
                    Stage5gFreshPackageLineage::NewFresh,
                    false,
                    true,
                    true,
                    vec![],
                    vec![],
                    vec![],
                ),
                expected_scenario:
                    Stage5gRestartScenarioId::Grst12MissingOrAmbiguousTruthRequiresReconciliation,
                expected_disposition:
                    Stage5gRestartReconciliationDisposition::AwaitFreshBrokerTruth,
                expected_reason: Stage5gFreshTruthReductionReason::OrdersTruthIncomplete,
            },
        ]
    }

    fn assert_case(index: usize) {
        let case = cases().remove(index);
        let actual = classify_case(&case.restart, &case.truth);
        assert_eq!(actual.scenario_id, case.expected_scenario);
        assert_eq!(actual.disposition, case.expected_disposition);
        assert_eq!(actual.reason, case.expected_reason);
        assert_eq!(
            actual.candidate.is_some(),
            case.expected_disposition
                == Stage5gRestartReconciliationDisposition::ApplyOwnedCandidate
        );
    }

    #[test]
    fn stage5g_edb_grst01() {
        assert_case(0);
    }
    #[test]
    fn stage5g_edb_grst02() {
        assert_case(1);
    }
    #[test]
    fn stage5g_edb_grst03() {
        assert_case(2);
    }
    #[test]
    fn stage5g_edb_grst04() {
        assert_case(3);
    }
    #[test]
    fn stage5g_edb_grst05() {
        assert_case(4);
    }
    #[test]
    fn stage5g_edb_grst06() {
        assert_case(5);
    }
    #[test]
    fn stage5g_edb_grst07() {
        assert_case(6);
    }
    #[test]
    fn stage5g_edb_grst08() {
        assert_case(7);
    }
    #[test]
    fn stage5g_edb_grst09() {
        assert_case(8);
    }
    #[test]
    fn stage5g_edb_grst10() {
        assert_case(9);
    }
    #[test]
    fn stage5g_edb_grst11() {
        assert_case(10);
    }
    #[test]
    fn stage5g_edb_grst12() {
        assert_case(11);
    }

    #[test]
    fn stage5g_edb_matrix_executes_frozen_ids_once_in_order() {
        let _owning_entry: fn(
            Stage5gCleanRestartedCapability,
            Stage5gRestartBoundFreshBrokerTruthPackage,
        ) -> Stage5gFreshTruthReduction = reduce_stage5g_fresh_broker_truth;
        let actual = cases()
            .into_iter()
            .map(|case| classify_case(&case.restart, &case.truth).scenario_id)
            .collect::<Vec<_>>();
        assert_eq!(actual, Stage5gRestartScenarioId::ALL);
        assert_eq!(actual.iter().copied().collect::<HashSet<_>>().len(), 12);
    }

    #[test]
    fn stage5g_edb_sequential_and_row_order_are_deterministic() {
        for mut case in cases() {
            let first = classify_case(&case.restart, &case.truth);
            case.truth.orders.reverse();
            case.truth.trades.reverse();
            case.truth.positions.reverse();
            let second = classify_case(&case.restart, &case.truth);
            assert_eq!(first.scenario_id, second.scenario_id);
            assert_eq!(first.disposition, second.disposition);
            assert_eq!(first.reason, second.reason);
            assert_eq!(
                first.candidate.as_ref().map(candidate_fingerprint),
                second.candidate.as_ref().map(candidate_fingerprint)
            );
        }
    }

    #[test]
    fn stage5g_edb_exact_replay_is_semantic_noop() {
        let case = cases().remove(8);
        let before = semantic_sha256(&case.restart);
        let actual = classify_case(&case.restart, &case.truth);
        assert_eq!(
            actual.disposition,
            Stage5gRestartReconciliationDisposition::ReconciliationRequired
        );
        assert!(actual.candidate.is_none());
        assert_eq!(before, semantic_sha256(&case.restart));
    }

    #[test]
    fn stage5g_edb_cross_binding_matrix_fails_closed() {
        let mut variants = Vec::new();

        let mut wrong_account = cases().remove(0);
        wrong_account.truth.operational_identity.account_id = BrokerAccountId::new("ACC_TEST_0002");
        variants.push(wrong_account);

        let mut wrong_strategy = cases().remove(0);
        wrong_strategy
            .truth
            .operational_identity
            .strategy_definition_id =
            Stage5gStrategyDefinitionId::parse("other_strategy").expect("strategy");
        variants.push(wrong_strategy);

        let mut wrong_config = cases().remove(0);
        wrong_config
            .truth
            .operational_identity
            .config_fingerprint_sha256 = Stage5gSha256::parse("b".repeat(64)).expect("config hash");
        variants.push(wrong_config);

        let mut wrong_instrument = cases().remove(0);
        wrong_instrument
            .truth
            .operational_identity
            .target_instrument
            .symbol = "OTHER".to_owned();
        wrong_instrument
            .truth
            .operational_identity
            .target_instrument
            .venue_symbol = Some("OTHER@RTSX".to_owned());
        variants.push(wrong_instrument);

        let mut wrong_runtime = cases().remove(0);
        wrong_runtime
            .restart
            .reconstructed_runtime_state_fingerprint_sha256 = "b".repeat(64);
        variants.push(wrong_runtime);

        for case in variants {
            let actual = classify_case(&case.restart, &case.truth);
            assert_eq!(
                actual.scenario_id,
                Stage5gRestartScenarioId::Grst10ConflictingReplayBlocks
            );
            assert_eq!(
                actual.reason,
                Stage5gFreshTruthReductionReason::OperationalIdentityConflict
            );
            assert!(actual.candidate.is_none());
        }
    }

    #[test]
    fn stage5g_edb_r1_all_operational_identity_fields_are_commitment_bound() {
        fn assert_bound_mutation_is_rejected(
            mutate: impl FnOnce(&mut Stage5gOperationalIdentityV1),
        ) {
            let case = cases().remove(2);
            let mut bound = bound_truth(&case.restart, &case.truth);
            mutate(&mut bound.package.operational_identity);
            let actual = classify(&case.restart, &bound);
            assert_eq!(
                actual.scenario_id,
                Stage5gRestartScenarioId::Grst10ConflictingReplayBlocks
            );
            assert_eq!(
                actual.reason,
                Stage5gFreshTruthReductionReason::OperationalIdentityConflict
            );
            assert!(actual.candidate.is_none());
        }

        assert_bound_mutation_is_rejected(|identity| {
            identity.broker_id = Stage5gBrokerId::parse("other-broker").expect("broker")
        });
        assert_bound_mutation_is_rejected(|identity| {
            identity.account_id = BrokerAccountId::new("ACC_TEST_0002")
        });
        assert_bound_mutation_is_rejected(|identity| {
            identity.strategy_definition_id =
                Stage5gStrategyDefinitionId::parse("other-strategy").expect("strategy")
        });
        assert_bound_mutation_is_rejected(|identity| {
            identity.strategy_instance_id =
                Stage5gStrategyInstanceId::parse("other-instance").expect("instance")
        });
        assert_bound_mutation_is_rejected(|identity| {
            identity.deployment_id =
                Stage5gDeploymentId::parse("other-deployment").expect("deployment")
        });
        assert_bound_mutation_is_rejected(|identity| {
            identity.deployment_generation =
                Stage5gDeploymentGeneration::parse(12).expect("generation")
        });
        assert_bound_mutation_is_rejected(|identity| {
            identity.gateway_instance_id =
                Stage5gGatewayInstanceId::parse("other-gateway").expect("gateway")
        });
        assert_bound_mutation_is_rejected(|identity| {
            identity.config_fingerprint_sha256 =
                Stage5gSha256::parse("b".repeat(64)).expect("config")
        });
        assert_bound_mutation_is_rejected(|identity| {
            identity.instrument_map_fingerprint_sha256 =
                Stage5gSha256::parse("c".repeat(64)).expect("instrument map")
        });
        assert_bound_mutation_is_rejected(|identity| {
            identity.market_data_generation =
                Stage5gFeedGeneration::parse(8).expect("market generation")
        });
        assert_bound_mutation_is_rejected(|identity| {
            identity.command_consumer_generation =
                Stage5gFeedGeneration::parse(10).expect("consumer generation")
        });
        assert_bound_mutation_is_rejected(|identity| {
            identity.target_instrument.venue_symbol = None
        });
    }

    #[test]
    fn stage5g_edb_r1_restart_replay_commitment_and_conflict_are_enforced() {
        let case = cases().remove(8);
        let mut changed_fingerprint = bound_truth(&case.restart, &case.truth);
        changed_fingerprint.package.canonical_fingerprint_sha256 = "b".repeat(64);
        let actual = classify(&case.restart, &changed_fingerprint);
        assert_eq!(
            actual.reason,
            Stage5gFreshTruthReductionReason::ReplayTupleNotInRestartLedger
        );
        assert_eq!(
            actual.disposition,
            Stage5gRestartReconciliationDisposition::ReconciliationRequired
        );
        assert!(actual.candidate.is_none());

        let case = cases().remove(2);
        let mut changed_authority = bound_truth(&case.restart, &case.truth);
        changed_authority.restart_replay_commitment_sha256 = "c".repeat(64);
        let actual = classify(&case.restart, &changed_authority);
        assert_eq!(
            actual.reason,
            Stage5gFreshTruthReductionReason::OperationalIdentityConflict
        );
        assert!(actual.candidate.is_none());
    }

    #[test]
    fn stage5g_edb_request_client_broker_and_trade_identity_fail_closed() {
        let mut broker_conflict = cases().remove(2);
        broker_conflict.truth.orders[0].broker_order_id =
            Some(BrokerOrderId::new("ORDER-CONFLICT"));

        let mut client_conflict = cases().remove(2);
        client_conflict.truth.orders[0].client_order_id =
            Some(ClientOrderId::new("CLIENT-CONFLICT").expect("client id"));

        let mut trade_conflict = cases().remove(3);
        trade_conflict.truth.trades[0].broker_order_id = Some(BrokerOrderId::new("ORDER-CONFLICT"));
        trade_conflict.truth.trades[0].client_order_id =
            Some(ClientOrderId::new("CLIENT-CONFLICT").expect("client id"));

        for (case, reason) in [
            (
                broker_conflict,
                Stage5gFreshTruthReductionReason::TargetOrderIdentityConflict,
            ),
            (
                client_conflict,
                Stage5gFreshTruthReductionReason::TargetOrderIdentityConflict,
            ),
            (
                trade_conflict,
                Stage5gFreshTruthReductionReason::TradeIdentityConflict,
            ),
        ] {
            let actual = classify_case(&case.restart, &case.truth);
            assert_eq!(actual.reason, reason);
            assert!(actual.candidate.is_none());
        }
    }

    #[test]
    fn stage5g_edb_r1_exact_trade_linkage_rejects_secondary_id_conflicts() {
        let order = order(OrderStatus::PartiallyFilled, "B1", Decimal::ONE);

        let mut exact = trade(Decimal::ONE, "B1");
        exact.client_order_id = Some(ClientOrderId::new("CLIENT-1").expect("client"));
        assert_eq!(
            stage5g_exact_trade_order_linkage(&order, &exact),
            Stage5gTradeOrderLinkage::Exact
        );

        let mut broker_conflict = exact.clone();
        broker_conflict.broker_order_id = Some(BrokerOrderId::new("B2"));
        assert_eq!(
            stage5g_exact_trade_order_linkage(&order, &broker_conflict),
            Stage5gTradeOrderLinkage::Conflict
        );

        let mut client_conflict = exact.clone();
        client_conflict.client_order_id = Some(ClientOrderId::new("C2").expect("client conflict"));
        assert_eq!(
            stage5g_exact_trade_order_linkage(&order, &client_conflict),
            Stage5gTradeOrderLinkage::Conflict
        );

        let mut both_conflict = client_conflict;
        both_conflict.broker_order_id = Some(BrokerOrderId::new("B2"));
        assert_eq!(
            stage5g_exact_trade_order_linkage(&order, &both_conflict),
            Stage5gTradeOrderLinkage::Unrelated
        );

        let mut client_only_order = order.clone();
        client_only_order.broker_order_id = None;
        let mut client_only_mismatch = exact.clone();
        client_only_mismatch.broker_order_id = None;
        client_only_mismatch.client_order_id =
            Some(ClientOrderId::new("C2").expect("client conflict"));
        assert_eq!(
            stage5g_exact_trade_order_linkage(&client_only_order, &client_only_mismatch),
            Stage5gTradeOrderLinkage::Unrelated
        );

        let mut broker_only_order = order;
        broker_only_order.client_order_id = None;
        let mut broker_only_exact = exact.clone();
        broker_only_exact.client_order_id = None;
        assert_eq!(
            stage5g_exact_trade_order_linkage(&broker_only_order, &broker_only_exact),
            Stage5gTradeOrderLinkage::Exact
        );

        let mut no_id_order = broker_only_order;
        no_id_order.broker_order_id = None;
        let mut no_id_trade = exact;
        no_id_trade.client_order_id = None;
        no_id_trade.broker_order_id = None;
        assert_eq!(
            stage5g_exact_trade_order_linkage(&no_id_order, &no_id_trade),
            Stage5gTradeOrderLinkage::Unrelated
        );
    }

    #[test]
    fn stage5g_edb_r1_source_relative_entry_exit_and_terminal_matrix() {
        let mut exit_restart = restart(
            Stage5gCleanRestartLifecycleKind::OrderPositionAwaitingCommitted,
            Some(slot(Some("ORDER-1"), false)),
            false,
        );
        exit_restart.slots[0].intent_class = Stage5gRestartIntentClass::Exit;
        exit_restart.slots[0].pre_position_qty = Decimal::ONE;
        exit_restart.slots[0].side = Some(OrderSide::Sell);
        let mut exit_order = order(OrderStatus::Filled, "ORDER-1", Decimal::ONE);
        exit_order.side = OrderSide::Sell;
        let mut exit_trade = trade(Decimal::ONE, "ORDER-1");
        exit_trade.side = OrderSide::Sell;
        let exit_truth = truth(
            Stage5gFreshPackageLineage::NewFresh,
            true,
            true,
            true,
            vec![exit_order],
            vec![exit_trade],
            vec![],
        );
        let exit = classify_case(&exit_restart, &exit_truth);
        assert_eq!(
            exit.disposition,
            Stage5gRestartReconciliationDisposition::ApplyOwnedCandidate
        );
        assert!(exit.candidate.is_some());

        let mut working_with_fill = cases().remove(2);
        working_with_fill.truth.orders[0].filled_qty = Decimal::new(1, 1);
        working_with_fill.truth.orders[0].remaining_qty = Some(Decimal::new(9, 1));

        let mut rejected_with_fill = cases().remove(10);
        rejected_with_fill.truth.orders[0].status = OrderStatus::Rejected;
        rejected_with_fill.truth.orders[0].filled_qty = Decimal::ONE;
        rejected_with_fill.truth.orders[0].remaining_qty = Some(Decimal::ZERO);
        rejected_with_fill.truth.trades = vec![trade(Decimal::ONE, "ORDER-1")];

        let mut canceled_partial_without_position = cases().remove(10);
        canceled_partial_without_position.truth.orders[0].qty = Decimal::new(2, 0);
        canceled_partial_without_position.truth.orders[0].filled_qty = Decimal::ONE;
        canceled_partial_without_position.truth.orders[0].remaining_qty = Some(Decimal::ONE);
        canceled_partial_without_position.truth.trades = vec![trade(Decimal::ONE, "ORDER-1")];

        let mut expired_partial_without_position = cases().remove(10);
        expired_partial_without_position.truth.orders[0].status = OrderStatus::Expired;
        expired_partial_without_position.truth.orders[0].qty = Decimal::new(2, 0);
        expired_partial_without_position.truth.orders[0].filled_qty = Decimal::ONE;
        expired_partial_without_position.truth.orders[0].remaining_qty = Some(Decimal::ONE);
        expired_partial_without_position.truth.trades = vec![trade(Decimal::ONE, "ORDER-1")];

        let mut filled_complete_empty_nonflat = cases().remove(4);
        filled_complete_empty_nonflat.truth.positions_complete = true;

        let mut timer_incomplete = cases().remove(6);
        timer_incomplete.truth.positions_complete = false;

        for case in [
            working_with_fill,
            rejected_with_fill,
            canceled_partial_without_position,
            expired_partial_without_position,
            filled_complete_empty_nonflat,
            timer_incomplete,
        ] {
            let actual = classify_case(&case.restart, &case.truth);
            assert!(actual.candidate.is_none());
            assert!(matches!(
                actual.disposition,
                Stage5gRestartReconciliationDisposition::AwaitFreshBrokerTruth
                    | Stage5gRestartReconciliationDisposition::ReconciliationRequired
                    | Stage5gRestartReconciliationDisposition::TerminalInconsistency
            ));
        }
    }

    #[test]
    fn stage5g_edb_incomplete_sections_never_mean_broker_absence() {
        let mut orders_incomplete = cases().remove(0);
        orders_incomplete.truth.orders_complete = false;
        let mut trades_incomplete = cases().remove(2);
        trades_incomplete.truth.trades_complete = false;
        let positions_incomplete = cases().remove(4);

        for (case, reason) in [
            (
                orders_incomplete,
                Stage5gFreshTruthReductionReason::OrdersTruthIncomplete,
            ),
            (
                trades_incomplete,
                Stage5gFreshTruthReductionReason::TradesTruthIncomplete,
            ),
            (
                positions_incomplete,
                Stage5gFreshTruthReductionReason::PositionsTruthIncomplete,
            ),
        ] {
            let actual = classify_case(&case.restart, &case.truth);
            assert_eq!(
                actual.disposition,
                Stage5gRestartReconciliationDisposition::AwaitFreshBrokerTruth
            );
            assert_eq!(actual.reason, reason);
            assert!(actual.candidate.is_none());
        }
    }

    #[test]
    fn stage5g_edb_quantity_and_position_mismatch_cannot_form_candidate() {
        let mut target_qty_mismatch = cases().remove(2);
        target_qty_mismatch.restart.slots[0].target_qty = Some(Decimal::new(2, 0));

        let mut partial_position_mismatch = cases().remove(3);
        partial_position_mismatch.truth.positions[0].qty = Decimal::new(5, 1);

        for case in [target_qty_mismatch, partial_position_mismatch] {
            let actual = classify_case(&case.restart, &case.truth);
            assert!(actual.candidate.is_none());
            assert!(matches!(
                actual.disposition,
                Stage5gRestartReconciliationDisposition::ReconciliationRequired
                    | Stage5gRestartReconciliationDisposition::TerminalInconsistency
            ));
        }
    }

    #[test]
    fn stage5g_edb_terminal_order_statuses_remain_distinct() {
        let fingerprints = [
            OrderStatus::Canceled,
            OrderStatus::Rejected,
            OrderStatus::Expired,
        ]
        .into_iter()
        .map(|status| {
            let restart = restart(
                Stage5gCleanRestartLifecycleKind::OrderPositionAwaitingCommitted,
                Some(slot(Some("ORDER-1"), false)),
                false,
            );
            let truth = truth(
                Stage5gFreshPackageLineage::NewFresh,
                true,
                true,
                true,
                vec![order(status, "ORDER-1", Decimal::ZERO)],
                vec![],
                vec![],
            );
            let actual = classify_case(&restart, &truth);
            assert_eq!(
                actual.disposition,
                Stage5gRestartReconciliationDisposition::ApplyOwnedCandidate
            );
            candidate_fingerprint(actual.candidate.as_ref().expect("candidate"))
        })
        .collect::<HashSet<_>>();
        assert_eq!(fingerprints.len(), 3);
    }

    #[test]
    fn stage5g_edb_parallel_execution_has_no_shared_mutable_state() {
        let handles = (0..12)
            .map(|index| {
                thread::spawn(move || {
                    let case = cases().remove(index);
                    let actual = classify_case(&case.restart, &case.truth);
                    (
                        actual.scenario_id,
                        actual.disposition,
                        actual.reason,
                        actual.candidate.as_ref().map(candidate_fingerprint),
                    )
                })
            })
            .collect::<Vec<_>>();
        let parallel = handles
            .into_iter()
            .map(|handle| handle.join().expect("deterministic worker"))
            .collect::<Vec<_>>();
        let sequential = cases()
            .into_iter()
            .map(|case| {
                let actual = classify_case(&case.restart, &case.truth);
                (
                    actual.scenario_id,
                    actual.disposition,
                    actual.reason,
                    actual.candidate.as_ref().map(candidate_fingerprint),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(parallel, sequential);
    }

    #[test]
    fn stage5g_edb_r1_owning_timer_ready_runs_export_decode_restore_validate_bind_reduce() {
        let restart =
            crate::stage5g_order_position::tests::stage5g_edb_restored_timer_ready_fixture();
        let projection = restart.fresh_truth_reducer_projection();
        let positions = (projection.committed_position_qty != Decimal::ZERO)
            .then(|| position(projection.committed_position_qty))
            .into_iter()
            .collect();
        let validated = validated_owning_package(
            &restart,
            "stage5g-edb-r1-fresh-timer",
            "stage5g-edb-r1-epoch-timer",
            OwningTruthRows::complete(Vec::new(), Vec::new(), positions),
        );
        let bound = bind_stage5g_fresh_truth_to_clean_restart(&restart, validated)
            .expect("authenticated restart binds full operational and replay authority");
        let reduction = reduce_stage5g_fresh_broker_truth(restart, bound);
        let evidence = reduction.evidence();
        assert_eq!(
            evidence.scenario_id,
            Stage5gRestartScenarioId::Grst07RestartAtTimerCheckpoint.frozen_id()
        );
        assert_eq!(
            evidence.disposition,
            disposition_id(
                Stage5gRestartReconciliationDisposition::ContinueFromCommittedCheckpoint
            )
        );
        assert_eq!(
            evidence.callback_count_before,
            evidence.callback_count_after
        );
        assert!(!evidence.runtime_mutated);
        assert!(!evidence.callback_invoked);
        assert!(!evidence.transport_opened);
        assert_eq!(evidence.operational_binding_commitment_sha256.len(), 64);
        assert_eq!(evidence.restart_replay_commitment_sha256.len(), 64);
        assert_eq!(evidence.package_identity_commitment_sha256.len(), 64);
    }

    #[test]
    fn stage5g_edb_r1_owning_awaiting_runs_export_decode_restore_validate_bind_reduce() {
        let restart = crate::stage5g_order_position::tests::stage5g_edb_restored_awaiting_fixture();
        let projection = restart.fresh_truth_reducer_projection();
        let slot = projection.slots.first().expect("awaiting fixture slot");
        let validated = validated_owning_package(
            &restart,
            "stage5g-edb-r1-fresh-awaiting",
            "stage5g-edb-r1-epoch-awaiting",
            OwningTruthRows::complete(
                slot.latest_order.clone().into_iter().collect(),
                slot.trades.clone(),
                slot.position.clone().into_iter().collect(),
            ),
        );
        let bound = bind_stage5g_fresh_truth_to_clean_restart(&restart, validated)
            .expect("authenticated awaiting restart binds fresh package");
        let reduction = reduce_stage5g_fresh_broker_truth(restart, bound);
        let evidence = reduction.evidence();
        assert_eq!(
            evidence.scenario_id,
            Stage5gRestartScenarioId::Grst04RestartAfterPartialFill.frozen_id()
        );
        assert_ne!(
            evidence.reason,
            Stage5gFreshTruthReductionReason::OperationalIdentityConflict
        );
        assert_eq!(
            evidence.callback_count_before,
            evidence.callback_count_after
        );
        assert!(!evidence.runtime_mutated);
        assert!(!evidence.callback_invoked);
        assert!(!evidence.transport_opened);
    }

    #[test]
    fn stage5g_edb_r1_owning_status_paths_cover_working_filled_terminal_and_missing() {
        let restart = crate::stage5g_order_position::tests::stage5g_edb_restored_awaiting_fixture();
        let projection = restart.fresh_truth_reducer_projection();
        let slot = projection.slots.first().expect("awaiting slot");
        let mut working = slot.latest_order.clone().expect("source order");
        working.status = OrderStatus::Working;
        working.lifecycle = BrokerOrderSnapshot::lifecycle_for(&working.status);
        working.filled_qty = Decimal::ZERO;
        working.remaining_qty = Some(working.qty);
        let bound = bound_owning_package(
            &restart,
            "stage5g-edb-r1-owning-working",
            "stage5g-edb-r1-epoch-working",
            OwningTruthRows::complete(vec![working], vec![], vec![]),
        );
        let evidence = reduce_stage5g_fresh_broker_truth(restart, bound).evidence();
        assert_eq!(
            evidence.scenario_id,
            Stage5gRestartScenarioId::Grst10ConflictingReplayBlocks.frozen_id()
        );
        assert_eq!(
            evidence.reason,
            Stage5gFreshTruthReductionReason::FilledQuantityRegression
        );
        assert!(!evidence.candidate_present);

        let restart = crate::stage5g_order_position::tests::stage5g_edb_restored_awaiting_fixture();
        let projection = restart.fresh_truth_reducer_projection();
        let slot = projection.slots.first().expect("awaiting slot");
        let mut filled = slot.latest_order.clone().expect("source order");
        filled.status = OrderStatus::Filled;
        filled.lifecycle = BrokerOrderSnapshot::lifecycle_for(&filled.status);
        filled.filled_qty = filled.qty;
        filled.remaining_qty = Some(Decimal::ZERO);
        let fill_trade = slot.trades.first().cloned().expect("source trade");
        let mut additional_trade = fill_trade.clone();
        additional_trade.broker_trade_id = BrokerTradeId::new("TRADE-R2-ADDITIONAL");
        additional_trade.qty = filled.qty - fill_trade.qty;
        let bound = bound_owning_package(
            &restart,
            "stage5g-edb-r1-owning-filled-incomplete",
            "stage5g-edb-r1-epoch-filled-incomplete",
            OwningTruthRows {
                orders_complete: true,
                trades_complete: true,
                positions_complete: false,
                orders: vec![filled],
                trades: vec![fill_trade, additional_trade],
                positions: vec![],
            },
        );
        let evidence = reduce_stage5g_fresh_broker_truth(restart, bound).evidence();
        assert_eq!(
            evidence.scenario_id,
            Stage5gRestartScenarioId::Grst05RestartFilledBeforePosition.frozen_id()
        );
        assert!(!evidence.candidate_present);

        let restart = crate::stage5g_order_position::tests::stage5g_edb_restored_awaiting_fixture();
        let projection = restart.fresh_truth_reducer_projection();
        let slot = projection.slots.first().expect("awaiting slot");
        let mut canceled = slot.latest_order.clone().expect("source order");
        canceled.status = OrderStatus::Canceled;
        canceled.lifecycle = BrokerOrderSnapshot::lifecycle_for(&canceled.status);
        canceled.remaining_qty = Some(canceled.qty - canceled.filled_qty);
        let bound = bound_owning_package(
            &restart,
            "stage5g-edb-r1-owning-canceled",
            "stage5g-edb-r1-epoch-canceled",
            OwningTruthRows::complete(
                vec![canceled],
                slot.trades.clone(),
                slot.position.clone().into_iter().collect(),
            ),
        );
        let evidence = reduce_stage5g_fresh_broker_truth(restart, bound).evidence();
        assert_eq!(
            evidence.scenario_id,
            Stage5gRestartScenarioId::Grst11FreshBrokerTruthOverridesStaleHint.frozen_id()
        );
        assert!(evidence.candidate_present);

        let restart = crate::stage5g_order_position::tests::stage5g_edb_restored_awaiting_fixture();
        let bound = bound_owning_package(
            &restart,
            "stage5g-edb-r1-owning-missing",
            "stage5g-edb-r1-epoch-missing",
            OwningTruthRows::complete(vec![], vec![], vec![]),
        );
        let evidence = reduce_stage5g_fresh_broker_truth(restart, bound).evidence();
        assert_eq!(
            evidence.scenario_id,
            Stage5gRestartScenarioId::Grst12MissingOrAmbiguousTruthRequiresReconciliation
                .frozen_id()
        );
        assert!(!evidence.candidate_present);
    }

    #[test]
    fn stage5g_edb_r1_owning_remaining_grst_paths_are_fail_closed_or_noop() {
        let restart =
            crate::stage5g_order_position::tests::stage5g_edb_restored_before_ack_fixture();
        let bound = bound_owning_package(
            &restart,
            "stage5g-edb-r1-owning-before-ack",
            "stage5g-edb-r1-epoch-before-ack",
            OwningTruthRows::complete(vec![], vec![], vec![]),
        );
        let evidence = reduce_stage5g_fresh_broker_truth(restart, bound).evidence();
        assert_eq!(
            evidence.scenario_id,
            Stage5gRestartScenarioId::Grst01RestartBeforeAck.frozen_id()
        );

        let restart =
            crate::stage5g_order_position::tests::stage5g_edb_restored_generated_escrow_fixture();
        let projection = restart.fresh_truth_reducer_projection();
        let slot = projection.slots.first().expect("generated slot");
        let mut discovered = order(OrderStatus::Working, "DISCOVERED-ORDER", Decimal::ZERO);
        discovered.client_order_id = slot.target_order_client_order_id.clone();
        discovered.side = slot.side.expect("generated side");
        discovered.qty = slot.target_qty.expect("generated quantity");
        discovered.remaining_qty = Some(discovered.qty);
        match &slot.source_action {
            crate::Stage5gMockIntentAction::Place {
                place_kind: crate::Stage5gMockPlaceKind::Market,
            } => {
                discovered.order_type = broker_core::OrderType::Market;
                discovered.limit_price = None;
            }
            crate::Stage5gMockIntentAction::Place {
                place_kind: crate::Stage5gMockPlaceKind::Limit,
            } => {
                discovered.order_type = broker_core::OrderType::Limit;
                discovered.limit_price = Some(Decimal::new(2200, 0));
            }
            crate::Stage5gMockIntentAction::Cancel { target_order_id } => {
                discovered.broker_order_id = Some(target_order_id.clone());
            }
        }
        let bound = bound_owning_package(
            &restart,
            "stage5g-edb-r1-owning-after-ack",
            "stage5g-edb-r1-epoch-after-ack",
            OwningTruthRows::complete(vec![discovered], vec![], vec![]),
        );
        let evidence = reduce_stage5g_fresh_broker_truth(restart, bound).evidence();
        assert_eq!(
            evidence.scenario_id,
            Stage5gRestartScenarioId::Grst02RestartAfterAckBeforeOrder.frozen_id()
        );
        assert!(evidence.candidate_present);

        let restart =
            crate::stage5g_order_position::tests::stage5g_edb_restored_terminal_applied_fixture();
        let projection = restart.fresh_truth_reducer_projection();
        let slot = projection.slots.first().expect("terminal slot");
        let bound = bound_owning_package(
            &restart,
            "stage5g-edb-r1-owning-terminal-applied",
            "stage5g-edb-r1-epoch-terminal-applied",
            OwningTruthRows::complete(
                slot.latest_order.clone().into_iter().collect(),
                slot.trades.clone(),
                slot.position.clone().into_iter().collect(),
            ),
        );
        let evidence = reduce_stage5g_fresh_broker_truth(restart, bound).evidence();
        assert_eq!(
            evidence.scenario_id,
            Stage5gRestartScenarioId::Grst06RestartAfterTerminalPositionApplied.frozen_id()
        );
        assert!(!evidence.candidate_present);

        let restart = crate::stage5g_order_position::tests::stage5g_edb_restored_awaiting_fixture();
        let projection = restart.fresh_truth_reducer_projection();
        let slot = projection.slots.first().expect("awaiting slot");
        let mut bound = bound_owning_package(
            &restart,
            "stage5g-edb-r1-owning-conflict",
            "stage5g-edb-r1-epoch-conflict",
            OwningTruthRows::complete(
                slot.latest_order.clone().into_iter().collect(),
                slot.trades.clone(),
                slot.position.clone().into_iter().collect(),
            ),
        );
        bound.operational_binding_commitment_sha256 = "f".repeat(64);
        let evidence = reduce_stage5g_fresh_broker_truth(restart, bound).evidence();
        assert_eq!(
            evidence.scenario_id,
            Stage5gRestartScenarioId::Grst10ConflictingReplayBlocks.frozen_id()
        );
        assert!(!evidence.candidate_present);
    }

    #[test]
    fn stage5g_edb_r1_owning_generated_intent_escrow_is_retained() {
        let restart =
            crate::stage5g_order_position::tests::stage5g_edb_restored_generated_escrow_fixture();
        assert!(restart
            .fresh_truth_reducer_projection()
            .generated_intent_escrow_fingerprint_sha256
            .is_some());
        let bound = bound_owning_package(
            &restart,
            "stage5g-edb-r1-owning-escrow",
            "stage5g-edb-r1-epoch-escrow",
            OwningTruthRows::complete(vec![], vec![], vec![]),
        );
        let evidence = reduce_stage5g_fresh_broker_truth(restart, bound).evidence();
        assert_eq!(
            evidence.scenario_id,
            Stage5gRestartScenarioId::Grst08RestartWithGeneratedIntentEscrow.frozen_id()
        );
        assert_eq!(
            evidence.reason,
            Stage5gFreshTruthReductionReason::GeneratedIntentEscrowRetained
        );
        assert!(!evidence.candidate_present);
    }

    #[test]
    fn stage5g_edb_r1_owning_exact_current_and_historical_replay_are_noops() {
        fn exact_replay_evidence(historical: bool) -> Stage5gFreshTruthReductionEvidence {
            let restart =
                crate::stage5g_order_position::tests::stage5g_edb_restored_awaiting_fixture();
            let projection = restart.fresh_truth_reducer_projection();
            let slot = projection.slots.first().expect("awaiting slot");
            let validated = validated_owning_package(
                &restart,
                if historical {
                    "stage5g-edb-r1-owning-historical-replay"
                } else {
                    "stage5g-edb-r1-owning-current-replay"
                },
                if historical {
                    "stage5g-edb-r1-epoch-historical-replay"
                } else {
                    "stage5g-edb-r1-epoch-current-replay"
                },
                OwningTruthRows::complete(
                    slot.latest_order.clone().into_iter().collect(),
                    slot.trades.clone(),
                    slot.position.clone().into_iter().collect(),
                ),
            );
            let replay_identity = Stage5gReconciledFreshPackageIdentity::validate(
                validated.package_id.as_str(),
                validated.snapshot_epoch.as_str(),
                validated.canonical_fingerprint_sha256.clone(),
            )
            .expect("validated replay tuple");
            let raw = raw_package_from_validated(&validated);
            let current_id = projection
                .checkpoint
                .payload
                .current_evidence_identity
                .as_deref()
                .expect("checkpoint identity");
            let current_epoch = projection
                .checkpoint
                .payload
                .package_discriminator
                .as_deref()
                .expect("checkpoint epoch");
            let accepted = historical
                .then(|| replay_identity.clone())
                .into_iter()
                .collect::<Vec<_>>();
            let reviewed_authority =
                super::super::stage5g_test_reviewed_operational_identity_authority(
                    raw.operational_identity.clone(),
                )
                .expect("test deployment identity passes reviewed authority issuer");
            let operational_authority =
                super::super::authorize_stage5g_fresh_truth_operational_identity(
                    &restart,
                    reviewed_authority,
                )
                .expect("replay package uses the reviewed restart identity");
            let package = validate_stage5g_fresh_broker_truth_package(
                raw,
                Stage5gFreshBrokerTruthValidationContext {
                    operational_authority,
                    pre_restart_package_id: current_id,
                    pre_restart_snapshot_epoch: current_epoch,
                    untrusted_last_reconciled_hint: (!historical).then_some(&replay_identity),
                    untrusted_accepted_replay_hints: &accepted,
                    untrusted_known_historical_hints: &[],
                    clean_restore_completed_at: Utc.with_ymd_and_hms(2020, 1, 1, 0, 0, 0).unwrap(),
                    validation_observed_at: Utc.with_ymd_and_hms(2030, 1, 1, 0, 0, 10).unwrap(),
                },
            )
            .expect("exact package revalidates through the owning input boundary");
            assert_eq!(
                package.lineage,
                if historical {
                    Stage5gFreshPackageLineage::HistoricalReplayNotAccepted
                } else {
                    Stage5gFreshPackageLineage::ReplayTupleNotInRestartLedger
                }
            );
            let bound = bind_stage5g_fresh_truth_to_clean_restart(&restart, package)
                .expect("exact replay binds through the production restart authority");
            reduce_stage5g_fresh_broker_truth(restart, bound).evidence()
        }

        for historical in [false, true] {
            let evidence = exact_replay_evidence(historical);
            assert_eq!(
                evidence.scenario_id,
                Stage5gRestartScenarioId::Grst09ExactReplayIsIdempotent.frozen_id()
            );
            assert_eq!(
                evidence.disposition,
                disposition_id(if historical {
                    Stage5gRestartReconciliationDisposition::ManualInterventionRequired
                } else {
                    Stage5gRestartReconciliationDisposition::ReconciliationRequired
                })
            );
            assert_eq!(
                evidence.pre_semantic_fingerprint_sha256,
                evidence.post_candidate_fingerprint_sha256
            );
            assert!(!evidence.candidate_present);
            assert!(!evidence.runtime_mutated);
            assert!(!evidence.callback_invoked);
            assert!(!evidence.transport_opened);
        }
    }

    #[test]
    fn stage5g_edb_r4_owning_grst01_and_grst07_ignore_complete_harmless_history() {
        for timer_ready in [true, false] {
            let restart = if timer_ready {
                crate::stage5g_order_position::tests::stage5g_edb_restored_timer_ready_fixture()
            } else {
                crate::stage5g_order_position::tests::stage5g_edb_restored_before_ack_fixture()
            };
            let projection = restart.fresh_truth_reducer_projection();
            let mut old_filled = order(OrderStatus::Filled, "OLD-FILLED-R4", Decimal::ONE);
            old_filled.client_order_id =
                Some(ClientOrderId::new("OLD-C-FILLED-R4").expect("old client"));
            let mut old_canceled = order(OrderStatus::Canceled, "OLD-CANCELED-R4", Decimal::ZERO);
            old_canceled.client_order_id =
                Some(ClientOrderId::new("OLD-C-CANCELED-R4").expect("old client"));
            let mut old_trade = trade(Decimal::ONE, "OLD-FILLED-R4");
            old_trade.broker_trade_id = BrokerTradeId::new("OLD-TRADE-R4");
            old_trade.client_order_id = old_filled.client_order_id.clone();
            let positions = (projection.committed_position_qty != Decimal::ZERO)
                .then(|| position(projection.committed_position_qty))
                .into_iter()
                .collect();
            let bound = bound_owning_package(
                &restart,
                if timer_ready {
                    "stage5g-edb-r4-timer-history"
                } else {
                    "stage5g-edb-r4-before-ack-history"
                },
                if timer_ready {
                    "stage5g-edb-r4-timer-history-epoch"
                } else {
                    "stage5g-edb-r4-before-ack-history-epoch"
                },
                OwningTruthRows::complete(
                    vec![old_filled, old_canceled],
                    vec![old_trade],
                    positions,
                ),
            );
            let evidence = reduce_stage5g_fresh_broker_truth(restart, bound).evidence();
            assert_eq!(
                evidence.scenario_id,
                if timer_ready {
                    Stage5gRestartScenarioId::Grst07RestartAtTimerCheckpoint.frozen_id()
                } else {
                    Stage5gRestartScenarioId::Grst01RestartBeforeAck.frozen_id()
                }
            );
            assert_eq!(evidence.ignored_unrelated_terminal_order_count, 2);
            assert_eq!(evidence.ignored_unrelated_historical_trade_count, 1);
            assert!(!evidence.candidate_present);
        }
    }

    #[test]
    fn stage5g_edb_r4_no_slot_active_and_unknown_orders_still_block() {
        for unknown in [false, true] {
            let restart =
                crate::stage5g_order_position::tests::stage5g_edb_restored_timer_ready_fixture();
            let mut blocking = order(OrderStatus::Working, "BLOCKING-R4", Decimal::ZERO);
            if unknown {
                blocking.status = OrderStatus::Unknown("broker-pending-r4".to_owned());
                blocking.lifecycle = BrokerOrderLifecycle::Unknown;
            }
            let bound = bound_owning_package(
                &restart,
                if unknown {
                    "stage5g-edb-r4-unknown-history"
                } else {
                    "stage5g-edb-r4-active-history"
                },
                if unknown {
                    "stage5g-edb-r4-unknown-history-epoch"
                } else {
                    "stage5g-edb-r4-active-history-epoch"
                },
                OwningTruthRows::complete(vec![blocking], vec![], vec![]),
            );
            let evidence = reduce_stage5g_fresh_broker_truth(restart, bound).evidence();
            assert_eq!(
                evidence.reason,
                if unknown {
                    Stage5gFreshTruthReductionReason::AccountWideUnknownOrderConflict
                } else {
                    Stage5gFreshTruthReductionReason::AccountWideActiveOrderConflict
                }
            );
            assert!(!evidence.candidate_present);
        }
    }

    #[test]
    fn stage5g_edb_r4_owning_grst06_canonicalizes_both_flat_representations() {
        for (committed_row_present, fresh_row_present) in [(true, false), (false, true)] {
            let restart =
                crate::stage5g_order_position::tests::stage5g_edb_restored_terminal_flat_fixture(
                    committed_row_present,
                );
            let projection = restart.fresh_truth_reducer_projection();
            let slot = projection.slots.first().expect("terminal flat slot");
            let positions = fresh_row_present
                .then(|| {
                    let mut flat = position(Decimal::ZERO);
                    flat.avg_price = if committed_row_present {
                        None
                    } else {
                        Some(Decimal::new(9_999, 0))
                    };
                    flat.unrealized_pnl = Some(Decimal::new(1_234, 2));
                    flat
                })
                .into_iter()
                .collect();
            let mut orders: Vec<_> = slot.latest_order.clone().into_iter().collect();
            let mut old_canceled = order(OrderStatus::Canceled, "OLD-GRST06-R4", Decimal::ZERO);
            old_canceled.client_order_id =
                Some(ClientOrderId::new("OLD-C-GRST06-R4").expect("old client"));
            orders.push(old_canceled);
            let mut trades = slot.trades.clone();
            let mut old_trade = trade(Decimal::ONE, "OLD-GRST06-R4");
            old_trade.broker_trade_id = BrokerTradeId::new("OLD-TRADE-GRST06-R4");
            old_trade.client_order_id =
                Some(ClientOrderId::new("OLD-C-GRST06-R4").expect("old client"));
            trades.push(old_trade);
            let bound = bound_owning_package(
                &restart,
                if committed_row_present {
                    "stage5g-edb-r4-flat-explicit-to-absent"
                } else {
                    "stage5g-edb-r4-flat-absent-to-explicit"
                },
                if committed_row_present {
                    "stage5g-edb-r4-flat-explicit-to-absent-epoch"
                } else {
                    "stage5g-edb-r4-flat-absent-to-explicit-epoch"
                },
                OwningTruthRows::complete(orders, trades, positions),
            );
            let evidence = reduce_stage5g_fresh_broker_truth(restart, bound).evidence();
            assert_eq!(
                evidence.scenario_id,
                Stage5gRestartScenarioId::Grst06RestartAfterTerminalPositionApplied.frozen_id()
            );
            assert_eq!(
                evidence.disposition,
                disposition_id(
                    Stage5gRestartReconciliationDisposition::ContinueFromCommittedCheckpoint
                )
            );
            assert_eq!(evidence.ignored_unrelated_terminal_order_count, 1);
            assert_eq!(evidence.ignored_unrelated_historical_trade_count, 1);
            assert!(!evidence.candidate_present);
        }
    }

    #[test]
    fn stage5g_edb_r4_flat_absence_never_overrides_incomplete_or_nonflat_truth() {
        let restart =
            crate::stage5g_order_position::tests::stage5g_edb_restored_terminal_flat_fixture(true);
        let projection = restart.fresh_truth_reducer_projection();
        let slot = projection.slots.first().expect("terminal flat slot");
        let bound = bound_owning_package(
            &restart,
            "stage5g-edb-r4-flat-incomplete",
            "stage5g-edb-r4-flat-incomplete-epoch",
            OwningTruthRows {
                orders_complete: true,
                trades_complete: true,
                positions_complete: false,
                orders: slot.latest_order.clone().into_iter().collect(),
                trades: slot.trades.clone(),
                positions: vec![],
            },
        );
        let evidence = reduce_stage5g_fresh_broker_truth(restart, bound).evidence();
        assert_eq!(
            evidence.reason,
            Stage5gFreshTruthReductionReason::PositionsTruthIncomplete
        );

        let restart =
            crate::stage5g_order_position::tests::stage5g_edb_restored_terminal_flat_fixture(false);
        let projection = restart.fresh_truth_reducer_projection();
        let slot = projection.slots.first().expect("terminal flat slot");
        let bound = bound_owning_package(
            &restart,
            "stage5g-edb-r4-flat-nonzero-conflict",
            "stage5g-edb-r4-flat-nonzero-conflict-epoch",
            OwningTruthRows::complete(
                slot.latest_order.clone().into_iter().collect(),
                slot.trades.clone(),
                vec![position(Decimal::ONE)],
            ),
        );
        let evidence = reduce_stage5g_fresh_broker_truth(restart, bound).evidence();
        assert_eq!(
            evidence.reason,
            Stage5gFreshTruthReductionReason::TerminalContradiction
        );
    }

    #[test]
    fn stage5g_edb_r4_cancel_target_authority_is_action_scoped_and_production_derived() {
        let restart = crate::stage5g_order_position::tests::
            stage5g_edb_restored_generated_cancel_escrow_fixture();
        let projection = restart.fresh_truth_reducer_projection();
        let slot = projection.slots.first().expect("cancel slot");
        assert_eq!(slot.command_client_order_id.as_str(), "C-CANCEL");
        assert!(slot.target_order_client_order_id.is_none());
        let authority = slot
            .cancel_target_order_authority
            .as_ref()
            .expect("broker target authority always exists");
        assert_eq!(
            authority.target_broker_order_id.as_str(),
            "CANCEL-TARGET-R2"
        );
        assert!(authority.target_order_client_order_id.is_none());
        assert!(authority.immutable_order_commitment_sha256.is_none());
        let mut target = discovered_order_for_slot(slot, OrderStatus::Canceled, "ignored");
        target.client_order_id = Some(ClientOrderId::new("C-PLACE").expect("target client"));
        let bound = bound_owning_package(
            &restart,
            "stage5g-edb-r4-cancel-unknown-client",
            "stage5g-edb-r4-cancel-unknown-client-epoch",
            OwningTruthRows::complete(vec![target], vec![], vec![]),
        );
        let reduction = reduce_stage5g_fresh_broker_truth(restart, bound);
        let candidate = reduction
            .candidate
            .as_ref()
            .expect("broker-only target is accepted");
        assert_eq!(candidate.command_client_order_id.as_str(), "C-CANCEL");
        assert!(candidate.target_order_client_order_id.is_none());

        let restart = crate::stage5g_order_position::tests::
            stage5g_edb_restored_cancel_target_authority_fixture();
        let projection = restart.fresh_truth_reducer_projection();
        let slot = projection
            .slots
            .first()
            .expect("authenticated cancel target slot");
        assert_eq!(slot.command_client_order_id.as_str(), "C-CANCEL");
        assert_eq!(
            slot.target_order_client_order_id
                .as_ref()
                .expect("target client authority")
                .as_str(),
            "C-PLACE"
        );
        assert_ne!(
            slot.command_client_order_id,
            slot.target_order_client_order_id.clone().unwrap()
        );
        assert!(slot
            .cancel_target_order_authority
            .as_ref()
            .and_then(|authority| authority.immutable_order_commitment_sha256.as_ref())
            .is_some());
    }

    #[test]
    fn stage5g_edb_r4_cancel_target_identity_conflicts_only_against_authenticated_authority() {
        for wrong_client in [false, true] {
            let restart = crate::stage5g_order_position::tests::
                stage5g_edb_restored_cancel_target_authority_fixture();
            let projection = restart.fresh_truth_reducer_projection();
            let slot = projection
                .slots
                .first()
                .expect("authenticated cancel target slot");
            let mut target = slot.latest_order.clone().expect("committed target order");
            target.status = OrderStatus::Canceled;
            target.lifecycle = BrokerOrderSnapshot::lifecycle_for(&target.status);
            if wrong_client {
                target.client_order_id =
                    Some(ClientOrderId::new("C-WRONG").expect("wrong target client"));
            } else {
                target.broker_order_id = Some(BrokerOrderId::new("B-WRONG"));
            }
            let bound = bound_owning_package(
                &restart,
                if wrong_client {
                    "stage5g-edb-r4-cancel-wrong-client"
                } else {
                    "stage5g-edb-r4-cancel-wrong-broker"
                },
                if wrong_client {
                    "stage5g-edb-r4-cancel-wrong-client-epoch"
                } else {
                    "stage5g-edb-r4-cancel-wrong-broker-epoch"
                },
                OwningTruthRows::complete(vec![target], vec![], vec![]),
            );
            let evidence = reduce_stage5g_fresh_broker_truth(restart, bound).evidence();
            assert_eq!(
                evidence.reason,
                Stage5gFreshTruthReductionReason::TargetOrderIdentityConflict
            );
            assert!(!evidence.candidate_present);
        }
    }

    #[test]
    fn stage5g_edb_r4_immutable_target_order_payload_cannot_drift() {
        for drift in 0..5 {
            let restart = crate::stage5g_order_position::tests::
                stage5g_edb_restored_cancel_target_authority_fixture();
            let projection = restart.fresh_truth_reducer_projection();
            let slot = projection
                .slots
                .first()
                .expect("authenticated cancel target slot");
            let mut target = slot.latest_order.clone().expect("committed target order");
            target.status = OrderStatus::Canceled;
            target.lifecycle = BrokerOrderSnapshot::lifecycle_for(&target.status);
            match drift {
                0 => target.side = OrderSide::Sell,
                1 => {
                    target.qty += Decimal::ONE;
                    target.remaining_qty = Some(target.qty - target.filled_qty);
                }
                2 => {
                    target.order_type = OrderType::Market;
                    target.limit_price = None;
                }
                3 => target.time_in_force = Some(TimeInForce::ImmediateOrCancel),
                4 => target.limit_price = Some(Decimal::new(2_209, 0)),
                _ => unreachable!(),
            }
            let bound = bound_owning_package(
                &restart,
                &format!("stage5g-edb-r4-immutable-drift-{drift}"),
                &format!("stage5g-edb-r4-immutable-drift-{drift}-epoch"),
                OwningTruthRows::complete(vec![target], vec![], vec![]),
            );
            let evidence = reduce_stage5g_fresh_broker_truth(restart, bound).evidence();
            assert_eq!(
                evidence.reason,
                Stage5gFreshTruthReductionReason::TerminalContradiction
            );
            assert!(!evidence.candidate_present);
        }

        let restart = crate::stage5g_order_position::tests::
            stage5g_edb_restored_cancel_target_authority_fixture();
        let projection = restart.fresh_truth_reducer_projection();
        let slot = projection
            .slots
            .first()
            .expect("authenticated cancel target slot");
        let mut target = slot.latest_order.clone().expect("committed target order");
        target.status = OrderStatus::Canceled;
        target.lifecycle = BrokerOrderSnapshot::lifecycle_for(&target.status);
        target.received_ts += chrono::Duration::seconds(1);
        let bound = bound_owning_package(
            &restart,
            "stage5g-edb-r4-immutable-terminal-advance",
            "stage5g-edb-r4-immutable-terminal-advance-epoch",
            OwningTruthRows::complete(vec![target], vec![], vec![]),
        );
        assert!(
            reduce_stage5g_fresh_broker_truth(restart, bound)
                .evidence()
                .candidate_present
        );
    }

    #[test]
    fn stage5g_edb_r4_place_market_tif_is_immutable() {
        let mut committed = order(OrderStatus::Working, "MARKET-TIF-R4", Decimal::ZERO);
        committed.time_in_force = None;
        let mut fresh = committed.clone();
        fresh.time_in_force = Some(TimeInForce::ImmediateOrCancel);
        assert!(!stage5g_immutable_order_payload_matches(&committed, &fresh));
    }

    fn stage5g_edb_r5_terminal_restart(
        status: OrderStatus,
        partial: bool,
    ) -> Stage5gCleanRestartedCapability {
        match status {
            OrderStatus::Filled => {
                crate::stage5g_order_position::tests::stage5g_edb_restored_terminal_applied_fixture(
                )
            }
            OrderStatus::Rejected => {
                crate::stage5g_order_position::tests::stage5g_edb_restored_terminal_rejected_fixture(
                )
            }
            OrderStatus::Canceled => {
                crate::stage5g_order_position::tests::stage5g_edb_restored_terminal_canceled_fixture(
                    partial,
                )
            }
            OrderStatus::Expired => {
                crate::stage5g_order_position::tests::stage5g_edb_restored_terminal_expired_fixture(
                    partial,
                )
            }
            _ => panic!("R5 terminal fixture requires a reviewed terminal status"),
        }
    }

    fn stage5g_edb_r5_terminal_advance_rows(
        status: OrderStatus,
        two_added_trades: bool,
    ) -> (Stage5gCleanRestartedCapability, OwningTruthRows) {
        let restart = stage5g_edb_r5_terminal_restart(status, true);
        let projection = restart.fresh_truth_reducer_projection();
        let slot = projection.slots.first().expect("R5 terminal slot");
        let mut fresh_order = slot.latest_order.clone().expect("committed terminal order");
        fresh_order.filled_qty = Decimal::new(8, 1);
        fresh_order.remaining_qty = Some(fresh_order.qty - fresh_order.filled_qty);
        fresh_order.received_ts += chrono::Duration::seconds(1);

        let mut trades = slot.trades.clone();
        let added_quantities = if two_added_trades {
            vec![Decimal::new(2, 1), Decimal::new(2, 1)]
        } else {
            vec![Decimal::new(4, 1)]
        };
        for (index, qty) in added_quantities.into_iter().enumerate() {
            let mut added = slot
                .trades
                .first()
                .cloned()
                .expect("committed partial trade");
            added.broker_trade_id = BrokerTradeId::new(format!("R5-LATE-{}", index + 1));
            added.qty = qty;
            added.source_ts += chrono::Duration::milliseconds(i64::try_from(index + 1).unwrap());
            added.received_ts += chrono::Duration::seconds(1)
                + chrono::Duration::milliseconds(i64::try_from(index + 1).unwrap());
            trades.push(added);
        }
        let mut fresh_position = slot.position.clone().expect("committed partial position");
        fresh_position.qty = fresh_order.filled_qty;
        fresh_position.received_ts += chrono::Duration::seconds(1);
        (
            restart,
            OwningTruthRows::complete(vec![fresh_order], trades, vec![fresh_position]),
        )
    }

    #[test]
    fn stage5g_edb_r5_all_exact_terminal_statuses_are_generic_grst06() {
        for (index, status, partial) in [
            (0, OrderStatus::Filled, false),
            (1, OrderStatus::Rejected, false),
            (2, OrderStatus::Canceled, false),
            (3, OrderStatus::Canceled, true),
            (4, OrderStatus::Expired, false),
            (5, OrderStatus::Expired, true),
        ] {
            let restart = stage5g_edb_r5_terminal_restart(status, partial);
            let projection = restart.fresh_truth_reducer_projection();
            let slot = projection.slots.first().expect("exact terminal slot");
            let mut exact_order = slot.latest_order.clone().expect("exact terminal order");
            exact_order.received_ts += chrono::Duration::seconds(1);
            let exact_trades = slot
                .trades
                .iter()
                .cloned()
                .map(|mut trade| {
                    trade.received_ts += chrono::Duration::seconds(1);
                    trade
                })
                .collect::<Vec<_>>();
            let exact_position = slot.position.clone().map(|mut position| {
                position.received_ts += chrono::Duration::seconds(1);
                position.unrealized_pnl = Some(Decimal::new(9_999, 2));
                position
            });
            let bound = bound_owning_package(
                &restart,
                &format!("stage5g-edb-r5-exact-terminal-{index}"),
                &format!("stage5g-edb-r5-exact-terminal-{index}-epoch"),
                OwningTruthRows::complete(
                    vec![exact_order],
                    exact_trades,
                    exact_position.into_iter().collect(),
                ),
            );
            let evidence = reduce_stage5g_fresh_broker_truth(restart, bound).evidence();
            assert_eq!(
                evidence.scenario_id,
                Stage5gRestartScenarioId::Grst06RestartAfterTerminalPositionApplied.frozen_id()
            );
            assert_eq!(
                evidence.disposition,
                disposition_id(
                    Stage5gRestartReconciliationDisposition::ContinueFromCommittedCheckpoint
                )
            );
            assert_eq!(
                evidence.reason,
                Stage5gFreshTruthReductionReason::TerminalPositionAlreadyApplied
            );
            assert!(!evidence.candidate_present);
            assert_eq!(
                evidence.pre_semantic_fingerprint_sha256,
                evidence.post_candidate_fingerprint_sha256
            );
            assert_eq!(
                evidence.callback_count_before,
                evidence.callback_count_after
            );
            assert!(!evidence.runtime_mutated);
            assert!(!evidence.callback_invoked);
            assert!(!evidence.transport_opened);
        }
    }

    #[test]
    fn stage5g_edb_r5_canceled_and_expired_late_fills_are_grst11_candidates() {
        for (index, status) in [OrderStatus::Canceled, OrderStatus::Expired]
            .into_iter()
            .enumerate()
        {
            let (restart, rows) = stage5g_edb_r5_terminal_advance_rows(status, false);
            let bound = bound_owning_package(
                &restart,
                &format!("stage5g-edb-r5-terminal-advance-{index}"),
                &format!("stage5g-edb-r5-terminal-advance-{index}-epoch"),
                rows,
            );
            let evidence = reduce_stage5g_fresh_broker_truth(restart, bound).evidence();
            assert_eq!(
                evidence.scenario_id,
                Stage5gRestartScenarioId::Grst11FreshBrokerTruthOverridesStaleHint.frozen_id()
            );
            assert_eq!(
                evidence.reason,
                Stage5gFreshTruthReductionReason::FreshTerminalOrderMatched
            );
            assert!(evidence.candidate_present);
            assert!(!evidence.runtime_mutated);
            assert!(!evidence.callback_invoked);
            assert!(!evidence.transport_opened);
        }
    }

    #[test]
    fn stage5g_edb_r5_terminal_late_fill_evidence_is_canonical_and_parallel() {
        fn evidence_bytes(reverse: bool) -> Vec<u8> {
            let (restart, mut rows) =
                stage5g_edb_r5_terminal_advance_rows(OrderStatus::Canceled, true);
            if reverse {
                rows.trades.reverse();
            }
            let bound = bound_owning_package(
                &restart,
                "stage5g-edb-r5-terminal-canonical",
                "stage5g-edb-r5-terminal-canonical-epoch",
                rows,
            );
            serde_json::to_vec(&reduce_stage5g_fresh_broker_truth(restart, bound).evidence())
                .expect("R5 evidence serializes")
        }

        let canonical = evidence_bytes(false);
        assert_eq!(canonical, evidence_bytes(true));
        let workers = (0..4)
            .map(|index| thread::spawn(move || evidence_bytes(index % 2 == 1)))
            .collect::<Vec<_>>();
        for worker in workers {
            assert_eq!(canonical, worker.join().expect("R5 evidence worker"));
        }
    }

    #[test]
    fn stage5g_edb_r5_terminal_late_fill_regressions_fail_closed() {
        for mutation in 0..5 {
            let (restart, mut rows) =
                stage5g_edb_r5_terminal_advance_rows(OrderStatus::Canceled, false);
            match mutation {
                0 => {
                    rows.trades.truncate(1);
                }
                1 => {
                    rows.trades[0].price += Decimal::ONE;
                }
                2 => {
                    rows.positions[0].qty += Decimal::ONE;
                }
                3 => {
                    rows.orders[0].filled_qty = Decimal::new(2, 1);
                    rows.orders[0].remaining_qty = Some(Decimal::new(8, 1));
                }
                4 => {
                    rows.trades.remove(0);
                    rows.trades[0].qty = rows.orders[0].filled_qty;
                }
                _ => unreachable!(),
            }
            let bound = bound_owning_package(
                &restart,
                &format!("stage5g-edb-r5-terminal-regression-{mutation}"),
                &format!("stage5g-edb-r5-terminal-regression-{mutation}-epoch"),
                rows,
            );
            let evidence = reduce_stage5g_fresh_broker_truth(restart, bound).evidence();
            assert!(!evidence.candidate_present);
            assert!(matches!(
                evidence.disposition,
                "terminal_inconsistency" | "reconciliation_required"
            ));
        }
    }

    #[test]
    fn stage5g_edb_r5_rejected_fill_and_terminal_status_transitions_fail_closed() {
        let rejected = stage5g_edb_r5_terminal_restart(OrderStatus::Rejected, false);
        let rejected_projection = rejected.fresh_truth_reducer_projection();
        let rejected_slot = rejected_projection.slots.first().expect("rejected slot");
        let mut rejected_order = rejected_slot.latest_order.clone().expect("rejected order");
        rejected_order.filled_qty = Decimal::new(1, 1);
        rejected_order.remaining_qty = Some(Decimal::new(9, 1));
        let mut rejected_trade = trade(Decimal::new(1, 1), "ORDER-1");
        rejected_trade.broker_order_id = rejected_order.broker_order_id.clone();
        rejected_trade.client_order_id = rejected_order.client_order_id.clone();
        let rejected_bound = bound_owning_package(
            &rejected,
            "stage5g-edb-r5-rejected-fill",
            "stage5g-edb-r5-rejected-fill-epoch",
            OwningTruthRows::complete(
                vec![rejected_order],
                vec![rejected_trade],
                vec![position(Decimal::new(1, 1))],
            ),
        );
        assert!(
            !reduce_stage5g_fresh_broker_truth(rejected, rejected_bound)
                .evidence()
                .candidate_present
        );

        for (index, status) in [
            OrderStatus::Working,
            OrderStatus::Expired,
            OrderStatus::Filled,
        ]
        .into_iter()
        .enumerate()
        {
            let restart = stage5g_edb_r5_terminal_restart(OrderStatus::Canceled, true);
            let projection = restart.fresh_truth_reducer_projection();
            let slot = projection.slots.first().expect("canceled slot");
            let mut changed = slot.latest_order.clone().expect("canceled order");
            changed.status = status.clone();
            changed.lifecycle = BrokerOrderSnapshot::lifecycle_for(&changed.status);
            if status == OrderStatus::Working {
                changed.filled_qty = Decimal::ZERO;
                changed.remaining_qty = Some(changed.qty);
            } else if status == OrderStatus::Filled {
                changed.filled_qty = changed.qty;
                changed.remaining_qty = Some(Decimal::ZERO);
            }
            let bound = bound_owning_package(
                &restart,
                &format!("stage5g-edb-r5-terminal-transition-{index}"),
                &format!("stage5g-edb-r5-terminal-transition-{index}-epoch"),
                OwningTruthRows::complete(
                    vec![changed],
                    if status == OrderStatus::Working {
                        vec![]
                    } else {
                        slot.trades.clone()
                    },
                    slot.position.clone().into_iter().collect(),
                ),
            );
            let evidence = reduce_stage5g_fresh_broker_truth(restart, bound).evidence();
            assert!(!evidence.candidate_present);
            assert_eq!(
                evidence.reason,
                Stage5gFreshTruthReductionReason::OrderTerminalRegression
            );
        }
    }

    #[test]
    fn stage5g_edb_r5_missing_owned_and_terminal_conflict_retain_history_counts() {
        for escrow in [true, false] {
            let restart = if escrow {
                crate::stage5g_order_position::tests::stage5g_edb_restored_generated_escrow_fixture(
                )
            } else {
                crate::stage5g_order_position::tests::stage5g_edb_restored_awaiting_fixture()
            };
            let mut old_order = order(OrderStatus::Filled, "R5-OLD-ORDER", Decimal::ONE);
            old_order.client_order_id =
                Some(ClientOrderId::new("R5-OLD-CLIENT").expect("old client"));
            let mut old_trade = trade(Decimal::ONE, "R5-OLD-ORDER");
            old_trade.broker_trade_id = BrokerTradeId::new("R5-OLD-TRADE");
            old_trade.client_order_id = old_order.client_order_id.clone();
            let bound = bound_owning_package(
                &restart,
                if escrow {
                    "stage5g-edb-r5-missing-owned-escrow"
                } else {
                    "stage5g-edb-r5-missing-owned"
                },
                if escrow {
                    "stage5g-edb-r5-missing-owned-escrow-epoch"
                } else {
                    "stage5g-edb-r5-missing-owned-epoch"
                },
                OwningTruthRows::complete(vec![old_order], vec![old_trade], vec![]),
            );
            let evidence = reduce_stage5g_fresh_broker_truth(restart, bound).evidence();
            assert!(!evidence.candidate_present);
            assert_eq!(evidence.ignored_unrelated_terminal_order_count, 1);
            assert_eq!(evidence.ignored_unrelated_historical_trade_count, 1);
        }

        let (restart, mut rows) = stage5g_edb_r5_terminal_advance_rows(OrderStatus::Expired, false);
        let mut old_order = order(OrderStatus::Canceled, "R5-CONFLICT-OLD", Decimal::ZERO);
        old_order.client_order_id =
            Some(ClientOrderId::new("R5-CONFLICT-CLIENT").expect("old client"));
        let mut old_trade = trade(Decimal::ONE, "R5-CONFLICT-OLD");
        old_trade.broker_trade_id = BrokerTradeId::new("R5-CONFLICT-TRADE");
        old_trade.client_order_id = old_order.client_order_id.clone();
        rows.orders.push(old_order);
        rows.trades.push(old_trade);
        rows.positions[0].qty += Decimal::ONE;
        let bound = bound_owning_package(
            &restart,
            "stage5g-edb-r5-terminal-conflict-history",
            "stage5g-edb-r5-terminal-conflict-history-epoch",
            rows,
        );
        let evidence = reduce_stage5g_fresh_broker_truth(restart, bound).evidence();
        assert!(!evidence.candidate_present);
        assert_eq!(evidence.ignored_unrelated_terminal_order_count, 1);
        assert_eq!(evidence.ignored_unrelated_historical_trade_count, 1);
    }

    #[test]
    fn stage5g_edb_r3_source_limit_price_authority_fails_closed_for_every_broker_shape() {
        let mut malformed = order(OrderStatus::Working, "LIMIT-MISSING", Decimal::ZERO);
        malformed.order_type = OrderType::Limit;
        malformed.limit_price = None;
        assert!(!stage5g_order_matches_source_action(
            &crate::Stage5gMockIntentAction::Place {
                place_kind: crate::Stage5gMockPlaceKind::Limit,
            },
            &malformed,
        ));
        for (index, order_type, limit_price) in [
            (0, OrderType::Limit, Some(Decimal::new(100, 0))),
            (1, OrderType::Limit, Some(Decimal::new(101, 0))),
            (2, OrderType::Market, None),
            (3, OrderType::Limit, Some(Decimal::new(1005, 1))),
        ] {
            let restart = crate::stage5g_order_position::tests::
                stage5g_edb_restored_generated_limit_escrow_fixture();
            let projection = restart.fresh_truth_reducer_projection();
            let slot = projection.slots.first().expect("limit source slot");
            let mut row =
                discovered_order_for_slot(slot, OrderStatus::Working, &format!("LIMIT-R3-{index}"));
            row.order_type = order_type;
            row.limit_price = limit_price;
            let bound = bound_owning_package(
                &restart,
                &format!("stage5g-edb-r3-limit-{index}"),
                &format!("stage5g-edb-r3-limit-epoch-{index}"),
                OwningTruthRows::complete(vec![row], vec![], vec![]),
            );
            let evidence = reduce_stage5g_fresh_broker_truth(restart, bound).evidence();
            assert_eq!(
                evidence.reason,
                Stage5gFreshTruthReductionReason::SourceLimitPriceAuthorityUnsupported
            );
            assert_eq!(
                evidence.disposition,
                disposition_id(Stage5gRestartReconciliationDisposition::ManualInterventionRequired)
            );
            assert!(!evidence.candidate_present);
        }
    }

    #[test]
    fn stage5g_edb_r3_cancel_command_and_target_order_identities_are_distinct() {
        let mut cancel_slot = slot(Some("TARGET-B1"), false);
        cancel_slot.command_client_order_id =
            ClientOrderId::new("C-CANCEL").expect("cancel client id");
        cancel_slot.target_order_client_order_id =
            Some(ClientOrderId::new("C-PLACE").expect("place client id"));
        cancel_slot.source_action = crate::Stage5gMockIntentAction::Cancel {
            target_order_id: BrokerOrderId::new("TARGET-B1"),
        };
        cancel_slot.intent_class = Stage5gRestartIntentClass::CancelCleanup;
        cancel_slot.side = None;

        let mut target = order(OrderStatus::Canceled, "TARGET-B1", Decimal::ZERO);
        target.client_order_id = Some(ClientOrderId::new("C-PLACE").expect("place client id"));
        let accepted = classify_case(
            &restart(
                Stage5gCleanRestartLifecycleKind::OrderPositionAwaitingCommitted,
                Some(cancel_slot.clone()),
                false,
            ),
            &truth(
                Stage5gFreshPackageLineage::NewFresh,
                true,
                true,
                true,
                vec![target.clone()],
                vec![],
                vec![],
            ),
        );
        let candidate = accepted.candidate.expect("target identity forms candidate");
        assert_eq!(candidate.command_client_order_id.as_str(), "C-CANCEL");
        assert_eq!(
            candidate
                .target_order_client_order_id
                .as_ref()
                .expect("known target client")
                .as_str(),
            "C-PLACE"
        );
        assert_ne!(
            candidate.command_client_order_id,
            candidate
                .target_order_client_order_id
                .expect("distinct target client")
        );

        target.client_order_id = Some(ClientOrderId::new("C-WRONG").expect("wrong target client"));
        let conflict = classify_case(
            &restart(
                Stage5gCleanRestartLifecycleKind::OrderPositionAwaitingCommitted,
                Some(cancel_slot),
                false,
            ),
            &truth(
                Stage5gFreshPackageLineage::NewFresh,
                true,
                true,
                true,
                vec![target],
                vec![],
                vec![],
            ),
        );
        assert_eq!(
            conflict.reason,
            Stage5gFreshTruthReductionReason::TargetOrderIdentityConflict
        );
        assert!(conflict.candidate.is_none());
    }

    #[test]
    fn stage5g_edb_r3_historical_same_instrument_rows_are_partitioned_and_counted() {
        let restart = crate::stage5g_order_position::tests::stage5g_edb_restored_awaiting_fixture();
        let projection = restart.fresh_truth_reducer_projection();
        let slot = projection.slots.first().expect("awaiting slot");
        let mut old_order = order(OrderStatus::Filled, "OLD-B1", Decimal::ONE);
        old_order.client_order_id = Some(ClientOrderId::new("OLD-C1").expect("old client"));
        let mut old_trade = trade(Decimal::ONE, "OLD-B1");
        old_trade.broker_trade_id = BrokerTradeId::new("OLD-T1");
        old_trade.client_order_id = Some(ClientOrderId::new("OLD-C1").expect("old client"));
        let mut orders = slot.latest_order.clone().into_iter().collect::<Vec<_>>();
        orders.push(old_order);
        let mut trades = slot.trades.clone();
        trades.push(old_trade);
        let bound = bound_owning_package(
            &restart,
            "stage5g-edb-r3-historical-rows",
            "stage5g-edb-r3-historical-rows-epoch",
            OwningTruthRows::complete(orders, trades, slot.position.clone().into_iter().collect()),
        );
        let evidence = reduce_stage5g_fresh_broker_truth(restart, bound).evidence();
        assert!(evidence.candidate_present);
        assert_eq!(evidence.ignored_unrelated_terminal_order_count, 1);
        assert_eq!(evidence.ignored_unrelated_historical_trade_count, 1);
    }

    #[test]
    fn stage5g_edb_r3_semantic_terminal_refresh_ignores_receipt_and_volatile_pnl() {
        let restart =
            crate::stage5g_order_position::tests::stage5g_edb_restored_terminal_applied_fixture();
        let projection = restart.fresh_truth_reducer_projection();
        let slot = projection.slots.first().expect("terminal slot");
        let refreshed_at = Utc.with_ymd_and_hms(2029, 1, 1, 0, 0, 0).unwrap();
        let mut refreshed_order = slot.latest_order.clone().expect("terminal order");
        refreshed_order.received_ts = refreshed_at;
        let refreshed_trades = slot
            .trades
            .iter()
            .cloned()
            .map(|mut trade| {
                trade.received_ts = refreshed_at;
                trade
            })
            .collect::<Vec<_>>();
        let mut refreshed_position = slot.position.clone().expect("terminal position");
        refreshed_position.received_ts = refreshed_at;
        refreshed_position.unrealized_pnl = Some(Decimal::new(12345, 2));
        let bound = bound_owning_package(
            &restart,
            "stage5g-edb-r3-semantic-terminal-refresh",
            "stage5g-edb-r3-semantic-terminal-refresh-epoch",
            OwningTruthRows::complete(
                vec![refreshed_order],
                refreshed_trades,
                vec![refreshed_position],
            ),
        );
        let evidence = reduce_stage5g_fresh_broker_truth(restart, bound).evidence();
        assert_eq!(
            evidence.scenario_id,
            Stage5gRestartScenarioId::Grst06RestartAfterTerminalPositionApplied.frozen_id()
        );
        assert_eq!(
            evidence.disposition,
            disposition_id(
                Stage5gRestartReconciliationDisposition::ContinueFromCommittedCheckpoint
            )
        );
        assert!(!evidence.candidate_present);
    }

    #[test]
    fn stage5g_edb_r3_shared_order_and_trade_correlation_truth_tables_are_exact() {
        let owned_client = ClientOrderId::new("OWNED-C1").expect("owned client");
        let owned_broker = BrokerOrderId::new("OWNED-B1");
        let mut owned = order(OrderStatus::Working, "OWNED-B1", Decimal::ZERO);
        owned.client_order_id = Some(owned_client.clone());

        assert_eq!(
            stage5g_order_ownership_correlation(&owned, Some(&owned_client), Some(&owned_broker),),
            Stage5gOrderOwnershipCorrelation::ExactOwned
        );
        let wrong_client = ClientOrderId::new("WRONG-C1").expect("wrong client");
        assert_eq!(
            stage5g_order_ownership_correlation(&owned, Some(&wrong_client), Some(&owned_broker),),
            Stage5gOrderOwnershipCorrelation::ConflictingOwnedIdentity
        );
        let unrelated_broker = BrokerOrderId::new("OTHER-B1");
        let unrelated_client = ClientOrderId::new("OTHER-C1").expect("other client");
        let mut terminal = order(OrderStatus::Canceled, "OLD-B1", Decimal::ZERO);
        terminal.client_order_id = Some(ClientOrderId::new("OLD-C1").expect("old client"));
        assert_eq!(
            stage5g_order_ownership_correlation(
                &terminal,
                Some(&unrelated_client),
                Some(&unrelated_broker),
            ),
            Stage5gOrderOwnershipCorrelation::UnrelatedTerminal
        );
        assert_eq!(
            stage5g_order_ownership_correlation(
                &owned,
                Some(&unrelated_client),
                Some(&unrelated_broker),
            ),
            Stage5gOrderOwnershipCorrelation::NonOwnedActive
        );
        let mut unknown = owned.clone();
        unknown.status = OrderStatus::Unknown("broker-new-state".to_owned());
        unknown.lifecycle = BrokerOrderLifecycle::Unknown;
        assert_eq!(
            stage5g_order_ownership_correlation(
                &unknown,
                Some(&unrelated_client),
                Some(&unrelated_broker),
            ),
            Stage5gOrderOwnershipCorrelation::NonOwnedUnknown
        );

        let mut linked = trade(Decimal::ONE, "OWNED-B1");
        linked.client_order_id = Some(owned_client.clone());
        assert_eq!(
            stage5g_exact_trade_order_linkage(&owned, &linked),
            Stage5gTradeOrderLinkage::Exact
        );
        let mut client_only = linked.clone();
        client_only.broker_order_id = None;
        assert_eq!(
            stage5g_exact_trade_order_linkage(&owned, &client_only),
            Stage5gTradeOrderLinkage::Exact
        );
        let mut broker_only = linked.clone();
        broker_only.client_order_id = None;
        assert_eq!(
            stage5g_exact_trade_order_linkage(&owned, &broker_only),
            Stage5gTradeOrderLinkage::Exact
        );
        let mut client_match_broker_conflict = linked.clone();
        client_match_broker_conflict.broker_order_id = Some(BrokerOrderId::new("WRONG-B1"));
        assert_eq!(
            stage5g_exact_trade_order_linkage(&owned, &client_match_broker_conflict),
            Stage5gTradeOrderLinkage::Conflict
        );
        let mut broker_match_client_conflict = linked.clone();
        broker_match_client_conflict.client_order_id = Some(wrong_client);
        assert_eq!(
            stage5g_exact_trade_order_linkage(&owned, &broker_match_client_conflict),
            Stage5gTradeOrderLinkage::Conflict
        );
        let mut no_ids = linked.clone();
        no_ids.client_order_id = None;
        no_ids.broker_order_id = None;
        assert_eq!(
            stage5g_exact_trade_order_linkage(&owned, &no_ids),
            Stage5gTradeOrderLinkage::Unrelated
        );
        let mut historical = linked;
        historical.client_order_id = Some(unrelated_client);
        historical.broker_order_id = Some(unrelated_broker);
        assert_eq!(
            stage5g_exact_trade_order_linkage(&owned, &historical),
            Stage5gTradeOrderLinkage::Unrelated
        );
    }

    #[test]
    fn stage5g_edb_r3_semantic_comparators_ignore_only_reviewed_volatile_fields() {
        let committed_at = Utc.with_ymd_and_hms(2028, 1, 1, 0, 0, 0).unwrap();
        let refreshed_at = Utc.with_ymd_and_hms(2029, 1, 1, 0, 0, 0).unwrap();
        let committed_order = order(OrderStatus::Filled, "SEM-B1", Decimal::ONE);
        let mut refreshed_order = committed_order.clone();
        refreshed_order.received_ts = refreshed_at;
        assert!(stage5g_semantic_terminal_order_matches(
            &committed_order,
            &refreshed_order
        ));
        refreshed_order.status = OrderStatus::Canceled;
        refreshed_order.lifecycle = BrokerOrderSnapshot::lifecycle_for(&refreshed_order.status);
        assert!(!stage5g_semantic_terminal_order_matches(
            &committed_order,
            &refreshed_order
        ));
        let mut changed_fill = committed_order.clone();
        changed_fill.filled_qty = Decimal::ZERO;
        assert!(!stage5g_semantic_terminal_order_matches(
            &committed_order,
            &changed_fill
        ));

        let committed_position = position(Decimal::ONE);
        let mut refreshed_position = committed_position.clone();
        refreshed_position.received_ts = refreshed_at;
        refreshed_position.unrealized_pnl = Some(Decimal::new(999, 2));
        assert!(stage5g_semantic_position_matches(
            &committed_position,
            &refreshed_position
        ));
        refreshed_position.qty = Decimal::new(2, 0);
        assert!(!stage5g_semantic_position_matches(
            &committed_position,
            &refreshed_position
        ));

        let mut committed_trade = trade(Decimal::ONE, "SEM-B1");
        committed_trade.received_ts = committed_at;
        let mut refreshed_trade = committed_trade.clone();
        refreshed_trade.received_ts = refreshed_at;
        assert!(stage5g_immutable_trade_payload_matches(
            &committed_trade,
            &refreshed_trade
        ));
        refreshed_trade.qty = Decimal::new(2, 0);
        assert!(!stage5g_immutable_trade_payload_matches(
            &committed_trade,
            &refreshed_trade
        ));
    }

    #[test]
    fn stage5g_edb_r3_owning_grst03_runs_full_authenticated_path() {
        let restart = crate::stage5g_order_position::tests::
            stage5g_edb_restored_generated_working_escrow_fixture();
        let projection = restart.fresh_truth_reducer_projection();
        let slot = projection.slots.first().expect("working slot");
        let row = discovered_order_for_slot(slot, OrderStatus::Working, "WORKING-TARGET-R3");
        let bound = bound_owning_package(
            &restart,
            "stage5g-edb-r3-owning-grst03",
            "stage5g-edb-r3-owning-grst03-epoch",
            OwningTruthRows::complete(vec![row], vec![], vec![]),
        );
        let evidence = reduce_stage5g_fresh_broker_truth(restart, bound).evidence();
        assert_eq!(
            evidence.scenario_id,
            Stage5gRestartScenarioId::Grst03RestartWithWorkingOrder.frozen_id()
        );
        assert!(evidence.candidate_present);
        assert!(!evidence.runtime_mutated);
        assert!(!evidence.callback_invoked);
        assert!(!evidence.transport_opened);
    }

    #[test]
    fn stage5g_edb_r3_working_order_requires_complete_pre_position_truth() {
        let restart = crate::stage5g_order_position::tests::
            stage5g_edb_restored_generated_working_escrow_fixture();
        let projection = restart.fresh_truth_reducer_projection();
        let slot = projection.slots.first().expect("working slot");
        let row = discovered_order_for_slot(slot, OrderStatus::Working, "WORKING-TARGET-R3");
        let bound = bound_owning_package(
            &restart,
            "stage5g-edb-r3-working-incomplete-position",
            "stage5g-edb-r3-working-incomplete-position-epoch",
            OwningTruthRows {
                orders_complete: true,
                trades_complete: true,
                positions_complete: false,
                orders: vec![row],
                trades: vec![],
                positions: vec![],
            },
        );
        let evidence = reduce_stage5g_fresh_broker_truth(restart, bound).evidence();
        assert_eq!(
            evidence.reason,
            Stage5gFreshTruthReductionReason::PositionsTruthIncomplete
        );
        assert!(!evidence.candidate_present);
    }

    #[test]
    fn stage5g_edb_r1_owning_row_order_and_parallel_evidence_are_deterministic() {
        fn owning_filled_evidence(reverse: bool) -> Vec<u8> {
            let restart =
                crate::stage5g_order_position::tests::stage5g_edb_restored_awaiting_fixture();
            let projection = restart.fresh_truth_reducer_projection();
            let slot = projection.slots.first().expect("awaiting slot");
            let mut filled = slot.latest_order.clone().expect("source order");
            filled.status = OrderStatus::Filled;
            filled.lifecycle = BrokerOrderSnapshot::lifecycle_for(&filled.status);
            filled.filled_qty = filled.qty;
            filled.remaining_qty = Some(Decimal::ZERO);
            let mut first = slot.trades.first().cloned().expect("source trade");
            first.qty = Decimal::new(4, 1);
            let mut second = first.clone();
            second.broker_trade_id = BrokerTradeId::new("FINAM-R1-TRADE-B");
            second.qty = Decimal::new(6, 1);
            second.source_ts += chrono::Duration::milliseconds(1);
            second.received_ts += chrono::Duration::milliseconds(1);
            let mut trades = vec![first, second];
            if reverse {
                trades.reverse();
            }
            let bound = bound_owning_package(
                &restart,
                "stage5g-edb-r1-owning-deterministic",
                "stage5g-edb-r1-epoch-deterministic",
                OwningTruthRows::complete(vec![filled], trades, vec![position(Decimal::ONE)]),
            );
            let evidence = reduce_stage5g_fresh_broker_truth(restart, bound).evidence();
            serde_json::to_vec(&evidence).expect("redacted evidence")
        }

        let canonical = owning_filled_evidence(false);
        assert_eq!(canonical, owning_filled_evidence(true));
        let handles = (0..4)
            .map(|index| thread::spawn(move || owning_filled_evidence(index % 2 == 1)))
            .collect::<Vec<_>>();
        for handle in handles {
            assert_eq!(canonical, handle.join().expect("owning worker"));
        }
    }
}
