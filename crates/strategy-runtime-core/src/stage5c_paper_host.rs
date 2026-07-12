use std::collections::{HashMap, HashSet};

use broker_core::{
    BrokerAccountId, BrokerInstrumentSpec, BrokerOrderId, InstrumentId,
    RuntimeHostBootstrapSnapshot, Stage4AcceptedPaperHostEvidence,
    Stage4BootstrapEvidenceReportStatus, Stage4BrokerTruthBootstrapStatus,
    Stage4BrokerTruthFreshnessSection, Stage4BrokerTruthFreshnessStatus,
    Stage4BrokerTruthSourceStatus, Stage4DirtyStartPolicyStatus,
    Stage4RuntimeBootstrapApplicationStatus, Stage4RuntimeBootstrapIntegrationEvent,
    Stage4RuntimeBootstrapIntegrationStatus, Stage4RuntimeLifecycleOrderingStatus,
    StrategyRequestId, STAGE4_BOOTSTRAP_EVIDENCE_REPORT_SCHEMA_VERSION,
    STAGE4_RUNTIME_BOOTSTRAP_APPLICATION_SCHEMA_VERSION,
};
use chrono::{DateTime, Utc};
use rust_decimal::prelude::ToPrimitive;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::hybrid_intraday_runtime::HybridIntradayRuntimeStrategy;
use crate::runtime_compat::{
    BootstrapSnapshot, GatewayPhase, PaperExecutionMode, PositionEvent, RuntimeStateRestored,
    Strategy, StrategyCtx, StrategyState, TradeMode,
};

pub const STAGE5C_PAPER_HOST_ADMISSION_SCHEMA_VERSION: u16 = 1;
pub const STAGE5C_RUNTIME_STATE_RESTORE_SCHEMA_VERSION: u16 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage5cPaperHostAdmissionError {
    Stage4ReportSchemaMismatch,
    Stage4ReportNotAccepted,
    Stage4EvidenceChainInconsistent,
    Stage4SafetyBoundaryOpen,
    Stage4ApplicationSchemaMismatch,
    Stage4ApplicationNotApplied,
    Stage4ApplicationInconsistent,
    Stage4ApplicationSnapshotMissing,
    Stage4ReportApplicationMismatch,
    TargetInstrumentMismatch,
    AccountScopeMismatch,
    InstrumentSpecMismatch,
    InvalidInstrumentPriceStep,
    TickSizeMismatch,
    LiveOrdersRequested,
    StrategyIdEmpty,
    EvidenceCheckedInFuture,
    EvidenceExpired,
}

impl std::fmt::Display for Stage5cPaperHostAdmissionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "stage 5C paper host admission blocked: {self:?}")
    }
}

impl std::error::Error for Stage5cPaperHostAdmissionError {}

/// Input accepts one opaque canonical Stage 4 chain, never independent report
/// and application DTOs.
///
/// ```compile_fail
/// # use broker_core::{BrokerAccountId, BrokerInstrumentSpec, InstrumentId, Stage4AcceptedPaperHostEvidence};
/// # use strategy_runtime_core::{admit_stage5c_paper_host, Stage5cPaperHostAdmissionInput};
/// # fn duplicate(evidence: Stage4AcceptedPaperHostEvidence, spec: &BrokerInstrumentSpec, account: &BrokerAccountId, target: &InstrumentId) {
/// let _ = admit_stage5c_paper_host(Stage5cPaperHostAdmissionInput {
///     stage4_evidence: evidence,
///     strategy_id: "hybrid_imoexf".to_string(),
///     instrument_spec: spec,
///     configured_account_id: account,
///     configured_target_instrument: target,
///     configured_tick_size: 0.5,
///     allow_live_orders: false,
/// });
/// let _ = admit_stage5c_paper_host(Stage5cPaperHostAdmissionInput {
///     stage4_evidence: evidence,
///     strategy_id: "hybrid_imoexf".to_string(),
///     instrument_spec: spec,
///     configured_account_id: account,
///     configured_target_instrument: target,
///     configured_tick_size: 0.5,
///     allow_live_orders: false,
/// });
/// # }
/// ```
pub struct Stage5cPaperHostAdmissionInput<'a> {
    pub stage4_evidence: Stage4AcceptedPaperHostEvidence,
    pub strategy_id: String,
    pub instrument_spec: &'a BrokerInstrumentSpec,
    pub configured_account_id: &'a BrokerAccountId,
    pub configured_target_instrument: &'a InstrumentId,
    pub configured_tick_size: f64,
    pub allow_live_orders: bool,
}

/// Opaque paper-host capability issued only by [`admit_stage5c_paper_host`].
///
/// It cannot be reconstructed from serialized evidence.
///
/// ```compile_fail
/// let _: strategy_runtime_core::Stage5cPaperHostAdmission =
///     serde_json::from_str("{}").unwrap();
/// ```
#[derive(PartialEq)]
pub struct Stage5cPaperHostAdmission {
    schema_version: u16,
    checked_ts: DateTime<Utc>,
    issued_ts: DateTime<Utc>,
    expires_at: DateTime<Utc>,
    strategy_id: String,
    account_id: BrokerAccountId,
    target_instrument: InstrumentId,
    tick_size: f64,
    bootstrap_snapshot: RuntimeHostBootstrapSnapshot,
    paper_only: bool,
    runtime_host_attached: bool,
    intent_sink_attached: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage5cBootstrapNotificationError {
    AdmissionExpired,
    StrategyTargetMismatch,
    StrategyTickSizeMismatch,
    ActiveOrdersRequireOwnershipMapping,
    SnapshotAccountMismatch,
    SnapshotInstrumentMismatch,
    PositionQuantityNotRepresentable,
    PositionAveragePriceNotRepresentable,
}

impl std::fmt::Display for Stage5cBootstrapNotificationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "Stage 5C bootstrap notification blocked: {self:?}"
        )
    }
}

impl std::error::Error for Stage5cBootstrapNotificationError {}

/// One-shot proof that only `NotifyBootstrapSnapshot` has completed.
/// Subsequent lifecycle gates must consume this receipt by value.
pub struct Stage5cBootstrapNotificationReceipt {
    admission: Stage5cPaperHostAdmission,
    notified_ts: DateTime<Utc>,
}

impl Stage5cBootstrapNotificationReceipt {
    pub fn notified_ts(&self) -> DateTime<Utc> {
        self.notified_ts
    }

    pub fn expires_at(&self) -> DateTime<Utc> {
        self.admission.expires_at()
    }

    pub fn strategy_id(&self) -> &str {
        self.admission.strategy_id()
    }

    pub fn bootstrap_snapshot(&self) -> &RuntimeHostBootstrapSnapshot {
        self.admission.bootstrap_snapshot()
    }

    pub fn runtime_state_restored(&self) -> bool {
        false
    }

    pub fn warmup_started(&self) -> bool {
        false
    }

    pub fn pending_recovery_started(&self) -> bool {
        false
    }

    pub fn semantic_bar_enabled(&self) -> bool {
        false
    }

    pub fn intent_sink_attached(&self) -> bool {
        false
    }
}

/// Linear type-state after exactly one successful bootstrap notification.
pub struct Stage5cBootstrappedPaperStrategy {
    strategy: HybridIntradayRuntimeStrategy,
    receipt: Stage5cBootstrapNotificationReceipt,
    restored: RuntimeStateRestored,
}

impl Stage5cBootstrappedPaperStrategy {
    pub fn receipt(&self) -> &Stage5cBootstrapNotificationReceipt {
        &self.receipt
    }

    #[cfg(test)]
    fn strategy(&self) -> &HybridIntradayRuntimeStrategy {
        &self.strategy
    }

    pub(crate) fn into_parts(
        self,
    ) -> (
        HybridIntradayRuntimeStrategy,
        Stage5cBootstrapNotificationReceipt,
        RuntimeStateRestored,
    ) {
        (self.strategy, self.receipt, self.restored)
    }
}

pub struct Stage5cRuntimeStateLoadedPaperStrategy {
    strategy: HybridIntradayRuntimeStrategy,
    admission: Stage5cPaperHostAdmission,
    restored: RuntimeStateRestored,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage5cLegacyNumericOrderIdPolicy {
    Reject,
    ConvertPositiveAlorNumeric,
}

/// Persisted-state envelope supplied to the one-shot restore gate.
pub struct Stage5cRuntimeStateRestoreInput {
    pub schema_version: u16,
    pub state_schema_version: u16,
    pub strategy_kind: String,
    pub strategy_id: String,
    pub account_id: BrokerAccountId,
    pub instrument: InstrumentId,
    pub tick_size: f64,
    pub config_fingerprint: String,
    pub profile: String,
    pub mr_variant: String,
    pub mr_gate_policy: String,
    pub risk_gate_mode: String,
    pub persisted_ts: DateTime<Utc>,
    pub state_json: String,
    pub known_order_ids: Vec<BrokerOrderId>,
    pub pending_requests: Vec<StrategyRequestId>,
    pub legacy_numeric_order_id_policy: Stage5cLegacyNumericOrderIdPolicy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage5cRuntimeStateRestoreError {
    SchemaMismatch,
    AdmissionExpired,
    StrategyIdMismatch,
    AccountMismatch,
    InstrumentMismatch,
    TickSizeMismatch,
    StateSchemaMismatch,
    StrategyKindMismatch,
    ConfigFingerprintMismatch,
    ProfileBindingMismatch,
    PersistedStateFromFuture,
    InvalidStateJson,
    WrongStrategyStateKind,
    LegacyNumericOrderIdRejected,
    InvalidLegacyNumericOrderId,
    BrokerTruthPositionMismatch,
    BrokerTruthSideMismatch,
    BrokerOwnedOrderIdMismatch,
}

impl std::fmt::Display for Stage5cRuntimeStateRestoreError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "Stage 5C runtime-state restore blocked: {self:?}"
        )
    }
}

impl std::error::Error for Stage5cRuntimeStateRestoreError {}

pub struct Stage5cRuntimeStateRestoreReceipt {
    bootstrap_receipt: Stage5cBootstrapNotificationReceipt,
    restored_ts: DateTime<Utc>,
    pending_requests: Vec<StrategyRequestId>,
}

impl Stage5cRuntimeStateRestoreReceipt {
    pub fn bootstrap_receipt(&self) -> &Stage5cBootstrapNotificationReceipt {
        &self.bootstrap_receipt
    }

    pub fn restored_ts(&self) -> DateTime<Utc> {
        self.restored_ts
    }

    pub fn runtime_state_restored(&self) -> bool {
        true
    }
    pub fn pending_requests(&self) -> &[StrategyRequestId] {
        &self.pending_requests
    }

    pub fn warmup_started(&self) -> bool {
        false
    }

    pub fn pending_recovery_started(&self) -> bool {
        false
    }

    pub fn semantic_bar_enabled(&self) -> bool {
        false
    }

    pub fn intent_sink_attached(&self) -> bool {
        false
    }
}

/// Linear type-state after exactly one validated state restore.
pub struct Stage5cRuntimeStateRestoredPaperStrategy {
    strategy: HybridIntradayRuntimeStrategy,
    receipt: Stage5cRuntimeStateRestoreReceipt,
}

impl Stage5cRuntimeStateRestoredPaperStrategy {
    pub fn receipt(&self) -> &Stage5cRuntimeStateRestoreReceipt {
        &self.receipt
    }

    #[cfg(test)]
    fn strategy(&self) -> &HybridIntradayRuntimeStrategy {
        &self.strategy
    }

    pub(crate) fn into_parts(
        self,
    ) -> (
        HybridIntradayRuntimeStrategy,
        Stage5cRuntimeStateRestoreReceipt,
    ) {
        (self.strategy, self.receipt)
    }
}

pub struct Stage5cHistoryBatchInput {
    pub bars: Vec<broker_core::HybridRuntimeBarEvent>,
    pub provenance: broker_core::Stage3StrategyBarProvenance,
}

pub struct Stage5cAcceptedHistoryBatch {
    bars: Vec<broker_core::HybridRuntimeBarEvent>,
    provenance: broker_core::Stage3StrategyBarProvenance,
    instrument: InstrumentId,
    start_ts: i64,
    end_ts: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage5cHistoryWarmupError {
    BrokerTruthExpired,
    LifecycleTimestampReversal,
    EmptyHistory,
    InstrumentMismatch,
    InvalidTimeframe,
    NonFinalBar,
    InvalidOrigin,
    NonMonotonicTimestamp,
    UnalignedTimestamp,
    InvalidOhlc,
    InvalidVolume,
    NoEligibleHistoryBars,
    Stage3ProvenanceRejected,
    FutureHistoryBar,
    InvalidHistoryTimestamp,
}

impl std::fmt::Display for Stage5cHistoryWarmupError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "Stage 5C history warmup blocked: {self:?}")
    }
}

impl std::error::Error for Stage5cHistoryWarmupError {}

pub struct Stage5cHistoryWarmupReceipt {
    restore_receipt: Stage5cRuntimeStateRestoreReceipt,
    started_ts: DateTime<Utc>,
    processed_bars: usize,
    input_bars: usize,
    source_mode: broker_core::Stage3StrategyBarSourceMode,
    last_history_ts: i64,
}

impl Stage5cHistoryWarmupReceipt {
    pub fn restore_receipt(&self) -> &Stage5cRuntimeStateRestoreReceipt {
        &self.restore_receipt
    }

    pub fn started_ts(&self) -> DateTime<Utc> {
        self.started_ts
    }

    pub fn processed_bars(&self) -> usize {
        self.processed_bars
    }

    pub fn input_bars(&self) -> usize {
        self.input_bars
    }

    pub fn skipped_bars(&self) -> usize {
        self.input_bars.saturating_sub(self.processed_bars)
    }

    pub fn source_mode(&self) -> broker_core::Stage3StrategyBarSourceMode {
        self.source_mode
    }
    pub fn last_history_ts(&self) -> i64 {
        self.last_history_ts
    }

    pub fn warmup_started(&self) -> bool {
        true
    }

    pub fn pending_recovery_started(&self) -> bool {
        false
    }

    pub fn semantic_bar_enabled(&self) -> bool {
        false
    }

    pub fn intent_sink_attached(&self) -> bool {
        false
    }
}

pub struct Stage5cWarmedPaperStrategy {
    strategy: HybridIntradayRuntimeStrategy,
    receipt: Stage5cHistoryWarmupReceipt,
}

impl Stage5cWarmedPaperStrategy {
    pub fn receipt(&self) -> &Stage5cHistoryWarmupReceipt {
        &self.receipt
    }

    #[cfg(test)]
    fn strategy(&self) -> &HybridIntradayRuntimeStrategy {
        &self.strategy
    }

    pub(crate) fn into_parts(self) -> (HybridIntradayRuntimeStrategy, Stage5cHistoryWarmupReceipt) {
        (self.strategy, self.receipt)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Stage5cPendingRecoveryPayload {
    Ack(broker_core::HybridRuntimeCommandAck),
    Order(broker_core::HybridRuntimeOrderEvent),
    StopOrder(broker_core::HybridRuntimeStopOrderEvent),
    Position(broker_core::HybridRuntimePositionEvent),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Stage5cPendingStreamKind {
    Ack,
    Order,
    StopOrder,
    Position,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Stage5cPendingRecoveryEvent {
    pub stream_kind: Stage5cPendingStreamKind,
    pub stream_name: String,
    pub entry_id: String,
    pub sequence: u64,
    pub payload: Stage5cPendingRecoveryPayload,
}

pub struct Stage5cPendingStreamClaimBoundary {
    pub stream_kind: Stage5cPendingStreamKind,
    pub stream_name: String,
    pub consumer_group: String,
    pub terminal_claim_cursor: String,
    pub snapshot_boundary_entry_id: String,
    pub claimed_count: usize,
}

pub struct Stage5cPendingRecoveryClaimProofInput {
    pub strategy_id: String,
    pub account_id: BrokerAccountId,
    pub target_instrument: InstrumentId,
    pub snapshot_received_ts: DateTime<Utc>,
    pub completed_ts: DateTime<Utc>,
    pub streams: Vec<Stage5cPendingStreamClaimBoundary>,
}

pub struct Stage5cPendingRecoveryClaimProof {
    strategy_id: String,
    account_id: BrokerAccountId,
    target_instrument: InstrumentId,
    snapshot_received_ts: DateTime<Utc>,
    completed_ts: DateTime<Utc>,
    streams: Vec<Stage5cPendingStreamClaimBoundary>,
}

pub struct Stage5cPendingRecoveryEvidenceInput {
    pub events: Vec<Stage5cPendingRecoveryEvent>,
    pub claim_proof: Stage5cPendingRecoveryClaimProof,
}

pub struct Stage5cAcceptedPendingRecoveryEvidence {
    events: Vec<Stage5cPendingRecoveryEvent>,
    duplicate_events: usize,
    claim_proof: Stage5cPendingRecoveryClaimProof,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage5cPendingRecoveryError {
    EvidenceIncomplete,
    InvalidEventIdentity,
    ConflictingDuplicate,
    NonMonotonicSequence,
    BrokerTruthExpired,
    LifecycleTimestampReversal,
    InstrumentMismatch,
    CallbackValidationFailed,
    UnexpectedIntent,
    ClaimScopeMismatch,
    ClaimBoundaryInvalid,
    StreamKindMismatch,
    FutureEvent,
    InvalidEventTimestamp,
    AckNotPending,
}

impl std::fmt::Display for Stage5cPendingRecoveryError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "Stage 5C pending recovery blocked: {self:?}")
    }
}

impl std::error::Error for Stage5cPendingRecoveryError {}

pub struct Stage5cPendingRecoveryReceipt {
    warmup_receipt: Stage5cHistoryWarmupReceipt,
    recovered_ts: DateTime<Utc>,
    replayed_events: usize,
    duplicate_events: usize,
}

impl Stage5cPendingRecoveryReceipt {
    pub fn recovered_ts(&self) -> DateTime<Utc> {
        self.recovered_ts
    }
    pub fn replayed_events(&self) -> usize {
        self.replayed_events
    }
    pub fn duplicate_events(&self) -> usize {
        self.duplicate_events
    }
    pub fn pending_recovery_started(&self) -> bool {
        true
    }
    pub fn semantic_bar_enabled(&self) -> bool {
        false
    }
    pub fn intent_sink_attached(&self) -> bool {
        false
    }
    pub fn warmup_receipt(&self) -> &Stage5cHistoryWarmupReceipt {
        &self.warmup_receipt
    }
}

pub struct Stage5cPendingRecoveredPaperStrategy {
    strategy: HybridIntradayRuntimeStrategy,
    receipt: Stage5cPendingRecoveryReceipt,
}

pub struct Stage5cSemanticBarInput {
    pub bar: broker_core::HybridRuntimeBarEvent,
    pub provenance: broker_core::Stage3StrategyBarProvenance,
    pub tick_size: f64,
}

pub struct Stage5cAcceptedSemanticBar {
    bar: broker_core::HybridRuntimeBarEvent,
    tick_size: f64,
    origin: broker_core::HybridRuntimeBarOrigin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage5cSemanticBarError {
    Stage3Rejected,
    InstrumentMismatch,
    TickSizeMismatch,
    BrokerTruthExpired,
    StaleOrDuplicateBar,
    FutureBar,
    InvalidTimestamp,
    CallbackValidationFailed,
    UnalignedTimestamp,
    InvalidOhlc,
    InvalidVolume,
}

impl std::fmt::Display for Stage5cSemanticBarError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "Stage 5C semantic bar blocked: {self:?}")
    }
}
impl std::error::Error for Stage5cSemanticBarError {}

pub struct Stage5cSemanticBarResult {
    strategy: HybridIntradayRuntimeStrategy,
    recovery_receipt: Stage5cPendingRecoveryReceipt,
    bar_close_ts: i64,
    origin: broker_core::HybridRuntimeBarOrigin,
    execution_eligible: bool,
    intents: Vec<crate::BrokerNeutralHybridIntent>,
}

impl Stage5cSemanticBarResult {
    pub fn bar_close_ts(&self) -> i64 {
        self.bar_close_ts
    }
    pub fn captured_intent_count(&self) -> usize {
        self.intents.len()
    }
    pub fn origin(&self) -> broker_core::HybridRuntimeBarOrigin {
        self.origin
    }
    pub fn execution_eligible(&self) -> bool {
        self.execution_eligible
    }
    pub fn intent_sink_attached(&self) -> bool {
        false
    }
    pub fn broker_transport_attached(&self) -> bool {
        false
    }
    pub fn recovery_receipt(&self) -> &Stage5cPendingRecoveryReceipt {
        &self.recovery_receipt
    }
    pub(crate) fn into_parts(
        self,
    ) -> (
        HybridIntradayRuntimeStrategy,
        Stage5cPendingRecoveryReceipt,
        i64,
        broker_core::HybridRuntimeBarOrigin,
        bool,
        Vec<crate::BrokerNeutralHybridIntent>,
    ) {
        (
            self.strategy,
            self.recovery_receipt,
            self.bar_close_ts,
            self.origin,
            self.execution_eligible,
            self.intents,
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage5cIntentSettlementError {
    TooManyIntents,
    MissingIntentClass,
    InstrumentNamespaceMismatch,
    InvalidQuantity,
    InvalidPrice,
    PriceNotTickAligned,
    InvalidStopEnd,
    ReplayIntentNotExecutable,
    MissingPendingRequest,
    RequestIdMismatch,
    DuplicateRequestId,
    UnsupportedIntentAction,
}

impl std::fmt::Display for Stage5cIntentSettlementError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "Stage 5C intent settlement blocked: {self:?}")
    }
}
impl std::error::Error for Stage5cIntentSettlementError {}

pub struct Stage5cPaperIntentBatch {
    strategy_id: String,
    account_id: BrokerAccountId,
    instrument: InstrumentId,
    bar_close_ts: i64,
    state_fingerprint: String,
    request_ids: Vec<StrategyRequestId>,
    records: Vec<Stage5cPaperIntentRecord>,
    observation_only: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stage5cPaperIntentBatchSummary {
    pub strategy_id: String,
    pub account_id: BrokerAccountId,
    pub instrument: InstrumentId,
    pub bar_close_ts: i64,
    pub state_fingerprint: String,
    pub request_ids: Vec<StrategyRequestId>,
    pub intent_count: usize,
    pub observation_only: bool,
}

#[derive(Clone)]
struct Stage5cPaperIntentRecord {
    request_id: StrategyRequestId,
    intent_class: crate::BrokerNeutralHybridIntentClass,
    intent: crate::BrokerNeutralHybridIntent,
}

impl Stage5cPaperIntentBatch {
    pub fn intent_count(&self) -> usize {
        self.records.len()
    }
    pub fn request_ids(&self) -> &[StrategyRequestId] {
        &self.request_ids
    }
    pub fn record_request_ids(&self) -> Vec<StrategyRequestId> {
        self.records
            .iter()
            .map(|record| record.request_id)
            .collect()
    }
    pub fn intent_classes(&self) -> Vec<crate::BrokerNeutralHybridIntentClass> {
        self.records
            .iter()
            .map(|record| record.intent_class)
            .collect()
    }
    pub fn has_actionable_intents(&self) -> bool {
        self.records.iter().any(|record| {
            !matches!(
                record.intent.base_intent(),
                crate::BrokerNeutralHybridIntent::Cancel { .. }
            )
        })
    }
    pub fn observation_only(&self) -> bool {
        self.observation_only
    }
    pub fn state_fingerprint(&self) -> &str {
        &self.state_fingerprint
    }
    pub fn bar_close_ts(&self) -> i64 {
        self.bar_close_ts
    }
    pub fn strategy_id(&self) -> &str {
        &self.strategy_id
    }
    pub fn account_id(&self) -> &BrokerAccountId {
        &self.account_id
    }
    pub fn instrument(&self) -> &InstrumentId {
        &self.instrument
    }
}

pub struct Stage5cSettledPaperStrategy {
    strategy: HybridIntradayRuntimeStrategy,
    recovery_receipt: Stage5cPendingRecoveryReceipt,
    batch: Stage5cPaperIntentBatch,
    settled_batch_history: Vec<Stage5cPaperIntentBatchSummary>,
}

impl std::fmt::Debug for Stage5cSettledPaperStrategy {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Stage5cSettledPaperStrategy")
            .field("bar_close_ts", &self.batch.bar_close_ts())
            .field("intent_count", &self.batch.intent_count())
            .field(
                "settled_batch_history_len",
                &self.settled_batch_history.len(),
            )
            .field("intent_sink_attached", &false)
            .field("broker_transport_attached", &false)
            .finish_non_exhaustive()
    }
}

impl Stage5cSettledPaperStrategy {
    pub fn intent_batch(&self) -> &Stage5cPaperIntentBatch {
        &self.batch
    }
    pub fn intent_sink_attached(&self) -> bool {
        false
    }
    pub fn broker_transport_attached(&self) -> bool {
        false
    }
    pub fn recovery_receipt(&self) -> &Stage5cPendingRecoveryReceipt {
        &self.recovery_receipt
    }
    pub fn settled_batch_history(&self) -> &[Stage5cPaperIntentBatchSummary] {
        &self.settled_batch_history
    }
    pub fn timer_path_enabled(&self) -> bool {
        false
    }
    #[cfg(test)]
    fn strategy(&self) -> &HybridIntradayRuntimeStrategy {
        &self.strategy
    }
    pub(crate) fn into_parts(
        self,
    ) -> (
        HybridIntradayRuntimeStrategy,
        Stage5cPendingRecoveryReceipt,
        Stage5cPaperIntentBatch,
    ) {
        (self.strategy, self.recovery_receipt, self.batch)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage5cNextBarLoopError {
    NonMonotonicBar,
    UnresolvedIntentBatch,
    Semantic(Stage5cSemanticBarError),
    Settlement(Stage5cIntentSettlementError),
}

impl std::fmt::Display for Stage5cNextBarLoopError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "Stage 5C controlled next-bar loop blocked: {self:?}"
        )
    }
}

impl std::error::Error for Stage5cNextBarLoopError {}

pub struct Stage5cNextBarBlocked {
    reason: Stage5cNextBarLoopError,
    settled: Stage5cSettledPaperStrategy,
}

impl Stage5cNextBarBlocked {
    pub fn reason(&self) -> Stage5cNextBarLoopError {
        self.reason
    }
    pub fn settled(&self) -> &Stage5cSettledPaperStrategy {
        &self.settled
    }
    pub fn into_settled(self) -> Stage5cSettledPaperStrategy {
        self.settled
    }
}

impl std::fmt::Debug for Stage5cNextBarBlocked {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Stage5cNextBarBlocked")
            .field("reason", &self.reason)
            .field(
                "previous_bar_close_ts",
                &self.settled.intent_batch().bar_close_ts(),
            )
            .field(
                "previous_intent_count",
                &self.settled.intent_batch().intent_count(),
            )
            .finish_non_exhaustive()
    }
}

impl std::fmt::Display for Stage5cNextBarBlocked {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "Stage 5C next bar blocked: {:?}", self.reason)
    }
}

impl std::error::Error for Stage5cNextBarBlocked {}

#[derive(Debug)]
pub enum Stage5cNextBarLoopFailure {
    Blocked(Box<Stage5cNextBarBlocked>),
    Failed(Stage5cNextBarLoopError),
}

