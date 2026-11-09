use super::*;
use crate::runner::primary::{
    recovery_tests::source,
    tests::{fixture, go, T},
};
use crate::session::primary_tests::wire;
fn fault(generation: u64) -> Failure {
    Failure {
        generation,
        kind: Kind::Close,
        liveness: None,
    }
}
fn runtime(work: impl Future<Output = ()>) {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(work);
}
#[test]
fn original_fault_and_both_latches_exist_before_any_recovery_await() {
    fixture(false, |mut owner, _| {
        let mut budget = RunBudget::new(&owner).unwrap();
        let episode = budget.begin(&mut owner, fault(1)).unwrap();
        let audit = episode.audit();
        let snapshot = audit.snapshot();
        assert_eq!(snapshot.original.kind, Kind::Close);
        assert_eq!(snapshot.attempt, 0);
        assert!(snapshot.transport_unresolved && snapshot.canonical_unresolved);
        assert!(owner.processor().gap_unresolved());
        let stop = owner.session.primary_runtime().unwrap().1;
        drop(episode);
        assert!(stop.is_stopped());
        assert_eq!(audit.snapshot().terminal, Some(Error::Cancelled));
    });
}
#[test]
fn security_clock_and_malformed_source_are_not_reconnect_causes() {
    fixture(false, |mut owner, _| {
        for kind in [
            Kind::Lease,
            Kind::Protocol,
            Kind::Clock,
            Kind::Source(PrimaryError::Numeric),
            Kind::Source(PrimaryError::Timestamp),
            Kind::Cancelled,
            Kind::RunDeadline,
        ] {
            let mut budget = RunBudget::new(&owner).unwrap();
            assert_eq!(
                budget
                    .begin(
                        &mut owner,
                        Failure {
                            generation: 1,
                            kind,
                            liveness: None
                        }
                    )
                    .err(),
                Some(Error::NotRecoverable)
            );
            assert!(!owner.processor().gap_unresolved());
        }
    });
}
#[test]
fn three_backoffs_are_exact_and_fourth_attempt_is_forbidden() {
    fixture(false, |mut owner, _| {
        runtime(async {
            tokio::time::pause();
            let mut budget = RunBudget::new(&owner).unwrap();
            let mut episode = budget.begin(&mut owner, fault(1)).unwrap();
            for (ordinal, delay) in [1, 2, 4].into_iter().enumerate() {
                let began = Instant::now();
                let permit = episode.next_attempt().await.unwrap();
                let target = Duration::from_secs(delay);
                assert!(
                    began.elapsed() >= target
                        && began.elapsed() <= target + Duration::from_millis(1)
                );
                assert_eq!(permit.ordinal(), ordinal as u8 + 1);
                assert_eq!(permit.generation(), ordinal as u64 + 2);
                assert_eq!(
                    episode
                        .bounded::<()>(&permit, Stage::FirstData, async { Err(Error::NoFreshData) })
                        .await,
                    Err(Error::NoFreshData)
                );
            }
            assert_eq!(
                episode.next_attempt().await.err(),
                Some(Error::AttemptsExhausted)
            );
            let state = episode.audit().snapshot();
            assert!(state.transport_unresolved && state.canonical_unresolved);
            assert_eq!(state.original.kind, Kind::Close);
        })
    });
}
#[test]
fn stop_interrupts_backoff_without_waiting_or_erasing_original_fault() {
    fixture(false, |mut owner, _| {
        runtime(async {
            tokio::time::pause();
            let stop = owner.session.primary_runtime().unwrap().1;
            let mut budget = RunBudget::new(&owner).unwrap();
            let mut episode = budget.begin(&mut owner, fault(1)).unwrap();
            let began = Instant::now();
            let cancel = async {
                tokio::task::yield_now().await;
                stop.stop();
            };
            let (result, _) = tokio::join!(episode.next_attempt(), cancel);
            assert_eq!(result.err(), Some(Error::Stopped));
            assert_eq!(began.elapsed(), Duration::ZERO);
            assert_eq!(episode.audit().snapshot().original.kind, Kind::Close);
        })
    });
}
#[test]
fn completed_auth_unavailability_differs_from_cancelled_partial_grant() {
    fixture(false, |mut owner, _| {
        runtime(async {
            tokio::time::pause();
            let mut budget = RunBudget::new(&owner).unwrap();
            let mut episode = budget.begin(&mut owner, fault(1)).unwrap();
            let permit = episode.next_attempt().await.unwrap();
            assert_eq!(
                episode
                    .bounded::<()>(&permit, Stage::Auth, async {
                        Err(Error::Auth(PrimaryAuthClass::Unavailable))
                    })
                    .await,
                Err(Error::Auth(PrimaryAuthClass::Unavailable))
            );
            let next = episode.next_attempt().await.unwrap();
            assert_eq!(
                episode
                    .bounded::<()>(&next, Stage::Auth, std::future::pending())
                    .await,
                Err(Error::Cancelled)
            );
            assert_eq!(episode.next_attempt().await.err(), Some(Error::Cancelled));
        })
    });
}
#[test]
fn total_episode_budget_includes_all_stages_and_never_restarts_at_connect() {
    fixture(false, |mut owner, _| {
        runtime(async {
            tokio::time::pause();
            let mut budget = RunBudget::new(&owner).unwrap();
            let mut episode = budget.begin(&mut owner, fault(1)).unwrap();
            let began = Instant::now();
            for _ in 0..2 {
                let permit = episode.next_attempt().await.unwrap();
                for stage in [
                    Stage::Auth,
                    Stage::Connect,
                    Stage::Subscribe,
                    Stage::Bootstrap,
                ] {
                    episode
                        .bounded(&permit, stage, async {
                            tokio::time::sleep(Duration::from_secs(4)).await;
                            Ok(())
                        })
                        .await
                        .unwrap();
                }
                assert_eq!(
                    episode
                        .bounded::<()>(&permit, Stage::FirstData, async {
                            tokio::time::sleep(Duration::from_secs(4)).await;
                            Err(Error::NoFreshData)
                        })
                        .await,
                    Err(Error::NoFreshData)
                );
            }
            assert_eq!(
                episode.next_attempt().await.err(),
                Some(Error::EpisodeDeadline)
            );
            // Tokio's millisecond timer wheel may schedule the refusal 1ms
            // later. The stored deadline itself is still exactly 45 seconds.
            assert_eq!(episode.audit().snapshot().deadline_ms, T - 100 + 45000);
            assert!(
                began.elapsed() >= Duration::from_secs(45)
                    && began.elapsed() <= Duration::from_millis(45001)
            );
        })
    });
}

