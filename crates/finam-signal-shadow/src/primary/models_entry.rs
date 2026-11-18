//! Explicit two-policy IMOEXF entry and file-only aggregate inspection.
//! No restart, model changes, order transport or operational acceptance.
use super::{entry, DeliveryPolicy, WsPrimaryConfig};
use crate::{seed::digest, transport::StopHandle};
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
pub(crate) mod artifacts;
pub mod reader;
pub mod seed;
#[cfg(test)]
pub(crate) mod tests;

pub const ARMS: [&str; 2] = ["IMOEXF/weekday_only", "IMOEXF/weekend_context_no_trades"];
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelPlan {
    pub schema_version: u32,
    pub source_config_sha256: String,
    pub generation: String,
    pub arms: [String; 2],
    pub history_sha256: String,
    pub context_calendar_sha256: String,
    pub stop: StopMode,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum StopMode {
    Bounded { stop_at_ms: i64 },
    ModelEod,
}
impl ModelPlan {
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        parse(bytes, 16384)
    }
    pub fn validate(&self, c: &WsPrimaryConfig) -> Result<()> {
        ensure!(
            self.schema_version == 1
                && self.source_config_sha256
                    == c.fingerprint().map_err(|_| anyhow::anyhow!("config"))?
                && self.arms == ARMS
                && c.primary.delivery == DeliveryPolicy::ObservedBoWsHistoricalRecoveryV8
                && c.base.previous_segment_id.is_none(),
            "model plan/source binding"
        );
        ensure!(
            !self.generation.is_empty() && self.generation.len() <= 128,
            "generation"
        );
        if let StopMode::Bounded { stop_at_ms } = self.stop {
            ensure!(
                stop_at_ms >= c.base.observation_start_utc_ms + 600000 + 10000
                    && stop_at_ms < c.base.run_deadline_utc_ms,
                "bounded stop range"
            );
        }
        Ok(())
    }
}
pub(crate) fn parse<T: serde::de::DeserializeOwned>(bytes: &[u8], limit: usize) -> Result<T> {
    ensure!(bytes.len() <= limit, "input bound");
    Ok(serde_json::from_value(super::json::parse(
        std::str::from_utf8(bytes)?,
    )?)?)
}
pub(crate) fn hash(v: &impl Serialize) -> Result<String> {
    Ok(digest(&serde_json::to_vec(v)?))
}

pub struct PreparedModels {
    entry: entry::PreparedEntry,
    pub(crate) config: WsPrimaryConfig,
    pub(crate) plan: ModelPlan,
    pub(crate) manifest: finam_shadow_models::local::contract::Manifest,
    pub(crate) history: Vec<u8>,
    pub(crate) calendar: Vec<u8>,
}
impl PreparedModels {
    pub fn account_id(&self) -> &str {
        self.entry.account_id()
    }
    pub fn receipt(&self) -> Result<Value> {
        Ok(
            json!({"schema":"finam.imoexf.cohort-admission.v1","entry":self.entry.receipt().map_err(|_|anyhow::anyhow!("entry receipt"))?,
            "model_plan_sha256":hash(&self.plan)?,"manifest_sha256":self.manifest.sha256()?,
            "arms":self.plan.arms,"restart_allowed":false,"observed_clean_session":false,"broker_execution":false}),
        )
    }
    pub(crate) fn open(self, output: &Path, stop: StopHandle) -> Result<OpenedModels> {
        if let StopMode::Bounded { stop_at_ms } = self.plan.stop {
            ensure!(crate::timing::now_ms() < stop_at_ms, "missed bounded stop");
        }
        // Preparation fully validates manifest and readiness before any output.
        let model_receipt = self.receipt()?;
        let opened = self
            .entry
            .open_models(output, stop, self.manifest.clone(), model_receipt.clone())
            .map_err(|_| anyhow::anyhow!("cohort open"))?;
        artifacts::record_inputs(
            output,
            &self.plan,
            &self.manifest,
            &self.history,
            &self.calendar,
            &model_receipt,
        )?;
        Ok(OpenedModels {
            opened,
            config: self.config,
            plan: self.plan,
            manifest: self.manifest,
            output: output.to_owned(),
        })
    }
}
pub fn prepare(
    config: WsPrimaryConfig,
    plan: entry::Plan,
    inputs: entry::Inputs<'_>,
    model_plan: ModelPlan,
    history: &[u8],
    calendar: &[u8],
) -> Result<PreparedModels> {
    model_plan.validate(&config)?;
    if let StopMode::Bounded { stop_at_ms } = model_plan.stop {
        ensure!(
            stop_at_ms < plan.calendar.eod_check_at_ms,
            "bounded stop must precede calendar EOD"
        );
    }
    ensure!(
        matches!(plan.startup, entry::Startup::FreshSession) && inputs.prefix.is_none(),
        "cohort restart closed"
    );
    let total = history
        .len()
        .checked_add(calendar.len())
        .context("input size")?;
    let total = [
        inputs.reference,
        inputs.mapping,
        inputs.seed_calendar,
        inputs.seed,
        inputs.model_day,
        inputs.raw_calendar,
    ]
    .iter()
    .try_fold(total, |n, b| n.checked_add(b.len()))
    .context("combined input size")?;
    ensure!(
        total as u64 <= config.base.max_input_bytes.min(16 * 1024 * 1024),
        "model input bound"
    );
    let manifest = seed::build(&config, &model_plan, history, calendar)?;
    seed::require_ready(&manifest)?;
    let entry = entry::prepare(config.clone(), plan, inputs)
        .map_err(|_| anyhow::anyhow!("entry admission"))?;
    Ok(PreparedModels {
        entry,
        config,
        plan: model_plan,
        manifest,
        history: history.to_vec(),
        calendar: calendar.to_vec(),
    })
}
pub(crate) struct OpenedModels {
    pub opened: entry::Opened,
    pub config: WsPrimaryConfig,
    pub plan: ModelPlan,
    pub manifest: finam_shadow_models::local::contract::Manifest,
    pub output: PathBuf,
}
pub async fn run(
    prepared: PreparedModels,
    auth: std::sync::Arc<crate::auth::AuthLifecycle>,
    client: std::sync::Arc<broker_finam::Stage8bP1fO2GetOnlyClientV1>,
    output: &Path,
    stop: StopHandle,
) -> Result<Value> {
    ensure!(
        auth.binds(prepared.account_id()) && client.account_id() == prepared.account_id(),
        "account binding"
    );
    let state = prepared.open(output, stop.clone())?;
    let entry::Opened {
        owner,
        calendar,
        artifacts,
        config,
    } = state.opened;
    let calendar_hash = calendar.sha256().to_owned();
    let work = crate::runner::primary::live::run(owner, calendar, auth, client);
    tokio::pin!(work);
    let report = match state.plan.stop {
        StopMode::Bounded { stop_at_ms } => {
            let remaining = stop_at_ms.saturating_sub(crate::timing::now_ms()).max(0) as u64;
            tokio::select! {biased;
                _=tokio::time::sleep(std::time::Duration::from_millis(remaining))=>{stop.stop();work.await},
                result=&mut work=>result,
            }
        }
        StopMode::ModelEod => work.await,
    };
    let public_report = serde_json::to_value(report.as_ref())?;
    let ordinary = artifacts.finish(&config, &calendar_hash, report.as_ref());
    let done = artifacts::finish(
        &state.output,
        &state.config,
        &state.plan,
        &state.manifest,
        &public_report,
        ordinary.is_ok(),
    )?;
    Ok(done)
}
