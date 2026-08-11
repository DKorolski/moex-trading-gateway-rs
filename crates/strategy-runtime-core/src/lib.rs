//! Broker-neutral strategy semantic kernels migrated from the accepted ALOR
//! runtime source.
//!
//! This crate contains no FINAM transport, Redis client, command consumer, or
//! real order endpoint.
//!
//! The source-compatible ALOR host seam is deliberately private. Downstream
//! hosts must use [`BrokerNeutralHybridStrategy`].
//!
//! ```compile_fail
//! use strategy_runtime_core::StrategyCtx;
//! ```
//!
//! ```compile_fail
//! use strategy_runtime_core::strategy_host::Strategy;
//! ```
//!
//! ```compile_fail
//! use strategy_runtime_core::state::StrategyState;
//! ```
//!
//! ```compile_fail
//! use strategy_runtime_core::{Stage5cSettledPaperStrategy, Stage5cTimerSettlement};
//!
//! let settled: Stage5cSettledPaperStrategy = unreachable!();
//! let _forged = Stage5cTimerSettlement::ReadyForContinuation {
//!     settled,
//!     checkpoint_ts_utc_ms: 0,
//! };
//! ```
//!
//! ```compile_fail
//! use strategy_runtime_core::{Stage5cSettledPaperStrategy, Stage5cTimerSettlement};
//!
//! let settled: Stage5cSettledPaperStrategy = unreachable!();
//! let _forged = Stage5cTimerSettlement::GeneratedIntentBatch(settled);
//! ```
//!
//! ```compile_fail
//! use strategy_runtime_core::{
//!     advance_stage5c_controlled_next_bar, Stage5cAcceptedSemanticBar, Stage5cTimerSettlement,
//! };
//!
//! let settlement: Stage5cTimerSettlement = unreachable!();
//! let accepted: Stage5cAcceptedSemanticBar = unreachable!();
//! let settled = settlement.into_settled();
//! let _ = advance_stage5c_controlled_next_bar(settled, accepted);
//! ```
//!
//! Stage 5G-b ACK feedback cannot be attached before ownership of the opaque
//! settled-intent capability has been established:
//!
//! ```compile_fail
//! use strategy_runtime_core::{
//!     attach_stage5g_mock_ack_session, Stage5cPaperIntentBatchSummary,
//!     Stage5gMockAckSessionInput,
//! };
//!
//! let summary: Stage5cPaperIntentBatchSummary = unreachable!();
//! let input: Stage5gMockAckSessionInput = unreachable!();
//! let _ = attach_stage5g_mock_ack_session(summary, input);
//! ```
//!
//! Stage 5G-d continuation capabilities are linear and cannot be forged or
//! cloned by a downstream host:
//!
//! ```compile_fail
//! use strategy_runtime_core::Stage5gTimerSession;
//! let _forged = Stage5gTimerSession {};
//! ```
//!
//! ```compile_fail
//! use strategy_runtime_core::Stage5gTimerReadyPaperStrategy;
//! let ready: Stage5gTimerReadyPaperStrategy = unreachable!();
//! let _copy = ready.clone();
//! ```
//!
//! The linear Stage 5G-b session itself cannot be forged:
//!
//! ```compile_fail
//! use strategy_runtime_core::Stage5gMockAckSession;
//! let _forged = Stage5gMockAckSession {};
//! ```
//!
//! A Stage 5G-e-c source is consumed by export and cannot survive the alleged
//! clean-process boundary:
//!
//! ```compile_fail,E0382
//! use strategy_runtime_core::{
//!     export_stage5g_clean_restart, Stage5gCleanRestartExportInput,
//!     Stage5gCleanRestartSource, Stage5gLifecycleCommitmentKey,
//! };
//! fn moved_source_cannot_be_reused(
//!     source: Stage5gCleanRestartSource,
//!     input: Stage5gCleanRestartExportInput,
//!     key: &Stage5gLifecycleCommitmentKey,
//! ) {
//!     let _bytes = export_stage5g_clean_restart(source, input, key);
//!     drop(source);
//! }
//! ```
//!
//! The operator-managed lifecycle commitment key is opaque, non-cloneable and
//! cannot be serialized into the restart package:
//!
//! ```compile_fail,E0599
//! use strategy_runtime_core::Stage5gLifecycleCommitmentKey;
//! let key = Stage5gLifecycleCommitmentKey::from_secret_bytes(&[7_u8; 32]).unwrap();
//! let _copy = key.clone();
//! ```
//!
//! ```compile_fail,E0277
//! use strategy_runtime_core::Stage5gLifecycleCommitmentKey;
//! let key = Stage5gLifecycleCommitmentKey::from_secret_bytes(&[7_u8; 32]).unwrap();
//! let _serialized = serde_json::to_string(&key).unwrap();
//! ```
//!
//! ```compile_fail,E0277
//! use strategy_runtime_core::Stage5gLifecycleCommitmentKey;
//! let key = Stage5gLifecycleCommitmentKey::from_secret_bytes(&[7_u8; 32]).unwrap();
//! println!("{key:?}");
//! ```
//!
//! The fresh reconstruction capability remains linear and cannot be cloned:
//!
//! ```compile_fail,E0599
//! use strategy_runtime_core::Stage5gCleanRestartedCapability;
//! let restored: Stage5gCleanRestartedCapability = unreachable!();
//! let _copy = restored.clone();
//! ```
// STAGE5D-ADDITIVE-BRIDGE-BEGIN: lib-stage5e-b3f-doctest-docs
//!
//! The following B3F witnesses use a doctest-only facade whose wrappers contain
//! the actual production escrow, seals, preflight borrow, and payload.
//!
//! ```compile_fail,E0599
//! // b3f_compile_fail_consume_seal_clone_or_copy
//! use strategy_runtime_core::stage5e_b3f_compile_fail_facade::consume_seal;
//! let seal = consume_seal();
//! let _clone = seal.clone();
//! let _copy = seal;
//! let _reuse = seal;
//! ```
//!
//! ```compile_fail,E0423
//! // b3f_compile_fail_consume_seal_reconstruction
//! use strategy_runtime_core::stage5e_b3f_compile_fail_facade::ConsumeSeal;
//! let _forged = ConsumeSeal(());
//! ```
//!
//! ```compile_fail,E0599
//! // b3f_compile_fail_capability_escape
//! use strategy_runtime_core::stage5e_b3f_compile_fail_facade::{consume_seal, escrow};
//! let payload = escrow().consume(&consume_seal());
//! let _escaped = payload.consume_seal();
//! ```
//!
//! ```compile_fail,E0382
//! // b3f_compile_fail_second_escrow_consume
//! use strategy_runtime_core::stage5e_b3f_compile_fail_facade::{consume_seal, escrow};
//! let escrow = escrow();
//! let seal = consume_seal();
//! let _first = escrow.consume(&seal);
//! let _second = escrow.consume(&seal);
//! ```
//!
//! ```compile_fail,E0505
//! // b3f_compile_fail_borrow_survives_consume
//! use strategy_runtime_core::stage5e_b3f_compile_fail_facade::{
//!     consume_seal, escrow, preflight_seal,
//! };
//! let escrow = escrow();
//! let preflight_seal = preflight_seal();
//! let consume_seal = consume_seal();
//! let borrowed = escrow.preflight(&preflight_seal);
//! let _payload = escrow.consume(&consume_seal);
//! drop(borrowed);
//! ```
// STAGE5D-ADDITIVE-BRIDGE-END: lib-stage5e-b3f-doctest-docs
// STAGE5G-EDC-COMPILE-FAIL-BEGIN
//!
//! Stage 5G-e-d-c consumes one opaque reduction. These doctest-only facade
//! witnesses mirror the production ownership shape without exposing the
//! production candidate or application function.
//!
//! ```compile_fail,E0599
//! // stage5g_edc_compile_fail_reduction_clone
//! use strategy_runtime_core::stage5g_edc_compile_fail_facade::reduction;
//! let reduction = reduction();
//! let _clone = reduction.clone();
//! ```
//!
//! ```compile_fail,E0599
//! // stage5g_edc_compile_fail_candidate_extraction
//! use strategy_runtime_core::stage5g_edc_compile_fail_facade::reduction;
//! let _candidate = reduction().candidate();
//! ```
//!
//! ```compile_fail,E0382
//! // stage5g_edc_compile_fail_apply_twice
//! use strategy_runtime_core::stage5g_edc_compile_fail_facade::{apply, reduction};
//! let reduction = reduction();
//! let _first = apply(reduction);
//! let _second = apply(reduction);
//! ```
//!
//! ```compile_fail,E0382
//! // stage5g_edc_compile_fail_reduction_reuse
//! use strategy_runtime_core::stage5g_edc_compile_fail_facade::{apply, reduction};
//! let reduction = reduction();
//! let _result = apply(reduction);
//! drop(reduction);
//! ```
//!
//! ```compile_fail,E0599
//! // stage5g_edc_compile_fail_blocked_to_candidate
//! use strategy_runtime_core::stage5g_edc_compile_fail_facade::blocked;
//! let _candidate = blocked().into_candidate();
//! ```
//!
//! ```compile_fail,E0599
//! // stage5g_edc_compile_fail_continued_to_candidate
//! use strategy_runtime_core::stage5g_edc_compile_fail_facade::continued;
//! let _candidate = continued().into_candidate();
//! ```
//!
//! ```compile_fail,E0599
//! // stage5g_edc_compile_fail_applied_exposes_candidate
//! use strategy_runtime_core::stage5g_edc_compile_fail_facade::applied;
//! let _candidate = applied().candidate();
//! ```
//!
//! ```compile_fail,E0599
//! // stage5g_edc_compile_fail_diagnostic_reconstruction
//! use strategy_runtime_core::stage5g_edc_compile_fail_facade::diagnostic;
//! let _candidate = diagnostic().into_candidate();
//! ```
//!
//! ```compile_fail,E0061
//! // stage5g_edc_compile_fail_raw_rows_application
//! use strategy_runtime_core::stage5g_edc_compile_fail_facade::apply;
//! let raw_broker_rows: Vec<String> = Vec::new();
//! let _result = apply(raw_broker_rows);
//! ```
//!
//! ```compile_fail,E0277
//! // stage5g_edc_compile_fail_reduction_serialization
//! use strategy_runtime_core::stage5g_edc_compile_fail_facade::reduction;
//! let _json = serde_json::to_string(&reduction()).unwrap();
//! ```
//!
//! ```compile_fail,E0382
//! // stage5g_edc_r1_compile_fail_candidate_applied_twice
//! use strategy_runtime_core::stage5g_edc_compile_fail_facade::{apply_candidate, candidate};
//! let candidate = candidate();
//! apply_candidate(candidate);
//! apply_candidate(candidate);
//! ```
//!
//! ```compile_fail,E0382
//! // stage5g_edc_r1_compile_fail_post_token_reused_after_export
//! use strategy_runtime_core::stage5g_edc_compile_fail_facade::{export_post, post_token};
//! let token = post_token();
//! export_post(token);
//! export_post(token);
//! ```
// STAGE5G-EDC-COMPILE-FAIL-END

