//! Fresh, no-riskgate shadow seed adapter. Never mutates a retained owner.
use super::*;

impl HybridIntradayRuntimeStrategy {
    /// Construct a NEW flat model using the existing history warmup algorithm.
    /// This is not restore/import, source admission, or a Stage5/6/7 capability.
    /// Caller must separately prove calendar coverage, range and reference seed.
    pub fn new_signal_shadow_from_history(
        config: HybridIntradayRuntimeConfig,
        context: broker_core::HybridRuntimeStrategyContext,
        history: &[broker_core::HybridRuntimeBarEvent],
    ) -> Result<Self, &'static str> {
        if context.trade_mode != broker_core::HybridRuntimeTradeMode::Paper
            || context.allow_live_orders
            || context.position_qty != Some(0.0)
            || config.live_mr_entries_enabled
            || config.risk_gate_mode != RiskGateMode::Disabled
            || config.mr_gate_policy != MrGatePolicy::Disabled
            || history.is_empty()
            || history.len() > 4096
        {
            return Err("invalid shadow seed");
        }
        let mut fresh = Self::new(config);
        fresh
            .validate_context_target(&context)
            .map_err(|_| "target mismatch")?;
        let mut bars = Vec::with_capacity(history.len());
        let mut last = None;
        for bar in history {
            fresh
                .validate_context_payload_instruments(&context, &bar.instrument)
                .map_err(|_| "instrument mismatch")?;
            if bar.origin != broker_core::HybridRuntimeBarOrigin::History
                || !bar.is_final
                || bar.timeframe_sec != 600
                || last.is_some_and(|ts| bar.close_time_utc <= ts)
                || bar
                    .close_time_utc
                    .checked_add(600)
                    .map_or(true, |close| close > context.event_ts_utc)
                || [bar.open, bar.high, bar.low, bar.close, bar.volume]
                    .iter()
                    .any(|x| !x.is_finite())
                || bar.low <= 0.0
                || bar.high < bar.low
                || bar.open < bar.low
                || bar.open > bar.high
                || bar.close < bar.low
                || bar.close > bar.high
                || bar.volume < 0.0
            {
                return Err("invalid history bar");
            }
            last = Some(bar.close_time_utc);
            bars.push(BarEvent {
                symbol: bar.instrument.symbol.clone(),
                close_time_utc: bar.close_time_utc,
                o: bar.open,
                h: bar.high,
                l: bar.low,
                close: bar.close,
                v: bar.volume,
                origin: DataOrigin::History,
            });
        }
        let ctx = Self::compatibility_context(&context);
        if Strategy::warmup_from_history(&mut fresh, &ctx, &bars) != bars.len()
            || !fresh.entry_ready
            || fresh.pending_entry.is_some()
            || fresh.pending_exit.is_some()
            || fresh.current_owner.is_some()
            || fresh.last_position_qty != 0.0
        {
            return Err("ineligible fresh warmup");
        }
        Ok(fresh)
    }

    /// Detached observation only: no setter, restore token or executable owner.
    pub fn signal_shadow_observation(&self) -> serde_json::Value {
        let state = serde_json::to_value(Strategy::state(self)).expect("finite model state");
        let source = &state["HybridIntradayRuntime"];
        let keys = [
            "entry_ready",
            "last_bar_close",
            "prev_day_close",
            "prev_day_range",
            "prev_day_return",
            "day_before_close",
            "last_day_local",
            "current_day_high",
            "current_day_low",
            "current_day_close",
            "today_start_local",
            "was_long_today",
            "was_short_today",
            "current_owner",
            "current_side",
            "last_position_qty",
            "pending_entry_owner",
            "pending_entry_side",
            "pending_entry_request_id",
            "pending_exit_request_id",
        ];
        let mut observation = serde_json::Value::Object(
            keys.into_iter()
                .map(|key| (key.into(), source[key].clone()))
                .collect(),
        );
        // Read the actual engine, not a separately seeded observer model. These
        // are raw thresholds (not rounded order prices or eligibility claims).
        let bo = &self.orchestrator.intraday_breakout;
        let anchors = bo.snapshot();
        observation["bo_levels"] = match (anchors.yesterday_close, anchors.yesterday_range) {
            (Some(close), Some(range)) => serde_json::json!({
                "long_entry": close + bo.config.k * range,
                "short_entry": close - bo.config.k * range,
                "long_stop1": close + bo.config.stop1_range * range,
                "short_stop1": close - bo.config.stop1_range * range,
                "long_stop2": close - bo.config.stop2_range * range,
                "short_stop2": close + bo.config.stop2_range * range,
            }),
            _ => serde_json::Value::Null,
        };
        // Pending reasons are retained model facts, NOT evidence that a new
        // decision occurred on this step. No suppression reason is inferred.
        observation["pending_entry_reason"] =
            serde_json::to_value(self.pending_entry.as_ref().map(|entry| entry.reason))
                .expect("model reason");
        observation["pending_exit_reason"] =
            serde_json::to_value(self.pending_exit.as_ref().map(|exit| exit.reason))
                .expect("model reason");
        // Detached actual orchestrator facts, including the pre-fill owner.
        // Runtime current_owner remains unset until the position callback.
        let orchestrator = self.orchestrator.snapshot();
        observation["orchestrator"] = serde_json::json!({
            "state": format!("{:?}", orchestrator.state),
            "owner": orchestrator.current_owner,
            "side": orchestrator.current_side,
            "pending_entry": orchestrator.has_pending_entry,
        });
        // Retain only a trace from the actual current-label call. Wrapper-level
        // early returns cannot borrow an earlier bar's diagnosis. An intent
        // added by a later runtime guard takes precedence over this lower-layer
        // empty-action trace; consumers use it only with a returned no-intent.
        observation["no_intent_trace"] = self.orchestrator.last_no_intent()
            .filter(|(dt, _)| self.last_processed_bar_ts
                .and_then(|label| self.utc_to_local_naive(label)) == Some(*dt))
            .map(|(_, trace)| serde_json::json!({
                "label_utc_ms": self.last_processed_bar_ts.and_then(|label| label.checked_mul(1000)),
                "reason": trace.reason,
                "veto": trace.veto,
            }))
            .unwrap_or(serde_json::Value::Null);
        observation
    }
}

