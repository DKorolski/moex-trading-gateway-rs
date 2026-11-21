use super::*;
use broker_finam::{AccessToken, AccessTokenLease, FinamAuthPolicy};
use futures_util::{SinkExt, StreamExt};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex,
};
use tokio::{net::TcpListener, task::JoinHandle};
use tokio_tungstenite::tungstenite::Message;

#[derive(Clone, Copy)]
pub(super) enum Peer {
    Fresh,
    Close,
    Quiet,
    Malformed,
    ProviderError,
}
pub(super) async fn peer(
    cases: Vec<Peer>,
) -> (
    std::net::SocketAddr,
    Arc<Mutex<Vec<serde_json::Value>>>,
    JoinHandle<()>,
) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let requests = Arc::new(Mutex::new(Vec::new()));
    let log = requests.clone();
    let task = tokio::spawn(async move {
        for case in cases {
            let (tcp, _) = listener.accept().await.unwrap();
            let mut ws = tokio_tungstenite::accept_async(tcp).await.unwrap();
            ws.send(Message::Text(
                r#"{"type":"EVENT","eventInfo":{"event":"HANDSHAKE_SUCCESS"}}"#.into(),
            ))
            .await
            .unwrap();
            let Some(Ok(Message::Text(request))) = ws.next().await else {
                continue;
            };
            log.lock()
                .unwrap()
                .push(serde_json::from_str(&request).unwrap());
            match case {
                Peer::Fresh => ws.send(crate::socket::tests::fresh_bar()).await.unwrap(),
                Peer::Close => {
                    ws.close(None).await.ok();
                    continue;
                }
                Peer::Quiet => ws
                    .send(Message::Ping(b"fixture-health".to_vec()))
                    .await
                    .unwrap(),
                Peer::Malformed => ws
                    .send(Message::Text("malformed-fixture".into()))
                    .await
                    .unwrap(),
                Peer::ProviderError => ws
                    .send(Message::Text(
                        r#"{"type":"ERROR","errorInfo":{"message":"fixture"}}"#.into(),
                    ))
                    .await
                    .unwrap(),
            }
            while let Some(Ok(frame)) = ws.next().await {
                if matches!(frame, Message::Close(_)) {
                    break;
                }
                if matches!(frame, Message::Ping(_)) {
                    ws.flush().await.ok();
                }
            }
        }
    });
    (addr, requests, task)
}

pub(super) fn current(n: usize) -> Grant {
    let at = std::time::Instant::now();
    Grant {
        lease: AccessTokenLease::new(
            AccessToken::new(format!("fixture-grant-{n}")),
            at,
            FinamAuthPolicy {
                token_ttl: Duration::from_secs(3600),
                renew_before: Duration::ZERO,
            },
        ),
        generation: n as u64,
        renew_at: at + Duration::from_secs(3000),
        expires_at: Utc::now() + chrono::Duration::hours(1),
    }
}
pub(super) fn factory(addr: std::net::SocketAddr) -> (StopHandle, RecoveryFactory) {
    let (stop, budget) = crate::transport::ReadOnlyCollector::new(
        Utc::now() + chrono::Duration::hours(1),
        Duration::from_secs(1),
    )
    .unwrap();
    (stop, RecoveryFactory::local_fixture(budget, addr))
}

fn budget_with_fault() -> (RunBudget, FaultGuard) {
    let audit = Audit::default();
    let error = TransportError::Socket(crate::socket::SocketFailure {
        stage: crate::socket::SocketStage::Receive,
        code: SocketCode::Eof,
        timestamp_bounds: None,
        quote_time: None,
        quote_conflict: None,
    });
    let guard = audit.latch_fault(&error, 1, 1, None);
    (RunBudget::with_audit(audit), guard)
}