pub mod hybrid_intraday;
// The accepted source wrapper intentionally retains Stage 5C/5D callbacks
// which are sealed from downstream code until their dedicated gates open.
#[allow(dead_code)]
mod hybrid_intraday_runtime;
// Source-compatible DTOs and traits remain complete for oracle correspondence,
// while only approved broker-neutral aliases are exported below.
#[allow(dead_code)]
mod runtime_compat;
mod stage5c_paper_host;
// STAGE5D-ADDITIVE-BRIDGE-BEGIN: lib-stage5d-module
mod stage5d_persistence;
#[allow(dead_code)] // Stage 5E-b1 is deliberately private until a later reviewed consumer.
mod stage5e_no_io_lifecycle;
// STAGE5D-ADDITIVE-BRIDGE-END: lib-stage5d-module
// STAGE5F-TEST-OBSERVATION-MODULE-BEGIN
#[cfg(test)]
mod stage5f_atomic_hybrid_semantics;
// STAGE5F-TEST-OBSERVATION-MODULE-END
mod stage5g_clean_restart;
#[allow(dead_code)] // Stage 5G-e-d-a contract is consumed by the reviewed e-d-b reducer.
mod stage5g_fresh_broker_truth;
#[cfg(any(test, feature = "stage5g-artifact-fixtures"))]
mod stage5g_lifecycle_freeze;
mod stage5g_mock_ack;
mod stage5g_order_position;
mod stage5g_protective_completion;
mod stage5g_timer;
mod stage6_durable_identity;
mod stage6_journal_backend;
mod stage6_replay;
mod stage6d_live_core;

