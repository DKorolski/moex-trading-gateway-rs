//! Input-only fixture adapter. It never opens expected traces or a broker.
use crate::context::{Barrier, Identity, Receipt, Store};
use crate::session::{Config, NumericalSession};
use crate::BarEvent;
use anyhow::{ensure, Result};
use chrono::NaiveDate;
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

pub const MAX_INPUT_BYTES: usize = 32 * 1024 * 1024;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Fixture {
    schema_version: String,
    evidence_kind: String,
    cases: Vec<Case>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct HistoricalInput {
    value: Receipt,
    processed_at_utc: i64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    id: String,
    identity: Identity,
    config: Config,
    history: Vec<HistoricalInput>,
    day: NaiveDate,
    freeze_at_utc: i64,
    operations: Vec<Op>,
}
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Op {
    Bar {
        value: BarEvent,
        observed_at_utc: i64,
    },
    Restart {
        now: i64,
    },
    Receipt {
        value: Receipt,
        processed_at_utc: i64,
    },
}

pub fn run(bytes: &[u8], implementation_id: &str) -> Result<Value> {
    ensure!(bytes.len() <= MAX_INPUT_BYTES, "fixture input limit");
    ensure!(
        !implementation_id.trim().is_empty(),
        "implementation identity required"
    );
    let fixture: Fixture = serde_json::from_slice(bytes)?;
    ensure!(
        fixture.schema_version == "weekend_reaction_fixture_v1"
            && fixture.evidence_kind == "synthetic_numerical_reaction_not_production",
        "only explicit synthetic fixture evidence accepted"
    );
    ensure!(
        !fixture.cases.is_empty() && fixture.cases.len() <= 128,
        "fixture case bound"
    );
    let mut ids = BTreeSet::new();
    let mut cases = Vec::new();
    for case in fixture.cases {
        ensure!(
            ids.insert(case.id.clone()) && !case.id.is_empty(),
            "duplicate/empty case ID"
        );
        ensure!(
            case.history.len() <= 1000 && case.operations.len() <= 160,
            "fixture operation bound"
        );
        let mut context = Store::new(case.identity.clone())?;
        let mut routes = Vec::new();
        for input in case.history {
            routes.push(context.ingest(input.value, input.processed_at_utc)?);
        }
        let barrier = Barrier {
            receipt_stream: case.identity.receipt_stream.clone(),
            through_utc: case.identity.cutoff(case.day),
            last_sequence: context.last_sequence(),
            ingress_sha256: context.ingress_sha256()?,
            attested_at_utc: case.freeze_at_utc,
        };
        let frozen = context.freeze(case.day, case.freeze_at_utc, &barrier)?;
        let mut session = NumericalSession::new(&context, case.day, case.config.clone())?;
        let mut rows = Vec::new();
        for (index, op) in case.operations.into_iter().enumerate() {
            let mut route = None;
            let kind = match op {
                Op::Bar {
                    value,
                    observed_at_utc,
                } => {
                    session.observe(value, observed_at_utc)?;
                    "bar"
                }
                Op::Receipt {
                    value,
                    processed_at_utc,
                } => {
                    let before = session.state()?;
                    route = Some(context.ingest(value, processed_at_utc)?);
                    ensure!(session.state()? == before, "receipt mutated frozen model");
                    "receipt"
                }
                Op::Restart { now } => {
                    let before = session.state()?;
                    session = NumericalSession::restore(
                        &case.identity,
                        &case.config,
                        &session.checkpoint_bytes()?,
                        now,
                    )?;
                    ensure!(session.state()? == before, "restart changed numeric state");
                    context = Store::restore(&case.identity, &context.checkpoint_bytes()?, now)?;
                    ensure!(
                        context.frozen(case.day) == Some(&frozen),
                        "restart changed frozen context"
                    );
                    "restart"
                }
            };
            let state = session.state()?;
            ensure!(
                state["order_emission_allowed"] == false
                    && state["frozen"] == serde_json::to_value(&frozen)?,
                "frozen state changed"
            );
            rows.push(json!({"index":index,"kind":kind,"route":route,
                "context_sequence":context.last_sequence(),"context_accepted":context.accepted_count(),
                "bars_processed":state["bars_processed"],"model":state["model"]}));
        }
        session.complete()?;
        cases.push(json!({"id":case.id,"identity":case.identity,"config":case.config,
            "routes":routes,"frozen":frozen,"rows":rows,"complete":true,"order_emission_allowed":false}));
    }
    Ok(json!({"schema_version":"weekend_reaction_actual_v1",
        "evidence_kind":fixture.evidence_kind,"implementation_id":implementation_id,
        "input_sha256":format!("{:x}",Sha256::digest(bytes)),"cases":cases,
        "operational_admission":false,"deployment":false}))
}
