use crate::local::{contract::*, implementation_sha256, Commit, Journal, VERSION};
mod br1_reviewer;
mod usd_ri;
use chrono::{Datelike, Duration, NaiveDate};
use serde_json::{json, Value};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "finam-local-test-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&p).unwrap();
        Self(p)
    }
    fn db(&self) -> PathBuf {
        self.0.join("session.sqlite")
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn day() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 10, 26).unwrap()
}
fn at(d: NaiveDate, h: u32, m: u32) -> i64 {
    (d.and_hms_opt(h, m, 0).unwrap() - Duration::hours(3))
        .and_utc()
        .timestamp_millis()
}
fn data(price: &str) -> Ohlcv {
    Ohlcv {
        open: price.into(),
        high: price.into(),
        low: price.into(),
        close: price.into(),
        volume: "1".into(),
    }
}
fn candle(binding: &Binding, seq: u64, start: i64, purpose: Purpose) -> Candle {
    Candle {
        instrument: binding.instrument.clone(),
        source_sequence: seq,
        provider_id: binding.provider_id.clone(),
        binding_sha256: binding.sha256().unwrap(),
        start_ms: start,
        end_ms: start + 600_000,
        received_ms: start + 600_001,
        durable_ms: start + 600_002,
        source_processed_ms: start + 600_900,
        purpose,
        data: data("100.1"),
        construction: Construction::ExternalM10V1 {
            raw_sha256: "a".repeat(64),
        },
    }
}
fn manifest() -> Manifest {
    let seeds = INSTRUMENTS
        .into_iter()
        .map(|symbol| {
            let binding = Binding {
                instrument: symbol.into(),
                provider_id: format!("fixture:{symbol}"),
                catalog_sha256: "b".repeat(64),
                basis: MappingBasis::FixtureOnly,
                tick: "0.01".into(),
                lot: 1,
                construction: ConstructionKind::ExternalM10V1,
            };
            let mut history = vec![];
            for date in [
                day() - Duration::days(4),
                day() - Duration::days(3),
                day() - Duration::days(2),
                day() - Duration::days(1),
            ] {
                let weekend = date.weekday().num_days_from_monday() >= 5;
                let start = if weekend {
                    at(date, 9, 50)
                } else {
                    at(date, 7, 0)
                };
                let count = if weekend { 55 } else { 101 };
                for slot in 0..count {
                    let mut c = candle(
                        &binding,
                        history.len() as u64 + 1,
                        start + slot * 600_000,
                        Purpose::ContextHistory,
                    );
                    c.data = data("100");
                    c.data.high = "102".into();
                    c.data.low = "98".into();
                    history.push(c);
                }
            }
            Seed {
                binding,
                history,
                declared_consumed_through_ms: at(day(), 7, 0),
            }
        })
        .collect();
    Manifest {
        schema: VERSION.into(),
        generation: "fixture-three-symbol-session".into(),
        implementation_sha256: implementation_sha256().unwrap(),
        evidence: Evidence::DeclaredClockReplay,
        initial_context: InitialContext::OrdinaryIngest,
        session_date: day(),
        freeze_at_ms: at(day(), 7, 0),
        seeds,
    }
}
fn event(m: &Manifest, symbol: usize, slot: u64) -> Candle {
    let s = &m.seeds[symbol];
    candle(
        &s.binding,
        s.history.len() as u64 + slot + 1,
        at(day(), 7, 0) + slot as i64 * 600_000,
        Purpose::Model,
    )
}
fn semantic(v: &Value) -> Value {
    let mut results = v["results"].clone();
    for r in results.as_array_mut().unwrap() {
        r.as_object_mut().unwrap().remove("checkpoint_sha256");
    }
    json!({"states":v["states"],"results":results})
}

