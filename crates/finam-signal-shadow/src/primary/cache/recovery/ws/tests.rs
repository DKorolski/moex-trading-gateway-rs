use super::super::tests::{processor_delivery, T};
use super::*;
use crate::primary::{CanonicalStatus, DeliveryPolicy};
use crate::session::primary_tests::wire;

fn fixture(policy: DeliveryPolicy, boundary: bool) -> PrimaryProcessor {
    let mut p = processor_delivery(policy);
    let owner = Arc::new(());
    p.physical_owner = Some(owner.clone());
    for i in 0..10 {
        let at = T + i * 60000 + 100;
        let mut frame = wire(T + i * 60000, at);
        frame.owner = Some(owner.clone());
        p.ingest(frame, at).unwrap();
    }
    if boundary {
        let mut frame = wire(T + 600000, T + 601364);
        frame.owner = Some(owner);
        p.ingest(frame, T + 601364).unwrap();
    }
    p
}
fn latch(p: &mut PrimaryProcessor) {
    assert!(matches!(
        p.prepare(T + 601904).unwrap(),
        CanonicalStatus::PendingProof { .. }
    ));
    assert!(p.gap_unresolved());
    p.relatch_data_gap();
}
#[test]
fn v8_late_ws_uses_original_grace_and_one_shot_historical_frontier() {
    let mut p = fixture(DeliveryPolicy::ObservedBoWsHistoricalRecoveryV8, true);
    latch(&mut p);
    assert!(p
        .prepare_ws_recovery(1, T + 646904, 1, T + 602363)
        .unwrap()
        .is_none());
    let a = p
        .prepare_ws_recovery(1, T + 646904, 1, T + 602364)
        .unwrap()
        .unwrap();
    let r = a.input.receipt();
    assert_eq!(r.canonical.origin, "WS_RECOVERY_BACKFILL");
    assert_eq!(r.canonical.boundary_received_at_ms, Some(T + 601364));
    assert_eq!(r.canonical.canonical_ready_at_ms, T + 602364);
    assert_eq!(r.virtual_callback_at_ms, T + 600000);
    assert!(!r.broker_execution);
    assert!(r.canonical.rest_snapshot_sha256.is_none());
    assert_eq!(p.counters.committed, 0);
    assert!(p
        .prepare_ws_recovery(1, T + 646904, 1, T + 602364)
        .unwrap()
        .is_none());
    p.recovered_ws(a.input, T + 602365).unwrap();
    assert_eq!(p.next_label_ms(), Some(T + 600000));
    assert_eq!(p.counters.recovery_committed, 1);
    assert!(
        p.committed_rest.is_empty(),
        "WS never invents REST authority"
    );
    assert!(p
        .finish_recovery(&a.completion, T + 602366)
        .unwrap()
        .is_some());
    assert!(!p.gap_unresolved());
}
#[test]
fn v8_does_not_relax_v7_or_integrity_or_physical_faults() {
    for case in 0..11 {
        let policy = if case == 0 {
            DeliveryPolicy::ObservedBoDataWaitRecoveryV7
        } else {
            DeliveryPolicy::ObservedBoWsHistoricalRecoveryV8
        };
        let mut p = fixture(policy, case != 1);
        match case {
            2 => {
                p.cache.remove(&(T + 540000));
            }
            3 => {
                p.cache.get_mut(&T).unwrap().generation = 2;
            }
            4 => {
                p.quarantined.insert(T);
            }
            5 => {
                p.subscribed_at_ms = T + 1;
            }
            6 => {
                p.physical_owner = None;
            }
            7 => {
                p.in_flight = Some(Arc::new(()));
            }
            _ => (),
        }
        if case == 7 {
            assert!(matches!(
                p.prepare(T + 601904).unwrap(),
                CanonicalStatus::AwaitingCommit
            ));
        } else {
            latch(&mut p);
        }
        if case == 8 {
            p.break_continuity();
            p.relatch_data_gap();
        }
        if case == 9 {
            p.physical_owner = Some(Arc::new(()));
        }
        if case == 10 {
            assert_eq!(
                p.prepare_ws_recovery(1, T + 646904, 1, T).err(),
                Some(PrimaryError::Clock)
            );
            continue;
        }
        assert!(
            p.prepare_ws_recovery(1, T + 646904, 1, T + 602364)
                .unwrap()
                .is_none(),
            "case {case}"
        );
        assert_eq!(p.counters.committed, 0);
    }
}
#[test]
fn v8_same_episode_deadline_is_exclusive_and_no_fake_now_or_late_live() {
    let mut p = fixture(DeliveryPolicy::ObservedBoWsHistoricalRecoveryV8, true);
    latch(&mut p);
    assert!(p
        .prepare_ws_recovery(1, T + 646904, 1, T + 646904)
        .unwrap()
        .is_none());
    assert_eq!(p.counters.committed, 0);
}

