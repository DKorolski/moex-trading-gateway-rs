//! Single-subscription protocol gate. Socket ownership remains with the driver.
//! M1 frames are observations only; closed REST admission owns model finality.
use broker_core::{event::Bar, MarketDataEvent};
use broker_finam::{AccessToken, FinamWsEnvelope};
use chrono::{DateTime, Duration, Utc};
use serde_json::Value;
use std::sync::Arc;

const SYMBOL: &str = "IMOEXF@RTSX";
const TIMEFRAME: &str = "TIME_FRAME_M1";

#[derive(Clone)]
pub struct Connection {
    owner: Arc<()>,
    generation: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum WsError {
    State,
    Connection,
    Clock,
    Silence,
    Frame,
    FrameAt(FrameCode),
    Quote(crate::quote::QuoteError),
    Provider,
    Budget,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum FrameCode {
    Size,
    EnvelopeJson,
    Binding,
    SubscriptionKey,
    PayloadJson,
    MissingPayload,
    PayloadIdentity,
    Rows,
    RowCount,
    TimestampMissing,
    TimestampParse,
    TimestampBounds,
    Mapper,
    EventKind,
    Ohlcv,
    SnapshotFlag,
    UnexpectedSnapshot,
}

#[derive(Debug)]
pub struct QuoteFrame {
    pub initial_discard: Option<crate::quote::InitialDiscard>,
    /// Committed cumulative auxiliary rows, saturating; not fresh price DATA.
    pub auxiliary_total: u64,
    /// Exact admitted text, not a reserialization. Record before quote effects,
    /// including duplicate-only frames whose normalized quotes are empty.
    pub raw_frame: String,
    pub received_at_ms: i64,
    pub quotes: Vec<crate::feedback::Quote>,
}

impl std::ops::Deref for QuoteFrame {
    type Target = Vec<crate::feedback::Quote>;

    fn deref(&self) -> &Self::Target {
        &self.quotes
    }
}

impl IntoIterator for QuoteFrame {
    type Item = crate::feedback::Quote;
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.quotes.into_iter()
    }
}

#[derive(Debug)]
pub enum Observation {
    Handshake,
    Ignore,
    ProvisionalBars(Vec<Bar>),
    InitialHistory {
        live: Vec<Bar>,
        history: SnapshotReceipt,
    },
    Quotes(QuoteFrame),
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SnapshotReceipt {
    pub rows: usize,
    pub min_timestamp_ms: i64,
    pub max_timestamp_ms: i64,
}

impl Observation {
    pub fn live_bars(&self) -> &[Bar] {
        match self {
            Self::ProvisionalBars(bars) | Self::InitialHistory { live: bars, .. } => bars,
            _ => &[],
        }
    }
}

// FINAM Bar's proto3 bool: omitted means false; both ProtoJSON field spellings
// are supported. Null, coercions and simultaneous aliases are rejected.
#[derive(serde::Deserialize)]
struct SnapshotFlag {
    #[serde(default, alias = "isDataSnapshot")]
    is_data_snapshot: bool,
}

#[derive(PartialEq)]
enum State {
    Idle,
    Handshake,
    Due,
    Sending,
    Data,
    Disconnected,
    Held,
    Stopped,
}

pub struct BarsProtocol {
    candle_liveness: bool,
    diagnostic_initial: bool,
    quotes: Option<crate::quote::QuoteInput>,
    owner: Arc<()>,
    generation: u64,
    state: State,
    controlled_connections: u64,
    auth_connections: u64,
    last_clock: Option<DateTime<Utc>>,
    last_data: Option<DateTime<Utc>>,
    connected_at: Option<DateTime<Utc>>,
    disconnected_at: Option<DateTime<Utc>>,
    key: Option<String>,
    silence: Duration,
    reconnect: Duration,
    snapshot_rows: usize,
    snapshot_bytes: usize,
    live_seen: bool,
}

impl BarsProtocol {
    pub(crate) fn reconnect_budget_ms(&self) -> u64 {
        self.reconnect.num_milliseconds() as u64
    }
    pub fn new(silence_ms: u64, reconnect_ms: u64) -> Result<Self, WsError> {
        if silence_ms == 0
            || reconnect_ms == 0
            || silence_ms > 3_600_000
            || reconnect_ms > 3_600_000
        {
            return Err(WsError::Budget);
        }
        Ok(Self {
            candle_liveness: false,
            diagnostic_initial: false,
            quotes: None,
            owner: Arc::new(()),
            generation: 0,
            state: State::Idle,
            controlled_connections: 0,
            auth_connections: 0,
            last_clock: None,
            last_data: None,
            connected_at: None,
            disconnected_at: None,
            key: None,
            silence: Duration::milliseconds(silence_ms as i64),
            reconnect: Duration::milliseconds(reconnect_ms as i64),
            snapshot_rows: 0,
            snapshot_bytes: 0,
            live_seen: false,
        })
    }

