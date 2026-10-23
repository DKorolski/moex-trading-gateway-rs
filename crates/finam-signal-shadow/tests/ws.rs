use broker_finam::AccessToken;
use chrono::{Duration, TimeZone, Utc};
use finam_signal_shadow::ws::*;

const HANDSHAKE: &str = r#"{"type":"EVENT","eventInfo":{"event":"HANDSHAKE_SUCCESS"}}"#;

fn now() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 23, 7, 2, 0).unwrap()
}

fn data() -> String {
    serde_json::json!({"type":"DATA","subscriptionType":"BARS","subscriptionKey":"test",
        "payload":{"symbol":"IMOEXF@RTSX","timeframe":"TIME_FRAME_M1","bars":[{
            "timestamp":"2026-10-23T07:01:00Z","open":"2200","high":"2201",
            "low":"2199","close":"2200.5","volume":"12"}]}})
    .to_string()
}

fn subscribed() -> (BarsProtocol, Connection) {
    let mut p = BarsProtocol::new(3000, 1000).unwrap();
    let c = p.connected(now()).unwrap();
    assert!(matches!(
        p.receive(&c, HANDSHAKE, now()).unwrap(),
        Observation::Handshake
    ));
    let wire = p
        .subscription_request(&c, &AccessToken::new("synthetic-token"))
        .unwrap();
    assert_eq!(wire["data"]["symbol"], "IMOEXF@RTSX");
    assert_eq!(wire["data"]["timeframe"], "TIME_FRAME_M1");
    p.subscription_sent(&c).unwrap();
    (p, c)
}

#[test]
fn data_is_provisional_even_after_minute_close() {
    let (mut p, c) = subscribed();
    let Observation::ProvisionalBars(bars) = p.receive(&c, &data(), now()).unwrap() else {
        panic!("expected observed bars")
    };
    assert_eq!(bars.len(), 1);
    assert!(!bars[0].is_final);
    assert_eq!(bars[0].timeframe_sec, 60);
    assert_eq!(bars[0].instrument.market, broker_core::Market::Futures);
}

#[test]
fn one_subscription_and_one_explicit_reconnect_stale_generation_rejected() {
    let (mut p, old) = subscribed();
    assert!(p.connected(now()).is_err());
    assert!(p
        .subscription_request(&old, &AccessToken::new("test"))
        .is_err());
    p.disconnected(&old, now()).unwrap();
    let new = p.connected(now() + Duration::milliseconds(1)).unwrap();
    assert!(matches!(
        p.receive(&old, &data(), now()),
        Err(WsError::Connection)
    ));
    assert!(matches!(
        p.receive(&new, HANDSHAKE, now() + Duration::milliseconds(1)),
        Ok(Observation::Handshake)
    ));
    p.disconnected(&new, now() + Duration::milliseconds(2))
        .unwrap();
    assert!(matches!(
        p.connected(now() + Duration::milliseconds(3)),
        Err(WsError::Budget)
    ));
}

#[test]
fn heartbeat_does_not_extend_data_silence_and_stop_is_terminal() {
    let (mut p, c) = subscribed();
    p.receive(
        &c,
        r#"{"type":"EVENT","eventInfo":{"event":"PING"}}"#,
        now() + Duration::milliseconds(2999),
    )
    .unwrap();
    assert_eq!(
        p.check_silence(now() + Duration::milliseconds(3000)),
        Err(WsError::Silence)
    );
    assert!(p
        .receive(&c, &data(), now() + Duration::milliseconds(3000))
        .is_err());
    let (mut p, c) = subscribed();
    p.stop();
    assert!(p.receive(&c, &data(), now()).is_err());
    assert!(p.connected(now()).is_err());
}

#[test]
fn wrong_instrument_provider_error_bad_time_and_ohlcv_hold() {
    for text in [
        data().replace("IMOEXF@RTSX", "OTHER"),
        data().replace("07:01:00", "07:03:00"),
        data().replace("07:01:00", "06:00:00"),
        data().replace("\"low\":\"2199\"", "\"low\":\"2202\""),
        r#"{"type":"ERROR","errorInfo":{"message":"do-not-echo-provider-text"}}"#.into(),
    ] {
        let (mut p, c) = subscribed();
        assert!(p.receive(&c, &text, now()).is_err());
        assert!(p.receive(&c, &data(), now()).is_err());
    }
}

