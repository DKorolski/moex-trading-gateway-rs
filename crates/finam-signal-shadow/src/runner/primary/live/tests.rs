use super::*;

#[test]
fn final_proof_has_a_distinct_market_http_role() {
    let ticket = RestTicket {
        owner: Arc::new(()),
        label_ms: T,
        reason: Some(ExceptionReason::SessionEnd),
        issued_at_ms: T + 603000,
        deadline_ms: T + 608000,
    };
    assert!(matches!(Job::Proof(ticket).role(), Role::SessionEnd));
}
use crate::{
    primary::PrimaryProcessor,
    session::primary_tests::raw_row,
    timing::{advance_fixture_clock, now_ms, with_fixture_clock},
};
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio_tungstenite::{
    tungstenite::{protocol::Role as WsRole, Message},
    WebSocketStream,
};

pub(super) const T: i64 = 1793678400000;
fn at(t: i64) -> DateTime<Utc> {
    DateTime::from_timestamp_millis(t).unwrap()
}
pub(super) fn go(t: i64) {
    assert!(t >= now_ms());
    advance_fixture_clock((t - now_ms()) as u64);
}
pub(super) fn context(audit: bool) -> (Coordinator, PrimaryPolicy, StopHandle, std::path::PathBuf) {
    context_delivery(audit, crate::primary::DeliveryPolicy::StrictBoundaryV1)
}
pub(super) fn context_delivery(
    audit: bool,
    delivery: crate::primary::DeliveryPolicy,
) -> (Coordinator, PrimaryPolicy, StopHandle, std::path::PathBuf) {
    let (session, c, _, path) = crate::session::tests::primary_fixture_delivery(T, audit, delivery);
    let stop = session.primary_runtime().unwrap().1;
    let instrument = broker_core::InstrumentId {
        symbol: "IMOEXF".into(),
        venue_symbol: Some("IMOEXF@RTSX".into()),
        exchange: broker_core::Exchange::Moex,
        market: broker_core::Market::Futures,
    };
    let processor = PrimaryProcessor::from_config(
        &c,
        instrument,
        (0..12).map(|i| T + i * 600000).collect(),
        1,
        T - 100,
    )
    .unwrap();
    (
        Coordinator::new(processor, session).unwrap(),
        c.primary,
        stop,
        path,
    )
}
fn runtime(work: impl Future<Output = ()>) {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(work);
}
fn data(rows: Vec<Value>) -> Message {
    Message::Text(
        json!({"type":"DATA","subscriptionType":"BARS","subscriptionKey":"fixture",
    "payload":{"symbol":"IMOEXF@RTSX","timeframe":"TIME_FRAME_M1","bars":rows}})
        .to_string(),
    )
}
pub(super) fn trace(path: &std::path::Path) -> Vec<Value> {
    std::fs::read_to_string(path)
        .unwrap()
        .split_inclusive('\n')
        .filter(|line| line.ends_with('\n'))
        .map(|s| serde_json::from_str(s).unwrap())
        .collect()
}
pub(super) async fn until(mut check: impl FnMut() -> bool) {
    tokio::time::timeout(Duration::from_secs(5), async {
        while !check() {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await
    .unwrap();
}
pub(super) async fn accepted(rx: &mut watch::Receiver<Option<LiveDataStamp>>, stamp: i64) {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if rx
                .borrow_and_update()
                .as_ref()
                .is_some_and(|v| v.received_at_ms == stamp)
            {
                return;
            }
            rx.changed().await.unwrap();
        }
    })
    .await
    .unwrap();
}
async fn never(_job: Job, _stop: StopHandle) -> HttpOutcome {
    panic!("dense WS must not dispatch HTTP")
}
fn failure(class: RestFailure, job: &Job) -> HttpOutcome {
    HttpOutcome {
        result: Err(class),
        receipt: HttpReceipt {
            started_at_ms: now_ms(),
            finished_at_ms: now_ms(),
            deadline_ms: job.deadline_ms(),
            get_attempts: 0,
            auth_generation: None,
            body: None,
            executor_completed: true,
        },
    }
}
pub(super) fn source(job: &Job, rows: Vec<Value>) -> HttpOutcome {
    use broker_finam::sparse_m10::{
        AdmittedClosedM1Snapshot, ClosedM1RestPartV1, ClosedM1SnapshotEvidenceV1,
        OBSERVED_M1_POLICY_V1,
    };
    let plan = job.plan().unwrap();
    let now = at(now_ms());
    let start = plan.start();
    let end = plan.end();
    let body = json!({"symbol":"IMOEXF@RTSX","bars":rows}).to_string();
    let result = AdmittedClosedM1Snapshot::admit(
        ClosedM1SnapshotEvidenceV1 {
            policy: OBSERVED_M1_POLICY_V1.into(),
            start,
            end,
            parts: vec![ClosedM1RestPartV1 {
                method: "GET".into(),
                endpoint: "https://api.finam.ru/v1/instruments/IMOEXF@RTSX/bars".into(),
                symbol: "IMOEXF@RTSX".into(),
                timeframe: "TIME_FRAME_M1".into(),
                start,
                end,
                requested_at: now,
                received_at: now,
                status: 200,
                transport_complete: true,
                declared_body_bytes: Some(body.len() as u64),
                response_sha256: crate::seed::digest(body.as_bytes()),
                raw_body: body,
            }],
        },
        &plan,
        now,
    )
    .unwrap();
    HttpOutcome {
        result: Ok(result),
        receipt: HttpReceipt {
            started_at_ms: now_ms(),
            finished_at_ms: now_ms(),
            deadline_ms: job.deadline_ms(),
            get_attempts: 1,
            auth_generation: Some(1),
            body: None,
            executor_completed: true,
        },
    }
}
type Peer = WebSocketStream<tokio::io::DuplexStream>;
type Progress = watch::Receiver<Option<LiveDataStamp>>;
async fn start<F, Fut>(
    audit: bool,
    fetch: F,
) -> (
    tokio::task::JoinHandle<Result<RunReport, Cause>>,
    Peer,
    Progress,
    StopHandle,
    Faults,
    std::path::PathBuf,
)
where
    F: Fn(Job, StopHandle) -> Fut + Clone + Send + 'static,
    Fut: Future<Output = HttpOutcome> + Send + 'static,
{
    let (owner, policy, stop, path) = context(audit);
    let slot = FailureSlot::default();
    let faults = Faults::default();
    let (a, b) = tokio::io::duplex(512 * 1024);
    let socket = WebSocketStream::from_raw_socket(a, WsRole::Client, None).await;
    let mut peer = WebSocketStream::from_raw_socket(b, WsRole::Server, None).await;
    let (observer, rx) = oneshot::channel();
    let socket_stop = stop.clone();
    let socket_slot = slot.clone();
    let task = tokio::spawn(drive(
        owner,
        at(T - 3600000),
        policy,
        stop.clone(),
        slot,
        faults.clone(),
        move |tx, feedback, start| {
            observer.send(feedback.clone()).unwrap();
            crate::socket::primary::fixture_pump(
                socket,
                socket_stop,
                socket_slot,
                tx,
                feedback,
                start,
            )
        },
        fetch,
    ));
    peer.send(Message::Text(
        r#"{"type":"EVENT","eventInfo":{"event":"HANDSHAKE_SUCCESS"}}"#.into(),
    ))
    .await
    .unwrap();
    assert!(matches!(
        peer.next().await.unwrap().unwrap(),
        Message::Text(_)
    ));
    let progress = rx.await.unwrap();
    until(|| {
        trace(&path)
            .iter()
            .any(|r| r["event"]["event"] == "primary_subscription_sent")
    })
    .await;
    (task, peer, progress, stop, faults, path)
}