#[test]
fn local_canonical_projection_is_declared_not_authority_and_hash_bound() {
    let t = Temp::new();
    let m = manifest();
    let mut journal = Journal::create(&t.db(), m.clone()).unwrap();
    let mut c = event(&m, 0, 0);
    let provenance = json!({"test":"unattested JSON, not a native capability"});
    c.construction = Construction::DeclaredCanonicalProjectionV1 {
        provenance_sha256: "a".repeat(64),
        provenance: provenance.clone(),
    };
    assert!(journal.apply(c.clone()).is_err());
    assert_eq!(journal.export().unwrap()["revision"], 0);
    c.construction = Construction::DeclaredCanonicalProjectionV1 {
        provenance_sha256: crate::local::hash(&provenance).unwrap(),
        provenance,
    };
    journal.apply(c.clone()).unwrap();
    let r = journal
        .committed_input(&c.instrument, c.source_sequence)
        .unwrap()
        .unwrap();
    assert_eq!(r.candle, c);
    assert_eq!(r.revision, 1);
    assert_eq!(r.result_ids.len(), 2);
    assert!(journal.committed_input("IMOEXF", 1).unwrap().is_none());
    assert_eq!(journal.export().unwrap()["observed_clean_session"], false);
    drop(journal);
    assert_eq!(
        Journal::open(&t.db(), m)
            .unwrap()
            .committed_input(&c.instrument, c.source_sequence)
            .unwrap()
            .unwrap()
            .result_ids,
        r.result_ids
    );
}

#[test]
fn local_checked_readback_rejects_result_index_and_checkpoint_corruption() {
    for sql in [
        "UPDATE outcomes SET source_sequence=source_sequence+1",
        "UPDATE arms SET checkpoint=zeroblob(5) WHERE instrument='IMOEXF'",
    ] {
        let t = Temp::new();
        let m = manifest();
        let c = event(&m, 0, 0);
        let mut journal = Journal::create(&t.db(), m).unwrap();
        journal.apply(c.clone()).unwrap();
        rusqlite::Connection::open(t.db())
            .unwrap()
            .execute_batch(sql)
            .unwrap();
        assert!(journal
            .committed_input("IMOEXF", c.source_sequence)
            .is_err());
    }
}

#[test]
fn local_checked_readback_rejects_stale_writer_snapshot() {
    let t = Temp::new();
    let m = manifest();
    let c = event(&m, 0, 0);
    let reader = Journal::create(&t.db(), m.clone()).unwrap();
    let mut writer = Journal::open(&t.db(), m).unwrap();
    writer.apply(c.clone()).unwrap();
    assert!(reader.committed_input("IMOEXF", c.source_sequence).is_err());
}

#[test]
fn local_six_arms_full_session_ready_flat_and_costs_not_redefined() {
    let t = Temp::new();
    let m = manifest();
    let mut j = Journal::create(&t.db(), m.clone()).unwrap();
    for slot in 0..101 {
        for instrument in 0..3 {
            j.apply(event(&m, instrument, slot)).unwrap();
        }
    }
    let v = j.export().unwrap();
    assert_eq!(v["revision"], 303);
    assert_eq!(v["states"].as_array().unwrap().len(), 6);
    assert_eq!(v["results"].as_array().unwrap().len(), 606);
    for state in v["states"].as_array().unwrap() {
        assert_eq!(state["ready"], true);
        assert_eq!(state["full_grid_flat"], true);
        assert_eq!(state["bars_processed"], 101);
        assert_eq!(state["observed_clean_session"], false);
    }
    assert!(v["results"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| !r["marks"].as_array().unwrap().is_empty()));
    assert_eq!(v["operational_activation"], false);
    drop(j);
    assert_eq!(Journal::open(&t.db(), m).unwrap().export().unwrap(), v);
}

