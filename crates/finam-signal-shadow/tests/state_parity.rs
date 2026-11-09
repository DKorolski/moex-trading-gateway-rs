//! Independent frozen Python state/decision oracle, not a second FINAM run.
//! Pending callback assertions below are protocol checks, NOT a Python pending
//! oracle. No-intent diagnostics come from independently traced Python branches.
use broker_core::*;
use chrono::{Datelike, Timelike};
use finam_signal_shadow::{
    feedback::{Action, Quote},
    model::ShadowModel,
};
use serde_json::Value;
use sha2::{Digest, Sha256};

const INPUT: &[u8] =
    include_bytes!("../../../fixtures/stage8b-p1f-parity/imoexf_raw_10m_msk_utc.csv");
const REFERENCE: &str =
    include_str!("../../../fixtures/signal-shadow-state-parity/python-steps.json");

// Independent archived ALOR source, not imports from strategy-runtime-core.
#[allow(dead_code)]
#[path = "../../../fixtures/signal-shadow-state-parity/oracle/rust/intraday_breakout.rs"]
mod intraday_breakout;
#[allow(dead_code)]
#[path = "../../../fixtures/signal-shadow-state-parity/oracle/rust/mean_reversion.rs"]
mod mean_reversion;
#[allow(dead_code)]
#[path = "../../../fixtures/signal-shadow-state-parity/oracle/rust/orchestrator.rs"]
mod orchestrator;
#[allow(dead_code)]
#[path = "../../../fixtures/signal-shadow-state-parity/oracle/rust/types.rs"]
mod types;

fn frozen_model() -> orchestrator::HybridOrchestrator {
    orchestrator::HybridOrchestrator::new(
        mean_reversion::MeanReversionEngine::new(mean_reversion::MeanReversionConfig::default()),
        intraday_breakout::IntradayBreakoutEngine::new(intraday_breakout::IntradayBreakoutConfig {
            k: 0.53,
            stop1_range: 0.51,
            stop2_range: 0.35,
            big_move_threshold: 0.025,
            min_range: 1.01,
            min_range_mode: intraday_breakout::MinRangeMode::Absolute,
            exclude_weekends: true,
            wait_hours: 3.0,
        }),
        orchestrator::HybridOrchestratorConfig::default(),
    )
}

fn frozen_bar(bar: &HybridRuntimeBarEvent, position: i64) -> orchestrator::BarInput {
    orchestrator::BarInput {
        dt: (chrono::DateTime::from_timestamp(bar.close_time_utc, 0).unwrap()
            + chrono::Duration::hours(3))
        .naive_utc(),
        open: bar.open,
        high: bar.high,
        low: bar.low,
        close: bar.close,
        // MR is disabled before evaluation; these MR-only inputs are unused.
        close_prev: 0.0,
        day_range_prev: 0.0,
        has_open_position: position != 0,
        has_live_orders: false,
    }
}

fn compare_frozen(frozen: &orchestrator::HybridOrchestrator, actual: &Value, label: &str) {
    let s = frozen.snapshot();
    let expected = serde_json::json!({ "state":format!("{:?}",s.state),
        "owner":s.current_owner,"side":s.current_side,"pending_entry":s.has_pending_entry });
    assert_eq!(
        expected, actual["orchestrator"],
        "{label}: archived ALOR orchestrator"
    );
    let b = frozen.intraday_breakout.snapshot();
    let expected = serde_json::json!({
        "prev_day_close":b.yesterday_close,"prev_day_range":b.yesterday_range,
        "prev_day_return":b.yesterday_return,"day_before_close":b.day_before_close,
        "current_day_high":b.cur_day_high,"current_day_low":b.cur_day_low,
        "current_day_close":b.cur_day_close,"was_long_today":b.was_long_today,
        "was_short_today":b.was_short_today,
    });
    compare(&expected, actual, label).unwrap();
}