#[test]
fn physical_ws_runs_two_actual_model_commits_without_regular_rest() {
    with_fixture_clock(T - 100, || {
        runtime(async {
            let (task, mut peer, mut progress, stop, _, path) = start(false, never).await;
            for i in 0..=20 {
                let stamp = T + i * 60000 + 100;
                go(stamp);
                peer.send(data(vec![raw_row(stamp - 100)])).await.unwrap();
                accepted(&mut progress, stamp).await;
                if i == 10 || i == 20 {
                    go(stamp + 1000);
                    peer.send(data(vec![raw_row(stamp - 100)])).await.unwrap();
                    until(|| {
                        trace(&path)
                            .iter()
                            .filter(|r| r["kind"] == "canonical_commit")
                            .count()
                            == (i / 10) as usize
                    })
                    .await;
                    // A durable row can become visible before the post-write
                    // fence and processor commit. Await the completed writer
                    // response, not just a file marker, before operator STOP.
                    progress.changed().await.unwrap();
                    progress.borrow_and_update();
                }
            }
            stop.stop();
            let report = task.await.unwrap().unwrap();
            assert!(report.all_workers_drained);
            assert!(report.session_finalized, "{report:?}");
            assert_eq!(report.canonical_commits, 2);
            assert_eq!(report.http.proof.scheduled, 0);
            assert_eq!(report.http.audit.scheduled, 0);
            assert!(matches!(report.cause, Cause::RequestedStop));
            assert!(!report.full_session_qualified);
            let rows = trace(&path);
            assert_eq!(rows.iter().filter(|r| r["kind"] == "model_step").count(), 2);
            assert!(rows
                .iter()
                .filter(|r| r["kind"] == "canonical_source")
                .all(|r| r["event"]["receipt"]["origin"] == "WS_LIVE"));
        })
    });
}