    /// Dedicated QUOTES socket; channel cannot be switched after construction.
    /// Quote dedup is retained across the one permitted reconnect.
    pub fn new_quotes(
        silence_ms: u64,
        reconnect_ms: u64,
        quote_age_ms: u64,
        capacity: usize,
    ) -> Result<Self, WsError> {
        let mut protocol = Self::new(silence_ms, reconnect_ms)?;
        protocol.quotes = Some(
            crate::quote::QuoteInput::new(quote_age_ms, capacity).map_err(|_| WsError::Budget)?,
        );
        Ok(protocol)
    }

    pub(crate) fn diagnostic_initial_quotes(mut self) -> Self {
        self.diagnostic_initial = true;
        self
    }

    /// Reserve a connection attempt; old socket must already be closed.
    /// Handshake completion is still required before subscription.
    /// At most one controlled reconnect (generation 2) in this qualification slice.
    pub fn connected(&mut self, now: DateTime<Utc>) -> Result<Connection, WsError> {
        self.connect_kind(now, false)
    }

    pub(crate) fn connected_for_auth(&mut self, now: DateTime<Utc>) -> Result<Connection, WsError> {
        self.connect_kind(now, true)
    }

    fn connect_kind(&mut self, now: DateTime<Utc>, auth: bool) -> Result<Connection, WsError> {
        if !matches!(self.state, State::Idle | State::Disconnected) {
            return Err(WsError::State);
        }
        self.clock(now)?;
        if (!auth && self.controlled_connections >= 2)
            || (auth && (self.auth_connections >= 127 || self.generation == 0))
            || self
                .disconnected_at
                .is_some_and(|t| now - t > self.reconnect)
        {
            self.state = State::Held;
            return Err(WsError::Budget);
        }
        self.generation += 1;
        if self.generation > 1 {
            if let Some(quotes) = &mut self.quotes {
                quotes.close_initial();
            }
        }
        if auth {
            self.auth_connections += 1;
        } else {
            self.controlled_connections += 1;
        }
        self.state = State::Handshake;
        // Rotation cannot reset the silence clock or discard quote dedup.
        if !auth {
            self.connected_at = Some(now);
            self.last_data = None;
        }
        self.key = None;
        self.snapshot_rows = 0;
        self.snapshot_bytes = 0;
        self.live_seen = false;
        Ok(Connection {
            owner: self.owner.clone(),
            generation: self.generation,
        })
    }

    pub fn subscription_request(
        &mut self,
        c: &Connection,
        token: &AccessToken,
    ) -> Result<Value, WsError> {
        self.require(c)?;
        if self.state != State::Due || token.is_empty() {
            return Err(WsError::State);
        }
        self.state = State::Sending;
        // Contains a secret: caller must send directly, never log/serialize evidence.
        Ok(if self.quotes.is_some() {
            broker_finam::build_ws_subscribe_quotes_request(&[SYMBOL.into()], token)
        } else {
            broker_finam::build_ws_subscribe_bars_request(SYMBOL, TIMEFRAME, token)
        })
    }

    pub fn subscription_sent(&mut self, c: &Connection) -> Result<(), WsError> {
        self.require(c)?;
        if self.state != State::Sending {
            return Err(WsError::State);
        }
        self.state = State::Data;
        Ok(())
    }

    pub(crate) fn quote_conflict_detail(&self) -> Option<crate::quote::QuoteConflictDetail> {
        self.quotes.as_ref().and_then(|q| q.conflict_detail())
    }

    pub(crate) fn quote_time_detail(&self) -> Option<crate::quote::QuoteTimeDetail> {
        self.quotes.as_ref().and_then(|q| q.time_detail())
    }

    pub fn receive(
        &mut self,
        c: &Connection,
        text: &str,
        now: DateTime<Utc>,
    ) -> Result<Observation, WsError> {
        self.require(c)?; // stale tasks cannot mutate the current generation
        if matches!(
            self.state,
            State::Held | State::Stopped | State::Disconnected
        ) {
            return Err(WsError::State);
        }
        let result = self.decode(text, now);
        if result.is_err() {
            self.state = State::Held;
        }
        result
    }

