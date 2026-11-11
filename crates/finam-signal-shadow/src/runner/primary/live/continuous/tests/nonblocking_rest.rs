//! Real managed writer/WS tasks, fixture clocks and local duplex peers only.
use super::*;
use crate::{primary::DeliveryPolicy, runner::primary::tests::ProofWaitIo};

#[test]
fn nonblocking_rest_r3_actual_writer_rechecks_ws_after_late_proof_io_without_another_wakeup() {
    for delivery in [
        DeliveryPolicy::ObservedBoRestPrecommitWsFirstV2,
        DeliveryPolicy::ObservedBoNonBlockingRestV3,
        DeliveryPolicy::ObservedBoAdmittedRestV4,
    ] {
        for (elapsed, missing) in [(0, false), (1, false), (0, true)] {
            with_fixture_clock(T - 100, || {
                runtime(async {
                    eprintln!("R3 case {delivery:?} elapsed={elapsed} missing={missing}");
                    let (mut owner, _, root, path) =
                        crate::runner::primary::live::tests::context_delivery(false, delivery);
                    let io = Arc::new(ProofWaitIo::new(elapsed));
                    owner.proof_wait_io = Some(io.clone());
                    let (factory, mut peers, mut progress, _) = setup().await;
                    let release = Arc::new(tokio::sync::Notify::new());
                    let notify = release.clone();
                    let fetch = move |job: Job, _stop: StopHandle| {
                        let release = notify.clone();
                        async move {
                            release.notified().await;
                            // Complete body at 1850; writer IO consumes the last 50ms.
                            let out =
                                source(&job, (0..10).map(|i| raw_row(T + i * 60000)).collect());
                            crate::timing::advance_fixture_clock(50);
                            out
                        }
                    };
                    let task = tokio::spawn(drive(
                        owner,
                        DateTime::from_timestamp_millis(T - 3600000).unwrap(),
                        factory,
                        fetch,
                    ));
                    let mut ack = progress.recv().await.unwrap();
                    let mut peer = peers.remove(0);
                    handshake(&mut peer).await;
                    until(|| {
                        trace(&path)
                            .iter()
                            .any(|r| r["event"]["event"] == "primary_subscription_sent")
                    })
                    .await;
                    for i in 0..9 {
                        go(T + i * 60000 + 100);
                        peer.send(data(T + i * 60000)).await.unwrap();
                        accepted(&mut ack, T + i * 60000 + 100).await;
                    }
                    go(T + 600900);
                    peer.send(data(T + 600000)).await.unwrap();
                    accepted(&mut ack, T + 600900).await;
                    go(T + 601000);
                    until(|| {
                        trace(&path)
                            .iter()
                            .any(|r| r["event"]["event"] == "primary_rest_proof_scheduled")
                    })
                    .await;
                    if !missing {
                        go(T + 601200);
                        peer.send(data(T + 540000)).await.unwrap();
                        accepted(&mut ack, T + 601200).await;
                    }
                    go(T + 601850);
                    release.notify_one();
                    until(|| io.entered.load(Ordering::SeqCst)).await;
                    eprintln!("R3 writer IO entered");
                    assert_eq!(io.before_ms.load(Ordering::SeqCst), T + 601900);
                    assert_eq!(
                        io.after_ms.load(Ordering::SeqCst),
                        T + 601900 + elapsed as i64
                    );
                    go(T + 601900 + elapsed as i64);
                    ack.borrow_and_update();
                    io.release.store(true, Ordering::SeqCst);
                    let expected = u64::from(elapsed == 0 && !missing);
                    if expected == 1 {
                        until(|| {
                            task.is_finished()
                                || trace(&path).iter().any(|r| r["kind"] == "canonical_commit")
                        })
                        .await;
                        // Feedback follows the writer's post-fsync fence.
                        // Do not race STOP against that final check.
                        if !task.is_finished() {
                            ack.changed().await.unwrap();
                        }
                    } else {
                        // Incomplete may wait, stale may recover or stop;
                        // neither can gain a fresh model callback.
                        until(|| {
                            task.is_finished()
                                || ack.has_changed().unwrap_or(false)
                                || trace(&path).iter().any(|r| {
                                    r["event"]["event"] == "primary_recovery_episode_started"
                                })
                        })
                        .await;
                    }
                    root.stop();
                    let report = task.await.unwrap().unwrap();
                    assert!(report.all_workers_drained, "{report:?}");
                    assert_eq!(
                        report.canonical_commits, expected,
                        "{delivery:?} {elapsed} {missing}: {report:?}"
                    );
                    assert_eq!(
                        trace(&path)
                            .iter()
                            .filter(|r| r["kind"] == "model_step")
                            .count(),
                        expected as usize
                    );
                    assert_eq!(report.http.proof.scheduled, 1);
                })
            });
        }
    }
}