#[cfg(test)]
mod observation_tests {
    use super::*;

    #[test]
    fn levels_use_engine_anchors_and_config_without_mutation() {
        let mut model = HybridIntradayRuntimeStrategy::new(
            super::super::tests::frozen_baseline07_no_riskgate_profile_v2_config(),
        );
        assert!(model.signal_shadow_observation()["bo_levels"].is_null());
        let bo = &mut model.orchestrator.intraday_breakout;
        let mut snapshot = bo.snapshot();
        snapshot.yesterday_close = Some(100.0);
        snapshot.yesterday_range = Some(20.0);
        bo.restore_snapshot(snapshot);
        bo.config.k = 0.5;
        bo.config.stop1_range = 0.25;
        bo.config.stop2_range = 0.1;
        let before = serde_json::to_value(Strategy::state(&model)).unwrap();
        let observation = model.signal_shadow_observation();
        assert_eq!(
            observation["bo_levels"],
            serde_json::json!({
                "long_entry":110.0, "short_entry":90.0,
                "long_stop1":105.0, "short_stop1":95.0,
                "long_stop2":98.0, "short_stop2":102.0,
            })
        );
        assert!(observation["pending_entry_reason"].is_null());
        assert!(observation["pending_exit_reason"].is_null());
        assert_eq!(
            before,
            serde_json::to_value(Strategy::state(&model)).unwrap()
        );
    }

    #[test]
    fn retained_exit_reason_is_the_actual_engine_decision() {
        let mut model = HybridIntradayRuntimeStrategy::new(
            super::super::tests::frozen_baseline07_no_riskgate_profile_v2_config(),
        );
        let bo = &mut model.orchestrator.intraday_breakout;
        let mut snapshot = bo.snapshot();
        snapshot.yesterday_close = Some(100.0);
        snapshot.yesterday_range = Some(20.0);
        bo.restore_snapshot(snapshot);
        let dt = chrono::NaiveDate::from_ymd_opt(2026, 10, 23)
            .unwrap()
            .and_hms_opt(12, 0, 0)
            .unwrap();
        let exit = bo.evaluate_exit(dt, 80.0, Side::Long).unwrap();
        assert_eq!(exit.reason, ReasonCode::BreakoutStop2Long);
        model.pending_exit = Some(PendingExit {
            owner: exit.owner,
            reason: exit.reason,
        });
        assert_eq!(
            model.signal_shadow_observation()["pending_exit_reason"],
            "breakout_stop2_long"
        );
        model.pending_exit = None;
        assert!(model.signal_shadow_observation()["pending_exit_reason"].is_null());
    }
}