#[test]
fn stop_cancels_pending_bootstrap_io_and_drains_without_model_effect() {
    with_fixture_clock(T - 100, || {
        runtime(async {
            let entered = Arc::new(std::sync::atomic::AtomicBool::new(false));
            let flag = entered.clone();
            let fetch = move |_job: Job, _stop: StopHandle| {
                let flag = flag.clone();
                async move {
                    flag.store(true, std::sync::atomic::Ordering::SeqCst);
                    std::future::pending::<HttpOutcome>().await
                }
            };
            let (task, mut peer, _, stop, _, path) = start(false, fetch).await;
            go(T + 100);
            peer.send(data(vec![raw_row(T - 600000)])).await.unwrap();
            until(|| entered.load(std::sync::atomic::Ordering::SeqCst)).await;
            stop.stop();
            let report = task.await.unwrap().unwrap();
            assert!(report.all_workers_drained);
            assert_eq!(report.canonical_commits, 0);
            assert_eq!(report.http.bootstrap.cancelled_or_unaccounted, 1);
            assert!(matches!(report.cause, Cause::RequestedStop));
            assert!(!trace(&path).iter().any(|r| r["kind"] == "model_step"));
        })
    });
}

#[test]
fn fresh_frames_queued_during_bootstrap_still_obey_original_ingress_age() {
    with_fixture_clock(T - 100, || {
        runtime(async {
            let release = Arc::new(tokio::sync::Notify::new());
            let gate = release.clone();
            let entered = Arc::new(std::sync::atomic::AtomicBool::new(false));
            let flag = entered.clone();
            let fetch = move |job: Job, _stop: StopHandle| {
                let gate = gate.clone();
                let flag = flag.clone();
                async move {
                    flag.store(true, std::sync::atomic::Ordering::SeqCst);
                    gate.notified().await;
                    source(&job, vec![raw_row(T - 600000)])
                }
            };
            let (task, mut peer, progress, _, _, path) = start(false, fetch).await;
            go(T + 100);
            peer.send(data(vec![raw_row(T - 600000)])).await.unwrap();
            until(|| entered.load(std::sync::atomic::Ordering::SeqCst)).await;
            go(T + 500);
            peer.send(data(vec![raw_row(T)])).await.unwrap();
            // Let the socket receive this frame at T+500 before advancing wall time.
            for _ in 0..8 {
                tokio::task::yield_now().await;
            }
            go(T + 3100);
            release.notify_one();
            let report = task.await.unwrap().unwrap();
            assert!(matches!(
                report.cause,
                Cause::Coordinator(CoordinatorError::Source(PrimaryError::Continuity))
            ));
            assert!(report.all_workers_drained);
            assert_eq!(report.canonical_commits, 0);
            assert!(progress.borrow().is_none());
            assert!(!trace(&path).iter().any(|r| r["kind"] == "model_step"));
        })
    });
}

