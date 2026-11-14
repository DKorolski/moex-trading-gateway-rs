use super::super::tests::{committed, evidence, processor_delivery, rows, T};
use super::*;
use crate::{primary::DeliveryPolicy, session::primary_tests::wire};

fn setup() -> (PrimaryProcessor, RecoveryPlan, i64) {
    setup_delivery(DeliveryPolicy::ObservedBoRestTailRecoveryV6)
}
fn setup_delivery(delivery: DeliveryPolicy) -> (PrimaryProcessor, RecoveryPlan, i64) {
    let mut p = processor_delivery(delivery);
    committed(&mut p, 2);
    for i in 21..30 {
        p.ingest(
            wire(T + i * 60000, T + i * 60000 + 100),
            T + i * 60000 + 100,
        )
        .unwrap();
    }
    let now = T + 1800000 + 100;
    p.begin_generation(2, now).unwrap();
    let plan = p
        .issue_recovery_request(
            T + 1800000,
            now,
            RecoveryRequestScope {
                episode: 1,
                ordinal: 1,
                deadline_ms: now + 45000,
            },
            None,
        )
        .unwrap();
    (p, plan, now)
}

#[test]
fn v7_r1_failed_ticket_is_consumed_without_source_and_cannot_cross_owner_or_generation() {
    let (mut p, plan, now) = setup_delivery(DeliveryPolicy::ObservedBoDataWaitRecoveryV7);
    let (_, foreign, _) = setup_delivery(DeliveryPolicy::ObservedBoDataWaitRecoveryV7);
    let before = (
        p.next,
        p.committed.len(),
        p.reference.len(),
        p.retained_ws.len(),
        p.source_changes.len(),
    );
    assert!(p.retire_failed_recovery_request(&foreign, now).is_err());
    let mut scope = plan.scope().unwrap();
    scope.ordinal += 1;
    assert!(p
        .issue_recovery_request(T + 1800000, now, scope, Some(&plan))
        .is_err());
    p.retire_failed_recovery_request(&plan, now + 1).unwrap();
    assert!(p.retire_failed_recovery_request(&plan, now + 1).is_err());
    assert!(p
        .prepare_recovery_request(&plan, &evidence(&plan, rows(&plan), now + 1), now + 1)
        .is_err());
    assert_eq!(
        (
            p.next,
            p.committed.len(),
            p.reference.len(),
            p.retained_ws.len(),
            p.source_changes.len()
        ),
        before
    );
    let next = p
        .issue_recovery_request(T + 1800000, now + 1001, scope, Some(&plan))
        .unwrap();
    assert_ne!(next.sha256(), plan.sha256());
    p.begin_generation(3, now + 1002).unwrap();
    assert!(p.retire_failed_recovery_request(&next, now + 1002).is_err());
}

#[test]
fn tail_v6_pending_is_non_destructive_and_new_ticket_is_required() {
    let (mut p, plan, now) = setup();
    let before = (
        p.next,
        p.committed.len(),
        p.reference.len(),
        p.retained_ws.len(),
        p.source_changes.len(),
    );
    let mut data: Vec<_> = rows(&plan).into_iter().take(27).collect();
    // A preceding provisional price change must not retain authority while
    // the later inventory hole still makes this whole batch Pending.
    data[20]["close"] = serde_json::json!({"value":"2200"});
    data[20]["volume"] = serde_json::json!({"value":"1"});
    let source = evidence(&plan, data, now);
    let result = p.prepare_recovery_request(&plan, &source, now).unwrap();
    assert!(
        matches!(result,RecoveryOutcome::DataNotReady(ref w) if w.missing_known_m1_ms.len()==3)
    );
    assert_eq!(
        (
            p.next,
            p.committed.len(),
            p.reference.len(),
            p.retained_ws.len(),
            p.source_changes.len()
        ),
        before
    );
    assert_ne!(p.state, super::super::super::PipelineState::Hold);
    assert!(p.prepare_recovery_request(&plan, &source, now).is_err());
    let mut scope = plan.scope().unwrap();
    scope.ordinal += 1;
    let next = p
        .issue_recovery_request(T + 1800000, now + 6000, scope, Some(&plan))
        .unwrap();
    assert_ne!(next.sha256(), plan.sha256());
    let full = evidence(&next, rows(&next), now + 6000);
    assert!(p
        .prepare_recovery_request(&plan, &full, now + 6000)
        .is_err());
    let RecoveryOutcome::Ready(batch) = p
        .prepare_recovery_request(&next, &full, now + 6000)
        .unwrap()
    else {
        panic!("ready")
    };
    assert_eq!(batch.steps(), 1);
    for input in batch.inputs {
        p.recovered(input, now + 6000).unwrap();
    }
    assert_eq!(p.next, 3);
    assert!(p
        .prepare_recovery_request(&next, &full, now + 6000)
        .is_err());
}

