//! Stage 5G-d deterministic paper timer and continuation arbitration.
//!
//! This module delegates every strategy callback to the accepted Stage 5C
//! type-state API. It adds only a linear Stage 5G ownership boundary and the
//! exact broker-package replay projection required across timer checkpoints.
//! It contains no clock read, scheduler, Redis, FINAM or broker dispatch.

use broker_core::StrategyRequestId;
use chrono::{DateTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::stage5c_paper_host::{
    advance_stage5c_paper_loop_once,
    advance_stage5c_timer_settlement_next_bar_transactional_at_checkpoint,
    advance_stage5c_timer_settlement_timer, settle_stage5c_timer_result,
    stage5gd_accepted_bar_checkpoint_ts_utc_ms, stage5gd_rearm_zero_intent_bar_continuation,
    Stage5cAcceptedSemanticBar, Stage5cPaperLoopError, Stage5cPaperLoopEvent,
    Stage5cPaperLoopState, Stage5cPaperTimerInput, Stage5cSettledPaperStrategy,
    Stage5cTimerContinuationError, Stage5cTimerSettlement,
};
use crate::stage5g_mock_ack::{
    apply_stage5g_mock_ack, attach_stage5g_mock_ack_session, Stage5gMockAckAdmissionBlocked,
    Stage5gMockAckAdmissionError, Stage5gMockAckEvent, Stage5gMockAckFailure,
    Stage5gMockAckSession, Stage5gMockAckSessionInput, Stage5gMockAckTransition,
    Stage5gResolvedMockAckPaperStrategy,
};
use crate::stage5g_order_position::{
    attach_stage5g_order_position_session_with_replay,
    canonicalize_stage5g_order_position_evidence, EvidenceIdentity,
    Stage5gCanonicalOrderPositionEvidence, Stage5gConvergedPaperStrategy,
    Stage5gEvidenceCanonicalizationError, Stage5gMarketTerminalConvergedPaperStrategy,
    Stage5gOrderPositionEvidence, Stage5gOrderPositionSession, Stage5gOrderPositionSummary,
    Stage5gReplayCheckpoint,
};

pub const STAGE5G_TIMER_CHECKPOINT_SCHEMA_VERSION: u16 = 1;
const STAGE5G_TIMER_CHECKPOINT_DOMAIN: &[u8] = b"moex.stage5g.timer-checkpoint.v1\0";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stage5gReplayLedgerEntry {
    pub identity: String,
    pub fingerprint_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stage5gTimerCheckpointPayload {
    pub schema_version: u16,
    pub package_discriminator: Option<String>,
    pub current_evidence_identity: Option<String>,
    pub evidence_replay_ledger: Vec<Stage5gReplayLedgerEntry>,
    pub last_broker_truth_received_at: Option<DateTime<Utc>>,
    pub last_broker_truth_received_ms: Option<i64>,
    pub duplicate_evidence_count: usize,
    pub last_total_sequence: Option<u64>,
    pub last_continuation_checkpoint_ts_utc_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stage5gTimerCheckpointEnvelope {
    pub payload: Stage5gTimerCheckpointPayload,
    pub payload_sha256: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage5gTimerCheckpointError {
    UnsupportedSchema,
    FingerprintMismatch,
    MissingPackageDiscriminator,
    MissingExactBrokerTruthReceipt,
    MissingMillisecondWatermark,
    PackageDiscriminatorMismatch,
    MillisecondWatermarkMismatch,
    MissingReplayLedger,
    InvalidReplayLedgerEntry,
    DuplicateReplayIdentity,
    MissingCurrentEvidenceIdentity,
    InvalidCurrentEvidenceIdentity,
    AmbiguousCurrentEvidenceIdentity,
    CurrentPackageMissingFromReplayLedger,
    ReplayLedgerReceiptRegression,
    CurrentEvidenceIdentityNotLatest,
    CurrentPackageReceiptMismatch,
    MissingTotalSequence,
    MissingContinuationCheckpoint,
    ContinuationBeforeBrokerTruth,
    DuplicateCounterIncoherent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage5gTimerError {
    NonMonotonicCheckpoint,
    ContinuationBeforeInnerSettlement,
    Stage5c(Stage5cPaperLoopError),
    UnexpectedStage5cState,
}

/// Linear paper-only continuation capability. It deliberately implements none
/// of Clone, Copy, Debug, Serialize or Deserialize.
pub struct Stage5gTimerSession {
    stage5c_state: Stage5cPaperLoopState,
    summary: Stage5gOrderPositionSummary,
    replay: Stage5gReplayCheckpoint,
    last_continuation_checkpoint_ts_utc_ms: Option<i64>,
}

pub struct Stage5gTimerReadyPaperStrategy {
    settlement: Stage5cTimerSettlement,
    summary: Stage5gOrderPositionSummary,
    replay: Stage5gReplayCheckpoint,
    checkpoint_ts_utc_ms: i64,
}

pub struct Stage5gTimerGeneratedIntentEscrow {
    settled: Stage5cSettledPaperStrategy,
    summary: Stage5gOrderPositionSummary,
    replay: Stage5gReplayCheckpoint,
    checkpoint_ts_utc_ms: i64,
}

pub struct Stage5gTimerMockAckSession {
    inner: Stage5gMockAckSession,
    summary: Stage5gOrderPositionSummary,
    replay: Stage5gReplayCheckpoint,
    checkpoint_ts_utc_ms: i64,
}

pub struct Stage5gTimerResolvedMockAckPaperStrategy {
    inner: Stage5gResolvedMockAckPaperStrategy,
    summary: Stage5gOrderPositionSummary,
    replay: Stage5gReplayCheckpoint,
    checkpoint_ts_utc_ms: i64,
}

pub enum Stage5gTimerMockAckTransition {
    Awaiting(Stage5gTimerMockAckSession),
    Resolved(Stage5gTimerResolvedMockAckPaperStrategy),
}

pub struct Stage5gTimerMockAckAdmissionBlocked {
    blocked: Box<Stage5gMockAckAdmissionBlocked>,
    summary: Stage5gOrderPositionSummary,
    replay: Stage5gReplayCheckpoint,
    checkpoint_ts_utc_ms: i64,
}

pub enum Stage5gTimerMockAckFailure {
    Blocked(Box<Stage5gTimerMockAckBlocked>),
    Terminal(Stage5gMockAckFailure),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage5gTimerMockAckError {
    AckBeforeContinuationCheckpoint,
    MockAck(crate::Stage5gMockAckError),
}

pub struct Stage5gTimerMockAckBlocked {
    reason: Stage5gTimerMockAckError,
    session: Stage5gTimerMockAckSession,
}

pub struct Stage5gTimerOrderPositionAdmissionBlocked {
    reason: crate::Stage5gOrderPositionAdmissionError,
    resolved: Stage5gTimerResolvedMockAckPaperStrategy,
}

impl std::fmt::Debug for Stage5gTimerOrderPositionAdmissionBlocked {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Stage5gTimerOrderPositionAdmissionBlocked")
            .field("reason", &self.reason)
            .field("checkpoint_ts_utc_ms", &self.resolved.checkpoint_ts_utc_ms)
            .finish_non_exhaustive()
    }
}

pub struct Stage5gBarContinuationPaperStrategy {
    settled: Stage5cSettledPaperStrategy,
    summary: Stage5gOrderPositionSummary,
    replay: Stage5gReplayCheckpoint,
    checkpoint_ts_utc_ms: i64,
}

pub enum Stage5gBarContinuationTransition {
    Ready(Stage5gTimerReadyPaperStrategy),
    GeneratedIntent(Stage5gTimerGeneratedIntentEscrow),
}

pub enum Stage5gTimerTransition {
    Ready(Stage5gTimerReadyPaperStrategy),
    GeneratedIntent(Stage5gTimerGeneratedIntentEscrow),
}

impl std::fmt::Debug for Stage5gTimerTransition {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Stage5gTimerTransition")
            .field(
                "kind",
                &match self {
                    Self::Ready(_) => "ready",
                    Self::GeneratedIntent(_) => "generated_intent",
                },
            )
            .finish_non_exhaustive()
    }
}

pub struct Stage5gTimerBlocked {
    reason: Stage5gTimerError,
    session: Stage5gTimerSession,
}

pub enum Stage5gTimerFailure {
    Blocked(Box<Stage5gTimerBlocked>),
    Terminal(Stage5gTimerError),
}

impl std::fmt::Debug for Stage5gTimerFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Stage5gTimerFailure")
            .field("reason", &self.reason())
            .field("retryable", &matches!(self, Self::Blocked(_)))
            .finish_non_exhaustive()
    }
}

impl Stage5gTimerFailure {
    pub fn reason(&self) -> Stage5gTimerError {
        match self {
            Self::Blocked(blocked) => blocked.reason,
            Self::Terminal(reason) => *reason,
        }
    }

    pub fn into_blocked(self) -> Option<Stage5gTimerBlocked> {
        match self {
            Self::Blocked(blocked) => Some(*blocked),
            Self::Terminal(_) => None,
        }
    }
}

impl Stage5gTimerBlocked {
    pub fn reason(&self) -> Stage5gTimerError {
        self.reason
    }

    pub fn into_session(self) -> Stage5gTimerSession {
        self.session
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage5gCheckpointReplayDisposition {
    ExactReplay,
    NewPackage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage5gCheckpointReplayError {
    NonMonotonicSequence,
    BrokerTruthBeforeContinuationCheckpoint,
    BrokerTruthTimeRegression,
    ConflictingDuplicateEvidence,
    TradeIdentityConflict,
    EvidenceIdentityGrammarViolation,
}

#[derive(Debug)]
pub struct Stage5gCheckpointReplayResult {
    disposition: Stage5gCheckpointReplayDisposition,
    replay: Stage5gReplayCheckpoint,
    last_continuation_checkpoint_ts_utc_ms: Option<i64>,
    canonical_new_package_candidate: Option<Stage5gCanonicalOrderPositionEvidence>,
}

impl Stage5gCheckpointReplayResult {
    pub fn disposition(&self) -> Stage5gCheckpointReplayDisposition {
        self.disposition
    }

    pub fn checkpoint(&self) -> Stage5gTimerCheckpointEnvelope {
        checkpoint_envelope(&self.replay, self.last_continuation_checkpoint_ts_utc_ms)
    }

    pub fn owns_canonical_new_package_candidate(&self) -> bool {
        self.canonical_new_package_candidate.is_some()
    }
}

pub fn attach_stage5g_timer_session(
    converged: Stage5gConvergedPaperStrategy,
) -> Stage5gTimerSession {
    let (resolved, summary, replay) = converged.into_stage5g_d_parts();
    timer_session(
        Stage5cPaperLoopState::BrokerLifecycleResolved(Box::new(resolved)),
        summary,
        replay,
    )
}

pub fn attach_stage5g_market_terminal_timer_session(
    converged: Stage5gMarketTerminalConvergedPaperStrategy,
) -> Stage5gTimerSession {
    let (settlement, summary, replay) = converged.into_stage5g_d_parts();
    timer_session(
        Stage5cPaperLoopState::BrokerLifecycleSettlement(Box::new(settlement)),
        summary,
        replay,
    )
}

fn timer_session(
    stage5c_state: Stage5cPaperLoopState,
    summary: Stage5gOrderPositionSummary,
    replay: Stage5gReplayCheckpoint,
) -> Stage5gTimerSession {
    let last_continuation_checkpoint_ts_utc_ms = max_optional_checkpoint(
        replay.last_continuation_checkpoint_ts_utc_ms,
        replay.last_broker_truth_received_ms,
    );
    Stage5gTimerSession {
        stage5c_state,
        summary,
        replay,
        last_continuation_checkpoint_ts_utc_ms,
    }
}

pub fn apply_stage5g_timer_checkpoint(
    session: Stage5gTimerSession,
    input: Stage5cPaperTimerInput,
) -> Result<Stage5gTimerTransition, Stage5gTimerFailure> {
    if session
        .last_continuation_checkpoint_ts_utc_ms
        .is_some_and(|last| input.now_ts_utc_ms <= last)
    {
        return Err(blocked(Stage5gTimerError::NonMonotonicCheckpoint, session));
    }

    let Stage5gTimerSession {
        stage5c_state,
        summary,
        replay,
        last_continuation_checkpoint_ts_utc_ms,
    } = session;
    match advance_stage5c_paper_loop_once(stage5c_state, Stage5cPaperLoopEvent::Timer(input)) {
        Ok(Stage5cPaperLoopState::TimerResolved(timer)) => timer_transition(
            settle_stage5c_timer_result(*timer),
            summary,
            replay,
            input.now_ts_utc_ms,
        ),
        Ok(stage5c_state) => Err(Stage5gTimerFailure::Blocked(Box::new(
            Stage5gTimerBlocked {
                reason: Stage5gTimerError::UnexpectedStage5cState,
                session: Stage5gTimerSession {
                    stage5c_state,
                    summary,
                    replay,
                    last_continuation_checkpoint_ts_utc_ms: Some(input.now_ts_utc_ms),
                },
            },
        ))),
        Err(failure) => {
            let reason = Stage5gTimerError::Stage5c(failure.reason());
            match failure.into_preserved_state() {
                Some(stage5c_state) => Err(Stage5gTimerFailure::Blocked(Box::new(
                    Stage5gTimerBlocked {
                        reason,
                        session: Stage5gTimerSession {
                            stage5c_state,
                            summary,
                            replay,
                            last_continuation_checkpoint_ts_utc_ms,
                        },
                    },
                ))),
                None => Err(Stage5gTimerFailure::Terminal(reason)),
            }
        }
    }
}

pub fn continue_stage5g_timer_with_timer(
    ready: Stage5gTimerReadyPaperStrategy,
    input: Stage5cPaperTimerInput,
) -> Result<Stage5gTimerTransition, Stage5gTimerFailure> {
    if input.now_ts_utc_ms <= ready.checkpoint_ts_utc_ms {
        return Err(ready_blocked(
            Stage5gTimerError::NonMonotonicCheckpoint,
            ready,
        ));
    }
    let Stage5gTimerReadyPaperStrategy {
        settlement,
        summary,
        replay,
        checkpoint_ts_utc_ms,
    } = ready;
    match advance_stage5c_timer_settlement_timer(settlement, input) {
        Ok(timer) => timer_transition(
            settle_stage5c_timer_result(timer),
            summary,
            replay,
            input.now_ts_utc_ms,
        ),
        Err(failure) => {
            let reason = Stage5gTimerError::Stage5c(Stage5cPaperLoopError::TimerContinuation(
                failure.reason(),
            ));
            match failure.into_blocked() {
                Some(blocked) => Err(Stage5gTimerFailure::Blocked(Box::new(
                    Stage5gTimerBlocked {
                        reason,
                        session: Stage5gTimerSession {
                            stage5c_state: Stage5cPaperLoopState::TimerSettlement(Box::new(
                                blocked.into_settlement(),
                            )),
                            summary,
                            replay,
                            last_continuation_checkpoint_ts_utc_ms: Some(checkpoint_ts_utc_ms),
                        },
                    },
                ))),
                None => Err(Stage5gTimerFailure::Terminal(reason)),
            }
        }
    }
}

pub fn continue_stage5g_timer_with_bar(
    ready: Stage5gTimerReadyPaperStrategy,
    accepted: Stage5cAcceptedSemanticBar,
) -> Result<Stage5gBarContinuationPaperStrategy, Stage5gTimerFailure> {
    let Stage5gTimerReadyPaperStrategy {
        settlement,
        summary,
        replay,
        checkpoint_ts_utc_ms,
    } = ready;
    if settlement
        .checkpoint_ts_utc_ms()
        .is_some_and(|inner| checkpoint_ts_utc_ms < inner)
    {
        return Err(Stage5gTimerFailure::Blocked(Box::new(
            Stage5gTimerBlocked {
                reason: Stage5gTimerError::ContinuationBeforeInnerSettlement,
                session: Stage5gTimerSession {
                    stage5c_state: Stage5cPaperLoopState::TimerSettlement(Box::new(settlement)),
                    summary,
                    replay,
                    last_continuation_checkpoint_ts_utc_ms: Some(checkpoint_ts_utc_ms),
                },
            },
        )));
    }
    let bar_checkpoint_ts_utc_ms = match stage5gd_accepted_bar_checkpoint_ts_utc_ms(&accepted) {
        Ok(checkpoint) => checkpoint,
        Err(reason) => {
            return Err(Stage5gTimerFailure::Blocked(Box::new(
                Stage5gTimerBlocked {
                    reason: Stage5gTimerError::Stage5c(Stage5cPaperLoopError::TimerContinuation(
                        reason,
                    )),
                    session: Stage5gTimerSession {
                        stage5c_state: Stage5cPaperLoopState::TimerSettlement(Box::new(settlement)),
                        summary,
                        replay,
                        last_continuation_checkpoint_ts_utc_ms: Some(checkpoint_ts_utc_ms),
                    },
                },
            )));
        }
    };
    match advance_stage5c_timer_settlement_next_bar_transactional_at_checkpoint(
        settlement,
        accepted,
        bar_checkpoint_ts_utc_ms,
        checkpoint_ts_utc_ms,
    ) {
        Ok(settled) => {
            let next_checkpoint = max_required_checkpoint(
                checkpoint_ts_utc_ms,
                replay.last_broker_truth_received_ms,
                bar_checkpoint_ts_utc_ms,
            );
            Ok(Stage5gBarContinuationPaperStrategy {
                settled,
                summary,
                replay,
                checkpoint_ts_utc_ms: next_checkpoint,
            })
        }
        Err(failure) => {
            let reason = Stage5gTimerError::Stage5c(Stage5cPaperLoopError::TimerContinuation(
                failure.reason(),
            ));
            match failure.into_blocked() {
                Some(blocked) => Err(Stage5gTimerFailure::Blocked(Box::new(
                    Stage5gTimerBlocked {
                        reason,
                        session: Stage5gTimerSession {
                            stage5c_state: Stage5cPaperLoopState::TimerSettlement(Box::new(
                                blocked.into_settlement(),
                            )),
                            summary,
                            replay,
                            last_continuation_checkpoint_ts_utc_ms: Some(checkpoint_ts_utc_ms),
                        },
                    },
                ))),
                None => Err(Stage5gTimerFailure::Terminal(reason)),
            }
        }
    }
}

/// Classifies an accepted Stage 5G-d bar result without exposing the raw
/// Stage 5C settled capability. Zero-intent bars retain their replay-owning
/// wrapper; generated intents enter the same escrow used by timer output.
pub fn settle_stage5g_bar_continuation(
    continuation: Stage5gBarContinuationPaperStrategy,
) -> Stage5gBarContinuationTransition {
    if continuation.settled.intent_batch().intent_count() == 0 {
        let Stage5gBarContinuationPaperStrategy {
            settled,
            summary,
            replay,
            checkpoint_ts_utc_ms,
        } = continuation;
        let settlement = stage5gd_rearm_zero_intent_bar_continuation(settled, checkpoint_ts_utc_ms)
            .expect("zero-intent classification re-arms only zero-intent settled state");
        Stage5gBarContinuationTransition::Ready(Stage5gTimerReadyPaperStrategy {
            settlement,
            summary,
            replay,
            checkpoint_ts_utc_ms,
        })
    } else {
        let Stage5gBarContinuationPaperStrategy {
            settled,
            summary,
            replay,
            checkpoint_ts_utc_ms,
        } = continuation;
        Stage5gBarContinuationTransition::GeneratedIntent(Stage5gTimerGeneratedIntentEscrow {
            settled,
            summary,
            replay,
            checkpoint_ts_utc_ms,
        })
    }
}

fn max_optional_checkpoint(left: Option<i64>, right: Option<i64>) -> Option<i64> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left.max(right)),
        (Some(value), None) | (None, Some(value)) => Some(value),
        (None, None) => None,
    }
}

fn max_required_checkpoint(previous: i64, broker: Option<i64>, event: i64) -> i64 {
    previous.max(broker.unwrap_or(i64::MIN)).max(event)
}

fn timer_transition(
    settlement: Stage5cTimerSettlement,
    summary: Stage5gOrderPositionSummary,
    replay: Stage5gReplayCheckpoint,
    checkpoint_ts_utc_ms: i64,
) -> Result<Stage5gTimerTransition, Stage5gTimerFailure> {
    if settlement.is_ready_for_continuation() {
        return Ok(Stage5gTimerTransition::Ready(
            Stage5gTimerReadyPaperStrategy {
                settlement,
                summary,
                replay,
                checkpoint_ts_utc_ms,
            },
        ));
    }
    match settlement.into_generated_intent_batch() {
        Ok(settled) => Ok(Stage5gTimerTransition::GeneratedIntent(
            Stage5gTimerGeneratedIntentEscrow {
                settled,
                summary,
                replay,
                checkpoint_ts_utc_ms,
            },
        )),
        Err(_) => Err(Stage5gTimerFailure::Terminal(
            Stage5gTimerError::UnexpectedStage5cState,
        )),
    }
}

fn blocked(reason: Stage5gTimerError, session: Stage5gTimerSession) -> Stage5gTimerFailure {
    Stage5gTimerFailure::Blocked(Box::new(Stage5gTimerBlocked { reason, session }))
}

fn ready_blocked(
    reason: Stage5gTimerError,
    ready: Stage5gTimerReadyPaperStrategy,
) -> Stage5gTimerFailure {
    let Stage5gTimerReadyPaperStrategy {
        settlement,
        summary,
        replay,
        checkpoint_ts_utc_ms,
    } = ready;
    blocked(
        reason,
        Stage5gTimerSession {
            stage5c_state: Stage5cPaperLoopState::TimerSettlement(Box::new(settlement)),
            summary,
            replay,
            last_continuation_checkpoint_ts_utc_ms: Some(checkpoint_ts_utc_ms),
        },
    )
}

impl Stage5gTimerSession {
    pub fn checkpoint(&self) -> Stage5gTimerCheckpointEnvelope {
        checkpoint_envelope(&self.replay, self.last_continuation_checkpoint_ts_utc_ms)
    }

    pub fn summary(&self) -> &Stage5gOrderPositionSummary {
        &self.summary
    }

    pub fn intent_sink_attached(&self) -> bool {
        false
    }
    pub fn redis_command_stream_attached(&self) -> bool {
        false
    }
    pub fn finam_transport_attached(&self) -> bool {
        false
    }
    pub fn broker_execution_attached(&self) -> bool {
        false
    }
}

impl Stage5gTimerReadyPaperStrategy {
    pub fn checkpoint(&self) -> Stage5gTimerCheckpointEnvelope {
        checkpoint_envelope(&self.replay, Some(self.checkpoint_ts_utc_ms))
    }
    pub fn checkpoint_ts_utc_ms(&self) -> i64 {
        self.checkpoint_ts_utc_ms
    }
    pub fn summary(&self) -> &Stage5gOrderPositionSummary {
        &self.summary
    }
    pub fn intent_sink_attached(&self) -> bool {
        false
    }
    pub fn redis_command_stream_attached(&self) -> bool {
        false
    }
    pub fn finam_transport_attached(&self) -> bool {
        false
    }
    pub fn broker_execution_attached(&self) -> bool {
        false
    }
}

impl Stage5gTimerGeneratedIntentEscrow {
    pub fn checkpoint(&self) -> Stage5gTimerCheckpointEnvelope {
        checkpoint_envelope(&self.replay, Some(self.checkpoint_ts_utc_ms))
    }
    pub fn intent_count(&self) -> usize {
        self.settled.intent_batch().intent_count()
    }
    pub fn request_ids(&self) -> &[StrategyRequestId] {
        self.settled.intent_batch().request_ids()
    }
    pub fn summary(&self) -> &Stage5gOrderPositionSummary {
        &self.summary
    }
    #[cfg(test)]
    pub(crate) fn source_intent_projections(
        &self,
    ) -> Vec<crate::stage5c_paper_host::Stage5gSourceIntentProjection> {
        self.settled.stage5g_source_intent_projections()
    }
    pub fn intent_sink_attached(&self) -> bool {
        false
    }
    pub fn redis_command_stream_attached(&self) -> bool {
        false
    }
    pub fn finam_transport_attached(&self) -> bool {
        false
    }
    pub fn broker_execution_attached(&self) -> bool {
        false
    }
}

pub fn attach_stage5g_timer_generated_mock_ack(
    escrow: Stage5gTimerGeneratedIntentEscrow,
    input: Stage5gMockAckSessionInput,
) -> Result<Stage5gTimerMockAckSession, Box<Stage5gTimerMockAckAdmissionBlocked>> {
    let Stage5gTimerGeneratedIntentEscrow {
        settled,
        summary,
        replay,
        checkpoint_ts_utc_ms,
    } = escrow;
    match attach_stage5g_mock_ack_session(settled, input) {
        Ok(inner) => Ok(Stage5gTimerMockAckSession {
            inner,
            summary,
            replay,
            checkpoint_ts_utc_ms,
        }),
        Err(blocked) => Err(Box::new(Stage5gTimerMockAckAdmissionBlocked {
            blocked,
            summary,
            replay,
            checkpoint_ts_utc_ms,
        })),
    }
}

pub fn apply_stage5g_timer_mock_ack(
    session: Stage5gTimerMockAckSession,
    event: Stage5gMockAckEvent,
) -> Result<Stage5gTimerMockAckTransition, Stage5gTimerMockAckFailure> {
    if event.ack.received_ts.timestamp_millis() < session.checkpoint_ts_utc_ms {
        return Err(Stage5gTimerMockAckFailure::Blocked(Box::new(
            Stage5gTimerMockAckBlocked {
                reason: Stage5gTimerMockAckError::AckBeforeContinuationCheckpoint,
                session,
            },
        )));
    }
    let Stage5gTimerMockAckSession {
        inner,
        summary,
        replay,
        checkpoint_ts_utc_ms,
    } = session;
    match apply_stage5g_mock_ack(inner, event) {
        Ok(Stage5gMockAckTransition::Awaiting(inner)) => Ok(
            Stage5gTimerMockAckTransition::Awaiting(Stage5gTimerMockAckSession {
                inner,
                summary,
                replay,
                checkpoint_ts_utc_ms,
            }),
        ),
        Ok(Stage5gMockAckTransition::Resolved(inner)) => Ok(
            Stage5gTimerMockAckTransition::Resolved(Stage5gTimerResolvedMockAckPaperStrategy {
                inner,
                summary,
                replay,
                checkpoint_ts_utc_ms,
            }),
        ),
        Err(Stage5gMockAckFailure::Blocked(blocked)) => {
            let reason = blocked.reason();
            Err(Stage5gTimerMockAckFailure::Blocked(Box::new(
                Stage5gTimerMockAckBlocked {
                    reason: Stage5gTimerMockAckError::MockAck(reason),
                    session: Stage5gTimerMockAckSession {
                        inner: blocked.into_session(),
                        summary,
                        replay,
                        checkpoint_ts_utc_ms,
                    },
                },
            )))
        }
        Err(failure @ Stage5gMockAckFailure::Terminal(_)) => {
            Err(Stage5gTimerMockAckFailure::Terminal(failure))
        }
    }
}

pub fn attach_stage5g_timer_order_position_session(
    resolved: Stage5gTimerResolvedMockAckPaperStrategy,
) -> Result<Stage5gOrderPositionSession, Box<Stage5gTimerOrderPositionAdmissionBlocked>> {
    let Stage5gTimerResolvedMockAckPaperStrategy {
        inner,
        summary,
        mut replay,
        checkpoint_ts_utc_ms,
    } = resolved;
    replay.last_continuation_checkpoint_ts_utc_ms = max_optional_checkpoint(
        replay.last_continuation_checkpoint_ts_utc_ms,
        Some(checkpoint_ts_utc_ms),
    );
    match attach_stage5g_order_position_session_with_replay(inner, Some(replay.clone())) {
        Ok(session) => Ok(session),
        Err(blocked) => {
            let reason = blocked.reason();
            Err(Box::new(Stage5gTimerOrderPositionAdmissionBlocked {
                reason,
                resolved: Stage5gTimerResolvedMockAckPaperStrategy {
                    inner: blocked.into_ack_resolved(),
                    summary,
                    replay,
                    checkpoint_ts_utc_ms,
                },
            }))
        }
    }
}

impl Stage5gTimerMockAckAdmissionBlocked {
    pub fn reason(&self) -> Stage5gMockAckAdmissionError {
        self.blocked.reason()
    }

    pub fn into_escrow(self) -> Stage5gTimerGeneratedIntentEscrow {
        Stage5gTimerGeneratedIntentEscrow {
            settled: self.blocked.into_settled(),
            summary: self.summary,
            replay: self.replay,
            checkpoint_ts_utc_ms: self.checkpoint_ts_utc_ms,
        }
    }
}

impl Stage5gTimerMockAckBlocked {
    pub fn reason(&self) -> Stage5gTimerMockAckError {
        self.reason
    }

    pub fn into_session(self) -> Stage5gTimerMockAckSession {
        self.session
    }
}

impl Stage5gTimerOrderPositionAdmissionBlocked {
    pub fn reason(&self) -> crate::Stage5gOrderPositionAdmissionError {
        self.reason
    }

    pub fn checkpoint(&self) -> Stage5gTimerCheckpointEnvelope {
        self.resolved.checkpoint()
    }

    pub fn retry(
        self,
    ) -> Result<Stage5gOrderPositionSession, Box<Stage5gTimerOrderPositionAdmissionBlocked>> {
        attach_stage5g_timer_order_position_session(self.resolved)
    }
}

impl Stage5gTimerMockAckSession {
    pub fn checkpoint(&self) -> Stage5gTimerCheckpointEnvelope {
        checkpoint_envelope(&self.replay, Some(self.checkpoint_ts_utc_ms))
    }
    pub fn summary(&self) -> &Stage5gOrderPositionSummary {
        &self.summary
    }
}

impl Stage5gTimerResolvedMockAckPaperStrategy {
    pub fn checkpoint(&self) -> Stage5gTimerCheckpointEnvelope {
        checkpoint_envelope(&self.replay, Some(self.checkpoint_ts_utc_ms))
    }
    pub fn summary(&self) -> &Stage5gOrderPositionSummary {
        &self.summary
    }
}

impl Stage5gBarContinuationPaperStrategy {
    pub fn checkpoint(&self) -> Stage5gTimerCheckpointEnvelope {
        checkpoint_envelope(&self.replay, Some(self.checkpoint_ts_utc_ms))
    }
    pub fn intent_count(&self) -> usize {
        self.settled.intent_batch().intent_count()
    }
    pub fn summary(&self) -> &Stage5gOrderPositionSummary {
        &self.summary
    }
}

pub fn validate_stage5g_timer_checkpoint(
    envelope: &Stage5gTimerCheckpointEnvelope,
) -> Result<(), Stage5gTimerCheckpointError> {
    let payload = &envelope.payload;
    if payload.schema_version != STAGE5G_TIMER_CHECKPOINT_SCHEMA_VERSION {
        return Err(Stage5gTimerCheckpointError::UnsupportedSchema);
    }
    if payload.payload_fingerprint() != envelope.payload_sha256 {
        return Err(Stage5gTimerCheckpointError::FingerprintMismatch);
    }
    if payload.evidence_replay_ledger.is_empty() {
        return Err(Stage5gTimerCheckpointError::MissingReplayLedger);
    }
    let package_discriminator = payload
        .package_discriminator
        .as_deref()
        .ok_or(Stage5gTimerCheckpointError::MissingPackageDiscriminator)?;
    let received_at = payload
        .last_broker_truth_received_at
        .ok_or(Stage5gTimerCheckpointError::MissingExactBrokerTruthReceipt)?;
    let received_ms = payload
        .last_broker_truth_received_ms
        .ok_or(Stage5gTimerCheckpointError::MissingMillisecondWatermark)?;
    let expected_discriminator = format!(
        "moex.broker-truth.package.v1:{}:{:09}",
        received_at.timestamp(),
        received_at.timestamp_subsec_nanos()
    );
    if package_discriminator != expected_discriminator {
        return Err(Stage5gTimerCheckpointError::PackageDiscriminatorMismatch);
    }
    if received_ms != received_at.timestamp_millis() {
        return Err(Stage5gTimerCheckpointError::MillisecondWatermarkMismatch);
    }
    let mut identities = std::collections::HashSet::new();
    let mut previous_ledger_receipt = None;
    let mut final_ledger_identity = None;
    let mut final_ledger_receipt = None;
    for entry in &payload.evidence_replay_ledger {
        if entry.identity.is_empty()
            || entry.fingerprint_sha256.len() != 64
            || !entry
                .fingerprint_sha256
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        {
            return Err(Stage5gTimerCheckpointError::InvalidReplayLedgerEntry);
        }
        if !identities.insert(entry.identity.as_str()) {
            return Err(Stage5gTimerCheckpointError::DuplicateReplayIdentity);
        }
        let parsed = parse_replay_evidence_identity(&entry.identity)
            .ok_or(Stage5gTimerCheckpointError::InvalidReplayLedgerEntry)?;
        if previous_ledger_receipt.is_some_and(|previous| parsed.received_at < previous) {
            return Err(Stage5gTimerCheckpointError::ReplayLedgerReceiptRegression);
        }
        previous_ledger_receipt = Some(parsed.received_at);
        final_ledger_identity = Some(entry.identity.as_str());
        final_ledger_receipt = Some(parsed.received_at);
    }
    let current_identity = payload
        .current_evidence_identity
        .as_deref()
        .ok_or(Stage5gTimerCheckpointError::MissingCurrentEvidenceIdentity)?;
    let current = parse_replay_evidence_identity(current_identity)
        .ok_or(Stage5gTimerCheckpointError::InvalidCurrentEvidenceIdentity)?;
    if current.package_discriminator != package_discriminator {
        return Err(Stage5gTimerCheckpointError::CurrentPackageReceiptMismatch);
    }
    let exact_current_identity_count = payload
        .evidence_replay_ledger
        .iter()
        .filter(|entry| entry.identity == current_identity)
        .count();
    if exact_current_identity_count == 0 {
        return Err(Stage5gTimerCheckpointError::CurrentPackageMissingFromReplayLedger);
    }
    if exact_current_identity_count != 1 {
        return Err(Stage5gTimerCheckpointError::AmbiguousCurrentEvidenceIdentity);
    }
    if final_ledger_identity != Some(current_identity) {
        return Err(Stage5gTimerCheckpointError::CurrentEvidenceIdentityNotLatest);
    }
    if current.received_at != received_at || final_ledger_receipt != Some(received_at) {
        return Err(Stage5gTimerCheckpointError::CurrentPackageReceiptMismatch);
    }
    let last_total_sequence = payload
        .last_total_sequence
        .ok_or(Stage5gTimerCheckpointError::MissingTotalSequence)?;
    let continuation_checkpoint = payload
        .last_continuation_checkpoint_ts_utc_ms
        .ok_or(Stage5gTimerCheckpointError::MissingContinuationCheckpoint)?;
    if continuation_checkpoint < received_ms {
        return Err(Stage5gTimerCheckpointError::ContinuationBeforeBrokerTruth);
    }
    let minimum_sequence = payload
        .evidence_replay_ledger
        .len()
        .checked_add(payload.duplicate_evidence_count)
        .and_then(|count| u64::try_from(count).ok())
        .ok_or(Stage5gTimerCheckpointError::DuplicateCounterIncoherent)?;
    if last_total_sequence < minimum_sequence {
        return Err(Stage5gTimerCheckpointError::DuplicateCounterIncoherent);
    }
    Ok(())
}

pub fn classify_stage5g_post_checkpoint_evidence(
    envelope: &Stage5gTimerCheckpointEnvelope,
    evidence: Stage5gOrderPositionEvidence,
) -> Result<Stage5gCheckpointReplayResult, Stage5gCheckpointReplayError> {
    validate_stage5g_timer_checkpoint(envelope)
        .map_err(|_| Stage5gCheckpointReplayError::ConflictingDuplicateEvidence)?;
    if envelope
        .payload
        .last_total_sequence
        .is_some_and(|last| evidence.total_sequence <= last)
    {
        return Err(Stage5gCheckpointReplayError::NonMonotonicSequence);
    }
    let total_sequence = evidence.total_sequence;
    let canonical_evidence =
        canonicalize_stage5g_order_position_evidence(evidence).map_err(|reason| match reason {
            Stage5gEvidenceCanonicalizationError::TradeIdentityConflict => {
                Stage5gCheckpointReplayError::TradeIdentityConflict
            }
            Stage5gEvidenceCanonicalizationError::EvidenceIdentityGrammarViolation => {
                Stage5gCheckpointReplayError::EvidenceIdentityGrammarViolation
            }
        })?;
    let identity = canonical_evidence.identity().to_string();
    let fingerprint = canonical_evidence.fingerprint().to_string();
    let mut replay = replay_from_payload(&envelope.payload);
    if let Some(previous) = replay
        .evidence_identities
        .iter()
        .find(|previous| previous.identity == identity)
    {
        if previous.fingerprint != fingerprint {
            return Err(Stage5gCheckpointReplayError::ConflictingDuplicateEvidence);
        }
        replay.last_total_sequence = Some(total_sequence);
        replay.duplicate_evidence_count += 1;
        return Ok(Stage5gCheckpointReplayResult {
            disposition: Stage5gCheckpointReplayDisposition::ExactReplay,
            replay,
            last_continuation_checkpoint_ts_utc_ms: envelope
                .payload
                .last_continuation_checkpoint_ts_utc_ms,
            canonical_new_package_candidate: None,
        });
    }
    let received_at = canonical_evidence.evidence().broker_truth.received_ts;
    let continuation_checkpoint = envelope
        .payload
        .last_continuation_checkpoint_ts_utc_ms
        .expect("validated Stage 5G-d checkpoint has a continuation watermark");
    if received_at.timestamp_millis() < continuation_checkpoint {
        return Err(Stage5gCheckpointReplayError::BrokerTruthBeforeContinuationCheckpoint);
    }
    if replay
        .last_broker_truth_received_at
        .is_some_and(|last| received_at < last)
    {
        return Err(Stage5gCheckpointReplayError::BrokerTruthTimeRegression);
    }
    replay.evidence_identities.push(EvidenceIdentity {
        identity: identity.clone(),
        fingerprint,
    });
    replay.current_evidence_identity = Some(identity);
    replay.last_total_sequence = Some(total_sequence);
    replay.last_broker_truth_received_at = Some(received_at);
    replay.last_broker_truth_received_ms = Some(received_at.timestamp_millis());
    replay.last_continuation_checkpoint_ts_utc_ms = max_optional_checkpoint(
        replay.last_continuation_checkpoint_ts_utc_ms,
        replay.last_broker_truth_received_ms,
    );
    replay.package_discriminator = Some(format!(
        "moex.broker-truth.package.v1:{}:{:09}",
        received_at.timestamp(),
        received_at.timestamp_subsec_nanos()
    ));
    Ok(Stage5gCheckpointReplayResult {
        disposition: Stage5gCheckpointReplayDisposition::NewPackage,
        replay,
        last_continuation_checkpoint_ts_utc_ms: envelope
            .payload
            .last_continuation_checkpoint_ts_utc_ms,
        canonical_new_package_candidate: Some(canonical_evidence),
    })
}

fn checkpoint_envelope(
    replay: &Stage5gReplayCheckpoint,
    last_continuation_checkpoint_ts_utc_ms: Option<i64>,
) -> Stage5gTimerCheckpointEnvelope {
    let last_continuation_checkpoint_ts_utc_ms = max_optional_checkpoint(
        max_optional_checkpoint(
            replay.last_continuation_checkpoint_ts_utc_ms,
            last_continuation_checkpoint_ts_utc_ms,
        ),
        replay.last_broker_truth_received_ms,
    );
    let payload = Stage5gTimerCheckpointPayload {
        schema_version: STAGE5G_TIMER_CHECKPOINT_SCHEMA_VERSION,
        package_discriminator: replay.package_discriminator.clone(),
        current_evidence_identity: replay.current_evidence_identity.clone(),
        evidence_replay_ledger: replay
            .evidence_identities
            .iter()
            .map(|item| Stage5gReplayLedgerEntry {
                identity: item.identity.clone(),
                fingerprint_sha256: item.fingerprint.clone(),
            })
            .collect(),
        last_broker_truth_received_at: replay.last_broker_truth_received_at,
        last_broker_truth_received_ms: replay.last_broker_truth_received_ms,
        duplicate_evidence_count: replay.duplicate_evidence_count,
        last_total_sequence: replay.last_total_sequence,
        last_continuation_checkpoint_ts_utc_ms,
    };
    let payload_sha256 = payload.payload_fingerprint();
    Stage5gTimerCheckpointEnvelope {
        payload,
        payload_sha256,
    }
}

fn replay_from_payload(payload: &Stage5gTimerCheckpointPayload) -> Stage5gReplayCheckpoint {
    Stage5gReplayCheckpoint {
        schema_version: 1,
        package_discriminator: payload.package_discriminator.clone(),
        current_evidence_identity: payload.current_evidence_identity.clone(),
        evidence_identities: payload
            .evidence_replay_ledger
            .iter()
            .map(|entry| EvidenceIdentity {
                identity: entry.identity.clone(),
                fingerprint: entry.fingerprint_sha256.clone(),
            })
            .collect(),
        last_broker_truth_received_at: payload.last_broker_truth_received_at,
        last_broker_truth_received_ms: payload.last_broker_truth_received_ms,
        duplicate_evidence_count: payload.duplicate_evidence_count,
        last_total_sequence: payload.last_total_sequence,
        last_continuation_checkpoint_ts_utc_ms: payload.last_continuation_checkpoint_ts_utc_ms,
    }
}

struct ParsedReplayEvidenceIdentity<'a> {
    package_discriminator: &'a str,
    received_at: DateTime<Utc>,
}

fn parse_replay_evidence_identity(identity: &str) -> Option<ParsedReplayEvidenceIdentity<'_>> {
    const PREFIX: &str = "moex.stage5g.order-position-evidence-identity.v3:";
    let rest = identity.strip_prefix(PREFIX)?;
    let mut parts = rest.splitn(3, ':');
    let request_id = parts.next()?;
    let account_id = parts.next()?;
    let package_discriminator = parts.next()?;
    let parsed_request_id = uuid::Uuid::parse_str(request_id).ok()?;
    if parsed_request_id.to_string() != request_id
        || account_id.is_empty()
        || account_id.contains(':')
    {
        return None;
    }
    const PACKAGE_PREFIX: &str = "moex.broker-truth.package.v1:";
    let package_clock = package_discriminator.strip_prefix(PACKAGE_PREFIX)?;
    let (seconds_raw, nanos_raw) = package_clock.split_once(':')?;
    if nanos_raw.len() != 9 || !nanos_raw.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let seconds = seconds_raw.parse::<i64>().ok()?;
    let nanos = nanos_raw.parse::<u32>().ok()?;
    let received_at = Utc.timestamp_opt(seconds, nanos).single()?;
    let canonical = format!(
        "moex.broker-truth.package.v1:{}:{:09}",
        received_at.timestamp(),
        received_at.timestamp_subsec_nanos()
    );
    if canonical != package_discriminator {
        return None;
    }
    Some(ParsedReplayEvidenceIdentity {
        package_discriminator,
        received_at,
    })
}

