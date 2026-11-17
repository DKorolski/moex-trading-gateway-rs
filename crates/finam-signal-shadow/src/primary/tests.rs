use super::cache::ExceptionReason;
use super::*;
use broker_core::{
    canonical_m1::aggregate_m1_window, event::Bar, Exchange, InstrumentId, Market,
    MarketDataSourceKind,
};
use broker_finam::sparse_m10::{
    AdmittedClosedM1Snapshot, ClosedM1RequestPlan, ClosedM1RestPartV1, ClosedM1SnapshotEvidenceV1,
    OBSERVED_M1_POLICY_V1,
};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde_json::{json, Value};

const T: i64 = 1793640000000; // fixed M10 label, no dependence on wall-clock date
const C: i64 = T + 600000;
mod consistent_sources;
#[cfg(feature = "models-bridge")]
mod models_bridge;
mod resilience;
fn at(ms: i64) -> DateTime<Utc> {
    DateTime::from_timestamp_millis(ms).unwrap()
}
fn instrument() -> InstrumentId {
    InstrumentId {
        symbol: "IMOEXF".into(),
        venue_symbol: Some("IMOEXF@RTSX".into()),
        exchange: Exchange::Moex,
        market: Market::Futures,
    }
}
fn row(ms: i64) -> Value {
    json!({"timestamp":at(ms).to_rfc3339(), "open":{"value":"2200"}, "high":{"value":"2201"},
           "low":{"value":"2199"}, "close":{"value":"2200.5"}, "volume":{"value":"2"}})
}
fn text(rows: Vec<Value>) -> String {
    json!({"type":"DATA", "subscription_type":"BARS", "subscription_key":"test-key",
        "payload":{"symbol":"IMOEXF@RTSX", "bars":rows}})
    .to_string()
}
fn frame(rows: Vec<Value>, received: i64) -> WireFrame {
    WireFrame::normalize(&text(rows), 1, "test-key", at(received)).unwrap()
}
fn processor(subscribed: i64) -> PrimaryProcessor {
    let config = WsPrimaryConfig::parse(&serde_json::to_vec(&config()).unwrap()).unwrap();
    PrimaryProcessor::from_fixture(&config, instrument(), vec![T, C], 1, subscribed).unwrap()
}

#[test]
fn receipt_and_clock_allowance_bind_actual_configuration() {
    let mut value = config();
    value["base"]["clock_uncertainty_ms"] = json!(50);
    let config = WsPrimaryConfig::parse(&serde_json::to_vec(&value).unwrap()).unwrap();
    let mut p =
        PrimaryProcessor::from_fixture(&config, instrument(), vec![T, C], 1, T - 100).unwrap();
    for i in 0..=10 {
        p.ingest(
            frame(vec![row(T + i * 60000)], T + i * 60000 + 100),
            T + i * 60000 + 100,
        )
        .unwrap();
    }
    let input = ready(&mut p, C + 1100);
    assert_eq!(input.receipt().config_sha256, config.fingerprint().unwrap());
    p.committed(*input, C + 9950).unwrap();
}
fn dense() -> PrimaryProcessor {
    let mut p = processor(T - 100);
    for i in 0..=10 {
        let ms = T + i * 60000;
        p.ingest(frame(vec![row(ms)], ms + 100), ms + 100).unwrap();
    }
    p
}
fn ready(p: &mut PrimaryProcessor, now: i64) -> Box<AdmittedCanonicalM10> {
    let CanonicalStatus::Ready(input) = p.prepare(now).unwrap() else {
        panic!("expected admission")
    };
    input
}
fn reference(rows: Vec<Value>, start: i64, end: i64, now: i64) -> AdmittedClosedM1Snapshot {
    let raw = json!({"symbol":"IMOEXF@RTSX", "bars":rows}).to_string();
    AdmittedClosedM1Snapshot::admit(
        ClosedM1SnapshotEvidenceV1 {
            policy: OBSERVED_M1_POLICY_V1.into(),
            start: at(start),
            end: at(end),
            parts: vec![ClosedM1RestPartV1 {
                method: "GET".into(),
                endpoint: "https://api.finam.ru/v1/instruments/IMOEXF@RTSX/bars".into(),
                symbol: "IMOEXF@RTSX".into(),
                timeframe: "TIME_FRAME_M1".into(),
                start: at(start),
                end: at(end),
                requested_at: at(now),
                received_at: at(now),
                status: 200,
                transport_complete: true,
                declared_body_bytes: Some(raw.len() as u64),
                response_sha256: crate::seed::digest(raw.as_bytes()),
                raw_body: raw,
            }],
        },
        &ClosedM1RequestPlan::new(at(start), at(end), vec![at(end)]).unwrap(),
        at(now),
    )
    .unwrap()
}