    fn decode(&mut self, text: &str, now: DateTime<Utc>) -> Result<Observation, WsError> {
        self.check_silence(now)?;
        if text.len() > 256 * 1024 {
            return Err(WsError::FrameAt(FrameCode::Size));
        }
        let e: FinamWsEnvelope =
            serde_json::from_str(text).map_err(|_| WsError::FrameAt(FrameCode::EnvelopeJson))?;
        if e.envelope_type == "ERROR" || e.error_info.is_some() {
            return Err(WsError::Provider);
        }
        if e.envelope_type == "EVENT" {
            return match e
                .event_info
                .as_ref()
                .and_then(|v| v.get("event"))
                .and_then(Value::as_str)
            {
                Some("HANDSHAKE_SUCCESS") if self.state == State::Handshake => {
                    if self.diagnostic_initial && self.generation == 1 {
                        if let Some(quotes) = &mut self.quotes {
                            quotes.initial_at_handshake(now);
                        }
                    }
                    self.state = State::Due;
                    Ok(Observation::Handshake)
                }
                Some("CONNECTION_CLOSED") => Err(WsError::Provider),
                Some("HANDSHAKE_SUCCESS") => Err(WsError::State),
                _ => Ok(Observation::Ignore),
            };
        }
        if self.state != State::Data
            || e.envelope_type != "DATA"
            || e.subscription_type.as_deref()
                != Some(if self.quotes.is_some() {
                    "QUOTES"
                } else {
                    "BARS"
                })
        {
            return Err(WsError::FrameAt(FrameCode::Binding));
        }
        if let Some(key) = &e.subscription_key {
            if key.is_empty() || key.len() > 256 || self.key.as_ref().is_some_and(|k| k != key) {
                return Err(WsError::FrameAt(FrameCode::SubscriptionKey));
            }
        }
        if let Some(quotes) = &mut self.quotes {
            let observed = quotes.receive(text, now).map_err(WsError::Quote)?;
            self.key = e.subscription_key.or_else(|| self.key.clone());
            // Replayed quotes cannot indefinitely refresh the data-silence budget.
            if !observed.is_empty() {
                self.last_data = Some(now);
            }
            return Ok(Observation::Quotes(QuoteFrame {
                initial_discard: quotes.initial_discard(),
                auxiliary_total: quotes.auxiliary_total(),
                raw_frame: text.to_owned(),
                received_at_ms: now.timestamp_millis(),
                quotes: observed,
            }));
        }
        let payload = match &e.payload {
            Some(Value::String(s)) => serde_json::from_str::<Value>(s)
                .map_err(|_| WsError::FrameAt(FrameCode::PayloadJson))?,
            Some(v) => v.clone(),
            None => return Err(WsError::FrameAt(FrameCode::MissingPayload)),
        };
        if payload.get("error").is_some_and(|v| !v.is_null())
            || payload
                .get("symbol")
                .is_some_and(|v| v.as_str() != Some(SYMBOL))
            || payload
                .get("timeframe")
                .is_some_and(|v| v.as_str() != Some(TIMEFRAME))
        {
            return Err(WsError::FrameAt(FrameCode::PayloadIdentity));
        }
        let rows = payload
            .get("bars")
            .or_else(|| payload.get("bar"))
            .and_then(Value::as_array)
            .ok_or(WsError::FrameAt(FrameCode::Rows))?;
        if rows.is_empty() || rows.len() > 512 {
            return Err(WsError::FrameAt(FrameCode::RowCount));
        }
        let mut flags = Vec::with_capacity(rows.len());
        let mut history: Option<SnapshotReceipt> = None;
        for row in rows {
            let flag: SnapshotFlag = serde_json::from_value(row.clone())
                .map_err(|_| WsError::FrameAt(FrameCode::SnapshotFlag))?;
            let snapshot = flag.is_data_snapshot;
            if snapshot && self.live_seen {
                return Err(WsError::FrameAt(FrameCode::UnexpectedSnapshot));
            }
            flags.push(snapshot);
            let t: DateTime<Utc> = row
                .get("timestamp")
                .and_then(Value::as_str)
                .ok_or(WsError::FrameAt(FrameCode::TimestampMissing))?
                .parse()
                .map_err(|_| WsError::FrameAt(FrameCode::TimestampParse))?;
            if t > now
                || t.timestamp() % 60 != 0
                || t.timestamp_subsec_nanos() != 0
                || t.checked_add_signed(Duration::minutes(1)).is_none()
                || (!snapshot
                    && t.checked_add_signed(Duration::minutes(1))
                        .is_some_and(|close| now - close > self.silence))
            {
                return Err(WsError::FrameAt(FrameCode::TimestampBounds));
            }
            if snapshot {
                let h = history.get_or_insert(SnapshotReceipt {
                    rows: 0,
                    min_timestamp_ms: t.timestamp_millis(),
                    max_timestamp_ms: t.timestamp_millis(),
                });
                h.rows += 1;
                h.min_timestamp_ms = h.min_timestamp_ms.min(t.timestamp_millis());
                h.max_timestamp_ms = h.max_timestamp_ms.max(t.timestamp_millis());
            }
        }
        // Conservatively charge the entire wire frame (including mixed live rows
        // and envelope) to the bootstrap byte budget; never undercount payload.
        if history.as_ref().is_some_and(|h| {
            self.snapshot_rows + h.rows > 512 || self.snapshot_bytes + text.len() > 256 * 1024
        }) {
            return Err(WsError::Budget);
        }
        let events = broker_finam::map_ws_market_data_events(&e, Some(SYMBOL), 60, now)
            .map_err(|_| WsError::FrameAt(FrameCode::Mapper))?;
        if events.len() != flags.len() {
            return Err(WsError::FrameAt(FrameCode::RowCount));
        }
        let mut bars = Vec::new();
        for (event, snapshot) in events.into_iter().zip(flags) {
            let MarketDataEvent::Bar(mut bar) = event else {
                return Err(WsError::FrameAt(FrameCode::EventKind));
            };
            if bar.instrument.venue_symbol.as_deref() != Some(SYMBOL)
                || bar.low > bar.open
                || bar.low > bar.close
                || bar.high < bar.open
                || bar.high < bar.close
                || bar.low <= 0.into()
                || bar.volume < 0.into()
            {
                return Err(WsError::FrameAt(FrameCode::Ohlcv));
            }
            bar.instrument.market = broker_core::Market::Futures;
            bar.is_final = false; // clock-based mapper finality is NOT source proof
            if !snapshot {
                bars.push(bar);
            }
        }
        self.key = e.subscription_key.or_else(|| self.key.clone());
        if !bars.is_empty() {
            self.last_data = Some(now);
            self.live_seen = true;
        }
        if let Some(history) = history {
            self.snapshot_rows += history.rows;
            self.snapshot_bytes += text.len();
            Ok(Observation::InitialHistory {
                live: bars,
                history,
            })
        } else {
            Ok(Observation::ProvisionalBars(bars))
        }
    }

