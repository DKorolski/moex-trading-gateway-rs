//! Real field/time compatibility without source, clock or model-policy waivers.
use super::*;
use crate::primary::calendar::{history_windows, CalendarError, RawCalendar};
use std::collections::BTreeSet;

fn reseal(part: &mut Value, body: &Value) {
    let raw = body.to_string();
    part["declared_body_bytes"] = json!(raw.len());
    part["response_sha256"] = json!(crate::seed::digest(raw.as_bytes()));
    part["raw_body"] = json!(raw);
}
fn calendar(source: &Value) -> RawCalendar {
    serde_json::from_value(source["calendar"].clone()).unwrap()
}
fn day_start(d: &Value) -> i64 {
    d["sources"][0]["history"]["required_labels_ms"][0]
        .as_i64()
        .unwrap()
        - 7 * 3600000
}

#[test]
fn known_snapshot_bool_aliases_preserve_data_without_becoming_finality() {
    with_fixture_clock(f::T, || {
        let baseline = admitted(&frozen(&draft())).unwrap();
        for key in ["is_data_snapshot", "isDataSnapshot"] {
            for flag in [false, true] {
                let mut d = draft();
                for source in d["sources"].as_array_mut().unwrap() {
                    let p = &mut source["history"]["snapshots"][0]["evidence"]["parts"][0];
                    let mut body: Value =
                        serde_json::from_str(p["raw_body"].as_str().unwrap()).unwrap();
                    for row in body["bars"].as_array_mut().unwrap() {
                        row[key] = json!(flag);
                    }
                    reseal(p, &body);
                }
                let actual = admitted(&frozen(&d)).unwrap();
                for (a, b) in actual
                    .source
                    .manifest
                    .seeds
                    .iter()
                    .zip(&baseline.source.manifest.seeds)
                {
                    assert_eq!(a.history.len(), b.history.len());
                    for (x, y) in a.history.iter().zip(&b.history) {
                        assert_eq!(x.start_ms, y.start_ms);
                        assert_eq!(
                            serde_json::to_value(&x.data).unwrap(),
                            serde_json::to_value(&y.data).unwrap()
                        );
                    }
                }
            }
        }
    });
}

#[test]
fn unknown_ambiguous_or_nonboolean_snapshot_fields_still_refuse() {
    with_fixture_clock(f::T, || {
        for n in 0..8 {
            let mut d = draft();
            let p = &mut d["sources"][0]["history"]["snapshots"][0]["evidence"]["parts"][0];
            let mut body: Value = serde_json::from_str(p["raw_body"].as_str().unwrap()).unwrap();
            match n {
                0 => body["bars"][0]["is_data_snapshot"] = Value::Null,
                1 => body["bars"][0]["is_data_snapshot"] = json!(0),
                2 => body["bars"][0]["is_data_snapshot"] = json!("false"),
                3 => body["bars"][0]["is_data_snapshot"] = json!([]),
                4 => body["bars"][0]["unexpected"] = json!(false),
                5 => {
                    body["bars"][0]["is_data_snapshot"] = json!(false);
                    body["bars"][0]["isDataSnapshot"] = json!(false);
                }
                6 => {
                    body["bars"][0]["is_data_snapshot"] = json!(false);
                    body["bars"][0]["unexpected"] = json!(false);
                }
                _ => body["bars"][0]["close"]["unexpected"] = json!(false),
            }
            reseal(p, &body);
            assert!(freeze(d.to_string().as_bytes()).is_err(), "case {n}");
        }
    });
}

#[test]
fn snapshot_flag_does_not_replace_transport_identity_or_closed_range_checks() {
    with_fixture_clock(f::T, || {
        for n in 0..4 {
            let mut d = draft();
            let e = &mut d["sources"][0]["history"]["snapshots"][0]["evidence"];
            let p = &mut e["parts"][0];
            let mut body: Value = serde_json::from_str(p["raw_body"].as_str().unwrap()).unwrap();
            body["bars"][0]["is_data_snapshot"] = json!(true);
            reseal(p, &body);
            match n {
                0 => p["transport_complete"] = json!(false),
                1 => p["status"] = json!(503),
                2 => p["symbol"] = json!("IMOEXF@RTSX"),
                _ => e["end"] = json!(at(f::T + 60000)),
            }
            assert!(freeze(d.to_string().as_bytes()).is_err(), "case {n}");
        }
    });
}

