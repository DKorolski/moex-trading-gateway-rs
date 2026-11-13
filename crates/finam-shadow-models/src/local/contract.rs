//! Fixed-grid M10 wire contract. Original millisecond clocks are retained.
//! All source attestations in this local slice are DECLARED, never verified.
use super::hash;
use crate::{BarEvent, DataOrigin};
use anyhow::{ensure, Context, Result};
use chrono::{Datelike, Duration, NaiveDate};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, str::FromStr};

pub const MAX_MANIFEST_BYTES: usize = 24 * 1024 * 1024;
pub const MAX_EVENT_BYTES: usize = 64 * 1024;
pub const INSTRUMENTS: [&str; 3] = ["IMOEXF", "USDRUBF", "RIZ6"];

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Evidence {
    DeclaredClockReplay,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MappingBasis {
    FixtureOnly,
    DeclaredCatalogSnapshot,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConstructionKind {
    ExternalM10V1,
    SparseM1V1,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Binding {
    pub instrument: String,
    pub provider_id: String,
    pub catalog_sha256: String,
    pub basis: MappingBasis,
    pub tick: String,
    pub lot: u32,
    pub construction: ConstructionKind,
}
impl Binding {
    pub fn sha256(&self) -> Result<String> {
        hash(self)
    }
    fn validate(&self) -> Result<()> {
        ensure!(
            INSTRUMENTS.contains(&self.instrument.as_str()),
            "unknown model instrument"
        );
        ensure!(
            !self.provider_id.trim().is_empty() && self.provider_id.len() <= 256,
            "provider ID"
        );
        digest(&self.catalog_sha256)?;
        ensure!(
            decimal(&self.tick)? > Decimal::ZERO && self.lot == 1,
            "tick/lot binding"
        );
        if self.basis == MappingBasis::FixtureOnly {
            ensure!(
                self.provider_id.starts_with("fixture:"),
                "fixture must not impersonate catalog"
            );
        } else if self.instrument == "IMOEXF" {
            ensure!(
                self.provider_id == "IMOEXF@RTSX" && decimal(&self.tick)? == Decimal::new(5, 1),
                "known IMOEXF mapping mismatch"
            );
        }
        // USDRUBF / RIZ6 values are declarations only until independently
        // checked against FINAM catalog; this type grants no operational use.
        Ok(())
    }
}
pub(crate) fn digest(s: &str) -> Result<()> {
    ensure!(
        s.len() == 64
            && s.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        "invalid SHA-256"
    );
    Ok(())
}
fn decimal(s: &str) -> Result<Decimal> {
    ensure!(!s.is_empty() && s.len() <= 48, "decimal bound");
    let d = Decimal::from_str(s).context("invalid exact decimal")?;
    ensure!(d.scale() <= 9, "decimal scale bound");
    Ok(d)
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Ohlcv {
    pub open: String,
    pub high: String,
    pub low: String,
    pub close: String,
    pub volume: String,
}
impl Ohlcv {
    fn values(&self, tick: Decimal) -> Result<[Decimal; 5]> {
        let v = [
            decimal(&self.open)?,
            decimal(&self.high)?,
            decimal(&self.low)?,
            decimal(&self.close)?,
            decimal(&self.volume)?,
        ];
        ensure!(
            v[0] > Decimal::ZERO
                && v[2] > Decimal::ZERO
                && v[4] >= Decimal::ZERO
                && v[1] >= v[0]
                && v[1] >= v[3]
                && v[2] <= v[0]
                && v[2] <= v[3],
            "invalid OHLCV"
        );
        ensure!(
            v[..4]
                .iter()
                .all(|p| p.checked_rem(tick) == Some(Decimal::ZERO)),
            "off-tick OHLC"
        );
        Ok(v)
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Minute {
    pub start_ms: i64,
    pub data: Ohlcv,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Construction {
    ExternalM10V1 {
        raw_sha256: String,
    },
    SparseM1V1 {
        raw_sha256: String,
        range_from_ms: i64,
        range_to_ms: i64,
        declared_complete_response: bool,
        actual_m1: Vec<Minute>,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Purpose {
    ContextHistory,
    ConfirmedClosingContext,
    Model,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Candle {
    pub instrument: String,
    pub source_sequence: u64,
    pub provider_id: String,
    pub binding_sha256: String,
    pub start_ms: i64,
    pub end_ms: i64,
    pub received_ms: i64,
    pub durable_ms: i64,
    pub source_processed_ms: i64,
    pub purpose: Purpose,
    pub data: Ohlcv,
    pub construction: Construction,
}
impl Candle {
    pub fn sha256(&self) -> Result<String> {
        hash(self)
    }
    pub(crate) fn validate(&self, binding: &Binding) -> Result<()> {
        ensure!(
            serde_json::to_vec(self)?.len() <= MAX_EVENT_BYTES,
            "event size bound"
        );
        ensure!(
            self.instrument == binding.instrument
                && self.provider_id == binding.provider_id
                && self.binding_sha256 == binding.sha256()?,
            "input mapping mismatch"
        );
        ensure!(
            self.source_sequence > 0 && self.source_sequence <= 10_000,
            "source sequence bound"
        );
        ensure!(
            self.start_ms > 0
                && self.start_ms % 600_000 == 0
                && self.start_ms.checked_add(600_000) == Some(self.end_ms)
                && self.end_ms <= self.received_ms
                && self.received_ms <= self.durable_ms
                && self.durable_ms <= self.source_processed_ms,
            "M10 grid/availability clocks"
        );
        ensure!(
            chrono::DateTime::from_timestamp_millis(self.source_processed_ms).is_some(),
            "clock range"
        );
        let tick = decimal(&binding.tick)?;
        let values = self.data.values(tick)?;
        match (&binding.construction, &self.construction) {
            (ConstructionKind::ExternalM10V1, Construction::ExternalM10V1 { raw_sha256 }) => {
                digest(raw_sha256)?
            }
            (
                ConstructionKind::SparseM1V1,
                Construction::SparseM1V1 {
                    raw_sha256,
                    range_from_ms,
                    range_to_ms,
                    declared_complete_response,
                    actual_m1,
                },
            ) => {
                digest(raw_sha256)?;
                ensure!(
                    *declared_complete_response
                        && *range_from_ms == self.start_ms
                        && *range_to_ms == self.end_ms
                        && !actual_m1.is_empty()
                        && actual_m1.len() <= 10,
                    "empty/truncated/untrusted declared M1 range"
                );
                let mut aggregate = None;
                let mut previous = None;
                for m in actual_m1 {
                    ensure!(
                        m.start_ms >= self.start_ms
                            && m.start_ms < self.end_ms
                            && m.start_ms % 60_000 == 0
                            && previous.map_or(true, |p| p < m.start_ms),
                        "duplicate/unordered/out-of-range M1"
                    );
                    let v = m.data.values(tick)?;
                    let a: &mut [Decimal; 5] = aggregate.get_or_insert(v);
                    a[1] = a[1].max(v[1]);
                    a[2] = a[2].min(v[2]);
                    a[3] = v[3];
                    if previous.is_some() {
                        a[4] = a[4].checked_add(v[4]).context("volume overflow")?;
                    }
                    previous = Some(m.start_ms);
                }
                ensure!(
                    aggregate == Some(values),
                    "M1/M10 exact aggregation mismatch"
                );
            }
            _ => anyhow::bail!("construction version changed"),
        }
        Ok(())
    }
    pub(crate) fn numerical_bar(&self, origin: DataOrigin) -> Result<BarEvent> {
        let f = |s: &str| -> Result<f64> {
            let v = s.parse::<f64>()?;
            ensure!(v.is_finite(), "numeric overflow");
            Ok(v)
        };
        Ok(BarEvent {
            symbol: self.instrument.clone(),
            close_time_utc: self.start_ms / 1000,
            o: f(&self.data.open)?,
            h: f(&self.data.high)?,
            l: f(&self.data.low)?,
            close: f(&self.data.close)?,
            v: f(&self.data.volume)?,
            origin,
        })
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Seed {
    pub binding: Binding,
    /// Original receipt clocks, NOT shifted to session cutoff.
    pub history: Vec<Candle>,
    /// Declared upstream watermark. This is not minted from the last bar.
    pub declared_consumed_through_ms: i64,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub schema: String,
    pub generation: String,
    pub implementation_sha256: String,
    pub evidence: Evidence,
    pub session_date: NaiveDate,
    pub freeze_at_ms: i64,
    pub seeds: Vec<Seed>,
}
pub(crate) fn cutoff(day: NaiveDate) -> i64 {
    (day.and_hms_opt(7, 0, 0).unwrap() - Duration::hours(3))
        .and_utc()
        .timestamp_millis()
}
impl Manifest {
    pub fn sha256(&self) -> Result<String> {
        hash(self)
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(
            serde_json::to_vec(self)?.len() <= MAX_MANIFEST_BYTES,
            "manifest size bound"
        );
        ensure!(
            self.schema == super::VERSION
                && self.implementation_sha256 == super::implementation_sha256()?,
            "generation schema/implementation mismatch"
        );
        ensure!(
            !self.generation.trim().is_empty() && self.generation.len() <= 128,
            "generation bound"
        );
        ensure!(
            self.session_date >= NaiveDate::from_ymd_opt(2026, 7, 14).unwrap()
                && self.session_date <= NaiveDate::from_ymd_opt(2026, 12, 17).unwrap()
                && self.session_date.weekday().num_days_from_monday() < 5
                && self.session_date != NaiveDate::from_ymd_opt(2026, 11, 4).unwrap(),
            "outside pinned single-session calendar/RI contract"
        );
        ensure!(
            self.freeze_at_ms == cutoff(self.session_date),
            "local replay freeze must use exact cutoff"
        );
        ensure!(self.seeds.len() == 3, "all three mappings required");
        let mut symbols = BTreeSet::new();
        for seed in &self.seeds {
            seed.binding.validate()?;
            ensure!(
                symbols.insert(&seed.binding.instrument),
                "duplicate instrument binding"
            );
            ensure!(
                seed.declared_consumed_through_ms == self.freeze_at_ms,
                "missing declared cutoff watermark"
            );
            ensure!(seed.history.len() <= 1000, "seed bound");
            let mut processed = 0;
            for (i, candle) in seed.history.iter().enumerate() {
                candle.validate(&seed.binding)?;
                ensure!(
                    candle.source_sequence == i as u64 + 1
                        && candle.purpose != Purpose::Model
                        && candle.end_ms < self.freeze_at_ms
                        && candle.source_processed_ms <= self.freeze_at_ms
                        && candle.source_processed_ms >= processed,
                    "seed causal prefix/cutoff"
                );
                processed = candle.source_processed_ms;
            }
        }
        Ok(())
    }
}
