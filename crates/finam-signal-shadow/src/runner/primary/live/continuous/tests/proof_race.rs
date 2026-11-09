//! Controlled HTTP stub races through the managed physical WS/writer driver.
use super::*;

#[test]
fn r3_io_crossing_inside_managed_writer_preserves_due_without_external_wakeup() {
    use crate::runner::primary::tests::ProofWaitIo;
    for (elapsed, mode) in [
        (1, "match"),
        (2, "match"),
        (5, "match"),
        (2, "mismatch"),
        (5802, "late"),
        (2, "stop"),
    ] {
        with_fixture_clock(T - 100, || {
            runtime(async {
                let (mut owner, _, root, path) = context(false);
                let io = Arc::new(ProofWaitIo::new(elapsed));
                owner.proof_wait_io = Some(io.clone());
                let (factory, mut peers, mut progress, calls) = setup().await;
                let release = Arc::new(tokio::sync::Notify::new());
                let fetch_release = release.clone();
                let fetch = move |job: Job, _stop: StopHandle| {
                    let release = fetch_release.clone();
                    async move {
                        assert!(matches!(job, Job::Proof(_)));
                        release.notified().await;
                        let plan = job.plan().unwrap();
                        let mut rows: Vec<_> = (plan.start().timestamp_millis()
                            ..plan.end().timestamp_millis())
                            .step_by(60000)
                            .map(raw_row)
                            .collect();
                        if mode == "mismatch" {
                            rows[0]["volume"] = json!({"value":"3"});
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
                let mut peer = peers.remove(0);
                handshake(&mut peer).await;
                until(|| {
                    trace(&path)
                        .iter()
                        .any(|r| r["event"]["event"] == "primary_subscription_sent")
                })
                .await;
                for i in 0..10 {
                    go(T + i * 60000 + 100);
                    peer.send(data(T + i * 60000)).await.unwrap();
                    accepted(&mut ack, T + i * 60000 + 100).await;
                }
                go(T + 603000);
                peer.send(data(T + 540000)).await.unwrap();
                until(|| {
                    trace(&path)
                        .iter()
                        .any(|r| r["event"]["event"] == "primary_rest_proof_scheduled")
                })
                .await;
                go(T + 603100);
                peer.send(data(T + 600000)).await.unwrap();
                accepted(&mut ack, T + 603100).await;
                go(T + 604099);
                release.notify_one();
                until(|| io.entered.load(Ordering::SeqCst)).await;
                // The one command really saw two different clock samples: readiness
                // at 4099, then durable waiting IO at/even after the 4100 deadline.
                assert_eq!(io.before_ms.load(Ordering::SeqCst), T + 604099);
                let after = io.after_ms.load(Ordering::SeqCst);
                assert_eq!(after, T + 604099 + elapsed as i64);
                assert_eq!(
                    trace(&path)
                        .iter()
                        .filter(|r| r["kind"] == "model_step")
                        .count(),
                    0
                );
                go(after); // synchronize only the test caller for subsequent commands
                if mode == "stop" {
                    root.stop();
                }
                io.release.store(true, Ordering::SeqCst);
                // No more DATA and no more HTTP completions: only the retained due
                // can schedule the next canonical evaluation.
                match mode {
                    "match" => {
                        until(|| {
                            trace(&path)
                                .iter()
                                .any(|r| r["event"]["event"] == "primary_proof_became_comparison")
                        })
                        .await
                    }
                    "mismatch" => until(|| task.is_finished()).await,
                    "late" => {
                        until(|| {
                            trace(&path)
                                .iter()
                                .any(|r| r["event"]["event"] == "primary_recovery_episode_started")
                        })
                        .await
                    }
                    "stop" => (),
                    _ => unreachable!(),
                }
                root.stop();
                let report = task.await.unwrap().unwrap();
                assert!(report.all_workers_drained, "{mode}: {report:?}");
                assert_eq!(report.http.proof.scheduled, 1);
                assert_eq!(report.http.proof.completed, 1);
                assert_eq!(calls.load(Ordering::SeqCst), 1);
                let expected = u64::from(mode == "match");
                assert_eq!(report.canonical_commits, expected, "{mode}: {report:?}");
                if mode != "late" {
                    assert!(report.recovery.is_empty(), "{mode}: {report:?}");
                }
                assert!(!report.full_session_qualified);
                let rows = trace(&path);
                assert_eq!(
                    rows.iter().filter(|r| r["kind"] == "model_step").count(),
                    expected as usize
                );
                assert_eq!(
                    rows.iter()
                        .filter(|r| r["kind"] == "canonical_commit")
                        .count(),
                    expected as usize
                );
                assert!(
                    rows.iter().filter(|r| r["kind"] == "virtual_fill").count()
                        <= expected as usize
                );
                if mode == "match" {
                    let receipt = &rows
                        .iter()
                        .find(|r| r["kind"] == "canonical_source")
                        .unwrap()["event"]["receipt"];
                    assert_eq!(receipt["origin"], "WS_LIVE");
                    let committed = rows
                        .iter()
                        .find(|r| r["kind"] == "canonical_commit")
                        .unwrap()["event"]["publication_checked_at_ms"]
                        .as_i64()
                        .unwrap();
                    assert!(committed + 100 <= T + 610000);
                }
                eprintln!("R3_IO_CASE mode={mode} elapsed_ms={elapsed} evaluation_at={} update_after={} commits={expected}", T+604099, after);
            })
        });
    }
}

#[test]
fn r2_r3_early_proof_and_boundary_timer_do_not_need_another_data_frame() {
    for mode in [
        "match",
        "mismatch",
        "stop",
        "unavailable",
        "late-unavailable",
    ] {
        with_fixture_clock(T - 100, || {
            runtime(async {
                let (owner, _, root, path) = context(false);
                let (factory, mut peers, mut progress, _) = setup().await;
                let release = Arc::new(tokio::sync::Notify::new());
                let fetch_release = release.clone();
                let fetch = move |job: Job, _stop: StopHandle| {
                    let release = fetch_release.clone();
                    async move {
                        release.notified().await;
                        let plan = job.plan().unwrap();
                        let mut rows: Vec<_> = (plan.start().timestamp_millis()
                            ..plan.end().timestamp_millis())
                            .step_by(60000)
                            .map(raw_row)
                            .collect();
                        if mode == "mismatch" {
                            rows[0]["volume"] = json!({"value":"3"});
                        }
                        let mut out = source(&job, rows);
                        if mode.ends_with("unavailable") {
                            out.result = Err(RestFailure::Unavailable);
                        }
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
                for i in 0..10 {
                    go(T + i * 60000 + 100);
                    peer.send(data(T + i * 60000)).await.unwrap();
                    accepted(&mut ack, T + i * 60000 + 100).await;
                }
                go(T + 603000);
                peer.send(data(T + 540000)).await.unwrap();
                until(|| {
                    trace(&path)
                        .iter()
                        .any(|r| r["event"]["event"] == "primary_rest_proof_scheduled")
                })
                .await;
                if mode == "unavailable" {
                    go(T + 603050);
                    release.notify_one();
                    until(|| {
                        trace(&path)
                            .iter()
                            .any(|r| r["event"]["event"] == "primary_rest_proof_failed")
                    })
                    .await;
                }
                go(T + 603100);
                peer.send(data(T + 600000)).await.unwrap();
                accepted(&mut ack, T + 603100).await;
                if !mode.ends_with("unavailable") {
                    go(T + 603200);
                    release.notify_one();
                    until(|| {
                        trace(&path)
                            .iter()
                            .any(|r| r["event"]["event"] == "primary_proof_waiting_ws_grace")
                    })
                    .await;
                }
                assert_eq!(
                    trace(&path)
                        .iter()
                        .filter(|r| r["kind"] == "model_step")
                        .count(),
                    0
                );
                if !mode.ends_with("unavailable") {
                    ack.changed().await.unwrap();
                    ack.borrow_and_update();
                }
                if mode == "stop" {
                    root.stop();
                } else {
                    // Only the writer timer can seal: there are NO new DATA/job
                    // completions after grace. Clock uncertainty stays 100 ms.
                    go(T + 604100);
                    if mode == "late-unavailable" {
                        release.notify_one();
                    }
                    until(|| {
                        if mode == "mismatch" {
                            task.is_finished()
                        } else {
                            trace(&path).iter().any(|r| r["kind"] == "canonical_commit")
                        }
                    })
                    .await;
                    if mode != "mismatch" {
                        ack.changed().await.unwrap();
                        ack.borrow_and_update();
                    }
                    root.stop();
                }
                let report = task.await.unwrap().unwrap();
                assert!(report.all_workers_drained, "{report:?}");
                assert_eq!(report.http.proof.scheduled, 1);
                assert_eq!(report.http.proof.completed, 1);
                let expected = u64::from(mode == "match" || mode.ends_with("unavailable"));
                assert_eq!(report.canonical_commits, expected, "{report:?}");
                assert!(report.recovery.is_empty(), "{report:?}");
                assert!(!report.full_session_qualified);
                let rows = trace(&path);
                assert_eq!(
                    rows.iter().filter(|r| r["kind"] == "model_step").count(),
                    expected as usize
                );
                if expected == 1 {
                    assert_eq!(
                        rows.iter()
                            .find(|r| r["kind"] == "canonical_source")
                            .unwrap()["event"]["receipt"]["origin"],
                        "WS_LIVE"
                    );
                    assert!(matches!(report.cause, Cause::RequestedStop));
                }
            })
        });
    }
}

#[test]
fn pending_critical_proof_allows_ws_commit_once_or_known_stop_without_callback() {
    for stop_during_proof in [false, true] {
        with_fixture_clock(T - 100, || {
            runtime(async {
                let (owner, _, root, path) = context(false);
                let (factory, mut peers, mut progress, _) = setup().await;
                let (scheduled, mut observed) = tokio::sync::mpsc::channel(1);
                let release = Arc::new(tokio::sync::Notify::new());
                let fetch_release = release.clone();
                let fetch = move |job: Job, stop: StopHandle| {
                    let scheduled = scheduled.clone();
                    let release = fetch_release.clone();
                    async move {
                        assert!(matches!(job, Job::Proof(_)));
                        let started = crate::timing::now_ms();
                        scheduled.send(()).await.unwrap();
                        tokio::select! {biased;
                            _=stop.cancelled()=>HttpOutcome{result:Err(RestFailure::Cancelled),receipt:HttpReceipt{started_at_ms:started,finished_at_ms:crate::timing::now_ms(),deadline_ms:job.deadline_ms(),get_attempts:1,auth_generation:Some(1),body:None,executor_completed:true}},
                            _=release.notified()=>{
                                let plan=job.plan().unwrap();source(&job,(plan.start().timestamp_millis()..plan.end().timestamp_millis()).step_by(60000).map(raw_row).collect())
                            },
                        }
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
                for i in 0..10 {
                    go(T + i * 60000 + 100);
                    peer.send(data(T + i * 60000)).await.unwrap();
                    accepted(&mut ack, T + i * 60000 + 100).await;
                }
                // An exact repeat wakes the writer after C+3000, but is NOT boundary
                // DATA and does not refresh its admitted DATA clock.
                go(T + 603100);
                peer.send(data(T + 9 * 60000)).await.unwrap();
                tokio::time::timeout(Duration::from_secs(2), observed.recv())
                    .await
                    .unwrap()
                    .unwrap();
                assert_eq!(
                    trace(&path)
                        .iter()
                        .filter(|r| r["kind"] == "model_step")
                        .count(),
                    0
                );
                if !stop_during_proof {
                    go(T + 603200);
                    peer.send(data(T + 600000)).await.unwrap();
                    accepted(&mut ack, T + 603200).await;
                    go(T + 604300);
                    peer.send(data(T + 600000)).await.unwrap();
                    until(|| {
                        trace(&path)
                            .iter()
                            .filter(|r| r["kind"] == "canonical_commit")
                            .count()
                            == 1
                    })
                    .await;
                    ack.changed().await.unwrap();
                    ack.borrow_and_update();
                    release.notify_one();
                    until(|| {
                        trace(&path)
                            .iter()
                            .any(|r| r["event"]["event"] == "primary_proof_became_comparison")
                    })
                    .await;
                    ack.changed().await.unwrap();
                    ack.borrow_and_update();
                }
                root.stop();
                let report = task.await.unwrap().unwrap();
                assert!(report.all_workers_drained, "{report:?}");
                assert!(matches!(report.cause, Cause::RequestedStop), "{report:?}");
                assert_eq!(report.http.proof.scheduled, 1);
                assert_eq!(report.http.proof.completed, 1);
                assert_eq!(report.http.proof.cancelled_or_unaccounted, 0);
                assert_eq!(
                    report.canonical_commits,
                    if stop_during_proof { 0 } else { 1 }
                );
                assert!(!report.full_session_qualified);
                let rows = trace(&path);
                assert_eq!(
                    rows.iter().filter(|r| r["kind"] == "model_step").count(),
                    if stop_during_proof { 0 } else { 1 }
                );
                assert_eq!(
                    rows.iter().filter(|r| r["kind"] == "virtual_fill").count(),
                    0
                );
            })
        });
    }
}
