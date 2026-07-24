//! Stage 5E-b1.1's private, observation-only live-bar-after-history boundary.
//!
//! This module intentionally has no public API. Its linear capability proves a
//! contextually valid `Live` bar after canonical history; it does not claim
//! market-gap continuity or any callback-ready authorization.

use chrono::{DateTime, Utc};

use crate::hybrid_intraday_runtime::HybridIntradayRuntimeStrategy;
use crate::stage5c_paper_host::{Stage5cPendingRecoveryReceipt, Stage5eNoIoBridgeSeal};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Stage5eContextualAdmissionError {
    NotLive,
    NotStrictlyAfterHistory,
    InstrumentMismatch,
    TickSizeMismatch,
    AdmissionExpired,
    FutureBar,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Stage5eSessionObservationError {
    ScheduleNotOpen,
    ScheduleNotFresh,
    InvalidObservedWindow,
    BarOutsideObservedOpenWindow,
}

// STAGE5E-NO-IO-VALIDATOR-BEGIN: contextual-admission-v1
#[allow(clippy::too_many_arguments)] // Pure validation keeps the independent context bindings explicit.
pub(crate) fn validate_contextual_live_bar_after_history(
    origin: broker_core::HybridRuntimeBarOrigin,
    bar_instrument: &broker_core::InstrumentId,
    target_instrument: &broker_core::InstrumentId,
    bar_tick_size: f64,
    admission_tick_size: f64,
    last_history_bar_close: i64,
    bar_close: i64,
    admission_expires_at: DateTime<Utc>,
    lifecycle_now: DateTime<Utc>,
) -> Result<(), Stage5eContextualAdmissionError> {
    if origin != broker_core::HybridRuntimeBarOrigin::Live {
        return Err(Stage5eContextualAdmissionError::NotLive);
    }
    if bar_instrument != target_instrument {
        return Err(Stage5eContextualAdmissionError::InstrumentMismatch);
    }
    // Both values are issued by the canonical Stage 5C admission/data contract;
    // exact IEEE-754 bits intentionally reject independently recomputed values.
    if bar_tick_size.to_bits() != admission_tick_size.to_bits() {
        return Err(Stage5eContextualAdmissionError::TickSizeMismatch);
    }
    if lifecycle_now > admission_expires_at {
        return Err(Stage5eContextualAdmissionError::AdmissionExpired);
    }
    if bar_close > lifecycle_now.timestamp() {
        return Err(Stage5eContextualAdmissionError::FutureBar);
    }
    if bar_close <= last_history_bar_close {
        return Err(Stage5eContextualAdmissionError::NotStrictlyAfterHistory);
    }
    Ok(())
}
// STAGE5E-NO-IO-VALIDATOR-END: contextual-admission-v1

pub(crate) struct Stage5eObservedLiveBarAfterHistory {
    strategy: HybridIntradayRuntimeStrategy,
    recovery_receipt: Stage5cPendingRecoveryReceipt,
    bar: broker_core::HybridRuntimeBarEvent,
    tick_size: f64,
}

// STAGE5E-NO-IO-CAPABILITY-PROOF-BEGIN: zero-side-effects-v1
impl Stage5eObservedLiveBarAfterHistory {
    pub(crate) fn from_stage5c_context(
        _seal: Stage5eNoIoBridgeSeal,
        strategy: HybridIntradayRuntimeStrategy,
        recovery_receipt: Stage5cPendingRecoveryReceipt,
        bar: broker_core::HybridRuntimeBarEvent,
        tick_size: f64,
    ) -> Self {
        Self {
            strategy,
            recovery_receipt,
            bar,
            tick_size,
        }
    }
}

impl Stage5eObservedLiveBarAfterHistory {
    pub(crate) fn bar_close_ts(&self) -> i64 {
        self.bar.close_time_utc
    }

    pub(crate) fn callback_count(&self) -> usize {
        0
    }

    pub(crate) fn intent_count(&self) -> usize {
        0
    }

    pub(crate) fn strategy_was_called(&self) -> bool {
        false
    }

    pub(crate) fn executable_intent_created(&self) -> bool {
        false
    }
}
// STAGE5E-NO-IO-CAPABILITY-PROOF-END: zero-side-effects-v1

// STAGE5E-NO-IO-SESSION-ELIGIBILITY-BEGIN: observed-open-session-v1
/// A linear, fresh observation that the candidate bar belongs to an explicitly
/// observed open session. This is not a calendar implementation: its interval
/// is supplied by a later schedule mapper and is rejected when stale, unknown,
/// or not open. `until` is the last allowed bar-close, so both bounds are
/// intentionally inclusive.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Stage5eObservedOpenSession {
    bar_close_ts: i64,
}

