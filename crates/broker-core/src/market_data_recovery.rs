use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MarketDataRecoveryMode {
    Cold,
    Warm,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MarketDataRecoveryPhase {
    LoadingHistory,
    SyncingGap,
    LiveSubscribing,
    LiveReady,
    Degraded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MarketDataRecoveryBlocker {
    InvalidTimeframe,
    ReplayWindowMissing,
    ReplayWindowDoesNotCoverWatermark,
    ReplayMissing,
    ReplayNotContiguousToWatermark,
    ReplayGapDetected,
    TransportDisconnected,
    LiveSubscriptionMissing,
    FirstLiveFinalBarMissing,
    FirstLiveFinalBeforeReplayEnd,
    GapAbsenceNotProven,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarketDataRecoveryPlanInput {
    pub timeframe_sec: u32,
    pub overlap_bars: u32,
    pub last_final_bar_close_ts: Option<DateTime<Utc>>,
    pub cold_history_from_ts: DateTime<Utc>,
    pub checked_ts: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarketDataRecoveryPlan {
    pub mode: MarketDataRecoveryMode,
    pub replay_from_ts: DateTime<Utc>,
    pub replay_to_ts: DateTime<Utc>,
    pub overlap_bars: u32,
    pub last_final_bar_close_ts: Option<DateTime<Utc>>,
    pub invalid_timeframe: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarketDataRecoveryInput {
    pub mode: MarketDataRecoveryMode,
    pub timeframe_sec: u32,
    pub generation: u64,
    pub last_final_bar_close_ts: Option<DateTime<Utc>>,
    pub replay_from_ts: Option<DateTime<Utc>>,
    pub replay_to_ts: Option<DateTime<Utc>>,
    pub replay_bar_count: u64,
    pub replay_first_bar_close_ts: Option<DateTime<Utc>>,
    pub replay_last_bar_close_ts: Option<DateTime<Utc>>,
    pub overlap_dedup_bar_count: u64,
    pub replay_gap_detected: bool,
    pub transport_connected: bool,
    pub live_subscription_sent: bool,
    pub live_subscription_confirmed: bool,
    pub first_live_final_bar_close_ts: Option<DateTime<Utc>>,
    pub checked_ts: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarketDataRecoveryReport {
    pub phase: MarketDataRecoveryPhase,
    pub blockers: Vec<MarketDataRecoveryBlocker>,
    pub mode: MarketDataRecoveryMode,
    pub timeframe_sec: u32,
    pub generation: u64,
    pub last_final_bar_close_ts: Option<DateTime<Utc>>,
    pub replay_from_ts: Option<DateTime<Utc>>,
    pub replay_to_ts: Option<DateTime<Utc>>,
    pub replay_bar_count: u64,
    pub replay_first_bar_close_ts: Option<DateTime<Utc>>,
    pub replay_last_bar_close_ts: Option<DateTime<Utc>>,
    pub overlap_dedup_bar_count: u64,
    pub transport_connected: bool,
    pub live_subscription_sent: bool,
    pub live_subscription_confirmed: bool,
    pub first_live_final_bar_close_ts: Option<DateTime<Utc>>,
    pub gap_absence_proven: bool,
    pub checked_ts: DateTime<Utc>,
}

pub fn plan_market_data_recovery(input: MarketDataRecoveryPlanInput) -> MarketDataRecoveryPlan {
    if input.timeframe_sec == 0 {
        return MarketDataRecoveryPlan {
            mode: MarketDataRecoveryMode::Cold,
            replay_from_ts: input.cold_history_from_ts,
            replay_to_ts: input.checked_ts,
            overlap_bars: input.overlap_bars,
            last_final_bar_close_ts: input.last_final_bar_close_ts,
            invalid_timeframe: true,
        };
    }

    let mode = if input.last_final_bar_close_ts.is_some() {
        MarketDataRecoveryMode::Warm
    } else {
        MarketDataRecoveryMode::Cold
    };
    let replay_from_ts = input
        .last_final_bar_close_ts
        .map_or(input.cold_history_from_ts, |ts| {
            let overlap_seconds = i64::from(input.timeframe_sec) * i64::from(input.overlap_bars);
            ts - Duration::seconds(overlap_seconds)
        });

    MarketDataRecoveryPlan {
        mode,
        replay_from_ts,
        replay_to_ts: input.checked_ts,
        overlap_bars: input.overlap_bars,
        last_final_bar_close_ts: input.last_final_bar_close_ts,
        invalid_timeframe: false,
    }
}

pub fn evaluate_market_data_recovery(input: MarketDataRecoveryInput) -> MarketDataRecoveryReport {
    let mut blockers = Vec::new();

    if input.timeframe_sec == 0 {
        blockers.push(MarketDataRecoveryBlocker::InvalidTimeframe);
    }
    if input.replay_from_ts.is_none() || input.replay_to_ts.is_none() {
        blockers.push(MarketDataRecoveryBlocker::ReplayWindowMissing);
    }
    if !replay_window_covers_watermark(
        input.replay_from_ts,
        input.replay_to_ts,
        input.last_final_bar_close_ts,
    ) {
        blockers.push(MarketDataRecoveryBlocker::ReplayWindowDoesNotCoverWatermark);
    }
    if input.mode == MarketDataRecoveryMode::Warm && input.replay_bar_count == 0 {
        blockers.push(MarketDataRecoveryBlocker::ReplayMissing);
    }
    if !replay_is_contiguous_to_watermark(
        input.timeframe_sec,
        input.last_final_bar_close_ts,
        input.replay_first_bar_close_ts,
        input.replay_last_bar_close_ts,
    ) {
        blockers.push(MarketDataRecoveryBlocker::ReplayNotContiguousToWatermark);
    }
    if input.replay_gap_detected {
        blockers.push(MarketDataRecoveryBlocker::ReplayGapDetected);
    }
    if !input.transport_connected {
        blockers.push(MarketDataRecoveryBlocker::TransportDisconnected);
    }
    if !input.live_subscription_sent || !input.live_subscription_confirmed {
        blockers.push(MarketDataRecoveryBlocker::LiveSubscriptionMissing);
    }
    if input.first_live_final_bar_close_ts.is_none() {
        blockers.push(MarketDataRecoveryBlocker::FirstLiveFinalBarMissing);
    }
    if first_live_before_replay_end(
        input.first_live_final_bar_close_ts,
        input.replay_last_bar_close_ts,
    ) {
        blockers.push(MarketDataRecoveryBlocker::FirstLiveFinalBeforeReplayEnd);
    }

    let gap_absence_proven = blockers.iter().all(|blocker| {
        !matches!(
            blocker,
            MarketDataRecoveryBlocker::InvalidTimeframe
                | MarketDataRecoveryBlocker::ReplayWindowMissing
                | MarketDataRecoveryBlocker::ReplayWindowDoesNotCoverWatermark
                | MarketDataRecoveryBlocker::ReplayMissing
                | MarketDataRecoveryBlocker::ReplayNotContiguousToWatermark
                | MarketDataRecoveryBlocker::ReplayGapDetected
                | MarketDataRecoveryBlocker::FirstLiveFinalBeforeReplayEnd
        )
    });
    if !gap_absence_proven {
        blockers.push(MarketDataRecoveryBlocker::GapAbsenceNotProven);
    }

    let phase = recovery_phase(&input, &blockers, gap_absence_proven);

    MarketDataRecoveryReport {
        phase,
        blockers,
        mode: input.mode,
        timeframe_sec: input.timeframe_sec,
        generation: input.generation,
        last_final_bar_close_ts: input.last_final_bar_close_ts,
        replay_from_ts: input.replay_from_ts,
        replay_to_ts: input.replay_to_ts,
        replay_bar_count: input.replay_bar_count,
        replay_first_bar_close_ts: input.replay_first_bar_close_ts,
        replay_last_bar_close_ts: input.replay_last_bar_close_ts,
        overlap_dedup_bar_count: input.overlap_dedup_bar_count,
        transport_connected: input.transport_connected,
        live_subscription_sent: input.live_subscription_sent,
        live_subscription_confirmed: input.live_subscription_confirmed,
        first_live_final_bar_close_ts: input.first_live_final_bar_close_ts,
        gap_absence_proven,
        checked_ts: input.checked_ts,
    }
}

fn recovery_phase(
    input: &MarketDataRecoveryInput,
    blockers: &[MarketDataRecoveryBlocker],
    gap_absence_proven: bool,
) -> MarketDataRecoveryPhase {
    if blockers.iter().any(|blocker| {
        matches!(
            blocker,
            MarketDataRecoveryBlocker::InvalidTimeframe
                | MarketDataRecoveryBlocker::ReplayWindowMissing
                | MarketDataRecoveryBlocker::ReplayWindowDoesNotCoverWatermark
                | MarketDataRecoveryBlocker::ReplayGapDetected
                | MarketDataRecoveryBlocker::TransportDisconnected
                | MarketDataRecoveryBlocker::FirstLiveFinalBeforeReplayEnd
        )
    }) {
        return MarketDataRecoveryPhase::Degraded;
    }
    if gap_absence_proven
        && input.live_subscription_sent
        && input.live_subscription_confirmed
        && input.first_live_final_bar_close_ts.is_some()
    {
        return MarketDataRecoveryPhase::LiveReady;
    }
    if input.replay_bar_count > 0 || input.mode == MarketDataRecoveryMode::Warm {
        return MarketDataRecoveryPhase::SyncingGap;
    }
    if input.mode == MarketDataRecoveryMode::Cold {
        return MarketDataRecoveryPhase::LoadingHistory;
    }
    MarketDataRecoveryPhase::LiveSubscribing
}

fn replay_window_covers_watermark(
    replay_from_ts: Option<DateTime<Utc>>,
    replay_to_ts: Option<DateTime<Utc>>,
    last_final_bar_close_ts: Option<DateTime<Utc>>,
) -> bool {
    let Some(last_final_bar_close_ts) = last_final_bar_close_ts else {
        return true;
    };
    match (replay_from_ts, replay_to_ts) {
        (Some(from), Some(to)) => from <= last_final_bar_close_ts && to >= last_final_bar_close_ts,
        _ => false,
    }
}

fn replay_is_contiguous_to_watermark(
    timeframe_sec: u32,
    last_final_bar_close_ts: Option<DateTime<Utc>>,
    replay_first_bar_close_ts: Option<DateTime<Utc>>,
    replay_last_bar_close_ts: Option<DateTime<Utc>>,
) -> bool {
    let Some(last_final_bar_close_ts) = last_final_bar_close_ts else {
        return true;
    };
    if timeframe_sec == 0 {
        return false;
    }
    let (Some(first), Some(last)) = (replay_first_bar_close_ts, replay_last_bar_close_ts) else {
        return false;
    };
    let timeframe = Duration::seconds(i64::from(timeframe_sec));
    first <= last_final_bar_close_ts && last >= last_final_bar_close_ts + timeframe
}

fn first_live_before_replay_end(
    first_live_final_bar_close_ts: Option<DateTime<Utc>>,
    replay_last_bar_close_ts: Option<DateTime<Utc>>,
) -> bool {
    match (first_live_final_bar_close_ts, replay_last_bar_close_ts) {
        (Some(first_live), Some(replay_last)) => first_live < replay_last,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn ts(minute: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 7, 5, 9, minute, 0).unwrap()
    }

    fn warm_input() -> MarketDataRecoveryInput {
        MarketDataRecoveryInput {
            mode: MarketDataRecoveryMode::Warm,
            timeframe_sec: 60,
            generation: 2,
            last_final_bar_close_ts: Some(ts(10)),
            replay_from_ts: Some(ts(8)),
            replay_to_ts: Some(ts(13)),
            replay_bar_count: 4,
            replay_first_bar_close_ts: Some(ts(9)),
            replay_last_bar_close_ts: Some(ts(12)),
            overlap_dedup_bar_count: 2,
            replay_gap_detected: false,
            transport_connected: true,
            live_subscription_sent: true,
            live_subscription_confirmed: true,
            first_live_final_bar_close_ts: Some(ts(13)),
            checked_ts: ts(13),
        }
    }

    #[test]
    fn recovery_plan_uses_warm_overlap_from_last_final_bar() {
        let plan = plan_market_data_recovery(MarketDataRecoveryPlanInput {
            timeframe_sec: 60,
            overlap_bars: 2,
            last_final_bar_close_ts: Some(ts(10)),
            cold_history_from_ts: ts(0),
            checked_ts: ts(13),
        });

        assert_eq!(plan.mode, MarketDataRecoveryMode::Warm);
        assert_eq!(plan.replay_from_ts, ts(8));
        assert_eq!(plan.replay_to_ts, ts(13));
        assert!(!plan.invalid_timeframe);
    }

    #[test]
    fn recovery_plan_without_watermark_is_cold_history() {
        let plan = plan_market_data_recovery(MarketDataRecoveryPlanInput {
            timeframe_sec: 60,
            overlap_bars: 2,
            last_final_bar_close_ts: None,
            cold_history_from_ts: ts(0),
            checked_ts: ts(13),
        });

        assert_eq!(plan.mode, MarketDataRecoveryMode::Cold);
        assert_eq!(plan.replay_from_ts, ts(0));
    }

    #[test]
    fn recovery_accepts_contiguous_replay_and_fresh_live_final_bar() {
        let report = evaluate_market_data_recovery(warm_input());

        assert_eq!(report.phase, MarketDataRecoveryPhase::LiveReady);
        assert!(report.gap_absence_proven);
        assert!(report.blockers.is_empty());
    }

    #[test]
    fn recovery_blocks_when_replay_does_not_cover_last_watermark() {
        let mut input = warm_input();
        input.replay_from_ts = Some(ts(11));
        let report = evaluate_market_data_recovery(input);

        assert_eq!(report.phase, MarketDataRecoveryPhase::Degraded);
        assert!(report
            .blockers
            .contains(&MarketDataRecoveryBlocker::ReplayWindowDoesNotCoverWatermark));
        assert!(report
            .blockers
            .contains(&MarketDataRecoveryBlocker::GapAbsenceNotProven));
    }

    #[test]
    fn recovery_blocks_when_replay_is_not_contiguous_to_watermark() {
        let mut input = warm_input();
        input.replay_first_bar_close_ts = Some(ts(11));
        input.replay_last_bar_close_ts = Some(ts(12));
        let report = evaluate_market_data_recovery(input);

        assert_eq!(report.phase, MarketDataRecoveryPhase::SyncingGap);
        assert!(report
            .blockers
            .contains(&MarketDataRecoveryBlocker::ReplayNotContiguousToWatermark));
        assert!(!report.gap_absence_proven);
    }

    #[test]
    fn recovery_blocks_when_gap_detected_even_if_live_arrives() {
        let mut input = warm_input();
        input.replay_gap_detected = true;
        let report = evaluate_market_data_recovery(input);

        assert_eq!(report.phase, MarketDataRecoveryPhase::Degraded);
        assert!(report
            .blockers
            .contains(&MarketDataRecoveryBlocker::ReplayGapDetected));
    }

    #[test]
    fn recovery_requires_first_live_final_bar_after_subscription() {
        let mut input = warm_input();
        input.first_live_final_bar_close_ts = None;
        let report = evaluate_market_data_recovery(input);

        assert_eq!(report.phase, MarketDataRecoveryPhase::SyncingGap);
        assert!(report
            .blockers
            .contains(&MarketDataRecoveryBlocker::FirstLiveFinalBarMissing));
    }

    #[test]
    fn recovery_rejects_live_final_bar_older_than_replay_tail() {
        let mut input = warm_input();
        input.first_live_final_bar_close_ts = Some(ts(11));
        let report = evaluate_market_data_recovery(input);

        assert_eq!(report.phase, MarketDataRecoveryPhase::Degraded);
        assert!(report
            .blockers
            .contains(&MarketDataRecoveryBlocker::FirstLiveFinalBeforeReplayEnd));
    }

    #[test]
    fn recovery_degrades_while_transport_disconnected() {
        let mut input = warm_input();
        input.transport_connected = false;
        let report = evaluate_market_data_recovery(input);

        assert_eq!(report.phase, MarketDataRecoveryPhase::Degraded);
        assert!(report
            .blockers
            .contains(&MarketDataRecoveryBlocker::TransportDisconnected));
    }
}
