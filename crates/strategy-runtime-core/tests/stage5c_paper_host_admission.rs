use broker_core::{
    build_stage4_accepted_paper_host_evidence, validate_stage4_broker_truth_bootstrap,
    BrokerAccountId, BrokerInstrumentSpec, BrokerKind, BrokerMarketSessionState, BrokerSymbol,
    BrokerTruthSnapshot, Exchange, InstrumentId, InstrumentMapEntry, InternalSymbol, Market,
    Stage4AcceptedPaperHostEvidence, Stage4AdoptionDisposition,
    Stage4BootstrapEvidenceSourceStatusSection, Stage4BrokerTruthBootstrapInput,
    Stage4BrokerTruthFreshnessInput, Stage4BrokerTruthFreshnessSection,
    Stage4BrokerTruthSafetyBoundary, Stage4BrokerTruthSourceStatus,
};
use chrono::{DateTime, Duration, Utc};
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

#[derive(Debug, Deserialize)]
struct BootstrapFixture {
    schema_version: u16,
    stage: String,
    status: String,
    stage4_evidence_consumed_into_admission: bool,
    admission_consumed_by_value: bool,
    strategy_consumed_by_value: bool,
    strategy_id_bound_in_admission: bool,
    strategy_symbol_binding_required: bool,
    strategy_tick_size_binding_required: bool,
    expiry_rechecked_at_notification: bool,
    snapshot_source: String,
    active_orders_without_ownership_mapping: String,
    source_callback: String,
    source_callback_intent_count: usize,
    lifecycle: BootstrapLifecycle,
    execution_boundary: BootstrapExecutionBoundary,
}

#[derive(Debug, Deserialize)]
struct BootstrapLifecycle {
    type_state: String,
    bootstrap_notification_emitted: bool,
    runtime_state_restored: bool,
    warmup_started: bool,
    pending_recovery_started: bool,
    semantic_bar_enabled: bool,
    intent_sink_attached: bool,
}

#[derive(Debug, Deserialize)]
struct BootstrapExecutionBoundary {
    runtime_live_enabled: bool,
    command_consumer_attached: bool,
    live_send_enabled: bool,
    real_post_delete_added: bool,
    stop_sltp_bracket_execution_enabled: bool,
}

struct CanonicalInput {
    evidence: Stage4AcceptedPaperHostEvidence,
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
            schedule_id: "MOEX_FORTS".to_string(),
            expiration_date: None,
            is_tradable: true,
        },
        broker_asset_id: Some("ASSET_TEST_IMOEXF".to_string()),
        board: Some("RTSX".to_string()),
        long_initial_margin: None,
        short_initial_margin: None,
    }
}

fn source_sections() -> Vec<Stage4BootstrapEvidenceSourceStatusSection> {
    [
        Stage4BrokerTruthFreshnessSection::Positions,
        Stage4BrokerTruthFreshnessSection::Orders,
        Stage4BrokerTruthFreshnessSection::Trades,
        Stage4BrokerTruthFreshnessSection::Cash,
        Stage4BrokerTruthFreshnessSection::Instruments,
        Stage4BrokerTruthFreshnessSection::Schedule,
    ]
    .into_iter()
    .map(|section| Stage4BootstrapEvidenceSourceStatusSection {
        section,
        source_status: Stage4BrokerTruthSourceStatus::Present,
        required_for_bootstrap: true,
    })
    .collect()
}

fn fixture() -> Fixture {
    serde_json::from_str(include_str!(
        "../../../tests/fixtures/stage5/stage5c_paper_host_admission.json"
    ))
    .expect("Stage 5C admission fixture")
}

fn bootstrap_fixture() -> BootstrapFixture {
    serde_json::from_str(include_str!(
        "../../../tests/fixtures/stage5/stage5cb_bootstrap_notification.json"
    ))
    .expect("Stage 5C-b bootstrap fixture")
}