impl Stage5gTimerCheckpointPayload {
    fn payload_fingerprint(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(STAGE5G_TIMER_CHECKPOINT_DOMAIN);
        hasher.update(
            serde_json::to_vec(self).expect("Stage 5G-d timer checkpoint payload serializes"),
        );
        format!("{:x}", hasher.finalize())
    }
}

#[allow(dead_code)]
fn _assert_stage5c_timer_error_is_used(_: Stage5cTimerContinuationError) {}

#[cfg(test)]
mod tests {
    use broker_core::{
        BrokerAccountId, BrokerPositionSnapshot, BrokerTradeId, BrokerTradeSnapshot, Exchange,
        InstrumentId, Market, OrderSide,
    };
    use chrono::TimeZone;
    use rust_decimal::Decimal;

    use super::*;

    fn canonical_evidence(
        evidence: &Stage5gOrderPositionEvidence,
    ) -> Stage5gCanonicalOrderPositionEvidence {
        canonicalize_stage5g_order_position_evidence(evidence.clone())
            .expect("test evidence canonicalizes")
    }

    fn evidence_identity(evidence: &Stage5gOrderPositionEvidence) -> String {
        canonical_evidence(evidence).identity().to_string()
    }

    fn evidence_fingerprint(evidence: &Stage5gOrderPositionEvidence) -> String {
        canonical_evidence(evidence).fingerprint().to_string()
    }

