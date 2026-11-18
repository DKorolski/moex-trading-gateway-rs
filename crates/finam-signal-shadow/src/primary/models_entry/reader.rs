//! Matching aggregate reader. No auth, sockets, restart capability or DB writes.
use super::*;
use finam_shadow_models::local::{contract::Manifest, Journal};
use std::collections::{BTreeMap, BTreeSet};
mod post_join;
mod publication;

#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Completion {
    pub schema: String,
    pub source_config_sha256: String,
    pub model_plan_sha256: String,
    pub manifest_sha256: String,
    pub control_journal_sha256: String,
    pub sqlite_sha256: String,
    pub post_join_report_sha256: String,
    pub status: String,
    pub coherent: bool,
    pub source_committed: u64,
    pub control_committed: usize,
    pub sql_revision: u64,
    pub states: Vec<Value>,
    pub observed_clean_session: bool,
    pub full_session_qualified: bool,
    pub network_qualified: bool,
    pub cgroup_qualified: bool,
    pub parity_accepted: bool,
    pub broker_execution: bool,
}
pub(crate) fn evaluate(
    config: &WsPrimaryConfig,
    plan: &ModelPlan,
    manifest: &Manifest,
    journal: &[u8],
    sqlite: &Path,
    report: &Value,
    now: i64,
) -> Result<Completion> {
    plan.validate(config)?;
    let sql_bytes = artifacts::read(sqlite, 64 * 1024 * 1024)?;
    let sql = Journal::inspect(sqlite, manifest.clone())?;
    ensure!(
        digest(&sql_bytes) == digest(&artifacts::read(sqlite, 64 * 1024 * 1024)?),
        "database changed during inspection"
    );
    let text = std::str::from_utf8(journal)?;
    ensure!(
        journal.len() as u64 <= config.base.max_output_bytes.min(64 * 1024 * 1024)
            && text.ends_with('\n'),
        "control journal bound/torn tail"
    );
    let rows: Vec<Value> = text
        .lines()
        .map(|l| parse(l.as_bytes(), 1048576))
        .collect::<Result<_>>()?;
    ensure!(rows.len() <= 16385, "control row count");
    let events = |name: &str| -> Vec<&Value> {
        rows.iter()
            .filter_map(|r| (r["event"]["event"] == name).then_some(&r["event"]))
            .collect()
    };
    let bounds = events("primary_runtime_cohort_bound");
    ensure!(
        bounds.len() == 1
            && bounds[0]["model_arms"] == 2
            && bounds[0]["runtime_count"] == 3
            && bounds[0]["model_instrument"] == "IMOEXF"
            && bounds[0]["manifest_sha256"] == manifest.sha256()?
            && bounds[0]["source_config_sha256"] == plan.source_config_sha256,
        "cohort binding"
    );
    let seeds = events("primary_models_seed_binding");
    ensure!(
        seeds.len() == 1
            && seeds[0]["binding"]["model_plan_sha256"] == hash(plan)?
            && seeds[0]["binding"]["manifest_sha256"] == manifest.sha256()?
            && seeds[0]["binding"]["arms"] == serde_json::to_value(&plan.arms)?,
        "production seed binding"
    );
    let frontier = events("primary_models_final_frontier");
    ensure!(
        frontier.len() == 1 && frontier[0]["writer_join_attested"] == false,
        "final writer frontier"
    );
    let source = frontier[0]["source_committed"]
        .as_u64()
        .context("source count")?;
    let control = rows
        .iter()
        .filter(|r| r["kind"] == "canonical_commit")
        .count();
    let required: Vec<_> = (0..101)
        .map(|i| config.base.observation_start_utc_ms + i * 600000)
        .collect();
    let scan =
        super::super::restart::journal::inspect_models(config, journal, &required, control, now)
            .map_err(|_| anyhow::anyhow!("native control verification"))?;
    ensure!(
        scan.terminal_reason.is_some(),
        "unfinalized control journal"
    );
    let inputs = sql["inputs"].as_array().context("SQL inputs")?;
    let results = sql["results"].as_array().context("SQL results")?;
    let revision = sql["revision"].as_u64().context("SQL revision")?;
    ensure!(
        revision as usize == inputs.len() && results.len() == 2 * inputs.len(),
        "SQL arm cardinality"
    );
    let expected_arms: BTreeSet<_> = ARMS.into_iter().collect();
    let mut sources = BTreeMap::new();
    for row in &rows {
        if row["kind"] == "canonical_source" {
            let e = &row["event"];
            sources.insert(e["source_sha256"].as_str().context("source SHA")?, e);
        }
    }
    let history_len = manifest
        .seeds
        .iter()
        .find(|s| s.binding.instrument == "IMOEXF")
        .context("seed")?
        .history
        .len();
    let commits = events("primary_imoexf_models_commit");
    let advances = events("primary_models_source_advanced");
    // Missing native evidence/arm/outcome is corruption, not a bounded success.
    ensure!(
        control == inputs.len() && commits.len() == inputs.len(),
        "control/SQL commits disagree"
    );
    publication::validate(&rows, &required, inputs.len(), advances.len(), now)?;
    for (i, (row, native)) in inputs.iter().zip(&scan.inputs).enumerate() {
        let candle = &row["candle"];
        let provenance = &candle["construction"]["provenance"];
        ensure!(
            provenance["schema"] == super::super::models_bridge::SCHEMA
                && provenance["bridge_implementation_sha256"]
                    == super::super::models_bridge::implementation()
                && provenance["mapping_sha256"] == super::super::models_bridge::MAPPING_SHA256,
            "bridge/source implementation binding"
        );
        let sequence = history_len + i + 1;
        let label = required[i];
        ensure!(
            candle["instrument"] == "IMOEXF"
                && candle["source_sequence"] == sequence
                && candle["start_ms"] == label
                && candle["end_ms"] == label + 600000
                && provenance["source_receipt_sha256"] == native.source_sha256,
            "native/model input linkage"
        );
        let e = sources
            .get(native.source_sha256.as_str())
            .context("native source receipt absent")?;
        let original = provenance["source_receipt_json"]
            .as_str()
            .context("source receipt JSON")?;
        ensure!(
            digest(original.as_bytes()) == native.source_sha256
                && parse::<Value>(original.as_bytes(), 65536)? == e["receipt"]
                && provenance["source_receipt"] == e["receipt"],
            "native receipt hash/content"
        );
        let canonical =
            if native.origin == "RECOVERY_BACKFILL" || native.origin == "WS_RECOVERY_BACKFILL" {
                // serde(flatten) means the full typed recovery receipt contains
                // canonical fields at its top level, not an invented nested object.
                provenance["canonical_receipt"].clone()
            } else {
                e["receipt"].clone()
            };
        let canonical_json = provenance["canonical_receipt_json"]
            .as_str()
            .context("canonical receipt JSON")?;
        ensure!(
            parse::<Value>(canonical_json.as_bytes(), 65536)? == canonical
                && digest(canonical_json.as_bytes()) == provenance["canonical_receipt_sha256"],
            "canonical digest"
        );
        for (key, value) in canonical.as_object().context("canonical object")? {
            ensure!(
                e["receipt"].get(key) == Some(value),
                "canonical/source receipt differs"
            );
        }
        let received = canonical["proof_received_at_ms"]
            .as_i64()
            .or_else(|| canonical["boundary_received_at_ms"].as_i64());
        ensure!(
            received.is_some()
                && candle["received_ms"].as_i64() == received
                && candle["durable_ms"] == canonical["canonical_ready_at_ms"]
                && candle["source_processed_ms"] == canonical["canonical_ready_at_ms"],
            "model availability provenance"
        );
        ensure!(
            canonical["actual_m1"] == serde_json::to_value(&native.actual_m1)?
                && canonical["label_ms"] == label,
            "canonical M1 lineage"
        );
        let aggregated = broker_core::canonical_m1::aggregate_m1_window(
            &native.bar.instrument,
            chrono::DateTime::from_timestamp_millis(label).context("label")?,
            &native.actual_m1,
        )
        .map_err(|_| anyhow::anyhow!("canonical aggregation"))?
        .provisional_bar;
        ensure!(
            candle["data"]
                == json!({"open":aggregated.open.to_string(),"high":aggregated.high.to_string(),
            "low":aggregated.low.to_string(),"close":aggregated.close.to_string(),"volume":aggregated.volume.to_string()}),
            "SQL OHLCV differs"
        );
        let outcomes: Vec<_> = results
            .iter()
            .filter(|r| r["source_sequence"] == sequence)
            .collect();
        let arms: BTreeSet<_> = outcomes.iter().filter_map(|r| r["arm"].as_str()).collect();
        ensure!(
            outcomes.len() == 2 && arms == expected_arms,
            "missing/foreign model arm"
        );
        let ids: BTreeSet<_> = outcomes
            .iter()
            .filter_map(|r| r["result_id"].as_str())
            .collect();
        let commit = commits[i];
        let claimed: BTreeSet<_> = commit["commit"]["result_ids"]
            .as_array()
            .context("result IDs")?
            .iter()
            .filter_map(|v| v.as_str())
            .collect();
        ensure!(
            commit["label_ms"] == label
                && commit["source_sha256"] == native.source_sha256
                && commit["readback_verified"] == true
                && commit["commit"]["revision"] == i + 1
                && ids.len() == 2
                && ids == claimed,
            "SQL durable commit receipt"
        );
        if let Some(rest) = provenance["rest_evidence"]["snapshot_sha256"].as_str() {
            let retained = scan
                .sources
                .get(rest)
                .context("referenced REST snapshot absent")?;
            ensure!(
                provenance["rest_evidence"]["evidence_sha256"] == hash(retained.evidence())?,
                "referenced REST evidence differs"
            );
        }
    }
    let coherent = source == revision
        && advances.len() == inputs.len()
        && frontier[0]["held"] == false
        && frontier[0]["next_label_ms"]
            == if source == 101 {
                Value::Null
            } else {
                json!(config.base.observation_start_utc_ms + source as i64 * 600000)
            };
    for (i, advance) in advances.iter().enumerate() {
        ensure!(i < scan.inputs.len(), "excess source advancement");
        ensure!(
            advance["label_ms"] == required[i]
                && advance["source_committed"] == i + 1
                && advance["source_sha256"] == scan.inputs[i].source_sha256,
            "source advance lineage"
        );
        let label = required[i];
        let stamp = advance["source_advanced_at_ms"]
            .as_i64()
            .context("source publication clock")?;
        let receipt = &sources[scan.inputs[i].source_sha256.as_str()]["receipt"];
        let ready = receipt["canonical_ready_at_ms"]
            .as_i64()
            .context("ready clock")?;
        ensure!(
            stamp >= ready && stamp <= now && stamp < config.base.run_deadline_utc_ms,
            "source advancement clock"
        );
        if let Some(deadline) = receipt["replay_deadline_ms"].as_i64() {
            ensure!(
                stamp <= deadline
                    && receipt["episode_deadline_ms"]
                        .as_i64()
                        .map_or(true, |d| stamp < d),
                "historical deadline"
            );
        } else {
            ensure!(
                stamp - label - 600000 + config.base.clock_uncertainty_ms as i64 <= 2000,
                "live publication budget"
            );
        }
    }
    let states: Vec<_> = sql["states"]
        .as_array()
        .context("states")?
        .iter()
        .filter(|s| s["arm"].as_str().is_some_and(|id| ARMS.contains(&id)))
        .cloned()
        .collect();
    ensure!(
        states.len() == 2
            && states
                .iter()
                .all(|s| s["ready"] == true && s["bars_processed"] == revision),
        "model final states"
    );
    let (r, accounted) = post_join::validate(config, &rows, report, now)?;
    let done = &r.completion;
    let drained = r.drained(source) && accounted;
    let mut status = "PARTIAL";
    if coherent && drained && source > 0 {
        match plan.stop {
            StopMode::Bounded { stop_at_ms }
                if r.cause == "RequestedStop"
                    && done.stop_started_at_ms >= stop_at_ms
                    && done.joined_at_ms
                        <= stop_at_ms
                            + config.base.stop_timeout_ms.min(15000) as i64
                            + config.base.clock_uncertainty_ms as i64 =>
            {
                status = "BOUNDED_COMPLETE"
            }
            StopMode::ModelEod
                if source == 101 && states.iter().all(|s| s["full_grid_flat"] == true) =>
            {
                let completion = serde_json::to_vec(done)?;
                let binding = super::super::verify::EvidenceBinding {
                    schema_version: 1,
                    config_sha256: plan.source_config_sha256.clone(),
                    calendar_sha256: done.calendar_sha256.clone(),
                    journal_sha256: digest(journal),
                    completion_sha256: digest(&completion),
                };
                super::super::verify::verify_models_control(
                    config,
                    &binding,
                    journal,
                    &completion,
                    now,
                )
                .map_err(|_| anyhow::anyhow!("cohort control EOD verification"))?;
                status = "MODEL_EOD";
            }
            _ => (),
        }
    }
    Ok(Completion {
        schema: "finam.imoexf.cohort-completion.v1".into(),
        source_config_sha256: plan.source_config_sha256.clone(),
        model_plan_sha256: hash(plan)?,
        manifest_sha256: manifest.sha256()?,
        control_journal_sha256: digest(journal),
        sqlite_sha256: digest(&sql_bytes),
        post_join_report_sha256: hash(report)?,
        status: status.into(),
        coherent,
        source_committed: source,
        control_committed: control,
        sql_revision: revision,
        states,
        observed_clean_session: false,
        full_session_qualified: false,
        network_qualified: false,
        cgroup_qualified: false,
        parity_accepted: false,
        broker_execution: false,
    })
}
pub fn verify_directory(config: &WsPrimaryConfig, output: &Path) -> Result<Completion> {
    let plan: ModelPlan = parse(
        &artifacts::read(&output.join("models-plan.json"), 16384)?,
        16384,
    )?;
    let history = artifacts::read(&output.join("models-history.json"), 16 * 1024 * 1024)?;
    let calendar = artifacts::read(&output.join("models-context-calendar.json"), 1024 * 1024)?;
    let manifest = seed::build(config, &plan, &history, &calendar)?;
    let supplied: Manifest = parse(
        &artifacts::read(&output.join("models-manifest.json"), 24 * 1024 * 1024)?,
        24 * 1024 * 1024,
    )?;
    ensure!(manifest == supplied, "derived manifest differs");
    let report: Value = parse(
        &artifacts::read(&output.join("models-post-join-report.json"), 1024 * 1024)?,
        1024 * 1024,
    )?;
    let ordinary_report: Value = parse(
        &artifacts::read(&output.join("run-report.json"), 1024 * 1024)?,
        1024 * 1024,
    )?;
    ensure!(
        report == ordinary_report,
        "post-join report copies disagree"
    );
    let journal = artifacts::read(
        &output.join("journal.jsonl"),
        config.base.max_output_bytes.min(64 * 1024 * 1024),
    )?;
    let result = evaluate(
        config,
        &plan,
        &manifest,
        &journal,
        &output.join("models.sqlite"),
        &report,
        crate::timing::now_ms(),
    )?;
    let admission: Value = parse(
        &artifacts::read(&output.join("models-admission.json"), 16384)?,
        16384,
    )?;
    let entry: Value = parse(&artifacts::read(&output.join("entry.json"), 16384)?, 16384)?;
    ensure!(admission["entry"] == entry, "entry/model admission differs");
    let bindings: Vec<Value> = std::str::from_utf8(&journal)?
        .lines()
        .map(|l| parse::<Value>(l.as_bytes(), 1048576))
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        .filter(|r| r["event"]["event"] == "primary_models_seed_binding")
        .collect();
    ensure!(
        bindings.len() == 1 && bindings[0]["event"]["binding"] == admission,
        "persisted model admission differs"
    );
    let claimed: Completion = parse(
        &artifacts::read(&output.join("models-completion.json"), 4 * 1024 * 1024)?,
        4 * 1024 * 1024,
    )?;
    ensure!(
        claimed == result,
        "completion differs from recomputed cohort"
    );
    artifacts::verify_manifest(output)?;
    Ok(result)
}