fn canonical_input_at(checked_ts: DateTime<Utc>) -> CanonicalInput {
    let account = BrokerAccountId::new("ACC_TEST_0001");
    let target = target();
    let spec = spec();
    let truth = BrokerTruthSnapshot {
        account_id: account.clone(),
        orders: Vec::new(),
        positions: Vec::new(),
        cash: None,
        trades: Vec::new(),
        instruments: vec![spec.clone()],
        received_ts: checked_ts,
    };
    let validated = validate_stage4_broker_truth_bootstrap(Stage4BrokerTruthBootstrapInput {
        broker_truth: &truth,
        broker_truth_source_status: Stage4BrokerTruthSourceStatus::Present,
        target_instrument: target.clone(),
        restored_runtime_state: None,
        freshness: Stage4BrokerTruthFreshnessInput::synthetic_all_sections_fresh_for_tests(
            checked_ts, 60_000,
        ),
        schedule_state: BrokerMarketSessionState::Open,
        adoption: Stage4AdoptionDisposition::default(),
        external_issues: Vec::new(),
        safety_boundary: Stage4BrokerTruthSafetyBoundary::closed(),
        checked_ts,
    });
    let evidence = build_stage4_accepted_paper_host_evidence(&validated, &source_sections())
        .expect("canonical Stage 4 chain accepted");
    CanonicalInput {
        evidence,
        spec,
        configured_account: account,
        configured_target: target,
        configured_tick_size: 0.5,
        allow_live_orders: false,
    }
}

fn canonical_input() -> CanonicalInput {
    canonical_input_at(Utc::now())
}

fn apply_mutation(input: &mut CanonicalInput, mutation: &str) {
    match mutation {
        "none" => {}
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
        "expired_evidence" => *input = canonical_input_at(Utc::now() - Duration::seconds(61)),
        "future_evidence" => *input = canonical_input_at(Utc::now() + Duration::seconds(60)),
        other => panic!("unknown fixture mutation {other}"),
    }
}

fn expected_error(name: &str) -> Stage5cPaperHostAdmissionError {
    match name {
        "target_instrument_mismatch" => Stage5cPaperHostAdmissionError::TargetInstrumentMismatch,
        "account_scope_mismatch" => Stage5cPaperHostAdmissionError::AccountScopeMismatch,
        "instrument_spec_mismatch" => Stage5cPaperHostAdmissionError::InstrumentSpecMismatch,
        "tick_size_mismatch" => Stage5cPaperHostAdmissionError::TickSizeMismatch,
        "live_orders_requested" => Stage5cPaperHostAdmissionError::LiveOrdersRequested,
        "evidence_expired" => Stage5cPaperHostAdmissionError::EvidenceExpired,
        "evidence_checked_in_future" => Stage5cPaperHostAdmissionError::EvidenceCheckedInFuture,
        other => panic!("unknown expected result {other}"),
    }
}

