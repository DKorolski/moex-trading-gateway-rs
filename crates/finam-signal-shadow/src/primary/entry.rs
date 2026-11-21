//! Explicit, bounded WS-primary entry. Preparation is file-only; it opens no
//! credential, transport or output. Only `run` invokes the fixed read-only
//! transports. A new physical subscription must still pass entry admission.
use super::{
    calendar::{self, AdmittedCalendar, CalendarBinding, CalendarInputs},
    restart::{self, CommittedPrefix, RestartBinding},
    WsPrimaryConfig,
};
use crate::{
    auth::AuthLifecycle,
    runner::primary::{live, Coordinator},
    seed::digest,
    segment::SeededSegment,
    session::ObservedSession,
    transport::StopHandle,
    ValidatedConfig,
};
use broker_finam::Stage8bP1fO2GetOnlyClientV1;
use serde::{Deserialize, Serialize};
use std::{path::Path, sync::Arc};
pub(crate) mod artifacts;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Plan {
    pub schema_version: u32,
    pub config_sha256: String,
    pub account_id: String,
    pub read_only_credentials: bool,
    pub auth_policy: String,
    pub calendar: CalendarBinding,
    pub startup: Startup,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Startup {
    FreshSession,
    CommittedPrefix { binding: RestartBinding },
}
impl Plan {
    pub fn parse(bytes: &[u8]) -> Result<Self, EntryError> {
        if bytes.len() > 16384 {
            return Err(EntryError::Bounds);
        }
        let value = super::json::parse(std::str::from_utf8(bytes).map_err(|_| EntryError::Plan)?)
            .map_err(|_| EntryError::Plan)?;
        serde_json::from_value(value).map_err(|_| EntryError::Plan)
    }
    fn fingerprint(&self) -> Result<String, EntryError> {
        let mut bytes = b"moex.signal-shadow.ws-primary-entry.v1\0".to_vec();
        bytes.extend(serde_json::to_vec(self).map_err(|_| EntryError::Plan)?);
        Ok(digest(&bytes))
    }
}
pub struct Inputs<'a> {
    pub reference: &'a [u8],
    pub mapping: &'a [u8],
    pub seed_calendar: &'a [u8],
    pub seed: &'a [u8],
    pub model_day: &'a [u8],
    pub raw_calendar: &'a [u8],
    pub prefix: Option<&'a [u8]>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum EntryError {
    Bounds,
    Plan,
    Binding,
    Calendar,
    Seed,
    Prefix,
    Stopped,
    Output,
    Session,
    Export,
}
/// No raw account ID, tokens, arbitrary provider fields or credential path.
#[derive(Debug, Serialize)]
pub struct AdmissionReceipt {
    pub schema_version: u32,
    pub config_sha256: String,
    pub plan_sha256: String,
    pub calendar_sha256: String,
    pub startup: &'static str,
    pub committed_prefix_labels: usize,
    pub previous_journal_sha256: Option<String>,
    pub next_label_ms: i64,
    pub required_labels: usize,
    pub prepared_at_ms: i64,
    pub entry_window_open_now: bool,
    pub history_proves_live_readiness: bool,
    pub broker_execution: bool,
}
/// Consumed once, never model-state import. Freshness/window are rechecked at
/// output creation and at the actual physical SUBSCRIBE, not frozen by prepare.
pub struct PreparedEntry {
    config: WsPrimaryConfig,
    plan: Plan,
    calendar: AdmittedCalendar,
    segment: SeededSegment,
    prefix: Option<CommittedPrefix>,
    prepared_at_ms: i64,
}
impl PreparedEntry {
    pub fn account_id(&self) -> &str {
        &self.plan.account_id
    }
    pub fn receipt(&self) -> Result<AdmissionReceipt, EntryError> {
        Ok(AdmissionReceipt {
            schema_version: 1,
            config_sha256: self.config.fingerprint().map_err(|_| EntryError::Binding)?,
            plan_sha256: self.plan.fingerprint()?,
            calendar_sha256: self.calendar.sha256().into(),
            startup: if self.prefix.is_some() {
                "committed_prefix"
            } else {
                "fresh_session"
            },
            committed_prefix_labels: self
                .prefix
                .as_ref()
                .map_or(0, CommittedPrefix::committed_labels),
            previous_journal_sha256: self
                .prefix
                .as_ref()
                .map(|p| p.binding().previous_journal_sha256.clone()),
            next_label_ms: self.plan.calendar.next_label_ms,
            required_labels: self.calendar.required().len(),
            prepared_at_ms: self.prepared_at_ms,
            entry_window_open_now: self
                .calendar
                .validate_entry(crate::timing::now_ms())
                .is_ok(),
            history_proves_live_readiness: false,
            broker_execution: false,
        })
    }
    pub(crate) fn open(self, output: &Path, stop: StopHandle) -> Result<Opened, EntryError> {
        self.open_inner(
            output,
            stop,
            #[cfg(feature = "models-bridge")]
            None,
        )
    }
    #[cfg(feature = "models-bridge")]
    pub(crate) fn open_models(
        self,
        output: &Path,
        stop: StopHandle,
        manifest: finam_shadow_models::local::contract::Manifest,
        binding: serde_json::Value,
    ) -> Result<Opened, EntryError> {
        self.open_inner(output, stop, Some((manifest, binding)))
    }
    fn open_inner(
        self,
        output: &Path,
        stop: StopHandle,
        #[cfg(feature = "models-bridge")] models: Option<(
            finam_shadow_models::local::contract::Manifest,
            serde_json::Value,
        )>,
    ) -> Result<Opened, EntryError> {
        if stop.is_stopped() {
            return Err(EntryError::Stopped);
        }
        self.calendar
            .validate_entry(crate::timing::now_ms())
            .map_err(|_| EntryError::Calendar)?;
        let receipt = self.receipt()?;
        let artifacts = artifacts::Artifacts::create(output)?;
        artifacts.record_admission(&receipt)?;
        let (mut session, processor) = match self.prefix {
            Some(prefix) => ObservedSession::resume_primary(
                self.segment,
                prefix,
                &self.config,
                &artifacts.journal(),
                stop,
            )
            .map_err(|_| EntryError::Prefix)?,
            None => {
                let processor = super::PrimaryProcessor::from_config(
                    &self.config,
                    instrument(),
                    self.calendar.required().to_vec(),
                    1,
                    crate::timing::now_ms(),
                )
                .map_err(|_| EntryError::Binding)?;
                let session = ObservedSession::create(self.segment, &artifacts.journal(), stop)
                    .map_err(|_| EntryError::Session)?;
                (session, processor)
            }
        };
        session
            .record_run_plan(&receipt)
            .map_err(|_| EntryError::Session)?;
        #[cfg(feature = "models-bridge")]
        let owner = if let Some((manifest, binding)) = models {
            session
                .record_transport(
                    &serde_json::json!({"event":"primary_models_seed_binding","binding":binding}),
                )
                .map_err(|_| EntryError::Session)?;
            let bridge = super::models_bridge::ImoexfModelBridge::create(
                &output.join("models.sqlite"),
                manifest,
                &self.config,
            )
            .map_err(|_| EntryError::Seed)?;
            Coordinator::new_with_imoexf_models(processor, session, bridge)
        } else {
            Coordinator::new(processor, session)
        }
        .map_err(|_| EntryError::Binding)?;
        #[cfg(not(feature = "models-bridge"))]
        let owner = Coordinator::new(processor, session).map_err(|_| EntryError::Binding)?;
        Ok(Opened {
            owner,
            calendar: self.calendar,
            artifacts,
            config: self.config,
        })
    }
}
pub(crate) struct Opened {
    pub(crate) owner: Coordinator,
    pub(crate) calendar: AdmittedCalendar,
    pub(crate) artifacts: artifacts::Artifacts,
    pub(crate) config: WsPrimaryConfig,
}

