//! Stage 5G-f paper/mock MR protective completion.
//!
//! This module is deliberately broker-neutral and paper/mock only. It consumes
//! an already-restored lifecycle authority plus canonical broker/runtime
//! feedback and classifies whether a Mean Reversion take-profit/stop-loss leg
//! has completed only after matching execution evidence and complete flat
//! position truth converge.

use broker_core::{
    BrokerAccountId, BrokerOrderId, BrokerPositionSnapshot, BrokerStopOrderId,
    HybridRuntimeAttribution, HybridRuntimeOrderEvent, HybridRuntimeOrderRole, HybridRuntimeOwner,
    HybridRuntimePositionEvent, HybridRuntimeStopOrderEvent, InstrumentId,
};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::stage5g_order_position::stage5g_integral_lot_decimal;

pub const STAGE5G_PROTECTIVE_COMPLETION_SCHEMA_VERSION: u16 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Stage5gProtectiveScenarioId {
    Gprt01F12MrLongTargetCompletesFlat,
    Gprt02F13MrShortTargetCompletesFlat,
    Gprt03F14MrLongStopCompletesFlat,
    Gprt04F15MrShortStopCompletesFlat,
    Gprt05WrongOwnerOrCycleBlocks,
    Gprt06WrongInstrumentOrOrderIdBlocks,
    Gprt07TriggerWithoutFlatPositionBlocks,
    Gprt08NonExecutionTerminalCannotInventExit,
}

impl Stage5gProtectiveScenarioId {
    pub const ALL: [Stage5gProtectiveScenarioId; 8] = [
        Stage5gProtectiveScenarioId::Gprt01F12MrLongTargetCompletesFlat,
        Stage5gProtectiveScenarioId::Gprt02F13MrShortTargetCompletesFlat,
        Stage5gProtectiveScenarioId::Gprt03F14MrLongStopCompletesFlat,
        Stage5gProtectiveScenarioId::Gprt04F15MrShortStopCompletesFlat,
        Stage5gProtectiveScenarioId::Gprt05WrongOwnerOrCycleBlocks,
        Stage5gProtectiveScenarioId::Gprt06WrongInstrumentOrOrderIdBlocks,
        Stage5gProtectiveScenarioId::Gprt07TriggerWithoutFlatPositionBlocks,
        Stage5gProtectiveScenarioId::Gprt08NonExecutionTerminalCannotInventExit,
    ];