#[test]
fn auth_failure_cause_survives_stop_before_http_response_dispatch() {
    with_fixture_clock(T - 100, || {
        runtime(async {
            let fetch = |job: Job, _stop: StopHandle| async move {
                failure(RestFailure::Authentication, &job)
            };
            let (task, mut peer, _, _, _, path) = start(false, fetch).await;
            go(T + 100);
            peer.send(data(vec![raw_row(T - 600000)])).await.unwrap();
            let report = task.await.unwrap().unwrap();
            assert!(matches!(
                report.cause,
                Cause::Http(RestFailure::Authentication)
            ));
            assert!(report.all_workers_drained);
            assert_eq!(report.canonical_commits, 0);
            assert!(!trace(&path).iter().any(|r| r["kind"] == "model_step"));
        })
    });
}

#[test]
fn unavailable_audit_does_not_block_two_subsequent_actual_ws_model_steps() {
    with_fixture_clock(T - 100, || {
        runtime(async {
            let fetch = |job: Job, _stop: StopHandle| async move {
                assert!(matches!(job, Job::Audit(_)));
                failure(RestFailure::Unavailable, &job)
            };
            let (task, mut peer, mut progress, stop, _, path) = start(true, fetch).await;
            for i in 0..=80 {
                let stamp = T + i * 60000 + 100;
                go(stamp);
                peer.send(data(vec![raw_row(stamp - 100)])).await.unwrap();
                accepted(&mut progress, stamp).await;
                if i > 0 && i % 10 == 0 {
                    go(stamp + 1000);
                    peer.send(data(vec![raw_row(stamp - 100)])).await.unwrap();
                    until(|| {
                        trace(&path)
                            .iter()
                            .filter(|r| r["kind"] == "canonical_commit")
                            .count()
                            == (i / 10) as usize
                    })
                    .await;
                    // The durable predicate is the barrier. watch coalesces
                    // updates, so a second notification is not guaranteed.
                    progress.borrow_and_update();
                    if i == 60 {
                        until(|| {
                            trace(&path).iter().any(|r| {
                                r["event"]["event"] == "primary_audit_result"
                                    && r["event"]["status"] == "Unavailable"
                            })
                        })
                        .await;
                        progress.borrow_and_update();
                    }
                }
            }
            stop.stop();
            let report = task.await.unwrap().unwrap();
            assert!(report.all_workers_drained);
            assert_eq!(report.canonical_commits, 8);
            assert_eq!(report.http.audit.scheduled, 1);
            assert_eq!(report.http.audit.completed, 1);
            assert_eq!(report.http.proof.scheduled, 0);
            assert!(matches!(report.cause, Cause::RequestedStop));
        })
    });
}

