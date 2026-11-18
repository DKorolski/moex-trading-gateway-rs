//! Synthetic native-admission tests. No network, credential or observed claim.
use super::*;
mod reviewer_cases;
use crate::primary::models_bridge::{ImoexfModelBridge, MAPPING_SHA256};
use finam_shadow_models::local::{contract::*, implementation_sha256, Commit, VERSION};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

const T: i64 = 1_793_592_000_000; // 02.11.2026 07:00 MSK, actual session cutoff
const C: i64 = T + 600_000;
static NEXT: AtomicU64 = AtomicU64::new(0);

struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "finam-bridge-{}-{}-{}",
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
        self.0.join("bridge.sqlite")
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
pub(crate) fn manifest() -> Manifest {
    use chrono::{Datelike, Duration, NaiveDate};
    let day = NaiveDate::from_ymd_opt(2026, 11, 2).unwrap();
    let mut seeds = Vec::new();
    for symbol in INSTRUMENTS {
        let binding = Binding {
            instrument: symbol.into(),
            provider_id: if symbol == "IMOEXF" {
                "IMOEXF@RTSX".into()
            } else {
                format!("fixture:{symbol}")
            },
            catalog_sha256: if symbol == "IMOEXF" {
                MAPPING_SHA256.into()
            } else {
                "b".repeat(64)
            },
            basis: if symbol == "IMOEXF" {
                MappingBasis::DeclaredCatalogSnapshot
            } else {
                MappingBasis::FixtureOnly
            },
            tick: if symbol == "IMOEXF" {
                "0.5".into()
            } else {
                "0.01".into()
            },
            lot: 1,
            construction: ConstructionKind::ExternalM10V1,
        };
        let mut history = Vec::new();
        for offset in (1..=4).rev() {
            let date = day - Duration::days(offset);
            let weekend = date.weekday().num_days_from_monday() >= 5;
            let start = (date
                .and_hms_opt(if weekend { 9 } else { 7 }, if weekend { 50 } else { 0 }, 0)
                .unwrap()
                - Duration::hours(3))
            .and_utc()
            .timestamp_millis();
            for slot in 0..if weekend { 55 } else { 101 } {
                let start = start + slot * 600_000;
                history.push(Candle {
                    instrument: symbol.into(),
                    source_sequence: history.len() as u64 + 1,
                    provider_id: binding.provider_id.clone(),
                    binding_sha256: binding.sha256().unwrap(),
                    start_ms: start,
                    end_ms: start + 600_000,
                    received_ms: start + 600_100,
                    durable_ms: start + 600_200,
                    source_processed_ms: start + 600_900,
                    purpose: Purpose::ContextHistory,
                    data: Ohlcv {
                        open: "100".into(),
                        high: "102".into(),
                        low: "98".into(),
                        close: "100".into(),
                        volume: "1".into(),
                    },
                    construction: Construction::ExternalM10V1 {
                        raw_sha256: "a".repeat(64),
                    },
                });
            }
        }
        seeds.push(Seed {
            binding,
            history,
            declared_consumed_through_ms: T,
        });
    }
    Manifest {
        schema: VERSION.into(),
        generation: "synthetic-native-imoexf-consumer".into(),
        implementation_sha256: implementation_sha256().unwrap(),
        evidence: Evidence::DeclaredClockReplay,
        session_date: day,
        freeze_at_ms: T,
        seeds,
    }
}
fn cfg() -> WsPrimaryConfig {
    let mut raw = config();
    raw["base"]["observation_start_utc_ms"] = json!(T);
    raw["base"]["run_deadline_utc_ms"] = json!(T + 101 * 600_000 + 10_000);
    raw["primary"]["delivery"] = json!(DeliveryPolicy::ObservedBoRestTailRecoveryV6);
    WsPrimaryConfig::parse(&serde_json::to_vec(&raw).unwrap()).unwrap()
}
fn p(config: &WsPrimaryConfig) -> PrimaryProcessor {
    PrimaryProcessor::from_fixture(
        config,
        instrument(),
        (0..101).map(|i| T + i * 600_000).collect(),
        1,
        T - 100,
    )
    .unwrap()
}
fn source_row(ms: i64, price: &str) -> Value {
    json!({"timestamp":at(ms).to_rfc3339(),"open":{"value":price},
        "high":{"value":price},"low":{"value":price},"close":{"value":price},
        "volume":{"value":"2"}})
}
fn admitted(p: &mut PrimaryProcessor, slot: i64) -> Box<AdmittedCanonicalM10> {
    let start = T + slot * 600_000;
    for i in if slot == 0 { 0 } else { 1 }..=10 {
        let ms = start + i * 60_000;
        p.ingest(frame(vec![source_row(ms, "100.5")], ms + 100), ms + 100)
            .unwrap();
    }
    ready(p, start + 601_100)
}
fn consume(b: &mut ImoexfModelBridge, p: &mut PrimaryProcessor, slot: i64) -> Commit {
    let input = admitted(p, slot);
    crate::timing::with_fixture_clock(T + slot * 600_000 + 601_200, || {
        b.consume(p, *input).unwrap()
    })
}
fn open(path: &Path, m: &Manifest, c: &WsPrimaryConfig) -> ImoexfModelBridge {
    ImoexfModelBridge::open(path, m.clone(), c).unwrap()
}