    pub fn check_silence(&mut self, now: DateTime<Utc>) -> Result<(), WsError> {
        if matches!(
            self.state,
            State::Idle | State::Disconnected | State::Held | State::Stopped
        ) {
            return Err(WsError::State);
        }
        self.clock(now)?;
        if !self.candle_liveness
            && self
                .last_data
                .or(self.connected_at)
                .is_some_and(|t| now - t >= self.silence)
        {
            self.state = State::Held;
            return Err(WsError::Silence);
        }
        Ok(())
    }

    pub(crate) fn enable_candle_liveness(&mut self) {
        // QUOTES always retains its accepted data freshness/silence checks.
        self.candle_liveness = self.quotes.is_none();
    }

    pub(crate) fn remaining_silence(
        &mut self,
        now: DateTime<Utc>,
    ) -> Result<std::time::Duration, WsError> {
        self.check_silence(now)?;
        let anchor = self.last_data.or(self.connected_at).ok_or(WsError::State)?;
        (self.silence - (now - anchor))
            .to_std()
            .map_err(|_| WsError::Silence)
    }

    /// Driver confirms closure of the physical socket. Faults do not auto-reconnect.
    pub fn disconnected(&mut self, c: &Connection, now: DateTime<Utc>) -> Result<(), WsError> {
        self.require(c)?;
        if matches!(
            self.state,
            State::Held | State::Stopped | State::Disconnected
        ) {
            return Err(WsError::State);
        }
        self.clock(now)?;
        self.state = State::Disconnected;
        self.disconnected_at = Some(now);
        Ok(())
    }

    pub fn stop(&mut self) {
        self.state = State::Stopped;
    }

    fn require(&self, c: &Connection) -> Result<(), WsError> {
        if !Arc::ptr_eq(&self.owner, &c.owner) || c.generation != self.generation {
            return Err(WsError::Connection);
        }
        Ok(())
    }