#[test]
fn blocked_writer_does_not_block_actual_ws_ping_pong() {
    with_fixture_clock(T - 100, || {
        runtime(async {
            let (owner, _, stop, path) = context(false);
            let slot = FailureSlot::default();
            let faults = Faults::default();
            let mut writer =
                Writer::spawn(owner, at(T - 3600000), stop.clone(), faults, slot.clone()).unwrap();
            let (a, b) = tokio::io::duplex(512 * 1024);
            let socket = WebSocketStream::from_raw_socket(a, WsRole::Client, None).await;
            let mut peer = WebSocketStream::from_raw_socket(b, WsRole::Server, None).await;
            let (tx, _rx) = bounded_ingress(&PrimaryPolicy::default(), 1).unwrap();
            let (_feedback, acks) = watch::channel(None);
            let (start, started) = oneshot::channel();
            let task = tokio::spawn(crate::socket::primary::fixture_pump(
                socket,
                stop.clone(),
                slot,
                tx,
                acks,
                start,
            ));
            peer.send(Message::Text(
                r#"{"type":"EVENT","eventInfo":{"event":"HANDSHAKE_SUCCESS"}}"#.into(),
            ))
            .await
            .unwrap();
            peer.next().await.unwrap().unwrap();
            writer
                .call(Operation::Bind(started.await.unwrap()))
                .await
                .unwrap();
            tokio::time::pause();
            let pause = Arc::new(super::super::writer::tests::Pause::new());
            let gate = pause.clone();
            let inspect = async {
                while !gate.entered.load(std::sync::atomic::Ordering::SeqCst) {
                    assert!(!stop.is_stopped(), "writer stopped before test pause");
                    tokio::task::yield_now().await;
                }
                tokio::time::advance(Duration::from_secs(30)).await;
                assert!(matches!(
                    peer.next().await.unwrap().unwrap(),
                    Message::Ping(_)
                ));
                peer.flush().await.unwrap();
                for _ in 0..8 {
                    tokio::task::yield_now().await;
                }
                gate.release
                    .store(true, std::sync::atomic::Ordering::SeqCst);
            };
            let (result, _) = tokio::join!(writer.call(Operation::Pause(pause)), inspect);
            result.unwrap();
            stop.stop();
            let report = task.await.unwrap().unwrap();
            assert!(report.liveness.last_matched_pong_at_ms.is_some());
            assert!(report.liveness.last_live_bars_at_ms.is_none());
            writer
                .call(Operation::Final(
                    json!({"event":"test_blocked_writer_shutdown"}),
                ))
                .await
                .unwrap();
            let deadline = writer.drain_deadline();
            assert!(writer.join(deadline).await);
            assert!(!trace(&path).iter().any(|r| r["kind"] == "model_step"));
        })
    });
}

#[test]
fn actual_socket_bootstrap_proof_does_not_refresh_data_or_call_model() {
    with_fixture_clock(T - 100, || {
        runtime(async {
            let fetch = |job: Job, _stop: StopHandle| async move {
                assert!(matches!(job, Job::Bootstrap(_)));
                source(&job, vec![raw_row(T - 600000)])
            };
            let (task, mut peer, mut progress, stop, _, path) = start(false, fetch).await;
            go(T + 100);
            peer.send(data(vec![raw_row(T - 600000)])).await.unwrap();
            until(|| {
                trace(&path)
                    .iter()
                    .any(|r| r["event"]["event"] == "primary_bootstrap_proof_applied")
            })
            .await;
            assert!(progress.borrow().is_none());
            assert!(!trace(&path).iter().any(|r| r["kind"] == "model_step"));
            go(T + 200);
            peer.send(data(vec![raw_row(T)])).await.unwrap();
            accepted(&mut progress, T + 200).await;
            stop.stop();
            let report = task.await.unwrap().unwrap();
            assert_eq!(report.http.bootstrap.scheduled, 1);
            assert_eq!(report.http.bootstrap.known_get_send_attempts, 1);
            assert!(report.all_workers_drained);
        })
    });
}

