use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::event::MarketDataSourceKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MarketDataLifecyclePhase {
    LoadingHistory,
    SyncingGap,
    LiveSubscribing,
    LiveReady,
    Degraded,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MarketDataLifecycleBlocker {
    BarsSubscriptionDisabled,
    TransportDisconnected,
    NoLiveBarsObserved,
    NoFinalLiveBarsObserved,
    MarketDataStale,
    RestDataNotStrategySource,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrokerMarketDataLifecycleInput {
    pub bars_subscription_enabled: bool,
    pub quotes_subscription_enabled: bool,
    pub transport_connected: bool,
    pub strategy_source_kind: MarketDataSourceKind,
    pub rest_bars_used_for_strategy: bool,
    pub rest_market_data_used_for_strategy: bool,
    pub history_bar_count: u64,
    pub recovery_bar_count: u64,
    pub live_bar_count: u64,
    pub live_final_bar_count: u64,
    pub live_forming_bar_count: u64,
    pub quote_count: u64,
    pub first_live_bar_seen: bool,
    pub first_live_final_bar_seen: bool,
    pub first_live_final_bar_close_ts: Option<DateTime<Utc>>,
    pub last_history_bar_close_ts: Option<DateTime<Utc>>,
    pub last_recovery_bar_close_ts: Option<DateTime<Utc>>,
    pub last_live_bar_close_ts: Option<DateTime<Utc>>,
    pub last_final_live_bar_close_ts: Option<DateTime<Utc>>,
    pub stale_after_sec: Option<u64>,
    pub checked_ts: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrokerMarketDataLifecycleSnapshot {
    pub phase: MarketDataLifecyclePhase,
    pub blockers: Vec<MarketDataLifecycleBlocker>,
    pub bars_subscription_enabled: bool,
    pub quotes_subscription_enabled: bool,
    pub transport_connected: bool,
    pub strategy_source_kind: MarketDataSourceKind,
    pub rest_bars_used_for_strategy: bool,
    pub rest_market_data_used_for_strategy: bool,
    pub history_bar_count: u64,
    pub recovery_bar_count: u64,
    pub live_bar_count: u64,
    pub live_final_bar_count: u64,
    pub live_forming_bar_count: u64,
    pub quote_count: u64,
    pub first_live_bar_seen: bool,
    pub first_live_final_bar_seen: bool,
    pub first_live_final_bar_close_ts: Option<DateTime<Utc>>,
    pub last_history_bar_close_ts: Option<DateTime<Utc>>,
    pub last_recovery_bar_close_ts: Option<DateTime<Utc>>,
    pub last_live_bar_close_ts: Option<DateTime<Utc>>,
    pub last_final_live_bar_close_ts: Option<DateTime<Utc>>,
    pub live_bar_stale_for_sec: Option<u64>,
    pub stale_after_sec: Option<u64>,
    pub checked_ts: DateTime<Utc>,
}

pub fn evaluate_market_data_lifecycle(
    input: BrokerMarketDataLifecycleInput,
) -> BrokerMarketDataLifecycleSnapshot {
    let mut blockers = Vec::new();

    if !input.bars_subscription_enabled {
        blockers.push(MarketDataLifecycleBlocker::BarsSubscriptionDisabled);
    }
    if !input.transport_connected {
        blockers.push(MarketDataLifecycleBlocker::TransportDisconnected);
    }
    if input.rest_bars_used_for_strategy || input.rest_market_data_used_for_strategy {
        blockers.push(MarketDataLifecycleBlocker::RestDataNotStrategySource);
    }
    if input.live_bar_count == 0 {
        blockers.push(MarketDataLifecycleBlocker::NoLiveBarsObserved);
    }
    if input.live_final_bar_count == 0 {
        blockers.push(MarketDataLifecycleBlocker::NoFinalLiveBarsObserved);
    }

    let live_bar_stale_for_sec = stale_for_sec(
        input.last_live_bar_close_ts,
        input.stale_after_sec,
        input.checked_ts,
    );
    if live_bar_stale_for_sec.is_some() {
        blockers.push(MarketDataLifecycleBlocker::MarketDataStale);
    }

    let phase = if blockers.iter().any(|blocker| {
        matches!(
            blocker,
            MarketDataLifecycleBlocker::BarsSubscriptionDisabled
                | MarketDataLifecycleBlocker::TransportDisconnected
                | MarketDataLifecycleBlocker::MarketDataStale
                | MarketDataLifecycleBlocker::RestDataNotStrategySource
        )
    }) {
        MarketDataLifecyclePhase::Degraded
    } else if input.live_final_bar_count > 0 {
        MarketDataLifecyclePhase::LiveReady
    } else if input.recovery_bar_count > 0 {
        MarketDataLifecyclePhase::SyncingGap
    } else if input.history_bar_count > 0 {
        MarketDataLifecyclePhase::LoadingHistory
    } else {
        MarketDataLifecyclePhase::LiveSubscribing
    };

    BrokerMarketDataLifecycleSnapshot {
        phase,
        blockers,
        bars_subscription_enabled: input.bars_subscription_enabled,
        quotes_subscription_enabled: input.quotes_subscription_enabled,
        transport_connected: input.transport_connected,
        strategy_source_kind: input.strategy_source_kind,
        rest_bars_used_for_strategy: input.rest_bars_used_for_strategy,
        rest_market_data_used_for_strategy: input.rest_market_data_used_for_strategy,
        history_bar_count: input.history_bar_count,
        recovery_bar_count: input.recovery_bar_count,
        live_bar_count: input.live_bar_count,
        live_final_bar_count: input.live_final_bar_count,
        live_forming_bar_count: input.live_forming_bar_count,
        quote_count: input.quote_count,
        first_live_bar_seen: input.first_live_bar_seen,
        first_live_final_bar_seen: input.first_live_final_bar_seen,
        first_live_final_bar_close_ts: input.first_live_final_bar_close_ts,
        last_history_bar_close_ts: input.last_history_bar_close_ts,
        last_recovery_bar_close_ts: input.last_recovery_bar_close_ts,
        last_live_bar_close_ts: input.last_live_bar_close_ts,
        last_final_live_bar_close_ts: input.last_final_live_bar_close_ts,
        live_bar_stale_for_sec,
        stale_after_sec: input.stale_after_sec,
        checked_ts: input.checked_ts,
    }
}

fn stale_for_sec(
    last_live_bar_close_ts: Option<DateTime<Utc>>,
    stale_after_sec: Option<u64>,
    checked_ts: DateTime<Utc>,
) -> Option<u64> {
    let stale_after_sec = stale_after_sec?;
    let last_live_bar_close_ts = last_live_bar_close_ts?;
    let age_sec = checked_ts
        .signed_duration_since(last_live_bar_close_ts)
        .num_seconds();
    if age_sec <= stale_after_sec as i64 {
        return None;
    }
    Some((age_sec - stale_after_sec as i64) as u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn base_input() -> BrokerMarketDataLifecycleInput {
        let now = Utc.with_ymd_and_hms(2026, 7, 5, 10, 1, 0).unwrap();
        BrokerMarketDataLifecycleInput {
            bars_subscription_enabled: true,
            quotes_subscription_enabled: false,
            transport_connected: true,
            strategy_source_kind: MarketDataSourceKind::LiveStream,
            rest_bars_used_for_strategy: false,
            rest_market_data_used_for_strategy: false,
            history_bar_count: 0,
            recovery_bar_count: 0,
            live_bar_count: 0,
            live_final_bar_count: 0,
            live_forming_bar_count: 0,
            quote_count: 0,
            first_live_bar_seen: false,
            first_live_final_bar_seen: false,
            first_live_final_bar_close_ts: None,
            last_history_bar_close_ts: None,
            last_recovery_bar_close_ts: None,
            last_live_bar_close_ts: None,
            last_final_live_bar_close_ts: None,
            stale_after_sec: Some(180),
            checked_ts: now,
        }
    }

    #[test]
    fn lifecycle_waits_for_live_stream_final_bar() {
        let snapshot = evaluate_market_data_lifecycle(base_input());

        assert_eq!(snapshot.phase, MarketDataLifecyclePhase::LiveSubscribing);
        assert!(snapshot
            .blockers
            .contains(&MarketDataLifecycleBlocker::NoLiveBarsObserved));
        assert!(snapshot
            .blockers
            .contains(&MarketDataLifecycleBlocker::NoFinalLiveBarsObserved));
    }

    #[test]
    fn lifecycle_distinguishes_history_and_gap_from_live_ready() {
        let mut history = base_input();
        history.history_bar_count = 10;
        let history_snapshot = evaluate_market_data_lifecycle(history);
        assert_eq!(
            history_snapshot.phase,
            MarketDataLifecyclePhase::LoadingHistory
        );

        let mut gap = base_input();
        gap.recovery_bar_count = 2;
        let gap_snapshot = evaluate_market_data_lifecycle(gap);
        assert_eq!(gap_snapshot.phase, MarketDataLifecyclePhase::SyncingGap);
    }

    #[test]
    fn lifecycle_accepts_fresh_final_live_stream_bar() {
        let mut input = base_input();
        let close_ts = Utc.with_ymd_and_hms(2026, 7, 5, 10, 0, 0).unwrap();
        input.live_bar_count = 1;
        input.live_final_bar_count = 1;
        input.first_live_bar_seen = true;
        input.first_live_final_bar_seen = true;
        input.first_live_final_bar_close_ts = Some(close_ts);
        input.last_live_bar_close_ts = Some(close_ts);
        input.last_final_live_bar_close_ts = Some(close_ts);

        let snapshot = evaluate_market_data_lifecycle(input);

        assert_eq!(snapshot.phase, MarketDataLifecyclePhase::LiveReady);
        assert!(snapshot.blockers.is_empty());
    }

    #[test]
    fn lifecycle_degrades_stale_live_data_and_rest_strategy_source() {
        let mut input = base_input();
        input.live_bar_count = 1;
        input.live_final_bar_count = 1;
        input.first_live_bar_seen = true;
        input.first_live_final_bar_seen = true;
        input.last_live_bar_close_ts = Some(Utc.with_ymd_and_hms(2026, 7, 5, 9, 50, 0).unwrap());
        input.last_final_live_bar_close_ts = input.last_live_bar_close_ts;
        input.rest_bars_used_for_strategy = true;

        let snapshot = evaluate_market_data_lifecycle(input);

        assert_eq!(snapshot.phase, MarketDataLifecyclePhase::Degraded);
        assert!(snapshot
            .blockers
            .contains(&MarketDataLifecycleBlocker::MarketDataStale));
        assert!(snapshot
            .blockers
            .contains(&MarketDataLifecycleBlocker::RestDataNotStrategySource));
    }
}
