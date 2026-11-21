use super::*;
use crate::primary::{
    bounded_ingress, IngressReceiver, PrimaryPolicy, PrimaryProcessor, SnapshotPresence,
    WsPrimaryConfig,
};
use broker_finam::{AccessToken, FinamAuthPolicy};
use serde_json::{json, Value};
use tokio::io::DuplexStream;
use tokio_tungstenite::tungstenite::protocol::Role;

struct Pair {
    task: tokio::task::JoinHandle<Result<Report, Failure>>,
    peer: WebSocketStream<DuplexStream>,
    rx: IngressReceiver,
    feedback: watch::Sender<Option<LiveDataStamp>>,
    started: oneshot::Receiver<Subscription>,
    stop: StopHandle,
    slot: FailureSlot,
}
fn lease(ttl: u64) -> AccessTokenLease {
    AccessTokenLease::new(
        AccessToken::new("synthetic-only"),
        std::time::Instant::now(),
        FinamAuthPolicy {
            token_ttl: Duration::from_secs(ttl),
            renew_before: Duration::ZERO,
        },
    )
}
async fn pair(lease: AccessTokenLease) -> Pair {
    pair_for(lease, broker_finam::ShadowInstrument::Imoexf).await
}
async fn pair_for(lease: AccessTokenLease, instrument: broker_finam::ShadowInstrument) -> Pair {
    let (a, b) = tokio::io::duplex(512 * 1024);
    let socket = WebSocketStream::from_raw_socket(a, Role::Client, Some(config())).await;
    let peer = WebSocketStream::from_raw_socket(b, Role::Server, Some(config())).await;
    let deadline = Utc::now() + chrono::Duration::hours(1);
    let (stop, _) = ReadOnlyCollector::new(deadline, Duration::from_secs(1)).unwrap();
    let slot = FailureSlot::default();
    let mut driver = Driver::new(socket, lease, 4, 1, stop.clone(), deadline, slot.clone());
    driver.instrument = instrument;
    let (tx, rx) = bounded_ingress(&PrimaryPolicy::default(), 1).unwrap();
    let (feedback, acks) = watch::channel(None);
    let (start, started) = oneshot::channel();
    let task = tokio::spawn(driver.pump(tx, acks, start));
    Pair {
        task,
        peer,
        rx,
        feedback,
        started,
        stop,
        slot,
    }
}

