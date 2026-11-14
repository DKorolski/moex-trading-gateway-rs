use super::super::tests::{accepted, context, go, source, trace, until, T};
use super::*;
use crate::{session::primary_tests::raw_row, timing::with_fixture_clock};
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::{
    collections::VecDeque,
    sync::atomic::{AtomicUsize, Ordering},
};
use tokio_tungstenite::{
    tungstenite::{protocol::Role, Message},
    WebSocketStream,
};
type Peer = WebSocketStream<tokio::io::DuplexStream>;
mod cleanup;
mod cohort;
mod consistent_sources;
mod data_wait;
mod eod;
mod nonblocking_rest;
mod proof_race;
mod tail;

#[test]
fn consistent_v5_dequeued_stale_fault_closes_gate_synchronously_clock_does_not_retry() {
    let policy = PrimaryPolicy {
        delivery: crate::primary::DeliveryPolicy::ObservedBoConsistentSourcesV5,
        ..PrimaryPolicy::default()
    };
    let gate = ContinuityGate::new(1);
    let slot = FailureSlot::with_gate(gate.clone(), StopHandle::default());
    let cause = ingress_failure(
        &policy,
        &gate,
        &slot,
        crate::primary::PrimaryError::Continuity,
    );
    assert!(
        matches!(cause,Cause::Socket(ref f) if f.kind==crate::socket::primary::Kind::Source(crate::primary::PrimaryError::Continuity))
    );
    assert!(!gate.healthy());
    assert!(gate.snapshot().failure.is_some());
    assert!(matches!(
        ingress_failure(&policy, &gate, &slot, crate::primary::PrimaryError::Clock),
        Cause::Coordinator(CoordinatorError::Source(
            crate::primary::PrimaryError::Clock
        ))
    ));
}
fn runtime(work: impl Future<Output = ()>) {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(work);
}
fn data(label: i64) -> Message {
    Message::Text(
        json!({"type":"DATA","subscriptionType":"BARS","subscriptionKey":"fixture",
    "payload":{"symbol":"IMOEXF@RTSX","timeframe":"TIME_FRAME_M1","bars":[raw_row(label)]}})
        .to_string(),
    )
}
async fn handshake(peer: &mut Peer) {
    peer.send(Message::Text(
        r#"{"type":"EVENT","eventInfo":{"event":"HANDSHAKE_SUCCESS"}}"#.into(),
    ))
    .await
    .unwrap();
    let Some(Ok(Message::Text(text))) = peer.next().await else {
        panic!("SUBSCRIBE required")
    };
    let value: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(value["action"], "SUBSCRIBE");
}
async fn setup() -> (
    impl Fn(Open) -> std::pin::Pin<Box<dyn Future<Output = Result<SocketReport, Failure>> + Send>>
        + Clone
        + Send
        + 'static,
    Vec<Peer>,
    tokio::sync::mpsc::Receiver<watch::Receiver<Option<LiveDataStamp>>>,
    Arc<AtomicUsize>,
) {
    let clients = Arc::new(Mutex::new(VecDeque::new()));
    let mut peers = Vec::new();
    for _ in 0..4 {
        let (a, b) = tokio::io::duplex(512 * 1024);
        let client = WebSocketStream::from_raw_socket(a, Role::Client, None).await;
        clients.lock().unwrap().push_back(client);
        peers.push(WebSocketStream::from_raw_socket(b, Role::Server, None).await);
    }
    let (progress, receiver) = tokio::sync::mpsc::channel(16);
    let calls = Arc::new(AtomicUsize::new(0));
    let count = calls.clone();
    let factory = move |request: Open| -> std::pin::Pin<
        Box<dyn Future<Output = Result<SocketReport, Failure>> + Send>,
    > {
        count.fetch_add(1, Ordering::SeqCst);
        let socket = clients.lock().unwrap().pop_front().unwrap();
        progress.try_send(request.feedback.clone()).unwrap();
        assert!(request.deadline_ms <= request.run_deadline_ms);
        Box::pin(crate::socket::primary::fixture_pump_until(
            socket,
            request.generation,
            request.stop,
            request.slot,
            request.tx,
            request.feedback,
            request.started,
            DateTime::from_timestamp_millis(request.run_deadline_ms).unwrap(),
        ))
    };
    (factory, peers, receiver, calls)
}
async fn feed(
    peer: &mut Peer,
    progress: &mut watch::Receiver<Option<LiveDataStamp>>,
    path: &std::path::Path,
    start: i64,
    end: i64,
    expected_first: usize,
) {
    for i in start..=end {
        let stamp = T + i * 60000 + 100;
        go(stamp);
        peer.send(data(stamp - 100)).await.unwrap();
        accepted(progress, stamp).await;
        if i > 0 && i % 10 == 0 {
            let expected = expected_first + ((i - start) / 10) as usize;
            go(stamp + 1000);
            peer.send(data(stamp - 100)).await.unwrap();
            until(|| {
                trace(path)
                    .iter()
                    .filter(|r| r["kind"] == "canonical_commit")
                    .count()
                    >= expected
            })
            .await;
            progress.changed().await.unwrap();
            progress.borrow_and_update();
        }
    }
}
async fn fetch(job: Job, _stop: StopHandle) -> HttpOutcome {
    let plan = job.plan().unwrap();
    let rows = (plan.start().timestamp_millis()..plan.end().timestamp_millis())
        .step_by(60000)
        .map(raw_row)
        .collect();
    source(&job, rows)
}

