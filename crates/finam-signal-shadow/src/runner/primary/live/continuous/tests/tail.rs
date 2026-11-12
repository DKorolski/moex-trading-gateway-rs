//! Managed writer/duplex peers, no provider or operational execution.
use super::*;
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Exhaust,
    Stop,
    NewFault,
    Auth,
}

async fn scenario(mode: Mode) {
    let (owner, _, root, path) = crate::runner::primary::live::tests::context_delivery(
        false,
        crate::primary::DeliveryPolicy::ObservedBoRestTailRecoveryV6,
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
                .step_by(60000)
                .map(raw_row)
                .collect();
            if matches!(&job, Job::Recovery(_)) {
                let n = count.fetch_add(1, Ordering::SeqCst);
                if mode == Mode::Auth && n > 0 {
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
                if n == 0 || mode == Mode::Exhaust {
                    values.truncate(values.len() - 3);
                }
            }
            source(&job, values)
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
    feed(&mut peers[0], &mut ack, &path, 0, 29, 0).await;
    let close = T + 1800000;
    go(close + 100);
    peers[0].close(None).await.unwrap();
    let mut ack2 = tokio::time::timeout(Duration::from_secs(5), progress.recv())
        .await
        .unwrap()
        .unwrap();
    go(close + 200);
    handshake(&mut peers[1]).await;
    until(|| {
        trace(&path).iter().any(|r| {
            r["event"]["event"] == "primary_subscription_sent" && r["event"]["generation"] == 2
        })
    })
    .await;
    go(close + 300);
    peers[1].send(data(close)).await.unwrap();
    accepted(&mut ack2, close + 300).await;
    until(|| {
        trace(&path)
            .iter()
            .any(|r| r["event"]["event"] == "primary_recovery_data_not_ready")
    })
    .await;
    assert_eq!(
        trace(&path)
            .iter()
            .filter(|r| r["kind"] == "canonical_commit")
            .count(),
        2
    );
    match mode {
        Mode::Stop => root.stop(),
        Mode::Exhaust => {
            for n in 1..4 {
                go(close + 300 + n * 6000);
                peers[1].send(data(close)).await.unwrap();
                until(|| {
                    trace(&path)
                        .iter()
                        .filter(|r| r["event"]["event"] == "primary_recovery_data_not_ready")
                        .count()
                        > n as usize
                })
                .await;
            }
            until(|| task.is_finished()).await;
        }
        Mode::Auth => {
            go(close + 6300);
            peers[1].send(data(close)).await.unwrap();
            until(|| task.is_finished()).await;
        }
        Mode::NewFault => {
            go(close + 1300);
            peers[1].close(None).await.unwrap();
            let mut ack3 = tokio::time::timeout(Duration::from_secs(5), progress.recv())
                .await
                .unwrap()
                .unwrap();
            go(close + 1400);
            handshake(&mut peers[2]).await;
            until(|| {
                trace(&path).iter().any(|r| {
                    r["event"]["event"] == "primary_subscription_sent"
                        && r["event"]["generation"] == 3
                })
            })
            .await;
            go(close + 1500);
            peers[2].send(data(close)).await.unwrap();
            accepted(&mut ack3, close + 1500).await;
            until(|| {
                trace(&path)
                    .iter()
                    .any(|r| r["event"]["event"] == "primary_recovery_episode_ready")
            })
            .await;
            root.stop();
        }
    }
    let report = tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(report.all_workers_drained, "{report:?}");
    assert_eq!(
        report.canonical_commits,
        if mode == Mode::NewFault { 3 } else { 2 }
    );
    assert_eq!(
        calls.load(Ordering::SeqCst),
        if mode == Mode::NewFault { 3 } else { 2 }
    );
    assert_eq!(
        reads.load(Ordering::SeqCst),
        match mode {
            Mode::Exhaust => 4,
            Mode::Stop => 1,
            _ => 2,
        }
    );
    if mode == Mode::Exhaust {
        assert!(matches!(
            report.cause,
            Cause::Recovery(reconnect::Error::RestTailRequestsExhausted)
        ));
    }
    if mode == Mode::Auth {
        assert!(matches!(
            report.cause,
            Cause::Http(RestFailure::Authentication)
        ));
    }
}

#[test]
fn tail_v6_managed_exhaustion_has_four_reads_and_no_rest_only_reconnect() {
    with_fixture_clock(T - 100, || runtime(scenario(Mode::Exhaust)));
}
#[test]
fn tail_v6_managed_stop_interrupts_source_backoff_without_another_get() {
    with_fixture_clock(T - 100, || runtime(scenario(Mode::Stop)));
}
#[test]
fn tail_v6_managed_new_socket_fault_invalidates_old_source_ticket() {
    with_fixture_clock(T - 100, || runtime(scenario(Mode::NewFault)));
}
#[test]
fn tail_v6_managed_auth_failure_during_retry_is_terminal() {
    with_fixture_clock(T - 100, || runtime(scenario(Mode::Auth)));
}
