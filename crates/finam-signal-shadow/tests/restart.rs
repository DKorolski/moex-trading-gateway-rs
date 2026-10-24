use broker_core::*;
use broker_finam::sparse_m10::*;
use chrono::{DateTime, Utc};
use finam_signal_shadow::{
    feedback,
    input::ClosedInputAdapter,
    prefix::*,
    profile,
    quote::{QuoteInput, QUOTE_DEDUP_CAPACITY},
    replay::OfflineReplay,
    seed::*,
    segment::SeededSegment,
    ValidatedConfig,
};
use serde_json::json;
use strategy_runtime_core::HybridIntradayRuntimeStrategy;

struct Dir(std::path::PathBuf);
impl Dir {
    fn new() -> Self {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let p = std::env::temp_dir().join(format!(
            "prefix-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
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
    config: finam_signal_shadow::ShadowConfig,
    reference: Vec<u8>,
    mapping: Vec<u8>,
    calendar: Vec<u8>,
    seed: Vec<u8>,
}
fn instrument() -> InstrumentId {
    InstrumentId {
        symbol: "IMOEXF".into(),
        venue_symbol: Some("IMOEXF@RTSX".into()),
        exchange: Exchange::Moex,
        market: Market::Futures,
    }
}
fn time(ms: i64) -> DateTime<Utc> {
    DateTime::from_timestamp_millis(ms).unwrap()
}
fn fixture() -> Fixture {
    let dates = ["2026-09-22", "2026-09-23", "2026-09-24", "2026-09-25"];
    let sessions = dates.map(|date| SessionInventory {
        date: date.into(),
        required_m10_labels_utc: ["04:00:00", "05:00:00", "20:40:00"]
            .map(|t| {
                DateTime::parse_from_rfc3339(&format!("{date}T{t}Z"))
                    .unwrap()
                    .timestamp()
            })
            .to_vec(),
    });
    let bars: Vec<_> = sessions
        .iter()
        .flat_map(|s| &s.required_m10_labels_utc)
        .map(|&label| HybridRuntimeBarEvent {
            instrument: instrument(),
            close_time_utc: label,
            open: 2200.0,
            high: 2201.0,
            low: 2199.0,
            close: 2200.5,
            volume: 10.0,
            origin: HybridRuntimeBarOrigin::History,
            is_final: true,
            timeframe_sec: 600,
        })
        .collect();
    let reference = b"pinned-test-reference-not-live".to_vec();
    let mapping = serde_json::to_vec(&InstrumentMapping {
        instrument: instrument(),
        tick_size: "0.5".into(),
        lot_size: 1,
    })
    .unwrap();
    let calendar = serde_json::to_vec(&CalendarInventory { sessions }).unwrap();
    let seed = serde_json::to_vec(&bars).unwrap();
    let start = DateTime::parse_from_rfc3339("2026-09-28T04:00:00Z")
        .unwrap()
        .timestamp_millis();
    let config=ValidatedConfig::parse(&serde_json::to_vec(&json!({
        "schema_version":1,"reference":"alor_freeze_2026_09_27","restart_mode":"new_segment",
        "model_profile":"imoexf-baseline07-bo-only-no-riskgate",
        "model_sha256":HybridIntradayRuntimeStrategy::new(profile::baseline07_config().unwrap()).stage5c_config_fingerprint(),
        "reference_sha256":digest(&reference),"feedback_policy_sha256":feedback::policy_fingerprint(),
        "instrument_mapping_sha256":digest(&mapping),"calendar_sha256":digest(&calendar),"seed_sha256":digest(&seed),
        "instrument":"IMOEXF@RTSX","run_id":"restart-test","segment_id":"new","previous_segment_id":"old",
        "warmup_session_dates":dates,"warmup_range_start_utc_ms":bars[0].close_time_utc*1000,
        "warmup_range_end_utc_ms":(bars.last().unwrap().close_time_utc+600)*1000,
        "observation_start_utc_ms":start,"run_deadline_utc_ms":start+86_400_000,
        "lmax_ms":1000,"max_quote_age_ms":1000,"clock_uncertainty_ms":10,
        "ws_silence_budget_ms":1000,"reconnect_budget_ms":1000,"stop_timeout_ms":1000,
        "max_input_bytes":2_000_000,"max_cache_bars":4096,"max_output_bytes":10_000_000
    })).unwrap()).unwrap().config().clone();
    Fixture {
        config,
        reference,
        mapping,
        calendar,
        seed,
    }
}
impl Fixture {
    fn segment(&self) -> SeededSegment {
        SeededSegment::from_inputs(
            ValidatedConfig::validate(self.config.clone()).unwrap(),
            &self.reference,
            &self.mapping,
            &self.calendar,
            &self.seed,
        )
        .unwrap()
    }
    fn binding(&self, events: &[PrefixEvent]) -> (PrefixBinding, Vec<u8>) {
        let bytes = serde_json::to_vec(events).unwrap();
        let frontier = match events.last().unwrap() {
            PrefixEvent::Snapshot { result_at_ms, .. } => *result_at_ms,
            PrefixEvent::Quotes { applied_at_ms, .. } => *applied_at_ms,
        };
        let count = events
            .iter()
            .filter(|e| matches!(e, PrefixEvent::Snapshot { .. }))
            .count();
        (
            PrefixBinding {
                schema_version: 1,
                config_sha256: ValidatedConfig::validate(self.config.clone())
                    .unwrap()
                    .fingerprint()
                    .into(),
                prefix_sha256: digest(&bytes),
                prior_output_inventory_sha256: digest(b"externally-approved-test-inventory"),
                previous_segment_id: "old".into(),
                quote_subscription_key: "pinned-quotes".into(),
                event_count: events.len(),
                next_label_utc_ms: self.config.observation_start_utc_ms + count as i64 * 600_000,
                frontier_utc_ms: frontier,
                max_elapsed_ms: 10_000,
            },
            bytes,
        )
    }
}

fn snapshot(f: &Fixture, index: i64) -> PrefixEvent {
    let start = time(f.config.observation_start_utc_ms + index * 600_000);
    let end = start + chrono::Duration::minutes(10);
    let (open, high, low, close) = if index >= 18 {
        ("2205", "2210", "2204", "2205")
    } else {
        ("2200", "2201", "2199", "2200.5")
    };
    let raw=json!({"symbol":"IMOEXF@RTSX","bars":[{"timestamp":start.to_rfc3339(),"open":{"value":open},"high":{"value":high},"low":{"value":low},"close":{"value":close},"volume":{"value":"10"}}]}).to_string();
    PrefixEvent::Snapshot {
        evidence: ClosedM1SnapshotEvidenceV1 {
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
                received_at: end + chrono::Duration::milliseconds(2),
                status: 200,
                transport_complete: true,
                declared_body_bytes: Some(raw.len() as u64),
                response_sha256: digest(raw.as_bytes()),
                raw_body: raw,
            }],
        },
        result_at_ms: end.timestamp_millis() + 4,
    }
}
fn quotes(at: i64) -> PrefixEvent {
    PrefixEvent::Quotes {raw_frame:json!({"type":"DATA","subscriptionType":"QUOTES","subscriptionKey":"pinned-quotes","payload":{"quote":[{"symbol":"IMOEXF@RTSX","timestamp":time(at+1).to_rfc3339(),"last":{"value":"2205"}}]}}).to_string(),received_at_ms:at+2,applied_at_ms:at+3}
}
fn events(f: &Fixture, filled: bool, end: bool) -> Vec<PrefixEvent> {
    let mut result: Vec<_> = (0..19).map(|i| snapshot(f, i)).collect();
    if filled {
        result.push(quotes(f.config.observation_start_utc_ms + 19 * 600_000 + 4));
    }
    if end {
        result.extend((19..100).map(|i| snapshot(f, i)));
        result.push(quotes(
            f.config.observation_start_utc_ms + 100 * 600_000 + 4,
        ));
    }
    result
}

// Independent uninterrupted coordinator: source admission/normalization feeds
// the existing actual OfflineReplay callbacks. No prefix state is copied in.
fn uninterrupted(f: &Fixture, events: &[PrefixEvent], path: &std::path::Path) -> OfflineReplay {
    let mut replay = OfflineReplay::create(f.segment(), path).unwrap();
    let mut adapter = ClosedInputAdapter::new(instrument(), 4096).unwrap();
    let mut quotes = QuoteInput::new(1000, QUOTE_DEDUP_CAPACITY).unwrap();
    for event in events {
        match event {
            PrefixEvent::Snapshot {
                evidence,
                result_at_ms,
            } => {
                let plan = ClosedM1RequestPlan::new(
                    evidence.start,
                    evidence.end,
                    evidence.parts.iter().map(|p| p.end).collect(),
                )
                .unwrap();
                let snapshot =
                    AdmittedClosedM1Snapshot::admit(evidence.clone(), &plan, time(*result_at_ms))
                        .unwrap();
                let inputs = adapter.prepare(&snapshot, &[evidence.start]).unwrap();
                for input in inputs {
                    replay
                        .step(input.bar, &input.snapshot_sha256, *result_at_ms)
                        .unwrap();
                }
            }
            PrefixEvent::Quotes {
                raw_frame,
                received_at_ms,
                applied_at_ms,
            } => {
                for quote in quotes.receive(raw_frame, time(*received_at_ms)).unwrap() {
                    replay.quote(quote, *applied_at_ms).unwrap();
                }
            }
        }
    }
    replay
}
fn semantic(mut state: serde_json::Value) -> serde_json::Value {
    for key in ["pending_entry_request_id", "pending_exit_request_id"] {
        state[key] = (!state[key].is_null()).into();
    }
    state
}

#[test]
fn uninterrupted_and_reconstructed_actual_feedback_pending_nonflat_and_side_used() {
    let f = fixture();
    let dir = Dir::new();
    for (case, filled, end) in [
        ("pending", false, false),
        ("nonflat", true, false),
        ("exited", true, true),
    ] {
        let input = events(&f, filled, end);
        let baseline = uninterrupted(&f, &input, &dir.0.join(format!("baseline-{case}")));
        let (binding, bytes) = f.binding(&input);
        let path = dir.0.join(case);
        let restart = reconstruct(f.segment(), binding.clone(), &bytes, &path).unwrap();
        assert_eq!(
            semantic(baseline.observation()),
            semantic(restart.observation()),
            "{case}"
        );
        assert_eq!(restart.pending().is_some(), !filled, "{case}");
        assert_eq!(
            restart.position_lots(),
            if filled && !end { 1 } else { 0 },
            "{case}"
        );
        if filled {
            assert_eq!(restart.observation()["was_long_today"], true);
        }
        let rows: Vec<serde_json::Value> = std::fs::read_to_string(&path)
            .unwrap()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        let baseline_rows: Vec<serde_json::Value> =
            std::fs::read_to_string(dir.0.join(format!("baseline-{case}")))
                .unwrap()
                .lines()
                .map(|l| serde_json::from_str(l).unwrap())
                .collect();
        let fills = |rows: Vec<serde_json::Value>| {
            rows.into_iter()
                .filter(|r| r["kind"] == "hypothetical_feedback")
                .map(|r| r["event"].clone())
                .collect::<Vec<_>>()
        };
        assert_eq!(fills(rows.clone()), fills(baseline_rows));
        assert_eq!(
            rows.last().unwrap()["event"]["prefix_binding"]["prefix_sha256"],
            binding.prefix_sha256
        );
        assert!(!std::fs::read_to_string(&path)
            .unwrap()
            .contains("LIVE_OBSERVED"));
        assert_eq!(restart.finish().unwrap(), end);
    }
}

#[test]
fn independently_received_rest_and_ws_queues_replay_without_retimestamping() {
    let f = fixture();
    let dir = Dir::new();
    for quote_queued in [true, false] {
        let mut input = events(&f, false, false);
        if quote_queued {
            // REST received T+5, callback T+10. Quote source T+7,
            // receive T+8, application T+12: valid but not fill-eligible.
            let t = f.config.observation_start_utc_ms + 19 * 600_000;
            if let PrefixEvent::Snapshot {
                evidence,
                result_at_ms,
            } = input.last_mut().unwrap()
            {
                evidence.parts[0].received_at = time(t + 5);
                *result_at_ms = t + 10;
            }
            let mut quote = quotes(t + 6);
            if let PrefixEvent::Quotes { applied_at_ms, .. } = &mut quote {
                *applied_at_ms = t + 12;
            }
            input.push(quote);
        } else {
            // Next REST received T+2 before queued quote applies T+3;
            // callback still occurs after that quote at T+4.
            let t = f.config.observation_start_utc_ms + 20 * 600_000;
            input.push(quotes(t));
            input.push(snapshot(&f, 19));
        }
        let name = if quote_queued {
            "queued-ws"
        } else {
            "queued-rest"
        };
        let baseline = uninterrupted(&f, &input, &dir.0.join(format!("{name}-baseline")));
        let (binding, bytes) = f.binding(&input);
        let path = dir.0.join(name);
        let rebuilt = reconstruct(f.segment(), binding, &bytes, &path).unwrap();
        assert_eq!(
            semantic(rebuilt.observation()),
            semantic(baseline.observation())
        );
        assert_eq!(rebuilt.pending().is_some(), quote_queued);
        assert_eq!(rebuilt.position_lots(), if quote_queued { 0 } else { 1 });
        assert_eq!(rebuilt.observation()["was_long_today"], !quote_queued);
        let rows: Vec<serde_json::Value> = std::fs::read_to_string(path)
            .unwrap()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        for event in &input {
            if let PrefixEvent::Snapshot {
                evidence,
                result_at_ms,
            } = event
            {
                let exact = serde_json::to_value(evidence).unwrap();
                assert!(rows
                    .iter()
                    .any(|r| r["kind"] == "source_snapshot" && r["event"]["evidence"] == exact));
                assert!(rows.iter().any(|r| r["kind"] == "model_step"
                    && r["event"]["result_at_ms"] == *result_at_ms
                    && r["event"]["received_at_ms"]
                        == evidence
                            .parts
                            .last()
                            .unwrap()
                            .received_at
                            .timestamp_millis()));
            }
            if let PrefixEvent::Quotes {
                raw_frame,
                received_at_ms,
                applied_at_ms,
            } = event
            {
                let row = rows
                    .iter()
                    .find(|r| r["event"]["event"] == "prefix_quotes")
                    .unwrap();
                assert_eq!(row["event"]["raw_frame"], *raw_frame);
                assert_eq!(row["event"]["received_at_ms"], *received_at_ms);
                assert_eq!(row["event"]["applied_at_ms"], *applied_at_ms);
                assert_eq!(row["event"]["mode"], "BACKFILL_REPLAY");
            }
        }
        assert_eq!(serde_json::to_vec(&input).unwrap(), bytes);
        // Effect rollback and receive-after-own-effect remain invalid.
        for receive_after in [false, true] {
            let mut broken = input.clone();
            match broken.last_mut().unwrap() {
                PrefixEvent::Quotes {
                    received_at_ms,
                    applied_at_ms,
                    ..
                } => {
                    if receive_after {
                        *received_at_ms = *applied_at_ms + 1;
                    } else {
                        *applied_at_ms -= 10;
                    }
                }
                PrefixEvent::Snapshot {
                    evidence,
                    result_at_ms,
                } => {
                    if receive_after {
                        evidence.parts[0].received_at = time(*result_at_ms + 1);
                    } else {
                        *result_at_ms -= 10;
                    }
                }
            }
            let (binding, bytes) = f.binding(&broken);
            assert!(reconstruct(
                f.segment(),
                binding,
                &bytes,
                &dir.0.join(format!("{name}-bad-{receive_after}"))
            )
            .is_err());
        }
    }
}

#[test]
fn corrupt_gapped_future_unbound_and_wrong_source_prefixes_refuse() {
    let f = fixture();
    let dir = Dir::new();
    for case in 0..11 {
        let mut input = events(&f, true, false);
        match case {
            0 => {
                input.remove(1);
            }
            1 => {
                if let PrefixEvent::Snapshot { evidence, .. } = &mut input[0] {
                    evidence.parts[0].raw_body.push(' ');
                }
            }
            2 => {
                if let PrefixEvent::Snapshot { evidence, .. } = &mut input[0] {
                    evidence.parts[0].transport_complete = false;
                }
            }
            3 => {
                if let PrefixEvent::Quotes { raw_frame, .. } = input.last_mut().unwrap() {
                    *raw_frame = raw_frame.replace("pinned-quotes", "other-quotes");
                }
            }
            _ => {}
        }
        let (mut binding, mut bytes) = f.binding(&input);
        match case {
            4 => bytes.push(b' '),
            5 => binding.frontier_utc_ms = Utc::now().timestamp_millis() + 86_400_000,
            6 => binding.max_elapsed_ms = 0,
            7 => binding.event_count += 1,
            8 => binding.config_sha256 = "0".repeat(64),
            9 => binding.previous_segment_id = "another".into(),
            10 => {
                binding.next_label_utc_ms += 600_000;
                binding.frontier_utc_ms = binding.next_label_utc_ms;
            }
            _ => {}
        }
        assert!(
            reconstruct(
                f.segment(),
                binding,
                &bytes,
                &dir.0.join(format!("bad-{case}"))
            )
            .is_err(),
            "{case}"
        );
    }
}

#[test]
fn restart_cli_reconstructs_nonflat_without_live_rows_or_overwriting() {
    let f = fixture();
    let dir = Dir::new();
    let (binding, bytes) = f.binding(&events(&f, true, false));
    let files = [
        ("config", serde_json::to_vec(&f.config).unwrap()),
        ("reference", f.reference.clone()),
        ("mapping", f.mapping.clone()),
        ("calendar", f.calendar.clone()),
        ("seed", f.seed.clone()),
        ("binding", serde_json::to_vec(&binding).unwrap()),
        ("prefix", bytes),
    ];
    let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_finam-signal-shadow"));
    command.arg("restart");
    for (name, bytes) in files {
        let path = dir.0.join(name);
        std::fs::write(&path, bytes).unwrap();
        command.arg(path);
    }
    let output = dir.0.join("output");
    command.arg(&output);
    let result = command.output().unwrap();
    assert_eq!(result.status.code(), Some(2), "{result:?}");
    let before = std::fs::read(&output).unwrap();
    assert!(!String::from_utf8_lossy(&before).contains("LIVE_OBSERVED"));
    assert!(String::from_utf8_lossy(&before).contains("INCOMPLETE_prefix_nonflat"));
    assert_eq!(command.output().unwrap().status.code(), Some(1));
    assert_eq!(std::fs::read(&output).unwrap(), before);
}

fn approval(prefix: &ReconstructedPrefix) -> finam_signal_shadow::runner::PrefixRunPlan {
    use finam_signal_shadow::runner::*;
    let c = prefix.config().config();
    let mut start = prefix.next_label_utc_ms();
    let last = c.observation_start_utc_ms + 100 * 600_000;
    let mut requests = Vec::new();
    while start <= last {
        let end = start + 600_000;
        requests.push(Request {
            request_at: time(end + 100),
            start: time(start),
            end: time(end),
            part_ends: vec![time(end)],
            required: vec![time(start)],
        });
        start = end;
    }
    PrefixRunPlan {
        plan: RunPlan {
            config_sha256: prefix.config().fingerprint().into(),
            account_id: "test-not-an-account".into(),
            read_only_credentials: true,
            auth_policy: "provider_expiry_readonly_rtsx_v1".into(),
            eod_check_at: time(last + 600_000 + 500),
            reconnect_after_data_frames: None,
            requests,
        },
        prefix_binding_sha256: prefix.binding_sha256(),
        source_coverage: PrefixSourceCoverage::OperatorApprovedPinnedPriorOutputInventory {
            inventory_sha256: prefix.binding().prior_output_inventory_sha256.clone(),
        },
    }
}

#[test]
fn independently_pinned_receipt_and_coverage_are_required_for_runner_plan() {
    use finam_signal_shadow::runner::PrefixSourceCoverage;
    let f = fixture();
    let dir = Dir::new();
    let mut input = events(&f, true, false);
    let mut second = quotes(f.config.observation_start_utc_ms + 19 * 600_000 + 20);
    if let PrefixEvent::Quotes { raw_frame, .. } = &mut second {
        *raw_frame = raw_frame.replace("2205", "2206");
    }
    input.push(second);
    let (binding, bytes) = f.binding(&input);
    let prefix = reconstruct(f.segment(), binding, &bytes, &dir.0.join("bound")).unwrap();
    let mut plan = approval(&prefix);
    assert!(plan.validate(&prefix).is_ok());
    // A plan starting at the seed, or one skipping the next bucket, is refused.
    let mut skipped = plan.clone();
    skipped.plan.requests.remove(0);
    assert!(skipped.validate(&prefix).is_err());
    plan.source_coverage = PrefixSourceCoverage::Unproven;
    assert_eq!(
        plan.validate(&prefix),
        Err("runner_prefix_coverage_unproven")
    );
    let plan = approval(&prefix);
    let mut changed = input;
    // Omit the first eligible quote while retaining the later differently
    // priced one, then regenerate the payload binding. Approval must fail.
    changed.remove(19);
    let (binding, bytes) = f.binding(&changed);
    let changed = reconstruct(f.segment(), binding, &bytes, &dir.0.join("changed")).unwrap();
    assert_eq!(
        plan.validate(&changed),
        Err("runner_prefix_receipt_binding")
    );
}

#[test]
fn pending_and_stale_handoffs_are_terminal_without_network() {
    use finam_signal_shadow::{
        session::{ObservedSession, SessionError},
        transport::ReadOnlyCollector,
    };
    let f = fixture();
    let dir = Dir::new();
    for filled in [false, true] {
        let (binding, bytes) = f.binding(&events(&f, filled, false));
        let path = dir.0.join(format!("handoff-{filled}"));
        let prefix = reconstruct(f.segment(), binding, &bytes, &path).unwrap();
        let plan = approval(&prefix);
        let (stop, _) = ReadOnlyCollector::new(
            Utc::now() + chrono::Duration::seconds(60),
            std::time::Duration::from_secs(1),
        )
        .unwrap();
        assert!(matches!(
            ObservedSession::resume(prefix, stop.clone(), &plan),
            Err(SessionError::Incomplete)
        ));
        assert!(stop.is_stopped());
        let text = std::fs::read_to_string(path).unwrap();
        assert!(text.contains("INCOMPLETE_prefix_handoff"));
        assert!(!text.contains("LIVE_OBSERVED"));
    }
}

#[test]
fn current_session_nonflat_handoff_keeps_side_used_and_rejects_old_prefix_quote() {
    use chrono::{Datelike, Timelike};
    use finam_signal_shadow::{
        session::{ObservedSession, SessionError},
        transport::ReadOnlyCollector,
    };
    let now = Utc::now();
    let local = now + chrono::Duration::hours(3);
    // Positive real-clock admission is only possible in its actual session
    // window. Outside it, historical handoff refusal is tested above; never
    // falsify current wall time to get a positive readiness assertion.
    let minute = local.hour() * 60 + local.minute();
    if local.weekday().number_from_monday() > 5
        || !(620..1420).contains(&minute)
        || now.timestamp_millis() % 600_000 > 590_000
    {
        eprintln!("SKIP_CURRENT_SESSION_NONFLAT_HANDOFF: outside eligible fixture window; NO_EVIDENCE_CLOSURE");
        return;
    }
    let mut f = fixture();
    let start = (local.date_naive().and_hms_opt(7, 0, 0).unwrap().and_utc()
        - chrono::Duration::hours(3))
    .timestamp_millis();
    let mut date = local.date_naive();
    let mut dates = Vec::new();
    while dates.len() < 4 {
        date = date.pred_opt().unwrap();
        if date.weekday().number_from_monday() <= 5 {
            dates.push(date);
        }
    }
    dates.reverse();
    let sessions = std::array::from_fn(|i| SessionInventory {
        date: dates[i].to_string(),
        required_m10_labels_utc: [4, 5, 20]
            .into_iter()
            .enumerate()
            .map(|(j, h)| {
                dates[i]
                    .and_hms_opt(h, if j == 2 { 40 } else { 0 }, 0)
                    .unwrap()
                    .and_utc()
                    .timestamp()
            })
            .collect(),
    });
    let mut bars: Vec<HybridRuntimeBarEvent> = serde_json::from_slice(&f.seed).unwrap();
    for (bar, label) in bars
        .iter_mut()
        .zip(sessions.iter().flat_map(|s| &s.required_m10_labels_utc))
    {
        bar.close_time_utc = *label;
    }
    f.calendar = serde_json::to_vec(&CalendarInventory { sessions }).unwrap();
    f.seed = serde_json::to_vec(&bars).unwrap();
    f.config.calendar_sha256 = digest(&f.calendar);
    f.config.seed_sha256 = digest(&f.seed);
    f.config.warmup_session_dates = std::array::from_fn(|i| dates[i].to_string());
    f.config.warmup_range_start_utc_ms = bars[0].close_time_utc * 1000;
    f.config.warmup_range_end_utc_ms = (bars.last().unwrap().close_time_utc + 600) * 1000;
    f.config.observation_start_utc_ms = start;
    f.config.run_deadline_utc_ms = start + 18 * 3_600_000;
    let count = (now.timestamp_millis() - start) / 600_000;
    let mut input = events(&f, true, false);
    input.extend((19..count).map(|i| snapshot(&f, i)));
    let (binding, bytes) = f.binding(&input);
    let dir = Dir::new();
    let path = dir.0.join("current");
    let prefix = reconstruct(f.segment(), binding, &bytes, &path).unwrap();
    assert_eq!(prefix.position_lots(), 1);
    assert!(prefix.pending().is_none());
    let before = prefix.observation();
    let frontier = prefix.frontier_utc_ms();
    let plan = approval(&prefix);
    let (stop, _) = ReadOnlyCollector::new(
        Utc::now() + chrono::Duration::seconds(60),
        std::time::Duration::from_secs(1),
    )
    .unwrap();
    let mut session = ObservedSession::resume(prefix, stop.clone(), &plan).unwrap();
    let rows: Vec<serde_json::Value> = std::fs::read_to_string(&path)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    let handoff = &rows.last().unwrap()["event"];
    assert_eq!(handoff["event"], "prefix_handoff");
    assert_eq!(handoff["state"], before);
    assert_eq!(handoff["position_lots"], 1);
    assert_eq!(handoff["state"]["was_long_today"], true);
    let feedback_count = rows
        .iter()
        .filter(|r| r["kind"] == "hypothetical_feedback")
        .count();
    assert_eq!(
        session.quote(feedback::Quote {
            instrument: "IMOEXF".into(),
            source_id: "old-prefix-quote".into(),
            price_ticks: 4410,
            event_at_ms: frontier,
            received_at_ms: Utc::now().timestamp_millis()
        }),
        Err(SessionError::Input)
    );
    assert!(stop.is_stopped());
    let rows: Vec<serde_json::Value> = std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(
        rows.iter()
            .filter(|r| r["kind"] == "hypothetical_feedback")
            .count(),
        feedback_count
    );
    println!("CURRENT_SESSION_NONFLAT_HANDOFF_ASSERTIONS_COMPLETED");
}