#[test]
fn nonblocking_rest_managed_command_isolation_preserves_ws_but_rejects_unbound_authority() {
    for (class, grant) in [
        (RestFailure::Integrity, Some(1)),
        (RestFailure::Authentication, Some(1)),
        (RestFailure::Unknown, Some(1)),
        (RestFailure::Authentication, None),
        (RestFailure::Integrity, None),
    ] {
        runtime(async {
            let root = StopHandle::default();
            let connection = StopHandle::connection_scope();
            let faults = Faults::default();
            let job = Job::Proof(RestTicket {
                owner: Arc::new(()),
                label_ms: T,
                reason: Some(ExceptionReason::SparseUnproven),
                issued_at_ms: crate::timing::now_ms(),
                deadline_ms: crate::timing::now_ms() + 1000,
            });
            let fetch = move |job: Job, stop: StopHandle| async move {
                stop.stop(); // Same as the production executor on a data error.
                HttpOutcome {
                    result: Err(class),
                    receipt: HttpReceipt {
                        started_at_ms: crate::timing::now_ms(),
                        finished_at_ms: crate::timing::now_ms(),
                        deadline_ms: job.deadline_ms(),
                        get_attempts: u64::from(grant.is_some()),
                        auth_generation: grant,
                        body: None,
                        executor_completed: true,
                    },
                }
            };
            let task = launch_managed_mode(
                job,
                fetch,
                connection.clone(),
                root.clone(),
                faults.clone(),
                true,
            );
            let outcome = task.worker.await.unwrap().unwrap();
            assert_eq!(outcome.result.err(), Some(class));
            assert!(!connection.is_stopped());
            assert_eq!(root.is_stopped(), grant.is_none());
            assert_eq!(faults.first().is_some(), grant.is_none());
        });
    }
}

#[test]
fn nonblocking_rest_watchdog_has_unknown_accounting_and_cannot_stop_ws_or_claim_success() {
    runtime(async {
        let root = StopHandle::default();
        let connection = StopHandle::connection_scope();
        let faults = Faults::default();
        let now = crate::timing::now_ms();
        let job = Job::Proof(RestTicket {
            owner: Arc::new(()),
            label_ms: T,
            reason: Some(ExceptionReason::SparseUnproven),
            issued_at_ms: now,
            deadline_ms: now + 5,
        });
        let fetch =
            |_job: Job, _stop: StopHandle| async { std::future::pending::<HttpOutcome>().await };
        let task = launch_managed_mode(
            job.clone(),
            fetch,
            connection.clone(),
            root.clone(),
            faults.clone(),
            true,
        );
        let outcome = task.worker.await.unwrap().unwrap();
        assert_eq!(outcome.result.as_ref().err(), Some(&RestFailure::Timeout));
        assert!(!outcome.receipt.executor_completed);
        let mut totals = HttpTotals::default();
        totals.role(job.role()).scheduled += 1;
        totals.completed(&job, Some(&outcome));
        assert_eq!(totals.proof.completed, 0);
        assert_eq!(totals.proof.cancelled_or_unaccounted, 1);
        assert!(!root.is_stopped() && !connection.is_stopped() && faults.first().is_none());
    });
}
