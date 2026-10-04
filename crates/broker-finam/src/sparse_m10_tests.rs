use super::*;
use chrono::TimeZone;
use serde_json::{json, Value};

fn time(s: &str) -> DateTime<Utc> {
    s.parse().unwrap()
}

fn fixture(name: &str) -> String {
    std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/sparse-m10")
            .join(name),
    )
    .unwrap()
}

fn evidence(label: &str) -> ClosedM1SnapshotEvidenceV1 {
    let meta: Value =
        serde_json::from_str(&fixture(&format!("finam-{label}-response-meta.json"))).unwrap();
    let raw_body = fixture(&format!("finam-{label}-response.json"));
    assert_eq!(raw_body.len() as u64, meta["bytes"].as_u64().unwrap());
    let start = time(meta["start_time"].as_str().unwrap());
    let end = time(meta["end_time"].as_str().unwrap());
    ClosedM1SnapshotEvidenceV1 {
        policy: OBSERVED_M1_POLICY_V1.into(),
        start,
        end,
        parts: vec![ClosedM1RestPartV1 {
            method: "GET".into(),
            endpoint: "https://api.finam.ru/v1/instruments/IMOEXF@RTSX/bars".into(),
            symbol: "IMOEXF@RTSX".into(),
            timeframe: "TIME_FRAME_M1".into(),
            start,
            end,
            requested_at: time(meta["request_started_at_utc"].as_str().unwrap()),
            received_at: time(meta["body_received_at_utc"].as_str().unwrap()),
            status: 200,
            transport_complete: true,
            // The retained byte count is not a captured Content-Length header.
            declared_body_bytes: None,
            response_sha256: digest(raw_body.as_bytes()),
            raw_body,
        }],
    }
}

fn admit(e: ClosedM1SnapshotEvidenceV1) -> Result<AdmittedClosedM1Snapshot, ClosedM1SnapshotError> {
    let now = e.parts.last().unwrap().received_at;
    let plan = ClosedM1RequestPlan::new(e.start, e.end, e.parts.iter().map(|p| p.end).collect())?;
    AdmittedClosedM1Snapshot::admit(e, &plan, now)
}

fn replace_body(e: &mut ClosedM1SnapshotEvidenceV1, value: Value) {
    let raw = serde_json::to_string(&value).unwrap();
    e.parts[0].response_sha256 = digest(raw.as_bytes());
    e.parts[0].declared_body_bytes = Some(raw.len() as u64);
    e.parts[0].raw_body = raw;
}

#[test]
fn real_404_buckets_preserve_18_sparse_and_four_full_data_differences() {
    let snapshot = admit(evidence("long")).unwrap();
    assert_eq!(snapshot.bars().count(), 4016);
    let mut total = 0;
    let mut sparse = 0;
    let mut differences = Vec::new();
    let mut missing_first = Vec::new();
    for line in fixture("alor-native-m10.jsonl").lines() {
        let value: Value = serde_json::from_str(line).unwrap();
        let p = &value["envelope"]["payload"];
        let open = Utc
            .timestamp_opt(p["close_time_utc"].as_i64().unwrap(), 0)
            .unwrap();
        let bucket = snapshot.bucket(open).unwrap();
        let actual = bucket.bar();
        let a = [
            actual.open,
            actual.high,
            actual.low,
            actual.close,
            actual.volume,
        ];
        let expected =
            ["o", "h", "l", "c", "v"].map(|k| p[k].to_string().parse::<Decimal>().unwrap());
        assert_eq!(actual.open_ts, open);
        assert_eq!(actual.close_ts, open + Duration::seconds(600));
        assert_eq!((actual.high, actual.low), (expected[1], expected[2]));
        if bucket.actual_m1().len() < 10 {
            sparse += 1;
            assert_eq!(a, expected);
        }
        if bucket.minute_bitmap() & 1 == 0 {
            missing_first.push(open);
        }
        if a != expected {
            assert_eq!(bucket.actual_m1().len(), 10);
            differences.push(open);
        }
        snapshot.verify_selection(open, bucket.actual_m1()).unwrap();
        total += 1;
    }
    assert_eq!((total, sparse), (404, 18));
    assert_eq!(
        differences,
        [
            "2026-09-28T04:00:00Z",
            "2026-09-29T04:00:00Z",
            "2026-09-29T14:10:00Z",
            "2026-09-30T14:00:00Z"
        ]
        .map(time)
    );
    assert_eq!(
        missing_first,
        ["2026-09-29T20:10:00Z", "2026-09-30T05:10:00Z"].map(time)
    );
}

