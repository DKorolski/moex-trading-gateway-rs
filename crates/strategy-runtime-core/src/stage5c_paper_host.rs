use std::collections::HashSet;

use broker_core::{
    BrokerAccountId, BrokerInstrumentSpec, InstrumentId, Stage4BootstrapEvidenceReport,
    Stage4BootstrapEvidenceReportStatus, Stage4BrokerTruthBootstrapStatus,
    Stage4BrokerTruthFreshnessSection, Stage4BrokerTruthFreshnessStatus,
    Stage4BrokerTruthSourceStatus, Stage4DirtyStartPolicyStatus,
    Stage4RuntimeBootstrapApplicationDecision, Stage4RuntimeBootstrapApplicationStatus,
    Stage4RuntimeBootstrapIntegrationEvent, Stage4RuntimeBootstrapIntegrationStatus,
    Stage4RuntimeLifecycleOrderingStatus, STAGE4_BOOTSTRAP_EVIDENCE_REPORT_SCHEMA_VERSION,
    STAGE4_RUNTIME_BOOTSTRAP_APPLICATION_SCHEMA_VERSION,
};
use chrono::{DateTime, Utc};
use rust_decimal::prelude::ToPrimitive;
use serde::{Deserialize, Serialize};

pub const STAGE5C_PAPER_HOST_ADMISSION_SCHEMA_VERSION: u16 = 1;

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
}

impl std::fmt::Display for Stage5cPaperHostAdmissionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "stage 5C paper host admission blocked: {self:?}")
    }
}

impl std::error::Error for Stage5cPaperHostAdmissionError {}

pub struct Stage5cPaperHostAdmissionInput<'a> {
    pub stage4j_report: &'a Stage4BootstrapEvidenceReport,
    pub stage4_application: &'a Stage4RuntimeBootstrapApplicationDecision,
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
#[derive(Clone, PartialEq)]
pub struct Stage5cPaperHostAdmission {
    schema_version: u16,
    checked_ts: DateTime<Utc>,
    account_id: BrokerAccountId,
    target_instrument: InstrumentId,
    tick_size: f64,
    paper_only: bool,
    runtime_host_attached: bool,
    intent_sink_attached: bool,
}

impl Stage5cPaperHostAdmission {
    pub fn schema_version(&self) -> u16 {
        self.schema_version
    }

    pub fn checked_ts(&self) -> DateTime<Utc> {
        self.checked_ts
    }

    pub fn account_id(&self) -> &BrokerAccountId {
        &self.account_id
    }

    pub fn target_instrument(&self) -> &InstrumentId {
        &self.target_instrument
    }

    pub fn tick_size(&self) -> f64 {
        self.tick_size
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
    let report = input.stage4j_report;
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

    let application = input.stage4_application;
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
    let snapshot = application
        .applied_snapshot
        .as_ref()
        .ok_or(Stage5cPaperHostAdmissionError::Stage4ApplicationSnapshotMissing)?;
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
        account_id: snapshot.account_id.clone(),
        target_instrument: report.target_instrument.clone(),
        tick_size: spec_tick_size,
        paper_only: true,
        runtime_host_attached: false,
        intent_sink_attached: false,
    })
}