#[tokio::test]
async fn additional_ws_subscription_and_physical_data_are_exactly_selected() {
    for i in [
        broker_finam::ShadowInstrument::Usdrubf,
        broker_finam::ShadowInstrument::Riz6,
    ] {
        let mut p = pair_for(lease(900), i).await;
        p.peer
            .send(Message::Text(
                r#"{"type":"EVENT","eventInfo":{"event":"HANDSHAKE_SUCCESS"}}"#.into(),
            ))
            .await
            .unwrap();
        let Some(Ok(Message::Text(text))) = p.peer.next().await else {
            panic!("missing SUBSCRIBE")
        };
        let value: Value = serde_json::from_str(&text).unwrap();
        assert!(value.to_string().contains(i.venue_symbol()));
        assert!(!value.to_string().contains("IMOEXF"));
        let sub = (&mut p.started).await.unwrap();
        assert_eq!(sub.instrument(), i);
        let t = minute();
        let mut v = row(t);
        for k in ["open", "high", "low", "close"] {
            v[k] =
                json!({"value":if i==broker_finam::ShadowInstrument::Riz6{"100000"}else{"100.01"}});
        }
        p.peer.send(Message::Text(json!({"type":"DATA","subscriptionType":"BARS","subscriptionKey":"fixture","payload":{"symbol":i.venue_symbol(),"bars":[v]}}).to_string())).await.unwrap();
        let frame = p.rx.receive(&p.stop).await.unwrap().unwrap();
        assert_eq!(frame.rows[0].bar.instrument, i.instrument());
        assert!(Arc::ptr_eq(frame.owner.as_ref().unwrap(), &sub.owner));
        p.stop.stop();
        assert!(p.task.await.unwrap().is_ok());
    }
}
async fn subscribe(p: &mut Pair) -> Subscription {
    p.peer
        .send(Message::Text(
            r#"{"type":"EVENT","eventInfo":{"event":"HANDSHAKE_SUCCESS"}}"#.into(),
        ))
        .await
        .unwrap();
    let Some(Ok(Message::Text(text))) = p.peer.next().await else {
        panic!("missing subscription")
    };
    let value: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(value["action"], "SUBSCRIBE");
    (&mut p.started).await.unwrap()
}
fn minute() -> i64 {
    Utc::now().timestamp_millis().div_euclid(60000) * 60000
}
fn row(t: i64) -> Value {
    crate::session::primary_tests::raw_row(t)
}
fn data(rows: Vec<Value>) -> Message {
    Message::Text(
        json!({"type":"DATA","subscriptionType":"BARS","subscriptionKey":"fixture",
        "payload":{"symbol":"IMOEXF@RTSX","timeframe":"TIME_FRAME_M1","bars":rows}})
        .to_string(),
    )
}
fn processor(subscription: &Subscription) -> PrimaryProcessor {
    let config =
        WsPrimaryConfig::parse(&serde_json::to_vec(&crate::primary::tests::config()).unwrap())
            .unwrap();
    let instrument = broker_core::InstrumentId {
        symbol: "IMOEXF".into(),
        venue_symbol: Some("IMOEXF@RTSX".into()),
        exchange: broker_core::Exchange::Moex,
        market: broker_core::Market::Futures,
    };
    let mut processor = PrimaryProcessor::from_config(
        &config,
        instrument,
        vec![minute().div_euclid(600000) * 600000],
        1,
        subscription.sent_at_ms(),
    )
    .unwrap();
    processor
        .bind_subscription(subscription, Utc::now().timestamp_millis())
        .unwrap();
    processor
}
async fn yield_feedback() {
    for _ in 0..5 {
        tokio::task::yield_now().await;
    }
}

#[tokio::test]
async fn unmarked_bootstrap_is_transported_without_age_or_count_heuristics() {
    let mut p = pair(lease(900)).await;
    let receipt = subscribe(&mut p).await;
    assert_eq!(receipt.generation(), 1);
    assert_eq!(receipt.auth_generation(), 4);
    let end = minute();
    p.peer
        .send(data((1..=300).map(|i| row(end - i * 60000)).collect()))
        .await
        .unwrap();
    let frame = p.rx.receive(&p.stop).await.unwrap().unwrap();
    assert_eq!(frame.rows.len(), 300);
    assert!(frame
        .rows
        .iter()
        .all(|r| r.presence == SnapshotPresence::Missing));
    p.stop.stop();
    let report = p.task.await.unwrap().unwrap();
    assert_eq!(report.frames_forwarded, 1);
    assert!(report.liveness.last_live_bars_at_ms.is_none());
    assert!(p.slot.first().is_none());
}
#[tokio::test]
async fn only_processor_accepted_actual_frame_refreshes_data_and_duplicate_does_not() {
    let mut p = pair(lease(900)).await;
    let receipt = subscribe(&mut p).await;
    let mut owner = processor(&receipt);
    let label = minute();
    p.peer.send(data(vec![row(label)])).await.unwrap();
    let frame = p.rx.receive(&p.stop).await.unwrap().unwrap();
    let stamp = frame.received_at_ms;
    owner.ingest(frame, Utc::now().timestamp_millis()).unwrap();
    p.feedback.send(owner.last_live_data()).unwrap();
    yield_feedback().await;
    p.peer.send(data(vec![row(label)])).await.unwrap();
    let duplicate = p.rx.receive(&p.stop).await.unwrap().unwrap();
    owner
        .ingest(duplicate, Utc::now().timestamp_millis())
        .unwrap();
    assert_eq!(owner.counters().valid_updates, 1);
    assert_eq!(owner.last_live_data().unwrap().received_at_ms, stamp);
    p.feedback.send(owner.last_live_data()).unwrap();
    yield_feedback().await;
    p.stop.stop();
    let report = p.task.await.unwrap().unwrap();
    assert_eq!(report.liveness.last_live_bars_at_ms, Some(stamp));
}
#[tokio::test]
async fn unbound_public_normalization_cannot_forge_physical_subscription() {
    let mut p = pair(lease(900)).await;
    let receipt = subscribe(&mut p).await;
    let mut owner = processor(&receipt);
    let Message::Text(text) = data(vec![row(minute())]) else {
        unreachable!()
    };
    let frame = WireFrame::normalize(&text, 1, "fixture", Utc::now()).unwrap();
    assert_eq!(
        owner.ingest(frame, Utc::now().timestamp_millis()),
        Err(PrimaryError::Generation)
    );
    assert!(!owner.subscription_data_ready());
    p.stop.stop();
    p.task.await.unwrap().unwrap();
}
#[tokio::test]
async fn wrong_owner_ack_is_fenced_even_with_matching_generation() {
    let mut p = pair(lease(900)).await;
    let _receipt = subscribe(&mut p).await;
    p.feedback
        .send(Some(LiveDataStamp {
            generation: 1,
            received_at_ms: Utc::now().timestamp_millis(),
            received_mono: Instant::now(),
            owner: Some(Arc::new(())),
        }))
        .unwrap();
    let failure = p.task.await.unwrap().unwrap_err();
    assert_eq!(failure.kind, Kind::Source(PrimaryError::Generation));
    assert!(p.stop.is_stopped());
    assert_eq!(p.slot.first().unwrap().kind, failure.kind);
}