#[test]
fn short_range_accounts_exact_end_without_counting_it_in_bucket() {
    let snapshot = admit(evidence("short")).unwrap();
    assert_eq!(
        snapshot.accounting()[0],
        ClosedM1RangeAccountingV1 {
            raw_rows: 28,
            in_range_unique_rows: 27,
            exact_right_boundary_rows: 1,
            equal_duplicate_rows: 0,
        }
    );
    assert_eq!(snapshot.bars().count(), 27);
    assert_eq!(
        snapshot
            .bucket(time("2026-09-28T19:40:00Z"))
            .unwrap()
            .minute_bitmap(),
        0b1111110001
    );
    assert_eq!(
        snapshot.bucket(time("2026-09-28T20:00:00Z")).unwrap_err(),
        ClosedM1SnapshotError::Range
    );
}

#[test]
fn raw_binding_detects_post_admission_loss_and_price_change() {
    let snapshot = admit(evidence("short")).unwrap();
    let open = time("2026-09-28T19:40:00Z");
    let bucket = snapshot.bucket(open).unwrap();
    let mut bars = bucket.actual_m1().to_vec();
    bars.pop();
    assert_eq!(
        snapshot.verify_selection(open, &bars),
        Err(ClosedM1SnapshotError::SelectionMismatch)
    );
    let mut bars = bucket.actual_m1().to_vec();
    bars[0].close += Decimal::ONE;
    assert_eq!(
        snapshot.verify_selection(open, &bars),
        Err(ClosedM1SnapshotError::SelectionMismatch)
    );
    assert_eq!(snapshot.sha256(), bucket.snapshot_sha256());
}

#[test]
fn synthetic_missing_last_single_and_empty_never_shift_nominal_bounds() {
    for count in [9, 1, 0] {
        let mut e = evidence("short");
        let mut body: Value = serde_json::from_str(&e.parts[0].raw_body).unwrap();
        body["bars"].as_array_mut().unwrap().retain(|b| {
            let t = time(b["timestamp"].as_str().unwrap());
            t < e.start + Duration::minutes(count)
        });
        replace_body(&mut e, body);
        let open = e.start;
        let snapshot = admit(e).unwrap();
        if count == 0 {
            assert_eq!(
                snapshot.bucket(open).unwrap_err(),
                ClosedM1SnapshotError::EmptyBucket
            );
        } else {
            let bucket = snapshot.bucket(open).unwrap();
            assert_eq!(bucket.actual_m1().len(), count as usize);
            assert_eq!(bucket.bar().open_ts, open);
            assert_eq!(bucket.bar().close_ts, open + Duration::minutes(10));
            assert_eq!(bucket.bar().close, bucket.actual_m1().last().unwrap().close);
        }
    }
}

#[test]
fn invalid_admission_is_fail_closed() {
    type Mutation = fn(&mut ClosedM1SnapshotEvidenceV1);
    let cases: &[Mutation] = &[
        |e| e.policy = "legacy-or-unrecognized".into(),
        |e| e.parts[0].method = "POST".into(),
        |e| e.parts[0].endpoint = "https://other.example/bars".into(),
        |e| e.parts[0].symbol = "OTHER".into(),
        |e| e.parts[0].timeframe = "TIME_FRAME_M10".into(),
        |e| e.parts[0].status = 206,
        |e| e.parts[0].transport_complete = false,
        |e| e.parts[0].declared_body_bytes = Some(1),
        |e| e.parts[0].response_sha256 = "00".repeat(32),
        |e| e.end += Duration::minutes(1),
        |e| e.parts[0].requested_at = e.end - Duration::seconds(1),
        |e| e.parts[0].received_at = e.parts[0].requested_at - Duration::seconds(1),
        |e| e.parts[0].start -= Duration::minutes(1),
    ];
    for mutation in cases {
        let mut e = evidence("short");
        mutation(&mut e);
        assert!(admit(e).is_err());
    }
    let e = evidence("short");
    let now = e.parts[0].received_at + Duration::seconds(901);
    let plan = ClosedM1RequestPlan::new(e.start, e.end, vec![e.end]).unwrap();
    assert_eq!(
        AdmittedClosedM1Snapshot::admit(e, &plan, now).unwrap_err(),
        ClosedM1SnapshotError::Freshness
    );
    let mut e = evidence("short");
    e.parts[0].raw_body = " ".repeat(SNAPSHOT_MAX_BODY_BYTES + 1);
    assert!(admit(e).is_err());
}

