// Synthetic source-component qualification; no provider/network readiness claim.
use super::plan::{hash, identity, INSTRUMENTS};
use super::*;
use crate::{
    primary::{
        calendar::{tests::Dir, RawCalendar, ScheduleSnapshot},
        WireFrame,
    },
    timing::{advance_fixture_clock, now_ms, with_fixture_clock},
    transport::StopHandle,
};
use broker_finam::ShadowInstrument;
use chrono::{DateTime, Datelike, Utc};
use finam_shadow_models::local::contract::*;
use serde_json::{json, Value};
mod references;

const T: i64 = 1_793_592_000_000; // synthetic 02-Nov first session, never current DATA
const ACCOUNT: &str = "SYNTHETIC_ACCOUNT_ONLY";
fn at(t: i64) -> DateTime<Utc> {
    DateTime::from_timestamp_millis(t).unwrap()
}
fn go(t: i64) {
    advance_fixture_clock((t - now_ms()).try_into().unwrap());
}
fn manifest() -> Manifest {
    let mut m = crate::primary::tests::models_bridge::manifest();
    m.initial_context = InitialContext::UsdRiFirstSession;
    m.generation = "synthetic-usd-ri-native-component".into();
    m.seeds[0].history.clear();
    for s in &mut m.seeds[1..] {
        let ri = s.binding.instrument == "RIZ6";
        s.binding.provider_id = if ri { "RIZ6@RTSX" } else { "USDRUBF@RTSX" }.into();
        s.binding.basis = MappingBasis::DeclaredCatalogSnapshot;
        s.binding.catalog_sha256 = crate::primary::models_bridge::MAPPING_SHA256.into();
        s.binding.tick = if ri { "10" } else { "0.01" }.into();
        s.history
            .retain(|c| at(c.start_ms).weekday().num_days_from_monday() < 5);
        let pin = s.binding.sha256().unwrap();
        for (n, c) in s.history.iter_mut().enumerate() {
            c.source_sequence = n as u64 + 1;
            c.provider_id = s.binding.provider_id.clone();
            c.binding_sha256 = pin.clone();
            c.received_ms = T - 3600000;
            c.durable_ms = c.received_ms + 1;
            c.source_processed_ms = c.durable_ms + 1;
            c.data = Ohlcv {
                open: if ri { "100000" } else { "100" }.into(),
                high: if ri { "104000" } else { "102" }.into(),
                low: if ri { "96000" } else { "98" }.into(),
                close: if ri { "100000" } else { "100" }.into(),
                volume: "1".into(),
            };
        }
    }
    m
}
fn plan(m: &Manifest) -> Plan {
    Plan {
        schema: "finam.shadow.usd-ri.source-component.v1".into(),
        manifest_sha256: m.sha256().unwrap(),
        account_sha256: crate::seed::digest(ACCOUNT.as_bytes()),
        start_not_before_ms: T,
        start_before_ms: T + 540000,
        stop_at_ms: T + 14 * 600000,
    }
}
fn snapshot(i: ShadowInstrument, suffix: &str, v: Value) -> ScheduleSnapshot {
    let raw = v.to_string();
    ScheduleSnapshot {
        method: "GET".into(),
        endpoint: format!(
            "https://api.finam.ru/v1/assets/{}{suffix}",
            i.venue_symbol()
        ),
        requested_at_ms: T - 1000,
        received_at_ms: T - 950,
        status: 200,
        transport_complete: true,
        declared_body_bytes: raw.len() as u64,
        response_sha256: crate::seed::digest(raw.as_bytes()),
        raw_body: raw,
    }
}
fn evidence(i: ShadowInstrument) -> InstrumentEvidence {
    let ri = i == ShadowInstrument::Riz6;
    let asset = snapshot(
        i,
        "",
        json!({"id":i.asset_id(),"ticker":i.symbol(),"name":if ri{"RTS-12.26"}else{"USDRUBF"},"mic":"RTSX","type":"FUTURES","board":"FUT","decimals":if ri{0}else{2},"min_step":if ri{"10"}else{"1"},"quote_currency":if ri{"USD"}else{"RUB"},"lot_size":{"value":if ri{"1.0"}else{"1000.0"}},"expiration_date":if ri{json!({"year":2026,"month":12,"day":17})}else{Value::Null},"future_details":{"contract_size":{"value":if ri{"1.0"}else{"1000.0"}},"expiration_date":if ri{json!("2026-12-16T21:00:00Z")}else{Value::Null}}}),
    );
    let params = snapshot(
        i,
        "/params",
        json!({"symbol":i.venue_symbol(),"account_id":ACCOUNT,"trade_lot_size":"1","is_tradable":true}),
    );
    let interval = |kind: &str, a, b| json!({"type":kind,"interval":{"start_time":at(a).to_rfc3339(),"end_time":at(b).to_rfc3339()}});
    let calendar = RawCalendar {
        schema_version: 1,
        previous_session_date: "2026-11-01".into(),
        snapshots: vec![snapshot(
            i,
            "/schedule",
            json!({"symbol":i.venue_symbol(),"sessions":[interval("CORE_TRADING",T-86400000,T-86400000+101*600000),interval("CLOSED",T-86400000+101*600000,T),interval("CORE_TRADING",T,T+101*600000)]}),
        )],
    };
    InstrumentEvidence {
        instrument: i,
        asset,
        params,
        calendar,
        bootstrap_reference_start_ms: T - 86400000,
    }
}
fn prepared() -> Prepared {
    let m = manifest();
    Prepared::admit(plan(&m), m, INSTRUMENTS.map(evidence).into()).unwrap()
}
fn row(t: i64, price: &str) -> Value {
    json!({"timestamp":at(t).to_rfc3339(),"open":{"value":price},"high":{"value":price},"low":{"value":price},"close":{"value":price},"volume":{"value":"2"}})
}
fn frame(
    i: ShadowInstrument,
    rows: Vec<Value>,
    t: i64,
    owner: &crate::socket::primary::Subscription,
) -> WireFrame {
    let text=json!({"type":"DATA","subscription_type":"BARS","subscription_key":"component-fixture","payload":{"symbol":i.venue_symbol(),"bars":rows}}).to_string();
    let mut f = WireFrame::normalize_for(i, &text, 1, "component-fixture", at(t)).unwrap();
    f.owner = Some(owner.owner.clone());
    f
}
fn attach(c: &mut Cohort) -> Vec<crate::socket::primary::Subscription> {
    INSTRUMENTS
        .iter()
        .map(|i| {
            let s = crate::socket::primary::fixture_subscription_for(*i, 1, T);
            c.bind_subscription(&s).unwrap();
            s
        })
        .collect()
}
fn feed_slot(c: &mut Cohort, subs: &[crate::socket::primary::Subscription], slot: i64) {
    let start = T + slot * 600000;
    for minute in if slot == 0 { 0 } else { 1 }..=10 {
        let t = start + minute * 60000 + 100;
        go(t);
        for (n, i) in INSTRUMENTS.iter().enumerate() {
            let price = if *i == ShadowInstrument::Riz6 {
                "103000"
            } else {
                "100"
            };
            c.ingest(*i, frame(*i, vec![row(t - 100, price)], t, &subs[n]))
                .unwrap();
        }
    }
    go(start + 601100);
}
#[test]
fn two_native_sources_four_arms_sql_readback_source_last_and_readonly_reader() {
    with_fixture_clock(T, || {
        let d = Dir::new();
        let output = d.0.join("cohort");
        let mut c = prepared().create(&output, StopHandle::default()).unwrap();
        let subs = attach(&mut c);
        for slot in 0..2 {
            feed_slot(&mut c, &subs, slot);
            for i in INSTRUMENTS {
                assert!(matches!(c.next(i).unwrap(), Progress::Committed(_)));
                assert_eq!(c.frontier(i), Some(T + (slot + 1) * 600000));
            }
        }
        let out = c.export().unwrap();
        assert_eq!(out["inputs"].as_array().unwrap().len(), 4);
        assert_eq!(out["results"].as_array().unwrap().len(), 8);
        assert!(out["states"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|s| s["arm"].as_str().unwrap().starts_with("IMOEXF/"))
            .all(|s| s["ready"] == false && s["bars_processed"] == 0));
        let done = c.finish().unwrap();
        assert_eq!(done.evidence["held"], false);
        let before = std::fs::read(output.join("models.sqlite")).unwrap();
        let r = reader::inspect(&output).unwrap();
        assert_eq!(r["status"], "COMPONENT_PREFIX_VERIFIED");
        assert_eq!(r["inputs"], 4);
        assert_eq!(std::fs::read(output.join("models.sqlite")).unwrap(), before);
        with_fixture_clock(T, || {
            assert!(prepared().create(&output, StopHandle::default()).is_err())
        });
    });
}
#[test]
fn duplicate_same_physical_frame_cannot_repeat_model_callback() {
    with_fixture_clock(T, || {
        let d = Dir::new();
        let mut c = prepared()
            .create(&d.0.join("c"), StopHandle::default())
            .unwrap();
        let subs = attach(&mut c);
        feed_slot(&mut c, &subs, 0);
        let f = frame(
            INSTRUMENTS[0],
            vec![row(T + 600000, "100")],
            T + 600100,
            &subs[0],
        );
        c.ingest(INSTRUMENTS[0], f).unwrap();
        assert!(matches!(
            c.next(INSTRUMENTS[0]).unwrap(),
            Progress::Committed(_)
        ));
        assert!(matches!(c.next(INSTRUMENTS[0]).unwrap(), Progress::Waiting));
        assert_eq!(c.export().unwrap()["revision"], 1);
    });
}
#[test]
fn foreign_physical_owner_and_foreign_instrument_latch_shared_stop() {
    for foreign_instrument in [false, true] {
        with_fixture_clock(T, || {
            let d = Dir::new();
            let stop = StopHandle::default();
            let mut c = prepared().create(&d.0.join("c"), stop.clone()).unwrap();
            let subs = attach(&mut c);
            let foreign = crate::socket::primary::fixture_subscription_for(INSTRUMENTS[0], 1, T);
            let frame = if foreign_instrument {
                frame(INSTRUMENTS[1], vec![row(T, "103000")], T, &subs[1])
            } else {
                frame(INSTRUMENTS[0], vec![row(T, "100")], T, &foreign)
            };
            assert!(c.ingest(INSTRUMENTS[0], frame).is_err());
            assert!(stop.is_stopped());
            assert_eq!(c.export().unwrap()["revision"], 0);
        });
    }
}
#[test]
fn post_sql_stop_and_expired_budget_leave_durable_prefix_without_source_advance() {
    for stop_case in [true, false] {
        with_fixture_clock(T, || {
            let d = Dir::new();
            let output = d.0.join("c");
            let mut c = prepared().create(&output, StopHandle::default()).unwrap();
            let subs = attach(&mut c);
            feed_slot(&mut c, &subs, 0);
            if stop_case {
                c.stop_after_sql = true;
            } else {
                c.after_sql_delay_ms = 1000;
            }
            assert!(c.next(INSTRUMENTS[0]).is_err());
            assert_eq!(c.frontier(INSTRUMENTS[0]), Some(T));
            assert_eq!(c.export().unwrap()["revision"], 1);
            c.finish().unwrap();
            let report = reader::inspect(&output).unwrap();
            assert_eq!(report["status"], "HELD_COMPONENT_EVIDENCE");
        });
    }
}
#[test]
fn stop_before_sql_and_wrong_subscription_never_emit_model_input() {
    with_fixture_clock(T, || {
        let d = Dir::new();
        let stop = StopHandle::default();
        let mut c = prepared().create(&d.0.join("a"), stop.clone()).unwrap();
        attach(&mut c);
        stop.stop();
        assert!(c.next(INSTRUMENTS[0]).is_err());
        assert_eq!(c.export().unwrap()["revision"], 0);
        let mut c = prepared()
            .create(&d.0.join("b"), StopHandle::default())
            .unwrap();
        let foreign = crate::socket::primary::fixture_subscription(1, T);
        assert!(c.bind_subscription(&foreign).is_err());
        assert_eq!(c.export().unwrap()["revision"], 0);
    });
}
#[test]
fn identity_checks_actual_account_exact_asset_tick_currency_lots_and_expiry() {
    with_fixture_clock(T, || {
        let account = crate::seed::digest(ACCOUNT.as_bytes());
        for i in INSTRUMENTS {
            let e = evidence(i);
            let public = identity(&e, &account, T).unwrap();
            let bytes = serde_json::to_string(&public).unwrap();
            assert!(!bytes.contains(ACCOUNT));
            for (field, bad) in [
                ("id", json!("999")),
                ("min_step", json!("5")),
                ("quote_currency", json!("EUR")),
                ("lot_size", json!({"value":"0"})),
                ("type", json!("STOCKS")),
            ] {
                let mut e = evidence(i);
                let mut v: Value = serde_json::from_str(&e.asset.raw_body).unwrap();
                v[field] = bad;
                e.asset = snapshot(i, "", v);
                assert!(identity(&e, &account, T).is_err(), "{field}");
            }
            let mut e = evidence(i);
            e.params = snapshot(
                i,
                "/params",
                json!({"symbol":i.venue_symbol(),"account_id":"OTHER_SYNTHETIC","trade_lot_size":"1","is_tradable":true}),
            );
            assert!(identity(&e, &account, T).is_err());
            let mut e = evidence(i);
            e.asset.transport_complete = false;
            assert!(identity(&e, &account, T).is_err());
            let e = evidence(i);
            assert!(identity(&e, &account, T + 300001).is_err());
            let mut e = evidence(i);
            let mut params: Value = serde_json::from_str(&e.params.raw_body).unwrap();
            params.as_object_mut().unwrap().remove("is_tradable");
            params["tradeable"] = json!(true);
            e.params = snapshot(i, "/params", params.clone());
            assert!(identity(&e, &account, T).is_ok());
            params["is_tradable"] = json!(false);
            e.params = snapshot(i, "/params", params);
            assert!(identity(&e, &account, T).is_err());
        }
        let mut e = evidence(ShadowInstrument::Riz6);
        let mut v: Value = serde_json::from_str(&e.asset.raw_body).unwrap();
        v["future_details"]["expiration_date"] = json!("2026-12-17T21:00:00Z");
        e.asset = snapshot(e.instrument, "", v);
        assert!(identity(&e, &account, T).is_err());
    });
}
#[test]
fn admission_rejects_duplicate_calendars_wrong_context_short_day_and_late_entry() {
    with_fixture_clock(T, || {
        let m = manifest();
        assert!(Prepared::admit(
            plan(&m),
            m.clone(),
            vec![evidence(INSTRUMENTS[0]), evidence(INSTRUMENTS[0])]
        )
        .is_err());
        let mut e = evidence(INSTRUMENTS[1]);
        let mut v: Value = serde_json::from_str(&e.calendar.snapshots[0].raw_body).unwrap();
        v["sessions"][2]["interval"]["end_time"] = json!(at(T + 40 * 600000).to_rfc3339());
        e.calendar.snapshots[0] = snapshot(e.instrument, "/schedule", v);
        assert!(Prepared::admit(plan(&m), m.clone(), vec![evidence(INSTRUMENTS[0]), e]).is_err());
        let mut wrong = m.clone();
        wrong.initial_context = InitialContext::OrdinaryIngest;
        assert!(Prepared::admit(plan(&wrong), wrong, INSTRUMENTS.map(evidence).into()).is_err());
        let stale = prepared();
        go(T + 300001);
        let d = Dir::new();
        assert!(stale
            .create(&d.0.join("stale"), StopHandle::default())
            .is_err());
        assert!(!d.0.join("stale").exists());
        go(T + 540000);
        assert!(Prepared::admit(plan(&m), m, INSTRUMENTS.map(evidence).into()).is_err());
    });
}
#[test]
fn numeric_only_wire_and_legacy_default_do_not_gain_physical_authority() {
    for i in INSTRUMENTS {
        let text=json!({"type":"DATA","subscription_type":"BARS","subscription_key":"x","payload":{"symbol":i.venue_symbol(),"bars":[row(T,if i==ShadowInstrument::Riz6{"100000"}else{"100.01"})]}}).to_string();
        assert!(WireFrame::normalize(&text, 1, "x", at(T)).is_err());
        assert!(WireFrame::normalize_for(i, &text, 1, "x", at(T)).is_ok());
        let mut v: Value = serde_json::from_str(&text).unwrap();
        v["payload"]["bars"][0]["open"]["value"] = json!("100.001");
        assert!(WireFrame::normalize_for(i, &v.to_string(), 1, "x", at(T)).is_err());
    }
}
#[test]
fn reader_refuses_control_mutation_and_partial_tail() {
    with_fixture_clock(T, || {
        let d = Dir::new();
        let output = d.0.join("c");
        let mut c = prepared().create(&output, StopHandle::default()).unwrap();
        let subs = attach(&mut c);
        feed_slot(&mut c, &subs, 0);
        c.next(INSTRUMENTS[0]).unwrap();
        c.finish().unwrap();
        reader::inspect(&output).unwrap();
        let path = output.join("source-control.jsonl");
        let original = std::fs::read(&path).unwrap();
        let mut rows: Vec<Value> = std::str::from_utf8(&original)
            .unwrap()
            .lines()
            .map(|s| serde_json::from_str(s).unwrap())
            .collect();
        rows.last_mut().unwrap()["payload"]["data"]["frontiers"][0]["committed"] = json!(99);
        // Rehashing the control chain cannot repair a false source/SQL frontier.
        let mut head = "0".repeat(64);
        let mut changed = String::new();
        for row in rows.iter_mut() {
            row["payload"]["previous_sha256"] = json!(head);
            row["sha256"] = json!(hash(&row["payload"]).unwrap());
            head = row["sha256"].as_str().unwrap().into();
            changed.push_str(&row.to_string());
            changed.push('\n');
        }
        std::fs::write(&path, changed).unwrap();
        assert!(reader::inspect(&output).is_err());
        std::fs::write(&path, &original[..original.len() - 1]).unwrap();
        assert!(reader::inspect(&output).is_err());
    });
}

