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

#[derive(Debug, PartialEq, Eq)]
pub enum WsError {
    State,
    Connection,
    Clock,
    Silence,
    Frame,
    Provider,
    Budget,
}

#[derive(Debug)]
pub struct QuoteFrame {
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
    Quotes(QuoteFrame),
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
    quotes: Option<crate::quote::QuoteInput>,
    owner: Arc<()>,
    generation: u64,
    state: State,
    last_clock: Option<DateTime<Utc>>,
    last_data: Option<DateTime<Utc>>,
    connected_at: Option<DateTime<Utc>>,
    disconnected_at: Option<DateTime<Utc>>,
    key: Option<String>,
    silence: Duration,
    reconnect: Duration,
}

impl BarsProtocol {
    pub fn new(silence_ms: u64, reconnect_ms: u64) -> Result<Self, WsError> {
        if silence_ms == 0
            || reconnect_ms == 0
            || silence_ms > 3_600_000
            || reconnect_ms > 3_600_000
        {
            return Err(WsError::Budget);
        }
        Ok(Self {
            quotes: None,
            owner: Arc::new(()),
            generation: 0,
            state: State::Idle,
            last_clock: None,
            last_data: None,
            connected_at: None,
            disconnected_at: None,
            key: None,
            silence: Duration::milliseconds(silence_ms as i64),
            reconnect: Duration::milliseconds(reconnect_ms as i64),
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

    /// Reserve a connection attempt; old socket must already be closed.
    /// Handshake completion is still required before subscription.
    /// At most one controlled reconnect (generation 2) in this qualification slice.
    pub fn connected(&mut self, now: DateTime<Utc>) -> Result<Connection, WsError> {
        if !matches!(self.state, State::Idle | State::Disconnected) {
            return Err(WsError::State);
        }
        self.clock(now)?;
        if self.generation >= 2
            || self
                .disconnected_at
                .is_some_and(|t| now - t > self.reconnect)
        {
            self.state = State::Held;
            return Err(WsError::Budget);
        }
        self.generation += 1;
        self.state = State::Handshake;
        self.connected_at = Some(now);
        self.last_data = None;
        self.key = None;
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
            return Err(WsError::Frame);
        }
        let e: FinamWsEnvelope = serde_json::from_str(text).map_err(|_| WsError::Frame)?;
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
            return Err(WsError::Frame);
        }
        if let Some(key) = &e.subscription_key {
            if key.is_empty() || key.len() > 256 || self.key.as_ref().is_some_and(|k| k != key) {
                return Err(WsError::Frame);
            }
        }
        if let Some(quotes) = &mut self.quotes {
            let observed = quotes.receive(text, now).map_err(|_| WsError::Frame)?;
            self.key = e.subscription_key.or_else(|| self.key.clone());
            // Replayed quotes cannot indefinitely refresh the data-silence budget.
            if !observed.is_empty() {
                self.last_data = Some(now);
            }
            return Ok(Observation::Quotes(QuoteFrame {
                raw_frame: text.to_owned(),
                received_at_ms: now.timestamp_millis(),
                quotes: observed,
            }));
        }
        let payload = match &e.payload {
            Some(Value::String(s)) => {
                serde_json::from_str::<Value>(s).map_err(|_| WsError::Frame)?
            }
            Some(v) => v.clone(),
            None => return Err(WsError::Frame),
        };
        if payload.get("error").is_some_and(|v| !v.is_null())
            || payload
                .get("symbol")
                .is_some_and(|v| v.as_str() != Some(SYMBOL))
            || payload
                .get("timeframe")
                .is_some_and(|v| v.as_str() != Some(TIMEFRAME))
        {
            return Err(WsError::Frame);
        }
        let rows = payload
            .get("bars")
            .or_else(|| payload.get("bar"))
            .and_then(Value::as_array)
            .ok_or(WsError::Frame)?;
        if rows.is_empty() || rows.len() > 512 {
            return Err(WsError::Frame);
        }
        for row in rows {
            let t: DateTime<Utc> = row
                .get("timestamp")
                .and_then(Value::as_str)
                .ok_or(WsError::Frame)?
                .parse()
                .map_err(|_| WsError::Frame)?;
            if t > now
                || t.timestamp() % 60 != 0
                || t.timestamp_subsec_nanos() != 0
                || t.checked_add_signed(Duration::minutes(1)).is_none()
                || t.checked_add_signed(Duration::minutes(1))
                    .is_some_and(|close| now - close > self.silence)
            {
                return Err(WsError::Frame);
            }
        }
        let events = broker_finam::map_ws_market_data_events(&e, Some(SYMBOL), 60, now)
            .map_err(|_| WsError::Frame)?;
        let mut bars = Vec::new();
        for event in events {
            let MarketDataEvent::Bar(mut bar) = event else {
                return Err(WsError::Frame);
            };
            if bar.instrument.venue_symbol.as_deref() != Some(SYMBOL)
                || bar.low > bar.open
                || bar.low > bar.close
                || bar.high < bar.open
                || bar.high < bar.close
                || bar.low <= 0.into()
                || bar.volume < 0.into()
            {
                return Err(WsError::Frame);
            }
            bar.instrument.market = broker_core::Market::Futures;
            bar.is_final = false; // clock-based mapper finality is NOT source proof
            bars.push(bar);
        }
        self.key = e.subscription_key.or_else(|| self.key.clone());
        self.last_data = Some(now);
        Ok(Observation::ProvisionalBars(bars))
    }

    pub fn check_silence(&mut self, now: DateTime<Utc>) -> Result<(), WsError> {
        if matches!(
            self.state,
            State::Idle | State::Disconnected | State::Held | State::Stopped
        ) {
            return Err(WsError::State);
        }
        self.clock(now)?;
        if self
            .last_data
            .or(self.connected_at)
            .is_some_and(|t| now - t >= self.silence)
        {
            self.state = State::Held;
            return Err(WsError::Silence);
        }
        Ok(())
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
