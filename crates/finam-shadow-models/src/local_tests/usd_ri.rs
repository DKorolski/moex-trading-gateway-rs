//! Synthetic first-session preparation only. No FINAM, credentials or live claim.
use super::*;

const MAPPING: &str = "9e06d4ff4c66a9f88534bd4f46404b12588a1c716f1c64373a97174401376b0c";

fn prepared() -> Manifest {
    let mut m = manifest();
    m.initial_context = InitialContext::UsdRiFirstSession;
    m.generation = "fixture-usd-ri-first-session".into();
    m.seeds[0].history.clear();
    for seed in &mut m.seeds[1..] {
        let ri = seed.binding.instrument == "RIZ6";
        seed.binding.provider_id = if ri { "RIZ6@RTSX" } else { "USDRUBF@RTSX" }.into();
        seed.binding.basis = MappingBasis::DeclaredCatalogSnapshot;
        seed.binding.catalog_sha256 = MAPPING.into();
        seed.binding.tick = if ri { "10" } else { "0.01" }.into();
        seed.history.retain(|c| {
            chrono::DateTime::from_timestamp_millis(c.start_ms)
                .unwrap()
                .weekday()
                .num_days_from_monday()
                < 5
        });
        let pin = seed.binding.sha256().unwrap();
        for (i, c) in seed.history.iter_mut().enumerate() {
            c.source_sequence = i as u64 + 1;
            c.provider_id = seed.binding.provider_id.clone();
            c.binding_sha256 = pin.clone();
            c.received_ms = at(day() - Duration::days(1), 21, 0);
            c.durable_ms = c.received_ms + 1;
            c.source_processed_ms = c.durable_ms + 1;
            c.data = data(if ri { "100000" } else { "100" });
            c.data.high = if ri { "104000" } else { "102" }.into();
            c.data.low = if ri { "96000" } else { "98" }.into();
        }
    }
    m
}
fn selected(states: &[Value]) -> Vec<&Value> {
    states
        .iter()
        .filter(|s| {
            s["arm"]
                .as_str()
                .is_some_and(|id| id.starts_with("USDRUBF/") || id.starts_with("RIZ6/"))
        })
        .collect()
}
fn input(m: &Manifest, symbol: usize, slot: u64) -> Candle {
    let mut c = event(m, symbol, slot);
    let price = if symbol == 1 {
        if slot == 0 {
            "100.1"
        } else if slot == 2 {
            "102"
        } else if slot == 3 {
            "100.4"
        } else {
            "100"
        }
    } else if slot <= 10 {
        "103000"
    } else if slot == 11 {
        "104000"
    } else {
        "105000"
    };
    c.data = data(price);
    c
}

#[test]
fn usd_ri_explicit_first_session_readies_four_arms_without_retiming() {
    let m = prepared();
    let original = m.clone();
    let states = Journal::initial_states(&m).unwrap();
    assert_eq!(m, original);
    let four = selected(&states);
    assert_eq!(four.len(), 4);
    assert!(four
        .iter()
        .all(|s| s["ready"] == true && s["bars_processed"] == 0));
    assert!(states
        .iter()
        .filter(|s| s["arm"].as_str().unwrap().starts_with("IMOEXF/"))
        .all(|s| s["ready"] == false));
    assert!(states
        .iter()
        .all(|s| s["evidence"] == "declared_clock_replay"
            && s["order_emission_allowed"] == false
            && s["observed_clean_session"] == false));
}

#[test]
fn usd_ri_ordinary_ingest_remains_unready_for_late_historical_collection() {
    let mut m = prepared();
    m.initial_context = InitialContext::OrdinaryIngest;
    assert!(selected(&Journal::initial_states(&m).unwrap())
        .iter()
        .all(|s| s["ready"] == false));
}

#[test]
fn usd_ri_exact_catalog_tick_and_contract_are_required() {
    for field in ["provider_id", "tick", "catalog_sha256", "basis", "lot"] {
        for instrument in [1, 2] {
            let mut v = serde_json::to_value(prepared()).unwrap();
            v["seeds"][instrument]["binding"][field] = match field {
                "tick" => json!("0.5"),
                "lot" => json!(1000),
                "basis" => json!("fixture_only"),
                "catalog_sha256" => json!("a".repeat(64)),
                _ => json!("RTS-12.26@RTSX"),
            };
            let m: Manifest = serde_json::from_value(v).unwrap();
            assert!(m.validate().is_err(), "{field}/{instrument}");
        }
    }
}

#[test]
fn usd_ri_missing_context_never_creates_a_database() {
    for symbol in [1, 2] {
        let mut m = prepared();
        m.seeds[symbol].history.clear();
        let t = Temp::new();
        assert!(Journal::create(&t.db(), m).is_err());
        assert!(!t.db().exists());
    }
    let mut m = prepared();
    m.seeds[0].history = manifest().seeds[0].history.clone();
    assert!(m.validate().is_err());
}

#[test]
fn usd_ri_receipts_at_cutoff_and_unknown_clocks_are_refused() {
    for symbol in [1, 2] {
        for offset in [0, 1] {
            let mut m = prepared();
            let cutoff = m.freeze_at_ms;
            for c in &mut m.seeds[symbol].history {
                c.received_ms = cutoff + offset;
                c.durable_ms = cutoff + offset;
                c.source_processed_ms = cutoff + offset;
            }
            assert!(Journal::initial_states(&m).is_err());
        }
    }
    let mut m = prepared();
    m.seeds[2].history[0].received_ms = m.seeds[2].history[0].end_ms - 1;
    assert!(Journal::initial_states(&m).is_err());
}