    fn target() -> InstrumentId {
        InstrumentId {
            symbol: "IMOEXF".to_string(),
            venue_symbol: Some("IMOEXF@RTSX".to_string()),
            exchange: Exchange::Moex,
            market: Market::Futures,
        }
    }

    fn received(nanos: u32) -> DateTime<Utc> {
        Utc.timestamp_opt(1_785_663_000, nanos).single().unwrap()
    }

    fn evidence(sequence: u64, nanos: u32) -> Stage5gOrderPositionEvidence {
        Stage5gOrderPositionEvidence {
            total_sequence: sequence,
            request_id: StrategyRequestId::from(uuid::Uuid::from_u128(0x005d_0001)),
            broker_truth: broker_core::BrokerTruthSnapshot {
                account_id: BrokerAccountId::new("ACC_TEST_0001"),
                orders: Vec::new(),
                positions: Vec::new(),
                cash: None,
                trades: Vec::new(),
                instruments: Vec::new(),
                received_ts: received(nanos),
            },
            order_attribution: None,
        }
    }

    fn trade(
        id: &str,
        price: Decimal,
        source_nanos: u32,
        received_nanos: u32,
    ) -> BrokerTradeSnapshot {
        BrokerTradeSnapshot {
            account_id: BrokerAccountId::new("ACC_TEST_0001"),
            broker_trade_id: BrokerTradeId::new(id),
            broker_order_id: None,
            client_order_id: None,
            instrument: target(),
            side: OrderSide::Buy,
            qty: Decimal::ONE,
            price,
            gross_amount: None,
            commission: None,
            broker_asset_id: None,
            board: None,
            expiration_date: None,
            source_ts: received(source_nanos),
            received_ts: received(received_nanos),
        }
    }