#[test]
fn native_two_arm_commit_reopen_and_exact_redelivery_emit_no_marks() {
    let t = Temp::new();
    let m = manifest();
    let c = cfg();
    let mut b = ImoexfModelBridge::create(&t.db(), m.clone(), &c).unwrap();
    let mut source = p(&c);
    let first = consume(&mut b, &mut source, 0);
    let Commit::Committed {
        revision,
        result_ids,
    } = first
    else {
        panic!()
    };
    assert_eq!(revision, 1);
    assert_eq!(result_ids.len(), 2);
    assert_eq!(source.counters().committed, 1);
    let exported = b.export().unwrap();
    assert_eq!(exported["revision"], 1);
    let input = &exported["inputs"][0]["candle"];
    assert_eq!(
        input["construction"]["kind"],
        "declared_canonical_projection_v1"
    );
    assert_eq!(
        input["construction"]["provenance"]["canonical_receipt"]["origin"],
        "WS_LIVE"
    );
    assert_eq!(input["data"]["volume"], "20");
    assert_eq!(exported["observed_clean_session"], false);
    assert!(exported["results"]
        .as_array()
        .unwrap()
        .iter()
        .all(|r| r["evidence"] == "declared_clock_replay" && r["order_emission_allowed"] == false));
    drop(b);
    let mut b = open(&t.db(), &m, &c);
    assert_eq!(b.export().unwrap(), exported);
    // A new synthetic processor produces the EXACT same receipt, but a new
    // ownership token. The capability must belong to THIS processor.
    let mut redelivery = p(&c);
    let duplicate = consume(&mut b, &mut redelivery, 0);
    let Commit::AlreadyCommitted {
        revision,
        result_ids: ids,
    } = duplicate
    else {
        panic!()
    };
    assert_eq!(revision, 1);
    assert_eq!(ids, result_ids);
    assert_eq!(redelivery.counters().committed, 1);
    assert_eq!(b.export().unwrap(), exported);
    consume(&mut b, &mut redelivery, 1);
    assert_eq!(b.export().unwrap()["revision"], 2);
}

