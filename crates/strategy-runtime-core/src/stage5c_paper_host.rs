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

impl Stage5cPendingRecoveredPaperStrategy {
    pub fn receipt(&self) -> &Stage5cPendingRecoveryReceipt {
        &self.receipt
    }
    #[cfg(test)]
    fn strategy(&self) -> &HybridIntradayRuntimeStrategy {
        &self.strategy
    }

    #[expect(
        dead_code,
        reason = "reserved consume-only transition for the still-closed semantic bar gate"
    )]
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
            stream.stream_name.trim().is_empty()
                || stream.consumer_group.trim().is_empty()
                || stream.terminal_claim_cursor != "0-0"
                || stream.snapshot_boundary_entry_id.trim().is_empty()
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
        if event.stream_name.trim().is_empty() || event.entry_id.trim().is_empty() {
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
    Ok(Stage5cAcceptedPendingRecoveryEvidence {
        events,
        duplicate_events,
        claim_proof: input.claim_proof,
    })
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
    let snapshot_ts = admission.bootstrap_snapshot().received_ts.timestamp();
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
        if !matches!(&event.payload, Stage5cPendingRecoveryPayload::Ack(_))
            && event_ts <= snapshot_ts
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

#[cfg(test)]
mod bootstrap_notification_tests {
    use super::*;
    use broker_core::{BrokerPositionSnapshot, Exchange, Market};
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
        let proof = recovery_claim(&warmed, now).unwrap();
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
                            consumer_group: "paper-runtime".to_string(),
                            terminal_claim_cursor: cursor.to_string(),
                            snapshot_boundary_entry_id: "0-0".to_string(),
                            claimed_count: 0,
                        },
                    )
                    .collect(),
            },
        )
    }
}
