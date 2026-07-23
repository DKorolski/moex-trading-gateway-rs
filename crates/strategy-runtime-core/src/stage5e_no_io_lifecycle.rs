//! Stage 5E-b1.1's private, observation-only live-bar-after-history boundary.
//!
//! This module intentionally has no public API. Its linear capability proves a
//! contextually valid `Live` bar after canonical history; it does not claim
//! market-gap continuity or any callback-ready authorization.

use chrono::{DateTime, Utc};

use crate::hybrid_intraday_runtime::HybridIntradayRuntimeStrategy;
use crate::stage5c_paper_host::{Stage5cPendingRecoveryReceipt, Stage5eNoIoBridgeSeal};

pub(crate) struct Stage5eNoIoLiveBarAfterHistoryInputs {
    strategy: HybridIntradayRuntimeStrategy,
    recovery_receipt: Stage5cPendingRecoveryReceipt,
    bar: broker_core::HybridRuntimeBarEvent,
    tick_size: f64,
    target_instrument: broker_core::InstrumentId,
    admission_tick_size: f64,
    admission_expires_at: DateTime<Utc>,
    lifecycle_now: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Stage5eContextualAdmissionError {
    NotLive,
    NotStrictlyAfterHistory,
    InstrumentMismatch,
    TickSizeMismatch,
    AdmissionExpired,
    FutureBar,
}

#[allow(clippy::too_many_arguments)] // Pure validation keeps the independent context bindings explicit.
fn validate_contextual_live_bar_after_history(
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

impl Stage5eNoIoLiveBarAfterHistoryInputs {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn from_stage5c_context(
        _seal: Stage5eNoIoBridgeSeal,
        strategy: HybridIntradayRuntimeStrategy,
        recovery_receipt: Stage5cPendingRecoveryReceipt,
        bar: broker_core::HybridRuntimeBarEvent,
        tick_size: f64,
        target_instrument: broker_core::InstrumentId,
        admission_tick_size: f64,
        admission_expires_at: DateTime<Utc>,
        lifecycle_now: DateTime<Utc>,
    ) -> Self {
        Self {
            strategy,
            recovery_receipt,
            bar,
            tick_size,
            target_instrument,
            admission_tick_size,
            admission_expires_at,
            lifecycle_now,
        }
    }
}

pub(crate) struct Stage5eObservedLiveBarAfterHistory {
    strategy: HybridIntradayRuntimeStrategy,
    recovery_receipt: Stage5cPendingRecoveryReceipt,
    bar: broker_core::HybridRuntimeBarEvent,
    tick_size: f64,
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

pub(crate) enum Stage5eNoIoLiveBarAfterHistoryBlocked {
    Contextual {
        reason: Stage5eContextualAdmissionError,
        inputs: Box<Stage5eNoIoLiveBarAfterHistoryInputs>,
    },
}

impl Stage5eNoIoLiveBarAfterHistoryBlocked {
    pub(crate) fn into_inputs(self) -> Stage5eNoIoLiveBarAfterHistoryInputs {
        match self {
            Self::Contextual { inputs, .. } => *inputs,
        }
    }
}

pub(crate) fn admit_stage5e_observation_only_live_bar_after_history(
    inputs: Stage5eNoIoLiveBarAfterHistoryInputs,
) -> Result<Stage5eObservedLiveBarAfterHistory, Stage5eNoIoLiveBarAfterHistoryBlocked> {
    if let Err(reason) = validate_contextual_live_bar_after_history(
        inputs.bar.origin,
        &inputs.bar.instrument,
        &inputs.target_instrument,
        inputs.tick_size,
        inputs.admission_tick_size,
        inputs.recovery_receipt.warmup_receipt().last_history_ts(),
        inputs.bar.close_time_utc,
        inputs.admission_expires_at,
        inputs.lifecycle_now,
    ) {
        return Err(Stage5eNoIoLiveBarAfterHistoryBlocked::Contextual {
            reason,
            inputs: Box::new(inputs),
        });
    }
    Ok(Stage5eObservedLiveBarAfterHistory {
        strategy: inputs.strategy,
        recovery_receipt: inputs.recovery_receipt,
        bar: inputs.bar,
        tick_size: inputs.tick_size,
    })
}

#[cfg(test)]
mod tests {
    use super::{validate_contextual_live_bar_after_history, Stage5eContextualAdmissionError};
    use broker_core::{Exchange, InstrumentId, Market};
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
}
