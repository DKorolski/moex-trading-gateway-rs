//! Read-only causal prefix inspection, never reconstruction of native authority.
use super::plan::{hash, sha, INSTRUMENTS};
use anyhow::{ensure, Context, Result};
use broker_finam::ShadowInstrument;
use finam_shadow_models::local::{
    contract::{Candle, Construction, Manifest},
    Journal,
};
use serde_json::{json, Value};
use std::{collections::BTreeMap, path::Path};

fn read(path: &Path, max: u64) -> Result<Vec<u8>> {
    let m = std::fs::symlink_metadata(path)?;
    ensure!(
        m.is_file() && !m.file_type().is_symlink() && m.len() <= max,
        "regular bounded evidence file required"
    );
    Ok(std::fs::read(path)?)
}
pub fn inspect(output: &Path) -> Result<Value> {
    let manifest: Manifest = serde_json::from_slice(&read(
        &output.join("model-manifest.json"),
        24 * 1024 * 1024,
    )?)?;
    let bytes = read(&output.join("source-control.jsonl"), 32 * 1024 * 1024)?;
    ensure!(bytes.ends_with(b"\n"), "partial source-control tail");
    let text = std::str::from_utf8(&bytes)?;
    let mut records = Vec::new();
    let mut head = "0".repeat(64);
    for (n, line) in text.lines().enumerate() {
        ensure!(line.len() <= 2 * 1024 * 1024 && n <= 2048, "control bound");
        let record: Value = serde_json::from_str(line)?;
        let p = &record["payload"];
        ensure!(
            p["index"] == n && p["previous_sha256"] == head && record["sha256"] == hash(p)?,
            "control sequence/hash chain"
        );
        head = record["sha256"].as_str().context("record hash")?.into();
        records.push(p.clone());
    }
    let prep = &records.first().context("missing preparation")?["data"];
    ensure!(
        records[0]["kind"] == "preparation"
            && prep["schema"] == "finam.shadow.usd-ri.preparation.v1"
            && prep["implementation_sha256"] == super::implementation()
            && prep["scope"] == "source_component_only"
            && prep["network_entry_enabled"] == false
            && prep["operational_activation"] == false
            && prep["orders"] == false,
        "source component preparation"
    );
    let plan: super::Plan = serde_json::from_value(prep["plan"].clone())?;
    plan.validate(&manifest)?;
    let configs: BTreeMap<ShadowInstrument, String> =
        serde_json::from_value(prep["source_configs"].clone())?;
    let identities: BTreeMap<ShadowInstrument, super::IdentityReceipt> =
        serde_json::from_value(prep["identities"].clone())?;
    ensure!(
        configs.len() == 2 && identities.len() == 2,
        "two distinct identities/configs"
    );
    let calendars = prep["calendars"].as_array().context("calendars")?;
    ensure!(calendars.len() == 2, "two calendars");
    let mut admitted_calendars = BTreeMap::new();
    for i in INSTRUMENTS {
        let identity = identities.get(&i).context("identity")?;
        ensure!(
            identity.instrument == i
                && identity.account_sha256 == plan.account_sha256
                && sha(&identity.asset_response_sha256)
                && sha(&identity.params_response_sha256)
                && identity.public_asset["id"] == i.asset_id()
                && identity.public_asset["ticker"] == i.symbol()
                && identity.public_asset["tick"] == i.tick().to_string()
                && super::plan::decimal(&identity.public_asset["reference_lot_size"])?
                    == super::plan::frozen_size(i)?
                && super::plan::decimal(&identity.public_asset["contract_size"])?
                    == super::plan::frozen_size(i)?
                && identity.public_params
                    == json!({"symbol":i.venue_symbol(),"trade_lot_size":"1","is_tradable":true}),
            "public identity projection"
        );
        let c = calendars
            .iter()
            .filter(|c| c["instrument"] == serde_json::to_value(i).unwrap())
            .collect::<Vec<_>>();
        ensure!(c.len() == 1, "independent unique calendar");
        let e = &c[0]["evidence"];
        let raw: crate::primary::calendar::RawCalendar = serde_json::from_str(
            e["raw_calendar_raw_json"]
                .as_str()
                .context("raw calendar")?,
        )?;
        let binding: crate::primary::calendar::CalendarBinding =
            serde_json::from_value(e["binding"].clone())?;
        let model: crate::primary::calendar::ModelDay =
            serde_json::from_str(e["model_day_raw_json"].as_str().context("model day")?)?;
        ensure!(
            model.session_date == manifest.session_date.to_string()
                && binding.next_label_ms == manifest.freeze_at_ms
                && binding.start_not_before_ms == plan.start_not_before_ms
                && binding.start_before_ms == plan.start_before_ms,
            "calendar belongs to exact manifest/entry window"
        );
        let mapping = e["mapping_raw_json"].as_str().context("mapping JSON")?;
        let admitted_at = e["admitted_at_ms"]
            .as_i64()
            .context("calendar admission clock")?;
        for n in 0..2 {
            ensure!(
                identity.requested_at_ms[n] >= 0
                    && identity.received_at_ms[n] >= identity.requested_at_ms[n]
                    && identity.received_at_ms[n] - identity.requested_at_ms[n] <= 5000
                    && admitted_at
                        .checked_sub(identity.received_at_ms[n])
                        .is_some_and(|age| (0..=300000).contains(&age)),
                "metadata original request/receive/admission clocks"
            );
        }
        let calendar = crate::primary::calendar::admit_additional(
            i,
            binding.clone(),
            model,
            raw.clone(),
            mapping.as_bytes(),
            admitted_at,
        )
        .map_err(|e| anyhow::anyhow!("calendar verification: {e:?}"))?;
        ensure!(
            e["calendar_sha256"] == calendar.sha256(),
            "calendar exact inputs"
        );
        ensure!(
            configs[&i]
                == hash(&(
                    &plan,
                    identity,
                    &raw,
                    binding.bootstrap_reference_start_ms,
                    super::plan::policy(),
                    super::implementation()
                ))?
                && binding.config_sha256 == configs[&i],
            "exact source configuration"
        );
        admitted_calendars.insert(i, calendar);
    }
    let db = output.join("models.sqlite");
    let metadata = std::fs::symlink_metadata(&db)?;
    ensure!(
        metadata.is_file()
            && !metadata.file_type().is_symlink()
            && metadata.len() <= 512 * 1024 * 1024,
        "SQL evidence bound"
    );
    let exported = Journal::inspect(&db, manifest.clone())?;
    let sql = exported["inputs"].as_array().context("SQL inputs")?;
    let mut intent: Option<Candle> = None;
    let mut matched = 0usize;
    let mut counts: BTreeMap<ShadowInstrument, u64> = INSTRUMENTS.map(|i| (i, 0)).into();
    let mut bound = std::collections::BTreeSet::new();
    let mut event_clock = plan.start_not_before_ms;
    let mut finish = None;
    for (n, p) in records.iter().enumerate().skip(1) {
        ensure!(finish.is_none(), "records after terminal snapshot");
        let data = &p["data"];
        match p["kind"].as_str().context("control kind")? {
            "subscription" => {
                let i: ShadowInstrument = serde_json::from_value(data["instrument"].clone())?;
                ensure!(
                    INSTRUMENTS.contains(&i)
                        && bound.insert(i)
                        && data["generation"] == 1
                        && data["bound_at_ms"].as_i64().is_some_and(|t| t
                            >= plan.start_not_before_ms
                            && t < plan.start_before_ms),
                    "source subscription"
                );
                let at = data["bound_at_ms"].as_i64().context("subscription clock")?;
                ensure!(at >= event_clock, "source event chronology");
                event_clock = at;
            }
            "reference" => {
                let i: ShadowInstrument = serde_json::from_value(data["instrument"].clone())?;
                ensure!(bound.contains(&i) && intent.is_none(), "reference source");
                let evidence: broker_finam::sparse_m10::ClosedM1SnapshotEvidenceV1 =
                    serde_json::from_value(data["evidence"].clone())
                        .context("reference evidence decode")?;
                let recorded = data["recorded_at_ms"]
                    .as_i64()
                    .context("reference recorded_at_ms missing")?;
                ensure!(
                    recorded >= event_clock && recorded < plan.stop_at_ms,
                    "reference event chronology"
                );
                check_reference(
                    i,
                    &evidence,
                    data["sha256"].as_str().context("reference hash")?,
                    admitted_calendars
                        .get(&i)
                        .context("reference instrument calendar")?,
                    recorded,
                )?;
                event_clock = recorded;
            }
            "publication_intent" => {
                ensure!(
                    intent.is_none() && bound.len() == 2,
                    "one serialized publication"
                );
                let c: Candle = serde_json::from_value(data["candle"].clone())?;
                let i = INSTRUMENTS
                    .into_iter()
                    .find(|i| i.symbol() == c.instrument)
                    .context("foreign SQL input")?;
                ensure!(
                    data["instrument"] == serde_json::to_value(i)?
                        && c.start_ms == manifest.freeze_at_ms + counts[&i] as i64 * 600000,
                    "per-source prefix"
                );
                check_projection(&c, i, &configs[&i])?;
                ensure!(
                    c.source_processed_ms >= event_clock,
                    "publication chronology"
                );
                event_clock = c.source_processed_ms;
                intent = Some(c);
            }
            "checked_readback" => {
                let c = intent.take().context("readback without publication")?;
                let i = INSTRUMENTS
                    .into_iter()
                    .find(|i| i.symbol() == c.instrument)
                    .context("instrument")?;
                let row = sql.get(matched).context("SQL prefix missing")?;
                let input_sha = c.sha256()?;
                let mut expected_ids: Vec<_> = exported["results"]
                    .as_array()
                    .context("SQL outcomes")?
                    .iter()
                    .filter(|o| o["input_sha256"] == input_sha)
                    .map(|o| o["result_id"].as_str().context("SQL result ID"))
                    .collect::<Result<_>>()?;
                let mut actual_ids: Vec<_> = data["result_ids"]
                    .as_array()
                    .context("readback result IDs")?
                    .iter()
                    .map(|v| v.as_str().context("result ID"))
                    .collect::<Result<_>>()?;
                expected_ids.sort();
                actual_ids.sort();
                ensure!(
                    serde_json::from_value::<Candle>(row["candle"].clone())? == c
                        && data["instrument"] == serde_json::to_value(i)?
                        && data["input_sha256"] == c.sha256()?
                        && data["source_sequence"] == c.source_sequence
                        && data["revision"] == row["revision"]
                        && expected_ids.len() == 2
                        && expected_ids == actual_ids,
                    "exact SQL checked-readback linkage"
                );
                matched += 1;
                *counts.get_mut(&i).unwrap() += 1;
            }
            "finish" => {
                ensure!(n + 1 == records.len(), "finish last");
                finish = Some(data);
            }
            _ => anyhow::bail!("unknown control event"),
        }
    }
    let terminal = finish
        .context("incomplete component: no final frontier witness; inspect SQL without resuming")?;
    let held = terminal["held"].as_bool().context("held flag")?;
    ensure!(
        terminal["scope"] == "source_component_only"
            && terminal["workers_joined"] == false
            && terminal["observed_clean_session"] == false
            && terminal["real_orders"] == false,
        "truthful source-only finish"
    );
    ensure!(
        matched <= sql.len() && sql.len() <= matched + usize::from(intent.is_some()),
        "SQL/control prefix count"
    );
    if sql.len() > matched {
        ensure!(
            serde_json::from_value::<Candle>(sql[matched]["candle"].clone())?
                == *intent.as_ref().context("unmatched SQL intent")?,
            "held SQL must match retained publication intent"
        );
    }
    let frontiers = terminal["frontiers"]
        .as_array()
        .context("final frontiers")?;
    ensure!(frontiers.len() == 2, "two frontiers");
    for i in INSTRUMENTS {
        let rows = frontiers
            .iter()
            .filter(|v| v["instrument"] == serde_json::to_value(i).unwrap())
            .collect::<Vec<_>>();
        ensure!(rows.len() == 1, "unique source frontier");
        let committed = rows[0]["committed"].as_u64().context("committed count")?;
        ensure!(
            committed <= counts[&i] && committed + u64::from(held) >= counts[&i],
            "source/SQL frontier mismatch"
        );
        let next = if committed == 101 {
            Value::Null
        } else {
            json!(manifest.freeze_at_ms + committed as i64 * 600000)
        };
        ensure!(rows[0]["next_label_ms"] == next, "source frontier label");
        if !held {
            ensure!(
                committed == counts[&i] && intent.is_none() && matched == sql.len(),
                "source-last completion required"
            );
        }
    }
    ensure!(
        exported["results"].as_array().context("results")?.len() == sql.len() * 2,
        "two outcomes per input"
    );
    Ok(
        json!({"status":if held{"HELD_COMPONENT_EVIDENCE"}else{"COMPONENT_PREFIX_VERIFIED"},"inputs":sql.len(),"results":sql.len()*2,
        "source_frontiers":frontiers,"control_head_sha256":head,"scope":"source_component_only",
        "metadata_evidence":"validated public projections and original hashes; private full bodies not exported",
        "native_authority_reconstructed":false,"operational_activation":false,"observed_clean_session":false,"real_orders":false}),
    )
}

