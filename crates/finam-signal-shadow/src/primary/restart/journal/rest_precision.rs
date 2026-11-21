//! Retained REST evidence must preserve original receipt precision on readback.
use super::*;
use chrono::Duration;
use serde_json::json;

const T: i64 = 1793640000000;

fn fixture(minutes: i64, nanos: i64) -> ClosedM1SnapshotEvidenceV1 {
    let rows = (0..minutes)
        .map(|n| {
            json!({"timestamp":at(T+n*60000).unwrap().to_rfc3339(),
                "open":{"value":"2200"},"high":{"value":"2201"},
                "low":{"value":"2199"},"close":{"value":"2200.5"},
                "volume":{"value":"2"}})
        })
        .collect();
    let mut e =
        crate::primary::tests::reference(rows, T, T + minutes * 60000, T + minutes * 60000 + 100)
            .evidence()
            .clone();
    e.parts[0].received_at += Duration::nanoseconds(nanos);
    e
}

fn expected(e: &ClosedM1SnapshotEvidenceV1) -> AdmittedClosedM1Snapshot {
    let p =
        ClosedM1RequestPlan::new(e.start, e.end, e.parts.iter().map(|p| p.end).collect()).unwrap();
    let exact = e.parts.iter().map(|p| p.received_at).max().unwrap();
    AdmittedClosedM1Snapshot::admit(e.clone(), &p, exact).unwrap()
}

fn multipart() -> ClosedM1SnapshotEvidenceV1 {
    let mut e = fixture(20, 100);
    let all: Value = serde_json::from_str(&e.parts[0].raw_body).unwrap();
    let rows = all["bars"].as_array().unwrap();
    let mut first = e.parts[0].clone();
    let mut last = first.clone();
    first.end = at(T + 600000).unwrap();
    last.start = first.end;
    last.requested_at = first.received_at + Duration::nanoseconds(100);
    last.received_at = first.received_at + Duration::nanoseconds(300);
    for (part, bars) in [(&mut first, &rows[..10]), (&mut last, &rows[10..])] {
        let raw = json!({"symbol":"IMOEXF@RTSX","bars":bars}).to_string();
        part.declared_body_bytes = Some(raw.len() as u64);
        part.response_sha256 = digest(raw.as_bytes());
        part.raw_body = raw;
    }
    e.parts = vec![first, last];
    e
}

#[test]
fn integer_millisecond_receipts_keep_the_same_snapshot_and_hash() {
    let e = fixture(10, 0);
    let want = expected(&e);
    let got = admit_rest(e.clone(), T + 600101).unwrap();
    assert_eq!(got.sha256(), want.sha256());
    assert_eq!(
        serde_json::to_value(got.evidence()).unwrap(),
        serde_json::to_value(e).unwrap()
    );
}

#[test]
fn nonzero_submillisecond_receipts_roundtrip_without_retiming() {
    for nanos in [1, 287124, 999999] {
        let e = fixture(10, nanos);
        let want = expected(&e);
        let got = admit_rest(e.clone(), T + 600101).unwrap();
        assert_eq!(got.sha256(), want.sha256());
        assert_eq!(
            serde_json::to_vec(got.evidence()).unwrap(),
            serde_json::to_vec(&e).unwrap()
        );
    }
}

#[test]
fn multipart_uses_latest_exact_receipt_not_earlier_in_same_millisecond() {
    let e = multipart();
    assert_eq!(
        e.parts[0].received_at.timestamp_millis(),
        e.parts[1].received_at.timestamp_millis()
    );
    assert!(e.parts[1].received_at > e.parts[0].received_at);
    let want = expected(&e);
    let got = admit_rest(e.clone(), T + 1200101).unwrap();
    assert_eq!(got.sha256(), want.sha256());
    assert_eq!(
        serde_json::to_vec(got.evidence()).unwrap(),
        serde_json::to_vec(&e).unwrap()
    );
}

#[test]
fn later_readback_uses_original_asof_not_new_freshness_or_new_identity() {
    let e = fixture(10, 287124);
    let want = expected(&e);
    assert_eq!(admit_rest(e, T + 86400000).unwrap().sha256(), want.sha256());
}

#[test]
fn future_receipt_beyond_existing_millisecond_guard_still_refuses() {
    let e = fixture(10, 287124);
    assert!(matches!(
        admit_rest(e, T + 600099),
        Err(RestartError::Source)
    ));
}

#[test]
fn original_hash_transport_and_receive_order_remain_mandatory() {
    for case in 0..4 {
        let mut e = fixture(10, 287124);
        match case {
            0 => e.parts[0].response_sha256 = "0".repeat(64),
            1 => e.parts[0].transport_complete = false,
            2 => e.parts[0].status = 503,
            _ => e.parts[0].received_at = e.parts[0].requested_at - Duration::nanoseconds(1),
        }
        assert!(
            matches!(admit_rest(e, T + 600101), Err(RestartError::Source)),
            "case {case}"
        );
    }
}

#[test]
fn multipart_request_before_previous_precise_receipt_remains_invalid() {
    let mut e = multipart();
    e.parts[1].requested_at = e.parts[0].received_at - Duration::nanoseconds(1);
    assert!(matches!(
        admit_rest(e, T + 1200101),
        Err(RestartError::Source)
    ));
}

#[test]
fn all_27_original_imoexf_snapshots_keep_their_hash_after_precision_readback() {
    let retained: Value = serde_json::from_slice(include_bytes!(
        "../../../../fixtures/retained-rest/imoexf-20261119-receipts.json"
    ))
    .unwrap();
    let cases = retained["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 27);
    for row in cases {
        let e: ClosedM1SnapshotEvidenceV1 =
            serde_json::from_value(row["evidence"].clone()).unwrap();
        let before = serde_json::to_vec(&e).unwrap();
        let got = admit_rest(e, 1795073100000).unwrap();
        assert_eq!(Some(got.sha256()), row["snapshot_sha256"].as_str());
        assert_eq!(serde_json::to_vec(got.evidence()).unwrap(), before);
    }
}
