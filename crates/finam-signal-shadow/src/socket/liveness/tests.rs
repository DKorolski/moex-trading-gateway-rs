use super::*;

#[tokio::test(start_paused = true)]
async fn ping_deadline_is_measured_from_successful_send_and_only_matching_pong_clears_it() {
    let mut w = Watchdog::new();
    tokio::time::advance(PING).await;
    assert!(w.ping_due(Instant::now()));
    let payload = w.payload();
    // Time spent preparing/sending does not consume the Pong allowance.
    tokio::time::advance(Duration::from_secs(2)).await;
    w.sent(Instant::now());
    tokio::time::advance(Duration::from_secs(14)).await;
    w.received(Instant::now());
    w.data(Instant::now());
    w.pong(b"wrong", Instant::now());
    assert_eq!(w.check(Instant::now()), Ok(()));
    tokio::time::advance(Duration::from_secs(1)).await;
    assert_eq!(w.check(Instant::now()), Err(TimeoutKind::Pong));
    assert_ne!(payload, b"wrong");
}

#[tokio::test(start_paused = true)]
async fn matched_pong_proves_transport_but_never_market_data() {
    let mut w = Watchdog::new();
    tokio::time::advance(PING).await;
    let payload = w.payload();
    w.sent(Instant::now());
    tokio::time::advance(Duration::from_secs(1)).await;
    w.pong(&payload, Instant::now());
    let s = w.snapshot();
    assert!(s.pending_pong_age_ms.is_none());
    assert!(s.last_matched_pong_at_ms.is_some());
    assert!(s.last_live_bars_at_ms.is_none());
    assert_eq!(s.bars_data_age_ms, 31000);
}

#[tokio::test(start_paused = true)]
async fn sixty_second_warning_is_once_until_real_data_resumes() {
    let mut w = Watchdog::new();
    tokio::time::advance(WARN).await;
    assert!(w.warn(Instant::now()));
    assert!(!w.warn(Instant::now()));
    assert_eq!(w.take_event().unwrap()["cause"], "BarsDataSilence");
    w.data(Instant::now());
    assert_eq!(w.take_event().unwrap()["cause"], "BarsDataResumed");
    assert_eq!(w.snapshot().bars_data_age_ms, 0);
}

#[tokio::test(start_paused = true)]
async fn healthy_pongs_cannot_extend_twenty_minute_data_deadline() {
    let mut w = Watchdog::new();
    for _ in 0..39 {
        tokio::time::advance(PING).await;
        let payload = w.payload();
        w.sent(Instant::now());
        w.pong(&payload, Instant::now());
        assert!(w.check(Instant::now()).is_ok());
    }
    tokio::time::advance(PING).await;
    w.received(Instant::now());
    assert_eq!(w.check(Instant::now()), Err(TimeoutKind::BarsDataSilence));
}

#[tokio::test(start_paused = true)]
async fn reconnect_does_not_reset_market_age_or_accept_old_pong() {
    let mut w = Watchdog::new();
    tokio::time::advance(PING).await;
    let old = w.payload();
    w.sent(Instant::now());
    w.reconnected();
    assert_eq!(w.snapshot().bars_data_age_ms, 30000);
    let new = w.payload();
    w.sent(Instant::now());
    assert_ne!(old, new);
    w.pong(&old, Instant::now());
    assert!(w.pending.is_some());
    w.pong(&new, Instant::now());
    assert!(w.pending.is_none());
}

#[tokio::test(start_paused = true)]
async fn transport_idle_has_its_own_cause() {
    let w = Watchdog::new();
    tokio::time::advance(IDLE).await;
    assert_eq!(w.check(Instant::now()), Err(TimeoutKind::TransportIdle));
}