#[test]
fn tail_v6_empty_last_bucket_is_pending_but_committed_hole_wins() {
    for bad_overlap in [false, true] {
        let (mut p, plan, now) = setup();
        let mut data: Vec<_> = rows(&plan).into_iter().take(20).collect();
        if bad_overlap {
            data.remove(0);
        }
        let source = evidence(&plan, data, now);
        let result = p.prepare_recovery_request(&plan, &source, now);
        if bad_overlap {
            assert_eq!(result.err(), Some(PrimaryError::CanonicalConflict));
        } else {
            assert!(matches!(result,Ok(RecoveryOutcome::DataNotReady(w)) if w.empty_last_bucket));
        }
        assert_eq!(p.next, 2);
    }
}

#[test]
fn tail_v6_interior_known_hole_never_becomes_pending() {
    let (mut p, plan, now) = setup();
    let mut data = rows(&plan);
    data.remove(24);
    let source = evidence(&plan, data, now);
    assert_eq!(
        p.prepare_recovery_request(&plan, &source, now).err(),
        Some(PrimaryError::CanonicalConflict)
    );
    assert_eq!(p.next, 2);
}

#[test]
fn tail_v6_retained_only_tail_and_sparse_inventory() {
    for interior in [false, true] {
        let (mut p, plan, now) = setup();
        let key = T + 29 * 60000;
        let v = p.cache.remove(&key).unwrap();
        p.retained_ws.insert(key, vec![v]);
        let mut data = rows(&plan);
        data.pop();
        if interior {
            data.remove(24);
        }
        let source = evidence(&plan, data, now);
        let result = p.prepare_recovery_request(&plan, &source, now);
        if interior {
            assert_eq!(result.err(), Some(PrimaryError::CanonicalConflict));
        } else {
            assert!(
                matches!(result,Ok(RecoveryOutcome::DataNotReady(w)) if w.missing_known_m1_ms==vec![key])
            );
        }
    }
    let (mut p, plan, now) = setup();
    let key = T + 24 * 60000;
    p.cache.remove(&key);
    let mut data = rows(&plan);
    data.remove(24);
    let source = evidence(&plan, data, now);
    assert!(matches!(
        p.prepare_recovery_request(&plan, &source, now),
        Ok(RecoveryOutcome::Ready(_))
    ));
}

#[test]
fn tail_v6_expired_response_cannot_mint_and_renewal_cannot_extend_episode() {
    let (mut p, plan, now) = setup();
    let source = evidence(&plan, rows(&plan), now);
    assert!(
        matches!(p.prepare_recovery_request(&plan,&source,now+5001),Ok(RecoveryOutcome::DataNotReady(w)) if w.reason=="RequestExpired")
    );
    let mut scope = plan.scope().unwrap();
    scope.ordinal += 1;
    scope.deadline_ms += 1;
    assert!(p
        .issue_recovery_request(T + 1800000, now + 6000, scope, Some(&plan))
        .is_err());
    scope.deadline_ms -= 1;
    assert!(p
        .issue_recovery_request(T + 1800000, now + 6000, scope, Some(&plan))
        .is_ok());
}

#[test]
fn tail_v6_new_generation_invalidates_waiting_ticket() {
    let (mut p, plan, now) = setup();
    let source = evidence(&plan, rows(&plan), now);
    p.begin_generation(3, now + 1).unwrap();
    assert!(p.prepare_recovery_request(&plan, &source, now + 1).is_err());
    assert!(p.gap_unresolved());
    assert_eq!(p.next, 2);
}

#[test]
fn tail_v6_partial_tail_arrivals_consume_distinct_requests_and_only_full_source_mints() {
    let (mut p, first, now) = setup();
    let mut current = first;
    let before = p.counters.committed;
    for (round, count) in [27, 29, 30].into_iter().enumerate() {
        let at_ms = now + round as i64 * 6000;
        let source = evidence(
            &current,
            rows(&current).into_iter().take(count).collect(),
            at_ms,
        );
        let outcome = p
            .prepare_recovery_request(&current, &source, at_ms)
            .unwrap();
        if count < 30 {
            assert!(matches!(outcome, RecoveryOutcome::DataNotReady(_)));
            assert_eq!(p.counters.committed, before);
            let mut scope = current.scope().unwrap();
            scope.ordinal += 1;
            current = p
                .issue_recovery_request(T + 1800000, at_ms + 6000, scope, Some(&current))
                .unwrap();
        } else {
            assert!(matches!(outcome, RecoveryOutcome::Ready(_)));
        }
    }
}

#[test]
fn tail_v6_extra_committed_slot_is_not_masked_by_missing_recent_tail() {
    let (mut p, plan, now) = setup();
    p.committed.remove(&(T + 240000));
    let source = evidence(&plan, rows(&plan).into_iter().take(27).collect(), now);
    assert_eq!(
        p.prepare_recovery_request(&plan, &source, now).err(),
        Some(PrimaryError::CanonicalConflict)
    );
}

