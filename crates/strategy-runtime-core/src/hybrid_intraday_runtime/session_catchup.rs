//! Initial, flat P1 seed only. Pure observation before the ordinary first-live
//! semantic transaction; no journal, seal, Redis, transport or clock override.
use super::*;
use broker_core::observed_m1::ObservedM1Receipt;
use chrono::DateTime;
use rust_decimal::{prelude::ToPrimitive, Decimal};

const MAX_CATCHUP_SECONDS: i64 = 4 * 24 * 60 * 60;

impl HybridIntradayRuntimeStrategy {
    pub(crate) fn stage8b_o3_initial_flat(&self) -> bool {
        self.stage8b_p1_bo_only_riskgate_disabled()
            && self.last_position_qty == 0.0
            && !self.has_live_orders()
            && self.pending_entry.is_none()
            && self.pending_entry_request_id.is_none()
            && self.deferred_entry.is_none()
            && self.deferred_exit.is_none()
            && self.current_owner.is_none()
            && self.current_side.is_none()
            && self.next_cycle_seq == 0
            && !self.orchestrator.snapshot().has_pending_entry
            && !self.safe_mode_close_only
    }

    pub(crate) fn stage8b_p1_catch_up_initial_session(
        &mut self,
        source: &ObservedM1Receipt,
        candidate_label: i64,
        strategy_id: &str,
        account_id: &broker_core::BrokerAccountId,
        coverage: Option<&crate::stage8b_o3_readmission::Stage8bO3Coverage>,
    ) -> Result<usize, ()> {
        let last = self.last_processed_bar_ts.ok_or(())?;
        let distance = candidate_label.checked_sub(last).ok_or(())?;
        if distance <= 0 || candidate_label.rem_euclid(600) != 0 || last.rem_euclid(600) != 0 {
            return Err(());
        }
        if let Some(coverage) = coverage {
            if !coverage.validate()
                || coverage.contains(last) != Some(true)
                || coverage.contains(candidate_label) != Some(true)
            {
                return Err(());
            }
        }
        if distance == 600 {
            return Ok(0);
        }
        if distance > MAX_CATCHUP_SECONDS
            || strategy_id.is_empty()
            || account_id.as_str().is_empty()
            || !self.stage8b_p1_bo_only_riskgate_disabled()
            || self.last_position_qty != 0.0
            || self.has_live_orders()
            || self.pending_entry.is_some()
            || self.pending_entry_request_id.is_some()
            || self.deferred_entry.is_some()
            || self.deferred_exit.is_some()
            || self.current_owner.is_some()
            || self.current_side.is_some()
            || self.next_cycle_seq != 0
            || self.orchestrator.snapshot().has_pending_entry
            || self.safe_mode_close_only
        {
            return Err(());
        }
        let last_time = DateTime::from_timestamp(last, 0).ok_or(())?;
        let candidate_time = DateTime::from_timestamp(candidate_label, 0).ok_or(())?;
        if source.start() > last_time
            || source.end() < candidate_time + ChronoDuration::minutes(10)
            || source.instrument().symbol != self.config.symbol
        {
            return Err(());
        }
        // A retained prefix must include the actual last processed bar. Full
        // initial/source-overlap equality remains enforced by S05 admission.
        let overlap = source.bucket(last_time).map_err(|_| ())?.bar;
        if overlap.close.to_f64() != self.last_bar_close {
            return Err(());
        }
        let tick = self
            .config
            .tick_size
            .to_string()
            .parse::<Decimal>()
            .map_err(|_| ())?;
        if tick <= Decimal::ZERO {
            return Err(());
        }
        // Validate the COMPLETE missing model-session range before touching
        // the in-memory strategy. Sparse M1 is allowed; an empty required M10
        // is not silently skipped or filled with invented zero-volume bars.
        let mut bars = Vec::new();
        for label in ((last + 600)..candidate_label).step_by(600) {
            let open = DateTime::from_timestamp(label, 0).ok_or(())?;
            if self.stage5d_classify_processed_bar_ts(open).ok_or(())?
                != Stage5dProcessedBarPolicy::RegularModelSession
            {
                continue;
            }
            if let Some(coverage) = coverage {
                match coverage.contains(label) {
                    Some(true) => {}
                    Some(false) => continue, // explicit approved clearing/non-trading window
                    None => return Err(()),
                }
            }
            let b = source.bucket(open).map_err(|_| ())?.bar;
            if [b.open, b.high, b.low, b.close]
                .iter()
                .any(|v| *v % tick != Decimal::ZERO)
            {
                return Err(());
            }
            let number = |v: Decimal| v.to_f64().filter(|v| v.is_finite()).ok_or(());
            bars.push(BarEvent {
                symbol: self.config.symbol.clone(),
                close_time_utc: label,
                o: number(b.open)?,
                h: number(b.high)?,
                l: number(b.low)?,
                close: number(b.close)?,
                v: number(b.volume)?,
                origin: DataOrigin::Replay,
            });
        }
        for bar in &bars {
            crate::stage8b_o3_readmission::observe_attempt(0);
            // This is the existing observation-only kernel, not generate-then-
            // discard. No pending owner or historical entry is ever acquired.
            let context = StrategyCtx {
                strategy_id: strategy_id.into(),
                portfolio: account_id.as_str().into(),
                exchange: "Moex".into(),
                symbol: self.config.symbol.clone(),
                tick_size: self.config.tick_size,
                trade_mode: crate::TradeMode::Paper,
                paper_execution_mode: crate::PaperExecutionMode::LiveOnly,
                allow_live_orders: false,
                gateway_phase: crate::live_guard::GatewayPhase::SyncingHistory,
                position_qty: Some(0.0),
                event_ts_utc: bar.close_time_utc,
                now_ts_utc: source.received_at().timestamp(),
                last_bar_ts: self.last_processed_bar_ts,
            };
            if !self
                .on_bar_with_first_boot_observation(&context, bar, true)
                .is_empty()
            {
                return Err(());
            }
        }
        Ok(bars.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use broker_core::{
        event::Bar, BrokerAccountId, Exchange, InstrumentId, Market, MarketDataSourceKind,
    };
    fn ts(v: &str) -> i64 {
        v.parse::<DateTime<Utc>>().unwrap().timestamp()
    }
    fn account() -> BrokerAccountId {
        BrokerAccountId::new("ACC_TEST_0001")
    }
    fn seed() -> HybridIntradayRuntimeStrategy {
        let mut s = HybridIntradayRuntimeStrategy::new(
            super::super::tests::frozen_baseline07_no_riskgate_profile_v2_config(),
        );
        assert_eq!(
            s.stage5c_config_fingerprint(),
            "665f18112142f6c9aee1b613e341870f0d559d783fe41df725e26415507e6290"
        );
        s.last_processed_bar_ts = Some(ts("2026-10-08T16:50:00Z"));
        s.last_day_local = Some(NaiveDate::from_ymd_opt(2026, 10, 8).unwrap());
        s.current_day_high = Some(2337.0);
        s.current_day_low = Some(2302.0);
        s.prev_day_close = Some(2337.0);
        s.prev_day_range = Some(26.0);
        s.last_bar_close = Some(2310.0);
        s.entry_ready = true;
        s.orchestrator.intraday_breakout.restore_snapshot(
            crate::hybrid_intraday::intraday_breakout::IntradayBreakoutSnapshot {
                cur_day_date: s.last_day_local,
                cur_day_high: s.current_day_high,
                cur_day_low: s.current_day_low,
                cur_day_close: Some(2310.0),
                yesterday_close: s.prev_day_close,
                yesterday_range: s.prev_day_range,
                yesterday_return: Some(0.004945173081057837),
                day_before_close: Some(2337.0),
                was_long_today: false,
                was_short_today: false,
                today_start: s.last_day_local.unwrap().and_hms_opt(7, 0, 0),
            },
        );
        s.sync_state();
        s
    }
    fn source(fault: &str) -> ObservedM1Receipt {
        let start = ts("2026-10-08T16:50:00Z");
        let candidate = ts("2026-10-09T07:10:00Z");
        let mut rows = Vec::new();
        for label in (start..candidate + 600).step_by(600) {
            if fault == "empty" && label == ts("2026-10-09T05:00:00Z") {
                continue;
            }
            // No first or final minute; the sparse aggregate is still exact.
            for minute in [1, 4, 7] {
                let open = DateTime::from_timestamp(label + minute * 60, 0).unwrap();
                let p = if label == candidate {
                    2400
                } else if label == start {
                    2310
                } else {
                    2320
                };
                let mut p = Decimal::from(p);
                if fault == "tick" && label == ts("2026-10-09T06:00:00Z") {
                    p += Decimal::new(1, 2);
                }
                if fault == "overlap" && label == start {
                    p += Decimal::ONE;
                }
                rows.push(Bar {
                    instrument: InstrumentId {
                        symbol: "IMOEXF".into(),
                        venue_symbol: Some("IMOEXF@RTSX".into()),
                        exchange: Exchange::Moex,
                        market: Market::Futures,
                    },
                    source_kind: MarketDataSourceKind::HistoricalPoll,
                    timeframe_sec: 60,
                    open_ts: open,
                    close_ts: open + ChronoDuration::minutes(1),
                    open: p,
                    high: p + Decimal::ONE,
                    low: p - Decimal::ONE,
                    close: p,
                    volume: Decimal::ONE,
                    is_final: true,
                });
            }
        }
        let from = if fault == "truncated" {
            start + 600
        } else {
            start
        };
        rows.retain(|r| r.open_ts.timestamp() >= from);
        ObservedM1Receipt::from_admitted_source(
            "ab".repeat(32),
            DateTime::from_timestamp(from, 0).unwrap(),
            DateTime::from_timestamp(candidate + 600, 0).unwrap(),
            DateTime::from_timestamp(candidate + 601, 0).unwrap(),
            rows[0].instrument.clone(),
            rows,
        )
        .unwrap()
    }
    #[test]
    fn catchup_actual_o2_seed_warms_previous_tail_and_new_session_without_intents() {
        let mut s = seed();
        let candidate = ts("2026-10-09T07:10:00Z");
        let receipt = source("valid");
        assert!(
            s.stage8b_p1_catch_up_initial_session(
                &receipt,
                candidate,
                "hybrid_imoexf",
                &account(),
                None
            )
            .unwrap()
                > 18
        );
        assert_eq!(s.next_cycle_seq, 0);
        assert!(s.pending_entry.is_none());
        assert_eq!(s.last_position_qty, 0.0);
        let bo = s.orchestrator.intraday_breakout.snapshot();
        assert_eq!(
            bo.today_start.unwrap(),
            NaiveDate::from_ymd_opt(2026, 10, 9)
                .unwrap()
                .and_hms_opt(7, 0, 0)
                .unwrap()
        );
        assert_eq!(bo.yesterday_close, Some(2320.0)); // actual missed prior-session tail
        let mut unwarmed = seed();
        let b = BarEvent {
            symbol: "IMOEXF".into(),
            close_time_utc: candidate,
            o: 2400.0,
            h: 2401.0,
            l: 2399.0,
            close: 2400.0,
            v: 3.0,
            origin: DataOrigin::Live,
        };
        let ctx = StrategyCtx {
            strategy_id: "hybrid_imoexf".into(),
            portfolio: account().as_str().into(),
            exchange: "Moex".into(),
            symbol: "IMOEXF".into(),
            tick_size: 0.5,
            trade_mode: crate::TradeMode::Paper,
            paper_execution_mode: crate::PaperExecutionMode::LiveOnly,
            allow_live_orders: false,
            gateway_phase: crate::live_guard::GatewayPhase::LiveReady,
            position_qty: Some(0.0),
            event_ts_utc: candidate + 600,
            now_ts_utc: candidate + 601,
            last_bar_ts: Some(candidate),
        };
        assert!(Strategy::on_bar(&mut unwarmed, &ctx, &b).is_empty());
        assert_eq!(Strategy::on_bar(&mut s, &ctx, &b).len(), 1);
    }
    #[test]
    fn catchup_rejects_entire_invalid_prefix_before_mutation() {
        for fault in [
            "empty",
            "truncated",
            "tick",
            "overlap",
            "open-position",
            "pending",
            "long-gap",
            "reversal",
        ] {
            let mut s = seed();
            if fault == "open-position" {
                s.last_position_qty = 1.0;
            }
            if fault == "pending" {
                s.pending_entry_request_id = Some(StrategyRequestId::new(uuid::Uuid::nil()));
            }
            let mut candidate = ts("2026-10-09T07:10:00Z");
            if fault == "long-gap" {
                candidate += 5 * 86400;
            }
            if fault == "reversal" {
                candidate = s.last_processed_bar_ts.unwrap();
            }
            let before = (
                serde_json::to_value(s.state()).unwrap(),
                s.last_processed_bar_ts,
                s.next_cycle_seq,
            );
            assert!(
                s.stage8b_p1_catch_up_initial_session(
                    &source(fault),
                    candidate,
                    "hybrid_imoexf",
                    &account(),
                    None,
                )
                .is_err(),
                "{fault}"
            );
            assert_eq!(
                before,
                (
                    serde_json::to_value(s.state()).unwrap(),
                    s.last_processed_bar_ts,
                    s.next_cycle_seq
                ),
                "{fault}"
            );
        }
    }

    #[test]
    fn o3_coverage_requires_explicit_gap_and_rejects_unknown_session() {
        use crate::stage8b_o3_readmission::*;
        let window = |a: &str, b: &str| Stage8bO3CoverageWindow {
            first_close_time_utc: ts(a),
            last_close_time_utc: ts(b),
        };
        let coverage = Stage8bO3Coverage {
            source_mode: "config-bound-explicit-session-windows-v1".into(),
            sessions: vec![
                Stage8bO3CoverageSession {
                    session_date: "2026-10-08".into(),
                    windows: vec![window("2026-10-08T04:00:00Z", "2026-10-08T20:40:00Z")],
                },
                Stage8bO3CoverageSession {
                    session_date: "2026-10-09".into(),
                    windows: vec![
                        window("2026-10-09T04:00:00Z", "2026-10-09T04:50:00Z"),
                        window("2026-10-09T05:10:00Z", "2026-10-09T20:40:00Z"),
                    ],
                },
            ],
        };
        let receipt = source("empty"); // 05:00 has no M1, NOT implicit zero-volume.
        let candidate = ts("2026-10-09T07:10:00Z");
        let mut accepted = seed();
        assert!(accepted
            .stage8b_p1_catch_up_initial_session(
                &receipt,
                candidate,
                "hybrid_imoexf",
                &account(),
                Some(&coverage)
            )
            .is_ok());
        for unknown in [false, true] {
            let mut invalid = coverage.clone();
            if unknown {
                invalid.sessions.pop();
            } else {
                invalid.sessions[1].windows[0].last_close_time_utc = ts("2026-10-09T05:00:00Z");
            }
            let mut s = seed();
            let before = serde_json::to_value(s.state()).unwrap();
            assert!(s
                .stage8b_p1_catch_up_initial_session(
                    &receipt,
                    candidate,
                    "hybrid_imoexf",
                    &account(),
                    Some(&invalid)
                )
                .is_err());
            assert_eq!(serde_json::to_value(s.state()).unwrap(), before);
        }
    }
}