#[test]
fn r1_unfinished_resync_still_expires_at_the_original_45_second_budget() {
    fixture(false, |mut owner, _| {
        runtime(async {
            tokio::time::pause();
            let mut budget = RunBudget::new(&owner).unwrap();
            let mut episode = budget.begin(&mut owner, fault(1)).unwrap();
            let started = Instant::now();
            let attempt = episode.next_attempt().await.unwrap();
            let deadline = episode.deadline();
            assert_eq!(deadline.duration_since(started), Duration::from_secs(45));
            // Earlier establishment work has consumed the budget. Resync must
            // not receive a new 45s (nor even a fresh full 5s command budget).
            tokio::time::advance(Duration::from_secs(43)).await;
            assert_eq!(
                episode
                    .bounded::<()>(&attempt, Stage::Resync, std::future::pending())
                    .await,
                Err(Error::EpisodeDeadline)
            );
            assert!(Instant::now() >= deadline);
            assert!(episode.audit().snapshot().canonical_unresolved);
            assert!(owner.processor().gap_unresolved());
        })
    });
}

#[test]
fn one_attempt_cannot_hide_repeated_connect_or_rest_commands() {
    fixture(false, |mut owner, _| {
        runtime(async {
            tokio::time::pause();
            let mut budget = RunBudget::new(&owner).unwrap();
            let mut episode = budget.begin(&mut owner, fault(1)).unwrap();
            let permit = episode.next_attempt().await.unwrap();
            episode
                .bounded(&permit, Stage::Connect, async { Ok(()) })
                .await
                .unwrap();
            assert_eq!(
                episode
                    .bounded::<()>(&permit, Stage::Connect, async {
                        panic!("repeated command must not be polled")
                    })
                    .await,
                Err(Error::Binding)
            );
            assert_eq!(episode.next_attempt().await.err(), Some(Error::Binding));
        })
    });
}
#[test]
fn handshake_history_and_pong_do_not_clear_transport_or_canonical() {
    fixture(false, |mut owner, _| {
        runtime(async {
            tokio::time::pause();
            let mut budget = RunBudget::new(&owner).unwrap();
            let mut episode = budget.begin(&mut owner, fault(1)).unwrap();
            let permit = episode.next_attempt().await.unwrap();
            let sub = crate::socket::primary::fixture_subscription(
                permit.generation(),
                crate::timing::now_ms(),
            );
            assert_eq!(
                episode.transport_ready(&permit, sub, None),
                Err(Error::NoFreshData)
            );
            let state = episode.audit().snapshot();
            assert!(state.transport_unresolved && state.canonical_unresolved);
        })
    });
}