#[tokio::test]
async fn declared_subscribe_timestamp_without_physical_receipt_is_not_authority() {
    let config =
        WsPrimaryConfig::parse(&serde_json::to_vec(&crate::primary::tests::config()).unwrap())
            .unwrap();
    let instrument = broker_core::InstrumentId {
        symbol: "IMOEXF".into(),
        venue_symbol: Some("IMOEXF@RTSX".into()),
        exchange: broker_core::Exchange::Moex,
        market: broker_core::Market::Futures,
    };
    let now = Utc::now();
    let mut owner = PrimaryProcessor::from_config(
        &config,
        instrument,
        vec![minute().div_euclid(600000) * 600000],
        1,
        now.timestamp_millis(),
    )
    .unwrap();
    let Message::Text(text) = data(vec![row(minute())]) else {
        unreachable!()
    };
    let frame = WireFrame::normalize(&text, 1, "fixture", now).unwrap();
    assert_eq!(
        owner.ingest(frame, now.timestamp_millis()),
        Err(PrimaryError::Generation)
    );
    assert_eq!(
        owner.prepare(now.timestamp_millis()).err(),
        Some(PrimaryError::Generation)
    );
    assert!(!owner.subscription_data_ready());
}
#[tokio::test]
async fn malformed_last_row_does_not_forward_a_partial_frame() {
    let mut p = pair(lease(900)).await;
    subscribe(&mut p).await;
    let mut bad = row(minute());
    bad["high"] = json!({"value":"2198"});
    p.peer.send(data(vec![row(minute()), bad])).await.unwrap();
    let failure = p.task.await.unwrap().unwrap_err();
    assert!(matches!(failure.kind, Kind::Source(PrimaryError::Numeric)));
    assert!(p.stop.is_stopped());
    assert!(p.rx.receive(&p.stop).await.unwrap().is_none());
}
#[tokio::test]
async fn queue_overflow_latches_before_stop_without_blocking_socket() {
    let mut p = pair(lease(900)).await;
    subscribe(&mut p).await;
    for _ in 0..129 {
        p.peer.send(data(vec![row(minute())])).await.unwrap();
    }
    let failure = p.task.await.unwrap().unwrap_err();
    assert_eq!(failure.kind, Kind::Source(PrimaryError::Continuity));
    assert!(p.rx.continuity_lost());
    assert!(p.stop.is_stopped());
    assert_eq!(p.slot.first().unwrap().kind, failure.kind);
}
#[tokio::test(start_paused = true)]
async fn heartbeat_continues_with_consumer_blocked_and_does_not_prove_data() {
    let mut p = pair(lease(900)).await;
    subscribe(&mut p).await;
    let mut history = row(minute() - 600000);
    history["isDataSnapshot"] = json!(true);
    p.peer.send(data(vec![history])).await.unwrap();
    yield_feedback().await;
    tokio::time::advance(Duration::from_secs(30)).await;
    assert!(matches!(
        p.peer.next().await.unwrap().unwrap(),
        Message::Ping(_)
    ));
    p.peer.flush().await.unwrap();
    yield_feedback().await;
    p.stop.stop();
    let report = p.task.await.unwrap().unwrap();
    assert!(report.liveness.last_matched_pong_at_ms.is_some());
    assert!(report.liveness.last_live_bars_at_ms.is_none());
}
#[tokio::test]
async fn remote_close_is_a_fault_not_clean_eod() {
    let mut p = pair(lease(900)).await;
    subscribe(&mut p).await;
    p.peer.close(None).await.unwrap();
    let failure = p.task.await.unwrap().unwrap_err();
    assert_eq!(failure.kind, Kind::Close);
    assert!(p.stop.is_stopped());
}

