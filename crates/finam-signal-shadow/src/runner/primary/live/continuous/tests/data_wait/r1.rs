use super::*;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Transient {
    Unavailable,
    Timeout,
    Exhaust,
    ExhaustTimeout,
    Stop,
    BadReceipt,
    PhysicalFault,
}

async fn transient(mode: Transient) {
    let (owner, _, root, path) = crate::runner::primary::live::tests::context_delivery(
        false,
        crate::primary::DeliveryPolicy::ObservedBoDataWaitRecoveryV7,
    );
    let (factory, mut peers, mut progress, calls) = setup().await;
    let reads = Arc::new(AtomicUsize::new(0));
    let counter = reads.clone();
    let fetch = move |job: Job, _stop: StopHandle| {
        let counter = counter.clone();
        async move {
            let plan = job.plan().unwrap();
            let mut rows: Vec<_> = (plan.start().timestamp_millis()..plan.end().timestamp_millis())
                .step_by(60000)
                .map(raw_row)
                .collect();
            if matches!(job, Job::Recovery(_)) {
                let n = counter.fetch_add(1, Ordering::SeqCst);
                if n == 0 || matches!(mode, Transient::Exhaust | Transient::ExhaustTimeout) {
                    let now = crate::timing::now_ms();
                    return HttpOutcome {
                        result: Err(
                            if matches!(mode, Transient::Timeout | Transient::ExhaustTimeout) {
                                RestFailure::Timeout
                            } else {
                                RestFailure::Unavailable
                            },
                        ),
                        receipt: HttpReceipt {
                            started_at_ms: now,
                            finished_at_ms: now,
                            deadline_ms: if mode == Transient::BadReceipt {
                                job.deadline_ms() - 1
                            } else {
                                job.deadline_ms()
                            },
                            get_attempts: 1,
                            auth_generation: Some(1),
                            body: None,
                            executor_completed: true,
                        },
                    };
                }
            } else {
                rows.pop();
            }
            source(&job, rows)
        }
    };
    let task = tokio::spawn(drive(
        owner,
        DateTime::from_timestamp_millis(T - 3600000).unwrap(),
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
    go(T + 601000);
    peers[0].send(data(T + 540000)).await.unwrap();
    until(|| {
        trace(&path)
            .iter()
            .any(|r| r["event"]["event"] == "primary_rest_observation")
    })
    .await;
    go(T + 601904);
    peers[0].send(data(T + 540000)).await.unwrap();
    let _clock = crate::timing::run_fixture_clock();
    if mode == Transient::Stop {
        until(|| {
            trace(&path)
                .iter()
                .any(|r| r["event"]["event"] == "primary_recovery_request_failed")
        })
        .await;
        root.stop();
    } else if matches!(
        mode,
        Transient::Unavailable | Transient::Timeout | Transient::PhysicalFault
    ) {
        if mode == Transient::PhysicalFault {
            until(|| {
                trace(&path)
                    .iter()
                    .any(|r| r["event"]["event"] == "primary_recovery_request_failed")
            })
            .await;
            peers[0].close(None).await.unwrap();
            let _ack2 = progress.recv().await.unwrap();
            handshake(&mut peers[1]).await;
            until(|| {
                trace(&path)
                    .iter()
                    .any(|r| r["event"]["event"] == "primary_recovery_waiting_first_data")
            })
            .await;
            peers[1].send(data(T + 600000)).await.unwrap();
        }
        until(|| {
            trace(&path)
                .iter()
                .any(|r| r["event"]["event"] == "primary_recovery_episode_ready")
        })
        .await;
        root.stop();
    }
    let report = tokio::time::timeout(Duration::from_secs(15), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(report.all_workers_drained, "{report:?}");
    assert_eq!(
        calls.load(Ordering::SeqCst),
        if mode == Transient::PhysicalFault {
            2
        } else {
            1
        },
        "data failures never reconnect a healthy WS"
    );
    assert!(report.unresolved_transport_fault.is_none(), "{report:?}");
    let rows = trace(&path);
    let failed: Vec<_> = rows
        .iter()
        .filter(|r| r["event"]["event"] == "primary_recovery_request_failed")
        .collect();
    let success = matches!(
        mode,
        Transient::Unavailable | Transient::Timeout | Transient::PhysicalFault
    );
    let exhausted = matches!(mode, Transient::Exhaust | Transient::ExhaustTimeout);
    assert_eq!(
        reads.load(Ordering::SeqCst),
        if success {
            2
        } else if exhausted {
            4
        } else {
            1
        }
    );
    assert_eq!(
        failed.len(),
        if mode == Transient::BadReceipt {
            0
        } else if exhausted {
            4
        } else {
            1
        }
    );
    for failure in failed {
        let hash = &failure["event"]["ticket_sha256"];
        assert!(!rows.iter().any(
            |r| r["event"]["event"] == "primary_recovery_source_validated"
                && &r["event"]["plan_sha256"] == hash
        ));
        assert_eq!(failure["event"]["callback"], false);
        assert_eq!(failure["event"]["validated_source"], false);
    }
    assert_eq!(report.canonical_commits, u64::from(success));
    let shutdown = rows
        .iter()
        .find(|r| r["event"]["event"] == "primary_async_shutdown")
        .unwrap();
    assert_eq!(shutdown["event"]["unresolved_canonical_gap"], !success);
    if exhausted {
        assert!(matches!(
            report.cause,
            Cause::Recovery(reconnect::Error::RestTailRequestsExhausted)
        ));
    } else if mode == Transient::BadReceipt {
        assert!(matches!(report.cause, Cause::Http(RestFailure::Integrity)));
    } else {
        assert!(matches!(report.cause, Cause::RequestedStop));
    }
}

#[test]
fn v7_r1_unavailable_then_valid_retains_same_ws_and_one_replay() {
    with_fixture_clock(T - 100, || runtime(transient(Transient::Unavailable)));
}
#[test]
fn v7_r1_timeout_then_valid_retains_same_ws_and_one_replay() {
    with_fixture_clock(T - 100, || runtime(transient(Transient::Timeout)));
}
#[test]
fn v7_r1_unavailable_exhausts_four_reads_without_transport_fault() {
    with_fixture_clock(T - 100, || runtime(transient(Transient::Exhaust)));
}
#[test]
fn v7_r1_timeout_exhausts_four_reads_without_transport_fault() {
    with_fixture_clock(T - 100, || runtime(transient(Transient::ExhaustTimeout)));
}
#[test]
fn v7_r1_stop_during_transient_backoff_issues_no_next_read() {
    with_fixture_clock(T - 100, || runtime(transient(Transient::Stop)));
}
#[test]
fn v7_r1_invalid_failure_receipt_is_terminal_not_renewal() {
    with_fixture_clock(T - 100, || runtime(transient(Transient::BadReceipt)));
}
#[test]
fn v7_r1_real_fault_during_transient_backoff_promotes_without_resetting_reads() {
    with_fixture_clock(T - 100, || runtime(transient(Transient::PhysicalFault)));
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Order {
    Before,
    After,
    Stop,
}
async fn bootstrap(order: Order) {
    let (owner, _, root, path) = crate::runner::primary::live::tests::context_delivery(
        false,
        crate::primary::DeliveryPolicy::ObservedBoDataWaitRecoveryV7,
    );
    let (factory, mut peers, mut progress, calls) = setup().await;
    let release = Arc::new(tokio::sync::Notify::new());
    let unblock = release.clone();
    let fetch = move |job: Job, stop: StopHandle| {
        let unblock = unblock.clone();
        async move {
            if matches!(job, Job::Recovery(_)) && order != Order::After {
                tokio::select! { _=unblock.notified()=>(), _=stop.cancelled()=>return cancelled(&job) }
            }
            let plan = job.plan().unwrap();
            source(
                &job,
                (plan.start().timestamp_millis()..plan.end().timestamp_millis())
                    .step_by(60000)
                    .map(raw_row)
                    .collect(),
            )
        }
    };
    let task = tokio::spawn(drive(
        owner,
        DateTime::from_timestamp_millis(T - 3600000).unwrap(),
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
    go(T + 600100);
    peers[0].close(None).await.unwrap();
    let _ack2 = progress.recv().await.unwrap();
    go(T + 601200);
    handshake(&mut peers[1]).await;
    let expected = if order == Order::After {
        "primary_recovery_waiting_first_data"
    } else {
        "primary_recovery_request_issued"
    };
    until(|| trace(&path).iter().any(|r| r["event"]["event"] == expected)).await;
    peers[1].send(data(T - 60000)).await.unwrap();
    until(|| {
        trace(&path)
            .iter()
            .any(|r| r["event"]["event"] == "primary_bootstrap_proof_required")
    })
    .await;
    let _clock = crate::timing::run_fixture_clock();
    if order == Order::Stop {
        root.stop();
    } else {
        release.notify_one();
        until(|| {
            trace(&path)
                .iter()
                .any(|r| r["event"]["event"] == "primary_bootstrap_proof_applied")
        })
        .await;
        assert!(!trace(&path)
            .iter()
            .any(|r| r["event"]["event"] == "primary_recovery_episode_ready"));
        peers[1].send(data(T + 600000)).await.unwrap();
        until(|| {
            trace(&path)
                .iter()
                .any(|r| r["event"]["event"] == "primary_recovery_episode_ready")
        })
        .await;
        root.stop();
    }
    let report = tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(report.all_workers_drained, "{report:?}");
    assert!(matches!(report.cause, Cause::RequestedStop), "{report:?}");
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert_eq!(report.http.recovery.scheduled, 1);
    assert_eq!(
        report.http.bootstrap.scheduled,
        u64::from(order != Order::Stop)
    );
    assert_eq!(report.canonical_commits, u64::from(order != Order::Stop));
}
#[test]
fn v7_r1_bootstrap_before_recovery_completion_is_mandatory_and_correlated() {
    with_fixture_clock(T - 100, || runtime(bootstrap(Order::Before)));
}
#[test]
fn v7_r1_bootstrap_after_recovery_completion_does_not_claim_live() {
    with_fixture_clock(T - 100, || runtime(bootstrap(Order::After)));
}
#[test]
fn v7_r1_stop_with_deferred_bootstrap_drains_without_issuing_it() {
    with_fixture_clock(T - 100, || runtime(bootstrap(Order::Stop)));
}

fn cancelled(job: &Job) -> HttpOutcome {
    let now = crate::timing::now_ms();
    HttpOutcome {
        result: Err(RestFailure::Cancelled),
        receipt: HttpReceipt {
            started_at_ms: now,
            finished_at_ms: now,
            deadline_ms: job.deadline_ms(),
            get_attempts: 1,
            auth_generation: Some(1),
            body: None,
            executor_completed: true,
        },
    }
}