fn instrument() -> InstrumentId {
    InstrumentId {
        symbol: "IMOEXF".into(),
        venue_symbol: Some("IMOEXF@RTSX".into()),
        exchange: Exchange::Moex,
        market: Market::Futures,
    }
}

fn bars() -> Vec<HybridRuntimeBarEvent> {
    csv::Reader::from_reader(INPUT)
        .records()
        .map(|r| {
            let r = r.unwrap();
            let n = |i| r.get(i).unwrap().parse::<f64>().unwrap();
            HybridRuntimeBarEvent {
                instrument: instrument(),
                close_time_utc: chrono::DateTime::parse_from_str(&r[1], "%Y-%m-%d %H:%M:%S%:z")
                    .unwrap()
                    .timestamp(),
                open: n(3),
                high: n(4),
                low: n(5),
                close: n(6),
                volume: n(7),
                origin: HybridRuntimeBarOrigin::Live,
                is_final: true,
                timeframe_sec: 600,
            }
        })
        .collect()
}

fn context() -> HybridRuntimeStrategyContext {
    let now = chrono::DateTime::parse_from_rfc3339("2026-09-21T04:00:00Z")
        .unwrap()
        .timestamp();
    HybridRuntimeStrategyContext {
        strategy_id: "s2-independent-state".into(),
        request_namespace_account: BrokerAccountId("virtual-only".into()),
        instrument: instrument(),
        tick_size: 0.5,
        trade_mode: HybridRuntimeTradeMode::Paper,
        paper_execution_mode: HybridRuntimePaperExecutionMode::LiveOnly,
        allow_live_orders: false,
        gateway_phase: HybridRuntimeGatewayPhase::LiveReady,
        position_qty: Some(0.0),
        event_ts_utc: now,
        strategy_now_ts_utc: now,
        last_bar_ts_utc: None,
    }
}

fn compare(expected: &Value, actual: &Value, path: &str) -> Result<(), String> {
    match expected {
        Value::Object(fields) => {
            for (key, value) in fields {
                compare(value, &actual[key], &format!("{path}.{key}"))?;
            }
        }
        Value::Number(n) => {
            let e = n.as_f64().unwrap();
            let a = actual
                .as_f64()
                .ok_or_else(|| format!("{path}: missing number"))?;
            if !a.is_finite() || (a - e).abs() > 1e-10 {
                return Err(format!("{path}: independent={e} actual={a}"));
            }
        }
        _ if expected != actual => {
            return Err(format!("{path}: independent={expected} actual={actual}"))
        }
        _ => {}
    }
    Ok(())
}

