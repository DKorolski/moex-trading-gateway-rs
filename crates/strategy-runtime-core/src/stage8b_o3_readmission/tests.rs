use super::*;
use chrono::{Duration, TimeZone};
use rust_decimal::Decimal;

fn source(now: DateTime<Utc>) -> Stage8bO3PaperSource {
    let date = (now - Duration::minutes(20) + Duration::hours(3)).date_naive();
    let start = date.and_hms_opt(0, 0, 0).unwrap().and_utc() - Duration::hours(3);
    let fixture = BrokerTruthSnapshot {
        account_id: BrokerAccountId::new("ACC_TEST_0001"),
        orders: vec![],
        positions: vec![],
        trades: vec![],
        cash: None,
        instruments: vec![BrokerInstrumentSpec {
            instrument: InstrumentMapEntry {
                internal_symbol: InternalSymbol("IMOEXF".into()),
                broker: BrokerKind::Finam,
                broker_symbol: BrokerSymbol("IMOEXF@RTSX".into()),
                exchange: Exchange::Moex,
                market: Market::Futures,
                price_step: Decimal::new(5, 1),
                qty_step: Decimal::ONE,
                lot_size: Decimal::ONE,
                min_qty: Decimal::ONE,
                step_value: Decimal::new(5, 0),
                currency: "RUB".into(),
                schedule_id: "RTSX".into(),
                expiration_date: None,
                is_tradable: true,
            },
            broker_asset_id: Some("SYNTHETIC-O3".into()),
            board: Some("RTSX".into()),
            long_initial_margin: Some(Money::new(5000, 0)),
            short_initial_margin: Some(Money::new(5000, 0)),
        }],
        received_ts: now - Duration::days(1),
    };
    Stage8bO3PaperSource::new(
        Stage8bO3PaperScope {
            phase_id: "isolated-test".into(),
            phase_sha256: "11".repeat(32),
            config_sha256: "22".repeat(32),
            plan_sha256: "33".repeat(32),
            fixture_sha256: digest(&encode(&fixture).unwrap()),
            operational_identity_sha256: "44".repeat(32),
            runtime_config_fingerprint_sha256: "55".repeat(32),
            strategy_id: "hybrid_imoexf".into(),
            not_before: now - Duration::seconds(1),
            deadline: now + Duration::seconds(1799),
            catchup_coverage: Stage8bO3Coverage {
                source_mode: "config-bound-explicit-session-windows-v1".into(),
                sessions: vec![Stage8bO3CoverageSession {
                    session_date: date.to_string(),
                    windows: vec![Stage8bO3CoverageWindow {
                        first_close_time_utc: start.timestamp(),
                        last_close_time_utc: (start + Duration::days(1) - Duration::minutes(10))
                            .timestamp(),
                    }],
                }],
            },
        },
        fixture,
    )
    .unwrap()
}

fn binding(now: DateTime<Utc>) -> Stage8bO3DecisionBinding {
    let close = now.timestamp() / 600 * 600;
    Stage8bO3DecisionBinding {
        source_seal_generation: 1,
        source_seal_commitment_sha256: "66".repeat(32),
        source_checkpoint_sha256: "77".repeat(32),
        source_frontier_sha256: "88".repeat(32),
        m10_redis_id: format!("{}-0", close * 1000),
        m10_semantic_id_sha256: "99".repeat(32),
        m10_payload_sha256: "aa".repeat(32),
        observed_source_sha256: "bb".repeat(32),
        prior_model_label: close - 1200,
        candidate_model_label: close - 600,
    }
}

#[test]
fn o3_real_production_constructors_and_exact_retained_reconstruction() {
    let now = Utc::now();
    let s = source(now);
    let before = encode(&s.fixture).unwrap();
    let p = s.issue(binding(now)).unwrap();
    assert!(p.admission.is_paper_only());
    assert_eq!(encode(&s.fixture).unwrap(), before);
    assert_eq!(p.admission.checked_ts(), p.proof.observed_at);
    assert_eq!(p.admission.issued_ts(), p.proof.issued_at);
    let r = Stage8bO3PaperPermit::restore_pending(p.retained_bytes(), s.scope()).unwrap();
    assert_eq!(p.proof, r.proof);
    assert!(p.admission == r.admission);
    assert_eq!(p.sha256(), r.sha256());
}

#[test]
fn o3_expiry_is_inclusive_and_recorded_replay_does_not_extend_phase() {
    let now = Utc.timestamp_opt(1791536400, 0).unwrap();
    let s = source(now);
    let mut p = s.issue_at(binding(now), now, now, false).unwrap();
    let expiry = now + Duration::milliseconds(60_000);
    assert_eq!(p.proof.expires_at, expiry);
    assert!(p.execution_time_at(expiry).is_ok());
    assert!(p
        .execution_time_at(expiry + Duration::milliseconds(1))
        .is_err());
    assert!(p
        .execution_time_at(now - Duration::milliseconds(1))
        .is_err());
    p.replay = true; // private instrumentation; production needs journal match.
    assert_eq!(
        p.execution_time_at(expiry + Duration::seconds(1)).unwrap(),
        now
    );
    assert!(p
        .execution_time_at(s.scope.deadline + Duration::milliseconds(1))
        .is_err());
    assert_eq!(p.proof.expires_at, expiry);
}

#[test]
fn o3_exact_proof_rejects_corruption_foreign_scope_and_timestamp_forgery() {
    let now = Utc::now();
    let s = source(now);
    let p = s.issue(binding(now)).unwrap();
    for field in [
        "phase_sha256",
        "config_sha256",
        "plan_sha256",
        "operational_identity_sha256",
        "runtime_config_fingerprint_sha256",
    ] {
        let mut v = serde_json::to_value(s.scope()).unwrap();
        v[field] = serde_json::json!("cc".repeat(32));
        let scope = serde_json::from_value(v).unwrap();
        assert!(
            Stage8bO3PaperPermit::restore_pending(p.retained_bytes(), &scope).is_err(),
            "{field}"
        );
    }
    let mut corrupt = p.retained_bytes().to_vec();
    corrupt.push(b' ');
    assert!(Stage8bO3PaperPermit::reconstruct(&corrupt, s.scope()).is_err());
    let mut proof = p.proof.clone();
    proof.expires_at += Duration::seconds(1);
    assert!(Stage8bO3PaperPermit::reconstruct(&encode(&proof).unwrap(), s.scope()).is_err());
    proof = p.proof.clone();
    proof.issued_at = proof.observed_at - Duration::seconds(1);
    assert!(Stage8bO3PaperPermit::reconstruct(&encode(&proof).unwrap(), s.scope()).is_err());
    let mut wrong = s.fixture.clone();
    wrong.instruments[0].broker_asset_id = Some("REAL-O2".into());
    let mut scope = s.scope.clone();
    scope.fixture_sha256 = digest(&encode(&wrong).unwrap());
    assert!(Stage8bO3PaperSource::new(scope, wrong).is_err());
}
