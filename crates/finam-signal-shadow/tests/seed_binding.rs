use broker_core::*;
use finam_signal_shadow::{feedback, profile, seed::*, ShadowConfig, ValidatedConfig};
use strategy_runtime_core::HybridIntradayRuntimeStrategy;

struct Fixture {
    config: ShadowConfig,
    reference: Vec<u8>,
    mapping: Vec<u8>,
    calendar: Vec<u8>,
    bars: Vec<HybridRuntimeBarEvent>,
}

fn fixture() -> Fixture {
    let instrument = InstrumentId {
        symbol: "IMOEXF".into(),
        venue_symbol: Some("IMOEXF@RTSX".into()),
        exchange: Exchange::Moex,
        market: Market::Futures,
    };
    let dates = ["2026-09-22", "2026-09-23", "2026-09-24", "2026-09-25"];
    // Explicit synthetic calendar, not a provider session-coverage claim.
    let sessions = dates.map(|date| SessionInventory {
        date: date.into(),
        required_m10_labels_utc: ["04:00:00", "05:00:00", "20:40:00"]
            .map(|time| {
                chrono::DateTime::parse_from_rfc3339(&format!("{date}T{time}Z"))
                    .unwrap()
                    .timestamp()
            })
            .to_vec(),
    });
    let bars: Vec<_> = sessions
        .iter()
        .flat_map(|s| &s.required_m10_labels_utc)
        .map(|&label| HybridRuntimeBarEvent {
            instrument: instrument.clone(),
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
    let reference = b"historical-reference-test-manifest-not-live".to_vec();
    let mapping = serde_json::to_vec(&InstrumentMapping {
        instrument,
        tick_size: "0.5".into(),
        lot_size: 1,
    })
    .unwrap();
    let calendar = serde_json::to_vec(&CalendarInventory { sessions }).unwrap();
    let start = chrono::DateTime::parse_from_rfc3339("2026-09-28T04:00:00Z")
        .unwrap()
        .timestamp_millis();
    let config = ShadowConfig {
        schema_version: 1,
        reference: finam_signal_shadow::Reference::AlorFreeze20260927,
        restart_mode: finam_signal_shadow::RestartMode::NewSegment,
        model_profile: "imoexf-baseline07-bo-only-no-riskgate".into(),
        model_sha256: HybridIntradayRuntimeStrategy::new(profile::baseline07_config().unwrap())
            .stage5c_config_fingerprint(),
        reference_sha256: digest(&reference),
        feedback_policy_sha256: feedback::policy_fingerprint(),
        instrument_mapping_sha256: digest(&mapping),
        calendar_sha256: digest(&calendar),
        seed_sha256: digest(&serde_json::to_vec(&bars).unwrap()),
        instrument: "IMOEXF@RTSX".into(),
        run_id: "test".into(),
        segment_id: "s1".into(),
        previous_segment_id: None,
        warmup_session_dates: dates.map(str::to_owned),
        warmup_range_start_utc_ms: bars[0].close_time_utc * 1000,
        warmup_range_end_utc_ms: (bars.last().unwrap().close_time_utc + 600) * 1000,
        observation_start_utc_ms: start,
        run_deadline_utc_ms: start + 3600000,
        lmax_ms: 1000,
        max_quote_age_ms: 1000,
        clock_uncertainty_ms: 10,
        ws_silence_budget_ms: 1000,
        reconnect_budget_ms: 1000,
        stop_timeout_ms: 1000,
        max_input_bytes: 1_000_000,
        max_cache_bars: 4096,
        max_output_bytes: 1_000_000,
    };
    Fixture {
        config,
        reference,
        mapping,
        calendar,
        bars,
    }
}

#[test]
fn configured_transports_bind_exact_config_and_refuse_expired_or_wrong_target() {
    use finam_signal_shadow::transport::ConfiguredTransports;
    let mut f = fixture();
    // Historical fixture is not a current operational run.
    assert!(
        ConfiguredTransports::new(&ValidatedConfig::validate(f.config.clone()).unwrap()).is_err()
    );
    f.config.run_deadline_utc_ms =
        (chrono::Utc::now() + chrono::Duration::seconds(10)).timestamp_millis();
    let config = ValidatedConfig::validate(f.config.clone()).unwrap();
    let channels = ConfiguredTransports::new(&config).unwrap();
    assert_eq!(channels.fingerprint(), config.fingerprint());
    drop(channels); // stops channels even if a StopHandle clone is retained
    f.config.instrument = "OTHER".into();
    assert!(ConfiguredTransports::new(&ValidatedConfig::validate(f.config).unwrap()).is_err());
}

#[test]
fn measured_model_refuses_late_input_without_callback_and_is_terminal() {
    use finam_signal_shadow::{
        model::ShadowModel, timing::CallbackFence, transport::ConfiguredTransports,
    };
    let mut f = fixture();
    f.config.run_deadline_utc_ms =
        (chrono::Utc::now() + chrono::Duration::seconds(10)).timestamp_millis();
    let config = ValidatedConfig::validate(f.config.clone()).unwrap();
    let channels = ConfiguredTransports::new(&config).unwrap();
    let mut fence = CallbackFence::new(&config, channels.stop_handle()).unwrap();
    let bar = replay_bar(&f);
    let mut model = ShadowModel::baseline07(
        HybridRuntimeStrategyContext {
            strategy_id: "measured-test".into(),
            request_namespace_account: BrokerAccountId("not-broker".into()),
            instrument: bar.instrument.clone(),
            tick_size: 0.5,
            trade_mode: HybridRuntimeTradeMode::Paper,
            paper_execution_mode: HybridRuntimePaperExecutionMode::LiveOnly,
            allow_live_orders: false,
            gateway_phase: HybridRuntimeGatewayPhase::LiveReady,
            position_qty: Some(0.0),
            event_ts_utc: 0,
            strategy_now_ts_utc: 0,
            last_bar_ts_utc: None,
        },
        1000,
    )
    .unwrap();
    let before = model.observation();
    let now = chrono::Utc::now().timestamp_millis();
    assert!(model
        .on_bar_measured(bar.clone(), now, now, &mut fence)
        .is_err());
    assert_eq!(model.observation(), before);
    assert!(model.held());
    assert!(fence.held());
    assert!(model.on_bar(bar, now).is_err());
    assert!(model.pending().is_none());
}

fn cli_fixture(dir: &ReplayDir, f: &Fixture, events: serde_json::Value) -> std::process::Command {
    let inputs = [
        ("config", serde_json::to_vec(&f.config).unwrap()),
        ("reference", f.reference.clone()),
        ("mapping", f.mapping.clone()),
        ("calendar", f.calendar.clone()),
        ("seed", serde_json::to_vec(&f.bars).unwrap()),
        ("events", serde_json::to_vec(&events).unwrap()),
    ];
    let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_finam-signal-shadow"));
    command.arg("replay");
    for (name, bytes) in inputs {
        let path = dir.0.join(name);
        std::fs::write(&path, bytes).unwrap();
        command.arg(path);
    }
    command.arg(dir.0.join("out.jsonl"));
    command
}

#[test]
fn executable_replay_is_offline_and_never_overwrites_output() {
    let f = fixture();
    let dir = ReplayDir::new();
    let bar = replay_bar(&f);
    let now = (bar.close_time_utc + 600) * 1000;
    let mut cmd = cli_fixture(
        &dir,
        &f,
        serde_json::json!([{
            "kind": "bar", "bar": bar, "raw_provenance_sha256": "a".repeat(64),
            "virtual_result_at_ms": now
        }]),
    );
    let first = cmd.output().unwrap();
    assert!(first.status.success(), "{:?}", first);
    assert_eq!(first.stdout, b"OFFLINE_REPLAY_END_FLAT\n");
    let path = dir.0.join("out.jsonl");
    let bytes = std::fs::read(&path).unwrap();
    assert!(!String::from_utf8_lossy(&bytes).contains("LIVE_OBSERVED"));
    assert!(!cmd.output().unwrap().status.success());
    assert_eq!(std::fs::read(path).unwrap(), bytes);
}

#[test]
fn executable_rejects_bad_input_before_output_and_reports_pending_incomplete() {
    let mut f = fixture();
    f.config.run_deadline_utc_ms = f.config.observation_start_utc_ms + 24 * 3_600_000;
    let dir = ReplayDir::new();
    let bad = cli_fixture(&dir, &f, serde_json::json!([{"kind": "live"}]))
        .output()
        .unwrap();
    assert!(!bad.status.success());
    assert!(!dir.0.join("out.jsonl").exists());
    let mut bar = replay_bar(&f);
    let first = bar.clone();
    let first_time = (first.close_time_utc + 600) * 1000;
    bar.close_time_utc += 3 * 3600;
    bar.high = 2210.0;
    bar.close = 2205.0;
    let now = (bar.close_time_utc + 600) * 1000;
    let result = cli_fixture(
        &dir,
        &f,
        serde_json::json!([{
            "kind": "bar", "bar": first, "raw_provenance_sha256": "a".repeat(64),
            "virtual_result_at_ms": first_time
        }, {
            "kind": "bar", "bar": bar, "raw_provenance_sha256": "a".repeat(64),
            "virtual_result_at_ms": now
        }]),
    )
    .output()
    .unwrap();
    assert_eq!(result.status.code(), Some(2), "{:?}", result);
    let text = std::fs::read_to_string(dir.0.join("out.jsonl")).unwrap();
    assert!(text.contains("INCOMPLETE_virtual_pending"));
}

fn check(f: &Fixture) -> Result<SeedReceipt, SeedError> {
    bind_seed(
        &ValidatedConfig::validate(f.config.clone()).unwrap(),
        &f.reference,
        &f.mapping,
        &f.calendar,
        &serde_json::to_vec(&f.bars).unwrap(),
    )
}

#[test]
fn frozen_five_sessions_first_entry_matches_independent_python_reference() {
    frozen_reference_experiment(false);
}

#[test]
fn frozen_five_sessions_rounds_under_explicit_synthetic_close_feedback() {
    frozen_reference_experiment(true);
}

// Conditional test experiment only. Synthetic close quotes are neither observed
// market quotes nor an implementation of the working shadow execution policy.
fn frozen_reference_experiment(synthetic_close: bool) {
    use finam_signal_shadow::{feedback::Action, replay::OfflineReplay};
    let raw = include_bytes!("../../../fixtures/stage8b-p1f-parity/imoexf_raw_10m_msk_utc.csv");
    let reference = include_bytes!(
        "../../../fixtures/stage8b-p1f-parity/baseline07_python_reference_trades.csv"
    );
    assert_eq!(
        digest(raw),
        "e4a30fb81a0abf5711a3702d19b918cad681820380f24422f6ba476b74f92cec"
    );
    assert_eq!(
        digest(reference),
        "2accaf0eec6c81f3446832059c231471415094c6cf43dc91e6ddbba85ccfe384"
    );
    let rows: Vec<_> = csv::Reader::from_reader(raw.as_slice())
        .records()
        .map(Result::unwrap)
        .collect();
    let rounds: Vec<_> = csv::Reader::from_reader(reference.as_slice())
        .records()
        .map(Result::unwrap)
        .collect();
    // Explicit historical comparison inventory, not operational calendar authority.
    let dates = [
        "2026-09-15",
        "2026-09-16",
        "2026-09-17",
        "2026-09-18",
        "2026-09-21",
        "2026-09-22",
        "2026-09-23",
        "2026-09-24",
        "2026-09-25",
    ];
    for index in 4..dates.len() {
        let mut f = fixture();
        let instrument = f.bars[0].instrument.clone();
        let decode = |r: &csv::StringRecord, origin| HybridRuntimeBarEvent {
            instrument: instrument.clone(),
            close_time_utc: chrono::DateTime::parse_from_str(&r[1], "%Y-%m-%d %H:%M:%S%:z")
                .unwrap()
                .timestamp(),
            open: r[3].parse().unwrap(),
            high: r[4].parse().unwrap(),
            low: r[5].parse().unwrap(),
            close: r[6].parse().unwrap(),
            volume: r[7].parse().unwrap(),
            origin,
            is_final: true,
            timeframe_sec: 600,
        };
        let selected = &dates[index - 4..index];
        f.bars = rows
            .iter()
            .filter(|r| {
                selected.contains(&&r[0][..10])
                    && &r[0][11..16] >= "07:00"
                    && &r[0][11..16] <= "23:40"
            })
            .map(|r| decode(r, HybridRuntimeBarOrigin::History))
            .collect();
        assert_eq!(f.bars.len(), 404);
        let sessions: [SessionInventory; 4] = std::array::from_fn(|i| {
            let start = chrono::DateTime::parse_from_rfc3339(&format!("{}T04:00:00Z", selected[i]))
                .unwrap()
                .timestamp();
            SessionInventory {
                date: selected[i].into(),
                required_m10_labels_utc: (0..101).map(|n| start + n * 600).collect(),
            }
        });
        f.calendar = serde_json::to_vec(&CalendarInventory { sessions }).unwrap();
        f.reference = reference.to_vec();
        f.config.reference_sha256 = digest(&f.reference);
        f.config.calendar_sha256 = digest(&f.calendar);
        f.config.seed_sha256 = digest(&serde_json::to_vec(&f.bars).unwrap());
        f.config.warmup_session_dates = std::array::from_fn(|i| selected[i].into());
        f.config.warmup_range_start_utc_ms = f.bars[0].close_time_utc * 1000;
        f.config.warmup_range_end_utc_ms = (f.bars.last().unwrap().close_time_utc + 600) * 1000;
        f.config.observation_start_utc_ms =
            chrono::DateTime::parse_from_rfc3339(&format!("{}T04:00:00Z", dates[index]))
                .unwrap()
                .timestamp_millis();
        f.config.run_deadline_utc_ms = f.config.observation_start_utc_ms + 24 * 3_600_000;
        let expected: Vec<_> = rounds
            .iter()
            .filter(|r| &r[4][..10] == dates[index])
            .collect();
        assert_eq!(expected.len(), 1);
        let expected = expected[0];
        assert_eq!(&expected[0], "bo_only_weekday07_control");
        assert_eq!(&expected[1], "0.0");
        assert_eq!(&expected[2], "BO");
        let dir = ReplayDir::new();
        let mut replay =
            OfflineReplay::create(segment(&f).unwrap(), &dir.0.join("out.jsonl")).unwrap();
        let mut found = false;
        let mut intents = 0;
        let mut processed = 0;
        for r in rows.iter().filter(|r| {
            &r[0][..10] == dates[index] && &r[0][11..16] >= "07:00" && &r[0][11..16] <= "23:40"
        }) {
            let bar = decode(r, HybridRuntimeBarOrigin::Replay);
            let now = (bar.close_time_utc + 600) * 1000;
            let close_ticks = bar.close * 2.0;
            assert_eq!(close_ticks.fract(), 0.0);
            processed += 1;
            if let Some(request) = replay.step(bar, &digest(raw), now).unwrap() {
                let exiting = intents == 1;
                assert!(intents < 2, "extra intent {}", dates[index]);
                assert_eq!(
                    &r[0],
                    &expected[if exiting { 5 } else { 4 }],
                    "intent label {}",
                    dates[index]
                );
                assert_eq!(
                    request.action,
                    if exiting {
                        Action::Exit
                    } else {
                        match &expected[3] {
                            "long" => Action::EnterLong,
                            "short" => Action::EnterShort,
                            _ => panic!("unexpected reference side"),
                        }
                    }
                );
                assert_eq!(request.quantity_lots, 1);
                assert!(!replay.complete_flat());
                found = true;
                intents += 1;
                if !synthetic_close {
                    break; // No quote or subsequent state/exit parity claim.
                }
                let expected_price: f64 = expected[if exiting { 9 } else { 8 }].parse().unwrap();
                assert_eq!(close_ticks / 2.0, expected_price, "conditional fill price");
                let fill = replay
                    .quote(
                        feedback::Quote {
                            instrument: "IMOEXF".into(),
                            source_id: "SYNTHETIC_CLOSE_NOT_OBSERVED".into(),
                            price_ticks: close_ticks as i64,
                            // Artificial test ordering, not historical receipt latency.
                            event_at_ms: now + 1,
                            received_at_ms: now + 2,
                        },
                        now + 3,
                    )
                    .unwrap()
                    .unwrap();
                assert_eq!(fill.position_lots == 0, exiting);
            }
        }
        assert!(found, "missing first entry {}", dates[index]);
        if synthetic_close {
            assert_eq!(processed, 101);
            assert_eq!(intents, 2, "missing exit {}", dates[index]);
            assert!(replay.complete_flat());
        }
        replay.finish().unwrap();
        let evidence = std::fs::read_to_string(dir.0.join("out.jsonl")).unwrap();
        if synthetic_close {
            assert!(evidence.contains("SYNTHETIC_CLOSE_NOT_OBSERVED"));
            assert!(evidence.contains("replay_end_flat"));
            assert!(!evidence.contains("LIVE_OBSERVED"));
        } else {
            assert!(evidence.contains("INCOMPLETE_virtual_pending"));
            assert!(!evidence.contains("hypothetical_feedback"));
        }
    }
}

#[test]
fn binds_config_run_segment_and_exact_seed() {
    let f = fixture();
    let receipt = check(&f).unwrap();
    assert_eq!(receipt.input_count, 12);
    assert_eq!(receipt.run_id, "test");
    assert_eq!(receipt.segment_id, "s1");
    assert_eq!(receipt.seed_sha256, f.config.seed_sha256);
    assert_eq!(
        receipt.config_sha256,
        ValidatedConfig::validate(f.config).unwrap().fingerprint()
    );
}

#[test]
fn removal_of_interior_bar_cannot_hide_behind_recomputed_seed_hash() {
    let mut f = fixture();
    f.bars.remove(1);
    f.config.seed_sha256 = digest(&serde_json::to_vec(&f.bars).unwrap());
    assert_eq!(check(&f).unwrap_err(), SeedError::Coverage);
}

#[test]
fn changed_profile_feedback_reference_or_mapping_refuses() {
    let mut f = fixture();
    f.config.model_sha256 = "0".repeat(64);
    assert_eq!(check(&f).unwrap_err(), SeedError::Identity);
    let mut f = fixture();
    f.config.feedback_policy_sha256 = "0".repeat(64);
    assert_eq!(check(&f).unwrap_err(), SeedError::Identity);
    let mut f = fixture();
    f.reference.push(b' ');
    assert_eq!(check(&f).unwrap_err(), SeedError::Identity);
    let mut f = fixture();
    f.mapping.push(b' ');
    assert_eq!(check(&f).unwrap_err(), SeedError::Identity);
}

#[test]
fn out_of_order_nonfinal_and_wrong_range_refuse() {
    let mut f = fixture();
    f.bars.swap(1, 2);
    f.config.seed_sha256 = digest(&serde_json::to_vec(&f.bars).unwrap());
    assert_eq!(check(&f).unwrap_err(), SeedError::Coverage);
    let mut f = fixture();
    f.bars[1].is_final = false;
    f.config.seed_sha256 = digest(&serde_json::to_vec(&f.bars).unwrap());
    assert_eq!(check(&f).unwrap_err(), SeedError::Bar);
    let mut f = fixture();
    f.config.warmup_range_start_utc_ms -= 600_000;
    assert_eq!(check(&f).unwrap_err(), SeedError::Coverage);
}

#[test]
fn bounded_payload_and_cache() {
    let mut f = fixture();
    f.config.max_input_bytes = 10;
    assert_eq!(check(&f).unwrap_err(), SeedError::Bound);
    let mut f = fixture();
    f.config.max_cache_bars = 11;
    assert_eq!(check(&f).unwrap_err(), SeedError::Bound);
}

fn segment(
    f: &Fixture,
) -> Result<finam_signal_shadow::segment::SeededSegment, finam_signal_shadow::segment::SegmentError>
{
    finam_signal_shadow::segment::SeededSegment::from_inputs(
        ValidatedConfig::validate(f.config.clone()).unwrap(),
        &f.reference,
        &f.mapping,
        &f.calendar,
        &serde_json::to_vec(&f.bars).unwrap(),
    )
}

#[test]
fn independent_real_headers_bind_every_non_attribution_parameter() {
    use finam_signal_shadow::observer::{compare_report_steps, Comparison, Decision, Step};
    use finam_signal_shadow::segment::ObservationMode;
    let f = fixture();
    let left = segment(&f).unwrap().seed_observation();
    let step = Step {
        mode: ObservationMode::LiveObserved,
        instrument: f.config.instrument.clone(),
        model_label_utc_ms: f.config.observation_start_utc_ms,
        nominal_close_utc_ms: f.config.observation_start_utc_ms + 600_000,
        provider_at_ms: None,
        received_at_ms: f.config.observation_start_utc_ms + 600_000,
        canonical_ready_at_ms: f.config.observation_start_utc_ms + 600_000,
        model_start_at_ms: f.config.observation_start_utc_ms + 600_000,
        result_at_ms: f.config.observation_start_utc_ms + 600_000,
        canonical_input_sha256: "a".repeat(64),
        raw_provenance_sha256: "b".repeat(64),
        before: left.state.clone(),
        after: left.state.clone(),
        decision: Decision {
            action: None,
            quantity_lots: 0,
            reason: "no_signal".into(),
        },
    };
    let mut independent = fixture();
    independent.config.run_id = "reference-run".into();
    independent.config.segment_id = "reference-segment".into();
    independent.config.previous_segment_id = Some("reference-previous".into());
    let right = segment(&independent).unwrap().seed_observation();
    assert_ne!(left.config_sha256, right.config_sha256);
    assert_eq!(left.semantic_config_sha256, right.semantic_config_sha256);
    assert_eq!(
        compare_report_steps(&left, &step, &right, &step),
        Comparison::SameInputMatch
    );

    // All of these headers go through production validation/binding/model warmup.
    // Exact-byte mapping/calendar changes are meaningful pinned identities even
    // when the decoded inventories are unchanged.
    for case in ["seed", "reference", "mapping", "calendar", "bound"] {
        let mut changed = fixture();
        match case {
            "seed" => {
                changed.bars[0].volume += 1.0;
                changed.config.seed_sha256 = digest(&serde_json::to_vec(&changed.bars).unwrap());
            }
            "reference" => {
                changed.reference.push(b' ');
                changed.config.reference_sha256 = digest(&changed.reference);
            }
            "mapping" => {
                changed.mapping.push(b' ');
                changed.config.instrument_mapping_sha256 = digest(&changed.mapping);
            }
            "calendar" => {
                changed.calendar.push(b' ');
                changed.config.calendar_sha256 = digest(&changed.calendar);
            }
            _ => changed.config.lmax_ms += 1,
        }
        let right = segment(&changed).unwrap().seed_observation();
        assert_ne!(
            left.semantic_config_sha256, right.semantic_config_sha256,
            "{case}"
        );
        assert_eq!(
            compare_report_steps(&left, &step, &right, &step),
            Comparison::NotComparable,
            "{case}"
        );
    }

    // Unsupported model/policy identities cannot produce a real header at all.
    for model in [true, false] {
        let mut changed = fixture();
        if model {
            changed.config.model_sha256 = "0".repeat(64);
        } else {
            changed.config.feedback_policy_sha256 = "0".repeat(64);
        }
        assert!(matches!(
            segment(&changed),
            Err(finam_signal_shadow::segment::SegmentError::Binding(
                SeedError::Identity
            ))
        ));
        // Tampering with either explicit identity in an otherwise real header
        // also fails comparison, independently of the semantic config binding.
        let mut right = segment(&f).unwrap().seed_observation();
        if model {
            right.model_sha256 = "0".repeat(64);
        } else {
            right.feedback_policy_sha256 = "0".repeat(64);
        }
        assert_eq!(
            compare_report_steps(&left, &step, &right, &step),
            Comparison::NotComparable
        );
    }
}

#[test]
fn bound_model_seed_observation_is_warmup_not_live() {
    let mut f = fixture();
    f.config.previous_segment_id = Some("previous".into());
    let segment = segment(&f).unwrap();
    let observation = segment.seed_observation();
    assert_eq!(
        observation.mode,
        finam_signal_shadow::segment::ObservationMode::Warmup
    );
    assert_eq!(observation.previous_segment_id.as_deref(), Some("previous"));
    assert_eq!(observation.live_observation_count, 0);
    assert_eq!(observation.virtual_position_lots, 0);
    assert_eq!(observation.state["entry_ready"], true);
    assert_eq!(
        observation.config_sha256,
        ValidatedConfig::validate(f.config).unwrap().fingerprint()
    );
    assert_eq!(
        observation.last_history_label_utc,
        f.bars.last().unwrap().close_time_utc
    );
    assert_eq!(
        serde_json::to_vec(&observation).unwrap(),
        serde_json::to_vec(&segment.seed_observation()).unwrap()
    );
}

#[test]
fn bound_model_cannot_skip_seed_check_or_cold_flat_mid_session() {
    use finam_signal_shadow::segment::SegmentError;
    let mut f = fixture();
    f.bars.remove(1);
    assert!(matches!(
        segment(&f),
        Err(SegmentError::Binding(SeedError::Identity))
    ));
    f.config.seed_sha256 = digest(&serde_json::to_vec(&f.bars).unwrap());
    assert!(matches!(
        segment(&f),
        Err(SegmentError::Binding(SeedError::Coverage))
    ));
    let mut f = fixture();
    f.config.observation_start_utc_ms += 600_000;
    assert!(matches!(segment(&f), Err(SegmentError::UnsupportedStart)));
}

#[test]
fn bound_model_rejects_quote_age_overflow() {
    let mut f = fixture();
    f.config.max_quote_age_ms = u64::MAX;
    assert!(matches!(
        segment(&f),
        Err(finam_signal_shadow::segment::SegmentError::Bounds)
    ));
}

struct ReplayDir(std::path::PathBuf);
impl ReplayDir {
    fn new() -> Self {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "moex-bound-replay-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for ReplayDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn replay_bar(f: &Fixture) -> HybridRuntimeBarEvent {
    let mut bar = f.bars.last().unwrap().clone();
    bar.close_time_utc = f.config.observation_start_utc_ms / 1000;
    bar.origin = HybridRuntimeBarOrigin::Replay;
    bar
}

fn candle_fixture() -> Fixture {
    let mut f = fixture();
    f.config.feedback_policy_sha256 = feedback::candle_policy_fingerprint();
    f.config.run_id = "candle-only-test".into();
    f.config.segment_id = "candle-new".into();
    f.config.run_deadline_utc_ms = f.config.observation_start_utc_ms + 86_400_000;
    f
}

fn candle_bar(f: &Fixture, minutes: i64, close: f64) -> HybridRuntimeBarEvent {
    let mut bar = replay_bar(f);
    bar.close_time_utc += minutes * 60;
    bar.open = close;
    bar.close = close;
    bar.high = close + 0.5;
    bar.low = close - 0.5;
    bar
}

fn candle_step(
    replay: &mut finam_signal_shadow::replay::OfflineReplay,
    bar: HybridRuntimeBarEvent,
) -> Option<feedback::Request> {
    let time = (bar.close_time_utc + 600) * 1000 + 17;
    replay.step(bar, &"a".repeat(64), time).unwrap()
}

#[test]
fn candle_digest_explicit_selection_and_quote_profile_not_comparable() {
    let quote = fixture();
    let candle = candle_fixture();
    assert!(!ValidatedConfig::validate(quote.config.clone())
        .unwrap()
        .candle_only());
    assert!(ValidatedConfig::validate(candle.config.clone())
        .unwrap()
        .candle_only());
    let q = segment(&quote).unwrap().seed_observation();
    let c = segment(&candle).unwrap().seed_observation();
    assert_eq!(q.model_sha256, c.model_sha256);
    assert_ne!(q.semantic_config_sha256, c.semantic_config_sha256);
    assert!(!finam_signal_shadow::observer::report_identities_compatible(&q, &c));
    let mut bad = candle_fixture();
    bad.config.feedback_policy_sha256 = "0".repeat(64);
    assert!(segment(&bad).is_err());
}

#[test]
fn candle_long_short_stop2_feedback_before_next_bar_no_quotes_or_reversal() {
    use finam_signal_shadow::{feedback::Action, replay::OfflineReplay};
    for (entry, exit, action, position) in [
        (2202.0, 2199.0, Action::EnterLong, 1),
        (2199.0, 2202.0, Action::EnterShort, -1),
    ] {
        let f = candle_fixture();
        let dir = ReplayDir::new();
        let path = dir.0.join("candle.jsonl");
        let mut r = OfflineReplay::create(segment(&f).unwrap(), &path).unwrap();
        assert!(candle_step(&mut r, candle_bar(&f, 0, 2200.5)).is_none());
        assert_eq!(
            candle_step(&mut r, candle_bar(&f, 180, entry))
                .unwrap()
                .action,
            action
        );
        assert_eq!(
            r.observation()["last_position_qty"].as_f64(),
            Some(position as f64)
        );
        assert!(r.observation()["pending_entry_request_id"].is_null());
        let bytes = std::fs::read(&path).unwrap();
        assert!(candle_step(&mut r, candle_bar(&f, 180, entry)).is_none());
        assert_eq!(
            bytes,
            std::fs::read(&path).unwrap(),
            "duplicate must not fill again"
        );
        assert_eq!(
            candle_step(&mut r, candle_bar(&f, 230, exit))
                .unwrap()
                .action,
            Action::Exit
        );
        assert!(r.complete_flat());
        assert!(r.observation()["pending_exit_request_id"].is_null());
        r.finish().unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(!text.contains("\"quote\":"));
        assert!(!text.contains("LIVE_OBSERVED"));
        let rows: Vec<serde_json::Value> = text
            .lines()
            .map(|s| serde_json::from_str(s).unwrap())
            .collect();
        let fills: Vec<_> = rows
            .iter()
            .enumerate()
            .filter(|(_, r)| r["kind"] == "hypothetical_candle_feedback")
            .collect();
        assert_eq!(fills.len(), 2);
        for (i, row) in fills {
            assert_eq!(rows[i - 1]["kind"], "model_step");
            let fill = &row["event"]["fill"];
            assert_eq!(fill["policy"], feedback::CANDLE_POLICY);
            assert!(
                fill["applied_at_ms"].as_i64().unwrap()
                    >= fill["request"]["decision_at_ms"].as_i64().unwrap()
            );
            assert_eq!(
                fill["raw_provenance_sha256"],
                rows[i - 1]["event"]["raw_provenance_sha256"]
            );
            assert_eq!(
                fill["canonical_input_sha256"],
                rows[i - 1]["event"]["canonical_input_sha256"]
            );
        }
        assert_eq!(
            rows.iter()
                .find(|r| r["event"]["decision"]["action"] == "Exit")
                .unwrap()["event"]["decision"]["reason"],
            if position == 1 {
                "breakout_stop2_long"
            } else {
                "breakout_stop2_short"
            }
        );
    }
}

#[test]
fn candle_stop1_only_minute50_and_eod_no_reentry() {
    use finam_signal_shadow::replay::OfflineReplay;
    for side in [1, -1] {
        let f = candle_fixture();
        for eod in [false, true] {
            let dir = ReplayDir::new();
            let path = dir.0.join("out.jsonl");
            let mut r = OfflineReplay::create(segment(&f).unwrap(), &path).unwrap();
            candle_step(&mut r, candle_bar(&f, 0, 2200.5));
            candle_step(
                &mut r,
                candle_bar(&f, 180, if side == 1 { 2202.0 } else { 2199.0 }),
            );
            if !eod {
                assert!(candle_step(&mut r, candle_bar(&f, 220, 2200.5)).is_none());
            }
            let exit_minute = if eod { 990 } else { 230 };
            let q = candle_step(&mut r, candle_bar(&f, exit_minute, 2200.5)).unwrap();
            assert_eq!(q.action, feedback::Action::Exit);
            assert!(r.complete_flat());
            if eod {
                assert!(candle_step(
                    &mut r,
                    candle_bar(&f, 1000, if side == 1 { 2198.0 } else { 2203.0 })
                )
                .is_none());
                assert_eq!(
                    r.observation()["no_intent_trace"]["reason"],
                    "eod_entry_cutoff"
                );
            }
            r.finish().unwrap();
            let text = std::fs::read_to_string(path).unwrap();
            assert!(text.contains(if eod {
                "breakout_eod_exit"
            } else if side == 1 {
                "breakout_stop1_long"
            } else {
                "breakout_stop1_short"
            }));
        }
    }
}

#[test]
fn candle_bad_tick_or_incomplete_bar_fails_without_fabricated_fill() {
    use finam_signal_shadow::replay::OfflineReplay;
    for invalid in ["tick", "unclosed", "future", "provenance"] {
        let f = candle_fixture();
        let dir = ReplayDir::new();
        let path = dir.0.join("out.jsonl");
        let mut r = OfflineReplay::create(segment(&f).unwrap(), &path).unwrap();
        candle_step(&mut r, candle_bar(&f, 0, 2200.5));
        let mut bar = candle_bar(&f, 180, if invalid == "tick" { 2202.25 } else { 2202.0 });
        bar.is_final = invalid != "unclosed";
        let now = (bar.close_time_utc + 600) * 1000;
        let raw_hash = "a".repeat(64);
        assert!(r
            .step(
                bar,
                if invalid == "provenance" {
                    "bad"
                } else {
                    &raw_hash
                },
                if invalid == "future" { now - 1 } else { now }
            )
            .is_err());
        assert!(r.closed());
        assert!(!std::fs::read_to_string(path)
            .unwrap()
            .contains("hypothetical_candle_feedback"));
    }
}

#[test]
fn candle_rejects_quote_events_even_flat_and_preserves_old_pending_contract() {
    use finam_signal_shadow::replay::{OfflineReplay, ReplayError};
    let f = candle_fixture();
    let dir = ReplayDir::new();
    let mut r = OfflineReplay::create(segment(&f).unwrap(), &dir.0.join("candle.jsonl")).unwrap();
    assert_eq!(
        r.quote(
            feedback::Quote {
                instrument: "IMOEXF".into(),
                source_id: "not-used".into(),
                price_ticks: 4400,
                event_at_ms: 1,
                received_at_ms: 1,
            },
            1
        ),
        Err(ReplayError::Input)
    );
    let mut old = fixture();
    old.config.run_deadline_utc_ms = old.config.observation_start_utc_ms + 86_400_000;
    let mut q = OfflineReplay::create(segment(&old).unwrap(), &dir.0.join("quote.jsonl")).unwrap();
    candle_step(&mut q, candle_bar(&old, 0, 2200.5));
    candle_step(&mut q, candle_bar(&old, 180, 2202.0));
    assert_eq!(q.observation()["last_position_qty"].as_f64(), Some(0.0));
    assert!(!q.observation()["pending_entry_request_id"].is_null());
    let next = candle_bar(&old, 190, 2202.0);
    assert!(q
        .step(
            next.clone(),
            &"a".repeat(64),
            (next.close_time_utc + 600) * 1000
        )
        .is_err());
}

#[test]
fn candle_eof_nonflat_not_completion_and_new_segment_not_resume() {
    use finam_signal_shadow::replay::OfflineReplay;
    let f = candle_fixture();
    let dir = ReplayDir::new();
    let path = dir.0.join("out.jsonl");
    let mut r = OfflineReplay::create(segment(&f).unwrap(), &path).unwrap();
    candle_step(&mut r, candle_bar(&f, 0, 2200.5));
    candle_step(&mut r, candle_bar(&f, 180, 2202.0));
    assert!(!r.complete_flat());
    r.finish().unwrap();
    let old_bytes = std::fs::read(&path).unwrap();
    assert!(String::from_utf8_lossy(&old_bytes).contains("INCOMPLETE_virtual_nonflat"));
    assert!(OfflineReplay::create(segment(&f).unwrap(), &path).is_err());
    let mut new = candle_fixture();
    new.config.segment_id = "new-independent-seed".into();
    new.config.previous_segment_id = Some(f.config.segment_id.clone());
    let header = segment(&new).unwrap().seed_observation();
    assert_eq!(header.virtual_position_lots, 0);
    assert_eq!(header.live_observation_count, 0);
    assert_eq!(old_bytes, std::fs::read(path).unwrap());
}

#[test]
fn candle_cli_executes_without_quote_events_and_names_simulation() {
    let f = candle_fixture();
    let dir = ReplayDir::new();
    let events: Vec<_> = [(0, 2200.5), (180, 2202.0), (190, 2199.0)]
        .into_iter()
        .map(|(m, p)| {
            let bar = candle_bar(&f, m, p);
            serde_json::json!({"kind":"bar","virtual_result_at_ms":(bar.close_time_utc+600)*1000,
            "bar":bar,"raw_provenance_sha256":"a".repeat(64)})
        })
        .collect();
    let result = cli_fixture(&dir, &f, serde_json::json!(events))
        .output()
        .unwrap();
    assert!(result.status.success(), "{result:?}");
    let text = std::fs::read_to_string(dir.0.join("out.jsonl")).unwrap();
    assert_eq!(
        text.matches("\"kind\":\"hypothetical_candle_feedback\"")
            .count(),
        2
    );
    assert!(text.contains("accepted_closed_m10_close"));
    assert!(!text.contains("LIVE_OBSERVED"));
}

#[test]
fn unified_replay_records_model_step_once_without_live_rows() {
    use finam_signal_shadow::replay::{OfflineReplay, ReplayError};
    let f = fixture();
    let dir = ReplayDir::new();
    let path = dir.0.join("out.jsonl");
    let mut replay = OfflineReplay::create(segment(&f).unwrap(), &path).unwrap();
    let bar = replay_bar(&f);
    let now = (bar.close_time_utc + 600) * 1000;
    replay.step(bar.clone(), &"a".repeat(64), now).unwrap();
    let first = std::fs::read(&path).unwrap();
    assert!(replay
        .step(bar.clone(), &"a".repeat(64), now)
        .unwrap()
        .is_none());
    assert_eq!(replay.steps(), 1);
    assert_eq!(first, std::fs::read(&path).unwrap());
    replay.finish().unwrap();
    let state = replay.observation();
    assert_eq!(
        replay.step(bar, &"a".repeat(64), now),
        Err(ReplayError::Closed)
    );
    assert_eq!(state, replay.observation());
    let text = std::fs::read_to_string(&path).unwrap();
    let rows: Vec<serde_json::Value> = text
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[1]["event"]["mode"], "BACKFILL_REPLAY");
    assert!(!text.contains("LIVE_OBSERVED"));
}

#[test]
fn evidence_budget_failure_is_terminal_no_followup_model_work() {
    use finam_signal_shadow::replay::{OfflineReplay, ReplayError};
    let mut f = fixture();
    let header = segment(&f).unwrap().seed_observation();
    let bytes = serde_json::to_vec(&serde_json::json!({
        "sequence":0,"kind":"seed","observation":header
    }))
    .unwrap();
    f.config.max_output_bytes = bytes.len() as u64 + 1;
    let dir = ReplayDir::new();
    let path = dir.0.join("out.jsonl");
    let mut replay = OfflineReplay::create(segment(&f).unwrap(), &path).unwrap();
    let before = std::fs::read(&path).unwrap();
    let mut bar = replay_bar(&f);
    let now = (bar.close_time_utc + 600) * 1000;
    assert_eq!(
        replay.step(bar.clone(), &"a".repeat(64), now),
        Err(ReplayError::Evidence)
    );
    assert!(replay.closed());
    assert_eq!(replay.steps(), 0);
    let state = replay.observation();
    bar.close_time_utc += 600;
    assert_eq!(
        replay.step(bar, &"a".repeat(64), now + 600_000),
        Err(ReplayError::Closed)
    );
    assert_eq!(state, replay.observation());
    assert_eq!(before, std::fs::read(&path).unwrap());
}

#[test]
fn replay_refuses_live_origin_and_outside_run_window_before_callback() {
    use finam_signal_shadow::replay::{OfflineReplay, ReplayError};
    for live in [true, false] {
        let f = fixture();
        let dir = ReplayDir::new();
        let path = dir.0.join("out.jsonl");
        let mut replay = OfflineReplay::create(segment(&f).unwrap(), &path).unwrap();
        let before = replay.observation();
        let mut bar = replay_bar(&f);
        let now = if live {
            bar.origin = HybridRuntimeBarOrigin::Live;
            (bar.close_time_utc + 600) * 1000
        } else {
            f.config.run_deadline_utc_ms
        };
        assert_eq!(
            replay.step(bar, &"a".repeat(64), now),
            Err(ReplayError::Input)
        );
        assert_eq!(before, replay.observation());
    }
}

#[test]
fn unified_replay_actual_entry_feedback_and_eod_exit() {
    use finam_signal_shadow::{
        feedback::{Action, Quote},
        replay::OfflineReplay,
    };
    let mut f = fixture();
    f.config.run_deadline_utc_ms = f.config.observation_start_utc_ms + 24 * 3_600_000;
    let dir = ReplayDir::new();
    let path = dir.0.join("out.jsonl");
    let mut replay = OfflineReplay::create(segment(&f).unwrap(), &path).unwrap();
    let mut bar = replay_bar(&f);
    let now = (bar.close_time_utc + 600) * 1000;
    assert!(replay
        .step(bar.clone(), &"a".repeat(64), now)
        .unwrap()
        .is_none());
    bar.close_time_utc += 3 * 3600;
    bar.high = 2210.0;
    bar.close = 2205.0;
    let now = (bar.close_time_utc + 600) * 1000;
    let entry = replay
        .step(bar.clone(), &"b".repeat(64), now)
        .unwrap()
        .expect("model entry");
    assert_eq!(entry.action, Action::EnterLong);
    replay
        .quote(
            Quote {
                instrument: "IMOEXF".into(),
                source_id: "entry-quote".into(),
                price_ticks: 4410,
                event_at_ms: now + 1,
                received_at_ms: now + 2,
            },
            now + 3,
        )
        .unwrap()
        .unwrap();
    assert_eq!(replay.observation()["current_owner"], "intraday_breakout");
    bar.close_time_utc = f.config.observation_start_utc_ms / 1000 + 16 * 3600 + 30 * 60;
    let now = (bar.close_time_utc + 600) * 1000;
    let exit = replay
        .step(bar, &"c".repeat(64), now)
        .unwrap()
        .expect("model EOD exit");
    assert_eq!(exit.action, Action::Exit);
    replay
        .quote(
            Quote {
                instrument: "IMOEXF".into(),
                source_id: "exit-quote".into(),
                price_ticks: 4410,
                event_at_ms: now + 1,
                received_at_ms: now + 2,
            },
            now + 3,
        )
        .unwrap()
        .unwrap();
    assert!(replay.observation()["current_owner"].is_null());
    replay.finish().unwrap();
    let rows: Vec<serde_json::Value> = std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(
        rows.iter()
            .filter(|r| r["kind"] == "hypothetical_feedback")
            .count(),
        2
    );
    assert_eq!(rows.last().unwrap()["event"]["reason"], "replay_end_flat");
}