    fn evidence_with_trades(
        sequence: u64,
        package_nanos: u32,
        trades: Vec<BrokerTradeSnapshot>,
    ) -> Stage5gOrderPositionEvidence {
        let mut event = evidence(sequence, package_nanos);
        event.broker_truth.trades = trades;
        event
    }

    fn checkpoint_for(event: &Stage5gOrderPositionEvidence) -> Stage5gTimerCheckpointEnvelope {
        checkpoint_envelope(
            &Stage5gReplayCheckpoint {
                schema_version: 1,
                package_discriminator: Some(format!(
                    "moex.broker-truth.package.v1:{}:{:09}",
                    event.broker_truth.received_ts.timestamp(),
                    event.broker_truth.received_ts.timestamp_subsec_nanos()
                )),
                current_evidence_identity: Some(evidence_identity(event)),
                evidence_identities: vec![EvidenceIdentity {
                    identity: evidence_identity(event),
                    fingerprint: evidence_fingerprint(event),
                }],
                last_broker_truth_received_at: Some(event.broker_truth.received_ts),
                last_broker_truth_received_ms: Some(
                    event.broker_truth.received_ts.timestamp_millis(),
                ),
                duplicate_evidence_count: 0,
                last_total_sequence: Some(event.total_sequence),
                last_continuation_checkpoint_ts_utc_ms: Some(
                    event.broker_truth.received_ts.timestamp_millis(),
                ),
            },
            Some(event.broker_truth.received_ts.timestamp_millis()),
        )
    }

