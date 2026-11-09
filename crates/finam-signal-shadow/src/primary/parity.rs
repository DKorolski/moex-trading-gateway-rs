//! T37 HISTORICAL component only: one real measured model + candle book across
//! five days, daily numeric admission owners, no production calendar/CLI/network.
//! Synthetic M1 decomposition preserves the frozen 505 M10 exactly; it is NOT
//! observed FINAM M1, a multi-day operational mode or new live timing authority.
use super::*;
use crate::{
    seed::digest,
    session::ObservedSession,
    timing::{advance_fixture_clock, now_ms, with_fixture_clock},
    transport::StopHandle,
};
use serde_json::{json, Value};
use std::{
    fs::File,
    io::{BufRead, BufReader},
    path::PathBuf,
};
mod check;
mod cohort;
mod fixture;
use fixture::{at, instrument, Fixture, GoldBar};

#[derive(Clone, Copy)]
enum Mode {
    Dense,
    Rest,
    Sparse,
}
impl Mode {
    fn name(self) -> &'static str {
        match self {
            Self::Dense => "dense-ws",
            Self::Rest => "rest-exception",
            Self::Sparse => "sparse-proof",
        }
    }
    fn sparse(self) -> bool {
        matches!(self, Self::Sparse)
    }
}
struct Dir(PathBuf);
impl Dir {
    fn new() -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        let p = std::env::temp_dir().join(format!(
            "primary-t37-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&p).unwrap();
        Self(p)
    }
}
impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn go(t: i64) {
    advance_fixture_clock((t - now_ms()).try_into().unwrap());
}
fn ingest(
    p: &mut PrimaryProcessor,
    sub: &crate::socket::primary::Subscription,
    row: Value,
    received: i64,
) {
    go(received);
    let text=json!({"type":"DATA","subscriptionType":"BARS","subscriptionKey":"t37","payload":{"symbol":"IMOEXF@RTSX","timeframe":"TIME_FRAME_M1","bars":[row]}}).to_string();
    let mut frame = WireFrame::normalize(&text, sub.generation(), "t37", at(received)).unwrap();
    // Existing cfg(test) physical identity fixture; never compiled into a CLI.
    frame.owner = Some(sub.owner.clone());
    p.ingest(frame, received).unwrap();
}
fn read_new(reader: &mut BufReader<File>) -> Vec<Value> {
    let mut rows = vec![];
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).unwrap() == 0 {
            break;
        }
        rows.push(serde_json::from_str(&line).unwrap());
    }
    rows
}