#[test]
fn original_second_precision_closed_splits_do_not_change_market_windows() {
    let d = draft();
    let mut raw = calendar(&d["sources"][0]);
    let start = day_start(&d);
    let expected = history_windows(INSTRUMENTS[0], &raw, start, f::T - 7 * 3600000, f::T).unwrap();
    let mut body: Value = serde_json::from_str(&raw.snapshots[0].raw_body).unwrap();
    let split = at(start + 1000).to_rfc3339();
    let mut second = body["sessions"][0].clone();
    second["interval"]["start_time"] = json!(split);
    body["sessions"][0]["interval"]["end_time"] = json!(split);
    body["sessions"].as_array_mut().unwrap().insert(1, second);
    let text = body.to_string();
    raw.snapshots[0].raw_body = text.clone();
    raw.snapshots[0].declared_body_bytes = text.len() as u64;
    raw.snapshots[0].response_sha256 = crate::seed::digest(text.as_bytes());
    assert_eq!(
        history_windows(INSTRUMENTS[0], &raw, start, f::T - 7 * 3600000, f::T).unwrap(),
        expected
    );
    body["sessions"][0]["interval"]["end_time"] =
        json!(at(start + 1000) + chrono::Duration::nanoseconds(1));
    let text = body.to_string();
    raw.snapshots[0].raw_body = text.clone();
    raw.snapshots[0].declared_body_bytes = text.len() as u64;
    raw.snapshots[0].response_sha256 = crate::seed::digest(text.as_bytes());
    assert_eq!(
        history_windows(INSTRUMENTS[0], &raw, start, f::T - 7 * 3600000, f::T),
        Err(CalendarError::Clock)
    );
}

fn auction_draft() -> Value {
    let mut d = draft();
    for source in d["sources"].as_array_mut().unwrap() {
        let first = source["history"]["required_labels_ms"][0].as_i64().unwrap();
        let m = first - 60000;
        let s = &mut source["calendar"]["snapshots"][0];
        let mut body: Value = serde_json::from_str(s["raw_body"].as_str().unwrap()).unwrap();
        let end = body["sessions"][0]["interval"]["end_time"].clone();
        body["sessions"][0]["interval"]["end_time"] = json!(at(m + 1000));
        body["sessions"].as_array_mut().unwrap().insert(1, json!({"type":"OPENING_AUCTION","interval":{"start_time":at(m+1000),"end_time":at(m+50000)}}));
        body["sessions"].as_array_mut().unwrap().insert(
            2,
            json!({"type":"CLOSED","interval":{"start_time":at(m+50000),"end_time":end}}),
        );
        reseal(s, &body);
        let e = &mut source["history"]["snapshots"][0]["evidence"];
        e["start"] = json!(at(first - 600000));
        e["parts"][0]["start"] = e["start"].clone();
        let p = &mut e["parts"][0];
        let mut body: Value = serde_json::from_str(p["raw_body"].as_str().unwrap()).unwrap();
        let mut row = body["bars"][0].clone();
        row["timestamp"] = json!(at(m));
        body["bars"].as_array_mut().unwrap().insert(0, row);
        reseal(p, &body);
        source["history"]["required_labels_ms"]
            .as_array_mut()
            .unwrap()
            .insert(0, json!(first - 600000));
    }
    d
}

#[test]
fn mixed_auction_minute_stays_in_its_original_nonempty_nominal_bucket() {
    with_fixture_clock(f::T, || {
        let d = auction_draft();
        let a = admitted(&frozen(&d)).unwrap();
        for seed in &a.source.manifest.seeds[1..] {
            assert_eq!(seed.history.len(), 203);
            assert_eq!(
                seed.history[0].start_ms,
                d["sources"][0]["history"]["required_labels_ms"][0]
                    .as_i64()
                    .unwrap()
            );
            assert_eq!(seed.history[0].end_ms - seed.history[0].start_ms, 600000);
        }
    });
}