    fn rehash(envelope: &mut Stage5gTimerCheckpointEnvelope) {
        envelope.payload_sha256 = envelope.payload.payload_fingerprint();
    }

    fn two_package_checkpoint() -> (
        Stage5gTimerCheckpointEnvelope,
        Stage5gOrderPositionEvidence,
        Stage5gOrderPositionEvidence,
    ) {
        let first = evidence(7, 100_000_000);
        let initial = checkpoint_for(&first);
        let second = evidence(8, 200_000_000);
        let checkpoint = classify_stage5g_post_checkpoint_evidence(&initial, second.clone())
            .unwrap()
            .checkpoint();
        (checkpoint, first, second)
    }

    #[test]
    fn checkpoint_roundtrip_preserves_exact_nanos_and_replay_ledger() {
        let event = evidence(7, 125_875_321);
        let envelope = checkpoint_for(&event);
        validate_stage5g_timer_checkpoint(&envelope).unwrap();
        let encoded = serde_json::to_vec(&envelope).unwrap();
        let restored: Stage5gTimerCheckpointEnvelope = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(restored, envelope);
        assert_eq!(
            restored.payload.last_broker_truth_received_at,
            Some(received(125_875_321))
        );
        assert_eq!(restored.payload.evidence_replay_ledger.len(), 1);
        assert_eq!(restored.payload.last_total_sequence, Some(7));
    }

