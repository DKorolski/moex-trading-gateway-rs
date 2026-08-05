//! Stage 5G-e-d-b deterministic fresh BrokerTruth reducer.
//!
//! This child module consumes only the two accepted linear authorities.  It
//! classifies and constructs an opaque in-memory candidate; it cannot mutate a
//! runtime, invoke a callback, persist state, publish Redis data or dispatch a
//! broker command.

use broker_core::{
    instrument_identity_matches, BrokerOrderId, BrokerOrderSnapshot, BrokerPositionSnapshot,
    BrokerTradeSnapshot, ClientOrderId, OrderSide, OrderStatus,
};
use rust_decimal::Decimal;
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::stage5g_clean_restart::{
    Stage5gCleanRestartLifecycleKind, Stage5gFreshTruthRestartProjection,
};
use crate::Stage5gCleanRestartedCapability;

use super::{
    Stage5gFreshPackageLineage, Stage5gRestartReconciliationDisposition, Stage5gRestartScenarioId,
    Stage5gValidatedFreshBrokerTruthPackage,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Stage5gFreshTruthReductionReason {
    ExactPackageReplay,
    ExactHistoricalReplay,
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
    OperationalIdentityConflict,
    UnsupportedLifecycleCombination,
    TerminalContradiction,
}

/// Opaque candidate for the separately reviewed e-d-c application/evidence
/// boundary.  Deliberately not Clone, Debug, Serialize or Deserialize.
pub(crate) struct Stage5gOwnedReconciliationCandidate {
    request_id: String,
    client_order_id: ClientOrderId,
    broker_order_id: Option<BrokerOrderId>,
    side: Option<OrderSide>,
    target_qty: Option<String>,
    order: BrokerOrderSnapshot,
    trades: Vec<BrokerTradeSnapshot>,
    position: Option<BrokerPositionSnapshot>,
}

/// Linear reduction result. Ownership of both accepted inputs is retained so
/// no competing continuation can be created after classification.
pub(crate) struct Stage5gFreshTruthReduction {
    scenario_id: Stage5gRestartScenarioId,
    disposition: Stage5gRestartReconciliationDisposition,
    reason: Stage5gFreshTruthReductionReason,
    pre_semantic_fingerprint_sha256: String,
    post_candidate_fingerprint_sha256: String,
    candidate: Option<Stage5gOwnedReconciliationCandidate>,
    _restart: Stage5gCleanRestartedCapability,
    _truth: Stage5gValidatedFreshBrokerTruthPackage,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct Stage5gFreshTruthReductionEvidence {
    pub(crate) scenario_id: &'static str,
    pub(crate) disposition: &'static str,
    pub(crate) reason: Stage5gFreshTruthReductionReason,
    pub(crate) package_fingerprint_sha256: String,
    pub(crate) pre_semantic_fingerprint_sha256: String,
    pub(crate) post_candidate_fingerprint_sha256: String,
    pub(crate) orders_complete: bool,
    pub(crate) trades_complete: bool,
    pub(crate) positions_complete: bool,
    pub(crate) candidate_present: bool,
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
            package_fingerprint_sha256: self._truth.canonical_fingerprint_sha256.clone(),
            pre_semantic_fingerprint_sha256: self.pre_semantic_fingerprint_sha256.clone(),
            post_candidate_fingerprint_sha256: self.post_candidate_fingerprint_sha256.clone(),
            orders_complete: self._truth.orders_complete,
            trades_complete: self._truth.trades_complete,
            positions_complete: self._truth.positions_complete,
            candidate_present: self.candidate.is_some(),
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
    truth: Stage5gValidatedFreshBrokerTruthPackage,
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
}

fn classify(
    restart: &Stage5gFreshTruthRestartProjection,
    truth: &Stage5gValidatedFreshBrokerTruthPackage,
) -> Classified {
    if !cross_binding_matches(restart, truth) {
        return blocked(
            Stage5gRestartScenarioId::Grst10ConflictingReplayBlocks,
            Stage5gRestartReconciliationDisposition::ReconciliationRequired,
            Stage5gFreshTruthReductionReason::OperationalIdentityConflict,
        );
    }

    match truth.lineage {
        Stage5gFreshPackageLineage::ExactLastReconciledReplay => {
            return blocked(
                replay_scenario(restart),
                Stage5gRestartReconciliationDisposition::ExactReplay,
                if restart.lifecycle_kind == Stage5gCleanRestartLifecycleKind::TimerReady {
                    Stage5gFreshTruthReductionReason::TimerCheckpointExact
                } else {
                    Stage5gFreshTruthReductionReason::ExactPackageReplay
                },
            );
        }
        Stage5gFreshPackageLineage::ExactAcceptedHistoricalReplay => {
            return blocked(
                replay_scenario(restart),
                Stage5gRestartReconciliationDisposition::ExactReplay,
                Stage5gFreshTruthReductionReason::ExactHistoricalReplay,
            );
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

    let target_orders = truth
        .orders
        .iter()
        .filter(|row| instrument_identity_matches(&row.instrument, &restart.instrument_id))
        .collect::<Vec<_>>();
    let target_trades = truth
        .trades
        .iter()
        .filter(|row| instrument_identity_matches(&row.instrument, &restart.instrument_id))
        .collect::<Vec<_>>();
    let target_positions = truth
        .positions
        .iter()
        .filter(|row| instrument_identity_matches(&row.instrument, &restart.instrument_id))
        .collect::<Vec<_>>();

    if target_positions.len() > 1 {
        return blocked(
            Stage5gRestartScenarioId::Grst12MissingOrAmbiguousTruthRequiresReconciliation,
            Stage5gRestartReconciliationDisposition::TerminalInconsistency,
            Stage5gFreshTruthReductionReason::UnexpectedTargetPosition,
        );
    }

    if restart.lifecycle_kind == Stage5gCleanRestartLifecycleKind::TimerReady {
        return blocked(
            Stage5gRestartScenarioId::Grst07RestartAtTimerCheckpoint,
            Stage5gRestartReconciliationDisposition::ContinueFromCommittedCheckpoint,
            Stage5gFreshTruthReductionReason::TimerCheckpointExact,
        );
    }

    if restart.slots.is_empty() {
        let target_position_is_flat = match target_positions.first() {
            Some(position) => position.qty == Decimal::ZERO,
            None => true,
        };
        return if target_orders.is_empty() && target_position_is_flat {
            blocked(
                Stage5gRestartScenarioId::Grst01RestartBeforeAck,
                Stage5gRestartReconciliationDisposition::AwaitFreshBrokerTruth,
                Stage5gFreshTruthReductionReason::AuthoritativeOrderMissing,
            )
        } else {
            blocked(
                Stage5gRestartScenarioId::Grst12MissingOrAmbiguousTruthRequiresReconciliation,
                Stage5gRestartReconciliationDisposition::TerminalInconsistency,
                Stage5gFreshTruthReductionReason::UnexpectedTargetPosition,
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
    let matching = target_orders
        .iter()
        .copied()
        .filter(|order| {
            order.client_order_id.as_ref() == Some(&slot.expected_client_order_id)
                || slot
                    .broker_order_id
                    .as_ref()
                    .is_some_and(|expected| order.broker_order_id.as_ref() == Some(expected))
        })
        .collect::<Vec<_>>();
    if matching.len() > 1 {
        return blocked(
            Stage5gRestartScenarioId::Grst10ConflictingReplayBlocks,
            Stage5gRestartReconciliationDisposition::ManualInterventionRequired,
            Stage5gFreshTruthReductionReason::ClientOrderIdentityConflict,
        );
    }
    if target_orders.len() != matching.len() && !target_orders.is_empty() {
        return blocked(
            Stage5gRestartScenarioId::Grst12MissingOrAmbiguousTruthRequiresReconciliation,
            Stage5gRestartReconciliationDisposition::ManualInterventionRequired,
            Stage5gFreshTruthReductionReason::ClientOrderIdentityConflict,
        );
    }
    let Some(order) = matching.first().copied() else {
        return if restart.generated_intent_escrow_fingerprint_sha256.is_some() {
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
    };

    if order.client_order_id.as_ref() != Some(&slot.expected_client_order_id) {
        return blocked(
            Stage5gRestartScenarioId::Grst10ConflictingReplayBlocks,
            Stage5gRestartReconciliationDisposition::ReconciliationRequired,
            Stage5gFreshTruthReductionReason::ClientOrderIdentityConflict,
        );
    }
    if slot
        .broker_order_id
        .as_ref()
        .is_some_and(|expected| order.broker_order_id.as_ref() != Some(expected))
    {
        return blocked(
            Stage5gRestartScenarioId::Grst10ConflictingReplayBlocks,
            Stage5gRestartReconciliationDisposition::ManualInterventionRequired,
            Stage5gFreshTruthReductionReason::BrokerOrderIdentityConflict,
        );
    }
    if slot.side.is_some_and(|side| side != order.side) {
        return blocked(
            Stage5gRestartScenarioId::Grst10ConflictingReplayBlocks,
            Stage5gRestartReconciliationDisposition::TerminalInconsistency,
            Stage5gFreshTruthReductionReason::TerminalContradiction,
        );
    }

    let linked_trades = target_trades
        .iter()
        .copied()
        .filter(|trade| {
            trade.client_order_id == order.client_order_id
                || trade.broker_order_id == order.broker_order_id
        })
        .collect::<Vec<_>>();
    if linked_trades.len() != target_trades.len() {
        return blocked(
            Stage5gRestartScenarioId::Grst10ConflictingReplayBlocks,
            Stage5gRestartReconciliationDisposition::ManualInterventionRequired,
            Stage5gFreshTruthReductionReason::TradeIdentityConflict,
        );
    }
    if target_trades.iter().any(|trade| {
        trade.client_order_id == order.client_order_id
            || trade.broker_order_id == order.broker_order_id
    }) && linked_trades.iter().any(|trade| trade.side != order.side)
    {
        return blocked(
            Stage5gRestartScenarioId::Grst10ConflictingReplayBlocks,
            Stage5gRestartReconciliationDisposition::TerminalInconsistency,
            Stage5gFreshTruthReductionReason::TradeIdentityConflict,
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
        );
    }

    match order.status {
        OrderStatus::New | OrderStatus::Working => candidate(
            if slot.broker_order_id.is_none() {
                Stage5gRestartScenarioId::Grst02RestartAfterAckBeforeOrder
            } else {
                Stage5gRestartScenarioId::Grst03RestartWithWorkingOrder
            },
            Stage5gFreshTruthReductionReason::FreshWorkingOrderMatched,
            slot,
            order,
            linked_trades,
            target_positions.first().copied(),
        ),
        OrderStatus::PartiallyFilled => {
            if !truth.positions_complete {
                return incomplete(
                    restart,
                    Stage5gFreshTruthReductionReason::PositionsTruthIncomplete,
                );
            }
            let Some(position) = target_positions.first().copied() else {
                return blocked(
                    Stage5gRestartScenarioId::Grst04RestartAfterPartialFill,
                    Stage5gRestartReconciliationDisposition::ReconciliationRequired,
                    Stage5gFreshTruthReductionReason::PositionQuantityMismatch,
                );
            };
            if !position_matches_fill(position, order) {
                return blocked(
                    Stage5gRestartScenarioId::Grst04RestartAfterPartialFill,
                    Stage5gRestartReconciliationDisposition::ReconciliationRequired,
                    if (position.qty < Decimal::ZERO) != (signed_fill(order) < Decimal::ZERO) {
                        Stage5gFreshTruthReductionReason::PositionDirectionMismatch
                    } else {
                        Stage5gFreshTruthReductionReason::PositionQuantityMismatch
                    },
                );
            }
            candidate(
                Stage5gRestartScenarioId::Grst04RestartAfterPartialFill,
                Stage5gFreshTruthReductionReason::PartialFillPositionConverged,
                slot,
                order,
                linked_trades,
                Some(position),
            )
        }
        OrderStatus::Filled => {
            if !truth.positions_complete {
                return blocked(
                    Stage5gRestartScenarioId::Grst05RestartFilledBeforePosition,
                    Stage5gRestartReconciliationDisposition::AwaitFreshBrokerTruth,
                    Stage5gFreshTruthReductionReason::PositionsTruthIncomplete,
                );
            }
            let fresh_position = target_positions.first().copied();
            if let Some(position) = fresh_position {
                if !position_matches_fill(position, order) {
                    return blocked(
                        Stage5gRestartScenarioId::Grst10ConflictingReplayBlocks,
                        Stage5gRestartReconciliationDisposition::TerminalInconsistency,
                        Stage5gFreshTruthReductionReason::PositionQuantityMismatch,
                    );
                }
            }
            if slot.terminal
                && slot.position.as_ref().map(|position| position.qty)
                    == fresh_position.map(|position| position.qty)
            {
                blocked(
                    Stage5gRestartScenarioId::Grst06RestartAfterTerminalPositionApplied,
                    Stage5gRestartReconciliationDisposition::ContinueFromCommittedCheckpoint,
                    Stage5gFreshTruthReductionReason::TerminalPositionAlreadyApplied,
                )
            } else {
                candidate(
                    Stage5gRestartScenarioId::Grst11FreshBrokerTruthOverridesStaleHint,
                    Stage5gFreshTruthReductionReason::FreshTerminalOrderMatched,
                    slot,
                    order,
                    linked_trades,
                    fresh_position,
                )
            }
        }
        OrderStatus::Canceled | OrderStatus::Rejected | OrderStatus::Expired => candidate(
            Stage5gRestartScenarioId::Grst11FreshBrokerTruthOverridesStaleHint,
            Stage5gFreshTruthReductionReason::FreshTerminalOrderMatched,
            slot,
            order,
            linked_trades,
            target_positions.first().copied(),
        ),
        OrderStatus::Unknown(_) => blocked(
            Stage5gRestartScenarioId::Grst12MissingOrAmbiguousTruthRequiresReconciliation,
            Stage5gRestartReconciliationDisposition::ReconciliationRequired,
            Stage5gFreshTruthReductionReason::UnsupportedLifecycleCombination,
        ),
    }
}

fn cross_binding_matches(
    restart: &Stage5gFreshTruthRestartProjection,
    truth: &Stage5gValidatedFreshBrokerTruthPackage,
) -> bool {
    restart.account_id == truth.operational_identity.account_id
        && restart.strategy_id == truth.operational_identity.strategy_definition_id.as_str()
        && restart.config_fingerprint_sha256
            == truth
                .operational_identity
                .config_fingerprint_sha256
                .as_str()
        && instrument_identity_matches(
            &restart.instrument_id,
            &truth.operational_identity.target_instrument,
        )
        && restart.strategy_state_fingerprint_sha256
            == restart.reconstructed_runtime_state_fingerprint_sha256
}

fn replay_scenario(restart: &Stage5gFreshTruthRestartProjection) -> Stage5gRestartScenarioId {
    if restart.lifecycle_kind == Stage5gCleanRestartLifecycleKind::TimerReady {
        Stage5gRestartScenarioId::Grst07RestartAtTimerCheckpoint
    } else {
        Stage5gRestartScenarioId::Grst09ExactReplayIsIdempotent
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
    }
}

fn candidate(
    scenario_id: Stage5gRestartScenarioId,
    reason: Stage5gFreshTruthReductionReason,
    slot: &crate::stage5g_order_position::Stage5gFreshTruthRestartSlotProjection,
    order: &BrokerOrderSnapshot,
    trades: Vec<&BrokerTradeSnapshot>,
    position: Option<&BrokerPositionSnapshot>,
) -> Classified {
    let candidate = Stage5gOwnedReconciliationCandidate {
        request_id: slot.request_id.clone(),
        client_order_id: slot.expected_client_order_id.clone(),
        broker_order_id: order.broker_order_id.clone(),
        side: slot.side,
        target_qty: slot.target_qty.clone(),
        order: order.clone(),
        trades: trades.into_iter().cloned().collect(),
        position: position.cloned(),
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
    }
}

fn candidate_is_self_consistent(candidate: &Stage5gOwnedReconciliationCandidate) -> bool {
    let order = &candidate.order;
    !candidate.request_id.is_empty()
        && order.client_order_id.as_ref() == Some(&candidate.client_order_id)
        && order.broker_order_id == candidate.broker_order_id
        && candidate
            .side
            .map(|side| side == order.side)
            .unwrap_or(true)
        && candidate
            .target_qty
            .as_deref()
            .map(|qty| qty.parse::<Decimal>().ok() == Some(order.qty))
            .unwrap_or(true)
        && candidate.trades.iter().all(|trade| {
            trade.side == order.side
                && (trade.client_order_id == order.client_order_id
                    || trade.broker_order_id == order.broker_order_id)
        })
        && candidate
            .trades
            .iter()
            .fold(Decimal::ZERO, |sum, trade| sum + trade.qty)
            == order.filled_qty
        && match candidate.position.as_ref() {
            Some(position) => {
                position.account_id == order.account_id
                    && instrument_identity_matches(&position.instrument, &order.instrument)
            }
            None => true,
        }
}

fn signed_fill(order: &BrokerOrderSnapshot) -> Decimal {
    match order.side {
        OrderSide::Buy => order.filled_qty,
        OrderSide::Sell => -order.filled_qty,
    }
}

fn position_matches_fill(position: &BrokerPositionSnapshot, order: &BrokerOrderSnapshot) -> bool {
    position.qty == signed_fill(order)
}

fn semantic_sha256<T: Serialize>(value: &T) -> String {
    let bytes = serde_json::to_vec(value).expect("accepted reducer projection is serializable");
    format!("{:x}", Sha256::digest(bytes))
}

fn candidate_fingerprint(candidate: &Stage5gOwnedReconciliationCandidate) -> String {
    #[derive(Serialize)]
    struct CandidateProjection<'a> {
        domain: &'static str,
        request_id: &'a str,
        client_order_id: &'a ClientOrderId,
        broker_order_id: &'a Option<BrokerOrderId>,
        side: Option<OrderSide>,
        target_qty: &'a Option<String>,
        order: &'a BrokerOrderSnapshot,
        trades: &'a [BrokerTradeSnapshot],
        position: &'a Option<BrokerPositionSnapshot>,
    }
    semantic_sha256(&CandidateProjection {
        domain: "moex.stage5g.fresh-truth-candidate.v1",
        request_id: &candidate.request_id,
        client_order_id: &candidate.client_order_id,
        broker_order_id: &candidate.broker_order_id,
        side: candidate.side,
        target_qty: &candidate.target_qty,
        order: &candidate.order,
        trades: &candidate.trades,
        position: &candidate.position,
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
        Market, OrderType,
    };
    use chrono::{TimeZone, Utc};

    use super::*;
    use crate::stage5g_clean_restart::Stage5gFreshTruthRestartProjection;
    use crate::stage5g_fresh_broker_truth::{
        Stage5gBrokerId, Stage5gDeploymentGeneration, Stage5gDeploymentId, Stage5gFeedGeneration,
        Stage5gGatewayInstanceId, Stage5gOperationalIdentityV1, Stage5gPackageId, Stage5gSha256,
        Stage5gSnapshotEpoch, Stage5gStrategyDefinitionId, Stage5gStrategyInstanceId,
    };
    use crate::stage5g_order_position::Stage5gFreshTruthRestartSlotProjection;
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
            slots: slot.into_iter().collect(),
            generated_intent_escrow_fingerprint_sha256: escrow.then(|| SHA.to_owned()),
        }
    }

    fn slot(
        broker_order_id: Option<&str>,
        terminal: bool,
    ) -> Stage5gFreshTruthRestartSlotProjection {
        Stage5gFreshTruthRestartSlotProjection {
            request_id: "00000000-0000-0000-0000-000000000001".to_owned(),
            expected_client_order_id: ClientOrderId::new("CLIENT-1").expect("client id"),
            broker_order_id: broker_order_id.map(BrokerOrderId::new),
            side: Some(OrderSide::Buy),
            target_qty: Some("1".to_owned()),
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
            order_type: OrderType::Limit,
            time_in_force: None,
            lifecycle: BrokerOrderSnapshot::lifecycle_for(&status),
            status,
            qty,
            filled_qty,
            remaining_qty: Some(qty - filled_qty),
            limit_price: Some(Decimal::new(2200, 0)),
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
        }
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
                    restart.slots[0].target_qty = Some("2".to_owned());
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
                    Stage5gFreshPackageLineage::ExactLastReconciledReplay,
                    true,
                    true,
                    true,
                    vec![],
                    vec![],
                    vec![],
                ),
                expected_scenario: Stage5gRestartScenarioId::Grst09ExactReplayIsIdempotent,
                expected_disposition: Stage5gRestartReconciliationDisposition::ExactReplay,
                expected_reason: Stage5gFreshTruthReductionReason::ExactPackageReplay,
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
                expected_reason: Stage5gFreshTruthReductionReason::BrokerOrderIdentityConflict,
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
        let actual = classify(&case.restart, &case.truth);
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
            Stage5gValidatedFreshBrokerTruthPackage,
        ) -> Stage5gFreshTruthReduction = reduce_stage5g_fresh_broker_truth;
        let actual = cases()
            .into_iter()
            .map(|case| classify(&case.restart, &case.truth).scenario_id)
            .collect::<Vec<_>>();
        assert_eq!(actual, Stage5gRestartScenarioId::ALL);
        assert_eq!(actual.iter().copied().collect::<HashSet<_>>().len(), 12);
    }

    #[test]
    fn stage5g_edb_sequential_and_row_order_are_deterministic() {
        for mut case in cases() {
            let first = classify(&case.restart, &case.truth);
            case.truth.orders.reverse();
            case.truth.trades.reverse();
            case.truth.positions.reverse();
            let second = classify(&case.restart, &case.truth);
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
        let actual = classify(&case.restart, &case.truth);
        assert_eq!(
            actual.disposition,
            Stage5gRestartReconciliationDisposition::ExactReplay
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
            let actual = classify(&case.restart, &case.truth);
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
                Stage5gFreshTruthReductionReason::BrokerOrderIdentityConflict,
            ),
            (
                client_conflict,
                Stage5gFreshTruthReductionReason::ClientOrderIdentityConflict,
            ),
            (
                trade_conflict,
                Stage5gFreshTruthReductionReason::TradeIdentityConflict,
            ),
        ] {
            let actual = classify(&case.restart, &case.truth);
            assert_eq!(actual.reason, reason);
            assert!(actual.candidate.is_none());
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
            let actual = classify(&case.restart, &case.truth);
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
        target_qty_mismatch.restart.slots[0].target_qty = Some("2".to_owned());

        let mut partial_position_mismatch = cases().remove(3);
        partial_position_mismatch.truth.positions[0].qty = Decimal::new(5, 1);

        for case in [target_qty_mismatch, partial_position_mismatch] {
            let actual = classify(&case.restart, &case.truth);
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
            let actual = classify(&restart, &truth);
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
                    let actual = classify(&case.restart, &case.truth);
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
                let actual = classify(&case.restart, &case.truth);
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
}