    fn clock(&mut self, now: DateTime<Utc>) -> Result<(), WsError> {
        if self.last_clock.is_some_and(|last| now < last) {
            self.state = State::Held;
            return Err(WsError::Clock);
        }
        self.last_clock = Some(now);
        Ok(())
    }
}

#[cfg(test)]
pub(crate) mod snapshot_tests {
    use super::*;
    use serde_json::json;
    pub(crate) fn last_projection_observations() -> Vec<Observation> {
        let now = now();
        let mut p = BarsProtocol::new_quotes(5000, 1000, 5000, 1).unwrap();
        let mut c = p.connected(now).unwrap();
        let mut out = Vec::new();
        let row = |t: DateTime<Utc>, aux: bool| {
            let mut r = json!({"symbol":"IMOEXF@RTSX","timestamp":t.to_rfc3339(),"last":{"value":"2535.5"}});
            if aux {
                r["bid"] = json!({"value":"2535.0"});
                r["ask"] = json!({"value":"2535.5"});
            }
            r
        };
        for (ms, rows) in [
            (1, vec![row(now, false)]),
            (2, vec![row(now, true)]),
            (5, vec![row(now, true)]),
            (
                6,
                vec![row(now, true), row(now + Duration::milliseconds(6), true)],
            ),
        ] {
            if ms == 1 || ms == 5 {
                if ms == 5 {
                    p.disconnected(&c, now + Duration::milliseconds(3)).unwrap();
                    c = p
                        .connected_for_auth(now + Duration::milliseconds(4))
                        .unwrap();
                }
                subscribe(&mut p, &c, now + Duration::milliseconds(ms - 1));
                out.push(Observation::Handshake);
            }
            let raw = json!({"type":"DATA","subscriptionType":"QUOTES","payload":{"quote":rows}})
                .to_string();
            out.push(
                p.receive(&c, &raw, now + Duration::milliseconds(ms))
                    .unwrap(),
            );
        }
        out
    }
    fn now() -> DateTime<Utc> {
        "2026-10-26T15:40:00Z".parse().unwrap()
    }
    fn subscribe(p: &mut BarsProtocol, c: &Connection, at: DateTime<Utc>) {
        p.receive(
            c,
            r#"{"type":"EVENT","eventInfo":{"event":"HANDSHAKE_SUCCESS"}}"#,
            at,
        )
        .unwrap();
        p.subscription_request(c, &AccessToken::new("SECRET_TOKEN"))
            .unwrap();
        p.subscription_sent(c).unwrap();
    }
    fn setup() -> (BarsProtocol, Connection) {
        let mut p = BarsProtocol::new(60_000, 5_000).unwrap();
        let c = p.connected(now()).unwrap();
        subscribe(&mut p, &c, now());
        (p, c)
    }
    fn row(snapshot: bool, at: DateTime<Utc>) -> Value {
        json!({"timestamp":at.to_rfc3339(),"open":"2200","high":"2201",
            "low":"2199","close":"2200.5","volume":"12","is_data_snapshot":snapshot})
    }
    fn frame(rows: Vec<Value>) -> String {
        json!({"type":"DATA","subscriptionType":"BARS","subscriptionKey":"SECRET_KEY",
            "payload":{"symbol":SYMBOL,"timeframe":TIMEFRAME,"bars":rows}})
        .to_string()
    }
    fn old() -> Value {
        row(true, now() - Duration::hours(5))
    }
    fn live() -> Value {
        row(false, now())
    }

    #[test]
    fn synthetic_300_history_then_live_and_late_snapshot() {
        let (mut p, c) = setup();
        let rows = (0..300)
            .map(|m| row(true, now() - Duration::minutes(300 - m)))
            .collect();
        let history = p
            .receive(&c, &frame(rows), now() + Duration::seconds(1))
            .unwrap();
        assert!(history.live_bars().is_empty());
        assert!(p.last_data.is_none());
        assert_eq!(p.snapshot_rows, 300);
        let mut witness = crate::s5::Witness::new_auth(crate::s5::Channel::Bars, 1, 1);
        witness.receipts(&Observation::Handshake, now().timestamp_millis());
        let receipts = witness.receipts(&history, now().timestamp_millis() + 1000);
        let encoded = serde_json::to_string(&receipts).unwrap();
        assert!(encoded.contains("snapshot_discarded"));
        assert!(!encoded.contains("first_validated_data"));
        assert!(!encoded.contains("SECRET"));
        let data = p
            .receive(&c, &frame(vec![live()]), now() + Duration::seconds(2))
            .unwrap();
        assert_eq!(data.live_bars().len(), 1);
        assert!(!data.live_bars()[0].is_final);
        assert!(
            serde_json::to_string(&witness.receipts(&data, now().timestamp_millis() + 2000))
                .unwrap()
                .contains("first_validated_data")
        );
        assert!(matches!(
            p.receive(&c, &frame(vec![old()]), now() + Duration::seconds(3)),
            Err(WsError::FrameAt(FrameCode::UnexpectedSnapshot))
        ));
    }

