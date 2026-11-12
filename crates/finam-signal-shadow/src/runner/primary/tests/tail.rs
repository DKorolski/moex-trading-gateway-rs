use super::*;
use crate::primary::{DeliveryPolicy, RecoveryRequestScope};

fn fixture(work: impl FnOnce(Coordinator, PathBuf)) {
    ws_first::candidate(
        T,
        T - 100,
        DeliveryPolicy::ObservedBoRestTailRecoveryV6,
        |mut owner, path| {
            for i in 0..10 {
                ws_first::send(&mut owner, raw_row(T + i * 60000), T + i * 60000 + 100);
            }
            go(T + 600100);
            owner.latch_recovery().unwrap();
            owner.processor.begin_generation(2, T + 600100).unwrap();
            work(owner, path);
        },
    );
}
fn source(plan: &crate::primary::RecoveryPlan, partial: bool) -> AdmittedClosedM1Snapshot {
    let original = crate::runner::primary::recovery_tests::source(plan);
    if !partial {
        return original;
    }
    let mut evidence = original.evidence().clone();
    let p = &mut evidence.parts[0];
    let mut raw: Value = serde_json::from_str(&p.raw_body).unwrap();
    let n = raw["bars"].as_array().unwrap().len();
    raw["bars"].as_array_mut().unwrap().truncate(n - 3);
    p.raw_body = raw.to_string();
    p.declared_body_bytes = Some(p.raw_body.len() as u64);
    p.response_sha256 = crate::seed::digest(p.raw_body.as_bytes());
    AdmittedClosedM1Snapshot::admit(evidence, plan.request(), at(crate::timing::now_ms())).unwrap()
}

#[test]
fn tail_v6_late_old_completion_and_current_completion_have_only_one_callback() {
    fixture(|mut owner, path| {
        let mut scope = RecoveryRequestScope {
            episode: 1,
            ordinal: 1,
            deadline_ms: T + 645100,
        };
        let old = owner
            .issue_recovery_request(T + 600000, scope, None)
            .unwrap();
        assert!(matches!(
            owner.complete_recovery_request(&old, &source(&old, true)),
            Ok(RecoveryProgress::DataNotReady(_))
        ));
        assert!(read(&path).iter().all(|r| r["kind"] != "model_step"));
        go(T + 606100);
        scope.ordinal = 2;
        let new = owner
            .issue_recovery_request(T + 600000, scope, Some(&old))
            .unwrap();
        let complete = source(&new, false);
        assert!(matches!(
            owner.complete_recovery_request(&old, &complete),
            Ok(RecoveryProgress::Retired)
        ));
        assert!(matches!(
            owner.complete_recovery_request(&new, &complete),
            Ok(RecoveryProgress::Ready(1))
        ));
        assert!(matches!(
            owner.complete_recovery_request(&new, &complete),
            Ok(RecoveryProgress::Retired)
        ));
        assert_eq!(
            read(&path)
                .iter()
                .filter(|r| r["kind"] == "model_step")
                .count(),
            1
        );
        assert_eq!(owner.processor.counters().committed, 1);
    });
}

#[test]
fn tail_v6_uncertain_source_step_or_commit_publication_never_reexecutes_model() {
    for kind in ["canonical_source", "model_step", "canonical_commit"] {
        fixture(|mut owner, path| {
            let plan = owner
                .issue_recovery_request(
                    T + 600000,
                    RecoveryRequestScope {
                        episode: 1,
                        ordinal: 1,
                        deadline_ms: T + 645100,
                    },
                    None,
                )
                .unwrap();
            let complete = source(&plan, false);
            owner.session.fail_primary_evidence(kind);
            assert!(owner.complete_recovery_request(&plan, &complete).is_err());
            let before = read(&path);
            assert_eq!(
                owner.complete_recovery_request(&plan, &complete).err(),
                Some(CoordinatorError::Held)
            );
            assert_eq!(read(&path), before);
            assert_eq!(owner.processor.counters().committed, 0);
        });
    }
}
