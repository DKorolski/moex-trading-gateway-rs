//! Deterministic manifest from source-admitted raw M1 and pinned inventory.
use super::*;
use broker_core::canonical_m1::aggregate_m1_window;
use broker_finam::sparse_m10::{
    AdmittedClosedM1Snapshot, ClosedM1RequestPlan, ClosedM1SnapshotEvidenceV1,
};
use chrono::{DateTime, Utc};
use finam_shadow_models::local::contract::*;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HistoryBundle {
    pub schema_version: u32,
    pub snapshots: Vec<HistorySnapshot>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HistorySnapshot {
    pub admitted_at_ms: i64,
    pub evidence: ClosedM1SnapshotEvidenceV1,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContextCalendar {
    pub schema_version: u32,
    pub required_labels_ms: Vec<i64>,
}
fn at(ms: i64) -> Result<DateTime<Utc>> {
    DateTime::from_timestamp_millis(ms).context("history clock")
}
pub fn build(
    c: &WsPrimaryConfig,
    p: &ModelPlan,
    history: &[u8],
    calendar: &[u8],
) -> Result<Manifest> {
    p.validate(c)?;
    ensure!(
        digest(history) == p.history_sha256 && digest(calendar) == p.context_calendar_sha256,
        "model inputs hash"
    );
    let inventory: ContextCalendar = parse(calendar, 1024 * 1024)?;
    ensure!(
        inventory.schema_version == 1
            && !inventory.required_labels_ms.is_empty()
            && inventory.required_labels_ms.len() <= 1000,
        "context inventory bound"
    );
    let freeze = c.base.observation_start_utc_ms;
    let mut previous = None;
    for &label in &inventory.required_labels_ms {
        ensure!(
            label > 0
                && label % 600000 == 0
                && label.checked_add(600000).is_some_and(|end| end < freeze)
                && previous.map_or(true, |old| label > old),
            "context inventory grid/order/cutoff"
        );
        previous = Some(label);
    }
    let bundle: HistoryBundle = parse(
        history,
        c.base.max_input_bytes.min(16 * 1024 * 1024) as usize,
    )?;
    ensure!(
        bundle.schema_version == 1 && !bundle.snapshots.is_empty() && bundle.snapshots.len() <= 16,
        "history bundle bound"
    );
    let mut admitted = Vec::new();
    let mut ranges = BTreeSet::new();
    let mut original = BTreeMap::new();
    for item in bundle.snapshots {
        ensure!(
            item.admitted_at_ms <= freeze && item.admitted_at_ms <= crate::timing::now_ms(),
            "future seed admission"
        );
        let e = &item.evidence;
        ensure!(
            ranges.insert((e.start, e.end)) && e.end.timestamp_millis() < freeze,
            "duplicate/future history range"
        );
        let request = ClosedM1RequestPlan::new(e.start, e.end, vec![e.end])
            .map_err(|_| anyhow::anyhow!("history range"))?;
        let source =
            AdmittedClosedM1Snapshot::admit(item.evidence, &request, at(item.admitted_at_ms)?)
                .map_err(|_| anyhow::anyhow!("history source admission"))?;
        for bar in source.bars() {
            ensure!(
                original.insert(bar.open_ts, bar.clone()).is_none(),
                "overlapping history sources"
            );
        }
        admitted.push((source, item.admitted_at_ms));
    }
    let catalog_raw =
        include_bytes!("../../../../../docs/stage-8/shadow-models-finam-mapping-20261113.json");
    ensure!(
        digest(catalog_raw) == super::super::models_bridge::MAPPING_SHA256,
        "catalog identity"
    );
    let catalog: Value = serde_json::from_slice(catalog_raw)?;
    let mut seeds = Vec::new();
    for row in catalog["bindings"].as_array().context("catalog bindings")? {
        let symbol = row["model_instrument"].as_str().context("instrument")?;
        let binding = Binding {
            instrument: symbol.into(),
            provider_id: row["provider_symbol"].as_str().context("provider")?.into(),
            catalog_sha256: super::super::models_bridge::MAPPING_SHA256.into(),
            basis: MappingBasis::DeclaredCatalogSnapshot,
            tick: row["price_tick"].as_str().context("tick")?.into(),
            lot: 1,
            construction: ConstructionKind::ExternalM10V1,
        };
        let mut candles = Vec::new();
        if symbol == "IMOEXF" {
            for (i, &label) in inventory.required_labels_ms.iter().enumerate() {
                let (source, processed) = admitted
                    .iter()
                    .find(|(s, _)| {
                        s.evidence().start <= at(label).unwrap()
                            && s.evidence().end >= at(label + 600000).unwrap()
                    })
                    .context("uncovered context M10")?;
                let rows: Vec<_> = source
                    .bars()
                    .filter(|b| {
                        b.open_ts.timestamp_millis() >= label
                            && b.open_ts.timestamp_millis() < label + 600000
                    })
                    .cloned()
                    .collect();
                let aggregated =
                    aggregate_m1_window(&super::super::entry::instrument(), at(label)?, &rows)
                        .map_err(|_| anyhow::anyhow!("context aggregation"))?;
                let received = source
                    .evidence()
                    .parts
                    .iter()
                    .map(|s| s.received_at.timestamp_millis())
                    .max()
                    .context("network receipt")?;
                let b = aggregated.provisional_bar;
                candles.push(Candle {
                    instrument: symbol.into(),
                    source_sequence: i as u64 + 1,
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
            history: candles,
            declared_consumed_through_ms: freeze,
        });
    }
    let manifest = Manifest {
        schema: finam_shadow_models::local::VERSION.into(),
        generation: p.generation.clone(),
        implementation_sha256: finam_shadow_models::local::implementation_sha256()?,
        evidence: Evidence::DeclaredClockReplay,
        session_date: (at(freeze)? + chrono::Duration::hours(3)).date_naive(),
        freeze_at_ms: freeze,
        seeds,
    };
    manifest.validate()?;
    Ok(manifest)
}
pub(crate) fn require_ready(manifest: &Manifest) -> Result<()> {
    // Bootstrap is in-memory: no output/database or model callbacks are created.
    let seed = manifest
        .seeds
        .iter()
        .find(|s| s.binding.instrument == "IMOEXF")
        .context("IMOEXF seed")?;
    let profile: Value = serde_json::from_slice(include_bytes!(
        "../../../../finam-shadow-models/profiles/imoexf.json"
    ))?;
    let config: finam_shadow_models::session::Config =
        serde_json::from_value(profile["config"].clone())?;
    for policy in [
        finam_shadow_models::context::Policy::WeekdayOnly,
        finam_shadow_models::context::Policy::WeekendContextNoTrades,
    ] {
        let identity = finam_shadow_models::context::Identity {
            profile: finam_shadow_models::context::Profile::ImoexfReaction07,
            symbol: "IMOEXF".into(),
            model_config_sha256: config.fingerprint()?,
            receipt_stream: format!("{}/IMOEXF/declared-clock", manifest.generation),
            policy,
            excluded_dates: BTreeSet::from([chrono::NaiveDate::from_ymd_opt(2026, 11, 4).unwrap()]),
        };
        let mut context = finam_shadow_models::context::Store::new(identity.clone())?;
        // Reuse the accepted numerical DTO conversion through serde, not a new kernel.
        for candle in &seed.history {
            let v = &candle.data;
            let number = |s: &str| s.parse::<f64>().context("numerical history");
            context.ingest(
                finam_shadow_models::context::Receipt {
                    sequence: candle.source_sequence,
                    source_id: candle.sha256()?,
                    bar: finam_shadow_models::BarEvent {
                        symbol: "IMOEXF".into(),
                        close_time_utc: candle.start_ms / 1000,
                        o: number(&v.open)?,
                        h: number(&v.high)?,
                        l: number(&v.low)?,
                        close: number(&v.close)?,
                        v: number(&v.volume)?,
                        origin: finam_shadow_models::DataOrigin::History,
                    },
                    received_at_utc: Some(candle.received_ms / 1000),
                    durable_at_utc: Some(candle.durable_ms / 1000),
                },
                candle.source_processed_ms / 1000,
            )?;
        }
        context.freeze(
            manifest.session_date,
            manifest.freeze_at_ms / 1000,
            &finam_shadow_models::context::Barrier {
                receipt_stream: identity.receipt_stream,
                through_utc: manifest.freeze_at_ms / 1000,
                last_sequence: context.last_sequence(),
                ingress_sha256: context.ingress_sha256()?,
                attested_at_utc: manifest.freeze_at_ms / 1000,
            },
        )?;
        ensure!(
            context
                .frozen(manifest.session_date)
                .is_some_and(|f| f.ready()),
            "both model contexts must be ready"
        );
    }
    Ok(())
}
