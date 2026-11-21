use chrono::{Duration, TimeZone, Utc};
use finam_signal_shadow::{
    feedback::{Action, Request, VirtualBook},
    quote::*,
};

fn now() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 23, 7, 0, 0).unwrap()
}
fn row(price: &str, offset: i64) -> serde_json::Value {
    serde_json::json!({"symbol":"IMOEXF@RTSX",
        "timestamp":(now()+Duration::milliseconds(offset)).to_rfc3339(),
        "last":{"value":price}})
}
fn frame(rows: Vec<serde_json::Value>) -> String {
    serde_json::json!({"type":"DATA","subscriptionType":"QUOTES",
        "payload":{"quote":rows}})
    .to_string()
}

#[test]
fn exact_tick_and_duplicate_preserve_one_effect() {
    let mut adapter = QuoteInput::new(1000, 8).unwrap();
    let input = frame(vec![row("2200.5", 1)]);
    let quotes = adapter
        .receive(&input, now() + Duration::milliseconds(2))
        .unwrap();
    assert_eq!(quotes.len(), 1);
    assert_eq!(quotes[0].price_ticks, 4401);
    assert_eq!(quotes[0].event_at_ms, now().timestamp_millis() + 1);
    assert!(adapter
        .receive(&input, now() + Duration::milliseconds(3))
        .unwrap()
        .is_empty());
    let mut book = VirtualBook::new("IMOEXF".into(), 1, 1000).unwrap();
    book.submit(Request {
        sequence: 1,
        instrument: "IMOEXF".into(),
        action: Action::EnterLong,
        quantity_lots: 1,
        decision_at_ms: now().timestamp_millis(),
    })
    .unwrap();
    assert!(book
        .observe(quotes[0].clone(), now().timestamp_millis() + 3)
        .unwrap()
        .is_some());
    assert_eq!(book.position_lots(), 1);
}

#[test]
fn missing_time_identity_last_offtick_future_and_stale_refuse() {
    let mut missing_time = row("2200", 0);
    missing_time.as_object_mut().unwrap().remove("timestamp");
    let mut missing_identity = row("2200", 0);
    missing_identity.as_object_mut().unwrap().remove("symbol");
    let mut missing_last = row("2200", 0);
    missing_last.as_object_mut().unwrap().remove("last");
    for bad in [
        missing_time,
        missing_identity,
        missing_last,
        row("2200.25", 0),
        row("-1", 0),
        row("2200", 1),
        row("2200", -1001),
    ] {
        let mut adapter = QuoteInput::new(1000, 8).unwrap();
        assert!(adapter.receive(&frame(vec![bad]), now()).is_err());
        assert_eq!(
            adapter.receive(&frame(vec![row("2200", 0)]), now()),
            Err(QuoteError::Held)
        );
    }
}

#[test]
fn conflicts_and_out_of_order_hold_without_partial_output() {
    let mut adapter = QuoteInput::new(1000, 8).unwrap();
    assert_eq!(
        adapter.receive(&frame(vec![row("2200", 0), row("2201", 0)]), now()),
        Err(QuoteError::Conflict)
    );
    let mut adapter = QuoteInput::new(1000, 8).unwrap();
    assert_eq!(
        adapter.receive(&frame(vec![row("2200", 0), row("2200", -1)]), now()),
        Err(QuoteError::Time)
    );
}

#[test]
fn long_fresh_stream_is_bounded_while_flat_without_pending() {
    for capacity in [1, 8, QUOTE_DEDUP_CAPACITY] {
        let mut adapter = QuoteInput::new(1000, capacity).unwrap();
        let mut book = VirtualBook::new("IMOEXF".into(), 1, 1000).unwrap();
        for offset in 0..5000 {
            let received = now() + Duration::milliseconds(offset);
            let quotes = adapter
                .receive(&frame(vec![row("2200", offset)]), received)
                .unwrap();
            assert_eq!(quotes.len(), 1);
            assert_eq!(adapter.retained_len(), capacity.min(offset as usize + 1));
            // Same no-pending gate as ObservedSession: quotes are normalized
            // continuously, but no fill callback is made while flat.
            for quote in quotes {
                if book.pending().is_some() {
                    book.observe(quote, received.timestamp_millis()).unwrap();
                }
            }
            assert_eq!(book.position_lots(), 0);
            assert!(book.pending().is_none());
        }
    }
}

#[test]
fn eviction_preserves_retained_duplicates_conflicts_and_old_event_hold() {
    for (offset, price, expected) in [
        (1, "2201", QuoteError::Conflict),
        (0, "2200", QuoteError::Time),
        (0, "2201", QuoteError::Time),
    ] {
        let mut adapter = QuoteInput::new(1000, 1).unwrap();
        let received = now() + Duration::milliseconds(2);
        assert_eq!(
            adapter
                .receive(&frame(vec![row("2200", 0), row("2200", 1)]), received)
                .unwrap()
                .len(),
            2
        );
        assert_eq!(adapter.retained_len(), 1);
        assert!(adapter
            .receive(&frame(vec![row("2200", 1)]), received)
            .unwrap()
            .is_empty());
        assert_eq!(
            adapter.receive(&frame(vec![row(price, offset)]), received),
            Err(expected)
        );
        assert_eq!(
            adapter.receive(&frame(vec![row("2200", 2)]), received),
            Err(QuoteError::Held)
        );
    }
}

#[test]
fn frame_keeps_duplicate_and_conflict_checks_across_staged_eviction() {
    let received = now() + Duration::milliseconds(2);
    let mut adapter = QuoteInput::new(1000, 1).unwrap();
    let quotes = adapter
        .receive(
            &frame(vec![row("2200", 0), row("2200", 1), row("2200", 0)]),
            received,
        )
        .unwrap();
    assert_eq!(quotes.len(), 2);
    assert_eq!(adapter.retained_len(), 1);
    let mut adapter = QuoteInput::new(1000, 1).unwrap();
    assert_eq!(
        adapter.receive(
            &frame(vec![row("2200", 0), row("2200", 1), row("2201", 0)]),
            received
        ),
        Err(QuoteError::Conflict)
    );
    assert_eq!(adapter.retained_len(), 0);
}

#[test]
fn pre_intent_quote_cannot_resolve_pending_and_crossed_spread_refuses() {
    let mut adapter = QuoteInput::new(1000, 8).unwrap();
    let q = adapter
        .receive(&frame(vec![row("2200", -1)]), now())
        .unwrap()
        .remove(0);
    let mut book = VirtualBook::new("IMOEXF".into(), 1, 1000).unwrap();
    book.submit(Request {
        sequence: 1,
        instrument: "IMOEXF".into(),
        action: Action::EnterLong,
        quantity_lots: 1,
        decision_at_ms: now().timestamp_millis(),
    })
    .unwrap();
    assert!(book.observe(q, now().timestamp_millis()).unwrap().is_none());
    assert!(book.pending().is_some());
    let mut crossed = row("2200", 0);
    crossed["bid"] = serde_json::json!({"value":"2201"});
    crossed["ask"] = serde_json::json!({"value":"2199"});
    assert_eq!(
        adapter.receive(&frame(vec![crossed]), now()),
        Err(QuoteError::Price)
    );
}
