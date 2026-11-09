use super::PrimaryError;
use broker_core::{event::Bar, MarketDataEvent};
use broker_finam::FinamWsEnvelope;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum SnapshotPresence {
    Missing,
    ExplicitFalse,
    ExplicitTrue,
}

impl SnapshotPresence {
    pub fn parse(row: &Value) -> Result<Self, PrimaryError> {
        let object = row.as_object().ok_or(PrimaryError::Wire)?;
        match (object.get("is_data_snapshot"), object.get("isDataSnapshot")) {
            (None, None) => Ok(Self::Missing),
            (Some(Value::Bool(true)), None) | (None, Some(Value::Bool(true))) => {
                Ok(Self::ExplicitTrue)
            }
            (Some(Value::Bool(false)), None) | (None, Some(Value::Bool(false))) => {
                Ok(Self::ExplicitFalse)
            }
            _ => Err(PrimaryError::SnapshotFlag),
        }
    }
}

#[derive(Debug, Clone)]
pub struct WireRow {
    pub(crate) bar: Bar,
    pub(crate) presence: SnapshotPresence,
}

#[derive(Debug, Clone)]
pub struct WireFrame {
    pub(crate) rows: Vec<WireRow>,
    pub(crate) generation: u64,
    pub(crate) received_at_ms: i64,
    pub(crate) wire_bytes: usize,
    pub(crate) sha256: String,
    pub(crate) received_mono: tokio::time::Instant,
    pub(crate) owner: Option<std::sync::Arc<()>>,
}

impl WireFrame {
    /// Numeric/wire validation only. The socket adapter must bind the provided
    /// subscription key and generation to its physical connection before use.
    /// This does not assign finality or infer history from age/count/first frame.
    pub fn normalize(
        text: &str,
        generation: u64,
        subscription_key: &str,
        received_at: DateTime<Utc>,
    ) -> Result<Self, PrimaryError> {
        let received_mono = tokio::time::Instant::now();
        if text.len() > 256 * 1024 || generation == 0 || subscription_key.is_empty() {
            return Err(PrimaryError::Wire);
        }
        let envelope: FinamWsEnvelope =
            serde_json::from_str(text).map_err(|_| PrimaryError::Wire)?;
        if envelope.envelope_type != "DATA"
            || envelope.subscription_type.as_deref() != Some("BARS")
            || envelope.subscription_key.as_deref() != Some(subscription_key)
        {
            return Err(PrimaryError::Identity);
        }
        let payload = match &envelope.payload {
            Some(Value::String(value)) => {
                serde_json::from_str(value).map_err(|_| PrimaryError::Wire)?
            }
            Some(value) => value.clone(),
            None => return Err(PrimaryError::Wire),
        };
        if payload.get("symbol").and_then(Value::as_str) != Some("IMOEXF@RTSX")
            || payload
                .get("timeframe")
                .is_some_and(|v| v != "TIME_FRAME_M1")
            || payload.get("error").is_some_and(|v| !v.is_null())
        {
            return Err(PrimaryError::Identity);
        }
        let values = match (payload.get("bars"), payload.get("bar")) {
            (Some(value), None) | (None, Some(value)) => {
                value.as_array().ok_or(PrimaryError::Wire)?
            }
            _ => return Err(PrimaryError::Wire),
        };
        if values.is_empty() || values.len() > 512 {
            return Err(PrimaryError::Wire);
        }
        let flags = values
            .iter()
            .map(SnapshotPresence::parse)
            .collect::<Result<Vec<_>, _>>()?;
        for row in values {
            let timestamp: DateTime<Utc> = row
                .get("timestamp")
                .and_then(Value::as_str)
                .ok_or(PrimaryError::Timestamp)?
                .parse()
                .map_err(|_| PrimaryError::Timestamp)?;
            if timestamp > received_at
                || timestamp.timestamp() < 0
                || timestamp.timestamp() % 60 != 0
                || timestamp.timestamp_subsec_nanos() != 0
                || timestamp
                    .checked_add_signed(chrono::Duration::minutes(1))
                    .is_none()
            {
                return Err(PrimaryError::Timestamp);
            }
        }
        // The mapper is used for pure Decimal DTO conversion, not finality.
        let events = broker_finam::map_ws_market_data_events(
            &envelope,
            Some("IMOEXF@RTSX"),
            60,
            received_at,
        )
        .map_err(|_| PrimaryError::Wire)?;
        if events.len() != values.len() {
            return Err(PrimaryError::Wire);
        }
        let mut rows = Vec::with_capacity(events.len());
        for (event, presence) in events.into_iter().zip(flags) {
            let MarketDataEvent::Bar(mut bar) = event else {
                return Err(PrimaryError::Wire);
            };
            bar.instrument.market = broker_core::Market::Futures;
            bar.is_final = false;
            check_bar(&bar, received_at.timestamp_millis())?;
            rows.push(WireRow { bar, presence });
        }
        Ok(Self {
            rows,
            generation,
            received_at_ms: received_at.timestamp_millis(),
            wire_bytes: text.len(),
            sha256: crate::seed::digest(text.as_bytes()),
            received_mono,
            owner: None,
        })
    }
    pub fn generation(&self) -> u64 {
        self.generation
    }
    pub fn received_at_ms(&self) -> i64 {
        self.received_at_ms
    }
    pub fn sha256(&self) -> &str {
        &self.sha256
    }
}

#[derive(Debug, Clone)]
pub struct LiveDataStamp {
    pub(crate) generation: u64,
    pub(crate) received_at_ms: i64,
    pub(crate) received_mono: tokio::time::Instant,
    pub(crate) owner: Option<std::sync::Arc<()>>,
}

pub(super) fn check_bar(bar: &Bar, received_ms: i64) -> Result<(), PrimaryError> {
    if bar.instrument.venue_symbol.as_deref() != Some("IMOEXF@RTSX") || bar.timeframe_sec != 60 {
        return Err(PrimaryError::Identity);
    }
    if bar.open_ts.timestamp_millis() < 0
        || bar.open_ts.timestamp() % 60 != 0
        || bar.open_ts.timestamp_subsec_nanos() != 0
        || bar.open_ts.timestamp_millis() > received_ms
        || bar.close_ts
            != bar
                .open_ts
                .checked_add_signed(chrono::Duration::minutes(1))
                .ok_or(PrimaryError::Timestamp)?
    {
        return Err(PrimaryError::Timestamp);
    }
    if bar.low <= Decimal::ZERO
        || bar.low > bar.high
        || bar.low > bar.open.min(bar.close)
        || bar.high < bar.open.max(bar.close)
        || bar.volume < Decimal::ZERO
        || !bar.volume.fract().is_zero()
        || [bar.open, bar.high, bar.low, bar.close]
            .iter()
            .any(|p| !(*p % Decimal::new(5, 1)).is_zero())
    {
        return Err(PrimaryError::Numeric);
    }
    Ok(())
}

pub(crate) fn same_values(a: &Bar, b: &Bar) -> bool {
    a.instrument == b.instrument
        && a.timeframe_sec == b.timeframe_sec
        && a.open_ts == b.open_ts
        && a.close_ts == b.close_ts
        && (a.open, a.high, a.low, a.close, a.volume) == (b.open, b.high, b.low, b.close, b.volume)
}