#[test]
fn stalled_blocking_writer_is_not_reported_drained_after_stop_budget() {
    with_fixture_clock(T - 100, || {
        runtime(async {
            let (owner, _, stop, path) = context(false);
            let faults = Faults::default();
            let mut writer = Writer::spawn(
                owner,
                at(T - 3600000),
                stop.clone(),
                faults.clone(),
                FailureSlot::default(),
            )
            .unwrap();
            tokio::time::pause();
            let mut pause = super::super::writer::tests::Pause::new();
            pause.ignore_stop = true;
            let pause = Arc::new(pause);
            struct Release(Arc<super::super::writer::tests::Pause>);
            impl Drop for Release {
                fn drop(&mut self) {
                    self.0
                        .release
                        .store(true, std::sync::atomic::Ordering::SeqCst);
                }
            }
            let _release = Release(pause.clone());
            // Pin the stop-budget origin before advancing the paused clock;
            // otherwise scheduler ordering can move the origin by the advance.
            let deadline = writer.drain_deadline();
            let gate = pause.clone();
            let inspect = async {
                while !gate.entered.load(std::sync::atomic::Ordering::SeqCst) {
                    tokio::task::yield_now().await;
                }
                stop.stop();
                tokio::task::yield_now().await;
                tokio::time::advance(Duration::from_secs(15)).await;
            };
            let (result, _) = tokio::join!(writer.call(Operation::Pause(pause.clone())), inspect);
            assert!(matches!(result, Err(Cause::DrainTimeout)));
            assert!(!writer.join(deadline).await);
            assert!(matches!(faults.first(), Some(Cause::DrainTimeout)));
            pause
                .release
                .store(true, std::sync::atomic::Ordering::SeqCst);
            // Release the dummy blocked thread before tearing down the test runtime.
            // It can only leave incomplete owner-drop evidence, never a clean terminal.
            while !trace(&path).iter().any(|r| r["kind"] == "segment_end") {
                tokio::task::yield_now().await;
            }
            assert!(trace(&path)
                .iter()
                .filter(|r| r["kind"] == "segment_end")
                .all(|r| r["event"]["reason"]
                    .as_str()
                    .unwrap()
                    .starts_with("INCOMPLETE_")));
        })
    });
}

#[test]
fn remote_close_survives_shared_stop_and_is_not_clean_eod() {
    with_fixture_clock(T - 100, || {
        runtime(async {
            let (task, mut peer, _, _, faults, path) = start(false, never).await;
            peer.close(None).await.unwrap();
            let report = task.await.unwrap().unwrap();
            assert!(matches!(report.cause, Cause::Socket(_)));
            assert!(faults.first().is_some());
            assert!(report.all_workers_drained);
            assert!(!report.full_session_qualified);
            assert!(trace(&path)
                .iter()
                .any(|r| r["event"]["event"] == "primary_async_shutdown"));
        })
    });
}

#[test]
fn abort_closes_socket_writer_and_leaves_incomplete_terminal() {
    with_fixture_clock(T - 100, || {
        runtime(async {
            let (task, mut peer, _, stop, faults, path) = start(false, never).await;
            task.abort();
            assert!(task.await.unwrap_err().is_cancelled());
            assert!(stop.is_stopped());
            assert!(matches!(faults.first(), Some(Cause::Cancelled)));
            until(|| trace(&path).iter().any(|r| r["kind"] == "segment_end")).await;
            let message = tokio::time::timeout(Duration::from_secs(1), peer.next())
                .await
                .unwrap();
            assert!(!matches!(message, Some(Ok(Message::Text(_)))));
        })
    });
}

#[test]
fn mandatory_bootstrap_failure_is_not_retried_and_all_tasks_join() {
    with_fixture_clock(T - 100, || {
        runtime(async {
            let fetch = |job: Job, _stop: StopHandle| async move {
                failure(RestFailure::Unavailable, &job)
            };
            let (task, mut peer, _, _, _, path) = start(false, fetch).await;
            go(T + 100);
            peer.send(data(vec![raw_row(T - 600000)])).await.unwrap();
            let report = task.await.unwrap().unwrap();
            assert!(matches!(
                report.cause,
                Cause::Coordinator(CoordinatorError::Rest(RestFailure::Unavailable))
            ));
            assert_eq!(report.http.bootstrap.scheduled, 1);
            assert_eq!(report.http.bootstrap.completed, 1);
            assert!(report.all_workers_drained);
            assert!(!trace(&path).iter().any(|r| r["kind"] == "model_step"));
        })
    });
}
