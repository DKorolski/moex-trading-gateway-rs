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
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::stage5g_order_position::stage5g_integral_lot_decimal;

pub const STAGE5G_PROTECTIVE_COMPLETION_SCHEMA_VERSION: u16 = 1;
pub const STAGE5G_PROTECTIVE_RESTART_PROJECTION_SCHEMA_VERSION: u16 = 1;
pub const STAGE5G_PROTECTIVE_CANONICAL_EVIDENCE_SCHEMA_VERSION: u16 = 1;

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
    UnsupportedCleanRestartLifecycleKind,
    MissingCanonicalBrokerTruth,
    CanonicalBrokerTruthMismatch,
    ProtectiveRestartProjectionMismatch,
    PostCallbackPositionNotIntegral,
    CleanupAuthorityMismatch,
    CleanupConflict,
    CleanupPositionTruthRequired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage5gProtectiveRestartProjectionKind {
    PreExecutionReady,
    AwaitingPositionTruth,
    FlatCleanupPending,
    Completed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stage5gProtectiveReceiptLedgerProjection {
    pub identity: String,
    pub fingerprint_sha256: String,
    pub disposition: Stage5gProtectiveDisposition,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stage5gProtectiveRestartProjectionV1 {
    pub schema_version: u16,
    pub projection_kind: Stage5gProtectiveRestartProjectionKind,
    pub scenario: Option<Stage5gProtectiveScenarioId>,
    pub disposition: Option<Stage5gProtectiveDisposition>,
    pub leg: Option<Stage5gProtectiveLeg>,
    pub authority_summary: Stage5gProtectiveAuthoritySummary,
    pub receipt_ledger: Vec<Stage5gProtectiveReceiptLedgerProjection>,
    pub accepted_execution_identity: Option<String>,
    pub accepted_execution_evidence_fingerprint_sha256: Option<String>,
    pub position_truth_disposition: Option<Stage5gProtectiveBlockReason>,
    pub post_runtime_stage5c_state_fingerprint_sha256: String,
    pub post_runtime_semantic_fingerprint_sha256: Option<String>,
    pub generated_cleanup_batch_fingerprint_sha256: Option<String>,
    pub cleanup_batch_restart_projection:
        Option<crate::stage5c_paper_host::Stage5gProtectiveCleanupBatchRestartProjectionV1>,
    pub settled_batch_history_fingerprint_sha256: Option<String>,
    pub cleanup_settlement_fingerprint_sha256: Option<String>,
    pub cleanup_settlement_ledger: Option<Stage5gProtectiveCleanupSettlementLedgerV1>,
    pub completion_fingerprint_sha256: Option<String>,
    pub cleanup_sibling_identity: Option<String>,
    pub callback_count: usize,
    pub cleanup_pending: bool,
    pub completed: bool,
    pub replay_protection_fingerprint_sha256: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage5gProtectiveCleanupOutcome {
    Pending,
    Canceled,
    Deleted,
    AlreadyTerminalNonExecution,
    ExecutionObserved,
    RejectedConflict,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage5gProtectiveCleanupSettlementState {
    Pending,
    TerminalNonExecution,
    ExecutionObserved,
    Conflict,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stage5gProtectiveCleanupRequestSettlementV1 {
    pub request_id: broker_core::StrategyRequestId,
    pub source_event_ts: i64,
    pub base_action: String,
    pub target_protective_id: String,
    pub expected_attribution: Option<broker_core::HybridRuntimeAttribution>,
    pub state: Stage5gProtectiveCleanupSettlementState,
    pub outcome: Option<Stage5gProtectiveCleanupOutcome>,
    pub cleanup_observation_fingerprint_sha256: Option<String>,
    pub settlement_fingerprint_sha256: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stage5gProtectiveCleanupSettlementLedgerV1 {
    pub schema_version: u16,
    pub batch_fingerprint_sha256: String,
    pub requests: Vec<Stage5gProtectiveCleanupRequestSettlementV1>,
    pub ledger_fingerprint_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stage5gProtectiveGprtArtifactRow {
    pub schema_version: u16,
    pub scenario: String,
    pub protective_leg: Option<Stage5gProtectiveLeg>,
    pub pre_runtime_semantic_fingerprint_sha256: String,
    pub pre_authenticated_restart_package_fingerprint_sha256: String,
    pub phase_a_disposition: String,
    pub phase_a_reason: String,
    pub phase_a_runtime_semantic_fingerprint_sha256: Option<String>,
    pub phase_a_execution_receipt_fingerprint_sha256: Option<String>,
    pub phase_a_execution_receipt_count: usize,
    pub phase_a_callback_count: usize,
    pub phase_a_cleanup_request_ids: Vec<broker_core::StrategyRequestId>,
    pub phase_a_cleanup_batch_fingerprint_sha256: Option<String>,
    pub phase_a_cleanup_ledger_fingerprint_sha256: Option<String>,
    pub phase_a_protective_restart_projection_fingerprint_sha256: Option<String>,
    pub phase_a_canonical_restart_package_fingerprint_sha256: Option<String>,
    pub phase_a_authenticated_restore_succeeded: bool,
    pub phase_b_disposition: Option<String>,
    pub phase_b_cleanup_request_states: Vec<Stage5gProtectiveCleanupSettlementState>,
    pub phase_b_cleanup_observation_fingerprints_sha256: Vec<String>,
    pub phase_b_final_cleanup_ledger_fingerprint_sha256: Option<String>,
    pub phase_b_completion_fingerprint_sha256: Option<String>,
    pub phase_b_final_runtime_fingerprint_sha256: Option<String>,
    pub phase_b_final_owner: Option<HybridRuntimeOwner>,
    pub phase_b_final_cycle_id: Option<String>,
    pub phase_b_final_position_qty: Option<Decimal>,
    pub phase_b_completed_restart_projection_fingerprint_sha256: Option<String>,
    pub phase_b_completed_canonical_restart_package_fingerprint_sha256: Option<String>,
    pub phase_b_authenticated_restore_succeeded: bool,
    pub cleanup_request_count: usize,
    pub cleanup_settlement_ledger_state: Option<String>,
    pub callback_count: usize,
    pub runtime_live_attached: bool,
    pub redis_command_stream_attached: bool,
    pub finam_transport_attached: bool,
    pub canonical_semantic_fingerprint_sha256: String,
}

#[cfg(any(test, feature = "stage5g-artifact-fixtures"))]
pub fn stage5g_f_gprt_artifact_rows() -> Vec<Stage5gProtectiveGprtArtifactRow> {
    Stage5gProtectiveScenarioId::ALL
        .into_iter()
        .map(stage5g_f_source_produced_gprt_artifact_row)
        .collect()
}

#[cfg(any(test, feature = "stage5g-artifact-fixtures"))]
pub fn stage5g_f_gprt_artifact_rows_parallel_verified() -> Vec<Stage5gProtectiveGprtArtifactRow> {
    let sequential = stage5g_f_gprt_artifact_rows();
    let mut handles = Vec::new();
    for scenario in Stage5gProtectiveScenarioId::ALL {
        handles.push(std::thread::spawn(move || {
            stage5g_f_source_produced_gprt_artifact_row(scenario)
        }));
    }
    let parallel: Vec<_> = handles
        .into_iter()
        .map(|handle| handle.join().expect("Stage 5G-f artifact worker joins"))
        .collect();
    assert_eq!(
        serde_json::to_vec(&sequential).expect("sequential artifact serializes"),
        serde_json::to_vec(&parallel).expect("parallel artifact serializes"),
        "Stage 5G-f R6 source-produced GPRT artifact must be parallel deterministic"
    );
    sequential
}

#[cfg(any(test, feature = "stage5g-artifact-fixtures"))]
pub fn stage5g_f_gprt_artifact_json_pretty() -> String {
    serde_json::to_string_pretty(&stage5g_f_gprt_artifact_rows_parallel_verified())
        .expect("Stage 5G-f GPRT artifact serializes")
}

#[cfg(any(test, feature = "stage5g-artifact-fixtures"))]
const STAGE5G_F_ARTIFACT_STRATEGY_ID: &str = "hybrid_imoexf";
#[cfg(any(test, feature = "stage5g-artifact-fixtures"))]
const STAGE5G_F_ARTIFACT_CYCLE_ID: &str = "abc1230001";

#[cfg(any(test, feature = "stage5g-artifact-fixtures"))]
struct Stage5gArtifactPreparedAuthority {
    authority: Stage5gProtectiveCompletionAuthority,
    pre_runtime_fingerprint_sha256: String,
    pre_package_fingerprint_sha256: String,
}

#[cfg(any(test, feature = "stage5g-artifact-fixtures"))]
fn stage5g_f_source_produced_gprt_artifact_row(
    scenario: Stage5gProtectiveScenarioId,
) -> Stage5gProtectiveGprtArtifactRow {
    let prepared = stage5g_f_artifact_authority(stage5g_f_artifact_scenario_side(scenario));
    let pre_runtime_fingerprint_sha256 = prepared.pre_runtime_fingerprint_sha256;
    let pre_package_fingerprint_sha256 = prepared.pre_package_fingerprint_sha256;
    let transition = stage5g_f_artifact_transition(scenario, prepared.authority);
    let mut row = match transition {
        Stage5gProtectiveCompletionTransition::FlatCleanupPending(pending) => {
            stage5g_f_artifact_row_from_flat_cleanup_pending(
                scenario,
                *pending,
                pre_runtime_fingerprint_sha256,
                pre_package_fingerprint_sha256,
            )
        }
        Stage5gProtectiveCompletionTransition::AwaitingPositionTruth(awaiting) => {
            let leg = awaiting.leg;
            let reason = format!("{:?}", awaiting.reason).to_ascii_lowercase();
            let receipt_fingerprint = awaiting.execution_receipt.fingerprint_sha256.clone();
            let receipt_count = awaiting.authority.accepted_receipts.len();
            let source = stage5g_protective_restart_source_from_transition(
                Stage5gProtectiveCompletionTransition::AwaitingPositionTruth(awaiting),
            )
            .expect("Stage 5G-f R7 awaiting transition is restartable");
            let phase_a_projection_fingerprint = source
                .projection
                .replay_protection_fingerprint_sha256
                .clone();
            let (restored, phase_a_package_fingerprint) = stage5g_f_authenticated_package_roundtrip(
                source,
                &format!("{}-awaiting", scenario.as_id()),
            );
            let continuation = restore_stage5g_protective_completion_continuation(restored)
                .expect("Stage 5G-f R7 awaiting continuation restores");
            let Stage5gProtectiveRestoredContinuation::AwaitingPositionTruth(awaiting) =
                continuation
            else {
                panic!("Stage 5G-f R7 expected awaiting continuation")
            };
            let phase_a_runtime = runtime_stage5c_state_fingerprint(
                awaiting
                    .authority
                    .runtime
                    .as_ref()
                    .expect("restored awaiting authority owns runtime"),
            )
            .expect("restored awaiting runtime fingerprints");
            let mut row = stage5g_f_artifact_base_row(
                scenario,
                Some(leg),
                pre_runtime_fingerprint_sha256,
                pre_package_fingerprint_sha256,
                "awaiting_position_truth",
                &reason,
                0,
            );
            row.phase_a_runtime_semantic_fingerprint_sha256 = Some(phase_a_runtime);
            row.phase_a_execution_receipt_fingerprint_sha256 = Some(receipt_fingerprint);
            row.phase_a_execution_receipt_count = receipt_count;
            row.phase_a_protective_restart_projection_fingerprint_sha256 =
                Some(phase_a_projection_fingerprint);
            row.phase_a_canonical_restart_package_fingerprint_sha256 =
                Some(phase_a_package_fingerprint);
            row.phase_a_authenticated_restore_succeeded = true;
            row
        }
        Stage5gProtectiveCompletionTransition::Blocked(blocked) => {
            let post_runtime = runtime_stage5c_state_fingerprint(
                blocked
                    .authority
                    .runtime
                    .as_ref()
                    .expect("blocked production authority retains runtime"),
            )
            .expect("blocked runtime fingerprints");
            let receipt_count = blocked.authority.accepted_receipts.len();
            let mut row = stage5g_f_artifact_base_row(
                scenario,
                stage5g_f_artifact_scenario_leg(scenario),
                pre_runtime_fingerprint_sha256,
                pre_package_fingerprint_sha256,
                "blocked",
                &format!("{:?}", blocked.reason).to_ascii_lowercase(),
                0,
            );
            row.phase_a_runtime_semantic_fingerprint_sha256 = Some(post_runtime);
            row.phase_a_execution_receipt_count = receipt_count;
            row
        }
        Stage5gProtectiveCompletionTransition::Completed(_) => {
            panic!("Stage 5G-f R7 GPRT01-08 must not bypass cleanup settlement")
        }
    };
    row.canonical_semantic_fingerprint_sha256 =
        semantic_sha256(&("stage5g-f-r7-production-authority-gprt-artifact-row", &row));
    row
}

#[cfg(any(test, feature = "stage5g-artifact-fixtures"))]
fn stage5g_f_artifact_row_from_flat_cleanup_pending(
    scenario: Stage5gProtectiveScenarioId,
    pending: Stage5gProtectiveFlatCleanupPending,
    pre_runtime_fingerprint_sha256: String,
    pre_package_fingerprint_sha256: String,
) -> Stage5gProtectiveGprtArtifactRow {
    let leg = pending.leg;
    let source = stage5g_protective_restart_source_from_transition(
        Stage5gProtectiveCompletionTransition::FlatCleanupPending(Box::new(pending)),
    )
    .expect("Stage 5G-f R7 phase-A transition is restartable");
    let phase_a_restart_projection = source
        .projection
        .replay_protection_fingerprint_sha256
        .clone();
    let (restored_capability, phase_a_package_fingerprint) =
        stage5g_f_authenticated_package_roundtrip(source, &format!("{}-phase-a", scenario.as_id()));
    let continuation = restore_stage5g_protective_completion_continuation(restored_capability)
        .expect("Stage 5G-f R7 phase-A continuation restores");
    let Stage5gProtectiveRestoredContinuation::FlatCleanupPending(mut restored) = continuation
    else {
        panic!("Stage 5G-f R7 expected flat-cleanup continuation")
    };
    let phase_a_ledger = restored.cleanup_settlement_ledger.clone();
    let cleanup_request_count = phase_a_ledger.requests.len();
    let cleanup_request_ids = phase_a_ledger
        .requests
        .iter()
        .map(|request| request.request_id)
        .collect::<Vec<_>>();
    let cleanup_batch_fingerprint = phase_a_ledger.batch_fingerprint_sha256.clone();
    let phase_a_runtime = restored
        .projection
        .post_runtime_semantic_fingerprint_sha256
        .clone()
        .expect("phase-A runtime semantic fingerprint");
    let phase_a_receipt = restored
        .projection
        .accepted_execution_evidence_fingerprint_sha256
        .clone()
        .expect("phase-A execution receipt fingerprint");
    let callback_count = restored.projection.callback_count;
    let execution_receipt_count = restored.projection.receipt_ledger.len();
    let request_count = restored.cleanup_settlement_ledger.requests.len();
    for index in 0..request_count {
        let record = restored
            .projection
            .cleanup_batch_restart_projection
            .as_ref()
            .expect("cleanup projection")
            .records[index]
            .clone();
        let accepted = accept_stage5g_protective_cleanup_truth(
            &restored,
            record.request_id,
            record.target_protective_id.clone(),
            stage5g_f_artifact_cleanup_terminal_status(&record),
            record.source_event_ts + 1,
        )
        .expect("Stage 5G-f artifact cleanup truth accepted");
        match apply_stage5g_protective_cleanup_completion(*restored, accepted) {
            Stage5gProtectiveCleanupTransition::FlatCleanupPending(next) => {
                restored = next;
            }
            Stage5gProtectiveCleanupTransition::Completed(completed) => {
                let source = stage5g_protective_restart_source_from_transition(
                    Stage5gProtectiveCompletionTransition::Completed(completed),
                )
                .expect("Stage 5G-f R7 completed transition restartable");
                let phase_b_restart_projection = source
                    .projection
                    .replay_protection_fingerprint_sha256
                    .clone();
                let (restored_capability, phase_b_package_fingerprint) =
                    stage5g_f_authenticated_package_roundtrip(
                        source,
                        &format!("{}-phase-b", scenario.as_id()),
                    );
                let completed =
                    match restore_stage5g_protective_completion_continuation(restored_capability)
                        .expect("Stage 5G-f R7 completed continuation restores")
                    {
                        Stage5gProtectiveRestoredContinuation::Completed(restored) => restored,
                        _ => panic!("Stage 5G-f R7 expected completed continuation"),
                    };
                let final_ledger = completed
                    .projection
                    .cleanup_settlement_ledger
                    .clone()
                    .expect("restored Completed owns final cleanup ledger");
                assert!(stage5g_cleanup_ledger_all_terminal_non_execution(
                    &final_ledger
                ));
                assert_eq!(
                    completed
                        .projection
                        .cleanup_settlement_fingerprint_sha256
                        .as_deref(),
                    Some(final_ledger.ledger_fingerprint_sha256.as_str())
                );
                let final_summary = completed.post_state.summary();
                let mut row = stage5g_f_artifact_base_row(
                    scenario,
                    Some(leg),
                    pre_runtime_fingerprint_sha256,
                    pre_package_fingerprint_sha256,
                    "flat_cleanup_pending",
                    "execution_and_flat_position_confirmed_cleanup_required",
                    callback_count,
                );
                row.phase_a_runtime_semantic_fingerprint_sha256 = Some(phase_a_runtime);
                row.phase_a_execution_receipt_fingerprint_sha256 = Some(phase_a_receipt);
                row.phase_a_execution_receipt_count = execution_receipt_count;
                row.phase_a_cleanup_request_ids = cleanup_request_ids;
                row.phase_a_cleanup_batch_fingerprint_sha256 = Some(cleanup_batch_fingerprint);
                row.phase_a_cleanup_ledger_fingerprint_sha256 =
                    Some(phase_a_ledger.ledger_fingerprint_sha256);
                row.phase_a_protective_restart_projection_fingerprint_sha256 =
                    Some(phase_a_restart_projection);
                row.phase_a_canonical_restart_package_fingerprint_sha256 =
                    Some(phase_a_package_fingerprint);
                row.phase_a_authenticated_restore_succeeded = true;
                row.phase_b_disposition = Some("completed".to_string());
                row.phase_b_cleanup_request_states = final_ledger
                    .requests
                    .iter()
                    .map(|request| request.state)
                    .collect();
                row.phase_b_cleanup_observation_fingerprints_sha256 = final_ledger
                    .requests
                    .iter()
                    .map(|request| {
                        request
                            .cleanup_observation_fingerprint_sha256
                            .clone()
                            .expect("terminal cleanup observation fingerprint")
                    })
                    .collect();
                row.phase_b_final_cleanup_ledger_fingerprint_sha256 =
                    Some(final_ledger.ledger_fingerprint_sha256);
                row.phase_b_completion_fingerprint_sha256 = Some(
                    completed
                        .projection
                        .completion_fingerprint_sha256
                        .clone()
                        .expect("restored Completed owns completion fingerprint"),
                );
                row.phase_b_final_runtime_fingerprint_sha256 =
                    Some(final_summary.post_callback_state_fingerprint_sha256.clone());
                row.phase_b_final_owner = final_summary.final_owner;
                row.phase_b_final_cycle_id = final_summary.final_cycle_id.clone();
                row.phase_b_final_position_qty = Some(final_summary.final_position_qty);
                row.phase_b_completed_restart_projection_fingerprint_sha256 =
                    Some(phase_b_restart_projection);
                row.phase_b_completed_canonical_restart_package_fingerprint_sha256 =
                    Some(phase_b_package_fingerprint);
                row.phase_b_authenticated_restore_succeeded = true;
                row.cleanup_request_count = cleanup_request_count;
                row.cleanup_settlement_ledger_state =
                    Some("all_cleanup_requests_terminal_non_execution".to_string());
                return row;
            }
            Stage5gProtectiveCleanupTransition::CleanupPositionTruthRequired(_)
            | Stage5gProtectiveCleanupTransition::Blocked { .. } => {
                panic!("Stage 5G-f artifact cleanup settlement must complete")
            }
        }
    }
    panic!("Stage 5G-f artifact cleanup loop did not complete")
}

#[cfg(any(test, feature = "stage5g-artifact-fixtures"))]
fn stage5g_f_artifact_base_row(
    scenario: Stage5gProtectiveScenarioId,
    protective_leg: Option<Stage5gProtectiveLeg>,
    pre_runtime_semantic_fingerprint_sha256: String,
    pre_authenticated_restart_package_fingerprint_sha256: String,
    phase_a_disposition: &str,
    phase_a_reason: &str,
    callback_count: usize,
) -> Stage5gProtectiveGprtArtifactRow {
    Stage5gProtectiveGprtArtifactRow {
        schema_version: 3,
        scenario: scenario.as_id().to_string(),
        protective_leg,
        pre_runtime_semantic_fingerprint_sha256,
        pre_authenticated_restart_package_fingerprint_sha256,
        phase_a_disposition: phase_a_disposition.to_string(),
        phase_a_reason: phase_a_reason.to_string(),
        phase_a_runtime_semantic_fingerprint_sha256: None,
        phase_a_execution_receipt_fingerprint_sha256: None,
        phase_a_execution_receipt_count: 0,
        phase_a_callback_count: callback_count,
        phase_a_cleanup_request_ids: Vec::new(),
        phase_a_cleanup_batch_fingerprint_sha256: None,
        phase_a_cleanup_ledger_fingerprint_sha256: None,
        phase_a_protective_restart_projection_fingerprint_sha256: None,
        phase_a_canonical_restart_package_fingerprint_sha256: None,
        phase_a_authenticated_restore_succeeded: false,
        phase_b_disposition: None,
        phase_b_cleanup_request_states: Vec::new(),
        phase_b_cleanup_observation_fingerprints_sha256: Vec::new(),
        phase_b_final_cleanup_ledger_fingerprint_sha256: None,
        phase_b_completion_fingerprint_sha256: None,
        phase_b_final_runtime_fingerprint_sha256: None,
        phase_b_final_owner: None,
        phase_b_final_cycle_id: None,
        phase_b_final_position_qty: None,
        phase_b_completed_restart_projection_fingerprint_sha256: None,
        phase_b_completed_canonical_restart_package_fingerprint_sha256: None,
        phase_b_authenticated_restore_succeeded: false,
        cleanup_request_count: 0,
        cleanup_settlement_ledger_state: None,
        callback_count,
        runtime_live_attached: false,
        redis_command_stream_attached: false,
        finam_transport_attached: false,
        canonical_semantic_fingerprint_sha256: String::new(),
    }
}

#[cfg(any(test, feature = "stage5g-artifact-fixtures"))]
fn stage5g_f_artifact_transition(
    scenario: Stage5gProtectiveScenarioId,
    authority: Stage5gProtectiveCompletionAuthority,
) -> Stage5gProtectiveCompletionTransition {
    let (side, evidence) = match scenario {
        Stage5gProtectiveScenarioId::Gprt01F12MrLongTargetCompletesFlat => (
            Stage5gProtectedPositionSide::Long,
            stage5g_f_artifact_target_evidence(
                Stage5gProtectedPositionSide::Long,
                "Filled",
                stage5g_f_artifact_flat_position_truth(),
            ),
        ),
        Stage5gProtectiveScenarioId::Gprt02F13MrShortTargetCompletesFlat => (
            Stage5gProtectedPositionSide::Short,
            stage5g_f_artifact_target_evidence(
                Stage5gProtectedPositionSide::Short,
                "Filled",
                stage5g_f_artifact_flat_position_truth(),
            ),
        ),
        Stage5gProtectiveScenarioId::Gprt03F14MrLongStopCompletesFlat => (
            Stage5gProtectedPositionSide::Long,
            stage5g_f_artifact_stop_evidence(
                Stage5gProtectedPositionSide::Long,
                "Triggered",
                stage5g_f_artifact_flat_position_truth(),
            ),
        ),
        Stage5gProtectiveScenarioId::Gprt04F15MrShortStopCompletesFlat => (
            Stage5gProtectedPositionSide::Short,
            stage5g_f_artifact_stop_evidence(
                Stage5gProtectedPositionSide::Short,
                "Executed",
                stage5g_f_artifact_flat_position_truth(),
            ),
        ),
        Stage5gProtectiveScenarioId::Gprt05WrongOwnerOrCycleBlocks => {
            let mut evidence = stage5g_f_artifact_target_evidence(
                Stage5gProtectedPositionSide::Long,
                "Filled",
                stage5g_f_artifact_flat_position_truth(),
            );
            if let Stage5gProtectiveExecutionEvidence::TargetOrder(order) = &mut evidence.execution
            {
                order.attribution = Some(stage5g_f_artifact_attr("TP", "wrong-cycle", "MR"));
            }
            (Stage5gProtectedPositionSide::Long, evidence)
        }
        Stage5gProtectiveScenarioId::Gprt06WrongInstrumentOrOrderIdBlocks => {
            let mut evidence = stage5g_f_artifact_target_evidence(
                Stage5gProtectedPositionSide::Long,
                "Filled",
                stage5g_f_artifact_flat_position_truth(),
            );
            if let Stage5gProtectiveExecutionEvidence::TargetOrder(order) = &mut evidence.execution
            {
                order.order_id = BrokerOrderId::new("TP_OTHER");
            }
            (Stage5gProtectedPositionSide::Long, evidence)
        }
        Stage5gProtectiveScenarioId::Gprt07TriggerWithoutFlatPositionBlocks => (
            Stage5gProtectedPositionSide::Long,
            stage5g_f_artifact_stop_evidence(
                Stage5gProtectedPositionSide::Long,
                "Triggered",
                stage5g_f_artifact_nonflat_position_truth(),
            ),
        ),
        Stage5gProtectiveScenarioId::Gprt08NonExecutionTerminalCannotInventExit => (
            Stage5gProtectedPositionSide::Long,
            stage5g_f_artifact_target_evidence(
                Stage5gProtectedPositionSide::Long,
                "Canceled",
                stage5g_f_artifact_flat_position_truth(),
            ),
        ),
    };
    debug_assert_eq!(side, authority.input.protected_position_side);
    match accept_stage5g_canonical_protective_broker_truth(&authority, evidence)
        .and_then(|accepted| issue_stage5g_canonical_protective_evidence(&authority, accepted))
    {
        Ok(validated) => apply_stage5g_protective_completion(authority, validated),
        Err(reason) => Stage5gProtectiveCompletionTransition::Blocked(Stage5gProtectiveBlocked {
            scenario: scenario_for_reason(scenario, reason),
            authority,
            reason,
        }),
    }
}

#[cfg(any(test, feature = "stage5g-artifact-fixtures"))]
fn stage5g_f_artifact_authority(
    side: Stage5gProtectedPositionSide,
) -> Stage5gArtifactPreparedAuthority {
    let source = stage5g_f_artifact_pre_execution_source(side);
    let pre_runtime_fingerprint_sha256 =
        runtime_stage5c_state_fingerprint(source.stage5g_runtime_strategy())
            .expect("Stage 5G-f R7 source runtime fingerprints");
    let (restored, pre_package_fingerprint_sha256) =
        stage5g_f_authenticated_package_roundtrip(source, &format!("{side:?}-pre"));
    let authority = prepare_stage5g_protective_completion(restored)
        .expect("Stage 5G-f R7 production prepare issues protective authority");
    Stage5gArtifactPreparedAuthority {
        authority,
        pre_runtime_fingerprint_sha256,
        pre_package_fingerprint_sha256,
    }
}

#[cfg(any(test, feature = "stage5g-artifact-fixtures"))]
fn stage5g_f_artifact_pre_execution_source(
    side: Stage5gProtectedPositionSide,
) -> Stage5gProtectiveRestartSource {
    let (mut runtime, cycle_id) =
        crate::HybridIntradayRuntimeStrategy::stage5g_artifact_mr_protective_runtime_fixture(
            side,
            BrokerOrderId::new("TP_STAGE5G_F"),
            BrokerStopOrderId::new("SL_STOP_STAGE5G_F"),
            BrokerOrderId::new("SL_EXCHANGE_STAGE5G_F"),
        );
    assert_eq!(cycle_id, STAGE5G_F_ARTIFACT_CYCLE_ID);
    stage5g_f_artifact_set_protective_strategy_state(&mut runtime, side);
    let persisted_at = stage5g_f_artifact_ts(1_800_000_030);
    let _ = crate::stage5d_persistence::stage5g_artifact_prepare_clean_restart_authority(
        &mut runtime,
        STAGE5G_F_ARTIFACT_STRATEGY_ID,
        persisted_at,
    );
    Stage5gProtectiveRestartSource::pre_execution_ready(
        runtime,
        STAGE5G_F_ARTIFACT_STRATEGY_ID.to_string(),
        stage5g_f_artifact_account(),
        stage5g_f_artifact_instrument(),
        stage5g_f_artifact_order_position_summary(),
        stage5g_f_artifact_timer_checkpoint(),
    )
    .expect("Stage 5G-f R7 source-owned pre-execution restart source")
}

#[cfg(any(test, feature = "stage5g-artifact-fixtures"))]
fn stage5g_f_artifact_set_protective_strategy_state(
    runtime: &mut crate::HybridIntradayRuntimeStrategy,
    side: Stage5gProtectedPositionSide,
) {
    let (runtime_side, position_qty) = match side {
        Stage5gProtectedPositionSide::Long => (crate::hybrid_intraday::Side::Long, 3.0),
        Stage5gProtectedPositionSide::Short => (crate::hybrid_intraday::Side::Short, -3.0),
    };
    crate::runtime_compat::Strategy::set_state(
        runtime,
        crate::runtime_compat::StrategyState::HybridIntradayRuntime {
            active_cycle_id: Some(STAGE5G_F_ARTIFACT_CYCLE_ID.to_string()),
            next_cycle_seq: 1,
            last_position_qty: position_qty,
            current_owner: Some(crate::hybrid_intraday::Owner::MeanReversion),
            current_side: Some(runtime_side),
            pending_entry_owner: None,
            pending_entry_side: None,
            pending_entry_cycle_id: None,
            pending_entry_request_id: None,
            pending_entry_created_ts_utc: None,
            deferred_entry_owner: None,
            deferred_entry_side: None,
            deferred_entry_cycle_id: None,
            deferred_entry_entry_style: None,
            deferred_entry_reason: None,
            deferred_entry_stop_price: None,
            deferred_entry_take_price: None,
            deferred_entry_ts_utc: None,
            deferred_entry_request_id: None,
            pending_exit_request_id: None,
            pending_exit_created_ts_utc: None,
            deferred_exit_owner: None,
            deferred_exit_reason: None,
            deferred_exit_cycle_id: None,
            deferred_exit_ts_utc: None,
            deferred_exit_request_id: None,
            pending_tp_request_id: None,
            pending_tp_created_ts_utc: None,
            pending_sl_request_id: None,
            pending_sl_created_ts_utc: None,
            tp_order_id: Some(BrokerOrderId::new("TP_STAGE5G_F")),
            sl_stop_order_id: Some(BrokerStopOrderId::new("SL_STOP_STAGE5G_F")),
            sl_exchange_order_id: Some(BrokerOrderId::new("SL_EXCHANGE_STAGE5G_F")),
            sl_triggered_ts: None,
            mr_take_price: Some(2250.0),
            mr_stop_price: Some(2190.0),
            repair_deadline_ts: None,
            next_repair_at_ts: None,
            repair_backoff_level: 0,
            repair_attempts: 0,
            safe_mode_close_only: false,
            safe_mode_reason: None,
            entry_ready: true,
            last_bar_close: Some(2220.0),
            prev_day_close: Some(2234.5),
            last_day_local: Some("2026-07-06".to_string()),
            current_day_high: Some(2262.0),
            current_day_low: Some(2165.5),
            current_day_close: Some(2220.0),
            prev_day_range: Some(60.5),
            prev_day_return: Some(0.0),
            day_before_close: Some(2234.5),
            today_start_local: Some("2026-07-06T09:00:00".to_string()),
            was_long_today: matches!(side, Stage5gProtectedPositionSide::Long),
            was_short_today: matches!(side, Stage5gProtectedPositionSide::Short),
            overnight_exit_armed_date: None,
            risk_gate_shadow_session_date: Some("2026-07-06".to_string()),
            risk_gate_shadow_pnl_points: 0.0,
            risk_gate_shadow_trade_count: 0,
            risk_gate_shadow_entry_ts_utc: None,
            risk_gate_shadow_entry_price: None,
            risk_gate_shadow_side: None,
            risk_gate_shadow_target_price: None,
            risk_gate_shadow_stop_price: None,
            risk_gate_pending_session_date: None,
            risk_gate_pending_shadow_pnl_points: 0.0,
            risk_gate_pending_shadow_trade_count: 0,
            risk_gate_mr_enabled_current_session: Some(true),
            risk_gate_rolling_sum_lb120: Some(158.6),
            risk_gate_last_finalized_session_date: Some("2026-07-03".to_string()),
            risk_gate_ledger_rows_count: 221,
        },
    );
}

#[cfg(any(test, feature = "stage5g-artifact-fixtures"))]
fn stage5g_f_authenticated_package_roundtrip(
    source: Stage5gProtectiveRestartSource,
    snapshot_id: &str,
) -> (crate::Stage5gCleanRestartedCapability, String) {
    let fresh_runtime = source
        .stage5g_runtime_strategy()
        .stage5g_clean_reconstruction_candidate();
    let source = crate::Stage5gCleanRestartSource::ProtectiveLifecycle(source);
    let persisted_at = stage5g_f_artifact_ts(1_800_000_030);
    let (riskgate, riskgate_evidence) =
        crate::stage5g_clean_restart::stage5g_test_persistence_authority_from_source(
            &source,
            persisted_at,
        );
    let input = crate::Stage5gCleanRestartExportInput {
        snapshot_id: snapshot_id.to_string(),
        snapshot_revision: 1,
        previous_revision: None,
        write_generation: 1,
        persisted_at_ts_utc: persisted_at,
        source_commit_or_build_id:
            crate::stage5d_persistence::STAGE5D_RUNTIME_SEMANTIC_COMPATIBILITY_ID.to_string(),
        lifecycle_watermarks: crate::Stage5dLifecycleWatermarks {
            persisted_event_watermark: Some(snapshot_id.to_string()),
            last_semantic_bar_ts: Some(persisted_at - chrono::Duration::minutes(1)),
            last_broker_event_ts: Some(persisted_at - chrono::Duration::seconds(1)),
        },
        riskgate,
        riskgate_evidence,
    };
    let commitment_key = crate::Stage5gLifecycleCommitmentKey::from_secret_bytes(&[0x6d; 32])
        .expect("Stage 5G-f R7 artifact commitment key");
    let bytes = crate::export_stage5g_clean_restart(source, input, &commitment_key)
        .expect("Stage 5G-f R7 canonical authenticated package exports");
    let package_fingerprint_sha256 = semantic_sha256(&(
        "moex.stage5g-f.r7.canonical-authenticated-package.v1",
        &bytes,
    ));
    let restored = crate::restore_stage5g_clean_restart(&bytes, &commitment_key, fresh_runtime)
        .expect("Stage 5G-f R7 canonical package strict-decodes, authenticates and restores");
    (restored, package_fingerprint_sha256)
}

#[cfg(any(test, feature = "stage5g-artifact-fixtures"))]
#[doc(hidden)]
pub fn stage6d_test_authenticated_restart_fixture() -> (
    Vec<u8>,
    crate::Stage5gLifecycleCommitmentKey,
    crate::HybridIntradayRuntimeStrategy,
) {
    let source = stage5g_f_artifact_pre_execution_source(Stage5gProtectedPositionSide::Long);
    let fresh_runtime = source
        .stage5g_runtime_strategy()
        .stage5g_clean_reconstruction_candidate();
    let source = crate::Stage5gCleanRestartSource::ProtectiveLifecycle(source);
    let persisted_at = stage5g_f_artifact_ts(1_800_000_030);
    let (riskgate, riskgate_evidence) =
        crate::stage5g_clean_restart::stage5g_test_persistence_authority_from_source(
            &source,
            persisted_at,
        );
    let input = crate::Stage5gCleanRestartExportInput {
        snapshot_id: "stage6d-authenticated-restart-fixture".to_string(),
        snapshot_revision: 1,
        previous_revision: None,
        write_generation: 1,
        persisted_at_ts_utc: persisted_at,
        source_commit_or_build_id:
            crate::stage5d_persistence::STAGE5D_RUNTIME_SEMANTIC_COMPATIBILITY_ID.to_string(),
        lifecycle_watermarks: crate::Stage5dLifecycleWatermarks {
            persisted_event_watermark: Some("stage6d-authenticated-restart-fixture".to_string()),
            last_semantic_bar_ts: Some(persisted_at - chrono::Duration::minutes(1)),
            last_broker_event_ts: Some(persisted_at - chrono::Duration::seconds(1)),
        },
        riskgate,
        riskgate_evidence,
    };
    let key = crate::Stage5gLifecycleCommitmentKey::from_secret_bytes(&[0x6d; 32])
        .expect("Stage 6D test commitment key");
    let bytes = crate::export_stage5g_clean_restart(source, input, &key)
        .expect("Stage 6D source-produced Stage 5G package exports");
    (bytes, key, fresh_runtime)
}

#[cfg(any(test, feature = "stage5g-artifact-fixtures"))]
fn stage5g_f_artifact_scenario_side(
    scenario: Stage5gProtectiveScenarioId,
) -> Stage5gProtectedPositionSide {
    match scenario {
        Stage5gProtectiveScenarioId::Gprt02F13MrShortTargetCompletesFlat
        | Stage5gProtectiveScenarioId::Gprt04F15MrShortStopCompletesFlat => {
            Stage5gProtectedPositionSide::Short
        }
        _ => Stage5gProtectedPositionSide::Long,
    }
}

#[cfg(any(test, feature = "stage5g-artifact-fixtures"))]
fn stage5g_f_artifact_scenario_leg(
    scenario: Stage5gProtectiveScenarioId,
) -> Option<Stage5gProtectiveLeg> {
    match scenario {
        Stage5gProtectiveScenarioId::Gprt03F14MrLongStopCompletesFlat
        | Stage5gProtectiveScenarioId::Gprt04F15MrShortStopCompletesFlat
        | Stage5gProtectiveScenarioId::Gprt07TriggerWithoutFlatPositionBlocks => {
            Some(Stage5gProtectiveLeg::Stop)
        }
        _ => Some(Stage5gProtectiveLeg::Target),
    }
}

#[cfg(any(test, feature = "stage5g-artifact-fixtures"))]
fn stage5g_f_artifact_account() -> BrokerAccountId {
    BrokerAccountId::new("ACC_STAGE5G_F_TEST")
}

#[cfg(any(test, feature = "stage5g-artifact-fixtures"))]
fn stage5g_f_artifact_instrument() -> InstrumentId {
    InstrumentId {
        symbol: "IMOEXF".to_string(),
        venue_symbol: Some("IMOEXF@RTSX".to_string()),
        exchange: broker_core::Exchange::Moex,
        market: broker_core::Market::Futures,
    }
}

#[cfg(any(test, feature = "stage5g-artifact-fixtures"))]
fn stage5g_f_artifact_attr(role: &str, cycle: &str, owner: &str) -> HybridRuntimeAttribution {
    HybridRuntimeAttribution::parse_source_comment(format!(
        "HYB|sid={STAGE5G_F_ARTIFACT_STRATEGY_ID}|c={cycle}|o={owner}|r={role}"
    ))
    .expect("Stage 5G-f artifact attribution parses")
}

#[cfg(any(test, feature = "stage5g-artifact-fixtures"))]
fn stage5g_f_artifact_ts(seconds: i64) -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::from_timestamp(seconds, 0).expect("Stage 5G-f artifact timestamp")
}

#[cfg(any(test, feature = "stage5g-artifact-fixtures"))]
fn stage5g_f_artifact_target_order(
    side: Stage5gProtectedPositionSide,
    status: &str,
) -> HybridRuntimeOrderEvent {
    HybridRuntimeOrderEvent {
        order_id: BrokerOrderId::new("TP_STAGE5G_F"),
        request_id: None,
        instrument: stage5g_f_artifact_instrument(),
        status: status.to_string(),
        side: expected_exit_side(side).to_string(),
        order_type: "limit".to_string(),
        qty: 3.0,
        filled_qty: 3.0,
        price: 2250.0,
        existing: true,
        attribution: Some(stage5g_f_artifact_attr(
            "TP",
            STAGE5G_F_ARTIFACT_CYCLE_ID,
            "MR",
        )),
        source_ts_utc: 1_800_000_010,
    }
}

#[cfg(any(test, feature = "stage5g-artifact-fixtures"))]
fn stage5g_f_artifact_stop_order(
    side: Stage5gProtectedPositionSide,
    status: &str,
) -> HybridRuntimeStopOrderEvent {
    HybridRuntimeStopOrderEvent {
        stop_order_id: BrokerStopOrderId::new("SL_STOP_STAGE5G_F"),
        exchange_order_id: Some(BrokerOrderId::new("SL_EXCHANGE_STAGE5G_F")),
        instrument: stage5g_f_artifact_instrument(),
        status: status.to_string(),
        side: expected_exit_side(side).to_string(),
        qty: 3.0,
        filled_qty: 3.0,
        stop_price: 2190.0,
        price: 2189.5,
        existing: true,
        attribution: Some(stage5g_f_artifact_attr(
            "SL",
            STAGE5G_F_ARTIFACT_CYCLE_ID,
            "MR",
        )),
        end_ts_utc: Some(1_800_010_000),
        source_ts_utc: 1_800_000_010,
    }
}

#[cfg(any(test, feature = "stage5g-artifact-fixtures"))]
fn stage5g_f_artifact_flat_position_truth() -> Stage5gProtectivePositionTruth {
    Stage5gProtectivePositionTruth {
        positions_complete: true,
        positions: vec![BrokerPositionSnapshot {
            account_id: stage5g_f_artifact_account(),
            instrument: stage5g_f_artifact_instrument(),
            qty: Decimal::ZERO,
            avg_price: None,
            unrealized_pnl: None,
            source_ts: Some(stage5g_f_artifact_ts(1_800_000_011)),
            received_ts: stage5g_f_artifact_ts(1_800_000_011),
        }],
        received_ts_utc: 1_800_000_011,
    }
}

#[cfg(any(test, feature = "stage5g-artifact-fixtures"))]
fn stage5g_f_artifact_nonflat_position_truth() -> Stage5gProtectivePositionTruth {
    Stage5gProtectivePositionTruth {
        positions_complete: true,
        positions: vec![BrokerPositionSnapshot {
            account_id: stage5g_f_artifact_account(),
            instrument: stage5g_f_artifact_instrument(),
            qty: Decimal::from(3),
            avg_price: None,
            unrealized_pnl: None,
            source_ts: Some(stage5g_f_artifact_ts(1_800_000_011)),
            received_ts: stage5g_f_artifact_ts(1_800_000_011),
        }],
        received_ts_utc: 1_800_000_011,
    }
}

#[cfg(any(test, feature = "stage5g-artifact-fixtures"))]
fn stage5g_f_artifact_target_evidence(
    side: Stage5gProtectedPositionSide,
    status: &str,
    position_truth: Stage5gProtectivePositionTruth,
) -> Stage5gProtectiveCompletionEvidence {
    Stage5gProtectiveCompletionEvidence {
        observed_account_id: stage5g_f_artifact_account(),
        execution: Stage5gProtectiveExecutionEvidence::TargetOrder(
            stage5g_f_artifact_target_order(side, status),
        ),
        position_truth,
        sibling_cleanup: None,
        sibling_terminal: None,
    }
}

#[cfg(any(test, feature = "stage5g-artifact-fixtures"))]
fn stage5g_f_artifact_stop_evidence(
    side: Stage5gProtectedPositionSide,
    status: &str,
    position_truth: Stage5gProtectivePositionTruth,
) -> Stage5gProtectiveCompletionEvidence {
    Stage5gProtectiveCompletionEvidence {
        observed_account_id: stage5g_f_artifact_account(),
        execution: Stage5gProtectiveExecutionEvidence::StopOrder(stage5g_f_artifact_stop_order(
            side, status,
        )),
        position_truth,
        sibling_cleanup: None,
        sibling_terminal: None,
    }
}

#[cfg(any(test, feature = "stage5g-artifact-fixtures"))]
fn stage5g_f_artifact_order_position_summary() -> crate::Stage5gOrderPositionSummary {
    let mut summary = crate::Stage5gOrderPositionSummary {
        schema_version: crate::STAGE5G_ORDER_POSITION_SCHEMA_VERSION,
        strategy_id: STAGE5G_F_ARTIFACT_STRATEGY_ID.to_string(),
        request_count: 0,
        terminal_request_count: 0,
        order_transition_count: 0,
        correlated_trade_count: 0,
        position_confirmation_count: 0,
        duplicate_evidence_count: 0,
        last_total_sequence: Some(1),
        lifecycle_fingerprint_sha256: String::new(),
        stage5c_callback_count: 0,
        mock_feedback_only: true,
        redis_attached: false,
        finam_transport_attached: false,
        broker_execution_attached: false,
    };
    summary.lifecycle_fingerprint_sha256 = semantic_sha256(&(
        "moex.stage5g-f.r7.artifact-order-position-summary.v1",
        &summary,
    ));
    summary
}

#[cfg(any(test, feature = "stage5g-artifact-fixtures"))]
fn stage5g_f_artifact_timer_checkpoint() -> crate::Stage5gTimerCheckpointEnvelope {
    let received_at = stage5g_f_artifact_ts(1_800_000_020);
    let package_discriminator = format!(
        "moex.broker-truth.package.v1:{}:{:09}",
        received_at.timestamp(),
        received_at.timestamp_subsec_nanos()
    );
    let evidence_identity = format!(
        "moex.stage5g.order-position-evidence-identity.v3:00000000-0000-4000-8000-000000000001:{}:{}",
        stage5g_f_artifact_account().0,
        package_discriminator
    );
    let payload = crate::Stage5gTimerCheckpointPayload {
        schema_version: crate::STAGE5G_TIMER_CHECKPOINT_SCHEMA_VERSION,
        package_discriminator: Some(package_discriminator),
        current_evidence_identity: Some(evidence_identity.clone()),
        evidence_replay_ledger: vec![crate::Stage5gReplayLedgerEntry {
            identity: evidence_identity.clone(),
            fingerprint_sha256: semantic_sha256(&(
                "moex.stage5g-f.r7.artifact-evidence.v1",
                &evidence_identity,
            )),
        }],
        last_broker_truth_received_at: Some(received_at),
        last_broker_truth_received_ms: Some(1_800_000_020_000),
        duplicate_evidence_count: 0,
        last_total_sequence: Some(1),
        last_continuation_checkpoint_ts_utc_ms: Some(1_800_000_020_001),
    };
    crate::Stage5gTimerCheckpointEnvelope {
        payload_sha256: stage5g_f_artifact_timer_checkpoint_fingerprint(&payload),
        payload,
    }
}

#[cfg(any(test, feature = "stage5g-artifact-fixtures"))]
fn stage5g_f_artifact_timer_checkpoint_fingerprint(
    payload: &crate::Stage5gTimerCheckpointPayload,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"moex.stage5g.timer-checkpoint.v1\0");
    hasher.update(serde_json::to_vec(payload).expect("Stage 5G-f R7 timer checkpoint serializes"));
    format!("{:x}", hasher.finalize())
}

#[cfg(any(test, feature = "stage5g-artifact-fixtures"))]
fn stage5g_f_artifact_cleanup_terminal_status(
    record: &crate::stage5c_paper_host::Stage5gProtectiveCleanupBatchRestartRecordV1,
) -> &'static str {
    match record.base_action {
        crate::stage5c_paper_host::Stage5gSourceBaseAction::DeleteStopLimit => "Deleted",
        crate::stage5c_paper_host::Stage5gSourceBaseAction::Cancel => "Canceled",
        _ => "Rejected",
    }
}

#[derive(Clone)]
pub(crate) struct Stage5gProtectiveRestartSeed {
    pub(crate) lifecycle_kind: crate::Stage5gCleanRestartLifecycleKind,
    pub(crate) strategy_id: String,
    pub(crate) account_id: BrokerAccountId,
    pub(crate) instrument: InstrumentId,
    pub(crate) summary: crate::Stage5gOrderPositionSummary,
    pub(crate) checkpoint: crate::Stage5gTimerCheckpointEnvelope,
    pub(crate) order_position_state:
        Option<crate::stage5g_order_position::Stage5gOrderPositionState>,
    pub(crate) parent_restart_package_fingerprint_sha256: String,
    pub(crate) lifecycle_source_authority_sha256: String,
    pub(crate) source_lifecycle_commit_sha256: String,
}

pub(crate) struct Stage5gProtectiveCleanRestartParts {
    pub(crate) runtime: crate::HybridIntradayRuntimeStrategy,
    pub(crate) input: Stage5gProtectiveCompletionAuthorityInput,
    pub(crate) restart_seed: Stage5gProtectiveRestartSeed,
    pub(crate) protective_projection: Option<Stage5gProtectiveRestartProjectionV1>,
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
    restart_seed: Option<Stage5gProtectiveRestartSeed>,
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
pub(crate) struct Stage5gProtectiveCompletionEvidence {
    pub observed_account_id: BrokerAccountId,
    pub execution: Stage5gProtectiveExecutionEvidence,
    pub position_truth: Stage5gProtectivePositionTruth,
    pub sibling_cleanup: Option<Stage5gProtectiveSiblingCleanupEvidence>,
    pub sibling_terminal: Option<Stage5gProtectiveSiblingTerminalEvidence>,
}

pub struct Stage5gAcceptedProtectiveBrokerTruth {
    evidence: Stage5gProtectiveCompletionEvidence,
    canonical_authority_fingerprint_sha256: String,
    broker_truth_fingerprint_sha256: String,
    position_section_complete: bool,
    sibling_terminal_authorized: bool,
}

impl std::fmt::Debug for Stage5gAcceptedProtectiveBrokerTruth {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Stage5gAcceptedProtectiveBrokerTruth")
            .field(
                "canonical_authority_fingerprint_sha256",
                &self.canonical_authority_fingerprint_sha256,
            )
            .field(
                "broker_truth_fingerprint_sha256",
                &self.broker_truth_fingerprint_sha256,
            )
            .field("position_section_complete", &self.position_section_complete)
            .field(
                "sibling_terminal_authorized",
                &self.sibling_terminal_authorized,
            )
            .finish_non_exhaustive()
    }
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
pub(crate) enum Stage5gProtectiveExecutionEvidence {
    TargetOrder(HybridRuntimeOrderEvent),
    StopOrder(HybridRuntimeStopOrderEvent),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct Stage5gProtectivePositionTruth {
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
pub(crate) struct Stage5gProtectiveSiblingCleanupEvidence {
    pub cleanup_order_id: BrokerOrderId,
    pub attribution: HybridRuntimeAttribution,
    pub paper_lifecycle_escrow: Stage5gProtectiveCleanupEscrowProof,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Stage5gProtectiveSiblingTerminalEvidence {
    sibling_order_id: BrokerOrderId,
    terminal_status: String,
    terminal_receipt_fingerprint_sha256: String,
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
    pub cleanup_settlement_fingerprint_sha256: Option<String>,
    pub final_cleanup_settlement_ledger: Option<Stage5gProtectiveCleanupSettlementLedgerV1>,
    pub final_cleanup_batch_restart_projection:
        Option<crate::stage5c_paper_host::Stage5gProtectiveCleanupBatchRestartProjectionV1>,
    pub completion_fingerprint_sha256: String,
    post_state: Stage5gProtectiveCommittedState,
    restart_seed: Option<Stage5gProtectiveRestartSeed>,
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
                "cleanup_settlement_fingerprint_sha256",
                &self.cleanup_settlement_fingerprint_sha256,
            )
            .field(
                "final_cleanup_settlement_ledger",
                &self.final_cleanup_settlement_ledger,
            )
            .field(
                "final_cleanup_batch_restart_projection",
                &self.final_cleanup_batch_restart_projection,
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
    pub cleanup_settlement_ledger: Stage5gProtectiveCleanupSettlementLedgerV1,
    post_state: Stage5gProtectiveCommittedState,
    generated_cleanup_batch: crate::Stage5cPaperIntentBatch,
    restart_seed: Option<Stage5gProtectiveRestartSeed>,
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
            .field("cleanup_settlement_ledger", &self.cleanup_settlement_ledger)
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

pub struct Stage5gProtectiveRestartSource {
    runtime: crate::HybridIntradayRuntimeStrategy,
    strategy_id: String,
    account_id: BrokerAccountId,
    instrument: InstrumentId,
    summary: crate::Stage5gOrderPositionSummary,
    checkpoint: crate::Stage5gTimerCheckpointEnvelope,
    order_position_state: Option<crate::stage5g_order_position::Stage5gOrderPositionState>,
    projection: Stage5gProtectiveRestartProjectionV1,
}

impl std::fmt::Debug for Stage5gProtectiveRestartSource {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Stage5gProtectiveRestartSource")
            .field("projection_kind", &self.projection.projection_kind)
            .field("strategy_id", &self.strategy_id)
            .field("account_id", &self.account_id)
            .field("instrument", &self.instrument)
            .field(
                "replay_protection_fingerprint_sha256",
                &self.projection.replay_protection_fingerprint_sha256,
            )
            .field("redis_command_stream_attached", &false)
            .field("finam_transport_attached", &false)
            .field("runtime_live_attached", &false)
            .finish_non_exhaustive()
    }
}

impl Stage5gProtectiveRestartSource {
    #[cfg(any(test, feature = "stage5g-artifact-fixtures"))]
    pub(crate) fn pre_execution_ready(
        runtime: crate::HybridIntradayRuntimeStrategy,
        strategy_id: String,
        account_id: BrokerAccountId,
        instrument: InstrumentId,
        summary: crate::Stage5gOrderPositionSummary,
        checkpoint: crate::Stage5gTimerCheckpointEnvelope,
    ) -> Result<Self, Stage5gProtectiveBlockReason> {
        let input = runtime
            .stage5g_protective_completion_authority_input(
                strategy_id.clone(),
                account_id.clone(),
                instrument.clone(),
                semantic_sha256(&(
                    "stage5g-f-r3-pre-execution-operational-identity",
                    &strategy_id,
                )),
                runtime_stage5c_state_fingerprint(&runtime)?,
                semantic_sha256(&checkpoint),
            )
            .ok_or(Stage5gProtectiveBlockReason::MissingCleanRestartProtectiveState)?;
        let authority_summary = Stage5gProtectiveAuthoritySummary {
            schema_version: STAGE5G_PROTECTIVE_COMPLETION_SCHEMA_VERSION,
            strategy_id: input.strategy_id.clone(),
            account_id: input.account_id.clone(),
            instrument: input.instrument.clone(),
            tick_size: input.tick_size,
            active_cycle_id: input.active_cycle_id.clone().unwrap_or_default(),
            protected_position_side: input.protected_position_side,
            protected_position_qty: input.protected_position_qty,
            tp_order_id: input.tp_order_id.clone(),
            sl_stop_order_id: input.sl_stop_order_id.clone(),
            sl_exchange_order_id: input.sl_exchange_order_id.clone(),
            protective_created_ts_utc: input.protective_created_ts_utc,
            last_lifecycle_checkpoint_ts_utc: input.last_lifecycle_checkpoint_ts_utc,
            accepted_receipt_count: 0,
            authority_fingerprint_sha256: semantic_sha256(&input),
        };
        let projection = protective_restart_projection(Stage5gProtectiveRestartProjectionInput {
            projection_kind: Stage5gProtectiveRestartProjectionKind::PreExecutionReady,
            scenario: None,
            disposition: None,
            leg: None,
            authority_summary,
            receipt_ledger: Vec::new(),
            accepted_execution_identity: None,
            accepted_execution_evidence_fingerprint_sha256: None,
            post_runtime_stage5c_state_fingerprint_sha256: runtime_stage5c_state_fingerprint(
                &runtime,
            )?,
            post_runtime_semantic_fingerprint_sha256: None,
            generated_cleanup_batch_fingerprint_sha256: None,
            cleanup_batch_restart_projection: None,
            settled_batch_history_fingerprint_sha256: None,
            cleanup_settlement_fingerprint_sha256: None,
            cleanup_settlement_ledger: None,
            completion_fingerprint_sha256: None,
            cleanup_sibling_identity: None,
            callback_count: 0,
            cleanup_pending: false,
            completed: false,
        });
        Ok(Self {
            runtime,
            strategy_id,
            account_id,
            instrument,
            summary,
            checkpoint,
            order_position_state: None,
            projection,
        })
    }

    pub(crate) fn stage5g_runtime_strategy(&self) -> &crate::HybridIntradayRuntimeStrategy {
        &self.runtime
    }

    pub(crate) fn stage5g_restart_binding(&self) -> (&str, &BrokerAccountId, &InstrumentId) {
        (&self.strategy_id, &self.account_id, &self.instrument)
    }

    pub(crate) fn stage5g_restart_summary(&self) -> crate::Stage5gOrderPositionSummary {
        self.summary.clone()
    }

    pub(crate) fn stage5g_restart_checkpoint(&self) -> crate::Stage5gTimerCheckpointEnvelope {
        self.checkpoint.clone()
    }

    pub(crate) fn stage5g_restart_order_position_state(
        &self,
    ) -> Option<crate::stage5g_order_position::Stage5gOrderPositionState> {
        self.order_position_state.clone()
    }

    pub(crate) fn stage5g_restart_projection(&self) -> Stage5gProtectiveRestartProjectionV1 {
        self.projection.clone()
    }
}

pub enum Stage5gProtectiveRestoredContinuation {
    PreExecutionReady(Stage5gProtectiveCompletionAuthority),
    AwaitingPositionTruth(Stage5gProtectiveAwaitingPositionTruth),
    FlatCleanupPending(Box<Stage5gProtectiveRestoredFlatCleanupPending>),
    Completed(Stage5gProtectiveRestoredCompleted),
}

pub struct Stage5gProtectiveRestoredFlatCleanupPending {
    pub projection: Stage5gProtectiveRestartProjectionV1,
    post_state: Stage5gProtectiveCommittedState,
    generated_cleanup_batch: crate::Stage5cPaperIntentBatch,
    generated_cleanup_batch_summary: crate::Stage5cPaperIntentBatchSummary,
    settled_batch_history: Vec<crate::Stage5cPaperIntentBatchSummary>,
    cleanup_settlement_ledger: Stage5gProtectiveCleanupSettlementLedgerV1,
    restart_seed: Option<Stage5gProtectiveRestartSeed>,
}

impl std::fmt::Debug for Stage5gProtectiveRestoredFlatCleanupPending {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Stage5gProtectiveRestoredFlatCleanupPending")
            .field("projection", &self.projection)
            .field("post_state", &self.post_state)
            .field(
                "generated_cleanup_batch_summary",
                &self.generated_cleanup_batch_summary,
            )
            .field("settled_batch_history", &self.settled_batch_history)
            .field("cleanup_settlement_ledger", &self.cleanup_settlement_ledger)
            .field("restart_seed_present", &self.restart_seed.is_some())
            .finish_non_exhaustive()
    }
}

#[derive(Debug)]
pub struct Stage5gProtectiveRestoredCompleted {
    pub projection: Stage5gProtectiveRestartProjectionV1,
    post_state: Stage5gProtectiveCommittedState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stage5gProtectiveCleanupSettlementEvidence {
    pub request_id: broker_core::StrategyRequestId,
    pub target_protective_id: String,
    pub base_action: String,
    pub expected_attribution: Option<broker_core::HybridRuntimeAttribution>,
    pub status: String,
    pub outcome: Stage5gProtectiveCleanupOutcome,
    pub received_ts_utc: i64,
    pub batch_fingerprint_sha256: String,
    pub cleanup_ledger_fingerprint_before_sha256: String,
    pub pending_cleanup_authority_sha256: String,
    pub cleanup_observation_fingerprint_sha256: String,
    pub settlement_fingerprint_sha256: String,
}

pub struct Stage5gAcceptedProtectiveCleanupTruth {
    evidence: Stage5gProtectiveCleanupSettlementEvidence,
}

impl std::fmt::Debug for Stage5gAcceptedProtectiveCleanupTruth {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Stage5gAcceptedProtectiveCleanupTruth")
            .field("request_id", &self.evidence.request_id)
            .field("target_protective_id", &self.evidence.target_protective_id)
            .field("status", &self.evidence.status)
            .field(
                "settlement_fingerprint_sha256",
                &self.evidence.settlement_fingerprint_sha256,
            )
            .finish_non_exhaustive()
    }
}

#[derive(Debug)]
pub enum Stage5gProtectiveCleanupTransition {
    Completed(Box<Stage5gProtectiveCompleted>),
    FlatCleanupPending(Box<Stage5gProtectiveRestoredFlatCleanupPending>),
    CleanupPositionTruthRequired(Box<Stage5gProtectiveRestoredFlatCleanupPending>),
    Blocked {
        projection: Box<Stage5gProtectiveRestartProjectionV1>,
        reason: Stage5gProtectiveBlockReason,
    },
}

impl Stage5gProtectiveRestoredFlatCleanupPending {
    pub fn post_state(&self) -> &Stage5gProtectiveCommittedState {
        &self.post_state
    }

    pub fn generated_cleanup_batch(&self) -> &crate::Stage5cPaperIntentBatch {
        &self.generated_cleanup_batch
    }

    pub fn generated_cleanup_batch_summary(&self) -> &crate::Stage5cPaperIntentBatchSummary {
        &self.generated_cleanup_batch_summary
    }

    pub fn cleanup_settlement_ledger(&self) -> &Stage5gProtectiveCleanupSettlementLedgerV1 {
        &self.cleanup_settlement_ledger
    }
}

impl Stage5gProtectiveRestoredCompleted {
    pub fn post_state(&self) -> &Stage5gProtectiveCommittedState {
        &self.post_state
    }
}

pub fn accept_stage5g_protective_cleanup_truth(
    pending: &Stage5gProtectiveRestoredFlatCleanupPending,
    request_id: broker_core::StrategyRequestId,
    target_protective_id: impl Into<String>,
    status: impl Into<String>,
    received_ts_utc: i64,
) -> Result<Stage5gAcceptedProtectiveCleanupTruth, Stage5gProtectiveBlockReason> {
    let target_protective_id = target_protective_id.into();
    let status = status.into();
    let cleanup_projection = pending
        .projection
        .cleanup_batch_restart_projection
        .as_ref()
        .ok_or(Stage5gProtectiveBlockReason::ProtectiveRestartProjectionMismatch)?;
    let record = cleanup_projection
        .records
        .iter()
        .find(|record| record.request_id == request_id)
        .ok_or(Stage5gProtectiveBlockReason::ProtectiveRestartProjectionMismatch)?;
    if !stage5g_cleanup_ledger_is_valid(&pending.cleanup_settlement_ledger, cleanup_projection) {
        return Err(Stage5gProtectiveBlockReason::ProtectiveRestartProjectionMismatch);
    }
    if target_protective_id != record.target_protective_id
        || cleanup_projection.batch_fingerprint
            != crate::stage5c_paper_host::stage5g_protective_cleanup_batch_projection_fingerprint(
                cleanup_projection,
            )
        || received_ts_utc < record.source_event_ts
    {
        return Err(Stage5gProtectiveBlockReason::ProtectiveRestartProjectionMismatch);
    }
    let outcome = stage5g_cleanup_outcome_for_action(record.base_action, &status);
    let pending_cleanup_authority_sha256 = stage5g_pending_cleanup_authority_sha256(pending)?;
    let mut evidence = Stage5gProtectiveCleanupSettlementEvidence {
        request_id,
        target_protective_id,
        base_action: stage5g_cleanup_action_name(record.base_action).to_string(),
        expected_attribution: record.expected_attribution.clone(),
        status,
        outcome,
        received_ts_utc,
        batch_fingerprint_sha256: cleanup_projection.batch_fingerprint.clone(),
        cleanup_ledger_fingerprint_before_sha256: pending
            .cleanup_settlement_ledger
            .ledger_fingerprint_sha256
            .clone(),
        pending_cleanup_authority_sha256,
        cleanup_observation_fingerprint_sha256: String::new(),
        settlement_fingerprint_sha256: String::new(),
    };
    evidence.cleanup_observation_fingerprint_sha256 = stage5g_cleanup_observation_fingerprint(
        pending.projection.scenario,
        pending.projection.leg,
        &pending.projection.authority_summary,
        &evidence,
    );
    evidence.settlement_fingerprint_sha256 =
        evidence.cleanup_observation_fingerprint_sha256.clone();
    Ok(Stage5gAcceptedProtectiveCleanupTruth { evidence })
}

pub fn apply_stage5g_protective_cleanup_completion(
    mut pending: Stage5gProtectiveRestoredFlatCleanupPending,
    accepted: Stage5gAcceptedProtectiveCleanupTruth,
) -> Stage5gProtectiveCleanupTransition {
    let Some(scenario) = pending.projection.scenario else {
        return Stage5gProtectiveCleanupTransition::Blocked {
            projection: Box::new(pending.projection),
            reason: Stage5gProtectiveBlockReason::ProtectiveRestartProjectionMismatch,
        };
    };
    let Some(leg) = pending.projection.leg else {
        return Stage5gProtectiveCleanupTransition::Blocked {
            projection: Box::new(pending.projection),
            reason: Stage5gProtectiveBlockReason::ProtectiveRestartProjectionMismatch,
        };
    };
    let Some(execution_receipt_projection) = pending.projection.receipt_ledger.last().cloned()
    else {
        return Stage5gProtectiveCleanupTransition::Blocked {
            projection: Box::new(pending.projection),
            reason: Stage5gProtectiveBlockReason::ProtectiveRestartProjectionMismatch,
        };
    };
    let Some(cleanup_projection) = pending.projection.cleanup_batch_restart_projection.as_ref()
    else {
        return Stage5gProtectiveCleanupTransition::Blocked {
            projection: Box::new(pending.projection),
            reason: Stage5gProtectiveBlockReason::ProtectiveRestartProjectionMismatch,
        };
    };
    if !stage5g_cleanup_ledger_is_valid(&pending.cleanup_settlement_ledger, cleanup_projection) {
        return Stage5gProtectiveCleanupTransition::Blocked {
            projection: Box::new(pending.projection),
            reason: Stage5gProtectiveBlockReason::ProtectiveRestartProjectionMismatch,
        };
    }
    let Ok(pending_authority) = stage5g_pending_cleanup_authority_sha256(&pending) else {
        return Stage5gProtectiveCleanupTransition::Blocked {
            projection: Box::new(pending.projection),
            reason: Stage5gProtectiveBlockReason::ProtectiveRestartProjectionMismatch,
        };
    };
    if pending_authority != accepted.evidence.pending_cleanup_authority_sha256
        || pending.cleanup_settlement_ledger.ledger_fingerprint_sha256
            != accepted.evidence.cleanup_ledger_fingerprint_before_sha256
        || pending.cleanup_settlement_ledger.batch_fingerprint_sha256
            != accepted.evidence.batch_fingerprint_sha256
    {
        return Stage5gProtectiveCleanupTransition::Blocked {
            projection: Box::new(pending.projection),
            reason: Stage5gProtectiveBlockReason::CleanupAuthorityMismatch,
        };
    }
    let Some((entry_index, entry)) = pending
        .cleanup_settlement_ledger
        .requests
        .iter()
        .enumerate()
        .find(|(_, entry)| entry.request_id == accepted.evidence.request_id)
    else {
        return Stage5gProtectiveCleanupTransition::Blocked {
            projection: Box::new(pending.projection),
            reason: Stage5gProtectiveBlockReason::ProtectiveRestartProjectionMismatch,
        };
    };
    if entry.target_protective_id != accepted.evidence.target_protective_id
        || entry.base_action != accepted.evidence.base_action
        || entry.expected_attribution != accepted.evidence.expected_attribution
    {
        return Stage5gProtectiveCleanupTransition::Blocked {
            projection: Box::new(pending.projection),
            reason: Stage5gProtectiveBlockReason::CleanupAuthorityMismatch,
        };
    }
    if let Some(existing_fingerprint) = &entry.cleanup_observation_fingerprint_sha256 {
        if existing_fingerprint == &accepted.evidence.cleanup_observation_fingerprint_sha256
            && entry.outcome == Some(accepted.evidence.outcome)
        {
            return Stage5gProtectiveCleanupTransition::FlatCleanupPending(Box::new(pending));
        }
        return Stage5gProtectiveCleanupTransition::Blocked {
            projection: Box::new(pending.projection),
            reason: Stage5gProtectiveBlockReason::CleanupConflict,
        };
    }
    let next_state = stage5g_cleanup_outcome_state(accepted.evidence.outcome);
    if next_state == Stage5gProtectiveCleanupSettlementState::Pending {
        return Stage5gProtectiveCleanupTransition::FlatCleanupPending(Box::new(pending));
    }
    pending.cleanup_settlement_ledger.requests[entry_index].state = next_state;
    pending.cleanup_settlement_ledger.requests[entry_index].outcome =
        Some(accepted.evidence.outcome);
    pending.cleanup_settlement_ledger.requests[entry_index]
        .cleanup_observation_fingerprint_sha256 = Some(
        accepted
            .evidence
            .cleanup_observation_fingerprint_sha256
            .clone(),
    );
    pending.cleanup_settlement_ledger.requests[entry_index].settlement_fingerprint_sha256 =
        Some(accepted.evidence.settlement_fingerprint_sha256.clone());
    pending.cleanup_settlement_ledger.ledger_fingerprint_sha256 =
        stage5g_cleanup_ledger_fingerprint(&pending.cleanup_settlement_ledger);
    pending.projection.cleanup_settlement_ledger = Some(pending.cleanup_settlement_ledger.clone());
    pending.projection.replay_protection_fingerprint_sha256 =
        protective_projection_fingerprint(&pending.projection);
    if next_state == Stage5gProtectiveCleanupSettlementState::ExecutionObserved {
        return Stage5gProtectiveCleanupTransition::CleanupPositionTruthRequired(Box::new(pending));
    }
    if next_state == Stage5gProtectiveCleanupSettlementState::Conflict {
        return Stage5gProtectiveCleanupTransition::Blocked {
            projection: Box::new(pending.projection),
            reason: Stage5gProtectiveBlockReason::CleanupConflict,
        };
    }
    if !stage5g_cleanup_ledger_all_terminal_non_execution(&pending.cleanup_settlement_ledger) {
        return Stage5gProtectiveCleanupTransition::FlatCleanupPending(Box::new(pending));
    }
    let execution_receipt = Stage5gProtectiveEvidenceReceipt {
        identity: execution_receipt_projection.identity,
        fingerprint_sha256: execution_receipt_projection.fingerprint_sha256,
        disposition: Stage5gProtectiveDisposition::Completed,
    };
    let post_state_summary = pending.post_state.summary().clone();
    let completion_fingerprint_sha256 = semantic_sha256(&(
        scenario,
        leg,
        &pending.projection.authority_summary,
        &execution_receipt,
        &pending.cleanup_settlement_ledger,
        post_state_summary.final_owner,
        &post_state_summary.final_cycle_id,
        post_state_summary.final_position_qty,
        pending.projection.callback_count,
        &pending.projection.post_runtime_semantic_fingerprint_sha256,
    ));
    let final_cleanup_settlement_ledger = pending.cleanup_settlement_ledger;
    let cleanup_settlement_fingerprint_sha256 = final_cleanup_settlement_ledger
        .ledger_fingerprint_sha256
        .clone();
    let final_cleanup_batch_restart_projection =
        pending.projection.cleanup_batch_restart_projection.clone();
    let completed = Stage5gProtectiveCompleted {
        scenario,
        leg,
        authority_summary: pending.projection.authority_summary,
        execution_receipt,
        final_owner: post_state_summary.final_owner,
        final_cycle_id: post_state_summary.final_cycle_id,
        final_position_qty: post_state_summary.final_position_qty,
        callback_count: pending.projection.callback_count,
        bridge_post_state_fingerprint_sha256: pending
            .projection
            .post_runtime_semantic_fingerprint_sha256
            .clone()
            .unwrap_or_else(|| {
                post_state_summary
                    .post_callback_state_fingerprint_sha256
                    .clone()
            }),
        post_callback_state_fingerprint_sha256: post_state_summary
            .post_callback_state_fingerprint_sha256,
        cleanup_settlement_fingerprint_sha256: Some(cleanup_settlement_fingerprint_sha256),
        final_cleanup_settlement_ledger: Some(final_cleanup_settlement_ledger),
        final_cleanup_batch_restart_projection,
        completion_fingerprint_sha256,
        post_state: pending.post_state,
        restart_seed: pending.restart_seed,
    };
    Stage5gProtectiveCleanupTransition::Completed(Box::new(completed))
}

fn stage5g_cleanup_action_name(
    action: crate::stage5c_paper_host::Stage5gSourceBaseAction,
) -> &'static str {
    match action {
        crate::stage5c_paper_host::Stage5gSourceBaseAction::Cancel => "cancel",
        crate::stage5c_paper_host::Stage5gSourceBaseAction::DeleteStopLimit => "delete_stop_limit",
        _ => "unsupported",
    }
}

fn stage5g_cleanup_outcome_for_action(
    action: crate::stage5c_paper_host::Stage5gSourceBaseAction,
    status: &str,
) -> Stage5gProtectiveCleanupOutcome {
    match status.trim().to_ascii_lowercase().as_str() {
        "working" | "pending" | "submitted" | "active" => Stage5gProtectiveCleanupOutcome::Pending,
        "filled" | "executed" | "triggered" | "completed-as-execution" => {
            Stage5gProtectiveCleanupOutcome::ExecutionObserved
        }
        "rejected" | "conflict" | "conflicting" => {
            Stage5gProtectiveCleanupOutcome::RejectedConflict
        }
        "already_terminal_non_execution" | "already-terminal-nonexecution" => {
            Stage5gProtectiveCleanupOutcome::AlreadyTerminalNonExecution
        }
        "canceled" | "cancelled" => Stage5gProtectiveCleanupOutcome::Canceled,
        "deleted"
            if action == crate::stage5c_paper_host::Stage5gSourceBaseAction::DeleteStopLimit =>
        {
            Stage5gProtectiveCleanupOutcome::Deleted
        }
        "deleted" => Stage5gProtectiveCleanupOutcome::RejectedConflict,
        _ => Stage5gProtectiveCleanupOutcome::RejectedConflict,
    }
}

fn stage5g_cleanup_outcome_state(
    outcome: Stage5gProtectiveCleanupOutcome,
) -> Stage5gProtectiveCleanupSettlementState {
    match outcome {
        Stage5gProtectiveCleanupOutcome::Pending => {
            Stage5gProtectiveCleanupSettlementState::Pending
        }
        Stage5gProtectiveCleanupOutcome::Canceled
        | Stage5gProtectiveCleanupOutcome::Deleted
        | Stage5gProtectiveCleanupOutcome::AlreadyTerminalNonExecution => {
            Stage5gProtectiveCleanupSettlementState::TerminalNonExecution
        }
        Stage5gProtectiveCleanupOutcome::ExecutionObserved => {
            Stage5gProtectiveCleanupSettlementState::ExecutionObserved
        }
        Stage5gProtectiveCleanupOutcome::RejectedConflict => {
            Stage5gProtectiveCleanupSettlementState::Conflict
        }
    }
}

fn stage5g_cleanup_ledger_from_projection(
    projection: &crate::stage5c_paper_host::Stage5gProtectiveCleanupBatchRestartProjectionV1,
) -> Result<Stage5gProtectiveCleanupSettlementLedgerV1, Stage5gProtectiveBlockReason> {
    if projection.request_ids.len() != projection.records.len()
        || projection.batch_fingerprint
            != crate::stage5c_paper_host::stage5g_protective_cleanup_batch_projection_fingerprint(
                projection,
            )
    {
        return Err(Stage5gProtectiveBlockReason::ProtectiveRestartProjectionMismatch);
    }
    let mut requests = Vec::with_capacity(projection.records.len());
    for (index, record) in projection.records.iter().enumerate() {
        if projection.request_ids.get(index) != Some(&record.request_id) {
            return Err(Stage5gProtectiveBlockReason::ProtectiveRestartProjectionMismatch);
        }
        requests.push(Stage5gProtectiveCleanupRequestSettlementV1 {
            request_id: record.request_id,
            source_event_ts: record.source_event_ts,
            base_action: stage5g_cleanup_action_name(record.base_action).to_string(),
            target_protective_id: record.target_protective_id.clone(),
            expected_attribution: record.expected_attribution.clone(),
            state: Stage5gProtectiveCleanupSettlementState::Pending,
            outcome: None,
            cleanup_observation_fingerprint_sha256: None,
            settlement_fingerprint_sha256: None,
        });
    }
    let mut ledger = Stage5gProtectiveCleanupSettlementLedgerV1 {
        schema_version: 1,
        batch_fingerprint_sha256: projection.batch_fingerprint.clone(),
        requests,
        ledger_fingerprint_sha256: String::new(),
    };
    ledger.ledger_fingerprint_sha256 = stage5g_cleanup_ledger_fingerprint(&ledger);
    Ok(ledger)
}

fn stage5g_cleanup_ledger_fingerprint(
    ledger: &Stage5gProtectiveCleanupSettlementLedgerV1,
) -> String {
    #[derive(Serialize)]
    struct LedgerPayload<'a> {
        schema_version: u16,
        batch_fingerprint_sha256: &'a str,
        requests: &'a [Stage5gProtectiveCleanupRequestSettlementV1],
    }
    semantic_sha256(&LedgerPayload {
        schema_version: ledger.schema_version,
        batch_fingerprint_sha256: &ledger.batch_fingerprint_sha256,
        requests: &ledger.requests,
    })
}

fn stage5g_cleanup_observation_fingerprint(
    scenario: Option<Stage5gProtectiveScenarioId>,
    leg: Option<Stage5gProtectiveLeg>,
    authority_summary: &Stage5gProtectiveAuthoritySummary,
    evidence: &Stage5gProtectiveCleanupSettlementEvidence,
) -> String {
    semantic_sha256(&(
        "stage5g-f-r6-stable-cleanup-observation",
        scenario,
        leg,
        authority_summary.strategy_id.as_str(),
        &authority_summary.account_id,
        &authority_summary.instrument,
        authority_summary.active_cycle_id.as_str(),
        authority_summary.protected_position_side,
        evidence.batch_fingerprint_sha256.as_str(),
        evidence.request_id,
        evidence.target_protective_id.as_str(),
        evidence.base_action.as_str(),
        &evidence.expected_attribution,
        evidence.outcome,
        evidence.received_ts_utc,
    ))
}

fn stage5g_cleanup_ledger_is_valid(
    ledger: &Stage5gProtectiveCleanupSettlementLedgerV1,
    projection: &crate::stage5c_paper_host::Stage5gProtectiveCleanupBatchRestartProjectionV1,
) -> bool {
    ledger.schema_version == 1
        && ledger.batch_fingerprint_sha256 == projection.batch_fingerprint
        && ledger.ledger_fingerprint_sha256 == stage5g_cleanup_ledger_fingerprint(ledger)
        && ledger.requests.len() == projection.records.len()
        && ledger
            .requests
            .iter()
            .zip(projection.records.iter())
            .all(|(entry, record)| {
                entry.request_id == record.request_id
                    && entry.source_event_ts == record.source_event_ts
                    && entry.base_action == stage5g_cleanup_action_name(record.base_action)
                    && entry.target_protective_id == record.target_protective_id
                    && entry.expected_attribution == record.expected_attribution
            })
}

fn stage5g_pending_cleanup_authority_sha256(
    pending: &Stage5gProtectiveRestoredFlatCleanupPending,
) -> Result<String, Stage5gProtectiveBlockReason> {
    let cleanup_projection = pending
        .projection
        .cleanup_batch_restart_projection
        .as_ref()
        .ok_or(Stage5gProtectiveBlockReason::ProtectiveRestartProjectionMismatch)?;
    if !stage5g_cleanup_ledger_is_valid(&pending.cleanup_settlement_ledger, cleanup_projection) {
        return Err(Stage5gProtectiveBlockReason::ProtectiveRestartProjectionMismatch);
    }
    Ok(semantic_sha256(&(
        "stage5g-f-r5-pending-cleanup-authority",
        &pending.projection.replay_protection_fingerprint_sha256,
        &cleanup_projection.batch_fingerprint,
        &pending.cleanup_settlement_ledger.ledger_fingerprint_sha256,
        &pending.projection.post_runtime_semantic_fingerprint_sha256,
        &pending.projection.authority_summary.strategy_id,
        &pending.projection.authority_summary.account_id,
        &pending.projection.authority_summary.instrument,
        pending.projection.scenario,
        pending.projection.leg,
    )))
}

fn stage5g_cleanup_ledger_all_terminal_non_execution(
    ledger: &Stage5gProtectiveCleanupSettlementLedgerV1,
) -> bool {
    !ledger.requests.is_empty()
        && ledger.requests.iter().all(|request| {
            request.state == Stage5gProtectiveCleanupSettlementState::TerminalNonExecution
        })
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

pub fn stage5g_protective_restart_source_from_transition(
    transition: Stage5gProtectiveCompletionTransition,
) -> Result<Stage5gProtectiveRestartSource, Stage5gProtectiveBlockReason> {
    match transition {
        Stage5gProtectiveCompletionTransition::AwaitingPositionTruth(awaiting) => {
            restart_source_from_awaiting(awaiting)
        }
        Stage5gProtectiveCompletionTransition::FlatCleanupPending(pending) => {
            restart_source_from_flat_cleanup_pending(*pending)
        }
        Stage5gProtectiveCompletionTransition::Completed(completed) => {
            restart_source_from_completed(*completed)
        }
        Stage5gProtectiveCompletionTransition::Blocked(_) => {
            Err(Stage5gProtectiveBlockReason::ProtectiveRestartProjectionMismatch)
        }
    }
}

pub fn restore_stage5g_protective_completion_continuation(
    restart: crate::Stage5gCleanRestartedCapability,
) -> Result<Stage5gProtectiveRestoredContinuation, Stage5gProtectiveBlockReason> {
    let parts = restart
        .into_stage5g_protective_completion_authority_parts()
        .ok_or(Stage5gProtectiveBlockReason::MissingCleanRestartProtectiveState)?;
    let Some(projection) = parts.protective_projection.clone() else {
        let authority = admit_stage5g_protective_completion_authority_from_source(
            parts.input,
            Some(parts.runtime),
            Some(parts.restart_seed),
        )?;
        return Ok(Stage5gProtectiveRestoredContinuation::PreExecutionReady(
            authority,
        ));
    };
    validate_stage5g_protective_restart_projection(
        &projection,
        &parts.input.strategy_id,
        &parts.input.account_id,
        &parts.input.instrument,
        &runtime_stage5c_state_fingerprint(&parts.runtime)?,
    )?;
    match projection.projection_kind {
        Stage5gProtectiveRestartProjectionKind::PreExecutionReady => {
            let authority = admit_stage5g_protective_completion_authority_from_source(
                parts.input,
                Some(parts.runtime),
                Some(parts.restart_seed),
            )?;
            Ok(Stage5gProtectiveRestoredContinuation::PreExecutionReady(
                authority,
            ))
        }
        Stage5gProtectiveRestartProjectionKind::AwaitingPositionTruth => {
            let mut authority = admit_stage5g_protective_completion_authority_from_source(
                parts.input,
                Some(parts.runtime),
                Some(parts.restart_seed),
            )?;
            authority.accepted_receipts = projection
                .receipt_ledger
                .iter()
                .map(|receipt| Stage5gProtectiveEvidenceReceipt {
                    identity: receipt.identity.clone(),
                    fingerprint_sha256: receipt.fingerprint_sha256.clone(),
                    disposition: receipt.disposition,
                })
                .collect();
            let execution_receipt = authority
                .accepted_receipts
                .last()
                .cloned()
                .ok_or(Stage5gProtectiveBlockReason::ProtectiveRestartProjectionMismatch)?;
            Ok(
                Stage5gProtectiveRestoredContinuation::AwaitingPositionTruth(
                    Stage5gProtectiveAwaitingPositionTruth {
                        scenario: projection.scenario.ok_or(
                            Stage5gProtectiveBlockReason::ProtectiveRestartProjectionMismatch,
                        )?,
                        leg: projection.leg.ok_or(
                            Stage5gProtectiveBlockReason::ProtectiveRestartProjectionMismatch,
                        )?,
                        authority,
                        execution_receipt,
                        reason: projection
                            .position_truth_disposition
                            .unwrap_or(Stage5gProtectiveBlockReason::PositionTruthIncomplete),
                    },
                ),
            )
        }
        Stage5gProtectiveRestartProjectionKind::FlatCleanupPending => {
            let post_state = Stage5gProtectiveCommittedState::new(parts.runtime);
            let cleanup_projection = projection
                .cleanup_batch_restart_projection
                .as_ref()
                .ok_or(Stage5gProtectiveBlockReason::ProtectiveRestartProjectionMismatch)?;
            let generated_cleanup_batch =
                crate::stage5c_paper_host::restore_stage5g_protective_cleanup_batch_from_projection(
                    cleanup_projection,
                    &parts.input.strategy_id,
                    &parts.input.account_id,
                    &parts.input.instrument,
                )
                .map_err(|_| Stage5gProtectiveBlockReason::ProtectiveRestartProjectionMismatch)?;
            let cleanup_settlement_ledger = projection
                .cleanup_settlement_ledger
                .clone()
                .ok_or(Stage5gProtectiveBlockReason::ProtectiveRestartProjectionMismatch)?;
            if !stage5g_cleanup_ledger_is_valid(&cleanup_settlement_ledger, cleanup_projection) {
                return Err(Stage5gProtectiveBlockReason::ProtectiveRestartProjectionMismatch);
            }
            let generated_cleanup_batch_summary = crate::Stage5cPaperIntentBatchSummary {
                strategy_id: cleanup_projection.strategy_id.clone(),
                account_id: cleanup_projection.account_id.clone(),
                instrument: cleanup_projection.instrument.clone(),
                origin_bar_close_ts: cleanup_projection.origin_bar_close_ts,
                bar_close_ts: cleanup_projection.bar_close_ts,
                min_source_event_ts: cleanup_projection
                    .records
                    .iter()
                    .map(|record| record.source_event_ts)
                    .min()
                    .unwrap_or(cleanup_projection.bar_close_ts),
                max_source_event_ts: cleanup_projection
                    .records
                    .iter()
                    .map(|record| record.source_event_ts)
                    .max()
                    .unwrap_or(cleanup_projection.bar_close_ts),
                state_fingerprint: cleanup_projection.state_fingerprint.clone(),
                request_ids: cleanup_projection.request_ids.clone(),
                intent_count: cleanup_projection.records.len(),
                observation_only: cleanup_projection.observation_only,
            };
            let settled_batch_history = vec![generated_cleanup_batch_summary.clone()];
            Ok(Stage5gProtectiveRestoredContinuation::FlatCleanupPending(
                Box::new(Stage5gProtectiveRestoredFlatCleanupPending {
                    projection,
                    post_state,
                    generated_cleanup_batch,
                    generated_cleanup_batch_summary,
                    settled_batch_history,
                    cleanup_settlement_ledger,
                    restart_seed: Some(parts.restart_seed),
                }),
            ))
        }
        Stage5gProtectiveRestartProjectionKind::Completed => {
            let post_state = Stage5gProtectiveCommittedState::new(parts.runtime);
            Ok(Stage5gProtectiveRestoredContinuation::Completed(
                Stage5gProtectiveRestoredCompleted {
                    projection,
                    post_state,
                },
            ))
        }
    }
}

pub fn prepare_stage5g_protective_completion(
    restart: crate::Stage5gCleanRestartedCapability,
) -> Result<Stage5gProtectiveCompletionAuthority, Stage5gProtectiveBlockReason> {
    let parts = restart
        .into_stage5g_protective_completion_authority_parts()
        .ok_or(Stage5gProtectiveBlockReason::MissingCleanRestartProtectiveState)?;
    match parts.restart_seed.lifecycle_kind {
        crate::Stage5gCleanRestartLifecycleKind::TimerReady
        | crate::Stage5gCleanRestartLifecycleKind::OrderPositionAwaitingCommitted
        | crate::Stage5gCleanRestartLifecycleKind::ProtectiveLifecycleCommitted => {}
    }
    let mut authority = admit_stage5g_protective_completion_authority_from_source(
        parts.input,
        Some(parts.runtime),
        Some(parts.restart_seed),
    )?;
    if let Some(projection) = parts.protective_projection {
        match projection.projection_kind {
            Stage5gProtectiveRestartProjectionKind::PreExecutionReady => {}
            Stage5gProtectiveRestartProjectionKind::AwaitingPositionTruth => {
                authority.accepted_receipts = projection
                    .receipt_ledger
                    .iter()
                    .map(|receipt| Stage5gProtectiveEvidenceReceipt {
                        identity: receipt.identity.clone(),
                        fingerprint_sha256: receipt.fingerprint_sha256.clone(),
                        disposition: receipt.disposition,
                    })
                    .collect();
            }
            Stage5gProtectiveRestartProjectionKind::FlatCleanupPending
            | Stage5gProtectiveRestartProjectionKind::Completed => {
                return Err(Stage5gProtectiveBlockReason::UnsupportedCleanRestartLifecycleKind);
            }
        }
    }
    Ok(authority)
}

#[cfg(test)]
pub(crate) fn admit_stage5g_protective_completion_authority(
    input: Stage5gProtectiveCompletionAuthorityInput,
) -> Result<Stage5gProtectiveCompletionAuthority, Stage5gProtectiveBlockReason> {
    admit_stage5g_protective_completion_authority_from_source(input, None, None)
}

fn admit_stage5g_protective_completion_authority_from_source(
    input: Stage5gProtectiveCompletionAuthorityInput,
    runtime: Option<crate::HybridIntradayRuntimeStrategy>,
    restart_seed: Option<Stage5gProtectiveRestartSeed>,
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
        restart_seed,
    })
}

pub fn issue_stage5g_canonical_protective_evidence(
    authority: &Stage5gProtectiveCompletionAuthority,
    accepted: Stage5gAcceptedProtectiveBrokerTruth,
) -> Result<Stage5gValidatedProtectiveEvidence, Stage5gProtectiveBlockReason> {
    if accepted.canonical_authority_fingerprint_sha256
        != authority.summary().authority_fingerprint_sha256
        || !sha256_like(&accepted.broker_truth_fingerprint_sha256)
    {
        return Err(Stage5gProtectiveBlockReason::CanonicalBrokerTruthMismatch);
    }
    validate_evidence(authority, &accepted.evidence)?;
    validate_preexisting_sibling_terminal(authority, &accepted.evidence)?;
    Ok(Stage5gValidatedProtectiveEvidence {
        evidence_fingerprint_sha256: accepted.broker_truth_fingerprint_sha256,
        evidence: accepted.evidence,
    })
}

#[allow(
    dead_code,
    reason = "Stage 5G-f R4 keeps this production crate-private issuer sealed until the next gated lifecycle caller is introduced"
)]
pub(crate) fn accept_stage5g_canonical_protective_broker_truth(
    authority: &Stage5gProtectiveCompletionAuthority,
    evidence: Stage5gProtectiveCompletionEvidence,
) -> Result<Stage5gAcceptedProtectiveBrokerTruth, Stage5gProtectiveBlockReason> {
    validate_evidence(authority, &evidence)?;
    validate_preexisting_sibling_terminal(authority, &evidence)?;
    let sibling_terminal_authorized = evidence.sibling_terminal.is_some();
    let position_section_complete = evidence.position_truth.positions_complete;
    Ok(Stage5gAcceptedProtectiveBrokerTruth {
        canonical_authority_fingerprint_sha256: authority.summary().authority_fingerprint_sha256,
        broker_truth_fingerprint_sha256: semantic_sha256(&evidence),
        position_section_complete,
        sibling_terminal_authorized,
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
        let cleanup_batch_restart_projection =
            crate::stage5c_paper_host::stage5g_protective_cleanup_batch_restart_projection(
                &generated_cleanup_batch,
            )
            .expect("Stage 5C generated cleanup batch is restart-projectable");
        let cleanup_settlement_ledger =
            stage5g_cleanup_ledger_from_projection(&cleanup_batch_restart_projection)
                .expect("Stage 5C cleanup projection builds a settlement ledger");
        let cleanup_pending_fingerprint_sha256 = semantic_sha256(&(
            base_scenario,
            leg,
            &authority_summary,
            &completed_receipt,
            &generated_cleanup_batch_summary,
            &callback.settled_batch_history,
            &cleanup_settlement_ledger,
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
            cleanup_settlement_ledger,
            post_state: callback.post_state,
            generated_cleanup_batch,
            restart_seed: callback.restart_seed,
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
        cleanup_settlement_fingerprint_sha256: None,
        final_cleanup_settlement_ledger: None,
        final_cleanup_batch_restart_projection: None,
        completion_fingerprint_sha256,
        post_state: callback.post_state,
        restart_seed: callback.restart_seed,
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
    restart_seed: Option<Stage5gProtectiveRestartSeed>,
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
    let protected_position_qty = match authority.input.protected_position_qty.to_f64() {
        Some(value) if value.is_finite() => value,
        _ => {
            return Err(Box::new((
                authority,
                Stage5gProtectiveBlockReason::PostCallbackPositionNotIntegral,
            )));
        }
    };
    let pre_position_qty = match authority.input.protected_position_side {
        Stage5gProtectedPositionSide::Long => protected_position_qty,
        Stage5gProtectedPositionSide::Short => -protected_position_qty,
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
    let restart_seed = authority.restart_seed.clone();
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
        restart_seed,
    })
}

fn restart_source_from_awaiting(
    awaiting: Stage5gProtectiveAwaitingPositionTruth,
) -> Result<Stage5gProtectiveRestartSource, Stage5gProtectiveBlockReason> {
    let Stage5gProtectiveAwaitingPositionTruth {
        scenario,
        leg,
        authority,
        execution_receipt,
        reason,
    } = awaiting;
    let Stage5gProtectiveCompletionAuthority {
        input: _,
        runtime,
        accepted_receipts,
        restart_seed,
    } = authority;
    let runtime = runtime.ok_or(Stage5gProtectiveBlockReason::CanonicalCallbackFailed)?;
    let seed =
        restart_seed.ok_or(Stage5gProtectiveBlockReason::MissingCleanRestartProtectiveState)?;
    let authority_summary = Stage5gProtectiveAuthoritySummary {
        accepted_receipt_count: accepted_receipts.len(),
        ..authority_summary_from_input(
            &runtime
                .stage5g_protective_completion_authority_input(
                    seed.strategy_id.clone(),
                    seed.account_id.clone(),
                    seed.instrument.clone(),
                    seed.lifecycle_source_authority_sha256.clone(),
                    seed.parent_restart_package_fingerprint_sha256.clone(),
                    semantic_sha256(&seed.checkpoint),
                )
                .ok_or(Stage5gProtectiveBlockReason::MissingCleanRestartProtectiveState)?,
        )
    };
    let projection = protective_restart_projection(Stage5gProtectiveRestartProjectionInput {
        projection_kind: Stage5gProtectiveRestartProjectionKind::AwaitingPositionTruth,
        scenario: Some(scenario),
        disposition: Some(Stage5gProtectiveDisposition::AwaitingPositionTruth),
        leg: Some(leg),
        authority_summary,
        receipt_ledger: receipt_ledger_projection(&accepted_receipts),
        accepted_execution_identity: Some(execution_receipt.identity),
        accepted_execution_evidence_fingerprint_sha256: Some(execution_receipt.fingerprint_sha256),
        post_runtime_stage5c_state_fingerprint_sha256: runtime_stage5c_state_fingerprint(&runtime)?,
        post_runtime_semantic_fingerprint_sha256: None,
        generated_cleanup_batch_fingerprint_sha256: None,
        cleanup_batch_restart_projection: None,
        settled_batch_history_fingerprint_sha256: None,
        cleanup_settlement_fingerprint_sha256: None,
        cleanup_settlement_ledger: None,
        completion_fingerprint_sha256: None,
        cleanup_sibling_identity: None,
        callback_count: 0,
        cleanup_pending: false,
        completed: false,
    })
    .with_position_truth_disposition(reason);
    Ok(source_from_seed(runtime, seed, projection))
}

fn restart_source_from_flat_cleanup_pending(
    pending: Stage5gProtectiveFlatCleanupPending,
) -> Result<Stage5gProtectiveRestartSource, Stage5gProtectiveBlockReason> {
    let seed = pending
        .restart_seed
        .ok_or(Stage5gProtectiveBlockReason::MissingCleanRestartProtectiveState)?;
    let runtime = pending.post_state.runtime;
    let cleanup_batch_restart_projection =
        crate::stage5c_paper_host::stage5g_protective_cleanup_batch_restart_projection(
            &pending.generated_cleanup_batch,
        )
        .map_err(|_| Stage5gProtectiveBlockReason::ProtectiveRestartProjectionMismatch)?;
    let generated_cleanup_batch_fingerprint_sha256 =
        Some(cleanup_batch_restart_projection.batch_fingerprint.clone());
    let settled_batch_history_fingerprint_sha256 =
        Some(semantic_sha256(&pending.settled_batch_history));
    let projection = protective_restart_projection(Stage5gProtectiveRestartProjectionInput {
        projection_kind: Stage5gProtectiveRestartProjectionKind::FlatCleanupPending,
        scenario: Some(pending.scenario),
        disposition: Some(Stage5gProtectiveDisposition::FlatCleanupPending),
        leg: Some(pending.leg),
        authority_summary: pending.authority_summary,
        receipt_ledger: receipt_ledger_projection(std::slice::from_ref(&pending.execution_receipt)),
        accepted_execution_identity: Some(pending.execution_receipt.identity),
        accepted_execution_evidence_fingerprint_sha256: Some(
            pending.execution_receipt.fingerprint_sha256,
        ),
        post_runtime_stage5c_state_fingerprint_sha256: runtime_stage5c_state_fingerprint(&runtime)?,
        post_runtime_semantic_fingerprint_sha256: Some(
            pending.post_callback_state_fingerprint_sha256,
        ),
        generated_cleanup_batch_fingerprint_sha256,
        cleanup_batch_restart_projection: Some(cleanup_batch_restart_projection),
        settled_batch_history_fingerprint_sha256,
        cleanup_settlement_fingerprint_sha256: None,
        cleanup_settlement_ledger: Some(pending.cleanup_settlement_ledger),
        completion_fingerprint_sha256: None,
        cleanup_sibling_identity: Some(cleanup_sibling_identity_from_summary(
            &pending.generated_cleanup_batch_summary,
        )),
        callback_count: pending.callback_count,
        cleanup_pending: true,
        completed: false,
    });
    Ok(source_from_seed(runtime, seed, projection))
}

#[allow(
    dead_code,
    reason = "R5 partial cleanup restart helper is exercised by tests and reserved for the next sealed lifecycle caller"
)]
fn restart_source_from_restored_flat_cleanup_pending(
    pending: Stage5gProtectiveRestoredFlatCleanupPending,
) -> Result<Stage5gProtectiveRestartSource, Stage5gProtectiveBlockReason> {
    let seed = pending
        .restart_seed
        .ok_or(Stage5gProtectiveBlockReason::MissingCleanRestartProtectiveState)?;
    let runtime = pending.post_state.runtime;
    validate_stage5g_protective_restart_projection(
        &pending.projection,
        &pending.projection.authority_summary.strategy_id,
        &pending.projection.authority_summary.account_id,
        &pending.projection.authority_summary.instrument,
        &pending
            .projection
            .post_runtime_stage5c_state_fingerprint_sha256,
    )?;
    Ok(source_from_seed(runtime, seed, pending.projection))
}

fn restart_source_from_completed(
    completed: Stage5gProtectiveCompleted,
) -> Result<Stage5gProtectiveRestartSource, Stage5gProtectiveBlockReason> {
    let seed = completed
        .restart_seed
        .ok_or(Stage5gProtectiveBlockReason::MissingCleanRestartProtectiveState)?;
    let runtime = completed.post_state.runtime;
    let final_cleanup_batch_restart_projection =
        completed.final_cleanup_batch_restart_projection.clone();
    let generated_cleanup_batch_fingerprint_sha256 = final_cleanup_batch_restart_projection
        .as_ref()
        .map(|projection| projection.batch_fingerprint.clone());
    let projection = protective_restart_projection(Stage5gProtectiveRestartProjectionInput {
        projection_kind: Stage5gProtectiveRestartProjectionKind::Completed,
        scenario: Some(completed.scenario),
        disposition: Some(Stage5gProtectiveDisposition::Completed),
        leg: Some(completed.leg),
        authority_summary: completed.authority_summary,
        receipt_ledger: receipt_ledger_projection(std::slice::from_ref(
            &completed.execution_receipt,
        )),
        accepted_execution_identity: Some(completed.execution_receipt.identity),
        accepted_execution_evidence_fingerprint_sha256: Some(
            completed.execution_receipt.fingerprint_sha256,
        ),
        post_runtime_stage5c_state_fingerprint_sha256: runtime_stage5c_state_fingerprint(&runtime)?,
        post_runtime_semantic_fingerprint_sha256: Some(
            completed.post_callback_state_fingerprint_sha256,
        ),
        generated_cleanup_batch_fingerprint_sha256,
        cleanup_batch_restart_projection: final_cleanup_batch_restart_projection,
        settled_batch_history_fingerprint_sha256: None,
        cleanup_settlement_fingerprint_sha256: completed.cleanup_settlement_fingerprint_sha256,
        cleanup_settlement_ledger: completed.final_cleanup_settlement_ledger,
        completion_fingerprint_sha256: Some(completed.completion_fingerprint_sha256),
        cleanup_sibling_identity: None,
        callback_count: completed.callback_count,
        cleanup_pending: false,
        completed: true,
    });
    Ok(source_from_seed(runtime, seed, projection))
}

fn source_from_seed(
    runtime: crate::HybridIntradayRuntimeStrategy,
    seed: Stage5gProtectiveRestartSeed,
    projection: Stage5gProtectiveRestartProjectionV1,
) -> Stage5gProtectiveRestartSource {
    debug_assert!(sha256_like(&seed.source_lifecycle_commit_sha256));
    Stage5gProtectiveRestartSource {
        runtime,
        strategy_id: seed.strategy_id,
        account_id: seed.account_id,
        instrument: seed.instrument,
        summary: seed.summary,
        checkpoint: seed.checkpoint,
        order_position_state: seed.order_position_state,
        projection,
    }
}

struct Stage5gProtectiveRestartProjectionInput {
    projection_kind: Stage5gProtectiveRestartProjectionKind,
    scenario: Option<Stage5gProtectiveScenarioId>,
    disposition: Option<Stage5gProtectiveDisposition>,
    leg: Option<Stage5gProtectiveLeg>,
    authority_summary: Stage5gProtectiveAuthoritySummary,
    receipt_ledger: Vec<Stage5gProtectiveReceiptLedgerProjection>,
    accepted_execution_identity: Option<String>,
    accepted_execution_evidence_fingerprint_sha256: Option<String>,
    post_runtime_stage5c_state_fingerprint_sha256: String,
    post_runtime_semantic_fingerprint_sha256: Option<String>,
    generated_cleanup_batch_fingerprint_sha256: Option<String>,
    cleanup_batch_restart_projection:
        Option<crate::stage5c_paper_host::Stage5gProtectiveCleanupBatchRestartProjectionV1>,
    settled_batch_history_fingerprint_sha256: Option<String>,
    cleanup_settlement_fingerprint_sha256: Option<String>,
    cleanup_settlement_ledger: Option<Stage5gProtectiveCleanupSettlementLedgerV1>,
    completion_fingerprint_sha256: Option<String>,
    cleanup_sibling_identity: Option<String>,
    callback_count: usize,
    cleanup_pending: bool,
    completed: bool,
}

fn protective_restart_projection(
    input: Stage5gProtectiveRestartProjectionInput,
) -> Stage5gProtectiveRestartProjectionV1 {
    let mut projection = Stage5gProtectiveRestartProjectionV1 {
        schema_version: STAGE5G_PROTECTIVE_RESTART_PROJECTION_SCHEMA_VERSION,
        projection_kind: input.projection_kind,
        scenario: input.scenario,
        disposition: input.disposition,
        leg: input.leg,
        authority_summary: input.authority_summary,
        receipt_ledger: input.receipt_ledger,
        accepted_execution_identity: input.accepted_execution_identity,
        accepted_execution_evidence_fingerprint_sha256: input
            .accepted_execution_evidence_fingerprint_sha256,
        position_truth_disposition: None,
        post_runtime_stage5c_state_fingerprint_sha256: input
            .post_runtime_stage5c_state_fingerprint_sha256,
        post_runtime_semantic_fingerprint_sha256: input.post_runtime_semantic_fingerprint_sha256,
        generated_cleanup_batch_fingerprint_sha256: input
            .generated_cleanup_batch_fingerprint_sha256,
        cleanup_batch_restart_projection: input.cleanup_batch_restart_projection,
        settled_batch_history_fingerprint_sha256: input.settled_batch_history_fingerprint_sha256,
        cleanup_settlement_fingerprint_sha256: input.cleanup_settlement_fingerprint_sha256,
        cleanup_settlement_ledger: input.cleanup_settlement_ledger,
        completion_fingerprint_sha256: input.completion_fingerprint_sha256,
        cleanup_sibling_identity: input.cleanup_sibling_identity,
        callback_count: input.callback_count,
        cleanup_pending: input.cleanup_pending,
        completed: input.completed,
        replay_protection_fingerprint_sha256: String::new(),
    };
    projection.replay_protection_fingerprint_sha256 =
        protective_projection_fingerprint(&projection);
    projection
}

trait Stage5gProtectiveRestartProjectionExt {
    fn with_position_truth_disposition(
        self,
        reason: Stage5gProtectiveBlockReason,
    ) -> Stage5gProtectiveRestartProjectionV1;
}

impl Stage5gProtectiveRestartProjectionExt for Stage5gProtectiveRestartProjectionV1 {
    fn with_position_truth_disposition(
        mut self,
        reason: Stage5gProtectiveBlockReason,
    ) -> Stage5gProtectiveRestartProjectionV1 {
        self.position_truth_disposition = Some(reason);
        self.replay_protection_fingerprint_sha256 = protective_projection_fingerprint(&self);
        self
    }
}

fn protective_projection_fingerprint(projection: &Stage5gProtectiveRestartProjectionV1) -> String {
    #[derive(Serialize)]
    struct ReplayProtectionProjection<'a> {
        schema_version: u16,
        projection_kind: Stage5gProtectiveRestartProjectionKind,
        scenario: Option<Stage5gProtectiveScenarioId>,
        disposition: Option<Stage5gProtectiveDisposition>,
        leg: Option<Stage5gProtectiveLeg>,
        authority_summary: &'a Stage5gProtectiveAuthoritySummary,
        receipt_ledger: &'a [Stage5gProtectiveReceiptLedgerProjection],
        accepted_execution_identity: &'a Option<String>,
        accepted_execution_evidence_fingerprint_sha256: &'a Option<String>,
        position_truth_disposition: Option<Stage5gProtectiveBlockReason>,
        post_runtime_stage5c_state_fingerprint_sha256: &'a str,
        post_runtime_semantic_fingerprint_sha256: &'a Option<String>,
        generated_cleanup_batch_fingerprint_sha256: &'a Option<String>,
        cleanup_batch_restart_projection:
            &'a Option<crate::stage5c_paper_host::Stage5gProtectiveCleanupBatchRestartProjectionV1>,
        settled_batch_history_fingerprint_sha256: &'a Option<String>,
        cleanup_settlement_fingerprint_sha256: &'a Option<String>,
        cleanup_settlement_ledger: &'a Option<Stage5gProtectiveCleanupSettlementLedgerV1>,
        completion_fingerprint_sha256: &'a Option<String>,
        cleanup_sibling_identity: &'a Option<String>,
        callback_count: usize,
        cleanup_pending: bool,
        completed: bool,
    }

    semantic_sha256(&ReplayProtectionProjection {
        schema_version: projection.schema_version,
        projection_kind: projection.projection_kind,
        scenario: projection.scenario,
        disposition: projection.disposition,
        leg: projection.leg,
        authority_summary: &projection.authority_summary,
        receipt_ledger: &projection.receipt_ledger,
        accepted_execution_identity: &projection.accepted_execution_identity,
        accepted_execution_evidence_fingerprint_sha256: &projection
            .accepted_execution_evidence_fingerprint_sha256,
        position_truth_disposition: projection.position_truth_disposition,
        post_runtime_stage5c_state_fingerprint_sha256: &projection
            .post_runtime_stage5c_state_fingerprint_sha256,
        post_runtime_semantic_fingerprint_sha256: &projection
            .post_runtime_semantic_fingerprint_sha256,
        generated_cleanup_batch_fingerprint_sha256: &projection
            .generated_cleanup_batch_fingerprint_sha256,
        cleanup_batch_restart_projection: &projection.cleanup_batch_restart_projection,
        settled_batch_history_fingerprint_sha256: &projection
            .settled_batch_history_fingerprint_sha256,
        cleanup_settlement_fingerprint_sha256: &projection.cleanup_settlement_fingerprint_sha256,
        cleanup_settlement_ledger: &projection.cleanup_settlement_ledger,
        completion_fingerprint_sha256: &projection.completion_fingerprint_sha256,
        cleanup_sibling_identity: &projection.cleanup_sibling_identity,
        callback_count: projection.callback_count,
        cleanup_pending: projection.cleanup_pending,
        completed: projection.completed,
    })
}

fn receipt_ledger_projection(
    receipts: &[Stage5gProtectiveEvidenceReceipt],
) -> Vec<Stage5gProtectiveReceiptLedgerProjection> {
    receipts
        .iter()
        .map(|receipt| Stage5gProtectiveReceiptLedgerProjection {
            identity: receipt.identity.clone(),
            fingerprint_sha256: receipt.fingerprint_sha256.clone(),
            disposition: receipt.disposition,
        })
        .collect()
}

fn cleanup_sibling_identity_from_summary(
    summary: &crate::Stage5cPaperIntentBatchSummary,
) -> String {
    semantic_sha256(&(
        "moex.stage5g-f.cleanup-sibling-identity.v1",
        &summary.strategy_id,
        &summary.account_id,
        &summary.instrument,
        &summary.request_ids,
    ))
}

pub(crate) fn validate_stage5g_protective_restart_projection(
    projection: &Stage5gProtectiveRestartProjectionV1,
    strategy_id: &str,
    account_id: &BrokerAccountId,
    instrument: &InstrumentId,
    runtime_stage5c_state_fingerprint_sha256: &str,
) -> Result<(), Stage5gProtectiveBlockReason> {
    if projection.schema_version != STAGE5G_PROTECTIVE_RESTART_PROJECTION_SCHEMA_VERSION
        || projection.authority_summary.schema_version
            != STAGE5G_PROTECTIVE_COMPLETION_SCHEMA_VERSION
        || projection.authority_summary.strategy_id != strategy_id
        || &projection.authority_summary.account_id != account_id
        || &projection.authority_summary.instrument != instrument
        || projection.post_runtime_stage5c_state_fingerprint_sha256
            != runtime_stage5c_state_fingerprint_sha256
        || projection.replay_protection_fingerprint_sha256
            != protective_projection_fingerprint(projection)
    {
        return Err(Stage5gProtectiveBlockReason::ProtectiveRestartProjectionMismatch);
    }
    if !sha256_like(&projection.authority_summary.authority_fingerprint_sha256)
        || !stage5c_semantic_fingerprint_like(
            &projection.post_runtime_stage5c_state_fingerprint_sha256,
        )
    {
        return Err(Stage5gProtectiveBlockReason::ProtectiveRestartProjectionMismatch);
    }
    if let Some(fingerprint) = &projection.accepted_execution_evidence_fingerprint_sha256 {
        if !sha256_like(fingerprint) {
            return Err(Stage5gProtectiveBlockReason::ProtectiveRestartProjectionMismatch);
        }
    }
    if let Some(fingerprint) = &projection.post_runtime_semantic_fingerprint_sha256 {
        if !stage5c_semantic_fingerprint_like(fingerprint) {
            return Err(Stage5gProtectiveBlockReason::ProtectiveRestartProjectionMismatch);
        }
    }
    if projection
        .receipt_ledger
        .iter()
        .any(|receipt| receipt.identity.is_empty() || !sha256_like(&receipt.fingerprint_sha256))
    {
        return Err(Stage5gProtectiveBlockReason::ProtectiveRestartProjectionMismatch);
    }
    match projection.projection_kind {
        Stage5gProtectiveRestartProjectionKind::PreExecutionReady => {
            if projection.scenario.is_some()
                || projection.disposition.is_some()
                || projection.leg.is_some()
                || !projection.receipt_ledger.is_empty()
                || projection.cleanup_batch_restart_projection.is_some()
                || projection.cleanup_settlement_fingerprint_sha256.is_some()
                || projection.cleanup_settlement_ledger.is_some()
                || projection.completion_fingerprint_sha256.is_some()
                || projection.cleanup_pending
                || projection.completed
            {
                return Err(Stage5gProtectiveBlockReason::ProtectiveRestartProjectionMismatch);
            }
        }
        Stage5gProtectiveRestartProjectionKind::AwaitingPositionTruth => {
            if projection.disposition != Some(Stage5gProtectiveDisposition::AwaitingPositionTruth)
                || projection.receipt_ledger.is_empty()
                || projection.callback_count != 0
                || projection.cleanup_batch_restart_projection.is_some()
                || projection.cleanup_settlement_fingerprint_sha256.is_some()
                || projection.cleanup_settlement_ledger.is_some()
                || projection.completion_fingerprint_sha256.is_some()
                || projection.cleanup_pending
                || projection.completed
            {
                return Err(Stage5gProtectiveBlockReason::ProtectiveRestartProjectionMismatch);
            }
        }
        Stage5gProtectiveRestartProjectionKind::FlatCleanupPending => {
            if projection.disposition != Some(Stage5gProtectiveDisposition::FlatCleanupPending)
                || projection
                    .generated_cleanup_batch_fingerprint_sha256
                    .is_none()
                || projection.cleanup_batch_restart_projection.is_none()
                || projection
                    .settled_batch_history_fingerprint_sha256
                    .is_none()
                || projection.cleanup_settlement_fingerprint_sha256.is_some()
                || projection.cleanup_settlement_ledger.is_none()
                || projection.completion_fingerprint_sha256.is_some()
                || !projection.cleanup_pending
                || projection.completed
            {
                return Err(Stage5gProtectiveBlockReason::ProtectiveRestartProjectionMismatch);
            }
            let cleanup_projection = projection
                .cleanup_batch_restart_projection
                .as_ref()
                .ok_or(Stage5gProtectiveBlockReason::ProtectiveRestartProjectionMismatch)?;
            if projection.generated_cleanup_batch_fingerprint_sha256.as_deref()
                != Some(cleanup_projection.batch_fingerprint.as_str())
                || cleanup_projection.batch_fingerprint
                    != crate::stage5c_paper_host::stage5g_protective_cleanup_batch_projection_fingerprint(
                        cleanup_projection,
                    )
            {
                return Err(Stage5gProtectiveBlockReason::ProtectiveRestartProjectionMismatch);
            }
            let cleanup_ledger = projection
                .cleanup_settlement_ledger
                .as_ref()
                .ok_or(Stage5gProtectiveBlockReason::ProtectiveRestartProjectionMismatch)?;
            if !stage5g_cleanup_ledger_is_valid(cleanup_ledger, cleanup_projection) {
                return Err(Stage5gProtectiveBlockReason::ProtectiveRestartProjectionMismatch);
            }
        }
        Stage5gProtectiveRestartProjectionKind::Completed => {
            if projection.disposition != Some(Stage5gProtectiveDisposition::Completed)
                || projection
                    .completion_fingerprint_sha256
                    .as_deref()
                    .map_or(true, |fingerprint| !sha256_like(fingerprint))
                || projection.cleanup_pending
                || !projection.completed
            {
                return Err(Stage5gProtectiveBlockReason::ProtectiveRestartProjectionMismatch);
            }
            match (
                projection.cleanup_settlement_fingerprint_sha256.as_deref(),
                projection.cleanup_settlement_ledger.as_ref(),
                projection.cleanup_batch_restart_projection.as_ref(),
                projection
                    .generated_cleanup_batch_fingerprint_sha256
                    .as_deref(),
            ) {
                (None, None, None, None) => {}
                (Some(fingerprint), Some(ledger), Some(batch), Some(batch_fingerprint))
                    if fingerprint == ledger.ledger_fingerprint_sha256
                        && batch_fingerprint == batch.batch_fingerprint
                        && stage5g_cleanup_ledger_is_valid(ledger, batch)
                        && stage5g_cleanup_ledger_all_terminal_non_execution(ledger) => {}
                _ => {
                    return Err(Stage5gProtectiveBlockReason::ProtectiveRestartProjectionMismatch);
                }
            }
        }
    }
    Ok(())
}

fn authority_summary_from_input(
    input: &Stage5gProtectiveCompletionAuthorityInput,
) -> Stage5gProtectiveAuthoritySummary {
    Stage5gProtectiveAuthoritySummary {
        schema_version: STAGE5G_PROTECTIVE_COMPLETION_SCHEMA_VERSION,
        strategy_id: input.strategy_id.clone(),
        account_id: input.account_id.clone(),
        instrument: input.instrument.clone(),
        tick_size: input.tick_size,
        active_cycle_id: input.active_cycle_id.clone().unwrap_or_default(),
        protected_position_side: input.protected_position_side,
        protected_position_qty: input.protected_position_qty,
        tp_order_id: input.tp_order_id.clone(),
        sl_stop_order_id: input.sl_stop_order_id.clone(),
        sl_exchange_order_id: input.sl_exchange_order_id.clone(),
        protective_created_ts_utc: input.protective_created_ts_utc,
        last_lifecycle_checkpoint_ts_utc: input.last_lifecycle_checkpoint_ts_utc,
        accepted_receipt_count: 0,
        authority_fingerprint_sha256: semantic_sha256(input),
    }
}

fn runtime_stage5c_state_fingerprint(
    runtime: &crate::HybridIntradayRuntimeStrategy,
) -> Result<String, Stage5gProtectiveBlockReason> {
    let state = serde_json::to_value(crate::runtime_compat::Strategy::state(runtime))
        .map_err(|_| Stage5gProtectiveBlockReason::ProtectiveRestartProjectionMismatch)?;
    crate::stage5c_paper_host::stage5c_semantic_value_fingerprint(&state)
        .map_err(|_| Stage5gProtectiveBlockReason::ProtectiveRestartProjectionMismatch)
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
        &completed.cleanup_settlement_fingerprint_sha256,
        &completed.final_cleanup_settlement_ledger,
        &completed.final_cleanup_batch_restart_projection,
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

fn stage5c_semantic_fingerprint_like(value: &str) -> bool {
    value
        .strip_prefix("stage5c_semantic_sha256:")
        .is_some_and(sha256_like)
        || sha256_like(value)
}

#[cfg(test)]
mod tests {
    use std::thread;

    use broker_core::{BrokerAccountId, Exchange, Market};
    use chrono::{Duration, TimeZone, Utc};
    use sha2::{Digest, Sha256};

    use super::*;

    const STRATEGY_ID: &str = "hybrid_imoexf";
    const CYCLE_ID: &str = "abc1230001";
    const SHA: &str = "14816e91784a75dfd6bf9a5d3d12b60b7fc732c7abb2ec92d26cdca8f493afbf";

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
            crate::HybridIntradayRuntimeStrategy::stage5g_artifact_mr_protective_runtime_fixture(
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
        admit_stage5g_protective_completion_authority_from_source(input, Some(runtime), None)
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

    fn timer_checkpoint() -> crate::Stage5gTimerCheckpointEnvelope {
        let received_at = ts(1_800_000_020);
        let package_discriminator = format!(
            "moex.broker-truth.package.v1:{}:{:09}",
            received_at.timestamp(),
            received_at.timestamp_subsec_nanos()
        );
        let evidence_identity = format!(
            "moex.stage5g.order-position-evidence-identity.v3:00000000-0000-4000-8000-000000000001:{}:{}",
            account().0,
            package_discriminator
        );
        let payload = crate::Stage5gTimerCheckpointPayload {
            schema_version: crate::STAGE5G_TIMER_CHECKPOINT_SCHEMA_VERSION,
            package_discriminator: Some(package_discriminator),
            current_evidence_identity: Some(evidence_identity.clone()),
            evidence_replay_ledger: vec![crate::Stage5gReplayLedgerEntry {
                identity: evidence_identity,
                fingerprint_sha256: SHA.to_string(),
            }],
            last_broker_truth_received_at: Some(received_at),
            last_broker_truth_received_ms: Some(1_800_000_020_000),
            duplicate_evidence_count: 0,
            last_total_sequence: Some(1),
            last_continuation_checkpoint_ts_utc_ms: Some(1_800_000_020_001),
        };
        crate::Stage5gTimerCheckpointEnvelope {
            payload_sha256: timer_checkpoint_payload_fingerprint(&payload),
            payload,
        }
    }

    fn timer_checkpoint_payload_fingerprint(
        payload: &crate::Stage5gTimerCheckpointPayload,
    ) -> String {
        let mut hasher = Sha256::new();
        hasher.update(b"moex.stage5g.timer-checkpoint.v1\0");
        hasher
            .update(serde_json::to_vec(payload).expect("test timer checkpoint payload serializes"));
        format!("{:x}", hasher.finalize())
    }

    fn order_position_summary() -> crate::Stage5gOrderPositionSummary {
        crate::Stage5gOrderPositionSummary {
            schema_version: crate::STAGE5G_ORDER_POSITION_SCHEMA_VERSION,
            strategy_id: STRATEGY_ID.to_string(),
            request_count: 0,
            terminal_request_count: 0,
            order_transition_count: 0,
            correlated_trade_count: 0,
            position_confirmation_count: 0,
            duplicate_evidence_count: 0,
            last_total_sequence: Some(1),
            lifecycle_fingerprint_sha256: SHA.to_string(),
            stage5c_callback_count: 0,
            mock_feedback_only: true,
            redis_attached: false,
            finam_transport_attached: false,
            broker_execution_attached: false,
        }
    }

    fn protective_restart_source(
        side: Stage5gProtectedPositionSide,
    ) -> Stage5gProtectiveRestartSource {
        let (mut runtime, cycle_id) =
            crate::HybridIntradayRuntimeStrategy::stage5g_artifact_mr_protective_runtime_fixture(
                side,
                BrokerOrderId::new("TP_STAGE5G_F"),
                BrokerStopOrderId::new("SL_STOP_STAGE5G_F"),
                BrokerOrderId::new("SL_EXCHANGE_STAGE5G_F"),
            );
        assert_eq!(cycle_id, CYCLE_ID);
        set_protective_strategy_state(&mut runtime, side);
        let _ = crate::stage5d_persistence::stage5f_test_seams::prepare_stage5g_clean_restart_test_authority(
            &mut runtime,
            STRATEGY_ID,
            ts(1_800_000_030),
        );
        Stage5gProtectiveRestartSource::pre_execution_ready(
            runtime,
            STRATEGY_ID.to_string(),
            account(),
            instrument(),
            order_position_summary(),
            timer_checkpoint(),
        )
        .expect("source-owned protective restart source")
    }

    fn set_protective_strategy_state(
        runtime: &mut crate::HybridIntradayRuntimeStrategy,
        side: Stage5gProtectedPositionSide,
    ) {
        let (runtime_side, position_qty) = match side {
            Stage5gProtectedPositionSide::Long => (crate::hybrid_intraday::Side::Long, 3.0),
            Stage5gProtectedPositionSide::Short => (crate::hybrid_intraday::Side::Short, -3.0),
        };
        crate::runtime_compat::Strategy::set_state(
            runtime,
            crate::runtime_compat::StrategyState::HybridIntradayRuntime {
                active_cycle_id: Some(CYCLE_ID.to_string()),
                next_cycle_seq: 1,
                last_position_qty: position_qty,
                current_owner: Some(crate::hybrid_intraday::Owner::MeanReversion),
                current_side: Some(runtime_side),
                pending_entry_owner: None,
                pending_entry_side: None,
                pending_entry_cycle_id: None,
                pending_entry_request_id: None,
                pending_entry_created_ts_utc: None,
                deferred_entry_owner: None,
                deferred_entry_side: None,
                deferred_entry_cycle_id: None,
                deferred_entry_entry_style: None,
                deferred_entry_reason: None,
                deferred_entry_stop_price: None,
                deferred_entry_take_price: None,
                deferred_entry_ts_utc: None,
                deferred_entry_request_id: None,
                pending_exit_request_id: None,
                pending_exit_created_ts_utc: None,
                deferred_exit_owner: None,
                deferred_exit_reason: None,
                deferred_exit_cycle_id: None,
                deferred_exit_ts_utc: None,
                deferred_exit_request_id: None,
                pending_tp_request_id: None,
                pending_tp_created_ts_utc: None,
                pending_sl_request_id: None,
                pending_sl_created_ts_utc: None,
                tp_order_id: Some(BrokerOrderId::new("TP_STAGE5G_F")),
                sl_stop_order_id: Some(BrokerStopOrderId::new("SL_STOP_STAGE5G_F")),
                sl_exchange_order_id: Some(BrokerOrderId::new("SL_EXCHANGE_STAGE5G_F")),
                sl_triggered_ts: None,
                mr_take_price: Some(2250.0),
                mr_stop_price: Some(2190.0),
                repair_deadline_ts: None,
                next_repair_at_ts: None,
                repair_backoff_level: 0,
                repair_attempts: 0,
                safe_mode_close_only: false,
                safe_mode_reason: None,
                entry_ready: true,
                last_bar_close: Some(2220.0),
                prev_day_close: Some(2234.5),
                last_day_local: Some("2026-07-06".to_string()),
                current_day_high: Some(2262.0),
                current_day_low: Some(2165.5),
                current_day_close: Some(2220.0),
                prev_day_range: Some(60.5),
                prev_day_return: Some(0.0),
                day_before_close: Some(2234.5),
                today_start_local: Some("2026-07-06T09:00:00".to_string()),
                was_long_today: matches!(side, Stage5gProtectedPositionSide::Long),
                was_short_today: matches!(side, Stage5gProtectedPositionSide::Short),
                overnight_exit_armed_date: None,
                risk_gate_shadow_session_date: Some("2026-07-06".to_string()),
                risk_gate_shadow_pnl_points: 0.0,
                risk_gate_shadow_trade_count: 0,
                risk_gate_shadow_entry_ts_utc: None,
                risk_gate_shadow_entry_price: None,
                risk_gate_shadow_side: None,
                risk_gate_shadow_target_price: None,
                risk_gate_shadow_stop_price: None,
                risk_gate_pending_session_date: None,
                risk_gate_pending_shadow_pnl_points: 0.0,
                risk_gate_pending_shadow_trade_count: 0,
                risk_gate_mr_enabled_current_session: Some(true),
                risk_gate_rolling_sum_lb120: Some(158.6),
                risk_gate_last_finalized_session_date: Some("2026-07-03".to_string()),
                risk_gate_ledger_rows_count: 221,
            },
        );
    }

    fn stage5g_f_r3_commitment_key() -> crate::Stage5gLifecycleCommitmentKey {
        crate::Stage5gLifecycleCommitmentKey::from_secret_bytes(&[0x6d; 32])
            .expect("valid test key")
    }

    fn restore_protective_source(
        source: Stage5gProtectiveRestartSource,
        snapshot_id: &str,
    ) -> crate::Stage5gCleanRestartedCapability {
        let fresh_runtime = source
            .stage5g_runtime_strategy()
            .stage5g_clean_reconstruction_candidate();
        let source = crate::Stage5gCleanRestartSource::ProtectiveLifecycle(source);
        let persisted_at = ts(1_800_000_030);
        let (riskgate, riskgate_evidence) =
            crate::stage5g_clean_restart::stage5g_test_persistence_authority_from_source(
                &source,
                persisted_at,
            );
        let input = crate::Stage5gCleanRestartExportInput {
            snapshot_id: snapshot_id.to_string(),
            snapshot_revision: 1,
            previous_revision: None,
            write_generation: 1,
            persisted_at_ts_utc: persisted_at,
            source_commit_or_build_id:
                crate::stage5d_persistence::STAGE5D_RUNTIME_SEMANTIC_COMPATIBILITY_ID.to_string(),
            lifecycle_watermarks: crate::Stage5dLifecycleWatermarks {
                persisted_event_watermark: Some(snapshot_id.to_string()),
                last_semantic_bar_ts: Some(persisted_at - Duration::minutes(1)),
                last_broker_event_ts: Some(persisted_at - Duration::seconds(1)),
            },
            riskgate,
            riskgate_evidence,
        };
        let bytes =
            crate::export_stage5g_clean_restart(source, input, &stage5g_f_r3_commitment_key())
                .expect("authenticated protective clean restart export");
        crate::restore_stage5g_clean_restart(&bytes, &stage5g_f_r3_commitment_key(), fresh_runtime)
            .expect("authenticated protective clean restart restore")
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
        match accept_stage5g_canonical_protective_broker_truth(&authority, evidence)
            .and_then(|accepted| issue_stage5g_canonical_protective_evidence(&authority, accepted))
        {
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

    fn restored_flat_cleanup_pending(
        side: Stage5gProtectedPositionSide,
        leg: Stage5gProtectiveLeg,
        snapshot_id: &str,
    ) -> Box<Stage5gProtectiveRestoredFlatCleanupPending> {
        let authority = prepare_stage5g_protective_completion(restore_protective_source(
            protective_restart_source(side),
            snapshot_id,
        ))
        .expect("protective authority");
        let evidence = match leg {
            Stage5gProtectiveLeg::Target => target_evidence(side, "Filled", flat_position_truth()),
            Stage5gProtectiveLeg::Stop => stop_evidence(side, "Triggered", flat_position_truth()),
        };
        let pending = flat_cleanup_pending(apply(authority, evidence));
        let source = stage5g_protective_restart_source_from_transition(
            Stage5gProtectiveCompletionTransition::FlatCleanupPending(Box::new(pending)),
        )
        .expect("flat-cleanup transition is source-restartable");
        let restored = restore_stage5g_protective_completion_continuation(
            restore_protective_source(source, &format!("{snapshot_id}-flat-cleanup")),
        )
        .expect("flat cleanup continuation restores");
        let Stage5gProtectiveRestoredContinuation::FlatCleanupPending(restored) = restored else {
            panic!("expected flat cleanup continuation")
        };
        restored
    }

    fn cleanup_terminal_status(
        record: &crate::stage5c_paper_host::Stage5gProtectiveCleanupBatchRestartRecordV1,
    ) -> &'static str {
        match record.base_action {
            crate::stage5c_paper_host::Stage5gSourceBaseAction::DeleteStopLimit => "Deleted",
            crate::stage5c_paper_host::Stage5gSourceBaseAction::Cancel => "Canceled",
            _ => "Rejected",
        }
    }

    fn accept_cleanup_record(
        pending: &Stage5gProtectiveRestoredFlatCleanupPending,
        record_index: usize,
        status: &str,
    ) -> Stage5gAcceptedProtectiveCleanupTruth {
        let cleanup_record = pending
            .projection
            .cleanup_batch_restart_projection
            .as_ref()
            .expect("cleanup projection")
            .records[record_index]
            .clone();
        accept_stage5g_protective_cleanup_truth(
            pending,
            cleanup_record.request_id,
            cleanup_record.target_protective_id,
            status,
            cleanup_record.source_event_ts + 1,
        )
        .expect("cleanup truth accepted")
    }

    fn settle_cleanup_index(
        pending: Box<Stage5gProtectiveRestoredFlatCleanupPending>,
        record_index: usize,
    ) -> Stage5gProtectiveCleanupTransition {
        let cleanup_record = pending
            .projection
            .cleanup_batch_restart_projection
            .as_ref()
            .expect("cleanup projection")
            .records[record_index]
            .clone();
        let accepted = accept_stage5g_protective_cleanup_truth(
            &pending,
            cleanup_record.request_id,
            cleanup_record.target_protective_id.clone(),
            cleanup_terminal_status(&cleanup_record),
            cleanup_record.source_event_ts + 1,
        )
        .expect("cleanup truth accepted");
        apply_stage5g_protective_cleanup_completion(*pending, accepted)
    }

    fn complete_cleanup_in_order(
        mut pending: Box<Stage5gProtectiveRestoredFlatCleanupPending>,
        order: &[usize],
    ) -> Box<Stage5gProtectiveCompleted> {
        for (position, record_index) in order.iter().copied().enumerate() {
            let transition = settle_cleanup_index(pending, record_index);
            if position + 1 == order.len() {
                let Stage5gProtectiveCleanupTransition::Completed(completed) = transition else {
                    panic!("last cleanup settlement must complete")
                };
                return completed;
            }
            let Stage5gProtectiveCleanupTransition::FlatCleanupPending(next_pending) = transition
            else {
                panic!("intermediate cleanup settlement must remain pending")
            };
            pending = next_pending;
        }
        panic!("empty cleanup order")
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
    fn stage5g_f_r3_authenticated_restart_prepares_protective_authority_and_canonical_issuer() {
        let restored = restore_protective_source(
            protective_restart_source(Stage5gProtectedPositionSide::Long),
            "stage5g-f-r3-pre-execution",
        );
        assert_eq!(
            restored.lifecycle_kind(),
            crate::Stage5gCleanRestartLifecycleKind::ProtectiveLifecycleCommitted
        );
        assert!(!restored.redis_command_stream_attached());
        assert!(!restored.finam_transport_attached());
        assert!(!restored.broker_execution_attached());

        let authority =
            prepare_stage5g_protective_completion(restored).expect("protective authority");
        let accepted = accept_stage5g_canonical_protective_broker_truth(
            &authority,
            target_evidence(
                Stage5gProtectedPositionSide::Long,
                "Filled",
                flat_position_truth(),
            ),
        )
        .expect("canonical broker truth accepted by crate-private issuer");
        let validated = issue_stage5g_canonical_protective_evidence(&authority, accepted)
            .expect("production issuer consumes only canonical accepted truth");
        let transition = apply_stage5g_protective_completion(authority, validated);
        assert_eq!(
            transition.disposition(),
            Stage5gProtectiveDisposition::FlatCleanupPending
        );
        assert_eq!(
            transition.scenario(),
            Stage5gProtectiveScenarioId::Gprt01F12MrLongTargetCompletesFlat
        );
    }

    #[test]
    fn stage5g_f_r3_awaiting_position_truth_survives_authenticated_restart() {
        let authority = prepare_stage5g_protective_completion(restore_protective_source(
            protective_restart_source(Stage5gProtectedPositionSide::Long),
            "stage5g-f-r3-awaiting-source",
        ))
        .expect("protective authority");
        let transition = apply(
            authority,
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
        let source = stage5g_protective_restart_source_from_transition(transition)
            .expect("awaiting transition is source-restartable");
        let restored = restore_stage5g_protective_completion_continuation(
            restore_protective_source(source, "stage5g-f-r3-awaiting-roundtrip"),
        )
        .expect("awaiting continuation restores");
        let Stage5gProtectiveRestoredContinuation::AwaitingPositionTruth(awaiting) = restored
        else {
            panic!("expected awaiting continuation")
        };
        assert_eq!(
            awaiting.scenario,
            Stage5gProtectiveScenarioId::Gprt07TriggerWithoutFlatPositionBlocks
        );
        assert_eq!(awaiting.leg, Stage5gProtectiveLeg::Stop);
        assert_eq!(
            awaiting.reason,
            Stage5gProtectiveBlockReason::PositionNotFlat
        );
        assert_eq!(awaiting.authority.accepted_receipts.len(), 1);
    }

    #[test]
    fn stage5g_f_r3_flat_cleanup_pending_survives_authenticated_restart() {
        let authority = prepare_stage5g_protective_completion(restore_protective_source(
            protective_restart_source(Stage5gProtectedPositionSide::Short),
            "stage5g-f-r3-flat-source",
        ))
        .expect("protective authority");
        let pending = flat_cleanup_pending(apply(
            authority,
            stop_evidence(
                Stage5gProtectedPositionSide::Short,
                "Executed",
                flat_position_truth(),
            ),
        ));
        let source = stage5g_protective_restart_source_from_transition(
            Stage5gProtectiveCompletionTransition::FlatCleanupPending(Box::new(pending)),
        )
        .expect("flat-cleanup transition is source-restartable");
        let restored = restore_stage5g_protective_completion_continuation(
            restore_protective_source(source, "stage5g-f-r3-flat-roundtrip"),
        )
        .expect("flat cleanup continuation restores");
        let Stage5gProtectiveRestoredContinuation::FlatCleanupPending(restored) = restored else {
            panic!("expected flat cleanup continuation")
        };
        assert_eq!(
            restored.projection.projection_kind,
            Stage5gProtectiveRestartProjectionKind::FlatCleanupPending
        );
        assert!(restored.projection.cleanup_pending);
        assert!(!restored.projection.completed);
        assert_eq!(
            restored.post_state.summary().final_position_qty,
            Decimal::ZERO
        );
        assert!(!restored.post_state.runtime_live_attached());
    }

    #[test]
    fn stage5g_f_r5_multi_request_cleanup_settles_only_after_all_requests() {
        let authority = prepare_stage5g_protective_completion(restore_protective_source(
            protective_restart_source(Stage5gProtectedPositionSide::Long),
            "stage5g-f-r4-flat-source",
        ))
        .expect("protective authority");
        let pending = flat_cleanup_pending(apply(
            authority,
            target_evidence(
                Stage5gProtectedPositionSide::Long,
                "Filled",
                flat_position_truth(),
            ),
        ));
        let source = stage5g_protective_restart_source_from_transition(
            Stage5gProtectiveCompletionTransition::FlatCleanupPending(Box::new(pending)),
        )
        .expect("flat-cleanup transition is source-restartable");
        let restored = restore_stage5g_protective_completion_continuation(
            restore_protective_source(source, "stage5g-f-r4-flat-roundtrip"),
        )
        .expect("flat cleanup continuation restores");
        let Stage5gProtectiveRestoredContinuation::FlatCleanupPending(restored) = restored else {
            panic!("expected flat cleanup continuation")
        };
        assert_eq!(
            restored.generated_cleanup_batch_summary().intent_count,
            restored.generated_cleanup_batch().intent_count()
        );
        let cleanup_projection = restored
            .projection
            .cleanup_batch_restart_projection
            .as_ref()
            .expect("cleanup projection");
        assert!(
            cleanup_projection.records.len() >= 2,
            "normal TP cleanup fixture must retain sibling stop and exchange-child cleanup"
        );
        assert_eq!(
            restored
                .projection
                .generated_cleanup_batch_fingerprint_sha256
                .as_deref(),
            Some(cleanup_projection.batch_fingerprint.as_str())
        );
        assert_eq!(
            restored.cleanup_settlement_ledger().requests.len(),
            cleanup_projection.records.len()
        );
        assert!(restored
            .cleanup_settlement_ledger()
            .requests
            .iter()
            .all(|request| request.state == Stage5gProtectiveCleanupSettlementState::Pending));
        let cleanup_record = cleanup_projection.records[0].clone();
        let accepted_cleanup = accept_stage5g_protective_cleanup_truth(
            &restored,
            cleanup_record.request_id,
            cleanup_record.target_protective_id.clone(),
            "Canceled",
            cleanup_record.source_event_ts + 1,
        )
        .expect("cleanup truth accepted");
        let cleanup_transition =
            apply_stage5g_protective_cleanup_completion(*restored, accepted_cleanup);
        let Stage5gProtectiveCleanupTransition::FlatCleanupPending(still_pending) =
            cleanup_transition
        else {
            panic!("first cleanup settlement must keep batch pending")
        };
        assert_eq!(
            still_pending.cleanup_settlement_ledger().requests[0].state,
            Stage5gProtectiveCleanupSettlementState::TerminalNonExecution
        );
        assert!(still_pending.cleanup_settlement_ledger().requests[0]
            .settlement_fingerprint_sha256
            .as_deref()
            .is_some_and(sha256_like));
        assert!(still_pending
            .cleanup_settlement_ledger()
            .requests
            .iter()
            .skip(1)
            .all(|request| request.state == Stage5gProtectiveCleanupSettlementState::Pending));
        let source = restart_source_from_restored_flat_cleanup_pending(*still_pending)
            .expect("partial cleanup transition is restartable");
        let restored = restore_stage5g_protective_completion_continuation(
            restore_protective_source(source, "stage5g-f-r5-partial-cleanup-roundtrip"),
        )
        .expect("partial cleanup continuation restores");
        let Stage5gProtectiveRestoredContinuation::FlatCleanupPending(restored) = restored else {
            panic!("expected partial cleanup pending continuation")
        };
        assert_eq!(
            restored.cleanup_settlement_ledger().requests[0].state,
            Stage5gProtectiveCleanupSettlementState::TerminalNonExecution
        );
        let cleanup_projection = restored
            .projection
            .cleanup_batch_restart_projection
            .as_ref()
            .expect("cleanup projection after partial restart");
        let cleanup_record = cleanup_projection.records[1].clone();
        let terminal_status = match cleanup_record.base_action {
            crate::stage5c_paper_host::Stage5gSourceBaseAction::DeleteStopLimit => "Deleted",
            crate::stage5c_paper_host::Stage5gSourceBaseAction::Cancel => "Canceled",
            _ => "Rejected",
        };
        let accepted_cleanup = accept_stage5g_protective_cleanup_truth(
            &restored,
            cleanup_record.request_id,
            cleanup_record.target_protective_id.clone(),
            terminal_status,
            cleanup_record.source_event_ts + 1,
        )
        .expect("second cleanup truth accepted");
        let cleanup_transition =
            apply_stage5g_protective_cleanup_completion(*restored, accepted_cleanup);
        let Stage5gProtectiveCleanupTransition::Completed(completed) = cleanup_transition else {
            panic!("all cleanup settlements must complete")
        };
        assert_eq!(
            completed
                .cleanup_settlement_fingerprint_sha256
                .as_ref()
                .map(String::len),
            Some(64)
        );
        assert_eq!(completed.final_position_qty, Decimal::ZERO);
        let source = stage5g_protective_restart_source_from_transition(
            Stage5gProtectiveCompletionTransition::Completed(completed),
        )
        .expect("completed transition is restartable");
        let restored = restore_stage5g_protective_completion_continuation(
            restore_protective_source(source, "stage5g-f-r4-completed-roundtrip"),
        )
        .expect("completed continuation restores");
        let Stage5gProtectiveRestoredContinuation::Completed(restored) = restored else {
            panic!("expected completed continuation")
        };
        assert_eq!(
            restored.projection.projection_kind,
            Stage5gProtectiveRestartProjectionKind::Completed
        );
        assert_eq!(
            restored.post_state.summary().final_position_qty,
            Decimal::ZERO
        );
        assert!(restored
            .projection
            .cleanup_settlement_fingerprint_sha256
            .as_deref()
            .is_some_and(sha256_like));
    }

    #[test]
    fn stage5g_f_r6_cleanup_exact_replay_after_partial_settlement_is_idempotent() {
        let restored = restored_flat_cleanup_pending(
            Stage5gProtectedPositionSide::Long,
            Stage5gProtectiveLeg::Target,
            "stage5g-f-r6-exact-replay",
        );
        assert!(
            restored
                .projection
                .cleanup_batch_restart_projection
                .as_ref()
                .expect("cleanup projection")
                .records
                .len()
                >= 2
        );
        let cleanup_record = restored
            .projection
            .cleanup_batch_restart_projection
            .as_ref()
            .expect("cleanup projection")
            .records[0]
            .clone();
        let accepted_first = accept_cleanup_record(restored.as_ref(), 0, "Canceled");
        let first_observation = accepted_first
            .evidence
            .cleanup_observation_fingerprint_sha256
            .clone();
        let Stage5gProtectiveCleanupTransition::FlatCleanupPending(still_pending) =
            apply_stage5g_protective_cleanup_completion(*restored, accepted_first)
        else {
            panic!("first cleanup settlement must keep sibling cleanup pending")
        };
        let before_ledger_json = serde_json::to_string(still_pending.cleanup_settlement_ledger())
            .expect("ledger serializes");
        let before_projection = still_pending
            .projection
            .replay_protection_fingerprint_sha256
            .clone();
        let accepted_replay = accept_stage5g_protective_cleanup_truth(
            &still_pending,
            cleanup_record.request_id,
            cleanup_record.target_protective_id,
            "Cancelled",
            cleanup_record.source_event_ts + 1,
        )
        .expect("exact alias replay accepted");
        assert_eq!(
            accepted_replay
                .evidence
                .cleanup_observation_fingerprint_sha256,
            first_observation
        );
        let Stage5gProtectiveCleanupTransition::FlatCleanupPending(replayed) =
            apply_stage5g_protective_cleanup_completion(*still_pending, accepted_replay)
        else {
            panic!("exact cleanup replay must remain pending and idempotent")
        };
        assert_eq!(
            serde_json::to_string(replayed.cleanup_settlement_ledger()).expect("ledger serializes"),
            before_ledger_json
        );
        assert_eq!(
            replayed.projection.replay_protection_fingerprint_sha256,
            before_projection
        );
    }

    #[test]
    fn stage5g_f_r6_cleanup_exact_replay_after_restart_is_idempotent() {
        let restored = restored_flat_cleanup_pending(
            Stage5gProtectedPositionSide::Long,
            Stage5gProtectiveLeg::Target,
            "stage5g-f-r6-restart-replay",
        );
        let cleanup_record = restored
            .projection
            .cleanup_batch_restart_projection
            .as_ref()
            .expect("cleanup projection")
            .records[0]
            .clone();
        let accepted_first = accept_cleanup_record(restored.as_ref(), 0, "Canceled");
        let Stage5gProtectiveCleanupTransition::FlatCleanupPending(still_pending) =
            apply_stage5g_protective_cleanup_completion(*restored, accepted_first)
        else {
            panic!("first cleanup settlement must keep sibling cleanup pending")
        };
        let source = restart_source_from_restored_flat_cleanup_pending(*still_pending)
            .expect("partial cleanup transition is restartable");
        let restored = restore_stage5g_protective_completion_continuation(
            restore_protective_source(source, "stage5g-f-r6-partial-replay-roundtrip"),
        )
        .expect("partial cleanup continuation restores");
        let Stage5gProtectiveRestoredContinuation::FlatCleanupPending(restored) = restored else {
            panic!("expected partial cleanup pending continuation")
        };
        let before_ledger_json =
            serde_json::to_string(restored.cleanup_settlement_ledger()).expect("ledger serializes");
        let before_projection = restored
            .projection
            .replay_protection_fingerprint_sha256
            .clone();
        let accepted_replay = accept_stage5g_protective_cleanup_truth(
            &restored,
            cleanup_record.request_id,
            cleanup_record.target_protective_id,
            "Canceled",
            cleanup_record.source_event_ts + 1,
        )
        .expect("exact replay after restart accepted");
        let Stage5gProtectiveCleanupTransition::FlatCleanupPending(replayed) =
            apply_stage5g_protective_cleanup_completion(*restored, accepted_replay)
        else {
            panic!("exact cleanup replay after restart must be idempotent")
        };
        assert_eq!(
            serde_json::to_string(replayed.cleanup_settlement_ledger()).expect("ledger serializes"),
            before_ledger_json
        );
        assert_eq!(
            replayed.projection.replay_protection_fingerprint_sha256,
            before_projection
        );
    }

    #[test]
    fn stage5g_f_r6_cleanup_changed_observation_after_settlement_conflicts() {
        let restored = restored_flat_cleanup_pending(
            Stage5gProtectedPositionSide::Long,
            Stage5gProtectiveLeg::Target,
            "stage5g-f-r6-conflict-replay",
        );
        let cleanup_record = restored
            .projection
            .cleanup_batch_restart_projection
            .as_ref()
            .expect("cleanup projection")
            .records[0]
            .clone();
        let accepted_first = accept_cleanup_record(restored.as_ref(), 0, "Canceled");
        let Stage5gProtectiveCleanupTransition::FlatCleanupPending(still_pending) =
            apply_stage5g_protective_cleanup_completion(*restored, accepted_first)
        else {
            panic!("first cleanup settlement must keep sibling cleanup pending")
        };
        let before_projection = still_pending
            .projection
            .replay_protection_fingerprint_sha256
            .clone();
        let accepted_conflict = accept_stage5g_protective_cleanup_truth(
            &still_pending,
            cleanup_record.request_id,
            cleanup_record.target_protective_id,
            "Rejected",
            cleanup_record.source_event_ts + 1,
        )
        .expect("changed cleanup observation accepted before conflict classification");
        let Stage5gProtectiveCleanupTransition::Blocked { projection, reason } =
            apply_stage5g_protective_cleanup_completion(*still_pending, accepted_conflict)
        else {
            panic!("changed cleanup observation must conflict with stored observation")
        };
        assert_eq!(reason, Stage5gProtectiveBlockReason::CleanupConflict);
        assert_eq!(
            projection.replay_protection_fingerprint_sha256,
            before_projection
        );
    }

    #[test]
    fn stage5g_f_r6_reverse_cleanup_ack_order_is_deterministic() {
        let pending_ab = restored_flat_cleanup_pending(
            Stage5gProtectedPositionSide::Long,
            Stage5gProtectiveLeg::Target,
            "stage5g-f-r6-order-fixed",
        );
        let request_count = pending_ab
            .projection
            .cleanup_batch_restart_projection
            .as_ref()
            .expect("cleanup projection")
            .records
            .len();
        assert_eq!(
            request_count, 2,
            "current frozen Stage 5G-f cleanup fixture has exactly two cleanup requests"
        );
        let completed_ab = complete_cleanup_in_order(pending_ab, &[0, 1]);
        let completed_ab_fingerprint = completed_ab.completion_fingerprint_sha256.clone();
        let completed_ab_cleanup = completed_ab.cleanup_settlement_fingerprint_sha256.clone();
        let completed_ab_runtime = completed_ab.post_callback_state_fingerprint_sha256.clone();
        let source_ab = stage5g_protective_restart_source_from_transition(
            Stage5gProtectiveCompletionTransition::Completed(completed_ab),
        )
        .expect("completed AB transition is restartable");
        let projection_ab = source_ab.stage5g_restart_projection();

        let pending_ba = restored_flat_cleanup_pending(
            Stage5gProtectedPositionSide::Long,
            Stage5gProtectiveLeg::Target,
            "stage5g-f-r6-order-fixed",
        );
        let completed_ba = complete_cleanup_in_order(pending_ba, &[1, 0]);
        assert_eq!(
            completed_ba.completion_fingerprint_sha256,
            completed_ab_fingerprint
        );
        assert_eq!(
            completed_ba.cleanup_settlement_fingerprint_sha256,
            completed_ab_cleanup
        );
        assert_eq!(
            completed_ba.post_callback_state_fingerprint_sha256,
            completed_ab_runtime
        );
        let source_ba = stage5g_protective_restart_source_from_transition(
            Stage5gProtectiveCompletionTransition::Completed(completed_ba),
        )
        .expect("completed BA transition is restartable");
        let projection_ba = source_ba.stage5g_restart_projection();
        assert_eq!(
            projection_ab.replay_protection_fingerprint_sha256,
            projection_ba.replay_protection_fingerprint_sha256
        );
        assert_eq!(projection_ab, projection_ba);
    }

    #[test]
    fn stage5g_f_r4_non_terminal_cleanup_truth_keeps_flat_cleanup_pending() {
        let pending = flat_cleanup_pending(apply(
            prepare_stage5g_protective_completion(restore_protective_source(
                protective_restart_source(Stage5gProtectedPositionSide::Short),
                "stage5g-f-r4-non-terminal-source",
            ))
            .expect("protective authority"),
            stop_evidence(
                Stage5gProtectedPositionSide::Short,
                "Executed",
                flat_position_truth(),
            ),
        ));
        let restored =
            restore_stage5g_protective_completion_continuation(restore_protective_source(
                stage5g_protective_restart_source_from_transition(
                    Stage5gProtectiveCompletionTransition::FlatCleanupPending(Box::new(pending)),
                )
                .expect("restart source"),
                "stage5g-f-r4-non-terminal-roundtrip",
            ))
            .expect("restore");
        let Stage5gProtectiveRestoredContinuation::FlatCleanupPending(restored) = restored else {
            panic!("expected pending")
        };
        let cleanup_record = restored
            .projection
            .cleanup_batch_restart_projection
            .as_ref()
            .and_then(|projection| projection.records.first())
            .expect("cleanup record");
        let accepted_cleanup = accept_stage5g_protective_cleanup_truth(
            &restored,
            cleanup_record.request_id,
            cleanup_record.target_protective_id.clone(),
            "Working",
            cleanup_record.source_event_ts + 1,
        )
        .expect("working cleanup truth accepted");
        let Stage5gProtectiveCleanupTransition::FlatCleanupPending(still_pending) =
            apply_stage5g_protective_cleanup_completion(*restored, accepted_cleanup)
        else {
            panic!("expected pending")
        };
        assert!(still_pending.projection.cleanup_pending);
        assert!(!still_pending.projection.completed);
    }

    #[test]
    fn stage5g_f_r5_cleanup_token_is_bound_to_exact_pending_authority() {
        let pending_a = flat_cleanup_pending(apply(
            prepare_stage5g_protective_completion(restore_protective_source(
                protective_restart_source(Stage5gProtectedPositionSide::Long),
                "stage5g-f-r5-pending-a-source",
            ))
            .expect("protective authority A"),
            target_evidence(
                Stage5gProtectedPositionSide::Long,
                "Filled",
                flat_position_truth(),
            ),
        ));
        let source_a = stage5g_protective_restart_source_from_transition(
            Stage5gProtectiveCompletionTransition::FlatCleanupPending(Box::new(pending_a)),
        )
        .expect("pending A restartable");
        let restored_a = restore_stage5g_protective_completion_continuation(
            restore_protective_source(source_a, "stage5g-f-r5-pending-a-roundtrip"),
        )
        .expect("pending A restores");
        let Stage5gProtectiveRestoredContinuation::FlatCleanupPending(restored_a) = restored_a
        else {
            panic!("expected pending A")
        };
        let record_a = restored_a
            .projection
            .cleanup_batch_restart_projection
            .as_ref()
            .expect("cleanup projection A")
            .records[0]
            .clone();
        let accepted_for_a = accept_stage5g_protective_cleanup_truth(
            &restored_a,
            record_a.request_id,
            record_a.target_protective_id,
            "Canceled",
            record_a.source_event_ts + 1,
        )
        .expect("cleanup truth accepted for pending A");

        let pending_b = flat_cleanup_pending(apply(
            prepare_stage5g_protective_completion(restore_protective_source(
                protective_restart_source(Stage5gProtectedPositionSide::Short),
                "stage5g-f-r5-pending-b-source",
            ))
            .expect("protective authority B"),
            target_evidence(
                Stage5gProtectedPositionSide::Short,
                "Filled",
                flat_position_truth(),
            ),
        ));
        let source_b = stage5g_protective_restart_source_from_transition(
            Stage5gProtectiveCompletionTransition::FlatCleanupPending(Box::new(pending_b)),
        )
        .expect("pending B restartable");
        let restored_b = restore_stage5g_protective_completion_continuation(
            restore_protective_source(source_b, "stage5g-f-r5-pending-b-roundtrip"),
        )
        .expect("pending B restores");
        let Stage5gProtectiveRestoredContinuation::FlatCleanupPending(restored_b) = restored_b
        else {
            panic!("expected pending B")
        };
        let before = restored_b
            .projection
            .replay_protection_fingerprint_sha256
            .clone();
        let Stage5gProtectiveCleanupTransition::Blocked { projection, reason } =
            apply_stage5g_protective_cleanup_completion(*restored_b, accepted_for_a)
        else {
            panic!("cross-pending cleanup token must fail closed")
        };
        assert_eq!(
            reason,
            Stage5gProtectiveBlockReason::CleanupAuthorityMismatch
        );
        assert_eq!(projection.replay_protection_fingerprint_sha256, before);
    }

    #[test]
    fn stage5g_f_r5_cleanup_execution_race_requires_position_truth() {
        let pending = flat_cleanup_pending(apply(
            prepare_stage5g_protective_completion(restore_protective_source(
                protective_restart_source(Stage5gProtectedPositionSide::Long),
                "stage5g-f-r5-execution-race-source",
            ))
            .expect("protective authority"),
            target_evidence(
                Stage5gProtectedPositionSide::Long,
                "Filled",
                flat_position_truth(),
            ),
        ));
        let source = stage5g_protective_restart_source_from_transition(
            Stage5gProtectiveCompletionTransition::FlatCleanupPending(Box::new(pending)),
        )
        .expect("pending restartable");
        let restored = restore_stage5g_protective_completion_continuation(
            restore_protective_source(source, "stage5g-f-r5-execution-race-roundtrip"),
        )
        .expect("pending restores");
        let Stage5gProtectiveRestoredContinuation::FlatCleanupPending(restored) = restored else {
            panic!("expected pending")
        };
        let cleanup_record = restored
            .projection
            .cleanup_batch_restart_projection
            .as_ref()
            .expect("cleanup projection")
            .records[0]
            .clone();
        let accepted_cleanup = accept_stage5g_protective_cleanup_truth(
            &restored,
            cleanup_record.request_id,
            cleanup_record.target_protective_id,
            "Filled",
            cleanup_record.source_event_ts + 1,
        )
        .expect("execution race observation accepted as typed cleanup truth");
        let Stage5gProtectiveCleanupTransition::CleanupPositionTruthRequired(pending) =
            apply_stage5g_protective_cleanup_completion(*restored, accepted_cleanup)
        else {
            panic!("sibling execution must require fresh position truth")
        };
        assert_eq!(
            pending.cleanup_settlement_ledger().requests[0].state,
            Stage5gProtectiveCleanupSettlementState::ExecutionObserved
        );
        assert!(pending.projection.cleanup_pending);
        assert!(!pending.projection.completed);
    }

    #[test]
    fn stage5g_f_r3_completed_is_not_immediate_when_sibling_cleanup_is_pending() {
        let transition = apply(
            prepare_stage5g_protective_completion(restore_protective_source(
                protective_restart_source(Stage5gProtectedPositionSide::Long),
                "stage5g-f-r3-completed-policy",
            ))
            .expect("protective authority"),
            target_evidence(
                Stage5gProtectedPositionSide::Long,
                "Filled",
                flat_position_truth(),
            ),
        );
        assert_eq!(
            transition.disposition(),
            Stage5gProtectiveDisposition::FlatCleanupPending
        );
        assert_ne!(
            transition.disposition(),
            Stage5gProtectiveDisposition::Completed
        );
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

    #[test]
    fn stage5g_f_r7_artifact_enters_through_authenticated_production_prepare() {
        let rows = stage5g_f_gprt_artifact_rows();
        assert_eq!(rows.len(), 8);
        assert!(rows.iter().all(|row| {
            row.schema_version == 3
                && sha256_like(&row.pre_authenticated_restart_package_fingerprint_sha256)
                && stage5c_semantic_fingerprint_like(&row.pre_runtime_semantic_fingerprint_sha256)
        }));
        assert!(rows[..4].iter().all(|row| {
            row.phase_a_authenticated_restore_succeeded
                && row.phase_b_authenticated_restore_succeeded
                && row
                    .phase_a_canonical_restart_package_fingerprint_sha256
                    .as_deref()
                    .is_some_and(sha256_like)
                && row
                    .phase_b_completed_canonical_restart_package_fingerprint_sha256
                    .as_deref()
                    .is_some_and(sha256_like)
        }));
    }

    #[test]
    fn stage5g_f_r7_gprt07_receipt_precedes_callback_and_runtime_is_unchanged() {
        let row = stage5g_f_source_produced_gprt_artifact_row(
            Stage5gProtectiveScenarioId::Gprt07TriggerWithoutFlatPositionBlocks,
        );
        assert_eq!(row.phase_a_disposition, "awaiting_position_truth");
        assert_eq!(row.phase_a_execution_receipt_count, 1);
        assert_eq!(row.phase_a_callback_count, 0);
        assert_eq!(row.callback_count, 0);
        assert_eq!(
            row.phase_a_runtime_semantic_fingerprint_sha256.as_deref(),
            Some(row.pre_runtime_semantic_fingerprint_sha256.as_str())
        );
        assert!(row.phase_a_authenticated_restore_succeeded);
        assert!(row
            .phase_a_canonical_restart_package_fingerprint_sha256
            .as_deref()
            .is_some_and(sha256_like));
    }

    #[test]
    fn stage5g_f_r7_blocked_rows_freeze_no_runtime_mutation_and_no_callback() {
        for scenario in [
            Stage5gProtectiveScenarioId::Gprt05WrongOwnerOrCycleBlocks,
            Stage5gProtectiveScenarioId::Gprt06WrongInstrumentOrOrderIdBlocks,
            Stage5gProtectiveScenarioId::Gprt08NonExecutionTerminalCannotInventExit,
        ] {
            let row = stage5g_f_source_produced_gprt_artifact_row(scenario);
            assert_eq!(row.phase_a_disposition, "blocked");
            assert_eq!(row.phase_a_callback_count, 0);
            assert_eq!(row.callback_count, 0);
            assert_eq!(
                row.phase_a_runtime_semantic_fingerprint_sha256.as_deref(),
                Some(row.pre_runtime_semantic_fingerprint_sha256.as_str())
            );
        }
    }

    #[test]
    fn stage5g_f_r7_completed_retains_and_authenticates_final_cleanup_ledger() {
        let pending = restored_flat_cleanup_pending(
            Stage5gProtectedPositionSide::Long,
            Stage5gProtectiveLeg::Target,
            "stage5g-f-r7-completed-ledger",
        );
        let order = (0..pending.cleanup_settlement_ledger.requests.len()).collect::<Vec<_>>();
        let completed = complete_cleanup_in_order(pending, &order);
        let final_ledger = completed
            .final_cleanup_settlement_ledger
            .clone()
            .expect("Completed owns the final cleanup ledger");
        let final_batch = completed
            .final_cleanup_batch_restart_projection
            .clone()
            .expect("Completed owns the original cleanup batch projection");
        assert!(stage5g_cleanup_ledger_is_valid(&final_ledger, &final_batch));
        assert!(stage5g_cleanup_ledger_all_terminal_non_execution(
            &final_ledger
        ));

        let source = stage5g_protective_restart_source_from_transition(
            Stage5gProtectiveCompletionTransition::Completed(completed),
        )
        .expect("Completed is restartable");
        let restored = restore_stage5g_protective_completion_continuation(
            restore_protective_source(source, "stage5g-f-r7-completed-ledger-roundtrip"),
        )
        .expect("Completed authenticates and restores");
        let Stage5gProtectiveRestoredContinuation::Completed(restored) = restored else {
            panic!("expected restored Completed")
        };
        assert_eq!(
            restored.projection.cleanup_settlement_ledger.as_ref(),
            Some(&final_ledger)
        );
        assert_eq!(
            restored
                .projection
                .cleanup_batch_restart_projection
                .as_ref(),
            Some(&final_batch)
        );
    }

    #[test]
    fn stage5g_f_r7_artifact_phase_b_ledger_is_restored_completed_truth() {
        for row in stage5g_f_gprt_artifact_rows().into_iter().take(4) {
            assert_eq!(row.phase_b_disposition.as_deref(), Some("completed"));
            assert!(row.phase_b_cleanup_request_states.iter().all(
                |state| *state == Stage5gProtectiveCleanupSettlementState::TerminalNonExecution
            ));
            assert_eq!(
                row.phase_b_cleanup_request_states.len(),
                row.phase_b_cleanup_observation_fingerprints_sha256.len()
            );
            assert!(row
                .phase_b_final_cleanup_ledger_fingerprint_sha256
                .as_deref()
                .is_some_and(sha256_like));
            assert!(row
                .phase_b_completion_fingerprint_sha256
                .as_deref()
                .is_some_and(sha256_like));
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
            {
                let mut evidence = target_evidence(
                    Stage5gProtectedPositionSide::Long,
                    "Filled",
                    flat_position_truth(),
                );
                if let Stage5gProtectiveExecutionEvidence::TargetOrder(order) =
                    &mut evidence.execution
                {
                    order.attribution = Some(attr("TP", "wrong-cycle", "MR"));
                }
                apply(authority(Stage5gProtectedPositionSide::Long), evidence)
                    .semantic_fingerprint_sha256()
            },
            {
                let mut evidence = target_evidence(
                    Stage5gProtectedPositionSide::Long,
                    "Filled",
                    flat_position_truth(),
                );
                if let Stage5gProtectiveExecutionEvidence::TargetOrder(order) =
                    &mut evidence.execution
                {
                    order.order_id = BrokerOrderId::new("TP_OTHER");
                }
                apply(authority(Stage5gProtectedPositionSide::Long), evidence)
                    .semantic_fingerprint_sha256()
            },
            apply(
                authority(Stage5gProtectedPositionSide::Long),
                stop_evidence(
                    Stage5gProtectedPositionSide::Long,
                    "Triggered",
                    nonflat_position_truth(),
                ),
            )
            .semantic_fingerprint_sha256(),
            apply(
                authority(Stage5gProtectedPositionSide::Long),
                target_evidence(
                    Stage5gProtectedPositionSide::Long,
                    "Canceled",
                    flat_position_truth(),
                ),
            )
            .semantic_fingerprint_sha256(),
        ]
    }
}