#[test]
fn snapshot_presence_is_not_bool_default() {
    assert_eq!(
        SnapshotPresence::parse(&json!({})).unwrap(),
        SnapshotPresence::Missing
    );
    for key in ["is_data_snapshot", "isDataSnapshot"] {
        assert_eq!(
            SnapshotPresence::parse(&json!({key:false})).unwrap(),
            SnapshotPresence::ExplicitFalse
        );
        assert_eq!(
            SnapshotPresence::parse(&json!({key:true})).unwrap(),
            SnapshotPresence::ExplicitTrue
        );
        for value in [Value::Null, json!("false"), json!(0), json!({})] {
            assert_eq!(
                SnapshotPresence::parse(&json!({key:value})),
                Err(PrimaryError::SnapshotFlag)
            );
        }
    }
    assert_eq!(
        SnapshotPresence::parse(&json!({"is_data_snapshot":true,"isDataSnapshot":true})),
        Err(PrimaryError::SnapshotFlag)
    );
}

#[test]
fn wire_string_and_object_payloads_are_equivalent() {
    let source = text(vec![row(T)]);
    let a = WireFrame::normalize(&source, 1, "test-key", at(T + 100)).unwrap();
    let mut other: Value = serde_json::from_str(&source).unwrap();
    other["payload"] = json!(other["payload"].to_string());
    let b = WireFrame::normalize(&other.to_string(), 1, "test-key", at(T + 100)).unwrap();
    assert_eq!(a.rows[0].bar, b.rows[0].bar);
    assert!(!a.rows[0].bar.is_final);
}

#[test]
fn wire_identity_and_row_location_are_strict() {
    for (field, value) in [
        ("subscription_type", json!("QUOTES")),
        ("subscription_key", json!("wrong")),
    ] {
        let mut data: Value = serde_json::from_str(&text(vec![row(T)])).unwrap();
        data[field] = value;
        assert!(WireFrame::normalize(&data.to_string(), 1, "test-key", at(T + 100)).is_err());
    }
    let mut data: Value = serde_json::from_str(&text(vec![row(T)])).unwrap();
    data["payload"]["bar"] = json!([]);
    assert!(WireFrame::normalize(&data.to_string(), 1, "test-key", at(T + 100)).is_err());
}

#[test]
fn timestamp_future_subsecond_and_unaligned_are_rejected() {
    for ms in [T + 60000, T + 1, T + 1000] {
        assert!(WireFrame::normalize(&text(vec![row(ms)]), 1, "test-key", at(T + 100)).is_err());
    }
}

#[test]
fn bad_tick_negative_volume_and_inconsistent_ohlc_are_rejected() {
    for (field, value) in [
        ("open", "2200.1"),
        ("volume", "-1"),
        ("volume", "0.5"),
        ("high", "2198"),
    ] {
        let mut bad = row(T);
        bad[field] = json!({"value":value});
        assert!(WireFrame::normalize(&text(vec![bad]), 1, "test-key", at(T + 100)).is_err());
    }
}

#[test]
fn dense_math_is_source_neutral_and_label_is_open_not_close() {
    let mut p = dense();
    let input = ready(&mut p, C + 1100);
    assert_eq!(input.receipt().minute_bitmap, 1023);
    assert_eq!(input.model_bar().close_time_utc, T / 1000);
    assert_eq!(input.model_bar().volume, 20.0);
    assert_eq!(input.receipt().boundary_received_at_ms, Some(C + 100));
    assert_eq!(input.receipt().last_component_received_at_ms, T + 540100);
    assert_eq!(input.receipt().origin, "WS_LIVE");
    assert_eq!(
        input.receipt().canonical_digest,
        crate::input::canonical_digest(input.model_bar()).unwrap()
    );
    assert_eq!(p.next_label_ms(), Some(T)); // source Ready is NOT model commit
}

