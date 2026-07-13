use std::collections::{HashMap, HashSet};

use broker_core::{
    BrokerAccountId, BrokerInstrumentSpec, BrokerOrderId, BrokerStopOrderId, InstrumentId,
    RuntimeHostBootstrapSnapshot, Stage4AcceptedPaperHostEvidence,
    Stage4BootstrapEvidenceReportStatus, Stage4BrokerTruthBootstrapStatus,
    Stage4BrokerTruthFreshnessSection, Stage4BrokerTruthFreshnessStatus,
    Stage4BrokerTruthSourceStatus, Stage4DirtyStartPolicyStatus,
    Stage4RuntimeBootstrapApplicationStatus, Stage4RuntimeBootstrapIntegrationEvent,
    Stage4RuntimeBootstrapIntegrationStatus, Stage4RuntimeLifecycleOrderingStatus,
    StrategyRequestId, STAGE4_BOOTSTRAP_EVIDENCE_REPORT_SCHEMA_VERSION,
    STAGE4_RUNTIME_BOOTSTRAP_APPLICATION_SCHEMA_VERSION,
};
use chrono::{DateTime, TimeZone, Utc};
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
    expected_attribution_by_request:
        HashMap<StrategyRequestId, broker_core::HybridRuntimeAttribution>,
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
        HashMap<StrategyRequestId, broker_core::HybridRuntimeAttribution>,
    ) {
        (
            self.strategy,
            self.recovery_receipt,
            self.bar_close_ts,
            self.origin,
            self.execution_eligible,
            self.intents,
            self.expected_attribution_by_request,
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
    pub origin_bar_close_ts: i64,
    pub bar_close_ts: i64,
    pub min_source_event_ts: i64,
    pub max_source_event_ts: i64,
    pub state_fingerprint: String,
    pub request_ids: Vec<StrategyRequestId>,
    pub intent_count: usize,
    pub observation_only: bool,
}

#[derive(Clone)]
struct Stage5cPaperIntentRecord {
    request_id: StrategyRequestId,
    source_event_ts: i64,
    intent_class: crate::BrokerNeutralHybridIntentClass,
    intent: crate::BrokerNeutralHybridIntent,
    expected_attribution: Option<broker_core::HybridRuntimeAttribution>,
}

#[derive(Default)]
struct Stage5cCleanupAttributionLedger {
    broker_orders: HashMap<BrokerOrderId, broker_core::HybridRuntimeAttribution>,
    stop_orders: HashMap<BrokerStopOrderId, broker_core::HybridRuntimeAttribution>,
    pending_entry_attribution: Option<broker_core::HybridRuntimeAttribution>,
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
    pub fn record_source_event_ts_by_request(&self) -> Vec<(StrategyRequestId, i64)> {
        self.records
            .iter()
            .map(|record| (record.request_id, record.source_event_ts))
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
    PositionSideMismatch,
    PositionOverfill,
    PositionRegression,
    AttributionMissing,
    AttributionStrategyMismatch,
    AttributionRoleMismatch,
    AttributionCycleMismatch,
    IntentFieldMismatch,
    MissingTerminalLifecycleEvent,
    UnknownOrderStatus,
    UnknownStopOrderStatus,
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
    resolved_batch: Stage5cPaperIntentBatch,
    resolved_batch_summary: Stage5cPaperIntentBatchSummary,
    ack_outcomes: Vec<Stage5cPaperAckOutcome>,
    broker_event_count: usize,
    remaining_lifecycle_expectations: Vec<Stage5cPaperBrokerLifecycleExpectation>,
    lifecycle_watermark_ts_utc: i64,
    generated_intent_batch: Option<Stage5cPaperIntentBatch>,
    settled_batch_history: Vec<Stage5cPaperIntentBatchSummary>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage5cPaperTimerError {
    BrokerTruthExpired,
    NonMonotonicTimer,
    UnresolvedBrokerLifecycle,
    UnresolvedGeneratedIntentBatch,
    CallbackValidationFailed,
    GeneratedIntentTerminal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stage5cPaperTimerInput {
    pub now_ts_utc_ms: i64,
}

pub struct Stage5cTimerResolvedPaperStrategy {
    strategy: HybridIntradayRuntimeStrategy,
    recovery_receipt: Stage5cPendingRecoveryReceipt,
    resolved_batch_summary: Stage5cPaperIntentBatchSummary,
    timer_ts_utc_ms: i64,
    generated_intent_batch: Option<Stage5cPaperIntentBatch>,
    settled_batch_history: Vec<Stage5cPaperIntentBatchSummary>,
}

impl std::fmt::Debug for Stage5cTimerResolvedPaperStrategy {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Stage5cTimerResolvedPaperStrategy")
            .field("timer_ts_utc_ms", &self.timer_ts_utc_ms)
            .field("generated_intent_count", &self.generated_intent_count())
            .field("intent_sink_attached", &false)
            .field("broker_transport_attached", &false)
            .finish_non_exhaustive()
    }
}

impl Stage5cTimerResolvedPaperStrategy {
    pub fn timer_ts_utc_ms(&self) -> i64 {
        self.timer_ts_utc_ms
    }
    pub fn generated_intent_count(&self) -> usize {
        self.generated_intent_batch
            .as_ref()
            .map(Stage5cPaperIntentBatch::intent_count)
            .unwrap_or_default()
    }
    pub fn generated_intent_batch_summary(&self) -> Option<Stage5cPaperIntentBatchSummary> {
        self.generated_intent_batch
            .as_ref()
            .map(stage5ch_batch_summary)
    }
    pub fn settled_batch_history(&self) -> &[Stage5cPaperIntentBatchSummary] {
        &self.settled_batch_history
    }
    pub fn recovery_receipt(&self) -> &Stage5cPendingRecoveryReceipt {
        &self.recovery_receipt
    }
    pub fn resolved_batch_summary(&self) -> &Stage5cPaperIntentBatchSummary {
        &self.resolved_batch_summary
    }
    pub fn post_timer_state_fingerprint(&self) -> String {
        stage5c_state_fingerprint(Strategy::state(&self.strategy))
    }
    pub fn intent_sink_attached(&self) -> bool {
        false
    }
    pub fn broker_transport_attached(&self) -> bool {
        false
    }
    pub fn redis_command_stream_attached(&self) -> bool {
        false
    }
}

pub struct Stage5cTimerSettlement {
    inner: Stage5cTimerSettlementKind,
}

enum Stage5cTimerSettlementKind {
    ReadyForContinuation {
        settled: Stage5cSettledPaperStrategy,
        checkpoint_ts_utc_ms: i64,
    },
    GeneratedIntentBatch(Stage5cSettledPaperStrategy),
}

impl std::fmt::Debug for Stage5cTimerSettlement {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (kind, settled, checkpoint_ts_utc_ms) = match &self.inner {
            Stage5cTimerSettlementKind::ReadyForContinuation {
                settled,
                checkpoint_ts_utc_ms,
            } => ("ReadyForContinuation", settled, Some(*checkpoint_ts_utc_ms)),
            Stage5cTimerSettlementKind::GeneratedIntentBatch(settled) => {
                ("GeneratedIntentBatch", settled, None)
            }
        };
        formatter
            .debug_struct("Stage5cTimerSettlement")
            .field("kind", &kind)
            .field("checkpoint_ts_utc_ms", &checkpoint_ts_utc_ms)
            .field(
                "timer_result_intent_count",
                &settled.intent_batch().intent_count(),
            )
            .field("intent_sink_attached", &false)
            .field("broker_transport_attached", &false)
            .finish_non_exhaustive()
    }
}

impl Stage5cTimerSettlement {
    fn ready_for_continuation(
        settled: Stage5cSettledPaperStrategy,
        checkpoint_ts_utc_ms: i64,
    ) -> Self {
        Self {
            inner: Stage5cTimerSettlementKind::ReadyForContinuation {
                settled,
                checkpoint_ts_utc_ms,
            },
        }
    }

    fn generated_intent_batch(settled: Stage5cSettledPaperStrategy) -> Self {
        Self {
            inner: Stage5cTimerSettlementKind::GeneratedIntentBatch(settled),
        }
    }

    pub fn is_ready_for_continuation(&self) -> bool {
        matches!(
            self.inner,
            Stage5cTimerSettlementKind::ReadyForContinuation { .. }
        )
    }
    pub fn is_generated_intent_batch(&self) -> bool {
        matches!(
            self.inner,
            Stage5cTimerSettlementKind::GeneratedIntentBatch(_)
        )
    }
    pub fn settled(&self) -> &Stage5cSettledPaperStrategy {
        match &self.inner {
            Stage5cTimerSettlementKind::ReadyForContinuation { settled, .. }
            | Stage5cTimerSettlementKind::GeneratedIntentBatch(settled) => settled,
        }
    }
    pub fn into_generated_intent_batch(self) -> Result<Stage5cSettledPaperStrategy, Box<Self>> {
        match self.inner {
            Stage5cTimerSettlementKind::GeneratedIntentBatch(settled) => Ok(settled),
            Stage5cTimerSettlementKind::ReadyForContinuation {
                settled,
                checkpoint_ts_utc_ms,
            } => Err(Box::new(Self::ready_for_continuation(
                settled,
                checkpoint_ts_utc_ms,
            ))),
        }
    }
    pub fn checkpoint_ts_utc_ms(&self) -> Option<i64> {
        match &self.inner {
            Stage5cTimerSettlementKind::ReadyForContinuation {
                checkpoint_ts_utc_ms,
                ..
            } => Some(*checkpoint_ts_utc_ms),
            Stage5cTimerSettlementKind::GeneratedIntentBatch(_) => None,
        }
    }
    pub fn intent_sink_attached(&self) -> bool {
        false
    }
    pub fn broker_transport_attached(&self) -> bool {
        false
    }
    pub fn redis_command_stream_attached(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage5cTimerContinuationError {
    GeneratedIntentBatchRequiresLifecycle,
    NonMonotonicTimer,
    BrokerTruthExpired,
    CallbackValidationFailed,
    GeneratedIntentTerminal,
    NextBar(Stage5cNextBarLoopError),
}

pub struct Stage5cTimerContinuationBlocked {
    reason: Stage5cTimerContinuationError,
    settlement: Stage5cTimerSettlement,
}

impl Stage5cTimerContinuationBlocked {
    pub fn reason(&self) -> Stage5cTimerContinuationError {
        self.reason
    }
    pub fn settlement(&self) -> &Stage5cTimerSettlement {
        &self.settlement
    }
    pub fn into_settlement(self) -> Stage5cTimerSettlement {
        self.settlement
    }
}

impl std::fmt::Debug for Stage5cTimerContinuationBlocked {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Stage5cTimerContinuationBlocked")
            .field("reason", &self.reason)
            .field(
                "checkpoint_ts_utc_ms",
                &self.settlement.checkpoint_ts_utc_ms(),
            )
            .field(
                "intent_count",
                &self.settlement.settled().intent_batch().intent_count(),
            )
            .finish_non_exhaustive()
    }
}

#[derive(Debug)]
pub enum Stage5cTimerContinuationFailure {
    Blocked(Box<Stage5cTimerContinuationBlocked>),
    Terminal(Stage5cTimerContinuationError),
}

impl Stage5cTimerContinuationFailure {
    pub fn reason(&self) -> Stage5cTimerContinuationError {
        match self {
            Self::Blocked(blocked) => blocked.reason(),
            Self::Terminal(reason) => *reason,
        }
    }
    pub fn into_blocked(self) -> Option<Stage5cTimerContinuationBlocked> {
        match self {
            Self::Blocked(blocked) => Some(*blocked),
            Self::Terminal(_) => None,
        }
    }
}

impl std::fmt::Display for Stage5cTimerContinuationFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "Stage 5C timer continuation failed: {:?}",
            self.reason()
        )
    }
}

impl std::error::Error for Stage5cTimerContinuationFailure {}

pub struct Stage5cPaperTimerBlocked {
    reason: Stage5cPaperTimerError,
    resolved: Stage5cBrokerLifecycleResolvedPaperStrategy,
}

impl std::fmt::Debug for Stage5cPaperTimerBlocked {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Stage5cPaperTimerBlocked")
            .field("reason", &self.reason)
            .field(
                "resolved_bar_close_ts",
                &self.resolved.resolved_batch_summary.bar_close_ts,
            )
            .finish_non_exhaustive()
    }
}

impl Stage5cPaperTimerBlocked {
    pub fn reason(&self) -> Stage5cPaperTimerError {
        self.reason
    }
    pub fn resolved(&self) -> &Stage5cBrokerLifecycleResolvedPaperStrategy {
        &self.resolved
    }
    #[cfg(test)]
    fn into_resolved(self) -> Stage5cBrokerLifecycleResolvedPaperStrategy {
        self.resolved
    }
}

#[derive(Debug)]
pub enum Stage5cPaperTimerFailure {
    Blocked(Box<Stage5cPaperTimerBlocked>),
    Terminal(Stage5cPaperTimerError),
}

impl Stage5cPaperTimerFailure {
    pub fn reason(&self) -> Stage5cPaperTimerError {
        match self {
            Self::Blocked(blocked) => blocked.reason(),
            Self::Terminal(reason) => *reason,
        }
    }
    #[cfg(test)]
    fn into_blocked(self) -> Option<Box<Stage5cPaperTimerBlocked>> {
        match self {
            Self::Blocked(blocked) => Some(blocked),
            Self::Terminal(_) => None,
        }
    }
}

impl std::fmt::Display for Stage5cPaperTimerFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "Stage 5C paper timer failed: {:?}",
            self.reason()
        )
    }
}

impl std::error::Error for Stage5cPaperTimerFailure {}

impl std::fmt::Debug for Stage5cBrokerLifecycleResolvedPaperStrategy {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Stage5cBrokerLifecycleResolvedPaperStrategy")
            .field(
                "resolved_bar_close_ts",
                &self.resolved_batch_summary.bar_close_ts,
            )
            .field("broker_event_count", &self.broker_event_count)
            .field(
                "lifecycle_watermark_ts_utc",
                &self.lifecycle_watermark_ts_utc,
            )
            .field(
                "remaining_lifecycle_expectation_count",
                &self.remaining_lifecycle_expectations.len(),
            )
            .field(
                "generated_intent_count",
                &self
                    .generated_intent_batch
                    .as_ref()
                    .map(Stage5cPaperIntentBatch::intent_count)
                    .unwrap_or_default(),
            )
            .field("intent_sink_attached", &false)
            .field("broker_transport_attached", &false)
            .finish_non_exhaustive()
    }
}

impl Stage5cBrokerLifecycleResolvedPaperStrategy {
    pub fn resolved_batch_summary(&self) -> &Stage5cPaperIntentBatchSummary {
        &self.resolved_batch_summary
    }
    pub fn full_resolved_intent_count(&self) -> usize {
        self.resolved_batch.intent_count()
    }
    #[cfg(test)]
    fn full_resolved_batch(&self) -> &Stage5cPaperIntentBatch {
        &self.resolved_batch
    }
    pub fn ack_outcomes(&self) -> &[Stage5cPaperAckOutcome] {
        &self.ack_outcomes
    }
    pub fn broker_event_count(&self) -> usize {
        self.broker_event_count
    }
    pub fn remaining_lifecycle_expectations(&self) -> &[Stage5cPaperBrokerLifecycleExpectation] {
        &self.remaining_lifecycle_expectations
    }
    pub fn lifecycle_watermark_ts_utc(&self) -> i64 {
        self.lifecycle_watermark_ts_utc
    }
    pub fn generated_intent_count(&self) -> usize {
        self.generated_intent_batch
            .as_ref()
            .map(Stage5cPaperIntentBatch::intent_count)
            .unwrap_or_default()
    }
    pub fn generated_intent_batch_summary(&self) -> Option<Stage5cPaperIntentBatchSummary> {
        self.generated_intent_batch
            .as_ref()
            .map(stage5ch_batch_summary)
    }
    #[cfg(test)]
    fn generated_intent_batch(&self) -> Option<&Stage5cPaperIntentBatch> {
        self.generated_intent_batch.as_ref()
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stage5cPaperBrokerLifecycleExpectation {
    pub request_id: StrategyRequestId,
    pub expected_event_kind: Stage5cPaperBrokerEventKind,
    pub reason: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage5cPaperLoopStateKind {
    PendingRecovered,
    SemanticResult,
    Settled,
    IntentLifecycleResolved,
    BrokerLifecycleResolved,
    TimerResolved,
    TimerSettlement,
}

pub enum Stage5cPaperLoopState {
    PendingRecovered(Box<Stage5cPendingRecoveredPaperStrategy>),
    SemanticResult(Box<Stage5cSemanticBarResult>),
    Settled(Box<Stage5cSettledPaperStrategy>),
    IntentLifecycleResolved(Box<Stage5cResolvedPaperIntentBatchStrategy>),
    BrokerLifecycleResolved(Box<Stage5cBrokerLifecycleResolvedPaperStrategy>),
    TimerResolved(Box<Stage5cTimerResolvedPaperStrategy>),
    TimerSettlement(Box<Stage5cTimerSettlement>),
}

impl std::fmt::Debug for Stage5cPaperLoopState {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Stage5cPaperLoopState")
            .field("kind", &self.kind())
            .field("intent_sink_attached", &self.intent_sink_attached())
            .field(
                "broker_transport_attached",
                &self.broker_transport_attached(),
            )
            .field(
                "redis_command_stream_attached",
                &self.redis_command_stream_attached(),
            )
            .finish_non_exhaustive()
    }
}

impl Stage5cPaperLoopState {
    pub fn kind(&self) -> Stage5cPaperLoopStateKind {
        match self {
            Self::PendingRecovered(_) => Stage5cPaperLoopStateKind::PendingRecovered,
            Self::SemanticResult(_) => Stage5cPaperLoopStateKind::SemanticResult,
            Self::Settled(_) => Stage5cPaperLoopStateKind::Settled,
            Self::IntentLifecycleResolved(_) => Stage5cPaperLoopStateKind::IntentLifecycleResolved,
            Self::BrokerLifecycleResolved(_) => Stage5cPaperLoopStateKind::BrokerLifecycleResolved,
            Self::TimerResolved(_) => Stage5cPaperLoopStateKind::TimerResolved,
            Self::TimerSettlement(_) => Stage5cPaperLoopStateKind::TimerSettlement,
        }
    }

    pub fn intent_sink_attached(&self) -> bool {
        false
    }

    pub fn broker_transport_attached(&self) -> bool {
        false
    }

    pub fn redis_command_stream_attached(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage5cPaperLoopEventKind {
    FinalM10Bar,
    SettleSemanticResult,
    Ack,
    OrderEvent,
    StopOrderEvent,
    PositionEvent,
    Timer,
    SettleTimerResult,
}

pub enum Stage5cPaperLoopEvent {
    FinalM10Bar(Box<Stage5cAcceptedSemanticBar>),
    SettleSemanticResult,
    Ack(Box<Stage5cPaperIntentLifecycleInput>),
    OrderEvent(Box<Stage5cPaperBrokerEventRecord>),
    StopOrderEvent(Box<Stage5cPaperBrokerEventRecord>),
    PositionEvent(Box<Stage5cPaperBrokerEventRecord>),
    Timer(Stage5cPaperTimerInput),
    SettleTimerResult,
}

impl std::fmt::Debug for Stage5cPaperLoopEvent {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Stage5cPaperLoopEvent")
            .field("kind", &self.kind())
            .finish_non_exhaustive()
    }
}

impl Stage5cPaperLoopEvent {
    pub fn kind(&self) -> Stage5cPaperLoopEventKind {
        match self {
            Self::FinalM10Bar(_) => Stage5cPaperLoopEventKind::FinalM10Bar,
            Self::SettleSemanticResult => Stage5cPaperLoopEventKind::SettleSemanticResult,
            Self::Ack(_) => Stage5cPaperLoopEventKind::Ack,
            Self::OrderEvent(_) => Stage5cPaperLoopEventKind::OrderEvent,
            Self::StopOrderEvent(_) => Stage5cPaperLoopEventKind::StopOrderEvent,
            Self::PositionEvent(_) => Stage5cPaperLoopEventKind::PositionEvent,
            Self::Timer(_) => Stage5cPaperLoopEventKind::Timer,
            Self::SettleTimerResult => Stage5cPaperLoopEventKind::SettleTimerResult,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage5cPaperLoopError {
    InvalidTransition {
        state: Stage5cPaperLoopStateKind,
        event: Stage5cPaperLoopEventKind,
    },
    BrokerEventKindMismatch {
        expected: Stage5cPaperBrokerEventKind,
        actual: Stage5cPaperBrokerEventKind,
    },
    Semantic(Stage5cSemanticBarError),
    IntentSettlement(Stage5cIntentSettlementError),
    NextBar(Stage5cNextBarLoopError),
    IntentLifecycle(Stage5cPaperIntentLifecycleError),
    BrokerLifecycle(Stage5cPaperBrokerLifecycleError),
    Timer(Stage5cPaperTimerError),
    TimerContinuation(Stage5cTimerContinuationError),
}

impl std::fmt::Display for Stage5cPaperLoopError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "Stage 5C bounded paper loop blocked: {self:?}")
    }
}

impl std::error::Error for Stage5cPaperLoopError {}

pub struct Stage5cPaperLoopFailure {
    reason: Stage5cPaperLoopError,
    preserved_state: Option<Box<Stage5cPaperLoopState>>,
}

impl std::fmt::Debug for Stage5cPaperLoopFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Stage5cPaperLoopFailure")
            .field("reason", &self.reason)
            .field(
                "preserved_state_kind",
                &self.preserved_state.as_ref().map(|state| state.kind()),
            )
            .finish_non_exhaustive()
    }
}

impl Stage5cPaperLoopFailure {
    pub fn reason(&self) -> Stage5cPaperLoopError {
        self.reason
    }

    pub fn preserved_state(&self) -> Option<&Stage5cPaperLoopState> {
        self.preserved_state.as_deref()
    }

