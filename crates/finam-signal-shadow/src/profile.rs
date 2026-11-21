//! Fixed baseline07 no-riskgate constructor, independent of durable-service.
use chrono::{Duration, NaiveTime};
use sha2::{Digest, Sha256};
use strategy_runtime_core::{
    hybrid_intraday::{
        BreakoutEodMode, HybridOrchestratorConfig, IntradayBreakoutConfig, MeanReversionConfig,
        MinRangeMode,
    },
    BrokerNeutralMarketOrderStyle, HybridIntradayProfile, HybridIntradayRuntimeConfig,
    MeanReversionVariant, MrGatePolicy, RiskGateMode,
};

pub const PROFILE_SHA256: &str = "b267f680d2d376d15c62d347acf89dc9dd26d8ce0a850a66f2dd7710d44282be";
const PROFILE: &[u8] = include_bytes!("../../../docs/stage-8/stage8b-p1e-runtime-profile-v2.json");

/// Hash protects the source profile; an integration regression compares the
/// resulting model fingerprint with the existing accepted V2 constructor.
pub fn baseline07_config() -> Result<HybridIntradayRuntimeConfig, &'static str> {
    if format!("{:x}", Sha256::digest(PROFILE)) != PROFILE_SHA256 {
        return Err("fixed profile bytes changed");
    }
    let time = |h, m, s| NaiveTime::from_hms_opt(h, m, s).expect("constant valid time");
    Ok(HybridIntradayRuntimeConfig {
        symbol: "IMOEXF".into(),
        profile: HybridIntradayProfile::BaselineRuntimeHybrid,
        mr_variant: MeanReversionVariant::High180,
        live_mr_entries_enabled: false,
        mr_gate_policy: MrGatePolicy::Disabled,
        risk_gate_mode: RiskGateMode::Disabled,
        risk_gate_seed_file: None,
        risk_gate_ledger_key: None,
        model_session_start_time: Some(time(7, 0, 0)),
        model_session_end_time: Some(time(23, 49, 59)),
        qty: 1.0,
        live_order_style: BrokerNeutralMarketOrderStyle::Market,
        tick_size: 0.5,
        marketable_limit_offset_ticks: 0,
        timezone_offset_hours: 3,
        session_close_hour: 23,
        session_close_minute: 49,
        weekends_off: true,
        stop_end_buffer_sec: 60,
        repair_deadline_sec: 180,
        sl_escalate_timeout_sec: 30,
        max_repair_retries: 3,
        repair_backoff_base_sec: 5,
        repair_backoff_max_sec: 60,
        pending_timeout_sec: 60,
        partial_entry_fill_timeout_ms: 3000,
        mr_config: MeanReversionConfig {
            min_range_long: 0.013,
            max_range_long: 0.035,
            k_long: 0.032,
            take_k_long: 0.11,
            stop_k_long: 0.44,
            min_range_short: 0.010,
            max_range_short: 0.045,
            k_short: 0.055,
            take_k_short: 0.16,
            stop_k_short: 0.43,
            tick_size: 0.5,
            session_end_time: time(9, 59, 0),
            exit_offset: Duration::seconds(600),
        },
        breakout_config: IntradayBreakoutConfig {
            k: 0.53,
            stop1_range: 0.51,
            stop2_range: 0.35,
            big_move_threshold: 0.025,
            min_range: 1.01,
            min_range_mode: MinRangeMode::Absolute,
            exclude_weekends: true,
            wait_hours: 3.0,
        },
        orchestrator_config: HybridOrchestratorConfig {
            breakout_eod_mode: BreakoutEodMode::SameDay,
            breakout_overnight_exit_time: time(9, 30, 0),
        },
    })
}
