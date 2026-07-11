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
    ) {
        (self.strategy, self.receipt)
    }
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
    pub strategy_id: String,
    pub account_id: BrokerAccountId,
    pub instrument: InstrumentId,
    pub tick_size: f64,
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
    InvalidStateJson,
    WrongStrategyStateKind,
    LegacyNumericOrderIdRejected,
    BrokerTruthPositionMismatch,
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

    #[expect(
        dead_code,
        reason = "reserved consume-only transition for the still-closed history warmup gate"
    )]
    pub(crate) fn into_parts(
        self,
    ) -> (
        HybridIntradayRuntimeStrategy,
        Stage5cRuntimeStateRestoreReceipt,
    ) {
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

/// Consumes the admission, preventing accidental duplicate notification.
///
/// ```compile_fail
/// # use strategy_runtime_core::{notify_stage5c_bootstrap, HybridIntradayRuntimeStrategy, Stage5cPaperHostAdmission};
/// # fn duplicate(strategy: HybridIntradayRuntimeStrategy, admission: Stage5cPaperHostAdmission) {
/// let _ = notify_stage5c_bootstrap(strategy, admission);
/// let _ = notify_stage5c_bootstrap(strategy, admission);
/// # }
/// ```
pub fn notify_stage5c_bootstrap(
    strategy: HybridIntradayRuntimeStrategy,
    admission: Stage5cPaperHostAdmission,
) -> Result<Stage5cBootstrappedPaperStrategy, Stage5cBootstrapNotificationError> {
    notify_stage5c_bootstrap_at(strategy, admission, Utc::now())
}

pub(crate) fn notify_stage5c_bootstrap_at(
    mut strategy: HybridIntradayRuntimeStrategy,
    admission: Stage5cPaperHostAdmission,
    notification_now: DateTime<Utc>,
) -> Result<Stage5cBootstrappedPaperStrategy, Stage5cBootstrapNotificationError> {
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

/// Applies persisted semantic state and emits exactly one restore notification.
/// The bootstrapped type-state is consumed, so neither restore nor bootstrap can
/// be repeated on the same strategy instance.
pub fn restore_stage5c_runtime_state(
    bootstrapped: Stage5cBootstrappedPaperStrategy,
    input: Stage5cRuntimeStateRestoreInput,
) -> Result<Stage5cRuntimeStateRestoredPaperStrategy, Stage5cRuntimeStateRestoreError> {
    restore_stage5c_runtime_state_at(bootstrapped, input, Utc::now())
}

fn restore_stage5c_runtime_state_at(
    bootstrapped: Stage5cBootstrappedPaperStrategy,
    input: Stage5cRuntimeStateRestoreInput,
    restored_ts: DateTime<Utc>,
) -> Result<Stage5cRuntimeStateRestoredPaperStrategy, Stage5cRuntimeStateRestoreError> {
    let (mut strategy, bootstrap_receipt) = bootstrapped.into_parts();
    let admission = &bootstrap_receipt.admission;
    if input.schema_version != STAGE5C_RUNTIME_STATE_RESTORE_SCHEMA_VERSION {
        return Err(Stage5cRuntimeStateRestoreError::SchemaMismatch);
    }
    if restored_ts > admission.expires_at() {
        return Err(Stage5cRuntimeStateRestoreError::AdmissionExpired);
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

    let raw_state: serde_json::Value = serde_json::from_str(&input.state_json)
        .map_err(|_| Stage5cRuntimeStateRestoreError::InvalidStateJson)?;
    if input.legacy_numeric_order_id_policy == Stage5cLegacyNumericOrderIdPolicy::Reject
        && contains_numeric_order_id(&raw_state)
    {
        return Err(Stage5cRuntimeStateRestoreError::LegacyNumericOrderIdRejected);
    }
    let restored_state: StrategyState = serde_json::from_value(raw_state)
        .map_err(|_| Stage5cRuntimeStateRestoreError::InvalidStateJson)?;
    let restored_position_qty = match &restored_state {
        StrategyState::HybridIntradayRuntime {
            last_position_qty, ..
        } => last_position_qty,
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

    Strategy::set_state(&mut strategy, restored_state);
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
    let restored = RuntimeStateRestored {
        known_order_ids: input.known_order_ids,
        pending_requests: input.pending_requests,
    };
    let intents = Strategy::on_runtime_state_restored(&mut strategy, &context, &restored);
    debug_assert!(
        intents.is_empty(),
        "accepted source runtime-state restore must not emit intents"
    );

    Ok(Stage5cRuntimeStateRestoredPaperStrategy {
        strategy,
        receipt: Stage5cRuntimeStateRestoreReceipt {
            bootstrap_receipt,
            restored_ts,
        },
    })
}

fn same_tick_size(left: f64, right: f64) -> bool {
    left.is_finite() && right.is_finite() && (left - right).abs() <= f64::EPSILON
}

fn contains_numeric_order_id(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Object(fields) => fields.iter().any(|(name, value)| {
            matches!(name.as_str(), "tp_order_id" | "sl_exchange_order_id") && value.is_number()
                || contains_numeric_order_id(value)
        }),
        serde_json::Value::Array(values) => values.iter().any(contains_numeric_order_id),
        _ => false,
    }
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
        let bootstrapped = notify_stage5c_bootstrap_at(
            strategy("IMOEXF", 0.5),
            admission(Decimal::ONE, expiry),
            expiry,
        )
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
        bootstrapped: &Stage5cBootstrappedPaperStrategy,
    ) -> Stage5cRuntimeStateRestoreInput {
        Stage5cRuntimeStateRestoreInput {
            schema_version: STAGE5C_RUNTIME_STATE_RESTORE_SCHEMA_VERSION,
            strategy_id: bootstrapped.receipt().strategy_id().to_string(),
            account_id: bootstrapped
                .receipt()
                .bootstrap_snapshot()
                .account_id
                .clone(),
            instrument: bootstrapped
                .receipt()
                .bootstrap_snapshot()
                .instrument
                .clone(),
            tick_size: 0.5,
            state_json: serde_json::to_string(Strategy::state(bootstrapped.strategy()))
                .expect("state JSON"),
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
        let bootstrapped = notify_stage5c_bootstrap_at(
            strategy("IMOEXF", 0.5),
            admission(Decimal::ONE, now + chrono::Duration::minutes(1)),
            now,
        )
        .expect("bootstrap");
        let input = restore_input(&bootstrapped);
        let restored =
            restore_stage5c_runtime_state_at(bootstrapped, input, now).expect("validated restore");

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
        let bootstrapped = notify_stage5c_bootstrap_at(
            strategy("IMOEXF", 0.5),
            admission(Decimal::ONE, now + chrono::Duration::minutes(1)),
            now,
        )
        .expect("bootstrap");
        let mut input = restore_input(&bootstrapped);
        let mut state: serde_json::Value =
            serde_json::from_str(&input.state_json).expect("state value");
        state["HybridIntradayRuntime"]["last_position_qty"] = serde_json::json!(0.0);
        input.state_json = serde_json::to_string(&state).expect("state JSON");

        assert!(matches!(
            restore_stage5c_runtime_state_at(bootstrapped, input, now),
            Err(Stage5cRuntimeStateRestoreError::BrokerTruthPositionMismatch)
        ));
    }

    #[test]
    fn stage5cc_requires_explicit_legacy_numeric_order_id_policy() {
        let numeric = serde_json::json!({"tp_order_id": 123});
        assert!(contains_numeric_order_id(&numeric));
        let string = serde_json::json!({"tp_order_id": "123"});
        assert!(!contains_numeric_order_id(&string));
    }
}