#[test]
fn local_restart_pending_duplicate_and_stable_mark_ids() {
    let t = Temp::new();
    let other = Temp::new();
    let m = manifest();
    let mut uninterrupted = Journal::create(&other.db(), m.clone()).unwrap();
    let mut j = Journal::create(&t.db(), m.clone()).unwrap();
    for slot in 0..3 {
        for symbol in 0..3 {
            let c = event(&m, symbol, slot);
            uninterrupted.apply(c.clone()).unwrap();
            j.apply(c.clone()).unwrap();
            let before = j.export().unwrap();
            for state in before["states"].as_array().unwrap().iter().filter(|s| {
                s["arm"]
                    .as_str()
                    .unwrap()
                    .starts_with(&m.seeds[symbol].binding.instrument)
            }) {
                if symbol < 2 && slot == 0 {
                    assert!(
                        !state["model"]["model"]["state"]["pending"].is_null(),
                        "real reaction pending required"
                    );
                } else if symbol < 2 && slot == 1 {
                    assert!(
                        !state["model"]["model"]["state"]["position"].is_null(),
                        "real reaction position required"
                    );
                } else if symbol == 2 && slot <= 1 {
                    assert!(
                        !state["model"]["mr"].is_null(),
                        "real RI MR position required"
                    );
                }
            }
            assert!(matches!(
                j.apply(c).unwrap(),
                Commit::AlreadyCommitted { .. }
            ));
            assert_eq!(j.export().unwrap(), before);
            drop(j);
            j = Journal::open(&t.db(), m.clone()).unwrap();
        }
    }
    assert_eq!(
        semantic(&j.export().unwrap()),
        semantic(&uninterrupted.export().unwrap())
    );
    assert_eq!(j.export().unwrap()["states"].as_array().unwrap().len(), 6);
}

#[test]
fn local_bad_input_conflict_gap_and_day_rollover_are_atomic() {
    let t = Temp::new();
    let m = manifest();
    let mut j = Journal::create(&t.db(), m.clone()).unwrap();
    let c = event(&m, 0, 0);
    j.apply(c.clone()).unwrap();
    let before = j.export().unwrap();
    let mut bad = c;
    bad.received_ms += 1;
    assert!(j.apply(bad).is_err());
    assert!(j.apply(event(&m, 0, 2)).is_err());
    let mut bad = event(&m, 0, 1);
    bad.data.high = "1".into();
    assert!(j.apply(bad).is_err());
    let mut bad = event(&m, 0, 1);
    bad.binding_sha256 = "0".repeat(64);
    assert!(j.apply(bad).is_err());
    let mut bad = event(&m, 0, 1);
    bad.start_ms += 86_400_000;
    bad.end_ms += 86_400_000;
    assert!(j.apply(bad).is_err());
    assert_eq!(j.export().unwrap(), before);
    assert!(Journal::create(&t.db(), m.clone()).is_err());
    drop(j);
    assert_eq!(Journal::open(&t.db(), m).unwrap().export().unwrap(), before);
}

#[test]
fn local_mapping_source_calendar_generation_and_clock_bindings() {
    let t = Temp::new();
    let m = manifest();
    let j = Journal::create(&t.db(), m.clone()).unwrap();
    drop(j);
    let mut bad = m.clone();
    bad.generation.push('x');
    assert!(Journal::open(&t.db(), bad).is_err());
    let mut bad = m.clone();
    bad.implementation_sha256 = "0".repeat(64);
    assert!(bad.validate().is_err());
    let mut bad = m.clone();
    bad.seeds[1].binding.provider_id = "USDRUBF@GUESS".into();
    assert!(bad.validate().is_err());
    let mut bad = m.clone();
    bad.seeds[1].declared_consumed_through_ms -= 1;
    assert!(bad.validate().is_err());
    let mut bad = m.clone();
    bad.freeze_at_ms += 1;
    assert!(bad.validate().is_err());
    let mut bad = m.clone();
    bad.session_date = NaiveDate::from_ymd_opt(2026, 11, 4).unwrap();
    bad.freeze_at_ms = at(bad.session_date, 7, 0);
    assert!(bad.validate().is_err());
    let mut v = serde_json::to_value(m).unwrap();
    v["evidence"] = json!("observed");
    assert!(serde_json::from_value::<Manifest>(v).is_err());
}