    pub fn into_preserved_state(self) -> Option<Stage5cPaperLoopState> {
        self.preserved_state.map(|state| *state)
    }
}

impl std::fmt::Display for Stage5cPaperLoopFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "Stage 5C bounded paper loop failed: {:?}",
            self.reason
        )
    }
}

impl std::error::Error for Stage5cPaperLoopFailure {}

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

fn stage5cn_invalid_transition(
    state: Stage5cPaperLoopState,
    event: Stage5cPaperLoopEventKind,
) -> Stage5cPaperLoopFailure {
    Stage5cPaperLoopFailure {
        reason: Stage5cPaperLoopError::InvalidTransition {
            state: state.kind(),
            event,
        },
        preserved_state: Some(Box::new(state)),
    }
}

fn stage5cn_terminal(reason: Stage5cPaperLoopError) -> Stage5cPaperLoopFailure {
    Stage5cPaperLoopFailure {
        reason,
        preserved_state: None,
    }
}

fn stage5cn_preserved(
    reason: Stage5cPaperLoopError,
    state: Stage5cPaperLoopState,
) -> Stage5cPaperLoopFailure {
    Stage5cPaperLoopFailure {
        reason,
        preserved_state: Some(Box::new(state)),
    }
}

fn stage5cn_single_broker_event_input(
    expected: Stage5cPaperBrokerEventKind,
    record: Stage5cPaperBrokerEventRecord,
) -> Result<Stage5cPaperBrokerLifecycleInput, Stage5cPaperLoopError> {
    let actual = record.payload.kind();
    if actual != expected {
        return Err(Stage5cPaperLoopError::BrokerEventKindMismatch { expected, actual });
    }
    Ok(Stage5cPaperBrokerLifecycleInput {
        event_records: vec![record],
    })
}

fn stage5cn_resolve_broker_event(
    resolved: Stage5cResolvedPaperIntentBatchStrategy,
    input: Stage5cPaperBrokerLifecycleInput,
) -> Result<Stage5cPaperLoopState, Stage5cPaperLoopFailure> {
    resolve_stage5c_paper_broker_lifecycle(resolved, input)
        .map(|state| Stage5cPaperLoopState::BrokerLifecycleResolved(Box::new(state)))
        .map_err(|failure| {
            let reason = Stage5cPaperLoopError::BrokerLifecycle(failure.reason());
            match failure.into_blocked() {
                Some(blocked) => stage5cn_preserved(
                    reason,
                    Stage5cPaperLoopState::IntentLifecycleResolved(Box::new(
                        blocked.into_resolved(),
                    )),
                ),
                None => stage5cn_terminal(reason),
            }
        })
}

