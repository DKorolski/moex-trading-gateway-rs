//! Candle-only transport liveness. No model readiness or reconnect authority.
use std::time::Duration;
use tokio::time::Instant;

const PING: Duration = Duration::from_secs(30);
const PONG: Duration = Duration::from_secs(15);
const IDLE: Duration = Duration::from_secs(70);
const WARN: Duration = Duration::from_secs(60);
const DATA: Duration = Duration::from_secs(1200);

pub(crate) fn policy() -> serde_json::Value {
    serde_json::json!({
        "version":"candle_bars_heartbeat_recovery_v2", "ping_interval_ms":PING.as_millis(),
        "matching_pong_timeout_ms":PONG.as_millis(), "transport_idle_ms":IDLE.as_millis(),
        "bars_warning_ms":WARN.as_millis(), "bars_hold_ms":DATA.as_millis(),
        "automatic_reconnect":true, "heartbeat_proves_market_readiness":false,
        "recovery_attempt_limit":3,"recovery_backoff_ms":[1000,2000,4000],
        "recovery_episode_budget_ms":45000,"recovery_run_episode_limit":3,
        "recovery_first_data_max_ms":5000,"automatic_canonical_resync":false,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum TimeoutKind {
    Pong,
    TransportIdle,
    BarsDataSilence,
    HeartbeatSend,
    ServerPongFlush,
    RunDeadline,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Snapshot {
    pub observed_at_ms: i64,
    pub connected_at_ms: i64,
    pub last_rx_at_ms: Option<i64>,
    pub last_live_bars_at_ms: Option<i64>,
    pub last_ping_at_ms: Option<i64>,
    pub last_matched_pong_at_ms: Option<i64>,
    pub rx_age_ms: u64,
    pub bars_data_age_ms: u64,
    pub pending_pong_age_ms: Option<u64>,
    pub timeout: Option<TimeoutKind>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum Event {
    PingSent,
    PongConfirmed,
    BarsDataSilence,
    BarsDataResumed,
}

pub(super) struct Watchdog {
    connected: (Instant, i64),
    rx: Option<(Instant, i64)>,
    data: Option<(Instant, i64)>,
    data_anchor: Instant,
    next_ping: Instant,
    pending: Option<(Instant, u64)>,
    serial: u64,
    last_ping: Option<i64>,
    last_pong: Option<i64>,
    warned: bool,
    event: Option<Event>,
    timeout: Option<TimeoutKind>,
}

impl Watchdog {
    pub fn new() -> Self {
        let now = Instant::now();
        Self {
            connected: (now, chrono::Utc::now().timestamp_millis()),
            rx: None,
            data: None,
            data_anchor: now,
            next_ping: now + PING,
            pending: None,
            serial: 0,
            last_ping: None,
            last_pong: None,
            warned: false,
            event: None,
            timeout: None,
        }
    }

    pub fn reconnected(&mut self) {
        let now = Instant::now();
        self.connected = (now, chrono::Utc::now().timestamp_millis());
        self.rx = None;
        self.pending = None;
        self.next_ping = now + PING;
        self.last_ping = None;
        self.last_pong = None;
        self.event = None;
        // Market-data silence and warning survive a planned healthy reconnect.
    }

    pub fn check(&self, now: Instant) -> Result<(), TimeoutKind> {
        if self.pending.is_some_and(|(at, _)| now >= at + PONG) {
            return Err(TimeoutKind::Pong);
        }
        if now >= self.rx.unwrap_or(self.connected).0 + IDLE {
            return Err(TimeoutKind::TransportIdle);
        }
        if now >= self.data_anchor + DATA {
            return Err(TimeoutKind::BarsDataSilence);
        }
        Ok(())
    }

    pub fn next(&self) -> Instant {
        let mut at = (self.rx.unwrap_or(self.connected).0 + IDLE).min(self.data_anchor + DATA);
        at = at.min(if let Some((sent, _)) = self.pending {
            sent + PONG
        } else {
            self.next_ping
        });
        if !self.warned {
            at = at.min(self.data_anchor + WARN);
        }
        at
    }

    pub fn warn(&mut self, now: Instant) -> bool {
        if !self.warned && now >= self.data_anchor + WARN {
            self.warned = true;
            self.event = Some(Event::BarsDataSilence);
            true
        } else {
            false
        }
    }

    pub fn ping_due(&self, now: Instant) -> bool {
        self.pending.is_none() && now >= self.next_ping
    }

    pub fn payload(&mut self) -> Vec<u8> {
        self.serial = self
            .serial
            .checked_add(1)
            .expect("heartbeat counter bounded by session");
        self.serial.to_be_bytes().to_vec()
    }

    pub fn sent(&mut self, now: Instant) {
        self.pending = Some((now, self.serial));
        self.last_ping = Some(chrono::Utc::now().timestamp_millis());
        self.next_ping = now + PING;
        self.event = Some(Event::PingSent);
    }

    pub fn received(&mut self, now: Instant) {
        self.rx = Some((now, chrono::Utc::now().timestamp_millis()));
    }

    pub fn pong(&mut self, payload: &[u8], now: Instant) {
        if self
            .pending
            .is_some_and(|(_, serial)| payload == serial.to_be_bytes())
        {
            self.pending = None;
            self.last_pong = Some(chrono::Utc::now().timestamp_millis());
            self.event = Some(Event::PongConfirmed);
        }
        self.received(now);
    }

    pub fn data(&mut self, now: Instant) {
        self.data_observed(now, chrono::Utc::now().timestamp_millis());
    }

    pub fn data_observed(&mut self, now: Instant, wall_ms: i64) {
        self.data_anchor = now;
        self.data = Some((now, wall_ms));
        if self.warned {
            self.event = Some(Event::BarsDataResumed);
        }
        self.warned = false;
    }

    pub fn failed(&mut self, kind: TimeoutKind) {
        self.timeout = Some(kind);
    }

    pub fn take_event(&mut self) -> Option<serde_json::Value> {
        self.event.take().map(|event| {
            serde_json::json!({
                "event":"bars_liveness", "cause":event, "liveness":self.snapshot(),
                "market_readiness_proven":false,
            })
        })
    }

    pub fn snapshot(&self) -> Snapshot {
        let now = Instant::now();
        let age =
            |at| u64::try_from(now.saturating_duration_since(at).as_millis()).unwrap_or(u64::MAX);
        Snapshot {
            observed_at_ms: chrono::Utc::now().timestamp_millis(),
            connected_at_ms: self.connected.1,
            last_rx_at_ms: self.rx.map(|(_, at)| at),
            last_live_bars_at_ms: self.data.map(|(_, at)| at),
            last_ping_at_ms: self.last_ping,
            last_matched_pong_at_ms: self.last_pong,
            rx_age_ms: age(self.rx.unwrap_or(self.connected).0),
            bars_data_age_ms: age(self.data_anchor),
            pending_pong_age_ms: self.pending.map(|(at, _)| age(at)),
            timeout: self.timeout,
        }
    }
}

#[cfg(test)]
mod tests;