#[test]
fn local_sparse_actual_m1_exact_decimal_no_first_or_last_no_padding() {
    let m = manifest();
    let mut b = m.seeds[0].binding.clone();
    b.construction = ConstructionKind::SparseM1V1;
    let mut c = candle(&b, 1, at(day(), 7, 0), Purpose::Model);
    c.data = data("100.1");
    c.data.volume = "0.3".into();
    let mut a = data("100.1");
    a.volume = "0.1".into();
    let mut z = data("100.1");
    z.volume = "0.2".into();
    let minutes = vec![
        Minute {
            start_ms: c.start_ms + 60_000,
            data: a,
        },
        Minute {
            start_ms: c.start_ms + 480_000,
            data: z,
        },
    ];
    c.construction = Construction::SparseM1V1 {
        raw_sha256: "f".repeat(64),
        range_from_ms: c.start_ms,
        range_to_ms: c.end_ms,
        declared_complete_response: true,
        actual_m1: minutes.clone(),
    };
    c.validate(&b).unwrap();
    assert_eq!(c.start_ms, at(day(), 7, 0));
    assert_eq!(minutes.len(), 2);
    for mode in 0..7 {
        let mut bad = c.clone();
        if let Construction::SparseM1V1 {
            actual_m1,
            range_to_ms,
            declared_complete_response,
            ..
        } = &mut bad.construction
        {
            match mode {
                0 => actual_m1.clear(),
                1 => actual_m1.push(actual_m1[1].clone()),
                2 => actual_m1.reverse(),
                3 => *range_to_ms -= 60_000,
                4 => *declared_complete_response = false,
                5 => actual_m1[1].start_ms = bad.end_ms,
                _ => bad.data.volume = "0.300000001".into(),
            }
        }
        assert!(bad.validate(&b).is_err(), "negative {mode}");
    }
}

#[test]
fn local_unready_is_explicit_and_does_not_block_other_instruments() {
    let t = Temp::new();
    let mut m = manifest();
    m.seeds[1].history.clear();
    let mut j = Journal::create(&t.db(), m.clone()).unwrap();
    for symbol in 0..3 {
        j.apply(event(&m, symbol, 0)).unwrap();
    }
    let v = j.export().unwrap();
    for s in v["states"].as_array().unwrap() {
        if s["arm"].as_str().unwrap().starts_with("USDRUBF") {
            assert_eq!(s["ready"], false);
            assert_eq!(s["full_grid_flat"], false);
        } else {
            assert_eq!(s["ready"], true);
        }
    }
    for r in v["results"].as_array().unwrap() {
        if r["arm"].as_str().unwrap().starts_with("USDRUBF") {
            assert!(r["marks"].as_array().unwrap().is_empty());
        }
    }
    drop(j);
    assert_eq!(Journal::open(&t.db(), m).unwrap().export().unwrap(), v);
}

#[test]
fn local_not_ready_weekend_policy_can_differ_without_cross_branch_mutation() {
    let t = Temp::new();
    let mut m = manifest();
    // Weekday arm sees no anchors; weekend-context arm has two real declared
    // weekend grids. Seed sequences are rebuilt explicitly.
    for seed in m.seeds.iter_mut().take(2) {
        seed.history
            .retain(|c| c.start_ms >= at(day() - Duration::days(2), 9, 50));
        for (i, c) in seed.history.iter_mut().enumerate() {
            c.source_sequence = i as u64 + 1;
        }
    }
    let mut j = Journal::create(&t.db(), m.clone()).unwrap();
    j.apply(event(&m, 0, 0)).unwrap();
    let v = j.export().unwrap();
    let states = v["states"].as_array().unwrap();
    assert_eq!(
        states
            .iter()
            .find(|s| s["arm"] == "IMOEXF/weekday_only")
            .unwrap()["ready"],
        false
    );
    assert_eq!(
        states
            .iter()
            .find(|s| s["arm"] == "IMOEXF/weekend_context_no_trades")
            .unwrap()["ready"],
        true
    );
    assert_eq!(
        states
            .iter()
            .find(|s| s["arm"] == "RIZ6/runtime_accepted_only")
            .unwrap()["bars_processed"],
        0
    );
}