#[test]
fn candle_only_five_sessions_match_frozen_python_without_synthetic_quotes() {
    use finam_signal_shadow::{
        feedback, replay::OfflineReplay, seed::*, segment::SeededSegment, ValidatedConfig,
    };
    let all = bars();
    let dates = ["2026-09-15", "2026-09-16", "2026-09-17", "2026-09-18"];
    let local = |b: &HybridRuntimeBarEvent| {
        chrono::DateTime::from_timestamp(b.close_time_utc, 0).unwrap() + chrono::Duration::hours(3)
    };
    let history: Vec<_> = all
        .iter()
        .filter(|b| {
            dates.contains(&local(b).date_naive().to_string().as_str()) && local(b).hour() >= 7
        })
        .cloned()
        .map(|mut b| {
            b.origin = HybridRuntimeBarOrigin::History;
            b
        })
        .collect();
    assert_eq!(history.len(), 404);
    let seed = serde_json::to_vec(&history).unwrap();
    let mapping = serde_json::to_vec(&InstrumentMapping {
        instrument: instrument(),
        tick_size: "0.5".into(),
        lot_size: 1,
    })
    .unwrap();
    let calendar = serde_json::to_vec(&CalendarInventory {
        sessions: dates.map(|d| SessionInventory {
            date: d.into(),
            required_m10_labels_utc: history
                .iter()
                .filter(|b| local(b).date_naive().to_string() == d)
                .map(|b| b.close_time_utc)
                .collect(),
        }),
    })
    .unwrap();
    let start = chrono::DateTime::parse_from_rfc3339("2026-09-21T07:00:00+03:00")
        .unwrap()
        .timestamp_millis();
    let model = ShadowModel::baseline07(context(), 1000).unwrap();
    let config=ValidatedConfig::parse(&serde_json::to_vec(&serde_json::json!({
        "schema_version":1,"reference":"alor_freeze_2026_09_27","restart_mode":"new_segment",
        "model_profile":"imoexf-baseline07-bo-only-no-riskgate","model_sha256":model.model_config_fingerprint(),
        "reference_sha256":digest(REFERENCE.as_bytes()),"feedback_policy_sha256":feedback::candle_policy_fingerprint(),
        "instrument_mapping_sha256":digest(&mapping),"calendar_sha256":digest(&calendar),"seed_sha256":digest(&seed),
        "instrument":"IMOEXF@RTSX","run_id":"candle-independent-oracle","segment_id":"new","previous_segment_id":null,
        "warmup_session_dates":dates,"warmup_range_start_utc_ms":history[0].close_time_utc*1000,
        "warmup_range_end_utc_ms":(history.last().unwrap().close_time_utc+600)*1000,
        "observation_start_utc_ms":start,"run_deadline_utc_ms":start+5*86_400_000,
        "lmax_ms":1000,"max_quote_age_ms":1000,"clock_uncertainty_ms":10,
        "ws_silence_budget_ms":1000,"reconnect_budget_ms":1000,"stop_timeout_ms":1000,
        "max_input_bytes":4_000_000,"max_cache_bars":4096,"max_output_bytes":16_000_000,
    })).unwrap()).unwrap();
    let segment =
        SeededSegment::from_inputs(config, REFERENCE.as_bytes(), &mapping, &calendar, &seed)
            .unwrap();
    let path = std::env::temp_dir().join(format!("candle-oracle-{}.jsonl", std::process::id()));
    let mut replay = OfflineReplay::create(segment, &path).unwrap();
    let reference: Value = serde_json::from_str(REFERENCE).unwrap();
    compare(&reference["seed_after"], &replay.observation(), "seed").unwrap();
    let selected: Vec<_> = all
        .into_iter()
        .filter(|b| {
            let t = local(b);
            t.year() == 2026 && t.month() == 9 && (21..=25).contains(&t.day()) && t.hour() >= 7
        })
        .collect();
    assert_eq!(selected.len(), 505);
    let mut expected_fills = 0;
    for (mut b, expected) in selected
        .into_iter()
        .zip(reference["steps"].as_array().unwrap())
    {
        let label = local(&b).format("%Y-%m-%dT%H:%M:%S").to_string();
        b.origin = HybridRuntimeBarOrigin::Replay;
        let now = (b.close_time_utc + 600) * 1000 + 17;
        let request = replay.step(b, &digest(INPUT), now).unwrap();
        let action = match request.as_ref().map(|r| r.action) {
            Some(Action::EnterLong) => "enter_long",
            Some(Action::EnterShort) => "enter_short",
            Some(Action::Exit) => "exit",
            None => "none",
        };
        assert_eq!(action, expected["action"], "{label}");
        expected_fills += usize::from(request.is_some());
        compare(&expected["after_feedback"], &replay.observation(), &label).unwrap();
    }
    assert!(replay.complete_flat());
    replay.finish().unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    let rows: Vec<Value> = text
        .lines()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect();
    assert_eq!(
        rows.iter().filter(|r| r["kind"] == "model_step").count(),
        505
    );
    let fills: Vec<_> = rows
        .iter()
        .filter(|r| r["kind"] == "hypothetical_candle_feedback")
        .collect();
    assert_eq!(fills.len(), expected_fills);
    assert_eq!(expected_fills, 10);
    assert!(!text.contains("\"quote\":"));
    assert!(!text.contains("LIVE_OBSERVED"));
    // Compare immediate pre-feedback decisions/reasons too, not just final positions.
    for (row, expected) in rows
        .iter()
        .filter(|r| r["kind"] == "model_step")
        .zip(reference["steps"].as_array().unwrap())
    {
        let reason = if expected["action"] == "none" {
            &expected["no_intent_trace"]["reason"]
        } else {
            &expected["reason"]
        };
        assert_eq!(&row["event"]["decision"]["reason"], reason);
    }
    std::fs::remove_file(path).unwrap();
}