#[tokio::test]
async fn actual_ws_reconnect_gets_grant_subscribes_and_requires_data_without_model_callbacks() {
    let (addr, requests, peer) = peer(vec![Peer::Close, Peer::Fresh]).await;
    let (stop, f) = factory(addr);
    let (tx, mut rx) = tokio::sync::mpsc::channel(128);
    let calls = Arc::new(AtomicUsize::new(0));
    let counter = calls.clone();
    let (mut run, _fault_guard) = budget_with_fault();
    let restored = restore(
        f,
        move || {
            let n = counter.fetch_add(1, Ordering::SeqCst) + 1;
            async move { Ok(current(n)) }
        },
        stop,
        &tx,
        &mut run,
        7,
        Reason::Eof,
        Duration::from_secs(1),
    )
    .await
    .unwrap();
    assert_eq!(restored.generation, 9);
    assert_eq!(restored.grant.generation, 2);
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert_eq!(run.episodes, 1);
    let mut ready = 0;
    let mut last_gen = 7;
    while let Ok(event) = rx.try_recv() {
        match event {
            Event::Snapshot(..) => {
                panic!("recovery must never publish canonical snapshots/callbacks")
            }
            Event::Liveness(v) if v["event"] == "bars_recovery_attempt" => {
                let g = v["detail"]["connection_generation"].as_u64().unwrap();
                assert!(g > last_gen);
                last_gen = g;
            }
            Event::Liveness(v) if v["event"] == "bars_recovery_succeeded" => {
                assert_eq!(v["detail"]["fresh_data_observed"], true);
                ready += 1;
            }
            _ => (),
        }
    }
    assert_eq!(ready, 1);
    {
        let requests = requests.lock().unwrap();
        assert_eq!(requests.len(), 2);
        assert_ne!(requests[0]["token"], requests[1]["token"]);
    }
    drop(restored);
    peer.await.unwrap();
}

#[tokio::test]
async fn repeated_provider_fault_exhausts_three_attempts() {
    let (addr, requests, peer) = peer(vec![Peer::ProviderError; 3]).await;
    let (stop, f) = factory(addr);
    let (tx, _rx) = tokio::sync::mpsc::channel(128);
    let (mut run, _fault_guard) = budget_with_fault();
    let result = restore(
        f,
        || async { Ok(current(1)) },
        stop,
        &tx,
        &mut run,
        1,
        Reason::Provider,
        Duration::from_secs(1),
    )
    .await;
    assert!(matches!(
        result,
        Err("runner_bars_recovery_attempts_exhausted")
    ));
    assert_eq!(requests.lock().unwrap().len(), 3);
    peer.await.unwrap();
}

#[tokio::test]
async fn handshake_and_server_ping_do_not_replace_first_live_data() {
    let (addr, requests, peer) = peer(vec![Peer::Quiet; 3]).await;
    let (stop, f) = factory(addr);
    let (tx, mut rx) = tokio::sync::mpsc::channel(128);
    let (mut run, _fault_guard) = budget_with_fault();
    let result = restore(
        f,
        || async { Ok(current(1)) },
        stop,
        &tx,
        &mut run,
        1,
        Reason::BarsDataSilence,
        Duration::from_millis(100),
    )
    .await;
    assert!(matches!(
        result,
        Err("runner_bars_recovery_attempts_exhausted")
    ));
    assert_eq!(requests.lock().unwrap().len(), 3);
    assert_eq!(
        run.audit.snapshot()["terminal_code"],
        "runner_bars_recovery_attempts_exhausted"
    );
    assert_eq!(run.audit.snapshot()["attempt"], 3);
    while let Ok(e) = rx.try_recv() {
        if let Event::Liveness(v) = e {
            assert_ne!(v["event"], "bars_recovery_succeeded");
        }
    }
    peer.await.unwrap();
}

#[tokio::test]
async fn malformed_frame_is_not_retried_as_a_network_error() {
    let (addr, requests, peer) = peer(vec![Peer::Malformed]).await;
    let (stop, f) = factory(addr);
    let (tx, _rx) = tokio::sync::mpsc::channel(128);
    let (mut run, _fault_guard) = budget_with_fault();
    assert!(matches!(
        restore(
            f,
            || async { Ok(current(1)) },
            stop,
            &tx,
            &mut run,
            1,
            Reason::Eof,
            Duration::from_secs(1)
        )
        .await,
        Err("runner_bars_recovery_protocol")
    ));
    assert_eq!(requests.lock().unwrap().len(), 1);
    peer.await.unwrap();
}

#[tokio::test]
async fn stop_interrupts_backoff_without_auth_or_connect() {
    let (stop, f) = factory("127.0.0.1:9".parse().unwrap());
    let (tx, _rx) = tokio::sync::mpsc::channel(128);
    let (mut run, _fault_guard) = budget_with_fault();
    let task = tokio::spawn({
        let stop = stop.clone();
        async move {
            tokio::time::sleep(Duration::from_millis(30)).await;
            stop.stop();
        }
    });
    let result = restore(
        f,
        || async { panic!("auth must not run") },
        stop,
        &tx,
        &mut run,
        1,
        Reason::Pong,
        Duration::from_secs(1),
    )
    .await;
    assert!(matches!(result, Err("runner_stopped")));
    task.await.unwrap();
}