#[test]
fn local_stale_writer_and_stale_export_require_reopen() {
    let t = Temp::new();
    let m = manifest();
    let mut first = Journal::create(&t.db(), m.clone()).unwrap();
    let mut second = Journal::open(&t.db(), m.clone()).unwrap();
    first.apply(event(&m, 0, 0)).unwrap();
    assert!(second.apply(event(&m, 1, 0)).is_err());
    assert!(second.export().is_err());
    drop(second);
    let mut second = Journal::open(&t.db(), m.clone()).unwrap();
    second.apply(event(&m, 1, 0)).unwrap();
    assert!(first.apply(event(&m, 2, 0)).is_err());
}

#[test]
fn local_corrupted_checkpoint_result_and_log_are_not_silently_reset() {
    for table in ["arms", "outcomes", "inputs", "meta"] {
        let t = Temp::new();
        let m = manifest();
        let mut j = Journal::create(&t.db(), m.clone()).unwrap();
        j.apply(event(&m, 0, 0)).unwrap();
        drop(j);
        let conn = rusqlite::Connection::open(t.db()).unwrap();
        conn.execute_batch(match table {
            "arms" => "UPDATE arms SET checkpoint=x'00' WHERE id='IMOEXF/weekday_only';",
            "outcomes" => "UPDATE outcomes SET result_sha='bad';",
            "inputs" => "UPDATE inputs SET input_sha='bad';",
            _ => "UPDATE meta SET revision=0;",
        })
        .unwrap();
        drop(conn);
        assert!(Journal::open(&t.db(), m).is_err(), "corrupt {table}");
        assert!(t.db().exists());
    }
}

#[test]
fn local_r1_sql_sequence_and_arm_mutants_fail_at_open_without_repair() {
    for mutant in ["sequence", "arm"] {
        let t = Temp::new();
        let m = manifest();
        let first = event(&m, 0, 0);
        let mut j = Journal::create(&t.db(), m.clone()).unwrap();
        j.apply(first.clone()).unwrap();
        drop(j);
        let corrupted = t.0.join("index-mutant.sqlite");
        std::fs::copy(t.db(), &corrupted).unwrap();
        let conn = rusqlite::Connection::open(&corrupted).unwrap();
        let payloads = |conn: &rusqlite::Connection| {
            let mut q = conn
                .prepare("SELECT result,result_sha FROM outcomes ORDER BY result")
                .unwrap();
            q.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
                .unwrap()
                .collect::<rusqlite::Result<Vec<_>>>()
                .unwrap()
        };
        let original = payloads(&conn);
        if mutant == "sequence" {
            assert_eq!(first.source_sequence, 313); // reviewer's exact mutant
            conn.execute("UPDATE outcomes SET source_sequence=314 WHERE arm='IMOEXF/weekday_only' AND source_sequence=313", []).unwrap();
        } else {
            conn.execute(
                "UPDATE outcomes SET arm='USDRUBF/weekday_only' WHERE arm='IMOEXF/weekday_only'",
                [],
            )
            .unwrap();
        }
        assert_eq!(payloads(&conn), original);
        assert_eq!(
            conn.query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))
                .unwrap(),
            "ok"
        );
        assert_eq!(
            conn.query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |r| r
                .get::<_, u64>(
                0
            ))
            .unwrap(),
            0
        );
        drop(conn);
        let error = Journal::open(&corrupted, m.clone())
            .err()
            .expect("must reject SQL key/payload mismatch");
        assert!(
            error
                .to_string()
                .contains("outcome SQL index/payload mismatch"),
            "{error:#}"
        );
        let conn = rusqlite::Connection::open(&corrupted).unwrap();
        assert_eq!(payloads(&conn), original);
        assert_eq!(
            conn.query_row("SELECT revision FROM meta", [], |r| r.get::<_, u64>(0))
                .unwrap(),
            1
        );
        if mutant == "sequence" {
            assert_eq!(
                conn.query_row(
                    "SELECT source_sequence FROM outcomes WHERE arm='IMOEXF/weekday_only'",
                    [],
                    |r| r.get::<_, u64>(0)
                )
                .unwrap(),
                314
            );
        } else {
            assert_eq!(
                conn.query_row(
                    "SELECT COUNT(*) FROM outcomes WHERE arm='USDRUBF/weekday_only'",
                    [],
                    |r| r.get::<_, u64>(0)
                )
                .unwrap(),
                1
            );
        }
        // Unmodified control: reopen + exact duplicate + next valid input.
        let mut control = Journal::open(&t.db(), m.clone()).unwrap();
        assert!(matches!(
            control.apply(first).unwrap(),
            Commit::AlreadyCommitted { .. }
        ));
        control.apply(event(&m, 0, 1)).unwrap();
        assert_eq!(control.export().unwrap()["revision"], 2);
    }
}