async fn repaired(
    owner: &mut Coordinator,
    budget: &mut RunBudget,
    cutover: i64,
    fail_after_resolution: bool,
) -> Audit {
    go(cutover + 100);
    let mut episode = budget
        .begin(owner, fault(owner.processor().generation()))
        .unwrap();
    let permit = episode.next_attempt().await.unwrap();
    owner
        .processor
        .begin_generation(permit.generation(), crate::timing::now_ms())
        .unwrap();
    let subscription =
        crate::socket::primary::fixture_subscription(permit.generation(), crate::timing::now_ms());
    owner.bind_subscription(&subscription).unwrap();
    go(cutover + 200);
    let mut frame = wire(cutover, cutover + 200);
    frame.generation = permit.generation();
    frame.owner = Some(subscription.owner.clone());
    owner.on_frame(frame).unwrap();
    let data = owner.processor().last_live_data();
    episode
        .transport_ready(&permit, subscription, data.as_ref())
        .unwrap();
    let state = episode.audit().snapshot();
    assert!(!state.transport_unresolved && state.canonical_unresolved);
    assert!(owner.processor().gap_unresolved());
    let plan = owner.plan_recovery(cutover).unwrap();
    let source = source(&plan);
    owner.complete_recovery(&plan, &source).unwrap();
    let receipt = owner.resolve_recovery().unwrap().unwrap();
    if fail_after_resolution {
        owner.latch_recovery().unwrap();
        assert_eq!(
            episode.canonical_ready(owner, &permit, &receipt),
            Err(Error::Binding)
        );
    } else {
        episode.canonical_ready(owner, &permit, &receipt).unwrap();
    }
    episode.audit()
}
#[test]
fn only_source_committed_repair_of_the_same_physical_generation_resolves_both_latches() {
    fixture(false, |mut owner, _| {
        runtime(async {
            tokio::time::pause();
            let mut budget = RunBudget::new(&owner).unwrap();
            let audit = repaired(&mut owner, &mut budget, T + 2 * 600000, false).await;
            let state = audit.snapshot();
            assert!(!state.transport_unresolved && !state.canonical_unresolved);
            assert_eq!(state.stage, Stage::Ready);
            assert!(!owner.session.primary_runtime().unwrap().1.is_stopped());
        })
    });
}
#[test]
fn a_new_fault_invalidates_old_success_receipt_even_on_same_connection() {
    fixture(false, |mut owner, _| {
        runtime(async {
            tokio::time::pause();
            let mut budget = RunBudget::new(&owner).unwrap();
            let audit = repaired(&mut owner, &mut budget, T + 2 * 600000, true).await;
            assert!(audit.snapshot().canonical_unresolved);
            assert_eq!(audit.snapshot().terminal, Some(Error::Binding));
            assert!(owner.processor().gap_unresolved());
        })
    });
}
#[test]
fn fourth_recovery_episode_is_not_authorized() {
    fixture(false, |mut owner, _| {
        runtime(async {
            tokio::time::pause();
            let mut budget = RunBudget::new(&owner).unwrap();
            for i in 1..=3 {
                let state = repaired(&mut owner, &mut budget, T + i * 2 * 600000, false)
                    .await
                    .snapshot();
                assert_eq!(state.episode, i as u8);
            }
            let generation = owner.processor().generation();
            assert_eq!(
                budget.begin(&mut owner, fault(generation)).err(),
                Some(Error::RunEpisodesExhausted)
            );
            assert!(owner.processor().gap_unresolved());
            assert!(owner.session.primary_runtime().unwrap().1.is_stopped());
        })
    });
}
