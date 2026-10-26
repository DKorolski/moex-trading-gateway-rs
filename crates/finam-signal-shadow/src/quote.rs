//! Normalizes dedicated QUOTES observations, not broker fills or trade timestamps.
use crate::{feedback::Quote, seed::digest};
use broker_core::MarketDataEvent;
use broker_finam::FinamWsEnvelope;
use chrono::{DateTime, Utc};
use rust_decimal::{prelude::ToPrimitive, Decimal};
use serde_json::Value;
use std::collections::BTreeMap;

/// Fixed QUOTES dedup budget, independent of the model's M10 bar cache.
pub const QUOTE_DEDUP_CAPACITY: usize = 4096;
/// Config provenance and semantic bindings must include this policy version.
/// Bump it when capacity, eviction, frontier or replay semantics change.
pub const QUOTE_DEDUP_POLICY: &str =
    "moex.shadow.quote-dedup.evict-oldest-4096.monotonic-frontier.old-hold.v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum QuoteError {
    Frame,
    Identity,
    Time,
    Price,
    Conflict,
    Capacity,
    Held,
}

pub struct QuoteInput {
    seen: BTreeMap<DateTime<Utc>, (Decimal, Option<Decimal>, Option<Decimal>)>,
    frontier: Option<DateTime<Utc>>,
    last_receive: Option<DateTime<Utc>>,
    age_ms: i64,
    limit: usize,
    held: bool,
}

impl QuoteInput {
    pub fn new(age_ms: u64, limit: usize) -> Result<Self, QuoteError> {
        if age_ms == 0 || age_ms > 3_600_000 || limit == 0 || limit > QUOTE_DEDUP_CAPACITY {
            return Err(QuoteError::Capacity);
        }
        Ok(Self {
            seen: BTreeMap::new(),
            frontier: None,
            last_receive: None,
            age_ms: age_ms as i64,
            limit,
            held: false,
        })
    }

    /// Retained identities (not the number of quotes admitted over the run).
    pub fn retained_len(&self) -> usize {
        self.seen.len()
    }

    /// Caller must first bind envelope to its dedicated active subscription.
    /// Atomic frame: malformed later rows never partially advance dedup state.
    pub fn receive(
        &mut self,
        text: &str,
        received: DateTime<Utc>,
    ) -> Result<Vec<Quote>, QuoteError> {
        if self.held {
            return Err(QuoteError::Held);
        }
        let result = self.checked(text, received);
        if result.is_err() {
            self.held = true;
        }
        result
    }