#[test]
fn local_r1_ready_off_tick_spellings_reject_before_model_or_commit() {
    let t = Temp::new();
    let m = manifest();
    let mut j = Journal::create(&t.db(), m.clone()).unwrap();
    let before = j.export().unwrap();
    assert!(before["states"]
        .as_array()
        .unwrap()
        .iter()
        .all(|s| s["ready"] == true));
    for wire in ["0.0000000000000000000000000100001e28", "100.001"] {
        let mut bad = event(&m, 0, 0);
        bad.data = data(wire);
        let error = j.apply(bad).unwrap_err().to_string();
        assert!(
            error.contains(if wire.contains('e') {
                "decimal wire grammar"
            } else {
                "off-tick OHLC"
            }),
            "{error}"
        );
        assert_eq!(j.export().unwrap(), before);
    }
    j.apply(event(&m, 0, 0)).unwrap();
    assert_eq!(j.export().unwrap()["revision"], 1);
}

#[test]
fn local_r1_sparse_lossy_inputs_and_accumulator_overflow_are_atomic() {
    let mut m = manifest();
    for seed in &mut m.seeds {
        seed.history.clear();
    }
    m.seeds[0].binding.construction = ConstructionKind::SparseM1V1;
    let make = |volumes: &[&str], declared: &str, price: &str, aggregate_price: &str| {
        let mut c = event(&m, 0, 0);
        c.data = data(aggregate_price);
        c.data.volume = declared.into();
        c.construction = Construction::SparseM1V1 {
            raw_sha256: "f".repeat(64),
            range_from_ms: c.start_ms,
            range_to_ms: c.end_ms,
            declared_complete_response: true,
            actual_m1: volumes
                .iter()
                .enumerate()
                .map(|(i, v)| {
                    let mut data = data(price);
                    data.volume = (*v).into();
                    Minute {
                        start_ms: c.start_ms + i as i64 * 60_000,
                        data,
                    }
                })
                .collect(),
        };
        c
    };
    // These values DO parse exactly: accumulation cannot silently erase .2.
    let cases = [
        make(
            &["10000000000000000000000000000.1"],
            "10000000000000000000000000000",
            "100.1",
            "100.1",
        ),
        make(
            &["4000000000000000000000000000.1"; 2],
            "8000000000000000000000000000",
            "100.1",
            "100.1",
        ),
        make(
            &["100000000000000000000.0000000001"],
            "100000000000000000000.00000000",
            "100.1",
            "100.1",
        ),
        make(
            &["0.00000000000000000000000001001e28"],
            "100",
            "100.1",
            "100.1",
        ),
        make(&["1"], "1", "0.00000000000000000000000001001e28", "100"),
        make(
            &["100000000000000000000000000000"; 2],
            "1",
            "100.1",
            "100.1",
        ),
    ];
    let diagnostics = [
        "M1/M10 exact aggregation mismatch",
        "M1/M10 exact aggregation mismatch",
        "decimal scale bound",
        "decimal wire grammar",
        "decimal wire grammar",
        "exact volume sum range exceeded",
    ];
    for (bad, diagnostic) in cases.into_iter().zip(diagnostics) {
        let t = Temp::new();
        let mut j = Journal::create(&t.db(), m.clone()).unwrap();
        let before = j.export().unwrap();
        let error = j.apply(bad).unwrap_err().to_string();
        assert!(error.contains(diagnostic), "{error}");
        assert_eq!(j.export().unwrap(), before);
        drop(j);
        assert_eq!(
            Journal::open(&t.db(), m.clone()).unwrap().export().unwrap(),
            before
        );
    }
    for (volumes, total) in [
        (vec!["0.1", "0.2"], "0.3"),
        (
            vec!["4000000000000000000000000000.1"; 2],
            "8000000000000000000000000000.2",
        ),
        (
            vec!["80000000000000000000000000000.1"; 2],
            "160000000000000000000000000000.2",
        ),
    ] {
        let t = Temp::new();
        let mut j = Journal::create(&t.db(), m.clone()).unwrap();
        j.apply(make(&volumes, total, "100.1", "100.1")).unwrap();
        let expected = j.export().unwrap();
        assert_eq!(expected["revision"], 1);
        drop(j);
        assert_eq!(
            Journal::open(&t.db(), m.clone()).unwrap().export().unwrap(),
            expected
        );
    }
}

