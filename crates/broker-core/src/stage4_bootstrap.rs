use std::collections::HashSet;

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::ids::BrokerOrderId;
use crate::instrument::{InstrumentId, Quantity};
use crate::operational_config::BrokerMarketSessionState;
use crate::operational_snapshot::{
    instrument_identity_matches, BrokerOrderLifecycle, BrokerTradeSnapshot,
    BrokerTruthInstrumentSummary, BrokerTruthSnapshot,
};
use crate::runtime_host::RuntimeHostBootstrapSnapshot;
use crate::runtime_state::RuntimeBootstrapSnapshotDto;

pub const STAGE4_BROKER_TRUTH_BOOTSTRAP_SCHEMA_VERSION: u16 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Stage4BrokerTruthBootstrapStatus {
    BootstrapReady,
    BootstrapBlocked,
    ManualInterventionRequired,
    BrokerTruthIncomplete,
    BrokerTruthStale,
    InstrumentMismatch,
    UnknownSchedule,
    EvidenceIncomplete,
    SafetyBoundaryOpen,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Stage4BrokerTruthFreshnessSection {
    Positions,
    Orders,
    Trades,
    Cash,
    Instruments,
    Schedule,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Stage4BrokerTruthFreshnessStatus {
    Fresh,
    Stale,
    Unknown,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stage4BrokerTruthFreshnessProbe {
    pub observed_ts: Option<DateTime<Utc>>,
    pub max_age_ms: u64,
    pub available: bool,
    pub required_for_bootstrap: bool,
}

impl Stage4BrokerTruthFreshnessProbe {
    pub fn fresh(
        observed_ts: DateTime<Utc>,
        max_age_ms: u64,
        required_for_bootstrap: bool,
    ) -> Self {
        Self {
            observed_ts: Some(observed_ts),
            max_age_ms,
            available: true,
            required_for_bootstrap,
        }
    }

    pub fn unknown(max_age_ms: u64, required_for_bootstrap: bool) -> Self {
        Self {
            observed_ts: None,
            max_age_ms,
            available: true,
            required_for_bootstrap,
        }
    }

    pub fn unavailable(max_age_ms: u64, required_for_bootstrap: bool) -> Self {
        Self {
            observed_ts: None,
            max_age_ms,
            available: false,
            required_for_bootstrap,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stage4BrokerTruthFreshnessInput {
    pub positions: Stage4BrokerTruthFreshnessProbe,
    pub orders: Stage4BrokerTruthFreshnessProbe,
    pub trades: Stage4BrokerTruthFreshnessProbe,
    pub cash: Stage4BrokerTruthFreshnessProbe,
    pub instruments: Stage4BrokerTruthFreshnessProbe,
    pub schedule: Stage4BrokerTruthFreshnessProbe,
}

impl Stage4BrokerTruthFreshnessInput {
    pub fn from_broker_truth_received_ts(received_ts: DateTime<Utc>, max_age_ms: u64) -> Self {
        Self {
            positions: Stage4BrokerTruthFreshnessProbe::fresh(received_ts, max_age_ms, true),
            orders: Stage4BrokerTruthFreshnessProbe::fresh(received_ts, max_age_ms, true),
            trades: Stage4BrokerTruthFreshnessProbe::fresh(received_ts, max_age_ms, false),
            cash: Stage4BrokerTruthFreshnessProbe::fresh(received_ts, max_age_ms, false),
            instruments: Stage4BrokerTruthFreshnessProbe::fresh(received_ts, max_age_ms, true),
            schedule: Stage4BrokerTruthFreshnessProbe::unknown(max_age_ms, true),
        }
    }

    pub fn synthetic_all_sections_fresh_for_tests(
        received_ts: DateTime<Utc>,
        max_age_ms: u64,
    ) -> Self {
        Self {
            positions: Stage4BrokerTruthFreshnessProbe::fresh(received_ts, max_age_ms, true),
            orders: Stage4BrokerTruthFreshnessProbe::fresh(received_ts, max_age_ms, true),
            trades: Stage4BrokerTruthFreshnessProbe::fresh(received_ts, max_age_ms, false),
            cash: Stage4BrokerTruthFreshnessProbe::fresh(received_ts, max_age_ms, false),
            instruments: Stage4BrokerTruthFreshnessProbe::fresh(received_ts, max_age_ms, true),
            schedule: Stage4BrokerTruthFreshnessProbe::fresh(received_ts, max_age_ms, true),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stage4BrokerTruthFreshnessSectionEvidence {
    pub section: Stage4BrokerTruthFreshnessSection,
    pub observed_ts: Option<DateTime<Utc>>,
    pub checked_ts: DateTime<Utc>,
    pub max_age_ms: u64,
    pub age_ms: Option<i64>,
    pub status: Stage4BrokerTruthFreshnessStatus,
    pub required_for_bootstrap: bool,
    pub blocks_bootstrap: bool,
}

impl Stage4BrokerTruthFreshnessSectionEvidence {
    fn evaluate(
        section: Stage4BrokerTruthFreshnessSection,
        probe: Stage4BrokerTruthFreshnessProbe,
        checked_ts: DateTime<Utc>,
    ) -> Self {
        let age_ms = probe.observed_ts.map(|observed_ts| {
            checked_ts
                .signed_duration_since(observed_ts)
                .num_milliseconds()
        });
        let status = if !probe.available {
            Stage4BrokerTruthFreshnessStatus::Unavailable
        } else {
            match age_ms {
                Some(age_ms) if age_ms >= 0 && age_ms as u64 <= probe.max_age_ms => {
                    Stage4BrokerTruthFreshnessStatus::Fresh
                }
                Some(_) => Stage4BrokerTruthFreshnessStatus::Stale,
                None => Stage4BrokerTruthFreshnessStatus::Unknown,
            }
        };
        let blocks_bootstrap =
            probe.required_for_bootstrap && status != Stage4BrokerTruthFreshnessStatus::Fresh;
        Self {
            section,
            observed_ts: probe.observed_ts,
            checked_ts,
            max_age_ms: probe.max_age_ms,
            age_ms,
            status,
            required_for_bootstrap: probe.required_for_bootstrap,
            blocks_bootstrap,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stage4BrokerTruthFreshness {
    pub sections: Vec<Stage4BrokerTruthFreshnessSectionEvidence>,
    pub blocking_section_count: usize,
}

impl Stage4BrokerTruthFreshness {
    pub fn evaluate(input: Stage4BrokerTruthFreshnessInput, checked_ts: DateTime<Utc>) -> Self {
        let sections = vec![
            Stage4BrokerTruthFreshnessSectionEvidence::evaluate(
                Stage4BrokerTruthFreshnessSection::Positions,
                input.positions,
                checked_ts,
            ),
            Stage4BrokerTruthFreshnessSectionEvidence::evaluate(
                Stage4BrokerTruthFreshnessSection::Orders,
                input.orders,
                checked_ts,
            ),
            Stage4BrokerTruthFreshnessSectionEvidence::evaluate(
                Stage4BrokerTruthFreshnessSection::Trades,
                input.trades,
                checked_ts,
            ),
            Stage4BrokerTruthFreshnessSectionEvidence::evaluate(
                Stage4BrokerTruthFreshnessSection::Cash,
                input.cash,
                checked_ts,
            ),
            Stage4BrokerTruthFreshnessSectionEvidence::evaluate(
                Stage4BrokerTruthFreshnessSection::Instruments,
                input.instruments,
                checked_ts,
            ),
            Stage4BrokerTruthFreshnessSectionEvidence::evaluate(
                Stage4BrokerTruthFreshnessSection::Schedule,
                input.schedule,
                checked_ts,
            ),
        ];
        let blocking_section_count = sections
            .iter()
            .filter(|section| section.blocks_bootstrap)
            .count();
        Self {
            sections,
            blocking_section_count,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stage4BrokerTruthOwnershipSummary {
    pub target_active_order_count: usize,
    pub account_active_order_count: usize,
    pub runtime_owned_target_order_count: usize,
    pub adopted_target_order_count: usize,
    pub observed_account_wide_order_count: usize,
    pub unknown_or_orphan_target_order_count: usize,
    pub target_unknown_order_count: usize,
    pub target_orphan_order_count: usize,
    pub restored_runtime_working_order_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stage4BrokerTruthTradeCorrelationSummary {
    pub target_recent_trade_count: usize,
    pub strategy_attributed_trade_count: usize,
    pub observed_unattributed_target_trade_count: usize,
    pub unknown_or_orphan_target_trade_count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Stage4DirtyStartDisposition {
    CleanBootstrap,
    AdoptTargetPositionExplicitly,
    AdoptTargetOrderExplicitly,
    AdoptTargetPositionAndOrderExplicitly,
    TargetNonFlatRequiresAdoption,
    TargetActiveOrderRequiresAdoptionOrRepair,
    ManualInterventionRequired,
    EvidenceIncomplete,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Stage4AdoptionDisposition {
    pub position_adoption_attempted: bool,
    pub position_adoption_allowed: bool,
    pub position_adoption_applied: bool,
    pub order_adoption_attempted: bool,
    pub order_adoption_allowed: bool,
    pub order_adoption_applied: bool,
    pub adopted_target_position_qty: Quantity,
    pub adopted_target_order_count: usize,
}

impl Default for Stage4AdoptionDisposition {
    fn default() -> Self {
        Self {
            position_adoption_attempted: false,
            position_adoption_allowed: false,
            position_adoption_applied: false,
            order_adoption_attempted: false,
            order_adoption_allowed: false,
            order_adoption_applied: false,
            adopted_target_position_qty: Decimal::ZERO,
            adopted_target_order_count: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Stage4ManualInterventionReason {
    AmbiguousTargetPositionRows,
    TargetNonFlatWithoutAdoption,
    TargetActiveOrderWithoutAdoptionOrRepair,
    UnknownOrOrphanTargetOrder,
    UnknownOrOrphanTargetTrade,
    RestoredRuntimeStateMissingFromBrokerTruth,
    ExternalBrokerTruthIssue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Stage4BrokerTruthExternalIssueKind {
    SameIdentityDifferentRequestId,
    OrphanBrokerOrder,
    OrphanBrokerTrade,
    PositionMismatch,
    LocalPendingStale,
    ManualInterventionRequired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Stage4BrokerTruthSourceStatus {
    Present,
    Missing,
    Unavailable,
    DecodeFailed,
    Incomplete,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stage4BrokerTruthExternalIssue {
    pub kind: Stage4BrokerTruthExternalIssueKind,
    pub affects_target_instrument: bool,
    pub manual_intervention_required: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Stage4BrokerTruthReadinessBlockerKind {
    BrokerTruthMissing,
    BrokerTruthUnavailable,
    BrokerTruthDecodeFailed,
    BrokerTruthIncomplete,
    PositionsStale,
    OrdersStale,
    TradesStale,
    CashStale,
    InstrumentsStale,
    ScheduleStale,
    UnknownSchedule,
    InstrumentIdentityMismatch,
    AmbiguousTargetPositionRows,
    TargetNonFlatWithoutAdoption,
    TargetActiveOrderWithoutAdoptionOrRepair,
    UnknownOrOrphanTargetOrder,
    UnknownOrOrphanTargetTrade,
    RestoredRuntimeStateMissingFromBrokerTruth,
    AdoptionEvidenceMissing,
    ExternalSameIdentityDifferentRequestId,
    ExternalOrphanBrokerOrder,
    ExternalOrphanBrokerTrade,
    ExternalPositionMismatch,
    ExternalLocalPendingStale,
    ExternalManualInterventionRequired,
    RuntimeLiveEnabled,
    RealFinamCommandConsumerEnabled,
    StrategyDrivenRealOrdersEnabled,
    RealPostDeleteEnabled,
    StopSltpBracketEnabled,
    RawPayloadExportAttempted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stage4BrokerTruthReadinessBlocker {
    pub kind: Stage4BrokerTruthReadinessBlockerKind,
    pub section: Option<Stage4BrokerTruthFreshnessSection>,
    pub manual_intervention_reason: Option<Stage4ManualInterventionReason>,
    pub blocks_runtime_live: bool,
}

impl Stage4BrokerTruthReadinessBlocker {
    fn blocker(kind: Stage4BrokerTruthReadinessBlockerKind) -> Self {
        Self {
            kind,
            section: None,
            manual_intervention_reason: None,
            blocks_runtime_live: true,
        }
    }

    fn freshness(section: Stage4BrokerTruthFreshnessSection) -> Self {
        let kind = match section {
            Stage4BrokerTruthFreshnessSection::Positions => {
                Stage4BrokerTruthReadinessBlockerKind::PositionsStale
            }
            Stage4BrokerTruthFreshnessSection::Orders => {
                Stage4BrokerTruthReadinessBlockerKind::OrdersStale
            }
            Stage4BrokerTruthFreshnessSection::Trades => {
                Stage4BrokerTruthReadinessBlockerKind::TradesStale
            }
            Stage4BrokerTruthFreshnessSection::Cash => {
                Stage4BrokerTruthReadinessBlockerKind::CashStale
            }
            Stage4BrokerTruthFreshnessSection::Instruments => {
                Stage4BrokerTruthReadinessBlockerKind::InstrumentsStale
            }
            Stage4BrokerTruthFreshnessSection::Schedule => {
                Stage4BrokerTruthReadinessBlockerKind::ScheduleStale
            }
        };
        Self {
            kind,
            section: Some(section),
            manual_intervention_reason: None,
            blocks_runtime_live: true,
        }
    }

    fn manual(
        kind: Stage4BrokerTruthReadinessBlockerKind,
        reason: Stage4ManualInterventionReason,
    ) -> Self {
        Self {
            kind,
            section: None,
            manual_intervention_reason: Some(reason),
            blocks_runtime_live: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stage4BrokerTruthSafetyBoundary {
    pub runtime_live_enabled: bool,
    pub real_finam_command_consumer_enabled: bool,
    pub strategy_driven_real_orders_enabled: bool,
    pub real_post_delete_enabled: bool,
    pub stop_sltp_bracket_enabled: bool,
    pub raw_payload_exported: bool,
}

impl Stage4BrokerTruthSafetyBoundary {
    pub fn closed() -> Self {
        Self {
            runtime_live_enabled: false,
            real_finam_command_consumer_enabled: false,
            strategy_driven_real_orders_enabled: false,
            real_post_delete_enabled: false,
            stop_sltp_bracket_enabled: false,
            raw_payload_exported: false,
        }
    }
}

impl Default for Stage4BrokerTruthSafetyBoundary {
    fn default() -> Self {
        Self::closed()
    }
}

pub struct Stage4BrokerTruthBootstrapInput<'a> {
    pub broker_truth: &'a BrokerTruthSnapshot,
    pub broker_truth_source_status: Stage4BrokerTruthSourceStatus,
    pub target_instrument: InstrumentId,
    pub restored_runtime_state: Option<&'a RuntimeBootstrapSnapshotDto>,
    pub freshness: Stage4BrokerTruthFreshnessInput,
    pub schedule_state: BrokerMarketSessionState,
    pub adoption: Stage4AdoptionDisposition,
    pub external_issues: Vec<Stage4BrokerTruthExternalIssue>,
    pub safety_boundary: Stage4BrokerTruthSafetyBoundary,
    pub checked_ts: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ValidatedStage4BrokerTruthBootstrap {
    pub schema_version: u16,
    pub checked_ts: DateTime<Utc>,
    pub target_instrument: InstrumentId,
    pub broker_truth_source_status: Stage4BrokerTruthSourceStatus,
    pub broker_truth_received_ts: DateTime<Utc>,
    pub runtime_bootstrap_snapshot: RuntimeHostBootstrapSnapshot,
    pub broker_truth_summary: BrokerTruthInstrumentSummary,
    pub target_position_qty: Quantity,
    pub target_is_flat: bool,
    pub target_zero_qty_position_rows_count: usize,
    pub account_zero_qty_position_rows_count: usize,
    pub freshness: Stage4BrokerTruthFreshness,
    pub ownership_summary: Stage4BrokerTruthOwnershipSummary,
    pub trade_correlation_summary: Stage4BrokerTruthTradeCorrelationSummary,
    pub dirty_start_disposition: Stage4DirtyStartDisposition,
    pub adoption: Stage4AdoptionDisposition,
    pub restored_runtime_state_present: bool,
    pub restored_runtime_missing_order_count: usize,
    pub schedule_state: BrokerMarketSessionState,
    pub blockers: Vec<Stage4BrokerTruthReadinessBlocker>,
    pub blocker_count: usize,
    pub manual_intervention_required: bool,
    pub status: Stage4BrokerTruthBootstrapStatus,
    pub safety_boundary: Stage4BrokerTruthSafetyBoundary,
}

pub fn validate_stage4_broker_truth_bootstrap(
    input: Stage4BrokerTruthBootstrapInput<'_>,
) -> ValidatedStage4BrokerTruthBootstrap {
    let runtime_bootstrap_snapshot = RuntimeHostBootstrapSnapshot::from_broker_truth(
        input.broker_truth,
        input.target_instrument.clone(),
    );
    let broker_truth_summary = input
        .broker_truth
        .summarize_for_instrument(&input.target_instrument);
    let target_position_qty = input
        .broker_truth
        .target_position_qty(&input.target_instrument);
    let target_is_flat = target_position_qty == Decimal::ZERO;
    let target_zero_qty_position_rows_count = input
        .broker_truth
        .positions
        .iter()
        .filter(|position| {
            position.qty == Decimal::ZERO && position.matches_instrument(&input.target_instrument)
        })
        .count();
    let account_zero_qty_position_rows_count = input
        .broker_truth
        .positions
        .iter()
        .filter(|position| position.qty == Decimal::ZERO)
        .count();
    let freshness = Stage4BrokerTruthFreshness::evaluate(input.freshness, input.checked_ts);
    let restored_runtime_working_order_ids =
        restored_runtime_working_order_ids(input.restored_runtime_state);
    let restored_runtime_known_order_ids =
        restored_runtime_known_order_ids(input.restored_runtime_state);
    let restored_runtime_order_ids = union_order_ids(
        &restored_runtime_working_order_ids,
        &restored_runtime_known_order_ids,
    );
    let ownership_summary = build_ownership_summary(
        input.broker_truth,
        &input.target_instrument,
        &restored_runtime_working_order_ids,
        &input.adoption,
    );
    let trade_correlation_summary = build_trade_correlation_summary(
        input.broker_truth,
        &input.target_instrument,
        &restored_runtime_order_ids,
    );
    let restored_runtime_missing_order_count = restored_runtime_missing_order_count(
        input.broker_truth,
        &restored_runtime_working_order_ids,
    );
    let dirty_start_disposition = dirty_start_disposition(
        target_is_flat,
        ownership_summary.target_active_order_count,
        &input.adoption,
    );

    let mut blockers = Vec::new();
    push_broker_truth_source_blocker(&mut blockers, input.broker_truth_source_status);
    push_safety_blockers(&mut blockers, &input.safety_boundary);
    for section in freshness
        .sections
        .iter()
        .filter(|section| section.blocks_bootstrap)
    {
        blockers.push(Stage4BrokerTruthReadinessBlocker::freshness(
            section.section,
        ));
    }
    if input.schedule_state == BrokerMarketSessionState::Unknown {
        blockers.push(Stage4BrokerTruthReadinessBlocker::blocker(
            Stage4BrokerTruthReadinessBlockerKind::UnknownSchedule,
        ));
    }
    if input
        .broker_truth
        .instruments
        .iter()
        .filter(|spec| spec.matches_instrument_id(&input.target_instrument))
        .count()
        != 1
    {
        blockers.push(Stage4BrokerTruthReadinessBlocker::blocker(
            Stage4BrokerTruthReadinessBlockerKind::InstrumentIdentityMismatch,
        ));
    }
    if target_is_flat && broker_truth_summary.target_open_positions_count > 0 {
        blockers.push(Stage4BrokerTruthReadinessBlocker::manual(
            Stage4BrokerTruthReadinessBlockerKind::AmbiguousTargetPositionRows,
            Stage4ManualInterventionReason::AmbiguousTargetPositionRows,
        ));
    }
    if !target_is_flat && !input.adoption.position_adoption_applied {
        blockers.push(Stage4BrokerTruthReadinessBlocker::manual(
            Stage4BrokerTruthReadinessBlockerKind::TargetNonFlatWithoutAdoption,
            Stage4ManualInterventionReason::TargetNonFlatWithoutAdoption,
        ));
    }
    if input.adoption.position_adoption_applied
        && (!input.adoption.position_adoption_attempted
            || !input.adoption.position_adoption_allowed
            || input.adoption.adopted_target_position_qty != target_position_qty)
    {
        blockers.push(Stage4BrokerTruthReadinessBlocker::blocker(
            Stage4BrokerTruthReadinessBlockerKind::AdoptionEvidenceMissing,
        ));
    }
    if !input.adoption.position_adoption_applied
        && input.adoption.adopted_target_position_qty != Decimal::ZERO
    {
        blockers.push(Stage4BrokerTruthReadinessBlocker::blocker(
            Stage4BrokerTruthReadinessBlockerKind::AdoptionEvidenceMissing,
        ));
    }
    if ownership_summary.unknown_or_orphan_target_order_count > 0 {
        blockers.push(Stage4BrokerTruthReadinessBlocker::manual(
            Stage4BrokerTruthReadinessBlockerKind::UnknownOrOrphanTargetOrder,
            Stage4ManualInterventionReason::UnknownOrOrphanTargetOrder,
        ));
    }
    if ownership_summary.target_active_order_count
        > ownership_summary.runtime_owned_target_order_count
            + ownership_summary.adopted_target_order_count
    {
        blockers.push(Stage4BrokerTruthReadinessBlocker::manual(
            Stage4BrokerTruthReadinessBlockerKind::TargetActiveOrderWithoutAdoptionOrRepair,
            Stage4ManualInterventionReason::TargetActiveOrderWithoutAdoptionOrRepair,
        ));
    }
    let expected_adoptable_target_order_count = ownership_summary
        .target_active_order_count
        .saturating_sub(ownership_summary.runtime_owned_target_order_count);
    if input.adoption.order_adoption_applied
        && (!input.adoption.order_adoption_attempted
            || !input.adoption.order_adoption_allowed
            || input.adoption.adopted_target_order_count != expected_adoptable_target_order_count)
    {
        blockers.push(Stage4BrokerTruthReadinessBlocker::blocker(
            Stage4BrokerTruthReadinessBlockerKind::AdoptionEvidenceMissing,
        ));
    }
    if !input.adoption.order_adoption_applied && input.adoption.adopted_target_order_count != 0 {
        blockers.push(Stage4BrokerTruthReadinessBlocker::blocker(
            Stage4BrokerTruthReadinessBlockerKind::AdoptionEvidenceMissing,
        ));
    }
    if trade_correlation_summary.unknown_or_orphan_target_trade_count > 0 {
        blockers.push(Stage4BrokerTruthReadinessBlocker::manual(
            Stage4BrokerTruthReadinessBlockerKind::UnknownOrOrphanTargetTrade,
            Stage4ManualInterventionReason::UnknownOrOrphanTargetTrade,
        ));
    }
    if restored_runtime_missing_order_count > 0 {
        blockers.push(Stage4BrokerTruthReadinessBlocker::manual(
            Stage4BrokerTruthReadinessBlockerKind::RestoredRuntimeStateMissingFromBrokerTruth,
            Stage4ManualInterventionReason::RestoredRuntimeStateMissingFromBrokerTruth,
        ));
    }
    for issue in &input.external_issues {
        if issue.affects_target_instrument {
            blockers.push(blocker_from_external_issue(issue));
        }
    }

    let manual_intervention_required = blockers
        .iter()
        .any(|blocker| blocker.manual_intervention_reason.is_some());
    let status = stage4_status(&blockers);
    let blocker_count = blockers.len();
    ValidatedStage4BrokerTruthBootstrap {
        schema_version: STAGE4_BROKER_TRUTH_BOOTSTRAP_SCHEMA_VERSION,
        checked_ts: input.checked_ts,
        target_instrument: input.target_instrument,
        broker_truth_source_status: input.broker_truth_source_status,
        broker_truth_received_ts: input.broker_truth.received_ts,
        runtime_bootstrap_snapshot,
        broker_truth_summary,
        target_position_qty,
        target_is_flat,
        target_zero_qty_position_rows_count,
        account_zero_qty_position_rows_count,
        freshness,
        ownership_summary,
        trade_correlation_summary,
        dirty_start_disposition,
        adoption: input.adoption,
        restored_runtime_state_present: input.restored_runtime_state.is_some(),
        restored_runtime_missing_order_count,
        schedule_state: input.schedule_state,
        blockers,
        blocker_count,
        manual_intervention_required,
        status,
        safety_boundary: input.safety_boundary,
    }
}

fn push_safety_blockers(
    blockers: &mut Vec<Stage4BrokerTruthReadinessBlocker>,
    safety: &Stage4BrokerTruthSafetyBoundary,
) {
    if safety.runtime_live_enabled {
        blockers.push(Stage4BrokerTruthReadinessBlocker::blocker(
            Stage4BrokerTruthReadinessBlockerKind::RuntimeLiveEnabled,
        ));
    }
    if safety.real_finam_command_consumer_enabled {
        blockers.push(Stage4BrokerTruthReadinessBlocker::blocker(
            Stage4BrokerTruthReadinessBlockerKind::RealFinamCommandConsumerEnabled,
        ));
    }
    if safety.strategy_driven_real_orders_enabled {
        blockers.push(Stage4BrokerTruthReadinessBlocker::blocker(
            Stage4BrokerTruthReadinessBlockerKind::StrategyDrivenRealOrdersEnabled,
        ));
    }
    if safety.real_post_delete_enabled {
        blockers.push(Stage4BrokerTruthReadinessBlocker::blocker(
            Stage4BrokerTruthReadinessBlockerKind::RealPostDeleteEnabled,
        ));
    }
    if safety.stop_sltp_bracket_enabled {
        blockers.push(Stage4BrokerTruthReadinessBlocker::blocker(
            Stage4BrokerTruthReadinessBlockerKind::StopSltpBracketEnabled,
        ));
    }
    if safety.raw_payload_exported {
        blockers.push(Stage4BrokerTruthReadinessBlocker::blocker(
            Stage4BrokerTruthReadinessBlockerKind::RawPayloadExportAttempted,
        ));
    }
}

fn push_broker_truth_source_blocker(
    blockers: &mut Vec<Stage4BrokerTruthReadinessBlocker>,
    source_status: Stage4BrokerTruthSourceStatus,
) {
    let kind = match source_status {
        Stage4BrokerTruthSourceStatus::Present => return,
        Stage4BrokerTruthSourceStatus::Missing => {
            Stage4BrokerTruthReadinessBlockerKind::BrokerTruthMissing
        }
        Stage4BrokerTruthSourceStatus::Unavailable => {
            Stage4BrokerTruthReadinessBlockerKind::BrokerTruthUnavailable
        }
        Stage4BrokerTruthSourceStatus::DecodeFailed => {
            Stage4BrokerTruthReadinessBlockerKind::BrokerTruthDecodeFailed
        }
        Stage4BrokerTruthSourceStatus::Incomplete => {
            Stage4BrokerTruthReadinessBlockerKind::BrokerTruthIncomplete
        }
    };
    blockers.push(Stage4BrokerTruthReadinessBlocker::blocker(kind));
}

fn stage4_status(
    blockers: &[Stage4BrokerTruthReadinessBlocker],
) -> Stage4BrokerTruthBootstrapStatus {
    if blockers.is_empty() {
        return Stage4BrokerTruthBootstrapStatus::BootstrapReady;
    }
    if blockers.iter().any(|blocker| {
        matches!(
            blocker.kind,
            Stage4BrokerTruthReadinessBlockerKind::RuntimeLiveEnabled
                | Stage4BrokerTruthReadinessBlockerKind::RealFinamCommandConsumerEnabled
                | Stage4BrokerTruthReadinessBlockerKind::StrategyDrivenRealOrdersEnabled
                | Stage4BrokerTruthReadinessBlockerKind::RealPostDeleteEnabled
                | Stage4BrokerTruthReadinessBlockerKind::StopSltpBracketEnabled
                | Stage4BrokerTruthReadinessBlockerKind::RawPayloadExportAttempted
        )
    }) {
        return Stage4BrokerTruthBootstrapStatus::SafetyBoundaryOpen;
    }
    if blockers.iter().any(|blocker| {
        matches!(
            blocker.kind,
            Stage4BrokerTruthReadinessBlockerKind::BrokerTruthMissing
                | Stage4BrokerTruthReadinessBlockerKind::BrokerTruthUnavailable
                | Stage4BrokerTruthReadinessBlockerKind::BrokerTruthDecodeFailed
                | Stage4BrokerTruthReadinessBlockerKind::BrokerTruthIncomplete
        )
    }) {
        return Stage4BrokerTruthBootstrapStatus::BrokerTruthIncomplete;
    }
    if blockers.iter().any(|blocker| {
        blocker.kind == Stage4BrokerTruthReadinessBlockerKind::InstrumentIdentityMismatch
    }) {
        return Stage4BrokerTruthBootstrapStatus::InstrumentMismatch;
    }
    if blockers
        .iter()
        .any(|blocker| blocker.kind == Stage4BrokerTruthReadinessBlockerKind::UnknownSchedule)
    {
        return Stage4BrokerTruthBootstrapStatus::UnknownSchedule;
    }
    if blockers.iter().any(|blocker| {
        matches!(
            blocker.kind,
            Stage4BrokerTruthReadinessBlockerKind::PositionsStale
                | Stage4BrokerTruthReadinessBlockerKind::OrdersStale
                | Stage4BrokerTruthReadinessBlockerKind::TradesStale
                | Stage4BrokerTruthReadinessBlockerKind::CashStale
                | Stage4BrokerTruthReadinessBlockerKind::InstrumentsStale
                | Stage4BrokerTruthReadinessBlockerKind::ScheduleStale
        )
    }) {
        return Stage4BrokerTruthBootstrapStatus::BrokerTruthStale;
    }
    if blockers.iter().any(|blocker| {
        blocker.kind == Stage4BrokerTruthReadinessBlockerKind::AdoptionEvidenceMissing
    }) {
        return Stage4BrokerTruthBootstrapStatus::EvidenceIncomplete;
    }
    if blockers
        .iter()
        .any(|blocker| blocker.manual_intervention_reason.is_some())
    {
        return Stage4BrokerTruthBootstrapStatus::ManualInterventionRequired;
    }
    Stage4BrokerTruthBootstrapStatus::BootstrapBlocked
}

fn dirty_start_disposition(
    target_is_flat: bool,
    target_active_order_count: usize,
    adoption: &Stage4AdoptionDisposition,
) -> Stage4DirtyStartDisposition {
    let position_adopted = !target_is_flat && adoption.position_adoption_applied;
    let order_adopted = target_active_order_count > 0 && adoption.order_adoption_applied;
    if position_adopted && order_adopted {
        return Stage4DirtyStartDisposition::AdoptTargetPositionAndOrderExplicitly;
    }
    if position_adopted {
        return Stage4DirtyStartDisposition::AdoptTargetPositionExplicitly;
    }
    if order_adopted {
        return Stage4DirtyStartDisposition::AdoptTargetOrderExplicitly;
    }
    if !target_is_flat && !adoption.position_adoption_applied {
        return Stage4DirtyStartDisposition::TargetNonFlatRequiresAdoption;
    }
    if target_active_order_count > 0 && !adoption.order_adoption_applied {
        return Stage4DirtyStartDisposition::TargetActiveOrderRequiresAdoptionOrRepair;
    }
    Stage4DirtyStartDisposition::CleanBootstrap
}

fn restored_runtime_working_order_ids(
    restored_runtime_state: Option<&RuntimeBootstrapSnapshotDto>,
) -> HashSet<BrokerOrderId> {
    let Some(restored_runtime_state) = restored_runtime_state else {
        return HashSet::new();
    };
    restored_runtime_state
        .working_orders
        .keys()
        .chain(restored_runtime_state.working_orders_strategy.keys())
        .cloned()
        .collect()
}

fn restored_runtime_known_order_ids(
    restored_runtime_state: Option<&RuntimeBootstrapSnapshotDto>,
) -> HashSet<BrokerOrderId> {
    let Some(restored_runtime_state) = restored_runtime_state else {
        return HashSet::new();
    };
    restored_runtime_state
        .known_order_ids
        .iter()
        .cloned()
        .collect()
}

fn union_order_ids(
    left: &HashSet<BrokerOrderId>,
    right: &HashSet<BrokerOrderId>,
) -> HashSet<BrokerOrderId> {
    left.union(right).cloned().collect()
}

fn restored_runtime_missing_order_count(
    truth: &BrokerTruthSnapshot,
    restored_runtime_working_order_ids: &HashSet<BrokerOrderId>,
) -> usize {
    restored_runtime_working_order_ids
        .iter()
        .filter(|order_id| {
            !truth
                .orders
                .iter()
                .any(|order| order.broker_order_id.as_ref() == Some(*order_id))
                && !truth
                    .trades
                    .iter()
                    .any(|trade| trade.broker_order_id.as_ref() == Some(*order_id))
        })
        .count()
}

fn build_ownership_summary(
    truth: &BrokerTruthSnapshot,
    target_instrument: &InstrumentId,
    restored_runtime_order_ids: &HashSet<BrokerOrderId>,
    adoption: &Stage4AdoptionDisposition,
) -> Stage4BrokerTruthOwnershipSummary {
    let target_active_orders = truth.active_orders_for_instrument(target_instrument);
    let runtime_owned_target_order_count = target_active_orders
        .iter()
        .filter(|order| {
            order
                .broker_order_id
                .as_ref()
                .is_some_and(|order_id| restored_runtime_order_ids.contains(order_id))
        })
        .count();
    let target_orphan_order_count = truth
        .orders
        .iter()
        .filter(|order| {
            instrument_identity_matches(&order.instrument, target_instrument)
                && (order.is_active_for_lifecycle()
                    || order.lifecycle == BrokerOrderLifecycle::Unknown)
                && !truth.orphan_reasons_for_order(order).is_empty()
        })
        .count();
    let target_unknown_order_count = truth.unknown_orders_for_instrument(target_instrument).len();
    let target_active_order_count = target_active_orders.len();
    let adopted_target_order_count = if adoption.order_adoption_applied {
        adoption.adopted_target_order_count
    } else {
        0
    };
    let unknown_or_orphan_target_order_count = target_active_order_count
        .saturating_sub(runtime_owned_target_order_count + adopted_target_order_count)
        + target_unknown_order_count
        + target_orphan_order_count;
    Stage4BrokerTruthOwnershipSummary {
        target_active_order_count,
        account_active_order_count: truth.account_wide_active_order_count(),
        runtime_owned_target_order_count,
        adopted_target_order_count,
        observed_account_wide_order_count: truth
            .account_wide_active_order_count()
            .saturating_sub(target_active_order_count),
        unknown_or_orphan_target_order_count,
        target_unknown_order_count,
        target_orphan_order_count,
        restored_runtime_working_order_count: restored_runtime_order_ids.len(),
    }
}

fn build_trade_correlation_summary(
    truth: &BrokerTruthSnapshot,
    target_instrument: &InstrumentId,
    restored_runtime_order_ids: &HashSet<BrokerOrderId>,
) -> Stage4BrokerTruthTradeCorrelationSummary {
    let target_trades = truth
        .trades
        .iter()
        .filter(|trade| instrument_identity_matches(&trade.instrument, target_instrument))
        .collect::<Vec<_>>();
    let strategy_attributed_trade_count = target_trades
        .iter()
        .filter(|trade| trade_order_is_restored_runtime_owned(trade, restored_runtime_order_ids))
        .count();
    let unknown_or_orphan_target_trade_count = target_trades
        .len()
        .saturating_sub(strategy_attributed_trade_count);
    let observed_unattributed_target_trade_count = target_trades
        .len()
        .saturating_sub(strategy_attributed_trade_count + unknown_or_orphan_target_trade_count);
    Stage4BrokerTruthTradeCorrelationSummary {
        target_recent_trade_count: target_trades.len(),
        strategy_attributed_trade_count,
        observed_unattributed_target_trade_count,
        unknown_or_orphan_target_trade_count,
    }
}

fn trade_order_is_restored_runtime_owned(
    trade: &BrokerTradeSnapshot,
    restored_runtime_order_ids: &HashSet<BrokerOrderId>,
) -> bool {
    trade
        .broker_order_id
        .as_ref()
        .is_some_and(|order_id| restored_runtime_order_ids.contains(order_id))
}

fn blocker_from_external_issue(
    issue: &Stage4BrokerTruthExternalIssue,
) -> Stage4BrokerTruthReadinessBlocker {
    let (kind, manual_reason) = match issue.kind {
        Stage4BrokerTruthExternalIssueKind::SameIdentityDifferentRequestId => (
            Stage4BrokerTruthReadinessBlockerKind::ExternalSameIdentityDifferentRequestId,
            None,
        ),
        Stage4BrokerTruthExternalIssueKind::OrphanBrokerOrder => (
            Stage4BrokerTruthReadinessBlockerKind::ExternalOrphanBrokerOrder,
            Some(Stage4ManualInterventionReason::ExternalBrokerTruthIssue),
        ),
        Stage4BrokerTruthExternalIssueKind::OrphanBrokerTrade => (
            Stage4BrokerTruthReadinessBlockerKind::ExternalOrphanBrokerTrade,
            Some(Stage4ManualInterventionReason::ExternalBrokerTruthIssue),
        ),
        Stage4BrokerTruthExternalIssueKind::PositionMismatch => (
            Stage4BrokerTruthReadinessBlockerKind::ExternalPositionMismatch,
            Some(Stage4ManualInterventionReason::ExternalBrokerTruthIssue),
        ),
        Stage4BrokerTruthExternalIssueKind::LocalPendingStale => (
            Stage4BrokerTruthReadinessBlockerKind::ExternalLocalPendingStale,
            Some(Stage4ManualInterventionReason::ExternalBrokerTruthIssue),
        ),
        Stage4BrokerTruthExternalIssueKind::ManualInterventionRequired => (
            Stage4BrokerTruthReadinessBlockerKind::ExternalManualInterventionRequired,
            Some(Stage4ManualInterventionReason::ExternalBrokerTruthIssue),
        ),
    };
    Stage4BrokerTruthReadinessBlocker {
        kind,
        section: None,
        manual_intervention_reason: manual_reason.filter(|_| issue.manual_intervention_required),
        blocks_runtime_live: true,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use chrono::TimeZone;

    use super::*;
    use crate::broker::BrokerKind;
    use crate::ids::{BrokerAccountId, BrokerOrderId, BrokerTradeId, ClientOrderId};
    use crate::instrument::{
        BrokerSymbol, Exchange, InstrumentMapEntry, InternalSymbol, Market, Money,
    };
    use crate::operational_snapshot::{
        BrokerInstrumentSpec, BrokerOrderLifecycle, BrokerOrderSnapshot, BrokerPositionSnapshot,
        BrokerTradeSnapshot,
    };
    use crate::order::{OrderSide, OrderStatus, OrderType};
    use crate::runtime_state::RuntimeOrderEvent;

    fn checked_ts() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 7, 10, 9, 10, 0)
            .single()
            .expect("timestamp")
    }

    fn target() -> InstrumentId {
        InstrumentId {
            symbol: "IMOEXF".to_string(),
            venue_symbol: Some("IMOEXF@RTSX".to_string()),
            exchange: Exchange::Moex,
            market: Market::Futures,
        }
    }

    fn spec() -> BrokerInstrumentSpec {
        BrokerInstrumentSpec {
            instrument: InstrumentMapEntry {
                internal_symbol: InternalSymbol("IMOEXF".to_string()),
                broker: BrokerKind::Finam,
                broker_symbol: BrokerSymbol("IMOEXF@RTSX".to_string()),
                exchange: Exchange::Moex,
                market: Market::Futures,
                price_step: Decimal::new(5, 1),
                qty_step: Decimal::ONE,
                lot_size: Decimal::ONE,
                min_qty: Decimal::ONE,
                step_value: Decimal::new(5, 0),
                currency: "RUB".to_string(),
                schedule_id: "RTSX".to_string(),
                expiration_date: None,
                is_tradable: true,
            },
            broker_asset_id: Some("ASSET_TEST_1".to_string()),
            board: Some("RTSX".to_string()),
            long_initial_margin: Some(Money::new(5000, 0)),
            short_initial_margin: Some(Money::new(5000, 0)),
        }
    }

    fn base_truth() -> BrokerTruthSnapshot {
        let now = checked_ts();
        BrokerTruthSnapshot {
            account_id: BrokerAccountId::new("ACC_TEST_0001"),
            orders: Vec::new(),
            positions: Vec::new(),
            cash: None,
            trades: Vec::new(),
            instruments: vec![spec()],
            received_ts: now,
        }
    }

    fn input<'a>(truth: &'a BrokerTruthSnapshot) -> Stage4BrokerTruthBootstrapInput<'a> {
        Stage4BrokerTruthBootstrapInput {
            broker_truth: truth,
            broker_truth_source_status: Stage4BrokerTruthSourceStatus::Present,
            target_instrument: target(),
            restored_runtime_state: None,
            freshness: Stage4BrokerTruthFreshnessInput::synthetic_all_sections_fresh_for_tests(
                truth.received_ts,
                60_000,
            ),
            schedule_state: BrokerMarketSessionState::Open,
            adoption: Stage4AdoptionDisposition::default(),
            external_issues: Vec::new(),
            safety_boundary: Stage4BrokerTruthSafetyBoundary::closed(),
            checked_ts: checked_ts(),
        }
    }

    fn target_position(qty: Decimal) -> BrokerPositionSnapshot {
        BrokerPositionSnapshot {
            account_id: BrokerAccountId::new("ACC_TEST_0001"),
            instrument: target(),
            qty,
            avg_price: None,
            unrealized_pnl: None,
            source_ts: Some(checked_ts()),
            received_ts: checked_ts(),
        }
    }

    fn target_order(order_id: &str, status: OrderStatus) -> BrokerOrderSnapshot {
        BrokerOrderSnapshot {
            account_id: BrokerAccountId::new("ACC_TEST_0001"),
            broker_order_id: Some(BrokerOrderId::new(order_id)),
            client_order_id: Some(ClientOrderId::new("CLIENT-ORDER-1").expect("client id")),
            instrument: target(),
            side: OrderSide::Buy,
            order_type: OrderType::Limit,
            time_in_force: None,
            status: status.clone(),
            lifecycle: BrokerOrderSnapshot::lifecycle_for(&status),
            qty: Decimal::ONE,
            filled_qty: Decimal::ZERO,
            remaining_qty: Some(Decimal::ONE),
            limit_price: Some(Decimal::new(2210, 0)),
            broker_asset_id: Some("ASSET_TEST_1".to_string()),
            board: Some("RTSX".to_string()),
            expiration_date: None,
            source_ts: Some(checked_ts()),
            received_ts: checked_ts(),
        }
    }

    #[test]
    fn stage4c_clean_flat_bootstrap_is_ready_without_live_authorization() {
        let truth = base_truth();
        let report = validate_stage4_broker_truth_bootstrap(input(&truth));

        assert_eq!(
            report.status,
            Stage4BrokerTruthBootstrapStatus::BootstrapReady
        );
        assert_eq!(report.blocker_count, 0);
        assert!(report.target_is_flat);
        assert!(!report.safety_boundary.runtime_live_enabled);
        assert!(!report.safety_boundary.real_finam_command_consumer_enabled);
        assert!(!report.safety_boundary.strategy_driven_real_orders_enabled);
    }

    #[test]
    fn stage4c_required_stale_section_blocks_as_broker_truth_stale() {
        let truth = base_truth();
        let mut request = input(&truth);
        request.freshness.positions = Stage4BrokerTruthFreshnessProbe::fresh(
            checked_ts() - chrono::Duration::seconds(120),
            60_000,
            true,
        );
        let report = validate_stage4_broker_truth_bootstrap(request);

        assert_eq!(
            report.status,
            Stage4BrokerTruthBootstrapStatus::BrokerTruthStale
        );
        assert!(report.blockers.iter().any(|blocker| {
            blocker.kind == Stage4BrokerTruthReadinessBlockerKind::PositionsStale
        }));
    }

    #[test]
    fn stage4c_target_non_flat_without_adoption_requires_manual_intervention() {
        let mut truth = base_truth();
        truth.positions.push(target_position(Decimal::new(3, 0)));

        let report = validate_stage4_broker_truth_bootstrap(input(&truth));

        assert_eq!(
            report.status,
            Stage4BrokerTruthBootstrapStatus::ManualInterventionRequired
        );
        assert_eq!(report.target_position_qty, Decimal::new(3, 0));
        assert!(!report.target_is_flat);
        assert!(!report.runtime_bootstrap_snapshot.target_is_flat);
        assert!(report.blockers.iter().any(|blocker| {
            blocker.kind == Stage4BrokerTruthReadinessBlockerKind::TargetNonFlatWithoutAdoption
        }));
    }

    #[test]
    fn stage4c_valid_position_adoption_can_be_bootstrap_ready() {
        let mut truth = base_truth();
        truth.positions.push(target_position(Decimal::new(3, 0)));
        let mut request = input(&truth);
        request.adoption = Stage4AdoptionDisposition {
            position_adoption_attempted: true,
            position_adoption_allowed: true,
            position_adoption_applied: true,
            adopted_target_position_qty: Decimal::new(3, 0),
            ..Stage4AdoptionDisposition::default()
        };

        let report = validate_stage4_broker_truth_bootstrap(request);

        assert_eq!(
            report.status,
            Stage4BrokerTruthBootstrapStatus::BootstrapReady
        );
        assert_eq!(
            report.dirty_start_disposition,
            Stage4DirtyStartDisposition::AdoptTargetPositionExplicitly
        );
        assert!(!report.manual_intervention_required);
    }

    #[test]
    fn stage4c_target_active_order_cannot_silently_disappear() {
        let mut truth = base_truth();
        truth
            .orders
            .push(target_order("BROKER-ORDER-1", OrderStatus::Working));

        let report = validate_stage4_broker_truth_bootstrap(input(&truth));

        assert_eq!(
            report.status,
            Stage4BrokerTruthBootstrapStatus::ManualInterventionRequired
        );
        assert_eq!(report.ownership_summary.target_active_order_count, 1);
        assert_eq!(
            report.runtime_bootstrap_snapshot.target_active_orders.len(),
            1
        );
        assert!(report.blockers.iter().any(|blocker| {
            blocker.kind
                == Stage4BrokerTruthReadinessBlockerKind::TargetActiveOrderWithoutAdoptionOrRepair
        }));
    }

    #[test]
    fn stage4c_valid_order_adoption_can_be_bootstrap_ready() {
        let mut truth = base_truth();
        truth
            .orders
            .push(target_order("BROKER-ORDER-1", OrderStatus::Working));
        let mut request = input(&truth);
        request.adoption = Stage4AdoptionDisposition {
            order_adoption_attempted: true,
            order_adoption_allowed: true,
            order_adoption_applied: true,
            adopted_target_order_count: 1,
            ..Stage4AdoptionDisposition::default()
        };

        let report = validate_stage4_broker_truth_bootstrap(request);

        assert_eq!(
            report.status,
            Stage4BrokerTruthBootstrapStatus::BootstrapReady
        );
        assert_eq!(
            report.dirty_start_disposition,
            Stage4DirtyStartDisposition::AdoptTargetOrderExplicitly
        );
        assert!(!report.manual_intervention_required);
    }

    #[test]
    fn stage4c_order_adoption_count_must_match_target_active_order_truth() {
        let mut truth = base_truth();
        truth
            .orders
            .push(target_order("BROKER-ORDER-1", OrderStatus::Working));
        let mut request = input(&truth);
        request.adoption = Stage4AdoptionDisposition {
            order_adoption_attempted: true,
            order_adoption_allowed: true,
            order_adoption_applied: true,
            adopted_target_order_count: 999,
            ..Stage4AdoptionDisposition::default()
        };

        let report = validate_stage4_broker_truth_bootstrap(request);

        assert_eq!(
            report.status,
            Stage4BrokerTruthBootstrapStatus::EvidenceIncomplete
        );
        assert_eq!(report.adoption.adopted_target_order_count, 999);
        assert!(report.blockers.iter().any(|blocker| {
            blocker.kind == Stage4BrokerTruthReadinessBlockerKind::AdoptionEvidenceMissing
        }));
    }

    #[test]
    fn stage4c_order_adoption_count_without_applied_flag_cannot_suppress_active_order_blocker() {
        let mut truth = base_truth();
        truth
            .orders
            .push(target_order("BROKER-ORDER-1", OrderStatus::Working));
        let mut request = input(&truth);
        request.adoption = Stage4AdoptionDisposition {
            order_adoption_attempted: false,
            order_adoption_allowed: false,
            order_adoption_applied: false,
            adopted_target_order_count: 1,
            ..Stage4AdoptionDisposition::default()
        };

        let report = validate_stage4_broker_truth_bootstrap(request);

        assert_eq!(
            report.status,
            Stage4BrokerTruthBootstrapStatus::EvidenceIncomplete
        );
        assert_eq!(report.ownership_summary.adopted_target_order_count, 0);
        assert_eq!(report.adoption.adopted_target_order_count, 1);
        assert!(report.blockers.iter().any(|blocker| {
            blocker.kind == Stage4BrokerTruthReadinessBlockerKind::AdoptionEvidenceMissing
        }));
        assert!(report.blockers.iter().any(|blocker| {
            blocker.kind == Stage4BrokerTruthReadinessBlockerKind::UnknownOrOrphanTargetOrder
        }));
    }

    #[test]
    fn stage4c_position_adoption_qty_without_applied_flag_is_evidence_incomplete() {
        let truth = base_truth();
        let mut request = input(&truth);
        request.adoption = Stage4AdoptionDisposition {
            position_adoption_attempted: false,
            position_adoption_allowed: false,
            position_adoption_applied: false,
            adopted_target_position_qty: Decimal::ONE,
            ..Stage4AdoptionDisposition::default()
        };

        let report = validate_stage4_broker_truth_bootstrap(request);

        assert_eq!(
            report.status,
            Stage4BrokerTruthBootstrapStatus::EvidenceIncomplete
        );
        assert!(report.blockers.iter().any(|blocker| {
            blocker.kind == Stage4BrokerTruthReadinessBlockerKind::AdoptionEvidenceMissing
        }));
    }

    #[test]
    fn stage4c_zero_qty_position_rows_are_diagnostic_not_open_position_truth() {
        let mut truth = base_truth();
        truth.positions.push(target_position(Decimal::ZERO));

        let report = validate_stage4_broker_truth_bootstrap(input(&truth));

        assert_eq!(
            report.status,
            Stage4BrokerTruthBootstrapStatus::BootstrapReady
        );
        assert_eq!(report.target_zero_qty_position_rows_count, 1);
        assert_eq!(report.account_zero_qty_position_rows_count, 1);
        assert!(report.target_is_flat);
    }

    #[test]
    fn stage4c_net_flat_from_offsetting_open_rows_requires_manual_intervention() {
        let mut truth = base_truth();
        truth.positions.push(target_position(Decimal::ONE));
        truth.positions.push(target_position(-Decimal::ONE));

        let report = validate_stage4_broker_truth_bootstrap(input(&truth));

        assert_eq!(
            report.status,
            Stage4BrokerTruthBootstrapStatus::ManualInterventionRequired
        );
        assert!(report.target_is_flat);
        assert_eq!(report.broker_truth_summary.target_open_positions_count, 2);
        assert!(report.blockers.iter().any(|blocker| {
            blocker.kind == Stage4BrokerTruthReadinessBlockerKind::AmbiguousTargetPositionRows
        }));
    }

    #[test]
    fn stage4c_restored_runtime_state_remains_state_not_broker_truth() {
        let truth = base_truth();
        let order_id = BrokerOrderId::new("RESTORED-MISSING");
        let mut working_orders_strategy = HashMap::new();
        working_orders_strategy.insert(
            order_id.clone(),
            RuntimeOrderEvent {
                order_id: order_id.clone(),
                client_order_id: None,
                symbol: Some("IMOEXF".to_string()),
                exchange: Some("MOEX".to_string()),
                status: Some("working".to_string()),
                side: Some("buy".to_string()),
                order_type: Some("limit".to_string()),
                source_ts: Some(checked_ts()),
            },
        );
        let restored = RuntimeBootstrapSnapshotDto {
            working_orders: HashMap::new(),
            working_orders_strategy,
            known_order_ids: vec![order_id],
            account_wide_orders_count: 1,
        };
        let mut request = input(&truth);
        request.restored_runtime_state = Some(&restored);

        let report = validate_stage4_broker_truth_bootstrap(request);

        assert!(report.restored_runtime_state_present);
        assert_eq!(report.restored_runtime_missing_order_count, 1);
        assert_eq!(
            report.status,
            Stage4BrokerTruthBootstrapStatus::ManualInterventionRequired
        );
        assert!(report.blockers.iter().any(|blocker| {
            blocker.kind
                == Stage4BrokerTruthReadinessBlockerKind::RestoredRuntimeStateMissingFromBrokerTruth
        }));
    }

    #[test]
    fn stage4c_historical_known_order_absent_from_current_broker_truth_is_diagnostic() {
        let truth = base_truth();
        let restored = RuntimeBootstrapSnapshotDto {
            working_orders: HashMap::new(),
            working_orders_strategy: HashMap::new(),
            known_order_ids: vec![BrokerOrderId::new("HISTORICAL-TERMINAL-1")],
            account_wide_orders_count: 1,
        };
        let mut request = input(&truth);
        request.restored_runtime_state = Some(&restored);

        let report = validate_stage4_broker_truth_bootstrap(request);

        assert_eq!(
            report.status,
            Stage4BrokerTruthBootstrapStatus::BootstrapReady
        );
        assert!(report.restored_runtime_state_present);
        assert_eq!(report.restored_runtime_missing_order_count, 0);
    }

    #[test]
    fn stage4c_unknown_orphan_target_trade_blocks_readiness() {
        let mut truth = base_truth();
        truth.trades.push(BrokerTradeSnapshot {
            account_id: BrokerAccountId::new("ACC_TEST_0001"),
            broker_trade_id: BrokerTradeId::new("TRADE-1"),
            broker_order_id: None,
            client_order_id: None,
            instrument: target(),
            side: OrderSide::Buy,
            qty: Decimal::ONE,
            price: Decimal::new(2210, 0),
            gross_amount: None,
            commission: None,
            broker_asset_id: Some("ASSET_TEST_1".to_string()),
            board: Some("RTSX".to_string()),
            expiration_date: None,
            source_ts: checked_ts(),
            received_ts: checked_ts(),
        });

        let report = validate_stage4_broker_truth_bootstrap(input(&truth));

        assert_eq!(
            report.status,
            Stage4BrokerTruthBootstrapStatus::ManualInterventionRequired
        );
        assert_eq!(
            report
                .trade_correlation_summary
                .unknown_or_orphan_target_trade_count,
            1
        );
        assert!(report.blockers.iter().any(|blocker| {
            blocker.kind == Stage4BrokerTruthReadinessBlockerKind::UnknownOrOrphanTargetTrade
        }));
    }

    #[test]
    fn stage4c_target_trade_with_unowned_broker_order_id_blocks_until_reconciled() {
        let mut truth = base_truth();
        truth.trades.push(BrokerTradeSnapshot {
            account_id: BrokerAccountId::new("ACC_TEST_0001"),
            broker_trade_id: BrokerTradeId::new("TRADE-2"),
            broker_order_id: Some(BrokerOrderId::new("UNOWNED-BROKER-ORDER")),
            client_order_id: None,
            instrument: target(),
            side: OrderSide::Buy,
            qty: Decimal::ONE,
            price: Decimal::new(2210, 0),
            gross_amount: None,
            commission: None,
            broker_asset_id: Some("ASSET_TEST_1".to_string()),
            board: Some("RTSX".to_string()),
            expiration_date: None,
            source_ts: checked_ts(),
            received_ts: checked_ts(),
        });

        let report = validate_stage4_broker_truth_bootstrap(input(&truth));

        assert_eq!(
            report.status,
            Stage4BrokerTruthBootstrapStatus::ManualInterventionRequired
        );
        assert_eq!(
            report
                .trade_correlation_summary
                .unknown_or_orphan_target_trade_count,
            1
        );
        assert!(report.blockers.iter().any(|blocker| {
            blocker.kind == Stage4BrokerTruthReadinessBlockerKind::UnknownOrOrphanTargetTrade
        }));
    }

    #[test]
    fn stage4c_external_issue_bridge_maps_target_issue_to_blocker() {
        let truth = base_truth();
        let mut request = input(&truth);
        request
            .external_issues
            .push(Stage4BrokerTruthExternalIssue {
                kind: Stage4BrokerTruthExternalIssueKind::LocalPendingStale,
                affects_target_instrument: true,
                manual_intervention_required: true,
            });

        let report = validate_stage4_broker_truth_bootstrap(request);

        assert_eq!(
            report.status,
            Stage4BrokerTruthBootstrapStatus::ManualInterventionRequired
        );
        assert!(report.blockers.iter().any(|blocker| {
            blocker.kind == Stage4BrokerTruthReadinessBlockerKind::ExternalLocalPendingStale
        }));
    }

    #[test]
    fn stage4c_missing_broker_truth_source_returns_broker_truth_incomplete() {
        let truth = base_truth();
        let mut request = input(&truth);
        request.broker_truth_source_status = Stage4BrokerTruthSourceStatus::Missing;

        let report = validate_stage4_broker_truth_bootstrap(request);

        assert_eq!(
            report.status,
            Stage4BrokerTruthBootstrapStatus::BrokerTruthIncomplete
        );
        assert!(report.blockers.iter().any(|blocker| {
            blocker.kind == Stage4BrokerTruthReadinessBlockerKind::BrokerTruthMissing
        }));
    }

    #[test]
    fn stage4c_missing_schedule_freshness_blocks_even_when_broker_truth_is_fresh() {
        let truth = base_truth();
        let mut request = input(&truth);
        request.freshness = Stage4BrokerTruthFreshnessInput::from_broker_truth_received_ts(
            truth.received_ts,
            60_000,
        );

        let report = validate_stage4_broker_truth_bootstrap(request);

        assert_eq!(
            report.status,
            Stage4BrokerTruthBootstrapStatus::BrokerTruthStale
        );
        assert!(report.blockers.iter().any(|blocker| {
            blocker.kind == Stage4BrokerTruthReadinessBlockerKind::ScheduleStale
        }));
    }

    #[test]
    fn stage4c_raw_payload_or_live_boundary_opens_safety_status() {
        let truth = base_truth();
        let mut request = input(&truth);
        request.safety_boundary.raw_payload_exported = true;
        request.safety_boundary.runtime_live_enabled = true;

        let report = validate_stage4_broker_truth_bootstrap(request);

        assert_eq!(
            report.status,
            Stage4BrokerTruthBootstrapStatus::SafetyBoundaryOpen
        );
        assert!(report.blockers.iter().any(|blocker| {
            blocker.kind == Stage4BrokerTruthReadinessBlockerKind::RawPayloadExportAttempted
        }));
        assert!(report.blockers.iter().any(|blocker| {
            blocker.kind == Stage4BrokerTruthReadinessBlockerKind::RuntimeLiveEnabled
        }));
    }

    #[test]
    fn stage4c_instrument_identity_missing_or_ambiguous_blocks_bootstrap() {
        let mut missing = base_truth();
        missing.instruments.clear();
        let missing_report = validate_stage4_broker_truth_bootstrap(input(&missing));

        assert_eq!(
            missing_report.status,
            Stage4BrokerTruthBootstrapStatus::InstrumentMismatch
        );

        let mut ambiguous = base_truth();
        ambiguous.instruments.push(spec());
        let ambiguous_report = validate_stage4_broker_truth_bootstrap(input(&ambiguous));

        assert_eq!(
            ambiguous_report.status,
            Stage4BrokerTruthBootstrapStatus::InstrumentMismatch
        );
    }

    #[test]
    fn stage4c_unknown_target_order_status_blocks_bootstrap() {
        let mut truth = base_truth();
        truth.orders.push(target_order(
            "BROKER-ORDER-UNKNOWN",
            OrderStatus::Unknown("BROKER_NATIVE_UNKNOWN".to_string()),
        ));

        let report = validate_stage4_broker_truth_bootstrap(input(&truth));

        assert_eq!(
            report.status,
            Stage4BrokerTruthBootstrapStatus::ManualInterventionRequired
        );
        assert_eq!(report.ownership_summary.target_unknown_order_count, 1);
        assert!(report.blockers.iter().any(|blocker| {
            blocker.kind == Stage4BrokerTruthReadinessBlockerKind::UnknownOrOrphanTargetOrder
        }));
    }

    #[test]
    fn stage4c_unknown_schedule_has_specific_status() {
        let truth = base_truth();
        let mut request = input(&truth);
        request.schedule_state = BrokerMarketSessionState::Unknown;

        let report = validate_stage4_broker_truth_bootstrap(request);

        assert_eq!(
            report.status,
            Stage4BrokerTruthBootstrapStatus::UnknownSchedule
        );
        assert!(report.blockers.iter().any(|blocker| {
            blocker.kind == Stage4BrokerTruthReadinessBlockerKind::UnknownSchedule
        }));
    }

    #[test]
    fn stage4c_imports_order_lifecycle_without_creating_duplicate_truth() {
        let order = target_order("BROKER-ORDER-2", OrderStatus::PartiallyFilled);

        assert_eq!(order.lifecycle, BrokerOrderLifecycle::Active);
    }
}
