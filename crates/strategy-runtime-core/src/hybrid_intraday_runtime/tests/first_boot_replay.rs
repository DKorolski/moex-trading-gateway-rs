//! First-boot observation is not ordinary paper Replay or historical simulation.
use super::*;
use broker_core::{
    HybridRuntimeBarEvent, HybridRuntimeBarOrigin as Origin, HybridRuntimeCallbackInput,
};

fn warmed() -> HybridIntradayRuntimeStrategy {
    let mut strategy =
        HybridIntradayRuntimeStrategy::new(frozen_baseline07_no_riskgate_profile_v2_config());
    let bars = [
        test_bar_ohlc(
            ts_local(2026, 10, 6, 10, 0, 0),
            2200.0,
            2240.0,
            2160.0,
            2200.0,
            DataOrigin::History,
        ),
        test_bar_ohlc(
            ts_local(2026, 10, 7, 10, 0, 0),
            2200.0,
            2240.0,
            2160.0,
            2200.0,
            DataOrigin::History,
        ),
        test_bar_ohlc(
            ts_local(2026, 10, 8, 7, 0, 0),
            2200.0,
            2201.0,
            2199.0,
            2200.0,
            DataOrigin::History,
        ),
    ];
    assert_eq!(
        strategy.warmup_from_history(&frozen_no_riskgate_context(), &bars),
        3
    );
    strategy
}

fn input(price: f64) -> HybridRuntimeCallbackInput<HybridRuntimeBarEvent> {
    let mut context = broker_context(
        broker_core::HybridRuntimeTradeMode::Paper,
        broker_core::HybridRuntimeGatewayPhase::LiveReady,
    );
    let label = ts_local(2026, 10, 8, 15, 0, 0);
    context.allow_live_orders = false;
    context.position_qty = Some(0.0);
    context.event_ts_utc = label + 600;
    context.strategy_now_ts_utc = label + 900;
    context.last_bar_ts_utc = Some(label + 600);
    HybridRuntimeCallbackInput {
        payload: HybridRuntimeBarEvent {
            instrument: context.instrument.clone(),
            close_time_utc: label,
            open: price,
            high: price + 0.5,
            low: price - 0.5,
            close: price,
            volume: 10.0,
            origin: Origin::Replay,
            is_final: true,
            timeframe_sec: 600,
        },
        context,
    }
}

#[test]
fn first_boot_replay_observes_bo_signal_without_intents_or_phantom_pending() {
    for (price, side) in [(2100.0, Side::Short), (2300.0, Side::Long)] {
        // Regression trigger: ordinary Paper/LiveOnly Replay still returns a
        // hypothetical entry. It is NOT legal first-boot material.
        let mut ordinary = warmed();
        assert_eq!(ordinary.on_broker_bar(input(price)).unwrap().len(), 1);
        let mut observed = warmed();
        assert!(observed
            .observe_stage8b_p1_first_boot_replay(input(price))
            .unwrap()
            .is_empty());
        assert_eq!(
            observed.last_processed_bar_ts,
            Some(input(price).payload.close_time_utc)
        );
        assert_eq!(observed.last_bar_close, Some(price));
        assert!(!observed.has_live_orders());
        assert!(observed.current_owner.is_none());
        assert!(observed.active_cycle_id.is_none());
        assert!(!observed.orchestrator.snapshot().has_pending_entry);
        assert_eq!(
            observed.orchestrator.snapshot().state,
            crate::hybrid_intraday::HybridState::Flat
        );
        assert!(!observed.breakout_snapshot().was_long_today);
        assert!(!observed.breakout_snapshot().was_short_today);
        assert_frozen_no_riskgate_accounting(&observed);
        // Same indicator projection as warming the bar; only execution differs.
        assert_eq!(
            format!("{:?}", observed.breakout_snapshot()),
            format!("{:?}", ordinary.breakout_snapshot())
        );
        let mut live = input(price);
        live.payload.origin = Origin::Live;
        live.payload.close_time_utc += 600;
        live.context.event_ts_utc += 600;
        live.context.strategy_now_ts_utc += 600;
        assert_eq!(observed.on_broker_bar(live).unwrap().len(), 1);
        assert_eq!(observed.pending_entry.unwrap().side, side);
    }
}

#[test]
fn first_boot_replay_rejects_non_observation_inputs_before_mutation() {
    for case in 0..8 {
        let mut strategy = warmed();
        let mut input = input(2100.0);
        match case {
            0 => input.payload.origin = Origin::Live,
            1 => input.payload.is_final = false,
            2 => input.payload.timeframe_sec = 60,
            3 => input.context.trade_mode = broker_core::HybridRuntimeTradeMode::Backtest,
            4 => {
                input.context.paper_execution_mode =
                    broker_core::HybridRuntimePaperExecutionMode::HistorySim
            }
            5 => input.context.allow_live_orders = true,
            6 => input.context.position_qty = Some(1.0),
            _ => input.payload.instrument.symbol = "OTHER".into(),
        }
        let before = format!("{:?}", strategy.state());
        assert!(
            strategy
                .observe_stage8b_p1_first_boot_replay(input)
                .is_none(),
            "case {case}"
        );
        assert_eq!(format!("{:?}", strategy.state()), before, "case {case}");
    }
}
