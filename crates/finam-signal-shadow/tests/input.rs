use broker_finam::sparse_m10::*;
use chrono::{Duration, TimeZone, Utc};
use finam_signal_shadow::{input::*, seed::digest};

fn snapshot(minutes: &[i64], price: &str) -> AdmittedClosedM1Snapshot {
    let start = Utc.with_ymd_and_hms(2026, 10, 23, 7, 0, 0).unwrap();
    let end = start + Duration::minutes(20);
    let rows: Vec<_> = minutes
        .iter()
        .map(|m| {
            serde_json::json!({
                "timestamp": (start + Duration::minutes(*m)).to_rfc3339(),
                "open":{"value":price}, "high":{"value":price},
                "low":{"value":price}, "close":{"value":price}, "volume":{"value":"1"}
            })
        })
        .collect();
    let raw = serde_json::json!({"symbol":"IMOEXF@RTSX","bars":rows}).to_string();
    let evidence = ClosedM1SnapshotEvidenceV1 {
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
            requested_at: end,
            received_at: end,
            status: 200,
            transport_complete: true,
            declared_body_bytes: Some(raw.len() as u64),
            response_sha256: digest(raw.as_bytes()),
            raw_body: raw,
        }],
    };
    AdmittedClosedM1Snapshot::admit(
        evidence,
        &ClosedM1RequestPlan::new(start, end, vec![end]).unwrap(),
        end,
    )
    .unwrap()
}

#[test]
fn sparse_closed_snapshot_maps_once_without_live_promotion() {
    let s = snapshot(&[1, 8, 11], "2200.5");
    let start = s.evidence().start;
    let instrument = s.bars().next().unwrap().instrument.clone();
    let mut adapter = ClosedInputAdapter::new(instrument, 4).unwrap();
    let labels = [start, start + Duration::minutes(10)];
    let out = adapter.prepare(&s, &labels).unwrap();
    assert_eq!(out.len(), 2);
    assert_eq!(out[0].minute_bitmap, (1 << 1) | (1 << 8));
    assert_eq!(out[0].bar.volume, 2.0);
    assert_eq!(out[0].bar.close_time_utc, start.timestamp());
    assert_eq!(
        out[0].bar.origin,
        broker_core::HybridRuntimeBarOrigin::Replay
    );
    assert_eq!(out[0].snapshot_sha256, s.sha256());
    let original = canonical_digest(&out[0].bar).unwrap();
    let mut live = out[0].bar.clone();
    live.origin = broker_core::HybridRuntimeBarOrigin::Live;
    assert_eq!(canonical_digest(&live).unwrap(), original);
    live.close += 0.5;
    assert_ne!(canonical_digest(&live).unwrap(), original);
    assert!(adapter.prepare(&s, &labels).unwrap().is_empty());
}

#[test]
fn missing_required_bucket_holds_entire_batch() {
    let s = snapshot(&[1, 8], "2200");
    let start = s.evidence().start;
    let mut adapter =
        ClosedInputAdapter::new(s.bars().next().unwrap().instrument.clone(), 4).unwrap();
    assert_eq!(
        adapter
            .prepare(&s, &[start, start + Duration::minutes(10)])
            .unwrap_err(),
        InputError::RequiredRange
    );
    assert!(adapter.prepare(&s, &[start]).is_err());
}

#[test]
fn conflicting_actual_minutes_hold_even_when_m10_ohlcv_is_equal() {
    let s = snapshot(&[1, 8], "2200");
    let changed = snapshot(&[2, 8], "2200");
    let start = s.evidence().start;
    let mut adapter =
        ClosedInputAdapter::new(s.bars().next().unwrap().instrument.clone(), 4).unwrap();
    adapter.prepare(&s, &[start]).unwrap();
    assert_eq!(
        adapter.prepare(&changed, &[start]).unwrap_err(),
        InputError::Conflict
    );
    assert!(adapter.prepare(&s, &[start]).is_err());
}

#[test]
fn cache_bound_and_instrument_mismatch_refuse() {
    let s = snapshot(&[1, 11], "2200");
    let start = s.evidence().start;
    let instrument = s.bars().next().unwrap().instrument.clone();
    let mut adapter = ClosedInputAdapter::new(instrument.clone(), 1).unwrap();
    adapter.prepare(&s, &[start]).unwrap();
    assert_eq!(
        adapter
            .prepare(&s, &[start + Duration::minutes(10)])
            .unwrap_err(),
        InputError::Bounds
    );
    let mut wrong = instrument;
    wrong.symbol = "OTHER".into();
    let mut adapter = ClosedInputAdapter::new(wrong, 2).unwrap();
    assert_eq!(
        adapter.prepare(&s, &[start]).unwrap_err(),
        InputError::Identity
    );
}
