use chrono::{Duration, NaiveDate, NaiveDateTime, NaiveTime};
use serde_json::Value;
use strategy_runtime_core::hybrid_intraday::orchestrator::BarInput;
use strategy_runtime_core::hybrid_intraday::{
    Action, EntrySignal, EntryStyle, High180MrConfig, HybridOrchestrator, HybridOrchestratorConfig,
    IntradayBreakoutConfig, IntradayBreakoutEngine, MeanReversionConfig, MeanReversionEngine,
    MinRangeMode, Owner, ReasonCode, Side,
};

fn dt(hour: u32, minute: u32) -> NaiveDateTime {
    NaiveDate::from_ymd_opt(2026, 1, 5)
        .unwrap_or(NaiveDate::MIN)
        .and_hms_opt(hour, minute, 0)
        .unwrap_or(NaiveDateTime::MIN)
}

#[test]
fn target_profile_freezes_active_high180_defaults_and_wrapper_binding() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/stage5/imoexf_high180_profile_binding.json"
    ))
    .expect("high180 profile fixture must decode");
    assert_eq!(fixture["schema_version"], 1);
    assert_eq!(fixture["mr_variant"], "high180");
    assert_eq!(fixture["active_entry_engine"], "High180MrEngine");
    assert_eq!(fixture["active_override_path"], "on_bar_with_mr_override");
    assert_eq!(
        fixture["classic_mr"]["active_entry_formula_for_target_profile"],
        false
    );
    assert_eq!(fixture["risk_gate"]["enforced_for_entry"], false);

    let config = High180MrConfig::default();
    assert_eq!(config.min_rel_range, 0.005);
    assert_eq!(config.max_rel_range, 0.050);
    assert_eq!(config.k_long, 0.085);
    assert_eq!(config.k_short, 0.090);
    assert_eq!(config.stop_loss_mult, 7.0);
    assert_eq!(config.max_hold, Duration::minutes(180));
    assert_eq!(
        config.entry_end_time,
        NaiveTime::from_hms_opt(11, 59, 59).unwrap_or(NaiveTime::MIN)
    );

    let wrapper_oracle =
        include_str!("../../../source-oracles/alor-stage5/hybrid_intraday_runtime.rs");
    assert!(wrapper_oracle
        .contains("let high180_mr = High180MrEngine::new(High180MrConfig::default());"));
    assert!(wrapper_oracle.contains("MeanReversionVariant::High180"));
    assert!(wrapper_oracle.contains(".on_bar_with_mr_override("));
}

#[test]
fn high180_override_path_selects_override_not_classic_mr_candidate() {
    let classic = MeanReversionEngine::new(MeanReversionConfig::default());
    let breakout = IntradayBreakoutEngine::new(IntradayBreakoutConfig {
        min_range_mode: MinRangeMode::Disabled,
        wait_hours: 0.0,
        ..IntradayBreakoutConfig::default()
    });
    let mut orchestrator =
        HybridOrchestrator::new(classic, breakout, HybridOrchestratorConfig::default());
    let bar = BarInput {
        dt: dt(10, 0),
        open: 100.0,
        high: 101.0,
        low: 99.0,
        close: 101.95,
        close_prev: 102.0,
        day_range_prev: 2.0,
        has_open_position: false,
        has_live_orders: false,
    };
    let classic_candidate = orchestrator
        .mean_reversion
        .evaluate_entry(bar.dt, bar.close, bar.close_prev, bar.day_range_prev)
        .expect("classic fixture should produce a candidate");
    assert_eq!(classic_candidate.side, Side::Long);

    let high180_override = EntrySignal {
        owner: Owner::MeanReversion,
        side: Side::Short,
        entry_style: EntryStyle::Bracket,
        reason: ReasonCode::MorningMeanReversionShort,
        stop_price: Some(110.0),
        take_price: Some(100.0),
    };
    let actions = orchestrator.on_bar_with_mr_override(bar, Some(high180_override), None);

    assert_eq!(actions.len(), 1);
    match &actions[0] {
        Action::SubmitEntry(entry) => {
            assert_eq!(entry.owner, Owner::MeanReversion);
            assert_eq!(entry.side, Side::Short);
            assert_eq!(entry.reason, ReasonCode::MorningMeanReversionShort);
        }
        action => panic!("expected high180 override entry, got {action:?}"),
    }
}
