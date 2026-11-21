use broker_core::*;
use finam_signal_shadow::{
    feedback::{Action, Quote},
    model::{Error, ShadowModel},
};
use runtime_durable_service::Stage8bP1RuntimeProfileV2;

fn instrument() -> InstrumentId {
    InstrumentId {
        symbol: "IMOEXF".into(),
        venue_symbol: Some("IMOEXF@RTSX".into()),
        exchange: Exchange::Moex,
        market: Market::Futures,
    }
}

fn host() -> ShadowModel {
    // Test-only factory dependency: no service/owner is started or attached.
    ShadowModel::baseline07(
        HybridRuntimeStrategyContext {
            strategy_id: "shadow-fixture".into(),
            request_namespace_account: BrokerAccountId("shadow-not-broker".into()),
            instrument: instrument(),
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
    .unwrap()
}

#[test]
fn fixed_profile_matches_existing_v2_model_constructor() {
    let (_, expected) = Stage8bP1RuntimeProfileV2::build_hybrid_runtime().unwrap();
    assert_eq!(host().model_config_fingerprint(), expected);
}

fn seed_context() -> HybridRuntimeStrategyContext {
    let now = chrono::DateTime::parse_from_rfc3339("2026-09-28T04:00:00Z")
        .unwrap()
        .timestamp();
    HybridRuntimeStrategyContext {
        strategy_id: "shadow-fixture".into(),
        request_namespace_account: BrokerAccountId("shadow-not-broker".into()),
        instrument: instrument(),
        tick_size: 0.5,
        trade_mode: HybridRuntimeTradeMode::Paper,
        paper_execution_mode: HybridRuntimePaperExecutionMode::LiveOnly,
        allow_live_orders: false,
        gateway_phase: HybridRuntimeGatewayPhase::SyncingHistory,
        position_qty: Some(0.0),
        event_ts_utc: now,
        strategy_now_ts_utc: now,
        last_bar_ts_utc: None,
    }
}

fn seed_history() -> (Vec<HybridRuntimeBarEvent>, [chrono::NaiveDate; 4]) {
    use chrono::Timelike;
    let sessions = [22, 23, 24, 25].map(|d| chrono::NaiveDate::from_ymd_opt(2026, 9, d).unwrap());
    let history = bars()
        .into_iter()
        .filter_map(|mut bar| {
            let local = chrono::DateTime::from_timestamp(bar.close_time_utc, 0).unwrap()
                + chrono::Duration::hours(3);
            if !sessions.contains(&local.date_naive()) || local.hour() < 7 {
                return None;
            }
            bar.origin = HybridRuntimeBarOrigin::History;
            Some(bar)
        })
        .collect();
    (history, sessions)
}

fn history_hash(history: &[HybridRuntimeBarEvent]) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(serde_json::to_vec(history).unwrap()))
}

#[test]
fn four_session_seed_uses_existing_warmup_without_intents() {
    let (history, sessions) = seed_history();
    let model = ShadowModel::baseline07_warmed(
        seed_context(),
        &history,
        sessions,
        &history_hash(&history),
        1000,
    )
    .unwrap();
    let observed = model.observation();
    assert_eq!(observed["entry_ready"], true);
    assert_eq!(observed["current_day_close"], 2275.5);
    assert!(observed["prev_day_range"].as_f64().unwrap() > 0.0);
    assert!(observed["current_owner"].is_null());
    assert!(observed["pending_entry_request_id"].is_null());
    assert!(model.pending().is_none());
    assert_eq!(model.position_lots(), 0);
}

#[test]
fn seed_rejects_wrong_hash_missing_boundary_future_and_duplicate() {
    let (history, sessions) = seed_history();
    assert!(ShadowModel::baseline07_warmed(
        seed_context(),
        &history,
        sessions,
        &"0".repeat(64),
        1000
    )
    .is_err());
    let missing = &history[1..];
    assert!(ShadowModel::baseline07_warmed(
        seed_context(),
        missing,
        sessions,
        &history_hash(missing),
        1000
    )
    .is_err());
    let mut early = seed_context();
    early.event_ts_utc = history.last().unwrap().close_time_utc;
    assert!(ShadowModel::baseline07_warmed(
        early,
        &history,
        sessions,
        &history_hash(&history),
        1000
    )
    .is_err());
    let mut duplicate = history.clone();
    duplicate.insert(1, history[0].clone());
    assert!(ShadowModel::baseline07_warmed(
        seed_context(),
        &duplicate,
        sessions,
        &history_hash(&duplicate),
        1000
    )
    .is_err());
}

