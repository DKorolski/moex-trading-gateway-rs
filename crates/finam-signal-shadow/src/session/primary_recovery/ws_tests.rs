//! Synthetic price path, real existing BO strategy and close-fill feedback.
use super::*;
use crate::primary::{CanonicalStatus, DeliveryPolicy, PrimaryProcessor};
use serde_json::{json, Value};

fn run(want_exit: bool, pending: bool) {
    run_timed(want_exit, pending, 0, false);
}
fn run_timed(want_exit: bool, pending: bool, delay: u64, stop: bool) {
    let (mut model, bar) = super::tail_tests::before_action(want_exit);
    let label = bar.close_time_utc * 1000;
    let close = label + 600000;
    if pending {
        assert!(model.on_bar(bar.clone(), close + 17).unwrap().is_some());
        assert!(model.pending().is_some());
    }
    crate::timing::with_fixture_clock(label - 100, || {
        let (mut session, config, _, path) = crate::session::tests::primary_fixture_delivery(
            label,
            false,
            DeliveryPolicy::ObservedBoWsHistoricalRecoveryV8,
        );
        session.model = model;
        let before = session.model.observation();
        let mut p = PrimaryProcessor::from_fixture(
            &config,
            bar.instrument.clone(),
            vec![label, close, close + 600000],
            1,
            label - 100,
        )
        .unwrap();
        let subscription = crate::socket::primary::fixture_subscription(1, label - 100);
        p.bind_subscription(&subscription, label - 100).unwrap();
        for (i, row) in super::tail_tests::rows(&bar).into_iter().enumerate() {
            let received = label + i as i64 * 60000 + 100;
            let text = json!({"type":"DATA","subscriptionType":"BARS","subscriptionKey":"test-key",
                "payload":{"symbol":"IMOEXF@RTSX","bars":[row]}})
            .to_string();
            let mut frame = crate::primary::WireFrame::normalize(
                &text,
                1,
                "test-key",
                DateTime::<Utc>::from_timestamp_millis(received).unwrap(),
            )
            .unwrap();
            frame.owner = Some(subscription.owner.clone());
            p.ingest(frame, received).unwrap();
        }
        let mut frame = crate::session::primary_tests::wire(close, close + 1364);
        frame.owner = Some(subscription.owner.clone());
        p.ingest(frame, close + 1364).unwrap();
        assert!(matches!(
            p.prepare(close + 1904).unwrap(),
            CanonicalStatus::PendingProof { .. }
        ));
        p.relatch_data_gap();
        crate::timing::advance_fixture_clock((close + 2364 - crate::timing::now_ms()) as u64);
        let a = p
            .prepare_ws_recovery(1, close + 46904, 1, close + 2364)
            .unwrap()
            .unwrap();
        crate::timing::advance_fixture_clock(delay);
        if stop {
            session.stop.stop();
        }
        let result = session.ws_recovery(&a.input);
        let rows: Vec<Value> = std::fs::read_to_string(&path)
            .unwrap()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        if pending || delay > 10000 || stop {
            assert!(result.is_err());
            assert_eq!(session.model.observation(), before);
            assert!(!rows
                .iter()
                .any(|r| r["kind"] == "canonical_source" || r["kind"] == "model_step"));
        } else {
            let publication = result.unwrap();
            p.recovered_ws(a.input, publication.committed_at_ms)
                .unwrap();
            assert!(session.model.pending().is_none());
            assert_eq!(session.model.position_lots() == 0, want_exit);
            let steps: Vec<_> = rows.iter().filter(|r| r["kind"] == "model_step").collect();
            assert_eq!(steps.len(), 1);
            assert_eq!(steps[0]["event"]["mode"], "BACKFILL_REPLAY");
            assert!(!steps[0]["event"]["decision"]["action"].is_null());
            assert_eq!(steps[0]["event"]["before"], before);
            assert_eq!(
                rows.iter()
                    .filter(|r| r["kind"] == "hypothetical_candle_feedback")
                    .count(),
                1
            );
            assert_eq!(
                rows.iter()
                    .filter(|r| r["kind"] == "canonical_commit")
                    .count(),
                1
            );
        }
        let _ = std::fs::remove_file(path);
    });
}
#[test]
fn v8_ws_historical_flat_entry_feedback_is_not_suppressed() {
    run(false, false);
}
#[test]
fn v8_ws_historical_open_position_exit_feedback_is_not_suppressed() {
    run(true, false);
}
#[test]
fn v8_ws_historical_pending_mutation_refuses_callback_without_changing_book() {
    run(false, true);
}
#[test]
fn v8_r1_session_publication_budget_boundary_and_stop_do_not_mutate_a_late_book() {
    for delay in [9999, 10000, 10001] {
        run_timed(false, false, delay, false);
    }
    run_timed(false, false, 1, true);
}