#[test]
fn usd_ri_short_history_cannot_invent_paired_anchors() {
    for symbol in [1, 2] {
        let mut m = prepared();
        let last = m.seeds[symbol].history.last().unwrap().start_ms;
        m.seeds[symbol]
            .history
            .retain(|c| c.start_ms >= last - 100 * 600000);
        for (i, c) in m.seeds[symbol].history.iter_mut().enumerate() {
            c.source_sequence = i as u64 + 1;
        }
        assert!(Journal::initial_states(&m).is_err());
    }
}

#[test]
fn usd_ri_sql_prefix_reopen_and_duplicate_are_exact() {
    let t = Temp::new();
    let m = prepared();
    let mut j = Journal::create(&t.db(), m.clone()).unwrap();
    for slot in 0..4 {
        for symbol in [1, 2] {
            j.apply(input(&m, symbol, slot)).unwrap();
        }
    }
    let before = j.export().unwrap();
    assert_eq!(before["revision"], 8);
    assert_eq!(before["results"].as_array().unwrap().len(), 16);
    drop(j);
    let mut j = Journal::open(&t.db(), m.clone()).unwrap();
    assert_eq!(j.export().unwrap(), before);
    assert!(matches!(
        j.apply(input(&m, 1, 3)).unwrap(),
        Commit::AlreadyCommitted { revision: 7, .. }
    ));
    assert!(matches!(
        j.apply(input(&m, 2, 3)).unwrap(),
        Commit::AlreadyCommitted { revision: 8, .. }
    ));
    assert_eq!(j.export().unwrap(), before);
    j.apply(input(&m, 1, 4)).unwrap();
    j.apply(input(&m, 2, 4)).unwrap();
    assert_eq!(j.export().unwrap()["revision"], 10);
}

#[test]
fn usd_ri_synthetic_model_actions_are_not_suppressed_by_initialization() {
    let t = Temp::new();
    let m = prepared();
    let mut j = Journal::create(&t.db(), m.clone()).unwrap();
    let mut candles = Vec::new();
    for slot in 0..13 {
        for symbol in [1, 2] {
            let c = input(&m, symbol, slot);
            j.apply(c.clone()).unwrap();
            candles.push(c);
        }
    }
    let out = j.export().unwrap();
    assert_eq!(out["revision"], 26);
    let usd = out["states"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|s| s["arm"].as_str().unwrap().starts_with("USDRUBF/"));
    for s in usd {
        assert!(!s["model"]["model"]["state"]["trades"]
            .as_array()
            .unwrap()
            .is_empty());
    }
    let ri = out["states"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|s| s["arm"].as_str().unwrap().starts_with("RIZ6/"));
    for s in ri {
        assert!(!s["model"]["bo"].is_null(), "{}", s["model"]);
    }
    assert!(out["results"]
        .as_array()
        .unwrap()
        .iter()
        .all(|r| r["order_emission_allowed"] == false && r["observed_clean_session"] == false));
    // Explicit test-only artifact capture. Never part of the production CLI.
    if let Some(path) = std::env::var_os("FINAM_USDRI_PREPARATION_FIXTURES_DIR") {
        let path = PathBuf::from(path);
        std::fs::create_dir(&path).unwrap();
        for (name, value) in [
            ("manifest.json", serde_json::to_value(&m).unwrap()),
            ("candles.json", serde_json::to_value(&candles).unwrap()),
            ("test-export.json", out),
        ] {
            std::fs::write(path.join(name), serde_json::to_vec(&value).unwrap()).unwrap();
        }
        drop(j);
        std::fs::copy(t.db(), path.join("fixture.sqlite")).unwrap();
    }
}

#[test]
fn usd_ri_foreign_input_or_source_gap_leaves_sql_prefix_unchanged() {
    let t = Temp::new();
    let m = prepared();
    let mut j = Journal::create(&t.db(), m.clone()).unwrap();
    let before = j.export().unwrap();
    let mut c = input(&m, 1, 0);
    c.provider_id = "IMOEXF@RTSX".into();
    assert!(j.apply(c).is_err());
    assert!(j.apply(input(&m, 2, 1)).is_err());
    assert_eq!(j.export().unwrap(), before);
    j.apply(input(&m, 1, 0)).unwrap();
    let before = j.export().unwrap();
    let mut changed = input(&m, 1, 0);
    changed.data = data("100.2");
    assert!(j.apply(changed).is_err());
    assert_eq!(j.export().unwrap(), before);
}

#[test]
fn usd_ri_generation_rebind_cannot_reopen_an_existing_prefix() {
    let t = Temp::new();
    let m = prepared();
    let mut j = Journal::create(&t.db(), m.clone()).unwrap();
    j.apply(input(&m, 2, 0)).unwrap();
    drop(j);
    let mut wrong = m.clone();
    wrong.generation.push_str("-other");
    assert!(Journal::open(&t.db(), wrong).is_err());
    assert_eq!(
        Journal::open(&t.db(), m).unwrap().export().unwrap()["revision"],
        1
    );
}