    #[test]
    fn exact_redelivery_uses_new_local_sequence_but_same_package_identity() {
        let original = evidence(7, 125_875_321);
        let mut envelope = checkpoint_for(&original);
        envelope.payload.last_continuation_checkpoint_ts_utc_ms =
            Some(original.broker_truth.received_ts.timestamp_millis() + 10_000);
        rehash(&mut envelope);
        let replay = evidence(8, 125_875_321);
        assert_eq!(evidence_identity(&original), evidence_identity(&replay));
        assert_eq!(
            evidence_fingerprint(&original),
            evidence_fingerprint(&replay)
        );
        let result = classify_stage5g_post_checkpoint_evidence(&envelope, replay).unwrap();
        assert_eq!(
            result.disposition(),
            Stage5gCheckpointReplayDisposition::ExactReplay
        );
        let next = result.checkpoint();
        assert_eq!(next.payload.last_total_sequence, Some(8));
        assert_eq!(next.payload.duplicate_evidence_count, 1);
        assert_eq!(
            next.payload.package_discriminator,
            envelope.payload.package_discriminator
        );
    }

    #[test]
    fn changed_payload_under_same_package_identity_fails_closed() {
        let original = evidence(7, 125_875_321);
        let envelope = checkpoint_for(&original);
        let mut changed = evidence(8, 125_875_321);
        changed.broker_truth.positions.push(BrokerPositionSnapshot {
            account_id: BrokerAccountId::new("ACC_TEST_0001"),
            instrument: target(),
            qty: Decimal::ONE,
            avg_price: None,
            unrealized_pnl: None,
            source_ts: Some(received(125_875_321)),
            received_ts: received(125_875_321),
        });
        assert_eq!(evidence_identity(&original), evidence_identity(&changed));
        assert_ne!(
            evidence_fingerprint(&original),
            evidence_fingerprint(&changed)
        );
        assert_eq!(
            classify_stage5g_post_checkpoint_evidence(&envelope, changed).unwrap_err(),
            Stage5gCheckpointReplayError::ConflictingDuplicateEvidence
        );
    }

    #[test]
    fn post_checkpoint_duplicate_trade_redelivery_matches_active_canonical_fingerprint() {
        let first = trade(
            "TRADE_CANONICAL_A",
            Decimal::new(2_210, 0),
            70_000_000,
            80_000_000,
        );
        let mut refreshed = first.clone();
        refreshed.received_ts = received(90_000_000);
        let original = evidence_with_trades(7, 100_000_000, vec![first, refreshed]);
        let canonical = canonical_evidence(&original);
        assert_eq!(canonical.evidence().broker_truth.trades.len(), 1);
        assert_eq!(
            canonical.evidence().broker_truth.trades[0].received_ts,
            received(90_000_000)
        );
        let envelope = checkpoint_for(&original);
        assert_eq!(
            envelope.payload.evidence_replay_ledger[0].fingerprint_sha256,
            canonical.fingerprint()
        );

        let mut raw_redelivery = original.clone();
        raw_redelivery.total_sequence = 8;
        raw_redelivery.broker_truth.trades.reverse();
        let replay = classify_stage5g_post_checkpoint_evidence(&envelope, raw_redelivery).unwrap();
        assert_eq!(
            replay.disposition(),
            Stage5gCheckpointReplayDisposition::ExactReplay
        );
        assert!(!replay.owns_canonical_new_package_candidate());
    }

    #[test]
    fn stage5gd_r4_active_restart_exact_duplicate_reversal_is_exact_replay() {
        let first = trade(
            "TRADE_R4_EXACT_REPLAY",
            Decimal::new(2_210, 0),
            70_000_000,
            80_000_000,
        );
        let mut refreshed = first.clone();
        refreshed.received_ts = received(90_000_000);
        let active = evidence_with_trades(7, 100_000_000, vec![first.clone(), refreshed.clone()]);
        let active_reversed =
            evidence_with_trades(7, 100_000_000, vec![refreshed.clone(), first.clone()]);
        let active_checkpoint = checkpoint_for(&active);
        assert_eq!(active_checkpoint, checkpoint_for(&active_reversed));
        assert_eq!(
            evidence_fingerprint(&active),
            evidence_fingerprint(&active_reversed)
        );

        let restart = evidence_with_trades(8, 100_000_000, vec![refreshed, first]);
        let replay =
            classify_stage5g_post_checkpoint_evidence(&active_checkpoint, restart).unwrap();
        assert_eq!(
            replay.disposition(),
            Stage5gCheckpointReplayDisposition::ExactReplay
        );
        assert!(!replay.owns_canonical_new_package_candidate());
    }

