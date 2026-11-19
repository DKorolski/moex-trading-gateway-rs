//! Rebuild the declared numerical seed ONLY from complete retained GET evidence.
//! Snapshot clocks are original; availability is not a physical fsync claim.
use super::INSTRUMENTS;
use super::*;
use broker_core::canonical_m1::aggregate_m1_window;
use broker_finam::sparse_m10::{AdmittedClosedM1Snapshot, ClosedM1RequestPlan};
use finam_shadow_models::local::contract::*;
fn at(ms: i64) -> Result<chrono::DateTime<chrono::Utc>> {
    chrono::DateTime::from_timestamp_millis(ms).context("history clock")
}
pub(super) fn build(
    generation: &str,
    day: NaiveDate,
    inputs: &[&HistoryInput],
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
