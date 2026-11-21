use super::*;
use crate::{feedback, replay::OfflineReplay, seed::*, ShadowConfig};
use broker_core::*;
use broker_finam::sparse_m10::*;
use chrono::Datelike;
use serde_json::{json, Value};

struct Dir(std::path::PathBuf);
impl Dir {
    fn new() -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        let p = std::env::temp_dir().join(format!(
            "candle-intraday-{}-{}",
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
struct Fixture {
    config: ShadowConfig,
    reference: Vec<u8>,
    mapping: Vec<u8>,
    calendar: Vec<u8>,
    seed: Vec<u8>,
    plan: IntradayRunPlan,
    evidence: ClosedM1SnapshotEvidenceV1,
    bytes: Vec<u8>,
    now: i64,
}
fn instrument() -> InstrumentId {
    InstrumentId {
        symbol: "IMOEXF".into(),
        venue_symbol: Some("IMOEXF@RTSX".into()),
        exchange: Exchange::Moex,
        market: Market::Futures,
    }
}
fn fixture(count: usize) -> Fixture {
    // Future synthetic weekday avoids dependence on the wall clock in live
    // handoff plumbing tests. Only private test-clock seams accept virtual now.
    let mut date = (Utc::now() + chrono::Duration::days(2)).date_naive();
    while date.weekday().number_from_monday() > 5 {
        date = date.succ_opt().unwrap();
    }
    fixture_at(count, date)
}
fn fixture_at(count: usize, date: chrono::NaiveDate) -> Fixture {
    let start = date
        .and_hms_opt(4, 0, 0)
        .unwrap()
        .and_utc()
        .timestamp_millis();
    let mut day = date;
    let mut days = Vec::new();
    while days.len() < 4 {
        day = day.pred_opt().unwrap();
        if day.weekday().number_from_monday() <= 5 {
            days.push(day);
        }
    }
    days.reverse();
    let dates: [String; 4] = days
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .try_into()
        .unwrap();
    let sessions = days
        .iter()
        .map(|d| SessionInventory {
            date: d.to_string(),
            required_m10_labels_utc: (0..101)
                .map(|i| d.and_hms_opt(4, 0, 0).unwrap().and_utc().timestamp() + i * 600)
                .collect(),
        })
        .collect::<Vec<_>>();
    let bars: Vec<_> = sessions
        .iter()
        .flat_map(|s| &s.required_m10_labels_utc)
        .map(|&t| HybridRuntimeBarEvent {
            instrument: instrument(),
            close_time_utc: t,
            timeframe_sec: 600,
            is_final: true,
            open: 2200.0,
            high: 2201.0,
            low: 2199.0,
            close: 2200.5,
            volume: 10.0,
            origin: HybridRuntimeBarOrigin::History,
        })
        .collect();
    let reference = b"synthetic historical reference; not current ALOR parity".to_vec();
    let mapping = serde_json::to_vec(&InstrumentMapping {
        instrument: instrument(),
        tick_size: "0.5".into(),
        lot_size: 1,
    })
    .unwrap();
    let calendar = serde_json::to_vec(&CalendarInventory {
        sessions: sessions.try_into().unwrap(),
    })
    .unwrap();
    let seed = serde_json::to_vec(&bars).unwrap();
    let config=ValidatedConfig::parse(&serde_json::to_vec(&json!({
        "schema_version":1,"reference":"alor_freeze_2026_09_27","restart_mode":"new_segment",
        "model_profile":"imoexf-baseline07-bo-only-no-riskgate",
        "model_sha256":strategy_runtime_core::HybridIntradayRuntimeStrategy::new(crate::profile::baseline07_config().unwrap()).stage5c_config_fingerprint(),
        "reference_sha256":digest(&reference),"feedback_policy_sha256":feedback::candle_policy_fingerprint(),
        "instrument_mapping_sha256":digest(&mapping),"calendar_sha256":digest(&calendar),"seed_sha256":digest(&seed),
        "instrument":"IMOEXF@RTSX","run_id":"intraday-test","segment_id":"new","previous_segment_id":null,
        "warmup_session_dates":dates,"warmup_range_start_utc_ms":bars[0].close_time_utc*1000,
        "warmup_range_end_utc_ms":(bars.last().unwrap().close_time_utc+600)*1000,
        "observation_start_utc_ms":start,"run_deadline_utc_ms":start+61_200_000,
        "lmax_ms":1000,"max_quote_age_ms":1000,"clock_uncertainty_ms":100,
        "ws_silence_budget_ms":1000,"reconnect_budget_ms":1000,"stop_timeout_ms":1000,
        "max_input_bytes":2_000_000,"max_cache_bars":4096,"max_output_bytes":20_000_000
    })).unwrap()).unwrap().config().clone();
    let end = time(start + count as i64 * 600_000).unwrap();
    let now = end.timestamp_millis() + 1000;
    let raw =
        json!({"symbol":"IMOEXF@RTSX","bars":(0..count).map(|i| row(start,i)).collect::<Vec<_>>()})
            .to_string();
    let evidence = ClosedM1SnapshotEvidenceV1 {
        policy: OBSERVED_M1_POLICY_V1.into(),
        start: time(start).unwrap(),
        end,
        parts: vec![ClosedM1RestPartV1 {
            method: "GET".into(),
            endpoint: "https://api.finam.ru/v1/instruments/IMOEXF@RTSX/bars".into(),
            symbol: "IMOEXF@RTSX".into(),
            timeframe: "TIME_FRAME_M1".into(),
            start: time(start).unwrap(),
            end,
            requested_at: end,
            received_at: end + chrono::Duration::milliseconds(2),
            status: 200,
            transport_complete: true,
            declared_body_bytes: Some(raw.len() as u64),
            response_sha256: digest(raw.as_bytes()),
            raw_body: raw,
        }],
    };
    let bytes = serde_json::to_vec(&evidence).unwrap();
    let validated = ValidatedConfig::validate(config.clone()).unwrap();
    let plan = IntradayRunPlan {
        schema_version: 1,
        history_sha256: digest(&bytes),
        history_part_ends: vec![end],
        next_label_utc_ms: end.timestamp_millis(),
        start_not_before_utc_ms: end.timestamp_millis(),
        start_before_utc_ms: end.timestamp_millis() + 300_000,
        max_reconstruction_ms: 10_000,
        plan: RunPlan {
            config_sha256: validated.fingerprint().into(),
            account_id: "synthetic-only".into(),
            read_only_credentials: true,
            auth_policy: "provider_expiry_readonly_rtsx_v1".into(),
            reconnect_after_data_frames: None,
            eod_check_at: time(start + 101 * 600_000 + 500).unwrap(),
            requests: (count..101)
                .map(|i| {
                    let a = time(start + i as i64 * 600_000).unwrap();
                    let b = a + chrono::Duration::minutes(10);
                    crate::runner::Request {
                        request_at: b + chrono::Duration::milliseconds(10),
                        start: a,
                        end: b,
                        part_ends: vec![b],
                        required: vec![a],
                    }
                })
                .collect(),
        },
    };
    Fixture {
        config,
        reference,
        mapping,
        calendar,
        seed,
        plan,
        evidence,
        bytes,
        now,
    }
}

/// Reuse the admitted four-session synthetic seed, not a model state setter.
pub(crate) fn primary_restart_materials() -> (ShadowConfig, [Vec<u8>; 4]) {
    let f = fixture(19);
    (f.config, [f.reference, f.mapping, f.calendar, f.seed])
}
/// Fixed historical SYNTHETIC inputs for actual-clock file-only verification.
/// No existing provider snapshot is re-timestamped; all fake rows are generated
/// from this date, including four prior synthetic warmup sessions.
pub(crate) fn primary_historical_materials() -> (ShadowConfig, [Vec<u8>; 4]) {
    let f = fixture_at(19, chrono::NaiveDate::from_ymd_opt(2026, 9, 28).unwrap());
    (f.config, [f.reference, f.mapping, f.calendar, f.seed])
}
fn row(start: i64, i: usize) -> Value {
    let (o, h, l, c) = if i >= 18 {
        ("2205", "2210", "2204", "2205")
    } else {
        ("2200", "2201", "2199", "2200.5")
    };
    json!({"timestamp":time(start+i as i64*600_000).unwrap().to_rfc3339(),"open":{"value":o},"high":{"value":h},"low":{"value":l},"close":{"value":c},"volume":{"value":"10"}})
}
impl Fixture {
    fn config(&self) -> ValidatedConfig {
        ValidatedConfig::validate(self.config.clone()).unwrap()
    }
    fn segment(&self) -> SeededSegment {
        SeededSegment::from_inputs(
            self.config(),
            &self.reference,
            &self.mapping,
            &self.calendar,
            &self.seed,
        )
        .unwrap()
    }
    fn reconstruct(&self, path: &Path) -> Result<ReconstructedCandleDay, IntradayError> {
        let start = Instant::now();
        reconstruct_with_clock(self.segment(), &self.plan, &self.bytes, path, || {
            self.now + start.elapsed().as_millis() as i64
        })
    }
    fn rebind(&mut self) {
        self.bytes = serde_json::to_vec(&self.evidence).unwrap();
        self.plan.history_sha256 = digest(&self.bytes);
    }
    fn mutate_raw(&mut self, f: impl FnOnce(&mut Value)) {
        let mut raw: Value = serde_json::from_str(&self.evidence.parts[0].raw_body).unwrap();
        f(&mut raw);
        let raw = raw.to_string();
        let p = &mut self.evidence.parts[0];
        p.response_sha256 = digest(raw.as_bytes());
        p.declared_body_bytes = Some(raw.len() as u64);
        p.raw_body = raw;
        self.rebind();
    }
}
fn rows(path: &Path) -> Vec<Value> {
    std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect()
}

#[test]
fn intraday_nonflat_reconstruction_and_remaining_callbacks_match_uninterrupted() {
    let f = fixture(19);
    let d = Dir::new();
    let output = d.0.join("reconstructed");
    let mut day = f.reconstruct(&output).unwrap();
    assert_eq!(day.position_lots(), 1);
    assert!(day.model.pending().is_none());
    let mut replay = OfflineReplay::create(f.segment(), &d.0.join("uninterrupted")).unwrap();
    let snapshot = AdmittedClosedM1Snapshot::admit(
        f.evidence.clone(),
        &f.plan.history_request(&f.config()).unwrap(),
        time(f.now).unwrap(),
    )
    .unwrap();
    let mut adapter = ClosedInputAdapter::new(instrument(), 4096).unwrap();
    let labels: Vec<_> = (0..19)
        .map(|i| time(f.config.observation_start_utc_ms + i * 600_000).unwrap())
        .collect();
    for input in adapter.prepare(&snapshot, &labels).unwrap() {
        replay
            .step(
                input.bar.clone(),
                &input.snapshot_sha256,
                (input.bar.close_time_utc + 600) * 1000,
            )
            .unwrap();
    }
    assert_eq!(day.observation(), replay.observation());
    let history = rows(&output);
    assert_eq!(
        history.iter().filter(|r| r["kind"] == "model_step").count(),
        19
    );
    assert!(history
        .iter()
        .filter(|r| r["kind"] == "model_step" || r["kind"] == "hypothetical_candle_feedback")
        .all(|r| r["event"]["mode"] == "BACKFILL_REPLAY"));
    assert_eq!(
        history
            .iter()
            .find(|r| r["kind"] == "source_snapshot")
            .unwrap()["event"]["evidence"],
        serde_json::to_value(&f.evidence).unwrap()
    );
    // Carry the SAME book/model across the split, with ordinary observed mode.
    day.model.select_observed();
    for i in 19..101 {
        let label = f.config.observation_start_utc_ms + i * 600_000;
        let close = label + 600_000;
        let bar = HybridRuntimeBarEvent {
            instrument: instrument(),
            close_time_utc: label / 1000,
            timeframe_sec: 600,
            is_final: true,
            open: 2205.0,
            high: 2210.0,
            low: 2204.0,
            close: 2205.0,
            volume: 10.0,
            origin: HybridRuntimeBarOrigin::Live,
        };
        let request = day.model.on_bar(bar.clone(), close).unwrap();
        if let Some(r) = request {
            day.model
                .on_candle_close(&r, close, close, &digest(b"synthetic-tail"))
                .unwrap();
        }
        let mut replay_bar = bar;
        replay_bar.origin = HybridRuntimeBarOrigin::Replay;
        replay
            .step(replay_bar, &digest(b"synthetic-tail"), close)
            .unwrap();
        assert_eq!(
            day.observation(),
            replay.observation(),
            "continuation label {i}"
        );
    }
    assert_eq!(day.position_lots(), 0);
}

#[test]
fn intraday_empty_missing_conflicting_and_bad_tick_buckets_are_rejected_before_output() {
    for kind in 0..5 {
        let mut f = fixture(19);
        let d = Dir::new();
        let p = d.0.join("out");
        f.mutate_raw(|v| match kind {
            0 => {
                v["bars"] = json!([]);
            }
            1 => {
                v["bars"].as_array_mut().unwrap().remove(4);
            }
            2 => {
                let mut row = v["bars"][3].clone();
                row["close"]["value"] = json!("2200");
                v["bars"].as_array_mut().unwrap().push(row);
            }
            3 => v["bars"][2]["low"]["value"] = json!("2200.1"),
            _ => v["symbol"] = json!("OTHER@RTSX"),
        });
        assert!(f.reconstruct(&p).is_err(), "kind {kind}");
        assert!(!p.exists());
    }
}

#[test]
fn intraday_partial_future_stale_response_and_hash_tamper_rejected() {
    for kind in 0..5 {
        let mut f = fixture(19);
        let d = Dir::new();
        let p = d.0.join("out");
        match kind {
            0 => f.evidence.parts[0].transport_complete = false,
            1 => f.evidence.parts[0].received_at = time(f.now + 60_000).unwrap(),
            2 => {
                f.evidence.parts[0].received_at = time(f.now - 900_001).unwrap();
                f.evidence.parts[0].requested_at = time(f.now - 900_002).unwrap();
            }
            3 => f.evidence.parts[0].response_sha256 = digest(b"wrong"),
            _ => f.plan.history_sha256 = digest(b"wrong-file"),
        }
        if kind != 4 {
            f.rebind();
        }
        assert!(f.reconstruct(&p).is_err());
        assert!(!p.exists());
    }
}

#[test]
fn intraday_window_plan_profile_and_coverage_are_explicit() {
    let f = fixture(19);
    let config = f.config();
    assert!(f.plan.validate_now(&config, f.now).is_ok());
    for at in [
        f.plan.start_not_before_utc_ms - 1,
        f.plan.start_before_utc_ms,
        f.plan.next_label_utc_ms + 600_000,
    ] {
        assert_eq!(f.plan.validate_now(&config, at), Err(IntradayError::Window));
    }
    for kind in 0..9 {
        let mut p = f.plan.clone();
        match kind {
            0 => p.next_label_utc_ms += 600_000,
            1 => {
                p.plan.requests.remove(0);
            }
            2 => {
                p.plan.requests.pop();
            }
            3 => p.plan.read_only_credentials = false,
            4 => p.plan.config_sha256 = digest(b"other"),
            5 => p.history_part_ends.clear(),
            6 => p.max_reconstruction_ms = 10_001,
            7 => p.plan.requests[0].required[0] = time(f.config.observation_start_utc_ms).unwrap(),
            _ => p.plan.eod_check_at += chrono::Duration::days(1),
        }
        assert!(p.validate(&config).is_err(), "kind {kind}");
    }
    let mut c = f.config.clone();
    c.feedback_policy_sha256 = feedback::policy_fingerprint();
    assert_eq!(
        f.plan.validate(&ValidatedConfig::validate(c).unwrap()),
        Err(IntradayError::Binding)
    );
    let mut c = f.config.clone();
    c.previous_segment_id = Some("old".into());
    assert_eq!(
        f.plan.validate(&ValidatedConfig::validate(c).unwrap()),
        Err(IntradayError::Binding)
    );
    assert!(
        f.plan.plan.validate(&config).is_err(),
        "tail cannot authorize ordinary cold-flat observe"
    );
}

#[test]
fn intraday_sparse_first_last_minute_and_exact_duplicate_preserve_semantics() {
    for offset in [0, 60, 540] {
        let mut f = fixture(19);
        let d = Dir::new();
        f.mutate_raw(|v| {
            for row in v["bars"].as_array_mut().unwrap() {
                let t = DateTime::parse_from_rfc3339(row["timestamp"].as_str().unwrap()).unwrap()
                    + chrono::Duration::seconds(offset);
                row["timestamp"] = json!(t.to_rfc3339());
            }
            let duplicate = v["bars"][3].clone();
            v["bars"].as_array_mut().unwrap().push(duplicate);
        });
        let day = f.reconstruct(&d.0.join("out")).unwrap();
        assert_eq!(day.position_lots(), 1);
        assert_eq!(day.next_label_utc_ms(), f.plan.next_label_utc_ms);
    }
}

#[test]
fn intraday_flat_early_prefix_and_last_bucket_frontier_are_supported() {
    for count in [1, 18, 100] {
        let f = fixture(count);
        let d = Dir::new();
        let day = f.reconstruct(&d.0.join("out")).unwrap();
        assert_eq!(day.next_label_utc_ms(), f.plan.next_label_utc_ms);
        assert!(day.model.pending().is_none());
        if count < 19 {
            assert_eq!(day.position_lots(), 0);
        }
    }
}

#[test]
fn intraday_late_or_changed_handoff_stops_without_live_rows() {
    for changed in [false, true] {
        let f = fixture(19);
        let d = Dir::new();
        let path = d.0.join("out");
        let day = f.reconstruct(&path).unwrap();
        let mut plan = f.plan.clone();
        if changed {
            plan.history_sha256 = digest(b"swapped");
        }
        let (stop, _collector) = crate::transport::ReadOnlyCollector::new(
            Utc::now() + chrono::Duration::seconds(30),
            std::time::Duration::from_secs(1),
        )
        .unwrap();
        let now = if changed {
            f.now
        } else {
            f.plan.start_before_utc_ms
        };
        assert!(crate::session::ObservedSession::resume_intraday_clock(
            day,
            stop.clone(),
            &plan,
            || now
        )
        .is_err());
        assert!(stop.is_stopped());
        assert!(!std::fs::read_to_string(path)
            .unwrap()
            .contains("LIVE_OBSERVED"));
    }
}

#[test]
fn intraday_duplicate_history_after_handoff_is_not_reapplied_and_conflict_is_hold() {
    let f = fixture(19);
    let d = Dir::new();
    let path = d.0.join("out");
    let day = f.reconstruct(&path).unwrap();
    let (stop, _collector) = crate::transport::ReadOnlyCollector::new(
        Utc::now() + chrono::Duration::seconds(30),
        std::time::Duration::from_secs(1),
    )
    .unwrap();
    let mut session =
        crate::session::ObservedSession::resume_intraday_clock(day, stop.clone(), &f.plan, || {
            f.now
        })
        .unwrap();
    let snapshot = AdmittedClosedM1Snapshot::admit(
        f.evidence.clone(),
        &f.plan.history_request(&f.config()).unwrap(),
        time(f.now).unwrap(),
    )
    .unwrap();
    let received = f.evidence.parts[0].received_at.timestamp_millis();
    // The before-fence duplicate path performs no callback, so no fake live time
    // is supplied to production CallbackFence. Test time is only for handoff.
    assert!(session
        .snapshot(&snapshot, &[f.evidence.start], received)
        .is_ok());
    assert_eq!(
        rows(&path)
            .iter()
            .filter(|r| r["kind"] == "model_step")
            .count(),
        19
    );
    let mut other = fixture(19);
    other.mutate_raw(|v| v["bars"][0]["close"]["value"] = json!("2200"));
    let conflicting = AdmittedClosedM1Snapshot::admit(
        other.evidence.clone(),
        &other.plan.history_request(&other.config()).unwrap(),
        time(other.now).unwrap(),
    )
    .unwrap();
    assert!(session
        .snapshot(&conflicting, &[f.evidence.start], received)
        .is_err());
    assert!(stop.is_stopped());
}

#[test]
fn intraday_does_not_overwrite_output_or_promote_partial_reconstruction() {
    let f = fixture(19);
    let d = Dir::new();
    let path = d.0.join("out");
    std::fs::write(&path, b"retained").unwrap();
    assert!(f.reconstruct(&path).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), b"retained");
    let path = d.0.join("late");
    let mut n = 0;
    let result = reconstruct_with_clock(f.segment(), &f.plan, &f.bytes, &path, || {
        n += 1;
        if n < 4 {
            f.now
        } else {
            f.plan.start_before_utc_ms
        }
    });
    assert!(result.is_err());
    let text = std::fs::read_to_string(path).unwrap();
    assert!(text.contains("INCOMPLETE_intraday_reconstruction_failed"));
    assert!(!text.contains("LIVE_OBSERVED"));
}
