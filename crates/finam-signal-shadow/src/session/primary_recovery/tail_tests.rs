//! Real BO model/book; reconstructed and positive REST rows are SYNTHETIC.
use super::*;
use crate::primary::{DeliveryPolicy, PrimaryProcessor, RecoveryOutcome, RecoveryRequestScope};
use broker_core::HybridRuntimeBarEvent;
use broker_finam::sparse_m10::{
    ClosedM1RequestPlan, ClosedM1RestPartV1, ClosedM1SnapshotEvidenceV1, OBSERVED_M1_POLICY_V1,
};
use serde_json::{json, Value};

fn before_action(want_exit: bool) -> (crate::model::ShadowModel, HybridRuntimeBarEvent) {
    let mut before = crate::model::candle_tests::empty_model();
    let mut probe = crate::model::candle_tests::empty_model();
    let instrument = crate::session::primary_tests::wire(1793678400000, 1793678400100).rows[0]
        .bar
        .instrument
        .clone();
    let raw =
        include_bytes!("../../../../../fixtures/stage8b-p1f-parity/imoexf_raw_10m_msk_utc.csv");
    for record in csv::Reader::from_reader(raw.as_slice()).records() {
        let r = record.unwrap();
        let n = |i| r.get(i).unwrap().parse().unwrap();
        let bar = HybridRuntimeBarEvent {
            instrument: instrument.clone(),
            close_time_utc: DateTime::parse_from_str(&r[1], "%Y-%m-%d %H:%M:%S%:z")
                .unwrap()
                .timestamp(),
            open: n(3),
            high: n(4),
            low: n(5),
            close: n(6),
            volume: n(7),
            origin: HybridRuntimeBarOrigin::Live,
            is_final: true,
            timeframe_sec: 600,
        };
        let now = (bar.close_time_utc + 600) * 1000 + 17;
        let request = probe.on_bar(bar.clone(), now).unwrap();
        if request
            .as_ref()
            .is_some_and(|r| matches!(r.action, crate::feedback::Action::Exit) == want_exit)
        {
            return (before, bar);
        }
        let before_request = before.on_bar(bar, now).unwrap();
        assert_eq!(before_request, request);
        if let Some(request) = request {
            probe
                .on_candle_close(&request, now, now, &"a".repeat(64))
                .unwrap();
            before
                .on_candle_close(&request, now, now, &"a".repeat(64))
                .unwrap();
        }
    }
    panic!("frozen fixture must cover the requested BO action")
}
fn rows(bar: &HybridRuntimeBarEvent) -> Vec<Value> {
    (0..10).map(|i|json!({"timestamp":DateTime::<Utc>::from_timestamp_millis(bar.close_time_utc*1000+i*60000).unwrap().to_rfc3339(),
        "open":{"value":bar.open.to_string()},"high":{"value":bar.high.to_string()},"low":{"value":bar.low.to_string()},"close":{"value":bar.close.to_string()},"volume":{"value":"2"}})).collect()
}
fn source(
    plan: &ClosedM1RequestPlan,
    rows: Vec<Value>,
    requested: i64,
    received: i64,
) -> AdmittedClosedM1Snapshot {
    let body = json!({"symbol":"IMOEXF@RTSX","bars":rows}).to_string();
    let at = |t| DateTime::<Utc>::from_timestamp_millis(t).unwrap();
    AdmittedClosedM1Snapshot::admit(
        ClosedM1SnapshotEvidenceV1 {
            policy: OBSERVED_M1_POLICY_V1.into(),
            start: plan.start(),
            end: plan.end(),
            parts: vec![ClosedM1RestPartV1 {
                method: "GET".into(),
                endpoint: "https://api.finam.ru/v1/instruments/IMOEXF@RTSX/bars".into(),
                symbol: "IMOEXF@RTSX".into(),
                timeframe: "TIME_FRAME_M1".into(),
                start: plan.start(),
                end: plan.end(),
                requested_at: at(requested),
                received_at: at(received),
                status: 200,
                transport_complete: true,
                declared_body_bytes: Some(body.len() as u64),
                response_sha256: crate::seed::digest(body.as_bytes()),
                raw_body: body,
            }],
        },
        plan,
        at(received),
    )
    .unwrap()
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Expected {
    Entry,
    Exit,
    Hold,
}

fn run(model: crate::model::ShadowModel, bar: HybridRuntimeBarEvent, expected: Expected) {
    let label = bar.close_time_utc * 1000;
    let close = label + 600000;
    let now = close + 100;
    crate::timing::with_fixture_clock(now, || {
        let (mut session, config, _, path) = crate::session::tests::primary_fixture_delivery(
            label,
            false,
            DeliveryPolicy::ObservedBoRestTailRecoveryV6,
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
        // Actual known slots are synthetic normalized WS rows for this model test.
        for (i, row) in rows(&bar).into_iter().enumerate() {
            let received = label + i as i64 * 60000 + 100;
            let text=json!({"type":"DATA","subscriptionType":"BARS","subscriptionKey":"test-key","payload":{"symbol":"IMOEXF@RTSX","bars":[row]}}).to_string();
            let frame = crate::primary::WireFrame::normalize(
                &text,
                1,
                "test-key",
                DateTime::<Utc>::from_timestamp_millis(received).unwrap(),
            )
            .unwrap();
            p.ingest(frame, received).unwrap();
        }
        p.begin_generation(2, now).unwrap();
        let mut scope = RecoveryRequestScope {
            episode: 1,
            ordinal: 1,
            deadline_ms: now + 45000,
        };
        let first = p.issue_recovery_request(close, now, scope, None).unwrap();
        let incomplete = source(
            first.request(),
            rows(&bar).into_iter().take(7).collect(),
            now,
            now,
        );
        assert!(matches!(
            p.prepare_recovery_request(&first, &incomplete, now),
            Ok(RecoveryOutcome::DataNotReady(_))
        ));
        assert_eq!(before, session.model.observation());
        assert_eq!(p.counters().recovery_committed, 0);
        crate::timing::advance_fixture_clock(6000);
        let renewed = crate::timing::now_ms();
        scope.ordinal = 2;
        let next = p
            .issue_recovery_request(close, renewed, scope, Some(&first))
            .unwrap();
        let complete = source(next.request(), rows(&bar), renewed, renewed);
        let RecoveryOutcome::Ready(batch) = p
            .prepare_recovery_request(&next, &complete, renewed)
            .unwrap()
        else {
            panic!("ready")
        };
        assert_eq!(batch.steps(), 1);
        for input in batch.inputs {
            let publication = session.recovery(&input, &complete).unwrap();
            p.recovered(input, publication.committed_at_ms).unwrap();
        }
        let after = session.model.observation();
        match expected {
            Expected::Entry => assert_ne!(session.model.position_lots(), 0),
            Expected::Exit => assert_eq!(session.model.position_lots(), 0),
            Expected::Hold => assert_eq!(after["last_position_qty"], before["last_position_qty"]),
        }
        let mut frame = crate::session::primary_tests::wire(close, renewed + 1);
        frame.generation = 2;
        crate::timing::advance_fixture_clock(1);
        p.ingest(frame, renewed + 1).unwrap();
        assert!(p
            .finish_recovery(&batch.completion, renewed + 1)
            .unwrap()
            .is_some());
        // Resume LIVE_OBSERVED via the normal startup-partial source choice,
        // using the SAME model/book rather than importing a position.
        let mut following = bar.clone();
        following.close_time_utc += 600;
        following.open = bar.close;
        following.high = bar.close;
        following.low = bar.close;
        following.close = bar.close;
        let live = close + 601100;
        crate::timing::advance_fixture_clock((live - crate::timing::now_ms()) as u64);
        let at = |t| DateTime::<Utc>::from_timestamp_millis(t).unwrap();
        let request =
            ClosedM1RequestPlan::new(at(close), at(close + 600000), vec![at(close + 600000)])
                .unwrap();
        let complete = source(&request, rows(&following), live, live);
        let input = p
            .prepare_rest_exception(
                &complete,
                crate::primary::ExceptionReason::StartupPartial,
                live,
            )
            .unwrap()
            .unwrap();
        let publication = session.canonical(&input).unwrap();
        p.committed(*input, publication.committed_at_ms).unwrap();
        let trace: Vec<Value> = std::fs::read_to_string(path)
            .unwrap()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        let steps: Vec<_> = trace.iter().filter(|r| r["kind"] == "model_step").collect();
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[0]["event"]["mode"], "BACKFILL_REPLAY");
        assert_eq!(steps[1]["event"]["mode"], "LIVE_OBSERVED");
        assert_eq!(
            steps[0]["event"]["before"],
            serde_json::to_value(before).unwrap()
        );
        assert_eq!(
            steps[1]["event"]["before"],
            serde_json::to_value(after).unwrap()
        );
        if expected != Expected::Hold {
            assert!(!steps[0]["event"]["decision"]["action"].is_null());
            assert!(trace
                .iter()
                .any(|r| r["kind"] == "hypothetical_candle_feedback"
                    && r["event"]["mode"] == "BACKFILL_REPLAY"));
        }
    });
}

#[test]
fn tail_v6_historical_bo_entry_feedback_and_next_live_use_same_book() {
    let (model, bar) = before_action(false);
    run(model, bar, Expected::Entry);
}
#[test]
fn tail_v6_historical_bo_exit_feedback_and_next_live_use_same_book() {
    let (model, bar) = before_action(true);
    assert_ne!(model.position_lots(), 0);
    run(model, bar, Expected::Exit);
}
#[test]
fn tail_v6_nonflat_long_short_historical_hold_then_same_live_book() {
    for side in [-1, 1] {
        let (model, mut bar) = crate::model::candle_tests::nonflat(side);
        bar.close_time_utc += 600;
        bar.open = bar.close;
        bar.high = bar.close;
        bar.low = bar.close;
        run(model, bar, Expected::Hold);
    }
}