#[test]
fn reconnect_budget_and_foreign_connection_are_rejected() {
    let (mut p, c) = subscribed();
    let (_, foreign) = subscribed();
    assert!(matches!(
        p.receive(&foreign, &data(), now()),
        Err(WsError::Connection)
    ));
    p.disconnected(&c, now()).unwrap();
    assert!(matches!(
        p.connected(now() + Duration::milliseconds(1001)),
        Err(WsError::Budget)
    ));
}

fn quote_data() -> String {
    serde_json::json!({"type":"DATA","subscriptionType":"QUOTES","subscriptionKey":"q1",
        "payload":{"quote":[{"symbol":"IMOEXF@RTSX","timestamp":now().to_rfc3339(),
        "last":{"value":"2200.5"}}]}})
    .to_string()
}

fn quote_subscribed() -> (BarsProtocol, Connection) {
    let mut p = BarsProtocol::new_quotes(3000, 1000, 5000, 32).unwrap();
    let c = p.connected(now()).unwrap();
    p.receive(&c, HANDSHAKE, now()).unwrap();
    let wire = p
        .subscription_request(&c, &AccessToken::new("test"))
        .unwrap();
    assert_eq!(wire["type"], "QUOTES");
    assert_eq!(wire["data"]["symbols"], serde_json::json!(["IMOEXF@RTSX"]));
    p.subscription_sent(&c).unwrap();
    (p, c)
}

#[test]
fn quotes_reconnect_retains_dedup_and_rejects_old_generation() {
    let (mut p, old) = quote_subscribed();
    let Observation::Quotes(q) = p.receive(&old, &quote_data(), now()).unwrap() else {
        panic!("quotes")
    };
    assert_eq!(q.len(), 1);
    p.disconnected(&old, now()).unwrap();
    let new = p.connected(now() + Duration::milliseconds(1)).unwrap();
    p.receive(&new, HANDSHAKE, now() + Duration::milliseconds(1))
        .unwrap();
    p.subscription_request(&new, &AccessToken::new("test"))
        .unwrap();
    p.subscription_sent(&new).unwrap();
    assert!(matches!(
        p.receive(&old, &quote_data(), now()),
        Err(WsError::Connection)
    ));
    let Observation::Quotes(q) = p
        .receive(&new, &quote_data(), now() + Duration::milliseconds(2))
        .unwrap()
    else {
        panic!("quotes")
    };
    assert!(q.is_empty());
    assert_eq!(q.raw_frame, quote_data());
    assert_eq!(
        q.received_at_ms,
        (now() + Duration::milliseconds(2)).timestamp_millis()
    );
}

#[test]
fn channel_mixing_and_changed_quote_subscription_key_hold() {
    let (mut bars, bc) = subscribed();
    assert!(bars.receive(&bc, &quote_data(), now()).is_err());
    let (mut quotes, qc) = quote_subscribed();
    assert!(quotes.receive(&qc, &data(), now()).is_err());
    let (mut quotes, qc) = quote_subscribed();
    quotes.receive(&qc, &quote_data(), now()).unwrap();
    assert!(quotes
        .receive(&qc, &quote_data().replace("q1", "q2"), now())
        .is_err());
}

#[test]
fn duplicate_quotes_do_not_refresh_silence() {
    let (mut p, c) = quote_subscribed();
    p.receive(&c, &quote_data(), now()).unwrap();
    p.receive(&c, &quote_data(), now() + Duration::milliseconds(2999))
        .unwrap();
    assert_eq!(
        p.check_silence(now() + Duration::milliseconds(3000)),
        Err(WsError::Silence)
    );
}

