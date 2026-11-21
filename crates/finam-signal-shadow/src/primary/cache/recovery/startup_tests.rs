use super::tests::evidence;
use super::*;
use crate::primary::calendar::tests::Fixture;
use crate::session::primary_tests::{raw_row, wire};

fn bound(f: &Fixture) -> PrimaryProcessor {
    let mut p = PrimaryProcessor::from_fixture(
        &f.config,
        broker_core::InstrumentId {
            symbol: "IMOEXF".into(),
            venue_symbol: Some("IMOEXF@RTSX".into()),
            exchange: broker_core::Exchange::Moex,
            market: broker_core::Market::Futures,
        },
        f.model.required_m10_labels_ms.clone(),
        1,
        f.open(),
    )
    .unwrap();
    p.bind_calendar(f.cap(f.open()).unwrap()).unwrap();
    p.begin_generation(2, f.open() + 100).unwrap();
    p
}
fn row(f: &Fixture) -> serde_json::Value {
    raw_row(f.binding.bootstrap_reference_start_ms + 101 * 600000 - 60000)
}
fn live(p: &mut PrimaryProcessor, f: &Fixture, now: i64) {
    let mut frame = wire(f.open(), now);
    frame.generation = 2;
    p.ingest(frame, now).unwrap();
}
fn scenario(work: impl FnOnce(Fixture)) {
    let f = Fixture::new();
    crate::timing::with_fixture_clock(f.open(), || work(f));
}

#[test]
fn startup_requires_independent_calendar_and_source_even_after_new_data() {
    scenario(|f| {
        let mut unbound = bound(&f);
        unbound.calendar = None;
        assert!(unbound.plan_recovery(f.open(), f.open() + 100).is_err());
        let mut p = bound(&f);
        live(&mut p, &f, f.open() + 200);
        assert!(p.gap_unresolved());
        let plan = p.plan_recovery(f.open(), f.open() + 200).unwrap();
        assert!(plan.startup_anchor());
        assert_eq!((plan.overlap_labels(), plan.missed_labels()), (0, 0));
        assert_eq!(
            plan.request().start().timestamp_millis(),
            f.binding.bootstrap_reference_start_ms
        );
        assert_eq!(plan.request().end().timestamp_millis(), f.open());
        let source = evidence(&plan, vec![row(&f)], f.open() + 300);
        let batch = p.admit_recovery(&plan, &source, f.open() + 300).unwrap();
        assert_eq!(batch.steps(), 0);
        assert_eq!(p.next, 0);
        assert_eq!(p.counters.committed, 0);
        let resolution = p
            .finish_recovery(&batch.completion, f.open() + 300)
            .unwrap()
            .unwrap();
        assert_eq!(resolution.cutover_ms(), f.open());
        assert!(!p.gap_unresolved());
        assert_eq!(p.counters.recovery_committed, 0);
        assert!(p.admit_recovery(&plan, &source, f.open() + 300).is_err());
        assert!(p
            .finish_recovery(&batch.completion, f.open() + 300)
            .is_err());
    });
}
#[test]
fn anchor_cannot_restore_data_readiness_or_survive_another_fault() {
    scenario(|f| {
        let mut p = bound(&f);
        let plan = p.plan_recovery(f.open(), f.open() + 200).unwrap();
        let source = evidence(&plan, vec![row(&f)], f.open() + 300);
        let batch = p.admit_recovery(&plan, &source, f.open() + 300).unwrap();
        assert!(p
            .finish_recovery(&batch.completion, f.open() + 300)
            .unwrap()
            .is_none());
        assert!(p.gap_unresolved());
        p.break_continuity();
        live(&mut p, &f, f.open() + 400);
        assert!(p
            .finish_recovery(&batch.completion, f.open() + 400)
            .is_err());
        assert!(p.gap_unresolved());
    });
}
#[test]
fn late_or_conflicting_startup_anchor_does_not_change_frontier() {
    scenario(|f| {
        let mut p = bound(&f);
        let plan = p.plan_recovery(f.open(), f.open() + 200).unwrap();
        let source = evidence(&plan, vec![row(&f)], f.open() + 300);
        assert!(p
            .admit_recovery(&plan, &source, plan.deadline_ms() + 1)
            .is_err());
        assert_eq!(p.next, 0);
        assert!(p.gap_unresolved());
        let mut p = bound(&f);
        let plan = p.plan_recovery(f.open(), f.open() + 200).unwrap();
        p.add_reference(&source).unwrap();
        let mut changed = row(&f);
        changed["volume"] = serde_json::json!({"value":"99"});
        let conflict = evidence(&plan, vec![changed], f.open() + 300);
        assert_eq!(
            p.admit_recovery(&plan, &conflict, f.open() + 300).err(),
            Some(PrimaryError::ReferenceConflict)
        );
        assert_eq!(p.next, 0);
        assert_eq!(p.state(), PipelineState::Hold);
    });
}

#[test]
fn empty_early_requested_or_wrong_range_source_never_mints_startup_completion() {
    scenario(|f| {
        for case in ["empty", "early", "range"] {
            let mut p = bound(&f);
            let plan = p.plan_recovery(f.open(), f.open() + 200).unwrap();
            let initial = evidence(
                &plan,
                if case == "empty" {
                    vec![]
                } else {
                    vec![row(&f)]
                },
                f.open() + 300,
            );
            let mut ev = initial.evidence().clone();
            if case == "early" {
                ev.parts[0].requested_at -= chrono::Duration::milliseconds(1);
            }
            if case == "range" {
                ev.start += chrono::Duration::minutes(1);
                ev.parts[0].start = ev.start;
            }
            let request = ClosedM1RequestPlan::new(ev.start, ev.end, vec![ev.end]).unwrap();
            let source =
                AdmittedClosedM1Snapshot::admit(ev, &request, at(f.open() + 300).unwrap()).unwrap();
            assert!(
                p.admit_recovery(&plan, &source, f.open() + 300).is_err(),
                "{case}"
            );
            assert_eq!(p.next, 0);
            assert!(p.gap_unresolved());
            assert_eq!(p.counters.committed, 0);
        }
    });
}
