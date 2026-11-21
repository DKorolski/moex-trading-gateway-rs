//! Local historical model callbacks with synthetic quote feedback only.
//! Not observed FINAM sessions, latency evidence, or two-session qualification.
use broker_core::*;
use chrono::{DateTime, Datelike, Duration, Timelike, Utc};
use finam_signal_shadow::{
    feedback::{Action, Quote},
    model::{Error, ShadowModel},
};

fn instrument() -> InstrumentId {
    InstrumentId {
        symbol: "IMOEXF".into(),
        venue_symbol: Some("IMOEXF@RTSX".into()),
        exchange: Exchange::Moex,
        market: Market::Futures,
    }
}

fn model() -> ShadowModel {
    ShadowModel::baseline07(
        HybridRuntimeStrategyContext {
            strategy_id: "local-eod-test".into(),
            request_namespace_account: BrokerAccountId("not-a-broker".into()),
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

fn bars() -> Vec<HybridRuntimeBarEvent> {
    csv::Reader::from_reader(
        include_bytes!("../../../fixtures/stage8b-p1f-parity/imoexf_raw_10m_msk_utc.csv")
            .as_slice(),
    )
    .records()
    .map(|record| {
        let r = record.unwrap();
        let label = DateTime::parse_from_str(&r[1], "%Y-%m-%d %H:%M:%S%:z").unwrap();
        let n = |i| r.get(i).unwrap().parse::<f64>().unwrap();
        HybridRuntimeBarEvent {
            instrument: instrument(),
            close_time_utc: label.timestamp(),
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

fn local(bar: &HybridRuntimeBarEvent) -> DateTime<Utc> {
    DateTime::from_timestamp(bar.close_time_utc, 0).unwrap() + Duration::hours(3)
}

fn fill(model: &mut ShadowModel, bar: &HybridRuntimeBarEvent, now: i64) {
    let request = model.pending().unwrap().clone();
    let fill = model
        .on_quote(
            Quote {
                instrument: "IMOEXF".into(),
                source_id: format!("synthetic-eod-{}", request.sequence),
                price_ticks: (bar.close * 2.0).round() as i64,
                event_at_ms: now + 1,
                received_at_ms: now + 2,
            },
            now + 3,
        )
        .unwrap()
        .unwrap();
    assert_eq!(model.position_lots(), fill.position_lots);
    assert!(model.pending().is_none());
}

#[test]
fn actual_eod_feedback_final_label_and_next_weekday_rollover() {
    let mut model = model();
    let mut eod_exits = 0;
    let mut final_labels = 0;
    let mut weekend_rollovers = 0;
    let mut previous_final: Option<(DateTime<Utc>, serde_json::Value)> = None;
    let mut inputs = bars();
    // Extend the pinned Friday endpoint with an explicit synthetic Monday
    // 07:00 label; the fixture itself supplies no following Monday session.
    let mut monday = inputs.last().unwrap().clone();
    assert_eq!(local(&monday).weekday(), chrono::Weekday::Fri);
    monday.close_time_utc = DateTime::parse_from_rfc3339("2026-09-28T07:00:00+03:00")
        .unwrap()
        .timestamp();
    monday.open = monday.close;
    monday.high = monday.close + 0.5;
    monday.low = monday.close - 0.5;
    monday.volume = 1.0;
    inputs.push(monday);
    for bar in inputs {
        let local = local(&bar);
        let now = (bar.close_time_utc + 600) * 1000;
        let request = model.on_bar(bar.clone(), now).unwrap();
        let state = model.observation();
        // Earlier dates in this historical inventory open at 08:50. Only
        // complete 07:00-start transitions witness the requested start seam.
        if let Some((last_local, last_state)) = previous_final
            .take()
            .filter(|_| (local.hour(), local.minute()) == (7, 0))
        {
            let mut next = last_local.date_naive().succ_opt().unwrap();
            while next.weekday().number_from_monday() > 5 {
                next = next.succ_opt().unwrap();
            }
            assert_eq!(local.date_naive(), next);
            assert_eq!((local.hour(), local.minute()), (7, 0));
            assert_eq!(state["prev_day_close"], last_state["last_bar_close"]);
            assert_eq!(state["current_day_high"], bar.high);
            assert_eq!(state["current_day_low"], bar.low);
            assert_eq!(state["last_day_local"], next.to_string());
            if last_local.weekday() == chrono::Weekday::Fri {
                assert_eq!(local.weekday(), chrono::Weekday::Mon);
                weekend_rollovers += 1;
            }
        }
        if let Some(request) = request {
            let eod = state["pending_exit_reason"] == "breakout_eod_exit";
            if eod {
                assert_eq!((local.hour(), local.minute()), (23, 30));
                assert_eq!(request.action, Action::Exit);
                assert_ne!(model.position_lots(), 0, "exit intent is not flat");
                assert!(model.pending().is_some());
                eod_exits += 1;
            }
            fill(&mut model, &bar, now);
            if eod {
                assert_eq!(model.position_lots(), 0);
                assert!(model.observation()["pending_exit_reason"].is_null());
            }
        }
        if (local.hour(), local.minute()) == (23, 40) {
            assert_eq!(model.position_lots(), 0);
            assert!(model.pending().is_none());
            assert_eq!(state["last_bar_close"], bar.close);
            assert_eq!(state["current_day_close"], bar.close);
            previous_final = Some((local, model.observation()));
            final_labels += 1;
        }
    }
    assert!(
        eod_exits > 0,
        "fixture must exercise actual EOD exit callbacks"
    );
    assert!(final_labels > 1);
    assert!(weekend_rollovers > 0);
    assert!(!model.held());
}

#[test]
fn unresolved_actual_eod_exit_is_nonflat_and_blocks_following_bar() {
    let mut model = model();
    let bars = bars();
    for pair in bars.windows(2) {
        let bar = &pair[0];
        let now = (bar.close_time_utc + 600) * 1000;
        if let Some(request) = model.on_bar(bar.clone(), now).unwrap() {
            if model.observation()["pending_exit_reason"] == "breakout_eod_exit" {
                assert_eq!((local(bar).hour(), local(bar).minute()), (23, 30));
                assert_eq!(request.action, Action::Exit);
                assert_ne!(model.position_lots(), 0);
                assert_eq!(
                    model.on_bar(pair[1].clone(), (pair[1].close_time_utc + 600) * 1000),
                    Err(Error::Pending)
                );
                assert_eq!(model.pending(), Some(&request));
                assert_ne!(model.position_lots(), 0);
                return;
            }
            fill(&mut model, bar, now);
        }
    }
    panic!("fixture must exercise an actual EOD exit");
}
