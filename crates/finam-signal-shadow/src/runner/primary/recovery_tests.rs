use super::tests::{fixture, go, T};
use super::*;
use crate::session::primary_tests::{raw_row, wire};
use broker_finam::sparse_m10::{
    ClosedM1RestPartV1, ClosedM1SnapshotEvidenceV1, OBSERVED_M1_POLICY_V1,
};
use serde_json::{json, Value};
fn at(t: i64) -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::from_timestamp_millis(t).unwrap()
}
fn trace(path: &std::path::Path) -> Vec<Value> {
    std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(|v| serde_json::from_str(v).unwrap())
        .collect()
}
fn dense(owner: &mut Coordinator, count: i64) {
    for i in 0..=count * 10 {
        let t = T + i * 60000;
        go(t + 100);
        owner.on_frame(wire(t, t + 100)).unwrap();
        if i > 0 && i % 10 == 0 {
            go(t + 1100);
            assert!(matches!(owner.tick().unwrap(), Tick::Published { .. }));
        }
    }
}
pub(super) fn source(plan: &crate::primary::RecoveryPlan) -> AdmittedClosedM1Snapshot {
    let request = plan.request();
    let (start, end) = (request.start(), request.end());
    let rows: Vec<_> = (start.timestamp_millis()..end.timestamp_millis())
        .step_by(60000)
        .map(raw_row)
        .collect();
    let body = json!({"symbol":"IMOEXF@RTSX","bars":rows}).to_string();
    let now = at(crate::timing::now_ms());
    AdmittedClosedM1Snapshot::admit(
        ClosedM1SnapshotEvidenceV1 {
            policy: OBSERVED_M1_POLICY_V1.into(),
            start,
            end,
            parts: vec![ClosedM1RestPartV1 {
                method: "GET".into(),
                endpoint: "https://api.finam.ru/v1/instruments/IMOEXF@RTSX/bars".into(),
                symbol: "IMOEXF@RTSX".into(),
                timeframe: "TIME_FRAME_M1".into(),
                start,
                end,
                requested_at: now,
                received_at: now,
                status: 200,
                transport_complete: true,
                declared_body_bytes: Some(body.len() as u64),
                response_sha256: crate::seed::digest(body.as_bytes()),
                raw_body: body,
            }],
        },
        request,
        now,
    )
    .unwrap()
}
#[test]
fn two_overlap_labels_never_repeat_actual_model_and_missing_two_are_historical() {
    fixture(false, |mut owner, path| {
        dense(&mut owner, 2);
        go(T + 4 * 600000 + 100);
        owner.latch_recovery().unwrap();
        owner
            .processor
            .begin_generation(2, crate::timing::now_ms())
            .unwrap();
        let plan = owner.plan_recovery(T + 4 * 600000).unwrap();
        let source = source(&plan);
        assert_eq!(owner.complete_recovery(&plan, &source).unwrap(), 2);
        assert!(owner.processor.gap_unresolved());
        assert!(owner.resolve_recovery().unwrap().is_none());
        let rows = trace(&path);
        let steps: Vec<_> = rows.iter().filter(|r| r["kind"] == "model_step").collect();
        assert_eq!(steps.len(), 4);
        assert_eq!(steps[0]["event"]["mode"], "LIVE_OBSERVED");
        assert_eq!(steps[2]["event"]["mode"], "BACKFILL_REPLAY");
        let recovered: Vec<_> = rows
            .iter()
            .filter(|r| {
                r["kind"] == "canonical_source"
                    && r["event"]["receipt"]["origin"] == "RECOVERY_BACKFILL"
            })
            .collect();
        assert_eq!(recovered.len(), 2);
        assert_eq!(
            recovered[0]["event"]["receipt"]["actual_received_at_ms"],
            T + 4 * 600000 + 100
        );
        assert_eq!(
            rows.iter()
                .filter(|r| r["kind"] == "source_snapshot")
                .count(),
            1
        );
        let mut frame = wire(T + 4 * 600000, T + 4 * 600000 + 200);
        frame.generation = 2;
        go(T + 4 * 600000 + 200);
        owner.on_frame(frame).unwrap();
        assert!(owner.processor.gap_unresolved());
        assert!(owner.resolve_recovery().unwrap().is_some());
        assert!(!owner.processor.gap_unresolved());
        assert_eq!(owner.processor.counters().recovery_committed, 2);
        assert!(owner.complete_recovery(&plan, &source).is_err());
    });
}
#[test]
fn recovery_source_write_failure_never_retries_mutated_or_unpublished_input() {
    fixture(false, |mut owner, path| {
        owner.latch_recovery().unwrap();
        go(T + 2 * 600000 + 100);
        let plan = owner.plan_recovery(T + 2 * 600000).unwrap();
        let source = source(&plan);
        owner.session.fail_primary_evidence("canonical_source");
        assert!(matches!(
            owner.complete_recovery(&plan, &source),
            Err(CoordinatorError::Session(SessionError::Evidence))
        ));
        assert_eq!(owner.processor.counters().recovery_committed, 0);
        assert!(owner.processor.gap_unresolved());
        assert_eq!(
            owner.complete_recovery(&plan, &source).err(),
            Some(CoordinatorError::Held)
        );
        assert!(!trace(&path).iter().any(|r| r["kind"] == "model_step"));
    });
}
#[test]
fn recovery_commit_failure_is_terminal_and_cannot_retry_the_callback() {
    fixture(false, |mut owner, path| {
        owner.latch_recovery().unwrap();
        go(T + 2 * 600000 + 100);
        let plan = owner.plan_recovery(T + 2 * 600000).unwrap();
        let source = source(&plan);
        owner.session.fail_primary_evidence("canonical_commit");
        assert!(owner.complete_recovery(&plan, &source).is_err());
        assert_eq!(owner.processor.counters().recovery_committed, 0);
        assert!(owner.processor.gap_unresolved());
        assert_eq!(
            owner.complete_recovery(&plan, &source).err(),
            Some(CoordinatorError::Held)
        );
        assert_eq!(
            trace(&path)
                .iter()
                .filter(|r| r["kind"] == "model_step")
                .count(),
            1
        );
    });
}
#[test]
fn fresh_generation_before_rest_does_not_clear_canonical_latch() {
    fixture(false, |mut owner, _path| {
        owner.latch_recovery().unwrap();
        go(T + 600100);
        owner.processor.begin_generation(2, T + 600100).unwrap();
        let mut frame = wire(T + 600000, T + 600200);
        frame.generation = 2;
        go(T + 600200);
        owner.on_frame(frame).unwrap();
        assert!(owner.processor.subscription_data_ready());
        assert!(owner.processor.gap_unresolved());
        let plan = owner.plan_recovery(T + 600000).unwrap();
        let source = source(&plan);
        owner.complete_recovery(&plan, &source).unwrap();
        assert!(owner.processor.gap_unresolved());
        assert!(owner.resolve_recovery().unwrap().is_some());
    });
}
#[test]
fn new_fault_during_resync_does_not_accept_old_completion() {
    fixture(false, |mut owner, _path| {
        owner.latch_recovery().unwrap();
        go(T + 600100);
        let plan = owner.plan_recovery(T + 600000).unwrap();
        let source = source(&plan);
        owner.complete_recovery(&plan, &source).unwrap();
        owner.latch_recovery().unwrap();
        assert!(owner.resolve_recovery().is_err());
        assert!(owner.processor.gap_unresolved());
    });
}