#[test]
fn stage5c_accepted_case_is_built_through_canonical_stage4_chain() {
    let fixture = fixture();
    assert_eq!(fixture.schema_version, 2);
    assert_eq!(fixture.stage, "Stage5C-a");
    assert_eq!(
        fixture.status,
        "canonical_time_bound_paper_admission_no_callbacks_no_send"
    );
    for case in fixture.cases {
        let mut canonical = canonical_input();
        apply_mutation(&mut canonical, &case.mutation);
        let expected_snapshot = canonical.evidence.applied_snapshot().clone();
        let expected_expiry = canonical.evidence.required_source_expires_at();
        let result = admit_stage5c_paper_host(Stage5cPaperHostAdmissionInput {
            stage4_evidence: canonical.evidence,
            strategy_id: "hybrid_imoexf".to_string(),
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
            assert_eq!(admission.bootstrap_snapshot(), &expected_snapshot);
            assert_eq!(admission.expires_at(), expected_expiry);
            assert_eq!(admission.strategy_id(), "hybrid_imoexf");
            assert!(admission.issued_ts() >= admission.checked_ts());
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

#[test]
fn stage5cb_fixture_freezes_one_shot_bootstrap_only_boundary() {
    let fixture = bootstrap_fixture();
    assert_eq!(fixture.schema_version, 2);
    assert_eq!(fixture.stage, "Stage5C-b");
    assert_eq!(
        fixture.status,
        "linear_type_state_bootstrap_notification_no_send"
    );
    assert!(fixture.stage4_evidence_consumed_into_admission);
    assert!(fixture.admission_consumed_by_value);
    assert!(fixture.strategy_consumed_by_value);
    assert!(fixture.strategy_id_bound_in_admission);
    assert!(fixture.strategy_symbol_binding_required);
    assert!(fixture.strategy_tick_size_binding_required);
    assert!(fixture.expiry_rechecked_at_notification);
    assert_eq!(
        fixture.snapshot_source,
        "Stage5cPaperHostAdmission.bootstrap_snapshot"
    );
    assert_eq!(fixture.active_orders_without_ownership_mapping, "blocked");
    assert_eq!(fixture.source_callback, "on_bootstrap_snapshot");
    assert_eq!(fixture.source_callback_intent_count, 0);
    assert_eq!(
        fixture.lifecycle.type_state,
        "Stage5cBootstrappedPaperStrategy"
    );
    assert!(fixture.lifecycle.bootstrap_notification_emitted);
    assert!(!fixture.lifecycle.runtime_state_restored);
    assert!(!fixture.lifecycle.warmup_started);
    assert!(!fixture.lifecycle.pending_recovery_started);
    assert!(!fixture.lifecycle.semantic_bar_enabled);
    assert!(!fixture.lifecycle.intent_sink_attached);
    assert!(!fixture.execution_boundary.runtime_live_enabled);
    assert!(!fixture.execution_boundary.command_consumer_attached);
    assert!(!fixture.execution_boundary.live_send_enabled);
    assert!(!fixture.execution_boundary.real_post_delete_added);
    assert!(
        !fixture
            .execution_boundary
            .stop_sltp_bracket_execution_enabled
    );
}

#[test]
fn stage5c_rejects_application_from_another_stage4_run() {
    let source = include_str!("../src/stage5c_paper_host.rs");
    assert!(source.contains("pub stage4_evidence: Stage4AcceptedPaperHostEvidence"));
    assert!(!source.contains("pub stage4_application:"));
    assert!(!source.contains("pub stage4j_report:"));
}

#[test]
fn stage5c_stage4_evidence_is_consumed_into_admission() {
    let source = include_str!("../src/stage5c_paper_host.rs");
    assert!(source.contains("pub stage4_evidence: Stage4AcceptedPaperHostEvidence"));
    let broker_source = include_str!("../../broker-core/src/stage4_bootstrap.rs");
    assert!(broker_source.contains("pub struct Stage4AcceptedPaperHostEvidence"));
    assert!(!broker_source
        .contains("#[derive(Clone, PartialEq)]\npub struct Stage4AcceptedPaperHostEvidence"));
}

#[test]
fn stage5cb_cannot_issue_two_admissions_from_one_evidence() {
    let source = include_str!("../src/stage5c_paper_host.rs");
    assert!(source.contains("stage4_evidence: evidence"));
    assert!(source.contains("Stage5cPaperHostAdmissionInput"));
    assert!(source.contains("```compile_fail"));
}

#[test]
fn stage5cb_lifecycle_cannot_continue_on_different_strategy_binding() {
    let source = include_str!("../src/stage5c_paper_host.rs");
    assert!(source.contains("pub struct Stage5cBootstrappedPaperStrategy"));
    let strategy_field = ["strategy: HybridIntraday", "RuntimeStrategy"].concat();
    assert!(source.contains(&strategy_field));
    assert!(!source.contains("pub fn into_parts("));
}

#[test]
fn stage5cc_fixture_freezes_restore_only_boundary() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/stage5/stage5cc_runtime_state_restore.json"
    ))
    .expect("Stage 5C-c fixture");
    assert_eq!(fixture["stage"], "Stage5C-c");
    assert_eq!(fixture["validation_before_mutation"], true);
    assert_eq!(fixture["broker_truth_position_may_be_overwritten"], false);
    assert_eq!(fixture["lifecycle"]["runtime_state_restored"], true);
    assert_eq!(fixture["lifecycle"]["warmup_started"], false);
    assert_eq!(fixture["lifecycle"]["semantic_bar_enabled"], false);
    assert_eq!(fixture["execution"]["endpoint_calls_allowed"], false);
    assert_eq!(fixture["execution"]["real_post_delete_added"], false);
}

#[test]
fn stage5c_capability_binds_exact_admitted_bootstrap_snapshot() {
    let canonical = canonical_input();
    let expected_snapshot = canonical.evidence.applied_snapshot().clone();
    let admission = admit_stage5c_paper_host(Stage5cPaperHostAdmissionInput {
        stage4_evidence: canonical.evidence,
        strategy_id: "hybrid_imoexf".to_string(),
        instrument_spec: &canonical.spec,
        configured_account_id: &canonical.configured_account,
        configured_target_instrument: &canonical.configured_target,
        configured_tick_size: canonical.configured_tick_size,
        allow_live_orders: false,
    })
    .expect("fresh canonical evidence");
    assert_eq!(admission.bootstrap_snapshot(), &expected_snapshot);
}

#[test]
fn stage5c_capability_records_evidence_expiry() {
    let canonical = canonical_input();
    let expected_expiry = canonical.evidence.required_source_expires_at();
    let admission = admit_stage5c_paper_host(Stage5cPaperHostAdmissionInput {
        stage4_evidence: canonical.evidence,
        strategy_id: "hybrid_imoexf".to_string(),
        instrument_spec: &canonical.spec,
        configured_account_id: &canonical.configured_account,
        configured_target_instrument: &canonical.configured_target,
        configured_tick_size: canonical.configured_tick_size,
        allow_live_orders: false,
    })
    .expect("fresh canonical evidence");
    assert_eq!(admission.expires_at(), expected_expiry);
    assert!(admission.issued_ts() <= admission.expires_at());
}

#[test]
fn stage5c_rejects_expired_stage4_evidence() {
    let canonical = canonical_input_at(Utc::now() - Duration::seconds(61));
    let result = admit_stage5c_paper_host(Stage5cPaperHostAdmissionInput {
        stage4_evidence: canonical.evidence,
        strategy_id: "hybrid_imoexf".to_string(),
        instrument_spec: &canonical.spec,
        configured_account_id: &canonical.configured_account,
        configured_target_instrument: &canonical.configured_target,
        configured_tick_size: canonical.configured_tick_size,
        allow_live_orders: false,
    });
    assert!(matches!(
        result,
        Err(Stage5cPaperHostAdmissionError::EvidenceExpired)
    ));
}

#[test]
fn stage5c_rejects_report_checked_in_the_future() {
    let canonical = canonical_input_at(Utc::now() + Duration::seconds(60));
    let result = admit_stage5c_paper_host(Stage5cPaperHostAdmissionInput {
        stage4_evidence: canonical.evidence,
        strategy_id: "hybrid_imoexf".to_string(),
        instrument_spec: &canonical.spec,
        configured_account_id: &canonical.configured_account,
        configured_target_instrument: &canonical.configured_target,
        configured_tick_size: canonical.configured_tick_size,
        allow_live_orders: false,
    });
    assert!(matches!(
        result,
        Err(Stage5cPaperHostAdmissionError::EvidenceCheckedInFuture)
    ));
}

#[test]
fn stage5c_effective_source_age_crossing_max_age_blocks_admission() {
    let canonical = canonical_input_at(Utc::now() - Duration::seconds(120));
    assert!(canonical.evidence.required_source_expires_at() < Utc::now());
    let result = admit_stage5c_paper_host(Stage5cPaperHostAdmissionInput {
        stage4_evidence: canonical.evidence,
        strategy_id: "hybrid_imoexf".to_string(),
        instrument_spec: &canonical.spec,
        configured_account_id: &canonical.configured_account,
        configured_target_instrument: &canonical.configured_target,
        configured_tick_size: canonical.configured_tick_size,
        allow_live_orders: false,
    });
    assert!(matches!(
        result,
        Err(Stage5cPaperHostAdmissionError::EvidenceExpired)
    ));
}