pub fn prepare(
    config: WsPrimaryConfig,
    plan: Plan,
    inputs: Inputs<'_>,
) -> Result<PreparedEntry, EntryError> {
    let now = crate::timing::now_ms();
    let total = [
        inputs.reference,
        inputs.mapping,
        inputs.seed_calendar,
        inputs.seed,
        inputs.model_day,
        inputs.raw_calendar,
        inputs.prefix.unwrap_or_default(),
    ]
    .iter()
    .try_fold(0usize, |sum, b| sum.checked_add(b.len()))
    .ok_or(EntryError::Bounds)?;
    let validated = ValidatedConfig::from_ws_primary(&config).map_err(|_| EntryError::Binding)?;
    if total as u64 > config.base.max_input_bytes.min(16 * 1024 * 1024) {
        return Err(EntryError::Bounds);
    }
    if plan.schema_version != 1
        || plan.config_sha256 != validated.fingerprint()
        || !plan.read_only_credentials
        || plan.auth_policy != "provider_expiry_readonly_rtsx_v1"
        || plan.account_id.is_empty()
        || plan.account_id.len() > 128
        || !plan
            .account_id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
    {
        return Err(EntryError::Plan);
    }
    let calendar = calendar::admit(
        &config,
        plan.calendar.clone(),
        CalendarInputs {
            model_day: inputs.model_day,
            raw_calendar: inputs.raw_calendar,
            mapping: inputs.mapping,
        },
        now,
    )
    .map_err(|_| EntryError::Calendar)?;
    let prefix = match (&plan.startup, inputs.prefix) {
        (Startup::FreshSession, None)
            if config.base.previous_segment_id.is_none()
                && plan.calendar.next_label_ms == config.base.observation_start_utc_ms =>
        {
            None
        }
        (Startup::CommittedPrefix { binding }, Some(bytes)) => {
            let prefix = restart::admit(&config, binding.clone(), bytes, calendar.required(), now)
                .map_err(|_| EntryError::Prefix)?;
            if prefix.next_label_ms() != plan.calendar.next_label_ms {
                return Err(EntryError::Binding);
            }
            Some(prefix)
        }
        _ => return Err(EntryError::Binding),
    };
    let segment = SeededSegment::from_inputs(
        validated,
        inputs.reference,
        inputs.mapping,
        inputs.seed_calendar,
        inputs.seed,
    )
    .map_err(|_| EntryError::Seed)?;
    Ok(PreparedEntry {
        config,
        plan,
        calendar,
        segment,
        prefix,
        prepared_at_ms: now,
    })
}

/// Explicit call only. Auth/client constructors perform no request. All actual
/// commands still use current checked grants and the existing GET-only client.
pub async fn run(
    prepared: PreparedEntry,
    auth: Arc<AuthLifecycle>,
    client: Arc<Stage8bP1fO2GetOnlyClientV1>,
    output: &Path,
    stop: StopHandle,
) -> Result<ExportResult, EntryError> {
    if !auth.binds(prepared.account_id()) || client.account_id() != prepared.account_id() {
        return Err(EntryError::Binding);
    }
    let Opened {
        owner,
        calendar,
        artifacts,
        config,
    } = prepared.open(output, stop)?;
    let calendar_sha256 = calendar.sha256().to_owned();
    let report = live::run(owner, calendar, auth, client).await;
    artifacts.finish(&config, &calendar_sha256, report.as_ref())
}
pub(super) fn instrument() -> broker_core::InstrumentId {
    broker_core::InstrumentId {
        symbol: "IMOEXF".into(),
        venue_symbol: Some("IMOEXF@RTSX".into()),
        exchange: broker_core::Exchange::Moex,
        market: broker_core::Market::Futures,
    }
}
pub use artifacts::ExportResult;
#[cfg(test)]
pub(crate) mod tests;
