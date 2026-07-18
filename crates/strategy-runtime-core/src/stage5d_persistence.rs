//! Stage 5D additive persistence freeze surface.
//!
//! Stage 5D-b2a adds versioned persistence DTOs and schema validation.
//! Stage 5D-b2b-a adds the controlled validated runtime-private bind/apply
//! bridge. Redis, FINAM, transport, dispatch and runtime-live remain closed.

use std::collections::{HashMap, HashSet};

use broker_core::{
    BrokerAccountId, BrokerOrderId, BrokerStopOrderId, BrokerTradeId, ClientOrderId, Exchange,
    InstrumentId, Market, StrategyRequestId,
};
#[cfg(test)]
use chrono::NaiveTime;
use chrono::{DateTime, Datelike, FixedOffset, NaiveDate, NaiveDateTime, Utc, Weekday};
use rust_decimal::prelude::ToPrimitive;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::runtime_compat::{RiskGateRuntimeState, Strategy, StrategyState};

/// Stage 5D additive freeze manifest schema version.
pub const STAGE5D_ADDITIVE_FREEZE_SCHEMA_VERSION: u16 = 1;
/// Stage 5D persistence envelope schema version.
pub const STAGE5D_PERSISTENCE_ENVELOPE_SCHEMA_VERSION: u16 = 1;
/// Stage 5D runtime-private extension schema version.
pub const STAGE5D_RUNTIME_PRIVATE_EXTENSION_SCHEMA_VERSION: u16 = 1;
/// Stage 5D riskgate persistence schema version.
pub const STAGE5D_RISKGATE_SCHEMA_VERSION: u16 = 1;
/// Stage 5D semantic strategy-state payload schema version.
pub const STAGE5D_STRATEGY_STATE_PAYLOAD_SCHEMA_VERSION: u16 = 1;
const STAGE5D_SUPPORTED_BROKER_PROTOCOL_SCHEMA_VERSION: u16 = 2;
const STAGE5D_SUPPORTED_RUNTIME_STATE_SCHEMA_VERSION: u16 = 2;
pub(crate) const STAGE5D_RUNTIME_SEMANTIC_COMPATIBILITY_ID: &str =
    "stage5d_runtime_semantic_compatibility_v1";
const STAGE5D_COMPATIBLE_SOURCE_BUILD_IDS: &[&str] = &[
    STAGE5D_RUNTIME_SEMANTIC_COMPATIBILITY_ID,
    "source_commit:92e6e0685b1cbab6f4c6271abe1db8ab690a1ded",
];

fn stage5d_is_false(value: &bool) -> bool {
    !*value
}

fn stage5d_string_is_empty(value: &str) -> bool {
    value.is_empty()
}

/// Opaque proof that a validated Stage 5D runtime-private extension has been
/// applied in the persistence-enabled restore path.
pub struct Stage5dPrivateStateAppliedPaperStrategy {
    loaded: crate::stage5c_paper_host::Stage5cRuntimeStateLoadedPaperStrategy,
    envelope: Stage5dPersistenceEnvelope,
}

impl Stage5dPrivateStateAppliedPaperStrategy {
    /// Redacted snapshot id for evidence and diagnostics.
    pub fn snapshot_id(&self) -> &str {
        &self.envelope.snapshot_id
    }

    /// Envelope schema version for evidence and diagnostics.
    pub fn schema_version(&self) -> u16 {
        self.envelope.schema_version
    }

    /// Redacted fingerprint of the validated restore evidence retained inside
    /// this opaque capability.
    pub fn evidence_fingerprint(&self) -> &str {
        &self.envelope.payload_checksum_sha256
    }

    /// Redacted proof that the loaded runtime capability is retained inside the
    /// opaque Stage 5D wrapper.
    pub fn runtime_private_applied(&self) -> bool {
        let _ = &self.loaded;
        true
    }
}

/// Opaque proof that a Stage 5C loaded runtime capability and a validated
/// Stage 5D persistence envelope were pair-bound before private mutation.
pub struct Stage5dEnvelopeBoundRuntimeStateLoaded {
    loaded: crate::stage5c_paper_host::Stage5cRuntimeStateLoadedPaperStrategy,
    envelope: Stage5dPersistenceEnvelope,
}

impl Stage5dEnvelopeBoundRuntimeStateLoaded {
    /// Redacted snapshot id for evidence and diagnostics.
    pub fn snapshot_id(&self) -> &str {
        &self.envelope.snapshot_id
    }

    /// Envelope schema version for evidence and diagnostics.
    pub fn schema_version(&self) -> u16 {
        self.envelope.schema_version
    }

    /// Redacted fingerprint of the validated restore evidence retained inside
    /// this opaque capability.
    pub fn evidence_fingerprint(&self) -> &str {
        &self.envelope.payload_checksum_sha256
    }
}

/// Opaque proof that the Stage 5D restore path has passed controlled bootstrap.
pub struct Stage5dBootstrappedPaperStrategy {
    bootstrapped: crate::stage5c_paper_host::Stage5cBootstrappedPaperStrategy,
    envelope: Stage5dPersistenceEnvelope,
}

impl Stage5dBootstrappedPaperStrategy {
    /// Redacted snapshot id for evidence and diagnostics.
    pub fn snapshot_id(&self) -> &str {
        &self.envelope.snapshot_id
    }

    /// Envelope schema version for evidence and diagnostics.
    pub fn schema_version(&self) -> u16 {
        self.envelope.schema_version
    }

    /// Redacted fingerprint of the validated restore evidence retained inside
    /// this opaque capability.
    pub fn evidence_fingerprint(&self) -> &str {
        &self.envelope.payload_checksum_sha256
    }

    /// Redacted proof that the Stage 5C bootstrap callback already completed
    /// inside this opaque Stage 5D capability.
    pub fn bootstrap_notified(&self) -> bool {
        let _ = &self.bootstrapped;
        true
    }
}

/// Opaque proof that authoritative riskgate state has been injected before the
/// runtime-state-restored callback.
///
/// A Stage 5D capability cannot bypass the separately reviewed restored-state
/// transition by calling the crate-private Stage 5C bridge directly.
///
/// ```compile_fail
/// use strategy_runtime_core::stage5c_paper_host;
/// ```
///
/// Recovery plans and decisions are crate-private, so downstream callers
/// cannot construct, forge, or reorder them.
///
/// ```compile_fail
/// use strategy_runtime_core::stage5d_persistence::Stage5dRiskGateRecoveryPlan;
/// ```
///
/// ```compile_fail
/// use strategy_runtime_core::stage5d_persistence::Stage5dRiskGateRecoveryPlanDecision;
/// ```
///
/// The injected capability cannot be constructed or unpacked externally because
/// all fields are private.
///
/// ```compile_fail
/// use strategy_runtime_core::stage5d_persistence::Stage5dRiskGateInjectedPaperStrategy;
/// let forged = Stage5dRiskGateInjectedPaperStrategy {};
/// ```
///
/// Private fields cannot be extracted from the opaque capability.
///
/// ```compile_fail
/// # fn demo(injected: strategy_runtime_core::stage5d_persistence::Stage5dRiskGateInjectedPaperStrategy) {
/// let _raw_bootstrapped = injected.bootstrapped;
/// # }
/// ```
///
/// The crate-private Stage 5C restored bridge cannot be imported or called by
/// external code.
///
/// ```compile_fail
/// use strategy_runtime_core::stage5c_paper_host::stage5d_notify_runtime_state_restored_bridge_at;
/// ```
///
/// The transition consumes the injected input, so a second call cannot reuse the
/// moved value.
///
/// ```compile_fail
/// use strategy_runtime_core::stage5d_persistence::stage5d_notify_runtime_state_restored;
/// # fn demo(injected: strategy_runtime_core::stage5d_persistence::Stage5dRiskGateInjectedPaperStrategy) {
/// let _first = stage5d_notify_runtime_state_restored(injected);
/// let _second = stage5d_notify_runtime_state_restored(injected);
/// # }
/// ```
///
/// A successful Stage 5C restored output cannot be converted back into a Stage
/// 5D injected capability.
///
/// ```compile_fail
/// # fn demo(restored: strategy_runtime_core::stage5c_paper_host::Stage5cRuntimeStateRestoredPaperStrategy) {
/// let _: strategy_runtime_core::stage5d_persistence::Stage5dRiskGateInjectedPaperStrategy = restored;
/// # }
/// ```
///
/// Blocked and terminal outcomes are not interchangeable.
///
/// ```compile_fail
/// # fn demo(outcome: strategy_runtime_core::stage5d_persistence::Stage5dRuntimeStateRestoreOutcome) {
/// let strategy_runtime_core::stage5d_persistence::Stage5dRuntimeStateRestoreOutcome::Blocked(_blocked) = outcome else {
///     return;
/// };
/// let strategy_runtime_core::stage5d_persistence::Stage5dRuntimeStateRestoreOutcome::Terminal(_terminal) = outcome else {
///     return;
/// };
/// # }
/// ```
///
/// Private preflight and test hooks cannot be imported externally.
///
/// ```compile_fail
/// use strategy_runtime_core::stage5d_persistence::validate_stage5d_runtime_state_restored_preflight;
/// ```
///
/// ```compile_fail
/// use strategy_runtime_core::stage5d_persistence::stage5d_test_notify_runtime_state_restored_with_state_override_at;
/// ```
pub struct Stage5dRiskGateInjectedPaperStrategy {
    bootstrapped: crate::stage5c_paper_host::Stage5cBootstrappedPaperStrategy,
    envelope: Stage5dPersistenceEnvelope,
    recovery_plan: Stage5dRiskGateRecoveryPlan,
}

impl Stage5dRiskGateInjectedPaperStrategy {
    /// Redacted snapshot id for evidence and diagnostics.
    pub fn snapshot_id(&self) -> &str {
        &self.envelope.snapshot_id
    }

    /// Envelope schema version for evidence and diagnostics.
    pub fn schema_version(&self) -> u16 {
        self.envelope.schema_version
    }

    /// Redacted fingerprint of the validated restore evidence retained inside
    /// this opaque capability.
    pub fn evidence_fingerprint(&self) -> &str {
        &self.envelope.payload_checksum_sha256
    }

    /// Redacted proof that authoritative riskgate state was injected through
    /// the Stage 5C crate-private bridge.
    pub fn riskgate_injected(&self) -> bool {
        let _ = &self.bootstrapped;
        true
    }

    /// Redacted number of deterministic recovery decisions retained by this
    /// opaque capability.
    pub fn recovery_decision_count(&self) -> usize {
        self.recovery_plan.decisions.len()
    }

    /// Whether the retained recovery proof contains no unresolved append,
    /// materialization or runtime acknowledgement work.
    pub fn recovery_complete(&self) -> bool {
        self.recovery_plan.recovery_complete
    }

    /// Redacted fingerprint binding the retained recovery plan to the exact
    /// envelope, ledger evidence, identity, generation and ordered frontiers.
    pub fn recovery_plan_fingerprint(&self) -> &str {
        &self.recovery_plan.plan_fingerprint_sha256
    }
}

/// Public redacted outcome for the controlled Stage 5D runtime-state-restored
/// transition. A block happens before the source callback and preserves the
/// unchanged input capability. A terminal failure happens after callback
/// execution has started and never exposes a retry capability.
pub enum Stage5dRuntimeStateRestoreOutcome {
    Blocked(Stage5dRuntimeStateRestoreBlocked),
    Terminal(Stage5dRuntimeStateRestoreTerminalFailure),
}

/// Recoverable pre-callback block. The exact input capability is retained
/// opaquely so callers can restart the recovery pipeline with fresh broker
/// truth or complete unfinished durable recovery before retrying.
pub struct Stage5dRuntimeStateRestoreBlocked {
    injected: Box<Stage5dRiskGateInjectedPaperStrategy>,
    reason: Stage5dRuntimeStateRestoreBlockedReason,
}

impl Stage5dRuntimeStateRestoreBlocked {
    /// Redacted reason for the pre-callback block.
    pub fn reason(&self) -> Stage5dRuntimeStateRestoreBlockedReason {
        self.reason
    }

    /// Redacted proof that the input capability was preserved unchanged.
    pub fn input_capability_preserved(&self) -> bool {
        let _ = &self.injected;
        true
    }

    /// Snapshot id retained for operator diagnostics.
    pub fn snapshot_id(&self) -> &str {
        self.injected.snapshot_id()
    }

    /// Redacted envelope/evidence fingerprint retained for diagnostics.
    pub fn evidence_fingerprint(&self) -> &str {
        self.injected.evidence_fingerprint()
    }

    /// Redacted recovery-plan fingerprint retained for diagnostics.
    pub fn recovery_plan_fingerprint(&self) -> &str {
        self.injected.recovery_plan_fingerprint()
    }

    /// Operator-safe recovery disposition.
    pub fn recovery_disposition(&self) -> Stage5dRuntimeStateRestoreRecoveryDisposition {
        match self.reason {
            Stage5dRuntimeStateRestoreBlockedReason::RecoveryIncomplete
            | Stage5dRuntimeStateRestoreBlockedReason::PendingRiskGateFinalizations
            | Stage5dRuntimeStateRestoreBlockedReason::RecoveryPlanBindingMismatch => {
                Stage5dRuntimeStateRestoreRecoveryDisposition::CompleteRecoveryThenRetryPipeline
            }
            Stage5dRuntimeStateRestoreBlockedReason::AdmissionExpired
            | Stage5dRuntimeStateRestoreBlockedReason::BrokerTruthPositionMismatch
            | Stage5dRuntimeStateRestoreBlockedReason::BrokerTruthSideMismatch
            | Stage5dRuntimeStateRestoreBlockedReason::BrokerQuantityNotRepresentable => {
                Stage5dRuntimeStateRestoreRecoveryDisposition::RestartWithFreshBrokerTruth
            }
            Stage5dRuntimeStateRestoreBlockedReason::LifecycleTimestampReversal
            | Stage5dRuntimeStateRestoreBlockedReason::BrokerOwnedProtectiveId
            | Stage5dRuntimeStateRestoreBlockedReason::ClosedBoundaryOpened
            | Stage5dRuntimeStateRestoreBlockedReason::RecoveryIndexMismatch => {
                Stage5dRuntimeStateRestoreRecoveryDisposition::RestartFromDurableEnvelope
            }
        }
    }

    #[cfg(test)]
    fn stage5d_test_strategy_state_fingerprint(&self) -> String {
        stage5d_test_strategy_state_fingerprint(self.injected.bootstrapped.stage5d_strategy())
    }

    #[cfg(test)]
    fn stage5d_test_closed_boundary_flags(&self) -> (bool, bool, bool) {
        let admission = self.injected.bootstrapped.stage5d_admission();
        (
            admission.is_paper_only(),
            admission.runtime_host_attached(),
            admission.intent_sink_attached(),
        )
    }
}

/// Terminal post-callback failure. The consumed input capability is not
/// exposed; operational recovery must restart from the durable envelope and
/// fresh broker-truth admission.
pub struct Stage5dRuntimeStateRestoreTerminalFailure {
    reason: Stage5dRuntimeStateRestoreTerminalReason,
    snapshot_id: String,
    evidence_fingerprint: String,
    recovery_plan_fingerprint: String,
}

impl Stage5dRuntimeStateRestoreTerminalFailure {
    /// Redacted terminal reason.
    pub fn reason(&self) -> Stage5dRuntimeStateRestoreTerminalReason {
        self.reason
    }

    /// Snapshot id retained for operator diagnostics.
    pub fn snapshot_id(&self) -> &str {
        &self.snapshot_id
    }

    /// Redacted envelope/evidence fingerprint retained for diagnostics.
    pub fn evidence_fingerprint(&self) -> &str {
        &self.evidence_fingerprint
    }

    /// Redacted recovery-plan fingerprint retained for diagnostics.
    pub fn recovery_plan_fingerprint(&self) -> &str {
        &self.recovery_plan_fingerprint
    }

    /// Terminal failures never retain a retryable input capability.
    pub fn retry_capability_available(&self) -> bool {
        false
    }
}

/// Redacted pre-callback block categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage5dRuntimeStateRestoreBlockedReason {
    RecoveryIncomplete,
    PendingRiskGateFinalizations,
    RecoveryPlanBindingMismatch,
    AdmissionExpired,
    LifecycleTimestampReversal,
    BrokerQuantityNotRepresentable,
    BrokerTruthPositionMismatch,
    BrokerTruthSideMismatch,
    BrokerOwnedProtectiveId,
    ClosedBoundaryOpened,
    RecoveryIndexMismatch,
}

/// Redacted terminal post-callback categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage5dRuntimeStateRestoreTerminalReason {
    CallbackEmittedIntent,
    PostCallbackBrokerTruthPositionMismatch,
    PostCallbackBrokerTruthSideMismatch,
    PostCallbackBrokerOwnedProtectiveId,
    PostCallbackStateInvalid,
}

/// Operator-safe recovery guidance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage5dRuntimeStateRestoreRecoveryDisposition {
    RestartWithFreshBrokerTruth,
    CompleteRecoveryThenRetryPipeline,
    RestartFromDurableEnvelope,
}

/// Opaque validated runtime-private extension marker.
pub struct Stage5dValidatedRuntimePrivateExtension {
    _private: (),
}

/// Opaque proof that Stage 5D-b2b-c riskgate ledger evidence passed
/// source-compatible identity, tail-hash and ledger-shape validation.
pub struct Stage5dValidatedRiskGateLedgerEvidence {
    evidence: Stage5dRiskGateLedgerEvidence,
    evidence_fingerprint_sha256: String,
}

impl Stage5dValidatedRiskGateLedgerEvidence {
    /// Redacted ledger tail hash for evidence and diagnostics.
    pub fn ledger_tail_hash(&self) -> &str {
        &self.evidence.ledger_tail_hash
    }

    /// Number of normalized ledger records in the validated evidence.
    pub fn ledger_records_count(&self) -> usize {
        self.evidence.ledger_records.len()
    }

    /// Redacted fingerprint binding the ledger tail to generation and
    /// current-shadow metadata. This is separate from the record-tail hash so
    /// the historical tail-hash meaning remains versioned and unchanged.
    pub fn evidence_fingerprint_sha256(&self) -> &str {
        &self.evidence_fingerprint_sha256
    }
}

/// Recoverable Stage 5D runtime-private apply block. The original loaded
/// capability is retained internally so no partial runtime mutation leaks to
/// callers, while diagnostics expose only the redacted reason.
pub struct Stage5dRuntimePrivateApplyBlocked {
    loaded: Box<crate::stage5c_paper_host::Stage5cRuntimeStateLoadedPaperStrategy>,
    reason: Stage5dEnvelopeValidationError,
}

impl Stage5dRuntimePrivateApplyBlocked {
    /// Return the redacted validation reason without exposing the strategy.
    pub fn reason(&self) -> Stage5dEnvelopeValidationError {
        self.reason
    }

    /// Redacted proof that the pre-mutation loaded capability was preserved.
    pub fn input_capability_preserved(&self) -> bool {
        let _ = &self.loaded;
        true
    }
}

/// Recoverable Stage 5D bootstrap block. The original applied capability is
/// retained internally so broker-truth bootstrap can be retried after the
/// authoritative snapshot is corrected, without exposing runtime internals.
pub struct Stage5dBootstrapBlocked {
    applied: Box<Stage5dPrivateStateAppliedPaperStrategy>,
    reason: Stage5dBootstrapBlockReason,
}

impl Stage5dBootstrapBlocked {
    /// Return the redacted bootstrap block reason.
    pub fn reason(&self) -> Stage5dBootstrapBlockReason {
        self.reason
    }

    /// Redacted proof that the pre-bootstrap applied capability was preserved.
    pub fn input_capability_preserved(&self) -> bool {
        let _ = &self.applied;
        true
    }

    /// Redacted snapshot id for evidence and diagnostics.
    pub fn snapshot_id(&self) -> &str {
        self.applied.snapshot_id()
    }
}

/// Recoverable Stage 5D riskgate injection block. The original bootstrapped
/// capability is retained internally so no public caller can proceed to the
/// runtime-state-restored callback with a partially validated riskgate state.
pub struct Stage5dRiskGateInjectionBlocked {
    bootstrapped: Box<Stage5dBootstrappedPaperStrategy>,
    reason: Stage5dRiskGateInjectionBlockReason,
}

impl Stage5dRiskGateInjectionBlocked {
    /// Return the redacted riskgate injection block reason.
    pub fn reason(&self) -> Stage5dRiskGateInjectionBlockReason {
        self.reason
    }

    /// Redacted proof that the pre-injection bootstrapped capability was
    /// preserved.
    pub fn input_capability_preserved(&self) -> bool {
        let _ = &self.bootstrapped;
        true
    }

    /// Redacted snapshot id for evidence and diagnostics.
    pub fn snapshot_id(&self) -> &str {
        self.bootstrapped.snapshot_id()
    }
}

/// Public redacted Stage 5D authoritative riskgate injection blocker
/// categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage5dRiskGateInjectionBlockReason {
    BindingMismatch,
    RiskGateNotApplicable,
    LedgerEvidenceInvalid,
    LedgerTailMismatch,
    LedgerGenerationMismatch,
    LedgerIdentityMismatch,
    LedgerRecordDerivedStateMismatch,
    LedgerFinalizationTimestampInvalid,
    MaterializedStateMismatch,
    MaterializedStateInvalid,
    RuntimePendingFinalizationMissingFromOutbox,
    OutboxStateInconsistent,
    OutboxIdentityMismatch,
    DuplicateOutboxSession,
    DuplicateOutboxIdentity,
    RecoveryPlanBindingMismatch,
}

/// Public redacted Stage 5D bootstrap blocker categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage5dBootstrapBlockReason {
    AdmissionExpired,
    StrategyTargetMismatch,
    StrategyTickSizeMismatch,
    ActiveOrdersRequireOwnershipMapping,
    SnapshotAccountMismatch,
    SnapshotInstrumentMismatch,
    PositionQuantityNotRepresentable,
    PositionAveragePriceNotRepresentable,
    BindingMismatch,
    BrokerTruthPositionMismatch,
    ExpectedWorkingOrderMissing,
    ExpectedWorkingStopUnsupported,
    SemanticStateInvalid,
}

/// Opaque proof that the persistence envelope passed strict Stage 5D-b2a
/// schema-only restore-contract validation. Future mutation gates must consume
/// this capability, not a raw deserialized envelope.
pub struct Stage5dValidatedPersistenceEnvelope {
    envelope: Stage5dPersistenceEnvelope,
}

impl Stage5dValidatedPersistenceEnvelope {
    /// Redacted snapshot id for evidence and diagnostics.
    pub fn snapshot_id(&self) -> &str {
        &self.envelope.snapshot_id
    }

    /// Envelope schema version for evidence and diagnostics.
    pub fn schema_version(&self) -> u16 {
        self.envelope.schema_version
    }
}

/// Bind a loaded Stage 5C runtime capability to one exact validated Stage 5D
/// persistence envelope before any private mutation can happen.
pub fn stage5d_bind_runtime_state_loaded(
    loaded: crate::stage5c_paper_host::Stage5cRuntimeStateLoadedPaperStrategy,
    validated: Stage5dValidatedPersistenceEnvelope,
) -> Result<Stage5dEnvelopeBoundRuntimeStateLoaded, Stage5dRuntimePrivateApplyBlocked> {
    let envelope = validated.envelope;
    if let Err(reason) = validate_loaded_envelope_binding(&loaded, &envelope) {
        return Err(Stage5dRuntimePrivateApplyBlocked {
            loaded: Box::new(loaded),
            reason,
        });
    }

    Ok(Stage5dEnvelopeBoundRuntimeStateLoaded { loaded, envelope })
}

/// Retry pair-binding after a recoverable Stage 5D block without exposing the
/// preserved Stage 5C loaded runtime capability.
pub fn stage5d_retry_bind_runtime_state_loaded(
    blocked: Stage5dRuntimePrivateApplyBlocked,
    validated: Stage5dValidatedPersistenceEnvelope,
) -> Result<Stage5dEnvelopeBoundRuntimeStateLoaded, Stage5dRuntimePrivateApplyBlocked> {
    stage5d_bind_runtime_state_loaded(*blocked.loaded, validated)
}

/// Apply the pair-bound Stage 5D runtime-private extension before any Stage 5C
/// bootstrap/runtime-restored callback is allowed to run.
///
/// The function consumes a bound capability produced by
/// [`stage5d_bind_runtime_state_loaded`]. It validates the private DTO against
/// already-restored semantic state first, then mutates the runtime in a single
/// apply step. Broker working sets are treated only as expected hints;
/// authoritative active objects must still come from broker truth in later gates.
pub fn stage5d_apply_runtime_private_extension(
    bound: Stage5dEnvelopeBoundRuntimeStateLoaded,
) -> Result<Stage5dPrivateStateAppliedPaperStrategy, Stage5dRuntimePrivateApplyBlocked> {
    let Stage5dEnvelopeBoundRuntimeStateLoaded { loaded, envelope } = bound;
    let (mut strategy, admission, restored, load_origin) = loaded.stage5d_into_parts();
    if let Err(reason) =
        strategy.stage5d_apply_runtime_private_extension(&envelope.runtime_private_extension)
    {
        return Err(Stage5dRuntimePrivateApplyBlocked {
            loaded: Box::new(
                crate::stage5c_paper_host::Stage5cRuntimeStateLoadedPaperStrategy::stage5d_from_parts(
                    strategy,
                    admission,
                    restored,
                    load_origin,
                ),
            ),
            reason,
        });
    }

    Ok(Stage5dPrivateStateAppliedPaperStrategy {
        loaded:
            crate::stage5c_paper_host::Stage5cRuntimeStateLoadedPaperStrategy::stage5d_from_parts(
                strategy,
                admission,
                restored,
                load_origin,
            ),
        envelope,
    })
}

/// Notify controlled broker-truth bootstrap after Stage 5D runtime-private
/// state has been applied.
///
/// This consumes only [`Stage5dPrivateStateAppliedPaperStrategy`]. The retained
/// persistence working sets are treated as expected hints, while the Stage 5C
/// admission broker snapshot remains authoritative. Missing or mismatched
/// broker objects fail closed before any bootstrap callback is emitted.
pub fn stage5d_notify_broker_truth_bootstrap(
    applied: Stage5dPrivateStateAppliedPaperStrategy,
) -> Result<Stage5dBootstrappedPaperStrategy, Stage5dBootstrapBlocked> {
    stage5d_notify_broker_truth_bootstrap_at(applied, Utc::now())
}

/// Retry controlled broker-truth bootstrap with a fresh Stage 5C admission.
///
/// The retry path consumes only a preserved [`Stage5dBootstrapBlocked`]
/// capability plus a newly admitted broker-truth snapshot. It does not expose
/// the private-applied strategy and does not re-run runtime-private apply.
pub fn stage5d_retry_broker_truth_bootstrap(
    blocked: Stage5dBootstrapBlocked,
    fresh_admission: crate::stage5c_paper_host::Stage5cPaperHostAdmission,
) -> Result<Stage5dBootstrappedPaperStrategy, Stage5dBootstrapBlocked> {
    stage5d_retry_broker_truth_bootstrap_at(blocked, fresh_admission, Utc::now())
}

/// Validate deterministic authoritative riskgate ledger evidence before it can
/// be consumed by the Stage 5D-b2b-c injection transition.
pub fn stage5d_validate_riskgate_ledger_evidence(
    evidence: Stage5dRiskGateLedgerEvidence,
) -> Result<Stage5dValidatedRiskGateLedgerEvidence, Stage5dRiskGateInjectionBlockReason> {
    if evidence.schema_version != STAGE5D_RISKGATE_SCHEMA_VERSION
        || evidence.identity.strategy_id.is_empty()
        || evidence.identity.profile_id.is_empty()
        || evidence.identity.mr_variant.is_empty()
        || evidence.identity.timeframe.is_empty()
        || evidence.identity.session_policy.is_empty()
        || evidence.identity.model_version.is_empty()
        || evidence.ledger_tail_hash.is_empty()
    {
        return Err(Stage5dRiskGateInjectionBlockReason::LedgerEvidenceInvalid);
    }
    if evidence.current_generation != crate::hybrid_intraday::RISK_GATE_STATE_GENERATION {
        return Err(Stage5dRiskGateInjectionBlockReason::LedgerGenerationMismatch);
    }
    stage5d_validate_source_exact_riskgate_records(&evidence, None)?;
    let computed = stage5d_compute_riskgate_ledger_tail_hash(&evidence)
        .map_err(|_| Stage5dRiskGateInjectionBlockReason::LedgerEvidenceInvalid)?;
    if computed != evidence.ledger_tail_hash {
        return Err(Stage5dRiskGateInjectionBlockReason::LedgerTailMismatch);
    }
    let evidence_fingerprint_sha256 = stage5d_compute_riskgate_evidence_fingerprint(&evidence)
        .map_err(|_| Stage5dRiskGateInjectionBlockReason::LedgerEvidenceInvalid)?;
    Ok(Stage5dValidatedRiskGateLedgerEvidence {
        evidence,
        evidence_fingerprint_sha256,
    })
}

/// Inject authoritative persisted riskgate projection after controlled
/// broker-truth bootstrap and before any runtime-state-restored callback.
///
/// The transition consumes only [`Stage5dBootstrappedPaperStrategy`]. It
/// cross-checks the persisted riskgate projection against the semantic runtime
/// snapshot and durable finalization outbox, then delegates the actual strategy
/// callback to one crate-private Stage 5C bridge.
pub fn stage5d_inject_authoritative_riskgate(
    bootstrapped: Stage5dBootstrappedPaperStrategy,
    validated_evidence: Stage5dValidatedRiskGateLedgerEvidence,
) -> Result<Stage5dRiskGateInjectedPaperStrategy, Stage5dRiskGateInjectionBlocked> {
    stage5d_inject_authoritative_riskgate_with_evidence(bootstrapped, validated_evidence)
}

/// Retry authoritative riskgate injection with fresh ledger evidence without
/// repeating runtime-private apply or broker-truth bootstrap.
pub fn stage5d_retry_authoritative_riskgate_injection(
    blocked: Stage5dRiskGateInjectionBlocked,
    fresh_evidence: Stage5dValidatedRiskGateLedgerEvidence,
) -> Result<Stage5dRiskGateInjectedPaperStrategy, Stage5dRiskGateInjectionBlocked> {
    stage5d_inject_authoritative_riskgate_with_evidence(*blocked.bootstrapped, fresh_evidence)
}

/// Notify the source runtime-state-restored callback exactly once after the
/// Stage 5D persistence chain has completed runtime-private apply,
/// broker-truth bootstrap and authoritative riskgate injection.
///
/// This is pure in-process lifecycle control. It does not open Redis, FINAM,
/// transport, dispatch, broker execution or runtime-live.
pub fn stage5d_notify_runtime_state_restored(
    injected: Stage5dRiskGateInjectedPaperStrategy,
) -> Result<
    crate::stage5c_paper_host::Stage5cRuntimeStateRestoredPaperStrategy,
    Stage5dRuntimeStateRestoreOutcome,
> {
    stage5d_notify_runtime_state_restored_at(injected, Utc::now())
}

fn stage5d_inject_authoritative_riskgate_with_evidence(
    bootstrapped: Stage5dBootstrappedPaperStrategy,
    validated_evidence: Stage5dValidatedRiskGateLedgerEvidence,
) -> Result<Stage5dRiskGateInjectedPaperStrategy, Stage5dRiskGateInjectionBlocked> {
    let Stage5dBootstrappedPaperStrategy {
        bootstrapped: stage5c_bootstrapped,
        envelope,
    } = bootstrapped;

    let (riskgate, recovery_plan) = match stage5d_authoritative_riskgate_state_from_evidence(
        &stage5c_bootstrapped,
        &envelope,
        &validated_evidence,
    ) {
        Ok(validated) => validated,
        Err(reason) => {
            return Err(Stage5dRiskGateInjectionBlocked {
                bootstrapped: Box::new(Stage5dBootstrappedPaperStrategy {
                    bootstrapped: stage5c_bootstrapped,
                    envelope,
                }),
                reason,
            });
        }
    };

    let bootstrapped = crate::stage5c_paper_host::stage5d_inject_authoritative_riskgate_state(
        stage5c_bootstrapped,
        riskgate,
    );

    Ok(Stage5dRiskGateInjectedPaperStrategy {
        bootstrapped,
        envelope,
        recovery_plan,
    })
}

fn stage5d_notify_runtime_state_restored_at(
    injected: Stage5dRiskGateInjectedPaperStrategy,
    restored_at: DateTime<Utc>,
) -> Result<
    crate::stage5c_paper_host::Stage5cRuntimeStateRestoredPaperStrategy,
    Stage5dRuntimeStateRestoreOutcome,
> {
    if let Err(reason) = validate_stage5d_runtime_state_restored_preflight(&injected, restored_at) {
        return Err(Stage5dRuntimeStateRestoreOutcome::Blocked(
            stage5d_runtime_restore_blocked(injected, reason),
        ));
    }

    let snapshot_id = injected.snapshot_id().to_string();
    let evidence_fingerprint = injected.evidence_fingerprint().to_string();
    let recovery_plan_fingerprint = injected.recovery_plan_fingerprint().to_string();
    let Stage5dRiskGateInjectedPaperStrategy { bootstrapped, .. } = injected;
    match crate::stage5c_paper_host::stage5d_notify_runtime_state_restored_bridge_at(
        bootstrapped,
        restored_at,
    ) {
        Ok(restored) => Ok(restored),
        Err(reason) => Err(Stage5dRuntimeStateRestoreOutcome::Terminal(
            stage5d_runtime_restore_terminal(
                snapshot_id,
                evidence_fingerprint,
                recovery_plan_fingerprint,
                reason,
            ),
        )),
    }
}

#[cfg(test)]
fn stage5d_test_notify_runtime_state_restored_forcing_intent_at(
    injected: Stage5dRiskGateInjectedPaperStrategy,
    restored_at: DateTime<Utc>,
) -> Result<
    crate::stage5c_paper_host::Stage5cRuntimeStateRestoredPaperStrategy,
    Stage5dRuntimeStateRestoreOutcome,
> {
    if let Err(reason) = validate_stage5d_runtime_state_restored_preflight(&injected, restored_at) {
        return Err(Stage5dRuntimeStateRestoreOutcome::Blocked(
            stage5d_runtime_restore_blocked(injected, reason),
        ));
    }

    let snapshot_id = injected.snapshot_id().to_string();
    let evidence_fingerprint = injected.evidence_fingerprint().to_string();
    let recovery_plan_fingerprint = injected.recovery_plan_fingerprint().to_string();
    let Stage5dRiskGateInjectedPaperStrategy { bootstrapped, .. } = injected;
    match crate::stage5c_paper_host::stage5d_test_notify_runtime_state_restored_bridge_forcing_intent_at(
        bootstrapped,
        restored_at,
    ) {
        Ok(restored) => Ok(restored),
        Err(reason) => Err(Stage5dRuntimeStateRestoreOutcome::Terminal(
            stage5d_runtime_restore_terminal(
                snapshot_id,
                evidence_fingerprint,
                recovery_plan_fingerprint,
                reason,
            ),
        )),
    }
}

#[cfg(test)]
fn stage5d_test_notify_runtime_state_restored_with_state_override_at(
    injected: Stage5dRiskGateInjectedPaperStrategy,
    restored_at: DateTime<Utc>,
    state: StrategyState,
) -> Result<
    crate::stage5c_paper_host::Stage5cRuntimeStateRestoredPaperStrategy,
    Stage5dRuntimeStateRestoreOutcome,
> {
    if let Err(reason) = validate_stage5d_runtime_state_restored_preflight(&injected, restored_at) {
        return Err(Stage5dRuntimeStateRestoreOutcome::Blocked(
            stage5d_runtime_restore_blocked(injected, reason),
        ));
    }

    let snapshot_id = injected.snapshot_id().to_string();
    let evidence_fingerprint = injected.evidence_fingerprint().to_string();
    let recovery_plan_fingerprint = injected.recovery_plan_fingerprint().to_string();
    let Stage5dRiskGateInjectedPaperStrategy { bootstrapped, .. } = injected;
    match crate::stage5c_paper_host::stage5d_test_notify_runtime_state_restored_bridge_with_state_override_at(
        bootstrapped,
        restored_at,
        state,
    ) {
        Ok(restored) => Ok(restored),
        Err(reason) => Err(Stage5dRuntimeStateRestoreOutcome::Terminal(
            stage5d_runtime_restore_terminal(
                snapshot_id,
                evidence_fingerprint,
                recovery_plan_fingerprint,
                reason,
            ),
        )),
    }
}

fn stage5d_runtime_restore_blocked(
    injected: Stage5dRiskGateInjectedPaperStrategy,
    reason: Stage5dRuntimeStateRestoreBlockedReason,
) -> Stage5dRuntimeStateRestoreBlocked {
    Stage5dRuntimeStateRestoreBlocked {
        injected: Box::new(injected),
        reason,
    }
}

fn stage5d_runtime_restore_terminal(
    snapshot_id: String,
    evidence_fingerprint: String,
    recovery_plan_fingerprint: String,
    reason: crate::stage5c_paper_host::Stage5dRuntimeStateRestoredBridgeError,
) -> Stage5dRuntimeStateRestoreTerminalFailure {
    let reason = match reason {
        crate::stage5c_paper_host::Stage5dRuntimeStateRestoredBridgeError::CallbackEmittedIntent => {
            Stage5dRuntimeStateRestoreTerminalReason::CallbackEmittedIntent
        }
        crate::stage5c_paper_host::Stage5dRuntimeStateRestoredBridgeError::Stage5c(
            crate::stage5c_paper_host::Stage5cRuntimeStateRestoreError::BrokerTruthPositionMismatch,
        ) => Stage5dRuntimeStateRestoreTerminalReason::PostCallbackBrokerTruthPositionMismatch,
        crate::stage5c_paper_host::Stage5dRuntimeStateRestoredBridgeError::Stage5c(
            crate::stage5c_paper_host::Stage5cRuntimeStateRestoreError::BrokerTruthSideMismatch,
        ) => Stage5dRuntimeStateRestoreTerminalReason::PostCallbackBrokerTruthSideMismatch,
        crate::stage5c_paper_host::Stage5dRuntimeStateRestoredBridgeError::Stage5c(
            crate::stage5c_paper_host::Stage5cRuntimeStateRestoreError::BrokerOwnedOrderIdMismatch,
        ) => Stage5dRuntimeStateRestoreTerminalReason::PostCallbackBrokerOwnedProtectiveId,
        crate::stage5c_paper_host::Stage5dRuntimeStateRestoredBridgeError::Stage5c(_) => {
            Stage5dRuntimeStateRestoreTerminalReason::PostCallbackStateInvalid
        }
    };
    Stage5dRuntimeStateRestoreTerminalFailure {
        reason,
        snapshot_id,
        evidence_fingerprint,
        recovery_plan_fingerprint,
    }
}

fn validate_stage5d_runtime_state_restored_preflight(
    injected: &Stage5dRiskGateInjectedPaperStrategy,
    restored_at: DateTime<Utc>,
) -> Result<(), Stage5dRuntimeStateRestoreBlockedReason> {
    if !injected.recovery_plan.recovery_complete
        || injected.recovery_plan.decisions.iter().any(|decision| {
            decision.durable_state != Stage5dRiskGateFinalizationState::AcknowledgedInRuntime
                || decision.action != Stage5dRiskGateRecoveryDecision::AlreadyAcknowledged
        })
    {
        return Err(Stage5dRuntimeStateRestoreBlockedReason::RecoveryIncomplete);
    }
    if !injected
        .envelope
        .runtime_private_extension
        .runtime_pending_finalizations
        .is_empty()
    {
        return Err(Stage5dRuntimeStateRestoreBlockedReason::PendingRiskGateFinalizations);
    }
    let expected_plan = stage5d_compute_riskgate_recovery_plan_fingerprint(&injected.recovery_plan)
        .map_err(|_| Stage5dRuntimeStateRestoreBlockedReason::RecoveryPlanBindingMismatch)?;
    if expected_plan != injected.recovery_plan.plan_fingerprint_sha256
        || injected.recovery_plan.envelope_fingerprint_sha256
            != injected.envelope.payload_checksum_sha256
    {
        return Err(Stage5dRuntimeStateRestoreBlockedReason::RecoveryPlanBindingMismatch);
    }
    if restored_at < injected.envelope.persisted_at_ts_utc {
        return Err(Stage5dRuntimeStateRestoreBlockedReason::LifecycleTimestampReversal);
    }

    let admission = injected.bootstrapped.stage5d_admission();
    let bootstrap_notified_at = injected.bootstrapped.stage5d_bootstrap_notified_ts();
    if !(admission.checked_ts() <= admission.issued_ts()
        && admission.issued_ts() <= bootstrap_notified_at
        && bootstrap_notified_at <= restored_at)
    {
        return Err(Stage5dRuntimeStateRestoreBlockedReason::LifecycleTimestampReversal);
    }
    if restored_at > admission.expires_at() {
        return Err(Stage5dRuntimeStateRestoreBlockedReason::AdmissionExpired);
    }
    if !admission.is_paper_only()
        || admission.runtime_host_attached()
        || admission.intent_sink_attached()
    {
        return Err(Stage5dRuntimeStateRestoreBlockedReason::ClosedBoundaryOpened);
    }
    if injected.bootstrapped.stage5d_restored().known_order_ids
        != injected.envelope.recovery_indexes.known_order_ids
        || injected.bootstrapped.stage5d_restored().pending_requests
            != injected.envelope.recovery_indexes.pending_requests
    {
        return Err(Stage5dRuntimeStateRestoreBlockedReason::RecoveryIndexMismatch);
    }
    validate_stage5d_runtime_restore_broker_truth(injected)
}

fn validate_stage5d_runtime_restore_broker_truth(
    injected: &Stage5dRiskGateInjectedPaperStrategy,
) -> Result<(), Stage5dRuntimeStateRestoreBlockedReason> {
    let admission = injected.bootstrapped.stage5d_admission();
    if admission.strategy_id() != injected.envelope.binding.strategy_id
        || admission.account_id() != &injected.envelope.binding.account_id
        || admission.target_instrument()
            != &injected.envelope.binding.instrument_id.to_instrument_id()
    {
        return Err(Stage5dRuntimeStateRestoreBlockedReason::RecoveryPlanBindingMismatch);
    }
    let broker_qty = admission
        .bootstrap_snapshot()
        .target_position_qty
        .to_f64()
        .filter(|value| value.is_finite())
        .ok_or(Stage5dRuntimeStateRestoreBlockedReason::BrokerQuantityNotRepresentable)?;
    let state = Strategy::state(injected.bootstrapped.stage5d_strategy());
    let StrategyState::HybridIntradayRuntime {
        last_position_qty,
        current_side,
        tp_order_id,
        sl_stop_order_id,
        sl_exchange_order_id,
        ..
    } = state
    else {
        return Err(Stage5dRuntimeStateRestoreBlockedReason::RecoveryPlanBindingMismatch);
    };
    if (*last_position_qty - broker_qty).abs() > f64::EPSILON {
        return Err(Stage5dRuntimeStateRestoreBlockedReason::BrokerTruthPositionMismatch);
    }
    let expected_side = if broker_qty > f64::EPSILON {
        Some(crate::hybrid_intraday::Side::Long)
    } else if broker_qty < -f64::EPSILON {
        Some(crate::hybrid_intraday::Side::Short)
    } else {
        None
    };
    if *current_side != expected_side {
        return Err(Stage5dRuntimeStateRestoreBlockedReason::BrokerTruthSideMismatch);
    }
    if tp_order_id.is_some() || sl_stop_order_id.is_some() || sl_exchange_order_id.is_some() {
        return Err(Stage5dRuntimeStateRestoreBlockedReason::BrokerOwnedProtectiveId);
    }
    Ok(())
}

#[cfg(test)]
fn stage5d_test_strategy_state_fingerprint(
    strategy: &crate::hybrid_intraday_runtime::HybridIntradayRuntimeStrategy,
) -> String {
    let state = serde_json::to_value(crate::runtime_compat::Strategy::state(strategy))
        .expect("test strategy state serializes");
    crate::stage5c_paper_host::stage5c_semantic_value_fingerprint(&state)
        .expect("test strategy state fingerprints")
}

fn stage5d_retry_broker_truth_bootstrap_at(
    blocked: Stage5dBootstrapBlocked,
    fresh_admission: crate::stage5c_paper_host::Stage5cPaperHostAdmission,
    notification_now: DateTime<Utc>,
) -> Result<Stage5dBootstrappedPaperStrategy, Stage5dBootstrapBlocked> {
    let Stage5dBootstrapBlocked { applied, reason: _ } = blocked;
    let Stage5dPrivateStateAppliedPaperStrategy { loaded, envelope } = *applied;
    let (strategy, previous_admission, restored, load_origin) = loaded.stage5d_into_parts();
    if !stage5d_fresh_admission_matches_applied_state(
        &strategy,
        &previous_admission,
        &fresh_admission,
        &envelope,
        notification_now,
    ) {
        return Err(stage5d_bootstrap_blocked(
            crate::stage5c_paper_host::Stage5cRuntimeStateLoadedPaperStrategy::stage5d_from_parts(
                strategy,
                previous_admission,
                restored,
                load_origin,
            ),
            envelope,
            Stage5dBootstrapBlockReason::BindingMismatch,
        ));
    }
    let refreshed_loaded =
        crate::stage5c_paper_host::Stage5cRuntimeStateLoadedPaperStrategy::stage5d_from_parts(
            strategy,
            fresh_admission,
            restored,
            load_origin,
        );
    stage5d_notify_broker_truth_bootstrap_at(
        Stage5dPrivateStateAppliedPaperStrategy {
            loaded: refreshed_loaded,
            envelope,
        },
        notification_now,
    )
}

fn stage5d_authoritative_riskgate_state_from_evidence(
    bootstrapped: &crate::stage5c_paper_host::Stage5cBootstrappedPaperStrategy,
    envelope: &Stage5dPersistenceEnvelope,
    validated_evidence: &Stage5dValidatedRiskGateLedgerEvidence,
) -> Result<(RiskGateRuntimeState, Stage5dRiskGateRecoveryPlan), Stage5dRiskGateInjectionBlockReason>
{
    let evidence = &validated_evidence.evidence;
    let strategy = bootstrapped.stage5d_strategy();
    if !strategy.stage5d_riskgate_applicable() {
        return Err(Stage5dRiskGateInjectionBlockReason::RiskGateNotApplicable);
    }

    let expected_identity =
        strategy.stage5d_expected_riskgate_identity(envelope.binding.strategy_id.clone());
    if !stage5d_riskgate_identity_matches_source(&envelope.riskgate.identity, &expected_identity)
        || !stage5d_riskgate_identity_matches_source(&evidence.identity, &expected_identity)
    {
        return Err(Stage5dRiskGateInjectionBlockReason::LedgerIdentityMismatch);
    }
    if evidence.ledger_tail_hash != envelope.riskgate.ledger_tail_hash {
        return Err(Stage5dRiskGateInjectionBlockReason::LedgerTailMismatch);
    }
    if evidence.current_generation != crate::hybrid_intraday::RISK_GATE_STATE_GENERATION
        || evidence.current_generation != envelope.riskgate.materialized_state.current_generation
    {
        return Err(Stage5dRiskGateInjectionBlockReason::LedgerGenerationMismatch);
    }

    let semantic: Stage5dSemanticStrategyStateV1 =
        serde_json::from_value(envelope.strategy_state.strategy_state_json.clone())
            .map_err(|_| Stage5dRiskGateInjectionBlockReason::MaterializedStateInvalid)?;
    let Stage5dSemanticStrategyStateV1::HybridIntradayRuntime(state) = semantic;

    let source_records = stage5d_validate_source_exact_riskgate_records(
        evidence,
        Some(envelope.persisted_at_ts_utc),
    )?;
    crate::hybrid_intraday::validate_ledger_record_identity(&source_records, &expected_identity)
        .map_err(|_| Stage5dRiskGateInjectionBlockReason::LedgerIdentityMismatch)?;
    let current_shadow_session_date = evidence
        .current_shadow_session_date
        .as_deref()
        .map(|value| {
            NaiveDate::parse_from_str(value, "%Y-%m-%d")
                .map_err(|_| Stage5dRiskGateInjectionBlockReason::LedgerEvidenceInvalid)
        })
        .transpose()?;
    let current_shadow_pnl_points = crate::hybrid_intraday::parse_riskgate_authority_decimal(
        &evidence.current_shadow_pnl_points,
    )
    .map_err(|_| Stage5dRiskGateInjectionBlockReason::LedgerEvidenceInvalid)?;

    let analysis = stage5d_analyze_riskgate_crash_frontiers(
        envelope,
        evidence,
        &source_records,
        &expected_identity,
        current_shadow_session_date,
        current_shadow_pnl_points,
    )?;
    stage5d_compare_rebuilt_riskgate_materialized(
        &envelope.riskgate.materialized_state,
        &analysis.materialized_projection,
    )?;
    stage5d_compare_semantic_riskgate_materialized(&state, &analysis.runtime_projection)?;
    stage5d_compare_semantic_current_shadow_overlap(&state, &analysis.current_shadow_overlap)?;
    stage5d_validate_current_shadow_source_state(
        &state,
        &analysis.current_shadow_overlap,
        &source_records,
        envelope,
        strategy,
    )?;

    let recovery_plan =
        stage5d_build_riskgate_recovery_plan(envelope, validated_evidence, &analysis)?;
    stage5d_validate_current_riskgate_recovery_decisions(&analysis)?;
    stage5d_validate_riskgate_recovery_plan_binding(&recovery_plan, envelope, validated_evidence)?;
    let runtime_projection = &analysis.runtime_projection;

    Ok((
        RiskGateRuntimeState {
            profile_id: evidence.identity.profile_id.clone(),
            last_finalized_session_date: runtime_projection.last_finalized_session_date,
            rolling_sum_lb120: runtime_projection.rolling_sum_lb120,
            mr_enabled_current_session: runtime_projection.mr_enabled_current_session,
            mr_enabled_next_session: runtime_projection.mr_enabled_next_session,
            ledger_rows_count: runtime_projection.ledger_rows_count,
        },
        recovery_plan,
    ))
}

fn stage5d_riskgate_identity_matches_source(
    identity: &Stage5dRiskGateIdentity,
    expected: &crate::hybrid_intraday::RiskGateProfileIdentity,
) -> bool {
    identity.strategy_id == expected.strategy_id
        && identity.profile_id == expected.profile_id
        && identity.mr_variant == expected.mr_variant
        && identity.timeframe == expected.timeframe
        && identity.session_policy == expected.session_policy
        && identity.model_version == expected.model_version
}

fn stage5d_source_riskgate_records_from_evidence(
    evidence: &Stage5dRiskGateLedgerEvidence,
) -> Result<Vec<crate::hybrid_intraday::RiskGateLedgerRecord>, Stage5dRiskGateInjectionBlockReason>
{
    evidence
        .ledger_records
        .iter()
        .map(|record| {
            let session_date = NaiveDate::parse_from_str(&record.session_date, "%Y-%m-%d")
                .map_err(|_| Stage5dRiskGateInjectionBlockReason::LedgerEvidenceInvalid)?;
            let shadow_pnl_points = parse_finite_decimal_string(&record.shadow_pnl_points)
                .map_err(|_| Stage5dRiskGateInjectionBlockReason::LedgerEvidenceInvalid)?;
            let rolling_sum_before_session =
                parse_finite_decimal_string(&record.rolling_sum_before_session)
                    .map_err(|_| Stage5dRiskGateInjectionBlockReason::LedgerEvidenceInvalid)?;
            let rolling_sum_lb120 = parse_finite_decimal_string(&record.rolling_sum_lb120)
                .map_err(|_| Stage5dRiskGateInjectionBlockReason::LedgerEvidenceInvalid)?;
            Ok(crate::hybrid_intraday::RiskGateLedgerRecord {
                row: crate::hybrid_intraday::RiskGateSessionRow {
                    session_date,
                    shadow_pnl_points,
                    shadow_trade_count: record.shadow_trade_count,
                    rolling_sum_before_session,
                    mr_enabled_for_session: record.mr_enabled_for_session,
                    source: match record.source {
                        Stage5dRiskGateRowSource::Seed => {
                            crate::hybrid_intraday::RiskGateRowSource::Seed
                        }
                        Stage5dRiskGateRowSource::Runtime => {
                            crate::hybrid_intraday::RiskGateRowSource::Runtime
                        }
                    },
                    status: match record.status {
                        Stage5dRiskGateRowStatus::Complete => {
                            crate::hybrid_intraday::RiskGateRowStatus::Complete
                        }
                        Stage5dRiskGateRowStatus::Incomplete => {
                            crate::hybrid_intraday::RiskGateRowStatus::Incomplete
                        }
                    },
                },
                profile_id: evidence.identity.profile_id.clone(),
                mr_variant: evidence.identity.mr_variant.clone(),
                timeframe: evidence.identity.timeframe.clone(),
                session_policy: evidence.identity.session_policy.clone(),
                rolling_sum_lb120,
                mr_enabled_next_session: record.mr_enabled_next_session,
                model_version: evidence.identity.model_version.clone(),
                finalized_at_utc: record.finalized_at_utc,
            })
        })
        .collect()
}

fn stage5d_validate_source_exact_riskgate_records(
    evidence: &Stage5dRiskGateLedgerEvidence,
    persisted_at_ts_utc: Option<DateTime<Utc>>,
) -> Result<Vec<crate::hybrid_intraday::RiskGateLedgerRecord>, Stage5dRiskGateInjectionBlockReason>
{
    stage5d_require_source_canonical_riskgate_decimal(&evidence.current_shadow_pnl_points)?;
    for stage_record in &evidence.ledger_records {
        stage5d_validate_canonical_riskgate_decimal(&stage_record.shadow_pnl_points)?;
        stage5d_validate_canonical_riskgate_decimal(&stage_record.rolling_sum_before_session)?;
        stage5d_validate_canonical_riskgate_decimal(&stage_record.rolling_sum_lb120)?;
    }
    let records = stage5d_source_riskgate_records_from_evidence(evidence)?;
    let row_seed_loaded = records
        .iter()
        .any(|record| record.row.source == crate::hybrid_intraday::RiskGateRowSource::Seed);
    if evidence.seed_loaded != row_seed_loaded {
        return Err(Stage5dRiskGateInjectionBlockReason::LedgerRecordDerivedStateMismatch);
    }
    let mut accepted_rows = Vec::with_capacity(records.len());
    let mut previous_finalized_at_utc = None;
    let mut runtime_rows_started = false;

    for (stage_record, source_record) in evidence.ledger_records.iter().zip(&records) {
        if source_record.row.status != crate::hybrid_intraday::RiskGateRowStatus::Complete {
            return Err(Stage5dRiskGateInjectionBlockReason::LedgerRecordDerivedStateMismatch);
        }
        match source_record.row.source {
            crate::hybrid_intraday::RiskGateRowSource::Seed if runtime_rows_started => {
                return Err(Stage5dRiskGateInjectionBlockReason::LedgerRecordDerivedStateMismatch);
            }
            crate::hybrid_intraday::RiskGateRowSource::Runtime => runtime_rows_started = true,
            crate::hybrid_intraday::RiskGateRowSource::Seed => {}
        }

        let expected_before = crate::hybrid_intraday::risk_gate::rolling_shadow_pnl_before_session(
            &accepted_rows,
            source_record.row.session_date,
            crate::hybrid_intraday::SHADOW_PNL_LB120_LOOKBACK_SESSIONS,
            crate::hybrid_intraday::SHADOW_PNL_LB120_MIN_HISTORY_SESSIONS,
        )
        .unwrap_or(0.0);
        let expected_for_session = expected_before > 0.0;
        if !stage5d_source_f64_eq(
            source_record.row.rolling_sum_before_session,
            expected_before,
        ) || source_record.row.mr_enabled_for_session != expected_for_session
        {
            return Err(Stage5dRiskGateInjectionBlockReason::LedgerRecordDerivedStateMismatch);
        }

        if source_record.row.source == crate::hybrid_intraday::RiskGateRowSource::Runtime {
            let rebuilt_runtime_row = crate::hybrid_intraday::build_runtime_session_row(
                &accepted_rows,
                source_record.row.session_date,
                source_record.row.shadow_pnl_points,
                source_record.row.shadow_trade_count,
            )
            .map_err(|_| Stage5dRiskGateInjectionBlockReason::LedgerRecordDerivedStateMismatch)?;
            if rebuilt_runtime_row != source_record.row {
                return Err(Stage5dRiskGateInjectionBlockReason::LedgerRecordDerivedStateMismatch);
            }
        }

        accepted_rows.push(source_record.row.clone());
        crate::hybrid_intraday::validate_regular_session_ledger(&accepted_rows)
            .map_err(|_| Stage5dRiskGateInjectionBlockReason::LedgerRecordDerivedStateMismatch)?;
        let expected_after = crate::hybrid_intraday::rolling_shadow_pnl_before_next_session(
            &accepted_rows,
            crate::hybrid_intraday::SHADOW_PNL_LB120_LOOKBACK_SESSIONS,
            0,
        )
        .unwrap_or(0.0);
        let expected_next =
            crate::hybrid_intraday::mr_enabled_for_next_session(&accepted_rows).unwrap_or(false);
        if !stage5d_source_f64_eq(source_record.rolling_sum_lb120, expected_after)
            || source_record.mr_enabled_next_session != expected_next
        {
            return Err(Stage5dRiskGateInjectionBlockReason::LedgerRecordDerivedStateMismatch);
        }

        stage5d_validate_canonical_riskgate_decimal(&stage_record.shadow_pnl_points)?;
        stage5d_validate_canonical_riskgate_decimal(&stage_record.rolling_sum_before_session)?;
        stage5d_validate_canonical_riskgate_decimal(&stage_record.rolling_sum_lb120)?;
        let finalized = DateTime::<Utc>::from_timestamp(source_record.finalized_at_utc, 0)
            .filter(|_| (946_684_800..=4_102_444_800).contains(&source_record.finalized_at_utc))
            .ok_or(Stage5dRiskGateInjectionBlockReason::LedgerFinalizationTimestampInvalid)?;
        if finalized.date_naive() < source_record.row.session_date
            || previous_finalized_at_utc
                .is_some_and(|previous| source_record.finalized_at_utc < previous)
            || persisted_at_ts_utc.is_some_and(|persisted| finalized > persisted)
        {
            return Err(Stage5dRiskGateInjectionBlockReason::LedgerFinalizationTimestampInvalid);
        }
        previous_finalized_at_utc = Some(source_record.finalized_at_utc);
    }

    Ok(records)
}

fn stage5d_validate_canonical_riskgate_decimal(
    value: &str,
) -> Result<(), Stage5dRiskGateInjectionBlockReason> {
    if value.trim() != value
        || value.starts_with('+')
        || value.contains(['e', 'E'])
        || value.is_empty()
    {
        return Err(Stage5dRiskGateInjectionBlockReason::LedgerEvidenceInvalid);
    }
    stage5d_require_source_canonical_riskgate_decimal(value)
}

fn stage5d_require_source_canonical_riskgate_decimal(
    value: &str,
) -> Result<(), Stage5dRiskGateInjectionBlockReason> {
    crate::hybrid_intraday::parse_riskgate_authority_decimal(value)
        .map(|_| ())
        .map_err(|_| Stage5dRiskGateInjectionBlockReason::LedgerEvidenceInvalid)
}

#[cfg(test)]
fn stage5d_source_format_riskgate_decimal(value: f64) -> String {
    crate::hybrid_intraday::format_riskgate_authority_decimal(value)
        .expect("test/source riskgate decimal must be finite and non-negative-zero")
}

fn stage5d_source_f64_eq(left: f64, right: f64) -> bool {
    left == right
}

fn stage5d_is_negative_zero(value: f64) -> bool {
    value == 0.0 && value.is_sign_negative()
}

fn stage5d_is_source_zero(value: f64) -> bool {
    value == 0.0 && !value.is_sign_negative()
}

fn stage5d_validate_semantic_authority_f64(
    value: f64,
) -> Result<(), Stage5dRiskGateInjectionBlockReason> {
    if !value.is_finite() || stage5d_is_negative_zero(value) {
        return Err(Stage5dRiskGateInjectionBlockReason::MaterializedStateInvalid);
    }
    Ok(())
}

fn stage5d_compare_rebuilt_riskgate_materialized(
    persisted: &Stage5dRiskGateMaterializedState,
    rebuilt: &crate::hybrid_intraday::RiskGateMaterializedState,
) -> Result<(), Stage5dRiskGateInjectionBlockReason> {
    if let Some(value) = persisted.rolling_sum_lb120.as_deref() {
        stage5d_require_source_canonical_riskgate_decimal(value)
            .map_err(|_| Stage5dRiskGateInjectionBlockReason::MaterializedStateInvalid)?;
    }
    if persisted.current_shadow_pnl_points.is_empty() {
        return Err(Stage5dRiskGateInjectionBlockReason::MaterializedStateInvalid);
    }
    stage5d_require_source_canonical_riskgate_decimal(&persisted.current_shadow_pnl_points)
        .map_err(|_| Stage5dRiskGateInjectionBlockReason::MaterializedStateInvalid)?;
    let persisted_rolling = persisted
        .rolling_sum_lb120
        .as_deref()
        .map(parse_finite_decimal_string)
        .transpose()
        .map_err(|_| Stage5dRiskGateInjectionBlockReason::MaterializedStateInvalid)?;
    let persisted_last = persisted
        .last_finalized_session_date
        .as_deref()
        .map(|value| {
            NaiveDate::parse_from_str(value, "%Y-%m-%d")
                .map_err(|_| Stage5dRiskGateInjectionBlockReason::MaterializedStateInvalid)
        })
        .transpose()?;
    let persisted_current_shadow = persisted
        .current_shadow_session_date
        .as_deref()
        .map(|value| {
            NaiveDate::parse_from_str(value, "%Y-%m-%d")
                .map_err(|_| Stage5dRiskGateInjectionBlockReason::MaterializedStateInvalid)
        })
        .transpose()?;
    let persisted_shadow_pnl = crate::hybrid_intraday::parse_riskgate_authority_decimal(
        &persisted.current_shadow_pnl_points,
    )
    .map_err(|_| Stage5dRiskGateInjectionBlockReason::MaterializedStateInvalid)?;
    let ledger_rows_count = usize::try_from(persisted.ledger_rows_count)
        .map_err(|_| Stage5dRiskGateInjectionBlockReason::MaterializedStateInvalid)?;

    if persisted_last != rebuilt.last_finalized_session_date
        || !stage5d_optional_source_f64_eq(persisted_rolling, rebuilt.rolling_sum_lb120)
        || persisted.mr_enabled_current_session != rebuilt.mr_enabled_current_session
        || persisted.mr_enabled_next_session != rebuilt.mr_enabled_next_session
        || persisted.seed_loaded != rebuilt.seed_loaded
        || ledger_rows_count != rebuilt.ledger_rows_count
        || persisted_current_shadow != rebuilt.current_shadow_session_date
        || !stage5d_source_f64_eq(persisted_shadow_pnl, rebuilt.current_shadow_pnl_points)
        || persisted.current_generation != rebuilt.current_generation
    {
        return Err(Stage5dRiskGateInjectionBlockReason::MaterializedStateMismatch);
    }
    Ok(())
}

fn stage5d_compare_semantic_riskgate_materialized(
    state: &Stage5dHybridIntradayStrategyStateV1,
    rebuilt: &crate::hybrid_intraday::RiskGateMaterializedState,
) -> Result<(), Stage5dRiskGateInjectionBlockReason> {
    let semantic_last = state
        .risk_gate_last_finalized_session_date
        .as_deref()
        .map(|value| {
            NaiveDate::parse_from_str(value, "%Y-%m-%d")
                .map_err(|_| Stage5dRiskGateInjectionBlockReason::MaterializedStateInvalid)
        })
        .transpose()?;
    if let Some(value) = state.risk_gate_rolling_sum_lb120 {
        stage5d_validate_semantic_authority_f64(value)?;
    }
    if state.risk_gate_mr_enabled_current_session != rebuilt.mr_enabled_current_session
        || !stage5d_optional_source_f64_eq(
            state.risk_gate_rolling_sum_lb120,
            rebuilt.rolling_sum_lb120,
        )
        || semantic_last != rebuilt.last_finalized_session_date
        || state.risk_gate_ledger_rows_count != rebuilt.ledger_rows_count
    {
        return Err(Stage5dRiskGateInjectionBlockReason::MaterializedStateMismatch);
    }
    Ok(())
}

fn stage5d_compare_semantic_current_shadow_overlap(
    state: &Stage5dHybridIntradayStrategyStateV1,
    overlap: &Stage5dRiskGateCurrentShadowOverlap,
) -> Result<(), Stage5dRiskGateInjectionBlockReason> {
    let semantic_session = state
        .risk_gate_shadow_session_date
        .as_deref()
        .map(|value| {
            NaiveDate::parse_from_str(value, "%Y-%m-%d")
                .map_err(|_| Stage5dRiskGateInjectionBlockReason::MaterializedStateInvalid)
        })
        .transpose()?;
    stage5d_validate_semantic_authority_f64(state.risk_gate_shadow_pnl_points)?;
    if semantic_session != overlap.session_date
        || !stage5d_source_f64_eq(state.risk_gate_shadow_pnl_points, overlap.pnl_points)
    {
        return Err(Stage5dRiskGateInjectionBlockReason::MaterializedStateMismatch);
    }
    Ok(())
}

fn stage5d_validate_current_shadow_source_state(
    state: &Stage5dHybridIntradayStrategyStateV1,
    overlap: &Stage5dRiskGateCurrentShadowOverlap,
    finalized_rows: &[crate::hybrid_intraday::RiskGateLedgerRecord],
    envelope: &Stage5dPersistenceEnvelope,
    strategy: &crate::hybrid_intraday_runtime::HybridIntradayRuntimeStrategy,
) -> Result<(), Stage5dRiskGateInjectionBlockReason> {
    let open_tuple_present = state.risk_gate_shadow_entry_ts_utc.is_some()
        || state.risk_gate_shadow_entry_price.is_some()
        || state.risk_gate_shadow_side.is_some()
        || state.risk_gate_shadow_target_price.is_some()
        || state.risk_gate_shadow_stop_price.is_some();
    match overlap.session_date {
        None => {
            if !stage5d_is_source_zero(overlap.pnl_points)
                || state.risk_gate_shadow_trade_count != 0
                || open_tuple_present
            {
                return Err(Stage5dRiskGateInjectionBlockReason::MaterializedStateMismatch);
            }
            if envelope
                .runtime_private_extension
                .last_processed_bar_ts
                .map(|processed_bar_ts| {
                    strategy.stage5d_classify_processed_bar_ts(processed_bar_ts)
                        == Some(
                            crate::hybrid_intraday_runtime::Stage5dProcessedBarPolicy::RegularModelSession,
                        )
                })
                .unwrap_or(false)
            {
                return Err(Stage5dRiskGateInjectionBlockReason::MaterializedStateMismatch);
            }
        }
        Some(session_date) => {
            if strategy.stage5d_weekends_off()
                && matches!(session_date.weekday(), Weekday::Sat | Weekday::Sun)
            {
                return Err(Stage5dRiskGateInjectionBlockReason::MaterializedStateMismatch);
            }
            let last_day_local = state
                .last_day_local
                .as_deref()
                .ok_or(Stage5dRiskGateInjectionBlockReason::MaterializedStateMismatch)
                .and_then(|value| {
                    NaiveDate::parse_from_str(value, "%Y-%m-%d")
                        .map_err(|_| Stage5dRiskGateInjectionBlockReason::MaterializedStateMismatch)
                })?;
            if last_day_local != session_date {
                return Err(Stage5dRiskGateInjectionBlockReason::MaterializedStateMismatch);
            }
            if finalized_rows
                .last()
                .is_some_and(|record| session_date <= record.row.session_date)
            {
                return Err(Stage5dRiskGateInjectionBlockReason::MaterializedStateMismatch);
            }
            for pending in &envelope
                .runtime_private_extension
                .runtime_pending_finalizations
            {
                let pending_session = NaiveDate::parse_from_str(&pending.session_date, "%Y-%m-%d")
                    .map_err(|_| Stage5dRiskGateInjectionBlockReason::OutboxStateInconsistent)?;
                if session_date <= pending_session {
                    return Err(Stage5dRiskGateInjectionBlockReason::MaterializedStateMismatch);
                }
            }
            let offset = FixedOffset::east_opt(
                strategy
                    .stage5d_timezone_offset_hours()
                    .saturating_mul(3600),
            )
            .ok_or(Stage5dRiskGateInjectionBlockReason::MaterializedStateMismatch)?;
            let processed_bar_ts = envelope.runtime_private_extension.last_processed_bar_ts;
            if processed_bar_ts
                .map(|ts| ts.with_timezone(&offset).date_naive() < session_date)
                .unwrap_or(false)
                || envelope
                    .persisted_at_ts_utc
                    .with_timezone(&offset)
                    .date_naive()
                    < session_date
            {
                return Err(Stage5dRiskGateInjectionBlockReason::MaterializedStateMismatch);
            }
            if let Some(processed_bar_ts) = processed_bar_ts {
                let processed_date = processed_bar_ts.with_timezone(&offset).date_naive();
                let policy = strategy
                    .stage5d_classify_processed_bar_ts(processed_bar_ts)
                    .ok_or(Stage5dRiskGateInjectionBlockReason::MaterializedStateMismatch)?;
                if processed_date > session_date
                    && policy
                        == crate::hybrid_intraday_runtime::Stage5dProcessedBarPolicy::RegularModelSession
                {
                    return Err(Stage5dRiskGateInjectionBlockReason::MaterializedStateMismatch);
                }
            }
            if open_tuple_present {
                let processed_bar_ts = processed_bar_ts
                    .ok_or(Stage5dRiskGateInjectionBlockReason::MaterializedStateMismatch)?;
                let entry_ts = state
                    .risk_gate_shadow_entry_ts_utc
                    .ok_or(Stage5dRiskGateInjectionBlockReason::MaterializedStateMismatch)?;
                let entry_dt = DateTime::<Utc>::from_timestamp(entry_ts, 0)
                    .ok_or(Stage5dRiskGateInjectionBlockReason::MaterializedStateMismatch)?;
                if entry_dt.with_timezone(&offset).date_naive() != session_date
                    || entry_dt > processed_bar_ts
                    || entry_dt > envelope.persisted_at_ts_utc
                {
                    return Err(Stage5dRiskGateInjectionBlockReason::MaterializedStateMismatch);
                }
                let entry = state
                    .risk_gate_shadow_entry_price
                    .ok_or(Stage5dRiskGateInjectionBlockReason::MaterializedStateMismatch)?;
                let target = state
                    .risk_gate_shadow_target_price
                    .ok_or(Stage5dRiskGateInjectionBlockReason::MaterializedStateMismatch)?;
                let stop = state
                    .risk_gate_shadow_stop_price
                    .ok_or(Stage5dRiskGateInjectionBlockReason::MaterializedStateMismatch)?;
                if !entry.is_finite()
                    || !target.is_finite()
                    || !stop.is_finite()
                    || entry <= 0.0
                    || target <= 0.0
                    || stop <= 0.0
                {
                    return Err(Stage5dRiskGateInjectionBlockReason::MaterializedStateMismatch);
                }
                match state
                    .risk_gate_shadow_side
                    .ok_or(Stage5dRiskGateInjectionBlockReason::MaterializedStateMismatch)?
                {
                    Stage5dSide::Long if !(stop < entry && entry < target) => {
                        return Err(Stage5dRiskGateInjectionBlockReason::MaterializedStateMismatch);
                    }
                    Stage5dSide::Short if !(target < entry && entry < stop) => {
                        return Err(Stage5dRiskGateInjectionBlockReason::MaterializedStateMismatch);
                    }
                    Stage5dSide::Long | Stage5dSide::Short => {}
                }
            }
        }
    }
    if (!stage5d_is_source_zero(overlap.pnl_points)
        || state.risk_gate_shadow_trade_count > 0
        || open_tuple_present)
        && overlap.session_date.is_none()
    {
        return Err(Stage5dRiskGateInjectionBlockReason::MaterializedStateMismatch);
    }
    if !stage5d_is_source_zero(overlap.pnl_points) && state.risk_gate_shadow_trade_count == 0 {
        return Err(Stage5dRiskGateInjectionBlockReason::MaterializedStateMismatch);
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stage5dRiskGateRecoveryDecision {
    AppendMissingLedgerRow,
    AdvanceToMaterialized,
    ReackRuntime,
    AlreadyAcknowledged,
}

impl Stage5dRiskGateRecoveryDecision {
    fn as_str(self) -> &'static str {
        match self {
            Self::AppendMissingLedgerRow => "append_missing_ledger_row",
            Self::AdvanceToMaterialized => "advance_to_materialized",
            Self::ReackRuntime => "reack_runtime",
            Self::AlreadyAcknowledged => "already_acknowledged",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Stage5dRiskGateRecoveryPlanDecision {
    session_date: NaiveDate,
    generation: u64,
    durable_state: Stage5dRiskGateFinalizationState,
    action: Stage5dRiskGateRecoveryDecision,
}

#[derive(Debug, Clone)]
struct Stage5dRiskGateAuthoritativeProjection {
    external_ledger_records_count: usize,
    recovery_target_records: Vec<crate::hybrid_intraday::RiskGateLedgerRecord>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Stage5dRiskGateLocalProjectionFrontier {
    outbox_tail_start: usize,
    ledger_records_count: usize,
    materialized_records_count: usize,
    runtime_records_count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Stage5dRiskGateCurrentShadowOverlap {
    session_date: Option<NaiveDate>,
    pnl_points: f64,
}

#[derive(Debug, Clone)]
struct Stage5dRiskGateCrashFrontierAnalysis {
    authoritative: Stage5dRiskGateAuthoritativeProjection,
    local: Stage5dRiskGateLocalProjectionFrontier,
    decisions: Vec<Stage5dRiskGateRecoveryPlanDecision>,
    materialized_projection: crate::hybrid_intraday::RiskGateMaterializedState,
    runtime_projection: crate::hybrid_intraday::RiskGateMaterializedState,
    current_shadow_overlap: Stage5dRiskGateCurrentShadowOverlap,
}

#[derive(Debug, Clone)]
struct Stage5dRiskGateRecoveryPlan {
    envelope_fingerprint_sha256: String,
    ledger_evidence_fingerprint_sha256: String,
    identity_fingerprint_sha256: String,
    source_generation: String,
    local: Stage5dRiskGateLocalProjectionFrontier,
    recovery_target_records_count: usize,
    decisions: Vec<Stage5dRiskGateRecoveryPlanDecision>,
    recovery_complete: bool,
    plan_fingerprint_sha256: String,
}

fn stage5d_analyze_riskgate_crash_frontiers(
    envelope: &Stage5dPersistenceEnvelope,
    evidence: &Stage5dRiskGateLedgerEvidence,
    source_records: &[crate::hybrid_intraday::RiskGateLedgerRecord],
    source_identity: &crate::hybrid_intraday::RiskGateProfileIdentity,
    current_shadow_session_date: Option<NaiveDate>,
    current_shadow_pnl_points: f64,
) -> Result<Stage5dRiskGateCrashFrontierAnalysis, Stage5dRiskGateInjectionBlockReason> {
    let ledger_by_session: HashMap<NaiveDate, &crate::hybrid_intraday::RiskGateLedgerRecord> =
        source_records
            .iter()
            .map(|record| (record.row.session_date, record))
            .collect();
    let pending_by_session: HashMap<NaiveDate, &Stage5dRuntimePendingRiskGateFinalization> =
        envelope
            .runtime_private_extension
            .runtime_pending_finalizations
            .iter()
            .map(|pending| {
                NaiveDate::parse_from_str(&pending.session_date, "%Y-%m-%d")
                    .map(|date| (date, pending))
                    .map_err(|_| Stage5dRiskGateInjectionBlockReason::OutboxStateInconsistent)
            })
            .collect::<Result<_, _>>()?;
    if pending_by_session.len()
        != envelope
            .runtime_private_extension
            .runtime_pending_finalizations
            .len()
    {
        return Err(Stage5dRiskGateInjectionBlockReason::OutboxStateInconsistent);
    }

    let mut outbox_sessions = HashSet::new();
    let mut outbox_identity_hashes = HashSet::new();
    let mut previous: Option<(NaiveDate, u64, u8)> = None;
    let mut target_records = source_records.to_vec();
    let mut outbox_dates = Vec::with_capacity(envelope.riskgate.durable_finalization_outbox.len());
    let mut ledger_presence =
        Vec::with_capacity(envelope.riskgate.durable_finalization_outbox.len());
    for record in &envelope.riskgate.durable_finalization_outbox {
        let session_date = NaiveDate::parse_from_str(&record.session_date, "%Y-%m-%d")
            .map_err(|_| Stage5dRiskGateInjectionBlockReason::OutboxStateInconsistent)?;
        if !outbox_sessions.insert(session_date) {
            return Err(Stage5dRiskGateInjectionBlockReason::DuplicateOutboxSession);
        }
        if record.generation == 0 {
            return Err(Stage5dRiskGateInjectionBlockReason::OutboxStateInconsistent);
        }
        let state_rank = stage5d_riskgate_outbox_state_rank(record.state);
        if previous.is_some_and(
            |(previous_date, previous_generation, previous_state_rank)| {
                session_date <= previous_date
                    || record.generation <= previous_generation
                    || state_rank > previous_state_rank
            },
        ) {
            return Err(Stage5dRiskGateInjectionBlockReason::OutboxStateInconsistent);
        }
        previous = Some((session_date, record.generation, state_rank));
        if !outbox_identity_hashes.insert(record.identity_hash.as_str()) {
            return Err(Stage5dRiskGateInjectionBlockReason::DuplicateOutboxIdentity);
        }
        if record.identity_hash
            != stage5d_riskgate_outbox_identity_hash(
                &evidence.identity,
                session_date,
                record.generation,
            )
        {
            return Err(Stage5dRiskGateInjectionBlockReason::OutboxIdentityMismatch);
        }

        let pending = pending_by_session.get(&session_date).copied();
        let ledger = ledger_by_session.get(&session_date).copied();
        match record.state {
            Stage5dRiskGateFinalizationState::Prepared => {
                let Some(pending) = pending else {
                    return Err(Stage5dRiskGateInjectionBlockReason::OutboxStateInconsistent);
                };
                if let Some(ledger) = ledger {
                    stage5d_pending_matches_ledger(pending, ledger)?;
                } else {
                    stage5d_require_source_canonical_riskgate_decimal(&pending.shadow_pnl_points)?;
                    let rows = crate::hybrid_intraday::rows_from_ledger_records(&target_records)
                        .map_err(|_| {
                            Stage5dRiskGateInjectionBlockReason::OutboxStateInconsistent
                        })?;
                    let row = crate::hybrid_intraday::build_runtime_session_row(
                        &rows,
                        session_date,
                        parse_finite_decimal_string(&pending.shadow_pnl_points).map_err(|_| {
                            Stage5dRiskGateInjectionBlockReason::OutboxStateInconsistent
                        })?,
                        pending.shadow_trade_count,
                    )
                    .map_err(|_| Stage5dRiskGateInjectionBlockReason::OutboxStateInconsistent)?;
                    let mut target_rows = rows;
                    target_rows.push(row);
                    let synthesized = crate::hybrid_intraday::build_ledger_records_from_rows(
                        &target_rows,
                        source_identity,
                        envelope.persisted_at_ts_utc.timestamp(),
                    )
                    .map_err(|_| Stage5dRiskGateInjectionBlockReason::OutboxStateInconsistent)?
                    .pop()
                    .ok_or(Stage5dRiskGateInjectionBlockReason::OutboxStateInconsistent)?;
                    target_records.push(synthesized);
                }
            }
            Stage5dRiskGateFinalizationState::LedgerAppended => {
                let Some(ledger) = ledger else {
                    return Err(Stage5dRiskGateInjectionBlockReason::OutboxStateInconsistent);
                };
                if let Some(pending) = pending {
                    stage5d_pending_matches_ledger(pending, ledger)?;
                }
            }
            Stage5dRiskGateFinalizationState::MaterializedUpdated => {
                let Some(ledger) = ledger else {
                    return Err(Stage5dRiskGateInjectionBlockReason::OutboxStateInconsistent);
                };
                if let Some(pending) = pending {
                    stage5d_pending_matches_ledger(pending, ledger)?;
                }
            }
            Stage5dRiskGateFinalizationState::AcknowledgedInRuntime => {
                if pending.is_some() || ledger.is_none() {
                    return Err(Stage5dRiskGateInjectionBlockReason::OutboxStateInconsistent);
                }
            }
        }
        outbox_dates.push(session_date);
        ledger_presence.push(ledger.is_some());
    }
    for session_date in pending_by_session.keys() {
        if !outbox_sessions.contains(session_date) {
            return Err(
                Stage5dRiskGateInjectionBlockReason::RuntimePendingFinalizationMissingFromOutbox,
            );
        }
    }

    let outbox_indexes = outbox_dates
        .iter()
        .map(|session_date| {
            target_records
                .iter()
                .position(|record| record.row.session_date == *session_date)
                .ok_or(Stage5dRiskGateInjectionBlockReason::OutboxStateInconsistent)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let outbox_tail_start = outbox_indexes
        .first()
        .copied()
        .unwrap_or(target_records.len());
    if outbox_indexes
        .iter()
        .enumerate()
        .any(|(offset, index)| *index != outbox_tail_start + offset)
        || outbox_tail_start + outbox_indexes.len() != target_records.len()
    {
        return Err(Stage5dRiskGateInjectionBlockReason::OutboxStateInconsistent);
    }

    let external_ledger_records_count = source_records.len();
    let materialized_records_count = stage5d_find_materialized_projection_frontier(
        &envelope.riskgate.materialized_state,
        &target_records,
        current_shadow_session_date,
        current_shadow_pnl_points,
        outbox_tail_start,
        external_ledger_records_count,
    )?;
    let semantic: Stage5dSemanticStrategyStateV1 =
        serde_json::from_value(envelope.strategy_state.strategy_state_json.clone())
            .map_err(|_| Stage5dRiskGateInjectionBlockReason::MaterializedStateInvalid)?;
    let Stage5dSemanticStrategyStateV1::HybridIntradayRuntime(semantic_state) = semantic;
    let runtime_records_count = stage5d_find_runtime_projection_frontier(
        &semantic_state,
        &target_records,
        current_shadow_session_date,
        current_shadow_pnl_points,
        outbox_tail_start,
        materialized_records_count,
    )?;
    if runtime_records_count > materialized_records_count
        || materialized_records_count > external_ledger_records_count
    {
        return Err(Stage5dRiskGateInjectionBlockReason::OutboxStateInconsistent);
    }

    let mut decisions = Vec::with_capacity(outbox_dates.len());
    for (offset, record) in envelope
        .riskgate
        .durable_finalization_outbox
        .iter()
        .enumerate()
    {
        let session_date = outbox_dates[offset];
        let target_index = outbox_tail_start + offset;
        let ledger_includes = ledger_presence[offset];
        let materialized_includes = target_index < materialized_records_count;
        let runtime_includes = target_index < runtime_records_count;
        let pending = pending_by_session.get(&session_date).copied();
        if runtime_includes && !materialized_includes || materialized_includes && !ledger_includes {
            return Err(Stage5dRiskGateInjectionBlockReason::OutboxStateInconsistent);
        }
        if runtime_includes {
            if pending.is_some() {
                return Err(Stage5dRiskGateInjectionBlockReason::OutboxStateInconsistent);
            }
        } else if pending.is_none() {
            return Err(Stage5dRiskGateInjectionBlockReason::OutboxStateInconsistent);
        }
        let action = match record.state {
            Stage5dRiskGateFinalizationState::Prepared => {
                if runtime_includes {
                    return Err(Stage5dRiskGateInjectionBlockReason::OutboxStateInconsistent);
                }
                if !ledger_includes {
                    if materialized_includes {
                        return Err(Stage5dRiskGateInjectionBlockReason::OutboxStateInconsistent);
                    }
                    Stage5dRiskGateRecoveryDecision::AppendMissingLedgerRow
                } else {
                    // Preserve the durable Prepared -> materialized transition
                    // even when an exact later projection survived the crash.
                    Stage5dRiskGateRecoveryDecision::AdvanceToMaterialized
                }
            }
            Stage5dRiskGateFinalizationState::LedgerAppended => {
                if !ledger_includes {
                    return Err(Stage5dRiskGateInjectionBlockReason::OutboxStateInconsistent);
                }
                // Materialization/marker advancement remains idempotent; a
                // lagging LedgerAppended marker cannot skip this transition.
                Stage5dRiskGateRecoveryDecision::AdvanceToMaterialized
            }
            Stage5dRiskGateFinalizationState::MaterializedUpdated => {
                if !ledger_includes || !materialized_includes {
                    return Err(Stage5dRiskGateInjectionBlockReason::OutboxStateInconsistent);
                }
                // Runtime acknowledgement/marker advancement remains
                // idempotent at this durable state.
                Stage5dRiskGateRecoveryDecision::ReackRuntime
            }
            Stage5dRiskGateFinalizationState::AcknowledgedInRuntime => {
                if !ledger_includes
                    || !materialized_includes
                    || !runtime_includes
                    || pending.is_some()
                {
                    return Err(Stage5dRiskGateInjectionBlockReason::OutboxStateInconsistent);
                }
                Stage5dRiskGateRecoveryDecision::AlreadyAcknowledged
            }
        };
        decisions.push(Stage5dRiskGateRecoveryPlanDecision {
            session_date,
            generation: record.generation,
            durable_state: record.state,
            action,
        });
    }

    let materialized_projection = stage5d_rebuild_riskgate_projection_prefix(
        &target_records,
        materialized_records_count,
        current_shadow_session_date,
        current_shadow_pnl_points,
    )?;
    let runtime_projection = stage5d_rebuild_riskgate_projection_prefix(
        &target_records,
        runtime_records_count,
        current_shadow_session_date,
        current_shadow_pnl_points,
    )?;
    Ok(Stage5dRiskGateCrashFrontierAnalysis {
        authoritative: Stage5dRiskGateAuthoritativeProjection {
            external_ledger_records_count,
            recovery_target_records: target_records,
        },
        local: Stage5dRiskGateLocalProjectionFrontier {
            outbox_tail_start,
            ledger_records_count: external_ledger_records_count,
            materialized_records_count,
            runtime_records_count,
        },
        decisions,
        materialized_projection,
        runtime_projection,
        current_shadow_overlap: Stage5dRiskGateCurrentShadowOverlap {
            session_date: current_shadow_session_date,
            pnl_points: current_shadow_pnl_points,
        },
    })
}

fn stage5d_find_materialized_projection_frontier(
    persisted: &Stage5dRiskGateMaterializedState,
    target_records: &[crate::hybrid_intraday::RiskGateLedgerRecord],
    current_shadow_session_date: Option<NaiveDate>,
    current_shadow_pnl_points: f64,
    minimum_records_count: usize,
    maximum_records_count: usize,
) -> Result<usize, Stage5dRiskGateInjectionBlockReason> {
    let mut matching = Vec::new();
    for records_count in minimum_records_count..=maximum_records_count {
        let candidate = stage5d_rebuild_riskgate_projection_prefix(
            target_records,
            records_count,
            current_shadow_session_date,
            current_shadow_pnl_points,
        )?;
        match stage5d_compare_rebuilt_riskgate_materialized(persisted, &candidate) {
            Ok(()) => matching.push(records_count),
            Err(Stage5dRiskGateInjectionBlockReason::MaterializedStateMismatch) => {}
            Err(reason) => return Err(reason),
        }
    }
    match matching.as_slice() {
        [records_count] => Ok(*records_count),
        _ => Err(Stage5dRiskGateInjectionBlockReason::MaterializedStateMismatch),
    }
}

fn stage5d_find_runtime_projection_frontier(
    semantic: &Stage5dHybridIntradayStrategyStateV1,
    target_records: &[crate::hybrid_intraday::RiskGateLedgerRecord],
    current_shadow_session_date: Option<NaiveDate>,
    current_shadow_pnl_points: f64,
    minimum_records_count: usize,
    maximum_records_count: usize,
) -> Result<usize, Stage5dRiskGateInjectionBlockReason> {
    let mut matching = Vec::new();
    for records_count in minimum_records_count..=maximum_records_count {
        let candidate = stage5d_rebuild_riskgate_projection_prefix(
            target_records,
            records_count,
            current_shadow_session_date,
            current_shadow_pnl_points,
        )?;
        match stage5d_compare_semantic_riskgate_materialized(semantic, &candidate) {
            Ok(()) => matching.push(records_count),
            Err(Stage5dRiskGateInjectionBlockReason::MaterializedStateMismatch) => {}
            Err(reason) => return Err(reason),
        }
    }
    match matching.as_slice() {
        [records_count] => Ok(*records_count),
        _ => Err(Stage5dRiskGateInjectionBlockReason::MaterializedStateMismatch),
    }
}

fn stage5d_rebuild_riskgate_projection_prefix(
    target_records: &[crate::hybrid_intraday::RiskGateLedgerRecord],
    records_count: usize,
    current_shadow_session_date: Option<NaiveDate>,
    current_shadow_pnl_points: f64,
) -> Result<crate::hybrid_intraday::RiskGateMaterializedState, Stage5dRiskGateInjectionBlockReason>
{
    let records = target_records
        .get(..records_count)
        .ok_or(Stage5dRiskGateInjectionBlockReason::OutboxStateInconsistent)?;
    let seed_loaded = records
        .iter()
        .any(|record| record.row.source == crate::hybrid_intraday::RiskGateRowSource::Seed);
    crate::hybrid_intraday::rebuild_materialized_state_from_ledger_records(
        records,
        current_shadow_session_date,
        current_shadow_pnl_points,
        seed_loaded,
    )
    .map_err(|_| Stage5dRiskGateInjectionBlockReason::MaterializedStateInvalid)
}

fn stage5d_build_riskgate_recovery_plan(
    envelope: &Stage5dPersistenceEnvelope,
    validated_evidence: &Stage5dValidatedRiskGateLedgerEvidence,
    analysis: &Stage5dRiskGateCrashFrontierAnalysis,
) -> Result<Stage5dRiskGateRecoveryPlan, Stage5dRiskGateInjectionBlockReason> {
    let identity_fingerprint_sha256 =
        stage5d_compute_riskgate_identity_fingerprint(&validated_evidence.evidence.identity)?;
    let recovery_complete = analysis.decisions.iter().all(|decision| {
        decision.durable_state == Stage5dRiskGateFinalizationState::AcknowledgedInRuntime
            && decision.action == Stage5dRiskGateRecoveryDecision::AlreadyAcknowledged
    });
    let mut plan = Stage5dRiskGateRecoveryPlan {
        envelope_fingerprint_sha256: envelope.payload_checksum_sha256.clone(),
        ledger_evidence_fingerprint_sha256: validated_evidence.evidence_fingerprint_sha256.clone(),
        identity_fingerprint_sha256,
        source_generation: validated_evidence.evidence.current_generation.clone(),
        local: analysis.local,
        recovery_target_records_count: analysis.authoritative.recovery_target_records.len(),
        decisions: analysis.decisions.clone(),
        recovery_complete,
        plan_fingerprint_sha256: String::new(),
    };
    if plan.local.ledger_records_count != analysis.authoritative.external_ledger_records_count {
        return Err(Stage5dRiskGateInjectionBlockReason::RecoveryPlanBindingMismatch);
    }
    plan.plan_fingerprint_sha256 = stage5d_compute_riskgate_recovery_plan_fingerprint(&plan)?;
    Ok(plan)
}

fn stage5d_validate_current_riskgate_recovery_decisions(
    analysis: &Stage5dRiskGateCrashFrontierAnalysis,
) -> Result<(), Stage5dRiskGateInjectionBlockReason> {
    for decision in &analysis.decisions {
        let target_index = analysis
            .authoritative
            .recovery_target_records
            .iter()
            .position(|record| record.row.session_date == decision.session_date)
            .ok_or(Stage5dRiskGateInjectionBlockReason::RecoveryPlanBindingMismatch)?;
        let ledger_includes = target_index < analysis.local.ledger_records_count;
        let materialized_includes = target_index < analysis.local.materialized_records_count;
        let runtime_includes = target_index < analysis.local.runtime_records_count;
        match decision.action {
            Stage5dRiskGateRecoveryDecision::AppendMissingLedgerRow => {
                if ledger_includes || materialized_includes || runtime_includes {
                    return Err(Stage5dRiskGateInjectionBlockReason::RecoveryPlanBindingMismatch);
                }
            }
            Stage5dRiskGateRecoveryDecision::AdvanceToMaterialized => {
                if !ledger_includes || runtime_includes {
                    return Err(Stage5dRiskGateInjectionBlockReason::RecoveryPlanBindingMismatch);
                }
            }
            Stage5dRiskGateRecoveryDecision::ReackRuntime => {
                if !ledger_includes || !materialized_includes || runtime_includes {
                    return Err(Stage5dRiskGateInjectionBlockReason::RecoveryPlanBindingMismatch);
                }
            }
            Stage5dRiskGateRecoveryDecision::AlreadyAcknowledged => {
                if !ledger_includes || !materialized_includes || !runtime_includes {
                    return Err(Stage5dRiskGateInjectionBlockReason::RecoveryPlanBindingMismatch);
                }
            }
        }
    }
    Ok(())
}

fn stage5d_compute_riskgate_identity_fingerprint(
    identity: &Stage5dRiskGateIdentity,
) -> Result<String, Stage5dRiskGateInjectionBlockReason> {
    let payload = serde_json::to_vec(&serde_json::json!({
        "schema": "stage5d_riskgate_identity_fingerprint_v1",
        "identity": identity,
    }))
    .map_err(|_| Stage5dRiskGateInjectionBlockReason::RecoveryPlanBindingMismatch)?;
    Ok(format!(
        "stage5d_riskgate_identity_sha256:{:x}",
        Sha256::digest(payload)
    ))
}

fn stage5d_compute_riskgate_recovery_plan_fingerprint(
    plan: &Stage5dRiskGateRecoveryPlan,
) -> Result<String, Stage5dRiskGateInjectionBlockReason> {
    let decisions = plan
        .decisions
        .iter()
        .map(|decision| {
            serde_json::json!({
                "session_date": decision.session_date.format("%Y-%m-%d").to_string(),
                "generation": decision.generation,
                "durable_state": stage5d_riskgate_outbox_state_name(decision.durable_state),
                "action": decision.action.as_str(),
            })
        })
        .collect::<Vec<_>>();
    let payload = serde_json::to_vec(&serde_json::json!({
        "schema": "stage5d_riskgate_recovery_plan_v1",
        "envelope_fingerprint_sha256": plan.envelope_fingerprint_sha256,
        "ledger_evidence_fingerprint_sha256": plan.ledger_evidence_fingerprint_sha256,
        "identity_fingerprint_sha256": plan.identity_fingerprint_sha256,
        "source_generation": plan.source_generation,
        "outbox_tail_start": plan.local.outbox_tail_start,
        "ledger_records_count": plan.local.ledger_records_count,
        "materialized_records_count": plan.local.materialized_records_count,
        "runtime_records_count": plan.local.runtime_records_count,
        "recovery_target_records_count": plan.recovery_target_records_count,
        "decisions": decisions,
        "recovery_complete": plan.recovery_complete,
    }))
    .map_err(|_| Stage5dRiskGateInjectionBlockReason::RecoveryPlanBindingMismatch)?;
    Ok(format!(
        "stage5d_riskgate_recovery_plan_sha256:{:x}",
        Sha256::digest(payload)
    ))
}

fn stage5d_validate_riskgate_recovery_plan_binding(
    plan: &Stage5dRiskGateRecoveryPlan,
    envelope: &Stage5dPersistenceEnvelope,
    validated_evidence: &Stage5dValidatedRiskGateLedgerEvidence,
) -> Result<(), Stage5dRiskGateInjectionBlockReason> {
    let expected_identity =
        stage5d_compute_riskgate_identity_fingerprint(&validated_evidence.evidence.identity)?;
    let expected_plan = stage5d_compute_riskgate_recovery_plan_fingerprint(plan)?;
    if plan.envelope_fingerprint_sha256 != envelope.payload_checksum_sha256
        || plan.ledger_evidence_fingerprint_sha256 != validated_evidence.evidence_fingerprint_sha256
        || plan.identity_fingerprint_sha256 != expected_identity
        || plan.source_generation != validated_evidence.evidence.current_generation
        || plan.local.ledger_records_count != validated_evidence.evidence.ledger_records.len()
        || plan.plan_fingerprint_sha256 != expected_plan
    {
        return Err(Stage5dRiskGateInjectionBlockReason::RecoveryPlanBindingMismatch);
    }
    Ok(())
}

fn stage5d_riskgate_outbox_state_rank(state: Stage5dRiskGateFinalizationState) -> u8 {
    match state {
        Stage5dRiskGateFinalizationState::Prepared => 0,
        Stage5dRiskGateFinalizationState::LedgerAppended => 1,
        Stage5dRiskGateFinalizationState::MaterializedUpdated => 2,
        Stage5dRiskGateFinalizationState::AcknowledgedInRuntime => 3,
    }
}

fn stage5d_riskgate_outbox_state_name(state: Stage5dRiskGateFinalizationState) -> &'static str {
    match state {
        Stage5dRiskGateFinalizationState::Prepared => "prepared",
        Stage5dRiskGateFinalizationState::LedgerAppended => "ledger_appended",
        Stage5dRiskGateFinalizationState::MaterializedUpdated => "materialized_updated",
        Stage5dRiskGateFinalizationState::AcknowledgedInRuntime => "acknowledged_in_runtime",
    }
}

fn stage5d_pending_matches_ledger(
    pending: &Stage5dRuntimePendingRiskGateFinalization,
    ledger: &crate::hybrid_intraday::RiskGateLedgerRecord,
) -> Result<(), Stage5dRiskGateInjectionBlockReason> {
    stage5d_require_source_canonical_riskgate_decimal(&pending.shadow_pnl_points)
        .map_err(|_| Stage5dRiskGateInjectionBlockReason::OutboxStateInconsistent)?;
    let pending_pnl = parse_finite_decimal_string(&pending.shadow_pnl_points)
        .map_err(|_| Stage5dRiskGateInjectionBlockReason::OutboxStateInconsistent)?;
    if !stage5d_source_f64_eq(pending_pnl, ledger.row.shadow_pnl_points)
        || pending.shadow_trade_count != ledger.row.shadow_trade_count
    {
        return Err(Stage5dRiskGateInjectionBlockReason::OutboxStateInconsistent);
    }
    Ok(())
}

fn stage5d_optional_source_f64_eq(left: Option<f64>, right: Option<f64>) -> bool {
    match (left, right) {
        (None, None) => true,
        (Some(left), Some(right)) => stage5d_source_f64_eq(left, right),
        _ => false,
    }
}

fn stage5d_compute_riskgate_ledger_tail_hash(
    evidence: &Stage5dRiskGateLedgerEvidence,
) -> Result<String, serde_json::Error> {
    let payload = serde_json::to_vec(&serde_json::json!({
        "schema": "stage5d_riskgate_ledger_tail_v1",
        "identity": evidence.identity,
        "records": evidence.ledger_records,
    }))?;
    Ok(format!(
        "stage5d_riskgate_ledger_tail_sha256:{:x}",
        Sha256::digest(payload)
    ))
}

fn stage5d_compute_riskgate_evidence_fingerprint(
    evidence: &Stage5dRiskGateLedgerEvidence,
) -> Result<String, serde_json::Error> {
    let payload = serde_json::to_vec(&serde_json::json!({
        "schema": "stage5d_riskgate_evidence_fingerprint_v1",
        "ledger_tail_hash": evidence.ledger_tail_hash,
        "seed_loaded": evidence.seed_loaded,
        "current_shadow_session_date": evidence.current_shadow_session_date,
        "current_shadow_pnl_points": evidence.current_shadow_pnl_points,
        "current_generation": evidence.current_generation,
    }))?;
    Ok(format!(
        "stage5d_riskgate_evidence_sha256:{:x}",
        Sha256::digest(payload)
    ))
}

fn stage5d_riskgate_outbox_identity_hash(
    identity: &Stage5dRiskGateIdentity,
    session_date: NaiveDate,
    generation: u64,
) -> String {
    let payload = serde_json::to_vec(&serde_json::json!({
        "schema": "stage5d_riskgate_outbox_identity_v1",
        "strategy_id": identity.strategy_id,
        "profile_id": identity.profile_id,
        "mr_variant": identity.mr_variant,
        "timeframe": identity.timeframe,
        "session_policy": identity.session_policy,
        "model_version": identity.model_version,
        "session_date": session_date.format("%Y-%m-%d").to_string(),
        "generation": generation,
    }))
    .expect("riskgate outbox identity payload must serialize");
    format!(
        "stage5d_riskgate_outbox_sha256:{:x}",
        Sha256::digest(payload)
    )
}

fn stage5d_notify_broker_truth_bootstrap_at(
    applied: Stage5dPrivateStateAppliedPaperStrategy,
    notification_now: DateTime<Utc>,
) -> Result<Stage5dBootstrappedPaperStrategy, Stage5dBootstrapBlocked> {
    let Stage5dPrivateStateAppliedPaperStrategy { loaded, envelope } = applied;
    if let Err(reason) = validate_stage5d_broker_truth_bootstrap(&loaded, &envelope) {
        return Err(stage5d_bootstrap_blocked(loaded, envelope, reason));
    }

    match crate::stage5c_paper_host::stage5d_bootstrap_preserving_loaded_at(
        loaded,
        notification_now,
    ) {
        Ok(bootstrapped) => Ok(Stage5dBootstrappedPaperStrategy {
            bootstrapped,
            envelope,
        }),
        Err(blocked) => {
            let (loaded, error) = *blocked;
            Err(stage5d_bootstrap_blocked(
                loaded,
                envelope,
                Stage5dBootstrapBlockReason::from(error),
            ))
        }
    }
}

fn stage5d_fresh_admission_matches_applied_state(
    strategy: &crate::hybrid_intraday_runtime::HybridIntradayRuntimeStrategy,
    previous_admission: &crate::stage5c_paper_host::Stage5cPaperHostAdmission,
    fresh_admission: &crate::stage5c_paper_host::Stage5cPaperHostAdmission,
    envelope: &Stage5dPersistenceEnvelope,
    notification_now: DateTime<Utc>,
) -> bool {
    let (symbol_matches, tick_size_matches) = strategy.stage5c_binding_matches(
        fresh_admission.target_instrument(),
        fresh_admission.tick_size(),
    );
    symbol_matches
        && tick_size_matches
        && fresh_admission.strategy_id() == previous_admission.strategy_id()
        && fresh_admission.strategy_id() == envelope.binding.strategy_id
        && fresh_admission.account_id() == previous_admission.account_id()
        && fresh_admission.account_id() == &envelope.binding.account_id
        && fresh_admission.target_instrument() == previous_admission.target_instrument()
        && fresh_admission.target_instrument() == &envelope.binding.instrument_id.to_instrument_id()
        && fresh_admission.tick_size() == previous_admission.tick_size()
        && fresh_admission.checked_ts() >= previous_admission.checked_ts()
        && fresh_admission.issued_ts() >= previous_admission.issued_ts()
        && notification_now <= fresh_admission.expires_at()
}

fn stage5d_bootstrap_blocked(
    loaded: crate::stage5c_paper_host::Stage5cRuntimeStateLoadedPaperStrategy,
    envelope: Stage5dPersistenceEnvelope,
    reason: Stage5dBootstrapBlockReason,
) -> Stage5dBootstrapBlocked {
    Stage5dBootstrapBlocked {
        applied: Box::new(Stage5dPrivateStateAppliedPaperStrategy { loaded, envelope }),
        reason,
    }
}

fn validate_stage5d_broker_truth_bootstrap(
    loaded: &crate::stage5c_paper_host::Stage5cRuntimeStateLoadedPaperStrategy,
    envelope: &Stage5dPersistenceEnvelope,
) -> Result<(), Stage5dBootstrapBlockReason> {
    let admission = loaded.stage5d_admission();
    let snapshot = admission.bootstrap_snapshot();
    if admission.strategy_id() != envelope.binding.strategy_id
        || admission.account_id() != &envelope.binding.account_id
        || admission.target_instrument() != &envelope.binding.instrument_id.to_instrument_id()
        || snapshot.account_id != envelope.binding.account_id
        || snapshot.instrument != envelope.binding.instrument_id.to_instrument_id()
    {
        return Err(Stage5dBootstrapBlockReason::BindingMismatch);
    }

    let persisted_qty = stage5d_persisted_position_qty(envelope)?;
    let broker_qty = snapshot
        .target_position_qty
        .to_f64()
        .filter(|value| value.is_finite())
        .ok_or(Stage5dBootstrapBlockReason::PositionQuantityNotRepresentable)?;
    if (persisted_qty - broker_qty).abs() > f64::EPSILON {
        return Err(Stage5dBootstrapBlockReason::BrokerTruthPositionMismatch);
    }

    let broker_active_order_ids: HashSet<_> = snapshot
        .target_active_orders
        .iter()
        .filter_map(|order| order.broker_order_id.as_ref())
        .collect();
    for expected in &envelope
        .runtime_private_extension
        .expected_working_sets
        .expected_working_order_ids
    {
        if !broker_active_order_ids.contains(expected) {
            return Err(Stage5dBootstrapBlockReason::ExpectedWorkingOrderMissing);
        }
    }
    if !envelope
        .runtime_private_extension
        .expected_working_sets
        .expected_working_stop_order_ids
        .is_empty()
    {
        return Err(Stage5dBootstrapBlockReason::ExpectedWorkingStopUnsupported);
    }

    Ok(())
}

fn stage5d_persisted_position_qty(
    envelope: &Stage5dPersistenceEnvelope,
) -> Result<f64, Stage5dBootstrapBlockReason> {
    let semantic: Stage5dSemanticStrategyStateV1 =
        serde_json::from_value(envelope.strategy_state.strategy_state_json.clone())
            .map_err(|_| Stage5dBootstrapBlockReason::SemanticStateInvalid)?;
    let Stage5dSemanticStrategyStateV1::HybridIntradayRuntime(state) = semantic;
    if !state.last_position_qty.is_finite() {
        return Err(Stage5dBootstrapBlockReason::SemanticStateInvalid);
    }
    Ok(state.last_position_qty)
}

impl From<crate::stage5c_paper_host::Stage5cBootstrapNotificationError>
    for Stage5dBootstrapBlockReason
{
    fn from(error: crate::stage5c_paper_host::Stage5cBootstrapNotificationError) -> Self {
        match error {
            crate::stage5c_paper_host::Stage5cBootstrapNotificationError::AdmissionExpired => {
                Self::AdmissionExpired
            }
            crate::stage5c_paper_host::Stage5cBootstrapNotificationError::StrategyTargetMismatch => {
                Self::StrategyTargetMismatch
            }
            crate::stage5c_paper_host::Stage5cBootstrapNotificationError::StrategyTickSizeMismatch => {
                Self::StrategyTickSizeMismatch
            }
            crate::stage5c_paper_host::Stage5cBootstrapNotificationError::ActiveOrdersRequireOwnershipMapping => {
                Self::ActiveOrdersRequireOwnershipMapping
            }
            crate::stage5c_paper_host::Stage5cBootstrapNotificationError::SnapshotAccountMismatch => {
                Self::SnapshotAccountMismatch
            }
            crate::stage5c_paper_host::Stage5cBootstrapNotificationError::SnapshotInstrumentMismatch => {
                Self::SnapshotInstrumentMismatch
            }
            crate::stage5c_paper_host::Stage5cBootstrapNotificationError::PositionQuantityNotRepresentable => {
                Self::PositionQuantityNotRepresentable
            }
            crate::stage5c_paper_host::Stage5cBootstrapNotificationError::PositionAveragePriceNotRepresentable => {
                Self::PositionAveragePriceNotRepresentable
            }
        }
    }
}

fn validate_loaded_envelope_binding(
    loaded: &crate::stage5c_paper_host::Stage5cRuntimeStateLoadedPaperStrategy,
    envelope: &Stage5dPersistenceEnvelope,
) -> Result<(), Stage5dEnvelopeValidationError> {
    let admission = loaded.stage5d_admission();
    if admission.strategy_id() != envelope.binding.strategy_id
        || admission.account_id() != &envelope.binding.account_id
        || admission.target_instrument() != &envelope.binding.instrument_id.to_instrument_id()
    {
        return Err(Stage5dEnvelopeValidationError::BindingMismatch);
    }

    let strategy = loaded.stage5d_strategy();
    if strategy.stage5c_config_fingerprint() != envelope.binding.stage5c_compat_config_fingerprint
        || stage5d_profile_binding_string(strategy) != envelope.binding.profile_binding
        || strategy.stage5d_canonical_config_fingerprint()
            != envelope.binding.stage5d_canonical_config_fingerprint
    {
        return Err(Stage5dEnvelopeValidationError::BindingMismatch);
    }
    if envelope.binding.broker_protocol_schema_version
        != STAGE5D_SUPPORTED_BROKER_PROTOCOL_SCHEMA_VERSION
        || envelope.binding.runtime_state_schema_version
            != STAGE5D_SUPPORTED_RUNTIME_STATE_SCHEMA_VERSION
        || !stage5d_source_build_is_compatible(&envelope.binding.source_commit_or_build_id)
    {
        return Err(Stage5dEnvelopeValidationError::BindingMismatch);
    }

    let current_state = serde_json::to_value(Strategy::state(strategy))
        .map_err(|_| Stage5dEnvelopeValidationError::SerializationFailed)?;
    if stage5d_persisted_owned_semantic_projection(current_state)?
        != stage5d_persisted_owned_semantic_projection(
            envelope.strategy_state.strategy_state_json.clone(),
        )?
    {
        return Err(Stage5dEnvelopeValidationError::BindingMismatch);
    }

    let restored = loaded.stage5d_restored();
    let loaded_order_ids: HashSet<_> = restored.known_order_ids.iter().collect();
    let envelope_order_ids: HashSet<_> = envelope.recovery_indexes.known_order_ids.iter().collect();
    let loaded_pending_requests: HashSet<_> = restored.pending_requests.iter().collect();
    let envelope_pending_requests: HashSet<_> =
        envelope.recovery_indexes.pending_requests.iter().collect();
    if loaded_order_ids != envelope_order_ids
        || loaded_pending_requests != envelope_pending_requests
    {
        return Err(Stage5dEnvelopeValidationError::RecoveryIndexInconsistent);
    }
    validate_loaded_origin_binding(loaded, envelope)?;
    Ok(())
}

fn validate_loaded_origin_binding(
    loaded: &crate::stage5c_paper_host::Stage5cRuntimeStateLoadedPaperStrategy,
    envelope: &Stage5dPersistenceEnvelope,
) -> Result<(), Stage5dEnvelopeValidationError> {
    let crate::stage5c_paper_host::Stage5cRuntimeStateLoadOrigin::Persisted {
        semantic_payload_fingerprint,
        persisted_ts,
        recovery_index_fingerprint,
    } = loaded.stage5d_load_origin()
    else {
        return Err(Stage5dEnvelopeValidationError::BindingMismatch);
    };
    let envelope_semantic_fingerprint =
        crate::stage5c_paper_host::stage5c_semantic_value_fingerprint(
            &envelope.strategy_state.strategy_state_json,
        )
        .map_err(|_| Stage5dEnvelopeValidationError::SerializationFailed)?;
    let envelope_recovery_fingerprint =
        crate::stage5c_paper_host::stage5c_recovery_index_fingerprint(
            &envelope.recovery_indexes.known_order_ids,
            &envelope.recovery_indexes.pending_requests,
        )
        .map_err(|_| Stage5dEnvelopeValidationError::SerializationFailed)?;
    if semantic_payload_fingerprint != &envelope_semantic_fingerprint
        || persisted_ts != &envelope.persisted_at_ts_utc
        || recovery_index_fingerprint != &envelope_recovery_fingerprint
    {
        return Err(Stage5dEnvelopeValidationError::BindingMismatch);
    }
    Ok(())
}

fn stage5d_source_build_is_compatible(source_commit_or_build_id: &str) -> bool {
    STAGE5D_COMPATIBLE_SOURCE_BUILD_IDS.contains(&source_commit_or_build_id)
}

fn stage5d_profile_binding_string(
    strategy: &crate::hybrid_intraday_runtime::HybridIntradayRuntimeStrategy,
) -> String {
    let (profile, mr_variant, mr_gate_policy, risk_gate_mode) = strategy.stage5c_profile_binding();
    format!("{profile}|{mr_variant}|{mr_gate_policy}|{risk_gate_mode}")
}

fn stage5d_persisted_owned_semantic_projection(
    mut value: Value,
) -> Result<Value, Stage5dEnvelopeValidationError> {
    let Some(state) = value.get_mut("HybridIntradayRuntime") else {
        return Err(Stage5dEnvelopeValidationError::SemanticStateInvalid);
    };
    let Some(fields) = state.as_object_mut() else {
        return Err(Stage5dEnvelopeValidationError::SemanticStateInvalid);
    };
    for recomputable in [
        "entry_ready",
        "last_bar_close",
        "prev_day_close",
        "last_day_local",
        "current_day_high",
        "current_day_low",
        "current_day_close",
        "prev_day_range",
        "prev_day_return",
        "day_before_close",
        "today_start_local",
        "risk_gate_mr_enabled_current_session",
        "risk_gate_rolling_sum_lb120",
        "risk_gate_last_finalized_session_date",
        "risk_gate_ledger_rows_count",
    ] {
        fields.remove(recomputable);
    }
    Ok(value)
}

/// Redacted blocked-restore marker for future Stage 5D transitions.
pub struct Stage5dRestoreBlocked {
    reason: Stage5dRestoreBlockReason,
}

impl Stage5dRestoreBlocked {
    /// Return the redacted block reason without exposing strategy internals.
    pub fn reason(&self) -> Stage5dRestoreBlockReason {
        self.reason
    }
}

/// Public redacted restore blocker categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage5dRestoreBlockReason {
    PrivateExtension,
    RiskGate,
    BrokerTruth,
    Integrity,
}

/// Redacted evidence that the Stage 5D additive freeze enforcement layer is
/// present. This is not a trading capability.
pub struct Stage5dAdditiveFreezeEvidence {
    schema_version: u16,
}

impl Stage5dAdditiveFreezeEvidence {
    /// Construct redacted local evidence for checker/tests.
    pub fn local() -> Self {
        Self {
            schema_version: STAGE5D_ADDITIVE_FREEZE_SCHEMA_VERSION,
        }
    }

    /// Schema version of the Stage 5D additive freeze surface.
    pub fn schema_version(&self) -> u16 {
        self.schema_version
    }
}

/// Timestamp units used by a specific numeric timestamp family in the envelope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage5dTimestampUnits {
    Seconds,
    Milliseconds,
}

/// Structured timestamp encoding used by typed timestamp fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage5dStructuredTimestampFormat {
    Rfc3339Utc,
}

/// Per-family timestamp policy. Source runtime semantic lifecycle timestamps
/// are epoch seconds, while runtime wall-clock timers are epoch milliseconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stage5dTimestampPolicy {
    pub semantic_event_ts_utc: Stage5dTimestampUnits,
    pub runtime_wall_clock_timer: Stage5dTimestampUnits,
    pub structured_timestamps: Stage5dStructuredTimestampFormat,
}

/// Persistence stage marker. Stage 5D envelopes cannot be silently reused by
/// another persistence generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage5dPersistenceStage {
    Stage5d,
}

/// Runtime strategy kind bound to the persisted snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage5dStrategyKind {
    HybridIntraday,
}

/// Strict Stage 5D-owned instrument binding. This prevents unknown fields in
/// broker-core `InstrumentId` JSON from being silently discarded before
/// checksum validation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stage5dInstrumentBinding {
    pub symbol: String,
    pub venue_symbol: Option<String>,
    pub exchange: Exchange,
    pub market: Market,
}

impl Stage5dInstrumentBinding {
    /// Convert the strict binding into the broker-neutral core instrument id.
    pub fn to_instrument_id(&self) -> InstrumentId {
        InstrumentId {
            symbol: self.symbol.clone(),
            venue_symbol: self.venue_symbol.clone(),
            exchange: self.exchange.clone(),
            market: self.market.clone(),
        }
    }
}

/// Stable side enum for Stage 5D persistence schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage5dSide {
    Long,
    Short,
}

/// Stable owner enum for Stage 5D persistence schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage5dOwner {
    MeanReversion,
    IntradayBreakout,
}

/// Stable entry-style enum for Stage 5D persistence schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage5dEntryStyle {
    Market,
    Bracket,
}

/// Stable reason enum for Stage 5D persistence schema.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage5dLifecycleReason {
    MorningMeanReversionLong,
    MorningMeanReversionShort,
    BreakoutLong,
    BreakoutShort,
    BreakoutEodExit,
    BreakoutStop2Long,
    BreakoutStop1Long,
    BreakoutStop2Short,
    BreakoutStop1Short,
    MeanRevTimeCutoff,
    WaitfixOvernightExit,
}

/// Snapshot binding that prevents a valid envelope from being restored against
/// a different account, instrument, runtime profile or protocol generation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stage5dSnapshotBinding {
    pub stage: Stage5dPersistenceStage,
    pub strategy_kind: Stage5dStrategyKind,
    pub strategy_id: String,
    pub account_id: BrokerAccountId,
    pub instrument_id: Stage5dInstrumentBinding,
    pub profile_binding: String,
    pub broker_protocol_schema_version: u16,
    pub runtime_state_schema_version: u16,
    pub stage5c_compat_config_fingerprint: String,
    pub stage5d_canonical_config_fingerprint: String,
    pub source_commit_or_build_id: String,
    pub created_at_ts_utc: DateTime<Utc>,
}

/// Canonical semantic StrategyState payload. This is stored as JSON to avoid
/// exporting runtime-private source structures through the public Stage 5D API.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stage5dStrategyStatePayload {
    pub schema_version: u16,
    pub strategy_state_json: Value,
}

/// Strict Stage 5D semantic StrategyState payload root.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Stage5dSemanticStrategyStateV1 {
    HybridIntradayRuntime(Stage5dHybridIntradayStrategyStateV1),
}

/// Strict Stage 5D mirror of the accepted HybridIntradayRuntime semantic
/// `StrategyState` fields. This is a public persistence schema, not a runtime
/// private source struct and not a mutation capability.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stage5dHybridIntradayStrategyStateV1 {
    pub active_cycle_id: Option<String>,
    pub next_cycle_seq: u32,
    pub last_position_qty: f64,
    pub current_owner: Option<Stage5dOwner>,
    pub current_side: Option<Stage5dSide>,
    pub pending_entry_owner: Option<Stage5dOwner>,
    pub pending_entry_side: Option<Stage5dSide>,
    pub pending_entry_cycle_id: Option<String>,
    pub pending_entry_request_id: Option<StrategyRequestId>,
    pub pending_entry_created_ts_utc: Option<i64>,
    pub deferred_entry_owner: Option<Stage5dOwner>,
    pub deferred_entry_side: Option<Stage5dSide>,
    pub deferred_entry_cycle_id: Option<String>,
    pub deferred_entry_entry_style: Option<Stage5dEntryStyle>,
    pub deferred_entry_reason: Option<Stage5dLifecycleReason>,
    pub deferred_entry_stop_price: Option<f64>,
    pub deferred_entry_take_price: Option<f64>,
    pub deferred_entry_ts_utc: Option<i64>,
    pub deferred_entry_request_id: Option<StrategyRequestId>,
    pub pending_exit_request_id: Option<StrategyRequestId>,
    pub pending_exit_created_ts_utc: Option<i64>,
    pub deferred_exit_owner: Option<Stage5dOwner>,
    pub deferred_exit_reason: Option<Stage5dLifecycleReason>,
    pub deferred_exit_cycle_id: Option<String>,
    pub deferred_exit_ts_utc: Option<i64>,
    pub deferred_exit_request_id: Option<StrategyRequestId>,
    pub pending_tp_request_id: Option<StrategyRequestId>,
    pub pending_tp_created_ts_utc: Option<i64>,
    pub pending_sl_request_id: Option<StrategyRequestId>,
    pub pending_sl_created_ts_utc: Option<i64>,
    pub tp_order_id: Option<BrokerOrderId>,
    pub sl_stop_order_id: Option<BrokerStopOrderId>,
    pub sl_exchange_order_id: Option<BrokerOrderId>,
    pub sl_triggered_ts: Option<i64>,
    pub mr_take_price: Option<f64>,
    pub mr_stop_price: Option<f64>,
    pub repair_deadline_ts: Option<i64>,
    pub next_repair_at_ts: Option<i64>,
    pub repair_backoff_level: u32,
    pub repair_attempts: u32,
    pub safe_mode_close_only: bool,
    pub safe_mode_reason: Option<String>,
    pub entry_ready: bool,
    pub last_bar_close: Option<f64>,
    pub prev_day_close: Option<f64>,
    pub last_day_local: Option<String>,
    pub current_day_high: Option<f64>,
    pub current_day_low: Option<f64>,
    pub current_day_close: Option<f64>,
    pub prev_day_range: Option<f64>,
    pub prev_day_return: Option<f64>,
    pub day_before_close: Option<f64>,
    pub today_start_local: Option<String>,
    pub was_long_today: bool,
    pub was_short_today: bool,
    pub overnight_exit_armed_date: Option<String>,
    pub risk_gate_shadow_session_date: Option<String>,
    pub risk_gate_shadow_pnl_points: f64,
    pub risk_gate_shadow_trade_count: u32,
    pub risk_gate_shadow_entry_ts_utc: Option<i64>,
    pub risk_gate_shadow_entry_price: Option<f64>,
    pub risk_gate_shadow_side: Option<Stage5dSide>,
    pub risk_gate_shadow_target_price: Option<f64>,
    pub risk_gate_shadow_stop_price: Option<f64>,
    pub risk_gate_pending_session_date: Option<String>,
    pub risk_gate_pending_shadow_pnl_points: f64,
    pub risk_gate_pending_shadow_trade_count: u32,
    pub risk_gate_mr_enabled_current_session: Option<bool>,
    pub risk_gate_rolling_sum_lb120: Option<f64>,
    pub risk_gate_last_finalized_session_date: Option<String>,
    pub risk_gate_ledger_rows_count: usize,
}

impl Stage5dHybridIntradayStrategyStateV1 {
    fn active_cycle_id_is_valid(&self) -> bool {
        self.active_cycle_id
            .as_deref()
            .map(stage5d_cycle_id_is_valid)
            .unwrap_or(true)
    }
}

/// Lifecycle watermarks that bind restored state to processed data progress.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stage5dLifecycleWatermarks {
    pub persisted_event_watermark: Option<String>,
    pub last_semantic_bar_ts: Option<DateTime<Utc>>,
    pub last_broker_event_ts: Option<DateTime<Utc>>,
}

/// Broker-neutral typed recovery indexes. Namespaces are intentionally
/// separated; `ClientOrderId` never substitutes for `StrategyRequestId`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stage5dRecoveryIndexes {
    pub known_order_ids: Vec<BrokerOrderId>,
    pub known_stop_order_ids: Vec<BrokerStopOrderId>,
    pub known_trade_ids: Vec<BrokerTradeId>,
    pub known_client_order_ids: Vec<ClientOrderId>,
    pub pending_requests: Vec<StrategyRequestId>,
}

/// Runtime-private pending entry schema. This is a persistence representation,
/// not the source-private runtime struct.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stage5dPendingEntryExtension {
    pub owner: Stage5dOwner,
    pub side: Stage5dSide,
    pub reason: Stage5dLifecycleReason,
    pub entry_style: Stage5dEntryStyle,
    pub target_qty: String,
    pub stop_price: Option<String>,
    pub take_price: Option<String>,
    pub request_id: Option<StrategyRequestId>,
}

/// Runtime-private partial-entry timer schema.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stage5dPartialEntryTimer {
    pub partial_started_at_ms: i64,
}

/// Runtime-private pending exit schema.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stage5dPendingExitExtension {
    pub owner: Stage5dOwner,
    pub reason: Stage5dLifecycleReason,
    pub request_id: StrategyRequestId,
}

/// Runtime-private bracket reconciliation timer schema.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stage5dBracketReconciliationTimer {
    pub bracket_terminal_reconcile_started_ms: i64,
}

/// Runtime-private cleanup retry schema.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stage5dCleanupRetryState {
    pub cleanup_stop_retry_attempts: u32,
}

/// Non-authoritative expected broker-object hints. Actual working sets must be
/// rebuilt from broker truth before callbacks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stage5dExpectedWorkingSets {
    pub expected_working_order_ids: Vec<BrokerOrderId>,
    pub expected_working_stop_order_ids: Vec<BrokerStopOrderId>,
}

/// Riskgate finalization outbox record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stage5dRiskGateFinalizationOutboxRecord {
    pub session_date: String,
    pub generation: u64,
    pub state: Stage5dRiskGateFinalizationState,
    pub identity_hash: String,
}

/// Runtime-private pending riskgate finalization payload. This mirrors the
/// source runtime vector payload and is intentionally separate from durable
/// outbox state-machine metadata.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stage5dRuntimePendingRiskGateFinalization {
    pub session_date: String,
    pub shadow_pnl_points: String,
    pub shadow_trade_count: u32,
}

/// Riskgate finalization outbox state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage5dRiskGateFinalizationState {
    Prepared,
    LedgerAppended,
    MaterializedUpdated,
    AcknowledgedInRuntime,
}

/// Versioned Stage 5D runtime-private extension DTO. This DTO is schema-only in
/// Stage 5D-b2a and is not applied to the runtime.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stage5dRuntimePrivateExtension {
    pub schema_version: u16,
    pub pending_entry: Option<Stage5dPendingEntryExtension>,
    pub partial_entry_timer: Option<Stage5dPartialEntryTimer>,
    pub pending_exit: Option<Stage5dPendingExitExtension>,
    pub bracket_reconciliation_timer: Option<Stage5dBracketReconciliationTimer>,
    pub cleanup_retry_state: Option<Stage5dCleanupRetryState>,
    pub expected_working_sets: Stage5dExpectedWorkingSets,
    pub last_processed_bar_ts: Option<DateTime<Utc>>,
    pub runtime_pending_finalizations: Vec<Stage5dRuntimePendingRiskGateFinalization>,
}

/// Riskgate identity section.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stage5dRiskGateIdentity {
    pub strategy_id: String,
    pub profile_id: String,
    pub mr_variant: String,
    pub timeframe: String,
    pub session_policy: String,
    pub model_version: String,
}

/// Materialized riskgate projection.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stage5dRiskGateMaterializedState {
    pub mr_enabled_current_session: Option<bool>,
    pub mr_enabled_next_session: Option<bool>,
    pub rolling_sum_lb120: Option<String>,
    pub last_finalized_session_date: Option<String>,
    pub ledger_rows_count: u64,
    #[serde(default, skip_serializing_if = "stage5d_is_false")]
    pub seed_loaded: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_shadow_session_date: Option<String>,
    #[serde(default, skip_serializing_if = "stage5d_string_is_empty")]
    pub current_shadow_pnl_points: String,
    #[serde(default, skip_serializing_if = "stage5d_string_is_empty")]
    pub current_generation: String,
}

/// Stable source enum for Stage 5D riskgate ledger evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage5dRiskGateRowSource {
    Seed,
    Runtime,
}

/// Stable status enum for Stage 5D riskgate ledger evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage5dRiskGateRowStatus {
    Complete,
    Incomplete,
}

/// Normalized durable riskgate ledger record evidence. This is an in-process
/// Stage 5D DTO and does not open Redis.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stage5dRiskGateLedgerRecord {
    pub session_date: String,
    pub shadow_pnl_points: String,
    pub shadow_trade_count: u32,
    pub rolling_sum_before_session: String,
    pub mr_enabled_for_session: bool,
    pub source: Stage5dRiskGateRowSource,
    pub status: Stage5dRiskGateRowStatus,
    pub rolling_sum_lb120: String,
    pub mr_enabled_next_session: bool,
    pub finalized_at_utc: i64,
}

/// Authoritative riskgate ledger evidence supplied to Stage 5D-b2b-c. It is
/// deterministic fixture/read-only evidence, not Redis transport.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stage5dRiskGateLedgerEvidence {
    pub schema_version: u16,
    pub identity: Stage5dRiskGateIdentity,
    pub ledger_tail_hash: String,
    pub ledger_records: Vec<Stage5dRiskGateLedgerRecord>,
    pub seed_loaded: bool,
    pub current_shadow_session_date: Option<String>,
    pub current_shadow_pnl_points: String,
    pub current_generation: String,
}

/// Riskgate persistence DTO. This is schema-only in Stage 5D-b2a.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stage5dRiskGatePersistence {
    pub schema_version: u16,
    pub identity: Stage5dRiskGateIdentity,
    pub materialized_state: Stage5dRiskGateMaterializedState,
    pub ledger_tail_hash: String,
    pub durable_finalization_outbox: Vec<Stage5dRiskGateFinalizationOutboxRecord>,
}

/// Versioned Stage 5D persistence envelope DTO.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stage5dPersistenceEnvelope {
    pub schema_version: u16,
    pub snapshot_id: String,
    pub snapshot_revision: u64,
    pub previous_revision: Option<u64>,
    pub write_generation: u64,
    pub persisted_at_ts_utc: DateTime<Utc>,
    pub timestamp_policy: Stage5dTimestampPolicy,
    pub canonical_config_fingerprint: String,
    pub binding: Stage5dSnapshotBinding,
    pub strategy_state: Stage5dStrategyStatePayload,
    pub payload_checksum_sha256: String,
    pub lifecycle_watermarks: Stage5dLifecycleWatermarks,
    pub recovery_indexes: Stage5dRecoveryIndexes,
    pub runtime_private_extension: Stage5dRuntimePrivateExtension,
    pub riskgate: Stage5dRiskGatePersistence,
}

impl Stage5dPersistenceEnvelope {
    /// Strictly decode and validate an envelope from JSON. Unknown fields at any
    /// Stage 5D DTO layer fail closed before checksum validation.
    pub fn from_json_str_strict(payload: &str) -> Result<Self, Stage5dEnvelopeValidationError> {
        let envelope: Self = serde_json::from_str(payload)
            .map_err(|_| Stage5dEnvelopeValidationError::DeserializationFailed)?;
        envelope.validate_schema_and_checksum()?;
        Ok(envelope)
    }

    /// Strictly decode, validate checksum/schema and prove schema-only
    /// restore-contract consistency without mutating runtime state.
    pub fn validated_from_json_str_strict(
        payload: &str,
    ) -> Result<Stage5dValidatedPersistenceEnvelope, Stage5dEnvelopeValidationError> {
        let envelope = Self::from_json_str_strict(payload)?;
        envelope.validate_restore_contract_schema_only()
    }

    /// Compute the canonical payload checksum with the checksum field cleared.
    pub fn compute_payload_checksum_sha256(
        &self,
    ) -> Result<String, Stage5dEnvelopeValidationError> {
        let mut canonical = self.clone();
        canonical.payload_checksum_sha256.clear();
        let payload = serde_json::to_vec(&canonical)
            .map_err(|_| Stage5dEnvelopeValidationError::SerializationFailed)?;
        Ok(format!("{:x}", Sha256::digest(payload)))
    }

    /// Validate schema versions, required identity fields and payload checksum.
    pub fn validate_schema_and_checksum(&self) -> Result<(), Stage5dEnvelopeValidationError> {
        if self.schema_version != STAGE5D_PERSISTENCE_ENVELOPE_SCHEMA_VERSION {
            return Err(Stage5dEnvelopeValidationError::EnvelopeSchemaMismatch);
        }
        if self.runtime_private_extension.schema_version
            != STAGE5D_RUNTIME_PRIVATE_EXTENSION_SCHEMA_VERSION
        {
            return Err(Stage5dEnvelopeValidationError::RuntimePrivateSchemaMismatch);
        }
        if self.riskgate.schema_version != STAGE5D_RISKGATE_SCHEMA_VERSION {
            return Err(Stage5dEnvelopeValidationError::RiskGateSchemaMismatch);
        }
        if self.strategy_state.schema_version != STAGE5D_STRATEGY_STATE_PAYLOAD_SCHEMA_VERSION {
            return Err(Stage5dEnvelopeValidationError::StrategyStateSchemaMismatch);
        }
        if self.snapshot_id.is_empty()
            || self.canonical_config_fingerprint.is_empty()
            || self.binding.strategy_id.is_empty()
            || self.binding.profile_binding.is_empty()
            || self.binding.stage5c_compat_config_fingerprint.is_empty()
            || self.binding.stage5d_canonical_config_fingerprint.is_empty()
            || self.binding.source_commit_or_build_id.is_empty()
            || self.strategy_state.strategy_state_json.is_null()
            || self.riskgate.identity.strategy_id.is_empty()
            || self.riskgate.identity.profile_id.is_empty()
            || self.riskgate.identity.mr_variant.is_empty()
            || self.riskgate.identity.timeframe.is_empty()
            || self.riskgate.identity.session_policy.is_empty()
            || self.riskgate.identity.model_version.is_empty()
            || self.riskgate.ledger_tail_hash.is_empty()
        {
            return Err(Stage5dEnvelopeValidationError::RequiredFieldEmpty);
        }
        if self.binding.broker_protocol_schema_version == 0
            || self.binding.runtime_state_schema_version == 0
        {
            return Err(Stage5dEnvelopeValidationError::RequiredFieldEmpty);
        }
        let expected = self.compute_payload_checksum_sha256()?;
        if self.payload_checksum_sha256 != expected {
            return Err(Stage5dEnvelopeValidationError::PayloadChecksumMismatch);
        }
        Ok(())
    }

    /// Validate semantic payload and cross-section consistency required before
    /// any future restore mutation gate can consume this envelope.
    pub fn validate_restore_contract_schema_only(
        &self,
    ) -> Result<Stage5dValidatedPersistenceEnvelope, Stage5dEnvelopeValidationError> {
        self.validate_schema_and_checksum()?;
        self.validate_timestamp_policy()?;
        if self.canonical_config_fingerprint != self.binding.stage5d_canonical_config_fingerprint {
            return Err(Stage5dEnvelopeValidationError::BindingMismatch);
        }
        if self.runtime_private_extension.cleanup_retry_state.is_none() {
            return Err(Stage5dEnvelopeValidationError::PendingStateInconsistent);
        }
        if self.binding.instrument_id.symbol.is_empty()
            || self.binding.account_id.as_str().is_empty()
            || self.riskgate.identity.strategy_id != self.binding.strategy_id
        {
            return Err(Stage5dEnvelopeValidationError::BindingMismatch);
        }

        let semantic: Stage5dSemanticStrategyStateV1 =
            serde_json::from_value(self.strategy_state.strategy_state_json.clone())
                .map_err(|_| Stage5dEnvelopeValidationError::SemanticStateInvalid)?;
        let canonical = serde_json::to_value(&semantic)
            .map_err(|_| Stage5dEnvelopeValidationError::SerializationFailed)?;
        if canonical != self.strategy_state.strategy_state_json {
            return Err(Stage5dEnvelopeValidationError::SemanticStateInvalid);
        }

        let Stage5dSemanticStrategyStateV1::HybridIntradayRuntime(state) = &semantic;
        self.validate_source_roundtrip_consistency(state)?;
        self.validate_hybrid_state_consistency(state)?;

        Ok(Stage5dValidatedPersistenceEnvelope {
            envelope: self.clone(),
        })
    }

    fn validate_timestamp_policy(&self) -> Result<(), Stage5dEnvelopeValidationError> {
        if self.timestamp_policy.semantic_event_ts_utc != Stage5dTimestampUnits::Seconds
            || self.timestamp_policy.runtime_wall_clock_timer != Stage5dTimestampUnits::Milliseconds
            || self.timestamp_policy.structured_timestamps
                != Stage5dStructuredTimestampFormat::Rfc3339Utc
        {
            return Err(Stage5dEnvelopeValidationError::TimestampPolicyInvalid);
        }
        Ok(())
    }

    fn validate_source_roundtrip_consistency(
        &self,
        state: &Stage5dHybridIntradayStrategyStateV1,
    ) -> Result<(), Stage5dEnvelopeValidationError> {
        validate_optional_local_date(state.last_day_local.as_deref())?;
        validate_optional_local_datetime(state.today_start_local.as_deref())?;
        validate_optional_local_date(state.overnight_exit_armed_date.as_deref())?;
        validate_optional_local_date(state.risk_gate_shadow_session_date.as_deref())?;
        validate_optional_local_date(state.risk_gate_pending_session_date.as_deref())?;
        validate_optional_local_date(state.risk_gate_last_finalized_session_date.as_deref())?;
        validate_optional_local_date(
            self.riskgate
                .materialized_state
                .last_finalized_session_date
                .as_deref(),
        )?;
        validate_optional_local_date(
            self.riskgate
                .materialized_state
                .current_shadow_session_date
                .as_deref(),
        )?;
        if let Some(value) = self
            .riskgate
            .materialized_state
            .rolling_sum_lb120
            .as_deref()
        {
            validate_decimal_string(value)?;
        }
        if !self
            .riskgate
            .materialized_state
            .current_shadow_pnl_points
            .is_empty()
        {
            validate_decimal_string(&self.riskgate.materialized_state.current_shadow_pnl_points)?;
        }
        for record in &self.runtime_private_extension.runtime_pending_finalizations {
            validate_local_date(&record.session_date)?;
            validate_decimal_string(&record.shadow_pnl_points)?;
        }
        for record in &self.riskgate.durable_finalization_outbox {
            validate_local_date(&record.session_date)?;
        }

        self.validate_watermark_chronology()?;
        self.validate_semantic_timestamp(state.pending_entry_created_ts_utc)?;
        self.validate_semantic_timestamp(state.deferred_entry_ts_utc)?;
        self.validate_semantic_timestamp(state.pending_exit_created_ts_utc)?;
        self.validate_semantic_timestamp(state.deferred_exit_ts_utc)?;
        self.validate_semantic_timestamp(state.pending_tp_created_ts_utc)?;
        self.validate_semantic_timestamp(state.pending_sl_created_ts_utc)?;
        self.validate_semantic_timestamp(state.sl_triggered_ts)?;
        self.validate_semantic_timestamp(state.repair_deadline_ts)?;
        self.validate_semantic_timestamp(state.next_repair_at_ts)?;
        self.validate_semantic_timestamp(state.risk_gate_shadow_entry_ts_utc)?;

        self.validate_semantic_event_not_after_persisted(state.pending_entry_created_ts_utc)?;
        self.validate_semantic_event_not_after_persisted(state.deferred_entry_ts_utc)?;
        self.validate_semantic_event_not_after_persisted(state.pending_exit_created_ts_utc)?;
        self.validate_semantic_event_not_after_persisted(state.deferred_exit_ts_utc)?;
        self.validate_semantic_event_not_after_persisted(state.pending_tp_created_ts_utc)?;
        self.validate_semantic_event_not_after_persisted(state.pending_sl_created_ts_utc)?;
        self.validate_semantic_event_not_after_persisted(state.sl_triggered_ts)?;
        self.validate_semantic_event_not_after_persisted(state.risk_gate_shadow_entry_ts_utc)?;

        if let Some(timer) = &self.runtime_private_extension.partial_entry_timer {
            self.validate_runtime_timer_ms(Some(timer.partial_started_at_ms))?;
        }
        if let Some(timer) = &self.runtime_private_extension.bracket_reconciliation_timer {
            self.validate_runtime_timer_ms(Some(timer.bracket_terminal_reconcile_started_ms))?;
        }

        validate_deferred_entry_tuple(state)?;
        validate_deferred_exit_tuple(state)?;
        validate_optional_request_timestamp_pair(
            state.pending_tp_request_id,
            state.pending_tp_created_ts_utc,
        )?;
        validate_optional_request_timestamp_pair(
            state.pending_sl_request_id,
            state.pending_sl_created_ts_utc,
        )?;
        validate_shadow_position_tuple(state)?;
        self.validate_riskgate_pending_finalization_cache(state)?;
        self.validate_recovery_indexes(state)?;
        self.validate_broker_id_indexes(state)?;

        Ok(())
    }

    fn validate_watermark_chronology(&self) -> Result<(), Stage5dEnvelopeValidationError> {
        if self
            .lifecycle_watermarks
            .last_broker_event_ts
            .is_some_and(|ts| ts > self.persisted_at_ts_utc)
            || self
                .lifecycle_watermarks
                .last_semantic_bar_ts
                .is_some_and(|ts| ts > self.persisted_at_ts_utc)
            || self
                .runtime_private_extension
                .last_processed_bar_ts
                .is_some_and(|ts| ts > self.persisted_at_ts_utc)
        {
            return Err(Stage5dEnvelopeValidationError::TimestampChronologyInvalid);
        }
        if let (Some(processed), Some(semantic)) = (
            self.runtime_private_extension.last_processed_bar_ts,
            self.lifecycle_watermarks.last_semantic_bar_ts,
        ) {
            if processed > semantic {
                return Err(Stage5dEnvelopeValidationError::TimestampChronologyInvalid);
            }
        }
        Ok(())
    }

    fn validate_semantic_timestamp(
        &self,
        value: Option<i64>,
    ) -> Result<(), Stage5dEnvelopeValidationError> {
        if let Some(ts) = value {
            if !(946_684_800..=4_102_444_800).contains(&ts) {
                return Err(Stage5dEnvelopeValidationError::TimestampChronologyInvalid);
            }
        }
        Ok(())
    }

    fn validate_semantic_event_not_after_persisted(
        &self,
        value: Option<i64>,
    ) -> Result<(), Stage5dEnvelopeValidationError> {
        if let Some(ts) = value {
            if ts > self.persisted_at_ts_utc.timestamp() {
                return Err(Stage5dEnvelopeValidationError::TimestampChronologyInvalid);
            }
        }
        Ok(())
    }

    fn validate_runtime_timer_ms(
        &self,
        value: Option<i64>,
    ) -> Result<(), Stage5dEnvelopeValidationError> {
        if let Some(ts) = value {
            if !(946_684_800_000..=self.persisted_at_ts_utc.timestamp_millis()).contains(&ts) {
                return Err(Stage5dEnvelopeValidationError::TimestampChronologyInvalid);
            }
        }
        Ok(())
    }

    fn validate_recovery_indexes(
        &self,
        state: &Stage5dHybridIntradayStrategyStateV1,
    ) -> Result<(), Stage5dEnvelopeValidationError> {
        ensure_unique(&self.recovery_indexes.pending_requests)?;
        ensure_unique(&self.recovery_indexes.known_order_ids)?;
        ensure_unique(&self.recovery_indexes.known_stop_order_ids)?;
        ensure_unique(&self.recovery_indexes.known_trade_ids)?;
        ensure_unique(&self.recovery_indexes.known_client_order_ids)?;
        ensure_unique(
            &self
                .runtime_private_extension
                .expected_working_sets
                .expected_working_order_ids,
        )?;
        ensure_unique(
            &self
                .runtime_private_extension
                .expected_working_sets
                .expected_working_stop_order_ids,
        )?;

        let expected_pending: HashSet<StrategyRequestId> = [
            state.pending_entry_request_id,
            state.deferred_entry_request_id,
            state.pending_exit_request_id,
            state.deferred_exit_request_id,
            state.pending_tp_request_id,
            state.pending_sl_request_id,
        ]
        .into_iter()
        .flatten()
        .collect();
        if expected_pending.len()
            != [
                state.pending_entry_request_id,
                state.deferred_entry_request_id,
                state.pending_exit_request_id,
                state.deferred_exit_request_id,
                state.pending_tp_request_id,
                state.pending_sl_request_id,
            ]
            .into_iter()
            .flatten()
            .count()
        {
            return Err(Stage5dEnvelopeValidationError::RecoveryIndexInconsistent);
        }
        let actual_pending: HashSet<StrategyRequestId> = self
            .recovery_indexes
            .pending_requests
            .iter()
            .copied()
            .collect();
        if actual_pending != expected_pending {
            return Err(Stage5dEnvelopeValidationError::RecoveryIndexInconsistent);
        }
        Ok(())
    }

    fn validate_broker_id_indexes(
        &self,
        state: &Stage5dHybridIntradayStrategyStateV1,
    ) -> Result<(), Stage5dEnvelopeValidationError> {
        if state
            .tp_order_id
            .as_ref()
            .is_some_and(|id| !self.recovery_indexes.known_order_ids.contains(id))
            || state
                .sl_exchange_order_id
                .as_ref()
                .is_some_and(|id| !self.recovery_indexes.known_order_ids.contains(id))
            || state
                .sl_stop_order_id
                .as_ref()
                .is_some_and(|id| !self.recovery_indexes.known_stop_order_ids.contains(id))
        {
            return Err(Stage5dEnvelopeValidationError::RecoveryIndexInconsistent);
        }
        if self
            .runtime_private_extension
            .expected_working_sets
            .expected_working_order_ids
            .iter()
            .any(|id| !self.recovery_indexes.known_order_ids.contains(id))
            || self
                .runtime_private_extension
                .expected_working_sets
                .expected_working_stop_order_ids
                .iter()
                .any(|id| !self.recovery_indexes.known_stop_order_ids.contains(id))
        {
            return Err(Stage5dEnvelopeValidationError::RecoveryIndexInconsistent);
        }
        Ok(())
    }

    fn validate_riskgate_pending_finalization_cache(
        &self,
        state: &Stage5dHybridIntradayStrategyStateV1,
    ) -> Result<(), Stage5dEnvelopeValidationError> {
        let mut previous_session: Option<NaiveDate> = None;
        let current_shadow_session = state
            .risk_gate_shadow_session_date
            .as_deref()
            .map(|session| {
                NaiveDate::parse_from_str(session, "%Y-%m-%d")
                    .map_err(|_| Stage5dEnvelopeValidationError::SourceRoundtripInconsistent)
            })
            .transpose()?;
        for record in &self.runtime_private_extension.runtime_pending_finalizations {
            let session_date = NaiveDate::parse_from_str(&record.session_date, "%Y-%m-%d")
                .map_err(|_| Stage5dEnvelopeValidationError::SourceRoundtripInconsistent)?;
            if matches!(session_date.weekday(), Weekday::Sat | Weekday::Sun) {
                return Err(Stage5dEnvelopeValidationError::RiskGateFinalizationInconsistent);
            }
            if previous_session.is_some_and(|previous| session_date <= previous) {
                return Err(Stage5dEnvelopeValidationError::RiskGateFinalizationInconsistent);
            }
            if current_shadow_session.is_some_and(|current| session_date >= current) {
                return Err(Stage5dEnvelopeValidationError::RiskGateFinalizationInconsistent);
            }
            previous_session = Some(session_date);
        }

        match self
            .runtime_private_extension
            .runtime_pending_finalizations
            .first()
        {
            None => {
                if state.risk_gate_pending_session_date.is_some()
                    || state.risk_gate_pending_shadow_pnl_points != 0.0
                    || state.risk_gate_pending_shadow_trade_count != 0
                {
                    return Err(Stage5dEnvelopeValidationError::RiskGateFinalizationInconsistent);
                }
            }
            Some(first) => {
                let first_pnl = parse_finite_decimal_string(&first.shadow_pnl_points)?;
                if state.risk_gate_pending_session_date.as_deref()
                    != Some(first.session_date.as_str())
                    || state.risk_gate_pending_shadow_pnl_points != first_pnl
                    || state.risk_gate_pending_shadow_trade_count != first.shadow_trade_count
                {
                    return Err(Stage5dEnvelopeValidationError::RiskGateFinalizationInconsistent);
                }
            }
        }
        Ok(())
    }

    fn validate_hybrid_state_consistency(
        &self,
        state: &Stage5dHybridIntradayStrategyStateV1,
    ) -> Result<(), Stage5dEnvelopeValidationError> {
        if !state.active_cycle_id_is_valid()
            || state
                .pending_entry_cycle_id
                .as_deref()
                .is_some_and(|cycle| !stage5d_cycle_id_is_valid(cycle))
            || state
                .deferred_entry_cycle_id
                .as_deref()
                .is_some_and(|cycle| !stage5d_cycle_id_is_valid(cycle))
            || state
                .deferred_exit_cycle_id
                .as_deref()
                .is_some_and(|cycle| !stage5d_cycle_id_is_valid(cycle))
        {
            return Err(Stage5dEnvelopeValidationError::SemanticStateInvalid);
        }

        let pending_requests = &self.recovery_indexes.pending_requests;
        let semantic_pending_entry_present = state.pending_entry_owner.is_some()
            || state.pending_entry_side.is_some()
            || state.pending_entry_cycle_id.is_some()
            || state.pending_entry_request_id.is_some()
            || state.pending_entry_created_ts_utc.is_some();
        if semantic_pending_entry_present != self.runtime_private_extension.pending_entry.is_some()
            || self.runtime_private_extension.partial_entry_timer.is_some()
                && self.runtime_private_extension.pending_entry.is_none()
        {
            return Err(Stage5dEnvelopeValidationError::PendingStateInconsistent);
        }
        if let Some(pending_entry) = &self.runtime_private_extension.pending_entry {
            let Some(request_id) = pending_entry.request_id else {
                return Err(Stage5dEnvelopeValidationError::PendingStateInconsistent);
            };
            let Some(pending_cycle) = state.pending_entry_cycle_id.as_deref() else {
                return Err(Stage5dEnvelopeValidationError::PendingStateInconsistent);
            };
            if state.pending_entry_request_id != Some(request_id)
                || state.pending_entry_owner != Some(pending_entry.owner)
                || state.pending_entry_side != Some(pending_entry.side)
                || state.pending_entry_created_ts_utc.is_none()
                || state.active_cycle_id.as_deref() != Some(pending_cycle)
                || !pending_requests.contains(&request_id)
            {
                return Err(Stage5dEnvelopeValidationError::PendingStateInconsistent);
            }

            let target_qty: f64 = pending_entry
                .target_qty
                .parse()
                .map_err(|_| Stage5dEnvelopeValidationError::PendingStateInconsistent)?;
            if !target_qty.is_finite() || target_qty <= 0.0 {
                return Err(Stage5dEnvelopeValidationError::PendingStateInconsistent);
            }
            let filled_qty = state.last_position_qty.abs();
            let timer_present = self.runtime_private_extension.partial_entry_timer.is_some();
            if filled_qty == 0.0 && timer_present
                || filled_qty > 0.0 && filled_qty < target_qty && !timer_present
                || filled_qty >= target_qty
            {
                return Err(Stage5dEnvelopeValidationError::PendingStateInconsistent);
            }
            if let Some(timer) = &self.runtime_private_extension.partial_entry_timer {
                let Some(created) = state.pending_entry_created_ts_utc else {
                    return Err(Stage5dEnvelopeValidationError::PendingStateInconsistent);
                };
                if timer.partial_started_at_ms < created.saturating_mul(1000) {
                    return Err(Stage5dEnvelopeValidationError::TimestampChronologyInvalid);
                }
            }
        } else if self.runtime_private_extension.partial_entry_timer.is_some() {
            return Err(Stage5dEnvelopeValidationError::PendingStateInconsistent);
        }

        let semantic_pending_exit_present =
            state.pending_exit_request_id.is_some() || state.pending_exit_created_ts_utc.is_some();
        if semantic_pending_exit_present != self.runtime_private_extension.pending_exit.is_some() {
            return Err(Stage5dEnvelopeValidationError::PendingStateInconsistent);
        }
        if let Some(pending_exit) = &self.runtime_private_extension.pending_exit {
            if state.pending_exit_request_id != Some(pending_exit.request_id)
                || state.pending_exit_created_ts_utc.is_none()
                || !pending_requests.contains(&pending_exit.request_id)
            {
                return Err(Stage5dEnvelopeValidationError::PendingStateInconsistent);
            }
        }

        Ok(())
    }
}

fn stage5d_cycle_id_is_valid(value: &str) -> bool {
    value.len() == 10 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn validate_local_date(value: &str) -> Result<(), Stage5dEnvelopeValidationError> {
    let parsed = NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map_err(|_| Stage5dEnvelopeValidationError::SourceRoundtripInconsistent)?;
    if parsed.format("%Y-%m-%d").to_string() != value {
        return Err(Stage5dEnvelopeValidationError::SourceRoundtripInconsistent);
    }
    Ok(())
}

fn validate_optional_local_date(value: Option<&str>) -> Result<(), Stage5dEnvelopeValidationError> {
    if let Some(value) = value {
        validate_local_date(value)?;
    }
    Ok(())
}

fn validate_optional_local_datetime(
    value: Option<&str>,
) -> Result<(), Stage5dEnvelopeValidationError> {
    if let Some(value) = value {
        let parsed = NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S")
            .map_err(|_| Stage5dEnvelopeValidationError::SourceRoundtripInconsistent)?;
        if parsed.format("%Y-%m-%dT%H:%M:%S").to_string() != value {
            return Err(Stage5dEnvelopeValidationError::SourceRoundtripInconsistent);
        }
    }
    Ok(())
}

fn parse_finite_decimal_string(value: &str) -> Result<f64, Stage5dEnvelopeValidationError> {
    let parsed: f64 = value
        .parse()
        .map_err(|_| Stage5dEnvelopeValidationError::SourceRoundtripInconsistent)?;
    if !parsed.is_finite() {
        return Err(Stage5dEnvelopeValidationError::SourceRoundtripInconsistent);
    }
    Ok(parsed)
}

fn validate_decimal_string(value: &str) -> Result<(), Stage5dEnvelopeValidationError> {
    parse_finite_decimal_string(value).map(|_| ())
}

fn validate_deferred_entry_tuple(
    state: &Stage5dHybridIntradayStrategyStateV1,
) -> Result<(), Stage5dEnvelopeValidationError> {
    let any = state.deferred_entry_owner.is_some()
        || state.deferred_entry_side.is_some()
        || state.deferred_entry_cycle_id.is_some()
        || state.deferred_entry_entry_style.is_some()
        || state.deferred_entry_reason.is_some()
        || state.deferred_entry_stop_price.is_some()
        || state.deferred_entry_take_price.is_some()
        || state.deferred_entry_ts_utc.is_some()
        || state.deferred_entry_request_id.is_some();
    let required = state.deferred_entry_owner.is_some()
        && state.deferred_entry_side.is_some()
        && state.deferred_entry_cycle_id.is_some()
        && state.deferred_entry_entry_style.is_some()
        && state.deferred_entry_reason.is_some()
        && state.deferred_entry_ts_utc.is_some()
        && state.deferred_entry_request_id.is_some();
    if any && !required {
        return Err(Stage5dEnvelopeValidationError::SourceRoundtripInconsistent);
    }
    Ok(())
}

fn validate_deferred_exit_tuple(
    state: &Stage5dHybridIntradayStrategyStateV1,
) -> Result<(), Stage5dEnvelopeValidationError> {
    let any = state.deferred_exit_owner.is_some()
        || state.deferred_exit_reason.is_some()
        || state.deferred_exit_cycle_id.is_some()
        || state.deferred_exit_ts_utc.is_some()
        || state.deferred_exit_request_id.is_some();
    let required = state.deferred_exit_owner.is_some()
        && state.deferred_exit_reason.is_some()
        && state.deferred_exit_cycle_id.is_some()
        && state.deferred_exit_ts_utc.is_some()
        && state.deferred_exit_request_id.is_some();
    if any && !required {
        return Err(Stage5dEnvelopeValidationError::SourceRoundtripInconsistent);
    }
    Ok(())
}

fn validate_optional_request_timestamp_pair(
    request_id: Option<StrategyRequestId>,
    timestamp: Option<i64>,
) -> Result<(), Stage5dEnvelopeValidationError> {
    if request_id.is_some() != timestamp.is_some() {
        return Err(Stage5dEnvelopeValidationError::SourceRoundtripInconsistent);
    }
    Ok(())
}

fn validate_shadow_position_tuple(
    state: &Stage5dHybridIntradayStrategyStateV1,
) -> Result<(), Stage5dEnvelopeValidationError> {
    let any = state.risk_gate_shadow_entry_ts_utc.is_some()
        || state.risk_gate_shadow_entry_price.is_some()
        || state.risk_gate_shadow_side.is_some()
        || state.risk_gate_shadow_target_price.is_some()
        || state.risk_gate_shadow_stop_price.is_some();
    let required = state.risk_gate_shadow_entry_ts_utc.is_some()
        && state.risk_gate_shadow_entry_price.is_some()
        && state.risk_gate_shadow_side.is_some()
        && state.risk_gate_shadow_target_price.is_some()
        && state.risk_gate_shadow_stop_price.is_some();
    if any && !required {
        return Err(Stage5dEnvelopeValidationError::SourceRoundtripInconsistent);
    }
    Ok(())
}

fn ensure_unique<T>(values: &[T]) -> Result<(), Stage5dEnvelopeValidationError>
where
    T: Eq + std::hash::Hash,
{
    if values.iter().collect::<HashSet<_>>().len() != values.len() {
        return Err(Stage5dEnvelopeValidationError::RecoveryIndexInconsistent);
    }
    Ok(())
}

/// Stage 5D envelope validation errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage5dEnvelopeValidationError {
    DeserializationFailed,
    EnvelopeSchemaMismatch,
    RuntimePrivateSchemaMismatch,
    RiskGateSchemaMismatch,
    StrategyStateSchemaMismatch,
    RequiredFieldEmpty,
    PayloadChecksumMismatch,
    SemanticStateInvalid,
    BindingMismatch,
    PendingStateInconsistent,
    TimestampPolicyInvalid,
    TimestampChronologyInvalid,
    SourceRoundtripInconsistent,
    RecoveryIndexInconsistent,
    RiskGateFinalizationInconsistent,
    SerializationFailed,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime_compat::Strategy;
    use rust_decimal::Decimal;
    use serde_json::Value;

    fn valid_fixture() -> Stage5dPersistenceEnvelope {
        serde_json::from_str(include_str!(
            "../../../tests/fixtures/stage5/stage5d_b2a_persistence_envelope.json"
        ))
        .expect("valid Stage 5D-b2a fixture must deserialize")
    }

    fn corrupt_checksum_fixture() -> Stage5dPersistenceEnvelope {
        serde_json::from_str(include_str!(
            "../../../tests/fixtures/stage5/stage5d_b2a_persistence_envelope_corrupt_checksum.json"
        ))
        .expect("corrupt Stage 5D-b2a checksum fixture must deserialize")
    }

    fn bad_version_fixture() -> Stage5dPersistenceEnvelope {
        serde_json::from_str(include_str!(
            "../../../tests/fixtures/stage5/stage5d_b2a_persistence_envelope_bad_version.json"
        ))
        .expect("bad-version Stage 5D-b2a fixture must deserialize")
    }

    fn empty_config_fixture() -> Stage5dPersistenceEnvelope {
        serde_json::from_str(include_str!(
            "../../../tests/fixtures/stage5/stage5d_b2a_persistence_envelope_empty_config.json"
        ))
        .expect("empty-config Stage 5D-b2a fixture must deserialize")
    }

    fn stage5d_test_strategy() -> crate::hybrid_intraday_runtime::HybridIntradayRuntimeStrategy {
        stage5d_test_strategy_with_config(|_| {})
    }

    fn stage5d_test_strategy_with_config(
        mutator: impl FnOnce(&mut crate::hybrid_intraday_runtime::HybridIntradayRuntimeConfig),
    ) -> crate::hybrid_intraday_runtime::HybridIntradayRuntimeStrategy {
        let mut config = crate::hybrid_intraday_runtime::HybridIntradayRuntimeConfig {
            symbol: "IMOEXF".to_string(),
            profile: crate::hybrid_intraday_runtime::HybridIntradayProfile::BaselineRuntimeHybrid,
            mr_variant: crate::hybrid_intraday_runtime::MeanReversionVariant::ClassicPrevDayRange,
            mr_gate_policy: crate::hybrid_intraday_runtime::MrGatePolicy::Disabled,
            risk_gate_mode: crate::hybrid_intraday_runtime::RiskGateMode::Disabled,
            risk_gate_seed_file: None,
            risk_gate_ledger_key: None,
            model_session_start_time: None,
            model_session_end_time: None,
            qty: 1.0,
            live_order_style: crate::runtime_compat::MarketBuyAndCloseLiveOrderStyle::Market,
            tick_size: 0.5,
            marketable_limit_offset_ticks: 0,
            timezone_offset_hours: 3,
            session_close_hour: 23,
            session_close_minute: 49,
            weekends_off: true,
            stop_end_buffer_sec: 60,
            repair_deadline_sec: 180,
            sl_escalate_timeout_sec: 30,
            max_repair_retries: 3,
            repair_backoff_base_sec: 5,
            repair_backoff_max_sec: 60,
            pending_timeout_sec: 30,
            partial_entry_fill_timeout_ms: 3_000,
            mr_config: crate::hybrid_intraday::MeanReversionConfig::default(),
            breakout_config: crate::hybrid_intraday::IntradayBreakoutConfig::default(),
            orchestrator_config: crate::hybrid_intraday::HybridOrchestratorConfig::default(),
        };
        mutator(&mut config);
        crate::hybrid_intraday_runtime::HybridIntradayRuntimeStrategy::new(config)
    }

    fn stage5d_test_riskgate_runtime_strategy(
    ) -> crate::hybrid_intraday_runtime::HybridIntradayRuntimeStrategy {
        stage5d_test_riskgate_runtime_strategy_with_config(|_| {})
    }

    fn stage5d_test_riskgate_runtime_strategy_with_config(
        mutator: impl FnOnce(&mut crate::hybrid_intraday_runtime::HybridIntradayRuntimeConfig),
    ) -> crate::hybrid_intraday_runtime::HybridIntradayRuntimeStrategy {
        stage5d_test_strategy_with_config(|config| {
            config.profile =
                crate::hybrid_intraday_runtime::HybridIntradayProfile::ImoexfPrimaryRiskgateHigh180Lb120;
            config.mr_variant = crate::hybrid_intraday_runtime::MeanReversionVariant::High180;
            config.mr_gate_policy =
                crate::hybrid_intraday_runtime::MrGatePolicy::ShadowPnlLb120Positive;
            config.risk_gate_mode = crate::hybrid_intraday_runtime::RiskGateMode::NormalAppend;
            config.qty = 3.0;
            mutator(config);
        })
    }

    fn stage5d_test_source_ctx() -> crate::runtime_compat::StrategyCtx {
        crate::runtime_compat::StrategyCtx {
            strategy_id: "stage5d-source-riskgate".to_string(),
            portfolio: "ACC_TEST_0001".to_string(),
            exchange: "MOEX".to_string(),
            symbol: "IMOEXF".to_string(),
            tick_size: 0.5,
            trade_mode: crate::runtime_compat::TradeMode::Live,
            paper_execution_mode: crate::runtime_compat::PaperExecutionMode::LiveOnly,
            allow_live_orders: false,
            gateway_phase: crate::live_guard::GatewayPhase::SyncingHistory,
            position_qty: Some(0.0),
            event_ts_utc: 0,
            now_ts_utc: 0,
            last_bar_ts: Some(1),
        }
    }

    fn stage5d_test_source_ts_local(y: i32, mo: u32, d: u32, h: u32, m: u32, s: u32) -> i64 {
        NaiveDate::from_ymd_opt(y, mo, d)
            .expect("source date")
            .and_hms_opt(h, m, s)
            .expect("source time")
            .and_utc()
            .timestamp()
            - 3 * 3600
    }

    fn stage5d_test_source_bar_ohlc(
        ts_utc: i64,
        open: f64,
        high: f64,
        low: f64,
        close: f64,
    ) -> crate::runtime_compat::BarEvent {
        crate::runtime_compat::BarEvent {
            symbol: "IMOEXF".to_string(),
            close_time_utc: ts_utc,
            close,
            o: open,
            h: high,
            l: low,
            v: 1.0,
            origin: crate::runtime_compat::DataOrigin::Live,
        }
    }

    fn stage5d_test_source_runtime_with_real_pending_finalization() -> (
        crate::hybrid_intraday_runtime::HybridIntradayRuntimeStrategy,
        Stage5dRuntimePrivateExtension,
    ) {
        let mut strategy = stage5d_test_riskgate_runtime_strategy();
        let ctx = stage5d_test_source_ctx();

        for bar in [
            stage5d_test_source_bar_ohlc(
                stage5d_test_source_ts_local(2026, 1, 6, 9, 0, 0),
                100.2,
                101.0,
                99.0,
                100.2,
            ),
            stage5d_test_source_bar_ohlc(
                stage5d_test_source_ts_local(2026, 1, 6, 9, 10, 0),
                99.9,
                100.1,
                99.8,
                99.9,
            ),
            stage5d_test_source_bar_ohlc(
                stage5d_test_source_ts_local(2026, 1, 7, 9, 0, 0),
                100.0,
                100.5,
                99.5,
                100.0,
            ),
        ] {
            let _ = strategy.on_bar(&ctx, &bar);
        }

        let finalizations = strategy.risk_gate_session_finalizations();
        assert_eq!(finalizations.len(), 1);
        assert_eq!(
            finalizations[0].session_date,
            NaiveDate::from_ymd_opt(2026, 1, 6).expect("date")
        );
        assert_eq!(finalizations[0].shadow_trade_count, 0);
        assert_eq!(finalizations[0].shadow_pnl_points, 0.0);

        let exported = strategy
            .stage5d_export_runtime_private_extension()
            .expect("source-produced runtime extension must export");
        assert_eq!(exported.runtime_pending_finalizations.len(), 1);
        assert_eq!(
            exported.runtime_pending_finalizations[0].session_date,
            "2026-01-06"
        );
        assert_eq!(
            exported.runtime_pending_finalizations[0].shadow_pnl_points,
            "0.0"
        );
        (strategy, exported)
    }

    fn stage5d_test_seed_riskgate_source_preconditions(
        strategy: &mut crate::hybrid_intraday_runtime::HybridIntradayRuntimeStrategy,
    ) {
        let mut envelope = flat_persisted_fixture();
        if let Value::Object(fields) =
            &mut envelope.strategy_state.strategy_state_json["HybridIntradayRuntime"]
        {
            for key in [
                "active_cycle_id",
                "current_owner",
                "current_side",
                "pending_entry_owner",
                "pending_entry_side",
                "pending_entry_cycle_id",
                "pending_entry_request_id",
                "pending_entry_created_ts_utc",
                "risk_gate_shadow_session_date",
                "risk_gate_shadow_entry_ts_utc",
                "risk_gate_shadow_entry_price",
                "risk_gate_shadow_side",
                "risk_gate_shadow_target_price",
                "risk_gate_shadow_stop_price",
                "risk_gate_pending_session_date",
                "risk_gate_mr_enabled_current_session",
                "risk_gate_rolling_sum_lb120",
                "risk_gate_last_finalized_session_date",
            ] {
                fields.insert(key.to_string(), Value::Null);
            }
            fields.insert("last_position_qty".to_string(), serde_json::json!(0.0));
            fields.insert("entry_ready".to_string(), Value::Bool(true));
            fields.insert("last_day_local".to_string(), Value::Null);
            fields.insert("prev_day_close".to_string(), serde_json::json!(100.0));
            fields.insert("prev_day_range".to_string(), serde_json::json!(4.0));
            fields.insert(
                "risk_gate_shadow_pnl_points".to_string(),
                serde_json::json!(0.0),
            );
            fields.insert(
                "risk_gate_shadow_trade_count".to_string(),
                serde_json::json!(0),
            );
            fields.insert(
                "risk_gate_pending_shadow_pnl_points".to_string(),
                serde_json::json!(0.0),
            );
            fields.insert(
                "risk_gate_pending_shadow_trade_count".to_string(),
                serde_json::json!(0),
            );
            fields.insert(
                "risk_gate_ledger_rows_count".to_string(),
                serde_json::json!(0),
            );
        }
        restore_semantic_state(strategy, &envelope);
    }

    fn stage5d_test_source_current_shadow_strategy(
        bars: Vec<crate::runtime_compat::BarEvent>,
    ) -> crate::hybrid_intraday_runtime::HybridIntradayRuntimeStrategy {
        stage5d_test_source_current_shadow_strategy_with_config(|_| {}, bars)
    }

    fn stage5d_test_source_current_shadow_strategy_with_config(
        mutator: impl FnOnce(&mut crate::hybrid_intraday_runtime::HybridIntradayRuntimeConfig),
        bars: Vec<crate::runtime_compat::BarEvent>,
    ) -> crate::hybrid_intraday_runtime::HybridIntradayRuntimeStrategy {
        let mut strategy = stage5d_test_riskgate_runtime_strategy_with_config(mutator);
        stage5d_test_seed_riskgate_source_preconditions(&mut strategy);
        let ctx = stage5d_test_source_ctx();
        for bar in bars {
            let intents = strategy.on_bar(&ctx, &bar);
            assert!(
                intents.is_empty(),
                "source current-shadow positive fixture must not emit live intents"
            );
        }
        strategy
    }

    fn stage5d_test_assert_source_current_shadow_full_path(
        source_strategy: crate::hybrid_intraday_runtime::HybridIntradayRuntimeStrategy,
        expected_session: Option<&str>,
        expected_side: Option<&str>,
        expected_trade_count: u32,
        expected_pnl_text: &str,
    ) {
        let exported = source_strategy
            .stage5d_export_runtime_private_extension()
            .expect("source current-shadow extension exports");
        assert!(
            exported.runtime_pending_finalizations.is_empty(),
            "current-shadow positive path must not smuggle pending finalizations"
        );
        assert!(
            exported.pending_entry.is_none()
                && exported.partial_entry_timer.is_none()
                && exported.pending_exit.is_none(),
            "current-shadow positive path must be produced without pending lifecycle post-edits"
        );
        let source_state_json =
            serde_json::to_value(source_strategy.state()).expect("source state serializes");
        let source_extension = exported.clone();
        let mut envelope = flat_persisted_fixture();
        envelope.strategy_state.strategy_state_json = source_state_json.clone();
        envelope.runtime_private_extension = exported;
        assert_eq!(
            envelope.strategy_state.strategy_state_json, source_state_json,
            "positive path must use exact source semantic state before Stage 5D binding"
        );
        assert_eq!(
            envelope.runtime_private_extension, source_extension,
            "positive path must use exact source private export before Stage 5D binding"
        );
        let identity = stage5d_test_riskgate_identity_for(&source_strategy, &envelope);
        let source_pnl_points = envelope.strategy_state.strategy_state_json
            ["HybridIntradayRuntime"]["risk_gate_shadow_pnl_points"]
            .as_f64()
            .expect("source shadow pnl");
        let source_shadow_session = envelope.strategy_state.strategy_state_json
            ["HybridIntradayRuntime"]["risk_gate_shadow_session_date"]
            .as_str()
            .map(str::to_string);
        let source_pnl_text =
            crate::hybrid_intraday::format_riskgate_authority_decimal(source_pnl_points)
                .expect("source shadow pnl must be authority-canonical");
        assert_eq!(
            source_pnl_text, expected_pnl_text,
            "test expectation must match source-produced canonical pnl text"
        );
        let mut evidence = stage5d_test_riskgate_evidence_before_source_pending(
            &identity,
            &envelope,
            source_shadow_session.as_deref(),
        );
        evidence.current_shadow_pnl_points = source_pnl_text;
        evidence.ledger_tail_hash =
            stage5d_compute_riskgate_ledger_tail_hash(&evidence).expect("shadow tail hash");
        stage5d_apply_riskgate_evidence_to_envelope(&mut envelope, &evidence);
        bind_fixture_to_strategy_config(&mut envelope, &source_strategy);
        envelope.riskgate.ledger_tail_hash = evidence.ledger_tail_hash.clone();
        envelope.payload_checksum_sha256 = envelope
            .compute_payload_checksum_sha256()
            .expect("source current-shadow checksum");
        stage5d_test_normalize_persisted_checksum(&mut envelope);

        if let Value::Object(fields) =
            &envelope.strategy_state.strategy_state_json["HybridIntradayRuntime"]
        {
            assert_eq!(
                fields
                    .get("risk_gate_shadow_session_date")
                    .and_then(Value::as_str),
                expected_session
            );
            assert_eq!(
                fields
                    .get("risk_gate_shadow_trade_count")
                    .and_then(Value::as_u64),
                Some(expected_trade_count as u64)
            );
            assert_eq!(
                fields.get("risk_gate_shadow_side").and_then(Value::as_str),
                expected_side
            );
        }
        let expected_restored_state_json = envelope.strategy_state.strategy_state_json.clone();

        let (bootstrapped, _strict_envelope, evidence) =
            stage5d_test_bootstrap_strict_envelope_with_strategy(
                envelope,
                source_strategy,
                evidence,
            );
        let injected = expect_stage5d_riskgate_ok(
            stage5d_inject_authoritative_riskgate(bootstrapped, evidence),
            "source-produced current-shadow state must pass full Stage 5D path",
        );
        assert!(injected.recovery_complete());
        let restored = stage5d_test_assert_injected_restores_once(
            injected,
            "source-produced current-shadow state must reach restored transition",
        );
        assert_eq!(
            serde_json::to_value(Strategy::state(restored.stage5d_strategy()))
                .expect("restored source state serializes"),
            expected_restored_state_json,
            "restored transition must preserve exact source-produced current-shadow state"
        );
    }

    fn restore_semantic_state(
        strategy: &mut crate::hybrid_intraday_runtime::HybridIntradayRuntimeStrategy,
        envelope: &Stage5dPersistenceEnvelope,
    ) {
        let state: crate::runtime_compat::StrategyState =
            serde_json::from_value(envelope.strategy_state.strategy_state_json.clone())
                .expect("Stage 5D semantic state must decode as source StrategyState");
        Strategy::set_state(strategy, state);
    }

    fn profile_binding_for(
        strategy: &crate::hybrid_intraday_runtime::HybridIntradayRuntimeStrategy,
    ) -> String {
        let (profile, mr_variant, mr_gate_policy, risk_gate_mode) =
            strategy.stage5c_profile_binding();
        format!("{profile}|{mr_variant}|{mr_gate_policy}|{risk_gate_mode}")
    }

    fn bind_fixture_to_strategy_config(
        envelope: &mut Stage5dPersistenceEnvelope,
        strategy: &crate::hybrid_intraday_runtime::HybridIntradayRuntimeStrategy,
    ) {
        let config_fingerprint = strategy.stage5c_config_fingerprint();
        let canonical_config_fingerprint = strategy.stage5d_canonical_config_fingerprint();
        let profile_binding = profile_binding_for(strategy);
        envelope.binding.stage5c_compat_config_fingerprint = config_fingerprint;
        envelope.binding.stage5d_canonical_config_fingerprint =
            canonical_config_fingerprint.clone();
        envelope.canonical_config_fingerprint = canonical_config_fingerprint;
        envelope.binding.profile_binding = profile_binding;
        envelope.payload_checksum_sha256 = envelope
            .compute_payload_checksum_sha256()
            .expect("checksum recomputation must succeed");
    }

    fn stage5d_test_admission_for_envelope(
        envelope: &Stage5dPersistenceEnvelope,
        position_qty: f64,
    ) -> crate::stage5c_paper_host::Stage5cPaperHostAdmission {
        let position_qty_decimal =
            Decimal::from_f64_retain(position_qty).expect("test position qty must convert");
        let admission = crate::stage5c_paper_host::Stage5cPaperHostAdmission::stage5d_test_new(
            envelope.binding.strategy_id.clone(),
            envelope.binding.account_id.clone(),
            envelope.binding.instrument_id.to_instrument_id(),
            0.5,
            position_qty_decimal,
            envelope.persisted_at_ts_utc,
        );
        if position_qty_decimal == Decimal::ZERO {
            return admission;
        }
        admission.stage5d_test_with_target_open_positions(vec![
            broker_core::BrokerPositionSnapshot {
                account_id: envelope.binding.account_id.clone(),
                instrument: envelope.binding.instrument_id.to_instrument_id(),
                qty: position_qty_decimal,
                avg_price: Some(Decimal::new(222_750, 2)),
                unrealized_pnl: None,
                source_ts: Some(envelope.persisted_at_ts_utc),
                received_ts: envelope.persisted_at_ts_utc,
            },
        ])
    }

    fn stage5d_test_normalize_persisted_checksum(envelope: &mut Stage5dPersistenceEnvelope) {
        let payload =
            serde_json::to_string(envelope).expect("serialize envelope for normalization");
        let mut normalized: Stage5dPersistenceEnvelope =
            serde_json::from_str(&payload).expect("deserialize normalized envelope");
        normalized.payload_checksum_sha256 = normalized
            .compute_payload_checksum_sha256()
            .expect("normalized checksum");
        envelope.payload_checksum_sha256 = normalized.payload_checksum_sha256;
    }

    fn load_origin_for_envelope(
        envelope: &Stage5dPersistenceEnvelope,
    ) -> crate::stage5c_paper_host::Stage5cRuntimeStateLoadOrigin {
        crate::stage5c_paper_host::Stage5cRuntimeStateLoadOrigin::Persisted {
            semantic_payload_fingerprint:
                crate::stage5c_paper_host::stage5c_semantic_value_fingerprint(
                    &envelope.strategy_state.strategy_state_json,
                )
                .expect("semantic fingerprint must serialize"),
            persisted_ts: envelope.persisted_at_ts_utc,
            recovery_index_fingerprint:
                crate::stage5c_paper_host::stage5c_recovery_index_fingerprint(
                    &envelope.recovery_indexes.known_order_ids,
                    &envelope.recovery_indexes.pending_requests,
                )
                .expect("recovery fingerprint must serialize"),
        }
    }

    fn apply_valid_fixture() -> Stage5dPersistenceEnvelope {
        fixture_with_mutated_envelope(|envelope| {
            let entry = envelope
                .runtime_private_extension
                .pending_entry
                .as_mut()
                .expect("pending entry");
            entry.owner = Stage5dOwner::MeanReversion;
            entry.side = Stage5dSide::Long;
            entry.entry_style = Stage5dEntryStyle::Bracket;
            entry.target_qty = "3".to_string();
            entry.reason = Stage5dLifecycleReason::MorningMeanReversionLong;
            entry.stop_price = Some("2200.0".to_string());
            entry.take_price = Some("2250.0".to_string());
        })
    }

    fn apply_valid_fixture_with(
        mutator: impl FnOnce(&mut Stage5dPersistenceEnvelope),
    ) -> Stage5dPersistenceEnvelope {
        let mut envelope = apply_valid_fixture();
        mutator(&mut envelope);
        envelope.payload_checksum_sha256 = envelope
            .compute_payload_checksum_sha256()
            .expect("checksum recomputation must succeed");
        envelope
    }

    fn flat_persisted_fixture() -> Stage5dPersistenceEnvelope {
        apply_valid_fixture_with(|envelope| {
            if let Value::Object(fields) =
                &mut envelope.strategy_state.strategy_state_json["HybridIntradayRuntime"]
            {
                for key in [
                    "active_cycle_id",
                    "current_owner",
                    "current_side",
                    "pending_entry_owner",
                    "pending_entry_side",
                    "pending_entry_cycle_id",
                    "pending_entry_request_id",
                    "pending_entry_created_ts_utc",
                    "mr_take_price",
                    "mr_stop_price",
                ] {
                    fields.insert(key.to_string(), Value::Null);
                }
                fields.insert("last_position_qty".to_string(), serde_json::json!(0.0));
            }
            envelope.recovery_indexes.known_order_ids.clear();
            envelope.recovery_indexes.known_stop_order_ids.clear();
            envelope.recovery_indexes.known_trade_ids.clear();
            envelope.recovery_indexes.known_client_order_ids.clear();
            envelope.recovery_indexes.pending_requests.clear();
            envelope.runtime_private_extension.pending_entry = None;
            envelope.runtime_private_extension.partial_entry_timer = None;
            envelope
                .runtime_private_extension
                .expected_working_sets
                .expected_working_order_ids
                .clear();
            envelope
                .runtime_private_extension
                .expected_working_sets
                .expected_working_stop_order_ids
                .clear();
        })
    }

    fn flat_pending_entry_fixture(
        mutator: impl FnOnce(&mut Stage5dPersistenceEnvelope),
    ) -> Stage5dPersistenceEnvelope {
        apply_valid_fixture_with(|envelope| {
            if let Value::Object(fields) =
                &mut envelope.strategy_state.strategy_state_json["HybridIntradayRuntime"]
            {
                fields.insert("last_position_qty".to_string(), serde_json::json!(0.0));
                fields.insert("current_owner".to_string(), Value::Null);
                fields.insert("current_side".to_string(), Value::Null);
            }
            envelope.runtime_private_extension.partial_entry_timer = None;
            mutator(envelope);
        })
    }

    fn stage5d_bound_test_fixture() -> (
        crate::stage5c_paper_host::Stage5cRuntimeStateLoadedPaperStrategy,
        Stage5dPersistenceEnvelope,
    ) {
        let mut envelope = apply_valid_fixture();
        let mut strategy = stage5d_test_strategy_with_config(|config| {
            config.qty = 3.0;
        });
        restore_semantic_state(&mut strategy, &envelope);
        bind_fixture_to_strategy_config(&mut envelope, &strategy);
        let admission = crate::stage5c_paper_host::Stage5cPaperHostAdmission::stage5d_test_new(
            envelope.binding.strategy_id.clone(),
            envelope.binding.account_id.clone(),
            envelope.binding.instrument_id.to_instrument_id(),
            0.5,
            Decimal::new(5, 1),
            envelope.persisted_at_ts_utc,
        );
        let restored = crate::runtime_compat::RuntimeStateRestored {
            known_order_ids: envelope.recovery_indexes.known_order_ids.clone(),
            pending_requests: envelope.recovery_indexes.pending_requests.clone(),
        };
        (
            crate::stage5c_paper_host::Stage5cRuntimeStateLoadedPaperStrategy::stage5d_test_loaded_from_parts(
                strategy,
                admission,
                restored,
                load_origin_for_envelope(&envelope),
            ),
            envelope,
        )
    }

    fn stage5d_loaded_for_envelope(
        mut envelope: Stage5dPersistenceEnvelope,
        strategy: crate::hybrid_intraday_runtime::HybridIntradayRuntimeStrategy,
        load_origin: crate::stage5c_paper_host::Stage5cRuntimeStateLoadOrigin,
    ) -> (
        crate::stage5c_paper_host::Stage5cRuntimeStateLoadedPaperStrategy,
        Stage5dPersistenceEnvelope,
    ) {
        let mut strategy = strategy;
        restore_semantic_state(&mut strategy, &envelope);
        bind_fixture_to_strategy_config(&mut envelope, &strategy);
        let position_qty = match &envelope.strategy_state.strategy_state_json {
            Value::Object(root) => root
                .get("HybridIntradayRuntime")
                .and_then(|state| state.get("last_position_qty"))
                .and_then(|qty| qty.as_f64())
                .unwrap_or_default(),
            _ => 0.0,
        };
        let admission = stage5d_test_admission_for_envelope(&envelope, position_qty);
        let restored = crate::runtime_compat::RuntimeStateRestored {
            known_order_ids: envelope.recovery_indexes.known_order_ids.clone(),
            pending_requests: envelope.recovery_indexes.pending_requests.clone(),
        };
        (
            crate::stage5c_paper_host::Stage5cRuntimeStateLoadedPaperStrategy::stage5d_test_loaded_from_parts(
                strategy,
                admission,
                restored,
                load_origin,
            ),
            envelope,
        )
    }

    fn expect_stage5d_blocked<T>(
        result: Result<T, Stage5dRuntimePrivateApplyBlocked>,
        message: &str,
    ) -> Stage5dRuntimePrivateApplyBlocked {
        match result {
            Ok(_) => panic!("{message}"),
            Err(blocked) => blocked,
        }
    }

    fn expect_stage5d_ok<T>(
        result: Result<T, Stage5dRuntimePrivateApplyBlocked>,
        message: &str,
    ) -> T {
        match result {
            Ok(value) => value,
            Err(blocked) => panic!("{message}: {:?}", blocked.reason()),
        }
    }

    fn expect_stage5d_bootstrap_blocked<T>(
        result: Result<T, Stage5dBootstrapBlocked>,
        message: &str,
    ) -> Stage5dBootstrapBlocked {
        match result {
            Ok(_) => panic!("{message}"),
            Err(blocked) => blocked,
        }
    }

    fn expect_stage5d_bootstrap_ok<T>(
        result: Result<T, Stage5dBootstrapBlocked>,
        message: &str,
    ) -> T {
        match result {
            Ok(value) => value,
            Err(blocked) => panic!("{message}: {:?}", blocked.reason()),
        }
    }

    fn expect_stage5d_riskgate_blocked<T>(
        result: Result<T, Stage5dRiskGateInjectionBlocked>,
        message: &str,
    ) -> Stage5dRiskGateInjectionBlocked {
        match result {
            Ok(_) => panic!("{message}"),
            Err(blocked) => blocked,
        }
    }

    fn expect_stage5d_riskgate_ok<T>(
        result: Result<T, Stage5dRiskGateInjectionBlocked>,
        message: &str,
    ) -> T {
        match result {
            Ok(value) => value,
            Err(blocked) => panic!("{message}: {:?}", blocked.reason()),
        }
    }

    fn expect_stage5d_restore_blocked<T>(
        result: Result<T, Stage5dRuntimeStateRestoreOutcome>,
        message: &str,
    ) -> Stage5dRuntimeStateRestoreBlocked {
        match result {
            Ok(_) => panic!("{message}"),
            Err(Stage5dRuntimeStateRestoreOutcome::Blocked(blocked)) => blocked,
            Err(Stage5dRuntimeStateRestoreOutcome::Terminal(terminal)) => {
                panic!("{message}: terminal {:?}", terminal.reason())
            }
        }
    }

    fn expect_stage5d_restore_terminal<T>(
        result: Result<T, Stage5dRuntimeStateRestoreOutcome>,
        message: &str,
    ) -> Stage5dRuntimeStateRestoreTerminalFailure {
        match result {
            Ok(_) => panic!("{message}"),
            Err(Stage5dRuntimeStateRestoreOutcome::Terminal(terminal)) => terminal,
            Err(Stage5dRuntimeStateRestoreOutcome::Blocked(blocked)) => {
                panic!("{message}: blocked {:?}", blocked.reason())
            }
        }
    }

    fn expect_stage5d_restore_ok<T>(
        result: Result<T, Stage5dRuntimeStateRestoreOutcome>,
        message: &str,
    ) -> T {
        match result {
            Ok(value) => value,
            Err(Stage5dRuntimeStateRestoreOutcome::Blocked(blocked)) => {
                panic!("{message}: blocked {:?}", blocked.reason())
            }
            Err(Stage5dRuntimeStateRestoreOutcome::Terminal(terminal)) => {
                panic!("{message}: terminal {:?}", terminal.reason())
            }
        }
    }

    fn stage5d_test_reset_restored_callback_count() {
        crate::stage5c_paper_host::stage5d_test_reset_runtime_restored_callback_count();
    }

    fn stage5d_test_restored_callback_count() -> usize {
        crate::stage5c_paper_host::stage5d_test_runtime_restored_callback_count()
    }

    fn stage5d_test_assert_restored_success_baseline(
        restored: &crate::stage5c_paper_host::Stage5cRuntimeStateRestoredPaperStrategy,
    ) {
        assert!(restored.receipt().runtime_state_restored());
        assert!(!restored.receipt().warmup_started());
        assert!(!restored.receipt().pending_recovery_started());
        assert!(!restored.receipt().semantic_bar_enabled());
        assert!(!restored.receipt().intent_sink_attached());
    }

    fn stage5d_test_assert_injected_restores_once(
        injected: Stage5dRiskGateInjectedPaperStrategy,
        label: &str,
    ) -> crate::stage5c_paper_host::Stage5cRuntimeStateRestoredPaperStrategy {
        let restored_at = injected.envelope.persisted_at_ts_utc;
        stage5d_test_reset_restored_callback_count();
        let restored = expect_stage5d_restore_ok(
            stage5d_notify_runtime_state_restored_at(injected, restored_at),
            label,
        );
        assert_eq!(stage5d_test_restored_callback_count(), 1, "{label}");
        stage5d_test_assert_restored_success_baseline(&restored);
        restored
    }

    fn stage5d_test_assert_injected_restores_indexes_once(
        injected: Stage5dRiskGateInjectedPaperStrategy,
        expected_known_order_ids: &[BrokerOrderId],
        expected_pending_requests: &[StrategyRequestId],
        label: &str,
    ) -> crate::stage5c_paper_host::Stage5cRuntimeStateRestoredPaperStrategy {
        let restored = stage5d_test_assert_injected_restores_once(injected, label);
        assert_eq!(
            restored.receipt().stage5d_test_known_order_ids(),
            expected_known_order_ids,
            "{label}: known-order index must be preserved in restored receipt"
        );
        assert_eq!(
            restored.receipt().pending_requests(),
            expected_pending_requests,
            "{label}: pending-request index must be preserved in restored receipt"
        );
        restored
    }

    fn stage5d_test_assert_restore_blocks_before_callback(
        injected: Stage5dRiskGateInjectedPaperStrategy,
        restored_at: DateTime<Utc>,
        expected: Stage5dRuntimeStateRestoreBlockedReason,
        label: &str,
    ) -> Stage5dRuntimeStateRestoreBlocked {
        let snapshot_id = injected.snapshot_id().to_string();
        let evidence = injected.evidence_fingerprint().to_string();
        let recovery = injected.recovery_plan_fingerprint().to_string();
        let admission = injected.bootstrapped.stage5d_admission();
        let expected_boundary_flags = (
            admission.is_paper_only(),
            admission.runtime_host_attached(),
            admission.intent_sink_attached(),
        );
        if expected != Stage5dRuntimeStateRestoreBlockedReason::ClosedBoundaryOpened {
            assert!(
                expected_boundary_flags.0,
                "{label}: non-boundary block must retain paper-only admission"
            );
            assert!(
                !expected_boundary_flags.1,
                "{label}: non-boundary block must not attach runtime host"
            );
            assert!(
                !expected_boundary_flags.2,
                "{label}: non-boundary block must not attach intent sink"
            );
        } else {
            assert!(
                !expected_boundary_flags.0 || expected_boundary_flags.1 || expected_boundary_flags.2,
                "{label}: ClosedBoundaryOpened fixture must deliberately mutate a closed-boundary flag"
            );
        }
        let strategy_fingerprint =
            stage5d_test_strategy_state_fingerprint(injected.bootstrapped.stage5d_strategy());
        stage5d_test_reset_restored_callback_count();
        let blocked = expect_stage5d_restore_blocked(
            stage5d_notify_runtime_state_restored_at(injected, restored_at),
            label,
        );
        assert_eq!(stage5d_test_restored_callback_count(), 0, "{label}");
        assert_eq!(blocked.reason(), expected, "{label}");
        assert!(blocked.input_capability_preserved(), "{label}");
        assert_eq!(blocked.snapshot_id(), snapshot_id, "{label}");
        assert_eq!(blocked.evidence_fingerprint(), evidence, "{label}");
        assert_eq!(blocked.recovery_plan_fingerprint(), recovery, "{label}");
        assert_eq!(
            blocked.stage5d_test_strategy_state_fingerprint(),
            strategy_fingerprint,
            "{label}: retained capability must keep strategy-state fingerprint unchanged"
        );
        assert_eq!(
            blocked.stage5d_test_closed_boundary_flags(),
            expected_boundary_flags,
            "{label}: retained capability must preserve closed-boundary flags exactly"
        );
        assert!(
            blocked.input_capability_preserved(),
            "r6 representable blockers use common callback-zero helper"
        );
        blocked
    }

    fn stage5d_test_complete_injected_fixture() -> Stage5dRiskGateInjectedPaperStrategy {
        stage5d_test_complete_injected_fixture_with_position_and_bootstrap_delay(
            0.0,
            None,
            chrono::Duration::zero(),
        )
    }

    fn stage5d_test_complete_injected_fixture_with_position_and_bootstrap_delay(
        qty: f64,
        side: Option<&str>,
        bootstrap_delay: chrono::Duration,
    ) -> Stage5dRiskGateInjectedPaperStrategy {
        let (bootstrapped, _envelope, validated_evidence) =
            riskgate_enabled_bootstrapped_fixture_with_bootstrap_delay(
                bootstrap_delay,
                |envelope, evidence| {
                    stage5d_test_set_position_side(envelope, qty, side);
                    stage5d_test_configure_single_tail_crash_frontier(
                        envelope,
                        evidence,
                        Stage5dRiskGateFinalizationState::AcknowledgedInRuntime,
                        true,
                        true,
                        true,
                        false,
                    );
                },
            );
        let injected = expect_stage5d_riskgate_ok(
            stage5d_inject_authoritative_riskgate(bootstrapped, validated_evidence),
            "complete fixture must inject riskgate",
        );
        assert!(
            injected.recovery_complete(),
            "complete fixture must be ready for restored callback"
        );
        injected
    }

    fn stage5d_test_complete_strict_injected_fixture_with_position(
        qty: f64,
        side: Option<&str>,
    ) -> Stage5dRiskGateInjectedPaperStrategy {
        let (bootstrapped, _envelope, validated_evidence) =
            riskgate_enabled_strict_bootstrapped_fixture_with_evidence(|envelope, evidence| {
                stage5d_test_set_position_side(envelope, qty, side);
                stage5d_test_configure_single_tail_crash_frontier(
                    envelope,
                    evidence,
                    Stage5dRiskGateFinalizationState::AcknowledgedInRuntime,
                    true,
                    true,
                    true,
                    false,
                );
            });
        let injected = expect_stage5d_riskgate_ok(
            stage5d_inject_authoritative_riskgate(bootstrapped, validated_evidence),
            "r5 strict round-trip fixture must inject riskgate",
        );
        assert!(
            injected.recovery_complete(),
            "r5 strict round-trip fixture must be ready for restored callback"
        );
        injected
    }

    fn stage5d_test_complete_injected_pending_entry_index_fixture(
    ) -> Stage5dRiskGateInjectedPaperStrategy {
        let request_id = StrategyRequestId::new(
            uuid::Uuid::parse_str("00000000-0000-0000-0000-0000000005d4").expect("uuid"),
        );
        let cycle_id = "5d4c1e0001";
        let created_ts = 1_784_009_340_i64;
        let (bootstrapped, _envelope, validated_evidence) =
            riskgate_enabled_strict_bootstrapped_fixture_with_evidence(|envelope, evidence| {
                stage5d_test_set_position_side(envelope, 0.5, Some("long"));
                if let Value::Object(fields) =
                    &mut envelope.strategy_state.strategy_state_json["HybridIntradayRuntime"]
                {
                    fields.insert(
                        "active_cycle_id".to_string(),
                        Value::String(cycle_id.to_string()),
                    );
                    fields.insert(
                        "current_owner".to_string(),
                        Value::String("mean_reversion".to_string()),
                    );
                    fields.insert(
                        "pending_entry_owner".to_string(),
                        Value::String("mean_reversion".to_string()),
                    );
                    fields.insert(
                        "pending_entry_side".to_string(),
                        Value::String("long".to_string()),
                    );
                    fields.insert(
                        "pending_entry_cycle_id".to_string(),
                        Value::String(cycle_id.to_string()),
                    );
                    fields.insert(
                        "pending_entry_request_id".to_string(),
                        serde_json::json!(request_id),
                    );
                    fields.insert(
                        "pending_entry_created_ts_utc".to_string(),
                        serde_json::json!(created_ts),
                    );
                }
                envelope.recovery_indexes.pending_requests = vec![request_id];
                envelope.runtime_private_extension.pending_entry =
                    Some(Stage5dPendingEntryExtension {
                        owner: Stage5dOwner::MeanReversion,
                        side: Stage5dSide::Long,
                        reason: Stage5dLifecycleReason::MorningMeanReversionLong,
                        entry_style: Stage5dEntryStyle::Bracket,
                        target_qty: "3".to_string(),
                        stop_price: Some("2200.0".to_string()),
                        take_price: Some("2250.0".to_string()),
                        request_id: Some(request_id),
                    });
                envelope.runtime_private_extension.partial_entry_timer =
                    Some(Stage5dPartialEntryTimer {
                        partial_started_at_ms: (created_ts * 1000) + 1,
                    });
                stage5d_test_configure_single_tail_crash_frontier(
                    envelope,
                    evidence,
                    Stage5dRiskGateFinalizationState::AcknowledgedInRuntime,
                    true,
                    true,
                    true,
                    false,
                );
            });
        let injected = expect_stage5d_riskgate_ok(
            stage5d_inject_authoritative_riskgate(bootstrapped, validated_evidence),
            "pending-entry index fixture must inject riskgate",
        );
        assert!(
            injected.recovery_complete(),
            "pending-entry index fixture must be ready for restored callback"
        );
        injected
    }

    fn stage5d_test_set_position_side(
        envelope: &mut Stage5dPersistenceEnvelope,
        qty: f64,
        side: Option<&str>,
    ) {
        if let Value::Object(fields) =
            &mut envelope.strategy_state.strategy_state_json["HybridIntradayRuntime"]
        {
            fields.insert("last_position_qty".to_string(), serde_json::json!(qty));
            fields.insert(
                "current_side".to_string(),
                side.map(|value| Value::String(value.to_string()))
                    .unwrap_or(Value::Null),
            );
            if qty.abs() <= f64::EPSILON {
                fields.insert("active_cycle_id".to_string(), Value::Null);
                fields.insert("current_owner".to_string(), Value::Null);
            } else {
                fields.insert(
                    "active_cycle_id".to_string(),
                    Value::String("5d2bd10001".to_string()),
                );
                fields.insert(
                    "current_owner".to_string(),
                    Value::String("intraday_breakout".to_string()),
                );
            }
        }
    }

    fn applied_stage5d_fixture() -> (
        Stage5dPrivateStateAppliedPaperStrategy,
        Stage5dPersistenceEnvelope,
    ) {
        applied_stage5d_fixture_with(|_| {}, |admission| admission)
    }

    fn applied_stage5d_fixture_with(
        envelope_mutator: impl FnOnce(&mut Stage5dPersistenceEnvelope),
        admission_mutator: impl FnOnce(
            crate::stage5c_paper_host::Stage5cPaperHostAdmission,
        ) -> crate::stage5c_paper_host::Stage5cPaperHostAdmission,
    ) -> (
        Stage5dPrivateStateAppliedPaperStrategy,
        Stage5dPersistenceEnvelope,
    ) {
        let mut envelope = flat_persisted_fixture();
        envelope_mutator(&mut envelope);
        let mut strategy = stage5d_test_strategy_with_config(|config| {
            config.qty = 3.0;
        });
        restore_semantic_state(&mut strategy, &envelope);
        bind_fixture_to_strategy_config(&mut envelope, &strategy);
        let position_qty = match &envelope.strategy_state.strategy_state_json {
            Value::Object(root) => root
                .get("HybridIntradayRuntime")
                .and_then(|state| state.get("last_position_qty"))
                .and_then(|qty| qty.as_f64())
                .unwrap_or_default(),
            _ => 0.0,
        };
        let admission = stage5d_test_admission_for_envelope(&envelope, position_qty);
        let restored = crate::runtime_compat::RuntimeStateRestored {
            known_order_ids: envelope.recovery_indexes.known_order_ids.clone(),
            pending_requests: envelope.recovery_indexes.pending_requests.clone(),
        };
        let loaded = crate::stage5c_paper_host::Stage5cRuntimeStateLoadedPaperStrategy::stage5d_test_loaded_from_parts(
            strategy,
            admission_mutator(admission),
            restored,
            load_origin_for_envelope(&envelope),
        );
        let validated = envelope
            .validate_restore_contract_schema_only()
            .expect("mutated envelope remains schema valid");
        let bound = expect_stage5d_ok(
            stage5d_bind_runtime_state_loaded(loaded, validated),
            "mutated fixture must bind",
        );
        let applied = expect_stage5d_ok(
            stage5d_apply_runtime_private_extension(bound),
            "mutated fixture must apply",
        );
        (applied, envelope)
    }

    fn riskgate_enabled_bootstrapped_fixture_with(
        envelope_mutator: impl FnOnce(&mut Stage5dPersistenceEnvelope),
    ) -> (
        Stage5dBootstrappedPaperStrategy,
        Stage5dPersistenceEnvelope,
        Stage5dValidatedRiskGateLedgerEvidence,
    ) {
        riskgate_enabled_bootstrapped_fixture_with_evidence(|envelope, _evidence| {
            envelope_mutator(envelope);
        })
    }

    fn riskgate_enabled_bootstrapped_fixture_with_evidence(
        mutator: impl FnOnce(&mut Stage5dPersistenceEnvelope, &mut Stage5dRiskGateLedgerEvidence),
    ) -> (
        Stage5dBootstrappedPaperStrategy,
        Stage5dPersistenceEnvelope,
        Stage5dValidatedRiskGateLedgerEvidence,
    ) {
        riskgate_enabled_bootstrapped_fixture_with_config_evidence_and_bootstrap_delay(
            |_| {},
            mutator,
            chrono::Duration::zero(),
        )
    }

    fn riskgate_enabled_bootstrapped_fixture_with_bootstrap_delay(
        bootstrap_delay: chrono::Duration,
        mutator: impl FnOnce(&mut Stage5dPersistenceEnvelope, &mut Stage5dRiskGateLedgerEvidence),
    ) -> (
        Stage5dBootstrappedPaperStrategy,
        Stage5dPersistenceEnvelope,
        Stage5dValidatedRiskGateLedgerEvidence,
    ) {
        riskgate_enabled_bootstrapped_fixture_with_config_evidence_and_bootstrap_delay(
            |_| {},
            mutator,
            bootstrap_delay,
        )
    }

    fn riskgate_enabled_bootstrapped_fixture_with_config_and_evidence(
        config_mutator: impl FnOnce(&mut crate::hybrid_intraday_runtime::HybridIntradayRuntimeConfig),
        mutator: impl FnOnce(&mut Stage5dPersistenceEnvelope, &mut Stage5dRiskGateLedgerEvidence),
    ) -> (
        Stage5dBootstrappedPaperStrategy,
        Stage5dPersistenceEnvelope,
        Stage5dValidatedRiskGateLedgerEvidence,
    ) {
        riskgate_enabled_bootstrapped_fixture_with_config_evidence_and_bootstrap_delay(
            config_mutator,
            mutator,
            chrono::Duration::zero(),
        )
    }

    fn riskgate_enabled_bootstrapped_fixture_with_config_evidence_and_bootstrap_delay(
        config_mutator: impl FnOnce(&mut crate::hybrid_intraday_runtime::HybridIntradayRuntimeConfig),
        mutator: impl FnOnce(&mut Stage5dPersistenceEnvelope, &mut Stage5dRiskGateLedgerEvidence),
        bootstrap_delay: chrono::Duration,
    ) -> (
        Stage5dBootstrappedPaperStrategy,
        Stage5dPersistenceEnvelope,
        Stage5dValidatedRiskGateLedgerEvidence,
    ) {
        let mut envelope = flat_persisted_fixture();
        let mut strategy = stage5d_test_strategy_with_config(|config| {
            config.profile =
                crate::hybrid_intraday_runtime::HybridIntradayProfile::ImoexfPrimaryRiskgateHigh180Lb120;
            config.mr_variant = crate::hybrid_intraday_runtime::MeanReversionVariant::High180;
            config.mr_gate_policy =
                crate::hybrid_intraday_runtime::MrGatePolicy::ShadowPnlLb120Positive;
            config.risk_gate_mode = crate::hybrid_intraday_runtime::RiskGateMode::NormalAppend;
            config.qty = 3.0;
            config_mutator(config);
        });
        let identity = stage5d_test_riskgate_identity_for(&strategy, &envelope);
        let mut evidence = stage5d_test_riskgate_evidence_for(&identity, &envelope);
        stage5d_apply_riskgate_evidence_to_envelope(&mut envelope, &evidence);
        mutator(&mut envelope, &mut evidence);
        evidence = stage5d_test_rebuild_evidence_from_envelope(&envelope, &evidence);
        envelope.riskgate.ledger_tail_hash = evidence.ledger_tail_hash.clone();
        envelope.payload_checksum_sha256 = envelope
            .compute_payload_checksum_sha256()
            .expect("checksum recomputation must succeed");
        restore_semantic_state(&mut strategy, &envelope);
        bind_fixture_to_strategy_config(&mut envelope, &strategy);
        let evidence = stage5d_test_rebuild_evidence_from_envelope(&envelope, &evidence);
        let validated_evidence = stage5d_validate_riskgate_ledger_evidence(evidence)
            .expect("riskgate ledger evidence must validate");
        let position_qty = match &envelope.strategy_state.strategy_state_json {
            Value::Object(root) => root
                .get("HybridIntradayRuntime")
                .and_then(|state| state.get("last_position_qty"))
                .and_then(|qty| qty.as_f64())
                .unwrap_or_default(),
            _ => 0.0,
        };
        let admission = stage5d_test_admission_for_envelope(&envelope, position_qty);
        let restored = crate::runtime_compat::RuntimeStateRestored {
            known_order_ids: envelope.recovery_indexes.known_order_ids.clone(),
            pending_requests: envelope.recovery_indexes.pending_requests.clone(),
        };
        let loaded = crate::stage5c_paper_host::Stage5cRuntimeStateLoadedPaperStrategy::stage5d_test_loaded_from_parts(
            strategy,
            admission,
            restored,
            load_origin_for_envelope(&envelope),
        );
        let validated = envelope
            .validate_restore_contract_schema_only()
            .expect("riskgate fixture remains schema valid");
        let bound = expect_stage5d_ok(
            stage5d_bind_runtime_state_loaded(loaded, validated),
            "riskgate fixture must bind",
        );
        let applied = expect_stage5d_ok(
            stage5d_apply_runtime_private_extension(bound),
            "riskgate fixture private extension must apply",
        );
        let bootstrap_notification_ts = envelope.persisted_at_ts_utc + bootstrap_delay;
        let bootstrapped = expect_stage5d_bootstrap_ok(
            stage5d_notify_broker_truth_bootstrap_at(applied, bootstrap_notification_ts),
            "riskgate fixture bootstrap must succeed",
        );
        (bootstrapped, envelope, validated_evidence)
    }

    fn riskgate_enabled_strict_bootstrapped_fixture_with_evidence(
        mutator: impl FnOnce(&mut Stage5dPersistenceEnvelope, &mut Stage5dRiskGateLedgerEvidence),
    ) -> (
        Stage5dBootstrappedPaperStrategy,
        Stage5dPersistenceEnvelope,
        Stage5dValidatedRiskGateLedgerEvidence,
    ) {
        let mut envelope = flat_persisted_fixture();
        let mut strategy = stage5d_test_strategy_with_config(|config| {
            config.profile =
                crate::hybrid_intraday_runtime::HybridIntradayProfile::ImoexfPrimaryRiskgateHigh180Lb120;
            config.mr_variant = crate::hybrid_intraday_runtime::MeanReversionVariant::High180;
            config.mr_gate_policy =
                crate::hybrid_intraday_runtime::MrGatePolicy::ShadowPnlLb120Positive;
            config.risk_gate_mode = crate::hybrid_intraday_runtime::RiskGateMode::NormalAppend;
            config.qty = 3.0;
        });
        let identity = stage5d_test_riskgate_identity_for(&strategy, &envelope);
        let mut evidence = stage5d_test_riskgate_evidence_for(&identity, &envelope);
        stage5d_apply_riskgate_evidence_to_envelope(&mut envelope, &evidence);
        mutator(&mut envelope, &mut evidence);
        evidence = stage5d_test_rebuild_evidence_from_envelope(&envelope, &evidence);
        envelope.riskgate.ledger_tail_hash = evidence.ledger_tail_hash.clone();
        envelope.payload_checksum_sha256 = envelope
            .compute_payload_checksum_sha256()
            .expect("strict r5 checksum after riskgate evidence");
        restore_semantic_state(&mut strategy, &envelope);
        bind_fixture_to_strategy_config(&mut envelope, &strategy);
        evidence = stage5d_test_rebuild_evidence_from_envelope(&envelope, &evidence);
        envelope.riskgate.ledger_tail_hash = evidence.ledger_tail_hash.clone();
        envelope.payload_checksum_sha256 = envelope
            .compute_payload_checksum_sha256()
            .expect("strict r5 checksum after source config binding");
        stage5d_test_bootstrap_strict_envelope_with_strategy(envelope, strategy, evidence)
    }

    fn stage5d_test_bootstrap_strict_envelope_with_strategy(
        envelope: Stage5dPersistenceEnvelope,
        mut strategy: crate::hybrid_intraday_runtime::HybridIntradayRuntimeStrategy,
        evidence: Stage5dRiskGateLedgerEvidence,
    ) -> (
        Stage5dBootstrappedPaperStrategy,
        Stage5dPersistenceEnvelope,
        Stage5dValidatedRiskGateLedgerEvidence,
    ) {
        let payload = serde_json::to_string(&envelope).expect("serialize strict envelope");
        let envelope = Stage5dPersistenceEnvelope::from_json_str_strict(&payload)
            .expect("strict envelope round-trip must pass");
        restore_semantic_state(&mut strategy, &envelope);
        let validated_evidence = stage5d_validate_riskgate_ledger_evidence(evidence)
            .expect("riskgate evidence must validate");
        let position_qty = match &envelope.strategy_state.strategy_state_json {
            Value::Object(root) => root
                .get("HybridIntradayRuntime")
                .and_then(|state| state.get("last_position_qty"))
                .and_then(|qty| qty.as_f64())
                .unwrap_or_default(),
            _ => 0.0,
        };
        let admission = stage5d_test_admission_for_envelope(&envelope, position_qty);
        let restored = crate::runtime_compat::RuntimeStateRestored {
            known_order_ids: envelope.recovery_indexes.known_order_ids.clone(),
            pending_requests: envelope.recovery_indexes.pending_requests.clone(),
        };
        let loaded = crate::stage5c_paper_host::Stage5cRuntimeStateLoadedPaperStrategy::stage5d_test_loaded_from_parts(
            strategy,
            admission,
            restored,
            load_origin_for_envelope(&envelope),
        );
        let validated = envelope
            .clone()
            .validate_restore_contract_schema_only()
            .expect("strict envelope remains valid");
        let bound = expect_stage5d_ok(
            stage5d_bind_runtime_state_loaded(loaded, validated),
            "strict envelope must bind",
        );
        let applied = expect_stage5d_ok(
            stage5d_apply_runtime_private_extension(bound),
            "strict envelope private extension must apply",
        );
        let bootstrapped = expect_stage5d_bootstrap_ok(
            stage5d_notify_broker_truth_bootstrap_at(applied, envelope.persisted_at_ts_utc),
            "strict envelope broker bootstrap must succeed",
        );
        (bootstrapped, envelope, validated_evidence)
    }

    fn align_riskgate_outbox_to_runtime_pending(envelope: &mut Stage5dPersistenceEnvelope) {
        let identity = envelope.riskgate.identity.clone();
        envelope.riskgate.durable_finalization_outbox = envelope
            .runtime_private_extension
            .runtime_pending_finalizations
            .iter()
            .enumerate()
            .map(|(index, pending)| Stage5dRiskGateFinalizationOutboxRecord {
                session_date: pending.session_date.clone(),
                generation: (index as u64) + 1,
                state: Stage5dRiskGateFinalizationState::Prepared,
                identity_hash: stage5d_riskgate_outbox_identity_hash(
                    &identity,
                    NaiveDate::parse_from_str(&pending.session_date, "%Y-%m-%d")
                        .expect("pending date"),
                    (index as u64) + 1,
                ),
            })
            .collect();
    }

    fn stage5d_test_riskgate_identity_for(
        strategy: &crate::hybrid_intraday_runtime::HybridIntradayRuntimeStrategy,
        envelope: &Stage5dPersistenceEnvelope,
    ) -> Stage5dRiskGateIdentity {
        let source =
            strategy.stage5d_expected_riskgate_identity(envelope.binding.strategy_id.clone());
        Stage5dRiskGateIdentity {
            strategy_id: source.strategy_id,
            profile_id: source.profile_id,
            mr_variant: source.mr_variant,
            timeframe: source.timeframe,
            session_policy: source.session_policy,
            model_version: source.model_version,
        }
    }

    fn stage5d_test_riskgate_evidence_for(
        identity: &Stage5dRiskGateIdentity,
        envelope: &Stage5dPersistenceEnvelope,
    ) -> Stage5dRiskGateLedgerEvidence {
        let source_identity = crate::hybrid_intraday::RiskGateProfileIdentity {
            strategy_id: identity.strategy_id.clone(),
            profile_id: identity.profile_id.clone(),
            mr_variant: identity.mr_variant.clone(),
            timeframe: identity.timeframe.clone(),
            session_policy: identity.session_policy.clone(),
            model_version: identity.model_version.clone(),
        };
        let mut date = NaiveDate::from_ymd_opt(2026, 3, 2).expect("date");
        let mut rows = Vec::new();
        while rows.len() < 65 {
            if !matches!(date.weekday(), Weekday::Sat | Weekday::Sun) {
                let idx = rows.len() as f64;
                let mut row = crate::hybrid_intraday::build_runtime_session_row(
                    &rows,
                    date,
                    if (idx as i64) % 2 == 0 { 2.0 } else { -0.5 },
                    1,
                )
                .expect("source runtime row");
                if rows.len() < 60 {
                    row.source = crate::hybrid_intraday::RiskGateRowSource::Seed;
                }
                rows.push(row);
            }
            date += chrono::Duration::days(1);
        }
        let records = crate::hybrid_intraday::build_ledger_records_from_rows(
            &rows,
            &source_identity,
            envelope.persisted_at_ts_utc.timestamp(),
        )
        .expect("source ledger records");
        let ledger_records = records
            .iter()
            .map(stage5d_test_stage_record_from_source)
            .collect::<Vec<_>>();
        let mut evidence = Stage5dRiskGateLedgerEvidence {
            schema_version: STAGE5D_RISKGATE_SCHEMA_VERSION,
            identity: identity.clone(),
            ledger_tail_hash: String::new(),
            ledger_records,
            seed_loaded: true,
            current_shadow_session_date: Some("2026-07-14".to_string()),
            current_shadow_pnl_points: "0.0".to_string(),
            current_generation: crate::hybrid_intraday::RISK_GATE_STATE_GENERATION.to_string(),
        };
        evidence.ledger_tail_hash =
            stage5d_compute_riskgate_ledger_tail_hash(&evidence).expect("tail hash");
        evidence
    }

    fn stage5d_test_riskgate_evidence_before_source_pending(
        identity: &Stage5dRiskGateIdentity,
        envelope: &Stage5dPersistenceEnvelope,
        current_shadow_session_date: Option<&str>,
    ) -> Stage5dRiskGateLedgerEvidence {
        let source_identity = crate::hybrid_intraday::RiskGateProfileIdentity {
            strategy_id: identity.strategy_id.clone(),
            profile_id: identity.profile_id.clone(),
            mr_variant: identity.mr_variant.clone(),
            timeframe: identity.timeframe.clone(),
            session_policy: identity.session_policy.clone(),
            model_version: identity.model_version.clone(),
        };
        let mut date = NaiveDate::from_ymd_opt(2025, 10, 13).expect("date");
        let stop = NaiveDate::from_ymd_opt(2026, 1, 6).expect("date");
        let mut rows = Vec::new();
        while date < stop {
            if !matches!(date.weekday(), Weekday::Sat | Weekday::Sun) {
                let idx = rows.len() as i64;
                let mut row = crate::hybrid_intraday::build_runtime_session_row(
                    &rows,
                    date,
                    if idx % 2 == 0 { 2.0 } else { -0.5 },
                    1,
                )
                .expect("source seed row");
                row.source = crate::hybrid_intraday::RiskGateRowSource::Seed;
                rows.push(row);
            }
            date += chrono::Duration::days(1);
        }
        assert!(
            rows.len() >= crate::hybrid_intraday::SHADOW_PNL_LB120_MIN_HISTORY_SESSIONS,
            "source evidence must cover riskgate minimum history"
        );
        assert_eq!(
            rows.last().expect("last seed").session_date,
            NaiveDate::from_ymd_opt(2026, 1, 5).expect("date")
        );
        let records = crate::hybrid_intraday::build_ledger_records_from_rows(
            &rows,
            &source_identity,
            envelope.persisted_at_ts_utc.timestamp(),
        )
        .expect("source ledger records");
        let ledger_records = records
            .iter()
            .map(stage5d_test_stage_record_from_source)
            .collect::<Vec<_>>();
        let mut evidence = Stage5dRiskGateLedgerEvidence {
            schema_version: STAGE5D_RISKGATE_SCHEMA_VERSION,
            identity: identity.clone(),
            ledger_tail_hash: String::new(),
            ledger_records,
            seed_loaded: true,
            current_shadow_session_date: current_shadow_session_date.map(str::to_string),
            current_shadow_pnl_points: "0.0".to_string(),
            current_generation: crate::hybrid_intraday::RISK_GATE_STATE_GENERATION.to_string(),
        };
        evidence.ledger_tail_hash =
            stage5d_compute_riskgate_ledger_tail_hash(&evidence).expect("tail hash");
        evidence
    }

    fn stage5d_test_stage_record_from_source(
        record: &crate::hybrid_intraday::RiskGateLedgerRecord,
    ) -> Stage5dRiskGateLedgerRecord {
        Stage5dRiskGateLedgerRecord {
            session_date: record.row.session_date.format("%Y-%m-%d").to_string(),
            shadow_pnl_points: stage5d_source_format_riskgate_decimal(record.row.shadow_pnl_points),
            shadow_trade_count: record.row.shadow_trade_count,
            rolling_sum_before_session: stage5d_source_format_riskgate_decimal(
                record.row.rolling_sum_before_session,
            ),
            mr_enabled_for_session: record.row.mr_enabled_for_session,
            source: match record.row.source {
                crate::hybrid_intraday::RiskGateRowSource::Seed => Stage5dRiskGateRowSource::Seed,
                crate::hybrid_intraday::RiskGateRowSource::Runtime => {
                    Stage5dRiskGateRowSource::Runtime
                }
            },
            status: match record.row.status {
                crate::hybrid_intraday::RiskGateRowStatus::Complete => {
                    Stage5dRiskGateRowStatus::Complete
                }
                crate::hybrid_intraday::RiskGateRowStatus::Incomplete => {
                    Stage5dRiskGateRowStatus::Incomplete
                }
            },
            rolling_sum_lb120: stage5d_source_format_riskgate_decimal(record.rolling_sum_lb120),
            mr_enabled_next_session: record.mr_enabled_next_session,
            finalized_at_utc: record.finalized_at_utc,
        }
    }

    fn stage5d_apply_riskgate_evidence_to_envelope(
        envelope: &mut Stage5dPersistenceEnvelope,
        evidence: &Stage5dRiskGateLedgerEvidence,
    ) {
        let source_records =
            stage5d_source_riskgate_records_from_evidence(evidence).expect("source records");
        let current_shadow_session_date = evidence
            .current_shadow_session_date
            .as_deref()
            .map(|value| NaiveDate::parse_from_str(value, "%Y-%m-%d").expect("date"));
        let current_shadow_pnl_points =
            parse_finite_decimal_string(&evidence.current_shadow_pnl_points).expect("pnl");
        let materialized = crate::hybrid_intraday::rebuild_materialized_state_from_ledger_records(
            &source_records,
            current_shadow_session_date,
            current_shadow_pnl_points,
            evidence.seed_loaded,
        )
        .expect("rebuilt materialized");
        envelope.riskgate.identity = evidence.identity.clone();
        envelope.riskgate.ledger_tail_hash = evidence.ledger_tail_hash.clone();
        envelope.riskgate.materialized_state =
            stage5d_stage_materialized_from_source(&materialized);
        if let Value::Object(fields) =
            &mut envelope.strategy_state.strategy_state_json["HybridIntradayRuntime"]
        {
            fields.insert(
                "risk_gate_mr_enabled_current_session".to_string(),
                materialized
                    .mr_enabled_current_session
                    .map(Value::Bool)
                    .unwrap_or(Value::Null),
            );
            fields.insert(
                "risk_gate_rolling_sum_lb120".to_string(),
                materialized
                    .rolling_sum_lb120
                    .map(|value| serde_json::json!(value))
                    .unwrap_or(Value::Null),
            );
            fields.insert(
                "risk_gate_last_finalized_session_date".to_string(),
                materialized
                    .last_finalized_session_date
                    .map(|date| Value::String(date.format("%Y-%m-%d").to_string()))
                    .unwrap_or(Value::Null),
            );
            fields.insert(
                "risk_gate_ledger_rows_count".to_string(),
                serde_json::json!(materialized.ledger_rows_count),
            );
            fields.insert(
                "risk_gate_shadow_session_date".to_string(),
                materialized
                    .current_shadow_session_date
                    .map(|date| Value::String(date.format("%Y-%m-%d").to_string()))
                    .unwrap_or(Value::Null),
            );
            fields.insert(
                "risk_gate_shadow_pnl_points".to_string(),
                serde_json::json!(materialized.current_shadow_pnl_points),
            );
        }
        align_riskgate_outbox_to_runtime_pending(envelope);
        envelope.payload_checksum_sha256 = envelope
            .compute_payload_checksum_sha256()
            .expect("checksum recomputation must succeed");
    }

    fn stage5d_stage_materialized_from_source(
        materialized: &crate::hybrid_intraday::RiskGateMaterializedState,
    ) -> Stage5dRiskGateMaterializedState {
        Stage5dRiskGateMaterializedState {
            mr_enabled_current_session: materialized.mr_enabled_current_session,
            mr_enabled_next_session: materialized.mr_enabled_next_session,
            rolling_sum_lb120: materialized
                .rolling_sum_lb120
                .map(stage5d_source_format_riskgate_decimal),
            last_finalized_session_date: materialized
                .last_finalized_session_date
                .map(|date| date.format("%Y-%m-%d").to_string()),
            ledger_rows_count: materialized.ledger_rows_count as u64,
            seed_loaded: materialized.seed_loaded,
            current_shadow_session_date: materialized
                .current_shadow_session_date
                .map(|date| date.format("%Y-%m-%d").to_string()),
            current_shadow_pnl_points: stage5d_source_format_riskgate_decimal(
                materialized.current_shadow_pnl_points,
            ),
            current_generation: materialized.current_generation.clone(),
        }
    }

    fn stage5d_test_rebuild_evidence_from_envelope(
        envelope: &Stage5dPersistenceEnvelope,
        evidence: &Stage5dRiskGateLedgerEvidence,
    ) -> Stage5dRiskGateLedgerEvidence {
        let mut evidence = evidence.clone();
        evidence.identity = envelope.riskgate.identity.clone();
        evidence.ledger_tail_hash =
            stage5d_compute_riskgate_ledger_tail_hash(&evidence).expect("tail hash");
        evidence
    }

    fn stage5d_test_rehash_riskgate_evidence(
        mut evidence: Stage5dRiskGateLedgerEvidence,
    ) -> Stage5dRiskGateLedgerEvidence {
        evidence.ledger_tail_hash =
            stage5d_compute_riskgate_ledger_tail_hash(&evidence).expect("tail hash");
        evidence
    }

    fn stage5d_test_configure_single_tail_crash_frontier(
        envelope: &mut Stage5dPersistenceEnvelope,
        evidence: &mut Stage5dRiskGateLedgerEvidence,
        state: Stage5dRiskGateFinalizationState,
        ledger_includes: bool,
        materialized_includes: bool,
        runtime_includes: bool,
        pending_present: bool,
    ) {
        let pending = envelope
            .runtime_private_extension
            .runtime_pending_finalizations[0]
            .clone();
        envelope
            .runtime_private_extension
            .runtime_pending_finalizations = if pending_present {
            vec![pending.clone()]
        } else {
            Vec::new()
        };
        stage5d_test_sync_runtime_pending_cache(envelope);

        if ledger_includes {
            stage5d_test_append_pending_to_evidence(envelope, evidence, &pending);
        }
        let source_records =
            stage5d_source_riskgate_records_from_evidence(evidence).expect("source records");
        let row_index = if ledger_includes {
            source_records.len() - 1
        } else {
            source_records.len()
        };
        let materialized_count = row_index + usize::from(materialized_includes);
        let runtime_count = row_index + usize::from(runtime_includes);
        stage5d_test_set_local_riskgate_projections(
            envelope,
            evidence,
            &source_records,
            materialized_count,
            runtime_count,
        );
        let session_date =
            NaiveDate::parse_from_str(&pending.session_date, "%Y-%m-%d").expect("pending date");
        envelope.riskgate.durable_finalization_outbox =
            vec![Stage5dRiskGateFinalizationOutboxRecord {
                session_date: pending.session_date,
                generation: 1,
                state,
                identity_hash: stage5d_riskgate_outbox_identity_hash(
                    &envelope.riskgate.identity,
                    session_date,
                    1,
                ),
            }];
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct Stage5dTestTailFrontierRow {
        state: Stage5dRiskGateFinalizationState,
        ledger: bool,
        materialized: bool,
        runtime: bool,
        pending: bool,
    }

    fn stage5d_test_configure_ordered_tail_crash_frontier(
        envelope: &mut Stage5dPersistenceEnvelope,
        evidence: &mut Stage5dRiskGateLedgerEvidence,
        rows: &[Stage5dTestTailFrontierRow],
    ) {
        let source_pendings = envelope
            .runtime_private_extension
            .runtime_pending_finalizations
            .clone();
        assert!(rows.len() <= source_pendings.len());
        envelope
            .runtime_private_extension
            .runtime_pending_finalizations = rows
            .iter()
            .zip(&source_pendings)
            .filter_map(|(row, pending)| row.pending.then_some(pending.clone()))
            .collect();
        stage5d_test_sync_runtime_pending_cache(envelope);

        for (row, pending) in rows.iter().zip(&source_pendings) {
            if row.ledger {
                stage5d_test_append_pending_to_evidence(envelope, evidence, pending);
            }
        }
        let source_records =
            stage5d_source_riskgate_records_from_evidence(evidence).expect("source records");
        let ledger_tail_count = rows.iter().filter(|row| row.ledger).count();
        let tail_start = source_records.len() - ledger_tail_count;
        let materialized_tail_count = rows.iter().take_while(|row| row.materialized).count();
        let runtime_tail_count = rows.iter().take_while(|row| row.runtime).count();
        stage5d_test_set_local_riskgate_projections(
            envelope,
            evidence,
            &source_records,
            tail_start + materialized_tail_count,
            tail_start + runtime_tail_count,
        );
        envelope.riskgate.durable_finalization_outbox = rows
            .iter()
            .zip(source_pendings)
            .enumerate()
            .map(|(index, (row, pending))| {
                let session_date = NaiveDate::parse_from_str(&pending.session_date, "%Y-%m-%d")
                    .expect("pending date");
                Stage5dRiskGateFinalizationOutboxRecord {
                    session_date: pending.session_date,
                    generation: (index as u64) + 1,
                    state: row.state,
                    identity_hash: stage5d_riskgate_outbox_identity_hash(
                        &envelope.riskgate.identity,
                        session_date,
                        (index as u64) + 1,
                    ),
                }
            })
            .collect();
    }

    fn stage5d_test_append_pending_to_evidence(
        envelope: &Stage5dPersistenceEnvelope,
        evidence: &mut Stage5dRiskGateLedgerEvidence,
        pending: &Stage5dRuntimePendingRiskGateFinalization,
    ) {
        let existing_records =
            stage5d_source_riskgate_records_from_evidence(evidence).expect("source records");
        let mut rows = crate::hybrid_intraday::rows_from_ledger_records(&existing_records)
            .expect("source rows");
        let session_date =
            NaiveDate::parse_from_str(&pending.session_date, "%Y-%m-%d").expect("pending date");
        rows.push(
            crate::hybrid_intraday::build_runtime_session_row(
                &rows,
                session_date,
                parse_finite_decimal_string(&pending.shadow_pnl_points).expect("pending pnl"),
                pending.shadow_trade_count,
            )
            .expect("source runtime row"),
        );
        let source_identity = crate::hybrid_intraday::RiskGateProfileIdentity {
            strategy_id: evidence.identity.strategy_id.clone(),
            profile_id: evidence.identity.profile_id.clone(),
            mr_variant: evidence.identity.mr_variant.clone(),
            timeframe: evidence.identity.timeframe.clone(),
            session_policy: evidence.identity.session_policy.clone(),
            model_version: evidence.identity.model_version.clone(),
        };
        let record = crate::hybrid_intraday::build_ledger_records_from_rows(
            &rows,
            &source_identity,
            envelope.persisted_at_ts_utc.timestamp(),
        )
        .expect("source ledger")
        .pop()
        .expect("appended record");
        evidence
            .ledger_records
            .push(stage5d_test_stage_record_from_source(&record));
    }

    fn stage5d_test_set_local_riskgate_projections(
        envelope: &mut Stage5dPersistenceEnvelope,
        evidence: &Stage5dRiskGateLedgerEvidence,
        source_records: &[crate::hybrid_intraday::RiskGateLedgerRecord],
        materialized_count: usize,
        runtime_count: usize,
    ) {
        let current_shadow_session_date = evidence
            .current_shadow_session_date
            .as_deref()
            .map(|value| NaiveDate::parse_from_str(value, "%Y-%m-%d").expect("shadow date"));
        let current_shadow_pnl_points =
            parse_finite_decimal_string(&evidence.current_shadow_pnl_points).expect("shadow pnl");
        let materialized = stage5d_rebuild_riskgate_projection_prefix(
            source_records,
            materialized_count,
            current_shadow_session_date,
            current_shadow_pnl_points,
        )
        .expect("materialized projection");
        let runtime = stage5d_rebuild_riskgate_projection_prefix(
            source_records,
            runtime_count,
            current_shadow_session_date,
            current_shadow_pnl_points,
        )
        .expect("runtime projection");
        envelope.riskgate.materialized_state =
            stage5d_stage_materialized_from_source(&materialized);
        stage5d_test_set_semantic_riskgate_projection(envelope, &runtime);
    }

    fn stage5d_test_set_semantic_riskgate_projection(
        envelope: &mut Stage5dPersistenceEnvelope,
        projection: &crate::hybrid_intraday::RiskGateMaterializedState,
    ) {
        if let Value::Object(fields) =
            &mut envelope.strategy_state.strategy_state_json["HybridIntradayRuntime"]
        {
            fields.insert(
                "risk_gate_mr_enabled_current_session".to_string(),
                projection
                    .mr_enabled_current_session
                    .map(Value::Bool)
                    .unwrap_or(Value::Null),
            );
            fields.insert(
                "risk_gate_rolling_sum_lb120".to_string(),
                projection
                    .rolling_sum_lb120
                    .map(|value| serde_json::json!(value))
                    .unwrap_or(Value::Null),
            );
            fields.insert(
                "risk_gate_last_finalized_session_date".to_string(),
                projection
                    .last_finalized_session_date
                    .map(|date| Value::String(date.format("%Y-%m-%d").to_string()))
                    .unwrap_or(Value::Null),
            );
            fields.insert(
                "risk_gate_ledger_rows_count".to_string(),
                serde_json::json!(projection.ledger_rows_count),
            );
        }
    }

    fn stage5d_test_assert_single_tail_frontier_reaches_recovery_complete(
        initial: (Stage5dRiskGateFinalizationState, bool, bool, bool, bool),
    ) {
        let (mut state, mut ledger, mut materialized, mut runtime, mut pending) = initial;
        for _ in 0..4 {
            let (bootstrapped, _envelope, evidence) =
                riskgate_enabled_bootstrapped_fixture_with_evidence(|envelope, evidence| {
                    stage5d_test_configure_single_tail_crash_frontier(
                        envelope,
                        evidence,
                        state,
                        ledger,
                        materialized,
                        runtime,
                        pending,
                    );
                });
            let injected = expect_stage5d_riskgate_ok(
                stage5d_inject_authoritative_riskgate(bootstrapped, evidence),
                "each simulated recovery frontier must remain valid",
            );
            let decision = injected.recovery_plan.decisions[0].action;
            match decision {
                Stage5dRiskGateRecoveryDecision::AppendMissingLedgerRow => {
                    assert!(!ledger && !materialized && !runtime && pending);
                    ledger = true;
                    state = Stage5dRiskGateFinalizationState::LedgerAppended;
                }
                Stage5dRiskGateRecoveryDecision::AdvanceToMaterialized => {
                    assert!(ledger && !runtime && pending);
                    materialized = true;
                    state = Stage5dRiskGateFinalizationState::MaterializedUpdated;
                }
                Stage5dRiskGateRecoveryDecision::ReackRuntime => {
                    assert!(ledger && materialized && !runtime && pending);
                    runtime = true;
                    pending = false;
                    state = Stage5dRiskGateFinalizationState::AcknowledgedInRuntime;
                }
                Stage5dRiskGateRecoveryDecision::AlreadyAcknowledged => {
                    assert!(ledger && materialized && runtime && !pending);
                    assert!(injected.recovery_complete());
                    let restored = stage5d_test_assert_injected_restores_once(
                        injected,
                        "completed single-row recovery must reach restored transition",
                    );
                    assert!(restored.receipt().pending_requests().is_empty());
                    return;
                }
            }
        }
        panic!("single-tail recovery simulation did not reach completion");
    }

    fn stage5d_test_assert_ordered_tail_frontier_reaches_recovery_complete(
        mut rows: Vec<Stage5dTestTailFrontierRow>,
    ) {
        for _ in 0..12 {
            let (bootstrapped, _envelope, evidence) =
                riskgate_enabled_bootstrapped_fixture_with_evidence(|envelope, evidence| {
                    stage5d_test_configure_ordered_tail_crash_frontier(envelope, evidence, &rows);
                });
            let injected = expect_stage5d_riskgate_ok(
                stage5d_inject_authoritative_riskgate(bootstrapped, evidence),
                "each simulated ordered recovery frontier must remain valid",
            );
            if injected.recovery_complete() {
                assert!(rows.iter().all(|row| {
                    row.state == Stage5dRiskGateFinalizationState::AcknowledgedInRuntime
                        && row.ledger
                        && row.materialized
                        && row.runtime
                        && !row.pending
                }));
                let restored = stage5d_test_assert_injected_restores_once(
                    injected,
                    "completed multi-row recovery must reach restored transition",
                );
                assert!(restored.receipt().pending_requests().is_empty());
                return;
            }
            let (index, decision) = injected
                .recovery_plan
                .decisions
                .iter()
                .enumerate()
                .find(|(_, decision)| {
                    decision.action != Stage5dRiskGateRecoveryDecision::AlreadyAcknowledged
                })
                .expect("noncomplete plan must have a next action");
            let before = rows.clone();
            stage5d_test_apply_ordered_tail_action(&mut rows, index, decision.action);
            let mut replay = rows.clone();
            stage5d_test_apply_ordered_tail_action(&mut replay, index, decision.action);
            assert_eq!(replay, rows, "replaying the same action must be idempotent");
            assert!(
                rows.iter()
                    .zip(&before)
                    .filter(|(after, before)| after.pending != before.pending)
                    .all(|(after, before)| {
                        before.pending
                            && !after.pending
                            && after.state
                                == Stage5dRiskGateFinalizationState::AcknowledgedInRuntime
                    }),
                "pending can only clear at runtime acknowledgement"
            );
        }
        panic!("ordered recovery simulation did not reach completion");
    }

    fn stage5d_test_apply_ordered_tail_action(
        rows: &mut [Stage5dTestTailFrontierRow],
        index: usize,
        action: Stage5dRiskGateRecoveryDecision,
    ) {
        let row = &mut rows[index];
        match action {
            Stage5dRiskGateRecoveryDecision::AppendMissingLedgerRow => {
                if row.ledger {
                    return;
                }
                assert!(!row.materialized && !row.runtime && row.pending);
                row.ledger = true;
                row.state = Stage5dRiskGateFinalizationState::LedgerAppended;
            }
            Stage5dRiskGateRecoveryDecision::AdvanceToMaterialized => {
                if row.materialized {
                    return;
                }
                assert!(row.ledger && !row.runtime && row.pending);
                row.materialized = true;
                row.state = Stage5dRiskGateFinalizationState::MaterializedUpdated;
            }
            Stage5dRiskGateRecoveryDecision::ReackRuntime => {
                if row.runtime {
                    return;
                }
                assert!(row.ledger && row.materialized && row.pending);
                row.runtime = true;
                row.pending = false;
                row.state = Stage5dRiskGateFinalizationState::AcknowledgedInRuntime;
            }
            Stage5dRiskGateRecoveryDecision::AlreadyAcknowledged => {}
        }
    }

    fn stage5d_test_sync_runtime_pending_cache(envelope: &mut Stage5dPersistenceEnvelope) {
        let first = envelope
            .runtime_private_extension
            .runtime_pending_finalizations
            .first();
        if let Value::Object(fields) =
            &mut envelope.strategy_state.strategy_state_json["HybridIntradayRuntime"]
        {
            fields.insert(
                "risk_gate_pending_session_date".to_string(),
                first
                    .map(|pending| Value::String(pending.session_date.clone()))
                    .unwrap_or(Value::Null),
            );
            fields.insert(
                "risk_gate_pending_shadow_pnl_points".to_string(),
                first
                    .map(|pending| {
                        serde_json::json!(parse_finite_decimal_string(&pending.shadow_pnl_points)
                            .expect("pending pnl"))
                    })
                    .unwrap_or_else(|| serde_json::json!(0.0)),
            );
            fields.insert(
                "risk_gate_pending_shadow_trade_count".to_string(),
                first
                    .map(|pending| serde_json::json!(pending.shadow_trade_count))
                    .unwrap_or_else(|| serde_json::json!(0)),
            );
        }
    }

    fn stage5d_active_order(
        account_id: BrokerAccountId,
        instrument: InstrumentId,
        order_id: BrokerOrderId,
        received_ts: DateTime<Utc>,
    ) -> broker_core::BrokerOrderSnapshot {
        broker_core::BrokerOrderSnapshot {
            account_id,
            broker_order_id: Some(order_id),
            client_order_id: None,
            instrument,
            side: broker_core::OrderSide::Buy,
            order_type: broker_core::OrderType::Limit,
            time_in_force: Some(broker_core::TimeInForce::Day),
            status: broker_core::OrderStatus::Working,
            lifecycle: broker_core::BrokerOrderLifecycle::Active,
            qty: Decimal::ONE,
            filled_qty: Decimal::ZERO,
            remaining_qty: Some(Decimal::ONE),
            limit_price: Some(Decimal::new(2210, 0)),
            broker_asset_id: None,
            board: None,
            expiration_date: None,
            source_ts: Some(received_ts),
            received_ts,
        }
    }

    fn stage5d_fresh_admission_for_envelope(
        envelope: &Stage5dPersistenceEnvelope,
        target_position_qty: Decimal,
        checked_ts: DateTime<Utc>,
    ) -> crate::stage5c_paper_host::Stage5cPaperHostAdmission {
        crate::stage5c_paper_host::Stage5cPaperHostAdmission::stage5d_test_new(
            envelope.binding.strategy_id.clone(),
            envelope.binding.account_id.clone(),
            envelope.binding.instrument_id.to_instrument_id(),
            0.5,
            target_position_qty,
            checked_ts,
        )
    }

    #[test]
    fn stage5d_b2a_valid_fixture_roundtrips_and_validates_checksum() {
        let envelope = valid_fixture();

        assert_eq!(envelope.binding.stage, Stage5dPersistenceStage::Stage5d);
        assert_eq!(
            envelope.binding.strategy_kind,
            Stage5dStrategyKind::HybridIntraday
        );
        assert!(envelope
            .strategy_state
            .strategy_state_json
            .get("HybridIntradayRuntime")
            .is_some());
        let semantic: Stage5dSemanticStrategyStateV1 =
            serde_json::from_value(envelope.strategy_state.strategy_state_json.clone())
                .expect("semantic state must be strict Stage 5D state");
        let Stage5dSemanticStrategyStateV1::HybridIntradayRuntime(state) = semantic;
        assert_eq!(state.active_cycle_id.as_deref(), Some("6a4badd811"));
        assert_eq!(state.last_position_qty, 0.5);
        assert_eq!(
            state.pending_entry_request_id,
            Some(envelope.recovery_indexes.pending_requests[0])
        );
        assert_eq!(state.pending_entry_created_ts_utc, Some(1_784_009_340));
        assert_eq!(
            state.today_start_local.as_deref(),
            Some("2026-07-14T09:00:00")
        );
        assert_eq!(
            envelope.timestamp_policy.semantic_event_ts_utc,
            Stage5dTimestampUnits::Seconds
        );
        assert_eq!(
            envelope.timestamp_policy.runtime_wall_clock_timer,
            Stage5dTimestampUnits::Milliseconds
        );
        assert_eq!(
            state.risk_gate_pending_session_date.as_deref(),
            Some("2026-07-10")
        );
        assert_eq!(state.risk_gate_pending_shadow_pnl_points, 12.5);
        assert_eq!(state.risk_gate_pending_shadow_trade_count, 2);
        assert_eq!(
            envelope
                .runtime_private_extension
                .runtime_pending_finalizations,
            vec![
                Stage5dRuntimePendingRiskGateFinalization {
                    session_date: "2026-07-10".to_string(),
                    shadow_pnl_points: "12.5".to_string(),
                    shadow_trade_count: 2,
                },
                Stage5dRuntimePendingRiskGateFinalization {
                    session_date: "2026-07-13".to_string(),
                    shadow_pnl_points: "-3.25".to_string(),
                    shadow_trade_count: 1,
                },
            ]
        );
        assert_eq!(
            envelope
                .runtime_private_extension
                .pending_entry
                .as_ref()
                .expect("fixture pending entry")
                .entry_style,
            Stage5dEntryStyle::Market
        );
        assert!(envelope.runtime_private_extension.pending_exit.is_none());

        envelope
            .validate_schema_and_checksum()
            .expect("fixture checksum must match canonical payload");
        let validated = envelope
            .validate_restore_contract_schema_only()
            .expect("fixture must pass schema-only restore contract");
        assert_eq!(validated.snapshot_id(), "SNAP_STAGE5D_B2A_0001");

        let serialized = serde_json::to_value(&envelope).expect("fixture must serialize");
        let expected: Value = serde_json::from_str(include_str!(
            "../../../tests/fixtures/stage5/stage5d_b2a_persistence_envelope.json"
        ))
        .expect("fixture JSON must parse");
        assert_eq!(serialized, expected);
    }

    #[test]
    fn stage5d_b2a_corrupt_checksum_fixture_is_rejected() {
        let envelope = corrupt_checksum_fixture();

        assert_eq!(
            envelope.validate_schema_and_checksum(),
            Err(Stage5dEnvelopeValidationError::PayloadChecksumMismatch)
        );
    }

    #[test]
    fn stage5d_b2a_schema_mismatch_is_rejected() {
        let envelope = bad_version_fixture();

        assert_eq!(
            envelope.validate_schema_and_checksum(),
            Err(Stage5dEnvelopeValidationError::RuntimePrivateSchemaMismatch)
        );
    }

    #[test]
    fn stage5d_b2a_required_identity_fields_are_rejected() {
        let envelope = empty_config_fixture();

        assert_eq!(
            envelope.validate_schema_and_checksum(),
            Err(Stage5dEnvelopeValidationError::RequiredFieldEmpty)
        );
    }

    fn strict_fixture_with_inserted_field(anchor: &str, inserted_field: &str) -> String {
        let payload =
            include_str!("../../../tests/fixtures/stage5/stage5d_b2a_persistence_envelope.json");
        payload.replacen(anchor, &format!("{anchor}\n{inserted_field}"), 1)
    }

    #[test]
    fn stage5d_b2a_unknown_root_field_is_rejected_before_checksum() {
        let payload = strict_fixture_with_inserted_field(
            "  \"schema_version\": 1,",
            "  \"unsupported_runtime_state\": {\"pending_real_order\": true},",
        );

        assert_eq!(
            Stage5dPersistenceEnvelope::from_json_str_strict(&payload),
            Err(Stage5dEnvelopeValidationError::DeserializationFailed)
        );
    }

    #[test]
    fn stage5d_b2a_unknown_runtime_private_field_is_rejected_before_checksum() {
        let payload = strict_fixture_with_inserted_field(
            "  \"runtime_private_extension\": {\n    \"schema_version\": 1,",
            "    \"unsupported_runtime_private\": true,",
        );

        assert_eq!(
            Stage5dPersistenceEnvelope::from_json_str_strict(&payload),
            Err(Stage5dEnvelopeValidationError::DeserializationFailed)
        );
    }

    #[test]
    fn stage5d_b2a_unknown_riskgate_field_is_rejected_before_checksum() {
        let payload = strict_fixture_with_inserted_field(
            "  \"riskgate\": {\n    \"schema_version\": 1,",
            "    \"unsupported_riskgate_state\": true,",
        );

        assert_eq!(
            Stage5dPersistenceEnvelope::from_json_str_strict(&payload),
            Err(Stage5dEnvelopeValidationError::DeserializationFailed)
        );
    }

    #[test]
    fn stage5d_b2a_unknown_nested_outbox_field_is_rejected_before_checksum() {
        let payload = strict_fixture_with_inserted_field(
            "        \"session_date\": \"2026-07-14\",",
            "        \"unsupported_outbox_field\": true,",
        );

        assert_eq!(
            Stage5dPersistenceEnvelope::from_json_str_strict(&payload),
            Err(Stage5dEnvelopeValidationError::DeserializationFailed)
        );
    }

    #[test]
    fn stage5d_b2a_unknown_nested_instrument_field_is_rejected_before_checksum() {
        let payload = strict_fixture_with_inserted_field(
            "      \"symbol\": \"IMOEXF\",",
            "      \"unsupported_instrument_binding\": true,",
        );

        assert_eq!(
            Stage5dPersistenceEnvelope::from_json_str_strict(&payload),
            Err(Stage5dEnvelopeValidationError::DeserializationFailed)
        );
    }

    fn fixture_with_mutated_state(mutator: impl FnOnce(&mut Value)) -> Stage5dPersistenceEnvelope {
        let mut envelope = valid_fixture();
        mutator(&mut envelope.strategy_state.strategy_state_json);
        envelope.payload_checksum_sha256 = envelope
            .compute_payload_checksum_sha256()
            .expect("checksum recomputation must succeed");
        envelope
    }

    fn fixture_with_mutated_envelope(
        mutator: impl FnOnce(&mut Stage5dPersistenceEnvelope),
    ) -> Stage5dPersistenceEnvelope {
        let mut envelope = valid_fixture();
        mutator(&mut envelope);
        envelope.payload_checksum_sha256 = envelope
            .compute_payload_checksum_sha256()
            .expect("checksum recomputation must succeed");
        envelope
    }

    #[test]
    fn stage5d_b2a_scalar_strategy_state_payload_is_rejected() {
        let mut envelope = valid_fixture();
        envelope.strategy_state.strategy_state_json = Value::Bool(true);
        envelope.payload_checksum_sha256 = envelope
            .compute_payload_checksum_sha256()
            .expect("checksum recomputation must succeed");

        assert_eq!(
            envelope.validate_restore_contract_schema_only().map(|_| ()),
            Err(Stage5dEnvelopeValidationError::SemanticStateInvalid)
        );
    }

    #[test]
    fn stage5d_b2a_unknown_semantic_state_field_is_rejected() {
        let envelope = fixture_with_mutated_state(|state| {
            state["HybridIntradayRuntime"]["pending_real_order"] = Value::Bool(true);
        });

        assert_eq!(
            envelope.validate_restore_contract_schema_only().map(|_| ()),
            Err(Stage5dEnvelopeValidationError::SemanticStateInvalid)
        );
    }

    #[test]
    fn stage5d_b2a_wrong_semantic_state_variant_is_rejected() {
        let mut envelope = valid_fixture();
        envelope.strategy_state.strategy_state_json = serde_json::json!({"Idle": {}});
        envelope.payload_checksum_sha256 = envelope
            .compute_payload_checksum_sha256()
            .expect("checksum recomputation must succeed");

        assert_eq!(
            envelope.validate_restore_contract_schema_only().map(|_| ()),
            Err(Stage5dEnvelopeValidationError::SemanticStateInvalid)
        );
    }

    #[test]
    fn stage5d_b2a_misspelled_semantic_state_field_is_rejected() {
        let envelope = fixture_with_mutated_state(|state| {
            if let Value::Object(fields) = &mut state["HybridIntradayRuntime"] {
                let active_cycle = fields
                    .remove("active_cycle_id")
                    .expect("fixture has active cycle");
                fields.insert("misspelled_active_cycle".to_string(), active_cycle);
            }
        });

        assert_eq!(
            envelope.validate_restore_contract_schema_only().map(|_| ()),
            Err(Stage5dEnvelopeValidationError::SemanticStateInvalid)
        );
    }

    #[test]
    fn stage5d_b2a_invalid_semantic_field_type_is_rejected() {
        let envelope = fixture_with_mutated_state(|state| {
            state["HybridIntradayRuntime"]["last_position_qty"] =
                Value::String("not-a-number".to_string());
        });

        assert_eq!(
            envelope.validate_restore_contract_schema_only().map(|_| ()),
            Err(Stage5dEnvelopeValidationError::SemanticStateInvalid)
        );
    }

    #[test]
    fn stage5d_b2a_inconsistent_pending_entry_is_rejected() {
        let envelope = fixture_with_mutated_state(|state| {
            state["HybridIntradayRuntime"]["pending_entry_cycle_id"] = Value::Null;
        });

        assert_eq!(
            envelope.validate_restore_contract_schema_only().map(|_| ()),
            Err(Stage5dEnvelopeValidationError::PendingStateInconsistent)
        );
    }

    #[test]
    fn stage5d_b2a_semantic_timestamp_in_milliseconds_is_rejected() {
        let envelope = fixture_with_mutated_state(|state| {
            state["HybridIntradayRuntime"]["pending_entry_created_ts_utc"] =
                Value::Number(1_784_009_340_000_i64.into());
        });

        assert_eq!(
            envelope.validate_restore_contract_schema_only().map(|_| ()),
            Err(Stage5dEnvelopeValidationError::TimestampChronologyInvalid)
        );
    }

    #[test]
    fn stage5d_b2a_runtime_timer_in_seconds_is_rejected() {
        let envelope = fixture_with_mutated_envelope(|envelope| {
            envelope
                .runtime_private_extension
                .partial_entry_timer
                .as_mut()
                .expect("fixture partial timer")
                .partial_started_at_ms = 1_784_009_370;
        });

        assert_eq!(
            envelope.validate_restore_contract_schema_only().map(|_| ()),
            Err(Stage5dEnvelopeValidationError::TimestampChronologyInvalid)
        );
    }

    #[test]
    fn stage5d_b2a_timer_after_persisted_at_is_rejected() {
        let envelope = fixture_with_mutated_envelope(|envelope| {
            envelope
                .runtime_private_extension
                .partial_entry_timer
                .as_mut()
                .expect("fixture partial timer")
                .partial_started_at_ms = envelope.persisted_at_ts_utc.timestamp_millis() + 1;
        });

        assert_eq!(
            envelope.validate_restore_contract_schema_only().map(|_| ()),
            Err(Stage5dEnvelopeValidationError::TimestampChronologyInvalid)
        );
    }

    #[test]
    fn stage5d_b2a_pending_entry_awaiting_ack_with_older_broker_watermark_is_accepted() {
        let envelope = fixture_with_mutated_envelope(|envelope| {
            envelope.lifecycle_watermarks.last_broker_event_ts =
                Some("2026-07-14T05:59:30Z".parse().expect("valid ts"));
        });

        envelope
            .validate_restore_contract_schema_only()
            .expect("pending entry may be newer than unrelated broker watermark");
    }

    #[test]
    fn stage5d_b2a_last_broker_event_after_persisted_at_is_rejected() {
        let envelope = fixture_with_mutated_envelope(|envelope| {
            envelope.lifecycle_watermarks.last_broker_event_ts =
                Some("2026-07-14T06:10:01Z".parse().expect("valid ts"));
        });

        assert_eq!(
            envelope.validate_restore_contract_schema_only().map(|_| ()),
            Err(Stage5dEnvelopeValidationError::TimestampChronologyInvalid)
        );
    }

    #[test]
    fn stage5d_b2a_last_semantic_bar_after_persisted_at_is_rejected() {
        let envelope = fixture_with_mutated_envelope(|envelope| {
            envelope.lifecycle_watermarks.last_semantic_bar_ts =
                Some("2026-07-14T06:10:01Z".parse().expect("valid ts"));
        });

        assert_eq!(
            envelope.validate_restore_contract_schema_only().map(|_| ()),
            Err(Stage5dEnvelopeValidationError::TimestampChronologyInvalid)
        );
    }

    #[test]
    fn stage5d_b2a_last_processed_bar_after_persisted_at_is_rejected() {
        let envelope = fixture_with_mutated_envelope(|envelope| {
            envelope.runtime_private_extension.last_processed_bar_ts =
                Some("2026-07-14T06:10:01Z".parse().expect("valid ts"));
        });

        assert_eq!(
            envelope.validate_restore_contract_schema_only().map(|_| ()),
            Err(Stage5dEnvelopeValidationError::TimestampChronologyInvalid)
        );
    }

    #[test]
    fn stage5d_b2a_last_processed_bar_ahead_of_semantic_watermark_is_rejected() {
        let envelope = fixture_with_mutated_envelope(|envelope| {
            envelope.runtime_private_extension.last_processed_bar_ts =
                Some("2026-07-14T06:00:01Z".parse().expect("valid ts"));
        });

        assert_eq!(
            envelope.validate_restore_contract_schema_only().map(|_| ()),
            Err(Stage5dEnvelopeValidationError::TimestampChronologyInvalid)
        );
    }

    #[test]
    fn stage5d_b2a_timestamp_policy_mismatch_is_rejected() {
        let envelope = fixture_with_mutated_envelope(|envelope| {
            envelope.timestamp_policy.semantic_event_ts_utc = Stage5dTimestampUnits::Milliseconds;
        });

        assert_eq!(
            envelope.validate_restore_contract_schema_only().map(|_| ()),
            Err(Stage5dEnvelopeValidationError::TimestampPolicyInvalid)
        );
    }

    #[test]
    fn stage5d_b2a_semantic_pending_entry_without_private_extension_is_rejected() {
        let envelope = fixture_with_mutated_envelope(|envelope| {
            envelope.runtime_private_extension.pending_entry = None;
            envelope.runtime_private_extension.partial_entry_timer = None;
        });

        assert_eq!(
            envelope.validate_restore_contract_schema_only().map(|_| ()),
            Err(Stage5dEnvelopeValidationError::PendingStateInconsistent)
        );
    }

    #[test]
    fn stage5d_b2a_private_pending_entry_without_semantic_lifecycle_is_rejected() {
        let envelope = fixture_with_mutated_state(|state| {
            let state = &mut state["HybridIntradayRuntime"];
            state["pending_entry_owner"] = Value::Null;
            state["pending_entry_side"] = Value::Null;
            state["pending_entry_cycle_id"] = Value::Null;
            state["pending_entry_request_id"] = Value::Null;
            state["pending_entry_created_ts_utc"] = Value::Null;
        });

        assert_eq!(
            envelope.validate_restore_contract_schema_only().map(|_| ()),
            Err(Stage5dEnvelopeValidationError::RecoveryIndexInconsistent)
        );
    }

    #[test]
    fn stage5d_b2a_partial_position_without_timer_is_rejected() {
        let envelope = fixture_with_mutated_envelope(|envelope| {
            envelope.runtime_private_extension.partial_entry_timer = None;
        });

        assert_eq!(
            envelope.validate_restore_contract_schema_only().map(|_| ()),
            Err(Stage5dEnvelopeValidationError::PendingStateInconsistent)
        );
    }

    #[test]
    fn stage5d_b2a_timer_without_pending_entry_is_rejected() {
        let envelope = fixture_with_mutated_envelope(|envelope| {
            envelope.runtime_private_extension.pending_entry = None;
        });

        assert_eq!(
            envelope.validate_restore_contract_schema_only().map(|_| ()),
            Err(Stage5dEnvelopeValidationError::PendingStateInconsistent)
        );
    }

    #[test]
    fn stage5d_b2a_full_target_with_pending_entry_is_rejected() {
        let envelope = fixture_with_mutated_state(|state| {
            state["HybridIntradayRuntime"]["last_position_qty"] = serde_json::json!(1.0);
        });

        assert_eq!(
            envelope.validate_restore_contract_schema_only().map(|_| ()),
            Err(Stage5dEnvelopeValidationError::PendingStateInconsistent)
        );
    }

    #[test]
    fn stage5d_b2a_pending_entry_cycle_mismatch_is_rejected() {
        let envelope = fixture_with_mutated_state(|state| {
            state["HybridIntradayRuntime"]["pending_entry_cycle_id"] =
                Value::String("aaaaaaaaaa".to_string());
        });

        assert_eq!(
            envelope.validate_restore_contract_schema_only().map(|_| ()),
            Err(Stage5dEnvelopeValidationError::PendingStateInconsistent)
        );
    }

    #[test]
    fn stage5d_b2a_invalid_source_local_date_is_rejected() {
        let envelope = fixture_with_mutated_state(|state| {
            state["HybridIntradayRuntime"]["last_day_local"] =
                Value::String("bad-date".to_string());
        });

        assert_eq!(
            envelope.validate_restore_contract_schema_only().map(|_| ()),
            Err(Stage5dEnvelopeValidationError::SourceRoundtripInconsistent)
        );
    }

    #[test]
    fn stage5d_b2a_invalid_source_local_datetime_is_rejected() {
        let envelope = fixture_with_mutated_state(|state| {
            state["HybridIntradayRuntime"]["today_start_local"] =
                Value::String("2026-07-14".to_string());
        });

        assert_eq!(
            envelope.validate_restore_contract_schema_only().map(|_| ()),
            Err(Stage5dEnvelopeValidationError::SourceRoundtripInconsistent)
        );
    }

    #[test]
    fn stage5d_b2a_partial_deferred_entry_tuple_is_rejected() {
        let envelope = fixture_with_mutated_state(|state| {
            state["HybridIntradayRuntime"]["deferred_entry_owner"] =
                Value::String("mean_reversion".to_string());
        });

        assert_eq!(
            envelope.validate_restore_contract_schema_only().map(|_| ()),
            Err(Stage5dEnvelopeValidationError::SourceRoundtripInconsistent)
        );
    }

    #[test]
    fn stage5d_b2a_partial_deferred_exit_tuple_is_rejected() {
        let envelope = fixture_with_mutated_state(|state| {
            state["HybridIntradayRuntime"]["deferred_exit_owner"] =
                Value::String("mean_reversion".to_string());
        });

        assert_eq!(
            envelope.validate_restore_contract_schema_only().map(|_| ()),
            Err(Stage5dEnvelopeValidationError::SourceRoundtripInconsistent)
        );
    }

    #[test]
    fn stage5d_b2a_partial_shadow_position_tuple_is_rejected() {
        let envelope = fixture_with_mutated_state(|state| {
            state["HybridIntradayRuntime"]["risk_gate_shadow_entry_price"] =
                serde_json::json!(2227.0);
        });

        assert_eq!(
            envelope.validate_restore_contract_schema_only().map(|_| ()),
            Err(Stage5dEnvelopeValidationError::SourceRoundtripInconsistent)
        );
    }

    #[test]
    fn stage5d_b2a_pending_tp_missing_from_pending_requests_is_rejected() {
        let envelope = fixture_with_mutated_state(|state| {
            state["HybridIntradayRuntime"]["pending_tp_request_id"] =
                Value::String("22222222-2222-2222-2222-222222222222".to_string());
            state["HybridIntradayRuntime"]["pending_tp_created_ts_utc"] =
                Value::Number(1_784_009_350_i64.into());
        });

        assert_eq!(
            envelope.validate_restore_contract_schema_only().map(|_| ()),
            Err(Stage5dEnvelopeValidationError::RecoveryIndexInconsistent)
        );
    }

    #[test]
    fn stage5d_b2a_broker_id_missing_from_known_index_is_rejected() {
        let envelope = fixture_with_mutated_state(|state| {
            state["HybridIntradayRuntime"]["tp_order_id"] =
                Value::String("MISSING-ORDER-ID".to_string());
        });

        assert_eq!(
            envelope.validate_restore_contract_schema_only().map(|_| ()),
            Err(Stage5dEnvelopeValidationError::RecoveryIndexInconsistent)
        );
    }

    #[test]
    fn stage5d_b2a_duplicate_recovery_indexes_are_rejected() {
        let envelope = fixture_with_mutated_envelope(|envelope| {
            let duplicate = envelope.recovery_indexes.known_order_ids[0].clone();
            envelope.recovery_indexes.known_order_ids.push(duplicate);
        });

        assert_eq!(
            envelope.validate_restore_contract_schema_only().map(|_| ()),
            Err(Stage5dEnvelopeValidationError::RecoveryIndexInconsistent)
        );
    }

    #[test]
    fn stage5d_b2a_single_runtime_pending_finalization_roundtrips() {
        let envelope = fixture_with_mutated_envelope(|envelope| {
            envelope
                .runtime_private_extension
                .runtime_pending_finalizations
                .truncate(1);
        });

        let semantic: Stage5dSemanticStrategyStateV1 =
            serde_json::from_value(envelope.strategy_state.strategy_state_json.clone())
                .expect("semantic state must decode");
        let Stage5dSemanticStrategyStateV1::HybridIntradayRuntime(state) = semantic;
        assert_eq!(
            envelope
                .runtime_private_extension
                .runtime_pending_finalizations[0],
            Stage5dRuntimePendingRiskGateFinalization {
                session_date: "2026-07-10".to_string(),
                shadow_pnl_points: "12.5".to_string(),
                shadow_trade_count: 2,
            }
        );
        assert_eq!(
            state.risk_gate_pending_session_date.as_deref(),
            Some("2026-07-10")
        );

        envelope
            .validate_restore_contract_schema_only()
            .expect("single runtime pending finalization must validate");
    }

    #[test]
    fn stage5d_b2a_semantic_first_runtime_finalization_cache_mismatch_is_rejected() {
        let envelope = fixture_with_mutated_state(|state| {
            state["HybridIntradayRuntime"]["risk_gate_pending_shadow_pnl_points"] =
                serde_json::json!(12.75);
        });

        assert_eq!(
            envelope.validate_restore_contract_schema_only().map(|_| ()),
            Err(Stage5dEnvelopeValidationError::RiskGateFinalizationInconsistent)
        );
    }

    #[test]
    fn stage5d_b2a_empty_runtime_finalizations_with_nonzero_semantic_cache_is_rejected() {
        let envelope = fixture_with_mutated_envelope(|envelope| {
            envelope
                .runtime_private_extension
                .runtime_pending_finalizations
                .clear();
        });

        assert_eq!(
            envelope.validate_restore_contract_schema_only().map(|_| ()),
            Err(Stage5dEnvelopeValidationError::RiskGateFinalizationInconsistent)
        );
    }

    #[test]
    fn stage5d_b2a_nonempty_runtime_finalization_with_null_semantic_date_is_rejected() {
        let envelope = fixture_with_mutated_state(|state| {
            state["HybridIntradayRuntime"]["risk_gate_pending_session_date"] = Value::Null;
        });

        assert_eq!(
            envelope.validate_restore_contract_schema_only().map(|_| ()),
            Err(Stage5dEnvelopeValidationError::RiskGateFinalizationInconsistent)
        );
    }

    #[test]
    fn stage5d_b2a_invalid_runtime_finalization_pnl_is_rejected() {
        let envelope = fixture_with_mutated_envelope(|envelope| {
            envelope
                .runtime_private_extension
                .runtime_pending_finalizations[0]
                .shadow_pnl_points = "NaN".to_string();
        });

        assert_eq!(
            envelope.validate_restore_contract_schema_only().map(|_| ()),
            Err(Stage5dEnvelopeValidationError::SourceRoundtripInconsistent)
        );
    }

    #[test]
    fn stage5d_b2a_duplicate_runtime_finalization_date_is_rejected() {
        let envelope = fixture_with_mutated_envelope(|envelope| {
            envelope
                .runtime_private_extension
                .runtime_pending_finalizations[1]
                .session_date = "2026-07-10".to_string();
        });

        assert_eq!(
            envelope.validate_restore_contract_schema_only().map(|_| ()),
            Err(Stage5dEnvelopeValidationError::RiskGateFinalizationInconsistent)
        );
    }

    #[test]
    fn stage5d_b2a_out_of_order_runtime_finalization_dates_are_rejected() {
        let envelope = fixture_with_mutated_envelope(|envelope| {
            envelope
                .runtime_private_extension
                .runtime_pending_finalizations
                .swap(0, 1);
        });

        assert_eq!(
            envelope.validate_restore_contract_schema_only().map(|_| ()),
            Err(Stage5dEnvelopeValidationError::RiskGateFinalizationInconsistent)
        );
    }

    #[test]
    fn stage5d_b2a_runtime_finalization_equal_to_current_shadow_session_is_rejected() {
        let envelope = fixture_with_mutated_envelope(|envelope| {
            envelope
                .runtime_private_extension
                .runtime_pending_finalizations[1]
                .session_date = "2026-07-14".to_string();
        });

        assert_eq!(
            envelope.validate_restore_contract_schema_only().map(|_| ()),
            Err(Stage5dEnvelopeValidationError::RiskGateFinalizationInconsistent)
        );
    }

    #[test]
    fn stage5d_b2a_runtime_finalization_after_current_shadow_session_is_rejected() {
        let envelope = fixture_with_mutated_envelope(|envelope| {
            envelope
                .runtime_private_extension
                .runtime_pending_finalizations[1]
                .session_date = "2026-07-15".to_string();
        });

        assert_eq!(
            envelope.validate_restore_contract_schema_only().map(|_| ()),
            Err(Stage5dEnvelopeValidationError::RiskGateFinalizationInconsistent)
        );
    }

    #[test]
    fn stage5d_b2a_ordered_runtime_finalizations_without_current_shadow_session_are_accepted() {
        let envelope = fixture_with_mutated_state(|state| {
            state["HybridIntradayRuntime"]["risk_gate_shadow_session_date"] = Value::Null;
        });

        envelope
            .validate_restore_contract_schema_only()
            .expect("ordered finalization vector is valid before current shadow session is known");
    }

    #[test]
    fn stage5d_b2a_saturday_runtime_finalization_is_rejected() {
        let envelope = fixture_with_mutated_envelope(|envelope| {
            envelope
                .runtime_private_extension
                .runtime_pending_finalizations[0]
                .session_date = "2026-07-11".to_string();
            if let Value::Object(fields) =
                &mut envelope.strategy_state.strategy_state_json["HybridIntradayRuntime"]
            {
                fields.insert(
                    "risk_gate_pending_session_date".to_string(),
                    Value::String("2026-07-11".to_string()),
                );
            }
        });

        assert_eq!(
            envelope.validate_restore_contract_schema_only().map(|_| ()),
            Err(Stage5dEnvelopeValidationError::RiskGateFinalizationInconsistent)
        );
    }

    #[test]
    fn stage5d_b2a_sunday_runtime_finalization_is_rejected() {
        let envelope = fixture_with_mutated_envelope(|envelope| {
            envelope
                .runtime_private_extension
                .runtime_pending_finalizations[0]
                .session_date = "2026-07-12".to_string();
            if let Value::Object(fields) =
                &mut envelope.strategy_state.strategy_state_json["HybridIntradayRuntime"]
            {
                fields.insert(
                    "risk_gate_pending_session_date".to_string(),
                    Value::String("2026-07-12".to_string()),
                );
            }
        });

        assert_eq!(
            envelope.validate_restore_contract_schema_only().map(|_| ()),
            Err(Stage5dEnvelopeValidationError::RiskGateFinalizationInconsistent)
        );
    }

    #[test]
    fn stage5d_b2a_friday_monday_vector_before_tuesday_shadow_session_is_accepted() {
        let envelope = valid_fixture();

        envelope
            .validate_restore_contract_schema_only()
            .expect("Friday and Monday finalizations before Tuesday shadow session must validate");
    }

    #[test]
    fn stage5d_b2a_weekend_runtime_finalization_without_current_shadow_session_is_rejected() {
        let envelope = fixture_with_mutated_envelope(|envelope| {
            envelope
                .runtime_private_extension
                .runtime_pending_finalizations[0]
                .session_date = "2026-07-12".to_string();
            if let Value::Object(fields) =
                &mut envelope.strategy_state.strategy_state_json["HybridIntradayRuntime"]
            {
                fields.insert("risk_gate_shadow_session_date".to_string(), Value::Null);
                fields.insert(
                    "risk_gate_pending_session_date".to_string(),
                    Value::String("2026-07-12".to_string()),
                );
            }
        });

        assert_eq!(
            envelope.validate_restore_contract_schema_only().map(|_| ()),
            Err(Stage5dEnvelopeValidationError::RiskGateFinalizationInconsistent)
        );
    }

    #[test]
    fn stage5d_b2b_runtime_private_apply_restores_full_pending_finalization_vector() {
        let envelope = apply_valid_fixture();
        let mut strategy = stage5d_test_strategy_with_config(|config| {
            config.qty = 3.0;
        });
        restore_semantic_state(&mut strategy, &envelope);

        let before_apply = strategy
            .stage5d_export_runtime_private_extension()
            .expect("stage5d export");
        assert_eq!(before_apply.runtime_pending_finalizations.len(), 1);

        strategy
            .stage5d_apply_runtime_private_extension(&envelope.runtime_private_extension)
            .expect("validated private extension must apply");

        let exported = strategy
            .stage5d_export_runtime_private_extension()
            .expect("stage5d export");
        assert_eq!(
            exported.runtime_pending_finalizations,
            envelope
                .runtime_private_extension
                .runtime_pending_finalizations
        );
        let exported_entry = exported.pending_entry.as_ref().expect("exported entry");
        let envelope_entry = envelope
            .runtime_private_extension
            .pending_entry
            .as_ref()
            .expect("fixture entry");
        assert_eq!(exported_entry.owner, envelope_entry.owner);
        assert_eq!(exported_entry.side, envelope_entry.side);
        assert_eq!(exported_entry.reason, envelope_entry.reason);
        assert_eq!(exported_entry.entry_style, envelope_entry.entry_style);
        assert_eq!(exported_entry.request_id, envelope_entry.request_id);
        assert_eq!(
            parse_finite_decimal_string(&exported_entry.target_qty),
            parse_finite_decimal_string(&envelope_entry.target_qty)
        );
        assert_eq!(
            exported_entry
                .stop_price
                .as_deref()
                .map(parse_finite_decimal_string),
            envelope_entry
                .stop_price
                .as_deref()
                .map(parse_finite_decimal_string)
        );
        assert_eq!(
            exported_entry
                .take_price
                .as_deref()
                .map(parse_finite_decimal_string),
            envelope_entry
                .take_price
                .as_deref()
                .map(parse_finite_decimal_string)
        );
        assert_eq!(
            exported.partial_entry_timer,
            envelope.runtime_private_extension.partial_entry_timer
        );
        assert_eq!(
            exported.cleanup_retry_state,
            envelope.runtime_private_extension.cleanup_retry_state
        );
        assert!(exported
            .expected_working_sets
            .expected_working_order_ids
            .is_empty());
    }

    #[test]
    fn stage5d_b2b_runtime_private_apply_blocks_without_partial_mutation() {
        let mut envelope = valid_fixture();
        let mut strategy = stage5d_test_strategy();
        restore_semantic_state(&mut strategy, &envelope);
        let before = strategy.stage5d_export_runtime_private_extension();

        envelope.runtime_private_extension.pending_entry = None;

        assert_eq!(
            strategy.stage5d_apply_runtime_private_extension(&envelope.runtime_private_extension),
            Err(Stage5dEnvelopeValidationError::PendingStateInconsistent)
        );
        assert_eq!(strategy.stage5d_export_runtime_private_extension(), before);
    }

    #[test]
    fn stage5d_b2b_public_bind_rejects_account_mismatch_before_apply() {
        let (loaded, mut envelope) = stage5d_bound_test_fixture();
        envelope.binding.account_id = BrokerAccountId::new("ACC_TEST_0002");
        envelope.payload_checksum_sha256 = envelope
            .compute_payload_checksum_sha256()
            .expect("checksum recomputation must succeed");
        let validated = envelope
            .validate_restore_contract_schema_only()
            .expect("envelope remains schema valid");

        let blocked = expect_stage5d_blocked(
            stage5d_bind_runtime_state_loaded(loaded, validated),
            "cross-account loaded/envelope pair must be blocked",
        );
        assert_eq!(
            blocked.reason(),
            Stage5dEnvelopeValidationError::BindingMismatch
        );
        assert!(blocked.input_capability_preserved());
    }

    #[test]
    fn stage5d_b2b_public_bind_rejects_instrument_mismatch_before_apply() {
        let (loaded, mut envelope) = stage5d_bound_test_fixture();
        envelope.binding.instrument_id.symbol = "RTS-9.26".to_string();
        envelope.payload_checksum_sha256 = envelope
            .compute_payload_checksum_sha256()
            .expect("checksum recomputation must succeed");
        let validated = envelope
            .validate_restore_contract_schema_only()
            .expect("envelope remains schema valid");

        let blocked = expect_stage5d_blocked(
            stage5d_bind_runtime_state_loaded(loaded, validated),
            "cross-instrument loaded/envelope pair must be blocked",
        );
        assert_eq!(
            blocked.reason(),
            Stage5dEnvelopeValidationError::BindingMismatch
        );
    }

    #[test]
    fn stage5d_b2bd1r6_earlier_gate_rejects_strategy_account_instrument_mismatches() {
        struct Case {
            name: &'static str,
            mutate: fn(&mut Stage5dPersistenceEnvelope),
        }

        fn strategy_mismatch(envelope: &mut Stage5dPersistenceEnvelope) {
            envelope.binding.strategy_id = "hybrid_imoexf_other_strategy".to_string();
            envelope.riskgate.identity.strategy_id = envelope.binding.strategy_id.clone();
        }

        fn account_mismatch(envelope: &mut Stage5dPersistenceEnvelope) {
            envelope.binding.account_id = BrokerAccountId::new("ACC_TEST_0002");
        }

        fn instrument_mismatch(envelope: &mut Stage5dPersistenceEnvelope) {
            envelope.binding.instrument_id.symbol = "RTS-9.26".to_string();
        }

        for case in [
            Case {
                name: "r6 earlier-owned strategy mismatch before restored transition",
                mutate: strategy_mismatch,
            },
            Case {
                name: "r6 earlier-owned account mismatch before restored transition",
                mutate: account_mismatch,
            },
            Case {
                name: "r6 earlier-owned instrument mismatch before restored transition",
                mutate: instrument_mismatch,
            },
        ] {
            let (loaded, mut envelope) = stage5d_bound_test_fixture();
            (case.mutate)(&mut envelope);
            envelope.payload_checksum_sha256 = envelope
                .compute_payload_checksum_sha256()
                .expect("checksum recomputation must succeed");
            let validated = envelope
                .validate_restore_contract_schema_only()
                .expect("mismatched identity envelope remains schema-valid");

            let blocked = expect_stage5d_blocked(
                stage5d_bind_runtime_state_loaded(loaded, validated),
                case.name,
            );
            assert_eq!(
                blocked.reason(),
                Stage5dEnvelopeValidationError::BindingMismatch
            );
        }
    }

    #[test]
    fn stage5d_b2bd1r6_earlier_gate_rejects_config_and_profile_mismatches() {
        struct Case {
            name: &'static str,
            mutate: fn(&mut Stage5dPersistenceEnvelope),
        }

        fn config_mismatch(envelope: &mut Stage5dPersistenceEnvelope) {
            envelope.binding.stage5c_compat_config_fingerprint =
                "stage5c_cfg_sha256:ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
                    .to_string();
        }

        fn profile_mismatch(envelope: &mut Stage5dPersistenceEnvelope) {
            envelope.binding.profile_binding =
                "hybrid|author41|riskgate-enabled|wrong-profile".to_string();
        }

        for case in [
            Case {
                name: "r6 exact config fingerprint mismatch before restored transition",
                mutate: config_mismatch,
            },
            Case {
                name: "r6 exact profile binding mismatch before restored transition",
                mutate: profile_mismatch,
            },
        ] {
            let (loaded, mut envelope) = stage5d_bound_test_fixture();
            (case.mutate)(&mut envelope);
            envelope.payload_checksum_sha256 = envelope
                .compute_payload_checksum_sha256()
                .expect("checksum recomputation must succeed");
            let validated = envelope
                .validate_restore_contract_schema_only()
                .expect("mismatched config/profile envelope remains schema-valid");

            let blocked = expect_stage5d_blocked(
                stage5d_bind_runtime_state_loaded(loaded, validated),
                case.name,
            );
            assert_eq!(
                blocked.reason(),
                Stage5dEnvelopeValidationError::BindingMismatch
            );
        }
    }

    #[test]
    fn stage5d_b2b_public_bind_rejects_semantic_state_mismatch_before_apply() {
        let (loaded, mut envelope) = stage5d_bound_test_fixture();
        if let Value::Object(fields) =
            &mut envelope.strategy_state.strategy_state_json["HybridIntradayRuntime"]
        {
            fields.insert("safe_mode_close_only".to_string(), Value::Bool(true));
        }
        envelope.payload_checksum_sha256 = envelope
            .compute_payload_checksum_sha256()
            .expect("checksum recomputation must succeed");
        let validated = envelope
            .validate_restore_contract_schema_only()
            .expect("envelope remains schema valid");

        let blocked = expect_stage5d_blocked(
            stage5d_bind_runtime_state_loaded(loaded, validated),
            "semantic mismatch must be blocked before apply",
        );
        assert_eq!(
            blocked.reason(),
            Stage5dEnvelopeValidationError::BindingMismatch
        );
    }

    #[test]
    fn stage5d_b2b_public_bind_rejects_recovery_index_mismatch_before_apply() {
        let (loaded, envelope) = stage5d_bound_test_fixture();
        let validated = envelope
            .validate_restore_contract_schema_only()
            .expect("envelope remains schema valid");
        let (strategy, admission, restored, load_origin) = loaded.stage5d_into_parts();
        let mismatched_loaded =
            crate::stage5c_paper_host::Stage5cRuntimeStateLoadedPaperStrategy::stage5d_test_loaded_from_parts(
                strategy,
                admission,
                crate::runtime_compat::RuntimeStateRestored {
                    known_order_ids: Vec::new(),
                    pending_requests: restored.pending_requests,
                },
                load_origin,
            );

        let blocked = expect_stage5d_blocked(
            stage5d_bind_runtime_state_loaded(mismatched_loaded, validated),
            "recovery-index mismatch must be blocked before apply",
        );
        assert_eq!(
            blocked.reason(),
            Stage5dEnvelopeValidationError::RecoveryIndexInconsistent
        );
    }

    #[test]
    fn stage5d_b2b_public_bind_rejects_clean_loaded_against_nonclean_envelope() {
        let (loaded, envelope) = stage5d_bound_test_fixture();
        let validated = envelope
            .validate_restore_contract_schema_only()
            .expect("envelope remains schema valid");
        let (strategy, admission, _restored, _load_origin) = loaded.stage5d_into_parts();
        let clean_loaded =
            crate::stage5c_paper_host::Stage5cRuntimeStateLoadedPaperStrategy::stage5d_test_loaded_from_parts(
                strategy,
                admission,
                crate::runtime_compat::RuntimeStateRestored {
                    known_order_ids: Vec::new(),
                    pending_requests: Vec::new(),
                },
                crate::stage5c_paper_host::Stage5cRuntimeStateLoadOrigin::CleanStart,
            );

        let blocked = expect_stage5d_blocked(
            stage5d_bind_runtime_state_loaded(clean_loaded, validated),
            "clean loaded capability cannot stand in for persisted restore",
        );
        assert_eq!(
            blocked.reason(),
            Stage5dEnvelopeValidationError::RecoveryIndexInconsistent
        );
    }

    #[test]
    fn stage5d_b2b_public_bound_pair_applies_and_retains_restore_evidence() {
        let (loaded, envelope) = stage5d_bound_test_fixture();
        let checksum = envelope.payload_checksum_sha256.clone();
        let validated = envelope
            .validate_restore_contract_schema_only()
            .expect("envelope remains schema valid");

        let bound = expect_stage5d_ok(
            stage5d_bind_runtime_state_loaded(loaded, validated),
            "exact loaded/envelope pair must bind",
        );
        assert_eq!(bound.snapshot_id(), "SNAP_STAGE5D_B2A_0001");
        assert_eq!(bound.evidence_fingerprint(), checksum);

        let applied = expect_stage5d_ok(
            stage5d_apply_runtime_private_extension(bound),
            "bound private extension must apply",
        );
        assert_eq!(applied.snapshot_id(), "SNAP_STAGE5D_B2A_0001");
        assert_eq!(
            applied.schema_version(),
            STAGE5D_PERSISTENCE_ENVELOPE_SCHEMA_VERSION
        );
        assert_eq!(applied.evidence_fingerprint(), checksum);
        assert!(applied.runtime_private_applied());
    }

    #[test]
    fn stage5d_b2bb_controlled_broker_truth_bootstrap_succeeds_after_private_apply() {
        let (applied, envelope) = applied_stage5d_fixture();
        let checksum = envelope.payload_checksum_sha256.clone();

        let bootstrapped = expect_stage5d_bootstrap_ok(
            stage5d_notify_broker_truth_bootstrap_at(applied, envelope.persisted_at_ts_utc),
            "exact broker truth bootstrap must succeed",
        );

        assert_eq!(bootstrapped.snapshot_id(), "SNAP_STAGE5D_B2A_0001");
        assert_eq!(
            bootstrapped.schema_version(),
            STAGE5D_PERSISTENCE_ENVELOPE_SCHEMA_VERSION
        );
        assert_eq!(bootstrapped.evidence_fingerprint(), checksum);
        assert!(bootstrapped.bootstrap_notified());
    }

    #[test]
    fn stage5d_b2bc_authoritative_riskgate_injection_succeeds_after_bootstrap() {
        let (bootstrapped, envelope, evidence) =
            riskgate_enabled_bootstrapped_fixture_with(|envelope| {
                align_riskgate_outbox_to_runtime_pending(envelope);
            });
        let checksum = envelope.payload_checksum_sha256.clone();
        let riskgate_evidence_fingerprint = evidence.evidence_fingerprint_sha256().to_string();

        let injected = expect_stage5d_riskgate_ok(
            stage5d_inject_authoritative_riskgate(bootstrapped, evidence),
            "authoritative riskgate state must inject after bootstrap",
        );

        assert_eq!(injected.snapshot_id(), "SNAP_STAGE5D_B2A_0001");
        assert_eq!(
            injected.schema_version(),
            STAGE5D_PERSISTENCE_ENVELOPE_SCHEMA_VERSION
        );
        assert_eq!(injected.evidence_fingerprint(), checksum);
        assert!(injected.riskgate_injected());
        assert!(riskgate_evidence_fingerprint.starts_with("stage5d_riskgate_evidence_sha256:"));
        assert_eq!(injected.recovery_decision_count(), 2);
        assert!(!injected.recovery_complete());
        assert!(injected
            .recovery_plan_fingerprint()
            .starts_with("stage5d_riskgate_recovery_plan_sha256:"));
    }

    #[test]
    fn stage5d_b2bc1r3_rejects_semantic_current_shadow_overlap_drift() {
        let mutations: [fn(&mut Stage5dPersistenceEnvelope); 4] = [
            |envelope| {
                envelope.strategy_state.strategy_state_json["HybridIntradayRuntime"]
                    ["risk_gate_shadow_session_date"] = Value::Null;
            },
            |envelope| {
                envelope.strategy_state.strategy_state_json["HybridIntradayRuntime"]
                    ["risk_gate_shadow_session_date"] = Value::String("2026-07-13".to_string());
            },
            |envelope| {
                envelope.strategy_state.strategy_state_json["HybridIntradayRuntime"]
                    ["risk_gate_shadow_pnl_points"] = serde_json::json!(123.0);
            },
            |envelope| {
                envelope.strategy_state.strategy_state_json["HybridIntradayRuntime"]
                    ["risk_gate_shadow_pnl_points"] = serde_json::json!(0.0000000000000001);
            },
        ];

        for mutate in mutations {
            let (mut bootstrapped, _envelope, evidence) =
                riskgate_enabled_bootstrapped_fixture_with(|envelope| {
                    align_riskgate_outbox_to_runtime_pending(envelope);
                });
            mutate(&mut bootstrapped.envelope);
            bootstrapped.envelope.payload_checksum_sha256 = bootstrapped
                .envelope
                .compute_payload_checksum_sha256()
                .expect("mutated checksum");
            let blocked = expect_stage5d_riskgate_blocked(
                stage5d_inject_authoritative_riskgate(bootstrapped, evidence),
                "semantic current-shadow overlap must exactly match authoritative state",
            );
            assert_eq!(
                blocked.reason(),
                Stage5dRiskGateInjectionBlockReason::MaterializedStateMismatch
            );
        }
    }

    #[test]
    fn stage5d_b2bc1r3_rejects_materialized_current_shadow_decimal_drift() {
        let mutations: [fn(&mut Stage5dPersistenceEnvelope); 4] = [
            |envelope| {
                envelope
                    .riskgate
                    .materialized_state
                    .current_shadow_pnl_points
                    .clear()
            },
            |envelope| {
                envelope
                    .riskgate
                    .materialized_state
                    .current_shadow_pnl_points = "0".to_string()
            },
            |envelope| {
                envelope
                    .riskgate
                    .materialized_state
                    .current_shadow_pnl_points = "0.5000000000000001".to_string()
            },
            |envelope| {
                envelope
                    .riskgate
                    .materialized_state
                    .current_shadow_session_date = Some("2026-07-13".to_string())
            },
        ];

        for mutate in mutations {
            let (bootstrapped, _envelope, evidence) =
                riskgate_enabled_bootstrapped_fixture_with(|envelope| {
                    align_riskgate_outbox_to_runtime_pending(envelope);
                    mutate(envelope);
                });
            let blocked = expect_stage5d_riskgate_blocked(
                stage5d_inject_authoritative_riskgate(bootstrapped, evidence),
                "materialized current-shadow authority must be explicit and exact",
            );
            assert!(matches!(
                blocked.reason(),
                Stage5dRiskGateInjectionBlockReason::MaterializedStateInvalid
                    | Stage5dRiskGateInjectionBlockReason::MaterializedStateMismatch
            ));
        }
    }

    #[test]
    fn stage5d_b2bc1r3_rejects_runtime_lag_without_exact_pending() {
        for (state, ledger, materialized, runtime, pending) in [
            (
                Stage5dRiskGateFinalizationState::Prepared,
                false,
                false,
                false,
                false,
            ),
            (
                Stage5dRiskGateFinalizationState::LedgerAppended,
                true,
                false,
                false,
                false,
            ),
            (
                Stage5dRiskGateFinalizationState::MaterializedUpdated,
                true,
                true,
                false,
                false,
            ),
            (
                Stage5dRiskGateFinalizationState::AcknowledgedInRuntime,
                true,
                true,
                true,
                true,
            ),
        ] {
            let (bootstrapped, _envelope, evidence) =
                riskgate_enabled_bootstrapped_fixture_with_evidence(|envelope, evidence| {
                    stage5d_test_configure_single_tail_crash_frontier(
                        envelope,
                        evidence,
                        state,
                        ledger,
                        materialized,
                        runtime,
                        pending,
                    );
                });
            let blocked = expect_stage5d_riskgate_blocked(
                stage5d_inject_authoritative_riskgate(bootstrapped, evidence),
                "runtime lagging rows require exact pending and acknowledged rows forbid it",
            );
            assert_eq!(
                blocked.reason(),
                Stage5dRiskGateInjectionBlockReason::OutboxStateInconsistent
            );
        }
    }

    #[test]
    fn stage5d_b2bc1r3_recovery_frontiers_are_stepwise_executable_to_completion() {
        for initial in [
            (
                Stage5dRiskGateFinalizationState::Prepared,
                false,
                false,
                false,
                true,
            ),
            (
                Stage5dRiskGateFinalizationState::Prepared,
                true,
                false,
                false,
                true,
            ),
            (
                Stage5dRiskGateFinalizationState::LedgerAppended,
                true,
                false,
                false,
                true,
            ),
            (
                Stage5dRiskGateFinalizationState::MaterializedUpdated,
                true,
                true,
                false,
                true,
            ),
            (
                Stage5dRiskGateFinalizationState::AcknowledgedInRuntime,
                true,
                true,
                true,
                false,
            ),
        ] {
            stage5d_test_assert_single_tail_frontier_reaches_recovery_complete(initial);
        }
    }

    #[test]
    fn stage5d_b2bc1r2_accepts_exact_durable_crash_frontiers_end_to_end() {
        let cases = [
            (
                Stage5dRiskGateFinalizationState::LedgerAppended,
                false,
                false,
                true,
                Stage5dRiskGateRecoveryDecision::AdvanceToMaterialized,
                false,
            ),
            (
                Stage5dRiskGateFinalizationState::MaterializedUpdated,
                true,
                false,
                true,
                Stage5dRiskGateRecoveryDecision::ReackRuntime,
                false,
            ),
            (
                Stage5dRiskGateFinalizationState::AcknowledgedInRuntime,
                true,
                true,
                false,
                Stage5dRiskGateRecoveryDecision::AlreadyAcknowledged,
                true,
            ),
        ];

        for (state, materialized, runtime, pending, expected_action, complete) in cases {
            let (bootstrapped, _envelope, evidence) =
                riskgate_enabled_bootstrapped_fixture_with_evidence(|envelope, evidence| {
                    stage5d_test_configure_single_tail_crash_frontier(
                        envelope,
                        evidence,
                        state,
                        true,
                        materialized,
                        runtime,
                        pending,
                    );
                });
            let injected = expect_stage5d_riskgate_ok(
                stage5d_inject_authoritative_riskgate(bootstrapped, evidence),
                "exact durable crash frontier must be accepted",
            );
            assert_eq!(injected.recovery_plan.decisions.len(), 1);
            assert_eq!(injected.recovery_plan.decisions[0].action, expected_action);
            assert_eq!(injected.recovery_complete(), complete);
        }
    }

    #[test]
    fn stage5d_b2bc1r2_accepts_outbox_marker_lag_only_with_exact_projection_frontier() {
        for (state, expected_action) in [
            (
                Stage5dRiskGateFinalizationState::Prepared,
                Stage5dRiskGateRecoveryDecision::AdvanceToMaterialized,
            ),
            (
                Stage5dRiskGateFinalizationState::LedgerAppended,
                Stage5dRiskGateRecoveryDecision::AdvanceToMaterialized,
            ),
        ] {
            let (bootstrapped, _envelope, evidence) =
                riskgate_enabled_bootstrapped_fixture_with_evidence(|envelope, evidence| {
                    stage5d_test_configure_single_tail_crash_frontier(
                        envelope, evidence, state, true, true, false, true,
                    );
                });
            let injected = expect_stage5d_riskgate_ok(
                stage5d_inject_authoritative_riskgate(bootstrapped, evidence),
                "outbox marker may lag only when exact later projection is durable",
            );
            assert_eq!(injected.recovery_plan.decisions[0].action, expected_action);
        }
    }

    #[test]
    fn stage5d_b2bc1r2_retained_recovery_plan_is_bound_and_tamper_evident() {
        let (bootstrapped, envelope, evidence) = riskgate_enabled_bootstrapped_fixture_with(|_| {});
        let duplicate_evidence =
            stage5d_validate_riskgate_ledger_evidence(evidence.evidence.clone()).unwrap();
        let mut injected = expect_stage5d_riskgate_ok(
            stage5d_inject_authoritative_riskgate(bootstrapped, evidence),
            "retained plan fixture must inject",
        );
        assert!(stage5d_validate_riskgate_recovery_plan_binding(
            &injected.recovery_plan,
            &envelope,
            &duplicate_evidence,
        )
        .is_ok());

        let mut wrong_envelope = envelope.clone();
        wrong_envelope.payload_checksum_sha256.push_str("-wrong");
        assert_eq!(
            stage5d_validate_riskgate_recovery_plan_binding(
                &injected.recovery_plan,
                &wrong_envelope,
                &duplicate_evidence,
            ),
            Err(Stage5dRiskGateInjectionBlockReason::RecoveryPlanBindingMismatch)
        );

        let mut wrong_evidence =
            stage5d_validate_riskgate_ledger_evidence(duplicate_evidence.evidence.clone()).unwrap();
        wrong_evidence
            .evidence_fingerprint_sha256
            .push_str("-wrong");
        assert_eq!(
            stage5d_validate_riskgate_recovery_plan_binding(
                &injected.recovery_plan,
                &envelope,
                &wrong_evidence,
            ),
            Err(Stage5dRiskGateInjectionBlockReason::RecoveryPlanBindingMismatch)
        );

        injected.recovery_plan.local.runtime_records_count += 1;
        assert_eq!(
            stage5d_validate_riskgate_recovery_plan_binding(
                &injected.recovery_plan,
                &envelope,
                &duplicate_evidence,
            ),
            Err(Stage5dRiskGateInjectionBlockReason::RecoveryPlanBindingMismatch)
        );
    }

    #[test]
    fn stage5d_b2bc_riskgate_injection_blocks_materialized_semantic_drift() {
        let (bootstrapped, _envelope, evidence) =
            riskgate_enabled_bootstrapped_fixture_with(|envelope| {
                align_riskgate_outbox_to_runtime_pending(envelope);
                if let Value::Object(fields) =
                    &mut envelope.strategy_state.strategy_state_json["HybridIntradayRuntime"]
                {
                    fields.insert(
                        "risk_gate_rolling_sum_lb120".to_string(),
                        serde_json::json!(159.6),
                    );
                }
            });

        let blocked = expect_stage5d_riskgate_blocked(
            stage5d_inject_authoritative_riskgate(bootstrapped, evidence),
            "semantic/materialized riskgate drift must block injection",
        );
        assert_eq!(
            blocked.reason(),
            Stage5dRiskGateInjectionBlockReason::MaterializedStateMismatch
        );
        assert!(blocked.input_capability_preserved());
        assert_eq!(blocked.snapshot_id(), "SNAP_STAGE5D_B2A_0001");
    }

    #[test]
    fn stage5d_b2bc1r2_rejects_unexplained_materialized_drift_at_reack_frontier() {
        type Mutation = fn(&mut Stage5dPersistenceEnvelope);
        let mutations: [Mutation; 5] = [
            |envelope| {
                envelope.riskgate.materialized_state.rolling_sum_lb120 = Some("999.0".to_string())
            },
            |envelope| {
                let current = envelope
                    .riskgate
                    .materialized_state
                    .mr_enabled_current_session
                    .unwrap_or(false);
                envelope
                    .riskgate
                    .materialized_state
                    .mr_enabled_current_session = Some(!current);
            },
            |envelope| {
                envelope
                    .riskgate
                    .materialized_state
                    .last_finalized_session_date = Some("2026-01-02".to_string())
            },
            |envelope| envelope.riskgate.materialized_state.ledger_rows_count += 1,
            |envelope| {
                envelope.riskgate.materialized_state.seed_loaded =
                    !envelope.riskgate.materialized_state.seed_loaded
            },
        ];

        let frontiers = [
            (
                Stage5dRiskGateFinalizationState::Prepared,
                false,
                false,
                false,
                true,
            ),
            (
                Stage5dRiskGateFinalizationState::Prepared,
                true,
                false,
                false,
                true,
            ),
            (
                Stage5dRiskGateFinalizationState::LedgerAppended,
                true,
                false,
                false,
                true,
            ),
            (
                Stage5dRiskGateFinalizationState::MaterializedUpdated,
                true,
                true,
                false,
                true,
            ),
            (
                Stage5dRiskGateFinalizationState::AcknowledgedInRuntime,
                true,
                true,
                true,
                false,
            ),
        ];
        for &(state, ledger, materialized, runtime, pending) in &frontiers {
            for &mutate in &mutations {
                let (bootstrapped, _envelope, evidence) =
                    riskgate_enabled_bootstrapped_fixture_with_evidence(|envelope, evidence| {
                        stage5d_test_configure_single_tail_crash_frontier(
                            envelope,
                            evidence,
                            state,
                            ledger,
                            materialized,
                            runtime,
                            pending,
                        );
                        mutate(envelope);
                    });
                let blocked = expect_stage5d_riskgate_blocked(
                    stage5d_inject_authoritative_riskgate(bootstrapped, evidence),
                    "unexplained materialized drift must not be treated as crash lag",
                );
                assert_eq!(
                    blocked.reason(),
                    Stage5dRiskGateInjectionBlockReason::MaterializedStateMismatch
                );
            }
        }
    }

    #[test]
    fn stage5d_b2bc1r2_rejects_semantic_ahead_of_materialized_frontier() {
        let (bootstrapped, _envelope, evidence) =
            riskgate_enabled_bootstrapped_fixture_with_evidence(|envelope, evidence| {
                stage5d_test_configure_single_tail_crash_frontier(
                    envelope,
                    evidence,
                    Stage5dRiskGateFinalizationState::LedgerAppended,
                    true,
                    false,
                    true,
                    false,
                );
            });
        let blocked = expect_stage5d_riskgate_blocked(
            stage5d_inject_authoritative_riskgate(bootstrapped, evidence),
            "runtime frontier cannot advance beyond materialized frontier",
        );
        assert_eq!(
            blocked.reason(),
            Stage5dRiskGateInjectionBlockReason::MaterializedStateMismatch
        );
    }

    #[test]
    fn stage5d_b2bc1r2_decimal_text_must_equal_source_formatter_output() {
        for canonical in [
            "0.0",
            "2.0",
            "-0.5",
            "0.5",
            "0.5000000000000001",
            "158.60000000000008",
        ] {
            assert!(stage5d_require_source_canonical_riskgate_decimal(canonical).is_ok());
        }
        for noncanonical in [
            "", "-0.0", "-0", "0", "2", "02.0", ".5", "1.", "+1.0", "1e0", " NaN", "NaN", "inf",
            "-inf",
        ] {
            assert_eq!(
                stage5d_require_source_canonical_riskgate_decimal(noncanonical),
                Err(Stage5dRiskGateInjectionBlockReason::LedgerEvidenceInvalid),
                "non-source decimal must fail: {noncanonical}"
            );
        }
    }

    #[test]
    fn stage5d_b2bc1r5_actual_runtime_export_uses_source_canonical_riskgate_decimals() {
        let session_date = NaiveDate::from_ymd_opt(2026, 7, 14).expect("date");
        for (value, expected) in [
            (0.0, "0.0"),
            (2.0, "2.0"),
            (-0.5, "-0.5"),
            (0.5, "0.5"),
            (0.5000000000000001, "0.5000000000000001"),
            (158.60000000000008, "158.60000000000008"),
        ] {
            let mut strategy = stage5d_test_strategy_with_config(|config| {
                config.profile =
                    crate::hybrid_intraday_runtime::HybridIntradayProfile::ImoexfPrimaryRiskgateHigh180Lb120;
                config.risk_gate_mode = crate::hybrid_intraday_runtime::RiskGateMode::NormalAppend;
            });
            strategy.stage5d_test_replace_pending_riskgate_finalizations(vec![(
                session_date,
                value,
                1,
            )]);
            let exported = strategy
                .stage5d_export_runtime_private_extension()
                .expect("source-valid riskgate decimal must export");
            assert_eq!(
                exported.runtime_pending_finalizations[0].shadow_pnl_points, expected,
                "actual runtime export for {value} must be source-canonical"
            );
            let json = serde_json::to_string(&exported).expect("serialize extension");
            let restored: Stage5dRuntimePrivateExtension =
                serde_json::from_str(&json).expect("deserialize extension");
            assert_eq!(
                restored.runtime_pending_finalizations[0],
                exported.runtime_pending_finalizations[0]
            );
        }
    }

    #[test]
    fn stage5d_b2bc1r5_actual_runtime_export_rejects_invalid_riskgate_decimals() {
        let session_date = NaiveDate::from_ymd_opt(2026, 1, 6).expect("date");
        for value in [-0.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let mut strategy = stage5d_test_strategy_with_config(|config| {
                config.profile =
                    crate::hybrid_intraday_runtime::HybridIntradayProfile::ImoexfPrimaryRiskgateHigh180Lb120;
                config.risk_gate_mode = crate::hybrid_intraday_runtime::RiskGateMode::NormalAppend;
            });
            strategy.stage5d_test_replace_pending_riskgate_finalizations(vec![(
                session_date,
                value,
                1,
            )]);
            assert_eq!(
                strategy
                    .stage5d_export_runtime_private_extension()
                    .map(|_| ()),
                Err(Stage5dEnvelopeValidationError::RiskGateFinalizationInconsistent),
                "invalid actual runtime riskgate decimal must fail before durable authority export"
            );
        }
    }

    #[test]
    fn stage5d_b2bc1r6_source_owned_decimal_codec_controls_authority_fields() {
        for (value, expected) in [
            (0.0, "0.0"),
            (2.0, "2.0"),
            (-0.5, "-0.5"),
            (0.5, "0.5"),
            (0.5000000000000001, "0.5000000000000001"),
            (158.60000000000008, "158.60000000000008"),
        ] {
            let text = crate::hybrid_intraday::format_riskgate_authority_decimal(value)
                .expect("source-owned authority decimal must format");
            assert_eq!(text, expected);
            assert_eq!(
                crate::hybrid_intraday::parse_riskgate_authority_decimal(&text)
                    .expect("source-owned authority decimal must parse"),
                value
            );
        }

        for alias in [
            "0", "2", "-0.0", "-0", "+0.0", "0.0 ", " 0.0", "5e-1", "NaN", "inf", "-inf",
        ] {
            assert!(
                crate::hybrid_intraday::parse_riskgate_authority_decimal(alias).is_err(),
                "noncanonical or invalid authority decimal {alias:?} must fail"
            );
        }

        let mut state = crate::hybrid_intraday::RiskGateMaterializedState {
            last_finalized_session_date: None,
            rolling_sum_lb120: Some(2.0),
            mr_enabled_current_session: Some(true),
            mr_enabled_next_session: Some(true),
            seed_loaded: true,
            ledger_rows_count: 1,
            current_shadow_session_date: Some(NaiveDate::from_ymd_opt(2026, 7, 14).unwrap()),
            current_shadow_pnl_points: 0.0,
            current_generation: crate::hybrid_intraday::RISK_GATE_STATE_GENERATION.to_string(),
        };
        assert_eq!(
            state.authority_redis_fields().expect("authority fields")[2],
            ("current_shadow_pnl_points".to_string(), "0.0".to_string())
        );
        state.current_shadow_pnl_points = -0.0;
        assert!(state.authority_redis_fields().is_err());
    }

    #[test]
    fn stage5d_b2bc1r7_source_produced_pending_finalization_reaches_recovery_complete() {
        let (source_strategy, exported) =
            stage5d_test_source_runtime_with_real_pending_finalization();
        let source_pending = exported.runtime_pending_finalizations[0].clone();

        let mut envelope = flat_persisted_fixture();
        envelope.strategy_state.strategy_state_json =
            serde_json::to_value(source_strategy.state()).expect("source state serializes");
        envelope.runtime_private_extension = exported;
        let identity = stage5d_test_riskgate_identity_for(&source_strategy, &envelope);
        let base_evidence = stage5d_test_riskgate_evidence_before_source_pending(
            &identity,
            &envelope,
            Some("2026-01-07"),
        );
        stage5d_apply_riskgate_evidence_to_envelope(&mut envelope, &base_evidence);
        bind_fixture_to_strategy_config(&mut envelope, &source_strategy);
        envelope.riskgate.ledger_tail_hash = base_evidence.ledger_tail_hash.clone();
        envelope.payload_checksum_sha256 = envelope
            .compute_payload_checksum_sha256()
            .expect("checksum after exact source export");

        assert_eq!(
            envelope
                .runtime_private_extension
                .runtime_pending_finalizations[0],
            source_pending,
            "strict envelope must carry the exact source-exported pending finalization"
        );
        assert_eq!(
            envelope.riskgate.durable_finalization_outbox[0].state,
            Stage5dRiskGateFinalizationState::Prepared
        );
        assert_eq!(
            envelope.riskgate.durable_finalization_outbox[0].session_date,
            source_pending.session_date
        );

        let mut durable_state = Stage5dRiskGateFinalizationState::Prepared;
        let mut ledger = false;
        let mut materialized = false;
        let mut runtime = false;
        let mut pending = true;
        let mut saw_actions = Vec::new();

        for _ in 0..4 {
            let mut iteration_envelope = envelope.clone();
            let mut iteration_evidence = base_evidence.clone();
            stage5d_test_configure_single_tail_crash_frontier(
                &mut iteration_envelope,
                &mut iteration_evidence,
                durable_state,
                ledger,
                materialized,
                runtime,
                pending,
            );
            iteration_evidence.ledger_tail_hash =
                stage5d_compute_riskgate_ledger_tail_hash(&iteration_evidence)
                    .expect("iteration tail hash");
            iteration_envelope.riskgate.ledger_tail_hash =
                iteration_evidence.ledger_tail_hash.clone();
            iteration_envelope.payload_checksum_sha256 = iteration_envelope
                .compute_payload_checksum_sha256()
                .expect("iteration checksum");
            let (bootstrapped, strict_envelope, evidence) =
                stage5d_test_bootstrap_strict_envelope_with_strategy(
                    iteration_envelope,
                    stage5d_test_riskgate_runtime_strategy(),
                    iteration_evidence,
                );
            assert_eq!(
                strict_envelope
                    .runtime_private_extension
                    .runtime_pending_finalizations
                    .first()
                    .cloned(),
                pending.then_some(source_pending.clone())
            );
            let injected = expect_stage5d_riskgate_ok(
                stage5d_inject_authoritative_riskgate(bootstrapped, evidence),
                "source-produced strict envelope must inject riskgate",
            );
            let decision = injected.recovery_plan.decisions[0].action;
            saw_actions.push(decision);
            if injected.recovery_complete() {
                assert_eq!(
                    decision,
                    Stage5dRiskGateRecoveryDecision::AlreadyAcknowledged
                );
                assert!(ledger && materialized && runtime && !pending);
                break;
            }
            match decision {
                Stage5dRiskGateRecoveryDecision::AppendMissingLedgerRow => {
                    assert!(!ledger && !materialized && !runtime && pending);
                    ledger = true;
                    durable_state = Stage5dRiskGateFinalizationState::LedgerAppended;
                }
                Stage5dRiskGateRecoveryDecision::AdvanceToMaterialized => {
                    assert!(ledger && !runtime && pending);
                    materialized = true;
                    durable_state = Stage5dRiskGateFinalizationState::MaterializedUpdated;
                }
                Stage5dRiskGateRecoveryDecision::ReackRuntime => {
                    assert!(ledger && materialized && pending);
                    runtime = true;
                    pending = false;
                    durable_state = Stage5dRiskGateFinalizationState::AcknowledgedInRuntime;
                }
                Stage5dRiskGateRecoveryDecision::AlreadyAcknowledged => {
                    panic!("already-acknowledged cannot appear before recovery_complete")
                }
            }
        }

        assert_eq!(
            saw_actions,
            vec![
                Stage5dRiskGateRecoveryDecision::AppendMissingLedgerRow,
                Stage5dRiskGateRecoveryDecision::AdvanceToMaterialized,
                Stage5dRiskGateRecoveryDecision::ReackRuntime,
                Stage5dRiskGateRecoveryDecision::AlreadyAcknowledged,
            ],
            "source-produced pending finalization must walk the complete durable recovery path"
        );

        let mut complete_envelope = envelope.clone();
        let mut complete_evidence = base_evidence.clone();
        stage5d_test_configure_single_tail_crash_frontier(
            &mut complete_envelope,
            &mut complete_evidence,
            Stage5dRiskGateFinalizationState::AcknowledgedInRuntime,
            true,
            true,
            true,
            false,
        );
        complete_evidence.ledger_tail_hash =
            stage5d_compute_riskgate_ledger_tail_hash(&complete_evidence)
                .expect("complete tail hash");
        complete_envelope.riskgate.ledger_tail_hash = complete_evidence.ledger_tail_hash.clone();
        complete_envelope.payload_checksum_sha256 = complete_envelope
            .compute_payload_checksum_sha256()
            .expect("complete checksum");
        for _ in 0..2 {
            let (bootstrapped, strict_envelope, evidence) =
                stage5d_test_bootstrap_strict_envelope_with_strategy(
                    complete_envelope.clone(),
                    stage5d_test_riskgate_runtime_strategy(),
                    complete_evidence.clone(),
                );
            assert!(
                strict_envelope
                    .runtime_private_extension
                    .runtime_pending_finalizations
                    .is_empty(),
                "runtime-pending vector must stay empty after acknowledged replay"
            );
            let injected = expect_stage5d_riskgate_ok(
                stage5d_inject_authoritative_riskgate(bootstrapped, evidence),
                "complete source-produced recovery replay must remain accepted",
            );
            assert!(injected.recovery_complete());
            assert_eq!(
                injected.recovery_plan.decisions[0].action,
                Stage5dRiskGateRecoveryDecision::AlreadyAcknowledged
            );
        }
    }

    #[test]
    fn stage5d_b2bc1r7_source_produced_current_shadow_matrix_passes_full_path() {
        let true_clean = stage5d_test_source_current_shadow_strategy(Vec::new());
        stage5d_test_assert_source_current_shadow_full_path(true_clean, None, None, 0, "0.0");

        let no_session_weekend_ignored =
            stage5d_test_source_current_shadow_strategy(vec![stage5d_test_source_bar_ohlc(
                stage5d_test_source_ts_local(2026, 1, 10, 9, 0, 0),
                100.0,
                100.1,
                99.9,
                100.0,
            )]);
        stage5d_test_assert_source_current_shadow_full_path(
            no_session_weekend_ignored,
            None,
            None,
            0,
            "0.0",
        );

        let clean_session =
            stage5d_test_source_current_shadow_strategy(vec![stage5d_test_source_bar_ohlc(
                stage5d_test_source_ts_local(2026, 1, 6, 9, 0, 0),
                100.0,
                100.1,
                99.9,
                100.0,
            )]);
        stage5d_test_assert_source_current_shadow_full_path(
            clean_session,
            Some("2026-01-06"),
            None,
            0,
            "0.0",
        );

        let long_open =
            stage5d_test_source_current_shadow_strategy(vec![stage5d_test_source_bar_ohlc(
                stage5d_test_source_ts_local(2026, 1, 6, 9, 0, 0),
                99.7,
                102.0,
                99.7,
                99.7,
            )]);
        stage5d_test_assert_source_current_shadow_full_path(
            long_open,
            Some("2026-01-06"),
            Some("long"),
            0,
            "0.0",
        );

        let short_open =
            stage5d_test_source_current_shadow_strategy(vec![stage5d_test_source_bar_ohlc(
                stage5d_test_source_ts_local(2026, 1, 6, 9, 0, 0),
                100.3,
                100.3,
                98.0,
                100.3,
            )]);
        stage5d_test_assert_source_current_shadow_full_path(
            short_open,
            Some("2026-01-06"),
            Some("short"),
            0,
            "0.0",
        );

        let realized_pnl = stage5d_test_source_current_shadow_strategy(vec![
            stage5d_test_source_bar_ohlc(
                stage5d_test_source_ts_local(2026, 1, 6, 9, 0, 0),
                99.7,
                102.0,
                99.7,
                99.7,
            ),
            stage5d_test_source_bar_ohlc(
                stage5d_test_source_ts_local(2026, 1, 6, 9, 10, 0),
                101.0,
                101.0,
                100.8,
                101.0,
            ),
        ]);
        stage5d_test_assert_source_current_shadow_full_path(
            realized_pnl,
            Some("2026-01-06"),
            None,
            1,
            "1.199999999999997",
        );

        let weekend_later_watermark = stage5d_test_source_current_shadow_strategy(vec![
            stage5d_test_source_bar_ohlc(
                stage5d_test_source_ts_local(2026, 1, 6, 9, 0, 0),
                100.0,
                100.1,
                99.9,
                100.0,
            ),
            stage5d_test_source_bar_ohlc(
                stage5d_test_source_ts_local(2026, 1, 10, 9, 0, 0),
                100.0,
                100.1,
                99.9,
                100.0,
            ),
        ]);
        stage5d_test_assert_source_current_shadow_full_path(
            weekend_later_watermark,
            Some("2026-01-06"),
            None,
            0,
            "0.0",
        );

        let before_model_later_watermark = stage5d_test_source_current_shadow_strategy_with_config(
            |config| {
                config.model_session_start_time = NaiveTime::from_hms_opt(9, 0, 0);
                config.model_session_end_time = NaiveTime::from_hms_opt(18, 45, 0);
            },
            vec![
                stage5d_test_source_bar_ohlc(
                    stage5d_test_source_ts_local(2026, 1, 6, 9, 0, 0),
                    100.0,
                    100.1,
                    99.9,
                    100.0,
                ),
                stage5d_test_source_bar_ohlc(
                    stage5d_test_source_ts_local(2026, 1, 7, 8, 30, 0),
                    100.0,
                    100.1,
                    99.9,
                    100.0,
                ),
            ],
        );
        stage5d_test_assert_source_current_shadow_full_path(
            before_model_later_watermark,
            Some("2026-01-06"),
            None,
            0,
            "0.0",
        );

        let after_model_later_watermark = stage5d_test_source_current_shadow_strategy_with_config(
            |config| {
                config.model_session_start_time = NaiveTime::from_hms_opt(9, 0, 0);
                config.model_session_end_time = NaiveTime::from_hms_opt(18, 45, 0);
            },
            vec![
                stage5d_test_source_bar_ohlc(
                    stage5d_test_source_ts_local(2026, 1, 6, 9, 0, 0),
                    100.0,
                    100.1,
                    99.9,
                    100.0,
                ),
                stage5d_test_source_bar_ohlc(
                    stage5d_test_source_ts_local(2026, 1, 7, 19, 0, 0),
                    100.0,
                    100.1,
                    99.9,
                    100.0,
                ),
            ],
        );
        stage5d_test_assert_source_current_shadow_full_path(
            after_model_later_watermark,
            Some("2026-01-06"),
            None,
            0,
            "0.0",
        );
    }

    #[test]
    fn stage5d_b2bc1r8_later_regular_model_watermark_must_advance_source_session() {
        let advanced = stage5d_test_source_current_shadow_strategy(vec![
            stage5d_test_source_bar_ohlc(
                stage5d_test_source_ts_local(2026, 1, 6, 9, 0, 0),
                100.0,
                100.1,
                99.9,
                100.0,
            ),
            stage5d_test_source_bar_ohlc(
                stage5d_test_source_ts_local(2026, 1, 7, 9, 0, 0),
                100.0,
                100.1,
                99.9,
                100.0,
            ),
        ]);
        let state_json = serde_json::to_value(advanced.state()).expect("state");
        assert_eq!(
            state_json["HybridIntradayRuntime"]["risk_gate_shadow_session_date"].as_str(),
            Some("2026-01-07")
        );
        let exported = advanced
            .stage5d_export_runtime_private_extension()
            .expect("export");
        assert_eq!(exported.runtime_pending_finalizations.len(), 1);
        assert_eq!(
            exported.runtime_pending_finalizations[0].session_date,
            "2026-01-06"
        );
    }

    #[test]
    fn stage5d_b2bc1r8_rejects_stale_session_after_later_regular_model_watermark() {
        let stale =
            stage5d_test_source_current_shadow_strategy(vec![stage5d_test_source_bar_ohlc(
                stage5d_test_source_ts_local(2026, 1, 6, 9, 0, 0),
                100.0,
                100.1,
                99.9,
                100.0,
            )]);
        let mut envelope = flat_persisted_fixture();
        envelope.strategy_state.strategy_state_json =
            serde_json::to_value(stale.state()).expect("source state serializes");
        envelope.runtime_private_extension = stale
            .stage5d_export_runtime_private_extension()
            .expect("source export");
        envelope.runtime_private_extension.last_processed_bar_ts = Some(
            DateTime::<Utc>::from_timestamp(stage5d_test_source_ts_local(2026, 1, 7, 9, 0, 0), 0)
                .expect("regular model watermark"),
        );
        let identity = stage5d_test_riskgate_identity_for(&stale, &envelope);
        let mut evidence = stage5d_test_riskgate_evidence_before_source_pending(
            &identity,
            &envelope,
            Some("2026-01-06"),
        );
        stage5d_apply_riskgate_evidence_to_envelope(&mut envelope, &evidence);
        bind_fixture_to_strategy_config(&mut envelope, &stale);
        envelope.riskgate.ledger_tail_hash = evidence.ledger_tail_hash.clone();
        envelope.payload_checksum_sha256 = envelope
            .compute_payload_checksum_sha256()
            .expect("checksum");
        stage5d_test_normalize_persisted_checksum(&mut envelope);
        evidence.ledger_tail_hash =
            stage5d_compute_riskgate_ledger_tail_hash(&evidence).expect("tail hash");

        let (bootstrapped, _strict_envelope, evidence) =
            stage5d_test_bootstrap_strict_envelope_with_strategy(
                envelope,
                stage5d_test_riskgate_runtime_strategy(),
                evidence,
            );
        let blocked = expect_stage5d_riskgate_blocked(
            stage5d_inject_authoritative_riskgate(bootstrapped, evidence),
            "old current shadow cannot survive a later regular model-session watermark",
        );
        assert_eq!(
            blocked.reason(),
            Stage5dRiskGateInjectionBlockReason::MaterializedStateMismatch
        );
    }

    #[test]
    fn stage5d_b2bd_runtime_state_restored_success_returns_stage5c_restored_capability() {
        let injected = stage5d_test_complete_injected_fixture();
        let restored_at = injected.envelope.persisted_at_ts_utc;
        stage5d_test_reset_restored_callback_count();
        let restored = expect_stage5d_restore_ok(
            stage5d_notify_runtime_state_restored_at(injected, restored_at),
            "complete injected capability must return to Stage 5C restored",
        );

        assert_eq!(stage5d_test_restored_callback_count(), 1);
        stage5d_test_assert_restored_success_baseline(&restored);
        assert!(restored.receipt().pending_requests().is_empty());
    }

    #[test]
    fn stage5d_b2bd1r3_source_produced_current_shadow_long_short_and_realized_pnl_restore() {
        let long_open =
            stage5d_test_source_current_shadow_strategy(vec![stage5d_test_source_bar_ohlc(
                stage5d_test_source_ts_local(2026, 1, 6, 9, 0, 0),
                99.7,
                102.0,
                99.7,
                99.7,
            )]);
        stage5d_test_assert_source_current_shadow_full_path(
            long_open,
            Some("2026-01-06"),
            Some("long"),
            0,
            "0.0",
        );

        let short_open =
            stage5d_test_source_current_shadow_strategy(vec![stage5d_test_source_bar_ohlc(
                stage5d_test_source_ts_local(2026, 1, 6, 9, 0, 0),
                100.3,
                100.3,
                98.0,
                100.3,
            )]);
        stage5d_test_assert_source_current_shadow_full_path(
            short_open,
            Some("2026-01-06"),
            Some("short"),
            0,
            "0.0",
        );

        let realized_pnl = stage5d_test_source_current_shadow_strategy(vec![
            stage5d_test_source_bar_ohlc(
                stage5d_test_source_ts_local(2026, 1, 6, 9, 0, 0),
                99.7,
                102.0,
                99.7,
                99.7,
            ),
            stage5d_test_source_bar_ohlc(
                stage5d_test_source_ts_local(2026, 1, 6, 9, 10, 0),
                101.0,
                101.0,
                100.8,
                101.0,
            ),
        ]);
        stage5d_test_assert_source_current_shadow_full_path(
            realized_pnl,
            Some("2026-01-06"),
            None,
            1,
            "1.199999999999997",
        );
    }

    #[test]
    fn stage5d_b2bd1r4_broker_position_long_short_restore_with_genuine_open_positions() {
        for (qty, side, expected_side) in [
            (3.0, "long", crate::hybrid_intraday::Side::Long),
            (-3.0, "short", crate::hybrid_intraday::Side::Short),
        ] {
            let injected =
                stage5d_test_complete_strict_injected_fixture_with_position(qty, Some(side));
            let r6_strict_marker = match expected_side {
                crate::hybrid_intraday::Side::Long => {
                    "r6 strict JSON round-trip actual Long broker-position evidence"
                }
                crate::hybrid_intraday::Side::Short => {
                    "r6 strict JSON round-trip actual Short broker-position evidence"
                }
            };
            assert!(
                injected
                    .envelope
                    .validate_restore_contract_schema_only()
                    .is_ok(),
                "r5 strict JSON round-trip broker-position {side} evidence must remain schema-valid"
            );
            assert!(
                injected
                    .envelope
                    .validate_restore_contract_schema_only()
                    .is_ok(),
                "{r6_strict_marker}"
            );
            let snapshot = injected
                .bootstrapped
                .stage5d_admission()
                .bootstrap_snapshot();
            assert_eq!(
                snapshot.target_open_positions.len(),
                1,
                "r4 actual broker-position positive must use a genuine broker position row"
            );
            assert_eq!(
                snapshot.target_open_positions[0].qty,
                Decimal::from_f64_retain(qty).expect("test qty decimal"),
                "r4 broker position row qty must match canonical imported runtime qty"
            );
            let state = Strategy::state(injected.bootstrapped.stage5d_strategy());
            let StrategyState::HybridIntradayRuntime {
                last_position_qty,
                current_side,
                ..
            } = state
            else {
                panic!("test fixture must be hybrid runtime state");
            };
            assert_eq!(*last_position_qty, qty);
            assert_eq!(*current_side, Some(expected_side));

            let restored = stage5d_test_assert_injected_restores_indexes_once(
                injected,
                &[],
                &[],
                "r4 broker-consistent open Long/Short position must restore",
            );
            let restored_state = Strategy::state(restored.stage5d_strategy());
            let StrategyState::HybridIntradayRuntime {
                last_position_qty,
                current_side,
                ..
            } = restored_state
            else {
                panic!("restored fixture must be hybrid runtime state");
            };
            assert_eq!(*last_position_qty, qty);
            assert_eq!(*current_side, Some(expected_side));
        }
    }

    #[test]
    fn stage5d_b2bd1r4_non_empty_recovery_indexes_preserve_through_restored_receipt() {
        let known_order_id = BrokerOrderId::new("STAGE5D-R4-KNOWN-ORDER");
        let (bootstrapped, _envelope, validated_evidence) =
            riskgate_enabled_strict_bootstrapped_fixture_with_evidence(|envelope, evidence| {
                envelope.recovery_indexes.known_order_ids = vec![known_order_id.clone()];
                stage5d_test_configure_single_tail_crash_frontier(
                    envelope,
                    evidence,
                    Stage5dRiskGateFinalizationState::AcknowledgedInRuntime,
                    true,
                    true,
                    true,
                    false,
                );
            });
        let injected = expect_stage5d_riskgate_ok(
            stage5d_inject_authoritative_riskgate(bootstrapped, validated_evidence),
            "known-order index positive fixture must inject riskgate",
        );
        assert!(injected.recovery_complete());
        assert!(
            injected
                .envelope
                .validate_restore_contract_schema_only()
                .is_ok(),
            "r5 strict JSON round-trip known-order index evidence must remain schema-valid"
        );
        assert!(
            injected
                .envelope
                .validate_restore_contract_schema_only()
                .is_ok(),
            "r6 strict JSON round-trip known-order index evidence must remain schema-valid"
        );
        stage5d_test_assert_injected_restores_indexes_once(
            injected,
            &[known_order_id],
            &[],
            "r4 non-empty known-order index must be preserved",
        );

        let pending_injected = stage5d_test_complete_injected_pending_entry_index_fixture();
        let expected_pending = pending_injected
            .envelope
            .recovery_indexes
            .pending_requests
            .clone();
        assert!(
            !expected_pending.is_empty(),
            "r4 pending-request positive must carry a non-empty source-compatible index"
        );
        assert!(
            pending_injected
                .envelope
                .validate_restore_contract_schema_only()
                .is_ok(),
            "r5 strict JSON round-trip pending-request index evidence must remain schema-valid"
        );
        assert!(
            pending_injected
                .envelope
                .validate_restore_contract_schema_only()
                .is_ok(),
            "r6 strict JSON round-trip pending-request index evidence must remain schema-valid"
        );
        stage5d_test_assert_injected_restores_indexes_once(
            pending_injected,
            &[],
            &expected_pending,
            "r4 non-empty pending-request index must be preserved",
        );
    }

    #[test]
    fn stage5d_b2bd1r6_strict_malformed_payload_shapes_fail_closed() {
        let mut malformed_known_order = serde_json::to_value(flat_persisted_fixture())
            .expect("flat fixture serializes to JSON");
        malformed_known_order["recovery_indexes"]["known_order_ids"] = serde_json::json!([""]);
        let malformed_known_order_payload =
            serde_json::to_string(&malformed_known_order).expect("malformed payload serializes");
        assert_eq!(
            Stage5dPersistenceEnvelope::from_json_str_strict(&malformed_known_order_payload)
                .map(|_| ()),
            Err(Stage5dEnvelopeValidationError::DeserializationFailed),
            "r6 strict malformed broker order id payload must fail during strict decode"
        );

        let impossible_pending_request = flat_persisted_fixture();
        let mut impossible_pending_value = serde_json::to_value(&impossible_pending_request)
            .expect("flat fixture serializes to JSON");
        impossible_pending_value["recovery_indexes"]["pending_requests"] =
            serde_json::json!(["00000000-0000-0000-0000-000000000556"]);
        let mut impossible_pending: Stage5dPersistenceEnvelope =
            serde_json::from_value(impossible_pending_value)
                .expect("pending-request shape remains representable");
        impossible_pending.payload_checksum_sha256 = impossible_pending
            .compute_payload_checksum_sha256()
            .expect("checksum recomputation must succeed");
        assert_eq!(
            impossible_pending
                .validate_restore_contract_schema_only()
                .map(|_| ()),
            Err(Stage5dEnvelopeValidationError::RecoveryIndexInconsistent),
            "r6 strict malformed pending-request/state relationship must fail at schema boundary"
        );

        let impossible_position_side = fixture_with_mutated_state(|state| {
            state["HybridIntradayRuntime"]["last_position_qty"] = serde_json::json!(1.0);
            state["HybridIntradayRuntime"]["current_side"] = Value::Null;
        });
        assert_eq!(
            impossible_position_side
                .validate_restore_contract_schema_only()
                .map(|_| ()),
            Err(Stage5dEnvelopeValidationError::PendingStateInconsistent),
            "r6 strict malformed non-flat position/current_side payload must fail before restored transition"
        );
    }

    #[test]
    fn stage5d_b2bd1r2_restored_success_can_continue_to_explicit_stage5c_warmup() {
        let injected = stage5d_test_complete_injected_fixture();
        let restored_at = injected.envelope.persisted_at_ts_utc;
        stage5d_test_reset_restored_callback_count();
        let restored = expect_stage5d_restore_ok(
            stage5d_notify_runtime_state_restored_at(injected, restored_at),
            "complete injected capability must restore before warmup",
        );
        assert_eq!(stage5d_test_restored_callback_count(), 1);
        stage5d_test_assert_restored_success_baseline(&restored);

        let history_close_ts = restored_at
            .checked_sub_signed(chrono::Duration::minutes(10))
            .expect("history before restored")
            .timestamp()
            .div_euclid(600)
            * 600;
        let history = crate::stage5c_paper_host::accept_stage5c_history_batch(
            crate::stage5c_paper_host::Stage5cHistoryBatchInput {
                bars: vec![broker_core::HybridRuntimeBarEvent {
                    instrument: broker_core::InstrumentId {
                        symbol: "IMOEXF".to_string(),
                        venue_symbol: Some("IMOEXF@RTSX".to_string()),
                        exchange: broker_core::Exchange::Moex,
                        market: broker_core::Market::Futures,
                    },
                    close_time_utc: history_close_ts,
                    open: 2200.0,
                    high: 2202.0,
                    low: 2199.0,
                    close: 2201.0,
                    volume: 100.0,
                    origin: broker_core::HybridRuntimeBarOrigin::History,
                    is_final: true,
                    timeframe_sec: 600,
                }],
                provenance:
                    broker_core::Stage3StrategyBarProvenance::finam_derived_m1_to_m10_complete(),
            },
        )
        .expect("accepted Stage 5C history");
        let warmed = crate::stage5c_paper_host::stage5d_test_warmup_stage5c_history_at(
            restored,
            history,
            restored_at,
        )
        .expect("explicit Stage 5C warmup after restored transition");
        assert!(warmed.receipt().warmup_started());
        assert_eq!(warmed.receipt().processed_bars(), 1);
        assert!(!warmed.receipt().pending_recovery_started());
        assert!(!warmed.receipt().semantic_bar_enabled());
        assert!(!warmed.receipt().intent_sink_attached());
    }

    #[test]
    fn stage5d_b2bd_exact_expiry_boundary_is_accepted() {
        let injected = stage5d_test_complete_injected_fixture();
        let expires_at = injected.bootstrapped.stage5d_admission().expires_at();
        stage5d_test_reset_restored_callback_count();
        let restored = expect_stage5d_restore_ok(
            stage5d_notify_runtime_state_restored_at(injected, expires_at),
            "exact expiry boundary follows Stage 5C equality policy",
        );
        assert_eq!(stage5d_test_restored_callback_count(), 1);
        assert_eq!(restored.receipt().restored_ts(), expires_at);
    }

    #[test]
    fn stage5d_b2bd1_exact_bootstrap_notification_boundary_is_accepted() {
        let injected = stage5d_test_complete_injected_fixture_with_position_and_bootstrap_delay(
            0.0,
            None,
            chrono::Duration::milliseconds(1),
        );
        let notified_at = injected.bootstrapped.stage5d_bootstrap_notified_ts();
        stage5d_test_reset_restored_callback_count();
        let restored = expect_stage5d_restore_ok(
            stage5d_notify_runtime_state_restored_at(injected, notified_at),
            "exact bootstrap notification boundary must be accepted",
        );
        assert_eq!(stage5d_test_restored_callback_count(), 1);
        assert_eq!(restored.receipt().restored_ts(), notified_at);
    }

    #[test]
    fn stage5d_b2bd1_restored_before_bootstrap_notification_blocks_before_callback() {
        let injected = stage5d_test_complete_injected_fixture_with_position_and_bootstrap_delay(
            0.0,
            None,
            chrono::Duration::milliseconds(2),
        );
        let before_notification = injected.bootstrapped.stage5d_bootstrap_notified_ts()
            - chrono::Duration::milliseconds(1);
        stage5d_test_assert_restore_blocks_before_callback(
            injected,
            before_notification,
            Stage5dRuntimeStateRestoreBlockedReason::LifecycleTimestampReversal,
            "restored timestamp before bootstrap notification must block before callback",
        );
    }

    fn stage5d_test_mutate_injected_current_side(
        injected: &mut Stage5dRiskGateInjectedPaperStrategy,
        side: Option<crate::hybrid_intraday::Side>,
    ) {
        let mut state = Strategy::state(injected.bootstrapped.stage5d_strategy()).clone();
        let StrategyState::HybridIntradayRuntime { current_side, .. } = &mut state else {
            panic!("test fixture must be hybrid runtime state");
        };
        *current_side = side;
        injected.bootstrapped.stage5d_test_set_strategy_state(state);
    }

    fn stage5d_test_mutate_injected_position_qty(
        injected: &mut Stage5dRiskGateInjectedPaperStrategy,
        qty: f64,
    ) {
        let mut state = Strategy::state(injected.bootstrapped.stage5d_strategy()).clone();
        let StrategyState::HybridIntradayRuntime {
            last_position_qty, ..
        } = &mut state
        else {
            panic!("test fixture must be hybrid runtime state");
        };
        *last_position_qty = qty;
        injected.bootstrapped.stage5d_test_set_strategy_state(state);
    }

    fn stage5d_test_mutate_state_tp_order_id(state: &mut StrategyState) {
        let StrategyState::HybridIntradayRuntime { tp_order_id, .. } = state else {
            panic!("test fixture must be hybrid runtime state");
        };
        *tp_order_id = Some(BrokerOrderId::new("stage5d-post-callback-tp"));
    }

    fn stage5d_test_mutate_state_sl_stop_order_id(state: &mut StrategyState) {
        let StrategyState::HybridIntradayRuntime {
            sl_stop_order_id, ..
        } = state
        else {
            panic!("test fixture must be hybrid runtime state");
        };
        *sl_stop_order_id = Some(BrokerStopOrderId::new("stage5d-post-callback-stop"));
    }

    fn stage5d_test_mutate_state_sl_exchange_order_id(state: &mut StrategyState) {
        let StrategyState::HybridIntradayRuntime {
            sl_exchange_order_id,
            ..
        } = state
        else {
            panic!("test fixture must be hybrid runtime state");
        };
        *sl_exchange_order_id = Some(BrokerOrderId::new("stage5d-post-callback-exchange"));
    }

    fn stage5d_test_terminal_state_override(
        mut state_mutator: impl FnMut(&mut StrategyState),
        expected: Stage5dRuntimeStateRestoreTerminalReason,
    ) {
        let injected = stage5d_test_complete_injected_fixture();
        let restored_at = injected.envelope.persisted_at_ts_utc;
        let mut state = Strategy::state(injected.bootstrapped.stage5d_strategy()).clone();
        state_mutator(&mut state);
        stage5d_test_reset_restored_callback_count();
        let terminal = expect_stage5d_restore_terminal(
            stage5d_test_notify_runtime_state_restored_with_state_override_at(
                injected,
                restored_at,
                state,
            ),
            "post-callback override must map to terminal outcome",
        );
        assert_eq!(stage5d_test_restored_callback_count(), 1);
        assert_eq!(terminal.reason(), expected);
        assert!(!terminal.retry_capability_available());
        assert!(!terminal.snapshot_id().is_empty());
        assert!(!terminal.evidence_fingerprint().is_empty());
        assert!(!terminal.recovery_plan_fingerprint().is_empty());
    }

    #[test]
    fn stage5d_b2bd1_flat_broker_side_is_exact_and_blocks_stale_side_before_callback() {
        struct Case {
            name: &'static str,
            side: Option<crate::hybrid_intraday::Side>,
            accepted: bool,
        }

        for case in [
            Case {
                name: "flat_none",
                side: None,
                accepted: true,
            },
            Case {
                name: "flat_long",
                side: Some(crate::hybrid_intraday::Side::Long),
                accepted: false,
            },
            Case {
                name: "flat_short",
                side: Some(crate::hybrid_intraday::Side::Short),
                accepted: false,
            },
        ] {
            let mut injected = stage5d_test_complete_injected_fixture();
            stage5d_test_mutate_injected_current_side(&mut injected, case.side);
            let restored_at = injected.envelope.persisted_at_ts_utc;
            if case.accepted {
                stage5d_test_reset_restored_callback_count();
                let result = stage5d_notify_runtime_state_restored_at(injected, restored_at);
                let restored = expect_stage5d_restore_ok(result, case.name);
                assert_eq!(stage5d_test_restored_callback_count(), 1);
                assert_eq!(restored.receipt().restored_ts(), restored_at);
            } else {
                stage5d_test_assert_restore_blocks_before_callback(
                    injected,
                    restored_at,
                    Stage5dRuntimeStateRestoreBlockedReason::BrokerTruthSideMismatch,
                    case.name,
                );
            }
        }
    }

    #[test]
    fn stage5d_b2bd1r4_open_broker_position_side_mismatch_blocks_before_callback() {
        for (qty, correct_side, wrong_side) in [
            (3.0, "long", Some(crate::hybrid_intraday::Side::Short)),
            (-3.0, "short", Some(crate::hybrid_intraday::Side::Long)),
        ] {
            let mut injected =
                stage5d_test_complete_injected_fixture_with_position_and_bootstrap_delay(
                    qty,
                    Some(correct_side),
                    chrono::Duration::zero(),
                );
            stage5d_test_mutate_injected_current_side(&mut injected, wrong_side);
            let restored_at = injected.envelope.persisted_at_ts_utc;
            stage5d_test_assert_restore_blocks_before_callback(
                injected,
                restored_at,
                Stage5dRuntimeStateRestoreBlockedReason::BrokerTruthSideMismatch,
                "r4 open broker position Long/Short side mismatch must block before callback",
            );
        }
    }

    #[test]
    fn stage5d_b2bd1r3_restored_before_persisted_envelope_blocks_before_callback() {
        let injected = stage5d_test_complete_injected_fixture();
        let before_persisted = injected.envelope.persisted_at_ts_utc - chrono::Duration::seconds(1);
        stage5d_test_assert_restore_blocks_before_callback(
            injected,
            before_persisted,
            Stage5dRuntimeStateRestoreBlockedReason::LifecycleTimestampReversal,
            "restored timestamp before persisted envelope must block before callback",
        );
    }

    #[test]
    fn stage5d_b2bd_expired_admission_blocks_before_callback_and_preserves_input() {
        let injected = stage5d_test_complete_injected_fixture();
        let expired_at = injected.bootstrapped.stage5d_admission().expires_at()
            + chrono::Duration::milliseconds(1);
        let blocked = stage5d_test_assert_restore_blocks_before_callback(
            injected,
            expired_at,
            Stage5dRuntimeStateRestoreBlockedReason::AdmissionExpired,
            "expired admission must block before callback",
        );
        assert_eq!(
            blocked.recovery_disposition(),
            Stage5dRuntimeStateRestoreRecoveryDisposition::RestartWithFreshBrokerTruth
        );
    }

    #[test]
    fn stage5d_b2bd_incomplete_recovery_blocks_before_callback() {
        let (bootstrapped, _envelope, validated_evidence) =
            riskgate_enabled_bootstrapped_fixture_with_evidence(|envelope, evidence| {
                stage5d_test_configure_single_tail_crash_frontier(
                    envelope,
                    evidence,
                    Stage5dRiskGateFinalizationState::Prepared,
                    false,
                    false,
                    false,
                    true,
                );
            });
        let injected = expect_stage5d_riskgate_ok(
            stage5d_inject_authoritative_riskgate(bootstrapped, validated_evidence),
            "incomplete fixture still injects but is not callback-ready",
        );
        assert!(!injected.recovery_complete());
        let restored_at = injected.envelope.persisted_at_ts_utc;
        stage5d_test_assert_restore_blocks_before_callback(
            injected,
            restored_at,
            Stage5dRuntimeStateRestoreBlockedReason::RecoveryIncomplete,
            "incomplete recovery must not reach callback",
        );
    }

    #[test]
    fn stage5d_b2bd1r2_pre_callback_matrix_blocks_without_callback() {
        struct Case {
            name: &'static str,
            mutate: fn(&mut Stage5dRiskGateInjectedPaperStrategy),
            expected: Stage5dRuntimeStateRestoreBlockedReason,
        }

        fn pending_finalization(injected: &mut Stage5dRiskGateInjectedPaperStrategy) {
            let pending = Stage5dRuntimePendingRiskGateFinalization {
                session_date: "2026-01-06".to_string(),
                shadow_pnl_points: "0.0".to_string(),
                shadow_trade_count: 0,
            };
            injected
                .envelope
                .runtime_private_extension
                .runtime_pending_finalizations
                .push(pending);
        }

        fn recovery_plan_fingerprint(injected: &mut Stage5dRiskGateInjectedPaperStrategy) {
            injected.recovery_plan.plan_fingerprint_sha256 = "stage5d-bad-plan".to_string();
        }

        fn known_index(injected: &mut Stage5dRiskGateInjectedPaperStrategy) {
            injected
                .envelope
                .recovery_indexes
                .known_order_ids
                .push(BrokerOrderId::new("stage5d-extra-known-order"));
        }

        fn pending_index(injected: &mut Stage5dRiskGateInjectedPaperStrategy) {
            injected
                .envelope
                .recovery_indexes
                .pending_requests
                .push(StrategyRequestId::new(
                    uuid::Uuid::parse_str("00000000-0000-0000-0000-000000000551").expect("uuid"),
                ));
        }

        fn broker_position(injected: &mut Stage5dRiskGateInjectedPaperStrategy) {
            stage5d_test_mutate_injected_position_qty(injected, 1.0);
        }

        fn broker_tp(injected: &mut Stage5dRiskGateInjectedPaperStrategy) {
            let mut state = Strategy::state(injected.bootstrapped.stage5d_strategy()).clone();
            stage5d_test_mutate_state_tp_order_id(&mut state);
            injected.bootstrapped.stage5d_test_set_strategy_state(state);
        }

        fn broker_sl_stop(injected: &mut Stage5dRiskGateInjectedPaperStrategy) {
            let mut state = Strategy::state(injected.bootstrapped.stage5d_strategy()).clone();
            stage5d_test_mutate_state_sl_stop_order_id(&mut state);
            injected.bootstrapped.stage5d_test_set_strategy_state(state);
        }

        fn broker_sl_exchange(injected: &mut Stage5dRiskGateInjectedPaperStrategy) {
            let mut state = Strategy::state(injected.bootstrapped.stage5d_strategy()).clone();
            stage5d_test_mutate_state_sl_exchange_order_id(&mut state);
            injected.bootstrapped.stage5d_test_set_strategy_state(state);
        }

        fn closed_boundary(injected: &mut Stage5dRiskGateInjectedPaperStrategy) {
            injected
                .bootstrapped
                .stage5d_test_mark_runtime_host_attached();
        }

        fn intent_sink_boundary(injected: &mut Stage5dRiskGateInjectedPaperStrategy) {
            injected
                .bootstrapped
                .stage5d_test_mark_intent_sink_attached();
        }

        fn not_paper_only_boundary(injected: &mut Stage5dRiskGateInjectedPaperStrategy) {
            injected.bootstrapped.stage5d_test_mark_not_paper_only();
        }

        fn non_acknowledged_recovery_decision(injected: &mut Stage5dRiskGateInjectedPaperStrategy) {
            let decision = injected
                .recovery_plan
                .decisions
                .first_mut()
                .expect("complete fixture carries one acknowledged decision");
            decision.action = Stage5dRiskGateRecoveryDecision::ReackRuntime;
        }

        for case in [
            Case {
                name: "pending_finalization",
                mutate: pending_finalization,
                expected: Stage5dRuntimeStateRestoreBlockedReason::PendingRiskGateFinalizations,
            },
            Case {
                name: "recovery_plan_fingerprint",
                mutate: recovery_plan_fingerprint,
                expected: Stage5dRuntimeStateRestoreBlockedReason::RecoveryPlanBindingMismatch,
            },
            Case {
                name: "known_index",
                mutate: known_index,
                expected: Stage5dRuntimeStateRestoreBlockedReason::RecoveryIndexMismatch,
            },
            Case {
                name: "pending_index",
                mutate: pending_index,
                expected: Stage5dRuntimeStateRestoreBlockedReason::RecoveryIndexMismatch,
            },
            Case {
                name: "broker_position",
                mutate: broker_position,
                expected: Stage5dRuntimeStateRestoreBlockedReason::BrokerTruthPositionMismatch,
            },
            Case {
                name: "broker_tp",
                mutate: broker_tp,
                expected: Stage5dRuntimeStateRestoreBlockedReason::BrokerOwnedProtectiveId,
            },
            Case {
                name: "broker_sl_stop",
                mutate: broker_sl_stop,
                expected: Stage5dRuntimeStateRestoreBlockedReason::BrokerOwnedProtectiveId,
            },
            Case {
                name: "broker_sl_exchange",
                mutate: broker_sl_exchange,
                expected: Stage5dRuntimeStateRestoreBlockedReason::BrokerOwnedProtectiveId,
            },
            Case {
                name: "closed_boundary",
                mutate: closed_boundary,
                expected: Stage5dRuntimeStateRestoreBlockedReason::ClosedBoundaryOpened,
            },
            Case {
                name: "intent_sink_boundary",
                mutate: intent_sink_boundary,
                expected: Stage5dRuntimeStateRestoreBlockedReason::ClosedBoundaryOpened,
            },
            Case {
                name: "not_paper_only_boundary",
                mutate: not_paper_only_boundary,
                expected: Stage5dRuntimeStateRestoreBlockedReason::ClosedBoundaryOpened,
            },
            Case {
                name: "non_acknowledged_recovery_decision",
                mutate: non_acknowledged_recovery_decision,
                expected: Stage5dRuntimeStateRestoreBlockedReason::RecoveryIncomplete,
            },
        ] {
            let mut injected = stage5d_test_complete_injected_fixture();
            (case.mutate)(&mut injected);
            let restored_at = injected.envelope.persisted_at_ts_utc;
            stage5d_test_assert_restore_blocks_before_callback(
                injected,
                restored_at,
                case.expected,
                case.name,
            );
        }
    }

    #[test]
    fn stage5d_b2bd_release_mode_nonempty_callback_intent_is_terminal() {
        let injected = stage5d_test_complete_injected_fixture();
        let restored_at = injected.envelope.persisted_at_ts_utc;
        stage5d_test_reset_restored_callback_count();
        let terminal = expect_stage5d_restore_terminal(
            stage5d_test_notify_runtime_state_restored_forcing_intent_at(injected, restored_at),
            "non-empty callback intents must be terminal in release mode",
        );

        assert_eq!(stage5d_test_restored_callback_count(), 1);
        assert_eq!(
            terminal.reason(),
            Stage5dRuntimeStateRestoreTerminalReason::CallbackEmittedIntent
        );
        assert!(!terminal.retry_capability_available());
        assert!(!terminal.snapshot_id().is_empty());
        assert!(!terminal.evidence_fingerprint().is_empty());
        assert!(!terminal.recovery_plan_fingerprint().is_empty());
    }

    #[test]
    fn stage5d_b2bd1r2_post_callback_terminal_matrix_uses_common_guard() {
        stage5d_test_terminal_state_override(
            |state| {
                let StrategyState::HybridIntradayRuntime {
                    last_position_qty, ..
                } = state
                else {
                    panic!("test fixture must be hybrid runtime state");
                };
                *last_position_qty = 1.0;
            },
            Stage5dRuntimeStateRestoreTerminalReason::PostCallbackBrokerTruthPositionMismatch,
        );
        stage5d_test_terminal_state_override(
            |state| {
                let StrategyState::HybridIntradayRuntime { current_side, .. } = state else {
                    panic!("test fixture must be hybrid runtime state");
                };
                *current_side = Some(crate::hybrid_intraday::Side::Long);
            },
            Stage5dRuntimeStateRestoreTerminalReason::PostCallbackBrokerTruthSideMismatch,
        );
        stage5d_test_terminal_state_override(
            stage5d_test_mutate_state_tp_order_id,
            Stage5dRuntimeStateRestoreTerminalReason::PostCallbackBrokerOwnedProtectiveId,
        );
        stage5d_test_terminal_state_override(
            stage5d_test_mutate_state_sl_stop_order_id,
            Stage5dRuntimeStateRestoreTerminalReason::PostCallbackBrokerOwnedProtectiveId,
        );
        stage5d_test_terminal_state_override(
            stage5d_test_mutate_state_sl_exchange_order_id,
            Stage5dRuntimeStateRestoreTerminalReason::PostCallbackBrokerOwnedProtectiveId,
        );
        stage5d_test_terminal_state_override(
            |state| *state = StrategyState::Idle,
            Stage5dRuntimeStateRestoreTerminalReason::PostCallbackStateInvalid,
        );
    }

    #[test]
    fn stage5d_b2bc1r6_current_shadow_chronology_uses_bound_timezone_config() {
        let false_accept = riskgate_enabled_bootstrapped_fixture_with_config_and_evidence(
            |config| config.timezone_offset_hours = 2,
            |envelope, evidence| {
                evidence.current_shadow_session_date = Some("2026-07-14".to_string());
                evidence.current_shadow_pnl_points = "0.5".to_string();
                stage5d_apply_riskgate_evidence_to_envelope(envelope, evidence);
                if let Value::Object(fields) =
                    &mut envelope.strategy_state.strategy_state_json["HybridIntradayRuntime"]
                {
                    fields.insert(
                        "last_day_local".to_string(),
                        Value::String("2026-07-14".to_string()),
                    );
                    fields.insert(
                        "risk_gate_shadow_trade_count".to_string(),
                        serde_json::json!(1),
                    );
                    fields.insert(
                        "risk_gate_shadow_entry_ts_utc".to_string(),
                        serde_json::json!(1_783_978_200_i64),
                    );
                    fields.insert(
                        "risk_gate_shadow_entry_price".to_string(),
                        serde_json::json!(2227.0),
                    );
                    fields.insert(
                        "risk_gate_shadow_side".to_string(),
                        serde_json::json!("long"),
                    );
                    fields.insert(
                        "risk_gate_shadow_target_price".to_string(),
                        serde_json::json!(2230.0),
                    );
                    fields.insert(
                        "risk_gate_shadow_stop_price".to_string(),
                        serde_json::json!(2220.0),
                    );
                }
            },
        );
        let blocked = expect_stage5d_riskgate_blocked(
            stage5d_inject_authoritative_riskgate(false_accept.0, false_accept.2),
            "UTC+2 source config must reject a hard-coded-UTC+3 false accept",
        );
        assert_eq!(
            blocked.reason(),
            Stage5dRiskGateInjectionBlockReason::MaterializedStateMismatch
        );

        let false_reject_control = riskgate_enabled_bootstrapped_fixture_with_config_and_evidence(
            |config| config.timezone_offset_hours = 4,
            |envelope, evidence| {
                evidence.current_shadow_session_date = Some("2026-07-14".to_string());
                evidence.current_shadow_pnl_points = "0.5".to_string();
                stage5d_apply_riskgate_evidence_to_envelope(envelope, evidence);
                if let Value::Object(fields) =
                    &mut envelope.strategy_state.strategy_state_json["HybridIntradayRuntime"]
                {
                    fields.insert(
                        "last_day_local".to_string(),
                        Value::String("2026-07-14".to_string()),
                    );
                    fields.insert(
                        "risk_gate_shadow_trade_count".to_string(),
                        serde_json::json!(1),
                    );
                    fields.insert(
                        "risk_gate_shadow_entry_ts_utc".to_string(),
                        serde_json::json!(1_783_974_600_i64),
                    );
                    fields.insert(
                        "risk_gate_shadow_entry_price".to_string(),
                        serde_json::json!(2227.0),
                    );
                    fields.insert(
                        "risk_gate_shadow_side".to_string(),
                        serde_json::json!("short"),
                    );
                    fields.insert(
                        "risk_gate_shadow_target_price".to_string(),
                        serde_json::json!(2220.0),
                    );
                    fields.insert(
                        "risk_gate_shadow_stop_price".to_string(),
                        serde_json::json!(2230.0),
                    );
                }
            },
        );
        expect_stage5d_riskgate_ok(
            stage5d_inject_authoritative_riskgate(false_reject_control.0, false_reject_control.2),
            "UTC+4 source config must accept its own local-date boundary",
        );
    }

    #[test]
    fn stage5d_b2bc1r4_rejects_negative_zero_at_every_riskgate_authority_boundary() {
        let ledger_cases: [fn(&mut Stage5dRiskGateLedgerEvidence); 3] = [
            |evidence| evidence.ledger_records[0].shadow_pnl_points = "-0.0".to_string(),
            |evidence| evidence.ledger_records[0].rolling_sum_before_session = "-0.0".to_string(),
            |evidence| evidence.ledger_records[0].rolling_sum_lb120 = "-0.0".to_string(),
        ];
        for mutate in ledger_cases {
            let (_bootstrapped, _envelope, evidence) =
                riskgate_enabled_bootstrapped_fixture_with(|_| {});
            let mut raw = evidence.evidence.clone();
            mutate(&mut raw);
            raw.ledger_tail_hash = stage5d_compute_riskgate_ledger_tail_hash(&raw).unwrap();
            assert_eq!(
                stage5d_validate_riskgate_ledger_evidence(raw).map(|_| ()),
                Err(Stage5dRiskGateInjectionBlockReason::LedgerEvidenceInvalid)
            );
        }

        let (_bootstrapped, _envelope, evidence) =
            riskgate_enabled_bootstrapped_fixture_with(|_| {});
        let mut raw = evidence.evidence.clone();
        raw.current_shadow_pnl_points = "-0.0".to_string();
        assert_eq!(
            stage5d_validate_riskgate_ledger_evidence(raw).map(|_| ()),
            Err(Stage5dRiskGateInjectionBlockReason::LedgerEvidenceInvalid)
        );

        let materialized_cases: [fn(&mut Stage5dPersistenceEnvelope); 2] = [
            |envelope| {
                envelope
                    .riskgate
                    .materialized_state
                    .current_shadow_pnl_points = "-0.0".to_string()
            },
            |envelope| {
                envelope.riskgate.materialized_state.rolling_sum_lb120 = Some("-0.0".to_string())
            },
        ];
        for mutate in materialized_cases {
            let (bootstrapped, _envelope, evidence) =
                riskgate_enabled_bootstrapped_fixture_with(|envelope| {
                    mutate(envelope);
                });
            let blocked = expect_stage5d_riskgate_blocked(
                stage5d_inject_authoritative_riskgate(bootstrapped, evidence),
                "materialized riskgate negative zero must fail closed",
            );
            assert_eq!(
                blocked.reason(),
                Stage5dRiskGateInjectionBlockReason::MaterializedStateInvalid
            );
        }

        let (mut bootstrapped, _envelope, evidence) =
            riskgate_enabled_bootstrapped_fixture_with(|_| {});
        bootstrapped
            .envelope
            .runtime_private_extension
            .runtime_pending_finalizations[0]
            .shadow_pnl_points = "-0.0".to_string();
        stage5d_test_sync_runtime_pending_cache(&mut bootstrapped.envelope);
        bootstrapped.envelope.payload_checksum_sha256 = bootstrapped
            .envelope
            .compute_payload_checksum_sha256()
            .expect("mutated checksum");
        let blocked = expect_stage5d_riskgate_blocked(
            stage5d_inject_authoritative_riskgate(bootstrapped, evidence),
            "runtime pending finalization negative zero must fail closed",
        );
        assert_eq!(
            blocked.reason(),
            Stage5dRiskGateInjectionBlockReason::LedgerEvidenceInvalid
        );
        let (_bootstrapped, envelope, evidence) =
            riskgate_enabled_bootstrapped_fixture_with(|_| {});
        let mut pending = envelope
            .runtime_private_extension
            .runtime_pending_finalizations[0]
            .clone();
        pending.shadow_pnl_points = "-0.0".to_string();
        let records =
            stage5d_source_riskgate_records_from_evidence(&evidence.evidence).expect("records");
        assert_eq!(
            stage5d_pending_matches_ledger(&pending, &records[0]),
            Err(Stage5dRiskGateInjectionBlockReason::OutboxStateInconsistent)
        );

        let semantic_cases: [fn(&mut Stage5dPersistenceEnvelope); 2] = [
            |envelope| {
                envelope.strategy_state.strategy_state_json["HybridIntradayRuntime"]
                    ["risk_gate_shadow_pnl_points"] = serde_json::json!(-0.0);
            },
            |envelope| {
                envelope.strategy_state.strategy_state_json["HybridIntradayRuntime"]
                    ["risk_gate_rolling_sum_lb120"] = serde_json::json!(-0.0);
            },
        ];
        for mutate in semantic_cases {
            let (mut bootstrapped, _envelope, evidence) =
                riskgate_enabled_bootstrapped_fixture_with(|_| {});
            mutate(&mut bootstrapped.envelope);
            bootstrapped.envelope.payload_checksum_sha256 = bootstrapped
                .envelope
                .compute_payload_checksum_sha256()
                .expect("mutated checksum");
            let blocked = expect_stage5d_riskgate_blocked(
                stage5d_inject_authoritative_riskgate(bootstrapped, evidence),
                "semantic riskgate negative zero must fail closed",
            );
            assert_eq!(
                blocked.reason(),
                Stage5dRiskGateInjectionBlockReason::MaterializedStateInvalid
            );
        }
    }

    #[test]
    fn stage5d_b2bc1r4_rejects_source_impossible_current_shadow_tuples() {
        struct Case {
            mutate: fn(&mut Stage5dPersistenceEnvelope, &mut Stage5dRiskGateLedgerEvidence),
            reason: Stage5dRiskGateInjectionBlockReason,
        }
        let cases = [
            Case {
                mutate: |envelope, evidence| {
                    evidence.current_shadow_session_date = None;
                    evidence.current_shadow_pnl_points = "0.5".to_string();
                    stage5d_apply_riskgate_evidence_to_envelope(envelope, evidence);
                },
                reason: Stage5dRiskGateInjectionBlockReason::MaterializedStateMismatch,
            },
            Case {
                mutate: |envelope, evidence| {
                    evidence.current_shadow_session_date = None;
                    evidence.current_shadow_pnl_points = "0.0".to_string();
                    stage5d_apply_riskgate_evidence_to_envelope(envelope, evidence);
                    if let Value::Object(fields) =
                        &mut envelope.strategy_state.strategy_state_json["HybridIntradayRuntime"]
                    {
                        fields.insert(
                            "risk_gate_shadow_trade_count".to_string(),
                            serde_json::json!(1),
                        );
                    }
                },
                reason: Stage5dRiskGateInjectionBlockReason::MaterializedStateMismatch,
            },
            Case {
                mutate: |envelope, evidence| {
                    evidence.current_shadow_session_date = None;
                    evidence.current_shadow_pnl_points = "0.0".to_string();
                    stage5d_apply_riskgate_evidence_to_envelope(envelope, evidence);
                    if let Value::Object(fields) =
                        &mut envelope.strategy_state.strategy_state_json["HybridIntradayRuntime"]
                    {
                        fields.insert(
                            "risk_gate_shadow_entry_ts_utc".to_string(),
                            serde_json::json!(1_783_843_200_i64),
                        );
                        fields.insert(
                            "risk_gate_shadow_entry_price".to_string(),
                            serde_json::json!(2227.0),
                        );
                        fields.insert(
                            "risk_gate_shadow_side".to_string(),
                            serde_json::json!("long"),
                        );
                        fields.insert(
                            "risk_gate_shadow_target_price".to_string(),
                            serde_json::json!(2230.0),
                        );
                        fields.insert(
                            "risk_gate_shadow_stop_price".to_string(),
                            serde_json::json!(2220.0),
                        );
                    }
                },
                reason: Stage5dRiskGateInjectionBlockReason::MaterializedStateMismatch,
            },
            Case {
                mutate: |envelope, evidence| {
                    evidence.current_shadow_session_date = Some("2026-07-11".to_string());
                    evidence.current_shadow_pnl_points = "0.0".to_string();
                    stage5d_apply_riskgate_evidence_to_envelope(envelope, evidence);
                },
                reason: Stage5dRiskGateInjectionBlockReason::MaterializedStateMismatch,
            },
            Case {
                mutate: |envelope, evidence| {
                    evidence.current_shadow_session_date = Some("2026-07-12".to_string());
                    evidence.current_shadow_pnl_points = "0.0".to_string();
                    stage5d_apply_riskgate_evidence_to_envelope(envelope, evidence);
                },
                reason: Stage5dRiskGateInjectionBlockReason::MaterializedStateMismatch,
            },
            Case {
                mutate: |envelope, evidence| {
                    let last = evidence
                        .ledger_records
                        .last()
                        .expect("last row")
                        .session_date
                        .clone();
                    evidence.current_shadow_session_date = Some(last);
                    evidence.current_shadow_pnl_points = "0.0".to_string();
                    stage5d_apply_riskgate_evidence_to_envelope(envelope, evidence);
                },
                reason: Stage5dRiskGateInjectionBlockReason::MaterializedStateMismatch,
            },
            Case {
                mutate: |envelope, evidence| {
                    let before_last = evidence.ledger_records[evidence.ledger_records.len() - 2]
                        .session_date
                        .clone();
                    evidence.current_shadow_session_date = Some(before_last);
                    evidence.current_shadow_pnl_points = "0.0".to_string();
                    stage5d_apply_riskgate_evidence_to_envelope(envelope, evidence);
                },
                reason: Stage5dRiskGateInjectionBlockReason::MaterializedStateMismatch,
            },
            Case {
                mutate: |envelope, evidence| {
                    evidence.current_shadow_session_date = envelope
                        .runtime_private_extension
                        .runtime_pending_finalizations[1]
                        .session_date
                        .clone()
                        .into();
                    evidence.current_shadow_pnl_points = "0.0".to_string();
                    stage5d_apply_riskgate_evidence_to_envelope(envelope, evidence);
                },
                reason: Stage5dRiskGateInjectionBlockReason::MaterializedStateMismatch,
            },
        ];

        for case in cases {
            let (mut bootstrapped, _envelope, evidence) =
                riskgate_enabled_bootstrapped_fixture_with(|_| {});
            let mut raw = evidence.evidence.clone();
            (case.mutate)(&mut bootstrapped.envelope, &mut raw);
            raw.ledger_tail_hash = stage5d_compute_riskgate_ledger_tail_hash(&raw).unwrap();
            bootstrapped.envelope.riskgate.ledger_tail_hash = raw.ledger_tail_hash.clone();
            bootstrapped.envelope.payload_checksum_sha256 = bootstrapped
                .envelope
                .compute_payload_checksum_sha256()
                .expect("mutated checksum");
            let evidence = stage5d_validate_riskgate_ledger_evidence(raw)
                .expect("impossible current-shadow evidence shape is otherwise valid");
            let blocked = expect_stage5d_riskgate_blocked(
                stage5d_inject_authoritative_riskgate(bootstrapped, evidence),
                "source-impossible current-shadow tuple must fail closed",
            );
            assert_eq!(blocked.reason(), case.reason);
        }
    }

    #[test]
    fn stage5d_b2bc1r4_accepts_source_valid_current_shadow_tuples() {
        let (bootstrapped, _envelope, evidence) =
            riskgate_enabled_bootstrapped_fixture_with_evidence(|envelope, evidence| {
                evidence.current_shadow_session_date = None;
                evidence.current_shadow_pnl_points = "0.0".to_string();
                envelope.runtime_private_extension.last_processed_bar_ts = None;
                envelope.lifecycle_watermarks.last_semantic_bar_ts = None;
                stage5d_apply_riskgate_evidence_to_envelope(envelope, evidence);
            });
        expect_stage5d_riskgate_ok(
            stage5d_inject_authoritative_riskgate(bootstrapped, evidence),
            "clean empty current-shadow tuple must remain valid",
        );

        let (bootstrapped, _envelope, evidence) =
            riskgate_enabled_bootstrapped_fixture_with_evidence(|envelope, evidence| {
                evidence.current_shadow_session_date = Some("2026-07-14".to_string());
                evidence.current_shadow_pnl_points = "0.5".to_string();
                envelope.runtime_private_extension.last_processed_bar_ts =
                    DateTime::<Utc>::from_timestamp(1_784_008_800, 0);
                envelope.lifecycle_watermarks.last_semantic_bar_ts =
                    DateTime::<Utc>::from_timestamp(1_784_008_800, 0);
                stage5d_apply_riskgate_evidence_to_envelope(envelope, evidence);
                if let Value::Object(fields) =
                    &mut envelope.strategy_state.strategy_state_json["HybridIntradayRuntime"]
                {
                    fields.insert(
                        "last_day_local".to_string(),
                        Value::String("2026-07-14".to_string()),
                    );
                    fields.insert(
                        "risk_gate_shadow_trade_count".to_string(),
                        serde_json::json!(1),
                    );
                    fields.insert(
                        "risk_gate_shadow_entry_ts_utc".to_string(),
                        serde_json::json!(1_784_008_800_i64),
                    );
                    fields.insert(
                        "risk_gate_shadow_entry_price".to_string(),
                        serde_json::json!(2227.0),
                    );
                    fields.insert(
                        "risk_gate_shadow_side".to_string(),
                        serde_json::json!("long"),
                    );
                    fields.insert(
                        "risk_gate_shadow_target_price".to_string(),
                        serde_json::json!(2230.0),
                    );
                    fields.insert(
                        "risk_gate_shadow_stop_price".to_string(),
                        serde_json::json!(2220.0),
                    );
                }
            });
        expect_stage5d_riskgate_ok(
            stage5d_inject_authoritative_riskgate(bootstrapped, evidence),
            "valid regular current-shadow tuple with semantic-owned open fields must pass",
        );
    }

    #[test]
    fn stage5d_b2bc1r4_multi_row_recovery_frontiers_are_stepwise_executable_to_completion() {
        stage5d_test_assert_ordered_tail_frontier_reaches_recovery_complete(vec![
            Stage5dTestTailFrontierRow {
                state: Stage5dRiskGateFinalizationState::AcknowledgedInRuntime,
                ledger: true,
                materialized: true,
                runtime: true,
                pending: false,
            },
            Stage5dTestTailFrontierRow {
                state: Stage5dRiskGateFinalizationState::Prepared,
                ledger: false,
                materialized: false,
                runtime: false,
                pending: true,
            },
        ]);

        stage5d_test_assert_ordered_tail_frontier_reaches_recovery_complete(vec![
            Stage5dTestTailFrontierRow {
                state: Stage5dRiskGateFinalizationState::MaterializedUpdated,
                ledger: true,
                materialized: true,
                runtime: false,
                pending: true,
            },
            Stage5dTestTailFrontierRow {
                state: Stage5dRiskGateFinalizationState::LedgerAppended,
                ledger: true,
                materialized: false,
                runtime: false,
                pending: true,
            },
        ]);
    }

    #[test]
    fn stage5d_b2bc_riskgate_injection_blocks_pending_finalization_without_outbox() {
        let (bootstrapped, _envelope, evidence) =
            riskgate_enabled_bootstrapped_fixture_with(|envelope| {
                envelope.riskgate.durable_finalization_outbox.clear();
            });

        let blocked = expect_stage5d_riskgate_blocked(
            stage5d_inject_authoritative_riskgate(bootstrapped, evidence),
            "runtime pending finalization missing from durable outbox must block injection",
        );
        assert_eq!(
            blocked.reason(),
            Stage5dRiskGateInjectionBlockReason::RuntimePendingFinalizationMissingFromOutbox
        );
        assert!(blocked.input_capability_preserved());
    }

    #[test]
    fn stage5d_b2bc_riskgate_injection_rejects_full_identity_mismatch() {
        let (bootstrapped, _envelope, evidence) =
            riskgate_enabled_bootstrapped_fixture_with(|_| {});
        let mut raw = evidence.evidence.clone();
        raw.identity.mr_variant = "author41_42".to_string();
        raw.ledger_tail_hash = stage5d_compute_riskgate_ledger_tail_hash(&raw).unwrap();
        let evidence = stage5d_validate_riskgate_ledger_evidence(raw).unwrap();

        let blocked = expect_stage5d_riskgate_blocked(
            stage5d_inject_authoritative_riskgate(bootstrapped, evidence),
            "wrong mr variant must block riskgate injection",
        );
        assert_eq!(
            blocked.reason(),
            Stage5dRiskGateInjectionBlockReason::LedgerIdentityMismatch
        );
    }

    #[test]
    fn stage5d_b2bc_riskgate_injection_rejects_timeframe_policy_and_model_mismatch() {
        for mutate in [
            |identity: &mut Stage5dRiskGateIdentity| identity.timeframe = "1m".to_string(),
            |identity: &mut Stage5dRiskGateIdentity| {
                identity.session_policy = "other_session".to_string()
            },
            |identity: &mut Stage5dRiskGateIdentity| {
                identity.model_version = "other_model".to_string()
            },
        ] {
            let (bootstrapped, _envelope, evidence) =
                riskgate_enabled_bootstrapped_fixture_with(|_| {});
            let mut raw = evidence.evidence.clone();
            mutate(&mut raw.identity);
            raw.ledger_tail_hash = stage5d_compute_riskgate_ledger_tail_hash(&raw).unwrap();
            let evidence = stage5d_validate_riskgate_ledger_evidence(raw).unwrap();

            let blocked = expect_stage5d_riskgate_blocked(
                stage5d_inject_authoritative_riskgate(bootstrapped, evidence),
                "full identity mismatch must block riskgate injection",
            );
            assert_eq!(
                blocked.reason(),
                Stage5dRiskGateInjectionBlockReason::LedgerIdentityMismatch
            );
        }
    }

    #[test]
    fn stage5d_b2bc_riskgate_injection_rejects_disabled_profile_noop() {
        let (applied, envelope) = applied_stage5d_fixture();
        let identity = Stage5dRiskGateIdentity {
            strategy_id: envelope.binding.strategy_id.clone(),
            profile_id: "baseline_runtime_hybrid".to_string(),
            mr_variant: "classic_prev_day_range".to_string(),
            timeframe: "10m".to_string(),
            session_policy: "moex_forts_main_evening".to_string(),
            model_version: STAGE5D_RUNTIME_SEMANTIC_COMPATIBILITY_ID.to_string(),
        };
        let evidence = stage5d_test_riskgate_evidence_for(&identity, &envelope);
        let evidence = stage5d_validate_riskgate_ledger_evidence(evidence).unwrap();
        let bootstrapped = expect_stage5d_bootstrap_ok(
            stage5d_notify_broker_truth_bootstrap_at(applied, envelope.persisted_at_ts_utc),
            "disabled fixture bootstrap must still complete",
        );

        let blocked = expect_stage5d_riskgate_blocked(
            stage5d_inject_authoritative_riskgate(bootstrapped, evidence),
            "disabled riskgate source callback no-op must not produce injected success",
        );
        assert_eq!(
            blocked.reason(),
            Stage5dRiskGateInjectionBlockReason::RiskGateNotApplicable
        );
    }

    #[test]
    fn stage5d_b2bc_riskgate_ledger_tail_hash_must_match_evidence() {
        let (_bootstrapped, _envelope, evidence) =
            riskgate_enabled_bootstrapped_fixture_with(|_| {});
        let mut raw = evidence.evidence.clone();
        raw.ledger_tail_hash = "stage5d_riskgate_ledger_tail_sha256:bad".to_string();

        assert_eq!(
            stage5d_validate_riskgate_ledger_evidence(raw).map(|_| ()),
            Err(Stage5dRiskGateInjectionBlockReason::LedgerTailMismatch)
        );
    }

    #[test]
    fn stage5d_b2bc1_riskgate_generation_is_source_exact_and_fingerprinted() {
        let (_bootstrapped, _envelope, evidence) =
            riskgate_enabled_bootstrapped_fixture_with(|_| {});
        let raw = evidence.evidence.clone();
        let original_tail = raw.ledger_tail_hash.clone();
        let original_fingerprint = evidence.evidence_fingerprint_sha256().to_string();

        for generation in ["", "runtime-ledger-v999"] {
            let mut changed = raw.clone();
            changed.current_generation = generation.to_string();
            assert_eq!(
                stage5d_compute_riskgate_ledger_tail_hash(&changed).unwrap(),
                original_tail,
                "record-tail hash intentionally excludes generation metadata"
            );
            assert_eq!(
                stage5d_validate_riskgate_ledger_evidence(changed).map(|_| ()),
                Err(Stage5dRiskGateInjectionBlockReason::LedgerGenerationMismatch)
            );
        }

        let mut changed_metadata = raw.clone();
        changed_metadata.current_shadow_pnl_points = "1.5".to_string();
        let changed = stage5d_validate_riskgate_ledger_evidence(changed_metadata).unwrap();
        assert_ne!(changed.evidence_fingerprint_sha256(), original_fingerprint);

        let mut contradicted_seed = raw;
        contradicted_seed.seed_loaded = !contradicted_seed.seed_loaded;
        assert_eq!(
            stage5d_validate_riskgate_ledger_evidence(contradicted_seed).map(|_| ()),
            Err(Stage5dRiskGateInjectionBlockReason::LedgerRecordDerivedStateMismatch)
        );
    }

    #[test]
    fn stage5d_b2bc1_riskgate_generation_must_match_envelope() {
        let (bootstrapped, _envelope, evidence) =
            riskgate_enabled_bootstrapped_fixture_with(|envelope| {
                envelope.riskgate.materialized_state.current_generation =
                    "runtime-ledger-v999".to_string();
            });

        let blocked = expect_stage5d_riskgate_blocked(
            stage5d_inject_authoritative_riskgate(bootstrapped, evidence),
            "evidence and envelope generations must match the source constant",
        );
        assert_eq!(
            blocked.reason(),
            Stage5dRiskGateInjectionBlockReason::LedgerGenerationMismatch
        );
    }

    #[test]
    fn stage5d_b2bc1_riskgate_rejects_each_mutated_derived_row_field() {
        type Mutation = fn(&mut Stage5dRiskGateLedgerRecord);
        let mutations: [Mutation; 4] = [
            |record| record.rolling_sum_before_session = "999.0".to_string(),
            |record| record.mr_enabled_for_session = !record.mr_enabled_for_session,
            |record| record.rolling_sum_lb120 = "999.0".to_string(),
            |record| record.mr_enabled_next_session = !record.mr_enabled_next_session,
        ];

        for mutate in mutations {
            let (_bootstrapped, _envelope, evidence) =
                riskgate_enabled_bootstrapped_fixture_with(|_| {});
            let mut raw = evidence.evidence.clone();
            mutate(&mut raw.ledger_records[0]);
            let raw = stage5d_test_rehash_riskgate_evidence(raw);
            assert_eq!(
                stage5d_validate_riskgate_ledger_evidence(raw).map(|_| ()),
                Err(Stage5dRiskGateInjectionBlockReason::LedgerRecordDerivedStateMismatch)
            );
        }
    }

    #[test]
    fn stage5d_b2bc1_riskgate_accepts_source_exact_seed_and_runtime_rows() {
        let (_bootstrapped, _envelope, evidence) =
            riskgate_enabled_bootstrapped_fixture_with(|_| {});
        let mut raw = evidence.evidence.clone();
        for record in raw.ledger_records.iter_mut().take(5) {
            record.source = Stage5dRiskGateRowSource::Seed;
        }
        let raw = stage5d_test_rehash_riskgate_evidence(raw);
        let validated = stage5d_validate_riskgate_ledger_evidence(raw)
            .expect("source-exact seed prefix followed by runtime rows must validate");
        assert_eq!(validated.ledger_records_count(), 65);
    }

    #[test]
    fn stage5d_b2bc1_riskgate_rejects_invalid_source_status_and_ordering() {
        let (_bootstrapped, _envelope, evidence) =
            riskgate_enabled_bootstrapped_fixture_with(|_| {});
        let mut incomplete = evidence.evidence.clone();
        incomplete.ledger_records[0].status = Stage5dRiskGateRowStatus::Incomplete;
        assert_eq!(
            stage5d_validate_riskgate_ledger_evidence(stage5d_test_rehash_riskgate_evidence(
                incomplete
            ))
            .map(|_| ()),
            Err(Stage5dRiskGateInjectionBlockReason::LedgerRecordDerivedStateMismatch)
        );

        let mut seed_after_runtime = evidence.evidence.clone();
        seed_after_runtime
            .ledger_records
            .last_mut()
            .expect("runtime suffix")
            .source = Stage5dRiskGateRowSource::Seed;
        assert_eq!(
            stage5d_validate_riskgate_ledger_evidence(stage5d_test_rehash_riskgate_evidence(
                seed_after_runtime
            ))
            .map(|_| ()),
            Err(Stage5dRiskGateInjectionBlockReason::LedgerRecordDerivedStateMismatch)
        );
    }

    #[test]
    fn stage5d_b2bc1_riskgate_rejects_invalid_and_decreasing_finalization_time() {
        let (_bootstrapped, _envelope, evidence) =
            riskgate_enabled_bootstrapped_fixture_with(|_| {});
        let mut invalid = evidence.evidence.clone();
        invalid.ledger_records[0].finalized_at_utc = i64::MAX;
        assert_eq!(
            stage5d_validate_riskgate_ledger_evidence(stage5d_test_rehash_riskgate_evidence(
                invalid
            ))
            .map(|_| ()),
            Err(Stage5dRiskGateInjectionBlockReason::LedgerFinalizationTimestampInvalid)
        );

        let mut decreasing = evidence.evidence.clone();
        decreasing.ledger_records[1].finalized_at_utc -= 1;
        assert_eq!(
            stage5d_validate_riskgate_ledger_evidence(stage5d_test_rehash_riskgate_evidence(
                decreasing
            ))
            .map(|_| ()),
            Err(Stage5dRiskGateInjectionBlockReason::LedgerFinalizationTimestampInvalid)
        );
    }

    #[test]
    fn stage5d_b2bc1_riskgate_rejects_finalization_after_envelope_persisted_at() {
        let (_bootstrapped, envelope, evidence) =
            riskgate_enabled_bootstrapped_fixture_with(|_| {});
        let mut raw = evidence.evidence.clone();
        raw.ledger_records
            .last_mut()
            .expect("ledger row")
            .finalized_at_utc = envelope.persisted_at_ts_utc.timestamp() + 1;
        assert_eq!(
            stage5d_validate_source_exact_riskgate_records(
                &stage5d_test_rehash_riskgate_evidence(raw),
                Some(envelope.persisted_at_ts_utc),
            )
            .map(|_| ()),
            Err(Stage5dRiskGateInjectionBlockReason::LedgerFinalizationTimestampInvalid),
            "injection-time envelope chronology must reject the future finalization"
        );
    }

    #[test]
    fn stage5d_b2bc_riskgate_injection_rebuilds_materialized_from_ledger() {
        let (bootstrapped, _envelope, evidence) =
            riskgate_enabled_bootstrapped_fixture_with(|envelope| {
                envelope.riskgate.materialized_state.ledger_rows_count += 1;
            });

        let blocked = expect_stage5d_riskgate_blocked(
            stage5d_inject_authoritative_riskgate(bootstrapped, evidence),
            "persisted materialized state must match rebuilt ledger projection",
        );
        assert_eq!(
            blocked.reason(),
            Stage5dRiskGateInjectionBlockReason::MaterializedStateMismatch
        );
    }

    #[test]
    fn stage5d_b2bc_riskgate_outbox_rejects_acknowledged_runtime_pending() {
        let (bootstrapped, _envelope, evidence) =
            riskgate_enabled_bootstrapped_fixture_with(|envelope| {
                envelope.riskgate.durable_finalization_outbox[0].state =
                    Stage5dRiskGateFinalizationState::AcknowledgedInRuntime;
            });

        let blocked = expect_stage5d_riskgate_blocked(
            stage5d_inject_authoritative_riskgate(bootstrapped, evidence),
            "acknowledged outbox entry cannot still have runtime pending finalization",
        );
        assert_eq!(
            blocked.reason(),
            Stage5dRiskGateInjectionBlockReason::OutboxStateInconsistent
        );
    }

    #[test]
    fn stage5d_b2bc1_riskgate_outbox_rejects_acknowledged_without_ledger_row() {
        let (bootstrapped, _envelope, evidence) =
            riskgate_enabled_bootstrapped_fixture_with(|envelope| {
                envelope
                    .runtime_private_extension
                    .runtime_pending_finalizations
                    .clear();
                stage5d_test_sync_runtime_pending_cache(envelope);
                envelope.riskgate.durable_finalization_outbox[0].state =
                    Stage5dRiskGateFinalizationState::AcknowledgedInRuntime;
            });

        let blocked = expect_stage5d_riskgate_blocked(
            stage5d_inject_authoritative_riskgate(bootstrapped, evidence),
            "acknowledged outbox state requires a durable ledger row",
        );
        assert_eq!(
            blocked.reason(),
            Stage5dRiskGateInjectionBlockReason::OutboxStateInconsistent
        );
    }

    #[test]
    fn stage5d_b2bc1_riskgate_prepared_existing_row_requires_exact_payload() {
        let (bootstrapped, _envelope, evidence) =
            riskgate_enabled_bootstrapped_fixture_with_evidence(|envelope, evidence| {
                stage5d_test_configure_single_tail_crash_frontier(
                    envelope,
                    evidence,
                    Stage5dRiskGateFinalizationState::Prepared,
                    true,
                    false,
                    false,
                    true,
                );
                envelope
                    .runtime_private_extension
                    .runtime_pending_finalizations[0]
                    .shadow_pnl_points = "999.0".to_string();
                stage5d_test_sync_runtime_pending_cache(envelope);
            });

        let blocked = expect_stage5d_riskgate_blocked(
            stage5d_inject_authoritative_riskgate(bootstrapped, evidence),
            "prepared outbox cannot adopt a mismatched existing ledger row",
        );
        assert_eq!(
            blocked.reason(),
            Stage5dRiskGateInjectionBlockReason::OutboxStateInconsistent
        );
    }

    #[test]
    fn stage5d_b2bc1_riskgate_prepared_requires_runtime_pending() {
        let (bootstrapped, _envelope, evidence) =
            riskgate_enabled_bootstrapped_fixture_with(|envelope| {
                envelope
                    .runtime_private_extension
                    .runtime_pending_finalizations
                    .clear();
                stage5d_test_sync_runtime_pending_cache(envelope);
            });

        let blocked = expect_stage5d_riskgate_blocked(
            stage5d_inject_authoritative_riskgate(bootstrapped, evidence),
            "prepared outbox state requires exact runtime pending finalization",
        );
        assert_eq!(
            blocked.reason(),
            Stage5dRiskGateInjectionBlockReason::OutboxStateInconsistent
        );
    }

    #[test]
    fn stage5d_b2bc1_riskgate_prepared_missing_rows_plan_one_append_each() {
        let (bootstrapped, envelope, evidence) = riskgate_enabled_bootstrapped_fixture_with(|_| {});
        let injected = expect_stage5d_riskgate_ok(
            stage5d_inject_authoritative_riskgate(bootstrapped, evidence),
            "prepared missing ledger rows must produce a retained append plan",
        );
        let decisions = &injected.recovery_plan.decisions;
        assert_eq!(
            decisions.len(),
            envelope.riskgate.durable_finalization_outbox.len()
        );
        assert!(decisions
            .iter()
            .all(|decision| decision.action
                == Stage5dRiskGateRecoveryDecision::AppendMissingLedgerRow));
        assert!(!injected.recovery_complete());
    }

    #[test]
    fn stage5d_b2bc1_riskgate_prepared_existing_row_never_plans_duplicate_append() {
        let (bootstrapped, _envelope, evidence) =
            riskgate_enabled_bootstrapped_fixture_with_evidence(|envelope, evidence| {
                stage5d_test_configure_single_tail_crash_frontier(
                    envelope,
                    evidence,
                    Stage5dRiskGateFinalizationState::Prepared,
                    true,
                    false,
                    false,
                    true,
                );
            });
        let injected = expect_stage5d_riskgate_ok(
            stage5d_inject_authoritative_riskgate(bootstrapped, evidence),
            "matching prepared/ledger payload must be accepted without duplicate append",
        );
        let decisions = &injected.recovery_plan.decisions;
        assert_eq!(
            decisions[0].action,
            Stage5dRiskGateRecoveryDecision::AdvanceToMaterialized
        );
        assert!(!injected.recovery_complete());
    }

    #[test]
    fn stage5d_b2bc_riskgate_outbox_rejects_ledger_appended_without_ledger_row() {
        let (bootstrapped, _envelope, evidence) =
            riskgate_enabled_bootstrapped_fixture_with(|envelope| {
                envelope.riskgate.durable_finalization_outbox[0].state =
                    Stage5dRiskGateFinalizationState::LedgerAppended;
            });

        let blocked = expect_stage5d_riskgate_blocked(
            stage5d_inject_authoritative_riskgate(bootstrapped, evidence),
            "ledger-appended outbox entry requires matching authoritative ledger row",
        );
        assert_eq!(
            blocked.reason(),
            Stage5dRiskGateInjectionBlockReason::OutboxStateInconsistent
        );
    }

    #[test]
    fn stage5d_b2bc1_riskgate_outbox_rejects_materialized_without_ledger_row() {
        let (bootstrapped, _envelope, evidence) =
            riskgate_enabled_bootstrapped_fixture_with(|envelope| {
                envelope.riskgate.durable_finalization_outbox[0].state =
                    Stage5dRiskGateFinalizationState::MaterializedUpdated;
            });

        let blocked = expect_stage5d_riskgate_blocked(
            stage5d_inject_authoritative_riskgate(bootstrapped, evidence),
            "materialized-updated outbox entry requires matching durable ledger row",
        );
        assert_eq!(
            blocked.reason(),
            Stage5dRiskGateInjectionBlockReason::OutboxStateInconsistent
        );
    }

    #[test]
    fn stage5d_b2bc1_riskgate_outbox_rejects_duplicate_session_and_identity() {
        let (bootstrapped, _envelope, evidence) =
            riskgate_enabled_bootstrapped_fixture_with(|envelope| {
                let duplicate = envelope.riskgate.durable_finalization_outbox[0].clone();
                envelope
                    .riskgate
                    .durable_finalization_outbox
                    .push(duplicate);
            });
        let blocked = expect_stage5d_riskgate_blocked(
            stage5d_inject_authoritative_riskgate(bootstrapped, evidence),
            "duplicate outbox session must block",
        );
        assert_eq!(
            blocked.reason(),
            Stage5dRiskGateInjectionBlockReason::DuplicateOutboxSession
        );

        let (bootstrapped, _envelope, evidence) =
            riskgate_enabled_bootstrapped_fixture_with(|envelope| {
                align_riskgate_outbox_to_runtime_pending(envelope);
                let first_hash = envelope.riskgate.durable_finalization_outbox[0]
                    .identity_hash
                    .clone();
                envelope.riskgate.durable_finalization_outbox[1].identity_hash = first_hash;
            });
        let blocked = expect_stage5d_riskgate_blocked(
            stage5d_inject_authoritative_riskgate(bootstrapped, evidence),
            "duplicate outbox identity must block",
        );
        assert_eq!(
            blocked.reason(),
            Stage5dRiskGateInjectionBlockReason::DuplicateOutboxIdentity
        );
    }

    #[test]
    fn stage5d_b2bc1_riskgate_outbox_rejects_reordered_generation_and_state() {
        let (bootstrapped, _envelope, evidence) =
            riskgate_enabled_bootstrapped_fixture_with(|envelope| {
                align_riskgate_outbox_to_runtime_pending(envelope);
                envelope.riskgate.durable_finalization_outbox[0].generation = 2;
                envelope.riskgate.durable_finalization_outbox[1].generation = 1;
                for record in &mut envelope.riskgate.durable_finalization_outbox {
                    let date = NaiveDate::parse_from_str(&record.session_date, "%Y-%m-%d")
                        .expect("outbox date");
                    record.identity_hash = stage5d_riskgate_outbox_identity_hash(
                        &envelope.riskgate.identity,
                        date,
                        record.generation,
                    );
                }
            });
        let blocked = expect_stage5d_riskgate_blocked(
            stage5d_inject_authoritative_riskgate(bootstrapped, evidence),
            "outbox generation must increase with session chronology",
        );
        assert_eq!(
            blocked.reason(),
            Stage5dRiskGateInjectionBlockReason::OutboxStateInconsistent
        );

        let (bootstrapped, _envelope, evidence) =
            riskgate_enabled_bootstrapped_fixture_with(|envelope| {
                align_riskgate_outbox_to_runtime_pending(envelope);
                envelope.riskgate.durable_finalization_outbox[1].state =
                    Stage5dRiskGateFinalizationState::AcknowledgedInRuntime;
                envelope
                    .runtime_private_extension
                    .runtime_pending_finalizations
                    .truncate(1);
            });
        let blocked = expect_stage5d_riskgate_blocked(
            stage5d_inject_authoritative_riskgate(bootstrapped, evidence),
            "a later session cannot be more advanced than an earlier unfinished session",
        );
        assert_eq!(
            blocked.reason(),
            Stage5dRiskGateInjectionBlockReason::OutboxStateInconsistent
        );
    }

    #[test]
    fn stage5d_b2bc_riskgate_outbox_rejects_generation_zero_and_identity_mismatch() {
        let (bootstrapped, _envelope, evidence) =
            riskgate_enabled_bootstrapped_fixture_with(|envelope| {
                envelope.riskgate.durable_finalization_outbox[0].generation = 0;
            });
        let blocked = expect_stage5d_riskgate_blocked(
            stage5d_inject_authoritative_riskgate(bootstrapped, evidence),
            "generation zero must block",
        );
        assert_eq!(
            blocked.reason(),
            Stage5dRiskGateInjectionBlockReason::OutboxStateInconsistent
        );

        let (bootstrapped, _envelope, evidence) =
            riskgate_enabled_bootstrapped_fixture_with(|envelope| {
                envelope.riskgate.durable_finalization_outbox[0].identity_hash =
                    "wrong".to_string();
            });
        let blocked = expect_stage5d_riskgate_blocked(
            stage5d_inject_authoritative_riskgate(bootstrapped, evidence),
            "identity hash mismatch must block",
        );
        assert_eq!(
            blocked.reason(),
            Stage5dRiskGateInjectionBlockReason::OutboxIdentityMismatch
        );
    }

    #[test]
    fn stage5d_b2bc_riskgate_retry_uses_fresh_ledger_evidence_without_rebootstrap() {
        let (bootstrapped, _envelope, evidence) =
            riskgate_enabled_bootstrapped_fixture_with(|_| {});
        let mut stale_raw = evidence.evidence.clone();
        stale_raw.ledger_records.pop();
        stale_raw.ledger_tail_hash = stage5d_compute_riskgate_ledger_tail_hash(&stale_raw).unwrap();
        let stale_evidence = stage5d_validate_riskgate_ledger_evidence(stale_raw).unwrap();
        let blocked = expect_stage5d_riskgate_blocked(
            stage5d_inject_authoritative_riskgate(bootstrapped, stale_evidence),
            "stale ledger evidence must block",
        );
        assert!(blocked.input_capability_preserved());

        let injected = expect_stage5d_riskgate_ok(
            stage5d_retry_authoritative_riskgate_injection(blocked, evidence),
            "fresh evidence must retry without repeating private apply/bootstrap",
        );
        assert!(injected.riskgate_injected());
    }

    #[test]
    fn stage5d_b2bb_bootstrap_blocks_position_drift_before_callback() {
        let (applied, envelope) = applied_stage5d_fixture_with(
            |_| {},
            |admission| admission.stage5d_test_with_target_position_qty(Decimal::new(15, 1)),
        );

        let blocked = expect_stage5d_bootstrap_blocked(
            stage5d_notify_broker_truth_bootstrap_at(applied, envelope.persisted_at_ts_utc),
            "broker/persisted position drift must block bootstrap",
        );
        assert_eq!(
            blocked.reason(),
            Stage5dBootstrapBlockReason::BrokerTruthPositionMismatch
        );
        assert!(blocked.input_capability_preserved());
        assert_eq!(blocked.snapshot_id(), "SNAP_STAGE5D_B2A_0001");
    }

    #[test]
    fn stage5d_b2bb_bootstrap_blocks_missing_expected_working_order() {
        let (applied, envelope) = applied_stage5d_fixture_with(
            |envelope| {
                envelope
                    .runtime_private_extension
                    .expected_working_sets
                    .expected_working_order_ids
                    .push(BrokerOrderId::new("EXPECTED-WORKING-ORDER"));
                envelope
                    .recovery_indexes
                    .known_order_ids
                    .push(BrokerOrderId::new("EXPECTED-WORKING-ORDER"));
            },
            |admission| admission,
        );

        let blocked = expect_stage5d_bootstrap_blocked(
            stage5d_notify_broker_truth_bootstrap_at(applied, envelope.persisted_at_ts_utc),
            "expected working order absent from broker truth must block bootstrap",
        );
        assert_eq!(
            blocked.reason(),
            Stage5dBootstrapBlockReason::ExpectedWorkingOrderMissing
        );
        assert!(blocked.input_capability_preserved());
    }

    #[test]
    fn stage5d_b2bb_bootstrap_treats_confirmed_active_order_as_closed_stage5c_surface() {
        let expected_order_id_for_envelope = BrokerOrderId::new("EXPECTED-WORKING-ORDER");
        let expected_order_id_for_admission = expected_order_id_for_envelope.clone();
        let (applied, envelope) = applied_stage5d_fixture_with(
            |envelope| {
                envelope
                    .runtime_private_extension
                    .expected_working_sets
                    .expected_working_order_ids
                    .push(expected_order_id_for_envelope.clone());
                envelope
                    .recovery_indexes
                    .known_order_ids
                    .push(expected_order_id_for_envelope);
            },
            |admission| {
                let order = stage5d_active_order(
                    admission.account_id().clone(),
                    admission.target_instrument().clone(),
                    expected_order_id_for_admission,
                    admission.checked_ts(),
                );
                admission.stage5d_test_with_target_active_orders(vec![order])
            },
        );

        let blocked = expect_stage5d_bootstrap_blocked(
            stage5d_notify_broker_truth_bootstrap_at(applied, envelope.persisted_at_ts_utc),
            "active orders are matched but still closed until ownership mapping opens",
        );
        assert_eq!(
            blocked.reason(),
            Stage5dBootstrapBlockReason::ActiveOrdersRequireOwnershipMapping
        );
        assert!(blocked.input_capability_preserved());
    }

    #[test]
    fn stage5d_b2bb_bootstrap_blocks_expected_stop_hints_until_stop_surface_opens() {
        let (applied, envelope) = applied_stage5d_fixture_with(
            |envelope| {
                envelope
                    .runtime_private_extension
                    .expected_working_sets
                    .expected_working_stop_order_ids
                    .push(BrokerStopOrderId::new("EXPECTED-WORKING-STOP"));
                envelope
                    .recovery_indexes
                    .known_stop_order_ids
                    .push(BrokerStopOrderId::new("EXPECTED-WORKING-STOP"));
            },
            |admission| admission,
        );

        let blocked = expect_stage5d_bootstrap_blocked(
            stage5d_notify_broker_truth_bootstrap_at(applied, envelope.persisted_at_ts_utc),
            "expected stop hints must block until broker stop truth surface opens",
        );
        assert_eq!(
            blocked.reason(),
            Stage5dBootstrapBlockReason::ExpectedWorkingStopUnsupported
        );
        assert!(blocked.input_capability_preserved());
    }

    #[test]
    fn stage5d_b2bb_bootstrap_preserves_applied_capability_on_expired_admission() {
        let (applied, envelope) = applied_stage5d_fixture();

        let blocked = expect_stage5d_bootstrap_blocked(
            stage5d_notify_broker_truth_bootstrap_at(
                applied,
                envelope.persisted_at_ts_utc + chrono::Duration::hours(2),
            ),
            "expired admission must block without losing applied capability",
        );
        assert_eq!(
            blocked.reason(),
            Stage5dBootstrapBlockReason::AdmissionExpired
        );
        assert!(blocked.input_capability_preserved());
    }

    #[test]
    fn stage5d_b2bb_retry_expired_admission_with_fresh_admission_succeeds() {
        let (applied, envelope) = applied_stage5d_fixture();
        let expired_at = envelope.persisted_at_ts_utc + chrono::Duration::hours(2);
        let blocked = expect_stage5d_bootstrap_blocked(
            stage5d_notify_broker_truth_bootstrap_at(applied, expired_at),
            "expired admission must preserve capability for retry",
        );
        let fresh_checked_ts = expired_at;
        let fresh_admission =
            stage5d_fresh_admission_for_envelope(&envelope, Decimal::ZERO, fresh_checked_ts);

        let bootstrapped = expect_stage5d_bootstrap_ok(
            stage5d_retry_broker_truth_bootstrap_at(blocked, fresh_admission, fresh_checked_ts),
            "fresh matching admission must retry without reapplying private state",
        );

        assert_eq!(bootstrapped.snapshot_id(), "SNAP_STAGE5D_B2A_0001");
        assert!(bootstrapped.bootstrap_notified());
    }

    #[test]
    fn stage5d_b2bb_retry_missing_order_with_fresh_snapshot_reaches_active_order_boundary() {
        let expected_order_id_for_envelope = BrokerOrderId::new("EXPECTED-WORKING-ORDER");
        let expected_order_id_for_admission = expected_order_id_for_envelope.clone();
        let (applied, envelope) = applied_stage5d_fixture_with(
            |envelope| {
                envelope
                    .runtime_private_extension
                    .expected_working_sets
                    .expected_working_order_ids
                    .push(expected_order_id_for_envelope.clone());
                envelope
                    .recovery_indexes
                    .known_order_ids
                    .push(expected_order_id_for_envelope);
            },
            |admission| admission,
        );
        let blocked = expect_stage5d_bootstrap_blocked(
            stage5d_notify_broker_truth_bootstrap_at(applied, envelope.persisted_at_ts_utc),
            "missing expected order must block before callback",
        );
        assert_eq!(
            blocked.reason(),
            Stage5dBootstrapBlockReason::ExpectedWorkingOrderMissing
        );
        let fresh_ts = envelope.persisted_at_ts_utc + chrono::Duration::minutes(1);
        let fresh_admission =
            stage5d_fresh_admission_for_envelope(&envelope, Decimal::ZERO, fresh_ts)
                .stage5d_test_with_target_active_orders(vec![stage5d_active_order(
                    envelope.binding.account_id.clone(),
                    envelope.binding.instrument_id.to_instrument_id(),
                    expected_order_id_for_admission,
                    fresh_ts,
                )]);

        let blocked = expect_stage5d_bootstrap_blocked(
            stage5d_retry_broker_truth_bootstrap_at(blocked, fresh_admission, fresh_ts),
            "confirmed active order must advance to ownership-mapping boundary",
        );

        assert_eq!(
            blocked.reason(),
            Stage5dBootstrapBlockReason::ActiveOrdersRequireOwnershipMapping
        );
        assert!(blocked.input_capability_preserved());
    }

    #[test]
    fn stage5d_b2bb_retry_rejects_cross_account_fresh_admission_and_preserves_capability() {
        let (applied, envelope) = applied_stage5d_fixture();
        let expired_at = envelope.persisted_at_ts_utc + chrono::Duration::hours(2);
        let blocked = expect_stage5d_bootstrap_blocked(
            stage5d_notify_broker_truth_bootstrap_at(applied, expired_at),
            "expired admission must preserve capability for retry",
        );
        let fresh_ts = expired_at;
        let fresh_admission =
            crate::stage5c_paper_host::Stage5cPaperHostAdmission::stage5d_test_new(
                envelope.binding.strategy_id.clone(),
                BrokerAccountId::new("ACC_TEST_0002"),
                envelope.binding.instrument_id.to_instrument_id(),
                0.5,
                Decimal::ZERO,
                fresh_ts,
            );

        let blocked = expect_stage5d_bootstrap_blocked(
            stage5d_retry_broker_truth_bootstrap_at(blocked, fresh_admission, fresh_ts),
            "cross-account fresh admission must not replace preserved capability",
        );

        assert_eq!(
            blocked.reason(),
            Stage5dBootstrapBlockReason::BindingMismatch
        );
        assert!(blocked.input_capability_preserved());
        assert_eq!(blocked.snapshot_id(), "SNAP_STAGE5D_B2A_0001");
    }

    #[test]
    fn stage5d_b2b_blocked_capability_can_retry_with_matching_envelope() {
        let (loaded, mut mismatched) = stage5d_bound_test_fixture();
        let mut corrected = mismatched.clone();
        mismatched.binding.account_id = BrokerAccountId::new("ACC_TEST_0002");
        mismatched.payload_checksum_sha256 = mismatched
            .compute_payload_checksum_sha256()
            .expect("checksum recomputation must succeed");

        let blocked = expect_stage5d_blocked(
            stage5d_bind_runtime_state_loaded(
                loaded,
                mismatched
                    .validate_restore_contract_schema_only()
                    .expect("mismatched envelope remains schema valid"),
            ),
            "mismatched envelope must be blocked",
        );
        assert_eq!(
            blocked.reason(),
            Stage5dEnvelopeValidationError::BindingMismatch
        );

        corrected.payload_checksum_sha256 = corrected
            .compute_payload_checksum_sha256()
            .expect("checksum recomputation must succeed");
        let bound = expect_stage5d_ok(
            stage5d_retry_bind_runtime_state_loaded(
                blocked,
                corrected
                    .validate_restore_contract_schema_only()
                    .expect("corrected envelope must validate"),
            ),
            "retry with exact envelope must recover preserved capability",
        );
        let applied = expect_stage5d_ok(
            stage5d_apply_runtime_private_extension(bound),
            "retried bound capability must apply",
        );
        assert_eq!(applied.snapshot_id(), "SNAP_STAGE5D_B2A_0001");
    }

    #[test]
    fn stage5d_b2b_cleanup_retry_state_none_is_rejected_with_recomputed_checksum() {
        let envelope = fixture_with_mutated_envelope(|envelope| {
            envelope.runtime_private_extension.cleanup_retry_state = None;
        });

        assert_eq!(
            envelope.validate_restore_contract_schema_only().map(|_| ()),
            Err(Stage5dEnvelopeValidationError::PendingStateInconsistent)
        );
    }

    #[test]
    fn stage5d_b2b_cleanup_retry_attempts_roundtrip_exactly() {
        let mut envelope = apply_valid_fixture();
        let mut strategy = stage5d_test_strategy_with_config(|config| {
            config.qty = 3.0;
        });
        restore_semantic_state(&mut strategy, &envelope);
        envelope.runtime_private_extension.cleanup_retry_state = Some(Stage5dCleanupRetryState {
            cleanup_stop_retry_attempts: 2,
        });
        envelope.payload_checksum_sha256 = envelope
            .compute_payload_checksum_sha256()
            .expect("checksum recomputation must succeed");

        strategy
            .stage5d_apply_runtime_private_extension(&envelope.runtime_private_extension)
            .expect("validated private extension must apply");
        assert_eq!(
            strategy
                .stage5d_export_runtime_private_extension()
                .expect("stage5d export")
                .cleanup_retry_state,
            Some(Stage5dCleanupRetryState {
                cleanup_stop_retry_attempts: 2,
            })
        );
    }

    #[test]
    fn stage5d_b2b_public_bind_rejects_active_cycle_mismatch_before_apply() {
        let (loaded, mut envelope) = stage5d_bound_test_fixture();
        if let Value::Object(fields) =
            &mut envelope.strategy_state.strategy_state_json["HybridIntradayRuntime"]
        {
            fields.insert(
                "active_cycle_id".to_string(),
                Value::String("deadbeef00".to_string()),
            );
            fields.insert(
                "pending_entry_cycle_id".to_string(),
                Value::String("deadbeef00".to_string()),
            );
        }
        envelope.payload_checksum_sha256 = envelope
            .compute_payload_checksum_sha256()
            .expect("checksum recomputation must succeed");
        let validated = envelope
            .validate_restore_contract_schema_only()
            .expect("cycle-mismatched envelope remains schema valid");

        let blocked = expect_stage5d_blocked(
            stage5d_bind_runtime_state_loaded(loaded, validated),
            "active cycle mismatch must be blocked before apply",
        );
        assert_eq!(
            blocked.reason(),
            Stage5dEnvelopeValidationError::BindingMismatch
        );
    }

    #[test]
    fn stage5d_b2b_public_bind_rejects_pending_request_index_mismatch_before_apply() {
        let (loaded, envelope) = stage5d_bound_test_fixture();
        let validated = envelope
            .validate_restore_contract_schema_only()
            .expect("envelope remains schema valid");
        let (strategy, admission, restored, load_origin) = loaded.stage5d_into_parts();
        let mismatched_loaded =
            crate::stage5c_paper_host::Stage5cRuntimeStateLoadedPaperStrategy::stage5d_test_loaded_from_parts(
                strategy,
                admission,
                crate::runtime_compat::RuntimeStateRestored {
                    known_order_ids: restored.known_order_ids,
                    pending_requests: Vec::new(),
                },
                load_origin,
            );

        let blocked = expect_stage5d_blocked(
            stage5d_bind_runtime_state_loaded(mismatched_loaded, validated),
            "pending request mismatch must be blocked before apply",
        );
        assert_eq!(
            blocked.reason(),
            Stage5dEnvelopeValidationError::RecoveryIndexInconsistent
        );
    }

    #[test]
    fn stage5d_b2b_public_bind_accepts_recomputable_field_mismatch_before_warmup() {
        let (loaded, mut envelope) = stage5d_bound_test_fixture();
        if let Value::Object(fields) =
            &mut envelope.strategy_state.strategy_state_json["HybridIntradayRuntime"]
        {
            fields.insert("last_bar_close".to_string(), serde_json::json!(9999.5));
            fields.insert("prev_day_close".to_string(), serde_json::json!(8888.5));
            fields.insert("entry_ready".to_string(), Value::Bool(true));
        }
        envelope.payload_checksum_sha256 = envelope
            .compute_payload_checksum_sha256()
            .expect("checksum recomputation must succeed");
        let validated = envelope
            .validate_restore_contract_schema_only()
            .expect("recomputable field mismatch remains schema valid");

        expect_stage5d_ok(
            stage5d_bind_runtime_state_loaded(loaded, validated),
            "recomputable mismatch must be retained for later warmup verification",
        );
    }

    #[test]
    fn stage5d_b2b_public_bind_rejects_stage5d_canonical_fingerprint_mismatch() {
        let (loaded, mut envelope) = stage5d_bound_test_fixture();
        envelope.canonical_config_fingerprint =
            "stage5d_cfg_sha256:ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
                .to_string();
        envelope.binding.stage5d_canonical_config_fingerprint =
            envelope.canonical_config_fingerprint.clone();
        envelope.payload_checksum_sha256 = envelope
            .compute_payload_checksum_sha256()
            .expect("checksum recomputation must succeed");
        let validated = envelope
            .validate_restore_contract_schema_only()
            .expect("self-consistent wrong Stage 5D fingerprint remains schema valid");

        let blocked = expect_stage5d_blocked(
            stage5d_bind_runtime_state_loaded(loaded, validated),
            "Stage 5D canonical fingerprint mismatch must be blocked",
        );
        assert_eq!(
            blocked.reason(),
            Stage5dEnvelopeValidationError::BindingMismatch
        );
    }

    #[test]
    fn stage5d_b2b_public_bind_rejects_unsupported_schema_versions() {
        let (loaded, mut envelope) = stage5d_bound_test_fixture();
        envelope.binding.broker_protocol_schema_version = 999;
        envelope.binding.runtime_state_schema_version = 999;
        envelope.payload_checksum_sha256 = envelope
            .compute_payload_checksum_sha256()
            .expect("checksum recomputation must succeed");
        let validated = envelope
            .validate_restore_contract_schema_only()
            .expect("nonzero unsupported versions remain schema valid");

        let blocked = expect_stage5d_blocked(
            stage5d_bind_runtime_state_loaded(loaded, validated),
            "unsupported schema versions must be blocked",
        );
        assert_eq!(
            blocked.reason(),
            Stage5dEnvelopeValidationError::BindingMismatch
        );
    }

    #[test]
    fn stage5d_b2b_public_bind_rejects_unsupported_source_build() {
        let (loaded, mut envelope) = stage5d_bound_test_fixture();
        envelope.binding.source_commit_or_build_id = "source_commit:unsupported".to_string();
        envelope.payload_checksum_sha256 = envelope
            .compute_payload_checksum_sha256()
            .expect("checksum recomputation must succeed");
        let validated = envelope
            .validate_restore_contract_schema_only()
            .expect("unsupported source build remains schema valid");

        let blocked = expect_stage5d_blocked(
            stage5d_bind_runtime_state_loaded(loaded, validated),
            "unsupported semantic source build must be blocked at pair-binding",
        );
        assert_eq!(
            blocked.reason(),
            Stage5dEnvelopeValidationError::BindingMismatch
        );
    }

    #[test]
    fn stage5d_b2b_public_bind_accepts_allowlisted_prior_source_build() {
        let (loaded, envelope) = stage5d_bound_test_fixture();
        assert_eq!(
            envelope.binding.source_commit_or_build_id,
            "source_commit:92e6e0685b1cbab6f4c6271abe1db8ab690a1ded"
        );
        let validated = envelope
            .validate_restore_contract_schema_only()
            .expect("allowlisted prior source build remains schema valid");

        expect_stage5d_ok(
            stage5d_bind_runtime_state_loaded(loaded, validated),
            "allowlisted prior source build must bind through explicit compatibility policy",
        );
    }

    #[test]
    fn stage5d_b2b_canonical_fingerprint_includes_riskgate_identity_hashes() {
        let ledger_a = stage5d_test_strategy_with_config(|config| {
            config.risk_gate_ledger_key = Some("ledger-A".to_string());
        });
        let ledger_b = stage5d_test_strategy_with_config(|config| {
            config.risk_gate_ledger_key = Some("ledger-B".to_string());
        });
        let seed_a = stage5d_test_strategy_with_config(|config| {
            config.risk_gate_seed_file = Some("/riskgate/seed-A.json".to_string());
        });
        let seed_b = stage5d_test_strategy_with_config(|config| {
            config.risk_gate_seed_file = Some("/riskgate/seed-B.json".to_string());
        });

        assert_ne!(
            ledger_a.stage5d_canonical_config_fingerprint(),
            ledger_b.stage5d_canonical_config_fingerprint()
        );
        assert_ne!(
            seed_a.stage5d_canonical_config_fingerprint(),
            seed_b.stage5d_canonical_config_fingerprint()
        );
    }

    #[test]
    fn stage5d_b2b_public_bind_rejects_clean_flat_loaded_against_flat_persisted_envelope() {
        let envelope = flat_persisted_fixture();
        let (loaded, envelope) = stage5d_loaded_for_envelope(
            envelope,
            stage5d_test_strategy(),
            crate::stage5c_paper_host::Stage5cRuntimeStateLoadOrigin::CleanStart,
        );
        let validated = envelope
            .validate_restore_contract_schema_only()
            .expect("flat envelope remains schema valid");

        let blocked = expect_stage5d_blocked(
            stage5d_bind_runtime_state_loaded(loaded, validated),
            "clean-start loaded capability must not bind to a flat persisted envelope",
        );
        assert_eq!(
            blocked.reason(),
            Stage5dEnvelopeValidationError::BindingMismatch
        );
    }

    #[test]
    fn stage5d_b2b_public_bind_accepts_persisted_flat_envelope_with_empty_indexes() {
        let envelope = flat_persisted_fixture();
        let origin = load_origin_for_envelope(&envelope);
        let (loaded, envelope) =
            stage5d_loaded_for_envelope(envelope, stage5d_test_strategy(), origin);
        let validated = envelope
            .validate_restore_contract_schema_only()
            .expect("flat envelope remains schema valid");

        expect_stage5d_ok(
            stage5d_bind_runtime_state_loaded(loaded, validated),
            "persisted flat loaded capability must bind to its own flat envelope",
        );
    }

    #[test]
    fn stage5d_b2b_public_bind_rejects_persisted_semantic_fingerprint_mismatch() {
        let envelope = flat_persisted_fixture();
        let mut origin = load_origin_for_envelope(&envelope);
        if let crate::stage5c_paper_host::Stage5cRuntimeStateLoadOrigin::Persisted {
            semantic_payload_fingerprint,
            ..
        } = &mut origin
        {
            *semantic_payload_fingerprint = "stage5c_semantic_sha256:bad".to_string();
        }
        let (loaded, envelope) =
            stage5d_loaded_for_envelope(envelope, stage5d_test_strategy(), origin);
        let validated = envelope
            .validate_restore_contract_schema_only()
            .expect("flat envelope remains schema valid");

        let blocked = expect_stage5d_blocked(
            stage5d_bind_runtime_state_loaded(loaded, validated),
            "persisted loaded semantic fingerprint must match the envelope",
        );
        assert_eq!(
            blocked.reason(),
            Stage5dEnvelopeValidationError::BindingMismatch
        );
    }

    fn expect_private_apply_rejects_without_mutation(
        mut envelope: Stage5dPersistenceEnvelope,
        mutator: impl FnOnce(
            &mut crate::hybrid_intraday_runtime::HybridIntradayRuntimeStrategy,
            &mut Stage5dRuntimePrivateExtension,
        ),
    ) {
        let mut strategy = stage5d_test_strategy();
        restore_semantic_state(&mut strategy, &envelope);
        let mut extension = envelope.runtime_private_extension.clone();
        mutator(&mut strategy, &mut extension);
        let before = strategy.stage5d_export_runtime_private_extension();
        assert_eq!(
            strategy.stage5d_apply_runtime_private_extension(&extension),
            Err(Stage5dEnvelopeValidationError::PendingStateInconsistent)
        );
        assert_eq!(strategy.stage5d_export_runtime_private_extension(), before);
        envelope.runtime_private_extension = extension;
    }

    #[test]
    fn stage5d_b2b_private_apply_rejects_cleanup_retry_above_source_max() {
        expect_private_apply_rejects_without_mutation(valid_fixture(), |_strategy, extension| {
            extension.cleanup_retry_state = Some(Stage5dCleanupRetryState {
                cleanup_stop_retry_attempts: 4,
            });
        });
    }

    #[test]
    fn stage5d_b2b_private_apply_rejects_partial_entry_sign_mismatch() {
        expect_private_apply_rejects_without_mutation(valid_fixture(), |_strategy, extension| {
            extension
                .pending_entry
                .as_mut()
                .expect("pending entry")
                .side = Stage5dSide::Short;
        });

        let envelope = fixture_with_mutated_state(|state| {
            state["HybridIntradayRuntime"]["last_position_qty"] = serde_json::json!(-0.5);
            state["HybridIntradayRuntime"]["current_side"] = Value::String("short".to_string());
        });
        expect_private_apply_rejects_without_mutation(envelope, |_strategy, extension| {
            extension
                .pending_entry
                .as_mut()
                .expect("pending entry")
                .side = Stage5dSide::Long;
        });
    }

    #[test]
    fn stage5d_b2b_private_apply_rejects_market_or_breakout_partial_pending() {
        expect_private_apply_rejects_without_mutation(valid_fixture(), |_strategy, extension| {
            extension
                .pending_entry
                .as_mut()
                .expect("pending entry")
                .entry_style = Stage5dEntryStyle::Market;
        });

        expect_private_apply_rejects_without_mutation(valid_fixture(), |_strategy, extension| {
            let entry = extension.pending_entry.as_mut().expect("pending entry");
            entry.owner = Stage5dOwner::IntradayBreakout;
            entry.entry_style = Stage5dEntryStyle::Market;
        });
    }

    #[test]
    fn stage5d_b2b_private_apply_rejects_target_qty_not_matching_runtime_config() {
        let envelope = apply_valid_fixture();
        let mut strategy = stage5d_test_strategy();
        restore_semantic_state(&mut strategy, &envelope);
        let before = strategy.stage5d_export_runtime_private_extension();

        assert_eq!(
            strategy.stage5d_apply_runtime_private_extension(&envelope.runtime_private_extension),
            Err(Stage5dEnvelopeValidationError::SourceRoundtripInconsistent)
        );
        assert_eq!(strategy.stage5d_export_runtime_private_extension(), before);
    }

    #[test]
    fn stage5d_b2b_private_apply_accepts_target_qty_matching_runtime_config() {
        let envelope = apply_valid_fixture();
        let mut strategy = stage5d_test_strategy_with_config(|config| {
            config.qty = 3.0;
        });
        restore_semantic_state(&mut strategy, &envelope);

        strategy
            .stage5d_apply_runtime_private_extension(&envelope.runtime_private_extension)
            .expect("source-valid config qty 3 / target qty 3 fixture must apply");
    }

    #[test]
    fn stage5d_b2b_private_apply_rejects_flat_mr_market_pending_entry() {
        let envelope = flat_pending_entry_fixture(|envelope| {
            let entry = envelope
                .runtime_private_extension
                .pending_entry
                .as_mut()
                .expect("pending entry");
            entry.entry_style = Stage5dEntryStyle::Market;
            entry.reason = Stage5dLifecycleReason::MorningMeanReversionLong;
        });
        let mut strategy = stage5d_test_strategy_with_config(|config| {
            config.qty = 3.0;
        });
        restore_semantic_state(&mut strategy, &envelope);
        let before = strategy.stage5d_export_runtime_private_extension();

        assert_eq!(
            strategy.stage5d_apply_runtime_private_extension(&envelope.runtime_private_extension),
            Err(Stage5dEnvelopeValidationError::PendingStateInconsistent)
        );
        assert_eq!(strategy.stage5d_export_runtime_private_extension(), before);
    }

    #[test]
    fn stage5d_b2b_private_apply_rejects_flat_breakout_bracket_pending_entry() {
        let envelope = flat_pending_entry_fixture(|envelope| {
            if let Value::Object(fields) =
                &mut envelope.strategy_state.strategy_state_json["HybridIntradayRuntime"]
            {
                fields.insert(
                    "pending_entry_owner".to_string(),
                    Value::String("intraday_breakout".to_string()),
                );
            }
            let entry = envelope
                .runtime_private_extension
                .pending_entry
                .as_mut()
                .expect("pending entry");
            entry.owner = Stage5dOwner::IntradayBreakout;
            entry.entry_style = Stage5dEntryStyle::Bracket;
            entry.reason = Stage5dLifecycleReason::BreakoutLong;
        });
        let mut strategy = stage5d_test_strategy_with_config(|config| {
            config.qty = 3.0;
        });
        restore_semantic_state(&mut strategy, &envelope);
        let before = strategy.stage5d_export_runtime_private_extension();

        assert_eq!(
            strategy.stage5d_apply_runtime_private_extension(&envelope.runtime_private_extension),
            Err(Stage5dEnvelopeValidationError::PendingStateInconsistent)
        );
        assert_eq!(strategy.stage5d_export_runtime_private_extension(), before);
    }

    #[test]
    fn stage5d_b2b_private_apply_rejects_pending_entry_reason_side_mismatch() {
        let envelope = flat_pending_entry_fixture(|envelope| {
            let entry = envelope
                .runtime_private_extension
                .pending_entry
                .as_mut()
                .expect("pending entry");
            entry.reason = Stage5dLifecycleReason::MorningMeanReversionShort;
        });
        let mut strategy = stage5d_test_strategy_with_config(|config| {
            config.qty = 3.0;
        });
        restore_semantic_state(&mut strategy, &envelope);
        let before = strategy.stage5d_export_runtime_private_extension();

        assert_eq!(
            strategy.stage5d_apply_runtime_private_extension(&envelope.runtime_private_extension),
            Err(Stage5dEnvelopeValidationError::PendingStateInconsistent)
        );
        assert_eq!(strategy.stage5d_export_runtime_private_extension(), before);
    }

    #[test]
    fn stage5d_b2b_private_apply_rejects_mr_bracket_without_stop_take() {
        let envelope = flat_pending_entry_fixture(|envelope| {
            let entry = envelope
                .runtime_private_extension
                .pending_entry
                .as_mut()
                .expect("pending entry");
            entry.stop_price = None;
        });
        let mut strategy = stage5d_test_strategy_with_config(|config| {
            config.qty = 3.0;
        });
        restore_semantic_state(&mut strategy, &envelope);
        let before = strategy.stage5d_export_runtime_private_extension();

        assert_eq!(
            strategy.stage5d_apply_runtime_private_extension(&envelope.runtime_private_extension),
            Err(Stage5dEnvelopeValidationError::PendingStateInconsistent)
        );
        assert_eq!(strategy.stage5d_export_runtime_private_extension(), before);
    }

    #[test]
    fn stage5d_b2b_private_apply_rejects_breakout_market_with_stop_take() {
        let envelope = flat_pending_entry_fixture(|envelope| {
            if let Value::Object(fields) =
                &mut envelope.strategy_state.strategy_state_json["HybridIntradayRuntime"]
            {
                fields.insert(
                    "pending_entry_owner".to_string(),
                    Value::String("intraday_breakout".to_string()),
                );
            }
            let entry = envelope
                .runtime_private_extension
                .pending_entry
                .as_mut()
                .expect("pending entry");
            entry.owner = Stage5dOwner::IntradayBreakout;
            entry.entry_style = Stage5dEntryStyle::Market;
            entry.reason = Stage5dLifecycleReason::BreakoutLong;
            entry.stop_price = Some("2200.0".to_string());
            entry.take_price = None;
        });
        let mut strategy = stage5d_test_strategy_with_config(|config| {
            config.qty = 3.0;
        });
        restore_semantic_state(&mut strategy, &envelope);
        let before = strategy.stage5d_export_runtime_private_extension();

        assert_eq!(
            strategy.stage5d_apply_runtime_private_extension(&envelope.runtime_private_extension),
            Err(Stage5dEnvelopeValidationError::PendingStateInconsistent)
        );
        assert_eq!(strategy.stage5d_export_runtime_private_extension(), before);
    }

    #[test]
    fn stage5d_b2b_private_apply_rejects_pending_exit_while_flat_or_without_cycle() {
        let envelope = fixture_with_mutated_state(|state| {
            state["HybridIntradayRuntime"]["last_position_qty"] = serde_json::json!(0.0);
            state["HybridIntradayRuntime"]["current_side"] = Value::Null;
        });
        expect_private_apply_rejects_without_mutation(envelope, |strategy, extension| {
            extension.pending_exit = Some(Stage5dPendingExitExtension {
                owner: Stage5dOwner::MeanReversion,
                reason: Stage5dLifecycleReason::MeanRevTimeCutoff,
                request_id: StrategyRequestId(
                    uuid::Uuid::parse_str("22222222-2222-2222-2222-222222222222").expect("uuid"),
                ),
            });
            let mut state =
                serde_json::to_value(Strategy::state(strategy)).expect("strategy state");
            state["HybridIntradayRuntime"]["pending_exit_request_id"] =
                Value::String("22222222-2222-2222-2222-222222222222".to_string());
            state["HybridIntradayRuntime"]["pending_exit_created_ts_utc"] =
                serde_json::json!(1784009400_i64);
            Strategy::set_state(
                strategy,
                serde_json::from_value(state).expect("mutated strategy state"),
            );
        });

        expect_private_apply_rejects_without_mutation(valid_fixture(), |strategy, extension| {
            extension.pending_exit = Some(Stage5dPendingExitExtension {
                owner: Stage5dOwner::MeanReversion,
                reason: Stage5dLifecycleReason::MeanRevTimeCutoff,
                request_id: StrategyRequestId(
                    uuid::Uuid::parse_str("22222222-2222-2222-2222-222222222222").expect("uuid"),
                ),
            });
            let mut state =
                serde_json::to_value(Strategy::state(strategy)).expect("strategy state");
            state["HybridIntradayRuntime"]["active_cycle_id"] = Value::Null;
            state["HybridIntradayRuntime"]["pending_exit_request_id"] =
                Value::String("22222222-2222-2222-2222-222222222222".to_string());
            state["HybridIntradayRuntime"]["pending_exit_created_ts_utc"] =
                serde_json::json!(1784009400_i64);
            Strategy::set_state(
                strategy,
                serde_json::from_value(state).expect("mutated strategy state"),
            );
        });
    }

    #[test]
    fn stage5d_b2b_private_apply_rejects_bracket_reconcile_marker_while_flat() {
        let envelope = fixture_with_mutated_state(|state| {
            state["HybridIntradayRuntime"]["last_position_qty"] = serde_json::json!(0.0);
            state["HybridIntradayRuntime"]["current_side"] = Value::Null;
            state["HybridIntradayRuntime"]["active_cycle_id"] = Value::Null;
        });
        expect_private_apply_rejects_without_mutation(envelope, |_strategy, extension| {
            extension.bracket_reconciliation_timer = Some(Stage5dBracketReconciliationTimer {
                bracket_terminal_reconcile_started_ms: 1_784_009_300_000,
            });
        });
    }
}