#[test]
fn historical_five_sessions_independent_states_and_decisions() {
    let reference: Value = serde_json::from_str(REFERENCE).unwrap();
    assert_eq!(
        format!("{:x}", Sha256::digest(INPUT)),
        reference["source_sha256"]["fixtures/imoexf_raw_10m_msk_utc.csv"]
    );
    let all = bars();
    let sessions = [15, 16, 17, 18].map(|d| chrono::NaiveDate::from_ymd_opt(2026, 9, d).unwrap());
    let local = |bar: &HybridRuntimeBarEvent| {
        chrono::DateTime::from_timestamp(bar.close_time_utc, 0).unwrap()
            + chrono::Duration::hours(3)
    };
    let history: Vec<_> = all
        .iter()
        .filter(|b| sessions.contains(&local(b).date_naive()) && local(b).hour() >= 7)
        .cloned()
        .map(|mut b| {
            b.origin = HybridRuntimeBarOrigin::History;
            b
        })
        .collect();
    assert_eq!(history.len(), 404);
    let hash = format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&history).unwrap())
    );
    let mut model =
        ShadowModel::baseline07_warmed(context(), &history, sessions, &hash, 1000).unwrap();
    let mut frozen = frozen_model();
    let mut frozen_position = 0;
    for bar in &history {
        frozen.warm_bar(frozen_bar(bar, 0));
    }
    compare_frozen(&frozen, &model.observation(), "seed");
    let replay: Vec<_> = all
        .into_iter()
        .filter(|b| {
            let t = local(b);
            t.year() == 2026 && t.month() == 9 && (21..=25).contains(&t.day()) && t.hour() >= 7
        })
        .collect();
    let steps = reference["steps"].as_array().unwrap();
    assert_eq!(steps.len(), 505);
    assert_eq!(replay.len(), steps.len());
    let mut previous = &reference["seed_after"];
    let (mut entries, mut exits, mut none) = (0, 0, 0);
    for (bar, expected) in replay.into_iter().zip(steps) {
        let label = local(&bar).format("%Y-%m-%dT%H:%M:%S").to_string();
        assert_eq!(expected["label"], label);
        compare(
            previous,
            &model.observation(),
            &format!("{label}.before_bar"),
        )
        .unwrap();
        compare_frozen(
            &frozen,
            &model.observation(),
            &format!("{label}.before_bar"),
        );
        assert_eq!(frozen_position, model.position_lots());
        assert!(model.pending().is_none());
        assert_eq!(
            model.position_lots(),
            expected["position_before"].as_i64().unwrap()
        );
        let now = (bar.close_time_utc + 600) * 1000;
        let frozen_actions =
            frozen.on_bar_with_mr_override(frozen_bar(&bar, frozen_position), None, None);
        let request = model.on_bar(bar.clone(), now).unwrap();
        let observation = model.observation();
        compare_frozen(&frozen, &observation, &format!("{label}.after_bar"));
        let action = match request.as_ref().map(|r| r.action) {
            Some(Action::EnterLong) => "enter_long",
            Some(Action::EnterShort) => "enter_short",
            Some(Action::Exit) => "exit",
            None => "none",
        };
        assert_eq!(expected["action"], action, "{label}");
        let decision = finam_signal_shadow::observer::Decision::from_model_observation(
            request.as_ref().map(|r| r.action),
            request.as_ref().map_or(0, |r| r.quantity_lots),
            &observation,
        )
        .unwrap();
        if action == "none" {
            compare(
                &expected["no_intent_trace"],
                &observation["no_intent_trace"],
                &format!("{label}.no_intent_trace"),
            )
            .unwrap();
            assert_eq!(
                observation["no_intent_trace"]["label_utc_ms"],
                bar.close_time_utc * 1000
            );
            assert_eq!(
                decision.reason,
                expected["no_intent_trace"]["reason"].as_str().unwrap()
            );
        } else {
            assert_eq!(decision.reason, expected["reason"].as_str().unwrap());
            assert!(observation["no_intent_trace"].is_null());
        }
        let (frozen_action, frozen_reason) = match frozen_actions.as_slice() {
            [] => ("none", Value::Null),
            [types::Action::SubmitEntry(e)] => (
                if e.side == types::Side::Long {
                    "enter_long"
                } else {
                    "enter_short"
                },
                serde_json::to_value(e.reason).unwrap(),
            ),
            [types::Action::SubmitExit { reason, .. }] => {
                ("exit", serde_json::to_value(reason).unwrap())
            }
            _ => panic!("unexpected frozen actions at {label}: {frozen_actions:?}"),
        };
        assert_eq!(frozen_action, action, "{label}");
        assert_eq!(frozen_reason, expected["reason"], "{label}");
        // All independent price/anchor fields hold before feedback as well.
        let mut after_bar = expected["after_feedback"].clone();
        for key in [
            "current_owner",
            "current_side",
            "last_position_qty",
            "was_long_today",
            "was_short_today",
        ] {
            after_bar.as_object_mut().unwrap().remove(key);
        }
        compare(&after_bar, &observation, &format!("{label}.after_bar")).unwrap();
        assert_eq!(
            observation["last_position_qty"].as_f64().unwrap(),
            model.position_lots() as f64
        );
        assert_eq!(observation["was_long_today"], expected["used_before"][0]);
        assert_eq!(observation["was_short_today"], expected["used_before"][1]);
        if let Some(request) = request {
            assert_eq!(model.pending(), Some(&request));
            assert_eq!(request.quantity_lots, 1);
            let is_exit = request.action == Action::Exit;
            let reason_key = if is_exit {
                "pending_exit_reason"
            } else {
                "pending_entry_reason"
            };
            assert_eq!(observation[reason_key], expected["reason"], "{label}");
            if is_exit {
                exits += 1;
                assert!(!observation["pending_exit_request_id"].is_null());
                assert_eq!(observation["current_owner"], "intraday_breakout");
                assert_eq!(observation["current_side"], previous["current_side"]);
            } else {
                entries += 1;
                assert!(!observation["pending_entry_request_id"].is_null());
                assert_eq!(observation["pending_entry_owner"], "intraday_breakout");
                assert_eq!(
                    observation["pending_entry_side"],
                    expected["after_feedback"]["current_side"]
                );
                assert!(observation["current_owner"].is_null());
            }
            let quote = Quote {
                instrument: "IMOEXF".into(),
                source_id: format!("s2-synthetic-{}", request.sequence),
                price_ticks: (bar.close / 0.5).round() as i64,
                event_at_ms: now + 1,
                received_at_ms: now + 2,
            };
            let fill = model.on_quote(quote, now + 3).unwrap().unwrap();
            assert_eq!(
                fill.position_lots,
                expected["after_feedback"]["last_position_qty"]
                    .as_i64()
                    .unwrap()
            );
            // Apply identical idealized full-fill to the archive's supported
            // callback, independently deriving its book from its own action.
            match frozen_actions.as_slice() {
                [types::Action::SubmitEntry(e)] => {
                    frozen.on_order_filled("entry", e.owner, Some(e.side));
                    frozen_position = if e.side == types::Side::Long { 1 } else { -1 };
                }
                [types::Action::SubmitExit { owner, .. }] => {
                    frozen.on_order_filled("exit", *owner, None);
                    frozen_position = 0;
                }
                _ => unreachable!(),
            }
        } else {
            none += 1;
            assert!(expected["reason"].is_null());
            assert!(expected["no_intent_trace"]["reason"].is_string());
        }
        let after = model.observation();
        compare_frozen(&frozen, &after, &format!("{label}.after_feedback"));
        assert_eq!(frozen_position, model.position_lots());
        for key in [
            "pending_entry_request_id",
            "pending_exit_request_id",
            "pending_entry_owner",
            "pending_entry_side",
            "pending_entry_reason",
            "pending_exit_reason",
        ] {
            assert!(after[key].is_null(), "{label}.{key}");
        }
        assert!(model.pending().is_none());
        compare(
            &expected["after_feedback"],
            &after,
            &format!("{label}.after_feedback"),
        )
        .unwrap();
        assert!(model.on_bar(bar, now).unwrap().is_none());
        assert_eq!(after, model.observation(), "duplicate bar changed state");
        assert!(!model.held());
        previous = &expected["after_feedback"];
    }
    assert_eq!((entries, exits, none), (5, 5, 495));
    assert_eq!(model.position_lots(), 0);
}