#[test]
fn invalid_calendar_reference_is_rejected_before_any_connection_task_is_started() {
    let f = crate::primary::calendar::tests::Fixture::new();
    let d = crate::primary::calendar::tests::Dir::new();
    with_fixture_clock(f.open(), || {
        runtime(async {
            let mut owner = f.owner(&d.0.join("journal"), false);
            let calendar = f.cap(crate::timing::now_ms()).unwrap();
            let wrong = calendar.reference_start_ms() + 60000;
            owner.bind_calendar(calendar).unwrap();
            let calls = Arc::new(AtomicUsize::new(0));
            let count = calls.clone();
            let factory = move |request: Open| {
                count.fetch_add(1, Ordering::SeqCst);
                async move {
                    Err(Failure {
                        generation: request.generation,
                        kind: crate::socket::primary::Kind::Connect,
                        liveness: None,
                    })
                }
            };
            assert!(matches!(
                drive(
                    owner,
                    DateTime::from_timestamp_millis(wrong).unwrap(),
                    factory,
                    fetch
                )
                .await,
                Err(Cause::Binding)
            ));
            assert_eq!(calls.load(Ordering::SeqCst), 0);
        })
    });
}

#[test]
fn actual_peer_close_or_eof_repairs_gap_and_continues_the_same_writer() {
    for graceful in [true, false] {
        with_fixture_clock(T - 100, || {
            runtime(async {
                let (owner, _, root, path) = context(false);
                let (factory, mut peers, mut progress, calls) = setup().await;
                let task = tokio::spawn(drive(
                    owner,
                    DateTime::from_timestamp_millis(T - 3600000).unwrap(),
                    factory,
                    fetch,
                ));
                let mut p1 = progress.recv().await.unwrap();
                handshake(&mut peers[0]).await;
                until(|| {
                    trace(&path)
                        .iter()
                        .any(|r| r["event"]["event"] == "primary_subscription_sent")
                })
                .await;
                feed(&mut peers[0], &mut p1, &path, 0, 20, 0).await;
                go(T + 3 * 600000 + 100);
                if graceful {
                    peers[0].close(None).await.unwrap();
                } else {
                    drop(peers.swap_remove(0));
                }
                let mut p2 = tokio::time::timeout(Duration::from_secs(5), progress.recv())
                    .await
                    .unwrap()
                    .unwrap();
                handshake(&mut peers[1]).await;
                until(|| {
                    trace(&path).iter().any(|r| {
                        r["event"]["event"] == "primary_subscription_sent"
                            && r["event"]["generation"] == 2
                    })
                })
                .await;
                go(T + 3 * 600000 + 200);
                peers[1].send(data(T + 3 * 600000)).await.unwrap();
                accepted(&mut p2, T + 3 * 600000 + 200).await;
                until(|| {
                    trace(&path)
                        .iter()
                        .any(|r| r["event"]["event"] == "primary_recovery_episode_ready")
                })
                .await;
                p2.changed().await.unwrap();
                p2.borrow_and_update();
                assert!(!root.is_stopped());
                let ready = trace(&path)
                    .into_iter()
                    .find(|r| r["event"]["event"] == "primary_recovery_episode_ready")
                    .unwrap();
                let episode_deadline = ready["event"]["audit"]["deadline_ms"].as_i64().unwrap();
                // The bucket entered after SUBSCRIBE is still STARTUP_PARTIAL; do not
                // pretend full WS coverage just because historical repair succeeded.
                for i in 31..=40 {
                    let stamp = T + i * 60000 + 100;
                    go(stamp);
                    peers[1].send(data(stamp - 100)).await.unwrap();
                    accepted(&mut p2, stamp).await;
                }
                go(T + 4 * 600000 + 3100);
                peers[1].send(data(T + 4 * 600000)).await.unwrap();
                until(|| {
                    trace(&path)
                        .iter()
                        .filter(|r| r["kind"] == "canonical_commit")
                        .count()
                        == 4
                })
                .await;
                p2.changed().await.unwrap();
                p2.borrow_and_update();
                feed(&mut peers[1], &mut p2, &path, 41, 50, 5).await;
                assert!(crate::timing::now_ms() > episode_deadline);
                assert!(
                    !task.is_finished(),
                    "restored Driver must outlive the recovery deadline"
                );
                root.stop();
                let report = task.await.unwrap().unwrap();
                assert!(report.all_workers_drained, "{report:?}");
                assert!(matches!(report.cause, Cause::RequestedStop), "{report:?}");
                assert_eq!(report.canonical_commits, 5);
                assert_eq!(calls.load(Ordering::SeqCst), 2);
                assert_eq!(report.http.recovery.scheduled, 1);
                assert_eq!(report.http.proof.scheduled, 1);
                assert_eq!(report.recovery.len(), 1);
                assert!(
                    !report.recovery[0].transport_unresolved
                        && !report.recovery[0].canonical_unresolved
                );
                assert!(report.unresolved_transport_fault.is_none());
                assert!(!report.full_session_qualified);
                let events = trace(&path);
                assert_eq!(events.iter().filter(|r| r["kind"] == "seed").count(), 1);
                let origins: Vec<_> = events
                    .iter()
                    .filter(|r| r["kind"] == "canonical_source")
                    .map(|r| r["event"]["receipt"]["origin"].as_str().unwrap())
                    .collect();
                assert_eq!(
                    origins,
                    vec![
                        "WS_LIVE",
                        "WS_LIVE",
                        "RECOVERY_BACKFILL",
                        "REST_EXCEPTION",
                        "WS_LIVE"
                    ]
                );
            })
        });
    }
}