#[test]
fn clipped_m10_request_and_wholly_closed_or_omitted_auction_rows_refuse() {
    with_fixture_clock(f::T, || {
        for n in 0..4 {
            let mut d = auction_draft();
            let source = &mut d["sources"][0];
            if n == 0 {
                let first = source["history"]["required_labels_ms"][0].as_i64().unwrap();
                source["history"]["snapshots"][0]["evidence"]["start"] =
                    json!(at(first + 9 * 60000));
                source["history"]["snapshots"][0]["evidence"]["parts"][0]["start"] =
                    json!(at(first + 9 * 60000));
            } else if n == 1 {
                let p = &mut source["history"]["snapshots"][0]["evidence"]["parts"][0];
                let mut body: Value =
                    serde_json::from_str(p["raw_body"].as_str().unwrap()).unwrap();
                let t =
                    DateTime::parse_from_rfc3339(body["bars"][0]["timestamp"].as_str().unwrap())
                        .unwrap();
                body["bars"][0]["timestamp"] = json!(t - chrono::Duration::minutes(1));
                reseal(p, &body);
            } else if n == 2 {
                source["history"]["required_labels_ms"]
                    .as_array_mut()
                    .unwrap()
                    .remove(0);
            } else {
                let m = source["history"]["required_labels_ms"][0].as_i64().unwrap() + 9 * 60000;
                let s = &mut source["calendar"]["snapshots"][0];
                let mut body: Value =
                    serde_json::from_str(s["raw_body"].as_str().unwrap()).unwrap();
                body["sessions"][0]["interval"]["end_time"] = json!(at(m - 59000));
                body["sessions"][1]["interval"]["start_time"] = json!(at(m - 59000));
                body["sessions"][1]["interval"]["end_time"] = json!(at(m));
                body["sessions"][2]["interval"]["start_time"] = json!(at(m));
                reseal(s, &body);
            }
            assert!(freeze(d.to_string().as_bytes()).is_err(), "case {n}");
        }
    });
}

#[test]
fn current_day_preopen_is_not_a_future_historical_query_requirement() {
    with_fixture_clock(f::T, || {
        let mut d = draft();
        for source in d["sources"].as_array_mut().unwrap() {
            let s = &mut source["calendar"]["snapshots"][0];
            let mut body: Value = serde_json::from_str(s["raw_body"].as_str().unwrap()).unwrap();
            let n = body["sessions"].as_array().unwrap().len() - 2;
            body["sessions"][n]["interval"]["end_time"] = json!(at(f::T - 600000));
            body["sessions"].as_array_mut().unwrap().insert(n + 1, json!({"type":"OPENING_AUCTION","interval":{"start_time":at(f::T-600000),"end_time":at(f::T-10000)}}));
            body["sessions"].as_array_mut().unwrap().insert(n + 2, json!({"type":"CLOSED","interval":{"start_time":at(f::T-10000),"end_time":at(f::T)}}));
            reseal(s, &body);
        }
        assert!(admitted(&frozen(&d)).is_ok());
        d["sources"][0]["history"]["snapshots"][0]["evidence"]["end"] = json!(at(f::T - 60000));
        d["sources"][0]["history"]["snapshots"][0]["evidence"]["parts"][0]["end"] =
            json!(at(f::T - 60000));
        assert!(freeze(d.to_string().as_bytes())
            .unwrap_err()
            .to_string()
            .contains("closed prior local days"));
        prior_midnight_bucket_and_exact_endpoint_row();
    });
}