#[test]
fn verifier_rejects_state_and_reason_mutations() {
    let reference: Value = serde_json::from_str(REFERENCE).unwrap();
    let state = &reference["steps"][0]["after_feedback"];
    for key in [
        "prev_day_close",
        "prev_day_range",
        "prev_day_return",
        "day_before_close",
        "current_owner",
        "current_side",
        "last_position_qty",
        "was_long_today",
        "was_short_today",
        "bo_levels",
    ] {
        let mut mutated = state.clone();
        mutated[key] = Value::String("mutation".into());
        assert!(compare(state, &mutated, "mutation").is_err(), "{key}");
    }
    let entry = reference["steps"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| !s["reason"].is_null())
        .unwrap();
    assert!(compare(
        &entry["reason"],
        &Value::String("wrong_reason".into()),
        "reason"
    )
    .is_err());
}

#[test]
fn compiled_oracle_sources_are_pinned() {
    let sources: &[(&[u8], &str)] = &[
        (
            include_bytes!(
                "../../../fixtures/signal-shadow-state-parity/oracle/rust/intraday_breakout.rs"
            ),
            "eeee8457c94d2ec7f9c46b8324f8f3a6d7b7e1429b4a0bc95593f43719a9267b",
        ),
        (
            include_bytes!(
                "../../../fixtures/signal-shadow-state-parity/oracle/rust/mean_reversion.rs"
            ),
            "6360f76526980258aff76132c469b4e512f4697ac24a4a874dac9c4d8fad927c",
        ),
        (
            include_bytes!(
                "../../../fixtures/signal-shadow-state-parity/oracle/rust/orchestrator.rs"
            ),
            "8b051119991c62c5f8e8a6dee2645177b788240b6662710e99ad525395f382e2",
        ),
        (
            include_bytes!("../../../fixtures/signal-shadow-state-parity/oracle/rust/types.rs"),
            "8b515e252bc493890483793887248a6a12bedcf072ab87c574d4d3efd3b7eedc",
        ),
    ];
    for (source, pin) in sources {
        assert_eq!(&format!("{:x}", Sha256::digest(source)), pin);
    }
}

