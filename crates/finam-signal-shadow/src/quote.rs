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
    "moex.shadow.quote-dedup.last-projection.evict-oldest-4096.monotonic-frontier.old-hold.v2";

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

/// Canonical numeric values only, never raw provider text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct QuotePrices {
    pub last: Decimal,
    pub bid: Option<Decimal>,
    pub ask: Option<Decimal>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct QuoteInstant {
    pub seconds: i64,
    pub nanos: u32,
}
impl From<DateTime<Utc>> for QuoteInstant {
    fn from(t: DateTime<Utc>) -> Self {
        Self {
            seconds: t.timestamp(),
            nanos: t.timestamp_subsec_nanos(),
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct QuoteConflictDetail {
    pub row: usize,
    pub source_time: QuoteInstant,
    pub previous_received_at: QuoteInstant,
    pub received_at: QuoteInstant,
    pub previous_from_same_frame: bool,
    pub previous: QuotePrices,
    pub current: QuotePrices,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SeenQuote {
    prices: QuotePrices,
    received_at: DateTime<Utc>,
}
/// Typed failure evidence only; no raw payload or identity strings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum QuoteTimeReason {
    ReceiveRegression,
    MissingSource,
    NegativeSource,
    FutureSource,
    StaleSource,
    UnknownOldSource,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct QuoteTimeDetail {
    pub reason: QuoteTimeReason,
    pub row: Option<usize>,
    pub source_time: Option<QuoteInstant>,
    pub received_at: QuoteInstant,
    pub previous_received_at: Option<QuoteInstant>,
    pub committed_frontier: Option<QuoteInstant>,
    pub compared_frontier: Option<QuoteInstant>,
    /// Same truncated-millisecond age used by the unchanged admission guard.
    pub age_ms: Option<i64>,
    pub max_age_ms: i64,
}
pub struct QuoteInput {
    initial: Option<InitialAdmission>,
    last_discard: Option<InitialDiscard>,
    auxiliary_total: u64,
    seen: BTreeMap<DateTime<Utc>, SeenQuote>,
    conflict_detail: Option<QuoteConflictDetail>,
    time_detail: Option<QuoteTimeDetail>,
    frontier: Option<DateTime<Utc>>,
    last_receive: Option<DateTime<Utc>>,
    age_ms: i64,
    limit: usize,
    held: bool,
}

pub(crate) const INITIAL_ROWS: usize = 512;
pub(crate) const INITIAL_BYTES: usize = 256 * 1024;
#[derive(Clone, Copy)]
struct InitialAdmission {
    anchor: DateTime<Utc>,
    rows: usize,
    bytes: usize,
}
#[derive(Clone, Debug, serde::Serialize)]
pub struct InitialDiscard {
    pub rows: usize,
    pub frame_bytes: usize,
    pub cumulative_rows: usize,
    pub cumulative_bytes: usize,
    pub snapshot_true: usize,
    pub snapshot_false: usize,
    pub snapshot_unspecified: usize,
}

impl QuoteInput {
    pub fn new(age_ms: u64, limit: usize) -> Result<Self, QuoteError> {
        if age_ms == 0 || age_ms > 3_600_000 || limit == 0 || limit > QUOTE_DEDUP_CAPACITY {
            return Err(QuoteError::Capacity);
        }
        Ok(Self {
            initial: None,
            last_discard: None,
            auxiliary_total: 0,
            seen: BTreeMap::new(),
            conflict_detail: None,
            time_detail: None,
            frontier: None,
            last_receive: None,
            age_ms: age_ms as i64,
            limit,
            held: false,
        })
    }

    pub(crate) fn initial_at_handshake(&mut self, at: DateTime<Utc>) {
        self.initial = Some(InitialAdmission {
            anchor: at,
            rows: 0,
            bytes: 0,
        });
    }
    pub(crate) fn close_initial(&mut self) {
        self.initial = None;
        self.last_discard = None;
    }
    pub(crate) fn initial_discard(&self) -> Option<InitialDiscard> {
        self.last_discard.clone()
    }

    /// Retained identities (not the number of quotes admitted over the run).
    pub fn retained_len(&self) -> usize {
        self.seen.len()
    }
    /// Saturating count of validated rows differing from the ORIGINAL auxiliary
    /// tuple at an already admitted price identity. Not merged BBO state.
    pub(crate) fn auxiliary_total(&self) -> u64 {
        self.auxiliary_total
    }

    pub(crate) fn conflict_detail(&self) -> Option<QuoteConflictDetail> {
        self.conflict_detail
    }

    pub(crate) fn time_detail(&self) -> Option<QuoteTimeDetail> {
        self.time_detail
    }

    fn time_failure(
        &mut self,
        reason: QuoteTimeReason,
        row: Option<usize>,
        source: Option<DateTime<Utc>>,
        received: DateTime<Utc>,
        compared_frontier: Option<DateTime<Utc>>,
    ) -> QuoteError {
        self.time_detail = Some(QuoteTimeDetail {
            reason,
            row,
            source_time: source.map(Into::into),
            received_at: received.into(),
            previous_received_at: self.last_receive.map(Into::into),
            committed_frontier: self.frontier.map(Into::into),
            compared_frontier: compared_frontier.map(Into::into),
            age_ms: source.map(|at| (received - at).num_milliseconds()),
            max_age_ms: self.age_ms,
        });
        QuoteError::Time
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
            return Err(self.time_failure(
                QuoteTimeReason::ReceiveRegression,
                None,
                None,
                received,
                self.frontier,
            ));
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
        let mut auxiliary_total = self.auxiliary_total;
        let mut frontier = self.frontier;
        let mut output = Vec::new();
        let mut initial = self.initial;
        let mut discarded = InitialDiscard {
            rows: 0,
            frame_bytes: text.len(),
            cumulative_rows: 0,
            cumulative_bytes: 0,
            snapshot_true: 0,
            snapshot_false: 0,
            snapshot_unspecified: 0,
        };
        for (row, event) in mapped.into_iter().enumerate() {
            let MarketDataEvent::Quote(q) = event else {
                return Err(QuoteError::Frame);
            };
            let Some(at) = q.source_ts else {
                return Err(self.time_failure(
                    QuoteTimeReason::MissingSource,
                    Some(row),
                    None,
                    received,
                    frontier,
                ));
            };
            let discard = initial.is_some_and(|i| at < i.anchor)
                && (received - at).num_milliseconds() > self.age_ms;
            if at.timestamp_millis() < 0
                || at > received
                || ((received - at).num_milliseconds() > self.age_ms && !discard)
            {
                let reason = if at.timestamp_millis() < 0 {
                    QuoteTimeReason::NegativeSource
                } else if at > received {
                    QuoteTimeReason::FutureSource
                } else {
                    QuoteTimeReason::StaleSource
                };
                return Err(self.time_failure(reason, Some(row), Some(at), received, frontier));
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
            let price_ticks = ticks.to_i64().ok_or(QuoteError::Price)?;
            if discard {
                let i = initial.as_mut().ok_or(QuoteError::Frame)?;
                i.rows += 1;
                if discarded.rows == 0 {
                    i.bytes = i
                        .bytes
                        .checked_add(text.len())
                        .ok_or(QuoteError::Capacity)?;
                }
                if i.rows > INITIAL_ROWS || i.bytes > INITIAL_BYTES {
                    return Err(QuoteError::Capacity);
                }
                let wire = &rows[row];
                let snake = wire.get("is_data_snapshot");
                let camel = wire.get("isDataSnapshot");
                if snake.is_some() && camel.is_some() {
                    return Err(QuoteError::Frame);
                }
                match snake.or(camel) {
                    None => discarded.snapshot_unspecified += 1,
                    Some(Value::Bool(true)) => discarded.snapshot_true += 1,
                    Some(Value::Bool(false)) => discarded.snapshot_false += 1,
                    _ => return Err(QuoteError::Frame),
                }
                discarded.rows += 1;
                discarded.cumulative_rows = i.rows;
                discarded.cumulative_bytes = i.bytes;
                continue;
            }
            // Permanently consume the exception on the first admitted fresh row.
            initial = None;
            let value = QuotePrices {
                last: price,
                bid: q.bid,
                ask: q.ask,
            };
            if let Some(old) = staged.get(&at) {
                if old.prices.last != value.last {
                    self.conflict_detail = Some(QuoteConflictDetail {
                        row,
                        source_time: at.into(),
                        previous_received_at: old.received_at.into(),
                        received_at: received.into(),
                        previous_from_same_frame: !self.seen.contains_key(&at),
                        previous: old.prices,
                        current: value,
                    });
                    return Err(QuoteError::Conflict);
                }
                if old.prices != value {
                    auxiliary_total = auxiliary_total.saturating_add(1);
                }
                continue;
            }
            // Eviction cannot make an old timestamp eligible again. Unknown
            // old events fail closed; their original value is no longer known.
            if frontier.is_some_and(|last| at <= last) {
                return Err(self.time_failure(
                    QuoteTimeReason::UnknownOldSource,
                    Some(row),
                    Some(at),
                    received,
                    frontier,
                ));
            }
            // Identity excludes receive time so a duplicate cannot become a new quote.
            let identity = format!("IMOEXF@RTSX|{}|{}", at.to_rfc3339(), price_ticks);
            output.push(Quote {
                instrument: "IMOEXF".into(),
                source_id: format!("finam-quote-{}", digest(identity.as_bytes())),
                price_ticks,
                event_at_ms: at.timestamp_millis(),
                received_at_ms: received.timestamp_millis(),
            });
            staged.insert(
                at,
                SeenQuote {
                    prices: value,
                    received_at: received,
                },
            );
            frontier = Some(at);
        }
        // Evict only after validating the whole frame, preserving duplicate /
        // conflict checks even for rows displaced within this frame. Staging
        // is bounded by limit + 512; committed history never exceeds limit.
        while staged.len() > self.limit {
            staged.pop_first();
        }
        self.seen = staged;
        self.initial = initial;
        self.last_discard = (discarded.rows > 0).then_some(discarded);
        self.auxiliary_total = auxiliary_total;
        self.frontier = frontier;
        self.last_receive = Some(received);
        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn real_pair_and_aux_variants_emit_last_once_without_refresh() {
        let at = DateTime::from_timestamp(1793164509, 224984788).unwrap();
        let received = DateTime::from_timestamp(1793164509, 255493707).unwrap();
        let mut q = QuoteInput::new(5000, 4096).unwrap();
        let first = q
            .receive(
                &conflict_frame(vec![conflict_row(at, "2535.5", None, None)]),
                received,
            )
            .unwrap();
        assert_eq!(first.len(), 1);
        let before = q.seen.clone();
        let next = DateTime::from_timestamp(1793164509, 294460603).unwrap();
        for row in [
            conflict_row(at, "2535.50", Some("2535.0"), Some("2535.5")),
            conflict_row(at, "2535.5", Some("2534.5"), None),
            conflict_row(at, "2535.5", None, Some("2536")),
        ] {
            assert!(q
                .receive(&conflict_frame(vec![row]), next)
                .unwrap()
                .is_empty());
        }
        assert_eq!(q.auxiliary_total(), 3);
        assert!(q
            .receive(
                &conflict_frame(vec![conflict_row(at, "2535.5", None, None)]),
                next
            )
            .unwrap()
            .is_empty());
        assert_eq!(q.auxiliary_total(), 3);
        assert_eq!(q.seen, before);
        assert_eq!(q.frontier, Some(at));
        assert!(q.conflict_detail().is_none());
        let new = at + chrono::Duration::seconds(1);
        assert_eq!(
            q.receive(
                &conflict_frame(vec![conflict_row(new, "2535.5", None, None)]),
                new
            )
            .unwrap()
            .len(),
            1
        );
    }
    #[test]
    fn aux_validations_and_late_error_do_not_commit_counter_or_state() {
        let at = DateTime::from_timestamp(1793164509, 224984788).unwrap();
        for invalid in [
            conflict_row(at, "2535.5", Some("0"), None),
            conflict_row(at, "2535.5", Some("-1"), None),
            conflict_row(at, "2535.5", Some("2536"), Some("2535")),
            conflict_row(at, "2535.25", None, None),
        ] {
            let mut q = QuoteInput::new(5000, 1).unwrap();
            q.receive(
                &conflict_frame(vec![conflict_row(at, "2535.5", None, None)]),
                at,
            )
            .unwrap();
            let before = q.seen.clone();
            assert_eq!(
                q.receive(
                    &conflict_frame(vec![
                        conflict_row(at, "2535.5", Some("2535"), Some("2536")),
                        invalid
                    ]),
                    at
                ),
                Err(QuoteError::Price)
            );
            assert_eq!(q.auxiliary_total(), 0);
            assert_eq!(q.seen, before);
            assert_eq!(q.frontier, Some(at));
        }
        let mut q = QuoteInput::new(5000, 1).unwrap();
        q.receive(
            &conflict_frame(vec![conflict_row(at, "2535.5", None, None)]),
            at,
        )
        .unwrap();
        assert_eq!(
            q.receive(
                &conflict_frame(vec![conflict_row(at, "2535.5", Some("2535"), None)]),
                at + chrono::Duration::milliseconds(5001)
            ),
            Err(QuoteError::Time)
        );
        assert_eq!(q.auxiliary_total(), 0);
    }
    #[test]
    fn same_frame_aux_is_atomic_and_counter_saturates() {
        let at = DateTime::from_timestamp(1793164509, 224984788).unwrap();
        let mut q = QuoteInput::new(5000, 1).unwrap();
        q.auxiliary_total = u64::MAX;
        assert_eq!(
            q.receive(
                &conflict_frame(vec![
                    conflict_row(at, "2535.5", None, None),
                    conflict_row(at, "2535.5", Some("2535"), None)
                ]),
                at
            )
            .unwrap()
            .len(),
            1
        );
        assert_eq!(q.auxiliary_total(), u64::MAX);
        assert_eq!(q.seen[&at].prices.bid, None);
    }

    fn conflict_row(at: DateTime<Utc>, last: &str, bid: Option<&str>, ask: Option<&str>) -> Value {
        let mut r = serde_json::json!({"symbol":"IMOEXF@RTSX", "timestamp":at.to_rfc3339(),
            "last":{"value":last},"ignored":"SECRET_CANARY"});
        if let Some(b) = bid {
            r["bid"] = serde_json::json!({"value":b});
        }
        if let Some(a) = ask {
            r["ask"] = serde_json::json!({"value":a});
        }
        r
    }
    fn conflict_frame(rows: Vec<Value>) -> String {
        serde_json::json!({"type":"DATA","subscriptionType":"QUOTES",
            "subscriptionKey":"SECRET_CANARY","payload":{"quote":rows}})
        .to_string()
    }
    #[test]
    fn time_reasons_preserve_failure_atomicity_and_precision() {
        use chrono::Duration;
        let at = DateTime::from_timestamp(1793112300, 123456789).unwrap();
        for (source, reason) in [
            (None, QuoteTimeReason::MissingSource),
            (
                DateTime::from_timestamp(-1, 0),
                QuoteTimeReason::NegativeSource,
            ),
            (
                Some(at + Duration::nanoseconds(1)),
                QuoteTimeReason::FutureSource,
            ),
            (
                Some(at - Duration::milliseconds(5001)),
                QuoteTimeReason::StaleSource,
            ),
        ] {
            let mut q = QuoteInput::new(5000, 2).unwrap();
            let mut row = conflict_row(at, "2200", None, None);
            match source {
                Some(t) => row["timestamp"] = serde_json::json!(t.to_rfc3339()),
                None => {
                    row.as_object_mut().unwrap().remove("timestamp");
                }
            }
            assert_eq!(
                q.receive(&conflict_frame(vec![row]), at),
                Err(QuoteError::Time)
            );
            let d = q.time_detail().unwrap();
            assert_eq!(d.reason, reason);
            assert_eq!(d.source_time, source.map(Into::into));
            assert_eq!(d.received_at.nanos, 123456789);
            assert_eq!(d.row, Some(0));
            assert_eq!(d.max_age_ms, 5000);
            assert!(q.seen.is_empty());
            assert!(q.frontier.is_none());
            assert!(q.last_receive.is_none());
            assert!(q.conflict_detail().is_none());
            let text = serde_json::to_string(&d).unwrap();
            assert!(!text.contains("SECRET_CANARY"));
            assert!(text.len() < 1200);
            assert_eq!(q.receive("bad", at), Err(QuoteError::Held));
            assert_eq!(q.time_detail(), Some(d));
        }
    }
    #[test]
    fn time_frontiers_distinguish_committed_staged_and_receive_regression() {
        use chrono::Duration;
        let at = DateTime::from_timestamp(1793112300, 123456789).unwrap();
        let mut q = QuoteInput::new(5000, 1).unwrap();
        q.receive(
            &conflict_frame(vec![conflict_row(at, "2200", None, None)]),
            at,
        )
        .unwrap();
        assert!(q.time_detail().is_none());
        let later = at + Duration::seconds(1);
        let middle = at + Duration::milliseconds(500);
        assert_eq!(
            q.receive(
                &conflict_frame(vec![
                    conflict_row(later, "2200", None, None),
                    conflict_row(middle, "2200", None, None)
                ]),
                later
            ),
            Err(QuoteError::Time)
        );
        let d = q.time_detail().unwrap();
        assert_eq!(d.reason, QuoteTimeReason::UnknownOldSource);
        assert_eq!(d.row, Some(1));
        assert_eq!(d.committed_frontier, Some(at.into()));
        assert_eq!(d.compared_frontier, Some(later.into()));
        assert_eq!(q.frontier, Some(at));
        assert_eq!(q.last_receive, Some(at));
        assert_eq!(q.seen.len(), 1);
        let mut q = QuoteInput::new(5000, 1).unwrap();
        q.receive(
            &conflict_frame(vec![conflict_row(at, "2200", None, None)]),
            at,
        )
        .unwrap();
        assert_eq!(
            q.receive(
                "invalid but receive guard is first",
                at - Duration::nanoseconds(1)
            ),
            Err(QuoteError::Time)
        );
        let d = q.time_detail().unwrap();
        assert_eq!(d.reason, QuoteTimeReason::ReceiveRegression);
        assert!(d.source_time.is_none() && d.row.is_none() && d.age_ms.is_none());
        assert_eq!(d.previous_received_at, Some(at.into()));
        assert_eq!(q.frontier, Some(at));
    }
    #[test]
    fn age_boundary_duplicate_and_unrelated_errors_have_no_time_detail() {
        use chrono::Duration;
        let at = DateTime::from_timestamp(1793112300, 123456789).unwrap();
        let frame = conflict_frame(vec![conflict_row(at, "2200", None, None)]);
        let mut q = QuoteInput::new(5000, 1).unwrap();
        let received = at + Duration::milliseconds(5000) + Duration::nanoseconds(999999);
        assert_eq!(q.receive(&frame, received).unwrap().len(), 1);
        assert!(q.receive(&frame, received).unwrap().is_empty());
        assert!(q.time_detail().is_none());
        assert_eq!(q.receive("bad", received), Err(QuoteError::Frame));
        assert!(q.time_detail().is_none());
    }
    #[test]
    fn conflict_detail_changed_last_is_bounded_atomic() {
        let at = DateTime::from_timestamp(1793112300, 123456789).unwrap();
        let old = conflict_row(at, "2200", Some("2199"), Some("2201"));
        for changed in [
            conflict_row(at, "2200.5", Some("2199"), Some("2201")),
            conflict_row(at, "2199.5", None, Some("2201")),
        ] {
            let mut q = QuoteInput::new(1000, 1).unwrap();
            let received = at + chrono::Duration::milliseconds(1);
            q.receive(&conflict_frame(vec![old.clone()]), received)
                .unwrap();
            let before = q.seen.clone();
            assert_eq!(
                q.receive(
                    &conflict_frame(vec![changed]),
                    received + chrono::Duration::milliseconds(1)
                ),
                Err(QuoteError::Conflict)
            );
            assert_eq!(q.seen, before);
            assert_eq!(q.frontier, Some(at));
            assert_eq!(q.last_receive, Some(received));
            let d = q.conflict_detail().unwrap();
            assert_eq!(d.source_time.nanos, 123456789);
            assert_eq!(d.previous_received_at, received.into());
            assert_eq!(
                d.received_at,
                (received + chrono::Duration::milliseconds(1)).into()
            );
            assert!(!d.previous_from_same_frame);
            let text = serde_json::to_string(&d).unwrap();
            assert!(!text.contains("SECRET_CANARY"));
            assert!(text.len() < 1200);
            assert_eq!(
                q.receive(&conflict_frame(vec![old.clone()]), received),
                Err(QuoteError::Held)
            );
        }
    }
    #[test]
    fn exact_duplicate_keeps_original_receipt_and_no_detail() {
        let at = DateTime::from_timestamp(1793112300, 0).unwrap();
        let row = conflict_row(at, "2200", None, None);
        let mut q = QuoteInput::new(1000, 2).unwrap();
        q.receive(&conflict_frame(vec![row.clone()]), at).unwrap();
        assert!(q
            .receive(
                &conflict_frame(vec![row]),
                at + chrono::Duration::milliseconds(1)
            )
            .unwrap()
            .is_empty());
        assert!(q.conflict_detail().is_none());
        assert_eq!(q.seen[&at].received_at, at);
        assert_eq!(
            q.receive(
                &conflict_frame(vec![conflict_row(at, "2201", None, None)]),
                at + chrono::Duration::milliseconds(2)
            ),
            Err(QuoteError::Conflict)
        );
        assert_eq!(q.conflict_detail().unwrap().previous_received_at, at.into());
    }
    #[test]
    fn within_frame_pair_and_no_partial_commit() {
        let at = DateTime::from_timestamp(1793112300, 0).unwrap();
        let mut q = QuoteInput::new(1000, 1).unwrap();
        let rows = vec![
            conflict_row(at, "2200", None, None),
            conflict_row(at, "2201", None, None),
        ];
        assert_eq!(
            q.receive(&conflict_frame(rows), at),
            Err(QuoteError::Conflict)
        );
        let d = q.conflict_detail().unwrap();
        assert!(d.previous_from_same_frame);
        assert_eq!(d.row, 1);
        assert_eq!(d.previous_received_at, d.received_at);
        assert!(q.seen.is_empty());
        assert!(q.frontier.is_none());
        assert!(q.last_receive.is_none());
    }
    #[test]
    fn invalid_price_before_conflict_has_no_detail() {
        let at = DateTime::from_timestamp(1793112300, 0).unwrap();
        let mut q = QuoteInput::new(1000, 1).unwrap();
        q.receive(
            &conflict_frame(vec![conflict_row(at, "2200", None, None)]),
            at,
        )
        .unwrap();
        assert_eq!(
            q.receive(
                &conflict_frame(vec![conflict_row(at, "2200.25", None, None)]),
                at
            ),
            Err(QuoteError::Price)
        );
        assert!(q.conflict_detail().is_none());
    }

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

#[cfg(test)]
mod initial_tests {
    use super::*;
    use chrono::Duration;
    use serde_json::json;
    fn now() -> DateTime<Utc> {
        DateTime::from_timestamp(1793181600, 687828728).unwrap()
    }
    fn old() -> Value {
        // Normalized observed timestamps; NOT a captured provider wire frame.
        row(DateTime::from_timestamp(1793181591, 848640177).unwrap())
    }
    fn row(t: DateTime<Utc>) -> Value {
        json!({"symbol":"IMOEXF@RTSX","timestamp":t.to_rfc3339(),"last":{"value":"2535.5"}})
    }
    fn frame(rows: Vec<Value>) -> String {
        json!({"type":"DATA","subscriptionType":"QUOTES","payload":{"quotes":rows}}).to_string()
    }
    fn input() -> QuoteInput {
        let mut q = QuoteInput::new(5000, 4096).unwrap();
        q.initial_at_handshake(DateTime::from_timestamp(1793181600, 667000000).unwrap());
        q
    }
    #[test]
    fn initial_actual_times_discard_then_fresh_and_strict_default() {
        let mut strict = QuoteInput::new(5000, 4096).unwrap();
        assert_eq!(
            strict.receive(&frame(vec![old()]), now()).unwrap_err(),
            QuoteError::Time
        );
        let mut q = input();
        assert!(q.receive(&frame(vec![old()]), now()).unwrap().is_empty());
        assert!(q.frontier.is_none() && q.seen.is_empty());
        assert_eq!(q.auxiliary_total, 0);
        assert_eq!(q.initial_discard().unwrap().rows, 1);
        assert_eq!(q.receive(&frame(vec![row(now())]), now()).unwrap().len(), 1);
        assert!(q.initial.is_none() && q.initial_discard().is_none());
        assert_eq!(
            q.receive(&frame(vec![old()]), now()).unwrap_err(),
            QuoteError::Time
        );
    }
    #[test]
    fn initial_mixed_prefix_and_rollback() {
        let mut q = input();
        assert_eq!(
            q.receive(&frame(vec![old(), row(now())]), now())
                .unwrap()
                .len(),
            1
        );
        assert_eq!(q.initial_discard().unwrap().rows, 1);
        for rows in [vec![row(now()), old()], vec![old(), row(now()), old()]] {
            let mut q = input();
            assert!(q.receive(&frame(rows), now()).is_err());
            assert!(q.seen.is_empty() && q.frontier.is_none() && q.last_discard.is_none());
            assert_eq!(q.initial.unwrap().rows, 0);
        }
    }
    #[test]
    fn initial_invalid_fields_and_late_row_never_hidden() {
        let mut cases = Vec::new();
        for (key, value) in [
            ("symbol", json!("OTHER")),
            ("timestamp", json!("invalid")),
            ("timestamp", json!("1960-01-01T00:00:00Z")),
            (
                "timestamp",
                json!((now() + Duration::seconds(1)).to_rfc3339()),
            ),
            ("last", json!({"value":"0"})),
            ("last", json!({"value":"2535.1"})),
            ("bid", json!({"value":"-1"})),
            ("isDataSnapshot", json!("true")),
        ] {
            let mut r = old();
            r[key] = value;
            cases.push(r);
        }
        let mut missing = old();
        missing.as_object_mut().unwrap().remove("timestamp");
        cases.push(missing);
        let mut crossed = old();
        crossed["bid"] = json!({"value":"2540"});
        crossed["ask"] = json!({"value":"2530"});
        cases.push(crossed);
        for bad in cases {
            let mut q = input();
            assert!(q.receive(&frame(vec![old(), bad]), now()).is_err());
            assert!(q.seen.is_empty() && q.last_discard.is_none());
            assert_eq!(q.initial.unwrap().rows, 0);
        }
    }
    #[test]
    fn initial_limits_are_cumulative_and_frame_bytes_count_once() {
        let mut q = input();
        let text = frame(vec![old(); 512]);
        assert!(q.receive(&text, now()).unwrap().is_empty());
        assert_eq!(q.initial_discard().unwrap().cumulative_bytes, text.len());
        assert_eq!(
            q.receive(&frame(vec![old()]), now()).unwrap_err(),
            QuoteError::Capacity
        );
        assert_eq!(q.initial.unwrap().rows, 512);
        let mut q = input();
        let mut padded = old();
        padded["padding"] = json!("x".repeat(140000));
        let text = frame(vec![padded]);
        q.receive(&text, now()).unwrap();
        assert_eq!(q.receive(&text, now()).unwrap_err(), QuoteError::Capacity);
        assert_eq!(q.initial.unwrap().bytes, text.len());
    }
    #[test]
    fn initial_flags_are_observed_not_authority_and_reconnect_is_strict() {
        let mut a = old();
        a["isDataSnapshot"] = json!(true);
        let mut b = old();
        b["is_data_snapshot"] = json!(false);
        let mut q = input();
        q.receive(&frame(vec![a, b, old()]), now()).unwrap();
        let d = q.initial_discard().unwrap();
        assert_eq!(
            (d.snapshot_true, d.snapshot_false, d.snapshot_unspecified),
            (1, 1, 1)
        );
        q.close_initial();
        assert!(q.receive(&frame(vec![old()]), now()).is_err());
        let mut q = input();
        assert!(q
            .receive(&frame(vec![row(now())]), now() + Duration::seconds(6))
            .is_err());
    }
}
