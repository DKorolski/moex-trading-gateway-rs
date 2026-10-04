//! Closed REST snapshot admission for observed-M1 M10 (additive policy V1).
//!
//! A successful provider snapshot is a trust boundary, not proof that every
//! missing minute had no trades. Stream silence cannot construct this type.
//! Raw bodies are retained once per snapshot; buckets bind to its digest.
//! This module performs no I/O and does not enable the policy operationally.

use std::collections::BTreeMap;

use broker_core::observed_m1::{ObservedM1Error, ObservedM1Receipt};
use broker_core::{event::Bar, Exchange, InstrumentId, Market, MarketDataSourceKind};
use chrono::{DateTime, Duration, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{dto, map_bar, STAGE8B_P1F_O2_VENUE_SYMBOL};

pub use broker_core::observed_m1::OBSERVED_M1_POLICY_V1;
pub const SNAPSHOT_MAX_BODY_BYTES: usize = 32 * 1024 * 1024;
const MAX_PARTS: usize = 8;
const MAX_SNAPSHOT_AGE_SECONDS: i64 = 900;

/// Fixed BEFORE transport from calendar/config authority, independently of
/// returned metadata. An incomplete response list cannot redefine its end.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClosedM1RequestPlan {
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    part_ends: Vec<DateTime<Utc>>,
}

impl ClosedM1RequestPlan {
    pub fn start(&self) -> DateTime<Utc> {
        self.start
    }
    pub fn end(&self) -> DateTime<Utc> {
        self.end
    }

    pub fn ranges(&self) -> impl Iterator<Item = (DateTime<Utc>, DateTime<Utc>)> + '_ {
        let mut start = self.start;
        self.part_ends.iter().map(move |end| {
            let range = (start, *end);
            start = *end;
            range
        })
    }

    /// All intervals must already be closed and inside provider M1 depth
    /// before the first request. Splitting old history cannot defeat this.
    pub fn preflight(&self, trusted_now: DateTime<Utc>) -> Result<(), ClosedM1SnapshotError> {
        if self.end > trusted_now || trusted_now - self.start > Duration::days(7) {
            return Err(ClosedM1SnapshotError::Freshness);
        }
        Ok(())
    }

    pub fn new(
        start: DateTime<Utc>,
        end: DateTime<Utc>,
        part_ends: Vec<DateTime<Utc>>,
    ) -> Result<Self, ClosedM1SnapshotError> {
        if !minute_aligned(start)
            || !minute_aligned(end)
            || start >= end
            || part_ends.is_empty()
            || part_ends.len() > MAX_PARTS
            || part_ends.last() != Some(&end)
        {
            return Err(ClosedM1SnapshotError::Range);
        }
        let mut cursor = start;
        for part_end in &part_ends {
            if !minute_aligned(*part_end)
                || *part_end <= cursor
                || *part_end > end
                || *part_end - cursor > Duration::days(7)
            {
                return Err(ClosedM1SnapshotError::Range);
            }
            cursor = *part_end;
        }
        Ok(Self {
            start,
            end,
            part_ends,
        })
    }
}