struct Run {
    rows: Vec<Value>,
    summary: Value,
}
fn run(mode: Mode) -> Run {
    let f = Fixture::new(mode);
    with_fixture_clock(f.config.base.observation_start_utc_ms, || {
        let d = Dir::new();
        let path = d.0.join("journal.jsonl");
        let mut session =
            ObservedSession::create(f.segment(), &path, StopHandle::default()).unwrap();
        let mut reader = BufReader::new(File::open(&path).unwrap());
        let mut rows = read_new(&mut reader);
        assert_eq!(rows.len(), 1);
        check::compare(
            &f.oracle["seed_after"],
            &rows[0]["observation"]["state"],
            "seed",
        )
        .unwrap();
        for (day, bars) in f.selected.chunks_exact(101).enumerate() {
            go(bars[0].label());
            let generation = day as u64 + 1;
            let mut p = PrimaryProcessor::from_config(
                &f.config,
                instrument(),
                bars.iter().map(GoldBar::label).collect(),
                generation,
                now_ms(),
            )
            .unwrap();
            let sub = crate::socket::primary::fixture_subscription(generation, now_ms());
            p.bind_subscription(&sub, now_ms()).unwrap();
            for (index, bar) in bars.iter().enumerate() {
                for i in 0..10 {
                    if mode.sparse() && i == 4 {
                        continue;
                    }
                    if matches!(mode, Mode::Dense) && index > 0 && i == 0 {
                        continue;
                    }
                    let lag = if i == 0 && index > 0 { 3250 } else { 100 };
                    ingest(&mut p, &sub, bar.minute(i), bar.label() + i * 60000 + lag);
                    assert!(matches!(
                        p.prepare(now_ms()).unwrap(),
                        CanonicalStatus::Waiting
                    ));
                }
                let close = bar.label() + 600000;
                let input = if matches!(mode, Mode::Dense) && index < 100 {
                    ingest(&mut p, &sub, bars[index + 1].minute(0), close + 100);
                    assert!(matches!(
                        p.prepare(now_ms()).unwrap(),
                        CanonicalStatus::Waiting
                    ));
                    go(close + 1100);
                    let CanonicalStatus::Ready(input) = p.prepare(now_ms()).unwrap() else {
                        panic!("dense source admission")
                    };
                    input
                } else {
                    go(close + 3100);
                    let CanonicalStatus::PendingProof { reason, .. } = p.prepare(now_ms()).unwrap()
                    else {
                        panic!("explicit proof required")
                    };
                    go(close + 3150);
                    let source = fixture::rest(bar, mode.sparse());
                    p.prepare_rest_exception(&source, reason, now_ms())
                        .unwrap()
                        .unwrap()
                };
                assert_eq!(
                    input.receipt().canonical_digest,
                    crate::input::canonical_digest(&bar.bar).unwrap()
                );
                let publication = session.canonical(&input).unwrap();
                p.committed(*input, publication.committed_at_ms).unwrap();
                rows.extend(read_new(&mut reader));
                check::compare(
                    &f.oracle["steps"][day * 101 + index]["after_feedback"],
                    check::last_state(&rows),
                    &bar.local_label(),
                )
                .unwrap();
            }
            assert!(p.next_label_ms().is_none());
            assert_eq!(p.counters().committed, 101);
        }
        session
            .finish("T37_HISTORICAL_ADMISSION_COMPONENT_NOT_EOD_QUALIFICATION")
            .unwrap();
        rows.extend(read_new(&mut reader));
        let mut summary = check::verify(&f, &rows).unwrap();
        summary["mode"] = json!(mode.name());
        summary["journal_sha256"] = json!(digest(&std::fs::read(&path).unwrap()));
        emit(&f, &path, &rows, &summary);
        Run { rows, summary }
    })
}
fn emit(f: &Fixture, journal: &std::path::Path, rows: &[Value], summary: &Value) {
    if let Some(base) = std::env::var_os("WS_PRIMARY_T37_EVIDENCE_DIR") {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        let path = PathBuf::from(base).join(format!(
            "run-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::create_dir(&path).unwrap();
        std::fs::copy(journal, path.join("journal.jsonl")).unwrap();
        let steps: Vec<_> = rows
            .iter()
            .filter(|r| r["kind"] == "model_step" || r["kind"] == "hypothetical_candle_feedback")
            .cloned()
            .collect();
        for (name, value) in [
            ("result.json", summary.clone()),
            ("config.json", serde_json::to_value(&f.config).unwrap()),
            ("model-records.json", json!(steps)),
        ] {
            let bytes = serde_json::to_vec_pretty(&value).unwrap();
            let mut out = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path.join(name))
                .unwrap();
            std::io::Write::write_all(&mut out, &bytes).unwrap();
            out.sync_all().unwrap();
        }
        println!("T37_EVIDENCE_DIR={}", path.display());
    }
}

#[test]
fn primary_admitted_505_bars_match_frozen_python_in_ws_rest_and_sparse_modes() {
    let runs = [run(Mode::Dense), run(Mode::Rest), run(Mode::Sparse)];
    for result in &runs {
        assert_eq!(result.summary["steps"], 505);
        assert_eq!(result.summary["fills"], 10);
        assert_eq!(result.summary["rounds"], 5);
    }
    assert_eq!(
        runs[0].summary["semantic_sha256"],
        runs[1].summary["semantic_sha256"]
    );
    assert_eq!(
        runs[0].summary["semantic_sha256"],
        runs[2].summary["semantic_sha256"]
    );
    assert_eq!(runs[0].summary["ws_labels"], 500);
    assert_eq!(runs[0].summary["rest_labels"], 5);
    assert_eq!(runs[1].summary["rest_labels"], 505);
    assert_eq!(runs[2].summary["sparse_labels"], 505);
}

#[test]
fn primary_parity_checker_rejects_numeric_reason_state_fill_and_commit_mutations() {
    let actual = run(Mode::Dense);
    let f = Fixture::new(Mode::Dense);
    for case in [
        "numeric",
        "reason",
        "state",
        "fill",
        "commit",
        "duplicate",
        "seed",
        "policy",
        "pending",
        "origin",
    ] {
        let mut rows = actual.rows.clone();
        match case {
            "numeric" => {
                rows.iter_mut()
                    .find(|r| r["kind"] == "canonical_source")
                    .unwrap()["event"]["receipt"]["actual_m1"][0]["high"] = json!("9999")
            }
            "reason" => {
                rows.iter_mut().find(|r| r["kind"] == "model_step").unwrap()["event"]["decision"]
                    ["reason"] = json!("wrong_reason")
            }
            "state" => {
                rows.iter_mut().find(|r| r["kind"] == "model_step").unwrap()["event"]["after"]
                    ["prev_day_range"] = json!(0)
            }
            "fill" => {
                rows.iter_mut()
                    .find(|r| r["kind"] == "hypothetical_candle_feedback")
                    .unwrap()["event"]["fill"]["price_ticks"] = json!(1)
            }
            "commit" => {
                rows.iter_mut()
                    .find(|r| r["kind"] == "canonical_commit")
                    .unwrap()["event"]["feedback_complete"] = json!(false)
            }
            "duplicate" => {
                let row = rows
                    .iter()
                    .find(|r| r["kind"] == "model_step")
                    .unwrap()
                    .clone();
                rows.insert(3, row);
            }
            "seed" => rows[0]["observation"]["state"]["prev_day_close"] = json!(0),
            "policy" => {
                rows.iter_mut()
                    .find(|r| r["kind"] == "hypothetical_candle_feedback")
                    .unwrap()["event"]["fill"]["policy"] = json!("wrong_policy")
            }
            "pending" => {
                rows.iter_mut()
                    .find(|r| r["kind"] == "hypothetical_candle_feedback")
                    .unwrap()["event"]["state_after"]["pending_entry_reason"] = json!("not_settled")
            }
            "origin" => {
                rows.iter_mut()
                    .find(|r| r["kind"] == "canonical_source")
                    .unwrap()["event"]["receipt"]["origin"] = json!("RECOVERY_BACKFILL")
            }
            _ => unreachable!(),
        }
        assert!(check::verify(&f, &rows).is_err(), "{case}");
    }
}
