//! Explicit USD/RI executable glue. File preparation is not runtime admission.
//! No restart, checkpoint input, URL override or operational permission.
use super::{
    plan::{hash, INSTRUMENTS},
    InstrumentEvidence, Plan, Prepared,
};
use crate::{auth::AuthLifecycle, transport::StopHandle};
use anyhow::{ensure, Context, Result};
use broker_finam::ShadowInstrument;
use chrono::NaiveDate;
use finam_shadow_models::local::contract::Manifest;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{path::Path, sync::Arc};
mod artifacts;
mod seed;
#[cfg(test)]
mod tests;

pub const MAX_INPUT_BYTES: usize = 24 * 1024 * 1024;
/// Exact operator-selected account and original raw HTTP data. Never print or
/// retain this whole private draft in public execution/handoff artifacts.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Draft {
    pub schema: String,
    pub account_id: String,
    pub generation: String,
    pub session_date: NaiveDate,
    pub start_not_before_ms: i64,
    pub start_before_ms: i64,
    pub stop_at_ms: i64,
    pub sources: Vec<SourceInput>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceInput {
    pub instrument: ShadowInstrument,
    pub asset: crate::primary::calendar::ScheduleSnapshot,
    pub params: crate::primary::calendar::ScheduleSnapshot,
    pub calendar: crate::primary::calendar::RawCalendar,
    pub bootstrap_reference_start_ms: i64,
    pub history: HistoryInput,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HistoryInput {
    pub instrument: ShadowInstrument,
    pub required_labels_ms: Vec<i64>,
    pub snapshots: Vec<HistorySnapshot>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HistorySnapshot {
    pub admitted_at_ms: i64,
    pub evidence: broker_finam::sparse_m10::ClosedM1SnapshotEvidenceV1,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Frozen {
    schema: String,
    draft: Draft,
    plan: Plan,
    manifest: Manifest,
}
fn parse<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T> {
    ensure!(bytes.len() <= MAX_INPUT_BYTES, "USD/RI input bound");
    let v = crate::primary::json::parse(std::str::from_utf8(bytes)?)?;
    Ok(serde_json::from_value(v)?)
}
fn account_valid(account: &str) -> bool {
    !account.is_empty()
        && account.len() <= 128
        && account
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b'.'))
}
impl Draft {
    fn validate(&self) -> Result<()> {
        ensure!(
            self.schema == "finam.shadow.usd-ri.input-draft.v1" && account_valid(&self.account_id),
            "draft/account binding"
        );
        ensure!(
            self.sources.len() == 2
                && INSTRUMENTS.iter().all(|i| self
                    .sources
                    .iter()
                    .filter(|s| s.instrument == *i && s.history.instrument == *i)
                    .count()
                    == 1),
            "exact USD and RIZ6 inputs"
        );
        Ok(())
    }
    fn public_history(&self) -> Result<Value> {
        Ok(
            json!({"schema":"finam.shadow.usd-ri.seed-evidence.v1","sources":INSTRUMENTS.iter().map(|i|&self.sources.iter().find(|s|s.instrument==*i).unwrap().history).collect::<Vec<_>>()}),
        )
    }
    fn pin(&self, now: i64) -> Result<(Plan, Manifest)> {
        self.validate()?;
        let history = self.sources.iter().map(|s| &s.history).collect::<Vec<_>>();
        let calendars = self
            .sources
            .iter()
            .map(|s| (s.instrument, &s.calendar))
            .collect::<Vec<_>>();
        let manifest = seed::build(
            &self.generation,
            self.session_date,
            &history,
            &calendars,
            now,
        )?;
        let plan = Plan {
            schema: "finam.shadow.usd-ri.source-component.v1".into(),
            manifest_sha256: manifest.sha256()?,
            account_sha256: crate::seed::digest(self.account_id.as_bytes()),
            start_not_before_ms: self.start_not_before_ms,
            start_before_ms: self.start_before_ms,
            stop_at_ms: self.stop_at_ms,
        };
        plan.validate(&manifest)?;
        Ok((plan, manifest))
    }
}
/// Pin raw inputs and derived history/plan only. Does NOT bypass or perform
/// fresh metadata/calendar/runtime admission; that is repeated by admit().
pub fn freeze(bytes: &[u8]) -> Result<Vec<u8>> {
    let draft: Draft = parse(bytes)?;
    let (plan, manifest) = draft.pin(crate::timing::now_ms())?;
    let out = serde_json::to_vec_pretty(&Frozen {
        schema: "finam.shadow.usd-ri.executable-input.v1".into(),
        draft,
        plan,
        manifest,
    })?;
    ensure!(out.len() <= MAX_INPUT_BYTES, "frozen input bound");
    Ok(out)
}
pub fn freeze_file(input: &Path, output: &Path) -> Result<Value> {
    let bytes = artifacts::read(input, MAX_INPUT_BYTES as u64)?;
    let frozen = freeze(&bytes)?;
    artifacts::write_new(output, &frozen)?;
    let parent = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    std::fs::File::open(parent)?.sync_all()?;
    Ok(
        json!({"status":"INPUTS_FROZEN_ONLY","input_sha256":crate::seed::digest(&frozen),
        "fresh_admission_performed":false,"operational_authority":false,"orders":false}),
    )
}
pub struct Admitted {
    source: Prepared,
    account: String,
    input_sha256: String,
    history: Value,
}
/// The pin comes from an independently retained prepare receipt, not from
/// recomputing a hash of the current, possibly replaced input file.
pub fn admit(bytes: &[u8], expected_sha256: &str) -> Result<Admitted> {
    ensure!(
        super::plan::sha(expected_sha256) && crate::seed::digest(bytes) == expected_sha256,
        "independent exact frozen input pin"
    );
    let frozen: Frozen = parse(bytes)?;
    ensure!(
        frozen.schema == "finam.shadow.usd-ri.executable-input.v1",
        "frozen schema"
    );
    let (plan, manifest) = frozen.draft.pin(crate::timing::now_ms())?;
    ensure!(
        serde_json::to_value(&plan)? == serde_json::to_value(&frozen.plan)?
            && manifest == frozen.manifest,
        "exact derived manifest/plan pins"
    );
    let evidence = frozen
        .draft
        .sources
        .iter()
        .map(|s| InstrumentEvidence {
            instrument: s.instrument,
            asset: s.asset.clone(),
            params: s.params.clone(),
            calendar: s.calendar.clone(),
            bootstrap_reference_start_ms: s.bootstrap_reference_start_ms,
        })
        .collect();
    let source = Prepared::admit(plan, manifest, evidence)?;
    let history = frozen.draft.public_history()?;
    Ok(Admitted {
        source,
        account: frozen.draft.account_id,
        input_sha256: crate::seed::digest(bytes),
        history,
    })
}
pub fn admit_file(path: &Path, expected_sha256: &str) -> Result<Admitted> {
    admit(
        &artifacts::read(path, MAX_INPUT_BYTES as u64)?,
        expected_sha256,
    )
}
impl Admitted {
    pub fn account_id(&self) -> &str {
        &self.account
    }
    pub fn receipt(&self) -> Result<Value> {
        let source = self.source.receipt()?;
        let now = crate::timing::now_ms();
        let admitted_at = source["calendars"][0]["evidence"]["admitted_at_ms"]
            .as_i64()
            .context("admission clock")?;
        Ok(
            json!({"schema":"finam.shadow.usd-ri.executable-admission.v1","input_sha256":self.input_sha256,
            "account_sha256":self.source.plan.account_sha256,"manifest_sha256":self.source.manifest.sha256()?,
            "history_sha256":hash(&self.history)?,"source_admission":source,"admitted_at_ms":admitted_at,
            "entry_window_open_now":now>=self.source.plan.start_not_before_ms&&now<self.source.plan.start_before_ms,
            "source_only":true,"operational_authority":false,"orders":false}),
        )
    }
    fn open(self, output: &Path, stop: StopHandle) -> Result<Opened> {
        self.source.identities_fresh(crate::timing::now_ms())?;
        let receipt = self.receipt()?;
        ensure!(
            receipt["entry_window_open_now"] == true,
            "actual executable entry window"
        );
        let managed = self.source.into_managed(output, stop)?;
        artifacts::write_new(&output.join("entry.json"), &serde_json::to_vec(&receipt)?)?;
        artifacts::write_new(
            &output.join("entry-history.json"),
            &serde_json::to_vec(&self.history)?,
        )?;
        std::fs::File::open(output)?.sync_all()?;
        Ok(Opened {
            managed,
            output: output.into(),
        })
    }
}
struct Opened {
    managed: super::managed::Managed,
    output: std::path::PathBuf,
}
/// No custom transport factory in production. Both fixed selectors share the
/// same exact-account AuthLifecycle and existing V8 managed implementation.
pub async fn run(
    admitted: Admitted,
    auth: Arc<AuthLifecycle>,
    output: &Path,
    stop: StopHandle,
) -> Result<Value> {
    ensure!(
        auth.binds(admitted.account_id()),
        "executable auth account binding"
    );
    let clients = INSTRUMENTS
        .map(|i| {
            broker_finam::Stage8bP1fO2GetOnlyClientV1::for_shadow_instrument(
                admitted.account.clone(),
                i,
            )
            .map(Arc::new)
        })
        .into_iter()
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let clients = clients
        .try_into()
        .map_err(|_| anyhow::anyhow!("two clients"))?;
    let opened = admitted.open(output, stop.clone())?;
    let result = opened.managed.run(auth, clients).await;
    artifacts::finish(&opened.output, result)
}
pub fn inspect(output: &Path) -> Result<Value> {
    artifacts::inspect(output)
}