fn price(i: ShadowInstrument, slot: i64) -> &'static str {
    if i == ShadowInstrument::Usdrubf {
        match slot {
            0 => "100.1",
            2 => "102",
            3 => "100.4",
            _ => "100",
        }
    } else if slot <= 10 {
        "103000"
    } else if slot == 11 {
        "104000"
    } else {
        "105000"
    }
}
#[test]
fn synthetic_native_four_arm_actions_and_retained_readonly_prefix() {
    with_fixture_clock(T, || {
        let temp = Dir::new();
        let output = std::env::var_os("FINAM_ADDITIONAL_SOURCE_FIXTURE_DIR")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| temp.0.join("actions"));
        let mut c = prepared().create(&output, StopHandle::default()).unwrap();
        let subs = attach(&mut c);
        for slot in 0..13 {
            let start = T + slot * 600000;
            for minute in if slot == 0 { 0 } else { 1 }..=10 {
                let t = start + minute * 60000 + 100;
                go(t);
                for (n, i) in INSTRUMENTS.iter().enumerate() {
                    let p = price(*i, slot + i64::from(minute == 10));
                    c.ingest(*i, frame(*i, vec![row(t - 100, p)], t, &subs[n]))
                        .unwrap();
                }
            }
            go(start + 601100);
            // Alternate interleaving without conflating the independent frontiers.
            for i in if slot % 2 == 0 {
                INSTRUMENTS
            } else {
                [INSTRUMENTS[1], INSTRUMENTS[0]]
            } {
                assert!(matches!(c.next(i).unwrap(), Progress::Committed(_)));
            }
        }
        let out = c.export().unwrap();
        assert_eq!(out["revision"], 26);
        assert_eq!(out["results"].as_array().unwrap().len(), 52);
        let states = out["states"].as_array().unwrap();
        assert_eq!(
            states
                .iter()
                .filter(|s| s["ready"] == true && s["bars_processed"] == 13)
                .count(),
            4
        );
        let marks: Vec<_> = out["results"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|r| r["marks"].as_array().unwrap())
            .collect();
        assert!(!marks.is_empty());
        assert!(states
            .iter()
            .filter(|s| s["arm"].as_str().unwrap().starts_with("USDRUBF/"))
            .any(|s| !s["model"]["model"]["state"]["trades"]
                .as_array()
                .unwrap()
                .is_empty()));
        c.finish().unwrap();
        let report = reader::inspect(&output).unwrap();
        assert_eq!(report["inputs"], 26);
        if std::env::var_os("FINAM_ADDITIONAL_SOURCE_FIXTURE_DIR").is_some() {
            std::fs::write(
                output.join("test-export.json"),
                serde_json::to_vec_pretty(&out).unwrap(),
            )
            .unwrap();
            std::fs::write(
                output.join("reader-result.json"),
                serde_json::to_vec_pretty(&report).unwrap(),
            )
            .unwrap();
        }
    });
}