#[test]
fn quote_frame_retains_exact_text_receive_time_and_duplicate_evidence() {
    let (mut p, c) = quote_subscribed();
    let raw = format!(" \n{}\t\r\n", quote_data());
    let Observation::Quotes(frame) = p.receive(&c, &raw, now()).unwrap() else {
        panic!("quotes")
    };
    assert_eq!(frame.raw_frame.as_bytes(), raw.as_bytes());
    assert_eq!(frame.received_at_ms, now().timestamp_millis());
    assert_eq!(frame.len(), 1);
    assert_eq!(frame[0].price_ticks, 4401);
    assert_eq!(frame[0].received_at_ms, frame.received_at_ms);
    let quotes: Vec<_> = frame.into_iter().collect();
    assert_eq!(quotes.len(), 1);
    // Equivalent JSON with different whitespace is still retained byte-exact,
    // although the source quote has already been admitted and has no effect.
    for (i, raw) in [quote_data(), raw].into_iter().enumerate() {
        let received = now() + Duration::milliseconds(i as i64 + 1);
        let Observation::Quotes(frame) = p.receive(&c, &raw, received).unwrap() else {
            panic!("duplicate frame must remain observable")
        };
        assert_eq!(frame.raw_frame.as_bytes(), raw.as_bytes());
        assert_eq!(frame.received_at_ms, received.timestamp_millis());
        assert!(frame.is_empty());
        assert_eq!(frame.into_iter().count(), 0);
    }
    assert_eq!(
        p.check_silence(now() + Duration::milliseconds(3000)),
        Err(WsError::Silence)
    );
}

#[test]
fn invalid_later_quote_row_emits_no_partially_normalized_frame() {
    let (mut p, c) = quote_subscribed();
    let mut raw: serde_json::Value = serde_json::from_str(&quote_data()).unwrap();
    let rows = raw["payload"]["quote"].as_array_mut().unwrap();
    let mut bad = rows[0].clone();
    bad["last"]["value"] = "2200.25".into();
    rows.push(bad);
    assert!(matches!(
        p.receive(&c, &raw.to_string(), now()),
        Err(WsError::Frame)
    ));
    assert!(matches!(
        p.receive(&c, &quote_data(), now()),
        Err(WsError::State)
    ));
}

#[test]
fn reconnect_after_eviction_retains_frontier_duplicate_and_conflict_checks() {
    // The fixture's 32-entry window evicts timestamp 0 after 33 fresh quotes.
    // All replay timestamps remain fresh: rejection is not an age-budget effect.
    for (offset, price, accepted) in [
        (32, "2200.5", true),
        (32, "2201", false),
        (0, "2200.5", false),
        (0, "2201", false),
    ] {
        let (mut p, old) = quote_subscribed();
        let data_at = |offset, price: &str| {
            let mut value: serde_json::Value = serde_json::from_str(&quote_data()).unwrap();
            value["payload"]["quote"][0]["timestamp"] =
                (now() + Duration::milliseconds(offset)).to_rfc3339().into();
            value["payload"]["quote"][0]["last"]["value"] = price.into();
            value.to_string()
        };
        for i in 0..33 {
            let Observation::Quotes(quotes) = p
                .receive(
                    &old,
                    &data_at(i, "2200.5"),
                    now() + Duration::milliseconds(i),
                )
                .unwrap()
            else {
                panic!("quotes")
            };
            assert_eq!(quotes.len(), 1);
        }
        let resumed = now() + Duration::milliseconds(34);
        p.disconnected(&old, resumed).unwrap();
        let new = p.connected(resumed).unwrap();
        p.receive(&new, HANDSHAKE, resumed).unwrap();
        p.subscription_request(&new, &AccessToken::new("test"))
            .unwrap();
        p.subscription_sent(&new).unwrap();
        let replay = p.receive(&new, &data_at(offset, price), resumed);
        if accepted {
            let Observation::Quotes(quotes) = replay.unwrap() else {
                panic!("quotes")
            };
            assert!(quotes.is_empty());
            // Retained duplicates cannot refresh post-reconnect silence.
            assert_eq!(
                p.check_silence(resumed + Duration::milliseconds(3000)),
                Err(WsError::Silence)
            );
        } else {
            assert!(matches!(replay, Err(WsError::Frame)));
            assert!(matches!(
                p.receive(&new, &data_at(34, "2200.5"), resumed),
                Err(WsError::State)
            ));
        }
    }
}