#[test]
fn operator_stop_in_backoff_preserves_original_close_and_does_not_reconnect() {
    with_fixture_clock(T - 100, || {
        runtime(async {
            let (owner, _, root, path) = context(false);
            let (factory, mut peers, mut progress, calls) = setup().await;
            let task = tokio::spawn(drive(
                owner,
                DateTime::from_timestamp_millis(T - 3600000).unwrap(),
                factory,
                fetch,
            ));
            let _p1 = progress.recv().await.unwrap();
            handshake(&mut peers[0]).await;
            until(|| {
                trace(&path)
                    .iter()
                    .any(|r| r["event"]["event"] == "primary_subscription_sent")
            })
            .await;
            peers[0].close(None).await.unwrap();
            until(|| {
                trace(&path)
                    .iter()
                    .any(|r| r["event"]["event"] == "primary_recovery_episode_started")
            })
            .await;
            root.stop();
            let report = task.await.unwrap().unwrap();
            assert!(report.all_workers_drained, "{report:?}");
            assert_eq!(calls.load(Ordering::SeqCst), 1);
            assert!(
                report.recovery[0].transport_unresolved && report.recovery[0].canonical_unresolved
            );
            assert_eq!(
                report.recovery[0].original.kind,
                crate::socket::primary::Kind::Close
            );
            assert!(report.unresolved_transport_fault.is_some());
            assert_eq!(report.canonical_commits, 0);
            assert!(!report.full_session_qualified);
        })
    });
}

