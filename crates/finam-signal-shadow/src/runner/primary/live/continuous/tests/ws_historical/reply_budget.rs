//! A real submitted V8 Frame completes once, then its reply is deliberately
//! paused. Ordinary readiness/backoff wakes must not abandon that future.
use super::*;

async fn reply(mode: u8) {
    let (mut owner, _, root, path) = crate::runner::primary::live::tests::context_delivery(
        false,
        DeliveryPolicy::ObservedBoWsHistoricalRecoveryV8,
    );
    let close = T + 1800000;
    let pause = Arc::new(crate::runner::primary::writer::tests::Pause::new());
    owner.frame_reply_pause = Some((close + 2364, pause.clone()));
    let (factory, mut peers, mut progress, calls) = setup().await;
    let fetch = |job: Job, _stop: StopHandle| async move {
        let plan = job.plan().unwrap();
        let mut rows: Vec<_> = (plan.start().timestamp_millis()..plan.end().timestamp_millis())
            .step_by(60000)
            .map(raw_row)
            .collect();
        rows.truncate(if matches!(&job, Job::Recovery(_)) {
            22
        } else {
            2
        });
        source(&job, rows)
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
    go(close + 1000);
    peers[0].send(data(close - 60000)).await.unwrap();
    until(|| {
        trace(&path)
            .iter()
            .any(|r| r["event"]["event"] == "primary_rest_observation")
    })
    .await;
    go(close + 1364);
    peers[0].send(data(close)).await.unwrap();
    accepted(&mut ack, close + 1364).await;
    go(close + 1904);
    peers[0].send(data(close - 60000)).await.unwrap();
    until(|| {
        trace(&path)
            .iter()
            .any(|r| r["event"]["event"] == "primary_recovery_data_not_ready")
    })
    .await;
    go(close + 2364);
    peers[0].send(data(close)).await.unwrap();
    until(|| pause.entered.load(Ordering::SeqCst)).await;
    assert_eq!(
        trace(&path)
            .iter()
            .filter(|r| r["kind"] == "canonical_commit")
            .count(),
        3
    );
    match mode {
        0 => {
            go(close + 3564);
            tokio::time::sleep(Duration::from_millis(1100)).await;
            assert!(
                !root.is_stopped(),
                "a normal wake cannot drop the submitted reply"
            );
            pause.release.store(true, Ordering::SeqCst);
            until(|| {
                trace(&path)
                    .iter()
                    .any(|r| r["event"]["event"] == "primary_recovery_episode_ready")
            })
            .await;
            root.stop();
        }
        1 => root.stop(),
        _ => {
            tokio::time::pause();
            go(close + 12400);
            tokio::time::advance(Duration::from_secs(11)).await;
            tokio::time::resume();
            until(|| root.is_stopped()).await;
            assert!(!trace(&path)
                .iter()
                .any(|r| r["event"]["event"] == "primary_recovery_episode_ready"));
        }
    }
    let _clock = crate::timing::run_fixture_clock();
    let report = tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(report.all_workers_drained, "{report:?}");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    let records = trace(&path);
    assert_eq!(
        records.iter().filter(|r| r["kind"] == "model_step").count(),
        3
    );
    assert_eq!(
        records
            .iter()
            .filter(|r| r["kind"] == "canonical_commit")
            .count(),
        3
    );
    if mode == 2 {
        assert!(
            matches!(
                report.cause,
                Cause::Recovery(reconnect::Error::EpisodeDeadline)
            ),
            "{report:?}"
        );
    }
    let _ = std::fs::remove_file(path);
}
#[test]
fn v8_r1_submitted_ws_reply_survives_readiness_and_backoff_wake() {
    with_fixture_clock(T - 100, || runtime(reply(0)));
}
#[test]
fn v8_r1_submitted_ws_reply_obeys_stop_and_is_drained() {
    with_fixture_clock(T - 100, || runtime(reply(1)));
}
#[test]
fn v8_r1_submitted_ws_reply_is_bounded_by_ten_seconds_not_entire_episode() {
    with_fixture_clock(T - 100, || runtime(reply(2)));
}
