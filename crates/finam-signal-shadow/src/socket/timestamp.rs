//! Failure-only, typed projection; never admission or raw provider logging.
use chrono::{DateTime, Duration, Utc};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct TimestampBoundsDetail {
    pub row: usize,
    pub timestamp_ms: i64,
    pub subsecond_nanos: u32,
    pub received_at_ms: i64,
    pub received_subsecond_nanos: u32,
    pub snapshot: bool,
    pub silence_ms: u64,
    pub future: bool,
    pub not_minute_aligned: bool,
    pub subsecond: bool,
    pub close_overflow: bool,
    pub stale_non_snapshot: bool,
}

pub(super) fn detail(
    text: &str,
    now: DateTime<Utc>,
    silence_ms: u64,
) -> Option<TimestampBoundsDetail> {
    if text.len() > 256 * 1024 || silence_ms > 3_600_000 {
        return None;
    }
    let e: broker_finam::FinamWsEnvelope = serde_json::from_str(text).ok()?;
    if e.envelope_type != "DATA" || e.subscription_type.as_deref() != Some("BARS") {
        return None;
    }
    let payload = match e.payload? {
        Value::String(s) => serde_json::from_str(&s).ok()?,
        v => v,
    };
    let rows = payload
        .get("bars")
        .or_else(|| payload.get("bar"))?
        .as_array()?;
    if rows.is_empty() || rows.len() > 512 {
        return None;
    }
    for (row, value) in rows.iter().enumerate() {
        let snapshot = match (value.get("is_data_snapshot"), value.get("isDataSnapshot")) {
            (None, None) => false,
            (Some(_), Some(_)) => return None,
            (a, b) => a.or(b)?.as_bool()?,
        };
        let t: DateTime<Utc> = value.get("timestamp")?.as_str()?.parse().ok()?;
        let close = t.checked_add_signed(Duration::minutes(1));
        let d = TimestampBoundsDetail {
            row,
            timestamp_ms: t.timestamp_millis(),
            subsecond_nanos: t.timestamp_subsec_nanos(),
            received_at_ms: now.timestamp_millis(),
            received_subsecond_nanos: now.timestamp_subsec_nanos(),
            snapshot,
            silence_ms,
            future: t > now,
            not_minute_aligned: t.timestamp() % 60 != 0,
            subsecond: t.timestamp_subsec_nanos() != 0,
            close_overflow: close.is_none(),
            stale_non_snapshot: !snapshot
                && close.is_some_and(|c| now - c > Duration::milliseconds(silence_ms as i64)),
        };
        if d.future
            || d.not_minute_aligned
            || d.subsecond
            || d.close_overflow
            || d.stale_non_snapshot
        {
            return Some(d);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn frame(rows: Value) -> String {
        json!({"type":"DATA","subscriptionType":"BARS","subscriptionKey":"SECRET",
            "payload":{"symbol":"IMOEXF@RTSX","bars":rows}})
        .to_string()
    }
    fn row(ts: &str, snapshot: bool) -> Value {
        json!({"timestamp":ts,"isDataSnapshot":snapshot,"open":"2200","high":"2201",
               "low":"2199","close":"2200","volume":"1","ignored":"SECRET"})
    }
    #[test]
    fn exact_predicates_agree_with_unchanged_parser() {
        use crate::ws::{BarsProtocol, FrameCode, WsError};
        let now: DateTime<Utc> = "2026-10-27T09:30:05Z".parse().unwrap();
        for (ts, flag, property) in [
            ("2026-10-27T09:31:00Z", true, "future"),
            ("2026-10-27T09:30:01Z", false, "not_minute_aligned"),
            ("2026-10-27T09:30:00.001Z", false, "subsecond"),
            ("2026-10-27T09:28:00Z", false, "stale_non_snapshot"),
        ] {
            let text = frame(json!([row(ts, flag)]));
            let mut p = BarsProtocol::new(5000, 5000).unwrap();
            let c = p.connected(now).unwrap();
            p.receive(
                &c,
                r#"{"type":"EVENT","eventInfo":{"event":"HANDSHAKE_SUCCESS"}}"#,
                now,
            )
            .unwrap();
            p.subscription_request(&c, &broker_finam::AccessToken::new("SECRET"))
                .unwrap();
            p.subscription_sent(&c).unwrap();
            assert_eq!(
                p.receive(&c, &text, now).unwrap_err(),
                WsError::FrameAt(FrameCode::TimestampBounds)
            );
            let d = detail(&text, now, 5000).unwrap();
            assert_eq!(serde_json::to_value(d).unwrap()[property], true);
            assert_eq!(d.received_at_ms, now.timestamp_millis());
            let encoded = serde_json::to_string(&d).unwrap();
            assert!(!encoded.contains("SECRET"));
            assert!(encoded.len() < 800);
        }
    }
    #[test]
    fn aliases_default_threshold_and_first_offending_row() {
        let now: DateTime<Utc> = "2026-10-27T09:30:05Z".parse().unwrap();
        let valid = row("2026-10-27T09:29:00Z", false);
        assert!(detail(&frame(json!([valid.clone()])), now, 5000).is_none());
        assert!(
            detail(&frame(json!([valid.clone()])), now, 4999)
                .unwrap()
                .stale_non_snapshot
        );
        let mut stale = row("2026-10-27T09:28:00Z", false);
        stale.as_object_mut().unwrap().remove("isDataSnapshot");
        let d = detail(&frame(json!([valid, stale.clone()])), now, 5000).unwrap();
        assert_eq!(d.row, 1);
        assert!(!d.snapshot);
        stale["is_data_snapshot"] = json!(true);
        assert!(detail(&frame(json!([stale.clone()])), now, 5000).is_none());
        stale["isDataSnapshot"] = json!(false);
        assert!(detail(&frame(json!([stale])), now, 5000).is_none());
    }
    #[test]
    fn malformed_and_other_channels_have_no_projection() {
        let now = Utc::now();
        for text in [
            "SECRET",
            &" ".repeat(256 * 1024 + 1),
            r#"{"type":"DATA","subscriptionType":"QUOTES","payload":{}}"#,
        ] {
            assert!(detail(text, now, 60000).is_none());
        }
        assert!(detail(&frame(json!([row("SECRET", false)])), now, 60000).is_none());
        let mut invalid = row("2026-10-27T09:28:00Z", false);
        invalid["isDataSnapshot"] = json!("SECRET");
        assert!(detail(&frame(json!([invalid])), now, 60000).is_none());
        assert!(detail(
            &frame(json!(vec![row("2026-10-27T09:28:00Z", false); 513])),
            now,
            60000
        )
        .is_none());
    }
    #[test]
    fn string_payload_supported_without_copying_provider_text() {
        let now: DateTime<Utc> = "2026-10-27T09:30:05Z".parse().unwrap();
        let mut e: Value =
            serde_json::from_str(&frame(json!([row("2026-10-27T09:31:00Z", false)]))).unwrap();
        e["payload"] = json!(e["payload"].to_string());
        assert!(detail(&e.to_string(), now, 60000).unwrap().future);
    }
}