#[test]
fn sparse_native_rest_exception_keeps_its_origin_and_checked_source_last() {
    use broker_finam::sparse_m10::*;
    with_fixture_clock(T, || {
        let d = Dir::new();
        let output = d.0.join("rest");
        let mut c = prepared().create(&output, StopHandle::default()).unwrap();
        let subs = attach(&mut c);
        let i = INSTRUMENTS[0];
        for minute in 0..=10 {
            let t = T + minute * 60000 + 100;
            go(t);
            if minute != 3 {
                c.ingest(i, frame(i, vec![row(t - 100, "100")], t, &subs[0]))
                    .unwrap();
            }
        }
        go(T + 601100);
        let Progress::ProofRequired { reason, .. } = c.next(i).unwrap() else {
            panic!("sparse proof required")
        };
        let raw=json!({"symbol":i.venue_symbol(),"bars":(0..10).map(|n|row(T+n*60000,"100")).collect::<Vec<_>>()}).to_string();
        let evidence = ClosedM1SnapshotEvidenceV1 {
            policy: OBSERVED_M1_POLICY_V1.into(),
            start: at(T),
            end: at(T + 600000),
            parts: vec![ClosedM1RestPartV1 {
                method: "GET".into(),
                endpoint: format!(
                    "https://api.finam.ru/v1/instruments/{}/bars",
                    i.venue_symbol()
                ),
                symbol: i.venue_symbol().into(),
                timeframe: "TIME_FRAME_M1".into(),
                start: at(T),
                end: at(T + 600000),
                requested_at: at(T + 601050),
                received_at: at(now_ms()),
                status: 200,
                transport_complete: true,
                declared_body_bytes: Some(raw.len() as u64),
                response_sha256: crate::seed::digest(raw.as_bytes()),
                raw_body: raw,
            }],
        };
        let source = AdmittedClosedM1Snapshot::admit_for(
            i,
            evidence,
            &ClosedM1RequestPlan::new(at(T), at(T + 600000), vec![at(T + 600000)]).unwrap(),
            at(now_ms()),
        )
        .unwrap();
        assert!(c.prove(i, &source, reason).unwrap().is_some());
        assert_eq!(c.frontier(i), Some(T + 600000));
        let out = c.export().unwrap();
        assert_eq!(
            out["inputs"][0]["candle"]["construction"]["provenance"]["canonical_receipt"]["origin"],
            "REST_EXCEPTION"
        );
        c.finish().unwrap();
        assert_eq!(reader::inspect(&output).unwrap()["inputs"], 1);
    });
}