pub use hybrid_intraday_runtime::{
    BrokerNeutralHybridCallbackResult, BrokerNeutralHybridStrategy, HybridIntradayProfile,
    HybridIntradayRuntimeConfig, HybridIntradayRuntimeStrategy,
    HybridRuntimeCallbackValidationError, MeanReversionVariant, MrGatePolicy, RiskGateMode,
};
#[allow(unused_imports)]
pub(crate) use runtime_compat::{
    BootstrapSnapshot, PaperExecutionMode, RuntimeStateRestored, StrategyCtx, TradeMode,
};
pub use runtime_compat::{
    Intent as BrokerNeutralHybridIntent, IntentClass as BrokerNeutralHybridIntentClass,
    MarketBuyAndCloseLiveOrderStyle as BrokerNeutralMarketOrderStyle,
    OrderSide as BrokerNeutralOrderSide, StopLimitCondition as BrokerNeutralStopLimitCondition,
};
pub use stage6_durable_identity::{
    Stage6CancelOutcomeV1, Stage6ConflictKindV1, Stage6DurableActionKind,
    Stage6DurableCommandSnapshotV1, Stage6DurableIdentityError, Stage6DurableRequestIdentityV1,
    Stage6JournalEventKind, Stage6JournalRecordId, Stage6JournalRecordV1, Stage6LifecycleSequence,
    Stage6ReconciliationDispositionV1, Stage6RequestFinalDispositionV1, Stage6Sha256Digest,
    STAGE6_DURABLE_RECORD_SCHEMA_VERSION,
};
pub use stage6_journal_backend::{
    Stage6FileJournalBackend, Stage6JournalAppendReceipt, Stage6JournalBackend,
    Stage6JournalCheckpointV1, Stage6JournalFrontierV1, Stage6JournalStorageError,
    Stage6MemoryJournalBackend, STAGE6_JOURNAL_MAX_RECORD_BYTES,
    STAGE6_JOURNAL_STORAGE_SCHEMA_VERSION,
};
pub use stage6_replay::{
    Stage6DispatchSafetyStateV1, Stage6RecoveredRequestV1, Stage6ReplayEngineV1, Stage6ReplayError,
    Stage6ReplaySnapshotV1, STAGE6_REPLAY_SCHEMA_VERSION,
};
pub use stage6d_live_core::{
    apply_stage6d_restart_fresh_truth, authorize_stage6d_first_boot, execute_stage6d_paper_outcome,
    first_boot_stage6d_paper, prepare_stage6d_paper_dispatch, restart_stage6d_paper,
    seal_stage6d_restart_package, Stage6dBootMode, Stage6dDurableRuntimeRecovered,
    Stage6dFirstBootAuthorization, Stage6dFirstBootConfig, Stage6dFreshBrokerTruthInput,
    Stage6dFreshTruthApplicationReport, Stage6dFreshTruthTransition, Stage6dLiveCoreError,
    Stage6dOperationalIdentityConfig, Stage6dPaperDispatchReceipt, Stage6dPaperExecutionReport,
    Stage6dPaperOutcome, STAGE6D_AUTHENTICATED_RESTART_SCHEMA_VERSION,
    STAGE6D_INTEGRATION_FINGERPRINT_SCHEMA_VERSION,
};
// STAGE5D-ADDITIVE-BRIDGE-BEGIN: lib-stage5e-b3f-doctest-facade
#[cfg(doctest)]
#[doc(hidden)]
pub mod stage5e_b3f_compile_fail_facade {
    use crate::stage5e_no_io_lifecycle::callback_authority;
    use std::marker::PhantomData;

