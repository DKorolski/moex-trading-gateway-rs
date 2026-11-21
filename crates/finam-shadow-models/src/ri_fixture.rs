//! Input-only RI synthetic prefix export. Declared receipt clocks here are
//! fixture clocks, never evidence of a FINAM delivery or a complete session.
use crate::ri::{local, selected_identity, RiSession, Step, PROFILE_SHA256, VERSION};
use crate::ri_context::{ContextInput, ContextPolicy, ReceiptBarrier, RiContextStore};
use crate::{BarEvent, DataOrigin};
use anyhow::{ensure, Result};
use chrono::{Datelike, NaiveDate, Timelike};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

pub const MAX_INPUT_BYTES: usize = 8 * 1024 * 1024;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Fixture {
    schema_version: String,
    evidence_kind: String,
    cases: Vec<Case>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    id: String,
    day: NaiveDate,
    warmup: Vec<BarEvent>,
    events: Vec<BarEvent>,
}

fn validate(bars: &[BarEvent], day: NaiveDate, warmup: bool) -> Result<()> {
    let mut last = None;
    for b in bars {
        let dt = local(b.close_time_utc)?;
        let start = if dt.date() < NaiveDate::from_ymd_opt(2026, 7, 14).unwrap() {
            9
        } else {
            7
        };
        ensure!(
            b.symbol == "RIZ6"
                && last.map_or(true, |t| b.close_time_utc > t)
                && dt.weekday().num_days_from_monday() < 5
                && dt.minute() % 10 == 0
                && dt.second() == 0
                && dt.hour() >= start
                && (dt.hour() < 23 || dt.minute() <= 40),
            "invalid RI fixture symbol/order/grid"
        );
        ensure!(
            [b.o, b.h, b.l, b.close, b.v].iter().all(|v| v.is_finite())
                && b.l > 0.
                && b.v >= 0.
                && b.l <= b.o
                && b.o <= b.h
                && b.l <= b.close
                && b.close <= b.h,
            "invalid RI fixture OHLCV"
        );
        ensure!(
            if warmup {
                dt.date() < day && matches!(b.origin, DataOrigin::History | DataOrigin::HistoryGap)
            } else {
                dt.date() == day && b.origin == DataOrigin::Live
            },
            "RI fixture day/origin mismatch"
        );
        last = Some(b.close_time_utc);
    }
    Ok(())
}

pub fn run(bytes: &[u8], implementation_id: &str) -> Result<Value> {
    ensure!(
        bytes.len() <= MAX_INPUT_BYTES && !implementation_id.trim().is_empty(),
        "RI input/identity bound"
    );
    let f: Fixture = serde_json::from_slice(bytes)?;
    ensure!(
        f.schema_version == "ri_prospective_fixture_v1"
            && f.evidence_kind == "synthetic_model_clock",
        "RI accepts explicit synthetic fixture evidence only"
    );
    ensure!(
        !f.cases.is_empty() && f.cases.len() <= 64,
        "RI case count bound"
    );
    let mut ids = BTreeSet::new();
    let mut rows = Vec::new();
    let mut audit: Vec<Value> = vec![];
    for c in f.cases {
        ensure!(
            !c.id.trim().is_empty() && c.id.len() <= 128 && ids.insert(c.id.clone()),
            "invalid/duplicate RI case"
        );
        ensure!(
            c.warmup.len() <= 1000 && !c.events.is_empty() && c.events.len() <= 101,
            "RI fixture row bound"
        );
        validate(&c.warmup, c.day, true)?;
        validate(&c.events, c.day, false)?;
        for policy in [
            ContextPolicy::RuntimeAcceptedOnly,
            ContextPolicy::RiConfirmedClosingContextV1,
        ] {
            let id = selected_identity(policy, "finam-ri-synthetic-fixture-v1".into())?;
            let mut store = RiContextStore::new(id.clone())?;
            for b in &c.warmup {
                let clock = b.close_time_utc + 600;
                store.ingest(
                    ContextInput {
                        sequence: store.last_sequence() + 1,
                        bar: b.clone(),
                        received_at_utc: Some(clock),
                        durable_at_utc: Some(clock),
                    },
                    clock,
                )?;
            }
            let cutoff = id.cutoff(c.day);
            let barrier = ReceiptBarrier {
                receipt_stream: id.receipt_stream.clone(),
                through_utc: cutoff,
                last_sequence: store.last_sequence(),
                ingress_sha256: store.ingress_sha256()?,
                attested_at_utc: cutoff,
            };
            store.freeze(c.day, cutoff, &barrier)?;
            let generation = format!("{}::{policy:?}", c.id);
            let mut session = RiSession::new(&store, c.day, generation.clone())?;
            let mut restart_checks = 0;
            for (index, b) in c.events.iter().enumerate() {
                let clock = b.close_time_utc + 600;
                let input = ContextInput {
                    sequence: store.last_sequence() + 1,
                    bar: b.clone(),
                    received_at_utc: Some(clock),
                    durable_at_utc: Some(clock),
                };
                store.ingest(input.clone(), clock)?;
                let out: Step = session.process(&store, input, clock)?;
                rows.push(json!({"case":c.id,"policy":policy,"index":index,
                    "bar_ts_utc":b.close_time_utc,"state":out.state}));
                audit.push(json!({"case":c.id,"policy":policy,"index":index,"step":out}));
                if index == c.events.len() / 2 {
                    let before = session.state();
                    let frozen = store.frozen(c.day).cloned();
                    let bytes = session.checkpoint_bytes()?;
                    store = RiContextStore::restore(&id, &store.checkpoint_bytes()?, clock)?;
                    session = RiSession::restore(&id, &generation, c.day, &bytes, clock)?;
                    ensure!(
                        session.state() == before && store.frozen(c.day) == frozen.as_ref(),
                        "RI restart changed state/context"
                    );
                    restart_checks += 1;
                }
            }
            ensure!(
                restart_checks == 1 && !session.order_emission_allowed(),
                "RI fixture isolation failed"
            );
        }
    }
    Ok(
        json!({"schema_version":"finam_ri_fixture_actual_v1","evidence_kind":f.evidence_kind,
        "adapter_version":VERSION,"implementation_id":implementation_id,
        "input_sha256":format!("{:x}",Sha256::digest(bytes)),"selected_profile_sha256":PROFILE_SHA256,
        "calendar_source":"exact_selected_profile","rows":rows,"audit":audit,
        "session_scope":"bounded_synthetic_prefix_not_full_session",
        "operational_admission":false,"order_emission_allowed":false,"deployment":false}),
    )
}
