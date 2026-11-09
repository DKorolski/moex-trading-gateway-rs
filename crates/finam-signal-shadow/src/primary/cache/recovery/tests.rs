use super::*;
use crate::session::primary_tests::{raw_row, wire};
use broker_finam::sparse_m10::{
    ClosedM1RestPartV1, ClosedM1SnapshotEvidenceV1, OBSERVED_M1_POLICY_V1,
};
use serde_json::{json, Value};
const T: i64 = 1793678400000;
fn processor() -> PrimaryProcessor {
    let mut config = crate::primary::tests::config();
    config["base"]["observation_start_utc_ms"] = json!(T);
    config["base"]["run_deadline_utc_ms"] = json!(T + 17 * 3600000);
    let config =
        crate::primary::WsPrimaryConfig::parse(&serde_json::to_vec(&config).unwrap()).unwrap();
    let instrument = broker_core::InstrumentId {
        symbol: "IMOEXF".into(),
        venue_symbol: Some("IMOEXF@RTSX".into()),
        exchange: broker_core::Exchange::Moex,
        market: broker_core::Market::Futures,
    };
    PrimaryProcessor::from_fixture(
        &config,
        instrument,
        (0..101).map(|i| T + i * 600000).collect(),
        1,
        T - 100,
    )
    .unwrap()
}
fn committed(p: &mut PrimaryProcessor, count: i64) {
    for i in 0..=count * 10 {
        let t = T + i * 60000;
        p.ingest(wire(t, t + 100), t + 100).unwrap();
        if i > 0 && i % 10 == 0 {
            let CanonicalStatus::Ready(input) = p.prepare(t + 1100).unwrap() else {
                panic!("dense input")
            };
            p.committed(*input, t + 1100).unwrap();
        }
    }
}
fn rows(plan: &RecoveryPlan) -> Vec<Value> {
    (plan.request.start().timestamp_millis()..plan.request.end().timestamp_millis())
        .step_by(60000)
        .map(raw_row)
        .collect()
}
pub(super) fn evidence(
    plan: &RecoveryPlan,
    rows: Vec<Value>,
    now: i64,
) -> AdmittedClosedM1Snapshot {
    let (start, end) = (plan.request.start(), plan.request.end());
    let raw = json!({"symbol":"IMOEXF@RTSX","bars":rows}).to_string();
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
                requested_at: at(plan.issued_at_ms).unwrap(),
                received_at: at(now).unwrap(),
                status: 200,
                transport_complete: true,
                declared_body_bytes: Some(raw.len() as u64),
                response_sha256: crate::seed::digest(raw.as_bytes()),
                raw_body: raw,
            }],
        },
        &plan.request,
        at(now).unwrap(),
    )
    .unwrap()
}
#[test]
fn identical_forming_m1_on_a_new_generation_establishes_only_new_data() {
    let mut p = processor();
    p.ingest(wire(T, T + 100), T + 100).unwrap();
    p.begin_generation(2, T + 200).unwrap();
    let mut frame = wire(T, T + 300);
    frame.generation = 2;
    p.ingest(frame, T + 300).unwrap();
    assert!(p.subscription_data_ready());
    assert_eq!(p.last_live_data().unwrap().generation, 2);
    assert_eq!(p.counters.valid_updates, 2);
    assert!(p.gap_unresolved());
}
#[test]
fn plan_is_independent_two_label_overlap_and_single_pending_request() {
    let mut p = processor();
    committed(&mut p, 3);
    p.break_continuity();
    let plan = p
        .plan_recovery(T + 5 * 600000, T + 5 * 600000 + 100)
        .unwrap();
    assert_eq!(plan.request.start().timestamp_millis(), T + 600000);
    assert_eq!(plan.overlap_labels(), 2);
    assert_eq!(plan.missed_labels(), 2);
    assert!(p
        .plan_recovery(T + 5 * 600000, T + 5 * 600000 + 100)
        .is_err());
}
#[test]
fn future_unaligned_oversixhour_and_cross_session_cutovers_fail() {
    for (cutover, now) in [
        (T + 600000, T + 599999),
        (T + 600001, T + 600100),
        (T + 37 * 600000, T + 37 * 600000 + 100),
        (T + 24 * 3600000, T + 24 * 3600000 + 100),
    ] {
        let mut p = processor();
        p.break_continuity();
        assert!(p.plan_recovery(cutover, now).is_err());
    }
    let mut p = processor();
    p.break_continuity();
    assert!(p
        .plan_recovery(T + 36 * 600000, T + 36 * 600000 + 100)
        .is_ok());
}
#[test]
fn grid_hole_and_no_fault_cannot_become_a_recovery_plan() {
    let mut p = processor();
    assert!(p.plan_recovery(T + 600000, T + 600100).is_err());
    p.break_continuity();
    p.required.remove(1);
    assert!(p
        .plan_recovery(T + 3 * 600000, T + 3 * 600000 + 100)
        .is_err());
}