    fn checked(&mut self, text: &str, received: DateTime<Utc>) -> Result<Vec<Quote>, QuoteError> {
        if text.len() > 256 * 1024 {
            return Err(QuoteError::Frame);
        }
        if self.last_receive.is_some_and(|t| received < t) {
            return Err(QuoteError::Time);
        }
        let e: FinamWsEnvelope = serde_json::from_str(text).map_err(|_| QuoteError::Frame)?;
        if e.envelope_type != "DATA"
            || e.subscription_type.as_deref() != Some("QUOTES")
            || e.error_info.is_some()
        {
            return Err(QuoteError::Frame);
        }
        let payload = match &e.payload {
            Some(Value::String(s)) => {
                serde_json::from_str::<Value>(s).map_err(|_| QuoteError::Frame)?
            }
            Some(v) => v.clone(),
            None => return Err(QuoteError::Frame),
        };
        if payload.get("error").is_some_and(|v| !v.is_null()) {
            return Err(QuoteError::Frame);
        }
        let rows = payload
            .get("quote")
            .or_else(|| payload.get("quotes"))
            .and_then(Value::as_array)
            .ok_or(QuoteError::Frame)?;
        if rows.is_empty() || rows.len() > 512 {
            return Err(QuoteError::Frame);
        }
        // Missing identity is refused, rather than silently borrowing a symbol.
        if rows
            .iter()
            .any(|r| r.get("symbol").and_then(Value::as_str) != Some("IMOEXF@RTSX"))
        {
            return Err(QuoteError::Identity);
        }
        let mapped = broker_finam::map_ws_market_data_events(&e, None, 60, received)
            .map_err(|_| QuoteError::Frame)?;
        let mut staged = self.seen.clone();
        let mut frontier = self.frontier;
        let mut output = Vec::new();
        for event in mapped {
            let MarketDataEvent::Quote(q) = event else {
                return Err(QuoteError::Frame);
            };
            let at = q.source_ts.ok_or(QuoteError::Time)?;
            if at.timestamp_millis() < 0
                || at > received
                || (received - at).num_milliseconds() > self.age_ms
            {
                return Err(QuoteError::Time);
            }
            let price = q.last.ok_or(QuoteError::Price)?;
            let ticks = price
                .checked_mul(Decimal::from(2))
                .ok_or(QuoteError::Price)?;
            if price <= Decimal::ZERO
                || !ticks.fract().is_zero()
                || q.bid.is_some_and(|v| v <= Decimal::ZERO)
                || q.ask.is_some_and(|v| v <= Decimal::ZERO)
                || matches!((q.bid,q.ask),(Some(b),Some(a)) if b > a)
            {
                return Err(QuoteError::Price);
            }
            let value = (price, q.bid, q.ask);
            if let Some(old) = staged.get(&at) {
                if old != &value {
                    return Err(QuoteError::Conflict);
                }
                continue;
            }
            // Eviction cannot make an old timestamp eligible again. Unknown
            // old events fail closed; their original value is no longer known.
            if frontier.is_some_and(|last| at <= last) {
                return Err(QuoteError::Time);
            }
            let price_ticks = ticks.to_i64().ok_or(QuoteError::Price)?;
            // Identity excludes receive time so a duplicate cannot become a new quote.
            let identity = format!("IMOEXF@RTSX|{}|{}", at.to_rfc3339(), price_ticks);
            output.push(Quote {
                instrument: "IMOEXF".into(),
                source_id: format!("finam-quote-{}", digest(identity.as_bytes())),
                price_ticks,
                event_at_ms: at.timestamp_millis(),
                received_at_ms: received.timestamp_millis(),
            });
            staged.insert(at, value);
            frontier = Some(at);
        }
        // Evict only after validating the whole frame, preserving duplicate /
        // conflict checks even for rows displaced within this frame. Staging
        // is bounded by limit + 512; committed history never exceeds limit.
        while staged.len() > self.limit {
            staged.pop_first();
        }
        self.seen = staged;
        self.frontier = frontier;
        self.last_receive = Some(received);
        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_frame_preserves_history_frontier_and_receive_clock() {
        let at = DateTime::from_timestamp_millis(1_000).unwrap();
        let frame = |rows| {
            serde_json::json!({
                "type": "DATA", "subscriptionType": "QUOTES", "payload": {"quote": rows}
            })
            .to_string()
        };
        let row = |millis, price| {
            serde_json::json!({
                "symbol": "IMOEXF@RTSX",
                "timestamp": DateTime::from_timestamp_millis(millis).unwrap().to_rfc3339(),
                "last": {"value": price}
            })
        };
        for bad in [row(1002, "2200.25"), row(1000, "2201"), row(999, "2200")] {
            let mut input = QuoteInput::new(1000, 1).unwrap();
            input.receive(&frame(vec![row(1000, "2200")]), at).unwrap();
            let before = input.seen.clone();
            assert!(input
                .receive(
                    &frame(vec![row(1001, "2200"), bad]),
                    at + chrono::Duration::milliseconds(2)
                )
                .is_err());
            assert_eq!(input.seen, before);
            assert_eq!(input.frontier, Some(at));
            assert_eq!(input.last_receive, Some(at));
            assert!(input.held);
        }
    }
}
