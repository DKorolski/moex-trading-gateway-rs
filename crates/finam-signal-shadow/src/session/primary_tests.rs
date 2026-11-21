use super::*;
use crate::primary::{
    AdmittedCanonicalM10, CanonicalStatus, PrimaryProcessor, WireFrame, WsPrimaryConfig,
};
use serde_json::{json, Value};
use std::path::PathBuf;

fn rows(path: &Path) -> Vec<Value> {
    std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

pub(crate) fn raw_row(label: i64) -> Value {
    json!({"timestamp":DateTime::<Utc>::from_timestamp_millis(label).unwrap().to_rfc3339(),
        "open":{"value":"2200"},"high":{"value":"2201"},"low":{"value":"2199"},
        "close":{"value":"2200.5"},"volume":{"value":"2"}})
}

pub(crate) fn wire(label: i64, received: i64) -> WireFrame {
    let text = json!({"type":"DATA","subscription_type":"BARS","subscription_key":"test-key",
        "payload":{"symbol":"IMOEXF@RTSX","bars":[raw_row(label)]}})
    .to_string();
    WireFrame::normalize(
        &text,
        1,
        "test-key",
        DateTime::from_timestamp_millis(received).unwrap(),
    )
    .unwrap()
}

pub(crate) fn dense(config: &WsPrimaryConfig, label: i64) -> PrimaryProcessor {
    let instrument = broker_core::InstrumentId {
        symbol: "IMOEXF".into(),
        venue_symbol: Some("IMOEXF@RTSX".into()),
        exchange: broker_core::Exchange::Moex,
        market: broker_core::Market::Futures,
    };
    let mut p = PrimaryProcessor::from_fixture(
        config,
        instrument,
        vec![label, label + 600000],
        1,
        label - 100,
    )
    .unwrap();
    for i in 0..=10 {
        let t = label + i * 60000;
        p.ingest(wire(t, t + 100), t + 100).unwrap();
    }
    p
}

pub(crate) fn fixture<T>(
    work: impl FnOnce(ObservedSession, WsPrimaryConfig, AdmittedClosedM1Snapshot, PathBuf, i64) -> T,
) -> T {
    let close = DateTime::parse_from_rfc3339("2026-11-03T04:10:00Z")
        .unwrap()
        .timestamp_millis();
    crate::timing::with_fixture_clock(close + 2000, || {
        let (s, c, source, path) = super::tests::primary_fixture();
        work(s, c, source, path, close - 600000)
    })
}

fn ready(p: &mut PrimaryProcessor, label: i64) -> Box<AdmittedCanonicalM10> {
    let CanonicalStatus::Ready(input) = p.prepare(label + 602000).unwrap() else {
        panic!("expected source admission")
    };
    input
}

#[test]
fn ws_source_runs_the_actual_model_once_and_commits_after_step() {
    fixture(|mut s, c, _, path, label| {
        let mut p = dense(&c, label);
        let input = ready(&mut p, label);
        let publication = s.canonical(&input).unwrap();
        assert_eq!(p.next_label_ms(), Some(label));
        assert_eq!(
            publication.source_sha256,
            input.provenance_sha256().unwrap()
        );
        p.committed(*input, publication.committed_at_ms).unwrap();
        assert_eq!(p.next_label_ms(), Some(label + 600000));
        let events = rows(&path);
        let source = events
            .iter()
            .position(|r| r["kind"] == "canonical_source")
            .unwrap();
        let model = events
            .iter()
            .position(|r| r["kind"] == "model_step")
            .unwrap();
        let commit = events
            .iter()
            .position(|r| r["kind"] == "canonical_commit")
            .unwrap();
        assert!(source < model && model < commit);
        assert_eq!(
            events.iter().filter(|r| r["kind"] == "model_step").count(),
            1
        );
        assert_eq!(events[model]["event"]["model_label_utc_ms"], label);
        assert_eq!(events[source]["event"]["receipt"]["origin"], "WS_LIVE");
        assert_eq!(events[commit]["event"]["feedback_complete"], true);
    })
}

#[test]
fn same_data_capability_cannot_repeat_actual_model_callback() {
    fixture(|mut s, c, _, path, label| {
        let mut p = dense(&c, label);
        let input = ready(&mut p, label);
        s.canonical(&input).unwrap();
        assert!(s.canonical(&input).is_err());
        assert_eq!(
            rows(&path)
                .iter()
                .filter(|r| r["kind"] == "model_step")
                .count(),
            1
        );
    })
}

#[test]
fn primary_session_refuses_legacy_snapshot_before_callback() {
    fixture(|mut s, _, source, path, _| {
        let received = source.evidence().parts[0].received_at.timestamp_millis();
        assert!(s
            .snapshot(&source, &[source.evidence().start], received)
            .is_err());
        assert!(!rows(&path).iter().any(|r| r["kind"] == "model_step"));
    })
}

#[test]
fn changed_primary_config_is_refused_before_source_or_model() {
    fixture(|mut s, mut c, _, path, label| {
        c.base.run_id = "another-run".into();
        let mut p = dense(&c, label);
        let input = ready(&mut p, label);
        assert!(s.canonical(&input).is_err());
        assert!(!rows(&path)
            .iter()
            .any(|r| r["kind"] == "canonical_source" || r["kind"] == "model_step"));
    })
}

#[test]
fn source_write_refusal_does_not_call_model_or_advance_frontier() {
    fixture(|mut s, c, _, path, label| {
        let mut p = dense(&c, label);
        let input = ready(&mut p, label);
        s.observer.fail_on_kind("canonical_source");
        assert!(matches!(s.canonical(&input), Err(SessionError::Evidence)));
        assert_eq!(p.next_label_ms(), Some(label));
        assert!(!rows(&path).iter().any(|r| r["kind"] == "model_step"));
    })
}

#[test]
fn commit_failure_is_terminal_after_one_model_step_not_retried() {
    fixture(|mut s, c, _, path, label| {
        let mut p = dense(&c, label);
        let input = ready(&mut p, label);
        s.observer.fail_on_kind("canonical_commit");
        assert!(matches!(s.canonical(&input), Err(SessionError::Evidence)));
        assert!(matches!(s.canonical(&input), Err(SessionError::Closed)));
        assert_eq!(p.next_label_ms(), Some(label));
        let events = rows(&path);
        assert_eq!(
            events.iter().filter(|r| r["kind"] == "model_step").count(),
            1
        );
        assert!(!events.iter().any(|r| r["kind"] == "canonical_commit"));
    })
}

#[test]
fn delayed_source_is_incomplete_without_retimestamping() {
    fixture(|mut s, c, _, path, label| {
        let mut p = dense(&c, label);
        let input = ready(&mut p, label);
        crate::timing::advance_fixture_clock(9000);
        assert!(s.canonical(&input).is_err());
        let events = rows(&path);
        assert!(!events.iter().any(|r| r["kind"] == "model_step"));
        assert_eq!(
            events.last().unwrap()["event"]["reason"],
            "INCOMPLETE_canonical_operation"
        );
    })
}
