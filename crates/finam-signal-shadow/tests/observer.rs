use finam_signal_shadow::{
    observer::*,
    segment::{ObservationMode, SeedObservation},
};
use serde_json::json;
use std::sync::atomic::{AtomicUsize, Ordering};

struct Dir(std::path::PathBuf);
impl Dir {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "moex-shadow-observer-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn header() -> SeedObservation {
    SeedObservation {
        mode: ObservationMode::Warmup,
        run_id: "run".into(),
        segment_id: "segment".into(),
        previous_segment_id: None,
        config_sha256: "a".repeat(64),
        semantic_config_sha256: "f".repeat(64),
        seed_sha256: "b".repeat(64),
        model_sha256: "c".repeat(64),
        reference_sha256: "d".repeat(64),
        feedback_policy_sha256: "e".repeat(64),
        last_history_label_utc: 0,
        observation_start_utc_ms: 600_000,
        virtual_position_lots: 0,
        live_observation_count: 0,
        state: json!({"entry_ready":true}),
    }
}

fn step() -> Step {
    Step {
        mode: ObservationMode::LiveObserved,
        instrument: "IMOEXF".into(),
        model_label_utc_ms: 600_000,
        nominal_close_utc_ms: 1_200_000,
        provider_at_ms: None,
        received_at_ms: 1_200_001,
        canonical_ready_at_ms: 1_200_002,
        model_start_at_ms: 1_200_003,
        result_at_ms: 1_200_004,
        canonical_input_sha256: "a".repeat(64),
        raw_provenance_sha256: "b".repeat(64),
        before: json!({"current_owner":null}),
        after: json!({"current_owner":null}),
        decision: Decision {
            action: None,
            quantity_lots: 0,
            reason: "no_signal".into(),
        },
    }
}

fn source_snapshot() -> broker_finam::sparse_m10::AdmittedClosedM1Snapshot {
    use broker_finam::sparse_m10::*;
    use chrono::{Duration, TimeZone, Utc};
    let start = Utc.with_ymd_and_hms(2026, 10, 23, 7, 0, 0).unwrap();
    let end = start + Duration::minutes(10);
    let raw = format!(" {{\"symbol\":\"IMOEXF@RTSX\",\"bars\":[{{\"timestamp\":\"{}\",\"open\":{{\"value\":\"2200\"}},\"high\":{{\"value\":\"2200\"}},\"low\":{{\"value\":\"2200\"}},\"close\":{{\"value\":\"2200\"}},\"volume\":{{\"value\":\"1\"}}}}]}}\n", start.to_rfc3339());
    AdmittedClosedM1Snapshot::admit(
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
                requested_at: end,
                received_at: end,
                status: 200,
                transport_complete: true,
                declared_body_bytes: Some(raw.len() as u64),
                response_sha256: finam_signal_shadow::seed::digest(raw.as_bytes()),
                raw_body: raw,
            }],
        },
        &ClosedM1RequestPlan::new(start, end, vec![end]).unwrap(),
        end,
    )
    .unwrap()
}

#[test]
fn source_evidence_roundtrips_raw_bytes_and_admission_digest_before_step() {
    use broker_finam::sparse_m10::*;
    let dir = Dir::new();
    let path = dir.0.join("source.jsonl");
    let snapshot = source_snapshot();
    let mut writer = Observer::create(&path, header(), 50_000, 10_000).unwrap();
    writer.record_source(&snapshot).unwrap();
    let mut step = step();
    step.raw_provenance_sha256 = snapshot.sha256().into();
    writer.record_step(&step).unwrap();
    writer.finish("done").unwrap();
    assert!(matches!(
        writer.record_source(&snapshot),
        Err(Error::Closed)
    ));
    let rows: Vec<serde_json::Value> = std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(rows[1]["kind"], "source_snapshot");
    assert_eq!(rows[1]["event"]["snapshot_sha256"], snapshot.sha256());
    assert_eq!(rows[2]["event"]["raw_provenance_sha256"], snapshot.sha256());
    let evidence: ClosedM1SnapshotEvidenceV1 =
        serde_json::from_value(rows[1]["event"]["evidence"].clone()).unwrap();
    assert_eq!(&evidence, snapshot.evidence());
    let plan = ClosedM1RequestPlan::new(evidence.start, evidence.end, vec![evidence.end]).unwrap();
    let readmitted =
        AdmittedClosedM1Snapshot::admit(evidence, &plan, snapshot.evidence().end).unwrap();
    assert_eq!(readmitted.sha256(), snapshot.sha256());
    assert_eq!(
        rows[1]["event"]["accounting"],
        serde_json::to_value(snapshot.accounting()).unwrap()
    );
}