    type ProductionEscrow = callback_authority::Stage5ePaperCallbackResultEscrow;
    type ProductionPreflightSeal =
        callback_authority::callback_settlement::Stage5ePaperSettlementPreflightSeal;
    type ProductionConsumeSeal =
        callback_authority::callback_settlement::Stage5ePaperSettlementConsumeSeal;

    pub struct Escrow(pub(crate) ProductionEscrow);
    pub struct PreflightSeal(pub(crate) ProductionPreflightSeal);
    pub struct ConsumeSeal(pub(crate) ProductionConsumeSeal);
    pub struct Preflight<'a>(PhantomData<&'a Escrow>);
    pub struct Payload(());

    pub fn escrow() -> Escrow {
        unreachable!("compile-fail facade is type-checked but never executed")
    }

    pub fn preflight_seal() -> PreflightSeal {
        unreachable!("compile-fail facade is type-checked but never executed")
    }

    pub fn consume_seal() -> ConsumeSeal {
        unreachable!("compile-fail facade is type-checked but never executed")
    }

    impl Escrow {
        pub fn preflight<'a>(&'a self, seal: &'a PreflightSeal) -> Preflight<'a> {
            crate::stage5e_no_io_lifecycle::callback_authority::callback_settlement::
                b3f_doctest_borrow_preflight(&self.0, &seal.0);
            Preflight(PhantomData)
        }

        pub fn consume(self, seal: &ConsumeSeal) -> Payload {
            crate::stage5e_no_io_lifecycle::callback_authority::callback_settlement::
                b3f_doctest_consume_escrow(self.0, &seal.0);
            Payload(())
        }
    }
}

#[cfg(doctest)]
#[doc(hidden)]
pub mod stage5g_edc_compile_fail_facade {
    pub struct Reduction(Option<crate::stage5g_fresh_broker_truth::Stage5gFreshTruthReduction>);
    pub struct Candidate(
        Option<crate::stage5g_fresh_broker_truth::Stage5gOwnedReconciliationCandidate>,
    );
    pub struct PostToken(
        Option<crate::stage5g_fresh_broker_truth::Stage5gValidatedPostApplication>,
    );
    pub struct SourceProof(
        Option<crate::stage5g_fresh_broker_truth::Stage5gFreshTruthApplicationSourceProof>,
    );
    pub struct FinalizedPostToken(
        Option<crate::stage5g_fresh_broker_truth::Stage5gFinalizedPostApplication>,
    );
    pub struct Applied(Option<crate::stage5g_fresh_broker_truth::Stage5gFreshTruthApplied>);
    pub struct Continued(Option<crate::stage5g_fresh_broker_truth::Stage5gFreshTruthContinued>);
    pub struct Blocked(
        Option<crate::stage5g_fresh_broker_truth::Stage5gFreshTruthApplicationBlocked>,
    );
    pub struct Diagnostic(pub(crate) ());
    pub enum Outcome {
        Applied(Applied),
        Continued(Continued),
        Blocked(Blocked),
    }