#[tokio::test]
async fn r3_lmax_owner_fault_survives_connection_retirement_without_pump_panic() {
    let p = pair(lease(60)).await;
    let failure = Failure {
        generation: 1,
        kind: Kind::Source(crate::primary::PrimaryError::Continuity),
        liveness: None,
    };
    p.slot.record(failure.clone());
    p.stop.stop();
    let returned = p
        .task
        .await
        .expect("drained Driver must not panic")
        .unwrap_err();
    assert_eq!(returned.kind, failure.kind);
    assert_eq!(p.slot.first().unwrap().kind, failure.kind);
}
#[tokio::test]
async fn cancelled_socket_retains_first_fault_and_stops_shared_run() {
    let mut p = pair(lease(900)).await;
    subscribe(&mut p).await;
    p.task.abort();
    assert!(p.task.await.unwrap_err().is_cancelled());
    assert!(p.stop.is_stopped());
    assert_eq!(p.slot.first().unwrap().kind, Kind::Cancelled);
}
#[tokio::test]
async fn subscribed_socket_is_not_replaced_when_command_jwt_expires() {
    let mut p = pair(lease(1)).await;
    subscribe(&mut p).await;
    tokio::time::sleep(Duration::from_millis(1100)).await;
    p.peer.send(data(vec![row(minute())])).await.unwrap();
    assert!(p.rx.receive(&p.stop).await.unwrap().is_some());
    assert!(!p.stop.is_stopped());
    p.stop.stop();
    p.task.await.unwrap().unwrap();
}
#[tokio::test]
async fn expired_command_grant_cannot_subscribe() {
    let expired = AccessTokenLease::new(
        AccessToken::new("synthetic-only"),
        std::time::Instant::now() - Duration::from_secs(2),
        FinamAuthPolicy {
            token_ttl: Duration::from_secs(1),
            renew_before: Duration::ZERO,
        },
    );
    let mut p = pair(expired).await;
    p.peer
        .send(Message::Text(
            r#"{"type":"EVENT","eventInfo":{"event":"HANDSHAKE_SUCCESS"}}"#.into(),
        ))
        .await
        .unwrap();
    assert_eq!(p.task.await.unwrap().unwrap_err().kind, Kind::Lease);
    assert!(p.stop.is_stopped());
}

#[test]
fn expected_run_stop_cancellation_does_not_invent_or_erase_a_market_fault() {
    let root = StopHandle::connection_scope();
    let gate = crate::primary::ContinuityGate::new(1);
    let slot = FailureSlot::with_gate(gate.clone(), root.clone());
    root.stop();
    slot.record(Failure {
        generation: 1,
        kind: Kind::Cancelled,
        liveness: None,
    });
    assert!(gate.healthy());
    slot.record(Failure {
        generation: 1,
        kind: Kind::Source(PrimaryError::Numeric),
        liveness: None,
    });
    assert!(!gate.healthy());
    slot.record(Failure {
        generation: 1,
        kind: Kind::Cancelled,
        liveness: None,
    });
    assert_eq!(
        gate.snapshot().failure.unwrap().kind,
        Kind::Source(PrimaryError::Numeric)
    );
}
