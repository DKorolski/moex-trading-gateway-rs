//! Managed writer/duplex peers, no provider or operational execution.
use super::*;
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Exhaust,
    Stop,
    NewFault,
    Auth,
    ReplyWake,
    ReplyStop,
    ReplyDeadline,
}

async fn scenario(mode: Mode) {
    let (mut owner, _, root, path) = crate::runner::primary::live::tests::context_delivery(
        false,
        crate::primary::DeliveryPolicy::ObservedBoRestTailRecoveryV6,
    );
    let pause = Arc::new(crate::runner::primary::writer::tests::Pause::new());
    let close = T + 1800000;
    if matches!(
        mode,
        Mode::ReplyWake | Mode::ReplyStop | Mode::ReplyDeadline
    ) {
        owner.frame_reply_pause = Some((close + 400, pause.clone()));
    }
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
        Mode::ReplyWake | Mode::ReplyStop | Mode::ReplyDeadline => {
            go(close + 400);
            let mut value: Value = serde_json::from_str(&data(close).into_text().unwrap()).unwrap();
            value["payload"]["bars"][0]["volume"] = json!({"value":"3"});
            peers[1]
                .send(Message::Text(value.to_string()))
                .await
                .unwrap();
            until(|| pause.entered.load(Ordering::SeqCst)).await;
            if mode == Mode::ReplyDeadline {
                // The actual Tokio episode deadline, not a test classification.
                tokio::time::pause();
                go(close + 46000);
                tokio::time::advance(Duration::from_secs(46)).await;
                tokio::time::resume();
                until(|| root.is_stopped()).await;
            } else if mode == Mode::ReplyStop {
                root.stop();
            } else {
                go(close + 1600);
                tokio::time::sleep(Duration::from_millis(1100)).await;
                // The reply is still owned despite crossing normal wake.
                assert!(!root.is_stopped());
            }
            pause.release.store(true, Ordering::SeqCst);
            if mode == Mode::ReplyWake {
                until(|| {
                    trace(&path)
                        .iter()
                        .any(|r| r["event"]["event"] == "primary_recovery_episode_ready")
                })
                .await;
                assert!(!root.is_stopped());
                for i in 31..=40 {
                    let at = T + i * 60000;
                    go(at + 100);
                    peers[1].send(data(at)).await.unwrap();
                    accepted(&mut ack2, at + 100).await;
                    if i == 40 {
                        go(at + 1100);
                        peers[1].send(data(at)).await.unwrap();
                        until(|| {
                            trace(&path)
                                .iter()
                                .filter(|r| r["kind"] == "canonical_commit")
                                .count()
                                == 4
                        })
                        .await;
                    }
                }
                // Observing a durable file write is not the same as receiving
                // the writer's completed Update. A fresh following Frame ACK
                // proves that the live commit reply reached the engine before
                // the test requests STOP (otherwise it races publication).
                let following = T + 41 * 60000;
                go(following + 100);
                peers[1].send(data(following)).await.unwrap();
                accepted(&mut ack2, following + 100).await;
                assert!(!root.is_stopped());
                root.stop();
            }
        }
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
        match mode {
            Mode::NewFault => 3,
            Mode::ReplyWake => 4,
            _ => 2,
        }
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
            Mode::ReplyStop | Mode::ReplyDeadline => 1,
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
    if mode == Mode::ReplyWake {
        assert_eq!(
            trace(&path)
                .iter()
                .filter(|r| r["kind"] == "canonical_source"
                    && r["event"]["receipt"]["origin"] == "RECOVERY_BACKFILL")
                .count(),
            1
        );
    }
    if mode == Mode::ReplyDeadline {
        assert!(
            matches!(
                report.cause,
                Cause::Recovery(reconnect::Error::EpisodeDeadline)
            ),
            "{report:?}"
        );
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

#[test]
fn tail_v6_r1_submitted_frame_reply_survives_backoff_wake_and_continues() {
    with_fixture_clock(T - 100, || runtime(scenario(Mode::ReplyWake)));
}
#[test]
fn tail_v6_r1_submitted_frame_reply_still_honors_operator_stop() {
    with_fixture_clock(T - 100, || runtime(scenario(Mode::ReplyStop)));
}
#[test]
fn tail_v6_r1_submitted_frame_reply_still_honors_episode_deadline() {
    with_fixture_clock(T - 100, || runtime(scenario(Mode::ReplyDeadline)));
}