    pub fn reduction() -> Reduction {
        unreachable!("compile-fail facade is type-checked but never executed")
    }
    pub fn apply(reduction: Reduction) -> Outcome {
        drop(reduction.0);
        unreachable!("compile-fail facade is type-checked but never executed")
    }
    pub fn candidate() -> Candidate {
        unreachable!("compile-fail facade is type-checked but never executed")
    }
    pub fn apply_candidate(candidate: Candidate) {
        drop(candidate.0);
    }
    pub fn post_token() -> PostToken {
        unreachable!("compile-fail facade is type-checked but never executed")
    }
    pub fn export_post(token: PostToken) {
        drop(token.0);
    }
    pub fn applied() -> Applied {
        unreachable!("compile-fail facade is type-checked but never executed")
    }
    pub fn continued() -> Continued {
        unreachable!("compile-fail facade is type-checked but never executed")
    }
    pub fn blocked() -> Blocked {
        unreachable!("compile-fail facade is type-checked but never executed")
    }
    pub fn diagnostic() -> Diagnostic {
        unreachable!("compile-fail facade is type-checked but never executed")
    }
}
// STAGE5D-ADDITIVE-BRIDGE-END: lib-stage5e-b3f-doctest-facade
pub use stage5c_paper_host::{
    accept_stage5c_history_batch, accept_stage5c_pending_recovery_evidence,
    accept_stage5c_semantic_bar, admit_stage5c_paper_host, advance_stage5c_controlled_next_bar,
    advance_stage5c_paper_loop_once, advance_stage5c_timer_settlement_next_bar,
    advance_stage5c_timer_settlement_timer, apply_stage5c_semantic_bar, notify_stage5c_bootstrap,
    notify_stage5c_runtime_state_restored, prepare_stage5c_without_runtime_state,
    prove_stage5c_pending_recovery_claim, recover_stage5c_pending_streams,
    resolve_stage5c_paper_broker_lifecycle, resolve_stage5c_paper_intent_lifecycle,
    resolve_stage5c_paper_timer, restore_stage5c_runtime_state,
    settle_stage5c_broker_lifecycle_result, settle_stage5c_semantic_result,
    settle_stage5c_timer_result, warmup_stage5c_history, Stage5cAcceptedHistoryBatch,
    Stage5cAcceptedPendingRecoveryEvidence, Stage5cAcceptedSemanticBar,
    Stage5cBootstrapNotificationError, Stage5cBootstrapNotificationReceipt,
    Stage5cBootstrappedPaperStrategy, Stage5cBrokerLifecycleResolvedPaperStrategy,
    Stage5cBrokerLifecycleSettlement, Stage5cHistoryBatchInput, Stage5cHistoryWarmupError,
    Stage5cHistoryWarmupReceipt, Stage5cIntentSettlementError, Stage5cLegacyNumericOrderIdPolicy,
    Stage5cNextBarBlocked, Stage5cNextBarLoopError, Stage5cNextBarLoopFailure,
    Stage5cPaperAckOutcome, Stage5cPaperAckRecord, Stage5cPaperBrokerEventKind,
    Stage5cPaperBrokerEventPayload, Stage5cPaperBrokerEventRecord,
    Stage5cPaperBrokerLifecycleBlocked, Stage5cPaperBrokerLifecycleError,
    Stage5cPaperBrokerLifecycleExpectation, Stage5cPaperBrokerLifecycleFailure,
    Stage5cPaperBrokerLifecycleInput, Stage5cPaperHostAdmission, Stage5cPaperHostAdmissionError,
    Stage5cPaperHostAdmissionInput, Stage5cPaperIntentBatch, Stage5cPaperIntentBatchSummary,
    Stage5cPaperIntentLifecycleBlocked, Stage5cPaperIntentLifecycleError,
    Stage5cPaperIntentLifecycleFailure, Stage5cPaperIntentLifecycleInput, Stage5cPaperLoopError,
    Stage5cPaperLoopEvent, Stage5cPaperLoopEventKind, Stage5cPaperLoopFailure,
    Stage5cPaperLoopState, Stage5cPaperLoopStateKind, Stage5cPaperTimerBlocked,
    Stage5cPaperTimerError, Stage5cPaperTimerFailure, Stage5cPaperTimerInput,
    Stage5cPendingRecoveredPaperStrategy, Stage5cPendingRecoveryClaimProof,
    Stage5cPendingRecoveryClaimProofInput, Stage5cPendingRecoveryError,
    Stage5cPendingRecoveryEvent, Stage5cPendingRecoveryEvidenceInput,
    Stage5cPendingRecoveryPayload, Stage5cPendingRecoveryReceipt,
    Stage5cPendingStreamClaimBoundary, Stage5cPendingStreamKind,
    Stage5cResolvedPaperIntentBatchStrategy, Stage5cRuntimeStateLoadedPaperStrategy,
    Stage5cRuntimeStateRestoreError, Stage5cRuntimeStateRestoreInput,
    Stage5cRuntimeStateRestoreReceipt, Stage5cRuntimeStateRestoredPaperStrategy,
    Stage5cSemanticBarError, Stage5cSemanticBarInput, Stage5cSemanticBarResult,
    Stage5cSettledPaperStrategy, Stage5cTimerContinuationBlocked, Stage5cTimerContinuationError,
    Stage5cTimerContinuationFailure, Stage5cTimerResolvedPaperStrategy, Stage5cTimerSettlement,
    Stage5cWarmedPaperStrategy, STAGE5C_PAPER_HOST_ADMISSION_SCHEMA_VERSION,
    STAGE5C_RUNTIME_STATE_RESTORE_SCHEMA_VERSION,
};
pub use stage5g_clean_restart::{
    export_stage5g_clean_restart, restore_stage5g_clean_restart, Stage5gCleanRestartError,
    Stage5gCleanRestartExportInput, Stage5gCleanRestartLifecycleKind, Stage5gCleanRestartSource,
    Stage5gCleanRestartedCapability, Stage5gLifecycleCommitmentKey,
    Stage5gLifecycleCommitmentKeyError, STAGE5G_CLEAN_RESTART_EXTENSION_SCHEMA_VERSION,
};
#[cfg(any(test, feature = "stage5g-artifact-fixtures"))]
pub use stage5g_lifecycle_freeze::{
    stage5g_g_lifecycle_artifact_json_pretty, stage5g_g_lifecycle_artifact_rows,
    stage5g_g_lifecycle_artifact_rows_parallel_verified,
    stage5g_h_sequential_lifecycle_artifact_json_pretty, Stage5gLifecycleArtifactRow,
};
pub use stage5g_mock_ack::{
    apply_stage5g_duplicate_after_resolution, apply_stage5g_mock_ack,
    attach_stage5g_mock_ack_session, Stage5gMockAckAdmissionBlocked, Stage5gMockAckAdmissionError,
    Stage5gMockAckBlocked, Stage5gMockAckError, Stage5gMockAckEvent, Stage5gMockAckFailure,
    Stage5gMockAckSession, Stage5gMockAckSessionInput, Stage5gMockAckSessionSummary,
    Stage5gMockAckSlotState, Stage5gMockAckSlotSummary, Stage5gMockAckTerminal,
    Stage5gMockAckTransition, Stage5gMockIntentAction, Stage5gMockIntentBinding,
    Stage5gMockPlaceKind, Stage5gResolvedMockAckPaperStrategy, Stage5gResolvedMockAckReplayBlocked,
    STAGE5G_MOCK_ACK_SCHEMA_VERSION,
};
pub use stage5g_order_position::{
    apply_stage5g_order_position_evidence, attach_stage5g_order_position_session,
    Stage5gConvergedPaperStrategy, Stage5gOrderPositionAdmissionBlocked,
    Stage5gOrderPositionAdmissionError, Stage5gOrderPositionBlocked, Stage5gOrderPositionError,
    Stage5gOrderPositionEvidence, Stage5gOrderPositionFailure, Stage5gOrderPositionSession,
    Stage5gOrderPositionSummary, Stage5gOrderPositionTerminal, Stage5gOrderPositionTransition,
    STAGE5G_ORDER_POSITION_SCHEMA_VERSION,
};
pub use stage5g_protective_completion::{
    accept_stage5g_protective_cleanup_truth, apply_stage5g_protective_cleanup_completion,
    apply_stage5g_protective_completion, issue_stage5g_canonical_protective_evidence,
    prepare_stage5g_protective_completion, restore_stage5g_protective_completion_continuation,
    stage5g_protective_restart_source_from_transition, Stage5gAcceptedProtectiveBrokerTruth,
    Stage5gAcceptedProtectiveCleanupTruth, Stage5gProtectedPositionSide,
    Stage5gProtectiveAuthoritySummary, Stage5gProtectiveBlockReason, Stage5gProtectiveBlocked,
    Stage5gProtectiveCleanupOutcome, Stage5gProtectiveCleanupRequestSettlementV1,
    Stage5gProtectiveCleanupSettlementEvidence, Stage5gProtectiveCleanupSettlementLedgerV1,
    Stage5gProtectiveCleanupSettlementState, Stage5gProtectiveCleanupTransition,
    Stage5gProtectiveCommittedState, Stage5gProtectiveCompleted,
    Stage5gProtectiveCompletionAuthority, Stage5gProtectiveCompletionTransition,
    Stage5gProtectiveDisposition, Stage5gProtectiveEvidenceReceipt,
    Stage5gProtectiveFlatCleanupPending, Stage5gProtectiveGprtArtifactRow, Stage5gProtectiveLeg,
    Stage5gProtectivePostStateSummary, Stage5gProtectiveReceiptLedgerProjection,
    Stage5gProtectiveRestartProjectionKind, Stage5gProtectiveRestartProjectionV1,
    Stage5gProtectiveRestartSource, Stage5gProtectiveRestoredCompleted,
    Stage5gProtectiveRestoredContinuation, Stage5gProtectiveRestoredFlatCleanupPending,
    Stage5gProtectiveScenarioId, Stage5gValidatedProtectiveEvidence,
    STAGE5G_PROTECTIVE_CANONICAL_EVIDENCE_SCHEMA_VERSION,
    STAGE5G_PROTECTIVE_COMPLETION_SCHEMA_VERSION,
    STAGE5G_PROTECTIVE_RESTART_PROJECTION_SCHEMA_VERSION,
};
#[cfg(any(test, feature = "stage5g-artifact-fixtures"))]
pub use stage5g_protective_completion::{
    stage5g_f_gprt_artifact_json_pretty, stage5g_f_gprt_artifact_rows,
    stage5g_f_gprt_artifact_rows_parallel_verified,
};
pub use stage5g_timer::{
    apply_stage5g_exact_replay_to_session, apply_stage5g_new_package_candidate,
    apply_stage5g_timer_checkpoint, apply_stage5g_timer_mock_ack,
    attach_stage5g_market_terminal_timer_session, attach_stage5g_timer_generated_mock_ack,
    attach_stage5g_timer_order_position_session, attach_stage5g_timer_session,
    classify_stage5g_post_checkpoint_evidence, continue_stage5g_timer_with_bar,
    continue_stage5g_timer_with_timer, settle_stage5g_bar_continuation,
    validate_stage5g_timer_checkpoint, Stage5gBarContinuationPaperStrategy,
    Stage5gBarContinuationTransition, Stage5gCheckpointReplayDisposition,
    Stage5gCheckpointReplayError, Stage5gCheckpointReplayResult,
    Stage5gCommittedAwaitingOrderPosition, Stage5gCommittedConvergedOrderPosition,
    Stage5gCommittedExactReplaySession, Stage5gCommittedMarketTerminalOrderPosition,
    Stage5gExactReplayApplyBlockReason, Stage5gExactReplayApplyBlocked,
    Stage5gExactReplayApplyFailure, Stage5gExactReplayCheckpoint,
    Stage5gExactReplayInvariantFailure, Stage5gExactReplayTerminal,
    Stage5gNewPackageApplyBlockReason, Stage5gNewPackageApplyBlocked,
    Stage5gNewPackageApplyFailure, Stage5gNewPackageApplyResult, Stage5gNewPackageCandidate,
    Stage5gNewPackageCommitError, Stage5gNewPackageCommitMismatch, Stage5gNewPackageTerminal,
    Stage5gReplayLedgerEntry, Stage5gTimerBlocked, Stage5gTimerCheckpointEnvelope,
    Stage5gTimerCheckpointError, Stage5gTimerCheckpointPayload, Stage5gTimerError,
    Stage5gTimerFailure, Stage5gTimerGeneratedIntentEscrow, Stage5gTimerMockAckAdmissionBlocked,
    Stage5gTimerMockAckBlocked, Stage5gTimerMockAckError, Stage5gTimerMockAckFailure,
    Stage5gTimerMockAckSession, Stage5gTimerMockAckTransition,
    Stage5gTimerOrderPositionAdmissionBlocked, Stage5gTimerReadyPaperStrategy,
    Stage5gTimerResolvedMockAckPaperStrategy, Stage5gTimerSession, Stage5gTimerTransition,
    STAGE5G_TIMER_CHECKPOINT_SCHEMA_VERSION,
};
// STAGE5D-ADDITIVE-BRIDGE-BEGIN: lib-stage5d-exports
pub use stage5d_persistence::{
    stage5d_apply_runtime_private_extension, stage5d_bind_runtime_state_loaded,
    stage5d_inject_authoritative_riskgate, stage5d_notify_broker_truth_bootstrap,
    stage5d_notify_runtime_state_restored, stage5d_retry_authoritative_riskgate_injection,
    stage5d_retry_bind_runtime_state_loaded, stage5d_retry_broker_truth_bootstrap,
    stage5d_validate_riskgate_ledger_evidence, Stage5dAdditiveFreezeEvidence,
    Stage5dBootstrapBlockReason, Stage5dBootstrapBlocked, Stage5dBootstrappedPaperStrategy,
    Stage5dBracketReconciliationTimer, Stage5dCleanupRetryState, Stage5dEntryStyle,
    Stage5dEnvelopeBoundRuntimeStateLoaded, Stage5dEnvelopeValidationError,
    Stage5dExpectedWorkingSets, Stage5dHybridIntradayStrategyStateV1, Stage5dInstrumentBinding,
    Stage5dLifecycleReason, Stage5dLifecycleWatermarks, Stage5dOwner, Stage5dPartialEntryTimer,
    Stage5dPendingEntryExtension, Stage5dPendingExitExtension, Stage5dPersistenceEnvelope,
    Stage5dPersistenceStage, Stage5dPrivateStateAppliedPaperStrategy, Stage5dRecoveryIndexes,
    Stage5dRestoreBlockReason, Stage5dRestoreBlocked, Stage5dRiskGateFinalizationOutboxRecord,
    Stage5dRiskGateFinalizationState, Stage5dRiskGateIdentity,
    Stage5dRiskGateInjectedPaperStrategy, Stage5dRiskGateInjectionBlockReason,
    Stage5dRiskGateInjectionBlocked, Stage5dRiskGateLedgerEvidence, Stage5dRiskGateLedgerRecord,
    Stage5dRiskGateMaterializedState, Stage5dRiskGatePersistence, Stage5dRiskGateRowSource,
    Stage5dRiskGateRowStatus, Stage5dRuntimePendingRiskGateFinalization,
    Stage5dRuntimePrivateApplyBlocked, Stage5dRuntimePrivateExtension,
    Stage5dRuntimeStateRestoreBlocked, Stage5dRuntimeStateRestoreBlockedReason,
    Stage5dRuntimeStateRestoreOutcome, Stage5dRuntimeStateRestoreRecoveryDisposition,
    Stage5dRuntimeStateRestoreTerminalFailure, Stage5dRuntimeStateRestoreTerminalReason,
    Stage5dSemanticStrategyStateV1, Stage5dSide, Stage5dSnapshotBinding, Stage5dStrategyKind,
    Stage5dStrategyStatePayload, Stage5dStructuredTimestampFormat, Stage5dTimestampPolicy,
    Stage5dTimestampUnits, Stage5dValidatedPersistenceEnvelope,
    Stage5dValidatedRiskGateLedgerEvidence, Stage5dValidatedRuntimePrivateExtension,
    STAGE5D_ADDITIVE_FREEZE_SCHEMA_VERSION, STAGE5D_PERSISTENCE_ENVELOPE_SCHEMA_VERSION,
    STAGE5D_RISKGATE_SCHEMA_VERSION, STAGE5D_RUNTIME_PRIVATE_EXTENSION_SCHEMA_VERSION,
    STAGE5D_STRATEGY_STATE_PAYLOAD_SCHEMA_VERSION,
};
// STAGE5D-ADDITIVE-BRIDGE-END: lib-stage5d-exports