    #[test]
    fn stage5gd_r4_new_package_instrument_conflicts_preserve_checkpoint() {
        let base = evidence(7, 100_000_000);
        let checkpoint = checkpoint_for(&base);
        let original_checkpoint = checkpoint.clone();
        let with_venue = trade(
            "TRADE_R4_VENUE_OPTION",
            Decimal::new(2_210, 0),
            150_000_000,
            160_000_000,
        );
        let mut without_venue = with_venue.clone();
        without_venue.instrument.venue_symbol = None;
        without_venue.received_ts = received(180_000_000);

        for rows in [
            vec![with_venue.clone(), without_venue.clone()],
            vec![without_venue.clone(), with_venue.clone()],
        ] {
            let candidate = evidence_with_trades(8, 200_000_000, rows);
            assert_eq!(
                classify_stage5g_post_checkpoint_evidence(&checkpoint, candidate).unwrap_err(),
                Stage5gCheckpointReplayError::TradeIdentityConflict
            );
            assert_eq!(checkpoint, original_checkpoint);
        }
    }

    #[test]
    fn post_checkpoint_known_payload_change_and_trade_identity_conflict_fail_closed() {
        let original = evidence_with_trades(
            7,
            100_000_000,
            vec![trade(
                "TRADE_CONFLICT_A",
                Decimal::new(2_210, 0),
                70_000_000,
                80_000_000,
            )],
        );
        let envelope = checkpoint_for(&original);

        let mut changed = original.clone();
        changed.total_sequence = 8;
        changed.broker_truth.trades[0].price += Decimal::ONE;
        assert_eq!(
            classify_stage5g_post_checkpoint_evidence(&envelope, changed).unwrap_err(),
            Stage5gCheckpointReplayError::ConflictingDuplicateEvidence
        );

        let mut conflicting = original.clone();
        conflicting.total_sequence = 8;
        let mut changed_duplicate = conflicting.broker_truth.trades[0].clone();
        changed_duplicate.price += Decimal::ONE;
        conflicting.broker_truth.trades.push(changed_duplicate);
        assert_eq!(
            classify_stage5g_post_checkpoint_evidence(&envelope, conflicting).unwrap_err(),
            Stage5gCheckpointReplayError::TradeIdentityConflict
        );
        assert_eq!(validate_stage5g_timer_checkpoint(&envelope), Ok(()));
    }

    #[test]
    fn new_post_checkpoint_package_owns_one_deduplicated_canonical_candidate() {
        let base = evidence(7, 100_000_000);
        let envelope = checkpoint_for(&base);
        let first = trade(
            "TRADE_NEW_A",
            Decimal::new(2_210, 0),
            150_000_000,
            160_000_000,
        );
        let mut refreshed = first.clone();
        refreshed.received_ts = received(180_000_000);
        let new_package = evidence_with_trades(8, 200_000_000, vec![refreshed, first]);
        let result = classify_stage5g_post_checkpoint_evidence(&envelope, new_package).unwrap();
        assert_eq!(
            result.disposition(),
            Stage5gCheckpointReplayDisposition::NewPackage
        );
        assert!(result.owns_canonical_new_package_candidate());
        let candidate = result.canonical_new_package_candidate.as_ref().unwrap();
        assert_eq!(candidate.evidence().broker_truth.trades.len(), 1);
        assert_eq!(
            result
                .checkpoint()
                .payload
                .evidence_replay_ledger
                .last()
                .unwrap()
                .fingerprint_sha256,
            candidate.fingerprint()
        );

        let first = trade(
            "TRADE_NEW_CONFLICT",
            Decimal::new(2_210, 0),
            150_000_000,
            160_000_000,
        );
        let mut conflicting = first.clone();
        conflicting.price += Decimal::ONE;
        let conflicting_package = evidence_with_trades(8, 200_000_000, vec![first, conflicting]);
        assert_eq!(
            classify_stage5g_post_checkpoint_evidence(&envelope, conflicting_package).unwrap_err(),
            Stage5gCheckpointReplayError::TradeIdentityConflict
        );
        assert_eq!(validate_stage5g_timer_checkpoint(&envelope), Ok(()));
    }

    #[test]
    fn replay_identity_grammar_requires_canonical_uuid_and_colon_free_account() {
        let event = evidence(7, 125_875_321);
        let mut noncanonical_uuid = checkpoint_for(&event);
        let canonical_request = event.request_id.to_string();
        let compact_request = canonical_request.replace('-', "");
        noncanonical_uuid.payload.current_evidence_identity = Some(
            noncanonical_uuid
                .payload
                .current_evidence_identity
                .as_deref()
                .unwrap()
                .replacen(&canonical_request, &compact_request, 1),
        );
        rehash(&mut noncanonical_uuid);
        assert_eq!(
            validate_stage5g_timer_checkpoint(&noncanonical_uuid),
            Err(Stage5gTimerCheckpointError::InvalidCurrentEvidenceIdentity)
        );

        let mut invalid_account = event;
        invalid_account.broker_truth.account_id = BrokerAccountId::new("ACC:INVALID");
        assert_eq!(
            canonicalize_stage5g_order_position_evidence(invalid_account).unwrap_err(),
            Stage5gEvidenceCanonicalizationError::EvidenceIdentityGrammarViolation
        );
    }

    #[test]
    fn two_packages_in_one_millisecond_keep_distinct_exact_identity() {
        let first = evidence(7, 125_100_000);
        let envelope = checkpoint_for(&first);
        let second = evidence(8, 125_900_000);
        assert_eq!(
            first.broker_truth.received_ts.timestamp_millis(),
            second.broker_truth.received_ts.timestamp_millis()
        );
        assert_ne!(evidence_identity(&first), evidence_identity(&second));
        let result = classify_stage5g_post_checkpoint_evidence(&envelope, second).unwrap();
        assert_eq!(
            result.disposition(),
            Stage5gCheckpointReplayDisposition::NewPackage
        );
        let next = result.checkpoint();
        assert_eq!(next.payload.evidence_replay_ledger.len(), 2);
        assert_eq!(
            next.payload.last_broker_truth_received_ms,
            Some(1_785_663_000_125)
        );
        assert!(next
            .payload
            .package_discriminator
            .as_deref()
            .unwrap()
            .ends_with(":125900000"));
        validate_stage5g_timer_checkpoint(&next).unwrap();
        let encoded = serde_json::to_vec(&next).unwrap();
        let restored: Stage5gTimerCheckpointEnvelope = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(restored, next);
    }

    #[test]
    fn new_post_restore_package_requires_continuation_chronology_but_exact_replay_does_not() {
        let original = evidence(7, 100_000_000);
        let mut envelope = checkpoint_for(&original);
        let same_second_floor = original
            .broker_truth
            .received_ts
            .timestamp_millis()
            .div_euclid(1_000)
            * 1_000;
        let continuation = same_second_floor + 900;
        envelope.payload.last_continuation_checkpoint_ts_utc_ms = Some(continuation);
        rehash(&mut envelope);
        validate_stage5g_timer_checkpoint(&envelope).unwrap();

        let early_new = evidence(8, 200_000_000);
        assert_eq!(
            classify_stage5g_post_checkpoint_evidence(&envelope, early_new).unwrap_err(),
            Stage5gCheckpointReplayError::BrokerTruthBeforeContinuationCheckpoint
        );

        let corrected = evidence(8, 950_000_000);
        let corrected_result =
            classify_stage5g_post_checkpoint_evidence(&envelope, corrected).unwrap();
        assert_eq!(
            corrected_result.disposition(),
            Stage5gCheckpointReplayDisposition::NewPackage
        );

        let exact_replay = evidence(8, 100_000_000);
        let exact_result =
            classify_stage5g_post_checkpoint_evidence(&envelope, exact_replay).unwrap();
        assert_eq!(
            exact_result.disposition(),
            Stage5gCheckpointReplayDisposition::ExactReplay
        );
    }

    #[test]
    fn sequence_and_payload_hash_are_not_package_identity_inputs() {
        let first = evidence(7, 125_875_321);
        let mut changed_sequence = first.clone();
        changed_sequence.total_sequence = 999;
        assert_eq!(
            evidence_identity(&first),
            evidence_identity(&changed_sequence)
        );
        assert_eq!(
            evidence_fingerprint(&first),
            evidence_fingerprint(&changed_sequence)
        );
        let identity = evidence_identity(&first);
        assert!(!identity.contains("999"));
        assert!(!identity.contains(&evidence_fingerprint(&first)));
    }

    #[test]
    fn dropped_nanos_and_omitted_ledger_fail_checkpoint_validation() {
        let event = evidence(7, 125_875_321);
        let mut dropped = checkpoint_for(&event);
        dropped.payload.last_broker_truth_received_at = Some(received(125_000_000));
        dropped.payload_sha256 = dropped.payload.payload_fingerprint();
        assert_eq!(
            validate_stage5g_timer_checkpoint(&dropped),
            Err(Stage5gTimerCheckpointError::PackageDiscriminatorMismatch)
        );

        let mut omitted = checkpoint_for(&event);
        omitted.payload.evidence_replay_ledger.clear();
        omitted.payload_sha256 = omitted.payload.payload_fingerprint();
        assert_eq!(
            validate_stage5g_timer_checkpoint(&omitted),
            Err(Stage5gTimerCheckpointError::MissingReplayLedger)
        );
    }