#[test]
fn no_model_authority_from_aggregation_or_is_final() {
    let rows: Vec<_> = (0..10)
        .map(|i| {
            frame(vec![row(T + i * 60000)], T + i * 60000 + 1)
                .rows
                .remove(0)
                .bar
        })
        .collect();
    let values = aggregate_m1_window(&instrument(), at(T), &rows).unwrap();
    assert!(!values.provisional_bar.is_final);
    assert_eq!(
        values.provisional_bar.source_kind,
        MarketDataSourceKind::Unknown
    );
}

#[test]
fn grace_edges_999_1000_1001_are_exact() {
    for (delta, expected) in [(999, false), (1000, true), (1001, true)] {
        let mut p = dense();
        let value = p.prepare(C + 100 + delta).unwrap();
        assert_eq!(matches!(value, CanonicalStatus::Ready(_)), expected);
    }
}

#[test]
fn duplicates_do_not_sum_volume_or_refresh_data() {
    let mut p = processor(T - 100);
    p.ingest(frame(vec![row(T)], T + 100), T + 100).unwrap();
    p.ingest(frame(vec![row(T)], T + 200), T + 200).unwrap();
    assert_eq!(p.counters().valid_updates, 1);
    assert_eq!(p.counters().duplicates, 1);
    assert_eq!(p.cached_m1(), 1);
}

#[test]
fn m1_update_replaces_volume_not_adds_it() {
    let mut p = processor(T - 100);
    p.ingest(frame(vec![row(T)], T + 100), T + 100).unwrap();
    let mut update = row(T);
    update["volume"] = json!({"value":"5"});
    p.ingest(frame(vec![update], T + 200), T + 200).unwrap();
    for i in 1..=10 {
        p.ingest(
            frame(vec![row(T + i * 60000)], T + i * 60000 + 100),
            T + i * 60000 + 100,
        )
        .unwrap();
    }
    assert_eq!(ready(&mut p, C + 1100).model_bar().volume, 23.0);
}

#[test]
fn regression_and_changed_open_hold_without_partial_effects() {
    for (field, value) in [
        ("open", "2200.5"),
        ("high", "2200.5"),
        ("low", "2199.5"),
        ("volume", "1"),
    ] {
        let mut p = processor(T - 100);
        p.ingest(frame(vec![row(T)], T + 100), T + 100).unwrap();
        let mut update = row(T);
        update[field] = json!({"value":value});
        assert_eq!(
            p.ingest(frame(vec![update], T + 200), T + 200),
            Err(PrimaryError::CanonicalConflict)
        );
        assert_eq!(p.counters().valid_updates, 1);
        assert_eq!(p.state(), PipelineState::Hold);
    }
}

#[test]
fn sparse_does_not_invent_zero_minutes_and_waits_before_get() {
    let mut p = processor(T - 100);
    p.ingest(frame(vec![row(T)], T + 100), T + 100).unwrap();
    p.ingest(frame(vec![row(C)], C + 100), C + 100).unwrap();
    assert!(matches!(
        p.prepare(C + 2999).unwrap(),
        CanonicalStatus::PendingProof {
            reason: ExceptionReason::SparseUnproven,
            may_request: false,
            ..
        }
    ));
    assert!(matches!(
        p.prepare(C + 3000).unwrap(),
        CanonicalStatus::PendingProof {
            reason: ExceptionReason::SparseUnproven,
            may_request: true,
            ..
        }
    ));
}

#[test]
fn no_boundary_data_means_no_ws_seal() {
    let mut p = processor(T - 100);
    for i in 0..10 {
        p.ingest(
            frame(vec![row(T + i * 60000)], T + i * 60000 + 100),
            T + i * 60000 + 100,
        )
        .unwrap();
    }
    assert!(matches!(
        p.prepare(C + 3000).unwrap(),
        CanonicalStatus::PendingProof {
            reason: ExceptionReason::BoundaryUnproven,
            ..
        }
    ));
}

#[test]
fn startup_partial_is_explicit_not_ws_live() {
    let mut p = processor(T + 100);
    for i in 0..=10 {
        p.ingest(
            frame(vec![row(T + i * 60000)], T + i * 60000 + 200),
            T + i * 60000 + 200,
        )
        .unwrap();
    }
    assert!(matches!(
        p.prepare(C + 3000).unwrap(),
        CanonicalStatus::PendingProof {
            reason: ExceptionReason::StartupPartial,
            ..
        }
    ));
}