#[test]
fn end_of_session_candidate_does_not_need_next_bar_and_never_falls_back() {
    let mut e = evidence("short");
    let start = time("2026-10-01T20:40:00Z");
    let close = start + Duration::minutes(10);
    // Explicit synthetic closing-session control: nine actual minutes, no last.
    let raw: Value = serde_json::from_str(&e.parts[0].raw_body).unwrap();
    let mut bars = Vec::new();
    for minute in 0..9 {
        let mut b = raw["bars"][0].clone();
        b["timestamp"] = json!((start + Duration::minutes(minute)).to_rfc3339());
        bars.push(b);
    }
    e.start = start;
    e.end = close;
    e.parts[0].start = start;
    e.parts[0].end = close;
    e.parts[0].requested_at = close + Duration::seconds(1);
    e.parts[0].received_at = close + Duration::seconds(2);
    replace_body(&mut e, json!({"symbol":"IMOEXF@RTSX", "bars": bars}));
    let now = e.parts[0].received_at;
    let snapshot = admit(e).unwrap();
    let bucket = snapshot.candidate_at(close, now).unwrap();
    assert_eq!(bucket.bar().close_ts, close);
    assert_eq!(bucket.actual_m1().len(), 9);
    assert_eq!(
        snapshot
            .candidate_at(close, close - Duration::seconds(1))
            .unwrap_err(),
        ClosedM1SnapshotError::Freshness
    );
    assert_eq!(
        snapshot
            .candidate_at(close, close + Duration::seconds(901))
            .unwrap_err(),
        ClosedM1SnapshotError::Freshness
    );
    assert_eq!(
        snapshot
            .candidate_at(close + Duration::minutes(10), close + Duration::minutes(10))
            .unwrap_err(),
        ClosedM1SnapshotError::Range
    );
}

#[test]
fn malformed_truncated_foreign_out_of_range_and_conflicting_rows_are_rejected() {
    for value in [
        "NaN",
        "Inf",
        "-1",
        "0.1",
        "100000000000000000000000000000000000000",
    ] {
        let mut e = evidence("short");
        let mut body: Value = serde_json::from_str(&e.parts[0].raw_body).unwrap();
        body["bars"][0]["high"]["value"] = json!(value);
        replace_body(&mut e, body);
        assert!(admit(e).is_err(), "{value}");
    }
    for timestamp in [
        "2026-09-28T19:29:00Z",
        "2026-09-28T20:01:00Z",
        "2026-09-28T19:30:01Z",
    ] {
        let mut e = evidence("short");
        let mut body: Value = serde_json::from_str(&e.parts[0].raw_body).unwrap();
        body["bars"][0]["timestamp"] = json!(timestamp);
        replace_body(&mut e, body);
        assert!(admit(e).is_err());
    }
    for body in [
        json!({"symbol":"IMOEXF@RTSX"}),
        json!({"symbol":"OTHER","bars":[]}),
    ] {
        let mut e = evidence("short");
        replace_body(&mut e, body);
        assert!(admit(e).is_err());
    }
    let mut e = evidence("short");
    e.parts[0].raw_body = e.parts[0].raw_body.trim_end().to_string();
    e.parts[0].raw_body.pop();
    e.parts[0].declared_body_bytes = None;
    e.parts[0].response_sha256 = digest(e.parts[0].raw_body.as_bytes());
    assert!(admit(e).is_err());
    let mut e = evidence("short");
    let mut body: Value = serde_json::from_str(&e.parts[0].raw_body).unwrap();
    let mut duplicate = body["bars"][0].clone();
    body["bars"].as_array_mut().unwrap().push(duplicate.clone());
    replace_body(&mut e, body.clone());
    assert_eq!(
        admit(e.clone()).unwrap().accounting()[0].equal_duplicate_rows,
        1
    );
    duplicate["volume"]["value"] = json!("999");
    body["bars"].as_array_mut().unwrap().push(duplicate);
    replace_body(&mut e, body);
    assert_eq!(admit(e).unwrap_err(), ClosedM1SnapshotError::Conflict);
}

