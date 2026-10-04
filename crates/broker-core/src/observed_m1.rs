//! Broker-neutral, retained projection of a previously admitted closed source.
//!
//! This is evidence, NOT transport or publication authority. The adapter must
//! first validate its complete raw response and map EVERY in-range M1. Durable
//! consumers must authenticate `expected_sha256` from their retained seal/source
//! binding, never take it from the candidate being checked. No provider API DTO,
//! network, clock override, or lifecycle capability is introduced here.

use crate::{event::Bar, InstrumentId, MarketDataSourceKind};
use chrono::{DateTime, Duration, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const OBSERVED_M1_POLICY_V1: &str = "closed-rest-observed-m1-to-m10-v1";
const RECEIPT_DOMAIN: &str = "moex.closed-rest-observed-m1-receipt.v1";
const MAX_BYTES: usize = 16 * 1024 * 1024;
const MAX_ROWS: usize = 7 * 24 * 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ObservedM1Error {
    #[error("observed M1 retained source binding is invalid")]
    Binding,
    #[error("observed M1 retained source encoding is invalid")]
    Encoding,
    #[error("observed M1 interval or chronology is invalid")]
    Range,
    #[error("observed M1 input is invalid")]
    Bar,
    #[error("observed M10 contains no actual M1")]
    EmptyBucket,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReceiptV1 {
    schema_version: u16,
    domain: String,
    policy: String,
    source_snapshot_sha256: String,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    received_at: DateTime<Utc>,
    instrument: InstrumentId,
    bars: Vec<Bar>,
}

/// One whole normalized source, stored once, not in every M10. No Deserialize
/// for this checked type. Its digest commits to the full minute inventory,
/// prices, raw snapshot reference and the closed range together.
#[derive(Debug)]
pub struct ObservedM1Receipt {
    value: ReceiptV1,
    bytes: Vec<u8>,
    sha256: String,
}

#[derive(Debug)]
pub struct ObservedM10Derivation {
    pub bar: Bar,
    pub actual_m1: Vec<Bar>,
    pub minute_bitmap: u16,
}

impl ObservedM1Receipt {
    /// Adapter-side projection only. Source completeness/finality has to be
    /// admitted before this call; a normalized vector cannot prove HTTP success.
    pub fn from_admitted_source(
        source_snapshot_sha256: String,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
        received_at: DateTime<Utc>,
        instrument: InstrumentId,
        mut bars: Vec<Bar>,
    ) -> Result<Self, ObservedM1Error> {
        for bar in &mut bars {
            bar.open = bar.open.normalize();
            bar.high = bar.high.normalize();
            bar.low = bar.low.normalize();
            bar.close = bar.close.normalize();
            bar.volume = bar.volume.normalize();
        }
        Self::checked(ReceiptV1 {
            schema_version: 1,
            domain: RECEIPT_DOMAIN.into(),
            policy: OBSERVED_M1_POLICY_V1.into(),
            source_snapshot_sha256,
            start,
            end,
            received_at,
            instrument,
            bars,
        })
    }

    /// `expected_sha256` must come from the authenticated retained source,
    /// independently of the canonical M10 payload being admitted.
    pub fn restore(bytes: &[u8], expected_sha256: &str) -> Result<Self, ObservedM1Error> {
        if bytes.len() > MAX_BYTES || !is_sha256(expected_sha256) || sha(bytes) != expected_sha256 {
            return Err(ObservedM1Error::Binding);
        }
        let value = serde_json::from_slice(bytes).map_err(|_| ObservedM1Error::Encoding)?;
        let result = Self::checked(value)?;
        if result.bytes != bytes {
            return Err(ObservedM1Error::Encoding);
        }
        Ok(result)
    }

    fn checked(value: ReceiptV1) -> Result<Self, ObservedM1Error> {
        if value.schema_version != 1
            || value.domain != RECEIPT_DOMAIN
            || value.policy != OBSERVED_M1_POLICY_V1
            || !is_sha256(&value.source_snapshot_sha256)
            || value.instrument.symbol.is_empty()
        {
            return Err(ObservedM1Error::Binding);
        }
        if !minute_aligned(value.start)
            || !minute_aligned(value.end)
            || value.start >= value.end
            || value.end > value.received_at
            || value.end - value.start > Duration::days(7)
            || value.bars.len() > MAX_ROWS
        {
            return Err(ObservedM1Error::Range);
        }
        let mut prior = None;
        for b in &value.bars {
            if b.instrument != value.instrument
                || b.source_kind != MarketDataSourceKind::HistoricalPoll
                || b.timeframe_sec != 60
                || !b.is_final
                || !minute_aligned(b.open_ts)
                || b.open_ts < value.start
                || b.open_ts >= value.end
                || b.open_ts.checked_add_signed(Duration::seconds(60)) != Some(b.close_ts)
                || prior.is_some_and(|p| p >= b.open_ts)
                || b.low <= Decimal::ZERO
                || b.low > b.high
                || b.high < b.open.max(b.close)
                || b.low > b.open.min(b.close)
                || b.volume < Decimal::ZERO
                || [b.open, b.high, b.low, b.close, b.volume]
                    .iter()
                    .any(|v| v.to_string() != v.normalize().to_string())
            {
                return Err(ObservedM1Error::Bar);
            }
            prior = Some(b.open_ts);
        }
        let bytes = serde_json::to_vec(&value).map_err(|_| ObservedM1Error::Encoding)?;
        if bytes.len() > MAX_BYTES {
            return Err(ObservedM1Error::Encoding);
        }
        let sha256 = sha(&bytes);
        Ok(Self {
            value,
            bytes,
            sha256,
        })
    }

    pub fn sha256(&self) -> &str {
        &self.sha256
    }
    pub fn source_snapshot_sha256(&self) -> &str {
        &self.value.source_snapshot_sha256
    }
    pub fn retained_bytes(&self) -> &[u8] {
        &self.bytes
    }
    pub fn bars(&self) -> &[Bar] {
        &self.value.bars
    }
    pub fn instrument(&self) -> &InstrumentId {
        &self.value.instrument
    }
    pub fn start(&self) -> DateTime<Utc> {
        self.value.start
    }
    pub fn end(&self) -> DateTime<Utc> {
        self.value.end
    }
    pub fn received_at(&self) -> DateTime<Utc> {
        self.value.received_at
    }

    /// Fixed nominal boundaries. Missing first/last M1 never shift the window.
    pub fn bucket(&self, open: DateTime<Utc>) -> Result<ObservedM10Derivation, ObservedM1Error> {
        let close = open
            .checked_add_signed(Duration::seconds(600))
            .ok_or(ObservedM1Error::Range)?;
        if !minute_aligned(open)
            || open.timestamp() % 600 != 0
            || open < self.value.start
            || close > self.value.end
        {
            return Err(ObservedM1Error::Range);
        }
        let start = self.value.bars.partition_point(|b| b.open_ts < open);
        let end = self.value.bars.partition_point(|b| b.open_ts < close);
        let actual_m1 = self.value.bars[start..end].to_vec();
        let mut bar = actual_m1
            .first()
            .ok_or(ObservedM1Error::EmptyBucket)?
            .clone();
        bar.open_ts = open;
        bar.close_ts = close;
        bar.timeframe_sec = 600;
        bar.close = actual_m1.last().ok_or(ObservedM1Error::EmptyBucket)?.close;
        bar.volume = Decimal::ZERO;
        let mut minute_bitmap = 0;
        for b in &actual_m1 {
            bar.high = bar.high.max(b.high);
            bar.low = bar.low.min(b.low);
            bar.volume = bar
                .volume
                .checked_add(b.volume)
                .ok_or(ObservedM1Error::Bar)?;
            minute_bitmap |= 1 << ((b.open_ts - open).num_seconds() / 60);
        }
        Ok(ObservedM10Derivation {
            bar,
            actual_m1,
            minute_bitmap,
        })
    }
}

fn minute_aligned(t: DateTime<Utc>) -> bool {
    t.timestamp() > 0 && t.timestamp() % 60 == 0 && t.timestamp_subsec_nanos() == 0
}
fn is_sha256(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}
fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
