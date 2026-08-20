//! Stage 8A-4 I4 read-only terminal ACK/current-readiness composition.
//!
//! Everything in this module is FINAM-private. It publishes nothing, owns no
//! Redis continuation and cannot construct transport or execution authority.

use std::path::Path;

use broker_core::command::CommandAckStatus;
use broker_core::{
    BrokerOrderId, BrokerReadinessSnapshot, BrokerTruthSnapshot, ClientOrderId,
    CommandAckReasonCode, StrategyRequestId,
};
use chrono::{DateTime, Utc};
use runtime_durable_service::{
    Stage7bCompositeReadinessSnapshot, Stage7bRecoveryReadyOwner, Stage7bStage8a4TerminalAuthority,
};
use strategy_runtime_core::{
    Stage5gLifecycleCommitmentKey, Stage6CancelOutcomeV1, Stage6DurableActionKind,
    Stage6ReconciliationLifecycleV2, Stage6RequestFinalDispositionV1,
};

use crate::stage8a1_execution_capability::{
    issue_stage8a4_i4_current_readiness, Stage8a4I4CurrentReadinessEvidence,
};

pub(crate) struct Stage8a4I4TerminalAckFacts {
    strategy_request_id: StrategyRequestId,
    durable_client_order_id: ClientOrderId,
    broker_order_id: Option<BrokerOrderId>,
    status: CommandAckStatus,
    reason_code: Option<CommandAckReasonCode>,
    terminal_request_ack_identity_sha256: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Stage8a4I4ReadinessState {
    Ready,
    Blocked,
}

/// Consumed, no-effect composition result. Its only outward form is a bounded
/// redacted diagnostic; raw identities and current-source payloads remain
/// private and cannot feed transport.
pub(crate) struct Stage8a4I4DerivedAckReadinessFacade {
    ack: Stage8a4I4TerminalAckFacts,
    readiness: Option<Stage8a4I4CurrentReadinessEvidence>,
    readiness_state: Stage8a4I4ReadinessState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Stage8a4I4DerivedDiagnostic {
    pub(crate) ack_status: CommandAckStatus,
    pub(crate) ack_reason_code: Option<CommandAckReasonCode>,
    pub(crate) broker_order_id_present: bool,
    pub(crate) readiness_state: Stage8a4I4ReadinessState,
    pub(crate) terminal_request_ack_identity_sha256: String,
    pub(crate) current_source_evidence_sha256: Option<String>,
    pub(crate) observed_at: Option<DateTime<Utc>>,
    pub(crate) valid_until: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub(crate) enum Stage8a4I4CompositionError {
    #[error("durable terminal authority is unavailable")]
    TerminalAuthorityUnavailable,
    #[error("durable transition has no canonical terminal ACK mapping")]
    AckMappingUnavailable,
}

impl Stage8a4I4DerivedAckReadinessFacade {
    pub(crate) fn diagnostic(&self) -> Stage8a4I4DerivedDiagnostic {
        let _ = (
            self.ack.strategy_request_id,
            &self.ack.durable_client_order_id,
        );
        let _ = self.readiness.as_ref().map(|value| {
            (
                &value.operational_identity_sha256,
                &value.runtime_config_fingerprint_sha256,
                &value.authority_root_sha256,
                &value.accepted_config_sha256,
            )
        });
        Stage8a4I4DerivedDiagnostic {
            ack_status: self.ack.status,
            ack_reason_code: self.ack.reason_code,
            broker_order_id_present: self.ack.broker_order_id.is_some(),
            readiness_state: self.readiness_state,
            terminal_request_ack_identity_sha256: self
                .ack
                .terminal_request_ack_identity_sha256
                .clone(),
            current_source_evidence_sha256: self
                .readiness
                .as_ref()
                .map(|value| value.current_source_evidence_sha256.clone()),
            observed_at: self.readiness.as_ref().map(|value| value.observed_at),
            valid_until: self.readiness.as_ref().map(|value| value.valid_until),
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn compose_stage8a4_i4_readonly(
    owner: &mut Stage7bRecoveryReadyOwner,
    commitment_key: &Stage5gLifecycleCommitmentKey,
    request_id: StrategyRequestId,
    authority_root: &Path,
    accepted_config_sha256: &str,
    composite_readiness: &Stage7bCompositeReadinessSnapshot,
    broker_truth: &BrokerTruthSnapshot,
    broker_readiness: &BrokerReadinessSnapshot,
    now: DateTime<Utc>,
) -> Result<Stage8a4I4DerivedAckReadinessFacade, Stage8a4I4CompositionError> {
    let terminal = owner
        .issue_stage8a4_terminal_authority(commitment_key, request_id)
        .map_err(|_| Stage8a4I4CompositionError::TerminalAuthorityUnavailable)?;
    let ack = terminal_ack_facts(&terminal)?;
    let readiness = issue_stage8a4_i4_current_readiness(
        &terminal,
        authority_root,
        accepted_config_sha256,
        composite_readiness,
        broker_truth,
        broker_readiness,
        now,
    )
    .ok();
    let readiness_state = if readiness.is_some() {
        Stage8a4I4ReadinessState::Ready
    } else {
        Stage8a4I4ReadinessState::Blocked
    };
    Ok(Stage8a4I4DerivedAckReadinessFacade {
        ack,
        readiness,
        readiness_state,
    })
}

fn terminal_ack_facts(
    terminal: &Stage7bStage8a4TerminalAuthority,
) -> Result<Stage8a4I4TerminalAckFacts, Stage8a4I4CompositionError> {
    let (status, reason_code) = canonical_ack_mapping(
        terminal.identity().action(),
        terminal.lifecycle(),
        terminal.cancel_outcome(),
        terminal.final_disposition(),
    )
    .ok_or(Stage8a4I4CompositionError::AckMappingUnavailable)?;
    Ok(Stage8a4I4TerminalAckFacts {
        strategy_request_id: terminal.identity().strategy_request_id(),
        durable_client_order_id: terminal.identity().durable_client_order_id().clone(),
        broker_order_id: terminal.broker_order_id().cloned(),
        status,
        reason_code: Some(reason_code),
        terminal_request_ack_identity_sha256: terminal
            .terminal_request_ack_identity_sha256()
            .to_string(),
    })
}

fn canonical_ack_mapping(
    action: Stage6DurableActionKind,
    lifecycle: Stage6ReconciliationLifecycleV2,
    cancel_outcome: Option<Stage6CancelOutcomeV1>,
    final_disposition: Stage6RequestFinalDispositionV1,
) -> Option<(CommandAckStatus, CommandAckReasonCode)> {
    use Stage6ReconciliationLifecycleV2 as Lifecycle;
    match (action, lifecycle, cancel_outcome, final_disposition) {
        (
            Stage6DurableActionKind::Place,
            Lifecycle::TerminalRejected,
            None,
            Stage6RequestFinalDispositionV1::Rejected,
        ) => Some((
            CommandAckStatus::Rejected,
            CommandAckReasonCode::BrokerRejected,
        )),
        (
            Stage6DurableActionKind::Place,
            Lifecycle::Working
            | Lifecycle::TerminalFilled
            | Lifecycle::TerminalCancelled
            | Lifecycle::TerminalExpired,
            None,
            Stage6RequestFinalDispositionV1::Completed,
        ) => Some((
            CommandAckStatus::Recovered,
            CommandAckReasonCode::RecoveredByBrokerTruth,
        )),
        (
            Stage6DurableActionKind::Cancel,
            Lifecycle::TerminalFilled,
            Some(Stage6CancelOutcomeV1::ExecutionObserved),
            Stage6RequestFinalDispositionV1::Completed,
        )
        | (
            Stage6DurableActionKind::Cancel,
            Lifecycle::TerminalRejected | Lifecycle::TerminalExpired,
            Some(Stage6CancelOutcomeV1::AlreadyTerminalNonExecution),
            Stage6RequestFinalDispositionV1::Completed,
        )
        | (
            Stage6DurableActionKind::Cancel,
            Lifecycle::TerminalCancelled,
            Some(Stage6CancelOutcomeV1::Canceled),
            Stage6RequestFinalDispositionV1::Completed,
        ) => Some((
            CommandAckStatus::Recovered,
            CommandAckReasonCode::RecoveredByBrokerTruth,
        )),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_place_and_cancel_mapping_is_closed() {
        assert_eq!(
            canonical_ack_mapping(
                Stage6DurableActionKind::Place,
                Stage6ReconciliationLifecycleV2::TerminalRejected,
                None,
                Stage6RequestFinalDispositionV1::Rejected,
            ),
            Some((
                CommandAckStatus::Rejected,
                CommandAckReasonCode::BrokerRejected
            ))
        );
        assert_eq!(
            canonical_ack_mapping(
                Stage6DurableActionKind::Cancel,
                Stage6ReconciliationLifecycleV2::TerminalCancelled,
                Some(Stage6CancelOutcomeV1::Canceled),
                Stage6RequestFinalDispositionV1::Completed,
            ),
            Some((
                CommandAckStatus::Recovered,
                CommandAckReasonCode::RecoveredByBrokerTruth,
            ))
        );
        assert_eq!(
            canonical_ack_mapping(
                Stage6DurableActionKind::Cancel,
                Stage6ReconciliationLifecycleV2::Working,
                None,
                Stage6RequestFinalDispositionV1::Completed,
            ),
            None
        );
    }

    #[test]
    fn mismatched_cancel_outcome_and_place_disposition_fail_closed() {
        assert_eq!(
            canonical_ack_mapping(
                Stage6DurableActionKind::Cancel,
                Stage6ReconciliationLifecycleV2::TerminalFilled,
                Some(Stage6CancelOutcomeV1::Canceled),
                Stage6RequestFinalDispositionV1::Completed,
            ),
            None
        );
        assert_eq!(
            canonical_ack_mapping(
                Stage6DurableActionKind::Place,
                Stage6ReconciliationLifecycleV2::TerminalFilled,
                None,
                Stage6RequestFinalDispositionV1::Rejected,
            ),
            None
        );
    }
}
