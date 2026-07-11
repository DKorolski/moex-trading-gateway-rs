use broker_core::{
    BrokerAccountId, BrokerInstrumentSpec, BrokerKind, BrokerSymbol, Exchange, InstrumentId,
    InstrumentMapEntry, InternalSymbol, Market, RuntimeHostBootstrapSnapshot,
    Stage4AdoptionDisposition, Stage4BootstrapEvidenceRedaction, Stage4BootstrapEvidenceReport,
    Stage4BootstrapEvidenceReportStatus, Stage4BootstrapEvidenceSourceSection,
    Stage4BrokerTruthBootstrapStatus, Stage4BrokerTruthFreshnessSection,
    Stage4BrokerTruthFreshnessStatus, Stage4BrokerTruthSafetyBoundary,
    Stage4BrokerTruthSourceStatus, Stage4DirtyStartDisposition, Stage4DirtyStartPolicyStatus,
    Stage4RuntimeBootstrapApplicationDecision, Stage4RuntimeBootstrapApplicationStatus,
    Stage4RuntimeBootstrapIntegrationEvent, Stage4RuntimeBootstrapIntegrationStatus,
    Stage4RuntimeLifecycleOrderingStatus, STAGE4_BOOTSTRAP_EVIDENCE_REPORT_SCHEMA_VERSION,
    STAGE4_RUNTIME_BOOTSTRAP_APPLICATION_SCHEMA_VERSION,
};
use chrono::{TimeZone, Utc};
use rust_decimal::Decimal;
use serde::Deserialize;
use strategy_runtime_core::{
    admit_stage5c_paper_host, Stage5cPaperHostAdmissionError, Stage5cPaperHostAdmissionInput,
    STAGE5C_PAPER_HOST_ADMISSION_SCHEMA_VERSION,
};

#[derive(Debug, Deserialize)]
struct Fixture {
    schema_version: u16,
    stage: String,
    status: String,
    cases: Vec<FixtureCase>,
    safety_boundary: FixtureSafetyBoundary,
}

#[derive(Debug, Deserialize)]
struct FixtureCase {
    name: String,
    mutation: String,
    expected: String,
}

#[derive(Debug, Deserialize)]
struct FixtureSafetyBoundary {
    runtime_host_attached: bool,
    strategy_callbacks_invoked: bool,
    intent_sink_attached: bool,
    command_consumer_attached: bool,
    live_send_enabled: bool,
    real_post_delete_added: bool,
}

struct CanonicalInput {
    report: Stage4BootstrapEvidenceReport,
    application: Stage4RuntimeBootstrapApplicationDecision,
    spec: BrokerInstrumentSpec,
    configured_account: BrokerAccountId,
    configured_target: InstrumentId,
    configured_tick_size: f64,
    allow_live_orders: bool,
}

fn target() -> InstrumentId {
    InstrumentId {
        symbol: "IMOEXF".to_string(),
        venue_symbol: Some("IMOEXF@RTSX".to_string()),
        exchange: Exchange::Moex,
        market: Market::Futures,
    }
}

fn fixture() -> Fixture {
    serde_json::from_str(include_str!(
        "../../../tests/fixtures/stage5/stage5c_paper_host_admission.json"
    ))
    .expect("Stage 5C admission fixture")
}