#[test]
fn directed_entry_gates_match_python_and_preserve_archived_outputs() {
    use strategy_runtime_core::hybrid_intraday::intraday_breakout as actual;
    let reference: Value = serde_json::from_str(REFERENCE).unwrap();
    for probe in reference["entry_probes"].as_array().unwrap() {
        let dt = chrono::NaiveDateTime::parse_from_str(
            probe["label"].as_str().unwrap(),
            "%Y-%m-%dT%H:%M:%S",
        )
        .unwrap();
        let mut old = frozen_model().intraday_breakout;
        let mut new = actual::IntradayBreakoutEngine::new(actual::IntradayBreakoutConfig {
            k: 0.53,
            ..Default::default()
        });
        let start = dt.date().and_hms_opt(7, 0, 0).unwrap();
        for old_or_new in 0..2 {
            // Configure identical explicit test state, never copy model output
            // into the oracle. Both evaluate against pinned Python probes.
            if old_or_new == 0 {
                let mut s = old.snapshot();
                s.today_start = Some(start);
                s.yesterday_close = probe["prev_close"].as_f64();
                s.yesterday_range = probe["prev_range"].as_f64();
                s.yesterday_return = probe["prev_return"].as_f64();
                s.was_long_today = probe["used_long"].as_bool().unwrap();
                s.was_short_today = probe["used_short"].as_bool().unwrap();
                old.restore_snapshot(s);
            } else {
                let mut s = new.snapshot();
                s.today_start = Some(start);
                s.yesterday_close = probe["prev_close"].as_f64();
                s.yesterday_range = probe["prev_range"].as_f64();
                s.yesterday_return = probe["prev_return"].as_f64();
                s.was_long_today = probe["used_long"].as_bool().unwrap();
                s.was_short_today = probe["used_short"].as_bool().unwrap();
                new.restore_snapshot(s);
            }
        }
        let close = probe["close"].as_f64().unwrap();
        let before = format!("{:?}", new.snapshot());
        let (signal, trace) = new.evaluate_entry_traced(dt, close);
        assert_eq!(
            serde_json::to_value(&signal).unwrap(),
            serde_json::to_value(old.evaluate_entry(dt, close)).unwrap()
        );
        assert_eq!(
            serde_json::to_value(new.evaluate_entry(dt, close)).unwrap(),
            serde_json::to_value(&signal).unwrap()
        );
        assert_eq!(
            before,
            format!("{:?}", new.snapshot()),
            "diagnostic evaluation mutated engine"
        );
        assert_eq!(
            serde_json::to_value(trace).unwrap(),
            probe["no_intent_trace"]
        );
        assert_eq!(
            serde_json::to_value(signal.map(|s| serde_json::json!({"side":s.side}))).unwrap(),
            probe["signal"]
        );
    }
}