#[test]
fn adjacent_parts_account_boundary_once_and_reject_conflicting_copies() {
    let original = evidence("short");
    let mut e = original.clone();
    let boundary = e.start + Duration::minutes(10);
    let response: Value = serde_json::from_str(&e.parts[0].raw_body).unwrap();
    let mut second = e.parts[0].clone();
    e.parts[0].end = boundary;
    second.start = boundary;
    second.requested_at = e.parts[0].received_at;
    second.received_at = second.requested_at + Duration::seconds(1);
    e.parts.push(second);
    for part in &mut e.parts {
        let mut body = response.clone();
        body["bars"].as_array_mut().unwrap().retain(|b| {
            let t = time(b["timestamp"].as_str().unwrap());
            part.start <= t && t <= part.end
        });
        part.raw_body = serde_json::to_string(&body).unwrap();
        part.response_sha256 = digest(part.raw_body.as_bytes());
        part.declared_body_bytes = Some(part.raw_body.len() as u64);
    }
    let snapshot = admit(e.clone()).unwrap();
    assert_eq!(
        snapshot.bars().collect::<Vec<_>>(),
        admit(original).unwrap().bars().collect::<Vec<_>>()
    );
    assert_eq!(
        snapshot
            .accounting()
            .iter()
            .map(|p| p.exact_right_boundary_rows)
            .sum::<usize>(),
        2
    );
    let mut body: Value = serde_json::from_str(&e.parts[1].raw_body).unwrap();
    body["bars"][0]["volume"]["value"] = json!("999");
    e.parts[1].raw_body = serde_json::to_string(&body).unwrap();
    e.parts[1].response_sha256 = digest(e.parts[1].raw_body.as_bytes());
    e.parts[1].declared_body_bytes = Some(e.parts[1].raw_body.len() as u64);
    assert_eq!(admit(e).unwrap_err(), ClosedM1SnapshotError::Conflict);
}

#[test]
fn missing_planned_part_cannot_shorten_its_own_range() {
    let mut e = evidence("short");
    let plan = ClosedM1RequestPlan::new(e.start, e.end, vec![e.end]).unwrap();
    let now = e.parts[0].received_at;
    // Simulate a lost tail and internally consistent forged response metadata.
    e.end -= Duration::minutes(10);
    e.parts[0].end = e.end;
    let mut body: Value = serde_json::from_str(&e.parts[0].raw_body).unwrap();
    body["bars"]
        .as_array_mut()
        .unwrap()
        .retain(|b| time(b["timestamp"].as_str().unwrap()) <= e.end);
    replace_body(&mut e, body);
    assert_eq!(
        AdmittedClosedM1Snapshot::admit(e, &plan, now).unwrap_err(),
        ClosedM1SnapshotError::Range
    );
    let e = evidence("short");
    let plan =
        ClosedM1RequestPlan::new(e.start, e.end, vec![e.start + Duration::minutes(10), e.end])
            .unwrap();
    // Same overall interval with one returned response cannot replace two
    // planned requests. Their receipt set must match, not only first/last.
    assert_eq!(
        AdmittedClosedM1Snapshot::admit(e, &plan, now).unwrap_err(),
        ClosedM1SnapshotError::Range
    );
}

#[test]
fn unsupported_history_depth_and_future_receipt_are_rejected() {
    let mut e = evidence("short");
    let plan = ClosedM1RequestPlan::new(e.start, e.end, vec![e.end]).unwrap();
    e.parts[0].requested_at = e.start + Duration::days(7) + Duration::seconds(1);
    e.parts[0].received_at = e.parts[0].requested_at;
    let now = e.parts[0].received_at;
    assert_eq!(
        AdmittedClosedM1Snapshot::admit(e, &plan, now).unwrap_err(),
        ClosedM1SnapshotError::Freshness
    );
    let e = evidence("short");
    let now = e.parts[0].received_at - Duration::milliseconds(1);
    assert_eq!(
        AdmittedClosedM1Snapshot::admit(e, &plan, now).unwrap_err(),
        ClosedM1SnapshotError::Freshness
    );
}
