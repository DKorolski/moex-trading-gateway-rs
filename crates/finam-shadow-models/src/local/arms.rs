use super::{
    contract::{Candle, Manifest, Purpose, Seed},
    hash,
};
use crate::{context, ri, ri_context, session, DataOrigin};
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeSet;

#[derive(Clone)]
enum Kernel {
    Reaction {
        context: context::Store,
        session: Option<Box<session::NumericalSession>>,
        config: session::Config,
    },
    Ri {
        context: ri_context::RiContextStore,
        session: Box<ri::RiSession>,
    },
}
#[derive(Clone)]
pub(super) struct Arm {
    pub id: String,
    pub instrument: String,
    kernel: Kernel,
    count: u32,
    clock: i64,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Saved {
    id: String,
    count: u32,
    clock: i64,
    context: Vec<u8>,
    session: Option<Vec<u8>>,
}
fn config(symbol: &str) -> Result<session::Config> {
    let (bytes, expected): (&[u8], &str) = match symbol {
        "IMOEXF" => (
            include_bytes!("../../profiles/imoexf.json"),
            "df266b75c97f1784891f38f8bdcd5f5133daf335f79acf73db00179b61166026",
        ),
        "USDRUBF" => (
            include_bytes!("../../profiles/usdrubf.json"),
            "8b8517e2c2b75d21d6a94686fe9a7abc7d52aa26b4456b17d1d9fccdfb3db496",
        ),
        _ => anyhow::bail!("not a reaction profile"),
    };
    use sha2::{Digest, Sha256};
    ensure!(
        format!("{:x}", Sha256::digest(bytes)) == expected,
        "pinned profile changed"
    );
    let profile: Value = serde_json::from_slice(bytes)?;
    Ok(serde_json::from_value(profile["config"].clone())?)
}
fn reaction_identity(
    symbol: &str,
    policy: context::Policy,
    generation: &str,
) -> Result<context::Identity> {
    Ok(context::Identity {
        profile: if symbol == "IMOEXF" {
            context::Profile::ImoexfReaction07
        } else {
            context::Profile::UsdrubfPreviousDay07
        },
        symbol: symbol.into(),
        model_config_sha256: config(symbol)?.fingerprint()?,
        receipt_stream: format!("{generation}/{symbol}/declared-clock"),
        policy,
        excluded_dates: BTreeSet::from([chrono::NaiveDate::from_ymd_opt(2026, 11, 4).unwrap()]),
    })
}
fn context_origin(c: &Candle) -> DataOrigin {
    if c.purpose == Purpose::ConfirmedClosingContext {
        DataOrigin::HistoryGap
    } else {
        DataOrigin::History
    }
}
impl Arm {
    pub fn bootstrap(manifest: &Manifest, seed: &Seed, second: bool) -> Result<Self> {
        let symbol = seed.binding.instrument.as_str();
        let clock = manifest.freeze_at_ms / 1000;
        let (policy_name, kernel) = if symbol == "RIZ6" {
            let policy = if second {
                ri_context::ContextPolicy::RiConfirmedClosingContextV1
            } else {
                ri_context::ContextPolicy::RuntimeAcceptedOnly
            };
            let id = ri::selected_identity(
                policy,
                format!("{}/{symbol}/declared-clock", manifest.generation),
            )?;
            let mut context = ri_context::RiContextStore::new(id.clone())?;
            for c in &seed.history {
                context.ingest(
                    ri_context::ContextInput {
                        sequence: c.source_sequence,
                        bar: c.numerical_bar(context_origin(c))?,
                        received_at_utc: Some(c.received_ms / 1000),
                        durable_at_utc: Some(c.durable_ms / 1000),
                    },
                    c.source_processed_ms / 1000,
                )?;
            }
            context.freeze(
                manifest.session_date,
                clock,
                &ri_context::ReceiptBarrier {
                    receipt_stream: id.receipt_stream.clone(),
                    through_utc: seed.declared_consumed_through_ms / 1000,
                    last_sequence: context.last_sequence(),
                    ingress_sha256: context.ingress_sha256()?,
                    attested_at_utc: clock,
                },
            )?;
            let session =
                ri::RiSession::new(&context, manifest.session_date, manifest.generation.clone())?;
            (
                if second {
                    "ri_confirmed_closing_context_v1"
                } else {
                    "runtime_accepted_only"
                },
                Kernel::Ri {
                    context,
                    session: Box::new(session),
                },
            )
        } else {
            let policy = if second {
                context::Policy::WeekendContextNoTrades
            } else {
                context::Policy::WeekdayOnly
            };
            let id = reaction_identity(symbol, policy, &manifest.generation)?;
            let config = config(symbol)?;
            let mut context = context::Store::new(id.clone())?;
            for c in &seed.history {
                context.ingest(
                    context::Receipt {
                        sequence: c.source_sequence,
                        source_id: c.sha256()?,
                        bar: c.numerical_bar(context_origin(c))?,
                        received_at_utc: Some(c.received_ms / 1000),
                        durable_at_utc: Some(c.durable_ms / 1000),
                    },
                    c.source_processed_ms / 1000,
                )?;
            }
            let f = context.freeze(
                manifest.session_date,
                clock,
                &context::Barrier {
                    receipt_stream: id.receipt_stream.clone(),
                    through_utc: seed.declared_consumed_through_ms / 1000,
                    last_sequence: context.last_sequence(),
                    ingress_sha256: context.ingress_sha256()?,
                    attested_at_utc: clock,
                },
            )?;
            let session = if f.ready() {
                Some(Box::new(session::NumericalSession::new(
                    &context,
                    manifest.session_date,
                    config.clone(),
                )?))
            } else {
                None
            };
            (
                if second {
                    "weekend_context_no_trades"
                } else {
                    "weekday_only"
                },
                Kernel::Reaction {
                    context,
                    session,
                    config,
                },
            )
        };
        Ok(Self {
            id: format!("{symbol}/{policy_name}"),
            instrument: symbol.into(),
            kernel,
            count: 0,
            clock,
        })
    }
    pub fn ready(&self, day: chrono::NaiveDate) -> bool {
        match &self.kernel {
            Kernel::Reaction { context, .. } => context.frozen(day).is_some_and(|f| f.ready()),
            Kernel::Ri { context, .. } => context.frozen(day).is_some_and(|f| f.ready()),
        }
    }
    pub fn complete(&self) -> bool {
        match &self.kernel {
            Kernel::Reaction { session, .. } => {
                session.as_ref().is_some_and(|s| s.complete().is_ok())
            }
            Kernel::Ri { session, .. } => session.complete().is_ok(),
        }
    }
    pub fn state(&self, day: chrono::NaiveDate) -> Result<Value> {
        let model = match &self.kernel {
            Kernel::Reaction {
                session: Some(s), ..
            } => s.state()?,
            Kernel::Reaction {
                context,
                session: None,
                ..
            } => json!({"context_not_ready":true, "frozen":context.frozen(day)}),
            Kernel::Ri { session, .. } => session.state(),
        };
        Ok(
            json!({"arm":self.id, "bars_processed":self.count, "declared_clock_floor_s":self.clock,
            "ready":self.ready(day), "full_grid_flat":self.complete(), "model":model,
            "evidence":"declared_clock_replay", "order_emission_allowed":false,
            "observed_clean_session":false}),
        )
    }
    pub fn step(&mut self, candle: &Candle) -> Result<Vec<Value>> {
        let now = candle.source_processed_ms / 1000;
        ensure!(
            now >= self.clock && self.count < 101,
            "arm clock/prefix bound"
        );
        let marks = match &mut self.kernel {
            Kernel::Reaction {
                session: Some(s), ..
            } => {
                let before = s.state()?;
                s.observe(candle.numerical_bar(DataOrigin::Replay)?, now)?;
                let after = s.state()?;
                let mut marks = Vec::new();
                for key in ["events", "trades"] {
                    let a = before["model"]["state"][key]
                        .as_array()
                        .context("reaction mark prefix missing")?;
                    let b = after["model"]["state"][key]
                        .as_array()
                        .context("reaction marks missing")?;
                    ensure!(b.starts_with(a), "reaction mark prefix changed");
                    marks.extend(b[a.len()..].iter().map(|v| json!({"kind":key, "value":v})));
                }
                marks
            }
            Kernel::Reaction { session: None, .. } => vec![],
            Kernel::Ri { context, session } => {
                // Live is the existing numerical kernel's route selector, NOT
                // a provider receipt assertion. The outer result always remains
                // declared_clock_replay and retains original millisecond clocks.
                let input = ri_context::ContextInput {
                    sequence: context.last_sequence() + 1,
                    bar: candle.numerical_bar(DataOrigin::Live)?,
                    received_at_utc: Some(candle.received_ms / 1000),
                    durable_at_utc: Some(candle.durable_ms / 1000),
                };
                context.ingest(input.clone(), now)?;
                session
                    .process(context, input, now)?
                    .model_marks
                    .iter()
                    .map(|m| Ok(json!({"kind":"model_mark", "value":serde_json::to_value(m)?})))
                    .collect::<Result<Vec<_>>>()?
            }
        };
        self.count += 1;
        self.clock = now;
        Ok(marks)
    }
    pub fn checkpoint(&self) -> Result<Vec<u8>> {
        let (context, session) = match &self.kernel {
            Kernel::Reaction {
                context, session, ..
            } => (
                context.checkpoint_bytes()?,
                session.as_ref().map(|s| s.checkpoint_bytes()).transpose()?,
            ),
            Kernel::Ri { context, session } => (
                context.checkpoint_bytes()?,
                Some(session.checkpoint_bytes()?),
            ),
        };
        Ok(serde_json::to_vec(&Saved {
            id: self.id.clone(),
            count: self.count,
            clock: self.clock,
            context,
            session,
        })?)
    }
    pub fn restore(&mut self, bytes: &[u8], manifest: &Manifest) -> Result<()> {
        ensure!(bytes.len() <= 64 * 1024 * 1024, "checkpoint bound");
        let p: Saved = serde_json::from_slice(bytes)?;
        ensure!(
            p.id == self.id && p.count <= 101 && p.clock >= manifest.freeze_at_ms / 1000,
            "saved arm identity/clock"
        );
        match &mut self.kernel {
            Kernel::Reaction {
                context,
                session,
                config,
            } => {
                let expected = context.identity().clone();
                *context = context::Store::restore(&expected, &p.context, p.clock)?;
                let ready = context
                    .frozen(manifest.session_date)
                    .context("missing frozen context")?
                    .ready();
                ensure!(
                    ready == p.session.is_some(),
                    "ready/session checkpoint mismatch"
                );
                *session = p
                    .session
                    .as_ref()
                    .map(|b| {
                        session::NumericalSession::restore(&expected, config, b, p.clock)
                            .map(Box::new)
                    })
                    .transpose()?;
                if let Some(s) = session {
                    ensure!(
                        s.state()?["bars_processed"].as_u64() == Some(u64::from(p.count)),
                        "reaction count mismatch"
                    );
                }
            }
            Kernel::Ri { context, session } => {
                let expected = context.identity().clone();
                *context = ri_context::RiContextStore::restore(&expected, &p.context, p.clock)?;
                **session = ri::RiSession::restore(
                    &expected,
                    &manifest.generation,
                    manifest.session_date,
                    p.session.as_ref().context("RI checkpoint missing")?,
                    p.clock,
                )?;
                ensure!(
                    context.last_sequence()
                        == (manifest
                            .seeds
                            .iter()
                            .find(|s| s.binding.instrument == self.instrument)
                            .context("RI seed missing")?
                            .history
                            .len() as u64
                            + u64::from(p.count)),
                    "RI checkpoint sequence mismatch"
                );
            }
        }
        self.count = p.count;
        self.clock = p.clock;
        // Full projection is independently cross-linked by the durable journal.
        hash(&self.state(manifest.session_date)?)?;
        Ok(())
    }
}
