//! Fresh source-component admission only. No managed network entry is enabled.
use crate::primary::{
    calendar::{self, AdmittedCalendar, CalendarBinding, ModelDay, RawCalendar, ScheduleSnapshot},
    models_bridge::MAPPING_SHA256,
};
use anyhow::{ensure, Context, Result};
use broker_finam::ShadowInstrument;
use finam_shadow_models::local::{
    contract::{InitialContext, Manifest},
    Journal,
};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::BTreeMap, str::FromStr};

pub(super) const INSTRUMENTS: [ShadowInstrument; 2] =
    [ShadowInstrument::Usdrubf, ShadowInstrument::Riz6];
pub(super) fn hash(v: &impl Serialize) -> Result<String> {
    Ok(crate::seed::digest(&serde_json::to_vec(v)?))
}
pub(super) fn sha(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
pub(super) fn policy() -> crate::primary::PrimaryPolicy {
    crate::primary::PrimaryPolicy {
        delivery: crate::primary::DeliveryPolicy::ObservedBoWsHistoricalRecoveryV8,
        ..Default::default()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Plan {
    pub schema: String,
    pub manifest_sha256: String,
    /// Hash of exact selected account ID; never an account number or credential.
    pub account_sha256: String,
    pub start_not_before_ms: i64,
    pub start_before_ms: i64,
    pub stop_at_ms: i64,
}
impl Plan {
    pub(super) fn validate(&self, m: &Manifest) -> Result<()> {
        m.validate()?;
        let open = m.freeze_at_ms;
        ensure!(
            self.schema == "finam.shadow.usd-ri.source-component.v1"
                && self.manifest_sha256 == m.sha256()?
                && sha(&self.account_sha256)
                && m.initial_context == InitialContext::UsdRiFirstSession,
            "source plan/manifest/account binding"
        );
        ensure!(
            self.start_not_before_ms >= open
                && self.start_not_before_ms < self.start_before_ms
                && self.start_before_ms <= open + 540000
                && self.stop_at_ms > self.start_before_ms
                && self.stop_at_ms <= open + 101 * 600000 + 10000,
            "fresh entry/bounded stop window"
        );
        Ok(())
    }
}
/// Private full HTTP bodies are consumed in memory. Only public validated
/// identity projection, original body hashes and request clocks are exported.
pub struct InstrumentEvidence {
    pub instrument: ShadowInstrument,
    pub asset: ScheduleSnapshot,
    pub params: ScheduleSnapshot,
    pub calendar: RawCalendar,
    pub bootstrap_reference_start_ms: i64,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct IdentityReceipt {
    pub instrument: ShadowInstrument,
    pub account_sha256: String,
    pub asset_response_sha256: String,
    pub params_response_sha256: String,
    pub requested_at_ms: [i64; 2],
    pub received_at_ms: [i64; 2],
    pub public_asset: Value,
    pub public_params: Value,
}
fn body(
    s: &ScheduleSnapshot,
    instrument: ShadowInstrument,
    suffix: &str,
    now: i64,
) -> Result<Value> {
    let endpoint = format!(
        "https://api.finam.ru/v1/assets/{}{suffix}",
        instrument.venue_symbol()
    );
    ensure!(
        s.method == "GET"
            && (s.endpoint == endpoint || s.endpoint == endpoint.replace('@', "%40"))
            && s.status == 200
            && s.transport_complete
            && s.requested_at_ms >= 0
            && s.received_at_ms >= s.requested_at_ms
            && s.received_at_ms <= now
            && now - s.received_at_ms <= 300000
            && s.received_at_ms - s.requested_at_ms <= 5000
            && s.raw_body.len() <= 65536
            && s.declared_body_bytes == s.raw_body.len() as u64
            && crate::seed::digest(s.raw_body.as_bytes()) == s.response_sha256,
        "metadata transport/clock/hash"
    );
    crate::primary::json::parse(&s.raw_body).map_err(|e| anyhow::anyhow!("metadata JSON: {e:?}"))
}
fn decimal(v: &Value) -> Result<Decimal> {
    Ok(Decimal::from_str(v.as_str().context("decimal string")?)?)
}
pub(super) fn identity(e: &InstrumentEvidence, account: &str, now: i64) -> Result<IdentityReceipt> {
    let i = e.instrument;
    ensure!(INSTRUMENTS.contains(&i), "additional instrument only");
    let asset = body(&e.asset, i, "", now)?;
    let params = body(&e.params, i, "/params", now)?;
    let decimals = if i == ShadowInstrument::Usdrubf { 2 } else { 0 };
    let currency = if i == ShadowInstrument::Usdrubf {
        "RUB"
    } else {
        "USD"
    };
    ensure!(
        asset["id"] == i.asset_id()
            && asset["ticker"] == i.symbol()
            && asset["mic"] == "RTSX"
            && asset["board"] == "FUT"
            && asset["type"] == "FUTURES"
            && asset["decimals"] == decimals
            && asset["quote_currency"] == currency
            && asset["name"]
                == if i == ShadowInstrument::Riz6 {
                    "RTS-12.26"
                } else {
                    "USDRUBF"
                }
            && asset["min_step"]
                == if i == ShadowInstrument::Usdrubf {
                    "1"
                } else {
                    "10"
                }
            && decimal(&asset["lot_size"]["value"])? > Decimal::ZERO
            && decimal(&asset["future_details"]["contract_size"]["value"])? > Decimal::ZERO,
        "asset identity/tick/reference lot/currency"
    );
    ensure!(
        params["symbol"] == i.venue_symbol()
            && decimal(&params["trade_lot_size"])? == Decimal::ONE
            && match (params.get("is_tradable"), params.get("tradeable")) {
                (Some(a), None) | (None, Some(a)) => a == true,
                (Some(a), Some(b)) => a == true && b == true,
                _ => false,
            }
            && crate::seed::digest(
                params["account_id"]
                    .as_str()
                    .context("params account missing")?
                    .as_bytes()
            ) == account,
        "params selected account/order lot"
    );
    if let Some(id) = asset.get("account_id") {
        ensure!(
            id.as_str()
                .is_some_and(|s| crate::seed::digest(s.as_bytes()) == account),
            "asset selected account"
        );
    }
    let expiry = asset["future_details"]["expiration_date"].clone();
    if i == ShadowInstrument::Riz6 {
        ensure!(
            expiry == "2026-12-16T21:00:00Z"
                && asset["expiration_date"] == json!({"year":2026,"month":12,"day":17}),
            "exact RIZ6 expiry; no rollover"
        );
    } else {
        ensure!(
            expiry.is_null() && asset["expiration_date"].is_null(),
            "unexpected perpetual expiry"
        );
    }
    Ok(IdentityReceipt {
        instrument: i,
        account_sha256: account.into(),
        asset_response_sha256: e.asset.response_sha256.clone(),
        params_response_sha256: e.params.response_sha256.clone(),
        requested_at_ms: [e.asset.requested_at_ms, e.params.requested_at_ms],
        received_at_ms: [e.asset.received_at_ms, e.params.received_at_ms],
        public_asset: json!({"id":i.asset_id(),"ticker":i.symbol(),"mic":"RTSX","board":"FUT","tick":i.tick().to_string(),"quote_currency":currency,"expiration_date":expiry,"reference_lot_size":asset["lot_size"]["value"]}),
        public_params: json!({"symbol":i.venue_symbol(),"trade_lot_size":"1","is_tradable":true}),
    })
}
/// No Deserialize, Clone or constructor from a receipt/projection. Still only
/// a source component: auth/managed IO/worker drain need their own entry gate.
pub struct Prepared {
    pub(super) plan: Plan,
    pub(super) manifest: Manifest,
    pub(super) calendars: BTreeMap<ShadowInstrument, AdmittedCalendar>,
    pub(super) identities: BTreeMap<ShadowInstrument, IdentityReceipt>,
    pub(super) configs: BTreeMap<ShadowInstrument, String>,
}
impl Prepared {
    pub(super) fn identities_fresh(&self, now: i64) -> Result<()> {
        ensure!(
            self.identities
                .values()
                .all(|r| r.received_at_ms.iter().all(|t| now
                    .checked_sub(*t)
                    .is_some_and(|age| (0..=300000).contains(&age)))),
            "metadata stale at actual source attachment"
        );
        Ok(())
    }
    pub fn admit(
        plan: Plan,
        manifest: Manifest,
        evidence: Vec<InstrumentEvidence>,
    ) -> Result<Self> {
        let now = crate::timing::now_ms();
        plan.validate(&manifest)?;
        ensure!(
            now < plan.start_before_ms && evidence.len() == 2,
            "admission window/two sources"
        );
        let states = Journal::initial_states(&manifest)?;
        ensure!(
            states
                .iter()
                .filter(|s| s["ready"] == true && s["bars_processed"] == 0)
                .count()
                == 4,
            "four READY arms"
        );
        let mut value = Self {
            plan,
            manifest,
            calendars: BTreeMap::new(),
            identities: BTreeMap::new(),
            configs: BTreeMap::new(),
        };
        for e in evidence {
            let i = e.instrument;
            let identity = identity(&e, &value.plan.account_sha256, now)?;
            ensure!(
                !value.identities.contains_key(&i),
                "duplicate instrument evidence"
            );
            let config = hash(&(
                &value.plan,
                &identity,
                &e.calendar,
                e.bootstrap_reference_start_ms,
                policy(),
                super::implementation(),
            ))?;
            let open = value.manifest.freeze_at_ms;
            let model = ModelDay {
                schema_version: 1,
                session_date: value.manifest.session_date.to_string(),
                required_m10_labels_ms: (0..101).map(|n| open + n * 600000).collect(),
            };
            let binding = CalendarBinding {
                schema_version: 1,
                config_sha256: config.clone(),
                model_day_sha256: hash(&model)?,
                raw_calendar_sha256: hash(&e.calendar)?,
                bootstrap_reference_start_ms: e.bootstrap_reference_start_ms,
                next_label_ms: open,
                start_not_before_ms: value.plan.start_not_before_ms,
                start_before_ms: value.plan.start_before_ms,
                eod_check_at_ms: open + 101 * 600000 + 9900,
            };
            let mapping = serde_json::to_vec(
                &json!({"instrument":i,"identity":identity,"catalog_sha256":MAPPING_SHA256}),
            )?;
            let calendar = calendar::admit_additional(i, binding, model, e.calendar, &mapping, now)
                .map_err(|e| anyhow::anyhow!("calendar: {e:?}"))?;
            value.calendars.insert(i, calendar);
            value.identities.insert(i, identity);
            value.configs.insert(i, config);
        }
        ensure!(
            INSTRUMENTS.iter().all(|i| value.configs.contains_key(i)),
            "USD and RIZ6 required"
        );
        Ok(value)
    }
    pub fn receipt(&self) -> Result<Value> {
        Ok(
            json!({"schema":"finam.shadow.usd-ri.preparation.v1","plan":self.plan,
        "implementation_sha256":super::implementation(),"identities":self.identities,"source_configs":self.configs,
        "calendars":self.calendars.iter().map(|(i,c)|json!({"instrument":i,"evidence":c.evidence()})).collect::<Vec<_>>(),
        "scope":"source_component_only","network_entry_enabled":false,"operational_activation":false,"orders":false}),
        )
    }
}