fn prior_midnight_bucket_and_exact_endpoint_row() {
    let mut d = draft();
    let end = f::T - 7 * 3600000;
    let label = end - 600000;
    for source in d["sources"].as_array_mut().unwrap() {
        let s = &mut source["calendar"]["snapshots"][0];
        let mut body: Value = serde_json::from_str(s["raw_body"].as_str().unwrap()).unwrap();
        let n = body["sessions"].as_array().unwrap().len() - 2;
        body["sessions"][n]["interval"]["end_time"] = json!(at(label));
        body["sessions"].as_array_mut().unwrap().insert(
            n + 1,
            json!({"type":"CORE_TRADING","interval":{"start_time":at(label),"end_time":at(end)}}),
        );
        body["sessions"].as_array_mut().unwrap().insert(
            n + 2,
            json!({"type":"CLOSED","interval":{"start_time":at(end),"end_time":at(f::T)}}),
        );
        reseal(s, &body);
        let e = &mut source["history"]["snapshots"][0]["evidence"];
        e["end"] = json!(at(end));
        e["parts"][0]["end"] = e["end"].clone();
        let p = &mut e["parts"][0];
        let mut body: Value = serde_json::from_str(p["raw_body"].as_str().unwrap()).unwrap();
        let row = body["bars"][0].clone();
        for t in (label..=end).step_by(60000) {
            let mut row = row.clone();
            row["timestamp"] = json!(at(t));
            body["bars"].as_array_mut().unwrap().push(row);
        }
        reseal(p, &body);
        source["history"]["required_labels_ms"]
            .as_array_mut()
            .unwrap()
            .push(json!(label));
    }
    // Freeze-only: a partial Sunday is not promoted to a complete READY anchor.
    let frozen: Frozen = serde_json::from_slice(&frozen(&d)).unwrap();
    for seed in &frozen.manifest.seeds[1..] {
        assert_eq!(seed.history.len(), 203);
        assert_eq!(seed.history.last().unwrap().end_ms, end);
        assert!(seed.history.iter().all(|c| c.start_ms < end));
    }
}

#[test]
fn real_19_november_raws_pass_schema_calendar_and_native_snapshot_unchanged() {
    let fixture: Value = serde_json::from_slice(include_bytes!(
        "../../../../../fixtures/real-input/usd-ri-20261119.json"
    ))
    .unwrap();
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 2);
    for case in cases {
        let instrument = serde_json::from_value(case["instrument"].clone()).unwrap();
        let evidence: ClosedM1SnapshotEvidenceV1 =
            serde_json::from_value(case["evidence"].clone()).unwrap();
        let original = serde_json::to_vec(&evidence).unwrap();
        let raw = &evidence.parts[0].raw_body;
        seed::public_body(raw).unwrap();
        assert_eq!(
            crate::seed::digest(raw.as_bytes()),
            case["expected_raw_sha256"].as_str().unwrap()
        );
        let now = case["admitted_at_ms"].as_i64().unwrap();
        let plan =
            ClosedM1RequestPlan::new(evidence.start, evidence.end, vec![evidence.end]).unwrap();
        let source =
            AdmittedClosedM1Snapshot::admit_for(instrument, evidence, &plan, at(now)).unwrap();
        assert_eq!(serde_json::to_vec(source.evidence()).unwrap(), original);
        let calendar: RawCalendar = serde_json::from_value(case["calendar"].clone()).unwrap();
        let windows = history_windows(
            instrument,
            &calendar,
            source.evidence().start.timestamp_millis(),
            source.evidence().end.timestamp_millis(),
            now,
        )
        .unwrap();
        let bars = source.bars().collect::<Vec<_>>();
        assert_eq!(bars.len() as u64, case["expected_bars"].as_u64().unwrap());
        let labels = bars
            .iter()
            .map(|b| {
                let t = b.open_ts.timestamp_millis();
                assert!(windows.iter().any(|(a, b)| *a < t + 60000 && t < *b));
                t.div_euclid(600000) * 600000
            })
            .collect::<BTreeSet<_>>();
        assert_eq!(labels.len() as u64, case["expected_m10"].as_u64().unwrap());
        assert_eq!(labels.len(), 101);
    }
}
