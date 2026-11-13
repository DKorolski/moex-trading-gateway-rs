//! Local RI prospective session, receipt binding and replay checkpoint.
//! This is not an operational durable writer or a provider/FINAM adapter.
use crate::ri_context::{
    ContextIdentity, ContextInput, ContextPolicy, ContextRoute, FrozenContext, RiContextStore,
};
use crate::ri_engine::{Engine, MarkAction};
use crate::DataOrigin;
use anyhow::{ensure, Context, Result};
use chrono::{DateTime, Duration, NaiveDate, NaiveDateTime};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

pub const VERSION: &str = "finam.ri.prospective.numerical.v1";
pub const PROFILE_SHA256: &str = "30e74209c058e288ab171afb7d18cba4d9bb93a0e4927cf9a6062325e96fe989";
pub const PROFILE: &[u8] = include_bytes!("../profiles/ri.json");
const MAX_BYTES: usize = 16 * 1024 * 1024;
const MAX_INPUTS: usize = 1024;

fn hash<T: Serialize>(value: &T) -> Result<String> {
    Ok(format!("{:x}", Sha256::digest(serde_json::to_vec(value)?)))
}
pub(crate) fn local(ts: i64) -> Result<NaiveDateTime> {
    DateTime::from_timestamp(ts, 0)
        .and_then(|d| d.naive_utc().checked_add_signed(Duration::hours(3)))
        .context("invalid RI timestamp")
}