#[test]
fn local_crash_child() {
    let Ok(root) = std::env::var("FINAM_LOCAL_TEST_CHILD") else {
        return;
    };
    let root = PathBuf::from(root);
    let m: Manifest =
        serde_json::from_slice(&std::fs::read(root.join("manifest.json")).unwrap()).unwrap();
    let c: Candle =
        serde_json::from_slice(&std::fs::read(root.join("candle.json")).unwrap()).unwrap();
    Journal::open(&root.join("session.sqlite"), m)
        .unwrap()
        .apply(c)
        .unwrap();
    panic!("crash frontier not reached");
}

#[cfg(unix)]
#[test]
fn local_real_sigkill_four_atomic_frontiers_no_partial_or_reemission() {
    use std::{
        process::{Command, Stdio},
        time::{Duration, Instant},
    };
    for frontier in [
        "after_input",
        "after_first_arm",
        "before_commit",
        "after_commit",
    ] {
        let t = Temp::new();
        let m = manifest();
        let c = event(&m, 0, 0);
        drop(Journal::create(&t.db(), m.clone()).unwrap());
        // Only synthetic fixtures. Created in a unique test-owned directory.
        std::fs::write(t.0.join("manifest.json"), serde_json::to_vec(&m).unwrap()).unwrap();
        std::fs::write(t.0.join("candle.json"), serde_json::to_vec(&c).unwrap()).unwrap();
        let marker = t.0.join("kill-ready");
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "local_tests::local_crash_child", "--nocapture"])
            .env("FINAM_LOCAL_TEST_CHILD", &t.0)
            .env("FINAM_LOCAL_TEST_CRASH_FRONTIER", frontier)
            .env("FINAM_LOCAL_TEST_MARKER", &marker)
            .stdout(Stdio::null())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(30);
        while !marker.exists() && Instant::now() < deadline && child.try_wait().unwrap().is_none() {
            std::thread::sleep(Duration::from_millis(20));
        }
        let reached = marker.exists();
        let _ = Command::new("kill")
            .args(["-KILL", &child.id().to_string()])
            .status();
        let status = child.wait().unwrap();
        assert!(
            reached && !status.success(),
            "frontier {frontier} not reached"
        );
        assert_eq!(std::fs::read_to_string(marker).unwrap(), frontier);
        let mut reopened = Journal::open(&t.db(), m.clone()).unwrap();
        let before = reopened.export().unwrap();
        assert_eq!(
            before["revision"],
            if frontier == "after_commit" { 1 } else { 0 }
        );
        let receipt = reopened.apply(c.clone()).unwrap();
        assert_eq!(
            matches!(receipt, Commit::AlreadyCommitted { .. }),
            frontier == "after_commit"
        );
        assert_eq!(reopened.export().unwrap()["revision"], 1);
        let after = reopened.export().unwrap();
        assert!(matches!(
            reopened.apply(c).unwrap(),
            Commit::AlreadyCommitted { .. }
        ));
        assert_eq!(reopened.export().unwrap(), after);
    }
}