    #[test]
    fn semantically_incomplete_checkpoints_fail_even_with_recomputed_hash() {
        let event = evidence(7, 125_875_321);

        let mut missing_discriminator = checkpoint_for(&event);
        missing_discriminator.payload.package_discriminator = None;
        rehash(&mut missing_discriminator);
        assert_eq!(
            validate_stage5g_timer_checkpoint(&missing_discriminator),
            Err(Stage5gTimerCheckpointError::MissingPackageDiscriminator)
        );

        let mut missing_receipt = checkpoint_for(&event);
        missing_receipt.payload.last_broker_truth_received_at = None;
        rehash(&mut missing_receipt);
        assert_eq!(
            validate_stage5g_timer_checkpoint(&missing_receipt),
            Err(Stage5gTimerCheckpointError::MissingExactBrokerTruthReceipt)
        );

        let mut missing_ms = checkpoint_for(&event);
        missing_ms.payload.last_broker_truth_received_ms = None;
        rehash(&mut missing_ms);
        assert_eq!(
            validate_stage5g_timer_checkpoint(&missing_ms),
            Err(Stage5gTimerCheckpointError::MissingMillisecondWatermark)
        );

        let mut missing_sequence = checkpoint_for(&event);
        missing_sequence.payload.last_total_sequence = None;
        rehash(&mut missing_sequence);
        assert_eq!(
            validate_stage5g_timer_checkpoint(&missing_sequence),
            Err(Stage5gTimerCheckpointError::MissingTotalSequence)
        );

        let mut missing_continuation = checkpoint_for(&event);
        missing_continuation
            .payload
            .last_continuation_checkpoint_ts_utc_ms = None;
        rehash(&mut missing_continuation);
        assert_eq!(
            validate_stage5g_timer_checkpoint(&missing_continuation),
            Err(Stage5gTimerCheckpointError::MissingContinuationCheckpoint)
        );

        let mut missing_current_identity = checkpoint_for(&event);
        missing_current_identity.payload.current_evidence_identity = None;
        rehash(&mut missing_current_identity);
        assert_eq!(
            validate_stage5g_timer_checkpoint(&missing_current_identity),
            Err(Stage5gTimerCheckpointError::MissingCurrentEvidenceIdentity)
        );

        let mut suffix_only = checkpoint_for(&event);
        let discriminator = suffix_only.payload.package_discriminator.clone().unwrap();
        let forged = format!("arbitrary-suffix-only:{discriminator}");
        suffix_only.payload.current_evidence_identity = Some(forged);
        rehash(&mut suffix_only);
        assert_eq!(
            validate_stage5g_timer_checkpoint(&suffix_only),
            Err(Stage5gTimerCheckpointError::InvalidCurrentEvidenceIdentity)
        );
    }

    #[test]
    fn replay_ledger_and_continuation_semantics_are_fail_closed() {
        let event = evidence(7, 125_875_321);

        let mut below_broker_truth = checkpoint_for(&event);
        below_broker_truth
            .payload
            .last_continuation_checkpoint_ts_utc_ms =
            Some(event.broker_truth.received_ts.timestamp_millis() - 1);
        rehash(&mut below_broker_truth);
        assert_eq!(
            validate_stage5g_timer_checkpoint(&below_broker_truth),
            Err(Stage5gTimerCheckpointError::ContinuationBeforeBrokerTruth)
        );

        let mut duplicate = checkpoint_for(&event);
        duplicate
            .payload
            .evidence_replay_ledger
            .push(duplicate.payload.evidence_replay_ledger[0].clone());
        rehash(&mut duplicate);
        assert_eq!(
            validate_stage5g_timer_checkpoint(&duplicate),
            Err(Stage5gTimerCheckpointError::DuplicateReplayIdentity)
        );

        let mut invalid_fingerprint = checkpoint_for(&event);
        invalid_fingerprint.payload.evidence_replay_ledger[0].fingerprint_sha256 =
            "not-a-sha256".to_string();
        rehash(&mut invalid_fingerprint);
        assert_eq!(
            validate_stage5g_timer_checkpoint(&invalid_fingerprint),
            Err(Stage5gTimerCheckpointError::InvalidReplayLedgerEntry)
        );

        let mut missing_current_package = checkpoint_for(&event);
        missing_current_package.payload.evidence_replay_ledger[0].identity = format!(
            "moex.stage5g.order-position-evidence-identity.v3:{}:ACC_TEST_0001:{}",
            uuid::Uuid::from_u128(0x005d_9999),
            missing_current_package
                .payload
                .package_discriminator
                .as_deref()
                .unwrap()
        );
        rehash(&mut missing_current_package);
        assert_eq!(
            validate_stage5g_timer_checkpoint(&missing_current_package),
            Err(Stage5gTimerCheckpointError::CurrentPackageMissingFromReplayLedger)
        );

        let mut incoherent_duplicates = checkpoint_for(&event);
        incoherent_duplicates.payload.duplicate_evidence_count = 8;
        incoherent_duplicates.payload.last_total_sequence = Some(7);
        rehash(&mut incoherent_duplicates);
        assert_eq!(
            validate_stage5g_timer_checkpoint(&incoherent_duplicates),
            Err(Stage5gTimerCheckpointError::DuplicateCounterIncoherent)
        );
    }

    #[test]
    fn multi_package_restore_requires_ordered_ledger_and_latest_current_projection() {
        let (valid, first, second) = two_package_checkpoint();
        validate_stage5g_timer_checkpoint(&valid).unwrap();
        let encoded = serde_json::to_vec(&valid).unwrap();
        let restored: Stage5gTimerCheckpointEnvelope = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(restored, valid);

        let mut stale_current = valid.clone();
        stale_current.payload.current_evidence_identity = Some(evidence_identity(&first));
        stale_current.payload.last_broker_truth_received_at = Some(first.broker_truth.received_ts);
        stale_current.payload.last_broker_truth_received_ms =
            Some(first.broker_truth.received_ts.timestamp_millis());
        stale_current.payload.package_discriminator = Some(format!(
            "moex.broker-truth.package.v1:{}:{:09}",
            first.broker_truth.received_ts.timestamp(),
            first.broker_truth.received_ts.timestamp_subsec_nanos()
        ));
        rehash(&mut stale_current);
        assert_eq!(
            validate_stage5g_timer_checkpoint(&stale_current),
            Err(Stage5gTimerCheckpointError::CurrentEvidenceIdentityNotLatest)
        );

        let mut regressed_receipt = valid.clone();
        regressed_receipt.payload.last_broker_truth_received_at =
            Some(first.broker_truth.received_ts);
        regressed_receipt.payload.last_broker_truth_received_ms =
            Some(first.broker_truth.received_ts.timestamp_millis());
        regressed_receipt.payload.package_discriminator = Some(format!(
            "moex.broker-truth.package.v1:{}:{:09}",
            first.broker_truth.received_ts.timestamp(),
            first.broker_truth.received_ts.timestamp_subsec_nanos()
        ));
        rehash(&mut regressed_receipt);
        assert_eq!(
            validate_stage5g_timer_checkpoint(&regressed_receipt),
            Err(Stage5gTimerCheckpointError::CurrentPackageReceiptMismatch)
        );

        let mut reversed_ledger = valid.clone();
        reversed_ledger.payload.evidence_replay_ledger.swap(0, 1);
        rehash(&mut reversed_ledger);
        assert_eq!(
            validate_stage5g_timer_checkpoint(&reversed_ledger),
            Err(Stage5gTimerCheckpointError::ReplayLedgerReceiptRegression)
        );

        let same_receipt_first = evidence(7, 400_000_000);
        let same_receipt_initial = checkpoint_for(&same_receipt_first);
        let mut same_receipt_second = evidence(8, 400_000_000);
        same_receipt_second.request_id =
            StrategyRequestId::from(uuid::Uuid::from_u128(0x005d_0002));
        let mut nonfinal_current =
            classify_stage5g_post_checkpoint_evidence(&same_receipt_initial, same_receipt_second)
                .unwrap()
                .checkpoint();
        nonfinal_current.payload.current_evidence_identity =
            Some(evidence_identity(&same_receipt_first));
        rehash(&mut nonfinal_current);
        assert_eq!(
            validate_stage5g_timer_checkpoint(&nonfinal_current),
            Err(Stage5gTimerCheckpointError::CurrentEvidenceIdentityNotLatest)
        );

        let mut later_ledger_receipt = valid.clone();
        let third = evidence(9, 300_000_000);
        later_ledger_receipt
            .payload
            .evidence_replay_ledger
            .push(Stage5gReplayLedgerEntry {
                identity: evidence_identity(&third),
                fingerprint_sha256: evidence_fingerprint(&third),
            });
        later_ledger_receipt.payload.last_total_sequence = Some(9);
        rehash(&mut later_ledger_receipt);
        assert_eq!(
            validate_stage5g_timer_checkpoint(&later_ledger_receipt),
            Err(Stage5gTimerCheckpointError::CurrentEvidenceIdentityNotLatest)
        );

        let second_identity = evidence_identity(&second);
        assert_eq!(
            valid.payload.current_evidence_identity.as_deref(),
            Some(second_identity.as_str())
        );
    }

    #[test]
    fn timer_checkpoint_fingerprint_is_deterministic() {
        let event = evidence(7, 125_875_321);
        let left = checkpoint_for(&event);
        let right = checkpoint_for(&event);
        assert_eq!(left, right);
        assert_eq!(left.payload_sha256.len(), 64);
    }
}