fn canonical_input() -> CanonicalInput {
    let checked_ts = Utc
        .with_ymd_and_hms(2026, 7, 11, 9, 0, 0)
        .single()
        .expect("valid timestamp");
    let account = BrokerAccountId::new("ACC_TEST_0001");
    let target = target();
    let snapshot = RuntimeHostBootstrapSnapshot {
        account_id: account.clone(),
        instrument: target.clone(),
        target_position_qty: Decimal::ZERO,
        target_open_positions: Vec::new(),
        target_active_orders: Vec::new(),
        account_active_orders_count: 0,
        target_is_flat: true,
        received_ts: checked_ts,
    };
    let application = Stage4RuntimeBootstrapApplicationDecision {
        schema_version: STAGE4_RUNTIME_BOOTSTRAP_APPLICATION_SCHEMA_VERSION,
        checked_ts,
        status: Stage4RuntimeBootstrapApplicationStatus::Applied,
        source_bootstrap_status: Stage4BrokerTruthBootstrapStatus::BootstrapReady,
        applied_snapshot: Some(snapshot),
        blockers: Vec::new(),
        blocker_count: 0,
        broker_truth_loaded_before_runtime_state: true,
        restored_runtime_state_present: false,
        restored_runtime_state_accepted_after_broker_truth: false,
        restored_runtime_overrode_broker_truth: false,
        target_position_qty: Decimal::ZERO,
        target_is_flat: true,
        target_active_order_count: 0,
        account_active_order_count: 0,
        dirty_start_disposition: Stage4DirtyStartDisposition::CleanBootstrap,
        adoption: Stage4AdoptionDisposition::default(),
        no_live_authorization: true,
    };
    let events = vec![
        Stage4RuntimeBootstrapIntegrationEvent::NotifyBootstrapSnapshot,
        Stage4RuntimeBootstrapIntegrationEvent::NotifyRuntimeStateRestored,
        Stage4RuntimeBootstrapIntegrationEvent::WarmupHistory,
        Stage4RuntimeBootstrapIntegrationEvent::RecoverPendingStreams,
    ];
    let report = Stage4BootstrapEvidenceReport {
        schema_version: STAGE4_BOOTSTRAP_EVIDENCE_REPORT_SCHEMA_VERSION,
        checked_ts,
        status: Stage4BootstrapEvidenceReportStatus::Accepted,
        target_instrument: target.clone(),
        broker_truth_source_status: Stage4BrokerTruthSourceStatus::Present,
        source_sections: [
            Stage4BrokerTruthFreshnessSection::Positions,
            Stage4BrokerTruthFreshnessSection::Orders,
            Stage4BrokerTruthFreshnessSection::Trades,
            Stage4BrokerTruthFreshnessSection::Cash,
            Stage4BrokerTruthFreshnessSection::Instruments,
            Stage4BrokerTruthFreshnessSection::Schedule,
        ]
        .into_iter()
        .map(|section| Stage4BootstrapEvidenceSourceSection {
            section,
            source_status: Stage4BrokerTruthSourceStatus::Present,
            freshness_status: Stage4BrokerTruthFreshnessStatus::Fresh,
            required_for_bootstrap: true,
            blocks_bootstrap: false,
            age_ms: Some(0),
            max_age_ms: 60_000,
        })
        .collect(),
        stage4c_status: Stage4BrokerTruthBootstrapStatus::BootstrapReady,
        stage4c_blocker_kinds: Vec::new(),
        stage4e_status: Stage4RuntimeBootstrapApplicationStatus::Applied,
        stage4e_blocker_kinds: Vec::new(),
        stage4f_status: Stage4DirtyStartPolicyStatus::Accepted,
        stage4f_blocker_kinds: Vec::new(),
        stage4g_status: Stage4RuntimeLifecycleOrderingStatus::Accepted,
        stage4g_blocker_kinds: Vec::new(),
        stage4g_lifecycle_issues: Vec::new(),
        stage4h_status: Stage4RuntimeBootstrapIntegrationStatus::Accepted,
        stage4h_blocker_kinds: Vec::new(),
        reason_chain: Vec::new(),
        redaction: Stage4BootstrapEvidenceRedaction::closed(),
        safety_boundary: Stage4BrokerTruthSafetyBoundary::closed(),
        target_is_flat: true,
        target_active_order_count: 0,
        account_active_order_count: 0,
        manual_intervention_required: false,
        no_live_authorization: true,
        runtime_events_emitted: true,
        mock_runtime_events: events,
        blocker_count: 0,
    };
    let spec = BrokerInstrumentSpec {
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
            schedule_id: "MOEX_FORTS".to_string(),
            expiration_date: None,
            is_tradable: true,
        },
        broker_asset_id: Some("ASSET_TEST_IMOEXF".to_string()),
        board: Some("RTSX".to_string()),
        long_initial_margin: None,
        short_initial_margin: None,
    };
    CanonicalInput {
        report,
        application,
        spec,
        configured_account: account,
        configured_target: target,
        configured_tick_size: 0.5,
        allow_live_orders: false,
    }
}