#[test]
fn source_row_and_total_limits_are_terminal_without_partial_append() {
    for (total, row) in [(50_000, 1_500), (1_500, 10_000)] {
        let dir = Dir::new();
        let path = dir.0.join("source.jsonl");
        let mut writer = Observer::create(&path, header(), total, row).unwrap();
        let before = std::fs::read(&path).unwrap();
        let snapshot = source_snapshot();
        assert!(matches!(writer.record_source(&snapshot), Err(Error::Limit)));
        assert_eq!(std::fs::read(&path).unwrap(), before);
        assert!(matches!(
            writer.record_source(&snapshot),
            Err(Error::Closed)
        ));
        assert!(matches!(writer.record_step(&step()), Err(Error::Closed)));
    }
}

#[test]
fn run_plan_is_retained_and_uses_terminal_output_bounds() {
    let dir = Dir::new();
    let path = dir.0.join("plan.jsonl");
    let mut writer = Observer::create(&path, header(), 50_000, 2_000).unwrap();
    let plan = json!({"instrument":"IMOEXF", "deadline_ms":12345});
    writer.record_run_plan(&plan).unwrap();
    let before = std::fs::read(&path).unwrap();
    let row: serde_json::Value =
        serde_json::from_slice(before.split(|b| *b == b'\n').nth(1).unwrap()).unwrap();
    assert_eq!(row["kind"], "run_plan");
    assert_eq!(row["event"], plan);
    assert!(matches!(
        writer.record_run_plan(&json!({"large":"x".repeat(3_000)})),
        Err(Error::Limit)
    ));
    assert_eq!(std::fs::read(&path).unwrap(), before);
    assert!(matches!(writer.record_run_plan(&plan), Err(Error::Closed)));
}

#[test]
fn writes_seed_no_signal_step_and_end_without_overwrite() {
    let dir = Dir::new();
    let path = dir.0.join("evidence.jsonl");
    let mut writer = Observer::create(&path, header(), 50_000, 10_000).unwrap();
    writer.record_step(&step()).unwrap();
    writer.finish("bounded_stop").unwrap();
    assert!(matches!(writer.record_step(&step()), Err(Error::Closed)));
    let before = std::fs::read(&path).unwrap();
    assert!(Observer::create(&path, header(), 50_000, 10_000).is_err());
    assert_eq!(before, std::fs::read(&path).unwrap());
    let rows: Vec<serde_json::Value> = std::str::from_utf8(&before)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0]["observation"]["mode"], "WARMUP");
    assert_eq!(rows[1]["event"]["mode"], "LIVE_OBSERVED");
    assert_eq!(rows[1]["event"]["decision"]["reason"], "no_signal");
    assert_eq!(rows[2]["sequence"], 2);
}

#[test]
fn duplicate_or_bad_clock_stops_writing() {
    for duplicate in [false, true] {
        let dir = Dir::new();
        let path = dir.0.join("evidence.jsonl");
        let mut writer = Observer::create(&path, header(), 50_000, 10_000).unwrap();
        let mut bad = step();
        if duplicate {
            writer.record_step(&bad).unwrap();
        } else {
            bad.result_at_ms = bad.model_start_at_ms - 1;
        }
        let before = std::fs::read(&path).unwrap();
        assert!(matches!(writer.record_step(&bad), Err(Error::Invalid)));
        assert!(matches!(writer.finish("retry"), Err(Error::Closed)));
        assert_eq!(before, std::fs::read(&path).unwrap());
    }
}

#[test]
fn output_budget_rejects_before_appending_row() {
    let dir = Dir::new();
    let path = dir.0.join("evidence.jsonl");
    let mut writer = Observer::create(&path, header(), 1_500, 1_500).unwrap();
    let before = std::fs::read(&path).unwrap();
    assert!(matches!(writer.record_step(&step()), Err(Error::Limit)));
    assert!(matches!(writer.finish("limit"), Err(Error::Closed)));
    assert_eq!(before, std::fs::read(&path).unwrap());
}

#[test]
fn comparator_separates_input_drift_and_model_drift() {
    let a = step();
    let mut b = a.clone();
    b.result_at_ms += 1; // Timing is evidence, not a semantic-state difference.
    assert_eq!(compare_steps(&a, &b), Comparison::SameInputMatch);
    b.canonical_input_sha256 = "c".repeat(64);
    assert_eq!(compare_steps(&a, &b), Comparison::DifferentInput);
    b = a.clone();
    b.after["current_owner"] = "intraday_breakout".into();
    assert_eq!(compare_steps(&a, &b), Comparison::SameInputModelDrift);
    b.mode = ObservationMode::BackfillReplay;
    assert_eq!(compare_steps(&a, &b), Comparison::NotComparable);
}