#[test]
fn marked_history_cannot_establish_readiness() {
    let mut p = processor(C);
    let mut history = row(T);
    history["isDataSnapshot"] = json!(true);
    p.ingest(frame(vec![history], C + 100), C + 100).unwrap();
    assert_eq!(p.state(), PipelineState::Bootstrap);
    assert_eq!(p.cached_m1(), 0);
    assert_eq!(p.counters().discarded_history, 1);
}

#[test]
fn missing_stale_requires_exact_admitted_reference() {
    let mut p = processor(C);
    assert_eq!(
        p.ingest(frame(vec![row(T)], C + 100), C + 100),
        Err(PrimaryError::ReferenceUnproven)
    );
    assert_eq!(p.state(), PipelineState::Bootstrap);
    p.add_reference(&reference(vec![row(T)], T, C, C + 100))
        .unwrap();
    p.ingest(frame(vec![row(T)], C + 101), C + 101).unwrap();
    assert_eq!(p.counters().discarded_history, 1);
    assert_eq!(p.state(), PipelineState::Bootstrap);
}

#[test]
fn stale_explicit_false_is_never_reference_discarded() {
    let mut p = processor(C);
    p.add_reference(&reference(vec![row(T)], T, C, C + 100))
        .unwrap();
    let mut stale = row(T);
    stale["is_data_snapshot"] = json!(false);
    assert_eq!(
        p.ingest(frame(vec![stale], C + 100), C + 100),
        Err(PrimaryError::Timestamp)
    );
}

#[test]
fn reference_checks_m1_not_only_m10_or_close() {
    let mut p = processor(C);
    p.add_reference(&reference(vec![row(T)], T, C, C + 100))
        .unwrap();
    let mut mismatch = row(T);
    mismatch["volume"] = json!({"value":"3"});
    assert_eq!(
        p.ingest(frame(vec![mismatch], C + 100), C + 100),
        Err(PrimaryError::ReferenceConflict)
    );
    assert_eq!(p.cached_m1(), 0);
}

#[test]
fn mixed_frame_failure_is_atomic() {
    let mut p = processor(C);
    let mut history = row(T);
    history["isDataSnapshot"] = json!(true);
    assert_eq!(
        p.ingest(
            frame(vec![history, row(C), row(T + 60000)], C + 100),
            C + 100
        ),
        Err(PrimaryError::ReferenceUnproven)
    );
    assert_eq!(p.cached_m1(), 0);
    assert_eq!(p.counters().valid_updates, 0);
    assert_eq!(p.counters().discarded_history, 0);
    assert_eq!(p.state(), PipelineState::Bootstrap);
}

#[test]
fn bootstrap_limits_charge_entire_mixed_frame() {
    let mut p = processor(C);
    let mut history = row(T);
    history["isDataSnapshot"] = json!(true);
    let mut f = frame(vec![history, row(C)], C + 100);
    f.wire_bytes = 256 * 1024 + 1;
    assert_eq!(p.ingest(f, C + 100), Err(PrimaryError::BootstrapBudget));
    assert_eq!(p.cached_m1(), 0);
}

#[test]
fn after_first_fresh_data_bootstrap_cannot_reopen() {
    let mut p = processor(T - 100);
    p.ingest(frame(vec![row(T)], T + 100), T + 100).unwrap();
    let mut history = row(T);
    history["isDataSnapshot"] = json!(true);
    assert_eq!(
        p.ingest(frame(vec![history], T + 200), T + 200),
        Err(PrimaryError::UnexpectedHistory)
    );
}

#[test]
fn queued_old_generation_cannot_mutate_new_state() {
    let mut p = processor(T - 100);
    p.begin_generation(2, T).unwrap();
    assert_eq!(
        p.ingest(frame(vec![row(T)], T + 100), T + 100),
        Err(PrimaryError::Generation)
    );
    assert_eq!(p.state(), PipelineState::Recovering);
    assert_eq!(p.cached_m1(), 0);
}

#[test]
fn fresh_new_generation_does_not_clear_canonical_gap() {
    let mut p = processor(T - 100);
    p.begin_generation(2, T).unwrap();
    let mut f = frame(vec![row(T)], T + 100);
    f.generation = 2;
    p.ingest(f, T + 100).unwrap();
    assert!(p.gap_unresolved());
    assert_eq!(p.state(), PipelineState::Recovering);
}

