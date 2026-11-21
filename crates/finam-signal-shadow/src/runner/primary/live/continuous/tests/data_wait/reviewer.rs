use super::*;

// Reviewer-only: a valid historical DATA may arrive while SUBSCRIBE-enabled
// recovery GET is in flight. Actual local WS codec; only REST completion delayed.
#[test]
fn reviewer_v7_bootstrap_during_pending_recovery_get_must_not_role_overlap() {
    with_fixture_clock(T - 100, || {
        runtime(async {
            let (owner, _, root, path) = crate::runner::primary::live::tests::context_delivery(
                false,
                crate::primary::DeliveryPolicy::ObservedBoDataWaitRecoveryV7,
            );
            let (factory, mut peers, mut progress, calls) = setup().await;
            let reads = Arc::new(AtomicUsize::new(0));
            let count = reads.clone();
            let release = Arc::new(tokio::sync::Notify::new());
            let unblock = release.clone();
            let fetch = move |job: Job, stop: StopHandle| {
                let count = count.clone();
                let unblock = unblock.clone();
                async move {
                    if matches!(job, Job::Recovery(_)) && count.fetch_add(1, Ordering::SeqCst) == 0
                    {
                        // Hold only the first GET until historical DATA has
                        // requested bootstrap; then return a valid source.
                        // A corrected driver may retire/reissue this ticket,
                        // but subsequent bounded GETs need no further signal.
                        let started_at_ms = crate::timing::now_ms();
                        tokio::select! { biased;
                            _=stop.cancelled()=>return HttpOutcome {
                                result: Err(RestFailure::Cancelled),
                                receipt: HttpReceipt {
                                    started_at_ms,
                                    finished_at_ms: crate::timing::now_ms(),
                                    deadline_ms: job.deadline_ms(),
                                    get_attempts: 1,
                                    auth_generation: Some(1),
                                    body: None,
                                    executor_completed: true,
                                },
                            },
                            _=unblock.notified()=>(),
                        }
                    }
                    let plan = job.plan().unwrap();
                    let values = (plan.start().timestamp_millis()..plan.end().timestamp_millis())
                        .step_by(60_000)
                        .map(raw_row)
                        .collect();
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
            go(T + 600100);
            peers[0].close(None).await.unwrap();
            let _new_ack = tokio::time::timeout(Duration::from_secs(5), progress.recv())
                .await
                .unwrap()
                .unwrap();
            go(T + 601200);
            handshake(&mut peers[1]).await;
            until(|| reads.load(Ordering::SeqCst) == 1).await;
            // The minute was already seen on the previous physical socket.
            // With no committed/reference proof yet, this ordinary initial
            // historical DATA must take the existing BootstrapBridge path.
            peers[1].send(data(T)).await.unwrap();
            until(|| {
                trace(&path)
                    .iter()
                    .any(|r| r["event"]["event"] == "primary_bootstrap_proof_required")
            })
            .await;
            let running_clock = crate::timing::run_fixture_clock();
            release.notify_one();
            // This DATA traverses the actual new physical socket and therefore
            // has that subscription's owner. If bootstrap is still pending,
            // production ingress waits for its proof before processing it.
            if !root.is_stopped() {
                let sent = peers[1].send(data(T + 600000)).await;
                assert!(
                    sent.is_ok() || root.is_stopped(),
                    "fresh DATA send failed without a terminal production result"
                );
            }
            until(|| {
                root.is_stopped()
                    || trace(&path)
                        .iter()
                        .any(|r| r["event"]["event"] == "primary_recovery_episode_ready")
            })
            .await;
            let reached_ready = trace(&path)
                .iter()
                .any(|r| r["event"]["event"] == "primary_recovery_episode_ready");
            // Successful recovery is a running state; only the harness then
            // requests STOP. Never wait for a healthy runner to end on its own.
            if !root.is_stopped() {
                root.stop();
            }
            let report = tokio::time::timeout(Duration::from_secs(5), task)
                .await
                .unwrap()
                .unwrap()
                .unwrap();
            eprintln!(
                "reviewer V7 pending Recovery + bootstrap: reads={}, ready={}, report={report:?}",
                reads.load(Ordering::SeqCst),
                reached_ready
            );
            eprintln!("reviewer source trace={}", path.display());
            assert_eq!(calls.load(Ordering::SeqCst), 2);
            assert!((1..=usize::from(crate::primary::MAX_REST_READS))
                .contains(&reads.load(Ordering::SeqCst)));
            assert!(report.all_workers_drained, "{report:?}");
            assert!(
                !matches!(report.cause, Cause::RoleOverlap),
                "valid bootstrap DATA while recovery GET pending caused RoleOverlap: {report:?}"
            );
            assert!(
                reached_ready,
                "valid resync/bootstrap/fresh DATA must recover: {report:?}"
            );
            assert!(matches!(report.cause, Cause::RequestedStop), "{report:?}");
            assert_eq!(report.canonical_commits, 1);
            let rows = trace(&path);
            assert_eq!(rows.iter().filter(|r| r["kind"] == "model_step").count(), 1);
            assert!(rows
                .iter()
                .any(|r| r["kind"] == "model_step" && r["event"]["mode"] == "BACKFILL_REPLAY"));
            drop(running_clock);
        })
    });
}

// Reviewer-only: a transient required REST failure does not mean physical WS loss.
#[test]
fn reviewer_v7_transient_unavailable_retries_on_same_ws_before_terminal_hold() {
    with_fixture_clock(T - 100, || {
        runtime(async {
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
                        let ordinal = count.fetch_add(1, Ordering::SeqCst);
                        if ordinal == 0 {
                            let now = crate::timing::now_ms();
                            return HttpOutcome {
                                result: Err(RestFailure::Unavailable),
                                receipt: HttpReceipt {
                                    started_at_ms: now,
                                    finished_at_ms: now,
                                    deadline_ms: job.deadline_ms(),
                                    get_attempts: 1,
                                    auth_generation: Some(1),
                                    body: None,
                                    executor_completed: true,
                                },
                            };
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
            let running_clock = crate::timing::run_fixture_clock();
            until(|| {
                root.is_stopped()
                    || trace(&path)
                        .iter()
                        .any(|r| r["event"]["event"] == "primary_recovery_episode_ready")
            })
            .await;
            // A corrected driver reaches ready after the second source response;
            // stop it explicitly once that bounded recovery has succeeded.
            if !root.is_stopped() {
                root.stop();
            }
            let report = tokio::time::timeout(Duration::from_secs(5), task)
                .await
                .unwrap()
                .unwrap()
                .unwrap();
            eprintln!(
                "reviewer V7 transient REST: reads={}, physical_calls={}, report={report:?}",
                reads.load(Ordering::SeqCst),
                calls.load(Ordering::SeqCst)
            );
            eprintln!("reviewer source trace={}", path.display());
            assert_eq!(
                calls.load(Ordering::SeqCst),
                1,
                "healthy WS must be retained"
            );
            assert!(report.all_workers_drained, "{report:?}");
            assert_eq!(reads.load(Ordering::SeqCst), 2,
                "retryable Unavailable consumed only one GET and stopped despite remaining episode budget: {report:?}");
            assert_eq!(
                report.canonical_commits, 1,
                "one replay only after valid source"
            );
            assert!(matches!(report.cause, Cause::RequestedStop), "{report:?}");
            let rows = trace(&path);
            assert_eq!(rows.iter().filter(|r| r["kind"] == "model_step").count(), 1);
            assert!(rows
                .iter()
                .any(|r| r["kind"] == "model_step" && r["event"]["mode"] == "BACKFILL_REPLAY"));
            drop(running_clock);
        })
    });
}