impl Stage5cNextBarLoopFailure {
    pub fn reason(&self) -> Stage5cNextBarLoopError {
        match self {
            Self::Blocked(blocked) => blocked.reason(),
            Self::Failed(reason) => *reason,
        }
    }
    pub fn into_blocked(self) -> Option<Stage5cNextBarBlocked> {
        match self {
            Self::Blocked(blocked) => Some(*blocked),
            Self::Failed(_) => None,
        }
    }
}

impl std::fmt::Display for Stage5cNextBarLoopFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "Stage 5C next bar failed: {:?}", self.reason())
    }
}

impl std::error::Error for Stage5cNextBarLoopFailure {}

#[derive(Debug, Clone)]
pub struct Stage5cPaperIntentLifecycleInput {
    pub ack_records: Vec<Stage5cPaperAckRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stage5cPaperAckRecord {
    pub total_sequence: u64,
    pub ack: broker_core::HybridRuntimeCommandAck,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stage5cPaperAckOutcome {
    pub total_sequence: u64,
    pub request_id: StrategyRequestId,
    pub status: broker_core::HybridRuntimeAckStatus,
    pub broker_order_id: Option<BrokerOrderId>,
    pub error_code: Option<broker_core::HybridRuntimeAckErrorCode>,
    pub processed_ts_utc: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage5cPaperBrokerEventKind {
    Order,
    StopOrder,
    Position,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Stage5cPaperBrokerEventPayload {
    Order(broker_core::HybridRuntimeOrderEvent),
    StopOrder(broker_core::HybridRuntimeStopOrderEvent),
    Position(broker_core::HybridRuntimePositionEvent),
}

impl Stage5cPaperBrokerEventPayload {
    fn kind(&self) -> Stage5cPaperBrokerEventKind {
        match self {
            Self::Order(_) => Stage5cPaperBrokerEventKind::Order,
            Self::StopOrder(_) => Stage5cPaperBrokerEventKind::StopOrder,
            Self::Position(_) => Stage5cPaperBrokerEventKind::Position,
        }
    }
    fn instrument(&self) -> &InstrumentId {
        match self {
            Self::Order(value) => &value.instrument,
            Self::StopOrder(value) => &value.instrument,
            Self::Position(value) => &value.instrument,
        }
    }
    fn source_ts_utc(&self) -> i64 {
        match self {
            Self::Order(value) => value.source_ts_utc,
            Self::StopOrder(value) => value.source_ts_utc,
            Self::Position(value) => value.source_ts_utc,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Stage5cPaperBrokerEventRecord {
    pub total_sequence: u64,
    pub request_id: StrategyRequestId,
    pub payload: Stage5cPaperBrokerEventPayload,
}

#[derive(Debug, Clone)]
pub struct Stage5cPaperBrokerLifecycleInput {
    pub event_records: Vec<Stage5cPaperBrokerEventRecord>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage5cPaperIntentLifecycleError {
    EmptyIntentBatch,
    StateFingerprintMismatch,
    MissingAck,
    DuplicateAck,
    UnknownAckRequestId,
    DuplicateSequence,
    NonMonotonicSequence,
    AckTimestampBeforeIntentBar,
    CallbackValidationFailed,
    CallbackGeneratedIntentTerminal,
}

impl std::fmt::Display for Stage5cPaperIntentLifecycleError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "Stage 5C paper intent lifecycle blocked: {self:?}"
        )
    }
}

impl std::error::Error for Stage5cPaperIntentLifecycleError {}

pub struct Stage5cPaperIntentLifecycleBlocked {
    reason: Stage5cPaperIntentLifecycleError,
    settled: Stage5cSettledPaperStrategy,
}

impl Stage5cPaperIntentLifecycleBlocked {
    pub fn reason(&self) -> Stage5cPaperIntentLifecycleError {
        self.reason
    }
    pub fn settled(&self) -> &Stage5cSettledPaperStrategy {
        &self.settled
    }
    pub fn into_settled(self) -> Stage5cSettledPaperStrategy {
        self.settled
    }
}

impl std::fmt::Debug for Stage5cPaperIntentLifecycleBlocked {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Stage5cPaperIntentLifecycleBlocked")
            .field("reason", &self.reason)
            .field("bar_close_ts", &self.settled.intent_batch().bar_close_ts())
            .field("intent_count", &self.settled.intent_batch().intent_count())
            .finish_non_exhaustive()
    }
}

#[derive(Debug)]
pub enum Stage5cPaperIntentLifecycleFailure {
    Blocked(Box<Stage5cPaperIntentLifecycleBlocked>),
    Terminal(Stage5cPaperIntentLifecycleError),
}

impl Stage5cPaperIntentLifecycleFailure {
    pub fn reason(&self) -> Stage5cPaperIntentLifecycleError {
        match self {
            Self::Blocked(blocked) => blocked.reason(),
            Self::Terminal(reason) => *reason,
        }
    }
    pub fn into_blocked(self) -> Option<Stage5cPaperIntentLifecycleBlocked> {
        match self {
            Self::Blocked(blocked) => Some(*blocked),
            Self::Terminal(_) => None,
        }
    }
}

impl std::fmt::Display for Stage5cPaperIntentLifecycleFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "Stage 5C paper intent lifecycle failed: {:?}",
            self.reason()
        )
    }
}

impl std::error::Error for Stage5cPaperIntentLifecycleFailure {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage5cPaperBrokerLifecycleError {
    DuplicateSequence,
    UnknownEventRequestId,
    DuplicateEvent,
    ConflictingDuplicateEvent,
    EventForTerminalAck,
    MissingExpectedBrokerEvent,
    UnexpectedBrokerEventKind,
    EventTimestampBeforeAck,
    InstrumentMismatch,
    OrderRequestIdMismatch,
    BrokerOrderIdMismatch,
    StopOrderIdMismatch,
    PositionEventRequiresMarketIntent,
    CallbackValidationFailed,
    CallbackGeneratedIntentTerminal,
}

impl std::fmt::Display for Stage5cPaperBrokerLifecycleError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "Stage 5C paper broker lifecycle blocked: {self:?}"
        )
    }
}

impl std::error::Error for Stage5cPaperBrokerLifecycleError {}

pub struct Stage5cPaperBrokerLifecycleBlocked {
    reason: Stage5cPaperBrokerLifecycleError,
    resolved: Stage5cResolvedPaperIntentBatchStrategy,
}

impl Stage5cPaperBrokerLifecycleBlocked {
    pub fn reason(&self) -> Stage5cPaperBrokerLifecycleError {
        self.reason
    }
    pub fn resolved(&self) -> &Stage5cResolvedPaperIntentBatchStrategy {
        &self.resolved
    }
    pub fn into_resolved(self) -> Stage5cResolvedPaperIntentBatchStrategy {
        self.resolved
    }
}

impl std::fmt::Debug for Stage5cPaperBrokerLifecycleBlocked {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Stage5cPaperBrokerLifecycleBlocked")
            .field("reason", &self.reason)
            .field(
                "resolved_bar_close_ts",
                &self.resolved.resolved_batch.bar_close_ts(),
            )
            .field(
                "resolved_intent_count",
                &self.resolved.resolved_batch.intent_count(),
            )
            .finish_non_exhaustive()
    }
}

#[derive(Debug)]
pub enum Stage5cPaperBrokerLifecycleFailure {
    Blocked(Box<Stage5cPaperBrokerLifecycleBlocked>),
    Terminal(Stage5cPaperBrokerLifecycleError),
}

impl Stage5cPaperBrokerLifecycleFailure {
    pub fn reason(&self) -> Stage5cPaperBrokerLifecycleError {
        match self {
            Self::Blocked(blocked) => blocked.reason(),
            Self::Terminal(reason) => *reason,
        }
    }
    pub fn into_blocked(self) -> Option<Stage5cPaperBrokerLifecycleBlocked> {
        match self {
            Self::Blocked(blocked) => Some(*blocked),
            Self::Terminal(_) => None,
        }
    }
}

impl std::fmt::Display for Stage5cPaperBrokerLifecycleFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "Stage 5C paper broker lifecycle failed: {:?}",
            self.reason()
        )
    }
}

impl std::error::Error for Stage5cPaperBrokerLifecycleFailure {}

pub struct Stage5cResolvedPaperIntentBatchStrategy {
    strategy: HybridIntradayRuntimeStrategy,
    recovery_receipt: Stage5cPendingRecoveryReceipt,
    resolved_batch: Stage5cPaperIntentBatch,
    ack_outcomes: Vec<Stage5cPaperAckOutcome>,
    settled_batch_history: Vec<Stage5cPaperIntentBatchSummary>,
}

impl std::fmt::Debug for Stage5cResolvedPaperIntentBatchStrategy {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Stage5cResolvedPaperIntentBatchStrategy")
            .field("resolved_bar_close_ts", &self.resolved_batch.bar_close_ts())
            .field("resolved_intent_count", &self.resolved_batch.intent_count())
            .field("ack_outcome_count", &self.ack_outcomes.len())
            .field(
                "settled_batch_history_len",
                &self.settled_batch_history.len(),
            )
            .field("intent_sink_attached", &false)
            .field("broker_transport_attached", &false)
            .finish_non_exhaustive()
    }
}

impl Stage5cResolvedPaperIntentBatchStrategy {
    pub fn resolved_batch_summary(&self) -> Stage5cPaperIntentBatchSummary {
        stage5ch_batch_summary(&self.resolved_batch)
    }
    pub fn ack_outcomes(&self) -> &[Stage5cPaperAckOutcome] {
        &self.ack_outcomes
    }
    #[cfg(test)]
    fn full_resolved_batch(&self) -> &Stage5cPaperIntentBatch {
        &self.resolved_batch
    }
    pub fn settled_batch_history(&self) -> &[Stage5cPaperIntentBatchSummary] {
        &self.settled_batch_history
    }
    pub fn intent_sink_attached(&self) -> bool {
        false
    }
    pub fn broker_transport_attached(&self) -> bool {
        false
    }
    pub fn timer_path_enabled(&self) -> bool {
        false
    }
    pub fn recovery_receipt(&self) -> &Stage5cPendingRecoveryReceipt {
        &self.recovery_receipt
    }
    pub fn post_lifecycle_state_fingerprint(&self) -> String {
        stage5c_state_fingerprint(Strategy::state(&self.strategy))
    }
    #[cfg(test)]
    fn strategy(&self) -> &HybridIntradayRuntimeStrategy {
        &self.strategy
    }
}

pub struct Stage5cBrokerLifecycleResolvedPaperStrategy {
    strategy: HybridIntradayRuntimeStrategy,
    recovery_receipt: Stage5cPendingRecoveryReceipt,
    resolved_batch_summary: Stage5cPaperIntentBatchSummary,
    ack_outcomes: Vec<Stage5cPaperAckOutcome>,
    broker_event_count: usize,
    settled_batch_history: Vec<Stage5cPaperIntentBatchSummary>,
}

impl std::fmt::Debug for Stage5cBrokerLifecycleResolvedPaperStrategy {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Stage5cBrokerLifecycleResolvedPaperStrategy")
            .field(
                "resolved_bar_close_ts",
                &self.resolved_batch_summary.bar_close_ts,
            )
            .field("broker_event_count", &self.broker_event_count)
            .field("intent_sink_attached", &false)
            .field("broker_transport_attached", &false)
            .finish_non_exhaustive()
    }
}

impl Stage5cBrokerLifecycleResolvedPaperStrategy {
    pub fn resolved_batch_summary(&self) -> &Stage5cPaperIntentBatchSummary {
        &self.resolved_batch_summary
    }
    pub fn ack_outcomes(&self) -> &[Stage5cPaperAckOutcome] {
        &self.ack_outcomes
    }
    pub fn broker_event_count(&self) -> usize {
        self.broker_event_count
    }
    pub fn settled_batch_history(&self) -> &[Stage5cPaperIntentBatchSummary] {
        &self.settled_batch_history
    }
    pub fn intent_sink_attached(&self) -> bool {
        false
    }
    pub fn broker_transport_attached(&self) -> bool {
        false
    }
    pub fn timer_path_enabled(&self) -> bool {
        false
    }
    pub fn recovery_receipt(&self) -> &Stage5cPendingRecoveryReceipt {
        &self.recovery_receipt
    }
    pub fn post_broker_lifecycle_state_fingerprint(&self) -> String {
        stage5c_state_fingerprint(Strategy::state(&self.strategy))
    }
    #[cfg(test)]
    fn strategy(&self) -> &HybridIntradayRuntimeStrategy {
        &self.strategy
    }
}

impl Stage5cPendingRecoveredPaperStrategy {
    pub fn receipt(&self) -> &Stage5cPendingRecoveryReceipt {
        &self.receipt
    }
    #[cfg(test)]
    fn strategy(&self) -> &HybridIntradayRuntimeStrategy {
        &self.strategy
    }

    pub(crate) fn into_parts(
        self,
    ) -> (HybridIntradayRuntimeStrategy, Stage5cPendingRecoveryReceipt) {
        (self.strategy, self.receipt)
    }
}

impl Stage5cPaperHostAdmission {
    pub fn schema_version(&self) -> u16 {
        self.schema_version
    }

    pub fn checked_ts(&self) -> DateTime<Utc> {
        self.checked_ts
    }

    pub fn issued_ts(&self) -> DateTime<Utc> {
        self.issued_ts
    }

    pub fn expires_at(&self) -> DateTime<Utc> {
        self.expires_at
    }

    pub fn account_id(&self) -> &BrokerAccountId {
        &self.account_id
    }

    pub fn strategy_id(&self) -> &str {
        &self.strategy_id
    }

    pub fn target_instrument(&self) -> &InstrumentId {
        &self.target_instrument
    }

    pub fn tick_size(&self) -> f64 {
        self.tick_size
    }

    pub fn bootstrap_snapshot(&self) -> &RuntimeHostBootstrapSnapshot {
        &self.bootstrap_snapshot
    }

    pub fn is_paper_only(&self) -> bool {
        self.paper_only
    }

    pub fn runtime_host_attached(&self) -> bool {
        self.runtime_host_attached
    }

    pub fn intent_sink_attached(&self) -> bool {
        self.intent_sink_attached
    }
}

pub fn admit_stage5c_paper_host(
    input: Stage5cPaperHostAdmissionInput<'_>,
) -> Result<Stage5cPaperHostAdmission, Stage5cPaperHostAdmissionError> {
    admit_stage5c_paper_host_at(input, Utc::now())
}

pub(crate) fn admit_stage5c_paper_host_at(
    input: Stage5cPaperHostAdmissionInput<'_>,
    admission_now: DateTime<Utc>,
) -> Result<Stage5cPaperHostAdmission, Stage5cPaperHostAdmissionError> {
    if input.strategy_id.trim().is_empty() {
        return Err(Stage5cPaperHostAdmissionError::StrategyIdEmpty);
    }
    let report = input.stage4_evidence.report();
    if report.checked_ts > admission_now {
        return Err(Stage5cPaperHostAdmissionError::EvidenceCheckedInFuture);
    }
    if admission_now > input.stage4_evidence.required_source_expires_at() {
        return Err(Stage5cPaperHostAdmissionError::EvidenceExpired);
    }
    if report.schema_version != STAGE4_BOOTSTRAP_EVIDENCE_REPORT_SCHEMA_VERSION {
        return Err(Stage5cPaperHostAdmissionError::Stage4ReportSchemaMismatch);
    }
    if report.status != Stage4BootstrapEvidenceReportStatus::Accepted {
        return Err(Stage5cPaperHostAdmissionError::Stage4ReportNotAccepted);
    }
    let expected_events = [
        Stage4RuntimeBootstrapIntegrationEvent::NotifyBootstrapSnapshot,
        Stage4RuntimeBootstrapIntegrationEvent::NotifyRuntimeStateRestored,
        Stage4RuntimeBootstrapIntegrationEvent::WarmupHistory,
        Stage4RuntimeBootstrapIntegrationEvent::RecoverPendingStreams,
    ];
    if report.stage4c_status != Stage4BrokerTruthBootstrapStatus::BootstrapReady
        || report.broker_truth_source_status != Stage4BrokerTruthSourceStatus::Present
        || !report.stage4c_blocker_kinds.is_empty()
        || report.stage4e_status != Stage4RuntimeBootstrapApplicationStatus::Applied
        || !report.stage4e_blocker_kinds.is_empty()
        || report.stage4f_status != Stage4DirtyStartPolicyStatus::Accepted
        || !report.stage4f_blocker_kinds.is_empty()
        || report.stage4g_status != Stage4RuntimeLifecycleOrderingStatus::Accepted
        || !report.stage4g_blocker_kinds.is_empty()
        || !report.stage4g_lifecycle_issues.is_empty()
        || report.stage4h_status != Stage4RuntimeBootstrapIntegrationStatus::Accepted
        || !report.stage4h_blocker_kinds.is_empty()
        || !report.reason_chain.is_empty()
        || report.blocker_count != 0
        || report.manual_intervention_required
        || !report.runtime_events_emitted
        || report.mock_runtime_events != expected_events
    {
        return Err(Stage5cPaperHostAdmissionError::Stage4EvidenceChainInconsistent);
    }
    let expected_source_sections = HashSet::from([
        Stage4BrokerTruthFreshnessSection::Positions,
        Stage4BrokerTruthFreshnessSection::Orders,
        Stage4BrokerTruthFreshnessSection::Trades,
        Stage4BrokerTruthFreshnessSection::Cash,
        Stage4BrokerTruthFreshnessSection::Instruments,
        Stage4BrokerTruthFreshnessSection::Schedule,
    ]);
    let actual_source_sections = report
        .source_sections
        .iter()
        .map(|section| section.section)
        .collect::<HashSet<_>>();
    if report.source_sections.len() != expected_source_sections.len()
        || actual_source_sections != expected_source_sections
        || report.source_sections.iter().any(|section| {
            section.blocks_bootstrap
                || (section.required_for_bootstrap
                    && (section.source_status != Stage4BrokerTruthSourceStatus::Present
                        || section.freshness_status != Stage4BrokerTruthFreshnessStatus::Fresh))
        })
    {
        return Err(Stage5cPaperHostAdmissionError::Stage4EvidenceChainInconsistent);
    }
    if !report.no_live_authorization
        || report.safety_boundary.runtime_live_enabled
        || report.safety_boundary.real_finam_command_consumer_enabled
        || report.safety_boundary.strategy_driven_real_orders_enabled
        || report.safety_boundary.real_post_delete_enabled
        || report.safety_boundary.stop_sltp_bracket_enabled
        || report.safety_boundary.raw_payload_exported
        || !report.redaction.report_redacted
        || report.redaction.raw_payloads_exported
        || report.redaction.secrets_exported
        || report.redaction.account_sensitive_dumps_exported
        || report.redaction.broker_account_id_exported
        || report.redaction.raw_order_comments_exported
    {
        return Err(Stage5cPaperHostAdmissionError::Stage4SafetyBoundaryOpen);
    }

    let application = input.stage4_evidence.application();
    if application.schema_version != STAGE4_RUNTIME_BOOTSTRAP_APPLICATION_SCHEMA_VERSION {
        return Err(Stage5cPaperHostAdmissionError::Stage4ApplicationSchemaMismatch);
    }
    if application.status != Stage4RuntimeBootstrapApplicationStatus::Applied {
        return Err(Stage5cPaperHostAdmissionError::Stage4ApplicationNotApplied);
    }
    if application.source_bootstrap_status != Stage4BrokerTruthBootstrapStatus::BootstrapReady
        || !application.blockers.is_empty()
        || application.blocker_count != 0
        || !application.broker_truth_loaded_before_runtime_state
        || application.restored_runtime_state_accepted_after_broker_truth
            != application.restored_runtime_state_present
        || application.restored_runtime_overrode_broker_truth
        || !application.no_live_authorization
    {
        return Err(Stage5cPaperHostAdmissionError::Stage4ApplicationInconsistent);
    }
    let snapshot = input.stage4_evidence.applied_snapshot();
    if application.applied_snapshot.as_ref() != Some(snapshot) {
        return Err(Stage5cPaperHostAdmissionError::Stage4ApplicationSnapshotMissing);
    }
    if application.checked_ts != report.checked_ts
        || application.target_position_qty != snapshot.target_position_qty
        || application.target_is_flat != snapshot.target_is_flat
        || application.target_active_order_count != snapshot.target_active_orders.len()
        || application.account_active_order_count != snapshot.account_active_orders_count
        || report.target_is_flat != snapshot.target_is_flat
        || report.target_active_order_count != snapshot.target_active_orders.len()
        || report.account_active_order_count != snapshot.account_active_orders_count
    {
        return Err(Stage5cPaperHostAdmissionError::Stage4ReportApplicationMismatch);
    }

    if report.target_instrument != snapshot.instrument
        || report.target_instrument != *input.configured_target_instrument
    {
        return Err(Stage5cPaperHostAdmissionError::TargetInstrumentMismatch);
    }
    if snapshot.account_id != *input.configured_account_id {
        return Err(Stage5cPaperHostAdmissionError::AccountScopeMismatch);
    }
    if input.instrument_spec.instrument_id() != report.target_instrument {
        return Err(Stage5cPaperHostAdmissionError::InstrumentSpecMismatch);
    }
    let spec_tick_size = input
        .instrument_spec
        .instrument
        .price_step
        .to_f64()
        .filter(|value| value.is_finite() && *value > 0.0)
        .ok_or(Stage5cPaperHostAdmissionError::InvalidInstrumentPriceStep)?;
    if !input.configured_tick_size.is_finite()
        || input.configured_tick_size <= 0.0
        || (input.configured_tick_size - spec_tick_size).abs() > f64::EPSILON
    {
        return Err(Stage5cPaperHostAdmissionError::TickSizeMismatch);
    }
    if input.allow_live_orders {
        return Err(Stage5cPaperHostAdmissionError::LiveOrdersRequested);
    }

    Ok(Stage5cPaperHostAdmission {
        schema_version: STAGE5C_PAPER_HOST_ADMISSION_SCHEMA_VERSION,
        checked_ts: report.checked_ts,
        issued_ts: admission_now,
        expires_at: input.stage4_evidence.required_source_expires_at(),
        strategy_id: input.strategy_id,
        account_id: snapshot.account_id.clone(),
        target_instrument: report.target_instrument.clone(),
        tick_size: spec_tick_size,
        bootstrap_snapshot: snapshot.clone(),
        paper_only: true,
        runtime_host_attached: false,
        intent_sink_attached: false,
    })
}

pub fn prepare_stage5c_without_runtime_state(
    strategy: HybridIntradayRuntimeStrategy,
    admission: Stage5cPaperHostAdmission,
) -> Stage5cRuntimeStateLoadedPaperStrategy {
    Stage5cRuntimeStateLoadedPaperStrategy {
        strategy,
        admission,
        restored: RuntimeStateRestored {
            known_order_ids: Vec::new(),
            pending_requests: Vec::new(),
        },
    }
}

/// Consumes the state-loaded type-state, preventing duplicate notification.
///
/// ```compile_fail
/// # use strategy_runtime_core::{notify_stage5c_bootstrap, Stage5cRuntimeStateLoadedPaperStrategy};
/// # fn duplicate(loaded: Stage5cRuntimeStateLoadedPaperStrategy) {
/// let _ = notify_stage5c_bootstrap(loaded);
/// let _ = notify_stage5c_bootstrap(loaded);
/// # }
/// ```
pub fn notify_stage5c_bootstrap(
    loaded: Stage5cRuntimeStateLoadedPaperStrategy,
) -> Result<Stage5cBootstrappedPaperStrategy, Stage5cBootstrapNotificationError> {
    notify_stage5c_bootstrap_at(loaded, Utc::now())
}

pub(crate) fn notify_stage5c_bootstrap_at(
    loaded: Stage5cRuntimeStateLoadedPaperStrategy,
    notification_now: DateTime<Utc>,
) -> Result<Stage5cBootstrappedPaperStrategy, Stage5cBootstrapNotificationError> {
    let Stage5cRuntimeStateLoadedPaperStrategy {
        mut strategy,
        admission,
        restored,
    } = loaded;
    validate_stage5cb_notification(&strategy, &admission, notification_now)?;
    let snapshot = admission.bootstrap_snapshot();
    let position_qty = snapshot
        .target_position_qty
        .to_f64()
        .filter(|value| value.is_finite())
        .ok_or(Stage5cBootstrapNotificationError::PositionQuantityNotRepresentable)?;
    let average_price = snapshot
        .target_open_positions
        .first()
        .and_then(|position| position.avg_price)
        .map(|price| {
            price
                .to_f64()
                .filter(|value| value.is_finite())
                .ok_or(Stage5cBootstrapNotificationError::PositionAveragePriceNotRepresentable)
        })
        .transpose()?
        .unwrap_or_default();
    let mut positions_strategy = HashMap::new();
    if !snapshot.target_open_positions.is_empty() || position_qty.abs() > f64::EPSILON {
        positions_strategy.insert(
            snapshot.instrument.symbol.clone(),
            PositionEvent {
                symbol: snapshot.instrument.symbol.clone(),
                qty: position_qty,
                existing: true,
                avg_price: average_price,
                ts_utc: snapshot.received_ts.timestamp(),
            },
        );
    }
    let source_snapshot = BootstrapSnapshot {
        positions_strategy,
        working_orders_strategy: HashMap::new(),
        working_stop_orders_strategy: HashMap::new(),
        snapshot_ts_utc: Some(snapshot.received_ts.timestamp()),
    };
    let context = StrategyCtx {
        strategy_id: admission.strategy_id().to_string(),
        portfolio: admission.account_id().as_str().to_string(),
        exchange: format!("{:?}", admission.target_instrument().exchange),
        symbol: admission.target_instrument().symbol.clone(),
        tick_size: admission.tick_size(),
        trade_mode: TradeMode::Paper,
        paper_execution_mode: PaperExecutionMode::LiveOnly,
        allow_live_orders: false,
        gateway_phase: GatewayPhase::SyncingHistory,
        position_qty: Some(position_qty),
        event_ts_utc: snapshot.received_ts.timestamp(),
        now_ts_utc: notification_now.timestamp(),
        last_bar_ts: None,
    };
    let intents = Strategy::on_bootstrap_snapshot(&mut strategy, &context, &source_snapshot);
    debug_assert!(
        intents.is_empty(),
        "accepted source bootstrap callback must not emit intents"
    );

    Ok(Stage5cBootstrappedPaperStrategy {
        strategy,
        receipt: Stage5cBootstrapNotificationReceipt {
            admission,
            notified_ts: notification_now,
        },
        restored,
    })
}