#[test]
fn foreign_capability_and_wrong_config_do_not_write_or_advance() {
    let t = Temp::new();
    let m = manifest();
    let c = cfg();
    let mut b = ImoexfModelBridge::create(&t.db(), m.clone(), &c).unwrap();
    let mut one = p(&c);
    let mut two = p(&c);
    let input = admitted(&mut one, 0);
    let _other = admitted(&mut two, 0);
    let error =
        crate::timing::with_fixture_clock(C + 1200, || b.consume(&mut two, *input)).unwrap_err();
    assert!(error.to_string().contains("precommit admission"));
    assert_eq!(b.export().unwrap()["revision"], 0);
    assert_eq!(one.counters().committed, 0);
    assert_eq!(two.counters().committed, 0);
    drop(b);
    let mut b = open(&t.db(), &m, &c);
    let mut changed = c.clone();
    changed.base.run_id = "different-source".into();
    let mut source = p(&changed);
    let input = admitted(&mut source, 0);
    assert!(
        crate::timing::with_fixture_clock(C + 1200, || b.consume(&mut source, *input))
            .unwrap_err()
            .to_string()
            .contains("configuration/policy")
    );
    assert_eq!(b.export().unwrap()["revision"], 0);
    assert_eq!(source.counters().committed, 0);
}

#[test]
fn source_deadline_checked_before_and_after_sql_no_fake_rollback() {
    let t = Temp::new();
    let m = manifest();
    let c = cfg();
    let mut b = ImoexfModelBridge::create(&t.db(), m.clone(), &c).unwrap();
    let mut source = p(&c);
    let input = admitted(&mut source, 0);
    assert!(
        crate::timing::with_fixture_clock(C + 2000, || b.consume(&mut source, *input)).is_err()
    );
    assert_eq!(b.export().unwrap()["revision"], 0);
    assert_eq!(source.counters().committed, 0);
    drop(b);
    let mut b = open(&t.db(), &m, &c);
    b.after_durable_delay_ms = 1000;
    let mut source = p(&c);
    let input = admitted(&mut source, 0);
    let error =
        crate::timing::with_fixture_clock(C + 1200, || b.consume(&mut source, *input)).unwrap_err();
    assert!(error
        .to_string()
        .contains("DURABLE_OUTPUT_SOURCE_NOT_ADVANCED"));
    assert_eq!(source.counters().committed, 0);
    assert_eq!(b.export().unwrap()["revision"], 1);
    let input = admitted(&mut p(&c), 0);
    assert!(b
        .consume(&mut p(&c), *input)
        .unwrap_err()
        .to_string()
        .contains("bridge held"));
    drop(b);
    let mut b = open(&t.db(), &m, &c);
    let mut exact = p(&c);
    assert!(matches!(
        consume(&mut b, &mut exact, 0),
        Commit::AlreadyCommitted { .. }
    ));
    assert_eq!(b.export().unwrap()["revision"], 1);
}

#[test]
fn mapping_policy_and_reopen_source_binding_fail_closed() {
    let t = Temp::new();
    let c = cfg();
    for field in ["provider", "mapping", "tick", "basis"] {
        let mut m = manifest();
        match field {
            "provider" => m.seeds[0].binding.provider_id = "USDRUBF@RTSX".into(),
            "mapping" => m.seeds[0].binding.catalog_sha256 = "b".repeat(64),
            "tick" => m.seeds[0].binding.tick = "0.01".into(),
            _ => m.seeds[0].binding.basis = MappingBasis::FixtureOnly,
        }
        assert!(ImoexfModelBridge::create(&t.db(), m, &c).is_err());
        assert!(!t.db().exists());
    }
    let m = manifest();
    let mut wrong = c.clone();
    wrong.primary.delivery = DeliveryPolicy::StrictBoundaryV1;
    assert!(ImoexfModelBridge::create(&t.db(), m.clone(), &wrong).is_err());
    assert!(!t.db().exists());
    let mut b = ImoexfModelBridge::create(&t.db(), m.clone(), &c).unwrap();
    consume(&mut b, &mut p(&c), 0);
    drop(b);
    let mut changed = c.clone();
    changed.base.run_id = "changed-after-restart".into();
    assert!(ImoexfModelBridge::open(&t.db(), m, &changed).is_err());
}