#[test]
fn ingress_staleness_closes_continuity_not_silent_drop() {
    let mut p = processor(T - 100);
    assert_eq!(
        p.ingest(frame(vec![row(T)], T + 100), T + 2101),
        Err(PrimaryError::Continuity)
    );
    assert!(p.gap_unresolved());
    assert_eq!(p.cached_m1(), 0);
}

#[test]
fn seal_does_not_advance_frontier_and_cannot_be_reissued() {
    let mut p = dense();
    let input = ready(&mut p, C + 1100);
    assert!(matches!(
        p.prepare(C + 1101).unwrap(),
        CanonicalStatus::AwaitingCommit
    ));
    assert_eq!(p.counters().committed, 0);
    p.committed(*input, C + 1200).unwrap();
    assert_eq!(p.counters().committed, 1);
    assert_eq!(p.next_label_ms(), Some(C));
}

#[test]
fn capability_cannot_be_committed_to_another_processor() {
    let mut a = dense();
    let input = ready(&mut a, C + 1100);
    let mut b = dense();
    let _other = ready(&mut b, C + 1100);
    assert_eq!(b.committed(*input, C + 1200), Err(PrimaryError::Continuity));
    assert_eq!(b.next_label_ms(), Some(T));
}

#[test]
fn revision_after_seal_or_commit_stops_admission() {
    for committed in [false, true] {
        let mut p = dense();
        let input = ready(&mut p, C + 1100);
        if committed {
            p.committed(*input, C + 1200).unwrap();
        }
        let mut revised = row(T + 540000);
        revised["volume"] = json!({"value":"3"});
        assert_eq!(
            p.ingest(frame(vec![revised], C + 1300), C + 1300),
            Err(PrimaryError::CanonicalConflict)
        );
    }
}

#[test]
fn late_commit_is_incomplete_not_a_second_callback() {
    let mut p = dense();
    let input = ready(&mut p, C + 1100);
    assert_eq!(p.committed(*input, C + 9901), Err(PrimaryError::Continuity));
    assert_eq!(p.next_label_ms(), Some(T));
    assert_eq!(p.state(), PipelineState::Hold);
}

#[test]
fn utc_regression_holds_the_processor() {
    let mut p = dense();
    assert!(matches!(p.prepare(C + 99), Err(PrimaryError::Clock)));
    assert_eq!(p.state(), PipelineState::Hold);
}

#[test]
fn policy_is_explicit_pinned_and_audit_can_be_disabled() {
    let policy = PrimaryPolicy::default();
    policy.validate().unwrap();
    let mut disabled = policy.clone();
    disabled.audit_every_commits = None;
    disabled.validate().unwrap();
    assert_ne!(
        policy.fingerprint().unwrap(),
        disabled.fingerprint().unwrap()
    );
    let mut relaxed = policy;
    relaxed.close_grace_ms = 999;
    assert_eq!(relaxed.validate(), Err(PrimaryError::Configuration));
}

#[test]
fn sparse_pure_math_preserves_nominal_bounds_and_source_is_unknown() {
    let rows: Vec<Bar> = [1, 8]
        .into_iter()
        .map(|i| {
            frame(vec![row(T + i * 60000)], T + i * 60000 + 1)
                .rows
                .remove(0)
                .bar
        })
        .collect();
    let values = aggregate_m1_window(&instrument(), at(T), &rows).unwrap();
    assert_eq!(values.minute_bitmap, (1 << 1) | (1 << 8));
    assert_eq!(values.provisional_bar.open_ts, at(T));
    assert_eq!(values.provisional_bar.close_ts, at(C));
    assert_eq!(values.provisional_bar.volume, Decimal::from(4));
    assert!(!values.provisional_bar.is_final);
    assert!(aggregate_m1_window(&instrument(), at(T), &[]).is_err());
}