#[test]
fn comparator_ignores_request_id_value_but_not_pending_presence() {
    let mut a = step();
    let mut b = step();
    a.after["pending_entry_request_id"] = "host-a".into();
    b.after["pending_entry_request_id"] = "host-b".into();
    assert_eq!(compare_steps(&a, &b), Comparison::SameInputMatch);
    b.after["pending_entry_request_id"] = serde_json::Value::Null;
    assert_eq!(compare_steps(&a, &b), Comparison::SameInputModelDrift);
}

#[test]
fn model_reasons_are_preserved_without_inventing_suppression() {
    use finam_signal_shadow::feedback::Action;
    let state = json!({"pending_entry_reason":"breakout_long",
        "pending_exit_reason":"breakout_stop2_long"});
    for (action, reason) in [
        (Action::EnterLong, "breakout_long"),
        (Action::Exit, "breakout_stop2_long"),
    ] {
        assert_eq!(
            Decision::from_model_observation(Some(action), 1, &state)
                .unwrap()
                .reason,
            reason
        );
    }
    assert_eq!(
        Decision::from_model_observation(None, 0, &state)
            .unwrap()
            .reason,
        "model_returned_no_intent"
    );
    assert!(Decision::from_model_observation(Some(Action::Exit), 1, &json!({})).is_err());
    assert!(Decision::from_model_observation(None, 1, &state).is_err());
    assert!(Decision::from_model_observation(Some(Action::Exit), 0, &state).is_err());
}

#[test]
fn report_binding_rejects_identity_seed_and_state_mismatches() {
    let a = header();
    let mut b = header();
    b.run_id = "independent-reference-run".into();
    b.segment_id = "independent-reference-segment".into();
    assert!(report_identities_compatible(&a, &b));
    assert_eq!(
        compare_report_steps(&a, &step(), &b, &step()),
        Comparison::SameInputMatch
    );
    for field in 0..10 {
        let mut b = header();
        match field {
            0 => b.semantic_config_sha256 = "a".repeat(64),
            1 => b.seed_sha256 = "f".repeat(64),
            2 => b.model_sha256 = "f".repeat(64),
            3 => b.reference_sha256 = "f".repeat(64),
            4 => b.feedback_policy_sha256 = "f".repeat(64),
            5 => b.last_history_label_utc += 600,
            6 => b.observation_start_utc_ms += 600_000,
            7 => b.virtual_position_lots = 1,
            8 => b.state["entry_ready"] = false.into(),
            _ => b.live_observation_count = 1,
        }
        assert_eq!(
            compare_report_steps(&a, &step(), &b, &step()),
            Comparison::NotComparable,
            "case {field}"
        );
    }
    let mut invalid = header();
    invalid.model_sha256 = "unknown".into();
    assert!(!report_identities_compatible(&invalid, &invalid));
    let mut invalid = header();
    invalid.config_sha256.clear();
    assert!(!report_identities_compatible(&invalid, &invalid));
    let mut invalid = header();
    invalid.semantic_config_sha256.clear();
    assert!(!report_identities_compatible(&invalid, &invalid));
}

#[test]
fn comparator_preserves_level_and_reason_drift() {
    let mut a = step();
    a.after["bo_levels"] = json!({"long_entry":110.0});
    a.after["pending_exit_reason"] = "breakout_stop1_long".into();
    let mut b = a.clone();
    b.after["bo_levels"]["long_entry"] = 111.0.into();
    assert_eq!(compare_steps(&a, &b), Comparison::SameInputModelDrift);
    b = a.clone();
    b.after["pending_exit_reason"] = "breakout_stop2_long".into();
    assert_eq!(compare_steps(&a, &b), Comparison::SameInputModelDrift);
    b = a.clone();
    b.decision.reason = "another_reason".into();
    assert_eq!(compare_steps(&a, &b), Comparison::SameInputModelDrift);
}

#[cfg(unix)]
#[test]
fn existing_symlink_is_not_followed() {
    let dir = Dir::new();
    let target = dir.0.join("target");
    let path = dir.0.join("evidence");
    std::fs::write(&target, b"keep").unwrap();
    std::os::unix::fs::symlink(&target, &path).unwrap();
    assert!(Observer::create(&path, header(), 50_000, 10_000).is_err());
    assert_eq!(std::fs::read(target).unwrap(), b"keep");
}