pub(crate) fn observe_open_session_for_bar(
    session_state: broker_core::BrokerMarketSessionState,
    schedule_freshness: broker_core::Stage4BrokerTruthFreshnessProbe,
    observed_open_from_bar_close: i64,
    observed_open_until_bar_close: i64,
    bar_close_ts: i64,
    lifecycle_now: DateTime<Utc>,
) -> Result<Stage5eObservedOpenSession, Stage5eSessionObservationError> {
    if session_state != broker_core::BrokerMarketSessionState::Open {
        return Err(Stage5eSessionObservationError::ScheduleNotOpen);
    }
    let Some(observed_ts) = schedule_freshness.observed_ts else {
        return Err(Stage5eSessionObservationError::ScheduleNotFresh);
    };
    let schedule_age_ms = lifecycle_now
        .signed_duration_since(observed_ts)
        .num_milliseconds();
    if !schedule_freshness.available
        || schedule_age_ms < 0
        || schedule_age_ms as u64 > schedule_freshness.max_age_ms
    {
        return Err(Stage5eSessionObservationError::ScheduleNotFresh);
    }
    if observed_open_from_bar_close >= observed_open_until_bar_close {
        return Err(Stage5eSessionObservationError::InvalidObservedWindow);
    }
    if bar_close_ts < observed_open_from_bar_close || bar_close_ts > observed_open_until_bar_close {
        return Err(Stage5eSessionObservationError::BarOutsideObservedOpenWindow);
    }
    Ok(Stage5eObservedOpenSession { bar_close_ts })
}

impl Stage5eObservedOpenSession {
    pub(crate) fn bar_close_ts(&self) -> i64 {
        self.bar_close_ts
    }

    pub(crate) fn callback_count(&self) -> usize {
        0
    }

    pub(crate) fn intent_count(&self) -> usize {
        0
    }

    pub(crate) fn strategy_was_called(&self) -> bool {
        false
    }

    pub(crate) fn executable_intent_created(&self) -> bool {
        false
    }
}
// STAGE5E-NO-IO-SESSION-ELIGIBILITY-END: observed-open-session-v1

#[cfg(test)]
mod tests {
    use super::{
        observe_open_session_for_bar, validate_contextual_live_bar_after_history,
        Stage5eContextualAdmissionError, Stage5eSessionObservationError,
    };
    use broker_core::{
        BrokerMarketSessionState, Exchange, InstrumentId, Market, Stage4BrokerTruthFreshnessProbe,
    };
    use chrono::{TimeZone, Utc};

    fn instrument(symbol: &str) -> InstrumentId {
        InstrumentId {
            symbol: symbol.to_string(),
            venue_symbol: Some(format!("{symbol}@RTSX")),
            exchange: Exchange::Moex,
            market: Market::Other("test".to_string()),
        }
    }

    fn valid() -> Result<(), Stage5eContextualAdmissionError> {
        validate_contextual_live_bar_after_history(
            broker_core::HybridRuntimeBarOrigin::Live,
            &instrument("IMOEXF"),
            &instrument("IMOEXF"),
            0.5,
            0.5,
            1_000,
            1_600,
            Utc.timestamp_opt(2_000, 0).single().unwrap(),
            Utc.timestamp_opt(1_700, 0).single().unwrap(),
        )
    }

    #[test]
    fn contextual_admission_requires_live_exact_binding_and_same_domain_freshness() {
        assert_eq!(
            validate_contextual_live_bar_after_history(
                broker_core::HybridRuntimeBarOrigin::Replay,
                &instrument("IMOEXF"),
                &instrument("IMOEXF"),
                0.5,
                0.5,
                1_000,
                1_600,
                Utc.timestamp_opt(2_000, 0).single().unwrap(),
                Utc.timestamp_opt(1_700, 0).single().unwrap(),
            ),
            Err(Stage5eContextualAdmissionError::NotLive)
        );
        assert_eq!(
            validate_contextual_live_bar_after_history(
                broker_core::HybridRuntimeBarOrigin::Live,
                &instrument("RI"),
                &instrument("IMOEXF"),
                0.5,
                0.5,
                1_000,
                1_600,
                Utc.timestamp_opt(2_000, 0).single().unwrap(),
                Utc.timestamp_opt(1_700, 0).single().unwrap(),
            ),
            Err(Stage5eContextualAdmissionError::InstrumentMismatch)
        );
        assert_eq!(
            validate_contextual_live_bar_after_history(
                broker_core::HybridRuntimeBarOrigin::Live,
                &instrument("IMOEXF"),
                &instrument("IMOEXF"),
                0.1,
                0.5,
                1_000,
                1_600,
                Utc.timestamp_opt(2_000, 0).single().unwrap(),
                Utc.timestamp_opt(1_700, 0).single().unwrap(),
            ),
            Err(Stage5eContextualAdmissionError::TickSizeMismatch)
        );
        assert_eq!(
            validate_contextual_live_bar_after_history(
                broker_core::HybridRuntimeBarOrigin::Live,
                &instrument("IMOEXF"),
                &instrument("IMOEXF"),
                0.5,
                0.5,
                1_000,
                1_000,
                Utc.timestamp_opt(2_000, 0).single().unwrap(),
                Utc.timestamp_opt(1_700, 0).single().unwrap(),
            ),
            Err(Stage5eContextualAdmissionError::NotStrictlyAfterHistory)
        );
        assert!(valid().is_ok());
    }