#[test]
fn tail_v6_aggregate_numeric_error_has_priority_over_missing_tail() {
    let (mut p, plan, now) = setup();
    let mut values: Vec<_> = rows(&plan).into_iter().take(27).collect();
    for row in &mut values[..2] {
        row["volume"] = serde_json::json!({"value":rust_decimal::Decimal::MAX.to_string()});
    }
    let source = evidence(&plan, values, now);
    assert_eq!(
        p.prepare_recovery_request(&plan, &source, now).err(),
        Some(PrimaryError::Numeric)
    );
    assert_eq!(p.next, 2);
    assert_eq!(p.counters.recovery_committed, 0);
}

#[test]
fn tail_v6_policy_fingerprint_is_explicit_and_strategy_feedback_are_unchanged() {
    let mut raw = crate::primary::tests::config();
    raw["primary"]["delivery"] = serde_json::json!(DeliveryPolicy::ObservedBoConsistentSourcesV5);
    let old = crate::primary::WsPrimaryConfig::parse(&serde_json::to_vec(&raw).unwrap()).unwrap();
    raw["primary"]["delivery"] = serde_json::json!(DeliveryPolicy::ObservedBoRestTailRecoveryV6);
    let new = crate::primary::WsPrimaryConfig::parse(&serde_json::to_vec(&raw).unwrap()).unwrap();
    assert_ne!(old.fingerprint().unwrap(), new.fingerprint().unwrap());
    assert_ne!(
        old.primary.fingerprint().unwrap(),
        new.primary.fingerprint().unwrap()
    );
    assert_eq!(old.base.model_sha256, new.base.model_sha256);
    assert_eq!(
        old.base.feedback_policy_sha256,
        new.base.feedback_policy_sha256
    );
    assert_eq!(new.primary.close_grace_ms, 1000);
}

#[test]
fn tail_v6_real_november_12_result_old_policy_holds_new_policy_waits() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../tests/fixtures/rest-tail-20261112.json"
    ))
    .unwrap();
    for delivery in [
        DeliveryPolicy::ObservedBoConsistentSourcesV5,
        DeliveryPolicy::ObservedBoRestTailRecoveryV6,
    ] {
        let mut raw = fixture["config"].clone();
        raw["primary"]["delivery"] = serde_json::json!(delivery);
        let config =
            crate::primary::WsPrimaryConfig::parse(&serde_json::to_vec(&raw).unwrap()).unwrap();
        let known: Vec<Bar> = serde_json::from_value(fixture["known_ws"].clone()).unwrap();
        let committed: Vec<Bar> =
            serde_json::from_value(fixture["committed_overlap"].clone()).unwrap();
        let start = config.base.observation_start_utc_ms;
        let mut p = PrimaryProcessor::from_fixture(
            &config,
            known[0].instrument.clone(),
            (0..101).map(|i| start + i * 600000).collect(),
            1,
            start - 100,
        )
        .unwrap();
        p.next = 18;
        for row in committed {
            p.committed.insert(row.open_ts.timestamp_millis(), row);
        }
        for row in known {
            let t = row.open_ts.timestamp_millis();
            // Reconstruction of cache STATE, not a claim about original wire
            // receive times; the original normalized values are byte-retained.
            p.cache.insert(
                t,
                super::super::super::Version {
                    bar: row,
                    generation: 1,
                    first_received_ms: t + 100,
                    last_changed_ms: t + 100,
                    frame_sha256: "1".repeat(64),
                },
            );
        }
        let raw: broker_finam::sparse_m10::ClosedM1SnapshotEvidenceV1 =
            serde_json::from_value(fixture["recovery"].clone()).unwrap();
        let now = raw.parts[0].requested_at.timestamp_millis() - 1;
        let cutover = raw.end.timestamp_millis();
        p.begin_generation(2, now).unwrap();
        let plan = if delivery == DeliveryPolicy::ObservedBoConsistentSourcesV5 {
            p.plan_recovery(cutover, now).unwrap()
        } else {
            p.issue_recovery_request(
                cutover,
                now,
                RecoveryRequestScope {
                    episode: 1,
                    ordinal: 1,
                    deadline_ms: now + 43000,
                },
                None,
            )
            .unwrap()
        };
        let received = raw.parts[0].received_at.timestamp_millis() + 1;
        let source =
            AdmittedClosedM1Snapshot::admit(raw, plan.request(), at(received).unwrap()).unwrap();
        if delivery == DeliveryPolicy::ObservedBoConsistentSourcesV5 {
            assert_eq!(
                p.admit_recovery(&plan, &source, received).err(),
                Some(PrimaryError::CanonicalConflict)
            );
        } else {
            let RecoveryOutcome::DataNotReady(wait) = p
                .prepare_recovery_request(&plan, &source, received)
                .unwrap()
            else {
                panic!("pending")
            };
            assert_eq!(
                wait.missing_known_m1_ms,
                vec![1794467220000, 1794467280000, 1794467340000]
            );
            assert_eq!(p.next, 18);
            assert_eq!(p.counters.recovery_committed, 0);
        }
    }
}