pub(crate) mod live_guard {
    pub use crate::runtime_compat::GatewayPhase;
}

pub(crate) mod state {
    pub use crate::runtime_compat::StrategyState;
}

pub(crate) mod strategy_host {
    #[allow(unused_imports)]
    pub use crate::runtime_compat::OrderEvent;
    #[allow(unused_imports)]
    pub use crate::runtime_compat::{
        BarEvent, BootstrapSnapshot, CommandAck, DataOrigin, Intent, PositionEvent,
        RiskGateRuntimeState, RiskGateSessionFinalization, RuntimeStateRestored, StopOrderEvent,
        Strategy, StrategyCtx,
    };
}

pub(crate) mod strategies {
    pub mod hybrid_intraday {
        pub use crate::hybrid_intraday::*;
    }

    pub mod market_buy_and_close {
        pub use crate::runtime_compat::MarketBuyAndCloseLiveOrderStyle;
    }
}

pub(crate) fn deterministic_request_id(
    strategy_id: &str,
    portfolio: &str,
    symbol: &str,
    action: &str,
    bar_ts: i64,
    seq: u8,
) -> broker_core::StrategyRequestId {
    broker_core::deterministic_request_id_from_legacy_parts(
        strategy_id,
        portfolio,
        symbol,
        action,
        bar_ts,
        seq,
    )
}

pub(crate) fn deterministic_market_request_id(
    strategy_id: &str,
    portfolio: &str,
    symbol: &str,
    created_ts_utc: i64,
    side: runtime_compat::OrderSide,
) -> broker_core::StrategyRequestId {
    let seq = match side {
        runtime_compat::OrderSide::Buy => 3,
        runtime_compat::OrderSide::Sell => 4,
    };
    deterministic_request_id(
        strategy_id,
        portfolio,
        symbol,
        "market",
        created_ts_utc,
        seq,
    )
}