    #[test]
    fn mixed_is_atomic_and_only_live_survives_mapping() {
        let (mut p, c) = setup();
        let observed = p
            .receive(&c, &frame(vec![old(), live(), row(true, now())]), now())
            .unwrap();
        assert_eq!(observed.live_bars().len(), 1);
        assert_eq!(p.snapshot_rows, 2);
        assert!(p.live_seen);
        let mut witness = crate::s5::Witness::new_auth(crate::s5::Channel::Bars, 2, 2);
        witness.receipts(&Observation::Handshake, 0);
        let receipts = witness.receipts(&observed, 1);
        assert_eq!(receipts.len(), 2);
        let value = serde_json::to_value(receipts).unwrap();
        assert_eq!(value[0]["event"], "snapshot_discarded");
        assert_eq!(value[1]["event"], "first_validated_data");
        assert_eq!(value[1]["data_items"], 1);
        assert_eq!(value[0]["auth_generation"], 2);
        for (field, value) in [
            ("timestamp", json!("2026-10-26T15:41:00Z")),
            ("timestamp", json!("2026-10-26T10:40:00.001Z")),
            ("timestamp", json!("2026-10-26T10:40:01Z")),
            ("timestamp", json!("invalid")),
            ("timestamp", json!("+262142-12-31T23:59:00Z")),
            ("open", json!("SECRET_BAD_DECIMAL")),
            ("low", json!("2300")),
            ("volume", json!("-1")),
        ] {
            let (mut p, c) = setup();
            let mut bad = old();
            bad[field] = value;
            assert!(p.receive(&c, &frame(vec![live(), bad]), now()).is_err());
            assert!(p.last_data.is_none());
            assert_eq!(p.snapshot_rows, 0);
            assert!(!p.live_seen);
            assert!(p.receive(&c, &frame(vec![live()]), now()).is_err());
        }
    }

    #[test]
    fn history_identity_and_key_checks_are_not_exempt() {
        for corrupt in ["symbol", "timeframe", "channel", "key"] {
            let (mut p, c) = setup();
            p.receive(&c, &frame(vec![old()]), now()).unwrap();
            let mut bad: Value = serde_json::from_str(&frame(vec![old()])).unwrap();
            match corrupt {
                "symbol" => bad["payload"]["symbol"] = json!("SECRET_OTHER"),
                "timeframe" => bad["payload"]["timeframe"] = json!("TIME_FRAME_M10"),
                "channel" => bad["subscriptionType"] = json!("QUOTES"),
                _ => bad["subscriptionKey"] = json!("CHANGED_KEY"),
            }
            assert!(p.receive(&c, &bad.to_string(), now()).is_err());
            assert_eq!(p.snapshot_rows, 1);
            assert!(p.last_data.is_none());
        }
    }

    #[test]
    fn flag_contract_is_strict_and_stale_live_never_falls_back() {
        for flag in [Value::Null, json!("true"), json!(1), json!({})] {
            let (mut p, c) = setup();
            let mut bad = old();
            bad["is_data_snapshot"] = flag;
            assert!(matches!(
                p.receive(&c, &frame(vec![bad]), now()),
                Err(WsError::FrameAt(FrameCode::SnapshotFlag))
            ));
        }
        let (mut p, c) = setup();
        let mut conflict = old();
        conflict["isDataSnapshot"] = json!(true);
        assert!(matches!(
            p.receive(&c, &frame(vec![conflict]), now()),
            Err(WsError::FrameAt(FrameCode::SnapshotFlag))
        ));
        for omitted in [true, false] {
            let (mut p, c) = setup();
            let mut stale = old();
            if omitted {
                stale.as_object_mut().unwrap().remove("is_data_snapshot");
            } else {
                stale["is_data_snapshot"] = json!(false);
            }
            assert!(matches!(
                p.receive(&c, &frame(vec![stale]), now()),
                Err(WsError::FrameAt(FrameCode::TimestampBounds))
            ));
        }
        let (mut p, c) = setup();
        let mut camel = old();
        camel.as_object_mut().unwrap().remove("is_data_snapshot");
        camel["isDataSnapshot"] = json!(true);
        assert!(p
            .receive(&c, &frame(vec![camel]), now())
            .unwrap()
            .live_bars()
            .is_empty());
    }

    #[test]
    fn split_history_caps_and_silence_do_not_reset() {
        let (mut p, c) = setup();
        for _ in 0..2 {
            p.receive(&c, &frame(vec![old(); 256]), now()).unwrap();
        }
        assert!(matches!(
            p.receive(&c, &frame(vec![old()]), now()),
            Err(WsError::Budget)
        ));
        let (mut p, c) = setup();
        let mut padded = old();
        padded["ignored"] = json!("x".repeat(140_000));
        p.receive(&c, &frame(vec![padded.clone()]), now()).unwrap();
        assert!(matches!(
            p.receive(&c, &frame(vec![padded]), now()),
            Err(WsError::Budget)
        ));
        let (mut p, c) = setup();
        p.receive(
            &c,
            &frame(vec![row(true, now())]),
            now() + Duration::seconds(59),
        )
        .unwrap();
        assert!(p.last_data.is_none());
        assert_eq!(
            p.check_silence(now() + Duration::seconds(60)),
            Err(WsError::Silence)
        );
    }

