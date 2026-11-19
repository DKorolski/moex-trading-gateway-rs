//! Rebuild the declared numerical seed ONLY from complete retained GET evidence.
//! Snapshot clocks are original; availability is not a physical fsync claim.
use super::INSTRUMENTS;
use super::*;
use crate::primary::calendar::RawCalendar;
use broker_core::canonical_m1::aggregate_m1_window;
use broker_finam::sparse_m10::{AdmittedClosedM1Snapshot, ClosedM1RequestPlan};
use finam_shadow_models::local::contract::*;
use std::collections::BTreeSet;

// Raw evidence is public only after an exact nested-schema check. The shared
// broker DTO intentionally tolerates extensions; this export boundary does not.
fn public_body(raw: &str) -> Result<()> {
    let body = crate::primary::json::parse(raw)?;
    let object = body.as_object().context("public M1 body")?;
    ensure!(
        object.len() == 2 && object.contains_key("symbol") && object.contains_key("bars"),
        "public M1 schema"
    );
    for row in body["bars"].as_array().context("public M1 rows")? {
        let fields = row.as_object().context("public M1 row")?;
        ensure!(
            fields.len() == 6 && row["timestamp"].is_string(),
            "public M1 row schema"
        );
        for k in ["open", "high", "low", "close", "volume"] {
            let number = row[k].as_object().context("public M1 decimal")?;
            ensure!(
                number.len() == 1 && number.get("value").is_some_and(Value::is_string),
                "public M1 decimal schema"
            );
        }
    }
    Ok(())
}
fn covers(
    ranges: &[(chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>)],
    start: i64,
    end: i64,
) -> bool {
    let mut cursor = start;
    for (a, b) in ranges {
        let a = a.timestamp_millis();
        let b = b.timestamp_millis();
        if b <= cursor {
            continue;
        }
        if a > cursor {
            return false;
        }
        cursor = b;
        if cursor >= end {
            return true;
        }
    }
    false
}
fn at(ms: i64) -> Result<chrono::DateTime<chrono::Utc>> {
    chrono::DateTime::from_timestamp_millis(ms).context("history clock")
}
pub(super) fn build(
    generation: &str,
    day: NaiveDate,
    inputs: &[&HistoryInput],
    calendars: &[(ShadowInstrument, &RawCalendar)],
    now: i64,
) -> Result<Manifest> {
    let freeze = (day.and_hms_opt(7, 0, 0).context("session cutoff")? - chrono::Duration::hours(3))
        .and_utc()
        .timestamp_millis();
    ensure!(
        inputs.len() == 2
            && INSTRUMENTS
                .iter()
                .all(|i| inputs.iter().filter(|s| s.instrument == *i).count() == 1),
        "two history bindings"
    );
    ensure!(
        calendars.len() == 2
            && INSTRUMENTS
                .iter()
                .all(|i| calendars.iter().filter(|(j, _)| j == i).count() == 1),
        "two history calendars"
    );
    let catalog_raw =
        include_bytes!("../../../../../../docs/stage-8/shadow-models-finam-mapping-20261113.json");
    ensure!(
        crate::seed::digest(catalog_raw) == crate::primary::models_bridge::MAPPING_SHA256,
        "catalog pin"
    );
    let catalog: Value = serde_json::from_slice(catalog_raw)?;
    let mut seeds = Vec::new();
    for row in catalog["bindings"].as_array().context("catalog")? {
        let symbol = row["model_instrument"].as_str().context("catalog symbol")?;
        let binding = Binding {
            instrument: symbol.into(),
            provider_id: row["provider_symbol"].as_str().context("provider")?.into(),
            catalog_sha256: crate::primary::models_bridge::MAPPING_SHA256.into(),
            basis: MappingBasis::DeclaredCatalogSnapshot,
            tick: row["price_tick"].as_str().context("tick")?.into(),
            lot: 1,
            construction: ConstructionKind::ExternalM10V1,
        };
        let mut history = Vec::new();
        if symbol != "IMOEXF" {
            let input = inputs
                .iter()
                .find(|s| s.instrument.symbol() == symbol)
                .context("history source")?;
            ensure!(
                !input.required_labels_ms.is_empty()
                    && input.required_labels_ms.len() <= 1000
                    && !input.snapshots.is_empty()
                    && input.snapshots.len() <= 16,
                "history/inventory bound"
            );
            let mut sources = Vec::new();
            let mut ranges = Vec::new();
            for item in &input.snapshots {
                let e = &item.evidence;
                for part in &e.parts {
                    public_body(&part.raw_body)?;
                }
                ensure!(
                    item.admitted_at_ms <= now
                        && item.admitted_at_ms <= freeze
                        && e.end.timestamp_millis() < freeze,
                    "future seed receipt/admission/range"
                );
                let p = ClosedM1RequestPlan::new(
                    e.start,
                    e.end,
                    e.parts.iter().map(|p| p.end).collect(),
                )
                .map_err(|e| anyhow::anyhow!("{e:?}"))?;
                p.preflight(at(item.admitted_at_ms)?)
                    .map_err(|e| anyhow::anyhow!("{e:?}"))?;
                let source = AdmittedClosedM1Snapshot::admit_for(
                    input.instrument,
                    e.clone(),
                    &p,
                    at(item.admitted_at_ms)?,
                )
                .map_err(|e| anyhow::anyhow!("{e:?}"))?;
                ranges.push((e.start, e.end));
                sources.push((source, item.admitted_at_ms));
            }
            ranges.sort();
            ensure!(
                ranges.windows(2).all(|r| r[0].1 <= r[1].0),
                "overlapping seed snapshots"
            );
            let first = ranges.first().context("history range")?.0;
            let day_start = (first + chrono::Duration::hours(3))
                .date_naive()
                .and_hms_opt(0, 0, 0)
                .context("history date")?
                .and_utc()
                .timestamp_millis()
                - 10800000;
            let calendar = calendars
                .iter()
                .find(|(i, _)| *i == input.instrument)
                .context("history calendar")?
                .1;
            let windows = crate::primary::calendar::history_windows(
                input.instrument,
                calendar,
                day_start,
                freeze,
                now,
            )
            .map_err(|e| anyhow::anyhow!("history calendar: {e:?}"))?;
            ensure!(
                windows.iter().all(|(a, b)| covers(&ranges, *a, *b)),
                "unqueried calendar trading interval"
            );
            let mut actual = BTreeSet::new();
            for (source, _) in &sources {
                for bar in source.bars() {
                    let t = bar.open_ts.timestamp_millis();
                    ensure!(
                        windows.iter().any(|(a, b)| *a <= t && t + 60000 <= *b),
                        "M1 contradicts historical schedule"
                    );
                    actual.insert(t.div_euclid(600000) * 600000);
                }
            }
            ensure!(
                input
                    .required_labels_ms
                    .iter()
                    .copied()
                    .eq(actual.iter().copied()),
                "exact all observed nonempty M10 inventory"
            );
            let mut previous = None;
            for (n, &label) in input.required_labels_ms.iter().enumerate() {
                ensure!(
                    label > 0
                        && label % 600000 == 0
                        && label.checked_add(600000).is_some_and(|end| end < freeze)
                        && previous.map_or(true, |old| label > old),
                    "history inventory order/grid"
                );
                previous = Some(label);
                let start = at(label)?;
                let end = at(label + 600000)?;
                let (source, processed) = sources
                    .iter()
                    .find(|(s, _)| s.evidence().start <= start && s.evidence().end >= end)
                    .context("uncovered context M10")?;
                let rows: Vec<_> = source
                    .bars()
                    .filter(|b| {
                        b.open_ts.timestamp_millis() >= label
                            && b.open_ts.timestamp_millis() < label + 600000
                    })
                    .cloned()
                    .collect();
                let aggregate =
                    aggregate_m1_window(&input.instrument.instrument(), at(label)?, &rows)
                        .map_err(|e| anyhow::anyhow!("{e:?}"))?;
                let b = aggregate.provisional_bar;
                let received = source
                    .evidence()
                    .parts
                    .iter()
                    .map(|p| p.received_at.timestamp_millis())
                    .max()
                    .context("seed received")?;
                history.push(Candle {
                    instrument: symbol.into(),
                    source_sequence: n as u64 + 1,
                    provider_id: binding.provider_id.clone(),
                    binding_sha256: binding.sha256()?,
                    start_ms: label,
                    end_ms: label + 600000,
                    received_ms: received,
                    durable_ms: *processed,
                    source_processed_ms: *processed,
                    purpose: Purpose::ContextHistory,
                    data: Ohlcv {
                        open: b.open.to_string(),
                        high: b.high.to_string(),
                        low: b.low.to_string(),
                        close: b.close.to_string(),
                        volume: b.volume.to_string(),
                    },
                    construction: Construction::ExternalM10V1 {
                        raw_sha256: source.sha256().into(),
                    },
                });
            }
        }
        seeds.push(Seed {
            binding,
            history,
            declared_consumed_through_ms: freeze,
        });
    }
    let m = Manifest {
        schema: finam_shadow_models::local::VERSION.into(),
        generation: generation.into(),
        implementation_sha256: finam_shadow_models::local::implementation_sha256()?,
        evidence: Evidence::DeclaredClockReplay,
        initial_context: InitialContext::UsdRiFirstSession,
        session_date: day,
        freeze_at_ms: freeze,
        seeds,
    };
    m.validate()?;
    Ok(m)
}