#[test]
fn orchestrator_trace_lifecycle_and_known_archive_eod_difference() {
    use strategy_runtime_core::hybrid_intraday::{
        intraday_breakout as bo, mean_reversion as mr, orchestrator as core,
    };
    let mut old = frozen_model();
    let mut new = core::HybridOrchestrator::new(
        mr::MeanReversionEngine::new(Default::default()),
        bo::IntradayBreakoutEngine::new(bo::IntradayBreakoutConfig {
            k: 0.53,
            ..Default::default()
        }),
        core::HybridOrchestratorConfig::default(),
    );
    let dt = chrono::NaiveDate::from_ymd_opt(2026, 9, 21)
        .unwrap()
        .and_hms_opt(23, 30, 0)
        .unwrap();
    let old_bar = orchestrator::BarInput {
        dt,
        open: 120.,
        high: 120.,
        low: 120.,
        close: 120.,
        close_prev: 100.,
        day_range_prev: 20.,
        has_open_position: false,
        has_live_orders: false,
    };
    let mut bar = core::BarInput {
        dt,
        open: 120.,
        high: 120.,
        low: 120.,
        close: 120.,
        close_prev: 100.,
        day_range_prev: 20.,
        has_open_position: false,
        has_live_orders: false,
    };
    // A new unused side crossing at EOD: explicitly retain the known older
    // orchestrator mismatch, never patch the archive to make a universal PASS.
    let mut s = old.intraday_breakout.snapshot();
    s.cur_day_date = Some(dt.date());
    s.today_start = Some(dt.date().and_hms_opt(7, 0, 0).unwrap());
    s.yesterday_close = Some(100.);
    s.yesterday_range = Some(20.);
    old.intraday_breakout.restore_snapshot(s);
    let mut s = new.intraday_breakout.snapshot();
    s.cur_day_date = Some(dt.date());
    s.today_start = Some(dt.date().and_hms_opt(7, 0, 0).unwrap());
    s.yesterday_close = Some(100.);
    s.yesterday_range = Some(20.);
    new.intraday_breakout.restore_snapshot(s);
    assert!(matches!(
        old.on_bar_with_mr_override(old_bar, None, None).as_slice(),
        [types::Action::SubmitEntry(_)]
    ));
    assert!(new.on_bar_with_mr_override(bar, None, None).is_empty());
    assert_eq!(new.last_no_intent().unwrap().1.reason, "eod_entry_cutoff");
    bar.has_live_orders = true;
    assert!(new.on_bar_with_mr_override(bar, None, None).is_empty());
    assert_eq!(new.last_no_intent().unwrap().1.reason, "live_orders_hold");
    new.warm_bar(bar);
    assert!(new.last_no_intent().is_none());
    bar.has_live_orders = false;
    bar.dt = dt.date().and_hms_opt(12, 0, 0).unwrap();
    assert_eq!(new.on_bar_with_mr_override(bar, None, None).len(), 1);
    assert!(
        new.last_no_intent().is_none(),
        "prior EOD reason leaked into entry"
    );
    assert!(new.on_bar_with_mr_override(bar, None, None).is_empty());
    assert_eq!(new.last_no_intent().unwrap().1.reason, "pending_entry_hold");
    let snapshot = new.snapshot();
    new.restore(snapshot);
    assert!(
        new.last_no_intent().is_none(),
        "trace must not restore as model truth"
    );
    new.reset();
    assert!(new.last_no_intent().is_none());
}