    #[test]
    fn auth_rotation_resets_only_quota_and_owner_not_silence() {
        let (mut p, c) = setup();
        p.receive(&c, &frame(vec![old(); 512]), now()).unwrap();
        p.disconnected(&c, now() + Duration::seconds(1)).unwrap();
        let next = p.connected_for_auth(now() + Duration::seconds(2)).unwrap();
        subscribe(&mut p, &next, now() + Duration::seconds(2));
        assert_eq!(p.snapshot_rows, 0);
        assert!(matches!(
            p.receive(&c, &frame(vec![old()]), now()),
            Err(WsError::Connection)
        ));
        p.receive(&next, &frame(vec![old()]), now() + Duration::seconds(3))
            .unwrap();
        assert_eq!(
            p.remaining_silence(now() + Duration::seconds(3))
                .unwrap()
                .as_secs(),
            57
        );
        p.stop();
        assert!(p
            .receive(&next, &frame(vec![live()]), now() + Duration::seconds(4))
            .is_err());
    }
}

#[cfg(test)]
mod auth_rotation_tests {
    use super::*;
    #[test]
    fn auxiliary_after_rotation_never_refreshes_silence_or_generation_readiness() {
        let now = Utc::now();
        let mut p = BarsProtocol::new_quotes(5000, 1000, 5000, 1).unwrap();
        let c = p.connected(now).unwrap();
        let subscribe = |p: &mut BarsProtocol, c: &Connection, t| {
            p.receive(
                c,
                r#"{"type":"EVENT","eventInfo":{"event":"HANDSHAKE_SUCCESS"}}"#,
                t,
            )
            .unwrap();
            p.subscription_request(c, &AccessToken::new("SECRET"))
                .unwrap();
            p.subscription_sent(c).unwrap();
        };
        subscribe(&mut p, &c, now);
        let frame = |aux: bool| {
            let mut row = serde_json::json!({"symbol":"IMOEXF@RTSX","timestamp":now.to_rfc3339(),"last":{"value":"2535.5"}});
            if aux {
                row["bid"] = serde_json::json!({"value":"2535"});
            }
            serde_json::json!({"type":"DATA","subscriptionType":"QUOTES","payload":{"quote":[row]}})
                .to_string()
        };
        let Observation::Quotes(q) = p.receive(&c, &frame(false), now).unwrap() else {
            panic!()
        };
        assert_eq!(q.len(), 1);
        p.disconnected(&c, now + Duration::milliseconds(1)).unwrap();
        let c = p
            .connected_for_auth(now + Duration::milliseconds(2))
            .unwrap();
        subscribe(&mut p, &c, now + Duration::milliseconds(2));
        for auth in [false, true] {
            let mut witness = if auth {
                crate::s5::Witness::new_auth(crate::s5::Channel::Quotes, 2, 2)
            } else {
                crate::s5::Witness::new(crate::s5::Channel::Quotes, 2)
            };
            witness.receipts(&Observation::Handshake, now.timestamp_millis());
            for ms in [3, 4] {
                let o = p
                    .receive(
                        &c,
                        &frame(true),
                        now + Duration::milliseconds(ms + if auth { 2 } else { 0 }),
                    )
                    .unwrap();
                let Observation::Quotes(q) = &o else { panic!() };
                assert!(q.is_empty());
                let r = serde_json::to_string(&witness.receipts(&o, now.timestamp_millis() + ms))
                    .unwrap();
                assert!(!r.contains("first_validated_data"));
                if ms == 3 {
                    assert!(r.contains("auxiliary_updates_observed"));
                } else {
                    assert_eq!(r, "[]");
                }
            }
        }
        assert_eq!(p.last_data, Some(now));
        assert_eq!(p.quotes.as_ref().unwrap().retained_len(), 1);
        assert_eq!(
            p.check_silence(now + Duration::milliseconds(5000)),
            Err(WsError::Silence)
        );
    }
    #[test]
    fn auth_rotations_do_not_consume_controlled_reconnect_or_reset_silence() {
        let now = Utc::now();
        let mut p = BarsProtocol::new(5000, 1000).unwrap();
        let mut c = p.connected(now).unwrap();
        for ms in [1, 2, 3] {
            let at = now + Duration::milliseconds(ms);
            p.disconnected(&c, at).unwrap();
            let old = c;
            c = p.connected_for_auth(at).unwrap();
            assert!(matches!(p.require(&old), Err(WsError::Connection)));
            assert_eq!(p.controlled_connections, 1);
            assert_eq!(
                p.remaining_silence(at).unwrap().as_millis(),
                5000 - ms as u128
            );
        }
        p.disconnected(&c, now + Duration::milliseconds(4)).unwrap();
        c = p.connected(now + Duration::milliseconds(4)).unwrap();
        assert_eq!(p.controlled_connections, 2);
        p.disconnected(&c, now + Duration::milliseconds(5)).unwrap();
        assert!(p.connected(now + Duration::milliseconds(5)).is_err());
    }