/// The active contract/calendar is compiled from the exact selected profile.
/// No legacy BO max2/last-entry17, live mode or contract substitution exists.
pub fn selected_identity(policy: ContextPolicy, receipt_stream: String) -> Result<ContextIdentity> {
    ensure!(
        format!("{:x}", Sha256::digest(PROFILE)) == PROFILE_SHA256,
        "selected RI profile bytes changed"
    );
    let profile: Value = serde_json::from_slice(PROFILE)?;
    let dates: BTreeSet<_> = profile["excluded_dates"]
        .as_array()
        .context("RI calendar missing")?
        .iter()
        .map(|d| NaiveDate::parse_from_str(d.as_str().unwrap_or(""), "%Y-%m-%d"))
        .collect::<std::result::Result<_, _>>()?;
    let id = ContextIdentity {
        profile_id: profile["model_id"]
            .as_str()
            .context("model ID missing")?
            .into(),
        symbol: profile["symbol"].as_str().context("symbol missing")?.into(),
        numerical_config_sha256: hash(&(
            VERSION,
            PROFILE_SHA256,
            format!("{:x}", Sha256::digest(include_bytes!("ri_engine.rs"))),
            format!("{:x}", Sha256::digest(include_bytes!("ri_context.rs"))),
            format!("{:x}", Sha256::digest(include_bytes!("ri.rs"))),
        ))?,
        model_feed_policy: "micro_regular_v1".into(),
        receipt_stream,
        policy,
        model_open_hour: 7,
        excluded_dates: dates,
    };
    ensure!(
        id.receipt_stream.len() <= 256,
        "RI receipt namespace too long"
    );
    id.validate()?;
    Ok(id)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Step {
    pub sequence: u64,
    pub bar_start_utc: i64,
    pub bar_close_at_utc: i64,
    pub received_at_utc: Option<i64>,
    pub durable_at_utc: Option<i64>,
    pub processed_at_utc: i64,
    pub origin: DataOrigin,
    pub disposition: String,
    pub frozen_context_sha256: String,
    pub model_marks: Vec<MarkAction>,
    pub state: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    value: ContextInput,
    processed_at_utc: i64,
    model_step: bool,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Payload {
    version: String,
    identity: ContextIdentity,
    generation: String,
    day: NaiveDate,
    base_context: String,
    base_clock_floor: i64,
    inputs: Vec<Input>,
    clock_floor: i64,
    witness: Engine,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    payload: Payload,
    sha256: String,
}

#[derive(Clone)]
pub struct RiSession {
    identity: ContextIdentity,
    generation: String,
    frozen: FrozenContext,
    base_context: String,
    base_clock_floor: i64,
    engine: Engine,
    inputs: Vec<Input>,
    sequence: u64,
    last_model_label: Option<i64>,
    clock_floor: i64,
}
impl RiSession {
    pub fn new(store: &RiContextStore, day: NaiveDate, generation: String) -> Result<Self> {
        let identity = store.identity().clone();
        ensure!(
            identity == selected_identity(identity.policy, identity.receipt_stream.clone())?,
            "RI selected config/contract/calendar mismatch"
        );
        ensure!(
            !generation.trim().is_empty() && generation.len() <= 256,
            "invalid RI generation"
        );
        let frozen = store
            .frozen(day)
            .context("RI session must be frozen before numerical use")?
            .clone();
        ensure!(
            frozen.barrier.last_sequence == store.last_sequence(),
            "new RI session requires exact frozen frontier"
        );
        let engine = Engine::new(&frozen)?;
        let base_clock_floor = store
            .last_processed_at_utc()
            .context("RI base clock missing")?;
        Ok(Self {
            identity,
            generation,
            frozen: frozen.clone(),
            base_context: String::from_utf8(store.checkpoint_bytes()?)?,
            base_clock_floor,
            engine,
            inputs: vec![],
            sequence: store.last_sequence(),
            last_model_label: None,
            clock_floor: base_clock_floor,
        })
    }

    /// Strict receipt order, atomic engine transition. Sparse numerical fixtures
    /// are allowed; complete() separately requires the entire actual model grid.
    pub fn process(
        &mut self,
        store: &RiContextStore,
        input: ContextInput,
        processed_at_utc: i64,
    ) -> Result<Step> {
        ensure!(self.inputs.len() < MAX_INPUTS, "RI prefix bound");
        // Rejected nonfinite raw payloads may have a context audit digest, but
        // must never be serialized as null into a supposedly restorable prefix.
        ensure!(
            [
                input.bar.o,
                input.bar.h,
                input.bar.l,
                input.bar.close,
                input.bar.v
            ]
            .iter()
            .all(|v| v.is_finite()),
            "RI nonfinite raw input cannot enter replay checkpoint"
        );
        ensure!(
            store.identity() == &self.identity,
            "wrong RI context identity"
        );
        ensure!(
            input.sequence == self.sequence + 1,
            "RI missing/duplicate receipt sequence"
        );
        ensure!(
            processed_at_utc >= self.clock_floor && local(processed_at_utc).is_ok(),
            "RI processing clock regressed"
        );
        let route = store.verified_route(&input)?;
        ensure!(
            store.verified_processed_at(&input)? == processed_at_utc,
            "RI processing clock not bound to ingress"
        );
        ensure!(
            store.frozen(self.frozen.session_date) == Some(&self.frozen),
            "RI frozen context changed"
        );
        let mut engine = self.engine.clone();
        engine.clear_step();
        let mut disposition = format!("context_only:{route:?}");
        let mut label = None;
        if route == ContextRoute::ContextOnly && input.bar.origin == DataOrigin::Live {
            let dt = local(input.bar.close_time_utc)?;
            ensure!(
                dt.date() == self.frozen.session_date,
                "RI session cannot cross days implicitly"
            );
            if local(input.received_at_utc.context("missing RI receipt")?)?.date() == dt.date() {
                ensure!(
                    self.last_model_label
                        .map_or(true, |last| input.bar.close_time_utc > last),
                    "RI duplicate/backdated model input"
                );
                engine.step(&input.bar, dt)?;
                disposition = if self.frozen.ready() {
                    "prospective_numerical"
                } else {
                    "context_not_ready"
                }
                .into();
                label = Some(input.bar.close_time_utc);
            } else {
                disposition = "previous_session_receipt_context_only".into();
            }
        }
        let step = Step {
            sequence: input.sequence,
            bar_start_utc: input.bar.close_time_utc,
            bar_close_at_utc: input
                .bar
                .close_time_utc
                .checked_add(600)
                .context("RI close clock overflow")?,
            received_at_utc: input.received_at_utc,
            durable_at_utc: input.durable_at_utc,
            processed_at_utc,
            origin: input.bar.origin.clone(),
            disposition,
            frozen_context_sha256: self.frozen.sha256()?,
            model_marks: engine.actions().to_vec(),
            state: engine.projection(&self.frozen),
        };
        self.engine = engine;
        self.sequence = input.sequence;
        if let Some(label) = label {
            self.last_model_label = Some(label);
        }
        self.inputs.push(Input {
            value: input,
            processed_at_utc,
            model_step: label.is_some(),
        });
        self.clock_floor = processed_at_utc;
        Ok(step)
    }

    pub fn state(&self) -> Value {
        self.engine.projection(&self.frozen)
    }
    pub fn order_emission_allowed(&self) -> bool {
        false
    }
    pub fn complete(&self) -> Result<()> {
        let first = self.identity.cutoff(self.frozen.session_date);
        let end = (self.frozen.session_date.and_hms_opt(23, 40, 0).unwrap() - Duration::hours(3))
            .and_utc()
            .timestamp();
        let expected: Vec<_> = (first..=end).step_by(600).collect();
        let actual: Vec<_> = self
            .inputs
            .iter()
            .filter(|i| i.model_step)
            .map(|i| i.value.bar.close_time_utc)
            .collect();
        ensure!(
            actual == expected && self.last_model_label == Some(end),
            "RI incomplete model calendar/terminal"
        );
        ensure!(
            self.engine.is_flat(),
            "RI unresolved position/pending at EOD"
        );
        Ok(())
    }

    pub fn checkpoint_bytes(&self) -> Result<Vec<u8>> {
        let payload = Payload {
            version: VERSION.into(),
            identity: self.identity.clone(),
            generation: self.generation.clone(),
            day: self.frozen.session_date,
            base_context: self.base_context.clone(),
            base_clock_floor: self.base_clock_floor,
            inputs: self.inputs.clone(),
            clock_floor: self.clock_floor,
            witness: self.engine.clone(),
        };
        let sha256 = hash(&payload)?;
        let bytes = serde_json::to_vec(&Envelope { payload, sha256 })?;
        ensure!(bytes.len() <= MAX_BYTES, "RI checkpoint byte bound");
        Ok(bytes)
    }

    /// Restore recomputes marks privately. No actions are returned or published.
    pub fn restore(
        expected: &ContextIdentity,
        generation: &str,
        day: NaiveDate,
        bytes: &[u8],
        now: i64,
    ) -> Result<Self> {
        ensure!(bytes.len() <= MAX_BYTES, "RI checkpoint byte bound");
        let saved: Envelope = serde_json::from_slice(bytes)?;
        let p = saved.payload;
        ensure!(
            p.version == VERSION
                && p.identity == *expected
                && p.generation == generation
                && p.day == day,
            "RI checkpoint identity/generation/day mismatch"
        );
        ensure!(hash(&p)? == saved.sha256, "RI checkpoint checksum mismatch");
        ensure!(
            p.inputs.len() <= MAX_INPUTS && p.clock_floor <= now && local(now).is_ok(),
            "RI restart bound/clock mismatch"
        );
        let mut store =
            RiContextStore::restore(expected, p.base_context.as_bytes(), p.base_clock_floor)?;
        let mut restored = Self::new(&store, day, generation.into())?;
        ensure!(
            restored.base_context == p.base_context,
            "RI base context clock changed"
        );
        for input in p.inputs {
            store.ingest(input.value.clone(), input.processed_at_utc)?;
            restored.process(&store, input.value, input.processed_at_utc)?;
            ensure!(
                restored.inputs.last().unwrap().model_step == input.model_step,
                "RI replay routing changed"
            );
        }
        ensure!(
            restored.engine == p.witness,
            "RI reconstructed state differs from witness"
        );
        ensure!(
            restored.clock_floor <= p.clock_floor,
            "RI saved processing clock mismatch"
        );
        // now is a restore guard, not a fabricated source or historical receipt.
        restored.clock_floor = now;
        Ok(restored)
    }
}