#[test]
fn observer_uses_branch_trace_and_rejects_stale_or_mutated_diagnosis() {
    use finam_signal_shadow::{
        observer::{compare_steps, Comparison, Decision, Step},
        segment::ObservationMode,
    };
    let label = 1_800_000;
    let after = serde_json::json!({"no_intent_trace":{"label_utc_ms":label,"reason":"before_wait","veto":null}});
    let decision = Decision::from_model_observation(None, 0, &after).unwrap();
    assert_eq!(decision.reason, "before_wait");
    let step = Step {
        mode: ObservationMode::BackfillReplay,
        instrument: "IMOEXF".into(),
        model_label_utc_ms: label,
        nominal_close_utc_ms: label + 600_000,
        provider_at_ms: None,
        received_at_ms: label + 600_000,
        canonical_ready_at_ms: label + 600_000,
        model_start_at_ms: label + 600_000,
        result_at_ms: label + 600_000,
        canonical_input_sha256: "a".repeat(64),
        raw_provenance_sha256: "b".repeat(64),
        before: serde_json::json!({}),
        after,
        decision,
    };
    assert_eq!(compare_steps(&step, &step), Comparison::SameInputMatch);
    let mut stale = step.clone();
    stale.after["no_intent_trace"]["label_utc_ms"] = (label - 600_000).into();
    assert_eq!(compare_steps(&step, &stale), Comparison::NotComparable);
    let mut wrong = step.clone();
    wrong.decision.reason = "minimum_range".into();
    assert_eq!(compare_steps(&step, &wrong), Comparison::NotComparable);
    let mut malformed = step.after.clone();
    malformed["no_intent_trace"]["reason"] = "guessed".into();
    assert!(Decision::from_model_observation(None, 0, &malformed).is_err());
    malformed = step.after.clone();
    malformed["pending_entry_reason"] = "breakout_long".into();
    assert!(Decision::from_model_observation(None, 0, &malformed).is_err());
    assert_eq!(
        Decision::from_model_observation(None, 0, &serde_json::json!({}))
            .unwrap()
            .reason,
        "model_returned_no_intent"
    );
}