    #[test]
    fn auth_rotation_does_not_heal_faulted_protocol() {
        let now = Utc::now();
        let mut p = BarsProtocol::new(1000, 500).unwrap();
        let c = p.connected(now).unwrap();
        assert!(p.check_silence(now + Duration::milliseconds(1000)).is_err());
        assert!(p
            .disconnected(&c, now + Duration::milliseconds(1000))
            .is_err());
        assert!(p
            .connected_for_auth(now + Duration::milliseconds(1000))
            .is_err());
    }
}

#[cfg(test)]
pub(crate) mod initial_quote_tests {
    use super::*;
    use serde_json::json;
    fn now() -> DateTime<Utc> {
        DateTime::from_timestamp(1793181600, 687828728).unwrap()
    }
    fn frame(t: DateTime<Utc>) -> String {
        json!({"type":"DATA","subscriptionType":"QUOTES","payload":{"quotes":[{
            "symbol":"IMOEXF@RTSX","timestamp":t.to_rfc3339(),"last":{"value":"2535.5"}
        }]}})
        .to_string()
    }
    fn handshake(p: &mut BarsProtocol, c: &Connection, t: DateTime<Utc>) -> Observation {
        let out = p
            .receive(
                c,
                r#"{"type":"EVENT","eventInfo":{"event":"HANDSHAKE_SUCCESS"}}"#,
                t,
            )
            .unwrap();
        p.subscription_request(c, &AccessToken::new("TEST"))
            .unwrap();
        p.subscription_sent(c).unwrap();
        out
    }
    pub(crate) fn observations() -> Vec<Observation> {
        let mut p = BarsProtocol::new_quotes(60000, 5000, 5000, 4096)
            .unwrap()
            .diagnostic_initial_quotes();
        let c = p.connected(now()).unwrap();
        vec![
            handshake(&mut p, &c, now()),
            p.receive(&c, &frame(now() - Duration::seconds(9)), now())
                .unwrap(),
            p.receive(&c, &frame(now()), now()).unwrap(),
        ]
    }
    #[test]
    fn initial_silence_not_extended_and_reconnect_never_reopens() {
        for auth in [false, true] {
            let mut p = BarsProtocol::new_quotes(60000, 5000, 5000, 4096)
                .unwrap()
                .diagnostic_initial_quotes();
            let c = p.connected(now()).unwrap();
            handshake(&mut p, &c, now());
            p.receive(&c, &frame(now() - Duration::seconds(9)), now())
                .unwrap();
            assert!(p.last_data.is_none());
            p.disconnected(&c, now() + Duration::seconds(1)).unwrap();
            let c = if auth {
                p.connected_for_auth(now() + Duration::seconds(2))
            } else {
                p.connected(now() + Duration::seconds(2))
            }
            .unwrap();
            handshake(&mut p, &c, now() + Duration::seconds(2));
            assert!(p
                .receive(
                    &c,
                    &frame(now() - Duration::seconds(9)),
                    now() + Duration::seconds(2)
                )
                .is_err());
        }
        let mut p = BarsProtocol::new_quotes(60000, 5000, 5000, 4096)
            .unwrap()
            .diagnostic_initial_quotes();
        let c = p.connected(now()).unwrap();
        handshake(&mut p, &c, now());
        p.receive(
            &c,
            &frame(now() - Duration::seconds(9)),
            now() + Duration::seconds(59),
        )
        .unwrap();
        assert!(p.last_data.is_none());
        assert!(matches!(
            p.check_silence(now() + Duration::seconds(60)),
            Err(WsError::Silence)
        ));
    }
    #[test]
    fn initial_anchor_is_handshake_not_connection_or_frame() {
        let mut p = BarsProtocol::new_quotes(60000, 5000, 5000, 4096)
            .unwrap()
            .diagnostic_initial_quotes();
        let c = p.connected(now()).unwrap();
        handshake(&mut p, &c, now() + Duration::seconds(2));
        let o = p
            .receive(
                &c,
                &frame(now() + Duration::seconds(1)),
                now() + Duration::seconds(9),
            )
            .unwrap();
        assert!(
            matches!(o, Observation::Quotes(f) if f.quotes.is_empty() && f.initial_discard.is_some())
        );
        assert!(p
            .receive(
                &c,
                &frame(now() + Duration::seconds(2)),
                now() + Duration::seconds(9)
            )
            .is_err());
    }
}