pub(crate) fn config() -> Value {
    json!({"schema_version":2,"market_data_mode":"ws_primary_v1","source_policy":WS_PRIMARY_POLICY,
        "primary":PrimaryPolicy::default(),"base":{
        "schema_version":1,"reference":"alor_freeze_2026_09_27","restart_mode":"new_segment",
        "model_profile":"imoexf-baseline07-bo-only-no-riskgate","model_sha256":"a".repeat(64),
        "reference_sha256":"b".repeat(64),"feedback_policy_sha256":crate::feedback::candle_policy_fingerprint(),
        "instrument_mapping_sha256":"c".repeat(64),"calendar_sha256":"d".repeat(64),"seed_sha256":"e".repeat(64),
        "instrument":"IMOEXF@RTSX","run_id":"primary-test","segment_id":"s1","previous_segment_id":null,
        "warmup_session_dates":["2026-09-22","2026-09-23","2026-09-24","2026-09-25"],
        "warmup_range_start_utc_ms":1,"warmup_range_end_utc_ms":2,
        "observation_start_utc_ms":T,"run_deadline_utc_ms":C+3600000,"lmax_ms":10000,
        "max_quote_age_ms":10000,"clock_uncertainty_ms":100,"ws_silence_budget_ms":60000,
        "reconnect_budget_ms":5000,"stop_timeout_ms":15000,"max_input_bytes":1048576,
        "max_cache_bars":4096,"max_output_bytes":16777216}})
}

#[test]
fn new_configuration_is_rejected_by_legacy_parser_not_partly_executed() {
    let bytes = serde_json::to_vec(&config()).unwrap();
    assert!(WsPrimaryConfig::parse(&bytes).is_ok());
    assert!(crate::ValidatedConfig::parse(&bytes).is_err());
    let base = serde_json::to_vec(&config()["base"]).unwrap();
    assert!(crate::ValidatedConfig::parse(&base).is_ok());
    assert!(WsPrimaryConfig::parse(&base).is_err());
}

#[test]
fn mode_and_policy_are_mandatory_and_do_not_change_model_identity() {
    for field in ["market_data_mode", "source_policy", "primary"] {
        let mut value = config();
        value.as_object_mut().unwrap().remove(field);
        assert!(WsPrimaryConfig::parse(&serde_json::to_vec(&value).unwrap()).is_err());
    }
    let original = config();
    let a = WsPrimaryConfig::parse(&serde_json::to_vec(&original).unwrap()).unwrap();
    let mut disabled = original.clone();
    disabled["primary"]["audit_every_commits"] = Value::Null;
    let b = WsPrimaryConfig::parse(&serde_json::to_vec(&disabled).unwrap()).unwrap();
    assert_ne!(a.fingerprint().unwrap(), b.fingerprint().unwrap());
    assert_ne!(
        a.semantic_fingerprint().unwrap(),
        b.semantic_fingerprint().unwrap()
    );
    assert_eq!(a.base.model_sha256, b.base.model_sha256);
    assert_eq!(a.base.feedback_policy_sha256, b.base.feedback_policy_sha256);
    assert_eq!(a.base.reference_sha256, b.base.reference_sha256);
}

#[test]
fn new_config_semantics_remove_only_attribution() {
    let a = WsPrimaryConfig::parse(&serde_json::to_vec(&config()).unwrap()).unwrap();
    let mut b = a.clone();
    b.base.run_id = "second-run".into();
    b.base.segment_id = "s2".into();
    assert_ne!(a.fingerprint().unwrap(), b.fingerprint().unwrap());
    assert_eq!(
        a.semantic_fingerprint().unwrap(),
        b.semantic_fingerprint().unwrap()
    );
}

#[test]
fn new_config_cannot_enable_quotes_relax_latency_or_add_unknown_fields() {
    for (key, value) in [
        ("feedback_policy_sha256", json!("a".repeat(64))),
        ("lmax_ms", json!(11000)),
        ("clock_uncertainty_ms", json!(101)),
    ] {
        let mut c = config();
        c["base"][key] = value;
        assert!(WsPrimaryConfig::parse(&serde_json::to_vec(&c).unwrap()).is_err());
    }
    let mut c = config();
    c["primary"]["allow_orders"] = json!(true);
    assert!(WsPrimaryConfig::parse(&serde_json::to_vec(&c).unwrap()).is_err());
}

