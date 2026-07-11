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
        let result = admit_stage5c_paper_host(Stage5cPaperHostAdmissionInput {
            stage4_evidence: &canonical.evidence,
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
            assert_eq!(
                admission.bootstrap_snapshot(),
                canonical.evidence.applied_snapshot()
            );
            assert_eq!(
                admission.expires_at(),
                canonical.evidence.required_source_expires_at()
            );
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
fn stage5c_rejects_application_from_another_stage4_run() {
    let source = include_str!("../src/stage5c_paper_host.rs");
    assert!(source.contains("pub stage4_evidence: &'a Stage4AcceptedPaperHostEvidence"));
    assert!(!source.contains("pub stage4_application:"));
    assert!(!source.contains("pub stage4j_report:"));
}

#[test]
fn stage5c_capability_binds_exact_admitted_bootstrap_snapshot() {
    let canonical = canonical_input();
    let admission = admit_stage5c_paper_host(Stage5cPaperHostAdmissionInput {
        stage4_evidence: &canonical.evidence,
        instrument_spec: &canonical.spec,
        configured_account_id: &canonical.configured_account,
        configured_target_instrument: &canonical.configured_target,
        configured_tick_size: canonical.configured_tick_size,
        allow_live_orders: false,
    })
    .expect("fresh canonical evidence");
    assert_eq!(
        admission.bootstrap_snapshot(),
        canonical.evidence.applied_snapshot()
    );
}

#[test]
fn stage5c_capability_records_evidence_expiry() {
    let canonical = canonical_input();
    let admission = admit_stage5c_paper_host(Stage5cPaperHostAdmissionInput {
        stage4_evidence: &canonical.evidence,
        instrument_spec: &canonical.spec,
        configured_account_id: &canonical.configured_account,
        configured_target_instrument: &canonical.configured_target,
        configured_tick_size: canonical.configured_tick_size,
        allow_live_orders: false,
    })
    .expect("fresh canonical evidence");
    assert_eq!(
        admission.expires_at(),
        canonical.evidence.required_source_expires_at()
    );
    assert!(admission.issued_ts() <= admission.expires_at());
}

#[test]
fn stage5c_rejects_expired_stage4_evidence() {
    let canonical = canonical_input_at(Utc::now() - Duration::seconds(61));
    let result = admit_stage5c_paper_host(Stage5cPaperHostAdmissionInput {
        stage4_evidence: &canonical.evidence,
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
        stage4_evidence: &canonical.evidence,
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
        stage4_evidence: &canonical.evidence,
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