#[tokio::test(start_paused = true)]
async fn total_budget_cancels_slow_auth_and_run_episode_limit_is_finite() {
    let (stop, f) = factory("127.0.0.1:9".parse().unwrap());
    let (tx, _rx) = tokio::sync::mpsc::channel(128);
    let (mut run, _fault_guard) = budget_with_fault();
    let start = Instant::now();
    let result = restore(
        f,
        std::future::pending,
        stop,
        &tx,
        &mut run,
        1,
        Reason::Pong,
        Duration::from_secs(1),
    )
    .await;
    assert!(matches!(result, Err("runner_bars_recovery_deadline")));
    assert_eq!(start.elapsed(), Duration::from_secs(45));
    run.episodes = 3;
    let (stop, f) = factory("127.0.0.1:9".parse().unwrap());
    assert!(matches!(
        restore(
            f,
            || async { panic!("auth must not run") },
            stop,
            &tx,
            &mut run,
            1,
            Reason::Pong,
            Duration::from_secs(1)
        )
        .await,
        Err("runner_bars_recovery_run_budget")
    ));
}

#[tokio::test]
async fn failed_auth_is_terminal_before_any_socket_attempt() {
    let (stop, f) = factory("127.0.0.1:9".parse().unwrap());
    let (tx, _rx) = tokio::sync::mpsc::channel(128);
    let (mut run, _fault_guard) = budget_with_fault();
    assert!(matches!(
        restore(
            f,
            || async { Err("auth_permissions") },
            stop,
            &tx,
            &mut run,
            1,
            Reason::Eof,
            Duration::from_secs(1)
        )
        .await,
        Err("auth_permissions")
    ));
}

#[test]
fn absolute_deadline_stop_and_bad_frame_are_not_reconnect_causes() {
    assert_eq!(Reason::classify(&TransportError::Stopped), None);
    assert_eq!(Reason::classify(&TransportError::Deadline), None);
    assert_eq!(Reason::classify(&TransportError::Clock), None);
    let e = TransportError::Socket(crate::socket::SocketFailure {
        stage: crate::socket::SocketStage::ProtocolReceive,
        code: SocketCode::Protocol(crate::ws::WsError::FrameAt(
            crate::ws::FrameCode::TimestampBounds,
        )),
        timestamp_bounds: None,
        quote_time: None,
        quote_conflict: None,
    });
    assert_eq!(Reason::classify(&e), None);
}

#[test]
fn actual_runtime_policy_discloses_recovery_without_claiming_canonical_resync() {
    let policy = crate::socket::candle_liveness_policy();
    assert_eq!(policy["version"], "candle_bars_heartbeat_recovery_v2");
    assert_eq!(policy["automatic_reconnect"], true);
    assert_eq!(policy["automatic_canonical_resync"], false);
    assert_eq!(policy["recovery_episode_budget_ms"], 45000);
    assert_eq!(policy["recovery_run_episode_limit"], 3);
}

#[tokio::test]
async fn stop_during_first_data_drops_candidate_and_does_not_wait_out_budget() {
    let (addr, _, peer) = peer(vec![Peer::Quiet]).await;
    let (stop, f) = factory(addr);
    let (tx, _rx) = tokio::sync::mpsc::channel(128);
    let (mut run, _fault_guard) = budget_with_fault();
    let task = tokio::spawn({
        let stop = stop.clone();
        async move {
            tokio::time::sleep(Duration::from_millis(1200)).await;
            stop.stop();
        }
    });
    let result = restore(
        f,
        || async { Ok(current(1)) },
        stop,
        &tx,
        &mut run,
        1,
        Reason::Eof,
        Duration::from_secs(5),
    )
    .await;
    assert!(matches!(result, Err("runner_stopped")));
    assert_eq!(run.audit.snapshot()["terminal_code"], "runner_stopped");
    assert_eq!(run.audit.snapshot()["last_episode_succeeded"], false);
    task.await.unwrap();
    peer.await.unwrap();
}
