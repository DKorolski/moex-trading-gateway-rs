use super::*;
use crate::{
    socket::tests::{fresh_bar, subscribed_pair},
    transport::StopHandle,
};
use tokio::io::DuplexStream;

async fn pair() -> (
    StopHandle,
    Session<DuplexStream>,
    WebSocketStream<DuplexStream>,
) {
    let (_, mut client, server) = subscribed_pair().await;
    let (stop, budget) = ReadOnlyCollector::new(
        Utc::now() + chrono::Duration::hours(1),
        Duration::from_secs(1),
    )
    .unwrap();
    client.budget = budget;
    (stop, client, server)
}

fn is_pong_timeout(e: TransportError) -> bool {
    matches!(
        e,
        TransportError::Socket(SocketFailure {
            code: SocketCode::Liveness(TimeoutKind::Pong),
            ..
        })
    )
}

#[tokio::test(start_paused = true)]
async fn quiet_bars_with_real_ping_pong_continue_until_market_silence_deadline() {
    let (_stop, mut client, mut server) = pair().await;
    let peer = tokio::spawn(async move {
        while let Some(Ok(frame)) = server.next().await {
            if matches!(frame, Message::Ping(_)) {
                server.flush().await.unwrap();
            }
        }
    });
    let mut warned = 0;
    let mut pongs = 0;
    loop {
        match client.observe().await {
            Ok(_) => {
                if let Some(e) = client.watchdog.take_event() {
                    if e["cause"] == "BarsDataSilence" {
                        warned += 1;
                    }
                    if e["cause"] == "PongConfirmed" {
                        pongs += 1;
                    }
                }
            }
            Err(TransportError::Socket(SocketFailure {
                code: SocketCode::Liveness(TimeoutKind::BarsDataSilence),
                ..
            })) => break,
            other => panic!("unexpected {other:?}"),
        }
    }
    assert_eq!(warned, 1);
    assert!(pongs >= 39);
    assert_eq!(client.watchdog.snapshot().bars_data_age_ms, 1_200_000);
    assert!(client.socket.is_none());
    peer.abort();
}

#[tokio::test(start_paused = true)]
async fn no_pong_stops_at_fifteen_seconds_after_ping() {
    let (_stop, mut client, _server) = pair().await;
    assert!(matches!(
        client.observe().await.unwrap(),
        Observation::Ignore
    ));
    assert_eq!(client.watchdog.snapshot().pending_pong_age_ms, Some(0));
    assert!(is_pong_timeout(client.observe().await.unwrap_err()));
    assert_eq!(client.watchdog.snapshot().pending_pong_age_ms, Some(15000));
    assert!(client.socket.is_none());
}

#[tokio::test(start_paused = true)]
async fn wrong_pong_and_continuous_data_do_not_cancel_pending_ping() {
    let (_stop, mut client, mut server) = pair().await;
    client.observe().await.unwrap();
    assert!(matches!(
        server.next().await.unwrap().unwrap(),
        Message::Ping(_)
    ));
    // Override tungstenite's queued auto-Pong with a wrong payload.
    server.send(Message::Pong(b"wrong".to_vec())).await.unwrap();
    client.observe().await.unwrap();
    assert!(client.watchdog.snapshot().pending_pong_age_ms.is_some());
    server.send(fresh_bar()).await.unwrap();
    assert_eq!(client.observe().await.unwrap().live_bars().len(), 1);
    assert!(is_pong_timeout(client.observe().await.unwrap_err()));
}

#[tokio::test(start_paused = true)]
async fn late_matching_pong_is_rejected_before_resetting_watchdog() {
    let (_stop, mut client, mut server) = pair().await;
    client.observe().await.unwrap();
    let Message::Ping(payload) = server.next().await.unwrap().unwrap() else {
        panic!()
    };
    tokio::time::advance(Duration::from_secs(15)).await;
    server.send(Message::Pong(payload)).await.unwrap();
    assert!(is_pong_timeout(client.observe().await.unwrap_err()));
    assert!(client.watchdog.snapshot().last_matched_pong_at_ms.is_none());
}

#[tokio::test(start_paused = true)]
async fn server_ping_gets_matching_pong_without_claiming_bars_data() {
    let (_stop, mut client, mut server) = pair().await;
    server
        .send(Message::Ping(b"server-probe".to_vec()))
        .await
        .unwrap();
    client.observe().await.unwrap();
    assert_eq!(
        server.next().await.unwrap().unwrap(),
        Message::Pong(b"server-probe".to_vec())
    );
    assert!(client.watchdog.snapshot().last_rx_at_ms.is_some());
    assert!(client.watchdog.snapshot().last_live_bars_at_ms.is_none());
}