fn apply_mutation(input: &mut CanonicalInput, mutation: &str) {
    match mutation {
        "none" => {}
        "report_blocked" => input.report.status = Stage4BootstrapEvidenceReportStatus::Blocked,
        "report_blocker" => input.report.blocker_count = 1,
        "report_live_enabled" => input.report.safety_boundary.runtime_live_enabled = true,
        "application_blocked" => {
            input.application.status = Stage4RuntimeBootstrapApplicationStatus::Blocked;
        }
        "application_snapshot_missing" => input.application.applied_snapshot = None,
        "configured_target_mismatch" => {
            input.configured_target.venue_symbol = Some("OTHER@RTSX".to_string());
        }
        "account_scope_mismatch" => {
            input.configured_account = BrokerAccountId::new("ACC_TEST_OTHER");
        }
        "instrument_spec_mismatch" => {
            input.spec.instrument.broker_symbol = BrokerSymbol("OTHER@RTSX".to_string());
        }
        "tick_size_mismatch" => input.configured_tick_size = 1.0,
        "live_orders_requested" => input.allow_live_orders = true,
        other => panic!("unknown fixture mutation {other}"),
    }
}

fn expected_error(name: &str) -> Stage5cPaperHostAdmissionError {
    match name {
        "stage4_report_not_accepted" => Stage5cPaperHostAdmissionError::Stage4ReportNotAccepted,
        "stage4_evidence_chain_inconsistent" => {
            Stage5cPaperHostAdmissionError::Stage4EvidenceChainInconsistent
        }
        "stage4_safety_boundary_open" => Stage5cPaperHostAdmissionError::Stage4SafetyBoundaryOpen,
        "stage4_application_not_applied" => {
            Stage5cPaperHostAdmissionError::Stage4ApplicationNotApplied
        }
        "stage4_application_snapshot_missing" => {
            Stage5cPaperHostAdmissionError::Stage4ApplicationSnapshotMissing
        }
        "target_instrument_mismatch" => Stage5cPaperHostAdmissionError::TargetInstrumentMismatch,
        "account_scope_mismatch" => Stage5cPaperHostAdmissionError::AccountScopeMismatch,
        "instrument_spec_mismatch" => Stage5cPaperHostAdmissionError::InstrumentSpecMismatch,
        "tick_size_mismatch" => Stage5cPaperHostAdmissionError::TickSizeMismatch,
        "live_orders_requested" => Stage5cPaperHostAdmissionError::LiveOrdersRequested,
        other => panic!("unknown expected result {other}"),
    }
}

#[test]
fn stage5c_fixture_matrix_is_fail_closed() {
    let fixture = fixture();
    assert_eq!(fixture.schema_version, 1);
    assert_eq!(fixture.stage, "Stage5C-a");
    assert_eq!(
        fixture.status,
        "paper_mock_admission_only_no_callbacks_no_send"
    );
    for case in fixture.cases {
        let mut canonical = canonical_input();
        apply_mutation(&mut canonical, &case.mutation);
        let result = admit_stage5c_paper_host(Stage5cPaperHostAdmissionInput {
            stage4j_report: &canonical.report,
            stage4_application: &canonical.application,
            instrument_spec: &canonical.spec,
            configured_account_id: &canonical.configured_account,
            configured_target_instrument: &canonical.configured_target,
            configured_tick_size: canonical.configured_tick_size,
            allow_live_orders: canonical.allow_live_orders,
        });
        if case.expected == "accepted" {
            let admission = result
                .unwrap_or_else(|error| panic!("{} unexpectedly blocked: {error}", case.name));
            assert_eq!(
                admission.schema_version(),
                STAGE5C_PAPER_HOST_ADMISSION_SCHEMA_VERSION
            );
            assert_eq!(admission.account_id(), &canonical.configured_account);
            assert_eq!(admission.target_instrument(), &canonical.configured_target);
            assert_eq!(admission.tick_size(), 0.5);
            assert!(admission.is_paper_only());
            assert!(!admission.runtime_host_attached());
            assert!(!admission.intent_sink_attached());
        } else {
            let error = match result {
                Ok(_) => panic!("{} unexpectedly admitted", case.name),
                Err(error) => error,
            };
            assert_eq!(
                error,
                expected_error(&case.expected),
                "fixture case {}",
                case.name
            );
        }
    }
}

#[test]
fn stage5c_fixture_keeps_all_execution_surfaces_closed() {
    let safety = fixture().safety_boundary;
    assert!(!safety.runtime_host_attached);
    assert!(!safety.strategy_callbacks_invoked);
    assert!(!safety.intent_sink_attached);
    assert!(!safety.command_consumer_attached);
    assert!(!safety.live_send_enabled);
    assert!(!safety.real_post_delete_added);
}