fn bars() -> Vec<HybridRuntimeBarEvent> {
    let mut reader = csv::Reader::from_reader(
        include_bytes!("../../../fixtures/stage8b-p1f-parity/imoexf_raw_10m_msk_utc.csv")
            .as_slice(),
    );
    reader
        .records()
        .map(|record| {
            let r = record.unwrap();
            let time = chrono::DateTime::parse_from_str(&r[1], "%Y-%m-%d %H:%M:%S%:z")
                .unwrap()
                .timestamp();
            let n = |i| r.get(i).unwrap().parse::<f64>().unwrap();
            HybridRuntimeBarEvent {
                instrument: instrument(),
                close_time_utc: time,
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

#[test]
fn actual_model_entry_position_feedback_and_later_exit() {
    let mut model = host();
    let mut entries = 0;
    let mut exits = 0;
    // Historical fixture with deliberately injected post-decision quotes.
    // This proves callback composition, not warmup admission or feed parity.
    for bar in bars() {
        let now = (bar.close_time_utc + 600) * 1000;
        let request = model.on_bar(bar.clone(), now).unwrap();
        assert!(model.on_bar(bar.clone(), now).unwrap().is_none());
        if let Some(request) = request {
            let before = model.position_lots();
            match request.action {
                Action::EnterLong | Action::EnterShort => {
                    assert_eq!(before, 0);
                    entries += 1;
                }
                Action::Exit => {
                    assert_ne!(before, 0);
                    exits += 1;
                }
            }
            let fill = model
                .on_quote(
                    Quote {
                        instrument: "IMOEXF".into(),
                        source_id: format!("synthetic-{}", request.sequence),
                        price_ticks: (bar.close / 0.5).round() as i64,
                        event_at_ms: now + 1,
                        received_at_ms: now + 2,
                    },
                    now + 3,
                )
                .unwrap()
                .unwrap();
            assert_eq!(model.position_lots(), fill.position_lots);
            assert!(model.pending().is_none());
            assert!(!model.held());
        }
        if entries > 0 && exits > 0 {
            break;
        }
    }
    assert!(
        entries > 0,
        "actual model must generate entry, not fake intent"
    );
    assert!(
        exits > 0,
        "actual model must generate exit after position feedback"
    );
    assert_eq!(model.position_lots(), 0);
}

#[test]
fn conflict_and_early_bar_stop_segment() {
    let bar = bars().remove(0);
    let now = (bar.close_time_utc + 600) * 1000;
    let mut model = host();
    assert_eq!(model.on_bar(bar.clone(), now - 1), Err(Error::Input));
    assert_eq!(model.on_bar(bar.clone(), now), Err(Error::Hold));
    let mut model = host();
    model.on_bar(bar.clone(), now).unwrap();
    let mut conflict = bar;
    conflict.volume += 1.0;
    assert_eq!(model.on_bar(conflict, now), Err(Error::Input));
    assert!(model.held());
}

#[test]
fn missing_quote_keeps_actual_model_pending_and_bad_quote_holds() {
    let mut model = host();
    let all = bars();
    for pair in all.windows(2) {
        let bar = &pair[0];
        let now = (bar.close_time_utc + 600) * 1000;
        if let Some(request) = model.on_bar(bar.clone(), now).unwrap() {
            assert_ne!(request.action, Action::Exit);
            assert_eq!(model.position_lots(), 0);
            let next = pair[1].clone();
            let next_time = (next.close_time_utc + 600) * 1000;
            assert_eq!(model.on_bar(next, next_time), Err(Error::Pending));
            assert_eq!(model.pending(), Some(&request));
            assert_eq!(
                model.on_quote(
                    Quote {
                        instrument: "WRONG".into(),
                        source_id: "wrong".into(),
                        price_ticks: 4400,
                        event_at_ms: now + 1,
                        received_at_ms: now + 2,
                    },
                    now + 3
                ),
                Err(Error::Feedback)
            );
            assert!(model.held());
            assert_eq!(model.position_lots(), 0);
            assert_eq!(model.pending(), Some(&request));
            return;
        }
    }
    panic!("fixture must exercise actual model pending");
}