/// File-only revalidation of full retained HTTP evidence. It does NOT create
/// a canonical publication capability, physical subscription, or WS authority.
pub(super) fn check_reference(
    instrument: ShadowInstrument,
    evidence: &broker_finam::sparse_m10::ClosedM1SnapshotEvidenceV1,
    expected_sha: &str,
    calendar: &crate::primary::calendar::AdmittedCalendar,
    recorded_at_ms: i64,
) -> Result<()> {
    use broker_finam::sparse_m10::{AdmittedClosedM1Snapshot, ClosedM1RequestPlan};
    ensure!(
        INSTRUMENTS.contains(&instrument) && sha(expected_sha),
        "reference identity/hash shape"
    );
    ensure!(
        evidence.start.timestamp_millis() >= calendar.reference_start_ms()
            && evidence.end.timestamp_millis() <= calendar.required()[0],
        "reference calendar range"
    );
    let now = chrono::DateTime::from_timestamp_millis(recorded_at_ms).context("reference clock")?;
    let request = ClosedM1RequestPlan::new(
        evidence.start,
        evidence.end,
        evidence.parts.iter().map(|p| p.end).collect(),
    )
    .map_err(|e| anyhow::anyhow!("reference request plan: {e:?}"))?;
    request
        .preflight(now)
        .map_err(|e| anyhow::anyhow!("reference request chronology: {e:?}"))?;
    let source = AdmittedClosedM1Snapshot::admit_for(instrument, evidence.clone(), &request, now)
        .map_err(|e| anyhow::anyhow!("reference snapshot admission: {e:?}"))?;
    ensure!(
        source.instrument() == instrument && source.sha256() == expected_sha,
        "reference snapshot hash mismatch"
    );
    Ok(())
}
fn check_projection(c: &Candle, i: ShadowInstrument, config: &str) -> Result<()> {
    let Construction::DeclaredCanonicalProjectionV1 {
        provenance,
        provenance_sha256,
    } = &c.construction
    else {
        anyhow::bail!("native projection required")
    };
    let p = provenance;
    let r = &p["canonical_receipt"];
    ensure!(
        hash(p)? == *provenance_sha256
            && p["schema"] == "finam.shadow.usd-ri.native-projection.v1"
            && p["implementation_sha256"] == super::implementation()
            && p["observed_clean_session"] == false
            && p["order_emission_allowed"] == false
            && p["clock_basis"] == "declared_numerical_availability_not_physical_durability"
            && r["config_sha256"] == config
            && r["policy_sha256"]
                == super::plan::policy()
                    .fingerprint()
                    .map_err(|e| anyhow::anyhow!("{e:?}"))?
            && serde_json::from_str::<Value>(
                p["source_receipt_json"]
                    .as_str()
                    .context("original source receipt JSON")?
            )? == *r
            && p["source_receipt_sha256"]
                == crate::seed::digest(
                    p["source_receipt_json"]
                        .as_str()
                        .context("source receipt JSON")?
                        .as_bytes()
                ),
        "native source provenance"
    );
    let bars: Vec<broker_core::event::Bar> = serde_json::from_value(r["actual_m1"].clone())?;
    ensure!(
        r["generation"] == 1
            && r["canonical_ready_at_ms"] == c.durable_ms
            && c.source_processed_ms == c.durable_ms
            && c.durable_ms - c.end_ms + 100 <= 2000,
        "canonical source clock/generation budget"
    );
    if p["rest_evidence"].is_null() {
        ensure!(
            r["origin"] == "WS_LIVE"
                && r["rest_snapshot_sha256"].is_null()
                && r["proof_received_at_ms"].is_null()
                && r["boundary_received_at_ms"] == c.received_ms,
            "WS origin/receipt"
        );
    } else {
        ensure!(
            r["origin"] == "REST_EXCEPTION" && r["proof_received_at_ms"] == c.received_ms,
            "mandatory REST origin/receipt"
        );
        let e: broker_finam::sparse_m10::ClosedM1SnapshotEvidenceV1 =
            serde_json::from_value(p["rest_evidence"].clone())?;
        let plan = broker_finam::sparse_m10::ClosedM1RequestPlan::new(
            e.start,
            e.end,
            e.parts.iter().map(|p| p.end).collect(),
        )
        .map_err(|e| anyhow::anyhow!("REST plan: {e:?}"))?;
        // Offline revalidation of retained HTTP bytes, not physical socket
        // ownership and not minting an AdmittedCanonicalM10 for publication.
        let source = broker_finam::sparse_m10::AdmittedClosedM1Snapshot::admit_for(
            i,
            e,
            &plan,
            chrono::DateTime::from_timestamp_millis(c.durable_ms).context("REST clock")?,
        )
        .map_err(|e| anyhow::anyhow!("REST evidence: {e:?}"))?;
        ensure!(
            r["rest_snapshot_sha256"] == source.sha256(),
            "exact retained REST source hash"
        );
        source
            .verify_selection(
                chrono::DateTime::from_timestamp_millis(c.start_ms).context("REST label")?,
                &bars,
            )
            .map_err(|e| anyhow::anyhow!("REST selected M1: {e:?}"))?;
    }
    let b = broker_core::canonical_m1::aggregate_m1_window(
        &i.instrument(),
        chrono::DateTime::from_timestamp_millis(c.start_ms).context("M10 label")?,
        &bars,
    )
    .map_err(|e| anyhow::anyhow!("M1: {e:?}"))?;
    ensure!(
        r["label_ms"] == c.start_ms
            && r["close_ms"] == c.end_ms
            && r["minute_bitmap"] == b.minute_bitmap
            && c.data.open == b.provisional_bar.open.to_string()
            && c.data.high == b.provisional_bar.high.to_string()
            && c.data.low == b.provisional_bar.low.to_string()
            && c.data.close == b.provisional_bar.close.to_string()
            && c.data.volume == b.provisional_bar.volume.to_string(),
        "exact M1 canonical aggregation"
    );
    Ok(())
}
