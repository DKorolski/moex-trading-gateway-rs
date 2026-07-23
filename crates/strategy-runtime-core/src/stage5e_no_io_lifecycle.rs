//! Stage 5E-b1's private, observation-only first-live-bar boundary.
//!
//! This module intentionally has no public API. It owns a linear capability
//! only after Stage 5C recovery and semantic-bar validation have completed.
//! Admitting the first fresh live bar does not invoke a strategy callback or
//! construct an intent batch.

use crate::hybrid_intraday_runtime::HybridIntradayRuntimeStrategy;
use crate::stage5c_paper_host::Stage5cPendingRecoveryReceipt;

pub(crate) struct Stage5eNoIoFirstLiveInputs {
    strategy: HybridIntradayRuntimeStrategy,
    recovery_receipt: Stage5cPendingRecoveryReceipt,
    bar: broker_core::HybridRuntimeBarEvent,
    tick_size: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stage5eFirstLiveEvidenceError {
    NotLive,
    NotStrictlyAfterHistory,
}

fn validate_first_live_evidence(
    origin: broker_core::HybridRuntimeBarOrigin,
    last_history_bar_close: i64,
    first_fresh_live_bar_close: i64,
) -> Result<(), Stage5eFirstLiveEvidenceError> {
    if origin != broker_core::HybridRuntimeBarOrigin::Live {
        return Err(Stage5eFirstLiveEvidenceError::NotLive);
    }
    if first_fresh_live_bar_close <= last_history_bar_close {
        return Err(Stage5eFirstLiveEvidenceError::NotStrictlyAfterHistory);
    }
    Ok(())
}

impl Stage5eNoIoFirstLiveInputs {
    pub(crate) fn from_stage5c_parts(
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

pub(crate) struct Stage5eObservedFirstFreshLiveBar {
    strategy: HybridIntradayRuntimeStrategy,
    recovery_receipt: Stage5cPendingRecoveryReceipt,
    bar: broker_core::HybridRuntimeBarEvent,
    tick_size: f64,
}

impl Stage5eObservedFirstFreshLiveBar {
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

pub(crate) enum Stage5eNoIoFirstLiveBlocked {
    NotLive {
        inputs: Box<Stage5eNoIoFirstLiveInputs>,
    },
    NotStrictlyAfterHistory {
        inputs: Box<Stage5eNoIoFirstLiveInputs>,
    },
}

impl Stage5eNoIoFirstLiveBlocked {
    pub(crate) fn into_inputs(self) -> Stage5eNoIoFirstLiveInputs {
        match self {
            Self::NotLive { inputs } | Self::NotStrictlyAfterHistory { inputs } => *inputs,
        }
    }
}

pub(crate) fn admit_stage5e_observation_only_first_live_bar(
    inputs: Stage5eNoIoFirstLiveInputs,
) -> Result<Stage5eObservedFirstFreshLiveBar, Stage5eNoIoFirstLiveBlocked> {
    match validate_first_live_evidence(
        inputs.bar.origin,
        inputs.recovery_receipt.warmup_receipt().last_history_ts(),
        inputs.bar.close_time_utc,
    ) {
        Err(Stage5eFirstLiveEvidenceError::NotLive) => {
            return Err(Stage5eNoIoFirstLiveBlocked::NotLive {
                inputs: Box::new(inputs),
            });
        }
        Err(Stage5eFirstLiveEvidenceError::NotStrictlyAfterHistory) => {
            return Err(Stage5eNoIoFirstLiveBlocked::NotStrictlyAfterHistory {
                inputs: Box::new(inputs),
            });
        }
        Ok(()) => {}
    }
    Ok(Stage5eObservedFirstFreshLiveBar {
        strategy: inputs.strategy,
        recovery_receipt: inputs.recovery_receipt,
        bar: inputs.bar,
        tick_size: inputs.tick_size,
    })
}

#[cfg(test)]
mod tests {
    use super::{validate_first_live_evidence, Stage5eFirstLiveEvidenceError};
    use broker_core::HybridRuntimeBarOrigin;

    #[test]
    fn first_live_evidence_requires_live_origin_and_strict_market_freshness() {
        assert_eq!(
            validate_first_live_evidence(HybridRuntimeBarOrigin::Replay, 1_000, 1_600),
            Err(Stage5eFirstLiveEvidenceError::NotLive)
        );
        assert_eq!(
            validate_first_live_evidence(HybridRuntimeBarOrigin::Live, 1_000, 1_000),
            Err(Stage5eFirstLiveEvidenceError::NotStrictlyAfterHistory)
        );
        assert!(validate_first_live_evidence(HybridRuntimeBarOrigin::Live, 1_000, 1_600).is_ok());
    }
}