    #[test]
    fn contextual_admission_rejects_expired_admission_and_future_market_bar() {
        assert_eq!(
            validate_contextual_live_bar_after_history(
                broker_core::HybridRuntimeBarOrigin::Live,
                &instrument("IMOEXF"),
                &instrument("IMOEXF"),
                0.5,
                0.5,
                1_000,
                1_600,
                Utc.timestamp_opt(1_600, 0).single().unwrap(),
                Utc.timestamp_opt(1_700, 0).single().unwrap(),
            ),
            Err(Stage5eContextualAdmissionError::AdmissionExpired)
        );
        assert_eq!(
            validate_contextual_live_bar_after_history(
                broker_core::HybridRuntimeBarOrigin::Live,
                &instrument("IMOEXF"),
                &instrument("IMOEXF"),
                0.5,
                0.5,
                1_000,
                1_800,
                Utc.timestamp_opt(2_000, 0).single().unwrap(),
                Utc.timestamp_opt(1_700, 0).single().unwrap(),
            ),
            Err(Stage5eContextualAdmissionError::FutureBar)
        );
    }

    fn fresh_schedule() -> Stage4BrokerTruthFreshnessProbe {
        Stage4BrokerTruthFreshnessProbe::fresh(
            Utc.timestamp_opt(1_700, 0).single().unwrap(),
            1_000_000,
            true,
        )
    }

    #[test]
    fn open_session_observation_requires_fresh_explicit_open_window() {
        let observed = observe_open_session_for_bar(
            BrokerMarketSessionState::Open,
            fresh_schedule(),
            1_500,
            1_800,
            1_600,
            Utc.timestamp_opt(1_900, 0).single().unwrap(),
        )
        .expect("fresh open session accepts contained bar");
        assert_eq!(observed.bar_close_ts(), 1_600);
        assert_eq!(observed.callback_count(), 0);
        assert_eq!(observed.intent_count(), 0);
        assert!(!observed.strategy_was_called());
        assert!(!observed.executable_intent_created());
    }

    #[test]
    fn session_observation_blocks_non_open_stale_and_out_of_window_evidence() {
        let now = Utc.timestamp_opt(1_900, 0).single().unwrap();
        assert_eq!(
            observe_open_session_for_bar(
                BrokerMarketSessionState::Break,
                fresh_schedule(),
                1_500,
                1_800,
                1_600,
                now,
            ),
            Err(Stage5eSessionObservationError::ScheduleNotOpen)
        );
        assert_eq!(
            observe_open_session_for_bar(
                BrokerMarketSessionState::Unknown,
                Stage4BrokerTruthFreshnessProbe::unknown(1_000, true),
                1_500,
                1_800,
                1_600,
                now,
            ),
            Err(Stage5eSessionObservationError::ScheduleNotOpen)
        );
        assert_eq!(
            observe_open_session_for_bar(
                BrokerMarketSessionState::Open,
                Stage4BrokerTruthFreshnessProbe::fresh(
                    Utc.timestamp_opt(1, 0).single().unwrap(),
                    1_000,
                    true,
                ),
                1_500,
                1_800,
                1_600,
                now,
            ),
            Err(Stage5eSessionObservationError::ScheduleNotFresh)
        );
        assert_eq!(
            observe_open_session_for_bar(
                BrokerMarketSessionState::Open,
                fresh_schedule(),
                1_800,
                1_500,
                1_600,
                now,
            ),
            Err(Stage5eSessionObservationError::InvalidObservedWindow)
        );
        assert_eq!(
            observe_open_session_for_bar(
                BrokerMarketSessionState::Open,
                fresh_schedule(),
                1_500,
                1_800,
                1_801,
                now,
            ),
            Err(Stage5eSessionObservationError::BarOutsideObservedOpenWindow)
        );
    }

    #[test]
    fn session_observation_has_explicit_inclusive_window_and_all_non_open_states_block() {
        let now = Utc.timestamp_opt(1_900, 0).single().unwrap();
        for state in [
            BrokerMarketSessionState::Break,
            BrokerMarketSessionState::Maintenance,
            BrokerMarketSessionState::Closed,
            BrokerMarketSessionState::Unknown,
        ] {
            assert_eq!(
                observe_open_session_for_bar(state, fresh_schedule(), 1_500, 1_800, 1_600, now),
                Err(Stage5eSessionObservationError::ScheduleNotOpen)
            );
        }
        for bar_close in [1_500, 1_800] {
            assert!(observe_open_session_for_bar(
                BrokerMarketSessionState::Open,
                fresh_schedule(),
                1_500,
                1_800,
                bar_close,
                now,
            )
            .is_ok());
        }
        assert_eq!(
            observe_open_session_for_bar(
                BrokerMarketSessionState::Open,
                Stage4BrokerTruthFreshnessProbe::fresh(
                    Utc.timestamp_opt(1_899, 999).single().unwrap(),
                    0,
                    true,
                ),
                1_500,
                1_800,
                1_600,
                now,
            ),
            Err(Stage5eSessionObservationError::ScheduleNotFresh)
        );
    }
}
