//! V7 managed-driver regressions: real local WS codec, stub REST, no FINAM.
use super::*;

// A timeout future may be scheduled before a simultaneously completed state
// predicate. Check the reached state once more at the test-only wait boundary;
// do not change any runner/HTTP/episode deadline.
#[track_caller]
fn until(mut check: impl FnMut() -> bool) -> impl Future<Output = ()> {
    let caller = std::panic::Location::caller();
    async move {
        let result = tokio::time::timeout(Duration::from_secs(5), async {
            while !check() {
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        })
        .await;
        assert!(
            result.is_ok() || check(),
            "fixture state not reached at {caller}"
        );
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Quiet,
    Exhaust,
    Stop,
    ReconnectWait,
    ReconnectFresh,
    MissingMiddle,
    Auth,
    NewFault,
    Deadline,
}

async fn scenario(mode: Mode) {
    let mut running_clock;
    let (owner, _, root, path) = crate::runner::primary::live::tests::context_delivery(
        false,
        crate::primary::DeliveryPolicy::ObservedBoDataWaitRecoveryV7,
    );
    let (factory, mut peers, mut progress, calls) = setup().await;
    let reads = Arc::new(AtomicUsize::new(0));
    let count = reads.clone();
    let fetch = move |job: Job, _stop: StopHandle| {
        let count = count.clone();
        async move {
            let plan = job.plan().unwrap();
            let mut values: Vec<_> = (plan.start().timestamp_millis()
                ..plan.end().timestamp_millis())
                .step_by(60_000)
                .map(raw_row)
                .collect();
            if matches!(job, Job::Recovery(_)) {
                let n = count.fetch_add(1, Ordering::SeqCst);
                if mode == Mode::Auth {
                    return HttpOutcome {
                        result: Err(RestFailure::Authentication),
                        receipt: HttpReceipt {
                            started_at_ms: crate::timing::now_ms(),
                            finished_at_ms: crate::timing::now_ms(),
                            deadline_ms: job.deadline_ms(),
                            get_attempts: 1,
                            auth_generation: Some(1),
                            body: None,
                            executor_completed: true,
                        },
                    };
                }
                if mode == Mode::MissingMiddle {
                    values.remove(4);
                }
                if mode == Mode::Exhaust
                    || (n == 0 && matches!(mode, Mode::Quiet | Mode::Stop | Mode::NewFault))
                {
                    values.pop();
                }
            } else {
                assert!(matches!(job, Job::Proof(_)));
                values.pop();
            }
            source(&job, values)
        }
    };
    let task = tokio::spawn(drive(
        owner,
        DateTime::from_timestamp_millis(T - 3_600_000).unwrap(),
        factory,
        fetch,
    ));
    let mut ack = progress.recv().await.unwrap();
    handshake(&mut peers[0]).await;
    until(|| {
        trace(&path)
            .iter()
            .any(|r| r["event"]["event"] == "primary_subscription_sent")
    })
    .await;
    feed(&mut peers[0], &mut ack, &path, 0, 9, 0).await;
    let physical = matches!(
        mode,
        Mode::ReconnectWait | Mode::ReconnectFresh | Mode::Deadline
    );
    if physical {
        go(T + 600100);
        peers[0].close(None).await.unwrap();
        ack = tokio::time::timeout(Duration::from_secs(5), progress.recv())
            .await
            .unwrap()
            .unwrap();
        go(T + 601200);
        handshake(&mut peers[1]).await;
        until(|| {
            trace(&path).iter().any(|r| {
                r["event"]["event"] == "primary_subscription_sent" && r["event"]["generation"] == 2
            })
        })
        .await;
        // HTTP resync must start WITHOUT a post-subscription live DATA.
        until(|| reads.load(Ordering::SeqCst) > 0).await;
        until(|| {
            trace(&path)
                .iter()
                .any(|r| r["event"]["event"] == "primary_recovery_waiting_first_data")
        })
        .await;
        assert!(ack.borrow().is_none());
        assert!(!trace(&path)
            .iter()
            .any(|r| r["event"]["event"] == "primary_recovery_episode_ready"));
        tokio::time::pause();
        go(T + 607500);
        tokio::time::advance(Duration::from_secs(6)).await;
        tokio::time::resume();
        assert!(
            !root.is_stopped(),
            "5s without a new trade must not reconnect V7"
        );
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        if mode == Mode::ReconnectFresh {
            peers[1].send(data(T + 600000)).await.unwrap();
            accepted(&mut ack, T + 607500).await;
            until(|| {
                trace(&path)
                    .iter()
                    .any(|r| r["event"]["event"] == "primary_recovery_episode_ready")
            })
            .await;
        }
        if mode == Mode::Deadline {
            tokio::time::pause();
            go(T + 647500);
            tokio::time::advance(Duration::from_secs(40)).await;
            tokio::time::resume();
        } else {
            root.stop();
        }
        running_clock = Some(crate::timing::run_fixture_clock());
    } else {
        go(T + 601000);
        peers[0].send(data(T + 540000)).await.unwrap();
        until(|| {
            trace(&path)
                .iter()
                .any(|r| r["event"]["event"] == "primary_rest_observation")
        })
        .await;
        assert!(trace(&path)
            .iter()
            .any(|r| r["event"]["assessment"]["missing_known_m1_ms"] == json!([T + 540000])));
        go(T + 601904);
        peers[0].send(data(T + 540000)).await.unwrap();
        running_clock = Some(crate::timing::run_fixture_clock());
        if !matches!(mode, Mode::MissingMiddle | Mode::Auth) {
            until(|| {
                trace(&path)
                    .iter()
                    .any(|r| r["event"]["event"] == "primary_recovery_data_not_ready")
            })
            .await;
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        if mode == Mode::Stop {
            root.stop();
        }
        if mode == Mode::NewFault {
            peers[0].close(None).await.unwrap();
            ack = tokio::time::timeout(Duration::from_secs(5), progress.recv())
                .await
                .unwrap()
                .unwrap();
            handshake(&mut peers[1]).await;
            until(|| {
                trace(&path)
                    .iter()
                    .any(|r| r["event"]["event"] == "primary_recovery_waiting_first_data")
            })
            .await;
            drop(running_clock.take());
            let fresh_at = (T + 608000).max(crate::timing::now_ms() + 1);
            go(fresh_at);
            peers[1].send(data(T + 600000)).await.unwrap();
            accepted(&mut ack, fresh_at).await;
            until(|| {
                trace(&path)
                    .iter()
                    .any(|r| r["event"]["event"] == "primary_recovery_episode_ready")
            })
            .await;
            root.stop();
        }
        if mode == Mode::Quiet {
            until(|| {
                trace(&path)
                    .iter()
                    .any(|r| r["event"]["event"] == "primary_recovery_episode_ready")
            })
            .await;
            assert_eq!(reads.load(Ordering::SeqCst), 2);
            drop(running_clock.take());
            assert_eq!(
                trace(&path)
                    .iter()
                    .filter(|r| r["kind"] == "model_step")
                    .count(),
                1
            );
            // A slow fixture may already be past the nominal continuation
            // timestamp. Never rewind UTC/monotonic time after a real wait.
            go((T + 610000).max(crate::timing::now_ms()));
            let fresh_at = (T + 604100).max(crate::timing::now_ms() + 1);
            go(fresh_at);
            peers[0].send(data(T + 600000)).await.unwrap();
            accepted(&mut ack, fresh_at).await;
            feed(&mut peers[0], &mut ack, &path, 11, 20, 2).await;
            root.stop();
        }
    }
    let report = tokio::time::timeout(Duration::from_secs(15), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(report.all_workers_drained, "{report:?}");
    if mode == Mode::Exhaust {
        assert_eq!(reads.load(Ordering::SeqCst), 4);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert!(matches!(
            report.cause,
            Cause::Recovery(reconnect::Error::RestTailRequestsExhausted)
        ));
        assert!(trace(&path)
            .iter()
            .any(|r| r["event"]["event"] == "primary_recovery_attempt_failed"
                && r["event"]["audit"]["last_error"] == "RestTailRequestsExhausted"));
    } else if mode == Mode::MissingMiddle {
        assert!(
            matches!(
                report.cause,
                Cause::Coordinator(CoordinatorError::Source(PrimaryError::CanonicalConflict))
            ),
            "{report:?}"
        );
        assert_eq!(reads.load(Ordering::SeqCst), 1);
        assert_eq!(report.canonical_commits, 0);
    } else if mode == Mode::Auth {
        assert!(
            matches!(report.cause, Cause::Http(RestFailure::Authentication)),
            "{report:?}"
        );
        assert_eq!(reads.load(Ordering::SeqCst), 1);
    } else if mode == Mode::Deadline {
        assert!(
            matches!(
                report.cause,
                Cause::Recovery(reconnect::Error::EpisodeDeadline)
            ),
            "{report:?}"
        );
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        assert_eq!(reads.load(Ordering::SeqCst), 1);
        assert!(trace(&path)
            .iter()
            .any(|r| r["event"]["event"] == "primary_recovery_attempt_failed"
                && r["event"]["stage_at_failure"] == "FirstData"
                && r["event"]["audit"]["last_error"] == "EpisodeDeadline"));
    } else {
        assert!(matches!(report.cause, Cause::RequestedStop), "{report:?}");
    }
    if mode == Mode::Quiet {
        assert_eq!(report.canonical_commits, 2);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert!(trace(&path)
            .iter()
            .any(|r| r["kind"] == "model_step" && r["event"]["mode"] == "BACKFILL_REPLAY"));
    }
    if mode == Mode::Stop {
        assert_eq!(reads.load(Ordering::SeqCst), 1);
    }
    if mode == Mode::NewFault {
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        assert_eq!(reads.load(Ordering::SeqCst), 2);
        assert_eq!(report.canonical_commits, 1);
        assert!(trace(&path)
            .iter()
            .any(|r| r["event"]["event"] == "primary_recovery_attempt_failed"
                && r["event"]["audit"]["last_error"]["Transport"] == "Close"));
    }
    drop(running_clock);
}

#[test]
fn v7_quiet_existing_socket_repairs_missing_suffix_without_reconnect_or_duplicate_callback() {
    with_fixture_clock(T - 100, || runtime(scenario(Mode::Quiet)));
}
#[test]
fn v7_missing_tail_exhaustion_is_bounded_and_does_not_reconnect_healthy_ws() {
    with_fixture_clock(T - 100, || runtime(scenario(Mode::Exhaust)));
}
#[test]
fn v7_stop_cancels_data_wait_without_another_get() {
    with_fixture_clock(T - 100, || runtime(scenario(Mode::Stop)));
}
#[test]
fn v7_network_reconnect_fetches_rest_before_first_data_but_does_not_claim_live() {
    with_fixture_clock(T - 100, || runtime(scenario(Mode::ReconnectWait)));
}
#[test]
fn v7_first_fresh_data_after_five_seconds_can_complete_same_bounded_episode() {
    with_fixture_clock(T - 100, || runtime(scenario(Mode::ReconnectFresh)));
}
#[test]
fn v7_missing_middle_is_a_conflict_not_a_retryable_tail() {
    with_fixture_clock(T - 100, || runtime(scenario(Mode::MissingMiddle)));
}
#[test]
fn v7_auth_rejection_during_data_repair_is_terminal() {
    with_fixture_clock(T - 100, || runtime(scenario(Mode::Auth)));
}
#[test]
fn v7_real_socket_fault_promotes_data_wait_to_physical_recovery() {
    with_fixture_clock(T - 100, || runtime(scenario(Mode::NewFault)));
}
#[test]
fn v7_no_first_data_expires_at_original_episode_deadline_and_retains_reason() {
    with_fixture_clock(T - 100, || runtime(scenario(Mode::Deadline)));
}

#[test]
fn v7_terminal_failure_metadata_survives_already_latched_stop_without_market_actions() {
    with_fixture_clock(T - 100, || {
        runtime(async {
            let (owner, _, root, path) = crate::runner::primary::live::tests::context_delivery(
                false,
                crate::primary::DeliveryPolicy::ObservedBoDataWaitRecoveryV7,
            );
            let mut writer = Writer::spawn(
                owner,
                DateTime::from_timestamp_millis(T - 3600000).unwrap(),
                root.clone(),
                Faults::default(),
                FailureSlot::default(),
            )
            .unwrap();
            root.stop();
            writer
                .call(Operation::RecoveryFailure {
                    stage: reconnect::Stage::FirstData,
                    audit: reconnect::Snapshot {
                        episode: 1,
                        attempt: 1,
                        generation: 2,
                        stage: reconnect::Stage::Failed,
                        original: Failure {
                            generation: 1,
                            kind: crate::socket::primary::Kind::Close,
                            liveness: None,
                        },
                        fault_observed_at_ms: T - 45000,
                        deadline_ms: T,
                        transport_unresolved: true,
                        canonical_unresolved: true,
                        last_error: Some(reconnect::Error::EpisodeDeadline),
                        terminal: Some(reconnect::Error::EpisodeDeadline),
                    },
                })
                .await
                .unwrap();
            assert!(writer.join(Instant::now() + Duration::from_secs(1)).await);
            let rows = trace(&path);
            assert!(rows
                .iter()
                .any(|r| r["event"]["event"] == "primary_recovery_attempt_failed"
                    && r["event"]["audit"]["last_error"] == "EpisodeDeadline"));
            assert!(!rows
                .iter()
                .any(|r| matches!(r["kind"].as_str(), Some("model_step" | "canonical_commit"))));
        })
    });
}