/// Retained evidence, not an admitted capability. Only a completed guarded
/// transport response may supply completion/length/status in production.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClosedM1RestPartV1 {
    pub method: String,
    pub endpoint: String,
    pub symbol: String,
    pub timeframe: String,
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub requested_at: DateTime<Utc>,
    pub received_at: DateTime<Utc>,
    pub status: u16,
    pub transport_complete: bool,
    pub declared_body_bytes: Option<u64>,
    pub response_sha256: String,
    pub raw_body: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClosedM1SnapshotEvidenceV1 {
    pub policy: String,
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub parts: Vec<ClosedM1RestPartV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClosedM1RangeAccountingV1 {
    pub raw_rows: usize,
    pub in_range_unique_rows: usize,
    pub exact_right_boundary_rows: usize,
    pub equal_duplicate_rows: usize,
}

/// No Deserialize or public fields: caller-supplied bars cannot replace the
/// exact mapping of the raw snapshot after admission.
#[derive(Debug)]
pub struct AdmittedClosedM1Snapshot {
    evidence: ClosedM1SnapshotEvidenceV1,
    sha256: String,
    receipt: ObservedM1Receipt,
    accounting: Vec<ClosedM1RangeAccountingV1>,
}

#[derive(Debug)]
pub struct ObservedM10Bucket {
    bar: Bar,
    actual_m1: Vec<Bar>,
    minute_bitmap: u16,
    snapshot_sha256: String,
}

impl ObservedM10Bucket {
    pub fn bar(&self) -> &Bar {
        &self.bar
    }

    pub fn actual_m1(&self) -> &[Bar] {
        &self.actual_m1
    }

    pub fn minute_bitmap(&self) -> u16 {
        self.minute_bitmap
    }

    pub fn snapshot_sha256(&self) -> &str {
        &self.snapshot_sha256
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ClosedM1SnapshotError {
    #[error("closed snapshot policy or identity mismatch")]
    Identity,
    #[error("closed snapshot range or planned parts are invalid/incomplete")]
    Range,
    #[error("closed snapshot transport did not complete successfully")]
    Transport,
    #[error("closed snapshot body length/hash/decode is invalid")]
    Body,
    #[error("closed snapshot is future, stale, or outside M1 history depth")]
    Freshness,
    #[error("closed snapshot M1 timestamp or OHLCV is invalid")]
    Bar,
    #[error("closed snapshot duplicate or boundary evidence conflicts")]
    Conflict,
    #[error("closed snapshot M10 is empty")]
    EmptyBucket,
    #[error("selected M1 differs from the exact admitted range")]
    SelectionMismatch,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RequiredBarsResponse {
    // Unlike the general DTO, a missing bars field is not an empty snapshot.
    bars: Vec<dto::Bar>,
    symbol: String,
}

fn minute_aligned(t: DateTime<Utc>) -> bool {
    t.timestamp() > 0 && t.timestamp() % 60 == 0 && t.timestamp_subsec_nanos() == 0
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn mapped_m1(raw: &dto::Bar) -> Result<Bar, ClosedM1SnapshotError> {
    let open: DateTime<Utc> = raw
        .timestamp
        .parse()
        .map_err(|_| ClosedM1SnapshotError::Bar)?;
    open.checked_add_signed(Duration::seconds(60))
        .ok_or(ClosedM1SnapshotError::Bar)?;
    let mut b =
        map_bar(STAGE8B_P1F_O2_VENUE_SYMBOL, raw, 60).map_err(|_| ClosedM1SnapshotError::Bar)?;
    b.instrument.market = Market::Futures;
    b.source_kind = MarketDataSourceKind::HistoricalPoll;
    let step = Decimal::new(5, 1);
    if !minute_aligned(b.open_ts)
        || b.low <= Decimal::ZERO
        || b.low > b.high
        || b.low > b.open.min(b.close)
        || b.high < b.open.max(b.close)
        || b.volume < Decimal::ZERO
        || [b.open, b.high, b.low, b.close]
            .iter()
            .any(|p| *p % step != Decimal::ZERO)
    {
        return Err(ClosedM1SnapshotError::Bar);
    }
    Ok(b)
}

impl AdmittedClosedM1Snapshot {
    pub fn admit(
        evidence: ClosedM1SnapshotEvidenceV1,
        plan: &ClosedM1RequestPlan,
        trusted_now: DateTime<Utc>,
    ) -> Result<Self, ClosedM1SnapshotError> {
        if evidence.policy != OBSERVED_M1_POLICY_V1 {
            return Err(ClosedM1SnapshotError::Identity);
        }
        if !minute_aligned(evidence.start)
            || !minute_aligned(evidence.end)
            || evidence.start >= evidence.end
            || evidence.parts.is_empty()
            || evidence.parts.len() > MAX_PARTS
            || evidence.start != plan.start
            || evidence.end != plan.end
            || evidence.parts.iter().map(|p| p.end).collect::<Vec<_>>() != plan.part_ends
        {
            return Err(ClosedM1SnapshotError::Range);
        }
        let mut cursor = evidence.start;
        let mut prior_received = None;
        let mut previous_boundary: Option<dto::Bar> = None;
        let mut mapped = BTreeMap::new();
        let mut accounting = Vec::new();
        let mut retained_bytes = 0usize;
        for part in &evidence.parts {
            if part.method != "GET"
                || part.endpoint != "https://api.finam.ru/v1/instruments/IMOEXF@RTSX/bars"
                || part.symbol != STAGE8B_P1F_O2_VENUE_SYMBOL
                || part.timeframe != "TIME_FRAME_M1"
            {
                return Err(ClosedM1SnapshotError::Identity);
            }
            if part.start != cursor
                || !minute_aligned(part.end)
                || part.start >= part.end
                || part.end > evidence.end
                || part.end - part.start > Duration::days(7)
            {
                return Err(ClosedM1SnapshotError::Range);
            }
            if part.end > part.requested_at
                || part.requested_at > part.received_at
                || part.received_at > trusted_now
                || part.requested_at - part.start > Duration::days(7)
                || trusted_now - part.received_at > Duration::seconds(MAX_SNAPSHOT_AGE_SECONDS)
                || prior_received.is_some_and(|p| part.requested_at < p)
            {
                return Err(ClosedM1SnapshotError::Freshness);
            }
            if part.status != 200 || !part.transport_complete {
                return Err(ClosedM1SnapshotError::Transport);
            }
            retained_bytes = retained_bytes
                .checked_add(part.raw_body.len())
                .filter(|size| *size <= SNAPSHOT_MAX_BODY_BYTES)
                .ok_or(ClosedM1SnapshotError::Body)?;
            if part.raw_body.is_empty()
                || part.raw_body.len() > SNAPSHOT_MAX_BODY_BYTES
                || part
                    .declared_body_bytes
                    .is_some_and(|n| n != part.raw_body.len() as u64)
                || digest(part.raw_body.as_bytes()) != part.response_sha256
            {
                return Err(ClosedM1SnapshotError::Body);
            }
            let response: RequiredBarsResponse =
                serde_json::from_str(&part.raw_body).map_err(|_| ClosedM1SnapshotError::Body)?;
            if response.symbol != STAGE8B_P1F_O2_VENUE_SYMBOL {
                return Err(ClosedM1SnapshotError::Identity);
            }
            let mut unique = BTreeMap::new();
            let mut counts = ClosedM1RangeAccountingV1 {
                raw_rows: response.bars.len(),
                in_range_unique_rows: 0,
                exact_right_boundary_rows: 0,
                equal_duplicate_rows: 0,
            };
            for raw in response.bars {
                let b = mapped_m1(&raw)?;
                if b.open_ts < part.start || b.open_ts > part.end {
                    return Err(ClosedM1SnapshotError::Range);
                }
                if b.open_ts == part.end {
                    counts.exact_right_boundary_rows += 1;
                }
                if let Some((prior, _)) = unique.get(&b.open_ts) {
                    if prior != &raw {
                        return Err(ClosedM1SnapshotError::Conflict);
                    }
                    counts.equal_duplicate_rows += 1;
                } else {
                    unique.insert(b.open_ts, (raw, b));
                }
            }
            if let Some(previous) = previous_boundary.take() {
                if unique.get(&part.start).map(|(raw, _)| raw) != Some(&previous) {
                    return Err(ClosedM1SnapshotError::Conflict);
                }
            }
            previous_boundary = unique.remove(&part.end).map(|(raw, _)| raw);
            counts.in_range_unique_rows = unique.len();
            for (_, (_, b)) in unique {
                if mapped.insert(b.open_ts, b).is_some() {
                    return Err(ClosedM1SnapshotError::Conflict);
                }
            }
            accounting.push(counts);
            cursor = part.end;
            prior_received = Some(part.received_at);
        }
        if cursor != evidence.end {
            return Err(ClosedM1SnapshotError::Range);
        }
        let bytes = serde_json::to_vec(&evidence).map_err(|_| ClosedM1SnapshotError::Body)?;
        let mut bound = OBSERVED_M1_POLICY_V1.as_bytes().to_vec();
        bound.push(0);
        bound.extend_from_slice(&bytes);
        let sha256 = digest(&bound);
        let receipt = ObservedM1Receipt::from_admitted_source(
            sha256.clone(),
            evidence.start,
            evidence.end,
            evidence
                .parts
                .last()
                .ok_or(ClosedM1SnapshotError::Range)?
                .received_at,
            InstrumentId {
                symbol: "IMOEXF".into(),
                venue_symbol: Some(STAGE8B_P1F_O2_VENUE_SYMBOL.into()),
                exchange: Exchange::Moex,
                market: Market::Futures,
            },
            mapped.into_values().collect(),
        )
        .map_err(map_receipt_error)?;
        Ok(Self {
            evidence,
            sha256,
            receipt,
            accounting,
        })
    }

    pub fn evidence(&self) -> &ClosedM1SnapshotEvidenceV1 {
        &self.evidence
    }

    pub fn sha256(&self) -> &str {
        &self.sha256
    }

    pub fn accounting(&self) -> &[ClosedM1RangeAccountingV1] {
        &self.accounting
    }

    pub fn bars(&self) -> impl Iterator<Item = &Bar> {
        self.receipt.bars().iter()
    }

    /// The complete broker-neutral projection was built from ALL admitted raw
    /// rows, not from a caller-supplied selection. Retain/seal this once beside
    /// the raw snapshot, and use its digest as independent canonical context.
    pub fn receipt(&self) -> &ObservedM1Receipt {
        &self.receipt
    }

    /// Candidate uses a calendar-selected nominal close, not a search for an
    /// older dense/available bucket. A missing last M1 cannot delay finality.
    /// The caller must select this close from the accepted session calendar.
    pub fn candidate_at(
        &self,
        nominal_close: DateTime<Utc>,
        trusted_now: DateTime<Utc>,
    ) -> Result<ObservedM10Bucket, ClosedM1SnapshotError> {
        if nominal_close > trusted_now
            || trusted_now - nominal_close > Duration::seconds(MAX_SNAPSHOT_AGE_SECONDS)
            || self
                .evidence
                .parts
                .last()
                .map_or(true, |p| p.received_at > trusted_now)
        {
            return Err(ClosedM1SnapshotError::Freshness);
        }
        let open = nominal_close
            .checked_sub_signed(Duration::seconds(600))
            .ok_or(ClosedM1SnapshotError::Range)?;
        self.bucket(open)
    }

    /// Calendar bounds are supplied explicitly. A missing first/last minute
    /// never moves them, and an empty bucket never advances the stream.
    pub fn bucket(&self, open: DateTime<Utc>) -> Result<ObservedM10Bucket, ClosedM1SnapshotError> {
        let derived = self.receipt.bucket(open).map_err(map_receipt_error)?;
        Ok(ObservedM10Bucket {
            bar: derived.bar,
            actual_m1: derived.actual_m1,
            minute_bitmap: derived.minute_bitmap,
            snapshot_sha256: self.sha256.clone(),
        })
    }

    /// Binds downstream selection to every actual minute in the admitted raw
    /// range. A shortened/repriced vector cannot keep the original binding.
    pub fn verify_selection(
        &self,
        open: DateTime<Utc>,
        selected: &[Bar],
    ) -> Result<(), ClosedM1SnapshotError> {
        if self.bucket(open)?.actual_m1 != selected {
            return Err(ClosedM1SnapshotError::SelectionMismatch);
        }
        Ok(())
    }
}

fn map_receipt_error(error: ObservedM1Error) -> ClosedM1SnapshotError {
    match error {
        ObservedM1Error::Range => ClosedM1SnapshotError::Range,
        ObservedM1Error::EmptyBucket => ClosedM1SnapshotError::EmptyBucket,
        _ => ClosedM1SnapshotError::Bar,
    }
}

#[cfg(test)]
#[path = "sparse_m10_tests.rs"]
mod tests;