#[test]
fn bootstrap_row_boundary_is_cumulative() {
    for count in [511, 512, 513] {
        let mut p = processor(C);
        let mut r = row(T);
        r["isDataSnapshot"] = json!(true);
        for _ in 0..count {
            let outcome = p.ingest(frame(vec![r.clone()], C + 100), C + 100);
            if p.counters().discarded_history == 512 && count == 513 {
                if outcome.is_err() {
                    assert_eq!(outcome, Err(PrimaryError::BootstrapBudget));
                    break;
                }
            } else {
                outcome.unwrap();
            }
        }
        assert_eq!(p.counters().discarded_history as usize, count.min(512));
        assert_eq!(p.state() == PipelineState::Hold, count == 513);
    }
}

#[test]
fn raw_decimal_spellings_have_the_same_canonical_numeric_digest() {
    let mut a = dense();
    let first = ready(&mut a, C + 1100);
    let mut p = processor(T - 100);
    for i in 0..=10 {
        let mut r = row(T + i * 60000);
        r["volume"] = json!({"value":"2.0000"});
        r["open"] = json!({"value":"2200.000"});
        p.ingest(frame(vec![r], T + i * 60000 + 100), T + i * 60000 + 100)
            .unwrap();
    }
    let second = ready(&mut p, C + 1100);
    assert_eq!(
        first.receipt().canonical_digest,
        second.receipt().canonical_digest
    );
}

#[test]
fn held_processor_cannot_be_reopened_by_reconnect_or_gap_signal() {
    let mut p = dense();
    assert!(p.prepare(T).is_err());
    p.break_continuity();
    assert_eq!(p.state(), PipelineState::Hold);
    assert_eq!(p.begin_generation(2, C + 2000), Err(PrimaryError::Held));
    assert!(matches!(p.prepare(C + 2000), Err(PrimaryError::Held)));
}

#[test]
fn committed_exact_overlap_is_deduplicated_without_new_readiness() {
    let mut p = dense();
    let input = ready(&mut p, C + 1100);
    p.committed(*input, C + 1200).unwrap();
    let before = p.counters().valid_updates;
    p.ingest(frame(vec![row(T)], C + 1300), C + 1300).unwrap();
    assert_eq!(p.next_label_ms(), Some(C));
    assert_eq!(p.counters().committed, 1);
    assert_eq!(p.counters().valid_updates, before);
    assert_eq!(p.counters().duplicates, 1);
}

#[test]
fn cache_limit_is_4096_not_unbounded() {
    let mut p = processor(T - 100);
    let template = frame(vec![row(T)], T + 100).rows.remove(0).bar;
    for i in 0..4097 {
        let timestamp = T + i * 60000;
        let mut bar = template.clone();
        bar.open_ts = at(timestamp);
        bar.close_ts = at(timestamp + 60000);
        let wire = WireFrame {
            rows: vec![WireRow {
                bar,
                presence: SnapshotPresence::Missing,
            }],
            generation: 1,
            received_at_ms: timestamp + 100,
            wire_bytes: 300,
            sha256: "a".repeat(64),
            received_mono: tokio::time::Instant::now(),
            owner: None,
        };
        let result = p.ingest(wire, timestamp + 100);
        if i < 4096 {
            result.unwrap();
        } else {
            assert_eq!(result, Err(PrimaryError::CacheBudget));
        }
    }
    assert_eq!(p.cached_m1(), 4096);
    assert_eq!(p.state(), PipelineState::Hold);
}

#[test]
fn last_acceptable_publication_includes_clock_allowance() {
    for (latency, accepted) in [(9899, true), (9900, true), (9901, false)] {
        let mut p = dense();
        let input = ready(&mut p, C + 1100);
        assert_eq!(p.committed(*input, C + latency).is_ok(), accepted);
    }
}

#[test]
fn missing_closed_tail_needs_reference_even_when_recent_not_stale() {
    let mut p = processor(T + 60000);
    assert_eq!(
        p.ingest(frame(vec![row(T)], T + 60100), T + 60100),
        Err(PrimaryError::ReferenceUnproven)
    );
    assert_eq!(p.state(), PipelineState::Bootstrap);
    assert_eq!(p.cached_m1(), 0);
    assert_eq!(p.counters().valid_updates, 0);
    // An explicit live marker still follows the ordinary freshness predicate.
    let mut current = row(T);
    current["isDataSnapshot"] = json!(false);
    p.ingest(frame(vec![current], T + 60200), T + 60200)
        .unwrap();
    assert_eq!(p.state(), PipelineState::LiveReady);
}