fn validate_stage5cb_notification(
    strategy: &HybridIntradayRuntimeStrategy,
    admission: &Stage5cPaperHostAdmission,
    notification_now: DateTime<Utc>,
) -> Result<(), Stage5cBootstrapNotificationError> {
    if notification_now > admission.expires_at() {
        return Err(Stage5cBootstrapNotificationError::AdmissionExpired);
    }
    let (symbol_matches, tick_size_matches) =
        strategy.stage5c_binding_matches(admission.target_instrument(), admission.tick_size());
    if !symbol_matches {
        return Err(Stage5cBootstrapNotificationError::StrategyTargetMismatch);
    }
    if !tick_size_matches {
        return Err(Stage5cBootstrapNotificationError::StrategyTickSizeMismatch);
    }
    let snapshot = admission.bootstrap_snapshot();
    if !snapshot.target_active_orders.is_empty() {
        return Err(Stage5cBootstrapNotificationError::ActiveOrdersRequireOwnershipMapping);
    }
    if snapshot.account_id != *admission.account_id() {
        return Err(Stage5cBootstrapNotificationError::SnapshotAccountMismatch);
    }
    if snapshot.instrument != *admission.target_instrument() {
        return Err(Stage5cBootstrapNotificationError::SnapshotInstrumentMismatch);
    }
    if snapshot.target_open_positions.iter().any(|position| {
        position.account_id != snapshot.account_id || position.instrument != snapshot.instrument
    }) {
        return Err(Stage5cBootstrapNotificationError::SnapshotInstrumentMismatch);
    }
    Ok(())
}

/// Validates and loads persisted semantic state before broker truth bootstrap.
pub fn restore_stage5c_runtime_state(
    strategy: HybridIntradayRuntimeStrategy,
    admission: Stage5cPaperHostAdmission,
    input: Stage5cRuntimeStateRestoreInput,
) -> Result<Stage5cRuntimeStateLoadedPaperStrategy, Stage5cRuntimeStateRestoreError> {
    restore_stage5c_runtime_state_at(strategy, admission, input, Utc::now())
}

fn restore_stage5c_runtime_state_at(
    mut strategy: HybridIntradayRuntimeStrategy,
    admission: Stage5cPaperHostAdmission,
    input: Stage5cRuntimeStateRestoreInput,
    restored_ts: DateTime<Utc>,
) -> Result<Stage5cRuntimeStateLoadedPaperStrategy, Stage5cRuntimeStateRestoreError> {
    if input.schema_version != STAGE5C_RUNTIME_STATE_RESTORE_SCHEMA_VERSION {
        return Err(Stage5cRuntimeStateRestoreError::SchemaMismatch);
    }
    if input.state_schema_version != 1 {
        return Err(Stage5cRuntimeStateRestoreError::StateSchemaMismatch);
    }
    if input.strategy_kind != "hybrid_intraday_runtime" {
        return Err(Stage5cRuntimeStateRestoreError::StrategyKindMismatch);
    }
    if restored_ts > admission.expires_at() {
        return Err(Stage5cRuntimeStateRestoreError::AdmissionExpired);
    }
    if input.persisted_ts > restored_ts {
        return Err(Stage5cRuntimeStateRestoreError::PersistedStateFromFuture);
    }
    if input.strategy_id != admission.strategy_id() {
        return Err(Stage5cRuntimeStateRestoreError::StrategyIdMismatch);
    }
    if input.account_id != *admission.account_id() {
        return Err(Stage5cRuntimeStateRestoreError::AccountMismatch);
    }
    if input.instrument != *admission.target_instrument() {
        return Err(Stage5cRuntimeStateRestoreError::InstrumentMismatch);
    }
    if !same_tick_size(input.tick_size, admission.tick_size()) {
        return Err(Stage5cRuntimeStateRestoreError::TickSizeMismatch);
    }
    if input.config_fingerprint != strategy.stage5c_config_fingerprint() {
        return Err(Stage5cRuntimeStateRestoreError::ConfigFingerprintMismatch);
    }
    let profile = strategy.stage5c_profile_binding();
    if (
        input.profile,
        input.mr_variant,
        input.mr_gate_policy,
        input.risk_gate_mode,
    ) != profile
    {
        return Err(Stage5cRuntimeStateRestoreError::ProfileBindingMismatch);
    }

    let mut raw_state: serde_json::Value = serde_json::from_str(&input.state_json)
        .map_err(|_| Stage5cRuntimeStateRestoreError::InvalidStateJson)?;
    normalize_legacy_order_ids(&mut raw_state, input.legacy_numeric_order_id_policy)?;
    let restored_state: StrategyState = serde_json::from_value(raw_state)
        .map_err(|_| Stage5cRuntimeStateRestoreError::InvalidStateJson)?;
    let (restored_position_qty, restored_side) = match &restored_state {
        StrategyState::HybridIntradayRuntime {
            last_position_qty,
            current_side,
            ..
        } => (*last_position_qty, *current_side),
        StrategyState::Idle => {
            return Err(Stage5cRuntimeStateRestoreError::WrongStrategyStateKind);
        }
    };
    let broker_position_qty = admission
        .bootstrap_snapshot()
        .target_position_qty
        .to_f64()
        .ok_or(Stage5cRuntimeStateRestoreError::BrokerTruthPositionMismatch)?;
    if (restored_position_qty - broker_position_qty).abs() > f64::EPSILON {
        return Err(Stage5cRuntimeStateRestoreError::BrokerTruthPositionMismatch);
    }
    let expected_side = if broker_position_qty > f64::EPSILON {
        Some(crate::hybrid_intraday::Side::Long)
    } else if broker_position_qty < -f64::EPSILON {
        Some(crate::hybrid_intraday::Side::Short)
    } else {
        None
    };
    if expected_side.is_some() && restored_side.is_some() && restored_side != expected_side {
        return Err(Stage5cRuntimeStateRestoreError::BrokerTruthSideMismatch);
    }

    Strategy::set_state(&mut strategy, restored_state);
    Ok(Stage5cRuntimeStateLoadedPaperStrategy {
        strategy,
        admission,
        restored: RuntimeStateRestored {
            known_order_ids: input.known_order_ids,
            pending_requests: input.pending_requests,
        },
    })
}

pub fn notify_stage5c_runtime_state_restored(
    bootstrapped: Stage5cBootstrappedPaperStrategy,
) -> Result<Stage5cRuntimeStateRestoredPaperStrategy, Stage5cRuntimeStateRestoreError> {
    notify_stage5c_runtime_state_restored_at(bootstrapped, Utc::now())
}

fn notify_stage5c_runtime_state_restored_at(
    bootstrapped: Stage5cBootstrappedPaperStrategy,
    restored_ts: DateTime<Utc>,
) -> Result<Stage5cRuntimeStateRestoredPaperStrategy, Stage5cRuntimeStateRestoreError> {
    let (mut strategy, bootstrap_receipt, restored) = bootstrapped.into_parts();
    let admission = &bootstrap_receipt.admission;
    let broker_position_qty = admission
        .bootstrap_snapshot()
        .target_position_qty
        .to_f64()
        .ok_or(Stage5cRuntimeStateRestoreError::BrokerTruthPositionMismatch)?;
    let context = StrategyCtx {
        strategy_id: admission.strategy_id().to_string(),
        portfolio: admission.account_id().as_str().to_string(),
        exchange: format!("{:?}", admission.target_instrument().exchange),
        symbol: admission.target_instrument().symbol.clone(),
        tick_size: admission.tick_size(),
        trade_mode: TradeMode::Paper,
        paper_execution_mode: PaperExecutionMode::LiveOnly,
        allow_live_orders: false,
        gateway_phase: GatewayPhase::SyncingHistory,
        position_qty: Some(broker_position_qty),
        event_ts_utc: restored_ts.timestamp(),
        now_ts_utc: restored_ts.timestamp(),
        last_bar_ts: None,
    };
    let pending_requests = restored.pending_requests.clone();
    let intents = Strategy::on_runtime_state_restored(&mut strategy, &context, &restored);
    debug_assert!(
        intents.is_empty(),
        "accepted source runtime-state restore must not emit intents"
    );
    validate_post_bootstrap_broker_truth(&strategy, admission)?;

    Ok(Stage5cRuntimeStateRestoredPaperStrategy {
        strategy,
        receipt: Stage5cRuntimeStateRestoreReceipt {
            bootstrap_receipt,
            restored_ts,
            pending_requests,
        },
    })
}

fn same_tick_size(left: f64, right: f64) -> bool {
    left.is_finite() && right.is_finite() && (left - right).abs() <= f64::EPSILON
}