#[test]
fn v8_ingress_loss_bad_ws_and_real_generation_change_invalidate_late_authority() {
    for case in 0..4 {
        let mut p = fixture(DeliveryPolicy::ObservedBoWsHistoricalRecoveryV8, true);
        latch(&mut p);
        let mut frame = wire(T + 600000, T + 602000);
        frame.owner = p.physical_owner.clone();
        match case {
            0 => {
                assert_eq!(p.ingest(frame, T + 604001), Err(PrimaryError::Continuity));
            }
            1 => {
                frame.rows[0].bar.high = rust_decimal::Decimal::ZERO;
                assert!(p.ingest(frame, T + 602000).is_err());
            }
            2 => {
                frame.rows[0].presence = crate::primary::SnapshotPresence::ExplicitTrue;
                assert_eq!(
                    p.ingest(frame, T + 602000),
                    Err(PrimaryError::UnexpectedHistory)
                );
            }
            3 => {
                p.begin_generation(2, T + 602000).unwrap();
            }
            _ => unreachable!(),
        }
        p.relatch_data_gap();
        assert!(
            !matches!(
                p.prepare_ws_recovery(1, T + 646904, 1, T + 604002),
                Ok(Some(_))
            ),
            "case {case}"
        );
        assert_eq!(p.counters.committed, 0);
    }
}
#[test]
fn v8_boundary_receipt_retains_first_frame_after_forming_boundary_updates() {
    let mut p = fixture(DeliveryPolicy::ObservedBoWsHistoricalRecoveryV8, true);
    let first = p.cache[&(T + 600000)].frame_sha256.clone();
    let mut frame = wire(T + 600000, T + 601500);
    frame.owner = p.physical_owner.clone();
    frame.rows[0].bar.high += rust_decimal::Decimal::ONE;
    p.ingest(frame, T + 601500).unwrap();
    latch(&mut p);
    let a = p
        .prepare_ws_recovery(1, T + 646904, 1, T + 602364)
        .unwrap()
        .unwrap();
    assert_eq!(a.input.receipt().boundary_frame_sha256, first);
    assert_eq!(
        a.input.receipt().canonical.boundary_received_at_ms,
        Some(T + 601364)
    );
}

// Reviewer target regressions: not yet executed in the reviewer environment.
#[test]
fn reviewer_v8_preserves_ten_second_post_admission_budget() {
    let mut p = fixture(DeliveryPolicy::ObservedBoWsHistoricalRecoveryV8, true);
    latch(&mut p);
    let ready = T + 602364;
    let episode = T + 646904;
    let a = p
        .prepare_ws_recovery(1, episode, 1, ready)
        .unwrap()
        .unwrap();
    assert_eq!(
        a.input.receipt().replay_deadline_ms,
        (ready + 10000).min(episode),
        "WS historical publication must not borrow the remaining entire episode"
    );
}

#[test]
fn reviewer_v8_rejects_consumption_after_ten_seconds_without_new_commit() {
    for delay in [9999, 10000, 10001] {
        let mut p = fixture(DeliveryPolicy::ObservedBoWsHistoricalRecoveryV8, true);
        latch(&mut p);
        let ready = T + 602364;
        let a = p
            .prepare_ws_recovery(1, T + 646904, 1, ready)
            .unwrap()
            .unwrap();
        let result = p.recovered_ws(a.input, ready + delay);
        if delay <= 10000 {
            assert!(result.is_ok(), "positive bounded control");
            assert_eq!(p.counters.committed, 1);
        } else {
            assert!(result.is_err(), "late publication must be refused");
            assert_eq!(p.counters.committed, 0);
        }
    }
}

#[test]
fn v8_r1_episode_and_run_deadlines_stay_exclusive_when_clipping_replay() {
    let ready = T + 602364;
    for (episode_delta, run_delta, delay, accepted) in [
        (44540, 60000, 10000, true),
        (7000, 60000, 6999, true),
        (7000, 60000, 7000, false),
        (10000, 60000, 10000, false),
        (44540, 8000, 8000, false),
    ] {
        let mut p = fixture(DeliveryPolicy::ObservedBoWsHistoricalRecoveryV8, true);
        latch(&mut p);
        p.run_deadline_ms = ready + run_delta;
        let a = p
            .prepare_ws_recovery(1, ready + episode_delta, 1, ready)
            .unwrap()
            .unwrap();
        assert_eq!(
            a.input.receipt().replay_deadline_ms,
            (ready + 10000)
                .min(ready + run_delta)
                .min(ready + episode_delta)
        );
        assert_eq!(p.recovered_ws(a.input, ready + delay).is_ok(), accepted);
        assert_eq!(p.counters.committed, u64::from(accepted));
    }
}