#[test]
fn conflicting_exact_slot_is_not_replayed_or_reemitted() {
    let t = Temp::new();
    let m = manifest();
    let c = cfg();
    let mut b = ImoexfModelBridge::create(&t.db(), m.clone(), &c).unwrap();
    consume(&mut b, &mut p(&c), 0);
    let before = b.export().unwrap();
    let mut source = p(&c);
    for i in 0..=10 {
        let ms = T + i * 60_000;
        source
            .ingest(frame(vec![source_row(ms, "101")], ms + 100), ms + 100)
            .unwrap();
    }
    let input = ready(&mut source, C + 1100);
    assert!(
        crate::timing::with_fixture_clock(C + 1200, || b.consume(&mut source, *input))
            .unwrap_err()
            .to_string()
            .contains("conflicting duplicate")
    );
    assert_eq!(b.export().unwrap(), before);
    assert_eq!(source.counters().committed, 0);
}

#[test]
fn numerical_continuation_after_restart_matches_uninterrupted_semantics() {
    let a = Temp::new();
    let b = Temp::new();
    let m = manifest();
    let c = cfg();
    let mut first = ImoexfModelBridge::create(&a.db(), m.clone(), &c).unwrap();
    let mut second = ImoexfModelBridge::create(&b.db(), m.clone(), &c).unwrap();
    let mut p1 = p(&c);
    let mut p2 = p(&c);
    for slot in 0..24 {
        consume(&mut first, &mut p1, slot);
        consume(&mut second, &mut p2, slot);
        if slot == 18 {
            drop(second);
            second = open(&b.db(), &m, &c);
        }
    }
    let mut left = first.export().unwrap();
    let mut right = second.export().unwrap();
    assert_eq!(left["revision"], 24);
    // Commit/prepared times and kernel checkpoint bookkeeping may differ;
    // all model state/marks/input identity MUST agree.
    for v in [&mut left, &mut right] {
        v.as_object_mut().unwrap().remove("chain_sha256");
        for i in v["inputs"].as_array_mut().unwrap() {
            i.as_object_mut()
                .unwrap()
                .remove("local_write_prepared_at_ms");
        }
        for r in v["results"].as_array_mut().unwrap() {
            r.as_object_mut().unwrap().remove("checkpoint_sha256");
        }
    }
    assert_eq!(left, right);
    assert_eq!(p1.counters().committed, 24);
    assert_eq!(p2.counters().committed, 24);
    assert!(left["results"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| !r["marks"].as_array().unwrap().is_empty()));
}

#[test]
fn bridge_crash_child() {
    let Ok(directory) = std::env::var("FINAM_BRIDGE_TEST_CHILD") else {
        return;
    };
    let c = cfg();
    let mut b =
        ImoexfModelBridge::create(&Path::new(&directory).join("bridge.sqlite"), manifest(), &c)
            .unwrap();
    consume(&mut b, &mut p(&c), 0);
    panic!("durable-before-source crash frontier not reached");
}

#[cfg(unix)]
#[test]
fn sigkill_after_durable_before_source_completion_is_exact_readback() {
    use std::os::unix::process::ExitStatusExt;
    use std::time::{Duration, Instant};
    let t = Temp::new();
    let marker = t.0.join("durable.marker");
    let mut child = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "primary::tests::models_bridge::bridge_crash_child",
            "--nocapture",
        ])
        .env("FINAM_BRIDGE_TEST_CHILD", &t.0)
        .env("FINAM_BRIDGE_TEST_DURABLE_MARKER", &marker)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(30);
    while !marker.exists() && Instant::now() < deadline {
        assert!(
            child.try_wait().unwrap().is_none(),
            "child exited before marker"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    let reached = marker.exists();
    child.kill().unwrap();
    let status = child.wait().unwrap();
    assert!(reached, "crash frontier timeout");
    assert_eq!(status.signal(), Some(9));
    assert_eq!(
        std::fs::read(&marker).unwrap(),
        b"durable_before_source_completion"
    );
    let c = cfg();
    let mut b = open(&t.db(), &manifest(), &c);
    let before = b.export().unwrap();
    let mut source = p(&c);
    assert!(matches!(
        consume(&mut b, &mut source, 0),
        Commit::AlreadyCommitted { revision: 1, .. }
    ));
    assert_eq!(source.counters().committed, 1);
    assert_eq!(b.export().unwrap(), before);
}