    pub fn as_id(self) -> &'static str {
        match self {
            Self::Gprt01F12MrLongTargetCompletesFlat => "GPRT01_F12_MR_LONG_TARGET_COMPLETES_FLAT",
            Self::Gprt02F13MrShortTargetCompletesFlat => {
                "GPRT02_F13_MR_SHORT_TARGET_COMPLETES_FLAT"
            }
            Self::Gprt03F14MrLongStopCompletesFlat => "GPRT03_F14_MR_LONG_STOP_COMPLETES_FLAT",
            Self::Gprt04F15MrShortStopCompletesFlat => "GPRT04_F15_MR_SHORT_STOP_COMPLETES_FLAT",
            Self::Gprt05WrongOwnerOrCycleBlocks => "GPRT05_WRONG_OWNER_OR_CYCLE_BLOCKS",
            Self::Gprt06WrongInstrumentOrOrderIdBlocks => {
                "GPRT06_WRONG_INSTRUMENT_OR_ORDER_ID_BLOCKS"
            }
            Self::Gprt07TriggerWithoutFlatPositionBlocks => {
                "GPRT07_TRIGGER_WITHOUT_FLAT_POSITION_BLOCKS"
            }
            Self::Gprt08NonExecutionTerminalCannotInventExit => {
                "GPRT08_NON_EXECUTION_TERMINAL_CANNOT_INVENT_EXIT"
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage5gProtectiveLeg {
    Target,
    Stop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage5gProtectedPositionSide {
    Long,
    Short,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage5gProtectiveDisposition {
    Completed,
    FlatCleanupPending,
    AwaitingPositionTruth,
    Blocked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage5gProtectiveBlockReason {
    EmptyStrategyId,
    EmptyCycleId,
    WrongOwner,
    MissingCycle,
    MissingProtectiveId,
    InvalidProtectedPositionQuantity,
    WrongCycle,
    WrongRole,
    AttributionMissing,
    AttributionConflict,
    AccountMismatch,
    InstrumentMismatch,
    TargetOrderIdMismatch,
    StopOrderIdMismatch,
    StopExchangeOrderIdMismatch,
    SideMismatch,
    QuantityMismatch,
    InvalidExecutionQuantity,
    ChronologyViolation,
    PositionTruthIncomplete,
    PositionNotFlat,
    NonExecutionTerminal,
    UnsupportedExecutionStatus,
    ConflictingDuplicateEvidence,
    MissingSiblingCleanupProof,
    SiblingCleanupOrderIdMismatch,
    SiblingCleanupAttributionMismatch,
    CanonicalCallbackFailed,
    MissingCleanRestartProtectiveState,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct Stage5gProtectiveCompletionAuthorityInput {
    pub strategy_id: String,
    pub account_id: BrokerAccountId,
    pub instrument: InstrumentId,
    pub tick_size: f64,
    pub current_owner: HybridRuntimeOwner,
    pub active_cycle_id: Option<String>,
    pub protected_position_side: Stage5gProtectedPositionSide,
    pub protected_position_qty: Decimal,
    pub tp_order_id: Option<BrokerOrderId>,
    pub sl_stop_order_id: Option<BrokerStopOrderId>,
    pub sl_exchange_order_id: Option<BrokerOrderId>,
    pub protective_created_ts_utc: i64,
    pub last_lifecycle_checkpoint_ts_utc: i64,
    pub operational_identity_commitment_sha256: String,
    pub restart_package_fingerprint_sha256: String,
    pub last_checkpoint_fingerprint_sha256: String,
}

pub struct Stage5gProtectiveCompletionAuthority {
    input: Stage5gProtectiveCompletionAuthorityInput,
    runtime: Option<crate::HybridIntradayRuntimeStrategy>,
    accepted_receipts: Vec<Stage5gProtectiveEvidenceReceipt>,
}

impl std::fmt::Debug for Stage5gProtectiveCompletionAuthority {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Stage5gProtectiveCompletionAuthority")
            .field("summary", &self.summary())
            .field("source_runtime_owned", &self.runtime.is_some())
            .finish_non_exhaustive()
    }
}

impl Stage5gProtectiveCompletionAuthority {
    pub fn strategy_id(&self) -> &str {
        &self.input.strategy_id
    }

    pub fn active_cycle_id(&self) -> &str {
        self.input
            .active_cycle_id
            .as_deref()
            .expect("authority admission guarantees active_cycle_id")
    }

    pub fn summary(&self) -> Stage5gProtectiveAuthoritySummary {
        Stage5gProtectiveAuthoritySummary {
            schema_version: STAGE5G_PROTECTIVE_COMPLETION_SCHEMA_VERSION,
            strategy_id: self.input.strategy_id.clone(),
            account_id: self.input.account_id.clone(),
            instrument: self.input.instrument.clone(),
            tick_size: self.input.tick_size,
            active_cycle_id: self.active_cycle_id().to_string(),
            protected_position_side: self.input.protected_position_side,
            protected_position_qty: self.input.protected_position_qty,
            tp_order_id: self.input.tp_order_id.clone(),
            sl_stop_order_id: self.input.sl_stop_order_id.clone(),
            sl_exchange_order_id: self.input.sl_exchange_order_id.clone(),
            protective_created_ts_utc: self.input.protective_created_ts_utc,
            last_lifecycle_checkpoint_ts_utc: self.input.last_lifecycle_checkpoint_ts_utc,
            accepted_receipt_count: self.accepted_receipts.len(),
            authority_fingerprint_sha256: semantic_sha256(&self.input),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Stage5gProtectiveAuthoritySummary {
    pub schema_version: u16,
    pub strategy_id: String,
    pub account_id: BrokerAccountId,
    pub instrument: InstrumentId,
    pub tick_size: f64,
    pub active_cycle_id: String,
    pub protected_position_side: Stage5gProtectedPositionSide,
    pub protected_position_qty: Decimal,
    pub tp_order_id: Option<BrokerOrderId>,
    pub sl_stop_order_id: Option<BrokerStopOrderId>,
    pub sl_exchange_order_id: Option<BrokerOrderId>,
    pub protective_created_ts_utc: i64,
    pub last_lifecycle_checkpoint_ts_utc: i64,
    pub accepted_receipt_count: usize,
    pub authority_fingerprint_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Stage5gProtectiveCompletionEvidence {
    pub observed_account_id: BrokerAccountId,
    pub execution: Stage5gProtectiveExecutionEvidence,
    pub position_truth: Stage5gProtectivePositionTruth,
    pub sibling_cleanup: Option<Stage5gProtectiveSiblingCleanupEvidence>,
    pub sibling_terminal: Option<Stage5gProtectiveSiblingTerminalEvidence>,
}

pub struct Stage5gValidatedProtectiveEvidence {
    evidence: Stage5gProtectiveCompletionEvidence,
    evidence_fingerprint_sha256: String,
}

impl std::fmt::Debug for Stage5gValidatedProtectiveEvidence {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Stage5gValidatedProtectiveEvidence")
            .field(
                "evidence_fingerprint_sha256",
                &self.evidence_fingerprint_sha256,
            )
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Stage5gProtectiveExecutionEvidence {
    TargetOrder(HybridRuntimeOrderEvent),
    StopOrder(HybridRuntimeStopOrderEvent),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Stage5gProtectivePositionTruth {
    pub positions_complete: bool,
    pub positions: Vec<BrokerPositionSnapshot>,
    pub received_ts_utc: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Stage5gProtectiveCleanupEscrowProof {
    lifecycle_receipt_fingerprint_sha256: String,
    accepted_cleanup_intent_fingerprint_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Stage5gProtectiveSiblingCleanupEvidence {
    pub cleanup_order_id: BrokerOrderId,
    pub attribution: HybridRuntimeAttribution,
    pub paper_lifecycle_escrow: Stage5gProtectiveCleanupEscrowProof,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stage5gProtectiveSiblingTerminalEvidence {
    pub sibling_order_id: BrokerOrderId,
    pub terminal_status: String,
    pub terminal_receipt_fingerprint_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stage5gProtectiveEvidenceReceipt {
    identity: String,
    fingerprint_sha256: String,
    disposition: Stage5gProtectiveDisposition,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stage5gProtectivePostStateSummary {
    pub final_owner: Option<HybridRuntimeOwner>,
    pub final_cycle_id: Option<String>,
    pub final_position_qty: Decimal,
    pub post_callback_state_fingerprint_sha256: String,
}

pub struct Stage5gProtectiveCommittedState {
    runtime: crate::HybridIntradayRuntimeStrategy,
    summary: Stage5gProtectivePostStateSummary,
}

impl std::fmt::Debug for Stage5gProtectiveCommittedState {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let runtime_fingerprint = self
            .runtime
            .stage5g_protective_completion_post_callback_summary()
            .3;
        formatter
            .debug_struct("Stage5gProtectiveCommittedState")
            .field("summary", &self.summary)
            .field("runtime_fingerprint_sha256", &runtime_fingerprint)
            .field("redis_command_stream_attached", &false)
            .field("finam_transport_attached", &false)
            .field("runtime_live_attached", &false)
            .finish_non_exhaustive()
    }
}

impl Stage5gProtectiveCommittedState {
    fn new(runtime: crate::HybridIntradayRuntimeStrategy) -> Self {
        let (
            final_owner,
            final_cycle_id,
            final_position_qty,
            post_callback_state_fingerprint_sha256,
        ) = runtime.stage5g_protective_completion_post_callback_summary();
        Self {
            runtime,
            summary: Stage5gProtectivePostStateSummary {
                final_owner,
                final_cycle_id,
                final_position_qty,
                post_callback_state_fingerprint_sha256,
            },
        }
    }

    pub fn summary(&self) -> &Stage5gProtectivePostStateSummary {
        &self.summary
    }

    pub fn runtime_live_attached(&self) -> bool {
        false
    }

    pub fn redis_command_stream_attached(&self) -> bool {
        false
    }

    pub fn finam_transport_attached(&self) -> bool {
        false
    }
}

pub struct Stage5gProtectiveCompleted {
    pub scenario: Stage5gProtectiveScenarioId,
    pub leg: Stage5gProtectiveLeg,
    pub authority_summary: Stage5gProtectiveAuthoritySummary,
    pub execution_receipt: Stage5gProtectiveEvidenceReceipt,
    pub final_owner: Option<HybridRuntimeOwner>,
    pub final_cycle_id: Option<String>,
    pub final_position_qty: Decimal,
    pub callback_count: usize,
    pub bridge_post_state_fingerprint_sha256: String,
    pub post_callback_state_fingerprint_sha256: String,
    pub completion_fingerprint_sha256: String,
    post_state: Stage5gProtectiveCommittedState,
}

impl std::fmt::Debug for Stage5gProtectiveCompleted {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Stage5gProtectiveCompleted")
            .field("scenario", &self.scenario)
            .field("leg", &self.leg)
            .field("authority_summary", &self.authority_summary)
            .field("execution_receipt", &self.execution_receipt)
            .field("final_owner", &self.final_owner)
            .field("final_cycle_id", &self.final_cycle_id)
            .field("final_position_qty", &self.final_position_qty)
            .field("callback_count", &self.callback_count)
            .field(
                "bridge_post_state_fingerprint_sha256",
                &self.bridge_post_state_fingerprint_sha256,
            )
            .field(
                "post_callback_state_fingerprint_sha256",
                &self.post_callback_state_fingerprint_sha256,
            )
            .field(
                "completion_fingerprint_sha256",
                &self.completion_fingerprint_sha256,
            )
            .finish_non_exhaustive()
    }
}

impl Stage5gProtectiveCompleted {
    pub fn post_state(&self) -> &Stage5gProtectiveCommittedState {
        &self.post_state
    }
}

pub struct Stage5gProtectiveFlatCleanupPending {
    pub scenario: Stage5gProtectiveScenarioId,
    pub leg: Stage5gProtectiveLeg,
    pub authority_summary: Stage5gProtectiveAuthoritySummary,
    pub execution_receipt: Stage5gProtectiveEvidenceReceipt,
    pub generated_cleanup_batch_summary: crate::Stage5cPaperIntentBatchSummary,
    pub settled_batch_history: Vec<crate::Stage5cPaperIntentBatchSummary>,
    pub final_owner: Option<HybridRuntimeOwner>,
    pub final_cycle_id: Option<String>,
    pub final_position_qty: Decimal,
    pub callback_count: usize,
    pub bridge_post_state_fingerprint_sha256: String,
    pub post_callback_state_fingerprint_sha256: String,
    pub cleanup_pending_fingerprint_sha256: String,
    post_state: Stage5gProtectiveCommittedState,
    generated_cleanup_batch: crate::Stage5cPaperIntentBatch,
}

impl std::fmt::Debug for Stage5gProtectiveFlatCleanupPending {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Stage5gProtectiveFlatCleanupPending")
            .field("scenario", &self.scenario)
            .field("leg", &self.leg)
            .field("authority_summary", &self.authority_summary)
            .field("execution_receipt", &self.execution_receipt)
            .field(
                "generated_cleanup_batch_summary",
                &self.generated_cleanup_batch_summary,
            )
            .field("settled_batch_history", &self.settled_batch_history)
            .field("final_owner", &self.final_owner)
            .field("final_cycle_id", &self.final_cycle_id)
            .field("final_position_qty", &self.final_position_qty)
            .field("callback_count", &self.callback_count)
            .field(
                "bridge_post_state_fingerprint_sha256",
                &self.bridge_post_state_fingerprint_sha256,
            )
            .field(
                "post_callback_state_fingerprint_sha256",
                &self.post_callback_state_fingerprint_sha256,
            )
            .field(
                "cleanup_pending_fingerprint_sha256",
                &self.cleanup_pending_fingerprint_sha256,
            )
            .finish_non_exhaustive()
    }
}

impl Stage5gProtectiveFlatCleanupPending {
    pub fn post_state(&self) -> &Stage5gProtectiveCommittedState {
        &self.post_state
    }

    pub fn generated_cleanup_batch(&self) -> &crate::Stage5cPaperIntentBatch {
        &self.generated_cleanup_batch
    }
}

#[derive(Debug)]
pub struct Stage5gProtectiveAwaitingPositionTruth {
    pub scenario: Stage5gProtectiveScenarioId,
    pub leg: Stage5gProtectiveLeg,
    pub authority: Stage5gProtectiveCompletionAuthority,
    pub execution_receipt: Stage5gProtectiveEvidenceReceipt,
    pub reason: Stage5gProtectiveBlockReason,
}

#[derive(Debug)]
pub struct Stage5gProtectiveBlocked {
    pub scenario: Stage5gProtectiveScenarioId,
    pub authority: Stage5gProtectiveCompletionAuthority,
    pub reason: Stage5gProtectiveBlockReason,
}

#[derive(Debug)]
pub enum Stage5gProtectiveCompletionTransition {
    Completed(Box<Stage5gProtectiveCompleted>),
    FlatCleanupPending(Box<Stage5gProtectiveFlatCleanupPending>),
    AwaitingPositionTruth(Stage5gProtectiveAwaitingPositionTruth),
    Blocked(Stage5gProtectiveBlocked),
}

impl Stage5gProtectiveCompletionTransition {
    pub fn disposition(&self) -> Stage5gProtectiveDisposition {
        match self {
            Self::Completed(_) => Stage5gProtectiveDisposition::Completed,
            Self::FlatCleanupPending(_) => Stage5gProtectiveDisposition::FlatCleanupPending,
            Self::AwaitingPositionTruth(_) => Stage5gProtectiveDisposition::AwaitingPositionTruth,
            Self::Blocked(_) => Stage5gProtectiveDisposition::Blocked,
        }
    }

    pub fn scenario(&self) -> Stage5gProtectiveScenarioId {
        match self {
            Self::Completed(completed) => completed.scenario,
            Self::FlatCleanupPending(pending) => pending.scenario,
            Self::AwaitingPositionTruth(awaiting) => awaiting.scenario,
            Self::Blocked(blocked) => blocked.scenario,
        }
    }

    pub fn semantic_fingerprint_sha256(&self) -> String {
        match self {
            Self::Completed(completed) => semantic_sha256(&completed_projection(completed)),
            Self::FlatCleanupPending(pending) => {
                semantic_sha256(&flat_cleanup_pending_projection(pending))
            }
            Self::AwaitingPositionTruth(awaiting) => semantic_sha256(&(
                awaiting.scenario,
                awaiting.leg,
                awaiting.authority.summary(),
                &awaiting.execution_receipt,
                awaiting.reason,
            )),
            Self::Blocked(blocked) => semantic_sha256(&(
                blocked.scenario,
                blocked.authority.summary(),
                blocked.reason,
            )),
        }
    }
}

pub fn prepare_stage5g_protective_completion(
    restart: crate::Stage5gCleanRestartedCapability,
) -> Result<Stage5gProtectiveCompletionAuthority, Stage5gProtectiveBlockReason> {
    let (runtime, input) = restart
        .into_stage5g_protective_completion_authority_input()
        .ok_or(Stage5gProtectiveBlockReason::MissingCleanRestartProtectiveState)?;
    admit_stage5g_protective_completion_authority_from_source(input, Some(runtime))
}

#[cfg(test)]
pub(crate) fn admit_stage5g_protective_completion_authority(
    input: Stage5gProtectiveCompletionAuthorityInput,
) -> Result<Stage5gProtectiveCompletionAuthority, Stage5gProtectiveBlockReason> {
    admit_stage5g_protective_completion_authority_from_source(input, None)
}

fn admit_stage5g_protective_completion_authority_from_source(
    input: Stage5gProtectiveCompletionAuthorityInput,
    runtime: Option<crate::HybridIntradayRuntimeStrategy>,
) -> Result<Stage5gProtectiveCompletionAuthority, Stage5gProtectiveBlockReason> {
    if input.strategy_id.is_empty() {
        return Err(Stage5gProtectiveBlockReason::EmptyStrategyId);
    }
    if input.current_owner != HybridRuntimeOwner::MeanReversion {
        return Err(Stage5gProtectiveBlockReason::WrongOwner);
    }
    if input
        .active_cycle_id
        .as_deref()
        .unwrap_or_default()
        .is_empty()
    {
        return Err(Stage5gProtectiveBlockReason::MissingCycle);
    }
    if input.protected_position_qty <= Decimal::ZERO {
        return Err(Stage5gProtectiveBlockReason::InvalidProtectedPositionQuantity);
    }
    if input.tp_order_id.is_none() || input.sl_stop_order_id.is_none() {
        return Err(Stage5gProtectiveBlockReason::MissingProtectiveId);
    }
    Ok(Stage5gProtectiveCompletionAuthority {
        input,
        runtime,
        accepted_receipts: Vec::new(),
    })
}

pub fn validate_stage5g_protective_completion_evidence(
    authority: &Stage5gProtectiveCompletionAuthority,
    evidence: Stage5gProtectiveCompletionEvidence,
) -> Result<Stage5gValidatedProtectiveEvidence, Stage5gProtectiveBlockReason> {
    validate_evidence(authority, &evidence)?;
    validate_preexisting_sibling_terminal(authority, &evidence)?;
    Ok(Stage5gValidatedProtectiveEvidence {
        evidence_fingerprint_sha256: semantic_sha256(&evidence),
        evidence,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Stage5gProtectiveReplayClassification {
    New,
    ExactReplay(Stage5gProtectiveEvidenceReceipt),
    FingerprintConflict,
}

pub fn apply_stage5g_protective_completion(
    authority: Stage5gProtectiveCompletionAuthority,
    validated: Stage5gValidatedProtectiveEvidence,
) -> Stage5gProtectiveCompletionTransition {
    let evidence = validated.evidence;
    let base_scenario = base_scenario_for(&authority, &evidence);
    let (leg, event_ts) = match execution_metadata(&evidence.execution) {
        Ok(value) => value,
        Err(reason) => {
            return Stage5gProtectiveCompletionTransition::Blocked(Stage5gProtectiveBlocked {
                scenario: scenario_for_reason(base_scenario, reason),
                authority,
                reason,
            });
        }
    };
    let replay = classify_replay(&authority, &evidence);
    if replay == Stage5gProtectiveReplayClassification::FingerprintConflict {
        let reason = Stage5gProtectiveBlockReason::ConflictingDuplicateEvidence;
        return Stage5gProtectiveCompletionTransition::Blocked(Stage5gProtectiveBlocked {
            scenario: scenario_for_reason(base_scenario, reason),
            authority,
            reason,
        });
    }
    let replay_should_append = matches!(replay, Stage5gProtectiveReplayClassification::New);
    let receipt = match replay {
        Stage5gProtectiveReplayClassification::New => Stage5gProtectiveEvidenceReceipt {
            identity: evidence_identity(&authority, &evidence),
            fingerprint_sha256: semantic_sha256(&evidence),
            disposition: Stage5gProtectiveDisposition::AwaitingPositionTruth,
        },
        Stage5gProtectiveReplayClassification::ExactReplay(receipt) => receipt,
        Stage5gProtectiveReplayClassification::FingerprintConflict => unreachable!(),
    };

    if let Err(reason) = position_truth_is_flat(&authority, &evidence.position_truth, event_ts) {
        return block_or_await(
            authority,
            scenario_for_reason(base_scenario, reason),
            leg,
            reason,
            Some(receipt),
            replay_should_append,
        );
    }

    let mut completed_receipt = receipt;
    completed_receipt.disposition = Stage5gProtectiveDisposition::Completed;
    let callback =
        match apply_stage5c_owned_protective_lifecycle_bridge(authority, &evidence, event_ts) {
            Ok(callback) => callback,
            Err(blocked) => {
                let (authority, reason) = *blocked;
                return Stage5gProtectiveCompletionTransition::Blocked(Stage5gProtectiveBlocked {
                    scenario: scenario_for_reason(base_scenario, reason),
                    authority,
                    reason,
                });
            }
        };
    let post_state_summary = callback.post_state.summary().clone();
    let authority_summary = callback.authority_summary.clone();
    if let (Some(generated_cleanup_batch), Some(generated_cleanup_batch_summary)) = (
        callback.generated_cleanup_batch,
        callback.generated_cleanup_batch_summary,
    ) {
        let cleanup_pending_fingerprint_sha256 = semantic_sha256(&(
            base_scenario,
            leg,
            &authority_summary,
            &completed_receipt,
            &generated_cleanup_batch_summary,
            &callback.settled_batch_history,
            post_state_summary.final_owner,
            &post_state_summary.final_cycle_id,
            post_state_summary.final_position_qty,
            callback.callback_count,
            &callback.bridge_post_state_fingerprint_sha256,
            &post_state_summary.post_callback_state_fingerprint_sha256,
        ));
        let pending = Stage5gProtectiveFlatCleanupPending {
            scenario: base_scenario,
            leg,
            authority_summary,
            execution_receipt: completed_receipt,
            generated_cleanup_batch_summary,
            settled_batch_history: callback.settled_batch_history,
            final_owner: post_state_summary.final_owner,
            final_cycle_id: post_state_summary.final_cycle_id,
            final_position_qty: post_state_summary.final_position_qty,
            callback_count: callback.callback_count,
            bridge_post_state_fingerprint_sha256: callback.bridge_post_state_fingerprint_sha256,
            post_callback_state_fingerprint_sha256: post_state_summary
                .post_callback_state_fingerprint_sha256,
            cleanup_pending_fingerprint_sha256,
            post_state: callback.post_state,
            generated_cleanup_batch,
        };
        return Stage5gProtectiveCompletionTransition::FlatCleanupPending(Box::new(pending));
    }
    let completion_fingerprint_sha256 = semantic_sha256(&(
        base_scenario,
        leg,
        &authority_summary,
        &completed_receipt,
        post_state_summary.final_owner,
        &post_state_summary.final_cycle_id,
        post_state_summary.final_position_qty,
        callback.callback_count,
        &callback.bridge_post_state_fingerprint_sha256,
        &post_state_summary.post_callback_state_fingerprint_sha256,
    ));
    let completed = Stage5gProtectiveCompleted {
        scenario: base_scenario,
        leg,
        authority_summary,
        execution_receipt: completed_receipt,
        final_owner: post_state_summary.final_owner,
        final_cycle_id: post_state_summary.final_cycle_id,
        final_position_qty: post_state_summary.final_position_qty,
        callback_count: callback.callback_count,
        bridge_post_state_fingerprint_sha256: callback.bridge_post_state_fingerprint_sha256,
        post_callback_state_fingerprint_sha256: post_state_summary
            .post_callback_state_fingerprint_sha256,
        completion_fingerprint_sha256,
        post_state: callback.post_state,
    };
    Stage5gProtectiveCompletionTransition::Completed(Box::new(completed))
}

fn block_or_await(
    mut authority: Stage5gProtectiveCompletionAuthority,
    scenario: Stage5gProtectiveScenarioId,
    leg: Stage5gProtectiveLeg,
    reason: Stage5gProtectiveBlockReason,
    receipt: Option<Stage5gProtectiveEvidenceReceipt>,
    append_receipt: bool,
) -> Stage5gProtectiveCompletionTransition {
    match reason {
        Stage5gProtectiveBlockReason::PositionTruthIncomplete
        | Stage5gProtectiveBlockReason::PositionNotFlat => {
            let mut execution_receipt = receipt.expect("await requires accepted execution receipt");
            execution_receipt.disposition = Stage5gProtectiveDisposition::AwaitingPositionTruth;
            if append_receipt {
                authority.accepted_receipts.push(execution_receipt.clone());
            }
            Stage5gProtectiveCompletionTransition::AwaitingPositionTruth(
                Stage5gProtectiveAwaitingPositionTruth {
                    scenario,
                    leg,
                    authority,
                    execution_receipt,
                    reason,
                },
            )
        }
        _ => Stage5gProtectiveCompletionTransition::Blocked(Stage5gProtectiveBlocked {
            scenario,
            authority,
            reason,
        }),
    }
}

struct Stage5gProtectiveCallbackOutcome {
    authority_summary: Stage5gProtectiveAuthoritySummary,
    post_state: Stage5gProtectiveCommittedState,
    generated_cleanup_batch: Option<crate::Stage5cPaperIntentBatch>,
    generated_cleanup_batch_summary: Option<crate::Stage5cPaperIntentBatchSummary>,
    settled_batch_history: Vec<crate::Stage5cPaperIntentBatchSummary>,
    callback_count: usize,
    bridge_post_state_fingerprint_sha256: String,
}

fn apply_stage5c_owned_protective_lifecycle_bridge(
    authority: Stage5gProtectiveCompletionAuthority,
    evidence: &Stage5gProtectiveCompletionEvidence,
    event_ts: i64,
) -> Result<
    Stage5gProtectiveCallbackOutcome,
    Box<(
        Stage5gProtectiveCompletionAuthority,
        Stage5gProtectiveBlockReason,
    )>,
> {
    let Some(runtime) = authority.runtime.as_ref() else {
        return Err(Box::new((
            authority,
            Stage5gProtectiveBlockReason::CanonicalCallbackFailed,
        )));
    };
    let candidate = runtime.clone();
    let pre_position_qty = match authority.input.protected_position_side {
        Stage5gProtectedPositionSide::Long => authority
            .input
            .protected_position_qty
            .to_string()
            .parse()
            .unwrap_or(0.0),
        Stage5gProtectedPositionSide::Short => -authority
            .input
            .protected_position_qty
            .to_string()
            .parse::<f64>()
            .unwrap_or(0.0),
    };
    let execution = match &evidence.execution {
        Stage5gProtectiveExecutionEvidence::TargetOrder(order) => {
            crate::stage5c_paper_host::Stage5gProtectiveBrokerLifecycleExecution::Order(
                order.clone(),
            )
        }
        Stage5gProtectiveExecutionEvidence::StopOrder(order) => {
            crate::stage5c_paper_host::Stage5gProtectiveBrokerLifecycleExecution::StopOrder(
                order.clone(),
            )
        }
    };
    let bridge_result =
        crate::stage5c_paper_host::resolve_stage5g_protective_broker_lifecycle_bridge(
            candidate,
            crate::stage5c_paper_host::Stage5gProtectiveBrokerLifecycleBridgeInput {
                strategy_id: authority.input.strategy_id.clone(),
                account_id: authority.input.account_id.clone(),
                instrument: authority.input.instrument.clone(),
                tick_size: authority.input.tick_size,
                pre_position_qty,
                execution,
                position: HybridRuntimePositionEvent {
                    instrument: authority.input.instrument.clone(),
                    qty: 0.0,
                    existing: false,
                    avg_price: 0.0,
                    source_ts_utc: evidence.position_truth.received_ts_utc.max(event_ts),
                },
            },
        );
    let bridge = match bridge_result {
        Ok(bridge) => bridge,
        Err(_) => {
            return Err(Box::new((
                authority,
                Stage5gProtectiveBlockReason::CanonicalCallbackFailed,
            )));
        }
    };
    let authority_summary = authority.summary();
    let post_state = Stage5gProtectiveCommittedState::new(bridge.strategy);
    Ok(Stage5gProtectiveCallbackOutcome {
        authority_summary,
        post_state,
        generated_cleanup_batch: bridge.generated_intent_batch,
        generated_cleanup_batch_summary: bridge.generated_intent_batch_summary,
        settled_batch_history: bridge.settled_batch_history,
        callback_count: bridge.callback_count,
        bridge_post_state_fingerprint_sha256: bridge.post_state_fingerprint_sha256,
    })
}

fn validate_evidence(
    authority: &Stage5gProtectiveCompletionAuthority,
    evidence: &Stage5gProtectiveCompletionEvidence,
) -> Result<(), Stage5gProtectiveBlockReason> {
    if evidence.observed_account_id != authority.input.account_id {
        return Err(Stage5gProtectiveBlockReason::AccountMismatch);
    }
    let (leg, event_ts) = execution_metadata(&evidence.execution)?;
    if event_ts < authority.input.protective_created_ts_utc
        || event_ts < authority.input.last_lifecycle_checkpoint_ts_utc
    {
        return Err(Stage5gProtectiveBlockReason::ChronologyViolation);
    }
    match &evidence.execution {
        Stage5gProtectiveExecutionEvidence::TargetOrder(order) => {
            validate_target_order(authority, order)?;
        }
        Stage5gProtectiveExecutionEvidence::StopOrder(order) => {
            validate_stop_order(authority, order)?;
        }
    }
    if !execution_status_is_supported(leg, &evidence.execution) {
        return Err(status_block_reason(&evidence.execution));
    }
    Ok(())
}

fn validate_target_order(
    authority: &Stage5gProtectiveCompletionAuthority,
    order: &HybridRuntimeOrderEvent,
) -> Result<(), Stage5gProtectiveBlockReason> {
    if order.instrument != authority.input.instrument {
        return Err(Stage5gProtectiveBlockReason::InstrumentMismatch);
    }
    if Some(&order.order_id) != authority.input.tp_order_id.as_ref() {
        return Err(Stage5gProtectiveBlockReason::TargetOrderIdMismatch);
    }
    validate_attribution(
        authority,
        order.attribution.as_ref(),
        HybridRuntimeOrderRole::TakeProfit,
    )?;
    validate_side_and_quantity(authority, &order.side, order.qty, order.filled_qty)
}

fn validate_stop_order(
    authority: &Stage5gProtectiveCompletionAuthority,
    order: &HybridRuntimeStopOrderEvent,
) -> Result<(), Stage5gProtectiveBlockReason> {
    if order.instrument != authority.input.instrument {
        return Err(Stage5gProtectiveBlockReason::InstrumentMismatch);
    }
    if Some(&order.stop_order_id) != authority.input.sl_stop_order_id.as_ref() {
        return Err(Stage5gProtectiveBlockReason::StopOrderIdMismatch);
    }
    if let Some(expected_exchange_order_id) = authority.input.sl_exchange_order_id.as_ref() {
        if order.exchange_order_id.as_ref() != Some(expected_exchange_order_id) {
            return Err(Stage5gProtectiveBlockReason::StopExchangeOrderIdMismatch);
        }
    }
    validate_attribution(
        authority,
        order.attribution.as_ref(),
        HybridRuntimeOrderRole::StopLoss,
    )?;
    validate_side_and_quantity(authority, &order.side, order.qty, order.filled_qty)
}

fn validate_attribution(
    authority: &Stage5gProtectiveCompletionAuthority,
    attribution: Option<&HybridRuntimeAttribution>,
    expected_role: HybridRuntimeOrderRole,
) -> Result<(), Stage5gProtectiveBlockReason> {
    let attribution = attribution.ok_or(Stage5gProtectiveBlockReason::AttributionMissing)?;
    attribution
        .validate_source_equivalence()
        .map_err(|_| Stage5gProtectiveBlockReason::AttributionConflict)?;
    if !attribution.belongs_to(&authority.input.strategy_id) {
        return Err(Stage5gProtectiveBlockReason::AttributionConflict);
    }
    if attribution.owner() != Some(HybridRuntimeOwner::MeanReversion) {
        return Err(Stage5gProtectiveBlockReason::WrongOwner);
    }
    if attribution.role() != Some(expected_role) {
        return Err(Stage5gProtectiveBlockReason::WrongRole);
    }
    if attribution.cycle_id() != authority.active_cycle_id() {
        return Err(Stage5gProtectiveBlockReason::WrongCycle);
    }
    Ok(())
}

fn validate_side_and_quantity(
    authority: &Stage5gProtectiveCompletionAuthority,
    side: &str,
    qty: f64,
    filled_qty: f64,
) -> Result<(), Stage5gProtectiveBlockReason> {
    if normalize_side(side) != expected_exit_side(authority.input.protected_position_side) {
        return Err(Stage5gProtectiveBlockReason::SideMismatch);
    }
    let qty = stage5g_integral_lot_decimal(qty)
        .ok_or(Stage5gProtectiveBlockReason::InvalidExecutionQuantity)?;
    let filled_qty = stage5g_integral_lot_decimal(filled_qty)
        .ok_or(Stage5gProtectiveBlockReason::InvalidExecutionQuantity)?;
    if qty <= Decimal::ZERO || filled_qty <= Decimal::ZERO {
        return Err(Stage5gProtectiveBlockReason::InvalidExecutionQuantity);
    }
    if qty != authority.input.protected_position_qty
        || filled_qty != authority.input.protected_position_qty
    {
        return Err(Stage5gProtectiveBlockReason::QuantityMismatch);
    }
    Ok(())
}

fn position_truth_is_flat(
    authority: &Stage5gProtectiveCompletionAuthority,
    truth: &Stage5gProtectivePositionTruth,
    event_ts_utc: i64,
) -> Result<(), Stage5gProtectiveBlockReason> {
    if !truth.positions_complete {
        return Err(Stage5gProtectiveBlockReason::PositionTruthIncomplete);
    }
    if truth.received_ts_utc < event_ts_utc {
        return Err(Stage5gProtectiveBlockReason::ChronologyViolation);
    }
    let mut target_position: Option<&BrokerPositionSnapshot> = None;
    for position in &truth.positions {
        if position.account_id != authority.input.account_id {
            return Err(Stage5gProtectiveBlockReason::AccountMismatch);
        }
        if position
            .source_ts
            .map(|source_ts| source_ts.timestamp() < event_ts_utc)
            .unwrap_or(false)
        {
            return Err(Stage5gProtectiveBlockReason::ChronologyViolation);
        }
        if position.instrument == authority.input.instrument {
            if target_position.is_some() {
                return Err(Stage5gProtectiveBlockReason::PositionNotFlat);
            }
            target_position = Some(position);
        }
    }
    let Some(position) = target_position else {
        return Ok(());
    };
    if position.qty != Decimal::ZERO {
        return Err(Stage5gProtectiveBlockReason::PositionNotFlat);
    }
    Ok(())
}

fn validate_preexisting_sibling_terminal(
    authority: &Stage5gProtectiveCompletionAuthority,
    evidence: &Stage5gProtectiveCompletionEvidence,
) -> Result<(), Stage5gProtectiveBlockReason> {
    if evidence.sibling_cleanup.is_some() && evidence.sibling_terminal.is_some() {
        return Err(Stage5gProtectiveBlockReason::SiblingCleanupAttributionMismatch);
    }
    if evidence.sibling_cleanup.is_some() {
        return Err(Stage5gProtectiveBlockReason::MissingSiblingCleanupProof);
    }
    let (leg, _) = execution_metadata(&evidence.execution)?;
    if let Some(terminal) = &evidence.sibling_terminal {
        if !terminal_status_is_safe(&terminal.terminal_status)
            || !sha256_like(&terminal.terminal_receipt_fingerprint_sha256)
        {
            return Err(Stage5gProtectiveBlockReason::MissingSiblingCleanupProof);
        }
        if !sibling_order_id_matches(authority, leg, &terminal.sibling_order_id) {
            return Err(Stage5gProtectiveBlockReason::SiblingCleanupOrderIdMismatch);
        }
        return Ok(());
    }
    Ok(())
}

fn sibling_order_id_matches(
    authority: &Stage5gProtectiveCompletionAuthority,
    completed_leg: Stage5gProtectiveLeg,
    observed: &BrokerOrderId,
) -> bool {
    match completed_leg {
        Stage5gProtectiveLeg::Target => authority
            .input
            .sl_exchange_order_id
            .as_ref()
            .is_some_and(|expected| expected == observed),
        Stage5gProtectiveLeg::Stop => authority
            .input
            .tp_order_id
            .as_ref()
            .is_some_and(|expected| expected == observed),
    }
}

fn terminal_status_is_safe(status: &str) -> bool {
    matches!(
        normalize_status(status).as_str(),
        "canceled"
            | "cancelled"
            | "expired"
            | "rejected"
            | "filled"
            | "executed"
            | "done"
            | "completed"
    )
}

fn execution_metadata(
    execution: &Stage5gProtectiveExecutionEvidence,
) -> Result<(Stage5gProtectiveLeg, i64), Stage5gProtectiveBlockReason> {
    match execution {
        Stage5gProtectiveExecutionEvidence::TargetOrder(order) => {
            Ok((Stage5gProtectiveLeg::Target, order.source_ts_utc))
        }
        Stage5gProtectiveExecutionEvidence::StopOrder(order) => {
            Ok((Stage5gProtectiveLeg::Stop, order.source_ts_utc))
        }
    }
}

fn execution_status_is_supported(
    leg: Stage5gProtectiveLeg,
    execution: &Stage5gProtectiveExecutionEvidence,
) -> bool {
    match (leg, execution) {
        (Stage5gProtectiveLeg::Target, Stage5gProtectiveExecutionEvidence::TargetOrder(order)) => {
            normalize_status(&order.status) == "filled"
        }
        (Stage5gProtectiveLeg::Stop, Stage5gProtectiveExecutionEvidence::StopOrder(order)) => {
            matches!(
                normalize_status(&order.status).as_str(),
                "filled" | "executed" | "triggered" | "done" | "completed"
            )
        }
        _ => false,
    }
}

fn status_block_reason(
    execution: &Stage5gProtectiveExecutionEvidence,
) -> Stage5gProtectiveBlockReason {
    let status = match execution {
        Stage5gProtectiveExecutionEvidence::TargetOrder(order) => normalize_status(&order.status),
        Stage5gProtectiveExecutionEvidence::StopOrder(order) => normalize_status(&order.status),
    };
    if matches!(
        status.as_str(),
        "canceled" | "cancelled" | "expired" | "rejected"
    ) {
        Stage5gProtectiveBlockReason::NonExecutionTerminal
    } else {
        Stage5gProtectiveBlockReason::UnsupportedExecutionStatus
    }
}

fn classify_replay(
    authority: &Stage5gProtectiveCompletionAuthority,
    evidence: &Stage5gProtectiveCompletionEvidence,
) -> Stage5gProtectiveReplayClassification {
    let identity = evidence_identity(authority, evidence);
    let fingerprint = semantic_sha256(evidence);
    match authority
        .accepted_receipts
        .iter()
        .find(|receipt| receipt.identity == identity)
    {
        Some(receipt) if receipt.fingerprint_sha256 == fingerprint => {
            Stage5gProtectiveReplayClassification::ExactReplay(receipt.clone())
        }
        Some(_) => Stage5gProtectiveReplayClassification::FingerprintConflict,
        None => Stage5gProtectiveReplayClassification::New,
    }
}

fn evidence_identity(
    authority: &Stage5gProtectiveCompletionAuthority,
    evidence: &Stage5gProtectiveCompletionEvidence,
) -> String {
    match &evidence.execution {
        Stage5gProtectiveExecutionEvidence::TargetOrder(order) => format!(
            "stage5g-f:{}:{}:target:{}",
            authority.strategy_id(),
            authority.active_cycle_id(),
            order.order_id.as_str()
        ),
        Stage5gProtectiveExecutionEvidence::StopOrder(order) => format!(
            "stage5g-f:{}:{}:stop:{}",
            authority.strategy_id(),
            authority.active_cycle_id(),
            order.stop_order_id.as_str()
        ),
    }
}

fn base_scenario_for(
    authority: &Stage5gProtectiveCompletionAuthority,
    evidence: &Stage5gProtectiveCompletionEvidence,
) -> Stage5gProtectiveScenarioId {
    match &evidence.execution {
        Stage5gProtectiveExecutionEvidence::TargetOrder(_) => {
            match authority.input.protected_position_side {
                Stage5gProtectedPositionSide::Long => {
                    Stage5gProtectiveScenarioId::Gprt01F12MrLongTargetCompletesFlat
                }
                Stage5gProtectedPositionSide::Short => {
                    Stage5gProtectiveScenarioId::Gprt02F13MrShortTargetCompletesFlat
                }
            }
        }
        Stage5gProtectiveExecutionEvidence::StopOrder(_) => {
            match authority.input.protected_position_side {
                Stage5gProtectedPositionSide::Long => {
                    Stage5gProtectiveScenarioId::Gprt03F14MrLongStopCompletesFlat
                }
                Stage5gProtectedPositionSide::Short => {
                    Stage5gProtectiveScenarioId::Gprt04F15MrShortStopCompletesFlat
                }
            }
        }
    }
}

fn scenario_for_reason(
    base: Stage5gProtectiveScenarioId,
    reason: Stage5gProtectiveBlockReason,
) -> Stage5gProtectiveScenarioId {
    match reason {
        Stage5gProtectiveBlockReason::WrongOwner
        | Stage5gProtectiveBlockReason::MissingCycle
        | Stage5gProtectiveBlockReason::WrongCycle => {
            Stage5gProtectiveScenarioId::Gprt05WrongOwnerOrCycleBlocks
        }
        Stage5gProtectiveBlockReason::InstrumentMismatch
        | Stage5gProtectiveBlockReason::TargetOrderIdMismatch
        | Stage5gProtectiveBlockReason::StopOrderIdMismatch
        | Stage5gProtectiveBlockReason::StopExchangeOrderIdMismatch
        | Stage5gProtectiveBlockReason::SiblingCleanupOrderIdMismatch => {
            Stage5gProtectiveScenarioId::Gprt06WrongInstrumentOrOrderIdBlocks
        }
        Stage5gProtectiveBlockReason::PositionTruthIncomplete
        | Stage5gProtectiveBlockReason::PositionNotFlat => {
            Stage5gProtectiveScenarioId::Gprt07TriggerWithoutFlatPositionBlocks
        }
        Stage5gProtectiveBlockReason::NonExecutionTerminal
        | Stage5gProtectiveBlockReason::UnsupportedExecutionStatus => {
            Stage5gProtectiveScenarioId::Gprt08NonExecutionTerminalCannotInventExit
        }
        _ => base,
    }
}

fn normalize_status(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

fn normalize_side(value: &str) -> &'static str {
    match value.trim().to_ascii_lowercase().as_str() {
        "buy" => "buy",
        "sell" => "sell",
        _ => "unknown",
    }
}

fn expected_exit_side(side: Stage5gProtectedPositionSide) -> &'static str {
    match side {
        Stage5gProtectedPositionSide::Long => "sell",
        Stage5gProtectedPositionSide::Short => "buy",
    }
}

fn completed_projection(completed: &Stage5gProtectiveCompleted) -> impl Serialize + '_ {
    (
        completed.scenario,
        completed.leg,
        &completed.authority_summary,
        &completed.execution_receipt,
        completed.final_owner,
        &completed.final_cycle_id,
        completed.final_position_qty,
        completed.callback_count,
        &completed.bridge_post_state_fingerprint_sha256,
        &completed.post_callback_state_fingerprint_sha256,
    )
}

fn flat_cleanup_pending_projection(
    pending: &Stage5gProtectiveFlatCleanupPending,
) -> impl Serialize + '_ {
    (
        pending.scenario,
        pending.leg,
        &pending.authority_summary,
        &pending.execution_receipt,
        &pending.generated_cleanup_batch_summary,
        &pending.settled_batch_history,
        pending.final_owner,
        &pending.final_cycle_id,
        pending.final_position_qty,
        pending.callback_count,
        &pending.bridge_post_state_fingerprint_sha256,
        &pending.post_callback_state_fingerprint_sha256,
    )
}

fn semantic_sha256<T: Serialize>(value: &T) -> String {
    let bytes = serde_json::to_vec(value).expect("Stage 5G-f semantic value serializes");
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn sha256_like(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use std::thread;

    use broker_core::{BrokerAccountId, Exchange, Market};
    use chrono::{TimeZone, Utc};

    use super::*;

    const STRATEGY_ID: &str = "hybrid_imoexf";
    const CYCLE_ID: &str = "MR5GFCYC01";
    const SHA: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    fn account() -> BrokerAccountId {
        BrokerAccountId::new("ACC_STAGE5G_F_TEST")
    }

    fn instrument() -> InstrumentId {
        InstrumentId {
            symbol: "IMOEXF".to_string(),
            venue_symbol: Some("IMOEXF@RTSX".to_string()),
            exchange: Exchange::Moex,
            market: Market::Futures,
        }
    }

    fn ts(seconds: i64) -> chrono::DateTime<Utc> {
        Utc.timestamp_opt(seconds, 0).single().expect("valid ts")
    }

    fn attr(role: &str, cycle: &str, owner: &str) -> HybridRuntimeAttribution {
        HybridRuntimeAttribution::parse_source_comment(format!(
            "HYB|sid={STRATEGY_ID}|c={cycle}|o={owner}|r={role}"
        ))
        .expect("valid attr")
    }

    fn authority(side: Stage5gProtectedPositionSide) -> Stage5gProtectiveCompletionAuthority {
        let (runtime, cycle_id) =
            crate::HybridIntradayRuntimeStrategy::stage5g_test_mr_protective_runtime_fixture(
                side,
                BrokerOrderId::new("TP_STAGE5G_F"),
                BrokerStopOrderId::new("SL_STOP_STAGE5G_F"),
                BrokerOrderId::new("SL_EXCHANGE_STAGE5G_F"),
            );
        assert_eq!(cycle_id, CYCLE_ID);
        let input = runtime
            .stage5g_protective_completion_authority_input(
                STRATEGY_ID.to_string(),
                account(),
                instrument(),
                SHA.to_string(),
                SHA.to_string(),
                SHA.to_string(),
            )
            .expect("source-owned MR protective authority input");
        admit_stage5g_protective_completion_authority_from_source(input, Some(runtime))
            .expect("authority")
    }

    fn cleanup_proof() -> Stage5gProtectiveCleanupEscrowProof {
        Stage5gProtectiveCleanupEscrowProof {
            lifecycle_receipt_fingerprint_sha256: SHA.to_string(),
            accepted_cleanup_intent_fingerprint_sha256: SHA.to_string(),
        }
    }

    fn cleanup_for_completed_leg(
        leg: Stage5gProtectiveLeg,
    ) -> Stage5gProtectiveSiblingCleanupEvidence {
        Stage5gProtectiveSiblingCleanupEvidence {
            cleanup_order_id: match leg {
                Stage5gProtectiveLeg::Target => BrokerOrderId::new("SL_EXCHANGE_STAGE5G_F"),
                Stage5gProtectiveLeg::Stop => BrokerOrderId::new("TP_STAGE5G_F"),
            },
            attribution: attr("CANCEL", CYCLE_ID, "MR"),
            paper_lifecycle_escrow: cleanup_proof(),
        }
    }

    fn target_order(side: Stage5gProtectedPositionSide, status: &str) -> HybridRuntimeOrderEvent {
        HybridRuntimeOrderEvent {
            order_id: BrokerOrderId::new("TP_STAGE5G_F"),
            request_id: None,
            instrument: instrument(),
            status: status.to_string(),
            side: expected_exit_side(side).to_string(),
            order_type: "limit".to_string(),
            qty: 3.0,
            filled_qty: 3.0,
            price: 2250.0,
            existing: true,
            attribution: Some(attr("TP", CYCLE_ID, "MR")),
            source_ts_utc: 1_800_000_010,
        }
    }

    fn stop_order(side: Stage5gProtectedPositionSide, status: &str) -> HybridRuntimeStopOrderEvent {
        HybridRuntimeStopOrderEvent {
            stop_order_id: BrokerStopOrderId::new("SL_STOP_STAGE5G_F"),
            exchange_order_id: Some(BrokerOrderId::new("SL_EXCHANGE_STAGE5G_F")),
            instrument: instrument(),
            status: status.to_string(),
            side: expected_exit_side(side).to_string(),
            qty: 3.0,
            filled_qty: 3.0,
            stop_price: 2190.0,
            price: 2189.5,
            existing: true,
            attribution: Some(attr("SL", CYCLE_ID, "MR")),
            end_ts_utc: Some(1_800_010_000),
            source_ts_utc: 1_800_000_010,
        }
    }

    fn flat_position_truth() -> Stage5gProtectivePositionTruth {
        Stage5gProtectivePositionTruth {
            positions_complete: true,
            positions: vec![BrokerPositionSnapshot {
                account_id: account(),
                instrument: instrument(),
                qty: Decimal::ZERO,
                avg_price: None,
                unrealized_pnl: None,
                source_ts: Some(ts(1_800_000_011)),
                received_ts: ts(1_800_000_011),
            }],
            received_ts_utc: 1_800_000_011,
        }
    }

    fn nonflat_position_truth() -> Stage5gProtectivePositionTruth {
        Stage5gProtectivePositionTruth {
            positions_complete: true,
            positions: vec![BrokerPositionSnapshot {
                account_id: account(),
                instrument: instrument(),
                qty: Decimal::from(3),
                avg_price: None,
                unrealized_pnl: None,
                source_ts: Some(ts(1_800_000_011)),
                received_ts: ts(1_800_000_011),
            }],
            received_ts_utc: 1_800_000_011,
        }
    }

    fn target_evidence(
        side: Stage5gProtectedPositionSide,
        status: &str,
        position_truth: Stage5gProtectivePositionTruth,
    ) -> Stage5gProtectiveCompletionEvidence {
        Stage5gProtectiveCompletionEvidence {
            observed_account_id: account(),
            execution: Stage5gProtectiveExecutionEvidence::TargetOrder(target_order(side, status)),
            position_truth,
            sibling_cleanup: None,
            sibling_terminal: None,
        }
    }

    fn stop_evidence(
        side: Stage5gProtectedPositionSide,
        status: &str,
        position_truth: Stage5gProtectivePositionTruth,
    ) -> Stage5gProtectiveCompletionEvidence {
        Stage5gProtectiveCompletionEvidence {
            observed_account_id: account(),
            execution: Stage5gProtectiveExecutionEvidence::StopOrder(stop_order(side, status)),
            position_truth,
            sibling_cleanup: None,
            sibling_terminal: None,
        }
    }

    fn apply(
        authority: Stage5gProtectiveCompletionAuthority,
        evidence: Stage5gProtectiveCompletionEvidence,
    ) -> Stage5gProtectiveCompletionTransition {
        match validate_stage5g_protective_completion_evidence(&authority, evidence) {
            Ok(validated) => apply_stage5g_protective_completion(authority, validated),
            Err(reason) => {
                let scenario = Stage5gProtectiveScenarioId::Gprt05WrongOwnerOrCycleBlocks;
                Stage5gProtectiveCompletionTransition::Blocked(Stage5gProtectiveBlocked {
                    scenario: scenario_for_reason(scenario, reason),
                    authority,
                    reason,
                })
            }
        }
    }

    fn flat_cleanup_pending(
        transition: Stage5gProtectiveCompletionTransition,
    ) -> Stage5gProtectiveFlatCleanupPending {
        match transition {
            Stage5gProtectiveCompletionTransition::FlatCleanupPending(pending) => *pending,
            other => panic!("expected flat cleanup pending, got {other:?}"),
        }
    }

    fn assert_flat_cleanup_pending_witness(
        pending: &Stage5gProtectiveFlatCleanupPending,
        scenario: Stage5gProtectiveScenarioId,
        leg: Stage5gProtectiveLeg,
    ) {
        assert_eq!(pending.scenario, scenario);
        assert_eq!(pending.leg, leg);
        assert_eq!(pending.final_position_qty, Decimal::ZERO);
        assert!(pending.final_owner.is_none());
        assert!(pending.final_cycle_id.is_none());
        assert_eq!(pending.callback_count, 2);
        assert!(pending.generated_cleanup_batch_summary.intent_count > 0);
        assert_eq!(
            pending.generated_cleanup_batch_summary.intent_count,
            pending.generated_cleanup_batch().intent_count()
        );
        assert_eq!(
            pending.post_state().summary().final_position_qty,
            Decimal::ZERO
        );
        assert_eq!(
            pending.post_callback_state_fingerprint_sha256,
            pending
                .post_state()
                .summary()
                .post_callback_state_fingerprint_sha256
        );
        assert!(sha256_like(&pending.bridge_post_state_fingerprint_sha256));
        assert!(sha256_like(&pending.post_callback_state_fingerprint_sha256));
        assert!(sha256_like(&pending.cleanup_pending_fingerprint_sha256));
        assert!(!pending.post_state().runtime_live_attached());
        assert!(!pending.post_state().redis_command_stream_attached());
        assert!(!pending.post_state().finam_transport_attached());
    }

    fn blocked_reason(
        transition: Stage5gProtectiveCompletionTransition,
    ) -> Stage5gProtectiveBlockReason {
        match transition {
            Stage5gProtectiveCompletionTransition::Blocked(blocked) => blocked.reason,
            Stage5gProtectiveCompletionTransition::AwaitingPositionTruth(awaiting) => {
                awaiting.reason
            }
            other => panic!("expected block/await, got {other:?}"),
        }
    }

    #[test]
    fn stage5g_f_gprt01_mr_long_target_filled_plus_flat_cleanup_pending() {
        let transition = apply(
            authority(Stage5gProtectedPositionSide::Long),
            target_evidence(
                Stage5gProtectedPositionSide::Long,
                "Filled",
                flat_position_truth(),
            ),
        );
        let pending = flat_cleanup_pending(transition);
        assert_flat_cleanup_pending_witness(
            &pending,
            Stage5gProtectiveScenarioId::Gprt01F12MrLongTargetCompletesFlat,
            Stage5gProtectiveLeg::Target,
        );
    }

    #[test]
    fn stage5g_f_gprt02_mr_short_target_filled_plus_flat_cleanup_pending() {
        let pending = flat_cleanup_pending(apply(
            authority(Stage5gProtectedPositionSide::Short),
            target_evidence(
                Stage5gProtectedPositionSide::Short,
                "Filled",
                flat_position_truth(),
            ),
        ));
        assert_flat_cleanup_pending_witness(
            &pending,
            Stage5gProtectiveScenarioId::Gprt02F13MrShortTargetCompletesFlat,
            Stage5gProtectiveLeg::Target,
        );
    }

    #[test]
    fn stage5g_f_gprt03_mr_long_stop_execution_plus_flat_cleanup_pending() {
        let pending = flat_cleanup_pending(apply(
            authority(Stage5gProtectedPositionSide::Long),
            stop_evidence(
                Stage5gProtectedPositionSide::Long,
                "Triggered",
                flat_position_truth(),
            ),
        ));
        assert_flat_cleanup_pending_witness(
            &pending,
            Stage5gProtectiveScenarioId::Gprt03F14MrLongStopCompletesFlat,
            Stage5gProtectiveLeg::Stop,
        );
    }

    #[test]
    fn stage5g_f_gprt04_mr_short_stop_execution_plus_flat_cleanup_pending() {
        let pending = flat_cleanup_pending(apply(
            authority(Stage5gProtectedPositionSide::Short),
            stop_evidence(
                Stage5gProtectedPositionSide::Short,
                "Executed",
                flat_position_truth(),
            ),
        ));
        assert_flat_cleanup_pending_witness(
            &pending,
            Stage5gProtectiveScenarioId::Gprt04F15MrShortStopCompletesFlat,
            Stage5gProtectiveLeg::Stop,
        );
    }

    #[test]
    fn stage5g_f_gprt05_wrong_owner_or_cycle_blocks() {
        let mut input = authority(Stage5gProtectedPositionSide::Long)
            .summary()
            .authority_fingerprint_sha256;
        input.push_str("");
        let mut raw = Stage5gProtectiveCompletionAuthorityInput {
            strategy_id: STRATEGY_ID.to_string(),
            account_id: account(),
            instrument: instrument(),
            tick_size: 0.5,
            current_owner: HybridRuntimeOwner::IntradayBreakout,
            active_cycle_id: Some(CYCLE_ID.to_string()),
            protected_position_side: Stage5gProtectedPositionSide::Long,
            protected_position_qty: Decimal::from(3),
            tp_order_id: Some(BrokerOrderId::new("TP_STAGE5G_F")),
            sl_stop_order_id: Some(BrokerStopOrderId::new("SL_STOP_STAGE5G_F")),
            sl_exchange_order_id: Some(BrokerOrderId::new("SL_EXCHANGE_STAGE5G_F")),
            protective_created_ts_utc: 1_800_000_000,
            last_lifecycle_checkpoint_ts_utc: 1_800_000_001,
            operational_identity_commitment_sha256: SHA.to_string(),
            restart_package_fingerprint_sha256: SHA.to_string(),
            last_checkpoint_fingerprint_sha256: SHA.to_string(),
        };
        assert_eq!(
            admit_stage5g_protective_completion_authority(raw.clone()).unwrap_err(),
            Stage5gProtectiveBlockReason::WrongOwner
        );
        raw.current_owner = HybridRuntimeOwner::MeanReversion;
        raw.active_cycle_id = None;
        assert_eq!(
            admit_stage5g_protective_completion_authority(raw).unwrap_err(),
            Stage5gProtectiveBlockReason::MissingCycle
        );

        let mut evidence = target_evidence(
            Stage5gProtectedPositionSide::Long,
            "Filled",
            flat_position_truth(),
        );
        if let Stage5gProtectiveExecutionEvidence::TargetOrder(order) = &mut evidence.execution {
            order.attribution = Some(attr("TP", "wrong-cycle", "MR"));
        }
        assert_eq!(
            blocked_reason(apply(
                authority(Stage5gProtectedPositionSide::Long),
                evidence
            )),
            Stage5gProtectiveBlockReason::WrongCycle
        );
        let transition = apply(
            authority(Stage5gProtectedPositionSide::Long),
            target_evidence(
                Stage5gProtectedPositionSide::Long,
                "Filled",
                flat_position_truth(),
            ),
        );
        assert_ne!(
            transition.scenario(),
            Stage5gProtectiveScenarioId::Gprt05WrongOwnerOrCycleBlocks
        );
    }

    #[test]
    fn stage5g_f_gprt06_wrong_instrument_or_ids_block() {
        let mut evidence = target_evidence(
            Stage5gProtectedPositionSide::Long,
            "Filled",
            flat_position_truth(),
        );
        if let Stage5gProtectiveExecutionEvidence::TargetOrder(order) = &mut evidence.execution {
            order.order_id = BrokerOrderId::new("TP_OTHER");
        }
        assert_eq!(
            {
                let transition = apply(authority(Stage5gProtectedPositionSide::Long), evidence);
                assert_eq!(
                    transition.scenario(),
                    Stage5gProtectiveScenarioId::Gprt06WrongInstrumentOrOrderIdBlocks
                );
                blocked_reason(transition)
            },
            Stage5gProtectiveBlockReason::TargetOrderIdMismatch
        );

        let mut evidence = stop_evidence(
            Stage5gProtectedPositionSide::Long,
            "Triggered",
            flat_position_truth(),
        );
        if let Stage5gProtectiveExecutionEvidence::StopOrder(order) = &mut evidence.execution {
            order.stop_order_id = BrokerStopOrderId::new("SL_OTHER");
        }
        assert_eq!(
            {
                let transition = apply(authority(Stage5gProtectedPositionSide::Long), evidence);
                assert_eq!(
                    transition.scenario(),
                    Stage5gProtectiveScenarioId::Gprt06WrongInstrumentOrOrderIdBlocks
                );
                blocked_reason(transition)
            },
            Stage5gProtectiveBlockReason::StopOrderIdMismatch
        );

        let mut evidence = stop_evidence(
            Stage5gProtectedPositionSide::Long,
            "Triggered",
            flat_position_truth(),
        );
        if let Stage5gProtectiveExecutionEvidence::StopOrder(order) = &mut evidence.execution {
            order.exchange_order_id = Some(BrokerOrderId::new("SL_EXCHANGE_OTHER"));
        }
        assert_eq!(
            {
                let transition = apply(authority(Stage5gProtectedPositionSide::Long), evidence);
                assert_eq!(
                    transition.scenario(),
                    Stage5gProtectiveScenarioId::Gprt06WrongInstrumentOrOrderIdBlocks
                );
                blocked_reason(transition)
            },
            Stage5gProtectiveBlockReason::StopExchangeOrderIdMismatch
        );
    }

    #[test]
    fn stage5g_f_position_truth_duplicate_and_contradictory_rows_do_not_sum_flat() {
        let mut truth = flat_position_truth();
        truth.positions.push(BrokerPositionSnapshot {
            account_id: account(),
            instrument: instrument(),
            qty: Decimal::ZERO,
            avg_price: None,
            unrealized_pnl: None,
            source_ts: Some(ts(1_800_000_011)),
            received_ts: ts(1_800_000_011),
        });
        assert_eq!(
            blocked_reason(apply(
                authority(Stage5gProtectedPositionSide::Long),
                target_evidence(Stage5gProtectedPositionSide::Long, "Filled", truth)
            )),
            Stage5gProtectiveBlockReason::PositionNotFlat
        );

        let mut contradictory = Stage5gProtectivePositionTruth {
            positions_complete: true,
            positions: Vec::new(),
            received_ts_utc: 1_800_000_011,
        };
        contradictory.positions.push(BrokerPositionSnapshot {
            account_id: account(),
            instrument: instrument(),
            qty: Decimal::from(3),
            avg_price: None,
            unrealized_pnl: None,
            source_ts: Some(ts(1_800_000_011)),
            received_ts: ts(1_800_000_011),
        });
        contradictory.positions.push(BrokerPositionSnapshot {
            account_id: account(),
            instrument: instrument(),
            qty: Decimal::from(-3),
            avg_price: None,
            unrealized_pnl: None,
            source_ts: Some(ts(1_800_000_011)),
            received_ts: ts(1_800_000_011),
        });
        assert_eq!(
            blocked_reason(apply(
                authority(Stage5gProtectedPositionSide::Long),
                target_evidence(Stage5gProtectedPositionSide::Long, "Filled", contradictory)
            )),
            Stage5gProtectiveBlockReason::PositionNotFlat
        );
    }

    #[test]
    fn stage5g_f_fractional_quantity_is_rejected_by_integral_lot_authority() {
        let mut evidence = target_evidence(
            Stage5gProtectedPositionSide::Long,
            "Filled",
            flat_position_truth(),
        );
        if let Stage5gProtectiveExecutionEvidence::TargetOrder(order) = &mut evidence.execution {
            order.filled_qty = 2.5;
        }
        assert_eq!(
            blocked_reason(apply(
                authority(Stage5gProtectedPositionSide::Long),
                evidence
            )),
            Stage5gProtectiveBlockReason::InvalidExecutionQuantity
        );
    }

    #[test]
    fn stage5g_f_gprt07_trigger_without_flat_awaits_position_truth() {
        let transition = apply(
            authority(Stage5gProtectedPositionSide::Long),
            stop_evidence(
                Stage5gProtectedPositionSide::Long,
                "Triggered",
                nonflat_position_truth(),
            ),
        );
        assert_eq!(
            transition.disposition(),
            Stage5gProtectiveDisposition::AwaitingPositionTruth
        );
        assert_eq!(
            transition.scenario(),
            Stage5gProtectiveScenarioId::Gprt07TriggerWithoutFlatPositionBlocks
        );
        assert_eq!(
            blocked_reason(transition),
            Stage5gProtectiveBlockReason::PositionNotFlat
        );
    }

    #[test]
    fn stage5g_f_gprt08_non_execution_terminal_cannot_invent_exit() {
        for status in ["Canceled", "Cancelled", "Expired", "Rejected"] {
            let target_transition = apply(
                authority(Stage5gProtectedPositionSide::Long),
                target_evidence(
                    Stage5gProtectedPositionSide::Long,
                    status,
                    flat_position_truth(),
                ),
            );
            assert_eq!(
                target_transition.scenario(),
                Stage5gProtectiveScenarioId::Gprt08NonExecutionTerminalCannotInventExit
            );
            assert_eq!(
                blocked_reason(target_transition),
                Stage5gProtectiveBlockReason::NonExecutionTerminal
            );
            let stop_transition = apply(
                authority(Stage5gProtectedPositionSide::Long),
                stop_evidence(
                    Stage5gProtectedPositionSide::Long,
                    status,
                    flat_position_truth(),
                ),
            );
            assert_eq!(
                stop_transition.scenario(),
                Stage5gProtectiveScenarioId::Gprt08NonExecutionTerminalCannotInventExit
            );
            assert_eq!(
                blocked_reason(stop_transition),
                Stage5gProtectiveBlockReason::NonExecutionTerminal
            );
        }
    }

    #[test]
    fn stage5g_f_f12_to_f15_bar_extremes_remain_no_bar_exit_authority() {
        let source = include_str!("stage5g_protective_completion.rs");
        let production_source = source
            .split("#[cfg(test)]")
            .next()
            .expect("production section");
        assert!(!production_source.contains("BarEvent"));
        assert!(!production_source.contains("bar high"));
        assert!(!production_source.contains("bar low"));
        assert!(!production_source.contains(".high"));
        assert!(!production_source.contains(".low"));
    }

    #[test]
    fn stage5g_f_owner_role_instrument_side_qty_and_chronology_are_exact() {
        let mut wrong_role = target_evidence(
            Stage5gProtectedPositionSide::Long,
            "Filled",
            flat_position_truth(),
        );
        if let Stage5gProtectiveExecutionEvidence::TargetOrder(order) = &mut wrong_role.execution {
            order.attribution = Some(attr("SL", CYCLE_ID, "MR"));
        }
        assert_eq!(
            blocked_reason(apply(
                authority(Stage5gProtectedPositionSide::Long),
                wrong_role
            )),
            Stage5gProtectiveBlockReason::WrongRole
        );

        let mut wrong_side = target_evidence(
            Stage5gProtectedPositionSide::Long,
            "Filled",
            flat_position_truth(),
        );
        if let Stage5gProtectiveExecutionEvidence::TargetOrder(order) = &mut wrong_side.execution {
            order.side = "buy".to_string();
        }
        assert_eq!(
            blocked_reason(apply(
                authority(Stage5gProtectedPositionSide::Long),
                wrong_side
            )),
            Stage5gProtectiveBlockReason::SideMismatch
        );

        let mut wrong_qty = target_evidence(
            Stage5gProtectedPositionSide::Long,
            "Filled",
            flat_position_truth(),
        );
        if let Stage5gProtectiveExecutionEvidence::TargetOrder(order) = &mut wrong_qty.execution {
            order.filled_qty = 2.0;
        }
        assert_eq!(
            blocked_reason(apply(
                authority(Stage5gProtectedPositionSide::Long),
                wrong_qty
            )),
            Stage5gProtectiveBlockReason::QuantityMismatch
        );

        let mut stale = target_evidence(
            Stage5gProtectedPositionSide::Long,
            "Filled",
            flat_position_truth(),
        );
        if let Stage5gProtectiveExecutionEvidence::TargetOrder(order) = &mut stale.execution {
            order.source_ts_utc = 1_799_999_999;
        }
        assert_eq!(
            blocked_reason(apply(authority(Stage5gProtectedPositionSide::Long), stale)),
            Stage5gProtectiveBlockReason::ChronologyViolation
        );
    }

    #[test]
    fn stage5g_f_complete_absent_target_position_is_flat_but_incomplete_absent_is_not() {
        let complete_absent = Stage5gProtectivePositionTruth {
            positions_complete: true,
            positions: Vec::new(),
            received_ts_utc: 1_800_000_011,
        };
        assert_eq!(
            apply(
                authority(Stage5gProtectedPositionSide::Long),
                target_evidence(
                    Stage5gProtectedPositionSide::Long,
                    "Filled",
                    complete_absent
                )
            )
            .disposition(),
            Stage5gProtectiveDisposition::FlatCleanupPending
        );

        let incomplete_absent = Stage5gProtectivePositionTruth {
            positions_complete: false,
            positions: Vec::new(),
            received_ts_utc: 1_800_000_011,
        };
        assert_eq!(
            apply(
                authority(Stage5gProtectedPositionSide::Long),
                target_evidence(
                    Stage5gProtectedPositionSide::Long,
                    "Filled",
                    incomplete_absent
                )
            )
            .disposition(),
            Stage5gProtectiveDisposition::AwaitingPositionTruth
        );
    }

    #[test]
    fn stage5g_f_duplicate_exact_is_idempotent_and_conflicting_duplicate_blocks() {
        let first_evidence = stop_evidence(
            Stage5gProtectedPositionSide::Long,
            "Triggered",
            nonflat_position_truth(),
        );
        let transition = apply(
            authority(Stage5gProtectedPositionSide::Long),
            first_evidence.clone(),
        );
        let replay_authority = match transition {
            Stage5gProtectiveCompletionTransition::AwaitingPositionTruth(awaiting) => {
                awaiting.authority
            }
            other => panic!("expected awaiting state, got {other:?}"),
        };
        let exact_replay = apply(replay_authority, first_evidence);
        assert_eq!(
            exact_replay.disposition(),
            Stage5gProtectiveDisposition::AwaitingPositionTruth
        );

        let transition = apply(
            authority(Stage5gProtectedPositionSide::Long),
            stop_evidence(
                Stage5gProtectedPositionSide::Long,
                "Triggered",
                nonflat_position_truth(),
            ),
        );
        let conflict_authority = match transition {
            Stage5gProtectiveCompletionTransition::AwaitingPositionTruth(awaiting) => {
                awaiting.authority
            }
            other => panic!("expected awaiting state, got {other:?}"),
        };
        let mut conflicting = stop_evidence(
            Stage5gProtectedPositionSide::Long,
            "Triggered",
            nonflat_position_truth(),
        );
        if let Stage5gProtectiveExecutionEvidence::StopOrder(order) = &mut conflicting.execution {
            order.price = 2188.0;
        }
        assert_eq!(
            blocked_reason(apply(conflict_authority, conflicting)),
            Stage5gProtectiveBlockReason::ConflictingDuplicateEvidence
        );
    }

    #[test]
    fn stage5g_f_standalone_json_restart_codec_is_not_available() {
        let production_source = include_str!("stage5g_protective_completion.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("production section");
        assert!(!production_source.contains("export_stage5g_protective_completion_for_restart"));
        assert!(!production_source.contains("restore_stage5g_protective_completion_from_restart"));
        assert!(!production_source.contains("serde_json::from_slice(bytes)"));
        assert!(!production_source.contains("serde_json::to_vec(transition)"));
    }

    #[test]
    fn stage5g_f_callback_generated_cleanup_is_retained_and_raw_cleanup_is_blocked() {
        let evidence = target_evidence(
            Stage5gProtectedPositionSide::Long,
            "Filled",
            flat_position_truth(),
        );
        let pending = flat_cleanup_pending(apply(
            authority(Stage5gProtectedPositionSide::Long),
            evidence,
        ));
        assert!(pending.generated_cleanup_batch_summary.intent_count > 0);
        assert_eq!(
            pending.generated_cleanup_batch_summary.intent_count,
            pending.generated_cleanup_batch().intent_count()
        );
        assert!(!pending.post_state().runtime_live_attached());

        let mut raw_cleanup = target_evidence(
            Stage5gProtectedPositionSide::Long,
            "Filled",
            flat_position_truth(),
        );
        raw_cleanup.sibling_cleanup = Some(cleanup_for_completed_leg(Stage5gProtectiveLeg::Target));
        assert_eq!(
            blocked_reason(apply(
                authority(Stage5gProtectedPositionSide::Long),
                raw_cleanup
            )),
            Stage5gProtectiveBlockReason::MissingSiblingCleanupProof
        );

        let mut mismatch = target_evidence(
            Stage5gProtectedPositionSide::Long,
            "Filled",
            flat_position_truth(),
        );
        mismatch.sibling_terminal = Some(Stage5gProtectiveSiblingTerminalEvidence {
            sibling_order_id: BrokerOrderId::new("WRONG_SIBLING"),
            terminal_status: "Canceled".to_string(),
            terminal_receipt_fingerprint_sha256: SHA.to_string(),
        });
        assert_eq!(
            blocked_reason(apply(
                authority(Stage5gProtectedPositionSide::Long),
                mismatch
            )),
            Stage5gProtectiveBlockReason::SiblingCleanupOrderIdMismatch
        );
    }

    #[test]
    fn stage5g_f_gprt_witnesses_are_frozen_and_ordered() {
        let rendered = Stage5gProtectiveScenarioId::ALL
            .iter()
            .map(|case| case.as_id())
            .collect::<Vec<_>>();
        assert_eq!(
            rendered,
            vec![
                "GPRT01_F12_MR_LONG_TARGET_COMPLETES_FLAT",
                "GPRT02_F13_MR_SHORT_TARGET_COMPLETES_FLAT",
                "GPRT03_F14_MR_LONG_STOP_COMPLETES_FLAT",
                "GPRT04_F15_MR_SHORT_STOP_COMPLETES_FLAT",
                "GPRT05_WRONG_OWNER_OR_CYCLE_BLOCKS",
                "GPRT06_WRONG_INSTRUMENT_OR_ORDER_ID_BLOCKS",
                "GPRT07_TRIGGER_WITHOUT_FLAT_POSITION_BLOCKS",
                "GPRT08_NON_EXECUTION_TERMINAL_CANNOT_INVENT_EXIT",
            ]
        );
    }

    #[test]
    fn stage5g_f_debug_release_parallel_evidence_is_deterministic_in_process() {
        let sequential = stage5g_f_witness_fingerprints();
        let handles = (0..4)
            .map(|_| thread::spawn(stage5g_f_witness_fingerprints))
            .collect::<Vec<_>>();
        for handle in handles {
            assert_eq!(handle.join().expect("thread"), sequential);
        }
    }

    fn stage5g_f_witness_fingerprints() -> Vec<String> {
        vec![
            apply(
                authority(Stage5gProtectedPositionSide::Long),
                target_evidence(
                    Stage5gProtectedPositionSide::Long,
                    "Filled",
                    flat_position_truth(),
                ),
            )
            .semantic_fingerprint_sha256(),
            apply(
                authority(Stage5gProtectedPositionSide::Short),
                target_evidence(
                    Stage5gProtectedPositionSide::Short,
                    "Filled",
                    flat_position_truth(),
                ),
            )
            .semantic_fingerprint_sha256(),
            apply(
                authority(Stage5gProtectedPositionSide::Long),
                stop_evidence(
                    Stage5gProtectedPositionSide::Long,
                    "Triggered",
                    flat_position_truth(),
                ),
            )
            .semantic_fingerprint_sha256(),
            apply(
                authority(Stage5gProtectedPositionSide::Short),
                stop_evidence(
                    Stage5gProtectedPositionSide::Short,
                    "Executed",
                    flat_position_truth(),
                ),
            )
            .semantic_fingerprint_sha256(),
        ]
    }
}