#[test]
fn malformed_new_generation_data_is_not_retried_as_a_network_fault() {
    with_fixture_clock(T - 100, || {
        runtime(async {
            let (owner, _, root, path) = context(false);
            let (factory, mut peers, mut progress, calls) = setup().await;
            let task = tokio::spawn(drive(
                owner,
                DateTime::from_timestamp_millis(T - 3600000).unwrap(),
                factory,
                fetch,
            ));
            let _p1 = progress.recv().await.unwrap();
            handshake(&mut peers[0]).await;
            until(|| {
                trace(&path)
                    .iter()
                    .any(|r| r["event"]["event"] == "primary_subscription_sent")
            })
            .await;
            peers[0].send(Message::Text(r#"{"type":"DATA","subscriptionType":"BARS","subscriptionKey":"fixture","payload":{"symbol":"WRONG","bars":[]}}"#.into())).await.unwrap();
            let report = task.await.unwrap().unwrap();
            assert!(root.is_stopped());
            assert!(report.all_workers_drained);
            assert_eq!(calls.load(Ordering::SeqCst), 1);
            assert!(report.unresolved_transport_fault.is_some());
            assert!(!report.full_session_qualified);
        })
    });
}

#[test]
fn completed_auth_unavailability_retries_but_never_reuses_the_old_socket() {
    with_fixture_clock(T - 100, || {
        runtime(async {
            let (owner, _, root, path) = context(false);
            let (base, mut peers, mut progress, calls) = setup().await;
            let commands = Arc::new(AtomicUsize::new(0));
            let auth_calls = commands.clone();
            let factory = move |request: Open| -> std::pin::Pin<
                Box<dyn Future<Output = Result<SocketReport, Failure>> + Send>,
            > {
                auth_calls.fetch_add(1, Ordering::SeqCst);
                if request.generation == 2 {
                    Box::pin(async move {
                        Err(Failure {
                            generation: 2,
                            kind: crate::socket::primary::Kind::Auth(
                                crate::auth::PrimaryAuthClass::Unavailable,
                            ),
                            liveness: None,
                        })
                    })
                } else {
                    base(request)
                }
            };
            let task = tokio::spawn(drive(
                owner,
                DateTime::from_timestamp_millis(T - 3600000).unwrap(),
                factory,
                fetch,
            ));
            let mut p1 = progress.recv().await.unwrap();
            handshake(&mut peers[0]).await;
            until(|| {
                trace(&path)
                    .iter()
                    .any(|r| r["event"]["event"] == "primary_subscription_sent")
            })
            .await;
            feed(&mut peers[0], &mut p1, &path, 0, 20, 0).await;
            go(T + 3 * 600000 + 100);
            peers[0].close(None).await.unwrap();
            let mut p3 = tokio::time::timeout(Duration::from_secs(6), progress.recv())
                .await
                .unwrap()
                .unwrap();
            handshake(&mut peers[1]).await;
            until(|| {
                trace(&path).iter().any(|r| {
                    r["event"]["event"] == "primary_subscription_sent"
                        && r["event"]["generation"] == 3
                })
            })
            .await;
            go(T + 3 * 600000 + 200);
            peers[1].send(data(T + 3 * 600000)).await.unwrap();
            accepted(&mut p3, T + 3 * 600000 + 200).await;
            until(|| {
                trace(&path)
                    .iter()
                    .any(|r| r["event"]["event"] == "primary_recovery_episode_ready")
            })
            .await;
            p3.changed().await.unwrap();
            p3.borrow_and_update();
            root.stop();
            let report = task.await.unwrap().unwrap();
            assert!(report.all_workers_drained, "{report:?}");
            assert!(matches!(report.cause, Cause::RequestedStop), "{report:?}");
            assert_eq!(commands.load(Ordering::SeqCst), 3);
            assert_eq!(calls.load(Ordering::SeqCst), 2);
            assert_eq!(report.recovery[0].attempt, 2);
            assert_eq!(report.recovery[0].generation, 3);
            assert_eq!(report.canonical_commits, 3);
            assert!(report.unresolved_transport_fault.is_none());
        })
    });
}

#[test]
fn operator_stop_during_auth_drains_candidate_and_retains_unresolved_close() {
    with_fixture_clock(T - 100, || {
        runtime(async {
            let (owner, _, root, path) = context(false);
            let (base, mut peers, mut progress, _) = setup().await;
            let entering = Arc::new(AtomicUsize::new(0));
            let marker = entering.clone();
            let factory = move |request: Open| -> std::pin::Pin<
                Box<dyn Future<Output = Result<SocketReport, Failure>> + Send>,
            > {
                if request.generation == 2 {
                    let marker = marker.clone();
                    Box::pin(async move {
                        marker.store(1, Ordering::SeqCst);
                        request.stop.cancelled().await;
                        Err(Failure {
                            generation: 2,
                            kind: crate::socket::primary::Kind::Cancelled,
                            liveness: None,
                        })
                    })
                } else {
                    base(request)
                }
            };
            let task = tokio::spawn(drive(
                owner,
                DateTime::from_timestamp_millis(T - 3600000).unwrap(),
                factory,
                fetch,
            ));
            let _p1 = progress.recv().await.unwrap();
            handshake(&mut peers[0]).await;
            until(|| {
                trace(&path)
                    .iter()
                    .any(|r| r["event"]["event"] == "primary_subscription_sent")
            })
            .await;
            peers[0].close(None).await.unwrap();
            until(|| entering.load(Ordering::SeqCst) == 1).await;
            root.stop();
            let report = task.await.unwrap().unwrap();
            assert!(report.all_workers_drained, "{report:?}");
            assert!(
                report.recovery[0].transport_unresolved && report.recovery[0].canonical_unresolved
            );
            assert_eq!(
                report.recovery[0].original.kind,
                crate::socket::primary::Kind::Close
            );
            assert!(report.unresolved_transport_fault.is_some());
            assert_eq!(report.canonical_commits, 0);
            assert!(!report.full_session_qualified);
        })
    });
}

#[test]
fn another_close_during_resync_reopens_transport_readiness_before_stop() {
    with_fixture_clock(T - 100, || {
        runtime(async {
            let (owner, _, root, path) = context(false);
            let (factory, mut peers, mut progress, calls) = setup().await;
            let entered = Arc::new(AtomicUsize::new(0));
            let marker = entered.clone();
            let operator_stop = root.clone();
            let stalled_fetch = move |job: Job, stop: StopHandle| {
                let marker = marker.clone();
                let operator_stop = operator_stop.clone();
                async move {
                    if matches!(job, Job::Recovery(_)) {
                        marker.store(1, Ordering::SeqCst);
                        stop.cancelled().await;
                        // Operator STOP races with retirement of the broken
                        // candidate: no next attempt may clear the new fault.
                        operator_stop.stop();
                        HttpOutcome {
                            result: Err(RestFailure::Cancelled),
                            receipt: HttpReceipt {
                                started_at_ms: crate::timing::now_ms(),
                                finished_at_ms: crate::timing::now_ms(),
                                deadline_ms: job.deadline_ms(),
                                get_attempts: 1,
                                auth_generation: Some(2),
                                body: None,
                                executor_completed: true,
                            },
                        }
                    } else {
                        fetch(job, stop).await
                    }
                }
            };
            let task = tokio::spawn(drive(
                owner,
                DateTime::from_timestamp_millis(T - 3600000).unwrap(),
                factory,
                stalled_fetch,
            ));
            let mut p1 = progress.recv().await.unwrap();
            handshake(&mut peers[0]).await;
            until(|| {
                trace(&path)
                    .iter()
                    .any(|r| r["event"]["event"] == "primary_subscription_sent")
            })
            .await;
            feed(&mut peers[0], &mut p1, &path, 0, 20, 0).await;
            go(T + 3 * 600000 + 100);
            peers[0].close(None).await.unwrap();
            let mut p2 = progress.recv().await.unwrap();
            handshake(&mut peers[1]).await;
            until(|| {
                trace(&path).iter().any(|r| {
                    r["event"]["event"] == "primary_subscription_sent"
                        && r["event"]["generation"] == 2
                })
            })
            .await;
            go(T + 3 * 600000 + 200);
            peers[1].send(data(T + 3 * 600000)).await.unwrap();
            accepted(&mut p2, T + 3 * 600000 + 200).await;
            until(|| entered.load(Ordering::SeqCst) == 1).await;
            peers[1].close(None).await.unwrap();
            let report = task.await.unwrap().unwrap();
            assert!(report.all_workers_drained, "{report:?}");
            assert_eq!(calls.load(Ordering::SeqCst), 2);
            assert_eq!(report.canonical_commits, 2);
            assert_eq!(report.recovery[0].original.generation, 1);
            assert_eq!(
                report.recovery[0].original.kind,
                crate::socket::primary::Kind::Close
            );
            assert!(
                report.recovery[0].transport_unresolved && report.recovery[0].canonical_unresolved,
                "{report:?}"
            );
            assert_eq!(
                report
                    .unresolved_transport_fault
                    .as_ref()
                    .unwrap()
                    .generation,
                2
            );
            assert!(!report.full_session_qualified);
        })
    });
}

#[test]
fn conflicting_rest_overlap_closes_admission_before_any_historical_callback() {
    with_fixture_clock(T - 100, || {
        runtime(async {
            let (owner, _, root, path) = context(false);
            let (factory, mut peers, mut progress, calls) = setup().await;
            let bad_fetch = |job: Job, _stop: StopHandle| async move {
                let plan = job.plan().unwrap();
                let mut rows: Vec<_> = (plan.start().timestamp_millis()
                    ..plan.end().timestamp_millis())
                    .step_by(60000)
                    .map(raw_row)
                    .collect();
                rows[0]["volume"] = json!({"value":"3"});
                source(&job, rows)
            };
            let task = tokio::spawn(drive(
                owner,
                DateTime::from_timestamp_millis(T - 3600000).unwrap(),
                factory,
                bad_fetch,
            ));
            let mut p1 = progress.recv().await.unwrap();
            handshake(&mut peers[0]).await;
            until(|| {
                trace(&path)
                    .iter()
                    .any(|r| r["event"]["event"] == "primary_subscription_sent")
            })
            .await;
            feed(&mut peers[0], &mut p1, &path, 0, 20, 0).await;
            go(T + 3 * 600000 + 100);
            peers[0].close(None).await.unwrap();
            let mut p2 = progress.recv().await.unwrap();
            handshake(&mut peers[1]).await;
            until(|| {
                trace(&path).iter().any(|r| {
                    r["event"]["event"] == "primary_subscription_sent"
                        && r["event"]["generation"] == 2
                })
            })
            .await;
            go(T + 3 * 600000 + 200);
            peers[1].send(data(T + 3 * 600000)).await.unwrap();
            accepted(&mut p2, T + 3 * 600000 + 200).await;
            let report = task.await.unwrap().unwrap();
            assert!(root.is_stopped());
            assert!(report.all_workers_drained, "{report:?}");
            assert_eq!(calls.load(Ordering::SeqCst), 2);
            assert!(
                matches!(
                    report.cause,
                    Cause::Coordinator(CoordinatorError::Source(PrimaryError::CanonicalConflict))
                ),
                "{report:?}"
            );
            assert!(report.recovery[0].canonical_unresolved);
            assert_eq!(report.canonical_commits, 2);
            assert_eq!(
                trace(&path)
                    .iter()
                    .filter(|r| r["kind"] == "model_step")
                    .count(),
                2
            );
        })
    });
}

#[test]
fn history_and_server_ping_do_not_complete_first_data_or_canonical_recovery() {
    with_fixture_clock(T - 100, || {
        runtime(async {
            let (owner, _, root, path) = context(false);
            let (factory, mut peers, mut progress, _) = setup().await;
            let task = tokio::spawn(drive(
                owner,
                DateTime::from_timestamp_millis(T - 3600000).unwrap(),
                factory,
                fetch,
            ));
            let mut p1 = progress.recv().await.unwrap();
            handshake(&mut peers[0]).await;
            until(|| {
                trace(&path)
                    .iter()
                    .any(|r| r["event"]["event"] == "primary_subscription_sent")
            })
            .await;
            feed(&mut peers[0], &mut p1, &path, 0, 20, 0).await;
            go(T + 3 * 600000 + 100);
            peers[0].close(None).await.unwrap();
            let p2 = progress.recv().await.unwrap();
            handshake(&mut peers[1]).await;
            until(|| {
                trace(&path).iter().any(|r| {
                    r["event"]["event"] == "primary_subscription_sent"
                        && r["event"]["generation"] == 2
                })
            })
            .await;
            let mut history = raw_row(T);
            history["isDataSnapshot"] = json!(true);
            peers[1].send(Message::Text(json!({"type":"DATA","subscriptionType":"BARS","subscriptionKey":"fixture","payload":{"symbol":"IMOEXF@RTSX","bars":[history]}}).to_string())).await.unwrap();
            peers[1].send(Message::Ping(vec![1, 2, 3])).await.unwrap();
            assert!(matches!(
                peers[1].next().await.unwrap().unwrap(),
                Message::Pong(_)
            ));
            assert!(p2.borrow().is_none());
            root.stop();
            let report = task.await.unwrap().unwrap();
            assert!(report.all_workers_drained, "{report:?}");
            assert!(
                report.recovery[0].transport_unresolved && report.recovery[0].canonical_unresolved
            );
            assert_eq!(report.http.recovery.scheduled, 0);
            assert_eq!(report.canonical_commits, 2);
        })
    });
}