#[test]
fn cutover_cannot_be_truncated_to_hide_already_closed_required_bars() {
    let mut p = processor();
    p.break_continuity();
    assert!(p.plan_recovery(T + 600000, T + 2 * 600000 + 100).is_err());
    assert_eq!(p.next, 0);
    assert!(p.gap_unresolved());
}
#[test]
fn complete_overlap_validation_precedes_any_step_capability() {
    let mut p = processor();
    committed(&mut p, 2);
    p.break_continuity();
    let now = T + 4 * 600000 + 100;
    let plan = p.plan_recovery(T + 4 * 600000, now).unwrap();
    let mut data = rows(&plan);
    data[1]["volume"] = json!({"value":"3"});
    let source = evidence(&plan, data, now);
    assert_eq!(
        p.admit_recovery(&plan, &source, now).err(),
        Some(PrimaryError::CanonicalConflict)
    );
    assert_eq!(p.next_label_ms(), Some(T + 2 * 600000));
    assert_eq!(p.counters.recovery_committed, 0);
    assert!(p.gap_unresolved());
}
#[test]
fn an_empty_last_missing_bucket_invalidates_the_whole_batch() {
    let mut p = processor();
    p.break_continuity();
    let now = T + 2 * 600000 + 100;
    let plan = p.plan_recovery(T + 2 * 600000, now).unwrap();
    let data = rows(&plan).into_iter().take(10).collect();
    let source = evidence(&plan, data, now);
    assert!(p.admit_recovery(&plan, &source, now).is_err());
    assert_eq!(p.counters.recovery_committed, 0);
    assert_eq!(p.next, 0);
}
#[test]
fn no_resolution_before_commits_or_without_fresh_data_then_one_shot_resolution() {
    let mut p = processor();
    p.begin_generation(2, T + 2 * 600000 + 100).unwrap();
    let now = T + 2 * 600000 + 100;
    let plan = p.plan_recovery(T + 2 * 600000, now).unwrap();
    let source = evidence(&plan, rows(&plan), now);
    let batch = p.admit_recovery(&plan, &source, now).unwrap();
    assert_eq!(batch.steps(), 2);
    assert!(p.finish_recovery(&batch.completion, now).is_err());
    for input in batch.inputs {
        p.recovered(input, now).unwrap();
    }
    assert!(p.finish_recovery(&batch.completion, now).unwrap().is_none());
    assert!(p.gap_unresolved());
    let mut frame = wire(T + 2 * 600000, now + 100);
    frame.generation = 2;
    p.ingest(frame, now + 100).unwrap();
    assert!(p.gap_unresolved());
    let resolution = p
        .finish_recovery(&batch.completion, now + 100)
        .unwrap()
        .unwrap();
    assert_eq!(resolution.cutover_ms(), T + 2 * 600000);
    assert!(!p.gap_unresolved());
    assert_eq!(p.counters.recovery_committed, 2);
    assert!(p.finish_recovery(&batch.completion, now + 100).is_err());
}
#[test]
fn a_second_fault_invalidates_an_already_replayed_completion() {
    let mut p = processor();
    p.break_continuity();
    let now = T + 600100;
    let plan = p.plan_recovery(T + 600000, now).unwrap();
    let source = evidence(&plan, rows(&plan), now);
    let batch = p.admit_recovery(&plan, &source, now).unwrap();
    for input in batch.inputs {
        p.recovered(input, now).unwrap();
    }
    p.break_continuity();
    p.ingest(wire(T + 600000, now + 100), now + 100).unwrap();
    assert!(p.finish_recovery(&batch.completion, now + 100).is_err());
    assert!(p.gap_unresolved());
}
#[test]
fn old_generation_and_another_processor_cannot_reuse_a_plan() {
    let mut p = processor();
    p.break_continuity();
    let now = T + 600100;
    let plan = p.plan_recovery(T + 600000, now).unwrap();
    let source = evidence(&plan, rows(&plan), now);
    let mut other = processor();
    other.break_continuity();
    assert!(other.admit_recovery(&plan, &source, now).is_err());
    p.begin_generation(2, now).unwrap();
    assert!(p.admit_recovery(&plan, &source, now).is_err());
    assert!(p.gap_unresolved());
}
#[test]
fn batch_keeps_real_receive_and_replay_clock_separate_and_cannot_reissue() {
    let mut p = processor();
    p.break_continuity();
    let now = T + 2 * 600000 + 100;
    let plan = p.plan_recovery(T + 2 * 600000, now).unwrap();
    let source = evidence(&plan, rows(&plan), now);
    let batch = p.admit_recovery(&plan, &source, now).unwrap();
    let receipt = batch.inputs[0].receipt();
    assert_eq!(receipt.canonical.origin, "RECOVERY_BACKFILL");
    assert_eq!(receipt.actual_received_at_ms, now);
    assert_eq!(receipt.virtual_callback_at_ms, T + 600000);
    assert!(p.admit_recovery(&plan, &source, now).is_err());
}