#[tokio::test(start_paused = true)]
async fn shared_stop_and_absolute_deadline_interrupt_heartbeat_wait() {
    let (stop, mut client, _server) = pair().await;
    stop.stop();
    assert_eq!(client.observe().await.unwrap_err(), TransportError::Stopped);
    assert!(client.socket.is_none());
    let (_, mut client, _server) = pair().await;
    let (_stop, budget) = ReadOnlyCollector::new(
        Utc::now() + chrono::Duration::milliseconds(500),
        Duration::from_millis(100),
    )
    .unwrap();
    client.budget = budget;
    assert_eq!(
        client.observe().await.unwrap_err(),
        TransportError::Deadline
    );
    assert_eq!(
        client.watchdog.snapshot().timeout,
        Some(TimeoutKind::RunDeadline)
    );
    assert!(client.socket.is_none());
}

#[tokio::test(start_paused = true)]
async fn eof_and_provider_error_remain_fatal() {
    let (_stop, mut client, server) = pair().await;
    drop(server);
    assert!(client.observe().await.is_err());
    assert!(client.socket.is_none());
    let (_stop, mut client, mut server) = pair().await;
    server
        .send(Message::Text(
            r#"{"type":"ERROR","errorInfo":{"message":"fixture"}}"#.into(),
        ))
        .await
        .unwrap();
    assert!(matches!(
        client.observe().await,
        Err(TransportError::Socket(SocketFailure {
            code: SocketCode::Protocol(crate::ws::WsError::Provider),
            ..
        }))
    ));
}

#[tokio::test(start_paused = true)]
async fn stalled_ping_send_is_bounded_and_drops_socket() {
    let (_stop, mut client, mut server) = pair().await;
    // Fill the outbound duplex buffer while the peer does not read.
    let socket = client.socket.as_mut().unwrap();
    let _ = tokio::time::timeout(
        Duration::from_millis(1),
        socket.send(Message::Binary(vec![0; 32768])),
    )
    .await;
    let error = client.observe().await.unwrap_err();
    assert!(matches!(
        error,
        TransportError::Socket(SocketFailure {
            code: SocketCode::Liveness(TimeoutKind::HeartbeatSend),
            ..
        })
    ));
    assert!(client.socket.is_none());
    server.close(None).await.ok();
}

#[tokio::test(start_paused = true)]
async fn data_after_warning_resumes_and_stale_frame_is_still_rejected() {
    let (_stop, mut client, mut server) = pair().await;
    client
        .watchdog
        .warn(tokio::time::Instant::now() + Duration::from_secs(60));
    client.watchdog.take_event();
    server.send(fresh_bar()).await.unwrap();
    assert_eq!(client.observe().await.unwrap().live_bars().len(), 1);
    assert_eq!(
        client.watchdog.take_event().unwrap()["cause"],
        "BarsDataResumed"
    );
    let Message::Text(text) = fresh_bar() else {
        panic!()
    };
    let mut frame: serde_json::Value = serde_json::from_str(&text).unwrap();
    frame["payload"]["bars"][0]["timestamp"] =
        serde_json::json!((Utc::now() - chrono::Duration::hours(2)).to_rfc3339());
    server.send(Message::Text(frame.to_string())).await.unwrap();
    assert!(matches!(
        client.observe().await,
        Err(TransportError::Socket(SocketFailure {
            code: SocketCode::Protocol(crate::ws::WsError::FrameAt(
                crate::ws::FrameCode::TimestampBounds
            )),
            ..
        }))
    ));
}

#[tokio::test(start_paused = true)]
async fn stalled_server_pong_flush_is_bounded() {
    let (_stop, mut client, mut server) = pair().await;
    let _ = tokio::time::timeout(
        Duration::from_millis(1),
        client
            .socket
            .as_mut()
            .unwrap()
            .send(Message::Binary(vec![0; 32768])),
    )
    .await;
    server.send(Message::Ping(b"probe".to_vec())).await.unwrap();
    assert!(matches!(
        client.observe().await,
        Err(TransportError::Socket(SocketFailure {
            code: SocketCode::Liveness(TimeoutKind::ServerPongFlush),
            ..
        }))
    ));
    assert!(client.socket.is_none());
}

#[tokio::test(start_paused = true)]
async fn stop_interrupts_blocked_heartbeat_send() {
    let (stop, mut client, _server) = pair().await;
    let _ = tokio::time::timeout(
        Duration::from_millis(1),
        client
            .socket
            .as_mut()
            .unwrap()
            .send(Message::Binary(vec![0; 32768])),
    )
    .await;
    let stop_task = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(30100)).await;
        stop.stop();
    });
    assert_eq!(client.observe().await.unwrap_err(), TransportError::Stopped);
    assert!(client.socket.is_none());
    stop_task.await.unwrap();
}