// Reviewer-only tests: expected to FAIL on 3c18bc8d R0.
// Apply to a COPY of the candidate, never to the frozen source tree.
#[test]
fn reviewer_rejects_positive_but_wrong_catalog_reference_lot_and_contract_size() {
    with_fixture_clock(T, || {
        let account = crate::seed::digest(ACCOUNT.as_bytes());
        for i in INSTRUMENTS {
            let mut e = evidence(i);
            let mut asset: Value = serde_json::from_str(&e.asset.raw_body).unwrap();
            asset["lot_size"]["value"] = json!("777.0");
            e.asset = snapshot(i, "", asset);
            assert!(
                identity(&e, &account, T).is_err(),
                "wrong reference lot: {:?}",
                i
            );

            let mut e = evidence(i);
            let mut asset: Value = serde_json::from_str(&e.asset.raw_body).unwrap();
            asset["future_details"]["contract_size"]["value"] = json!("777.0");
            e.asset = snapshot(i, "", asset);
            assert!(
                identity(&e, &account, T).is_err(),
                "wrong contract size: {:?}",
                i
            );
        }
    });
}

#[test]
fn reviewer_rejects_rehashed_reference_event_without_admitted_rest_source() {
    with_fixture_clock(T, || {
        let d = Dir::new();
        let output = d.0.join("forged-reference");
        let mut c = prepared().create(&output, StopHandle::default()).unwrap();
        let subs = attach(&mut c);
        feed_slot(&mut c, &subs, 0);
        assert!(matches!(
            c.next(INSTRUMENTS[0]).unwrap(),
            Progress::Committed(_)
        ));
        c.finish().unwrap();
        assert_eq!(
            reader::inspect(&output).unwrap()["status"],
            "COMPONENT_PREFIX_VERIFIED"
        );

        let path = output.join("source-control.jsonl");
        let original = std::fs::read_to_string(&path).unwrap();
        let mut rows: Vec<Value> = original
            .lines()
            .map(|s| serde_json::from_str(s).unwrap())
            .collect();
        assert_eq!(rows[0]["payload"]["kind"], "preparation");
        assert_eq!(rows[1]["payload"]["kind"], "subscription");
        assert_eq!(rows[2]["payload"]["kind"], "subscription");
        // Fabricated reference has no admitted REST evidence. Hash syntax
        // and the entire control hash chain are otherwise perfectly valid.
        rows.insert(
            3,
            json!({"payload":{"index":0,"previous_sha256":"","kind":"reference",
            "data":{"instrument":"usdrubf","sha256":"f".repeat(64),"evidence":{}}},"sha256":""}),
        );
        let mut head = "0".repeat(64);
        let mut text = String::new();
        for (n, row) in rows.iter_mut().enumerate() {
            row["payload"]["index"] = json!(n);
            row["payload"]["previous_sha256"] = json!(head);
            head = hash(&row["payload"]).unwrap();
            row["sha256"] = json!(head);
            text.push_str(&row.to_string());
            text.push('\n');
        }
        std::fs::write(&path, text).unwrap();
        assert!(
            reader::inspect(&output).is_err(),
            "forged reference passed semantic readback"
        );
    });
}