fn normalize_legacy_order_ids(
    value: &mut serde_json::Value,
    policy: Stage5cLegacyNumericOrderIdPolicy,
) -> Result<(), Stage5cRuntimeStateRestoreError> {
    match value {
        serde_json::Value::Object(fields) => {
            for (name, field) in fields {
                if matches!(name.as_str(), "tp_order_id" | "sl_exchange_order_id")
                    && field.is_number()
                {
                    if policy == Stage5cLegacyNumericOrderIdPolicy::Reject {
                        return Err(Stage5cRuntimeStateRestoreError::LegacyNumericOrderIdRejected);
                    }
                    let numeric = field
                        .as_i64()
                        .filter(|value| *value > 0)
                        .ok_or(Stage5cRuntimeStateRestoreError::InvalidLegacyNumericOrderId)?;
                    let converted =
                        BrokerOrderId::try_from_legacy_alor_numeric(numeric).map_err(|_| {
                            Stage5cRuntimeStateRestoreError::InvalidLegacyNumericOrderId
                        })?;
                    *field = serde_json::Value::String(converted.as_str().to_string());
                } else {
                    normalize_legacy_order_ids(field, policy)?;
                }
            }
            Ok(())
        }
        serde_json::Value::Array(values) => {
            for value in values {
                normalize_legacy_order_ids(value, policy)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

fn validate_post_bootstrap_broker_truth(
    strategy: &HybridIntradayRuntimeStrategy,
    admission: &Stage5cPaperHostAdmission,
) -> Result<(), Stage5cRuntimeStateRestoreError> {
    let state = Strategy::state(strategy);
    let broker_qty = admission
        .bootstrap_snapshot()
        .target_position_qty
        .to_f64()
        .ok_or(Stage5cRuntimeStateRestoreError::BrokerTruthPositionMismatch)?;
    match state {
        StrategyState::HybridIntradayRuntime {
            last_position_qty,
            current_side,
            tp_order_id,
            sl_stop_order_id,
            sl_exchange_order_id,
            ..
        } => {
            if (*last_position_qty - broker_qty).abs() > f64::EPSILON {
                return Err(Stage5cRuntimeStateRestoreError::BrokerTruthPositionMismatch);
            }
            let expected = if broker_qty > f64::EPSILON {
                Some(crate::hybrid_intraday::Side::Long)
            } else if broker_qty < -f64::EPSILON {
                Some(crate::hybrid_intraday::Side::Short)
            } else {
                None
            };
            if expected.is_some() && *current_side != expected {
                return Err(Stage5cRuntimeStateRestoreError::BrokerTruthSideMismatch);
            }
            if tp_order_id.is_some() || sl_stop_order_id.is_some() || sl_exchange_order_id.is_some()
            {
                return Err(Stage5cRuntimeStateRestoreError::BrokerOwnedOrderIdMismatch);
            }
            Ok(())
        }
        StrategyState::Idle => Err(Stage5cRuntimeStateRestoreError::WrongStrategyStateKind),
    }
}

pub fn accept_stage5c_history_batch(
    input: Stage5cHistoryBatchInput,
) -> Result<Stage5cAcceptedHistoryBatch, Stage5cHistoryWarmupError> {
    if input.bars.is_empty() {
        return Err(Stage5cHistoryWarmupError::EmptyHistory);
    }
    let instrument = input.bars[0].instrument.clone();
    let mut previous_close_ts = None;
    for bar in &input.bars {
        if bar.instrument != instrument {
            return Err(Stage5cHistoryWarmupError::InstrumentMismatch);
        }
        if bar.timeframe_sec != 600 {
            return Err(Stage5cHistoryWarmupError::InvalidTimeframe);
        }
        if !bar.is_final {
            return Err(Stage5cHistoryWarmupError::NonFinalBar);
        }
        if bar.origin != broker_core::HybridRuntimeBarOrigin::History {
            return Err(Stage5cHistoryWarmupError::InvalidOrigin);
        }
        if bar.close_time_utc.rem_euclid(600) != 0 {
            return Err(Stage5cHistoryWarmupError::UnalignedTimestamp);
        }
        let close_ts = DateTime::<Utc>::from_timestamp(bar.close_time_utc, 0)
            .ok_or(Stage5cHistoryWarmupError::InvalidHistoryTimestamp)?;
        let open_ts = close_ts
            .checked_sub_signed(chrono::Duration::seconds(600))
            .ok_or(Stage5cHistoryWarmupError::InvalidHistoryTimestamp)?;
        if previous_close_ts.is_some_and(|previous| bar.close_time_utc <= previous) {
            return Err(Stage5cHistoryWarmupError::NonMonotonicTimestamp);
        }
        previous_close_ts = Some(bar.close_time_utc);
        if ![bar.open, bar.high, bar.low, bar.close]
            .iter()
            .all(|value| value.is_finite())
            || bar.low > bar.high
            || bar.high < bar.open.max(bar.close)
            || bar.low > bar.open.min(bar.close)
        {
            return Err(Stage5cHistoryWarmupError::InvalidOhlc);
        }
        if !bar.volume.is_finite() || bar.volume < 0.0 {
            return Err(Stage5cHistoryWarmupError::InvalidVolume);
        }
        let stage3_bar = broker_core::event::Bar {
            instrument: bar.instrument.clone(),
            source_kind: broker_core::MarketDataSourceKind::HistoricalPoll,
            timeframe_sec: bar.timeframe_sec,
            open_ts,
            close_ts,
            open: rust_decimal::Decimal::ZERO,
            high: rust_decimal::Decimal::ZERO,
            low: rust_decimal::Decimal::ZERO,
            close: rust_decimal::Decimal::ZERO,
            volume: rust_decimal::Decimal::ZERO,
            is_final: bar.is_final,
        };
        if !broker_core::evaluate_stage3_strategy_input_gate(&stage3_bar, &input.provenance)
            .accepted
        {
            return Err(Stage5cHistoryWarmupError::Stage3ProvenanceRejected);
        }
    }
    Ok(Stage5cAcceptedHistoryBatch {
        start_ts: input.bars[0].close_time_utc,
        end_ts: input.bars.last().expect("non-empty history").close_time_utc,
        bars: input.bars,
        provenance: input.provenance,
        instrument,
    })
}

pub fn warmup_stage5c_history(
    restored: Stage5cRuntimeStateRestoredPaperStrategy,
    history: Stage5cAcceptedHistoryBatch,
) -> Result<Stage5cWarmedPaperStrategy, Stage5cHistoryWarmupError> {
    warmup_stage5c_history_at(restored, history, Utc::now())
}

fn warmup_stage5c_history_at(
    restored: Stage5cRuntimeStateRestoredPaperStrategy,
    history: Stage5cAcceptedHistoryBatch,
    warmup_now: DateTime<Utc>,
) -> Result<Stage5cWarmedPaperStrategy, Stage5cHistoryWarmupError> {
    let (mut strategy, restore_receipt) = restored.into_parts();
    let bootstrap_receipt = restore_receipt.bootstrap_receipt();
    let admission = &bootstrap_receipt.admission;
    if warmup_now > bootstrap_receipt.expires_at() {
        return Err(Stage5cHistoryWarmupError::BrokerTruthExpired);
    }
    if !(admission.checked_ts() <= admission.issued_ts()
        && admission.issued_ts() <= bootstrap_receipt.notified_ts()
        && bootstrap_receipt.notified_ts() <= restore_receipt.restored_ts()
        && restore_receipt.restored_ts() <= warmup_now)
    {
        return Err(Stage5cHistoryWarmupError::LifecycleTimestampReversal);
    }
    validate_stage5cd_time_boundary(&history, admission, warmup_now)?;

    let input_bars = history.bars.len();
    let source_mode = history.provenance.source_mode;
    let last_history_ts = history.end_ts;
    let mut bars = Vec::with_capacity(input_bars);
    for bar in history.bars {
        bars.push(crate::runtime_compat::BarEvent {
            symbol: bar.instrument.symbol,
            close_time_utc: bar.close_time_utc,
            close: bar.close,
            o: bar.open,
            h: bar.high,
            l: bar.low,
            v: bar.volume,
            origin: crate::runtime_compat::DataOrigin::History,
        });
    }

    let context = StrategyCtx {
        strategy_id: admission.strategy_id().to_string(),
        portfolio: admission.account_id().as_str().to_string(),
        exchange: format!("{:?}", admission.target_instrument().exchange),
        symbol: admission.target_instrument().symbol.clone(),
        tick_size: admission.tick_size(),
        trade_mode: TradeMode::Paper,
        paper_execution_mode: PaperExecutionMode::HistorySim,
        allow_live_orders: false,
        gateway_phase: GatewayPhase::SyncingHistory,
        position_qty: admission.bootstrap_snapshot().target_position_qty.to_f64(),
        event_ts_utc: bars
            .last()
            .map_or(warmup_now.timestamp(), |bar| bar.close_time_utc),
        now_ts_utc: warmup_now.timestamp(),
        last_bar_ts: bars.last().map(|bar| bar.close_time_utc),
    };
    let processed_bars = Strategy::warmup_from_history(&mut strategy, &context, &bars);
    if processed_bars == 0 {
        return Err(Stage5cHistoryWarmupError::NoEligibleHistoryBars);
    }

    Ok(Stage5cWarmedPaperStrategy {
        strategy,
        receipt: Stage5cHistoryWarmupReceipt {
            restore_receipt,
            started_ts: warmup_now,
            processed_bars,
            input_bars,
            source_mode,
            last_history_ts,
        },
    })
}

fn validate_stage5cd_time_boundary(
    history: &Stage5cAcceptedHistoryBatch,
    admission: &Stage5cPaperHostAdmission,
    warmup_now: DateTime<Utc>,
) -> Result<(), Stage5cHistoryWarmupError> {
    if history.instrument != *admission.target_instrument() {
        return Err(Stage5cHistoryWarmupError::InstrumentMismatch);
    }
    if DateTime::<Utc>::from_timestamp(history.start_ts, 0).is_none()
        || DateTime::<Utc>::from_timestamp(history.end_ts, 0).is_none()
    {
        return Err(Stage5cHistoryWarmupError::InvalidHistoryTimestamp);
    }
    if history.end_ts > warmup_now.timestamp() {
        return Err(Stage5cHistoryWarmupError::FutureHistoryBar);
    }
    Ok(())
}

pub fn prove_stage5c_pending_recovery_claim(
    warmed: &Stage5cWarmedPaperStrategy,
    input: Stage5cPendingRecoveryClaimProofInput,
) -> Result<Stage5cPendingRecoveryClaimProof, Stage5cPendingRecoveryError> {
    let admission = &warmed
        .receipt()
        .restore_receipt()
        .bootstrap_receipt()
        .admission;
    if input.strategy_id != admission.strategy_id()
        || input.account_id != *admission.account_id()
        || input.target_instrument != *admission.target_instrument()
        || input.snapshot_received_ts != admission.bootstrap_snapshot().received_ts
    {
        return Err(Stage5cPendingRecoveryError::ClaimScopeMismatch);
    }
    let required = [
        Stage5cPendingStreamKind::Ack,
        Stage5cPendingStreamKind::Order,
        Stage5cPendingStreamKind::StopOrder,
        Stage5cPendingStreamKind::Position,
    ];
    if input.completed_ts < warmed.receipt().started_ts()
        || input.streams.len() != required.len()
        || required.iter().any(|kind| {
            !input
                .streams
                .iter()
                .any(|stream| stream.stream_kind == *kind)
        })
        || input.streams.iter().any(|stream| {
            stream.stream_name
                != canonical_pending_stream_name(stream.stream_kind, &input.account_id)
                || stream.consumer_group
                    != format!("paper-runtime:{}:{}", input.account_id, input.strategy_id)
                || stream.terminal_claim_cursor != "0-0"
                || parse_redis_stream_id(&stream.snapshot_boundary_entry_id).is_none()
        })
    {
        return Err(Stage5cPendingRecoveryError::ClaimBoundaryInvalid);
    }
    Ok(Stage5cPendingRecoveryClaimProof {
        strategy_id: input.strategy_id,
        account_id: input.account_id,
        target_instrument: input.target_instrument,
        snapshot_received_ts: input.snapshot_received_ts,
        completed_ts: input.completed_ts,
        streams: input.streams,
    })
}

pub fn accept_stage5c_pending_recovery_evidence(
    input: Stage5cPendingRecoveryEvidenceInput,
) -> Result<Stage5cAcceptedPendingRecoveryEvidence, Stage5cPendingRecoveryError> {
    let mut unique = HashMap::<(String, String), Stage5cPendingRecoveryEvent>::new();
    let mut duplicate_events = 0usize;
    for event in input.events {
        if event.stream_name.trim().is_empty() || parse_redis_stream_id(&event.entry_id).is_none() {
            return Err(Stage5cPendingRecoveryError::InvalidEventIdentity);
        }
        let boundary = input
            .claim_proof
            .streams
            .iter()
            .find(|stream| {
                stream.stream_kind == event.stream_kind && stream.stream_name == event.stream_name
            })
            .ok_or(Stage5cPendingRecoveryError::StreamKindMismatch)?;
        let payload_kind = match &event.payload {
            Stage5cPendingRecoveryPayload::Ack(_) => Stage5cPendingStreamKind::Ack,
            Stage5cPendingRecoveryPayload::Order(_) => Stage5cPendingStreamKind::Order,
            Stage5cPendingRecoveryPayload::StopOrder(_) => Stage5cPendingStreamKind::StopOrder,
            Stage5cPendingRecoveryPayload::Position(_) => Stage5cPendingStreamKind::Position,
        };
        if payload_kind != event.stream_kind || boundary.consumer_group.trim().is_empty() {
            return Err(Stage5cPendingRecoveryError::StreamKindMismatch);
        }
        let key = (event.stream_name.clone(), event.entry_id.clone());
        if let Some(existing) = unique.get(&key) {
            if existing != &event {
                return Err(Stage5cPendingRecoveryError::ConflictingDuplicate);
            }
            duplicate_events += 1;
        } else {
            unique.insert(key, event);
        }
    }
    let mut events: Vec<_> = unique.into_values().collect();
    events.sort_by_key(|event| event.sequence);
    if events
        .windows(2)
        .any(|pair| pair[0].sequence >= pair[1].sequence)
    {
        return Err(Stage5cPendingRecoveryError::NonMonotonicSequence);
    }
    if input.claim_proof.streams.iter().any(|stream| {
        stream.claimed_count
            != events
                .iter()
                .filter(|event| event.stream_kind == stream.stream_kind)
                .count()
    }) {
        return Err(Stage5cPendingRecoveryError::ClaimBoundaryInvalid);
    }
    Ok(Stage5cAcceptedPendingRecoveryEvidence {
        events,
        duplicate_events,
        claim_proof: input.claim_proof,
    })
}

fn canonical_pending_stream_name(
    kind: Stage5cPendingStreamKind,
    account: &BrokerAccountId,
) -> String {
    let prefix = match kind {
        Stage5cPendingStreamKind::Ack => "cmd.acks",
        Stage5cPendingStreamKind::Order => "broker.orders",
        Stage5cPendingStreamKind::StopOrder => "broker.stop_orders",
        Stage5cPendingStreamKind::Position => "broker.positions",
    };
    format!("{prefix}.{account}")
}

fn parse_redis_stream_id(value: &str) -> Option<(u64, u64)> {
    let (milliseconds, sequence) = value.split_once('-')?;
    Some((milliseconds.parse().ok()?, sequence.parse().ok()?))
}

pub fn recover_stage5c_pending_streams(
    warmed: Stage5cWarmedPaperStrategy,
    evidence: Stage5cAcceptedPendingRecoveryEvidence,
) -> Result<Stage5cPendingRecoveredPaperStrategy, Stage5cPendingRecoveryError> {
    recover_stage5c_pending_streams_at(warmed, evidence, Utc::now())
}

fn recover_stage5c_pending_streams_at(
    warmed: Stage5cWarmedPaperStrategy,
    evidence: Stage5cAcceptedPendingRecoveryEvidence,
    recovered_ts: DateTime<Utc>,
) -> Result<Stage5cPendingRecoveredPaperStrategy, Stage5cPendingRecoveryError> {
    let (mut strategy, warmup_receipt) = warmed.into_parts();
    let bootstrap_receipt = warmup_receipt.restore_receipt().bootstrap_receipt();
    let admission = &bootstrap_receipt.admission;
    if recovered_ts > bootstrap_receipt.expires_at() {
        return Err(Stage5cPendingRecoveryError::BrokerTruthExpired);
    }
    if warmup_receipt.started_ts() > recovered_ts {
        return Err(Stage5cPendingRecoveryError::LifecycleTimestampReversal);
    }
    if evidence.claim_proof.strategy_id != admission.strategy_id()
        || evidence.claim_proof.account_id != *admission.account_id()
        || evidence.claim_proof.target_instrument != *admission.target_instrument()
        || evidence.claim_proof.snapshot_received_ts != admission.bootstrap_snapshot().received_ts
        || evidence.claim_proof.completed_ts > recovered_ts
    {
        return Err(Stage5cPendingRecoveryError::ClaimScopeMismatch);
    }
    for event in &evidence.events {
        let instrument = match &event.payload {
            Stage5cPendingRecoveryPayload::Ack(_) => None,
            Stage5cPendingRecoveryPayload::Order(value) => Some(&value.instrument),
            Stage5cPendingRecoveryPayload::StopOrder(value) => Some(&value.instrument),
            Stage5cPendingRecoveryPayload::Position(value) => Some(&value.instrument),
        };
        if instrument.is_some_and(|value| value != admission.target_instrument()) {
            return Err(Stage5cPendingRecoveryError::InstrumentMismatch);
        }
        let event_ts = match &event.payload {
            Stage5cPendingRecoveryPayload::Ack(value) => value.processed_ts_utc,
            Stage5cPendingRecoveryPayload::Order(value) => value.source_ts_utc,
            Stage5cPendingRecoveryPayload::StopOrder(value) => value.source_ts_utc,
            Stage5cPendingRecoveryPayload::Position(value) => value.source_ts_utc,
        };
        if DateTime::<Utc>::from_timestamp(event_ts, 0).is_none() {
            return Err(Stage5cPendingRecoveryError::InvalidEventTimestamp);
        }
        if event_ts > recovered_ts.timestamp() {
            return Err(Stage5cPendingRecoveryError::FutureEvent);
        }
        if let Stage5cPendingRecoveryPayload::Ack(value) = &event.payload {
            if !warmup_receipt
                .restore_receipt()
                .pending_requests()
                .contains(&value.request_id)
            {
                return Err(Stage5cPendingRecoveryError::AckNotPending);
            }
        }
    }
    let position_qty = admission.bootstrap_snapshot().target_position_qty.to_f64();
    let mut replayed_events = 0usize;
    let duplicate_events = evidence.duplicate_events;
    for event in evidence.events {
        let event_ts = match &event.payload {
            Stage5cPendingRecoveryPayload::Ack(value) => value.processed_ts_utc,
            Stage5cPendingRecoveryPayload::Order(value) => value.source_ts_utc,
            Stage5cPendingRecoveryPayload::StopOrder(value) => value.source_ts_utc,
            Stage5cPendingRecoveryPayload::Position(value) => value.source_ts_utc,
        };
        let context = broker_core::HybridRuntimeStrategyContext {
            strategy_id: admission.strategy_id().to_string(),
            request_namespace_account: admission.account_id().clone(),
            instrument: admission.target_instrument().clone(),
            tick_size: admission.tick_size(),
            trade_mode: broker_core::HybridRuntimeTradeMode::Paper,
            paper_execution_mode: broker_core::HybridRuntimePaperExecutionMode::LiveOnly,
            allow_live_orders: false,
            gateway_phase: broker_core::HybridRuntimeGatewayPhase::CatchingUp,
            position_qty,
            event_ts_utc: event_ts,
            strategy_now_ts_utc: recovered_ts.timestamp(),
            last_bar_ts_utc: None,
        };
        let boundary = evidence
            .claim_proof
            .streams
            .iter()
            .find(|stream| stream.stream_kind == event.stream_kind)
            .expect("accepted evidence has every typed stream");
        let entry_id =
            parse_redis_stream_id(&event.entry_id).expect("accepted evidence has valid entry IDs");
        let snapshot_boundary = parse_redis_stream_id(&boundary.snapshot_boundary_entry_id)
            .expect("accepted proof has valid boundary IDs");
        if !matches!(&event.payload, Stage5cPendingRecoveryPayload::Ack(_))
            && entry_id <= snapshot_boundary
        {
            continue;
        }
        let result = match event.payload {
            Stage5cPendingRecoveryPayload::Ack(value) => {
                crate::BrokerNeutralHybridStrategy::on_broker_ack(&mut strategy, value)
            }
            Stage5cPendingRecoveryPayload::Order(value) => {
                crate::BrokerNeutralHybridStrategy::on_broker_order(
                    &mut strategy,
                    broker_core::HybridRuntimeCallbackInput {
                        context,
                        payload: value,
                    },
                )
            }
            Stage5cPendingRecoveryPayload::StopOrder(value) => {
                crate::BrokerNeutralHybridStrategy::on_broker_stop_order(
                    &mut strategy,
                    broker_core::HybridRuntimeCallbackInput {
                        context,
                        payload: value,
                    },
                )
            }
            Stage5cPendingRecoveryPayload::Position(value) => {
                crate::BrokerNeutralHybridStrategy::on_broker_position(
                    &mut strategy,
                    broker_core::HybridRuntimeCallbackInput {
                        context,
                        payload: value,
                    },
                )
            }
        }
        .map_err(|_| Stage5cPendingRecoveryError::CallbackValidationFailed)?;
        if !result.is_empty() {
            return Err(Stage5cPendingRecoveryError::UnexpectedIntent);
        }
        replayed_events += 1;
    }
    Ok(Stage5cPendingRecoveredPaperStrategy {
        strategy,
        receipt: Stage5cPendingRecoveryReceipt {
            warmup_receipt,
            recovered_ts,
            replayed_events,
            duplicate_events,
        },
    })
}

pub fn accept_stage5c_semantic_bar(
    input: Stage5cSemanticBarInput,
) -> Result<Stage5cAcceptedSemanticBar, Stage5cSemanticBarError> {
    let close_ts = DateTime::<Utc>::from_timestamp(input.bar.close_time_utc, 0)
        .ok_or(Stage5cSemanticBarError::InvalidTimestamp)?;
    let open_ts = close_ts
        .checked_sub_signed(chrono::Duration::seconds(600))
        .ok_or(Stage5cSemanticBarError::InvalidTimestamp)?;
    if !matches!(
        input.bar.origin,
        broker_core::HybridRuntimeBarOrigin::Live | broker_core::HybridRuntimeBarOrigin::Replay
    ) {
        return Err(Stage5cSemanticBarError::Stage3Rejected);
    }
    if input.bar.close_time_utc.rem_euclid(600) != 0 {
        return Err(Stage5cSemanticBarError::UnalignedTimestamp);
    }
    if ![
        input.bar.open,
        input.bar.high,
        input.bar.low,
        input.bar.close,
    ]
    .iter()
    .all(|value| value.is_finite())
        || input.bar.low > input.bar.high
        || input.bar.high < input.bar.open.max(input.bar.close)
        || input.bar.low > input.bar.open.min(input.bar.close)
    {
        return Err(Stage5cSemanticBarError::InvalidOhlc);
    }
    if !input.bar.volume.is_finite() || input.bar.volume < 0.0 {
        return Err(Stage5cSemanticBarError::InvalidVolume);
    }
    let gate_bar = broker_core::event::Bar {
        instrument: input.bar.instrument.clone(),
        source_kind: broker_core::MarketDataSourceKind::LiveStream,
        timeframe_sec: input.bar.timeframe_sec,
        open_ts,
        close_ts,
        open: rust_decimal::Decimal::ZERO,
        high: rust_decimal::Decimal::ZERO,
        low: rust_decimal::Decimal::ZERO,
        close: rust_decimal::Decimal::ZERO,
        volume: rust_decimal::Decimal::ZERO,
        is_final: input.bar.is_final,
    };
    if !broker_core::evaluate_stage3_strategy_input_gate(&gate_bar, &input.provenance).accepted {
        return Err(Stage5cSemanticBarError::Stage3Rejected);
    }
    Ok(Stage5cAcceptedSemanticBar {
        origin: input.bar.origin,
        bar: input.bar,
        tick_size: input.tick_size,
    })
}

pub fn apply_stage5c_semantic_bar(
    recovered: Stage5cPendingRecoveredPaperStrategy,
    accepted: Stage5cAcceptedSemanticBar,
) -> Result<Stage5cSemanticBarResult, Stage5cSemanticBarError> {
    apply_stage5c_semantic_bar_at(recovered, accepted, Utc::now())
}

fn apply_stage5c_semantic_bar_at(
    recovered: Stage5cPendingRecoveredPaperStrategy,
    accepted: Stage5cAcceptedSemanticBar,
    now: DateTime<Utc>,
) -> Result<Stage5cSemanticBarResult, Stage5cSemanticBarError> {
    let (mut strategy, recovery_receipt) = recovered.into_parts();
    let admission = &recovery_receipt
        .warmup_receipt()
        .restore_receipt()
        .bootstrap_receipt()
        .admission;
    if now
        > recovery_receipt
            .warmup_receipt()
            .restore_receipt()
            .bootstrap_receipt()
            .expires_at()
    {
        return Err(Stage5cSemanticBarError::BrokerTruthExpired);
    }
    if accepted.bar.instrument != *admission.target_instrument() {
        return Err(Stage5cSemanticBarError::InstrumentMismatch);
    }
    if !same_tick_size(accepted.tick_size, admission.tick_size()) {
        return Err(Stage5cSemanticBarError::TickSizeMismatch);
    }
    if accepted.bar.close_time_utc <= recovery_receipt.recovered_ts().timestamp()
        || accepted.bar.close_time_utc <= recovery_receipt.warmup_receipt().last_history_ts()
    {
        return Err(Stage5cSemanticBarError::StaleOrDuplicateBar);
    }
    if accepted.bar.close_time_utc > now.timestamp() {
        return Err(Stage5cSemanticBarError::FutureBar);
    }
    let context = stage5cf_semantic_context(&strategy, admission, accepted.bar.close_time_utc, now);
    let bar_close_ts = accepted.bar.close_time_utc;
    let origin = accepted.origin;
    let execution_eligible = origin == broker_core::HybridRuntimeBarOrigin::Live;
    let intents = crate::BrokerNeutralHybridStrategy::on_broker_bar(
        &mut strategy,
        broker_core::HybridRuntimeCallbackInput {
            context,
            payload: accepted.bar,
        },
    )
    .map_err(|_| Stage5cSemanticBarError::CallbackValidationFailed)?;
    Ok(Stage5cSemanticBarResult {
        strategy,
        recovery_receipt,
        bar_close_ts,
        origin,
        execution_eligible,
        intents,
    })
}

fn stage5cf_semantic_context(
    strategy: &HybridIntradayRuntimeStrategy,
    admission: &Stage5cPaperHostAdmission,
    bar_close_ts: i64,
    now: DateTime<Utc>,
) -> broker_core::HybridRuntimeStrategyContext {
    broker_core::HybridRuntimeStrategyContext {
        strategy_id: admission.strategy_id().to_string(),
        request_namespace_account: admission.account_id().clone(),
        instrument: admission.target_instrument().clone(),
        tick_size: admission.tick_size(),
        trade_mode: broker_core::HybridRuntimeTradeMode::Paper,
        paper_execution_mode: broker_core::HybridRuntimePaperExecutionMode::LiveOnly,
        allow_live_orders: false,
        gateway_phase: broker_core::HybridRuntimeGatewayPhase::LiveReady,
        position_qty: Some(strategy.stage5c_current_position_qty()),
        event_ts_utc: bar_close_ts,
        strategy_now_ts_utc: now.timestamp(),
        last_bar_ts_utc: Some(bar_close_ts),
    }
}

pub fn settle_stage5c_semantic_result(
    result: Stage5cSemanticBarResult,
) -> Result<Stage5cSettledPaperStrategy, Stage5cIntentSettlementError> {
    let (strategy, recovery_receipt, bar_close_ts, origin, execution_eligible, intents) =
        result.into_parts();
    if intents.len() > u8::MAX as usize {
        return Err(Stage5cIntentSettlementError::TooManyIntents);
    }
    let admission = &recovery_receipt
        .warmup_receipt()
        .restore_receipt()
        .bootstrap_receipt()
        .admission;
    let mut request_ids = Vec::with_capacity(intents.len());
    let mut records = Vec::with_capacity(intents.len());
    let mut seen_request_ids = HashSet::new();
    let state = Strategy::state(&strategy);
    if !execution_eligible && !intents.is_empty() {
        return Err(Stage5cIntentSettlementError::ReplayIntentNotExecutable);
    }
    for intent in intents {
        validate_stage5cg_intent(
            &intent,
            &admission.target_instrument().symbol,
            admission.tick_size(),
            bar_close_ts,
        )?;
        let class = intent
            .explicit_class()
            .ok_or(Stage5cIntentSettlementError::MissingIntentClass)?;
        let request_id = stage5cg_source_request_id(
            admission.strategy_id(),
            admission.account_id().as_str(),
            &admission.target_instrument().symbol,
            bar_close_ts,
            &intent,
        )?;
        stage5cg_verify_pending_request_id(state, class, request_id)?;
        if !seen_request_ids.insert(request_id) {
            return Err(Stage5cIntentSettlementError::DuplicateRequestId);
        }
        request_ids.push(request_id);
        records.push(Stage5cPaperIntentRecord {
            request_id,
            intent_class: class,
            intent,
        });
    }
    let state_fingerprint = stage5c_state_fingerprint(state);
    let strategy_id = admission.strategy_id().to_string();
    let account_id = admission.account_id().clone();
    let instrument = admission.target_instrument().clone();
    let batch = Stage5cPaperIntentBatch {
        strategy_id,
        account_id,
        instrument,
        bar_close_ts,
        state_fingerprint,
        request_ids,
        records,
        observation_only: origin == broker_core::HybridRuntimeBarOrigin::Replay,
    };
    let settled_batch_history = vec![stage5ch_batch_summary(&batch)];
    Ok(Stage5cSettledPaperStrategy {
        strategy,
        recovery_receipt,
        batch,
        settled_batch_history,
    })
}

fn stage5ch_batch_summary(batch: &Stage5cPaperIntentBatch) -> Stage5cPaperIntentBatchSummary {
    Stage5cPaperIntentBatchSummary {
        strategy_id: batch.strategy_id.clone(),
        account_id: batch.account_id.clone(),
        instrument: batch.instrument.clone(),
        bar_close_ts: batch.bar_close_ts,
        state_fingerprint: batch.state_fingerprint.clone(),
        request_ids: batch.request_ids.clone(),
        intent_count: batch.intent_count(),
        observation_only: batch.observation_only,
    }
}

fn stage5c_state_fingerprint(state: &StrategyState) -> String {
    let state_bytes = serde_json::to_vec(state).expect("strategy state is serializable");
    format!("{:x}", Sha256::digest(&state_bytes))
}

pub fn advance_stage5c_controlled_next_bar(
    settled: Stage5cSettledPaperStrategy,
    accepted: Stage5cAcceptedSemanticBar,
) -> Result<Stage5cSettledPaperStrategy, Stage5cNextBarLoopFailure> {
    advance_stage5c_controlled_next_bar_at(settled, accepted, Utc::now())
}

fn advance_stage5c_controlled_next_bar_at(
    settled: Stage5cSettledPaperStrategy,
    accepted: Stage5cAcceptedSemanticBar,
    now: DateTime<Utc>,
) -> Result<Stage5cSettledPaperStrategy, Stage5cNextBarLoopFailure> {
    if accepted.bar.close_time_utc <= settled.batch.bar_close_ts() {
        return Err(Stage5cNextBarLoopFailure::Blocked(Box::new(
            Stage5cNextBarBlocked {
                reason: Stage5cNextBarLoopError::NonMonotonicBar,
                settled,
            },
        )));
    }
    if settled.batch.intent_count() > 0 {
        return Err(Stage5cNextBarLoopFailure::Blocked(Box::new(
            Stage5cNextBarBlocked {
                reason: Stage5cNextBarLoopError::UnresolvedIntentBatch,
                settled,
            },
        )));
    }
    if now
        > settled
            .recovery_receipt
            .warmup_receipt()
            .restore_receipt()
            .bootstrap_receipt()
            .expires_at()
    {
        return Err(Stage5cNextBarLoopFailure::Blocked(Box::new(
            Stage5cNextBarBlocked {
                reason: Stage5cNextBarLoopError::Semantic(
                    Stage5cSemanticBarError::BrokerTruthExpired,
                ),
                settled,
            },
        )));
    }
    let mut history = settled.settled_batch_history.clone();
    let (strategy, recovery_receipt, _) = settled.into_parts();
    let recovered = Stage5cPendingRecoveredPaperStrategy {
        strategy,
        receipt: recovery_receipt,
    };
    let semantic = apply_stage5c_semantic_bar_at(recovered, accepted, now).map_err(|reason| {
        Stage5cNextBarLoopFailure::Failed(Stage5cNextBarLoopError::Semantic(reason))
    })?;
    let mut next = settle_stage5c_semantic_result(semantic).map_err(|reason| {
        Stage5cNextBarLoopFailure::Failed(Stage5cNextBarLoopError::Settlement(reason))
    })?;
    history.push(stage5ch_batch_summary(next.intent_batch()));
    next.settled_batch_history = history;
    Ok(next)
}

pub fn resolve_stage5c_paper_intent_lifecycle(
    settled: Stage5cSettledPaperStrategy,
    input: Stage5cPaperIntentLifecycleInput,
) -> Result<Stage5cResolvedPaperIntentBatchStrategy, Stage5cPaperIntentLifecycleFailure> {
    if settled.batch.intent_count() == 0 {
        return Err(Stage5cPaperIntentLifecycleFailure::Blocked(Box::new(
            Stage5cPaperIntentLifecycleBlocked {
                reason: Stage5cPaperIntentLifecycleError::EmptyIntentBatch,
                settled,
            },
        )));
    }
    let state_fingerprint = stage5c_state_fingerprint(Strategy::state(&settled.strategy));
    if state_fingerprint != settled.batch.state_fingerprint {
        return Err(Stage5cPaperIntentLifecycleFailure::Blocked(Box::new(
            Stage5cPaperIntentLifecycleBlocked {
                reason: Stage5cPaperIntentLifecycleError::StateFingerprintMismatch,
                settled,
            },
        )));
    }
    let expected_request_ids: HashSet<StrategyRequestId> =
        settled.batch.request_ids.iter().copied().collect();
    let mut seen = HashSet::new();
    let mut sequences = HashSet::new();
    for record in &input.ack_records {
        if !sequences.insert(record.total_sequence) {
            return Err(Stage5cPaperIntentLifecycleFailure::Blocked(Box::new(
                Stage5cPaperIntentLifecycleBlocked {
                    reason: Stage5cPaperIntentLifecycleError::DuplicateSequence,
                    settled,
                },
            )));
        }
        if record.ack.processed_ts_utc < settled.batch.bar_close_ts() {
            return Err(Stage5cPaperIntentLifecycleFailure::Blocked(Box::new(
                Stage5cPaperIntentLifecycleBlocked {
                    reason: Stage5cPaperIntentLifecycleError::AckTimestampBeforeIntentBar,
                    settled,
                },
            )));
        }
        if !expected_request_ids.contains(&record.ack.request_id) {
            return Err(Stage5cPaperIntentLifecycleFailure::Blocked(Box::new(
                Stage5cPaperIntentLifecycleBlocked {
                    reason: Stage5cPaperIntentLifecycleError::UnknownAckRequestId,
                    settled,
                },
            )));
        }
        if !seen.insert(record.ack.request_id) {
            return Err(Stage5cPaperIntentLifecycleFailure::Blocked(Box::new(
                Stage5cPaperIntentLifecycleBlocked {
                    reason: Stage5cPaperIntentLifecycleError::DuplicateAck,
                    settled,
                },
            )));
        }
    }
    if seen.len() != expected_request_ids.len() {
        return Err(Stage5cPaperIntentLifecycleFailure::Blocked(Box::new(
            Stage5cPaperIntentLifecycleBlocked {
                reason: Stage5cPaperIntentLifecycleError::MissingAck,
                settled,
            },
        )));
    }
    let Stage5cSettledPaperStrategy {
        mut strategy,
        recovery_receipt,
        batch,
        settled_batch_history,
    } = settled;
    let mut ack_records = input.ack_records;
    ack_records.sort_by_key(|record| record.total_sequence);
    let mut last_sequence = None;
    let mut ack_outcomes = Vec::with_capacity(ack_records.len());
    for record in ack_records {
        if last_sequence.is_some_and(|previous| record.total_sequence <= previous) {
            return Err(Stage5cPaperIntentLifecycleFailure::Terminal(
                Stage5cPaperIntentLifecycleError::NonMonotonicSequence,
            ));
        }
        last_sequence = Some(record.total_sequence);
        let outcome = Stage5cPaperAckOutcome {
            total_sequence: record.total_sequence,
            request_id: record.ack.request_id,
            status: record.ack.status,
            broker_order_id: record.ack.broker_order_id.clone(),
            error_code: record.ack.error_code.clone(),
            processed_ts_utc: record.ack.processed_ts_utc,
        };
        let intents = crate::BrokerNeutralHybridStrategy::on_broker_ack(&mut strategy, record.ack)
            .map_err(|_| {
                Stage5cPaperIntentLifecycleFailure::Terminal(
                    Stage5cPaperIntentLifecycleError::CallbackValidationFailed,
                )
            })?;
        if !intents.is_empty() {
            return Err(Stage5cPaperIntentLifecycleFailure::Terminal(
                Stage5cPaperIntentLifecycleError::CallbackGeneratedIntentTerminal,
            ));
        }
        ack_outcomes.push(outcome);
    }
    Ok(Stage5cResolvedPaperIntentBatchStrategy {
        strategy,
        recovery_receipt,
        resolved_batch: batch,
        ack_outcomes,
        settled_batch_history,
    })
}

pub fn resolve_stage5c_paper_broker_lifecycle(
    resolved: Stage5cResolvedPaperIntentBatchStrategy,
    input: Stage5cPaperBrokerLifecycleInput,
) -> Result<Stage5cBrokerLifecycleResolvedPaperStrategy, Stage5cPaperBrokerLifecycleFailure> {
    let mut sequences = HashSet::new();
    let mut event_by_request: HashMap<StrategyRequestId, Stage5cPaperBrokerEventRecord> =
        HashMap::new();
    let mut event_identity_by_request: HashMap<StrategyRequestId, String> = HashMap::new();
    let mut canonical_event_records = Vec::with_capacity(input.event_records.len());
    for record in &input.event_records {
        if !sequences.insert(record.total_sequence) {
            return Err(stage5cj_block(
                Stage5cPaperBrokerLifecycleError::DuplicateSequence,
                resolved,
            ));
        }
        if !resolved
            .resolved_batch
            .request_ids
            .contains(&record.request_id)
        {
            return Err(stage5cj_block(
                Stage5cPaperBrokerLifecycleError::UnknownEventRequestId,
                resolved,
            ));
        }
        if record.payload.instrument() != resolved.resolved_batch.instrument() {
            return Err(stage5cj_block(
                Stage5cPaperBrokerLifecycleError::InstrumentMismatch,
                resolved,
            ));
        }
        let identity = match stage5cj_event_identity(record) {
            Ok(identity) => identity,
            Err(_) => {
                return Err(stage5cj_block(
                    Stage5cPaperBrokerLifecycleError::CallbackValidationFailed,
                    resolved,
                ));
            }
        };
        if let Some(previous_identity) = event_identity_by_request.get(&record.request_id) {
            if previous_identity == &identity {
                continue;
            }
            return Err(stage5cj_block(
                Stage5cPaperBrokerLifecycleError::ConflictingDuplicateEvent,
                resolved,
            ));
        }
        event_identity_by_request.insert(record.request_id, identity);
        event_by_request.insert(record.request_id, record.clone());
        canonical_event_records.push(record.clone());
    }
    canonical_event_records.sort_by_key(|record| record.total_sequence);
    let mut strategy = resolved.strategy;
    let recovery_receipt = resolved.recovery_receipt;
    let batch = resolved.resolved_batch;
    let ack_outcomes = resolved.ack_outcomes;
    let settled_batch_history = resolved.settled_batch_history;
    let admission = &recovery_receipt
        .warmup_receipt()
        .restore_receipt()
        .bootstrap_receipt()
        .admission;
    let ack_by_request: HashMap<StrategyRequestId, Stage5cPaperAckOutcome> = ack_outcomes
        .iter()
        .cloned()
        .map(|outcome| (outcome.request_id, outcome))
        .collect();
    for record in canonical_event_records {
        let Some(intent_record) = batch
            .records
            .iter()
            .find(|intent| intent.request_id == record.request_id)
        else {
            return Err(Stage5cPaperBrokerLifecycleFailure::Terminal(
                Stage5cPaperBrokerLifecycleError::UnknownEventRequestId,
            ));
        };
        let Some(ack) = ack_by_request.get(&record.request_id) else {
            return Err(Stage5cPaperBrokerLifecycleFailure::Terminal(
                Stage5cPaperBrokerLifecycleError::UnknownEventRequestId,
            ));
        };
        if stage5cj_ack_is_terminal(ack.status) {
            return Err(Stage5cPaperBrokerLifecycleFailure::Terminal(
                Stage5cPaperBrokerLifecycleError::EventForTerminalAck,
            ));
        }
        if record.payload.source_ts_utc() < ack.processed_ts_utc {
            return Err(Stage5cPaperBrokerLifecycleFailure::Terminal(
                Stage5cPaperBrokerLifecycleError::EventTimestampBeforeAck,
            ));
        }
        let expected_kind = stage5cj_expected_event_kind(&intent_record.intent);
        if record.payload.kind() != expected_kind {
            return Err(Stage5cPaperBrokerLifecycleFailure::Terminal(
                Stage5cPaperBrokerLifecycleError::UnexpectedBrokerEventKind,
            ));
        }
        stage5cj_validate_event_mapping(&record, ack, &intent_record.intent)?;
        let context = stage5cj_broker_lifecycle_context(
            &strategy,
            admission,
            batch.bar_close_ts(),
            record.payload.source_ts_utc(),
        );
        let intents = match record.payload {
            Stage5cPaperBrokerEventPayload::Order(payload) => {
                crate::BrokerNeutralHybridStrategy::on_broker_order(
                    &mut strategy,
                    broker_core::HybridRuntimeCallbackInput { context, payload },
                )
            }
            Stage5cPaperBrokerEventPayload::StopOrder(payload) => {
                crate::BrokerNeutralHybridStrategy::on_broker_stop_order(
                    &mut strategy,
                    broker_core::HybridRuntimeCallbackInput { context, payload },
                )
            }
            Stage5cPaperBrokerEventPayload::Position(payload) => {
                crate::BrokerNeutralHybridStrategy::on_broker_position(
                    &mut strategy,
                    broker_core::HybridRuntimeCallbackInput { context, payload },
                )
            }
        }
        .map_err(|_| {
            Stage5cPaperBrokerLifecycleFailure::Terminal(
                Stage5cPaperBrokerLifecycleError::CallbackValidationFailed,
            )
        })?;
        if !intents.is_empty() {
            return Err(Stage5cPaperBrokerLifecycleFailure::Terminal(
                Stage5cPaperBrokerLifecycleError::CallbackGeneratedIntentTerminal,
            ));
        }
    }
    for request_id in &batch.request_ids {
        let ack = ack_by_request
            .get(request_id)
            .expect("ACK lifecycle enforces exact request coverage");
        if stage5cj_ack_is_terminal(ack.status) {
            if event_by_request.contains_key(request_id) {
                return Err(Stage5cPaperBrokerLifecycleFailure::Terminal(
                    Stage5cPaperBrokerLifecycleError::EventForTerminalAck,
                ));
            }
            continue;
        }
        if !event_by_request.contains_key(request_id) {
            return Err(Stage5cPaperBrokerLifecycleFailure::Terminal(
                Stage5cPaperBrokerLifecycleError::MissingExpectedBrokerEvent,
            ));
        }
    }
    let resolved_batch_summary = stage5ch_batch_summary(&batch);
    Ok(Stage5cBrokerLifecycleResolvedPaperStrategy {
        strategy,
        recovery_receipt,
        resolved_batch_summary,
        ack_outcomes,
        broker_event_count: event_by_request.len(),
        settled_batch_history,
    })
}

fn stage5cj_block(
    reason: Stage5cPaperBrokerLifecycleError,
    resolved: Stage5cResolvedPaperIntentBatchStrategy,
) -> Stage5cPaperBrokerLifecycleFailure {
    Stage5cPaperBrokerLifecycleFailure::Blocked(Box::new(Stage5cPaperBrokerLifecycleBlocked {
        reason,
        resolved,
    }))
}

fn stage5cj_event_identity(
    record: &Stage5cPaperBrokerEventRecord,
) -> Result<String, serde_json::Error> {
    serde_json::to_string(&(record.request_id, &record.payload))
}

fn stage5cj_ack_is_terminal(status: broker_core::HybridRuntimeAckStatus) -> bool {
    matches!(
        status,
        broker_core::HybridRuntimeAckStatus::Rejected
            | broker_core::HybridRuntimeAckStatus::Expired
            | broker_core::HybridRuntimeAckStatus::Error
    )
}

fn stage5cj_expected_event_kind(
    intent: &crate::BrokerNeutralHybridIntent,
) -> Stage5cPaperBrokerEventKind {
    use crate::BrokerNeutralHybridIntent as Intent;
    match intent.base_intent() {
        Intent::Market { .. } => Stage5cPaperBrokerEventKind::Position,
        Intent::Place { .. } | Intent::Cancel { .. } | Intent::Replace { .. } => {
            Stage5cPaperBrokerEventKind::Order
        }
        Intent::CreateStopLimit { .. } | Intent::DeleteStopLimit { .. } => {
            Stage5cPaperBrokerEventKind::StopOrder
        }
        Intent::Classified { .. } | Intent::Routed { .. } => {
            unreachable!("base_intent unwraps wrappers")
        }
    }
}

fn stage5cj_validate_event_mapping(
    record: &Stage5cPaperBrokerEventRecord,
    ack: &Stage5cPaperAckOutcome,
    intent: &crate::BrokerNeutralHybridIntent,
) -> Result<(), Stage5cPaperBrokerLifecycleFailure> {
    match &record.payload {
        Stage5cPaperBrokerEventPayload::Order(order) => {
            if order.request_id != Some(record.request_id) {
                return Err(Stage5cPaperBrokerLifecycleFailure::Terminal(
                    Stage5cPaperBrokerLifecycleError::OrderRequestIdMismatch,
                ));
            }
            if let Some(expected) = &ack.broker_order_id {
                if &order.order_id != expected {
                    return Err(Stage5cPaperBrokerLifecycleFailure::Terminal(
                        Stage5cPaperBrokerLifecycleError::BrokerOrderIdMismatch,
                    ));
                }
            }
        }
        Stage5cPaperBrokerEventPayload::StopOrder(stop) => {
            if let Some(expected) = &ack.broker_order_id {
                if stop.exchange_order_id.as_ref() != Some(expected) {
                    return Err(Stage5cPaperBrokerLifecycleFailure::Terminal(
                        Stage5cPaperBrokerLifecycleError::BrokerOrderIdMismatch,
                    ));
                }
            }
            if let crate::BrokerNeutralHybridIntent::DeleteStopLimit { order_id, .. } =
                intent.base_intent()
            {
                if &stop.stop_order_id != order_id {
                    return Err(Stage5cPaperBrokerLifecycleFailure::Terminal(
                        Stage5cPaperBrokerLifecycleError::StopOrderIdMismatch,
                    ));
                }
            }
        }
        Stage5cPaperBrokerEventPayload::Position(position) => {
            if !matches!(
                intent.base_intent(),
                crate::BrokerNeutralHybridIntent::Market { .. }
            ) || !position.existing
                || position.qty == 0.0
            {
                return Err(Stage5cPaperBrokerLifecycleFailure::Terminal(
                    Stage5cPaperBrokerLifecycleError::PositionEventRequiresMarketIntent,
                ));
            }
        }
    }
    Ok(())
}

fn stage5cj_broker_lifecycle_context(
    strategy: &HybridIntradayRuntimeStrategy,
    admission: &Stage5cPaperHostAdmission,
    bar_close_ts: i64,
    event_ts_utc: i64,
) -> broker_core::HybridRuntimeStrategyContext {
    broker_core::HybridRuntimeStrategyContext {
        strategy_id: admission.strategy_id().to_string(),
        request_namespace_account: admission.account_id().clone(),
        instrument: admission.target_instrument().clone(),
        tick_size: admission.tick_size(),
        trade_mode: broker_core::HybridRuntimeTradeMode::Paper,
        paper_execution_mode: broker_core::HybridRuntimePaperExecutionMode::LiveOnly,
        allow_live_orders: false,
        gateway_phase: broker_core::HybridRuntimeGatewayPhase::LiveReady,
        position_qty: Some(strategy.stage5c_current_position_qty()),
        event_ts_utc,
        strategy_now_ts_utc: event_ts_utc,
        last_bar_ts_utc: Some(bar_close_ts),
    }
}

fn stage5cg_source_request_id(
    strategy_id: &str,
    account_id: &str,
    symbol: &str,
    bar_close_ts: i64,
    intent: &crate::BrokerNeutralHybridIntent,
) -> Result<StrategyRequestId, Stage5cIntentSettlementError> {
    use crate::BrokerNeutralHybridIntent as Intent;
    use crate::BrokerNeutralOrderSide as OrderSide;
    match intent.base_intent() {
        Intent::Place { .. } => Ok(crate::deterministic_request_id(
            strategy_id,
            account_id,
            symbol,
            "place",
            bar_close_ts,
            0,
        )),
        Intent::Cancel { .. } => Ok(crate::deterministic_request_id(
            strategy_id,
            account_id,
            symbol,
            "cancel",
            bar_close_ts,
            1,
        )),
        Intent::Replace { .. } => Ok(crate::deterministic_request_id(
            strategy_id,
            account_id,
            symbol,
            "replace",
            bar_close_ts,
            2,
        )),
        Intent::Market { side, .. } => {
            let seq = match side {
                OrderSide::Buy => 3,
                OrderSide::Sell => 4,
            };
            Ok(crate::deterministic_request_id(
                strategy_id,
                account_id,
                symbol,
                "market",
                bar_close_ts,
                seq,
            ))
        }
        Intent::CreateStopLimit { .. } => Ok(crate::deterministic_request_id(
            strategy_id,
            account_id,
            symbol,
            "create_stop_limit",
            bar_close_ts,
            5,
        )),
        Intent::DeleteStopLimit { .. } => Ok(crate::deterministic_request_id(
            strategy_id,
            account_id,
            symbol,
            "delete_stop_limit",
            bar_close_ts,
            6,
        )),
        Intent::Classified { .. } | Intent::Routed { .. } => {
            Err(Stage5cIntentSettlementError::UnsupportedIntentAction)
        }
    }
}

fn stage5cg_verify_pending_request_id(
    state: &StrategyState,
    class: crate::BrokerNeutralHybridIntentClass,
    request_id: StrategyRequestId,
) -> Result<(), Stage5cIntentSettlementError> {
    let expected = match state {
        StrategyState::HybridIntradayRuntime {
            pending_entry_request_id,
            pending_exit_request_id,
            pending_tp_request_id,
            pending_sl_request_id,
            ..
        } => match class {
            crate::BrokerNeutralHybridIntentClass::Entry => *pending_entry_request_id,
            crate::BrokerNeutralHybridIntentClass::Exit => *pending_exit_request_id,
            crate::BrokerNeutralHybridIntentClass::ProtectiveRepair => {
                if *pending_tp_request_id == Some(request_id) {
                    *pending_tp_request_id
                } else {
                    *pending_sl_request_id
                }
            }
            crate::BrokerNeutralHybridIntentClass::CancelCleanup => {
                return Ok(());
            }
        },
        _ => None,
    };
    match expected {
        Some(expected) if expected == request_id => Ok(()),
        Some(_) => Err(Stage5cIntentSettlementError::RequestIdMismatch),
        None => Err(Stage5cIntentSettlementError::MissingPendingRequest),
    }
}

fn validate_stage5cg_intent(
    intent: &crate::BrokerNeutralHybridIntent,
    symbol: &str,
    tick_size: f64,
    bar_close_ts: i64,
) -> Result<(), Stage5cIntentSettlementError> {
    if intent.explicit_class().is_none() {
        return Err(Stage5cIntentSettlementError::MissingIntentClass);
    }
    if let crate::BrokerNeutralHybridIntent::Routed {
        intent,
        symbol: routed,
    } = intent
    {
        if routed != symbol {
            return Err(Stage5cIntentSettlementError::InstrumentNamespaceMismatch);
        }
        return validate_stage5cg_intent(intent, symbol, tick_size, bar_close_ts);
    }
    if let crate::BrokerNeutralHybridIntent::Classified { intent, .. } = intent {
        return validate_stage5cg_base_intent(intent, tick_size, bar_close_ts);
    }
    Err(Stage5cIntentSettlementError::MissingIntentClass)
}

fn validate_stage5cg_base_intent(
    intent: &crate::BrokerNeutralHybridIntent,
    tick_size: f64,
    bar_close_ts: i64,
) -> Result<(), Stage5cIntentSettlementError> {
    use crate::BrokerNeutralHybridIntent as Intent;
    let qty = match intent {
        Intent::Place { qty, price, .. } => {
            validate_stage5cg_price(*price, tick_size)?;
            Some(*qty)
        }
        Intent::Market {
            qty, fill_price, ..
        } => {
            if let Some(price) = fill_price {
                validate_stage5cg_price(*price, tick_size)?;
            }
            Some(*qty)
        }
        Intent::Replace {
            new_qty, new_price, ..
        } => {
            validate_stage5cg_price(*new_price, tick_size)?;
            Some(*new_qty)
        }
        Intent::CreateStopLimit {
            qty,
            trigger_price,
            price,
            stop_end_unix_time,
            ..
        } => {
            validate_stage5cg_price(*trigger_price, tick_size)?;
            validate_stage5cg_price(*price, tick_size)?;
            if *stop_end_unix_time <= bar_close_ts {
                return Err(Stage5cIntentSettlementError::InvalidStopEnd);
            }
            Some(*qty)
        }
        Intent::Cancel { .. } | Intent::DeleteStopLimit { .. } => None,
        Intent::Classified { .. } | Intent::Routed { .. } => {
            return validate_stage5cg_intent(intent, "", tick_size, bar_close_ts)
        }
    };
    if qty.is_some_and(|value| !value.is_finite() || value <= 0.0) {
        return Err(Stage5cIntentSettlementError::InvalidQuantity);
    }
    Ok(())
}

fn validate_stage5cg_price(price: f64, tick_size: f64) -> Result<(), Stage5cIntentSettlementError> {
    if !price.is_finite() || price <= 0.0 {
        return Err(Stage5cIntentSettlementError::InvalidPrice);
    }
    let ticks = price / tick_size;
    if (ticks - ticks.round()).abs() > 1e-9 {
        return Err(Stage5cIntentSettlementError::PriceNotTickAligned);
    }
    Ok(())
}

#[cfg(test)]
mod bootstrap_notification_tests {
    use super::*;
    use broker_core::{BrokerPositionSnapshot, BrokerStopOrderId, Exchange, Market};
    use chrono::TimeZone;
    use rust_decimal::Decimal;

    use crate::hybrid_intraday::{
        HybridOrchestratorConfig, IntradayBreakoutConfig, MeanReversionConfig,
    };
    use crate::hybrid_intraday_runtime::{
        HybridIntradayProfile, HybridIntradayRuntimeConfig, MeanReversionVariant, MrGatePolicy,
        RiskGateMode,
    };
    use crate::runtime_compat::MarketBuyAndCloseLiveOrderStyle;

    fn target() -> InstrumentId {
        InstrumentId {
            symbol: "IMOEXF".to_string(),
            venue_symbol: Some("IMOEXF@RTSX".to_string()),
            exchange: Exchange::Moex,
            market: Market::Futures,
        }
    }

    fn strategy(symbol: &str, tick_size: f64) -> HybridIntradayRuntimeStrategy {
        HybridIntradayRuntimeStrategy::new(HybridIntradayRuntimeConfig {
            symbol: symbol.to_string(),
            profile: HybridIntradayProfile::BaselineRuntimeHybrid,
            mr_variant: MeanReversionVariant::ClassicPrevDayRange,
            mr_gate_policy: MrGatePolicy::Disabled,
            risk_gate_mode: RiskGateMode::Disabled,
            risk_gate_seed_file: None,
            risk_gate_ledger_key: None,
            model_session_start_time: None,
            model_session_end_time: None,
            qty: 1.0,
            live_order_style: MarketBuyAndCloseLiveOrderStyle::Market,
            tick_size,
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
            mr_config: MeanReversionConfig::default(),
            breakout_config: IntradayBreakoutConfig::default(),
            orchestrator_config: HybridOrchestratorConfig::default(),
        })
    }

    fn admission(position_qty: Decimal, expires_at: DateTime<Utc>) -> Stage5cPaperHostAdmission {
        let checked_ts = Utc
            .with_ymd_and_hms(2026, 7, 11, 9, 0, 0)
            .single()
            .expect("timestamp");
        let account_id = BrokerAccountId::new("ACC_TEST_0001");
        let target = target();
        let positions = if position_qty == Decimal::ZERO {
            Vec::new()
        } else {
            vec![BrokerPositionSnapshot {
                account_id: account_id.clone(),
                instrument: target.clone(),
                qty: position_qty,
                avg_price: Some(Decimal::new(222_750, 2)),
                unrealized_pnl: None,
                source_ts: Some(checked_ts),
                received_ts: checked_ts,
            }]
        };
        let bootstrap_snapshot = RuntimeHostBootstrapSnapshot {
            account_id: account_id.clone(),
            instrument: target.clone(),
            target_position_qty: position_qty,
            target_open_positions: positions,
            target_active_orders: Vec::new(),
            account_active_orders_count: 0,
            target_is_flat: position_qty == Decimal::ZERO,
            received_ts: checked_ts,
        };
        Stage5cPaperHostAdmission {
            schema_version: STAGE5C_PAPER_HOST_ADMISSION_SCHEMA_VERSION,
            checked_ts,
            issued_ts: checked_ts,
            expires_at,
            strategy_id: "hybrid_imoexf".to_string(),
            account_id,
            target_instrument: target,
            tick_size: 0.5,
            bootstrap_snapshot,
            paper_only: true,
            runtime_host_attached: false,
            intent_sink_attached: false,
        }
    }

    #[test]
    fn stage5cb_rechecks_expiry_before_notification_without_state_mutation() {
        let expiry = Utc
            .with_ymd_and_hms(2026, 7, 11, 9, 1, 0)
            .single()
            .expect("expiry");
        let strategy = strategy("IMOEXF", 0.5);
        let before = serde_json::to_value(Strategy::state(&strategy)).expect("state before");
        let admission = admission(Decimal::ONE, expiry);
        let result = validate_stage5cb_notification(
            &strategy,
            &admission,
            expiry + chrono::Duration::milliseconds(1),
        );
        assert!(matches!(
            result,
            Err(Stage5cBootstrapNotificationError::AdmissionExpired)
        ));
        let after = serde_json::to_value(Strategy::state(&strategy)).expect("state after");
        assert_eq!(before, after);
    }

    #[test]
    fn stage5cb_uses_exact_snapshot_and_opens_no_later_lifecycle_step() {
        let expiry = Utc
            .with_ymd_and_hms(2026, 7, 11, 9, 1, 0)
            .single()
            .expect("expiry");
        let exact_snapshot = admission(Decimal::ONE, expiry).bootstrap_snapshot().clone();
        let loaded = prepare_stage5c_without_runtime_state(
            strategy("IMOEXF", 0.5),
            admission(Decimal::ONE, expiry),
        );
        let bootstrapped = notify_stage5c_bootstrap_at(loaded, expiry)
            .expect("notification at expiry remains valid");
        let receipt = bootstrapped.receipt();

        assert_eq!(receipt.bootstrap_snapshot(), &exact_snapshot);
        assert_eq!(receipt.notified_ts(), expiry);
        assert!(!receipt.runtime_state_restored());
        assert!(!receipt.warmup_started());
        assert!(!receipt.pending_recovery_started());
        assert!(!receipt.semantic_bar_enabled());
        assert!(!receipt.intent_sink_attached());
        assert_eq!(receipt.strategy_id(), "hybrid_imoexf");
        let state = serde_json::to_value(Strategy::state(bootstrapped.strategy())).expect("state");
        assert_eq!(state["HybridIntradayRuntime"]["last_position_qty"], 1.0);
    }

    #[test]
    fn stage5cb_rejects_strategy_configured_for_another_symbol() {
        let expiry = Utc
            .with_ymd_and_hms(2026, 7, 11, 9, 1, 0)
            .single()
            .expect("expiry");
        let strategy = strategy("SBER", 0.5);
        assert_eq!(
            validate_stage5cb_notification(&strategy, &admission(Decimal::ZERO, expiry), expiry,),
            Err(Stage5cBootstrapNotificationError::StrategyTargetMismatch)
        );
    }

    #[test]
    fn stage5cb_rejects_strategy_tick_size_mismatch() {
        let expiry = Utc
            .with_ymd_and_hms(2026, 7, 11, 9, 1, 0)
            .single()
            .expect("expiry");
        let strategy = strategy("IMOEXF", 1.0);
        assert_eq!(
            validate_stage5cb_notification(&strategy, &admission(Decimal::ZERO, expiry), expiry,),
            Err(Stage5cBootstrapNotificationError::StrategyTickSizeMismatch)
        );
    }

    #[test]
    fn stage5cb_binding_error_does_not_mutate_strategy_state() {
        let expiry = Utc
            .with_ymd_and_hms(2026, 7, 11, 9, 1, 0)
            .single()
            .expect("expiry");
        let strategy = strategy("SBER", 1.0);
        let before = serde_json::to_value(Strategy::state(&strategy)).expect("state before");
        let _ = validate_stage5cb_notification(&strategy, &admission(Decimal::ONE, expiry), expiry);
        let after = serde_json::to_value(Strategy::state(&strategy)).expect("state after");
        assert_eq!(before, after);
    }

    fn restore_input(
        configured: &HybridIntradayRuntimeStrategy,
        accepted: &Stage5cPaperHostAdmission,
        persisted_ts: DateTime<Utc>,
    ) -> Stage5cRuntimeStateRestoreInput {
        let qty_decimal = accepted.bootstrap_snapshot().target_position_qty;
        let seed_loaded = prepare_stage5c_without_runtime_state(
            strategy("IMOEXF", 0.5),
            admission(qty_decimal, accepted.expires_at()),
        );
        let seeded = notify_stage5c_bootstrap_at(seed_loaded, persisted_ts).expect("seed state");
        let mut state = serde_json::to_value(Strategy::state(seeded.strategy()))
            .expect("persisted state value");
        let qty = accepted
            .bootstrap_snapshot()
            .target_position_qty
            .to_f64()
            .expect("qty");
        state["HybridIntradayRuntime"]["last_position_qty"] = serde_json::json!(qty);
        state["HybridIntradayRuntime"]["current_side"] = if qty > 0.0 {
            serde_json::json!("long")
        } else if qty < 0.0 {
            serde_json::json!("short")
        } else {
            serde_json::Value::Null
        };
        let (profile, mr_variant, mr_gate_policy, risk_gate_mode) =
            configured.stage5c_profile_binding();
        Stage5cRuntimeStateRestoreInput {
            schema_version: STAGE5C_RUNTIME_STATE_RESTORE_SCHEMA_VERSION,
            state_schema_version: 1,
            strategy_kind: "hybrid_intraday_runtime".to_string(),
            strategy_id: accepted.strategy_id().to_string(),
            account_id: accepted.account_id().clone(),
            instrument: accepted.target_instrument().clone(),
            tick_size: 0.5,
            config_fingerprint: configured.stage5c_config_fingerprint(),
            profile,
            mr_variant,
            mr_gate_policy,
            risk_gate_mode,
            persisted_ts,
            state_json: serde_json::to_string(&state).expect("state JSON"),
            known_order_ids: Vec::new(),
            pending_requests: Vec::new(),
            legacy_numeric_order_id_policy: Stage5cLegacyNumericOrderIdPolicy::Reject,
        }
    }

    #[test]
    fn stage5cc_restores_same_strategy_and_opens_no_later_gate() {
        let now = Utc
            .with_ymd_and_hms(2026, 7, 11, 9, 0, 30)
            .single()
            .expect("timestamp");
        let strategy = strategy("IMOEXF", 0.5);
        let admission = admission(Decimal::ONE, now + chrono::Duration::minutes(1));
        let input = restore_input(&strategy, &admission, now);
        let loaded = restore_stage5c_runtime_state_at(strategy, admission, input, now)
            .expect("validated load");
        let bootstrapped = notify_stage5c_bootstrap_at(loaded, now).expect("bootstrap");
        let restored = notify_stage5c_runtime_state_restored_at(bootstrapped, now)
            .expect("restore notification");

        assert!(restored.receipt().runtime_state_restored());
        assert!(!restored.receipt().warmup_started());
        assert!(!restored.receipt().pending_recovery_started());
        assert!(!restored.receipt().semantic_bar_enabled());
        assert!(!restored.receipt().intent_sink_attached());
        let state = serde_json::to_value(Strategy::state(restored.strategy())).expect("state");
        assert_eq!(state["HybridIntradayRuntime"]["last_position_qty"], 1.0);
    }

    #[test]
    fn stage5cc_rejects_state_that_overrides_broker_truth_position() {
        let now = Utc
            .with_ymd_and_hms(2026, 7, 11, 9, 0, 30)
            .single()
            .expect("timestamp");
        let strategy = strategy("IMOEXF", 0.5);
        let admission = admission(Decimal::ONE, now + chrono::Duration::minutes(1));
        let mut input = restore_input(&strategy, &admission, now);
        let mut state: serde_json::Value =
            serde_json::from_str(&input.state_json).expect("state value");
        state["HybridIntradayRuntime"]["last_position_qty"] = serde_json::json!(0.0);
        input.state_json = serde_json::to_string(&state).expect("state JSON");

        assert!(matches!(
            restore_stage5c_runtime_state_at(strategy, admission, input, now),
            Err(Stage5cRuntimeStateRestoreError::BrokerTruthPositionMismatch)
        ));
    }

    #[test]
    fn stage5cc_requires_explicit_legacy_numeric_order_id_policy() {
        let mut numeric = serde_json::json!({"tp_order_id": 123});
        normalize_legacy_order_ids(
            &mut numeric,
            Stage5cLegacyNumericOrderIdPolicy::ConvertPositiveAlorNumeric,
        )
        .expect("positive conversion");
        assert_eq!(numeric["tp_order_id"], "123");
    }

    #[test]
    fn stage5cc_rejects_short_side_for_long_broker_position() {
        let now = Utc
            .with_ymd_and_hms(2026, 7, 11, 9, 0, 30)
            .single()
            .unwrap();
        let strategy = strategy("IMOEXF", 0.5);
        let admission = admission(Decimal::ONE, now + chrono::Duration::minutes(1));
        let mut input = restore_input(&strategy, &admission, now);
        let mut state: serde_json::Value = serde_json::from_str(&input.state_json).unwrap();
        state["HybridIntradayRuntime"]["current_side"] = serde_json::json!("short");
        input.state_json = serde_json::to_string(&state).unwrap();
        assert!(matches!(
            restore_stage5c_runtime_state_at(strategy, admission, input, now),
            Err(Stage5cRuntimeStateRestoreError::BrokerTruthSideMismatch)
        ));
    }

    #[test]
    fn stage5cc_rejects_long_side_for_short_broker_position() {
        let now = Utc
            .with_ymd_and_hms(2026, 7, 11, 9, 0, 30)
            .single()
            .unwrap();
        let strategy = strategy("IMOEXF", 0.5);
        let admission = admission(Decimal::NEGATIVE_ONE, now + chrono::Duration::minutes(1));
        let mut input = restore_input(&strategy, &admission, now);
        let mut state: serde_json::Value = serde_json::from_str(&input.state_json).unwrap();
        state["HybridIntradayRuntime"]["current_side"] = serde_json::json!("long");
        input.state_json = serde_json::to_string(&state).unwrap();
        assert!(matches!(
            restore_stage5c_runtime_state_at(strategy, admission, input, now),
            Err(Stage5cRuntimeStateRestoreError::BrokerTruthSideMismatch)
        ));
    }

    #[test]
    fn stage5cc_bootstrap_removes_stale_persisted_broker_ids() {
        let now = Utc
            .with_ymd_and_hms(2026, 7, 11, 9, 0, 30)
            .single()
            .unwrap();
        let strategy = strategy("IMOEXF", 0.5);
        let admission = admission(Decimal::ONE, now + chrono::Duration::minutes(1));
        let mut input = restore_input(&strategy, &admission, now);
        let mut state: serde_json::Value = serde_json::from_str(&input.state_json).unwrap();
        state["HybridIntradayRuntime"]["tp_order_id"] = serde_json::json!("123");
        state["HybridIntradayRuntime"]["sl_stop_order_id"] = serde_json::json!("STOP-OLD");
        state["HybridIntradayRuntime"]["sl_exchange_order_id"] = serde_json::json!("456");
        input.state_json = serde_json::to_string(&state).unwrap();
        let loaded = restore_stage5c_runtime_state_at(strategy, admission, input, now).unwrap();
        let bootstrapped = notify_stage5c_bootstrap_at(loaded, now).unwrap();
        let restored = notify_stage5c_runtime_state_restored_at(bootstrapped, now).unwrap();
        let state = serde_json::to_value(Strategy::state(restored.strategy())).unwrap();
        assert!(state["HybridIntradayRuntime"]["tp_order_id"].is_null());
        assert!(state["HybridIntradayRuntime"]["sl_stop_order_id"].is_null());
        assert!(state["HybridIntradayRuntime"]["sl_exchange_order_id"].is_null());
    }

    #[test]
    fn stage5cc_legacy_numeric_conversion_and_invalid_matrix() {
        for invalid in [
            serde_json::json!(0),
            serde_json::json!(-1),
            serde_json::json!(1.5),
            serde_json::json!(u64::MAX),
        ] {
            let mut value = serde_json::json!({"tp_order_id": invalid});
            assert_eq!(
                normalize_legacy_order_ids(
                    &mut value,
                    Stage5cLegacyNumericOrderIdPolicy::ConvertPositiveAlorNumeric,
                ),
                Err(Stage5cRuntimeStateRestoreError::InvalidLegacyNumericOrderId)
            );
        }
        let mut rejected = serde_json::json!({"sl_exchange_order_id": 456});
        assert_eq!(
            normalize_legacy_order_ids(&mut rejected, Stage5cLegacyNumericOrderIdPolicy::Reject),
            Err(Stage5cRuntimeStateRestoreError::LegacyNumericOrderIdRejected)
        );
        let mut string_ids = serde_json::json!({"tp_order_id": "FINAM-123"});
        normalize_legacy_order_ids(
            &mut string_ids,
            Stage5cLegacyNumericOrderIdPolicy::ConvertPositiveAlorNumeric,
        )
        .unwrap();
        assert_eq!(string_ids["tp_order_id"], "FINAM-123");
    }

    #[test]
    fn stage5cc_full_state_converts_positive_numeric_tp_and_sl_ids() {
        let now = Utc
            .with_ymd_and_hms(2026, 7, 11, 9, 0, 30)
            .single()
            .unwrap();
        let strategy = strategy("IMOEXF", 0.5);
        let admission = admission(Decimal::ONE, now + chrono::Duration::minutes(1));
        let mut input = restore_input(&strategy, &admission, now);
        let mut state: serde_json::Value = serde_json::from_str(&input.state_json).unwrap();
        state["HybridIntradayRuntime"]["tp_order_id"] = serde_json::json!(123);
        state["HybridIntradayRuntime"]["sl_exchange_order_id"] = serde_json::json!(456);
        input.state_json = serde_json::to_string(&state).unwrap();
        input.legacy_numeric_order_id_policy =
            Stage5cLegacyNumericOrderIdPolicy::ConvertPositiveAlorNumeric;

        let loaded = restore_stage5c_runtime_state_at(strategy, admission, input, now).unwrap();
        let state = serde_json::to_value(Strategy::state(&loaded.strategy)).unwrap();
        assert_eq!(state["HybridIntradayRuntime"]["tp_order_id"], "123");
        assert_eq!(
            state["HybridIntradayRuntime"]["sl_exchange_order_id"],
            "456"
        );
    }

    fn restored_strategy(now: DateTime<Utc>) -> Stage5cRuntimeStateRestoredPaperStrategy {
        let strategy = strategy("IMOEXF", 0.5);
        let admission = admission(Decimal::ZERO, now + chrono::Duration::minutes(2));
        let input = restore_input(&strategy, &admission, now);
        let loaded = restore_stage5c_runtime_state_at(strategy, admission, input, now).unwrap();
        let bootstrapped = notify_stage5c_bootstrap_at(loaded, now).unwrap();
        notify_stage5c_runtime_state_restored_at(bootstrapped, now).unwrap()
    }

    fn history_bar(close_time_utc: i64) -> broker_core::HybridRuntimeBarEvent {
        broker_core::HybridRuntimeBarEvent {
            instrument: target(),
            close_time_utc,
            open: 2200.0,
            high: 2202.0,
            low: 2199.0,
            close: 2201.0,
            volume: 100.0,
            origin: broker_core::HybridRuntimeBarOrigin::History,
            is_final: true,
            timeframe_sec: 600,
        }
    }

    fn accepted_history(
        bars: Vec<broker_core::HybridRuntimeBarEvent>,
    ) -> Stage5cAcceptedHistoryBatch {
        accept_stage5c_history_batch(Stage5cHistoryBatchInput {
            bars,
            provenance: broker_core::Stage3StrategyBarProvenance::finam_derived_m1_to_m10_complete(
            ),
        })
        .expect("canonical history")
    }

    fn warmed_strategy(now: DateTime<Utc>) -> Stage5cWarmedPaperStrategy {
        let close_ts = Utc
            .with_ymd_and_hms(2026, 7, 10, 10, 0, 0)
            .single()
            .unwrap()
            .timestamp();
        warmup_stage5c_history_at(
            restored_strategy(now),
            accepted_history(vec![history_bar(close_ts)]),
            now,
        )
        .unwrap()
    }

    #[test]
    fn stage5cd_warms_canonical_history_without_opening_later_gates() {
        let now = Utc
            .with_ymd_and_hms(2026, 7, 11, 9, 0, 30)
            .single()
            .unwrap();
        let close_ts = Utc
            .with_ymd_and_hms(2026, 7, 10, 10, 0, 0)
            .single()
            .unwrap()
            .timestamp();
        let warmed = warmup_stage5c_history_at(
            restored_strategy(now),
            accepted_history(vec![history_bar(close_ts)]),
            now,
        )
        .unwrap();
        assert!(warmed.receipt().warmup_started());
        assert_eq!(warmed.receipt().processed_bars(), 1);
        assert!(!warmed.receipt().pending_recovery_started());
        assert!(!warmed.receipt().semantic_bar_enabled());
        assert!(!warmed.receipt().intent_sink_attached());
        let _ = Strategy::state(warmed.strategy());
    }

    #[test]
    fn stage5cd_rejects_noncanonical_history_matrix() {
        let close_ts = Utc
            .with_ymd_and_hms(2026, 7, 10, 10, 0, 0)
            .single()
            .unwrap()
            .timestamp();
        let mut wrong_timeframe = history_bar(close_ts);
        wrong_timeframe.timeframe_sec = 60;
        assert!(matches!(
            accept_stage5c_history_batch(Stage5cHistoryBatchInput {
                bars: vec![wrong_timeframe],
                provenance:
                    broker_core::Stage3StrategyBarProvenance::finam_derived_m1_to_m10_complete(),
            }),
            Err(Stage5cHistoryWarmupError::InvalidTimeframe)
        ));
        let duplicate = history_bar(close_ts);
        assert!(matches!(
            accept_stage5c_history_batch(Stage5cHistoryBatchInput {
                bars: vec![duplicate.clone(), duplicate],
                provenance:
                    broker_core::Stage3StrategyBarProvenance::finam_derived_m1_to_m10_complete(),
            }),
            Err(Stage5cHistoryWarmupError::NonMonotonicTimestamp)
        ));
        let mut forming = history_bar(close_ts);
        forming.is_final = false;
        assert!(matches!(
            accept_stage5c_history_batch(Stage5cHistoryBatchInput {
                bars: vec![forming],
                provenance:
                    broker_core::Stage3StrategyBarProvenance::finam_derived_m1_to_m10_complete(),
            }),
            Err(Stage5cHistoryWarmupError::NonFinalBar)
        ));

        let mut wrong_origin = history_bar(close_ts);
        wrong_origin.origin = broker_core::HybridRuntimeBarOrigin::Live;
        assert!(matches!(
            accept_stage5c_history_batch(Stage5cHistoryBatchInput {
                bars: vec![wrong_origin],
                provenance:
                    broker_core::Stage3StrategyBarProvenance::finam_derived_m1_to_m10_complete(),
            }),
            Err(Stage5cHistoryWarmupError::InvalidOrigin)
        ));
        let mut invalid_ohlc = history_bar(close_ts);
        invalid_ohlc.high = 2190.0;
        assert!(matches!(
            accept_stage5c_history_batch(Stage5cHistoryBatchInput {
                bars: vec![invalid_ohlc],
                provenance:
                    broker_core::Stage3StrategyBarProvenance::finam_derived_m1_to_m10_complete(),
            }),
            Err(Stage5cHistoryWarmupError::InvalidOhlc)
        ));
    }

    #[test]
    fn stage5cd_rechecks_freshness_and_lifecycle_clock() {
        let now = Utc
            .with_ymd_and_hms(2026, 7, 11, 9, 0, 30)
            .single()
            .unwrap();
        let close_ts = Utc
            .with_ymd_and_hms(2026, 7, 10, 10, 0, 0)
            .single()
            .unwrap()
            .timestamp();
        assert!(matches!(
            warmup_stage5c_history_at(
                restored_strategy(now),
                accepted_history(vec![history_bar(close_ts)]),
                now + chrono::Duration::minutes(3),
            ),
            Err(Stage5cHistoryWarmupError::BrokerTruthExpired)
        ));
        assert!(matches!(
            warmup_stage5c_history_at(
                restored_strategy(now),
                accepted_history(vec![history_bar(close_ts)]),
                now - chrono::Duration::seconds(1),
            ),
            Err(Stage5cHistoryWarmupError::LifecycleTimestampReversal)
        ));
    }

    #[test]
    fn stage5cd_rejects_unapproved_stage3_provenance_matrix() {
        let close_ts = Utc
            .with_ymd_and_hms(2026, 7, 10, 10, 0, 0)
            .single()
            .unwrap()
            .timestamp();
        for provenance in [
            broker_core::Stage3StrategyBarProvenance::raw_finam_m1(),
            broker_core::Stage3StrategyBarProvenance::finam_native_m10_pending(),
        ] {
            assert!(matches!(
                accept_stage5c_history_batch(Stage5cHistoryBatchInput {
                    bars: vec![history_bar(close_ts)],
                    provenance,
                }),
                Err(Stage5cHistoryWarmupError::Stage3ProvenanceRejected)
            ));
        }
        let mut incomplete =
            broker_core::Stage3StrategyBarProvenance::finam_derived_m1_to_m10_complete();
        incomplete.aggregation_complete = false;
        assert!(matches!(
            accept_stage5c_history_batch(Stage5cHistoryBatchInput {
                bars: vec![history_bar(close_ts)],
                provenance: incomplete,
            }),
            Err(Stage5cHistoryWarmupError::Stage3ProvenanceRejected)
        ));
        let mut gap_unproven =
            broker_core::Stage3StrategyBarProvenance::finam_derived_m1_to_m10_complete();
        gap_unproven.gap_absence_proven = false;
        assert!(matches!(
            accept_stage5c_history_batch(Stage5cHistoryBatchInput {
                bars: vec![history_bar(close_ts)],
                provenance: gap_unproven,
            }),
            Err(Stage5cHistoryWarmupError::Stage3ProvenanceRejected)
        ));
    }

    #[test]
    fn stage5cd_rejects_future_and_unrepresentable_history_timestamps() {
        let now = Utc.with_ymd_and_hms(2026, 7, 13, 9, 0, 0).single().unwrap();
        let future = now.timestamp() + 600;
        assert!(matches!(
            warmup_stage5c_history_at(
                restored_strategy(now),
                accepted_history(vec![history_bar(future)]),
                now,
            ),
            Err(Stage5cHistoryWarmupError::FutureHistoryBar)
        ));
        let unrepresentable = i64::MAX - i64::MAX.rem_euclid(600);
        assert!(matches!(
            accept_stage5c_history_batch(Stage5cHistoryBatchInput {
                bars: vec![history_bar(unrepresentable)],
                provenance:
                    broker_core::Stage3StrategyBarProvenance::finam_derived_m1_to_m10_complete(),
            }),
            Err(Stage5cHistoryWarmupError::InvalidHistoryTimestamp)
        ));
        assert!(warmup_stage5c_history_at(
            restored_strategy(now),
            accepted_history(vec![history_bar(now.timestamp())]),
            now,
        )
        .is_ok());
    }

    #[test]
    fn stage5cd_executes_remaining_history_error_matrix() {
        let close_ts = Utc
            .with_ymd_and_hms(2026, 7, 10, 10, 0, 0)
            .single()
            .unwrap()
            .timestamp();
        assert!(matches!(
            accept_stage5c_history_batch(Stage5cHistoryBatchInput {
                bars: Vec::new(),
                provenance:
                    broker_core::Stage3StrategyBarProvenance::finam_derived_m1_to_m10_complete(),
            }),
            Err(Stage5cHistoryWarmupError::EmptyHistory)
        ));
        let mut another_instrument = history_bar(close_ts + 600);
        another_instrument.instrument.symbol = "RI".to_string();
        assert!(matches!(
            accept_stage5c_history_batch(Stage5cHistoryBatchInput {
                bars: vec![history_bar(close_ts), another_instrument],
                provenance:
                    broker_core::Stage3StrategyBarProvenance::finam_derived_m1_to_m10_complete(),
            }),
            Err(Stage5cHistoryWarmupError::InstrumentMismatch)
        ));
        let mut unaligned = history_bar(close_ts + 1);
        assert!(matches!(
            accept_stage5c_history_batch(Stage5cHistoryBatchInput {
                bars: vec![unaligned.clone()],
                provenance:
                    broker_core::Stage3StrategyBarProvenance::finam_derived_m1_to_m10_complete(),
            }),
            Err(Stage5cHistoryWarmupError::UnalignedTimestamp)
        ));
        unaligned.close_time_utc = close_ts;
        unaligned.volume = -1.0;
        assert!(matches!(
            accept_stage5c_history_batch(Stage5cHistoryBatchInput {
                bars: vec![unaligned],
                provenance:
                    broker_core::Stage3StrategyBarProvenance::finam_derived_m1_to_m10_complete(),
            }),
            Err(Stage5cHistoryWarmupError::InvalidVolume)
        ));
        let saturday = Utc.with_ymd_and_hms(2026, 7, 11, 9, 0, 0).single().unwrap();
        assert!(matches!(
            warmup_stage5c_history_at(
                restored_strategy(saturday),
                accepted_history(vec![history_bar(saturday.timestamp())]),
                saturday,
            ),
            Err(Stage5cHistoryWarmupError::NoEligibleHistoryBars)
        ));
    }

    #[test]
    fn stage5cd_rejected_provenance_and_future_time_do_not_mutate_strategy() {
        let strategy = strategy("IMOEXF", 0.5);
        let before = serde_json::to_value(Strategy::state(&strategy)).unwrap();
        let close_ts = Utc
            .with_ymd_and_hms(2026, 7, 10, 10, 0, 0)
            .single()
            .unwrap()
            .timestamp();
        assert!(accept_stage5c_history_batch(Stage5cHistoryBatchInput {
            bars: vec![history_bar(close_ts)],
            provenance: broker_core::Stage3StrategyBarProvenance::raw_finam_m1(),
        })
        .is_err());
        assert_eq!(
            before,
            serde_json::to_value(Strategy::state(&strategy)).unwrap()
        );

        let now = Utc.with_ymd_and_hms(2026, 7, 13, 9, 0, 0).single().unwrap();
        let restored = restored_strategy(now);
        let before = serde_json::to_value(Strategy::state(restored.strategy())).unwrap();
        let history = accepted_history(vec![history_bar(now.timestamp() + 600)]);
        let admission = &restored.receipt().bootstrap_receipt().admission;
        assert_eq!(
            validate_stage5cd_time_boundary(&history, admission, now),
            Err(Stage5cHistoryWarmupError::FutureHistoryBar)
        );
        assert_eq!(
            before,
            serde_json::to_value(Strategy::state(restored.strategy())).unwrap()
        );
    }

    #[test]
    fn stage5ce_recovers_complete_empty_pending_set_without_opening_later_gates() {
        let now = Utc
            .with_ymd_and_hms(2026, 7, 11, 9, 0, 30)
            .single()
            .unwrap();
        let warmed = warmed_strategy(now);
        let proof = recovery_claim(&warmed, now).unwrap();
        let evidence =
            accept_stage5c_pending_recovery_evidence(Stage5cPendingRecoveryEvidenceInput {
                events: Vec::new(),
                claim_proof: proof,
            })
            .unwrap();
        let recovered =
            recover_stage5c_pending_streams_at(warmed, evidence, now).expect("empty recovery");
        assert!(recovered.receipt().pending_recovery_started());
        assert_eq!(recovered.receipt().replayed_events(), 0);
        assert!(!recovered.receipt().semantic_bar_enabled());
        assert!(!recovered.receipt().intent_sink_attached());
        let _ = Strategy::state(recovered.strategy());
    }

    #[test]
    fn stage5ce_deduplicates_identical_pending_events() {
        let now = Utc
            .with_ymd_and_hms(2026, 7, 11, 9, 0, 30)
            .single()
            .unwrap();
        let event = Stage5cPendingRecoveryEvent {
            stream_kind: Stage5cPendingStreamKind::Position,
            stream_name: "broker.positions.ACC_TEST_0001".to_string(),
            entry_id: "1-0".to_string(),
            sequence: 1,
            payload: Stage5cPendingRecoveryPayload::Position(
                broker_core::HybridRuntimePositionEvent {
                    instrument: target(),
                    qty: 0.0,
                    existing: true,
                    avg_price: 0.0,
                    source_ts_utc: now.timestamp(),
                },
            ),
        };
        let warmed = warmed_strategy(now);
        let mut proof = recovery_claim(&warmed, now).unwrap();
        proof
            .streams
            .iter_mut()
            .find(|stream| stream.stream_kind == Stage5cPendingStreamKind::Position)
            .unwrap()
            .claimed_count = 1;
        let evidence =
            accept_stage5c_pending_recovery_evidence(Stage5cPendingRecoveryEvidenceInput {
                events: vec![event.clone(), event],
                claim_proof: proof,
            })
            .unwrap();
        let recovered = recover_stage5c_pending_streams_at(warmed, evidence, now)
            .expect("deduplicated recovery");
        assert_eq!(recovered.receipt().replayed_events(), 1);
        assert_eq!(recovered.receipt().duplicate_events(), 1);
    }

    #[test]
    fn stage5ce_rejects_incomplete_and_conflicting_recovery_evidence() {
        let now = Utc
            .with_ymd_and_hms(2026, 7, 11, 9, 0, 30)
            .single()
            .unwrap();
        let warmed = warmed_strategy(now);
        assert!(matches!(
            recovery_claim_with_cursor(&warmed, now, "1-0"),
            Err(Stage5cPendingRecoveryError::ClaimBoundaryInvalid)
        ));
    }

    fn recovery_claim(
        warmed: &Stage5cWarmedPaperStrategy,
        now: DateTime<Utc>,
    ) -> Result<Stage5cPendingRecoveryClaimProof, Stage5cPendingRecoveryError> {
        recovery_claim_with_cursor(warmed, now, "0-0")
    }

    fn recovery_claim_with_cursor(
        warmed: &Stage5cWarmedPaperStrategy,
        now: DateTime<Utc>,
        cursor: &str,
    ) -> Result<Stage5cPendingRecoveryClaimProof, Stage5cPendingRecoveryError> {
        let admission = &warmed
            .receipt()
            .restore_receipt()
            .bootstrap_receipt()
            .admission;
        let names = [
            (Stage5cPendingStreamKind::Ack, "cmd.acks.ACC_TEST_0001"),
            (
                Stage5cPendingStreamKind::Order,
                "broker.orders.ACC_TEST_0001",
            ),
            (
                Stage5cPendingStreamKind::StopOrder,
                "broker.stop_orders.ACC_TEST_0001",
            ),
            (
                Stage5cPendingStreamKind::Position,
                "broker.positions.ACC_TEST_0001",
            ),
        ];
        prove_stage5c_pending_recovery_claim(
            warmed,
            Stage5cPendingRecoveryClaimProofInput {
                strategy_id: admission.strategy_id().to_string(),
                account_id: admission.account_id().clone(),
                target_instrument: admission.target_instrument().clone(),
                snapshot_received_ts: admission.bootstrap_snapshot().received_ts,
                completed_ts: now,
                streams: names
                    .into_iter()
                    .map(
                        |(stream_kind, stream_name)| Stage5cPendingStreamClaimBoundary {
                            stream_kind,
                            stream_name: stream_name.to_string(),
                            consumer_group: "paper-runtime:ACC_TEST_0001:hybrid_imoexf".to_string(),
                            terminal_claim_cursor: cursor.to_string(),
                            snapshot_boundary_entry_id: "0-0".to_string(),
                            claimed_count: 0,
                        },
                    )
                    .collect(),
            },
        )
    }

    fn semantic_input(close_ts: i64) -> Stage5cSemanticBarInput {
        let mut bar = history_bar(close_ts);
        bar.origin = broker_core::HybridRuntimeBarOrigin::Live;
        Stage5cSemanticBarInput {
            bar,
            provenance: broker_core::Stage3StrategyBarProvenance::finam_derived_m1_to_m10_complete(
            ),
            tick_size: 0.5,
        }
    }

    #[test]
    fn stage5cf_validates_actual_payload_matrix() {
        let close_ts = Utc
            .with_ymd_and_hms(2026, 7, 13, 10, 0, 0)
            .single()
            .unwrap()
            .timestamp();
        assert!(accept_stage5c_semantic_bar(semantic_input(close_ts)).is_ok());
        assert!(matches!(
            accept_stage5c_semantic_bar(semantic_input(close_ts + 1)),
            Err(Stage5cSemanticBarError::UnalignedTimestamp)
        ));
        let mut invalid = semantic_input(close_ts);
        invalid.bar.high = 2190.0;
        assert!(matches!(
            accept_stage5c_semantic_bar(invalid),
            Err(Stage5cSemanticBarError::InvalidOhlc)
        ));
        let mut non_finite = semantic_input(close_ts);
        non_finite.bar.close = f64::NAN;
        assert!(matches!(
            accept_stage5c_semantic_bar(non_finite),
            Err(Stage5cSemanticBarError::InvalidOhlc)
        ));
        let mut volume = semantic_input(close_ts);
        volume.bar.volume = -1.0;
        assert!(matches!(
            accept_stage5c_semantic_bar(volume),
            Err(Stage5cSemanticBarError::InvalidVolume)
        ));
    }

    #[test]
    fn stage5cf_context_uses_current_strategy_position() {
        let now = Utc.with_ymd_and_hms(2026, 7, 13, 9, 0, 0).single().unwrap();
        let admission = admission(Decimal::ZERO, now + chrono::Duration::minutes(20));
        let mut strategy = strategy("IMOEXF", 0.5);
        let context = StrategyCtx {
            strategy_id: "hybrid_imoexf".to_string(),
            portfolio: "ACC_TEST_0001".to_string(),
            exchange: "Moex".to_string(),
            symbol: "IMOEXF".to_string(),
            tick_size: 0.5,
            trade_mode: TradeMode::Paper,
            paper_execution_mode: PaperExecutionMode::LiveOnly,
            allow_live_orders: false,
            gateway_phase: GatewayPhase::SyncingGap,
            position_qty: Some(1.0),
            event_ts_utc: now.timestamp(),
            now_ts_utc: now.timestamp(),
            last_bar_ts: None,
        };
        Strategy::on_position(
            &mut strategy,
            &context,
            &PositionEvent {
                symbol: "IMOEXF".to_string(),
                qty: 1.0,
                existing: true,
                avg_price: 2200.0,
                ts_utc: now.timestamp(),
            },
        );
        let semantic = stage5cf_semantic_context(&strategy, &admission, now.timestamp(), now);
        assert_eq!(semantic.position_qty, Some(1.0));
    }

    fn empty_recovered(now: DateTime<Utc>) -> Stage5cPendingRecoveredPaperStrategy {
        let warmed = warmed_strategy(now);
        let proof = recovery_claim(&warmed, now).unwrap();
        let evidence =
            accept_stage5c_pending_recovery_evidence(Stage5cPendingRecoveryEvidenceInput {
                events: Vec::new(),
                claim_proof: proof,
            })
            .unwrap();
        recover_stage5c_pending_streams_at(warmed, evidence, now).unwrap()
    }

    fn empty_recovered_until(
        now: DateTime<Utc>,
        expires_at: DateTime<Utc>,
    ) -> Stage5cPendingRecoveredPaperStrategy {
        let strategy = strategy("IMOEXF", 0.5);
        let admission = admission(Decimal::ZERO, expires_at);
        let input = restore_input(&strategy, &admission, now);
        let loaded = restore_stage5c_runtime_state_at(strategy, admission, input, now).unwrap();
        let bootstrapped = notify_stage5c_bootstrap_at(loaded, now).unwrap();
        let restored = notify_stage5c_runtime_state_restored_at(bootstrapped, now).unwrap();
        let warmed = warmup_stage5c_history_at(
            restored,
            accepted_history(vec![history_bar(
                Utc.with_ymd_and_hms(2026, 7, 10, 10, 0, 0)
                    .single()
                    .unwrap()
                    .timestamp(),
            )]),
            now,
        )
        .unwrap();
        let proof = recovery_claim(&warmed, now).unwrap();
        let evidence =
            accept_stage5c_pending_recovery_evidence(Stage5cPendingRecoveryEvidenceInput {
                events: Vec::new(),
                claim_proof: proof,
            })
            .unwrap();
        recover_stage5c_pending_streams_at(warmed, evidence, now).unwrap()
    }

    fn set_hybrid_pending_request(
        strategy: &mut HybridIntradayRuntimeStrategy,
        class: crate::BrokerNeutralHybridIntentClass,
        request_id: StrategyRequestId,
    ) {
        let mut state = Strategy::state(strategy).clone();
        match &mut state {
            StrategyState::HybridIntradayRuntime {
                pending_entry_request_id,
                pending_exit_request_id,
                pending_tp_request_id,
                ..
            } => match class {
                crate::BrokerNeutralHybridIntentClass::Entry => {
                    *pending_entry_request_id = Some(request_id);
                }
                crate::BrokerNeutralHybridIntentClass::Exit => {
                    *pending_exit_request_id = Some(request_id);
                }
                crate::BrokerNeutralHybridIntentClass::ProtectiveRepair => {
                    *pending_tp_request_id = Some(request_id);
                }
                crate::BrokerNeutralHybridIntentClass::CancelCleanup => {}
            },
            StrategyState::Idle => panic!("expected hybrid runtime state"),
        }
        Strategy::set_state(strategy, state);
    }

    fn set_hybrid_pending_sl_request(
        strategy: &mut HybridIntradayRuntimeStrategy,
        request_id: StrategyRequestId,
    ) {
        let mut state = Strategy::state(strategy).clone();
        match &mut state {
            StrategyState::HybridIntradayRuntime {
                pending_sl_request_id,
                ..
            } => {
                *pending_sl_request_id = Some(request_id);
            }
            StrategyState::Idle => panic!("expected hybrid runtime state"),
        }
        Strategy::set_state(strategy, state);
    }

    fn stage5cg_semantic_result(
        strategy: HybridIntradayRuntimeStrategy,
        recovery_receipt: Stage5cPendingRecoveryReceipt,
        bar_close_ts: i64,
        origin: broker_core::HybridRuntimeBarOrigin,
        intents: Vec<crate::BrokerNeutralHybridIntent>,
    ) -> Stage5cSemanticBarResult {
        Stage5cSemanticBarResult {
            strategy,
            recovery_receipt,
            bar_close_ts,
            origin,
            execution_eligible: origin == broker_core::HybridRuntimeBarOrigin::Live,
            intents,
        }
    }

    fn stage5cg_market_intent(
        side: crate::BrokerNeutralOrderSide,
        class: crate::BrokerNeutralHybridIntentClass,
    ) -> crate::BrokerNeutralHybridIntent {
        crate::BrokerNeutralHybridIntent::Market {
            qty: 1.0,
            side,
            fill_price: Some(2227.5),
            comment: None,
        }
        .with_class(class)
        .with_symbol("IMOEXF")
    }

    fn stage5cg_place_intent() -> crate::BrokerNeutralHybridIntent {
        crate::BrokerNeutralHybridIntent::Place {
            price: 2230.0,
            qty: 1.0,
            side: crate::BrokerNeutralOrderSide::Sell,
            comment: None,
        }
        .with_class(crate::BrokerNeutralHybridIntentClass::ProtectiveRepair)
        .with_symbol("IMOEXF")
    }

    fn stage5cg_stop_intent(stop_end_unix_time: i64) -> crate::BrokerNeutralHybridIntent {
        crate::BrokerNeutralHybridIntent::CreateStopLimit {
            side: crate::BrokerNeutralOrderSide::Sell,
            qty: 1.0,
            trigger_price: 2210.0,
            price: 2209.5,
            condition: crate::runtime_compat::StopLimitCondition::LessOrEqual,
            stop_end_unix_time,
            comment: None,
            instrument_group: None,
            check_duplicates: Some(true),
        }
        .with_class(crate::BrokerNeutralHybridIntentClass::ProtectiveRepair)
        .with_symbol("IMOEXF")
    }

    fn stage5cg_cancel_intent() -> crate::BrokerNeutralHybridIntent {
        crate::BrokerNeutralHybridIntent::Cancel {
            order_id: BrokerOrderId::new("ORDER_TEST_0001"),
        }
        .with_class(crate::BrokerNeutralHybridIntentClass::CancelCleanup)
        .with_symbol("IMOEXF")
    }

    fn stage5ci_ack(request_id: StrategyRequestId) -> broker_core::HybridRuntimeCommandAck {
        stage5ci_ack_with(
            request_id,
            broker_core::HybridRuntimeAckStatus::Accepted,
            Utc.with_ymd_and_hms(2026, 7, 13, 9, 10, 1)
                .single()
                .unwrap()
                .timestamp(),
        )
    }

    fn stage5ci_ack_with(
        request_id: StrategyRequestId,
        status: broker_core::HybridRuntimeAckStatus,
        processed_ts_utc: i64,
    ) -> broker_core::HybridRuntimeCommandAck {
        broker_core::HybridRuntimeCommandAck {
            request_id,
            status,
            broker_order_id: Some(BrokerOrderId::new("ORDER_TEST_ACK_0001")),
            error_code: None,
            error_message: None,
            processed_ts_utc,
        }
    }

    fn stage5ci_ack_record(
        total_sequence: u64,
        request_id: StrategyRequestId,
    ) -> Stage5cPaperAckRecord {
        Stage5cPaperAckRecord {
            total_sequence,
            ack: stage5ci_ack(request_id),
        }
    }

    fn stage5cj_position_event(
        total_sequence: u64,
        request_id: StrategyRequestId,
        qty: f64,
        source_ts_utc: i64,
    ) -> Stage5cPaperBrokerEventRecord {
        Stage5cPaperBrokerEventRecord {
            total_sequence,
            request_id,
            payload: Stage5cPaperBrokerEventPayload::Position(
                broker_core::HybridRuntimePositionEvent {
                    instrument: target(),
                    qty,
                    existing: true,
                    avg_price: 2227.5,
                    source_ts_utc,
                },
            ),
        }
    }

    fn stage5cj_order_event(
        total_sequence: u64,
        request_id: StrategyRequestId,
        order_id: BrokerOrderId,
        source_ts_utc: i64,
    ) -> Stage5cPaperBrokerEventRecord {
        Stage5cPaperBrokerEventRecord {
            total_sequence,
            request_id,
            payload: Stage5cPaperBrokerEventPayload::Order(broker_core::HybridRuntimeOrderEvent {
                order_id,
                request_id: Some(request_id),
                instrument: target(),
                status: "working".to_string(),
                side: "sell".to_string(),
                order_type: "limit".to_string(),
                qty: 1.0,
                filled_qty: 0.0,
                price: 2230.0,
                existing: true,
                attribution: None,
                source_ts_utc,
            }),
        }
    }

    fn stage5cj_stop_event(
        total_sequence: u64,
        request_id: StrategyRequestId,
        exchange_order_id: BrokerOrderId,
        source_ts_utc: i64,
    ) -> Stage5cPaperBrokerEventRecord {
        Stage5cPaperBrokerEventRecord {
            total_sequence,
            request_id,
            payload: Stage5cPaperBrokerEventPayload::StopOrder(
                broker_core::HybridRuntimeStopOrderEvent {
                    stop_order_id: BrokerStopOrderId::new("STOP_TEST_0001"),
                    exchange_order_id: Some(exchange_order_id),
                    instrument: target(),
                    status: "working".to_string(),
                    side: "sell".to_string(),
                    qty: 1.0,
                    filled_qty: 0.0,
                    stop_price: 2210.0,
                    price: 2209.5,
                    existing: true,
                    attribution: None,
                    end_ts_utc: Some(source_ts_utc + 600),
                    source_ts_utc,
                },
            ),
        }
    }

    fn stage5ci_entry_settled() -> (Stage5cSettledPaperStrategy, StrategyRequestId, i64) {
        let now = Utc
            .with_ymd_and_hms(2026, 7, 13, 9, 0, 30)
            .single()
            .unwrap();
        let recovered = empty_recovered_until(
            now,
            Utc.with_ymd_and_hms(2026, 7, 13, 9, 40, 30)
                .single()
                .unwrap(),
        );
        let (mut strategy, recovery_receipt) = recovered.into_parts();
        let bar_close_ts = Utc
            .with_ymd_and_hms(2026, 7, 13, 9, 10, 0)
            .single()
            .unwrap()
            .timestamp();
        let expected_request_id = crate::deterministic_request_id(
            "hybrid_imoexf",
            "ACC_TEST_0001",
            "IMOEXF",
            "market",
            bar_close_ts,
            3,
        );
        set_hybrid_pending_request(
            &mut strategy,
            crate::BrokerNeutralHybridIntentClass::Entry,
            expected_request_id,
        );
        let settled = settle_stage5c_semantic_result(stage5cg_semantic_result(
            strategy,
            recovery_receipt,
            bar_close_ts,
            broker_core::HybridRuntimeBarOrigin::Live,
            vec![stage5cg_market_intent(
                crate::BrokerNeutralOrderSide::Buy,
                crate::BrokerNeutralHybridIntentClass::Entry,
            )],
        ))
        .unwrap();
        (settled, expected_request_id, bar_close_ts)
    }

    fn stage5ci_protective_settled() -> (
        Stage5cSettledPaperStrategy,
        StrategyRequestId,
        StrategyRequestId,
        i64,
    ) {
        let now = Utc
            .with_ymd_and_hms(2026, 7, 13, 9, 0, 30)
            .single()
            .unwrap();
        let recovered = empty_recovered_until(
            now,
            Utc.with_ymd_and_hms(2026, 7, 13, 9, 40, 30)
                .single()
                .unwrap(),
        );
        let (mut strategy, recovery_receipt) = recovered.into_parts();
        let bar_close_ts = Utc
            .with_ymd_and_hms(2026, 7, 13, 9, 10, 0)
            .single()
            .unwrap()
            .timestamp();
        let tp_expected = crate::deterministic_request_id(
            "hybrid_imoexf",
            "ACC_TEST_0001",
            "IMOEXF",
            "place",
            bar_close_ts,
            0,
        );
        let sl_expected = crate::deterministic_request_id(
            "hybrid_imoexf",
            "ACC_TEST_0001",
            "IMOEXF",
            "create_stop_limit",
            bar_close_ts,
            5,
        );
        set_hybrid_pending_request(
            &mut strategy,
            crate::BrokerNeutralHybridIntentClass::ProtectiveRepair,
            tp_expected,
        );
        set_hybrid_pending_sl_request(&mut strategy, sl_expected);
        let settled = settle_stage5c_semantic_result(stage5cg_semantic_result(
            strategy,
            recovery_receipt,
            bar_close_ts,
            broker_core::HybridRuntimeBarOrigin::Live,
            vec![
                stage5cg_place_intent(),
                stage5cg_stop_intent(bar_close_ts + 600),
            ],
        ))
        .unwrap();
        (settled, tp_expected, sl_expected, bar_close_ts)
    }

    #[test]
    fn stage5cg_settles_zero_intent_result_without_sink() {
        let now = Utc
            .with_ymd_and_hms(2026, 7, 11, 9, 0, 30)
            .single()
            .unwrap();
        let recovered = empty_recovered(now);
        let (strategy, recovery_receipt) = recovered.into_parts();
        let settled = settle_stage5c_semantic_result(Stage5cSemanticBarResult {
            strategy,
            recovery_receipt,
            bar_close_ts: now.timestamp() + 600,
            origin: broker_core::HybridRuntimeBarOrigin::Live,
            execution_eligible: true,
            intents: Vec::new(),
        })
        .unwrap();
        assert_eq!(settled.intent_batch().intent_count(), 0);
        assert!(settled.intent_batch().request_ids().is_empty());
        assert!(!settled.intent_batch().observation_only());
        assert!(!settled.intent_sink_attached());
        assert!(!settled.broker_transport_attached());
        let _ = Strategy::state(settled.strategy());
    }

    #[test]
    fn stage5cg_rejects_invalid_intent_before_settlement() {
        let now = Utc
            .with_ymd_and_hms(2026, 7, 11, 9, 0, 30)
            .single()
            .unwrap();
        let recovered = empty_recovered_until(
            now,
            Utc.with_ymd_and_hms(2026, 7, 13, 9, 40, 30)
                .single()
                .unwrap(),
        );
        let (strategy, recovery_receipt) = recovered.into_parts();
        let intent = crate::BrokerNeutralHybridIntent::Market {
            qty: -1.0,
            side: crate::BrokerNeutralOrderSide::Buy,
            fill_price: None,
            comment: None,
        }
        .with_class(crate::BrokerNeutralHybridIntentClass::Entry)
        .with_symbol("IMOEXF");
        assert!(matches!(
            settle_stage5c_semantic_result(Stage5cSemanticBarResult {
                strategy,
                recovery_receipt,
                bar_close_ts: now.timestamp() + 600,
                origin: broker_core::HybridRuntimeBarOrigin::Live,
                execution_eligible: true,
                intents: vec![intent],
            }),
            Err(Stage5cIntentSettlementError::InvalidQuantity)
        ));
    }

    #[test]
    fn stage5cg_live_entry_batch_id_matches_pending_entry_request_id() {
        let now = Utc
            .with_ymd_and_hms(2026, 7, 11, 9, 0, 30)
            .single()
            .unwrap();
        let recovered = empty_recovered(now);
        let (mut strategy, recovery_receipt) = recovered.into_parts();
        let bar_close_ts = now.timestamp() + 600;
        let expected = crate::deterministic_request_id(
            "hybrid_imoexf",
            "ACC_TEST_0001",
            "IMOEXF",
            "market",
            bar_close_ts,
            3,
        );
        set_hybrid_pending_request(
            &mut strategy,
            crate::BrokerNeutralHybridIntentClass::Entry,
            expected,
        );
        let settled = settle_stage5c_semantic_result(stage5cg_semantic_result(
            strategy,
            recovery_receipt,
            bar_close_ts,
            broker_core::HybridRuntimeBarOrigin::Live,
            vec![stage5cg_market_intent(
                crate::BrokerNeutralOrderSide::Buy,
                crate::BrokerNeutralHybridIntentClass::Entry,
            )],
        ))
        .unwrap();
        assert_eq!(settled.intent_batch().request_ids(), &[expected]);
        assert_eq!(
            settled.intent_batch().request_ids().first().copied(),
            match Strategy::state(settled.strategy()) {
                StrategyState::HybridIntradayRuntime {
                    pending_entry_request_id,
                    ..
                } => *pending_entry_request_id,
                StrategyState::Idle => None,
            }
        );
    }

    #[test]
    fn stage5cg_live_exit_batch_id_matches_pending_exit_request_id() {
        let now = Utc
            .with_ymd_and_hms(2026, 7, 11, 9, 0, 30)
            .single()
            .unwrap();
        let recovered = empty_recovered(now);
        let (mut strategy, recovery_receipt) = recovered.into_parts();
        let bar_close_ts = now.timestamp() + 600;
        let expected = crate::deterministic_request_id(
            "hybrid_imoexf",
            "ACC_TEST_0001",
            "IMOEXF",
            "market",
            bar_close_ts,
            4,
        );
        set_hybrid_pending_request(
            &mut strategy,
            crate::BrokerNeutralHybridIntentClass::Exit,
            expected,
        );
        let settled = settle_stage5c_semantic_result(stage5cg_semantic_result(
            strategy,
            recovery_receipt,
            bar_close_ts,
            broker_core::HybridRuntimeBarOrigin::Live,
            vec![stage5cg_market_intent(
                crate::BrokerNeutralOrderSide::Sell,
                crate::BrokerNeutralHybridIntentClass::Exit,
            )],
        ))
        .unwrap();
        assert_eq!(settled.intent_batch().request_ids(), &[expected]);
    }

    #[test]
    fn stage5cg_protective_tp_sl_ids_match_wrapper_pending_ids() {
        let now = Utc
            .with_ymd_and_hms(2026, 7, 11, 9, 0, 30)
            .single()
            .unwrap();
        let recovered = empty_recovered(now);
        let (mut strategy, recovery_receipt) = recovered.into_parts();
        let bar_close_ts = now.timestamp() + 600;
        let tp_expected = crate::deterministic_request_id(
            "hybrid_imoexf",
            "ACC_TEST_0001",
            "IMOEXF",
            "place",
            bar_close_ts,
            0,
        );
        let sl_expected = crate::deterministic_request_id(
            "hybrid_imoexf",
            "ACC_TEST_0001",
            "IMOEXF",
            "create_stop_limit",
            bar_close_ts,
            5,
        );
        set_hybrid_pending_request(
            &mut strategy,
            crate::BrokerNeutralHybridIntentClass::ProtectiveRepair,
            tp_expected,
        );
        set_hybrid_pending_sl_request(&mut strategy, sl_expected);
        let settled = settle_stage5c_semantic_result(stage5cg_semantic_result(
            strategy,
            recovery_receipt,
            bar_close_ts,
            broker_core::HybridRuntimeBarOrigin::Live,
            vec![
                stage5cg_place_intent(),
                stage5cg_stop_intent(bar_close_ts + 600),
            ],
        ))
        .unwrap();
        assert_eq!(
            settled.intent_batch().request_ids(),
            &[tp_expected, sl_expected]
        );
    }

    #[test]
    fn stage5cg_replay_intent_is_blocked() {
        let now = Utc
            .with_ymd_and_hms(2026, 7, 11, 9, 0, 30)
            .single()
            .unwrap();
        let recovered = empty_recovered(now);
        let (mut strategy, recovery_receipt) = recovered.into_parts();
        let bar_close_ts = now.timestamp() + 600;
        let expected = crate::deterministic_request_id(
            "hybrid_imoexf",
            "ACC_TEST_0001",
            "IMOEXF",
            "market",
            bar_close_ts,
            3,
        );
        set_hybrid_pending_request(
            &mut strategy,
            crate::BrokerNeutralHybridIntentClass::Entry,
            expected,
        );
        assert!(matches!(
            settle_stage5c_semantic_result(stage5cg_semantic_result(
                strategy,
                recovery_receipt,
                bar_close_ts,
                broker_core::HybridRuntimeBarOrigin::Replay,
                vec![stage5cg_market_intent(
                    crate::BrokerNeutralOrderSide::Buy,
                    crate::BrokerNeutralHybridIntentClass::Entry,
                )],
            )),
            Err(Stage5cIntentSettlementError::ReplayIntentNotExecutable)
        ));
    }

    #[test]
    fn stage5cg_source_request_id_collision_is_blocked_not_hidden_by_index() {
        let now = Utc
            .with_ymd_and_hms(2026, 7, 11, 9, 0, 30)
            .single()
            .unwrap();
        let recovered = empty_recovered(now);
        let (mut strategy, recovery_receipt) = recovered.into_parts();
        let bar_close_ts = now.timestamp() + 600;
        let expected = crate::deterministic_request_id(
            "hybrid_imoexf",
            "ACC_TEST_0001",
            "IMOEXF",
            "place",
            bar_close_ts,
            0,
        );
        set_hybrid_pending_request(
            &mut strategy,
            crate::BrokerNeutralHybridIntentClass::ProtectiveRepair,
            expected,
        );
        assert!(matches!(
            settle_stage5c_semantic_result(stage5cg_semantic_result(
                strategy,
                recovery_receipt,
                bar_close_ts,
                broker_core::HybridRuntimeBarOrigin::Live,
                vec![stage5cg_place_intent(), stage5cg_place_intent()],
            )),
            Err(Stage5cIntentSettlementError::DuplicateRequestId)
        ));
    }

    #[test]
    fn stage5cg_nonzero_valid_intent_batch_preserves_state_fingerprint() {
        let now = Utc
            .with_ymd_and_hms(2026, 7, 11, 9, 0, 30)
            .single()
            .unwrap();
        let recovered = empty_recovered(now);
        let (mut strategy, recovery_receipt) = recovered.into_parts();
        let bar_close_ts = now.timestamp() + 600;
        let expected = crate::deterministic_request_id(
            "hybrid_imoexf",
            "ACC_TEST_0001",
            "IMOEXF",
            "market",
            bar_close_ts,
            3,
        );
        set_hybrid_pending_request(
            &mut strategy,
            crate::BrokerNeutralHybridIntentClass::Entry,
            expected,
        );
        let expected_fingerprint = format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(Strategy::state(&strategy)).unwrap())
        );
        let settled = settle_stage5c_semantic_result(stage5cg_semantic_result(
            strategy,
            recovery_receipt,
            bar_close_ts,
            broker_core::HybridRuntimeBarOrigin::Live,
            vec![stage5cg_market_intent(
                crate::BrokerNeutralOrderSide::Buy,
                crate::BrokerNeutralHybridIntentClass::Entry,
            )],
        ))
        .unwrap();
        assert_eq!(
            settled.intent_batch().state_fingerprint(),
            expected_fingerprint
        );
    }

    #[test]
    fn stage5ch_controlled_next_bar_requires_settled_input_and_accumulates_history() {
        let now = Utc
            .with_ymd_and_hms(2026, 7, 13, 9, 0, 30)
            .single()
            .unwrap();
        let recovered = empty_recovered_until(
            now,
            Utc.with_ymd_and_hms(2026, 7, 13, 9, 40, 30)
                .single()
                .unwrap(),
        );
        let (strategy, recovery_receipt) = recovered.into_parts();
        let first_close_ts = Utc
            .with_ymd_and_hms(2026, 7, 13, 9, 10, 0)
            .single()
            .unwrap()
            .timestamp();
        let settled = settle_stage5c_semantic_result(Stage5cSemanticBarResult {
            strategy,
            recovery_receipt,
            bar_close_ts: first_close_ts,
            origin: broker_core::HybridRuntimeBarOrigin::Live,
            execution_eligible: true,
            intents: Vec::new(),
        })
        .unwrap();
        assert_eq!(settled.settled_batch_history().len(), 1);
        let next_close_ts = Utc
            .with_ymd_and_hms(2026, 7, 13, 9, 20, 0)
            .single()
            .unwrap()
            .timestamp();
        let accepted = accept_stage5c_semantic_bar(semantic_input(next_close_ts)).unwrap();
        let advanced = advance_stage5c_controlled_next_bar_at(
            settled,
            accepted,
            Utc.with_ymd_and_hms(2026, 7, 13, 9, 20, 30)
                .single()
                .unwrap(),
        )
        .unwrap();
        assert_eq!(advanced.intent_batch().bar_close_ts(), next_close_ts);
        assert_eq!(advanced.settled_batch_history().len(), 2);
        assert_eq!(
            advanced.settled_batch_history()[0].bar_close_ts,
            first_close_ts
        );
        assert_eq!(
            advanced.settled_batch_history()[1].bar_close_ts,
            next_close_ts
        );
        assert!(!advanced.intent_sink_attached());
        assert!(!advanced.broker_transport_attached());
        assert!(!advanced.timer_path_enabled());
    }

    #[test]
    fn stage5ch_rejects_non_monotonic_next_bar_before_callback() {
        let now = Utc
            .with_ymd_and_hms(2026, 7, 13, 9, 0, 30)
            .single()
            .unwrap();
        let recovered = empty_recovered(now);
        let (strategy, recovery_receipt) = recovered.into_parts();
        let first_close_ts = Utc
            .with_ymd_and_hms(2026, 7, 13, 9, 10, 0)
            .single()
            .unwrap()
            .timestamp();
        let settled = settle_stage5c_semantic_result(Stage5cSemanticBarResult {
            strategy,
            recovery_receipt,
            bar_close_ts: first_close_ts,
            origin: broker_core::HybridRuntimeBarOrigin::Live,
            execution_eligible: true,
            intents: Vec::new(),
        })
        .unwrap();
        let accepted = accept_stage5c_semantic_bar(semantic_input(first_close_ts)).unwrap();
        let failure = advance_stage5c_controlled_next_bar_at(
            settled,
            accepted,
            Utc.with_ymd_and_hms(2026, 7, 13, 9, 20, 30)
                .single()
                .unwrap(),
        )
        .expect_err("non-monotonic bar must be blocked before callback");
        assert_eq!(failure.reason(), Stage5cNextBarLoopError::NonMonotonicBar);
        assert!(failure.into_blocked().is_some());
    }

    #[test]
    fn stage5ch_nonzero_live_batch_blocks_next_bar_and_preserves_full_intent_batch() {
        let now = Utc
            .with_ymd_and_hms(2026, 7, 13, 9, 0, 30)
            .single()
            .unwrap();
        let recovered = empty_recovered_until(
            now,
            Utc.with_ymd_and_hms(2026, 7, 13, 9, 40, 30)
                .single()
                .unwrap(),
        );
        let (mut strategy, recovery_receipt) = recovered.into_parts();
        let first_close_ts = Utc
            .with_ymd_and_hms(2026, 7, 13, 9, 10, 0)
            .single()
            .unwrap()
            .timestamp();
        let expected_request_id = crate::deterministic_request_id(
            "hybrid_imoexf",
            "ACC_TEST_0001",
            "IMOEXF",
            "market",
            first_close_ts,
            3,
        );
        set_hybrid_pending_request(
            &mut strategy,
            crate::BrokerNeutralHybridIntentClass::Entry,
            expected_request_id,
        );
        let expected_state_fingerprint = format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(Strategy::state(&strategy)).unwrap())
        );
        let settled = settle_stage5c_semantic_result(stage5cg_semantic_result(
            strategy,
            recovery_receipt,
            first_close_ts,
            broker_core::HybridRuntimeBarOrigin::Live,
            vec![stage5cg_market_intent(
                crate::BrokerNeutralOrderSide::Buy,
                crate::BrokerNeutralHybridIntentClass::Entry,
            )],
        ))
        .unwrap();
        assert_eq!(settled.intent_batch().intent_count(), 1);
        let next_close_ts = Utc
            .with_ymd_and_hms(2026, 7, 13, 9, 20, 0)
            .single()
            .unwrap()
            .timestamp();
        let accepted = accept_stage5c_semantic_bar(semantic_input(next_close_ts)).unwrap();
        let failure = advance_stage5c_controlled_next_bar_at(
            settled,
            accepted,
            Utc.with_ymd_and_hms(2026, 7, 13, 9, 20, 30)
                .single()
                .unwrap(),
        )
        .expect_err("unresolved live intent batch must block the next bar");
        assert_eq!(
            failure.reason(),
            Stage5cNextBarLoopError::UnresolvedIntentBatch
        );
        let blocked = failure
            .into_blocked()
            .expect("unresolved batch must preserve settled type-state");
        assert_eq!(
            blocked.reason(),
            Stage5cNextBarLoopError::UnresolvedIntentBatch
        );
        assert_eq!(blocked.settled().intent_batch().intent_count(), 1);
        assert_eq!(
            blocked.settled().intent_batch().request_ids(),
            &[expected_request_id]
        );
        assert_eq!(
            blocked.settled().intent_batch().record_request_ids(),
            vec![expected_request_id]
        );
        assert_eq!(
            blocked.settled().intent_batch().intent_classes(),
            vec![crate::BrokerNeutralHybridIntentClass::Entry]
        );
        assert_eq!(
            blocked.settled().intent_batch().state_fingerprint(),
            expected_state_fingerprint
        );
        assert_eq!(
            match Strategy::state(blocked.settled().strategy()) {
                StrategyState::HybridIntradayRuntime {
                    pending_entry_request_id,
                    ..
                } => *pending_entry_request_id,
                StrategyState::Idle => None,
            },
            Some(expected_request_id)
        );
    }

    #[test]
    fn stage5ch_unresolved_batch_does_not_invoke_on_broker_bar_or_change_strategy_state() {
        let now = Utc
            .with_ymd_and_hms(2026, 7, 13, 9, 0, 30)
            .single()
            .unwrap();
        let recovered = empty_recovered_until(
            now,
            Utc.with_ymd_and_hms(2026, 7, 13, 9, 40, 30)
                .single()
                .unwrap(),
        );
        let (mut strategy, recovery_receipt) = recovered.into_parts();
        let first_close_ts = Utc
            .with_ymd_and_hms(2026, 7, 13, 9, 10, 0)
            .single()
            .unwrap()
            .timestamp();
        let expected_request_id = crate::deterministic_request_id(
            "hybrid_imoexf",
            "ACC_TEST_0001",
            "IMOEXF",
            "market",
            first_close_ts,
            3,
        );
        set_hybrid_pending_request(
            &mut strategy,
            crate::BrokerNeutralHybridIntentClass::Entry,
            expected_request_id,
        );
        let settled = settle_stage5c_semantic_result(stage5cg_semantic_result(
            strategy,
            recovery_receipt,
            first_close_ts,
            broker_core::HybridRuntimeBarOrigin::Live,
            vec![stage5cg_market_intent(
                crate::BrokerNeutralOrderSide::Buy,
                crate::BrokerNeutralHybridIntentClass::Entry,
            )],
        ))
        .unwrap();
        let before_state = serde_json::to_value(Strategy::state(settled.strategy())).unwrap();
        let next_close_ts = Utc
            .with_ymd_and_hms(2026, 7, 13, 9, 20, 0)
            .single()
            .unwrap()
            .timestamp();
        let accepted = accept_stage5c_semantic_bar(semantic_input(next_close_ts)).unwrap();
        let blocked = advance_stage5c_controlled_next_bar_at(
            settled,
            accepted,
            Utc.with_ymd_and_hms(2026, 7, 13, 9, 20, 30)
                .single()
                .unwrap(),
        )
        .expect_err("unresolved live intent batch must block the next bar")
        .into_blocked()
        .expect("blocked unresolved batch must return original settled state");
        let after_state =
            serde_json::to_value(Strategy::state(blocked.settled().strategy())).unwrap();
        assert_eq!(after_state, before_state);
        assert_eq!(
            blocked.settled().intent_batch().bar_close_ts(),
            first_close_ts
        );
    }

    #[test]
    fn stage5ch_cancel_only_batch_is_still_unresolved() {
        let now = Utc
            .with_ymd_and_hms(2026, 7, 13, 9, 0, 30)
            .single()
            .unwrap();
        let recovered = empty_recovered_until(
            now,
            Utc.with_ymd_and_hms(2026, 7, 13, 9, 40, 30)
                .single()
                .unwrap(),
        );
        let (strategy, recovery_receipt) = recovered.into_parts();
        let first_close_ts = Utc
            .with_ymd_and_hms(2026, 7, 13, 9, 10, 0)
            .single()
            .unwrap()
            .timestamp();
        let settled = settle_stage5c_semantic_result(stage5cg_semantic_result(
            strategy,
            recovery_receipt,
            first_close_ts,
            broker_core::HybridRuntimeBarOrigin::Live,
            vec![stage5cg_cancel_intent()],
        ))
        .unwrap();
        assert_eq!(settled.intent_batch().intent_count(), 1);
        assert!(!settled.intent_batch().has_actionable_intents());
        let next_close_ts = Utc
            .with_ymd_and_hms(2026, 7, 13, 9, 20, 0)
            .single()
            .unwrap()
            .timestamp();
        let accepted = accept_stage5c_semantic_bar(semantic_input(next_close_ts)).unwrap();
        let failure = advance_stage5c_controlled_next_bar_at(
            settled,
            accepted,
            Utc.with_ymd_and_hms(2026, 7, 13, 9, 20, 30)
                .single()
                .unwrap(),
        )
        .expect_err("cancel-only batch still requires lifecycle settlement");
        assert_eq!(
            failure.reason(),
            Stage5cNextBarLoopError::UnresolvedIntentBatch
        );
        assert_eq!(
            failure
                .into_blocked()
                .expect("cancel-only block must preserve batch")
                .settled()
                .intent_batch()
                .intent_classes(),
            vec![crate::BrokerNeutralHybridIntentClass::CancelCleanup]
        );
    }

    #[test]
    fn stage5ch_zero_intent_batch_allows_next_bar() {
        let now = Utc
            .with_ymd_and_hms(2026, 7, 13, 9, 0, 30)
            .single()
            .unwrap();
        let recovered = empty_recovered_until(
            now,
            Utc.with_ymd_and_hms(2026, 7, 13, 9, 40, 30)
                .single()
                .unwrap(),
        );
        let (strategy, recovery_receipt) = recovered.into_parts();
        let first_close_ts = Utc
            .with_ymd_and_hms(2026, 7, 13, 9, 10, 0)
            .single()
            .unwrap()
            .timestamp();
        let settled = settle_stage5c_semantic_result(Stage5cSemanticBarResult {
            strategy,
            recovery_receipt,
            bar_close_ts: first_close_ts,
            origin: broker_core::HybridRuntimeBarOrigin::Live,
            execution_eligible: true,
            intents: Vec::new(),
        })
        .unwrap();
        let next_close_ts = Utc
            .with_ymd_and_hms(2026, 7, 13, 9, 20, 0)
            .single()
            .unwrap()
            .timestamp();
        let accepted = accept_stage5c_semantic_bar(semantic_input(next_close_ts)).unwrap();
        let advanced = advance_stage5c_controlled_next_bar_at(
            settled,
            accepted,
            Utc.with_ymd_and_hms(2026, 7, 13, 9, 20, 30)
                .single()
                .unwrap(),
        )
        .unwrap();
        assert_eq!(advanced.intent_batch().bar_close_ts(), next_close_ts);
    }

    #[test]
    fn stage5ch_broker_truth_expiry_block_preserves_settled_state() {
        let now = Utc
            .with_ymd_and_hms(2026, 7, 13, 9, 0, 30)
            .single()
            .unwrap();
        let recovered = empty_recovered(now);
        let (strategy, recovery_receipt) = recovered.into_parts();
        let first_close_ts = Utc
            .with_ymd_and_hms(2026, 7, 13, 9, 10, 0)
            .single()
            .unwrap()
            .timestamp();
        let settled = settle_stage5c_semantic_result(Stage5cSemanticBarResult {
            strategy,
            recovery_receipt,
            bar_close_ts: first_close_ts,
            origin: broker_core::HybridRuntimeBarOrigin::Live,
            execution_eligible: true,
            intents: Vec::new(),
        })
        .unwrap();
        let next_close_ts = Utc
            .with_ymd_and_hms(2026, 7, 13, 9, 20, 0)
            .single()
            .unwrap()
            .timestamp();
        let accepted = accept_stage5c_semantic_bar(semantic_input(next_close_ts)).unwrap();
        let failure = advance_stage5c_controlled_next_bar_at(
            settled,
            accepted,
            Utc.with_ymd_and_hms(2026, 7, 13, 9, 40, 31)
                .single()
                .unwrap(),
        )
        .expect_err("expired broker truth must block before callback");
        assert_eq!(
            failure.reason(),
            Stage5cNextBarLoopError::Semantic(Stage5cSemanticBarError::BrokerTruthExpired)
        );
        let blocked = failure
            .into_blocked()
            .expect("preflight expiry must preserve settled state");
        assert_eq!(
            blocked.settled().intent_batch().bar_close_ts(),
            first_close_ts
        );
    }

    #[test]
    fn stage5ch_rechecks_broker_truth_expiry() {
        let now = Utc
            .with_ymd_and_hms(2026, 7, 13, 9, 0, 30)
            .single()
            .unwrap();
        let recovered = empty_recovered(now);
        let (strategy, recovery_receipt) = recovered.into_parts();
        let first_close_ts = Utc
            .with_ymd_and_hms(2026, 7, 13, 9, 10, 0)
            .single()
            .unwrap()
            .timestamp();
        let settled = settle_stage5c_semantic_result(Stage5cSemanticBarResult {
            strategy,
            recovery_receipt,
            bar_close_ts: first_close_ts,
            origin: broker_core::HybridRuntimeBarOrigin::Live,
            execution_eligible: true,
            intents: Vec::new(),
        })
        .unwrap();
        let next_close_ts = Utc
            .with_ymd_and_hms(2026, 7, 13, 9, 20, 0)
            .single()
            .unwrap()
            .timestamp();
        let accepted = accept_stage5c_semantic_bar(semantic_input(next_close_ts)).unwrap();
        assert_eq!(
            advance_stage5c_controlled_next_bar_at(
                settled,
                accepted,
                Utc.with_ymd_and_hms(2026, 7, 13, 9, 40, 31)
                    .single()
                    .unwrap(),
            )
            .expect_err("expired broker truth must block")
            .reason(),
            Stage5cNextBarLoopError::Semantic(Stage5cSemanticBarError::BrokerTruthExpired)
        );
    }

    #[test]
    fn stage5ci_resolves_nonzero_batch_by_exact_ack_without_sink_or_transport() {
        let (settled, expected_request_id, _) = stage5ci_entry_settled();
        let resolved = resolve_stage5c_paper_intent_lifecycle(
            settled,
            Stage5cPaperIntentLifecycleInput {
                ack_records: vec![stage5ci_ack_record(1, expected_request_id)],
            },
        )
        .unwrap();
        let summary = resolved.resolved_batch_summary();
        assert_eq!(summary.intent_count, 1);
        assert_eq!(summary.request_ids, vec![expected_request_id]);
        assert_eq!(
            resolved.full_resolved_batch().record_request_ids(),
            vec![expected_request_id]
        );
        assert_eq!(resolved.ack_outcomes().len(), 1);
        assert_eq!(resolved.ack_outcomes()[0].total_sequence, 1);
        assert_eq!(resolved.ack_outcomes()[0].request_id, expected_request_id);
        assert_eq!(
            resolved.ack_outcomes()[0].status,
            broker_core::HybridRuntimeAckStatus::Accepted
        );
        assert_eq!(
            resolved.ack_outcomes()[0].broker_order_id,
            Some(BrokerOrderId::new("ORDER_TEST_ACK_0001"))
        );
        assert_eq!(resolved.settled_batch_history().len(), 1);
        assert!(!resolved.intent_sink_attached());
        assert!(!resolved.broker_transport_attached());
        assert!(!resolved.timer_path_enabled());
        assert_eq!(
            match Strategy::state(resolved.strategy()) {
                StrategyState::HybridIntradayRuntime {
                    pending_entry_request_id,
                    ..
                } => *pending_entry_request_id,
                StrategyState::Idle => None,
            },
            Some(expected_request_id),
            "accepted ACK alone is not a fill/position lifecycle and must not fake flat/filled state"
        );
    }

    #[test]
    fn stage5ci_rejects_missing_unknown_and_duplicate_ack() {
        let (settled_for_missing, _, bar_close_ts) = stage5ci_entry_settled();
        let blocked = resolve_stage5c_paper_intent_lifecycle(
            settled_for_missing,
            Stage5cPaperIntentLifecycleInput {
                ack_records: Vec::new(),
            },
        )
        .expect_err("missing ACK must preserve settled type-state")
        .into_blocked()
        .expect("missing ACK is a recoverable preflight block");
        assert_eq!(
            blocked.reason(),
            Stage5cPaperIntentLifecycleError::MissingAck
        );
        assert_eq!(blocked.settled().intent_batch().intent_count(), 1);
        let unknown = crate::deterministic_request_id(
            "hybrid_imoexf",
            "ACC_TEST_0001",
            "IMOEXF",
            "market",
            bar_close_ts,
            4,
        );
        let (settled_for_unknown, _, _) = stage5ci_entry_settled();
        assert!(matches!(
            resolve_stage5c_paper_intent_lifecycle(
                settled_for_unknown,
                Stage5cPaperIntentLifecycleInput {
                    ack_records: vec![stage5ci_ack_record(1, unknown)]
                },
            ),
            Err(Stage5cPaperIntentLifecycleFailure::Blocked(_))
        ));
        let (settled_for_duplicate, expected_request_id, _) = stage5ci_entry_settled();
        assert!(matches!(
            resolve_stage5c_paper_intent_lifecycle(
                settled_for_duplicate,
                Stage5cPaperIntentLifecycleInput {
                    ack_records: vec![
                        stage5ci_ack_record(1, expected_request_id),
                        stage5ci_ack_record(2, expected_request_id)
                    ],
                },
            ),
            Err(Stage5cPaperIntentLifecycleFailure::Blocked(_))
        ));
    }

    #[test]
    fn stage5ci_rejects_state_fingerprint_mismatch() {
        let now = Utc
            .with_ymd_and_hms(2026, 7, 13, 9, 0, 30)
            .single()
            .unwrap();
        let recovered = empty_recovered_until(
            now,
            Utc.with_ymd_and_hms(2026, 7, 13, 9, 40, 30)
                .single()
                .unwrap(),
        );
        let (mut strategy, recovery_receipt) = recovered.into_parts();
        let bar_close_ts = Utc
            .with_ymd_and_hms(2026, 7, 13, 9, 10, 0)
            .single()
            .unwrap()
            .timestamp();
        let expected_request_id = crate::deterministic_request_id(
            "hybrid_imoexf",
            "ACC_TEST_0001",
            "IMOEXF",
            "market",
            bar_close_ts,
            3,
        );
        set_hybrid_pending_request(
            &mut strategy,
            crate::BrokerNeutralHybridIntentClass::Entry,
            expected_request_id,
        );
        let settled = settle_stage5c_semantic_result(stage5cg_semantic_result(
            strategy,
            recovery_receipt,
            bar_close_ts,
            broker_core::HybridRuntimeBarOrigin::Live,
            vec![stage5cg_market_intent(
                crate::BrokerNeutralOrderSide::Buy,
                crate::BrokerNeutralHybridIntentClass::Entry,
            )],
        ))
        .unwrap();
        let (mut strategy, recovery_receipt, batch) = settled.into_parts();
        set_hybrid_pending_request(
            &mut strategy,
            crate::BrokerNeutralHybridIntentClass::Entry,
            crate::deterministic_request_id(
                "hybrid_imoexf",
                "ACC_TEST_0001",
                "IMOEXF",
                "market",
                bar_close_ts + 600,
                3,
            ),
        );
        let drifted = Stage5cSettledPaperStrategy {
            strategy,
            recovery_receipt,
            batch: Stage5cPaperIntentBatch {
                strategy_id: batch.strategy_id.clone(),
                account_id: batch.account_id.clone(),
                instrument: batch.instrument.clone(),
                bar_close_ts: batch.bar_close_ts,
                state_fingerprint: batch.state_fingerprint.clone(),
                request_ids: batch.request_ids.clone(),
                records: batch.records.clone(),
                observation_only: batch.observation_only,
            },
            settled_batch_history: vec![stage5ch_batch_summary(&batch)],
        };
        assert!(matches!(
            resolve_stage5c_paper_intent_lifecycle(
                drifted,
                Stage5cPaperIntentLifecycleInput {
                    ack_records: vec![stage5ci_ack_record(1, expected_request_id)]
                },
            ),
            Err(Stage5cPaperIntentLifecycleFailure::Blocked(_))
        ));
    }

    #[test]
    fn stage5ci_same_ack_set_has_one_canonical_application_order() {
        let (settled_a, tp_request_id, sl_request_id, bar_close_ts) = stage5ci_protective_settled();
        let (settled_b, _, _, _) = stage5ci_protective_settled();
        let tp_ack = Stage5cPaperAckRecord {
            total_sequence: 1,
            ack: stage5ci_ack_with(
                tp_request_id,
                broker_core::HybridRuntimeAckStatus::Rejected,
                bar_close_ts + 100,
            ),
        };
        let sl_ack = Stage5cPaperAckRecord {
            total_sequence: 2,
            ack: stage5ci_ack_with(
                sl_request_id,
                broker_core::HybridRuntimeAckStatus::Rejected,
                bar_close_ts + 200,
            ),
        };
        let resolved_a = resolve_stage5c_paper_intent_lifecycle(
            settled_a,
            Stage5cPaperIntentLifecycleInput {
                ack_records: vec![tp_ack.clone(), sl_ack.clone()],
            },
        )
        .unwrap();
        let resolved_b = resolve_stage5c_paper_intent_lifecycle(
            settled_b,
            Stage5cPaperIntentLifecycleInput {
                ack_records: vec![sl_ack, tp_ack],
            },
        )
        .unwrap();
        assert_eq!(
            resolved_a.post_lifecycle_state_fingerprint(),
            resolved_b.post_lifecycle_state_fingerprint()
        );
        assert_eq!(
            resolved_b
                .ack_outcomes()
                .iter()
                .map(|outcome| outcome.total_sequence)
                .collect::<Vec<_>>(),
            vec![1, 2]
        );
    }

    #[test]
    fn stage5ci_rejects_duplicate_sequence_and_ack_before_intent_bar() {
        let (settled_duplicate, expected_request_id, _) = stage5ci_entry_settled();
        let duplicate = resolve_stage5c_paper_intent_lifecycle(
            settled_duplicate,
            Stage5cPaperIntentLifecycleInput {
                ack_records: vec![
                    stage5ci_ack_record(1, expected_request_id),
                    stage5ci_ack_record(1, expected_request_id),
                ],
            },
        )
        .expect_err("duplicate sequence must be blocked")
        .into_blocked()
        .expect("duplicate sequence is a recoverable preflight block");
        assert_eq!(
            duplicate.reason(),
            Stage5cPaperIntentLifecycleError::DuplicateSequence
        );

        let (settled_early, expected_request_id, bar_close_ts) = stage5ci_entry_settled();
        let early = resolve_stage5c_paper_intent_lifecycle(
            settled_early,
            Stage5cPaperIntentLifecycleInput {
                ack_records: vec![Stage5cPaperAckRecord {
                    total_sequence: 1,
                    ack: stage5ci_ack_with(
                        expected_request_id,
                        broker_core::HybridRuntimeAckStatus::Accepted,
                        bar_close_ts - 1,
                    ),
                }],
            },
        )
        .expect_err("ACK before intent bar must be blocked")
        .into_blocked()
        .expect("early ACK is a recoverable preflight block");
        assert_eq!(
            early.reason(),
            Stage5cPaperIntentLifecycleError::AckTimestampBeforeIntentBar
        );
    }

    #[test]
    fn stage5ci_rejects_empty_intent_batch() {
        let now = Utc
            .with_ymd_and_hms(2026, 7, 13, 9, 0, 30)
            .single()
            .unwrap();
        let recovered = empty_recovered_until(
            now,
            Utc.with_ymd_and_hms(2026, 7, 13, 9, 40, 30)
                .single()
                .unwrap(),
        );
        let (strategy, recovery_receipt) = recovered.into_parts();
        let settled = settle_stage5c_semantic_result(Stage5cSemanticBarResult {
            strategy,
            recovery_receipt,
            bar_close_ts: Utc
                .with_ymd_and_hms(2026, 7, 13, 9, 10, 0)
                .single()
                .unwrap()
                .timestamp(),
            origin: broker_core::HybridRuntimeBarOrigin::Live,
            execution_eligible: true,
            intents: Vec::new(),
        })
        .unwrap();
        assert!(matches!(
            resolve_stage5c_paper_intent_lifecycle(
                settled,
                Stage5cPaperIntentLifecycleInput {
                    ack_records: Vec::new()
                },
            ),
            Err(Stage5cPaperIntentLifecycleFailure::Blocked(_))
        ));
    }

    #[test]
    fn stage5cj_market_ack_requires_position_fill_event_before_next_type_state() {
        let (settled, request_id, _) = stage5ci_entry_settled();
        let resolved = resolve_stage5c_paper_intent_lifecycle(
            settled,
            Stage5cPaperIntentLifecycleInput {
                ack_records: vec![stage5ci_ack_record(1, request_id)],
            },
        )
        .unwrap();
        let missing = resolve_stage5c_paper_broker_lifecycle(
            resolved,
            Stage5cPaperBrokerLifecycleInput {
                event_records: Vec::new(),
            },
        )
        .expect_err("accepted market ACK is not fill evidence");
        assert_eq!(
            missing.reason(),
            Stage5cPaperBrokerLifecycleError::MissingExpectedBrokerEvent
        );

        let (settled, request_id, _) = stage5ci_entry_settled();
        let resolved = resolve_stage5c_paper_intent_lifecycle(
            settled,
            Stage5cPaperIntentLifecycleInput {
                ack_records: vec![stage5ci_ack_record(1, request_id)],
            },
        )
        .unwrap();
        let broker_resolved = resolve_stage5c_paper_broker_lifecycle(
            resolved,
            Stage5cPaperBrokerLifecycleInput {
                event_records: vec![stage5cj_position_event(
                    2,
                    request_id,
                    1.0,
                    Utc.with_ymd_and_hms(2026, 7, 13, 9, 10, 2)
                        .single()
                        .unwrap()
                        .timestamp(),
                )],
            },
        )
        .unwrap();
        assert_eq!(broker_resolved.broker_event_count(), 1);
        assert!(!broker_resolved.intent_sink_attached());
        assert!(!broker_resolved.broker_transport_attached());
        assert!(!broker_resolved.timer_path_enabled());
        assert_eq!(
            match Strategy::state(broker_resolved.strategy()) {
                StrategyState::HybridIntradayRuntime {
                    last_position_qty, ..
                } => *last_position_qty,
                StrategyState::Idle => 0.0,
            },
            1.0,
            "position lifecycle is fill evidence and updates the broker-neutral position truth"
        );
        assert_eq!(
            match Strategy::state(broker_resolved.strategy()) {
                StrategyState::HybridIntradayRuntime {
                    pending_entry_request_id,
                    ..
                } => *pending_entry_request_id,
                StrategyState::Idle => None,
            },
            Some(request_id),
            "facade must not invent pending cleanup semantics that source runtime does not expose"
        );
    }

    #[test]
    fn stage5cj_rejected_ack_expects_no_broker_state_event() {
        let (settled, request_id, bar_close_ts) = stage5ci_entry_settled();
        let rejected_ack = Stage5cPaperAckRecord {
            total_sequence: 1,
            ack: stage5ci_ack_with(
                request_id,
                broker_core::HybridRuntimeAckStatus::Rejected,
                bar_close_ts + 1,
            ),
        };
        let resolved = resolve_stage5c_paper_intent_lifecycle(
            settled,
            Stage5cPaperIntentLifecycleInput {
                ack_records: vec![rejected_ack],
            },
        )
        .unwrap();
        let broker_resolved = resolve_stage5c_paper_broker_lifecycle(
            resolved,
            Stage5cPaperBrokerLifecycleInput {
                event_records: Vec::new(),
            },
        )
        .unwrap();
        assert_eq!(broker_resolved.broker_event_count(), 0);

        let (settled, request_id, bar_close_ts) = stage5ci_entry_settled();
        let rejected_ack = Stage5cPaperAckRecord {
            total_sequence: 1,
            ack: stage5ci_ack_with(
                request_id,
                broker_core::HybridRuntimeAckStatus::Rejected,
                bar_close_ts + 1,
            ),
        };
        let resolved = resolve_stage5c_paper_intent_lifecycle(
            settled,
            Stage5cPaperIntentLifecycleInput {
                ack_records: vec![rejected_ack],
            },
        )
        .unwrap();
        assert_eq!(
            resolve_stage5c_paper_broker_lifecycle(
                resolved,
                Stage5cPaperBrokerLifecycleInput {
                    event_records: vec![stage5cj_position_event(
                        2,
                        request_id,
                        1.0,
                        bar_close_ts + 2
                    )]
                },
            )
            .expect_err("terminal ACK must not accept a broker-state event")
            .reason(),
            Stage5cPaperBrokerLifecycleError::EventForTerminalAck
        );
    }

    #[test]
    fn stage5cj_order_and_stop_events_are_applied_in_canonical_total_sequence() {
        let (settled_a, tp_request_id, sl_request_id, bar_close_ts) = stage5ci_protective_settled();
        let (settled_b, _, _, _) = stage5ci_protective_settled();
        let tp_ack = Stage5cPaperAckRecord {
            total_sequence: 1,
            ack: stage5ci_ack_with(
                tp_request_id,
                broker_core::HybridRuntimeAckStatus::Accepted,
                bar_close_ts + 1,
            ),
        };
        let sl_ack = Stage5cPaperAckRecord {
            total_sequence: 2,
            ack: stage5ci_ack_with(
                sl_request_id,
                broker_core::HybridRuntimeAckStatus::Accepted,
                bar_close_ts + 1,
            ),
        };
        let resolved_a = resolve_stage5c_paper_intent_lifecycle(
            settled_a,
            Stage5cPaperIntentLifecycleInput {
                ack_records: vec![tp_ack.clone(), sl_ack.clone()],
            },
        )
        .unwrap();
        let resolved_b = resolve_stage5c_paper_intent_lifecycle(
            settled_b,
            Stage5cPaperIntentLifecycleInput {
                ack_records: vec![sl_ack, tp_ack],
            },
        )
        .unwrap();
        let order_event = stage5cj_order_event(
            3,
            tp_request_id,
            BrokerOrderId::new("ORDER_TEST_ACK_0001"),
            bar_close_ts + 2,
        );
        let stop_event = stage5cj_stop_event(
            4,
            sl_request_id,
            BrokerOrderId::new("ORDER_TEST_ACK_0001"),
            bar_close_ts + 3,
        );
        let broker_a = resolve_stage5c_paper_broker_lifecycle(
            resolved_a,
            Stage5cPaperBrokerLifecycleInput {
                event_records: vec![order_event.clone(), stop_event.clone()],
            },
        )
        .unwrap();
        let broker_b = resolve_stage5c_paper_broker_lifecycle(
            resolved_b,
            Stage5cPaperBrokerLifecycleInput {
                event_records: vec![stop_event, order_event],
            },
        )
        .unwrap();
        assert_eq!(
            broker_a.post_broker_lifecycle_state_fingerprint(),
            broker_b.post_broker_lifecycle_state_fingerprint()
        );
        assert_eq!(broker_b.broker_event_count(), 2);
    }

    #[test]
    fn stage5cj_blocks_wrong_event_kind_and_broker_order_mismatch() {
        let (settled, request_id, bar_close_ts) = stage5ci_entry_settled();
        let resolved = resolve_stage5c_paper_intent_lifecycle(
            settled,
            Stage5cPaperIntentLifecycleInput {
                ack_records: vec![stage5ci_ack_record(1, request_id)],
            },
        )
        .unwrap();
        assert_eq!(
            resolve_stage5c_paper_broker_lifecycle(
                resolved,
                Stage5cPaperBrokerLifecycleInput {
                    event_records: vec![stage5cj_order_event(
                        2,
                        request_id,
                        BrokerOrderId::new("ORDER_TEST_ACK_0001"),
                        bar_close_ts + 2
                    )]
                },
            )
            .expect_err("market intent must not resolve through order event")
            .reason(),
            Stage5cPaperBrokerLifecycleError::UnexpectedBrokerEventKind
        );

        let (settled, tp_request_id, _, bar_close_ts) = stage5ci_protective_settled();
        let resolved = resolve_stage5c_paper_intent_lifecycle(
            settled,
            Stage5cPaperIntentLifecycleInput {
                ack_records: vec![
                    Stage5cPaperAckRecord {
                        total_sequence: 1,
                        ack: stage5ci_ack_with(
                            tp_request_id,
                            broker_core::HybridRuntimeAckStatus::Accepted,
                            bar_close_ts + 1,
                        ),
                    },
                    Stage5cPaperAckRecord {
                        total_sequence: 2,
                        ack: stage5ci_ack_with(
                            crate::deterministic_request_id(
                                "hybrid_imoexf",
                                "ACC_TEST_0001",
                                "IMOEXF",
                                "create_stop_limit",
                                bar_close_ts,
                                5,
                            ),
                            broker_core::HybridRuntimeAckStatus::Rejected,
                            bar_close_ts + 1,
                        ),
                    },
                ],
            },
        )
        .unwrap();
        assert_eq!(
            resolve_stage5c_paper_broker_lifecycle(
                resolved,
                Stage5cPaperBrokerLifecycleInput {
                    event_records: vec![stage5cj_order_event(
                        3,
                        tp_request_id,
                        BrokerOrderId::new("ORDER_TEST_OTHER"),
                        bar_close_ts + 2
                    )]
                },
            )
            .expect_err("broker order id must match ACK outcome")
            .reason(),
            Stage5cPaperBrokerLifecycleError::BrokerOrderIdMismatch
        );
    }

    #[test]
    fn stage5cj_deduplicates_identical_events_and_blocks_conflicting_duplicate() {
        let (settled, request_id, bar_close_ts) = stage5ci_entry_settled();
        let resolved = resolve_stage5c_paper_intent_lifecycle(
            settled,
            Stage5cPaperIntentLifecycleInput {
                ack_records: vec![stage5ci_ack_record(1, request_id)],
            },
        )
        .unwrap();
        let event = stage5cj_position_event(2, request_id, 1.0, bar_close_ts + 2);
        let duplicate = Stage5cPaperBrokerEventRecord {
            total_sequence: 3,
            ..event.clone()
        };
        let broker_resolved = resolve_stage5c_paper_broker_lifecycle(
            resolved,
            Stage5cPaperBrokerLifecycleInput {
                event_records: vec![duplicate, event],
            },
        )
        .unwrap();
        assert_eq!(broker_resolved.broker_event_count(), 1);

        let (settled, request_id, bar_close_ts) = stage5ci_entry_settled();
        let resolved = resolve_stage5c_paper_intent_lifecycle(
            settled,
            Stage5cPaperIntentLifecycleInput {
                ack_records: vec![stage5ci_ack_record(1, request_id)],
            },
        )
        .unwrap();
        assert_eq!(
            resolve_stage5c_paper_broker_lifecycle(
                resolved,
                Stage5cPaperBrokerLifecycleInput {
                    event_records: vec![
                        stage5cj_position_event(2, request_id, 1.0, bar_close_ts + 2),
                        stage5cj_position_event(3, request_id, 2.0, bar_close_ts + 2)
                    ]
                },
            )
            .expect_err("same request with different payload is conflicting duplicate")
            .reason(),
            Stage5cPaperBrokerLifecycleError::ConflictingDuplicateEvent
        );
    }

    #[test]
    fn stage5cj_explicitly_distinguishes_supported_lifecycle_event_kinds() {
        let now = Utc.timestamp_opt(1_783_342_200, 0).single().unwrap();
        let market = stage5cg_market_intent(
            crate::BrokerNeutralOrderSide::Buy,
            crate::BrokerNeutralHybridIntentClass::Entry,
        );
        let place = stage5cg_place_intent();
        let cancel = stage5cg_cancel_intent();
        let replace = crate::BrokerNeutralHybridIntent::Replace {
            order_id: BrokerOrderId::new("ORDER_TEST_0002"),
            new_price: 2231.0,
            new_qty: 1.0,
        }
        .with_class(crate::BrokerNeutralHybridIntentClass::ProtectiveRepair)
        .with_symbol("IMOEXF");
        let create_stop = stage5cg_stop_intent(now.timestamp() + 600);
        let delete_stop = crate::BrokerNeutralHybridIntent::DeleteStopLimit {
            order_id: BrokerStopOrderId::new("STOP_TEST_0002"),
            side: Some(crate::BrokerNeutralOrderSide::Sell),
            check_duplicates: Some(true),
        }
        .with_class(crate::BrokerNeutralHybridIntentClass::CancelCleanup)
        .with_symbol("IMOEXF");
        assert_eq!(
            stage5cj_expected_event_kind(&market),
            Stage5cPaperBrokerEventKind::Position
        );
        assert_eq!(
            stage5cj_expected_event_kind(&place),
            Stage5cPaperBrokerEventKind::Order
        );
        assert_eq!(
            stage5cj_expected_event_kind(&cancel),
            Stage5cPaperBrokerEventKind::Order
        );
        assert_eq!(
            stage5cj_expected_event_kind(&replace),
            Stage5cPaperBrokerEventKind::Order
        );
        assert_eq!(
            stage5cj_expected_event_kind(&create_stop),
            Stage5cPaperBrokerEventKind::StopOrder
        );
        assert_eq!(
            stage5cj_expected_event_kind(&delete_stop),
            Stage5cPaperBrokerEventKind::StopOrder
        );
    }
}