pub fn advance_stage5c_paper_loop_once(
    state: Stage5cPaperLoopState,
    event: Stage5cPaperLoopEvent,
) -> Result<Stage5cPaperLoopState, Stage5cPaperLoopFailure> {
    let event_kind = event.kind();
    match (state, event) {
        (
            Stage5cPaperLoopState::PendingRecovered(recovered),
            Stage5cPaperLoopEvent::FinalM10Bar(bar),
        ) => apply_stage5c_semantic_bar(*recovered, *bar)
            .map(|state| Stage5cPaperLoopState::SemanticResult(Box::new(state)))
            .map_err(|reason| stage5cn_terminal(Stage5cPaperLoopError::Semantic(reason))),
        (
            Stage5cPaperLoopState::SemanticResult(result),
            Stage5cPaperLoopEvent::SettleSemanticResult,
        ) => settle_stage5c_semantic_result(*result)
            .map(|state| Stage5cPaperLoopState::Settled(Box::new(state)))
            .map_err(|reason| stage5cn_terminal(Stage5cPaperLoopError::IntentSettlement(reason))),
        (Stage5cPaperLoopState::Settled(settled), Stage5cPaperLoopEvent::FinalM10Bar(bar)) => {
            advance_stage5c_controlled_next_bar(*settled, *bar)
                .map(|state| Stage5cPaperLoopState::Settled(Box::new(state)))
                .map_err(|failure| {
                    let reason = Stage5cPaperLoopError::NextBar(failure.reason());
                    match failure.into_blocked() {
                        Some(blocked) => stage5cn_preserved(
                            reason,
                            Stage5cPaperLoopState::Settled(Box::new(blocked.into_settled())),
                        ),
                        None => stage5cn_terminal(reason),
                    }
                })
        }
        (Stage5cPaperLoopState::Settled(settled), Stage5cPaperLoopEvent::Ack(input)) => {
            resolve_stage5c_paper_intent_lifecycle(*settled, *input)
                .map(|state| Stage5cPaperLoopState::IntentLifecycleResolved(Box::new(state)))
                .map_err(|failure| {
                    let reason = Stage5cPaperLoopError::IntentLifecycle(failure.reason());
                    match failure.into_blocked() {
                        Some(blocked) => stage5cn_preserved(
                            reason,
                            Stage5cPaperLoopState::Settled(Box::new(blocked.into_settled())),
                        ),
                        None => stage5cn_terminal(reason),
                    }
                })
        }
        (
            Stage5cPaperLoopState::IntentLifecycleResolved(resolved),
            Stage5cPaperLoopEvent::OrderEvent(record),
        ) => {
            let input = match stage5cn_single_broker_event_input(
                Stage5cPaperBrokerEventKind::Order,
                *record,
            ) {
                Ok(input) => input,
                Err(reason) => {
                    return Err(stage5cn_preserved(
                        reason,
                        Stage5cPaperLoopState::IntentLifecycleResolved(resolved),
                    ))
                }
            };
            stage5cn_resolve_broker_event(*resolved, input)
        }
        (
            Stage5cPaperLoopState::IntentLifecycleResolved(resolved),
            Stage5cPaperLoopEvent::StopOrderEvent(record),
        ) => {
            let input = match stage5cn_single_broker_event_input(
                Stage5cPaperBrokerEventKind::StopOrder,
                *record,
            ) {
                Ok(input) => input,
                Err(reason) => {
                    return Err(stage5cn_preserved(
                        reason,
                        Stage5cPaperLoopState::IntentLifecycleResolved(resolved),
                    ))
                }
            };
            stage5cn_resolve_broker_event(*resolved, input)
        }
        (
            Stage5cPaperLoopState::IntentLifecycleResolved(resolved),
            Stage5cPaperLoopEvent::PositionEvent(record),
        ) => {
            let input = match stage5cn_single_broker_event_input(
                Stage5cPaperBrokerEventKind::Position,
                *record,
            ) {
                Ok(input) => input,
                Err(reason) => {
                    return Err(stage5cn_preserved(
                        reason,
                        Stage5cPaperLoopState::IntentLifecycleResolved(resolved),
                    ))
                }
            };
            stage5cn_resolve_broker_event(*resolved, input)
        }
        (
            Stage5cPaperLoopState::BrokerLifecycleResolved(resolved),
            Stage5cPaperLoopEvent::Timer(input),
        ) => resolve_stage5c_paper_timer(*resolved, input)
            .map(|state| Stage5cPaperLoopState::TimerResolved(Box::new(state)))
            .map_err(|failure| stage5cn_terminal(Stage5cPaperLoopError::Timer(failure.reason()))),
        (Stage5cPaperLoopState::TimerResolved(timer), Stage5cPaperLoopEvent::SettleTimerResult) => {
            Ok(Stage5cPaperLoopState::TimerSettlement(Box::new(
                settle_stage5c_timer_result(*timer),
            )))
        }
        (
            Stage5cPaperLoopState::TimerSettlement(settlement),
            Stage5cPaperLoopEvent::FinalM10Bar(bar),
        ) => advance_stage5c_timer_settlement_next_bar(*settlement, *bar)
            .map(|state| Stage5cPaperLoopState::Settled(Box::new(state)))
            .map_err(|failure| {
                let reason = Stage5cPaperLoopError::TimerContinuation(failure.reason());
                match failure.into_blocked() {
                    Some(blocked) => stage5cn_preserved(
                        reason,
                        Stage5cPaperLoopState::TimerSettlement(Box::new(blocked.into_settlement())),
                    ),
                    None => stage5cn_terminal(reason),
                }
            }),
        (
            Stage5cPaperLoopState::TimerSettlement(settlement),
            Stage5cPaperLoopEvent::Timer(input),
        ) => advance_stage5c_timer_settlement_timer(*settlement, input)
            .map(|state| Stage5cPaperLoopState::TimerResolved(Box::new(state)))
            .map_err(|failure| {
                let reason = Stage5cPaperLoopError::TimerContinuation(failure.reason());
                match failure.into_blocked() {
                    Some(blocked) => stage5cn_preserved(
                        reason,
                        Stage5cPaperLoopState::TimerSettlement(Box::new(blocked.into_settlement())),
                    ),
                    None => stage5cn_terminal(reason),
                }
            }),
        (Stage5cPaperLoopState::TimerSettlement(settlement), Stage5cPaperLoopEvent::Ack(input)) => {
            match (*settlement).into_generated_intent_batch() {
                Ok(settled) => resolve_stage5c_paper_intent_lifecycle(settled, *input)
                    .map(|state| Stage5cPaperLoopState::IntentLifecycleResolved(Box::new(state)))
                    .map_err(|failure| {
                        let reason = Stage5cPaperLoopError::IntentLifecycle(failure.reason());
                        match failure.into_blocked() {
                            Some(blocked) => stage5cn_preserved(
                                reason,
                                Stage5cPaperLoopState::Settled(Box::new(blocked.into_settled())),
                            ),
                            None => stage5cn_terminal(reason),
                        }
                    }),
                Err(settlement) => Err(stage5cn_invalid_transition(
                    Stage5cPaperLoopState::TimerSettlement(settlement),
                    Stage5cPaperLoopEventKind::Ack,
                )),
            }
        }
        (state, _) => Err(stage5cn_invalid_transition(state, event_kind)),
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
    let pre_callback_cleanup_ledger =
        stage5cj_cleanup_attribution_ledger(Strategy::state(&strategy), admission.strategy_id());
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
    let expected_attribution_by_request =
        stage5cj_expected_generated_attribution_by_request_from_ledger(
            admission,
            bar_close_ts,
            &intents,
            &pre_callback_cleanup_ledger,
        )
        .map_err(|_| Stage5cSemanticBarError::CallbackValidationFailed)?;
    Ok(Stage5cSemanticBarResult {
        strategy,
        recovery_receipt,
        bar_close_ts,
        origin,
        execution_eligible,
        intents,
        expected_attribution_by_request,
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
    settle_stage5c_semantic_result_with_expected_attribution(result, &HashMap::new())
}

fn settle_stage5c_semantic_result_with_expected_attribution(
    result: Stage5cSemanticBarResult,
    expected_attribution_by_request: &HashMap<
        StrategyRequestId,
        broker_core::HybridRuntimeAttribution,
    >,
) -> Result<Stage5cSettledPaperStrategy, Stage5cIntentSettlementError> {
    let (
        strategy,
        recovery_receipt,
        bar_close_ts,
        origin,
        execution_eligible,
        intents,
        mut result_expected_attribution_by_request,
    ) = result.into_parts();
    result_expected_attribution_by_request.extend(expected_attribution_by_request.clone());
    let admission = &recovery_receipt
        .warmup_receipt()
        .restore_receipt()
        .bootstrap_receipt()
        .admission;
    if !execution_eligible && !intents.is_empty() {
        return Err(Stage5cIntentSettlementError::ReplayIntentNotExecutable);
    }
    let batch = stage5c_build_paper_intent_batch(
        &strategy,
        admission,
        bar_close_ts,
        origin,
        intents,
        &result_expected_attribution_by_request,
    )?;
    let settled_batch_history = vec![stage5ch_batch_summary(&batch)];
    Ok(Stage5cSettledPaperStrategy {
        strategy,
        recovery_receipt,
        batch,
        settled_batch_history,
    })
}

fn stage5c_build_paper_intent_batch(
    strategy: &HybridIntradayRuntimeStrategy,
    admission: &Stage5cPaperHostAdmission,
    bar_close_ts: i64,
    origin: broker_core::HybridRuntimeBarOrigin,
    intents: Vec<crate::BrokerNeutralHybridIntent>,
    expected_attribution_by_request: &HashMap<
        StrategyRequestId,
        broker_core::HybridRuntimeAttribution,
    >,
) -> Result<Stage5cPaperIntentBatch, Stage5cIntentSettlementError> {
    if intents.len() > u8::MAX as usize {
        return Err(Stage5cIntentSettlementError::TooManyIntents);
    }
    let mut request_ids = Vec::with_capacity(intents.len());
    let mut records = Vec::with_capacity(intents.len());
    let mut seen_request_ids = HashSet::new();
    let state = Strategy::state(strategy);
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
        let expected_attribution = expected_attribution_by_request
            .get(&request_id)
            .cloned()
            .or_else(|| {
                stage5cj_expected_attribution_for_intent(
                    state,
                    admission.strategy_id(),
                    class,
                    &intent,
                )
            });
        records.push(Stage5cPaperIntentRecord {
            request_id,
            source_event_ts: bar_close_ts,
            intent_class: class,
            expected_attribution,
            intent,
        });
    }
    Ok(Stage5cPaperIntentBatch {
        strategy_id: admission.strategy_id().to_string(),
        account_id: admission.account_id().clone(),
        instrument: admission.target_instrument().clone(),
        bar_close_ts,
        state_fingerprint: stage5c_state_fingerprint(state),
        request_ids,
        records,
        observation_only: origin == broker_core::HybridRuntimeBarOrigin::Replay,
    })
}

fn stage5ch_batch_summary(batch: &Stage5cPaperIntentBatch) -> Stage5cPaperIntentBatchSummary {
    let min_source_event_ts = batch
        .records
        .iter()
        .map(|record| record.source_event_ts)
        .min()
        .unwrap_or(batch.bar_close_ts);
    let max_source_event_ts = batch
        .records
        .iter()
        .map(|record| record.source_event_ts)
        .max()
        .unwrap_or(batch.bar_close_ts);
    Stage5cPaperIntentBatchSummary {
        strategy_id: batch.strategy_id.clone(),
        account_id: batch.account_id.clone(),
        instrument: batch.instrument.clone(),
        origin_bar_close_ts: batch.bar_close_ts,
        bar_close_ts: batch.bar_close_ts,
        min_source_event_ts,
        max_source_event_ts,
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
    let source_ts_by_request: HashMap<StrategyRequestId, i64> = settled
        .batch
        .records
        .iter()
        .map(|record| (record.request_id, record.source_event_ts))
        .collect();
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
        let Some(source_event_ts) = source_ts_by_request.get(&record.ack.request_id) else {
            return Err(Stage5cPaperIntentLifecycleFailure::Blocked(Box::new(
                Stage5cPaperIntentLifecycleBlocked {
                    reason: Stage5cPaperIntentLifecycleError::UnknownAckRequestId,
                    settled,
                },
            )));
        };
        if record.ack.processed_ts_utc < *source_event_ts {
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
    let mut event_identity_records: HashMap<String, Stage5cPaperBrokerEventRecord> = HashMap::new();
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
        if let Some(previous) = event_identity_records.get_mut(&identity) {
            if record.payload != previous.payload {
                return Err(stage5cj_block(
                    Stage5cPaperBrokerLifecycleError::ConflictingDuplicateEvent,
                    resolved,
                ));
            }
            if record.total_sequence < previous.total_sequence {
                *previous = record.clone();
            }
            continue;
        }
        event_identity_records.insert(identity, record.clone());
    }
    let mut canonical_event_records: Vec<_> = event_identity_records.into_values().collect();
    canonical_event_records.sort_by_key(|record| record.total_sequence);
    let admission_strategy_id = resolved
        .recovery_receipt
        .warmup_receipt()
        .restore_receipt()
        .bootstrap_receipt()
        .admission
        .strategy_id()
        .to_string();
    let ack_by_request: HashMap<StrategyRequestId, Stage5cPaperAckOutcome> = resolved
        .ack_outcomes
        .iter()
        .cloned()
        .map(|outcome| (outcome.request_id, outcome))
        .collect();
    let mut events_by_request: HashMap<StrategyRequestId, Vec<Stage5cPaperBrokerEventRecord>> =
        HashMap::new();
    for record in &canonical_event_records {
        events_by_request
            .entry(record.request_id)
            .or_default()
            .push(record.clone());
    }
    let mut remaining_lifecycle_expectations = Vec::new();
    for intent_record in &resolved.resolved_batch.records {
        let ack = ack_by_request
            .get(&intent_record.request_id)
            .expect("ACK lifecycle enforces exact request coverage");
        let request_events = events_by_request
            .get(&intent_record.request_id)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        if stage5cj_ack_is_terminal(ack.status) {
            if !request_events.is_empty() {
                return Err(stage5cj_block(
                    Stage5cPaperBrokerLifecycleError::EventForTerminalAck,
                    resolved,
                ));
            }
            continue;
        }
        if request_events.is_empty() {
            return Err(stage5cj_block(
                Stage5cPaperBrokerLifecycleError::MissingExpectedBrokerEvent,
                resolved,
            ));
        }
        let mut terminal_seen = false;
        let mut simulated_position_qty = stage5cj_position_qty(Strategy::state(&resolved.strategy));
        for record in request_events {
            if record.payload.source_ts_utc() < ack.processed_ts_utc {
                return Err(stage5cj_block(
                    Stage5cPaperBrokerLifecycleError::EventTimestampBeforeAck,
                    resolved,
                ));
            }
            if !stage5cj_allowed_event_kinds(intent_record).contains(&record.payload.kind()) {
                return Err(stage5cj_block(
                    Stage5cPaperBrokerLifecycleError::UnexpectedBrokerEventKind,
                    resolved,
                ));
            }
            let validation = stage5cj_validate_event_mapping(
                record,
                ack,
                intent_record,
                &admission_strategy_id,
                simulated_position_qty,
            );
            if let Err(failure) = validation {
                return Err(match failure {
                    Stage5cPaperBrokerLifecycleFailure::Blocked(blocked) => {
                        Stage5cPaperBrokerLifecycleFailure::Blocked(blocked)
                    }
                    Stage5cPaperBrokerLifecycleFailure::Terminal(reason) => {
                        stage5cj_block(reason, resolved)
                    }
                });
            }
            if stage5cj_event_is_terminal_for_intent(record, intent_record, request_events) {
                terminal_seen = true;
            }
            if let Stage5cPaperBrokerEventPayload::Position(position) = &record.payload {
                simulated_position_qty = position.qty;
            }
        }
        if !terminal_seen {
            remaining_lifecycle_expectations.push(Stage5cPaperBrokerLifecycleExpectation {
                request_id: intent_record.request_id,
                expected_event_kind: stage5cj_next_expected_event_kind(
                    intent_record,
                    request_events,
                ),
                reason: "terminal_lifecycle_not_observed".to_string(),
            });
        }
    }
    let Stage5cResolvedPaperIntentBatchStrategy {
        mut strategy,
        recovery_receipt,
        resolved_batch: batch,
        ack_outcomes,
        mut settled_batch_history,
    } = resolved;
    let admission = &recovery_receipt
        .warmup_receipt()
        .restore_receipt()
        .bootstrap_receipt()
        .admission;
    let broker_event_count = canonical_event_records.len();
    let lifecycle_watermark_ts_utc =
        stage5ck_lifecycle_watermark_ts_utc(&batch, &ack_outcomes, &canonical_event_records);
    let mut generated_intent_batch: Option<Stage5cPaperIntentBatch> = None;
    for record in canonical_event_records {
        let Some(_intent_record) = batch
            .records
            .iter()
            .find(|intent| intent.request_id == record.request_id)
        else {
            return Err(Stage5cPaperBrokerLifecycleFailure::Terminal(
                Stage5cPaperBrokerLifecycleError::UnknownEventRequestId,
            ));
        };
        let Some(_ack) = ack_by_request.get(&record.request_id) else {
            return Err(Stage5cPaperBrokerLifecycleFailure::Terminal(
                Stage5cPaperBrokerLifecycleError::UnknownEventRequestId,
            ));
        };
        let source_ts = record.payload.source_ts_utc();
        let context = stage5cj_broker_lifecycle_context(
            &strategy,
            admission,
            batch.bar_close_ts(),
            source_ts,
        );
        let cleanup_ledger = stage5cj_cleanup_attribution_ledger(
            Strategy::state(&strategy),
            admission.strategy_id(),
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
            let expected_attribution_by_request =
                stage5cj_expected_generated_attribution_by_request_from_ledger(
                    admission,
                    source_ts,
                    &intents,
                    &cleanup_ledger,
                )
                .map_err(|_| {
                    Stage5cPaperBrokerLifecycleFailure::Terminal(
                        Stage5cPaperBrokerLifecycleError::CallbackGeneratedIntentTerminal,
                    )
                })?;
            let callback_batch = stage5c_build_paper_intent_batch(
                &strategy,
                admission,
                source_ts,
                broker_core::HybridRuntimeBarOrigin::Live,
                intents,
                &expected_attribution_by_request,
            )
            .map_err(|_| {
                Stage5cPaperBrokerLifecycleFailure::Terminal(
                    Stage5cPaperBrokerLifecycleError::CallbackGeneratedIntentTerminal,
                )
            })?;
            stage5cj_merge_generated_batch(&mut generated_intent_batch, callback_batch).map_err(
                |_| {
                    Stage5cPaperBrokerLifecycleFailure::Terminal(
                        Stage5cPaperBrokerLifecycleError::CallbackGeneratedIntentTerminal,
                    )
                },
            )?;
        }
    }
    if let Some(generated_batch) = &mut generated_intent_batch {
        stage5cj_verify_generated_batch_final_pending_consistency(
            Strategy::state(&strategy),
            generated_batch,
        )
        .map_err(|_| {
            Stage5cPaperBrokerLifecycleFailure::Terminal(
                Stage5cPaperBrokerLifecycleError::CallbackGeneratedIntentTerminal,
            )
        })?;
        generated_batch.state_fingerprint = stage5c_state_fingerprint(Strategy::state(&strategy));
        settled_batch_history.push(stage5ch_batch_summary(generated_batch));
    }
    let resolved_batch_summary = stage5ch_batch_summary(&batch);
    Ok(Stage5cBrokerLifecycleResolvedPaperStrategy {
        strategy,
        recovery_receipt,
        resolved_batch: batch,
        resolved_batch_summary,
        ack_outcomes,
        broker_event_count,
        remaining_lifecycle_expectations,
        lifecycle_watermark_ts_utc,
        generated_intent_batch,
        settled_batch_history,
    })
}

pub fn resolve_stage5c_paper_timer(
    resolved: Stage5cBrokerLifecycleResolvedPaperStrategy,
    input: Stage5cPaperTimerInput,
) -> Result<Stage5cTimerResolvedPaperStrategy, Stage5cPaperTimerFailure> {
    if !resolved.remaining_lifecycle_expectations.is_empty() {
        return Err(stage5ck_block(
            Stage5cPaperTimerError::UnresolvedBrokerLifecycle,
            resolved,
        ));
    }
    if resolved.generated_intent_batch.is_some() {
        return Err(stage5ck_block(
            Stage5cPaperTimerError::UnresolvedGeneratedIntentBatch,
            resolved,
        ));
    }
    let lifecycle_watermark_ts_utc_ms = resolved.lifecycle_watermark_ts_utc.saturating_mul(1_000);
    if input.now_ts_utc_ms < lifecycle_watermark_ts_utc_ms {
        return Err(stage5ck_block(
            Stage5cPaperTimerError::NonMonotonicTimer,
            resolved,
        ));
    }
    let Some(timer_now) = Utc.timestamp_millis_opt(input.now_ts_utc_ms).single() else {
        return Err(stage5ck_block(
            Stage5cPaperTimerError::BrokerTruthExpired,
            resolved,
        ));
    };
    if timer_now
        > resolved
            .recovery_receipt
            .warmup_receipt()
            .restore_receipt()
            .bootstrap_receipt()
            .expires_at()
    {
        return Err(stage5ck_block(
            Stage5cPaperTimerError::BrokerTruthExpired,
            resolved,
        ));
    }
    let Stage5cBrokerLifecycleResolvedPaperStrategy {
        mut strategy,
        recovery_receipt,
        resolved_batch_summary,
        mut settled_batch_history,
        lifecycle_watermark_ts_utc,
        ..
    } = resolved;
    let admission = &recovery_receipt
        .warmup_receipt()
        .restore_receipt()
        .bootstrap_receipt()
        .admission;
    let timer_ts_utc = input.now_ts_utc_ms.div_euclid(1_000);
    let cleanup_ledger =
        stage5cj_cleanup_attribution_ledger(Strategy::state(&strategy), admission.strategy_id());
    let context = stage5ck_timer_context(&strategy, admission, lifecycle_watermark_ts_utc, input);
    let intents = crate::BrokerNeutralHybridStrategy::on_broker_timer(
        &mut strategy,
        broker_core::HybridRuntimeCallbackInput {
            context,
            payload: broker_core::HybridRuntimeTimerEvent {
                now_ts_utc_ms: input.now_ts_utc_ms,
            },
        },
    )
    .map_err(|_| {
        Stage5cPaperTimerFailure::Terminal(Stage5cPaperTimerError::CallbackValidationFailed)
    })?;
    let generated_intent_batch = if intents.is_empty() {
        None
    } else {
        let expected_attribution_by_request =
            stage5cj_expected_generated_attribution_by_request_from_ledger(
                admission,
                timer_ts_utc,
                &intents,
                &cleanup_ledger,
            )
            .map_err(|_| {
                Stage5cPaperTimerFailure::Terminal(Stage5cPaperTimerError::GeneratedIntentTerminal)
            })?;
        let batch = stage5c_build_paper_intent_batch(
            &strategy,
            admission,
            timer_ts_utc,
            broker_core::HybridRuntimeBarOrigin::Live,
            intents,
            &expected_attribution_by_request,
        )
        .map_err(|_| {
            Stage5cPaperTimerFailure::Terminal(Stage5cPaperTimerError::GeneratedIntentTerminal)
        })?;
        stage5cj_verify_generated_batch_final_pending_consistency(
            Strategy::state(&strategy),
            &batch,
        )
        .map_err(|_| {
            Stage5cPaperTimerFailure::Terminal(Stage5cPaperTimerError::GeneratedIntentTerminal)
        })?;
        settled_batch_history.push(stage5ch_batch_summary(&batch));
        Some(batch)
    };
    Ok(Stage5cTimerResolvedPaperStrategy {
        strategy,
        recovery_receipt,
        resolved_batch_summary,
        timer_ts_utc_ms: input.now_ts_utc_ms,
        generated_intent_batch,
        settled_batch_history,
    })
}

pub fn settle_stage5c_timer_result(
    timer: Stage5cTimerResolvedPaperStrategy,
) -> Stage5cTimerSettlement {
    let Stage5cTimerResolvedPaperStrategy {
        strategy,
        recovery_receipt,
        resolved_batch_summary,
        timer_ts_utc_ms,
        generated_intent_batch,
        mut settled_batch_history,
    } = timer;
    match generated_intent_batch {
        Some(batch) => {
            Stage5cTimerSettlement::generated_intent_batch(Stage5cSettledPaperStrategy {
                strategy,
                recovery_receipt,
                batch,
                settled_batch_history,
            })
        }
        None => {
            let batch = stage5cl_zero_timer_batch(
                &strategy,
                &recovery_receipt,
                &resolved_batch_summary,
                timer_ts_utc_ms,
            );
            settled_batch_history.push(stage5ch_batch_summary(&batch));
            Stage5cTimerSettlement::ready_for_continuation(
                Stage5cSettledPaperStrategy {
                    strategy,
                    recovery_receipt,
                    batch,
                    settled_batch_history,
                },
                timer_ts_utc_ms,
            )
        }
    }
}

pub fn advance_stage5c_timer_settlement_next_bar(
    settlement: Stage5cTimerSettlement,
    accepted: Stage5cAcceptedSemanticBar,
) -> Result<Stage5cSettledPaperStrategy, Stage5cTimerContinuationFailure> {
    advance_stage5c_timer_settlement_next_bar_at(settlement, accepted, Utc::now())
}

fn advance_stage5c_timer_settlement_next_bar_at(
    settlement: Stage5cTimerSettlement,
    accepted: Stage5cAcceptedSemanticBar,
    now: DateTime<Utc>,
) -> Result<Stage5cSettledPaperStrategy, Stage5cTimerContinuationFailure> {
    let (settled, checkpoint_ts_utc_ms) = match settlement.inner {
        Stage5cTimerSettlementKind::ReadyForContinuation {
            settled,
            checkpoint_ts_utc_ms,
        } => (settled, checkpoint_ts_utc_ms),
        Stage5cTimerSettlementKind::GeneratedIntentBatch(settled) => {
            return Err(stage5cm_block(
                Stage5cTimerContinuationError::GeneratedIntentBatchRequiresLifecycle,
                Stage5cTimerSettlement::generated_intent_batch(settled),
            ));
        }
    };
    match advance_stage5c_controlled_next_bar_at(settled, accepted, now) {
        Ok(advanced) => Ok(advanced),
        Err(Stage5cNextBarLoopFailure::Blocked(blocked)) => {
            let reason = blocked.reason();
            Err(stage5cm_block(
                Stage5cTimerContinuationError::NextBar(reason),
                Stage5cTimerSettlement::ready_for_continuation(
                    blocked.into_settled(),
                    checkpoint_ts_utc_ms,
                ),
            ))
        }
        Err(Stage5cNextBarLoopFailure::Failed(reason)) => {
            Err(Stage5cTimerContinuationFailure::Terminal(
                Stage5cTimerContinuationError::NextBar(reason),
            ))
        }
    }
}

pub fn advance_stage5c_timer_settlement_timer(
    settlement: Stage5cTimerSettlement,
    input: Stage5cPaperTimerInput,
) -> Result<Stage5cTimerResolvedPaperStrategy, Stage5cTimerContinuationFailure> {
    match settlement.inner {
        Stage5cTimerSettlementKind::ReadyForContinuation {
            settled,
            checkpoint_ts_utc_ms,
        } => stage5cm_advance_timer_from_settled(settled, input, checkpoint_ts_utc_ms),
        Stage5cTimerSettlementKind::GeneratedIntentBatch(settled) => Err(stage5cm_block(
            Stage5cTimerContinuationError::GeneratedIntentBatchRequiresLifecycle,
            Stage5cTimerSettlement::generated_intent_batch(settled),
        )),
    }
}

fn stage5ck_block(
    reason: Stage5cPaperTimerError,
    resolved: Stage5cBrokerLifecycleResolvedPaperStrategy,
) -> Stage5cPaperTimerFailure {
    Stage5cPaperTimerFailure::Blocked(Box::new(Stage5cPaperTimerBlocked { reason, resolved }))
}

fn stage5ck_timer_context(
    strategy: &HybridIntradayRuntimeStrategy,
    admission: &Stage5cPaperHostAdmission,
    lifecycle_watermark_ts_utc: i64,
    input: Stage5cPaperTimerInput,
) -> broker_core::HybridRuntimeStrategyContext {
    let timer_ts_utc = input.now_ts_utc_ms.div_euclid(1_000);
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
        event_ts_utc: timer_ts_utc,
        strategy_now_ts_utc: timer_ts_utc,
        last_bar_ts_utc: Some(lifecycle_watermark_ts_utc),
    }
}

fn stage5ck_lifecycle_watermark_ts_utc(
    batch: &Stage5cPaperIntentBatch,
    ack_outcomes: &[Stage5cPaperAckOutcome],
    event_records: &[Stage5cPaperBrokerEventRecord],
) -> i64 {
    let mut watermark = batch.bar_close_ts();
    for record in &batch.records {
        watermark = watermark.max(record.source_event_ts);
    }
    for ack in ack_outcomes {
        watermark = watermark.max(ack.processed_ts_utc);
    }
    for record in event_records {
        watermark = watermark.max(record.payload.source_ts_utc());
    }
    watermark
}

fn stage5cl_zero_timer_batch(
    strategy: &HybridIntradayRuntimeStrategy,
    recovery_receipt: &Stage5cPendingRecoveryReceipt,
    resolved_batch_summary: &Stage5cPaperIntentBatchSummary,
    timer_ts_utc_ms: i64,
) -> Stage5cPaperIntentBatch {
    let admission = &recovery_receipt
        .warmup_receipt()
        .restore_receipt()
        .bootstrap_receipt()
        .admission;
    let timer_ts_utc = timer_ts_utc_ms.div_euclid(1_000);
    Stage5cPaperIntentBatch {
        strategy_id: admission.strategy_id().to_string(),
        account_id: admission.account_id().clone(),
        instrument: admission.target_instrument().clone(),
        bar_close_ts: timer_ts_utc,
        state_fingerprint: stage5c_state_fingerprint(Strategy::state(strategy)),
        request_ids: Vec::new(),
        records: Vec::new(),
        observation_only: resolved_batch_summary.observation_only,
    }
}

fn stage5cm_block(
    reason: Stage5cTimerContinuationError,
    settlement: Stage5cTimerSettlement,
) -> Stage5cTimerContinuationFailure {
    Stage5cTimerContinuationFailure::Blocked(Box::new(Stage5cTimerContinuationBlocked {
        reason,
        settlement,
    }))
}

fn stage5cm_advance_timer_from_settled(
    settled: Stage5cSettledPaperStrategy,
    input: Stage5cPaperTimerInput,
    checkpoint_ts_utc_ms: i64,
) -> Result<Stage5cTimerResolvedPaperStrategy, Stage5cTimerContinuationFailure> {
    if settled.batch.intent_count() > 0 {
        return Err(stage5cm_block(
            Stage5cTimerContinuationError::GeneratedIntentBatchRequiresLifecycle,
            Stage5cTimerSettlement::generated_intent_batch(settled),
        ));
    }
    if input.now_ts_utc_ms <= checkpoint_ts_utc_ms {
        return Err(stage5cm_block(
            Stage5cTimerContinuationError::NonMonotonicTimer,
            Stage5cTimerSettlement::ready_for_continuation(settled, checkpoint_ts_utc_ms),
        ));
    }
    let Some(timer_now) = Utc.timestamp_millis_opt(input.now_ts_utc_ms).single() else {
        return Err(stage5cm_block(
            Stage5cTimerContinuationError::BrokerTruthExpired,
            Stage5cTimerSettlement::ready_for_continuation(settled, checkpoint_ts_utc_ms),
        ));
    };
    if timer_now
        > settled
            .recovery_receipt
            .warmup_receipt()
            .restore_receipt()
            .bootstrap_receipt()
            .expires_at()
    {
        return Err(stage5cm_block(
            Stage5cTimerContinuationError::BrokerTruthExpired,
            Stage5cTimerSettlement::ready_for_continuation(settled, checkpoint_ts_utc_ms),
        ));
    }
    let Stage5cSettledPaperStrategy {
        mut strategy,
        recovery_receipt,
        batch,
        mut settled_batch_history,
    } = settled;
    let admission = &recovery_receipt
        .warmup_receipt()
        .restore_receipt()
        .bootstrap_receipt()
        .admission;
    let timer_ts_utc = input.now_ts_utc_ms.div_euclid(1_000);
    let cleanup_ledger =
        stage5cj_cleanup_attribution_ledger(Strategy::state(&strategy), admission.strategy_id());
    let context = stage5ck_timer_context(&strategy, admission, batch.bar_close_ts(), input);
    let intents = crate::BrokerNeutralHybridStrategy::on_broker_timer(
        &mut strategy,
        broker_core::HybridRuntimeCallbackInput {
            context,
            payload: broker_core::HybridRuntimeTimerEvent {
                now_ts_utc_ms: input.now_ts_utc_ms,
            },
        },
    )
    .map_err(|_| {
        Stage5cTimerContinuationFailure::Terminal(
            Stage5cTimerContinuationError::CallbackValidationFailed,
        )
    })?;
    let generated_intent_batch = if intents.is_empty() {
        None
    } else {
        let expected_attribution_by_request =
            stage5cj_expected_generated_attribution_by_request_from_ledger(
                admission,
                timer_ts_utc,
                &intents,
                &cleanup_ledger,
            )
            .map_err(|_| {
                Stage5cTimerContinuationFailure::Terminal(
                    Stage5cTimerContinuationError::GeneratedIntentTerminal,
                )
            })?;
        let generated_batch = stage5c_build_paper_intent_batch(
            &strategy,
            admission,
            timer_ts_utc,
            broker_core::HybridRuntimeBarOrigin::Live,
            intents,
            &expected_attribution_by_request,
        )
        .map_err(|_| {
            Stage5cTimerContinuationFailure::Terminal(
                Stage5cTimerContinuationError::GeneratedIntentTerminal,
            )
        })?;
        stage5cj_verify_generated_batch_final_pending_consistency(
            Strategy::state(&strategy),
            &generated_batch,
        )
        .map_err(|_| {
            Stage5cTimerContinuationFailure::Terminal(
                Stage5cTimerContinuationError::GeneratedIntentTerminal,
            )
        })?;
        settled_batch_history.push(stage5ch_batch_summary(&generated_batch));
        Some(generated_batch)
    };
    let resolved_batch_summary = stage5ch_batch_summary(&batch);
    Ok(Stage5cTimerResolvedPaperStrategy {
        strategy,
        recovery_receipt,
        resolved_batch_summary,
        timer_ts_utc_ms: input.now_ts_utc_ms,
        generated_intent_batch,
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

fn stage5cj_merge_generated_batch(
    target: &mut Option<Stage5cPaperIntentBatch>,
    mut next: Stage5cPaperIntentBatch,
) -> Result<(), Stage5cIntentSettlementError> {
    let Some(existing) = target else {
        *target = Some(next);
        return Ok(());
    };
    let mut seen: HashSet<_> = existing.request_ids.iter().copied().collect();
    for request_id in &next.request_ids {
        if !seen.insert(*request_id) {
            return Err(Stage5cIntentSettlementError::DuplicateRequestId);
        }
    }
    existing.bar_close_ts = existing.bar_close_ts.min(next.bar_close_ts);
    existing.request_ids.append(&mut next.request_ids);
    existing.records.append(&mut next.records);
    Ok(())
}

fn stage5cj_verify_generated_batch_final_pending_consistency(
    final_state: &StrategyState,
    generated_batch: &Stage5cPaperIntentBatch,
) -> Result<(), Stage5cIntentSettlementError> {
    for record in &generated_batch.records {
        stage5cg_verify_pending_request_id(final_state, record.intent_class, record.request_id)?;
    }
    Ok(())
}

fn stage5cj_event_identity(
    record: &Stage5cPaperBrokerEventRecord,
) -> Result<String, serde_json::Error> {
    match &record.payload {
        Stage5cPaperBrokerEventPayload::Order(order) => serde_json::to_string(&(
            record.request_id,
            Stage5cPaperBrokerEventKind::Order,
            &order.order_id,
            &order.status,
            order.source_ts_utc,
        )),
        Stage5cPaperBrokerEventPayload::StopOrder(stop) => serde_json::to_string(&(
            record.request_id,
            Stage5cPaperBrokerEventKind::StopOrder,
            &stop.stop_order_id,
            &stop.status,
            stop.source_ts_utc,
        )),
        Stage5cPaperBrokerEventPayload::Position(position) => serde_json::to_string(&(
            record.request_id,
            Stage5cPaperBrokerEventKind::Position,
            position.source_ts_utc,
        )),
    }
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

fn stage5cj_allowed_event_kinds(
    intent_record: &Stage5cPaperIntentRecord,
) -> Vec<Stage5cPaperBrokerEventKind> {
    use crate::BrokerNeutralHybridIntent as Intent;
    match intent_record.intent.base_intent() {
        Intent::Market { .. } => vec![Stage5cPaperBrokerEventKind::Position],
        Intent::Place { .. } => match intent_record.intent_class {
            crate::BrokerNeutralHybridIntentClass::Entry
            | crate::BrokerNeutralHybridIntentClass::Exit
            | crate::BrokerNeutralHybridIntentClass::ProtectiveRepair => vec![
                Stage5cPaperBrokerEventKind::Order,
                Stage5cPaperBrokerEventKind::Position,
            ],
            crate::BrokerNeutralHybridIntentClass::CancelCleanup => {
                vec![Stage5cPaperBrokerEventKind::Order]
            }
        },
        Intent::Cancel { .. } | Intent::Replace { .. } => {
            vec![Stage5cPaperBrokerEventKind::Order]
        }
        Intent::CreateStopLimit { .. } => vec![
            Stage5cPaperBrokerEventKind::StopOrder,
            Stage5cPaperBrokerEventKind::Position,
        ],
        Intent::DeleteStopLimit { .. } => vec![Stage5cPaperBrokerEventKind::StopOrder],
        Intent::Classified { .. } | Intent::Routed { .. } => {
            unreachable!("base_intent unwraps wrappers")
        }
    }
}

fn stage5cj_next_expected_event_kind(
    intent_record: &Stage5cPaperIntentRecord,
    events: &[Stage5cPaperBrokerEventRecord],
) -> Stage5cPaperBrokerEventKind {
    if stage5cj_lifecycle_has_execution_order_like_event_before(intent_record, events, None) {
        Stage5cPaperBrokerEventKind::Position
    } else {
        stage5cj_expected_event_kind(&intent_record.intent)
    }
}

fn stage5cj_validate_event_mapping(
    record: &Stage5cPaperBrokerEventRecord,
    ack: &Stage5cPaperAckOutcome,
    intent_record: &Stage5cPaperIntentRecord,
    admission_strategy_id: &str,
    pre_position_qty: f64,
) -> Result<(), Stage5cPaperBrokerLifecycleFailure> {
    let intent = &intent_record.intent;
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
            stage5cj_validate_attribution(
                order.attribution.as_ref(),
                admission_strategy_id,
                stage5cj_expected_order_role(intent_record),
                intent_record.expected_attribution.as_ref(),
            )?;
            stage5cj_validate_order_fields(order, intent)?;
        }
        Stage5cPaperBrokerEventPayload::StopOrder(stop) => {
            if let Some(expected) = &ack.broker_order_id {
                if stop.exchange_order_id.as_ref() != Some(expected) {
                    return Err(Stage5cPaperBrokerLifecycleFailure::Terminal(
                        Stage5cPaperBrokerLifecycleError::BrokerOrderIdMismatch,
                    ));
                }
            }
            stage5cj_validate_attribution(
                stop.attribution.as_ref(),
                admission_strategy_id,
                stage5cj_expected_order_role(intent_record),
                intent_record.expected_attribution.as_ref(),
            )?;
            stage5cj_validate_stop_fields(stop, intent)?;
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
        Stage5cPaperBrokerEventPayload::Position(position) => match intent.base_intent() {
            crate::BrokerNeutralHybridIntent::Market { .. }
            | crate::BrokerNeutralHybridIntent::Place { .. }
            | crate::BrokerNeutralHybridIntent::CreateStopLimit { .. } => {
                stage5cj_validate_position_transition(
                    intent_record.intent_class,
                    intent,
                    pre_position_qty,
                    position.qty,
                    position.existing,
                )?;
            }
            _ => {
                return Err(Stage5cPaperBrokerLifecycleFailure::Terminal(
                    Stage5cPaperBrokerLifecycleError::UnexpectedBrokerEventKind,
                ));
            }
        },
    }
    Ok(())
}

fn stage5cj_validate_position_transition(
    intent_class: crate::BrokerNeutralHybridIntentClass,
    intent: &crate::BrokerNeutralHybridIntent,
    pre_position_qty: f64,
    new_position_qty: f64,
    existing: bool,
) -> Result<(), Stage5cPaperBrokerLifecycleFailure> {
    if !existing {
        return Err(Stage5cPaperBrokerLifecycleFailure::Terminal(
            Stage5cPaperBrokerLifecycleError::PositionEventRequiresMarketIntent,
        ));
    }
    match intent_class {
        crate::BrokerNeutralHybridIntentClass::Entry => {
            let Some((side, target_qty)) = stage5cj_entry_side_and_target_qty(intent) else {
                return Err(Stage5cPaperBrokerLifecycleFailure::Terminal(
                    Stage5cPaperBrokerLifecycleError::PositionEventRequiresMarketIntent,
                ));
            };
            let signed_ok = match side {
                crate::BrokerNeutralOrderSide::Buy => new_position_qty > f64::EPSILON,
                crate::BrokerNeutralOrderSide::Sell => new_position_qty < -f64::EPSILON,
            };
            if !signed_ok {
                return Err(Stage5cPaperBrokerLifecycleFailure::Terminal(
                    Stage5cPaperBrokerLifecycleError::PositionSideMismatch,
                ));
            }
            if new_position_qty.abs() > target_qty.abs() + f64::EPSILON {
                return Err(Stage5cPaperBrokerLifecycleFailure::Terminal(
                    Stage5cPaperBrokerLifecycleError::PositionOverfill,
                ));
            }
            if pre_position_qty.abs() > f64::EPSILON
                && new_position_qty.signum() == pre_position_qty.signum()
                && new_position_qty.abs() + f64::EPSILON < pre_position_qty.abs()
            {
                return Err(Stage5cPaperBrokerLifecycleFailure::Terminal(
                    Stage5cPaperBrokerLifecycleError::PositionRegression,
                ));
            }
            Ok(())
        }
        crate::BrokerNeutralHybridIntentClass::Exit => {
            if pre_position_qty.abs() > f64::EPSILON && new_position_qty.abs() <= f64::EPSILON {
                Ok(())
            } else {
                Err(Stage5cPaperBrokerLifecycleFailure::Terminal(
                    Stage5cPaperBrokerLifecycleError::PositionEventRequiresMarketIntent,
                ))
            }
        }
        crate::BrokerNeutralHybridIntentClass::ProtectiveRepair => {
            if new_position_qty.abs() <= f64::EPSILON {
                Ok(())
            } else {
                Err(Stage5cPaperBrokerLifecycleFailure::Terminal(
                    Stage5cPaperBrokerLifecycleError::PositionEventRequiresMarketIntent,
                ))
            }
        }
        crate::BrokerNeutralHybridIntentClass::CancelCleanup => {
            Err(Stage5cPaperBrokerLifecycleFailure::Terminal(
                Stage5cPaperBrokerLifecycleError::PositionEventRequiresMarketIntent,
            ))
        }
    }
}

fn stage5cj_entry_side_and_target_qty(
    intent: &crate::BrokerNeutralHybridIntent,
) -> Option<(crate::BrokerNeutralOrderSide, f64)> {
    match intent.base_intent() {
        crate::BrokerNeutralHybridIntent::Market { side, qty, .. }
        | crate::BrokerNeutralHybridIntent::Place { side, qty, .. } => Some((*side, *qty)),
        _ => None,
    }
}

fn stage5cj_validate_attribution(
    attribution: Option<&broker_core::HybridRuntimeAttribution>,
    admission_strategy_id: &str,
    expected_role: Option<broker_core::HybridRuntimeOrderRole>,
    expected_attribution: Option<&broker_core::HybridRuntimeAttribution>,
) -> Result<(), Stage5cPaperBrokerLifecycleFailure> {
    let Some(attribution) = attribution else {
        return Err(Stage5cPaperBrokerLifecycleFailure::Terminal(
            Stage5cPaperBrokerLifecycleError::AttributionMissing,
        ));
    };
    if !attribution.belongs_to(admission_strategy_id) {
        return Err(Stage5cPaperBrokerLifecycleFailure::Terminal(
            Stage5cPaperBrokerLifecycleError::AttributionStrategyMismatch,
        ));
    }
    if expected_role.is_some() && attribution.role() != expected_role {
        return Err(Stage5cPaperBrokerLifecycleFailure::Terminal(
            Stage5cPaperBrokerLifecycleError::AttributionRoleMismatch,
        ));
    }
    if let Some(expected) = expected_attribution {
        if attribution.cycle_id() != expected.cycle_id()
            || attribution.owner() != expected.owner()
            || attribution.role() != expected.role()
        {
            return Err(Stage5cPaperBrokerLifecycleFailure::Terminal(
                Stage5cPaperBrokerLifecycleError::AttributionCycleMismatch,
            ));
        }
    }
    Ok(())
}

fn stage5cj_expected_order_role(
    intent_record: &Stage5cPaperIntentRecord,
) -> Option<broker_core::HybridRuntimeOrderRole> {
    match intent_record.intent.base_intent() {
        crate::BrokerNeutralHybridIntent::Place { .. } => match intent_record.intent_class {
            crate::BrokerNeutralHybridIntentClass::Entry => {
                Some(broker_core::HybridRuntimeOrderRole::Entry)
            }
            crate::BrokerNeutralHybridIntentClass::Exit => {
                Some(broker_core::HybridRuntimeOrderRole::Exit)
            }
            crate::BrokerNeutralHybridIntentClass::ProtectiveRepair => {
                Some(broker_core::HybridRuntimeOrderRole::TakeProfit)
            }
            crate::BrokerNeutralHybridIntentClass::CancelCleanup => {
                Some(broker_core::HybridRuntimeOrderRole::Cancel)
            }
        },
        crate::BrokerNeutralHybridIntent::CreateStopLimit { .. } => {
            Some(broker_core::HybridRuntimeOrderRole::StopLoss)
        }
        crate::BrokerNeutralHybridIntent::Cancel { .. }
        | crate::BrokerNeutralHybridIntent::DeleteStopLimit { .. } => intent_record
            .expected_attribution
            .as_ref()
            .and_then(broker_core::HybridRuntimeAttribution::role),
        crate::BrokerNeutralHybridIntent::Replace { .. } => {
            Some(broker_core::HybridRuntimeOrderRole::TakeProfit)
        }
        crate::BrokerNeutralHybridIntent::Market { .. }
        | crate::BrokerNeutralHybridIntent::Classified { .. }
        | crate::BrokerNeutralHybridIntent::Routed { .. } => None,
    }
}

fn stage5cj_expected_attribution_for_intent(
    state: &StrategyState,
    strategy_id: &str,
    intent_class: crate::BrokerNeutralHybridIntentClass,
    intent: &crate::BrokerNeutralHybridIntent,
) -> Option<broker_core::HybridRuntimeAttribution> {
    if let Some(comment) = stage5cj_expected_comment(intent) {
        return broker_core::HybridRuntimeAttribution::parse_source_comment(comment).ok();
    }
    match (state, intent.base_intent()) {
        (
            StrategyState::HybridIntradayRuntime {
                active_cycle_id,
                current_owner,
                tp_order_id,
                sl_stop_order_id,
                sl_exchange_order_id,
                ..
            },
            crate::BrokerNeutralHybridIntent::Cancel { order_id },
        ) if intent_class == crate::BrokerNeutralHybridIntentClass::CancelCleanup => {
            let role = if tp_order_id.as_ref() == Some(order_id) {
                Some(broker_core::HybridRuntimeOrderRole::TakeProfit)
            } else if sl_exchange_order_id.as_ref() == Some(order_id) {
                Some(broker_core::HybridRuntimeOrderRole::StopLoss)
            } else {
                None
            }?;
            stage5cj_build_expected_attribution(strategy_id, active_cycle_id, current_owner, role)
        }
        (
            StrategyState::HybridIntradayRuntime {
                active_cycle_id,
                current_owner,
                sl_stop_order_id,
                ..
            },
            crate::BrokerNeutralHybridIntent::DeleteStopLimit { order_id, .. },
        ) if intent_class == crate::BrokerNeutralHybridIntentClass::CancelCleanup
            && sl_stop_order_id.as_ref() == Some(order_id) =>
        {
            stage5cj_build_expected_attribution(
                strategy_id,
                active_cycle_id,
                current_owner,
                broker_core::HybridRuntimeOrderRole::StopLoss,
            )
        }
        _ => None,
    }
}

fn stage5cj_cleanup_attribution_ledger(
    state: &StrategyState,
    strategy_id: &str,
) -> Stage5cCleanupAttributionLedger {
    let mut ledger = Stage5cCleanupAttributionLedger::default();
    let StrategyState::HybridIntradayRuntime {
        active_cycle_id,
        current_owner,
        tp_order_id,
        sl_stop_order_id,
        sl_exchange_order_id,
        pending_entry_owner,
        pending_entry_cycle_id,
        ..
    } = state
    else {
        return ledger;
    };
    if let Some(attribution) = stage5cj_build_expected_attribution(
        strategy_id,
        active_cycle_id,
        current_owner,
        broker_core::HybridRuntimeOrderRole::TakeProfit,
    ) {
        if let Some(order_id) = tp_order_id {
            ledger.broker_orders.insert(order_id.clone(), attribution);
        }
    }
    if let Some(attribution) = stage5cj_build_expected_attribution(
        strategy_id,
        active_cycle_id,
        current_owner,
        broker_core::HybridRuntimeOrderRole::StopLoss,
    ) {
        if let Some(order_id) = sl_exchange_order_id {
            ledger
                .broker_orders
                .insert(order_id.clone(), attribution.clone());
        }
        if let Some(stop_order_id) = sl_stop_order_id {
            ledger
                .stop_orders
                .insert(stop_order_id.clone(), attribution);
        }
    }
    ledger.pending_entry_attribution = stage5cj_build_expected_attribution(
        strategy_id,
        pending_entry_cycle_id,
        pending_entry_owner,
        broker_core::HybridRuntimeOrderRole::Entry,
    );
    ledger
}

fn stage5cj_expected_cleanup_attribution_from_ledger(
    ledger: &Stage5cCleanupAttributionLedger,
    intent: &crate::BrokerNeutralHybridIntent,
) -> Option<broker_core::HybridRuntimeAttribution> {
    match intent.base_intent() {
        crate::BrokerNeutralHybridIntent::Cancel { order_id } => ledger
            .broker_orders
            .get(order_id)
            .cloned()
            .or_else(|| ledger.pending_entry_attribution.clone()),
        crate::BrokerNeutralHybridIntent::DeleteStopLimit { order_id, .. } => {
            ledger.stop_orders.get(order_id).cloned()
        }
        _ => None,
    }
}

fn stage5cj_expected_generated_attribution_by_request_from_ledger(
    admission: &Stage5cPaperHostAdmission,
    source_ts: i64,
    intents: &[crate::BrokerNeutralHybridIntent],
    ledger: &Stage5cCleanupAttributionLedger,
) -> Result<
    HashMap<StrategyRequestId, broker_core::HybridRuntimeAttribution>,
    Stage5cIntentSettlementError,
> {
    let mut expected = HashMap::new();
    for intent in intents {
        if let Some(attribution) = stage5cj_expected_cleanup_attribution_from_ledger(ledger, intent)
        {
            let request_id = stage5cg_source_request_id(
                admission.strategy_id(),
                admission.account_id().as_str(),
                &admission.target_instrument().symbol,
                source_ts,
                intent,
            )?;
            expected.insert(request_id, attribution);
        }
    }
    Ok(expected)
}

fn stage5cj_build_expected_attribution(
    strategy_id: &str,
    active_cycle_id: &Option<String>,
    current_owner: &Option<crate::hybrid_intraday::Owner>,
    role: broker_core::HybridRuntimeOrderRole,
) -> Option<broker_core::HybridRuntimeAttribution> {
    let cycle = active_cycle_id.as_deref()?;
    let owner = match current_owner.as_ref()? {
        crate::hybrid_intraday::Owner::MeanReversion => "MR",
        crate::hybrid_intraday::Owner::IntradayBreakout => "BO",
    };
    let role = match role {
        broker_core::HybridRuntimeOrderRole::Entry => "ENTRY",
        broker_core::HybridRuntimeOrderRole::Exit => "EXIT",
        broker_core::HybridRuntimeOrderRole::TakeProfit => "TP",
        broker_core::HybridRuntimeOrderRole::StopLoss => "SL",
        broker_core::HybridRuntimeOrderRole::Cancel => "CANCEL",
        broker_core::HybridRuntimeOrderRole::Repair => "REPAIR",
    };
    broker_core::HybridRuntimeAttribution::parse_source_comment(format!(
        "HYB|sid={strategy_id}|c={cycle}|o={owner}|r={role}"
    ))
    .ok()
}

fn stage5cj_expected_comment(intent: &crate::BrokerNeutralHybridIntent) -> Option<&str> {
    match intent.base_intent() {
        crate::BrokerNeutralHybridIntent::Place { comment, .. }
        | crate::BrokerNeutralHybridIntent::Market { comment, .. }
        | crate::BrokerNeutralHybridIntent::CreateStopLimit { comment, .. } => comment.as_deref(),
        _ => None,
    }
}

fn stage5cj_validate_order_fields(
    order: &broker_core::HybridRuntimeOrderEvent,
    intent: &crate::BrokerNeutralHybridIntent,
) -> Result<(), Stage5cPaperBrokerLifecycleFailure> {
    if !stage5cj_order_status_is_known(&order.status) {
        return Err(Stage5cPaperBrokerLifecycleFailure::Terminal(
            Stage5cPaperBrokerLifecycleError::UnknownOrderStatus,
        ));
    }
    match intent.base_intent() {
        crate::BrokerNeutralHybridIntent::Place {
            price, qty, side, ..
        } if !stage5cj_side_matches(&order.side, *side)
            || !stage5cj_f64_eq(order.qty, *qty)
            || !stage5cj_f64_eq(order.price, *price)
            || !order.order_type.eq_ignore_ascii_case("limit") =>
        {
            return Err(Stage5cPaperBrokerLifecycleFailure::Terminal(
                Stage5cPaperBrokerLifecycleError::IntentFieldMismatch,
            ));
        }
        crate::BrokerNeutralHybridIntent::Cancel { order_id } => {
            if &order.order_id != order_id {
                return Err(Stage5cPaperBrokerLifecycleFailure::Terminal(
                    Stage5cPaperBrokerLifecycleError::BrokerOrderIdMismatch,
                ));
            }
            if !stage5cj_order_status_is_cancel_terminal(&order.status)
                && !stage5cj_order_status_is_working(&order.status)
            {
                return Err(Stage5cPaperBrokerLifecycleFailure::Terminal(
                    Stage5cPaperBrokerLifecycleError::IntentFieldMismatch,
                ));
            }
        }
        crate::BrokerNeutralHybridIntent::Replace {
            order_id,
            new_price,
            new_qty,
        } if &order.order_id != order_id
            || !stage5cj_f64_eq(order.price, *new_price)
            || !stage5cj_f64_eq(order.qty, *new_qty) =>
        {
            return Err(Stage5cPaperBrokerLifecycleFailure::Terminal(
                Stage5cPaperBrokerLifecycleError::IntentFieldMismatch,
            ));
        }
        _ => {}
    }
    Ok(())
}

fn stage5cj_validate_stop_fields(
    stop: &broker_core::HybridRuntimeStopOrderEvent,
    intent: &crate::BrokerNeutralHybridIntent,
) -> Result<(), Stage5cPaperBrokerLifecycleFailure> {
    if !stage5cj_stop_status_is_known(&stop.status) {
        return Err(Stage5cPaperBrokerLifecycleFailure::Terminal(
            Stage5cPaperBrokerLifecycleError::UnknownStopOrderStatus,
        ));
    }
    match intent.base_intent() {
        crate::BrokerNeutralHybridIntent::CreateStopLimit {
            side,
            qty,
            trigger_price,
            price,
            stop_end_unix_time,
            ..
        } if !stage5cj_side_matches(&stop.side, *side)
            || !stage5cj_f64_eq(stop.qty, *qty)
            || !stage5cj_f64_eq(stop.stop_price, *trigger_price)
            || !stage5cj_f64_eq(stop.price, *price)
            || stop.end_ts_utc != Some(*stop_end_unix_time) =>
        {
            return Err(Stage5cPaperBrokerLifecycleFailure::Terminal(
                Stage5cPaperBrokerLifecycleError::IntentFieldMismatch,
            ));
        }
        crate::BrokerNeutralHybridIntent::DeleteStopLimit { order_id, side, .. }
            if &stop.stop_order_id != order_id
                || side.is_some_and(|expected| !stage5cj_side_matches(&stop.side, expected)) =>
        {
            return Err(Stage5cPaperBrokerLifecycleFailure::Terminal(
                Stage5cPaperBrokerLifecycleError::IntentFieldMismatch,
            ));
        }
        _ => {}
    }
    Ok(())
}

fn stage5cj_event_is_terminal_for_intent(
    record: &Stage5cPaperBrokerEventRecord,
    intent_record: &Stage5cPaperIntentRecord,
    events: &[Stage5cPaperBrokerEventRecord],
) -> bool {
    match (&record.payload, intent_record.intent.base_intent()) {
        (
            Stage5cPaperBrokerEventPayload::Position(position),
            crate::BrokerNeutralHybridIntent::Market { qty, .. },
        ) => stage5cj_market_position_is_terminal(intent_record.intent_class, *qty, position.qty),
        (
            Stage5cPaperBrokerEventPayload::Position(position),
            crate::BrokerNeutralHybridIntent::Place { .. },
        ) => {
            stage5cj_lifecycle_has_execution_order_like_event_before(
                intent_record,
                events,
                Some(record.total_sequence),
            ) && stage5cj_position_confirms_place_terminal(
                intent_record.intent_class,
                &intent_record.intent,
                position.qty,
            )
        }
        (
            Stage5cPaperBrokerEventPayload::Position(position),
            crate::BrokerNeutralHybridIntent::CreateStopLimit { .. },
        ) => {
            stage5cj_lifecycle_has_execution_order_like_event_before(
                intent_record,
                events,
                Some(record.total_sequence),
            ) && position.qty.abs() <= f64::EPSILON
        }
        (
            Stage5cPaperBrokerEventPayload::Order(order),
            crate::BrokerNeutralHybridIntent::Place { .. },
        ) => stage5cj_order_status_is_cancel_terminal(&order.status),
        (
            Stage5cPaperBrokerEventPayload::Order(order),
            crate::BrokerNeutralHybridIntent::Cancel { .. },
        ) => stage5cj_order_status_is_cancel_terminal(&order.status),
        (
            Stage5cPaperBrokerEventPayload::Order(order),
            crate::BrokerNeutralHybridIntent::Replace { .. },
        ) => stage5cj_order_status_is_known(&order.status),
        (
            Stage5cPaperBrokerEventPayload::StopOrder(stop),
            crate::BrokerNeutralHybridIntent::DeleteStopLimit { .. },
        ) => stage5cj_stop_status_is_terminal(&stop.status),
        (
            Stage5cPaperBrokerEventPayload::StopOrder(stop),
            crate::BrokerNeutralHybridIntent::CreateStopLimit { .. },
        ) => stage5cj_stop_status_is_non_execution_terminal(&stop.status),
        _ => false,
    }
}

fn stage5cj_lifecycle_has_execution_order_like_event_before(
    intent_record: &Stage5cPaperIntentRecord,
    events: &[Stage5cPaperBrokerEventRecord],
    before_total_sequence: Option<u64>,
) -> bool {
    events.iter().any(
        |event| match (&event.payload, intent_record.intent.base_intent()) {
            _ if before_total_sequence.is_some_and(|before| event.total_sequence >= before) => {
                false
            }
            (
                Stage5cPaperBrokerEventPayload::Order(order),
                crate::BrokerNeutralHybridIntent::Place { .. },
            ) => stage5cj_order_status_is_filled(&order.status),
            (
                Stage5cPaperBrokerEventPayload::StopOrder(stop),
                crate::BrokerNeutralHybridIntent::CreateStopLimit { .. },
            ) => stage5cj_stop_status_is_execution(&stop.status),
            _ => false,
        },
    )
}

fn stage5cj_market_position_is_terminal(
    intent_class: crate::BrokerNeutralHybridIntentClass,
    target_qty: f64,
    position_qty: f64,
) -> bool {
    match intent_class {
        crate::BrokerNeutralHybridIntentClass::Entry => {
            position_qty.abs() + f64::EPSILON >= target_qty.abs()
        }
        crate::BrokerNeutralHybridIntentClass::Exit => position_qty.abs() <= f64::EPSILON,
        crate::BrokerNeutralHybridIntentClass::ProtectiveRepair
        | crate::BrokerNeutralHybridIntentClass::CancelCleanup => false,
    }
}

fn stage5cj_position_confirms_place_terminal(
    intent_class: crate::BrokerNeutralHybridIntentClass,
    intent: &crate::BrokerNeutralHybridIntent,
    position_qty: f64,
) -> bool {
    match intent_class {
        crate::BrokerNeutralHybridIntentClass::Entry => match intent.base_intent() {
            crate::BrokerNeutralHybridIntent::Place { qty, .. } => {
                position_qty.abs() + f64::EPSILON >= qty.abs()
            }
            _ => false,
        },
        crate::BrokerNeutralHybridIntentClass::Exit
        | crate::BrokerNeutralHybridIntentClass::ProtectiveRepair => {
            position_qty.abs() <= f64::EPSILON
        }
        crate::BrokerNeutralHybridIntentClass::CancelCleanup => false,
    }
}

fn stage5cj_order_status_is_working(status: &str) -> bool {
    matches!(
        status.to_ascii_lowercase().as_str(),
        "working" | "active" | "accepted" | "new" | "partially_filled" | "partial"
    )
}

fn stage5cj_order_status_is_filled(status: &str) -> bool {
    matches!(
        status.to_ascii_lowercase().as_str(),
        "filled" | "done" | "completed"
    )
}

fn stage5cj_order_status_is_cancel_terminal(status: &str) -> bool {
    matches!(
        status.to_ascii_lowercase().as_str(),
        "canceled" | "cancelled" | "expired" | "rejected"
    )
}

fn stage5cj_order_status_is_known(status: &str) -> bool {
    stage5cj_order_status_is_working(status)
        || stage5cj_order_status_is_filled(status)
        || stage5cj_order_status_is_cancel_terminal(status)
}

fn stage5cj_stop_status_is_terminal(status: &str) -> bool {
    stage5cj_stop_status_is_execution(status)
        || stage5cj_stop_status_is_non_execution_terminal(status)
}

fn stage5cj_stop_status_is_execution(status: &str) -> bool {
    matches!(
        status.to_ascii_lowercase().as_str(),
        "triggered" | "filled" | "executed" | "done" | "completed"
    )
}

fn stage5cj_stop_status_is_non_execution_terminal(status: &str) -> bool {
    matches!(
        status.to_ascii_lowercase().as_str(),
        "canceled" | "cancelled" | "expired" | "rejected"
    )
}

fn stage5cj_stop_status_is_known(status: &str) -> bool {
    matches!(
        status.to_ascii_lowercase().as_str(),
        "working" | "active" | "accepted" | "new"
    ) || stage5cj_stop_status_is_terminal(status)
}

fn stage5cj_side_matches(actual: &str, expected: crate::BrokerNeutralOrderSide) -> bool {
    matches!(
        (actual.to_ascii_lowercase().as_str(), expected),
        ("buy", crate::BrokerNeutralOrderSide::Buy) | ("sell", crate::BrokerNeutralOrderSide::Sell)
    )
}

fn stage5cj_f64_eq(left: f64, right: f64) -> bool {
    (left - right).abs() <= 1e-9
}

fn stage5cj_position_qty(state: &StrategyState) -> f64 {
    match state {
        StrategyState::HybridIntradayRuntime {
            last_position_qty, ..
        } => *last_position_qty,
        StrategyState::Idle => 0.0,
    }
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
        Intent::Cancel { order_id } => Ok(crate::deterministic_request_id(
            strategy_id,
            account_id,
            symbol,
            &format!("cancel:{}", order_id.as_str()),
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
        Intent::DeleteStopLimit { order_id, .. } => Ok(crate::deterministic_request_id(
            strategy_id,
            account_id,
            symbol,
            &format!("delete_stop_limit:{}", order_id.as_str()),
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
            expected_attribution_by_request: HashMap::new(),
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
            comment: Some("HYB|sid=hybrid_imoexf|c=abc1230001|o=MR|r=TP".to_string()),
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
            comment: Some("HYB|sid=hybrid_imoexf|c=abc1230001|o=MR|r=SL".to_string()),
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

    fn stage5cj_attribution(role: &str) -> broker_core::HybridRuntimeAttribution {
        stage5cj_attribution_with_cycle(role, "abc1230001")
    }

    fn stage5cj_attribution_with_cycle(
        role: &str,
        cycle: &str,
    ) -> broker_core::HybridRuntimeAttribution {
        broker_core::HybridRuntimeAttribution::parse_source_comment(format!(
            "HYB|sid=hybrid_imoexf|c={cycle}|o=MR|r={role}"
        ))
        .unwrap()
    }

    fn stage5cj_order_event(
        total_sequence: u64,
        request_id: StrategyRequestId,
        order_id: BrokerOrderId,
        status: &str,
        source_ts_utc: i64,
    ) -> Stage5cPaperBrokerEventRecord {
        Stage5cPaperBrokerEventRecord {
            total_sequence,
            request_id,
            payload: Stage5cPaperBrokerEventPayload::Order(broker_core::HybridRuntimeOrderEvent {
                order_id,
                request_id: Some(request_id),
                instrument: target(),
                status: status.to_string(),
                side: "sell".to_string(),
                order_type: "limit".to_string(),
                qty: 1.0,
                filled_qty: if stage5cj_order_status_is_filled(status) {
                    1.0
                } else {
                    0.0
                },
                price: 2230.0,
                existing: true,
                attribution: Some(stage5cj_attribution("TP")),
                source_ts_utc,
            }),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn stage5cj_order_event_with_role(
        total_sequence: u64,
        request_id: StrategyRequestId,
        order_id: BrokerOrderId,
        status: &str,
        side: &str,
        price: f64,
        role: &str,
        source_ts_utc: i64,
    ) -> Stage5cPaperBrokerEventRecord {
        Stage5cPaperBrokerEventRecord {
            total_sequence,
            request_id,
            payload: Stage5cPaperBrokerEventPayload::Order(broker_core::HybridRuntimeOrderEvent {
                order_id,
                request_id: Some(request_id),
                instrument: target(),
                status: status.to_string(),
                side: side.to_string(),
                order_type: "limit".to_string(),
                qty: 1.0,
                filled_qty: if stage5cj_order_status_is_filled(status) {
                    1.0
                } else {
                    0.0
                },
                price,
                existing: true,
                attribution: Some(stage5cj_attribution(role)),
                source_ts_utc,
            }),
        }
    }

    fn stage5cj_stop_event(
        total_sequence: u64,
        request_id: StrategyRequestId,
        exchange_order_id: BrokerOrderId,
        status: &str,
        end_ts_utc: i64,
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
                    status: status.to_string(),
                    side: "sell".to_string(),
                    qty: 1.0,
                    filled_qty: 0.0,
                    stop_price: 2210.0,
                    price: 2209.5,
                    existing: true,
                    attribution: Some(stage5cj_attribution("SL")),
                    end_ts_utc: Some(end_ts_utc),
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

    fn stage5ci_exit_settled() -> (Stage5cSettledPaperStrategy, StrategyRequestId, i64) {
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
            4,
        );
        let mut state = Strategy::state(&strategy).clone();
        match &mut state {
            StrategyState::HybridIntradayRuntime {
                active_cycle_id,
                last_position_qty,
                current_side,
                pending_exit_request_id,
                ..
            } => {
                *active_cycle_id = Some("abc1230001".to_string());
                *last_position_qty = 1.0;
                *current_side = Some(crate::hybrid_intraday::Side::Long);
                *pending_exit_request_id = Some(expected_request_id);
            }
            StrategyState::Idle => panic!("expected hybrid runtime state"),
        }
        Strategy::set_state(&mut strategy, state);
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

    fn stage5cj_place_entry_settled(
        side: crate::BrokerNeutralOrderSide,
        qty: f64,
    ) -> (Stage5cSettledPaperStrategy, StrategyRequestId, i64) {
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
        let request_id = crate::deterministic_request_id(
            "hybrid_imoexf",
            "ACC_TEST_0001",
            "IMOEXF",
            "place",
            bar_close_ts,
            0,
        );
        set_hybrid_pending_request(
            &mut strategy,
            crate::BrokerNeutralHybridIntentClass::Entry,
            request_id,
        );
        let intent = crate::BrokerNeutralHybridIntent::Place {
            price: 2227.5,
            qty,
            side,
            comment: Some("HYB|sid=hybrid_imoexf|c=abc1230001|o=MR|r=ENTRY".to_string()),
        }
        .with_class(crate::BrokerNeutralHybridIntentClass::Entry)
        .with_symbol("IMOEXF");
        let settled = settle_stage5c_semantic_result(stage5cg_semantic_result(
            strategy,
            recovery_receipt,
            bar_close_ts,
            broker_core::HybridRuntimeBarOrigin::Live,
            vec![intent],
        ))
        .unwrap();
        (settled, request_id, bar_close_ts)
    }

    fn stage5cj_place_order_event(
        total_sequence: u64,
        request_id: StrategyRequestId,
        status: &str,
        side: &str,
        qty: f64,
        source_ts_utc: i64,
    ) -> Stage5cPaperBrokerEventRecord {
        let mut event = stage5cj_order_event_with_role(
            total_sequence,
            request_id,
            BrokerOrderId::new("ORDER_TEST_ACK_0001"),
            status,
            side,
            2227.5,
            "ENTRY",
            source_ts_utc,
        );
        if let Stage5cPaperBrokerEventPayload::Order(order) = &mut event.payload {
            order.qty = qty;
            order.filled_qty = if stage5cj_order_status_is_filled(status) {
                qty
            } else {
                0.0
            };
        }
        event
    }

    fn stage5cj_cleanup_cancel_settled() -> (Stage5cSettledPaperStrategy, StrategyRequestId, i64) {
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
        let request_id = crate::deterministic_request_id(
            "hybrid_imoexf",
            "ACC_TEST_0001",
            "IMOEXF",
            "cancel:ORDER_TEST_0001",
            bar_close_ts,
            1,
        );
        let mut state = Strategy::state(&strategy).clone();
        match &mut state {
            StrategyState::HybridIntradayRuntime {
                active_cycle_id,
                current_owner,
                tp_order_id,
                ..
            } => {
                *active_cycle_id = Some("abc1230001".to_string());
                *current_owner = Some(crate::hybrid_intraday::Owner::MeanReversion);
                *tp_order_id = Some(BrokerOrderId::new("ORDER_TEST_0001"));
            }
            StrategyState::Idle => panic!("expected hybrid runtime state"),
        }
        Strategy::set_state(&mut strategy, state);
        let intent = crate::BrokerNeutralHybridIntent::Cancel {
            order_id: BrokerOrderId::new("ORDER_TEST_0001"),
        }
        .with_class(crate::BrokerNeutralHybridIntentClass::CancelCleanup)
        .with_symbol("IMOEXF");
        let settled = settle_stage5c_semantic_result(stage5cg_semantic_result(
            strategy,
            recovery_receipt,
            bar_close_ts,
            broker_core::HybridRuntimeBarOrigin::Live,
            vec![intent],
        ))
        .unwrap();
        (settled, request_id, bar_close_ts)
    }

    fn stage5cj_cleanup_delete_stop_settled(
    ) -> (Stage5cSettledPaperStrategy, StrategyRequestId, i64) {
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
        let request_id = crate::deterministic_request_id(
            "hybrid_imoexf",
            "ACC_TEST_0001",
            "IMOEXF",
            "delete_stop_limit:STOP_TEST_0001",
            bar_close_ts,
            6,
        );
        let mut state = Strategy::state(&strategy).clone();
        match &mut state {
            StrategyState::HybridIntradayRuntime {
                active_cycle_id,
                current_owner,
                sl_stop_order_id,
                ..
            } => {
                *active_cycle_id = Some("abc1230001".to_string());
                *current_owner = Some(crate::hybrid_intraday::Owner::MeanReversion);
                *sl_stop_order_id = Some(BrokerStopOrderId::new("STOP_TEST_0001"));
            }
            StrategyState::Idle => panic!("expected hybrid runtime state"),
        }
        Strategy::set_state(&mut strategy, state);
        let intent = crate::BrokerNeutralHybridIntent::DeleteStopLimit {
            order_id: BrokerStopOrderId::new("STOP_TEST_0001"),
            side: Some(crate::BrokerNeutralOrderSide::Sell),
            check_duplicates: Some(true),
        }
        .with_class(crate::BrokerNeutralHybridIntentClass::CancelCleanup)
        .with_symbol("IMOEXF");
        let settled = settle_stage5c_semantic_result(stage5cg_semantic_result(
            strategy,
            recovery_receipt,
            bar_close_ts,
            broker_core::HybridRuntimeBarOrigin::Live,
            vec![intent],
        ))
        .unwrap();
        (settled, request_id, bar_close_ts)
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
            expected_attribution_by_request: HashMap::new(),
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
                expected_attribution_by_request: HashMap::new(),
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
        let wall_now = Utc::now();
        let now = wall_now - chrono::Duration::hours(3);
        let recovered = empty_recovered_until(now, wall_now + chrono::Duration::hours(1));
        let (strategy, recovery_receipt) = recovered.into_parts();
        let first_close_ts = wall_now.timestamp().div_euclid(600) * 600 - 7_200;
        let settled = settle_stage5c_semantic_result(Stage5cSemanticBarResult {
            strategy,
            recovery_receipt,
            bar_close_ts: first_close_ts,
            origin: broker_core::HybridRuntimeBarOrigin::Live,
            execution_eligible: true,
            intents: Vec::new(),
            expected_attribution_by_request: HashMap::new(),
        })
        .unwrap();
        assert_eq!(settled.settled_batch_history().len(), 1);
        let next_close_ts = first_close_ts + 600;
        let accepted = accept_stage5c_semantic_bar(semantic_input(next_close_ts)).unwrap();
        let advanced = advance_stage5c_controlled_next_bar_at(
            settled,
            accepted,
            DateTime::<Utc>::from_timestamp(next_close_ts + 30, 0).unwrap(),
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
            expected_attribution_by_request: HashMap::new(),
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
            expected_attribution_by_request: HashMap::new(),
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
            expected_attribution_by_request: HashMap::new(),
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
            expected_attribution_by_request: HashMap::new(),
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
            expected_attribution_by_request: HashMap::new(),
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
    fn stage5cj_market_exit_accepts_flat_position_event_and_rejects_nonflat() {
        let (settled, request_id, bar_close_ts) = stage5ci_exit_settled();
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
                    0.0,
                    bar_close_ts + 2,
                )],
            },
        )
        .unwrap();
        assert_eq!(
            match Strategy::state(broker_resolved.strategy()) {
                StrategyState::HybridIntradayRuntime {
                    last_position_qty,
                    pending_exit_request_id,
                    ..
                } => (*last_position_qty, *pending_exit_request_id),
                StrategyState::Idle => (f64::NAN, None),
            },
            (0.0, None)
        );

        let (settled, request_id, bar_close_ts) = stage5ci_exit_settled();
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
                    event_records: vec![stage5cj_position_event(
                        2,
                        request_id,
                        1.0,
                        bar_close_ts + 2
                    )]
                },
            )
            .expect_err("exit lifecycle must finish flat")
            .reason(),
            Stage5cPaperBrokerLifecycleError::PositionEventRequiresMarketIntent
        );
    }

    #[test]
    fn stage5cj_market_entry_checks_position_direction() {
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
                    event_records: vec![stage5cj_position_event(
                        2,
                        request_id,
                        -1.0,
                        bar_close_ts + 2
                    )]
                },
            )
            .expect_err("buy entry cannot settle into short broker position")
            .reason(),
            Stage5cPaperBrokerLifecycleError::PositionSideMismatch
        );

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
        let request_id = crate::deterministic_request_id(
            "hybrid_imoexf",
            "ACC_TEST_0001",
            "IMOEXF",
            "market",
            bar_close_ts,
            4,
        );
        set_hybrid_pending_request(
            &mut strategy,
            crate::BrokerNeutralHybridIntentClass::Entry,
            request_id,
        );
        let settled = settle_stage5c_semantic_result(stage5cg_semantic_result(
            strategy,
            recovery_receipt,
            bar_close_ts,
            broker_core::HybridRuntimeBarOrigin::Live,
            vec![stage5cg_market_intent(
                crate::BrokerNeutralOrderSide::Sell,
                crate::BrokerNeutralHybridIntentClass::Entry,
            )],
        ))
        .unwrap();
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
                    event_records: vec![stage5cj_position_event(
                        2,
                        request_id,
                        1.0,
                        bar_close_ts + 2
                    )]
                },
            )
            .expect_err("sell entry cannot settle into long broker position")
            .reason(),
            Stage5cPaperBrokerLifecycleError::PositionSideMismatch
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
            "filled",
            bar_close_ts + 2,
        );
        let stop_event = stage5cj_stop_event(
            4,
            sl_request_id,
            BrokerOrderId::new("ORDER_TEST_ACK_0001"),
            "working",
            bar_close_ts + 600,
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
        assert_eq!(broker_b.remaining_lifecycle_expectations().len(), 2);
    }

    #[test]
    fn stage5cj_place_lifecycle_accepts_working_then_filled_and_preserves_full_batch() {
        let (settled, tp_request_id, sl_request_id, bar_close_ts) = stage5ci_protective_settled();
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
                            sl_request_id,
                            broker_core::HybridRuntimeAckStatus::Rejected,
                            bar_close_ts + 1,
                        ),
                    },
                ],
            },
        )
        .unwrap();
        let broker_resolved = resolve_stage5c_paper_broker_lifecycle(
            resolved,
            Stage5cPaperBrokerLifecycleInput {
                event_records: vec![
                    stage5cj_order_event(
                        4,
                        tp_request_id,
                        BrokerOrderId::new("ORDER_TEST_ACK_0001"),
                        "filled",
                        bar_close_ts + 3,
                    ),
                    stage5cj_order_event(
                        3,
                        tp_request_id,
                        BrokerOrderId::new("ORDER_TEST_ACK_0001"),
                        "working",
                        bar_close_ts + 2,
                    ),
                    stage5cj_position_event(5, tp_request_id, 0.0, bar_close_ts + 4),
                ],
            },
        )
        .unwrap();
        assert_eq!(broker_resolved.broker_event_count(), 3);
        assert!(broker_resolved
            .remaining_lifecycle_expectations()
            .is_empty());
        assert_eq!(broker_resolved.full_resolved_batch().intent_count(), 2);
    }

    #[test]
    fn stage5cj_order_and_stop_events_require_valid_attribution() {
        let (settled, tp_request_id, sl_request_id, bar_close_ts) = stage5ci_protective_settled();
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
                            sl_request_id,
                            broker_core::HybridRuntimeAckStatus::Rejected,
                            bar_close_ts + 1,
                        ),
                    },
                ],
            },
        )
        .unwrap();
        let mut event = stage5cj_order_event(
            3,
            tp_request_id,
            BrokerOrderId::new("ORDER_TEST_ACK_0001"),
            "working",
            bar_close_ts + 2,
        );
        if let Stage5cPaperBrokerEventPayload::Order(order) = &mut event.payload {
            order.attribution = None;
        }
        assert_eq!(
            resolve_stage5c_paper_broker_lifecycle(
                resolved,
                Stage5cPaperBrokerLifecycleInput {
                    event_records: vec![event]
                },
            )
            .expect_err("source wrapper would ignore unattributed order events")
            .reason(),
            Stage5cPaperBrokerLifecycleError::AttributionMissing
        );
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
                        "filled",
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
                        "filled",
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
    fn stage5cj_marketable_limit_entry_exit_accept_source_roles_and_position_confirmation() {
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
        let entry_request_id = crate::deterministic_request_id(
            "hybrid_imoexf",
            "ACC_TEST_0001",
            "IMOEXF",
            "place",
            bar_close_ts,
            0,
        );
        set_hybrid_pending_request(
            &mut strategy,
            crate::BrokerNeutralHybridIntentClass::Entry,
            entry_request_id,
        );
        let entry_intent = crate::BrokerNeutralHybridIntent::Place {
            price: 2230.0,
            qty: 1.0,
            side: crate::BrokerNeutralOrderSide::Buy,
            comment: Some("HYB|sid=hybrid_imoexf|c=abc1230001|o=MR|r=ENTRY".to_string()),
        }
        .with_class(crate::BrokerNeutralHybridIntentClass::Entry)
        .with_symbol("IMOEXF");
        let settled = settle_stage5c_semantic_result(stage5cg_semantic_result(
            strategy,
            recovery_receipt,
            bar_close_ts,
            broker_core::HybridRuntimeBarOrigin::Live,
            vec![entry_intent],
        ))
        .unwrap();
        let resolved = resolve_stage5c_paper_intent_lifecycle(
            settled,
            Stage5cPaperIntentLifecycleInput {
                ack_records: vec![stage5ci_ack_record(1, entry_request_id)],
            },
        )
        .unwrap();
        let broker_resolved = resolve_stage5c_paper_broker_lifecycle(
            resolved,
            Stage5cPaperBrokerLifecycleInput {
                event_records: vec![
                    stage5cj_order_event_with_role(
                        2,
                        entry_request_id,
                        BrokerOrderId::new("ORDER_TEST_ACK_0001"),
                        "filled",
                        "buy",
                        2230.0,
                        "ENTRY",
                        bar_close_ts + 2,
                    ),
                    stage5cj_position_event(3, entry_request_id, 1.0, bar_close_ts + 3),
                ],
            },
        )
        .unwrap();
        assert!(broker_resolved
            .remaining_lifecycle_expectations()
            .is_empty());

        let recovered = empty_recovered_until(
            now,
            Utc.with_ymd_and_hms(2026, 7, 13, 9, 40, 30)
                .single()
                .unwrap(),
        );
        let (mut strategy, recovery_receipt) = recovered.into_parts();
        let exit_request_id = crate::deterministic_request_id(
            "hybrid_imoexf",
            "ACC_TEST_0001",
            "IMOEXF",
            "place",
            bar_close_ts,
            0,
        );
        let mut state = Strategy::state(&strategy).clone();
        match &mut state {
            StrategyState::HybridIntradayRuntime {
                active_cycle_id,
                last_position_qty,
                current_side,
                pending_exit_request_id,
                ..
            } => {
                *active_cycle_id = Some("abc1230001".to_string());
                *last_position_qty = 1.0;
                *current_side = Some(crate::hybrid_intraday::Side::Long);
                *pending_exit_request_id = Some(exit_request_id);
            }
            StrategyState::Idle => panic!("expected hybrid runtime state"),
        }
        Strategy::set_state(&mut strategy, state);
        let exit_intent = crate::BrokerNeutralHybridIntent::Place {
            price: 2220.0,
            qty: 1.0,
            side: crate::BrokerNeutralOrderSide::Sell,
            comment: Some("HYB|sid=hybrid_imoexf|c=abc1230001|o=MR|r=EXIT".to_string()),
        }
        .with_class(crate::BrokerNeutralHybridIntentClass::Exit)
        .with_symbol("IMOEXF");
        set_hybrid_pending_request(
            &mut strategy,
            crate::BrokerNeutralHybridIntentClass::Exit,
            exit_request_id,
        );
        let settled = settle_stage5c_semantic_result(stage5cg_semantic_result(
            strategy,
            recovery_receipt,
            bar_close_ts,
            broker_core::HybridRuntimeBarOrigin::Live,
            vec![exit_intent],
        ))
        .unwrap();
        let resolved = resolve_stage5c_paper_intent_lifecycle(
            settled,
            Stage5cPaperIntentLifecycleInput {
                ack_records: vec![stage5ci_ack_record(1, exit_request_id)],
            },
        )
        .unwrap();
        let broker_resolved = resolve_stage5c_paper_broker_lifecycle(
            resolved,
            Stage5cPaperBrokerLifecycleInput {
                event_records: vec![
                    stage5cj_order_event_with_role(
                        2,
                        exit_request_id,
                        BrokerOrderId::new("ORDER_TEST_ACK_0001"),
                        "filled",
                        "sell",
                        2220.0,
                        "EXIT",
                        bar_close_ts + 2,
                    ),
                    stage5cj_position_event(3, exit_request_id, 0.0, bar_close_ts + 3),
                ],
            },
        )
        .unwrap();
        assert!(broker_resolved
            .remaining_lifecycle_expectations()
            .is_empty());
    }

    #[test]
    fn stage5cj_tp_cancel_accepts_original_tp_attribution_and_wrong_cycle_blocks() {
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
        let cancel_request_id = crate::deterministic_request_id(
            "hybrid_imoexf",
            "ACC_TEST_0001",
            "IMOEXF",
            "cancel:ORDER_TEST_0001",
            bar_close_ts,
            1,
        );
        let cancel_intent = crate::BrokerNeutralHybridIntent::Cancel {
            order_id: BrokerOrderId::new("ORDER_TEST_0001"),
        }
        .with_class(crate::BrokerNeutralHybridIntentClass::CancelCleanup)
        .with_symbol("IMOEXF");
        let mut state = Strategy::state(&strategy).clone();
        match &mut state {
            StrategyState::HybridIntradayRuntime {
                active_cycle_id,
                current_owner,
                tp_order_id,
                ..
            } => {
                *active_cycle_id = Some("abc1230001".to_string());
                *current_owner = Some(crate::hybrid_intraday::Owner::MeanReversion);
                *tp_order_id = Some(BrokerOrderId::new("ORDER_TEST_0001"));
            }
            StrategyState::Idle => panic!("expected hybrid runtime state"),
        }
        Strategy::set_state(&mut strategy, state);
        let settled = settle_stage5c_semantic_result(stage5cg_semantic_result(
            strategy,
            recovery_receipt,
            bar_close_ts,
            broker_core::HybridRuntimeBarOrigin::Live,
            vec![cancel_intent],
        ))
        .unwrap();
        let resolved = resolve_stage5c_paper_intent_lifecycle(
            settled,
            Stage5cPaperIntentLifecycleInput {
                ack_records: vec![Stage5cPaperAckRecord {
                    total_sequence: 1,
                    ack: broker_core::HybridRuntimeCommandAck {
                        broker_order_id: Some(BrokerOrderId::new("ORDER_TEST_0001")),
                        ..stage5ci_ack(cancel_request_id)
                    },
                }],
            },
        )
        .unwrap();
        let broker_resolved = resolve_stage5c_paper_broker_lifecycle(
            resolved,
            Stage5cPaperBrokerLifecycleInput {
                event_records: vec![stage5cj_order_event_with_role(
                    2,
                    cancel_request_id,
                    BrokerOrderId::new("ORDER_TEST_0001"),
                    "canceled",
                    "sell",
                    2230.0,
                    "TP",
                    bar_close_ts + 2,
                )],
            },
        )
        .unwrap();
        assert!(broker_resolved
            .remaining_lifecycle_expectations()
            .is_empty());

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
        let mut event = stage5cj_order_event(
            3,
            tp_request_id,
            BrokerOrderId::new("ORDER_TEST_ACK_0001"),
            "working",
            bar_close_ts + 2,
        );
        if let Stage5cPaperBrokerEventPayload::Order(order) = &mut event.payload {
            order.attribution = Some(stage5cj_attribution_with_cycle("TP", "deadbeef01"));
        }
        assert_eq!(
            resolve_stage5c_paper_broker_lifecycle(
                resolved,
                Stage5cPaperBrokerLifecycleInput {
                    event_records: vec![event]
                },
            )
            .expect_err("wrong HYB cycle must be blocked before source callback")
            .reason(),
            Stage5cPaperBrokerLifecycleError::AttributionCycleMismatch
        );
    }

    #[test]
    fn stage5cj_triggered_and_executed_stop_require_position_confirmation() {
        for status in ["triggered", "executed"] {
            let (settled, tp_request_id, sl_request_id, bar_close_ts) =
                stage5ci_protective_settled();
            let resolved = resolve_stage5c_paper_intent_lifecycle(
                settled,
                Stage5cPaperIntentLifecycleInput {
                    ack_records: vec![
                        Stage5cPaperAckRecord {
                            total_sequence: 1,
                            ack: stage5ci_ack_with(
                                tp_request_id,
                                broker_core::HybridRuntimeAckStatus::Rejected,
                                bar_close_ts + 1,
                            ),
                        },
                        Stage5cPaperAckRecord {
                            total_sequence: 2,
                            ack: stage5ci_ack_with(
                                sl_request_id,
                                broker_core::HybridRuntimeAckStatus::Accepted,
                                bar_close_ts + 1,
                            ),
                        },
                    ],
                },
            )
            .unwrap();
            let broker_resolved = resolve_stage5c_paper_broker_lifecycle(
                resolved,
                Stage5cPaperBrokerLifecycleInput {
                    event_records: vec![stage5cj_stop_event(
                        3,
                        sl_request_id,
                        BrokerOrderId::new("ORDER_TEST_ACK_0001"),
                        status,
                        bar_close_ts + 600,
                        bar_close_ts + 2,
                    )],
                },
            )
            .unwrap();
            assert_eq!(
                broker_resolved.remaining_lifecycle_expectations()[0].expected_event_kind,
                Stage5cPaperBrokerEventKind::Position
            );
        }
    }

    #[test]
    fn stage5cj_canceled_and_rejected_stop_are_terminal_without_position() {
        for status in ["canceled", "rejected"] {
            let (settled, tp_request_id, sl_request_id, bar_close_ts) =
                stage5ci_protective_settled();
            let resolved = resolve_stage5c_paper_intent_lifecycle(
                settled,
                Stage5cPaperIntentLifecycleInput {
                    ack_records: vec![
                        Stage5cPaperAckRecord {
                            total_sequence: 1,
                            ack: stage5ci_ack_with(
                                tp_request_id,
                                broker_core::HybridRuntimeAckStatus::Rejected,
                                bar_close_ts + 1,
                            ),
                        },
                        Stage5cPaperAckRecord {
                            total_sequence: 2,
                            ack: stage5ci_ack_with(
                                sl_request_id,
                                broker_core::HybridRuntimeAckStatus::Accepted,
                                bar_close_ts + 1,
                            ),
                        },
                    ],
                },
            )
            .unwrap();
            let broker_resolved = resolve_stage5c_paper_broker_lifecycle(
                resolved,
                Stage5cPaperBrokerLifecycleInput {
                    event_records: vec![stage5cj_stop_event(
                        3,
                        sl_request_id,
                        BrokerOrderId::new("ORDER_TEST_ACK_0001"),
                        status,
                        bar_close_ts + 600,
                        bar_close_ts + 2,
                    )],
                },
            )
            .unwrap();
            assert!(broker_resolved
                .remaining_lifecycle_expectations()
                .is_empty());
        }
    }

    #[test]
    fn stage5cj_place_non_execution_terminal_finishes_without_position() {
        for status in ["canceled", "expired", "rejected"] {
            let (settled, request_id, bar_close_ts) =
                stage5cj_place_entry_settled(crate::BrokerNeutralOrderSide::Buy, 1.0);
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
                    event_records: vec![stage5cj_place_order_event(
                        2,
                        request_id,
                        status,
                        "buy",
                        1.0,
                        bar_close_ts + 2,
                    )],
                },
            )
            .unwrap();
            assert!(broker_resolved
                .remaining_lifecycle_expectations()
                .is_empty());
        }
    }

    #[test]
    fn stage5cj_place_filled_still_requires_position_confirmation() {
        let (settled, request_id, bar_close_ts) =
            stage5cj_place_entry_settled(crate::BrokerNeutralOrderSide::Buy, 1.0);
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
                event_records: vec![stage5cj_place_order_event(
                    2,
                    request_id,
                    "filled",
                    "buy",
                    1.0,
                    bar_close_ts + 2,
                )],
            },
        )
        .unwrap();
        assert_eq!(
            broker_resolved.remaining_lifecycle_expectations()[0].expected_event_kind,
            Stage5cPaperBrokerEventKind::Position
        );
    }

    #[test]
    fn stage5cj_partial_entry_position_reduction_blocks_before_callback() {
        let (settled, request_id, bar_close_ts) =
            stage5cj_place_entry_settled(crate::BrokerNeutralOrderSide::Buy, 3.0);
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
                        stage5cj_place_order_event(
                            2,
                            request_id,
                            "filled",
                            "buy",
                            3.0,
                            bar_close_ts + 2,
                        ),
                        stage5cj_position_event(3, request_id, 1.0, bar_close_ts + 3),
                        stage5cj_position_event(4, request_id, 0.5, bar_close_ts + 4),
                    ]
                },
            )
            .expect_err("partial entry regression must be blocked before source callback")
            .reason(),
            Stage5cPaperBrokerLifecycleError::PositionRegression
        );
    }

    #[test]
    fn stage5cj_place_entry_rejects_wrong_side_position() {
        let (settled, request_id, bar_close_ts) =
            stage5cj_place_entry_settled(crate::BrokerNeutralOrderSide::Buy, 1.0);
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
                        stage5cj_place_order_event(
                            2,
                            request_id,
                            "filled",
                            "buy",
                            1.0,
                            bar_close_ts + 2
                        ),
                        stage5cj_position_event(3, request_id, -1.0, bar_close_ts + 3),
                    ]
                },
            )
            .expect_err("place buy cannot settle into short broker position")
            .reason(),
            Stage5cPaperBrokerLifecycleError::PositionSideMismatch
        );

        let (settled, request_id, bar_close_ts) =
            stage5cj_place_entry_settled(crate::BrokerNeutralOrderSide::Sell, 1.0);
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
                        stage5cj_place_order_event(
                            2,
                            request_id,
                            "filled",
                            "sell",
                            1.0,
                            bar_close_ts + 2
                        ),
                        stage5cj_position_event(3, request_id, 1.0, bar_close_ts + 3),
                    ]
                },
            )
            .expect_err("place sell cannot settle into long broker position")
            .reason(),
            Stage5cPaperBrokerLifecycleError::PositionSideMismatch
        );
    }

    #[test]
    fn stage5cj_partial_place_entry_keeps_expectation_and_target_closes() {
        let (settled, request_id, bar_close_ts) =
            stage5cj_place_entry_settled(crate::BrokerNeutralOrderSide::Buy, 3.0);
        let resolved = resolve_stage5c_paper_intent_lifecycle(
            settled,
            Stage5cPaperIntentLifecycleInput {
                ack_records: vec![stage5ci_ack_record(1, request_id)],
            },
        )
        .unwrap();
        let partial = resolve_stage5c_paper_broker_lifecycle(
            resolved,
            Stage5cPaperBrokerLifecycleInput {
                event_records: vec![
                    stage5cj_place_order_event(
                        2,
                        request_id,
                        "filled",
                        "buy",
                        3.0,
                        bar_close_ts + 2,
                    ),
                    stage5cj_position_event(3, request_id, 1.0, bar_close_ts + 3),
                ],
            },
        )
        .unwrap();
        assert_eq!(
            partial.remaining_lifecycle_expectations()[0].expected_event_kind,
            Stage5cPaperBrokerEventKind::Position
        );

        let (settled, request_id, bar_close_ts) =
            stage5cj_place_entry_settled(crate::BrokerNeutralOrderSide::Buy, 3.0);
        let resolved = resolve_stage5c_paper_intent_lifecycle(
            settled,
            Stage5cPaperIntentLifecycleInput {
                ack_records: vec![stage5ci_ack_record(1, request_id)],
            },
        )
        .unwrap();
        let complete = resolve_stage5c_paper_broker_lifecycle(
            resolved,
            Stage5cPaperBrokerLifecycleInput {
                event_records: vec![
                    stage5cj_place_order_event(
                        2,
                        request_id,
                        "filled",
                        "buy",
                        3.0,
                        bar_close_ts + 2,
                    ),
                    stage5cj_position_event(3, request_id, 3.0, bar_close_ts + 3),
                ],
            },
        )
        .unwrap();
        assert!(complete.remaining_lifecycle_expectations().is_empty());
    }

    #[test]
    fn stage5cj_place_and_market_entry_overfill_block_before_callback() {
        let (settled, request_id, bar_close_ts) =
            stage5cj_place_entry_settled(crate::BrokerNeutralOrderSide::Buy, 1.0);
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
                        stage5cj_place_order_event(
                            2,
                            request_id,
                            "filled",
                            "buy",
                            1.0,
                            bar_close_ts + 2
                        ),
                        stage5cj_position_event(3, request_id, 2.0, bar_close_ts + 3),
                    ]
                },
            )
            .expect_err("place overfill must be blocked before source callback")
            .reason(),
            Stage5cPaperBrokerLifecycleError::PositionOverfill
        );

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
                    event_records: vec![stage5cj_position_event(
                        2,
                        request_id,
                        2.0,
                        bar_close_ts + 2
                    )]
                },
            )
            .expect_err("market overfill must be blocked before source callback")
            .reason(),
            Stage5cPaperBrokerLifecycleError::PositionOverfill
        );
    }

    #[test]
    fn stage5cj_tp_cancel_rejects_wrong_role_and_cycle() {
        for (role, cycle, expected) in [
            (
                "ENTRY",
                "abc1230001",
                Stage5cPaperBrokerLifecycleError::AttributionRoleMismatch,
            ),
            (
                "TP",
                "deadbeef01",
                Stage5cPaperBrokerLifecycleError::AttributionCycleMismatch,
            ),
        ] {
            let (settled, request_id, bar_close_ts) = stage5cj_cleanup_cancel_settled();
            let resolved = resolve_stage5c_paper_intent_lifecycle(
                settled,
                Stage5cPaperIntentLifecycleInput {
                    ack_records: vec![Stage5cPaperAckRecord {
                        total_sequence: 1,
                        ack: broker_core::HybridRuntimeCommandAck {
                            broker_order_id: Some(BrokerOrderId::new("ORDER_TEST_0001")),
                            ..stage5ci_ack(request_id)
                        },
                    }],
                },
            )
            .unwrap();
            let mut event = stage5cj_order_event_with_role(
                2,
                request_id,
                BrokerOrderId::new("ORDER_TEST_0001"),
                "canceled",
                "sell",
                2230.0,
                role,
                bar_close_ts + 2,
            );
            if let Stage5cPaperBrokerEventPayload::Order(order) = &mut event.payload {
                order.attribution = Some(stage5cj_attribution_with_cycle(role, cycle));
            }
            assert_eq!(
                resolve_stage5c_paper_broker_lifecycle(
                    resolved,
                    Stage5cPaperBrokerLifecycleInput {
                        event_records: vec![event]
                    },
                )
                .expect_err("TP cancel must preserve original attribution")
                .reason(),
                expected
            );
        }
    }

    #[test]
    fn stage5cj_sl_delete_rejects_wrong_role_and_cycle() {
        for (role, cycle, expected) in [
            (
                "TP",
                "abc1230001",
                Stage5cPaperBrokerLifecycleError::AttributionRoleMismatch,
            ),
            (
                "SL",
                "deadbeef01",
                Stage5cPaperBrokerLifecycleError::AttributionCycleMismatch,
            ),
        ] {
            let (settled, request_id, bar_close_ts) = stage5cj_cleanup_delete_stop_settled();
            let resolved = resolve_stage5c_paper_intent_lifecycle(
                settled,
                Stage5cPaperIntentLifecycleInput {
                    ack_records: vec![Stage5cPaperAckRecord {
                        total_sequence: 1,
                        ack: broker_core::HybridRuntimeCommandAck {
                            broker_order_id: Some(BrokerOrderId::new("ORDER_TEST_ACK_0001")),
                            ..stage5ci_ack(request_id)
                        },
                    }],
                },
            )
            .unwrap();
            let mut event = stage5cj_stop_event(
                2,
                request_id,
                BrokerOrderId::new("ORDER_TEST_ACK_0001"),
                "canceled",
                bar_close_ts + 600,
                bar_close_ts + 2,
            );
            if let Stage5cPaperBrokerEventPayload::StopOrder(stop) = &mut event.payload {
                stop.attribution = Some(stage5cj_attribution_with_cycle(role, cycle));
            }
            assert_eq!(
                resolve_stage5c_paper_broker_lifecycle(
                    resolved,
                    Stage5cPaperBrokerLifecycleInput {
                        event_records: vec![event]
                    },
                )
                .expect_err("SL delete must preserve original attribution")
                .reason(),
                expected
            );
        }
    }

    #[test]
    fn stage5cj_position_flat_preserves_generated_cleanup_intents_with_original_attribution() {
        let (settled, request_id, bar_close_ts) = stage5ci_exit_settled();
        let resolved = resolve_stage5c_paper_intent_lifecycle(
            settled,
            Stage5cPaperIntentLifecycleInput {
                ack_records: vec![stage5ci_ack_record(1, request_id)],
            },
        )
        .unwrap();
        let Stage5cResolvedPaperIntentBatchStrategy {
            mut strategy,
            recovery_receipt,
            resolved_batch,
            ack_outcomes,
            settled_batch_history,
        } = resolved;
        let mut state = Strategy::state(&strategy).clone();
        match &mut state {
            StrategyState::HybridIntradayRuntime {
                active_cycle_id,
                current_owner,
                current_side,
                last_position_qty,
                tp_order_id,
                sl_stop_order_id,
                sl_exchange_order_id,
                ..
            } => {
                *active_cycle_id = Some("abc1230001".to_string());
                *current_owner = Some(crate::hybrid_intraday::Owner::MeanReversion);
                *current_side = Some(crate::hybrid_intraday::Side::Long);
                *last_position_qty = 1.0;
                *tp_order_id = Some(BrokerOrderId::new("TP_ORDER_TEST_0001"));
                *sl_stop_order_id = Some(BrokerStopOrderId::new("STOP_TEST_0001"));
                *sl_exchange_order_id = Some(BrokerOrderId::new("SL_EXCHANGE_TEST_0001"));
            }
            StrategyState::Idle => panic!("expected hybrid runtime state"),
        }
        Strategy::set_state(&mut strategy, state);
        let resolved = Stage5cResolvedPaperIntentBatchStrategy {
            strategy,
            recovery_receipt,
            resolved_batch,
            ack_outcomes,
            settled_batch_history,
        };
        let broker_resolved = resolve_stage5c_paper_broker_lifecycle(
            resolved,
            Stage5cPaperBrokerLifecycleInput {
                event_records: vec![stage5cj_position_event(
                    2,
                    request_id,
                    0.0,
                    bar_close_ts + 2,
                )],
            },
        )
        .unwrap();
        let generated = broker_resolved
            .generated_intent_batch()
            .expect("flat position cleanup must be preserved as no-send generated batch");
        assert_eq!(generated.intent_count(), 3);
        let source_ts = bar_close_ts + 2;
        assert!(generated
            .request_ids()
            .contains(&crate::deterministic_request_id(
                "hybrid_imoexf",
                "ACC_TEST_0001",
                "IMOEXF",
                "cancel:TP_ORDER_TEST_0001",
                source_ts,
                1,
            )));
        assert!(generated
            .request_ids()
            .contains(&crate::deterministic_request_id(
                "hybrid_imoexf",
                "ACC_TEST_0001",
                "IMOEXF",
                "cancel:SL_EXCHANGE_TEST_0001",
                source_ts,
                1,
            )));
        assert!(generated
            .request_ids()
            .contains(&crate::deterministic_request_id(
                "hybrid_imoexf",
                "ACC_TEST_0001",
                "IMOEXF",
                "delete_stop_limit:STOP_TEST_0001",
                source_ts,
                6,
            )));
        assert!(!generated
            .request_ids()
            .contains(&crate::deterministic_request_id(
                "hybrid_imoexf",
                "ACC_TEST_0001",
                "IMOEXF",
                "cancel:TP_ORDER_TEST_0001",
                bar_close_ts,
                1,
            )));
        let roles: Vec<_> = generated
            .records
            .iter()
            .map(|record| {
                record
                    .expected_attribution
                    .as_ref()
                    .and_then(broker_core::HybridRuntimeAttribution::role)
            })
            .collect();
        assert_eq!(
            roles,
            vec![
                Some(broker_core::HybridRuntimeOrderRole::TakeProfit),
                Some(broker_core::HybridRuntimeOrderRole::StopLoss),
                Some(broker_core::HybridRuntimeOrderRole::StopLoss),
            ]
        );
        assert!(generated.records.iter().all(|record| record
            .expected_attribution
            .as_ref()
            .is_some_and(|attr| {
                attr.cycle_id() == "abc1230001"
                    && attr.owner() == Some(broker_core::HybridRuntimeOwner::MeanReversion)
            })));
    }

    #[test]
    fn stage5cj_merged_generated_batch_preserves_per_record_source_ts_and_final_fingerprint() {
        fn run_multi_callback_generated_case() -> (
            Stage5cBrokerLifecycleResolvedPaperStrategy,
            StrategyRequestId,
            StrategyRequestId,
            i64,
            i64,
        ) {
            let (settled, tp_request_id, sl_request_id, bar_close_ts) =
                stage5ci_protective_settled();
            let mut resolved = resolve_stage5c_paper_intent_lifecycle(
                settled,
                Stage5cPaperIntentLifecycleInput {
                    ack_records: vec![
                        Stage5cPaperAckRecord {
                            total_sequence: 1,
                            ack: broker_core::HybridRuntimeCommandAck {
                                broker_order_id: Some(BrokerOrderId::new("TP_ORDER_TEST_0001")),
                                ..stage5ci_ack_with(
                                    tp_request_id,
                                    broker_core::HybridRuntimeAckStatus::Accepted,
                                    bar_close_ts + 1,
                                )
                            },
                        },
                        Stage5cPaperAckRecord {
                            total_sequence: 2,
                            ack: broker_core::HybridRuntimeCommandAck {
                                broker_order_id: Some(BrokerOrderId::new("SL_EXCHANGE_TEST_0001")),
                                ..stage5ci_ack_with(
                                    sl_request_id,
                                    broker_core::HybridRuntimeAckStatus::Accepted,
                                    bar_close_ts + 1,
                                )
                            },
                        },
                    ],
                },
            )
            .unwrap();
            let mut state = Strategy::state(&resolved.strategy).clone();
            match &mut state {
                StrategyState::HybridIntradayRuntime {
                    active_cycle_id,
                    current_owner,
                    current_side,
                    last_position_qty,
                    ..
                } => {
                    *active_cycle_id = Some("abc1230001".to_string());
                    *current_owner = Some(crate::hybrid_intraday::Owner::MeanReversion);
                    *current_side = Some(crate::hybrid_intraday::Side::Long);
                    *last_position_qty = 1.0;
                }
                StrategyState::Idle => panic!("expected hybrid runtime state"),
            }
            Strategy::set_state(&mut resolved.strategy, state);
            let stop_ts = bar_close_ts + 2;
            let flat_ts = bar_close_ts + 5;
            let broker_resolved = resolve_stage5c_paper_broker_lifecycle(
                resolved,
                Stage5cPaperBrokerLifecycleInput {
                    event_records: vec![
                        stage5cj_order_event(
                            3,
                            tp_request_id,
                            BrokerOrderId::new("TP_ORDER_TEST_0001"),
                            "working",
                            bar_close_ts + 1,
                        ),
                        stage5cj_stop_event(
                            4,
                            sl_request_id,
                            BrokerOrderId::new("SL_EXCHANGE_TEST_0001"),
                            "triggered",
                            bar_close_ts + 600,
                            stop_ts,
                        ),
                        stage5cj_position_event(5, sl_request_id, 0.0, flat_ts),
                    ],
                },
            )
            .unwrap();
            (
                broker_resolved,
                tp_request_id,
                sl_request_id,
                stop_ts,
                flat_ts,
            )
        }

        let (broker_resolved, _, _, stop_ts, flat_ts) = run_multi_callback_generated_case();
        let generated = broker_resolved
            .generated_intent_batch()
            .expect("stop trigger and flat position must produce generated cleanup batch");
        assert_eq!(generated.intent_count(), 2);
        assert_eq!(
            generated.state_fingerprint(),
            broker_resolved.post_broker_lifecycle_state_fingerprint()
        );
        let source_ts_by_request: HashMap<_, _> = generated
            .record_source_event_ts_by_request()
            .into_iter()
            .collect();
        let cancel_tp_request_id = crate::deterministic_request_id(
            "hybrid_imoexf",
            "ACC_TEST_0001",
            "IMOEXF",
            "cancel:TP_ORDER_TEST_0001",
            stop_ts,
            1,
        );
        let cancel_sl_request_id = crate::deterministic_request_id(
            "hybrid_imoexf",
            "ACC_TEST_0001",
            "IMOEXF",
            "cancel:SL_EXCHANGE_TEST_0001",
            flat_ts,
            1,
        );
        assert_eq!(
            source_ts_by_request.get(&cancel_tp_request_id),
            Some(&stop_ts)
        );
        assert_eq!(
            source_ts_by_request.get(&cancel_sl_request_id),
            Some(&flat_ts)
        );
        let summary = broker_resolved.generated_intent_batch_summary().unwrap();
        assert_eq!(summary.min_source_event_ts, stop_ts);
        assert_eq!(summary.max_source_event_ts, flat_ts);

        let (mut early_broker_resolved, _, _, _, early_flat_ts) =
            run_multi_callback_generated_case();
        let early_generated_batch = early_broker_resolved
            .generated_intent_batch
            .take()
            .expect("generated batch must exist");
        let early_settled = Stage5cSettledPaperStrategy {
            strategy: early_broker_resolved.strategy,
            recovery_receipt: early_broker_resolved.recovery_receipt,
            batch: early_generated_batch,
            settled_batch_history: early_broker_resolved.settled_batch_history,
        };
        assert_eq!(
            resolve_stage5c_paper_intent_lifecycle(
                early_settled,
                Stage5cPaperIntentLifecycleInput {
                    ack_records: vec![
                        Stage5cPaperAckRecord {
                            total_sequence: 6,
                            ack: stage5ci_ack_with(
                                cancel_tp_request_id,
                                broker_core::HybridRuntimeAckStatus::Accepted,
                                stop_ts,
                            ),
                        },
                        Stage5cPaperAckRecord {
                            total_sequence: 7,
                            ack: stage5ci_ack_with(
                                cancel_sl_request_id,
                                broker_core::HybridRuntimeAckStatus::Accepted,
                                early_flat_ts - 1,
                            ),
                        },
                    ],
                },
            )
            .expect_err("second generated ACK before its own source timestamp must block")
            .reason(),
            Stage5cPaperIntentLifecycleError::AckTimestampBeforeIntentBar
        );

        let (mut ok_broker_resolved, _, _, _, ok_flat_ts) = run_multi_callback_generated_case();
        let ok_generated_batch = ok_broker_resolved
            .generated_intent_batch
            .take()
            .expect("generated batch must exist");
        let ok_settled = Stage5cSettledPaperStrategy {
            strategy: ok_broker_resolved.strategy,
            recovery_receipt: ok_broker_resolved.recovery_receipt,
            batch: ok_generated_batch,
            settled_batch_history: ok_broker_resolved.settled_batch_history,
        };
        let generated_ack_resolved = resolve_stage5c_paper_intent_lifecycle(
            ok_settled,
            Stage5cPaperIntentLifecycleInput {
                ack_records: vec![
                    Stage5cPaperAckRecord {
                        total_sequence: 6,
                        ack: stage5ci_ack_with(
                            cancel_tp_request_id,
                            broker_core::HybridRuntimeAckStatus::Accepted,
                            stop_ts,
                        ),
                    },
                    Stage5cPaperAckRecord {
                        total_sequence: 7,
                        ack: stage5ci_ack_with(
                            cancel_sl_request_id,
                            broker_core::HybridRuntimeAckStatus::Accepted,
                            ok_flat_ts,
                        ),
                    },
                ],
            },
        )
        .expect("generated ACK lifecycle must use final fingerprint and per-record source ts");
        assert_eq!(generated_ack_resolved.ack_outcomes().len(), 2);
    }

    #[test]
    fn stage5cj_generated_executable_intents_require_final_pending_state() {
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
        let (strategy, receipt) = recovered.into_parts();
        let admission = &receipt
            .warmup_receipt()
            .restore_receipt()
            .bootstrap_receipt()
            .admission;
        let source_ts = now.timestamp() + 600;
        let exit_request_id = crate::deterministic_request_id(
            "hybrid_imoexf",
            "ACC_TEST_0001",
            "IMOEXF",
            "market",
            source_ts,
            4,
        );
        let cleanup_request_id = crate::deterministic_request_id(
            "hybrid_imoexf",
            "ACC_TEST_0001",
            "IMOEXF",
            "cancel:TP_ORDER_TEST_0001",
            source_ts,
            1,
        );
        let exit_batch = Stage5cPaperIntentBatch {
            strategy_id: admission.strategy_id().to_string(),
            account_id: admission.account_id().clone(),
            instrument: admission.target_instrument().clone(),
            bar_close_ts: source_ts,
            state_fingerprint: stage5c_state_fingerprint(Strategy::state(&strategy)),
            request_ids: vec![exit_request_id],
            records: vec![Stage5cPaperIntentRecord {
                request_id: exit_request_id,
                source_event_ts: source_ts,
                intent_class: crate::BrokerNeutralHybridIntentClass::Exit,
                intent: stage5cg_market_intent(
                    crate::BrokerNeutralOrderSide::Sell,
                    crate::BrokerNeutralHybridIntentClass::Exit,
                ),
                expected_attribution: None,
            }],
            observation_only: false,
        };
        assert_eq!(
            stage5cj_verify_generated_batch_final_pending_consistency(
                Strategy::state(&strategy),
                &exit_batch,
            )
            .expect_err("executable generated exit without final pending state must block"),
            Stage5cIntentSettlementError::MissingPendingRequest
        );

        let cleanup_batch = Stage5cPaperIntentBatch {
            strategy_id: admission.strategy_id().to_string(),
            account_id: admission.account_id().clone(),
            instrument: admission.target_instrument().clone(),
            bar_close_ts: source_ts,
            state_fingerprint: stage5c_state_fingerprint(Strategy::state(&strategy)),
            request_ids: vec![cleanup_request_id],
            records: vec![Stage5cPaperIntentRecord {
                request_id: cleanup_request_id,
                source_event_ts: source_ts,
                intent_class: crate::BrokerNeutralHybridIntentClass::CancelCleanup,
                intent: crate::BrokerNeutralHybridIntent::Cancel {
                    order_id: BrokerOrderId::new("TP_ORDER_TEST_0001"),
                }
                .with_class(crate::BrokerNeutralHybridIntentClass::CancelCleanup)
                .with_symbol("IMOEXF"),
                expected_attribution: None,
            }],
            observation_only: false,
        };
        stage5cj_verify_generated_batch_final_pending_consistency(
            Strategy::state(&strategy),
            &cleanup_batch,
        )
        .expect("cleanup generated intents do not require final pending state");
    }

    fn stage5ck_clean_broker_resolved_at(
        ack_ts_utc: i64,
        position_ts_utc: i64,
    ) -> (Stage5cBrokerLifecycleResolvedPaperStrategy, i64) {
        let (settled, request_id, bar_close_ts) = stage5ci_exit_settled();
        let resolved = resolve_stage5c_paper_intent_lifecycle(
            settled,
            Stage5cPaperIntentLifecycleInput {
                ack_records: vec![Stage5cPaperAckRecord {
                    total_sequence: 1,
                    ack: stage5ci_ack_with(
                        request_id,
                        broker_core::HybridRuntimeAckStatus::Accepted,
                        ack_ts_utc,
                    ),
                }],
            },
        )
        .unwrap();
        let broker_resolved = resolve_stage5c_paper_broker_lifecycle(
            resolved,
            Stage5cPaperBrokerLifecycleInput {
                event_records: vec![stage5cj_position_event(2, request_id, 0.0, position_ts_utc)],
            },
        )
        .unwrap();
        (broker_resolved, bar_close_ts)
    }

    fn stage5ck_clean_broker_resolved() -> (Stage5cBrokerLifecycleResolvedPaperStrategy, i64) {
        let (settled, request_id, bar_close_ts) = stage5ci_exit_settled();
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
                    0.0,
                    bar_close_ts + 2,
                )],
            },
        )
        .unwrap();
        (broker_resolved, bar_close_ts)
    }

    #[test]
    fn stage5ck_zero_intent_timer_invokes_only_paper_timer_callback() {
        let (broker_resolved, bar_close_ts) = stage5ck_clean_broker_resolved();
        let timer = resolve_stage5c_paper_timer(
            broker_resolved,
            Stage5cPaperTimerInput {
                now_ts_utc_ms: (bar_close_ts + 10) * 1_000,
            },
        )
        .unwrap();
        assert_eq!(timer.generated_intent_count(), 0);
        assert!(!timer.intent_sink_attached());
        assert!(!timer.broker_transport_attached());
        assert!(!timer.redis_command_stream_attached());
    }

    #[test]
    fn stage5ck_timer_clock_is_bound_to_lifecycle_watermark() {
        let (broker_resolved, bar_close_ts) = stage5ck_clean_broker_resolved();
        assert_eq!(
            broker_resolved.lifecycle_watermark_ts_utc(),
            bar_close_ts + 2
        );
        let blocked = resolve_stage5c_paper_timer(
            broker_resolved,
            Stage5cPaperTimerInput {
                now_ts_utc_ms: (bar_close_ts + 1) * 1_000,
            },
        )
        .expect_err("timer before latest PositionEvent must be blocked")
        .into_blocked()
        .expect("non-monotonic timer preserves broker-resolved type-state");
        assert_eq!(blocked.reason(), Stage5cPaperTimerError::NonMonotonicTimer);
        let broker_resolved = blocked.into_resolved();
        assert_eq!(
            broker_resolved.lifecycle_watermark_ts_utc(),
            bar_close_ts + 2
        );

        let timer = resolve_stage5c_paper_timer(
            broker_resolved,
            Stage5cPaperTimerInput {
                now_ts_utc_ms: (bar_close_ts + 2) * 1_000,
            },
        )
        .expect("timer exactly at lifecycle watermark is allowed");
        assert_eq!(timer.generated_intent_count(), 0);

        let (broker_resolved, bar_close_ts) =
            stage5ck_clean_broker_resolved_at(bar_close_ts + 5, bar_close_ts + 5);
        assert_eq!(
            broker_resolved.lifecycle_watermark_ts_utc(),
            bar_close_ts + 5
        );
        assert_eq!(
            resolve_stage5c_paper_timer(
                broker_resolved,
                Stage5cPaperTimerInput {
                    now_ts_utc_ms: (bar_close_ts + 4) * 1_000,
                },
            )
            .expect_err("timer before latest ACK/event timestamp must be blocked")
            .reason(),
            Stage5cPaperTimerError::NonMonotonicTimer
        );

        let (broker_resolved, bar_close_ts) = stage5ck_clean_broker_resolved();
        let timer = resolve_stage5c_paper_timer(
            broker_resolved,
            Stage5cPaperTimerInput {
                now_ts_utc_ms: (bar_close_ts + 10) * 1_000,
            },
        )
        .expect("timer later than lifecycle watermark is allowed");
        assert_eq!(timer.generated_intent_count(), 0);
    }

    #[test]
    fn stage5ck_blocks_unresolved_broker_lifecycle_and_generated_batch() {
        let (settled, tp_request_id, sl_request_id, bar_close_ts) = stage5ci_protective_settled();
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
                            sl_request_id,
                            broker_core::HybridRuntimeAckStatus::Accepted,
                            bar_close_ts + 1,
                        ),
                    },
                ],
            },
        )
        .unwrap();
        let broker_resolved = resolve_stage5c_paper_broker_lifecycle(
            resolved,
            Stage5cPaperBrokerLifecycleInput {
                event_records: vec![
                    stage5cj_order_event(
                        3,
                        tp_request_id,
                        BrokerOrderId::new("ORDER_TEST_ACK_0001"),
                        "working",
                        bar_close_ts + 2,
                    ),
                    stage5cj_stop_event(
                        4,
                        sl_request_id,
                        BrokerOrderId::new("ORDER_TEST_ACK_0001"),
                        "working",
                        bar_close_ts + 600,
                        bar_close_ts + 2,
                    ),
                ],
            },
        )
        .unwrap();
        assert_eq!(
            resolve_stage5c_paper_timer(
                broker_resolved,
                Stage5cPaperTimerInput {
                    now_ts_utc_ms: (bar_close_ts + 10) * 1_000,
                },
            )
            .expect_err("timer must wait for complete broker lifecycle")
            .reason(),
            Stage5cPaperTimerError::UnresolvedBrokerLifecycle
        );

        let (settled, request_id, bar_close_ts) = stage5ci_exit_settled();
        let resolved = resolve_stage5c_paper_intent_lifecycle(
            settled,
            Stage5cPaperIntentLifecycleInput {
                ack_records: vec![stage5ci_ack_record(1, request_id)],
            },
        )
        .unwrap();
        let Stage5cResolvedPaperIntentBatchStrategy {
            mut strategy,
            recovery_receipt,
            resolved_batch,
            ack_outcomes,
            settled_batch_history,
        } = resolved;
        let mut state = Strategy::state(&strategy).clone();
        match &mut state {
            StrategyState::HybridIntradayRuntime {
                active_cycle_id,
                current_owner,
                current_side,
                last_position_qty,
                tp_order_id,
                ..
            } => {
                *active_cycle_id = Some("abc1230001".to_string());
                *current_owner = Some(crate::hybrid_intraday::Owner::MeanReversion);
                *current_side = Some(crate::hybrid_intraday::Side::Long);
                *last_position_qty = 1.0;
                *tp_order_id = Some(BrokerOrderId::new("TP_ORDER_TEST_0001"));
            }
            StrategyState::Idle => panic!("expected hybrid runtime state"),
        }
        Strategy::set_state(&mut strategy, state);
        let resolved = Stage5cResolvedPaperIntentBatchStrategy {
            strategy,
            recovery_receipt,
            resolved_batch,
            ack_outcomes,
            settled_batch_history,
        };
        let broker_resolved = resolve_stage5c_paper_broker_lifecycle(
            resolved,
            Stage5cPaperBrokerLifecycleInput {
                event_records: vec![stage5cj_position_event(
                    2,
                    request_id,
                    0.0,
                    bar_close_ts + 2,
                )],
            },
        )
        .unwrap();
        assert!(broker_resolved.generated_intent_count() > 0);
        assert_eq!(
            resolve_stage5c_paper_timer(
                broker_resolved,
                Stage5cPaperTimerInput {
                    now_ts_utc_ms: (bar_close_ts + 10) * 1_000,
                },
            )
            .expect_err("timer must wait for generated batch lifecycle")
            .reason(),
            Stage5cPaperTimerError::UnresolvedGeneratedIntentBatch
        );
    }

    #[test]
    fn stage5cj_semantic_bar_cleanup_attribution_is_captured_before_wrapper_take() {
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
        let (mut strategy, receipt) = recovered.into_parts();
        let close_ts = Utc
            .with_ymd_and_hms(2026, 7, 13, 9, 10, 0)
            .single()
            .unwrap()
            .timestamp();
        let mut state = Strategy::state(&strategy).clone();
        match &mut state {
            StrategyState::HybridIntradayRuntime {
                active_cycle_id,
                current_owner,
                current_side,
                last_position_qty,
                tp_order_id,
                sl_stop_order_id,
                sl_exchange_order_id,
                sl_triggered_ts,
                mr_take_price,
                mr_stop_price,
                repair_deadline_ts,
                ..
            } => {
                *active_cycle_id = Some("abc1230001".to_string());
                *current_owner = Some(crate::hybrid_intraday::Owner::MeanReversion);
                *current_side = Some(crate::hybrid_intraday::Side::Long);
                *last_position_qty = 1.0;
                *tp_order_id = Some(BrokerOrderId::new("TP_ORDER_TEST_0001"));
                *sl_stop_order_id = Some(BrokerStopOrderId::new("STOP_TEST_0001"));
                *sl_exchange_order_id = Some(BrokerOrderId::new("SL_EXCHANGE_TEST_0001"));
                *sl_triggered_ts = Some(close_ts - 31);
                *mr_take_price = Some(2235.0);
                *mr_stop_price = Some(2210.0);
                *repair_deadline_ts = Some(close_ts - 1);
            }
            StrategyState::Idle => panic!("expected hybrid runtime state"),
        }
        Strategy::set_state(&mut strategy, state);
        let recovered = Stage5cPendingRecoveredPaperStrategy { strategy, receipt };
        let accepted = accept_stage5c_semantic_bar(semantic_input(close_ts)).unwrap();
        let semantic = apply_stage5c_semantic_bar_at(
            recovered,
            accepted,
            Utc.with_ymd_and_hms(2026, 7, 13, 9, 10, 30)
                .single()
                .unwrap(),
        )
        .unwrap();
        let settled = settle_stage5c_semantic_result(semantic).unwrap();
        let cleanup_records: Vec<_> = settled
            .intent_batch()
            .records
            .iter()
            .filter(|record| {
                record.intent_class == crate::BrokerNeutralHybridIntentClass::CancelCleanup
            })
            .collect();
        assert_eq!(cleanup_records.len(), 2);
        assert!(cleanup_records.iter().all(|record| record
            .expected_attribution
            .as_ref()
            .is_some_and(|attr| attr.cycle_id() == "abc1230001")));
        assert!(cleanup_records.iter().any(|record| record
            .expected_attribution
            .as_ref()
            .and_then(broker_core::HybridRuntimeAttribution::role)
            == Some(broker_core::HybridRuntimeOrderRole::TakeProfit)));
        assert_eq!(
            cleanup_records
                .iter()
                .filter(|record| {
                    record
                        .expected_attribution
                        .as_ref()
                        .and_then(broker_core::HybridRuntimeAttribution::role)
                        == Some(broker_core::HybridRuntimeOrderRole::StopLoss)
                })
                .count(),
            1
        );
    }

    #[test]
    fn stage5ck_partial_entry_cleanup_uses_pending_entry_attribution() {
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
        let (mut strategy, receipt) = recovered.into_parts();
        let strategy_id = receipt
            .warmup_receipt()
            .restore_receipt()
            .bootstrap_receipt()
            .admission
            .strategy_id()
            .to_string();
        let mut state = Strategy::state(&strategy).clone();
        match &mut state {
            StrategyState::HybridIntradayRuntime {
                pending_entry_owner,
                pending_entry_side,
                pending_entry_cycle_id,
                pending_entry_request_id,
                ..
            } => {
                *pending_entry_owner = Some(crate::hybrid_intraday::Owner::MeanReversion);
                *pending_entry_side = Some(crate::hybrid_intraday::Side::Long);
                *pending_entry_cycle_id = Some("abc1230001".to_string());
                *pending_entry_request_id =
                    Some(StrategyRequestId::from(uuid::Uuid::from_u128(0x5c0ffee)));
            }
            StrategyState::Idle => panic!("expected hybrid runtime state"),
        }
        Strategy::set_state(&mut strategy, state);

        let ledger = stage5cj_cleanup_attribution_ledger(Strategy::state(&strategy), &strategy_id);
        let cancel_entry = crate::BrokerNeutralHybridIntent::Cancel {
            order_id: BrokerOrderId::new("ENTRY_WORKING_ORDER_TEST_0001"),
        }
        .with_class(crate::BrokerNeutralHybridIntentClass::CancelCleanup);
        let attribution = stage5cj_expected_cleanup_attribution_from_ledger(&ledger, &cancel_entry)
            .expect("pending-entry cleanup cancel receives exact ENTRY attribution");
        assert_eq!(attribution.strategy_id(), strategy_id);
        assert_eq!(attribution.cycle_id(), "abc1230001");
        assert_eq!(
            attribution.role(),
            Some(broker_core::HybridRuntimeOrderRole::Entry)
        );
    }

    #[test]
    fn stage5cl_zero_timer_settlement_allows_controlled_continuation() {
        let (broker_resolved, bar_close_ts) = stage5ck_clean_broker_resolved();
        let timer_ts_utc = bar_close_ts + 10;
        let timer = resolve_stage5c_paper_timer(
            broker_resolved,
            Stage5cPaperTimerInput {
                now_ts_utc_ms: timer_ts_utc * 1_000,
            },
        )
        .unwrap();
        let settlement = settle_stage5c_timer_result(timer);
        assert!(settlement.is_ready_for_continuation());
        assert!(!settlement.intent_sink_attached());
        assert!(!settlement.broker_transport_attached());
        assert!(!settlement.redis_command_stream_attached());
        assert_eq!(settlement.settled().intent_batch().intent_count(), 0);
        assert_eq!(
            settlement.settled().intent_batch().bar_close_ts(),
            timer_ts_utc
        );
        assert_eq!(settlement.settled().settled_batch_history().len(), 2);

        let next_close_ts = bar_close_ts + 600;
        let accepted = accept_stage5c_semantic_bar(semantic_input(next_close_ts)).unwrap();
        let advanced = advance_stage5c_timer_settlement_next_bar_at(
            settlement,
            accepted,
            Utc.timestamp_opt(next_close_ts + 30, 0).single().unwrap(),
        )
        .expect("zero timer settlement is a controlled continuation checkpoint");
        assert_eq!(advanced.intent_batch().bar_close_ts(), next_close_ts);
        assert_eq!(advanced.settled_batch_history().len(), 3);
    }

    #[test]
    fn stage5cl_nonzero_timer_batch_must_reenter_ack_lifecycle() {
        let (settled, _, bar_close_ts) = stage5ci_exit_settled();
        let Stage5cSettledPaperStrategy {
            strategy,
            recovery_receipt,
            batch,
            settled_batch_history,
        } = settled;
        let timer = Stage5cTimerResolvedPaperStrategy {
            strategy,
            recovery_receipt,
            resolved_batch_summary: stage5ch_batch_summary(&batch),
            timer_ts_utc_ms: (bar_close_ts + 10) * 1_000,
            generated_intent_batch: Some(batch),
            settled_batch_history,
        };

        let settlement = settle_stage5c_timer_result(timer);
        assert!(settlement.is_generated_intent_batch());
        assert_eq!(settlement.settled().intent_batch().intent_count(), 1);
        let generated_settled = settlement
            .into_generated_intent_batch()
            .expect("generated timer settlement exposes only the generated batch");
        assert_eq!(
            advance_stage5c_controlled_next_bar_at(
                generated_settled,
                accept_stage5c_semantic_bar(semantic_input(bar_close_ts + 600)).unwrap(),
                Utc.timestamp_opt(bar_close_ts + 630, 0).single().unwrap(),
            )
            .expect_err("nonzero timer batch must not skip ACK lifecycle")
            .reason(),
            Stage5cNextBarLoopError::UnresolvedIntentBatch
        );

        let (settled, request_id, bar_close_ts) = stage5ci_exit_settled();
        let Stage5cSettledPaperStrategy {
            strategy,
            recovery_receipt,
            batch,
            settled_batch_history,
        } = settled;
        let timer = Stage5cTimerResolvedPaperStrategy {
            strategy,
            recovery_receipt,
            resolved_batch_summary: stage5ch_batch_summary(&batch),
            timer_ts_utc_ms: (bar_close_ts + 10) * 1_000,
            generated_intent_batch: Some(batch),
            settled_batch_history,
        };
        let generated_settled = settle_stage5c_timer_result(timer)
            .into_generated_intent_batch()
            .expect("timer-generated batch reuses the Stage 5C-i ACK lifecycle");
        let resolved = resolve_stage5c_paper_intent_lifecycle(
            generated_settled,
            Stage5cPaperIntentLifecycleInput {
                ack_records: vec![stage5ci_ack_record(1, request_id)],
            },
        )
        .expect("timer-generated batch reuses the Stage 5C-i ACK lifecycle");
        assert_eq!(resolved.resolved_batch_summary().intent_count, 1);
    }

    #[test]
    fn stage5cm_ready_checkpoint_can_continue_to_timer_or_bar_once() {
        let (broker_resolved, bar_close_ts) = stage5ck_clean_broker_resolved();
        let timer_ts_utc = bar_close_ts + 10;
        let first_timer = resolve_stage5c_paper_timer(
            broker_resolved,
            Stage5cPaperTimerInput {
                now_ts_utc_ms: timer_ts_utc * 1_000,
            },
        )
        .unwrap();
        let ready = settle_stage5c_timer_result(first_timer);
        let second_timer = advance_stage5c_timer_settlement_timer(
            ready,
            Stage5cPaperTimerInput {
                now_ts_utc_ms: (timer_ts_utc + 1) * 1_000,
            },
        )
        .expect("ready timer checkpoint may advance to one later timer");
        assert_eq!(second_timer.generated_intent_count(), 0);

        let (broker_resolved, bar_close_ts) = stage5ck_clean_broker_resolved();
        let timer_ts_utc = bar_close_ts + 10;
        let first_timer = resolve_stage5c_paper_timer(
            broker_resolved,
            Stage5cPaperTimerInput {
                now_ts_utc_ms: timer_ts_utc * 1_000,
            },
        )
        .unwrap();
        let ready = settle_stage5c_timer_result(first_timer);
        let next_close_ts = bar_close_ts + 600;
        let advanced = advance_stage5c_timer_settlement_next_bar_at(
            ready,
            accept_stage5c_semantic_bar(semantic_input(next_close_ts)).unwrap(),
            Utc.timestamp_opt(next_close_ts + 30, 0).single().unwrap(),
        )
        .expect("ready timer checkpoint may advance to one later bar");
        assert_eq!(advanced.intent_batch().bar_close_ts(), next_close_ts);
    }

    fn stage5cm_ready_subsecond_checkpoint() -> (Stage5cTimerSettlement, i64, i64) {
        let (broker_resolved, bar_close_ts) = stage5ck_clean_broker_resolved();
        let checkpoint_ts_utc_ms = (bar_close_ts + 10) * 1_000 + 900;
        let timer = resolve_stage5c_paper_timer(
            broker_resolved,
            Stage5cPaperTimerInput {
                now_ts_utc_ms: checkpoint_ts_utc_ms,
            },
        )
        .unwrap();
        let settlement = settle_stage5c_timer_result(timer);
        assert!(settlement.is_ready_for_continuation());
        assert_eq!(
            settlement.checkpoint_ts_utc_ms(),
            Some(checkpoint_ts_utc_ms)
        );
        (settlement, bar_close_ts, checkpoint_ts_utc_ms)
    }

    #[test]
    fn stage5cm_timer_before_exact_millisecond_checkpoint_is_blocked() {
        let (settlement, _, checkpoint_ts_utc_ms) = stage5cm_ready_subsecond_checkpoint();
        let blocked = advance_stage5c_timer_settlement_timer(
            settlement,
            Stage5cPaperTimerInput {
                now_ts_utc_ms: checkpoint_ts_utc_ms - 400,
            },
        )
        .expect_err("timer before exact millisecond checkpoint must be blocked");
        assert_eq!(
            blocked.reason(),
            Stage5cTimerContinuationError::NonMonotonicTimer
        );
        assert_eq!(
            blocked
                .into_blocked()
                .unwrap()
                .settlement()
                .checkpoint_ts_utc_ms(),
            Some(checkpoint_ts_utc_ms)
        );
    }

    #[test]
    fn stage5cm_timer_equal_to_exact_checkpoint_is_blocked() {
        let (settlement, _, checkpoint_ts_utc_ms) = stage5cm_ready_subsecond_checkpoint();
        let blocked = advance_stage5c_timer_settlement_timer(
            settlement,
            Stage5cPaperTimerInput {
                now_ts_utc_ms: checkpoint_ts_utc_ms,
            },
        )
        .expect_err("timer equal to exact millisecond checkpoint must be blocked");
        assert_eq!(
            blocked.reason(),
            Stage5cTimerContinuationError::NonMonotonicTimer
        );
    }

    #[test]
    fn stage5cm_timer_one_millisecond_after_checkpoint_is_accepted() {
        let (settlement, _, checkpoint_ts_utc_ms) = stage5cm_ready_subsecond_checkpoint();
        let advanced = advance_stage5c_timer_settlement_timer(
            settlement,
            Stage5cPaperTimerInput {
                now_ts_utc_ms: checkpoint_ts_utc_ms + 1,
            },
        )
        .expect("timer one millisecond after exact checkpoint is monotonic");
        assert_eq!(advanced.generated_intent_count(), 0);
        assert_eq!(advanced.timer_ts_utc_ms(), checkpoint_ts_utc_ms + 1);
    }

    #[test]
    fn stage5cm_blocked_subsecond_timer_preserves_settlement() {
        let (settlement, _, checkpoint_ts_utc_ms) = stage5cm_ready_subsecond_checkpoint();
        let blocked = advance_stage5c_timer_settlement_timer(
            settlement,
            Stage5cPaperTimerInput {
                now_ts_utc_ms: checkpoint_ts_utc_ms - 400,
            },
        )
        .expect_err("subsecond nonmonotonic timer is recoverable");
        let blocked = blocked.into_blocked().unwrap();
        assert!(blocked.settlement().is_ready_for_continuation());
        assert_eq!(
            blocked.settlement().checkpoint_ts_utc_ms(),
            Some(checkpoint_ts_utc_ms)
        );
        assert_eq!(
            blocked.settlement().settled().intent_batch().intent_count(),
            0
        );
    }

    #[test]
    fn stage5cm_nonmonotonic_next_bar_preserves_ready_settlement() {
        let (settlement, bar_close_ts, checkpoint_ts_utc_ms) =
            stage5cm_ready_subsecond_checkpoint();
        let previous_fingerprint = settlement
            .settled()
            .intent_batch()
            .state_fingerprint()
            .to_string();
        let blocked = advance_stage5c_timer_settlement_next_bar_at(
            settlement,
            accept_stage5c_semantic_bar(semantic_input(bar_close_ts)).unwrap(),
            Utc.timestamp_opt(bar_close_ts + 30, 0).single().unwrap(),
        )
        .expect_err("nonmonotonic next bar must preserve ready settlement");
        assert_eq!(
            blocked.reason(),
            Stage5cTimerContinuationError::NextBar(Stage5cNextBarLoopError::NonMonotonicBar)
        );
        let blocked = blocked.into_blocked().unwrap();
        assert!(blocked.settlement().is_ready_for_continuation());
        assert_eq!(
            blocked.settlement().checkpoint_ts_utc_ms(),
            Some(checkpoint_ts_utc_ms)
        );
        assert_eq!(
            blocked
                .settlement()
                .settled()
                .intent_batch()
                .state_fingerprint(),
            previous_fingerprint
        );
    }

    #[test]
    fn stage5cm_expired_next_bar_preserves_ready_settlement() {
        let (settlement, bar_close_ts, checkpoint_ts_utc_ms) =
            stage5cm_ready_subsecond_checkpoint();
        let expires_at = settlement
            .settled()
            .recovery_receipt()
            .warmup_receipt()
            .restore_receipt()
            .bootstrap_receipt()
            .expires_at();
        let blocked = advance_stage5c_timer_settlement_next_bar_at(
            settlement,
            accept_stage5c_semantic_bar(semantic_input(bar_close_ts + 600)).unwrap(),
            expires_at + chrono::Duration::milliseconds(1),
        )
        .expect_err("expired next bar preflight must preserve ready settlement");
        assert_eq!(
            blocked.reason(),
            Stage5cTimerContinuationError::NextBar(Stage5cNextBarLoopError::Semantic(
                Stage5cSemanticBarError::BrokerTruthExpired,
            ))
        );
        let blocked = blocked.into_blocked().unwrap();
        assert!(blocked.settlement().is_ready_for_continuation());
        assert_eq!(
            blocked.settlement().checkpoint_ts_utc_ms(),
            Some(checkpoint_ts_utc_ms)
        );
    }

    #[test]
    fn stage5cm_blocked_next_bar_does_not_invoke_callback() {
        let (settlement, bar_close_ts, _) = stage5cm_ready_subsecond_checkpoint();
        let previous_fingerprint = settlement
            .settled()
            .intent_batch()
            .state_fingerprint()
            .to_string();
        let blocked = advance_stage5c_timer_settlement_next_bar_at(
            settlement,
            accept_stage5c_semantic_bar(semantic_input(bar_close_ts)).unwrap(),
            Utc.timestamp_opt(bar_close_ts + 30, 0).single().unwrap(),
        )
        .expect_err("blocked next bar should stop before semantic callback");
        let blocked_settlement = blocked.into_blocked().unwrap().into_settlement();
        assert_eq!(
            blocked_settlement
                .settled()
                .intent_batch()
                .state_fingerprint(),
            previous_fingerprint
        );
        assert_eq!(
            blocked_settlement.settled().intent_batch().intent_count(),
            0
        );
    }

    #[test]
    fn stage5cm_blocked_next_bar_allows_later_timer_retry() {
        let (settlement, bar_close_ts, checkpoint_ts_utc_ms) =
            stage5cm_ready_subsecond_checkpoint();
        let blocked = advance_stage5c_timer_settlement_next_bar_at(
            settlement,
            accept_stage5c_semantic_bar(semantic_input(bar_close_ts)).unwrap(),
            Utc.timestamp_opt(bar_close_ts + 30, 0).single().unwrap(),
        )
        .expect_err("recoverable next-bar block should return settlement for retry");
        let retry_settlement = blocked.into_blocked().unwrap().into_settlement();
        let retry = advance_stage5c_timer_settlement_timer(
            retry_settlement,
            Stage5cPaperTimerInput {
                now_ts_utc_ms: checkpoint_ts_utc_ms + 1,
            },
        )
        .expect("ready settlement returned from blocked next-bar may continue via timer");
        assert_eq!(retry.generated_intent_count(), 0);
        assert_eq!(retry.timer_ts_utc_ms(), checkpoint_ts_utc_ms + 1);
    }

    #[test]
    fn stage5cm_generated_timer_batch_blocks_continuation_until_lifecycle() {
        let (settled, _, bar_close_ts) = stage5ci_exit_settled();
        let Stage5cSettledPaperStrategy {
            strategy,
            recovery_receipt,
            batch,
            settled_batch_history,
        } = settled;
        let timer = Stage5cTimerResolvedPaperStrategy {
            strategy,
            recovery_receipt,
            resolved_batch_summary: stage5ch_batch_summary(&batch),
            timer_ts_utc_ms: (bar_close_ts + 10) * 1_000,
            generated_intent_batch: Some(batch),
            settled_batch_history,
        };
        let generated = settle_stage5c_timer_result(timer);
        assert_eq!(
            advance_stage5c_timer_settlement_next_bar(
                generated,
                accept_stage5c_semantic_bar(semantic_input(bar_close_ts + 600)).unwrap(),
            )
            .expect_err("generated timer batch cannot advance directly to next bar")
            .reason(),
            Stage5cTimerContinuationError::GeneratedIntentBatchRequiresLifecycle
        );

        let (settled, _, bar_close_ts) = stage5ci_exit_settled();
        let Stage5cSettledPaperStrategy {
            strategy,
            recovery_receipt,
            batch,
            settled_batch_history,
        } = settled;
        let timer = Stage5cTimerResolvedPaperStrategy {
            strategy,
            recovery_receipt,
            resolved_batch_summary: stage5ch_batch_summary(&batch),
            timer_ts_utc_ms: (bar_close_ts + 10) * 1_000,
            generated_intent_batch: Some(batch),
            settled_batch_history,
        };
        let generated = settle_stage5c_timer_result(timer);
        let blocked = advance_stage5c_timer_settlement_timer(
            generated,
            Stage5cPaperTimerInput {
                now_ts_utc_ms: (bar_close_ts + 11) * 1_000,
            },
        )
        .expect_err("generated timer batch cannot advance directly to another timer");
        assert_eq!(
            blocked.reason(),
            Stage5cTimerContinuationError::GeneratedIntentBatchRequiresLifecycle
        );
        assert!(blocked
            .into_blocked()
            .expect("blocked generated batch preserves settlement")
            .settlement()
            .is_generated_intent_batch());
    }

    #[test]
    fn stage5cn_settle_is_bounded_no_send_step() {
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
        let semantic_state =
            Stage5cPaperLoopState::SemanticResult(Box::new(Stage5cSemanticBarResult {
                strategy,
                recovery_receipt,
                bar_close_ts: first_close_ts,
                origin: broker_core::HybridRuntimeBarOrigin::Live,
                execution_eligible: true,
                intents: Vec::new(),
                expected_attribution_by_request: HashMap::new(),
            }));

        let settled_state = advance_stage5c_paper_loop_once(
            semantic_state,
            Stage5cPaperLoopEvent::SettleSemanticResult,
        )
        .expect("bounded loop settles the captured semantic result explicitly");
        assert_eq!(settled_state.kind(), Stage5cPaperLoopStateKind::Settled);
        assert!(!settled_state.intent_sink_attached());
        assert!(!settled_state.broker_transport_attached());
        assert!(!settled_state.redis_command_stream_attached());
    }

    #[test]
    fn stage5cn_invalid_transition_preserves_input_state() {
        let recovered = empty_recovered(Utc::now());
        let failure = advance_stage5c_paper_loop_once(
            Stage5cPaperLoopState::PendingRecovered(Box::new(recovered)),
            Stage5cPaperLoopEvent::Timer(Stage5cPaperTimerInput {
                now_ts_utc_ms: Utc::now().timestamp_millis(),
            }),
        )
        .expect_err("timer is not a valid first paper-loop event");
        assert_eq!(
            failure.reason(),
            Stage5cPaperLoopError::InvalidTransition {
                state: Stage5cPaperLoopStateKind::PendingRecovered,
                event: Stage5cPaperLoopEventKind::Timer,
            }
        );
        assert_eq!(
            failure.preserved_state().map(Stage5cPaperLoopState::kind),
            Some(Stage5cPaperLoopStateKind::PendingRecovered)
        );
    }

    #[test]
    fn stage5cn_generated_timer_batch_can_reenter_ack_lifecycle() {
        let (settled, request_id, bar_close_ts) = stage5ci_exit_settled();
        let Stage5cSettledPaperStrategy {
            strategy,
            recovery_receipt,
            batch,
            settled_batch_history,
        } = settled;
        let timer = Stage5cTimerResolvedPaperStrategy {
            strategy,
            recovery_receipt,
            resolved_batch_summary: stage5ch_batch_summary(&batch),
            timer_ts_utc_ms: (bar_close_ts + 10) * 1_000,
            generated_intent_batch: Some(batch),
            settled_batch_history,
        };
        let timer_settlement = settle_stage5c_timer_result(timer);
        let resolved = advance_stage5c_paper_loop_once(
            Stage5cPaperLoopState::TimerSettlement(Box::new(timer_settlement)),
            Stage5cPaperLoopEvent::Ack(Box::new(Stage5cPaperIntentLifecycleInput {
                ack_records: vec![stage5ci_ack_record(1, request_id)],
            })),
        )
        .expect("generated timer batch reenters ACK lifecycle through coordinator");
        assert_eq!(
            resolved.kind(),
            Stage5cPaperLoopStateKind::IntentLifecycleResolved
        );
    }

    #[test]
    fn stage5cn_ready_timer_settlement_rejects_ack_without_revealing_ready_state() {
        let (settlement, _, _) = stage5cm_ready_subsecond_checkpoint();
        let failure = advance_stage5c_paper_loop_once(
            Stage5cPaperLoopState::TimerSettlement(Box::new(settlement)),
            Stage5cPaperLoopEvent::Ack(Box::new(Stage5cPaperIntentLifecycleInput {
                ack_records: Vec::new(),
            })),
        )
        .expect_err("ready timer settlement is not a generated batch");
        assert_eq!(
            failure.reason(),
            Stage5cPaperLoopError::InvalidTransition {
                state: Stage5cPaperLoopStateKind::TimerSettlement,
                event: Stage5cPaperLoopEventKind::Ack,
            }
        );
        assert_eq!(
            failure.preserved_state().map(Stage5cPaperLoopState::kind),
            Some(Stage5cPaperLoopStateKind::TimerSettlement)
        );
    }

    #[test]
    fn stage5cn_broker_event_variant_must_match_payload_kind() {
        let (settled, request_id, bar_close_ts) =
            stage5cj_place_entry_settled(crate::BrokerNeutralOrderSide::Buy, 1.0);
        let resolved = resolve_stage5c_paper_intent_lifecycle(
            settled,
            Stage5cPaperIntentLifecycleInput {
                ack_records: vec![stage5ci_ack_record(1, request_id)],
            },
        )
        .unwrap();
        let order_record =
            stage5cj_place_order_event(2, request_id, "working", "buy", 1.0, bar_close_ts + 2);
        let failure = advance_stage5c_paper_loop_once(
            Stage5cPaperLoopState::IntentLifecycleResolved(Box::new(resolved)),
            Stage5cPaperLoopEvent::PositionEvent(Box::new(order_record)),
        )
        .expect_err("event wrapper kind must match broker payload kind");
        assert_eq!(
            failure.reason(),
            Stage5cPaperLoopError::BrokerEventKindMismatch {
                expected: Stage5cPaperBrokerEventKind::Position,
                actual: Stage5cPaperBrokerEventKind::Order,
            }
        );
        assert_eq!(
            failure.preserved_state().map(Stage5cPaperLoopState::kind),
            Some(Stage5cPaperLoopStateKind::IntentLifecycleResolved)
        );
    }

    #[test]
    fn stage5cj_position_before_filled_order_does_not_close_lifecycle() {
        let (settled, request_id, bar_close_ts) =
            stage5cj_place_entry_settled(crate::BrokerNeutralOrderSide::Buy, 1.0);
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
                event_records: vec![
                    stage5cj_position_event(2, request_id, 1.0, bar_close_ts + 2),
                    stage5cj_place_order_event(
                        3,
                        request_id,
                        "filled",
                        "buy",
                        1.0,
                        bar_close_ts + 3,
                    ),
                ],
            },
        )
        .unwrap();
        assert_eq!(
            broker_resolved.remaining_lifecycle_expectations()[0].expected_event_kind,
            Stage5cPaperBrokerEventKind::Position
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
