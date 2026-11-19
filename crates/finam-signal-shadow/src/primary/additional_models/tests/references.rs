//! R1 SRC-1/SRC-2 regressions. Synthetic full admitted GET references only.
use super::*;
use broker_finam::sparse_m10::*;
use std::path::Path;

fn reference(i: ShadowInstrument) -> AdmittedClosedM1Snapshot {
    let start = T - 86400000 + 100 * 600000;
    let end = start + 600000;
    let price = if i == ShadowInstrument::Riz6 {
        "100000"
    } else {
        "100.01"
    };
    let body=json!({"symbol":i.venue_symbol(),"bars":(0..10).map(|n|row(start+n*60000,price)).collect::<Vec<_>>()}).to_string();
    let evidence = ClosedM1SnapshotEvidenceV1 {
        policy: OBSERVED_M1_POLICY_V1.into(),
        start: at(start),
        end: at(end),
        parts: vec![ClosedM1RestPartV1 {
            method: "GET".into(),
            endpoint: format!(
                "https://api.finam.ru/v1/instruments/{}/bars",
                i.venue_symbol()
            ),
            symbol: i.venue_symbol().into(),
            timeframe: "TIME_FRAME_M1".into(),
            start: at(start),
            end: at(end),
            requested_at: at(T - 500),
            received_at: at(T - 400),
            status: 200,
            transport_complete: true,
            declared_body_bytes: Some(body.len() as u64),
            response_sha256: crate::seed::digest(body.as_bytes()),
            raw_body: body,
        }],
    };
    AdmittedClosedM1Snapshot::admit_for(
        i,
        evidence,
        &ClosedM1RequestPlan::new(at(start), at(end), vec![at(end)]).unwrap(),
        at(T),
    )
    .unwrap()
}
fn cohort(output: &Path) {
    let mut c = prepared().create(output, StopHandle::default()).unwrap();
    let subs = attach(&mut c);
    for i in INSTRUMENTS {
        c.add_reference(i, &reference(i)).unwrap();
    }
    feed_slot(&mut c, &subs, 0);
    for i in INSTRUMENTS {
        assert!(matches!(c.next(i).unwrap(), Progress::Committed(_)));
    }
    c.finish().unwrap();
}
fn control(path: &Path) -> Vec<Value> {
    std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect()
}
fn rehash(rows: &mut [Value]) -> String {
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
    text
}
#[test]
fn exact_frozen_sizes_accept_decimal_equivalents_without_changing_order_lot() {
    with_fixture_clock(T, || {
        for i in INSTRUMENTS {
            let mut e = evidence(i);
            let mut a: Value = serde_json::from_str(&e.asset.raw_body).unwrap();
            let size = if i == ShadowInstrument::Usdrubf {
                "1000.000"
            } else {
                "1.000"
            };
            a["lot_size"]["value"] = json!(size);
            a["future_details"]["contract_size"]["value"] = json!(size);
            e.asset = snapshot(i, "", a);
            let r = identity(&e, &crate::seed::digest(ACCOUNT.as_bytes()), T).unwrap();
            assert_eq!(r.public_params["trade_lot_size"], "1");
            assert_eq!(r.public_asset["reference_lot_size"], size);
            assert_eq!(r.public_asset["contract_size"], size);
        }
    });
}
#[test]
fn full_native_add_reference_roundtrips_both_instruments_without_sql_mutation() {
    with_fixture_clock(T, || {
        let d = Dir::new();
        let output = std::env::var_os("FINAM_ADDITIONAL_REFERENCE_FIXTURE_DIR")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| d.0.join("reference"));
        cohort(&output);
        let rows = control(&output.join("source-control.jsonl"));
        assert_eq!(
            rows.iter()
                .filter(|r| r["payload"]["kind"] == "reference")
                .count(),
            2
        );
        let before = std::fs::read(output.join("models.sqlite")).unwrap();
        let report = reader::inspect(&output).unwrap();
        assert_eq!(report["status"], "COMPONENT_PREFIX_VERIFIED");
        assert_eq!(report["inputs"], 2);
        assert_eq!(report["results"], 4);
        assert_eq!(before, std::fs::read(output.join("models.sqlite")).unwrap());
        if std::env::var_os("FINAM_ADDITIONAL_REFERENCE_FIXTURE_DIR").is_some() {
            std::fs::write(
                output.join("reader-result.json"),
                serde_json::to_vec_pretty(&report).unwrap(),
            )
            .unwrap();
        }
    });
}
#[test]
fn resealed_reference_mutants_fail_full_rest_range_hash_and_chronology_validation() {
    with_fixture_clock(T, || {
        let d = Dir::new();
        let output = d.0.join("mutants");
        cohort(&output);
        let path = output.join("source-control.jsonl");
        let original = std::fs::read(&path).unwrap();
        let base = control(&path);
        let n = base
            .iter()
            .position(|r| r["payload"]["kind"] == "reference")
            .unwrap();
        let db = std::fs::read(output.join("models.sqlite")).unwrap();
        let mut rejections = Vec::new();
        let retain = std::env::var_os("FINAM_ADDITIONAL_REFERENCE_MUTANTS_DIR")
            .map(std::path::PathBuf::from);
        if let Some(p) = &retain {
            std::fs::create_dir(p).unwrap();
        }
        for case in [
            "missing_evidence",
            "empty_evidence",
            "wrong_snapshot_hash",
            "wrong_instrument",
            "corrupt_raw_body",
            "transport_incomplete",
            "http_status",
            "foreign_endpoint",
            "future_response",
            "request_after_response",
            "before_subscription",
            "after_run_deadline",
            "stale_at_use",
            "range_before_calendar",
            "range_after_cutoff",
            "changed_body_with_new_response_hash",
        ] {
            let mut rows = base.clone();
            let data = &mut rows[n]["payload"]["data"];
            match case {
                "missing_evidence" => {
                    data.as_object_mut().unwrap().remove("evidence");
                }
                "empty_evidence" => data["evidence"] = json!({}),
                "wrong_snapshot_hash" => data["sha256"] = json!("f".repeat(64)),
                "wrong_instrument" => data["instrument"] = json!("riz6"),
                "corrupt_raw_body" => data["evidence"]["parts"][0]["raw_body"] = json!("{"),
                "transport_incomplete" => {
                    data["evidence"]["parts"][0]["transport_complete"] = json!(false)
                }
                "http_status" => data["evidence"]["parts"][0]["status"] = json!(503),
                "foreign_endpoint" => {
                    data["evidence"]["parts"][0]["endpoint"] = json!("https://example.invalid/bars")
                }
                "future_response" => data["evidence"]["parts"][0]["received_at"] = json!(at(T + 1)),
                "request_after_response" => {
                    data["evidence"]["parts"][0]["requested_at"] = json!(at(T - 100))
                }
                "before_subscription" => data["recorded_at_ms"] = json!(T - 1),
                "after_run_deadline" => data["recorded_at_ms"] = json!(T + 14 * 600000),
                "stale_at_use" => data["recorded_at_ms"] = json!(T + 900001),
                "range_before_calendar" => {
                    data["evidence"]["start"] = json!(at(T - 86400000 - 60000))
                }
                "range_after_cutoff" => data["evidence"]["end"] = json!(at(T + 60000)),
                "changed_body_with_new_response_hash" => {
                    let part = &mut data["evidence"]["parts"][0];
                    let mut body: Value =
                        serde_json::from_str(part["raw_body"].as_str().unwrap()).unwrap();
                    for key in ["open", "high", "low", "close"] {
                        body["bars"][0][key] = json!({"value":"100.02"});
                    }
                    let raw = body.to_string();
                    part["raw_body"] = json!(raw);
                    part["declared_body_bytes"] = json!(raw.len());
                    part["response_sha256"] = json!(crate::seed::digest(raw.as_bytes()));
                }
                _ => unreachable!(),
            }
            let text = rehash(&mut rows);
            std::fs::write(&path, &text).unwrap();
            let error = reader::inspect(&output).unwrap_err().to_string();
            assert!(error.contains("reference"), "{case}: {error}");
            assert_eq!(db, std::fs::read(output.join("models.sqlite")).unwrap());
            if let Some(p) = &retain {
                std::fs::write(p.join(format!("{case}.jsonl")), &text).unwrap();
            }
            rejections.push(json!({"case":case,"reader_error":error,"control_chain_rehashed":true,"sql_unchanged":true}));
        }
        std::fs::write(&path, original).unwrap();
        assert_eq!(
            reader::inspect(&output).unwrap()["status"],
            "COMPONENT_PREFIX_VERIFIED"
        );
        if let Some(p) = &retain {
            std::fs::write(
                p.join("reader-rejections.json"),
                serde_json::to_vec_pretty(&rejections).unwrap(),
            )
            .unwrap();
        }
    });
}
#[test]
fn stale_native_snapshot_is_refused_before_reference_publication_or_sql() {
    with_fixture_clock(T, || {
        let d = Dir::new();
        let output = d.0.join("stale");
        let mut c = prepared().create(&output, StopHandle::default()).unwrap();
        attach(&mut c);
        let source = reference(INSTRUMENTS[0]);
        go(T + 900001);
        assert!(c
            .add_reference(INSTRUMENTS[0], &source)
            .unwrap_err()
            .to_string()
            .contains("reference"));
        assert_eq!(c.export().unwrap()["revision"], 0);
        assert_eq!(c.frontier(INSTRUMENTS[0]), Some(T));
        c.finish().unwrap();
        assert!(!control(&output.join("source-control.jsonl"))
            .iter()
            .any(|r| r["payload"]["kind"] == "reference"));
    });
}
